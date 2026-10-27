//! svg2tgs-core — конвертация SVG в Telegram TGS.
//!
//! TGS — это gzip-сжатый Lottie JSON. Telegram показывает векторные стикеры
//! и эмодзи только в этом формате, поэтому статичный SVG оборачивается
//! в Lottie-анимацию заданной длительности.
//!
//! Конвейер:
//! SVG → нормализация (usvg: `<use>`, CSS, единицы, текст в кривые)
//! → обход дерева и перевод в Lottie → JSON → gzip.

mod clip;
mod convert;
mod geometry;
mod lottie;
mod options;
mod placement;
mod report;
pub mod telegram;

use std::io::Write;
use std::sync::{Arc, OnceLock};

pub use options::{Background, Crop, Fit, Optimization, Options, Outline, Preset, MAX_OUTLINE};
pub use placement::{place, Placement};

/// Фон, как его рисовать: заливка и рамка — скруглённые квадраты на весь канвас.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackgroundPlan {
    pub fill: Option<[u8; 3]>,
    /// Цвет и толщина рамки; рамка лежит внутри канваса.
    pub border: Option<([u8; 3], f64)>,
    /// Радиус скругления внешнего края, не больше половины канваса.
    pub radius: f64,
}
pub use report::{Conversion, Severity, Warning, WarningCode};
/// usvg той же версии, что использует ядро, — для рендера эталона (resvg).
pub use usvg;

/// Версия ядра.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Ошибки, при которых результат получить нельзя.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Входные данные — не SVG или SVG повреждён.
    #[error("не удалось разобрать SVG: {0}")]
    InvalidSvg(String),
    /// В SVG нет ничего, что можно нарисовать.
    #[error("в SVG нет видимых элементов")]
    Empty,
    /// Некорректный параметр конвертации.
    #[error("некорректный параметр: {0}")]
    InvalidOption(String),
}

/// Системные шрифты загружаются один раз за время работы программы:
/// они нужны, чтобы перевести `<text>` в кривые.
fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        map_generic_families(&mut db);
        Arc::new(db)
    })
    .clone()
}

/// Предпочтения для общих семейств CSS (`sans-serif` и т. д.) — первое
/// установленное. По умолчанию fontdb ждёт Arial и Times New Roman, которых
/// нет в большинстве Linux: без этой привязки текст молча пропадал.
const SANS: &[&str] = &[
    "Arial",
    "Helvetica",
    "Segoe UI",
    "Liberation Sans",
    "DejaVu Sans",
    "Noto Sans",
    "Roboto",
    "Ubuntu",
    "Cantarell",
    "FreeSans",
];
const SERIF: &[&str] = &[
    "Times New Roman",
    "Times",
    "Liberation Serif",
    "DejaVu Serif",
    "Noto Serif",
    "FreeSerif",
    "Georgia",
];
const MONO: &[&str] = &[
    "Courier New",
    "Menlo",
    "Consolas",
    "Liberation Mono",
    "DejaVu Sans Mono",
    "Noto Sans Mono",
    "FreeMono",
];
const CURSIVE: &[&str] = &["Comic Sans MS", "Apple Chancery"];
const FANTASY: &[&str] = &["Impact", "Papyrus"];

fn map_generic_families(db: &mut usvg::fontdb::Database) {
    let installed: Vec<String> = db
        .faces()
        .flat_map(|f| f.families.iter().map(|(name, _)| name.clone()))
        .collect();
    let pick = |prefs: &[&str]| {
        prefs
            .iter()
            .find(|p| installed.iter().any(|n| n.eq_ignore_ascii_case(p)))
            .map(|p| p.to_string())
    };
    // Если нет ни одного из привычных шрифтов, подойдёт любой установленный.
    let Some(sans) = pick(SANS).or_else(|| installed.first().cloned()) else {
        return;
    };
    db.set_serif_family(pick(SERIF).unwrap_or_else(|| sans.clone()));
    db.set_monospace_family(pick(MONO).unwrap_or_else(|| sans.clone()));
    db.set_cursive_family(pick(CURSIVE).unwrap_or_else(|| sans.clone()));
    db.set_fantasy_family(pick(FANTASY).unwrap_or_else(|| sans.clone()));
    db.set_sans_serif_family(sans);
}

/// Разбирает SVG в нормализованное дерево usvg с системными шрифтами.
pub fn parse_svg(svg: &[u8]) -> Result<usvg::Tree, Error> {
    parse_with(svg, fontdb()).map(|(tree, _)| tree)
}

/// Разбор с заданной базой шрифтов. Второе значение — был ли текст, для
/// которого не нашлось ни одного шрифта: usvg такой текст выбрасывает из
/// дерева, и без этого флага он пропадал бы без замечания.
fn parse_with(
    svg: &[u8],
    fontdb: Arc<usvg::fontdb::Database>,
) -> Result<(usvg::Tree, bool), Error> {
    let text_lost = std::sync::atomic::AtomicBool::new(false);
    let default_select = usvg::FontResolver::default_font_selector();
    let usvg_options = usvg::Options {
        fontdb,
        font_resolver: usvg::FontResolver {
            select_font: Box::new(|font, db| {
                let found = default_select(font, db);
                if found.is_none() {
                    text_lost.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                found
            }),
            select_fallback: usvg::FontResolver::default_fallback_selector(),
        },
        ..usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_data(svg, &usvg_options).map_err(|e| Error::InvalidSvg(e.to_string()))?;
    Ok((tree, text_lost.load(std::sync::atomic::Ordering::Relaxed)))
}

/// Ступени автоподгонки под лимит размера: (знаков после запятой,
/// допуск упрощения в пикселях канваса 512). Первая ступень визуально
/// неотличима от оригинала; каждая следующая используется, только если
/// файл всё ещё больше лимита.
const AUTO_LADDER: [(u8, f64); 7] = [
    (2, 0.05),
    (2, 0.1),
    (1, 0.25),
    (1, 0.5),
    (1, 1.0),
    (0, 1.5),
    (0, 2.5),
];

/// Конвертирует SVG (содержимое файла) в TGS.
pub fn convert(svg: &[u8], options: &Options) -> Result<Conversion, Error> {
    let background = background_plan(options)?;
    let outline = match &options.outline {
        Some(o) if options.outline_width() > 0.0 => Some(parse_color(&o.color, "обводки")?),
        _ => None,
    };
    let (tree, text_lost) = parse_with(svg, fontdb())?;
    let limit = options.preset.max_bytes();

    let ladder: Vec<(u8, f64)> = match options.optimization {
        Optimization::Auto => AUTO_LADDER.to_vec(),
        Optimization::Lossless => vec![(3, 0.0)],
        Optimization::Manual(n) => vec![(n.min(6), 0.05)],
    };

    let mut result = None;
    for (step, &(decimals, tolerance)) in ladder.iter().enumerate() {
        let built = build(&tree, options, background, outline, decimals, tolerance)?;
        let fits = built.tgs.len() <= limit && built.max_points <= telegram::MAX_POINTS_PER_SHAPE;
        result = Some((step, tolerance, built));
        if fits {
            break;
        }
    }
    let (step, tolerance, built) = result.expect("в лестнице есть хотя бы одна ступень");

    let mut warnings = telegram::check_options(options);
    warnings.extend(built.warnings);
    if text_lost && !warnings.iter().any(|w| w.code == WarningCode::TextDropped) {
        warnings.push(Warning {
            code: WarningCode::TextDropped,
            element_id: None,
        });
    }
    if step > 0 {
        warnings.push(Warning {
            code: WarningCode::Simplified {
                tolerance: tolerance as f32,
            },
            element_id: None,
        });
    }
    if options.preset == Preset::Emoji {
        if let Some(w) = built
            .thinnest_stroke
            .filter(|&w| w < telegram::EMOJI_MIN_STROKE)
        {
            warnings.push(Warning {
                code: WarningCode::ThinLinesForEmoji {
                    width: (w * 10.0).round() as f32 / 10.0,
                    recommended: telegram::EMOJI_MIN_STROKE as f32,
                },
                element_id: None,
            });
        }
    }
    if built.max_points > telegram::MAX_POINTS_PER_SHAPE {
        warnings.push(Warning {
            code: WarningCode::TooComplex {
                points: built.max_points,
                limit: telegram::MAX_POINTS_PER_SHAPE,
            },
            element_id: None,
        });
    }
    if built.tgs.len() > limit {
        warnings.push(Warning {
            code: WarningCode::SizeOverLimit {
                bytes: built.tgs.len(),
                limit,
            },
            element_id: None,
        });
    }

    Ok(Conversion {
        tgs: built.tgs,
        json_bytes: built.json_bytes,
        warnings,
    })
}

struct Built {
    tgs: Vec<u8>,
    json_bytes: usize,
    warnings: Vec<Warning>,
    thinnest_stroke: Option<f64>,
    /// Точек в самой сложной группе (см. `telegram::MAX_POINTS_PER_SHAPE`).
    max_points: usize,
}

/// Один проход конвертации с заданной точностью.
fn build(
    tree: &usvg::Tree,
    options: &Options,
    background: BackgroundPlan,
    outline: Option<[u8; 3]>,
    decimals: u8,
    tolerance: f64,
) -> Result<Built, Error> {
    let canvas = options.effective_canvas_size();
    let placement = place(tree.size(), canvas as f32, options);

    let round = lottie::Rounder::with_tolerance(decimals, tolerance);
    let mut conv = convert::Converter::new(round);
    if outline.is_some() {
        conv.silhouette = Some(Vec::new());
    }
    conv.run(
        tree.root(),
        placement.transform,
        placement.clip,
        canvas as f64,
    );
    if conv.is_empty() {
        return Err(Error::Empty);
    }

    let frames = options.frame_count();
    let mut layers = Vec::new();
    // В Lottie первый слой — верхний, в SVG последний элемент рисуется поверх.
    for draft in conv.layers.into_iter().rev() {
        let mut items = draft.items;
        items.reverse();
        let mut layer = lottie::Layer::new(0, frames, items);
        layer
            .ks
            .set_opacity(lottie::Rounder::new(3).num(draft.opacity * 100.0));
        layers.push(layer);
    }
    // Обводка — под рисунком, над фоном; за канвас не выходит.
    if let (Some(color), Some(silhouette)) = (outline, conv.silhouette.take()) {
        let width = options.outline_width() as f64;
        let c = canvas as f64;
        let area = clip::intersect(
            &clip::outline(&silhouette, width),
            &clip::rect([0.0, 0.0, c, c]),
        );
        if !area.is_empty() {
            let beziers = clip::to_beziers(&area, &round);
            layers.push(fill_layer(
                "outline",
                beziers,
                color,
                lottie::FILL_RULE_NONZERO,
                frames,
            ));
        }
    }
    layers.extend(background_layers(&background, canvas, frames));
    for (n, layer) in layers.iter_mut().enumerate() {
        layer.ind = n as u32 + 1;
    }

    let animation = lottie::Animation {
        v: "5.7.4",
        fr: lottie::Num(options.effective_fps() as f64),
        ip: lottie::Num(0.0),
        op: lottie::Num(frames as f64),
        w: canvas,
        h: canvas,
        nm: "svg2tgs",
        ddd: 0,
        assets: [],
        layers,
        tgs: 1,
    };
    let max_points = animation
        .layers
        .iter()
        .flat_map(|l| &l.shapes)
        .map(lottie::Shape::rlottie_points)
        .max()
        .unwrap_or(0);
    let json =
        serde_json::to_vec(&animation).expect("сериализация Lottie не может завершиться ошибкой");
    Ok(Built {
        max_points,
        tgs: gzip(&json),
        json_bytes: json.len(),
        warnings: conv.warnings,
        thinnest_stroke: conv.thinnest_stroke,
    })
}

/// Цвет фона из параметров: `None` — прозрачный.
pub fn parse_background(bg: &Background) -> Result<Option<[u8; 3]>, Error> {
    match bg {
        Background::Transparent => Ok(None),
        Background::Solid(s) => parse_color(s, "фона").map(Some),
    }
}

/// Цвет `#rrggbb`; `what` — чей цвет, для сообщения об ошибке.
fn parse_color(s: &str, what: &str) -> Result<[u8; 3], Error> {
    let hex = s.trim().trim_start_matches('#');
    let bad = || Error::InvalidOption(format!("цвет {what} «{s}»"));
    if hex.len() != 6 || !hex.is_ascii() {
        return Err(bad());
    }
    let ch = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
    Ok([ch(0)?, ch(2)?, ch(4)?])
}

/// Параметры фона из настроек: цвета проверены, радиус и толщина ограничены.
pub fn background_plan(options: &Options) -> Result<BackgroundPlan, Error> {
    let half = options.effective_canvas_size() as f64 / 2.0;
    let border = match &options.background_border {
        Some(b) if b.width > 0.0 => Some((
            parse_color(&b.color, "рамки фона")?,
            (b.width.min(MAX_OUTLINE) as f64).min(half),
        )),
        _ => None,
    };
    Ok(BackgroundPlan {
        fill: parse_background(&options.background)?,
        border,
        radius: (options.background_radius.max(0.0) as f64).min(half),
    })
}

/// Квадрат `[x0, y0]–[x1, y1]` со скруглёнными углами радиуса `r`,
/// по часовой стрелке (в осях канваса). Дуги — кубические кривые.
fn rounded_rect(x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> lottie::Bezier {
    use lottie::{Bezier, Num};
    let r = r.max(0.0).min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let pt = |x: f64, y: f64| [Num(x), Num(y)];
    if r <= 0.0 {
        return Bezier {
            c: true,
            v: vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)],
            i: vec![pt(0.0, 0.0); 4],
            o: vec![pt(0.0, 0.0); 4],
        };
    }
    // 0.5523 — длина касательных, при которой кубическая кривая ближе всего к четверти окружности.
    let k = 0.552_284_75 * r;
    let z = pt(0.0, 0.0);
    Bezier {
        c: true,
        v: vec![
            pt(x0 + r, y0),
            pt(x1 - r, y0),
            pt(x1, y0 + r),
            pt(x1, y1 - r),
            pt(x1 - r, y1),
            pt(x0 + r, y1),
            pt(x0, y1 - r),
            pt(x0, y0 + r),
        ],
        i: vec![pt(-k, 0.0), z, pt(0.0, -k), z, pt(k, 0.0), z, pt(0.0, k), z],
        o: vec![z, pt(k, 0.0), z, pt(0.0, k), z, pt(-k, 0.0), z, pt(0.0, -k)],
    }
}

/// Слои фона, сверху вниз: рамка, заливка.
fn background_layers(plan: &BackgroundPlan, canvas: u32, frames: u32) -> Vec<lottie::Layer> {
    let c = canvas as f64;
    let mut out = Vec::new();
    if let Some((rgb, w)) = plan.border {
        // Кольцо: внешний край — край фона, внутренний — отступ w внутрь
        // с радиусом r − w; правило evenodd вырезает середину.
        let outer = rounded_rect(0.0, 0.0, c, c, plan.radius);
        let inner = rounded_rect(w, w, c - w, c - w, plan.radius - w);
        out.push(fill_layer(
            "background-border",
            vec![outer, inner],
            rgb,
            lottie::FILL_RULE_EVENODD,
            frames,
        ));
    }
    if let Some(rgb) = plan.fill {
        let rect = rounded_rect(0.0, 0.0, c, c, plan.radius);
        out.push(fill_layer(
            "background",
            vec![rect],
            rgb,
            lottie::FILL_RULE_NONZERO,
            frames,
        ));
    }
    out
}

/// Слой из одной группы: кривые, залитые сплошным цветом.
fn fill_layer(
    name: &str,
    beziers: Vec<lottie::Bezier>,
    rgb: [u8; 3],
    rule: u8,
    frames: u32,
) -> lottie::Layer {
    use lottie::{GroupTransform, Num, Shape, Static};
    let ch = |v: u8| lottie::Rounder::new(3).num(v as f64 / 255.0);
    let mut it: Vec<Shape> = beziers
        .into_iter()
        .map(|b| Shape::Path { ks: Static::new(b) })
        .collect();
    it.push(Shape::Fill {
        c: Static::new([ch(rgb[0]), ch(rgb[1]), ch(rgb[2]), Num(1.0)]),
        o: Static::new(Num(100.0)),
        r: rule,
    });
    it.push(Shape::Transform(GroupTransform::default()));
    let group = Shape::Group {
        nm: Some(name.into()),
        it,
    };
    lottie::Layer::new(0, frames, vec![group])
}

/// gzip с нулевым временем в заголовке: одинаковый вход — одинаковый файл.
fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    enc.write_all(data).expect("запись в память");
    enc.finish().expect("запись в память")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><text x="10" y="30" font-family="sans-serif">OK</text></svg>"#;

    #[test]
    fn text_without_any_font_is_reported() {
        let (_, lost) = parse_with(TEXT_SVG, Arc::new(usvg::fontdb::Database::new())).unwrap();
        assert!(lost, "без шрифтов текст пропадает — это должно быть видно");
    }

    #[test]
    fn generic_family_falls_back_to_installed_font() {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        if db.is_empty() {
            return; // в системе нет шрифтов — проверять нечего
        }
        map_generic_families(&mut db);
        let (tree, lost) = parse_with(TEXT_SVG, Arc::new(db)).unwrap();
        assert!(!lost);
        assert!(tree.root().has_children(), "текст должен остаться в дереве");
    }

    #[test]
    fn default_options_follow_sticker_preset() {
        let o = Options::default();
        assert_eq!(o.preset, Preset::Sticker);
        assert_eq!(o.effective_canvas_size(), 512);
        assert_eq!(o.effective_fps(), 60);
        assert_eq!(o.frame_count(), 60);
    }

    #[test]
    fn overrides_take_priority_over_preset() {
        let o = Options {
            fps: Some(30),
            duration_secs: Some(2.0),
            ..Options::default()
        };
        assert_eq!(o.frame_count(), 60);
    }

    #[test]
    fn options_roundtrip_through_json() {
        let o = Options {
            preset: Preset::Emoji,
            background: Background::Solid("#ffffff".into()),
            optimization: Optimization::Manual(2),
            ..Options::default()
        };
        let json = serde_json::to_string(&o).unwrap();
        let back: Options = serde_json::from_str(&json).unwrap();
        assert_eq!(o, back);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let o: Options = serde_json::from_str(r#"{"preset":"emoji"}"#).unwrap();
        assert_eq!(o.preset, Preset::Emoji);
        assert_eq!(o.fit, Fit::Contain);
    }

    #[test]
    fn warning_serializes_with_flat_code() {
        let w = Warning {
            code: WarningCode::FilterRemoved {
                filter: "blur".into(),
            },
            element_id: Some("shadow".into()),
        };
        let v = serde_json::to_value(&w).unwrap();
        assert_eq!(v["code"], "filter_removed");
        assert_eq!(v["filter"], "blur");
        assert_eq!(v["element_id"], "shadow");
        assert_eq!(w.code.severity(), Severity::Warning);
    }
}

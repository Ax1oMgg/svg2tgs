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
mod report;
pub mod telegram;

use std::io::Write;
use std::sync::{Arc, OnceLock};

use usvg::Transform;

pub use options::{Background, Fit, Optimization, Options, Preset};
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
    let background = parse_background(&options.background)?;
    let (tree, text_lost) = parse_with(svg, fontdb())?;
    let limit = options.preset.max_bytes();

    let ladder: Vec<(u8, f64)> = match options.optimization {
        Optimization::Auto => AUTO_LADDER.to_vec(),
        Optimization::Lossless => vec![(3, 0.0)],
        Optimization::Manual(n) => vec![(n.min(6), 0.05)],
    };

    let mut result = None;
    for (step, &(decimals, tolerance)) in ladder.iter().enumerate() {
        let built = build(&tree, options, background, decimals, tolerance)?;
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
    background: Option<[u8; 3]>,
    decimals: u8,
    tolerance: f64,
) -> Result<Built, Error> {
    let canvas = options.effective_canvas_size();
    let fit = fit_transform(tree.size(), canvas as f32, options);

    let mut conv = convert::Converter::new(lottie::Rounder::with_tolerance(decimals, tolerance));
    conv.run(tree.root(), fit, canvas as f64);
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
    if let Some(color) = background {
        layers.push(background_layer(color, canvas, frames));
    }
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

/// Матрица, вписывающая рисунок размера `size` в квадрат `canvas`
/// с учётом отступа и способа вписывания; рисунок центрируется.
pub fn fit_transform(size: usvg::Size, canvas: f32, options: &Options) -> Transform {
    let (w, h) = (size.width(), size.height());
    let padding = options.padding.clamp(0.0, 0.45);
    let area = canvas * (1.0 - 2.0 * padding);
    let scale = match options.fit {
        Fit::Contain => (area / w).min(area / h),
        Fit::Cover => (area / w).max(area / h),
    };
    let tx = (canvas - w * scale) / 2.0;
    let ty = (canvas - h * scale) / 2.0;
    Transform::from_row(scale, 0.0, 0.0, scale, tx, ty)
}

/// Цвет фона из параметров: `None` — прозрачный.
pub fn parse_background(bg: &Background) -> Result<Option<[u8; 3]>, Error> {
    match bg {
        Background::Transparent => Ok(None),
        Background::Solid(s) => {
            let hex = s.trim().trim_start_matches('#');
            let bad = || Error::InvalidOption(format!("цвет фона «{s}»"));
            if hex.len() != 6 || !hex.is_ascii() {
                return Err(bad());
            }
            let ch = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
            Ok(Some([ch(0)?, ch(2)?, ch(4)?]))
        }
    }
}

fn background_layer(rgb: [u8; 3], canvas: u32, frames: u32) -> lottie::Layer {
    use lottie::{Bezier, GroupTransform, Num, Shape, Static};
    let c = canvas as f64;
    let rect = Bezier {
        c: true,
        v: [[0.0, 0.0], [c, 0.0], [c, c], [0.0, c]]
            .map(|[x, y]| [Num(x), Num(y)])
            .to_vec(),
        i: vec![[Num(0.0); 2]; 4],
        o: vec![[Num(0.0); 2]; 4],
    };
    let ch = |v: u8| lottie::Rounder::new(3).num(v as f64 / 255.0);
    let group = Shape::Group {
        nm: Some("background".into()),
        it: vec![
            Shape::Path {
                ks: Static::new(rect),
            },
            Shape::Fill {
                c: Static::new([ch(rgb[0]), ch(rgb[1]), ch(rgb[2]), Num(1.0)]),
                o: Static::new(Num(100.0)),
                r: lottie::FILL_RULE_NONZERO,
            },
            Shape::Transform(GroupTransform::default()),
        ],
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

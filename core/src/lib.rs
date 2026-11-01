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
pub mod naming;
mod options;
mod placement;
mod report;
pub mod telegram;

use std::io::Write;
use std::sync::{Arc, OnceLock};

pub use options::{
    Background, Bounds, Crop, Fit, Grid, Optimization, Options, OptionsError, Outline,
    ParsedOptions, Preset, MAX_OUTLINE, OPTIONS_FORMAT,
};
pub use placement::{
    board_size, place, place_tree, shaped_aspect, source_rect, tree_aspect, Placement,
};

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

/// Шрифты, которыми ядро переводит `<text>` в кривые.
struct Fonts {
    db: Arc<usvg::fontdb::Database>,
    /// Шрифты встроены сборкой (веб-версия): любой текст рисуется ими,
    /// поэтому замена общих семейств (`sans-serif` и т. п.) тоже замечание.
    embedded: bool,
}

/// Шрифты вместо системных, если их задала сборка ([`use_embedded_fonts`]).
static EMBEDDED_FONTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
static FONTS: OnceLock<Fonts> = OnceLock::new();

/// Шрифты для сборок без доступа к системе (веб-версия): текст переводится
/// в кривые только ими, системные не ищутся. Общие семейства (`sans-serif`
/// и т. д.) ведут к первому из них. Вызывать до первой конвертации;
/// `false` — шрифты уже загружены и замена не действует.
pub fn use_embedded_fonts(fonts: Vec<Vec<u8>>) -> bool {
    if FONTS.get().is_some() {
        return false;
    }
    EMBEDDED_FONTS.set(fonts).is_ok()
}

/// Шрифты загружаются один раз за время работы программы: системные
/// или встроенные сборкой.
fn fonts() -> &'static Fonts {
    FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        let embedded = match EMBEDDED_FONTS.get() {
            Some(list) => {
                for data in list {
                    db.load_font_data(data.clone());
                }
                true
            }
            None => {
                db.load_system_fonts();
                false
            }
        };
        map_generic_families(&mut db);
        Fonts {
            db: Arc::new(db),
            embedded,
        }
    })
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
    parse_with(svg, fonts()).map(|(tree, _)| tree)
}

/// Что случилось с текстом при разборе: usvg молча выбрасывает текст без
/// шрифта и знаки без начертания — без этих сведений потеря была бы не видна.
#[derive(Debug, Default, Clone, PartialEq)]
struct TextNotes {
    /// Был текст, для которого не нашлось ни одного шрифта.
    lost: bool,
    /// Замены шрифта: (что просил SVG, чем нарисовано), без повторов.
    replaced: Vec<(String, String)>,
    /// Знаки, которых нет ни в одном шрифте, без повторов. usvg спрашивает
    /// замену для первого такого знака в строке текста — остальные знаки
    /// той строки сюда могут не попасть.
    missing: Vec<char>,
}

/// Сколько недостающих знаков перечислять в замечании.
const MAX_MISSING_CHARS: usize = 8;

impl TextNotes {
    fn warnings(&self) -> Vec<Warning> {
        let mut out = Vec::new();
        if self.lost {
            out.push(Warning {
                code: WarningCode::TextDropped,
                element_id: None,
            });
        }
        for (family, used) in &self.replaced {
            out.push(Warning {
                code: WarningCode::FontReplaced {
                    family: family.clone(),
                    used: used.clone(),
                },
                element_id: None,
            });
        }
        if !self.missing.is_empty() {
            out.push(Warning {
                code: WarningCode::GlyphsMissing {
                    chars: self.missing.iter().take(MAX_MISSING_CHARS).collect(),
                },
                element_id: None,
            });
        }
        out
    }
}

/// Название семейства, как его пишут в CSS.
fn family_name(f: &usvg::FontFamily) -> String {
    match f {
        usvg::FontFamily::Named(name) => name.clone(),
        other => other.to_string(),
    }
}

/// Разбор с заданными шрифтами; второе значение — что случилось с текстом.
fn parse_with(svg: &[u8], fonts: &Fonts) -> Result<(usvg::Tree, TextNotes), Error> {
    let notes = std::sync::Mutex::new(TextNotes::default());
    let embedded = fonts.embedded;
    let default_select = usvg::FontResolver::default_font_selector();
    let default_fallback = usvg::FontResolver::default_fallback_selector();
    let usvg_options = usvg::Options {
        fontdb: fonts.db.clone(),
        font_resolver: usvg::FontResolver {
            select_font: Box::new(|font, db| {
                let found = default_select(font, db);
                let mut notes = notes.lock().unwrap_or_else(|e| e.into_inner());
                let Some(id) = found else {
                    notes.lost = true;
                    return None;
                };
                // Замена — если нарисовано не тем семейством, что названо в
                // SVG. Общие семейства в системе ведут к привычным шрифтам
                // и заменой не считаются; во встроенных шрифтах — считаются.
                let used = db
                    .face(id)
                    .and_then(|f| f.families.first())
                    .map(|(n, _)| n.clone())
                    .unwrap_or_default();
                let named: Vec<&String> = font
                    .families()
                    .iter()
                    .filter_map(|f| match f {
                        usvg::FontFamily::Named(n) => Some(n),
                        _ => None,
                    })
                    .collect();
                let matches_named = named.iter().any(|n| n.eq_ignore_ascii_case(&used));
                if !matches_named && (embedded || !named.is_empty()) {
                    let family = font.families().first().map(family_name).unwrap_or_default();
                    let pair = (family, used);
                    if !notes.replaced.contains(&pair) {
                        notes.replaced.push(pair);
                    }
                }
                found
            }),
            select_fallback: Box::new(|c, exclude, db| {
                let found = default_fallback(c, exclude, db);
                if found.is_none() && !c.is_whitespace() && !c.is_control() {
                    let mut notes = notes.lock().unwrap_or_else(|e| e.into_inner());
                    if !notes.missing.contains(&c) {
                        notes.missing.push(c);
                    }
                }
                found
            }),
        },
        ..usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_data(svg, &usvg_options).map_err(|e| Error::InvalidSvg(e.to_string()))?;
    drop(usvg_options);
    Ok((tree, notes.into_inner().unwrap_or_else(|e| e.into_inner())))
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

/// Сетка нарезки по пропорциям рисунка после трансформаций и обрезки,
/// не больше `max_cols` столбцов (см. [`Grid::auto`]).
pub fn auto_grid(tree: &usvg::Tree, options: &Options, max_cols: u32) -> Grid {
    Grid::auto(tree_aspect(tree, options), max_cols)
}

/// Итог нарезки: сетка (подобранная, если `grid_auto`) и файлы клеток
/// по строкам слева направо.
#[derive(Debug)]
pub struct Tiles {
    pub grid: Grid,
    pub tiles: Vec<Conversion>,
    /// Соотношение сторон рисунка после трансформаций и обрезки
    /// (ширина / высота) — для ползунка размера сетки.
    pub aspect: f32,
}

/// Конвертирует SVG (содержимое файла) в TGS. Нарезка (`Options::grid`)
/// здесь не действует — рисунок ложится на один канвас; для нарезки —
/// [`convert_grid`].
pub fn convert(svg: &[u8], options: &Options) -> Result<Conversion, Error> {
    let single = Options {
        grid: None,
        ..options.clone()
    };
    let mut out = convert_grid(svg, &single)?;
    Ok(out.tiles.remove(0))
}

/// Конвертирует SVG в набор эмодзи по сетке `Options::grid`: по файлу
/// на клетку, по строкам слева направо. Без сетки — один файл. Пустые
/// клетки тоже возвращаются (прозрачные), чтобы сетка не съехала.
pub fn convert_grid(svg: &[u8], options: &Options) -> Result<Tiles, Error> {
    convert_grid_limited(svg, options, options.preset.max_bytes())
}

/// Как [`convert_grid`], но с другим пределом размера файла: автоподгонка
/// упрощает рисунок, пока каждый файл не станет не больше `max_bytes`.
/// Нужно для обложки набора стикеров (Telegram принимает TGS до 32 КБ).
pub fn convert_grid_limited(
    svg: &[u8],
    options: &Options,
    max_bytes: usize,
) -> Result<Tiles, Error> {
    let outline_rgb = match &options.outline {
        Some(o) if options.outline_width() > 0.0 => Some(parse_color(&o.color, "обводки")?),
        _ => None,
    };
    let (tree, text_notes) = parse_with(svg, fonts())?;
    let mut options = options.clone();
    if options.grid.is_some() && options.grid_auto {
        options.grid = Some(auto_grid(&tree, &options, Grid::AUTO_COLS));
    }
    let options = &options;
    let aspect = tree_aspect(&tree, options);
    let board = Board::new(&tree, options, outline_rgb)?;

    let ladder: Vec<(u8, f64)> = match options.optimization {
        Optimization::Auto => AUTO_LADDER.to_vec(),
        Optimization::Lossless => vec![(3, 0.0)],
        Optimization::Manual(n) => vec![(n.min(6), 0.05)],
    };
    let limit = max_bytes;

    let mut out = Vec::new();
    let mut drawn = false;
    for row in 0..board.grid.rows {
        for col in 0..board.grid.cols {
            let mut result = None;
            for (step, &(decimals, tolerance)) in ladder.iter().enumerate() {
                let built = board.build(&tree, options, (col, row), decimals, tolerance);
                let fits =
                    built.tgs.len() <= limit && built.max_points <= telegram::MAX_POINTS_PER_SHAPE;
                result = Some((step, tolerance, built));
                if fits {
                    break;
                }
            }
            let (step, tolerance, built) = result.expect("в лестнице есть хотя бы одна ступень");
            drawn |= !built.empty;
            out.push(finish(built, options, step, tolerance, &text_notes, limit));
        }
    }
    if !drawn {
        return Err(Error::Empty);
    }
    Ok(Tiles {
        grid: board.grid,
        tiles: out,
        aspect,
    })
}

/// Замечания к готовому файлу.
fn finish(
    built: Built,
    options: &Options,
    step: usize,
    tolerance: f64,
    text_notes: &TextNotes,
    limit: usize,
) -> Conversion {
    let mut warnings = telegram::check_options(options);
    warnings.extend(built.warnings);
    for w in text_notes.warnings() {
        if !warnings.iter().any(|x| x.code == w.code) {
            warnings.push(w);
        }
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
    Conversion {
        tgs: built.tgs,
        json_bytes: built.json_bytes,
        warnings,
    }
}

struct Built {
    tgs: Vec<u8>,
    json_bytes: usize,
    warnings: Vec<Warning>,
    thinnest_stroke: Option<f64>,
    /// Точек в самой сложной группе (см. `telegram::MAX_POINTS_PER_SHAPE`).
    max_points: usize,
    /// В клетке ничего не нарисовано (ни рисунка, ни обводки, ни фона).
    empty: bool,
}

/// Холст: все канвасы сетки и то, что считается для него целиком —
/// размещение, видимая область, обводка вокруг всего рисунка, фон.
struct Board {
    grid: Grid,
    /// Сторона одного канваса.
    tile: f64,
    placement: Placement,
    background: BackgroundPlan,
    /// Рамка фона, по которой обрезается рисунок (координаты холста).
    frame: Option<clip::Contours>,
    /// Цвет обводки рисунка и (для сетки) её готовая область на холсте.
    outline: Option<([u8; 3], Option<clip::Contours>)>,
}

impl Board {
    fn new(
        tree: &usvg::Tree,
        options: &Options,
        outline_rgb: Option<[u8; 3]>,
    ) -> Result<Self, Error> {
        let grid = options.grid();
        let tile = options.effective_canvas_size();
        let (w, h) = placement::board_size(tile as f32, grid);
        let (w, h) = (w as f64, h as f64);
        let placement = place_tree(tree, tile as f32, grid, options);
        let background = background_plan_for(options, w, h)?;
        let frame = (options.background_clip
            && (background.fill.is_some() || background.border.is_some()))
        .then(|| {
            let inset = background.border.map_or(0.0, |(_, bw)| bw);
            clip::flatten(&[rounded_rect(
                inset,
                inset,
                w - inset,
                h - inset,
                background.radius - inset,
            )])
        });
        let mut board = Self {
            grid,
            tile: tile as f64,
            placement,
            background,
            frame,
            outline: outline_rgb.map(|rgb| (rgb, None)),
        };
        // Без нарезки обводка собирается в том же проходе, что и рисунок.
        // С нарезкой — заранее, по всему рисунку: иначе она легла бы
        // и вдоль разрезов между клетками.
        if let (Some(rgb), true) = (outline_rgb, grid.cols * grid.rows > 1) {
            let round = lottie::Rounder::with_tolerance(AUTO_LADDER[0].0, AUTO_LADDER[0].1);
            let mut conv = convert::Converter::new(round);
            conv.silhouette = Some(Vec::new());
            conv.run(tree.root(), placement.transform, board.area(None));
            let silhouette = conv.silhouette.take().unwrap_or_default();
            board.outline = Some((rgb, Some(board.outline_region(&silhouette, options))));
        }
        Ok(board)
    }

    fn size(&self) -> (f64, f64) {
        (
            self.tile * self.grid.cols as f64,
            self.tile * self.grid.rows as f64,
        )
    }

    /// Видимая область в координатах холста или (с `cell`) клетки.
    /// Край, совпадающий с краем холста, получает допуск 1 px: выход за край
    /// на доли пикселя (сглаживание, контрольные точки) не виден, а обрезка
    /// спрямляет кривые. Внутри холста край обрезки точный.
    fn area(&self, cell: Option<(u32, u32)>) -> clip::Area {
        let (w, h) = self.size();
        let slack = |v: f64, edge: f64, out: f64| if (v - edge).abs() < 0.5 { v + out } else { v };
        let c = self.placement.clip;
        let mut r = [
            slack(c[0], 0.0, -1.0),
            slack(c[1], 0.0, -1.0),
            slack(c[2], w, 1.0),
            slack(c[3], h, 1.0),
        ];
        let (dx, dy) = self.offset(cell);
        if let Some((col, row)) = cell {
            let t = self.tile;
            let x0 = col as f64 * t;
            let y0 = row as f64 * t;
            r = [
                r[0].max(if col == 0 { -1.0 } else { x0 }),
                r[1].max(if row == 0 { -1.0 } else { y0 }),
                r[2].min(if col + 1 == self.grid.cols {
                    w + 1.0
                } else {
                    x0 + t
                }),
                r[3].min(if row + 1 == self.grid.rows {
                    h + 1.0
                } else {
                    y0 + t
                }),
            ];
            r = [r[0] - dx, r[1] - dy, r[2] - dx, r[3] - dy];
            if r[2] <= r[0] || r[3] <= r[1] {
                return clip::Area::from_rect([0.0, 0.0, 0.0, 0.0]).intersect(&Vec::new());
            }
        }
        let area = clip::Area::from_rect(r);
        match &self.frame {
            Some(f) => area.intersect(&clip::translate(f, -dx, -dy)),
            None => area,
        }
    }

    /// Сдвиг клетки относительно холста.
    fn offset(&self, cell: Option<(u32, u32)>) -> (f64, f64) {
        cell.map_or((0.0, 0.0), |(c, r)| {
            (c as f64 * self.tile, r as f64 * self.tile)
        })
    }

    /// Обводка вокруг силуэта: не выходит за холст и за рамку фона.
    fn outline_region(&self, silhouette: &clip::Contours, options: &Options) -> clip::Contours {
        let (w, h) = self.size();
        let width = options.outline_width() as f64;
        let mut area = clip::intersect(
            &clip::outline(silhouette, width),
            &clip::rect([0.0, 0.0, w, h]),
        );
        if let Some(f) = &self.frame {
            area = clip::intersect(&area, f);
        }
        area
    }

    /// Один проход конвертации клетки `cell` с заданной точностью.
    fn build(
        &self,
        tree: &usvg::Tree,
        options: &Options,
        cell: (u32, u32),
        decimals: u8,
        tolerance: f64,
    ) -> Built {
        let single = self.grid.cols * self.grid.rows == 1;
        let canvas = self.tile;
        let (dx, dy) = self.offset(Some(cell));
        let fit = usvg::Transform::from_translate(-dx as f32, -dy as f32)
            .pre_concat(self.placement.transform);

        let round = lottie::Rounder::with_tolerance(decimals, tolerance);
        let mut conv = convert::Converter::new(round);
        if single && self.outline.is_some() {
            conv.silhouette = Some(Vec::new());
        }
        let area = if single {
            self.area(None)
        } else {
            self.area(Some(cell))
        };
        conv.run(tree.root(), fit, area);

        let frames = options.frame_count();
        let mut layers = Vec::new();
        // В Lottie первый слой — верхний, в SVG последний элемент рисуется поверх.
        for draft in conv.layers.into_iter().rev() {
            if draft.items.is_empty() {
                continue;
            }
            let mut items = draft.items;
            items.reverse();
            let mut layer = lottie::Layer::new(0, frames, items);
            layer
                .ks
                .set_opacity(lottie::Rounder::new(3).num(draft.opacity * 100.0));
            layers.push(layer);
        }
        // Обводка — под рисунком, над фоном; за холст и рамку фона не выходит.
        if let Some((color, ready)) = &self.outline {
            let region = match (ready, conv.silhouette.take()) {
                (Some(board), _) => {
                    let cellr = [dx, dy, dx + canvas, dy + canvas];
                    clip::translate(&clip::intersect(board, &clip::rect(cellr)), -dx, -dy)
                }
                (None, Some(silhouette)) => self.outline_region(&silhouette, options),
                (None, None) => Vec::new(),
            };
            if !region.is_empty() {
                let beziers = clip::to_beziers(&region, &round);
                layers.push(fill_layer(
                    "outline",
                    beziers,
                    *color,
                    lottie::FILL_RULE_NONZERO,
                    frames,
                ));
            }
        }
        if single {
            layers.extend(background_layers(&self.background, canvas, canvas, frames));
        } else {
            layers.extend(self.background_cell(cell, &round, frames));
        }
        let empty = layers.is_empty();
        if empty {
            layers.push(blank_layer(canvas, frames));
        }
        for (n, layer) in layers.iter_mut().enumerate() {
            layer.ind = n as u32 + 1;
        }

        let animation = lottie::Animation {
            v: "5.7.4",
            fr: lottie::Num(options.effective_fps() as f64),
            ip: lottie::Num(0.0),
            op: lottie::Num(frames as f64),
            w: canvas as u32,
            h: canvas as u32,
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
        let json = serde_json::to_vec(&animation)
            .expect("сериализация Lottie не может завершиться ошибкой");
        Built {
            max_points,
            tgs: gzip(&json),
            json_bytes: json.len(),
            warnings: conv.warnings,
            thinnest_stroke: conv.thinnest_stroke,
            empty,
        }
    }

    /// Фон в клетке сетки: области фона всего холста, обрезанные по клетке.
    fn background_cell(
        &self,
        (col, row): (u32, u32),
        round: &lottie::Rounder,
        frames: u32,
    ) -> Vec<lottie::Layer> {
        let (w, h) = self.size();
        let t = self.tile;
        let (dx, dy) = (col as f64 * t, row as f64 * t);
        let cell = clip::rect([dx, dy, dx + t, dy + t]);
        let plan = &self.background;
        let piece = |region: clip::Contours| {
            clip::to_beziers(
                &clip::translate(&clip::intersect(&region, &cell), -dx, -dy),
                round,
            )
        };
        let mut out = Vec::new();
        if let Some((rgb, bw)) = plan.border {
            let outer = rounded_rect(0.0, 0.0, w, h, plan.radius);
            let inner = rounded_rect(bw, bw, w - bw, h - bw, plan.radius - bw);
            let ring = piece(clip::region(&[outer, inner], lottie::FILL_RULE_EVENODD));
            if !ring.is_empty() {
                out.push(fill_layer(
                    "background-border",
                    ring,
                    rgb,
                    lottie::FILL_RULE_NONZERO,
                    frames,
                ));
            }
        }
        if let Some(rgb) = plan.fill {
            let rect = piece(clip::flatten(&[rounded_rect(0.0, 0.0, w, h, plan.radius)]));
            if !rect.is_empty() {
                out.push(fill_layer(
                    "background",
                    rect,
                    rgb,
                    lottie::FILL_RULE_NONZERO,
                    frames,
                ));
            }
        }
        out
    }
}

/// Слой пустой клетки: невидимая точка, чтобы в файле был хоть один слой.
fn blank_layer(canvas: f64, frames: u32) -> lottie::Layer {
    let c = canvas / 2.0;
    let mut layer = fill_layer(
        "blank",
        vec![rounded_rect(c, c, c + 1.0, c + 1.0, 0.0)],
        [0, 0, 0],
        lottie::FILL_RULE_NONZERO,
        frames,
    );
    layer.ks.set_opacity(lottie::Num(0.0));
    layer
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

/// Параметры фона из настроек для одного канваса: цвета проверены,
/// радиус и толщина ограничены.
pub fn background_plan(options: &Options) -> Result<BackgroundPlan, Error> {
    let c = options.effective_canvas_size() as f64;
    background_plan_for(options, c, c)
}

/// Параметры фона для холста `w × h` (сетка нарезки — фон на весь холст).
pub fn background_plan_for(options: &Options, w: f64, h: f64) -> Result<BackgroundPlan, Error> {
    let half = w.min(h) / 2.0;
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

/// Слои фона на канвас `w × h`, сверху вниз: рамка, заливка.
fn background_layers(plan: &BackgroundPlan, w: f64, h: f64, frames: u32) -> Vec<lottie::Layer> {
    let mut out = Vec::new();
    if let Some((rgb, bw)) = plan.border {
        // Кольцо: внешний край — край фона, внутренний — отступ w внутрь
        // с радиусом r − w; правило evenodd вырезает середину.
        let outer = rounded_rect(0.0, 0.0, w, h, plan.radius);
        let inner = rounded_rect(bw, bw, w - bw, h - bw, plan.radius - bw);
        out.push(fill_layer(
            "background-border",
            vec![outer, inner],
            rgb,
            lottie::FILL_RULE_EVENODD,
            frames,
        ));
    }
    if let Some(rgb) = plan.fill {
        let rect = rounded_rect(0.0, 0.0, w, h, plan.radius);
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

    /// Встроенный шрифт веб-версии — он же для проверок замен.
    const NOTO: &[u8] = include_bytes!("../../web/fonts/NotoSans-Regular.ttf");

    fn embedded_fonts() -> Fonts {
        let mut db = usvg::fontdb::Database::new();
        db.load_font_data(NOTO.to_vec());
        map_generic_families(&mut db);
        Fonts {
            db: Arc::new(db),
            embedded: true,
        }
    }

    fn text_svg(family: &str, text: &str) -> Vec<u8> {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><text x="10" y="30" font-family="{family}">{text}</text></svg>"#
        )
        .into_bytes()
    }

    #[test]
    fn text_without_any_font_is_reported() {
        let empty = Fonts {
            db: Arc::new(usvg::fontdb::Database::new()),
            embedded: false,
        };
        let (_, notes) = parse_with(TEXT_SVG, &empty).unwrap();
        assert!(
            notes.lost,
            "без шрифтов текст пропадает — это должно быть видно"
        );
    }

    #[test]
    fn generic_family_falls_back_to_installed_font() {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        if db.is_empty() {
            return; // в системе нет шрифтов — проверять нечего
        }
        map_generic_families(&mut db);
        let fonts = Fonts {
            db: Arc::new(db),
            embedded: false,
        };
        let (tree, notes) = parse_with(TEXT_SVG, &fonts).unwrap();
        assert!(!notes.lost);
        // Общее семейство в системе ведёт к привычному шрифту — не замена.
        assert!(notes.replaced.is_empty(), "{:?}", notes.replaced);
        assert!(tree.root().has_children(), "текст должен остаться в дереве");
    }

    #[test]
    fn embedded_font_reports_every_replacement() {
        let fonts = embedded_fonts();
        // Общее семейство во встроенных шрифтах — тоже замена.
        let (tree, notes) = parse_with(TEXT_SVG, &fonts).unwrap();
        assert!(tree.root().has_children());
        assert_eq!(
            notes.replaced,
            vec![("sans-serif".into(), "Noto Sans".into())]
        );
        // Названный шрифт, которого нет.
        let (_, notes) = parse_with(&text_svg("Comic Sans MS, cursive", "Hi"), &fonts).unwrap();
        assert_eq!(
            notes.replaced,
            vec![("Comic Sans MS".into(), "Noto Sans".into())]
        );
        // Сам встроенный шрифт — не замена.
        let (_, notes) = parse_with(&text_svg("Noto Sans", "Hi"), &fonts).unwrap();
        assert!(notes.replaced.is_empty());
    }

    #[test]
    fn missing_glyphs_are_listed_once() {
        let fonts = embedded_fonts();
        // Кириллица в шрифте есть, иероглифов — нет. usvg спрашивает замену
        // для первого недостающего знака строки — он и попадает в замечание.
        let (_, notes) = parse_with(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><text x="10" y="20" font-family="Noto Sans">Привет 你好</text><text x="10" y="40" font-family="Noto Sans">ok 你 ok</text></svg>"#
                .as_bytes(),
            &fonts,
        )
        .unwrap();
        assert_eq!(notes.missing, vec!['你']);
        assert!(!notes.lost);
        let w = notes.warnings();
        assert!(w.iter().any(|w| w.code
            == WarningCode::GlyphsMissing {
                chars: "你".into()
            }));
        // Без недостающих знаков замечания нет.
        let (_, notes) = parse_with(&text_svg("Noto Sans", "Ёлка Ωmega ß"), &fonts).unwrap();
        assert!(notes.missing.is_empty(), "{:?}", notes.missing);
    }

    #[test]
    fn font_notes_reach_the_conversion_warnings() {
        let fonts = embedded_fonts();
        let (_, notes) = parse_with(&text_svg("Arial", "OK"), &fonts).unwrap();
        let codes: Vec<String> = notes
            .warnings()
            .iter()
            .map(|w| {
                serde_json::to_value(w).unwrap()["code"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(codes, ["font_replaced"]);
        let json = serde_json::to_value(&notes.warnings()[0]).unwrap();
        assert_eq!(json["family"], "Arial");
        assert_eq!(json["used"], "Noto Sans");
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

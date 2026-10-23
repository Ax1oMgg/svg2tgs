//! svg2tgs-core — конвертация SVG в Telegram TGS.
//!
//! TGS — это gzip-сжатый Lottie JSON. Telegram показывает векторные стикеры
//! и эмодзи только в этом формате, поэтому статичный SVG оборачивается
//! в Lottie-анимацию заданной длительности.
//!
//! Конвейер:
//! SVG → нормализация (usvg: `<use>`, CSS, единицы, текст в кривые)
//! → обход дерева и перевод в Lottie → JSON → gzip.

mod convert;
mod geometry;
mod lottie;
mod options;
mod report;

use std::io::Write;
use std::sync::{Arc, OnceLock};

use usvg::Transform;

pub use options::{Background, Fit, Optimization, Options, Preset};
pub use report::{Conversion, Severity, Warning, WarningCode};

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
        Arc::new(db)
    })
    .clone()
}

/// Конвертирует SVG (содержимое файла) в TGS.
pub fn convert(svg: &[u8], options: &Options) -> Result<Conversion, Error> {
    let background = parse_background(&options.background)?;

    let usvg_options = usvg::Options {
        fontdb: fontdb(),
        ..usvg::Options::default()
    };
    let tree =
        usvg::Tree::from_data(svg, &usvg_options).map_err(|e| Error::InvalidSvg(e.to_string()))?;

    let canvas = options.effective_canvas_size();
    let fit = fit_transform(tree.size(), canvas as f32, options);

    let decimals = match options.optimization {
        Optimization::Auto => 2,
        Optimization::Lossless => 4,
        Optimization::Manual(n) => n,
    };
    let mut conv = convert::Converter::new(decimals);
    conv.run(tree.root(), fit);
    if conv.is_empty() {
        return Err(Error::Empty);
    }

    let frames = options.frame_count();
    let mut layers = Vec::new();
    // В Lottie первый слой — верхний, в SVG последний элемент рисуется поверх.
    for draft in conv.layers.into_iter().rev() {
        let mut items = draft.items;
        items.reverse();
        let mut layer = lottie::Layer::new(0, frames, items, draft.masks);
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
    let json =
        serde_json::to_vec(&animation).expect("сериализация Lottie не может завершиться ошибкой");

    let mut warnings = conv.warnings;
    let tgs = gzip(&json);
    let limit = options.preset.max_bytes();
    if tgs.len() > limit {
        warnings.push(Warning {
            code: WarningCode::SizeOverLimit {
                bytes: tgs.len(),
                limit,
            },
            element_id: None,
        });
    }

    Ok(Conversion {
        tgs,
        json_bytes: json.len(),
        warnings,
    })
}

/// Матрица, вписывающая рисунок размера `size` в квадрат `canvas`
/// с учётом отступа и способа вписывания; рисунок центрируется.
fn fit_transform(size: usvg::Size, canvas: f32, options: &Options) -> Transform {
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

fn parse_background(bg: &Background) -> Result<Option<[u8; 3]>, Error> {
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
    lottie::Layer::new(0, frames, vec![group], vec![])
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

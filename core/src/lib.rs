//! svg2tgs-core — конвертация SVG в Telegram TGS.
//!
//! TGS — это gzip-сжатый Lottie JSON. Telegram показывает векторные стикеры
//! и эмодзи только в этом формате, поэтому статичный SVG оборачивается
//! в Lottie-анимацию заданной длительности.
//!
//! Конвейер (реализуется на этапе 1):
//! SVG → нормализация (usvg) → дерево Lottie → оптимизация → gzip.

mod options;
mod report;

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
    /// Функция ещё не реализована (каркас этапа 0).
    #[error("конвертация ещё не реализована")]
    NotImplemented,
}

/// Конвертирует SVG (содержимое файла) в TGS.
pub fn convert(svg: &[u8], options: &Options) -> Result<Conversion, Error> {
    let _ = (svg, options);
    Err(Error::NotImplemented)
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

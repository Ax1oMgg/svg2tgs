//! Требования Telegram к анимированным стикерам и эмодзи (формат TGS).
//!
//! Источник: <https://core.telegram.org/stickers>, раздел про анимированные
//! стикеры; сверено 24.10.2026. Для анимированных эмодзи Telegram не даёт
//! отдельных цифр — действуют те же правила, что и для стикеров.
//!
//! - канвас 512×512;
//! - 60 кадров в секунду;
//! - длительность не больше 3 секунд;
//! - файл не больше 64 КБ;
//! - анимация зациклена (статичный рисунок зациклен сам собой);
//! - объекты не выходят за канвас (конвертер обрезает их, модуль `clip`);
//! - запрещены: маски, merge paths, звёзды, градиентные обводки, repeater,
//!   картинки, сплошные слои (solid), текст, 3D-слои, эффекты слоёв,
//!   выражения и т. п. Конвертер ничего из этого не выдаёт (проверяется
//!   тестом `output_uses_only_features_allowed_by_telegram`).

use crate::options::Options;
use crate::report::{Warning, WarningCode};

/// Сторона канваса, пиксели.
pub const CANVAS: u32 = 512;
/// Кадров в секунду.
pub const FPS: u32 = 60;
/// Максимальная длительность, секунды.
pub const MAX_DURATION_SECS: f32 = 3.0;
/// Максимальный размер файла .tgs, байты.
pub const MAX_BYTES: usize = 64 * 1024;

/// Замечания к параметрам, с которыми Telegram файл не примет.
/// Конвертация всё равно выполняется — решает пользователь.
pub fn check_options(options: &Options) -> Vec<Warning> {
    let mut out = Vec::new();
    let mut push = |code| {
        out.push(Warning {
            code,
            element_id: None,
        })
    };
    let canvas = options.effective_canvas_size();
    if canvas != CANVAS {
        push(WarningCode::CanvasNotAllowed {
            size: canvas,
            required: CANVAS,
        });
    }
    let fps = options.effective_fps();
    if fps != FPS {
        push(WarningCode::FpsNotAllowed { fps, required: FPS });
    }
    // Длительность считается по целому числу кадров, как в файле.
    let secs = options.frame_count() as f32 / fps.max(1) as f32;
    if secs > MAX_DURATION_SECS + 1e-3 {
        push(WarningCode::DurationTooLong {
            secs,
            max: MAX_DURATION_SECS,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Preset;

    #[test]
    fn presets_follow_telegram_requirements() {
        for preset in [Preset::Sticker, Preset::Emoji] {
            assert_eq!(preset.canvas_size(), 512);
            assert_eq!(preset.fps(), 60);
            assert!(preset.duration_secs() <= 3.0);
            assert_eq!(preset.max_bytes(), 64 * 1024);
            let o = Options {
                preset,
                ..Options::default()
            };
            assert!(check_options(&o).is_empty());
        }
    }

    #[test]
    fn wrong_overrides_are_reported() {
        let o = Options {
            canvas_size: Some(100),
            fps: Some(30),
            duration_secs: Some(5.0),
            ..Options::default()
        };
        let codes: Vec<_> = check_options(&o).into_iter().map(|w| w.code).collect();
        assert_eq!(codes.len(), 3);
        assert!(matches!(
            codes[0],
            WarningCode::CanvasNotAllowed { size: 100, .. }
        ));
        assert!(matches!(
            codes[1],
            WarningCode::FpsNotAllowed { fps: 30, .. }
        ));
        assert!(matches!(codes[2], WarningCode::DurationTooLong { .. }));
    }

    #[test]
    fn exactly_three_seconds_is_fine() {
        let o = Options {
            duration_secs: Some(3.0),
            ..Options::default()
        };
        assert!(check_options(&o).is_empty());
    }
}

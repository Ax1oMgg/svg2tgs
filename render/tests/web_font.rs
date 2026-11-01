//! Попиксельная проверка веб-версии: текст переводится в кривые только
//! встроенным шрифтом (`svg2tgs_core::use_embedded_fonts`), системные
//! не ищутся. Отдельный файл тестов — отдельный процесс: выбор шрифтов
//! действует на всё время работы программы.

use svg2tgs_core::{convert, use_embedded_fonts, Options, WarningCode};
use svg2tgs_render::{compare, render_svg, render_tgs};

const NOTO: &[u8] = include_bytes!("../../web/fonts/NotoSans-Regular.ttf");

const TEXT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
  <rect width="200" height="100" rx="16" fill="#3157d5"/>
  <text x="16" y="62" font-family="Georgia, serif" font-size="40" fill="#fff">Привет</text>
</svg>"##;

#[test]
fn embedded_font_draws_text_like_the_reference() {
    assert!(use_embedded_fonts(vec![NOTO.to_vec()]));
    let options = Options::default();
    let conv = convert(TEXT.as_bytes(), &options).unwrap();
    // Шрифт заменён — громко: замечание с названием и заменой.
    assert!(conv.warnings.iter().any(|w| w.code
        == WarningCode::FontReplaced {
            family: "Georgia".into(),
            used: "Noto Sans".into(),
        }));
    assert!(!conv
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::TextDropped));
    // Эталон (resvg) и результат (rlottie) — тем же встроенным шрифтом.
    let reference = render_svg(TEXT.as_bytes(), 256, &options).unwrap();
    let result = render_tgs(&conv.tgs, 256, 0).unwrap();
    let diff = compare(&reference, &result);
    assert!(
        diff.mean < 1.5 && diff.bad_ratio < 0.002,
        "разница {:.2}, заметно {:.3}%",
        diff.mean,
        diff.bad_ratio * 100.0
    );
    // Текст действительно нарисован: белые пиксели внутри синей плашки.
    let white = result
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200 && p[3] > 200)
        .count();
    assert!(white > 500, "белых пикселей текста: {white}");
    // После первой конвертации шрифты уже не поменять.
    assert!(!use_embedded_fonts(Vec::new()));
}

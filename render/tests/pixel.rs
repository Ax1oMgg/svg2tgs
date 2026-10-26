//! Попиксельная проверка: каждый тестовый SVG конвертируется в TGS,
//! результат рисуется rlottie (движком Telegram), эталон — resvg,
//! и картинки сравниваются.
//!
//! При провале рядом с целевой папкой появляется картинка различий
//! `target/pixel-diff/<файл>.png`: эталон | результат | карта различий.

use std::path::PathBuf;

use svg2tgs_core::{convert, Options};
use svg2tgs_render::{compare, diff_sheet, render_svg, render_tgs, Diff, RLOTTIE_VENDOR};

const SIZE: u32 = 256;

/// Допуски. Средняя разница ниже 1 на глаз не видна; заметно отличающихся
/// пикселей (обычно это сглаживание краёв) допускается 0,2 %.
const MAX_MEAN: f64 = 1.5;
const MAX_BAD: f64 = 0.002;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
}

fn diff_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/pixel-diff")
}

fn check(name: &str) -> Result<Diff, String> {
    let svg = std::fs::read(fixtures_dir().join(name)).unwrap();
    let options = Options::default();
    let conv = convert(&svg, &options).map_err(|e| format!("{name}: {e}"))?;
    let reference = render_svg(&svg, SIZE, &options).unwrap();
    let result = render_tgs(&conv.tgs, SIZE, 0).unwrap();
    let diff = compare(&reference, &result);

    // Размытие убирается намеренно: эталон с размытием, результат без.
    let max_mean = if name == "filter-blur.svg" {
        2.5
    } else {
        MAX_MEAN
    };
    if diff.mean > max_mean || diff.bad_ratio > MAX_BAD {
        std::fs::create_dir_all(diff_dir()).unwrap();
        let out = diff_dir().join(name.replace(".svg", ".png"));
        std::fs::write(&out, diff_sheet(&reference, &result).to_png()).unwrap();
        return Err(format!(
            "{name}: разница {:.2} (допуск {max_mean}), заметно {:.2}% (допуск {:.1}%), см. {}",
            diff.mean,
            diff.bad_ratio * 100.0,
            MAX_BAD * 100.0,
            out.display()
        ));
    }
    Ok(diff)
}

#[test]
fn every_fixture_looks_like_its_svg() {
    let mut names: Vec<String> = std::fs::read_dir(fixtures_dir())
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            n.ends_with(".svg").then_some(n)
        })
        .filter(|n| n != "invalid.svg" && n != "empty.svg")
        .collect();
    names.sort();
    assert!(names.len() >= 17);

    let mut failures = Vec::new();
    for name in &names {
        match check(name) {
            Ok(d) => eprintln!(
                "{name:28} разница {:.2}, заметно {:.3}%",
                d.mean,
                d.bad_ratio * 100.0
            ),
            Err(e) => failures.push(e),
        }
    }
    assert!(
        failures.is_empty(),
        "rlottie ({RLOTTIE_VENDOR}) рисует не так, как SVG:\n{}",
        failures.join("\n")
    );
}

/// Проверка самой проверки: разные рисунки должны считаться разными.
#[test]
fn comparison_detects_a_wrong_picture() {
    let options = Options::default();
    let a = std::fs::read(fixtures_dir().join("evenodd-ring.svg")).unwrap();
    let b = std::fs::read(fixtures_dir().join("use-symbol.svg")).unwrap();
    let reference = render_svg(&a, SIZE, &options).unwrap();
    let wrong = render_tgs(&convert(&b, &options).unwrap().tgs, SIZE, 0).unwrap();
    let d = compare(&reference, &wrong);
    assert!(d.mean > MAX_MEAN && d.bad_ratio > MAX_BAD, "{d:?}");
}

/// Мелкая деталь (точка диаметром ~2 % стороны) почти не влияет на среднюю
/// разницу, но должна ловиться долей заметно отличающихся пикселей.
#[test]
fn comparison_detects_a_missing_dot() {
    let options = Options::default();
    let with_dot = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <rect x="10" y="10" width="80" height="40" fill="#2f81f7"/><circle cx="50" cy="75" r="2.5"/></svg>"##;
    let without = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <rect x="10" y="10" width="80" height="40" fill="#2f81f7"/></svg>"##;
    let reference = render_svg(with_dot, SIZE, &options).unwrap();
    let result = render_tgs(&convert(without, &options).unwrap().tgs, SIZE, 0).unwrap();
    let d = compare(&reference, &result);
    assert!(d.bad_ratio > MAX_BAD, "пропавшая точка не замечена: {d:?}");
}

/// Обводка вокруг рисунка. Эталон строится вручную: силуэт с обводкой
/// SVG толщиной 2W и скруглёнными углами — это и есть область, расширенная
/// на W; дырка кольца заливается.
#[test]
fn outline_surrounds_the_silhouette() {
    use svg2tgs_core::Outline;
    let art = r##"<path d="M10 10H90V50H50V90H10Z" fill="#2f81f7"/>
        <circle cx="76" cy="76" r="14" fill="none" stroke="#e5534b" stroke-width="6"/>"##;
    let svg =
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{art}</svg>"#);
    let w = 16.0_f32;
    let options = Options {
        outline: Some(Outline {
            width: w,
            color: "#20c070".into(),
        }),
        ..Options::default()
    };
    // Рисунок 100×100 вписывается в 512 − 2W.
    let s = (512.0 - 2.0 * w) / 100.0;
    let reference = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
        <g transform="translate({w} {w}) scale({s})">
        <path d="M10 10H90V50H50V90H10Z M76 59a17 17 0 1 0 0.001 0Z" fill="#20c070"
            stroke="#20c070" stroke-width="{sw}" stroke-linejoin="round"/>
        {art}</g></svg>"##,
        sw = 2.0 * w / s
    );
    let conv = convert(svg.as_bytes(), &options).unwrap();
    let expected = render_svg(reference.as_bytes(), SIZE, &Options::default()).unwrap();
    let result = render_tgs(&conv.tgs, SIZE, 0).unwrap();
    let d = compare(&expected, &result);
    if d.mean > MAX_MEAN || d.bad_ratio > MAX_BAD {
        std::fs::create_dir_all(diff_dir()).unwrap();
        let out = diff_dir().join("outline.png");
        std::fs::write(&out, diff_sheet(&expected, &result).to_png()).unwrap();
        panic!("обводка не совпала: {d:?}, см. {}", out.display());
    }

    // Без обводки результат заметно другой — тест действительно её видит.
    let plain = render_tgs(
        &convert(svg.as_bytes(), &Options::default()).unwrap().tgs,
        SIZE,
        0,
    )
    .unwrap();
    let d = compare(&expected, &plain);
    assert!(d.bad_ratio > MAX_BAD, "{d:?}");
}

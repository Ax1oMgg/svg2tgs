//! Попиксельная проверка: каждый тестовый SVG конвертируется в TGS,
//! результат рисуется rlottie (движком Telegram), эталон — resvg,
//! и картинки сравниваются.
//!
//! При провале рядом с целевой папкой появляется картинка различий
//! `target/pixel-diff/<файл>.png`: эталон | результат | карта различий.

use std::path::PathBuf;

use svg2tgs_core::{convert, convert_grid, Options};
use svg2tgs_render::{
    compare, diff_sheet, render_svg, render_tgs, render_tgs_grid, Diff, RLOTTIE_VENDOR,
};

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

/// Фон со скруглением и рамкой. Эталон — SVG: квадрат с `rx` и рамка
/// обводкой SVG толщиной W по квадрату, утопленному на W/2 (её внешний
/// край совпадает с краем фона, внутренний радиус — r − W).
#[test]
fn rounded_background_with_border() {
    use svg2tgs_core::{Background, Outline};
    let art = r##"<circle cx="50" cy="50" r="30" fill="#2f81f7"/>"##;
    let svg =
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{art}</svg>"#);
    let (r, w) = (64.0_f32, 12.0_f32);
    let options = Options {
        background: Background::Solid("#fbe0e4".into()),
        background_radius: r,
        background_border: Some(Outline {
            width: w,
            color: "#20c070".into(),
        }),
        ..Options::default()
    };
    let reference = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
        <rect width="512" height="512" rx="{r}" fill="#fbe0e4"/>
        <rect x="{h}" y="{h}" width="{iw}" height="{iw}" rx="{ir}" fill="none" stroke="#20c070" stroke-width="{w}"/>
        <g transform="scale(5.12)">{art}</g></svg>"##,
        h = w / 2.0,
        iw = 512.0 - w,
        ir = r - w / 2.0,
    );
    let expected = render_svg(reference.as_bytes(), SIZE, &Options::default()).unwrap();
    let conv = convert(svg.as_bytes(), &options).unwrap();
    let result = render_tgs(&conv.tgs, SIZE, 0).unwrap();
    // Превью исходника в приложении рисует фон тем же способом.
    let preview = render_svg(svg.as_bytes(), SIZE, &options).unwrap();
    for (name, img) in [("tgs", &result), ("preview", &preview)] {
        let d = compare(&expected, img);
        if d.mean > MAX_MEAN || d.bad_ratio > MAX_BAD {
            std::fs::create_dir_all(diff_dir()).unwrap();
            let out = diff_dir().join(format!("background-{name}.png"));
            std::fs::write(&out, diff_sheet(&expected, img).to_png()).unwrap();
            panic!("фон ({name}) не совпал: {d:?}, см. {}", out.display());
        }
    }
}

/// Трансформация и размещение: отражение, поворот на произвольный угол,
/// пропорции, обрезка краёв, вписывание по ширине/высоте со сдвигом и отступ.
/// Эталон — resvg с той же матрицей и обрезкой по видимой области.
#[test]
fn transforms_and_placement_match_svg() {
    use svg2tgs_core::{Crop, Fit};
    let svg = std::fs::read(fixtures_dir().join("shapes-basic.svg")).unwrap();
    let cases = [
        (
            "flip-rotate",
            Options {
                flip_h: true,
                rotation: 30.0,
                ..Options::default()
            },
        ),
        (
            "aspect",
            Options {
                aspect: 1.6,
                ..Options::default()
            },
        ),
        (
            "crop-width-top",
            Options {
                crop: Some(Crop {
                    left: 0.1,
                    right: 0.2,
                    top: 0.05,
                    bottom: 0.0,
                }),
                fit: Fit::Width,
                position_y: 0.0,
                padding: 0.08,
                ..Options::default()
            },
        ),
        (
            "aspect-then-rotate-crop",
            Options {
                aspect: 1.5,
                rotation: 35.0,
                crop: Some(Crop {
                    left: 0.15,
                    right: 0.1,
                    top: -0.1,
                    bottom: 0.2,
                }),
                ..Options::default()
            },
        ),
        (
            "height-left",
            Options {
                fit: Fit::Height,
                rotation: -12.0,
                position_x: 0.0,
                ..Options::default()
            },
        ),
        (
            "free",
            Options {
                fit: Fit::Free,
                scale: 1.4,
                position_x: 0.8,
                position_y: 0.3,
                ..Options::default()
            },
        ),
    ];
    let mut failures = Vec::new();
    for (name, options) in cases {
        let conv = convert(&svg, &options).unwrap();
        let reference = render_svg(&svg, SIZE, &options).unwrap();
        let result = render_tgs(&conv.tgs, SIZE, 0).unwrap();
        let d = compare(&reference, &result);
        if d.mean > MAX_MEAN || d.bad_ratio > MAX_BAD {
            std::fs::create_dir_all(diff_dir()).unwrap();
            let out = diff_dir().join(format!("placement-{name}.png"));
            std::fs::write(&out, diff_sheet(&reference, &result).to_png()).unwrap();
            failures.push(format!("{name}: {d:?}, см. {}", out.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    // Проверка проверки: без трансформации картинка заметно другая.
    let rotated = Options {
        rotation: 30.0,
        ..Options::default()
    };
    let a = render_svg(&svg, SIZE, &rotated).unwrap();
    let b = render_tgs(&convert(&svg, &Options::default()).unwrap().tgs, SIZE, 0).unwrap();
    assert!(compare(&a, &b).bad_ratio > MAX_BAD);
}

/// «По краю»: рамка облегает видимый рисунок (с толщиной обводки), пустые
/// поля документа не учитываются. Эталон — тот же круг, нарисованный на весь
/// канвас вручную.
#[test]
fn content_bounds_hug_the_drawing() {
    use svg2tgs_core::Bounds;
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <circle cx="30" cy="40" r="20" fill="#2f81f7" stroke="#1e9e62" stroke-width="4"/></svg>"##;
    let options = Options {
        bounds: Bounds::Content,
        ..Options::default()
    };
    // Рамка 44×44 (радиус 20 + половина обводки 2) — на весь канвас 512.
    let k = 512.0 / 44.0;
    let reference = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
        <circle cx="256" cy="256" r="{r}" fill="#2f81f7" stroke="#1e9e62" stroke-width="{sw}"/></svg>"##,
        r = 20.0 * k,
        sw = 4.0 * k
    );
    let expected = render_svg(reference.as_bytes(), SIZE, &Options::default()).unwrap();
    let result = render_tgs(&convert(svg, &options).unwrap().tgs, SIZE, 0).unwrap();
    let d = compare(&expected, &result);
    if d.mean > MAX_MEAN || d.bad_ratio > MAX_BAD {
        std::fs::create_dir_all(diff_dir()).unwrap();
        let out = diff_dir().join("content-bounds.png");
        std::fs::write(&out, diff_sheet(&expected, &result).to_png()).unwrap();
        panic!("рамка по краю: {d:?}, см. {}", out.display());
    }
    // По контейнеру — круг мельче и не по центру: картинка заметно другая.
    let boxed = render_tgs(&convert(svg, &Options::default()).unwrap().tgs, SIZE, 0).unwrap();
    assert!(compare(&expected, &boxed).bad_ratio > MAX_BAD);
}

/// Сравнение с картинкой различий при провале.
fn assert_close(name: &str, expected: &svg2tgs_render::Image, result: &svg2tgs_render::Image) {
    let d = compare(expected, result);
    if d.mean > MAX_MEAN || d.bad_ratio > MAX_BAD {
        std::fs::create_dir_all(diff_dir()).unwrap();
        let out = diff_dir().join(format!("{name}.png"));
        std::fs::write(&out, diff_sheet(expected, result).to_png()).unwrap();
        panic!("{name}: {d:?}, см. {}", out.display());
    }
}

/// Обрезка по фону: рисунок на весь канвас и его обводка режутся по
/// скруглённой рамке фона (по внутреннему краю её рамки). Эталон — SVG
/// с `clipPath` той же формы.
#[test]
fn drawing_is_clipped_by_rounded_background() {
    use svg2tgs_core::{Background, Outline};
    let art = r##"<rect width="100" height="100" fill="#2f81f7"/><circle cx="50" cy="50" r="20" fill="#fff"/>"##;
    let svg =
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{art}</svg>"#);
    let (r, w) = (120.0_f32, 16.0_f32);
    let options = Options {
        background: Background::Solid("#fbe0e4".into()),
        background_radius: r,
        background_border: Some(Outline {
            width: w,
            color: "#20c070".into(),
        }),
        ..Options::default()
    };
    let reference = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
        <defs><clipPath id="f"><rect x="{w}" y="{w}" width="{iw}" height="{iw}" rx="{ir}"/></clipPath></defs>
        <rect width="512" height="512" rx="{r}" fill="#fbe0e4"/>
        <rect x="{h}" y="{h}" width="{bw}" height="{bw}" rx="{br}" fill="none" stroke="#20c070" stroke-width="{w}"/>
        <g clip-path="url(#f)"><g transform="scale(5.12)">{art}</g></g></svg>"##,
        h = w / 2.0,
        bw = 512.0 - w,
        br = r - w / 2.0,
        iw = 512.0 - 2.0 * w,
        ir = r - w,
    );
    let expected = render_svg(reference.as_bytes(), SIZE, &Options::default()).unwrap();
    let result = render_tgs(&convert(svg.as_bytes(), &options).unwrap().tgs, SIZE, 0).unwrap();
    assert_close("background-clip-tgs", &expected, &result);
    let preview = render_svg(svg.as_bytes(), SIZE, &options).unwrap();
    assert_close("background-clip-preview", &expected, &preview);

    // Без обрезки углы рисунка видны поверх фона — картинка другая.
    let open = Options {
        background_clip: false,
        ..options
    };
    let unclipped = render_tgs(&convert(svg.as_bytes(), &open).unwrap().tgs, SIZE, 0).unwrap();
    assert!(compare(&expected, &unclipped).bad_ratio > MAX_BAD);
}

/// Нарезка на эмодзи: клетки, сложенные вплотную, дают тот же рисунок,
/// что и весь холст целиком, — вместе с обводкой вокруг всего рисунка
/// (а не вдоль разрезов) и фоном на весь холст.
#[test]
fn grid_tiles_assemble_into_the_whole_picture() {
    use svg2tgs_core::{Background, Grid, Outline};
    let svg = std::fs::read(fixtures_dir().join("shapes-basic.svg")).unwrap();
    let options = Options {
        grid: Some(Grid { cols: 3, rows: 2 }),
        outline: Some(Outline {
            width: 10.0,
            color: "#20c070".into(),
        }),
        background: Background::Solid("#fbe0e4".into()),
        background_radius: 90.0,
        padding: 0.05,
        ..Options::default()
    };
    let tiles = convert_grid(&svg, &options).unwrap().tiles;
    assert_eq!(tiles.len(), 6);
    let cell = SIZE / 2;
    let tgs: Vec<Vec<u8>> = tiles.into_iter().map(|t| t.tgs).collect();
    let result = render_tgs_grid(&tgs, 3, cell).unwrap();
    let preview = render_svg(&svg, 3 * cell, &options).unwrap();
    assert_eq!(
        (preview.width, preview.height),
        (result.width, result.height)
    );
    // Обводку эталон (resvg) не рисует — сравниваем без неё…
    let plain = Options {
        outline: None,
        ..options.clone()
    };
    let plain_tiles: Vec<Vec<u8>> = convert_grid(&svg, &plain)
        .unwrap()
        .tiles
        .into_iter()
        .map(|t| t.tgs)
        .collect();
    let plain_result = render_tgs_grid(&plain_tiles, 3, cell).unwrap();
    let plain_preview = render_svg(&svg, 3 * cell, &plain).unwrap();
    assert_close("grid", &plain_preview, &plain_result);
    // …а с обводкой сверяем с одним большим TGS на том же холсте: собранный
    // из клеток результат не должен отличаться от целого.
    assert!(
        compare(&result, &plain_result).bad_ratio > MAX_BAD,
        "обводка видна"
    );
}

/// «По краю» при повороте: рамка считается по самому рисунку после поворота,
/// поэтому повёрнутый круг занимает то же место, что и без поворота
/// (раньше рамкой был повёрнутый квадрат вокруг круга — и круг уменьшался).
#[test]
fn content_bounds_follow_rotation() {
    use svg2tgs_core::Bounds;
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <circle cx="40" cy="50" r="25" fill="#2f81f7" stroke="#1e9e62" stroke-width="4"/></svg>"##;
    let still = Options {
        bounds: Bounds::Content,
        ..Options::default()
    };
    let turned = Options {
        rotation: 45.0,
        ..still.clone()
    };
    let a = render_tgs(&convert(svg, &still).unwrap().tgs, SIZE, 0).unwrap();
    let b = render_tgs(&convert(svg, &turned).unwrap().tgs, SIZE, 0).unwrap();
    assert_close("content-rotation", &a, &b);
    // Превью исходника считает так же.
    assert_close(
        "content-rotation-preview",
        &a,
        &render_svg(svg, SIZE, &turned).unwrap(),
    );
    // «По контейнеру» поворот рамку увеличивает — картинка другая.
    let boxed = Options {
        rotation: 45.0,
        ..Options::default()
    };
    let c = render_tgs(&convert(svg, &boxed).unwrap().tgs, SIZE, 0).unwrap();
    assert!(compare(&a, &c).bad_ratio > MAX_BAD);
}

//! Конвертация каждого файла из `tests/fixtures` и проверка того,
//! ради чего этот файл заведён (см. `tests/fixtures/README.md`).
//!
//! Попиксельное сравнение с исходным SVG — этап 2; здесь проверяется
//! структура Lottie и числа, на которых ошибался старый скрипт.

use std::io::Read;
use std::path::PathBuf;

use serde_json::Value;
use svg2tgs_core::{convert, Background, Error, Options, Preset, WarningCode};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

struct Out {
    json: Value,
    warnings: Vec<WarningCode>,
    ids: Vec<Option<String>>,
}

fn run_with(name: &str, options: &Options) -> Out {
    let conv = convert(&fixture(name), options).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(&conv.tgs[..])
        .read_to_string(&mut text)
        .expect("TGS должен быть корректным gzip");
    assert_eq!(text.len(), conv.json_bytes);
    Out {
        json: serde_json::from_str(&text).expect("внутри TGS должен быть JSON"),
        warnings: conv.warnings.iter().map(|w| w.code.clone()).collect(),
        ids: conv.warnings.iter().map(|w| w.element_id.clone()).collect(),
    }
}

fn run(name: &str) -> Out {
    run_with(name, &Options::default())
}

/// Все элементы с заданным `ty` во всём документе.
fn find_all<'a>(v: &'a Value, ty: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(map) => {
            if map.get("ty").and_then(Value::as_str) == Some(ty) {
                out.push(v);
            }
            map.values().for_each(|c| find_all(c, ty, out));
        }
        Value::Array(items) => items.iter().for_each(|c| find_all(c, ty, out)),
        _ => {}
    }
}

fn all<'a>(v: &'a Value, ty: &str) -> Vec<&'a Value> {
    let mut out = Vec::new();
    find_all(v, ty, &mut out);
    out
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("ожидалось число: {v}"))
}

/// Цвет Lottie (0–1) → `#rrggbb`.
fn hex(c: &Value) -> String {
    let k = &c["k"];
    let ch = |i: usize| (num(&k[i]) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", ch(0), ch(1), ch(2))
}

/// Границы всех вершин всех путей: (min_x, min_y, max_x, max_y).
fn bounds(json: &Value) -> (f64, f64, f64, f64) {
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for sh in all(json, "sh") {
        for p in sh["ks"]["k"]["v"].as_array().unwrap() {
            let (x, y) = (num(&p[0]), num(&p[1]));
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
    }
    b
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

// ---------------------------------------------------------------------------

#[test]
fn every_fixture_produces_valid_tgs_or_expected_error() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "svg") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let result = convert(&std::fs::read(&path).unwrap(), &Options::default());
        match name.as_str() {
            "invalid.svg" => assert!(matches!(result, Err(Error::InvalidSvg(_)))),
            "empty.svg" => assert!(matches!(result, Err(Error::Empty))),
            _ => {
                let out = run(&name);
                let j = &out.json;
                assert_eq!(j["tgs"], 1, "{name}: нет признака tgs");
                assert_eq!(j["w"], 512, "{name}");
                assert_eq!(j["h"], 512, "{name}");
                assert_eq!(j["fr"], 60, "{name}");
                assert_eq!(j["ip"], 0, "{name}");
                assert_eq!(j["op"], 60, "{name}");
                assert!(!j["layers"].as_array().unwrap().is_empty(), "{name}");
                let (x0, y0, x1, y1) = bounds(j);
                assert!(
                    x0 >= -1.0 && y0 >= -1.0 && x1 <= 513.0 && y1 <= 513.0,
                    "{name}: рисунок вышел за канвас: {x0} {y0} {x1} {y1}"
                );
            }
        }
        checked += 1;
    }
    assert_eq!(
        checked, 21,
        "количество тестовых SVG изменилось — обновите тест"
    );
}

#[test]
fn stroke_width_scales_with_drawing() {
    // Иконка 24×24 с обводкой 2 на канвасе 512: 2 × 512 / 24 ≈ 42.67.
    let out = run("stroke-icon.svg");
    let strokes = all(&out.json, "st");
    assert_eq!(strokes.len(), 2);
    for st in strokes {
        assert!(
            close(num(&st["w"]["k"]), 42.67, 0.01),
            "толщина {}",
            st["w"]["k"]
        );
        assert_eq!(st["lc"], 2, "round cap");
        assert_eq!(st["lj"], 2, "round join");
        // currentColor по умолчанию — чёрный.
        assert_eq!(hex(&st["c"]), "#000000");
    }
    assert!(
        all(&out.json, "fl").is_empty(),
        "fill=\"none\" не должен давать заливку"
    );
}

#[test]
fn evenodd_is_preserved() {
    let out = run("evenodd-ring.svg");
    let fills = all(&out.json, "fl");
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0]["r"], 2, "evenodd в Lottie — r: 2");
    // Оба круга — в одной группе, иначе дырки не будет.
    assert_eq!(all(&out.json, "sh").len(), 2);
}

#[test]
fn minified_arcs_are_parsed() {
    // Круг r=10 с центром (12,12) в квадрате 24 занимает 2…22 → 42.67…469.33.
    let out = run("svgo-minified-arcs.svg");
    let (x0, y0, x1, y1) = bounds(&out.json);
    for (got, want) in [(x0, 42.67), (y0, 42.67), (x1, 469.33), (y1, 469.33)] {
        assert!(close(got, want, 0.5), "границы {x0} {y0} {x1} {y1}");
    }
}

#[test]
fn group_opacity_is_applied_once() {
    let out = run("nested-opacity.svg");
    let fills = all(&out.json, "fl");
    // Круг: 0.5 × 0.5 = 25 %.
    assert!(
        fills.iter().any(|f| close(num(&f["o"]["k"]), 25.0, 0.01)),
        "нет заливки с прозрачностью 25 %"
    );
    // Пересекающиеся квадраты в группе с opacity 0.6 — отдельный слой
    // с прозрачностью 60 %, сами квадраты непрозрачные.
    let layers = out.json["layers"].as_array().unwrap();
    let isolated: Vec<_> = layers
        .iter()
        .filter(|l| close(num(&l["ks"]["o"]["k"]), 60.0, 0.01))
        .collect();
    assert_eq!(isolated.len(), 1, "ожидался один слой с прозрачностью 60 %");
    let inner = all(isolated[0], "fl");
    assert_eq!(inner.len(), 2);
    assert!(inner.iter().all(|f| close(num(&f["o"]["k"]), 100.0, 0.01)));
}

#[test]
fn use_and_symbol_are_expanded() {
    let out = run("use-symbol.svg");
    let mut colors: Vec<String> = all(&out.json, "fl").iter().map(|f| hex(&f["c"])).collect();
    colors.sort();
    assert_eq!(colors, ["#2f81f7", "#e8505b", "#f5b400"]);
}

#[test]
fn css_classes_and_ids_are_applied() {
    let out = run("css-classes.svg");
    let fills: Vec<String> = all(&out.json, "fl").iter().map(|f| hex(&f["c"])).collect();
    assert!(fills.contains(&"#1f9d55".to_string()));
    assert_eq!(fills.iter().filter(|c| *c == "#ffffff").count(), 2);
    let strokes = all(&out.json, "st");
    assert_eq!(strokes.len(), 1);
    assert_eq!(hex(&strokes[0]["c"]), "#1b1f24");
    // 4px в квадрате 100 на канвасе 512: 4 × 5.12.
    assert!(close(num(&strokes[0]["w"]["k"]), 20.48, 0.01));
}

#[test]
fn units_and_missing_viewbox() {
    // 512×256 без viewBox: масштаб 1, по вертикали отступ 128.
    let out = run("units-px.svg");
    let (x0, y0, x1, y1) = bounds(&out.json);
    assert!(close(x0, 16.0, 0.01) && close(x1, 496.0, 0.01), "{x0} {x1}");
    assert!(
        close(y0, 128.0 + 16.0, 0.01) && close(y1, 128.0 + 240.0, 0.01),
        "{y0} {y1}"
    );
    let st = all(&out.json, "st");
    assert!(close(num(&st[0]["w"]["k"]), 8.0, 0.01));
}

#[test]
fn linear_gradient_is_transferred() {
    let out = run("linear-gradient.svg");
    let gf = all(&out.json, "gf");
    assert_eq!(gf.len(), 1);
    assert_eq!(gf[0]["t"], 1);
    assert_eq!(gf[0]["g"]["p"], 3);
    // 3 цвета × 4 числа + 3 прозрачности × 2 числа (есть stop-opacity).
    assert_eq!(gf[0]["g"]["k"]["k"].as_array().unwrap().len(), 18);
    // Квадратная фигура: градиент по диагонали её рамки (10…90 → 51.2…460.8).
    let s = &gf[0]["s"]["k"];
    let e = &gf[0]["e"]["k"];
    assert!(close(num(&s[0]), 51.2, 0.01) && close(num(&e[0]), 460.8, 0.01));
    assert!(out.warnings.is_empty());
}

#[test]
fn linear_gradient_on_stretched_shape_stays_exact() {
    // Градиент objectBoundingBox на прямоугольнике 200×100: по диагонали рамки,
    // но линии одного цвета не параллельны второй диагонали. Конечная точка
    // должна учитывать растяжение: проверяем, что значение в углах — 0 и 1.
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">
        <linearGradient id="g" x2="1" y2="1"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient>
        <rect y="50" width="200" height="100" fill="url(#g)"/></svg>"##;
    let conv = convert(svg, &Options::default()).unwrap();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&conv.tgs[..])
        .read_to_string(&mut text)
        .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    let gf = all(&json, "gf")[0];
    let s = [num(&gf["s"]["k"][0]), num(&gf["s"]["k"][1])];
    let e = [num(&gf["e"]["k"][0]), num(&gf["e"]["k"][1])];
    let t = |p: [f64; 2]| {
        let d = [e[0] - s[0], e[1] - s[1]];
        ((p[0] - s[0]) * d[0] + (p[1] - s[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1])
    };
    // Канвас 512 / 200 = 2.56: прямоугольник 0…512 × 128…384.
    assert!(close(t([0.0, 128.0]), 0.0, 0.01));
    assert!(close(t([512.0, 384.0]), 1.0, 0.01));
    // В SVG углы (200,50) и (0,150) имеют значение 0.5.
    assert!(close(t([512.0, 128.0]), 0.5, 0.01));
    assert!(close(t([0.0, 384.0]), 0.5, 0.01));
}

#[test]
fn radial_gradient_is_transferred() {
    let out = run("radial-gradient.svg");
    let gf = all(&out.json, "gf");
    assert_eq!(gf.len(), 1);
    assert_eq!(gf[0]["t"], 2);
    // Центр (40,40) и радиус 50 → (204.8, 204.8), радиус 256.
    let s = &gf[0]["s"]["k"];
    let e = &gf[0]["e"]["k"];
    assert!(close(num(&s[0]), 204.8, 0.01) && close(num(&s[1]), 204.8, 0.01));
    assert!(close(num(&e[0]) - num(&s[0]), 256.0, 0.01));
    assert!(out.warnings.is_empty());
}

#[test]
fn text_becomes_paths() {
    let out = run("text.svg");
    if out.warnings.contains(&WarningCode::TextDropped) {
        // В системе без шрифтов текст пропускается — это тоже честное поведение.
        eprintln!("шрифтов нет, проверка текста пропущена");
        return;
    }
    assert!(out.warnings.contains(&WarningCode::TextConvertedToPaths));
    // Прямоугольник + несколько контуров букв.
    assert!(all(&out.json, "sh").len() > 3);
}

#[test]
fn transforms_are_baked() {
    // Зелёный прямоугольник 10…30 × 10…20, отражённый matrix(1 0 0 −1 0 100):
    // 10…30 × 80…90 → на канвасе 51.2…153.6 × 409.6…460.8.
    let out = run("transforms.svg");
    let groups = all(&out.json, "gr");
    let green = groups
        .iter()
        .find(|g| {
            all(g, "fl")
                .first()
                .is_some_and(|f| hex(&f["c"]) == "#1f9d55")
        })
        .expect("нет зелёного прямоугольника");
    let (x0, y0, x1, y1) = bounds(green);
    assert!(close(x0, 51.2, 0.01) && close(x1, 153.6, 0.01));
    assert!(close(y0, 409.6, 0.01) && close(y1, 460.8, 0.01));
}

#[test]
fn clip_path_is_applied_geometrically() {
    // Telegram запрещает маски: обрезка выполняется заранее.
    let out = run("clip-path.svg");
    let layers = out.json["layers"].as_array().unwrap();
    assert_eq!(layers.len(), 1);
    assert!(layers[0].get("masksProperties").is_none());
    assert!(layers[0].get("hasMask").is_none());
    // Две половинки круга r=35 (в квадрате 100 → радиус 179.2 вокруг 256).
    let fills = all(&out.json, "fl");
    assert_eq!(fills.len(), 2);
    let (x0, y0, x1, y1) = bounds(&out.json);
    let r = 179.2;
    for (got, want) in [
        (x0, 256.0 - r),
        (y0, 256.0 - r),
        (x1, 256.0 + r),
        (y1, 256.0 + r),
    ] {
        assert!(close(got, want, 0.2), "границы {x0} {y0} {x1} {y1}");
    }
}

/// Возможности Lottie, которые Telegram запрещает в стикерах
/// (https://core.telegram.org/stickers, «must not use»).
fn assert_telegram_safe(name: &str, json: &Value) {
    for ty in ["gs", "mm", "sr", "rp"] {
        assert!(all(json, ty).is_empty(), "{name}: запрещённый элемент {ty}");
    }
    for layer in json["layers"].as_array().unwrap() {
        assert_eq!(layer["ty"], 4, "{name}: разрешены только слои фигур");
        assert!(layer.get("masksProperties").is_none(), "{name}: маски");
        assert!(layer.get("ef").is_none(), "{name}: эффекты слоя");
        assert_ne!(layer["ddd"], 1, "{name}: 3D");
    }
}

#[test]
fn output_uses_only_features_allowed_by_telegram() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures");
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if !name.ends_with(".svg") || name == "invalid.svg" || name == "empty.svg" {
            continue;
        }
        let out = run(&name);
        assert_telegram_safe(&name, &out.json);
        let opts = Options {
            fit: svg2tgs_core::Fit::Cover,
            background: Background::Solid("#ffffff".into()),
            ..Options::default()
        };
        assert_telegram_safe(&name, &run_with(&name, &opts).json);
    }
}

#[test]
fn gradient_stroke_becomes_filled_outline() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <linearGradient id="g"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>
        <circle cx="50" cy="50" r="30" fill="none" stroke="url(#g)" stroke-width="8"/></svg>"##;
    let conv = convert(svg, &Options::default()).unwrap();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&conv.tgs[..])
        .read_to_string(&mut text)
        .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert!(all(&json, "gs").is_empty());
    assert!(all(&json, "st").is_empty());
    assert_eq!(all(&json, "gf").len(), 1, "контур с градиентной заливкой");
}

#[test]
fn content_beyond_canvas_is_cut() {
    // Режим «заполнить»: широкий рисунок выходит за канвас по бокам.
    let options = Options {
        fit: svg2tgs_core::Fit::Cover,
        ..Options::default()
    };
    let out = run_with("units-px.svg", &options);
    let (x0, y0, x1, y1) = bounds(&out.json);
    // Допуск обрезки — 1 px за краем канваса (невидимо).
    assert!(
        x0 >= -1.0 && y0 >= -1.0 && x1 <= 513.0 && y1 <= 513.0,
        "{x0} {y0} {x1} {y1}"
    );
    // Без обрезки прямоугольник 512×256 при «заполнить» шире канваса вдвое.
    assert!(x0 > -10.0 && x1 < 520.0);
}

#[test]
fn filter_is_reported_with_element_id() {
    let out = run("filter-blur.svg");
    assert_eq!(
        out.warnings,
        [WarningCode::FilterRemoved {
            filter: "blur".into()
        }]
    );
    assert_eq!(out.ids, [Some("shadow-shape".to_string())]);
}

#[test]
fn basic_shapes_are_all_present() {
    let out = run("shapes-basic.svg");
    assert_eq!(all(&out.json, "gr").len(), 6);
    assert_eq!(all(&out.json, "fl").len(), 4);
    assert_eq!(all(&out.json, "st").len(), 2);
}

#[test]
fn paint_order_puts_stroke_above_fill() {
    // В группе Lottie стиль, стоящий раньше, рисуется поверх.
    let out = run("css-classes.svg");
    for g in all(&out.json, "gr") {
        let it = g["it"].as_array().unwrap();
        let pos = |ty: &str| it.iter().position(|s| s["ty"] == ty);
        if let (Some(st), Some(fl)) = (pos("st"), pos("fl")) {
            assert!(st < fl);
        }
    }
}

#[test]
fn background_layer_is_at_the_bottom() {
    let options = Options {
        background: Background::Solid("#ffffff".into()),
        ..Options::default()
    };
    let out = run_with("shapes-basic.svg", &options);
    let layers = out.json["layers"].as_array().unwrap();
    let last = layers.last().unwrap();
    assert_eq!(hex(&all(last, "fl")[0]["c"]), "#ffffff");
    let inds: Vec<i64> = layers.iter().map(|l| l["ind"].as_i64().unwrap()).collect();
    assert_eq!(inds, (1..=layers.len() as i64).collect::<Vec<_>>());
}

#[test]
fn bad_background_is_an_error() {
    let options = Options {
        background: Background::Solid("красный".into()),
        ..Options::default()
    };
    let r = convert(&fixture("shapes-basic.svg"), &options);
    assert!(matches!(r, Err(Error::InvalidOption(_))));
}

#[test]
fn padding_shrinks_drawing() {
    let options = Options {
        padding: 0.1,
        ..Options::default()
    };
    // Круг на весь квадрат 24 (2…22) с отступом 10 %: область 409.6, сдвиг 51.2.
    let out = run_with("svgo-minified-arcs.svg", &options);
    let (x0, _, x1, _) = bounds(&out.json);
    let k = 409.6 / 24.0;
    assert!(close(x0, 51.2 + 2.0 * k, 0.5) && close(x1, 51.2 + 22.0 * k, 0.5));
}

#[test]
fn emoji_preset_and_overrides_affect_timing() {
    let options = Options {
        preset: Preset::Emoji,
        fps: Some(30),
        duration_secs: Some(2.0),
        ..Options::default()
    };
    let out = run_with("shapes-basic.svg", &options);
    assert_eq!(out.json["fr"], 30);
    assert_eq!(out.json["op"], 60);
    for l in out.json["layers"].as_array().unwrap() {
        assert_eq!(l["op"], 60);
    }
}

#[test]
fn output_is_deterministic() {
    let a = convert(&fixture("use-symbol.svg"), &Options::default()).unwrap();
    let b = convert(&fixture("use-symbol.svg"), &Options::default()).unwrap();
    assert_eq!(a.tgs, b.tgs);
}

#[test]
fn small_files_fit_the_limit() {
    for name in ["shapes-basic.svg", "text.svg", "use-symbol.svg"] {
        let conv = convert(&fixture(name), &Options::default()).unwrap();
        assert!(
            conv.tgs_bytes() < 4 * 1024,
            "{name}: {} байт",
            conv.tgs_bytes()
        );
        assert!(!conv
            .warnings
            .iter()
            .any(|w| matches!(w.code, WarningCode::SizeOverLimit { .. })));
    }
}

#[test]
fn zero_length_strokes_become_dots() {
    let out = run("dots.svg");
    // Линия — обводка; две круглые точки и одна квадратная — заливки
    // (по одной на элемент); точка с butt ничего не рисует.
    assert_eq!(all(&out.json, "st").len(), 1);
    let fills = all(&out.json, "fl");
    assert_eq!(fills.len(), 3);
    for f in &fills {
        assert_eq!(hex(&f["c"]), "#1b1f24");
    }
    // Точка диаметром с толщину линии: 2 × 512/24 ≈ 42.67 → радиус 21.33.
    let dot = all(&out.json, "sh")
        .into_iter()
        .find(|s| s["ks"]["k"]["v"].as_array().unwrap().len() == 4 && s["ks"]["k"]["c"] == true)
        .unwrap();
    let v = dot["ks"]["k"]["v"].as_array().unwrap();
    let w = (num(&v[1][0]) - num(&v[3][0])).abs();
    assert!(close(w, 42.67, 0.02), "диаметр точки {w}");
}

#[test]
fn subpaths_never_start_at_current_point() {
    for name in ["shared-start.svg", "dots.svg"] {
        let out = run(name);
        for g in all(&out.json, "gr") {
            let shapes: Vec<&Value> = g["it"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|s| s["ty"] == "sh")
                .collect();
            for w in shapes.windows(2) {
                let prev = &w[0]["ks"]["k"];
                let v = prev["v"].as_array().unwrap();
                let cur = if prev["c"] == true {
                    &v[0]
                } else {
                    v.last().unwrap()
                };
                let next = &w[1]["ks"]["k"]["v"][0];
                let same = close(num(&cur[0]), num(&next[0]), 0.5)
                    && close(num(&cur[1]), num(&next[1]), 0.5);
                assert!(!same, "{name}: подпуть начинается в текущей точке");
            }
        }
    }
}

#[test]
fn pill_has_no_duplicate_vertices() {
    let out = run("pill.svg");
    for sh in all(&out.json, "sh") {
        let v = sh["ks"]["k"]["v"].as_array().unwrap();
        for w in v.windows(2) {
            assert_ne!(w[0], w[1], "соседние вершины-дубликаты");
        }
    }
}

#[test]
fn dashes_are_cut_into_segments() {
    let out = run("dashed-curves.svg");
    // Пунктир нарезан конвертером: у обводок нет параметров пунктира Lottie,
    // а штрихов много — по подпути на каждый.
    for st in all(&out.json, "st") {
        assert!(st.get("d").is_none(), "пунктир не должен уходить в плеер");
    }
    assert!(all(&out.json, "sh").len() > 20);
    // Бусины — круги, залитые цветом обводки.
    let red_fill = all(&out.json, "fl")
        .into_iter()
        .any(|f| hex(&f["c"]) == "#e8505b");
    assert!(red_fill, "нет бусин");
}

#[test]
fn stretched_stroke_becomes_outline() {
    let out = run("stretched-stroke.svg");
    assert!(
        all(&out.json, "st").is_empty(),
        "обводка должна стать контуром"
    );
    let fills: Vec<String> = all(&out.json, "fl").iter().map(|f| hex(&f["c"])).collect();
    assert!(fills.contains(&"#1b1f24".to_string()), "контур обводки");
    assert!(fills.contains(&"#f5b400".to_string()), "заливка");
    // Обводка поверх заливки: её группа идёт в Lottie раньше.
    let groups = all(&out.json, "gr");
    let pos = |c: &str| {
        groups
            .iter()
            .position(|g| hex(&all(g, "fl")[0]["c"]) == c)
            .unwrap()
    };
    assert!(pos("#1b1f24") < pos("#f5b400"));
}

#[test]
fn thin_lines_are_reported_for_emoji_only() {
    let thin = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
        <circle cx="256" cy="256" r="200" fill="none" stroke="#000" stroke-width="4"/></svg>"##;
    let emoji = Options {
        preset: Preset::Emoji,
        ..Options::default()
    };
    let has = |c: &svg2tgs_core::Conversion| {
        c.warnings
            .iter()
            .any(|w| matches!(w.code, WarningCode::ThinLinesForEmoji { .. }))
    };
    assert!(has(&convert(thin, &emoji).unwrap()));
    assert!(
        !has(&convert(thin, &Options::default()).unwrap()),
        "у стикера не нужно"
    );
    // Иконка 24×24 со stroke-width 2 даёт ≈43 px — для эмодзи достаточно.
    assert!(!has(&convert(&fixture("stroke-icon.svg"), &emoji).unwrap()));
}

/// Сотни мелких зубчатых многоугольников: без упрощения > 64 КБ.
fn heavy_svg() -> String {
    let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">"#);
    for row in 0..20 {
        for col in 0..20 {
            let (cx, cy) = (13.0 + col as f64 * 25.6, 13.0 + row as f64 * 25.6);
            let mut d = String::new();
            for k in 0..120 {
                let a = k as f64 / 120.0 * std::f64::consts::TAU;
                let r = 10.0 + 0.05 * ((k * 7 + row + col) % 3) as f64;
                d += &format!(
                    "{}{:.3} {:.3}",
                    if k == 0 { "M" } else { "L" },
                    cx + r * a.cos(),
                    cy + r * a.sin()
                );
            }
            svg += &format!(r##"<path fill="#2f81f7" d="{d}Z"/>"##);
        }
    }
    svg += "</svg>";
    svg
}

#[test]
fn heavy_drawing_is_simplified_to_fit_the_limit() {
    let svg = heavy_svg();
    let lossless = convert(
        svg.as_bytes(),
        &Options {
            optimization: svg2tgs_core::Optimization::Lossless,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        lossless.tgs_bytes() > 64 * 1024,
        "тест должен быть тяжёлым: {}",
        lossless.tgs_bytes()
    );

    let auto = convert(svg.as_bytes(), &Options::default()).unwrap();
    assert!(auto.tgs_bytes() <= 64 * 1024, "{} байт", auto.tgs_bytes());
    assert!(auto
        .warnings
        .iter()
        .any(|w| matches!(w.code, WarningCode::Simplified { .. })));
    assert!(!auto
        .warnings
        .iter()
        .any(|w| matches!(w.code, WarningCode::SizeOverLimit { .. })));
}

#[test]
fn cover_fits_a_smaller_limit() {
    // Обложка набора стикеров: Telegram принимает TGS до 32 КБ — предел
    // меньше обычного. Здесь предел ещё меньше, чем обычный результат.
    let svg = heavy_svg();
    let usual = convert(svg.as_bytes(), &Options::default()).unwrap();
    let limit = usual.tgs_bytes() * 2 / 3;
    let tiles =
        svg2tgs_core::convert_grid_limited(svg.as_bytes(), &Options::default(), limit).unwrap();
    let cover = &tiles.tiles[0];
    assert!(
        cover.tgs_bytes() <= limit,
        "{} > {limit}",
        cover.tgs_bytes()
    );
    assert!(!cover
        .warnings
        .iter()
        .any(|w| matches!(w.code, WarningCode::SizeOverLimit { .. })));
    // Обычный предел — тот же результат, что у `convert`.
    let same =
        svg2tgs_core::convert_grid_limited(svg.as_bytes(), &Options::default(), 64 * 1024).unwrap();
    assert_eq!(same.tiles[0].tgs, usual.tgs);
}

#[test]
fn huge_single_shape_is_simplified_below_rlottie_limit() {
    // Один контур из 20 000 вершин (≈60 000 точек — больше потолка
    // растеризатора rlottie) с еле заметной «рябью».
    let mut d = String::new();
    for k in 0..20_000 {
        let a = k as f64 / 20_000.0 * std::f64::consts::TAU;
        let r = 200.0 + 0.02 * (k % 2) as f64;
        d += &format!(
            "{}{:.3} {:.3}",
            if k == 0 { "M" } else { "L" },
            256.0 + r * a.cos(),
            256.0 + r * a.sin()
        );
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path fill="#2f81f7" d="{d}Z"/></svg>"##
    );
    let conv = convert(svg.as_bytes(), &Options::default()).unwrap();
    assert!(!conv
        .warnings
        .iter()
        .any(|w| matches!(w.code, WarningCode::TooComplex { .. })));
    let mut text = String::new();
    flate2::read::GzDecoder::new(&conv.tgs[..])
        .read_to_string(&mut text)
        .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    let verts: usize = all(&json, "sh")
        .iter()
        .map(|s| s["ks"]["k"]["v"].as_array().unwrap().len())
        .sum();
    assert!(verts * 3 < 30_000, "вершин {verts}");
}

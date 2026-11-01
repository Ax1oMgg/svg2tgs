//! svg2tgs-render — картинки для превью и проверок.
//!
//! - [`render_tgs`] рисует TGS через rlottie — движок, которым Telegram
//!   показывает стикеры. Что нарисовано здесь, то увидит пользователь.
//! - [`render_svg`] рисует исходный SVG через resvg — эталон для сравнения,
//!   в тех же координатах, что и конвертер (вписывание, отступы, фон).
//! - [`compare`] считает разницу двух картинок, [`diff_sheet`] собирает
//!   картинку «эталон / результат / различия».

mod compare;
#[cfg(feature = "rlottie")]
mod ffi;

#[cfg(feature = "rlottie")]
use std::ffi::CString;
#[cfg(feature = "rlottie")]
use std::io::Read;
#[cfg(feature = "rlottie")]
use std::sync::Mutex;

pub use compare::{compare, diff_sheet, Diff};
use svg2tgs_core::Options;

/// Какой rlottie вкомпилирован: `rlottie-samsung`, `rlottie-telegram`
/// или `none` (сборка без rlottie — веб-версия).
pub const RLOTTIE_VENDOR: &str = env!("SVG2TGS_RLOTTIE_VENDOR");

/// Картинка RGBA, 8 бит на канал, прозрачность не умножена на цвет.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// Кодирует в PNG.
    pub fn to_png(&self) -> Vec<u8> {
        self.to_pixmap()
            .encode_png()
            .expect("кодирование PNG в память")
    }

    fn to_pixmap(&self) -> tiny_skia::Pixmap {
        let mut pm = tiny_skia::Pixmap::new(self.width, self.height).expect("размер > 0");
        for (dst, src) in pm.pixels_mut().iter_mut().zip(self.rgba.as_chunks::<4>().0) {
            *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
        }
        pm
    }

    fn from_pixmap(pm: &tiny_skia::Pixmap) -> Self {
        let mut rgba = Vec::with_capacity(pm.data().len());
        for p in pm.pixels() {
            let c = p.demultiply();
            rgba.extend([c.red(), c.green(), c.blue(), c.alpha()]);
        }
        Self {
            width: pm.width(),
            height: pm.height(),
            rgba,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("TGS повреждён: {0}")]
    BadTgs(String),
    #[error("rlottie не смог открыть анимацию")]
    BadLottie,
    #[error(transparent)]
    Svg(#[from] svg2tgs_core::Error),
    #[error("размер картинки должен быть от 1 до 4096")]
    BadSize,
}

/// rlottie собран без потоков и кэша; вызовы сериализуются, чтобы
/// параллельные тесты и превью не мешали друг другу.
#[cfg(feature = "rlottie")]
static RLOTTIE: Mutex<()> = Mutex::new(());

#[cfg(feature = "rlottie")]
/// Рисует кадр Lottie JSON квадратом `size`×`size` через rlottie.
pub fn render_lottie(json: &[u8], size: u32, frame: usize) -> Result<Image, RenderError> {
    if size == 0 || size > 4096 {
        return Err(RenderError::BadSize);
    }
    let data = CString::new(json).map_err(|_| RenderError::BadLottie)?;
    // Ключ кэша rlottie: кэш выключен, но пустая строка безопаснее null.
    let key = CString::new("").unwrap();
    let resources = CString::new("").unwrap();
    let n = size as usize;
    let mut buf = vec![0u32; n * n];

    let _guard = RLOTTIE.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: строки живут до конца вызова; буфер n×n u32 с шагом строки n×4
    // байт ровно соответствует аргументам; анимация удаляется один раз.
    unsafe {
        let anim = ffi::lottie_animation_from_data(data.as_ptr(), key.as_ptr(), resources.as_ptr());
        if anim.is_null() {
            return Err(RenderError::BadLottie);
        }
        let total = ffi::lottie_animation_get_totalframe(anim);
        let frame = frame.min(total.saturating_sub(1));
        ffi::lottie_animation_render(anim, frame, buf.as_mut_ptr(), n, n, n * 4);
        ffi::lottie_animation_destroy(anim);
    }
    drop(_guard);

    // ARGB32 с умноженной прозрачностью → RGBA с обычной.
    let mut img = Image::new(size, size);
    for (px, dst) in buf.iter().zip(img.rgba.as_chunks_mut::<4>().0) {
        let a = (px >> 24) & 0xff;
        if a == 0 {
            continue;
        }
        let un = |c: u32| ((c * 255 + a / 2) / a).min(255) as u8;
        dst.copy_from_slice(&[
            un((px >> 16) & 0xff),
            un((px >> 8) & 0xff),
            un(px & 0xff),
            a as u8,
        ]);
    }
    Ok(img)
}

#[cfg(feature = "rlottie")]
/// Рисует кадр TGS (gzip-сжатый Lottie) так, как его покажет Telegram.
pub fn render_tgs(tgs: &[u8], size: u32, frame: usize) -> Result<Image, RenderError> {
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(tgs)
        .read_to_end(&mut json)
        .map_err(|e| RenderError::BadTgs(e.to_string()))?;
    render_lottie(&json, size, frame)
}

#[cfg(feature = "rlottie")]
/// Склеивает клетки нарезки (TGS по строкам слева направо) в одну картинку:
/// клетка — `cell`×`cell` пикселей, вплотную, как эмодзи в строке Telegram.
pub fn render_tgs_grid(tiles: &[Vec<u8>], cols: u32, cell: u32) -> Result<Image, RenderError> {
    let cols = cols.max(1);
    let rows = (tiles.len() as u32).div_ceil(cols).max(1);
    let mut out = Image::new(cols * cell, rows * cell);
    for (n, tgs) in tiles.iter().enumerate() {
        let img = render_tgs(tgs, cell, 0)?;
        let (cx, cy) = ((n as u32 % cols) * cell, (n as u32 / cols) * cell);
        for y in 0..cell {
            let src = (y * cell * 4) as usize;
            let dst = (((cy + y) * out.width + cx) * 4) as usize;
            let len = (cell * 4) as usize;
            out.rgba[dst..dst + len].copy_from_slice(&img.rgba[src..src + len]);
        }
    }
    Ok(out)
}

/// Рисует исходный SVG через resvg в тех же координатах, что и конвертер:
/// канвас пресета (или весь холст сетки нарезки), вписывание, отступы, фон
/// и обрезка по фону из `options`. Длинная сторона картинки — `size`.
pub fn render_svg(svg: &[u8], size: u32, options: &Options) -> Result<Image, RenderError> {
    if size == 0 || size > 4096 {
        return Err(RenderError::BadSize);
    }
    let tree = svg2tgs_core::parse_svg(svg)?;
    let grid = options.grid();
    let tile = options.effective_canvas_size() as f32;
    let (bw, bh) = svg2tgs_core::board_size(tile, grid);
    let background = svg2tgs_core::background_plan_for(options, bw as f64, bh as f64)?;
    let placement = svg2tgs_core::place_tree(&tree, tile, grid, options);
    let k = size as f32 / bw.max(bh);
    let (iw, ih) = (
        ((bw * k).round() as u32).max(1),
        ((bh * k).round() as u32).max(1),
    );
    let transform = tiny_skia::Transform::from_scale(k, k).pre_concat(placement.transform);

    let mut pm = tiny_skia::Pixmap::new(iw, ih).expect("размер > 0");
    draw_background(&mut pm, &background, k);
    // Рисунок — отдельно и поверх фона только внутри видимой области,
    // как в TGS (там лишнее обрезается геометрически).
    let mut art = tiny_skia::Pixmap::new(iw, ih).expect("размер > 0");
    resvg::render(&tree, transform, &mut art.as_mut());
    let [x0, y0, x1, y1] = placement.clip.map(|v| v as f32 * k);
    let mut mask = tiny_skia::Mask::new(iw, ih).expect("размер > 0");
    if let Some(rect) = tiny_skia::Rect::from_ltrb(x0, y0, x1, y1) {
        let path = tiny_skia::PathBuilder::from_rect(rect);
        mask.fill_path(
            &path,
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );
    }
    // Обрезка по фону: рамка — внутренний край рамки фона.
    if options.background_clip && (background.fill.is_some() || background.border.is_some()) {
        let inset = background.border.map_or(0.0, |(_, w)| w as f32) * k;
        let r = background.radius as f32 * k - inset;
        if let Some(frame) = rounded_rect(inset, inset, iw as f32 - inset, ih as f32 - inset, r) {
            let mut fm = tiny_skia::Mask::new(iw, ih).expect("размер > 0");
            fm.fill_path(
                &frame,
                tiny_skia::FillRule::Winding,
                true,
                tiny_skia::Transform::identity(),
            );
            for (m, f) in mask.data_mut().iter_mut().zip(fm.data()) {
                *m = ((*m as u16 * *f as u16 + 127) / 255) as u8;
            }
        }
    }
    pm.draw_pixmap(
        0,
        0,
        art.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        Some(&mask),
    );
    Ok(Image::from_pixmap(&pm))
}

/// Фон как в TGS: заливка и рамка — скруглённые прямоугольники на весь
/// холст (`k` — масштаб от холста к картинке).
fn draw_background(pm: &mut tiny_skia::Pixmap, plan: &svg2tgs_core::BackgroundPlan, k: f32) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let r = plan.radius as f32 * k;
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    if let Some([red, g, b]) = plan.fill {
        paint.set_color_rgba8(red, g, b, 255);
        if let Some(path) = rounded_rect(0.0, 0.0, w, h, r) {
            pm.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    if let Some(([red, g, b], bw)) = plan.border {
        let bw = bw as f32 * k;
        paint.set_color_rgba8(red, g, b, 255);
        let mut pb = tiny_skia::PathBuilder::new();
        for (inset, radius) in [(0.0, r), (bw, r - bw)] {
            if let Some(p) = rounded_rect(inset, inset, w - inset, h - inset, radius) {
                pb.push_path(&p);
            }
        }
        if let Some(ring) = pb.finish() {
            pm.fill_path(
                &ring,
                &paint,
                tiny_skia::FillRule::EvenOdd,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
}

fn rounded_rect(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.max(0.0).min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let k = 0.552_284_8 * r;
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod svg_tests {
    use super::*;

    #[test]
    fn svg_reference_uses_converter_fit() {
        // Квадрат 0…10 в viewBox 20×10: вписывается в 512 с отступом 128 сверху.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10"><rect width="10" height="10"/></svg>"#;
        let img = render_svg(svg, 512, &Options::default()).unwrap();
        assert_eq!(img.pixel(10, 200)[3], 255);
        assert_eq!(img.pixel(10, 100)[3], 0);
        assert_eq!(img.pixel(300, 200)[3], 0);
    }

    #[test]
    fn png_roundtrip_has_signature() {
        let png = Image::new(4, 4).to_png();
        assert_eq!(&png[1..4], b"PNG");
    }
}

#[cfg(all(test, feature = "rlottie"))]
mod tests {
    use super::*;

    const RED_SQUARE: &str = r##"{"v":"5.7.4","fr":60,"ip":0,"op":60,"w":100,"h":100,"ddd":0,"assets":[],
        "layers":[{"ddd":0,"ind":1,"ty":4,"sr":1,"ao":0,"ip":0,"op":60,"st":0,"bm":0,
        "ks":{"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[0,0,0]},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]}},
        "shapes":[{"ty":"gr","it":[
          {"ty":"sh","ks":{"a":0,"k":{"c":true,"i":[[0,0],[0,0],[0,0],[0,0]],"o":[[0,0],[0,0],[0,0],[0,0]],"v":[[25,25],[75,25],[75,75],[25,75]]}}},
          {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":50},"r":1},
          {"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}]}]}]}"##;

    #[test]
    fn rlottie_draws_a_square_with_straight_alpha() {
        let img = render_lottie(RED_SQUARE.as_bytes(), 100, 0).unwrap();
        // Центр: красный с прозрачностью 50 %, цвет не умножен на альфу.
        let [r, g, b, a] = img.pixel(50, 50);
        assert!(
            (r as i32 - 255).abs() <= 2 && g == 0 && b == 0,
            "{r} {g} {b}"
        );
        assert!((a as i32 - 128).abs() <= 2, "альфа {a}");
        // Угол пуст.
        assert_eq!(img.pixel(5, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn rlottie_scales_to_requested_size() {
        let img = render_lottie(RED_SQUARE.as_bytes(), 200, 0).unwrap();
        assert_eq!(img.width, 200);
        assert_eq!(img.pixel(100, 100)[3] as i32 / 10, 12);
        assert_eq!(img.pixel(40, 40)[3], 0);
    }

    #[test]
    fn broken_input_is_an_error() {
        assert!(matches!(
            render_lottie(b"{oops", 10, 0),
            Err(RenderError::BadLottie)
        ));
        assert!(matches!(
            render_tgs(b"not gzip", 10, 0),
            Err(RenderError::BadTgs(_))
        ));
        assert!(matches!(
            render_lottie(RED_SQUARE.as_bytes(), 0, 0),
            Err(RenderError::BadSize)
        ));
    }

    /// Обёртка анимации 100×100 вокруг готовых слоёв.
    fn animation(layers: &[String]) -> String {
        format!(
            r#"{{"v":"5.7.4","fr":60,"ip":0,"op":60,"w":100,"h":100,"ddd":0,"assets":[],"layers":[{}]}}"#,
            layers.join(",")
        )
    }

    fn layer(ind: usize, opacity: u32, path: &str, rgb: &str) -> String {
        format!(
            r#"{{"ddd":0,"ind":{ind},"ty":4,"sr":1,"ao":0,"ip":0,"op":60,"st":0,"bm":0,
            "ks":{{"o":{{"a":0,"k":{opacity}}},"r":{{"a":0,"k":0}},"p":{{"a":0,"k":[0,0,0]}},"a":{{"a":0,"k":[0,0,0]}},"s":{{"a":0,"k":[100,100,100]}}}},
            "shapes":[{{"ty":"gr","it":[{{"ty":"sh","ks":{{"a":0,"k":{path}}}}},
              {{"ty":"fl","c":{{"a":0,"k":[{rgb},1]}},"o":{{"a":0,"k":100}},"r":1}},
              {{"ty":"tr","p":{{"a":0,"k":[0,0]}},"a":{{"a":0,"k":[0,0]}},"s":{{"a":0,"k":[100,100]}},"r":{{"a":0,"k":0}},"o":{{"a":0,"k":100}}}}]}}]}}"#
        )
    }

    fn square(x: u32, y: u32, s: u32) -> String {
        format!(
            r#"{{"c":true,"i":[[0,0],[0,0],[0,0],[0,0]],"o":[[0,0],[0,0],[0,0],[0,0]],"v":[[{x},{y}],[{},{y}],[{},{}],[{x},{}]]}}"#,
            x + s,
            x + s,
            y + s,
            y + s
        )
    }

    /// Правка в vendor/rlottie-samsung (vdrawhelper_common.cpp): без неё
    /// каждый полупрозрачный слой делает всё под собой прозрачнее на 1/255.
    #[test]
    fn semi_transparent_layers_do_not_fade_the_background() {
        let mut layers: Vec<String> = (0..45)
            .map(|k| {
                layer(
                    k + 1,
                    60,
                    &square(10 + (k as u32 % 8) * 10, 10 + (k as u32 / 8) * 10, 6),
                    "0,0,1",
                )
            })
            .collect();
        layers.push(layer(46, 100, &square(0, 0, 100), "1,0,0"));
        let img = render_lottie(animation(&layers).as_bytes(), 100, 0).unwrap();
        assert_eq!(img.pixel(95, 95), [255, 0, 0, 255]);
    }

    /// Квадрат 80×80, контур которого разбит на `per_side × 4` вершин.
    fn dense_square(per_side: usize) -> String {
        let mut v = Vec::new();
        for side in 0..4 {
            for k in 0..per_side {
                let t = 80.0 * k as f64 / per_side as f64;
                let p = match side {
                    0 => [10.0 + t, 10.0],
                    1 => [90.0, 10.0 + t],
                    2 => [90.0 - t, 90.0],
                    _ => [10.0, 90.0 - t],
                };
                v.push(format!("[{:.3},{:.3}]", p[0], p[1]));
            }
        }
        let zeros = vec!["[0,0]"; v.len()].join(",");
        format!(
            r#"{{"c":true,"i":[{zeros}],"o":[{zeros}],"v":[{}]}}"#,
            v.join(",")
        )
    }

    /// Правка в vendor/rlottie-samsung (lottieitem.cpp): лимит 15000 точек
    /// снят — иначе большие стикеры рисуются частично, а в Telegram целиком.
    /// 8000 вершин ≈ 24 000 точек: больше лимита Samsung, меньше потолка
    /// растеризатора (32 767).
    #[test]
    fn many_points_are_all_drawn() {
        let json = animation(&[layer(1, 100, &dense_square(2000), "0,0,1")]);
        let img = render_lottie(json.as_bytes(), 100, 0).unwrap();
        assert_eq!(img.pixel(50, 50), [0, 0, 255, 255]);
    }

    /// Правка в vendor/rlottie-*/vraster.cpp: фигура больше 32 767 точек
    /// раньше подвешивала отрисовку навсегда. Теперь она пропускается.
    #[test]
    fn too_many_points_do_not_hang() {
        let json = animation(&[layer(1, 100, &dense_square(5000), "0,0,1")]);
        let img = render_lottie(json.as_bytes(), 100, 0).unwrap();
        assert_eq!(
            img.pixel(50, 50)[3],
            0,
            "такую фигуру растеризатор не рисует"
        );
    }
}

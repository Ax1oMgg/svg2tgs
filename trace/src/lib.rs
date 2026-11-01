//! svg2tgs-trace — растровая картинка (PNG, JPEG, WebP) → контуры → TGS.
//!
//! Трассировка выдаёт SVG, дальше — обычный путь ядра (`svg2tgs-core`):
//! вписывание, обводка, нарезка, автоподгонка. Здесь:
//!
//! - чтение картинки (`decode`), «Убрать фон», пиксель-арт и фото
//!   (`analyze`) — для «Авто»;
//! - рабочий масштаб: от того, сколько пикселей рисунок займёт на холсте,
//!   с пределом по числу пикселей; пороги — в пикселях канваса;
//! - число цветов (`quantize`), градиенты (`gradient`), контуры
//!   (`vectorize`, перенесено из VTracer);
//! - автоподгонка под лимит: сначала меньше масштаб, затем меньше цветов
//!   и крупнее пятна, затем обычное упрощение контуров ядра.
//!
//! Без сети и без импортов в WebAssembly; случайность — с постоянным seed.

mod analyze;
mod decode;
mod gradient;
mod image;
mod quantize;
mod vectorize;

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use svg2tgs_core::{
    Conversion, Error, Optimization, Options, PathMode, Tiles, TraceInfo, TracePreset, Warning,
    WarningCode,
};
use visioncortex::PathSimplifyMode;

pub use decode::{sniff, Format};

use crate::analyze::Blocks;
use crate::image::Rgba;

/// Наибольшее число пикселей трассировки.
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_TRACE_PIXELS: u64 = 4 * 1024 * 1024;
/// В браузере — один поток и память WebAssembly.
#[cfg(target_arch = "wasm32")]
pub const MAX_TRACE_PIXELS: u64 = 2 * 1024 * 1024;
/// Наименьшая длинная сторона трассировки (если картинка не меньше).
const MIN_TRACE_SIDE: f64 = 64.0;
/// Наименьшая непрозрачность, которая ещё участвует в разбиении на цвета
/// (полупрозрачные пиксели — для градиентов прозрачности; вне градиента
/// прозрачнее 50 % — прозрачно).
const ALPHA_MIN: u8 = 16;
/// Длинная сторона картинки в превью исходника.
const PREVIEW_SIDE: u32 = 2048;

/// Растр ли это (по сигнатуре).
pub fn is_raster(bytes: &[u8]) -> bool {
    sniff(bytes).is_some()
}

/// Итог трассировки и конвертации.
#[derive(Debug)]
pub struct Traced {
    pub tiles: Tiles,
    pub info: TraceInfo,
    /// SVG, которым стала картинка (последняя ступень автоподгонки).
    pub svg: String,
}

/// Прочитанная картинка и то, что о ней известно (без настроек вписывания).
struct Prepared {
    img: Rgba,
    format: Format,
    blocks: Option<Blocks>,
    photo: bool,
    /// Фон убран (при `remove_bg`).
    bg_removed: bool,
    /// Рамка видимого (непрозрачность от [`ALPHA_MIN`]) `[x0, y0, x1, y1)`.
    bounds: Option<[u32; 4]>,
}

fn hash_of(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Маленький кэш «ключ → значение» на последние `N` записей.
struct Cache<K, V, const N: usize>(Vec<(K, Arc<V>)>);

impl<K: PartialEq, V, const N: usize> Cache<K, V, N> {
    const fn new() -> Self {
        Self(Vec::new())
    }
    fn get(&mut self, k: &K) -> Option<Arc<V>> {
        let i = self.0.iter().position(|(key, _)| key == k)?;
        let e = self.0.remove(i);
        let v = e.1.clone();
        self.0.push(e);
        Some(v)
    }
    fn put(&mut self, k: K, v: Arc<V>) {
        if self.0.len() >= N {
            self.0.remove(0);
        }
        self.0.push((k, v));
    }
}

static PREPARED: Mutex<Cache<(u64, usize, bool), Prepared, 4>> = Mutex::new(Cache::new());
static TRACED: Mutex<Cache<(u64, TraceKey), vectorize::Svg, 8>> = Mutex::new(Cache::new());

fn prepare(bytes: &[u8], remove_bg: bool) -> Result<Arc<Prepared>, Error> {
    let key = (hash_of(bytes), bytes.len(), remove_bg);
    if let Some(p) = PREPARED.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Ok(p);
    }
    let format = sniff(bytes).ok_or_else(|| Error::InvalidImage("не растр".into()))?;
    let mut img = decode::decode(bytes)?;
    let bg_removed = remove_bg && analyze::remove_background(&mut img);
    let blocks = analyze::find_blocks(&img);
    // Фото — по числу цветов картинки разумного размера: оно растёт
    // с размером.
    let probe = fit_side(&img, 512);
    let photo = blocks.is_none() && analyze::is_photo(&probe);
    // Рамка — по всему, что участвует в трассировке (и в градиентах
    // прозрачности), а не только по непрозрачному.
    let bounds = img.opaque_bounds(ALPHA_MIN);
    let p = Arc::new(Prepared {
        img,
        format,
        blocks,
        photo,
        bg_removed,
        bounds,
    });
    PREPARED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(key, p.clone());
    Ok(p)
}

/// Картинка, уменьшенная до длинной стороны `side` (если больше).
fn fit_side(img: &Rgba, side: u32) -> Rgba {
    let long = img.w.max(img.h);
    if long <= side {
        return img.clone();
    }
    let k = side as f64 / long as f64;
    img.resize_area(
        ((img.w as f64 * k).round() as u32).max(1),
        ((img.h as f64 * k).round() as u32).max(1),
    )
}

/// SVG для превью исходника: картинка внутри документа того же размера,
/// что и трассировка, — превью «оригинал / результат» и кадрирование
/// совпадают. Непрозрачная часть кладётся на своё место, чтобы рамка
/// «по краю рисунка» совпадала с трассировкой.
pub fn preview_svg(bytes: &[u8], options: &Options) -> Result<Vec<u8>, Error> {
    let p = prepare(bytes, options.trace.remove_bg)?;
    let (w, h) = (p.img.w, p.img.h);
    let Some(b) = p.bounds else {
        return Ok(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"/>"#
        )
        .into_bytes());
    };
    let part = fit_side(&p.img.crop(b), PREVIEW_SIDE);
    let rendering = if p.blocks.is_some() {
        r#" image-rendering="optimizeSpeed""#
    } else {
        ""
    };
    Ok(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none"{rendering} href="data:image/png;base64,{}"/></svg>"#,
        b[0],
        b[1],
        b[2] - b[0],
        b[3] - b[1],
        base64(&part.to_png())
    )
    .into_bytes())
}

/// Пресет после «Авто».
fn resolve(preset: TracePreset, p: &Prepared) -> TracePreset {
    match preset {
        TracePreset::Auto if p.blocks.is_some() => TracePreset::Pixel,
        TracePreset::Auto if p.photo => TracePreset::Photo,
        TracePreset::Auto => TracePreset::Drawing,
        other => other,
    }
}

/// Что определяет SVG трассировки (ключ кэша).
#[derive(Debug, Clone, Copy, PartialEq)]
struct TraceKey {
    preset: TracePreset,
    /// Размер трассировки.
    tw: u32,
    th: u32,
    colors: u32,
    /// Сторона наименьшего пятна, пиксели трассировки.
    speckle: f32,
    gradients: bool,
    /// Пикселей трассировки на пиксель канваса.
    t2c: f32,
    remove_bg: bool,
}

/// «Детали» → сторона наименьшего пятна (по пресету: в пикселях канваса,
/// у пиксель-арта — в блоках): от `speckle.0` при 0 до `speckle.1` при 100.
fn speckle_side(preset: TracePreset, details: u32) -> f32 {
    let (a, b) = preset.params().speckle;
    a * (b / a).powf(details as f32 / svg2tgs_core::MAX_DETAILS as f32)
}

/// «Детали», при которых пятно в `mul` раз крупнее, чем при `details`.
fn details_for_speckle(preset: TracePreset, details: u32, mul: f32) -> u32 {
    let (a, b) = preset.params().speckle;
    let d = details as f32 + svg2tgs_core::MAX_DETAILS as f32 * mul.ln() / (b / a).ln();
    d.round().clamp(0.0, svg2tgs_core::MAX_DETAILS as f32) as u32
}

/// Ступень автоподгонки: множители масштаба, числа цветов и пятна.
const FIT_LADDER: [(f64, f32, f32); 6] = [
    (1.0, 1.0, 1.0),
    (0.71, 1.0, 1.0),
    (0.5, 1.0, 1.0),
    (0.5, 0.75, 1.5),
    (0.5, 0.5, 2.25),
    (0.5, 0.35, 3.4),
];

/// Трассирует растр и конвертирует в TGS (с нарезкой, если она включена).
pub fn convert_grid(bytes: &[u8], options: &Options) -> Result<Traced, Error> {
    convert_grid_limited(bytes, options, options.preset.max_bytes())
}

/// Как [`convert_grid`], но с другим пределом размера файла (обложка набора).
pub fn convert_grid_limited(
    bytes: &[u8],
    options: &Options,
    max_bytes: usize,
) -> Result<Traced, Error> {
    let p = prepare(bytes, options.trace.remove_bg)?;
    let trace = &options.trace;
    let preset = resolve(trace.preset, &p);
    let params = preset.params();
    let colors = trace.colors_for(preset);
    let details = trace.details_for(preset);
    let gradients = trace.gradients_for(preset);

    // Рамка рисунка в пикселях картинки — как её увидит ядро.
    let (w, h) = (p.img.w, p.img.h);
    let rect = match (options.bounds, p.bounds) {
        (svg2tgs_core::Bounds::Content, Some(b)) => [
            b[0] as f32,
            b[1] as f32,
            (b[2] - b[0]) as f32,
            (b[3] - b[1]) as f32,
        ],
        _ => [0.0, 0.0, w as f32, h as f32],
    };
    let (canvas_scale, grid) = svg2tgs_core::board_scale(rect, options);
    let tiles_n = grid.cols * grid.rows;
    let k = trace
        .scale
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(params.scale * if tiles_n > 1 { params.grid_scale } else { 1.0 })
        as f64;

    let auto = options.optimization == Optimization::Auto;
    let ladder: &[(f64, f32, f32)] = if auto { &FIT_LADDER } else { &FIT_LADDER[..1] };
    let mut last = None;
    for (step, &(scale_mul, colors_mul, speckle_mul)) in ladder.iter().enumerate() {
        // Пиксель-арт не уменьшаем: блоки точные.
        let scale_mul = if preset == TracePreset::Pixel {
            1.0
        } else {
            scale_mul
        };
        if step > 0 && preset == TracePreset::Pixel && colors_mul == 1.0 && speckle_mul == 1.0 {
            continue;
        }
        let colors_now = ((colors as f32 * colors_mul).round() as u32).max(params.colors.0);
        let details_now = details_for_speckle(preset, details, speckle_mul);
        let (key, frame, source) = plan(
            &p,
            preset,
            canvas_scale,
            k * scale_mul,
            colors_now,
            details_now,
            gradients,
            trace.remove_bg,
        );
        let svg = trace_cached(bytes, &p, key, frame, &source)?;
        let last_step = step + 1 == ladder.len();
        // До последней ступени — без упрощения контуров ядром: оно —
        // последнее средство.
        let core_options = if auto && !last_step {
            Options {
                optimization: Optimization::Manual(2),
                ..options.clone()
            }
        } else {
            options.clone()
        };
        let tiles =
            svg2tgs_core::convert_grid_limited(svg.text.as_bytes(), &core_options, max_bytes);
        // Пустая трассировка (всё прозрачно) — дальше не станет лучше.
        let tiles = tiles?;
        let fits = tiles.tiles.iter().all(|c| {
            !c.warnings.iter().any(|w| {
                matches!(
                    w.code,
                    WarningCode::SizeOverLimit { .. } | WarningCode::TooComplex { .. }
                )
            })
        });
        let info = TraceInfo {
            preset,
            auto: trace.preset == TracePreset::Auto,
            width: w,
            height: h,
            traced_width: key.tw,
            traced_height: key.th,
            colors: colors_now,
            details: details_now,
            colors_min: params.colors.0,
            colors_max: params.colors.1,
            gradients,
            gradient_count: svg.gradients,
            block: p.blocks.map_or(1, |b| b.n),
            background_removed: p.bg_removed,
        };
        last = Some((tiles, info, svg, step));
        if fits {
            break;
        }
    }
    let (mut tiles, info, svg, step) = last.expect("хотя бы одна ступень");
    let mut notes = Vec::new();
    if info.auto {
        notes.push(WarningCode::TracePresetChosen {
            preset: match preset {
                TracePreset::Pixel => "pixel",
                TracePreset::Photo => "photo",
                _ => "drawing",
            }
            .into(),
        });
    }
    if preset == TracePreset::Photo {
        notes.push(WarningCode::PhotoPoster);
    }
    if info.gradient_count > 0 {
        notes.push(WarningCode::GradientsApplied {
            count: info.gradient_count,
        });
    }
    if step > 0 && (info.colors != colors || info.details != details) {
        notes.push(WarningCode::TraceReduced {
            colors: info.colors,
            requested_colors: colors,
            details: info.details,
            requested_details: details,
        });
    }
    if trace.remove_bg && !p.bg_removed {
        notes.push(WarningCode::BackgroundNotFound);
    }
    for tile in &mut tiles.tiles {
        add_notes(tile, &notes);
    }
    Ok(Traced {
        tiles,
        info,
        svg: svg.text.clone(),
    })
}

fn add_notes(tile: &mut Conversion, notes: &[WarningCode]) {
    let mut front: Vec<Warning> = notes
        .iter()
        .map(|code| Warning {
            code: code.clone(),
            element_id: None,
        })
        .collect();
    front.append(&mut tile.warnings);
    tile.warnings = front;
}

/// Картинка трассировки и её размещение в документе.
enum Source {
    /// Уменьшение по площади до `tw × th`.
    Area,
    /// Блоки пиксель-арта.
    Blocks(Blocks),
}

#[allow(clippy::too_many_arguments)]
fn plan(
    p: &Prepared,
    preset: TracePreset,
    canvas_scale: f64,
    k: f64,
    colors: u32,
    details: u32,
    gradients: bool,
    remove_bg: bool,
) -> (TraceKey, vectorize::Frame, Source) {
    let (w, h) = (p.img.w, p.img.h);
    let blocks = (preset == TracePreset::Pixel).then_some(p.blocks).flatten();
    let (tw, th, source, view) = match blocks {
        Some(b) => {
            let tw = ((w - b.ox) / b.n).max(1);
            let th = ((h - b.oy) / b.n).max(1);
            let n = b.n as f64;
            let view = [
                -(b.ox as f64) / n,
                -(b.oy as f64) / n,
                w as f64 / n,
                h as f64 / n,
            ];
            (tw, th, Source::Blocks(b), view)
        }
        None => {
            // Пикселей трассировки на пиксель картинки.
            let mut s = (k * canvas_scale).min(1.0);
            // Ступенями через √2: ползунки размещения не запускают
            // трассировку на каждом шаге.
            s = 2f64.powf((s.log2() * 2.0).round() / 2.0).min(1.0);
            let long = w.max(h) as f64;
            s = s.max((MIN_TRACE_SIDE / long).min(1.0));
            let px = w as f64 * h as f64 * s * s;
            if px > MAX_TRACE_PIXELS as f64 {
                s *= (MAX_TRACE_PIXELS as f64 / px).sqrt();
            }
            let tw = ((w as f64 * s).round() as u32).max(1);
            let th = ((h as f64 * s).round() as u32).max(1);
            (tw, th, Source::Area, [0.0, 0.0, tw as f64, th as f64])
        }
    };
    // Пикселей трассировки на пиксель канваса.
    let t2c = (tw as f64 / w as f64 / canvas_scale.max(1e-9)) as f32;
    let side = speckle_side(preset, details);
    let speckle = if preset == TracePreset::Pixel {
        side
    } else {
        side * t2c
    };
    let key = TraceKey {
        preset,
        tw,
        th,
        colors,
        speckle,
        gradients,
        t2c,
        remove_bg,
    };
    let frame = vectorize::Frame {
        view,
        width: w,
        height: h,
    };
    (key, frame, source)
}

fn trace_cached(
    bytes: &[u8],
    p: &Prepared,
    key: TraceKey,
    frame: vectorize::Frame,
    source: &Source,
) -> Result<Arc<vectorize::Svg>, Error> {
    let ck = (hash_of(bytes), key);
    if let Some(s) = TRACED.lock().unwrap_or_else(|e| e.into_inner()).get(&ck) {
        return Ok(s);
    }
    let work = match source {
        Source::Blocks(b) => p.img.downscale_blocks(b.n, b.ox, b.oy),
        Source::Area => p.img.resize_area(key.tw, key.th),
    };
    let params = key.preset.params();
    let smooth = if params.blur > 0 || p.format == Format::Jpeg {
        work.blur(params.blur.max(1))
    } else {
        work.clone()
    };
    // Фото и JPEG разбиваются на цвета по сглаженной картинке (шум),
    // рисунки — по точной.
    let base = if params.blur > 0 { &smooth } else { &work };
    let q = quantize::quantize(base, key.colors, ALPHA_MIN);
    let grads = if key.gradients {
        gradient::find(&smooth, &q, key.t2c)
    } else {
        gradient::Gradients::default()
    };
    let mode = match params.mode {
        PathMode::Pixel => PathSimplifyMode::None,
        PathMode::Spline => PathSimplifyMode::Spline,
    };
    let area = (key.speckle * key.speckle).round().max(1.0) as usize;
    let vp = vectorize::Params {
        mode,
        speckle_area: if area <= 1 { 0 } else { area },
        corner_threshold: (params.corner_threshold as f64).to_radians(),
        length_threshold: (params.length_threshold * key.t2c).max(1.0) as f64,
        splice_threshold: (params.splice_threshold as f64).to_radians(),
    };
    let svg = Arc::new(vectorize::vectorize(work.w, work.h, &q, &grads, vp, frame));
    TRACED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(ck, svg.clone());
    Ok(svg)
}

/// Base64 без переносов строк.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Любой вход: растр трассируется, SVG идёт в ядро как есть.
pub fn convert_any(
    bytes: &[u8],
    options: &Options,
    max_bytes: usize,
) -> Result<(Tiles, Option<TraceInfo>), Error> {
    if is_raster(bytes) {
        let t = convert_grid_limited(bytes, options, max_bytes)?;
        Ok((t.tiles, Some(t.info)))
    } else {
        Ok((
            svg2tgs_core::convert_grid_limited(bytes, options, max_bytes)?,
            None,
        ))
    }
}

/// SVG для превью и кадрирования: у растра — [`preview_svg`], у SVG — он сам.
pub fn source_svg<'a>(
    bytes: &'a [u8],
    options: &Options,
) -> Result<std::borrow::Cow<'a, [u8]>, Error> {
    if is_raster(bytes) {
        Ok(std::borrow::Cow::Owned(preview_svg(bytes, options)?))
    } else {
        Ok(std::borrow::Cow::Borrowed(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(img: &Rgba) -> Vec<u8> {
        img.to_png()
    }

    /// Логотип: два цветных круга на прозрачном фоне со сглаженным краем.
    fn logo(side: u32) -> Rgba {
        let mut px = Vec::new();
        let s = side as f32;
        for y in 0..side {
            for x in 0..side {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let d1 = ((fx - 0.35 * s).powi(2) + (fy - 0.5 * s).powi(2)).sqrt() - 0.25 * s;
                let d2 = ((fx - 0.68 * s).powi(2) + (fy - 0.5 * s).powi(2)).sqrt() - 0.2 * s;
                let a1 = (0.5 - d1).clamp(0.0, 1.0);
                let a2 = (0.5 - d2).clamp(0.0, 1.0);
                px.push(if a2 > 0.0 {
                    [30, 90, 220, (a2.max(a1) * 255.0) as u8]
                } else {
                    [230, 40, 40, (a1 * 255.0) as u8]
                });
            }
        }
        Rgba::new(side, side, px)
    }

    #[test]
    fn traces_logo_into_small_tgs() {
        let bytes = png(&logo(400));
        let t = convert_grid(&bytes, &Options::default()).unwrap();
        assert_eq!(t.info.preset, TracePreset::Drawing);
        assert!(t.info.auto);
        assert!(t.tiles.tiles[0].tgs.len() < 64 * 1024);
        assert!(
            t.svg.contains("#e62828") || t.svg.contains("#e6"),
            "{}",
            &t.svg[..200]
        );
        assert!(t.tiles.tiles[0]
            .warnings
            .iter()
            .any(|w| matches!(w.code, WarningCode::TracePresetChosen { .. })));
        // Тот же результат повторно (кэш и постоянный seed).
        let again = convert_grid(&bytes, &Options::default()).unwrap();
        assert_eq!(again.tiles.tiles[0].tgs, t.tiles.tiles[0].tgs);
    }

    #[test]
    fn working_scale_follows_canvas_size() {
        // Картинка 2000 px в стикер 512 при k = 1 — трассировка около 512.
        let bytes = png(&logo(2000));
        let t = convert_grid(&bytes, &Options::default()).unwrap();
        assert!(
            (360..=724).contains(&t.info.traced_width),
            "{}",
            t.info.traced_width
        );
        // С отступом 20 % — меньше.
        let padded = Options {
            padding: 0.2,
            ..Options::default()
        };
        let p = convert_grid(&bytes, &padded).unwrap();
        assert!(p.info.traced_width < t.info.traced_width);
    }

    #[test]
    fn pixel_art_keeps_blocks() {
        let cells: Vec<[u8; 4]> = (0..16 * 16)
            .map(|i| match (i * 5 + i / 16) % 3 {
                0 => [255, 200, 0, 255],
                1 => [0, 0, 0, 0],
                _ => [40, 40, 160, 255],
            })
            .collect();
        let mut px = Vec::new();
        for y in 0..128 {
            for x in 0..128 {
                px.push(cells[(y / 8) * 16 + x / 8]);
            }
        }
        let bytes = png(&Rgba::new(128, 128, px));
        let t = convert_grid(&bytes, &Options::default()).unwrap();
        assert_eq!(t.info.preset, TracePreset::Pixel);
        assert_eq!(t.info.block, 8);
        assert_eq!((t.info.traced_width, t.info.traced_height), (16, 16));
    }

    #[test]
    fn preview_is_svg_with_image() {
        let bytes = png(&logo(64));
        let svg = preview_svg(&bytes, &Options::default()).unwrap();
        let text = String::from_utf8(svg).unwrap();
        assert!(text.contains("data:image/png;base64,"));
        assert!(text.contains(r#"width="64""#));
        assert!(svg2tgs_core::parse_svg(text.as_bytes()).is_ok());
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn empty_picture_is_an_error() {
        let bytes = png(&Rgba::new(8, 8, vec![[0, 0, 0, 0]; 64]));
        assert!(matches!(
            convert_grid(&bytes, &Options::default()),
            Err(Error::Empty)
        ));
    }
}

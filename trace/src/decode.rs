//! Чтение PNG, JPEG и WebP декодерами на чистом Rust (`png`, `zune-jpeg`,
//! `image-webp`) — тех же версий, что у resvg. Анимированные картинки
//! не переводим: первый кадр без остальных вводил бы в заблуждение.

use std::io::Cursor;

use svg2tgs_core::Error;

use crate::image::Rgba;

/// Наибольшая сторона картинки: больше — почти наверняка ошибка, а память
/// под такую картинку (особенно в браузере) не стоит того.
pub const MAX_SIDE: u32 = 16_384;
/// Наибольшее число пикселей картинки.
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;

/// Формат по сигнатуре файла.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Gif,
}

/// Формат растра по первым байтам; `None` — не растр (SVG или что-то ещё).
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Format::Webp)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else {
        None
    }
}

fn bad(e: impl std::fmt::Display) -> Error {
    Error::InvalidImage(e.to_string())
}

fn check_size(w: u32, h: u32) -> Result<(), Error> {
    if w == 0 || h == 0 {
        return Err(bad("пустая картинка"));
    }
    if w > MAX_SIDE || h > MAX_SIDE || w as u64 * h as u64 > MAX_PIXELS {
        return Err(Error::ImageTooLarge(w, h));
    }
    Ok(())
}

/// Читает картинку в RGBA (без премультипликации).
pub fn decode(bytes: &[u8]) -> Result<Rgba, Error> {
    match sniff(bytes) {
        Some(Format::Png) => png(bytes),
        Some(Format::Jpeg) => jpeg(bytes),
        Some(Format::Webp) => webp(bytes),
        // GIF почти всегда — анимация или палитра под пиксель-арт; формат
        // не заявлен, читать его нечем (декодер не тянем).
        Some(Format::Gif) => Err(bad("GIF не поддерживается")),
        None => Err(bad("не PNG, JPEG или WebP")),
    }
}

fn png(bytes: &[u8]) -> Result<Rgba, Error> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(bad)?;
    let info = reader.info();
    if info.animation_control.is_some_and(|a| a.num_frames > 1) {
        return Err(Error::AnimatedImage);
    }
    check_size(info.width, info.height)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| bad("размер буфера"))?;
    let mut buf = vec![0; size];
    let frame = reader.next_frame(&mut buf).map_err(bad)?;
    let (w, h) = (frame.width, frame.height);
    let data = &buf[..frame.buffer_size()];
    let px: Vec<[u8; 4]> = match frame.color_type {
        png::ColorType::Rgba => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(bad("палитра не развёрнута")),
    };
    Ok(Rgba::new(w, h, px))
}

fn jpeg(bytes: &[u8]) -> Result<Rgba, Error> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(MAX_SIDE as usize)
        .set_max_height(MAX_SIDE as usize);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder
        .decode_headers()
        .map_err(|e| bad(format!("{e:?}")))?;
    let info = decoder.info().ok_or_else(|| bad("нет заголовка"))?;
    let (w, h) = (info.width as u32, info.height as u32);
    check_size(w, h)?;
    let orientation = decoder.exif().map_or(1, |e| exif_orientation(e));
    let data = decoder.decode().map_err(|e| bad(format!("{e:?}")))?;
    let px: Vec<[u8; 4]> = data
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2], 255])
        .collect();
    if px.len() != (w * h) as usize {
        return Err(bad("размер данных"));
    }
    Ok(Rgba::new(w, h, px).oriented(orientation))
}

fn webp(bytes: &[u8]) -> Result<Rgba, Error> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(bad)?;
    if decoder.is_animated() {
        return Err(Error::AnimatedImage);
    }
    let (w, h) = decoder.dimensions();
    check_size(w, h)?;
    let size = decoder
        .output_buffer_size()
        .ok_or_else(|| bad("размер буфера"))?;
    let mut buf = vec![0; size];
    decoder.read_image(&mut buf).map_err(bad)?;
    let px: Vec<[u8; 4]> = if decoder.has_alpha() {
        buf.as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect()
    } else {
        buf.as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2], 255])
            .collect()
    };
    Ok(Rgba::new(w, h, px))
}

/// Поворот снимка из EXIF (тег 0x0112): 1 — как есть, 2–8 — отражения
/// и повороты. Телефоны пишут фото «боком» и ставят этот тег.
fn exif_orientation(exif: &[u8]) -> u16 {
    // Данные APP1 могут начинаться с «Exif\0\0».
    let tiff = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    if tiff.len() < 8 {
        return 1;
    }
    let le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return 1,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b = tiff.get(i..i + 2)?;
        Some(if le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b = tiff.get(i..i + 4)?;
        Some(if le {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    };
    let Some(ifd) = u32_at(4) else { return 1 };
    let ifd = ifd as usize;
    let Some(count) = u16_at(ifd) else { return 1 };
    for k in 0..count as usize {
        let e = ifd + 2 + k * 12;
        if u16_at(e) == Some(0x0112) {
            return u16_at(e + 8).filter(|v| (1..=8).contains(v)).unwrap_or(1);
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_formats() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nxxxx"), Some(Format::Png));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Format::Jpeg));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::Webp));
        assert_eq!(sniff(b"<svg"), None);
        assert!(matches!(decode(b"<svg/>"), Err(Error::InvalidImage(_))));
    }

    #[test]
    fn reads_exif_orientation() {
        // II, 42, смещение IFD 8, одна запись: 0x0112, SHORT, 1, значение 6.
        let mut t = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0".to_vec();
        t.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0]);
        assert_eq!(exif_orientation(&t), 6);
        assert_eq!(exif_orientation(b"junk"), 1);
    }
}

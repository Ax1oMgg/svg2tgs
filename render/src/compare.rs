//! Сравнение картинок: эталон (SVG) против результата (TGS).

use crate::Image;

/// Насколько две картинки различаются.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diff {
    /// Средняя разница каналов, 0–255. Ниже 1 — на глаз не отличить.
    pub mean: f64,
    /// Доля пикселей, отличающихся заметно (больше 64 по любому каналу).
    /// Ловит пропавшие мелкие детали, которые почти не влияют на среднее.
    pub bad_ratio: f64,
}

const BAD_THRESHOLD: i32 = 64;

/// Сравнивает картинки одного размера. Каждая накладывается на белый
/// и на чёрный фон: так учитываются и цвет, и прозрачность.
pub fn compare(a: &Image, b: &Image) -> Diff {
    assert_eq!(
        (a.width, a.height),
        (b.width, b.height),
        "размеры различаются"
    );
    let mut sum = 0u64;
    let mut bad = 0u64;
    for (pa, pb) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)) {
        let mut worst = 0i32;
        for bg in [255i32, 0] {
            for c in 0..3 {
                let va = over(pa[c], pa[3], bg);
                let vb = over(pb[c], pb[3], bg);
                let d = (va - vb).abs();
                sum += d as u64;
                worst = worst.max(d);
            }
        }
        if worst > BAD_THRESHOLD {
            bad += 1;
        }
    }
    let pixels = (a.width * a.height) as f64;
    Diff {
        mean: sum as f64 / (pixels * 6.0),
        bad_ratio: bad as f64 / pixels,
    }
}

/// Цвет канала поверх фона `bg` (0–255).
fn over(c: u8, a: u8, bg: i32) -> i32 {
    let (c, a) = (c as i32, a as i32);
    (c * a + bg * (255 - a) + 127) / 255
}

/// Картинка для разбора: эталон, результат и карта различий
/// (красным — заметные отличия) на шахматном фоне.
pub fn diff_sheet(a: &Image, b: &Image) -> Image {
    let (w, h) = (a.width, a.height);
    let mut out = Image::new(w * 3, h);
    let checker = |x: u32, y: u32| {
        if ((x / 16) + (y / 16)).is_multiple_of(2) {
            255
        } else {
            225
        }
    };
    for y in 0..h {
        for x in 0..w {
            let bg = checker(x, y);
            let pa = a.pixel(x, y);
            let pb = b.pixel(x, y);
            let flat = |p: [u8; 4]| [0, 1, 2].map(|c| over(p[c], p[3], bg) as u8);
            let ca = flat(pa);
            let cb = flat(pb);
            let d = (0..3)
                .map(|c| (ca[c] as i32 - cb[c] as i32).abs())
                .max()
                .unwrap();
            let gray = (ca.iter().map(|&v| v as u32).sum::<u32>() / 3) as u8 / 3 + 170;
            let cd = if d > BAD_THRESHOLD {
                [230, 30, 30]
            } else if d > 16 {
                [240, 160, 40]
            } else {
                [gray, gray, gray]
            };
            for (k, c) in [ca, cb, cd].into_iter().enumerate() {
                let i = ((y * out.width + x + k as u32 * w) * 4) as usize;
                out.rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(c: [u8; 4]) -> Image {
        let mut img = Image::new(4, 4);
        for p in img.rgba.chunks_exact_mut(4) {
            p.copy_from_slice(&c);
        }
        img
    }

    #[test]
    fn identical_images_have_no_difference() {
        let a = solid([10, 20, 30, 200]);
        assert_eq!(
            compare(&a, &a),
            Diff {
                mean: 0.0,
                bad_ratio: 0.0
            }
        );
    }

    #[test]
    fn invisible_color_does_not_matter() {
        // Полностью прозрачные пиксели равны, какой бы цвет в них ни лежал.
        let d = compare(&solid([255, 0, 0, 0]), &solid([0, 0, 255, 0]));
        assert_eq!(d.mean, 0.0);
    }

    #[test]
    fn missing_shape_is_bad() {
        let d = compare(&solid([0, 0, 0, 255]), &solid([0, 0, 0, 0]));
        assert_eq!(d.bad_ratio, 1.0);
        assert!(d.mean > 100.0);
    }
}

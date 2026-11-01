//! Картинка RGBA и операции над ней: поворот по EXIF, уменьшение,
//! сглаживание, рамка непрозрачного, кодирование в PNG, цвет в OKLab.

/// Картинка: пиксели RGBA по строкам, без премультипликации.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<[u8; 4]>,
}

impl Rgba {
    pub fn new(w: u32, h: u32, px: Vec<[u8; 4]>) -> Self {
        debug_assert_eq!(px.len(), (w * h) as usize);
        Self { w, h, px }
    }

    #[inline]
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        self.px[(y * self.w + x) as usize]
    }

    /// Поворот и отражение по тегу EXIF Orientation (1–8).
    pub fn oriented(self, orientation: u16) -> Self {
        if orientation <= 1 || orientation > 8 {
            return self;
        }
        let (w, h) = (self.w, self.h);
        let swap = orientation >= 5;
        let (nw, nh) = if swap { (h, w) } else { (w, h) };
        let mut px = vec![[0; 4]; self.px.len()];
        for y in 0..nh {
            for x in 0..nw {
                // Откуда брать пиксель (x, y) результата.
                let (sx, sy) = match orientation {
                    2 => (w - 1 - x, y),
                    3 => (w - 1 - x, h - 1 - y),
                    4 => (x, h - 1 - y),
                    5 => (y, x),
                    6 => (y, h - 1 - x),
                    7 => (w - 1 - y, h - 1 - x),
                    8 => (w - 1 - y, x),
                    _ => (x, y),
                };
                px[(y * nw + x) as usize] = self.at(sx, sy);
            }
        }
        Self::new(nw, nh, px)
    }

    /// Уменьшение до `nw × nh` усреднением по площади (премультипликация,
    /// чтобы прозрачные пиксели не темнили края).
    pub fn resize_area(&self, nw: u32, nh: u32) -> Self {
        let (nw, nh) = (nw.max(1), nh.max(1));
        if nw == self.w && nh == self.h {
            return self.clone();
        }
        let wx = weights(self.w, nw);
        let wy = weights(self.h, nh);
        // Сначала по строкам: w → nw.
        let mut rows = vec![[0f32; 4]; (nw * self.h) as usize];
        for y in 0..self.h {
            for (x, taps) in wx.iter().enumerate() {
                let mut acc = [0f32; 4];
                for &(sx, k) in taps {
                    let p = self.at(sx, y);
                    let a = p[3] as f32 * k;
                    acc[0] += p[0] as f32 * a;
                    acc[1] += p[1] as f32 * a;
                    acc[2] += p[2] as f32 * a;
                    acc[3] += a;
                }
                rows[(y * nw) as usize + x] = acc;
            }
        }
        let mut px = vec![[0u8; 4]; (nw * nh) as usize];
        for (y, taps) in wy.iter().enumerate() {
            for x in 0..nw as usize {
                let mut acc = [0f32; 4];
                for &(sy, k) in taps {
                    let r = rows[sy as usize * nw as usize + x];
                    for c in 0..4 {
                        acc[c] += r[c] * k;
                    }
                }
                px[y * nw as usize + x] = unpremultiply(acc);
            }
        }
        Self::new(nw, nh, px)
    }

    /// Уменьшение пиксель-арта в `n` раз: из каждого блока `n × n`
    /// (сетка со сдвигом `ox, oy`) берётся средний пиксель блока.
    pub fn downscale_blocks(&self, n: u32, ox: u32, oy: u32) -> Self {
        let n = n.max(1);
        let nw = ((self.w - ox) / n).max(1);
        let nh = ((self.h - oy) / n).max(1);
        let mut px = Vec::with_capacity((nw * nh) as usize);
        for by in 0..nh {
            for bx in 0..nw {
                let x = (ox + bx * n + n / 2).min(self.w - 1);
                let y = (oy + by * n + n / 2).min(self.h - 1);
                px.push(self.at(x, y));
            }
        }
        Self::new(nw, nh, px)
    }

    /// Сглаживание квадратным окном радиуса `r` (два прохода — почти
    /// гауссово). Премультипликация, как при уменьшении.
    pub fn blur(&self, r: u32) -> Self {
        if r == 0 {
            return self.clone();
        }
        let mut cur: Vec<[f32; 4]> = self
            .px
            .iter()
            .map(|p| {
                let a = p[3] as f32;
                [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, a]
            })
            .collect();
        for _ in 0..2 {
            cur = box_pass(&cur, self.w, self.h, r, true);
            cur = box_pass(&cur, self.w, self.h, r, false);
        }
        let px = cur
            .into_iter()
            .map(|p| unpremultiply([p[0], p[1], p[2], p[3]]))
            .collect();
        Self::new(self.w, self.h, px)
    }

    /// Рамка пикселей с непрозрачностью не меньше `alpha` — `[x0, y0, x1, y1)`.
    pub fn opaque_bounds(&self, alpha: u8) -> Option<[u32; 4]> {
        let mut b = [u32::MAX, u32::MAX, 0, 0];
        for y in 0..self.h {
            for x in 0..self.w {
                if self.at(x, y)[3] >= alpha {
                    b[0] = b[0].min(x);
                    b[1] = b[1].min(y);
                    b[2] = b[2].max(x + 1);
                    b[3] = b[3].max(y + 1);
                }
            }
        }
        (b[0] < b[2]).then_some(b)
    }

    /// Часть картинки `[x0, y0, x1, y1)`.
    pub fn crop(&self, r: [u32; 4]) -> Self {
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let mut px = Vec::with_capacity((w * h) as usize);
        for y in r[1]..r[3] {
            let s = (y * self.w + r[0]) as usize;
            px.extend_from_slice(&self.px[s..s + w as usize]);
        }
        Self::new(w, h, px)
    }

    /// PNG без сжатия палитрой — для превью исходника.
    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.w, self.h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            let mut writer = enc.write_header().expect("PNG в память");
            let data: Vec<u8> = self.px.iter().flatten().copied().collect();
            writer.write_image_data(&data).expect("PNG в память");
        }
        out
    }
}

/// Веса усреднения по площади: для каждого пикселя результата — какие
/// пиксели исходника и с какой долей в него входят.
fn weights(from: u32, to: u32) -> Vec<Vec<(u32, f32)>> {
    let f = from as f64 / to as f64;
    (0..to)
        .map(|i| {
            let a = i as f64 * f;
            let b = ((i + 1) as f64 * f).min(from as f64);
            let mut taps = Vec::new();
            let mut s = a.floor() as u32;
            while (s as f64) < b && s < from {
                let lo = a.max(s as f64);
                let hi = b.min(s as f64 + 1.0);
                if hi > lo {
                    taps.push((s, ((hi - lo) / (b - a)) as f32));
                }
                s += 1;
            }
            if taps.is_empty() {
                taps.push(((a as u32).min(from - 1), 1.0));
            }
            taps
        })
        .collect()
}

fn unpremultiply(p: [f32; 4]) -> [u8; 4] {
    let a = p[3];
    if a <= 1e-3 {
        return [0, 0, 0, 0];
    }
    let c = |v: f32| (v / a).round().clamp(0.0, 255.0) as u8;
    [c(p[0]), c(p[1]), c(p[2]), a.round().clamp(0.0, 255.0) as u8]
}

/// Один проход квадратного окна вдоль строк (`horizontal`) или столбцов.
fn box_pass(src: &[[f32; 4]], w: u32, h: u32, r: u32, horizontal: bool) -> Vec<[f32; 4]> {
    let (w, h, r) = (w as i64, h as i64, r as i64);
    let mut out = vec![[0f32; 4]; src.len()];
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let idx = |line: i64, i: i64| -> usize {
        if horizontal {
            (line * w + i) as usize
        } else {
            (i * w + line) as usize
        }
    };
    for line in 0..lines {
        let mut acc = [0f32; 4];
        let mut n = 0f32;
        for i in -r..=r {
            if (0..len).contains(&i) {
                let p = src[idx(line, i)];
                for c in 0..4 {
                    acc[c] += p[c];
                }
                n += 1.0;
            }
        }
        for i in 0..len {
            out[idx(line, i)] = [acc[0] / n, acc[1] / n, acc[2] / n, acc[3] / n];
            let drop = i - r;
            let add = i + r + 1;
            if drop >= 0 {
                let p = src[idx(line, drop)];
                for c in 0..4 {
                    acc[c] -= p[c];
                }
                n -= 1.0;
            }
            if add < len {
                let p = src[idx(line, add)];
                for c in 0..4 {
                    acc[c] += p[c];
                }
                n += 1.0;
            }
        }
    }
    out
}

/// sRGB (0–255) в линейный свет (0–1).
#[inline]
pub fn to_linear(c: u8) -> f32 {
    LINEAR.with(|t| t[c as usize])
}

thread_local! {
    static LINEAR: [f32; 256] = {
        let mut t = [0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
        }
        t
    };
}

/// Цвет в OKLab (L 0–1, a и b около ±0,4) — пространство, где расстояние
/// близко к заметности разницы на глаз.
pub fn oklab(p: [u8; 4]) -> [f32; 3] {
    let (r, g, b) = (to_linear(p[0]), to_linear(p[1]), to_linear(p[2]));
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Точка для сравнения цветов: OKLab плюс непрозрачность (с весом:
/// разница непрозрачности 1 ≈ разнице от чёрного до белого).
#[inline]
pub fn lab_alpha(p: [u8; 4]) -> [f32; 4] {
    let l = oklab(p);
    [l[0], l[1], l[2], p[3] as f32 / 255.0]
}

#[inline]
pub fn dist2(a: [f32; 4], b: [f32; 4]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] + d[3] * d[3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_resize_keeps_flat_color_and_edges() {
        let img = Rgba::new(4, 4, vec![[200, 100, 50, 255]; 16]);
        let small = img.resize_area(2, 2);
        assert!(small.px.iter().all(|&p| p == [200, 100, 50, 255]));
        // Прозрачная половина не темнит цвет края.
        let mut px = vec![[0, 0, 0, 0]; 4];
        px[0] = [255, 0, 0, 255];
        let half = Rgba::new(2, 2, px).resize_area(1, 1);
        assert_eq!(&half.px[0][..3], &[255, 0, 0]);
        assert!((60..=70).contains(&half.px[0][3]));
    }

    #[test]
    fn orientation_rotates() {
        let img = Rgba::new(2, 1, vec![[1, 0, 0, 255], [2, 0, 0, 255]]);
        let r = img.clone().oriented(6);
        assert_eq!((r.w, r.h), (1, 2));
        assert_eq!(r.px[0][0], 1);
        let r = img.oriented(8);
        assert_eq!(r.px[0][0], 2);
    }

    #[test]
    fn oklab_white_and_black() {
        let w = oklab([255, 255, 255, 255]);
        let k = oklab([0, 0, 0, 255]);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-3);
        assert!(k[0].abs() < 1e-3);
    }

    #[test]
    fn png_roundtrip() {
        let img = Rgba::new(3, 2, (0..6).map(|i| [i * 40, 0, 255, 255 - i]).collect());
        let back = crate::decode::decode(&img.to_png()).unwrap();
        assert_eq!(back, img);
    }
}

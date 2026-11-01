//! Число цветов («Цвета»): k-средних в OKLab с непрозрачностью.
//! Случайность — с постоянным seed, результат повторяется в тестах,
//! программе и веб-версии.

use std::collections::HashMap;

use crate::image::{dist2, lab_alpha, Rgba};

/// Метка прозрачного пикселя.
pub const CLEAR: u16 = u16::MAX;

/// Сколько пикселей берётся для подбора палитры.
const SAMPLE: usize = 65_536;
const ITERATIONS: usize = 12;
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// Палитра и метки пикселей.
#[derive(Debug, Clone)]
pub struct Quantized {
    /// Цвет каждой метки (средний цвет её пикселей).
    pub colors: Vec<[u8; 4]>,
    /// Метка каждого пикселя; [`CLEAR`] — прозрачный.
    pub labels: Vec<u16>,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[inline]
fn pack(p: [u8; 4]) -> u32 {
    u32::from_le_bytes(p)
}

/// Делит пиксели с непрозрачностью не меньше `alpha_min` на `n` цветов.
pub fn quantize(img: &Rgba, n: u32, alpha_min: u8) -> Quantized {
    let n = n.clamp(1, 255) as usize;
    let visible = |p: &[u8; 4]| p[3] >= alpha_min;

    // Если разных цветов не больше n — палитра точная.
    let mut exact: HashMap<u32, u16> = HashMap::new();
    let mut exact_ok = true;
    for p in img.px.iter().filter(|p| visible(p)) {
        let len = exact.len();
        exact.entry(pack(*p)).or_insert(len as u16);
        if exact.len() > n {
            exact_ok = false;
            break;
        }
    }
    if exact_ok {
        let mut colors = vec![[0u8; 4]; exact.len()];
        for (&k, &i) in &exact {
            colors[i as usize] = k.to_le_bytes();
        }
        let labels = img
            .px
            .iter()
            .map(|p| if visible(p) { exact[&pack(*p)] } else { CLEAR })
            .collect();
        return Quantized { colors, labels };
    }

    // Выборка пикселей равномерным шагом.
    let vis: Vec<[u8; 4]> = img.px.iter().copied().filter(visible).collect();
    let step = vis.len().div_ceil(SAMPLE).max(1);
    let sample: Vec<[f32; 4]> = vis.iter().step_by(step).map(|&p| lab_alpha(p)).collect();

    // k-means++.
    let mut rng = Rng(SEED);
    let mut centers: Vec<[f32; 4]> = vec![sample[(rng.next() as usize) % sample.len()]];
    let mut d2: Vec<f32> = sample.iter().map(|s| dist2(*s, centers[0])).collect();
    while centers.len() < n {
        let total: f64 = d2.iter().map(|&d| d as f64).sum();
        if total <= 0.0 {
            break;
        }
        let mut r = rng.unit() * total;
        let mut pick = sample.len() - 1;
        for (i, &d) in d2.iter().enumerate() {
            r -= d as f64;
            if r <= 0.0 {
                pick = i;
                break;
            }
        }
        let c = sample[pick];
        centers.push(c);
        for (i, s) in sample.iter().enumerate() {
            d2[i] = d2[i].min(dist2(*s, c));
        }
    }
    let nearest = |c: &[[f32; 4]], p: [f32; 4]| -> usize {
        let mut best = (0, f32::MAX);
        for (i, q) in c.iter().enumerate() {
            let d = dist2(p, *q);
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    };
    for _ in 0..ITERATIONS {
        let mut sum = vec![[0f64; 4]; centers.len()];
        let mut cnt = vec![0usize; centers.len()];
        for s in &sample {
            let k = nearest(&centers, *s);
            for c in 0..4 {
                sum[k][c] += s[c] as f64;
            }
            cnt[k] += 1;
        }
        let mut moved = 0f32;
        for k in 0..centers.len() {
            if cnt[k] > 0 {
                let m = [
                    (sum[k][0] / cnt[k] as f64) as f32,
                    (sum[k][1] / cnt[k] as f64) as f32,
                    (sum[k][2] / cnt[k] as f64) as f32,
                    (sum[k][3] / cnt[k] as f64) as f32,
                ];
                moved = moved.max(dist2(m, centers[k]));
                centers[k] = m;
            }
        }
        if moved < 1e-7 {
            break;
        }
    }

    // Метки всех пикселей; цвет метки — среднее её пикселей в sRGB.
    let mut cache: HashMap<u32, u16> = HashMap::new();
    let mut labels = Vec::with_capacity(img.px.len());
    let mut sum = vec![[0u64; 4]; centers.len()];
    let mut cnt = vec![0u64; centers.len()];
    for p in &img.px {
        if !visible(p) {
            labels.push(CLEAR);
            continue;
        }
        let k = *cache
            .entry(pack(*p))
            .or_insert_with(|| nearest(&centers, lab_alpha(*p)) as u16);
        labels.push(k);
        let a = p[3] as u64;
        sum[k as usize][0] += p[0] as u64 * a;
        sum[k as usize][1] += p[1] as u64 * a;
        sum[k as usize][2] += p[2] as u64 * a;
        sum[k as usize][3] += a;
        cnt[k as usize] += 1;
    }
    let colors = (0..centers.len())
        .map(|k| {
            let a = sum[k][3];
            if a == 0 {
                return [0, 0, 0, 0];
            }
            [
                (sum[k][0] / a) as u8,
                (sum[k][1] / a) as u8,
                (sum[k][2] / a) as u8,
                (a / cnt[k].max(1)) as u8,
            ]
        })
        .collect();
    Quantized { colors, labels }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_palette_when_few_colors() {
        let img = Rgba::new(
            2,
            2,
            vec![[1, 2, 3, 255], [9, 9, 9, 255], [1, 2, 3, 255], [0, 0, 0, 0]],
        );
        let q = quantize(&img, 8, 128);
        assert_eq!(q.colors.len(), 2);
        assert_eq!(q.labels[0], q.labels[2]);
        assert_eq!(q.labels[3], CLEAR);
    }

    #[test]
    fn repeats_and_respects_count() {
        let px: Vec<[u8; 4]> = (0..64 * 64)
            .map(|i| {
                [
                    (i % 256) as u8,
                    (i / 64 * 4) as u8,
                    (i * 7 % 256) as u8,
                    255,
                ]
            })
            .collect();
        let img = Rgba::new(64, 64, px);
        let a = quantize(&img, 6, 128);
        let b = quantize(&img, 6, 128);
        assert_eq!(a.colors.len(), 6);
        assert_eq!(a.labels, b.labels);
    }
}

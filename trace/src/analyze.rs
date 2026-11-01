//! Что за картинка: пиксель-арт (сетка блоков), фото (много цветов),
//! сплошной фон по углам («Убрать фон»).

use std::collections::{HashSet, VecDeque};

use crate::image::{dist2, lab_alpha, Rgba};

/// Перепад цвета между соседними пикселями, который считается границей
/// (сумма разниц каналов RGBA, 0–1020). JPEG «звенит» внутри блоков
/// на меньшие значения.
const EDGE: u32 = 48;
/// Доля границ, которые должны лечь на линии сетки блоков.
const ON_GRID: f64 = 0.97;
/// Доля пикселей блока, которые могут отличаться от его середины (JPEG).
const BLOCK_NOISE: f64 = 0.03;
/// Больше цветов — уже не пиксель-арт.
const PIXEL_MAX_COLORS: usize = 64;
/// Пиксель-арт в родном размере (блок 1 × 1) — только такой маленький.
const PIXEL_NATIVE_MAX_SIDE: u32 = 128;
/// Фото: не меньше стольких цветов (по 6 бит на канал)…
const PHOTO_MIN_COLORS: usize = 2000;
/// …и не меньше такой доли от числа непрозрачных пикселей.
const PHOTO_COLOR_SHARE: f64 = 0.05;

/// Сетка блоков пиксель-арта: шаг и сдвиг первой линии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blocks {
    pub n: u32,
    pub ox: u32,
    pub oy: u32,
}

fn differs(a: [u8; 4], b: [u8; 4]) -> bool {
    let d: u32 = (0..4).map(|c| a[c].abs_diff(b[c]) as u32).sum();
    d > EDGE
}

/// Ищет сетку блоков: наибольший шаг `n`, при котором почти все границы
/// цветов лежат на линиях сетки, блоки однотонны и цветов мало.
/// Родной размер (блок 1 × 1) — только для маленьких картинок без
/// полупрозрачных пикселей.
pub fn find_blocks(img: &Rgba) -> Option<Blocks> {
    let (w, h) = (img.w, img.h);
    if w < 4 || h < 4 {
        return None;
    }
    // Границы между столбцами x-1 | x и строками y-1 | y.
    let mut tx = vec![0u64; w as usize];
    let mut ty = vec![0u64; h as usize];
    for y in 0..h {
        for x in 0..w {
            let p = img.at(x, y);
            if x > 0 && differs(img.at(x - 1, y), p) {
                tx[x as usize] += 1;
            }
            if y > 0 && differs(img.at(x, y - 1), p) {
                ty[y as usize] += 1;
            }
        }
    }
    let total_x: u64 = tx.iter().sum();
    let total_y: u64 = ty.iter().sum();
    if total_x + total_y < 16 {
        return None;
    }
    let best_offset = |t: &[u64], total: u64, n: u32| -> Option<u32> {
        if total == 0 {
            return Some(0);
        }
        (0..n)
            .map(|o| {
                let on: u64 = t.iter().skip(o as usize).step_by(n as usize).sum();
                (o, on)
            })
            .max_by_key(|&(_, on)| on)
            .filter(|&(_, on)| on as f64 >= ON_GRID * total as f64)
            .map(|(o, _)| o)
    };
    let max_n = (w.min(h) / 8).min(64);
    for n in (2..=max_n).rev() {
        let (Some(ox), Some(oy)) = (best_offset(&tx, total_x, n), best_offset(&ty, total_y, n))
        else {
            continue;
        };
        let b = Blocks { n, ox, oy };
        if blocks_flat(img, b) && few_colors(&img.downscale_blocks(n, ox, oy)) {
            return Some(b);
        }
    }
    let native = w.max(h) <= PIXEL_NATIVE_MAX_SIDE
        && img.px.iter().filter(|p| p[3] != 0 && p[3] != 255).count() * 50 <= img.px.len()
        && few_colors(img);
    native.then_some(Blocks { n: 1, ox: 0, oy: 0 })
}

/// Блоки однотонны: почти все пиксели совпадают с серединой блока.
fn blocks_flat(img: &Rgba, b: Blocks) -> bool {
    let small = img.downscale_blocks(b.n, b.ox, b.oy);
    let mut bad = 0u64;
    let mut all = 0u64;
    for by in 0..small.h {
        for bx in 0..small.w {
            let c = small.at(bx, by);
            for dy in 0..b.n {
                for dx in 0..b.n {
                    let x = b.ox + bx * b.n + dx;
                    let y = b.oy + by * b.n + dy;
                    if x < img.w && y < img.h {
                        all += 1;
                        if differs(img.at(x, y), c) {
                            bad += 1;
                        }
                    }
                }
            }
        }
    }
    all > 0 && (bad as f64) <= BLOCK_NOISE * all as f64
}

/// Цветов (с непрозрачностью, по 5 бит на канал) не больше предела пиксель-арта.
fn few_colors(img: &Rgba) -> bool {
    let mut seen = HashSet::new();
    for p in &img.px {
        let key = if p[3] < 128 {
            u32::MAX
        } else {
            (p[0] as u32 >> 3) << 10 | (p[1] as u32 >> 3) << 5 | (p[2] as u32 >> 3)
        };
        seen.insert(key);
        if seen.len() > PIXEL_MAX_COLORS + 1 {
            return false;
        }
    }
    true
}

/// Фото — много разных цветов среди непрозрачных пикселей (уже уменьшенной
/// картинки: число цветов зависит от размера).
pub fn is_photo(img: &Rgba) -> bool {
    let mut seen = HashSet::new();
    let mut opaque = 0usize;
    for p in &img.px {
        if p[3] >= 128 {
            opaque += 1;
            seen.insert((p[0] as u32 >> 2) << 12 | (p[1] as u32 >> 2) << 6 | (p[2] as u32 >> 2));
        }
    }
    seen.len() >= PHOTO_MIN_COLORS && seen.len() as f64 >= PHOTO_COLOR_SHARE * opaque as f64
}

/// Насколько цвет фона может отличаться (квадрат расстояния OKLab):
/// края сжатого JPEG и лёгкая виньетка.
const BG_TOLERANCE: f32 = 0.06 * 0.06;

/// «Убрать фон»: если хотя бы три угла картинки одного непрозрачного цвета,
/// всё этого цвета, что связано с краем картинки, становится прозрачным;
/// пиксели на границе (сглаживание) — полупрозрачными. `false` — фона нет.
pub fn remove_background(img: &mut Rgba) -> bool {
    let (w, h) = (img.w, img.h);
    if w < 3 || h < 3 {
        return false;
    }
    let corner = |cx: u32, cy: u32| -> Option<[f32; 4]> {
        let p = img.at(cx, cy);
        (p[3] == 255).then(|| lab_alpha(p))
    };
    let corners: Vec<[f32; 4]> = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]
        .into_iter()
        .filter_map(|(x, y)| corner(x, y))
        .collect();
    // Цвет фона — угол, с которым согласны ещё хотя бы два.
    let Some(bg) = corners.iter().copied().find(|c| {
        corners
            .iter()
            .filter(|d| dist2(*c, **d) <= BG_TOLERANCE)
            .count()
            >= 3
    }) else {
        return false;
    };
    let near = |p: [u8; 4]| p[3] > 0 && dist2(lab_alpha(p), bg) <= BG_TOLERANCE;
    let mut removed = vec![false; img.px.len()];
    let mut queue = VecDeque::new();
    let push = |x: u32, y: u32, removed: &mut Vec<bool>, queue: &mut VecDeque<(u32, u32)>| {
        let i = (y * w + x) as usize;
        if !removed[i] && near(img.at(x, y)) {
            removed[i] = true;
            queue.push_back((x, y));
        }
    };
    for x in 0..w {
        push(x, 0, &mut removed, &mut queue);
        push(x, h - 1, &mut removed, &mut queue);
    }
    for y in 0..h {
        push(0, y, &mut removed, &mut queue);
        push(w - 1, y, &mut removed, &mut queue);
    }
    while let Some((x, y)) = queue.pop_front() {
        if x > 0 {
            push(x - 1, y, &mut removed, &mut queue);
        }
        if x + 1 < w {
            push(x + 1, y, &mut removed, &mut queue);
        }
        if y > 0 {
            push(x, y - 1, &mut removed, &mut queue);
        }
        if y + 1 < h {
            push(x, y + 1, &mut removed, &mut queue);
        }
    }
    if !removed.iter().any(|&r| r) {
        return false;
    }
    // Сглаженный край: чем ближе к фону, тем прозрачнее.
    let tol = BG_TOLERANCE.sqrt();
    let mut fringe = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if removed[i] {
                continue;
            }
            let touches = (x > 0 && removed[i - 1])
                || (x + 1 < w && removed[i + 1])
                || (y > 0 && removed[i - w as usize])
                || (y + 1 < h && removed[i + w as usize]);
            if touches {
                let d = dist2(lab_alpha(img.px[i]), bg).sqrt();
                if d < 3.0 * tol {
                    let k = ((d - tol) / (2.0 * tol)).clamp(0.0, 1.0);
                    fringe.push((i, k));
                }
            }
        }
    }
    for (i, r) in removed.iter().enumerate() {
        if *r {
            img.px[i] = [0, 0, 0, 0];
        }
    }
    for (i, k) in fringe {
        img.px[i][3] = (img.px[i][3] as f32 * k).round() as u8;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пиксель-арт `cells` (цвета по клеткам), увеличенный в `n` раз.
    fn blocky(cells: &[[u8; 4]], cw: u32, n: u32) -> Rgba {
        let ch = cells.len() as u32 / cw;
        let mut px = Vec::new();
        for y in 0..ch * n {
            for x in 0..cw * n {
                px.push(cells[((y / n) * cw + x / n) as usize]);
            }
        }
        Rgba::new(cw * n, ch * n, px)
    }

    fn checker(cw: u32, ch: u32) -> Vec<[u8; 4]> {
        (0..cw * ch)
            .map(|i| {
                let (x, y) = (i % cw, i / cw);
                match (x * 7 + y * 3) % 4 {
                    0 => [255, 0, 0, 255],
                    1 => [0, 0, 0, 0],
                    2 => [20, 200, 40, 255],
                    _ => [250, 250, 250, 255],
                }
            })
            .collect()
    }

    #[test]
    fn finds_enlarged_pixel_art() {
        let img = blocky(&checker(16, 16), 16, 8);
        assert_eq!(find_blocks(&img), Some(Blocks { n: 8, ox: 0, oy: 0 }));
        let small = blocky(&checker(16, 16), 16, 1);
        assert_eq!(find_blocks(&small).map(|b| b.n), Some(1));
    }

    #[test]
    fn smooth_drawing_is_not_pixel_art() {
        // Плавный переход: много цветов, границы не на сетке.
        let px = (0..200 * 200)
            .map(|i| {
                let (x, y) = (i % 200, i / 200);
                [(x + y) as u8, (x * 3 / 2) as u8, 128, 255]
            })
            .collect();
        let img = Rgba::new(200, 200, px);
        assert_eq!(find_blocks(&img), None);
        assert!(!is_photo(&Rgba::new(4, 4, vec![[1, 2, 3, 255]; 16])));
    }

    #[test]
    fn removes_white_background_only_from_edges() {
        let mut px = vec![[255, 255, 255, 255]; 20 * 20];
        for y in 5..15 {
            for x in 5..15 {
                px[y * 20 + x] = [200, 30, 30, 255];
            }
        }
        // Белая «дырка» внутри красного — не фон.
        px[10 * 20 + 10] = [255, 255, 255, 255];
        let mut img = Rgba::new(20, 20, px);
        assert!(remove_background(&mut img));
        assert_eq!(img.at(0, 0)[3], 0);
        assert_eq!(img.at(7, 7), [200, 30, 30, 255]);
        assert_eq!(img.at(10, 10), [255, 255, 255, 255]);
        let mut red = Rgba::new(4, 4, vec![[200, 30, 30, 255]; 16]);
        red.px[0] = [0, 0, 255, 255];
        red.px[3] = [0, 255, 0, 255];
        assert!(!remove_background(&mut red));
    }
}

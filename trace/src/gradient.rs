//! Плавные переходы → линейные градиенты (своя доработка, в VTracer такого
//! нет). Разбиение на цвета делает из плавного перехода полосы; здесь
//! соседние полосы собираются в одну фигуру с градиентом, если выполнены
//! все четыре проверки (`docs/ROADMAP.md`, «Градиенты»):
//!
//! 1. граница между полосами не нарисована — перепад цвета поперёк неё
//!    не больше перепада в глубине полос;
//! 2. соседство существенное — общая граница не меньше 30 % периметра
//!    меньшей полосы, шаг цвета мал;
//! 3. группа — один переход: не меньше трёх полос, достаточная площадь;
//! 4. подгонка точна — градиент ошибается в среднем не больше полос
//!    и нигде заметно (95-й процентиль ошибки ниже порога).
//!
//! Если переход по подгонке незаметен (концы почти одного цвета), это
//! не градиент, а однотонная область, которую шум (JPEG) разбил на оттенки:
//! она склеивается в одну сплошную заливку.
//!
//! Подгонка — наименьшими квадратами в sRGB: в нём смешивают цвета
//! rlottie и lottie-web. Цвет и непрозрачность подбираются отдельно.

use std::collections::HashMap;

use crate::image::{dist2, lab_alpha, Rgba};
use crate::quantize::{Quantized, CLEAR};

/// Проверка 1: перепад поперёк границы не больше стольких перепадов
/// в глубине полос (плюс шум).
const ACROSS_RATIO: f32 = 2.0;
/// Шум перепада (OKLab): ниже — разница не видна.
const STEP_NOISE: f32 = 0.004;
/// Глубина замера перепада внутри полосы, пиксели трассировки.
const DEPTH: u32 = 3;
/// Проверка 2: доля периметра меньшей полосы и наибольший шаг цвета (OKLab).
const MIN_SHARED: f32 = 0.3;
const MAX_STEP: f32 = 0.12;
/// Проверка 3: полос не меньше и площадь не меньше (пиксели канваса).
const MIN_REGIONS: usize = 3;
const MIN_AREA_CANVAS: f32 = 24.0 * 24.0;
/// Проверка 4: 95-й процентиль ошибки градиента (OKLab) — порог заметности.
const MAX_P95: f32 = 0.04;
/// Переход заметен, если цвета его концов различаются хотя бы на столько
/// (OKLab с непрозрачностью); меньше — однотонная область.
const MIN_SPAN: f32 = 0.04;
/// Третья точка — только если снижает среднюю ошибку хотя бы на столько.
const THIRD_STOP_GAIN: f32 = 0.7;

/// Градиент группы полос в координатах трассировки.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// Точки: смещение 0–1 и цвет RGBA.
    pub stops: Vec<(f32, [u8; 4])>,
    /// Средний цвет группы — для пикселей перед векторизацией.
    pub mean: [u8; 4],
    /// Перехода нет (однотонная область): заливка — `mean`.
    pub flat: bool,
}

/// Итог: у каждого пикселя — номер группы с градиентом или `None`.
#[derive(Debug, Clone, Default)]
pub struct Gradients {
    pub groups: Vec<Gradient>,
    /// Номер группы для пикселя (`u32::MAX` — без градиента).
    pub pixel_group: Vec<u32>,
}

impl Gradients {
    pub const NONE: u32 = u32::MAX;
}

struct Region {
    label: u16,
    area: u32,
    perimeter: u32,
}

#[derive(Default, Clone, Copy)]
struct Edge {
    len: u32,
    across: f32,
    inside: f32,
    inside_n: u32,
}

/// Связные области одной метки (4-связность). Возвращает номер области
/// каждого пикселя (`u32::MAX` — прозрачный) и области.
fn regions(q: &Quantized, w: u32, h: u32) -> (Vec<u32>, Vec<Region>) {
    let n = q.labels.len();
    let mut id = vec![u32::MAX; n];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..n {
        if id[start] != u32::MAX || q.labels[start] == CLEAR {
            continue;
        }
        let label = q.labels[start];
        let rid = out.len() as u32;
        let mut area = 0;
        id[start] = rid;
        stack.push(start);
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = ((i as u32) % w, (i as u32) / w);
            let mut visit = |j: usize| {
                if id[j] == u32::MAX && q.labels[j] == label {
                    id[j] = rid;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w as usize);
            }
            if y + 1 < h {
                visit(i + w as usize);
            }
        }
        out.push(Region {
            label,
            area,
            perimeter: 0,
        });
    }
    (id, out)
}

struct Dsu(Vec<u32>);

impl Dsu {
    fn find(&mut self, a: u32) -> u32 {
        let mut a = a;
        while self.0[a as usize] != a {
            self.0[a as usize] = self.0[self.0[a as usize] as usize];
            a = self.0[a as usize];
        }
        a
    }
    fn union(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b) as usize] = a.min(b);
        }
    }
}

/// Ищет плавные переходы. `src` — картинка трассировки (сглаженная, если
/// у пресета есть сглаживание), `q` — её разбиение на цвета, `scale` —
/// пикселей трассировки на пиксель канваса.
pub fn find(src: &Rgba, q: &Quantized, scale: f32) -> Gradients {
    let (w, h) = (src.w, src.h);
    let lab: Vec<[f32; 4]> = src.px.iter().map(|&p| lab_alpha(p)).collect();
    let (rid, mut regs) = regions(q, w, h);
    let at = |x: u32, y: u32| (y * w + x) as usize;

    // Границы между областями: длина, перепад поперёк и в глубине.
    let mut edges: HashMap<(u32, u32), Edge> = HashMap::new();
    let step_inside = |from: (u32, u32), dir: (i32, i32), r: u32| -> Option<f32> {
        // Идём вглубь области r от пикселя границы на DEPTH шагов
        // (сколько получится) и меряем перепад последнего шага.
        let (mut x, mut y) = (from.0 as i32, from.1 as i32);
        let mut prev = None;
        for _ in 0..DEPTH {
            let (nx, ny) = (x + dir.0, y + dir.1);
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                break;
            }
            if rid[at(nx as u32, ny as u32)] != r {
                break;
            }
            prev = Some((x, y));
            x = nx;
            y = ny;
        }
        let (px, py) = prev?;
        Some(dist2(lab[at(px as u32, py as u32)], lab[at(x as u32, y as u32)]).sqrt())
    };
    for y in 0..h {
        for x in 0..w {
            let a = rid[at(x, y)];
            for (dx, dy) in [(1u32, 0u32), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= w || ny >= h {
                    continue;
                }
                let b = rid[at(nx, ny)];
                if a == b {
                    continue;
                }
                if a != u32::MAX {
                    regs[a as usize].perimeter += 1;
                }
                if b != u32::MAX {
                    regs[b as usize].perimeter += 1;
                }
                if a == u32::MAX || b == u32::MAX {
                    continue;
                }
                let across = dist2(lab[at(x, y)], lab[at(nx, ny)]).sqrt();
                let back = (-(dx as i32), -(dy as i32));
                let fwd = (dx as i32, dy as i32);
                let ia = step_inside((x, y), back, a);
                let ib = step_inside((nx, ny), fwd, b);
                let e = edges.entry((a.min(b), a.max(b))).or_default();
                e.len += 1;
                e.across += across;
                for v in [ia, ib].into_iter().flatten() {
                    e.inside += v;
                    e.inside_n += 1;
                }
            }
        }
    }
    // Края картинки — тоже периметр.
    for x in 0..w {
        for y in [0, h - 1] {
            if let Some(r) = regs.get_mut(rid[at(x, y)] as usize) {
                r.perimeter += 1;
            }
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            if let Some(r) = regs.get_mut(rid[at(x, y)] as usize) {
                r.perimeter += 1;
            }
        }
    }

    // Проверки 1 и 2 — по каждой границе.
    let palette: Vec<[f32; 4]> = q.colors.iter().map(|&c| lab_alpha(c)).collect();
    let mut dsu = Dsu((0..regs.len() as u32).collect());
    let mut linked = vec![false; regs.len()];
    let mut keys: Vec<_> = edges.keys().copied().collect();
    keys.sort_unstable();
    for (a, b) in keys {
        let e = edges[&(a, b)];
        let (ra, rb) = (&regs[a as usize], &regs[b as usize]);
        let shared = e.len as f32 / ra.perimeter.min(rb.perimeter).max(1) as f32;
        let step = dist2(palette[ra.label as usize], palette[rb.label as usize]).sqrt();
        if e.inside_n == 0 || shared < MIN_SHARED || step > MAX_STEP {
            continue;
        }
        let across = e.across / e.len as f32;
        let inside = e.inside / e.inside_n as f32;
        if across <= ACROSS_RATIO * inside + STEP_NOISE {
            dsu.union(a, b);
            linked[a as usize] = true;
            linked[b as usize] = true;
        }
    }

    // Группы (проверка 3) и подгонка (проверка 4).
    let mut members: HashMap<u32, Vec<u32>> = HashMap::new();
    for r in 0..regs.len() as u32 {
        if linked[r as usize] {
            members.entry(dsu.find(r)).or_default().push(r);
        }
    }
    let mut roots: Vec<u32> = members.keys().copied().collect();
    roots.sort_unstable();
    let min_area = MIN_AREA_CANVAS * scale * scale;
    let mut out = Gradients {
        groups: Vec::new(),
        pixel_group: vec![Gradients::NONE; src.px.len()],
    };
    let mut group_of_region = vec![Gradients::NONE; regs.len()];
    for root in roots {
        let rs = &members[&root];
        let area: u32 = rs.iter().map(|&r| regs[r as usize].area).sum();
        if rs.len() < MIN_REGIONS || (area as f32) < min_area {
            continue;
        }
        let gid = out.groups.len() as u32;
        for &r in rs {
            group_of_region[r as usize] = gid;
        }
        let pixels: Vec<u32> = (0..src.px.len() as u32)
            .filter(|&i| {
                rid[i as usize] != u32::MAX && group_of_region[rid[i as usize] as usize] == gid
            })
            .collect();
        match fit(src, &lab, &pixels, |i| {
            palette[regs[rid[i as usize] as usize].label as usize]
        }) {
            Some(g) => {
                for &i in &pixels {
                    out.pixel_group[i as usize] = gid;
                }
                out.groups.push(g);
            }
            None => {
                for &r in rs {
                    group_of_region[r as usize] = Gradients::NONE;
                }
            }
        }
    }
    out
}

/// Решает систему 3 × 3 (метод Крамера); `None` — вырожденная.
fn solve3(m: [[f64; 3]; 3], v: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(m);
    if d.abs() < 1e-12 {
        return None;
    }
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let mut mk = m;
        for r in 0..3 {
            mk[r][k] = v[r];
        }
        *o = det(mk) / d;
    }
    Some(out)
}

/// Цвет градиента в точке `u` (0–1) по точкам `stops` (sRGB 0–1).
fn eval(stops: &[(f32, [f32; 4])], u: f32) -> [f32; 4] {
    let u = u.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if u <= b.0 {
            let t = if b.0 > a.0 {
                (u - a.0) / (b.0 - a.0)
            } else {
                0.0
            };
            return std::array::from_fn(|c| a.1[c] + (b.1[c] - a.1[c]) * t);
        }
    }
    stops.last().map_or([0.0; 4], |s| s.1)
}

fn to_u8(c: [f32; 4]) -> [u8; 4] {
    c.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// Подгонка линейного градиента к пикселям группы. `band` — цвет полосы
/// пикселя (OKLab) для сравнения ошибок.
fn fit(
    src: &Rgba,
    lab: &[[f32; 4]],
    pixels: &[u32],
    band: impl Fn(u32) -> [f32; 4],
) -> Option<Gradient> {
    let w = src.w;
    let rgba = |i: u32| -> [f64; 4] { src.px[i as usize].map(|v| v as f64 / 255.0) };
    // Плоскость v = c0 + cx·x + cy·y по каждому каналу; цвет — с весом
    // непрозрачности (у прозрачных пикселей цвета нет).
    let mut grads = [[0f64; 2]; 4];
    for (ch, g) in grads.iter_mut().enumerate() {
        let mut m = [[0f64; 3]; 3];
        let mut v = [0f64; 3];
        for &i in pixels {
            let p = rgba(i);
            let wt = if ch == 3 { 1.0 } else { p[3] };
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            let b = [1.0, x, y];
            for r in 0..3 {
                for c in 0..3 {
                    m[r][c] += wt * b[r] * b[c];
                }
                v[r] += wt * b[r] * p[ch];
            }
        }
        let s = solve3(m, v)?;
        *g = [s[1], s[2]];
    }
    // Направление — главная ось суммы g·gᵀ.
    let (mut sxx, mut sxy, mut syy) = (0f64, 0f64, 0f64);
    for g in grads {
        sxx += g[0] * g[0];
        sxy += g[0] * g[1];
        syy += g[1] * g[1];
    }
    let tr = sxx + syy;
    if tr < 1e-12 {
        return None;
    }
    let ang = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (dx, dy) = (ang.cos(), ang.sin());
    // Проекции и их пределы (без крайних 0,5 %).
    let mut ts: Vec<f64> = pixels
        .iter()
        .map(|&i| (i % w) as f64 * dx + (i / w) as f64 * dy)
        .collect();
    let mut sorted = ts.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let lo = sorted[sorted.len() / 200];
    let hi = sorted[sorted.len() - 1 - sorted.len() / 200];
    if hi - lo < 2.0 {
        return None;
    }
    for t in ts.iter_mut() {
        *t = ((*t - lo) / (hi - lo)).clamp(0.0, 1.0);
    }
    // Ломаная с узлами `knots` наименьшими квадратами по каждому каналу.
    let fit_knots = |knots: &[f64]| -> Option<Vec<(f32, [f32; 4])>> {
        let k = knots.len();
        let basis = |u: f64| -> Vec<f64> {
            let mut b = vec![0.0; k];
            for j in 0..k - 1 {
                let (a, c) = (knots[j], knots[j + 1]);
                if u >= a && u <= c {
                    let t = if c > a { (u - a) / (c - a) } else { 0.0 };
                    b[j] = 1.0 - t;
                    b[j + 1] = t;
                    break;
                }
            }
            b
        };
        let mut stops = vec![[0f32; 4]; k];
        for ch in 0..4 {
            let mut m = [[0f64; 3]; 3];
            let mut v = [0f64; 3];
            for (n, &i) in pixels.iter().enumerate() {
                let p = rgba(i);
                let wt = if ch == 3 { 1.0 } else { p[3].max(1e-3) };
                let b = basis(ts[n]);
                for r in 0..k {
                    for c in 0..k {
                        m[r][c] += wt * b[r] * b[c];
                    }
                    v[r] += wt * b[r] * p[ch];
                }
            }
            if k == 2 {
                m[2][2] = 1.0;
            }
            let s = solve3(m, v)?;
            for j in 0..k {
                stops[j][ch] = s[j].clamp(0.0, 1.0) as f32;
            }
        }
        Some(
            knots
                .iter()
                .zip(stops)
                .map(|(&u, c)| (u as f32, c))
                .collect(),
        )
    };
    let errors = |stops: &[(f32, [f32; 4])]| -> (f32, f32) {
        let mut e: Vec<f32> = pixels
            .iter()
            .enumerate()
            .map(|(n, &i)| {
                dist2(lab_alpha(to_u8(eval(stops, ts[n] as f32))), lab[i as usize]).sqrt()
            })
            .collect();
        let mean = e.iter().sum::<f32>() / e.len() as f32;
        e.sort_by(|a, b| a.total_cmp(b));
        (mean, e[(e.len() * 95 / 100).min(e.len() - 1)])
    };
    let mut best = fit_knots(&[0.0, 1.0])?;
    let mut best_err = errors(&best);
    let two = best_err.0;
    for m in [0.3, 0.4, 0.5, 0.6, 0.7] {
        if let Some(s) = fit_knots(&[0.0, m, 1.0]) {
            let e = errors(&s);
            if e.0 <= THIRD_STOP_GAIN * two && e.0 < best_err.0 {
                best = s;
                best_err = e;
            }
        }
    }
    // Проверка 4.
    let band_err = pixels
        .iter()
        .map(|&i| dist2(band(i), lab[i as usize]).sqrt())
        .sum::<f32>()
        / pixels.len() as f32;
    if best_err.0 > band_err || best_err.1 > MAX_P95 {
        return None;
    }
    // Концы градиента на прямой через середину группы.
    let (mut cx, mut cy) = (0f64, 0f64);
    for &i in pixels {
        cx += (i % w) as f64;
        cy += (i / w) as f64;
    }
    cx /= pixels.len() as f64;
    cy /= pixels.len() as f64;
    let tc = cx * dx + cy * dy;
    let p = |t: f64| ((cx + dx * (t - tc)) as f32, (cy + dy * (t - tc)) as f32);
    let (x1, y1) = p(lo);
    let (x2, y2) = p(hi);
    let mut sum = [0f64; 4];
    for &i in pixels {
        let q = rgba(i);
        for c in 0..4 {
            sum[c] += q[c];
        }
    }
    let mean = to_u8(sum.map(|s| (s / pixels.len() as f64) as f32));
    let ends: Vec<[f32; 4]> = best.iter().map(|s| lab_alpha(to_u8(s.1))).collect();
    let span = ends
        .iter()
        .flat_map(|a| ends.iter().map(move |b| dist2(*a, *b).sqrt()))
        .fold(0f32, f32::max);
    Some(Gradient {
        flat: span < MIN_SPAN,
        x1: x1 + 0.5,
        y1: y1 + 0.5,
        x2: x2 + 0.5,
        y2: y2 + 0.5,
        stops: best.into_iter().map(|(u, c)| (u, to_u8(c))).collect(),
        mean,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantize::quantize;

    fn ramp(w: u32, h: u32) -> Rgba {
        let px = (0..w * h)
            .map(|i| {
                let x = i % w;
                let v = (40 + x * 180 / w) as u8;
                [v, 60, 255 - v, 255]
            })
            .collect();
        Rgba::new(w, h, px)
    }

    #[test]
    fn smooth_ramp_becomes_one_gradient() {
        let img = ramp(200, 120);
        let q = quantize(&img, 8, 128);
        let g = find(&img, &q, 1.0);
        assert_eq!(g.groups.len(), 1, "одна группа");
        let gr = &g.groups[0];
        assert!((gr.x2 - gr.x1).abs() > 150.0 && (gr.y2 - gr.y1).abs() < 5.0);
        assert!(g.pixel_group.iter().all(|&p| p == 0));
        assert!(!gr.flat);
    }

    #[test]
    fn noisy_flat_area_is_flat_not_gradient() {
        // Сплошной цвет с шумом (как JPEG), разбитый на много оттенков.
        let px = (0..200u32 * 120)
            .map(|i| {
                let n = ((i.wrapping_mul(2_654_435_761) >> 13) % 9) as u8;
                [200 + n, 40 + n, 50, 255]
            })
            .collect();
        let img = Rgba::new(200, 120, px);
        let q = quantize(&img, 8, 128);
        let g = find(&img, &q, 1.0);
        assert!(g.groups.iter().all(|g| g.flat), "{:?}", g.groups);
    }

    #[test]
    fn hard_stripes_stay_stripes() {
        // Четыре полосы с резкими границами — флаг, а не переход.
        let colors = [
            [200, 30, 30, 255],
            [220, 60, 50, 255],
            [240, 90, 70, 255],
            [250, 120, 90, 255],
        ];
        let px = (0..200 * 120)
            .map(|i| colors[((i % 200) / 50) as usize])
            .collect();
        let img = Rgba::new(200, 120, px);
        let q = quantize(&img, 8, 128);
        assert!(find(&img, &q, 1.0).groups.is_empty());
    }

    #[test]
    fn small_ramp_is_ignored() {
        let img = ramp(12, 12);
        let q = quantize(&img, 6, 128);
        assert!(find(&img, &q, 1.0).groups.is_empty());
    }
}

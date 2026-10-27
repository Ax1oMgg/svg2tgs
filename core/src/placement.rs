//! Размещение рисунка на канвасе.
//!
//! Рисунок проходит цепочку шагов, каждый работает с результатом предыдущего:
//! отразить → повернуть → пропорции → обрезать края → вписать в рабочую
//! область (канвас минус отступ и обводка) → сдвинуть на заданное положение.
//! Всё, что после этого вышло за рабочую область или за рамку обрезки,
//! отрезается. Итог — одна матрица и один прямоугольник видимой области.

use usvg::Transform;

use crate::options::{Fit, Options};

/// Где и как рисуется рисунок.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Из координат SVG в пиксели канваса.
    pub transform: Transform,
    /// Видимая область `[x0, y0, x1, y1]` в пикселях канваса.
    pub clip: [f64; 4],
}

/// Наименьший видимый остаток после обрезки краёв, доля стороны.
const MIN_KEEP: f32 = 0.05;

/// Рамка `[x0, y0, x1, y1]` прямоугольника `w × h` после матрицы `m`.
fn bounds(m: &Transform, w: f32, h: f32) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
        let px = m.sx * x + m.kx * y + m.tx;
        let py = m.ky * x + m.sy * y + m.ty;
        b = [b[0].min(px), b[1].min(py), b[2].max(px), b[3].max(py)];
    }
    b
}

/// Размещение рисунка размера `size` на квадратном канвасе стороной `canvas`.
pub fn place(size: usvg::Size, canvas: f32, options: &Options) -> Placement {
    let (w, h) = (size.width(), size.height());

    // 1–3. Отразить, повернуть, растянуть — вокруг центра рисунка.
    let fx = if options.flip_h { -1.0 } else { 1.0 };
    let fy = if options.flip_v { -1.0 } else { 1.0 };
    let aspect = if options.aspect.is_finite() && options.aspect > 0.0 {
        options.aspect.clamp(0.25, 4.0)
    } else {
        1.0
    };
    let rotation = if options.rotation.is_finite() {
        options.rotation
    } else {
        0.0
    };
    let shape = Transform::from_scale(aspect, 1.0)
        .pre_rotate(rotation)
        .pre_scale(fx, fy)
        .pre_translate(-w / 2.0, -h / 2.0);

    // 4. Обрезать края — доли рамки, которую видит пользователь.
    let b = bounds(&shape, w, h);
    let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
    let c = options.crop;
    let frac = |v: f32| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let (mut l, mut r) = (frac(c.left), frac(c.right));
    let (mut t, mut bo) = (frac(c.top), frac(c.bottom));
    let squeeze = |a: &mut f32, b: &mut f32| {
        let over = *a + *b - (1.0 - MIN_KEEP);
        if over > 0.0 {
            let k = (1.0 - MIN_KEEP) / (*a + *b);
            *a *= k;
            *b *= k;
        }
    };
    squeeze(&mut l, &mut r);
    squeeze(&mut t, &mut bo);
    let crop = [b[0] + l * bw, b[1] + t * bh, b[2] - r * bw, b[3] - bo * bh];
    let (cw, ch) = ((crop[2] - crop[0]).max(1e-6), (crop[3] - crop[1]).max(1e-6));

    // 5. Вписать в рабочую область: канвас минус отступ и место под обводку.
    let padding = options.padding.clamp(0.0, 0.45);
    let area = (canvas * (1.0 - 2.0 * padding) - 2.0 * options.outline_width()).max(1.0);
    let origin = (canvas - area) / 2.0;
    let contain = (area / cw).min(area / ch);
    let scale = match options.fit {
        Fit::Contain => contain,
        Fit::Cover => (area / cw).max(area / ch),
        Fit::Width => area / cw,
        Fit::Height => area / ch,
        Fit::Free => contain * options.scale.clamp(0.05, 20.0),
    };

    // 6. Положение: доля свободного места (или лишнего, если рисунок больше).
    let pos = |v: f32| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.5
        }
    };
    let (pw, ph) = (cw * scale, ch * scale);
    let x = origin + (area - pw) * pos(options.position_x);
    let y = origin + (area - ph) * pos(options.position_y);

    let transform = Transform::from_translate(x, y)
        .pre_scale(scale, scale)
        .pre_translate(-crop[0], -crop[1])
        .pre_concat(shape);

    // Видимое — пересечение рабочей области и обрезанного рисунка.
    let (a0, a1) = (origin as f64, (origin + area) as f64);
    let clip = [
        (x as f64).max(a0),
        (y as f64).max(a0),
        ((x + pw) as f64).min(a1),
        ((y + ph) as f64).min(a1),
    ];
    Placement { transform, clip }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(w: f32, h: f32) -> usvg::Size {
        usvg::Size::from_wh(w, h).unwrap()
    }

    fn map(t: &Transform, x: f32, y: f32) -> (f32, f32) {
        (t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty)
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn contain_centers_like_before() {
        let p = place(size(200.0, 100.0), 512.0, &Options::default());
        assert!(close(map(&p.transform, 0.0, 0.0), (0.0, 128.0)));
        assert!(close(map(&p.transform, 200.0, 100.0), (512.0, 384.0)));
        assert_eq!(p.clip, [0.0, 128.0, 512.0, 384.0]);
    }

    #[test]
    fn width_fit_with_position_chooses_visible_part() {
        // Высокий рисунок по ширине: видна верхняя часть при Y = 0.
        let o = Options {
            fit: Fit::Width,
            position_y: 0.0,
            ..Options::default()
        };
        let p = place(size(100.0, 300.0), 512.0, &o);
        assert!(close(map(&p.transform, 0.0, 0.0), (0.0, 0.0)));
        assert!(close(map(&p.transform, 100.0, 100.0), (512.0, 512.0)));
        assert_eq!(p.clip, [0.0, 0.0, 512.0, 512.0]);
    }

    #[test]
    fn padding_is_a_margin_even_when_cropping() {
        let o = Options {
            fit: Fit::Cover,
            padding: 0.1,
            ..Options::default()
        };
        let p = place(size(300.0, 100.0), 500.0, &o);
        assert_eq!(p.clip, [50.0, 50.0, 450.0, 450.0]);
    }

    #[test]
    fn flip_rotate_and_crop() {
        let o = Options {
            flip_h: true,
            ..Options::default()
        };
        let p = place(size(100.0, 100.0), 100.0, &o);
        assert!(close(map(&p.transform, 0.0, 0.0), (100.0, 0.0)));

        let o = Options {
            rotation: 90.0,
            ..Options::default()
        };
        let p = place(size(200.0, 100.0), 200.0, &o);
        // После поворота рисунок стоит, левый верх уходит в правый верх.
        assert!(close(map(&p.transform, 0.0, 0.0), (150.0, 0.0)));

        let o = Options {
            crop: crate::options::Crop {
                right: 0.5,
                ..Default::default()
            },
            ..Options::default()
        };
        let p = place(size(200.0, 100.0), 100.0, &o);
        // Осталась левая половина (квадрат) — на весь канвас.
        assert!(close(map(&p.transform, 100.0, 100.0), (100.0, 100.0)));
        assert_eq!(p.clip, [0.0, 0.0, 100.0, 100.0]);
    }
}

//! Геометрическая обрезка.
//!
//! Telegram запрещает в стикерах маски Lottie, поэтому `clipPath` и края
//! канваса («объекты не должны выходить за канвас») применяются заранее:
//! фигуры пересекаются с областью обрезки, на выходе — обычные пути.
//!
//! Булевы операции выполняет i_overlay; кривые для этого спрямляются
//! с точностью [`FLATTEN_TOLERANCE`]. Спрямляются только фигуры, которые
//! действительно пересекают границу обрезки; остальные остаются кривыми.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::relate::FloatRelate;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;

use crate::lottie::{Bezier, Num, Rounder, FILL_RULE_EVENODD};

/// Допустимое отклонение спрямлённой кривой, пиксели канваса 512.
pub const FLATTEN_TOLERANCE: f64 = 0.1;

/// Замкнутые контуры области, без самопересечений, правило nonzero.
pub type Contours = Vec<Vec<[f64; 2]>>;

/// Как фигура расположена относительно области обрезки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Целиком внутри — обрезать не нужно.
    Inside,
    /// Целиком снаружи — фигура не видна.
    Outside,
    /// Пересекает границу — нужна обрезка.
    Crossing,
}

/// Прямоугольник `[x0, y0, x1, y1]`.
pub fn rect(r: [f64; 4]) -> Contours {
    vec![vec![[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]]
}

/// Спрямляет кривые Lottie в многоугольники. Каждый подпуть замыкается —
/// так заливка трактует и открытые подпути.
pub fn flatten(beziers: &[Bezier]) -> Contours {
    let mut out = Vec::with_capacity(beziers.len());
    for b in beziers {
        let n = b.v.len();
        if n < 2 {
            continue;
        }
        let pt = |k: usize| [b.v[k][0].0, b.v[k][1].0];
        let mut contour = vec![pt(0)];
        let segments = if b.c { n } else { n - 1 };
        for k in 0..segments {
            let j = (k + 1) % n;
            let p0 = pt(k);
            let p3 = pt(j);
            let p1 = [p0[0] + b.o[k][0].0, p0[1] + b.o[k][1].0];
            let p2 = [p3[0] + b.i[j][0].0, p3[1] + b.i[j][1].0];
            flatten_cubic(p0, p1, p2, p3, &mut contour);
        }
        if contour.len() > 1 && contour.first() == contour.last() {
            contour.pop();
        }
        if contour.len() >= 3 {
            out.push(contour);
        }
    }
    out
}

fn flatten_cubic(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], out: &mut Vec<[f64; 2]>) {
    let straight = p1 == p0 && p2 == p3;
    if straight {
        out.push(p3);
        return;
    }
    // Число отрезков по оценке отклонения: вторые разности контрольных точек.
    let dd = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        ((a[0] - 2.0 * b[0] + c[0]).powi(2) + (a[1] - 2.0 * b[1] + c[1]).powi(2)).sqrt()
    };
    let l = dd(p0, p1, p2).max(dd(p1, p2, p3));
    let n = ((0.75 * l / FLATTEN_TOLERANCE).sqrt().ceil() as usize).clamp(1, 256);
    for k in 1..=n {
        let t = k as f64 / n as f64;
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        out.push([
            a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
            a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
        ]);
    }
}

fn fill_rule(lottie_rule: u8) -> FillRule {
    if lottie_rule == FILL_RULE_EVENODD {
        FillRule::EvenOdd
    } else {
        FillRule::NonZero
    }
}

fn shapes_to_contours(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> Contours {
    shapes.into_iter().flatten().collect()
}

/// Область, закрашиваемая кривыми по правилу заливки Lottie (1 или 2).
pub fn region(beziers: &[Bezier], lottie_rule: u8) -> Contours {
    let contours = flatten(beziers);
    if contours.is_empty() {
        return Vec::new();
    }
    shapes_to_contours(contours.simplify_shape(fill_rule(lottie_rule)))
}

pub fn intersect(a: &Contours, b: &Contours) -> Contours {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    shapes_to_contours(a.overlay(b, OverlayRule::Intersect, FillRule::NonZero))
}

pub fn union(a: &Contours, b: &Contours) -> Contours {
    if a.is_empty() {
        return b.clone();
    }
    if b.is_empty() {
        return a.clone();
    }
    shapes_to_contours(a.overlay(b, OverlayRule::Union, FillRule::NonZero))
}

/// Склеивает контуры в одну область по правилу nonzero.
pub fn simplify(c: &Contours) -> Contours {
    if c.is_empty() {
        return Vec::new();
    }
    shapes_to_contours(c.simplify_shape(FillRule::NonZero))
}

/// Обводка вокруг области шириной `width`: область, расширенная наружу
/// со скруглёнными углами. Дырки заполняются — как у вырезанного стикера.
pub fn outline(region: &Contours, width: f64) -> Contours {
    use i_overlay::mesh::float::outline::offset::OutlineOffset;
    use i_overlay::mesh::float::style::{LineJoin, OutlineStyle};
    if region.is_empty() || width <= 0.0 {
        return Vec::new();
    }
    // Дуга скругления — отрезками длиной L, отклонение L²/8R ≤ допуска.
    let step = (8.0 * FLATTEN_TOLERANCE / width).sqrt().clamp(0.05, 0.5);
    let style = OutlineStyle::new(width).line_join(LineJoin::Round(step));
    let shapes = region
        .simplify_shape(FillRule::NonZero)
        .into_iter()
        .filter_map(|shape| shape.into_iter().next())
        .collect::<Contours>();
    // Первый контур каждой части — внешний; дырки (и те, что остались
    // после расширения) выбрасываются, затем части склеиваются.
    let outer: Contours = shapes
        .outline(&style)
        .into_iter()
        .filter_map(|shape| shape.into_iter().next())
        .collect();
    simplify(&outer)
}

/// Видимая область: прямоугольник (канвас) или произвольная форма.
/// Для прямоугольника положение фигуры считается простым сравнением.
#[derive(Debug, Clone)]
pub struct Area {
    pub contours: Contours,
    rect: Option<[f64; 4]>,
}

impl Area {
    pub fn from_rect(r: [f64; 4]) -> Self {
        Self {
            contours: rect(r),
            rect: Some(r),
        }
    }

    /// Сужает область до пересечения с `other`.
    pub fn intersect(&self, other: &Contours) -> Self {
        Self {
            contours: intersect(&self.contours, other),
            rect: None,
        }
    }

    pub fn relate(&self, b: [f64; 4]) -> Relation {
        match self.rect {
            Some(r) => {
                if b[0] >= r[0] && b[1] >= r[1] && b[2] <= r[2] && b[3] <= r[3] {
                    Relation::Inside
                } else if b[2] < r[0] || b[3] < r[1] || b[0] > r[2] || b[1] > r[3] {
                    Relation::Outside
                } else {
                    Relation::Crossing
                }
            }
            None => relate(&self.contours, b),
        }
    }
}

/// Положение прямоугольника `bbox` относительно области.
pub fn relate(region: &Contours, bbox: [f64; 4]) -> Relation {
    if region.is_empty() || bbox[0] >= bbox[2] && bbox[1] >= bbox[3] {
        return Relation::Outside;
    }
    // Вырожденную рамку (линия) чуть расширяем, чтобы у неё была площадь.
    let e = 1e-3;
    let r = rect([bbox[0] - e, bbox[1] - e, bbox[2] + e, bbox[3] + e]);
    if region.covers(&r) {
        Relation::Inside
    } else if !region.intersects(&r) {
        Relation::Outside
    } else {
        Relation::Crossing
    }
}

/// Рамка кривых по вершинам и контрольным точкам (кривая лежит внутри).
pub fn bbox(beziers: &[Bezier]) -> Option<[f64; 4]> {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    let mut any = false;
    for bz in beziers {
        for (k, v) in bz.v.iter().enumerate() {
            let p = [v[0].0, v[1].0];
            for q in [
                p,
                [p[0] + bz.i[k][0].0, p[1] + bz.i[k][1].0],
                [p[0] + bz.o[k][0].0, p[1] + bz.o[k][1].0],
            ] {
                b = [
                    b[0].min(q[0]),
                    b[1].min(q[1]),
                    b[2].max(q[0]),
                    b[3].max(q[1]),
                ];
                any = true;
            }
        }
    }
    any.then_some(b)
}

/// Контуры области → кривые Lottie из прямых отрезков (правило nonzero:
/// внешние контуры и дырки у i_overlay обходятся в разные стороны).
/// Дырки у i_overlay часто касаются внешнего контура в одной точке. Если
/// контур начинается в текущей точке (начале предыдущего замкнутого), rlottie
/// склеивает их — поэтому начало такого контура сдвигается на другую вершину.
pub fn to_beziers(region: &Contours, round: &Rounder) -> Vec<Bezier> {
    let zero = [Num(0.0); 2];
    let mut out = Vec::with_capacity(region.len());
    let mut current: Option<[Num; 2]> = None;
    for c in region.iter().filter(|c| c.len() >= 3) {
        let c = crate::geometry::simplify_polygon(c, round.tolerance);
        let mut v: Vec<[Num; 2]> = c.iter().map(|p| round.pt(*p)).collect();
        if let Some(cur) = current {
            let near =
                |p: &[Num; 2]| (p[0].0 - cur[0].0).abs() < 0.5 && (p[1].0 - cur[1].0).abs() < 0.5;
            if near(&v[0]) {
                if let Some(k) = v.iter().position(|p| !near(p)) {
                    v.rotate_left(k);
                }
            }
        }
        current = Some(v[0]);
        out.push(Bezier {
            c: true,
            i: vec![zero; v.len()],
            o: vec![zero; v.len()],
            v,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(c: &Contours) -> f64 {
        c.iter()
            .map(|poly| {
                let n = poly.len();
                (0..n)
                    .map(|k| {
                        let (a, b) = (poly[k], poly[(k + 1) % n]);
                        a[0] * b[1] - b[0] * a[1]
                    })
                    .sum::<f64>()
                    / 2.0
            })
            .sum::<f64>()
            .abs()
    }

    #[test]
    fn intersection_of_squares() {
        let a = rect([0.0, 0.0, 10.0, 10.0]);
        let b = rect([5.0, 5.0, 15.0, 15.0]);
        assert!((area(&intersect(&a, &b)) - 25.0).abs() < 1e-6);
        assert!((area(&union(&a, &b)) - 175.0).abs() < 1e-6);
    }

    #[test]
    fn relation_to_region() {
        let r = rect([0.0, 0.0, 100.0, 100.0]);
        assert_eq!(relate(&r, [10.0, 10.0, 20.0, 20.0]), Relation::Inside);
        assert_eq!(relate(&r, [110.0, 10.0, 120.0, 20.0]), Relation::Outside);
        assert_eq!(relate(&r, [90.0, 10.0, 120.0, 20.0]), Relation::Crossing);
        // Горизонтальная линия на границе тоже обрабатывается.
        assert_eq!(relate(&r, [10.0, 50.0, 20.0, 50.0]), Relation::Inside);
    }

    #[test]
    fn evenodd_ring_keeps_hole() {
        // Квадрат 10×10 с вложенным квадратом 4×4: evenodd — дырка.
        let r = Rounder::new(3);
        let sq = |a: f64, b: f64| Bezier {
            c: true,
            v: [[a, a], [b, a], [b, b], [a, b]].map(|p| r.pt(p)).to_vec(),
            i: vec![[Num(0.0); 2]; 4],
            o: vec![[Num(0.0); 2]; 4],
        };
        let ring = region(&[sq(0.0, 10.0), sq(3.0, 7.0)], FILL_RULE_EVENODD);
        assert!((area(&ring) - 84.0).abs() < 1e-6);
    }

    #[test]
    fn flattened_circle_is_accurate() {
        // Четверть окружности радиуса 100 кубической кривой (kappa).
        let k = 0.552_284_749_830_793_6 * 100.0;
        let r = Rounder::new(4);
        let b = Bezier {
            c: false,
            v: vec![r.pt([100.0, 0.0]), r.pt([0.0, 100.0])],
            i: vec![r.pt([0.0, 0.0]), r.pt([k, 0.0])],
            o: vec![r.pt([0.0, k]), r.pt([0.0, 0.0])],
        };
        let c = &flatten(&[b])[0];
        for p in c {
            let d = (p[0] * p[0] + p[1] * p[1]).sqrt();
            // Точки на кривой (погрешность kappa ~0.03 % радиуса).
            assert!((d - 100.0).abs() < 0.05, "{d}");
        }
        assert!(c.len() > 10);
    }
}

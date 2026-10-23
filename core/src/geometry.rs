//! Геометрия: перевод путей usvg в кривые Lottie и работа с матрицами.

use usvg::tiny_skia_path::{PathSegment, Point};
use usvg::Transform;

use crate::lottie::{Bezier, Rounder};

/// Применяет матрицу к точке и переводит её в `f64`.
pub fn map(m: &Transform, p: Point) -> [f64; 2] {
    let (x, y) = (p.x as f64, p.y as f64);
    [
        m.sx as f64 * x + m.kx as f64 * y + m.tx as f64,
        m.ky as f64 * x + m.sy as f64 * y + m.ty as f64,
    ]
}

/// Средний коэффициент масштаба матрицы: √|det|.
/// Им масштабируются толщина обводки, пунктир и радиусы.
pub fn mean_scale(m: &Transform) -> f64 {
    ((m.sx as f64 * m.sy as f64 - m.kx as f64 * m.ky as f64).abs()).sqrt()
}

/// Сохраняет ли матрица форму (только поворот, равномерный масштаб,
/// отражение и сдвиг). Если нет — окружности становятся эллипсами,
/// и толщину линии или радиальный градиент можно передать лишь приближённо.
pub fn is_similarity(m: &Transform) -> bool {
    let (a, b, c, d) = (m.sx as f64, m.ky as f64, m.kx as f64, m.sy as f64);
    let col1 = (a * a + b * b).sqrt();
    let col2 = (c * c + d * d).sqrt();
    let scale = col1.max(col2);
    if scale == 0.0 {
        return true;
    }
    let dot = (a * c + b * d).abs() / (col1 * col2).max(f64::EPSILON);
    (col1 - col2).abs() / scale < 0.01 && dot < 0.01
}

/// Одна вершина с касательными в абсолютных координатах.
#[derive(Clone, Copy)]
struct Vertex {
    p: [f64; 2],
    /// Входящая контрольная точка (абсолютная).
    c_in: [f64; 2],
    /// Исходящая контрольная точка (абсолютная).
    c_out: [f64; 2],
}

impl Vertex {
    fn at(p: [f64; 2]) -> Self {
        Self {
            p,
            c_in: p,
            c_out: p,
        }
    }
}

struct SubPath {
    verts: Vec<Vertex>,
    closed: bool,
}

/// Переводит путь в набор кривых Lottie (по одной на подпуть),
/// применяя матрицу `m`. Вырожденные подпути (одна точка) отбрасываются.
pub fn path_to_beziers(
    path: &usvg::tiny_skia_path::Path,
    m: &Transform,
    round: &Rounder,
) -> Vec<Bezier> {
    let mut subs: Vec<SubPath> = Vec::new();
    let mut cur: Option<SubPath> = None;

    let flush = |cur: &mut Option<SubPath>, subs: &mut Vec<SubPath>| {
        if let Some(sp) = cur.take() {
            subs.push(sp);
        }
    };

    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                flush(&mut cur, &mut subs);
                cur = Some(SubPath {
                    verts: vec![Vertex::at(map(m, p))],
                    closed: false,
                });
            }
            PathSegment::LineTo(p) => {
                if let Some(sp) = cur.as_mut() {
                    sp.verts.push(Vertex::at(map(m, p)));
                }
            }
            PathSegment::QuadTo(c, p) => {
                if let Some(sp) = cur.as_mut() {
                    let start = sp.verts.last().unwrap().p;
                    let (c, p) = (map(m, c), map(m, p));
                    // Квадратичная кривая → кубическая.
                    let c1 = lerp(start, c, 2.0 / 3.0);
                    let c2 = lerp(p, c, 2.0 / 3.0);
                    sp.verts.last_mut().unwrap().c_out = c1;
                    sp.verts.push(Vertex {
                        p,
                        c_in: c2,
                        c_out: p,
                    });
                }
            }
            PathSegment::CubicTo(c1, c2, p) => {
                if let Some(sp) = cur.as_mut() {
                    sp.verts.last_mut().unwrap().c_out = map(m, c1);
                    let p = map(m, p);
                    sp.verts.push(Vertex {
                        p,
                        c_in: map(m, c2),
                        c_out: p,
                    });
                }
            }
            PathSegment::Close => {
                if let Some(sp) = cur.as_mut() {
                    sp.closed = true;
                    // Последняя вершина совпадает с первой: сливаем их,
                    // иначе в Lottie получится лишний нулевой отрезок.
                    if sp.verts.len() > 1 {
                        let first = sp.verts[0].p;
                        let last = *sp.verts.last().unwrap();
                        if close_enough(first, last.p) {
                            sp.verts.pop();
                            sp.verts[0].c_in = last.c_in;
                        }
                    }
                }
                // После Z новый подпуть начинается с той же точки,
                // tiny-skia в этом случае всегда выдаёт MoveTo.
                flush(&mut cur, &mut subs);
            }
        }
    }
    flush(&mut cur, &mut subs);
    subs.iter_mut().for_each(drop_zero_segments);
    subs.retain(|sp| sp.verts.len() > 1);
    avoid_joined_starts(&mut subs);

    subs.into_iter()
        .map(|sp| Bezier {
            c: sp.closed,
            v: sp.verts.iter().map(|v| round.pt(v.p)).collect(),
            i: sp
                .verts
                .iter()
                .map(|v| round.pt(sub(v.c_in, v.p)))
                .collect(),
            o: sp
                .verts
                .iter()
                .map(|v| round.pt(sub(v.c_out, v.p)))
                .collect(),
        })
        .collect()
}

/// Склеивает соседние совпадающие вершины, соединённые прямым отрезком
/// нулевой длины (так выглядят боковые стороны «таблетки» — rect с rx = h/2).
/// Форма не меняется, а файл становится чуть меньше.
/// Если подпуть целиком нулевой длины (точка), остаются две вершины —
/// по ним он потом распознаётся как точка.
fn drop_zero_segments(sp: &mut SubPath) {
    let degenerate =
        |a: &Vertex, b: &Vertex| same(a.p, b.p) && same(a.c_out, a.p) && same(b.c_in, b.p);
    let mut out: Vec<Vertex> = Vec::with_capacity(sp.verts.len());
    for v in sp.verts.drain(..) {
        match out.last_mut() {
            Some(last) if degenerate(last, &v) => last.c_out = v.c_out,
            _ => out.push(v),
        }
    }
    if sp.closed && out.len() > 1 {
        let (first, last) = (out[0], *out.last().unwrap());
        if degenerate(&last, &first) {
            out.pop();
            out[0].c_in = last.c_in;
        }
    }
    if out.len() == 1 {
        out.push(out[0]);
    }
    sp.verts = out;
}

fn same(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

/// Обход особенности rlottie (движка Telegram): если подпуть начинается
/// в текущей точке (там, где закончился предыдущий; у замкнутого — в его
/// начале), rlottie не начинает новый подпуть, а продолжает старый —
/// рисунок ломается. Так бывает у путей вида `…Z m0 0 …` из Lucide и Figma.
///
/// Форму можно сохранить в точности: замкнутый подпуть начать с другой
/// вершины, открытый — пройти в обратную сторону.
fn avoid_joined_starts(subs: &mut [SubPath]) {
    let mut current: Option<[f64; 2]> = None;
    for sp in subs.iter_mut() {
        if let Some(cur) = current {
            if near(sp.verts[0].p, cur) {
                if sp.closed {
                    if let Some(k) = sp.verts.iter().position(|v| !near(v.p, cur)) {
                        sp.verts.rotate_left(k);
                    }
                } else if !near(sp.verts.last().unwrap().p, cur) {
                    sp.verts.reverse();
                    for v in &mut sp.verts {
                        std::mem::swap(&mut v.c_in, &mut v.c_out);
                    }
                }
            }
        }
        current = Some(if sp.closed {
            sp.verts[0].p
        } else {
            sp.verts.last().unwrap().p
        });
    }
}

/// Точки совпадают с запасом: rlottie сравнивает их с допуском,
/// а координаты потом ещё округляются.
fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.5
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn close_enough(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

#[cfg(test)]
mod tests {
    use super::*;
    use usvg::tiny_skia_path::PathBuilder;

    #[test]
    fn closed_square_has_four_vertices() {
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(10.0, 0.0);
        pb.line_to(10.0, 10.0);
        pb.line_to(0.0, 10.0);
        pb.close();
        let path = pb.finish().unwrap();
        let b = path_to_beziers(&path, &Transform::identity(), &Rounder::new(2));
        assert_eq!(b.len(), 1);
        assert!(b[0].c);
        assert_eq!(b[0].v.len(), 4);
    }

    #[test]
    fn curve_back_to_start_merges_tangent() {
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(10.0, 0.0);
        pb.cubic_to(10.0, 5.0, 5.0, 10.0, 0.0, 0.0);
        pb.close();
        let path = pb.finish().unwrap();
        let b = path_to_beziers(&path, &Transform::identity(), &Rounder::new(2));
        assert_eq!(b[0].v.len(), 2);
        // Входящая касательная первой вершины пришла от кривой: (5,10) − (0,0).
        assert_eq!(b[0].i[0][0].0, 5.0);
        assert_eq!(b[0].i[0][1].0, 10.0);
    }

    /// Начальная точка каждого подпути не совпадает с текущей точкой
    /// после предыдущего — иначе rlottie склеит подпути.
    fn assert_no_joined_starts(b: &[Bezier]) {
        let pt = |p: &[crate::lottie::Num; 2]| [p[0].0, p[1].0];
        for w in b.windows(2) {
            let prev = &w[0];
            let cur = if prev.c {
                pt(&prev.v[0])
            } else {
                pt(prev.v.last().unwrap())
            };
            assert!(
                !near(pt(&w[1].v[0]), cur),
                "подпуть начинается в текущей точке"
            );
        }
    }

    #[test]
    fn closed_then_moveto_same_point_is_rotated() {
        // M0 0 h10 v10 Z m0 0 h-10 — второй подпуть стартует в начале первого.
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(10.0, 0.0);
        pb.line_to(10.0, 10.0);
        pb.close();
        pb.move_to(0.0, 0.0);
        pb.line_to(-10.0, 0.0);
        let path = pb.finish().unwrap();
        let b = path_to_beziers(&path, &Transform::identity(), &Rounder::new(2));
        assert_eq!(b.len(), 2);
        assert_no_joined_starts(&b);
        // Открытый отрезок развёрнут: начинается в (−10, 0).
        assert_eq!(b[1].v[0][0].0, -10.0);
    }

    #[test]
    fn open_then_closed_from_end_point_is_rotated() {
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.cubic_to(0.0, 5.0, 5.0, 10.0, 10.0, 10.0);
        pb.move_to(10.0, 10.0);
        pb.line_to(20.0, 10.0);
        pb.line_to(20.0, 20.0);
        pb.close();
        let path = pb.finish().unwrap();
        let b = path_to_beziers(&path, &Transform::identity(), &Rounder::new(2));
        assert_no_joined_starts(&b);
        assert_eq!(b[1].v.len(), 3, "вершины замкнутого подпути сохранены");
    }

    #[test]
    fn reversed_open_curve_keeps_its_shape() {
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(5.0, 0.0);
        pb.close();
        pb.move_to(0.0, 0.0);
        pb.cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 5.0);
        let path = pb.finish().unwrap();
        let b = path_to_beziers(&path, &Transform::identity(), &Rounder::new(2));
        let c = &b[1];
        // Теперь кривая идёт из (5,5) в (0,0): исходящая касательная (5,5)
        // ведёт к (3,4), входящая (0,0) — от (1,2).
        assert_eq!([c.v[0][0].0, c.v[0][1].0], [5.0, 5.0]);
        assert_eq!([c.o[0][0].0, c.o[0][1].0], [-2.0, -1.0]);
        assert_eq!([c.i[1][0].0, c.i[1][1].0], [1.0, 2.0]);
    }

    #[test]
    fn zero_length_straight_segments_are_merged() {
        // Дуга — нулевой отрезок — дуга: средняя вершина-дубликат исчезает,
        // касательные дуг сохраняются.
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.cubic_to(5.0, 0.0, 10.0, 5.0, 10.0, 10.0);
        pb.line_to(10.0, 10.0);
        pb.cubic_to(10.0, 15.0, 5.0, 20.0, 0.0, 20.0);
        let path = pb.finish().unwrap();
        let b = path_to_beziers(&path, &Transform::identity(), &Rounder::new(2));
        assert_eq!(b[0].v.len(), 3);
        assert_eq!([b[0].i[1][0].0, b[0].i[1][1].0], [0.0, -5.0]);
        assert_eq!([b[0].o[1][0].0, b[0].o[1][1].0], [0.0, 5.0]);
    }

    #[test]
    fn dot_keeps_two_vertices() {
        let mut pb = PathBuilder::new();
        pb.move_to(3.0, 3.0);
        pb.line_to(3.0, 3.0);
        let path = pb.finish();
        // tiny-skia может отказаться строить путь без размеров —
        // тогда и проверять нечего.
        if let Some(path) = path {
            let b = path_to_beziers(&path, &Transform::identity(), &Rounder::new(2));
            assert_eq!(b[0].v.len(), 2);
        }
    }

    #[test]
    fn scale_is_detected() {
        assert!(is_similarity(&Transform::from_scale(3.0, 3.0)));
        assert!(is_similarity(&Transform::from_rotate(30.0)));
        assert!(!is_similarity(&Transform::from_scale(2.0, 1.0)));
        assert!(!is_similarity(&Transform::from_skew(0.3, 0.0)));
        assert!((mean_scale(&Transform::from_scale(4.0, 4.0)) - 4.0).abs() < 1e-9);
    }
}

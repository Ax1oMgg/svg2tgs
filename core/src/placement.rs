//! Размещение рисунка на канвасе.
//!
//! Рисунок проходит цепочку шагов, каждый работает с результатом предыдущего:
//! отразить → пропорции → повернуть → обрезать по рамке → вписать в рабочую
//! область (холст минус отступ и обводка) → сдвинуть на заданное положение.
//! Всё, что после этого вышло за рабочую область или за рамку обрезки,
//! отрезается. Итог — одна матрица и один прямоугольник видимой области.
//!
//! Холст — сетка нарезки из `cols × rows` канвасов (без нарезки — один канвас).

use usvg::Transform;

use crate::options::{Bounds, Fit, Grid, Options};

/// Где и как рисуется рисунок.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Из координат SVG в пиксели холста.
    pub transform: Transform,
    /// Видимая область `[x0, y0, x1, y1]` в пикселях холста.
    pub clip: [f64; 4],
}

/// Наименьший видимый остаток после обрезки, доля стороны.
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

/// Рамка рисунка `[x, y, w, h]` в координатах SVG: вся область документа
/// или (`Bounds::Content`) только видимый рисунок с толщиной обводок.
pub fn source_rect(tree: &usvg::Tree, options: &Options) -> [f32; 4] {
    let (dw, dh) = (tree.size().width(), tree.size().height());
    if options.bounds == Bounds::Content {
        let b = tree.root().abs_stroke_bounding_box();
        // Видно только то, что внутри области документа.
        let x0 = b.left().max(0.0);
        let y0 = b.top().max(0.0);
        let x1 = b.right().min(dw);
        let y1 = b.bottom().min(dh);
        if x1 - x0 > 1e-3 && y1 - y0 > 1e-3 {
            return [x0, y0, x1 - x0, y1 - y0];
        }
    }
    [0.0, 0.0, dw, dh]
}

fn finite_or(v: f32, default: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        default
    }
}

/// Пропорции в допустимых пределах.
fn aspect(options: &Options) -> f32 {
    if options.aspect.is_finite() && options.aspect > 0.0 {
        options.aspect.clamp(0.25, 4.0)
    } else {
        1.0
    }
}

/// Размеры холста `(ширина, высота)` для канваса `tile` и сетки.
pub fn board_size(tile: f32, grid: Grid) -> (f32, f32) {
    (tile * grid.cols as f32, tile * grid.rows as f32)
}

/// Соотношение сторон рисунка после трансформаций и обрезки (ширина / высота) —
/// по нему подбирается сетка нарезки.
pub fn shaped_aspect(rect: [f32; 4], options: &Options) -> f32 {
    let (_, crop) = shape(rect, options, None);
    (crop[2] - crop[0]) / (crop[3] - crop[1])
}

/// То же для дерева: в режиме «По краю» — по самому рисунку после поворота.
pub fn tree_aspect(tree: &usvg::Tree, options: &Options) -> f32 {
    let rect = source_rect(tree, options);
    let (_, crop) = shape(rect, options, content_measure(tree, options).as_ref());
    (crop[2] - crop[0]) / (crop[3] - crop[1])
}

/// Размещение рисунка из дерева: рамка по [`source_rect`], а в режиме
/// «По краю» — точные границы рисунка уже после отражения, пропорций
/// и поворота (повёрнутый круг не уменьшается).
pub fn place_tree(tree: &usvg::Tree, tile: f32, grid: Grid, options: &Options) -> Placement {
    let rect = source_rect(tree, options);
    place_with(
        rect,
        tile,
        grid,
        options,
        content_measure(tree, options).as_ref(),
    )
}

/// Измеритель рамки рисунка после матрицы — только для «По краю».
type Measure<'a> = Box<dyn Fn(&Transform) -> Option<[f32; 4]> + 'a>;

fn content_measure<'a>(tree: &'a usvg::Tree, options: &Options) -> Option<Measure<'a>> {
    if options.bounds != Bounds::Content {
        return None;
    }
    let (dw, dh) = (tree.size().width(), tree.size().height());
    Some(Box::new(move |m: &Transform| {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        content_bounds(tree.root(), m, &mut b);
        if b[2] <= b[0] || b[3] <= b[1] {
            return None;
        }
        // Видно только то, что внутри области документа.
        let d = bounds(m, dw, dh);
        let r = [
            b[0].max(d[0]),
            b[1].max(d[1]),
            b[2].min(d[2]),
            b[3].min(d[3]),
        ];
        (r[2] - r[0] > 1e-3 && r[3] - r[1] > 1e-3).then_some(r)
    }))
}

/// Точные границы видимых контуров группы после матрицы `m`, с толщиной обводок.
fn content_bounds(g: &usvg::Group, m: &Transform, b: &mut [f32; 4]) {
    for node in g.children() {
        match node {
            usvg::Node::Group(child) => content_bounds(child, m, b),
            usvg::Node::Text(t) => content_bounds(t.flattened(), m, b),
            usvg::Node::Path(p) if p.is_visible() => {
                let t = m.pre_concat(p.abs_transform());
                let Some(r) = p
                    .data()
                    .clone()
                    .transform(t)
                    .and_then(|d| d.compute_tight_bounds())
                else {
                    continue;
                };
                // Половина обводки — с масштабом матрицы.
                let half = p.stroke().map_or(0.0, |s| {
                    let k = ((t.sx * t.sy - t.kx * t.ky).abs()).sqrt();
                    s.width().get() * k / 2.0
                });
                *b = [
                    b[0].min(r.left() - half),
                    b[1].min(r.top() - half),
                    b[2].max(r.right() + half),
                    b[3].max(r.bottom() + half),
                ];
            }
            _ => {}
        }
    }
}

/// Отразить, растянуть, повернуть — вокруг центра рисунка; затем рамка
/// обрезки в получившихся координатах (центр рисунка — начало координат).
/// `measure` — для «По краю»: рамка самого рисунка после этих шагов.
fn shape(rect: [f32; 4], options: &Options, measure: Option<&Measure>) -> (Transform, [f32; 4]) {
    let [rx, ry, w, h] = rect;
    let fx = if options.flip_h { -1.0 } else { 1.0 };
    let fy = if options.flip_v { -1.0 } else { 1.0 };
    let k = aspect(options);
    let shape = Transform::from_rotate(finite_or(options.rotation, 0.0))
        .pre_scale(k, 1.0)
        .pre_scale(fx, fy)
        .pre_translate(-rx - w / 2.0, -ry - h / 2.0);

    // Рамка повёрнутого рисунка: «По краю» — по самому рисунку,
    // иначе — повёрнутый прямоугольник рамки.
    let b = measure
        .and_then(|f| f(&shape))
        .unwrap_or_else(|| bounds(&shape.pre_translate(rx, ry), w, h));
    let Some(c) = options.crop else {
        return (shape, b);
    };
    // Рамка обрезки — от рамки рисунка без поворота, относительно центра.
    let (w0, h0) = (w * k, h);
    let frac = |v: f32| finite_or(v, 0.0).clamp(-4.0, 1.0);
    let mut f = [
        -w0 / 2.0 + frac(c.left) * w0,
        -h0 / 2.0 + frac(c.top) * h0,
        w0 / 2.0 - frac(c.right) * w0,
        h0 / 2.0 - frac(c.bottom) * h0,
    ];
    // Не шире рисунка и не меньше MIN_KEEP его стороны.
    let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
    let fit = |lo: &mut f32, hi: &mut f32, min: f32, max: f32, keep: f32| {
        *lo = lo.max(min);
        *hi = hi.min(max);
        if *hi - *lo < keep {
            let mid = ((*lo + *hi) / 2.0).clamp(min + keep / 2.0, max - keep / 2.0);
            *lo = mid - keep / 2.0;
            *hi = mid + keep / 2.0;
        }
    };
    let (mut x0, mut x1, mut y0, mut y1) = (f[0], f[2], f[1], f[3]);
    fit(&mut x0, &mut x1, b[0], b[2], (MIN_KEEP * bw).max(1e-6));
    fit(&mut y0, &mut y1, b[1], b[3], (MIN_KEEP * bh).max(1e-6));
    f = [x0, y0, x1, y1];
    (shape, f)
}

/// Размещение рисунка с рамкой `rect` (`[x, y, w, h]`, см. [`source_rect`])
/// на холсте из `grid` канвасов стороной `tile`.
pub fn place(rect: [f32; 4], tile: f32, grid: Grid, options: &Options) -> Placement {
    place_with(rect, tile, grid, options, None)
}

fn place_with(
    rect: [f32; 4],
    tile: f32,
    grid: Grid,
    options: &Options,
    measure: Option<&Measure>,
) -> Placement {
    let (shape, crop) = shape(rect, options, measure);
    let (cw, ch) = ((crop[2] - crop[0]).max(1e-6), (crop[3] - crop[1]).max(1e-6));

    // Вписать в рабочую область: холст минус отступ (доля канваса)
    // и место под обводку.
    let (bw, bh) = board_size(tile, grid);
    let inset = tile * options.padding.clamp(0.0, 0.45) + options.outline_width();
    let area_w = (bw - 2.0 * inset).max(1.0);
    let area_h = (bh - 2.0 * inset).max(1.0);
    let (ox, oy) = ((bw - area_w) / 2.0, (bh - area_h) / 2.0);
    let contain = (area_w / cw).min(area_h / ch);
    let scale = match options.fit {
        Fit::Contain => contain,
        Fit::Cover => (area_w / cw).max(area_h / ch),
        Fit::Width => area_w / cw,
        Fit::Height => area_h / ch,
        Fit::Free => contain * finite_or(options.scale, 1.0).clamp(0.05, 20.0),
    };

    // Положение: доля свободного места (или лишнего, если рисунок больше).
    let pos = |v: f32| finite_or(v, 0.5).clamp(0.0, 1.0);
    let (pw, ph) = (cw * scale, ch * scale);
    let x = ox + (area_w - pw) * pos(options.position_x);
    let y = oy + (area_h - ph) * pos(options.position_y);

    let transform = Transform::from_translate(x, y)
        .pre_scale(scale, scale)
        .pre_translate(-crop[0], -crop[1])
        .pre_concat(shape);

    // Видимое — пересечение рабочей области и обрезанного рисунка.
    let clip = [
        (x as f64).max(ox as f64),
        (y as f64).max(oy as f64),
        ((x + pw) as f64).min((ox + area_w) as f64),
        ((y + ph) as f64).min((oy + area_h) as f64),
    ];
    Placement { transform, clip }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(w: f32, h: f32) -> [f32; 4] {
        [0.0, 0.0, w, h]
    }

    fn map(t: &Transform, x: f32, y: f32) -> (f32, f32) {
        (t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty)
    }

    fn place1(rect: [f32; 4], canvas: f32, o: &Options) -> Placement {
        place(rect, canvas, Grid { cols: 1, rows: 1 }, o)
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn contain_centers_like_before() {
        let p = place1(size(200.0, 100.0), 512.0, &Options::default());
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
        let p = place1(size(100.0, 300.0), 512.0, &o);
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
        let p = place1(size(300.0, 100.0), 500.0, &o);
        assert_eq!(p.clip, [50.0, 50.0, 450.0, 450.0]);
    }

    #[test]
    fn flip_rotate_and_crop() {
        let o = Options {
            flip_h: true,
            ..Options::default()
        };
        let p = place1(size(100.0, 100.0), 100.0, &o);
        assert!(close(map(&p.transform, 0.0, 0.0), (100.0, 0.0)));

        let o = Options {
            rotation: 90.0,
            ..Options::default()
        };
        let p = place1(size(200.0, 100.0), 200.0, &o);
        // После поворота рисунок стоит, левый верх уходит в правый верх.
        assert!(close(map(&p.transform, 0.0, 0.0), (150.0, 0.0)));

        let o = Options {
            crop: Some(crate::options::Crop {
                right: 0.5,
                ..Default::default()
            }),
            ..Options::default()
        };
        let p = place1(size(200.0, 100.0), 100.0, &o);
        // Осталась левая половина (квадрат) — на весь канвас.
        assert!(close(map(&p.transform, 100.0, 100.0), (100.0, 100.0)));
        assert_eq!(p.clip, [0.0, 0.0, 100.0, 100.0]);
    }

    #[test]
    fn aspect_is_applied_before_rotation() {
        // Квадрат, растянутый вдвое по ширине и повёрнутый на 90°, стоит
        // вертикально: высота вдвое больше ширины.
        let o = Options {
            aspect: 2.0,
            rotation: 90.0,
            ..Options::default()
        };
        let a = shaped_aspect(size(100.0, 100.0), &o);
        assert!((a - 0.5).abs() < 1e-4, "{a}");
    }

    #[test]
    fn crop_frame_stays_put_when_rotating() {
        // Рамка — средняя половина рисунка без поворота. При повороте на 30°
        // она остаётся того же размера: повёрнутый рисунок больше рамки.
        let crop = Some(crate::options::Crop {
            top: 0.25,
            right: 0.25,
            bottom: 0.25,
            left: 0.25,
        });
        let still = Options {
            crop,
            ..Options::default()
        };
        let turned = Options {
            rotation: 30.0,
            ..still.clone()
        };
        let (_, a) = shape(size(100.0, 100.0), &still, None);
        let (_, b) = shape(size(100.0, 100.0), &turned, None);
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-4, "{a:?} {b:?}");
        }
        assert!((a[2] - a[0] - 50.0).abs() < 1e-4);
    }

    #[test]
    fn auto_grid_matches_proportions() {
        assert_eq!(Grid::auto(1.6, 8), Grid { cols: 8, rows: 5 });
        assert_eq!(Grid::auto(1.5, 8), Grid { cols: 6, rows: 4 });
        assert_eq!(Grid::auto(1.0, 8), Grid { cols: 8, rows: 8 });
        assert_eq!(Grid::auto(2.0, 8), Grid { cols: 8, rows: 4 });
    }

    #[test]
    fn grid_board_holds_the_whole_drawing() {
        // Рисунок 2 : 1 на сетке 2 × 1 заполняет оба канваса.
        let p = place(
            size(200.0, 100.0),
            512.0,
            Grid { cols: 2, rows: 1 },
            &Options::default(),
        );
        assert!(close(map(&p.transform, 200.0, 100.0), (1024.0, 512.0)));
        assert_eq!(p.clip, [0.0, 0.0, 1024.0, 512.0]);
    }
}

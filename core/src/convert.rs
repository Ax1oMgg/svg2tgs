//! Перевод нормализованного дерева usvg в слои Lottie.
//!
//! Все координаты «запекаются»: каждая точка сразу пересчитывается
//! в пиксели канваса, а группы Lottie получают тождественное преобразование.
//! Так результат не зависит от того, как разные плееры раскладывают матрицы.
//!
//! Обрезка (`clipPath`, края канваса) выполняется геометрически
//! (модуль `clip`): маски Lottie Telegram запрещает.

use std::rc::Rc;

use usvg::{
    BlendMode, FillRule, Group, LineCap, LineJoin, Node, Paint, PaintOrder, Path, SpreadMethod,
    Transform,
};

use crate::clip::{self, Area, Contours, Relation};
use crate::geometry::{is_similarity, map, mean_scale, path_to_beziers};
use crate::lottie::{
    Bezier, Gradient, GradientStops, GroupTransform, Line, Num, Rounder, Shape, Static,
    FILL_RULE_EVENODD, FILL_RULE_NONZERO,
};
use crate::report::{Warning, WarningCode};

/// Слой в процессе сборки: элементы в порядке документа (снизу вверх).
pub struct LayerDraft {
    key: Vec<usize>,
    /// Прозрачность всего слоя, 0–1.
    pub opacity: f64,
    pub items: Vec<Shape>,
}

/// Контекст обхода, наследуемый вниз по дереву.
#[derive(Clone)]
struct Ctx {
    matrix: Transform,
    /// Множитель прозрачности для заливок и обводок внутри текущего слоя.
    opacity: f64,
    /// Идентификаторы прозрачных групп — по ним элементы делятся на слои.
    layer_key: Vec<usize>,
    /// Область, в которой элементы видны: канвас ∩ все активные `clipPath`.
    clip: Rc<Area>,
    /// Прозрачность слоя, если элементы внутри прозрачной группы.
    layer_opacity: Option<f64>,
}

pub struct Converter {
    round: Rounder,
    /// Округление цветов и долей: 3 знака достаточно для 8-битного цвета.
    round_color: Rounder,
    pub layers: Vec<LayerDraft>,
    pub warnings: Vec<Warning>,
    next_layer_id: usize,
}

impl Converter {
    pub fn new(decimals: u8) -> Self {
        Self {
            round: Rounder::new(decimals),
            round_color: Rounder::new(3),
            layers: Vec::new(),
            warnings: Vec::new(),
            next_layer_id: 0,
        }
    }

    /// Обходит дерево, начиная с корня, с матрицей вписывания в канвас
    /// стороной `canvas`. Всё, что выходит за канвас, обрезается:
    /// по правилам Telegram объекты не должны выходить за его пределы.
    pub fn run(&mut self, root: &Group, fit: Transform, canvas: f64) {
        let ctx = Ctx {
            matrix: fit,
            opacity: 1.0,
            layer_key: Vec::new(),
            // Допуск 1 px: выход за край на доли пикселя (сглаживание,
            // контрольные точки у края) не виден, а обрезка спрямляет кривые.
            clip: Rc::new(Area::from_rect([-1.0, -1.0, canvas + 1.0, canvas + 1.0])),
            layer_opacity: None,
        };
        self.walk_group_children(root, &ctx);
    }

    /// Есть ли хоть один нарисованный элемент.
    pub fn is_empty(&self) -> bool {
        self.layers.iter().all(|l| l.items.is_empty())
    }

    fn warn(&mut self, code: WarningCode, id: &str) {
        let w = Warning {
            code,
            element_id: (!id.is_empty()).then(|| id.to_string()),
        };
        if !self.warnings.contains(&w) {
            self.warnings.push(w);
        }
    }

    fn walk_group_children(&mut self, g: &Group, ctx: &Ctx) {
        for node in g.children() {
            match node {
                Node::Group(child) => self.walk_group(child, ctx),
                Node::Path(p) => self.draw_path(p, ctx),
                Node::Image(img) => self.warn(WarningCode::ImageRemoved, img.id()),
                Node::Text(text) => {
                    let flat = text.flattened();
                    if flat.has_children() {
                        self.warn(WarningCode::TextConvertedToPaths, text.id());
                        self.walk_group(flat, ctx);
                    } else {
                        self.warn(WarningCode::TextDropped, text.id());
                    }
                }
            }
        }
    }

    fn walk_group(&mut self, g: &Group, parent: &Ctx) {
        let mut ctx = parent.clone();
        ctx.matrix = parent.matrix.pre_concat(g.transform());
        let group_opacity = g.opacity().get() as f64;
        if group_opacity <= 0.0 || ctx.opacity <= 0.0 {
            return;
        }
        // Прозрачность группы в SVG применяется к группе целиком: перекрытия
        // внутри неё не темнеют. Если просто умножить её на прозрачность
        // каждого элемента, перекрытия потемнеют. Поэтому группа с несколькими
        // элементами выносится в отдельный слой с прозрачностью слоя —
        // rlottie смешивает такой слой честно. Вложить слой в слой без
        // прекомпозиций нельзя, поэтому внутри такого слоя — умножение.
        if group_opacity < 1.0 && ctx.layer_opacity.is_none() && paint_count(g, 2) >= 2 {
            ctx.layer_key.push(self.new_layer_id());
            ctx.layer_opacity = Some(ctx.opacity * group_opacity);
            ctx.opacity = 1.0;
        } else {
            ctx.opacity *= group_opacity;
        }

        // Один фильтр обычно состоит из нескольких примитивов (тень из Figma —
        // это смещение, размытие, цвет и смешивание), поэтому на фильтр
        // выдаётся одно замечание с самым узнаваемым эффектом.
        for filter in g.filters() {
            let names: Vec<&str> = filter
                .primitives()
                .iter()
                .map(|p| filter_name(p.kind()))
                .collect();
            let main = ["drop_shadow", "blur"]
                .into_iter()
                .find(|n| names.contains(n))
                .or_else(|| names.first().copied())
                .unwrap_or("filter");
            self.warn(
                WarningCode::FilterRemoved {
                    filter: main.to_string(),
                },
                label(g),
            );
        }
        if g.mask().is_some() {
            self.warn(WarningCode::MaskRemoved, label(g));
        }
        if g.blend_mode() != BlendMode::Normal {
            let mode = format!("{:?}", g.blend_mode()).to_lowercase();
            self.warn(WarningCode::BlendModeIgnored { mode }, label(g));
        }

        if let Some(clip) = g.clip_path() {
            self.push_clip(clip, &mut ctx);
        }

        self.walk_group_children(g, &ctx);
    }

    fn new_layer_id(&mut self) -> usize {
        self.next_layer_id += 1;
        self.next_layer_id
    }

    /// Сужает видимую область контекста до `clipPath`
    /// (и вложенного в него `clip-path`, если есть).
    fn push_clip(&mut self, clip_path: &usvg::ClipPath, ctx: &mut Ctx) {
        let m = ctx.matrix.pre_concat(clip_path.transform());
        let mut area = Contours::new();
        self.collect_clip_region(clip_path.root(), m, &mut area);
        ctx.clip = Rc::new(ctx.clip.intersect(&area));
        if let Some(inner) = clip_path.clip_path() {
            self.push_clip(inner, ctx);
        }
    }

    /// Объединение всех фигур `clipPath` с их правилами `clip-rule`.
    fn collect_clip_region(&mut self, g: &Group, m: Transform, out: &mut Contours) {
        for node in g.children() {
            match node {
                Node::Group(child) => {
                    if child.clip_path().is_some() {
                        self.warn(WarningCode::ClipApproximated, child.id());
                    }
                    self.collect_clip_region(child, m.pre_concat(child.transform()), out);
                }
                Node::Path(p) if p.is_visible() => {
                    let rule = match p.fill().map(|f| f.rule()) {
                        Some(FillRule::EvenOdd) => FILL_RULE_EVENODD,
                        _ => FILL_RULE_NONZERO,
                    };
                    let beziers = path_to_beziers(p.data(), &m, &self.round);
                    *out = clip::union(out, &clip::region(&beziers, rule));
                }
                Node::Text(t) => {
                    let flat = t.flattened();
                    self.collect_clip_region(flat, m.pre_concat(flat.transform()), out);
                }
                _ => {}
            }
        }
    }

    fn draw_path(&mut self, p: &Path, ctx: &Ctx) {
        if !p.is_visible() {
            return;
        }
        let m = ctx.matrix;
        let id = p.id();
        let base = path_to_beziers(p.data(), &m, &self.round);
        if base.is_empty() {
            return;
        }
        match self.relation(&base, p, ctx) {
            Relation::Outside => return,
            Relation::Crossing => return self.draw_clipped(p, ctx),
            Relation::Inside => {}
        }

        // У отрезка нет площади: заливка ничего не рисует, только занимает место.
        let has_area = base.iter().any(|b| {
            b.v.len() > 2
                || b.i
                    .iter()
                    .chain(&b.o)
                    .any(|t| t[0].0 != 0.0 || t[1].0 != 0.0)
        });
        let fill = p
            .fill()
            .filter(|_| has_area)
            .and_then(|f| self.fill_shape(f, ctx, id));
        let stroke = match p.stroke() {
            Some(s) => self.stroke_out(p, s, ctx),
            None => StrokeOut::None,
        };

        // В Lottie стиль, стоящий раньше в списке, рисуется поверх;
        // в документе (и в `push_item`) — наоборот, позже значит выше.
        let stroke_on_top = p.paint_order() == PaintOrder::FillAndStroke;
        let mut stroke_groups: Vec<Shape> = Vec::new();
        let mut shared: Option<(Vec<Bezier>, Shape)> = None;
        match stroke {
            StrokeOut::None => {}
            StrokeOut::Lines {
                lines,
                dots,
                style,
                same_as_fill,
            } => {
                if let Some(style) = style.map(|s| *s) {
                    if same_as_fill {
                        shared = Some((lines, style));
                    } else {
                        stroke_groups.push(group(id, lines, vec![style]));
                    }
                }
                if let Some(s) = p.stroke().filter(|_| !dots.is_empty()) {
                    if let Some(g) = self.dots_group(&dots, s, ctx, id) {
                        stroke_groups.push(g);
                    }
                }
            }
            StrokeOut::Outline(outline) => {
                if let Some(s) = p.stroke() {
                    let f =
                        self.paint_fill(s.paint(), s.opacity().get(), FILL_RULE_NONZERO, ctx, id);
                    if let Some(f) = f {
                        stroke_groups.push(group(id, outline, vec![f]));
                    }
                }
            }
        }

        if let Some((lines, style)) = shared {
            // Одна геометрия — одна группа с обоими стилями.
            let styles: Vec<Shape> = if stroke_on_top {
                std::iter::once(style).chain(fill).collect()
            } else {
                fill.into_iter().chain(std::iter::once(style)).collect()
            };
            let g = group(id, lines, styles);
            self.push_item(g, ctx);
            for g in stroke_groups {
                self.push_item(g, ctx);
            }
            return;
        }

        let fill_group = fill.map(|f| {
            let geometry = base.into_iter().filter(|b| !is_zero_length(b)).collect();
            group(id, geometry, vec![f])
        });
        let ordered: Vec<Shape> = if stroke_on_top {
            fill_group.into_iter().chain(stroke_groups).collect()
        } else {
            stroke_groups.into_iter().chain(fill_group).collect()
        };
        for g in ordered {
            self.push_item(g, ctx);
        }
    }

    /// Положение элемента относительно видимой области — по рамке
    /// с запасом на толщину обводки, острые углы и концы линий.
    fn relation(&self, base: &[Bezier], p: &Path, ctx: &Ctx) -> Relation {
        // Точные границы кривой (по экстремумам), а не рамка контрольных точек.
        let tight = p
            .data()
            .clone()
            .transform(ctx.matrix)
            .and_then(|t| t.compute_tight_bounds())
            .map(|r| {
                [
                    r.left() as f64,
                    r.top() as f64,
                    r.right() as f64,
                    r.bottom() as f64,
                ]
            });
        let Some(mut b) = tight.or_else(|| clip::bbox(base)) else {
            return Relation::Outside;
        };
        if let Some(s) = p.stroke() {
            let half = s.width().get() as f64 * mean_scale(&ctx.matrix) / 2.0;
            let reach = match s.linejoin() {
                LineJoin::Miter | LineJoin::MiterClip => (s.miterlimit().get() as f64).max(1.5),
                _ => 1.5,
            };
            let e = half * reach;
            b = [b[0] - e, b[1] - e, b[2] + e, b[3] + e];
        }
        let rough = ctx.clip.relate(b);
        // Оценка с запасом говорит «пересекает» — уточняем по реальному
        // контуру обводки, чтобы не резать то, что на самом деле внутри.
        if rough != Relation::Crossing {
            return rough;
        }
        let Some(s) = p.stroke() else {
            return rough;
        };
        let Some(exact) = self.stroke_outline(p, s, ctx).and_then(|o| clip::bbox(&o)) else {
            return rough;
        };
        let fill = clip::bbox(base).filter(|_| p.fill().is_some());
        let b = match fill {
            Some(f) => [
                f[0].min(exact[0]),
                f[1].min(exact[1]),
                f[2].max(exact[2]),
                f[3].max(exact[3]),
            ],
            None => exact,
        };
        ctx.clip.relate(b)
    }

    /// Элемент, пересекающий границу видимой области: заливка и обводка
    /// превращаются в области, пересекаются с ней и рисуются заливкой.
    fn draw_clipped(&mut self, p: &Path, ctx: &Ctx) {
        let id = p.id();
        let fill = p.fill().and_then(|f| {
            let rule = match f.rule() {
                FillRule::NonZero => FILL_RULE_NONZERO,
                FillRule::EvenOdd => FILL_RULE_EVENODD,
            };
            let base = path_to_beziers(p.data(), &ctx.matrix, &self.round);
            let area = clip::intersect(&clip::region(&base, rule), &ctx.clip.contours);
            if area.is_empty() {
                return None;
            }
            let style =
                self.paint_fill(f.paint(), f.opacity().get(), FILL_RULE_NONZERO, ctx, id)?;
            Some(group(id, clip::to_beziers(&area, &self.round), vec![style]))
        });
        let stroke = p.stroke().and_then(|s| {
            let outline = self.stroke_outline(p, s, ctx)?;
            let area = clip::intersect(
                &clip::region(&outline, FILL_RULE_NONZERO),
                &ctx.clip.contours,
            );
            if area.is_empty() {
                return None;
            }
            let style =
                self.paint_fill(s.paint(), s.opacity().get(), FILL_RULE_NONZERO, ctx, id)?;
            Some(group(id, clip::to_beziers(&area, &self.round), vec![style]))
        });
        let (bottom, top) = match p.paint_order() {
            PaintOrder::FillAndStroke => (fill, stroke),
            PaintOrder::StrokeAndFill => (stroke, fill),
        };
        for g in bottom.into_iter().chain(top) {
            self.push_item(g, ctx);
        }
    }

    /// Обводка как залитый контур (с пунктиром), в координатах канваса.
    fn stroke_outline(&self, p: &Path, s: &usvg::Stroke, ctx: &Ctx) -> Option<Vec<Bezier>> {
        let resolution = mean_scale(&ctx.matrix).max(1.0) as f32;
        let dashed = match dash_path(p.data(), s, resolution) {
            Dashing::Solid => None,
            Dashing::Dashed(path) => Some(path),
            Dashing::Invisible => return None,
        };
        let source = dashed.as_ref().unwrap_or(p.data());
        let outline = source.stroke(&s.to_tiny_skia(), resolution)?;
        Some(path_to_beziers(&outline, &ctx.matrix, &self.round))
    }

    /// Готовит обводку.
    ///
    /// - Пунктир нарезается здесь, а не в плеере: rlottie неточно меряет
    ///   длину кривых, и на дугах штрихи «уплывают».
    /// - Под неравномерным растяжением (`scale(2 1)`, перекос) толщина линии
    ///   в SVG разная в разных направлениях, а rlottie умеет только одинаковую.
    ///   Такая обводка превращается в залитый контур, построенный в исходных
    ///   координатах, — после растяжения он совпадает с SVG.
    /// - Градиентные обводки Telegram запрещает — они тоже становятся
    ///   контуром с градиентной заливкой.
    /// - Отрезки нулевой длины со скруглённым или квадратным концом в SVG
    ///   рисуются точкой (точки в иконках, бусины в пунктире «0 6»);
    ///   rlottie их не рисует, они уходят в `dots`.
    fn stroke_out(&mut self, p: &Path, s: &usvg::Stroke, ctx: &Ctx) -> StrokeOut {
        let m = ctx.matrix;
        let resolution = mean_scale(&m).max(1.0) as f32;
        let dashed = match dash_path(p.data(), s, resolution) {
            Dashing::Solid => None,
            Dashing::Dashed(path) => Some(path),
            Dashing::Invisible => return StrokeOut::None,
        };
        let source = dashed.as_ref().unwrap_or(p.data());

        let gradient = !matches!(s.paint(), Paint::Color(_) | Paint::Pattern(_));
        if !is_similarity(&m) || gradient {
            return match self.stroke_outline(p, s, ctx) {
                Some(outline) => StrokeOut::Outline(outline),
                None => StrokeOut::None,
            };
        }

        let (dots, lines): (Vec<Bezier>, Vec<Bezier>) = path_to_beziers(source, &m, &self.round)
            .into_iter()
            .partition(is_zero_length);
        let style = if lines.is_empty() {
            None
        } else {
            self.stroke_shape(s, ctx, p.id()).map(Box::new)
        };
        StrokeOut::Lines {
            lines,
            dots,
            style,
            same_as_fill: dashed.is_none(),
        }
    }

    /// Точки из отрезков нулевой длины: круг (round) или квадрат (square)
    /// со стороной, равной толщине обводки, залитый её цветом.
    fn dots_group(
        &mut self,
        dots: &[Bezier],
        s: &usvg::Stroke,
        ctx: &Ctx,
        id: &str,
    ) -> Option<Shape> {
        let cap = s.linecap();
        if cap == LineCap::Butt {
            return None;
        }
        let m = ctx.matrix;
        let half = s.width().get() as f64 * mean_scale(&m) / 2.0;
        // Квадратный конец повёрнут вдоль оси x пользовательской системы.
        let len = ((m.sx as f64).powi(2) + (m.ky as f64).powi(2))
            .sqrt()
            .max(f64::EPSILON);
        let axis = [m.sx as f64 / len, m.ky as f64 / len];

        let mut it: Vec<Shape> = dots
            .iter()
            .map(|d| {
                let c = [d.v[0][0].0, d.v[0][1].0];
                let b = match cap {
                    LineCap::Square => square(c, half, axis, &self.round),
                    _ => circle(c, half, &self.round),
                };
                Shape::Path { ks: Static::new(b) }
            })
            .collect();
        it.push(self.paint_fill(s.paint(), s.opacity().get(), FILL_RULE_NONZERO, ctx, id)?);
        it.push(Shape::Transform(GroupTransform::default()));
        Some(Shape::Group {
            nm: (!id.is_empty()).then(|| id.to_string()),
            it,
        })
    }

    fn push_item(&mut self, item: Shape, ctx: &Ctx) {
        let same_layer = self.layers.last().is_some_and(|l| l.key == ctx.layer_key);
        if !same_layer {
            self.layers.push(LayerDraft {
                key: ctx.layer_key.clone(),
                opacity: ctx.layer_opacity.unwrap_or(1.0),
                items: Vec::new(),
            });
        }
        self.layers.last_mut().unwrap().items.push(item);
    }

    fn opacity(&self, paint_opacity: f32, ctx: &Ctx) -> Static<Num> {
        Static::new(
            self.round_color
                .num(paint_opacity as f64 * ctx.opacity * 100.0),
        )
    }

    fn fill_shape(&mut self, f: &usvg::Fill, ctx: &Ctx, id: &str) -> Option<Shape> {
        let r = match f.rule() {
            FillRule::NonZero => FILL_RULE_NONZERO,
            FillRule::EvenOdd => FILL_RULE_EVENODD,
        };
        self.paint_fill(f.paint(), f.opacity().get(), r, ctx, id)
    }

    /// Заливка краской `paint`: сплошной цвет или градиент.
    fn paint_fill(
        &mut self,
        paint: &Paint,
        opacity: f32,
        r: u8,
        ctx: &Ctx,
        id: &str,
    ) -> Option<Shape> {
        let o = self.opacity(opacity, ctx);
        if o.k_value() <= 0.0 {
            return None;
        }
        Some(match self.paint(paint, ctx, id) {
            PaintOut::Solid(c) => Shape::Fill {
                c: Static::new(c),
                o,
                r,
            },
            PaintOut::Gradient(g) => Shape::GradientFill { o, r, g },
        })
    }

    fn stroke_shape(&mut self, s: &usvg::Stroke, ctx: &Ctx, id: &str) -> Option<Shape> {
        let o = self.opacity(s.opacity().get(), ctx);
        if o.k_value() <= 0.0 {
            return None;
        }
        let scale = mean_scale(&ctx.matrix);
        let width = s.width().get() as f64 * scale;
        if width <= 0.0 {
            return None;
        }

        let lj = match s.linejoin() {
            LineJoin::Miter | LineJoin::MiterClip => 1,
            LineJoin::Round => 2,
            LineJoin::Bevel => 3,
        };
        let line = Line {
            w: Static::new(self.round.num(width)),
            lc: match s.linecap() {
                LineCap::Butt => 1,
                LineCap::Round => 2,
                LineCap::Square => 3,
            },
            lj,
            ml: (lj == 1).then(|| self.round.num(s.miterlimit().get() as f64)),
        };

        Some(match self.paint(s.paint(), ctx, id) {
            PaintOut::Solid(c) => Shape::Stroke {
                c: Static::new(c),
                o,
                line,
            },
            // Градиентные обводки Telegram запрещает: они заранее
            // превращаются в контур (`stroke_out`) и сюда не попадают.
            PaintOut::Gradient(_) => return None,
        })
    }

    fn color(&self, c: usvg::Color) -> [Num; 4] {
        let ch = |v: u8| self.round_color.num(v as f64 / 255.0);
        [ch(c.red), ch(c.green), ch(c.blue), Num(1.0)]
    }

    fn paint(&mut self, paint: &Paint, ctx: &Ctx, id: &str) -> PaintOut {
        match paint {
            Paint::Color(c) => PaintOut::Solid(self.color(*c)),
            Paint::Pattern(_) => {
                self.warn(WarningCode::PatternReplaced, id);
                PaintOut::Solid(self.color(usvg::Color::new_rgb(128, 128, 128)))
            }
            Paint::LinearGradient(lg) => {
                let m = ctx.matrix.pre_concat(lg.transform());
                if lg.spread_method() != SpreadMethod::Pad {
                    self.warn(WarningCode::GradientApproximated, id);
                }
                let s = map(&m, pt(lg.x1(), lg.y1()));
                let e = linear_gradient_end(&m, s, lg.x2() - lg.x1(), lg.y2() - lg.y1());
                PaintOut::Gradient(Gradient {
                    t: 1,
                    s: Static::new(self.round.pt(s)),
                    e: Static::new(self.round.pt(e)),
                    g: self.stops(lg.stops()),
                    h: None,
                    a: None,
                })
            }
            Paint::RadialGradient(rg) => {
                let m = ctx.matrix.pre_concat(rg.transform());
                if !is_similarity(&m)
                    || rg.spread_method() != SpreadMethod::Pad
                    || rg.fr().get() > 0.0
                {
                    self.warn(WarningCode::GradientApproximated, id);
                }
                let r = rg.r().get();
                let c = map(&m, pt(rg.cx(), rg.cy()));
                let e = map(&m, pt(rg.cx() + r, rg.cy()));
                let f = map(&m, pt(rg.fx(), rg.fy()));
                let radius = dist(c, e);
                let (h, a) = if radius > 0.0 && dist(c, f) > 1e-6 {
                    let h = (dist(c, f) / radius * 100.0).min(99.0);
                    let a = angle_deg(c, f) - angle_deg(c, e);
                    (
                        Some(Static::new(self.round.num(h))),
                        Some(Static::new(self.round.num(a))),
                    )
                } else {
                    (None, None)
                };
                PaintOut::Gradient(Gradient {
                    t: 2,
                    s: Static::new(self.round.pt(c)),
                    e: Static::new(self.round.pt(e)),
                    g: self.stops(rg.stops()),
                    h,
                    a,
                })
            }
        }
    }

    fn stops(&self, stops: &[usvg::Stop]) -> GradientStops {
        let mut k = Vec::with_capacity(stops.len() * 6);
        for s in stops {
            let [r, g, b, _] = self.color(s.color());
            k.extend([self.round_color.num(s.offset().get() as f64), r, g, b]);
        }
        if stops.iter().any(|s| s.opacity().get() < 1.0) {
            for s in stops {
                k.push(self.round_color.num(s.offset().get() as f64));
                k.push(self.round_color.num(s.opacity().get() as f64));
            }
        }
        GradientStops {
            p: stops.len(),
            k: Static::new(k),
        }
    }
}

/// Группа Lottie: кривые, стили и тождественное преобразование.
fn group(id: &str, beziers: Vec<Bezier>, styles: Vec<Shape>) -> Shape {
    let mut it: Vec<Shape> = beziers
        .into_iter()
        .map(|b| Shape::Path { ks: Static::new(b) })
        .collect();
    it.extend(styles);
    it.push(Shape::Transform(GroupTransform::default()));
    Shape::Group {
        nm: (!id.is_empty()).then(|| id.to_string()),
        it,
    }
}

/// Подпуть, все точки и касательные которого совпадают, — отрезок нулевой длины.
fn is_zero_length(b: &Bezier) -> bool {
    let first = b.v[0];
    b.v.iter()
        .all(|v| v[0].0 == first[0].0 && v[1].0 == first[1].0)
        && b.i
            .iter()
            .chain(&b.o)
            .all(|t| t[0].0 == 0.0 && t[1].0 == 0.0)
}

/// Окружность радиуса `r` из четырёх кубических кривых.
fn circle(c: [f64; 2], r: f64, round: &Rounder) -> Bezier {
    const K: f64 = 0.552_284_749_830_793_6;
    let k = r * K;
    let v = [
        [c[0], c[1] - r],
        [c[0] + r, c[1]],
        [c[0], c[1] + r],
        [c[0] - r, c[1]],
    ];
    let o = [[k, 0.0], [0.0, k], [-k, 0.0], [0.0, -k]];
    Bezier {
        c: true,
        v: v.iter().map(|p| round.pt(*p)).collect(),
        i: o.iter().map(|t| round.pt([-t[0], -t[1]])).collect(),
        o: o.iter().map(|t| round.pt(*t)).collect(),
    }
}

/// Квадрат с половиной стороны `h`, повёрнутый вдоль единичного вектора `axis`.
fn square(c: [f64; 2], h: f64, axis: [f64; 2], round: &Rounder) -> Bezier {
    let (ux, uy) = (axis[0] * h, axis[1] * h);
    let (vx, vy) = (-uy, ux);
    let v = [
        [c[0] - ux - vx, c[1] - uy - vy],
        [c[0] + ux - vx, c[1] + uy - vy],
        [c[0] + ux + vx, c[1] + uy + vy],
        [c[0] - ux + vx, c[1] - uy + vy],
    ];
    Bezier {
        c: true,
        v: v.iter().map(|p| round.pt(*p)).collect(),
        i: vec![[Num(0.0); 2]; 4],
        o: vec![[Num(0.0); 2]; 4],
    }
}

/// Обводка, подготовленная к записи в Lottie.
enum StrokeOut {
    None,
    /// Линии со стилем обводки и точки из отрезков нулевой длины.
    /// `same_as_fill` — геометрия совпадает с заливкой (нет пунктира),
    /// оба стиля можно положить в одну группу.
    Lines {
        lines: Vec<Bezier>,
        dots: Vec<Bezier>,
        style: Option<Box<Shape>>,
        same_as_fill: bool,
    },
    /// Обводка, превращённая в залитый контур.
    Outline(Vec<Bezier>),
}

enum Dashing {
    Solid,
    Dashed(usvg::tiny_skia_path::Path),
    /// Все штрихи попали в промежутки.
    Invisible,
}

/// Нарезает пунктир в исходных координатах. Вырожденный пунктир
/// (сумма длин 0) по правилам SVG рисуется сплошной линией.
fn dash_path(data: &usvg::tiny_skia_path::Path, s: &usvg::Stroke, resolution: f32) -> Dashing {
    let Some(array) = s.dasharray() else {
        return Dashing::Solid;
    };
    let mut array = array.to_vec();
    if array.len() % 2 == 1 {
        array.extend_from_within(..);
    }
    let Some(dash) = usvg::tiny_skia_path::StrokeDash::new(array, s.dashoffset()) else {
        return Dashing::Solid;
    };
    match data.dash(&dash, resolution) {
        Some(path) => Dashing::Dashed(path),
        None => Dashing::Invisible,
    }
}

enum PaintOut {
    Solid([Num; 4]),
    Gradient(Gradient),
}

fn pt(x: f32, y: f32) -> usvg::tiny_skia_path::Point {
    usvg::tiny_skia_path::Point::from_xy(x, y)
}

/// Конечная точка линейного градиента после преобразования `m`.
///
/// Просто перенести обе точки нельзя: при растяжении или перекосе линии
/// одинакового цвета перестают быть перпендикулярны отрезку s→e, а Lottie
/// считает именно так. Значение градиента — линейная функция точки,
/// g(q) = (q − s′)·u, где u = A⁻ᵀ·d / |d|² (A — линейная часть `m`,
/// d — вектор градиента). Lottie даёт ту же функцию при e = s′ + u / |u|².
fn linear_gradient_end(m: &Transform, s: [f64; 2], dx: f32, dy: f32) -> [f64; 2] {
    let (dx, dy) = (dx as f64, dy as f64);
    let len2 = dx * dx + dy * dy;
    let (a, b, c, d) = (m.sx as f64, m.kx as f64, m.ky as f64, m.sy as f64);
    let det = a * d - b * c;
    if len2 == 0.0 || det.abs() < 1e-12 {
        return s;
    }
    // A = [[a, b], [c, d]] (x′ = a·x + b·y), A⁻ᵀ = 1/det · [[d, −c], [−b, a]].
    let ux = (d * dx - c * dy) / det / len2;
    let uy = (-b * dx + a * dy) / det / len2;
    let u2 = ux * ux + uy * uy;
    [s[0] + ux / u2, s[1] + uy / u2]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn angle_deg(from: [f64; 2], to: [f64; 2]) -> f64 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

/// Сколько заливок и обводок внутри группы (считает до `limit`).
/// Заливка и обводка одного элемента тоже перекрываются, поэтому
/// элемент с обоими считается за два.
fn paint_count(g: &Group, limit: usize) -> usize {
    let mut n = 0;
    for node in g.children() {
        n += match node {
            Node::Path(p) if p.is_visible() => {
                p.fill().is_some() as usize + p.stroke().is_some() as usize
            }
            Node::Group(child) => paint_count(child, limit - n.min(limit)),
            Node::Text(t) => paint_count(t.flattened(), limit - n.min(limit)),
            _ => 0,
        };
        if n >= limit {
            break;
        }
    }
    n
}

/// Имя группы для замечаний: её `id`, а если его нет — `id` первого
/// дочернего элемента (usvg переносит `id` элемента с фильтром внутрь).
fn label(g: &Group) -> &str {
    if !g.id().is_empty() {
        return g.id();
    }
    g.children()
        .iter()
        .map(|n| n.id())
        .find(|id| !id.is_empty())
        .unwrap_or("")
}

/// Короткое имя эффекта фильтра для замечания.
fn filter_name(kind: &usvg::filter::Kind) -> &'static str {
    use usvg::filter::Kind;
    match kind {
        Kind::GaussianBlur(_) => "blur",
        Kind::DropShadow(_) => "drop_shadow",
        Kind::Offset(_) => "offset",
        Kind::Blend(_) => "blend",
        Kind::ColorMatrix(_) => "color_matrix",
        Kind::ComponentTransfer(_) => "component_transfer",
        Kind::Composite(_) => "composite",
        Kind::ConvolveMatrix(_) => "convolve_matrix",
        Kind::DiffuseLighting(_) | Kind::SpecularLighting(_) => "lighting",
        Kind::DisplacementMap(_) => "displacement_map",
        Kind::Flood(_) => "flood",
        Kind::Image(_) => "image",
        Kind::Merge(_) => "merge",
        Kind::Morphology(_) => "morphology",
        Kind::Tile(_) => "tile",
        Kind::Turbulence(_) => "turbulence",
    }
}

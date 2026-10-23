//! Подмножество формата Lottie, которое нужно для статичных рисунков.
//!
//! Структуры сериализуются ровно в тот JSON, который ожидают rlottie
//! (движок Telegram) и lottie-web. Все значения статичные (`"a": 0`).

use serde::{Serialize, Serializer};

/// Число в JSON: целые значения пишутся без `.0`, чтобы не тратить байты.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Num(pub f64);

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = self.0;
        if v.fract() == 0.0 && v.abs() < 1e15 {
            s.serialize_i64(v as i64)
        } else {
            s.serialize_f64(v)
        }
    }
}

/// Точность записи геометрии: округление до заданного числа знаков
/// (`-0` превращается в `0`) и допуск упрощения контуров в пикселях.
#[derive(Debug, Clone, Copy)]
pub struct Rounder {
    factor: f64,
    /// Насколько упрощённый контур может отойти от исходного, пиксели.
    /// 0 — не упрощать.
    pub tolerance: f64,
}

impl Rounder {
    pub fn new(decimals: u8) -> Self {
        Self {
            factor: 10f64.powi(decimals.min(6) as i32),
            tolerance: 0.0,
        }
    }

    pub fn with_tolerance(decimals: u8, tolerance: f64) -> Self {
        Self {
            tolerance,
            ..Self::new(decimals)
        }
    }

    pub fn num(&self, v: f64) -> Num {
        let r = (v * self.factor).round() / self.factor;
        Num(if r == 0.0 { 0.0 } else { r })
    }

    pub fn pt(&self, p: [f64; 2]) -> [Num; 2] {
        [self.num(p[0]), self.num(p[1])]
    }
}

/// Статичное (не анимированное) свойство.
#[derive(Debug, Clone, Serialize)]
pub struct Static<T> {
    a: u8,
    k: T,
}

impl<T> Static<T> {
    pub fn new(k: T) -> Self {
        Self { a: 0, k }
    }

    pub fn get(&self) -> &T {
        &self.k
    }
}

impl Static<Num> {
    pub fn k_value(&self) -> f64 {
        self.k.0
    }
}

#[derive(Debug, Serialize)]
pub struct Animation {
    pub v: &'static str,
    pub fr: Num,
    pub ip: Num,
    pub op: Num,
    pub w: u32,
    pub h: u32,
    pub nm: &'static str,
    pub ddd: u8,
    pub assets: [(); 0],
    pub layers: Vec<Layer>,
    /// Признак TGS, его ставит официальный экспортёр Telegram.
    pub tgs: u8,
}

#[derive(Debug, Serialize)]
pub struct LayerTransform {
    o: Static<Num>,
    r: Static<Num>,
    p: Static<[Num; 3]>,
    a: Static<[Num; 3]>,
    s: Static<[Num; 3]>,
}

impl LayerTransform {
    /// Прозрачность слоя, 0–100.
    pub fn set_opacity(&mut self, o: Num) {
        self.o = Static::new(o);
    }
}

impl Default for LayerTransform {
    fn default() -> Self {
        let zero = [Num(0.0); 3];
        Self {
            o: Static::new(Num(100.0)),
            r: Static::new(Num(0.0)),
            p: Static::new(zero),
            a: Static::new(zero),
            s: Static::new([Num(100.0); 3]),
        }
    }
}

/// Слой с фигурами (`ty: 4`).
#[derive(Debug, Serialize)]
pub struct Layer {
    pub ddd: u8,
    pub ind: u32,
    pub ty: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nm: Option<String>,
    pub sr: u8,
    pub ks: LayerTransform,
    pub ao: u8,
    pub shapes: Vec<Shape>,
    pub ip: Num,
    pub op: Num,
    pub st: u8,
    pub bm: u8,
}

impl Layer {
    pub fn new(ind: u32, frames: u32, shapes: Vec<Shape>) -> Self {
        Self {
            ddd: 0,
            ind,
            ty: 4,
            nm: None,
            sr: 1,
            ks: LayerTransform::default(),
            ao: 0,
            shapes,
            ip: Num(0.0),
            op: Num(frames as f64),
            st: 0,
            bm: 0,
        }
    }
}

/// Кривая Безье: вершины и касательные (смещения от вершины).
#[derive(Debug, Clone, Serialize)]
pub struct Bezier {
    pub c: bool,
    pub i: Vec<[Num; 2]>,
    pub o: Vec<[Num; 2]>,
    pub v: Vec<[Num; 2]>,
}

/// Элемент внутри слоя или группы.
#[derive(Debug, Serialize)]
#[serde(tag = "ty")]
pub enum Shape {
    #[serde(rename = "gr")]
    Group {
        #[serde(skip_serializing_if = "Option::is_none")]
        nm: Option<String>,
        it: Vec<Shape>,
    },
    #[serde(rename = "sh")]
    Path { ks: Static<Bezier> },
    #[serde(rename = "fl")]
    Fill {
        c: Static<[Num; 4]>,
        o: Static<Num>,
        r: u8,
    },
    #[serde(rename = "gf")]
    GradientFill {
        o: Static<Num>,
        r: u8,
        #[serde(flatten)]
        g: Gradient,
    },
    #[serde(rename = "st")]
    Stroke {
        c: Static<[Num; 4]>,
        o: Static<Num>,
        #[serde(flatten)]
        line: Line,
    },
    #[serde(rename = "tr")]
    Transform(GroupTransform),
}

impl Shape {
    /// Сколько точек получит rlottie, собрав все пути этой группы в одну
    /// фигуру: по 3 на вершину (вершина и две контрольные), плюс замыкание
    /// и начало подпути.
    pub fn rlottie_points(&self) -> usize {
        match self {
            Shape::Group { it, .. } => it.iter().map(Shape::rlottie_points).sum(),
            Shape::Path { ks } => 3 * ks.get().v.len() + 5,
            _ => 0,
        }
    }
}

/// Заливка: правило заполнения в терминах Lottie.
pub const FILL_RULE_NONZERO: u8 = 1;
pub const FILL_RULE_EVENODD: u8 = 2;

/// Общие параметры градиента.
#[derive(Debug, Serialize)]
pub struct Gradient {
    /// 1 — линейный, 2 — радиальный.
    pub t: u8,
    pub s: Static<[Num; 2]>,
    pub e: Static<[Num; 2]>,
    pub g: GradientStops,
    /// Смещение фокуса радиального градиента, % радиуса.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<Static<Num>>,
    /// Угол смещения фокуса, градусы.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Static<Num>>,
}

/// Остановки градиента: сначала `p` цветов (offset, r, g, b),
/// затем, если есть прозрачность, пары (offset, alpha).
#[derive(Debug, Serialize)]
pub struct GradientStops {
    pub p: usize,
    pub k: Static<Vec<Num>>,
}

/// Параметры линии обводки.
#[derive(Debug, Serialize)]
pub struct Line {
    pub w: Static<Num>,
    /// 1 — butt, 2 — round, 3 — square.
    pub lc: u8,
    /// 1 — miter, 2 — round, 3 — bevel.
    pub lj: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ml: Option<Num>,
}

/// Тождественное преобразование группы — координаты уже «запечены».
#[derive(Debug, Serialize)]
pub struct GroupTransform {
    p: Static<[Num; 2]>,
    a: Static<[Num; 2]>,
    s: Static<[Num; 2]>,
    r: Static<Num>,
    o: Static<Num>,
}

impl Default for GroupTransform {
    fn default() -> Self {
        Self {
            p: Static::new([Num(0.0); 2]),
            a: Static::new([Num(0.0); 2]),
            s: Static::new([Num(100.0); 2]),
            r: Static::new(Num(0.0)),
            o: Static::new(Num(100.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_written_without_fraction() {
        assert_eq!(serde_json::to_string(&Num(100.0)).unwrap(), "100");
        assert_eq!(serde_json::to_string(&Num(42.67)).unwrap(), "42.67");
    }

    #[test]
    fn rounder_rounds_and_removes_negative_zero() {
        let r = Rounder::new(2);
        assert_eq!(r.num(1.23456).0, 1.23);
        assert_eq!(r.num(-0.001).0, 0.0);
        assert!(r.num(-0.001).0.is_sign_positive());
    }

    #[test]
    fn shapes_are_tagged_with_ty() {
        let fill = Shape::Fill {
            c: Static::new([Num(1.0), Num(0.0), Num(0.0), Num(1.0)]),
            o: Static::new(Num(100.0)),
            r: FILL_RULE_EVENODD,
        };
        let v = serde_json::to_value(&fill).unwrap();
        assert_eq!(v["ty"], "fl");
        assert_eq!(v["r"], 2);
        assert_eq!(v["c"]["a"], 0);
    }
}

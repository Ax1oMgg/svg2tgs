//! Настройки трассировки растра (PNG, JPEG, WebP → контуры) и пресеты.
//!
//! Сама трассировка — в крейте `svg2tgs-trace`; здесь только данные:
//! что видит и меняет пользователь ([`Trace`], часть [`crate::Options`])
//! и замороженные значения пресетов с допустимыми диапазонами
//! ([`TracePreset::params`]). Значения — «лучшее на сейчас»: их подбирает
//! прогон наборов (`render/examples/trace_corpus.rs`), итог — в `SPEC.md`.

use serde::{Deserialize, Serialize};

/// Способ трассировки. `Auto` выбирает один из остальных у каждого файла.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TracePreset {
    #[default]
    Auto,
    /// Рисунок, логотип, иконка, простая иллюстрация.
    Drawing,
    /// Пиксель-арт: квадратные пиксели, без сглаживания контуров.
    Pixel,
    /// Фото → постер из пятен.
    Photo,
}

/// Настройки трассировки, которые видит пользователь. `None` — умолчание
/// пресета; смена пресета сбрасывает их в `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Trace {
    pub preset: TracePreset,
    /// Число цветов (ползунок «Цвета»), в пределах [`TraceParams::colors`].
    pub colors: Option<u32>,
    /// Мелкие детали (ползунок «Детали»), 0 — крупнее, 100 — мельче.
    pub details: Option<u32>,
    /// Сплошной фон по углам картинки становится прозрачным.
    pub remove_bg: bool,
    /// Плавные переходы — градиентом, а не полосами.
    pub gradients: Option<bool>,
    /// Отладка (`--trace-scale`): пикселей трассировки на пиксель канваса
    /// вместо значения пресета. В интерфейсе не показывается.
    pub scale: Option<f32>,
}

/// Наибольшее значение «Деталей».
pub const MAX_DETAILS: u32 = 100;

/// Как упрощать контуры.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    /// Кривые (сплайны) — рисунки и фото.
    Spline,
    /// Ступеньки по пикселям без упрощения — пиксель-арт.
    Pixel,
}

/// Значения пресета: замороженные параметры трассировки и диапазоны
/// с умолчаниями того, что меняет пользователь.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceParams {
    /// «Цвета»: наименьшее, наибольшее, по умолчанию.
    pub colors: (u32, u32, u32),
    /// «Детали» по умолчанию (0–100).
    pub details: u32,
    /// «Градиенты» по умолчанию.
    pub gradients: bool,
    /// Пикселей трассировки на пиксель канваса 512 (k) для стикера.
    pub scale: f32,
    /// Множитель k для клетки нарезки: эмодзи показываются мелко, а лимит
    /// 64 КБ — у каждой клетки.
    pub grid_scale: f32,
    /// Сторона наименьшего пятна, пиксели канваса: при «Деталях» 0 и 100.
    /// У пиксель-арта — в пикселях картинки (блоках).
    pub speckle: (f32, f32),
    /// Сглаживание перед разбиением на цвета (радиус, пиксели трассировки):
    /// 0 — нет.
    pub blur: u32,
    pub mode: PathMode,
    /// Угол, острее которого — угол, а не изгиб (градусы).
    pub corner_threshold: f32,
    /// Наименьшая длина отрезка при подгонке кривых, пиксели канваса.
    pub length_threshold: f32,
    /// Угол склейки сплайнов (градусы).
    pub splice_threshold: f32,
}

impl TracePreset {
    /// Значения пресета; для `Auto` — как у «Рисунка» (до выбора).
    pub fn params(self) -> TraceParams {
        match self {
            TracePreset::Auto | TracePreset::Drawing => TraceParams {
                colors: (2, 64, 16),
                details: 50,
                gradients: true,
                scale: 1.0,
                grid_scale: 0.5,
                speckle: (12.0, 1.0),
                blur: 0,
                mode: PathMode::Spline,
                corner_threshold: 60.0,
                length_threshold: 4.0,
                splice_threshold: 45.0,
            },
            TracePreset::Pixel => TraceParams {
                colors: (2, 64, 64),
                details: 100,
                gradients: false,
                scale: 1.0,
                grid_scale: 1.0,
                speckle: (4.0, 1.0),
                blur: 0,
                mode: PathMode::Pixel,
                corner_threshold: 0.0,
                length_threshold: 0.0,
                splice_threshold: 0.0,
            },
            TracePreset::Photo => TraceParams {
                colors: (4, 64, 24),
                details: 35,
                gradients: true,
                scale: 0.5,
                grid_scale: 0.5,
                speckle: (16.0, 2.0),
                blur: 1,
                mode: PathMode::Spline,
                corner_threshold: 180.0,
                length_threshold: 4.0,
                splice_threshold: 45.0,
            },
        }
    }
}

impl Trace {
    /// Число цветов в пределах пресета `preset` (уже выбранного).
    pub fn colors_for(&self, preset: TracePreset) -> u32 {
        let (lo, hi, def) = preset.params().colors;
        self.colors.unwrap_or(def).clamp(lo, hi)
    }

    /// «Детали» с умолчанием пресета.
    pub fn details_for(&self, preset: TracePreset) -> u32 {
        self.details
            .unwrap_or(preset.params().details)
            .min(MAX_DETAILS)
    }

    /// «Градиенты» с умолчанием пресета; у пиксель-арта их нет.
    pub fn gradients_for(&self, preset: TracePreset) -> bool {
        preset != TracePreset::Pixel && self.gradients.unwrap_or(preset.params().gradients)
    }
}

/// Что сделала трассировка — для интерфейса и CLI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceInfo {
    /// Каким пресетом трассировано (у «Авто» — выбранный).
    pub preset: TracePreset,
    /// Пресет выбран сам («Авто»).
    pub auto: bool,
    /// Ширина и высота картинки, пиксели.
    pub width: u32,
    pub height: u32,
    /// Размер трассировки, пиксели.
    pub traced_width: u32,
    pub traced_height: u32,
    /// Цветов и «Деталей» в итоге (после автоподгонки).
    pub colors: u32,
    pub details: u32,
    /// Диапазон «Цветов» пресета.
    pub colors_min: u32,
    pub colors_max: u32,
    /// Включены ли градиенты и сколько переходов ими заменено.
    pub gradients: bool,
    pub gradient_count: u32,
    /// Шаг блоков пиксель-арта (1 — пиксель картинки).
    pub block: u32,
    /// Фон убран.
    pub background_removed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_inside_ranges() {
        for p in [TracePreset::Drawing, TracePreset::Pixel, TracePreset::Photo] {
            let v = p.params();
            assert!(v.colors.0 <= v.colors.2 && v.colors.2 <= v.colors.1);
            assert!(v.details <= MAX_DETAILS);
            assert!(v.scale > 0.0 && v.grid_scale > 0.0);
        }
        let t = Trace {
            colors: Some(1000),
            ..Trace::default()
        };
        assert_eq!(t.colors_for(TracePreset::Drawing), 64);
        assert!(!Trace {
            gradients: Some(true),
            ..Trace::default()
        }
        .gradients_for(TracePreset::Pixel));
    }

    /// Интерфейс показывает диапазоны и умолчания пресетов из своей копии
    /// (`TRACE_PARAMS` в `app/src/types.ts`) — она должна совпадать с ядром.
    #[test]
    fn ts_mirror_matches() {
        let ts =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../app/src/types.ts"))
                .unwrap();
        for (name, p) in [
            ("drawing", TracePreset::Drawing),
            ("pixel", TracePreset::Pixel),
            ("photo", TracePreset::Photo),
        ] {
            let v = p.params();
            let line = format!(
                "  {name}: {{ colors: [{}, {}, {}], details: {}, gradients: {} }},",
                v.colors.0, v.colors.1, v.colors.2, v.details, v.gradients
            );
            assert!(ts.contains(&line), "в types.ts нет строки:\n{line}");
        }
    }

    #[test]
    fn old_options_without_trace_still_read() {
        let o: crate::Options = serde_json::from_str(r#"{"rotation": 10}"#).unwrap();
        assert_eq!(o.trace, Trace::default());
        let o: crate::Options =
            serde_json::from_str(r#"{"trace": {"preset": "pixel", "colors": 8}}"#).unwrap();
        assert_eq!(o.trace.preset, TracePreset::Pixel);
        assert_eq!(o.trace.colors, Some(8));
    }
}

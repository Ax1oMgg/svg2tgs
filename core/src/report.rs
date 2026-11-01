//! Результат конвертации и замечания к нему.
//!
//! Замечания передаются как машинные коды с параметрами, а не как готовый
//! текст: интерфейс и CLI сами переводят их на нужный язык.

use serde::{Deserialize, Serialize};

/// Насколько замечание важно для пользователя.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Справка: что-то преобразовано, но выглядит так же.
    Info,
    /// Результат может отличаться от исходника.
    Warning,
}

/// Код замечания. Новые коды добавляются в конец, старые не меняют смысл.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum WarningCode {
    /// Текст переведён в кривые.
    TextConvertedToPaths,
    /// Эффект фильтра (размытие, тень и т. п.) не поддерживается и убран.
    FilterRemoved { filter: String },
    /// Маска не поддерживается и убрана.
    MaskRemoved,
    /// Растровое изображение внутри SVG не поддерживается и убрано.
    ImageRemoved,
    /// Паттерн заменён сплошным цветом.
    PatternReplaced,
    /// Режим наложения не поддерживается, использован обычный.
    BlendModeIgnored { mode: String },
    /// Файл превышает лимит пресета.
    SizeOverLimit { bytes: usize, limit: usize },
    /// Текст не удалось перевести в кривые (нет подходящего шрифта) — он пропущен.
    TextDropped,
    /// Шрифта `family` нет: текст нарисован шрифтом `used`. В веб-версии
    /// (один встроенный шрифт) — для любого текста, кроме самого `used`.
    FontReplaced { family: String, used: String },
    /// Этих знаков нет ни в одном шрифте — они не нарисованы. Перечислены
    /// первые недостающие знаки строк текста (до 8, без повторов).
    GlyphsMissing { chars: String },
    /// Вложенная обрезка сложной формы передана приближённо.
    ClipApproximated,
    /// Градиент передан приближённо: перекос или растяжение формы,
    /// повторение (`spreadMethod`) или фокусный радиус.
    GradientApproximated,
    /// Канвас не того размера, который требует Telegram.
    CanvasNotAllowed { size: u32, required: u32 },
    /// Частота кадров не та, которую требует Telegram.
    FpsNotAllowed { fps: u32, required: u32 },
    /// Анимация длиннее, чем разрешает Telegram.
    DurationTooLong { secs: f32, max: f32 },
    /// Чтобы уложиться в лимит размера, контуры упрощены: отклонение
    /// не больше `tolerance` пикселей канваса 512.
    Simplified { tolerance: f32 },
    /// Фигура слишком сложна для движка Telegram даже после упрощения:
    /// `points` точек при пределе `limit`. В Telegram она не нарисуется.
    TooComplex { points: usize, limit: usize },
    /// Для эмодзи: линии такой толщины (пиксели канваса 512) почти
    /// исчезнут при показе в строке текста.
    ThinLinesForEmoji { width: f32, recommended: f32 },
}

impl WarningCode {
    pub fn severity(&self) -> Severity {
        match self {
            WarningCode::TextConvertedToPaths => Severity::Info,
            // Меньше полупикселя на канвасе 512 на глаз не видно.
            WarningCode::Simplified { tolerance } if *tolerance < 0.5 => Severity::Info,
            _ => Severity::Warning,
        }
    }
}

/// Одно замечание, опционально привязанное к элементу SVG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    #[serde(flatten)]
    pub code: WarningCode,
    /// `id` элемента исходного SVG, если он есть.
    pub element_id: Option<String>,
}

/// Успешный результат конвертации.
#[derive(Debug, Clone)]
pub struct Conversion {
    /// Готовый .tgs (gzip-сжатый Lottie JSON).
    pub tgs: Vec<u8>,
    /// Размер Lottie JSON до сжатия, байт.
    pub json_bytes: usize,
    pub warnings: Vec<Warning>,
}

impl Conversion {
    pub fn tgs_bytes(&self) -> usize {
        self.tgs.len()
    }
}

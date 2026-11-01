//! Параметры конвертации.
//!
//! Значения пресетов берутся из требований Telegram (модуль `telegram`).

use serde::{Deserialize, Serialize};

use crate::telegram;
use crate::trace::Trace;

/// Наибольшая толщина обводки, пиксели канваса.
pub const MAX_OUTLINE: f32 = 64.0;

/// Для чего готовим файл. Пресет задаёт канвас, fps и длительность.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Анимированный стикер.
    #[default]
    Sticker,
    /// Кастомный эмодзи.
    Emoji,
}

impl Preset {
    /// Сторона квадратного канваса в пикселях.
    /// Для эмодзи Telegram отдельных цифр не даёт: правила те же.
    pub fn canvas_size(self) -> u32 {
        telegram::CANVAS
    }

    /// Кадров в секунду.
    pub fn fps(self) -> u32 {
        telegram::FPS
    }

    /// Длительность по умолчанию, секунды. Рисунок статичный, поэтому
    /// подойдёт любая в пределах лимита; одна секунда — 60 кадров.
    pub fn duration_secs(self) -> f32 {
        1.0
    }

    /// Лимит размера итогового .tgs в байтах.
    pub fn max_bytes(self) -> usize {
        telegram::MAX_BYTES
    }
}

/// Как вписать рисунок в квадратный канвас.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Целиком, с сохранением пропорций (как `xMidYMid meet`).
    #[default]
    Contain,
    /// Заполнить рабочую область, лишнее обрезается (как `xMidYMid slice`).
    Cover,
    /// Ширина рисунка равна ширине рабочей области.
    Width,
    /// Высота рисунка равна высоте рабочей области.
    Height,
    /// Масштаб вручную (`Options::scale`, 1 — как `Contain`).
    Free,
}

/// Что считать рамкой рисунка.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Bounds {
    /// Вся область документа SVG (viewBox), с пустыми полями.
    #[default]
    Container,
    /// Только видимый рисунок с толщиной обводок.
    Content,
}

/// Рамка обрезки. Края — отступы внутрь от рамки рисунка **без поворота**
/// (после отражения и пропорций), доли её ширины (`left`, `right`) и высоты
/// (`top`, `bottom`). Рамка стоит относительно центра рисунка и при повороте
/// не двигается: поворачивается рисунок под ней. Отступ может быть
/// отрицательным — рамка шире рисунка без поворота (у повёрнутого рисунка
/// углы выступают). Всё, что вне рамки, отрезается.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Crop {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Нарезка на эмодзи: рисунок раскладывается на сетку `cols × rows`,
/// каждая клетка — отдельный файл с канвасом пресета.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
}

impl Grid {
    /// Наибольшее число столбцов и строк.
    pub const MAX: u32 = 20;

    /// Столбцов для автоматической сетки: столько эмодзи клиенты Telegram
    /// показывают в ряд (панель эмодзи), и картинка собирается без переносов.
    /// Это практика клиентов, в документации Telegram числа нет.
    pub const AUTO_COLS: u32 = 8;

    /// Столбцы и строки в пределах 1…[`Grid::MAX`].
    pub fn clamped(self) -> Self {
        Self {
            cols: self.cols.clamp(1, Self::MAX),
            rows: self.rows.clamp(1, Self::MAX),
        }
    }

    /// Сетка по пропорциям рисунка `aspect` (ширина / высота), не больше
    /// `max_cols` столбцов: из вариантов с самым близким соотношением сторон
    /// берётся самый крупный.
    pub fn auto(aspect: f32, max_cols: u32) -> Self {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect as f64
        } else {
            1.0
        };
        let mut best = (1, 1, f64::MAX);
        for c in 1..=max_cols.clamp(1, Self::MAX) {
            let r = ((c as f64 / aspect).round() as u32).clamp(1, Self::MAX);
            let err = ((c as f64 / r as f64) / aspect).ln().abs();
            if err < best.2 - 1e-9 || (err - best.2).abs() <= 1e-9 && c > best.0 {
                best = (c, r, err);
            }
        }
        Self {
            cols: best.0,
            rows: best.1,
        }
    }
}

/// Фон под рисунком.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind", content = "color")]
pub enum Background {
    #[default]
    Transparent,
    /// Цвет в виде `#rrggbb`.
    Solid(String),
}

/// Насколько агрессивно уменьшать размер файла.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "mode", content = "precision")]
pub enum Optimization {
    /// Ужать под лимит пресета с минимальной видимой потерей.
    #[default]
    Auto,
    /// Только преобразования без потерь.
    Lossless,
    /// Фиксированная точность координат: знаков после запятой.
    Manual(u8),
}

/// Обводка вокруг всего рисунка (как у вырезанных стикеров) или рамка фона.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    /// Толщина в пикселях канваса.
    pub width: f32,
    /// Цвет в виде `#rrggbb`.
    pub color: String,
}

impl Default for Outline {
    fn default() -> Self {
        Self {
            width: 12.0,
            color: "#ffffff".into(),
        }
    }
}

/// Полный набор параметров одной конвертации.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub preset: Preset,
    /// Отступ от края канваса, доля стороны: 0.0–0.2.
    pub padding: f32,
    /// Рамка рисунка: по контейнеру (документу) или по краю рисунка.
    pub bounds: Bounds,
    pub fit: Fit,
    /// Масштаб для `Fit::Free`: 1 — рисунок целиком.
    pub scale: f32,
    /// Положение в рабочей области по X и Y, 0–1 (0.5 — по центру). Сдвигает
    /// рисунок по свободному месту или выбирает, какая часть видна, если он больше.
    pub position_x: f32,
    pub position_y: f32,
    /// Отразить по горизонтали / вертикали.
    pub flip_h: bool,
    pub flip_v: bool,
    /// Поворот по часовой стрелке, градусы.
    pub rotation: f32,
    /// Пропорции: растяжение по ширине относительно высоты (1 — без искажения).
    pub aspect: f32,
    /// Рамка обрезки; `None` — рисунок целиком.
    pub crop: Option<Crop>,
    pub background: Background,
    /// Скругление углов фона, пиксели канваса (0 — прямые углы).
    pub background_radius: f32,
    /// Рамка фона внутри канваса; `None` — без рамки.
    pub background_border: Option<Outline>,
    /// Обрезать рисунок и его обводку по скруглённой рамке фона (по внутреннему
    /// краю рамки фона, если она есть). Действует, только когда фон виден.
    pub background_clip: bool,
    /// Нарезка на эмодзи; `None` — один файл.
    pub grid: Option<Grid>,
    /// Сетка нарезки подбирается по пропорциям рисунка (см. [`Grid::auto`]),
    /// `grid` тогда задаёт только, что нарезка включена.
    pub grid_auto: bool,
    /// Обводка вокруг рисунка; `None` — без обводки.
    pub outline: Option<Outline>,
    pub optimization: Optimization,
    /// Трассировка растровой картинки (PNG, JPEG, WebP); для SVG не действует.
    pub trace: Trace,
    /// Переопределения пресета (расширенные настройки).
    pub canvas_size: Option<u32>,
    pub fps: Option<u32>,
    pub duration_secs: Option<f32>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            preset: Preset::default(),
            padding: 0.0,
            bounds: Bounds::default(),
            fit: Fit::default(),
            scale: 1.0,
            position_x: 0.5,
            position_y: 0.5,
            flip_h: false,
            flip_v: false,
            rotation: 0.0,
            aspect: 1.0,
            crop: None,
            background: Background::default(),
            background_radius: 0.0,
            background_border: None,
            background_clip: true,
            grid: None,
            grid_auto: false,
            outline: None,
            optimization: Optimization::default(),
            trace: Trace::default(),
            canvas_size: None,
            fps: None,
            duration_secs: None,
        }
    }
}

impl Options {
    pub fn effective_canvas_size(&self) -> u32 {
        self.canvas_size
            .unwrap_or_else(|| self.preset.canvas_size())
    }

    pub fn effective_fps(&self) -> u32 {
        self.fps.unwrap_or_else(|| self.preset.fps())
    }

    pub fn effective_duration_secs(&self) -> f32 {
        self.duration_secs
            .unwrap_or_else(|| self.preset.duration_secs())
    }

    /// Сетка нарезки в допустимых пределах; 1 × 1 — то же, что без нарезки.
    pub fn grid(&self) -> Grid {
        self.grid.map_or(Grid { cols: 1, rows: 1 }, Grid::clamped)
    }

    /// Толщина обводки, если она включена и видна.
    pub fn outline_width(&self) -> f32 {
        self.outline
            .as_ref()
            .map_or(0.0, |o| o.width.clamp(0.0, MAX_OUTLINE))
    }

    /// Число кадров анимации (не меньше 1).
    pub fn frame_count(&self) -> u32 {
        let frames = (self.effective_fps() as f32 * self.effective_duration_secs()).round();
        (frames as u32).max(1)
    }
}

/// Метка формата файла настроек (`Options::to_json`). Номер меняется, только
/// если старые файлы перестанут читаться как есть.
pub const OPTIONS_FORMAT: &str = "svg2tgs-options/1";

/// Почему не удалось прочитать настройки из текста. Текст для людей
/// делают интерфейс и CLI по коду.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OptionsError {
    /// `syntax` — не JSON; `value` — у поля значение не того вида;
    /// `not_object` — не объект `{…}`; `newer_format` — файл от более новой версии.
    pub code: &'static str,
    /// Строка и столбец (с 1), если известны.
    pub line: usize,
    pub column: usize,
}

/// Прочитанные настройки и сколько полей в тексте узнано.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedOptions {
    pub options: Options,
    pub known: usize,
}

impl Options {
    /// Настройки текстом (JSON с меткой формата) — для копирования и файла.
    pub fn to_json(&self) -> String {
        let mut v = serde_json::to_value(self).expect("настройки всегда сериализуются");
        if let serde_json::Value::Object(map) = &mut v {
            let mut out = serde_json::Map::new();
            out.insert("format".into(), OPTIONS_FORMAT.into());
            out.extend(std::mem::take(map));
            v = serde_json::Value::Object(out);
        }
        serde_json::to_string_pretty(&v).expect("JSON в память")
    }

    /// Настройки из текста. Неизвестные поля пропускаются, недостающие —
    /// по умолчанию; метка формата необязательна.
    pub fn from_json(text: &str) -> Result<ParsedOptions, OptionsError> {
        let err = |code, e: &serde_json::Error| OptionsError {
            code,
            line: e.line(),
            column: e.column(),
        };
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| err("syntax", &e))?;
        let Some(map) = value.as_object() else {
            return Err(OptionsError {
                code: "not_object",
                line: 1,
                column: 1,
            });
        };
        if let Some(f) = map.get("format").and_then(|f| f.as_str()) {
            let newer = f
                .strip_prefix("svg2tgs-options/")
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| n > 1);
            if newer {
                return Err(OptionsError {
                    code: "newer_format",
                    line: 1,
                    column: 1,
                });
            }
        }
        // Значения не того вида — с местом ошибки в исходном тексте.
        let options: Options = serde_json::from_str(text).map_err(|e| err("value", &e))?;
        let fields = serde_json::to_value(Options::default()).expect("сериализация");
        let known = map
            .keys()
            .filter(|k| fields.get(k.as_str()).is_some())
            .count();
        Ok(ParsedOptions { options, known })
    }
}

#[cfg(test)]
mod json_tests {
    use super::*;

    #[test]
    fn options_roundtrip_through_text() {
        let o = Options {
            rotation: 45.0,
            grid: Some(Grid { cols: 8, rows: 5 }),
            outline: Some(Outline::default()),
            ..Options::default()
        };
        let text = o.to_json();
        assert!(text.contains(OPTIONS_FORMAT));
        let back = Options::from_json(&text).unwrap();
        assert_eq!(back.options, o);
    }

    #[test]
    fn partial_and_unknown_fields() {
        let p = Options::from_json(r#"{"rotation": 30, "something": 1}"#).unwrap();
        assert_eq!(p.options.rotation, 30.0);
        assert_eq!(p.known, 1);
        assert_eq!(p.options.fit, Fit::Contain);
    }

    #[test]
    fn errors_have_codes_and_places() {
        let e = Options::from_json("{\n  \"rotation\": \"сорок\"\n}").unwrap_err();
        assert_eq!((e.code, e.line), ("value", 2));
        assert_eq!(Options::from_json("{oops").unwrap_err().code, "syntax");
        assert_eq!(Options::from_json("[1]").unwrap_err().code, "not_object");
        assert_eq!(
            Options::from_json(r#"{"format": "svg2tgs-options/2"}"#)
                .unwrap_err()
                .code,
            "newer_format"
        );
    }
}

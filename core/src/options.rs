//! Параметры конвертации.
//!
//! Значения пресетов берутся из требований Telegram (модуль `telegram`).

use serde::{Deserialize, Serialize};

use crate::telegram;

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

/// Обрезка краёв рисунка, доли стороны (после отражения, поворота и пропорций).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Crop {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
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
    pub crop: Crop,
    pub background: Background,
    /// Скругление углов фона, пиксели канваса (0 — прямые углы).
    pub background_radius: f32,
    /// Рамка фона внутри канваса; `None` — без рамки.
    pub background_border: Option<Outline>,
    /// Обводка вокруг рисунка; `None` — без обводки.
    pub outline: Option<Outline>,
    pub optimization: Optimization,
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
            crop: Crop::default(),
            background: Background::default(),
            background_radius: 0.0,
            background_border: None,
            outline: None,
            optimization: Optimization::default(),
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

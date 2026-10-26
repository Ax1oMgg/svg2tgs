/** Типы, общие для интерфейса и Rust-части (совпадают с serde в core). */

export type Preset = "sticker" | "emoji";
export type Fit = "contain" | "cover";

export type Background = { kind: "transparent" } | { kind: "solid"; color: string };

export type Optimization =
  | { mode: "auto" }
  | { mode: "lossless" }
  | { mode: "manual"; precision: number };

/** Обводка вокруг рисунка — `svg2tgs_core::Outline`. */
export interface Outline {
  /** Толщина, пиксели канваса 512. */
  width: number;
  color: string;
}

/** Параметры конвертации — `svg2tgs_core::Options`. */
export interface Options {
  preset: Preset;
  /** Доля стороны, 0–0.2. */
  padding: number;
  fit: Fit;
  background: Background;
  /** Скругление углов фона, px канваса 512. */
  background_radius?: number;
  /** Рамка фона внутри канваса; `null` — без рамки. */
  background_border?: Outline | null;
  /** `null` — без обводки. */
  outline?: Outline | null;
  optimization: Optimization;
  fps?: number | null;
  duration_secs?: number | null;
}

/** Замечание ядра: код плюс параметры (`svg2tgs_core::Warning`). */
export interface Warning {
  code: string;
  element_id?: string | null;
  [param: string]: unknown;
}

/** Итог конвертации одного файла. */
export interface Outcome {
  id: string;
  tgsBytes: number | null;
  warnings: Warning[];
  error: string | null;
}

export type Source = { kind: "path"; path: string } | { kind: "text"; text: string };

export type Status = "pending" | "ok" | "warn" | "error";

/** Файл в списке. */
export interface FileItem {
  id: string;
  /** Имя для показа, с расширением. */
  name: string;
  source: Source;
  status: Status;
  outcome?: Outcome;
  /** Миниатюра результата (data-URL). */
  thumb?: string;
  /** Свои настройки файла; без них действуют общие. */
  options?: Options;
}

/** Что сохранить (`files::SaveRequest`): папка — `dir`, иначе папка `source`. */
export interface SaveRequest {
  id: string;
  /** Имя без расширения. */
  name: string;
  dir?: string | null;
  source?: string | null;
  subfolder?: string | null;
}

export type Conflict = "overwrite" | "skip" | "number";

export interface Saved {
  id: string;
  status: "saved" | "skipped" | "failed";
  path: string | null;
}

export const DEFAULT_OPTIONS: Options = {
  preset: "sticker",
  padding: 0,
  fit: "contain",
  background: { kind: "transparent" },
  background_radius: 0,
  background_border: null,
  outline: null,
  optimization: { mode: "auto" },
};

export const DEFAULT_OUTLINE: Outline = { width: 12, color: "#ffffff" };
export const MAX_OUTLINE = 64;
export const MAX_RADIUS = 256;
export const DEFAULT_BORDER: Outline = { width: 12, color: "#3157d5" };

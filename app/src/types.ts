/** Типы, общие для интерфейса и Rust-части (совпадают с serde в core). */

export type Preset = "sticker" | "emoji";
export type Bounds = "container" | "content";
export type Fit = "contain" | "cover" | "width" | "height" | "free";

/** Обрезка краёв, доли стороны — `svg2tgs_core::Crop`. */
export interface Crop {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

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
  /** Рамка рисунка: вся область SVG или по краю рисунка. */
  bounds?: Bounds;
  fit: Fit;
  /** Масштаб для `free`, 1 — целиком. */
  scale?: number;
  /** Положение в рабочей области, 0–1 (0.5 — по центру). */
  position_x?: number;
  position_y?: number;
  flip_h?: boolean;
  flip_v?: boolean;
  /** Поворот по часовой, градусы. */
  rotation?: number;
  /** Пропорции: ширина относительно высоты, 1 — без искажения. */
  aspect?: number;
  crop?: Crop;
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
  /** Заменить существующий файл. */
  overwrite?: boolean;
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
  bounds: "container",
  fit: "contain",
  scale: 1,
  position_x: 0.5,
  position_y: 0.5,
  flip_h: false,
  flip_v: false,
  rotation: 0,
  aspect: 1,
  crop: { top: 0, right: 0, bottom: 0, left: 0 },
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

/** Ключи настроек по разделам панели — для сброса раздела и для
 *  автоматического перехода на «этого файла» при правке трансформации. */
export const SECTION_KEYS = {
  position: ["bounds", "fit", "scale", "position_x", "position_y", "padding"],
  transform: ["flip_h", "flip_v", "rotation", "aspect", "crop"],
  style: ["outline"],
  background: ["background", "background_radius", "background_border"],
  extra: ["optimization", "fps", "duration_secs"],
} as const satisfies Record<string, readonly (keyof Options)[]>;

export type SectionId = keyof typeof SECTION_KEYS;

/** Совпадают ли настройки `a` и `b` по ключам `keys`. */
export function sameKeys(a: Options, b: Options, keys: readonly (keyof Options)[]): boolean {
  return keys.every((k) => JSON.stringify(a[k] ?? DEFAULT_OPTIONS[k] ?? null) === JSON.stringify(b[k] ?? DEFAULT_OPTIONS[k] ?? null));
}

/** Сброс ключей `keys` к значениям по умолчанию. */
export function resetKeys(o: Options, keys: readonly (keyof Options)[]): Options {
  const out: Options = { ...o };
  for (const k of keys) (out as unknown as Record<string, unknown>)[k] = DEFAULT_OPTIONS[k] ?? null;
  return out;
}

/** Файл, который уже есть в папке экспорта (`files::Existing`). */
export interface Existing {
  id: string;
  fileName: string;
  path: string;
  /** Предлагаемое новое имя без расширения. */
  suggested: string;
}

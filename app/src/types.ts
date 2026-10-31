/** Типы, общие для интерфейса и Rust-части (совпадают с serde в core). */

export type Preset = "sticker" | "emoji";
export type Bounds = "container" | "content";
export type Fit = "contain" | "cover" | "width" | "height" | "free";

/**
 * Рамка обрезки — `svg2tgs_core::Crop`: отступы внутрь от рамки рисунка
 * без поворота, доли её ширины и высоты (могут быть отрицательными).
 * Рамка стоит относительно центра и при повороте не двигается.
 */
export interface Crop {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export type Background = { kind: "transparent" } | { kind: "solid"; color: string };

/** Сетка нарезки на эмодзи — `svg2tgs_core::Grid`. */
export interface Grid {
  cols: number;
  rows: number;
}

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
  /** `null` — без обрезки. */
  crop?: Crop | null;
  background: Background;
  /** Скругление углов фона, px канваса 512. */
  background_radius?: number;
  /** Рамка фона внутри канваса; `null` — без рамки. */
  background_border?: Outline | null;
  /** Обрезать рисунок по скруглённой рамке фона. */
  background_clip?: boolean;
  /** Нарезка на эмодзи; `null` — один файл. */
  grid?: Grid | null;
  /** Сетка подбирается по пропорциям рисунка. */
  grid_auto?: boolean;
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
  /** Размер файла; при нарезке — самой большой клетки. */
  tgsBytes: number | null;
  /** Сетка нарезки `[столбцы, строки]`, `[1, 1]` — один файл. */
  grid: [number, number];
  /** Соотношение сторон рисунка после трансформаций (ширина / высота). */
  aspect: number;
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
  /** Сколько файлов записано (клеток нарезки). */
  count: number;
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
  crop: null,
  background: { kind: "transparent" },
  background_radius: 0,
  background_border: null,
  background_clip: true,
  grid: null,
  grid_auto: true,
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
  transform: ["flip_h", "flip_v", "aspect", "rotation", "crop"],
  style: ["outline"],
  background: ["background", "background_radius", "background_border", "background_clip"],
  extra: ["optimization", "fps", "duration_secs"],
} as const satisfies Record<string, readonly (keyof Options)[]>;

/** Настройки нарезки (блок «Результат»). */
export const GRID_KEYS = ["grid", "grid_auto"] as const satisfies readonly (keyof Options)[];

/** Правка этих ключей при нескольких файлах уходит только в выбранные
 *  файлы: трансформация и положение почти всегда свои у каждого рисунка. */
export const OWN_KEYS = [...SECTION_KEYS.transform, "position_x", "position_y"] as const;

/** Столбцов в автоматической сетке: столько эмодзи клиенты Telegram
 *  показывают в ряд. Практика клиентов, не требование документации. */
export const AUTO_GRID_COLS = 8;
export const MAX_GRID = 20;

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

/** Ошибка чтения настроек из текста (`svg2tgs_core::OptionsError`). */
export interface OptionsError {
  code: "syntax" | "value" | "not_object" | "newer_format" | string;
  line: number;
  column: number;
}

/** Прочитанные из текста настройки: сами настройки и сколько полей узнано. */
export interface ParsedOptions {
  options: Options;
  known: number;
}

/** Файл, который уже есть в папке экспорта (`files::Existing`). */
export interface Existing {
  id: string;
  fileName: string;
  path: string;
  /** Предлагаемое новое имя без расширения. */
  suggested: string;
}

/* ----------------------------- Выгрузка в Telegram ---------------------------- */

/** Состояние выгрузки (`telegram::Status`). */
export interface TgStatus {
  /** Переключатель в «Настройках»; выключен — программа в сеть не ходит. */
  enabled: boolean;
  /** Имя бота без @, если токен проверен. */
  bot: string | null;
  /** Владелец наборов: `@ник` или имя; `null` — боту ещё не написали /start. */
  owner: string | null;
}

/** Ошибка выгрузки: код и параметры (`TgError`, `PublishError`). */
export interface TgError {
  code: string;
  [key: string]: unknown;
}

export type SetKind = "stickers" | "emoji";

/** Один стикер задания: файл списка и, при нарезке, номер клетки. */
export interface PublishItem {
  id: string;
  tile: number | null;
  /** Имя файла без расширения. */
  name: string;
  emoji: string[];
}

export interface PublishRequest {
  kind: SetKind;
  title: string;
  /** Начало адреса, без `_by_<бот>`. */
  stem: string;
  resume: boolean;
  items: PublishItem[];
}

export interface PublishProgress {
  done: number;
  total: number;
}

export interface PublishDone {
  name: string;
  link: string;
  count: number;
  resumed_from: number;
}

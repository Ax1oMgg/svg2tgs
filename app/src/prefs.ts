/** Настройки, которые программа помнит между запусками (localStorage окна). */

import { hasLang, systemLang, type Lang } from "./i18n";
import { DEFAULT_OPTIONS, stripExt, type Options } from "./types";

export type Theme = "system" | "light" | "dark";
export type PreviewMode = "chat" | "closeup";
export type PreviewBg = "light" | "dark" | "checker";
/** Как показать результат: стикером или эмодзи. На файл не влияет. */
export type PreviewKind = "sticker" | "emoji";

/** Поле с флажком: выключенное не применяется, но значение помнится. */
export interface Toggle {
  on: boolean;
  value: string;
}

/** Куда и под каким именем экспортировать. */
export interface ExportPrefs {
  /** Папка экспорта; `null` — рядом с исходным SVG. */
  dir: string | null;
  suffix: Toggle;
  prefix: Toggle;
  subfolder: Toggle;
}

export interface Prefs {
  options: Options;
  lang: Lang;
  theme: Theme;
  recursive: boolean;
  export: ExportPrefs;
  previewMode: PreviewMode;
  previewBg: PreviewBg;
  previewKind: PreviewKind;
}

const KEY = "svg2tgs.prefs.v1";

export function defaultPrefs(): Prefs {
  // Язык системы, если для него есть перевод, иначе английский.
  const lang: Lang = systemLang();
  return {
    options: DEFAULT_OPTIONS,
    lang,
    theme: "system",
    recursive: true,
    export: {
      dir: null,
      suffix: { on: false, value: "" },
      prefix: { on: false, value: "" },
      subfolder: { on: false, value: "" },
    },
    previewMode: "chat",
    previewBg: "light",
    previewKind: "sticker",
  };
}

export function loadPrefs(): Prefs {
  const base = defaultPrefs();
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return base;
    const saved = JSON.parse(raw) as Partial<Prefs>;
    // До 0.1.5 «Стикер / Эмодзи» был настройкой экспорта.
    const kind = saved.previewKind ?? (saved.options?.preset === "emoji" ? "emoji" : base.previewKind);
    return {
      ...base,
      ...saved,
      previewKind: kind,
      // Перевод могли убрать — тогда язык системы.
      lang: saved.lang && hasLang(saved.lang) ? saved.lang : base.lang,
      options: migrateOptions({ ...base.options, ...saved.options }),
      export: offAtStart({ ...base.export, ...saved.export }),
    };
  } catch {
    return base;
  }
}

/** Настройки из прошлых версий: обрезка «без отступов» значила «без обрезки». */
function migrateOptions(o: Options): Options {
  const c = o.crop;
  if (c && c.top === 0 && c.right === 0 && c.bottom === 0 && c.left === 0) return { ...o, crop: null };
  return o;
}

/** При запуске суффикс, префикс и подпапка выключены; текст сохраняется. */
function offAtStart(e: ExportPrefs): ExportPrefs {
  return {
    ...e,
    suffix: { ...e.suffix, on: false },
    prefix: { ...e.prefix, on: false },
    subfolder: { ...e.subfolder, on: false },
  };
}

export function savePrefs(p: Prefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    // Хранилище недоступно — настройки просто не запомнятся.
  }
}

/** Имя результата без расширения: префикс + имя исходника + суффикс. */
export function exportName(fileName: string, e: ExportPrefs): string {
  const base = stripExt(fileName);
  const prefix = e.prefix.on ? e.prefix.value : "";
  const suffix = e.suffix.on ? e.suffix.value : "";
  return `${prefix}${base}${suffix}`;
}

/** Подпапка экспорта, если включена и не пуста. */
export function exportSubfolder(e: ExportPrefs): string | null {
  const v = e.subfolder.value.trim();
  return e.subfolder.on && v ? v : null;
}

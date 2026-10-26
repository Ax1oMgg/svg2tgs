/** Настройки, которые программа помнит между запусками (localStorage окна). */

import type { Lang } from "./i18n";
import { DEFAULT_OPTIONS, type Conflict, type Options } from "./types";

export type Theme = "system" | "light" | "dark";
export type PreviewMode = "chat" | "closeup";
export type PreviewBg = "light" | "dark" | "checker";

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
  conflict: Conflict;
  previewMode: PreviewMode;
  previewBg: PreviewBg;
}

const KEY = "svg2tgs.prefs.v1";

export function defaultPrefs(): Prefs {
  const lang: Lang = (navigator.language || "ru").toLowerCase().startsWith("ru") ? "ru" : "en";
  return {
    options: DEFAULT_OPTIONS,
    lang,
    theme: "system",
    recursive: true,
    export: {
      dir: null,
      suffix: { on: true, value: "_tg" },
      prefix: { on: false, value: "" },
      subfolder: { on: true, value: "tgs" },
    },
    conflict: "number",
    previewMode: "chat",
    previewBg: "light",
  };
}

export function loadPrefs(): Prefs {
  const base = defaultPrefs();
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return base;
    const saved = JSON.parse(raw) as Partial<Prefs>;
    return {
      ...base,
      ...saved,
      options: { ...base.options, ...saved.options },
      export: { ...base.export, ...saved.export },
    };
  } catch {
    return base;
  }
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
  const base = fileName.replace(/\.svg$/i, "");
  const prefix = e.prefix.on ? e.prefix.value : "";
  const suffix = e.suffix.on ? e.suffix.value : "";
  return `${prefix}${base}${suffix}`;
}

/** Подпапка экспорта, если включена и не пуста. */
export function exportSubfolder(e: ExportPrefs): string | null {
  const v = e.subfolder.value.trim();
  return e.subfolder.on && v ? v : null;
}

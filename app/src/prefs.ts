/** Настройки, которые программа помнит между запусками (localStorage окна). */

import type { Lang } from "./i18n";
import { DEFAULT_OPTIONS, type Conflict, type Options } from "./types";

export type Theme = "system" | "light" | "dark";
export type PreviewMode = "chat" | "closeup";
export type PreviewBg = "light" | "dark" | "checker";

export interface Prefs {
  options: Options;
  lang: Lang;
  theme: Theme;
  recursive: boolean;
  outputDir: string | null;
  nameTemplate: string;
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
    outputDir: null,
    nameTemplate: "{name}",
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
    return { ...base, ...saved, options: { ...base.options, ...saved.options } };
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

/** Имя файла по шаблону: `{name}` — имя исходника без расширения. */
export function applyTemplate(template: string, fileName: string): string {
  const base = fileName.replace(/\.svg$/i, "");
  const out = (template.trim() || "{name}").split("{name}").join(base);
  return out.replace(/\.tgs$/i, "");
}

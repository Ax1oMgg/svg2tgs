/** Тексты интерфейса на русском и английском. */

import type { Warning } from "./types";

export type Lang = "ru" | "en";

const ru = {
  add: "Добавить",
  addFiles: "Добавить файлы",
  addFolder: "Добавить папку",
  includeSubfolders: "Включая вложенные папки",
  dropTitle: "Перетащите SVG сюда",
  dropHint: "Файлы или папки. Можно вставить SVG-код из буфера — Ctrl+V.",
  dropRelease: "Отпустите, чтобы добавить",
  files: "Файлы",
  onlyProblems: "Только с замечаниями",
  clear: "Очистить",
  remove: "Убрать из списка",
  converting: "Конвертация…",
  conversion: "Конвертация",
  countOk: "Без замечаний",
  countWarn: "С замечаниями",
  countError: "С ошибкой",
  ownSettings: "свои настройки",
  clipboardName: (n: number) => `Из буфера ${n}`,
  summary: (ok: number, warn: number, err: number) =>
    [ok && `${ok} без замечаний`, warn && `${warn} с замечаниями`, err && `${err} с ошибкой`]
      .filter(Boolean)
      .join(", "),
  // Превью
  inChat: "Как в чате",
  closeUp: "Крупно",
  chatLight: "Светлый чат",
  chatDark: "Тёмный чат",
  checker: "Прозрачность",
  original: "Исходный SVG",
  result: "Так покажет Telegram",
  chatMessage: "Смотри, что получилось",
  emojiMessage: "Новый эмодзи",
  noPreview: "Превью появится после конвертации",
  allGood: "Замечаний нет — файл готов для Telegram.",
  fileFailed: "Файл не конвертирован",
  // Панель экспорта
  exportPanel: "Панель экспорта",
  export: "Экспортировать",
  toSource: "В папку источника",
  toSourceHint: "Каждый TGS сохраняется рядом со своим SVG",
  choose: "Выбрать",
  suffix: "Суффикс",
  prefix: "Префикс",
  subfolder: "Подпапка",
  exportSettings: "Настройки экспорта",
  scopeAll: "всех файлов",
  scopeOne: "этого файла",
  scopeOneHint: (name: string) => `Настройки ниже — только для ${name}. Остальные файлы — по общим.`,
  outline: "Обводка",
  outlineWidth: "Толщина обводки, px",
  outlineColor: "Цвет обводки",
  preset: "Стикер или эмодзи",
  sticker: "Стикер",
  emoji: "Эмодзи",
  object: "Объект",
  padding: "Отступ от края",
  fit: "Вписать",
  fitContain: "Целиком",
  fitContainHint: "Рисунок целиком, по краям — прозрачные поля",
  fitCover: "С обрезкой",
  fitCoverHint: "Заполнить квадрат, лишнее обрезается",
  background: "Фон",
  radius: "Скругление",
  fill: "Заливка",
  fillColor: "Цвет заливки фона",
  borderWidth: "Толщина обводки фона, px",
  borderColor: "Цвет обводки фона",
  optimization: "Размер файла",
  optAuto: "Авто",
  optAutoHint: "Ужать под лимит Telegram 64 КБ",
  optLossless: "Без потерь",
  optLosslessHint: "Без упрощения контуров",
  optManual: "Вручную",
  optManualHint: "Точность вручную",
  decimals: "Знаков после запятой",
  advanced: "Дополнительно",
  fps: "Кадров в секунду",
  duration: "Длительность, с",
  telegramDefault: "как требует Telegram",
  // Настройки программы
  settings: "Настройки",
  closeUnchanged: "Закрыть без изменений",
  done: "Готово",
  language: "Язык",
  theme: "Тема",
  themeSystem: "Как в системе",
  themeLight: "Светлая",
  themeDark: "Тёмная",
  // Экспорт
  saved: (n: number) => `Сохранено файлов: ${n}`,
  skipped: (n: number) => `пропущено: ${n}`,
  failed: (n: number) => `не удалось: ${n}`,
  openFolder: "Открыть папку",
  pastedNeedFolder: "SVG из буфера: выберите папку",
  browserMode: "Браузерный режим: тестовые данные",
  // О программе
  about: "О программе",
  version: (v: string) => `Версия ${v}`,
  aboutProject: "Проект «Чайная Аксиома»",
  aboutAuthor: "Автор — Ax1oM",
  aboutLicense: "Лицензия MIT, работает без интернета",
  projectPage: "Страница проекта",
};

type Dict = typeof ru;

const en: Dict = {
  add: "Add",
  addFiles: "Add files",
  addFolder: "Add folder",
  includeSubfolders: "Include subfolders",
  dropTitle: "Drop SVG files here",
  dropHint: "Files or folders. You can also paste SVG code — Ctrl+V.",
  dropRelease: "Release to add",
  files: "Files",
  onlyProblems: "Only with notes",
  clear: "Clear",
  remove: "Remove from list",
  converting: "Converting…",
  conversion: "Conversion",
  countOk: "Clean",
  countWarn: "With notes",
  countError: "Failed",
  ownSettings: "own settings",
  clipboardName: (n) => `Pasted ${n}`,
  summary: (ok, warn, err) =>
    [ok && `${ok} clean`, warn && `${warn} with notes`, err && `${err} failed`].filter(Boolean).join(", "),
  inChat: "In chat",
  closeUp: "Close-up",
  chatLight: "Light chat",
  chatDark: "Dark chat",
  checker: "Transparency",
  original: "Original SVG",
  result: "As Telegram shows it",
  chatMessage: "Look what I made",
  emojiMessage: "New emoji",
  noPreview: "Preview appears after conversion",
  allGood: "No notes — ready for Telegram.",
  fileFailed: "File was not converted",
  exportPanel: "Export",
  export: "Export",
  toSource: "Next to the source",
  toSourceHint: "Each TGS is saved next to its SVG",
  choose: "Choose",
  suffix: "Suffix",
  prefix: "Prefix",
  subfolder: "Subfolder",
  exportSettings: "Export settings",
  scopeAll: "all files",
  scopeOne: "this file",
  scopeOneHint: (name) => `Settings below apply only to ${name}. Other files use the common ones.`,
  outline: "Outline",
  outlineWidth: "Outline width, px",
  outlineColor: "Outline color",
  preset: "Sticker or emoji",
  sticker: "Sticker",
  emoji: "Emoji",
  object: "Object",
  padding: "Padding",
  fit: "Fit",
  fitContain: "Whole",
  fitContainHint: "Whole picture, transparent margins",
  fitCover: "Crop",
  fitCoverHint: "Fill the square, the excess is cropped",
  background: "Background",
  radius: "Corner radius",
  fill: "Fill",
  fillColor: "Background fill color",
  borderWidth: "Background outline width, px",
  borderColor: "Background outline color",
  optimization: "File size",
  optAuto: "Auto",
  optAutoHint: "Fit Telegram's 64 KB limit",
  optLossless: "Lossless",
  optLosslessHint: "No outline simplification",
  optManual: "Manual",
  optManualHint: "Manual precision",
  decimals: "Decimal places",
  advanced: "Advanced",
  fps: "Frames per second",
  duration: "Duration, s",
  telegramDefault: "as Telegram requires",
  settings: "Settings",
  closeUnchanged: "Close without changes",
  done: "Done",
  language: "Language",
  theme: "Theme",
  themeSystem: "System",
  themeLight: "Light",
  themeDark: "Dark",
  saved: (n) => `Saved: ${n}`,
  skipped: (n) => `skipped: ${n}`,
  failed: (n) => `failed: ${n}`,
  openFolder: "Open folder",
  pastedNeedFolder: "Pasted SVG: choose a folder",
  browserMode: "Browser mode: sample data",
  about: "About",
  version: (v) => `Version ${v}`,
  aboutProject: "A Chajnaya Axioma project",
  aboutAuthor: "Author — Ax1oM",
  aboutLicense: "MIT license, works offline",
  projectPage: "Project page",
};

export const strings: Record<Lang, Dict> = { ru, en };

const filterNames: Record<Lang, Record<string, string>> = {
  ru: {
    blur: "размытие",
    drop_shadow: "тень",
    offset: "смещение",
    color_matrix: "цветовая матрица",
    lighting: "освещение",
    turbulence: "шум",
  },
  en: {
    blur: "blur",
    drop_shadow: "drop shadow",
    offset: "offset",
    color_matrix: "color matrix",
    lighting: "lighting",
    turbulence: "noise",
  },
};

const kb = (b: unknown) => ((b as number) / 1024).toFixed(1);

/** Текст замечания для людей. */
export function warningText(w: Warning, lang: Lang): string {
  const ru = lang === "ru";
  const p = w as Record<string, unknown>;
  const filter = filterNames[lang][p.filter as string] ?? (p.filter as string);
  const text = (() => {
    switch (w.code) {
      case "text_converted_to_paths":
        return ru ? "Текст переведён в кривые" : "Text converted to outlines";
      case "filter_removed":
        return ru
          ? `Эффект «${filter}» в Telegram не поддерживается и убран`
          : `“${filter}” effect is not supported in Telegram and was removed`;
      case "mask_removed":
        return ru ? "Маска не поддерживается и убрана" : "Mask is not supported and was removed";
      case "image_removed":
        return ru
          ? "Растровая картинка внутри SVG не поддерживается и убрана"
          : "Raster image inside the SVG is not supported and was removed";
      case "pattern_replaced":
        return ru ? "Узор заменён серым цветом" : "Pattern replaced with gray";
      case "blend_mode_ignored":
        return ru
          ? `Режим наложения «${p.mode}» не поддерживается, использован обычный`
          : `Blend mode “${p.mode}” is not supported, normal is used`;
      case "size_over_limit":
        return ru
          ? `Файл ${kb(p.bytes)} КБ при лимите Telegram ${kb(p.limit)} КБ — упростите рисунок`
          : `File is ${kb(p.bytes)} KB, Telegram allows ${kb(p.limit)} KB — simplify the drawing`;
      case "text_dropped":
        return ru ? "Текст пропущен: нет подходящего шрифта" : "Text skipped: no matching font";
      case "clip_approximated":
        return ru ? "Сложная обрезка передана приближённо" : "Complex clipping is approximated";
      case "gradient_approximated":
        return ru ? "Градиент передан приближённо" : "Gradient is approximated";
      case "canvas_not_allowed":
        return ru
          ? `Канвас ${p.size}×${p.size}, а Telegram требует ${p.required}×${p.required}`
          : `Canvas is ${p.size}×${p.size}, Telegram requires ${p.required}×${p.required}`;
      case "fps_not_allowed":
        return ru
          ? `${p.fps} кадров/с, а Telegram требует ${p.required}`
          : `${p.fps} fps, Telegram requires ${p.required}`;
      case "duration_too_long":
        return ru
          ? `Длительность ${(p.secs as number).toFixed(2)} с, Telegram разрешает до ${p.max} с`
          : `Duration is ${(p.secs as number).toFixed(2)} s, Telegram allows up to ${p.max} s`;
      case "simplified":
        return ru
          ? `Чтобы уложиться в лимит, контуры упрощены — отклонение до ${p.tolerance} px из 512`
          : `Outlines simplified to fit the limit — up to ${p.tolerance} px off out of 512`;
      case "too_complex":
        return ru
          ? `Слишком сложная фигура (${p.points} точек при пределе ${p.limit}): Telegram её не нарисует`
          : `Shape is too complex (${p.points} points, limit ${p.limit}): Telegram won't draw it`;
      case "thin_lines_for_emoji":
        return ru
          ? `Линии ${p.width} px из 512 почти пропадут в строке текста — лучше от ${p.recommended} px`
          : `${p.width} px lines (of 512) will nearly vanish in a text line — use ${p.recommended} px or more`;
      default:
        return w.code;
    }
  })();
  return w.element_id ? `${text} (${ru ? "элемент" : "element"} «${w.element_id}»)` : text;
}

/** Текст ошибки конвертации. */
export function errorText(code: string, lang: Lang): string {
  const ru = lang === "ru";
  switch (code) {
    case "read_failed":
      return ru ? "Не удалось прочитать файл" : "Could not read the file";
    case "invalid_svg":
      return ru ? "Файл повреждён или это не SVG" : "The file is damaged or not an SVG";
    case "empty":
      return ru ? "В SVG нет ничего видимого" : "Nothing visible in the SVG";
    case "invalid_option":
      return ru ? "Неверный цвет — нужен вид #rrggbb" : "Invalid color — use #rrggbb";
    default:
      return code;
  }
}

/** Важность замечания: справочное или предупреждение (как `Severity` в core). */
export function isInfo(w: Warning): boolean {
  if (w.code === "text_converted_to_paths") return true;
  if (w.code === "simplified") return (w.tolerance as number) < 0.5;
  return false;
}

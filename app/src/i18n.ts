/**
 * Тексты интерфейса. Сами переводы — файлы данных `locales/<код>.json`
 * (только строки, никакого кода): чтобы добавить язык, достаточно положить
 * рядом новый файл — он подхватится сам, а `npm run check-locales` (и сборка)
 * проверит, что в нём те же ключи и те же подстановки `{имя}`, что в ru.json.
 *
 * Здесь — только то, что относится к коду: какие строки принимают
 * параметры (и в каком порядке), сборка составных фраз, выбор языка.
 * Тексты выводятся как текст (React экранирует), адреса ссылок — в коде
 * и в Rust, поэтому ошибка в переводе максимум испортит надпись.
 */

import type { Warning } from "./types";
import ru from "./locales/ru.json";

/** Код языка — имя файла перевода: "ru", "en", "pt-BR"… */
export type Lang = string;

type Raw = Record<string, string>;

const files = import.meta.glob<{ default: Raw }>("./locales/*.json", { eager: true });

/** Переводы по коду языка. */
const LOCALES: Record<Lang, Raw> = Object.fromEntries(
  Object.entries(files).map(([path, mod]) => [path.replace(/^.*\/|\.json$/g, ""), mod.default]),
);

/** Языки для выбора в настройках: код и название на самом языке. */
export const LANGS: { code: Lang; name: string }[] = Object.entries(LOCALES)
  .map(([code, raw]) => ({ code, name: raw._language ?? code }))
  .sort((a, b) => (a.code === "ru" ? -1 : b.code === "ru" ? 1 : a.name.localeCompare(b.name)));

/** Строки с параметрами: имена подстановок по порядку аргументов. */
const PARAMS = {
  clipboardName: ["n"],
  aboutVersion: ["v"],
  scopeOneHint: ["name"],
  saved: ["n"],
  skipped: ["n"],
  failed: ["n"],
  version: ["v"],
  resetSection: ["title"],
  ownSettingsToast: ["name"],
  exists: ["name"],
  applyAll: ["n"],
  cropRotation: ["deg"],
  sliceInfo: ["n", "first", "last"],
  sliceSource: ["c", "r"],
  sliceResult: ["n", "size"],
  exportN: ["n"],
  exportOne: ["name"],
  exportOneTiles: ["name", "n"],
  exportErrors: ["n"],
  scopeSelectedHint: ["n"],
  ownSettingsToastMany: ["n"],
  importHelp: ["target"],
  importTargetOwn: ["name"],
  importTargetMany: ["n"],
  importOk: ["n"],
  importErr_syntax: ["line", "column"],
  importErr_value: ["line", "column"],
  tgBotFound: ["bot"],
  tgOwner: ["owner"],
  tgPublishGrid: ["name"],
  pubVia: ["bot", "owner"],
  pubEmojiFor: ["name"],
  pubCell: ["n"],
  pubGridInfo: ["n", "name", "c", "r"],
  pubGo: ["n"],
  pubProgress: ["done", "total"],
  pubWait: ["secs"],
  pubDoneText: ["n", "title"],
  pubFound: ["title", "count", "free"],
  pubFoundNoRoom: ["title", "count", "free", "n"],
  pubGoAppend: ["n"],
  pubDoneAppend: ["n", "title"],
  webFontToast: ["name"],
} as const satisfies Partial<Record<keyof typeof ru, readonly string[]>>;

type Key = Exclude<keyof typeof ru, `_${string}`>;
type Arg = string | number;
export type Dict = {
  [K in Key]: K extends keyof typeof PARAMS ? (...args: Arg[]) => string : string;
} & {
  /** «3 без замечаний, 1 с замечаниями, 1 с ошибкой» — нулевые части пропускаются. */
  summary: (ok: number, warn: number, err: number) => string;
};

/** Подставляет `{имя}` из `values`; неизвестные подстановки остаются как есть. */
function fill(template: string, values: Record<string, Arg>): string {
  return template.replace(/\{(\w+)\}/g, (m, name: string) => (name in values ? String(values[name]) : m));
}

function build(raw: Raw): Dict {
  // Недостающая строка берётся из русского — надпись не пропадёт.
  const get = (k: string) => (typeof raw[k] === "string" ? raw[k] : (ru as Raw)[k]);
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(ru)) {
    if (key.startsWith("_")) continue;
    const names = (PARAMS as Record<string, readonly string[]>)[key];
    out[key] = names
      ? (...args: Arg[]) => fill(get(key), Object.fromEntries(names.map((n, i) => [n, args[i]])))
      : get(key);
  }
  out.summary = (ok: number, warn: number, err: number) =>
    [ok && fill(get("summaryOk"), { n: ok }), warn && fill(get("summaryWarn"), { n: warn }), err && fill(get("summaryError"), { n: err })]
      .filter(Boolean)
      .join(", ");
  return out as Dict;
}

const DICTS: Record<Lang, Dict> = Object.fromEntries(Object.entries(LOCALES).map(([code, raw]) => [code, build(raw)]));

/** Есть ли такой перевод. */
export const hasLang = (lang: string): boolean => lang in LOCALES;

/** Язык системы, если для него есть перевод (сначала точное совпадение
 *  вида pt-BR, затем основной язык pt), иначе английский. */
export function systemLang(): Lang {
  const prefs = navigator.languages?.length ? navigator.languages : [navigator.language || "en"];
  for (const tag of prefs) {
    const exact = Object.keys(LOCALES).find((c) => c.toLowerCase() === tag.toLowerCase());
    if (exact) return exact;
    const base = tag.split("-")[0].toLowerCase();
    if (hasLang(base)) return base;
  }
  return hasLang("en") ? "en" : "ru";
}

/** Тексты на языке `lang` (неизвестный язык — английский). */
export const strings: Record<Lang, Dict> = new Proxy(DICTS, {
  get: (target, lang: string) => target[lang] ?? target.en ?? target.ru,
});

/** Текст замечания для людей. */
export function warningText(w: Warning, lang: Lang): string {
  const t = strings[lang] as unknown as Record<string, unknown>;
  const p = w as Record<string, unknown>;
  const tpl = t[`warn_${w.code}`];
  if (typeof tpl !== "string") return w.code;
  const filter = typeof p.filter === "string" ? (t[`filter_${p.filter}`] as string | undefined) ?? p.filter : "";
  const kb = (b: unknown) => ((b as number) / 1024).toFixed(1);
  const values: Record<string, Arg> = { ...(p as Record<string, Arg>), filter };
  if (w.code === "size_over_limit") Object.assign(values, { size: kb(p.bytes), limit: kb(p.limit) });
  if (w.code === "duration_too_long") values.secs = (p.secs as number).toFixed(2);
  const text = fill(tpl, values);
  return w.element_id ? fill(t.warnElement as string, { text, id: w.element_id }) : text;
}

/** Текст ошибки конвертации. */
export function errorText(code: string, lang: Lang): string {
  const tpl = (strings[lang] as unknown as Record<string, unknown>)[`error_${code}`];
  return typeof tpl === "string" ? tpl : code;
}

/** Текст ошибки выгрузки в Telegram: шаблон `tgErr_<код>`, номера файлов — с 1. */
export function tgErrorText(e: { code: string; [k: string]: unknown }, lang: Lang): string {
  const tpl = (strings[lang] as unknown as Record<string, unknown>)[`tgErr_${e.code}`];
  if (typeof tpl !== "string") return typeof e.detail === "string" ? e.detail : e.code;
  const values: Record<string, Arg> = {};
  for (const [k, v] of Object.entries(e)) if (typeof v === "string" || typeof v === "number") values[k] = v;
  if (typeof e.index === "number") values.index = e.index + 1;
  return fill(tpl, values);
}

/** Важность замечания: справочное или предупреждение (как `Severity` в core). */
export function isInfo(w: Warning): boolean {
  if (w.code === "text_converted_to_paths") return true;
  if (w.code === "simplified") return (w.tolerance as number) < 0.5;
  return false;
}

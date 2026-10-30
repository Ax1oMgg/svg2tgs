/**
 * Мост к Rust-части приложения.
 *
 * Внутри Tauri вызовы идут в команды из `src-tauri`. В обычном браузере
 * (`npm run dev` без Tauri) подставляются тестовые данные — так интерфейс
 * можно открыть отдельно, например для сверки с макетом.
 */

import { invoke } from "@tauri-apps/api/core";
import type { Existing, Options, Outcome, SaveRequest, Saved, Source } from "./types";

export interface AppInfo {
  version: string;
  coreVersion: string;
  /** "tauri" — настоящее приложение, "browser" — режим с тестовыми данными. */
  runtime: "tauri" | "browser";
}

export interface Entry {
  path: string;
  name: string;
}

export const isTauri = (): boolean => "__TAURI_INTERNALS__" in window;

export async function getAppInfo(): Promise<AppInfo> {
  if (!isTauri()) {
    return { version: "0.1.5", coreVersion: "0.1.5", runtime: "browser" };
  }
  const info = await invoke<Omit<AppInfo, "runtime">>("app_info");
  return { ...info, runtime: "tauri" };
}

export async function startupPaths(): Promise<string[]> {
  return isTauri() ? invoke<string[]>("startup_paths") : [];
}

export async function expandPaths(paths: string[], recursive: boolean): Promise<Entry[]> {
  return isTauri() ? invoke<Entry[]>("expand_paths", { paths, recursive }) : [];
}

export async function convert(id: string, source: Source, options: Options): Promise<Outcome> {
  if (isTauri()) return invoke<Outcome>("convert", { id, source, options });
  return mock.convert(id, source, options);
}

/** Картинка превью (data-URL PNG) или `null`. */
export async function preview(id: string, kind: "svg" | "tgs" | "source", size: number): Promise<string | null> {
  if (isTauri()) return invoke<string | null>("preview", { id, kind, size });
  return mock.preview(id);
}

export async function forget(ids: string[]): Promise<void> {
  if (isTauri()) await invoke("forget", { ids });
  else ids.forEach((id) => mock.svgs.delete(id));
}

/** Размер рисунка: ширина и высота области просмотра SVG. */
export async function sourceSize(id: string): Promise<[number, number] | null> {
  if (isTauri()) return invoke<[number, number] | null>("source_size", { id });
  const svg = mock.svgs.get(id);
  const vb = svg?.match(/viewBox="[\d.\s-]*?([\d.]+)\s+([\d.]+)"/);
  return vb ? [Number(vb[1]), Number(vb[2])] : null;
}

/** Какие результаты лягут на уже существующие файлы. */
export async function findExisting(requests: SaveRequest[]): Promise<Existing[]> {
  if (isTauri()) return invoke<Existing[]>("find_existing", { requests });
  // Браузерный режим: «уже есть» первый файл — чтобы показать окно.
  const r = requests[0];
  return r ? [{ id: r.id, fileName: `${r.name}.tgs`, path: `/demo/${r.name}.tgs`, suggested: `${r.name}_1` }] : [];
}

/** Сохраняет готовые TGS; совпадения без `overwrite` получают номер. */
export async function save(requests: SaveRequest[]): Promise<Saved[]> {
  if (isTauri()) return invoke<Saved[]>("save", { requests, conflict: "number" });
  return requests.map((r) => {
    const dir = r.dir ?? (r.source ? r.source.replace(/[\\/][^\\/]*$/, "") : null);
    if (!dir) return { id: r.id, status: "failed", path: null, count: 0 };
    const sub = r.subfolder ? `/${r.subfolder}` : "";
    const n = mock.grids.get(r.id) ?? 1;
    const name = n > 1 ? `${r.name}_01` : r.name;
    return { id: r.id, status: "saved", path: `${dir}${sub}/${name}.tgs`, count: n };
  });
}

export async function openFolder(path: string): Promise<void> {
  if (isTauri()) await invoke("open_folder", { path });
}

export type Link = "project" | "issues" | "telegram" | "email";

/** Адреса ссылок «О программе» (в Rust они же — белый список). */
export const LINKS: Record<Link, string> = {
  project: "https://github.com/Ax1oMgg/svg2tgs",
  issues: "https://github.com/Ax1oMgg/svg2tgs/issues",
  telegram: "https://t.me/chajnaya_Axioma",
  email: "mailto:chajnayaaxioma@gmail.com",
};

/** Открыть ссылку в браузере пользователя (сама программа в сеть не ходит). */
export async function openLink(link: Link): Promise<void> {
  if (isTauri()) await invoke("open_link", { link });
  else window.open(LINKS[link], "_blank", "noopener");
}


/** Системный диалог: выбрать SVG-файлы или папку. */
export async function pickPaths(kind: "files" | "folder"): Promise<string[]> {
  if (!isTauri()) return [];
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open(
    kind === "files"
      ? { multiple: true, filters: [{ name: "SVG", extensions: ["svg"] }] }
      : { directory: true, multiple: false },
  );
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}

/** Системный диалог: выбрать папку для экспорта. */
export async function pickFolder(defaultPath?: string, title?: string): Promise<string | null> {
  if (!isTauri()) return defaultPath ?? "C:\\Users\\demo\\Documents\\stickers";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, defaultPath, title });
  return typeof picked === "string" ? picked : null;
}

/** Подписка на перетаскивание файлов в окно. Возвращает отписку. */
export async function onFileDrop(handlers: {
  over: (active: boolean) => void;
  drop: (paths: string[]) => void;
}): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") handlers.over(true);
    else if (p.type === "leave") handlers.over(false);
    else if (p.type === "drop") {
      handlers.over(false);
      handlers.drop(p.paths);
    }
  });
}

/* ------------------------------------------------------------------------- */
/* Тестовые данные для браузерного режима                                     */
/* ------------------------------------------------------------------------- */

const SAMPLE_SVGS: Record<string, string> = {
  "heart.svg": `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#e8505b" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 21s-7-4.35-7-10a4 4 0 0 1 7-2.65A4 4 0 0 1 19 11c0 5.65-7 10-7 10z"/></svg>`,
  "star.svg": `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#f5b400" d="M12 2l3 7h7l-5.5 4.5L18.5 21 12 16.5 5.5 21l2-7.5L2 9h7z"/></svg>`,
  "rocket.svg": `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path fill="#3157d5" d="M32 4c10 8 14 20 12 34H20C18 24 22 12 32 4z"/><circle cx="32" cy="24" r="6" fill="#fff"/><path fill="#f08c2a" d="M24 40h16l-8 18z"/></svg>`,
  "shadow.svg": `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="38" fill="#1e9e62"/><circle cx="38" cy="42" r="6" fill="#fff"/><circle cx="62" cy="42" r="6" fill="#fff"/></svg>`,
  "broken.svg": "не svg",
};

const mock = {
  svgs: new Map<string, string>(),
  /** Сколько клеток нарезки у файла. */
  grids: new Map<string, number>(),

  async convert(id: string, source: Source, options: Options): Promise<Outcome> {
    await new Promise((r) => setTimeout(r, 150 + Math.random() * 250));
    const text =
      source.kind === "text"
        ? source.text
        : SAMPLE_SVGS[source.path.split("/").pop() ?? ""] ?? SAMPLE_SVGS["star.svg"];
    if (!text.includes("<svg")) {
      return { id, tgsBytes: null, grid: [1, 1], warnings: [], error: "invalid_svg" };
    }
    mock.svgs.set(id, text);
    // Нарезка: авто — по пропорциям viewBox, до 8 в ряд (как в ядре).
    let grid: [number, number] = [1, 1];
    if (options.grid) {
      grid = [options.grid.cols, options.grid.rows];
      if (options.grid_auto !== false) {
        const vb = text.match(/viewBox="[\d.\s-]*?([\d.]+)\s+([\d.]+)"/);
        grid = autoGrid(vb ? Number(vb[1]) / Number(vb[2]) : 1);
      }
    }
    mock.grids.set(id, grid[0] * grid[1]);
    const warnings =
      source.kind === "path" && source.path.endsWith("shadow.svg")
        ? [{ code: "filter_removed", filter: "drop_shadow", element_id: "shadow" }]
        : options.preset === "emoji" && text.includes("stroke-width")
          ? [{ code: "thin_lines_for_emoji", width: 8.5, recommended: 12 }]
          : [];
    return { id, tgsBytes: 300 + text.length * 2, grid, warnings, error: null };
  },

  async preview(id: string): Promise<string | null> {
    const svg = mock.svgs.get(id);
    return svg ? `data:image/svg+xml;utf8,${encodeURIComponent(svg)}` : null;
  },
};

/** Сетка по пропорциям (как `Grid::auto` в ядре). */
function autoGrid(aspect: number): [number, number] {
  let best: [number, number, number] = [1, 1, Infinity];
  for (let c = 1; c <= 8; c++) {
    const r = Math.min(20, Math.max(1, Math.round(c / aspect)));
    const err = Math.abs(Math.log(c / r / aspect));
    if (err < best[2] - 1e-9 || (Math.abs(err - best[2]) <= 1e-9 && c > best[0])) best = [c, r, err];
  }
  return [best[0], best[1]];
}

/** Файлы для демонстрации в браузерном режиме. */
export const SAMPLE_PATHS = Object.keys(SAMPLE_SVGS).map((n) => `/demo/${n}`);

/**
 * Веб-версия: то же, что команды Tauri в установленной программе, но в
 * браузере. Ядро работает в фоновом потоке (`worker.ts`), файлы читаются
 * из выбранных или перетащенных, результат скачивается файлом или архивом,
 * а в Chrome и Edge может записываться в выбранную папку.
 *
 * Подключается только в сборке `--mode web` (псевдоним `@web` в
 * `vite.config.ts`); в установленной программе вместо него — `stub.ts`.
 * Никаких запросов, кроме загрузки своих файлов сайта.
 */

import lottie from "lottie-web/build/player/lottie_light_canvas";
import type {
  Existing,
  Options,
  OptionsError,
  Outcome,
  ParsedOptions,
  SaveRequest,
  Saved,
  Source,
} from "../types";
import type { Request, Response } from "./worker";
import wasmUrl from "./wasm/svg2tgs_web.wasm?url";

/* ------------------------------ Фоновый поток ------------------------------ */

const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
worker.postMessage({ wasmUrl: new URL(wasmUrl, location.href).href });

let seq = 0;
const pending = new Map<number, (r: Response) => void>();
worker.onmessage = (e: MessageEvent<Response>) => {
  pending.get(e.data.seq)?.(e.data);
  pending.delete(e.data.seq);
};

/** Ядро упало и загружено заново — прочитанные SVG надо передать снова. */
class Crash extends Error {}

function call(cmd: Record<string, unknown>, data?: ArrayBuffer): Promise<{ code: number; bytes: Uint8Array }> {
  const n = ++seq;
  return new Promise((resolve, reject) => {
    pending.set(n, (r) => {
      if (r.crash != null) {
        known.clear();
        reject(new Crash(r.crash));
      } else resolve({ code: r.code ?? 2, bytes: new Uint8Array(r.bytes ?? new ArrayBuffer(0)) });
    });
    worker.postMessage({ seq: n, cmd, data } satisfies Request, data ? [data] : []);
  });
}

const decoder = new TextDecoder();
async function callJson<T>(cmd: Record<string, unknown>, data?: ArrayBuffer): Promise<T> {
  const r = await call(cmd, data);
  const value = JSON.parse(decoder.decode(r.bytes));
  if (r.code === 2) throw new Error(value.error ?? "error");
  return value as T;
}

async function callBytes(cmd: Record<string, unknown>): Promise<Uint8Array | null> {
  const r = await call(cmd);
  if (r.code !== 1) return null;
  return r.bytes.length > 0 ? r.bytes : null;
}

/* --------------------------------- Файлы ---------------------------------- */

/** Добавленный файл: сам файл и глубина в выбранной папке (0 — прямо в ней). */
interface Picked {
  file: File;
  depth: number;
}

/** Файлы под условными путями `web:<n>/<путь в папке>` — путь нужен списку,
 *  чтобы различать файлы, и для подписи; на диск он не указывает. */
const registry = new Map<string, Picked>();
let added = 0;

const isSvg = (name: string) => /\.svg$/i.test(name);

/** Запомнить файлы; `rel` — путь внутри выбранной папки или имя файла. */
function register(files: { file: File; rel: string }[]): string[] {
  const batch = ++added;
  return files
    .filter((f) => isSvg(f.file.name))
    .sort((a, b) => a.rel.localeCompare(b.rel))
    .map(({ file, rel }) => {
      const path = `web:${batch}/${rel}`;
      registry.set(path, { file, depth: Math.max(0, rel.split("/").length - 2) });
      return path;
    });
}

/** Выбор файлов или папки обычным полем выбора (работает во всех браузерах). */
export function pickPaths(kind: "files" | "folder"): Promise<string[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    if (kind === "files") {
      input.multiple = true;
      input.accept = ".svg,image/svg+xml";
    } else {
      input.webkitdirectory = true;
    }
    input.onchange = () => {
      const files = [...(input.files ?? [])];
      resolve(register(files.map((file) => ({ file, rel: file.webkitRelativePath || file.name }))));
    };
    input.oncancel = () => resolve([]);
    input.click();
  });
}

/** Файлы для списка; без `recursive` — только лежащие прямо в папке. */
export function expandPaths(paths: string[], recursive: boolean): { path: string; name: string }[] {
  return paths
    .filter((p) => registry.has(p) && (recursive || registry.get(p)!.depth === 0))
    .map((p) => ({ path: p, name: registry.get(p)!.file.name }));
}

/** Перетаскивание файлов и папок в окно. */
export function onFileDrop(handlers: { over: (active: boolean) => void; drop: (paths: string[]) => void }): () => void {
  let depth = 0;
  const hasFiles = (e: DragEvent) => !!e.dataTransfer?.types.includes("Files");
  const enter = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    depth += 1;
    handlers.over(true);
  };
  const over = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  };
  const leave = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    depth = Math.max(0, depth - 1);
    if (depth === 0) handlers.over(false);
  };
  const drop = (e: DragEvent) => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    depth = 0;
    handlers.over(false);
    // Папки видны только через webkitGetAsEntry; без него — просто файлы.
    const entries: FileSystemEntry[] = [];
    const plain: { file: File; rel: string }[] = [];
    for (const item of e.dataTransfer?.items ?? []) {
      const entry = item.webkitGetAsEntry?.();
      const file = entry ? null : item.getAsFile();
      if (entry) entries.push(entry);
      else if (file) plain.push({ file, rel: file.name });
    }
    void collect(entries).then((files) => handlers.drop(register([...plain, ...files])));
  };
  document.addEventListener("dragenter", enter);
  document.addEventListener("dragover", over);
  document.addEventListener("dragleave", leave);
  document.addEventListener("drop", drop);
  return () => {
    document.removeEventListener("dragenter", enter);
    document.removeEventListener("dragover", over);
    document.removeEventListener("dragleave", leave);
    document.removeEventListener("drop", drop);
  };
}

/** Файлы из перетащенного: папки — со всем содержимым. */
async function collect(entries: FileSystemEntry[]): Promise<{ file: File; rel: string }[]> {
  const out: { file: File; rel: string }[] = [];
  const walk = async (entry: FileSystemEntry, top: boolean): Promise<void> => {
    if (entry.isFile) {
      const file = await new Promise<File>((res, rej) => (entry as FileSystemFileEntry).file(res, rej));
      // Отдельный файл — без папки; в папке — путь от неё.
      out.push({ file, rel: top ? file.name : entry.fullPath.replace(/^\//, "") });
      return;
    }
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    // readEntries отдаёт папку частями, пока не вернёт пустой список.
    for (;;) {
      const part = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
      if (part.length === 0) break;
      for (const child of part) await walk(child, false);
    }
  };
  for (const entry of entries) {
    try {
      await walk(entry, true);
    } catch {
      // Недоступный файл просто не добавляется.
    }
  }
  return out;
}

/* ------------------------------- Конвертация ------------------------------ */

/** Файлы, SVG которых уже в ядре: повторно передавать не нужно. */
const known = new Set<string>();

async function sourceBytes(source: Source): Promise<ArrayBuffer | null> {
  if (source.kind === "text") return new TextEncoder().encode(source.text).buffer as ArrayBuffer;
  const picked = registry.get(source.path);
  return picked ? picked.file.arrayBuffer().catch(() => null) : null;
}

const failed = (id: string, error: string): Outcome => ({ id, tgsBytes: null, grid: [1, 1], aspect: 1, warnings: [], error });

export async function coreVersion(): Promise<string> {
  return (await callJson<{ coreVersion: string }>({ cmd: "init" })).coreVersion;
}

export async function convert(id: string, source: Source, options: Options): Promise<Outcome> {
  const run = async (fresh: boolean): Promise<Outcome> => {
    const data = fresh ? await sourceBytes(source) : undefined;
    if (fresh && !data) return failed(id, "read_failed");
    const out = await callJson<Outcome>({ cmd: "convert", id, options }, data ?? undefined);
    known.add(id);
    return out;
  };
  try {
    const out = await run(!known.has(id));
    // Ядро загружалось заново и забыло SVG — передать ещё раз.
    return out.error === "read_failed" ? await run(true) : out;
  } catch (e) {
    if (e instanceof Crash) return failed(id, "internal");
    throw e;
  }
}

export async function forget(ids: string[]): Promise<void> {
  ids.forEach((id) => known.delete(id));
  await callJson({ cmd: "forget", ids });
}

export async function sourceSize(id: string): Promise<[number, number] | null> {
  return callJson<[number, number] | null>({ cmd: "source_size", id });
}

export async function parseOptions(text: string): Promise<ParsedOptions | OptionsError> {
  return callJson<ParsedOptions | OptionsError>({ cmd: "parse_options", text });
}

export async function optionsJson(options: Options): Promise<string> {
  return callJson<string>({ cmd: "options_json", options });
}

/* ---------------------------------- Превью --------------------------------- */

/** Превью: исходник рисует resvg в ядре, результат — lottie-web здесь. */
export async function preview(id: string, kind: "svg" | "tgs" | "source", size: number): Promise<string | null> {
  if (kind !== "tgs") {
    const png = await callBytes({ cmd: "preview", id, kind, size });
    return png ? `data:image/png;base64,${base64(png)}` : null;
  }
  const lot = await callJson<{ cols: number; rows: number; tiles: object[] } | null>({ cmd: "lottie", id });
  if (!lot || lot.tiles.length === 0) return null;
  const side = Math.max(lot.cols, lot.rows);
  const cell = lot.tiles.length > 1 ? Math.max(16, Math.floor(size / side)) : size;
  const out = document.createElement("canvas");
  out.width = lot.cols * cell;
  out.height = lot.rows * cell;
  const ctx = out.getContext("2d")!;
  for (const [n, data] of lot.tiles.entries()) {
    const tile = await drawLottie(data, cell);
    ctx.drawImage(tile, (n % lot.cols) * cell, Math.floor(n / lot.cols) * cell);
  }
  return out.toDataURL("image/png");
}

/** Первый кадр Lottie на холсте `size`×`size` (lottie-web, отрисовка в canvas). */
function drawLottie(data: object, size: number): Promise<HTMLCanvasElement> {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const context = canvas.getContext("2d")!;
  return new Promise((resolve) => {
    const anim = lottie.loadAnimation({
      renderer: "canvas",
      loop: false,
      autoplay: false,
      animationData: data,
      rendererSettings: { context, clearCanvas: true, preserveAspectRatio: "xMidYMid meet" },
    } as Parameters<typeof lottie.loadAnimation>[0]);
    // Размер берётся из холста при загрузке; resize() здесь не вызывать:
    // в lottie-web 5.13 после него canvas-рендер теряет цвет заливки.
    const done = () => {
      anim.goToAndStop(0, true);
      anim.destroy();
      resolve(canvas);
    };
    if (anim.isLoaded) done();
    else anim.addEventListener("DOMLoaded", done);
  });
}

function base64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

/* --------------------------------- Экспорт --------------------------------- */
/* Без выбранной папки — скачивание: один файл — .tgs, несколько — .zip.
   С папкой (Chrome, Edge: File System Access API) — запись в неё, как в
   установленной программе: совпадения — вопрос, «номер» — `имя_1`. */

/** Можно ли выбрать папку экспорта в этом браузере. */
export const canPickFolder = (): boolean => "showDirectoryPicker" in window;

let exportDir: FileSystemDirectoryHandle | null = null;

/** Выбрать папку экспорта; в настройки попадает только её имя. */
export async function pickFolder(): Promise<string | null> {
  if (!canPickFolder()) return null;
  try {
    const picker = (window as unknown as { showDirectoryPicker: (o: object) => Promise<FileSystemDirectoryHandle> })
      .showDirectoryPicker;
    exportDir = await picker({ id: "svg2tgs-export", mode: "readwrite" });
    await storeHandle(exportDir);
    return exportDir.name;
  } catch {
    return null; // отменено
  }
}

/** Вернуть папку из прошлого раза (имя — из настроек); `false` — её нет. */
export async function restoreFolder(name: string): Promise<boolean> {
  if (!canPickFolder()) return false;
  const handle = await loadHandle();
  if (handle && handle.name === name) {
    exportDir = handle;
    return true;
  }
  return false;
}

/** Папку убрали в панели — дальше скачивание. */
export function clearFolder(): void {
  exportDir = null;
  void storeHandle(null);
}

/** Разрешение на запись — его можно спросить только сразу по нажатию,
 *  поэтому экспорт начинается с этого. Нет разрешения — скачивание. */
export async function prepareExport(): Promise<void> {
  writable = true;
  if (!exportDir) return;
  const h = exportDir as FileSystemDirectoryHandle & {
    queryPermission(o: object): Promise<PermissionState>;
    requestPermission(o: object): Promise<PermissionState>;
  };
  try {
    if ((await h.queryPermission({ mode: "readwrite" })) === "granted") return;
    if ((await h.requestPermission({ mode: "readwrite" })) === "granted") return;
  } catch {
    // Браузер не дал спросить — тоже скачивание.
  }
  writable = false;
}
let writable = true;

const useFolder = () => exportDir && writable;

interface FilesReply {
  folder: string | null;
  files: { id: string; names: string[] }[];
}

const filesOf = (requests: SaveRequest[], folder?: string | null) =>
  callJson<FilesReply>({ cmd: "files", items: requests.map((r) => ({ id: r.id, name: r.name })), folder: folder ?? null });

async function subdir(folder: string | null, create: boolean): Promise<FileSystemDirectoryHandle | null> {
  if (!exportDir) return null;
  if (!folder) return exportDir;
  return exportDir.getDirectoryHandle(folder, { create }).catch(() => null);
}

const exists = (dir: FileSystemDirectoryHandle | null, name: string) =>
  dir ? dir.getFileHandle(`${name}.tgs`).then(() => true, () => false) : Promise.resolve(false);

async function anyExists(dir: FileSystemDirectoryHandle | null, names: string[]): Promise<string | null> {
  for (const n of names) if (await exists(dir, n)) return n;
  return null;
}

/** Свободное имя `имя_1`, `имя_2`… для всех клеток сразу. */
async function freeName(dir: FileSystemDirectoryHandle | null, r: SaveRequest): Promise<string> {
  for (let k = 1; ; k++) {
    const name = `${r.name}_${k}`;
    const { files } = await filesOf([{ ...r, name }]);
    if (!(await anyExists(dir, files[0].names))) return name;
  }
}

export async function findExisting(requests: SaveRequest[]): Promise<Existing[]> {
  if (!useFolder()) return [];
  const { folder, files } = await filesOf(requests, requests[0]?.subfolder);
  const dir = await subdir(folder, false);
  const out: Existing[] = [];
  for (const [i, r] of requests.entries()) {
    const names = files[i].names;
    const first = await anyExists(dir, names);
    if (!first) continue;
    out.push({
      id: r.id,
      fileName: names.length > 1 ? `${names[0]}.tgs … ${names[names.length - 1]}.tgs` : `${names[0]}.tgs`,
      path: [exportDir!.name, folder, `${first}.tgs`].filter(Boolean).join("/"),
      suggested: await freeName(dir, r),
    });
  }
  return out;
}

export async function save(requests: SaveRequest[]): Promise<Saved[]> {
  if (requests.length === 0) return [];
  return useFolder() ? saveToFolder(requests) : download(requests);
}

async function saveToFolder(requests: SaveRequest[]): Promise<Saved[]> {
  const sub = requests[0].subfolder ?? null;
  const { folder } = await filesOf([], sub);
  const dir = await subdir(folder, true);
  const out: Saved[] = [];
  for (const r of requests) {
    try {
      if (!dir) throw new Error("no_dir");
      let { files } = await filesOf([r]);
      // Совпадение без «заменить» — свободное имя с номером.
      if (!r.overwrite && (await anyExists(dir, files[0].names))) {
        ({ files } = await filesOf([{ ...r, name: await freeName(dir, r) }]));
      }
      const names = files[0].names;
      for (const [tile, name] of names.entries()) {
        const tgs = await callBytes({ cmd: "tgs", id: r.id, tile });
        if (!tgs) throw new Error("missing");
        const handle = await dir.getFileHandle(`${name}.tgs`, { create: true });
        const w = await (handle as FileSystemFileHandle & { createWritable(): Promise<FileSystemWritableFileStream> }).createWritable();
        await w.write(tgs as BlobPart);
        await w.close();
      }
      out.push({ id: r.id, status: "saved", path: [exportDir!.name, folder, `${names[0]}.tgs`].filter(Boolean).join("/"), count: names.length });
    } catch {
      out.push({ id: r.id, status: "failed", path: null, count: 0 });
    }
  }
  return out;
}

async function download(requests: SaveRequest[]): Promise<Saved[]> {
  const sub = requests[0].subfolder ?? null;
  const { folder, files } = await filesOf(requests, sub);
  const total = files.reduce((n, f) => n + f.names.length, 0);
  // Один файл без подпапки — сам .tgs, иначе архив.
  if (total === 1 && !folder) {
    const tgs = await callBytes({ cmd: "tgs", id: requests[0].id, tile: 0 });
    if (!tgs) return requests.map((r) => ({ id: r.id, status: "failed", path: null, count: 0 }));
    const name = `${files[0].names[0]}.tgs`;
    offer(new Blob([tgs as BlobPart], { type: "application/x-tgsticker" }), name);
    return [{ id: requests[0].id, status: "saved", path: name, count: 1 }];
  }
  const zip = await callBytes({ cmd: "zip", items: requests.map((r) => ({ id: r.id, name: r.name })), folder });
  if (!zip) return requests.map((r) => ({ id: r.id, status: "failed", path: null, count: 0 }));
  const name = `${folder ?? (requests.length === 1 ? files[0].names[0].replace(/_\d+$/, "") : "svg2tgs")}.zip`;
  offer(new Blob([zip as BlobPart], { type: "application/zip" }), name);
  return requests.map((r, i) => ({ id: r.id, status: "saved", path: name, count: files[i].names.length }));
}

/** Скачать как файл. */
function offer(blob: Blob, name: string): void {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  // Сразу отзывать нельзя: часть браузеров начинает скачивание позже.
  setTimeout(() => URL.revokeObjectURL(a.href), 60_000);
}

/* ------------------------- Папка экспорта в IndexedDB ------------------------ */
/* Сама папка (не путь) хранится в браузере: доступ к ней браузер спросит
   при первом экспорте после перезапуска. */

const DB = "svg2tgs";
const STORE = "handles";

function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function storeHandle(h: FileSystemDirectoryHandle | null): Promise<void> {
  try {
    const d = await db();
    const tx = d.transaction(STORE, "readwrite");
    if (h) tx.objectStore(STORE).put(h, "export");
    else tx.objectStore(STORE).delete("export");
  } catch {
    // Хранилище недоступно — папку придётся выбрать снова.
  }
}

async function loadHandle(): Promise<FileSystemDirectoryHandle | null> {
  try {
    const d = await db();
    return await new Promise((resolve) => {
      const req = d.transaction(STORE).objectStore(STORE).get("export");
      req.onsuccess = () => resolve((req.result as FileSystemDirectoryHandle) ?? null);
      req.onerror = () => resolve(null);
    });
  } catch {
    return null;
  }
}

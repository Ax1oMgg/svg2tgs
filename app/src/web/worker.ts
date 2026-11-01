/**
 * Фоновый поток веб-версии: ядро svg2tgs в WebAssembly (`web/` в корне
 * репозитория). Окно не подвисает, пока конвертируются большие наборы.
 *
 * Обмен с модулем — через его память (см. `web/src/lib.rs`): команда JSON
 * и данные кладутся в буферы `alloc`, `call` возвращает вид ответа, сам ответ
 * читается по `reply_ptr` / `reply_len`. Сеть модулю недоступна: у него
 * нет ни одного импорта.
 */

interface Exports {
  memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  call(cmd: number, cmdLen: number, data: number, dataLen: number): number;
  reply_ptr(): number;
  reply_len(): number;
}

/** Запрос из окна: номер, команда и данные. */
export interface Request {
  seq: number;
  cmd: Record<string, unknown>;
  data?: ArrayBuffer;
}

/** Ответ: вид (0 — JSON, 1 — байты, 2 — ошибка) и байты; `crash` — модуль упал. */
export interface Response {
  seq: number;
  code?: number;
  bytes?: ArrayBuffer;
  crash?: string;
}

const encoder = new TextEncoder();
let wasmUrl = "";
let module: Promise<Exports> | null = null;

async function load(): Promise<Exports> {
  const response = await fetch(wasmUrl);
  const { instance } = WebAssembly.instantiateStreaming
    ? await WebAssembly.instantiateStreaming(response, {})
    : await WebAssembly.instantiate(await response.arrayBuffer(), {});
  const w = instance.exports as unknown as Exports;
  run(w, { cmd: "init" }, new Uint8Array());
  return w;
}

function put(w: Exports, bytes: Uint8Array): [number, number] {
  const ptr = w.alloc(bytes.length);
  new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
}

function run(w: Exports, cmd: Record<string, unknown>, data: Uint8Array): { code: number; bytes: Uint8Array } {
  const [cp, cl] = put(w, encoder.encode(JSON.stringify(cmd)));
  const [dp, dl] = put(w, data);
  const code = w.call(cp, cl, dp, dl);
  w.dealloc(cp, cl);
  w.dealloc(dp, dl);
  const bytes = new Uint8Array(w.memory.buffer, w.reply_ptr(), w.reply_len()).slice();
  return { code, bytes };
}

self.onmessage = async (e: MessageEvent<Request | { wasmUrl: string }>) => {
  if ("wasmUrl" in e.data) {
    wasmUrl = e.data.wasmUrl;
    return;
  }
  const { seq, cmd, data } = e.data;
  try {
    module ??= load();
    const w = await module;
    const { code, bytes } = run(w, cmd, new Uint8Array(data ?? new ArrayBuffer(0)));
    (self as unknown as Worker).postMessage({ seq, code, bytes: bytes.buffer as ArrayBuffer } satisfies Response, [bytes.buffer as ArrayBuffer]);
  } catch (err) {
    // Модуль упал (паника Rust) — его память больше не годится: следующая
    // команда загрузит его заново, результаты файлов придётся пересчитать.
    module = null;
    (self as unknown as Worker).postMessage({ seq, crash: String(err) } satisfies Response);
  }
};

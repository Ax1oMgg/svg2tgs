/**
 * Мост к Rust-части приложения.
 *
 * Внутри Tauri вызовы идут в команды из `src-tauri`. В обычном браузере
 * (`npm run dev` без Tauri) подставляются тестовые данные — так интерфейс
 * можно открыть отдельно, например для захвата экранов в Figma.
 */

import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  coreVersion: string;
  /** "tauri" — настоящее приложение, "browser" — режим с тестовыми данными. */
  runtime: "tauri" | "browser";
}

export const isTauri = (): boolean => "__TAURI_INTERNALS__" in window;

export async function getAppInfo(): Promise<AppInfo> {
  if (!isTauri()) {
    return { version: "0.1.0", coreVersion: "0.1.0", runtime: "browser" };
  }
  const info = await invoke<Omit<AppInfo, "runtime">>("app_info");
  return { ...info, runtime: "tauri" };
}

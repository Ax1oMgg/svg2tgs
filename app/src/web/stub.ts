/**
 * Заглушка веб-версии для установленной программы и режима `npm run dev`:
 * псевдоним `@web` ведёт сюда во всех сборках, кроме `--mode web`
 * (`vite.config.ts`), поэтому ни модуль ядра, ни lottie-web в программу
 * не попадают. Вызывается только при `isWeb()`, то есть никогда.
 */

import type * as Client from "./client";

const off = (): never => {
  throw new Error("веб-версия не входит в эту сборку");
};

export const pickPaths: typeof Client.pickPaths = off;
export const expandPaths: typeof Client.expandPaths = off;
export const onFileDrop: typeof Client.onFileDrop = off;
export const coreVersion: typeof Client.coreVersion = off;
export const convert: typeof Client.convert = off;
export const forget: typeof Client.forget = off;
export const sourceSize: typeof Client.sourceSize = off;
export const parseOptions: typeof Client.parseOptions = off;
export const optionsJson: typeof Client.optionsJson = off;
export const preview: typeof Client.preview = off;
export const canPickFolder: typeof Client.canPickFolder = () => false;
export const pickFolder: typeof Client.pickFolder = off;
export const restoreFolder: typeof Client.restoreFolder = off;
export const clearFolder: typeof Client.clearFolder = off;
export const prepareExport: typeof Client.prepareExport = off;
export const findExisting: typeof Client.findExisting = off;
export const save: typeof Client.save = off;

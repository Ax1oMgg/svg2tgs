// Сборка ядра для веб-версии: крейт `web` → WebAssembly → src/web/wasm/.
// Нужна цель Rust wasm32-unknown-unknown: `rustup target add wasm32-unknown-unknown`.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, statSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dirname, "../..");
const target = "wasm32-unknown-unknown";
execFileSync("cargo", ["build", "-p", "svg2tgs-web", "--target", target, "--profile", "web", "--locked"], {
  cwd: root,
  stdio: "inherit",
});
const src = join(root, "target", target, "web", "svg2tgs_web.wasm");
const dir = join(import.meta.dirname, "../src/web/wasm");
mkdirSync(dir, { recursive: true });
copyFileSync(src, join(dir, "svg2tgs_web.wasm"));
console.log(`Модуль ядра: ${(statSync(src).size / 1024 / 1024).toFixed(1)} МБ`);

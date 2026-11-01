import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

/**
 * Политика содержимого веб-версии: только свои файлы сайта, модуль ядра
 * (`wasm-unsafe-eval` — компиляция WebAssembly, не `eval`), фоновый поток
 * и картинки превью. Ни счётчиков, ни сторонних адресов.
 */
const WEB_CSP = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self'",
  "connect-src 'self'",
  "img-src 'self' data: blob:",
  "style-src 'self' 'unsafe-inline'",
  "font-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

function webCsp(): Plugin {
  return {
    name: "svg2tgs-web-csp",
    // В режиме разработки Vite встраивает свои скрипты — политика только в сборке.
    apply: "build",
    transformIndexHtml: (html) =>
      html.replace(
        '<meta charset="UTF-8" />',
        `<meta charset="UTF-8" />\n    <meta http-equiv="Content-Security-Policy" content="${WEB_CSP}" />`,
      ),
  };
}

// Обычная сборка рассчитана на Tauri: фиксированный порт для `tauri dev`
// и сборка под системные WebView (WebView2, WebKit). `--mode web` —
// веб-версия для GitHub Pages (docs/BUILD.md, «Веб-версия»).
export default defineConfig(({ mode }) => {
  const web = mode === "web";
  return {
    plugins: web ? [react(), webCsp()] : [react()],
    // Сайт лежит в подпапке: ax1omgg.github.io/svg2tgs/.
    base: web ? "/svg2tgs/" : "/",
    resolve: {
      alias: {
        // Веб-часть (ядро в WebAssembly, lottie-web) — только в веб-версии.
        // Путь от корня проекта (app/).
        "@web": web ? "/src/web/client.ts" : "/src/web/stub.ts",
      },
    },
    clearScreen: false,
    server: {
      // Явный IPv4: на Windows `localhost` у Vite может оказаться ::1,
      // а `tauri dev` ждёт 127.0.0.1 и висит на «Waiting for your frontend».
      host: "127.0.0.1",
      port: 1420,
      strictPort: !web,
      watch: { ignored: ["**/src-tauri/**"] },
    },
    envPrefix: ["VITE_", "TAURI_ENV_"],
    worker: { format: "es" as const },
    build: {
      target: "es2021",
      outDir: web ? "dist-web" : "dist",
      emptyOutDir: true,
      // Веб-версия: lottie-web входит в основной файл.
      chunkSizeWarningLimit: web ? 800 : 500,
    },
  };
});

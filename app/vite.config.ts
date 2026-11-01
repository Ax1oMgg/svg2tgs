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

/** Адрес сайта: превью ссылки требует полных адресов. */
const SITE = "https://ax1omgg.github.io/svg2tgs/";

/**
 * Веб-версия: иконка вкладки и превью ссылки (Open Graph — Telegram, VK,
 * Discord). Иконки — те же исходники, что у программы: Vite кладёт их
 * в сборку сам. Картинка превью — `web-public/og.png` (макет, страница
 * «Промо и прочее»): ей нужен постоянный адрес, поэтому она без хеша.
 */
function webHead(): Plugin {
  const title = "svg2tgs — веб-версия";
  const description =
    "SVG → стикеры и эмодзи Telegram прямо в браузере. Без лимитов, ничего не нужно устанавливать, файлы не покидают ваш компьютер.";
  const tags = [
    `<meta name="description" content="${description}" />`,
    `<link rel="icon" type="image/svg+xml" href="./app-icon-small.svg" />`,
    `<link rel="icon" type="image/png" sizes="32x32" href="./src-tauri/icons/32x32.png" />`,
    `<link rel="apple-touch-icon" href="./src-tauri/icons/128x128@2x.png" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:url" content="${SITE}" />`,
    `<meta property="og:site_name" content="svg2tgs" />`,
    `<meta property="og:title" content="${title}" />`,
    `<meta property="og:description" content="${description}" />`,
    `<meta property="og:locale" content="ru_RU" />`,
    `<meta property="og:image" content="${SITE}og.png" />`,
    `<meta property="og:image:width" content="1200" />`,
    `<meta property="og:image:height" content="630" />`,
    `<meta property="og:image:alt" content="svg2tgs — веб-версия: SVG в стикеры и эмодзи Telegram в браузере" />`,
    `<meta name="twitter:card" content="summary_large_image" />`,
  ];
  return {
    name: "svg2tgs-web-head",
    apply: "build",
    // До обработки HTML самим Vite: иначе он не увидит иконки как файлы сборки.
    transformIndexHtml: {
      order: "pre",
      handler: (html) => html.replace("  </head>", `    ${tags.join("\n    ")}\n  </head>`),
    },
  };
}

// Обычная сборка рассчитана на Tauri: фиксированный порт для `tauri dev`
// и сборка под системные WebView (WebView2, WebKit). `--mode web` —
// веб-версия для GitHub Pages (docs/BUILD.md, «Веб-версия»).
export default defineConfig(({ mode }) => {
  const web = mode === "web";
  return {
    plugins: web ? [react(), webCsp(), webHead()] : [react()],
    // Файлы веб-версии с постоянными адресами (картинка превью ссылки).
    publicDir: web ? "web-public" : "public",
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

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Настройки рассчитаны на Tauri: фиксированный порт для `tauri dev`
// и сборка под системные WebView (WebView2, WebKit).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2021",
    outDir: "dist",
    emptyOutDir: true,
  },
});

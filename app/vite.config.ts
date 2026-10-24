import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Настройки рассчитаны на Tauri: фиксированный порт для `tauri dev`
// и сборка под системные WebView (WebView2, WebKit).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    // Явный IPv4: на Windows `localhost` у Vite может оказаться ::1,
    // а `tauri dev` ждёт 127.0.0.1 и висит на «Waiting for your frontend».
    host: "127.0.0.1",
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

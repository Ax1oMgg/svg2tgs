import { useEffect, useState } from "react";
import { getAppInfo, type AppInfo } from "./backend";
import "./App.css";

/**
 * Этап 0: стартовый экран-заглушка, проверяющий связку интерфейса и Rust.
 * Настоящий интерфейс (список файлов, превью, настройки) — этап 5.
 */
export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    getAppInfo().then(setInfo).catch(() => setInfo(null));
  }, []);

  return (
    <main className="app">
      <section className="dropzone" aria-label="Область для файлов">
        <div className="dropzone__icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="40" height="40" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 16V4" />
            <path d="m7 9 5-5 5 5" />
            <path d="M4 16v3a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-3" />
          </svg>
        </div>
        <h1 className="dropzone__title">Перетащите SVG сюда</h1>
        <p className="dropzone__hint">Файлы или папки — конвертация в TGS для Telegram</p>
      </section>
      <footer className="statusbar">
        {info
          ? `svg2tgs ${info.version} · ядро ${info.coreVersion} · ${info.runtime === "tauri" ? "приложение" : "браузерный режим"}`
          : "svg2tgs"}
      </footer>
    </main>
  );
}

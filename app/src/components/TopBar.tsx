import type { Lang } from "../i18n";
import { strings } from "../i18n";

interface Props {
  lang: Lang;
  readyCount: number;
  onAddFiles: () => void;
  onAddFolder: () => void;
  onSave: () => void;
}

/** Верхняя панель: добавление файлов и главное действие — сохранить. */
export function TopBar({ lang, readyCount, onAddFiles, onAddFolder, onSave }: Props) {
  const t = strings[lang];
  return (
    <header className="topbar">
      <div className="topbar__brand">
        <Logo />
        <span className="topbar__name">svg2tgs</span>
      </div>
      <div className="topbar__actions">
        <button type="button" className="btn btn--quiet" onClick={onAddFiles}>
          {t.addFiles}
        </button>
        <button type="button" className="btn btn--quiet" onClick={onAddFolder}>
          {t.addFolder}
        </button>
        <button
          type="button"
          className="btn btn--primary"
          onClick={onSave}
          disabled={readyCount === 0}
        >
          {readyCount > 0 ? t.saveAll(readyCount) : t.save}
        </button>
      </div>
    </header>
  );
}

function Logo() {
  return (
    <svg className="topbar__logo" viewBox="0 0 32 32" aria-hidden="true">
      <rect x="2" y="2" width="28" height="28" rx="8" fill="var(--color-accent)" />
      <path
        d="M10 9h9l4 4v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 9 22V10.5A1.5 1.5 0 0 1 10.5 9z"
        fill="var(--color-on-accent)"
      />
      <path d="M19 9v3a1 1 0 0 0 1 1h3z" fill="var(--color-accent-soft)" />
    </svg>
  );
}

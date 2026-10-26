import type { Lang } from "../i18n";
import { strings } from "../i18n";

interface Props {
  lang: Lang;
  recursive: boolean;
  onRecursive: (v: boolean) => void;
  onAddFiles: () => void;
  onAddFolder: () => void;
}

/** Пустой экран: приглашение перетащить SVG. */
export function EmptyState({ lang, recursive, onRecursive, onAddFiles, onAddFolder }: Props) {
  const t = strings[lang];
  return (
    <section className="empty">
      <div className="empty__sticker" aria-hidden="true">
        <svg viewBox="0 0 120 120">
          <path
            className="empty__peel"
            d="M24 18h52l26 26v52a8 8 0 0 1-8 8H24a8 8 0 0 1-8-8V26a8 8 0 0 1 8-8z"
          />
          <path className="empty__corner" d="M76 18v18a8 8 0 0 0 8 8h18z" />
          <path className="empty__arrow" d="M59 82V50m-14 14 14-14 14 14" />
        </svg>
      </div>
      <h1 className="empty__title">{t.dropTitle}</h1>
      <p className="empty__hint">{t.dropHint}</p>
      <div className="empty__actions">
        <button type="button" className="btn btn--primary" onClick={onAddFiles}>
          {t.addFiles}
        </button>
        <button type="button" className="btn btn--default" onClick={onAddFolder}>
          {t.addFolder}
        </button>
      </div>
      <label className="check">
        <input type="checkbox" checked={recursive} onChange={(e) => onRecursive(e.target.checked)} />
        {t.includeSubfolders}
      </label>
    </section>
  );
}

import type { Lang } from "../i18n";
import { errorText, strings } from "../i18n";
import type { FileItem } from "../types";
import { FileName } from "./FileName";

interface Props {
  lang: Lang;
  items: FileItem[];
  selectedId: string | null;
  onlyProblems: boolean;
  onToggleProblems: (v: boolean) => void;
  onSelect: (id: string) => void;
  onRemove: (id: string) => void;
  onClear: () => void;
}

const LIMIT = 64 * 1024;

export function formatSize(bytes: number, lang: Lang): string {
  const [b, k] = lang === "ru" ? ["Б", "КБ"] : ["B", "KB"];
  return bytes < 1024 ? `${bytes} ${b}` : `${(bytes / 1024).toFixed(1)} ${k}`;
}

/** Список добавленных файлов со статусом и размером результата. */
export function FileList(props: Props) {
  const { lang, items, selectedId, onlyProblems } = props;
  const t = strings[lang];
  const shown = onlyProblems ? items.filter((it) => it.status === "warn" || it.status === "error") : items;
  const count = (s: FileItem["status"]) => items.filter((it) => it.status === s).length;

  return (
    <aside className="files" aria-label={t.files}>
      <div className="files__head">
        <h2 className="panel-title">
          {t.files} <span className="files__count">{items.length}</span>
        </h2>
        <button type="button" className="btn btn--link" onClick={props.onClear}>
          {t.clear}
        </button>
      </div>
      <label className="check files__filter">
        <input
          type="checkbox"
          checked={onlyProblems}
          onChange={(e) => props.onToggleProblems(e.target.checked)}
        />
        {t.onlyProblems}
      </label>

      <ul className="files__list" role="listbox" aria-label={t.files}>
        {shown.map((it) => (
          <li
            key={it.id}
            role="option"
            aria-selected={it.id === selectedId}
            tabIndex={0}
            className={`file file--${it.status}${it.id === selectedId ? " file--selected" : ""}`}
            onClick={() => props.onSelect(it.id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") props.onSelect(it.id);
              if (e.key === "Delete") props.onRemove(it.id);
            }}
          >
            <span className="file__thumb">
              {it.thumb ? <img src={it.thumb} alt="" /> : <span className="file__thumb-empty" />}
            </span>
            <span className="file__body">
              <FileName name={it.name} className="file__name" />
              <span className="file__meta">
                {it.status === "pending" && t.converting}
                {it.status === "error" && it.outcome?.error && errorText(it.outcome.error, lang)}
                {(it.status === "ok" || it.status === "warn") && it.outcome?.tgsBytes != null && (
                  <span className={it.outcome.tgsBytes > LIMIT ? "file__size--over" : undefined}>
                    {formatSize(it.outcome.tgsBytes, lang)}
                  </span>
                )}
              </span>
            </span>
            <StatusMark status={it.status} />
            <button
              type="button"
              className="file__remove"
              aria-label={t.remove}
              title={t.remove}
              onClick={(e) => {
                e.stopPropagation();
                props.onRemove(it.id);
              }}
            >
              ×
            </button>
          </li>
        ))}
      </ul>

      <p className="files__summary">{t.summary(count("ok"), count("warn"), count("error"))}</p>
    </aside>
  );
}

function StatusMark({ status }: { status: FileItem["status"] }) {
  if (status === "pending") return <span className="status status--pending" aria-hidden="true" />;
  const glyph = status === "ok" ? "✓" : status === "warn" ? "!" : "×";
  return (
    <span className={`status status--${status}`} aria-hidden="true">
      {glyph}
    </span>
  );
}

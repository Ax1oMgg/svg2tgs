import type { Lang } from "../i18n";
import { errorText, strings } from "../i18n";
import type { FileItem } from "../types";

interface Props {
  lang: Lang;
  items: FileItem[];
  selectedId: string | null;
  onlyProblems: boolean;
  onToggleProblems: (v: boolean) => void;
  onSelect: (id: string) => void;
  onRemove: (id: string) => void;
  onClear: () => void;
  onAddFiles: () => void;
  onAddFolder: () => void;
}

const LIMIT = 64 * 1024;

export function formatSize(bytes: number, lang: Lang): string {
  const [b, k] = lang === "ru" ? ["Б", "КБ"] : ["B", "KB"];
  return bytes < 1024 ? `${bytes} ${b}` : `${(bytes / 1024).toFixed(1)} ${k}`;
}

/** Сколько последних символов имени видно всегда. */
const TAIL = 16;
/** Насколько левее можно сдвинуть начало хвоста к разделителю. */
const SNAP = 4;

/**
 * Имя делится на голову (обрезается многоточием) и хвост (виден всегда):
 * так в длинных именах видны номер и расширение. Если рядом левее есть
 * пробел, `_` или `-`, хвост начинается с него.
 */
export function splitName(name: string): [string, string] {
  let cut = Math.max(0, name.length - TAIL);
  for (let i = cut; i > 0 && i >= cut - SNAP; i--) {
    if (" _-".includes(name[i])) {
      cut = i;
      break;
    }
  }
  return [name.slice(0, cut), name.slice(cut)];
}

/** Список добавленных файлов со статусом и размером результата. */
export function FileList(props: Props) {
  const { lang, items, selectedId, onlyProblems } = props;
  const t = strings[lang];
  const shown = onlyProblems ? items.filter((it) => it.status === "warn" || it.status === "error") : items;
  const count = (s: FileItem["status"]) => items.filter((it) => it.status === s).length;
  const pending = count("pending");
  const percent = items.length ? Math.round(((items.length - pending) / items.length) * 100) : 0;

  return (
    <aside className="files" aria-label={t.files}>
      <div className="files__head">
        <h2 className="panel-title">
          {t.files} <span className="files__count">{items.length}</span>
        </h2>
        <div className="files__actions">
          <div className="add-group" role="group" aria-label={t.add}>
            <button type="button" className="btn btn--default" title={t.addFiles} onClick={props.onAddFiles}>
              {t.add}
            </button>
            <button
              type="button"
              className="btn btn--default btn--icon"
              aria-label={t.addFolder}
              title={t.addFolder}
              onClick={props.onAddFolder}
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2.5h7.5A2.5 2.5 0 0 1 21 10v7.5a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z" />
              </svg>
            </button>
          </div>
          <button type="button" className="btn btn--text" title={t.clearHint} onClick={props.onClear}>
            {t.clear}
          </button>
        </div>
      </div>
      <label className="check files__filter">
        <input
          type="checkbox"
          checked={onlyProblems}
          onChange={(e) => props.onToggleProblems(e.target.checked)}
        />
        {t.onlyProblems}
      </label>

      <ul className="files__list scroll" role="listbox" aria-label={t.files}>
        {shown.map((it) => {
          const [head, tail] = splitName(it.name);
          return (
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
                <span className="file__name" title={it.name}>
                  <span className="file__name-head">{head}</span>
                  <span className="file__name-tail">{tail}</span>
                </span>
                <span className="file__meta">
                  {it.status === "pending" && t.converting}
                  {it.status === "error" && it.outcome?.error && errorText(it.outcome.error, lang)}
                  {(it.status === "ok" || it.status === "warn") && it.outcome?.tgsBytes != null && (
                    <span className={it.outcome.tgsBytes > LIMIT ? "file__size--over" : undefined}>
                      {formatSize(it.outcome.tgsBytes, lang)}
                    </span>
                  )}
                  {it.options && <span className="file__own"> · {t.ownSettings}</span>}
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
          );
        })}
      </ul>

      {pending > 0 ? (
        <div
          className="files__progress"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
          aria-label={t.conversion}
        >
          <div className="progress">
            <div className="progress__fill" style={{ width: `${percent}%` }} />
          </div>
          <span className="files__percent">{percent}%</span>
          <span className="count count--ok" title={t.countOk}>
            {count("ok")}
          </span>
          <span className="count count--warn" title={t.countWarn}>
            {count("warn")}
          </span>
          <span className="count count--error" title={t.countError}>
            {count("error")}
          </span>
        </div>
      ) : (
        <p className="files__summary">{t.summary(count("ok"), count("warn"), count("error"))}</p>
      )}
    </aside>
  );
}

function StatusMark({ status }: { status: FileItem["status"] }) {
  if (status === "pending") return <span className="status status--pending" aria-hidden="true" />;
  return (
    <span className={`status status--${status}`} aria-hidden="true">
      {status === "ok" ? (
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path
            d="M2 5.2 4.2 7.4 8 3"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      ) : status === "warn" ? (
        "!"
      ) : (
        "×"
      )}
    </span>
  );
}

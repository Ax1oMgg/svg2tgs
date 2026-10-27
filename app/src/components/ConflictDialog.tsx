import { useEffect, useRef, useState } from "react";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { Existing } from "../types";

export type ConflictChoice =
  | { kind: "save"; name: string }
  | { kind: "overwrite" }
  | { kind: "skip" }
  | { kind: "cancel" };

interface Props {
  lang: Lang;
  existing: Existing;
  /** Сколько ещё совпадений после этого. */
  rest: number;
  onChoose: (choice: ConflictChoice, applyAll: boolean) => void;
}

/**
 * Файл с таким именем уже есть. Спрашивается по каждому совпадению по очереди;
 * «Применить ко всем» — тот же выбор для остальных (имена — с номером).
 * «Отмена», «×» и Esc — экспорт отменяется, ничего не записывается.
 */
export function ConflictDialog({ lang, existing, rest, onChoose }: Props) {
  const t = strings[lang];
  const ref = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(existing.suggested);
  const [all, setAll] = useState(false);

  useEffect(() => {
    if (!ref.current?.open) ref.current?.showModal();
  }, []);
  useEffect(() => setName(existing.suggested), [existing]);

  const save = () => onChoose({ kind: "save", name: name.trim() || existing.suggested }, all);

  return (
    <dialog
      ref={ref}
      className="dialog dialog--conflict"
      aria-labelledby="conflict-title"
      onCancel={(e) => {
        e.preventDefault();
        onChoose({ kind: "cancel" }, false);
      }}
    >
      <div className="dialog__body">
        <div className="dialog__head">
          <svg className="dialog__warn" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M12 9v4M12 17h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" />
          </svg>
          <h2 id="conflict-title" className="dialog__title dialog__title--sm" title={existing.path}>
            {t.exists(existing.fileName)}
          </h2>
          <button
            type="button"
            className="btn btn--text btn--icon dialog__close"
            aria-label={t.cancelExport}
            title={t.cancelExport}
            onClick={() => onChoose({ kind: "cancel" }, false)}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
              <path d="M6 6l12 12M18 6 6 18" />
            </svg>
          </button>
        </div>

        <label className="conflict__name">
          <span>{t.saveAsNew}</span>
          <span className="conflict__input">
            <input
              value={name}
              disabled={all}
              aria-label={t.newName}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && save()}
            />
            <span className="conflict__ext">.tgs</span>
          </span>
        </label>

        <div className="conflict__actions">
          <button type="button" className="btn btn--primary" onClick={save}>
            {t.save}
          </button>
          <button type="button" className="btn btn--default" onClick={() => onChoose({ kind: "overwrite" }, all)}>
            {t.overwrite}
          </button>
          <button type="button" className="btn btn--default" onClick={() => onChoose({ kind: "skip" }, all)}>
            {t.skip}
          </button>
          <button type="button" className="btn btn--text" onClick={() => onChoose({ kind: "cancel" }, false)}>
            {t.cancel}
          </button>
        </div>

        {rest > 0 && (
          <label className="check conflict__all">
            <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
            {t.applyAll(rest)}
          </label>
        )}
      </div>
    </dialog>
  );
}

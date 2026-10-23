import { useEffect, useRef, useState } from "react";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { Conflict } from "../types";

interface Props {
  lang: Lang;
  count: number;
  dir: string | null;
  template: string;
  conflict: Conflict;
  onChooseDir: () => void;
  onTemplate: (v: string) => void;
  onConflict: (c: Conflict) => void;
  onCancel: () => void;
  onSave: () => void;
}

/** Диалог сохранения: папка, имя файла, что делать с существующими. */
export function SaveDialog(p: Props) {
  const t = strings[p.lang];
  const ref = useRef<HTMLDialogElement>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  return (
    <dialog ref={ref} className="dialog" onCancel={p.onCancel} aria-labelledby="save-title">
      <h2 id="save-title" className="dialog__title">
        {t.saveTitle}
      </h2>

      <div className="field">
        <div className="field__label">{t.folder}</div>
        <div className="row">
          <output className="path" title={p.dir ?? ""}>
            {p.dir ?? t.chooseFolderFirst}
          </output>
          <button type="button" className="btn btn--outline" onClick={p.onChooseDir}>
            {t.choose}
          </button>
        </div>
      </div>

      <div className="field">
        <div className="field__label">{t.nameTemplate}</div>
        <div className="row">
          <input value={p.template} onChange={(e) => p.onTemplate(e.target.value)} aria-label={t.nameTemplate} />
          <span className="suffix">.tgs</span>
        </div>
        <p className="hint">{t.nameTemplateHint}</p>
      </div>

      <fieldset className="field">
        <legend className="field__label">{t.ifExists}</legend>
        {(["number", "overwrite", "skip"] as const).map((c) => (
          <label key={c} className="radio">
            <input type="radio" name="conflict" checked={p.conflict === c} onChange={() => p.onConflict(c)} />
            {c === "number" ? t.number : c === "overwrite" ? t.overwrite : t.skip}
          </label>
        ))}
      </fieldset>

      <div className="dialog__actions">
        <button type="button" className="btn btn--quiet" onClick={p.onCancel}>
          {t.cancel}
        </button>
        <button
          type="button"
          className="btn btn--primary"
          disabled={!p.dir || busy}
          onClick={() => {
            setBusy(true);
            p.onSave();
          }}
        >
          {t.saveAll(p.count)}
        </button>
      </div>
    </dialog>
  );
}

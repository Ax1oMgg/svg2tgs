import { useEffect, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { OptionsError, ParsedOptions } from "../types";

interface Props {
  lang: Lang;
  /** К чему применится: «общим настройкам», «настройкам heart.svg»… */
  target: string;
  /** Текст в поле при открытии (например, прочитанный из файла). */
  initial?: string;
  onApply: (parsed: ParsedOptions) => void;
  onClose: () => void;
}

/** Ошибка или итог разбора — текстом для людей. */
export function optionsErrorText(e: OptionsError, lang: Lang): string {
  const t = strings[lang];
  switch (e.code) {
    case "syntax":
      return t.importErr_syntax(e.line, e.column);
    case "value":
      return t.importErr_value(e.line, e.column);
    case "not_object":
      return t.importErr_not_object;
    case "newer_format":
      return t.importErr_newer_format;
    default:
      return t.importErr_too_large;
  }
}

const isParsed = (r: ParsedOptions | OptionsError): r is ParsedOptions => "options" in r;

/**
 * Вставить настройки текстом. Текст разбирает ядро прямо при вводе:
 * под полем — сколько настроек узнано или где ошибка; «Применить»
 * активна, только когда текст прочитан.
 */
export function ImportDialog({ lang, target, initial, onApply, onClose }: Props) {
  const t = strings[lang];
  const ref = useRef<HTMLDialogElement>(null);
  const [text, setText] = useState(initial ?? "");
  const [result, setResult] = useState<ParsedOptions | OptionsError | null>(null);

  useEffect(() => {
    if (!ref.current?.open) ref.current?.showModal();
  }, []);

  useEffect(() => {
    if (!text.trim()) {
      setResult(null);
      return;
    }
    let stale = false;
    const timer = setTimeout(() => {
      void backend.parseOptions(text).then((r) => !stale && setResult(r));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [text]);

  const parsed = result && isParsed(result) ? result : null;

  return (
    <dialog
      ref={ref}
      className="dialog dialog--import"
      aria-labelledby="import-title"
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div className="dialog__body">
        <div className="dialog__head">
          <h2 id="import-title" className="dialog__title">
            {t.importTitle}
          </h2>
          <button
            type="button"
            className="btn btn--text btn--icon dialog__close"
            aria-label={t.closeUnchanged}
            title={t.closeUnchanged}
            onClick={onClose}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
              <path d="M6 6l12 12M18 6 6 18" />
            </svg>
          </button>
        </div>
        <p className="hint">{t.importHelp(target)}</p>
        <textarea
          className={`import__text${result && !parsed ? " import__text--bad" : ""}`}
          aria-label={t.importField}
          spellCheck={false}
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        {result && !parsed && <p className="note note--error">{optionsErrorText(result as OptionsError, lang)}</p>}
        {parsed && <p className="import__ok">{t.importOk(parsed.known)}</p>}
        <div className="dialog__actions">
          <button type="button" className="btn btn--text" onClick={onClose}>
            {t.cancel}
          </button>
          <button type="button" className="btn btn--primary" disabled={!parsed} onClick={() => parsed && onApply(parsed)}>
            {t.apply}
          </button>
        </div>
      </div>
    </dialog>
  );
}

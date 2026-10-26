import { useEffect, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { Theme } from "../prefs";

interface Props {
  lang: Lang;
  theme: Theme;
  version: string;
  onApply: (lang: Lang, theme: Theme) => void;
  onClose: () => void;
}

/**
 * Настройки программы (шестерёнка): язык, тема, «О программе».
 * Выбор применяется только кнопкой «Готово» — она активна, когда что-то
 * изменено. «×», Esc и щелчок мимо окна закрывают его без изменений.
 */
export function SettingsDialog({ lang, theme, version, onApply, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [draftLang, setDraftLang] = useState(lang);
  const [draftTheme, setDraftTheme] = useState(theme);
  const changed = draftLang !== lang || draftTheme !== theme;
  // Подписи — на языке, который сейчас у программы, пока выбор не применён.
  const t = strings[lang];

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby="settings-title"
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      // Щелчок по подложке приходит в сам <dialog>; содержимое — в .dialog__body,
      // поэтому щелчок по полям окна его не закрывает.
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="dialog__body">
      <div className="dialog__head">
        <h2 id="settings-title" className="dialog__title">
          {t.settings}
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

      <div className="field">
        <label className="field__label" htmlFor="settings-lang">
          {t.language}
        </label>
        <select id="settings-lang" value={draftLang} onChange={(e) => setDraftLang(e.target.value as Lang)}>
          <option value="ru">Русский</option>
          <option value="en">English</option>
        </select>
      </div>

      <div className="field">
        <label className="field__label" htmlFor="settings-theme">
          {t.theme}
        </label>
        <select id="settings-theme" value={draftTheme} onChange={(e) => setDraftTheme(e.target.value as Theme)}>
          <option value="system">{t.themeSystem}</option>
          <option value="light">{t.themeLight}</option>
          <option value="dark">{t.themeDark}</option>
        </select>
      </div>

      <section className="about" aria-label={t.about}>
        <h3>{t.about}</h3>
        <p>
          svg2tgs{version && ` · ${t.version(version)}`}
          <br />
          {t.aboutProject}
          <br />
          {t.aboutAuthor}
          <br />
          {t.aboutLicense}
        </p>
        <button type="button" className="btn btn--link" onClick={() => void backend.openProjectPage()}>
          {t.projectPage}
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
          </svg>
        </button>
      </section>

      <div className="dialog__actions">
        <button type="button" className="btn btn--primary" disabled={!changed} onClick={() => onApply(draftLang, draftTheme)}>
          {t.done}
        </button>
      </div>
      </div>
    </dialog>
  );
}

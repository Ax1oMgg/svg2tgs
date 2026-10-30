import { useEffect, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import { Dropdown } from "./Dropdown";
import { SegButton } from "./Preview";
import type { Theme } from "../prefs";
import appIcon from "../../app-icon.svg";

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
        <div className="field__label">{t.language}</div>
        <Dropdown
          label={t.language}
          value={draftLang}
          options={[
            { value: "ru", label: "Русский" },
            { value: "en", label: "English" },
          ]}
          onChange={setDraftLang}
        />
      </div>

      <div className="field">
        <div className="field__label">{t.theme}</div>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.theme}>
          {(["system", "light", "dark"] as const).map((v) => (
            <SegButton key={v} active={draftTheme === v} onClick={() => setDraftTheme(v)}>
              {v === "system" ? t.themeSystem : v === "light" ? t.themeLight : t.themeDark}
            </SegButton>
          ))}
        </div>
      </div>

      <section className="about" aria-label={t.about}>
        <h3>{t.about}</h3>
        {/* Шапка: иконка слева, название с версией и канал справа. */}
        <div className="about__head">
          <img className="about__icon" src={appIcon} alt="" width={52} height={52} />
          <div className="about__title">
            <span className="about__name">
              svg2tgs <span className="about__version">v{version || "—"}</span>
            </span>
            <span>
              {t.aboutChannel} <ExtLink link="telegram">{t.aboutChannelLink}</ExtLink>
            </span>
          </div>
        </div>
        <p>
          {t.aboutAuthorFull}
          <br />
          {t.aboutLicenseShort}
          <br />
          {t.aboutProjectPage} <ExtLink link="project">GitHub</ExtLink>
          <br />
          {t.aboutIssues} <ExtLink link="issues">GitHub</ExtLink>
          <br />
          {t.aboutEmail} <ExtLink link="email">chajnayaaxioma@gmail.com</ExtLink>
        </p>
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

/** Ссылка текстом; внешние — со значком ↗, почта — без. Открывается вне программы. */
function ExtLink({ link, children }: { link: backend.Link; children: React.ReactNode }) {
  return (
    <a
      className="text-link"
      href={backend.LINKS[link]}
      onClick={(e) => {
        e.preventDefault();
        void backend.openLink(link);
      }}
    >
      {children}
      {link !== "email" && (
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
        </svg>
      )}
    </a>
  );
}

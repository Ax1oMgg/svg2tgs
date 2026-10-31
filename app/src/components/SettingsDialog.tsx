import { useEffect, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { LANGS, strings, tgErrorText } from "../i18n";
import { Dropdown } from "./Dropdown";
import { SegButton } from "./Preview";
import type { Theme } from "../prefs";
import type { TgError, TgStatus } from "../types";
import appIcon from "../../app-icon.svg";

interface Props {
  lang: Lang;
  theme: Theme;
  version: string;
  /** Выгрузка в Telegram: меняется сразу, без «Готово». */
  tg: TgStatus;
  onTg: (s: TgStatus) => void;
  onApply: (lang: Lang, theme: Theme) => void;
  onClose: () => void;
}

/**
 * Настройки программы (шестерёнка): язык, тема, «О программе».
 * Выбор применяется только кнопкой «Готово» — она активна, когда что-то
 * изменено. «×», Esc и щелчок мимо окна закрывают его без изменений.
 */
export function SettingsDialog({ lang, theme, version, tg, onTg, onApply, onClose }: Props) {
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
          options={LANGS.map((l) => ({ value: l.code, label: l.name }))}
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

      <TelegramSection lang={lang} status={tg} onStatus={onTg} />

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

/** Как часто спрашивать Telegram, написали ли боту /start. */
const OWNER_POLL_MS = 3000;

/**
 * «Выгрузка в Telegram»: переключатель (по умолчанию выключен — тогда
 * программа в сеть не ходит), затем токен → «Проверить» → /start боту →
 * подключено. Действует сразу, кнопка «Готово» к нему не относится.
 */
function TelegramSection({ lang, status, onStatus }: { lang: Lang; status: TgStatus; onStatus: (s: TgStatus) => void }) {
  const t = strings[lang];
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<TgError | null>(null);
  const waiting = status.enabled && !!status.bot && !status.owner;

  // Ждём /start: спрашиваем Telegram, пока окно открыто и владельца нет.
  useEffect(() => {
    if (!waiting) return;
    let alive = true;
    const tick = () =>
      backend
        .tgFindOwner()
        .then((s) => alive && onStatus(s))
        .catch((e: TgError) => alive && setError(e));
    void tick();
    const timer = setInterval(tick, OWNER_POLL_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
    // onStatus меняется на каждой отрисовке — опрос от него не зависит.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [waiting]);

  const check = async () => {
    setBusy(true);
    setError(null);
    try {
      onStatus(await backend.tgCheckToken(token));
      setToken("");
    } catch (e) {
      setError(e as TgError);
    } finally {
      setBusy(false);
    }
  };
  const disconnect = async () => {
    setError(null);
    onStatus(await backend.tgDisconnect());
  };

  return (
    <section className="tg" aria-labelledby="tg-title">
      <div className="tg__head">
        <h3 id="tg-title">{t.tgSection}</h3>
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={status.enabled}
          aria-labelledby="tg-title"
          onClick={() => void backend.tgSetEnabled(!status.enabled).then(onStatus)}
        >
          <span className="switch__knob" />
        </button>
      </div>
      <p className="tg__lead">{status.enabled ? t.tgLeadOn : t.tgLeadOff}</p>

      {status.enabled && !status.bot && (
        <div className="tg__form">
          <label htmlFor="tg-token">{t.tgToken}</label>
          <div className="row">
            <input
              id="tg-token"
              className={`input input--mono${error ? " input--bad" : ""}`}
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder="123456789:AA…"
              value={token}
              onChange={(e) => {
                setToken(e.target.value);
                setError(null);
              }}
              onKeyDown={(e) => e.key === "Enter" && token.trim() && !busy && void check()}
            />
            <button type="button" className="btn btn--default" disabled={!token.trim() || busy} onClick={() => void check()}>
              {busy ? t.tgChecking : t.tgCheck}
            </button>
          </div>
          {error && <p className="note note--error">{tgErrorText(error, lang)}</p>}
          <span className="tg__help">
            {t.tgHelpBefore} <ExtLink link="botfather">@BotFather</ExtLink> {t.tgHelpAfter}
          </span>
        </div>
      )}

      {waiting && (
        <div className="tg__card tg__card--column">
          <span className="tg__strong">{t.tgBotFound(`@${status.bot}`)}</span>
          <span className="tg__wait">
            <span className="spinner" aria-hidden="true" />
            {t.tgWaitStart}
          </span>
          {error && <p className="note note--error">{tgErrorText(error, lang)}</p>}
          <div className="row">
            <button type="button" className="btn btn--default" onClick={() => void backend.tgOpen()}>
              {t.tgOpenBot}
              <ExtIcon />
            </button>
            <button type="button" className="btn btn--text" onClick={() => void disconnect()}>
              {t.tgOtherToken}
            </button>
          </div>
        </div>
      )}

      {status.enabled && status.bot && status.owner && (
        <div className="tg__card">
          <span className="tg__ok" aria-hidden="true">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round">
              <path d="M20 6 9 17l-5-5" />
            </svg>
          </span>
          <span className="tg__who">
            <span className="tg__strong">@{status.bot}</span>
            <br />
            {t.tgOwner("")}
            <span className="tg__strong">{status.owner}</span>
          </span>
          <button type="button" className="btn btn--text" title={t.tgDisconnectHint} onClick={() => void disconnect()}>
            {t.tgDisconnect}
          </button>
        </div>
      )}
    </section>
  );
}

function ExtIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
    </svg>
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

import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings } from "../i18n";

interface Props {
  lang: Lang;
  onClose: () => void;
}

/**
 * Плашка веб-версии при первом запуске: что работает иначе, чем
 * в установленной программе. Закрывается и больше не показывается.
 */
export function WebNotice({ lang, onClose }: Props) {
  const t = strings[lang];
  return (
    <aside className="web-notice" role="note" aria-labelledby="web-notice-title">
      <div className="web-notice__body">
        <h2 id="web-notice-title" className="web-notice__title">
          {t.webNoticeTitle}
        </h2>
        <p className="web-notice__text">{t.webNoticeText}</p>
        <ul className="web-notice__list">
          <li>{t.webNoticePreview}</li>
          <li>{t.webNoticeFont}</li>
          <li>{t.webNoticeSave}</li>
          <li>{t.webNoticeTelegram}</li>
        </ul>
      </div>
      <div className="web-notice__actions">
        <button type="button" className="btn btn--default" onClick={() => void backend.openLink("releases")}>
          {t.webNoticeInstall}
        </button>
        <button type="button" className="btn btn--primary" onClick={onClose}>
          {t.webNoticeOk}
        </button>
      </div>
    </aside>
  );
}

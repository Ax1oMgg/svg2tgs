import { useEffect, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { errorText, isInfo, strings, warningText } from "../i18n";
import type { PreviewBg, PreviewMode } from "../prefs";
import type { FileItem, Preset } from "../types";
import { formatSize } from "./FileList";

interface Props {
  lang: Lang;
  item: FileItem | null;
  preset: Preset;
  mode: PreviewMode;
  bg: PreviewBg;
  onMode: (m: PreviewMode) => void;
  onBg: (b: PreviewBg) => void;
}

const SIZE = 512;

/**
 * Сцена превью. «Как в чате» — стикер в диалоге Telegram (эмодзи — в строке
 * текста и крупным сообщением). «Крупно» — исходник и результат рядом.
 * Результат рисуется rlottie — тем же движком, что в Telegram.
 */
export function Preview({ lang, item, preset, mode, bg, onMode, onBg }: Props) {
  const t = strings[lang];
  const [images, setImages] = useState<{ svg: string | null; tgs: string | null }>({
    svg: null,
    tgs: null,
  });

  const key = item ? `${item.id}:${item.status}:${item.outcome?.tgsBytes}` : "";
  useEffect(() => {
    let cancelled = false;
    setImages({ svg: null, tgs: null });
    if (!item || item.status === "pending" || item.status === "error") return;
    void Promise.all([
      backend.preview(item.id, "svg", SIZE),
      backend.preview(item.id, "tgs", SIZE),
    ]).then(([svg, tgs]) => {
      if (!cancelled) setImages({ svg, tgs });
    });
    return () => {
      cancelled = true;
    };
    // Перерисовка — только когда меняется сам результат.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const chatBg = bg === "dark" ? "dark" : "light";

  return (
    <main className="stage">
      <div className="stage__toolbar">
        <div className="segmented" role="radiogroup" aria-label={t.inChat}>
          <SegButton active={mode === "chat"} onClick={() => onMode("chat")}>
            {t.inChat}
          </SegButton>
          <SegButton active={mode === "closeup"} onClick={() => onMode("closeup")}>
            {t.closeUp}
          </SegButton>
        </div>
        <div className="segmented" role="radiogroup" aria-label={t.background}>
          <SegButton active={chatBg === "light" && bg !== "checker"} onClick={() => onBg("light")}>
            {t.chatLight}
          </SegButton>
          <SegButton active={bg === "dark"} onClick={() => onBg("dark")}>
            {t.chatDark}
          </SegButton>
          {mode === "closeup" && (
            <SegButton active={bg === "checker"} onClick={() => onBg("checker")}>
              {t.checker}
            </SegButton>
          )}
        </div>
      </div>

      {item && (
        <div className="stage__name" title={item.name}>
          {item.name}
        </div>
      )}

      <div className="stage__canvas">
        {!item && <p className="stage__placeholder">{t.noPreview}</p>}
        {item && item.status === "error" && (
          <div className="stage__error">
            <strong>{t.fileFailed}</strong>
            <span>{item.outcome?.error ? errorText(item.outcome.error, lang) : ""}</span>
          </div>
        )}
        {item && item.status !== "error" && mode === "chat" && (
          <Chat lang={lang} preset={preset} theme={chatBg} image={images.tgs} />
        )}
        {item && item.status !== "error" && mode === "closeup" && (
          <div className="closeup">
            <figure className={`closeup__frame bg-${bg}`}>
              {images.svg ? <img src={images.svg} alt={t.original} /> : <Spinner />}
              <figcaption>{t.original}</figcaption>
            </figure>
            <figure className={`closeup__frame bg-${bg}`}>
              {images.tgs ? <img src={images.tgs} alt={t.result} /> : <Spinner />}
              <figcaption>
                {t.result}
                {item.outcome?.tgsBytes != null && ` · ${formatSize(item.outcome.tgsBytes, lang)}`}
              </figcaption>
            </figure>
          </div>
        )}
      </div>

      {item && item.outcome && !item.outcome.error && (
        <section className="notes" aria-live="polite">
          {item.outcome.warnings.length === 0 ? (
            <p className="notes__ok">{t.allGood}</p>
          ) : (
            <ul className="notes__list">
              {item.outcome.warnings.map((w, i) => (
                <li key={i} className={isInfo(w) ? "note note--info" : "note note--warn"}>
                  {warningText(w, lang)}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </main>
  );
}

/** Диалог Telegram: стикер без пузыря; эмодзи — в тексте и крупно. */
function Chat({
  lang,
  preset,
  theme,
  image,
}: {
  lang: Lang;
  preset: Preset;
  theme: "light" | "dark";
  image: string | null;
}) {
  const t = strings[lang];
  return (
    <div className={`chat chat--${theme}`}>
      <div className="chat__row chat__row--in">
        <div className="bubble bubble--in">{t.chatMessage}</div>
      </div>
      {preset === "sticker" ? (
        <div className="chat__row chat__row--out">
          <div className="chat__sticker">{image ? <img src={image} alt={t.result} /> : <Spinner />}</div>
        </div>
      ) : (
        <>
          <div className="chat__row chat__row--out">
            <div className="bubble bubble--out">
              {t.emojiMessage}{" "}
              {image ? <img className="chat__emoji-inline" src={image} alt="" /> : null}
            </div>
          </div>
          <div className="chat__row chat__row--out">
            <div className="chat__emoji-big">{image ? <img src={image} alt={t.result} /> : <Spinner />}</div>
          </div>
        </>
      )}
    </div>
  );
}

export function SegButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button type="button" role="radio" aria-checked={active} className="segmented__item" onClick={onClick}>
      {children}
    </button>
  );
}

function Spinner() {
  return <span className="spinner" aria-hidden="true" />;
}

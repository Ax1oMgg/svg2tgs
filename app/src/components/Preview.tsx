import { useEffect, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { errorText, isInfo, strings, warningText } from "../i18n";
import type { PreviewBg, PreviewKind, PreviewMode } from "../prefs";
import type { FileItem } from "../types";
import { EMOJI_ONLY } from "../useFiles";
import { formatSize } from "./FileList";

interface Props {
  lang: Lang;
  item: FileItem | null;
  /** Стикер или эмодзи — только вид, файл не пересобирается. */
  kind: PreviewKind;
  onKind: (k: PreviewKind) => void;
  /** Настройки меняют рисунок: тогда слева — «Модифицированный SVG». */
  modified: boolean;
  mode: PreviewMode;
  bg: PreviewBg;
  onMode: (m: PreviewMode) => void;
  onBg: (b: PreviewBg) => void;
  /** Кадрирование мышью — показывается вместо сцены. */
  cropEditor?: React.ReactNode;
}

const SIZE = 512;

/**
 * Сцена превью. «Как в чате» — стикер в диалоге Telegram (эмодзи — в строке
 * текста, крупным сообщением и реакцией; нарезка — сообщением из эмодзи).
 * «Крупно» — исходник и результат рядом. Результат рисуется rlottie — тем же
 * движком, что в Telegram.
 */
export function Preview({ lang, item, kind, onKind, modified, mode, bg, onMode, onBg, cropEditor }: Props) {
  const t = strings[lang];
  const [images, setImages] = useState<{ svg: string | null; tgs: string | null }>({
    svg: null,
    tgs: null,
  });

  // Перерисовка — когда меняется результат (статус, размер, сетка нарезки).
  const key = item ? `${item.id}:${item.status}:${item.outcome?.tgsBytes}:${item.outcome?.grid?.join("x")}` : "";
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
  const grid = item?.outcome?.grid ?? [1, 1];
  const sliced = grid[0] * grid[1] > 1;
  // Проверка тонких линий — только в виде «Эмодзи».
  const warnings = (item?.outcome?.warnings ?? []).filter((w) => kind === "emoji" || w.code !== EMOJI_ONLY);

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
        <div className="segmented" role="radiogroup" aria-label={t.viewKind}>
          <SegButton active={kind === "sticker"} title={t.viewStickerHint} onClick={() => onKind("sticker")}>
            {t.sticker}
          </SegButton>
          <SegButton active={kind === "emoji"} title={t.viewEmojiHint} onClick={() => onKind("emoji")}>
            {t.emoji}
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
        {cropEditor}
        {!cropEditor && !item && <p className="stage__placeholder">{t.noPreview}</p>}
        {!cropEditor && item && item.status === "error" && (
          <div className="stage__error">
            <strong>{t.fileFailed}</strong>
            <span>{item.outcome?.error ? errorText(item.outcome.error, lang) : ""}</span>
          </div>
        )}
        {!cropEditor && item && item.status !== "error" && mode === "chat" && (
          <Chat lang={lang} kind={kind} theme={chatBg} image={images.tgs} grid={sliced ? grid : null} />
        )}
        {!cropEditor && item && item.status !== "error" && mode === "closeup" && (
          <div className="closeup">
            <figure className={`closeup__frame bg-${bg}`}>
              {images.svg ? (
                sliced ? (
                  <GridOverlay grid={grid} image={images.svg} />
                ) : (
                  <img src={images.svg} alt={modified ? t.originalModified : t.original} />
                )
              ) : (
                <Spinner />
              )}
              <figcaption>
                {sliced ? t.sliceSource(grid[0], grid[1]) : modified ? t.originalModified : t.original}
              </figcaption>
            </figure>
            <figure className={`closeup__frame bg-${bg}`}>
              {images.tgs ? (
                <img className={sliced ? "closeup__mosaic" : undefined} src={images.tgs} alt={t.result} />
              ) : (
                <Spinner />
              )}
              <figcaption>
                {sliced
                  ? t.sliceResult(grid[0] * grid[1], formatSize(item.outcome?.tgsBytes ?? 0, lang))
                  : t.result}
                {!sliced && item.outcome?.tgsBytes != null && ` · ${formatSize(item.outcome.tgsBytes, lang)}`}
              </figcaption>
            </figure>
          </div>
        )}
      </div>

      {item && item.outcome && !item.outcome.error && (
        <section className="notes" aria-live="polite">
          {warnings.length === 0 ? (
            <p className="notes__ok">{t.allGood}</p>
          ) : (
            <ul className="notes__list">
              {warnings.map((w, i) => (
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

/** Исходник при нарезке: сетка разрезов и номера клеток (номер = номер файла). */
function GridOverlay({ grid, image }: { grid: [number, number]; image: string }) {
  const [cols, rows] = grid;
  return (
    <div className="slice-src" style={{ aspectRatio: `${cols} / ${rows}` }}>
      <img src={image} alt="" />
      <div
        className="slice-src__grid"
        style={{ gridTemplateColumns: `repeat(${cols}, 1fr)`, gridTemplateRows: `repeat(${rows}, 1fr)` }}
        aria-hidden="true"
      >
        {Array.from({ length: cols * rows }, (_, i) => (
          <span key={i}>{i + 1}</span>
        ))}
      </div>
    </div>
  );
}

/** Сторона эмодзи в сообщении из нарезки, px. */
const SLICE_CHAT_CELL = 26;

/** Диалог Telegram: стикер без пузыря; эмодзи — в тексте, крупно и реакцией;
 *  нарезка — сообщение из эмодзи вплотную, по строке сетки на строку. */
function Chat({
  lang,
  kind,
  theme,
  image,
  grid,
}: {
  lang: Lang;
  kind: PreviewKind;
  theme: "light" | "dark";
  image: string | null;
  grid: [number, number] | null;
}) {
  const t = strings[lang];
  return (
    <div className={`chat chat--${theme}`}>
      <div className="chat__row chat__row--in">
        <div className="bubble bubble--in">{t.chatMessage}</div>
      </div>
      {grid ? (
        <div className="chat__row chat__row--out">
          <div className="bubble bubble--out bubble--mosaic">
            {image ? (
              <img src={image} alt={t.result} style={{ width: grid[0] * SLICE_CHAT_CELL, height: grid[1] * SLICE_CHAT_CELL }} />
            ) : (
              <Spinner />
            )}
          </div>
        </div>
      ) : kind === "sticker" ? (
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
          {/* Реакция: собеседник поставил наш эмодзи на сообщение. */}
          <div className="chat__row chat__row--out">
            <div className="bubble bubble--out bubble--with-reaction">
              <span>{t.reactionMessage}</span>
              <span className="reaction" title={t.reactionHint}>
                {image ? <img src={image} alt="" /> : null}1
              </span>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

export function SegButton({
  active,
  onClick,
  title,
  children,
}: {
  active: boolean;
  onClick: () => void;
  title?: string;
  children: React.ReactNode;
}) {
  return (
    <button type="button" role="radio" aria-checked={active} className="segmented__item" title={title} onClick={onClick}>
      {children}
    </button>
  );
}

function Spinner() {
  return <span className="spinner" aria-hidden="true" />;
}

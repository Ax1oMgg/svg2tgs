import { useEffect, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { errorText, isInfo, strings, warningText } from "../i18n";
import type { PreviewBg, PreviewKind, PreviewMode } from "../prefs";
import type { FileItem } from "../types";
import { EMOJI_ONLY } from "../useFiles";
import { formatSize } from "./FileList";

interface Props {
  lang: Lang;
  /** Веб-версия: результат рисует lottie-web — об этом подпись. */
  web?: boolean;
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
/** Шаги масштаба в «Крупно». */
const ZOOMS = [1, 1.5, 2, 3, 4, 6, 8];
/** Самое большое превью при увеличении (длинная сторона, px). */
const MAX_HI = 2048;

/** Масштаб и сдвиг «Крупно» — общие для обеих половин. Сдвиг — в долях
 *  стороны окошка, от центра. */
interface View {
  zoom: number;
  x: number;
  y: number;
}
const FIT: View = { zoom: 1, x: 0, y: 0 };
const clampView = (v: View): View => {
  const lim = (v.zoom - 1) / 2;
  return { zoom: v.zoom, x: Math.max(-lim, Math.min(lim, v.x)), y: Math.max(-lim, Math.min(lim, v.y)) };
};

/**
 * Сцена превью. «Как в чате» — стикер в диалоге Telegram (эмодзи — в строке
 * текста, крупным сообщением и реакцией; нарезка — сообщением из эмодзи).
 * «Крупно» — исходник и результат рядом. Результат рисуется rlottie — тем же
 * движком, что в Telegram.
 */
export function Preview({ lang, web, item, kind, onKind, modified, mode, bg, onMode, onBg, cropEditor }: Props) {
  const t = strings[lang];
  const [images, setImages] = useState<{ svg: string | null; tgs: string | null }>({
    svg: null,
    tgs: null,
  });

  const [view, setView] = useState<View>(FIT);
  // Картинки в большем разрешении — при увеличении, чтобы не мылились.
  const [hi, setHi] = useState<{ svg: string | null; tgs: string | null; size: number }>({ svg: null, tgs: null, size: 0 });

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

  // Другой файл — масштаб сначала.
  useEffect(() => setView(FIT), [item?.id]);

  // Увеличенное превью: длинная сторона ≈ 512 × масштаб (не больше MAX_HI),
  // с паузой — не на каждый щелчок колёсика.
  const hiSize = Math.min(MAX_HI, Math.ceil((SIZE * view.zoom) / 256) * 256);
  useEffect(() => {
    if (!item || mode !== "closeup" || hiSize <= SIZE || item.status === "pending" || item.status === "error") return;
    if (hi.size === hiSize && hi.svg) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      void Promise.all([backend.preview(item.id, "svg", hiSize), backend.preview(item.id, "tgs", hiSize)]).then(
        ([svg, tgs]) => !cancelled && setHi({ svg, tgs, size: hiSize }),
      );
    }, 200);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, hiSize, mode]);
  useEffect(() => setHi({ svg: null, tgs: null, size: 0 }), [key]);
  const svgImg = view.zoom > 1 && hi.svg ? hi.svg : images.svg;
  const tgsImg = view.zoom > 1 && hi.tgs ? hi.tgs : images.tgs;
  const zoomStep = (dir: 1 | -1) => {
    const i = ZOOMS.findIndex((z) => z >= view.zoom - 1e-6);
    const z = ZOOMS[Math.max(0, Math.min(ZOOMS.length - 1, (i < 0 ? ZOOMS.length - 1 : i) + dir))];
    setView((v) => clampView({ zoom: z, x: (v.x * z) / v.zoom, y: (v.y * z) / v.zoom }));
  };

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
              {svgImg ? (
                <ZoomPane view={view} onView={setView} aspect={sliced ? grid[0] / grid[1] : 1} hint={t.zoomHint}>
                  {sliced ? (
                    <GridOverlay grid={grid} image={svgImg} />
                  ) : (
                    <img src={svgImg} alt={modified ? t.originalModified : t.original} draggable={false} />
                  )}
                </ZoomPane>
              ) : (
                <Spinner />
              )}
              <figcaption>
                {sliced ? t.sliceSource(grid[0], grid[1]) : modified ? t.originalModified : t.original}
              </figcaption>
            </figure>
            <figure className={`closeup__frame bg-${bg}`}>
              {tgsImg ? (
                <ZoomPane view={view} onView={setView} aspect={sliced ? grid[0] / grid[1] : 1} hint={t.zoomHint}>
                  <img src={tgsImg} alt={t.result} draggable={false} />
                </ZoomPane>
              ) : (
                <Spinner />
              )}
              <figcaption>
                {sliced
                  ? t.sliceResult(grid[0] * grid[1], formatSize(item.outcome?.tgsBytes ?? 0, lang))
                  : web
                    ? t.webResult
                    : t.result}
                {!sliced && item.outcome?.tgsBytes != null && ` · ${formatSize(item.outcome.tgsBytes, lang)}`}
              </figcaption>
            </figure>
            {/* Масштаб — общий для обеих половин. */}
            <div className="zoombar" role="toolbar" aria-label={t.zoomBar}>
              <button type="button" className="btn btn--text btn--icon" title={t.zoomOut} aria-label={t.zoomOut} disabled={view.zoom <= ZOOMS[0]} onClick={() => zoomStep(-1)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><path d="M5 12h14" /></svg>
              </button>
              <span className="zoombar__value">{Math.round(view.zoom * 100)} %</span>
              <button type="button" className="btn btn--text btn--icon" title={t.zoomIn} aria-label={t.zoomIn} disabled={view.zoom >= ZOOMS[ZOOMS.length - 1]} onClick={() => zoomStep(1)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><path d="M5 12h14M12 5v14" /></svg>
              </button>
              <span className="zoombar__sep" aria-hidden="true" />
              <button type="button" className="btn btn--text zoombar__fit" title={t.zoomFitHint} disabled={view === FIT || (view.zoom === 1 && view.x === 0 && view.y === 0)} onClick={() => setView(FIT)}>
                {t.zoomFit}
              </button>
            </div>
          </div>
        )}
      </div>

      {web && item && item.status !== "error" && !cropEditor && <p className="stage__engine">{t.webEngineNote}</p>}

      {item && item.outcome && !item.outcome.error && (
        <section className="notes" aria-live="polite">
          {web && warnings.some((w) => w.code === "font_replaced" || w.code === "glyphs_missing") && (
            <p className="notes__banner" role="alert">
              {t.webFontBanner}
            </p>
          )}
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

/**
 * Окошко с масштабом и сдвигом. Колёсико — масштаб вокруг курсора,
 * перетаскивание — сдвиг, двойной щелчок — 100 %. Состояние общее
 * (`view`), поэтому второе окошко повторяет всё 1 в 1.
 */
function ZoomPane({
  view,
  onView,
  aspect,
  hint,
  children,
}: {
  view: View;
  onView: (v: View | ((old: View) => View)) => void;
  aspect: number;
  hint: string;
  children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const [grabbing, setGrabbing] = useState(false);

  // Колёсико — не пассивный слушатель, чтобы не прокручивать окно.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      // Точка под курсором — от центра, в долях стороны.
      const cx = (e.clientX - r.left) / r.width - 0.5;
      const cy = (e.clientY - r.top) / r.height - 0.5;
      onView((v) => {
        const i = ZOOMS.findIndex((z) => z >= v.zoom - 1e-6);
        const next = ZOOMS[Math.max(0, Math.min(ZOOMS.length - 1, (i < 0 ? ZOOMS.length - 1 : i) + (e.deltaY < 0 ? 1 : -1)))];
        const k = next / v.zoom;
        return clampView({ zoom: next, x: cx - (cx - v.x) * k, y: cy - (cy - v.y) * k });
      });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [onView]);

  return (
    <div
      ref={ref}
      className={`zoompane${view.zoom > 1 ? " zoompane--zoomed" : ""}${grabbing ? " zoompane--grabbing" : ""}`}
      style={{ aspectRatio: String(aspect) }}
      title={hint}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { x: e.clientX, y: e.clientY };
        setGrabbing(true);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const r = e.currentTarget.getBoundingClientRect();
        const dx = (e.clientX - d.x) / r.width;
        const dy = (e.clientY - d.y) / r.height;
        drag.current = { x: e.clientX, y: e.clientY };
        onView((v) => clampView({ ...v, x: v.x + dx, y: v.y + dy }));
      }}
      onPointerUp={() => {
        drag.current = null;
        setGrabbing(false);
      }}
      onPointerCancel={() => {
        drag.current = null;
        setGrabbing(false);
      }}
      onDoubleClick={() => onView(FIT)}
    >
      <div
        className="zoompane__content"
        style={{ transform: `translate(${view.x * 100}%, ${view.y * 100}%) scale(${view.zoom})` }}
      >
        {children}
      </div>
    </div>
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

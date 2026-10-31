import { useEffect, useMemo, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings, tgErrorText } from "../i18n";
import type { FileItem, PublishDone, PublishProgress, SetKind, TgError, TgStatus } from "../types";
import { SegButton } from "./Preview";

/** Что выгружаем: клетки нарезки одного файла или файлы списка. */
export type PublishSource =
  | { kind: "grid"; item: FileItem; cols: number; rows: number }
  | { kind: "list"; items: FileItem[] };

interface Props {
  lang: Lang;
  status: TgStatus;
  source: PublishSource;
  /** Стикеры или эмодзи — по умолчанию (нарезка — эмодзи). */
  defaultKind: SetKind;
  onToast: (text: string) => void;
  onClose: () => void;
}

/** Один будущий стикер. */
interface Entry {
  key: string;
  id: string;
  tile: number | null;
  /** Имя файла для Telegram, без расширения. */
  name: string;
  thumb?: string;
}

type Phase = "form" | "running" | "broken" | "done";

const base = (name: string) => name.replace(/\.svg$/i, "");
const pad = (i: number) => String(i + 1).padStart(2, "0");

/** Эмодзи из поля: по символам (графемам), пробелы и запятые — разделители. */
export function parseEmoji(text: string): string[] {
  const Seg = (Intl as unknown as { Segmenter?: new (l: string, o: object) => { segment: (s: string) => Iterable<{ segment: string }> } }).Segmenter;
  const parts = Seg ? Array.from(new Seg("en", { granularity: "grapheme" }).segment(text), (s) => s.segment) : Array.from(text);
  return parts.filter((p) => p.trim() && p !== ",");
}

/** Все стикеры задания по порядку: клетки — как у экспорта (`имя_01`…). */
function entriesOf(source: PublishSource): Entry[] {
  const tilesOf = (it: FileItem): Entry[] => {
    const [c, r] = it.outcome?.grid ?? [1, 1];
    const n = c * r;
    if (n <= 1) return [{ key: it.id, id: it.id, tile: null, name: base(it.name), thumb: it.thumb }];
    return Array.from({ length: n }, (_, i) => ({ key: `${it.id}:${i}`, id: it.id, tile: i, name: `${base(it.name)}_${pad(i)}` }));
  };
  return source.kind === "grid" ? tilesOf(source.item) : source.items.flatMap(tilesOf);
}

/**
 * Окно выгрузки в Telegram: форма → ход → (обрыв → продолжить) → готово.
 * Во время выгрузки окно не закрывается — только «Остановить».
 */
export function PublishDialog({ lang, status, source, defaultKind, onToast, onClose }: Props) {
  const t = strings[lang];
  const ref = useRef<HTMLDialogElement>(null);
  const entries = useMemo(() => entriesOf(source), [source]);
  const [kind, setKind] = useState<SetKind>(defaultKind);
  const [title, setTitle] = useState(() => base(source.kind === "grid" ? source.item.name : (source.items[0]?.name ?? "")));
  const [stem, setStem] = useState("");
  const [stemTouched, setStemTouched] = useState(false);
  const [emojiAll, setEmojiAll] = useState("⭐");
  const [own, setOwn] = useState<Record<string, string>>({});
  const [cell, setCell] = useState<number | null>(null);
  const [phase, setPhase] = useState<Phase>("form");
  const [progress, setProgress] = useState<PublishProgress>({ done: 0, total: entries.length });
  const [error, setError] = useState<TgError | null>(null);
  const [done, setDone] = useState<PublishDone | null>(null);

  useEffect(() => {
    if (!ref.current?.open) ref.current?.showModal();
  }, []);

  // Адрес из названия, пока его не правили руками.
  useEffect(() => {
    if (stemTouched) return;
    let alive = true;
    void backend.tgStem(title).then((s) => alive && setStem(s));
    return () => {
      alive = false;
    };
  }, [title, stemTouched]);

  const emojiOf = (e: Entry) => parseEmoji(own[e.key] ?? emojiAll);

  const run = async (resume: boolean) => {
    const bad = entries.findIndex((e) => {
      const n = emojiOf(e).length;
      return n === 0 || n > 20;
    });
    if (bad >= 0) {
      setError({ code: "emoji_invalid", index: bad });
      return;
    }
    setError(null);
    setPhase("running");
    try {
      const result = await backend.tgPublish(
        {
          kind,
          title: title.trim(),
          stem: stem.trim(),
          resume,
          items: entries.map((e) => ({ id: e.id, tile: e.tile, name: e.name, emoji: emojiOf(e) })),
        },
        setProgress,
      );
      setDone(result);
      setPhase("done");
    } catch (e) {
      const err = e as TgError;
      setError(err);
      // Уже выгруженное осталось в наборе — можно продолжить.
      if (typeof err.done === "number") {
        setProgress({ done: err.done, total: entries.length });
        setPhase("broken");
      } else setPhase("form");
    }
  };

  const running = phase === "running";
  const heading = phase === "done" ? t.pubDoneTitle : phase === "form" ? t.pubTitle : t.pubRunning;
  const pct = `${(progress.done / Math.max(1, progress.total)) * 100}%`;
  const grid = source.kind === "grid" ? source : null;

  return (
    <dialog
      ref={ref}
      className="dialog dialog--publish"
      aria-labelledby="pub-title"
      onCancel={(e) => {
        e.preventDefault();
        if (!running) onClose();
      }}
    >
      <div className="dialog__body">
        <div className="dialog__head">
          <h2 id="pub-title" className="dialog__title">
            {heading}
          </h2>
          {!running && (
            <button type="button" className="btn btn--text btn--icon dialog__close" aria-label={t.close} title={t.close} onClick={onClose}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                <path d="M6 6l12 12M18 6 6 18" />
              </svg>
            </button>
          )}
        </div>

        {phase === "form" && (
          <div className="pub">
            <p className="hint">{t.pubVia(`@${status.bot}`, status.owner ?? "")}</p>

            <div className="pub__pair">
              <div className="field">
                <div className="field__label">{t.pubKind}</div>
                <div className="segmented segmented--wide" role="radiogroup" aria-label={t.pubKind}>
                  <SegButton active={kind === "stickers"} onClick={() => setKind("stickers")}>
                    {t.pubKindStickers}
                  </SegButton>
                  <SegButton active={kind === "emoji"} onClick={() => setKind("emoji")}>
                    {t.pubKindEmoji}
                  </SegButton>
                </div>
              </div>
              <div className="field">
                <div className="field__label">{t.pubSet}</div>
                <div className="segmented segmented--wide" role="radiogroup" aria-label={t.pubSet}>
                  <SegButton active onClick={() => {}}>
                    {t.pubSetNew}
                  </SegButton>
                  <button type="button" role="radio" aria-checked="false" className="segmented__item" disabled title={t.pubSetAppendHint}>
                    {t.pubSetAppend}
                  </button>
                </div>
              </div>
            </div>

            <label className="field">
              <span className="field__label">{t.pubName}</span>
              <input type="text" value={title} maxLength={64} onChange={(e) => setTitle(e.target.value)} />
            </label>

            <label className="field">
              <span className="field__label">{t.pubAddress}</span>
              <span className="pub__address">
                <span className="pub__affix">t.me/{kind === "emoji" ? "addemoji" : "addstickers"}/</span>
                <input
                  type="text"
                  value={stem}
                  spellCheck={false}
                  onChange={(e) => {
                    setStemTouched(true);
                    setStem(e.target.value.replace(/[^A-Za-z0-9_]/g, ""));
                  }}
                />
                <span className="pub__affix">_by_{status.bot}</span>
              </span>
            </label>

            <div className="field">
              <div className="pub__emoji-head">
                <span className="field__label">{grid ? t.pubEmojiCells : t.pubEmojiStickers}</span>
                <label className="pub__emoji-all" title={t.pubEmojiAllHint}>
                  <input type="text" className="pub__emoji-input" value={emojiAll} onChange={(e) => setEmojiAll(e.target.value)} />
                  <span>{t.pubEmojiAll}</span>
                </label>
              </div>

              {grid ? (
                <div className="pub__grid-box">
                  <div className="pub__grid" style={{ gridTemplateColumns: `repeat(${grid.cols}, minmax(0, 1fr))` }}>
                    {entries.map((e, i) => {
                      const c = i % grid.cols;
                      const r = Math.floor(i / grid.cols);
                      const label = `${t.pubCell(i + 1)}: ${emojiOf(e).join(" ")}`;
                      return (
                        <button
                          key={e.key}
                          type="button"
                          className={`pub__cell${cell === i ? " pub__cell--active" : ""}${own[e.key] != null ? " pub__cell--own" : ""}`}
                          title={label}
                          aria-label={label}
                          aria-pressed={cell === i}
                          onClick={() => setCell(cell === i ? null : i)}
                          style={
                            grid.item.thumb
                              ? {
                                  backgroundImage: `url("${grid.item.thumb}")`,
                                  backgroundSize: `${grid.cols * 100}% ${grid.rows * 100}%`,
                                  backgroundPosition: `${grid.cols > 1 ? (c / (grid.cols - 1)) * 100 : 0}% ${grid.rows > 1 ? (r / (grid.rows - 1)) * 100 : 0}%`,
                                }
                              : undefined
                          }
                        >
                          <span className="pub__cell-emoji">{emojiOf(e)[0] ?? ""}</span>
                        </button>
                      );
                    })}
                  </div>
                  {cell != null && entries[cell] && (
                    <label className="row pub__cell-edit">
                      <span>{t.pubCell(cell + 1)}</span>
                      <input
                        type="text"
                        className="pub__emoji-input"
                        autoFocus
                        value={own[entries[cell].key] ?? emojiAll}
                        onChange={(e) => setOwn({ ...own, [entries[cell].key]: e.target.value })}
                      />
                    </label>
                  )}
                  <span className="hint">{t.pubGridInfo(entries.length, grid.item.name, grid.cols, grid.rows)}</span>
                </div>
              ) : (
                <ol className="pub__list scroll" aria-label={t.pubOrder}>
                  {entries.map((e, i) => (
                    <li key={e.key} className="pub__row">
                      <span className="pub__n">{i + 1}</span>
                      <span className="pub__thumb">{e.thumb && <img src={e.thumb} alt="" />}</span>
                      <span className="pub__name" title={e.name}>
                        {e.name}
                      </span>
                      <input
                        type="text"
                        className="pub__emoji-input"
                        aria-label={t.pubEmojiFor(e.name)}
                        title={t.pubEmojiFor(e.name)}
                        value={own[e.key] ?? emojiAll}
                        onChange={(ev) => setOwn({ ...own, [e.key]: ev.target.value })}
                      />
                    </li>
                  ))}
                </ol>
              )}
            </div>

            {error && <p className="note note--error">{tgErrorText(error, lang)}</p>}

            <div className="dialog__actions">
              <button type="button" className="btn btn--text" onClick={onClose}>
                {t.cancel}
              </button>
              <button type="button" className="btn btn--primary" disabled={!title.trim() || !stem.trim() || entries.length === 0} onClick={() => void run(false)}>
                {t.pubGo(entries.length)}
              </button>
            </div>
          </div>
        )}

        {(phase === "running" || phase === "broken") && (
          <div className="pub">
            <div className="pub__progress-head">
              <span className="tg__strong">«{title.trim()}»</span>
              <span className="hint">{t.pubProgress(progress.done, progress.total)}</span>
            </div>
            <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={progress.total} aria-valuenow={progress.done} aria-label={t.pubRunning}>
              <div className={`progress__fill${phase === "broken" ? " progress__fill--warn" : ""}`} style={{ width: pct }} />
            </div>
            {running && (
              <span className="tg__wait">
                <span className="spinner spinner--sm" aria-hidden="true" />
                {entries[Math.min(progress.done, entries.length - 1)]?.name}.tgs
              </span>
            )}
            {phase === "broken" && error && (
              <p className="note note--warn">
                {tgErrorText(error, lang)} {error.code !== "set_full" && t.pubKept}
              </p>
            )}
            <div className="dialog__actions">
              {running ? (
                <button type="button" className="btn btn--default" title={t.pubStopHint} onClick={() => void backend.tgStop()}>
                  {t.pubStop}
                </button>
              ) : (
                <>
                  <button type="button" className="btn btn--text" onClick={onClose}>
                    {t.close}
                  </button>
                  {error?.code !== "set_full" && (
                    <button type="button" className="btn btn--primary" onClick={() => void run(true)}>
                      {t.pubContinue}
                    </button>
                  )}
                </>
              )}
            </div>
          </div>
        )}

        {phase === "done" && done && (
          <div className="pub">
            <div className="pub__done">
              <span className="pub__done-mark" aria-hidden="true">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              </span>
              <span>
                {t.pubDoneText(done.count, title.trim())}
                <br />
                <span className="hint">{t.pubDoneSub}</span>
              </span>
            </div>
            <div className="pub__link">
              <span className="pub__link-text">{done.link.replace(/^https:\/\//, "")}</span>
              <button
                type="button"
                className="btn btn--text btn--icon"
                aria-label={t.pubCopyLink}
                title={t.pubCopyLink}
                onClick={() => void navigator.clipboard.writeText(done.link).then(() => onToast(t.pubLinkCopied))}
              >
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <rect x="9" y="9" width="12" height="12" rx="2" />
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                </svg>
              </button>
            </div>
            <div className="dialog__actions">
              <button type="button" className="btn btn--default" onClick={() => void backend.tgOpen({ kind, name: done.name })}>
                {t.pubOpen}
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                </svg>
              </button>
              <button type="button" className="btn btn--primary" onClick={onClose}>
                {t.done}
              </button>
            </div>
          </div>
        )}
      </div>
    </dialog>
  );
}

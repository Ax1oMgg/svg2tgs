import { useEffect, useMemo, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings, tgErrorText } from "../i18n";
import { THUMB_TGS_MAX, type EditDone, type PublishProgress, type SetKind, type SetView, type StickerPreview, type TgError, type TgStatus } from "../types";
import { parseEmoji, StarIcon } from "./PublishDialog";
import { SetPicker, stemOf } from "./SetPicker";

interface Props {
  lang: Lang;
  status: TgStatus;
  /** Какой набор открыть сразу (полное имя); `null` — выбрать в окне. */
  initial: string | null;
  onToast: (text: string) => void;
  onClose: () => void;
}

type Phase = "edit" | "confirm" | "running" | "broken" | "done";

/** Сторона картинки стикера в сетке (с запасом на плотные экраны). */
const THUMB = 96;
/** Сколько стикеров скачивать одновременно. */
const PARALLEL = 4;

/** Набор, открытый для правки, и что с ним сделал пользователь. */
interface Draft {
  set: SetView;
  kind: SetKind;
  /** Все стикеры (`fileId`) в новом порядке; удалённые остаются на месте. */
  order: string[];
  deleted: Set<string>;
  /** Новые эмодзи — текст поля, как ввёл пользователь. */
  emoji: Record<string, string>;
  cover: string | null;
}

const draftOf = (set: SetView, fallback: SetKind): Draft => ({
  set,
  kind: set.kind ?? fallback,
  order: set.stickers.map((s) => s.fileId),
  deleted: new Set(),
  emoji: {},
  cover: null,
});

/**
 * Окно «Набор в Telegram»: набор бота сеткой стикеров. Перетаскиванием —
 * порядок, по щелчку — эмодзи, обложка, удаление. «Сохранить» отправляет
 * в Telegram только изменения; удаление — после подтверждения. Обрыв —
 * «Продолжить»: правка считается от того, что уже лежит в наборе.
 */
export function SetDialog({ lang, status, initial, onToast, onClose }: Props) {
  const t = strings[lang];
  const ref = useRef<HTMLDialogElement>(null);
  const [stem, setStem] = useState(initial ? stemOf(initial, status.bot) : "");
  const [finding, setFinding] = useState(false);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [pictures, setPictures] = useState<Record<string, StickerPreview>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [dragged, setDragged] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("edit");
  const [progress, setProgress] = useState<PublishProgress>({ done: 0, total: 0 });
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<TgError | null>(null);
  const [done, setDone] = useState<EditDone | null>(null);

  useEffect(() => {
    if (!ref.current?.open) ref.current?.showModal();
  }, []);

  const open = async (s: string) => {
    if (!s.trim()) return;
    setFinding(true);
    setError(null);
    setDraft(null);
    setSelected(null);
    try {
      const set = await backend.tgSetInfo(s.trim());
      const known = status.sets.find((k) => k.name.toLowerCase() === set.name.toLowerCase());
      setDraft(draftOf(set, known?.kind ?? "stickers"));
      setPhase("edit");
    } catch (e) {
      setError(e as TgError);
    } finally {
      setFinding(false);
    }
  };

  useEffect(() => {
    if (initial) void open(stem);
    // Только при открытии окна.
  }, []);

  // Картинки стикеров: скачиваются по несколько, по мере готовности.
  const ids = useMemo(() => draft?.set.stickers.map((s) => s.fileId) ?? [], [draft?.set]);
  useEffect(() => {
    let alive = true;
    const queue = ids.filter((id) => !pictures[id]);
    const worker = async () => {
      for (let id = queue.shift(); id && alive; id = queue.shift()) {
        try {
          const p = await backend.tgStickerPreview(id, THUMB);
          if (alive) setPictures((all) => ({ ...all, [id]: p }));
        } catch {
          if (alive) setPictures((all) => ({ ...all, [id]: { png: null, bytes: 0 } }));
        }
      }
    };
    for (let i = 0; i < PARALLEL; i++) void worker();
    return () => {
      alive = false;
    };
    // Только когда открыт другой набор; уже скачанные не запрашиваются заново.
  }, [ids]);

  const byId = useMemo(() => new Map(draft?.set.stickers.map((s) => [s.fileId, s]) ?? []), [draft?.set]);

  /** Изменённые эмодзи: только там, где они отличаются от того, что в наборе. */
  const emojiChanges = useMemo(() => {
    if (!draft) return [];
    return Object.entries(draft.emoji)
      .filter(([id]) => !draft.deleted.has(id))
      .map(([id, text]) => ({ fileId: id, emoji: parseEmoji(text) }))
      .filter((c) => c.emoji.length !== 1 || c.emoji[0] !== byId.get(c.fileId)?.emoji);
  }, [draft, byId]);

  const kept = draft ? draft.order.filter((id) => !draft.deleted.has(id)) : [];
  const orderChanged = draft ? kept.join() !== draft.set.stickers.map((s) => s.fileId).filter((id) => !draft.deleted.has(id)).join() : false;
  const changes = [
    orderChanged && t.setChOrder,
    emojiChanges.length > 0 && t.setChEmoji(emojiChanges.length),
    draft && draft.deleted.size > 0 && t.setChDelete(draft.deleted.size),
    draft?.cover && t.setChCover,
  ].filter(Boolean) as string[];
  const badEmoji = emojiChanges.find((c) => c.emoji.length === 0 || c.emoji.length > 20);

  const update = (f: (d: Draft) => Partial<Draft>) => setDraft((d) => (d ? { ...d, ...f(d) } : d));

  const move = (id: string, to: number) =>
    update((d) => {
      const order = d.order.filter((x) => x !== id);
      order.splice(Math.max(0, Math.min(order.length, to)), 0, id);
      return { order };
    });

  const save = async () => {
    if (!draft) return;
    setError(null);
    setStopping(false);
    setPhase("running");
    setProgress({ done: 0, total: 0 });
    try {
      const result = await backend.tgEdit(
        {
          name: draft.set.name,
          kind: draft.kind,
          seen: draft.set.stickers.map((s) => s.fileId),
          delete: [...draft.deleted],
          order: kept,
          emoji: emojiChanges,
          cover: draft.cover,
        },
        setProgress,
      );
      setDone(result);
      setPhase("done");
    } catch (e) {
      const err = e as TgError;
      setError(err);
      setPhase(typeof err.done === "number" ? "broken" : "edit");
    }
  };

  const running = phase === "running";
  const sel = selected && draft ? draft.order.indexOf(selected) : -1;
  const selInfo = selected ? byId.get(selected) : undefined;
  const selPicture = selected ? pictures[selected] : undefined;
  const coverTooBig = draft?.kind === "stickers" && (selPicture?.bytes ?? 0) > THUMB_TGS_MAX;
  const title = draft?.set.title ?? "";
  const pct = `${(progress.done / Math.max(1, progress.total)) * 100}%`;

  return (
    <dialog
      ref={ref}
      className="dialog dialog--set"
      aria-labelledby="set-title"
      onCancel={(e) => {
        e.preventDefault();
        if (!running) onClose();
      }}
    >
      <div className="dialog__body">
        <div className="dialog__head">
          <h2 id="set-title" className="dialog__title">
            {phase === "running" || phase === "broken" ? t.setRunning : t.setTitle}
          </h2>
          {!running && (
            <button type="button" className="btn btn--text btn--icon dialog__close" aria-label={t.close} title={t.close} onClick={onClose}>
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                <path d="M6 6l12 12M18 6 6 18" />
              </svg>
            </button>
          )}
        </div>

        {phase === "edit" && (
          <div className="pub">
            <SetPicker
              lang={lang}
              status={status}
              stem={stem}
              finding={finding}
              disabled={changes.length > 0}
              onPick={(s) => {
                setStem(s);
                void open(s);
              }}
              onType={setStem}
              onFind={() => void open(stem)}
            />

            {!draft && !finding && !error && <p className="hint">{t.setPick}</p>}
            {finding && (
              <span className="tg__wait">
                <span className="spinner spinner--sm" aria-hidden="true" />
                {t.setLoading}
              </span>
            )}

            {draft && (
              <>
                <div className="set__head">
                  <span className="tg__strong">«{title}»</span>
                  <span className="hint">
                    {Object.keys(pictures).filter((id) => byId.has(id)).length < ids.length
                      ? t.setPictures(Object.keys(pictures).filter((id) => byId.has(id)).length, ids.length)
                      : t.setHint}
                  </span>
                </div>
                <ol className={`set__grid scroll${draft.kind === "emoji" ? " set__grid--emoji" : ""}`} aria-label={t.pubOrder}>
                  {draft.order.map((id, i) => {
                    const info = byId.get(id);
                    const pic = pictures[id];
                    const gone = draft.deleted.has(id);
                    const emoji = draft.emoji[id] != null ? (parseEmoji(draft.emoji[id])[0] ?? "") : (info?.emoji ?? "");
                    const n = kept.indexOf(id) + 1;
                    return (
                      <li key={id} className="set__cell-wrap">
                        <button
                          type="button"
                          draggable
                          className={`set__cell${selected === id ? " set__cell--active" : ""}${gone ? " set__cell--gone" : ""}${dragged === id ? " set__cell--dragged" : ""}`}
                          aria-pressed={selected === id}
                          aria-label={`${t.setSticker(i + 1)}: ${emoji}${gone ? `, ${t.setDeleted}` : ""}${draft.cover === id ? `, ${t.pubCover}` : ""}`}
                          title={gone ? t.setDeleted : t.setSticker(n)}
                          onClick={() => setSelected(selected === id ? null : id)}
                          onDragStart={(e) => {
                            setDragged(id);
                            e.dataTransfer.effectAllowed = "move";
                            e.dataTransfer.setData("text/plain", id);
                          }}
                          onDragEnd={() => setDragged(null)}
                          onDragOver={(e) => {
                            if (dragged && dragged !== id) e.preventDefault();
                          }}
                          onDrop={(e) => {
                            e.preventDefault();
                            if (dragged && dragged !== id) move(dragged, i);
                            setDragged(null);
                          }}
                        >
                          {pic?.png ? <img src={pic.png} alt="" draggable={false} /> : <span className="spinner spinner--sm" aria-hidden="true" />}
                          {!gone && <span className="set__n">{n}</span>}
                          <span className="set__emoji">{emoji}</span>
                          {draft.cover === id && (
                            <span className="pub__cover-mark" aria-hidden="true">
                              <StarIcon filled />
                            </span>
                          )}
                        </button>
                      </li>
                    );
                  })}
                </ol>

                {selected && selInfo && (
                  <div className="set__edit">
                    <div className="row">
                      <label htmlFor="set-emoji" className="tg__strong">
                        {t.setSticker(sel + 1)} · {t.setEmoji}
                      </label>
                      <input
                        id="set-emoji"
                        type="text"
                        className="pub__emoji-input"
                        disabled={draft.deleted.has(selected)}
                        value={draft.emoji[selected] ?? selInfo.emoji ?? ""}
                        onChange={(e) => update((d) => ({ emoji: { ...d.emoji, [selected]: e.target.value } }))}
                      />
                    </div>
                    <span className="hint">{t.setEmojiHint}</span>
                    <div className="row set__actions">
                      <button type="button" className="btn btn--default" disabled={sel <= 0} onClick={() => move(selected, sel - 1)}>
                        ← {t.setEarlier}
                      </button>
                      <button type="button" className="btn btn--default" disabled={sel >= draft.order.length - 1} onClick={() => move(selected, sel + 1)}>
                        {t.setLater} →
                      </button>
                      <button
                        type="button"
                        className={`btn btn--text pub__cover${draft.cover === selected ? " pub__cover--on" : ""}`}
                        aria-pressed={draft.cover === selected}
                        disabled={draft.deleted.has(selected) || (coverTooBig && draft.cover !== selected)}
                        title={coverTooBig ? t.setCoverTooBig(Math.ceil((selPicture?.bytes ?? 0) / 1024)) : t.setCoverHint}
                        onClick={() => update((d) => ({ cover: d.cover === selected ? null : selected }))}
                      >
                        <StarIcon filled={draft.cover === selected} />
                        {t.setCover}
                      </button>
                      <button
                        type="button"
                        className={`btn btn--text${draft.deleted.has(selected) ? "" : " set__delete"}`}
                        onClick={() =>
                          update((d) => {
                            const deleted = new Set(d.deleted);
                            if (deleted.has(selected)) deleted.delete(selected);
                            else deleted.add(selected);
                            return { deleted, cover: deleted.has(selected) && d.cover === selected ? null : d.cover };
                          })
                        }
                      >
                        {draft.deleted.has(selected) ? t.setRestore : t.setDelete}
                      </button>
                    </div>
                    {coverTooBig && <span className="hint">{t.setCoverTooBig(Math.ceil((selPicture?.bytes ?? 0) / 1024))}</span>}
                  </div>
                )}
              </>
            )}

            {error && <p className="note note--error">{tgErrorText(error, lang)}</p>}
            {badEmoji && <p className="note note--warn">{tgErrorText({ code: "emoji_invalid", index: Math.max(0, kept.indexOf(badEmoji.fileId)) }, lang)}</p>}

            <div className="dialog__actions set__footer">
              <span className="hint">{draft ? (changes.length ? t.setChanges(changes.join(", ")) : t.setNoChanges) : ""}</span>
              <button type="button" className="btn btn--text" onClick={onClose}>
                {changes.length ? t.cancel : t.close}
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={!draft || changes.length === 0 || !!badEmoji || kept.length === 0}
                onClick={() => (draft && draft.deleted.size > 0 ? setPhase("confirm") : void save())}
              >
                {t.setSave}
              </button>
            </div>
          </div>
        )}

        {phase === "confirm" && draft && (
          <div className="pub">
            <p className="tg__strong">{t.setConfirmTitle(draft.deleted.size)}</p>
            <div className="set__gone">
              {[...draft.deleted].map((id) => (
                <span key={id} className="set__gone-item">
                  {pictures[id]?.png && <img src={pictures[id].png ?? ""} alt="" />}
                </span>
              ))}
            </div>
            <p className="note note--warn">{t.setConfirmText}</p>
            <div className="dialog__actions">
              <button type="button" className="btn btn--text" onClick={() => setPhase("edit")}>
                {t.setBack}
              </button>
              <button type="button" className="btn btn--danger" onClick={() => void save()}>
                {t.setConfirmGo}
              </button>
            </div>
          </div>
        )}

        {(phase === "running" || phase === "broken") && (
          <div className="pub">
            <div className="pub__progress-head">
              <span className="tg__strong">«{title}»</span>
              <span className="hint">{t.pubProgress(progress.done, progress.total)}</span>
            </div>
            <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={progress.total} aria-valuenow={progress.done} aria-label={t.setRunning}>
              <div className={`progress__fill${phase === "broken" ? " progress__fill--warn" : ""}`} style={{ width: pct }} />
            </div>
            {running && progress.wait != null && !stopping && (
              <p className="note note--info" role="status">
                {t.pubWait(progress.wait)}
              </p>
            )}
            {phase === "broken" && error && (
              <p className="note note--warn">
                {tgErrorText(error, lang)} {t.setKept}
              </p>
            )}
            <div className="dialog__actions">
              {running ? (
                <button
                  type="button"
                  className="btn btn--default"
                  title={stopping ? t.pubStoppingHint : t.pubStopHint}
                  disabled={stopping}
                  onClick={() => {
                    setStopping(true);
                    void backend.tgStop();
                  }}
                >
                  {stopping ? t.pubStopping : t.pubStop}
                </button>
              ) : (
                <>
                  <button type="button" className="btn btn--text" onClick={onClose}>
                    {t.close}
                  </button>
                  <button type="button" className="btn btn--primary" onClick={() => void save()}>
                    {t.pubContinue}
                  </button>
                </>
              )}
            </div>
          </div>
        )}

        {phase === "done" && done && draft && (
          <div className="pub">
            <div className="pub__done">
              <span className="pub__done-mark" aria-hidden="true">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              </span>
              <span>{t.setDone(title, done.count)}</span>
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
              <button type="button" className="btn btn--default" onClick={() => void backend.tgOpen({ kind: draft.kind, name: done.name })}>
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

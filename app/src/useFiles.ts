/**
 * Список файлов и очередь конвертации.
 *
 * Файлы конвертируются в фоне по несколько сразу. У файла могут быть
 * свои настройки; без них действуют общие. При смене общих настроек
 * пересчитываются только файлы без своих, при смене своих — только этот
 * файл. У каждого файла счётчик правок: ответ на устаревшие настройки
 * отбрасывается, а один и тот же файл не конвертируется дважды одновременно.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import * as backend from "./backend";
import { isInfo } from "./i18n";
import type { FileItem, Options, Outcome, Source, Status } from "./types";

const PARALLEL = 3;
const THUMB_SIZE = 96;
/** Пауза перед пересчётом: не запускать конвертацию на каждый шаг ползунка. */
const DEBOUNCE_MS = 250;

/** Замечание только для вида «Эмодзи» — на значок файла в списке не влияет. */
export const EMOJI_ONLY = "thin_lines_for_emoji";

function statusOf(o: Outcome): Status {
  if (o.error) return "error";
  return o.warnings.some((w) => !isInfo(w) && w.code !== EMOJI_ONLY) ? "warn" : "ok";
}

let counter = 0;
const newId = () => `f${Date.now().toString(36)}${(counter++).toString(36)}`;

/** Настройки, по которым конвертируется файл. */
export const effectiveOptions = (item: FileItem, global: Options): Options => item.options ?? global;

/** Настройки для ядра: пресет «эмодзи» включает проверку тонких линий —
 *  на сам файл он не влияет, а замечание показывается в виде «Эмодзи». */
const forCore = (o: Options): Options => ({ ...o, preset: "emoji" });

export function useFiles(options: Options) {
  const [items, setItems] = useState<FileItem[]>([]);
  const optionsRef = useRef(options);
  const revs = useRef(new Map<string, number>());
  const queue = useRef<string[]>([]);
  const running = useRef(new Set<string>());
  const itemsRef = useRef<FileItem[]>([]);
  itemsRef.current = items;
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  const update = (id: string, patch: Partial<FileItem>) =>
    setItems((list) => list.map((it) => (it.id === id ? { ...it, ...patch } : it)));

  const pump = useCallback(() => {
    let skipped = 0;
    while (running.current.size < PARALLEL && queue.current.length > skipped) {
      const id = queue.current.shift()!;
      const item = itemsRef.current.find((it) => it.id === id);
      if (!item) continue;
      // Этот файл ещё конвертируется — вернуть в конец очереди.
      if (running.current.has(id)) {
        queue.current.push(id);
        skipped += 1;
        continue;
      }
      running.current.add(id);
      const rev = revs.current.get(id);
      const fresh = () => revs.current.get(id) === rev;
      backend
        .convert(id, item.source, forCore(effectiveOptions(item, optionsRef.current)))
        .then(async (outcome) => {
          if (!fresh()) return;
          const thumb = outcome.error ? undefined : await backend.preview(id, "tgs", THUMB_SIZE);
          if (!fresh()) return;
          update(id, { outcome, status: statusOf(outcome), thumb: thumb ?? undefined });
        })
        .catch(() => {
          if (fresh()) {
            update(id, {
              status: "error",
              outcome: { id, tgsBytes: null, grid: [1, 1], aspect: 1, warnings: [], error: "read_failed" },
            });
          }
        })
        .finally(() => {
          running.current.delete(id);
          pump();
        });
    }
  }, []);

  /** Ставит файлы в очередь; прежние ответы для них станут устаревшими. */
  const enqueue = useCallback(
    (ids: string[]) => {
      for (const id of ids) revs.current.set(id, (revs.current.get(id) ?? 0) + 1);
      queue.current.push(...ids.filter((id) => !queue.current.includes(id)));
      // Состояние обновляется асинхронно — запускаем очередь на следующем тике,
      // когда новые файлы и их настройки уже в списке.
      setTimeout(pump, 0);
    },
    [pump],
  );

  /** Пересчитать файлы: пометить «в работе» и поставить в очередь. */
  const reconvert = useCallback(
    (ids: string[]) => {
      if (ids.length === 0) return;
      const set = new Set(ids);
      setItems((list) => list.map((it) => (set.has(it.id) ? { ...it, status: "pending" as Status } : it)));
      enqueue(ids);
    },
    [enqueue],
  );

  /** Добавляет файлы; повторно добавленный путь не дублируется. */
  const add = useCallback(
    (entries: { name: string; source: Source }[]) => {
      const existing = new Set(
        itemsRef.current.map((it) => (it.source.kind === "path" ? it.source.path : it.id)),
      );
      const fresh: FileItem[] = entries
        .filter((e) => e.source.kind !== "path" || !existing.has(e.source.path))
        .map((e) => ({ id: newId(), name: e.name, source: e.source, status: "pending" }));
      if (fresh.length === 0) return [];
      setItems((list) => [...list, ...fresh]);
      itemsRef.current = [...itemsRef.current, ...fresh];
      enqueue(fresh.map((f) => f.id));
      return fresh.map((f) => f.id);
    },
    [enqueue],
  );

  const forgetIds = (ids: string[]) => {
    for (const id of ids) {
      revs.current.delete(id);
      clearTimeout(timers.current.get(id));
      timers.current.delete(id);
    }
    queue.current = queue.current.filter((id) => !ids.includes(id));
    void backend.forget(ids);
  };

  const remove = useCallback((ids: string[]) => {
    forgetIds(ids);
    setItems((list) => list.filter((it) => !ids.includes(it.id)));
  }, []);

  const clear = useCallback(() => {
    forgetIds(itemsRef.current.map((it) => it.id));
    setItems([]);
  }, []);

  /** Свои настройки файлов (`undefined` — снова общие); файлы пересчитываются. */
  const setOptionsFor = useCallback(
    (entries: [string, Options | undefined][]) => {
      if (entries.length === 0) return;
      const map = new Map(entries);
      setItems((list) => list.map((it) => (map.has(it.id) ? { ...it, options: map.get(it.id) } : it)));
      itemsRef.current = itemsRef.current.map((it) => (map.has(it.id) ? { ...it, options: map.get(it.id) } : it));
      for (const id of map.keys()) {
        clearTimeout(timers.current.get(id));
        timers.current.set(
          id,
          setTimeout(() => {
            timers.current.delete(id);
            reconvert([id]);
          }, DEBOUNCE_MS),
        );
      }
    },
    [reconvert],
  );

  const setItemOptions = useCallback(
    (id: string, own: Options | undefined) => setOptionsFor([[id, own]]),
    [setOptionsFor],
  );

  // Смена общих настроек — пересчитать файлы, у которых нет своих.
  const mounted = useRef(false);
  useEffect(() => {
    optionsRef.current = options;
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    const t = setTimeout(() => {
      reconvert(itemsRef.current.filter((it) => !it.options).map((it) => it.id));
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [options, reconvert]);

  return { items, add, remove, clear, setItemOptions, setOptionsFor };
}

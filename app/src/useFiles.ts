/**
 * Список файлов и очередь конвертации.
 *
 * Файлы конвертируются в фоне по несколько сразу; при смене настроек
 * пересчитываются все. Ответы на устаревшие настройки отбрасываются.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import * as backend from "./backend";
import { isInfo } from "./i18n";
import type { FileItem, Options, Outcome, Source, Status } from "./types";

const PARALLEL = 3;
const THUMB_SIZE = 96;

function statusOf(o: Outcome): Status {
  if (o.error) return "error";
  return o.warnings.some((w) => !isInfo(w)) ? "warn" : "ok";
}

let counter = 0;
const newId = () => `f${Date.now().toString(36)}${(counter++).toString(36)}`;

export function useFiles(options: Options) {
  const [items, setItems] = useState<FileItem[]>([]);
  const optionsRef = useRef(options);
  const generation = useRef(0);
  const queue = useRef<string[]>([]);
  const running = useRef(0);
  const itemsRef = useRef<FileItem[]>([]);
  itemsRef.current = items;

  const update = (id: string, patch: Partial<FileItem>) =>
    setItems((list) => list.map((it) => (it.id === id ? { ...it, ...patch } : it)));

  const pump = useCallback(() => {
    while (running.current < PARALLEL && queue.current.length > 0) {
      const id = queue.current.shift()!;
      const item = itemsRef.current.find((it) => it.id === id);
      if (!item) continue;
      running.current += 1;
      const gen = generation.current;
      backend
        .convert(id, item.source, optionsRef.current)
        .then(async (outcome) => {
          if (gen !== generation.current) return;
          const thumb = outcome.error ? undefined : await backend.preview(id, "tgs", THUMB_SIZE);
          if (gen !== generation.current) return;
          update(id, { outcome, status: statusOf(outcome), thumb: thumb ?? undefined });
        })
        .catch(() => {
          if (gen === generation.current) {
            update(id, {
              status: "error",
              outcome: { id, tgsBytes: null, warnings: [], error: "read_failed" },
            });
          }
        })
        .finally(() => {
          running.current -= 1;
          pump();
        });
    }
  }, []);

  const enqueue = useCallback(
    (ids: string[]) => {
      queue.current.push(...ids.filter((id) => !queue.current.includes(id)));
      // Состояние обновляется асинхронно — запускаем очередь на следующем тике,
      // когда новые файлы уже в списке.
      setTimeout(pump, 0);
    },
    [pump],
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

  const remove = useCallback((ids: string[]) => {
    queue.current = queue.current.filter((id) => !ids.includes(id));
    setItems((list) => list.filter((it) => !ids.includes(it.id)));
    void backend.forget(ids);
  }, []);

  const clear = useCallback(() => {
    const ids = itemsRef.current.map((it) => it.id);
    generation.current += 1;
    queue.current = [];
    setItems([]);
    void backend.forget(ids);
  }, []);

  // Смена настроек — пересчитать всё (с небольшой задержкой, чтобы
  // не запускать конвертацию на каждый шаг ползунка).
  useEffect(() => {
    optionsRef.current = options;
    const t = setTimeout(() => {
      if (itemsRef.current.length === 0) return;
      generation.current += 1;
      queue.current = [];
      setItems((list) => list.map((it) => ({ ...it, status: "pending" as Status })));
      enqueue(itemsRef.current.map((it) => it.id));
    }, 250);
    return () => clearTimeout(t);
  }, [options, enqueue]);

  return { items, add, remove, clear };
}

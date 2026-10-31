import { useCallback, useEffect, useRef, useState } from "react";
import * as backend from "./backend";
import { EmptyState } from "./components/EmptyState";
import { ExportPanel, type Scope } from "./components/ExportPanel";
import { FileList } from "./components/FileList";
import { Preview } from "./components/Preview";
import { ConflictDialog, type ConflictChoice } from "./components/ConflictDialog";
import { CropEditor } from "./components/CropEditor";
import { ImportDialog } from "./components/ImportDialog";
import { PublishDialog, type PublishSource } from "./components/PublishDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { Toast, type ToastData } from "./components/Toast";
import { TopBar } from "./components/TopBar";
import { strings } from "./i18n";
import { exportName, exportSubfolder, loadPrefs, savePrefs, type Prefs } from "./prefs";
import {
  DEFAULT_OPTIONS,
  GRID_KEYS,
  OWN_KEYS,
  SECTION_KEYS,
  sameKeys,
  type Existing,
  type FileItem,
  type Options,
  type SaveRequest,
  type Saved,
  type SectionId,
  type SetKind,
  type TgStatus,
} from "./types";
import { effectiveOptions, useFiles } from "./useFiles";
import "./App.css";

/** Открытый вопрос «файл уже есть»: ответ передаётся в `resolve`. */
interface ConflictAsk {
  existing: Existing;
  rest: number;
  resolve: (r: { choice: ConflictChoice; all: boolean }) => void;
}

/** Через сколько закрывается уведомление с «Отменить». */
const UNDO_MS = 6000;

/** Папка файла по его пути (оба вида разделителей). */
const dirname = (path: string) => path.replace(/[\\/][^\\/]*$/, "");

/** Ключи настроек, которые различаются у `a` и `b`. */
const changedKeys = (a: Options, b: Options) =>
  (Object.keys({ ...a, ...b }) as (keyof Options)[]).filter((k) => JSON.stringify(a[k]) !== JSON.stringify(b[k]));

/** Меняют ли настройки сам рисунок (для подписи «Модифицированный SVG»). */
const DRAWING_KEYS = [
  ...SECTION_KEYS.position,
  ...SECTION_KEYS.transform,
  ...SECTION_KEYS.style,
  ...SECTION_KEYS.background,
  ...GRID_KEYS,
] as (keyof Options)[];

/** Снимок настроек для Ctrl+Z: общие и свои у каждого файла. */
interface Snapshot {
  global: Options;
  own: [string, Options | undefined][];
}
/** Правки чаще этого склеиваются в один шаг отмены (перетаскивание ползунка). */
const UNDO_GROUP_MS = 600;
const UNDO_DEPTH = 100;

export function App() {
  const [prefs, setPrefs] = useState<Prefs>(loadPrefs);
  const patch = (p: Partial<Prefs>) => setPrefs((old) => ({ ...old, ...p }));
  const { items, add, remove, clear, setItemOptions, setOptionsFor } = useFiles(prefs.options);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // Выбранные файлы (Ctrl/Shift в списке); `selectedId` — главный из них.
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(new Set());
  const anchor = useRef<string | null>(null);
  const [onlyProblems, setOnlyProblems] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [toast, setToast] = useState<ToastData | null>(null);
  const [cropping, setCropping] = useState(false);
  // Раскрытые разделы панели — только на время работы; при запуске все свёрнуты.
  // Раскрытые разделы — свои у каждого набора настроек: «all» — общий
  // для файлов на общих настройках, id файла — у файла со своими.
  // Только на время работы; при запуске всё свёрнуто.
  const [sectionsBy, setSectionsBy] = useState<Record<string, Partial<Record<SectionId, boolean>>>>({});
  // Вставка настроек текстом: открыто ли окно и что в поле.
  const [importing, setImporting] = useState<{ text: string } | null>(null);
  const [conflict, setConflict] = useState<ConflictAsk | null>(null);
  const [browserMode, setBrowserMode] = useState(false);
  const [version, setVersion] = useState("");
  // Выгрузка в Telegram: состояние читается без сети.
  const [tg, setTg] = useState<TgStatus>({ enabled: false, bot: null, owner: null });
  const [publishing, setPublishing] = useState<{ source: PublishSource; kind: SetKind } | null>(null);
  const pasted = useRef(0);
  const t = strings[prefs.lang];

  useEffect(() => savePrefs(prefs), [prefs]);

  // Тема: явная — атрибутом, системная — по prefers-color-scheme.
  useEffect(() => {
    const root = document.documentElement;
    if (prefs.theme === "system") delete root.dataset.theme;
    else root.dataset.theme = prefs.theme;
    root.lang = prefs.lang;
  }, [prefs.theme, prefs.lang]);

  const addPaths = useCallback(
    async (paths: string[]) => {
      if (!backend.isTauri()) {
        add(backend.SAMPLE_PATHS.map((p) => ({ name: p.split("/").pop()!, source: { kind: "path", path: p } })));
        return;
      }
      const entries = await backend.expandPaths(paths, prefs.recursive);
      const ids = add(entries.map((e) => ({ name: e.name, source: { kind: "path", path: e.path } })));
      if (ids.length > 0) setSelectedId((s) => s ?? ids[0]);
    },
    [add, prefs.recursive],
  );
  const addFiles = () => void backend.pickPaths("files").then(addPaths);
  const addFolder = () => void backend.pickPaths("folder").then(addPaths);

  // Файлы, переданные при запуске, и демонстрационные данные в браузере.
  useEffect(() => {
    void backend.getAppInfo().then((info) => {
      setBrowserMode(info.runtime === "browser");
      setVersion(info.version);
    });
    void backend.tgStatus().then(setTg);
    void backend.startupPaths().then((paths) => {
      if (paths.length > 0) void addPaths(paths);
    });
    if (!backend.isTauri() && new URLSearchParams(location.search).has("demo")) void addPaths([]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Перетаскивание файлов в окно.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void backend
      .onFileDrop({ over: setDragActive, drop: (paths) => void addPaths(paths) })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  }, [addPaths]);

  // Вставка SVG-кода из буфера.
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && /^(INPUT|TEXTAREA)$/.test(target.tagName)) return;
      const text = e.clipboardData?.getData("text/plain") ?? "";
      if (!/<svg[\s>]/i.test(text)) return;
      pasted.current += 1;
      const ids = add([{ name: `${t.clipboardName(pasted.current)}.svg`, source: { kind: "text", text } }]);
      if (ids[0]) setSelectedId(ids[0]);
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, [add, t]);

  // Выбранный файл всегда существует, если список не пуст.
  useEffect(() => {
    if (items.length === 0) setSelectedId(null);
    else if (!items.some((it) => it.id === selectedId)) setSelectedId(items[0].id);
  }, [items, selectedId]);
  // Выбор — только среди файлов списка, главный — всегда в выборе.
  useEffect(() => {
    setSelectedIds((old) => {
      const ids = new Set([...old].filter((id) => items.some((it) => it.id === id)));
      if (selectedId) ids.add(selectedId);
      return ids.size === old.size && [...ids].every((id) => old.has(id)) ? old : ids;
    });
  }, [items, selectedId]);

  const onSelect = (id: string, how: { add: boolean; range: boolean }, shown: string[]) => {
    if (how.range && anchor.current && shown.includes(anchor.current)) {
      const [a, b] = [shown.indexOf(anchor.current), shown.indexOf(id)].sort((x, y) => x - y);
      setSelectedIds(new Set(shown.slice(a, b + 1)));
      setSelectedId(id);
      return;
    }
    anchor.current = id;
    if (how.add) {
      const next = new Set(selectedIds);
      if (next.has(id) && next.size > 1) {
        next.delete(id);
        if (id === selectedId) setSelectedId([...next][0]);
      } else {
        next.add(id);
        setSelectedId(id);
      }
      setSelectedIds(next);
      return;
    }
    setSelectedIds(new Set([id]));
    setSelectedId(id);
  };

  const selected = items.find((it) => it.id === selectedId) ?? null;
  const chosen = items.filter((it) => selectedIds.has(it.id));

  /* --------------------------- Отмена правок --------------------------- */
  const snapshot = (): Snapshot => ({
    global: prefs.options,
    own: items.map((it) => [it.id, it.options]),
  });
  const history = useRef<{ undo: Snapshot[]; redo: Snapshot[]; last: number }>({ undo: [], redo: [], last: 0 });
  /** Запомнить состояние перед правкой (частые правки — один шаг). */
  const remember = () => {
    const h = history.current;
    const now = Date.now();
    if (now - h.last > UNDO_GROUP_MS) {
      h.undo.push(snapshot());
      if (h.undo.length > UNDO_DEPTH) h.undo.shift();
      h.redo = [];
    }
    h.last = now;
  };
  const restore = (s: Snapshot) => {
    patch({ options: s.global });
    const known = new Set(items.map((it) => it.id));
    const own = s.own.filter(([id]) => known.has(id));
    const changed = own.filter(([id, o]) => items.find((it) => it.id === id)?.options !== o);
    setOptionsFor(changed);
  };
  const undo = () => {
    const h = history.current;
    const s = h.undo.pop();
    if (!s) return;
    h.redo.push(snapshot());
    h.last = 0;
    restore(s);
  };
  const redo = () => {
    const h = history.current;
    const s = h.redo.pop();
    if (!s) return;
    h.undo.push(snapshot());
    h.last = 0;
    restore(s);
  };
  const undoRef = useRef({ undo, redo });
  undoRef.current = { undo, redo };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key !== "z" && key !== "y" && e.code !== "KeyZ" && e.code !== "KeyY") return;
      // В текстовых полях — их собственная отмена.
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "TEXTAREA" || (el.tagName === "INPUT" && (el as HTMLInputElement).type === "text"))) return;
      e.preventDefault();
      const isRedo = e.code === "KeyY" || key === "y" || e.shiftKey;
      if (isRedo) undoRef.current.redo();
      else undoRef.current.undo();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Панель правит общие настройки или свои настройки выбранных файлов.
  const scope: Scope = selected?.options ? "one" : "all";
  const panelOptions = selected ? effectiveOptions(selected, prefs.options) : prefs.options;
  /** Правка панели уходит в выбранные файлы: изменённые ключи — в настройки
   *  каждого из них (у одного файла — все настройки целиком). */
  const applyToChosen = (o: Options) => {
    if (chosen.length <= 1 && selected) return setItemOptions(selected.id, o);
    const keys = changedKeys(panelOptions, o);
    setOptionsFor(
      chosen.map((it) => {
        const own: Options = { ...effectiveOptions(it, prefs.options) };
        for (const k of keys) (own as unknown as Record<string, unknown>)[k] = o[k];
        return [it.id, own];
      }),
    );
  };
  const onPanelOptions = (o: Options) => {
    remember();
    if (scope === "one" && selected) return applyToChosen(o);
    // Трансформация и положение X/Y почти всегда свои у каждого рисунка:
    // если файлов несколько и правятся они, правка уходит только в выбранные
    // файлы, у них появляются свои настройки, а уведомление позволяет отменить.
    if (selected && items.length > 1 && !sameKeys(o, prefs.options, OWN_KEYS)) {
      const ids = chosen.length > 0 ? chosen.map((it) => it.id) : [selected.id];
      applyToChosen(o);
      setToast({
        text: chosen.length > 1 ? t.ownSettingsToastMany(chosen.length) : t.ownSettingsToast(selected.name),
        action: { label: t.undo, run: () => setOptionsFor(ids.map((id) => [id, undefined])) },
        timeout: UNDO_MS,
      });
      return;
    }
    patch({ options: o });
  };
  const onScope = (s: Scope) => {
    if (!selected || s === scope) return;
    remember();
    const targets = chosen.length > 0 ? chosen : [selected];
    setOptionsFor(targets.map((it) => [it.id, s === "one" ? { ...prefs.options } : undefined]));
  };
  /** Изменён ли рисунок настройками (по сравнению с «как есть»). */
  const modified = !sameKeys(panelOptions, DEFAULT_OPTIONS, DRAWING_KEYS);

  // Для ожидания конвертации при экспорте — всегда свежий список.
  const itemsRef = useRef<FileItem[]>(items);
  itemsRef.current = items;

  /**
   * Экспорт файлов `ids`: сначала дожидается конвертации, файлы с ошибкой
   * пропускает (и сообщает, сколько), про уже существующие — спрашивает.
   */
  const runExport = async (ids: string[]) => {
    if (ids.length === 0) return;
    setExporting(true);
    try {
      const pick = () => itemsRef.current.filter((it) => ids.includes(it.id));
      if (pick().some((it) => it.status === "pending")) {
        setToast({ text: t.exportWait });
        while (pick().some((it) => it.status === "pending")) await new Promise((r) => setTimeout(r, 150));
        setToast(null);
      }
      const list = pick();
      const ready = list.filter((it) => it.status === "ok" || it.status === "warn");
      const broken = list.length - ready.length;
      if (ready.length === 0) {
        setToast({ text: [t.saved(0), t.exportErrors(broken)].join(", ") });
        return;
      }
      const ex = prefs.export;
      // SVG из буфера лежит не в папке: если папка экспорта не задана,
      // для таких файлов её нужно выбрать.
      let pastedDir: string | null = null;
      if (!ex.dir && ready.some((it) => it.source.kind === "text")) {
        pastedDir = await backend.pickFolder(undefined, t.pastedNeedFolder);
      }
      const requests: SaveRequest[] = ready.map((it) => ({
        id: it.id,
        name: exportName(it.name, ex),
        dir: ex.dir ?? (it.source.kind === "text" ? pastedDir : null),
        source: it.source.kind === "path" ? it.source.path : null,
        subfolder: exportSubfolder(ex),
      }));
      // Файлы, которые уже есть: спросить по каждому (или один раз для всех).
      const existing = await backend.findExisting(requests);
      let always: ConflictChoice | null = null;
      for (const [i, ex] of existing.entries()) {
        const choice: ConflictChoice =
          always ??
          (await new Promise<{ choice: ConflictChoice; all: boolean }>((resolve) =>
            setConflict({ existing: ex, rest: existing.length - i - 1, resolve }),
          ).then(({ choice, all }) => {
            if (all) always = choice.kind === "save" ? { kind: "save", name: "" } : choice;
            return choice;
          }));
        if (choice.kind === "cancel") return;
        const req = requests.find((r) => r.id === ex.id)!;
        if (choice.kind === "overwrite") req.overwrite = true;
        if (choice.kind === "skip") requests.splice(requests.indexOf(req), 1);
        // Своё имя — только у текущего файла; при «ко всем» — номер по порядку.
        if (choice.kind === "save") req.name = choice.name || ex.suggested;
      }
      setConflict(null);
      const results: Saved[] = await backend.save(requests);
      const n = (s: Saved["status"]) => results.filter((r) => r.status === s).length;
      // Сохранено — файлов, с нарезкой это все клетки.
      const files = results.filter((r) => r.status === "saved").reduce((sum, r) => sum + Math.max(1, r.count ?? 1), 0);
      const parts = [t.saved(files)];
      if (n("skipped")) parts.push(t.skipped(n("skipped")));
      if (n("failed")) parts.push(t.failed(n("failed")));
      if (broken) parts.push(t.exportErrors(broken));
      // «Открыть папку» — когда всё легло в одну папку.
      const folders = new Set(results.filter((r) => r.path).map((r) => dirname(r.path!)));
      const folder = folders.size === 1 && backend.isTauri() ? [...folders][0] : null;
      // Сводка пропадает при первом же новом действии.
      setToast({
        text: parts.join(", "),
        action: folder ? { label: t.openFolder, run: () => void backend.openFolder(folder) } : undefined,
        dismissOnAction: true,
      });
    } finally {
      setConflict(null);
      setExporting(false);
    }
  };

  // Набор настроек на панели: общий или свой у выбранного файла.
  const contextKey = scope === "one" && selected ? selected.id : "all";
  // Свой набор файла начинается с раскрытости общих.
  const sections = sectionsBy[contextKey] ?? sectionsBy.all ?? {};
  const onSection = (id: SectionId, open: boolean) =>
    setSectionsBy((all) => ({ ...all, [contextKey]: { ...(all[contextKey] ?? all.all ?? {}), [id]: open } }));

  /** К чему применится импорт настроек — для подписи в окне. */
  const importTarget =
    scope !== "one" || !selected
      ? t.importTargetAll
      : chosen.length > 1
        ? t.importTargetMany(chosen.length)
        : t.importTargetOwn(selected.name);
  const applyImported = (o: Options) => {
    onPanelOptions(o);
    setImporting(null);
    setToast({ text: t.settingsApplied, dismissOnAction: true });
  };
  const loadSettings = async () => {
    try {
      const text = await backend.importOptionsFile();
      if (text == null) return;
      const parsed = await backend.parseOptions(text);
      // Ошибку в файле показывает окно вставки — с текстом файла и местом.
      if ("options" in parsed) applyImported(parsed.options);
      else setImporting({ text });
    } catch {
      setToast({ text: t.importErr_too_large, dismissOnAction: true });
    }
  };
  const copySettings = async () => {
    await navigator.clipboard.writeText(await backend.optionsJson(panelOptions));
    setToast({ text: t.copied, dismissOnAction: true });
  };
  const saveSettings = async () => {
    if (await backend.exportOptionsFile(panelOptions, t.optionsFileName))
      setToast({ text: t.settingsSaved, dismissOnAction: true });
  };

  // «В Telegram»: нарезанный выбранный файл — его клетки (эмодзи),
  // иначе — все готовые файлы списка по порядку.
  const sliced =
    selected && selected.status !== "error" && selected.outcome && !selected.outcome.error && selected.outcome.grid[0] * selected.outcome.grid[1] > 1
      ? selected
      : null;
  const ready = items.filter((it) => (it.status === "ok" || it.status === "warn") && it.outcome && !it.outcome.error);
  const connected = !!tg.bot && !!tg.owner;
  const publish = tg.enabled
    ? {
        title: !connected ? t.tgPublishNoBot : sliced ? t.tgPublishGrid(sliced.name) : t.tgPublishList,
        onClick:
          connected && (sliced || ready.length > 0)
            ? () =>
                setPublishing(
                  sliced && sliced.outcome
                    ? { source: { kind: "grid", item: sliced, cols: sliced.outcome.grid[0], rows: sliced.outcome.grid[1] }, kind: "emoji" }
                    : { source: { kind: "list", items: ready }, kind: prefs.previewKind === "emoji" ? "emoji" : "stickers" },
                )
            : null,
      }
    : null;

  const exportPanel = (
    <ExportPanel
      // Панель не пересоздаётся при смене файла или области: прокрутка
      // остаётся на месте, запомненные цвета берутся по contextKey.
      contextKey={contextKey}
      aspect={selected?.outcome?.aspect ?? 1}
      onPasteSettings={() => setImporting({ text: "" })}
      onLoadSettings={() => void loadSettings()}
      onCopySettings={() => void copySettings()}
      onSaveSettings={() => void saveSettings()}
      lang={prefs.lang}
      options={panelOptions}
      onOptions={onPanelOptions}
      resetTo={scope === "one" ? prefs.options : DEFAULT_OPTIONS}
      scope={scope}
      onScope={onScope}
      selectedName={selected?.name ?? null}
      selectedCount={Math.max(chosen.length, selected ? 1 : 0)}
      actualGrid={selected?.outcome && !selected.outcome.error ? selected.outcome.grid : null}
      exportPrefs={prefs.export}
      onExportPrefs={(e) => patch({ export: e })}
      sections={sections}
      onSection={onSection}
      onCrop={() => selected && setCropping(true)}
      totalCount={items.length}
      exporting={exporting}
      onExport={() => void runExport(items.map((it) => it.id))}
      onExportOne={selected && selected.status !== "error" ? () => void runExport([selected.id]) : null}
      publish={publish}
      onChooseDir={() =>
        void backend
          .pickFolder(prefs.export.dir ?? undefined)
          .then((d) => d && patch({ export: { ...prefs.export, dir: d } }))
      }
    />
  );

  return (
    <div className={`app${dragActive ? " app--drag" : ""}`}>
      <TopBar lang={prefs.lang} onSettings={() => setSettingsOpen(true)} />

      {items.length === 0 ? (
        <div className="layout layout--empty">
          <EmptyState
            lang={prefs.lang}
            recursive={prefs.recursive}
            onRecursive={(recursive) => patch({ recursive })}
            onAddFiles={addFiles}
            onAddFolder={addFolder}
          />
          {exportPanel}
        </div>
      ) : (
        <div className="layout">
          <FileList
            lang={prefs.lang}
            items={items}
            selectedId={selectedId}
            selectedIds={selectedIds}
            onlyProblems={onlyProblems}
            onToggleProblems={setOnlyProblems}
            onSelect={onSelect}
            onRemove={(id) => remove([id])}
            onClear={clear}
            onAddFiles={addFiles}
            onAddFolder={addFolder}
          />
          <Preview
            lang={prefs.lang}
            item={selected}
            kind={prefs.previewKind}
            onKind={(previewKind) => patch({ previewKind })}
            modified={modified}
            mode={prefs.previewMode}
            bg={prefs.previewBg}
            onMode={(previewMode) =>
              patch({
                previewMode,
                previewBg: previewMode === "chat" && prefs.previewBg === "checker" ? "light" : prefs.previewBg,
              })
            }
            onBg={(previewBg) => patch({ previewBg })}
            cropEditor={
              cropping && selected ? (
                <CropEditor
                  lang={prefs.lang}
                  id={selected.id}
                  options={panelOptions}
                  onOptions={(patchO) => onPanelOptions({ ...panelOptions, ...patchO })}
                  onDone={() => setCropping(false)}
                />
              ) : undefined
            }
          />
          {exportPanel}
        </div>
      )}

      {dragActive && (
        <div className="drop-overlay" aria-hidden="true">
          <span>{t.dropRelease}</span>
        </div>
      )}

      {settingsOpen && (
        <SettingsDialog
          lang={prefs.lang}
          theme={prefs.theme}
          version={version}
          tg={tg}
          onTg={setTg}
          onApply={(lang, theme) => {
            patch({ lang, theme });
            setSettingsOpen(false);
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}

      {conflict && (
        <ConflictDialog
          key={conflict.existing.id}
          lang={prefs.lang}
          existing={conflict.existing}
          rest={conflict.rest}
          onChoose={(choice, all) => conflict.resolve({ choice, all })}
        />
      )}

      {importing && (
        <ImportDialog
          lang={prefs.lang}
          target={importTarget}
          initial={importing.text}
          onApply={(parsed) => applyImported(parsed.options)}
          onClose={() => setImporting(null)}
        />
      )}

      {publishing && tg.bot && (
        <PublishDialog
          lang={prefs.lang}
          status={tg}
          source={publishing.source}
          defaultKind={publishing.kind}
          onToast={(text) => setToast({ text, dismissOnAction: true })}
          onClose={() => setPublishing(null)}
        />
      )}

      {toast && <Toast toast={toast} closeLabel={t.close} onClose={() => setToast(null)} />}

      {browserMode && <div className="badge-browser">{t.browserMode}</div>}
    </div>
  );
}

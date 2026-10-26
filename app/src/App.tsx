import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as backend from "./backend";
import { EmptyState } from "./components/EmptyState";
import { ExportPanel, type Scope } from "./components/ExportPanel";
import { FileList } from "./components/FileList";
import { Preview } from "./components/Preview";
import { SettingsDialog } from "./components/SettingsDialog";
import { TopBar } from "./components/TopBar";
import { strings } from "./i18n";
import { exportName, exportSubfolder, loadPrefs, savePrefs, type Prefs } from "./prefs";
import type { Options, SaveRequest, Saved } from "./types";
import { effectiveOptions, useFiles } from "./useFiles";
import "./App.css";

interface Toast {
  text: string;
  folder: string | null;
}

/** Папка файла по его пути (оба вида разделителей). */
const dirname = (path: string) => path.replace(/[\\/][^\\/]*$/, "");

export function App() {
  const [prefs, setPrefs] = useState<Prefs>(loadPrefs);
  const patch = (p: Partial<Prefs>) => setPrefs((old) => ({ ...old, ...p }));
  const { items, add, remove, clear, setItemOptions } = useFiles(prefs.options);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [onlyProblems, setOnlyProblems] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const [browserMode, setBrowserMode] = useState(false);
  const [version, setVersion] = useState("");
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

  const selected = items.find((it) => it.id === selectedId) ?? null;
  const ready = useMemo(() => items.filter((it) => it.status === "ok" || it.status === "warn"), [items]);

  // Панель правит общие настройки или свои настройки выбранного файла.
  const scope: Scope = selected?.options ? "one" : "all";
  const panelOptions = selected ? effectiveOptions(selected, prefs.options) : prefs.options;
  const onPanelOptions = (o: Options) => {
    if (scope === "one" && selected) setItemOptions(selected.id, o);
    else patch({ options: o });
  };
  const onScope = (s: Scope) => {
    if (!selected || s === scope) return;
    setItemOptions(selected.id, s === "one" ? { ...prefs.options } : undefined);
  };

  const runExport = async () => {
    if (ready.length === 0) return;
    setExporting(true);
    try {
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
      const results: Saved[] = await backend.save(requests, prefs.conflict);
      const n = (s: Saved["status"]) => results.filter((r) => r.status === s).length;
      const parts = [t.saved(n("saved"))];
      if (n("skipped")) parts.push(t.skipped(n("skipped")));
      if (n("failed")) parts.push(t.failed(n("failed")));
      // «Открыть папку» — когда всё легло в одну папку.
      const folders = new Set(results.filter((r) => r.path).map((r) => dirname(r.path!)));
      setToast({ text: parts.join(", "), folder: folders.size === 1 ? [...folders][0] : null });
    } finally {
      setExporting(false);
    }
  };

  const exportPanel = (
    <ExportPanel
      // Новый файл или область — панель заново берёт из настроек
      // запомненные цвет фона и параметры обводки.
      key={scope === "one" ? selected?.id : "all"}
      lang={prefs.lang}
      options={panelOptions}
      onOptions={onPanelOptions}
      scope={scope}
      onScope={onScope}
      selectedName={selected?.name ?? null}
      exportPrefs={prefs.export}
      onExportPrefs={(e) => patch({ export: e })}
      readyCount={ready.length}
      exporting={exporting}
      onExport={() => void runExport()}
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
            onlyProblems={onlyProblems}
            onToggleProblems={setOnlyProblems}
            onSelect={setSelectedId}
            onRemove={(id) => remove([id])}
            onClear={clear}
            onAddFiles={addFiles}
            onAddFolder={addFolder}
          />
          <Preview
            lang={prefs.lang}
            item={selected}
            preset={panelOptions.preset}
            mode={prefs.previewMode}
            bg={prefs.previewBg}
            onMode={(previewMode) =>
              patch({
                previewMode,
                previewBg: previewMode === "chat" && prefs.previewBg === "checker" ? "light" : prefs.previewBg,
              })
            }
            onBg={(previewBg) => patch({ previewBg })}
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
          onApply={(lang, theme) => {
            patch({ lang, theme });
            setSettingsOpen(false);
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}

      {toast && (
        <div className="toast" role="status">
          <span>{toast.text}</span>
          {toast.folder && backend.isTauri() && (
            <button type="button" className="btn btn--link" onClick={() => void backend.openFolder(toast.folder!)}>
              {t.openFolder}
            </button>
          )}
          <button type="button" className="toast__close" aria-label="×" onClick={() => setToast(null)}>
            ×
          </button>
        </div>
      )}

      {browserMode && <div className="badge-browser">{t.browserMode}</div>}
    </div>
  );
}

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as backend from "./backend";
import { EmptyState } from "./components/EmptyState";
import { FileList } from "./components/FileList";
import { Preview } from "./components/Preview";
import { SaveDialog } from "./components/SaveDialog";
import { SettingsPanel } from "./components/SettingsPanel";
import { TopBar } from "./components/TopBar";
import { strings } from "./i18n";
import { applyTemplate, loadPrefs, savePrefs, type Prefs } from "./prefs";
import type { Saved } from "./types";
import { useFiles } from "./useFiles";
import "./App.css";

interface Toast {
  text: string;
  folder: string | null;
}

export function App() {
  const [prefs, setPrefs] = useState<Prefs>(loadPrefs);
  const patch = (p: Partial<Prefs>) => setPrefs((old) => ({ ...old, ...p }));
  const { items, add, remove, clear } = useFiles(prefs.options);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [onlyProblems, setOnlyProblems] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [saving, setSaving] = useState(false);
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

  const openSave = async () => {
    if (ready.length === 0) return;
    if (!prefs.outputDir) {
      const first = items.find((it) => it.source.kind === "path");
      const dir = first && first.source.kind === "path" ? await backend.defaultOutputDir(first.source.path) : null;
      if (dir) patch({ outputDir: dir });
    }
    setSaving(true);
  };

  const doSave = async () => {
    if (!prefs.outputDir) return;
    const requests = ready.map((it) => ({ id: it.id, name: applyTemplate(prefs.nameTemplate, it.name) }));
    const results: Saved[] = await backend.save(requests, prefs.outputDir, prefs.conflict);
    setSaving(false);
    const n = (s: Saved["status"]) => results.filter((r) => r.status === s).length;
    const parts = [t.saved(n("saved"))];
    if (n("skipped")) parts.push(t.skipped(n("skipped")));
    if (n("failed")) parts.push(t.failed(n("failed")));
    setToast({ text: parts.join(", "), folder: prefs.outputDir });
  };

  return (
    <div className={`app${dragActive ? " app--drag" : ""}`}>
      <TopBar
        lang={prefs.lang}
        readyCount={ready.length}
        onAddFiles={() => void backend.pickPaths("files").then(addPaths)}
        onAddFolder={() => void backend.pickPaths("folder").then(addPaths)}
        onSave={() => void openSave()}
      />

      {items.length === 0 ? (
        <div className="layout layout--empty">
          <EmptyState
            lang={prefs.lang}
            recursive={prefs.recursive}
            onRecursive={(recursive) => patch({ recursive })}
            onAddFiles={() => void backend.pickPaths("files").then(addPaths)}
            onAddFolder={() => void backend.pickPaths("folder").then(addPaths)}
          />
          <SettingsPanel
            lang={prefs.lang}
            theme={prefs.theme}
            options={prefs.options}
            onOptions={(options) => patch({ options })}
            onLang={(lang) => patch({ lang })}
            onTheme={(theme) => patch({ theme })}
            version={version}
          />
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
          />
          <Preview
            lang={prefs.lang}
            item={selected}
            preset={prefs.options.preset}
            mode={prefs.previewMode}
            bg={prefs.previewBg}
            onMode={(previewMode) => patch({ previewMode, previewBg: previewMode === "chat" && prefs.previewBg === "checker" ? "light" : prefs.previewBg })}
            onBg={(previewBg) => patch({ previewBg })}
          />
          <SettingsPanel
            lang={prefs.lang}
            theme={prefs.theme}
            options={prefs.options}
            onOptions={(options) => patch({ options })}
            onLang={(lang) => patch({ lang })}
            onTheme={(theme) => patch({ theme })}
            version={version}
          />
        </div>
      )}

      {dragActive && (
        <div className="drop-overlay" aria-hidden="true">
          <span>{t.dropRelease}</span>
        </div>
      )}

      {saving && (
        <SaveDialog
          lang={prefs.lang}
          count={ready.length}
          dir={prefs.outputDir}
          template={prefs.nameTemplate}
          conflict={prefs.conflict}
          onChooseDir={() =>
            void backend.pickFolder(prefs.outputDir ?? undefined).then((d) => d && patch({ outputDir: d }))
          }
          onTemplate={(nameTemplate) => patch({ nameTemplate })}
          onConflict={(conflict) => patch({ conflict })}
          onCancel={() => setSaving(false)}
          onSave={() => void doSave()}
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

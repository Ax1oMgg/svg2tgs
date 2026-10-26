import { useState } from "react";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { ExportPrefs, Toggle } from "../prefs";
import { DEFAULT_OUTLINE, MAX_OUTLINE, type Options, type Outline } from "../types";
import { SegButton } from "./Preview";

export type Scope = "all" | "one";

interface Props {
  lang: Lang;
  /** Настройки, которые сейчас правятся: общие или выбранного файла. */
  options: Options;
  onOptions: (o: Options) => void;
  scope: Scope;
  onScope: (s: Scope) => void;
  /** Имя выбранного файла; `null` — файлов нет, «этого файла» недоступно. */
  selectedName: string | null;
  exportPrefs: ExportPrefs;
  onExportPrefs: (e: ExportPrefs) => void;
  /** Сколько файлов готово к экспорту. */
  readyCount: number;
  exporting: boolean;
  onExport: () => void;
  onChooseDir: () => void;
}

/**
 * Панель экспорта: куда и под каким именем сохранить, затем настройки
 * конвертации — общие для всех файлов или свои для выбранного.
 */
export function ExportPanel(p: Props) {
  const t = strings[p.lang];
  const { options, exportPrefs: ex } = p;
  const set = (patch: Partial<Options>) => p.onOptions({ ...options, ...patch });
  const setEx = (patch: Partial<ExportPrefs>) => p.onExportPrefs({ ...ex, ...patch });
  const [color, setColor] = useState(options.background.kind === "solid" ? options.background.color : "#ffffff");
  // Толщина и цвет обводки помнятся, пока она выключена.
  const [outline, setOutline] = useState<Outline>(options.outline ?? DEFAULT_OUTLINE);
  const outlineOn = !!options.outline;
  const shownOutline = options.outline ?? outline;
  const changeOutline = (patch: Partial<Outline>) => {
    const next = { ...shownOutline, ...patch };
    setOutline(next);
    set({ outline: next });
  };
  const pad = Math.round(options.padding * 100);
  const setPad = (v: number) => set({ padding: Math.max(0, Math.min(20, Math.round(v) || 0)) / 100 });

  const naming: [keyof Pick<ExportPrefs, "suffix" | "prefix" | "subfolder">, string, string][] = [
    ["suffix", t.suffix, t.suffixHint],
    ["prefix", t.prefix, t.prefixHint],
    ["subfolder", t.subfolder, t.subfolderHint],
  ];

  return (
    <aside className="export" aria-label={t.exportPanel}>
      <h2 className="panel-title">{t.exportPanel}</h2>
      <button
        type="button"
        className="btn btn--primary btn--wide export__go"
        disabled={p.readyCount === 0 || p.exporting}
        onClick={p.onExport}
      >
        {t.export}
      </button>

      <div className="row export__dest">
        {ex.dir ? (
          <output className="path" title={ex.dir}>
            <span dir="ltr">{ex.dir}</span>
          </output>
        ) : (
          <output className="path path--placeholder" title={t.toSourceHint}>
            <span dir="ltr">{t.toSource}</span>
          </output>
        )}
        {ex.dir && (
          <button
            type="button"
            className="btn btn--text btn--icon export__reset"
            aria-label={t.toSource}
            title={t.toSource}
            onClick={() => setEx({ dir: null })}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
              <path d="M6 6l12 12M18 6 6 18" />
            </svg>
          </button>
        )}
        <button type="button" className="btn btn--default" onClick={p.onChooseDir}>
          {t.choose}
        </button>
      </div>

      <div className="export__naming">
        {naming.map(([key, label, hint]) => {
          const f: Toggle = ex[key];
          return (
            <div key={key} className="opt-row">
              <label className="opt-row__label">
                <input type="checkbox" checked={f.on} onChange={() => setEx({ [key]: { ...f, on: !f.on } })} />
                {label}
              </label>
              <input
                aria-label={label}
                value={f.value}
                placeholder={hint}
                disabled={!f.on}
                onChange={(e) => setEx({ [key]: { ...f, value: e.target.value } })}
              />
            </div>
          );
        })}
      </div>

      <div className="export__scope">
        <div className="field__label">{t.exportSettings}</div>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.exportSettings}>
          <SegButton active={p.scope === "all"} onClick={() => p.onScope("all")}>
            {t.scopeAll}
          </SegButton>
          <SegButton active={p.scope === "one"} onClick={() => p.selectedName && p.onScope("one")}>
            {t.scopeOne}
          </SegButton>
        </div>
        {p.scope === "one" && p.selectedName && (
          <p className="hint">{t.scopeOneHint(p.selectedName)}</p>
        )}
      </div>

      <div className="segmented segmented--wide" role="radiogroup" aria-label={t.preset}>
        <SegButton active={options.preset === "sticker"} onClick={() => set({ preset: "sticker" })}>
          {t.sticker}
        </SegButton>
        <SegButton active={options.preset === "emoji"} onClick={() => set({ preset: "emoji" })}>
          {t.emoji}
        </SegButton>
      </div>

      <div className="field field--slider">
        <div className="field__label">
          <label htmlFor="padding">{t.padding}</label>
          <span className="opt-row__unit">
            <input
              className="num"
              type="number"
              min={0}
              max={20}
              value={pad}
              onChange={(e) => setPad(Number(e.target.value))}
              aria-label={`${t.padding}, %`}
            />
            %
          </span>
        </div>
        <input
          id="padding"
          type="range"
          min={0}
          max={20}
          step={1}
          value={pad}
          onChange={(e) => setPad(Number(e.target.value))}
          style={{ "--fill": `${(pad / 20) * 100}%` } as React.CSSProperties}
        />
      </div>

      <div className="opt-row">
        <label className="opt-row__label">
          <input
            type="checkbox"
            checked={outlineOn}
            onChange={() => set({ outline: outlineOn ? null : shownOutline })}
          />
          {t.outline}
        </label>
        <span className="opt-row__unit">
          <input
            className="num"
            type="number"
            min={1}
            max={MAX_OUTLINE}
            value={shownOutline.width}
            disabled={!outlineOn}
            onChange={(e) =>
              changeOutline({ width: Math.max(1, Math.min(MAX_OUTLINE, Math.round(Number(e.target.value)) || 1)) })
            }
            aria-label={t.outlineWidth}
          />
          px
        </span>
        <input
          type="color"
          value={shownOutline.color}
          disabled={!outlineOn}
          onChange={(e) => changeOutline({ color: e.target.value })}
          aria-label={t.outlineColor}
          title={t.outlineColor}
        />
      </div>

      <div className="field">
        <label className="field__label" htmlFor="fit">
          {t.fit}
        </label>
        <select id="fit" value={options.fit} onChange={(e) => set({ fit: e.target.value as Options["fit"] })}>
          <option value="contain">{t.fitContain}</option>
          <option value="cover">{t.fitCover}</option>
        </select>
      </div>

      <div className="field">
        <label className="field__label" htmlFor="background">
          {t.background}
        </label>
        <div className="row">
          <select
            id="background"
            value={options.background.kind}
            onChange={(e) =>
              set({
                background: e.target.value === "solid" ? { kind: "solid", color } : { kind: "transparent" },
              })
            }
          >
            <option value="transparent">{t.transparent}</option>
            <option value="solid">{t.color}</option>
          </select>
          {options.background.kind === "solid" && (
            <input
              type="color"
              value={color}
              aria-label={t.color}
              onChange={(e) => {
                setColor(e.target.value);
                set({ background: { kind: "solid", color: e.target.value } });
              }}
            />
          )}
        </div>
      </div>

      <div className="field">
        <label className="field__label" htmlFor="optimization">
          {t.optimization}
        </label>
        <select
          id="optimization"
          value={options.optimization.mode}
          onChange={(e) => {
            const mode = e.target.value as Options["optimization"]["mode"];
            set({
              optimization: mode === "manual" ? { mode, precision: 2 } : ({ mode } as Options["optimization"]),
            });
          }}
        >
          <option value="auto">{t.optAuto}</option>
          <option value="lossless">{t.optLossless}</option>
          <option value="manual">{t.optManual}</option>
        </select>
        {options.optimization.mode === "manual" && (
          <label className="sub">
            {t.decimals}
            <input
              type="number"
              min={0}
              max={6}
              value={options.optimization.precision}
              onChange={(e) =>
                set({
                  optimization: {
                    mode: "manual",
                    precision: Math.max(0, Math.min(6, Number(e.target.value) || 0)),
                  },
                })
              }
            />
          </label>
        )}
      </div>

      <details className="advanced">
        <summary>{t.advanced}</summary>
        <div className="field">
          <label className="field__label" htmlFor="fps">
            {t.fps}
          </label>
          <input
            id="fps"
            type="number"
            min={1}
            max={120}
            placeholder={`60 — ${t.telegramDefault}`}
            value={options.fps ?? ""}
            onChange={(e) => set({ fps: e.target.value ? Number(e.target.value) : null })}
          />
        </div>
        <div className="field">
          <label className="field__label" htmlFor="duration">
            {t.duration}
          </label>
          <input
            id="duration"
            type="number"
            min={0.1}
            max={10}
            step={0.1}
            placeholder="1"
            value={options.duration_secs ?? ""}
            onChange={(e) => set({ duration_secs: e.target.value ? Number(e.target.value) : null })}
          />
        </div>
      </details>
    </aside>
  );
}

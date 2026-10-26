import { useState } from "react";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { ExportPrefs, Toggle } from "../prefs";
import { DEFAULT_BORDER, DEFAULT_OUTLINE, MAX_OUTLINE, MAX_RADIUS, type Options, type Outline } from "../types";
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
  const [border, setBorder] = useState<Outline>(options.background_border ?? DEFAULT_BORDER);
  const borderOn = !!options.background_border;
  const shownBorder = options.background_border ?? border;
  const changeBorder = (patch: Partial<Outline>) => {
    const next = { ...shownBorder, ...patch };
    setBorder(next);
    set({ background_border: next });
  };
  const radius = Math.round(options.background_radius ?? 0);
  const pad = Math.round(options.padding * 100);
  const setPad = (v: number) => set({ padding: Math.max(0, Math.min(20, Math.round(v) || 0)) / 100 });

  const naming: [keyof Pick<ExportPrefs, "suffix" | "prefix" | "subfolder">, string][] = [
    ["suffix", t.suffix],
    ["prefix", t.prefix],
    ["subfolder", t.subfolder],
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
        {naming.map(([key, label]) => {
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

      <details className="advanced">
        <summary>{t.object}</summary>
        <div className="section">
          <SliderField
            id="padding"
            label={t.padding}
            unit="%"
            max={20}
            value={pad}
            onChange={setPad}
          />
          <ColorRow
            label={t.outline}
            on={outlineOn}
            onToggle={() => set({ outline: outlineOn ? null : shownOutline })}
            width={shownOutline.width}
            onWidth={(width) => changeOutline({ width })}
            color={shownOutline.color}
            onColor={(color) => changeOutline({ color })}
            widthLabel={t.outlineWidth}
            colorLabel={t.outlineColor}
          />
        </div>
      </details>

      <details className="advanced">
        <summary>{t.background}</summary>
        <div className="section">
          <SliderField
            id="radius"
            label={t.radius}
            unit="px"
            max={MAX_RADIUS}
            value={radius}
            onChange={(v) => set({ background_radius: Math.max(0, Math.min(MAX_RADIUS, Math.round(v) || 0)) })}
          />
          <ColorRow
            label={t.fill}
            on={options.background.kind === "solid"}
            onToggle={() =>
              set({
                background: options.background.kind === "solid" ? { kind: "transparent" } : { kind: "solid", color },
              })
            }
            color={color}
            onColor={(c) => {
              setColor(c);
              set({ background: { kind: "solid", color: c } });
            }}
            colorLabel={t.fillColor}
          />
          <ColorRow
            label={t.outline}
            on={borderOn}
            onToggle={() => set({ background_border: borderOn ? null : shownBorder })}
            width={shownBorder.width}
            onWidth={(width) => changeBorder({ width })}
            color={shownBorder.color}
            onColor={(c) => changeBorder({ color: c })}
            widthLabel={t.borderWidth}
            colorLabel={t.borderColor}
          />
        </div>
      </details>

      <div className="field">
        <div className="field__label">{t.fit}</div>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.fit}>
          <SegButton active={options.fit === "contain"} onClick={() => set({ fit: "contain" })} title={t.fitContainHint}>
            {t.fitContain}
          </SegButton>
          <SegButton active={options.fit === "cover"} onClick={() => set({ fit: "cover" })} title={t.fitCoverHint}>
            {t.fitCover}
          </SegButton>
        </div>
      </div>

      <div className="field">
        <div className="field__label">{t.optimization}</div>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.optimization}>
          {(["auto", "lossless", "manual"] as const).map((mode) => (
            <SegButton
              key={mode}
              active={options.optimization.mode === mode}
              title={mode === "auto" ? t.optAutoHint : mode === "lossless" ? t.optLosslessHint : t.optManualHint}
              onClick={() =>
                set({
                  optimization: mode === "manual" ? { mode, precision: 2 } : ({ mode } as Options["optimization"]),
                })
              }
            >
              {mode === "auto" ? t.optAuto : mode === "lossless" ? t.optLossless : t.optManual}
            </SegButton>
          ))}
        </div>
        {options.optimization.mode === "manual" && (
          <label className="sub">
            {t.decimals}
            <input
              className="num"
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

/** Числовое поле с единицей и ползунок под ним (отступ, скругление). */
function SliderField(p: {
  id: string;
  label: string;
  unit: string;
  max: number;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <div className="field field--slider">
      <div className="field__label">
        <label htmlFor={p.id}>{p.label}</label>
        <span className="opt-row__unit">
          <input
            className="num"
            type="number"
            min={0}
            max={p.max}
            value={p.value}
            onChange={(e) => p.onChange(Number(e.target.value))}
            aria-label={`${p.label}, ${p.unit}`}
          />
          {p.unit}
        </span>
      </div>
      <input
        id={p.id}
        type="range"
        min={0}
        max={p.max}
        step={1}
        value={p.value}
        onChange={(e) => p.onChange(Number(e.target.value))}
        style={{ "--fill": `${(p.value / p.max) * 100}%` } as React.CSSProperties}
      />
    </div>
  );
}

/** Строка «флажок + подпись | толщина px | цвет» (обводки, заливка фона). */
function ColorRow(p: {
  label: string;
  on: boolean;
  onToggle: () => void;
  /** Без толщины — только цвет (заливка). */
  width?: number;
  onWidth?: (w: number) => void;
  color: string;
  onColor: (c: string) => void;
  widthLabel?: string;
  colorLabel: string;
}) {
  return (
    <div className="opt-row">
      <label className="opt-row__label">
        <input type="checkbox" checked={p.on} onChange={p.onToggle} />
        {p.label}
      </label>
      {p.width !== undefined && p.onWidth && (
        <span className="opt-row__unit">
          <input
            className="num"
            type="number"
            min={1}
            max={MAX_OUTLINE}
            value={p.width}
            disabled={!p.on}
            onChange={(e) => p.onWidth!(Math.max(1, Math.min(MAX_OUTLINE, Math.round(Number(e.target.value)) || 1)))}
            aria-label={p.widthLabel}
          />
          px
        </span>
      )}
      <input
        type="color"
        value={p.color}
        disabled={!p.on}
        onChange={(e) => p.onColor(e.target.value)}
        aria-label={p.colorLabel}
        title={p.colorLabel}
      />
    </div>
  );
}

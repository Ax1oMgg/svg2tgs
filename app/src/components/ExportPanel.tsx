import { useState } from "react";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { ExportPrefs, Toggle } from "../prefs";
import {
  DEFAULT_BORDER,
  DEFAULT_OPTIONS,
  DEFAULT_OUTLINE,
  MAX_RADIUS,
  SECTION_KEYS,
  resetKeys,
  sameKeys,
  type Fit,
  type Options,
  type Outline,
  type SectionId,
} from "../types";
import { ColorRow, NumField, ResetButton, Section, SliderField } from "./controls";
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
  /** Какие разделы раскрыты. */
  sections: Partial<Record<SectionId, boolean>>;
  onSection: (id: SectionId, open: boolean) => void;
  /** Сколько файлов готово к экспорту. */
  readyCount: number;
  exporting: boolean;
  onExport: () => void;
  onChooseDir: () => void;
  /** Открыть кадрирование мышью в превью. */
  onCrop: () => void;
}

const ALL_KEYS = Object.values(SECTION_KEYS).flat();

/** Угол в пределах −180…180. */
const normAngle = (a: number) => ((((a + 180) % 360) + 360) % 360) - 180;

/**
 * Панель экспорта: куда и под каким именем сохранить, затем настройки
 * конвертации — общие для всех файлов или свои для выбранного. Порядок
 * обработки рисунка: трансформация → положение → оформление → фон.
 */
export function ExportPanel(p: Props) {
  const t = strings[p.lang];
  const { options, exportPrefs: ex } = p;
  const set = (patch: Partial<Options>) => p.onOptions({ ...options, ...patch });
  const setEx = (patch: Partial<ExportPrefs>) => p.onExportPrefs({ ...ex, ...patch });
  const [color, setColor] = useState(options.background.kind === "solid" ? options.background.color : "#ffffff");
  // Толщина и цвет обводок помнятся, пока обводка выключена.
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

  const crop = options.crop ?? DEFAULT_OPTIONS.crop!;
  const cropped = crop.top + crop.right + crop.bottom + crop.left > 0;
  const rotation = options.rotation ?? 0;

  const section = (id: SectionId, title: string, children: React.ReactNode) => (
    <Section
      title={title}
      open={!!p.sections[id]}
      onToggle={(open) => p.onSection(id, open)}
      resetTitle={t.resetSection(title)}
      isDefault={sameKeys(options, DEFAULT_OPTIONS, SECTION_KEYS[id])}
      onReset={() => p.onOptions(resetKeys(options, SECTION_KEYS[id]))}
    >
      {children}
    </Section>
  );

  const naming: [keyof Pick<ExportPrefs, "suffix" | "prefix" | "subfolder">, string][] = [
    ["suffix", t.suffix],
    ["prefix", t.prefix],
    ["subfolder", t.subfolder],
  ];
  const fits: [Fit, string, string][] = [
    ["contain", t.fitContain, t.fitContainHint],
    ["width", t.fitWidth, t.fitWidthHint],
    ["height", t.fitHeight, t.fitHeightHint],
    ["free", t.fitFree, t.fitFreeHint],
  ];

  return (
    <aside className="export scroll scroll--16" aria-label={t.exportPanel}>
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
        <div className="field__label">
          <span>{t.exportSettings}</span>
          <ResetButton
            title={t.resetAll}
            disabled={sameKeys(options, DEFAULT_OPTIONS, [...ALL_KEYS, "preset", "optimization"])}
            onClick={() => p.onOptions({ ...DEFAULT_OPTIONS })}
          />
        </div>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.exportSettings}>
          <SegButton active={p.scope === "all"} onClick={() => p.onScope("all")}>
            {t.scopeAll}
          </SegButton>
          <SegButton active={p.scope === "one"} onClick={() => p.selectedName && p.onScope("one")}>
            {t.scopeOne}
          </SegButton>
        </div>
        {p.scope === "one" && p.selectedName && <p className="hint">{t.scopeOneHint(p.selectedName)}</p>}
      </div>

      <div className="segmented segmented--wide" role="radiogroup" aria-label={t.preset}>
        <SegButton active={options.preset === "sticker"} onClick={() => set({ preset: "sticker" })}>
          {t.sticker}
        </SegButton>
        <SegButton active={options.preset === "emoji"} onClick={() => set({ preset: "emoji" })}>
          {t.emoji}
        </SegButton>
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
            <NumField
              value={options.optimization.precision}
              min={0}
              max={6}
              label={t.decimals}
              onChange={(v) => set({ optimization: { mode: "manual", precision: Math.round(v) } })}
            />
          </label>
        )}
      </div>

      {section(
        "position",
        t.position,
        <>
          <div className="field">
            <div className="field__label">{t.fit}</div>
            <div className="segmented segmented--wide segmented--tight" role="radiogroup" aria-label={t.fit}>
              {fits.map(([fit, label, hint]) => (
                <SegButton key={fit} active={options.fit === fit} title={hint} onClick={() => set({ fit })}>
                  {label}
                </SegButton>
              ))}
            </div>
          </div>
          {options.fit === "free" && (
            <SliderField
              id="scale"
              label={t.scale}
              unit="%"
              min={10}
              max={400}
              value={Math.round((options.scale ?? 1) * 100)}
              onChange={(v) => set({ scale: v / 100 })}
            />
          )}
          <SliderField
            id="pos-x"
            label={t.positionX}
            unit="%"
            max={100}
            value={Math.round((options.position_x ?? 0.5) * 1000) / 10}
            decimals={1}
            onChange={(v) => set({ position_x: v / 100 })}
          />
          <SliderField
            id="pos-y"
            label={t.positionY}
            unit="%"
            max={100}
            value={Math.round((options.position_y ?? 0.5) * 1000) / 10}
            decimals={1}
            onChange={(v) => set({ position_y: v / 100 })}
          />
          <SliderField
            id="padding"
            label={t.padding}
            unit="%"
            max={20}
            value={Math.round(options.padding * 1000) / 10}
            decimals={1}
            onChange={(v) => set({ padding: v / 100 })}
          />
          <p className="hint hint--xs">{t.precisionHint}</p>
        </>,
      )}

      {section(
        "transform",
        t.transform,
        <>
          <div className="opt-row">
            <span className="opt-row__label opt-row__label--short">{t.flip}</span>
            <IconToggle pressed={!!options.flip_h} title={t.flipH} onClick={() => set({ flip_h: !options.flip_h })}>
              <path d="M12 3v18" />
              <path d="m3 7 5 5-5 5V7" />
              <path d="m21 7-5 5 5 5V7" />
            </IconToggle>
            <IconToggle pressed={!!options.flip_v} title={t.flipV} onClick={() => set({ flip_v: !options.flip_v })}>
              <path d="M3 12h18" />
              <path d="m7 3 5 5 5-5H7" />
              <path d="m7 21 5-5 5 5H7" />
            </IconToggle>
          </div>
          <div className="opt-row">
            <span className="opt-row__label opt-row__label--short">{t.rotate}</span>
            <IconToggle title={t.rotateLeft} onClick={() => set({ rotation: normAngle(rotation - 90) })}>
              <path d="M20 9V7a2 2 0 0 0-2-2h-6" />
              <path d="m15 2-3 3 3 3" />
              <path d="M20 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h2" />
            </IconToggle>
            <IconToggle title={t.rotateRight} onClick={() => set({ rotation: normAngle(rotation + 90) })}>
              <path d="M12 5H6a2 2 0 0 0-2 2v3" />
              <path d="m9 8 3-3-3-3" />
              <path d="M4 14v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2" />
            </IconToggle>
            <span className="opt-row__unit opt-row__end">
              <NumField
                value={rotation}
                min={-180}
                max={180}
                decimals={1}
                width={52}
                label={t.rotationAngle}
                onChange={(v) => set({ rotation: v })}
              />
              °
            </span>
          </div>
          <div className="opt-row">
            <span className="opt-row__label opt-row__label--short">{t.crop}</span>
            <button type="button" className="btn btn--default btn--sm" title={t.cropHint} onClick={p.onCrop}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M6 2v14a2 2 0 0 0 2 2h14" />
                <path d="M18 22V8a2 2 0 0 0-2-2H2" />
              </svg>
              {t.cropOpen}
            </button>
            {cropped && (
              <ResetButton
                title={t.cropReset}
                disabled={false}
                onClick={() => set({ crop: { top: 0, right: 0, bottom: 0, left: 0 } })}
              />
            )}
          </div>
          <SliderField
            id="aspect"
            label={t.aspect}
            unit="%"
            min={50}
            max={200}
            value={Math.round((options.aspect ?? 1) * 100)}
            onChange={(v) => set({ aspect: v / 100 })}
          />
        </>,
      )}

      {section(
        "style",
        t.styleSection,
        <ColorRow
          label={t.outline}
          on={outlineOn}
          onToggle={() => set({ outline: outlineOn ? null : shownOutline })}
          width={shownOutline.width}
          onWidth={(width) => changeOutline({ width })}
          color={shownOutline.color}
          onColor={(c) => changeOutline({ color: c })}
          widthLabel={t.outlineWidth}
          colorLabel={t.outlineColor}
        />,
      )}

      {section(
        "background",
        t.background,
        <>
          <SliderField
            id="radius"
            label={t.radius}
            unit="px"
            max={MAX_RADIUS}
            value={Math.round(options.background_radius ?? 0)}
            onChange={(v) => set({ background_radius: Math.round(v) })}
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
        </>,
      )}

      {section(
        "extra",
        t.advanced,
        <>
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
        </>,
      )}
    </aside>
  );
}

/** Квадратная кнопка-значок 32×32; `pressed` — нажатый переключатель. */
function IconToggle(p: { pressed?: boolean; title: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      className={`btn btn--default btn--icon btn--square${p.pressed ? " btn--pressed" : ""}`}
      aria-pressed={p.pressed}
      title={p.title}
      aria-label={p.title}
      onClick={p.onClick}
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        {p.children}
      </svg>
    </button>
  );
}

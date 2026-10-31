import { useEffect, useRef, useState } from "react";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { ExportPrefs, Toggle } from "../prefs";
import {
  AUTO_GRID_COLS,
  DEFAULT_BORDER,
  DEFAULT_OPTIONS,
  DEFAULT_OUTLINE,
  GRID_KEYS,
  MAX_GRID,
  MAX_RADIUS,
  SECTION_KEYS,
  sameKeys,
  type Fit,
  type Options,
  type Outline,
  type SectionId,
} from "../types";
import { ColorRow, NumField, Range, ResetButton, Section, SliderField } from "./controls";
import { SegButton } from "./Preview";

export type Scope = "all" | "one";

interface Props {
  lang: Lang;
  /** Настройки, которые сейчас правятся: общие или выбранного файла. */
  options: Options;
  onOptions: (o: Options) => void;
  /** Куда ведёт сброс: в «Всех файлов» — к умолчаниям, в своих
   *  настройках файла — к общим настройкам. */
  resetTo: Options;
  scope: Scope;
  onScope: (s: Scope) => void;
  /** Имя выбранного файла; `null` — файлов нет, «этого файла» недоступно. */
  selectedName: string | null;
  /** Сколько файлов выбрано в списке. */
  selectedCount: number;
  /** Сетка, по которой нарезан выбранный файл (подобранная, если авто). */
  actualGrid: [number, number] | null;
  /** Пропорции выбранного рисунка после трансформаций (ширина / высота). */
  aspect: number;
  /** Какой набор настроек на панели («all» или id файла): при смене
   *  панель не пересоздаётся (прокрутка на месте), а запомненные цвета
   *  и толщины берутся из нового набора. */
  contextKey: string;
  exportPrefs: ExportPrefs;
  onExportPrefs: (e: ExportPrefs) => void;
  /** Какие разделы раскрыты. */
  sections: Partial<Record<SectionId, boolean>>;
  onSection: (id: SectionId, open: boolean) => void;
  /** Сколько файлов в списке. */
  totalCount: number;
  exporting: boolean;
  onExport: () => void;
  /** Экспорт только выбранного файла; `null` — его нечем экспортировать. */
  onExportOne: (() => void) | null;
  onChooseDir: () => void;
  /** Открыть кадрирование мышью в превью. */
  onCrop: () => void;
  /** Импорт и экспорт настроек («Дополнительно»). */
  onPasteSettings: () => void;
  onLoadSettings: () => void;
  onCopySettings: () => void;
  onSaveSettings: () => void;
}

const ALL_KEYS = [...Object.values(SECTION_KEYS).flat(), ...GRID_KEYS];

/** Угол в пределах −180…180 (180 так и остаётся 180). */
const normAngle = (a: number) => {
  const n = ((((a + 180) % 360) + 360) % 360) - 180;
  return n === -180 && a > 0 ? 180 : n;
};

/** Ближайший к углу кратный 90° — середина шкалы поворота. */
const baseOf = (a: number) => Math.round(a / 90) * 90;

/** Отступ хранится долей канваса, показывается в пикселях из 512. */
const CANVAS = 512;

/** Значок «файл со стрелкой вниз» (Lucide `file-down`, ISC). */
function FileDownIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
      <path d="M14 2v4a2 2 0 0 0 2 2h4" />
      <path d="M12 18v-6" />
      <path d="m9 15 3 3 3-3" />
    </svg>
  );
}

/** Значок «волшебная палочка» (Lucide `wand-sparkles`, ISC). */
function WandIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72" />
      <path d="m14 7 3 3" />
      <path d="M5 6v4M19 14v4M10 2v2M7 8H3M21 16h-4M11 3H9" />
    </svg>
  );
}

/**
 * Панель экспорта: куда и под каким именем сохранить, затем настройки
 * конвертации — общие для всех файлов или свои для выбранных. Порядок
 * обработки рисунка: трансформация → положение → оформление → фон.
 */
export function ExportPanel(p: Props) {
  const t = strings[p.lang];
  const { options, exportPrefs: ex } = p;
  const set = (patch: Partial<Options>) => p.onOptions({ ...options, ...patch });
  const setEx = (patch: Partial<ExportPrefs>) => p.onExportPrefs({ ...ex, ...patch });
  /** Сброс ключей к `resetTo`. */
  const reset = (keys: readonly (keyof Options)[]) => {
    const out: Options = { ...options };
    for (const k of keys) (out as unknown as Record<string, unknown>)[k] = p.resetTo[k] ?? DEFAULT_OPTIONS[k] ?? null;
    p.onOptions(out);
  };
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

  // Другой набор настроек — запомненные цвета и толщины берутся из него.
  const firstContext = useRef(p.contextKey);
  useEffect(() => {
    if (firstContext.current === p.contextKey) return;
    firstContext.current = p.contextKey;
    setColor(options.background.kind === "solid" ? options.background.color : "#ffffff");
    setOutline(options.outline ?? DEFAULT_OUTLINE);
    setBorder(options.background_border ?? DEFAULT_BORDER);
    setRotBase(baseOf(options.rotation ?? 0));
    // Только при смене набора, не при каждой правке.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.contextKey]);

  const cropped = !!options.crop;
  const rotation = options.rotation ?? 0;
  // Шкала поворота — «середина ± 90°»; кнопки ±90° сдвигают и угол, и середину.
  const [rotBase, setRotBase] = useState(() => baseOf(rotation));
  useEffect(() => {
    if (Math.abs(normAngle(rotation - rotBase)) > 90) setRotBase(baseOf(rotation));
  }, [rotation, rotBase]);
  const rotShown = rotBase + normAngle(rotation - rotBase);
  const rotate90 = (dir: 1 | -1) => {
    set({ rotation: normAngle(rotation + dir * 90) });
    setRotBase(normAngle(rotBase + dir * 90));
  };

  const grid = options.grid ?? null;
  const shownGrid: [number, number] =
    options.grid_auto !== false && p.actualGrid && (p.actualGrid[0] > 1 || p.actualGrid[1] > 1)
      ? p.actualGrid
      : grid
        ? [grid.cols, grid.rows]
        : [AUTO_GRID_COLS, 1];
  const tiles = shownGrid[0] * shownGrid[1];
  const pad2 = (i: number) => String(i).padStart(Math.max(2, String(tiles).length), "0");
  const base = (p.selectedName ?? "image").replace(/\.svg$/i, "");

  const section = (id: SectionId, title: string, children: React.ReactNode) => (
    <Section
      title={title}
      open={!!p.sections[id]}
      onToggle={(open) => p.onSection(id, open)}
      resetTitle={t.resetSection(title)}
      isDefault={sameKeys(options, p.resetTo, SECTION_KEYS[id])}
      onReset={() => reset(SECTION_KEYS[id])}
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
  const many = p.selectedCount > 1;
  const oneTitle = p.selectedName
    ? options.grid
      ? t.exportOneTiles(p.selectedName, tiles)
      : t.exportOne(p.selectedName)
    : t.exportOne("…");

  return (
    <aside className="export" aria-label={t.exportPanel}>
      {/* Шапка как у «Файлов»: не прокручивается. «Экспортировать» — весь
          список; значок рядом — только выбранный файл. */}
      <div className="export__head">
        <h2 className="panel-title">{t.exportPanel}</h2>
        <div className="export__actions">
          <button
            type="button"
            className="btn btn--primary btn--wide"
            title={t.exportHint}
            disabled={p.totalCount === 0 || p.exporting}
            onClick={p.onExport}
          >
            {p.totalCount > 0 ? t.exportN(p.totalCount) : t.export}
          </button>
          <button
            type="button"
            className="btn btn--default btn--icon export__one"
            title={oneTitle}
            aria-label={oneTitle}
            disabled={!p.onExportOne || p.exporting}
            onClick={() => p.onExportOne?.()}
          >
            <FileDownIcon />
          </button>
        </div>
      </div>
      <div className="export__body scroll scroll--16">

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
        <button type="button" className="btn btn--default" title={t.chooseHint} onClick={p.onChooseDir}>
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
            disabled={sameKeys(options, p.resetTo, [...ALL_KEYS, "optimization"])}
            onClick={() => reset(ALL_KEYS)}
          />
        </div>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.exportSettings}>
          <SegButton active={p.scope === "all"} title={t.scopeAllHint} onClick={() => p.onScope("all")}>
            {t.scopeAll}
          </SegButton>
          <SegButton
            active={p.scope === "one"}
            title={many ? t.scopeSelectedHint(p.selectedCount) : t.scopeOneHint2}
            onClick={() => p.selectedName && p.onScope("one")}
          >
            {many ? t.scopeSelected : t.scopeOne}
          </SegButton>
        </div>
        {p.scope === "one" && p.selectedName && (
          <p className="hint">{many ? t.scopeSelectedHint(p.selectedCount) : t.scopeOneHint(p.selectedName)}</p>
        )}
      </div>

      {/* Результат: один файл или нарезка на эмодзи. Сетка по умолчанию
          подбирается по пропорциям рисунка, до 8 в ряд; числа можно
          поменять, палочка возвращает подбор. */}
      <div className="field">
        <div className="field__label">{t.resultKind}</div>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.resultKind}>
          <SegButton active={!grid} title={t.singleHint} onClick={() => set({ grid: null })}>
            {t.single}
          </SegButton>
          <SegButton
            active={!!grid}
            title={t.sliceHint}
            onClick={() => !grid && set({ grid: { cols: shownGrid[0], rows: shownGrid[1] }, grid_auto: true })}
          >
            {t.slice}
          </SegButton>
        </div>
        {grid && (
          <>
            <div className="opt-row grid-row">
              <label className="grid-row__field">
                {t.cols}
                <NumField
                  value={shownGrid[0]}
                  min={1}
                  max={MAX_GRID}
                  coarse={2}
                  label={t.cols}
                  onChange={(v) => set({ grid: { cols: Math.round(v), rows: shownGrid[1] }, grid_auto: false })}
                />
              </label>
              <span aria-hidden="true">×</span>
              <label className="grid-row__field">
                {t.rows}
                <NumField
                  value={shownGrid[1]}
                  min={1}
                  max={MAX_GRID}
                  coarse={2}
                  label={t.rows}
                  onChange={(v) => set({ grid: { cols: shownGrid[0], rows: Math.round(v) }, grid_auto: false })}
                />
              </label>
              <button
                type="button"
                className="btn btn--text btn--icon opt-row__end"
                title={t.autoGrid}
                aria-label={t.autoGrid}
                disabled={options.grid_auto !== false}
                onClick={() => set({ grid_auto: true })}
              >
                <WandIcon />
              </button>
            </div>
            {/* Размер сетки: столбцы, строки — по пропорциям рисунка. */}
            <div className="field field--slider">
              <div className="range-scale" aria-hidden="true">
                <span>{t.gridBigger}</span>
                <span>{t.gridFiner}</span>
              </div>
              <Range
                value={shownGrid[0]}
                min={1}
                max={MAX_GRID}
                coarse={2}
                label={t.gridSize}
                onChange={(v) => {
                  const cols = Math.max(1, Math.round(v));
                  const rows = Math.max(1, Math.min(MAX_GRID, Math.round(cols / (p.aspect > 0 ? p.aspect : 1))));
                  set({ grid: { cols, rows }, grid_auto: false });
                }}
              />
            </div>
            <p className="hint">{t.sliceInfo(tiles, `${base}_${pad2(1)}`, `${base}_${pad2(tiles)}`)}</p>
            {shownGrid[0] > AUTO_GRID_COLS && <p className="note note--warn">{t.sliceWide}</p>}
          </>
        )}
      </div>

      {section(
        "position",
        t.position,
        <>
          <div className="field">
            <div className="field__label">{t.bounds}</div>
            <div className="segmented segmented--wide" role="radiogroup" aria-label={t.bounds}>
              <SegButton
                active={(options.bounds ?? "container") === "container"}
                title={t.boundsContainerHint}
                onClick={() => set({ bounds: "container" })}
              >
                {t.boundsContainer}
              </SegButton>
              <SegButton
                active={options.bounds === "content"}
                title={t.boundsContentHint}
                onClick={() => set({ bounds: "content" })}
              >
                {t.boundsContent}
              </SegButton>
            </div>
          </div>
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
              coarse={10}
              value={Math.round((options.scale ?? 1) * 100)}
              onChange={(v) => set({ scale: v / 100 })}
            />
          )}
          <SliderField
            id="pos-x"
            label={t.positionX}
            unit="%"
            max={100}
            coarse={5}
            value={Math.round((options.position_x ?? 0.5) * 1000) / 10}
            decimals={1}
            onChange={(v) => set({ position_x: v / 100 })}
          />
          <SliderField
            id="pos-y"
            label={t.positionY}
            unit="%"
            max={100}
            coarse={5}
            value={Math.round((options.position_y ?? 0.5) * 1000) / 10}
            decimals={1}
            onChange={(v) => set({ position_y: v / 100 })}
          />
          <SliderField
            id="padding"
            label={t.padding}
            unit="px"
            max={100}
            coarse={4}
            value={Math.round(options.padding * CANVAS * 10) / 10}
            decimals={1}
            onChange={(v) => set({ padding: v / CANVAS })}
          />
          <p className="hint hint--xs">{t.precisionHint}</p>
        </>,
      )}

      {section(
        "transform",
        t.transform,
        <>
          {/* Порядок — как в ядре: отразить → пропорции → повернуть → обрезать. */}
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
          <SliderField
            id="aspect"
            label={t.aspect}
            unit="%"
            min={50}
            max={200}
            coarse={10}
            value={Math.round((options.aspect ?? 1) * 100)}
            onChange={(v) => set({ aspect: v / 100 })}
          />
          <div className="field field--slider">
            <div className="opt-row">
              <span className="opt-row__label opt-row__label--short">{t.rotate}</span>
              <IconToggle title={t.rotateLeft} onClick={() => rotate90(-1)}>
                <path d="M20 9V7a2 2 0 0 0-2-2h-6" />
                <path d="m15 2-3 3 3 3" />
                <path d="M20 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h2" />
              </IconToggle>
              <IconToggle title={t.rotateRight} onClick={() => rotate90(1)}>
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
                  coarse={15}
                  label={t.rotationAngle}
                  onChange={(v) => set({ rotation: normAngle(v) })}
                />
                °
              </span>
            </div>
            {/* Шкала «середина ± 90°» с подписями; середина — ближайший кратный 90°. */}
            <div className="range-scale" aria-hidden="true">
              <span>{normAngle(rotBase - 90)}°</span>
              <span>{normAngle(rotBase)}°</span>
              <span>{normAngle(rotBase + 90)}°</span>
            </div>
            <Range
              value={rotShown}
              min={rotBase - 90}
              max={rotBase + 90}
              center
              coarse={15}
              label={t.rotationAngle}
              onChange={(v) => set({ rotation: normAngle(v) })}
            />
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
            {cropped && <ResetButton title={t.cropReset} disabled={false} onClick={() => set({ crop: null })} />}
          </div>
        </>,
      )}

      {section(
        "style",
        t.styleSection,
        <ColorRow
          slider
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
            coarse={8}
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
            slider
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
          <label className="check" title={t.bgClipHint}>
            <input
              type="checkbox"
              checked={options.background_clip !== false}
              onChange={() => set({ background_clip: options.background_clip === false })}
            />
            {t.bgClip}
          </label>
        </>,
      )}

      {section(
        "extra",
        t.advanced,
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
          {/* Настройки текстом (JSON ядра, как у svg2tgs --options): для того,
              что сейчас правится; папка и имена файлов не входят. */}
          <div className="field settings-io">
            <div className="field__label">{t.settingsImport}</div>
            <div className="settings-io__row">
              <button type="button" className="btn btn--default" title={t.pasteHint} onClick={p.onPasteSettings}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <rect width="8" height="4" x="8" y="2" rx="1" />
                  <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
                </svg>
                {t.paste}
              </button>
              <button type="button" className="btn btn--default" title={t.fromFileHint} onClick={p.onLoadSettings}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
                  <path d="M14 2v4a2 2 0 0 0 2 2h4" />
                  <path d="M12 12v6" />
                  <path d="m15 15-3-3-3 3" />
                </svg>
                {t.fromFile}
              </button>
            </div>
            <div className="field__label">{t.settingsExport}</div>
            <div className="settings-io__row">
              <button type="button" className="btn btn--default" title={t.copyHint} onClick={p.onCopySettings}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <rect width="14" height="14" x="8" y="8" rx="2" />
                  <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
                </svg>
                {t.copy}
              </button>
              <button type="button" className="btn btn--default" title={t.toFileHint} onClick={p.onSaveSettings}>
                <FileDownIcon />
                {t.toFile}
              </button>
            </div>
          </div>
        </div>,
      )}
      </div>
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

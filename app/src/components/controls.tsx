/**
 * Поля панели: число с единицей, ползунок, строка «флажок + толщина + цвет»,
 * раскрывающийся раздел с кнопкой сброса.
 *
 * Клавиши одинаковы для ползунков (перетаскивание), стрелок в полях и
 * колёсика над полем в фокусе:
 * - Alt (⌥) или Ctrl — плавно, шаг в 10 раз мельче;
 * - Shift — шагами поля (`coarse`: поворот 15°, X/Y 5 %, отступ 4 px…);
 * - Shift+Alt — плавно, но с шагом 1.
 */

import { useEffect, useRef } from "react";
import { MAX_OUTLINE } from "../types";

type Mods = { altKey: boolean; ctrlKey: boolean; shiftKey: boolean; metaKey?: boolean };

/** Мелкий шаг: Alt или Ctrl (⌘ на macOS). */
const fine = (e: Mods) => e.altKey || e.ctrlKey || !!e.metaKey;

/** Множитель скорости перетаскивания: Alt/Ctrl — в 10 раз медленнее. */
export function precision(e: Mods): number {
  return fine(e) && !e.shiftKey ? 0.1 : 1;
}

/** Шаг стрелок и колёсика по зажатым клавишам. */
function keyStep(e: Mods, coarse: number): number {
  if (e.shiftKey) return fine(e) ? 1 : coarse;
  return fine(e) ? 0.1 : 1;
}

/** Округление значения после шага: шаг Shift — до кратного шагу. */
function snap(v: number, step: number): number {
  if (step >= 1) return Math.round(v / step) * step;
  return Math.round(v * 10) / 10;
}

const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(max, v));

/**
 * Колёсико над полем в фокусе — как стрелки (вверх — больше). Без фокуса
 * колёсико прокручивает панель. Слушатель не пассивный: иначе нельзя
 * отменить прокрутку панели и родной шаг числового поля.
 */
function useWheel<T extends HTMLElement>(handler: (e: WheelEvent, dir: number) => void) {
  const ref = useRef<T>(null);
  const latest = useRef(handler);
  latest.current = handler;
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const on = (e: WheelEvent) => {
      if (document.activeElement !== el) return;
      const d = e.deltaY || e.deltaX;
      const dir = d < 0 ? 1 : d > 0 ? -1 : 0;
      if (!dir) return;
      e.preventDefault();
      latest.current(e, dir);
    };
    el.addEventListener("wheel", on, { passive: false });
    return () => el.removeEventListener("wheel", on);
  }, []);
  return ref;
}

/** Числовое поле: ввод, стрелки (с Alt/Shift), без системных стрелочек. */
export function NumField(p: {
  value: number;
  min: number;
  max: number;
  /** Сколько знаков после запятой показывать. */
  decimals?: number;
  disabled?: boolean;
  label: string;
  width?: number;
  /** Шаг с Shift. */
  coarse?: number;
  onChange: (v: number) => void;
}) {
  const d = p.decimals ?? 0;
  const k = 10 ** d;
  const coarse = p.coarse ?? 10;
  const bump = (e: Mods, dir: number) => {
    const step = keyStep(e, coarse);
    p.onChange(clamp(snap(p.value + dir * step, step), p.min, p.max));
  };
  const ref = useWheel<HTMLInputElement>((e, dir) => !p.disabled && bump(e, dir));
  return (
    <input
      ref={ref}
      className="num"
      type="number"
      min={p.min}
      max={p.max}
      step={1 / k}
      value={Math.round(p.value * k) / k}
      disabled={p.disabled}
      aria-label={p.label}
      style={p.width ? { width: p.width } : undefined}
      onChange={(e) => {
        const v = Number(e.target.value);
        if (Number.isFinite(v)) p.onChange(clamp(v, p.min, p.max));
      }}
      onKeyDown={(e) => {
        if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
        e.preventDefault();
        bump(e, e.key === "ArrowUp" ? 1 : -1);
      }}
    />
  );
}

/**
 * Ползунок со своим перетаскиванием: значение меняется на сдвиг мыши,
 * умноженный на точность (Alt/Shift можно зажать и отпустить на ходу).
 * Щелчок по дорожке без Alt ставит значение туда, куда щёлкнули.
 */
export function Range(p: {
  id?: string;
  value: number;
  min: number;
  max: number;
  disabled?: boolean;
  label: string;
  /** Ноль посередине: заливка от центра к бегунку. */
  center?: boolean;
  /** Шаг с Shift. */
  coarse?: number;
  onChange: (v: number) => void;
}) {
  const drag = useRef<{ value: number } | null>(null);
  const coarse = p.coarse ?? 10;
  const ref = useWheel<HTMLInputElement>((e, dir) => {
    if (p.disabled) return;
    const step = keyStep(e, coarse);
    p.onChange(clamp(snap(p.value + dir * step, step), p.min, p.max));
  });
  const span = p.max - p.min;
  const fill = span > 0 ? ((p.value - p.min) / span) * 100 : 0;
  const style = p.center
    ? ({ "--from": `${Math.min(50, fill)}%`, "--to": `${Math.max(50, fill)}%` } as React.CSSProperties)
    : ({ "--fill": `${fill}%` } as React.CSSProperties);
  return (
    <input
      ref={ref}
      id={p.id}
      type="range"
      min={p.min}
      max={p.max}
      step="any"
      value={p.value}
      disabled={p.disabled}
      aria-label={p.label}
      className={p.center ? "range--center" : undefined}
      style={style}
      onChange={(e) => {
        // Клавиатура (стрелки, Home/End) — обычное поведение.
        if (!drag.current) p.onChange(clamp(Math.round(Number(e.target.value)), p.min, p.max));
      }}
      onKeyDown={(e) => {
        if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) return;
        e.preventDefault();
        const dir = e.key === "ArrowRight" || e.key === "ArrowUp" ? 1 : -1;
        const step = keyStep(e, coarse);
        p.onChange(clamp(snap(p.value + dir * step, step), p.min, p.max));
      }}

      onPointerDown={(e) => {
        if (p.disabled || e.button !== 0) return;
        e.preventDefault();
        const el = e.currentTarget;
        el.setPointerCapture(e.pointerId);
        el.focus();
        let value = p.value;
        if (!fine(e)) {
          const r = el.getBoundingClientRect();
          value = clamp(p.min + ((e.clientX - r.left) / r.width) * span, p.min, p.max);
          p.onChange(e.shiftKey ? snap(value, coarse) : Math.round(value));
        }
        drag.current = { value };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const width = e.currentTarget.getBoundingClientRect().width || 1;
        d.value = clamp(d.value + (e.movementX / width) * span * (fine(e) ? 0.1 : 1), p.min, p.max);
        // Shift — шагами поля; Shift+Alt — плавно, но целыми; Alt — плавно.
        const v = e.shiftKey ? (fine(e) ? Math.round(d.value) : snap(d.value, coarse)) : fine(e) ? Math.round(d.value * 10) / 10 : Math.round(d.value);
        p.onChange(clamp(v, p.min, p.max));
      }}
      onMouseDown={(e) => e.preventDefault()}
      onPointerUp={() => (drag.current = null)}
      onPointerCancel={() => (drag.current = null)}
    />
  );
}

/** Подпись, число с единицей и ползунок под ними. */
export function SliderField(p: {
  id: string;
  label: string;
  unit: string;
  min?: number;
  max: number;
  value: number;
  decimals?: number;
  /** Шаг с Shift. */
  coarse?: number;
  onChange: (v: number) => void;
}) {
  const min = p.min ?? 0;
  return (
    <div className="field field--slider">
      <div className="field__label">
        <label htmlFor={p.id}>{p.label}</label>
        <span className="opt-row__unit">
          <NumField
            value={p.value}
            min={min}
            max={p.max}
            decimals={p.decimals}
            coarse={p.coarse}
            label={`${p.label}, ${p.unit}`}
            onChange={p.onChange}
          />
          {p.unit}
        </span>
      </div>
      <Range id={p.id} value={p.value} min={min} max={p.max} coarse={p.coarse} label={p.label} onChange={p.onChange} />
    </div>
  );
}

/** Строка «флажок + подпись | толщина px | цвет» (обводки, заливка фона). */
export function ColorRow(p: {
  /** Ползунок толщины под строкой (1…64). */
  slider?: boolean;
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
  const row = (
    <div className="opt-row">
      <label className="opt-row__label">
        <input type="checkbox" checked={p.on} onChange={p.onToggle} />
        {p.label}
      </label>
      {p.width !== undefined && p.onWidth && (
        <span className="opt-row__unit">
          <NumField
            value={p.width}
            min={1}
            max={MAX_OUTLINE}
            disabled={!p.on}
            label={p.widthLabel ?? p.label}
            coarse={4}
            onChange={(w) => p.onWidth!(Math.max(1, Math.round(w)))}
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
  if (!p.slider || p.width === undefined || !p.onWidth) return row;
  return (
    <div className="field field--slider">
      {row}
      <Range
        value={p.width}
        min={1}
        max={MAX_OUTLINE}
        coarse={4}
        disabled={!p.on}
        label={p.widthLabel ?? p.label}
        onChange={(w) => p.onWidth!(Math.max(1, Math.round(w)))}
      />
    </div>
  );
}

/** Значок сброса: круговая стрелка (Lucide `rotate-ccw`, ISC). */
export function ResetIcon({ size = 16 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
      <path d="M3 3v5h5" />
    </svg>
  );
}

/** Кнопка сброса раздела: неактивна, если в разделе всё по умолчанию. */
export function ResetButton(p: { title: string; disabled: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      className="btn btn--text btn--icon sec__reset"
      title={p.title}
      aria-label={p.title}
      disabled={p.disabled}
      onClick={(e) => {
        // Кнопка лежит в <summary>: щелчок не должен сворачивать раздел.
        e.preventDefault();
        e.stopPropagation();
        p.onClick();
      }}
    >
      <ResetIcon />
    </button>
  );
}

/** Раскрывающийся раздел: шеврон, название, справа — сброс раздела. */
export function Section(p: {
  title: string;
  open: boolean;
  onToggle: (open: boolean) => void;
  resetTitle: string;
  isDefault: boolean;
  onReset: () => void;
  children: React.ReactNode;
}) {
  return (
    <details
      className="advanced"
      open={p.open}
      onToggle={(e) => {
        const open = (e.currentTarget as HTMLDetailsElement).open;
        if (open !== p.open) p.onToggle(open);
      }}
    >
      <summary className="sec">
        <svg className="sec__chev" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="m9 6 6 6-6 6" />
        </svg>
        <span>{p.title}</span>
        <ResetButton title={p.resetTitle} disabled={p.isDefault} onClick={p.onReset} />
      </summary>
      <div className="section">{p.children}</div>
    </details>
  );
}

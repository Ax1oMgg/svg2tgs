import { useEffect, useRef, useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { Crop, Options } from "../types";
import { precision } from "./controls";

interface Props {
  lang: Lang;
  id: string;
  options: Options;
  onOptions: (patch: Partial<Options>) => void;
  onDone: () => void;
}

const NO_CROP: Crop = { top: 0, right: 0, bottom: 0, left: 0 };
/** Наименьший остаток рисунка после обрезки, доля стороны. */
const MIN_KEEP = 0.05;
const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v));

/** Рамка рисунка после отражения, поворота и пропорций (как в ядре, placement.rs). */
function shapeBounds(w: number, h: number, o: Options): [number, number] {
  const a = ((o.rotation ?? 0) * Math.PI) / 180;
  const k = o.aspect ?? 1;
  let x0 = Infinity;
  let x1 = -Infinity;
  let y0 = Infinity;
  let y1 = -Infinity;
  for (const [px, py] of [
    [-w / 2, -h / 2],
    [w / 2, -h / 2],
    [w / 2, h / 2],
    [-w / 2, h / 2],
  ]) {
    const x = (px * Math.cos(a) - py * Math.sin(a)) * k;
    const y = px * Math.sin(a) + py * Math.cos(a);
    x0 = Math.min(x0, x);
    x1 = Math.max(x1, x);
    y0 = Math.min(y0, y);
    y1 = Math.max(y1, y);
  }
  return [x1 - x0, y1 - y0];
}

type Handle = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw" | "move" | "rotate";

/**
 * Кадрирование мышью в превью: рамка с ручками по краям и углам — обрезать,
 * перетаскивание внутри рамки — сдвинуть её, кружок сверху — повернуть.
 * Alt — точнее, Shift при повороте — шаг 15°. Правки сразу уходят в настройки.
 */
export function CropEditor({ lang, id, options, onOptions, onDone }: Props) {
  const t = strings[lang];
  const stage = useRef<HTMLDivElement>(null);
  const [side, setSide] = useState(360);
  const [image, setImage] = useState<string | null>(null);
  const [size, setSize] = useState<[number, number] | null>(null);
  const drag = useRef<{ handle: Handle; crop: Crop; rotation: number; angle: number; acc: number } | null>(null);

  useEffect(() => {
    void backend.preview(id, "source", 512).then(setImage);
    void backend.sourceSize(id).then(setSize);
  }, [id]);

  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setSide(Math.max(160, Math.min(e.contentRect.width, e.contentRect.height) - 64)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const crop = options.crop ?? NO_CROP;
  const rotation = options.rotation ?? 0;
  const [w, h] = size ?? [1, 1];
  const [bw, bh] = shapeBounds(w, h, options);
  const f = side / Math.max(bw, bh);
  // Рамка рисунка (до обрезки) в пикселях сцены, по центру.
  const box = { w: bw * f, h: bh * f };
  const frame = {
    left: crop.left * box.w,
    top: crop.top * box.h,
    width: (1 - crop.left - crop.right) * box.w,
    height: (1 - crop.top - crop.bottom) * box.h,
  };
  const square = Math.max(w, h) * f;
  const fx = options.flip_h ? -1 : 1;
  const fy = options.flip_v ? -1 : 1;
  const imgTransform = `translate(-50%, -50%) scale(${options.aspect ?? 1}, 1) rotate(${rotation}deg) scale(${fx}, ${fy})`;

  const start = (handle: Handle) => (e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const r = stage.current!.getBoundingClientRect();
    const angle = Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2));
    drag.current = { handle, crop: { ...crop }, rotation, angle, acc: 0 };
  };

  const move = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const k = precision(e) < 1 ? 0.1 : 1;
    if (d.handle === "rotate") {
      const r = stage.current!.getBoundingClientRect();
      const angle = Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2));
      let delta = angle - d.angle;
      if (delta > Math.PI) delta -= 2 * Math.PI;
      if (delta < -Math.PI) delta += 2 * Math.PI;
      d.acc += (delta * 180) / Math.PI * k;
      d.angle = angle;
      let next = d.rotation + d.acc;
      next = ((((next + 180) % 360) + 360) % 360) - 180;
      if (e.shiftKey) next = Math.round(next / 15) * 15;
      onOptions({ rotation: Math.round(next * 10) / 10 });
      return;
    }
    const dx = (e.movementX / box.w) * k;
    const dy = (e.movementY / box.h) * k;
    const c = { ...d.crop };
    const h_ = d.handle;
    if (h_ === "move") {
      const mx = clamp(dx, -c.left, c.right);
      const my = clamp(dy, -c.top, c.bottom);
      c.left += mx;
      c.right -= mx;
      c.top += my;
      c.bottom -= my;
    } else {
      if (h_.includes("w")) c.left = clamp(c.left + dx, 0, 1 - MIN_KEEP - c.right);
      if (h_.includes("e")) c.right = clamp(c.right - dx, 0, 1 - MIN_KEEP - c.left);
      if (h_.includes("n")) c.top = clamp(c.top + dy, 0, 1 - MIN_KEEP - c.bottom);
      if (h_.includes("s")) c.bottom = clamp(c.bottom - dy, 0, 1 - MIN_KEEP - c.top);
    }
    d.crop = c;
    onOptions({ crop: c });
  };

  const end = () => (drag.current = null);
  const handles: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

  return (
    <div className="crop">
      <div className="crop__bar">
        <span className="crop__title">{t.cropTitle}</span>
        <span className="crop__meta">{t.cropRotation(rotation)}</span>
        <button
          type="button"
          className="btn btn--text btn--sm crop__reset"
          onClick={() => onOptions({ crop: NO_CROP, rotation: 0 })}
        >
          {t.cropResetAll}
        </button>
        <button type="button" className="btn btn--primary btn--sm" onClick={onDone}>
          {t.done}
        </button>
      </div>
      <div ref={stage} className="crop__stage" onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
        <div className="crop__box bg-checker" style={{ width: box.w, height: box.h }}>
          {image && (
            <img className="crop__img crop__img--ghost" src={image} alt="" style={{ width: square, height: square, transform: imgTransform }} />
          )}
          <div className="crop__frame" style={frame} onPointerDown={start("move")}>
            <div className="crop__clip" style={{ left: -frame.left, top: -frame.top, width: box.w, height: box.h }}>
              {image && <img className="crop__img" src={image} alt="" style={{ width: square, height: square, transform: imgTransform }} />}
            </div>
            <span className="crop__grid" aria-hidden="true" />
            {handles.map((hd) => (
              <span key={hd} className={`crop__handle crop__handle--${hd}`} onPointerDown={start(hd)} />
            ))}
            <span className="crop__stem" aria-hidden="true" />
            <span className="crop__rotate" title={t.rotate} onPointerDown={start("rotate")} />
          </div>
        </div>
      </div>
      <p className="crop__help">{t.cropHelp}</p>
    </div>
  );
}

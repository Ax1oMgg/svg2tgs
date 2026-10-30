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

/** Наименьший остаток рисунка после обрезки, доля стороны (как в ядре). */
const MIN_KEEP = 0.05;
const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v));

/** Прямоугольник `[x0, y0, x1, y1]` относительно центра рисунка. */
type Rect = [number, number, number, number];

/**
 * Рамка рисунка после отражения, пропорций и поворота (как в ядре,
 * placement.rs): пропорции — до поворота. Возвращает ширину и высоту.
 */
function shapeBounds(w: number, h: number, o: Options): [number, number] {
  const a = ((o.rotation ?? 0) * Math.PI) / 180;
  const k = o.aspect ?? 1;
  let x1 = 0;
  let y1 = 0;
  for (const [px, py] of [
    [w / 2, -h / 2],
    [w / 2, h / 2],
  ]) {
    x1 = Math.max(x1, Math.abs(px * k * Math.cos(a) - py * Math.sin(a)));
    y1 = Math.max(y1, Math.abs(px * k * Math.sin(a) + py * Math.cos(a)));
  }
  return [2 * x1, 2 * y1];
}

/** Рамка обрезки в единицах рисунка (как в ядре): от рамки без поворота
 *  `w0 × h0`, не шире повёрнутого рисунка `bw × bh`. */
function frameOf(crop: Crop | null | undefined, w0: number, h0: number, bw: number, bh: number): Rect {
  if (!crop) return [-bw / 2, -bh / 2, bw / 2, bh / 2];
  return [
    Math.max(-bw / 2, -w0 / 2 + crop.left * w0),
    Math.max(-bh / 2, -h0 / 2 + crop.top * h0),
    Math.min(bw / 2, w0 / 2 - crop.right * w0),
    Math.min(bh / 2, h0 / 2 - crop.bottom * h0),
  ];
}

/** Обратно: рамка → отступы от рамки без поворота. */
function cropOf(r: Rect, w0: number, h0: number): Crop {
  const k = (v: number) => Math.round(v * 10000) / 10000;
  return {
    left: k((r[0] + w0 / 2) / w0),
    top: k((r[1] + h0 / 2) / h0),
    right: k((w0 / 2 - r[2]) / w0),
    bottom: k((h0 / 2 - r[3]) / h0),
  };
}

type Handle = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw" | "move" | "rotate";

/**
 * Кадрирование мышью в превью: рамка с ручками по краям и углам — обрезать,
 * перетаскивание внутри рамки — сдвинуть её, кружок сверху — повернуть.
 * Рамка стоит на месте при повороте — поворачивается рисунок под ней.
 * Alt или Ctrl — точнее, Shift при повороте — шаг 15°. Правки сразу уходят
 * в настройки.
 */
export function CropEditor({ lang, id, options, onOptions, onDone }: Props) {
  const t = strings[lang];
  const stage = useRef<HTMLDivElement>(null);
  const [side, setSide] = useState(360);
  const [image, setImage] = useState<string | null>(null);
  const [size, setSize] = useState<[number, number] | null>(null);
  const drag = useRef<{ handle: Handle; rect: Rect; rotation: number; angle: number; acc: number } | null>(null);

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

  const rotation = options.rotation ?? 0;
  const [w, h] = size ?? [1, 1];
  const k = options.aspect ?? 1;
  const [w0, h0] = [w * k, h];
  const [bw, bh] = shapeBounds(w, h, options);
  const f = side / Math.max(bw, bh);
  // Рамка рисунка (до обрезки) в пикселях сцены, по центру.
  const box = { w: bw * f, h: bh * f };
  const rect = frameOf(options.crop, w0, h0, bw, bh);
  const frame = {
    left: (rect[0] + bw / 2) * f,
    top: (rect[1] + bh / 2) * f,
    width: (rect[2] - rect[0]) * f,
    height: (rect[3] - rect[1]) * f,
  };
  const square = Math.max(w, h) * f;
  const fx = options.flip_h ? -1 : 1;
  const fy = options.flip_v ? -1 : 1;
  // CSS применяет справа налево: отразить → пропорции → повернуть.
  const imgTransform = `translate(-50%, -50%) rotate(${rotation}deg) scale(${k}, 1) scale(${fx}, ${fy})`;

  const start = (handle: Handle) => (e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const r = stage.current!.getBoundingClientRect();
    const angle = Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2));
    drag.current = { handle, rect: [...rect], rotation, angle, acc: 0 };
  };

  const move = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const slow = precision(e);
    if (d.handle === "rotate") {
      const r = stage.current!.getBoundingClientRect();
      const angle = Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2));
      let delta = angle - d.angle;
      if (delta > Math.PI) delta -= 2 * Math.PI;
      if (delta < -Math.PI) delta += 2 * Math.PI;
      d.acc += ((delta * 180) / Math.PI) * slow;
      d.angle = angle;
      let next = d.rotation + d.acc;
      next = ((((next + 180) % 360) + 360) % 360) - 180;
      if (e.shiftKey) next = Math.round(next / 15) * 15;
      onOptions({ rotation: Math.round(next * 10) / 10 });
      return;
    }
    const dx = (e.movementX / f) * slow;
    const dy = (e.movementY / f) * slow;
    const [x0, y0, x1, y1] = d.rect;
    const minW = MIN_KEEP * bw;
    const minH = MIN_KEEP * bh;
    let next: Rect;
    if (d.handle === "move") {
      const mx = clamp(dx, -bw / 2 - x0, bw / 2 - x1);
      const my = clamp(dy, -bh / 2 - y0, bh / 2 - y1);
      next = [x0 + mx, y0 + my, x1 + mx, y1 + my];
    } else {
      const hd = d.handle;
      next = [
        hd.includes("w") ? clamp(x0 + dx, -bw / 2, x1 - minW) : x0,
        hd.includes("n") ? clamp(y0 + dy, -bh / 2, y1 - minH) : y0,
        hd.includes("e") ? clamp(x1 + dx, x0 + minW, bw / 2) : x1,
        hd.includes("s") ? clamp(y1 + dy, y0 + minH, bh / 2) : y1,
      ];
    }
    d.rect = next;
    onOptions({ crop: cropOf(next, w0, h0) });
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
          onClick={() => onOptions({ crop: null, rotation: 0 })}
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

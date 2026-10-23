#!/usr/bin/env python3
"""
svg2tgs.py — конвертер статичных SVG в анимированные Telegram-стикеры/эмодзи (.tgs).

TGS = gzip-сжатый Lottie JSON. Telegram не поддерживает статичные векторные
эмодзи напрямую, поэтому мы собираем Lottie-анимацию без реального движения
(один и тот же кадр в начале и в конце), но с длительностью и параметрами,
которые Telegram считает валидными.

Не использует сторонние библиотеки — только стандартная библиотека Python 3.

Использование:
    Один файл:
        python3 svg2tgs.py icon.svg -o icon.tgs

    Пакетно (папка -> папка):
        python3 svg2tgs.py ./svg_folder -o ./tgs_folder

    Пакетно по маске:
        python3 svg2tgs.py "./icons/*.svg" -o ./tgs_folder

Опции:
    --size N        сторона канваса в пикселях (по умолчанию 512, для кастомных
                     эмодзи Telegram обычно использует 100)
    --fps N         кадров в секунду (по умолчанию 60, требование Telegram)
    --duration S    длительность анимации в секундах (по умолчанию 1.0)
    --bg none|COLOR фон (по умолчанию прозрачный)
    --max-size KB   предупреждать, если итоговый .tgs больше этого размера (по умолчанию 64 КБ)
"""

import argparse
import glob
import gzip
import json
import math
import os
import re
import sys
import xml.etree.ElementTree as ET

SVG_NS = "{http://www.w3.org/2000/svg}"

# ---------------------------------------------------------------------------
# Матрицы 2D-аффинных преобразований: (a, b, c, d, e, f)
#   x' = a*x + c*y + e
#   y' = b*x + d*y + f
# ---------------------------------------------------------------------------

IDENTITY = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def mat_mul(m1, m2):
    a1, b1, c1, d1, e1, f1 = m1
    a2, b2, c2, d2, e2, f2 = m2
    return (
        a1 * a2 + c1 * b2,
        b1 * a2 + d1 * b2,
        a1 * c2 + c1 * d2,
        b1 * c2 + d1 * d2,
        a1 * e2 + c1 * f2 + e1,
        b1 * e2 + d1 * f2 + f1,
    )


def mat_point(m, x, y):
    a, b, c, d, e, f = m
    return (a * x + c * y + e, b * x + d * y + f)


def mat_vector(m, x, y):
    """Преобразование вектора (без учёта смещения) — для тангенсов кривых."""
    a, b, c, d, e, f = m
    return (a * x + c * y, b * x + d * y)


NUM_RE = re.compile(r"[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?")


def parse_transform(s):
    if not s:
        return IDENTITY
    m = IDENTITY
    for func, args in re.findall(r"(\w+)\s*\(([^)]*)\)", s):
        nums = [float(x) for x in NUM_RE.findall(args)]
        func = func.strip().lower()
        if func == "translate":
            tx = nums[0] if len(nums) > 0 else 0.0
            ty = nums[1] if len(nums) > 1 else 0.0
            fm = (1.0, 0.0, 0.0, 1.0, tx, ty)
        elif func == "scale":
            sx = nums[0] if len(nums) > 0 else 1.0
            sy = nums[1] if len(nums) > 1 else sx
            fm = (sx, 0.0, 0.0, sy, 0.0, 0.0)
        elif func == "rotate":
            ang = math.radians(nums[0] if nums else 0.0)
            cos_a, sin_a = math.cos(ang), math.sin(ang)
            if len(nums) >= 3:
                cx, cy = nums[1], nums[2]
                fm = mat_mul(
                    mat_mul((1, 0, 0, 1, cx, cy), (cos_a, sin_a, -sin_a, cos_a, 0, 0)),
                    (1, 0, 0, 1, -cx, -cy),
                )
            else:
                fm = (cos_a, sin_a, -sin_a, cos_a, 0.0, 0.0)
        elif func == "skewx":
            fm = (1.0, 0.0, math.tan(math.radians(nums[0])), 1.0, 0.0, 0.0)
        elif func == "skewy":
            fm = (1.0, math.tan(math.radians(nums[0])), 0.0, 1.0, 0.0, 0.0)
        elif func == "matrix":
            if len(nums) == 6:
                fm = tuple(nums)
            else:
                fm = IDENTITY
        else:
            fm = IDENTITY
        m = mat_mul(m, fm)
    return m


# ---------------------------------------------------------------------------
# Цвета
# ---------------------------------------------------------------------------

CSS_COLORS = {
    "black": "#000000", "silver": "#c0c0c0", "gray": "#808080", "grey": "#808080",
    "white": "#ffffff", "maroon": "#800000", "red": "#ff0000", "purple": "#800080",
    "fuchsia": "#ff00ff", "green": "#008000", "lime": "#00ff00", "olive": "#808000",
    "yellow": "#ffff00", "navy": "#000080", "blue": "#0000ff", "teal": "#008080",
    "aqua": "#00ffff", "orange": "#ffa500", "aliceblue": "#f0f8ff",
    "antiquewhite": "#faebd7", "aquamarine": "#7fffd4", "azure": "#f0ffff",
    "beige": "#f5f5dc", "bisque": "#ffe4c4", "blanchedalmond": "#ffebcd",
    "blueviolet": "#8a2be2", "brown": "#a52a2a", "burlywood": "#deb887",
    "cadetblue": "#5f9ea0", "chartreuse": "#7fff00", "chocolate": "#d2691e",
    "coral": "#ff7f50", "cornflowerblue": "#6495ed", "cornsilk": "#fff8dc",
    "crimson": "#dc143c", "cyan": "#00ffff", "darkblue": "#00008b",
    "darkcyan": "#008b8b", "darkgoldenrod": "#b8860b", "darkgray": "#a9a9a9",
    "darkgreen": "#006400", "darkgrey": "#a9a9a9", "darkkhaki": "#bdb76b",
    "darkmagenta": "#8b008b", "darkolivegreen": "#556b2f", "darkorange": "#ff8c00",
    "darkorchid": "#9932cc", "darkred": "#8b0000", "darksalmon": "#e9967a",
    "darkseagreen": "#8fbc8f", "darkslateblue": "#483d8b", "darkslategray": "#2f4f4f",
    "darkturquoise": "#00ced1", "darkviolet": "#9400d3", "deeppink": "#ff1493",
    "deepskyblue": "#00bfff", "dimgray": "#696969", "dodgerblue": "#1e90ff",
    "firebrick": "#b22222", "floralwhite": "#fffaf0", "forestgreen": "#228b22",
    "gainsboro": "#dcdcdc", "ghostwhite": "#f8f8ff", "gold": "#ffd700",
    "goldenrod": "#daa520", "greenyellow": "#adff2f", "honeydew": "#f0fff0",
    "hotpink": "#ff69b4", "indianred": "#cd5c5c", "indigo": "#4b0082",
    "ivory": "#fffff0", "khaki": "#f0e68c", "lavender": "#e6e6fa",
    "lavenderblush": "#fff0f5", "lawngreen": "#7cfc00", "lemonchiffon": "#fffacd",
    "lightblue": "#add8e6", "lightcoral": "#f08080", "lightcyan": "#e0ffff",
    "lightgoldenrodyellow": "#fafad2", "lightgray": "#d3d3d3", "lightgreen": "#90ee90",
    "lightgrey": "#d3d3d3", "lightpink": "#ffb6c1", "lightsalmon": "#ffa07a",
    "lightseagreen": "#20b2aa", "lightskyblue": "#87cefa", "lightslategray": "#778899",
    "lightsteelblue": "#b0c4de", "lightyellow": "#ffffe0", "limegreen": "#32cd32",
    "linen": "#faf0e6", "magenta": "#ff00ff", "mediumaquamarine": "#66cdaa",
    "mediumblue": "#0000cd", "mediumorchid": "#ba55d3", "mediumpurple": "#9370db",
    "mediumseagreen": "#3cb371", "mediumslateblue": "#7b68ee",
    "mediumspringgreen": "#00fa9a", "mediumturquoise": "#48d1cc",
    "mediumvioletred": "#c71585", "midnightblue": "#191970", "mintcream": "#f5fffa",
    "mistyrose": "#ffe4e1", "moccasin": "#ffe4b5", "navajowhite": "#ffdead",
    "oldlace": "#fdf5e6", "olivedrab": "#6b8e23", "orangered": "#ff4500",
    "orchid": "#da70d6", "palegoldenrod": "#eee8aa", "palegreen": "#98fb98",
    "paleturquoise": "#afeeee", "palevioletred": "#db7093", "papayawhip": "#ffefd5",
    "peachpuff": "#ffdab9", "peru": "#cd853f", "pink": "#ffc0cb", "plum": "#dda0dd",
    "powderblue": "#b0e0e6", "rosybrown": "#bc8f8f", "royalblue": "#4169e1",
    "saddlebrown": "#8b4513", "salmon": "#fa8072", "sandybrown": "#f4a460",
    "seagreen": "#2e8b57", "seashell": "#fff5ee", "sienna": "#a0522d",
    "skyblue": "#87ceeb", "slateblue": "#6a5acd", "slategray": "#708090",
    "snow": "#fffafa", "springgreen": "#00ff7f", "steelblue": "#4682b4",
    "tan": "#d2b48c", "thistle": "#d8bfd8", "tomato": "#ff6347",
    "turquoise": "#40e0d0", "violet": "#ee82ee", "wheat": "#f5deb3",
    "whitesmoke": "#f5f5f5", "yellowgreen": "#9acd32", "transparent": "#00000000",
}


def parse_color(s, default_opacity=1.0):
    """Возвращает ([r,g,b] в 0..1, opacity) либо None, если заливки/обводки нет."""
    if s is None:
        return None
    s = s.strip()
    if s in ("none", ""):
        return None
    if s == "currentColor":
        s = "#000000"
    if s.startswith("#"):
        hexs = s[1:]
        if len(hexs) == 3:
            r, g, b = (int(c * 2, 16) for c in hexs)
            return [r / 255, g / 255, b / 255], default_opacity
        if len(hexs) == 4:
            r, g, b, a = (int(c * 2, 16) for c in hexs)
            return [r / 255, g / 255, b / 255], (a / 255) * default_opacity
        if len(hexs) == 6:
            r, g, b = int(hexs[0:2], 16), int(hexs[2:4], 16), int(hexs[4:6], 16)
            return [r / 255, g / 255, b / 255], default_opacity
        if len(hexs) == 8:
            r, g, b, a = (int(hexs[i:i + 2], 16) for i in range(0, 8, 2))
            return [r / 255, g / 255, b / 255], (a / 255) * default_opacity
        return [0, 0, 0], default_opacity
    m = re.match(r"rgba?\(([^)]+)\)", s)
    if m:
        parts = [p.strip() for p in m.group(1).split(",")]
        def chan(v):
            v = v.strip()
            if v.endswith("%"):
                return float(v[:-1]) / 100.0
            return float(v) / 255.0
        r, g, b = chan(parts[0]), chan(parts[1]), chan(parts[2])
        a = float(parts[3]) if len(parts) > 3 else 1.0
        return [r, g, b], a * default_opacity
    low = s.lower()
    if low in CSS_COLORS:
        return parse_color(CSS_COLORS[low], default_opacity)
    return [0, 0, 0], default_opacity


def parse_style(style_str):
    out = {}
    if not style_str:
        return out
    for chunk in style_str.split(";"):
        if ":" in chunk:
            k, v = chunk.split(":", 1)
            out[k.strip().lower()] = v.strip()
    return out


def parse_length(s, reference=None):
    if s is None:
        return None
    s = s.strip()
    m = re.match(r"^([-+]?[0-9]*\.?[0-9]+)\s*%?$", s)
    if not m:
        return None
    val = float(m.group(1))
    if s.endswith("%") and reference is not None:
        val = val / 100.0 * reference
    return val


# ---------------------------------------------------------------------------
# Стилевой контекст (наследуемые атрибуты SVG)
# ---------------------------------------------------------------------------

class Style:
    __slots__ = ("fill", "fill_opacity", "opacity", "stroke", "stroke_opacity",
                 "stroke_width", "fill_rule", "visible")

    def __init__(self):
        self.fill = ([0, 0, 0], 1.0)   # default: black fill
        self.fill_opacity = 1.0
        self.opacity = 1.0
        self.stroke = None
        self.stroke_opacity = 1.0
        self.stroke_width = 1.0
        self.fill_rule = "nonzero"
        self.visible = True

    def clone(self):
        s = Style()
        for slot in self.__slots__:
            setattr(s, slot, getattr(self, slot))
        return s

    def update(self, elem):
        attrs = dict(elem.attrib)
        attrs.update(parse_style(attrs.get("style")))

        if attrs.get("display") == "none" or attrs.get("visibility") == "hidden":
            self.visible = False

        if "fill" in attrs:
            c = parse_color(attrs["fill"])
            self.fill = c
        if "fill-opacity" in attrs:
            try:
                self.fill_opacity = float(attrs["fill-opacity"])
            except ValueError:
                pass
        if "opacity" in attrs:
            try:
                self.opacity = float(attrs["opacity"])
            except ValueError:
                pass
        if "stroke" in attrs:
            self.stroke = parse_color(attrs["stroke"])
        if "stroke-opacity" in attrs:
            try:
                self.stroke_opacity = float(attrs["stroke-opacity"])
            except ValueError:
                pass
        if "stroke-width" in attrs:
            v = parse_length(attrs["stroke-width"])
            if v is not None:
                self.stroke_width = v
        if "fill-rule" in attrs:
            self.fill_rule = attrs["fill-rule"]
        return attrs


# ---------------------------------------------------------------------------
# Разбор пути ("d" атрибут <path>) -> список подпутей.
# Каждый подпуть: {"closed": bool, "verts": [(x,y),...], "in": [(dx,dy),...], "out":[(dx,dy),...]}
# in/out — относительные смещения контрольных точек от соответствующей вершины.
# ---------------------------------------------------------------------------

TOKEN_RE = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]|" + NUM_RE.pattern)


def tokenize_path(d):
    return TOKEN_RE.findall(d)


def parse_path(d):
    tokens = tokenize_path(d)
    i = 0
    n = len(tokens)

    subpaths = []
    cur = None  # текущий подпуть
    cx = cy = 0.0
    start_x = start_y = 0.0
    prev_cmd = None
    prev_ctrl = None  # для отражения S/T

    def new_subpath(x, y):
        nonlocal cur
        cur = {"closed": False, "verts": [[x, y]], "in": [[0.0, 0.0]], "out": [[0.0, 0.0]]}
        subpaths.append(cur)

    def add_vertex(x, y, in_dx=0.0, in_dy=0.0):
        cur["verts"].append([x, y])
        cur["in"].append([in_dx, in_dy])
        cur["out"].append([0.0, 0.0])

    def set_last_out(dx, dy):
        cur["out"][-1] = [dx, dy]

    while i < n:
        t = tokens[i]
        if t.isalpha():
            cmd = t
            i += 1
        else:
            # повтор предыдущей команды (кроме M->L и Z без аргументов)
            if prev_cmd is None:
                raise ValueError("Некорректный path: число без команды")
            cmd = {"M": "L", "m": "l"}.get(prev_cmd, prev_cmd)
        is_rel = cmd.islower()
        C = cmd.upper()

        def nextf():
            nonlocal i
            v = float(tokens[i])
            i += 1
            return v

        if C == "M":
            x, y = nextf(), nextf()
            if is_rel and cur is not None:
                x += cx
                y += cy
            new_subpath(x, y)
            cx, cy = x, y
            start_x, start_y = x, y
            prev_ctrl = None
            prev_cmd = cmd
            continue
        elif C == "Z":
            if cur is not None:
                cur["closed"] = True
                cx, cy = start_x, start_y
            prev_ctrl = None
            prev_cmd = cmd
            continue
        elif C == "L":
            x, y = nextf(), nextf()
            if is_rel:
                x += cx; y += cy
            add_vertex(x, y)
            cx, cy = x, y
            prev_ctrl = None
        elif C == "H":
            x = nextf()
            if is_rel:
                x += cx
            add_vertex(x, cy)
            cx = x
            prev_ctrl = None
        elif C == "V":
            y = nextf()
            if is_rel:
                y += cy
            add_vertex(cx, y)
            cy = y
            prev_ctrl = None
        elif C == "C":
            x1, y1, x2, y2, x, y = (nextf() for _ in range(6))
            if is_rel:
                x1 += cx; y1 += cy; x2 += cx; y2 += cy; x += cx; y += cy
            set_last_out(x1 - cx, y1 - cy)
            add_vertex(x, y, x2 - x, y2 - y)
            prev_ctrl = (x2, y2)
            cx, cy = x, y
        elif C == "S":
            x2, y2, x, y = (nextf() for _ in range(4))
            if is_rel:
                x2 += cx; y2 += cy; x += cx; y += cy
            if prev_cmd and prev_cmd.upper() in ("C", "S") and prev_ctrl:
                x1, y1 = 2 * cx - prev_ctrl[0], 2 * cy - prev_ctrl[1]
            else:
                x1, y1 = cx, cy
            set_last_out(x1 - cx, y1 - cy)
            add_vertex(x, y, x2 - x, y2 - y)
            prev_ctrl = (x2, y2)
            cx, cy = x, y
        elif C == "Q":
            x1, y1, x, y = (nextf() for _ in range(4))
            if is_rel:
                x1 += cx; y1 += cy; x += cx; y += cy
            # Квадратичная -> кубическая
            c1x, c1y = cx + 2 / 3 * (x1 - cx), cy + 2 / 3 * (y1 - cy)
            c2x, c2y = x + 2 / 3 * (x1 - x), y + 2 / 3 * (y1 - y)
            set_last_out(c1x - cx, c1y - cy)
            add_vertex(x, y, c2x - x, c2y - y)
            prev_ctrl = (x1, y1)
            cx, cy = x, y
        elif C == "T":
            x, y = nextf(), nextf()
            if is_rel:
                x += cx; y += cy
            if prev_cmd and prev_cmd.upper() in ("Q", "T") and prev_ctrl:
                x1, y1 = 2 * cx - prev_ctrl[0], 2 * cy - prev_ctrl[1]
            else:
                x1, y1 = cx, cy
            c1x, c1y = cx + 2 / 3 * (x1 - cx), cy + 2 / 3 * (y1 - cy)
            c2x, c2y = x + 2 / 3 * (x1 - x), y + 2 / 3 * (y1 - y)
            set_last_out(c1x - cx, c1y - cy)
            add_vertex(x, y, c2x - x, c2y - y)
            prev_ctrl = (x1, y1)
            cx, cy = x, y
        elif C == "A":
            rx, ry, rot, laf, sf, x, y = (nextf() for _ in range(7))
            if is_rel:
                x += cx; y += cy
            segs = arc_to_beziers(cx, cy, rx, ry, rot, laf, sf, x, y)
            for (c1x, c1y, c2x, c2y, ex, ey) in segs:
                set_last_out(c1x - cx, c1y - cy)
                add_vertex(ex, ey, c2x - ex, c2y - ey)
                cx, cy = ex, ey
            prev_ctrl = None
        else:
            raise ValueError(f"Неподдерживаемая команда path: {cmd}")
        prev_cmd = cmd

    return subpaths


def arc_to_beziers(x1, y1, rx, ry, phi_deg, large_arc, sweep, x2, y2):
    """Стандартный алгоритм endpoint->center (спецификация SVG 1.1, F.6),
    возвращает список кубических сегментов Безье, аппроксимирующих дугу."""
    if rx == 0 or ry == 0 or (x1 == x2 and y1 == y2):
        return [(x1, y1, x2, y2, x2, y2)]

    rx, ry = abs(rx), abs(ry)
    phi = math.radians(phi_deg % 360)
    cos_phi, sin_phi = math.cos(phi), math.sin(phi)

    dx2, dy2 = (x1 - x2) / 2.0, (y1 - y2) / 2.0
    x1p = cos_phi * dx2 + sin_phi * dy2
    y1p = -sin_phi * dx2 + cos_phi * dy2

    lam = (x1p ** 2) / (rx ** 2) + (y1p ** 2) / (ry ** 2)
    if lam > 1:
        s = math.sqrt(lam)
        rx *= s
        ry *= s

    num = rx ** 2 * ry ** 2 - rx ** 2 * y1p ** 2 - ry ** 2 * x1p ** 2
    den = rx ** 2 * y1p ** 2 + ry ** 2 * x1p ** 2
    co = math.sqrt(max(num / den, 0)) if den != 0 else 0
    if large_arc == sweep:
        co = -co
    cxp = co * (rx * y1p) / ry
    cyp = -co * (ry * x1p) / rx

    cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0
    cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0

    def angle(ux, uy, vx, vy):
        dot = ux * vx + uy * vy
        length = math.sqrt((ux ** 2 + uy ** 2) * (vx ** 2 + vy ** 2))
        a = math.acos(max(-1, min(1, dot / length))) if length != 0 else 0
        if ux * vy - uy * vx < 0:
            a = -a
        return a

    theta1 = angle(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry)
    dtheta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry)
    if not sweep and dtheta > 0:
        dtheta -= 2 * math.pi
    if sweep and dtheta < 0:
        dtheta += 2 * math.pi

    # Разбиваем на сегменты не более 90 градусов
    n_segs = max(1, int(math.ceil(abs(dtheta) / (math.pi / 2))))
    seg_ang = dtheta / n_segs
    alpha = 4.0 / 3.0 * math.tan(seg_ang / 4.0)

    segs = []
    theta = theta1
    for _ in range(n_segs):
        theta_next = theta + seg_ang
        c_t, s_t = math.cos(theta), math.sin(theta)
        c_tn, s_tn = math.cos(theta_next), math.sin(theta_next)

        def ellipse_pt(ct, st):
            ex = cx + rx * ct * cos_phi - ry * st * sin_phi
            ey = cy + rx * ct * sin_phi + ry * st * cos_phi
            return ex, ey

        def ellipse_deriv(ct, st):
            dx = -rx * st * cos_phi - ry * ct * sin_phi
            dy = -rx * st * sin_phi + ry * ct * cos_phi
            return dx, dy

        p0x, p0y = ellipse_pt(c_t, s_t)
        p3x, p3y = ellipse_pt(c_tn, s_tn)
        d0x, d0y = ellipse_deriv(c_t, s_t)
        d3x, d3y = ellipse_deriv(c_tn, s_tn)

        p1x, p1y = p0x + alpha * d0x, p0y + alpha * d0y
        p2x, p2y = p3x - alpha * d3x, p3y - alpha * d3y

        segs.append((p1x, p1y, p2x, p2y, p3x, p3y))
        theta = theta_next

    return segs


def ellipse_subpath(cxv, cyv, rx, ry):
    """Аппроксимация эллипса 4 кубическими кривыми Безье (kappa)."""
    k = 0.5522847498307936
    verts = [
        [cxv + rx, cyv], [cxv, cyv + ry], [cxv - rx, cyv], [cxv, cyv - ry],
    ]
    outs = [
        [0, ry * k], [-rx * k, 0], [0, -ry * k], [rx * k, 0],
    ]
    ins = [
        [0, -ry * k], [rx * k, 0], [0, ry * k], [-rx * k, 0],
    ]
    return {"closed": True, "verts": verts, "in": ins, "out": outs}


def rect_subpath(x, y, w, h, rx, ry):
    if not rx and not ry:
        verts = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
        zero = [[0, 0]] * 4
        return {"closed": True, "verts": verts, "in": list(zero), "out": list(zero)}
    rx = rx or ry
    ry = ry or rx
    rx = min(rx, w / 2.0)
    ry = min(ry, h / 2.0)
    k = 0.5522847498307936
    verts = [
        [x + rx, y], [x + w - rx, y],
        [x + w, y + ry], [x + w, y + h - ry],
        [x + w - rx, y + h], [x + rx, y + h],
        [x, y + h - ry], [x, y + ry],
    ]
    ins = [
        [0, 0], [-rx * k, 0],
        [0, -ry * k], [0, 0],
        [rx * k, 0], [0, 0],
        [0, ry * k], [0, 0],
    ]
    outs = [
        [0, 0], [rx * k, 0],
        [0, 0], [0, ry * k],
        [0, 0], [-rx * k, 0],
        [0, 0], [0, -ry * k],
    ]
    return {"closed": True, "verts": verts, "in": ins, "out": outs}


def poly_subpath(points_str, closed):
    nums = [float(x) for x in NUM_RE.findall(points_str)]
    pts = [[nums[i], nums[i + 1]] for i in range(0, len(nums) - 1, 2)]
    zero = [[0, 0] for _ in pts]
    return {"closed": closed, "verts": pts, "in": zero, "out": [list(z) for z in zero]}


# ---------------------------------------------------------------------------
# Основной конвертер
# ---------------------------------------------------------------------------

class SVGtoLottie:
    def __init__(self, canvas_size=512, fps=60, duration=1.0, bg=None):
        self.canvas = canvas_size
        self.fps = fps
        self.duration = duration
        self.bg = bg
        self.draw_items = []  # список в порядке документа: (subpaths, fill, stroke, stroke_width, opacity, name)

    def convert_file(self, path):
        tree = ET.parse(path)
        root = tree.getroot()
        self._process_root(root)
        return self.build_lottie()

    def convert_string(self, svg_text):
        root = ET.fromstring(svg_text)
        self._process_root(root)
        return self.build_lottie()

    def _process_root(self, root):
        vb = root.attrib.get("viewBox")
        if vb:
            vals = [float(x) for x in NUM_RE.findall(vb)]
            vb_x, vb_y, vb_w, vb_h = vals
        else:
            w = parse_length(root.attrib.get("width", str(self.canvas))) or self.canvas
            h = parse_length(root.attrib.get("height", str(self.canvas))) or self.canvas
            vb_x, vb_y, vb_w, vb_h = 0.0, 0.0, w, h

        scale = min(self.canvas / vb_w, self.canvas / vb_h) if vb_w and vb_h else 1.0
        off_x = (self.canvas - vb_w * scale) / 2.0
        off_y = (self.canvas - vb_h * scale) / 2.0

        base = mat_mul((1, 0, 0, 1, off_x, off_y), (scale, 0, 0, scale, 0, 0))
        base = mat_mul(base, (1, 0, 0, 1, -vb_x, -vb_y))

        style = Style()
        self._walk(root, base, style)

    def _walk(self, elem, matrix, style):
        tag = elem.tag.split("}")[-1]
        if tag in ("defs", "clipPath", "mask", "symbol", "title", "desc", "metadata"):
            return  # не поддерживаем градиенты/клипы — пропускаем как безопасный фолбэк

        local_style = style.clone()
        attrs = local_style.update(elem)

        local_matrix = mat_mul(matrix, parse_transform(attrs.get("transform")))

        if not local_style.visible:
            return

        if tag == "g" or tag == "svg" or tag == "a":
            for child in elem:
                self._walk(child, local_matrix, local_style)
            return

        subpaths = None
        if tag == "path" and "d" in attrs:
            try:
                subpaths = parse_path(attrs["d"])
            except Exception:
                subpaths = None
        elif tag == "rect":
            x = parse_length(attrs.get("x", "0")) or 0.0
            y = parse_length(attrs.get("y", "0")) or 0.0
            w = parse_length(attrs.get("width", "0")) or 0.0
            h = parse_length(attrs.get("height", "0")) or 0.0
            rx = parse_length(attrs.get("rx")) if attrs.get("rx") else None
            ry = parse_length(attrs.get("ry")) if attrs.get("ry") else None
            if w > 0 and h > 0:
                subpaths = [rect_subpath(x, y, w, h, rx or 0, ry or 0)]
        elif tag == "circle":
            cx = parse_length(attrs.get("cx", "0")) or 0.0
            cy = parse_length(attrs.get("cy", "0")) or 0.0
            r = parse_length(attrs.get("r", "0")) or 0.0
            if r > 0:
                subpaths = [ellipse_subpath(cx, cy, r, r)]
        elif tag == "ellipse":
            cx = parse_length(attrs.get("cx", "0")) or 0.0
            cy = parse_length(attrs.get("cy", "0")) or 0.0
            rx = parse_length(attrs.get("rx", "0")) or 0.0
            ry = parse_length(attrs.get("ry", "0")) or 0.0
            if rx > 0 and ry > 0:
                subpaths = [ellipse_subpath(cx, cy, rx, ry)]
        elif tag in ("polygon", "polyline") and "points" in attrs:
            subpaths = [poly_subpath(attrs["points"], closed=(tag == "polygon"))]
        elif tag == "line":
            x1 = parse_length(attrs.get("x1", "0")) or 0.0
            y1 = parse_length(attrs.get("y1", "0")) or 0.0
            x2 = parse_length(attrs.get("x2", "0")) or 0.0
            y2 = parse_length(attrs.get("y2", "0")) or 0.0
            subpaths = [{"closed": False, "verts": [[x1, y1], [x2, y2]],
                         "in": [[0, 0], [0, 0]], "out": [[0, 0], [0, 0]]}]
            local_style.fill = None  # у линии обычно нет заливки

        if subpaths:
            baked = []
            for sp in subpaths:
                verts = [mat_point(local_matrix, x, y) for x, y in sp["verts"]]
                ins = [mat_vector(local_matrix, x, y) for x, y in sp["in"]]
                outs = [mat_vector(local_matrix, x, y) for x, y in sp["out"]]
                baked.append({"closed": sp["closed"], "verts": verts, "in": ins, "out": outs})

            total_opacity = local_style.opacity
            fill = local_style.fill
            fill_op = (fill[1] * local_style.fill_opacity * total_opacity) if fill else 0
            stroke = local_style.stroke
            stroke_op = (stroke[1] * local_style.stroke_opacity * total_opacity) if stroke else 0

            self.draw_items.append({
                "subpaths": baked,
                "fill_color": fill[0] if fill else None,
                "fill_opacity": fill_op,
                "stroke_color": stroke[0] if stroke else None,
                "stroke_opacity": stroke_op,
                "stroke_width": local_style.stroke_width,
                "fill_rule": local_style.fill_rule,
                "name": attrs.get("id") or tag,
            })

    # ------------------------------------------------------------------
    def build_lottie(self):
        op_frame = max(2, round(self.fps * self.duration))
        layers = []
        # SVG: последующие элементы рисуются поверх. В Lottie layers[0] — самый верхний слой.
        for idx, item in enumerate(reversed(self.draw_items)):
            shapes = []
            for sp in item["subpaths"]:
                verts = [list(v) for v in sp["verts"]]
                ins = [list(v) for v in sp["in"]]
                outs = [list(v) for v in sp["out"]]
                shapes.append({
                    "ty": "sh",
                    "ks": {"a": 0, "k": {"c": sp["closed"], "i": ins, "o": outs, "v": verts}},
                    "nm": "path",
                })
            if item["fill_color"] is not None and item["fill_opacity"] > 0:
                shapes.append({
                    "ty": "fl",
                    "c": {"a": 0, "k": item["fill_color"] + [1]},
                    "o": {"a": 0, "k": item["fill_opacity"] * 100},
                    "r": 1 if item["fill_rule"] == "evenodd" else 1,
                    "nm": "fill",
                })
            if item["stroke_color"] is not None and item["stroke_opacity"] > 0:
                shapes.append({
                    "ty": "st",
                    "c": {"a": 0, "k": item["stroke_color"] + [1]},
                    "o": {"a": 0, "k": item["stroke_opacity"] * 100},
                    "w": {"a": 0, "k": item["stroke_width"]},
                    "lc": 2, "lj": 2,
                    "nm": "stroke",
                })
            if not shapes:
                continue
            shapes.append({
                "ty": "tr",
                "p": {"a": 0, "k": [0, 0]},
                "a": {"a": 0, "k": [0, 0]},
                "s": {"a": 0, "k": [100, 100]},
                "r": {"a": 0, "k": 0},
                "o": {"a": 0, "k": 100},
            })
            layers.append({
                "ddd": 0, "ty": 4, "nm": item["name"],
                "sr": 1,
                "ks": {
                    "o": {"a": 0, "k": 100},
                    "p": {"a": 0, "k": [0, 0, 0]},
                    "a": {"a": 0, "k": [0, 0, 0]},
                    "s": {"a": 0, "k": [100, 100, 100]},
                    "r": {"a": 0, "k": 0},
                },
                "ao": 0,
                "shapes": [{"ty": "gr", "it": shapes, "nm": item["name"]}],
                "ip": 0, "op": op_frame, "st": 0, "bm": 0,
                "ind": idx + 1,
            })

        if self.bg:
            color, _ = parse_color(self.bg)
            layers.append({
                "ddd": 0, "ty": 1, "nm": "background",
                "sr": 1,
                "ks": {
                    "o": {"a": 0, "k": 100},
                    "p": {"a": 0, "k": [self.canvas / 2, self.canvas / 2, 0]},
                    "a": {"a": 0, "k": [0, 0, 0]},
                    "s": {"a": 0, "k": [100, 100, 100]},
                    "r": {"a": 0, "k": 0},
                },
                "sw": self.canvas, "sh": self.canvas, "sc": color and "#%02x%02x%02x" % tuple(
                    round(c * 255) for c in color) or "#ffffff",
                "ip": 0, "op": op_frame, "st": 0, "bm": 0,
                "ind": 0,
            })

        lottie = {
            "v": "5.7.4",
            "fr": self.fps,
            "ip": 0,
            "op": op_frame,
            "w": self.canvas,
            "h": self.canvas,
            "nm": "svg2tgs",
            "ddd": 0,
            "assets": [],
            "layers": layers,
        }
        return lottie


def convert_svg_to_tgs(svg_path, tgs_path, size=512, fps=60, duration=1.0, bg=None):
    conv = SVGtoLottie(canvas_size=size, fps=fps, duration=duration, bg=bg)
    lottie = conv.convert_file(svg_path)
    data = json.dumps(lottie, separators=(",", ":")).encode("utf-8")
    with gzip.GzipFile(tgs_path, "wb", mtime=0) as f:
        f.write(data)
    return len(data), os.path.getsize(tgs_path)


# ---------------------------------------------------------------------------
# CLI / пакетная обработка
# ---------------------------------------------------------------------------

def collect_inputs(input_arg):
    if os.path.isdir(input_arg):
        return sorted(glob.glob(os.path.join(input_arg, "*.svg")))
    expanded = glob.glob(input_arg)
    if expanded:
        return sorted(expanded)
    if os.path.isfile(input_arg):
        return [input_arg]
    return []


def main():
    ap = argparse.ArgumentParser(description="Конвертация SVG в Telegram TGS (batch-режим).")
    ap.add_argument("input", help="SVG-файл, папка с SVG или маска (*.svg)")
    ap.add_argument("-o", "--output", required=True,
                     help="Выходной .tgs файл (для одного входного файла) "
                          "или папка (для батча/маски/папки)")
    ap.add_argument("--size", type=int, default=512, help="сторона канваса (по умолчанию 512)")
    ap.add_argument("--fps", type=int, default=60, help="кадров в секунду (по умолчанию 60)")
    ap.add_argument("--duration", type=float, default=1.0, help="длительность, сек (по умолчанию 1.0)")
    ap.add_argument("--bg", default=None, help="цвет фона (например #ffffff), по умолчанию прозрачный")
    ap.add_argument("--max-size", type=int, default=64, help="порог предупреждения о размере файла, КБ (по умолчанию 64)")
    args = ap.parse_args()

    inputs = collect_inputs(args.input)
    if not inputs:
        print(f"Не найдено ни одного SVG по пути/маске: {args.input}", file=sys.stderr)
        sys.exit(1)

    batch_mode = len(inputs) > 1 or os.path.isdir(args.input) or ("*" in args.input or "?" in args.input)

    if batch_mode:
        os.makedirs(args.output, exist_ok=True)

    ok, failed = 0, 0
    for src in inputs:
        if batch_mode:
            base = os.path.splitext(os.path.basename(src))[0]
            dst = os.path.join(args.output, base + ".tgs")
        else:
            dst = args.output
            out_dir = os.path.dirname(dst)
            if out_dir:
                os.makedirs(out_dir, exist_ok=True)

        try:
            raw_size, gz_size = convert_svg_to_tgs(
                src, dst, size=args.size, fps=args.fps, duration=args.duration, bg=args.bg
            )
            warn = ""
            if gz_size > args.max_size * 1024:
                warn = f"  [ПРЕВЫШЕН лимит {args.max_size} КБ: {gz_size / 1024:.1f} КБ]"
            print(f"OK   {src} -> {dst}  ({gz_size / 1024:.1f} КБ){warn}")
            ok += 1
        except Exception as e:
            print(f"FAIL {src}: {e}", file=sys.stderr)
            failed += 1

    print(f"\nГотово: {ok} успешно, {failed} с ошибкой.")
    if failed:
        sys.exit(2)


if __name__ == "__main__":
    main()

"""Собирает значки приложения из двух исходников.

app/app-icon.svg       — основная иконка (48 px и больше);
app/app-icon-small.svg — упрощённая для 16–32 px (толстые линии, без узлов).

Запуск из корня репозитория (нужны Node и Python с Pillow):
    python tools/icons.py
"""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "app"
ICONS = APP / "src-tauri" / "icons"
KEEP = {"32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico", "icon.icns"}


def tauri_icon(svg: str, out: Path) -> None:
    npm = "npm.cmd" if sys.platform == "win32" else "npm"
    subprocess.run([npm, "run", "tauri", "--", "icon", svg, "-o", str(out)], cwd=APP, check=True)


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        big, small = Path(tmp, "big"), Path(tmp, "small")
        tauri_icon("app-icon.svg", big)
        tauri_icon("app-icon-small.svg", small)
        for name in KEEP:
            shutil.copy(big / name, ICONS / name)
        # Мелкие размеры — из упрощённой иконки.
        shutil.copy(small / "32x32.png", ICONS / "32x32.png")
        s, b = Image.open(small / "icon.ico"), Image.open(big / "icon.ico")
        images = [s.ico.getimage((n, n)).convert("RGBA") for n in (16, 24, 32)]
        images += [b.ico.getimage((n, n)).convert("RGBA") for n in (48, 64, 256)]
        images[-1].save(
            ICONS / "icon.ico",
            format="ICO",
            sizes=[(i.width, i.height) for i in images],
            append_images=images[:-1],
        )
    print("Готово:", ", ".join(sorted(KEEP)))


if __name__ == "__main__":
    main()

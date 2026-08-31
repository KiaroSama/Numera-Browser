"""Generate the icon set from the brand logo.

Run from the repository root:  python scripts/make-icons.py

Everything it writes is derived from `docs/brand/Logo.png`, so the outputs are
regenerable and the intermediate sizes are git-ignored - only the source, the
1024 master and the one the README uses are tracked.

One source of truth (`docs/brand/Logo.png`) rendered to every size the project
actually consumes: the 1024 master, the Tauri window/installer icons, and a
favicon for the dashboard - which had none, so a browser tab showed the
default document glyph.
"""
import io
from pathlib import Path

from PIL import Image

ROOT = Path(".")
SRC = ROOT / "docs" / "brand" / "Logo.png"
src = Image.open(SRC).convert("RGBA")
print(f"source {SRC} {src.size}")


def square(size: int) -> Image.Image:
    """Resize onto a transparent square, preserving aspect ratio.

    The source is already square, but centring on a canvas means a future logo
    that is not square still produces correct icons instead of a stretched one.
    """
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    w, h = src.size
    scale = min(size / w, size / h)
    resized = src.resize((max(1, round(w * scale)), max(1, round(h * scale))),
                         Image.LANCZOS)
    out.paste(resized, ((size - resized.width) // 2, (size - resized.height) // 2))
    return out


brand = ROOT / "docs" / "brand"
written = []

# The master the brief asked for, plus the sizes GitHub and the docs use.
for size in (1024, 512, 256, 128):
    p = brand / f"logo-{size}.png"
    square(size).save(p, "PNG", optimize=True)
    written.append((p, size))

# Tauri: keep the existing .ico name so tauri.conf.json needs no change, and add
# the PNGs Tauri wants on Linux/macOS builds.
icons = ROOT / "desktop" / "src-tauri" / "icons"
icons.mkdir(parents=True, exist_ok=True)
square(256).save(icons / "icon.ico", "ICO",
                 sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
written.append((icons / "icon.ico", "multi"))
for name, size in [("32x32.png", 32), ("128x128.png", 128),
                   ("128x128@2x.png", 256), ("icon.png", 512)]:
    p = icons / name
    square(size).save(p, "PNG", optimize=True)
    written.append((p, size))

# The dashboard had no favicon at all.
pub = ROOT / "dashboard" / "public"
pub.mkdir(parents=True, exist_ok=True)
square(256).save(pub / "favicon.ico", "ICO",
                 sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
square(180).save(pub / "apple-touch-icon.png", "PNG", optimize=True)
square(512).save(pub / "logo-512.png", "PNG", optimize=True)
written += [(pub / "favicon.ico", "multi"), (pub / "apple-touch-icon.png", 180),
            (pub / "logo-512.png", 512)]

for p, size in written:
    print(f"  {str(p):48} {size:>6}  {p.stat().st_size // 1024:>5} KB")

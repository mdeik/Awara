#!/usr/bin/env python3
"""Generate platform icon files from the SVG source (single source of truth).

Windows: .ico — multi-resolution (16/24/32/48/64/128/256 px)
macOS:   .icns — multi-resolution (16/32/64/128/256/512/1024 px)

Requires: rsvg-convert (librsvg), ImageMagick, Python + Pillow.
  Ubuntu/Debian:  apt install librsvg2-bin imagemagick python3-pil
  macOS (Homebrew): brew install librsvg imagemagick python-pillow
"""

import subprocess
import tempfile
import shutil
import sys
import os
from pathlib import Path
from PIL import Image

# ── paths ──────────────────────────────────────────────────────────────
REPO = Path(__file__).resolve().parent.parent
SVG = REPO / "resources" / "icons" / "awara.svg"
ICO_OUT = REPO / "resources" / "windows" / "awara.ico"
ICNS_OUT = REPO / "resources" / "macos" / "awara.icns"

# ── icon sizes ─────────────────────────────────────────────────────────
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICNS_SIZES = [16, 32, 64, 128, 256, 512, 1024]


def check_deps() -> None:
    """Verify required tools are available."""
    missing = []
    for name in ("rsvg-convert", "magick", "convert"):
        if not shutil.which(name):
            missing.append(name)
    if missing:
        print("Missing dependencies: " + ", ".join(missing), file=sys.stderr)
        print("Install with:", file=sys.stderr)
        print("  apt install librsvg2-bin imagemagick   # Debian/Ubuntu", file=sys.stderr)
        print("  brew install librsvg imagemagick        # macOS", file=sys.stderr)
        sys.exit(1)


def svg_to_png(size: int, dest: Path) -> None:
    """Render the SVG at *size* px (square) to a PNG file."""
    subprocess.run(
        ["rsvg-convert", "-w", str(size), "-h", str(size),
         "-o", str(dest), str(SVG)],
        check=True, capture_output=True)


def generate_ico(png_dir: Path) -> None:
    """Assemble multi-resolution .ico from pre-rendered PNGs."""
    # ImageMagick's icon:auto-resize handles the ICO container format properly.
    # We feed it the largest PNG and let it resize down for all requested sizes.
    largest = png_dir / f"{ICO_SIZES[-1]}.png"
    sizes_arg = ",".join(str(s) for s in ICO_SIZES)
    subprocess.run(
        ["magick", str(largest),
         "-background", "none",
         "-define", f"icon:auto-resize={sizes_arg}",
         str(ICO_OUT)],
        check=True, capture_output=True)
    print(f"  ✓ {ICO_OUT}  ({', '.join(f'{s}×{s}' for s in ICO_SIZES)})")


def generate_icns(png_dir: Path) -> None:
    """Assemble multi-resolution .icns from pre-rendered PNGs."""
    # Pillow's ICNS encoder accepts a list of (size, image) pairs.
    images = []
    for size in ICNS_SIZES:
        path = png_dir / f"{size}.png"
        im = Image.open(path).convert("RGBA")
        images.append((size, im))

    # Pillow saves ICNS using the first image's size as the primary icon,
    # and includes the rest as alternative icon resources.
    primary = images[0]
    rest = images[1:]
    primary[1].save(
        str(ICNS_OUT),
        format="ICNS",
        append_images=[im for _, im in rest],
    )
    print(f"  ✓ {ICNS_OUT}  ({', '.join(f'{s}×{s}' for s in ICNS_SIZES)})")


def main() -> None:
    check_deps()

    if not SVG.exists():
        print(f"Error: source SVG not found: {SVG}", file=sys.stderr)
        sys.exit(1)

    print(f"Source: {SVG}")

    with tempfile.TemporaryDirectory(prefix="awara_icons_") as tmp:
        png_dir = Path(tmp)

        # Render PNGs at each unique size needed
        needed = set(ICO_SIZES + ICNS_SIZES)
        for size in sorted(needed):
            dest = png_dir / f"{size}.png"
            svg_to_png(size, dest)
            print(f"  · rendered {size}×{size}")

        generate_ico(png_dir)
        generate_icns(png_dir)

        for size in sorted(needed):
            os.remove(png_dir / f"{size}.png")

    print("Done.")


if __name__ == "__main__":
    main()

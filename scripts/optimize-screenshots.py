#!/usr/bin/env python3
"""Shrinks the PNGs in docs/screenshots: 256 adaptive colours, no dithering.

Interface screenshots have few distinct colours, so this makes them about four times smaller
and looks the same. Run it after `docs_shots` redraws the pictures (needs Pillow):

    python3 scripts/optimize-screenshots.py [folder]
"""
import os
import sys

from PIL import Image

folder = sys.argv[1] if len(sys.argv) > 1 else "docs/screenshots"
before = after = 0
for name in sorted(os.listdir(folder)):
    if not name.endswith(".png"):
        continue
    path = os.path.join(folder, name)
    before += os.path.getsize(path)
    image = Image.open(path).convert("RGB")
    image.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(
        path, optimize=True
    )
    after += os.path.getsize(path)
print(f"{before // 1024} KiB -> {after // 1024} KiB")

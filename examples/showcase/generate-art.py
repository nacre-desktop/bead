#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Regenerate wallpaper.jpg, view/art.png and palette.json (python3 generate-art.py, needs Pillow).

All three are procedural and deterministic (seed 7): no photo or third-party image is used. The palette
roles are picked from the wallpaper's own colours, so the UI follows the picture.
"""
import colorsys
import json
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

here = Path(__file__).resolve().parent
W, H = 2560, 1440
rng = random.Random(7)

# Soft colour blobs on a dark vertical gradient, drawn at 1/8 size and blurred, then scaled up.
s = 8
small = Image.new("RGB", (W // s, H // s))
px = small.load()
for y in range(H // s):
    t = y / (H // s - 1)
    for x in range(W // s):
        px[x, y] = (int(14 - 4 * t), int(16 + 26 * t), int(42 + 14 * t))
blobs = [(0.15, 0.2, 0.34, (120, 78, 224)), (0.78, 0.25, 0.30, (40, 200, 170)), (0.62, 0.88, 0.40, (230, 76, 150)),
         (0.30, 0.9, 0.26, (255, 150, 92)), (0.95, 0.75, 0.22, (70, 120, 240))]
draw = ImageDraw.Draw(small)
for cx, cy, r, colour in blobs:
    r *= W // s
    draw.ellipse((cx * W // s - r, cy * H // s - r, cx * W // s + r, cy * H // s + r), fill=colour)
small = small.filter(ImageFilter.GaussianBlur(22))
wall = small.resize((W, H), Image.BICUBIC)
# A few thin rings for structure.
rings = Image.new("RGBA", (W, H), (0, 0, 0, 0))
rd = ImageDraw.Draw(rings)
for cx, cy, r in [(0.7, 0.45, 520), (0.7, 0.45, 760), (0.7, 0.45, 1080), (0.2, 0.8, 420)]:
    rd.ellipse((cx * W - r, cy * H - r, cx * W + r, cy * H + r), outline=(255, 255, 255, 26), width=3)
wall = Image.alpha_composite(wall.convert("RGBA"), rings.filter(ImageFilter.GaussianBlur(1.2))).convert("RGB")
wall.save(here / "wallpaper.jpg", quality=90, optimize=True)
wall.crop((1450, 150, 2250, 950)).resize((256, 256), Image.LANCZOS).save(here / "view" / "art.png", optimize=True)

# Palette: the most saturated bright colour of the wallpaper is the accent; its darkest colour tints the surfaces.
quant = wall.resize((160, 90)).quantize(8).convert("RGB")
colours = [c for _, c in sorted(quant.getcolors(), reverse=True)]
hsv = [colorsys.rgb_to_hsv(*(v / 255 for v in c)) for c in colours]
accent = max((c for c in hsv if c[2] > 0.5), key=lambda c: c[1])[0]
tint = min(hsv, key=lambda c: c[2])[0]


def hexc(h, sat, val):
    return "".join(f"{round(v * 255):02x}" for v in colorsys.hsv_to_rgb(h % 1, sat, val))


roles = {
    "surface": hexc(tint, .45, .10), "surfaceContainer": hexc(tint, .40, .16), "surfaceContainerHigh": hexc(tint, .36, .21),
    "surfaceContainerHighest": hexc(tint, .32, .26), "onSurface": hexc(tint, .06, .94), "onSurfaceVariant": hexc(tint, .14, .78),
    "outline": hexc(tint, .16, .54), "outlineVariant": hexc(tint, .22, .34),
    "primary": hexc(accent, .42, .96), "onPrimary": hexc(accent, .85, .16), "primaryContainer": hexc(accent, .55, .34),
    "onPrimaryContainer": hexc(accent, .18, .94), "secondary": hexc(accent + .1, .3, .86),
    "error": "f2b8b5", "onError": "601410",
}
(here / "palette.json").write_text(json.dumps({"formatVersion": 1, "mode": "dark", "colours": roles}, indent=1) + "\n")

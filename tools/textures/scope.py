"""The scope rings: black outside a round glass, a soft shadow inside its edge, a reticle, and bullet
drop ticks worked out from each gun's numbers in assets/data/items.ron (speed, gravity, zoom).

    python3 tools/textures/scope.py     writes assets/textures/panga/scope_{2,4,6}x.png
"""

import math
import os

import numpy as np
from PIL import Image, ImageDraw

SIZE = 1024
SS = 3
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "assets", "textures", "panga")
# zoom: (bullet speed, gravity) of the gun that carries it.
GUNS = {2: (280.0, 5.0), 4: (400.0, 6.0), 6: (300.0, 9.0)}


def smooth(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def lines(zoom):
    """The reticle, drawn large then shrunk: white alpha mask plus a red dot."""
    s = SIZE * SS
    c = s / 2
    speed, gravity = GUNS[zoom]
    per_rad = s / (2 * math.atan(math.tan(math.radians(35)) / zoom))
    dark = Image.new("L", (s, s), 0)
    d = ImageDraw.Draw(dark)
    w = 2 * SS
    if zoom == 2:
        d.ellipse((c - 0.14 * s, c - 0.14 * s, c + 0.14 * s, c + 0.14 * s), outline=255, width=w)
        for dx, dy in ((1, 0), (-1, 0), (0, 1)):
            d.line((c + dx * 0.14 * s, c + dy * 0.14 * s, c + dx * 0.05 * s, c + dy * 0.05 * s), fill=255, width=w)
    else:
        post = 7 * SS
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            d.line((c + dx * 0.012 * s, c + dy * 0.012 * s, c + dx * 0.2 * s, c + dy * 0.2 * s), fill=255, width=w)
            d.line((c + dx * 0.2 * s, c + dy * 0.2 * s, c + dx * 0.49 * s, c + dy * 0.49 * s), fill=255, width=post)
    # Hold-over marks: 100, 200 and 300 m, on the gun's own drop.
    for k, metres in enumerate((100, 200, 300)):
        t = metres / speed
        y = c + per_rad * (0.5 * gravity * t * t) / metres
        half = (0.016 - 0.003 * k) * s
        d.line((c - half, y, c + half, y), fill=255, width=w)
    return dark.resize((SIZE, SIZE), Image.LANCZOS)


def build(zoom):
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)
    r = np.hypot(x - SIZE / 2 + 0.5, y - SIZE / 2 + 0.5) / (SIZE / 2)
    glass = smooth(0.91, 0.95, r)
    shade = smooth(0.62, 0.93, r) ** 2 * 0.55
    ink = np.asarray(lines(zoom), dtype=np.float32) / 255.0
    alpha = np.maximum(np.maximum(glass, shade), ink * 0.92)
    rgb = np.zeros((SIZE, SIZE, 3), dtype=np.float32)
    # A red point at the very centre, for dark backgrounds.
    dot = 1.0 - smooth(0.006, 0.011, r)
    rgb[..., 0] = dot * 0.95
    rgb[..., 1] = dot * 0.1
    rgb[..., 2] = dot * 0.05
    alpha = np.maximum(alpha, dot)
    out = np.dstack([rgb, alpha])
    path = os.path.join(OUT, f"scope_{zoom}x.png")
    Image.fromarray((out * 255).astype(np.uint8), "RGBA").save(path, optimize=True)
    print(path, os.path.getsize(path) // 1024, "KB")


if __name__ == "__main__":
    for z in GUNS:
        build(z)

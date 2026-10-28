"""Procedurally drawn foliage and bark textures (numpy), so trees need no downloaded images.

Leaf atlas (RGBA, 3072x2048), six 1024 tiles (3 columns x 2 rows):
  0 neem: pinnate sprays of narrow serrated leaflets
  1 peepal: heart-shaped leaves with long drip tips
  2 banyan: broad oval leathery leaves
  3 palm: one frond segment (leaflets angled off a midrib)
  4 lantana: small rough leaves with clusters of orange/pink flowers
  5 bougainvillea: papery magenta bracts among leaves
"""

import math
import os

import bpy
import numpy as np

QUADRANTS = {"neem": 0, "peepal": 1, "banyan": 2, "palm": 3, "lantana": 4, "bougainvillea": 5}
COLS, ROWS = 3, 2


def quadrant_uv(name):
    """(u0, v0, u1, v1) of a species' atlas tile (UV origin bottom-left, tiles counted from top-left)."""
    q = QUADRANTS[name]
    u0 = (q % COLS) / COLS
    v0 = 1.0 - (q // COLS + 1) / ROWS
    return u0, v0, u0 + 1.0 / COLS, v0 + 1.0 / ROWS


def _leaf(canvas, alpha, cx, cy, angle, length, width, color, profile, rng, vein=0.35):
    """Rasterise one leaf (pixel units) into canvas/alpha with a shape profile w(t), t in 0..1 base->tip."""
    h, w = alpha.shape
    r = int(length * 0.6 + width) + 2
    x0, x1 = max(0, int(cx - r)), min(w, int(cx + r))
    y0, y1 = max(0, int(cy - r)), min(h, int(cy + r))
    if x0 >= x1 or y0 >= y1:
        return
    ys, xs = np.mgrid[y0:y1, x0:x1].astype(np.float32)
    ca, sa = math.cos(angle), math.sin(angle)
    dx, dy = xs - cx, ys - cy
    t = (dx * ca + dy * sa) / length          # along the leaf, 0 at the stem
    s = (-dx * sa + dy * ca) / width          # across, -1..1 at the widest point
    half = profile(np.clip(t, 0, 1))
    inside = (t >= 0) & (t <= 1) & (np.abs(s) <= half)
    if not inside.any():
        return
    # Shading: darker toward the edge and along the midrib, lighter blade.
    edge = np.clip(1 - np.abs(s) / np.maximum(half, 1e-3), 0, 1)
    shade = 0.75 + 0.25 * edge
    rib = np.exp(-(s / 0.06) ** 2) * (t < 0.95)
    col = np.array(color, dtype=np.float32) * (0.9 + 0.2 * rng.random())
    tile = canvas[y0:y1, x0:x1]
    a = alpha[y0:y1, x0:x1]
    for c in range(3):
        v = col[c] * shade * (1 - vein * 0.5 * rib) + vein * 0.12 * rib
        tile[..., c] = np.where(inside, v, tile[..., c])
    a[...] = np.where(inside, 1.0, a)


def _stem(canvas, alpha, xa, ya, xb, yb, thick, color):
    h, w = alpha.shape
    x0, x1 = max(0, int(min(xa, xb) - thick - 1)), min(w, int(max(xa, xb) + thick + 2))
    y0, y1 = max(0, int(min(ya, yb) - thick - 1)), min(h, int(max(ya, yb) + thick + 2))
    ys, xs = np.mgrid[y0:y1, x0:x1].astype(np.float32)
    vx, vy = xb - xa, yb - ya
    L2 = vx * vx + vy * vy + 1e-6
    t = np.clip(((xs - xa) * vx + (ys - ya) * vy) / L2, 0, 1)
    d = np.hypot(xs - (xa + t * vx), ys - (ya + t * vy))
    inside = d <= thick
    for c in range(3):
        canvas[y0:y1, x0:x1, c] = np.where(inside, color[c], canvas[y0:y1, x0:x1, c])
    alpha[y0:y1, x0:x1] = np.where(inside, 1.0, alpha[y0:y1, x0:x1])


def _flower_cluster(canvas, alpha, cx, cy, radius, colors, rng, petals=5, n=14):
    """A rounded head of tiny 4-5 petal florets (lantana) or papery bracts."""
    for _ in range(n):
        a, r = rng.uniform(0, 2 * math.pi), radius * math.sqrt(rng.uniform(0, 1))
        fx, fy = cx + math.cos(a) * r, cy + math.sin(a) * r
        col = colors[rng.integers(len(colors))]
        for k in range(petals):
            pa = 2 * math.pi * k / petals + rng.uniform(0, 0.5)
            _leaf(canvas, alpha, fx, fy, pa, radius * 0.32, radius * 0.16, col, lambda t_: np.sin(np.pi * t_) ** 0.5, rng, vein=0.1)


def draw_atlas(path, size=2048, seed=3):
    rng = np.random.default_rng(seed)
    half = size // 2                                   # tile size
    canvas = np.zeros((half * ROWS, half * COLS, 3), dtype=np.float32)
    alpha = np.zeros((half * ROWS, half * COLS), dtype=np.float32)
    stem_col = (0.22, 0.20, 0.10)

    def origin(q):
        return (q % COLS) * half, (q // COLS) * half

    # --- neem: three pinnate sprays per tile
    ox, oy = origin(0)
    for k in range(3):
        bx, by = ox + half * (0.2 + 0.3 * k), oy + half * 0.95
        ang = -math.pi / 2 + (k - 1) * 0.35
        L = half * 0.85
        tx, ty = bx + math.cos(ang) * L, by + math.sin(ang) * L
        _stem(canvas, alpha, bx, by, tx, ty, 3, stem_col)
        n = 9
        for i in range(n):
            t = 0.12 + 0.85 * i / n
            px, py = bx + (tx - bx) * t, by + (ty - by) * t
            for side in (-1, 1):
                la = ang + side * (1.0 - 0.3 * t)
                _leaf(canvas, alpha, px, py, la, half * 0.17 * (1 - 0.35 * t), half * 0.035,
                      (0.16, 0.36, 0.10), lambda t_: np.sin(np.pi * t_ ** 0.8) * (1 - 0.06 * np.sin(t_ * 60)), rng)
        _leaf(canvas, alpha, tx, ty, ang, half * 0.15, half * 0.035, (0.17, 0.38, 0.11), lambda t_: np.sin(np.pi * t_ ** 0.8), rng)

    # --- peepal: heart leaves with drip tips on short stems
    ox, oy = origin(1)
    for _ in range(26):
        cx, cy = ox + rng.uniform(0.12, 0.88) * half, oy + rng.uniform(0.12, 0.88) * half
        ang = rng.uniform(0, 2 * math.pi)
        L = half * rng.uniform(0.22, 0.30)
        heart = lambda t_: np.where(t_ < 0.75, 0.55 + 0.5 * np.sin(np.pi * np.clip(t_ / 0.75, 0, 1) * 0.9) - 0.4 * (1 - np.clip(t_ * 5, 0, 1)),
                                    0.95 * (1 - (t_ - 0.75) / 0.25) ** 1.8)
        _leaf(canvas, alpha, cx, cy, ang, L, L * 0.42, (0.20, 0.42, 0.12), heart, rng)

    # --- banyan: broad ovals, leathery and darker
    ox, oy = origin(2)
    for _ in range(24):
        cx, cy = ox + rng.uniform(0.12, 0.88) * half, oy + rng.uniform(0.12, 0.88) * half
        ang = rng.uniform(0, 2 * math.pi)
        L = half * rng.uniform(0.22, 0.30)
        _leaf(canvas, alpha, cx, cy, ang, L, L * 0.33, (0.12, 0.30, 0.09), lambda t_: np.sin(np.pi * t_) ** 0.7, rng, vein=0.5)

    # --- palm frond segment: midrib up the middle, leaflets angled toward the tip
    ox, oy = origin(3)
    mx = ox + half / 2
    _stem(canvas, alpha, mx, oy + half, mx, oy, 5, (0.45, 0.40, 0.18))
    for i in range(30):
        y = oy + half * (1 - (i + 0.5) / 30)
        for side in (-1, 1):
            ang = -math.pi / 2 + side * 1.15
            _leaf(canvas, alpha, mx, y, ang, half * 0.49, half * 0.022, (0.24, 0.42, 0.12),
                  lambda t_: np.minimum(1.0, t_ * 6) * (1 - t_ ** 3), rng, vein=0.6)

    # --- lantana: dense small rough leaves and flower heads
    ox, oy = origin(4)
    for _ in range(70):
        cx, cy = ox + rng.uniform(0.06, 0.94) * half, oy + rng.uniform(0.06, 0.94) * half
        L = half * rng.uniform(0.09, 0.14)
        _leaf(canvas, alpha, cx, cy, rng.uniform(0, 2 * math.pi), L, L * 0.45, (0.18, 0.36, 0.11),
              lambda t_: np.sin(np.pi * t_) ** 0.8 * (1 - 0.08 * np.abs(np.sin(t_ * 40))), rng, vein=0.45)
    for _ in range(9):
        cx, cy = ox + rng.uniform(0.15, 0.85) * half, oy + rng.uniform(0.15, 0.85) * half
        _flower_cluster(canvas, alpha, cx, cy, half * 0.06,
                        [(0.98, 0.50, 0.10), (0.95, 0.30, 0.45), (1.0, 0.80, 0.20)], rng, petals=4, n=16)

    # --- bougainvillea: magenta bract clusters among leaves
    ox, oy = origin(5)
    for _ in range(40):
        cx, cy = ox + rng.uniform(0.08, 0.92) * half, oy + rng.uniform(0.08, 0.92) * half
        L = half * rng.uniform(0.10, 0.16)
        _leaf(canvas, alpha, cx, cy, rng.uniform(0, 2 * math.pi), L, L * 0.5, (0.17, 0.37, 0.10),
              lambda t_: np.sin(np.pi * t_) ** 0.7, rng, vein=0.35)
    for _ in range(22):
        cx, cy = ox + rng.uniform(0.1, 0.9) * half, oy + rng.uniform(0.1, 0.9) * half
        _flower_cluster(canvas, alpha, cx, cy, half * rng.uniform(0.06, 0.09),
                        [(0.85, 0.08, 0.50), (0.92, 0.18, 0.58), (0.75, 0.05, 0.42)], rng, petals=3, n=5)

    # Cluster tiles fade to a soft round edge so card borders never show as straight lines.
    yy, xx = np.mgrid[0:half, 0:half].astype(np.float32)
    rr = np.hypot(xx - half / 2, yy - half / 2) / (half / 2)
    vignette = np.clip((1.0 - rr) / 0.12, 0, 1)
    for q in (1, 2, 4, 5):
        ox, oy = origin(q)
        alpha[oy:oy + half, ox:ox + half] *= vignette
    rgba = np.dstack([canvas, alpha])
    # Bleed colour outward under transparent pixels so mip-mapping doesn't fringe dark.
    for _ in range(6):
        m = rgba[..., 3] > 0
        for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0)):
            shifted = np.roll(np.roll(rgba[..., :3], dy, 0), dx, 1)
            sm = np.roll(np.roll(m, dy, 0), dx, 1)
            fill = (~m) & sm
            rgba[..., :3][fill] = shifted[fill]
    save_rgba(path, rgba)
    return path


def tileable_bark(path, size=512, seed=5, tint=(0.36, 0.30, 0.25), fissure=1.0):
    """Vertical fissured bark, periodic in both directions (built from integer-frequency waves)."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    h = np.zeros((size, size), dtype=np.float32)
    for k in range(18):
        fx = rng.integers(4, 18)
        fy = rng.integers(1, 4)
        ph = rng.uniform(0, 2 * np.pi)
        warp = 0.15 * np.sin(2 * np.pi * (fy * 2 * y + rng.uniform()))
        h += np.abs(np.sin(2 * np.pi * (fx * x + warp) + ph * np.sin(2 * np.pi * fy * y))) / (1 + k * 0.15)
    h = (h - h.min()) / (h.max() - h.min())
    ridges = h ** (1.5 * fissure)
    col = np.dstack([ridges * 0.5 + 0.5] * 3) * np.array(tint, dtype=np.float32)
    col *= (0.85 + 0.3 * np.roll(ridges, 7, 1))[..., None]
    save_rgba(path, np.dstack([col, np.ones((size, size), dtype=np.float32)]))
    # Normal map from the height field (tangent space, +Y up).
    gy, gx = np.gradient(ridges * 4.0)
    n = np.dstack([-gx, gy, np.ones_like(gx)])
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    save_rgba(path.replace(".png", "_normal.png"), np.dstack([(n + 1) * 0.5, np.ones((size, size), dtype=np.float32)]), non_color=True)


def save_rgba(path, rgba, non_color=False):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    h, w = rgba.shape[:2]
    img = bpy.data.images.new(os.path.basename(path), w, h, alpha=True)
    if non_color:
        img.colorspace_settings.name = "Non-Color"
    # Blender images are bottom-up.
    img.pixels.foreach_set(np.ascontiguousarray(rgba[::-1]).ravel().astype(np.float32))
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()
    return img

"""Where each species sits in the leaf atlas, and drawn bark.

The atlas (RGBA, 3072x2048) is built by leaf_atlas.py from photographs, six 1024 tiles (3 columns x 2
rows):
  0 neem: pinnate sprays of narrow leaflets
  1 peepal: heart-shaped leaves with long drip tips
  2 banyan: broad oval leathery leaves
  3 palm: one frond's length (leaflets angled off a midrib)
  4 lantana: small rough leaves with heads of orange and pink florets
  5 bougainvillea: papery magenta bracts among leaves
"""

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

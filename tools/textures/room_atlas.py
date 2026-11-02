#!/usr/bin/env python3
"""The rooms' one texture (assets/textures/rooms/room_atlas.png): six tiling strips, 6.4 m long and
3.2 m tall, at 200 pixels a metre, from CC0 scans. crate::buildings::interior in the game maps its
rows, and the vertices tint them, so the two wall rows are near white.

    0  painted plaster with a room's life on it: oil-paint dado, skirting, conduit, a tube light,
       switchboards, the ghosts of calendars, damp at the foot, soot at the top
    1  old plaster coming off the brick: godowns and stair wells
    2  mosaic: terrazzo with brass strips, the floor of most Delhi flats
    3  ceramic tiles: shops and dhabas
    4  old marble squares: offices and lobbies
    5  worn concrete: godowns
    6  a whitewashed ceiling

Raw scans stay in build/rooms, out of git (fetched by the commands in docs/references.md's rows):
PaintedPlaster017 and Terrazzo013 (ambientCG), grey_plaster, damaged_plaster, interior_tiles,
floor_tiles_06 and concrete_floor_worn_001 (Poly Haven), all CC0.

    python3 tools/textures/room_atlas.py
"""
import os

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..", "..")
RAW = os.path.join(ROOT, "build", "rooms")
OUT = os.path.join(ROOT, "assets", "textures", "rooms", "room_atlas.png")

M = 200  # Pixels a metre.
W = 1280  # 6.4 m.
ROW = 640  # 3.2 m.


def scan(name, repeats):
    """A scan repeated `repeats` times across a row's 3.2 m, and so twice that along it: floats."""
    im = Image.open(os.path.join(RAW, name)).convert("RGB")
    a = np.asarray(im, dtype=np.uint8)
    tiled = Image.fromarray(np.tile(a, (repeats, repeats, 1)))
    small = np.asarray(tiled.resize((ROW, ROW), Image.LANCZOS), dtype=np.float32) / 255.0
    return np.concatenate([small, small], axis=1)


def luma(rgb):
    return rgb @ np.array([0.3, 0.55, 0.15], dtype=np.float32)


def blur(a, r):
    """A wrapped box blur, three passes: near enough to a gaussian."""
    for _ in range(3):
        for axis in (0, 1):
            a = sum(np.roll(a, k, axis=axis) for k in range(-r, r + 1)) / (2 * r + 1)
    return a


def noise(rng, cell):
    """Smooth, tiling noise, 0..1, features about `cell` pixels across."""
    n = blur(rng.random((ROW, W), dtype=np.float32), max(1, cell // 3))
    return (n - n.min()) / (n.max() - n.min())


def smooth(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def wall(rng):
    """Row 0. Heights in metres over the floor, along the row."""
    h = (ROW - 0.5 - np.arange(ROW, dtype=np.float32))[:, None] / M
    m = (np.arange(W, dtype=np.float32) + 0.5)[None, :] / M
    paint = luma(scan("PaintedPlaster017/PaintedPlaster017_1K-JPG_Color.jpg", 1))
    stain = luma(scan("grey_plaster.jpg", 1))
    # Near white, keeping the scan's cracks and its brush marks, with the old stains under them.
    k = 0.90 + (paint - paint.mean()) * 1.8
    k *= 1.0 + (blur(stain, 2) - stain.mean()) * 0.32
    # Oil paint to shoulder height, a line of darker paint along its top, a skirting.
    dado = h < 1.1
    k = np.where(dado, k * 0.86, k)
    k = np.where((h >= 1.1) & (h < 1.116), k * 0.70, k)
    k = np.where(h < 0.10, k * 0.70, k)
    # Damp rising from the floor, soft and uneven; soot under the ceiling; a touch of grime on the
    # paint where shoulders rub it.
    damp = noise(rng, 90)
    k *= 1.0 - 0.20 * (1.0 - smooth(0.1, 0.25 + 0.75 * damp, h))
    k *= 1.0 - 0.14 * smooth(2.5, 3.2, h)
    k *= 1.0 - 0.05 * np.exp(-((h - 1.35) / 0.25) ** 2) * noise(rng, 40)
    # Where a calendar hung for years, the paint is cleaner.
    for at, y, w, t in [(0.9, 1.75, 0.34, 0.26), (3.9, 1.8, 0.46, 0.34), (5.5, 1.62, 0.22, 0.3)]:
        k = np.where((np.abs(m - at) < w) & (np.abs(h - y) < t), k * 1.03, k)
    rgb = np.stack([k * 0.98, k * 0.965, k * 0.93], axis=2)

    def put(mask, colour):
        rgb[np.broadcast_to(mask, rgb.shape[:2])] = colour

    # A conduit along the wall, with its shadow, and a tube light hung on it.
    put((h >= 2.54) & (h < 2.565), (0.62, 0.60, 0.56))
    put((h >= 2.525) & (h < 2.54), (0.36, 0.35, 0.33))
    put((h >= 2.44) & (h < 2.50) & (m > 2.6) & (m < 3.82), (1.0, 0.99, 0.94))
    put((h >= 2.42) & (h < 2.44) & (m > 2.58) & (m < 3.84), (0.5, 0.49, 0.46))
    # Switchboards, a drop of conduit to each.
    for at, y in [(1.3, 1.28), (4.7, 1.24)]:
        put((np.abs(m - at) < 0.008) & (h > y) & (h < 2.54), (0.58, 0.56, 0.52))
        put((np.abs(m - at) < 0.13) & (np.abs(h - y) < 0.085), (0.94, 0.93, 0.89))
        for s in range(-2, 3):
            put((np.abs(m - at - s * 0.042) < 0.007) & (np.abs(h - y) < 0.03), (0.28, 0.28, 0.30))
    return rgb


def old_wall():
    """Row 1. Toned towards grey, so that a vertex's tint can warm or cool it."""
    a = scan("damaged_plaster.jpg", 1)
    g = luma(a)[..., None]
    return np.clip((g + (a - g) * 0.75) * 1.25, 0, 1)


def mosaic():
    """Row 2. The chips are finer and greyer than the scan's, set in cement, with brass strips."""
    a = scan("Terrazzo013/Terrazzo013_1K-JPG_Color.jpg", 3)
    g = luma(a)[..., None]
    a = (0.34 + 0.62 * (g + (a - g) * 0.7)) * np.array([0.98, 0.95, 0.89], dtype=np.float32)
    strip = ROW // 3
    x, y = np.meshgrid(np.arange(W), np.arange(ROW))
    a[((x % strip) < 2) | ((y % strip) < 2)] = (0.56, 0.44, 0.26)
    return a


def tiles():
    """Row 3."""
    return scan("interior_tiles.jpg", 2) * np.array([0.98, 0.96, 0.94], dtype=np.float32)


def marble():
    """Row 4."""
    return np.clip(scan("floor_tiles_06.jpg", 2) * 1.12, 0, 1)


def concrete():
    """Row 5. The scan is dark: lifted, so a vertex's tint sets how dark."""
    a = scan("concrete_floor_worn_001.jpg", 1)
    return np.clip(a * 1.9, 0, 1)


def ceiling(rng):
    """Row 6. Whitewash over the old plaster's patches."""
    a = luma(scan("grey_plaster.jpg", 1))
    k = 0.93 + (blur(a, 1) - a.mean()) * 0.28 - 0.05 * noise(rng, 120)
    return np.stack([k * 0.99, k * 0.98, k * 0.95], axis=2)


def main():
    rng = np.random.default_rng(7)
    rows = [wall(rng), old_wall(), mosaic(), tiles(), marble(), concrete(), ceiling(rng)]
    atlas = np.concatenate(rows, axis=0)
    # The first texel stands for a plain surface: boxes drawn without UVs land on it.
    atlas[0:8, 0:8] = (0.93, 0.92, 0.89)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    Image.fromarray((np.clip(atlas, 0, 1) * 255 + 0.5).astype(np.uint8)).save(OUT, optimize=True)
    print(f"{OUT}: {atlas.shape[1]} x {atlas.shape[0]}, {os.path.getsize(OUT) / 1e6:.1f} MB")


if __name__ == "__main__":
    main()

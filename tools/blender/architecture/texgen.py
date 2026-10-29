"""Tileable sandstone and a carved-ornament trim sheet, drawn with numpy as height fields, then
converted to basecolor / normal / ORM PNGs. Shared by every building in the India kit.

Trim sheet (1024 x 2048): eight horizontal strips, 256 px tall each, tiling along U:
  0 padma: lotus petals          4 lata: vine scroll
  1 bead and reel                5 dentils (block frieze)
  2 chevron / zigzag             6 rope twist
  3 gavaksha: horseshoe arches   7 plain dressed stone (chamfer)
"""

import math
import os

import numpy as np

STRIPS = ["padma", "bead", "chevron", "gavaksha", "lata", "dentil", "rope", "plain"]


def strip_v(name):
    """(v0, v1) of a strip in UV space (V up)."""
    k = STRIPS.index(name)
    return 1.0 - (k + 1) / len(STRIPS), 1.0 - k / len(STRIPS)


def periodic_noise(h, w, rng, octaves=5, base=4):
    """Tileable value-like noise from integer-frequency cosines."""
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    out = np.zeros((h, w), dtype=np.float32)
    amp = 1.0
    for o in range(octaves):
        f = base * 2 ** o
        for _ in range(4):
            fx, fy = rng.integers(1, f + 1), rng.integers(1, f + 1)
            ph = rng.uniform(0, 2 * np.pi, 2)
            out += amp * np.cos(2 * np.pi * fx * x / w + ph[0]) * np.cos(2 * np.pi * fy * y / h + ph[1])
        amp *= 0.5
    out -= out.min()
    return out / out.max()


def normal_from_height(hgt, strength):
    gy, gx = np.gradient(hgt * strength)
    n = np.dstack([-gx, gy, np.ones_like(gx)])
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    return (n + 1) * 0.5


def cavity(hgt, radius=6):
    """Crevice darkness: how far below its neighbourhood average a pixel sits."""
    k = np.ones(radius * 2 + 1, dtype=np.float32) / (radius * 2 + 1)
    blur = np.apply_along_axis(lambda r: np.convolve(np.pad(r, radius, mode="wrap"), k, "valid"), 1, hgt)
    blur = np.apply_along_axis(lambda c: np.convolve(np.pad(c, radius, mode="wrap"), k, "valid"), 0, blur)
    return np.clip((blur - hgt) * 6.0, 0, 1)


def sandstone(out_dir, size=1024, seed=2, tint=(0.80, 0.62, 0.43), name="sandstone"):
    """Ashlar masonry: 0.5 m courses of 1 m blocks in running bond (tile = 2 m), per-block tone,
    recessed mortar joints, fine grain and faint bedding inside each block."""
    rng = np.random.default_rng(seed)
    grain = periodic_noise(size, size, rng, 6, 6)
    fine = rng.random((size, size)).astype(np.float32)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    rows, cols = 4, 2
    row = np.floor(y * rows)
    xs = (x * cols + 0.5 * (row % 2)) % 1.0
    ys = (y * rows) % 1.0
    block_id = (row * cols + np.floor((x * cols + 0.5 * (row % 2)) % cols)).astype(int)
    tones = rng.uniform(0.86, 1.10, rows * cols + 1)
    tone = tones[block_id]
    # Distance to the nearest joint, in block-relative units.
    jx = np.minimum(xs, 1 - xs) * (2.0 / cols)
    jy = np.minimum(ys, 1 - ys) * (2.0 / rows)
    jd = np.minimum(jx, jy)
    joint = np.clip(1 - jd / 0.006, 0, 1)
    bevel = np.clip(jd / 0.018, 0, 1)
    bedding = 0.5 + 0.5 * np.sin(2 * np.pi * (ys * 3 + 0.3 * grain))
    hgt = 0.75 * bevel + 0.12 * grain + 0.05 * fine + 0.03 * bedding - 0.4 * joint
    col = np.dstack([0.80 + 0.2 * grain] * 3) * np.array(tint, dtype=np.float32) * tone[..., None]
    col *= (0.97 + 0.03 * bedding)[..., None]
    col = col * (1 - 0.45 * joint[..., None]) + np.array([0.55, 0.48, 0.40]) * 0.45 * joint[..., None] * 0.6
    blush = periodic_noise(size, size, rng, 3, 2)
    col = col * (1 - 0.12 * blush[..., None]) + np.array([0.70, 0.42, 0.28]) * 0.12 * blush[..., None]
    rough = 0.8 + 0.12 * fine + 0.08 * joint
    ao = 1 - 0.5 * joint
    write_set(out_dir, name, np.clip(col, 0, 1), normal_from_height(hgt, 4.0), ao, rough)


def _strip_heights(name, w, h, rng):
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    u, v = x / w, y / h
    if name == "padma":
        n = 16  # petals per tile
        cu = (u * n) % 1.0 - 0.5
        # Petal: pointed upward from a rounded base.
        shape = np.clip(1.0 - (np.abs(cu) / (0.48 * np.clip(1.0 - v, 0, 1) ** 0.6 + 1e-3)), 0, 1)
        petal = np.where(v < 0.92, shape, 0) * np.clip(v * 4, 0, 1)
        rib = np.exp(-(cu / 0.03) ** 2) * 0.25
        return 0.25 + 0.7 * petal ** 0.5 + rib * (petal > 0)
    if name == "bead":
        n = 24
        cu = (u * n) % 1.0
        bead = np.where(cu < 0.62, np.sqrt(np.clip(1 - ((cu - 0.31) / 0.31) ** 2 - ((v - 0.5) / 0.38) ** 2, 0, 1)), 0)
        reel = np.where(cu >= 0.62, np.sqrt(np.clip(1 - ((cu - 0.81) / 0.15) ** 2 - ((v - 0.5) / 0.22) ** 2, 0, 1)) * 0.7, 0)
        return 0.2 + 0.8 * (bead + reel)
    if name == "chevron":
        n = 12
        zz = np.abs(((u * n + v * 0.9) % 1.0) - 0.5) * 2
        return 0.3 + 0.6 * np.clip((0.5 - np.abs(zz - v)) * 4, 0, 1)
    if name == "gavaksha":
        # Interlocking horseshoe arches ("sun window" mesh of shikharas).
        n = 6
        cu = (u * n) % 1.0 - 0.5
        cv = (v * 2 + 0.5 * (np.floor(u * n) % 2)) % 1.0
        r = np.hypot(cu, cv - 0.55)
        ring = np.clip(1 - np.abs(r - 0.36) / 0.06, 0, 1)
        hole = r < 0.30
        return np.where(hole, 0.1 + 0.25 * (cv < 0.55), 0.5) + 0.5 * ring
    if name == "lata":
        n = 4
        ph = 2 * np.pi * u * n
        stem = np.clip(1 - np.abs(v - 0.5 - 0.28 * np.sin(ph)) / 0.05, 0, 1)
        leaf = np.zeros_like(u)
        for k in range(2):
            lc_u = ((u * n + 0.25 + 0.5 * k) % 1.0 - 0.5) / n
            lc_v = v - (0.5 + (0.12 if k == 0 else -0.12))
            leaf += np.clip(1 - np.hypot(lc_u * n * 3.0, lc_v * 4.5), 0, 1)
        return 0.3 + 0.55 * np.maximum(stem, leaf ** 0.6)
    if name == "dentil":
        n = 20
        cu = (u * n) % 1.0
        return 0.25 + 0.6 * ((cu > 0.15) & (cu < 0.85) & (v > 0.15) & (v < 0.85))
    if name == "rope":
        n = 14
        t = (u * n + v * 1.2) % 1.0
        return 0.25 + 0.7 * np.sin(np.pi * t) * np.clip(1 - np.abs(v - 0.5) * 2.2, 0, 1)
    # plain dressed stone with chamfered edges
    return 0.6 - 0.3 * (np.clip(0.08 - np.minimum(v, 1 - v), 0, 1) / 0.08)


def trim_sheet(out_dir, w=1024, strip_h=256, seed=4, tint=(0.80, 0.62, 0.43), name="trim", streaks=0.0):
    rng = np.random.default_rng(seed)
    hs = [_strip_heights(nm, w, strip_h, rng) for nm in STRIPS]
    hgt = np.vstack(hs).astype(np.float32)
    grain = periodic_noise(hgt.shape[0], w, rng, 5, 4)
    hgt_d = hgt + 0.05 * grain
    cav = cavity(hgt_d, 5)
    base = np.dstack([0.78 + 0.22 * grain] * 3) * np.array(tint, dtype=np.float32)
    col = base * (1 - 0.45 * cav[..., None])
    ao = 1 - 0.6 * cav
    rough = 0.85 - 0.1 * hgt
    if streaks > 0:
        for k in range(len(STRIPS)):
            band = slice(k * strip_h, (k + 1) * strip_h)
            col[band] = weather_streaks(col[band], rng, streaks)
    write_set(out_dir, name, col, normal_from_height(hgt_d, 9.0), ao, rough)


def jali(out_dir, size=1024, cells=6, frame=0.045, seed=21, tint=(0.80, 0.62, 0.43)):
    """A pierced sandstone screen for a square window, `cells` x `cells` panels inside a solid
    frame (`frame` as a fraction of the side). Each cell: an eight-pointed star of two squares
    traced as a rib, a rosette hub on four spokes inside it, and ribs from the star's tips out to
    the cell border. The holes go in the basecolor alpha (alpha-tested in game); ribs are rounded
    by blurring the solid mask into a height field."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    inner = (x > frame) & (x < 1 - frame) & (y > frame) & (y < 1 - frame)
    u = (x - frame) / (1 - 2 * frame) * cells
    v = (y - frame) / (1 - 2 * frame) * cells
    px, py = (u % 1.0) - 0.5, (v % 1.0) - 0.5
    qx, qy = (px + py) / math.sqrt(2), (px - py) / math.sqrt(2)
    star = np.minimum(np.maximum(abs(px), abs(py)), np.maximum(abs(qx), abs(qy)))
    w = 0.075
    r = 0.29
    ring = abs(star - r) < w / 2
    border = np.maximum(abs(px), abs(py)) > 0.5 - w / 2
    hub = np.hypot(px, py) < 0.085
    spokes = (star < r) & ((abs(px) < w * 0.35) | (abs(py) < w * 0.35))
    tips = (star > r) & ((abs(px) < w * 0.4) | (abs(py) < w * 0.4) | (abs(qx) < w * 0.4) | (abs(qy) < w * 0.4))
    solid = ~inner | ring | border | hub | spokes | tips
    # Rounded ribs: blur the mask twice; height falls toward each rib's edge.
    hgt = solid.astype(np.float32)
    k = 5
    ker = np.ones(2 * k + 1, dtype=np.float32) / (2 * k + 1)
    for _ in range(2):
        hgt = np.apply_along_axis(lambda row: np.convolve(np.pad(row, k, mode="edge"), ker, "valid"), 1, hgt)
        hgt = np.apply_along_axis(lambda c: np.convolve(np.pad(c, k, mode="edge"), ker, "valid"), 0, hgt)
    hgt = np.sqrt(np.clip(hgt, 0, 1)) * solid
    grain = periodic_noise(size, size, rng, 5, 6)
    col = np.dstack([0.82 + 0.18 * grain] * 3) * np.array(tint, dtype=np.float32)
    # Darker in the cut recesses, sun-bleached on the rib crowns.
    col *= (0.72 + 0.28 * hgt)[..., None]
    rough = 0.82 + 0.1 * grain
    ao = 0.55 + 0.45 * hgt
    write_set(out_dir, "jali", np.clip(col, 0, 1), normal_from_height(hgt + 0.08 * grain * solid, 6.0), ao, rough,
              alpha=solid.astype(np.float32))


def write_set(out_dir, name, col, nrm, ao, rough, alpha=None):
    import bpy
    os.makedirs(out_dir, exist_ok=True)
    h, w = col.shape[:2]

    def save(arr, fname, non_color, a=None):
        img = bpy.data.images.new(fname, w, h, alpha=a is not None)
        if non_color:
            img.colorspace_settings.name = "Non-Color"
        rgba = np.dstack([arr, a if a is not None else np.ones((h, w), dtype=np.float32)]).astype(np.float32)
        img.pixels.foreach_set(np.ascontiguousarray(rgba[::-1]).ravel())
        img.filepath_raw = os.path.join(out_dir, fname)
        img.file_format = "PNG"
        img.save()

    save(np.clip(col, 0, 1), f"{name}_basecolor.png", False, alpha)
    save(nrm, f"{name}_normal.png", True)
    orm = np.dstack([ao, rough, np.zeros_like(ao)])
    save(np.clip(orm, 0, 1), f"{name}_orm.png", True)


# ------------------------------------------------------------------ building materials (tiling, 2 m)

def weather_streaks(col, rng, strength):
    """Darken an (h, w, 3) tile with the black lichen and rain run-off of South Indian stucco:
    soft vertical streaks hanging from the tile top (a ledge), in patches, plus faint mottling."""
    h, w = col.shape[:2]
    y = np.mgrid[0:h, 0:w][0].astype(np.float32) / h
    row = periodic_noise(h, w, rng, 4, 20)[h // 3]
    streak = np.clip((row - 0.45) * 2.5, 0, 1)[None, :]
    patch = np.clip(periodic_noise(h, w, rng, 2, 2) * 1.6 - 0.3, 0, 1)
    run = np.clip(1 - y * 1.1, 0, 1) ** 0.7
    mottle = np.clip((periodic_noise(h, w, rng, 3, 3) - 0.5) * 1.5, 0, 1)
    dark = np.clip(strength * (0.55 * streak * patch * (0.3 + 0.7 * run) + 0.25 * mottle), 0, 0.8)
    return col * (1 - dark[..., None]) + dark[..., None] * np.array([0.08, 0.08, 0.07])


def plaster(out_dir, size=1024, seed=11, name="plaster", tint=(1.0, 0.985, 0.96), grime=0.08, streaks=0.0):
    """Lime plaster, near-white so it can be tinted (white / ochre / Jodhpur blue) by the material.
    Trowel waves, hairline cracks, damp staining toward the bottom of each tile."""
    rng = np.random.default_rng(seed)
    trowel = periodic_noise(size, size, rng, 4, 3)
    speck = rng.random((size, size)).astype(np.float32)
    cracks = periodic_noise(size, size, rng, 6, 5)
    crack = np.clip(1 - np.abs(cracks - 0.5) / 0.012, 0, 1) * (periodic_noise(size, size, rng, 2, 2) > 0.6)
    y = np.mgrid[0:size, 0:size][0].astype(np.float32) / size
    stain = periodic_noise(size, size, rng, 3, 2) * 0.5
    hgt = 0.6 * trowel + 0.08 * speck - 0.5 * crack
    col = np.dstack([0.93 - 0.06 * trowel - 0.25 * crack - grime * stain * 4] * 3)
    col *= np.array(tint, dtype=np.float32)
    if streaks > 0:
        col = weather_streaks(col, rng, streaks)
    write_set(out_dir, name, np.clip(col, 0, 1), normal_from_height(hgt, 2.0), 1 - 0.4 * crack, 0.9 - 0.05 * trowel)


def brick(out_dir, size=1024, seed=12):
    """Small Indian bricks (lakhori-style thin bricks) in lime mortar, running bond."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    rows, cols = 32, 8
    row = np.floor(y * rows)
    xs = (x * cols + 0.5 * (row % 2)) % 1.0
    ys = (y * rows) % 1.0
    bid = (row * cols + np.floor((x * cols + 0.5 * (row % 2)) % cols)).astype(int)
    tone = rng.uniform(0.8, 1.15, rows * cols + 1)[bid]
    jd = np.minimum(np.minimum(xs, 1 - xs) * (2.0 / cols) * 4, np.minimum(ys, 1 - ys) * (2.0 / rows))
    joint = np.clip(1 - jd / 0.0045, 0, 1)
    grain = periodic_noise(size, size, rng, 5, 8)
    hgt = np.clip(jd / 0.01, 0, 1) * 0.8 + 0.15 * grain - 0.3 * joint
    col = np.array([0.66, 0.34, 0.24], dtype=np.float32) * (tone * (0.85 + 0.2 * grain))[..., None]
    col = col * (1 - joint[..., None]) + np.array([0.75, 0.70, 0.62]) * joint[..., None]
    write_set(out_dir, "brick", np.clip(col, 0, 1), normal_from_height(hgt, 5.0), 1 - 0.5 * joint, 0.9 + 0 * hgt)


def granite(out_dir, size=1024, seed=13):
    """Hampi's pinkish-grey granite: speckled feldspar and biotite, broad exfoliation patches."""
    rng = np.random.default_rng(seed)
    big = periodic_noise(size, size, rng, 3, 2)
    mid = periodic_noise(size, size, rng, 5, 6)
    speck = rng.random((size, size)).astype(np.float32)
    dark = (speck > 0.93).astype(np.float32)
    pink = (speck < 0.10).astype(np.float32)
    hgt = 0.5 * mid + 0.3 * big + 0.05 * speck
    base = np.array([0.62, 0.57, 0.52], dtype=np.float32) * (0.85 + 0.25 * big)[..., None]
    col = base * (1 - 0.6 * dark[..., None]) + np.array([0.72, 0.55, 0.50]) * 0.25 * pink[..., None]
    col *= (0.9 + 0.15 * mid)[..., None]
    write_set(out_dir, "granite", np.clip(col, 0, 1), normal_from_height(hgt, 3.5), 0.85 + 0.15 * mid, 0.75 + 0.15 * speck)


def wood(out_dir, size=1024, seed=14):
    """Weathered teak planks running along U."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    planks = 8
    pid = np.floor(y * planks)
    ys = (y * planks) % 1.0
    tone = rng.uniform(0.8, 1.15, planks + 1)[pid.astype(int)]
    grain = periodic_noise(size, size, rng, 5, 4)
    streak = 0.5 + 0.5 * np.sin(2 * np.pi * (x * 3 + ys * 0.4 + grain * 1.5 + pid * 0.37) * 6)
    gap = np.clip(1 - np.minimum(ys, 1 - ys) / 0.03, 0, 1)
    hgt = 0.4 * streak + 0.2 * grain - 0.7 * gap
    col = np.array([0.42, 0.28, 0.16], dtype=np.float32) * (tone * (0.8 + 0.25 * streak))[..., None] * (1 - 0.6 * gap)[..., None]
    write_set(out_dir, "wood", np.clip(col, 0, 1), normal_from_height(hgt, 3.0), 1 - 0.5 * gap, 0.7 + 0.15 * grain)


def thatch(out_dir, size=1024, seed=15):
    """Old straw thatch, 1.5 m tile: overlapping courses of fine strands hanging along -V, sun-greyed
    on top, darker and damp toward each course's butt ends, with moss and rot patches."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    rows = 10
    warp = periodic_noise(size, size, rng, 3, 3)
    ys = (y * rows + 0.35 * warp) % 1.0
    fib = np.zeros((size, size), dtype=np.float32)
    for k in range(8):
        f = rng.integers(140, 320)
        fib += np.abs(np.sin(2 * np.pi * (x * f + ys * rng.uniform(0.2, 0.9) + warp * rng.uniform(1, 3) + rng.uniform())))
    fib /= 8
    clump = periodic_noise(size, size, rng, 5, 12)
    hgt = 0.55 * fib + 0.25 * clump + 0.25 * ys
    age = periodic_noise(size, size, rng, 3, 2)
    straw = np.array([0.62, 0.52, 0.36], dtype=np.float32)
    grey = np.array([0.48, 0.45, 0.40], dtype=np.float32)
    col = straw * (1 - 0.6 * age[..., None]) + grey * 0.6 * age[..., None]
    col = col * (0.6 + 0.45 * fib)[..., None] * (0.7 + 0.35 * ys)[..., None] * (0.85 + 0.2 * clump)[..., None]
    moss = np.clip((periodic_noise(size, size, rng, 4, 4) - 0.7) * 4, 0, 1)
    col = col * (1 - 0.45 * moss[..., None]) + np.array([0.26, 0.28, 0.16]) * 0.45 * moss[..., None]
    write_set(out_dir, "thatch", np.clip(col, 0, 1), normal_from_height(hgt, 3.0), 0.6 + 0.4 * ys, 0.95 + 0 * ys)


def mudwall(out_dir, size=1024, seed=16, name="mudwall", tint=(0.78, 0.60, 0.42)):
    """Village wall, 4 m tile: hand-smoothed mud-and-dung plaster above a red-ochre (geru) dado
    with a white mandana border of triangles and dots. The dado fills V 0..0.15 (0..0.6 m)."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    v = 1.0 - y  # image rows run top-down, V runs up
    swirl = periodic_noise(size, size, rng, 4, 6)
    hand = 0.5 + 0.5 * np.sin(2 * np.pi * (x * 40 + swirl * 3.0)) * np.sin(2 * np.pi * (v * 28 + swirl * 2.0))
    crack = np.clip(1 - np.abs(periodic_noise(size, size, rng, 6, 4) - 0.5) / 0.01, 0, 1) * (periodic_noise(size, size, rng, 2, 2) > 0.62)
    hgt = 0.35 * swirl + 0.12 * hand - 0.5 * crack
    col = np.dstack([0.88 + 0.12 * swirl - 0.04 * hand] * 3) * np.array(tint, dtype=np.float32)
    dado = v < 0.15
    geru = np.array([0.55, 0.20, 0.12], dtype=np.float32) * (0.85 + 0.2 * swirl)[..., None]
    col = np.where(dado[..., None], geru, col)
    # Mandana border along the dado top: a band of white triangles with dots above.
    band = (v > 0.118) & (v < 0.148)
    tri = (np.abs(((x * 64) % 1.0) - 0.5) * 2) < (v - 0.118) / 0.03
    dots = np.hypot(((x * 64) % 1.0 - 0.5) / 64, (v - 0.106)) < 0.0045
    white = (band & tri) | dots | ((v > 0.148) & (v < 0.152))
    col = np.where(white[..., None], np.array([0.92, 0.89, 0.82], dtype=np.float32) * (0.9 + 0.1 * swirl)[..., None], col)
    # Damp and splash darkening at the foot of the wall.
    col *= (1.0 - 0.25 * np.clip(1 - v / 0.05, 0, 1))[..., None]
    col *= (1 - 0.35 * crack)[..., None]
    write_set(out_dir, name, np.clip(col, 0, 1), normal_from_height(hgt, 2.5), 1 - 0.4 * crack, 0.92 + 0 * hgt)


def basalt(out_dir, size=1024, seed=17):
    """Deccan trap basalt (Ajanta): dark brown-grey rock in horizontal flow layers, with ochre
    seepage stains and pale lichen. 4 m tile; layers run along U."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    warp = periodic_noise(size, size, rng, 3, 2)
    layers = 0.5 + 0.5 * np.sin(2 * np.pi * (y * 2 + warp * 1.5))
    fine = periodic_noise(size, size, rng, 6, 8)
    joints = np.clip(1 - np.abs(periodic_noise(size, size, rng, 3, 2) - 0.5) / 0.008, 0, 1) * (periodic_noise(size, size, rng, 2, 2) > 0.5)
    pits = (rng.random((size, size)) > 0.985).astype(np.float32)
    hgt = 0.45 * layers + 0.35 * fine - 0.6 * joints - 0.2 * pits
    base = np.array([0.36, 0.31, 0.27], dtype=np.float32) * (0.8 + 0.3 * fine)[..., None] * (0.88 + 0.16 * layers)[..., None]
    stain = periodic_noise(size, size, rng, 3, 2)
    ochre = np.clip((stain - 0.55) * 3, 0, 1)
    col = base * (1 - 0.3 * ochre[..., None]) + np.array([0.55, 0.38, 0.22]) * 0.3 * ochre[..., None]
    lichen = np.clip((periodic_noise(size, size, rng, 5, 5) - 0.72) * 5, 0, 1)
    col = col * (1 - 0.35 * lichen[..., None]) + np.array([0.62, 0.62, 0.52]) * 0.35 * lichen[..., None]
    col *= (1 - 0.5 * joints)[..., None]
    write_set(out_dir, "basalt", np.clip(col, 0, 1), normal_from_height(hgt, 4.0), 1 - 0.5 * joints, 0.88 + 0.08 * fine)


def cloth(out_dir, name, colors, stripes=8, size=512, seed=18):
    """Cotton awning cloth: woven stripes of `colors` (repeating) along U, 1 m tile, with weave
    texture and sun fading."""
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) / size
    idx = np.floor(x * stripes).astype(int) % len(colors)
    pal = np.array(colors, dtype=np.float32)
    weave = 0.5 + 0.25 * np.sin(2 * np.pi * x * 180) + 0.25 * np.sin(2 * np.pi * y * 180)
    fade = periodic_noise(size, size, rng, 3, 2)
    col = pal[idx] * (0.85 + 0.15 * weave)[..., None]
    col = col * (1 - 0.2 * fade[..., None]) + 0.2 * fade[..., None] * np.array([0.9, 0.85, 0.75])
    write_set(out_dir, name, np.clip(col, 0, 1), normal_from_height(weave, 0.6), 0.95 + 0 * weave, 0.95 + 0 * weave)


def building_set(out_dir):
    sandstone(out_dir)
    trim_sheet(out_dir)
    plaster(out_dir)
    brick(out_dir)
    granite(out_dir)
    wood(out_dir)
    thatch(out_dir)


if __name__ == "__main__":
    import sys
    building_set(sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "assets/textures/india")

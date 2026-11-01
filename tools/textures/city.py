"""The city maps' facade textures, painted as height fields with numpy so each comes with a normal
map, and a roughness/metal map so glass and steel catch the sun while plaster stays matte. Detail
lives in the textures, not the geometry: frames, grills, louvres, AC grilles and shutter ribs are
flat walls with their relief in the normal map.

  facade (1024 x 1024): two bays by two storeys (crate::buildings::BAY x STOREY each), the windows
      where the sills and lintels the game builds expect them: [[plain, AC], [balcony, shuttered]],
      with rain stains, chipped plaster showing brick, a drain pipe and a cable run.
  shop (1024 x 512): two bays of ground storey: a rolling shutter, and an open shop of shelves.

Near-white where the wall is, so the game's vertex colours tint each building.

  roof (512): clay tiles in cement for the flat roofs.
  ground_detail (512, RGBA): asphalt, slab and earth detail for the ground shader.

python3 tools/textures/city.py   -> assets/textures/panga/{facade,shop,roof}_{basecolor,normal,mr}.png,
                                    assets/textures/panga/ground_detail.png
"""

import os
import sys

import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools", "blender", "architecture"))
from texgen import cavity, normal_from_height, periodic_noise  # noqa: E402

OUT = os.path.join(ROOT, "assets", "textures", "panga")
N = 512  # pixels per bay (3.2 m) and per storey (3.4 m)


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def box(u, v, u0, u1, v0, v1, soft=0.004):
    """1 inside the rectangle, 0 out, with a soft edge a couple of pixels wide."""
    return smooth(u0 - soft, u0 + soft, u) * smooth(-u1 - soft, -u1 + soft, -u) * smooth(v0 - soft, v0 + soft, v) * smooth(-v1 - soft, -v1 + soft, -v)


def bevelled(u, v, u0, u1, v0, v1, bev):
    """A raised rectangle with sloped sides `bev` wide: 0 outside, 1 on top."""
    d = np.minimum.reduce([u - u0, u1 - u, v - v0, v1 - v])
    return np.clip(d / bev, 0, 1)


class Layer:
    """Height, colour, roughness and metal of one texture, painted over in order."""

    def __init__(self, h, w, rng):
        self.h, self.w, self.rng = h, w, rng
        self.hgt = np.zeros((h, w), np.float32)
        self.col = np.ones((h, w, 3), np.float32)
        self.rough = np.full((h, w), 0.9, np.float32)
        self.metal = np.zeros((h, w), np.float32)

    def paint(self, mask, col=None, hgt=None, rough=None, metal=None, add=False):
        m = mask[..., None]
        if col is not None:
            c = np.asarray(col, np.float32)
            self.col = self.col * (1 - m) + m * (c if c.ndim == 3 else c[None, None, :])
        if hgt is not None:
            self.hgt = self.hgt + mask * hgt if add else self.hgt * (1 - mask) + mask * hgt
        if rough is not None:
            self.rough = self.rough * (1 - mask) + mask * rough
        if metal is not None:
            self.metal = self.metal * (1 - mask) + mask * metal

    def save(self, name, strength):
        os.makedirs(OUT, exist_ok=True)
        # Crevices and recesses darker: baked occlusion in the colour, for every quality level.
        occ = 1 - 0.55 * cavity(self.hgt, 5)
        col = np.clip(self.col * occ[..., None], 0, 1)
        Image.fromarray((col ** (1 / 2.2) * 255 + 0.5).astype(np.uint8)).save(os.path.join(OUT, f"{name}_basecolor.png"))
        nrm = normal_from_height(self.hgt, strength)
        Image.fromarray((nrm * 255 + 0.5).astype(np.uint8)).save(os.path.join(OUT, f"{name}_normal.png"))
        # glTF / Bevy metallic-roughness: G roughness, B metal; half size is plenty.
        mr = np.dstack([np.zeros_like(self.rough), self.rough, self.metal])
        img = Image.fromarray((np.clip(mr, 0, 1) * 255 + 0.5).astype(np.uint8))
        img.resize((self.w // 2, self.h // 2), Image.LANCZOS).save(os.path.join(OUT, f"{name}_mr.png"))
        print(f"[city] {name}: {self.w}x{self.h}")


def lin(c):
    """sRGB to linear, so the colours below read as picked."""
    return [x ** 2.2 for x in c]


def plaster(L, grime):
    """Lime plaster over the whole sheet: trowel waves, speckle, hairline cracks, patches of damp."""
    rng = L.rng
    trowel = periodic_noise(L.h, L.w, rng, 4, 4)
    speck = rng.random((L.h, L.w)).astype(np.float32)
    cracks = periodic_noise(L.h, L.w, rng, 6, 6)
    crack = np.clip(1 - np.abs(cracks - 0.5) / 0.01, 0, 1) * (periodic_noise(L.h, L.w, rng, 2, 3) > 0.62)
    damp = np.clip(periodic_noise(L.h, L.w, rng, 3, 3) * 1.8 - 0.8, 0, 1)
    L.hgt += 0.25 * trowel + 0.06 * speck - 0.6 * crack
    g = 0.95 - 0.05 * trowel - 0.03 * speck - 0.25 * crack - grime * damp
    L.col = np.dstack([g, g * 0.985, g * 0.965]).astype(np.float32)
    L.rough[:] = 0.92 - 0.05 * trowel


def bricks(L, x0, y0, w, h, ppm_x, ppm_y):
    """A field of exposed brick (height, colour) over a region, for chipped plaster."""
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    bh, bw = 0.075 * ppm_y, 0.23 * ppm_x
    row = np.floor((y + y0) / bh)
    xs = x + x0 + (row % 2) * bw * 0.5
    fu, fv = (xs % bw) / bw, ((y + y0) % bh) / bh
    mortar = 1 - box(fu, fv, 0.06, 0.94, 0.14, 0.86, 0.04)
    k = L.rng.random((int(row.max()) + 2, int(xs.max() / bw) + 2)).astype(np.float32)
    shade = k[row.astype(int), (xs / bw).astype(int)]
    col = np.dstack([0.58 + 0.12 * shade, 0.30 + 0.06 * shade, 0.22 + 0.04 * shade]) * (1 - 0.45 * mortar[..., None])
    col = col + mortar[..., None] * np.array([0.30, 0.28, 0.25]) * 0.6
    return col.astype(np.float32), (1 - mortar).astype(np.float32)


def facade():
    rng = np.random.default_rng(41)
    L = Layer(2 * N, 2 * N, rng)
    plaster(L, 0.10)
    y, x = np.mgrid[0:2 * N, 0:2 * N].astype(np.float32)
    streak_cols = periodic_noise(2 * N, 2 * N, rng, 4, 24)[N // 3]
    for cy in range(2):
        for cx in range(2):
            kind = [[0, 2], [3, 1]][cy][cx]
            sl = (slice(cy * N, (cy + 1) * N), slice(cx * N, (cx + 1) * N))
            u, v = (x[sl] - cx * N) / N, (y[sl] - cy * N) / N
            C = Layer(N, N, rng)
            C.hgt, C.col, C.rough, C.metal = L.hgt[sl].copy(), L.col[sl].copy(), L.rough[sl].copy(), L.metal[sl].copy()

            # Chipped plaster: brick showing through, in a patch or two, never round the window.
            chip = np.clip(periodic_noise(N, N, rng, 4, 3) * 2.6 - 2.05, 0, 1)
            chip *= 1 - box(u, v, 0.18, 0.82, 0.08, 0.98, 0.04)
            chip = smooth(0.05, 0.12, chip)
            bc, bm = bricks(C, cx * N, cy * N, N, N, N / 3.2, N / 3.4)
            C.paint(chip, col=bc, rough=0.95)
            C.hgt = C.hgt - chip * (1.2 - 0.5 * bm)

            # Rain run-off from the sill down to the floor band, in streaks.
            run = box(u, v, 0.24, 0.76, 0.78, 0.97, 0.03) * np.clip(1 - (v - 0.78) / 0.2, 0, 1) ** 0.6
            streak = np.clip((streak_cols[None, cx * N:(cx + 1) * N] - 0.4) * 2.5, 0, 1)
            C.col *= (1 - 0.35 * run * (0.35 + 0.65 * streak))[..., None]

            # A string course at each floor: a raised band.
            band = bevelled(u, v, -1, 2, 0.955, 1.02, 0.008)
            C.paint(np.clip(band * 4, 0, 1), col=lin([0.86, 0.85, 0.82]), rough=0.85)
            C.hgt += 1.2 * band

            # The window: a raised frame, the reveal stepping in, the glass at the back.
            frame = bevelled(u, v, 0.25, 0.75, 0.13, 0.73, 0.01)
            reveal = box(u, v, 0.28, 0.72, 0.16, 0.70, 0.003)
            C.paint(np.clip(frame * 3, 0, 1), col=lin([0.88, 0.87, 0.84]), rough=0.8)
            C.hgt += 1.5 * frame - 3.5 * reveal
            opening = reveal
            if kind == 3:
                # The balcony's door-height opening runs down to the slab.
                door = box(u, v, 0.28, 0.72, 0.16, 0.80, 0.003)
                C.hgt -= 3.5 * np.clip(door - reveal, 0, 1)
                opening = door

            # Glass: dark, bluer up top where it catches the sky, a pale band of reflection across.
            k = np.clip(1 - (v - 0.16) / 0.6, 0, 1)
            refl = np.clip(1 - np.abs((u - 0.5) * 0.8 + (v - 0.4) - 0.0) / 0.06, 0, 1) * 0.5
            glass = np.dstack([0.06 + 0.07 * k + refl * 0.25, 0.08 + 0.09 * k + refl * 0.27, 0.11 + 0.14 * k + refl * 0.3])
            C.paint(opening, col=glass, rough=0.08)
            # Mullions: a cross of white frame dividing the glass.
            mull = (box(u, v, 0.49, 0.51, 0.16, 0.70, 0.002) + box(u, v, 0.28, 0.72, 0.36, 0.38, 0.002)) * reveal
            C.paint(np.clip(mull, 0, 1), col=lin([0.85, 0.84, 0.80]), rough=0.6)
            C.hgt += 1.5 * np.clip(mull, 0, 1)

            if kind == 0:
                # A curtain behind the right pane, and the iron grill every Delhi window wears.
                cur = box(u, v, 0.51, 0.72, 0.16, 0.70, 0.002)
                folds = (0.75 + 0.25 * np.sin(u * 180))[..., None]
                C.paint(cur, col=np.array(lin([0.62, 0.30, 0.22]), np.float32) * folds, rough=0.9)
                C.hgt += cur * 0.6 * np.sin(u * 180)
                bars = (np.abs(((u - 0.28) / 0.44 * 7) % 1 - 0.5) < 0.06) | (np.abs(v - 0.43) < 0.004) | (np.abs(v - 0.22) < 0.004)
                arc = np.abs(np.hypot((u - 0.5) / 0.22, (v - 0.43) / 0.2) - 0.8) < 0.025
                grill = (bars | (arc & (v < 0.43))) * box(u, v, 0.27, 0.73, 0.15, 0.71, 0.002)
                g = smooth(0, 1, grill.astype(np.float32))
                C.paint(g, col=lin([0.12, 0.12, 0.13]), rough=0.5, metal=0.6)
                C.hgt += 3.0 * g
            if kind == 1:
                # Louvred wooden shutters folded back each side, painted green.
                for (a, b) in [(0.10, 0.25), (0.75, 0.90)]:
                    sh = bevelled(u, v, a, b, 0.13, 0.73, 0.008)
                    slat = ((v - 0.13) * 90) % 1
                    C.paint(np.clip(sh * 4, 0, 1), col=np.dstack([0.12 + 0.05 * slat, 0.30 + 0.08 * slat, 0.20 + 0.05 * slat]), rough=0.7)
                    C.hgt += sh * (1.5 + 0.8 * slat) * box(u, v, a + 0.012, b - 0.012, 0.15, 0.71, 0.002) + sh * 1.0
            if kind == 2:
                # An AC unit hung under the window: a box with a slatted grille and a fan guard,
                # and its drip stain down the wall.
                ac = bevelled(u, v, 0.34, 0.66, 0.79, 0.94, 0.012)
                C.paint(np.clip(ac * 5, 0, 1), col=lin([0.80, 0.80, 0.78]), rough=0.45, metal=0.1)
                slats = box(u, v, 0.37, 0.48, 0.81, 0.92, 0.003) * (((v - 0.81) * 160) % 1 > 0.5)
                fan = np.hypot((u - 0.57) / 0.064, (v - 0.865) / 0.06)
                guard = (fan < 1) * ((np.abs((fan * 6) % 1 - 0.5) < 0.18) | (np.abs(u - 0.57) < 0.002))
                C.paint(slats + guard * 1.0, col=lin([0.30, 0.31, 0.32]), rough=0.5)
                C.hgt += 3.5 * ac - 0.8 * (slats + (fan < 1))
                drip = box(u, v, 0.585, 0.60, 0.94, 1.0, 0.004) * (1 - (v - 0.94) / 0.06)
                C.col *= (1 - 0.4 * drip)[..., None]
            if kind == 3:
                # Behind the opening, a wooden door; across its foot, the balcony rail.
                leaf = box(u, v, 0.30, 0.70, 0.18, 0.80, 0.003)
                panel = bevelled(u, v, 0.32, 0.48, 0.21, 0.76, 0.01) + bevelled(u, v, 0.52, 0.68, 0.21, 0.76, 0.01)
                grain = periodic_noise(N, N, rng, 3, 8)[:, :1] * 0.15
                C.paint(leaf, col=np.dstack([0.16 + grain, 0.07 + grain * 0.4, 0.03 + 0.0 * grain]), rough=0.7)
                C.hgt += leaf * (0.6 * np.clip(panel, 0, 1))
                gap = box(u, v, 0.495, 0.505, 0.18, 0.80, 0.002)
                C.paint(gap, col=lin([0.05, 0.04, 0.03]))
                rail = ((np.abs(((u - 0.2) / 0.6 * 14) % 1 - 0.5) < 0.1) | (np.abs(v - 0.535) < 0.01) | (np.abs(v - 0.78) < 0.006)) * box(u, v, 0.20, 0.80, 0.52, 0.80, 0.002)
                r = smooth(0, 1, rail.astype(np.float32))
                C.paint(r, col=lin([0.15, 0.16, 0.16]), rough=0.45, metal=0.7)
                C.hgt += 4.0 * r

            # Grime gathering under the frame's sill (the game puts a sill ledge just here).
            C.col *= (1 - 0.18 * box(u, v, 0.24, 0.76, 0.73, 0.79, 0.01))[..., None]
            L.hgt[sl], L.col[sl], L.rough[sl], L.metal[sl] = C.hgt, C.col, C.rough, C.metal

    # A drain pipe down the right edge of the right bay, both storeys, clamped at each floor.
    u = x / N
    pu = (u - 1.94) / 0.025
    pipe = np.clip(1 - pu ** 2, 0, 1)
    pm = (np.abs(pu) < 1).astype(np.float32)
    L.paint(pm, col=np.dstack([0.55 + 0.15 * np.sqrt(pipe)] * 3) * np.array([1.0, 0.98, 0.95]), rough=0.5)
    L.hgt = L.hgt * (1 - pm) + pm * (2.5 + 3.0 * np.sqrt(pipe))
    clamp = (np.abs(pu) < 1.4) & (np.abs((y / N) % 1 - 0.5) < 0.008)
    L.paint(clamp.astype(np.float32), col=[0.25, 0.25, 0.25], rough=0.5, metal=0.5)
    # A cable run along the top of each storey, sagging a little between hooks.
    v = (y / N) % 1
    sag = 0.035 + 0.012 * np.sin(np.pi * ((x / N * 2) % 1))
    cable = (np.abs(v - sag) < 0.0045).astype(np.float32)
    L.paint(cable, col=[0.03, 0.03, 0.03], rough=0.5)
    L.hgt += cable * 2.5
    L.save("facade", 2.2)


def shop():
    rng = np.random.default_rng(43)
    L = Layer(N, 2 * N, rng)
    plaster(L, 0.18)
    y, x = np.mgrid[0:N, 0:2 * N].astype(np.float32)
    u, v = (x % N) / N, y / N
    bay = (x // N).astype(int)
    inside = box(u, v, 0.06, 0.94, 0.20, 1.01, 0.004)
    # Piers and beam: plaster, grimier toward the street.
    dirt = np.clip((v - 0.80) / 0.2, 0, 1)
    L.col *= (1 - 0.3 * dirt * (1 - inside))[..., None]
    L.hgt += 1.5 * bevelled(u, v, -0.06, 0.06, -1, 2, 0.01) + 1.5 * bevelled(u, v, 0.94, 1.06, -1, 2, 0.01)
    # Posters pasted on the piers, peeling.
    for k in range(6):
        px, py = rng.uniform(0.005, 0.03) + (0.94 if k % 2 else 0.0), rng.uniform(0.3, 0.8)
        b = (k // 2)
        m = box(u, v, px, px + 0.03, py, py + 0.12, 0.002) * (bay == b % 2)
        c = [[0.85, 0.75, 0.30], [0.80, 0.25, 0.25], [0.30, 0.55, 0.80], [0.90, 0.88, 0.80]][k % 4]
        L.paint(m, col=lin(c), rough=0.8)

    shutter = inside * (bay == 0)
    rib = ((y % 9) / 9.0)
    prof = np.sin(rib * np.pi * 2) * 0.5 + 0.5
    rust = np.clip((v - 0.4) / 0.6, 0, 1) * np.clip(periodic_noise(N, 2 * N, rng, 3, 6) * 1.6 - 0.3, 0, 1)
    sc = np.dstack([0.55 + 0.06 * prof + 0.15 * rust, 0.56 + 0.06 * prof - 0.05 * rust, 0.57 + 0.06 * prof - 0.14 * rust])
    L.paint(shutter, col=sc, rough=0.45 + 0.4 * rust, metal=0.8 * (1 - rust))
    L.hgt = L.hgt * (1 - shutter) + shutter * (-1.0 + 1.0 * prof)
    # The bottom bar with its lock, and the handle in the middle.
    bar = box(u, v, 0.06, 0.94, 0.93, 0.97, 0.003) * (bay == 0)
    L.paint(bar, col=[0.20, 0.20, 0.21], rough=0.4, metal=0.8)
    L.hgt += 1.5 * bar
    lock = box(u, v, 0.48, 0.52, 0.90, 0.96, 0.003) * (bay == 0)
    L.paint(lock, col=lin([0.70, 0.55, 0.20]), rough=0.3, metal=1.0)
    L.hgt += 2.5 * lock

    # The open shop: dark inside, shelves stacked with goods, a counter across the front.
    shop = inside * (bay == 1)
    depth = np.dstack([0.05 + 0.04 * (1 - v)] * 3) * np.array([1.0, 0.92, 0.82])
    L.paint(shop, col=depth, rough=0.9, metal=0.0)
    L.hgt = L.hgt * (1 - shop) + shop * -4.0
    s = ((v - 0.22) / 0.52 * 4)
    shelf = (s % 1 < 0.08) & (s >= 0) & (s < 4)
    goods = (s % 1 > 0.25) & (s >= 0) & (s < 4)
    item_w = 0.035
    col_id = np.floor((u - 0.08) / item_w).astype(int)
    hsh = (np.sin(col_id * 12.9898 + np.floor(s) * 78.233) * 43758.5453) % 1
    palette = np.array([lin(c) for c in [[0.80, 0.22, 0.18], [0.95, 0.72, 0.18], [0.20, 0.45, 0.75], [0.90, 0.88, 0.82], [0.25, 0.60, 0.30], [0.85, 0.40, 0.60]]], np.float32)
    gcol = palette[(hsh * 5.99).astype(int)]
    top = (s % 1) > (0.25 + 0.35 * ((hsh * 7) % 1))
    gm = (goods & top & (((u - 0.08) / item_w) % 1 > 0.1)).astype(np.float32) * shop * (v < 0.74)
    lit = (0.55 + 0.45 * (1 - (s % 1))) [..., None]
    L.paint(gm, col=gcol * lit * 0.8, rough=0.6)
    L.hgt += gm * 1.5
    sm = shelf.astype(np.float32) * shop * (v < 0.76)
    L.paint(sm, col=lin([0.42, 0.32, 0.22]), rough=0.7)
    L.hgt += sm * 2.0
    counter = bevelled(u, v, 0.06, 0.94, 0.76, 1.1, 0.01) * (bay == 1)
    L.paint(np.clip(counter * 4, 0, 1), col=lin([0.62, 0.62, 0.60]), rough=0.35, metal=0.6)
    L.hgt += counter * 5.0
    # Street grime at the foot of it all.
    L.col *= (1 - 0.35 * np.clip((v - 0.92) / 0.08, 0, 1))[..., None]
    L.save("shop", 2.2)


def roof():
    """Flat roofs, 2 m a repeat (crate::buildings::TILE): square clay tiles laid in cement, some
    cracked, black water stains and dust in patches. Light grey-white so each roof's vertex colour
    makes it terracotta, cement or lime-washed."""
    rng = np.random.default_rng(45)
    n = 512
    L = Layer(n, n, rng)
    y, x = np.mgrid[0:n, 0:n].astype(np.float32)
    t = n / 8  # eight tiles a repeat: 25 cm tiles
    fu, fv = (x % t) / t, (y % t) / t
    ti, tj = (x // t).astype(int), (y // t).astype(int)
    k = rng.random((8, 8)).astype(np.float32)[tj, ti]
    tile = bevelled(fu, fv, 0.03, 0.97, 0.03, 0.97, 0.05)
    crack_line = ((np.abs(fu - 0.2 - 0.6 * fv) < 0.012) & (k > 0.8)).astype(np.float32)
    dust = periodic_noise(n, n, rng, 4, 3)
    stain = np.clip(periodic_noise(n, n, rng, 3, 2) * 2.2 - 1.3, 0, 1)
    g = (0.80 + 0.12 * k - 0.10 * dust) * (0.55 + 0.45 * tile) * (1 - 0.5 * crack_line)
    L.col = np.dstack([g, g * 0.98, g * 0.95]).astype(np.float32) * (1 - 0.55 * stain[..., None])
    L.hgt = 1.2 * tile + 0.3 * dust - 0.8 * crack_line + 0.15 * rng.random((n, n)).astype(np.float32)
    L.rough[:] = 0.85 - 0.25 * stain
    L.save("roof", 1.6)


def ground():
    """Detail for the city ground, 4 m a repeat, one kind per channel (linear, 0.5 = no change), mixed
    per vertex by what covers the ground there (crate::ground): R asphalt with grit, cracks and
    patched repairs; G pavement slabs with their joints; B trodden earth with pebbles. The shader
    shades the ground by it and bends the light off it, so joints and cracks catch the sun."""
    rng = np.random.default_rng(47)
    n = 512
    y, x = np.mgrid[0:n, 0:n].astype(np.float32)
    grit = rng.random((n, n)).astype(np.float32)
    # Asphalt: fine grit, worn patches, a few long cracks (no one-off marks: they'd show the repeat).
    cr = periodic_noise(n, n, rng, 5, 3)
    crack = np.clip(1 - np.abs(cr - 0.5) / 0.008, 0, 1) * (periodic_noise(n, n, rng, 2, 2) > 0.55)
    worn = periodic_noise(n, n, rng, 3, 4)
    asphalt = 0.55 + 0.12 * (grit - 0.5) + 0.12 * (worn - 0.5) - 0.35 * crack
    # Pavement: 60 x 40 cm slabs in stretcher bond, each its own shade, chipped corners.
    sw, sh = n / 4 / 1.5, n / 10.0
    row = np.floor(y / sh)
    xs = x + (row % 2) * sw * 0.5
    fu, fv = (xs % sw) / sw, (y % sh) / sh
    slab_k = rng.random((11, 8)).astype(np.float32)[row.astype(int) % 11, (xs // sw).astype(int) % 8]
    joint = 1 - bevelled(fu, fv, 0.0, 1.0, 0.0, 1.0, 0.06)
    slabs = 0.58 + 0.12 * (slab_k - 0.5) + 0.06 * (grit - 0.5) + 0.06 * (worn - 0.5) - 0.38 * joint
    # Earth: soft mounds, pebbles, dry cracks.
    mounds = periodic_noise(n, n, rng, 4, 5)
    peb = periodic_noise(n, n, rng, 2, 40)
    pebbles = np.clip((peb - 0.78) * 6, 0, 1)
    dry = np.clip(1 - np.abs(periodic_noise(n, n, rng, 4, 6) - 0.5) / 0.01, 0, 1) * 0.6
    earth = 0.52 + 0.18 * (mounds - 0.5) + 0.25 * pebbles - 0.25 * dry + 0.05 * (grit - 0.5)
    img = np.dstack([asphalt, slabs, earth, np.ones_like(earth)])
    os.makedirs(OUT, exist_ok=True)
    Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8), "RGBA").save(os.path.join(OUT, "ground_detail.png"))
    print("[city] ground_detail: 512x512")


facade()
shop()
roof()
ground()

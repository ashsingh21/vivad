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
  wear (512): grime and scratches for the street kit.
  stucco, stone (512): Connaught Place's clean white walls and its stone-tiled verandahs.
  limewash (512): mottled near-white under the shops' paint colours (Mat::Paint).
  goods (2048 x 1024): stock on shop shelves, a trade a column (kirana, bartan, kapda, dabba), two
      stockings each.

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

    def save(self, name, strength, paint=0.45):
        """Write the set. Painted, as a stylised game's textures are: the relief smoothed of pixel
        noise, crevices darkened, and light painted in from the top left (edges facing up and
        left lit, edges facing down shadowed) so every frame, sill and band reads in any light."""
        os.makedirs(OUT, exist_ok=True)
        hgt = soften(self.hgt, 1)
        occ = 1 - 0.55 * cavity(hgt, 5)
        n = normal_from_height(hgt, strength) * 2 - 1
        lit = np.clip((0.85 * n[..., 1] - 0.35 * n[..., 0]) * 4.0, -1, 1)
        col = np.clip(self.col * (occ * (1 + paint * lit))[..., None], 0, 1)
        Image.fromarray((col ** (1 / 2.2) * 255 + 0.5).astype(np.uint8)).save(os.path.join(OUT, f"{name}_basecolor.png"))
        nrm = (n + 1) * 0.5
        Image.fromarray((nrm * 255 + 0.5).astype(np.uint8)).save(os.path.join(OUT, f"{name}_normal.png"))
        # glTF / Bevy metallic-roughness: G roughness, B metal; half size is plenty.
        mr = np.dstack([np.zeros_like(self.rough), self.rough, self.metal])
        img = Image.fromarray((np.clip(mr, 0, 1) * 255 + 0.5).astype(np.uint8))
        img.resize((self.w // 2, self.h // 2), Image.LANCZOS).save(os.path.join(OUT, f"{name}_mr.png"))
        print(f"[city] {name}: {self.w}x{self.h}")


def soften(a, r):
    """A wrapping box blur `r` pixels each way: takes the pixel noise out of a height field."""
    out = a.copy()
    for axis in (0, 1):
        acc = np.zeros_like(out)
        for d in range(-r, r + 1):
            acc += np.roll(out, d, axis=axis)
        out = acc / (2 * r + 1)
    return out


def lin(c):
    """sRGB to linear, so the colours below read as picked."""
    return [x ** 2.2 for x in c]


def plaster(L, grime):
    """Lime plaster over the whole sheet, painted: broad soft trowel waves and patches of damp,
    no speckle or hairline cracks (pixel detail reads as noise, not as wall)."""
    rng = L.rng
    trowel = periodic_noise(L.h, L.w, rng, 3, 2)
    damp = np.clip(periodic_noise(L.h, L.w, rng, 2, 2) * 1.8 - 0.8, 0, 1)
    L.hgt += 0.2 * trowel
    g = 0.95 - 0.04 * trowel - grime * damp
    L.col = np.dstack([g, g * 0.985, g * 0.965]).astype(np.float32)
    L.rough[:] = 0.92 - 0.04 * trowel


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

            # Painted light: each storey a touch lighter at the top, darker at its foot.
            C.col *= (1.04 - 0.1 * v)[..., None]
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
    # Rust: patches eating through the galvanising, worst toward the bottom where the rain splashes,
    # streaks running down from the ribs, and the steel still grey between.
    patches = np.clip(periodic_noise(N, 2 * N, rng, 4, 7) * 2.2 - 0.75, 0, 1)
    low = np.clip(0.3 + 0.9 * (v - 0.2) / 0.8, 0, 1)
    streak = np.clip((periodic_noise(N, 2 * N, rng, 4, 40)[N // 2][None, :] - 0.45) * 3.0, 0, 1) * np.clip((v - 0.25) * 1.5, 0, 1)
    rib_rust = (rib < 0.2) * 0.5 * patches
    rust = np.clip(patches * low + 0.6 * streak * low + rib_rust, 0, 1)
    steel = np.dstack([0.50 + 0.07 * prof] * 3) * np.array([1.0, 1.0, 1.02])
    rust_col = np.dstack([0.32 + 0.10 * patches, 0.14 + 0.04 * patches, 0.06 + 0.02 * patches])
    sc = steel * (1 - rust[..., None] * 0.9) + rust_col * rust[..., None] * 0.9
    L.paint(shutter, col=sc, rough=0.45 + 0.45 * rust, metal=0.8 * (1 - rust))
    L.hgt += shutter * 0.6 * rust * (rng.random((N, 2 * N)).astype(np.float32) - 0.3)
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
    # Stocked as the shelves in the hand-built shops are (goods()): a grocer's, in the dim.
    for k in range(4):
        top, floor = int((0.22 + 0.13 * k) * N) + 6, int((0.22 + 0.13 * (k + 1)) * N)
        stock(L, rng, "kirana" if k % 2 == 0 else "dabba", N + int(0.08 * N), N + int(0.92 * N), top, floor)
    L.col[:, N:] = L.col[:, N:] * (1 - 0.35 * shop[:, N:, None])
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
    L.hgt = 1.2 * tile + 0.3 * dust - 0.8 * crack_line
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



def wear():
    """Wear for the street kit (crate::street), 1.5 m a repeat on UVs the game projects in the world:
    grime in blotches and finer dirt, scratches and chips through the paint. A grey multiplier
    (0.8 clean, which the material lifts back to 1) and a normal map."""
    rng = np.random.default_rng(49)
    n = 512
    blot = periodic_noise(n, n, rng, 4, 3)
    fine = periodic_noise(n, n, rng, 3, 24)
    grime = np.clip(np.clip((blot - 0.45) * 2.2, 0, 1) * 0.7 + 0.24 * (fine - 0.5), 0, 1)
    y, x = np.mgrid[0:n, 0:n].astype(np.float32)
    scratch = np.zeros((n, n), np.float32)
    for _ in range(90):
        x0, y0 = rng.uniform(0, n, 2)
        a = rng.uniform(0, np.pi)
        ln = rng.uniform(8, 50)
        dx, dy = np.cos(a), np.sin(a)
        # Distance to the segment, wrapped so it tiles.
        px, py = (x - x0 + n / 2) % n - n / 2, (y - y0 + n / 2) % n - n / 2
        t = np.clip(px * dx + py * dy, 0, ln)
        d = np.hypot(px - t * dx, py - t * dy)
        scratch = np.maximum(scratch, np.clip(1 - d / 0.9, 0, 1))
    chips = np.clip((periodic_noise(n, n, rng, 2, 30) - 0.82) * 8, 0, 1)
    marks = np.maximum(scratch * 0.8, chips)
    L = Layer(n, n, rng)
    g = 0.8 * (1 - 0.3 * grime) + 0.12 * marks
    L.col = np.dstack([g, g, g]).astype(np.float32)
    L.hgt = 0.6 * grime - 1.5 * marks
    os.makedirs(OUT, exist_ok=True)
    Image.fromarray((np.clip(L.col, 0, 1) ** (1 / 2.2) * 255 + 0.5).astype(np.uint8)).save(os.path.join(OUT, "wear_basecolor.png"))
    Image.fromarray((normal_from_height(L.hgt, 1.2) * 255 + 0.5).astype(np.uint8)).save(os.path.join(OUT, "wear_normal.png"))
    print("[city] wear: 512x512")


def stucco():
    """Clean lime-washed stucco, 2 m a repeat (crate::buildings::TILE): Connaught Place's white.
    Soft trowel marks and a fine sand grain; no cracks or grime, just a whisper of warmth."""
    rng = np.random.default_rng(51)
    n = 512
    L = Layer(n, n, rng)
    trowel = periodic_noise(n, n, rng, 4, 3)
    grain = rng.random((n, n)).astype(np.float32)
    soft = periodic_noise(n, n, rng, 2, 2)
    g = 0.86 - 0.03 * trowel - 0.015 * grain - 0.025 * soft
    L.col = np.dstack([g, g * 0.975, g * 0.93]).astype(np.float32)
    # Relief from the trowel only: per-pixel grain in the normals speckles the sheen on paint.
    L.hgt = 0.35 * trowel
    L.rough[:] = 0.92 - 0.03 * trowel
    L.save("stucco", 1.2)


def stone():
    """Square stone floor tiles, 50 cm, 2 m a repeat: buff sandstone, each a shade of its own,
    thin grout, worn smooth and a little glossy where walked."""
    rng = np.random.default_rng(53)
    n = 512
    L = Layer(n, n, rng)
    y, x = np.mgrid[0:n, 0:n].astype(np.float32)
    t = n / 4
    fu, fv = (x % t) / t, (y % t) / t
    k = rng.random((4, 4)).astype(np.float32)[(y // t).astype(int), (x // t).astype(int)]
    tile = bevelled(fu, fv, 0.01, 0.99, 0.01, 0.99, 0.02)
    mottle = periodic_noise(n, n, rng, 4, 6)
    g = (0.78 + 0.08 * k + 0.05 * (mottle - 0.5)) * (0.6 + 0.4 * tile)
    L.col = np.dstack([g, g * 0.95, g * 0.86]).astype(np.float32)
    L.hgt = 1.0 * tile + 0.15 * mottle
    L.rough[:] = 0.55 + 0.25 * (1 - tile) + 0.1 * mottle
    L.save("stone", 1.4)


def limewash():
    """Limewash, 2 m a repeat, near-white to take the paint's colour: brushed on in coats that
    fade unevenly in the sun, patches where it's been touched up, a faint damp bloom. The mottle
    is what makes a painted Delhi wall read as old paint, not plastic."""
    rng = np.random.default_rng(55)
    n = 512
    L = Layer(n, n, rng)
    coats = periodic_noise(n, n, rng, 5, 3)
    patches = np.clip((periodic_noise(n, n, rng, 3, 2) - 0.55) * 4, 0, 1)
    brush = periodic_noise(n, n, rng, 3, 24)
    damp = np.clip((periodic_noise(n, n, rng, 4, 4) - 0.6) * 3, 0, 1)
    g = 0.97 - 0.10 * coats + 0.06 * patches - 0.03 * brush - 0.10 * damp
    L.col = np.dstack([g, g * 0.985, g * 0.96]).astype(np.float32)
    L.hgt = 0.3 * coats + 0.12 * brush
    L.rough[:] = 0.95
    L.save("limewash", 1.0)


def rustic():
    """Rusticated stucco, 2 m a repeat: the stucco cut into horizontal bands 50 cm high by deep
    V-grooves, the band faces softly weathered, darker in the grooves where dust sits."""
    rng = np.random.default_rng(57)
    n = 512
    L = Layer(n, n, rng)
    y = np.mgrid[0:n, 0:n][0].astype(np.float32)
    fv = (y % (n / 4)) / (n / 4)
    # A crisp, narrow channel at each joint (painted light puts a highlight on the band below
    # and a shadow line under the one above), each band a touch different in tone.
    band = np.clip(np.minimum(fv, 1 - fv) / 0.022, 0, 1) ** 0.6
    row = (y // (n / 4)).astype(int)
    tone = np.array([0.0, 0.025, -0.015, 0.012])[row % 4]
    trowel = periodic_noise(n, n, rng, 3, 2)
    weather = np.clip(periodic_noise(n, n, rng, 2, 2) * 1.5 - 0.5, 0, 1)
    g = (0.90 + tone - 0.025 * trowel - 0.04 * weather) * (0.84 + 0.16 * band)
    L.col = np.dstack([g, g * 0.98, g * 0.94]).astype(np.float32)
    L.hgt = 2.0 * band + 0.3 * trowel
    L.rough[:] = 0.9
    L.save("rustic", 1.6)


# Goods on shop shelves (crate::buildings SignKind::Shelves): an atlas of square cells, a trade a
# column, two stockings of it a row. Each cell a shelf unit 2.2 m square: a back board, four
# wooden shelves, the stock standing on them lit from above, shadowed under the shelf over it.
GOODS_TRADES = ("kirana", "bartan", "kapda", "dabba")
GOODS_CELL = 512


def goods():
    rng = np.random.default_rng(61)
    S = GOODS_CELL
    L = Layer(2 * S, len(GOODS_TRADES) * S, rng)
    grain = periodic_noise(2 * S, len(GOODS_TRADES) * S, rng, 3, 24)
    L.col = np.dstack([0.16 + 0.04 * grain, 0.11 + 0.03 * grain, 0.07 + 0.02 * grain]).astype(np.float32)
    L.hgt[:] = -2.0
    L.rough[:] = 0.85
    rows = 4
    for ti, trade in enumerate(GOODS_TRADES):
        for vi in range(2):
            ox, oy = ti * S, vi * S
            r = np.random.default_rng(100 + ti * 7 + vi)
            pitch = S / rows
            for k in range(rows):
                top, base = int(oy + k * pitch), int(oy + (k + 1) * pitch)
                board = int(pitch * 0.07)
                # Shadow under the shelf above, fading down the back board.
                ys = np.arange(top, base)
                sh = np.clip((ys - top) / (pitch * 0.5), 0, 1)[:, None, None]
                L.col[top:base, ox:ox + S] *= 0.55 + 0.45 * sh
                stock(L, r, trade, ox + 6, ox + S - 6, top + int(pitch * 0.12), base - board)
                # The shelf board: wood grain, a lit front edge, a dark line under it.
                b0 = base - board
                g = grain[b0:base, ox:ox + S]
                L.col[b0:base, ox:ox + S] = np.dstack([0.42 + 0.08 * g, 0.29 + 0.06 * g, 0.17 + 0.04 * g])
                L.col[b0:b0 + 2, ox:ox + S] *= 1.35
                L.hgt[b0:base, ox:ox + S] = 3.0
                L.rough[b0:base, ox:ox + S] = 0.7
            # Uprights at the sides.
            for x0 in (ox, ox + S - 6):
                L.col[oy:oy + S, x0:x0 + 6] = np.array(lin([0.36, 0.25, 0.15]))
                L.hgt[oy:oy + S, x0:x0 + 6] = 3.5
    L.save("goods", 2.0)


def stock(L, r, trade, x0, x1, top, floor):
    """One shelf's worth of `trade`, standing on `floor` (pixel row), no higher than `top`."""
    x = x0 + int(r.integers(0, 6))
    while x < x1 - 8:
        room = floor - top
        if trade == "kirana":
            kind = r.choice(["jar", "jar", "bottle", "packet", "packet", "tin"])
            if kind == "jar":
                w, h = int(r.integers(26, 40)), int(room * r.uniform(0.55, 0.8))
                fill = lin([[0.92, 0.75, 0.25], [0.95, 0.93, 0.85], [0.70, 0.18, 0.10], [0.55, 0.38, 0.22], [0.30, 0.50, 0.22]][r.integers(0, 5)])
                cylinder(L, x, w, floor - h, floor, fill, gloss=0.25, rough=0.15)
                cap = lin([[0.85, 0.15, 0.12], [0.15, 0.35, 0.70], [0.95, 0.85, 0.25]][r.integers(0, 3)])
                cylinder(L, x - 1, w + 2, floor - h - 6, floor - h, cap, gloss=0.3, rough=0.4)
                label(L, r, x + 4, w - 8, floor - int(h * 0.65), floor - int(h * 0.35))
            elif kind == "bottle":
                w, h = int(r.integers(14, 20)), int(room * r.uniform(0.7, 0.92))
                fill = lin([[0.95, 0.78, 0.20], [0.25, 0.45, 0.20], [0.55, 0.12, 0.08]][r.integers(0, 3)])
                cylinder(L, x, w, floor - int(h * 0.7), floor, fill, gloss=0.35, rough=0.1)
                cylinder(L, x + w // 3, max(w // 3, 4), floor - h, floor - int(h * 0.7), fill, gloss=0.35, rough=0.1)
                label(L, r, x + 2, w - 4, floor - int(h * 0.5), floor - int(h * 0.25))
            elif kind == "tin":
                w, h = int(r.integers(22, 32)), int(room * r.uniform(0.35, 0.5))
                cylinder(L, x, w, floor - h, floor, lin([0.70, 0.70, 0.68]), gloss=0.5, rough=0.3, metal=1.0)
                label(L, r, x, w, floor - int(h * 0.8), floor - int(h * 0.2))
            else:
                w, h = int(r.integers(20, 34)), int(room * r.uniform(0.4, 0.75))
                packet(L, r, x, w, floor - h, floor)
            x += w + int(r.integers(1, 5))
        elif trade == "bartan":
            kind = r.choice(["plates", "glasses", "pot", "pot", "plates"])
            steel = np.array(lin([0.74, 0.74, 0.72]))
            if kind == "plates":
                w, n = int(r.integers(50, 70)), int(r.integers(6, 14))
                for j in range(n):
                    yy = floor - 4 * (j + 1)
                    cylinder(L, x, w, yy, yy + 4, steel * (0.85 + 0.15 * (j % 2)), gloss=0.6, rough=0.2, metal=1.0)
            elif kind == "glasses":
                w = int(r.integers(16, 22))
                for j in range(int(r.integers(2, 4))):
                    cylinder(L, x, w, floor - int(room * 0.45) - j * 8, floor - j * 8, steel, gloss=0.7, rough=0.15, metal=1.0)
            else:
                w, h = int(r.integers(44, 70)), int(room * r.uniform(0.4, 0.7))
                col = steel if r.random() < 0.7 else np.array(lin([0.72, 0.45, 0.25]))
                cylinder(L, x, w, floor - h, floor, col, gloss=0.7, rough=0.2, metal=1.0)
                cylinder(L, x - 3, w + 6, floor - h - 3, floor - h + 2, col * 1.05, gloss=0.7, rough=0.2, metal=1.0)
            x += w + int(r.integers(2, 6))
        elif trade == "kapda":
            # Folded cloth in a pile: soft-edged slabs, each its own colour, a border or a stripe.
            w = int(r.integers(54, 80))
            yy = floor
            while yy - top > 14:
                hh = int(r.integers(9, 15))
                c = lin([[0.75, 0.10, 0.25], [0.95, 0.62, 0.10], [0.12, 0.40, 0.45], [0.90, 0.85, 0.75], [0.40, 0.15, 0.45], [0.20, 0.35, 0.65],
                         [0.85, 0.35, 0.15], [0.30, 0.50, 0.25]][r.integers(0, 8)])
                cloth(L, r, x + int(r.integers(-2, 3)), w, yy - hh, yy, c)
                yy -= hh
            x += w + int(r.integers(3, 8))
        else:
            # Cartons, kraft brown, printed panels, sometimes two stacked.
            w, h = int(r.integers(40, 70)), int(room * r.uniform(0.45, 0.95))
            if r.random() < 0.35 and h > room * 0.6:
                h1 = h // 2
                carton(L, r, x, w, floor - h1, floor)
                carton(L, r, x + 3, w - 6, floor - h, floor - h1)
            else:
                carton(L, r, x, w, floor - h, floor)
            x += w + int(r.integers(2, 6))


def _region(L, x, w, y0, y1):
    x, w = max(x, 0), min(w, L.w - max(x, 0))
    y0, y1 = max(int(y0), 0), min(int(y1), L.h)
    return x, w, y0, y1


def cylinder(L, x, w, y0, y1, col, gloss=0.3, rough=0.4, metal=0.0):
    """An upright round thing seen side on: shaded across its width, a highlight left of middle."""
    x, w, y0, y1 = _region(L, x, w, y0, y1)
    if w <= 1 or y1 <= y0:
        return
    t = (np.arange(w) + 0.5) / w * 2 - 1
    shade = np.sqrt(np.clip(1 - t * t, 0, 1)) * 0.7 + 0.3
    hi = np.exp(-((t + 0.35) / 0.12) ** 2) * gloss
    c = np.asarray(col, np.float32)[None, None, :] * shade[None, :, None] + hi[None, :, None]
    L.col[y0:y1, x:x + w] = np.clip(c, 0, 1)
    L.hgt[y0:y1, x:x + w] = 2.0 + 2.0 * shade[None, :]
    L.rough[y0:y1, x:x + w] = rough
    L.metal[y0:y1, x:x + w] = metal


def label(L, r, x, w, y0, y1):
    """A printed label: a pale band, a coloured stripe and a line of 'lettering'."""
    x, w, y0, y1 = _region(L, x, w, y0, y1)
    if w <= 2 or y1 <= y0 + 2:
        return
    L.col[y0:y1, x:x + w] = L.col[y0:y1, x:x + w] * 0.3 + 0.7 * np.array(lin([0.93, 0.90, 0.82]))
    m = (y0 + y1) // 2
    L.col[m - 1:m + 1, x + 2:x + w - 2] = np.array(lin([[0.80, 0.15, 0.10], [0.10, 0.30, 0.65], [0.15, 0.50, 0.20]][r.integers(0, 3)]))
    L.metal[y0:y1, x:x + w] = 0.0


def packet(L, r, x, w, y0, y1):
    c = np.array(lin([[0.85, 0.25, 0.15], [0.95, 0.75, 0.15], [0.25, 0.50, 0.75], [0.30, 0.60, 0.30], [0.90, 0.88, 0.80], [0.60, 0.20, 0.45]][r.integers(0, 6)]))
    x, w, y0, y1 = _region(L, x, w, y0, y1)
    if w <= 2 or y1 <= y0:
        return
    h = y1 - y0
    v = (np.arange(h) / h)[:, None, None]
    L.col[y0:y1, x:x + w] = c * (0.8 + 0.25 * (1 - v))
    L.col[y0:y1, x:x + 2] *= 0.7
    L.col[y0:y1, x + w - 2:x + w] *= 0.6
    L.hgt[y0:y1, x:x + w] = 3.0
    L.rough[y0:y1, x:x + w] = 0.5
    label(L, r, x + 3, w - 6, y0 + int(h * 0.3), y0 + int(h * 0.55))


def cloth(L, r, x, w, y0, y1, col):
    x, w, y0, y1 = _region(L, x, w, y0, y1)
    if w <= 2 or y1 <= y0:
        return
    h = y1 - y0
    v = (np.arange(h) + 0.5) / h
    bulge = (np.sin(v * np.pi) * 0.35 + 0.65)[:, None, None]
    c = np.asarray(col, np.float32) * bulge
    L.col[y0:y1, x:x + w] = c
    if r.random() < 0.6:
        b = max(1, h // 5)
        L.col[y0 + b:y0 + 2 * b, x:x + w] = np.array(lin([0.95, 0.80, 0.35])) * 0.9
    L.col[y0:y1, x:x + 2] *= 0.75
    L.hgt[y0:y1, x:x + w] = 2.0 + bulge[..., 0] * 1.5
    L.rough[y0:y1, x:x + w] = 0.95


def carton(L, r, x, w, y0, y1):
    x, w, y0, y1 = _region(L, x, w, y0, y1)
    if w <= 4 or y1 <= y0 + 4:
        return
    kraft = np.array(lin([0.62, 0.47, 0.30])) * r.uniform(0.85, 1.1)
    L.col[y0:y1, x:x + w] = kraft
    # Tape across the top flap seam, and a printed panel with a logo bar.
    m = y0 + (y1 - y0) // 6
    L.col[m - 1:m + 2, x:x + w] = kraft * 1.15
    px0, px1 = x + w // 5, x + w - w // 5
    py0, py1 = y0 + (y1 - y0) // 3, y0 + (y1 - y0) * 2 // 3
    if px1 > px0 + 2 and py1 > py0 + 2:
        L.col[py0:py1, px0:px1] = np.array(lin([[0.90, 0.88, 0.82], [0.80, 0.15, 0.12], [0.15, 0.30, 0.60]][r.integers(0, 3)]))
        L.col[py0 + 2:py0 + 4, px0 + 2:px1 - 2] = np.array(lin([0.10, 0.10, 0.10]))
    L.col[y0:y1, x:x + 2] *= 0.7
    L.col[y0:y1, x + w - 2:x + w] *= 0.6
    L.hgt[y0:y1, x:x + w] = 3.0
    L.rough[y0:y1, x:x + w] = 0.85


JOBS = {f.__name__: f for f in (facade, shop, rustic, limewash, stucco, stone, roof, ground, wear, goods)}
# All of them, or just those named: python3 tools/textures/city.py goods
for job in sys.argv[1:] or JOBS:
    JOBS[job]()

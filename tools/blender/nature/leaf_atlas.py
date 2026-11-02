"""The leaf atlas: real leaves on twigs, at their real size.

Six 1024 px tiles, 3 x 2, in the order textures.quadrant_uv reads them. Every leaf is a photograph
from ambientCG's leaf sets (CC0): LeafSet023 for the peepal's heart with its drip tip, 022 for the
banyan's oval, 013's narrow blades for neem leaflets and palm fronds, 024 for the bushes. A card is
about 2 m across, so a tile holds a hundred-odd leaves on twigs where the drawn atlas had fifteen:
the same cards read as a real tree's many small leaves, with no new triangles, and the gaps between
leaves are what show sky through a crown. Whole leaves stop short of the tile's edge, so no card
shows its outline.

The photos are bright scans. Real foliage reflects about a tenth of the light, and Delhi's is dusty
olive rather than lawn green, so each set is brought to a measured mean colour first.

Needs numpy, scipy and Pillow, not Blender. The raw sets stay out of git, each unpacked into its own
build/look_src/<set>, from https://ambientcg.com/get?file=<set>_2K-PNG.zip. Then, in this order:

    python3 tools/blender/nature/leaf_atlas.py [build/textures/trees/leaf_atlas.png]
    just nature
"""

import math
import os
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
SRC = os.path.join(ROOT, "build", "look_src")
TILE = 1024
COLS, ROWS = 3, 2
MARGIN = 10

# Where each set's photos point, as degrees to turn them so the tip is at +x (the stalk at -x).
UP, RIGHT = 90.0, 0.0
# Mean linear colour of each kind of leaf on the tree: a tenth of the light in the green, olive.
PEEPAL = (0.056, 0.100, 0.028)
BANYAN = (0.038, 0.072, 0.024)
NEEM = (0.042, 0.078, 0.022)
PALM = (0.076, 0.098, 0.030)
BUSH = (0.050, 0.086, 0.030)
STEM = (0.060, 0.048, 0.034)
PALE_STEM = (0.13, 0.115, 0.06)
# How much of a leaf's neighbourhood may be covered already before it goes elsewhere: this keeps the
# gaps open that sky shows through.
CROWDED = 0.62


def to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1.0 / 2.4) - 0.055)


def sprites(name, turn, least_area=20000):
    """A set's whole leaves as float arrays in linear light, straight alpha, tip at +x, cut from its
    photo by the opacity map. Leaves with holes bitten out are as welcome as whole ones."""
    colour = np.asarray(Image.open(os.path.join(SRC, name, f"{name}_2K-PNG_Color.png")).convert("RGB"), np.float32) / 255.0
    alpha = np.asarray(Image.open(os.path.join(SRC, name, f"{name}_2K-PNG_Opacity.png")).convert("L"), np.float32) / 255.0
    labels, n = ndimage.label(alpha > 0.5)
    out = []
    for k, box in enumerate(ndimage.find_objects(labels), start=1):
        h, w = box[0].stop - box[0].start, box[1].stop - box[1].start
        if h * w < least_area:
            continue
        y0, y1 = max(box[0].start - 4, 0), box[0].stop + 4
        x0, x1 = max(box[1].start - 4, 0), box[1].stop + 4
        rgb = to_linear(colour[y0:y1, x0:x1])
        a = alpha[y0:y1, x0:x1] * (labels[y0:y1, x0:x1] == k)
        rgba = Image.fromarray((np.dstack([to_srgb(rgb), a]) * 255.0).astype(np.uint8), "RGBA").convert("RGBa")
        out.append(rgba.rotate(-turn, Image.BICUBIC, expand=True))
    return out


def cleanest(sets, keep):
    """The `keep` sprites with the fewest dark blotches: a spotted leaf reads as disease this small."""
    def spots(s):
        a = np.asarray(s.convert("RGBA"), np.float32) / 255.0
        m = a[..., 3] > 0.5
        y = to_linear(a[..., :3])[m].mean(axis=1)
        return float((y < np.median(y) * 0.45).mean())
    return sorted(sets, key=spots)[:keep]


def mean_colour(sets):
    """The mean linear colour of some sprites, by covered area."""
    total, area = np.zeros(3), 0.0
    for s in sets:
        a = np.asarray(s.convert("RGBA"), np.float32) / 255.0
        m = a[..., 3] > 0.5
        total += to_linear(a[..., :3])[m].sum(axis=0)
        area += m.sum()
    return total / area


class Tile:
    """One tile being laid, back to front, in linear light."""

    def __init__(self, rng):
        self.rgb = np.zeros((TILE, TILE, 3), np.float32)
        self.a = np.zeros((TILE, TILE), np.float32)
        self.rng = rng

    def coverage(self):
        return float((self.a > 0.5).mean())

    def crowd(self, reach):
        """How covered each pixel's neighbourhood is, on a quarter-size map, to look up by `crowded`."""
        small = ndimage.zoom((self.a > 0.5).astype(np.float32), 0.25, order=1)
        self.crowding = ndimage.gaussian_filter(small, reach / 4.0)

    def crowded(self, x, y):
        i, j = min(int(y / 4), self.crowding.shape[0] - 1), min(int(x / 4), self.crowding.shape[1] - 1)
        return self.crowding[max(i, 0), max(j, 0)] > CROWDED

    def stem(self, pts, r0, r1, colour):
        """A twig along points, thinning from r0 to r1 pixels."""
        pts = np.asarray(pts, np.float32)
        for i in range(len(pts) - 1):
            (ax, ay), (bx, by) = pts[i], pts[i + 1]
            ra = r0 + (r1 - r0) * i / (len(pts) - 1)
            rb = r0 + (r1 - r0) * (i + 1) / (len(pts) - 1)
            pad = int(max(ra, rb) + 3)
            x0, x1 = max(0, int(min(ax, bx)) - pad), min(TILE, int(max(ax, bx)) + pad + 1)
            y0, y1 = max(0, int(min(ay, by)) - pad), min(TILE, int(max(ay, by)) + pad + 1)
            if x0 >= x1 or y0 >= y1:
                continue
            ys, xs = np.mgrid[y0:y1, x0:x1].astype(np.float32)
            vx, vy = bx - ax, by - ay
            t = np.clip(((xs - ax) * vx + (ys - ay) * vy) / (vx * vx + vy * vy + 1e-6), 0.0, 1.0)
            d = np.hypot(xs - (ax + t * vx), ys - (ay + t * vy))
            r = ra + (rb - ra) * t
            cover = np.clip(r + 0.5 - d, 0.0, 1.0)
            for c in range(3):
                self.rgb[y0:y1, x0:x1, c] += (colour[c] - self.rgb[y0:y1, x0:x1, c]) * cover
            self.a[y0:y1, x0:x1] = np.maximum(self.a[y0:y1, x0:x1], cover)

    def leaf(self, sprite, x, y, angle, length, across, scale, tint, shade=0.28):
        """Lays a leaf with its stalk at (x, y) pointing at `angle` (radians), `length` pixels long and
        narrowed by `across` (a leaf tilted away from the eye), its colour scaled by `scale` to the
        wanted mean and by `tint`. Returns False, laying nothing, if it would cross the tile's edge."""
        w, h = sprite.size
        new_w = max(4, int(length))
        new_h = max(3, int(h * length / w * across))
        small = sprite.resize((new_w, new_h), Image.LANCZOS)
        deg = -math.degrees(angle)
        turned = small.rotate(deg, Image.BICUBIC, expand=True)
        tw, th = turned.size
        # The stalk end of the leaf, once turned, is where the twig holds it.
        cx = x + math.cos(angle) * new_w * 0.5
        cy = y + math.sin(angle) * new_w * 0.5
        x0, y0 = int(cx - tw * 0.5), int(cy - th * 0.5)
        if x0 < MARGIN or y0 < MARGIN or x0 + tw > TILE - MARGIN or y0 + th > TILE - MARGIN:
            return False
        px = np.asarray(turned.convert("RGBA"), np.float32) / 255.0
        a = px[..., 3]
        rgb = to_linear(px[..., :3]) * scale * np.asarray(tint, np.float32)
        here = (slice(y0, y0 + th), slice(x0, x0 + tw))
        # What it shades: what lies under it goes darker, so leaves stand off each other.
        if shade > 0.0:
            soft = ndimage.gaussian_filter(np.pad(a, 8), 4.0)
            sy0, sx0 = max(y0 - 8, 0), max(x0 - 8, 0)
            sy1, sx1 = min(y0 + th + 8, TILE), min(x0 + tw + 8, TILE)
            cut = soft[sy0 - (y0 - 8):sy1 - (y0 - 8), sx0 - (x0 - 8):sx1 - (x0 - 8)]
            self.rgb[sy0:sy1, sx0:sx1] *= (1.0 - shade * cut)[..., None]
        self.rgb[here] = self.rgb[here] * (1.0 - a[..., None]) + rgb * a[..., None]
        self.a[here] = np.maximum(self.a[here], a)
        return True


def scale_to(sets, target):
    """What to multiply a set by so its mean is `target`, by channel."""
    return np.asarray(target, np.float32) / np.maximum(mean_colour(sets), 1e-4)


def leaf_tint(rng, spread=0.2, yellow=0.06):
    """One leaf's own colour: lighter or darker, and now and then an old yellow one."""
    v = 1.0 + rng.uniform(-spread, spread)
    w = rng.uniform(-0.1, 0.1)
    if rng.random() < yellow:
        return (v * 1.5, v * 1.15, v * 0.8)
    return (v * (1.0 + w), v, v * (1.0 - w))


def walk(rng, x, y, angle, steps, step, wander):
    """A twig's points."""
    pts = [(x, y)]
    for _ in range(steps):
        angle += rng.uniform(-wander, wander)
        x += math.cos(angle) * step
        y += math.sin(angle) * step
        pts.append((x, y))
    return pts, angle


def twigs(rng, count, reach):
    """Twigs spreading from near the tile's middle, each (points, thickness at its foot)."""
    out = []
    for k in range(count):
        a = 2 * math.pi * (k + rng.uniform(-0.3, 0.3)) / count
        d = rng.uniform(0.0, 0.14) * TILE
        x, y = TILE * 0.5 + math.cos(a + 2.4) * d, TILE * 0.5 + math.sin(a + 2.4) * d
        steps = 7
        pts, _ = walk(rng, x, y, a, steps, reach * TILE / steps, 0.32)
        out.append(pts)
    return out


def leafy(rng, sprites_, target, count, length, across=(0.55, 1.0), yellow=0.06, stalk=0.0, twig_count=6, want=0.5):
    """A tile of leaves on twigs: the deep leaves first and darker, the near ones lit. `length` is in
    pixels, a leaf's real length at the card's scale."""
    t = Tile(rng)
    scale = scale_to(sprites_, target)
    paths = twigs(rng, twig_count, 0.40)
    for p in paths:
        t.stem(p, rng.uniform(3.0, 4.5), 1.2, STEM)
    laid = 0
    tries = 0
    t.crowd(length)
    while tries < count * 12 and (laid < count or t.coverage() < want):
        tries += 1
        if tries % 12 == 0:
            t.crowd(length)
        # Anywhere along a twig, with a side shoot's worth of scatter.
        p = paths[rng.integers(len(paths))]
        k = rng.uniform(0.0, len(p) - 1.001)
        i = int(k)
        f = k - i
        x = p[i][0] + (p[i + 1][0] - p[i][0]) * f + rng.normal(0.0, 0.07) * TILE
        y = p[i][1] + (p[i + 1][1] - p[i][1]) * f + rng.normal(0.0, 0.07) * TILE
        if t.crowded(x + 0.5 * length * rng.uniform(-1.0, 1.0), y + 0.5 * length * rng.uniform(-1.0, 1.0)):
            continue
        depth = min(1.0, laid / max(count, 1))
        along = math.atan2(p[i + 1][1] - p[i][1], p[i + 1][0] - p[i][0])
        angle = along + rng.choice([-1, 1]) * rng.uniform(0.25, 1.35)
        if stalk > 0.0:
            d = stalk * length
            t.stem([(x - math.cos(angle) * d, y - math.sin(angle) * d), (x, y)], 1.3, 1.0, PALE_STEM)
        tint = np.asarray(leaf_tint(rng, yellow=yellow)) * (0.62 + 0.5 * depth)
        ok = t.leaf(sprites_[rng.integers(len(sprites_))], x, y, angle, length * rng.uniform(0.8, 1.2), rng.uniform(*across), scale, tint)
        laid += ok
    return t, laid


def neem(rng, blades):
    """Neem's leaves are sprays: a thin stalk with a dozen narrow leaflets in pairs."""
    t = Tile(rng)
    scale = scale_to(blades, NEEM)
    paths = twigs(rng, 6, 0.36)
    for p in paths:
        t.stem(p, rng.uniform(3.0, 4.0), 1.2, STEM)
    laid = 0
    t.crowd(90.0)
    for sprays in range(400):
        if t.coverage() > 0.42:
            break
        if sprays % 4 == 0:
            t.crowd(90.0)
        p = paths[rng.integers(len(paths))]
        k = rng.uniform(0.0, len(p) - 1.001)
        i = int(k)
        f = k - i
        x = p[i][0] + (p[i + 1][0] - p[i][0]) * f
        y = p[i][1] + (p[i + 1][1] - p[i][1]) * f
        angle = rng.uniform(0.0, 2.0 * math.pi)
        length = rng.uniform(150.0, 230.0)
        pts, end = walk(rng, x, y, angle, 6, length / 6.0, 0.12)
        if not all(MARGIN + 30 < v < TILE - MARGIN - 30 for q in pts for v in q):
            continue
        if t.crowded(*pts[len(pts) // 2]):
            continue
        t.stem(pts, 1.6, 0.9, PALE_STEM)
        depth = min(1.0, sprays / 120.0)
        base = (0.62 + 0.5 * depth) * rng.uniform(0.85, 1.15)
        n = rng.integers(10, 15)
        for j in range(n):
            s = (j + 1.0) / (n + 1.0)
            q = int(s * (len(pts) - 1))
            qx = pts[q][0] + (pts[q + 1][0] - pts[q][0]) * (s * (len(pts) - 1) - q)
            qy = pts[q][1] + (pts[q + 1][1] - pts[q][1]) * (s * (len(pts) - 1) - q)
            run = math.atan2(pts[q + 1][1] - pts[q][1], pts[q + 1][0] - pts[q][0])
            for side in (-1, 1):
                a = run + side * rng.uniform(0.75, 1.15)
                size = rng.uniform(46.0, 62.0) * (1.0 - 0.35 * abs(2.0 * s - 1.0))
                tint = np.asarray(leaf_tint(rng, 0.12, 0.03)) * base
                laid += t.leaf(blades[rng.integers(len(blades))], qx, qy, a, size, rng.uniform(0.55, 0.95), scale, tint, shade=0.14)
        laid += t.leaf(blades[rng.integers(len(blades))], pts[-1][0], pts[-1][1], end, 50.0, 0.8, scale, np.asarray(leaf_tint(rng, 0.12, 0.03)) * base, shade=0.14)
    return t, laid


def palm(rng, blades):
    """One frond's length: a midrib up the tile, long leaflets off both sides, angled toward its tip.
    trees.palm_fronds folds a strip along this midrib, so its middle must stay the middle."""
    t = Tile(rng)
    scale = scale_to(blades, PALM)
    mid = TILE * 0.5
    t.stem([(mid, TILE - MARGIN - 1), (mid, MARGIN + 1)], 7.0, 2.5, PALE_STEM)
    laid = 0
    rows = 62
    for i in range(rows):
        y = MARGIN + 40.0 + (TILE - 2 * MARGIN - 80.0) * (i + 0.5) / rows
        for side in (-1, 1):
            a = -math.pi / 2.0 + side * rng.uniform(1.05, 1.3)
            size = rng.uniform(400.0, 480.0)
            # Stretched from a narrow blade: a coconut leaflet's strip.
            tint = np.asarray(leaf_tint(rng, 0.16, 0.12))
            laid += t.leaf(blades[rng.integers(len(blades))], mid, y, a, size, 0.15, scale, tint, shade=0.08)
    return t, laid


def flowers(t, rng, hearts, colours, heads, radius, florets, size):
    """Small heads of petals over a bush tile: a photographed leaf in another colour for each, as
    bracts and florets are leaf-shaped and papery, and no CC0 scan of these exists."""
    white = 1.0 / np.maximum(mean_colour(hearts), 1e-4)
    for _ in range(heads):
        cx, cy = rng.uniform(0.15, 0.85) * TILE, rng.uniform(0.15, 0.85) * TILE
        for _ in range(florets):
            a, r = rng.uniform(0.0, 2.0 * math.pi), radius * math.sqrt(rng.random())
            c = np.asarray(colours[rng.integers(len(colours))], np.float32) * rng.uniform(0.75, 1.1)
            t.leaf(hearts[rng.integers(len(hearts))], cx + math.cos(a) * r, cy + math.sin(a) * r, rng.uniform(0.0, 2.0 * math.pi), size * rng.uniform(0.8, 1.2), rng.uniform(0.6, 1.0), white, c, shade=0.1)


def build(out, seed=3):
    rng = np.random.default_rng(seed)
    peepal_set = sprites("LeafSet023", RIGHT)
    banyan_set = sprites("LeafSet022", UP)
    blades = cleanest(sprites("LeafSet013", UP), 3)
    banyan_set = cleanest(banyan_set, 4)
    bush_set = sprites("LeafSet024", UP)
    print("sprites", len(peepal_set), len(banyan_set), len(blades), len(bush_set))
    tiles = []
    t, n = neem(rng, blades)
    tiles.append(("neem", t, n))
    # Peepal: a 14 cm heart on a card of 2.1 m, its long stalk among the twigs.
    t, n = leafy(rng, peepal_set, PEEPAL, 110, 92.0, stalk=0.25, want=0.42)
    tiles.append(("peepal", t, n))
    # Banyan: a 14 cm oval on a card of 2.4 m, leathery and dark.
    t, n = leafy(rng, banyan_set, BANYAN, 130, 70.0, yellow=0.03, want=0.42)
    tiles.append(("banyan", t, n))
    t, n = palm(rng, blades)
    tiles.append(("palm", t, n))
    # Lantana: 6 cm leaves on a card of 0.75 m, with its small mixed heads.
    t, n = leafy(rng, bush_set, BUSH, 90, 96.0, yellow=0.04, want=0.42)
    flowers(t, rng, peepal_set, [(0.42, 0.15, 0.03), (0.34, 0.05, 0.10), (0.50, 0.32, 0.06)], 12, 26.0, 34, 13.0)
    tiles.append(("lantana", t, n))
    # Bougainvillea: papery bracts among smaller leaves.
    t, n = leafy(rng, bush_set, BUSH, 80, 84.0, yellow=0.02, want=0.36)
    flowers(t, rng, peepal_set, [(0.30, 0.03, 0.13), (0.36, 0.05, 0.16), (0.25, 0.02, 0.10)], 32, 46.0, 14, 40.0)
    tiles.append(("bougainvillea", t, n))

    atlas = np.zeros((TILE * ROWS, TILE * COLS, 4), np.float32)
    for q, (name, t, n) in enumerate(tiles):
        x, y = (q % COLS) * TILE, (q // COLS) * TILE
        atlas[y:y + TILE, x:x + TILE, :3] = t.rgb
        atlas[y:y + TILE, x:x + TILE, 3] = t.a
        m = t.a > 0.5
        print(f"{name:14} {n:4} leaves, cover {m.mean():.2f}, mean linear {t.rgb[m].mean(axis=0).round(3)}")
    # Bleed the colour out under the transparent pixels, or mip-mapping fringes the leaves dark.
    solid = atlas[..., 3] > 0.02
    far = ndimage.distance_transform_edt(~solid, return_distances=False, return_indices=True)
    bled = atlas[..., :3][far[0], far[1]]
    atlas[..., :3] = np.where(solid[..., None], atlas[..., :3], bled)
    rgba = np.dstack([to_srgb(atlas[..., :3]), atlas[..., 3]])
    os.makedirs(os.path.dirname(out), exist_ok=True)
    Image.fromarray((rgba * 255.0 + 0.5).astype(np.uint8), "RGBA").save(out, optimize=True)
    return atlas


if __name__ == "__main__":
    target = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "build", "textures", "trees", "leaf_atlas.png")
    build(target)
    print("wrote", target, round(os.path.getsize(target) / 1e6, 1), "MB")

"""What a Delhi market sells: the visual-only goods that dress the street kit's stalls and the shop
fronts. Heaps of fruit and vegetables, crates, open sacks of spice, stacked grain sacks, racks of
hanging cloth, bolts of cloth, steel pots, buckets, cartons, tarps on bamboo poles, pavement spreads
(a patri) laid out on the ground, and a crow.

One file, every piece a mesh of its own (`heap_orange`, ...), coloured per vertex and mapped into
one atlas of tiles (goods_atlas.py), with no materials of its own: crates/game/src/life/goods.rs
copies them into one mesh per patch of street with one shared material, so a whole market is a few
draws. No collider stands behind any of this, so what stands at a warrior's height keeps to a wall
or to a piece of kit that has its own collider, what lies in the open stays below his shins, and
what hangs in the open is over his head. Nothing carries a logo or a word.

Each piece stands on its origin (z = 0), facing Blender +Y (game -Z). Sizes are real, in metres.

blender -b --factory-startup -P tools/blender/life/goods.py
"""

import math
import os
import random

import bmesh
import bpy
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "models", "life", "goods.glb")

rng = random.Random(7)

# The atlas: goods_atlas.py's tiles, by name, in rows of six.
TILES = [
    "white", "orange", "apple", "tomato", "onion", "potato",
    "guava", "banana", "cauli", "greens", "coconut", "brinjal",
    "chilli", "turmeric", "coriander", "rice", "lentil", "jute",
    "gingham", "floral", "linen", "quatrefoil", "poplin", "denim",
    "terlenka", "tarp", "card", "wood", "steel", "plastic",
]
TILE, SIDE, COLS = 170, 1024, 6


def tile_uv(name, u, v):
    k = TILES.index(name)
    col, row = k % COLS, k // COLS
    # The exporter turns Blender's v over; the atlas's rows count from its top.
    return ((col + 0.03 + 0.94 * u) * TILE / SIDE, 1.0 - (row + 0.03 + 0.94 * v) * TILE / SIDE)


def lin(rgb):
    return tuple(c ** 2.2 for c in rgb)


def vary(rgb, amount=0.08):
    """The same thing, a little different."""
    k = 1.0 + rng.uniform(-amount, amount)
    return tuple(min(1.0, max(0.0, c * k + rng.uniform(-amount, amount) * 0.25)) for c in rgb)


WHITE = (1.0, 1.0, 1.0)


class Piece:
    def __init__(self, name):
        self.name = name
        self.bm = bmesh.new()
        self.col = self.bm.loops.layers.float_color.new("Col")
        self.uv = self.bm.loops.layers.uv.new("UVMap")

    def paint(self, rgb=WHITE, tile="white", map_uv=None, shade=None):
        """Paints the faces not painted yet. `map_uv(point)` says where in the tile a corner goes,
        `shade(point)` darkens by where it is."""
        c = lin(rgb)
        for f in self.bm.faces:
            if f.tag:
                continue
            for loop in f.loops:
                k = shade(loop.vert.co) if shade else 1.0
                loop[self.col] = (c[0] * k, c[1] * k, c[2] * k, 1.0)
                u, v = map_uv(loop.vert.co) if map_uv else (0.5, 0.5)
                loop[self.uv].uv = tile_uv(tile, max(0.0, min(1.0, u)), max(0.0, min(1.0, v)))
            f.tag = True

    def box(self, mn, mx, rgb=WHITE, tile="white", shade=None):
        """A box, its tile laid over each face from its largest sides' corners."""
        bmesh.ops.create_cube(self.bm, size=1.0, matrix=Matrix.Translation(((mn[0] + mx[0]) / 2, (mn[1] + mx[1]) / 2, (mn[2] + mx[2]) / 2)) @ Matrix.Diagonal((mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2], 1.0)))
        size = [max(mx[k] - mn[k], 1e-4) for k in range(3)]
        c = lin(rgb)
        for f in self.bm.faces:
            if f.tag:
                continue
            n = f.normal
            ax = max(range(3), key=lambda k: abs(n[k]))
            a, b = [k for k in range(3) if k != ax]
            for loop in f.loops:
                p = loop.vert.co
                k = shade(p) if shade else 1.0
                loop[self.col] = (c[0] * k, c[1] * k, c[2] * k, 1.0)
                loop[self.uv].uv = tile_uv(tile, (p[a] - mn[a]) / size[a], (p[b] - mn[b]) / size[b])
            f.tag = True

    def rod(self, a, b, r, rgb, seg=5, r2=None, tile="white"):
        a, b = Vector(a), Vector(b)
        d = b - a
        rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
        bmesh.ops.create_cone(self.bm, cap_ends=True, segments=seg, radius1=r, radius2=r if r2 is None else r2, depth=d.length, matrix=Matrix.Translation(a + d / 2) @ rot)
        self.paint(rgb, tile, (lambda p: (0.5 + 0.4 * math.sin(p.x * 40 + p.y * 40), (p - a).length / max(d.length, 1e-3))) if tile != "white" else None)

    def lathe(self, profile, rgb=WHITE, seg=9, at=(0, 0, 0), tile="white", shade=None, closed=True, scale=(1, 1, 1), height=None):
        """A round thing from (radius, height) pairs, its tile wrapped round it, mirrored at the
        seam so nothing shows."""
        rings = []
        for r, z in profile:
            rings.append([self.bm.verts.new((at[0] + scale[0] * r * math.cos(k / seg * math.tau), at[1] + scale[1] * r * math.sin(k / seg * math.tau), at[2] + scale[2] * z)) for k in range(seg)])
        for a, b in zip(rings, rings[1:]):
            for k in range(seg):
                self.bm.faces.new((a[k], a[(k + 1) % seg], b[(k + 1) % seg], b[k]))
        if closed:
            self.bm.faces.new(rings[-1])
        top = height or max(z for _, z in profile) * scale[2]
        def around(p):
            ang = (math.atan2(p.y - at[1], p.x - at[0]) / math.tau) % 1.0
            return abs(2 * ang - 1), (p.z - at[2]) / max(top, 1e-3)
        self.paint(rgb, tile, around, shade)

    def heap(self, cx, cy, z0, rx, ry, h, tile, rgb=WHITE, rough=0.05):
        """A dome with its tile laid on from above: a heap of hundreds of things for fifty faces.
        Its edge is lumpy, as a heap's is."""
        rings = 5
        seg = 11
        top = self.bm.verts.new((cx, cy, z0 + h))
        all_rings = []
        for i in range(1, rings + 1):
            t = i / rings
            polar = t * math.pi / 2
            ring = []
            for k in range(seg):
                a = k / seg * math.tau
                lump = 1 + rough * math.sin(a * 5 + cx * 9 + i) + rough * 0.6 * math.sin(a * 3 + cy * 7 + 2 * i)
                ring.append(self.bm.verts.new((cx + rx * lump * math.sin(polar) * math.cos(a), cy + ry * lump * math.sin(polar) * math.sin(a), z0 + h * lump * math.cos(polar))))
            all_rings.append(ring)
        for k in range(seg):
            self.bm.faces.new((top, all_rings[0][k], all_rings[0][(k + 1) % seg]))
        for a, b in zip(all_rings, all_rings[1:]):
            for k in range(seg):
                self.bm.faces.new((a[k], b[k], b[(k + 1) % seg], a[(k + 1) % seg]))
        self.paint(rgb, tile, lambda p: (0.5 + (p.x - cx) / (2.1 * rx), 0.5 + (p.y - cy) / (2.1 * ry)), lambda p: 0.55 + 0.45 * min(1.0, (p.z - z0) / max(h, 1e-3) * 1.4))

    def sheet(self, grid, rgb=WHITE, tile="white", shade=None, tile_span=(1, 1)):
        """Cloth: a grid of points (rows of Vectors); the game's material draws both faces."""
        rows = [[self.bm.verts.new(p) for p in row] for row in grid]
        where = {}
        for i, row in enumerate(rows):
            for j, v in enumerate(row):
                where[v] = (j / (len(row) - 1) * tile_span[0], i / (len(rows) - 1) * tile_span[1])
        for a, b in zip(rows, rows[1:]):
            for k in range(len(a) - 1):
                self.bm.faces.new((a[k], a[k + 1], b[k + 1], b[k]))
        lookup = {tuple(v.co): where[v] for v in where}
        self.paint(rgb, tile, lambda p: lookup.get(tuple(p), (0.5, 0.5)), shade)

    def finish(self):
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces)
        me = bpy.data.meshes.new(self.name)
        self.bm.normal_update()
        self.bm.to_mesh(me)
        self.bm.free()
        me.color_attributes.active_color = me.color_attributes["Col"]
        me.color_attributes.render_color_index = me.color_attributes.active_color_index
        o = bpy.data.objects.new(self.name, me)
        bpy.context.scene.collection.objects.link(o)
        for p in me.polygons:
            p.use_smooth = True
        me.set_sharp_from_angle(angle=math.radians(50))
        return o


BAMBOO = (0.58, 0.49, 0.32)
STEEL = (0.86, 0.87, 0.90)
WOOD_DK = (0.62, 0.55, 0.50)


def crate_box(p, x, y, z, w, d, h, rgb=WHITE):
    """An open slatted crate."""
    t = 0.014
    for sx in (-1, 1):
        p.box((x + sx * w / 2 - t, y - d / 2, z), (x + sx * w / 2 + t, y + d / 2, z + h), vary(rgb, 0.06), "wood")
    for sy in (-1, 1):
        p.box((x - w / 2, y + sy * d / 2 - t, z), (x + w / 2, y + sy * d / 2 + t, z + h), vary(rgb, 0.06), "wood")
    p.box((x - w / 2, y - d / 2, z), (x + w / 2, y + d / 2, z + 0.02), vary(rgb, 0.06), "wood")


def thela_top(name, zones):
    """What's heaped on a fruit cart: its deck is 1 x 2 m at 0.8 m, rimmed to 0.92 m."""
    p = Piece(name)
    n = len(zones)
    for k, tile in enumerate(zones):
        y = -0.98 + (k + 0.5) * 1.96 / n
        p.heap(0.0, y, 0.82, 0.50, 1.0 / n + 0.06, 0.32, tile)
    return p


def thela_canopy(name, colour):
    """A sheet on four bamboo poles lashed to a cart, up over a warrior's head."""
    p = Piece(name)
    for x in (-0.55, 0.55):
        for y in (-1.0, 1.0):
            p.rod((x, y, 0.85), (x, y, 2.35 if y < 0 else 2.2), 0.018, BAMBOO, 4)
    grid = []
    for i in range(7):
        u = i / 6
        row = []
        for j in range(4):
            v = j / 3
            sag = 0.09 * math.sin(u * math.pi) * math.sin(v * math.pi) + 0.012 * math.sin(u * 19 + v * 7)
            row.append(Vector((-0.8 + 1.6 * v, -1.25 + 2.5 * u, 2.4 - 0.2 * u - sag)))
        grid.append(row)
    p.sheet(grid, colour, "tarp", lambda c: 0.86 + 0.14 * math.sin(c.y * 11.0))
    return p


def crate_of(name, tile):
    """A crate of something on the ground."""
    p = Piece(name)
    crate_box(p, 0, 0, 0.0, 0.56, 0.40, 0.24)
    p.heap(0, 0, 0.18, 0.28, 0.20, 0.13, tile)
    return p


def crate_stand(name, tiles):
    """Crates on crates, against a wall, tilted at the street."""
    p = Piece(name)
    for k, tile in enumerate(tiles):
        x = (k - (len(tiles) - 1) / 2) * 0.60
        crate_box(p, x, 0, 0.0, 0.56, 0.40, 0.30, WOOD_DK)
        crate_box(p, x, 0.03, 0.30, 0.56, 0.40, 0.22)
        p.heap(x, 0.03, 0.46, 0.28, 0.20, 0.14, tile)
    return p


def sack(p, x, y, r, h, rgb, top=None):
    """A jute sack, tied shut, or open with its neck rolled and a mound of what's in it."""
    bulge = [(r * 0.86, 0.0), (r * 1.02, h * 0.22), (r * 1.06, h * 0.55), (r * 0.98, h * 0.85)]
    dark = lambda c: 0.66 + 0.34 * min(1.0, c.z / h)  # noqa: E731
    if top is None:
        p.lathe(bulge + [(r * 0.5, h * 1.02), (r * 0.16, h * 1.1), (r * 0.25, h * 1.2)], rgb, 7, (x, y, 0), "jute", dark)
    else:
        p.lathe(bulge + [(r * 1.08, h * 0.94), (r * 0.92, h * 0.98)], rgb, 8, (x, y, 0), "jute", dark, closed=False)
        p.heap(x, y, h * 0.9, r * 0.95, r * 0.95, h * 0.24, top)


def sacks_open(name, tops):
    p = Piece(name)
    for k, top in enumerate(tops):
        x = (k - (len(tops) - 1) / 2) * 0.46 + rng.uniform(-0.03, 0.03)
        sack(p, x, rng.uniform(-0.05, 0.05), 0.2, 0.52, vary(WHITE, 0.07), top)
    return p


def sacks_stack(name):
    """Grain sacks laid flat and piled against a wall."""
    p = Piece(name)
    for z in range(3):
        for k in range(3 - z):
            x = (k - (2 - z) / 2) * 0.62 + rng.uniform(-0.03, 0.03)
            cz = z * 0.235
            c = vary(WHITE, 0.08) if rng.random() < 0.7 else (1.0, 0.98, 0.9)
            y = rng.uniform(-0.03, 0.03)
            p.lathe([(0.0, 0.0), (0.16, 0.0), (0.27, 0.07), (0.31, 0.14), (0.27, 0.21), (0.16, 0.27), (0.0, 0.28)], c, 8, (x, y, cz), "jute", lambda q, b=cz: 0.7 + 0.3 * min(1.0, (q.z - b) / 0.26), closed=False, scale=(1.0, 0.7, 1.0))
    return p


CLOTH_PLAIN = [(0.74, 0.20, 0.20), (0.82, 0.60, 0.18), (0.18, 0.34, 0.52), (0.20, 0.44, 0.32), (0.70, 0.30, 0.46), (0.86, 0.80, 0.70), (0.46, 0.22, 0.42), (0.78, 0.42, 0.18), (0.30, 0.30, 0.36), (0.86, 0.68, 0.58)]
CLOTH_TILES = [("linen", 0.6), ("poplin", 0.1), ("gingham", 0.1), ("floral", 0.1), ("quatrefoil", 0.05), ("denim", 0.05)]


def some_cloth():
    roll = rng.random()
    for tile, share in CLOTH_TILES:
        roll -= share
        if roll < 0:
            break
    # Patterned scans keep their own colours; plain linen takes a dye.
    return (tile, vary(rng.choice(CLOTH_PLAIN), 0.08) if tile == "linen" else vary((0.92, 0.92, 0.92), 0.05))


def garment(p, x, y, top, w, drop, tile, colour):
    """A length of cloth from a rail, in folds."""
    grid = []
    rows = 5
    for i in range(rows):
        u = i / (rows - 1)
        row = []
        for j in range(5):
            v = j / 4
            fold = 0.04 * math.sin(v * 11.0 + x * 7.0) * (0.3 + u)
            row.append(Vector((x + (v - 0.5) * w * (1.0 - 0.06 * u), y + fold, top - u * drop)))
        grid.append(row)
    p.sheet(grid, colour, tile, lambda c: 0.70 + 0.30 * abs(math.sin((c.x - x) / w * 11.0)), (0.5, min(1.0, drop)))


def rack_cloth(name, garments, drop=(1.0, 1.35)):
    """A rail along a wall with cloth hung the whole way."""
    p = Piece(name)
    w = 2.8
    p.rod((-w / 2, 0.40, 2.45), (w / 2, 0.40, 2.45), 0.012, STEEL, 4)
    for x in (-w / 2, w / 2):
        p.rod((x, 0.0, 2.5), (x, 0.40, 2.45), 0.01, STEEL, 4)
    for k in range(garments):
        x = (k + 0.5) / garments * w - w / 2
        tile, c = some_cloth()
        garment(p, x, 0.40 + rng.uniform(-0.02, 0.02), 2.44, w / garments * 1.25, rng.uniform(*drop), tile, c)
    return p


def rack_high(name, garments):
    """Cloth hung high on a shop front, above the shutter, clear of anyone."""
    p = Piece(name)
    w = 2.8
    p.rod((-w / 2, 0.12, 3.05), (w / 2, 0.12, 3.05), 0.012, STEEL, 4)
    for k in range(garments):
        x = (k + 0.5) / garments * w - w / 2
        tile, c = some_cloth()
        garment(p, x, 0.14, 3.04, w / garments * 1.3, rng.uniform(0.55, 0.85), tile, c)
    return p


def bolts(name):
    """Rolled cloth stacked on a low bench against a wall."""
    p = Piece(name)
    p.box((-0.8, -0.26, 0.0), (0.8, 0.26, 0.3), WOOD_DK, "wood")
    for row in range(3):
        n = 7 - row * 2
        for k in range(n):
            x = (k - (n - 1) / 2) * 0.2
            z = 0.39 + row * 0.16
            tile, c = some_cloth()
            p.rod((x, -0.3, z), (x, 0.3, z), 0.085, c, 6, tile=tile)
    return p


def pots(name):
    """Steel pots and pans, stacked by size."""
    p = Piece(name)
    p.box((-0.7, -0.26, 0.0), (0.7, 0.26, 0.28), WOOD_DK, "wood")
    for k in range(5):
        x = (k - 2) * 0.28
        z = 0.28
        for level in range(rng.randint(2, 4)):
            r = 0.13 - level * 0.016
            p.lathe([(r * 0.8, 0.0), (r, 0.02), (r, 0.1), (r * 1.06, 0.11)], STEEL, 8, (x, 0, z), "steel")
            z += 0.095
    return p


def buckets(name):
    p = Piece(name)
    for k, c in enumerate(((0.20, 0.34, 0.50), (0.62, 0.24, 0.20), (0.26, 0.42, 0.30), (0.66, 0.56, 0.24))):
        x = (k - 1.5) * 0.34
        for level in range(3 - k % 2):
            p.lathe([(0.1, 0.0), (0.145, 0.3)], vary(c, 0.05), 8, (x, 0, level * 0.075), "plastic", closed=False)
    return p


def cartons(name):
    """Plain cartons against a wall."""
    p = Piece(name)
    for (x, z, w, h, d) in ((-0.44, 0.0, 0.5, 0.42, 0.4), (0.1, 0.0, 0.56, 0.36, 0.42), (-0.28, 0.42, 0.46, 0.34, 0.36), (0.62, 0.0, 0.42, 0.5, 0.4)):
        p.box((x - w / 2, -d / 2, z), (x + w / 2, d / 2, z + h), vary(WHITE, 0.06), "card")
    return p


def tarp(name, w, depth, colour):
    """A tarp stretched from a wall out over the pavement: from y = 0 at the wall, 3.2 m up, to two
    bamboo poles out over the street at 2.7 m."""
    p = Piece(name)
    for x in (-w / 2 + 0.1, w / 2 - 0.1):
        p.rod((x, depth, 0.0), (x, depth, 2.72), 0.022, BAMBOO, 4)
    grid = []
    rows, cols = 7, 6
    for i in range(rows):
        u = i / (rows - 1)
        row = []
        for j in range(cols):
            v = j / (cols - 1)
            sag = 0.22 * math.sin(u * math.pi) * (0.55 + 0.45 * math.sin(v * math.pi))
            ripple = 0.018 * math.sin(v * 21.0 + u * 5.0)
            row.append(Vector(((v - 0.5) * w, depth * u, 3.2 - 0.5 * u - sag + ripple)))
        grid.append(row)
    p.sheet(grid, colour, "tarp", lambda c: 0.82 + 0.18 * abs(math.sin(c.x * 3.0 + c.y * 1.3)))
    return p


def free_tarp(name, colour):
    """A sheet on four bamboo poles over a pavement spread, 2.4 m and up."""
    p = Piece(name)
    w, d = 2.7, 2.1
    for x in (-w / 2, w / 2):
        for y in (-d / 2, d / 2):
            p.rod((x, y, 0.0), (x, y, 2.62 if y < 0 else 2.42), 0.02, BAMBOO, 4)
    grid = []
    for i in range(6):
        u = i / 5
        row = []
        for j in range(6):
            v = j / 5
            sag = 0.12 * math.sin(u * math.pi) * math.sin(v * math.pi) + 0.01 * math.sin(v * 17 + u * 6)
            row.append(Vector(((v - 0.5) * (w + 0.3), (u - 0.5) * (d + 0.3), 2.66 - 0.22 * u - sag)))
        grid.append(row)
    p.sheet(grid, colour, "tarp", lambda c: 0.84 + 0.16 * abs(math.sin(c.x * 2.3 + c.y * 3.1)))
    return p


def rug(p, w, d, tile, colour):
    grid = [[Vector(((j / 3 - 0.5) * w, (i / 3 - 0.5) * d, 0.012 + 0.006 * math.sin(i * 3.1 + j * 2.3))) for j in range(4)] for i in range(4)]
    p.sheet(grid, colour, tile)


def stool(p):
    """Where the seller sits, behind the spread."""
    crate_box(p, 0.0, -0.98, 0.0, 0.42, 0.34, 0.36, WOOD_DK)
    p.box((-0.21, -1.15, 0.36), (0.21, -0.81, 0.38), WOOD_DK, "wood")


def patri(name, sheet, heaps):
    """A pavement spread: a sheet on the ground and low heaps on it. Nothing over a shin."""
    p = Piece(name)
    rug(p, 2.2, 1.3, *sheet)
    stool(p)
    n = len(heaps)
    for k, tile in enumerate(heaps):
        x = (k - (n - 1) / 2) * 2.0 / n
        p.heap(x, rng.uniform(-0.06, 0.06), 0.01, 1.0 / n + 0.04, 0.52, 0.17, tile)
    return p


def patri_clothes(name):
    """Folded clothes in low stacks."""
    p = Piece(name)
    rug(p, 2.2, 1.3, "tarp", (0.2, 0.26, 0.42))
    stool(p)
    for i in range(3):
        for j in range(5):
            x, y = -0.8 + j * 0.4 + rng.uniform(-0.02, 0.02), -0.4 + i * 0.4 + rng.uniform(-0.02, 0.02)
            tile, c = some_cloth()
            h = rng.uniform(0.05, 0.16)
            p.box((x - 0.17, y - 0.15, 0.015), (x + 0.17, y + 0.15, 0.015 + h), c, tile, lambda q, b=h: 0.75 + 0.25 * min(1.0, (q.z / b)))
    return p


def patri_shoes(name):
    """Chappals and shoes in rows on a sheet."""
    p = Piece(name)
    rug(p, 2.2, 1.3, "tarp", (0.56, 0.56, 0.52))
    stool(p)
    colours = [(0.14, 0.12, 0.11), (0.34, 0.20, 0.12), (0.52, 0.34, 0.20), (0.62, 0.20, 0.18), (0.20, 0.28, 0.42), (0.74, 0.70, 0.64)]
    for i in range(4):
        for j in range(8):
            x, y = -0.9 + j * 0.26, -0.45 + i * 0.3
            c = vary(rng.choice(colours), 0.06)
            for s in (-0.05, 0.05):
                p.box((x + s - 0.04, y - 0.12, 0.012), (x + s + 0.04, y + 0.12, 0.012 + rng.uniform(0.03, 0.08)), c, "plastic")
    return p


def patri_steel(name):
    """Plates, bowls and tumblers on a sheet."""
    p = Piece(name)
    rug(p, 2.2, 1.3, "tarp", (0.3, 0.34, 0.3))
    stool(p)
    for i in range(4):
        for j in range(7):
            x, y = -0.9 + j * 0.3 + rng.uniform(-0.02, 0.02), -0.45 + i * 0.3 + rng.uniform(-0.02, 0.02)
            r = rng.uniform(0.08, 0.13)
            p.lathe([(r * 0.5, 0.0), (r, 0.02), (r * 1.1, 0.05)], STEEL, 7, (x, y, 0.014), "steel", closed=True)
    return p


def crow():
    """A crow on a wire: dark, head up, tail down."""
    p = Piece("crow")
    black = (0.05, 0.05, 0.06)
    p.lathe([(0.0, 0.0), (0.05, 0.03), (0.07, 0.1), (0.06, 0.16), (0.0, 0.2)], black, 6, (0, 0, 0.03), scale=(0.8, 1.2, 1.0), closed=False)
    p.lathe([(0.0, 0.0), (0.035, 0.03), (0.035, 0.06), (0.0, 0.08)], (0.1, 0.1, 0.11), 6, (0, 0.1, 0.14), closed=False)
    p.rod((0, 0.135, 0.18), (0, 0.19, 0.17), 0.014, black, 4, 0.002)
    p.rod((0, -0.08, 0.1), (0, -0.2, 0.05), 0.03, black, 4, 0.008)
    return p


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    made = [
        thela_top("heap_orange_apple", ["orange", "apple", "orange"]),
        thela_top("heap_banana_guava", ["banana", "guava", "banana"]),
        thela_top("heap_tomato_onion", ["tomato", "onion", "potato"]),
        thela_top("heap_veg", ["brinjal", "greens", "tomato"]),
        thela_top("heap_cauli", ["cauli", "greens", "cauli"]),
        thela_canopy("canopy_blue", (0.17, 0.32, 0.54)),
        thela_canopy("canopy_orange", (0.76, 0.38, 0.14)),
        thela_canopy("canopy_white", (0.80, 0.80, 0.75)),
        crate_of("crate_tomato", "tomato"),
        crate_of("crate_onion", "onion"),
        crate_of("crate_cauli", "cauli"),
        crate_of("crate_coconut", "coconut"),
        crate_stand("stand_fruit", ["orange", "apple", "guava"]),
        crate_stand("stand_veg", ["potato", "onion", "tomato"]),
        patri("greens", ("jute", WHITE), ["greens"]),
        sacks_open("sacks_spice", ["chilli", "turmeric", "coriander", "chilli"]),
        sacks_open("sacks_grain", ["rice", "lentil", "rice", "coriander"]),
        sacks_stack("sacks_stack"),
        rack_cloth("rack_cloth", 16),
        rack_cloth("rack_short", 12, (0.7, 0.95)),
        rack_high("rack_high", 14),
        bolts("bolts"),
        pots("pots"),
        buckets("buckets"),
        cartons("cartons"),
        tarp("tarp_blue", 4.4, 3.0, (0.15, 0.27, 0.46)),
        tarp("tarp_orange", 4.4, 3.0, (0.68, 0.32, 0.12)),
        tarp("tarp_grey", 5.2, 3.4, (0.58, 0.58, 0.54)),
        tarp("tarp_green", 3.6, 2.6, (0.20, 0.34, 0.24)),
        free_tarp("sheet_blue", (0.16, 0.30, 0.50)),
        free_tarp("sheet_orange", (0.72, 0.34, 0.13)),
        free_tarp("sheet_white", (0.74, 0.74, 0.70)),
        patri("patri_veg", ("jute", WHITE), ["tomato", "onion", "potato", "greens"]),
        patri("patri_fruit", ("jute", WHITE), ["orange", "banana", "apple"]),
        patri("patri_roots", ("tarp", (0.2, 0.3, 0.46)), ["potato", "onion", "cauli"]),
        patri_clothes("patri_clothes"),
        patri_shoes("patri_shoes"),
        patri_steel("patri_steel"),
        crow(),
    ]
    built = [p.finish() for p in made]
    bpy.ops.object.select_all(action="DESELECT")
    for o in built:
        o.select_set(True)
    bpy.context.view_layer.objects.active = built[0]
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    bpy.ops.export_scene.gltf(filepath=OUT, export_format="GLB", use_selection=True, export_yup=True, export_apply=True, export_normals=True,
                              export_texcoords=True, export_materials="NONE", export_vertex_color="ACTIVE", export_all_vertex_colors=False)
    tris = {o.name: sum(len(p.vertices) - 2 for p in o.data.polygons) for o in built}
    print(f"[goods] {OUT}: {sum(tris.values())} tris, {tris}")


main()

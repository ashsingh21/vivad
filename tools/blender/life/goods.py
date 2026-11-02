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

    def heap(self, cx, cy, z0, rx, ry, h, tile, rgb=WHITE, rough=0.05, rings=3, seg=9):
        """A dome with its tile laid on from above: a heap of hundreds of things for fifty faces.
        Its edge is lumpy, as a heap's is."""
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

    def sheet(self, grid, rgb=WHITE, tile="white", shade=None, tile_span=(1, 1), repeat=False):
        """Cloth: a grid of points (rows of Vectors); the game's material draws both faces.
        `repeat` lays the whole tile on every other cell, mirrored, for a big sheet."""
        rows = [[self.bm.verts.new(p) for p in row] for row in grid]
        where = {}
        for i, row in enumerate(rows):
            for j, v in enumerate(row):
                where[v] = (j % 2, i % 2) if repeat else (j / (len(row) - 1) * tile_span[0], i / (len(rows) - 1) * tile_span[1])
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
        p.lathe(bulge + [(r * 0.5, h * 1.02), (r * 0.16, h * 1.1), (r * 0.25, h * 1.2)], rgb, 6, (x, y, 0), "jute", dark)
    else:
        p.lathe(bulge[1:] + [(r * 1.08, h * 0.94), (r * 0.92, h * 0.98)], rgb, 6, (x, y, 0), "jute", dark, closed=False)
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
            p.lathe([(0.0, 0.0), (0.2, 0.0), (0.31, 0.14), (0.2, 0.27), (0.0, 0.28)], c, 6, (x, y, cz), "jute", lambda q, b=cz: 0.7 + 0.3 * min(1.0, (q.z - b) / 0.26), closed=False, scale=(1.0, 0.7, 1.0))
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
    for i in range(3):
        for j in range(6):
            x, y = -0.85 + j * 0.34, -0.4 + i * 0.4
            c = vary(rng.choice(colours), 0.06)
            for s in (-0.05, 0.05):
                p.box((x + s - 0.04, y - 0.12, 0.012), (x + s + 0.04, y + 0.12, 0.012 + rng.uniform(0.03, 0.08)), c, "plastic")
    return p


def patri_steel(name):
    """Plates, bowls and tumblers on a sheet."""
    p = Piece(name)
    rug(p, 2.2, 1.3, "tarp", (0.3, 0.34, 0.3))
    stool(p)
    for i in range(3):
        for j in range(6):
            x, y = -0.85 + j * 0.34 + rng.uniform(-0.02, 0.02), -0.4 + i * 0.4 + rng.uniform(-0.02, 0.02)
            r = rng.uniform(0.09, 0.14)
            p.lathe([(r * 0.5, 0.0), (r * 1.1, 0.05)], STEEL, 6, (x, y, 0.014), "steel", closed=True)
    return p


# ------------------------------------------------------------------ stall rows

ROW_W = 2.7
TOP = 0.78


def table(p, skirt, colour=WHITE, w=2.4, d=1.0):
    """A plank table with a cloth skirt down its front, so what's under it stays dark."""
    p.box((-w / 2, -d / 2, TOP - 0.04), (w / 2, d / 2, TOP), WOOD_DK, "wood")
    for x in (-w / 2 + 0.07, w / 2 - 0.07):
        for y in (-d / 2 + 0.07, d / 2 - 0.07):
            p.box((x - 0.03, y - 0.03, 0.0), (x + 0.03, y + 0.03, TOP - 0.04), WOOD_DK, "wood")
    grid = [[Vector(((j / 5 - 0.5) * w, d / 2 + 0.01 + 0.012 * math.sin(j * 2.4), TOP - i / 2 * (TOP - 0.06))) for j in range(6)] for i in range(3)]
    p.sheet(grid, colour, skirt, lambda c: 0.65 + 0.35 * min(1.0, c.z / TOP))


def back_sheet(p, tile, colour, top=2.76, low=2.02, w=2.7):
    """A sheet hung from the back of a stall's cover. It stops above a warrior's eyes: nothing in
    a row may be a wall, since a shot goes straight through it and a player has to see that."""
    grid = [[Vector(((j / 8 - 0.5) * w, -0.72 + 0.02 * math.sin(j * 2.1 + i), top - i / 2 * (top - low))) for j in range(9)] for i in range(3)]
    p.sheet(grid, colour, tile, lambda c: 0.72 + 0.28 * abs(math.sin(c.x * 4.1)), repeat=True)


def bags_row(p, z, n, w, y, colours, size=(0.2, 0.05, 0.24)):
    for k in range(n):
        x = (k + 0.5) / n * w - w / 2 + rng.uniform(-0.015, 0.015)
        c = vary(rng.choice(colours), 0.07)
        dz = rng.uniform(-0.03, 0.03)
        p.box((x - size[0] / 2, y - size[1] / 2, z - size[2] + dz), (x + size[0] / 2, y + size[1] / 2, z + dz), c, "plastic")


def stall_fruit(name, tiles, hang):
    p = Piece(name)
    table(p, "jute")
    back_sheet(p, "jute", (0.9, 0.86, 0.76))
    for k, tile in enumerate(tiles):
        x = (k - (len(tiles) - 1) / 2) * 2.3 / len(tiles)
        p.heap(x, 0.0, TOP, 1.15 / len(tiles) + 0.05, 0.5, 0.26, tile)
    for k in range(hang):
        x = (k + 0.5) / hang * 2.3 - 1.15
        p.lathe([(0.0, -0.42), (0.1, -0.34), (0.13, -0.16), (0.09, 0.0)], (1.0, 1.0, 0.92), 6, (x, 0.42, 2.2), "banana", closed=False)
    return p


def stall_cloth(name):
    p = Piece(name)
    table(p, "linen", (0.5, 0.2, 0.26))
    back_sheet(p, "linen", (0.46, 0.42, 0.4))
    p.rod((-1.25, 0.56, 2.32), (1.25, 0.56, 2.32), 0.012, STEEL, 4)
    for k in range(7):
        x = (k + 0.5) / 7 * 2.5 - 1.25
        tile, c = some_cloth()
        garment(p, x, 0.56, 2.3, 0.3, rng.uniform(0.32, 0.62), tile, c)
    for i in range(2):
        for j in range(6):
            x, y = -1.0 + j * 0.4, -0.2 + i * 0.42
            tile, c = some_cloth()
            h = rng.uniform(0.08, 0.24)
            p.box((x - 0.18, y - 0.17, TOP), (x + 0.18, y + 0.17, TOP + h), c, tile)
    return p


def stall_bags(name):
    p = Piece(name)
    table(p, "denim")
    back_sheet(p, "tarp", (0.28, 0.28, 0.3))
    colours = [(0.34, 0.20, 0.12), (0.16, 0.14, 0.14), (0.60, 0.22, 0.20), (0.22, 0.30, 0.44), (0.70, 0.55, 0.30), (0.40, 0.42, 0.26)]
    bags_row(p, 2.3, 6, 2.4, 0.56, colours, (0.3, 0.09, 0.3))
    for k in range(6):
        x = -1.0 + k * 0.4
        c = vary(rng.choice(colours), 0.06)
        p.box((x - 0.16, -0.14, TOP), (x + 0.16, 0.08, TOP + rng.uniform(0.22, 0.32)), c, "plastic")
    return p


def stall_shoes(name):
    """Three steps of shoes, the way a footpath seller stands them."""
    p = Piece(name)
    back_sheet(p, "tarp", (0.2, 0.26, 0.4))
    colours = [(0.13, 0.11, 0.10), (0.34, 0.20, 0.12), (0.52, 0.34, 0.20), (0.62, 0.20, 0.18), (0.20, 0.28, 0.42), (0.74, 0.70, 0.64)]
    for k in range(3):
        y0, y1, z = 0.5 - k * 0.36, 0.14 - k * 0.36, 0.28 + k * 0.28
        p.box((-1.2, y1, 0.0), (1.2, y0, z), (0.5, 0.48, 0.46), "wood")
        for j in range(7):
            x = -1.05 + j * 0.35
            c = vary(rng.choice(colours), 0.05)
            for s in (-0.06, 0.06):
                p.box((x + s - 0.05, y1 + 0.05, z), (x + s + 0.05, y0 - 0.03, z + rng.uniform(0.07, 0.11)), c, "plastic")
    return p


def stall_steel(name):
    p = Piece(name)
    table(p, "tarp", (0.36, 0.38, 0.42))
    back_sheet(p, "tarp", (0.34, 0.36, 0.38))
    for k in range(6):
        x = -1.0 + k * 0.4
        z = TOP
        for level in range(rng.randint(2, 3)):
            r = rng.uniform(0.12, 0.16) - level * 0.015
            p.lathe([(r * 0.8, 0.0), (r, 0.03), (r * 1.06, 0.12)], STEEL, 6, (x, rng.uniform(-0.2, 0.2), z), "steel")
            z += 0.1
    p.rod((-1.2, 0.56, 2.28), (1.2, 0.56, 2.28), 0.012, STEEL, 4)
    for k in range(6):
        x = -1.05 + k * 0.42
        r = rng.uniform(0.1, 0.14)
        grid = [[Vector((x + (r * a) * math.cos(t), 0.58, 2.1 + (r * a) * math.sin(t))) for t in [q / 6 * math.tau for q in range(7)]] for a in (0.05, 1.0)]
        p.sheet(grid, STEEL, "steel")
    return p


def stall_spice(name, tops):
    """Open sacks stood along the front, a low table of tins behind."""
    p = Piece(name)
    back_sheet(p, "jute", (0.84, 0.78, 0.66))
    n = len(tops)
    for k, top in enumerate(tops):
        x = (k - (n - 1) / 2) * 2.4 / n
        sack(p, x, 0.26, 0.21, 0.56, vary(WHITE, 0.06), top)
    p.box((-1.2, -0.44, 0.0), (1.2, -0.04, 0.72), WOOD_DK, "wood")
    colours = [(0.70, 0.24, 0.12), (0.72, 0.56, 0.16), (0.36, 0.40, 0.20), (0.56, 0.40, 0.26)]
    for k in range(6):
        x = -1.05 + k * 0.42
        p.lathe([(0.1, 0.0), (0.1, 0.24), (0.075, 0.28)], vary(rng.choice(colours), 0.07), 5, (x, -0.24, 0.72), "plastic")
    return p


def stall_toys(name):
    p = Piece(name)
    table(p, "tarp", (0.72, 0.36, 0.14))
    back_sheet(p, "tarp", (0.2, 0.34, 0.44))
    colours = [(0.86, 0.22, 0.16), (0.96, 0.74, 0.14), (0.16, 0.50, 0.74), (0.26, 0.66, 0.34), (0.90, 0.44, 0.60)]
    bags_row(p, 2.3, 7, 2.4, 0.56, colours, (0.24, 0.06, 0.26))
    for k in range(12):
        x, y = rng.uniform(-1.1, 1.1), rng.uniform(-0.38, 0.38)
        r = rng.uniform(0.07, 0.13)
        p.lathe([(0.0, 0.0), (r, r * 0.6), (r, r * 1.4), (0.0, r * 2)], vary(rng.choice(colours), 0.06), 5, (x, y, TOP), "plastic", closed=False)
    return p


def stall_snack(name):
    """A fry stall: a black pan on a stove, trays of something golden, steel tins."""
    p = Piece(name)
    table(p, "tarp", (0.74, 0.70, 0.60))
    back_sheet(p, "tarp", (0.3, 0.26, 0.24))
    p.lathe([(0.0, 0.0), (0.42, 0.04), (0.5, 0.14)], (0.07, 0.07, 0.08), 12, (-0.72, 0.0, TOP + 0.14), "steel", closed=False)
    p.lathe([(0.0, 0.0), (0.46, 0.0)], (0.70, 0.46, 0.12), 12, (-0.72, 0.0, TOP + 0.24), closed=False)
    p.box((-1.04, -0.32, TOP), (-0.4, 0.32, TOP + 0.14), (0.14, 0.14, 0.15), "steel")
    for k, tile in enumerate(("turmeric", "coriander", "chilli")):
        x = 0.1 + k * 0.42
        p.box((x - 0.2, -0.3, TOP), (x + 0.2, 0.3, TOP + 0.03), STEEL, "steel")
        p.heap(x, 0.0, TOP + 0.03, 0.19, 0.28, 0.12, tile)
    return p


def roof(name, colour):
    """The row's cover: a sheet from 2.8 m behind to 2.4 m at the front on four poles, with a
    scalloped edge hanging. A little wider than a stall, so a row's sheets lap over each other."""
    p = Piece(name)
    w = ROW_W + 0.5
    for x in (-ROW_W / 2 + 0.05, ROW_W / 2 - 0.05):
        p.rod((x, -0.72, 0.0), (x, -0.72, 2.8), 0.024, BAMBOO, 3)
        p.rod((x, 0.92, 0.0), (x, 0.92, 2.42), 0.022, BAMBOO, 3)
    grid = []
    for i in range(3):
        u = i / 2
        row = []
        for j in range(5):
            v = j / 4
            sag = 0.09 * math.sin(u * math.pi) * (0.5 + 0.5 * math.sin(v * math.pi * 2)) + 0.012 * math.sin(v * 23 + u * 5)
            row.append(Vector(((v - 0.5) * w, -0.85 + 1.95 * u, 2.82 - 0.44 * u - sag)))
        grid.append(row)
    p.sheet(grid, colour, "tarp", lambda c: 0.8 + 0.2 * abs(math.sin(c.x * 2.6 + c.y * 1.9)))
    edge = []
    for i in range(2):
        row = []
        for j in range(9):
            v = j / 8
            drop = 0.2 + 0.1 * (j % 2) if i else 0.0
            row.append(Vector(((v - 0.5) * w, 1.1, 2.38 - drop)))
        edge.append(row)
    p.sheet(edge, colour, "tarp", lambda c: 0.7 + 0.3 * abs(math.sin(c.x * 6.0)))
    return p


def awning(name, a, b):
    """A striped cloth awning off a shop front: up at the wall, out over the pavement."""
    p = Piece(name)
    w, out = 3.1, 1.9
    n = 8
    for k in range(n):
        x0, x1 = -w / 2 + k * w / n, -w / 2 + (k + 1) * w / n
        c = a if k % 2 == 0 else b
        grid = [[Vector((x, out * u, 2.95 - 0.6 * u - 0.05 * math.sin(u * math.pi))) for x in (x0, x1)] for u in (0.0, 0.34, 0.67, 1.0)]
        grid.append([Vector((x, out + 0.01, 2.35 - 0.24)) for x in (x0, x1)])
        p.sheet(grid, c, "linen", lambda q: 0.82 + 0.18 * abs(math.sin(q.x * 2.0 + q.y * 3.0)))
    return p


def shop_front(name, kind):
    """A shop's stock brought out to its door: shelves against the wall, full to the top, with
    things hung along the front."""
    p = Piece(name)
    w, d = 2.9, 0.42
    for x in (-w / 2, w / 2):
        p.box((x - 0.025, 0.0, 0.0), (x + 0.025, d, 2.3), WOOD_DK, "wood")
    p.box((-w / 2, 0.0, 0.0), (w / 2, 0.02, 2.3), (0.3, 0.27, 0.25), "wood")
    for z in (0.05, 0.62, 1.2, 1.78):
        p.box((-w / 2, 0.0, z - 0.025), (w / 2, d, z), WOOD_DK, "wood")
        x = -w / 2 + 0.06
        while x < w / 2 - 0.3:
            bw = rng.uniform(0.34, 0.72)
            h = rng.uniform(0.3, 0.5)
            if kind == "cloth":
                tile, c = some_cloth()
            elif kind == "bartan":
                tile, c = "steel", STEEL
            elif kind == "grocery":
                tile, c = rng.choice([("card", WHITE), ("jute", WHITE), ("plastic", (0.7, 0.3, 0.2)), ("plastic", (0.8, 0.66, 0.2)), ("plastic", (0.26, 0.5, 0.3))])
            else:
                tile, c = rng.choice([("card", WHITE), ("plastic", (0.3, 0.4, 0.6)), ("plastic", (0.7, 0.26, 0.24)), ("plastic", (0.8, 0.7, 0.3)), ("card", WHITE)])
            p.box((x, 0.04, z), (x + bw, d - rng.uniform(0.02, 0.1), z + h), vary(c, 0.06), tile)
            x += bw + 0.03
    if kind in ("cloth", "general"):
        for k in range(3):
            xx = (k + 0.5) / 3 * w - w / 2
            tile, c = some_cloth()
            garment(p, xx, d + 0.05, 2.34, 0.8, rng.uniform(0.5, 0.8), tile, c)
    return p


def far_stall(name, tiles, skirt):
    """A stall at forty metres: a table and a low heap, the same size and colours."""
    p = Piece(name)
    p.box((-1.2, -0.5, 0.0), (1.2, 0.5, TOP), WOOD_DK, skirt)
    for k, tile in enumerate(tiles):
        x = (k - (len(tiles) - 1) / 2) * 2.3 / len(tiles)
        p.heap(x, 0.0, TOP, 1.15 / len(tiles) + 0.05, 0.5, 0.26, tile, rings=2, seg=6)
    return p


def far_shop(name, tile, colour):
    """A stocked shop front at forty metres: shelves are a colour."""
    p = Piece(name)
    p.box((-1.45, 0.0, 0.0), (1.45, 0.42, 2.3), colour, tile)
    return p


def far_patri(name, tiles):
    p = Piece(name)
    rug(p, 2.2, 1.3, "tarp", (0.3, 0.3, 0.32))
    for k, tile in enumerate(tiles):
        x = (k - (len(tiles) - 1) / 2) * 2.0 / len(tiles)
        p.heap(x, 0.0, 0.01, 1.0 / len(tiles) + 0.04, 0.52, 0.17, tile, rings=2, seg=6)
    return p


def far_scooter():
    """A scooter at forty metres, or a bunch of them."""
    p = Piece("far_scooter")
    c = (0.34, 0.22, 0.20)
    p.box((-0.12, -0.75, 0.2), (0.12, 0.75, 0.62), c)
    p.box((-0.2, -0.3, 0.62), (0.2, 0.55, 0.84), (0.12, 0.11, 0.11))
    p.box((-0.3, 0.68, 0.8), (0.3, 0.74, 0.86), (0.1, 0.1, 0.1))
    p.lathe([(0.0, -0.05), (0.2, -0.05), (0.2, 0.05), (0.0, 0.05)], (0.05, 0.05, 0.05), 6, (0, 0.65, 0.2), closed=False, scale=(1, 1, 1))
    return p


def bunting_pole():
    """A tall bamboo that holds a banner's rope over the road."""
    p = Piece("bamboo_tall")
    p.rod((0, 0, 0), (0, 0, 6.3), 0.03, BAMBOO, 5, 0.018)
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
        stall_fruit("stall_fruit", ["orange", "apple", "guava"], 4),
        stall_fruit("stall_tropic", ["banana", "orange", "coconut"], 5),
        stall_fruit("stall_veg", ["potato", "onion", "tomato"], 0),
        stall_fruit("stall_green", ["cauli", "greens", "brinjal"], 0),
        stall_cloth("stall_cloth"),
        stall_cloth("stall_cloth2"),
        stall_bags("stall_bags"),
        stall_shoes("stall_shoes"),
        stall_steel("stall_steel"),
        stall_spice("stall_spice", ["chilli", "turmeric", "coriander", "chilli", "lentil"]),
        stall_spice("stall_grain", ["rice", "lentil", "rice", "coriander", "rice"]),
        stall_toys("stall_toys"),
        stall_snack("stall_snack"),
        roof("roof_blue", (0.15, 0.27, 0.46)),
        roof("roof_orange", (0.70, 0.33, 0.12)),
        roof("roof_white", (0.78, 0.78, 0.73)),
        roof("roof_green", (0.20, 0.36, 0.26)),
        roof("roof_red", (0.56, 0.14, 0.13)),
        awning("awning_red", (0.62, 0.15, 0.14), (0.86, 0.82, 0.74)),
        awning("awning_blue", (0.15, 0.28, 0.48), (0.86, 0.82, 0.74)),
        awning("awning_green", (0.17, 0.38, 0.27), (0.84, 0.78, 0.56)),
        awning("awning_orange", (0.78, 0.42, 0.12), (0.84, 0.78, 0.60)),
        shop_front("shop_cloth", "cloth"),
        shop_front("shop_bartan", "bartan"),
        shop_front("shop_grocery", "grocery"),
        shop_front("shop_general", "general"),
        far_stall("far_fruit", ["orange", "apple", "guava"], "jute"),
        far_stall("far_veg", ["potato", "onion", "greens"], "jute"),
        far_stall("far_cloth", ["gingham", "linen", "floral"], "linen"),
        far_stall("far_mixed", ["steel", "card", "turmeric"], "tarp"),
        far_shop("far_shop_a", "card", (0.9, 0.86, 0.8)),
        far_shop("far_shop_b", "floral", (0.9, 0.86, 0.9)),
        far_shop("far_shop_c", "steel", (0.84, 0.86, 0.9)),
        far_patri("far_patri_a", ["tomato", "onion", "potato"]),
        far_patri("far_patri_b", ["linen", "gingham", "floral"]),
        far_scooter(),
        bunting_pole(),
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

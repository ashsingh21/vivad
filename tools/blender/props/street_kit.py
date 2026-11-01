"""The street kit: the things that make a Delhi lane a Delhi lane, for the city maps' streets and
rooftops. A parked auto, a scooter, a fruit thela, a chai stall, an electricity pole with its
transformer, a blue water drum, a stack of crates, plastic chairs and a tulsi pot on the street;
a desert cooler, a dish, a solar panel, a washing line and a stair room (mumty) on the roofs.

One file, every piece a mesh named after it (`auto_LOD0`, `auto_LOD1`, ...), coloured per vertex
(COLOR_0) with no textures or materials of its own: the game merges every placed copy into one mesh
per chunk with one shared material (crate::street), so a whole street is a few draws
(docs/rendering-guidelines.md section 3). LOD1 is LOD0 decimated to ~40%.

Each piece stands on its origin (z = 0), facing Blender +Y (game -Z). Sizes are real, in metres; the
footprints the map uses for colliders are in shared::map::kit.

blender -b --factory-startup -P tools/blender/props/street_kit.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import bake  # noqa: E402
import export  # noqa: E402
import numpy as np  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "models", "props", "street_kit.glb")

# Flat colours (sRGB), shared across the kit.
C = {
    "auto_green": (0.10, 0.52, 0.22), "auto_yellow": (0.99, 0.80, 0.08), "black": (0.07, 0.07, 0.08),
    "tyre": (0.05, 0.05, 0.05), "chrome": (0.78, 0.78, 0.80), "glass": (0.35, 0.50, 0.55),
    "lamp": (1.0, 0.95, 0.80), "red": (0.84, 0.12, 0.08), "blue": (0.10, 0.35, 0.80), "white": (0.95, 0.94, 0.90),
    "wood": (0.55, 0.36, 0.20), "wood_lt": (0.74, 0.55, 0.33), "orange": (0.98, 0.55, 0.08), "banana": (0.98, 0.85, 0.20),
    "apple": (0.80, 0.10, 0.12), "leaf": (0.20, 0.55, 0.20), "leaf_dk": (0.12, 0.40, 0.15), "steel": (0.55, 0.56, 0.58),
    "tin": (0.62, 0.64, 0.66), "tarp": (0.12, 0.35, 0.75), "concrete": (0.70, 0.68, 0.64), "grey": (0.45, 0.46, 0.48),
    "drum": (0.10, 0.38, 0.78), "crate_r": (0.82, 0.16, 0.12), "crate_g": (0.15, 0.55, 0.25), "crate_b": (0.10, 0.36, 0.72),
    "terracotta": (0.74, 0.36, 0.22), "beige": (0.86, 0.80, 0.66), "panel": (0.10, 0.16, 0.34), "pink": (0.92, 0.25, 0.50),
    "saffron": (0.98, 0.60, 0.12), "cream": (0.95, 0.90, 0.80), "door_green": (0.16, 0.45, 0.30), "rope": (0.80, 0.78, 0.72),
    "kettle": (0.72, 0.70, 0.66), "chai": (0.75, 0.50, 0.30), "scooter": (0.86, 0.18, 0.16),
}


def lin(rgb):
    return tuple(c ** 2.2 for c in rgb) + (1.0,)


class Piece:
    """One kit piece being built: a bmesh, every new face painted the current colour."""

    def __init__(self, name):
        self.name = name
        self.bm = bmesh.new()
        self.col = self.bm.loops.layers.float_color.new("Col")
        # Only the shells' edges are rounded (boxes, hulls); rods, wheels and fruit stay as they are.
        self.bw = self.bm.edges.layers.float.new("bevel_weight_edge")

    def paint(self, colour, round=False):
        c = lin(C[colour])
        for f in self.bm.faces:
            if not f.tag:
                for loop in f.loops:
                    loop[self.col] = c
                    if round:
                        loop.edge[self.bw] = 1.0
                f.tag = True

    def box(self, mn, mx, colour, round=True):
        bmesh.ops.create_cube(self.bm, size=1.0, matrix=Matrix.Translation(((mn[0] + mx[0]) / 2, (mn[1] + mx[1]) / 2, (mn[2] + mx[2]) / 2))
                              @ Matrix.Diagonal((mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2], 1.0)))
        self.paint(colour, round=round)

    def hull(self, pts, colour):
        new = [self.bm.verts.new(p) for p in pts]
        res = bmesh.ops.convex_hull(self.bm, input=new)
        # Interior points the hull didn't use are left loose: drop them.
        loose = [v for v in res.get("geom_interior", []) if isinstance(v, bmesh.types.BMVert)]
        bmesh.ops.delete(self.bm, geom=loose, context="VERTS")
        # Merge the hull's coplanar triangles, so only its real edges get rounded.
        fresh = [f for f in self.bm.faces if not f.tag]
        bmesh.ops.dissolve_limit(self.bm, angle_limit=math.radians(2), verts=list({v for f in fresh for v in f.verts}),
                                 edges=list({e for f in fresh for e in f.edges}))
        self.paint(colour, round=True)

    def cyl(self, a, b, r, colour, seg=12, r2=None):
        a, b = Vector(a), Vector(b)
        d = b - a
        rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
        bmesh.ops.create_cone(self.bm, cap_ends=True, segments=seg, radius1=r, radius2=r if r2 is None else r2, depth=d.length,
                              matrix=Matrix.Translation(a + d / 2) @ rot)
        self.paint(colour)

    def ball(self, c, r, colour, sub=1, squash=1.0):
        bmesh.ops.create_icosphere(self.bm, subdivisions=sub, radius=r, matrix=Matrix.Translation(Vector(c)) @ Matrix.Diagonal((1, 1, squash, 1)))
        self.paint(colour)

    def quad(self, pts, colour):
        """A flat piece (cloth), both faces."""
        vs = [self.bm.verts.new(p) for p in pts]
        self.bm.faces.new(vs)
        vs2 = [self.bm.verts.new(p) for p in reversed(pts)]
        self.bm.faces.new(vs2)
        self.paint(colour)

    def wheel(self, centre, r, w, axis="x"):
        x, y, z = centre
        if axis == "x":
            a, b = (x - w / 2, y, z), (x + w / 2, y, z)
        else:
            a, b = (x, y - w / 2, z), (x, y + w / 2, z)
        self.cyl(a, b, r, "tyre", 20)
        ha = tuple(p + (q - p) * -0.08 for p, q in zip(a, b))
        hb = tuple(p + (q - p) * 1.08 for p, q in zip(a, b))
        self.cyl(ha, hb, r * 0.42, "chrome", 10)

    def finish(self, bevel=0.03):
        me = bpy.data.meshes.new(f"{self.name}_LOD0")
        self.bm.normal_update()
        self.bm.to_mesh(me)
        self.bm.free()
        # The exporter writes the active colour layer as COLOR_0.
        me.color_attributes.active_color = me.color_attributes["Col"]
        me.color_attributes.render_color_index = me.color_attributes.active_color_index
        o = bpy.data.objects.new(f"{self.name}_LOD0", me)
        bpy.context.scene.collection.objects.link(o)
        if bevel:
            m = o.modifiers.new("bevel", "BEVEL")
            m.width = bevel
            # Rounded, not chamfered: three segments, clamped so thin parts keep their shape.
            m.segments = 3
            m.use_clamp_overlap = True
            m.limit_method = "WEIGHT"
            bpy.context.view_layer.objects.active = o
            bpy.ops.object.modifier_apply(modifier=m.name)
        for p in me.polygons:
            p.use_smooth = True
        me.set_sharp_from_angle(angle=math.radians(40))
        export.triangulate(o)
        bake_ao(o)
        lod1 = o.copy()
        lod1.data = o.data.copy()
        lod1.name = lod1.data.name = f"{self.name}_LOD1"
        bpy.context.scene.collection.objects.link(lod1)
        d = lod1.modifiers.new("decimate", "DECIMATE")
        d.ratio = 0.4
        bpy.context.view_layer.objects.active = lod1
        bpy.ops.object.modifier_apply(modifier=d.name)
        return [o, lod1]


def bake_ao(o, strength=0.7):
    """Ambient occlusion into the piece's vertex colours: dark inside the auto's cab, under a
    stall's counter, where parts meet and where it stands on the road (a ground plane, there only
    for the bake). The other pieces are hidden from it. Costs nothing in the game."""
    others = [x for x in bpy.context.scene.objects if x is not o]
    for x in others:
        x.hide_render = True
    bpy.ops.mesh.primitive_plane_add(size=20, location=(0, 0, 0))
    ground = bpy.context.active_object
    me = o.data
    ao = me.color_attributes.new("AO", "FLOAT_COLOR", "CORNER")
    me.color_attributes.active_color = ao
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.bake(type="AO", target="VERTEX_COLORS")
    k = np.empty(len(me.loops) * 4, np.float32)
    ao.data.foreach_get("color", k)
    col = me.color_attributes["Col"]
    c = np.empty(len(me.loops) * 4, np.float32)
    col.data.foreach_get("color", c)
    occ = 1.0 - strength * (1.0 - k.reshape(-1, 4)[:, :1])
    c = c.reshape(-1, 4)
    c[:, :3] *= occ
    col.data.foreach_set("color", c.ravel())
    me.color_attributes.remove(me.color_attributes["AO"])
    me.color_attributes.active_color = me.color_attributes["Col"]
    me.color_attributes.render_color_index = me.color_attributes.active_color_index
    bpy.data.objects.remove(ground, do_unlink=True)
    for x in others:
        x.hide_render = False


# ------------------------------------------------------------------ street

def auto():
    p = Piece("auto")
    p.hull([(x, y, z) for x in (-0.68, 0.68) for y in (-1.22, 0.25) for z in (0.32, 0.98)], "auto_green")
    p.hull([(x * w, y, z) for (y, w, z0, z1) in ((0.2, 0.62, 0.32, 1.0), (0.9, 0.42, 0.36, 1.02), (1.32, 0.26, 0.5, 0.98)) for x in (-1, 1) for z in (z0, z1)], "auto_green")
    p.box((-0.70, -1.24, 0.60), (0.70, 0.22, 0.68), "auto_yellow")
    roof = [(x, y, z) for x in (-0.70, 0.70) for y in (-1.28, 0.62) for z in (1.62, 1.70)] + [(x, y, 1.80) for x in (-0.5, 0.5) for y in (-1.15, 0.45)]
    p.hull(roof, "auto_yellow")
    p.box((-0.68, -1.24, 0.95), (0.68, -1.16, 1.64), "black")
    for x in (-0.66, 0.66):
        p.cyl((x, 0.55, 0.98), (x, 0.55, 1.64), 0.025, "black", 6)
    p.hull([(x, y, z) for x in (-0.55, 0.55) for (y, z) in ((0.70, 1.02), (0.74, 1.02), (0.60, 1.58), (0.64, 1.58))], "glass")
    p.cyl((0, 1.28, 0.98), (0, 1.41, 0.98), 0.10, "lamp", 10)
    p.box((-0.62, -1.14, 0.45), (0.62, -0.62, 0.72), "black")
    for (x, y) in ((0.0, 1.08), (-0.66, -0.78), (0.66, -0.78)):
        p.wheel((x, y, 0.22), 0.22, 0.12)
    return p.finish(0.06)


def scooter():
    p = Piece("scooter")
    p.hull([(x, y, z) for x in (-0.17, 0.17) for (y, z) in ((-0.75, 0.30), (-0.75, 0.62), (0.15, 0.25), (0.15, 0.45))], "scooter")
    p.hull([(x, y, z) for x in (-0.22, 0.22) for (y, z) in ((0.30, 0.25), (0.62, 0.30), (0.45, 1.00), (0.58, 1.00))], "scooter")
    p.box((-0.16, -0.70, 0.62), (0.16, -0.05, 0.74), "black")
    p.box((-0.12, 0.10, 0.24), (0.12, 0.45, 0.30), "grey")
    p.cyl((0, 0.55, 0.95), (0, 0.56, 1.10), 0.03, "black", 6)
    p.cyl((-0.33, 0.56, 1.10), (0.33, 0.56, 1.10), 0.02, "chrome", 6)
    p.cyl((0, 0.60, 1.05), (0, 0.66, 1.05), 0.06, "lamp", 8)
    p.wheel((0, -0.62, 0.2), 0.2, 0.09)
    p.wheel((0, 0.62, 0.2), 0.2, 0.09)
    return p.finish(0.05)


def thela():
    """A fruit cart: a plank bed on two bicycle wheels and a prop leg, heaped with fruit."""
    p = Piece("thela")
    p.box((-0.5, -1.0, 0.72), (0.5, 1.0, 0.80), "wood_lt")
    for x in (-0.5, 0.5):
        p.box((x - 0.03, -1.0, 0.80), (x + 0.03, 1.0, 0.92), "wood")
    for y in (-1.0, 1.0):
        p.box((-0.5, y - 0.03, 0.80), (0.5, y + 0.03, 0.92), "wood")
    p.cyl((-0.15, 1.0, 0.76), (-0.15, 1.45, 0.76), 0.03, "wood", 6)
    p.cyl((0.15, 1.0, 0.76), (0.15, 1.45, 0.76), 0.03, "wood", 6)
    p.wheel((-0.6, 0.1, 0.36), 0.36, 0.05)
    p.wheel((0.6, 0.1, 0.36), 0.36, 0.05)
    p.cyl((0, -0.8, 0.0), (0, -0.8, 0.72), 0.04, "wood", 6)
    k = 0
    # Heaps of fruit, a kind to each end and the middle.
    for i in range(3):
        for j in range(6):
            colour = ("orange", "apple", "banana")[j // 2]
            x, y = -0.3 + 0.3 * i, -0.75 + 0.3 * j
            h = 0.95 + 0.06 * math.sin(i * 1.3 + j)
            p.ball((x, y, h), 0.13, colour, 1, 0.8)
            k += 1
    return p.finish(0.0)


def chai():
    """A chai stall: a plank counter with a stove, a kettle and glasses, a tin sheet on four poles,
    a blue tarp, a bench in front."""
    p = Piece("chai")
    p.box((-0.8, -0.35, 0.0), (0.8, 0.35, 0.95), "wood")
    p.box((-0.85, -0.40, 0.95), (0.85, 0.40, 1.0), "wood_lt")
    p.box((-0.6, -0.2, 1.0), (-0.2, 0.2, 1.12), "black")
    p.cyl((-0.4, 0.0, 1.12), (-0.4, 0.0, 1.38), 0.12, "kettle", 20, 0.09)
    p.cyl((-0.3, 0.0, 1.30), (-0.16, 0.0, 1.40), 0.025, "kettle", 6, 0.012)
    for k in range(5):
        p.cyl((0.1 + 0.12 * k, -0.15, 1.0), (0.1 + 0.12 * k, -0.15, 1.1), 0.035, "chai", 8)
    for x in (-0.85, 0.85):
        for y in (-0.4, 1.6):
            p.cyl((x, y, 0.0), (x, y, 2.3 if y < 0 else 2.1), 0.03, "wood", 6)
    p.hull([(x, y, z) for x in (-1.0, 1.0) for (y, z) in ((-0.55, 2.32), (1.75, 2.10), (-0.55, 2.36), (1.75, 2.14))], "tin")
    p.quad([(-0.9, 1.6, 2.1), (0.9, 1.6, 2.1), (0.9, 1.6, 1.6), (-0.9, 1.6, 1.6)], "tarp")
    p.box((-0.7, 1.0, 0.40), (0.7, 1.3, 0.46), "wood_lt")
    for x in (-0.6, 0.6):
        p.box((x - 0.04, 1.05, 0.0), (x + 0.04, 1.25, 0.40), "wood")
    return p.finish(0.01)


def pole():
    """An electricity pole: tapered concrete, a crossarm with insulators, a transformer box."""
    p = Piece("pole")
    p.hull([(x * w, y * w, z) for (z, w) in ((0.0, 0.14), (9.0, 0.08)) for x in (-1, 1) for y in (-1, 1)], "concrete")
    p.box((-0.9, -0.05, 8.3), (0.9, 0.05, 8.42), "steel")
    for x in (-0.8, -0.3, 0.3, 0.8):
        p.cyl((x, 0, 8.42), (x, 0, 8.6), 0.04, "white", 8)
    p.box((-0.35, 0.12, 3.2), (0.35, 0.62, 4.2), "grey")
    p.box((-0.38, 0.10, 4.2), (0.38, 0.64, 4.26), "steel")
    for x in (-0.15, 0.15):
        p.cyl((x, 0.37, 4.26), (x, 0.37, 4.5), 0.035, "white", 8)
    return p.finish(0.01)


def drum():
    p = Piece("drum")
    p.cyl((0, 0, 0), (0, 0, 0.92), 0.30, "drum", 24)
    for z in (0.25, 0.62):
        p.cyl((0, 0, z), (0, 0, z + 0.04), 0.315, "drum", 24)
    p.cyl((0, 0, 0.92), (0, 0, 0.95), 0.12, "drum", 12)
    return p.finish(0.03)


def crates():
    p = Piece("crates")
    for k, (colour, x, z) in enumerate((("crate_r", -0.32, 0.0), ("crate_g", 0.32, 0.0), ("crate_b", -0.32, 0.32), ("crate_r", 0.0, 0.64))):
        p.box((x - 0.3, -0.2, z), (x + 0.3, 0.2, z + 0.3), colour)
        p.box((x - 0.31, -0.21, z + 0.12), (x + 0.31, 0.21, z + 0.18), "black", round=False)
    return p.finish(0.01)


def chair(p, x, y, yaw):
    r = Matrix.Rotation(yaw, 3, "Z")
    o = Vector((x, y, 0))
    t = lambda q: tuple(o + r @ Vector(q))  # noqa: E731
    for (lx, ly) in ((-0.2, -0.2), (0.2, -0.2), (-0.2, 0.2), (0.2, 0.2)):
        p.cyl(t((lx, ly, 0)), t((lx * 1.05, ly * 1.05, 0.44)), 0.02, "white", 6)
    p.hull([t((x2, y2, z)) for x2 in (-0.23, 0.23) for y2 in (-0.22, 0.22) for z in (0.42, 0.46)], "white")
    p.hull([t((x2, 0.2 + 0.05 * (z - 0.46) / 0.4, z)) for x2 in (-0.22, 0.22) for z in (0.46, 0.86)] + [t((x2, 0.24, z)) for x2 in (-0.22, 0.22) for z in (0.46, 0.86)], "white")


def chairs():
    p = Piece("chairs")
    chair(p, -0.35, 0.0, 0.3)
    chair(p, 0.4, 0.1, -0.5)
    return p.finish(0.008)


def pot():
    p = Piece("pot")
    p.cyl((0, 0, 0), (0, 0, 0.42), 0.16, "terracotta", 20, 0.23)
    p.cyl((0, 0, 0.40), (0, 0, 0.46), 0.25, "terracotta", 20)
    for k in range(6):
        a = k * 1.05
        p.ball((0.1 * math.cos(a), 0.1 * math.sin(a), 0.62 + 0.06 * (k % 2)), 0.13, "leaf" if k % 2 else "leaf_dk", 1)
    p.ball((0, 0, 0.75), 0.12, "leaf", 1)
    return p.finish(0.0)


# ------------------------------------------------------------------ rooftop

def cooler():
    p = Piece("cooler")
    p.box((-0.45, -0.32, 0.0), (0.45, 0.32, 1.1), "beige")
    for z in range(6):
        p.box((-0.40, 0.32, 0.18 + 0.13 * z), (0.40, 0.34, 0.24 + 0.13 * z), "grey", round=False)
    for x in (-0.46, 0.46):
        p.box((x - 0.01, -0.25, 0.2), (x + 0.01, 0.25, 0.95), "wood_lt", round=False)
    p.cyl((0, 0, 1.1), (0, 0, 1.14), 0.25, "grey", 12)
    return p.finish(0.01)


def dish():
    p = Piece("dish")
    p.cyl((0, 0, 0), (0, 0, 1.2), 0.03, "steel", 6)
    p.box((-0.2, -0.2, 0.0), (0.2, 0.2, 0.05), "steel")
    c = Vector((0, 0.05, 1.35))
    axis = Vector((0, 0.9, 0.45)).normalized()
    p.cyl(tuple(c), tuple(c + axis * 0.1), 0.38, "white", 16, 0.30)
    p.cyl(tuple(c + Vector((0, 0, -0.3))), tuple(c + axis * 0.55), 0.015, "steel", 6)
    p.cyl(tuple(c + axis * 0.55), tuple(c + axis * 0.62), 0.04, "grey", 8)
    return p.finish(0.0)


def solar():
    p = Piece("solar")
    for x in (-0.8, 0.8):
        p.cyl((x, -0.4, 0), (x, -0.4, 0.55), 0.025, "steel", 6)
        p.cyl((x, 0.4, 0), (x, 0.4, 1.05), 0.025, "steel", 6)
    p.hull([(x, y, z) for x in (-0.9, 0.9) for (y, z) in ((-0.5, 0.55), (0.5, 1.07), (-0.5, 0.59), (0.5, 1.11))], "panel")
    for k in range(1, 4):
        x = -0.9 + 0.45 * k
        p.hull([(x + dx, y, z) for dx in (-0.008, 0.008) for (y, z) in ((-0.5, 0.595), (0.5, 1.115))], "chrome")
    return p.finish(0.005)


def clothesline():
    """Two poles, a line, the wash: a kurta, a saree's length, towels."""
    p = Piece("clothesline")
    for x in (-2.0, 2.0):
        p.cyl((x, 0, 0), (x, 0, 2.0), 0.03, "steel", 6)
    p.cyl((-2.0, 0, 1.92), (2.0, 0, 1.92), 0.008, "rope", 4)
    for (x0, x1, h, colour) in ((-1.8, -1.2, 0.9, "white"), (-1.1, -0.2, 1.3, "pink"), (-0.1, 0.4, 0.6, "blue"), (0.5, 1.1, 0.8, "saffron"), (1.2, 1.8, 0.55, "crate_g")):
        p.quad([(x0, 0, 1.9), (x1, 0, 1.9), (x1, 0.02, 1.9 - h), (x0, 0.02, 1.9 - h)], colour)
    return p.finish(0.0)


def mumty():
    """The stair room on a roof: a cream box with a green door and a slab lip."""
    p = Piece("mumty")
    p.box((-1.5, -1.2, 0.0), (1.5, 1.2, 2.6), "cream")
    p.box((-1.62, -1.32, 2.6), (1.62, 1.32, 2.75), "white")
    p.box((-0.45, 1.2, 0.0), (0.45, 1.24, 2.1), "door_green")
    p.box((-0.55, 1.2, 2.1), (0.55, 1.26, 2.2), "white")
    p.box((0.7, -1.24, 1.4), (1.2, -1.2, 1.9), "glass")
    return p.finish(0.02)


def main():
    export.reset_scene()
    bake.setup_cycles(32)
    bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new("World")
    bpy.context.scene.world.light_settings.distance = 0.6
    objs = []
    for build in (auto, scooter, thela, chai, pole, drum, crates, chairs, pot, cooler, dish, solar, clothesline, mumty):
        objs += build()
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    bpy.ops.export_scene.gltf(filepath=OUT, export_format="GLB", use_selection=True, export_yup=True, export_apply=True,
                              export_normals=True, export_texcoords=False, export_materials="NONE", export_vertex_color="ACTIVE",
                              export_all_vertex_colors=False)
    tris = {o.name: sum(len(p.vertices) - 2 for p in o.data.polygons) for o in objs}
    print(f"[street_kit] {OUT}: {tris}")


main()

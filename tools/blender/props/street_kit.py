"""The street kit: the things that make a Delhi lane a Delhi lane, for the city maps' streets and
rooftops. A parked auto, a scooter, a fruit thela, a chai stall, an electricity pole with its
transformer, a blue water drum, a stack of crates, plastic chairs and a tulsi pot on the street;
a desert cooler, a dish, a solar panel, a washing line and a stair room (mumty) on the roofs.

One file, every piece a mesh named after it (`auto_LOD0`, `auto_LOD1`, ...), coloured per vertex
(COLOR_0) with no textures or materials of its own: the game merges every placed copy into one mesh
per chunk with one shared material (crate::street), so a whole street is a few draws
(docs/rendering-guidelines.md section 3). LOD1 is LOD0 decimated to ~40%; the heavy pieces' LOD2 to 10%.

Each piece stands on its origin (z = 0), facing Blender +Y (game -Z). Sizes are real, in metres; the
footprints the map uses for colliders are in shared::map::kit.

blender -b --factory-startup -P tools/blender/props/street_kit.py
"""

import math
import random
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import bake  # noqa: E402
import bikes  # noqa: E402
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


# Surfaces a piece's parts can be marked as (Piece.made_of), for furniture.py's materials.
SURFACES = (None, "glass", "print", "kraft", "metal", "cotton", "wood", "plastic")


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
        # Faces shaded smooth whatever their angles (foliage), not split at sharp edges.
        self.soft = self.bm.faces.layers.int.new("soft")
        # What the faces painted next are made of, for a baked texture (furniture.py): an index
        # into SURFACES, 0 to leave it to their colour.
        self.surface = self.bm.faces.layers.int.new("surface")
        self.made_of = None

    def paint(self, colour, round=False):
        """`colour`: a name in C, or an sRGB triple."""
        c = lin(C[colour] if isinstance(colour, str) else colour)
        for f in self.bm.faces:
            if not f.tag:
                f[self.surface] = SURFACES.index(self.made_of) if self.made_of else 0
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

    def ball(self, c, r, colour, sub=1, squash=1.0, soft=False):
        bmesh.ops.create_icosphere(self.bm, subdivisions=sub, radius=r, matrix=Matrix.Translation(Vector(c)) @ Matrix.Diagonal((1, 1, squash, 1)))
        if soft:
            for f in self.bm.faces:
                if not f.tag:
                    f[self.soft] = 1
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
        soft = me.attributes.get("soft")
        if soft is not None:
            if any(v.value for v in soft.data):
                sharp = me.attributes["sharp_edge"].data
                keys = {e.key: e.index for e in me.edges}
                for poly in me.polygons:
                    if soft.data[poly.index].value:
                        for k in poly.edge_keys:
                            sharp[keys[k]].value = False
            me.attributes.remove(soft)
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
        # The heavy pieces (the two-wheelers, the auto: thousands of triangles in spokes, lamps and
        # cables) get a LOD2 for the far band, a tenth of LOD0: a parked row seen across a circle.
        if len(me.polygons) < HEAVY:
            return [o, lod1]
        lod2 = o.copy()
        lod2.data = o.data.copy()
        lod2.name = lod2.data.name = f"{self.name}_LOD2"
        bpy.context.scene.collection.objects.link(lod2)
        d = lod2.modifiers.new("decimate", "DECIMATE")
        d.ratio = 0.1
        bpy.context.view_layer.objects.active = lod2
        bpy.ops.object.modifier_apply(modifier=d.name)
        return [o, lod1, lod2]


# Triangles in LOD0 from which a piece gets a LOD2.
HEAVY = 3000


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

# The compact hatchbacks parked round the circles, by body colour (one piece each: the kit's
# colours are in the vertices).
CARS = {"car_white": (0.92, 0.92, 0.90), "car_red": (0.72, 0.08, 0.08), "car_silver": (0.62, 0.64, 0.66), "car_blue": (0.12, 0.24, 0.52)}


def car(name):
    """A compact hatchback, our own: a rounded tub on four wheels, the glasshouse set back with
    a short bonnet sloping to the nose, a flat roof, the tail cut off square; bumpers, lamps,
    grille, mirrors and plates. 3.8 m long, 1.66 wide, 1.5 high; the nose toward +Y."""
    p = Piece(name)
    body, dark = CARS[name], (0.12, 0.12, 0.13)
    R = 0.31
    # Wheels: tyres across X with a silver hub; the body's flanks hide their tops.
    for x in (-0.70, 0.70):
        for y in (-1.22, 1.24):
            p.cyl((x - 0.1, y, R), (x + 0.1, y, R), R, "tyre", 18)
            p.cyl((x + math.copysign(0.1, x), y, R), (x + math.copysign(0.115, x), y, R), 0.17, "chrome", 14)
    # The tub: tail to nose, its waist at about 0.85 m, the bonnet falling to the nose.
    bikes.loft(p, [((0, -1.90, 0.62), 0.78, 0.26, 0.35), ((0, -1.82, 0.60), 0.83, 0.30, 0.3), ((0, 0.9, 0.58), 0.83, 0.29, 0.3),
                   ((0, 1.6, 0.55), 0.81, 0.24, 0.32), ((0, 1.90, 0.50), 0.74, 0.17, 0.4)], body)
    # The glasshouse: tinted, narrowing to the roof, the windscreen raked well back, the tailgate
    # nearly upright; the roof flush on top and slim pillars along its edges.
    belt, top = 0.86, 1.42
    (yb0, yb1), (yt0, yt1), (xb, xt) = (-1.76, 0.95), (-1.62, 0.12), (0.78, 0.65)
    p.hull([(x, y, belt) for x in (-xb, xb) for y in (yb0, yb1)] + [(x, y, top) for x in (-xt, xt) for y in (yt0, yt1)], C["glass"])
    p.hull([(x, y, z) for x in (-xt + 0.01, xt - 0.01) for y in (yt0 + 0.02, yt1 - 0.02) for z in (top - 0.01, top + 0.05)], body)
    for x in (-1, 1):
        for (yb, yt) in ((yb1, yt1), (-0.38, -0.40), (yb0, yt0)):
            p.cyl((x * (xb + 0.005), yb, belt), (x * (xt + 0.005), yt, top), 0.04, body, 8)
        # The door mirror.
        p.box((x * 0.86 - 0.06, 0.70, 0.90), (x * 0.86 + 0.06, 0.82, 1.0), body)
    # Bumpers in dark plastic; the grille between the headlamps; lamps and plates.
    p.box((-0.82, 1.84, 0.30), (0.82, 1.98, 0.50), dark)
    p.box((-0.82, -1.98, 0.30), (0.82, -1.86, 0.52), dark)
    p.box((-0.34, 1.86, 0.55), (0.34, 1.92, 0.66), dark, round=False)
    for x in (-1, 1):
        p.box((x * 0.58 - 0.17, 1.80, 0.62), (x * 0.58 + 0.17, 1.90, 0.73), "lamp")
        p.box((x * 0.64 - 0.12, -1.93, 0.72), (x * 0.64 + 0.12, -1.86, 0.90), "red")
    p.box((-0.22, 1.98, 0.36), (0.22, 2.0, 0.46), "white", round=False)
    p.box((-0.22, -2.0, 0.60), (0.22, -1.98, 0.70), "white", round=False)
    # Door lines and handles, a shade darker than the paint.
    shade = tuple(c * 0.55 for c in body)
    for x in (-1, 1):
        for y in (-0.36, 0.86):
            p.box((x * 0.832, y - 0.01, 0.38), (x * 0.838, y + 0.01, 0.85), shade, round=False)
        p.box((x * 0.832, 0.32, 0.78), (x * 0.842, 0.46, 0.81), shade, round=False)
    return p.finish(0.03)


def auto():
    """The CNG auto-rickshaw: a rounded green tub tapering into its nose, a domed yellow canopy,
    the passengers' bench under it and the driver's saddle up front; one wheel ahead, two behind."""
    p = Piece("auto")
    green, yellow, black = C["auto_green"], C["auto_yellow"], C["black"]
    R = 0.22
    # Wheels: tyres and rims, the front one under the nose, the two rear ones at the sides.
    for (x, y) in ((0.0, 0.98), (-0.66, -0.78), (0.66, -0.78)):
        bikes.torus(p, y, R, R - 0.055, 0.055, bikes.TYRE, x=x, seg=20, ring=6)
        bikes.torus(p, y, R, R - 0.11, 0.018, bikes.ALLOY, x=x, seg=16, ring=4)
        p.cyl((x - 0.06, y, R), (x + 0.06, y, R), 0.06, "chrome", 10)
    # The tub: rounded, from the rear bumper to the dashboard, with a yellow waist band.
    bikes.loft(p, [((0, -1.27, 0.64), 0.62, 0.28, 0.35), ((0, -1.18, 0.64), 0.69, 0.34, 0.3), ((0, 0.15, 0.64), 0.69, 0.34, 0.3),
             ((0, 0.38, 0.66), 0.64, 0.32, 0.32)], green)
    bikes.loft(p, [((0, -1.20, 0.68), 0.70, 0.035, 0.6), ((0, 0.28, 0.68), 0.66, 0.035, 0.6)], yellow)
    # The nose: narrowing and rounding off over the front wheel; its headlamp and mudguard.
    bikes.loft(p, [((0, 0.32, 0.70), 0.62, 0.32, 0.32), ((0, 0.72, 0.76), 0.52, 0.33, 0.35), ((0, 1.0, 0.77), 0.40, 0.32, 0.4),
             ((0, 1.13, 0.77), 0.30, 0.26, 0.5)], green)
    p.cyl((0, 1.18, 0.88), (0, 1.25, 0.88), 0.085, "lamp", 16)
    p.cyl((0, 1.12, 0.88), (0, 1.20, 0.88), 0.10, "chrome", 16)
    # The front fork, from the wheel up into the cowl.
    p.cyl((0, 0.98, 0.22), (0, 0.95, 0.6), 0.03, "black", 8)
    # Bumper bars, chrome, front and back.
    p.cyl((-0.3, 1.13, 0.48), (0.3, 1.13, 0.48), 0.022, "chrome", 8)
    p.cyl((-0.62, -1.32, 0.40), (0.62, -1.32, 0.40), 0.025, "chrome", 8)
    for x in (-0.55, 0.55):
        p.cyl((x, -1.32, 0.40), (x, -1.2, 0.40), 0.02, "chrome", 6)
    bikes.arc_band(p, 0.98, R, R + 0.03, R + 0.045, 20, 120, 0.08, green, 8)
    for x in (-1, 1):
        p.box((x * 0.36 - 0.03, 1.02, 0.95), (x * 0.36 + 0.03, 1.08, 1.0), (0.98, 0.55, 0.06))
        bikes.arc_band(p, -0.78, R, R + 0.03, R + 0.045, 60, 175, 0.07, black, 8)
    # The canopy: a domed yellow roof on a black back, two posts at the front, the windscreen.
    # The canopy: one curved shell arching over the top and down the sides like a tent, its
    # black canvas back following the same arch; posts and the windscreen under it.
    bikes.arch(p, [((0, -1.27, 1.36), 0.72, 0.34, 0.7), ((0, -1.0, 1.36), 0.75, 0.37, 0.7), ((0, 0.40, 1.36), 0.75, 0.37, 0.7),
                   ((0, 0.66, 1.33), 0.71, 0.33, 0.7)], 0.035, yellow)
    bikes.arch_panel(p, -1.23, 0.95, 1.36, 0.69, 0.32, 0.7, 0.05, black)
    for x in (-0.64, 0.64):
        p.cyl((x, 0.56, 0.96), (x, 0.58, 1.48), 0.026, "black", 8)
        p.cyl((x, -0.30, 0.98), (x, -0.30, 1.48), 0.024, "black", 8)
        # The side curtains, rolled up under the canopy's edge, and the passengers' grab rail.
        p.cyl((x * 1.1, -1.18, 1.43), (x * 1.1, 0.48, 1.43), 0.045, "black", 10)
        p.cyl((x * 0.96, -1.05, 1.28), (x * 0.96, -0.36, 1.28), 0.014, "chrome", 6)
    bikes.loft(p, [((0, 0.73, 1.02), 0.56, 0.02, 0.5), ((0, 0.67, 1.3), 0.58, 0.02, 0.5), ((0, 0.63, 1.5), 0.5, 0.02, 0.5)], C["glass"])
    # Seats: the passengers' bench and backrest, the driver's saddle; the handlebar and its grips.
    bikes.cushion(p, -0.60, -1.12, 0.92, 0.62, 0.62, 0.12, 0.12, black, seg=14, n=8)
    bikes.loft(p, [((0, -1.14, 1.08), 0.6, 0.06, 0.45), ((0, -1.17, 1.32), 0.6, 0.06, 0.45)], black)
    bikes.cushion(p, 0.36, 0.06, 0.92, 0.12, 0.17, 0.09, 0.1, black)
    p.cyl((0, 0.66, 0.98), (0, 0.62, 1.10), 0.02, "black", 6)
    p.cyl((-0.34, 0.60, 1.10), (0.34, 0.60, 1.10), 0.014, "chrome", 6)
    for x in (-1, 1):
        p.cyl((x * 0.26, 0.60, 1.10), (x * 0.38, 0.60, 1.10), 0.02, "black", 8)
    # The back in truck art: bands of colour across the tail, and a sticker or two on the sides.
    for k, (c, z) in enumerate((("red", 0.44), ("auto_yellow", 0.51), ("blue", 0.58), ("pink", 0.65))):
        p.box((-0.46, -1.286, z), (0.46, -1.27, z + 0.055), c, round=False)
    for x in (-1, 1):
        p.box((x * 0.692, -0.9, 0.74), (x * 0.70, -0.62, 0.84), "saffron", round=False)
        p.box((x * 0.692, 0.0, 0.52), (x * 0.70, 0.22, 0.6), "white", round=False)
    # Tail lamps and the plates.
    for x in (-1, 1):
        p.box((x * 0.5 - 0.06, -1.29, 0.62), (x * 0.5 + 0.06, -1.25, 0.70), "red")
    p.box((-0.18, -1.29, 0.42), (0.18, -1.27, 0.54), "white", round=False)
    p.box((-0.12, 1.17, 0.56), (0.12, 1.19, 0.66), "white", round=False)
    return p.finish(0.03)


def scooter():
    """The everyday automatic scooter: a curved leg shield, a flat floor, a rounded body swelling
    under the seat, the engine unit on the left, 10-inch alloys."""
    p = Piece("scooter")
    red, dark, grey = C["scooter"], (0.16, 0.16, 0.17), (0.40, 0.41, 0.43)
    R, yr, yf = 0.23, -0.60, 0.62
    for y in (yr, yf):
        bikes.wheel(p, y, R, 0.09, False, bikes.ALLOY, hub_r=0.05)
    # Front: fork, leg shield rising and curving back to the handlebar cowl, the lamp in it.
    p.cyl((0, yf, R), (0, 0.50, 0.62), 0.028, "black", 8)
    bikes.loft(p, [((0, 0.53, 0.30), 0.19, 0.10, 0.5), ((0, 0.50, 0.55), 0.21, 0.07, 0.5),
                   ((0, 0.47, 0.80), 0.20, 0.06, 0.5), ((0, 0.43, 0.97), 0.15, 0.06, 0.55)], red)
    bikes.loft(p, [((0, 0.52, 1.04), 0.12, 0.06, 0.55), ((0, 0.44, 1.07), 0.13, 0.07, 0.55), ((0, 0.34, 1.06), 0.10, 0.05, 0.6)], red)
    p.cyl((0, 0.53, 1.03), (0, 0.57, 1.03), 0.05, "lamp", 14)
    p.cyl((-0.33, 0.40, 1.08), (0.33, 0.40, 1.08), 0.012, "black", 6)
    for x in (-1, 1):
        p.cyl((x * 0.25, 0.40, 1.08), (x * 0.35, 0.40, 1.08), 0.019, "black", 8)
        p.cyl((x * 0.20, 0.40, 1.09), (x * 0.26, 0.42, 1.30), 0.006, "black", 4)
        p.cyl((x * 0.26, 0.41, 1.33), (x * 0.26, 0.43, 1.33), 0.045, "black", 12)
        p.box((x * 0.18 - 0.02, 0.56, 0.98), (x * 0.18 + 0.02, 0.58, 1.01), (0.98, 0.55, 0.06))
    bikes.arc_band(p, yf, R, R + 0.025, R + 0.04, 20, 150, 0.07, red, 10)
    # The floor, and the body under the seat.
    bikes.loft(p, [((0, 0.44, 0.28), 0.17, 0.035, 0.4), ((0, 0.10, 0.27), 0.17, 0.035, 0.4), ((0, -0.08, 0.28), 0.16, 0.035, 0.4)], dark)
    bikes.loft(p, [((0, -0.02, 0.40), 0.15, 0.13, 0.6), ((0, -0.20, 0.46), 0.19, 0.18, 0.55), ((0, -0.48, 0.48), 0.20, 0.18, 0.55),
                   ((0, -0.70, 0.52), 0.16, 0.13, 0.6), ((0, -0.80, 0.56), 0.10, 0.08, 0.7)], red)
    bikes.cushion(p, -0.08, -0.74, 0.62, 0.12, 0.15, 0.10, 0.11, C["black"])
    # Engine unit and swingarm on the left, the silencer on the right.
    bikes.loft(p, [((-0.10, -0.26, 0.26), 0.05, 0.08, 0.5), ((-0.10, -0.45, 0.24), 0.06, 0.09, 0.5), ((-0.08, -0.62, 0.23), 0.05, 0.06, 0.6)], grey)
    p.cyl((0.10, -0.38, 0.22), (0.15, -0.82, 0.30), 0.045, "black", 12, r2=0.035)
    # Grab rail, tail lamp, plates.
    for x in (-1, 1):
        p.cyl((x * 0.15, -0.58, 0.70), (x * 0.12, -0.84, 0.68), 0.012, "chrome", 6)
    p.cyl((-0.12, -0.84, 0.68), (0.12, -0.84, 0.68), 0.012, "chrome", 6)
    p.box((-0.07, -0.84, 0.55), (0.07, -0.80, 0.60), "red")
    p.box((-0.09, -0.82, 0.38), (0.09, -0.81, 0.50), "white", round=False)
    # The front plate, on the leg shield's face.
    p.box((-0.07, 0.605, 0.40), (0.07, 0.615, 0.48), "white", round=False)
    return p.finish(0.02)


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


# ------------------------------------------------------------------ open spaces (parks, plazas)

def bench():
    """A park bench: wooden slats on two cast-iron ends."""
    p = Piece("bench")
    iron, wood = (0.10, 0.12, 0.11), (0.62, 0.38, 0.18)
    for x in (-0.75, 0.75):
        p.box((x - 0.04, -0.28, 0.0), (x + 0.04, 0.28, 0.06), iron)
        p.box((x - 0.035, -0.22, 0.0), (x + 0.035, -0.16, 0.44), iron)
        p.box((x - 0.035, 0.16, 0.0), (x + 0.035, 0.22, 0.80), iron)
        p.box((x - 0.035, -0.26, 0.40), (x + 0.035, 0.24, 0.46), iron)
    for k in range(4):
        y = -0.24 + k * 0.13
        p.box((-0.9, y, 0.44), (0.9, y + 0.1, 0.49), wood)
    for k in range(3):
        z = 0.55 + k * 0.1
        p.box((-0.9, 0.19, z), (0.9, 0.24, z + 0.07), wood)
    return p.finish(0.012)


def lamp():
    """A cast-iron street lamp: fluted post on a stepped base, a lantern globe on top."""
    p = Piece("lamp")
    iron = (0.08, 0.09, 0.09)
    p.cyl((0, 0, 0), (0, 0, 0.35), 0.17, iron, 12)
    p.cyl((0, 0, 0.35), (0, 0, 0.55), 0.12, iron, 12, r2=0.08)
    p.cyl((0, 0, 0.55), (0, 0, 3.6), 0.06, iron, 10, r2=0.045)
    p.cyl((0, 0, 3.6), (0, 0, 3.7), 0.1, iron, 10)
    p.ball((0, 0, 3.95), 0.24, (1.0, 0.95, 0.80), 2)
    p.cyl((0, 0, 4.17), (0, 0, 4.3), 0.13, iron, 10, r2=0.03)
    return p.finish(0.0)


def planter():
    """A round concrete planter, a lip round its rim, filled with dark soil. Its bush is the
    nature set's bougainvillea or lantana, set in by crate::map::kit::place."""
    p = Piece("planter")
    bikes.loft(p, [((0, 0, 0.0), 0.56, 0.56, 1.0), ((0, 0, 0.5), 0.64, 0.64, 1.0), ((0, 0, 0.53), 0.7, 0.7, 1.0),
                   ((0, 0, 0.6), 0.7, 0.7, 1.0)], C["concrete"])
    # The soil 1.5 cm proud of the pot's closed top (sharing its plane, the two flickered).
    p.cyl((0, 0, 0.56), (0, 0, 0.615), 0.62, (0.16, 0.11, 0.08), 16)
    return p.finish(0.0)


def umbrella():
    """A hawker's stall under a big striped umbrella: a folding table of goods, a stool."""
    p = Piece("umbrella")
    p.cyl((0, 0, 0), (0, 0, 2.35), 0.03, "steel", 8)
    cols = [(0.95, 0.20, 0.15), (0.98, 0.92, 0.85)]
    n = 12
    for k in range(n):
        a0, a1 = 2 * math.pi * k / n, 2 * math.pi * (k + 1) / n
        tip = Vector((0, 0, 2.55))
        rim = [Vector((1.35 * math.cos(a), 1.35 * math.sin(a), 2.05)) for a in (a0, a1)]
        p.quad([tip, rim[0], rim[1]], cols[k % 2])
    p.box((-0.6, 0.15, 0.0), (0.6, 0.75, 0.72), "wood")
    p.box((-0.65, 0.1, 0.72), (0.65, 0.8, 0.78), "wood_lt")
    for k in range(6):
        x = -0.5 + k * 0.2
        p.ball((x, 0.45, 0.85), 0.08, [C["orange"], C["banana"], C["apple"]][k % 3], 1)
    p.cyl((0.0, -0.5, 0.0), (0.0, -0.5, 0.42), 0.16, "blue", 10)
    return p.finish(0.02)


def bin():
    """A green municipal dustbin on a post."""
    p = Piece("bin")
    p.cyl((0, 0, 0), (0, 0, 0.5), 0.04, "steel", 6)
    bikes.loft(p, [((0, 0, 0.35), 0.24, 0.24, 1.0), ((0, 0, 0.95), 0.27, 0.27, 1.0)], (0.12, 0.45, 0.22))
    p.cyl((0, 0, 0.95), (0, 0, 1.0), 0.29, (0.08, 0.30, 0.15), 14)
    return p.finish(0.0)


# ------------------------------------------------------------------ furniture (indoors)
# Each stands on the floor at its origin, its front (the side you use it from) toward +Y.

def legs(p, x0, x1, y0, y1, h, r, colour, inset=0.05):
    for x in (x0 + inset, x1 - inset):
        for y in (y0 + inset, y1 - inset):
            p.cyl((x, y, 0), (x, y, h), r, colour, 6)


def charpai():
    """The woven cot: a wooden frame on four turned legs, the rope webbing in stripes."""
    p = Piece("charpai")
    legs(p, -0.95, 0.95, -0.46, 0.46, 0.46, 0.035, "wood", 0.04)
    for y in (-0.46, 0.46):
        p.box((-0.98, y - 0.035, 0.36), (0.98, y + 0.035, 0.44), "wood")
    for x in (-0.95, 0.95):
        p.box((x - 0.035, -0.46, 0.36), (x + 0.035, 0.46, 0.44), "wood")
    for k in range(9):
        x0 = -0.91 + k * 0.2022
        p.box((x0, -0.43, 0.40), (x0 + 0.19, 0.43, 0.42), "rope" if k % 3 else (0.20, 0.36, 0.62), round=False)
    return p.finish(0.0)


def almirah():
    """The steel cupboard: two doors, a handle and a lock, on short feet; grey-green enamel."""
    p = Piece("almirah")
    body = (0.42, 0.55, 0.52)
    p.box((-0.46, -0.25, 0.06), (0.46, 0.25, 1.86), body)
    legs(p, -0.46, 0.46, -0.25, 0.25, 0.07, 0.03, "black", 0.04)
    p.box((-0.003, 0.25, 0.12), (0.003, 0.255, 1.80), tuple(c * 0.6 for c in body), round=False)
    p.box((0.03, 0.25, 0.95), (0.06, 0.28, 1.15), "chrome")
    p.box((-0.44, 0.25, 1.78), (0.44, 0.256, 1.8), tuple(c * 0.8 for c in body), round=False)
    p.box((-0.48, -0.27, 1.86), (0.48, 0.27, 1.9), body)
    return p.finish(0.01)


def dining():
    """A wooden dining table and four chairs round it."""
    p = Piece("dining")
    legs(p, -0.7, 0.7, -0.4, 0.4, 0.73, 0.03, "wood")
    p.box((-0.72, -0.42, 0.72), (0.72, 0.42, 0.76), "wood_lt")
    for (x, y, yaw) in ((-0.38, -0.62, 0.0), (0.38, -0.62, 0.0), (-0.38, 0.62, math.pi), (0.38, 0.62, math.pi)):
        r = Matrix.Rotation(yaw, 3, "Z")
        o = Vector((x, y, 0))
        t = lambda q: tuple(o + r @ Vector(q))  # noqa: E731
        for (lx, ly) in ((-0.18, -0.18), (0.18, -0.18), (-0.18, 0.18), (0.18, 0.18)):
            p.cyl(t((lx, ly, 0)), t((lx, ly, 0.45)), 0.018, "wood", 6)
        p.hull([t((a, b, z)) for a in (-0.21, 0.21) for b in (-0.2, 0.2) for z in (0.44, 0.48)], "wood")
        p.hull([t((a, b, z)) for a in (-0.2, 0.2) for b in (-0.22, -0.18) for z in (0.48, 0.92)], "wood")
    return p.finish(0.005)


def sofa():
    """A three-seater in maroon velvet: three seat cushions, back cushions, square arms, on
    wooden feet."""
    p = Piece("sofa")
    red, dark = (0.50, 0.10, 0.14), (0.38, 0.07, 0.10)
    legs(p, -1.0, 1.0, -0.42, 0.42, 0.1, 0.03, "wood", 0.06)
    p.box((-1.0, -0.42, 0.1), (1.0, 0.42, 0.32), dark)
    p.box((-0.85, -0.42, 0.32), (0.85, -0.24, 0.62), dark)
    for k in range(3):
        x0 = -0.84 + k * 0.56
        p.box((x0 + 0.01, -0.26, 0.32), (x0 + 0.55, 0.40, 0.46), red)
        p.box((x0 + 0.02, -0.40, 0.46), (x0 + 0.54, -0.22, 0.86), red)
    for x in (-1, 1):
        p.box((x * 0.93 - 0.08, -0.42, 0.10), (x * 0.93 + 0.08, 0.42, 0.64), dark)
    return p.finish(0.03)


def tv():
    """A low wooden cabinet with a flat TV on its stand."""
    p = Piece("tv")
    p.box((-0.7, -0.22, 0.0), (0.7, 0.22, 0.5), "wood")
    for x in (-0.35, 0.35):
        p.box((x - 0.33, 0.22, 0.06), (x + 0.33, 0.225, 0.44), "wood_lt", round=False)
    p.box((-0.15, -0.1, 0.5), (0.15, 0.05, 0.52), "black")
    p.box((-0.03, -0.04, 0.52), (0.03, 0.0, 0.62), "black")
    p.box((-0.55, -0.05, 0.62), (0.55, 0.0, 1.24), "black")
    p.box((-0.52, 0.0, 0.65), (0.52, 0.004, 1.21), (0.06, 0.09, 0.14), round=False)
    return p.finish(0.005)


def fridge():
    """A single-door fridge in maroon, the freezer's line near the top, a long handle."""
    p = Piece("fridge")
    body = (0.55, 0.12, 0.16)
    p.box((-0.31, -0.32, 0.04), (0.31, 0.32, 1.56), body)
    p.box((-0.3, -0.3, 0.0), (0.3, 0.3, 0.05), "black")
    p.box((-0.29, 0.32, 1.22), (0.29, 0.325, 1.23), tuple(c * 0.6 for c in body), round=False)
    p.box((0.22, 0.32, 0.6), (0.25, 0.36, 1.15), "chrome")
    return p.finish(0.03)


def fan():
    """A ceiling fan hanging from its origin (the ceiling): a down-rod, the motor, three blades."""
    p = Piece("fan")
    p.cyl((0, 0, 0), (0, 0, -0.32), 0.015, "white", 6)
    p.cyl((0, 0, -0.30), (0, 0, -0.42), 0.11, "white", 14, 0.09)
    for k in range(3):
        a = k * 2 * math.pi / 3
        d, n = Vector((math.cos(a), math.sin(a), 0)), Vector((-math.sin(a), math.cos(a), 0))
        pts = []
        for (r, w) in ((0.10, 0.04), (0.62, 0.065)):
            for side in (-1, 1):
                for z in (-0.37, -0.355):
                    q = d * r + n * w * side
                    pts.append((q.x, q.y, z + 0.008 * side))
        p.hull(pts, "cream")
    return p.finish(0.0)


def dari():
    """A cotton dari, flat on the floor: broad stripes of colour with thin bands between."""
    p = Piece("dari")
    stripes = [(0.72, 0.18, 0.14), "cream", (0.16, 0.30, 0.55), "cream", (0.82, 0.55, 0.12), "cream", (0.72, 0.18, 0.14)]
    n = len(stripes)
    for k, c in enumerate(stripes):
        y0 = -0.8 + 1.6 * k / n
        p.box((-1.2, y0, 0.0), (1.2, y0 + 1.6 / n, 0.012), c, round=False)
    return p.finish(0.0)


def desk():
    """An office desk: a laminate top on panel ends, a drawer pedestal, a monitor and keyboard."""
    p = Piece("desk")
    top, panel = (0.80, 0.70, 0.56), (0.55, 0.45, 0.35)
    p.box((-0.7, -0.35, 0.72), (0.7, 0.35, 0.75), top)
    p.box((-0.7, -0.35, 0.0), (-0.67, 0.35, 0.72), panel)
    p.box((0.25, -0.33, 0.0), (0.68, 0.33, 0.72), panel)
    for z in (0.18, 0.42, 0.62):
        p.box((0.28, 0.33, z), (0.65, 0.335, z + 0.02), "chrome", round=False)
    p.box((-0.65, -0.34, 0.25), (0.25, -0.32, 0.7), panel)
    p.box((-0.12, -0.2, 0.75), (0.12, -0.08, 0.77), "black")
    p.box((-0.02, -0.16, 0.77), (0.02, -0.12, 0.88), "black")
    p.box((-0.27, -0.18, 0.86), (0.27, -0.15, 1.18), "black")
    p.box((-0.25, -0.15, 0.88), (0.25, -0.148, 1.16), (0.10, 0.18, 0.28), round=False)
    p.box((-0.22, 0.02, 0.75), (0.22, 0.16, 0.77), "grey", round=False)
    return p.finish(0.005)


def stove():
    """The kitchen: a tiled counter with a black stone top, a two-burner steel stove on it, the red
    LPG cylinder at its end."""
    p = Piece("stove")
    p.box((-0.8, -0.3, 0.0), (0.8, 0.3, 0.82), (0.86, 0.88, 0.84))
    for x in (-0.4, 0.4):
        p.box((x - 0.36, 0.3, 0.06), (x + 0.36, 0.305, 0.76), (0.36, 0.55, 0.62), round=False)
    p.box((-0.82, -0.32, 0.82), (0.82, 0.32, 0.86), (0.08, 0.08, 0.09))
    p.box((-0.35, -0.2, 0.86), (0.35, 0.18, 0.94), "steel")
    for x in (-0.17, 0.17):
        p.cyl((x, -0.01, 0.94), (x, -0.01, 0.96), 0.08, "black", 12)
    p.cyl((0.62, -0.05, 0.86), (0.62, -0.05, 0.98), 0.12, "kettle", 12)
    p.cyl((-0.98, 0.0, 0.0), (-0.98, 0.0, 0.52), 0.15, (0.80, 0.10, 0.08), 14)
    p.cyl((-0.98, 0.0, 0.52), (-0.98, 0.0, 0.6), 0.06, "chrome", 8)
    return p.finish(0.01)


FURNITURE = (charpai, almirah, dining, sofa, tv, fridge, fan, dari, desk, stove)


# ------------------------------------------------------------------ shop racks
# A shop's shelving, stocked: 1.2 m of rack 2.0 m high and 0.4 deep, its back on the wall (-Y),
# the stock facing the shop (+Y). One per trade; each seeded, so a rack looks the same each build.
RACKS = ("kirana", "bartan", "kapda", "dabba")
RACK_W, RACK_D, RACK_H, SHELF_Z = 1.2, 0.4, 2.0, (0.08, 0.5, 0.92, 1.34, 1.76)


def rack_frame(p, wood):
    hw, d = RACK_W / 2, RACK_D
    frame = "wood" if wood else (0.42, 0.44, 0.46)
    p.made_of = "wood" if wood else "metal"
    for x in (-hw, hw - 0.04):
        for y in (-d / 2, d / 2 - 0.04):
            p.box((x, y, 0), (x + 0.04, y + 0.04, RACK_H), frame, round=False)
    for z in SHELF_Z:
        p.box((-hw, -d / 2, z - 0.025), (hw, d / 2, z), frame if wood else (0.5, 0.52, 0.54), round=False)
    # The back panel, plywood.
    p.made_of = "wood"
    p.box((-hw, -d / 2, 0), (hw, -d / 2 + 0.01, RACK_H), (0.45, 0.33, 0.22), round=False)
    p.made_of = None


def rack(trade):
    rnd = random.Random(f"rack_{trade}")
    p = Piece(f"rack_{trade}")
    rack_frame(p, trade in ("kapda", "kirana"))
    hw, d = RACK_W / 2 - 0.05, RACK_D / 2
    for k in range(len(SHELF_Z) - 1):
        z0, z1 = SHELF_Z[k], SHELF_Z[k + 1] - 0.04
        x = -hw + rnd.uniform(0.0, 0.03)
        while True:
            if trade == "kirana":
                kind = rnd.choice(["jar", "jar", "bottle", "packet", "packet", "tin"])
                if kind == "jar":
                    r, h = rnd.uniform(0.045, 0.065), rnd.uniform(0.14, 0.22)
                    if x + 2 * r > hw:
                        break
                    for row in (0.06, -0.08):
                        fill = rnd.choice([(0.92, 0.72, 0.22), (0.95, 0.93, 0.86), (0.72, 0.16, 0.08), (0.55, 0.36, 0.20), (0.36, 0.50, 0.20)])
                        p.made_of = "glass"
                        p.cyl((x + r, row, z0), (x + r, row, z0 + h), r, fill, 10)
                        p.made_of = "plastic"
                        p.cyl((x + r, row, z0 + h), (x + r, row, z0 + h + 0.025), r * 0.85, rnd.choice(["red", "blue", "auto_yellow"]), 10)
                        p.made_of = "print"
                        p.cyl((x + r, row, z0 + h * 0.35), (x + r, row, z0 + h * 0.7), r * 1.01, "white", 10)
                    x += 2 * r + 0.01
                elif kind == "bottle":
                    r, h = rnd.uniform(0.028, 0.038), rnd.uniform(0.24, 0.32)
                    if x + 2 * r > hw:
                        break
                    fill = rnd.choice([(0.95, 0.78, 0.22), (0.22, 0.42, 0.20), (0.55, 0.12, 0.08)])
                    p.made_of = "glass"
                    p.cyl((x + r, 0.07, z0), (x + r, 0.07, z0 + h * 0.65), r, fill, 8)
                    p.cyl((x + r, 0.07, z0 + h * 0.65), (x + r, 0.07, z0 + h), r * 0.4, fill, 8, r * 0.3)
                    p.made_of = "print"
                    p.cyl((x + r, 0.07, z0 + h * 0.2), (x + r, 0.07, z0 + h * 0.45), r * 1.02, "cream", 8)
                    x += 2 * r + 0.008
                elif kind == "tin":
                    r, h = rnd.uniform(0.04, 0.06), rnd.uniform(0.08, 0.13)
                    if x + 2 * r > hw:
                        break
                    for row in (0.07, -0.07):
                        p.made_of = "metal"
                        p.cyl((x + r, row, z0), (x + r, row, z0 + h), r, "tin", 10)
                        p.made_of = "print"
                        p.cyl((x + r, row, z0 + h * 0.2), (x + r, row, z0 + h * 0.8), r * 1.01, rnd.choice(["red", "blue", "leaf", "saffron"]), 10)
                    x += 2 * r + 0.01
                else:
                    w, h, dd = rnd.uniform(0.07, 0.13), rnd.uniform(0.14, 0.3), rnd.uniform(0.04, 0.08)
                    if x + w > hw:
                        break
                    p.made_of = "print"
                    c = rnd.choice([(0.80, 0.22, 0.14), (0.93, 0.72, 0.16), (0.22, 0.42, 0.68), (0.28, 0.55, 0.28), (0.92, 0.90, 0.82), (0.55, 0.20, 0.40)])
                    for row in range(3):
                        y = d - 0.02 - dd / 2 - row * (dd + 0.01)
                        p.box((x, y - dd / 2, z0), (x + w, y + dd / 2, z0 + min(h, z1 - z0)), c, round=False)
                    x += w + 0.006
            elif trade == "bartan":
                kind = rnd.choice(["plates", "plates", "glasses", "pot", "cooker"])
                if kind == "plates":
                    r = rnd.uniform(0.11, 0.15)
                    if x + 2 * r > hw:
                        break
                    n = rnd.randint(6, 14)
                    p.made_of = "metal"
                    for j in range(n):
                        p.cyl((x + r, 0.0, z0 + j * 0.012), (x + r, 0.0, z0 + j * 0.012 + 0.01), r - (j % 2) * 0.004, "steel", 14)
                    x += 2 * r + 0.015
                elif kind == "glasses":
                    r = 0.038
                    if x + 2 * r > hw:
                        break
                    p.made_of = "metal"
                    for row in (0.08, -0.04):
                        n = rnd.randint(2, 5)
                        p.cyl((x + r, row, z0), (x + r, row, z0 + 0.11 + 0.025 * n), r, "steel", 10, r * 0.85)
                    x += 2 * r + 0.01
                elif kind == "pot":
                    r, h = rnd.uniform(0.09, 0.14), rnd.uniform(0.09, 0.16)
                    if x + 2 * r > hw:
                        break
                    c = "steel" if rnd.random() < 0.75 else (0.72, 0.42, 0.24)
                    p.made_of = "metal"
                    p.cyl((x + r, 0.0, z0), (x + r, 0.0, z0 + h), r * 0.9, c, 14, r)
                    p.cyl((x + r, 0.0, z0 + h), (x + r, 0.0, z0 + h + 0.012), r * 1.05, c, 14)
                    x += 2 * r + 0.012
                else:
                    r, h = 0.1, 0.16
                    if x + 2 * r > hw:
                        break
                    p.made_of = "metal"
                    p.cyl((x + r, 0.0, z0), (x + r, 0.0, z0 + h), r, "steel", 14)
                    p.made_of = "plastic"
                    p.cyl((x + r, 0.0, z0 + h), (x + r, 0.0, z0 + h + 0.03), r * 0.95, "black", 14)
                    p.box((x + r - 0.01, 0.0, z0 + h + 0.01), (x + r + 0.01, 0.2, z0 + h + 0.03), "black", round=False)
                    p.cyl((x + r, 0.0, z0 + h + 0.03), (x + r, 0.0, z0 + h + 0.07), 0.015, "black", 6)
                    x += 2 * r + 0.015
            elif trade == "kapda":
                w = rnd.uniform(0.26, 0.36)
                if x + w > hw:
                    break
                zz = z0
                p.made_of = "cotton"
                while zz < z1 - 0.06:
                    hh = rnd.uniform(0.035, 0.06)
                    c = rnd.choice([(0.72, 0.10, 0.22), (0.92, 0.60, 0.12), (0.14, 0.38, 0.44), (0.88, 0.84, 0.74), (0.38, 0.14, 0.42),
                                    (0.20, 0.32, 0.62), (0.80, 0.34, 0.16), (0.30, 0.48, 0.24), (0.10, 0.10, 0.12)])
                    o = rnd.uniform(-0.012, 0.012)
                    p.box((x + o, -d + 0.03, zz), (x + w + o, d - 0.02, zz + hh), c)
                    zz += hh + 0.002
                x += w + rnd.uniform(0.01, 0.03)
            else:
                w, h = rnd.uniform(0.2, 0.36), rnd.uniform(0.18, z1 - z0 - 0.02)
                if x + w > hw:
                    break
                p.made_of = "kraft"
                kraft = tuple(c * rnd.uniform(0.85, 1.1) for c in (0.62, 0.47, 0.30))
                p.box((x, -d + 0.03, z0), (x + w, d - 0.02, z0 + h), kraft, round=False)
                # The printed panel on its front, and the tape over the flaps.
                p.made_of = "print"
                p.box((x + w * 0.2, d - 0.02, z0 + h * 0.3), (x + w * 0.8, d - 0.017, z0 + h * 0.7), rnd.choice(["white", "red", "blue", "cream"]), round=False)
                p.made_of = "kraft"
                p.box((x + w * 0.45, -d + 0.03, z0 + h), (x + w * 0.55, d - 0.02, z0 + h + 0.003), tuple(c * 1.15 for c in kraft), round=False)
                x += w + rnd.uniform(0.006, 0.02)
    p.made_of = None
    return p.finish(0.0)


def racks():
    return [o for t in RACKS for o in rack(t)]



def main():
    export.reset_scene()
    bake.setup_cycles(32)
    bpy.context.scene.world = bpy.context.scene.world or bpy.data.worlds.new("World")
    bpy.context.scene.world.light_settings.distance = 0.6
    objs = []
    for build in (auto, scooter, thela, chai, pole, drum, crates, chairs, pot, cooler, dish, solar, clothesline, mumty,
                  bench, lamp, planter, umbrella, bin):
        objs += build()
    for build in bikes.BIKES:
        objs += build(Piece)
    for name in CARS:
        objs += car(name)
    for build in FURNITURE:
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


if __name__ == "__main__":
    main()

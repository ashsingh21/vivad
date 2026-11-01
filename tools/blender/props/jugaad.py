"""Udta Auto: the jugaad drop ride, Panga's default (the Pushpaka Vimana stays, VIVAD_VIMANA=pushpaka).
A Delhi CNG auto-rickshaw blown up seven times to carry the whole lobby: green tub, yellow canopy,
one headlight, three wheels, and a rear panel in truck art with HORN OK PLEASE. It flies on jugaad:
four ceiling fans on bamboo poles lashed across the roof, two gas cylinders strapped under the back
as rockets, flames out of them; marigold garlands round the canopy and a nimbu-mirchi hanging off
the front for luck.

Modelled at a real auto's size, then scaled by S. The origin is the vimana's position in the sim
(`Match::vimana_pos`), the passenger floor at z = 0; it flies toward Blender +Y (game -Z).
~20 m nose to rockets, 30 m across the fans, 18 m tall. Flat colours, bevelled, no textures.

The fan rotors (`fan_0..3`) and flames (`flame_0..1`) are separate nodes, origins at their hubs and
nozzles, for the game to spin and flicker (crate::visuals).

blender -b --factory-startup -P tools/blender/props/jugaad.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import bikes  # noqa: E402
import export  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "models", "props")
S = 7.0
DECK = 0.45  # the passenger floor's height on a real auto

MATS = {}


def mat(name, rgb, rough=0.6, metal=0.0, glow=0.0, alpha=1.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    p = m.node_tree.nodes["Principled BSDF"]
    lin = tuple(c ** 2.2 for c in rgb)
    p.inputs["Base Color"].default_value = (*lin, 1.0)
    p.inputs["Roughness"].default_value = rough
    p.inputs["Metallic"].default_value = metal
    if glow:
        p.inputs["Emission Color"].default_value = (*lin, 1.0)
        p.inputs["Emission Strength"].default_value = glow
    if alpha < 1.0:
        # See-through (exported as glTF BLEND): the flame's outer layer shows its core.
        p.inputs["Alpha"].default_value = alpha
        m.surface_render_method = "BLENDED"
    MATS[name] = m
    return m


def link(name, bm, material, *, bevel=0.0, origin=Vector()):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.materials.append(MATS[material])
    o = bpy.data.objects.new(name, me)
    o.location = origin
    bpy.context.scene.collection.objects.link(o)
    if bevel:
        b = o.modifiers.new("bevel", "BEVEL")
        b.width = bevel
        b.segments = 2
        b.limit_method = "ANGLE"
        b.harden_normals = False
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_apply(modifier=b.name)
    for p in o.data.polygons:
        p.use_smooth = True
    o.data.set_sharp_from_angle(angle=math.radians(35))
    return o


def hull(name, pts, material, bevel=0.03):
    """The convex hull of some points: chunky rounded shapes in a few lines."""
    bm = bmesh.new()
    for p in pts:
        bm.verts.new(p)
    bmesh.ops.convex_hull(bm, input=bm.verts)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return link(name, bm, material, bevel=bevel)


def box(name, mn, mx, material, bevel=0.02):
    pts = [Vector((x, y, z)) for x in (mn[0], mx[0]) for y in (mn[1], mx[1]) for z in (mn[2], mx[2])]
    return hull(name, pts, material, bevel)


def cyl(name, a, b, r, material, seg=16, bevel=0.0, origin=None):
    """A cylinder from a to b (world points); its object origin at `origin` (default: a)."""
    a, b = Vector(a), Vector(b)
    origin = Vector(origin) if origin is not None else a
    bm = bmesh.new()
    d = (b - a)
    rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r, radius2=r, depth=d.length,
                          matrix=Matrix.Translation(a + d / 2 - origin) @ rot)
    return link(name, bm, material, bevel=bevel, origin=origin)


def cone(name, a, b, r1, r2, material, seg=16, origin=None):
    a, b = Vector(a), Vector(b)
    origin = Vector(origin) if origin is not None else a
    bm = bmesh.new()
    d = b - a
    rot = d.to_track_quat("Z", "Y").to_matrix().to_4x4()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r1, radius2=r2, depth=d.length,
                          matrix=Matrix.Translation(a + d / 2 - origin) @ rot)
    return link(name, bm, material, origin=origin)


def ball(name, c, r, material, sub=1):
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=sub, radius=r, matrix=Matrix.Translation(Vector(c)))
    return link(name, bm, material)


def text(name, s, at, size, material, rot=(math.pi / 2, 0, 0), depth=0.01):
    """Raised lettering; by default facing -Y (the back), readable from behind."""
    cu = bpy.data.curves.new(name, "FONT")
    cu.body = s
    cu.size = size
    cu.extrude = depth
    cu.align_x = "CENTER"
    cu.align_y = "CENTER"
    o = bpy.data.objects.new(name, cu)
    o.location = Vector(at)
    o.rotation_euler = rot
    bpy.context.scene.collection.objects.link(o)
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    bpy.ops.object.convert(target="MESH")
    o = bpy.context.view_layer.objects.active
    o.data.materials.clear()
    o.data.materials.append(MATS[material])
    o.select_set(False)
    return o


# ------------------------------------------------------------------ the auto

class Part:
    """A bmesh for the smooth-body helpers in bikes.py (loft, cushion, torus, arc_band), which
    paint by face tag; here each part takes a material instead (`link`)."""

    def __init__(self):
        self.bm = bmesh.new()

    def paint(self, colour, round=False):
        for f in self.bm.faces:
            f.tag = True


def smooth(name, material, build):
    """One part built by `build(part)` from the smooth helpers, linked with its material."""
    part = Part()
    build(part)
    return link(name, part.bm, material)


def auto():
    parts = []
    add = parts.append
    R = 0.22
    # The tub: rounded, from the rear bumper to the dashboard; the nose narrowing over the front
    # wheel, short and tall like an auto's, not a car's bonnet.
    add(smooth("tub", "green", lambda p: bikes.loft(p, [((0, -1.27, 0.64), 0.62, 0.28, 0.35), ((0, -1.18, 0.64), 0.69, 0.34, 0.3), ((0, 0.15, 0.64), 0.69, 0.34, 0.3),
             ((0, 0.38, 0.66), 0.64, 0.32, 0.32)], 0)))
    add(smooth("nose", "green", lambda p: bikes.loft(p, [((0, 0.32, 0.70), 0.62, 0.32, 0.32), ((0, 0.72, 0.76), 0.52, 0.33, 0.35), ((0, 1.0, 0.77), 0.40, 0.32, 0.4),
             ((0, 1.13, 0.77), 0.30, 0.26, 0.5)], 0)))
    add(box("floor", (-0.64, -1.18, DECK - 0.04), (0.64, 0.6, DECK), "dark", 0.01))
    add(smooth("band", "yellow", lambda p: bikes.loft(p, [((0, -1.20, 0.68), 0.70, 0.035, 0.6), ((0, 0.28, 0.68), 0.66, 0.035, 0.6)], 0)))
    add(smooth("skirt", "chrome", lambda p: bikes.loft(p, [((0, -1.18, 0.36), 0.70, 0.025, 0.6), ((0, 0.26, 0.36), 0.65, 0.025, 0.6)], 0)))
    # The canopy: a domed yellow roof on its black canvas back; posts; the windscreen.
    add(smooth("roof", "yellow", lambda p: bikes.loft(p, [((0, -1.32, 1.66), 0.66, 0.06, 0.6), ((0, -1.22, 1.70), 0.74, 0.10, 0.5),
                                                          ((0, 0.45, 1.70), 0.74, 0.10, 0.5), ((0, 0.66, 1.64), 0.66, 0.06, 0.6)], 0)))
    add(smooth("canvas", "dark", lambda p: bikes.loft(p, [((0, -1.22, 0.96), 0.68, 0.05, 0.4), ((0, -1.24, 1.30), 0.71, 0.05, 0.4),
                                                          ((0, -1.22, 1.62), 0.70, 0.05, 0.45)], 0)))
    for x in (-0.66, 0.66):
        add(cyl(f"pillar_f{x}", (x, 0.56, 0.96), (x, 0.58, 1.64), 0.026, "dark", 8))
        add(cyl(f"pillar_m{x}", (x, -0.3, 0.98), (x, -0.3, 1.64), 0.025, "dark", 8))
    add(smooth("screen", "glass", lambda p: bikes.loft(p, [((0, 0.73, 1.02), 0.56, 0.02, 0.5), ((0, 0.66, 1.34), 0.58, 0.02, 0.5),
                                                           ((0, 0.62, 1.58), 0.56, 0.02, 0.5)], 0)))
    add(cyl("bar", (-0.34, 0.60, 1.10), (0.34, 0.60, 1.10), 0.016, "chrome", 8))
    for x in (-1, 1):
        add(cyl(f"grip{x}", (x * 0.26, 0.60, 1.10), (x * 0.38, 0.60, 1.10), 0.022, "dark", 8))
    add(cyl("headlight_shell", (0, 1.12, 0.88), (0, 1.22, 0.88), 0.11, "chrome", 16, 0.01))
    add(cyl("headlight", (0, 1.22, 0.88), (0, 1.23, 0.88), 0.09, "lamp", 16))
    add(cyl("fork", (0, 0.98, 0.22), (0, 0.95, 0.6), 0.03, "dark", 8))
    # Seats: the passengers' bench and backrest, the driver's saddle.
    add(smooth("seat", "seat", lambda p: bikes.cushion(p, -0.60, -1.12, DECK + 0.02, 0.62, 0.62, 0.24, 0.24, 0, seg=14, n=8)))
    add(smooth("seat_back", "seat", lambda p: bikes.loft(p, [((0, -1.14, 0.80), 0.6, 0.07, 0.45), ((0, -1.17, 1.12), 0.6, 0.07, 0.45)], 0)))
    add(smooth("driver_seat", "seat", lambda p: bikes.cushion(p, 0.42, 0.06, DECK + 0.08, 0.13, 0.19, 0.2, 0.22, 0)))
    # Wheels: tyres and rims, one under the nose, two at the back; mudguards.
    for name, (x, y) in (("wheel_f", (0.0, 0.98)), ("wheel_l", (-0.66, -0.78)), ("wheel_r", (0.66, -0.78))):
        add(smooth(name, "tyre", lambda p, x=x, y=y: bikes.torus(p, y, R, R - 0.055, 0.055, 0, x=x, seg=20, ring=6)))
        add(smooth(name + "_rim", "chrome", lambda p, x=x, y=y: bikes.torus(p, y, R, R - 0.11, 0.018, 0, x=x, seg=16, ring=4)))
        add(cyl(name + "_hub", (x - 0.075, y, R), (x + 0.075, y, R), 0.06, "chrome", 12))
    add(smooth("mudguard", "green", lambda p: bikes.arc_band(p, 0.98, R, R + 0.03, R + 0.045, 20, 120, 0.08, 0, 8)))
    # The back in truck art: bands of colour, tail lights, HORN OK PLEASE.
    for k, (c, z) in enumerate((("red", 0.42), ("yellow", 0.50), ("blue", 0.58), ("pink", 0.66), ("green_lt", 0.74))):
        add(box(f"stripe_{k}", (-0.5, -1.285, z), (0.5, -1.262, z + 0.07), c, 0.0))
    for x in (-0.5, 0.5):
        add(cyl(f"tail{x}", (x, -1.24, 0.86), (x, -1.29, 0.86), 0.055, "tail", 12))
    add(text("horn", "HORN OK PLEASE", (0, -1.29, 1.32), 0.17, "white"))
    add(text("sign_l", "UDTA AUTO", (-0.715, -0.5, 0.82), 0.16, "white", rot=(math.pi / 2, 0, -math.pi / 2)))
    add(text("sign_r", "UDTA AUTO", (0.715, -0.5, 0.82), 0.16, "white", rot=(math.pi / 2, 0, math.pi / 2)))
    return parts


def flame(name, nozzle, length, radius, material):
    """A stylised flame out of a nozzle, aft (-Y): a teardrop that swells then tapers to a point,
    its rim rippling in three lobes that twist along it. Origin at the nozzle, which the game
    flickers it about (crate::visuals::run_jugaad)."""
    bm = bmesh.new()
    rings, n, seg = [], 12, 18
    for i in range(n + 1):
        t = i / n
        r = radius * (1.0 + 0.6 * math.sin(math.pi * min(t / 0.7, 1.0) * 0.5)) * (1 - t) ** 0.75
        ring = []
        for j in range(seg):
            a = 2 * math.pi * j / seg
            rr = max(r * (1 + 0.16 * math.sin(3 * a + 7 * t)), 0.002)
            ring.append(bm.verts.new((rr * math.cos(a), -length * t, rr * math.sin(a))))
        rings.append(ring)
    for i in range(n):
        for j in range(seg):
            bm.faces.new((rings[i][j], rings[i][(j + 1) % seg], rings[i + 1][(j + 1) % seg], rings[i + 1][j]))
    bm.faces.new(rings[0])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return link(name, bm, material, origin=nozzle)


def jugaad():
    """The flying rig: bamboo across the roof, fans up on poles, cylinders for rockets."""
    parts, movers = [], []
    add = parts.append
    zr = 1.80
    for y in (0.35, -0.95):
        add(cyl(f"cross_{y}", (-2.15, y, zr + 0.04), (2.15, y, zr + 0.04), 0.045, "bamboo", 10))
    for x in (-1.9, 1.9):
        add(cyl(f"rail_{x}", (x, -1.15, zr + 0.04), (x, 0.55, zr + 0.04), 0.04, "bamboo", 10))
    # Rope lashings where bamboo meets roof.
    for x in (-0.6, 0.6):
        for y in (0.35, -0.95):
            add(cyl(f"lash_{x}_{y}", (x - 0.05, y, zr + 0.04), (x + 0.05, y, zr + 0.04), 0.06, "rope", 8))
    k = 0
    for x in (-1.9, 1.9):
        for y in (0.35, -0.95):
            hub = Vector((x, y, zr + 0.62))
            add(cyl(f"pole_{k}", (x, y, zr), hub, 0.035, "bamboo", 10))
            add(cyl(f"motor_{k}", hub - Vector((0, 0, 0.1)), hub + Vector((0, 0, 0.02)), 0.12, "white", 16, 0.02))
            # The rotor: three blades and a cap, spun about its hub.
            bm = bmesh.new()
            for b in range(3):
                a = 2 * math.pi * b / 3
                rot = Matrix.Rotation(a, 4, "Z")
                blade = bmesh.ops.create_cube(bm, size=1.0)["verts"]
                for v in blade:
                    v.co = rot @ Vector((v.co.x * 0.72 + 0.48, v.co.y * 0.16, v.co.z * 0.012 + 0.03))
            bmesh.ops.create_cone(bm, cap_ends=True, segments=12, radius1=0.09, radius2=0.06, depth=0.07, matrix=Matrix.Translation((0, 0, 0.06)))
            movers.append(link(f"fan_{k}", bm, "fan", origin=hub))
            k += 1
    # Two gas cylinders under the back, nozzles aft, flames out of them.
    for i, x in enumerate((-0.36, 0.36)):
        y0, y1, z = -0.9, -1.75, 0.16
        add(cyl(f"cyl_{i}", (x, y0, z), (x, y1, z), 0.16, "red", 20, 0.05))
        add(cone(f"nozzle_{i}", (x, y1, z), (x, y1 - 0.14, z), 0.07, 0.11, "chrome", 16))
        for y in (-1.05, -1.55):
            add(cyl(f"strap_{i}_{y}", (x, y - 0.02, z), (x, y + 0.02, z), 0.17, "dark", 20))
        nozzle = Vector((x, y1 - 0.14, z))
        # The flame: an orange teardrop with a hot yellow-white core (each flickers on its own).
        movers.append(flame(f"flame_{i}", nozzle, 1.05, 0.11, "flame"))
        movers.append(flame(f"flame_{i + 2}", nozzle, 0.62, 0.065, "flame_core"))
    return parts, movers


def garlands():
    """Marigolds slung under the canopy's edge on all four sides, and a nimbu-mirchi off the front."""
    parts = []
    corners = [(-0.70, 0.62), (0.70, 0.62), (0.70, -1.28), (-0.70, -1.28)]
    for (ax, ay), (bx, by) in zip(corners, corners[1:] + corners[:1]):
        length = math.hypot(bx - ax, by - ay)
        n = int(length / 0.07)
        for k in range(n + 1):
            t = k / n
            z = 1.60 - 0.16 * math.sin(math.pi * t)
            colour = "marigold" if k % 2 == 0 else "marigold2"
            parts.append(ball(f"m_{ax}_{ay}_{k}", (ax + (bx - ax) * t, ay + (by - ay) * t, z), 0.04, colour))
    top = Vector((0, 0.64, 1.62))
    parts.append(cyl("totka_string", top, top - Vector((0, 0, 0.42)), 0.006, "dark", 6))
    parts.append(ball("nimbu", top - Vector((0, 0, 0.1)), 0.05, "lemon", 2))
    for k in range(5):
        a = 2 * math.pi * k / 5
        p = top - Vector((0, 0, 0.2 + 0.035 * k))
        parts.append(cone(f"mirchi_{k}", p, p + Vector((0.07 * math.cos(a), 0.07 * math.sin(a), -0.13)), 0.018, 0.002, "chilli", 8))
    parts.append(ball("nimbu2", top - Vector((0, 0, 0.42)), 0.05, "lemon", 2))
    return parts


def finish(objs, name):
    """Scale to size, deck to z = 0, apply; join the static parts into one."""
    for o in objs:
        o.location = (o.location - Vector((0, 0, DECK))) * S
        o.scale = (S, S, S)
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)


def main():
    export.reset_scene()
    mat("green", (0.10, 0.52, 0.22), 0.45)
    mat("green_lt", (0.30, 0.80, 0.30), 0.5)
    mat("yellow", (0.99, 0.80, 0.08), 0.45)
    mat("dark", (0.07, 0.07, 0.08), 0.7)
    mat("chrome", (0.85, 0.85, 0.88), 0.25, 1.0)
    mat("glass", (0.35, 0.50, 0.55), 0.05)
    mat("tyre", (0.05, 0.05, 0.05), 0.85)
    mat("seat", (0.30, 0.12, 0.08), 0.6)
    mat("lamp", (1.0, 0.92, 0.70), 0.2, glow=6.0)
    mat("tail", (1.0, 0.10, 0.05), 0.3, glow=4.0)
    mat("red", (0.84, 0.12, 0.08), 0.4)
    mat("blue", (0.10, 0.35, 0.80), 0.5)
    mat("pink", (0.92, 0.18, 0.50), 0.5)
    mat("white", (0.97, 0.95, 0.90), 0.5)
    mat("bamboo", (0.80, 0.64, 0.36), 0.6)
    mat("rope", (0.72, 0.60, 0.42), 0.9)
    mat("fan", (0.92, 0.90, 0.85), 0.4)
    # Stylised fire in layers: a deep orange, half see-through outer flame round a hot yellow core.
    mat("flame", (1.0, 0.38, 0.05), 0.5, glow=3.0, alpha=0.55)
    mat("flame_core", (1.0, 0.86, 0.35), 0.5, glow=6.0)
    mat("marigold", (0.99, 0.52, 0.04), 0.7)
    mat("marigold2", (0.99, 0.76, 0.08), 0.7)
    mat("lemon", (0.95, 0.88, 0.20), 0.5)
    mat("chilli", (0.15, 0.55, 0.15), 0.5)
    rig, movers = jugaad()
    static = auto() + rig + garlands()
    finish(static + movers, "jugaad")
    bpy.ops.object.select_all(action="DESELECT")
    for o in static:
        o.select_set(True)
    bpy.context.view_layer.objects.active = static[0]
    bpy.ops.object.join()
    body = bpy.context.view_layer.objects.active
    body.name = "jugaad_LOD0"
    for o in [body] + movers:
        export.triangulate(o)
    tris = export.export_glb([body] + movers, os.path.join(OUT, "jugaad.glb"))
    print(f"[jugaad] {sum(tris.values())} triangles: {tris}")


main()

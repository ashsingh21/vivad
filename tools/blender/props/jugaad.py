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

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import export  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "models", "props")
S = 7.0
DECK = 0.45  # the passenger floor's height on a real auto

MATS = {}


def mat(name, rgb, rough=0.6, metal=0.0, glow=0.0):
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

def auto():
    parts = []
    add = parts.append
    # The tub: a rounded rear body and a nose that narrows to the front wheel.
    add(hull("tub", [Vector((x, y, z)) for x in (-0.68, 0.68) for y in (-1.22, 0.25) for z in (0.32, 0.98)], "green", 0.09))
    add(hull("nose", [Vector((x * w, y, z)) for (y, w, z0, z1) in ((0.2, 0.62, 0.32, 1.0), (0.9, 0.42, 0.36, 1.02), (1.32, 0.26, 0.5, 0.98))
                      for x in (-1, 1) for z in (z0, z1)], "green", 0.08))
    add(box("floor", (-0.64, -1.18, DECK - 0.04), (0.64, 0.6, DECK), "dark", 0.01))
    # A chrome strip and a yellow band round the tub.
    add(box("band", (-0.70, -1.24, 0.60), (0.70, 0.22, 0.68), "yellow", 0.02))
    add(box("skirt", (-0.71, -1.25, 0.30), (0.71, 0.24, 0.36), "chrome", 0.015))
    # The canopy: a yellow roof, rounded, and its black canvas back.
    roof = [Vector((x, y, z)) for x in (-0.70, 0.70) for y in (-1.28, 0.62) for z in (1.62, 1.70)]
    roof += [Vector((x, y, 1.80)) for x in (-0.5, 0.5) for y in (-1.15, 0.45)]
    add(hull("roof", roof, "yellow", 0.06))
    add(box("canvas", (-0.68, -1.24, 0.95), (0.68, -1.16, 1.64), "dark", 0.02))
    for x in (-0.66, 0.66):
        add(cyl(f"pillar_f{x}", (x, 0.55, 0.98), (x, 0.55, 1.64), 0.025, "dark", 8))
        add(cyl(f"pillar_m{x}", (x, -0.3, 0.98), (x, -0.3, 1.64), 0.025, "dark", 8))
    # Windscreen in a black frame, the handlebar behind it, a single headlight on the nose.
    add(hull("screen", [Vector((x, y, z)) for x in (-0.55, 0.55) for (y, z) in ((0.70, 1.02), (0.74, 1.02), (0.60, 1.58), (0.64, 1.58))], "glass", 0.0))
    add(box("screen_top", (-0.60, 0.58, 1.56), (0.60, 0.66, 1.63), "dark", 0.01))
    add(cyl("bar", (-0.32, 0.55, 1.10), (0.32, 0.55, 1.10), 0.02, "chrome", 8))
    add(cyl("headlight_shell", (0, 1.28, 0.98), (0, 1.40, 0.98), 0.11, "chrome", 16, 0.01))
    add(cyl("headlight", (0, 1.40, 0.98), (0, 1.41, 0.98), 0.09, "lamp", 16))
    add(box("seat", (-0.62, -1.14, DECK), (0.62, -0.62, 0.72), "seat", 0.05))
    add(box("seat_back", (-0.62, -1.15, 0.72), (0.62, -1.02, 1.12), "seat", 0.05))
    add(box("driver_seat", (-0.25, 0.05, DECK), (0.25, 0.45, 0.75), "seat", 0.05))
    # Wheels: one at the front under a mudguard, two at the back.
    for name, (x, y) in (("wheel_f", (0.0, 1.08)), ("wheel_l", (-0.66, -0.78)), ("wheel_r", (0.66, -0.78))):
        add(cyl(name, (x - 0.06, y, 0.22), (x + 0.06, y, 0.22), 0.22, "tyre", 20, 0.02))
        add(cyl(name + "_hub", (x - 0.075, y, 0.22), (x + 0.075, y, 0.22), 0.09, "chrome", 12))
    add(hull("mudguard", [Vector((x, y, z)) for x in (-0.1, 0.1) for (y, z) in ((0.84, 0.36), (1.08, 0.50), (1.30, 0.40))], "green", 0.02))
    # The back in truck art: bands of colour, tail lights, HORN OK PLEASE.
    for k, (c, z) in enumerate((("red", 0.36), ("yellow", 0.46), ("blue", 0.56), ("pink", 0.66), ("green_lt", 0.76))):
        add(box(f"stripe_{k}", (-0.6, -1.26, z), (0.6, -1.23, z + 0.08), c, 0.0))
    for x in (-0.5, 0.5):
        add(cyl(f"tail{x}", (x, -1.24, 0.88), (x, -1.29, 0.88), 0.06, "tail", 12))
    add(text("horn", "HORN OK PLEASE", (0, -1.27, 1.32), 0.17, "white"))
    add(text("sign_l", "UDTA AUTO", (-0.715, -0.5, 0.82), 0.16, "white", rot=(math.pi / 2, 0, -math.pi / 2)))
    add(text("sign_r", "UDTA AUTO", (0.715, -0.5, 0.82), 0.16, "white", rot=(math.pi / 2, 0, math.pi / 2)))
    return parts


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
        movers.append(cone(f"flame_{i}", nozzle, nozzle + Vector((0, -0.75, 0)), 0.10, 0.0, "flame", 12, origin=nozzle))
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
    mat("flame", (1.0, 0.55, 0.10), 0.5, glow=12.0)
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

"""Panga's street fights: the lathi, the wrestler's gada, the tawa and the hockey stick, lathed and
lofted like the bat (desi.py); the talwar, our old straight sword (old_sword.py) with its blade
bent; and the dhal, a museum scan repainted plain (docs/references.md).

Axes as the bat: origin where the hand closes, the long axis Blender +Z (glTF +Y), the striking
face toward Blender +Y. The dhal's face looks along Blender +X: the hand is behind its middle.

The dhal needs research/fights-sounds/models/ (not in git; see its CREDIT.txt).

blender -b --factory-startup -P tools/blender/weapons/melee.py [-- name ...]
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector  # noqa: E402

import arsenal as A  # noqa: E402
import desi as D  # noqa: E402
import export  # noqa: E402
import finishes as F  # noqa: E402
import geo  # noqa: E402

X, Y, Z = Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1))

RUST = (0.30, 0.16, 0.08)
# The dhal stands this far out from the fist, so the hand is behind it.
HAND_BEHIND = 0.07
DHAL_SCAN = os.path.join(A.ROOT, "research", "fights-sounds", "models", "sketchfab_via_objaverse",
                         "indian_dhal_shield_19th_century_fca108", "model.glb")


def bamboo_profile(lo, hi, r_lo, r_hi, nodes):
    """A stick from `lo` to `hi`, thinning from r_lo to r_hi, swelling a little at each node."""
    pts = [(0.0, lo)]
    for k in range(nodes + 1):
        z = lo + (hi - lo) * k / nodes
        r = r_lo + (r_hi - r_lo) * k / nodes
        pts += [(r, z - 0.006), (r + 0.0028, z), (r, z + 0.006)]
    return pts[:1] + [p for p in pts[1:] if lo <= p[1] <= hi] + [(0.0, hi)]


def lathi():
    """A chowkidar's plain bamboo stick, taller than the hip it stands at: cloth on the grip, brass
    on the far end."""
    cane = F.plain("lathi_bamboo", (0.60, 0.44, 0.18), rough=0.5)
    cloth = F.wrap("lathi_grip", (0.17, 0.11, 0.07), pitch=0.009)
    brass = F.metal("lathi_ferrule", (0.55, 0.40, 0.18), rough=0.4, tarnish=(0.15, 0.10, 0.05))
    parts = [A.lathe("cane", bamboo_profile(-0.40, 1.15, 0.0165, 0.0135, 7), cane, seg=12)]
    parts.append(A.lathe("grip", [(0.0, -0.13), (0.0178, -0.13), (0.0192, -0.12), (0.0186, 0.0), (0.0185, 0.12), (0.0165, 0.13), (0.0, 0.13)],
                         cloth, seg=12))
    parts.append(A.lathe("ferrule", [(0.0, 1.10), (0.0165, 1.10), (0.0168, 1.16), (0.012, 1.172), (0.0, 1.172)], brass, seg=12))
    A.finish("lathi", parts, res=512)


def gada():
    """The akhara's plain gada: a round stone on a metre of bamboo. No gold, no paint."""
    stone = F.plain("gada_stone", (0.42, 0.41, 0.39), rough=0.85)
    cane = F.plain("gada_bamboo", (0.50, 0.36, 0.17), rough=0.55)
    iron = F.metal("gada_collar", (0.17, 0.17, 0.18), rough=0.55, tarnish=RUST)
    cloth = F.wrap("gada_grip", (0.22, 0.14, 0.08), pitch=0.009)
    parts = [A.lathe("shaft", bamboo_profile(-0.30, 0.62, 0.021, 0.024, 3), cane, seg=12)]
    parts.append(A.lathe("grip", [(0.0, -0.16), (0.0235, -0.16), (0.0245, -0.15), (0.024, 0.12), (0.022, 0.13), (0.0, 0.13)], cloth, seg=12))
    parts.append(A.lathe("collar", [(0.0, 0.56), (0.034, 0.56), (0.036, 0.57), (0.036, 0.615), (0.03, 0.625), (0.0, 0.625)], iron, seg=14))
    # The stone: a ball, a touch heavier toward the end of the stick.
    R, c, n = 0.135, 0.735, 14
    ball = [(0.0, c - R * 0.95)]
    for k in range(1, n):
        a = math.pi * k / n
        ball.append((R * math.sin(a) * (1.0 + 0.06 * math.cos(a) * -1), c - R * 0.95 * math.cos(a)))
    ball.append((0.0, c + R * 0.95))
    parts.append(A.lathe("stone", ball, stone, seg=18))
    A.finish("gada", parts, res=512)


def tawa():
    """The iron griddle off a dhaba's stove: a shallow dish, a flat strap, a wooden handle."""
    iron = F.metal("tawa_iron", (0.055, 0.052, 0.05), rough=0.5, tarnish=(0.17, 0.10, 0.06))
    wood = F.plain("tawa_wood", (0.25, 0.14, 0.06), rough=0.6)
    R, c = 0.145, Vector((0, 0, 0.235))
    dish = [(0.0, -0.007), (R * 0.55, -0.0065), (R * 0.85, -0.002), (R, 0.009), (R + 0.002, 0.012), (R - 0.002, 0.0135),
            (R * 0.85, 0.002), (R * 0.55, -0.0035), (0.0, -0.004)]
    parts = [A.lathe("dish", dish, iron, seg=32, axis=Y, origin=c)]
    parts.append(D.box("strap", (-0.011, -0.0055, 0.05), (0.011, -0.0015, 0.1), iron))
    parts.append(D.rod("handle", (0, -0.0035, -0.085), (0, -0.0035, 0.065), 0.0145, wood))
    A.finish("tawa", parts, res=512)


def hockey_stick():
    """A school team's stick: a hooked toe, tape on the handle, two yellow bands, no maker's name."""
    paint = F.plain("stick_paint", (0.03, 0.12, 0.27), rough=0.4)
    band = F.plain("stick_band", (0.85, 0.60, 0.08), rough=0.4)
    tape = F.wrap("stick_tape", (0.07, 0.07, 0.08), pitch=0.013)
    ctrl = [(0, 0, -0.24), (0, 0, 0.05), (0, 0, 0.35), (0, 0.0, 0.58), (0, 0.012, 0.655), (0, 0.05, 0.712), (0, 0.098, 0.722), (0, 0.128, 0.69)]
    pts = geo.resample_even(geo.catmull_rom([Vector(p) for p in ctrl], 24), 34)
    parts = [A.tube("shaft", pts, lambda t: 0.0195 + 0.006 * t, paint, seg=10, caps=("fan", "fan"), up=X, rx=lambda t: 0.014 - 0.002 * t)]
    parts.append(A.lathe("tape", [(0.0, -0.245), (0.021, -0.245), (0.0225, -0.235), (0.0225, 0.14), (0.0205, 0.15), (0.0, 0.15)], tape, seg=12))
    for k, z in enumerate((0.40, 0.46)):
        parts.append(A.lathe(f"band{k}", [(0.0, z), (0.0212, z), (0.0212, z + 0.03), (0.0, z + 0.03)], band, seg=12))
    A.finish("hockey_stick", parts, res=512)


def imported(path):
    """The one big mesh of a GLB, its parents' transforms baked in, the rest of the file gone."""
    bpy.ops.import_scene.gltf(filepath=path)
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    obj = max(meshes, key=lambda o: len(o.data.polygons))
    world = obj.matrix_world.copy()
    for o in list(bpy.data.objects):
        if o is not obj:
            bpy.data.objects.remove(o)
    obj.parent = None
    obj.matrix_world = world
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    obj.name = "imported_LOD0"
    return obj


def lighter(obj, target):
    """Down to about `target` triangles."""
    export.triangulate(obj)
    have = len(obj.data.polygons)
    if have > target:
        d = obj.modifiers.new("Decimate", "DECIMATE")
        d.decimate_type = "COLLAPSE"
        d.ratio = target / have
        d.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier=d.name)


def shrink_images(obj, size):
    for mat in obj.data.materials:
        for n in mat.node_tree.nodes:
            if n.type == "TEX_IMAGE" and n.image and max(n.image.size) > size:
                n.image.scale(size, size)


def talwar():
    """The old sword (old_sword.py) bent to a talwar's curve, edge on the outside of the arc."""
    obj = imported(os.path.join(A.OUT, "old_sword.glb"))
    me = obj.data
    z0, R = 0.058, 1.9
    thetas = [max(0.0, v.co.z - z0) / R for v in me.vertices]
    normals = [Vector(c.vector) for c in me.corner_normals]
    for v, a in zip(me.vertices, thetas):
        if a > 0.0:
            y = v.co.y
            v.co.y = -R * (1.0 - math.cos(a)) + y * math.cos(a)
            v.co.z = z0 + R * math.sin(a) + y * math.sin(a)
    # The blade's normals turn with it.
    turned = []
    for loop, n in zip(me.loops, normals):
        a = thetas[loop.vertex_index]
        c, s = math.cos(a), math.sin(a)
        turned.append((n.x, n.y * c - n.z * s, n.y * s + n.z * c))
    me.normals_split_custom_set(turned)
    lighter(obj, 7000)
    shrink_images(obj, 1024)
    obj.name = "talwar_LOD0"
    counts = export.export_glb(export.make_lods(obj, (0.35,)), os.path.join(A.OUT, "talwar.glb"))
    print(f"[talwar] exported tris: {counts}")


def plain_face(img, rgb_for):
    """Rewrites the painted front of the dhal scan, a circle on its texture: `rgb_for` gets the
    pixels and each pixel's (d, inside a boss) and returns the new pixels. Bosses stay as scanned."""
    w, h = img.size
    px = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(h, w, 4)
    # Measured on the texture, image y down, in fractions of its size.
    cx, cy, radius = 0.654, 0.336, 0.326
    yy, xx = np.mgrid[0:h, 0:w]
    u, v = (xx + 0.5) / w, 1.0 - (yy + 0.5) / h
    d = np.hypot(u - cx, v - cy) / radius
    bosses = [(0.578, 0.244), (0.742, 0.232), (0.584, 0.398), (0.748, 0.388)]
    boss = np.zeros_like(d, dtype=bool)
    for bx, by in bosses:
        boss |= np.hypot(u - bx, v - by) < 0.046
    face = d < 1.0
    new = rgb_for(px[..., :3].copy(), d, boss)
    px[..., :3][face & ~boss] = new[face & ~boss]
    img.pixels.foreach_set(px.reshape(-1))
    img.update()
    img.pack()


def dhal():
    """A 19th century dhal: its round form and steel bosses kept, its painted lions and sun face
    painted out, red lacquer and a plain brass boss and rim in their place."""
    obj = imported(DHAL_SCAN)
    me = obj.data
    co = np.array([v.co for v in me.vertices])
    lo, hi = co.min(0), co.max(0)
    size = hi - lo
    axis = int(np.argmin(size))
    # The hand goes behind the middle, the dome faces +X.
    dome_up = (co[:, axis].mean() - (lo[axis] + hi[axis]) / 2) < 0.0
    print(f"[dhal] size {size}, dome axis {axis}, dome toward {'+' if dome_up else '-'}")
    scale = 0.46 / max(size)
    mid = (lo + hi) / 2
    flip = 1.0 if dome_up else -1.0
    rest = [a for a in range(3) if a != axis]
    for v in me.vertices:
        p = (Vector(v.co) - Vector(mid)) * scale
        v.co = Vector((p[axis] * flip + HAND_BEHIND, p[rest[0]], p[rest[1]]))
    # The turn above may mirror the mesh: the winding must follow it.
    me.update()
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.normals_make_consistent(inside=False)
    bpy.ops.object.mode_set(mode="OBJECT")
    if me.has_custom_normals:
        bpy.ops.mesh.customdata_custom_splitnormals_clear()
    bpy.ops.object.shade_smooth()

    mat = obj.data.materials[0]
    # The scan glows with its own colour: a shield in a lane does not.
    for link in list(mat.node_tree.links):
        if link.to_socket.name.startswith("Emission"):
            mat.node_tree.links.remove(link)
    # So does its varnish: the maps below say how shiny it is, not these factors.
    for n in mat.node_tree.nodes:
        if n.type == "MATH":
            n.inputs[1].default_value = 1.0
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Emission Strength"].default_value = 0.0
    bsdf.inputs["Emission Color"].default_value = (0, 0, 0, 1)
    base = next(l.from_node for l in mat.node_tree.links if l.to_socket.name == "Base Color")
    base = base if base.type == "TEX_IMAGE" else next(n for n in mat.node_tree.nodes if n.type == "TEX_IMAGE")

    def paint(px, d, boss):
        grain = np.random.default_rng(7).normal(0.0, 0.012, d.shape)[..., None]
        red = np.array([0.20, 0.012, 0.018]) + grain
        brass = np.array([0.48, 0.30, 0.08]) + grain
        out = np.broadcast_to(red, px.shape).copy()
        ring = (d > 0.90) | (d < 0.20)
        out[ring] = np.broadcast_to(brass, px.shape)[ring]
        groove = (np.abs(d - 0.90) < 0.012) | (np.abs(d - 0.20) < 0.012)
        out[groove] *= 0.35
        return out

    def flat(px, d, boss):
        # Tangent-space up: the carving goes with the paint.
        return np.broadcast_to(np.array([0.5, 0.5, 1.0]), px.shape).copy()

    def worn(px, d, boss):
        # Metal-roughness, by the same masks: lacquer is rough dielectric, brass a dull metal.
        out = np.broadcast_to(np.array([0.0, 0.55, 0.0]), px.shape).copy()
        ring = (d > 0.90) | (d < 0.20)
        out[ring] = np.array([0.0, 0.45, 0.85])
        return out

    plain_face(base.image, paint)
    for n in mat.node_tree.nodes:
        if n.type == "NORMAL_MAP":
            src = n.inputs["Color"].links[0].from_node
            plain_face(src.image, flat)
    rough = bsdf.inputs["Roughness"].links
    if rough:
        node = rough[0].from_node
        while node.type != "TEX_IMAGE":
            node = node.inputs[0].links[0].from_node
        plain_face(node.image, worn)
    shrink_images(obj, 512)
    lighter(obj, 2800)
    obj.name = "dhal_LOD0"
    counts = export.export_glb(export.make_lods(obj, (0.35,)), os.path.join(A.OUT, "dhal.glb"))
    print(f"[dhal] exported tris: {counts}")


BUILDERS = {"lathi": lathi, "gada": gada, "tawa": tawa, "hockey_stick": hockey_stick, "talwar": talwar, "dhal": dhal}


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else list(BUILDERS)
    for name in names:
        export.reset_scene()
        BUILDERS[name]()


if __name__ == "__main__":
    main()

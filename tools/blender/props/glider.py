"""Patang glider: the kite every player opens under them after jumping from the vimana. A Delhi
patang (fighter kite) grown to 4.4 m: a paper diamond billowing up in the middle, split in two
colours across its wings with a round "chand" in the middle, on a bamboo spine and a bow, a
paper tassel at its tail and two ribbons streaming from its wing tips; six strings come down to a
bamboo bar the player holds.

Flat colours, no textures (the polished stylised look): four colourways, `patang_glider_{a..d}.glb`.

The origin is the player's feet; the kite flies toward Blender +Y (game -Z, the way the player
faces); its wing tips are ~2.9 m up, the bar 2.05 m.

blender -b --factory-startup -P tools/blender/props/glider.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import export  # noqa: E402
import geo  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "models", "props")

# The diamond's corners: left tip, nose, right tip, tail (the bow sits a little behind the middle).
Z0 = 2.9
LEFT, NOSE, RIGHT, TAIL = Vector((-2.2, -0.15, Z0)), Vector((0, 1.55, Z0 + 0.1)), Vector((2.2, -0.15, Z0)), Vector((0, -1.95, Z0 + 0.05))
BILLOW = 0.42
GRID = 26
BAR = (2.05, 0.95, 0.15)  # height, width, forward offset

# (wing front, wing back, chand, tassel): festival colours.
COLOURWAYS = {
    "a": ((0.90, 0.12, 0.46), (0.99, 0.70, 0.08), (0.97, 0.95, 0.90), (0.99, 0.70, 0.08)),
    "b": ((0.04, 0.46, 0.62), (0.18, 0.66, 0.24), (0.99, 0.70, 0.08), (0.90, 0.12, 0.46)),
    "c": ((0.86, 0.12, 0.10), (0.97, 0.95, 0.90), (0.04, 0.46, 0.62), (0.86, 0.12, 0.10)),
    "d": ((0.46, 0.16, 0.58), (0.99, 0.82, 0.10), (0.90, 0.12, 0.46), (0.18, 0.66, 0.24)),
}


def material(name, rgb, rough=0.7):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Base Color"].default_value = (*[c ** 2.2 for c in rgb], 1.0)
    p.inputs["Roughness"].default_value = rough
    m.use_backface_culling = False
    return m


def surface(s, t):
    """A point of the canopy: a bilinear patch over the four corners (s from the left tip toward
    the nose, t toward the tail), lifted in a dome toward the middle."""
    a = LEFT.lerp(NOSE, s)
    b = TAIL.lerp(RIGHT, s)
    p = a.lerp(b, t)
    p.z += BILLOW * math.sin(math.pi * s) * math.sin(math.pi * t)
    return p


def canopy(mats):
    """The paper: front wings one colour, back wings the other (split along the bow), a round
    chand over the middle. One sheet, drawn from both sides."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    verts = [[bm.verts.new(surface(i / GRID, j / GRID)) for j in range(GRID + 1)] for i in range(GRID + 1)]
    for i in range(GRID):
        for j in range(GRID):
            f = bm.faces.new((verts[i][j], verts[i + 1][j], verts[i + 1][j + 1], verts[i][j + 1]))
            s, t = (i + 0.5) / GRID, (j + 0.5) / GRID
            # The bow runs from the left tip (0, 0) to the right tip (1, 1).
            front = s > t
            chand = math.hypot(s - 0.5, t - 0.5) < 0.17
            f.material_index = 2 if chand else (0 if front else 1)
            for loop in f.loops:
                loop[uv].uv = (s, t)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    for f in bm.faces:
        if f.normal.z < 0:
            f.normal_flip()
    return obj_from("canopy", bm, mats)


def tube(bm, pts, r, sides=6):
    """A thin round tube along a polyline (bamboo, string)."""
    rings = []
    for k, p in enumerate(pts):
        d = (pts[min(k + 1, len(pts) - 1)] - pts[max(k - 1, 0)]).normalized()
        side = d.cross(Vector((0, 0, 1)))
        if side.length < 1e-3:
            side = d.cross(Vector((1, 0, 0)))
        side.normalize()
        up = side.cross(d).normalized()
        rings.append([bm.verts.new(p + (side * math.cos(a) + up * math.sin(a)) * r) for a in (2 * math.pi * q / sides for q in range(sides))])
    for r0, r1 in zip(rings, rings[1:]):
        for q in range(sides):
            bm.faces.new((r0[q], r0[(q + 1) % sides], r1[(q + 1) % sides], r1[q]))


def along(f, n=16, drop=0.035):
    """Points along a canopy line `f(k)` for k in 0..1, just under the paper."""
    return [surface(*f(k / n)) - Vector((0, 0, drop)) for k in range(n + 1)]


def frame(mats):
    """The spine nose to tail, the bow tip to tip (bent back a little more, as a patang's is),
    and the bar with its six strings."""
    bm = bmesh.new()
    tube(bm, along(lambda k: (1 - k, k)), 0.028)
    tube(bm, along(lambda k: (k, k)), 0.024)
    h, w, fwd = BAR
    bar_l, bar_r = Vector((-w / 2, fwd, h)), Vector((w / 2, fwd, h))
    tube(bm, [bar_l - Vector((0.08, 0, 0)), bar_r + Vector((0.08, 0, 0))], 0.022)
    return obj_from("frame", bm, mats)


def strings(mats):
    bm = bmesh.new()
    h, w, fwd = BAR
    bar_l, bar_r = Vector((-w / 2, fwd, h)), Vector((w / 2, fwd, h))
    lift = Vector((0, 0, -0.04))
    for top, low in ((surface(0.04, 0.04), bar_l), (surface(0.96, 0.96), bar_r), (surface(0.75, 0.25), bar_l), (surface(0.75, 0.25), bar_r),
                     (surface(0.3, 0.7), bar_l), (surface(0.3, 0.7), bar_r)):
        tube(bm, [top + lift, low], 0.006, sides=4)
    return obj_from("strings", bm, mats)


def tail(mats):
    """A paper tassel off the tail and a ribbon off each wing tip, both curling as they trail."""
    bm = bmesh.new()
    t = TAIL + Vector((0, 0, -0.02))
    a, b, c = bm.verts.new(t), bm.verts.new(t + Vector((-0.32, -0.55, -0.06))), bm.verts.new(t + Vector((0.32, -0.55, -0.06)))
    bm.faces.new((a, b, c)).material_index = 0
    for tip, side in ((LEFT, -1), (RIGHT, 1)):
        prev = None
        for k in range(9):
            u = k / 8
            p = tip + Vector((side * 0.15 * u, -1.7 * u, -0.25 * u + 0.08 * math.sin(u * 7)))
            pair = (bm.verts.new(p + Vector((-0.045, 0, 0))), bm.verts.new(p + Vector((0.045, 0, 0))))
            if prev:
                bm.faces.new((prev[0], prev[1], pair[1], pair[0])).material_index = 1
            prev = pair
    return obj_from("tails", bm, mats)


def obj_from(name, bm, mats):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for m in mats:
        me.materials.append(m)
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    for p in me.polygons:
        p.use_smooth = True
    return o


def main():
    os.makedirs(OUT, exist_ok=True)
    for key, (front, back, chand, tassel) in COLOURWAYS.items():
        export.reset_scene()
        paper = [material(f"paper_front_{key}", front), material(f"paper_back_{key}", back), material(f"chand_{key}", chand)]
        bamboo = material("bamboo", (0.80, 0.64, 0.38), 0.55)
        thread = material("thread", (0.96, 0.94, 0.88), 0.8)
        trail = [material(f"tassel_{key}", tassel), material(f"ribbon_{key}", front)]
        parts = [canopy(paper), frame([bamboo]), strings([thread]), tail(trail)]
        glider = geo.join(parts, f"patang_glider_{key}_LOD0")
        export.triangulate(glider)
        path = os.path.join(OUT, f"patang_glider_{key}.glb")
        tris = export.export_glb([glider], path)
        print(f"[glider] {path}: {tris}")


main()

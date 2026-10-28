"""Village and bazaar kit: thatched mud huts (mud-plaster and brick) and market stalls.

Conventions as the rest of the India kit: Blender X = game X, Blender -Y = game +Z (the front,
where the door / counter faces), Z up, origin on the ground at the footprint centre.

The hut matches `india.rs::hut()` at its base size (HUT_W x HUT_D, walls 2.8 m, 0.4 m thick,
door centred on the front, window on the +X wall); the game scales it in X/Z per hut. The stall
matches `india.rs::stall()` (posts at +-1.6 / +-1.2, counter 3.0 x 0.9 x 1.6).

blender -b --factory-startup -P tools/blender/architecture/vernacular.py
"""

import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Vector, noise  # noqa: E402

import export  # noqa: E402
import geo  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402

OUT = os.path.join(T.ROOT, "assets", "models", "props")

HUT_W, HUT_D, HUT_H, HUT_T = 5.2, 4.5, 2.8, 0.4
RIDGE_Z = 4.35       # top of the thatch along the ridge
EAVE_Z = 2.55        # underside of the thatch at the eave
EAVE_OUT = 0.6       # eave overhang beyond the wall face
GABLE_OUT = 0.45     # thatch overhang beyond the gable walls
MUD_TILE = 4.5       # the mud texture tile (its geru dado fills the bottom 15 %)


def mats():
    texgen.mudwall(T.TEX)
    texgen.brick(T.TEX)
    texgen.thatch(T.TEX)
    texgen.wood(T.TEX)
    texgen.cloth(T.TEX, "cloth_saffron", [(0.93, 0.50, 0.08), (0.93, 0.86, 0.70)], stripes=6)
    texgen.cloth(T.TEX, "cloth_red", [(0.70, 0.10, 0.10), (0.92, 0.88, 0.80), (0.70, 0.10, 0.10), (0.20, 0.35, 0.20)], stripes=10)
    texgen.cloth(T.TEX, "cloth_indigo", [(0.12, 0.18, 0.45), (0.85, 0.85, 0.82)], stripes=4)
    T.MATS.update({
        "mud": T.tex_material("mudwall", "mudwall"),
        "brick": T.tex_material("brick", "brick"),
        "thatch": T.tex_material("thatch", "thatch"),
        "wood": T.tex_material("wood", "wood"),
        "cloth_saffron": T.tex_material("cloth_saffron", "cloth_saffron"),
        "cloth_red": T.tex_material("cloth_red", "cloth_red"),
        "cloth_indigo": T.tex_material("cloth_indigo", "cloth_indigo"),
        "terracotta": T.flat_material("terracotta", (0.34, 0.10, 0.045), 0.0, 0.85),
        "brass": T.flat_material("brass", (0.80, 0.50, 0.17), 1.0, 0.35),
        "bamboo": T.flat_material("bamboo", (0.33, 0.22, 0.09), 0.0, 0.6),
        "wicker": T.flat_material("wicker", (0.24, 0.13, 0.05), 0.0, 0.9),
        "haldi": T.flat_material("haldi", (0.80, 0.40, 0.01), 0.0, 0.95),
        "mirch": T.flat_material("mirch", (0.40, 0.012, 0.005), 0.0, 0.95),
        "dhania": T.flat_material("dhania", (0.15, 0.13, 0.03), 0.0, 0.95),
        "jute": T.flat_material("jute", (0.38, 0.25, 0.11), 0.0, 0.95),
        "silk_a": T.flat_material("silk_a", (0.52, 0.01, 0.07), 0.0, 0.5),
        "silk_b": T.flat_material("silk_b", (0.01, 0.17, 0.14), 0.0, 0.5),
        "silk_c": T.flat_material("silk_c", (0.79, 0.34, 0.01), 0.0, 0.5),
    })
    for k in ("cloth_saffron", "cloth_red", "cloth_indigo"):
        T.MATS[k].use_backface_culling = False


def proj_uvs(obj, tile, v0=0.0):
    """Box-project metric UVs by face normal: vertical faces get (along-face, z), tops get (x, y)."""
    me = obj.data
    uv = me.uv_layers[0] if me.uv_layers else me.uv_layers.new(name="UVMap")
    for p in me.polygons:
        n = p.normal
        for li in p.loop_indices:
            c = me.vertices[me.loops[li].vertex_index].co
            if abs(n.z) > 0.7:
                uv.data[li].uv = (c.x / tile, c.y / tile)
            elif abs(n.x) > abs(n.y):
                uv.data[li].uv = (c.y / tile, (c.z + v0) / tile)
            else:
                uv.data[li].uv = (c.x / tile, (c.z + v0) / tile)


def apply_xform(o):
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    o.select_set(False)
    return o


def rounded_rect(hx, hy, r, per=3):
    pts = []
    for cx, cy, a0 in ((hx - r, hy - r, 0.0), (-hx + r, hy - r, 0.5 * math.pi), (-hx + r, -hy + r, math.pi), (hx - r, -hy + r, 1.5 * math.pi)):
        for k in range(per + 1):
            a = a0 + 0.5 * math.pi * k / per
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def prism(name, outline, z0, z1, mat):
    """Closed vertical prism of a CCW outline."""
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    lo = [bm.verts.new((x, y, z0)) for x, y in outline]
    hi = [bm.verts.new((x, y, z1)) for x, y in outline]
    n = len(outline)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((lo[i], lo[j], hi[j], hi[i]))
    bm.faces.new(hi)
    bm.faces.new(list(reversed(lo)))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return T.new_obj(name, bm, (mat,))


def cylinder_between(name, a, b, r, mat, seg=8):
    a, b = Vector(a), Vector(b)
    d = b - a
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r, radius2=r, depth=d.length)
    o = T.new_obj(name, bm, (mat,))
    o.rotation_mode = "QUATERNION"
    o.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(d.normalized())
    o.location = (a + b) / 2
    apply_xform(o)
    proj_uvs(o, 1.0)
    return o


# ------------------------------------------------------------------ thatch

def thatch_slope(name, x0, x1, eave, ridge, thick, seed):
    """One thatch slope as a closed slab: top surface lumpy and slightly sagging, the eave edge
    ragged. `eave` / `ridge` are (y, z) of the top surface. UV V runs up the slope."""
    rnd = random.Random(seed)
    off = Vector((rnd.uniform(0, 50), rnd.uniform(0, 50), 0))
    nx, ns = 22, 10
    e, r = Vector((0, *eave)), Vector((0, *ridge))
    up_slope = (r - e).normalized()
    nrm = Vector((0, -up_slope.z, up_slope.y)) if eave[0] < ridge[0] else Vector((0, up_slope.z, -up_slope.y))
    if nrm.z < 0:
        nrm = -nrm
    L = (r - e).length
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    top, bot = [], []
    for i in range(nx + 1):
        x = x0 + (x1 - x0) * i / nx
        rt, rb = [], []
        for j in range(ns + 1):
            s = j / ns
            p = e + (r - e) * s + Vector((x, 0, 0))
            sag = -0.07 * math.sin(math.pi * s) * (1 - (2 * i / nx - 1) ** 2 * 0.5)
            bump = noise.fractal(Vector((x * 0.9, s * L * 0.9, 0)) + off, 0.6, 2.0, 3) * 0.05
            ragged = (noise.noise(Vector((x * 3.0, 0.0, 0.0)) + off) * 0.08) if j == 0 else 0.0
            pt = p + nrm * (sag + bump) - up_slope * ragged
            t = thick * (1.0 - 0.55 * s)
            rt.append(bm.verts.new(pt))
            rb.append(bm.verts.new(pt - nrm * t))
        top.append(rt)
        bot.append(rb)

    def quad(vs, uvs):
        f = bm.faces.new(vs)
        for loop, t in zip(f.loops, uvs):
            loop[uv].uv = t
        return f

    for i in range(nx):
        for j in range(ns):
            u0, u1 = (x0 + (x1 - x0) * i / nx) / 1.5, (x0 + (x1 - x0) * (i + 1) / nx) / 1.5
            v0, v1 = j / ns * L / 1.5, (j + 1) / ns * L / 1.5
            quad((top[i][j], top[i + 1][j], top[i + 1][j + 1], top[i][j + 1]), ((u0, v0), (u1, v0), (u1, v1), (u0, v1)))
            quad((bot[i][j + 1], bot[i + 1][j + 1], bot[i + 1][j], bot[i][j]), ((u0, v1), (u1, v1), (u1, v0), (u0, v0)))
    for i in range(nx):  # eave lip (shows the cut ends of the straw) and the ridge edge
        u0, u1 = i / nx * (x1 - x0) / 1.5, (i + 1) / nx * (x1 - x0) / 1.5
        quad((top[i][0], bot[i][0], bot[i + 1][0], top[i + 1][0]), ((u0, 0.3), (u0, 0.0), (u1, 0.0), (u1, 0.3)))
        quad((top[i + 1][ns], bot[i + 1][ns], bot[i][ns], top[i][ns]), ((u1, 0.1), (u1, 0.0), (u0, 0.0), (u0, 0.1)))
    for i, flip in ((0, False), (nx, True)):
        for j in range(ns):
            vs = (top[i][j], top[i][j + 1], bot[i][j + 1], bot[i][j])
            quad(tuple(reversed(vs)) if flip else vs, ((j / ns, 0.3), ((j + 1) / ns, 0.3), ((j + 1) / ns, 0.0), (j / ns, 0.0)))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    o = T.new_obj(name, bm, ("thatch",))
    for p in o.data.polygons:
        p.use_smooth = True
    return o


def thatch_roof(kit, hx, hy, seed):
    """Gable thatch over walls of half-size hx, hy (ridge along X), with a rolled ridge cap,
    bamboo rafter ends under the eaves and lashing rings."""
    x0, x1 = -hx - GABLE_OUT, hx + GABLE_OUT
    for side in (-1, 1):
        eave = (side * (hy + EAVE_OUT), EAVE_Z + 0.28)
        kit.add(thatch_slope(f"thatch{side}", x0, x1, eave, (0.0, RIDGE_Z), 0.32, seed + side))
    kit.add(cylinder_between("ridge", (x0 - 0.05, 0, RIDGE_Z - 0.05), (x1 + 0.05, 0, RIDGE_Z - 0.05), 0.17, "thatch", 10))
    for k in range(7):
        x = x0 + 0.3 + (x1 - x0 - 0.6) * k / 6
        kit.add(cylinder_between("lash", (x - 0.04, 0, RIDGE_Z - 0.05), (x + 0.04, 0, RIDGE_Z - 0.05), 0.19, "bamboo", 10))
    n = int((x1 - x0) / 0.65)
    for side in (-1, 1):
        for k in range(n + 1):
            x = x0 + 0.15 + (x1 - x0 - 0.3) * k / n
            a = Vector((x, side * (hy + EAVE_OUT + 0.08), EAVE_Z - 0.02))
            b = Vector((x, side * 0.3, RIDGE_Z - 0.35))
            kit.add(cylinder_between("rafter", a, a + (b - a) * 0.45, 0.04, "bamboo", 6))
    # Purlin resting on the gable walls just under the ridge.
    kit.add(cylinder_between("purlin", (x0 + 0.1, 0, RIDGE_Z - 0.42), (x1 - 0.1, 0, RIDGE_Z - 0.42), 0.09, "wood", 8))


# ------------------------------------------------------------------ hut

def hut(name, wall_mat):
    kit = T.Kit()
    hw, hd, t, h = HUT_W / 2, HUT_D / 2, HUT_T, HUT_H
    tile = MUD_TILE if wall_mat == "mud" else 2.0
    # Plinth (chabutra): a low geru-plastered platform, rounded, stepping out under the door.
    plinth = prism("plinth", rounded_rect(hw + t / 2 + 0.35, hd + t / 2 + 0.35, 0.3, 3), -0.2, 0.1, "mud")
    proj_uvs(plinth, MUD_TILE, 0.25)
    kit.add(plinth)
    walls = prism("walls", rounded_rect(hw + t / 2, hd + t / 2, 0.22, 3), 0.0, h, wall_mat)
    for cutter in (
        T.box("inside", (-hw + t / 2, -hd + t / 2, -0.5), (hw - t / 2, hd - t / 2, h + 1)),
        T.box("door", (-0.7, -hd - 1, 0.1), (0.7, -hd + 1, 2.4)),
        T.box("window", (hw - 1, -0.6, 1.0), (hw + 1, 0.6, 2.2)),
    ):
        T.boolean_cut(walls, cutter)
    # Hand-built walls are never true: lean and bulge them a little.
    for v in walls.data.vertices:
        v.co.x += noise.noise(Vector((v.co.y * 0.8, v.co.z * 0.8, 3.1))) * 0.03
        v.co.y += noise.noise(Vector((v.co.x * 0.8, v.co.z * 0.8, 7.7))) * 0.03
    proj_uvs(walls, tile)
    kit.add(walls)
    # Gable walls up to the ridge purlin.
    for sx in (-1, 1):
        bm = bmesh.new()
        bm.loops.layers.uv.new("UVMap")
        x_in, x_out = sx * (hw - t / 2), sx * (hw + t / 2)
        tri = [(-hd - t / 2 + 0.05, h), (hd + t / 2 - 0.05, h), (0.0, RIDGE_Z - 0.4)]
        a = [bm.verts.new((x_in, y, z)) for y, z in tri]
        b = [bm.verts.new((x_out, y, z)) for y, z in tri]
        bm.faces.new(a)
        bm.faces.new(list(reversed(b)))
        for i in range(3):
            j = (i + 1) % 3
            bm.faces.new((a[i], a[j], b[j], b[i]))
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        g = T.new_obj("gable", bm, (wall_mat,))
        proj_uvs(g, tile)
        kit.add(g)
    # Door frame (chaukhat) with a carved lintel and threshold; window frame with bars.
    y_out, y_in = -hd - t / 2 - 0.04, -hd + t / 2 + 0.02
    for sx in (-1, 1):
        kit.add(T.box("jamb", (sx * 0.7 - 0.07, y_out, 0.1), (sx * 0.7 + 0.07, y_in, 2.45), "wood"))
    lintel = T.box("lintel", (-0.95, y_out - 0.04, 2.4), (0.95, y_in, 2.62), "trim")
    kit.add(lintel)
    kit.add(T.box("sill", (-0.75, y_out - 0.08, 0.06), (0.75, y_in, 0.16), "wood"))
    for y in (-0.36, -0.12, 0.12, 0.36):
        kit.add(cylinder_between("wbar", (hw, y, 1.0), (hw, y, 2.2), 0.03, "wood", 6))
    for sy in (-1, 1):  # a shutter folded back against the wall
        kit.add(T.box("shutter", (hw + t / 2, sy * 0.62, 1.02), (hw + t / 2 + 0.05, sy * 1.18, 2.18), "wood"))
    for z0, z1 in ((0.94, 1.02), (2.18, 2.28)):
        kit.add(T.box("wframe", (hw - t / 2 - 0.03, -0.7, z0), (hw + t / 2 + 0.06, 0.7, z1), "wood"))
    thatch_roof(kit, hw + t / 2, hd + t / 2, seed=hash(name) % 97)
    # A water pot (matka) on the plinth by the door, and a smaller one stacked on it.
    for (px, z, s) in ((1.35, 0.1, 1.0), (1.35, 0.62, 0.62)):
        prof = [(0.0, 0), (0.12, 0.01), (0.24, 0.12), (0.27, 0.26), (0.22, 0.42), (0.11, 0.5), (0.13, 0.56), (0.1, 0.57)]
        kit.add(T.lathe("matka", [(r * s, z + zz * s) for r, zz in prof], 16, (px, -hd - t / 2 - 0.22), mat="terracotta"))
    # Hearth in the back corner: three stones and a pot.
    for k in range(3):
        a = k * 2 * math.pi / 3
        kit.add(T.box("hearth", (-hw + 0.9 + 0.22 * math.cos(a) - 0.09, hd - 0.9 + 0.22 * math.sin(a) - 0.09, 0.0),
                      (-hw + 0.9 + 0.22 * math.cos(a) + 0.09, hd - 0.9 + 0.22 * math.sin(a) + 0.09, 0.2), "mud"))
    kit.add(T.lathe("handi", [(0.0, 0.2), (0.16, 0.22), (0.2, 0.32), (0.15, 0.42), (0.16, 0.45)], 14, (-hw + 0.9, hd - 0.9), mat="terracotta"))
    return finish(name, kit)


# ------------------------------------------------------------------ market stall

def awning(name, mat, x0, x1, y_front, y_back, z_front, z_back, valance):
    """Sagging cloth canopy with a scalloped valance along the front edge."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    nx, ny = 16, 8
    grid = []
    for i in range(nx + 1):
        u = i / nx
        x = x0 + (x1 - x0) * u
        col = []
        for j in range(ny + 1):
            v = j / ny
            y = y_front + (y_back - y_front) * v
            z = z_front + (z_back - z_front) * v - 0.12 * math.sin(math.pi * v) * (0.6 + 0.4 * math.sin(math.pi * u))
            z -= 0.05 * math.sin(math.pi * u * 2) ** 2 * math.sin(math.pi * v)
            col.append(bm.verts.new((x, y, z)))
        grid.append(col)
    for i in range(nx):
        for j in range(ny):
            f = bm.faces.new((grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]))
            for loop in f.loops:
                c = loop.vert.co
                loop[uv].uv = (c.x, c.y)
    # Valance: a hanging band with a scalloped hem.
    n = 48
    top, hem = [], []
    for k in range(n + 1):
        x = x0 + (x1 - x0) * k / n
        top.append(bm.verts.new((x, y_front - 0.01, z_front)))
        sc = abs(math.sin(math.pi * k / n * 9))
        hem.append(bm.verts.new((x, y_front - 0.02, z_front - valance + 0.08 * (1 - sc))))
    for k in range(n):
        f = bm.faces.new((hem[k], hem[k + 1], top[k + 1], top[k]))
        for loop in f.loops:
            c = loop.vert.co
            loop[uv].uv = (c.x, c.z * 0.3)
    o = T.new_obj(name, bm, (mat,))
    sol = o.modifiers.new("s", "SOLIDIFY")
    sol.thickness = 0.015
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.modifier_apply(modifier="s")
    for p in o.data.polygons:
        p.use_smooth = True
    return o


def basket(kit, x, y, z, r, fill):
    kit.add(T.lathe("basket", [(r * 0.75, z), (r, z + 0.12 * r / 0.3), (r * 1.05, z + 0.2 * r / 0.3), (r * 0.98, z + 0.22 * r / 0.3)],
                    16, (x, y), ribs=16, rib_depth=0.04, mat="wicker"))
    zt = z + 0.2 * r / 0.3
    kit.add(T.lathe("heap", [(r * 0.98, zt), (r * 0.7, zt + 0.08), (r * 0.35, zt + 0.15), (0.0, zt + 0.18)], 16, (x, y), mat=fill))


def stall(name, cloth_mat, goods, seed):
    rnd = random.Random(seed)
    kit = T.Kit()
    # Bamboo posts, the front pair shorter so the canopy sheds rain toward the street.
    for x in (-1.6, 1.6):
        for y, ztop in ((-1.2, 2.5), (1.2, 2.85)):
            kit.add(cylinder_between("post", (x, y, 0.0), (x, y, ztop), 0.06, "bamboo", 8))
            for zn in (0.7, 1.4, 2.1):
                kit.add(cylinder_between("node", (x, y, zn - 0.02), (x, y, zn + 0.02), 0.068, "bamboo", 8))
    for y, z in ((-1.2, 2.47), (1.2, 2.82)):
        kit.add(cylinder_between("beam", (-1.75, y, z), (1.75, y, z), 0.045, "bamboo", 8))
    kit.add(awning("awning", cloth_mat, -1.9, 1.9, -1.45, 1.45, 2.5, 2.88, 0.28))
    # Takht (counter) with a cloth drape down the front.
    kit.add(T.box("takht", (-1.5, -0.8, 0.78), (1.5, 0.8, 0.9), "wood"))
    for sx in (-1, 1):
        for sy in (-1, 1):
            kit.add(T.box("leg", (sx * 1.4 - 0.06, sy * 0.7 - 0.06, 0.0), (sx * 1.4 + 0.06, sy * 0.7 + 0.06, 0.78), "wood"))
    drape = awning("drape", cloth_mat, -1.52, 1.52, -0.82, -0.82, 0.9, 0.9, 0.82)
    kit.add(drape)
    if goods == "spice":
        fills = ["haldi", "mirch", "dhania", "haldi", "mirch"]
        for k in range(5):
            x = -1.15 + k * 0.575
            basket(kit, x, -0.3 + rnd.uniform(-0.05, 0.05), 0.9, 0.24, fills[k])
        for k in range(3):
            basket(kit, -0.8 + k * 0.8, 0.45, 0.9, 0.2, fills[(k + 2) % 5])
    elif goods == "brass":
        for k in range(7):
            x, y = -1.2 + k * 0.4, -0.35 + (k % 2) * 0.55
            s = rnd.uniform(0.7, 1.15)
            prof = [(0.0, 0.0), (0.14, 0.01), (0.2, 0.1), (0.2, 0.16), (0.1, 0.26), (0.08, 0.32), (0.11, 0.34)]
            kit.add(T.lathe("lota", [(r * s, 0.9 + z * s) for r, z in prof], 16, (x, y), mat="brass"))
        for k in range(3):
            kit.add(T.lathe("thali", [(0.0, 0.9 + k * 0.03), (0.3, 0.9 + k * 0.03), (0.32, 0.92 + k * 0.03)], 20, (0.9, 0.4), mat="brass"))
    else:  # cloth bolts
        for k in range(6):
            m = ("silk_a", "silk_b", "silk_c")[k % 3]
            x = -1.2 + k * 0.48
            kit.add(cylinder_between("bolt", (x, -0.6, 1.02), (x, 0.6, 1.02), 0.12, m, 12))
        for k in range(4):
            m = ("silk_c", "silk_a", "silk_b")[k % 3]
            kit.add(cylinder_between("bolt", (-0.9 + k * 0.6, -0.5, 1.24), (-0.9 + k * 0.6, 0.5, 1.24), 0.11, m, 12))
    # Sacks and a crate behind the counter.
    for k in range(3):
        x = -1.2 + k * 0.7 + rnd.uniform(-0.1, 0.1)
        prof = [(0.0, 0.0), (0.22, 0.02), (0.27, 0.25), (0.24, 0.5), (0.12, 0.6), (0.06, 0.66), (0.0, 0.67)]
        kit.add(T.lathe("sack", prof and [(r, z) for r, z in prof], 12, (x, 1.15), mat="jute"))
    kit.add(T.box("crate", (0.75, 0.95, 0.0), (1.35, 1.4, 0.5), "wood"))
    return finish(name, kit)


# ------------------------------------------------------------------ finishing

def finish(name, kit):
    obj = geo.join(kit.objs, f"{name}_LOD0")
    obj.data.set_sharp_from_angle(angle=math.radians(40))
    for p in obj.data.polygons:
        p.use_smooth = True
    T.bake_weathering(obj)
    export.triangulate(obj)
    return export.make_lods(obj, (0.3,))


def main():
    export.reset_scene()
    texgen.trim_sheet(T.TEX)
    T.MATS["trim"] = T.tex_material("trim", "trim")
    T.MATS["stone"] = T.MATS["trim"]
    mats()
    only = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    pieces = {
        "hut_mud": lambda: hut("hut_mud", "mud"),
        "hut_brick": lambda: hut("hut_brick", "brick"),
        "stall_spice": lambda: stall("stall_spice", "cloth_saffron", "spice", 1),
        "stall_brass": lambda: stall("stall_brass", "cloth_red", "brass", 2),
        "stall_cloth": lambda: stall("stall_cloth", "cloth_indigo", "cloth", 3),
    }
    for key, make in pieces.items():
        if only and key not in only:
            continue
        for o in list(bpy.data.objects):
            bpy.data.objects.remove(o)
        lods = make()
        print(f"[vernacular] {key}: {export.export_glb(lods, os.path.join(OUT, f'{key}.glb'))}")


if __name__ == "__main__":
    main()

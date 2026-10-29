"""Hampi (Vijayanagara) kit: granite boulders, a gopuram, a Dravidian temple, the stone chariot,
and the composite pillar. Same conventions as the temple kit (front toward -Y = glTF +Z).

blender -b --factory-startup -P tools/blender/architecture/hampi.py
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


def mats():
    texgen.sandstone(T.TEX)
    texgen.trim_sheet(T.TEX, tint=(0.66, 0.60, 0.55), streaks=0.8)
    texgen.granite(T.TEX)
    # Weathered lime stucco over brick, grey with age.
    texgen.plaster(T.TEX, name="stucco", tint=(0.66, 0.64, 0.60), grime=0.2, streaks=1.5)
    T.MATS.update({
        "stone": T.tex_material("granite", "granite"),
        "trim": T.tex_material("trim", "trim"),
        "stucco": T.tex_material("stucco", "stucco"),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
        "cloth": T.flat_material("dhvaja", (0.95, 0.45, 0.05), 0.0, 0.8),
    })


def finish(name, kit, lods=(0.4,), smooth=False):
    obj = geo.join(kit.objs, f"{name}_LOD0")
    if not smooth:
        obj.data.set_sharp_from_angle(angle=math.radians(40))
    for p in obj.data.polygons:
        p.use_smooth = True
    T.bake_weathering(obj, streak_k=0.32)
    export.triangulate(obj)
    return export.make_lods(obj, lods) if lods else [obj]


def restucco(obj):
    """Switch an object's 'stone' slot to stucco."""
    for i, m in enumerate(obj.data.materials):
        if m and m.name == "granite":
            obj.data.materials[i] = T.MATS["stucco"]
    return obj


# ------------------------------------------------------------------ boulders

def boulder(seed):
    """Exfoliated granite boulder: a smooth, rounded, slightly lumpy ellipsoid with a flattened
    base (unit radius). Hampi's boulders weather into soft domes, not sharp rocks."""
    rnd = random.Random(seed)
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=4, radius=1.0)
    sx, sy, sz = rnd.uniform(0.9, 1.15), rnd.uniform(0.8, 1.0), rnd.uniform(0.65, 0.85)
    off = Vector((rnd.uniform(0, 100), rnd.uniform(0, 100), rnd.uniform(0, 100)))
    for v in bm.verts:
        d = v.co.normalized()
        n = noise.fractal(d * 1.1 + off, 0.5, 2.0, 3) * 0.12 + noise.noise(d * 3.0 + off) * 0.02
        v.co = Vector((d.x * sx, d.y * sy, d.z * sz)) * (1.0 + n)
        if v.co.z < -0.55 * sz:
            v.co.z = -0.55 * sz + (v.co.z + 0.55 * sz) * 0.2
    uv = bm.loops.layers.uv.new("UVMap")
    for f in bm.faces:
        us = []
        for loop in f.loops:
            c = loop.vert.co
            us.append(math.atan2(c.y, c.x) / math.pi * 3.0)
        # Keep each face on one side of the atan2 seam.
        if max(us) - min(us) > 3.0:
            us = [u + 6.0 if u < 0 else u for u in us]
        for loop, u in zip(f.loops, us):
            loop[uv].uv = (u, loop.vert.co.z * 2.0)
    obj = T.new_obj(f"boulder_{seed}", bm, ("stone",))
    for p in obj.data.polygons:
        p.use_smooth = True
    obj["smooth"] = True
    return obj


# ------------------------------------------------------------------ miniature shrines (kutas, shalas)

def kuta(kit, x, y, z, s, mat="stucco"):
    """Square miniature shrine with a domed roof."""
    kit.add(restucco(T.molded("kuta", [(z, T.rect(0.5 * s, 0.5 * s, x, y), "plain"), (z + 0.7 * s, T.rect(0.5 * s, 0.5 * s, x, y), "dentil"),
                                       (z + 0.8 * s, T.rect(0.58 * s, 0.58 * s, x, y), None)])))
    o = T.lathe("kdome", [(0.48 * s, z + 0.8 * s), (0.5 * s, z + 0.95 * s), (0.36 * s, z + 1.2 * s), (0.12 * s, z + 1.32 * s), (0.0, z + 1.34 * s)],
                12, (x, y), mat=mat)
    kit.add(o)
    kit.add(T.lathe("kfin", [(0.05 * s, z + 1.3 * s), (0.09 * s, z + 1.4 * s), (0.0, z + 1.55 * s)], 8, (x, y), mat=mat))


def shala(kit, x, y, z, length, s, axis="x", mat="stucco"):
    """Oblong miniature shrine with a barrel-vaulted roof (and horseshoe arched gable ends)."""
    hx, hy = (length / 2, 0.45 * s) if axis == "x" else (0.45 * s, length / 2)
    kit.add(restucco(T.molded("shala", [(z, T.rect(hx, hy, x, y), "plain"), (z + 0.7 * s, T.rect(hx, hy, x, y), "dentil"),
                                        (z + 0.8 * s, T.rect(hx + 0.06, hy + 0.06, x, y), None)])))
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    seg = 10
    L = length + 0.08
    r = 0.5 * s
    rows = []
    for k in range(seg + 1):
        a = math.pi * k / seg
        ring = []
        for e in (-1, 1):
            if axis == "x":
                ring.append(bm.verts.new((x + e * L / 2, y + r * math.cos(a), z + 0.8 * s + r * 1.15 * math.sin(a))))
            else:
                ring.append(bm.verts.new((x + r * math.cos(a), y + e * L / 2, z + 0.8 * s + r * 1.15 * math.sin(a))))
        rows.append(ring)
    for k in range(seg):
        bm.faces.new((rows[k][0], rows[k][1], rows[k + 1][1], rows[k + 1][0]))
    for e in (0, 1):
        bm.faces.new([rows[k][e] for k in range(seg + 1)])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    o = T.new_obj("vault", bm, (mat,))
    from vernacular import proj_uvs
    proj_uvs(o, T.TILE)
    kit.add(o)
    # Finials along the ridge.
    n = max(1, int(length / (0.6 * s)))
    for i in range(n):
        t = (i + 0.5) / n - 0.5
        px, py = (x + t * length, y) if axis == "x" else (x, y + t * length)
        kit.add(T.lathe("sfin", [(0.06 * s, z + 1.35 * s), (0.08 * s, z + 1.42 * s), (0.0, z + 1.55 * s)], 6, (px, py), mat=mat))


def tier_parapet(kit, cx, cy, hx, hy, z, s, mat="stucco"):
    """Hara: the parapet of kutas at the corners and shalas along each side that rings every tier."""
    for sx in (-1, 1):
        for sy in (-1, 1):
            kuta(kit, cx + sx * (hx - 0.5 * s), cy + sy * (hy - 0.5 * s), z, s, mat)
    long_len = max(0.0, 2 * hx - 2.6 * s)
    short_len = max(0.0, 2 * hy - 2.6 * s)
    for sy in (-1, 1):
        if long_len > 0.5 * s:
            shala(kit, cx, cy + sy * (hy - 0.45 * s), z, long_len, s, "x", mat)
    for sx in (-1, 1):
        if short_len > 0.5 * s:
            shala(kit, cx + sx * (hx - 0.45 * s), cy, z, short_len, s, "y", mat)


# ------------------------------------------------------------------ gopuram

def gopuram():
    """Gateway tower. Matches the gameplay blocks: granite piers x 2.5..9 each side, depth 6,
    passage 5 wide x 7 high, lintel 7..8; superstructure 16 x 5.5 tapering to ~0.4 over 14 m."""
    kit = T.Kit()
    # Granite base: moulded plinth, walls, the passage cut through with an exact boolean.
    plan = T.rect(9.0, 3.0)
    prof = [(0.0, 0.25, "padma"), (0.45, 0.18, "bead"), (0.65, 0.08, None), (6.6, 0.08, "lata"), (7.1, 0.1, "dentil"),
            (7.5, 0.35, "plain"), (8.0, 0.35, None)]
    base = T.molded("gbase", [(z, T.offset_poly(plan, d), s) for z, d, s in prof])
    T.boolean_cut(base, T.box("passage", (-2.5, -5, -0.1), (2.5, 5, 7.0)))
    T.cube_uvs(base)
    kit.add(base)
    # Pilasters on both facades.
    for side in (-1, 1):
        for x in (-8.0, -5.5, -3.2, 3.2, 5.5, 8.0):
            kit.add(T.box("pil", (x - 0.25, side * 3.08 - 0.12, 0.65), (x + 0.25, side * 3.08 + 0.12, 6.6), "trim"))
    # Superstructure: receding stucco tiers, each ringed by a parapet of miniature shrines.
    z = 8.0
    tiers = 6
    for i in range(tiers):
        t = i / tiers
        hx = 8.0 * (1 - 0.55 * t) - 0.3
        hy = 2.75 * (1 - 0.4 * t) - 0.2
        h = 2.0 - 0.15 * i
        core = restucco(T.molded(f"tier{i}", [(z, T.rect(hx, hy), "gavaksha"), (z + h * 0.5, T.rect(hx, hy), "lata"),
                                               (z + h * 0.85, T.rect(hx + 0.15, hy + 0.15), "dentil"), (z + h, T.rect(hx + 0.15, hy + 0.15), None)]))
        kit.add(core)
        tier_parapet(kit, 0.0, 0.0, hx, hy, z + h, 0.9 - 0.08 * i)
        z += h
    # Crowning shala: a great barrel vault with a row of kalashas and horn finials.
    hx = 8.0 * 0.45 - 0.3
    shala(kit, 0.0, 0.0, z, 2 * hx, 2.6)
    for i in range(7):
        x = -hx * 0.8 + (2 * hx * 0.8) * i / 6
        T.kalasha(kit, (x, 0.0), z + 2.6 * 1.35, 0.55)
    for sx in (-1, 1):
        kit.add(T.lathe("horn", [(0.35, z + 2.2), (0.3, z + 2.8), (0.18, z + 3.4), (0.0, z + 3.9)], 10, (sx * (hx + 0.2), 0.0), mat="stucco"))
    return finish("gopuram", kit)


# ------------------------------------------------------------------ Vijayanagara pillar and Dravidian temple

def vijayanagara_pillar(kit, x, y, z0, h, s=1.0):
    """Composite pillar: square shaft with carved cubical blocks, octagonal bands, and a
    pushpa-potika (flower-bud) bracket capital."""
    w = 0.32 * s
    kit.add(T.box("pb", (x - w - 0.05, y - w - 0.05, z0), (x + w + 0.05, y + w + 0.05, z0 + 0.25 * h / 4)))
    zz = z0 + 0.25 * h / 4
    for k in range(3):
        blk = 0.7 * h / 4
        o = T.box("cube", (x - w, y - w, zz), (x + w, y + w, zz + blk), "trim")
        v0, v1 = texgen.strip_v("lata" if k % 2 == 0 else "padma")
        uvl = o.data.uv_layers[0]
        for loop, d in zip(o.data.loops, uvl.data):
            c = o.data.vertices[loop.vertex_index].co
            d.uv = ((c.x + c.y) / (2 * w) * 0.5, v0 + (v1 - v0) * (c.z - zz) / blk)
        kit.add(o)
        zz += blk
        if k < 2:
            kit.add(T.lathe("oct", [(w * 0.85, zz), (w * 0.8, zz + 0.12 * h / 4), (w * 0.85, zz + 0.25 * h / 4)], 8, (x, y)))
            zz += 0.25 * h / 4
    kit.add(T.lathe("neck", [(w * 0.7, zz), (w * 0.6, zz + 0.15 * h / 4), (w * 1.0, zz + 0.3 * h / 4)], 16, (x, y), uv_strip="padma"))
    zz += 0.3 * h / 4
    top = z0 + h
    kit.add(T.box("abacus", (x - w * 1.2, y - w * 1.2, zz), (x + w * 1.2, y + w * 1.2, zz + 0.12)))
    # Pushpa-potika: drooping bud brackets on four sides.
    for ang in range(4):
        dx, dy = math.cos(ang * math.pi / 2), math.sin(ang * math.pi / 2)
        kit.add(T.box("potika", (x + min(0, dx) * 0.75 - 0.12 * abs(dy), y + min(0, dy) * 0.75 - 0.12 * abs(dx), zz + 0.12),
                      (x + max(0, dx) * 0.75 + 0.12 * abs(dy), y + max(0, dy) * 0.75 + 0.12 * abs(dx), top)))
        kit.add(T.lathe("bud", [(0.0, zz - 0.15), (0.08, zz - 0.05), (0.1, zz + 0.08), (0.06, zz + 0.12)], 8, (x + dx * 0.68, y + dy * 0.68), uv_strip="padma"))


def kapota(kit, hx, hy, cx, cy, z):
    """The deep, curved Vijayanagara eave (kapota) running round a roof: projection grows
    steeply then flattens, like a drooping lip."""
    prof = [(0.0, 0.0, "bead"), (0.08, 0.28, "plain"), (0.18, 0.52, "plain"), (0.3, 0.72, "plain"), (0.42, 0.84, "plain"),
            (0.5, 0.88, None)]
    kit.add(T.molded("kapota", [(z + dz, T.offset_poly(T.rect(hx, hy, cx, cy), d), s) for dz, d, s in prof]))


def dravida_temple():
    """Vijayanagara temple matching `india.rs::temple()` blocks at scale 1 (plinth 14 x 18,
    mandapa pillars, sanctum 9 x 9 centred 4 m behind, tower above)."""
    kit = T.Kit()
    base = T.rect(7.0, 9.0)
    prof = [(0.0, 0.3, "padma"), (0.25, 0.22, "bead"), (0.45, 0.14, "chevron"), (0.75, 0.2, "dentil"), (1.0, 0.05, None), (T.PLINTH, 0.0, None)]
    kit.add(T.molded("djagati", [(z, T.offset_poly(base, d), s) for z, d, s in prof]))
    # Steps as stacked slabs, each only its own rise tall: full-height blocks would overlap one
    # another's sides in the same plane and z-fight.
    for i in range(6):
        kit.add(T.box(f"dstep{i}", (-2.0, -12.0 + i * 0.5, i * T.PLINTH / 6), (2.0, -8.9, (i + 1) * T.PLINTH / 6)))
    # Mandapa on Vijayanagara pillars under a flat roof with a kapota eave and a hara parapet.
    z0 = T.PLINTH
    for x in (-4.5, -1.5, 1.5, 4.5):
        for y in (-2.0, -5.0, -8.0):
            vijayanagara_pillar(kit, x, y, z0, 4.0)
    top = z0 + 4.0
    kit.add(T.molded("droof", [(top, T.rect(5.6, 3.8, 0, -5.0), "lata"), (top + 0.4, T.rect(5.6, 3.8, 0, -5.0), None)]))
    kapota(kit, 5.6, 3.8, 0.0, -5.0, top - 0.3)
    tier_parapet(kit, 0.0, -5.0, 5.2, 3.4, top + 0.4, 0.75, mat="stone")
    # Sanctum: pilastered granite walls with kumbha-panjara niches, hollowed, doorway cut.
    c = (0.0, T.SANCTUM_Y)
    plan = [(x, y + T.SANCTUM_Y) for x, y in T.pancharatha(T.SANCTUM_R, [(2.4, 0.15), (1.2, 0.3)])]
    prof = [(z0, 0.2, "padma"), (z0 + 0.4, 0.12, "bead"), (z0 + 0.55, 0.0, None), (z0 + T.WALL_H - 0.6, 0.0, "lata"),
            (z0 + T.WALL_H - 0.3, 0.05, None), (z0 + T.WALL_H, 0.05, None)]
    shell = T.molded("dsanctum", [(z, T.offset_poly(plan, d), s) for z, d, s in prof])
    T.boolean_cut(shell, T.box("in", (-T.SANCTUM_R + 0.7, T.SANCTUM_Y - T.SANCTUM_R + 0.7, z0), (T.SANCTUM_R - 0.7, T.SANCTUM_Y + T.SANCTUM_R - 0.7, z0 + T.WALL_H - 0.6)))
    T.boolean_cut(shell, T.box("door", (-1.0, T.SANCTUM_Y - T.SANCTUM_R - 2.0, z0 - 0.01), (1.0, T.SANCTUM_Y - T.SANCTUM_R + 1.2, z0 + 3.0)))
    T.cube_uvs(shell)
    kit.add(shell)
    kapota(kit, T.SANCTUM_R + 0.1, T.SANCTUM_R + 0.1, *c, z0 + T.WALL_H - 0.35)
    # Vimana: three receding talas (storeys), each with a hara of kutas and shalas, then the
    # griva (neck) and an octagonal shikhara dome with the stupi (kalasha).
    z = z0 + T.WALL_H + 0.35
    for i, (r, h) in enumerate(((4.4, 2.4), (3.4, 2.1), (2.5, 1.9))):
        kit.add(restucco(T.molded(f"tala{i}", [(z, T.rect(r, r, *c), "gavaksha"), (z + h * 0.8, T.rect(r, r, *c), "dentil"),
                                                (z + h, T.rect(r + 0.12, r + 0.12, *c), None)])))
        tier_parapet(kit, c[0], c[1], r, r, z + h, 0.85 - 0.12 * i)
        z += h
    kit.add(T.lathe("griva", [(1.4, z), (1.2, z + 0.9)], 8, c, mat="stucco"))
    dome = [(1.25, z + 0.9), (1.9, z + 1.3), (2.0, z + 1.8), (1.7, z + 2.5), (1.0, z + 3.0), (0.3, z + 3.2), (0.0, z + 3.22)]
    kit.add(T.lathe("dshikhara", dome, 8, c, mat="stucco"))
    T.kalasha(kit, c, z + 3.2, 0.9)
    return finish("temple_dravida", kit)


# ------------------------------------------------------------------ stone chariot

def stone_chariot():
    """After Vittala's ratha: a moulded granite chassis on four carved stone wheels, a small
    pillared shrine on top. Matches the gameplay blocks (base 6 x 4.4 x 2.2, wheels r 1.3)."""
    kit = T.Kit()
    plan = T.rect(3.0, 2.2)
    prof = [(0.6, 0.1, "padma"), (0.9, 0.05, "lata"), (1.6, 0.12, "bead"), (1.9, 0.2, "dentil"), (2.2, 0.0, None)]
    kit.add(T.molded("chassis", [(z, T.offset_poly(plan, d), s) for z, d, s in prof]))
    # Wheels: axis along Y (the long sides are the ±Y faces here).
    for x in (-1.6, 1.6):
        for sy in (-1, 1):
            y = sy * 2.6
            hub = T.lathe("wheel", [(0.0, -0.2), (1.3, -0.2), (1.3, 0.2), (0.0, 0.2)], 32, (0.0, 0.0), mat="stone")
            rim_ring = T.lathe("wrim", [(1.18, -0.24), (1.32, -0.24), (1.32, 0.24), (1.18, 0.24)], 32, (0.0, 0.0), uv_strip="bead")
            spokes = T.lathe("whub", [(0.0, -0.3), (0.42, -0.3), (0.42, 0.3), (0.0, 0.3)], 16, (0.0, 0.0), uv_strip="padma")
            for o in (hub, rim_ring, spokes):
                o.rotation_euler = (math.pi / 2, 0.0, 0.0)
                o.location = (x, y, 1.3)
                bpy.context.view_layer.objects.active = o
                o.select_set(True)
                bpy.ops.object.transform_apply(location=True, rotation=True)
                o.select_set(False)
                kit.add(o)
    # Shrine body: pillared cella on the chassis, small stepped roof and dome.
    for x in (-1.6, 1.6):
        for y in (-1.6, 1.6):
            vijayanagara_pillar(kit, x, y, 2.2, 2.3, 0.6)
    kit.add(T.molded("cella", [(2.2, T.rect(1.2, 1.2), "lata"), (4.3, T.rect(1.2, 1.2), "dentil"), (4.5, T.rect(2.0, 2.0), None)]))
    kapota(kit, 2.0, 2.0, 0.0, 0.0, 4.2)
    kit.add(T.molded("croof", [(4.5, T.rect(1.6, 1.4), "gavaksha"), (5.5, T.rect(1.1, 0.9), "dentil"), (5.7, T.rect(1.1, 0.9), None)]))
    kit.add(T.lathe("cdome", [(0.9, 5.7), (1.1, 6.0), (0.8, 6.5), (0.2, 6.8), (0.0, 6.82)], 8, (0.0, 0.0)))
    T.kalasha(kit, (0.0, 0.0), 6.8, 0.4)
    return finish("stone_chariot", kit)


# ------------------------------------------------------------------ ruined mandapa

# Pillar states on the 6 x 4 grid (x = -10 + 4i, game z = -6 + 4j): "F" standing to the beams,
# a number = a broken stump of that height, None = gone. Must match `india.rs::MANDAPA_RUIN`.
RUIN = [
    ["F", "F", "F", "F"],
    ["F", "F", "F", "F"],
    ["F", "F", "F", "F"],
    ["F", 2.2, "F", None],
    [1.4, "F", None, 2.8],
    [None, 1.0, "F", 2.0],
]
RUIN_PLINTH = 0.6
RUIN_PILLAR = 4.2


def broken_pillar(kit, x, y, z0, h, seed):
    """A snapped Vijayanagara pillar: base, a carved block or two, a rough fractured top."""
    rnd = random.Random(seed)
    w = 0.32
    kit.add(T.box("pb", (x - w - 0.05, y - w - 0.05, z0), (x + w + 0.05, y + w + 0.05, z0 + 0.26)))
    zz = z0 + 0.26
    blk = 0.73
    k = 0
    while zz + 0.05 < z0 + h:
        top = min(zz + blk, z0 + h)
        o = T.box("cube", (x - w, y - w, zz), (x + w, y + w, top), "trim")
        v0, v1 = texgen.strip_v("lata" if k % 2 == 0 else "padma")
        uvl = o.data.uv_layers[0]
        for loop, d in zip(o.data.loops, uvl.data):
            c = o.data.vertices[loop.vertex_index].co
            d.uv = ((c.x + c.y) / (2 * w) * 0.5, v0 + (v1 - v0) * (c.z - zz) / blk)
        if top >= z0 + h - 1e-4:
            # Fracture: tilt and jag the top face.
            tx, ty = rnd.uniform(-0.25, 0.25), rnd.uniform(-0.25, 0.25)
            for v in o.data.vertices:
                if v.co.z > top - 1e-4:
                    v.co.z += (v.co.x - x) * tx + (v.co.y - y) * ty + rnd.uniform(-0.06, 0.06)
        kit.add(o)
        zz = top + 0.25
        if zz < z0 + h:
            kit.add(T.lathe("oct", [(w * 0.85, top), (w * 0.8, top + 0.12), (w * 0.85, zz)], 8, (x, y)))
        k += 1


def fallen_pillar(kit, cx, cy, z, yaw, length, seed):
    """A toppled pillar shaft lying on its side."""
    w = 0.32
    parts = []
    n = 3
    for k in range(n):
        x0 = -length / 2 + k * length / n
        o = T.box("fcube", (x0 + 0.04, -w, -w), (x0 + length / n - 0.04, w, w), "trim" if k % 2 == 0 else "stone")
        parts.append(o)
    for o in parts:
        o.rotation_euler = (0.0, 0.0, yaw)
        o.location = (cx, cy, z + w)
        bpy.context.view_layer.objects.active = o
        o.select_set(True)
        bpy.ops.object.transform_apply(location=True, rotation=True)
        o.select_set(False)
        kit.add(o)


def mandapa_ruin():
    """The ruined pillared hall at Hampi: a moulded granite plinth with steps front and back, a
    grid of composite pillars (some snapped, some gone), the beams and roof slabs still in place
    over the western half with a broken edge, fallen shafts and a cracked slab on the ground."""
    kit = T.Kit()
    plan = T.rect(12.0, 8.0)
    prof = [(0.0, 0.18, "padma"), (0.2, 0.12, "bead"), (0.32, 0.06, None), (0.48, 0.12, "dentil"), (RUIN_PLINTH, 0.12, None)]
    kit.add(T.molded("rplinth", [(z, T.offset_poly(plan, d), st) for z, d, st in prof]))
    for (sx, sy) in ((4.0, -1), (-4.0, 1)):
        for k in range(3):
            y_out = sy * (8.12 + 1.8 - k * 0.6)
            y_in = sy * 8.1
            kit.add(T.box("rstep", (sx - 1.5, min(y_out, y_in), k * 0.2), (sx + 1.5, max(y_out, y_in), (k + 1) * 0.2)))
    z0 = RUIN_PLINTH
    for i, col in enumerate(RUIN):
        for j, st in enumerate(col):
            x, y = -10.0 + 4 * i, -(-6.0 + 4 * j)
            if st == "F":
                vijayanagara_pillar(kit, x, y, z0, RUIN_PILLAR)
            elif st is not None:
                broken_pillar(kit, x, y, z0, st, i * 7 + j)
    # Beams along both axes over the roofed bay (x -10..-2, plus the stubs reaching x = 1).
    top = z0 + RUIN_PILLAR
    for j in range(4):
        y = -(-6.0 + 4 * j)
        kit.add(T.molded("rbeam", [(top - 0.42, T.rect(5.8, 0.24, -5.2, y), "lata"), (top, T.rect(5.8, 0.24, -5.2, y), None)]))
    for i in range(3):
        x = -10.0 + 4 * i
        kit.add(T.box("rbeam_y", (x - 0.24, -6.5, top - 0.42), (x + 0.24, 6.5, top)))
    # Roof: granite slabs laid across the beams, the eastern edge broken off unevenly, a kapota
    # eave on the three intact sides.
    rnd = random.Random(11)
    for k in range(6):
        xa = -11.0 + k * 2.0
        xb = xa + 2.0
        if k == 5:
            xb = 1.0
        o = T.box("rslab", (xa + 0.02, -7.0, top), (xb - 0.02, 7.0, top + 0.6))
        if k == 5:
            for v in o.data.vertices:
                if v.co.x > xa + 1.0:
                    v.co.x = xa + 0.3 + 1.6 * (0.5 + 0.5 * math.sin(v.co.y * 1.7 + 0.4)) + rnd.uniform(-0.2, 0.2)
        kit.add(o)
    prof = [(0.0, 0.0, "bead"), (0.08, 0.25, "plain"), (0.18, 0.45, "plain"), (0.3, 0.6, "plain"), (0.38, 0.66, None)]
    for side in (-1, 1):
        kit.add(T.molded("rkapota", [(top - 0.25 + dz, T.rect(6.0 + d / 2, 0.01 + d / 2, -5.0, side * (7.0 + d / 2)), st)
                                      for dz, d, st in prof]))
    kit.add(T.molded("rkapota_w", [(top - 0.25 + dz, T.rect(0.01 + d / 2, 7.0 + d, -11.0 - d / 2, 0.0), st) for dz, d, st in prof]))
    # Debris: fallen shafts, a cracked slab, rubble blocks.
    fallen_pillar(kit, 7.5, -3.5, z0, 0.3, 3.4, 1)
    fallen_pillar(kit, 14.0, 3.0, 0.0, 1.2, 3.0, 2)
    o = T.box("fslab", (2.6, 3.8, z0), (5.4, 6.3, z0 + 0.4))
    o.rotation_euler = (0.0, 0.08, 0.15)
    kit.add(o)
    for k in range(9):
        x, y = rnd.uniform(1.5, 11.0), rnd.uniform(-7.5, 7.5)
        if any(abs(x - (-10 + 4 * i)) < 0.8 and abs(y - (6 - 4 * j)) < 0.8 for i in range(6) for j in range(4)):
            continue
        s = rnd.uniform(0.25, 0.5)
        o = T.box("rubble", (x - s, y - s * 0.7, z0), (x + s, y + s * 0.7, z0 + s * 0.8))
        o.rotation_euler = (0.0, 0.0, rnd.uniform(0, math.pi))
        kit.add(o)
    return finish("mandapa_ruin", kit)


def main():
    export.reset_scene()
    mats()
    out = {}
    for seed, name in ((1, "rock_a"), (2, "rock_b"), (3, "rock_c")):
        for o in list(bpy.data.objects):
            bpy.data.objects.remove(o)
        kit = T.Kit()
        kit.add(boulder(seed))
        lods = finish(name, kit, lods=(0.35,), smooth=True)
        out[name] = export.export_glb(lods, os.path.join(OUT, f"{name}.gltf"))
    for name, make in (("gopuram", gopuram), ("temple_dravida", dravida_temple), ("stone_chariot", stone_chariot),
                       ("mandapa_ruin", mandapa_ruin)):
        for o in list(bpy.data.objects):
            bpy.data.objects.remove(o)
        lods = make()
        out[name] = export.export_glb(lods, os.path.join(OUT, f"{name}.gltf"))
    for k, v in out.items():
        print(f"[hampi] {k}: {v}")


if __name__ == "__main__":
    main()

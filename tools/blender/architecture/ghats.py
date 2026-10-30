"""Kashi Ghat: the riverfront embankment of the ghat town, over the map's colliders (india.rs
`river_and_ghats`). Five ghats of broad buff-sandstone flights, each broken by two landings and
running down into the water, between six burj: bastions standing out into the river with a
rounded prow, banded walls and a parapet. Also the pieces scattered on the landings and the
river: a small Shiva shrine with a curvilinear spire, a cane umbrella over a wooden chowki, and
two wooden boats (one with a canopy).

The plan repeats india.rs (`KASHI_TOP`, `GHAT_*`, `BURJ_*`): keep the two in step. The
embankment's origin is the world origin. Blender X = game X, Blender Y = -game Z.

blender -b --factory-startup -P tools/blender/architecture/ghats.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix  # noqa: E402

import buddhist as B  # noqa: E402
import export  # noqa: E402
import ornaments as O  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402

OUT = os.path.join(T.ROOT, "assets", "models", "props")

TOP = 5.02                  # the deck along the top of the ghats (the town bank is at 5.0)
BED = -1.6
BOTTOM = -1.7
TERRACE_X = 152.0           # the deck runs from here to the edge of the first flight
# Flights (x from, x to, y from, y to) descending east; landings between them.
FLIGHTS = ((156.0, 160.0, TOP, 2.82), (164.0, 168.0, 2.82, 0.62), (170.5, 175.5, 0.62, BED + 0.08))
STEP_RISE = 0.22
BURJ_Z = (-26.5, 4.0, 34.5, 65.5, 96.0, 126.5)
BURJ_HALF = 3.5
BURJ_TIP = 171.0            # centre of the rounded prow (radius BURJ_HALF), out in the river
PARAPET = (0.4, 0.9)        # thickness, height


# ------------------------------------------------------------------ embankment

def ghat_profile():
    """The ghat's section (x, y): the deck, the three flights and their landings, closed below."""
    pts = [(TERRACE_X, BOTTOM), (TERRACE_X, TOP)]
    for x0, x1, y0, y1 in FLIGHTS:
        n = round((y0 - y1) / STEP_RISE)
        run, rise = (x1 - x0) / n, (y0 - y1) / n
        pts.append((x0, y0))
        for k in range(n):
            pts.append((x0 + k * run, y0 - (k + 1) * rise))
            pts.append((x0 + (k + 1) * run, y0 - (k + 1) * rise))
    pts.append((FLIGHTS[-1][1], BOTTOM))
    # Drop repeats (a flight starting where the last one ended).
    out = []
    for p in pts:
        if not out or (abs(out[-1][0] - p[0]) > 1e-4 or abs(out[-1][1] - p[1]) > 1e-4):
            out.append(p)
    return out


def ghat(kit, z0, z1):
    """One ghat's flights between burj faces at game z0 < z1."""
    o = O.solid_from_outline("ghat", ghat_profile(), -z1, -z0, "stone")
    kit.add(o)


def burj_plan(zc, grow=0.0):
    """Plan polygon (Blender XY, CCW) of a burj: a rectangle from the deck's back to the prow's
    centre, then the half circle of the prow."""
    h = BURJ_HALF + grow
    y = -zc
    pts = [(TERRACE_X - grow, y - h), (BURJ_TIP, y - h)]
    seg = 16
    for k in range(1, seg):
        a = -math.pi / 2 + math.pi * k / seg
        pts.append((BURJ_TIP + h * math.cos(a), y + h * math.sin(a)))
    pts.append((BURJ_TIP, y + h))
    pts.append((TERRACE_X - grow, y + h))
    return pts


def burj(kit, zc):
    plan = burj_plan(zc)
    # Body with a water-stained plinth band, a string course and a cornice under the parapet.
    kit.add(T.molded("burj", [(BOTTOM, plan, None), (-0.2, plan, None)]))
    kit.add(T.molded("burj_band", [(-0.2, burj_plan(zc, 0.12), "padma"), (0.3, burj_plan(zc, 0.12), None)]))
    kit.add(T.molded("burj", [(0.3, plan, None), (2.6, plan, None)]))
    kit.add(T.molded("burj_band", [(2.6, burj_plan(zc, 0.08), "dentil"), (2.85, burj_plan(zc, 0.08), None)]))
    kit.add(T.molded("burj", [(2.85, plan, None), (4.45, plan, None)]))
    kit.add(T.molded("burj_band", [(4.45, burj_plan(zc, 0.18), "lata"), (TOP, burj_plan(zc, 0.18), None)]))
    # Pilasters down the side walls, between the bands.
    for side in (-1, 1):
        y = -zc + side * BURJ_HALF
        for x in (158.5, 162.5, 166.5, 170.0):
            for z0, z1 in ((0.3, 2.6), (2.85, 4.45)):
                kit.add(T.box("pilaster", (x - 0.3, min(y, y + side * 0.1), z0), (x + 0.3, max(y, y + side * 0.1), z1), "trim"))
    # Parapet along the sides and round the prow, with a coping; open toward the deck.
    t, ph = PARAPET
    for side in (-1, 1):
        y0, y1 = sorted((-zc + side * BURJ_HALF, -zc + side * (BURJ_HALF - t)))
        kit.add(T.box("bparapet", (156.0, y0, TOP), (BURJ_TIP, y1, TOP + ph)))
    seg = 12
    r0, r1 = BURJ_HALF - t, BURJ_HALF
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    rings = []
    for k in range(seg + 1):
        a = -math.pi / 2 + math.pi * k / seg
        c, s = math.cos(a), math.sin(a)
        rings.append([bm.verts.new((BURJ_TIP + r * c, -zc + r * s, z)) for r, z in ((r0, TOP), (r1, TOP), (r1, TOP + ph), (r0, TOP + ph))])
    for k in range(seg):
        for i in range(4):
            j = (i + 1) % 4
            bm.faces.new((rings[k][i], rings[k][j], rings[k + 1][j], rings[k + 1][i]))
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    o = T.new_obj("bparapet_arc", bm, ("stone",))
    T.cube_uvs(o)
    kit.add(o)
    # Coping: a moulded cap 4 cm proud on both faces.
    for side in (-1, 1):
        y0, y1 = sorted((-zc + side * (BURJ_HALF + 0.04), -zc + side * (BURJ_HALF - t - 0.04)))
        kit.add(T.box("bcoping", (156.0, y0, TOP + ph), (BURJ_TIP + 0.01, y1, TOP + ph + 0.1), "trim"))
    kit.add(T.lathe("bcoping_arc", [(r0 - 0.04, TOP + ph), (r1 + 0.04, TOP + ph), (r1 + 0.04, TOP + ph + 0.1), (r0 - 0.04, TOP + ph + 0.1)],
                    32, (BURJ_TIP, -zc)))
    # The full-circle lathe would cut through the burj's top behind the prow: trim it away by
    # keeping only the prow's half (delete faces with x < BURJ_TIP).
    lat = kit.objs[-1]
    bm = bmesh.new()
    bm.from_mesh(lat.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.calc_center_median().x < BURJ_TIP - 0.02], context="FACES")
    bm.to_mesh(lat.data)
    bm.free()


def embankment():
    kit = T.Kit()
    edges = [z + s * BURJ_HALF for z in BURJ_Z for s in (-1, 1)]
    for z0, z1 in zip(edges[1::2], edges[2::2]):
        ghat(kit, z0, z1)
    for zc in BURJ_Z:
        burj(kit, zc)
    return B.finish("ghats", kit, lods=(0.3,))


# ------------------------------------------------------------------ pieces

def shrine():
    """A small Shiva shrine on a moulded plinth: a square cella open to the east (+X) holding a
    linga on its yoni, a slab roof and a latina spire with an amalaka. 2 x 2 m, origin at the
    base centre."""
    kit = T.Kit()
    kit.add(T.molded("splinth", [(0.0, T.rect(1.0, 1.0), "padma"), (0.3, T.rect(1.0, 1.0), None)]))
    h0, h1, w, t = 0.3, 1.9, 0.75, 0.14
    door = 0.32
    kit.add(T.box("swall", (-w, -w, h0), (-w + t, w, h1)))
    kit.add(T.box("swall", (-w + t, -w, h0), (w, -w + t, h1)))
    kit.add(T.box("swall", (-w + t, w - t, h0), (w, w, h1)))
    kit.add(T.box("swall", (w - t, -w + t, h0), (w, -door, h1)))
    kit.add(T.box("swall", (w - t, door, h0), (w, w - t, h1)))
    kit.add(T.box("slintel", (w - t, -door, 1.35), (w, door, h1)))
    kit.add(T.box("sdark", (-w + t, -w + t, h0), (-w + t + 0.01, w - t, h1), "shadow"))
    kit.add(T.molded("sroof", [(h1, T.rect(0.86, 0.86), "dentil"), (h1 + 0.16, T.rect(0.86, 0.86), None)]))
    kit.add(T.lathe("yoni", [(0.0, h0), (0.22, h0), (0.22, h0 + 0.12), (0.16, h0 + 0.16), (0.0, h0 + 0.16)], 20, (-0.1, 0.0), mat="dark"))
    kit.add(T.lathe("linga", [(0.0, h0 + 0.16), (0.08, h0 + 0.16), (0.08, h0 + 0.4), (0.05, h0 + 0.47), (0.0, h0 + 0.49)], 16, (-0.1, 0.0), mat="dark"))
    T.shikhara(kit, T.rect(0.72, 0.72), (0.0, 0.0), h1 + 0.16, 1.6, bands=3, scale=0.22)
    obj = O.finish_piece("ghat_shrine", kit)
    T.bake_weathering(obj)
    return [obj]


def umbrella():
    """A cane umbrella on a bamboo pole over a wooden chowki (a low platform), 2 x 1.3 m; the
    pole stands at the platform's back edge and the canopy (radius 1.7) leans over it."""
    kit = T.Kit()
    w, d, h = 1.0, 0.65, 0.45
    kit.add(T.box("ctop", (-w, -d, h - 0.08), (w, d, h), "wood"))
    for sx in (-1, 1):
        for sy in (-1, 1):
            x0, x1 = sorted((sx * w, sx * (w - 0.08)))
            y0, y1 = sorted((sy * d, sy * (d - 0.08)))
            kit.add(T.box("cleg", (x0, y0, 0.0), (x1, y1, h - 0.08), "wood"))
    for sy in (-1, 1):
        y0, y1 = sorted((sy * (d - 0.01), sy * (d - 0.04)))
        kit.add(T.box("cskirt", (-w + 0.06, y0, h - 0.2), (w - 0.06, y1, h - 0.08), "wood"))
    # Pole and canopy, tilted 8 degrees toward the platform.
    tilt = Matrix.Translation((-0.15, d + 0.08, 0.0)) @ Matrix.Rotation(math.radians(-8), 4, "X")
    pole = T.lathe("pole", [(0.045, 0.0), (0.04, 2.75)], 8, (0, 0), mat="bamboo")
    kit.add(B.transform(pole, tilt))
    R, rise, seg = 1.7, 0.45, 24
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    tip = bm.verts.new((0, 0, 2.7))
    rim = [bm.verts.new((R * math.cos(2 * math.pi * k / seg), R * math.sin(2 * math.pi * k / seg), 2.7 - rise - 0.06 * (k % 2))) for k in range(seg)]
    for k in range(seg):
        f = bm.faces.new((tip, rim[k], rim[(k + 1) % seg]))
        for loop in f.loops:
            co = loop.vert.co
            loop[uv].uv = (co.x / 1.5, co.y / 1.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    for f in bm.faces:
        if f.normal.z < 0:
            f.normal_flip()
    can = T.new_obj("canopy", bm, ("cane",))
    sol = can.modifiers.new("s", "SOLIDIFY")
    sol.thickness = 0.03
    bpy.context.view_layer.objects.active = can
    bpy.ops.object.modifier_apply(modifier="s")
    kit.add(B.transform(can, tilt))
    for k in range(0, seg, 2):
        a = 2 * math.pi * k / seg
        rib = T.box("rib", (0.0, -0.012, -0.012), (R, 0.012, 0.012), "bamboo")
        Mr = Matrix.Translation((0, 0, 2.68)) @ Matrix.Rotation(a, 4, "Z") @ Matrix.Rotation(math.atan2(rise, R), 4, "Y")
        kit.add(B.transform(rib, tilt @ Mr))
    obj = O.finish_piece("ghat_umbrella", kit)
    return [obj]


def hull(kit, L, beam, depth):
    """A wooden river-boat hull along X, keel at z = 0, the ends swept up into a prow and stern."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    nu = 24
    sec = []
    for i in range(nu + 1):
        u = -1 + 2 * i / nu
        x = u * L / 2
        b = beam / 2 * max(0.04, (1 - abs(u) ** 2.6)) ** 0.6
        zt = depth + 0.38 * abs(u) ** 4
        zk = 0.28 * abs(u) ** 3
        pts = [(-b, zt), (-b * 0.92, zk + 0.28 * (zt - zk)), (-b * 0.55, zk + 0.04), (0.0, zk), (b * 0.55, zk + 0.04), (b * 0.92, zk + 0.28 * (zt - zk)), (b, zt)]
        sec.append([bm.verts.new((x, y, z)) for y, z in pts])
    for i in range(nu):
        for k in range(6):
            f = bm.faces.new((sec[i][k], sec[i + 1][k], sec[i + 1][k + 1], sec[i][k + 1]))
            for loop in f.loops:
                co = loop.vert.co
                loop[uv].uv = (co.x / 2.0, (co.z + abs(co.y)) / 2.0)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    o = T.new_obj("hull", bm, ("wood",))
    sol = o.modifiers.new("s", "SOLIDIFY")
    sol.thickness = 0.06
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.modifier_apply(modifier="s")
    kit.add(o)
    # Painted gunwale strip and thwarts.
    for side in (-1, 1):
        kit.add(T.box("gunwale", (-L * 0.36, side * beam / 2 - 0.05, depth - 0.12), (L * 0.36, side * beam / 2 + 0.03, depth + 0.02), "paint"))
    for x in (-L * 0.25, 0.0, L * 0.25):
        kit.add(T.box("thwart", (x - 0.15, -beam / 2 + 0.08, depth - 0.25), (x + 0.15, beam / 2 - 0.08, depth - 0.19), "wood"))


def boat(canopy):
    kit = T.Kit()
    L, beam, depth = 7.0, 2.0, 0.85
    hull(kit, L, beam, depth)
    if canopy:
        # A bajra's awning: four posts and a cloth roof, gently curved.
        for sx in (-1, 1):
            for sy in (-1, 1):
                kit.add(T.lathe("post", [(0.04, depth - 0.2), (0.035, depth + 1.35)], 8, (sx * 1.1, sy * 0.8), mat="wood"))
        bm = bmesh.new()
        uv = bm.loops.layers.uv.new("UVMap")
        n = 10
        rows = []
        for i in range(2):
            x = -1.3 + 2.6 * i
            rows.append([bm.verts.new((x, -1.0 + 2.0 * k / n, depth + 1.35 + 0.25 * math.sin(math.pi * k / n))) for k in range(n + 1)])
        for k in range(n):
            f = bm.faces.new((rows[0][k], rows[1][k], rows[1][k + 1], rows[0][k + 1]))
            for loop in f.loops:
                co = loop.vert.co
                loop[uv].uv = (co.x / 2.6 + 0.5, co.y / 2.0 + 0.5)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        for f in bm.faces:
            if f.normal.z < 0:
                f.normal_flip()
        o = T.new_obj("awning", bm, ("cloth",))
        sol = o.modifiers.new("s", "SOLIDIFY")
        sol.thickness = 0.03
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_apply(modifier="s")
        kit.add(o)
    obj = O.finish_piece("boat_bajra" if canopy else "boat", kit)
    return [obj]


def main():
    export.reset_scene()
    texgen.sandstone(T.TEX)
    texgen.trim_sheet(T.TEX)
    texgen.wood(T.TEX)
    T.MATS.update({
        "stone": T.tex_material("sandstone", "sandstone"),
        "trim": T.tex_material("trim", "trim"),
        "wood": T.tex_material("wood", "wood"),
        "shadow": T.flat_material("shadow", (0.03, 0.025, 0.02), 0.0, 0.95),
        "dark": T.flat_material("basalt", (0.08, 0.075, 0.07), 0.0, 0.35),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
        "cloth": T.flat_material("dhvaja", (0.95, 0.45, 0.05), 0.0, 0.8),
        "cane": T.flat_material("cane", (0.55, 0.42, 0.24), 0.0, 0.9),
        "bamboo": T.flat_material("bamboo", (0.50, 0.40, 0.22), 0.0, 0.7),
        "paint": T.flat_material("boatpaint", (0.10, 0.30, 0.55), 0.0, 0.6),
    })
    only = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else None
    pieces = {
        "ghats": embankment,
        "ghat_shrine": shrine,
        "ghat_umbrella": umbrella,
        "boat": lambda: boat(False),
        "boat_bajra": lambda: boat(True),
    }
    for key, make in pieces.items():
        if only and key not in only:
            continue
        for o in list(bpy.data.objects):
            bpy.data.objects.remove(o)
        objs = make()
        print(f"[ghats] {key}: {export.export_glb(objs, os.path.join(OUT, f'{key}.gltf'))}")


if __name__ == "__main__":
    main()

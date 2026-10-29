"""Rani ki Baoli: the stepwell's carved lining. Tiers of niches between pilasters and cornices on
the two long walls and the west wall, each holding one of the figures (Vishnu, apsaras with a
mirror or dancing, devotees, Krishna with the flute) or a pair of them, and jali, vine and
lotus-rosette panels in between; two styles of carved pillar for the pavilions, moulded fascias
on the pavilion roofs (the bridges across the well), railings along the bridges and the rim, and
kiosks at the bridges' ends.

Matches `india.rs::stepwell()` (frame at STEPWELL_C, yaw 0): pit 62 x 20, seven levels 3 m apart
(21 m deep), flights descending west from x = +31 with a pillared pavilion over each landing,
rising storey by storey to the bridge at ground level, and a well head on the bottom floor.
Blender X = game X, Blender Y = -game Z.

blender -b --factory-startup -P tools/blender/architecture/stepwell.py
"""

import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import buddhist as B  # noqa: E402
import export  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402
from vernacular import cylinder_between  # noqa: E402

OUT = os.path.join(T.ROOT, "assets", "models", "props")
HL, HW, LEVELS, LVL = 31.0, 10.0, 7, 3.0
DEPTH = LEVELS * LVL
FLIGHT, LANDING, STAIR_W = 4.5, 3.2, 9.0
WELL_X, WELL_R = -26.0, 3.2


def vishnu(kit, mat_world, s=1.0):
    """Standing four-armed Vishnu (samabhanga) on a lotus pedestal: the sculpted figure
    (figures.py) with a tall crown, the discus and conch in the upper hands and the mace under
    the lower left. ~1.75*s tall, at the origin facing -Y, moved by `mat_world`."""
    base, h = 0.14 * s, 1.5 * s
    parts = [T.lathe("vpad", [(0.26 * s, 0.0), (0.3 * s, 0.07 * s), (0.22 * s, base)], 16, (0, 0), uv_strip="padma")]
    meta = B.figure_meta("vishnu")
    at = {k: Vector((v.x * h, v.y * h, base + v.z * h)) for k, v in meta.items()}
    lod = "hi" if s > 1.2 else "lo"
    kit.add(B.figure("vishnu", lod, mat_world @ Matrix.Translation((0, 0, base)) @ Matrix.Scale(h, 4)))
    # Kirita mukuta: the tall cylindrical crown, set down over the crown of the head.
    top = at["head_top"]
    parts.append(T.lathe("kirita", [(r * s, top.z + z * s) for r, z in ((0.07, -0.05), (0.075, 0.0), (0.068, 0.08), (0.05, 0.16), (0.02, 0.21), (0.0, 0.23))],
                         12, (top.x, top.y), uv_strip="bead"))
    d, c = at["discus"], at["conch"]
    parts.append(cylinder_between("chakra", d + Vector((0, -0.02, 0.07)) * s, d + Vector((0, 0.02, 0.07)) * s, 0.09 * s, "trim", 16))
    parts.append(B.uv_sphere("shankha", c + Vector((0, 0, 0.06)) * s, (0.045 * s, 0.045 * s, 0.08 * s), "stone", 8, 6))
    m = at["mace"]
    parts.append(cylinder_between("gada", (m.x, m.y, base), (m.x, m.y, m.z - 0.04 * s), 0.03 * s, "stone", 6))
    parts.append(B.uv_sphere("gadahead", (m.x, m.y, m.z - 0.02 * s), (0.065 * s, 0.065 * s, 0.07 * s), "stone", 8, 6))
    for o in parts:
        kit.add(B.transform(o, mat_world))


def standing(name, crown=False):
    """A standing figure (figures.py) on a lotus pedestal, with a short crown for the men.
    ~1.7*s tall, at the origin facing -Y, moved by `mat_world`."""
    def place(kit, mat_world, s=1.0):
        base, h = 0.14 * s, 1.5 * s
        kit.add(B.transform(T.lathe("vpad", [(0.26 * s, 0.0), (0.3 * s, 0.07 * s), (0.22 * s, base)], 16, (0, 0), uv_strip="padma"), mat_world))
        kit.add(B.figure(name, "hi" if s > 1.2 else "lo", mat_world @ Matrix.Translation((0, 0, base)) @ Matrix.Scale(h, 4)))
        if crown:
            top = B.figure_meta(name)["head_top"]
            tx, ty, tz = top.x * h, top.y * h, base + top.z * h
            kit.add(B.transform(T.lathe("mukuta", [(r * s, tz + z * s) for r, z in ((0.065, -0.04), (0.07, 0.0), (0.05, 0.07), (0.015, 0.12), (0.0, 0.13))],
                                        10, (tx, ty), uv_strip="bead"), mat_world))
    return place


# Niche figures: Vishnu less often than the others, so no one figure dominates a wall.
FIGURES = [vishnu, standing("darpana"), standing("nartaki"), standing("anjali", True), standing("venugopala", True)]
SMALL = FIGURES[1:]


def rosette(kit, M, xc, zc, r):
    """A carved lotus medallion on the panel face (the panel's front is at y = -0.08)."""
    prof = [(r, 0.0), (r * 0.95, 0.05), (r * 0.6, 0.09), (r * 0.25, 0.11), (0.0, 0.12)]
    o = T.lathe("rosette", prof, 24, (0, 0), ribs=16, rib_depth=0.05 * r / 0.3, uv_strip="padma")
    kit.add(B.transform(o, M @ Matrix.Translation((xc, -0.07, zc)) @ Matrix.Rotation(math.pi / 2, 4, "X")))


def bay(kit, M, xc, z_lo, kind, rng):
    if kind == "niche":
        B.niche(kit, M, xc, z_lo + 0.35, 0.95, 1.75, 0.92, figure=rng.choice(FIGURES))
    elif kind == "pair":
        # A wide, lower niche holding two smaller figures side by side.
        a, b = rng.sample(SMALL, 2)

        def both(kit, mat_world, s):
            a(kit, mat_world @ Matrix.Translation((-0.34, 0.0, 0.0)), s)
            b(kit, mat_world @ Matrix.Translation((0.34, 0.0, 0.0)), s)
        B.niche(kit, M, xc, z_lo + 0.35, 1.4, 1.4, 0.72, figure=both)
    elif kind == "jali":
        # A lattice of small horseshoe arches over a lotus band.
        B.frieze(kit, M, xc - 0.84, xc + 0.84, z_lo + 0.6, z_lo + 1.9, 0.08, "gavaksha")
        B.frieze(kit, M, xc - 0.84, xc + 0.84, z_lo + 0.3, z_lo + 0.6, 0.12, "padma")
    elif kind == "vine":
        # Two vine-scroll panels parted by a rope moulding.
        B.frieze(kit, M, xc - 0.84, xc + 0.84, z_lo + 0.3, z_lo + 1.05, 0.1, "lata")
        B.frieze(kit, M, xc - 0.9, xc + 0.9, z_lo + 1.05, z_lo + 1.17, 0.15, "rope")
        B.frieze(kit, M, xc - 0.84, xc + 0.84, z_lo + 1.17, z_lo + 1.9, 0.1, "lata")
    else:
        # A lotus medallion on a chevron-carved panel.
        B.frieze(kit, M, xc - 0.84, xc + 0.84, z_lo + 0.3, z_lo + 1.9, 0.08, "chevron")
        rosette(kit, M, xc, z_lo + 1.1, 0.5)


BAYS = ["niche"] * 6 + ["pair"] * 2 + ["jali"] * 4 + ["vine"] * 4 + ["rosette"] * 4


def lined_wall(kit, M, length, seed, x_skip=()):
    """Tiers on one wall in piece space (X along the wall centred on 0, front -Y, plane Y=0)."""
    rng = random.Random(seed)
    span = 2.3
    n = int(length / span)
    x0 = -n * span / 2
    for t in range(LEVELS):
        z_hi = -t * LVL
        z_lo = z_hi - LVL
        # Cornice ledge at the top of the tier and a carved band under it.
        B.frieze(kit, M, x0, -x0, z_hi - 0.35, z_hi - 0.05, 0.42, "dentil")
        B.frieze(kit, M, x0, -x0, z_hi - 0.75, z_hi - 0.35, 0.18, "lata" if t % 2 == 0 else "gavaksha")
        for k in range(n + 1):
            x = x0 + k * span
            kit.add(B.transform(T.box("pil", (x - 0.16, -0.22, z_lo), (x + 0.16, 0.0, z_hi - 0.75), "trim"), M))
        prev = None
        for k in range(n):
            xc = x0 + (k + 0.5) * span
            if any(a < xc < b for a, b in x_skip):
                continue
            # Never the same kind of bay twice running along a tier.
            kind = rng.choice([b for b in BAYS if b != prev])
            bay(kit, M, xc, z_lo, kind, rng)
            prev = kind
    # Coping at ground level, tall enough to cap the top tier's niche heads (to z = +0.30) and
    # set 2 cm behind the wall plane so their backs, flush with that plane, sit inside it.
    xm, half = 0.0, -x0 + 0.3
    kit.add(B.transform(T.molded("coping", [(-0.05, T.rect(half, 0.31, xm, -0.29), "padma"),
                                            (0.39, T.rect(half, 0.31, xm, -0.29), None)]), M))


def pillar_fluted(kit, x, y, z0, h):
    """The second pavilion style: a pot base, a fluted round shaft girdled by a chain-and-bell
    band, a ribbed cushion capital, and the same cruciform bracket as `T.pillar`."""
    c = (x, y)
    q = h / 4
    kit.add(T.lathe("f_base", [(0.36, z0), (0.36, z0 + 0.22 * q), (0.3, z0 + 0.26 * q)], 4, c, square=True))
    kit.add(T.lathe("f_kumbha", [(0.3, z0 + 0.26 * q), (0.34, z0 + 0.45 * q), (0.3, z0 + 0.62 * q), (0.25, z0 + 0.7 * q)], 16, c,
                    ribs=16, rib_depth=0.04, uv_strip="padma"))
    kit.add(T.lathe("f_shaft", [(0.24, z0 + 0.7 * q), (0.22, z0 + 1.9 * q)], 16, c, ribs=16, rib_depth=0.035))
    kit.add(T.lathe("f_band", [(0.22, z0 + 1.9 * q), (0.27, z0 + 2.0 * q), (0.27, z0 + 2.25 * q), (0.22, z0 + 2.35 * q)], 16, c, uv_strip="rope"))
    kit.add(T.lathe("f_shaft2", [(0.22, z0 + 2.35 * q), (0.2, z0 + 3.0 * q)], 16, c, ribs=16, rib_depth=0.035))
    kit.add(T.lathe("f_cushion", [(0.2, z0 + 3.0 * q), (0.36, z0 + 3.18 * q), (0.38, z0 + 3.35 * q), (0.3, z0 + 3.55 * q)], 16, c,
                    ribs=24, rib_depth=0.05, uv_strip="rope"))
    kit.add(T.lathe("f_abacus", [(0.46, z0 + 3.55 * q), (0.46, z0 + 3.7 * q)], 4, c, square=True))
    kit.add(T.box("f_brk1", (x - 0.75, y - 0.2, z0 + 3.7 * q), (x + 0.75, y + 0.2, z0 + h)))
    kit.add(T.box("f_brk2", (x - 0.2, y - 0.75, z0 + 3.7 * q + 0.02), (x + 0.2, y + 0.75, z0 + h - 0.02)))


def railing(kit, axis, a, b, c, z0):
    """Vedika along `axis` from a to b, centred on the other coordinate c, standing at z0."""
    n = max(1, round((b - a) / 0.9))
    def box(name, u0, u1, w, z_a, z_b, mat="stone"):
        if axis == "x":
            return T.box(name, (u0, c - w, z_a), (u1, c + w, z_b), mat)
        return T.box(name, (c - w, u0, z_a), (c + w, u1, z_b), mat)
    for k in range(n + 1):
        u = a + (b - a) * k / n
        kit.add(box("vpost", u - 0.09, u + 0.09, 0.12, z0, z0 + 0.58))
    # Bars sit between the posts (only the posts carry the coping), 1 cm thinner than them.
    for k in range(n):
        u0 = a + (b - a) * k / n + 0.09
        u1 = a + (b - a) * (k + 1) / n - 0.09
        for zb in (0.16, 0.36):
            kit.add(box("vbar", u0, u1, 0.07, z0 + zb, z0 + zb + 0.1, "trim"))
    rail = T.molded("vrail", [(z0 + 0.58, T.rect((b - a) / 2 + 0.1, 0.12, (a + b) / 2, c) if axis == "x" else T.rect(0.12, (b - a) / 2 + 0.1, c, (a + b) / 2), "bead"),
                              (z0 + 0.7, T.rect((b - a) / 2 + 0.1, 0.12, (a + b) / 2, c) if axis == "x" else T.rect(0.12, (b - a) / 2 + 0.1, c, (a + b) / 2), None)])
    kit.add(rail)


def stepwell():
    kit = T.Kit()
    # Long walls: the north one (game z = -10, Blender Y = +10) faces -Y already.
    lined_wall(kit, Matrix.Translation((0.0, HW, 0.0)), 2 * HL, 1)
    lined_wall(kit, Matrix.Translation((0.0, -HW, 0.0)) @ Matrix.Rotation(math.pi, 4, "Z"), 2 * HL, 2)
    lined_wall(kit, Matrix.Translation((-HL, 0.0, 0.0)) @ Matrix.Rotation(math.pi / 2, 4, "Z"), 2 * HW, 3)
    # Pavilions over the landings: storeys of carved pillars, a moulded beam across the well at
    # each storey's top, and a moulded fascia on the roof-bridge at ground level.
    x = HL
    bridges = []
    for i in range(LEVELS - 1):
        bottom = -(i + 1) * LVL
        x -= FLIGHT
        rows = (x - LANDING + 0.6, x - 0.6)
        for px in rows:
            for pz in (-HW + 0.9, -STAIR_W / 2 - 0.4, STAIR_W / 2 + 0.4, HW - 0.9):
                for s in range(i + 1):
                    z0 = bottom + s * LVL
                    # The top storey's pillar reaches the bridge; lower ones stop under a beam.
                    h = (-0.5 - z0) if s == i else LVL - 0.45
                    (T.pillar if i % 2 == 0 else pillar_fluted)(kit, px, -pz, z0, h)
            for s in range(1, i + 1):
                zt = bottom + s * LVL
                kit.add(T.molded("beam", [(zt - 0.45, T.rect(0.28, HW, px, 0.0), "lata"), (zt, T.rect(0.28, HW, px, 0.0), None)]))
        cx = x - LANDING / 2
        kit.add(T.molded("bridge", [(-0.55, T.rect(LANDING / 2 + 0.08, HW, cx, 0.0), "dentil"),
                                    (-0.3, T.rect(LANDING / 2 + 0.08, HW, cx, 0.0), "lata"),
                                    (0.0, T.rect(LANDING / 2 + 0.08, HW, cx, 0.0), None)]))
        # Kirtimukha brackets under the bridge edges, between the pillars.
        for px in rows:
            for pz in (-7.0, 0.0, 7.0):
                kit.add(T.box("brk", (px - 0.15, -pz - 0.5, -0.9), (px + 0.15, -pz + 0.5, -0.55), "trim"))
        # Railings along both edges of the bridge, between the kiosks at its ends (india.rs).
        for ex in (x - LANDING + 0.14, x - 0.14):
            railing(kit, "y", -HW + 2.6, HW - 2.6, ex, 0.0)
        bridges.append((x - LANDING - 0.1, x + 0.1))
        x -= LANDING
    # Railings on the coping along the long rims, broken where the bridges meet them.
    edges = [-HL + 0.4] + [e for br in sorted(bridges) for e in br] + [HL - 0.4]
    for a, b in zip(edges[0::2], edges[1::2]):
        if b - a > 1.5:
            for side in (-1, 1):
                railing(kit, "x", a + 0.3, b - 0.3, side * (HW - 0.35), 0.39)
    # Well head on the bottom floor: a moulded parapet ring round the pool.
    b = -DEPTH
    kit.add(T.lathe("wellhead", [(WELL_R - 0.05, b), (WELL_R - 0.05, b + 0.72), (WELL_R + 0.05, b + 0.78), (WELL_R + 0.05, b + 0.9),
                                 (WELL_R - 0.55, b + 0.9), (WELL_R - 0.55, b + 0.3)], 40, (WELL_X, 0.0)))
    kit.add(T.lathe("wellband", [(WELL_R - 0.04, b + 0.25), (WELL_R + 0.02, b + 0.3), (WELL_R + 0.02, b + 0.6), (WELL_R - 0.04, b + 0.65)],
                    40, (WELL_X, 0.0), uv_strip="padma"))
    return B.finish("stepwell", kit, lods=(0.3,))


def main():
    export.reset_scene()
    texgen.sandstone(T.TEX)
    texgen.trim_sheet(T.TEX)
    T.MATS.update({
        "stone": T.tex_material("sandstone", "sandstone"),
        "trim": T.tex_material("trim", "trim"),
        "shadow": T.flat_material("shadow", (0.05, 0.035, 0.025), 0.0, 0.95),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
    })
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o)
    lods = stepwell()
    print(f"[stepwell] {export.export_glb(lods, os.path.join(OUT, 'stepwell.gltf'))}")


if __name__ == "__main__":
    main()

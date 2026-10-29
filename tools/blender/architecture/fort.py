"""Garh Qila: the hill fort's masonry over the map's colliders (india.rs `fort`). Curtain walls
with a battered outer face, a moulded plinth, string courses and a corbelled cornice under a
parapet of kangura merlons with loopholes; round corner bastions, half-round bastions standing
out from the walls and two octagonal towers flanking the gate (Pol), which has a cusped pointed
arch in a framed portal and studded doors standing open; a postern on the west; and the
Diwan-i-Aam, a hall of audience: a plinth, three rows of fluted pillars carrying cusped arches,
a flat roof with a drip-shade eave and a kangura parapet.

The plan constants and the tower / parapet / merlon layout repeat india.rs (`FORT_*`,
`fort_towers`, `fort_parapet_runs`, `fort_merlons`, `fort_ring`): keep the two in step.
Model origin at FORT_C on the summit (FORT_TOP). Blender X = game X, Blender Y = -game Z.

blender -b --factory-startup -P tools/blender/architecture/fort.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import buddhist as B  # noqa: E402
import export  # noqa: E402
import ornaments as O  # noqa: E402
import stepwell as S  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402

OUT = os.path.join(T.ROOT, "assets", "models", "props")
HW, HD = 27.0, 21.0
H, WT = 8.0, 2.4            # wall walk height, wall thickness
BATTER = 0.4
STEP = 2.2                  # merlon spacing
GATE = (4.4, 5.5)
POSTERN = (3.0, 3.6)
DIWAN_COLS = (-8.4, -5.04, -1.68, 1.68, 5.04, 8.4)
DIWAN_ROWS = (-11.4, -14.6, -17.8)      # game z


# ------------------------------------------------------------------ plan (as india.rs)

def towers():
    t = WT / 2
    v = [((x, z), 4.2) for x, z in ((-HW, -HD), (HW, -HD), (HW, HD), (-HW, HD))]
    v += [(c, 3.2) for c in ((-6.0, -HD - t), (6.0, -HD - t), (HW + t, 0.0), (-HW - t, -10.5), (-HW - t, 10.5))]
    v += [((x, HD), 2.4) for x in (-5.0, 5.0)]
    return v


def sides():
    """(a, c, openings as (at, width, top)) clockwise, game (x, z)."""
    return [((-HW, HD), (HW, HD), [(HW, *GATE)]), ((HW, HD), (HW, -HD), []),
            ((HW, -HD), (-HW, -HD), []), ((-HW, -HD), (-HW, HD), [(HD, *POSTERN)])]


def parapet_runs(a, c):
    a, c = Vector(a), Vector(c)
    ln = (c - a).length
    d = (c - a) / ln
    cuts = []
    for p, r in towers():
        rel = Vector(p) - a
        along = rel.dot(d)
        off = abs(rel.x * d.y - rel.y * d.x)
        if off < r + WT and -r < along < ln + r:
            cuts.append((along - r + 0.3, along + r - 0.3))
    cuts.sort()
    runs, s = [], 0.0
    for c0, c1 in cuts:
        if c0 > s + 0.5:
            runs.append((s, c0))
        s = max(s, c1)
    if ln > s + 0.5:
        runs.append((s, ln))
    return runs


def merlons(s, e):
    n = math.floor((e - s) / STEP)
    off = (e - s - (n - 1) * STEP) / 2
    return [s + off + i * STEP for i in range(max(n, 0))]


def ring(c, r):
    n = max(round(2 * math.pi * r / STEP), 6)
    rr = r - 0.4
    gx, gz = HW + WT / 2, HD + WT / 2
    out = []
    for k in range(n):
        a = (k + 0.5) * 2 * math.pi / n
        px, pz = c[0] + math.cos(a) * rr, c[1] + math.sin(a) * rr
        if not (abs(px) < gx and abs(pz) < gz):
            out.append((k, a, 2 * rr * math.sin(math.pi / n) + 0.06))
    return out


def wall_space(a, c):
    """Wall space -> model: X along the wall from a, Y inward (outer face toward -Y), Z up."""
    a, c = Vector(a), Vector(c)
    d = (c - a).normalized()
    n = Vector((-d.y, d.x))                  # outward, game (x, z)
    M = Matrix(((d.x, -n.x, 0, a.x), (-d.y, n.y, 0, -a.y), (0, 0, 1, 0), (0, 0, 0, 1)))
    return M


def outer_y(z):
    return -WT / 2 + BATTER * z / H


# ------------------------------------------------------------------ pieces

def kangura(kit, M, x, z0, y0, y1, s=1.0):
    """A kangura merlon: a rounded leaf with a pointed tip and a loophole, 1.1 x 0.9 (x s)."""
    w, sh = 0.55 * s, 0.45 * s
    pts = [(-w, 0.0), (w, 0.0), (w, sh)]
    for k in range(1, 24):
        th = math.pi * k / 24
        tip = 0.12 * s * max(0.0, 1 - abs(th - math.pi / 2) / 0.35)
        pts.append((w * math.cos(th), sh + sh * math.sin(th) + tip))
    pts.append((-w, sh))
    o = O.solid_from_outline("kangura", pts, y0, y1, "stone")
    kit.add(B.transform(o, M @ Matrix.Translation((x, 0, z0))))
    if s > 0.8:
        kit.add(B.transform(T.box("slit", (x - 0.05, y0 - 0.015, z0 + 0.15), (x + 0.05, y0 + 0.06, z0 + 0.6), "shadow"), M))


def band(kit, M, x0, x1, z0, z1, proud, back, strip):
    """A moulding along the outer face from z0 to z1, standing `proud` of it."""
    st = []
    for z in (z0, z1):
        y = outer_y(z)
        st.append((z, T.rect((x1 - x0) / 2, (back + proud) / 2, (x0 + x1) / 2, y - proud + (back + proud) / 2)))
    kit.add(B.transform(T.molded("band", [(st[0][0], st[0][1], strip), (st[1][0], st[1][1], None)]), M))


def curtain(kit, a, c, ops):
    M = wall_space(a, c)
    ln = (Vector(c) - Vector(a)).length

    def prism(x0, x1, z0, z1):
        st = [(z, T.rect((x1 - x0) / 2, (WT / 2 - outer_y(z)) / 2, (x0 + x1) / 2, (WT / 2 + outer_y(z)) / 2)) for z in (z0, z1)]
        kit.add(B.transform(T.molded("curtain", [(st[0][0], st[0][1], None), (st[1][0], st[1][1], None)]), M))

    cur = 0.0
    gaps = []
    for at, w, top in sorted(ops):
        s, e = at - w / 2, at + w / 2
        prism(cur, s, 0.0, H)
        prism(s, e, top, H)
        cur = e
        # Bands stop at the portal's frame.
        gaps.append((s - 0.45, e + 0.45, top))
    prism(cur, ln, 0.0, H)

    def runs_below(zt):
        out, s = [], -0.2
        for g0, g1, top in gaps:
            if zt < top + 0.5:
                out.append((s, g0))
                s = g1
        out.append((s, ln + 0.2))
        return out

    for x0, x1 in runs_below(0.45):
        band(kit, M, x0, x1, 0.0, 0.45, 0.25, 0.6, "padma")
    for x0, x1 in runs_below(5.25):
        band(kit, M, x0, x1, 5.0, 5.25, 0.15, 0.3, "dentil")
    band(kit, M, -0.2, ln + 0.2, 7.55, 7.95, 0.3, 0.3, "lata")
    # Inner coping at the wall walk's edge.
    kit.add(B.transform(T.box("coping", (-0.2, WT / 2 - 0.15, H - 0.25), (ln + 0.2, WT / 2 + 0.12, H + 0.05), "stone"), M))
    # Parapet and merlons, and corbels under the cornice.
    y0, y1 = outer_y(H), outer_y(H) + 0.6
    for s, e in parapet_runs(a, c):
        kit.add(B.transform(T.box("parapet", (s, y0, H), (e, y1, H + 1.0), "stone"), M))
        for x in merlons(s, e):
            kangura(kit, M, x, H + 1.0, y0, y1)
        for k in range(int((e - s) / 1.1)):
            x = s + 0.55 + k * 1.1
            yo = outer_y(7.4)
            kit.add(B.transform(T.box("corbel", (x - 0.1, yo - 0.25, 7.15), (x + 0.1, yo + 0.1, 7.55), "trim"), M))
    return M


def tower(kit, c, r, seg):
    cx, cy = c[0], -c[1]

    def rad(z):
        return r + BATTER * (1 - z / H)
    kit.add(T.lathe("tower", [(rad(0), 0.0), (r, H + 0.02), (0.0, H + 0.02)], seg, (cx, cy)))
    for z0, z1, proud, strip in ((0.0, 0.45, 0.25, "padma"), (5.0, 5.25, 0.15, "dentil"), (7.55, 7.95, 0.3, "lata")):
        kit.add(T.lathe("tband", [(rad(z0), z0), (rad(z0) + proud, z0), (rad(z1) + proud, z1), (rad(z1), z1)], seg, (cx, cy), uv_strip=strip))
    for k, a, chord in ring(c, r):
        # Piece frame: X along the tangent, Y inward, Z up; parapet tops alternate by 1 cm so
        # neighbours' overlapping corners never share a plane.
        p = (c[0] + math.cos(a) * (r - 0.4), c[1] + math.sin(a) * (r - 0.4))
        Mr = Matrix(((math.sin(a), -math.cos(a), 0, p[0]), (math.cos(a), math.sin(a), 0, -p[1]), (0, 0, 1, 0), (0, 0, 0, 1)))
        zt = H + 1.02 + 0.01 * (k % 2)
        kit.add(B.transform(T.box("rparapet", (-chord / 2, -0.3, H + 0.02), (chord / 2, 0.3, zt), "stone"), Mr))
        kangura(kit, Mr, 0.0, zt, -0.3, 0.3)


def portal(kit, M, x, half, top, spring, doors):
    """Cusped pointed arch in the rectangular hole at x (wall space): piers, carved spandrel,
    a rounded rim, a framing band over it and, for the gate, iron-studded doors standing open."""
    a = half - 0.03
    for sx in (-1, 1):
        x0, x1 = sorted((x + sx * a, x + sx * (half + 0.4)))
        kit.add(B.transform(T.box("pier", (x0, -WT / 2 - 0.3, 0.0), (x1, WT / 2 + 0.02, top + 0.1), "stone"), M))
    inner = O.cusped_arch(a, spring, top - 0.05, lobes=9, cusp=0.09)
    sp = O.solid_from_outline("spandrel", inner + [(a, top + 0.02), (-a, top + 0.02)], -WT / 2 - 0.25, WT / 2 + 0.01, "trim", strip="lata", u_scale=1.2)
    kit.add(B.transform(sp, M @ Matrix.Translation((x, 0, 0))))
    for y in (-WT / 2 - 0.27, WT / 2 + 0.03):
        kit.add(B.transform(O.rim_sweep("rim", inner, y, 0.06), M @ Matrix.Translation((x, 0, 0))))
    kit.add(B.transform(T.box("alfiz", (x - half - 0.4, -WT / 2 - 0.3, top + 0.1), (x + half + 0.4, outer_y(top) + 0.3, top + 0.45), "trim"), M))
    if not doors:
        return
    for sx in (-1, 1):
        f0, f1 = x + sx * (a - 0.14), x + sx * (a - 0.04)
        lo, hi = min(f0, f1), max(f0, f1)
        kit.add(B.transform(T.box("leaf", (lo, -1.05, 0.02), (hi, 0.95, spring - 0.1), "wood"), M))
        face = lo if sx > 0 else hi
        for z in (0.6, 1.6, 2.6):
            kit.add(B.transform(T.box("strap", (face - 0.015 if sx > 0 else face, -1.05, z - 0.06),
                                      (face if sx > 0 else face + 0.015, 0.95, z + 0.06), "iron"), M))
        # Spikes against elephants, pointing into the passage.
        for y in (-0.6, 0.0, 0.6):
            for z in (1.1, 2.1, 3.0):
                cone = T.lathe("spike", [(0.06, 0.0), (0.0, 0.22)], 8, (0, 0), mat="iron")
                R = Matrix.Rotation(-sx * math.pi / 2, 4, "Y")
                kit.add(B.transform(cone, M @ Matrix.Translation((face, y, z)) @ R))


def eave(kit, x0, x1, y0, y1, z, depth):
    """Chhajja: a thin sloping drip-shade round a roof, its inner edge at z."""
    import bmesh
    for (ax, ay, bx, by, ox, oy) in ((x0, y0, x1, y0, 0, -1), (x1, y0, x1, y1, 1, 0), (x1, y1, x0, y1, 0, 1), (x0, y1, x0, y0, -1, 0)):
        bm = bmesh.new()
        bm.loops.layers.uv.new("UVMap")
        ext = depth
        # Extend along the edge so the corners meet.
        dx, dy = (bx - ax), (by - ay)
        ln = math.hypot(dx, dy)
        ux, uy = dx / ln, dy / ln
        p = [(ax - ux * ext, ay - uy * ext), (bx + ux * ext, by + uy * ext)]
        v = []
        for (px, py) in p:
            v.append((px, py, z))
            v.append((px + ox * depth, py + oy * depth, z - 0.28))
        top = [bm.verts.new(q) for q in (v[0], v[2], v[3], v[1])]
        bot = [bm.verts.new((q[0], q[1], q[2] - 0.08)) for q in (v[0], v[2], v[3], v[1])]
        bm.faces.new(top)
        bm.faces.new(list(reversed(bot)))
        for i in range(4):
            j = (i + 1) % 4
            bm.faces.new((top[j], top[i], bot[i], bot[j]))
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        o = T.new_obj("chhajja", bm, ("stone",))
        T.cube_uvs(o)
        kit.add(o)


def diwan(kit):
    Y = lambda z: -z  # noqa: E731
    cy = 14.6
    kit.add(T.box("plinth", (-9.0, 10.8, 0.0), (9.0, 18.4, 1.0)))
    kit.add(T.molded("pband", [(0.0, T.rect(9.15, 3.95, 0, cy), "padma"), (0.35, T.rect(9.15, 3.95, 0, cy), None)]))
    kit.add(T.molded("pband", [(0.78, T.rect(9.1, 3.9, 0, cy), "dentil"), (0.98, T.rect(9.1, 3.9, 0, cy), None)]))
    for x in DIWAN_COLS:
        for z in DIWAN_ROWS:
            S.pillar_fluted(kit, x, Y(z), 1.0, 4.2)
    spring, apex, top = 3.9, 4.75, 5.22

    def arch(xm, ym, a, along_y):
        inner = O.cusped_arch(a, spring, apex, lobes=7, cusp=0.07)
        R = Matrix.Rotation(math.pi / 2, 4, "Z") if along_y else Matrix.Identity(4)
        M = Matrix.Translation((xm, ym, 0)) @ R
        # 3 cm inside the pillar brackets (±0.2), so their faces never share a plane.
        kit.add(B.transform(O.solid_from_outline("arch", inner + [(a, top), (-a, top)], -0.17, 0.17, "trim", strip="lata", u_scale=1.2), M))
        for y in (-0.19, 0.19):
            kit.add(B.transform(O.rim_sweep("arim", inner, y, 0.04), M))

    for z in DIWAN_ROWS:
        for x0, x1 in zip(DIWAN_COLS, DIWAN_COLS[1:]):
            arch((x0 + x1) / 2, Y(z), (x1 - x0) / 2 - 0.2, False)
    for x in (DIWAN_COLS[0], DIWAN_COLS[-1]):
        for z0, z1 in zip(DIWAN_ROWS, DIWAN_ROWS[1:]):
            arch(x, (Y(z0) + Y(z1)) / 2, abs(z1 - z0) / 2 - 0.2, True)
    # Roof: slab, cornice, drip-shade, parapet with small kanguras (open where the stair lands).
    kit.add(T.box("roof", (-9.3, 10.5, 5.2), (9.3, 18.7, 6.0)))
    kit.add(T.molded("rband", [(5.7, T.rect(9.42, 4.22, 0, cy), "lata"), (5.95, T.rect(9.42, 4.22, 0, cy), None)]))
    eave(kit, -9.42, 9.42, 10.38, 18.82, 5.62, 0.75)
    Mi = Matrix.Identity(4)
    for (x0, y0, x1, y1, axis) in ((-9.3, 10.5, 9.3, 10.8, "x"), (-9.3, 18.4, 9.3, 18.7, "x"), (9.0, 10.8, 9.3, 18.4, "y"),
                                   (-9.3, 10.8, -9.0, 13.5, "y"), (-9.3, 15.7, -9.0, 18.4, "y")):
        kit.add(T.box("rpar", (x0, y0, 6.0), (x1, y1, 6.4)))
        if axis == "x":
            for x in merlons_n(x0, x1, 1.1):
                kangura(kit, Mi, x, 6.4, y0, y1, 0.6)
        else:
            Rz = Matrix.Translation(((x0 + x1) / 2, 0, 0)) @ Matrix.Rotation(math.pi / 2, 4, "Z")
            for y in merlons_n(y0, y1, 1.1):
                kangura(kit, Rz @ Matrix.Translation((y, 0, 0)), 0.0, 6.4, -0.15, 0.15, 0.6)


def merlons_n(s, e, step):
    n = math.floor((e - s) / step)
    off = (e - s - (n - 1) * step) / 2
    return [s + off + i * step for i in range(max(n, 0))]


def fort():
    kit = T.Kit()
    for a, c, ops in sides():
        M = curtain(kit, a, c, ops)
        for at, w, top in ops:
            gate = w > 4.0
            portal(kit, M, at, w / 2, top, 3.6 if gate else 2.5, gate)
    for c, r in towers():
        tower(kit, c, r, 8 if r < 3.0 else 40)
    diwan(kit)
    return B.finish("fort", kit, lods=(0.3,))


def main():
    export.reset_scene()
    red = (0.74, 0.42, 0.32)
    texgen.sandstone(T.TEX, seed=5, tint=red, name="redstone")
    texgen.trim_sheet(T.TEX, seed=6, tint=red, name="redtrim")
    T.MATS.update({
        "stone": T.tex_material("redstone", "redstone"),
        "trim": T.tex_material("redtrim", "redtrim"),
        "shadow": T.flat_material("shadow", (0.04, 0.025, 0.02), 0.0, 0.95),
        "wood": T.flat_material("wood", (0.22, 0.12, 0.06), 0.0, 0.8),
        "iron": T.flat_material("iron", (0.07, 0.07, 0.075), 0.8, 0.45),
    })
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o)
    lods = fort()
    print(f"[fort] {export.export_glb(lods, os.path.join(OUT, 'fort.gltf'))}")


if __name__ == "__main__":
    main()

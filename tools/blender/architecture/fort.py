"""Garh Qila: the hill fort's masonry over the map's colliders (india.rs `fort`). Curtain walls
with a battered outer face, a moulded plinth, string courses and a corbelled cornice under a
parapet of kangura merlons with loopholes; round corner bastions, half-round bastions standing
out from the walls and two octagonal towers flanking the gate (Pol), which has a cusped pointed
arch in a framed portal and studded doors standing open; a postern on the west; and the
Diwan-i-Aam, a hall of audience: a plinth, three rows of fluted pillars carrying cusped arches,
a flat roof with a drip-shade eave and a kangura parapet; two palace wings (mahal) facing each
other across the court, arcaded halls with a bangaldar-roofed pavilion on the roof; and the
court's flagstones.

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


def eave(kit, x0, x1, y0, y1, z, depth, skip=None):
    """Chhajja: a thin sloping drip-shade round a roof, its inner edge at z. `skip`: a side left
    bare (0 = -Y, 1 = +X, 2 = +Y, 3 = -X)."""
    import bmesh
    sides = ((x0, y0, x1, y0, 0, -1), (x1, y0, x1, y1, 1, 0), (x1, y1, x0, y1, 0, 1), (x0, y1, x0, y0, -1, 0))
    for (ax, ay, bx, by, ox, oy) in (sd for k, sd in enumerate(sides) if k != skip):
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


def arch_panel(kit, xm, ym, a, along_y, spring, apex, top):
    """A cusped arch panel between two pillars' brackets, centred at (xm, ym)."""
    inner = O.cusped_arch(a, spring, apex, lobes=7, cusp=0.07)
    R = Matrix.Rotation(math.pi / 2, 4, "Z") if along_y else Matrix.Identity(4)
    M = Matrix.Translation((xm, ym, 0)) @ R
    # 3 cm inside the pillar brackets (±0.2), so their faces never share a plane.
    kit.add(B.transform(O.solid_from_outline("arch", inner + [(a, top), (-a, top)], -0.17, 0.17, "trim", strip="lata", u_scale=1.2), M))
    for y in (-0.19, 0.19):
        kit.add(B.transform(O.rim_sweep("arim", inner, y, 0.04), M))


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
        arch_panel(kit, xm, ym, a, along_y, spring, apex, top)

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


def bangla(kit, cx, cy, L, wd, z0, rise, over=0.3):
    """Bangaldar roof: a curved vault along Y whose ridge droops toward both ends, its eaves
    curling down past the corners."""
    import bmesh
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    nu, nk = 28, 14
    half = L / 2 + over
    grid = []
    for i in range(nu + 1):
        u = -half + 2 * half * i / nu
        f = 1 - 0.4 * (u / half) ** 2
        row = []
        for k in range(nk + 1):
            ang = math.pi * k / nk
            row.append(bm.verts.new((cx - (wd / 2 + over) * math.cos(ang), cy + u, z0 + rise * f * math.sin(ang) - 0.25 * (1 - f))))
        grid.append(row)
    for i in range(nu):
        for k in range(nk):
            fc = bm.faces.new((grid[i][k], grid[i][k + 1], grid[i + 1][k + 1], grid[i + 1][k]))
            for loop in fc.loops:
                co = loop.vert.co
                loop[uv].uv = (co.y / T.TILE, (co.x + co.z) / T.TILE)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    for fc in bm.faces:
        if fc.normal.z < 0:
            fc.normal_flip()
    roof = T.new_obj("bangaldar", bm, ("stone",))
    sol = roof.modifiers.new("s", "SOLIDIFY")
    sol.thickness = 0.1
    bpy.context.view_layer.objects.active = roof
    bpy.ops.object.modifier_apply(modifier="s")
    kit.add(roof)
    T.kalasha(kit, (cx, cy), z0 + rise - 0.04, 0.3)


# Palace wing (mahal) plan, as india.rs `MAHAL_*`: |x| from the court axis, game z.
MAHAL_X = (9.5, 16.5)           # plinth: court face, back
MAHAL_Z = (-10.0, 2.0)
MAHAL_ROWS = (9.9, 12.9)        # pillar rows (|x|)
MAHAL_COLS = (-9.2, -7.12, -5.04, -2.96, -0.88, 1.2)    # the end ones engaged in the end walls (0.8 thick)
MAHAL_FLOOR, MAHAL_ROOF, MAHAL_TOP = 0.6, 4.2, 4.9
MAHAL_GAP = (-8.2, -6.0)        # back parapet open where the outside stair lands
PAV_ROWS = (11.4, 14.6)
PAV_COLS = (-6.2, -4.0, -1.8)
PAV_TOP = 7.5


def mahal(kit, s):
    """A palace wing facing the court: an arcaded hall (dalan) of two pillar rows on a plinth,
    a roof terrace with a kangura parapet, and a bangaldar-roofed pavilion on the terrace.
    s = +1 east wing, -1 west wing."""
    def box(name, xl0, xl1, z0, z1, h0, h1, mat="stone"):
        xa, xb = sorted((s * xl0, s * xl1))
        return kit.add(T.box(name, (xa, -z1, h0), (xb, -z0, h1), mat))
    (xf, xb), (z0, z1) = MAHAL_X, MAHAL_Z
    cx, cy = s * (xf + xb) / 2, -(z0 + z1) / 2
    hx, hy = (xb - xf) / 2, (z1 - z0) / 2
    fl, rf, tp = MAHAL_FLOOR, MAHAL_ROOF, MAHAL_TOP
    box("mplinth", xf, xb, z0, z1, 0.0, fl)
    kit.add(T.molded("mpband", [(0.0, T.rect(hx + 0.12, hy + 0.12, cx, cy), "padma"), (0.3, T.rect(hx + 0.12, hy + 0.12, cx, cy), None)]))
    # End walls (whole depth) and the back wall between them.
    box("mwall", xf, xb, z0, MAHAL_COLS[0], fl, rf)
    box("mwall", xf, xb, MAHAL_COLS[-1], z1, fl, rf)
    box("mwall", xb - 0.4, xb, MAHAL_COLS[0], MAHAL_COLS[-1], fl, rf)
    for xl in MAHAL_ROWS:
        for z in MAHAL_COLS:
            S.pillar_fluted(kit, s * xl, -z, fl, rf - fl)
    spring, apex, top = rf - 1.3, rf - 0.45, rf + 0.02
    for xl in MAHAL_ROWS:
        for za, zb in zip(MAHAL_COLS, MAHAL_COLS[1:]):
            arch_panel(kit, s * xl, -(za + zb) / 2, (zb - za) / 2 - 0.2, True, spring, apex, top)
    xa, xb2 = MAHAL_ROWS
    for z in MAHAL_COLS[1:-1]:
        arch_panel(kit, s * (xa + xb2) / 2, -z, (xb2 - xa) / 2 - 0.2, False, spring, apex, top)
    # Roof slab, cornice band and the drip-shade (none at the back, over the stair).
    box("mroof", xf + 0.1, xb, z0, z1, rf, tp)
    rhx, rcx = (xb - xf - 0.1) / 2, s * (xf + 0.1 + xb) / 2
    kit.add(T.molded("mrband", [(rf + 0.4, T.rect(rhx + 0.12, hy + 0.12, rcx, cy), "lata"), (rf + 0.65, T.rect(rhx + 0.12, hy + 0.12, rcx, cy), None)]))
    ex0, ex1 = sorted((s * (xf + 0.1), s * xb))
    eave(kit, ex0 - 0.12, ex1 + 0.12, -z1 - 0.12, -z0 + 0.12, rf + 0.32, 0.6, skip=1 if s > 0 else 3)
    # Parapet with small kanguras, open at the back where the stair lands.
    pz = tp + 0.7
    xi = xf + 0.1
    runs = [(xi, xi + 0.3, z0, z1, "z"), (xi + 0.3, xb, z0, z0 + 0.3, "x"), (xi + 0.3, xb, z1 - 0.3, z1, "x"),
            (xb - 0.3, xb, z0 + 0.3, MAHAL_GAP[0], "z"), (xb - 0.3, xb, MAHAL_GAP[1], z1 - 0.3, "z")]
    for (a0, a1, b0, b1, along) in runs:
        box("mpar", a0, a1, b0, b1, tp, pz)
        xa_, xb_ = sorted((s * a0, s * a1))
        if along == "x":
            for x in merlons_n(xa_, xb_, 1.1):
                kangura(kit, Matrix.Identity(4), x, pz, -b1, -b0, 0.6)
        else:
            Rz = Matrix.Translation(((xa_ + xb_) / 2, 0, 0)) @ Matrix.Rotation(math.pi / 2, 4, "Z")
            for y in merlons_n(-b1, -b0, 1.1):
                kangura(kit, Rz @ Matrix.Translation((y, 0, 0)), 0.0, pz, -0.15, 0.15, 0.6)
    # Rooftop pavilion: six pillars, an entablature, a bangaldar roof.
    for xl in PAV_ROWS:
        for z in PAV_COLS:
            S.pillar_fluted(kit, s * xl, -z, tp, PAV_TOP - tp)
    px = (PAV_ROWS[0] + PAV_ROWS[1]) / 2
    box("pent", PAV_ROWS[0] - 0.4, PAV_ROWS[1] + 0.4, PAV_COLS[0] - 0.4, PAV_COLS[-1] + 0.4, PAV_TOP, PAV_TOP + 0.25)
    kit.add(T.molded("pband", [(PAV_TOP + 0.08, T.rect((PAV_ROWS[1] - PAV_ROWS[0]) / 2 + 0.45, (PAV_COLS[-1] - PAV_COLS[0]) / 2 + 0.45, s * px,
                                                         -(PAV_COLS[0] + PAV_COLS[-1]) / 2), "dentil"),
                               (PAV_TOP + 0.22, T.rect((PAV_ROWS[1] - PAV_ROWS[0]) / 2 + 0.45, (PAV_COLS[-1] - PAV_COLS[0]) / 2 + 0.45, s * px,
                                                       -(PAV_COLS[0] + PAV_COLS[-1]) / 2), None)]))
    bangla(kit, s * px, -(PAV_COLS[0] + PAV_COLS[-1]) / 2, PAV_COLS[-1] - PAV_COLS[0] + 0.8, PAV_ROWS[1] - PAV_ROWS[0] + 0.8,
           PAV_TOP + 0.25, 1.0)


def paving(kit):
    """Flagstones over the court and the gate passage, 2 cm proud of the summit."""
    import bmesh
    for (x0, x1, y0, y1) in ((-HW + WT / 2, HW - WT / 2, -HD + WT / 2, HD - WT / 2), (-GATE[0] / 2 + 0.03, GATE[0] / 2 - 0.03, -HD - WT / 2, -HD + WT / 2)):
        o = T.box("paving", (x0, y0, -0.1), (x1, y1, 0.02), "paving")
        uvl = o.data.uv_layers[0]
        for loop, d in zip(o.data.loops, uvl.data):
            c = o.data.vertices[loop.vertex_index].co
            d.uv = (c.x / 3.0, c.y / 3.0)
        kit.add(o)


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
    for side in (1, -1):
        mahal(kit, side)
    paving(kit)
    return B.finish("fort", kit, lods=(0.3, 0.08))


def main():
    export.reset_scene()
    red = (0.74, 0.42, 0.32)
    texgen.sandstone(T.TEX, seed=5, tint=red, name="redstone")
    texgen.trim_sheet(T.TEX, seed=6, tint=red, name="redtrim")
    texgen.sandstone(T.TEX, seed=9, tint=(0.80, 0.66, 0.50), name="paving")
    T.MATS.update({
        "stone": T.tex_material("redstone", "redstone"),
        "trim": T.tex_material("redtrim", "redtrim"),
        "shadow": T.flat_material("shadow", (0.04, 0.025, 0.02), 0.0, 0.95),
        "wood": T.flat_material("wood", (0.22, 0.12, 0.06), 0.0, 0.8),
        "iron": T.flat_material("iron", (0.07, 0.07, 0.075), 0.8, 0.45),
        "paving": T.tex_material("paving", "paving"),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
    })
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o)
    lods = fort()
    print(f"[fort] {export.export_glb(lods, os.path.join(OUT, 'fort.gltf'))}")


if __name__ == "__main__":
    main()

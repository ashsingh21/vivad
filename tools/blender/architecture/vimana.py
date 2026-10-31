"""Pushpaka Vimana: the flying palace-chariot every warrior rides in on. A boat-shaped hull of
vermilion lacquer banded in gold, a hamsa (swan) rising from the prow and a fanned peacock tail at
the stern; on the teak deck a railed, pillared mandapa holding a throne, under three receding
storeys (talas) ringed with kutas and shalas and crowned by a ribbed dome and kalasha; a chhatri
fore and aft, saffron pennants, hanging bells, and a glowing lotus under the keel that holds it up.

The origin is the vimana's position in the sim (`Match::vimana_pos`), with the deck at z = 0; it
flies toward Blender +Y (game -Z, the way `look_to` points it). ~29 m from beak to tail, 10 m across, 16 m tall.

blender -b --factory-startup -P tools/blender/architecture/vimana.py
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

import buddhist as B  # noqa: E402
import export  # noqa: E402
import geo  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402
from vernacular import cylinder_between, proj_uvs  # noqa: E402

OUT = os.path.join(T.ROOT, "assets", "models", "props")
L, W = 11.0, 5.0          # hull half-length and half-beam at the deck
SIDE = 16                 # plan points per side of the hull
PAV = (3.0, 4.4)          # pillar rows of the mandapa (x, y)
PAV_Z0, PAV_H = 0.6, 3.4  # plinth top and pillar height
ROOF = PAV_Z0 + PAV_H     # underside of the mandapa ceiling


# ------------------------------------------------------------------ helpers

def remat(o, *names):
    """Swap the material slots of a kit piece (molded: trim, stone) for other kit materials."""
    for i, n in enumerate(names):
        o.data.materials[i] = T.MATS[n]
    return o


def half_beam(y, w=W, ln=L):
    """Half-width of the lens-shaped hull plan at y: full amidships, coming to a point at both ends."""
    t = min(abs(y) / ln, 1.0)
    return w * (1.0 - t ** 2.2) ** 0.62


def hull_plan(w, ln):
    """CCW ring: up the starboard (+x) side from the stern tip to the bow tip, back down to port."""
    ys = [-ln + 2 * ln * (k + 0.5) / SIDE for k in range(SIDE)]
    ring = [(0.0, -ln)]
    ring += [(half_beam(y, w, ln), y) for y in ys]
    ring += [(0.0, ln)]
    ring += [(-half_beam(y, w, ln), y) for y in reversed(ys)]
    return ring


def cone_between(name, a, b, r0, r1, mat, seg=12):
    a, b = Vector(a), Vector(b)
    d = b - a
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r0, radius2=r1, depth=d.length)
    o = T.new_obj(name, bm, (mat,))
    o.rotation_mode = "QUATERNION"
    o.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(d.normalized())
    o.location = (a + b) / 2
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    o.select_set(False)
    proj_uvs(o, 1.0)
    return o


def tube(name, points, radii, mat, seg=14):
    """A closed tube along `points` (Vectors) with a radius per point, capped at both ends."""
    frames = geo.path_frames(points, Vector((1, 0, 0)))
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    rings = []
    for (o, x, y), r in zip(frames, radii):
        rings.append([bm.verts.new(o + (x * math.cos(a) + y * math.sin(a)) * r)
                      for a in (2 * math.pi * k / seg for k in range(seg))])
    arc = 0.0
    for k in range(len(rings) - 1):
        step = (points[k + 1] - points[k]).length
        for i in range(seg):
            j = (i + 1) % seg
            f = bm.faces.new((rings[k][i], rings[k][j], rings[k + 1][j], rings[k + 1][i]))
            for loop, t in zip(f.loops, ((i / seg, arc), ((i + 1) / seg, arc), ((i + 1) / seg, arc + step), (i / seg, arc + step))):
                loop[uv].uv = t
        arc += step
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return T.new_obj(name, bm, (mat,))


def bell(kit, x, y, z_top):
    """A small temple bell (ghanta) on a short chain, hanging from z_top."""
    kit.add(cylinder_between("chain", (x, y, z_top), (x, y, z_top - 0.35), 0.015, "gold", 6))
    z = z_top - 0.35
    kit.add(T.lathe("bell", [(0.03, z), (0.07, z - 0.04), (0.09, z - 0.16), (0.13, z - 0.27), (0.13, z - 0.3), (0.0, z - 0.3)],
                    12, (x, y), mat="gold"))


def pennant(kit, x, y, z_base, height, length, lean=1.0):
    """A gold pole with a swallow-tailed saffron pennant streaming aft (-Y)."""
    top = z_base + height
    kit.add(cylinder_between("pole", (x, y, z_base), (x, y, top), 0.045, "gold", 8))
    kit.add(B.uv_sphere("polenub", (x, y, top + 0.05), (0.08, 0.08, 0.08), "gold", 8, 6))
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    a = bm.verts.new((x, y, top - 0.05))
    b = bm.verts.new((x, y, top - 0.85))
    c = bm.verts.new((x + 0.15 * lean, y - length, top - 0.25))
    d = bm.verts.new((x + 0.1 * lean, y - length * 0.78, top - 0.5))
    e = bm.verts.new((x + 0.2 * lean, y - length, top - 0.8))
    bm.faces.new((a, b, d))
    bm.faces.new((a, d, c))
    bm.faces.new((b, e, d))
    kit.add(T.new_obj("pennant", bm, ("cloth",)))


def kuta(kit, x, y, z, s):
    """A square domed aedicule (kuta) for the corners of a storey's parapet."""
    kit.add(T.box("kuta_base", (x - 0.42 * s, y - 0.42 * s, z), (x + 0.42 * s, y + 0.42 * s, z + 0.5 * s), "stone"))
    kit.add(T.lathe("kuta_ledge", [(0.5 * s, z + 0.5 * s), (0.5 * s, z + 0.6 * s)], 4, (x, y), square=True, uv_strip="bead"))
    dome = [(0.44 * s, z + 0.6 * s), (0.46 * s, z + 0.75 * s), (0.36 * s, z + 0.98 * s), (0.18 * s, z + 1.12 * s), (0.0, z + 1.16 * s)]
    kit.add(T.lathe("kuta_dome", dome, 4, (x, y), square=True, uv_strip="gavaksha"))
    kit.add(T.lathe("kuta_stupi", [(0.08 * s, z + 1.12 * s), (0.11 * s, z + 1.22 * s), (0.0, z + 1.42 * s)], 10, (x, y), mat="gold"))


def shala(kit, axis, x, y, z, length, s):
    """A barrel-roofed pavilion (shala) on the middle of a parapet side; its lower half sits in the
    storey below. `axis` is the direction the vault runs."""
    half = Vector((length / 2, 0, 0)) if axis == "x" else Vector((0, length / 2, 0))
    c = Vector((x, y, z + 0.1 * s))
    kit.add(cylinder_between("shala", c - half, c + half, 0.48 * s, "stone", 16))
    for e in (-1, 1):
        kit.add(cylinder_between("shala_end", c + half * e, c + half * (e * 1.06), 0.55 * s, "trim", 16))
    for k in (-1, 0, 1):
        p = c + half * (0.55 * k) + Vector((0, 0, 0.45 * s))
        kit.add(T.lathe("shala_stupi", [(0.07 * s, p.z), (0.1 * s, p.z + 0.1 * s), (0.0, p.z + 0.32 * s)], 10, (p.x, p.y), mat="gold"))


# ------------------------------------------------------------------ hull, prow, stern

def hull(kit):
    # (z, width scale, length scale, strip), keel up: the band above each station is carved with
    # its strip, gold where carved, vermilion lacquer where plain. Open at the top: the deck closes it.
    st = [(-3.3, 0.32, 0.72, None), (-2.85, 0.58, 0.83, "rope"), (-2.65, 0.66, 0.86, None),
          (-1.85, 0.87, 0.93, "lata"), (-1.45, 0.92, 0.95, None), (-0.48, 0.99, 0.995, "bead"),
          (-0.32, 1.0, 1.0, "padma"), (0.0, 1.0, 1.0, None)]
    stations = [(z, hull_plan(W * sw, L * sl), strip) for z, sw, sl, strip in st]
    kit.add(remat(T.molded("hull", stations, cap_top=False), "gtrim", "lacquer"))
    # Teak deck: quads across the beam from port to starboard, so the baked shade has vertices to
    # land on everywhere.
    ring = hull_plan(W, L)
    n = len(ring)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    stern, bow = bm.verts.new((0, -L, 0)), bm.verts.new((0, L, 0))
    starboard = [bm.verts.new((x, y, 0.0)) for x, y in ring[1:SIDE + 1]]
    port = [bm.verts.new((x, y, 0.0)) for x, y in reversed(ring[SIDE + 2:n])]
    rows = [(stern, stern)] + list(zip(port, starboard)) + [(bow, bow)]
    for (pa, sa), (pb, sb) in zip(rows, rows[1:]):
        quad = [pa, sa, sb, pb]
        uniq = [v for i, v in enumerate(quad) if v not in quad[:i]]
        f = bm.faces.new(uniq)
        for loop in f.loops:
            loop[uv].uv = (loop.vert.co.y / 2.0, loop.vert.co.x / 2.0)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    for f in bm.faces:
        if f.normal.z < 0:
            f.normal_flip()
    kit.add(T.new_obj("deck", bm, ("wood",)))
    # Gold rubbing strakes along the sides, proud of the lacquer.
    for z, sw, sl in ((-0.9, 0.975, 0.985), (-2.3, 0.78, 0.9)):
        for side in (-1, 1):
            pts = [Vector((side * half_beam(y, W * sw, L * sl) * 1.0 + side * 0.05, y, z))
                   for y in (L * sl * (-0.92 + 1.84 * k / 30) for k in range(31))]
            kit.add(tube("strake", pts, [0.07] * len(pts), "gold", 8))


def railing(kit):
    """A vedika round the deck edge: gold posts with lotus knobs and ivory rails, open at the bow
    and stern tips where the swan's neck and the tail rise."""
    inset = 0.28
    ys = [-L * 0.86 + 2 * L * 0.86 * k / 26 for k in range(27)]
    for side in (-1, 1):
        pts = [Vector((side * (half_beam(y) - inset), y, 0.0)) for y in ys]
        for p in pts:
            kit.add(T.lathe("rpost", [(0.07, 0.0), (0.07, 0.72), (0.1, 0.78), (0.0, 0.92)], 8, (p.x, p.y), mat="gold"))
        for zb, r in ((0.3, 0.035), (0.55, 0.035), (0.76, 0.055)):
            kit.add(tube("rail", [p + Vector((0, 0, zb)) for p in pts], [r] * len(pts), "ivory_rail" if zb < 0.7 else "gold", 8))


def hamsa_prow(kit):
    """The swan: a neck curving up from the bow tip, a head looking ahead, a gold beak and crest."""
    ctrl = [Vector(p) for p in ((0, L - 1.6, -0.9), (0, L + 0.3, -0.3), (0, L + 1.4, 1.4), (0, L + 1.2, 3.2),
                                (0, L + 1.5, 4.6), (0, L + 2.3, 5.3))]
    pts = geo.resample_even(geo.catmull_rom(ctrl, 60), 28)
    radii = [0.75 - 0.4 * (k / 27) ** 0.8 for k in range(28)]
    kit.add(tube("neck", pts, radii, "gold", 16))
    head = pts[-1] + Vector((0, 0.35, 0.05))
    kit.add(B.uv_sphere("head", head, (0.42, 0.62, 0.45), "gold", 16, 10))
    kit.add(cone_between("beak", head + Vector((0, 0.45, -0.05)), head + Vector((0, 1.35, -0.2)), 0.2, 0.02, "gtrim", 10))
    for side in (-1, 1):
        kit.add(B.uv_sphere("eye", head + Vector((side * 0.36, 0.2, 0.12)), (0.07, 0.09, 0.09), "jewel", 8, 6))
    # Crest: three gold plumes swept back from the crown.
    for k, a in enumerate((-0.35, 0.0, 0.35)):
        tip = head + Vector((a * 0.5, -0.9, 0.9 - abs(a) * 0.5))
        kit.add(cone_between("crest", head + Vector((0, -0.1, 0.35)), tip, 0.09, 0.02, "gold", 8))
        kit.add(B.uv_sphere("crestgem", tip, (0.08, 0.08, 0.08), "jewel", 8, 6))
    # A jewelled collar where the neck leaves the hull.
    t = (pts[7] - pts[5]).normalized() * 0.14
    kit.add(tube("collar", [pts[6] - t, pts[6] + t], [radii[6] + 0.08] * 2, "gtrim", 16))


def peacock_stern(kit):
    """A tail curling up from the stern tip and a fan of plumes, each with an enamelled eye."""
    ctrl = [Vector(p) for p in ((0, -L + 1.6, -0.8), (0, -L - 0.4, -0.2), (0, -L - 1.0, 1.4), (0, -L - 0.6, 2.6))]
    pts = geo.resample_even(geo.catmull_rom(ctrl, 40), 18)
    radii = [0.65 - 0.3 * k / 17 for k in range(18)]
    kit.add(tube("tail", pts, radii, "gold", 14))
    root = pts[-1]
    n = 9
    for k in range(n):
        a = math.radians(-70 + 140 * k / (n - 1))
        d = Vector((math.sin(a) * 1.0, -0.55, math.cos(a) * 1.0)).normalized()
        tip = root + d * (3.0 - 0.6 * abs(math.sin(a)))
        kit.add(cone_between("plume", root, tip, 0.12, 0.05, "gold", 8))
        kit.add(B.uv_sphere("eye_ring", tip, (0.36, 0.12, 0.42), "gtrim", 12, 8))
        kit.add(B.uv_sphere("eye", tip + d * 0.02 + Vector((0, -0.08, 0)), (0.2, 0.08, 0.24), "jewel", 10, 6))


def lotus_keel(kit):
    """An upturned lotus of light under the keel, and a smaller one fore and aft."""
    # The rims sit just inside the flat of the keel, so only the cups show.
    z = -3.25
    for y, s in ((0.0, 1.0), (6.0, 0.6), (-6.0, 0.6)):
        prof = [(0.0, z - 1.45 * s), (0.9 * s, z - 1.35 * s), (1.7 * s, z - 0.9 * s), (2.0 * s, z - 0.45 * s), (1.5 * s, z)]
        kit.add(T.lathe("lotus", prof, 64, (0.0, y), ribs=8, rib_depth=0.3, mat="glow"))


# ------------------------------------------------------------------ the palace

def mandapa(kit):
    px, py = PAV
    # Plinth: a moulded base with steps down to the deck at the front.
    kit.add(T.molded("plinth", [(0.0, T.rect(px + 0.7, py + 0.7), "padma"), (0.3, T.rect(px + 0.62, py + 0.62), "bead"),
                                (PAV_Z0, T.rect(px + 0.62, py + 0.62), None)]))
    for k in range(2):
        z = PAV_Z0 * k / 2
        kit.add(T.box("step", (-1.0, py + 0.62, z), (1.0, py + 1.0 + 0.35 * (1 - k), z + PAV_Z0 / 2), "stone"))
    for x in (-px, 0.0, px):
        for y in (-py, 0.0, py):
            if x != 0.0 or y != 0.0:
                T.pillar(kit, x, y, PAV_Z0, PAV_H)
    # Entablature and the eave (chhajja) all round, bells hanging from its corners.
    kit.add(T.molded("entab", [(ROOF, T.rect(px + 0.55, py + 0.55), "lata"), (ROOF + 0.45, T.rect(px + 0.55, py + 0.55), "dentil"),
                               (ROOF + 0.62, T.offset_poly(T.rect(px + 0.55, py + 0.55), 0.55), "plain"),
                               (ROOF + 0.74, T.offset_poly(T.rect(px + 0.55, py + 0.55), 0.55), None),
                               (ROOF + 1.0, T.rect(px + 0.4, py + 0.4), None)]))
    ex, ey = px + 0.55 + 0.45, py + 0.55 + 0.45
    for x in (-ex, 0.0, ex):
        for y in (-ey, 0.0, ey):
            if x == 0.0 and y == 0.0:
                continue
            bell(kit, x, y, ROOF + 0.6)
    # The throne at the heart of the hall, with a parasol over it.
    z = PAV_Z0
    kit.add(T.molded("throne_base", [(z, T.rect(0.9, 0.7, 0, -0.6), "padma"), (z + 0.45, T.rect(0.8, 0.6, 0, -0.6), None)]))
    kit.add(remat(T.molded("seat", [(z + 0.45, T.rect(0.75, 0.55, 0, -0.6), "bead"), (z + 0.6, T.rect(0.75, 0.55, 0, -0.6), None)]),
                  "gtrim", "cushion"))
    kit.add(T.box("throne_back", (-0.7, -1.25, z + 0.45), (0.7, -1.05, z + 1.9), "gold"))
    kit.add(B.uv_sphere("prabha", (0.0, -1.1, z + 1.9), (0.75, 0.12, 0.6), "gold", 16, 8))
    for side in (-1, 1):
        kit.add(T.box("arm", (side * 0.75 - 0.1, -1.05, z + 0.6), (side * 0.75 + 0.1, -0.15, z + 1.0), "gold"))
        kit.add(B.uv_sphere("armknob", (side * 0.75, -0.12, z + 1.05), (0.13, 0.13, 0.13), "gold", 10, 6))
    kit.add(cylinder_between("chhatra_pole", (0.0, -1.35, z + 0.6), (0.0, -1.35, ROOF - 0.35), 0.035, "gold", 8))
    kit.add(T.lathe("chhatra", [(0.0, ROOF - 0.2), (1.1, ROOF - 0.55), (1.15, ROOF - 0.65), (0.05, ROOF - 0.45)], 24, (0.0, -1.0),
                    ribs=12, rib_depth=0.05, mat="cushion"))


def storeys(kit):
    """Three receding talas over the hall, each walled with a carved band and capped with a
    cornice carrying kutas at the corners and shalas between, then the dome."""
    z = ROOF + 1.0
    tiers = [(PAV[0] - 0.1, PAV[1] - 0.6, 1.5), (PAV[0] - 1.0, PAV[1] - 1.7, 1.25), (PAV[0] - 1.85, PAV[1] - 2.75, 1.05)]
    for k, (hx, hy, h) in enumerate(tiers):
        s = 1.0 - 0.17 * k
        kit.add(T.molded(f"tala{k}", [(z, T.rect(hx, hy), "bead"), (z + 0.15, T.rect(hx - 0.1, hy - 0.1), "gavaksha"),
                                      (z + h - 0.35, T.rect(hx - 0.1, hy - 0.1), "dentil"),
                                      (z + h - 0.15, T.offset_poly(T.rect(hx - 0.1, hy - 0.1), 0.35), "plain"),
                                      (z + h, T.offset_poly(T.rect(hx - 0.1, hy - 0.1), 0.35), None)]))
        top = z + h
        # Kutas just inside the cornice's corners; a shala on each side; the next storey inside.
        kx, ky = hx - 0.1 - 0.42 * s + 0.25, hy - 0.1 - 0.42 * s + 0.25
        if k < len(tiers) - 1:
            for sx in (-1, 1):
                for sy in (-1, 1):
                    kuta(kit, sx * kx, sy * ky, top, s)
            for sy in (-1, 1):
                shala(kit, "x", 0.0, sy * (ky + 0.05), top, max(1.2, hx * 0.8), s)
            if hy - hx > 0.4:
                for sx in (-1, 1):
                    shala(kit, "y", sx * (kx + 0.05), 0.0, top, max(1.0, (hy - hx) + 0.6), s * 0.85)
        z = top
    # Griva and an octagonal ribbed dome (the shikhara of a Dravidian vimana), kalasha and flag.
    r = 1.2
    kit.add(T.lathe("griva", [(r * 0.8, z), (r * 0.8, z + 0.6), (r * 0.9, z + 0.7)], 8, (0, 0), uv_strip="padma"))
    dome = [(r * 0.9, z + 0.7), (r * 1.08, z + 1.0), (r * 1.12, z + 1.4), (r * 1.0, z + 1.85), (r * 0.72, z + 2.25),
            (r * 0.38, z + 2.5), (r * 0.12, z + 2.6), (0.0, z + 2.62)]
    kit.add(T.lathe("dome", dome, 32, (0, 0), ribs=16, rib_depth=0.04, mat="gold"))
    T.kalasha(kit, (0.0, 0.0), z + 2.55, 0.75)
    pennant(kit, 0.0, 0.0, z + 3.4, 2.4, 2.6)


def chhatri(kit, cy, s):
    """A domed kiosk on the deck: four slim pillars, a wide eave, a ribbed gold dome."""
    p, h = 0.55 * s, 1.5 * s
    kit.add(T.molded("cbase", [(0.0, T.rect(0.75 * s, 0.75 * s, 0, cy), "padma"), (0.18 * s, T.rect(0.75 * s, 0.75 * s, 0, cy), None)]))
    for sx in (-1, 1):
        for sy in (-1, 1):
            c = (sx * p, cy + sy * p)
            kit.add(T.lathe("ccol", [(0.09 * s, 0.18 * s), (0.08 * s, 0.26 * s), (0.06 * s, 0.34 * s), (0.06 * s, h - 0.3 * s),
                                     (0.1 * s, h - 0.18 * s), (0.08 * s, h - 0.08 * s), (0.12 * s, h)], 10, c, uv_strip="bead"))
    kit.add(T.molded("cchhajja", [(h, T.rect(0.88 * s, 0.88 * s, 0, cy), "dentil"), (h + 0.08 * s, T.rect(0.88 * s, 0.88 * s, 0, cy), "plain"),
                                  (h + 0.2 * s, T.rect(0.62 * s, 0.62 * s, 0, cy), None)]))
    kit.add(T.molded("cdrum", [(h + 0.2 * s, T.rect(0.58 * s, 0.58 * s, 0, cy), "padma"), (h + 0.36 * s, T.rect(0.58 * s, 0.58 * s, 0, cy), None)]))
    dome = [(0.62 * s, h + 0.36 * s), (0.63 * s, h + 0.48 * s), (0.58 * s, h + 0.66 * s), (0.44 * s, h + 0.86 * s),
            (0.26 * s, h + 0.98 * s), (0.09 * s, h + 1.04 * s), (0.0, h + 1.05 * s)]
    kit.add(T.lathe("cdome", dome, 32, (0.0, cy), ribs=16, rib_depth=0.03, mat="gold"))
    T.kalasha(kit, (0.0, cy), h + 1.03 * s, 0.22 * s)
    for sx in (-1, 1):
        for sy in (-1, 1):
            bell(kit, sx * 0.84 * s, cy + sy * 0.84 * s, h)


def pushpaka():
    kit = T.Kit()
    hull(kit)
    railing(kit)
    hamsa_prow(kit)
    peacock_stern(kit)
    lotus_keel(kit)
    mandapa(kit)
    storeys(kit)
    chhatri(kit, 7.7, 1.9)
    chhatri(kit, -7.5, 1.6)
    # Pennants at the railing's ends, fore and aft, streaming aft.
    for side in (-1, 1):
        for y in (L * 0.62, -L * 0.62):
            x = side * (half_beam(y) - 0.6)
            pennant(kit, x, y, 0.0, 3.4, 1.8, side)
    obj = geo.join(kit.objs, "vimana_LOD0")
    obj.data.set_sharp_from_angle(angle=math.radians(40))
    for p in obj.data.polygons:
        p.use_smooth = True
    # Shade only: a divine chariot gets no rain streaks or ground grime.
    T.bake_weathering(obj, streak_k=0.0)
    export.triangulate(obj)
    return export.make_lods(obj, (0.35,))


def gold_tex_material(name, prefix):
    m = T.tex_material(name, prefix)
    p = m.node_tree.nodes["Principled BSDF"]
    for link in list(p.inputs["Metallic"].links):
        m.node_tree.links.remove(link)
    p.inputs["Metallic"].default_value = 0.9
    return m


def glow_material():
    m = T.flat_material("lotus_glow", (1.0, 0.78, 0.42), 0.0, 0.6)
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Emission Color"].default_value = (1.0, 0.7, 0.35, 1.0)
    p.inputs["Emission Strength"].default_value = 1.2
    return m


def main():
    export.reset_scene()
    ivory = (0.93, 0.89, 0.80)
    texgen.sandstone(T.TEX, tint=ivory, name="ivory")
    texgen.trim_sheet(T.TEX, tint=ivory, name="trim_ivory")
    texgen.trim_sheet(T.TEX, tint=(0.95, 0.70, 0.30), name="trim_gold")
    texgen.wood(T.TEX)
    T.MATS.update({
        "stone": T.tex_material("ivory", "ivory"),
        "trim": T.tex_material("trim_ivory", "trim_ivory"),
        "ivory_rail": T.flat_material("ivory_rail", ivory, 0.0, 0.5),
        "gtrim": gold_tex_material("trim_gold", "trim_gold"),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
        "lacquer": T.flat_material("lacquer", (0.42, 0.05, 0.03), 0.0, 0.35),
        "cushion": T.flat_material("cushion", (0.55, 0.07, 0.12), 0.0, 0.8),
        "jewel": T.flat_material("jewel", (0.02, 0.35, 0.38), 0.3, 0.15),
        "wood": T.tex_material("teak", "wood"),
        "cloth": T.flat_material("dhvaja", (0.95, 0.45, 0.05), 0.0, 0.8),
        "glow": glow_material(),
    })
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o)
    lods = pushpaka()
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(T.ROOT, "build", "vimana.blend"))
    print(f"[vimana] {export.export_glb(lods, os.path.join(OUT, 'vimana.gltf'))}")


if __name__ == "__main__":
    main()

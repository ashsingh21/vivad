"""Nagara temple: jagati and moulded adhisthana, pillared mandapa with a phamsana (stepped) roof,
pancharatha garbhagriha with niches and a carved doorway, curvilinear latina shikhara with bhumi
bands, corner amalakas and urushringas, crowned by amalaka, gold kalasha and a saffron dhvaja.

Matches the gameplay colliders of `india.rs::temple()` at scale 1 (front toward -Y here, which
is +Z in glTF / the game). Materials: tiling sandstone, carved trim sheet, gold.

blender -b --factory-startup -P tools/blender/architecture/temple.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import export  # noqa: E402
import texgen  # noqa: E402

TEX = os.path.join(ROOT, "build", "textures", "india_kit")
TILE = 2.0          # metres per sandstone / trim repeat along U
PLINTH = 1.2
WALL_H = 5.0
SANCTUM_Y = 4.0     # sanctum centre (Blender +Y = game -Z, i.e. behind the mandapa)
SANCTUM_R = 4.5


# ------------------------------------------------------------------ plans

def stepped_side(R, steps):
    """Offsets along one side: steps = [(half_width, depth)], widest first. Returns [(s, depth)]."""
    pts = [(-R, 0.0)]
    prev = 0.0
    for hw, d in steps:
        pts += [(-hw, prev), (-hw, d)]
        prev = d
    return pts + [(-s, d) for s, d in reversed(pts)]


def pancharatha(R, steps):
    """CCW polygon (XY) for a square of half-size R with stepped projections on every side."""
    side = stepped_side(R, steps)
    poly = [(R + d, s) for s, d in side] + [(-s, R + d) for s, d in side] + \
           [(-R - d, -s) for s, d in side] + [(s, -R - d) for s, d in side]
    clean = []
    for p in poly:
        if not clean or Vector(p) != Vector(clean[-1]):
            clean.append(p)
    if Vector(clean[0]) == Vector(clean[-1]):
        clean.pop()
    n = len(clean)
    out = []
    for i in range(n):
        a, b, c = Vector(clean[i - 1]), Vector(clean[i]), Vector(clean[(i + 1) % n])
        if abs((b - a).cross(c - b)) > 1e-9:
            out.append((b.x, b.y))
    return out


def rect(hx, hy, cx=0.0, cy=0.0):
    return [(cx + hx, cy - hy), (cx + hx, cy + hy), (cx - hx, cy + hy), (cx - hx, cy - hy)]


def offset_poly(poly, d):
    """Offset a CCW rectilinear polygon outward by d (each edge moves d along its normal)."""
    n = len(poly)
    out = []
    for i in range(n):
        p, c, q = Vector(poly[i - 1]), Vector(poly[i]), Vector(poly[(i + 1) % n])
        e1, e2 = (c - p).normalized(), (q - c).normalized()
        n1, n2 = Vector((e1.y, -e1.x)), Vector((e2.y, -e2.x))
        bis = n1 + n2
        denom = 1.0 + n1.dot(n2)
        k = d * 2.0 / denom if denom > 1e-6 else d
        out.append(tuple(c + bis * (k / 2.0)))
    return out


def scale_poly(poly, s, cx, cy):
    return [(cx + (x - cx) * s, cy + (y - cy) * s) for x, y in poly]


# ------------------------------------------------------------------ mesh builders

MATS = {}


class Kit:
    def __init__(self):
        self.objs = []

    def add(self, obj):
        if obj is not None:
            self.objs.append(obj)
        return obj


def new_obj(name, bm, mats):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for m in mats:
        me.materials.append(MATS[m])
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def molded(name, stations, *, cap_top=True, cap_bottom=True, u_scale=TILE):
    """Closed stack of plan rings. stations = [(z, poly, strip_or_None)]; the band between station
    k and k+1 uses strip k of the carved trim sheet, or plain tiling sandstone when None.
    Material slots: 0 = trim, 1 = stone."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    rings = [[bm.verts.new((x, y, z)) for x, y in poly] for z, poly, _ in stations]
    for k in range(len(stations) - 1):
        strip = stations[k][2]
        ra, rb = rings[k], rings[k + 1]
        za, zb = stations[k][0], stations[k + 1][0]
        n = len(ra)
        arc = 0.0
        for i in range(n):
            j = (i + 1) % n
            seg = (ra[j].co - ra[i].co).length
            f = bm.faces.new((ra[i], ra[j], rb[j], rb[i]))
            if strip is not None:
                f.material_index = 0
                v0, v1 = texgen.strip_v(strip)
                uvs = [(arc / u_scale, v0), ((arc + seg) / u_scale, v0), ((arc + seg) / u_scale, v1), (arc / u_scale, v1)]
            else:
                f.material_index = 1
                uvs = [(arc / u_scale, za / u_scale), ((arc + seg) / u_scale, za / u_scale),
                       ((arc + seg) / u_scale, zb / u_scale), (arc / u_scale, zb / u_scale)]
            for loop, t in zip(f.loops, uvs):
                loop[uv].uv = t
            arc += seg
    for ring, want, flip in ((rings[-1], cap_top, False), (rings[0], cap_bottom, True)):
        if want:
            f = bm.faces.new(list(reversed(ring)) if flip else ring)
            f.material_index = 1
            for loop in f.loops:
                loop[uv].uv = (loop.vert.co.x / u_scale, loop.vert.co.y / u_scale)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    return new_obj(name, bm, ("trim", "stone"))


def lathe(name, profile, seg, center, *, ribs=0, rib_depth=0.0, square=False, uv_strip=None, mat=None):
    """Revolve [(r, z)] around a vertical axis at `center` (x, y). `square` gives 4-sided sections."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    rings = []
    for r, z in profile:
        ring = []
        for k in range(seg):
            a = 2 * math.pi * k / seg + (math.pi / seg if square else 0)
            rr = r * (1 + rib_depth * abs(math.cos(a * ribs / 2))) if ribs else r
            if square:
                rr = r / math.cos(math.pi / seg)
            ring.append(bm.verts.new((center[0] + rr * math.cos(a), center[1] + rr * math.sin(a), z)))
        rings.append(ring)
    for k in range(len(profile) - 1):
        for i in range(seg):
            j = (i + 1) % seg
            f = bm.faces.new((rings[k][i], rings[k][j], rings[k + 1][j], rings[k + 1][i]))
            v0, v1 = texgen.strip_v(uv_strip) if uv_strip else (profile[k][1] / TILE, profile[k + 1][1] / TILE)
            for loop, (uu, vv) in zip(f.loops, ((i / seg, v0), ((i + 1) / seg, v0), ((i + 1) / seg, v1), (i / seg, v1))):
                loop[uv].uv = (uu * max(1, round(2 * math.pi * profile[k][0] / TILE)), vv)
    if profile[-1][0] > 0:
        f = bm.faces.new(rings[-1])
        for loop in f.loops:
            loop[uv].uv = (loop.vert.co.x / TILE, loop.vert.co.y / TILE)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj(name, bm, (mat or ("trim" if uv_strip else "stone"),))


def box(name, mn, mx, mat="stone"):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((mn[0] + (v.co.x + 0.5) * (mx[0] - mn[0]), mn[1] + (v.co.y + 0.5) * (mx[1] - mn[1]), mn[2] + (v.co.z + 0.5) * (mx[2] - mn[2])))
    uv = bm.loops.layers.uv.new("UVMap")
    for f in bm.faces:
        nrm = f.normal
        for loop in f.loops:
            c = loop.vert.co
            if abs(nrm.z) > 0.5:
                loop[uv].uv = (c.x / TILE, c.y / TILE)
            elif abs(nrm.x) > 0.5:
                loop[uv].uv = (c.y / TILE, c.z / TILE)
            else:
                loop[uv].uv = (c.x / TILE, c.z / TILE)
    return new_obj(name, bm, (mat,))


def boolean_cut(target, cutter):
    mod = target.modifiers.new("cut", "BOOLEAN")
    mod.operation = "DIFFERENCE"
    mod.solver = "EXACT"
    mod.object = cutter
    bpy.context.view_layer.objects.active = target
    bpy.ops.object.modifier_apply(modifier=mod.name)
    bpy.data.objects.remove(cutter)


def cube_uvs(obj):
    """Re-project tiling UVs after booleans (world metres / TILE, by dominant axis)."""
    me = obj.data
    uv = me.uv_layers[0] if me.uv_layers else me.uv_layers.new(name="UVMap")
    stone = [i for i, m in enumerate(me.materials) if m and m.name == "sandstone"]
    for p in me.polygons:
        if p.material_index not in stone:
            continue
        nrm = p.normal
        for li in p.loop_indices:
            c = me.vertices[me.loops[li].vertex_index].co
            if abs(nrm.z) > 0.6:
                uv.data[li].uv = (c.x / TILE, c.y / TILE)
            elif abs(nrm.x) > abs(nrm.y):
                uv.data[li].uv = (c.y / TILE, c.z / TILE)
            else:
                uv.data[li].uv = (c.x / TILE, c.z / TILE)


# ------------------------------------------------------------------ temple parts

def jagati(kit):
    """Platform + moulded plinth (adhisthana) around the whole temple."""
    base = rect(7.0, 9.0)
    prof = [  # (z, outward offset, strip of the band above)
        (0.00, 0.30, "plain"), (0.18, 0.30, None), (0.20, 0.18, "padma"), (0.42, 0.24, "bead"), (0.52, 0.16, "chevron"),
        (0.74, 0.10, "plain"), (0.80, 0.22, "dentil"), (0.98, 0.22, "plain"), (1.06, 0.04, None), (PLINTH, 0.0, None),
    ]
    st = [(z, offset_poly(base, d), s) for z, d, s in prof]
    obj = molded("jagati", st, cap_top=True)
    kit.add(obj)
    # Front steps with sloping side balustrades (hasti-hasta).
    for i in range(6):
        z0 = i * PLINTH / 6
        y0 = -9.0 - 3.0 + i * 0.5
        kit.add(box(f"step{i}", (-2.0, y0, z0), (2.0, -8.9, z0 + PLINTH / 6)))
    for sx in (-1, 1):
        bm = bmesh.new()
        pts = [(-12.3, 0.0), (-12.3, 0.55), (-9.0, PLINTH + 0.55), (-8.9, PLINTH + 0.55), (-8.9, 0.0)]
        x0, x1 = sx * 2.0, sx * 2.45
        vs = [[bm.verts.new((x, y, z)) for (y, z) in pts] for x in (min(x0, x1), max(x0, x1))]
        bm.faces.new(vs[0][::-1])
        bm.faces.new(vs[1])
        for k in range(len(pts)):
            j = (k + 1) % len(pts)
            bm.faces.new((vs[0][k], vs[0][j], vs[1][j], vs[1][k]))
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.loops.layers.uv.new("UVMap")
        o = new_obj("rail", bm, ("stone",))
        cube_uvs(o)
        kit.add(o)


def pillar(kit, x, y, z0, h):
    """Stambha: square base, octagonal shaft, round banded shaft, ghata-pallava capital, bracket."""
    c = (x, y)
    kit.add(lathe("p_base", [(0.36, z0), (0.36, z0 + 0.28 * h / 4), (0.30, z0 + 0.32 * h / 4)], 4, c, square=True))
    kit.add(lathe("p_oct", [(0.27, z0 + 0.32 * h / 4), (0.27, z0 + 1.35 * h / 4)], 8, c))
    kit.add(lathe("p_band", [(0.29, z0 + 1.35 * h / 4), (0.31, z0 + 1.45 * h / 4), (0.29, z0 + 1.6 * h / 4)], 16, c, uv_strip="bead"))
    kit.add(lathe("p_round", [(0.25, z0 + 1.6 * h / 4), (0.24, z0 + 2.75 * h / 4)], 16, c))
    # Ghata-pallava: the pot of plenty with foliage spilling over its rim.
    gp = [(0.24, z0 + 2.75 * h / 4), (0.36, z0 + 2.95 * h / 4), (0.40, z0 + 3.15 * h / 4), (0.30, z0 + 3.35 * h / 4),
          (0.34, z0 + 3.45 * h / 4), (0.42, z0 + 3.55 * h / 4)]
    kit.add(lathe("p_ghata", gp, 16, c, ribs=16, rib_depth=0.06, uv_strip="padma"))
    kit.add(lathe("p_abacus", [(0.46, z0 + 3.55 * h / 4), (0.46, z0 + 3.7 * h / 4)], 4, c, square=True))
    # Cruciform bracket capital. The cross arm is 2 cm shorter top and bottom, so where the two
    # cross only one arm's faces show (shared planes would z-fight, seen from below).
    kit.add(box("p_brk1", (x - 0.75, y - 0.2, z0 + 3.7 * h / 4), (x + 0.75, y + 0.2, z0 + h)))
    kit.add(box("p_brk2", (x - 0.2, y - 0.75, z0 + 3.7 * h / 4 + 0.02), (x + 0.2, y + 0.75, z0 + h - 0.02)))


def mandapa(kit):
    z0 = PLINTH
    h = 4.0
    xs = (-4.5, -1.5, 1.5, 4.5)
    ys = (-2.0, -5.0, -8.0)
    for x in xs:
        for y in ys:
            pillar(kit, x, y, z0, h)
    # Beams and ceiling.
    top = z0 + h
    for y in ys:
        kit.add(molded("beam", [(top, rect(5.0, 0.22, 0, y), "lata"), (top + 0.42, rect(5.0, 0.22, 0, y), None)]))
    for x in xs:
        kit.add(box("beam_y", (x - 0.22, -8.3, top), (x + 0.22, -0.4, top + 0.42)))
    # Kakshasana: sloping seat-back parapet between the outer pillars on both sides.
    for x in (-4.5, 4.5):
        for (ya, yb) in ((-8.0, -5.0), (-5.0, -2.0)):
            kit.add(box("seat", (x - 0.35, ya + 0.3, z0), (x + 0.35, yb - 0.3, z0 + 0.45)))
            kit.add(molded("seatback", [(z0 + 0.45, rect(0.12, (yb - ya) / 2 - 0.3, x + 0.2 * (1 if x > 0 else -1), (ya + yb) / 2), "padma"),
                                                (z0 + 1.0, rect(0.08, (yb - ya) / 2 - 0.3, x + 0.42 * (1 if x > 0 else -1), (ya + yb) / 2), None)]))
    # Phamsana roof: stacked, receding slabs with drip-mould lips, then the bell and amalaka.
    roof_c = (0.0, -5.0)
    tiers = [(6.0, 4.0), (5.2, 3.4), (4.4, 2.8), (3.6, 2.2), (2.8, 1.6)]
    z = top + 0.42
    for k, (hx, hy) in enumerate(tiers):
        base = rect(hx, hy, *roof_c)
        st = [(z, offset_poly(base, 0.25), "dentil"), (z + 0.18, offset_poly(base, 0.25), "plain"),
              (z + 0.26, offset_poly(base, 0.0), "chevron"), (z + 0.62, offset_poly(base, -0.15), None)]
        obj = molded(f"roof{k}", st, cap_top=True)
        kit.add(obj)
        z += 0.62
    bell = [(1.3, z), (1.5, z + 0.25), (1.2, z + 0.8), (0.55, z + 1.25), (0.35, z + 1.35)]
    kit.add(lathe("ghanta", bell, 24, roof_c))
    am = [(0.35, z + 1.35), (0.85, z + 1.45), (0.95, z + 1.6), (0.85, z + 1.75), (0.3, z + 1.85)]
    kit.add(lathe("m_amalaka", am, 32, roof_c, ribs=32, rib_depth=0.1, uv_strip="rope"))
    kalasha(kit, roof_c, z + 1.85, 0.5)


def kalasha(kit, c, z, s):
    prof = [(0.25 * s, z), (0.45 * s, z + 0.1 * s), (0.55 * s, z + 0.35 * s), (0.4 * s, z + 0.6 * s), (0.18 * s, z + 0.7 * s),
            (0.24 * s, z + 0.8 * s), (0.12 * s, z + 0.95 * s), (0.2 * s, z + 1.05 * s), (0.0, z + 1.4 * s)]
    kit.add(lathe("kalasha", prof, 24, c, mat="gold"))


def garbhagriha(kit):
    """Sanctum: pancharatha walls with a moulded base and cornice, carved doorway, three niches."""
    c = (0.0, SANCTUM_Y)
    plan = [(x, y + SANCTUM_Y) for x, y in pancharatha(SANCTUM_R, [(2.6, 0.18), (1.5, 0.42)])]
    z0 = PLINTH
    prof = [
        (z0, 0.22, "padma"), (z0 + 0.35, 0.14, "bead"), (z0 + 0.5, 0.06, None),
        (z0 + 0.6, 0.0, None),                                   # jangha (wall face)
        (z0 + WALL_H - 0.7, 0.0, "lata"), (z0 + WALL_H - 0.4, 0.05, "dentil"), (z0 + WALL_H - 0.2, 0.3, "plain"),
        (z0 + WALL_H, 0.3, None), (z0 + WALL_H + 0.5, 0.05, None),
    ]
    st = [(z, offset_poly(plan, d), s) for z, d, s in prof]
    shell = molded("sanctum", st)
    # Hollow it out and open the doorway (front, -Y): exact booleans on the closed shell.
    boolean_cut(shell, box("inner", (-SANCTUM_R + 0.7, SANCTUM_Y - SANCTUM_R + 0.7, z0),
                           (SANCTUM_R - 0.7, SANCTUM_Y + SANCTUM_R - 0.7, z0 + WALL_H - 0.6)))
    boolean_cut(shell, box("door", (-1.0, SANCTUM_Y - SANCTUM_R - 2.0, z0 - 0.01), (1.0, SANCTUM_Y - SANCTUM_R + 1.2, z0 + 3.0)))
    cube_uvs(shell)
    kit.add(shell)
    # Doorframe: three stepped shakhas (jambs) of different carvings, and a lintel with a lalatabimba.
    fy = SANCTUM_Y - SANCTUM_R - 0.42
    for k, (strip, w) in enumerate((("lata", 0.18), ("bead", 0.14), ("padma", 0.2))):
        o = 1.0 + sum(ww for _, ww in (("", 0.18), ("", 0.14), ("", 0.2))[:k])
        for sx in (-1, 1):
            xa, xb = sx * o, sx * (o + w)
            kit.add(molded("jamb", [(z0, rect(abs(xb - xa) / 2, 0.08 + 0.04 * k, (xa + xb) / 2, fy - 0.04 * k), strip),
                                            (z0 + 3.0 + 0.2 * k, rect(abs(xb - xa) / 2, 0.08 + 0.04 * k, (xa + xb) / 2, fy - 0.04 * k), None)]))
        kit.add(molded("lintel", [(z0 + 3.0 + 0.2 * k, rect(o + w, 0.08 + 0.04 * k, 0, fy - 0.04 * k), strip),
                                          (z0 + 3.2 + 0.2 * k, rect(o + w, 0.08 + 0.04 * k, 0, fy - 0.04 * k), None)]))
    # Niches (devakoshtha) on the three other bhadras: framed recess with a pediment.
    for ang in (0.0, math.pi / 2, math.pi):
        nx, ny = math.cos(ang), math.sin(ang)
        face = SANCTUM_R + 0.42
        cx_, cy_ = nx * face, SANCTUM_Y + ny * face
        tx, ty = -ny, nx
        for k in range(2):
            d = 0.12 + 0.1 * k
            w = 0.6 + 0.18 * (1 - k)
            hz = 1.0 + 0.15 * (1 - k)
            p = [(cx_ + tx * sx * w + nx * dd, cy_ + ty * sx * w + ny * dd) for sx, dd in ((-1, 0), (1, 0), (1, d), (-1, d))]
            p = p if ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0])) > 0 else p[::-1]
            kit.add(molded("niche", [(z0 + 1.4, p, "lata" if k == 0 else "bead"), (z0 + 1.4 + 2 * hz, p, None)]))
        # Pediment (udgama): a small stepped gable over the niche.
        for k in range(3):
            w = 0.85 - 0.25 * k
            p = [(cx_ + tx * sx * w + nx * dd, cy_ + ty * sx * w + ny * dd) for sx, dd in ((-1, 0), (1, 0), (1, 0.3), (-1, 0.3))]
            p = p if ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0])) > 0 else p[::-1]
            zz = z0 + 3.75 + 0.22 * k
            kit.add(molded("udgama", [(zz, p, "gavaksha"), (zz + 0.22, p, None)]))
    return plan


def shikhara(kit, base_plan, c, z0, height, *, name="shikhara", bands=7, crown=True, scale=1.0):
    """Latina shikhara: curvilinear taper, bhumi bands with corner amalakas, crowned."""
    stations = []
    n_st = bands * 4
    for k in range(n_st + 1):
        t = k / n_st
        s = 1.0 - 0.60 * t ** 1.65
        z = z0 + height * t
        poly = scale_poly(base_plan, s, *c)
        band_pos = (k % 4)
        if band_pos == 0 and k < n_st:
            # Bhumi division: a projecting drip-moulding.
            stations.append((z, offset_poly(poly, 0.12 * scale), "bead"))
            stations.append((z + height / n_st * 0.35, offset_poly(poly, 0.12 * scale), "gavaksha"))
        else:
            stations.append((z, poly, "gavaksha" if k < n_st else None))
    obj = molded(name, stations, cap_top=True, u_scale=TILE * 0.8 * scale)
    kit.add(obj)
    # Bhumi-amalakas at the corners of each band.
    corners = [p for p in base_plan if abs(abs(p[0] - c[0]) - abs(p[1] - c[1])) < 1e-3]
    for b in range(1, bands):
        t = b / bands
        s = 1.0 - 0.60 * t ** 1.65
        z = z0 + height * t
        for x, y in corners:
            px, py = c[0] + (x - c[0]) * s, c[1] + (y - c[1]) * s
            r = 0.5 * scale * (1 - 0.35 * t)
            kit.add(lathe("bhumi_amalaka", [(r * 0.5, z - r * 0.4), (r, z - r * 0.15), (r, z + r * 0.15), (r * 0.5, z + r * 0.4)],
                                  16, (px, py), ribs=16, rib_depth=0.12, uv_strip="rope"))
    if not crown:
        return
    s_top = 0.40
    top_r = max(abs(x - c[0]) for x, _ in base_plan) * s_top
    zt = z0 + height
    neck = [(top_r * 0.75, zt), (top_r * 0.6, zt + 0.35 * scale)]
    kit.add(lathe("griva", neck, 24, c))
    # Amalaka: a thick, deeply ribbed cushion (the myrobalan fruit), barely wider than the tower top.
    am = [(top_r * 0.62, zt + 0.35 * scale), (top_r * 0.95, zt + 0.5 * scale), (top_r * 1.12, zt + 0.85 * scale),
          (top_r * 1.12, zt + 1.15 * scale), (top_r * 0.95, zt + 1.45 * scale), (top_r * 0.5, zt + 1.6 * scale)]
    kit.add(lathe("amalaka", am, 80, c, ribs=40, rib_depth=0.16, uv_strip="rope"))
    kit.add(lathe("chandrika", [(top_r * 0.5, zt + 1.6 * scale), (top_r * 0.55, zt + 1.75 * scale), (top_r * 0.3, zt + 1.8 * scale)], 24, c))
    kalasha(kit, c, zt + 1.8 * scale, 1.1 * scale)
    # Dhvaja: flagpole with a saffron pennant.
    pole = lathe("dhvaja_pole", [(0.04, zt + 1.4 * scale), (0.03, zt + 4.6 * scale)], 8, (c[0] + top_r * 0.4, c[1] + top_r * 0.4), mat="gold")
    kit.add(pole)
    bm = bmesh.new()
    px, py = c[0] + top_r * 0.4, c[1] + top_r * 0.4
    a = bm.verts.new((px, py, zt + 4.5 * scale))
    b = bm.verts.new((px, py, zt + 3.7 * scale))
    d = bm.verts.new((px + 1.6 * scale, py - 0.4 * scale, zt + 4.2 * scale))
    bm.faces.new((a, b, d))
    bm.loops.layers.uv.new("UVMap")
    kit.add(new_obj("dhvaja", bm, ("cloth",)))


def urushringas(kit, plan, c, z0):
    """Half-spires clustered on the three rear/side bhadras, rising against the main tower."""
    for ang in (0.0, math.pi / 2, math.pi):
        nx, ny = math.cos(ang), math.sin(ang)
        # Sit on the face of the main tower so roughly half of each spire stands proud of it.
        for k, (dist, zz, sc) in enumerate(((SANCTUM_R + 0.2, z0, 0.40), (SANCTUM_R - 0.2, z0 + 3.6, 0.30))):
            cc = (c[0] + nx * dist, c[1] + ny * dist)
            sub = [(cc[0] + (x - c[0]) * sc, cc[1] + (y - c[1]) * sc) for x, y in plan]
            shikhara(kit, sub, cc, zz, 14.0 * sc * 0.9, name="urushringa", bands=4, crown=False, scale=sc)
            am_r = SANCTUM_R * sc * 0.42
            zt = zz + 14.0 * sc * 0.9
            kit.add(lathe("u_amalaka", [(am_r * 0.6, zt), (am_r * 1.2, zt + 0.2), (am_r * 1.2, zt + 0.4), (am_r * 0.4, zt + 0.55)],
                                  24, cc, ribs=24, rib_depth=0.1, uv_strip="rope"))
            kalasha(kit, cc, zt + 0.55, 0.45 * sc / 0.42)


# ------------------------------------------------------------------ materials, weathering, export

def tex_material(name, prefix):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    p = nt.nodes["Principled BSDF"]

    def img(fn, non_color):
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = bpy.data.images.load(os.path.join(TEX, fn))
        if non_color:
            n.image.colorspace_settings.name = "Non-Color"
        return n

    base = img(f"{prefix}_basecolor.png", False)
    vc = nt.nodes.new("ShaderNodeVertexColor")
    vc.layer_name = "Col"
    mul = nt.nodes.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs[0].default_value = 1.0
    nt.links.new(base.outputs["Color"], mul.inputs[6])
    nt.links.new(vc.outputs["Color"], mul.inputs[7])
    nt.links.new(mul.outputs[2], p.inputs["Base Color"])
    orm = img(f"{prefix}_orm.png", True)
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(orm.outputs["Color"], sep.inputs["Color"])
    nt.links.new(sep.outputs["Green"], p.inputs["Roughness"])
    nt.links.new(sep.outputs["Blue"], p.inputs["Metallic"])
    occ = nt.nodes.new("ShaderNodeGroup")
    occ.node_tree = export.gltf_output_group()
    nt.links.new(sep.outputs["Red"], occ.inputs["Occlusion"])
    nrm = img(f"{prefix}_normal.png", True)
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(nrm.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], p.inputs["Normal"])
    return m


def flat_material(name, rgb, metal, rough):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Base Color"].default_value = (*rgb, 1)
    p.inputs["Metallic"].default_value = metal
    p.inputs["Roughness"].default_value = rough
    if name == "dhvaja":
        m.use_backface_culling = False
    return m


def bake_weathering(obj, streak_k=0.18):
    """Ambient occlusion plus age into vertex colours: grime rising from the ground, rain streaks
    darkening below every ledge, a pale sun-bleached crown."""
    import bake as bk
    bk.setup_cycles(64)
    scene = bpy.context.scene
    me = obj.data
    if "Col" not in me.color_attributes:
        me.color_attributes.new("Col", "BYTE_COLOR", "CORNER")
    me.color_attributes.active_color = me.color_attributes["Col"]
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    scene.render.bake.target = "VERTEX_COLORS"
    bpy.ops.object.bake(type="AO")
    col = me.color_attributes["Col"]
    import random
    rnd = random.Random(5)
    streak_seed = {}
    # Heights from the model's lowest point (stepwells and caves reach below their origin).
    zmin = min(v.co.z for v in me.vertices)
    zmax = max(v.co.z for v in me.vertices) - zmin
    for li, loop in enumerate(me.loops):
        v = me.vertices[loop.vertex_index].co.copy()
        v.z -= zmin
        ao = col.data[li].color[0]
        ground = 1.0 - 0.35 * max(0.0, 1.0 - v.z / 1.6)
        key = (round(v.x * 1.3), round(v.y * 1.3))
        if key not in streak_seed:
            streak_seed[key] = rnd.random()
        streak = 1.0 - streak_k * streak_seed[key] * (1.0 - v.z / zmax)
        bleach = 1.0 + 0.06 * (v.z / zmax)
        k = (0.45 + 0.55 * ao) * ground * streak * bleach
        col.data[li].color = (k, k * 0.98, k * 0.95, 1.0)


def main():
    export.reset_scene()
    texgen.sandstone(TEX)
    texgen.trim_sheet(TEX)
    MATS.update({
        "stone": tex_material("sandstone", "sandstone"),
        "trim": tex_material("trim", "trim"),
        "gold": flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
        "cloth": flat_material("dhvaja", (0.95, 0.45, 0.05), 0.0, 0.8),
    })
    kit = Kit()
    jagati(kit)
    mandapa(kit)
    plan = garbhagriha(kit)
    shikhara(kit, plan, (0.0, SANCTUM_Y), PLINTH + WALL_H + 0.5, 14.0)
    urushringas(kit, plan, (0.0, SANCTUM_Y), PLINTH + WALL_H + 0.5)

    import geo
    temple = geo.join(kit.objs, "temple_nagara_LOD0")
    # Hard-edged stone: auto-sharp by angle for crisp mouldings, smooth curved members.
    temple.data.set_sharp_from_angle(angle=math.radians(40))
    for p in temple.data.polygons:
        p.use_smooth = True
    bake_weathering(temple)
    export.triangulate(temple)
    lods = export.make_lods(temple, (0.35,))
    out = os.path.join(ROOT, "assets", "models", "props", "temple_nagara.gltf")
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ROOT, "build", "temple_nagara.blend"))
    counts = export.export_glb(lods, out)
    print(f"[temple] {counts}")


if __name__ == "__main__":
    main()

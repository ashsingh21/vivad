"""Buddhist kit: the Sanchi-style great stupa (medhi, anda, harmika, chhatravali, vedika railing
and four toranas) and the Ajanta cliff (rock-cut chaitya and vihara caves with carved facades,
pillared halls, rock-cut stupas, seated Buddha figures, and the stair cut into a rock spur).

Both match the gameplay blocks in `india.rs::caves_and_stupa()`. Conventions: Blender X = game X,
Blender Y = -game Z, Z up.
  stupa: origin at the stupa centre on the ground.
  ajanta: origin at the cliff face (world x = -192), z = 0; the face is the plane X = 0 and
          the cliff runs back to X = -48, Y in [-75, 75].

blender -b --factory-startup -P tools/blender/architecture/buddhist.py [-- stupa|ajanta|rocks]
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
from mathutils import Matrix, Vector, noise  # noqa: E402

import export  # noqa: E402
import geo  # noqa: E402
import ornaments as O  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402
from vernacular import apply_xform, cylinder_between, prism, proj_uvs  # noqa: E402

OUT = os.path.join(T.ROOT, "assets", "models", "props")

# Ajanta, in model space (see module doc).
CLIFF_H = 24.0
CLIFF_BACK = -48.0
CLIFF_Y = 75.0
HALL_D, HALL_W, HALL_H = 20.0, 12.0, 6.5
HALLS = [-48.0, -16.0, 16.0, 48.0]          # world z of each hall
CHAITYA = {-48.0, 16.0}                      # the others are viharas
SPUR = (-73.0, -67.0)                        # rock spur (Blender Y), stairs in its middle 4 m
SPUR_RUN = 28.0
ROCK_TILE = 4.0


def sandstone_mats():
    texgen.sandstone(T.TEX)
    texgen.trim_sheet(T.TEX)
    T.MATS.update({
        "stone": T.tex_material("sandstone", "sandstone"),
        "trim": T.tex_material("trim", "trim"),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
    })


def basalt_mats():
    texgen.basalt(T.TEX)
    texgen.trim_sheet(T.TEX, tint=(0.46, 0.40, 0.35), name="trim_basalt")
    T.MATS.update({
        "stone": T.tex_material("basalt", "basalt"),
        "trim": T.tex_material("trim_basalt", "trim_basalt"),
        "shadow": T.flat_material("shadow", (0.025, 0.021, 0.018), 0.0, 0.95),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
    })
    # The cliff is built from open sheets: cull back faces so caps never fight the face.
    for k in ("stone", "trim", "shadow"):
        T.MATS[k].use_backface_culling = True


def finish(name, kit, lods=(0.35,)):
    obj = geo.join(kit.objs, f"{name}_LOD0")
    obj.data.set_sharp_from_angle(angle=math.radians(40))
    for p in obj.data.polygons:
        p.use_smooth = True
    T.bake_weathering(obj)
    export.triangulate(obj)
    return export.make_lods(obj, lods)


def transform(o, mat):
    o.matrix_world = mat @ o.matrix_world
    return apply_xform(o)


def uv_sphere(name, c, r, mat, seg=12, rings=8):
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    bmesh.ops.create_uvsphere(bm, u_segments=seg, v_segments=rings, radius=1.0)
    for v in bm.verts:
        v.co = Vector((c[0] + v.co.x * r[0], c[1] + v.co.y * r[1], c[2] + v.co.z * r[2]))
    o = T.new_obj(name, bm, (mat,))
    proj_uvs(o, 1.0)
    return o


# ------------------------------------------------------------------ figures and reliefs

def seated_buddha(kit, mat_world, s=1.0):
    """Seated Buddha in dhyana: lotus pedestal, crossed legs, robed torso, hands in the lap, head
    with ushnisha, halo. Built at the origin facing -Y, ~1.1*s tall, then moved by `mat_world`."""
    parts = []
    pad = [(0.46, 0.0), (0.5, 0.08), (0.44, 0.16), (0.36, 0.2)]
    parts.append(T.lathe("padma", [(r * s, z * s) for r, z in pad], 20, (0, 0), uv_strip="padma"))
    parts.append(uv_sphere("legs", (0, -0.03 * s, 0.31 * s), (0.42 * s, 0.27 * s, 0.13 * s), "stone"))
    torso = T.lathe("torso", [(r * s, z * s) for r, z in ((0.19, 0.3), (0.21, 0.45), (0.23, 0.6), (0.24, 0.68), (0.13, 0.75), (0.07, 0.8))],
                    14, (0, 0))
    for v in torso.data.vertices:
        v.co.y = v.co.y * 0.72 + 0.03 * s
    parts.append(torso)
    for sx in (-1, 1):
        sh, el, hd = Vector((sx * 0.22, 0.03, 0.66)), Vector((sx * 0.29, -0.03, 0.43)), Vector((sx * 0.07, -0.16, 0.36))
        parts.append(cylinder_between("uarm", sh * s, el * s, 0.06 * s, "stone", 8))
        parts.append(cylinder_between("farm", el * s, hd * s, 0.05 * s, "stone", 8))
    parts.append(uv_sphere("hands", (0, -0.16 * s, 0.37 * s), (0.1 * s, 0.06 * s, 0.04 * s), "stone"))
    parts.append(uv_sphere("head", (0, 0.02 * s, 0.9 * s), (0.1 * s, 0.11 * s, 0.12 * s), "stone"))
    parts.append(uv_sphere("ushnisha", (0, 0.03 * s, 1.0 * s), (0.055 * s, 0.055 * s, 0.05 * s), "stone"))
    for sx in (-1, 1):
        parts.append(uv_sphere("ear", (sx * 0.1 * s, 0.03 * s, 0.86 * s), (0.02 * s, 0.03 * s, 0.07 * s), "stone", 6, 4))
    halo = cylinder_between("halo", (0, 0.13 * s, 0.9 * s), (0, 0.17 * s, 0.9 * s), 0.3 * s, "trim", 24)
    parts.append(halo)
    for o in parts:
        kit.add(transform(o, mat_world))


def piece_to_world(y_center, x_out=0.0):
    """Facade 'piece space' (X along the face, front -Y, face plane Y=0) -> cliff model space,
    for the hall whose centre is at model Y = y_center."""
    return Matrix.Translation((x_out, y_center, 0.0)) @ Matrix.Rotation(math.pi / 2, 4, "Z")


def niche(kit, M, x, z0, w, h, fig_s, figure=None):
    """Rock-cut niche relief: a projecting pilastered frame with a horseshoe-arched head, a dark
    back panel and a seated figure."""
    d = 0.5
    objs = []
    for sx in (-1, 1):
        objs.append(T.box("npil", (x + sx * w / 2 - 0.16, -d, z0), (x + sx * w / 2 + 0.16, 0.0, z0 + h), "stone"))
    objs.append(T.box("nsill", (x - w / 2 - 0.3, -d - 0.1, z0 - 0.25), (x + w / 2 + 0.3, 0.0, z0), "stone"))
    objs.append(T.box("nback", (x - w / 2, -0.06, z0), (x + w / 2, 0.0, z0 + h), "shadow"))
    a = w / 2 + 0.16
    objs.append(T.box("nlintel", (x - a, -d, z0 + h), (x + a, 0.0, z0 + h + 0.2 * a), "stone"))
    head = horseshoe(a * 0.9, z0 + h + a * 0.55, 0.45, 40)
    outer = horseshoe(a * 0.9 + 0.28, z0 + h + a * 0.55, 0.45, 40)
    ring = outer + list(reversed(head))
    objs.append(O.solid_from_outline("nhead", [(x + px, pz) for px, pz in ring], -d, 0.0, "trim", strip="gavaksha", u_scale=1.0))
    inner = [(x + px, pz) for px, pz in head]
    objs.append(O.solid_from_outline("nheadback", inner, -0.06, 0.0, "shadow"))
    for o in objs:
        kit.add(transform(o, M))
    (figure or seated_buddha)(kit, M @ Matrix.Translation((x, -0.1, z0 + 0.02)), fig_s)


def horseshoe(R, zc, beta, samples):
    """Horseshoe (chaitya) arch: a circle of radius R about (0, zc) carried below its centre by
    `beta` radians each side. Points run from the left foot over the top to the right foot."""
    pts = []
    for k in range(samples + 1):
        t = k / samples
        phi = (math.pi + beta) - t * (math.pi + 2 * beta)
        pts.append((R * math.cos(phi), zc + R * math.sin(phi)))
    return pts


def chaitya_window(kit, M, zc, R):
    """The great horseshoe window over a chaitya entrance: a deep carved frame, ribs inside
    imitating the timber original, a dark interior, and a finial."""
    beta = 0.42
    foot = zc - R * math.sin(beta)
    inner = horseshoe(R, zc, beta, 64)
    outer = horseshoe(R + 0.75, zc, beta, 64)
    legs_in = [(inner[0][0], foot - 1.2)] + inner + [(inner[-1][0], foot - 1.2)]
    legs_out = [(outer[0][0], foot - 1.2)] + outer + [(outer[-1][0], foot - 1.2)]
    objs = [O.solid_from_outline("cwframe", legs_out + list(reversed(legs_in)), -0.8, 0.0, "stone"),
            O.rim_sweep("cwrim", legs_in, -0.82, 0.1),
            O.rim_sweep("cwrim2", outer, -0.82, 0.09),
            O.solid_from_outline("cwback", legs_in, -0.05, 0.0, "shadow")]
    # Ribs radiating from a hub and a transom across the springing.
    hub = Vector((0.0, -0.35, foot + 0.6))
    for k in range(1, 12):
        px, pz = inner[int(k * 64 / 12)]
        objs.append(cylinder_between("rib", hub, (px * 0.98, -0.35, pz - 0.04), 0.07, "stone", 6))
    objs.append(T.box("transom", (inner[0][0], -0.45, foot + 0.5), (inner[-1][0], -0.2, foot + 0.72), "trim"))
    for o in objs:
        kit.add(transform(o, M))
    T.kalasha(kit_proxy := T.Kit(), (0.0, -0.4), zc + R + 0.75, 0.6)
    for o in kit_proxy.objs:
        o.data.materials[0] = T.MATS["stone"]
        kit.add(transform(o, M))


def frieze(kit, M, x0, x1, z0, z1, depth, strip):
    o = T.molded("frieze", [(z0, T.rect((x1 - x0) / 2, depth / 2, (x0 + x1) / 2, -depth / 2), strip),
                            (z1, T.rect((x1 - x0) / 2, depth / 2, (x0 + x1) / 2, -depth / 2), None)])
    kit.add(transform(o, M))


def facade(kit, y_center, chaitya):
    M = piece_to_world(y_center)
    a = HALL_W / 2
    # Pilastered jambs and a deep portico cornice carried on the free-standing porch pillars.
    for sx in (-1, 1):
        kit.add(transform(T.box("jamb", (sx * a - 0.35, -0.35, 0.0), (sx * a + 0.35, 0.0, HALL_H)), M))
    frieze(kit, M, -a - 0.8, a + 0.8, HALL_H, HALL_H + 0.35, 1.9, "dentil")
    frieze(kit, M, -a - 0.8, a + 0.8, HALL_H + 0.35, HALL_H + 0.85, 1.7, "gavaksha")
    frieze(kit, M, -a - 0.6, a + 0.6, HALL_H + 0.85, HALL_H + 1.05, 1.9, "plain")
    for sx in (-1, 1):
        niche(kit, M, sx * (a + 2.4), 1.2, 1.6, 2.6, 1.25)
    if chaitya:
        chaitya_window(kit, M, 11.2, 3.2)
        for sx in (-1, 1):
            niche(kit, M, sx * 6.3, 9.0, 1.2, 1.8, 0.9)
        frieze(kit, M, -9.5, 9.5, 15.6, 16.2, 0.45, "gavaksha")
        frieze(kit, M, -9.8, 9.8, 16.2, 16.5, 0.6, "dentil")
    else:
        frieze(kit, M, -a - 0.6, a + 0.6, HALL_H + 1.05, HALL_H + 1.6, 0.4, "lata")
        for k in range(5):
            niche(kit, M, -5.0 + k * 2.5, 8.4, 1.0, 1.3, 0.75)
        frieze(kit, M, -a - 0.6, a + 0.6, 10.6, 11.0, 0.5, "dentil")


# ------------------------------------------------------------------ cliff surfaces

def rect_minus(u0, u1, v0, v1, holes):
    rects = [(u0, u1, v0, v1)]
    for (hu0, hu1, hv0, hv1) in holes:
        nxt = []
        for (a0, a1, b0, b1) in rects:
            c0, c1, d0, d1 = max(a0, hu0), min(a1, hu1), max(b0, hv0), min(b1, hv1)
            if c0 >= c1 or d0 >= d1:
                nxt.append((a0, a1, b0, b1))
                continue
            if b0 < d0:
                nxt.append((a0, a1, b0, d0))
            if d1 < b1:
                nxt.append((a0, a1, d1, b1))
            if a0 < c0:
                nxt.append((a0, c0, d0, d1))
            if c1 < a1:
                nxt.append((c1, a1, d0, d1))
        rects = nxt
    return rects


def plane(name, rects, to3d, mat, tile=ROCK_TILE, flip=False):
    """Quads for rects (u0, u1, v0, v1) in a 2D parameter plane mapped by `to3d(u, v)`."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    for (u0, u1, v0, v1) in rects:
        corners = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)]
        if flip:
            corners.reverse()
        f = bm.faces.new([bm.verts.new(to3d(u, v)) for u, v in corners])
        for loop, (u, v) in zip(f.loops, corners):
            loop[uv].uv = (u / tile, v / tile)
    o = T.new_obj(name, bm, (mat,))
    return o


def zone_weight(y, z):
    """1 on the dressed facade planes around each cave, fading to 0 in the raw cliff."""
    w = 0.0
    for hz in HALLS:
        yc = -hz
        half, top = (11.5, 17.2) if hz in CHAITYA else (11.0, 11.6)
        dy = max(0.0, abs(y - yc) - half)
        dz = max(0.0, z - top)
        w = max(w, 1.0 - min(1.0, math.hypot(dy, dz) / 2.5))
    if SPUR[0] - 1.5 < y < SPUR[1] + 1.5:
        w = max(w, 0.6)
    return w


def cliff_disp(y, z, off):
    """Outward displacement of the raw cliff: big buttresses, trap-flow ledges with overhangs,
    vertical fissures and small breakup. Near the ground it stays close to the collider plane."""
    p = Vector((y * 0.045, z * 0.06, 0.0)) + off
    big = noise.fractal(p, 0.55, 2.0, 4) * 2.4
    small = noise.noise(Vector((y * 0.35, z * 0.4, 1.7)) + off) * 0.3
    # Lava-flow ledges: a smooth periodic notch (a sawtooth here folds the sheet over itself).
    f = z / 3.4 + 0.35 * noise.noise(Vector((y * 0.04, 0.0, 5.0)) + off)
    ledge = -1.0 * max(0.0, math.cos(2 * math.pi * f)) ** 3 + 0.15
    fiss = 1.0 - abs(noise.noise(Vector((y * 0.22, z * 0.02, 9.0)) + off))
    fissure = -1.2 * max(0.0, fiss - 0.82) / 0.18
    d = big + small + ledge + fissure
    lo = -0.5 if z < 2.5 else -2.8
    return max(lo, min(0.4, d))


def lip_radius(y, off):
    """How far the cliff's top edge has weathered back, varying along the face."""
    r = 2.2 + 1.6 * noise.fractal(Vector((y * 0.03, 3.3, 0.0)) + off, 0.5, 2.0, 3)
    return max(0.6, min(4.0, r)) * min(1.0, (CLIFF_Y - abs(y)) / 4.0) + 0.05


LIP_X = -7.0  # where the weathered lip meets the flat cliff top


def profile(y, off):
    """Points (x, z, normal_x, normal_z, kind) up the face, round the eroded lip and back across
    the top to LIP_X. The first 14 points are z = 0..6.5 exactly (cave mouths cut on them)."""
    r = lip_radius(y, off)
    pts = [(0.0, k * 0.5, 1.0, 0.0, "face") for k in range(14)]
    top_v = CLIFF_H - r
    for k in range(1, 32):
        pts.append((0.0, 6.5 + (top_v - 6.5) * k / 31, 1.0, 0.0, "face"))
    for k in range(1, 9):
        a = 0.5 * math.pi * k / 8
        pts.append((-r + r * math.cos(a), top_v + r * math.sin(a), math.cos(a), math.sin(a), "lip"))
    for k in range(1, 7):
        pts.append((-r + (LIP_X + r) * k / 6, CLIFF_H, 0.0, 1.0, "top"))
    return pts


def cliff(kit):
    off = Vector((13.1, 4.7, 0.0))
    holes = [(-hz - HALL_W / 2, -hz + HALL_W / 2) for hz in HALLS]
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    ny = int(2 * CLIFF_Y)
    cols = []
    for i in range(ny + 1):
        y = -CLIFF_Y + i
        col = []
        for (x, z, nx, nz, kind) in profile(y, off):
            m = (1.0 - zone_weight(y, z)) * min(1.0, (CLIFF_Y - abs(y)) / 2.0)
            if kind == "face":
                d = cliff_disp(y, z, off) * m * min(1.0, (CLIFF_H - z) / 3.0 + 0.3)
            elif kind == "lip":
                d = min(0.0, cliff_disp(y, z, off)) * 0.6 * m
            else:
                d = -0.25 * abs(noise.noise(Vector((y * 0.2, x * 0.2, 2.0)) + off)) * min(1.0, (x - LIP_X) / 2.0)
            col.append(bm.verts.new((x + nx * d, y, z + nz * d)))
        cols.append(col)
    nj = len(cols[0])
    for i in range(ny):
        yc = -CLIFF_Y + i + 0.5
        for j in range(nj - 1):
            if j < 13 and any(h0 < yc < h1 for h0, h1 in holes):
                continue
            f = bm.faces.new((cols[i][j], cols[i + 1][j], cols[i + 1][j + 1], cols[i][j + 1]))
            for loop in f.loops:
                c = loop.vert.co
                loop[uv].uv = (c.y / ROCK_TILE, (c.z - c.x) / ROCK_TILE)
    front = T.new_obj("cliff_front", bm, ("stone",))
    orient_sheet(front, (1.0, 0.0, 0.3))
    kit.add(front)
    # Ends of the cliff, displaced outward; the flat top behind the lip; the back.
    nz = int(CLIFF_H / 0.5)
    for sy in (-1, 1):
        bm = bmesh.new()
        uv = bm.loops.layers.uv.new("UVMap")
        grid = [[None] * (nz + 1) for _ in range(49)]
        for i in range(49):
            for j in range(nz + 1):
                x, z = -i * 1.0, j * 0.5
                m = min(1.0, -x / 2.0) * min(1.0, (CLIFF_H - z) / 1.5) * min(1.0, (x - CLIFF_BACK) / 2.0)
                d = cliff_disp(x * sy + 200.0, z, off) * m
                grid[i][j] = bm.verts.new((x, sy * (CLIFF_Y + d), z))
        for i in range(48):
            for j in range(nz):
                f = bm.faces.new((grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]))
                for loop in f.loops:
                    c = loop.vert.co
                    loop[uv].uv = (c.x / ROCK_TILE, c.z / ROCK_TILE)
        o = T.new_obj("cliff_end", bm, ("stone",))
        orient_sheet(o, (0.0, sy, 0.0))
        kit.add(o)
        # Close the gap between the end wall and the lip's profile.
        y = sy * CLIFF_Y
        prof = [(x, z) for x, z, *_ in profile(y, off)][44:]
        bm = bmesh.new()
        bm.loops.layers.uv.new("UVMap")
        vs = [bm.verts.new((x, y, z)) for x, z in prof] + [bm.verts.new((LIP_X, y, CLIFF_H)), bm.verts.new((0.0, y, CLIFF_H))]
        bm.faces.new(vs)
        o = T.new_obj("lip_end", bm, ("stone",))
        fix_normals_toward(o, (CLIFF_BACK / 2, 0.0, CLIFF_H / 2), True)
        kit.add(o)
    top_rects = [(CLIFF_BACK, LIP_X, -CLIFF_Y + k * 5.0, -CLIFF_Y + (k + 1) * 5.0) for k in range(30)]
    for o in (plane("cliff_top", top_rects, lambda u, v: (u, v, CLIFF_H), "stone"),
              plane("cliff_back", [(-CLIFF_Y, CLIFF_Y, 0.0, CLIFF_H)], lambda u, v: (CLIFF_BACK, u, v), "stone")):
        fix_normals_toward(o, (CLIFF_BACK / 2, 0.0, CLIFF_H / 2), True)
        kit.add(o)


def orient_sheet(o, direction):
    """Flip a consistently wound sheet as a whole so it faces `direction` on (area-weighted) average."""
    d = Vector(direction)
    if sum(p.normal.dot(d) * p.area for p in o.data.polygons) < 0:
        for p in o.data.polygons:
            p.flip()


def fix_normals_toward(o, inside_point, outward):
    """Make every face of `o` face away from `inside_point` (outward=True) or toward it."""
    me = o.data
    for p in me.polygons:
        to_c = Vector(inside_point) - p.center
        if (p.normal.dot(to_c) > 0) == outward:
            p.flip()


def hall(kit, hz):
    yc = -hz
    a = HALL_W / 2
    chaitya = hz in CHAITYA
    cells = [] if chaitya else [(-5.0, 1.1), (-9.5, 1.1), (-14.0, 1.1)]
    # Side walls (with cell doorways in viharas), back wall, floor, ceiling.
    for sy in (-1, 1):
        holes = [(x - w / 2, x + w / 2, 0.0, 2.2) for x, w in cells]
        o = plane("hwall", rect_minus(-HALL_D, 0.0, 0.0, HALL_H, holes), lambda u, v, sy=sy: (u, yc + sy * a, v), "stone")
        fix_normals_toward(o, (-HALL_D / 2, yc, HALL_H / 2), False)
        kit.add(o)
        for x, w in cells:
            # Cell: a dark room beyond a short passage, with a carved door frame.
            for (p0, p1) in (((x - w / 2, 0.0), (x - w / 2, 2.2)), ((x + w / 2, 0.0), (x + w / 2, 2.2))):
                o = plane("cellside", [(0.0, 1.0, 0.0, 2.2)], lambda u, v, px=p0[0], sy=sy: (px, yc + sy * (a + u), v), "stone")
                fix_normals_toward(o, (x, yc + sy * (a + 0.5), 1.1), False)
                kit.add(o)
            o = plane("celltop", [(x - w / 2, x + w / 2, 0.0, 1.0)], lambda u, v, sy=sy: (u, yc + sy * (a + v), 2.2), "stone")
            fix_normals_toward(o, (x, yc + sy * (a + 0.5), 1.1), False)
            kit.add(o)
            kit.add(T.box("cellback", (x - w / 2, min(yc + sy * (a + 1.0), yc + sy * (a + 1.05)), 0.0),
                          (x + w / 2, max(yc + sy * (a + 1.0), yc + sy * (a + 1.05)), 2.2), "shadow"))
            for px in (x - w / 2 - 0.12, x + w / 2 + 0.12):
                kit.add(T.box("cjamb", (px - 0.12, yc + sy * a - 0.1, 0.0), (px + 0.12, yc + sy * a + 0.1, 2.35), "trim"))
            kit.add(T.box("clintel", (x - w / 2 - 0.3, yc + sy * a - 0.12, 2.2), (x + w / 2 + 0.3, yc + sy * a + 0.12, 2.5), "trim"))
    back = plane("hback", [(yc - a, yc + a, 0.0, HALL_H)], lambda u, v: (-HALL_D, u, v), "stone")
    fix_normals_toward(back, (-HALL_D / 2, yc, HALL_H / 2), False)
    kit.add(back)
    floor = plane("hfloor", [(-HALL_D, 0.0, yc - a, yc + a)], lambda u, v: (u, v, 0.04), "stone")
    fix_normals_toward(floor, (-HALL_D / 2, yc, HALL_H / 2), False)
    kit.add(floor)
    if chaitya:
        # Barrel vault with stone ribs (copied in the rock from timber halls), closed at the front.
        bm = bmesh.new()
        bm.loops.layers.uv.new("UVMap")
        seg = 16
        rows = []
        for xi, x in enumerate((-HALL_D, -0.03)):
            rows.append([bm.verts.new((x, yc + a * math.cos(math.pi * k / seg), HALL_H + 3.2 * math.sin(math.pi * k / seg)))
                         for k in range(seg + 1)])
        for k in range(seg):
            bm.faces.new((rows[0][k], rows[0][k + 1], rows[1][k + 1], rows[1][k]))
        bm.faces.new(list(reversed(rows[0])))
        bm.faces.new(rows[1])
        vault = T.new_obj("vault", bm, ("stone",))
        fix_normals_toward(vault, (-HALL_D / 2, yc, HALL_H + 0.5), False)
        proj_uvs(vault, ROCK_TILE)
        kit.add(vault)
        for k in range(14):
            x = -HALL_D + 0.7 + k * 1.4
            pts = [Vector((x, yc + (a - 0.1) * math.cos(math.pi * t / 12), HALL_H + 3.1 * math.sin(math.pi * t / 12))) for t in range(13)]
            for p0, p1 in zip(pts, pts[1:]):
                kit.add(cylinder_between("rib", p0, p1, 0.12, "trim", 6))
        # Rock-cut stupa at the apse end.
        sc = (-HALL_D + 4.0, yc)
        kit.add(T.lathe("cdrum", [(2.25, 0.0), (2.25, 0.3), (2.15, 0.35), (2.15, 1.3), (2.25, 1.35), (2.25, 1.6)], 24, sc, uv_strip="bead"))
        dome = [(2.12 * math.cos(t * math.pi / 2 / 8), 1.6 + 2.12 * math.sin(t * math.pi / 2 / 8)) for t in range(8)]
        kit.add(T.lathe("cdome", dome + [(0.6, 3.62)], 24, sc))
        for k, (r, z0) in enumerate(((0.55, 3.6), (0.65, 3.95), (0.75, 4.1), (0.85, 4.25))):
            kit.add(T.box("charmika", (sc[0] - r, sc[1] - r, z0), (sc[0] + r, sc[1] + r, z0 + (0.35 if k == 0 else 0.15)), "trim" if k else "stone"))
        kit.add(T.lathe("cchatra", [(0.06, 4.4), (0.06, 5.2), (0.7, 5.25), (0.0, 5.45)], 16, sc))
    else:
        ceil = plane("hceil", [(-HALL_D, 0.0, yc - a, yc + a)], lambda u, v: (u, v, HALL_H), "stone")
        fix_normals_toward(ceil, (-HALL_D / 2, yc, HALL_H / 2), False)
        kit.add(ceil)
        # Painted coffers on the vihara ceiling: a grid of beams.
        for k in range(1, 5):
            x = -HALL_D + k * HALL_D / 5
            kit.add(T.box("cbeam", (x - 0.2, yc - a, HALL_H - 0.3), (x + 0.2, yc + a, HALL_H), "trim"))
        # Shrine in the back wall: a frame and a large seated Buddha.
        # Piece space faces -Y; turn it to face +X, out of the back wall into the hall.
        M = Matrix.Translation((-HALL_D, yc, 0.0)) @ Matrix.Rotation(math.pi / 2, 4, "Z")
        for sx in (-1, 1):
            kit.add(transform(T.box("spil", (sx * 1.9 - 0.3, -0.45, 0.0), (sx * 1.9 + 0.3, 0.0, 4.2), "trim"), M))
        kit.add(transform(T.box("slintel", (-2.3, -0.5, 4.2), (2.3, 0.0, 4.8), "trim"), M))
        kit.add(transform(T.box("splinth", (-1.5, -1.2, 0.0), (1.5, -0.02, 0.8), "stone"), M))
        seated_buddha(kit, M @ Matrix.Translation((0.0, -0.5, 0.8)), 2.4)
    # Pillars: two rows down the hall, two porch pillars outside each side of the centre line.
    for xi in range(4):
        for sy in (-1, 1):
            T.pillar(kit, -3.0 - xi * 4.0, yc + sy * 3.6, 0.0, HALL_H)
    for sy in (-1, 1):
        for off in (2.0, 4.6):
            T.pillar(kit, 0.9, yc + sy * off, 0.0, HALL_H)
    facade(kit, yc, chaitya)


def spur(kit):
    """A rock spur buttressing the north end of the cliff, with a stair cut up its back."""
    y0, y1 = SPUR
    s0, s1 = y0 + 1.0, y1 - 1.0
    bm = bmesh.new()
    bm.loops.layers.uv.new("UVMap")
    steps = int(round(CLIFF_H / 0.22))
    run = SPUR_RUN / steps
    rise = CLIFF_H / steps
    prof = [(SPUR_RUN, 0.0)]
    for k in range(steps):
        x = SPUR_RUN - k * run
        prof += [(x, (k + 1) * rise), (x - run, (k + 1) * rise)]
    left = [bm.verts.new((x, s0, z)) for x, z in prof]
    right = [bm.verts.new((x, s1, z)) for x, z in prof]
    for i in range(len(prof) - 1):
        bm.faces.new((left[i], left[i + 1], right[i + 1], right[i]))
    o = T.new_obj("steps", bm, ("trim",))
    fix_normals_toward(o, (SPUR_RUN / 2, (s0 + s1) / 2, -50.0), True)
    uvl = o.data.uv_layers[0]
    v0, v1 = texgen.strip_v("plain")
    for p in o.data.polygons:
        for li in p.loop_indices:
            c = o.data.vertices[o.data.loops[li].vertex_index].co
            uvl.data[li].uv = (c.y / 2.0, v0 + (v1 - v0) * (0.5 + 0.5 * math.sin(c.x * 3.0)))
    kit.add(o)
    # Rock mass: parapets either side of the flight and the sloping sides of the spur.
    off = Vector((3.3, 9.1, 0.0))
    for (ya, yb, out) in ((y0, s0, -1), (s1, y1, 1)):
        bm = bmesh.new()
        uv = bm.loops.layers.uv.new("UVMap")
        n = 28
        top = lambda x: CLIFF_H * (1.0 - x / SPUR_RUN) + 0.9
        outer_y = ya if out < 0 else yb
        inner_y = yb if out < 0 else ya
        rows = []
        for i in range(n + 1):
            x = SPUR_RUN * i / n
            d = noise.fractal(Vector((x * 0.15, 0.0, outer_y)) + off, 0.5, 2.0, 3) * 0.6
            zt = top(x)
            row = [bm.verts.new((x, inner_y, zt - 0.9)), bm.verts.new((x, inner_y, zt)), bm.verts.new((x, outer_y + out * max(0.0, d), zt))]
            for k in range(1, 9):
                z = zt * (1 - k / 8)
                fade = min(1.0, (zt - z) / 1.0)
                dd = 0.4 * (z < 0.01) + max(-0.3, cliff_disp(x * 1.7 + outer_y * 3.0, z, off) + 0.6) * fade
                row.append(bm.verts.new((x, outer_y + out * (max(0.0, d) * (1 - fade) + dd), z)))
            rows.append(row)
        for i in range(n):
            for k in range(len(rows[0]) - 1):
                f = bm.faces.new((rows[i][k], rows[i + 1][k], rows[i + 1][k + 1], rows[i][k + 1]))
                for loop in f.loops:
                    c = loop.vert.co
                    loop[uv].uv = (c.x / ROCK_TILE, (c.z + c.y * 0.3) / ROCK_TILE)
        end = bm.faces.new([rows[n][k] for k in range(len(rows[0]))])
        for loop in end.loops:
            c = loop.vert.co
            loop[uv].uv = (c.y / ROCK_TILE, c.z / ROCK_TILE)
        o = T.new_obj("spur", bm, ("stone",))
        orient_sheet(o, (0.0, out, 0.0))
        kit.add(o)
    # The spur's front end below the first step.
    kit.add(plane("spur_end", [(y0, y1, 0.0, 0.9)], lambda u, v: (SPUR_RUN + 0.02, u, v), "stone", flip=True))


def ajanta():
    kit = T.Kit()
    cliff(kit)
    for hz in HALLS:
        hall(kit, hz)
    spur(kit)
    return finish("ajanta", kit, lods=(0.35,))


def basalt_rock(seed):
    """Angular basalt block: a lumpy sphere, decimated to hard facets."""
    import hampi
    kit = T.Kit()
    o = hampi.boulder(seed)
    o["smooth"] = False
    d = o.modifiers.new("d", "DECIMATE")
    d.ratio = 0.18
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.modifier_apply(modifier="d")
    for v in o.data.vertices:
        v.co.z *= 0.8
    kit.add(o)
    obj = geo.join(kit.objs, f"rock_basalt_{seed}_LOD0")
    for p in obj.data.polygons:
        p.use_smooth = False
    proj_uvs(obj, 1.5)
    T.bake_weathering(obj)
    export.triangulate(obj)
    return [obj]


# ------------------------------------------------------------------ the great stupa

def vedika(kit, R, z0, height, gaps, gap_half, post_step=1.3, rail_n=3):
    """Circular railing: octagonal posts, lens-section crossbars, a rounded coping (ushnisha).
    `gaps` are angles of the openings (each 2*gap_half wide in metres)."""
    da = gap_half / R
    spans = []
    if gaps:
        g = sorted(gaps)
        for i, a0 in enumerate(g):
            a1 = g[(i + 1) % len(g)] + (2 * math.pi if i == len(g) - 1 else 0.0)
            spans.append((a0 + da, a1 - da))
    else:
        spans.append((0.0, 2 * math.pi))
    pw = 0.16 * height / 2.6 + 0.08
    for a0, a1 in spans:
        n = max(2, int((a1 - a0) * R / post_step))
        for k in range(n + 1):
            a = a0 + (a1 - a0) * k / n
            c = (R * math.cos(a), R * math.sin(a))
            kit.add(T.lathe("vpost", [(pw, z0), (pw, z0 + height * 0.86)], 8, c))
        for r in range(rail_n):
            zc = z0 + height * (0.2 + 0.22 * r)
            pts = [Vector((R * math.cos(a0 + (a1 - a0) * t / 48), R * math.sin(a0 + (a1 - a0) * t / 48), zc)) for t in range(49)]
            frames = geo.path_frames(pts, Vector((0, 0, 1)))
            rings = [geo.ellipse(pw * 0.55, height * 0.08, 8) for _ in pts]
            o = geo.loft("suchi", frames, rings, cap_start="fan", cap_end="fan")
            o.data.materials.append(T.MATS["stone"])
            kit.add(o)
        pts = [Vector((R * math.cos(a0 + (a1 - a0) * t / 64), R * math.sin(a0 + (a1 - a0) * t / 64), z0 + height * 0.93)) for t in range(65)]
        frames = geo.path_frames(pts, Vector((0, 0, 1)))
        rings = [[(x * 1.4, y) for x, y in geo.ellipse(pw * 1.0, height * 0.08, 12)] for _ in pts]
        o = geo.loft("ushnisha", frames, rings, cap_start="fan", cap_end="fan")
        o.data.materials.append(T.MATS["trim"])
        uvl = o.data.uv_layers[0]
        v0, v1 = texgen.strip_v("lata")
        for d in uvl.data:
            d.uv = (d.uv[1] / 1.5, v0 + (v1 - v0) * ((d.uv[0] / (2 * math.pi * pw)) % 1.0))
        kit.add(o)


def torana(kit, M):
    """Sanchi gateway in local space: X across the gate, -Y outward (away from the stupa), Z up.
    Two square pillars with capitals, three architraves with volute ends separated by dies,
    a dharmachakra above and triratnas over the pillars."""
    objs = []
    for sx in (-1, 1):
        x = sx * 2.2
        objs.append(T.box("tpil", (x - 0.35, -0.35, 0.0), (x + 0.35, 0.35, 5.0), "trim"))
        objs.append(T.box("tcap", (x - 0.5, -0.5, 5.0), (x + 0.5, 0.5, 5.2)))
        objs.append(T.lathe("tbell", [(0.35, 5.2), (0.48, 5.45), (0.45, 5.6)], 12, (x, 0.0), uv_strip="padma"))
        objs.append(T.box("tabacus", (x - 0.55, -0.55, 5.6), (x + 0.55, 0.55, 5.9), "trim"))
    for k, (z0, half) in enumerate(((5.9, 3.6), (6.85, 3.8), (7.8, 4.0))):
        objs.append(T.box("archi", (-half, -0.25, z0), (half, 0.25, z0 + 0.45), "trim"))
        for sx in (-1, 1):
            for side in (-1, 1):
                objs.append(cylinder_between("volute", (sx * half, side * 0.25, z0 + 0.22), (sx * half, side * 0.3, z0 + 0.22), 0.36, "stone", 16))
                objs.append(cylinder_between("volute2", (sx * half, side * 0.3, z0 + 0.22), (sx * half, side * 0.34, z0 + 0.22), 0.2, "trim", 12))
        if k < 2:
            for sx in (-1, 1):
                objs.append(T.box("die", (sx * 2.2 - 0.3, -0.22, z0 + 0.45), (sx * 2.2 + 0.3, 0.22, z0 + 0.95), "trim"))
            for x in (-1.2, 0.0, 1.2):
                objs.append(T.lathe("baluster", [(0.09, z0 + 0.45), (0.13, z0 + 0.6), (0.09, z0 + 0.75), (0.12, z0 + 0.95)], 8, (x, 0.0)))
    # Dharmachakra on the top architrave, triratnas over the pillars.
    objs.append(cylinder_between("chakra", (0, -0.1, 9.0), (0, 0.1, 9.0), 0.75, "trim", 24))
    objs.append(cylinder_between("chub", (0, -0.16, 9.0), (0, 0.16, 9.0), 0.18, "stone", 12))
    objs.append(T.box("cpost", (-0.12, -0.12, 8.25), (0.12, 0.12, 8.4)))
    for sx in (-1, 1):
        x = sx * 2.2
        objs.append(T.box("tri_base", (x - 0.12, -0.1, 8.25), (x + 0.12, 0.1, 8.75)))
        objs.append(cylinder_between("tri_ring", (x, -0.1, 8.95), (x, 0.1, 8.95), 0.3, "stone", 16))
        for s2 in (-1, 0, 1):
            objs.append(uv_sphere("tri_tip", (x + s2 * 0.24, 0.0, 9.3 + (0.1 if s2 == 0 else 0.0)), (0.09, 0.09, 0.16), "stone", 8, 6))
    # Yakshi bracket figures between the pillar capitals and the lowest architrave ends.
    for sx in (-1, 1):
        objs.append(cylinder_between("yakshi", (sx * 2.6, 0.0, 5.3), (sx * 3.3, 0.0, 6.0), 0.12, "stone", 8))
        objs.append(uv_sphere("yhead", (sx * 3.35, 0.0, 6.05), (0.1, 0.1, 0.12), "stone", 8, 6))
    for o in objs:
        kit.add(transform(o, M))


def stupa():
    kit = T.Kit()
    # Medhi: the drum terrace, moulded, with a railing round its walkway and a stair on the west.
    prof = [(0.0, 11.25, "plain"), (0.35, 11.1, "bead"), (0.55, 11.0, None), (2.7, 11.0, "dentil"), (3.0, 11.15, None)]
    kit.add(T.lathe("medhi", [(r, z) for z, r, _ in prof], 64, (0, 0)))
    vedika(kit, 10.75, 3.0, 0.95, [math.pi], 1.4, post_step=0.9, rail_n=2)
    # Anda: the hemispherical dome, matching the gameplay sphere (r 9.5 about z = 2).
    pts = []
    for k in range(17):
        z = 3.0 + (11.2 - 3.0) * k / 16
        pts.append((math.sqrt(max(0.0, 9.5 ** 2 - (z - 2.0) ** 2)), z))
    kit.add(T.lathe("anda", pts + [(0.0, 11.2)], 64, (0, 0)))
    # Harmika: square railing on the crown, the shaft and three diminishing parasols.
    kit.add(T.box("hcore", (-1.3, -1.3, 11.2), (1.3, 1.3, 12.6)))
    for k in range(4):
        a = k * math.pi / 2
        for t in range(5):
            u = -1.5 + 3.0 * t / 4
            x, y = 1.5 * math.cos(a) - u * math.sin(a), 1.5 * math.sin(a) + u * math.cos(a)
            kit.add(T.box("hpost", (x - 0.1, y - 0.1, 11.2), (x + 0.1, y + 0.1, 12.7)))
    for z in (11.55, 11.95):
        kit.add(T.molded("hrail", [(z, T.rect(1.52, 1.52), "bead"), (z + 0.18, T.rect(1.52, 1.52), None)]))
    kit.add(T.molded("hcoping", [(12.7, T.rect(1.65, 1.65), "lata"), (13.0, T.rect(1.65, 1.65), None)]))
    kit.add(T.lathe("yasti", [(0.15, 13.0), (0.12, 16.0)], 10, (0, 0)))
    for z, r in ((13.6, 1.6), (14.5, 1.2), (15.4, 0.8)):
        kit.add(T.lathe("chhatra", [(0.12, z - 0.05), (r, z), (r, z + 0.12), (0.12, z + 0.3)], 24, (0, 0), uv_strip="padma"))
    # Stair up to the medhi (west, game -X), with balustrades.
    steps = 14
    for k in range(steps):
        x0 = -16.0 + k * 5.0 / steps
        kit.add(T.box("stupa_step", (x0, -1.25, 0.0), (x0 + 5.0 / steps + 0.02, 1.25, (k + 1) * 3.0 / steps)))
    for sy in (-1, 1):
        bm = bmesh.new()
        bm.loops.layers.uv.new("UVMap")
        pts = [(-16.3, 0.0), (-16.3, 0.9), (-11.0, 3.9), (-10.8, 3.9), (-10.8, 0.0)]
        ys = (sy * 1.25, sy * 1.6)
        vs = [[bm.verts.new((x, y, z)) for (x, z) in pts] for y in ys]
        bm.faces.new(vs[0])
        bm.faces.new(list(reversed(vs[1])))
        for k in range(len(pts)):
            j = (k + 1) % len(pts)
            bm.faces.new((vs[0][k], vs[0][j], vs[1][j], vs[1][k]))
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        o = T.new_obj("balustrade", bm, ("stone",))
        T.cube_uvs(o)
        kit.add(o)
    # Ground railing with four entrances, each faced by a torana.
    vedika(kit, 15.0, 0.0, 2.6, [0.0, math.pi / 2, math.pi, 1.5 * math.pi], 2.0)
    for q in range(4):
        a = q * math.pi / 2
        M = Matrix.Rotation(a, 4, "Z") @ Matrix.Translation((17.0, 0.0, 0.0)) @ Matrix.Rotation(math.pi / 2, 4, "Z")
        torana(kit, M)
    # Paving of the circumambulatory path.
    kit.add(T.lathe("path", [(11.3, 0.02), (14.8, 0.02)], 64, (0, 0)))
    return finish("stupa", kit, lods=(0.3,))


def main():
    export.reset_scene()
    only = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []

    def run(key, mat_setup, make, group=None):
        if only and key not in only and group not in only:
            return
        for o in list(bpy.data.objects):
            bpy.data.objects.remove(o)
        mat_setup()
        lods = make()
        print(f"[buddhist] {key}: {export.export_glb(lods, os.path.join(OUT, f'{key}.gltf'))}")

    run("stupa", sandstone_mats, stupa)
    run("ajanta", basalt_mats, ajanta)
    for seed in (1, 2, 3):
        run(f"rock_basalt_{seed}", basalt_mats, lambda s=seed: basalt_rock(s), "rocks")


if __name__ == "__main__":
    main()

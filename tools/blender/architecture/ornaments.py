"""Architectural kit pieces for Indian buildings: cusped arch windows and doorways, a fort gate
arch, a jharokha balcony with a bangaldar roof, and a domed chhatri.

Every piece is authored at real size with its wall plane at Y=0 (Blender), opening centred on
X=0, sill at Z=0, front face toward -Y (glTF +Z). Pieces span a 0.35 m wall; the game scales Y
for thicker walls.

blender -b --factory-startup -P tools/blender/architecture/ornaments.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import export  # noqa: E402
import geo  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402

WALL_T = 0.35
OUT = os.path.join(T.ROOT, "assets", "models", "props")


# ------------------------------------------------------------------ arch outlines

def cusped_arch(a, spring, apex, lobes=7, cusp=0.07, samples=96):
    """Inner edge of a pointed, multifoil (cusped) arch from the left springing to the right.
    a: half width; spring: springing height; apex: crown height."""
    r = apex - spring
    c = (r * r - a * a) / (2 * a) if r > a else 0.0
    R = c + a
    pts = []
    for k in range(samples + 1):
        t = k / samples
        # Left arc (centre at +c) for t<0.5, mirrored right arc after.
        u = t * 2 if t < 0.5 else (1 - t) * 2
        ang_end = math.atan2(r, -c) if c > 0 else math.pi / 2
        ang = math.pi + (ang_end - math.pi) * u if c > 0 else math.pi - (math.pi / 2) * u
        x = c + R * math.cos(ang) if c > 0 else a * math.cos(ang)
        z = spring + (R * math.sin(ang) if c > 0 else r * math.sin(ang))
        if t >= 0.5:
            x = -x
        # Cusps: the curve dips toward the opening centre at lobe junctions.
        lobe = abs(math.sin(math.pi * lobes * t))
        inward = cusp * (1 - lobe) ** 4 if 0.02 < t < 0.98 else 0.0
        d = Vector((0 - x, spring - z))
        if d.length > 1e-6:
            d.normalize()
        pts.append((x + d.x * inward, z + d.y * inward))
    return pts


def solid_from_outline(name, outline, y0, y1, mat, strip=None, u_scale=1.0):
    """Extrude a planar XZ outline (CCW seen from -Y) from y0 to y1 into a closed solid."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    front = [bm.verts.new((x, y0, z)) for x, z in outline]
    back = [bm.verts.new((x, y1, z)) for x, z in outline]
    f1 = bm.faces.new(front)
    f2 = bm.faces.new(list(reversed(back)))
    n = len(outline)
    sides = []
    for i in range(n):
        j = (i + 1) % n
        sides.append(bm.faces.new((front[j], front[i], back[i], back[j])))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    v0, v1 = texgen.strip_v(strip) if strip else (0.0, 1.0)
    zs = [z for _, z in outline]
    zmin, zmax = min(zs), max(zs)
    for f in bm.faces:
        for loop in f.loops:
            co = loop.vert.co
            if f in (f1, f2):
                vv = (co.z - zmin) / max(zmax - zmin, 1e-6)
                loop[uv].uv = (co.x / u_scale, v0 + (v1 - v0) * vv) if strip else (co.x / T.TILE, co.z / T.TILE)
            else:
                loop[uv].uv = ((co.x + co.z) / u_scale, v0 + (v1 - v0) * (co.y - y0) / max(y1 - y0, 1e-6)) if strip else ((co.x + co.z) / T.TILE, co.y / T.TILE)
    bmesh.ops.triangulate(bm, faces=[f1, f2])
    return T.new_obj(name, bm, (mat,))


def offset_curve(pts, d):
    out = []
    n = len(pts)
    for i in range(n):
        p0 = Vector(pts[max(i - 1, 0)])
        p1 = Vector(pts[min(i + 1, n - 1)])
        t = (p1 - p0).normalized()
        nrm = Vector((-t.y, t.x))
        out.append(tuple(Vector(pts[i]) + nrm * d))
    return out


def rim_sweep(name, curve, y, radius, seg=8):
    """Swept round moulding along an XZ curve at depth y (robust where offsets would self-intersect)."""
    pts = [Vector((x, y, z)) for x, z in curve]
    frames = geo.path_frames(pts, Vector((0, 1, 0)))
    rings = [geo.ellipse(radius, radius, seg) for _ in pts]
    obj = geo.loft(name, frames, rings, cap_start="fan", cap_end="fan")
    obj.data.materials.append(T.MATS["trim"])
    uvl = obj.data.uv_layers[0]
    v0, v1 = texgen.strip_v("bead")
    for d in uvl.data:
        d.uv = (d.uv[1] / 0.4, v0 + (v1 - v0) * ((d.uv[0] / (2 * 3.1416 * radius)) % 1.0))
    return obj


def arch_opening(name, w, h, *, lobes=7, cusp=0.07, spring_frac=0.62, wall_t=WALL_T, pilasters=True, chhajja=True, sill=True):
    """An arched opening surround for a rectangular wall hole of w x h (sill at z=0)."""
    kit = T.Kit()
    a = w / 2
    spring = h * spring_frac
    apex = h - 0.02
    inner = cusped_arch(a, spring, apex, lobes, cusp)
    # Spandrel infill: the part of the rectangular hole above the arch, carved with a vine scroll.
    spandrel = inner + [(a, h), (-a, h)]
    kit.add(solid_from_outline("spandrel", spandrel, -wall_t / 2, wall_t / 2, "trim", strip="lata", u_scale=1.2))
    # Arch rim: a rounded moulding swept along the cusped edge on both faces.
    for side in (-1, 1):
        kit.add(rim_sweep("rim", inner, side * (wall_t / 2 + 0.02), 0.045))
    # Alfiz: rectangular frame around the whole arch.
    fw = 0.11
    for (x0, z0, x1, z1) in ((-a - fw * 2, h, a + fw * 2, h + fw), (-a - fw * 2, 0, -a - fw, h), (a + fw, 0, a + fw * 2, h)):
        kit.add(T.box("alfiz", (x0, -wall_t / 2 - 0.06, z0), (x1, wall_t / 2 + 0.06, z1), "trim"))
    if pilasters:
        for sx in (-1, 1):
            x = sx * (a + 0.02)
            for side in (-1, 1):
                y = side * (wall_t / 2 + 0.035)
                kit.add(T.lathe("pil", [(0.05, 0.0), (0.05, 0.06), (0.035, 0.1), (0.035, spring - 0.12), (0.055, spring - 0.06),
                                        (0.06, spring)], 10, (x, y), uv_strip="bead"))
    if sill:
        kit.add(T.box("sill", (-a - 0.25, -wall_t / 2 - 0.14, -0.1), (a + 0.25, wall_t / 2 + 0.14, 0.0), "stone"))
    if chhajja:
        # Thin sloping drip-shade on corbels over the opening, both faces.
        for side in (-1, 1):
            y0 = side * (wall_t / 2)
            y1 = side * (wall_t / 2 + 0.42)
            bm = bmesh.new()
            bm.loops.layers.uv.new("UVMap")
            z = h + 0.2
            vs = [bm.verts.new(v) for v in ((-a - 0.35, y0, z + 0.08), (a + 0.35, y0, z + 0.08), (a + 0.35, y1, z - 0.06),
                                            (-a - 0.35, y1, z - 0.06), (-a - 0.35, y0, z), (a + 0.35, y0, z),
                                            (a + 0.35, y1, z - 0.12), (-a - 0.35, y1, z - 0.12))]
            for f in ((0, 1, 2, 3), (7, 6, 5, 4), (0, 3, 7, 4), (1, 5, 6, 2), (3, 2, 6, 7), (0, 4, 5, 1)):
                bm.faces.new([vs[i] for i in f])
            bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
            o = T.new_obj("chhajja", bm, ("stone",))
            T.cube_uvs(o)
            kit.add(o)
            for x in (-a - 0.15, a + 0.15):
                kit.add(T.box("corbel", (x - 0.06, min(y0, y1 * 0.6), z - 0.28), (x + 0.06, max(y0, y1 * 0.6), z)))
    return finish_piece(name, kit)


def jharokha(name="jharokha"):
    """Projecting balcony window: corbels, carved base, three cusped bays on slim columns,
    a low carved balustrade and a curved bangaldar roof with drooping eaves."""
    kit = T.Kit()
    W, D = 2.0, 0.78
    # Corbels: stepped brackets with a curved profile under the base.
    for i, (wd, dz) in enumerate(((1.0, 0.0), (1.5, 0.18), (1.85, 0.34))):
        kit.add(T.molded(f"jbase{i}", [(dz, T.rect(wd / 2, D * (0.5 + 0.25 * i) / 2, 0, -D * (0.5 + 0.25 * i) / 2), "padma"),
                                       (dz + 0.18, T.rect(wd / 2, D * (0.5 + 0.25 * i) / 2, 0, -D * (0.5 + 0.25 * i) / 2), None)]))
    zb = 0.52
    kit.add(T.molded("jfloor", [(zb, T.rect(W / 2 + 0.08, D / 2 + 0.04, 0, -D / 2), "bead"), (zb + 0.12, T.rect(W / 2 + 0.08, D / 2 + 0.04, 0, -D / 2), None)]))
    # Balustrade panels (carved, like stone jaali) on three sides.
    zp = zb + 0.12
    for (mn, mx) in (((-W / 2, -D, zp), (W / 2, -D + 0.07, zp + 0.55)), ((-W / 2, -D, zp), (-W / 2 + 0.07, 0.0, zp + 0.55)),
                     ((W / 2 - 0.07, -D, zp), (W / 2, 0.0, zp + 0.55))):
        o = T.box("jaali", mn, mx, "trim")
        uvl = o.data.uv_layers[0]
        v0, v1 = texgen.strip_v("gavaksha")
        for loop, d in zip(o.data.loops, uvl.data):
            c = o.data.vertices[loop.vertex_index].co
            d.uv = ((c.x + c.y) / 0.9, v0 + (v1 - v0) * (c.z - zp) / 0.55)
        kit.add(o)
    # Columns and the cusped arcade between them.
    zc = zp + 0.55
    col_h = 1.25
    xs = (-W / 2 + 0.06, -W / 6, W / 6, W / 2 - 0.06)
    for x in xs:
        kit.add(T.lathe("jcol", [(0.05, zc), (0.045, zc + 0.08), (0.035, zc + 0.12), (0.035, zc + col_h - 0.2), (0.06, zc + col_h - 0.12),
                                 (0.045, zc + col_h - 0.06), (0.07, zc + col_h)], 10, (x, -D + 0.04), uv_strip="bead"))
    for i in range(3):
        x0, x1 = xs[i] + 0.05, xs[i + 1] - 0.05
        a = (x1 - x0) / 2
        cx = (x0 + x1) / 2
        inner = [(px + cx, pz) for px, pz in cusped_arch(a, zc + col_h * 0.55, zc + col_h - 0.02, 5, 0.04)]
        sp = list(reversed(inner + [(cx + a, zc + col_h), (cx - a, zc + col_h)]))
        kit.add(solid_from_outline("jarch", sp, -D, -D + 0.08, "trim", strip="lata", u_scale=0.8))
    # Bangaldar roof: curved vault along X whose ridge droops toward both ends.
    zr = zc + col_h
    kit.add(T.box("jlintel", (-W / 2 - 0.05, -D - 0.05, zr), (W / 2 + 0.05, 0.0, zr + 0.1)))
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    nx, nk = 24, 12
    grid = []
    for i in range(nx + 1):
        x = -W / 2 - 0.25 + (W + 0.5) * i / nx
        f = 1 - 0.45 * (2 * x / (W + 0.5)) ** 2
        row = []
        for k in range(nk + 1):
            t = k / nk
            ang = math.pi * t
            y = -D / 2 - (D / 2 + 0.25) * math.cos(ang)
            z = zr + 0.1 + 0.5 * f * math.sin(ang) - 0.12 * (1 - f)
            row.append(bm.verts.new((x, y, z)))
        grid.append(row)
    for i in range(nx):
        for k in range(nk):
            fc = bm.faces.new((grid[i][k], grid[i + 1][k], grid[i + 1][k + 1], grid[i][k + 1]))
            for loop in fc.loops:
                co = loop.vert.co
                loop[uv].uv = (co.x / T.TILE, (co.y + co.z) / T.TILE)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    for fc in bm.faces:
        if fc.normal.z < 0:
            fc.normal_flip()
    roof = T.new_obj("bangaldar", bm, ("stone",))
    sol = roof.modifiers.new("s", "SOLIDIFY")
    sol.thickness = 0.06
    bpy.context.view_layer.objects.active = roof
    bpy.ops.object.modifier_apply(modifier="s")
    kit.add(roof)
    T.kalasha(kit, (0.0, -D / 2), zr + 0.6, 0.22)
    return finish_piece(name, kit)


def chhatri(name="chhatri"):
    """Domed pavilion, authored at size 1 (matches `Builder::chhatri(size=1)`): four pillars,
    cusped arches, a wide chhajja, a lotus-banded drum and a ribbed dome with a gold finial."""
    kit = T.Kit()
    s = 1.0
    h = 1.1 * s
    p = 0.42 * s
    kit.add(T.molded("cbase", [(0.0, T.rect(0.55, 0.55), "bead"), (0.08, T.rect(0.55, 0.55), None)]))
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        kit.add(T.lathe("ccol", [(0.07, 0.08), (0.06, 0.14), (0.045, 0.2), (0.045, h - 0.25), (0.075, h - 0.15), (0.06, h - 0.08),
                                 (0.09, h)], 10, (sx * p, sy * p), uv_strip="bead"))
    for k in range(4):
        ang = k * math.pi / 2
        inner = cusped_arch(p - 0.05, h * 0.55, h - 0.03, 5, 0.035)
        sp = list(reversed(inner + [(p - 0.05, h), (-(p - 0.05), h)]))
        o = solid_from_outline("carch", sp, -p - 0.03, -p + 0.03, "trim", strip="lata", u_scale=0.7)
        o.rotation_euler.z = ang
        bpy.context.view_layer.objects.active = o
        o.select_set(True)
        bpy.ops.object.transform_apply(rotation=True)
        o.select_set(False)
        kit.add(o)
    # Chhajja: thin sloped eave all round.
    kit.add(T.molded("cchhajja", [(h, T.rect(0.66, 0.66), "dentil"), (h + 0.06, T.rect(0.66, 0.66), "plain"),
                                  (h + 0.14, T.rect(0.5, 0.5), None)]))
    kit.add(T.molded("cdrum", [(h + 0.14, T.offset_poly(T.rect(0.44, 0.44), 0.0), "padma"), (h + 0.26, T.rect(0.44, 0.44), None)]))
    dome = [(0.47, h + 0.26), (0.48, h + 0.36), (0.44, h + 0.5), (0.34, h + 0.65), (0.2, h + 0.75), (0.07, h + 0.8), (0.0, h + 0.81)]
    kit.add(T.lathe("cdome", dome, 32, (0.0, 0.0), ribs=16, rib_depth=0.03))
    kit.add(T.lathe("clotus", [(0.1, h + 0.79), (0.16, h + 0.83), (0.08, h + 0.88)], 16, (0.0, 0.0), uv_strip="padma"))
    T.kalasha(kit, (0.0, 0.0), h + 0.86, 0.16)
    return finish_piece(name, kit)


def finish_piece(name, kit):
    obj = geo.join(kit.objs, f"{name}_LOD0")
    obj.data.set_sharp_from_angle(angle=math.radians(40))
    for p in obj.data.polygons:
        p.use_smooth = True
    export.triangulate(obj)
    return obj


def main():
    export.reset_scene()
    texgen.sandstone(T.TEX)
    texgen.trim_sheet(T.TEX)
    T.MATS.update({
        "stone": T.tex_material("sandstone", "sandstone"),
        "trim": T.tex_material("trim", "trim"),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
        "cloth": T.flat_material("dhvaja", (0.95, 0.45, 0.05), 0.0, 0.8),
    })
    pieces = {
        "arch_window": lambda: arch_opening("arch_window", 1.2, 1.2),
        "arch_door": lambda: arch_opening("arch_door", 1.4, 2.4, lobes=9, cusp=0.08, spring_frac=0.68, sill=False, chhajja=False),
        "arch_gate": lambda: arch_opening("arch_gate", 5.0, 5.0, lobes=0, cusp=0.0, spring_frac=0.45, wall_t=2.0, sill=False,
                                          chhajja=False, pilasters=False),
        "jharokha": jharokha,
        "chhatri": chhatri,
    }
    for key, make in pieces.items():
        bpy.ops.object.select_all(action="DESELECT")
        for o in list(bpy.data.objects):
            bpy.data.objects.remove(o)
        obj = make()
        T.bake_weathering(obj)
        counts = export.export_glb([obj], os.path.join(OUT, f"{key}.glb"))
        print(f"[ornament] {key}: {counts}")


if __name__ == "__main__":
    main()

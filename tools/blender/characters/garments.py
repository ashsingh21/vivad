"""Garment and accessory construction on an MPFB body.

Everything here is grown from or measured against the proxy body in its rest (A-) pose, so
results fit any body shape and deform with the same game_engine rig.
"""

import math

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.kdtree import KDTree


class BodyContext:
    def __init__(self, proxy, rig, weights):
        self.proxy = proxy
        self.rig = rig
        self.weights = weights
        me = proxy.data
        self.co = np.array([v.co[:] for v in me.vertices])
        self.height = float(self.co[:, 2].max())
        self.kd = KDTree(len(me.vertices))
        for v in me.vertices:
            self.kd.insert(v.co, v.index)
        self.kd.balance()
        self.group_names = [g.name for g in proxy.vertex_groups]

    def bone(self, name):
        b = self.rig.data.bones[name]
        mw = self.rig.matrix_world
        return mw @ b.head_local, mw @ b.tail_local

    def w(self, vi, *groups):
        d = self.weights[vi]
        return sum(d.get(g, 0.0) for g in groups)

    def verts_where(self, pred):
        return [i for i in range(len(self.co)) if pred(i)]


# ------------------------------------------------------------------ helpers

def new_object(name, me, ctx, parent_rig=True):
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    if parent_rig:
        obj.parent = ctx.rig
        mod = obj.modifiers.new("Armature", "ARMATURE")
        mod.object = ctx.rig
    return obj


def ensure_groups(obj, names):
    for n in names:
        if n not in obj.vertex_groups:
            obj.vertex_groups.new(name=n)


def transfer_weights(obj, ctx, k=6, only=None):
    """Copy bone weights from the nearest body vertices (inverse-distance blend of k neighbours)."""
    ensure_groups(obj, ctx.group_names)
    for g in obj.vertex_groups:
        g.remove(range(len(obj.data.vertices)))
    for v in obj.data.vertices:
        acc = {}
        total = 0.0
        for co, idx, dist in ctx.kd.find_n(v.co, k):
            wgt = 1.0 / max(dist, 1e-4) ** 2
            total += wgt
            for g, x in ctx.weights[idx].items():
                if only is None or g in only:
                    acc[g] = acc.get(g, 0.0) + x * wgt
        s = sum(acc.values()) or 1.0
        for g, x in acc.items():
            if x / s > 0.01:
                obj.vertex_groups[g].add([v.index], x / s, "REPLACE")


def set_weight(obj, group, value=1.0):
    ensure_groups(obj, [group])
    obj.vertex_groups[group].add(list(range(len(obj.data.vertices))), value, "REPLACE")


def add_float_attr(obj, name, values):
    attr = obj.data.attributes.get(name) or obj.data.attributes.new(name, "FLOAT", "POINT")
    attr.data.foreach_set("value", np.asarray(values, dtype=np.float32))


def edge_distance_attr(obj, name="edge_d"):
    """Distance from each vertex to the nearest open boundary (hems, necklines) for border patterns."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    boundary = [v.co.copy() for v in bm.verts if v.is_boundary]
    bm.free()
    if not boundary:
        add_float_attr(obj, name, [1.0] * len(obj.data.vertices))
        return
    kd = KDTree(len(boundary))
    for i, c in enumerate(boundary):
        kd.insert(c, i)
    kd.balance()
    add_float_attr(obj, name, [kd.find(v.co)[2] for v in obj.data.vertices])


def finish_cloth(obj, thickness=0.004, smooth_shading=True):
    if thickness > 0:
        sol = obj.modifiers.new("Solidify", "SOLIDIFY")
        sol.thickness = thickness
        sol.offset = -1.0
        sol.use_rim = True
        sol.use_even_offset = True
        # Solidify before the armature so the shell thickness deforms with the body.
        bpy.context.view_layer.objects.active = obj
        while obj.modifiers[0].name != "Solidify":
            bpy.ops.object.modifier_move_up(modifier="Solidify")
    if smooth_shading:
        for p in obj.data.polygons:
            p.use_smooth = True
    return obj


# ------------------------------------------------------------------ shells

def smooth_boundaries(bm, iterations=10, factor=0.5, keep=None):
    """Relax open edges along themselves so face-selection staircases become clean curves.
    `keep(co)` may return a snapped position (e.g. onto a hem plane) applied after each pass."""
    bverts = [v for v in bm.verts if v.is_boundary]
    nbrs = {v: [e.other_vert(v) for e in v.link_edges if e.is_boundary] for v in bverts}
    for _ in range(iterations):
        new = {}
        for v in bverts:
            ns = nbrs[v]
            if len(ns) == 2:
                new[v] = v.co.lerp((ns[0].co + ns[1].co) * 0.5, factor)
        for v, c in new.items():
            v.co = c
            if keep is not None:
                k = keep(v.co.copy())
                if k is not None:
                    v.co = k


def z_snap(lo, hi, tol=0.06):
    """Snap hem/waist boundary vertices onto horizontal planes."""
    def f(co):
        if abs(co.z - lo) < tol:
            co.z = lo
            return co
        if abs(co.z - hi) < tol:
            co.z = hi
            return co
        return None
    return f


def shell(ctx, name, face_pred, offset_fn, *, smooth=3, thickness=0.004, smooth_factor=0.5, snap=None, edge_iters=10):
    """Duplicate body faces passing `face_pred(vertex_indices)`, relax them and push them out along
    normals by `offset_fn(co, normal) -> metres`. Weights come with the duplicated vertices."""
    bm = bmesh.new()
    bm.from_mesh(ctx.proxy.data)
    bm.verts.ensure_lookup_table()
    keep = set(f.index for f in bm.faces if face_pred([v.index for v in f.verts]))
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.index not in keep], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    smooth_boundaries(bm, edge_iters, keep=snap)
    # Relax body detail (abs, navel, nipples) out of the cloth, keeping hems where they are.
    interior = [v for v in bm.verts if not v.is_boundary]
    for _ in range(smooth):
        bmesh.ops.smooth_vert(bm, verts=interior, factor=smooth_factor, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    bm.normal_update()
    for v in bm.verts:
        v.co += v.normal * offset_fn(v.co.copy(), v.normal.copy())
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = new_object(name, me, ctx)
    ensure_groups(obj, ctx.group_names)  # deform layer indices match the proxy's group order
    finish_cloth(obj, thickness)
    return obj


def bone_param(ctx, bone, co):
    """Fraction along a bone (0 at head, 1 at tail) and distance from its axis."""
    h, t = ctx.bone(bone)
    axis = t - h
    L = axis.length
    s = (co - h).dot(axis) / (L * L)
    radial = (co - (h + axis * s)).length
    return s, radial


def ring(ctx, name, bone, s, *, width=0.03, thick=0.006, offset=0.004, seg=40, beads=0, min_weight=0.4):
    """Rigid ring (armband, bangle, anklet, torque) fitted around a limb at fraction `s` of a bone.
    The limb's cross-section is sampled from the body so the ring hugs it."""
    h, t = ctx.bone(bone)
    axis = (t - h).normalized()
    center = h + (t - h) * s
    # Orthonormal frame around the bone axis.
    ref = Vector((0, 0, 1)) if abs(axis.z) < 0.9 else Vector((1, 0, 0))
    u = axis.cross(ref).normalized()
    v = axis.cross(u).normalized()
    radii = [0.0] * seg
    for i in range(len(ctx.co)):
        if ctx.w(i, bone) < min_weight:
            continue
        p = Vector(ctx.co[i]) - center
        along = p.dot(axis)
        if abs(along) > width:
            continue
        q = p - axis * along
        a = math.atan2(q.dot(v), q.dot(u)) % (2 * math.pi)
        k = int(a / (2 * math.pi) * seg) % seg
        radii[k] = max(radii[k], q.length)
    # Fill gaps and smooth the outline.
    for _ in range(3):
        radii = [max(radii[k], 0.5 * (radii[k - 1] + radii[(k + 1) % seg])) for k in range(seg)]
    radii = [0.25 * radii[k - 1] + 0.5 * radii[k] + 0.25 * radii[(k + 1) % seg] for k in range(seg)]
    bm = bmesh.new()
    prof = 8
    rows = []
    for k in range(seg):
        a = 2 * math.pi * k / seg
        radial = (u * math.cos(a) + v * math.sin(a))
        r = radii[k] + offset + thick * 0.5
        if beads:
            r += thick * 0.35 * max(0.0, math.cos(a * beads))
        c = center + radial * r
        col = []
        for j in range(prof):
            b = 2 * math.pi * j / prof
            # Rounded band: wider along the limb than it is thick.
            col.append(bm.verts.new(c + radial * (math.cos(b) * thick * 0.5) + axis * (math.sin(b) * width * 0.5)))
        rows.append(col)
    for k in range(seg):
        for j in range(prof):
            bm.faces.new((rows[k][j], rows[k][(j + 1) % prof], rows[(k + 1) % seg][(j + 1) % prof], rows[(k + 1) % seg][j]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = new_object(name, me, ctx)
    transfer_weights(obj, ctx, k=8)
    for p in obj.data.polygons:
        p.use_smooth = True
    return obj


def band(ctx, name, bone, s0, s1, offset, *, thickness=0.004, min_weight=0.4, extra_groups=()):
    """Ring around a limb between fractions s0..s1 of a bone (armbands, bangles, anklets)."""
    groups = (bone,) + tuple(extra_groups)

    def pred(vids):
        for i in vids:
            if ctx.w(i, *groups) < min_weight:
                return False
            s, _ = bone_param(ctx, bone, Vector(ctx.co[i]))
            if not (s0 <= s <= s1):
                return False
        return True

    return shell(ctx, name, pred, lambda co, n: offset, smooth=1, thickness=thickness)


# ------------------------------------------------------------------ strips (draped cloth that hangs free)

def strip(ctx, name, path, widths, *, normals=None, thickness=0.004, shrink_to=None, shrink_offset=0.01):
    """Ribbon along `path` (list of Vectors) with per-point width. Faces subdivided across the
    width. If `shrink_to` is given, the ribbon is conformed onto that surface with an offset."""
    n = len(path)
    across = 6
    bm = bmesh.new()
    rows = []
    for i, p in enumerate(path):
        t = (path[min(i + 1, n - 1)] - path[max(i - 1, 0)]).normalized()
        nrm = normals[i] if normals else Vector((0, -1, 0))
        side = t.cross(nrm).normalized()
        w = widths[i]
        rows.append([bm.verts.new(p + side * (w * (k / across - 0.5))) for k in range(across + 1)])
    for i in range(n - 1):
        for k in range(across):
            bm.faces.new((rows[i][k], rows[i][k + 1], rows[i + 1][k + 1], rows[i + 1][k]))
    uv = bm.loops.layers.uv.new("UVMap")
    grid = {v: (k, i) for i, row in enumerate(rows) for k, v in enumerate(row)}
    length = sum((path[i + 1] - path[i]).length for i in range(n - 1))
    wmax = max(widths)
    for f in bm.faces:
        for loop in f.loops:
            k, i = grid[loop.vert]
            # Metric UVs (width x length) so the bake gives it fair texel density.
            loop[uv].uv = (k / across * wmax, i / (n - 1) * length)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = new_object(name, me, ctx)
    if shrink_to is not None:
        sw = obj.modifiers.new("Shrinkwrap", "SHRINKWRAP")
        sw.target = shrink_to
        sw.wrap_method = "NEAREST_SURFACEPOINT"
        sw.wrap_mode = "OUTSIDE_SURFACE"
        sw.offset = shrink_offset
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.modifier_move_to_index(modifier="Shrinkwrap", index=0)
        bpy.ops.object.modifier_apply(modifier="Shrinkwrap")
    transfer_weights(obj, ctx)
    finish_cloth(obj, thickness)
    return obj


# ------------------------------------------------------------------ turban (pagdi)

def turban(ctx, name, *, wraps=6, puff=0.010, tilt=0.12, brow_top=None):
    """Wrapped turban: alternating tilted bands around the cranium, a crown and a tail at the back."""
    head_vs = [i for i in range(len(ctx.co)) if ctx.w(i, "head") > 0.6]
    pts = ctx.co[head_vs]
    top = pts[:, 2].max()
    brow_z = brow_top if brow_top is not None else top - 0.09
    cx, cy = float(pts[:, 0].mean()), float(pts[pts[:, 2] > brow_z][:, 1].mean())

    def extent(z):
        sl = pts[np.abs(pts[:, 2] - z) < 0.012]
        if len(sl) < 6:
            return 0.06, 0.07
        return float(np.abs(sl[:, 0] - cx).max()), float(np.abs(sl[:, 1] - cy).max())

    bm = bmesh.new()
    seg, prof = 48, 8
    base = brow_z + 0.016
    height = top - base + 0.028
    weights_head = []
    for wi in range(wraps):
        f = wi / (wraps - 1)
        bw = 0.036 * (1.0 - 0.2 * f)        # band width
        bt = 0.013                           # band thickness
        # First wrap sits on the skin; the rest climb and fan out.
        zc = base + bw * 0.42 + (height - bw) * 0.82 * f
        # Follow the skull below the crown, then keep the width (turbans are fuller than heads).
        rx, ry = extent(min(zc, base + height * 0.45))
        grow = puff * (1.0 + 1.2 * f)
        rx, ry = rx + grow, ry + grow + 0.004
        # Alternate wraps cross diagonally (left-high / right-high), the hallmark of a tied pagdi.
        tl = 0.022 * (1 if wi % 2 == 0 else -1) * (0.4 + 0.6 * f)
        ring = []
        for s in range(seg):
            a = 2 * math.pi * s / seg
            ca, sa = math.cos(a), math.sin(a)
            # Front (−Y) dips toward the brow, back rises: the classic Rajput slope.
            z_off = tl * ca - 0.010 * max(0.0, -sa) * (1 - f) + 0.012 * max(0.0, sa) * f
            center = Vector((cx + ca * rx, cy + sa * ry, zc + z_off))
            radial = Vector((ca, sa, 0)).normalized()
            col = []
            for k in range(prof):
                b = 2 * math.pi * k / prof
                col.append(bm.verts.new(center + radial * (math.cos(b) * bt * 0.5) + Vector((0, 0, 1)) * (math.sin(b) * bw * 0.5)))
            ring.append(col)
        for s in range(seg):
            for k in range(prof):
                a, b2 = ring[s][k], ring[s][(k + 1) % prof]
                c, d = ring[(s + 1) % seg][(k + 1) % prof], ring[(s + 1) % seg][k]
                bm.faces.new((a, b2, c, d))
    # Crown: a flattened dome filling the top inside the wraps.
    rx, ry = extent(base + height * 0.45)
    crown_c = Vector((cx, cy + 0.005, base + height * 0.74))
    lat, lon = 6, 32
    grid = []
    for i in range(lat + 1):
        phi = (math.pi / 2) * i / lat
        row = []
        for j in range(lon):
            th = 2 * math.pi * j / lon
            row.append(bm.verts.new(crown_c + Vector(((rx + puff * 1.5) * math.cos(phi) * math.cos(th),
                                                       (ry + puff * 1.5) * math.cos(phi) * math.sin(th),
                                                       0.05 * math.sin(phi)))))
        grid.append(row)
    for i in range(lat):
        for j in range(lon):
            a, b2 = grid[i][j], grid[i][(j + 1) % lon]
            c, d = grid[i + 1][(j + 1) % lon], grid[i + 1][j]
            if len({a, b2, c, d}) == 4:
                bm.faces.new((a, b2, c, d))
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = new_object(name, me, ctx)
    set_weight(obj, "head", 1.0)
    for p in obj.data.polygons:
        p.use_smooth = True
    info = {"center": Vector((cx, cy, base)), "top": top, "back": Vector((cx, cy + extent(base + 0.04)[1] + puff, base + 0.04))}
    return obj, info


# ------------------------------------------------------------------ preview materials (shape review)

def flat_material(name, rgb, rough=0.7, metal=0.0):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    m.use_nodes = True
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Base Color"].default_value = (*rgb, 1)
    p.inputs["Roughness"].default_value = rough
    p.inputs["Metallic"].default_value = metal
    return m


def preview_materials(proxy, parts, items):
    skin = flat_material("skin_preview", (0.36, 0.20, 0.12), 0.5)
    proxy.data.materials.clear()
    proxy.data.materials.append(skin)
    for o, role in items:
        col = o.get("preview_color", (0.8, 0.8, 0.8))
        mat = flat_material(f"{o.name}_preview", tuple(col), 0.3 if o.get("metal") else 0.8, 1.0 if o.get("metal") else 0.0)
        o.data.materials.clear()
        o.data.materials.append(mat)

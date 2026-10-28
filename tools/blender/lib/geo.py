"""Procedural geometry: lofts (sweeps of 2D rings along frames), lathes and curves.

Every part is built as a loft so topology is all quads (triangles only at
collapsed tips and fan caps), UVs come out metric and stretch-free, and
per-corner attributes can drive procedural shading before baking.

Ring convention: rings are CCW in the frame's (x_axis, y_axis) plane and
stations advance along x_axis x y_axis, which gives outward-facing quads.
"""

import math

import bmesh
import bpy
from mathutils import Vector

UV_ISLAND_GAP = 50.0  # metric offset that guarantees islands never touch before packing


# --------------------------------------------------------------------------- math helpers

def lerp(a, b, t):
    return a + (b - a) * t


def clamp01(x):
    return max(0.0, min(1.0, x))


def smoothstep(e0, e1, x):
    t = clamp01((x - e0) / (e1 - e0))
    return t * t * (3 - 2 * t)


def interp_keys(keys, t):
    """Monotone-friendly smooth interpolation through (t, value) keys (smoothstep between keys)."""
    if t <= keys[0][0]:
        return keys[0][1]
    for (t0, v0), (t1, v1) in zip(keys, keys[1:]):
        if t <= t1:
            return lerp(v0, v1, smoothstep(t0, t1, t))
    return keys[-1][1]


def catmull_rom(points, samples):
    """Resample a polyline of Vectors as a centripetal-ish Catmull-Rom curve with `samples` points."""
    pts = [points[0] + (points[0] - points[1])] + list(points) + [points[-1] + (points[-1] - points[-2])]
    segs = len(points) - 1
    out = []
    for k in range(samples):
        u = k / (samples - 1) * segs
        i = min(int(u), segs - 1)
        t = u - i
        p0, p1, p2, p3 = pts[i], pts[i + 1], pts[i + 2], pts[i + 3]
        t2, t3 = t * t, t * t * t
        out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2
                          + (-p0 + 3 * p1 - 3 * p2 + p3) * t3))
    return out


def resample_even(points, samples):
    """Re-space a dense polyline evenly by arc length."""
    d = [0.0]
    for a, b in zip(points, points[1:]):
        d.append(d[-1] + (b - a).length)
    total = d[-1]
    out, j = [], 0
    for k in range(samples):
        target = total * k / (samples - 1)
        while j < len(d) - 2 and d[j + 1] < target:
            j += 1
        seg = max(d[j + 1] - d[j], 1e-12)
        out.append(points[j].lerp(points[j + 1], (target - d[j]) / seg))
    return out


def path_frames(points, up_hint):
    """Frames along a path. y_axis stays as close to `up_hint` (the binormal) as possible.

    Returns (origin, x_axis, y_axis) with x_axis x y_axis == tangent.
    """
    frames = []
    for i, p in enumerate(points):
        a = points[max(i - 1, 0)]
        b = points[min(i + 1, len(points) - 1)]
        t = (b - a).normalized()
        y = (up_hint - t * up_hint.dot(t)).normalized()
        x = y.cross(t).normalized()
        frames.append((p.copy(), x, y))
    return frames


def axis_frames(origins, x_axis=Vector((1, 0, 0)), y_axis=Vector((0, 1, 0))):
    return [(Vector(o), x_axis.copy(), y_axis.copy()) for o in origins]


def ellipse(rx, ry, n, phase=0.0):
    return [(rx * math.cos(phase + 2 * math.pi * i / n), ry * math.sin(phase + 2 * math.pi * i / n))
            for i in range(n)]


def rounded_polygon(n_sides, radius, corner, per_corner=3, phase=0.0):
    """Regular polygon whose corners are rounded with `per_corner` points (CCW)."""
    pts = []
    for s in range(n_sides):
        a = phase + 2 * math.pi * s / n_sides
        c = Vector((math.cos(a), math.sin(a))) * (radius - corner / math.cos(math.pi / n_sides))
        for k in range(per_corner):
            b = a - math.pi / n_sides + (2 * math.pi / n_sides) * (k + 0.5) / per_corner
            pts.append((c.x + corner * math.cos(b), c.y + corner * math.sin(b)))
    return pts


def signed_area(ring):
    return 0.5 * sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(ring, ring[1:] + ring[:1]))


# --------------------------------------------------------------------------- loft

def loft(name, frames, rings, *, closed=True, cap_start=None, cap_end=None,
         uv_breaks=(0,), v_splits=(), corner_attrs=None, material=None, merge=False):
    """Build a mesh object by connecting rings placed in frames.

    frames:       list of (origin, x_axis, y_axis)
    rings:        list of rings (same length), each a list of (a, b) 2D points
    closed:       ring wraps around
    cap_start/end: None | "fan" (triangle fan to ring centroid)
    uv_breaks:    ring indices where a new UV island starts (cyclic)
    v_splits:     station indices where the UV island is cut along the length
    corner_attrs: {name: fn(si, ri, ctx) -> float} evaluated per face corner; ri may equal
                  len(ring) for the closing seam of a closed ring. ctx has 'ring_arc', 'col_arc',
                  'ring_len', 'S', 'n'.
    merge:        weld coincident verts (collapsed tips)
    """
    n = len(rings[0])
    S = len(rings)
    assert all(len(r) == n for r in rings), name
    if closed:
        for r in rings:
            area = signed_area(r)
            assert area >= -1e-12, f"{name}: ring is clockwise (area {area})"

    pos = [[o + xa * a + ya * b for a, b in ring] for (o, xa, ya), ring in zip(frames, rings)]

    # Ring arc length (n+1 entries when closed) and column arc length along stations.
    ring_arc = []
    for row in pos:
        acc, arc = 0.0, [0.0]
        cols = n + 1 if closed else n
        for i in range(1, cols):
            acc += (row[i % n] - row[i - 1]).length
            arc.append(acc)
        ring_arc.append(arc)
    col_arc = [[0.0] * n]
    for s in range(1, S):
        col_arc.append([col_arc[-1][i] + (pos[s][i] - pos[s - 1][i]).length for i in range(n)])
    ctx = {"ring_arc": ring_arc, "col_arc": col_arc, "ring_len": [a[-1] for a in ring_arc], "S": S, "n": n}

    bm = bmesh.new()
    verts = [[bm.verts.new(p) for p in row] for row in pos]
    uv_layer = bm.loops.layers.uv.new("UVMap")
    attr_layers = {k: bm.loops.layers.float.new(k) for k in (corner_attrs or {})}

    # Island k spans ring indices [breaks[k], breaks[k+1]]; corner index n is the closing seam.
    breaks = sorted(set(uv_breaks) | {0})

    def island_of(i):
        return max(k for k, b in enumerate(breaks) if b <= i)

    def u_of(s, ci, island):
        return ring_arc[s][ci] - ring_arc[s][breaks[island]] + island * UV_ISLAND_GAP

    split_set = sorted(v_splits)

    def group_of(s):
        return sum(1 for v in split_set if s >= v)

    seg_count = n if closed else n - 1
    for s in range(S - 1):
        g = group_of(s)
        for i in range(seg_count):
            j = i + 1
            vs = (verts[s][i], verts[s][j % n], verts[s + 1][j % n], verts[s + 1][i])
            if len(set(vs)) < 3:
                continue
            try:
                f = bm.faces.new(vs)
            except ValueError:
                continue
            isl = island_of(i)
            # Corners in (station, ring-index) order: (s,i), (s,j), (s+1,j), (s+1,i)
            corners = ((s, i), (s, j), (s + 1, j), (s + 1, i))
            for loop, (cs, ci) in zip(f.loops, corners):
                u = u_of(cs, ci, isl)
                v = col_arc[cs][ci % n] + g * UV_ISLAND_GAP
                loop[uv_layer].uv = (u, v)
                for k, fn in (corner_attrs or {}).items():
                    loop[attr_layers[k]] = fn(cs, ci, ctx)

    def add_cap(s, flip):
        ring = verts[s]
        c = bm.verts.new(sum((v.co for v in ring), Vector()) / n)
        o, xa, ya = frames[s]
        for i in range(n):
            a, b = ring[i], ring[(i + 1) % n]
            tri = (c, b, a) if flip else (c, a, b)
            try:
                f = bm.faces.new(tri)
            except ValueError:
                continue
            for loop in f.loops:
                d = loop.vert.co - o
                loop[uv_layer].uv = (d.dot(xa) + (len(breaks) + 1 + (s > 0)) * UV_ISLAND_GAP, d.dot(ya))
                for k, fn in (corner_attrs or {}).items():
                    ri = i if loop.vert is a else (i + 1) % n if loop.vert is b else i
                    loop[attr_layers[k]] = fn(s, ri, ctx)

    if cap_start == "fan":
        add_cap(0, flip=True)
    if cap_end == "fan":
        add_cap(S - 1, flip=False)

    if merge:
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
        # Drop faces that collapsed to slivers.
        bad = [f for f in bm.faces if f.calc_area() < 1e-14]
        bmesh.ops.delete(bm, geom=bad, context="FACES")

    for f in bm.faces:
        f.smooth = True

    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    if material is not None:
        mesh.materials.append(material)
    return obj


def lathe(name, profile, segments, *, cap_start=None, cap_end=None, corner_attrs=None, material=None,
          z_axis=Vector((0, 0, 1)), origin=Vector((0, 0, 0))):
    """Revolve a (radius, height) profile around z_axis. Profile order sets normal direction:
    list it so that walking the profile, the solid's outside is on the outward-radial side when going +z."""
    ref = Vector((1, 0, 0)) if abs(z_axis.dot(Vector((1, 0, 0)))) < 0.9 else Vector((0, 1, 0))
    xa = (ref - z_axis * ref.dot(z_axis)).normalized()
    ya = z_axis.cross(xa)
    frames = [(origin + z_axis * h, xa, ya) for r, h in profile]
    rings = [ellipse(max(r, 0.0), max(r, 0.0), segments) for r, h in profile]
    return loft(name, frames, rings, cap_start=cap_start, cap_end=cap_end, corner_attrs=corner_attrs,
                material=material, merge=any(r <= 0 for r, _ in profile))


# --------------------------------------------------------------------------- finishing

def finish_hard_surface(obj, *, bevel_width=0.0004, bevel_segments=2, angle_deg=35.0):
    """Mark sharp edges, add a light-catching bevel and weighted normals (applied later)."""
    mesh = obj.data
    mesh.set_sharp_from_angle(angle=math.radians(angle_deg))
    bev = obj.modifiers.new("Bevel", "BEVEL")
    bev.width = bevel_width
    bev.segments = bevel_segments
    bev.limit_method = "ANGLE"
    bev.angle_limit = math.radians(angle_deg)
    bev.harden_normals = True
    bev.use_clamp_overlap = True
    bev.miter_outer = "MITER_ARC"
    wn = obj.modifiers.new("WeightedNormal", "WEIGHTED_NORMAL")
    wn.keep_sharp = True
    wn.weight = 50
    return obj


def apply_all_modifiers(obj):
    bpy.context.view_layer.objects.active = obj
    for m in list(obj.modifiers):
        bpy.ops.object.modifier_apply(modifier=m.name)


def join(objs, name):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = name
    obj.data.name = name
    return obj


def tri_count(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)

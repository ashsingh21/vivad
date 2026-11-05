"""Face details on top of the MPFB head, placed from landmarks measured on the rest-pose mesh:
the male's stubble (a `beard` vertex attribute the skin shader darkens by) and moustache (a
sculpted mesh), the female's bindi and kajal (points the skin shader paints, stored on the
proxy), and the male's hair trimmed to what shows below the pagri.

Proxy and eye meshes face -Y in Blender (MakeHuman), +X is the character's left.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

import garments


class Face:
    def __init__(self, proxy, eyes):
        mw = proxy.matrix_world
        self.co = [mw @ v.co for v in proxy.data.vertices]
        ew = eyes.matrix_world
        pts = [ew @ v.co for v in eyes.data.vertices]
        left = [p for p in pts if p.x > 0]
        right = [p for p in pts if p.x <= 0]
        self.eye_l = sum(left, Vector()) / len(left)
        self.eye_r = sum(right, Vector()) / len(right)
        self.eye_z = (self.eye_l.z + self.eye_r.z) / 2
        # Eyeball radius from its extent.
        self.eye_rad = (max(p.x for p in left) - min(p.x for p in left)) / 2
        self.nose = min((p for p in self.co if abs(p.x) < 0.012 and self.eye_z - 0.07 < p.z < self.eye_z - 0.015), key=lambda p: p.y)
        bm = bmesh.new()
        bm.from_mesh(proxy.data)
        bm.transform(mw)
        self.tree = BVHTree.FromBMesh(bm)
        bm.free()

    def surface(self, x, z):
        """The skin point (and normal) seen from straight in front at (x, z)."""
        hit, n, _, _ = self.tree.ray_cast(Vector((x, self.nose.y - 0.2, z)), Vector((0, 1, 0)))
        return hit, n


def smoothstep(e0, e1, x):
    t = max(0.0, min(1.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def stubble(proxy, f):
    """`beard` attribute, 0..1: upper lip, cheeks below the cheekbones, jaw and chin, fading out
    toward the ears and down the throat, clear of the lips."""
    attr = proxy.data.attributes.new("beard", "FLOAT", "POINT")
    vals = []
    for p in f.co:
        dz = p.z - f.nose.z
        back = p.y - f.eye_l.y          # 0 at the eyes' plane, positive toward the ears
        w = smoothstep(0.0, -0.022, dz) * smoothstep(-0.15, -0.115, dz)
        w *= smoothstep(0.085, 0.05, back) * smoothstep(0.085, 0.06, abs(p.x))
        # Clear of the lips (a band below the moustache line, across the mouth).
        lips = smoothstep(0.03, 0.022, abs(p.x)) * smoothstep(-0.026, -0.03, dz) * smoothstep(-0.056, -0.05, dz)
        w *= 1.0 - lips
        vals.append(w)
    attr.data.foreach_set("value", vals)


def moustache(f, ctx, name="moustache"):
    """A full, slightly drooping moustache along the upper lip, parted at the philtrum, built as a
    closed tube on the skin and skinned to the head."""
    bm = bmesh.new()
    n_seg = 26
    half = 0.033
    rings = []
    z0 = f.nose.z - 0.02
    for i in range(n_seg + 1):
        u = -1.0 + 2.0 * i / n_seg
        x = u * half
        a = abs(u)
        z = z0 - 0.008 * a ** 1.6 - 0.003 * smoothstep(0.75, 1.0, a)
        hit, nrm = f.surface(x, z)
        if hit is None:
            continue
        h = (0.0082 * (1 - a ** 2.2) + 0.002) * (0.75 + 0.25 * smoothstep(0.0, 0.16, a))
        depth = 0.0026 * (1 - 0.6 * a ** 2) + 0.0009
        up = Vector((0, 0, 1))
        side = up.cross(nrm).normalized()
        up = nrm.cross(side).normalized()
        base = hit - nrm * 0.0008
        # Section: skin-top, outer-top, outer-bottom (hanging over the lip), skin-bottom.
        sec = [base + up * h * 0.5, base + up * h * 0.38 + nrm * depth,
               base - up * h * 0.55 + nrm * depth * 1.1, base - up * h * 0.5 + nrm * 0.0005]
        rings.append([bm.verts.new(p) for p in sec])
    for a, b in zip(rings, rings[1:]):
        for k in range(4):
            bm.faces.new((a[k], a[(k + 1) % 4], b[(k + 1) % 4], b[k]))
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    obj = garments.new_object(name, me, ctx)
    garments.ensure_groups(obj, ctx.group_names)
    obj.vertex_groups["head"].add(range(len(me.vertices)), 1.0, "REPLACE")
    obj["kind"] = "hairmesh"
    return obj


def anime_eyes(f, ctx, eyes, name="anime_eyes"):
    """Big painted anime eyes (the way anime 3D characters' are made): a card over each eye socket,
    `W` across and `H` tall, laid onto the skin and eyeball as seen from in front, its depth held
    out over the socket (the eyelids' rims never poke through) and a hair proud of the face;
    skinned to the head. Its texture (finish.anime_eye_decal) is drawn inner corner to outer, so
    the right eye's card is the left's mirrored. Kind `decal`: an alpha card, not baked."""
    W, H, NU, NV = 0.05, 0.036, 16, 12
    bm = bmesh.new()
    mw = eyes.matrix_world
    bm.from_mesh(eyes.data)
    bm.transform(mw)
    eye_tree = BVHTree.FromBMesh(bm)
    bm.free()
    out = bmesh.new()
    uv = out.loops.layers.uv.new("UVMap")
    for c, side in ((f.eye_l, 1.0), (f.eye_r, -1.0)):
        cx, cz = c.x + side * 0.003, c.z + 0.002
        ys = {}
        for i in range(NU + 1):
            for j in range(NV + 1):
                x = cx + side * (i / NU - 0.5) * W
                z = cz + (j / NV - 0.5) * H
                best = None
                for tree in (f.tree, eye_tree):
                    hit = tree.ray_cast(Vector((x, f.nose.y - 0.2, z)), Vector((0, 1, 0)))[0]
                    if hit is not None and (best is None or hit.y < best):
                        best = hit.y
                ys[i, j] = best
        known = [y for y in ys.values() if y is not None]
        fallback = min(known) if known else c.y - f.eye_rad
        # Held out over the socket: each point as far forward as anything within 9 mm of it.
        grid = {}
        for (i, j), y in ys.items():
            near = [ys[a, b] for a in range(max(0, i - 3), min(NU, i + 3) + 1) for b in range(max(0, j - 3), min(NV, j + 3) + 1) if ys[a, b] is not None]
            grid[i, j] = (min(near) if near else fallback) - 0.0015
        verts = {}
        for (i, j), y in grid.items():
            x = cx + side * (i / NU - 0.5) * W
            z = cz + (j / NV - 0.5) * H
            verts[i, j] = out.verts.new((x, y, z))
        for i in range(NU):
            for j in range(NV):
                quad = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
                if side < 0:
                    quad.reverse()
                face = out.faces.new([verts[k] for k in quad])
                for loop, k in zip(face.loops, quad):
                    loop[uv].uv = (k[0] / NU, k[1] / NV)
    # Facing out of the face (-Y).
    for face in out.faces:
        face.normal_update()
        if face.normal.y > 0:
            face.normal_flip()
    me = bpy.data.meshes.new(name)
    out.to_mesh(me)
    out.free()
    for p in me.polygons:
        p.use_smooth = True
    obj = garments.new_object(name, me, ctx)
    garments.ensure_groups(obj, ctx.group_names)
    obj.vertex_groups["head"].add(range(len(me.vertices)), 1.0, "REPLACE")
    obj["kind"] = "decal"
    return obj


def lock(ctx, name, path, widths, thick, forward=None):
    """A chunky anime hair lock along `path`: a lens-shaped section `widths[i]` across and
    `thick[i]` deep (its flat faces toward `forward`, default out of the face, -Y), closed at the
    root and drawn to a point at the tip; skinned to the head."""
    bm = bmesh.new()
    rings = []
    n = len(path)
    for i, p in enumerate(path):
        t = (path[min(i + 1, n - 1)] - path[max(i - 1, 0)]).normalized()
        fwd = forward[i] if forward else Vector((0, -1, 0))
        side = t.cross(fwd).normalized()
        fwd = side.cross(t).normalized()
        w, d = widths[i] * 0.5, thick[i] * 0.5
        ring = []
        for k in range(8):
            a = 2 * math.pi * k / 8
            # A lens: full width at the sides, its depth swelling in the middle.
            ring.append(bm.verts.new(p + side * (w * math.cos(a)) + fwd * (d * math.sin(a) * (0.6 + 0.4 * abs(math.sin(a))))))
        rings.append(ring)
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(8):
            bm.faces.new((r0[k], r0[(k + 1) % 8], r1[(k + 1) % 8], r1[k]))
    bm.faces.new(list(reversed(rings[0])))
    tip = bm.verts.new(path[-1] + (path[-1] - path[-2]).normalized() * widths[-1])
    for k in range(8):
        bm.faces.new((rings[-1][k], rings[-1][(k + 1) % 8], tip))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    obj = garments.new_object(name, me, ctx)
    garments.ensure_groups(obj, ctx.group_names)
    obj.vertex_groups["head"].add(range(len(me.vertices)), 1.0, "REPLACE")
    obj["kind"] = "toonhair"
    return obj


def anime_hair(f, ctx, parts):
    """Anime hair, solid and chunky (alpha cards read as wisps cel shaded): a shell over the skull,
    nine pointed bangs over the forehead to the brows (a longer one between the eyes), a lock each
    side framing the cheeks to the jaw, and a high ponytail of three tapering clumps with a red tie.
    Replaces the MakeHuman hair. Returns (object, role) items."""
    import numpy as np
    co = ctx.co
    head = [i for i in range(len(co)) if ctx.w(i, "head") > 0.5]
    top = float(co[head][:, 2].max())
    ey = f.eye_l.y
    brow = ctx.brow_top if getattr(ctx, "brow_top", None) else f.eye_z + 0.03

    def hairline(p):
        if p[1] < ey + 0.035:
            return brow + 0.028
        if abs(p[0]) > 0.05 and p[1] < ey + 0.1:
            return f.eye_z + 0.018
        return f.eye_z - 0.035

    def pred(vids):
        # Clear of the ears (they stand out past 7 cm either side).
        return all(ctx.w(v, "head") > 0.5 and co[v][2] > hairline(co[v]) and abs(co[v][0]) < 0.071 for v in vids)

    import finish
    finish.ANIME_RING[:] = [top - 0.045, top - 0.03]
    items = []
    cap = garments.shell(ctx, "anime_hair_cap", pred, lambda c, n: 0.013 + 0.004 * max(0.0, c.z - (top - 0.05)) / 0.05, smooth=4, thickness=0.006)
    cap["kind"] = "toonhair"
    items.append((cap, "cover"))
    # Bangs: roots high on the forehead, tips at the brows, each following the face's curve a
    # little proud of it.
    n = 9
    for i in range(n):
        u = (i + 0.5) / n * 2 - 1
        x0 = u * 0.066
        z0 = brow + 0.06 - 0.012 * u * u
        z1 = brow - 0.004 + (0.016 if i == n // 2 else 0.0) - 0.006 * ((i * 7) % 3) + 0.01 * u * u
        path, widths, thick = [], [], []
        for k in range(7):
            t = k / 6
            z = z0 + (z1 - z0) * t
            x = x0 * (1.0 + 0.12 * t) + 0.006 * math.sin(i * 1.7) * t
            hit, _ = f.surface(x, z)
            if hit is None:
                continue
            path.append(hit + Vector((0, -0.012 - 0.006 * math.sin(math.pi * t), 0)))
            widths.append(0.034 * (1 - t) ** 0.8 + 0.002)
            thick.append(0.010 * (1 - t) + 0.002)
        if len(path) > 2:
            items.append((lock(ctx, f"anime_bang_{i}", path, widths, thick), "accessory"))
    # Side locks, framing the face in front of the ears.
    for side in (1.0, -1.0):
        path, widths, thick = [], [], []
        for k in range(8):
            t = k / 7
            z = brow + 0.04 + (f.nose.z - 0.05 - brow - 0.04) * t
            x = side * (0.066 + 0.004 * t)
            hit, _ = f.surface(x, z)
            # Past the cheek's edge the ray misses: carry straight on down from the last point.
            if hit is None or (path and abs(hit.y - path[-1].y) > 0.02):
                if not path:
                    continue
                hit = path[-1] - Vector((side * 0.006, -0.006, 0)) + Vector((0, 0, z - path[-1].z))
            path.append(hit + Vector((side * 0.006, -0.006, 0)))
            widths.append(0.03 * (1 - t) ** 0.7 + 0.003)
            thick.append(0.012 * (1 - t) + 0.002)
        fwd = [Vector((side * 0.7, -0.7, 0)).normalized()] * len(path)
        items.append((lock(ctx, f"anime_sidelock_{int(side)}", path, widths, thick, fwd), "accessory"))
    # The ponytail, high at the back: up and back off the crown, then falling.
    back_y = float(co[head][:, 1].max())
    root = Vector((0.0, back_y - 0.03, top - 0.035))
    for j, (dx, dr) in enumerate(((0.0, 1.0), (-0.016, 0.8), (0.016, 0.8))):
        path, widths, thick = [], [], []
        for k in range(10):
            t = k / 9
            # A cubic from the root, out and up, over and down to between the shoulder blades.
            p0, p1, p2, p3 = root, root + Vector((0, 0.07, 0.05)), root + Vector((dx * 2, 0.14, -0.08)), root + Vector((dx * 3, 0.10, -0.34))
            p = (1 - t) ** 3 * p0 + 3 * (1 - t) ** 2 * t * p1 + 3 * (1 - t) * t * t * p2 + t ** 3 * p3
            path.append(p + Vector((dx * t, 0, 0)))
            w = (0.075 * (1 - t) ** 0.55 + 0.005) * dr * (0.85 + 0.45 * math.sin(math.pi * min(1.0, t * 1.6)))
            widths.append(w)
            thick.append(w * 0.8)
        fwd = [Vector((1, 0, 0))] * len(path)
        items.append((lock(ctx, f"anime_tail_{j}", path, widths, thick, fwd), "accessory"))
    # The tie round its root.
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=16, radius1=0.02, radius2=0.02, depth=0.018,
                          matrix=Matrix.Translation(root + Vector((0, 0.03, 0.02))) @ Matrix.Rotation(-1.1, 4, "X"))
    me = bpy.data.meshes.new("anime_tie")
    bm.to_mesh(me)
    bm.free()
    tie = garments.new_object("anime_tie", me, ctx)
    garments.ensure_groups(tie, ctx.group_names)
    tie.vertex_groups["head"].add(range(len(me.vertices)), 1.0, "REPLACE")
    tie["kind"] = "lacquer"
    items.append((tie, "accessory"))
    # The MakeHuman hair goes.
    if "hair" in parts:
        bpy.data.objects.remove(parts.pop("hair"))
    return items


def trim_hair(hair, below_z):
    """Keep only the hair below a covering turban's lower edge (the rest is hidden inside it)."""
    bm = bmesh.new()
    bm.from_mesh(hair.data)
    mw = hair.matrix_world
    gone = [fc for fc in bm.faces if all((mw @ v.co).z > below_z for v in fc.verts)]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bm.to_mesh(hair.data)
    bm.free()
    return len(gone)


def decorate(variant, proxy, parts, ctx=None):
    """Face marks for the skin shader (stored on the proxy) and, for the male, the stubble mask
    and moustache. Returns extra (object, role) outfit items."""
    f = Face(proxy, parts["eyes"])
    inv = proxy.matrix_world.inverted()
    items = []
    if variant in ("female", "anime"):
        # Kajal rings (the anime one's lash lines) round each eye: both centres then the radius, in
        # the proxy's object space.
        proxy["eyes"] = [*(inv @ f.eye_l), *(inv @ f.eye_r), f.eye_rad]
        if variant == "female":
            hit, _ = f.surface(0.0, f.eye_z + 0.024)
            proxy["bindi"] = list(inv @ hit)
        else:
            # The painted eyes carry their own lashes; the modelled ones would poke through them.
            if "eyelashes" in parts:
                bpy.data.objects.remove(parts.pop("eyelashes"))
            if ctx is not None:
                items.append((anime_eyes(f, ctx, parts["eyes"]), "accessory"))
                items += anime_hair(f, ctx, parts)
    else:
        stubble(proxy, f)
        if ctx is not None:
            items.append((moustache(f, ctx), "accessory"))
    print(f"[face] nose {f.nose.z - f.eye_z:+.3f} below the eyes; eye radius {f.eye_rad:.4f}")
    return items

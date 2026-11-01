"""Face details on top of the MPFB head, placed from landmarks measured on the rest-pose mesh:
the male's stubble (a `beard` vertex attribute the skin shader darkens by) and moustache (a
sculpted mesh), the female's bindi and kajal (points the skin shader paints, stored on the
proxy), and the male's hair trimmed to what shows below the pagri.

regions() paints the masks the head material reads, one vertex attribute each: `lip` (from the
basemesh's own lips group), `blush`, `socket`, `oily`, `pores`, `lines`, and `detail`, the part of
the head that keeps its subdivided triangles.

Proxy and eye meshes face -Y in Blender (MakeHuman), +X is the character's left.
"""

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

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


def basemesh_masks(body, co, names):
    """{group: 0/1 per vertex at `co`}: whether the nearest real skin vertex of the basemesh, as
    shaped by its targets, is in that vertex group of the basemesh."""
    key = body.shape_key_add(name="_fitted", from_mix=True) if body.data.shape_keys else None
    shaped = key.data if key else body.data.vertices
    groups = {g.index: g.name for g in body.vertex_groups}
    member = {n: set() for n in (*names, "body")}
    for v in body.data.vertices:
        for g in v.groups:
            if g.weight > 0.5 and groups.get(g.group) in member:
                member[groups[g.group]].add(v.index)
    tree = KDTree(len(member["body"]))
    for i in member["body"]:
        tree.insert(shaped[i].co, i)
    tree.balance()
    nearest = [tree.find(p)[1] for p in co]
    if key:
        body.shape_key_remove(key)
    return {n: np.array([i in member[n] for i in nearest], dtype=np.float32) for n in names}


def gauss(*terms):
    return np.exp(-sum(t * t for t in terms))


def blur(me, values, passes):
    """Average each vertex with its neighbours, to soften a mask's edge."""
    ends = np.empty(len(me.edges) * 2, dtype=np.int64)
    me.edges.foreach_get("vertices", ends)
    a, b = ends[::2], ends[1::2]
    for _ in range(passes):
        total = values.copy()
        count = np.ones(len(values))
        np.add.at(total, a, values[b])
        np.add.at(total, b, values[a])
        np.add.at(count, a, 1)
        np.add.at(count, b, 1)
        values = total / count
    return values


def regions(proxy, body, f):
    """The material's masks, as vertex attributes on the proxy. Distances are in metres from the
    measured landmarks, so they follow the face targets."""
    co = np.array([p[:] for p in f.co])
    x, y, z = np.abs(co[:, 0]), co[:, 1], co[:, 2]
    ex, ey = abs(f.eye_l.x), f.eye_l.y
    nz, ny = f.nose.z, f.nose.y
    front = np.clip((ey + 0.05 - y) / 0.03, 0, 1)
    me = proxy.data
    group = basemesh_masks(body, f.co, ("lips", "ears"))
    lip = blur(me, group["lips"], 1)
    socket = gauss((x - ex) / 0.03, (z - f.eye_z + 0.002) / 0.02) * front
    cheek = gauss((x - 0.05) / 0.03, (z - nz + 0.004) / 0.028) * front
    nose = gauss(x / 0.014, (z - nz) / 0.016, (y - ny) / 0.02)
    brow = gauss(x / 0.06, (z - f.eye_z - 0.05) / 0.02) * front
    # The crease from the nose's wing to the corner of the mouth.
    t = np.clip((nz - 0.004 - z) / 0.048, 0, 1)
    fold = blur(me, gauss((x - (0.022 + 0.018 * t)) / 0.007) * ((z < nz) & (z > nz - 0.06)) * front, 3)
    masks = {
        "lip": lip,
        "blush": np.clip(cheek + 0.7 * nose + 0.6 * blur(me, group["ears"], 2), 0, 1),
        "socket": socket,
        "oily": np.clip(nose + 0.7 * brow + 0.3 * cheek, 0, 1),
        "pores": np.clip(0.35 + nose + 0.8 * cheek - lip - socket, 0, 1),
        "lines": fold,
    }
    # What the eye reads closest: lids, lips and the nose. The decimator keeps the rest's curvature.
    held = ((socket > 0.4) | (lip > 0.3) | (nose > 0.2)) & (y < ey + 0.02)
    masks["detail"] = held.astype(np.float32)
    for name, vals in masks.items():
        attr = me.attributes.new(name, "FLOAT", "POINT")
        attr.data.foreach_set("value", vals.astype(np.float32))
    print(f"[face] {int(group['lips'].sum())} lip vertices, {int(masks['detail'].sum())} of {len(co)} vertices hold detail")


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


def decorate(variant, proxy, parts, body, ctx=None):
    """Face marks for the skin shader (stored on the proxy) and, for the male, the stubble mask
    and moustache. `body` is the fitted basemesh. Returns extra (object, role) outfit items."""
    f = Face(proxy, parts["eyes"])
    regions(proxy, body, f)
    inv = proxy.matrix_world.inverted()
    items = []
    if variant == "female":
        # Kajal rings round each eye: both centres then the radius, in the proxy's object space.
        proxy["eyes"] = [*(inv @ f.eye_l), *(inv @ f.eye_r), f.eye_rad]
        hit, _ = f.surface(0.0, f.eye_z + 0.024)
        proxy["bindi"] = list(inv @ hit)
    else:
        stubble(proxy, f)
        if ctx is not None:
            items.append((moustache(f, ctx), "accessory"))
    print(f"[face] nose {f.nose.z - f.eye_z:+.3f} below the eyes; eye radius {f.eye_rad:.4f}")
    return items

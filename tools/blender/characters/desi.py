"""Desi looks for the VRoid bodies (vroid.py --outfit, --skin, --hair): Indian clothes modelled as
garments in their own right (outfits.loft: sections sized off the body's widest points, never the
body's own shape), and skin and hair retinted in their textures with the shading kept.

Each outfit returns its garments; vroid.py joins them into the body before its LODs and outlines.
Colours are sRGB. The salwar-suit heroine (anime_salwar), from VRoid's CC0 AvatarSample_G:

blender -b -t 1 -P tools/blender/characters/vroid.py -- build/vroid/cc0/AvatarSample_G.vrm anime_salwar \
    --drop cloth --fit woman --outfit salwar_suit --skin 0.62,0.42,0.30 --hair 0.09,0.06,0.05
python3 tools/glb_textures.py assets/models/characters/anime_salwar.glb
"""

import math

import bmesh
import bpy
import numpy as np
from mathutils import Vector

import garments as g
import outfits as o


def linear(c):
    return tuple(x ** 2.2 for x in c[:3]) + (1.0,)


def cloth(name, rgb, rough=0.85):
    """A plain garment material, cel shaded in the game (toon_*)."""
    m = bpy.data.materials.new(f"toon_{name}")
    m.use_nodes = True
    p = m.node_tree.nodes["Principled BSDF"]
    p.inputs["Base Color"].default_value = linear(rgb)
    p.inputs["Roughness"].default_value = rough
    return m


def dress(obj, mat):
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    return obj


def retint(img, rgb, keep_light=0.0):
    """A texture retinted toward `rgb` with its shading kept: every pixel scaled by the colour over
    the texture's own typical colour (its median), so blush and shade stay where they were.
    `keep_light`: how much of each pixel's own brightness survives (hair keeps its highlights)."""
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h * w, 4)
    rgbs = px[:, :3]
    seen = rgbs[px[:, 3] > 0.5]
    if len(seen) == 0:
        return
    typical = np.median(seen, axis=0) + 1e-4
    want = np.array([x ** 2.2 for x in rgb], dtype=np.float32) if img.colorspace_settings.name == "Non-Color" else np.array(rgb, dtype=np.float32)
    scaled = rgbs / typical * want
    if keep_light > 0:
        # Highlights as bright, relative to the texture's own, as they were.
        w = np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
        lum = (rgbs @ w) / max(float(typical @ w), 1e-4)
        scaled = scaled * (1 - keep_light) + np.outer(lum, want) * keep_light
    px[:, :3] = np.clip(scaled, 0.0, 1.0)
    img.pixels[:] = px.ravel()
    img.update()
    img.pack()


def ours(name):
    """A VRoid bone's group as ours for measuring: its bust bones (J_Sec_*_Bust*) are the chest's."""
    return "spine_03" if "bust" in name.lower() else name


def body_context(body, rig):
    """outfits/garments' measure of a body: its vertices, their bone weights (the bust's counted as
    the chest's: otherwise the kameez is measured without it), the rig."""
    names = {vg.index: vg.name for vg in body.vertex_groups}
    weights = []
    for v in body.data.vertices:
        d = {}
        for e in v.groups:
            if e.group in names:
                k = ours(names[e.group])
                d[k] = d.get(k, 0.0) + e.weight
        weights.append(d)
    return g.BodyContext(body, rig, weights)


def hide_covered(body, groups, below=None):
    """The body's skin faces the clothes cover all round (all their vertices mostly in `groups`, and
    below `below` (m) where given: the neckline) dropped: nothing to poke through the cloth, fewer
    triangles."""
    names = {vg.index: vg.name for vg in body.vertex_groups}
    under = [sum(e.weight for e in v.groups if ours(names.get(e.group, "")) in groups) > 0.6 and (below is None or v.co.z < below) for v in body.data.vertices]
    mats = [s.material.name.lower() if s.material else "" for s in body.material_slots]
    bm = bmesh.new()
    bm.from_mesh(body.data)
    gone = [f for f in bm.faces if "skin" in mats[f.material_index] and all(under[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bm.to_mesh(body.data)
    bm.free()
    return len(gone)


def ring(p, d, r, n=16):
    """A circle of radius `r` round `p`, across direction `d`."""
    u = d.cross(Vector((0, 0, 1)))
    u = u.normalized() if u.length > 1e-3 else Vector((1, 0, 0))
    v = d.cross(u).normalized()
    return [p + (u * math.cos(2 * math.pi * k / n) + v * math.sin(2 * math.pi * k / n)) * r for k in range(n)]


def tube(ctx, name, path, only):
    """A sleeve or a cuff: rings of (point, radius) along a path."""
    rs = []
    for k, (p, r) in enumerate(path):
        a, b = path[max(k - 1, 0)][0], path[min(k + 1, len(path) - 1)][0]
        rs.append(ring(p, (b - a).normalized(), r))
    return o.loft(ctx, name, rs, only=only)


def half(c, hw, hd, front, n=32, gap=0.22, e=2.4):
    """A front or back half of o.section, short of the sides by `gap` (radians): a panel."""
    out = []
    a0, a1 = (math.pi + gap, 2 * math.pi - gap) if front else (gap, math.pi - gap)
    for k in range(n + 1):
        a = a0 + (a1 - a0) * k / n
        ca, sa = math.cos(a), math.sin(a)
        out.append(c + Vector((hw * math.copysign(abs(ca) ** (2 / e), ca), hd * math.copysign(abs(sa) ** (2 / e), sa), 0.0)))
    return out


def panel(ctx, name, rings, thickness=0.004):
    """An open strip of half-rings (top to bottom), skinned like o.loft."""
    bm = bmesh.new()
    vs = [[bm.verts.new(p) for p in r] for r in rings]
    for a, b in zip(vs, vs[1:]):
        for k in range(len(a) - 1):
            bm.faces.new((a[k], a[k + 1], b[k + 1], b[k]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    uv = bm.loops.layers.uv.new("UVMap")
    for f in bm.faces:
        for loop in f.loops:
            loop[uv].uv = (loop.vert.co.x, loop.vert.co.z)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = g.new_object(name, me, ctx)
    sub = obj.modifiers.new("Smooth", "SUBSURF")
    sub.levels = sub.render_levels = 1
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier="Smooth")
    g.finish_cloth(obj, thickness)
    return obj


def skirt_weights(obj, ctx, top, hem):
    """A kameez's skirt panel: on the pelvis at the hip, following the thighs only partly toward the
    hem (it swings between the legs rather than tearing apart with them)."""
    g.transfer_weights(obj, ctx, only=("pelvis", "thigh_l", "thigh_r"))
    groups = {vg.name: vg for vg in obj.vertex_groups}
    for v in obj.data.vertices:
        t = min(max((top - v.co.z) / max(top - hem, 1e-3), 0.0), 1.0)
        ws = {vg.name: vg.weight(v.index) for vg in obj.vertex_groups if vg.name in ("pelvis", "thigh_l", "thigh_r") and _has(vg, v.index)}
        thighs = {k: w * 0.55 * t for k, w in ws.items() if k != "pelvis"}
        pelvis = max(1.0 - sum(thighs.values()), 0.0)
        for k, w in thighs.items():
            groups[k].add([v.index], w, "REPLACE")
        groups["pelvis"].add([v.index], pelvis, "REPLACE")


def _has(vg, i):
    try:
        vg.weight(i)
        return True
    except RuntimeError:
        return False


def salwar_suit(ctx, kameez=(0.86, 0.16, 0.42), salwar=(0.95, 0.90, 0.80), dupatta=(0.99, 0.74, 0.12), trim=(1.0, 0.76, 0.30), sole=(0.42, 0.24, 0.12)):
    """A knee-length kameez (loose, slit at the sides below the hip, three-quarter sleeves, a gold
    band at the hem and cuffs), a baggy salwar gathered at the ankle, a dupatta round the neck with
    both ends down the front, and flat chappals."""
    L = o.Landmarks(ctx)
    mk, ms, md, mt, mf = cloth("kameez", kameez), cloth("salwar", salwar), cloth("dupatta", dupatta), cloth("zari", trim, 0.5), cloth("chappal", sole)
    items = []
    torso = o.TORSO_GROUPS
    # The torso at its widest and deepest from just under the collarbones (VRoid's bust sits above
    # spine_03, where the hoodie's measure started) to the hips.
    top = L.clav - 0.03
    zs = [top - k * (top - L.hip + 0.04) / 12 for k in range(13)]
    ext = [o.extent(ctx, z, torso + ("thigh_l", "thigh_r")) for z in zs]
    hw = max(max(abs(e[0]), abs(e[1])) for e in ext) + 0.03
    front = min(e[2] for e in ext) - 0.03
    back = max(e[3] for e in ext) + 0.03
    yc, hd = (front + back) / 2, (back - front) / 2
    neck = o.extent(ctx, L.clav + 0.04, ("neck_01", "spine_03"))
    ny = (neck[2] + neck[3]) / 2
    sx = abs(L.shoulder_l.x) + 0.03
    print(f"[desi] torso: front {front:.3f} back {back:.3f} centre {yc:.3f} half-width {hw:.3f}; neck y {ny:.3f}; clav {L.clav:.3f} chest {L.chest:.3f} waist {L.waist:.3f} hip {L.hip:.3f}")
    slit = L.hip - 0.06
    hem = L.knee + 0.02
    V = Vector
    # The bodice, neck to the top of the slits: each section the body's widest and deepest in a
    # band round its height, plus ease (shaped through the bust and waist, not a sack); the front
    # never in nearer than the bust's.
    bust_front = front

    def at(z, ease, band=0.035):
        e = [o.extent(ctx, z + dz, torso + ("thigh_l", "thigh_r")) for dz in (-band, 0.0, band)]
        w = max(max(abs(x[0]), abs(x[1])) for x in e) + ease
        f = min(min(x[2] for x in e) - ease, bust_front if z > L.waist else 1.0)
        b = max(x[3] for x in e) + ease
        return o.section(V((0, (f + b) / 2, z)), w, (b - f) / 2)
    rings = [
        o.section(V((0, ny - 0.005, L.clav + 0.045)), 0.072, 0.07),
        o.section(V((0, (ny + yc) / 2, L.clav + 0.005)), sx * 0.8, hd * 0.85, e=2.2),
        at(L.clav - 0.05, 0.03),
        at(L.chest, 0.03),
        at((L.chest + L.waist) / 2, 0.03),
        at(L.waist, 0.032),
        at(L.hip, 0.035),
        at(slit, 0.04),
    ]
    # The skirt's flare starts from the hip section's size.
    last = rings[-1]
    hw = max(abs(p.x) for p in last)
    yc = sum(p.y for p in last) / len(last)
    hd = (max(p.y for p in last) - min(p.y for p in last)) / 2
    items.append(dress(o.loft(ctx, "kameez", rings, only=torso + ("upperarm_l", "upperarm_r", "thigh_l", "thigh_r")), mk))
    # Below: a front and a back panel, flaring a little to the knee, the sides open between them.
    for side, name in ((True, "front"), (False, "back")):
        tops = [(slit + 0.01, 0.0, 0.0), (slit - (slit - hem) * 0.5, 0.03, 0.02), (hem, 0.06, 0.04)]
        rs = [half(V((0, yc, z)), hw + dx, hd + dy, side) for z, dx, dy in tops]
        p = dress(panel(ctx, f"kameez_{name}", rs), mk)
        skirt_weights(p, ctx, slit, hem)
        items.append(p)
        band = [half(V((0, yc, hem + 0.035)), hw + 0.058, hd + 0.039, side), half(V((0, yc, hem - 0.002)), hw + 0.062, hd + 0.042, side)]
        b = dress(panel(ctx, f"kameez_{name}_zari", band, thickness=0.005), mt)
        skirt_weights(b, ctx, slit, hem)
        items.append(b)
    # Three-quarter sleeves, loose, a gold cuff.
    for s in ("l", "r"):
        sh, _ = ctx.bone(f"upperarm_{s}")
        el, _ = ctx.bone(f"lowerarm_{s}")
        wr, _ = ctx.bone(f"hand_{s}")
        root = sh + (sh - el).normalized() * 0.07
        end = el.lerp(wr, 0.5)
        only = (f"upperarm_{s}", f"lowerarm_{s}", f"clavicle_{s}", "spine_03")
        sleeve = dress(tube(ctx, f"kameez_sleeve_{s}", [(root, 0.045), (sh, 0.066), (sh.lerp(el, 0.5), 0.064), (el, 0.06), (end, 0.056)], only), mk)
        root_vs = [v.index for v in sleeve.data.vertices if (v.co - root).length < 0.06]
        for grp in sleeve.vertex_groups:
            grp.remove(root_vs)
        sleeve.vertex_groups[f"clavicle_{s}"].add(root_vs, 1.0, "REPLACE")
        items.append(sleeve)
        d = (wr - el).normalized()
        items.append(dress(tube(ctx, f"kameez_cuff_{s}", [(end - d * 0.03, 0.058), (end + d * 0.002, 0.059)], (f"lowerarm_{s}",)), mt))
    # The salwar: a roomy seat, and legs ballooning out under the kameez, gathered at the ankle.
    wx = o.extent(ctx, L.waist - 0.02, o.LEG_GROUPS)
    hx = o.extent(ctx, L.hip, o.LEG_GROUPS)
    cy = (hx[2] + hx[3]) / 2
    seat = [o.section(V((0, (wx[2] + wx[3]) / 2, L.waist - 0.02)), max(abs(wx[0]), wx[1]) + 0.01, (wx[3] - wx[2]) / 2 + 0.01),
            o.section(V((0, cy, L.hip)), max(abs(hx[0]), hx[1]) + 0.025, (hx[3] - hx[2]) / 2 + 0.025),
            o.section(V((0, cy, L.hip - 0.1)), max(abs(hx[0]), hx[1]) + 0.02, (hx[3] - hx[2]) / 2 + 0.02)]
    items.append(dress(o.loft(ctx, "salwar_seat", seat, only=o.LEG_GROUPS), ms))
    for s in ("l", "r"):
        th, _ = ctx.bone(f"thigh_{s}")
        kn, _ = ctx.bone(f"calf_{s}")
        an, _ = ctx.bone(f"foot_{s}")
        pts = [(th + V((0, 0, 0.02)), 0.1), (th.lerp(kn, 0.5), 0.1), (kn, 0.098), (kn.lerp(an, 0.45), 0.094), (an + V((0, 0, 0.13)), 0.08),
               (an + V((0, 0, 0.075)), 0.052), (an + V((0, 0, 0.05)), 0.048)]
        rs = [o.section(V((p.x, p.y, p.z)), r, r * 0.95, n=24, e=2.2) for p, r in pts]
        items.append(dress(o.loft(ctx, f"salwar_leg_{s}", rs, only=(f"thigh_{s}", f"calf_{s}", f"foot_{s}", "pelvis")), ms))
    # The dupatta: round the back of the neck, over both shoulders, both ends down the front to
    # below the waist, standing clear of the kameez.
    # Behind the neck it lies on the kameez's back, outside it (inside, its edges came through).
    out_y = front - 0.008
    back_y = max(p.y for p in rings[2]) + 0.012
    path = [V((0.075, out_y, L.waist - 0.14)), V((0.08, out_y, L.waist)), V((0.085, out_y, L.chest)), V((0.09, front + 0.015, L.clav - 0.03)),
            V((0.1, ny - 0.02, L.clav + 0.06)), V((0.09, (ny + back_y) / 2, L.clav + 0.06)), V((0.0, back_y, L.clav + 0.045))]
    path = path + [V((-p.x, p.y, p.z)) for p in reversed(path[:-1])]
    # Each point's outward way (from the neck, tipped up over the shoulders): the ribbon lies across
    # it. Over the shoulders and behind the neck, narrower and lying back along the shoulders' slope.
    nrm = [V((q.x, q.y - ny, 0.9 if q.z > L.clav + 0.02 else 0.0)).normalized() for q in path]
    widths = [0.085 if q.z < L.clav else 0.05 for q in path]
    items.append(dress(g.strip(ctx, "dupatta", path, widths, normals=nrm, thickness=0.003), md))
    # Flat chappals: a sole under each foot, a strap over the toes.
    for s in ("l", "r"):
        feet = [p for i, p in enumerate(ctx.co) if ctx.w(i, f"foot_{s}", f"ball_{s}") > 0.5]
        if not feet:
            continue
        xs, ys, zs_ = [p[0] for p in feet], [p[1] for p in feet], [p[2] for p in feet]
        lo = V((min(xs), min(ys), min(zs_)))
        hi = V((max(xs), max(ys), max(zs_)))
        c = V(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z + 0.006))
        only = (f"foot_{s}", f"ball_{s}")
        items.append(dress(o.rounded_box(ctx, f"chappal_{s}", c, (hi.x - lo.x + 0.02, hi.y - lo.y + 0.025, 0.014), only, bevel=0.005), mf))
        toe = V((c.x, lo.y + (hi.y - lo.y) * 0.25, lo.z + 0.028))
        items.append(dress(o.rounded_box(ctx, f"chappal_strap_{s}", toe, (hi.x - lo.x + 0.012, 0.03, 0.035), only, bevel=0.008), mf))
    return items


def trim_hair(hair_objs, ctx):
    """Hair ends tucked in under a collar: hair faces wholly below the collarbones and behind the
    neck dropped (they came through the kameez's back)."""
    L = o.Landmarks(ctx)
    neck = o.extent(ctx, L.clav + 0.04, ("neck_01", "spine_03"))
    ny = (neck[2] + neck[3]) / 2
    n = 0
    for h in hair_objs:
        mats = [s.material.name.lower() if s.material else "" for s in h.material_slots]
        bm = bmesh.new()
        bm.from_mesh(h.data)
        gone = {v for f in bm.faces if "hair" in mats[f.material_index] for v in f.verts if v.co.z < L.clav + 0.05 and v.co.y > ny - 0.02}
        bmesh.ops.delete(bm, geom=list(gone), context="VERTS")
        bm.to_mesh(h.data)
        bm.free()
        if gone:
            print(f"[desi] {h.name}: {len(gone)} hair vertices tucked under the collar")
        n += len(gone)
    return n


OUTFITS = {"salwar_suit": salwar_suit}
# What each outfit covers all round: the body's skin there is dropped (`hide_covered`).
COVERS = {"salwar_suit": ("pelvis", "spine_01", "spine_02", "spine_03", "clavicle_l", "clavicle_r", "thigh_l", "thigh_r", "calf_l", "calf_r", "upperarm_l", "upperarm_r")}
# How high up the body the covering reaches (from the collarbones' height): the neckline.
NECKLINE = {"salwar_suit": -0.035}

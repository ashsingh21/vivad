"""Outfits: each returns [(object, role)], role in {"cover", "accessory", "hair_replace"}.

Colours here are review colours; final cloth patterns (zari borders, bandhani dots, weave) are
procedural and baked in finish.py.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

import garments as g

DHOTI_WHITE = (0.86, 0.83, 0.76)
MAROON = (0.42, 0.06, 0.09)
SAFFRON = (0.93, 0.45, 0.07)
TURBAN_RED = (0.72, 0.10, 0.07)
GOLD = (1.0, 0.74, 0.32)
SILVER = (0.85, 0.86, 0.88)
SAREE_GREEN = (0.06, 0.36, 0.20)
CHOLI_MAROON = (0.48, 0.06, 0.12)

LEG_GROUPS = ("pelvis", "thigh_l", "thigh_r", "calf_l", "calf_r", "spine_01")
TORSO_GROUPS = ("pelvis", "spine_01", "spine_02", "spine_03", "clavicle_l", "clavicle_r", "neck_01")


def smoothstep(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def tag(obj, color, metal=False, kind="cloth"):
    obj["preview_color"] = color
    obj["metal"] = metal
    obj["kind"] = kind
    return obj


class Landmarks:
    def __init__(self, ctx):
        self.waist = ctx.bone("spine_01")[0].z
        self.hip = ctx.bone("thigh_l")[0].z
        self.knee = ctx.bone("calf_l")[0].z
        self.ankle = ctx.bone("foot_l")[0].z
        self.chest = ctx.bone("spine_03")[0].z
        self.clav = ctx.bone("clavicle_l")[0].z
        self.shoulder_l = ctx.bone("upperarm_l")[0]
        self.shoulder_r = ctx.bone("upperarm_r")[0]
        self.hip_l = ctx.bone("thigh_l")[0]
        self.hip_r = ctx.bone("thigh_r")[0]
        bm = bmesh.new()
        bm.from_mesh(ctx.proxy.data)
        self.bvh = BVHTree.FromBMesh(bm)
        bm.free()

    def surface(self, x, z, front=True, push=0.0):
        """Body surface point at (x, z) seen from the front (-Y) or back (+Y)."""
        origin = Vector((x, -1.0 if front else 1.0, z))
        hit = self.bvh.ray_cast(origin, Vector((0, 1 if front else -1, 0)), 2.0)
        if hit[0] is None:
            return Vector((x, 0.0, z))
        return hit[0] + hit[1] * push


def all_in(ctx, vids, groups, lo, hi, min_w=0.5):
    for i in vids:
        z = ctx.co[i][2]
        if not (lo <= z <= hi) or ctx.w(i, *groups) < min_w:
            return False
    return True


def diagonal_sash(ctx, L, name, *, from_left_shoulder=True, half_width=0.065, offset=0.014, z_min=None, smooth=3):
    """Band of torso faces around the plane through one shoulder and the opposite hip."""
    a = (L.shoulder_l if from_left_shoulder else L.shoulder_r) + Vector((0, 0, 0.05))
    b = (L.hip_r if from_left_shoulder else L.hip_l) + Vector((0, 0, 0.02))
    nrm = (b - a).cross(Vector((0, 1, 0))).normalized()
    z_min = L.hip - 0.02 if z_min is None else z_min
    groups = TORSO_GROUPS + ("upperarm_l", "upperarm_r", "thigh_l", "thigh_r")

    def pred(vids):
        for i in vids:
            p = Vector(ctx.co[i])
            if abs((p - a).dot(nrm)) > half_width or p.z < z_min or ctx.w(i, *groups) < 0.5:
                return False
            if ctx.w(i, "upperarm_l", "upperarm_r") > 0.5 and p.z < L.shoulder_l.z - 0.06:
                return False
        return True

    return g.shell(ctx, name, pred, lambda co, n: offset, smooth=smooth, thickness=0.004, edge_iters=16)


def jewellery(ctx, items, *, armbands=True, kadas=1, anklets=False, neck=False, metal=GOLD, metal_kind="gold"):
    for side in ("l", "r"):
        if armbands:
            items.append((tag(g.ring(ctx, f"bajuband_{side}", f"upperarm_{side}", 0.38, width=0.032, thick=0.007, beads=6), metal, True, "gold"), "accessory"))
        for k in range(kadas):
            s = 0.83 + k * 0.045
            gold = k % 2 == 0
            items.append((tag(g.ring(ctx, f"kada_{side}{k}", f"lowerarm_{side}", s, width=0.016 if kadas > 1 else 0.022, thick=0.007, offset=0.008),
                              metal if gold else (0.62, 0.08, 0.10), gold, metal_kind if gold else "lacquer"), "accessory"))
        if anklets:
            items.append((tag(g.ring(ctx, f"payal_{side}", f"calf_{side}", 0.93, width=0.014, thick=0.006, offset=0.006, beads=14), SILVER, True, "silver"), "accessory"))
    if neck:
        items.append((tag(g.ring(ctx, "hansuli", "neck_01", 0.12, width=0.014, thick=0.011, offset=0.012, min_weight=0.3), metal, True, "gold"), "accessory"))


# ------------------------------------------------------------------ Panga: today's street clothes

SHIRT_MUSTARD = (0.93, 0.64, 0.10)
# Mid-blue, washed: reads as jeans against any top.
DENIM = (0.20, 0.34, 0.58)
KAMEEZ_PINK = (0.90, 0.12, 0.46)
SALWAR_CREAM = (0.92, 0.88, 0.78)
DUPATTA_YELLOW = (0.99, 0.76, 0.10)
STEEL = (0.72, 0.73, 0.75)


def upper_body(ctx, L, hem, sleeve, neck_front, neck_back, lower_sleeve=0.0):
    """A top's face predicate: torso (and hips down to `hem`), sleeves to `sleeve` of the upper
    arm (or into the forearm to `lower_sleeve`), and a neckline `neck_front` / `neck_back` below
    the collarbone."""
    def pred(vids):
        for i in vids:
            x, y, z = ctx.co[i]
            arm = ctx.w(i, "upperarm_l", "upperarm_r")
            fore = ctx.w(i, "lowerarm_l", "lowerarm_r")
            side = "l" if x > 0 else "r"
            if fore > 0.5:
                s, _ = g.bone_param(ctx, f"lowerarm_{side}", Vector(ctx.co[i]))
                if s > lower_sleeve:
                    return False
                continue
            if arm > 0.5:
                s, _ = g.bone_param(ctx, f"upperarm_{side}", Vector(ctx.co[i]))
                if s > sleeve:
                    return False
                continue
            if ctx.w(i, *(TORSO_GROUPS + ("thigh_l", "thigh_r"))) < 0.5 or z < hem:
                return False
            if z > L.clav - (neck_front if y < 0 else neck_back) + 0.03 * min(abs(x) / 0.1, 1.0) ** 2:
                return False
        return True
    return pred


def male_street(ctx):
    """Over the modelled shirt and jeans (tools/blender/characters/human.py "clothes"): a steel kada;
    his own hair."""
    items = []
    jewellery(ctx, items, armbands=False, kadas=1, neck=False, metal=STEEL, metal_kind="silver")
    return items


def female_street(ctx):
    """Over the modelled tee and jeans: glass bangles."""
    items = []
    jewellery(ctx, items, armbands=False, kadas=4, anklets=False, neck=False)
    return items


# How far out the carrier reaches above the chest, of the way to the shoulder joint.
ARMHOLE = 0.72


def tactical_kit(ctx, items, armholes=False):
    """A plate carrier over the modelled top (front and back plates on shoulder straps, wrapping
    the ribs), three magazine pouches across its front, a rigger's belt, knee pads. `armholes`: cut
    in above the chest (the male, whose shirt moves with his arms there: motus.shoulder_heat)."""
    L = Landmarks(ctx)
    top = L.clav - 0.075

    def carrier(vids):
        for i in vids:
            x, y, z = ctx.co[i]
            if ctx.w(i, "upperarm_l", "upperarm_r", "lowerarm_l", "lowerarm_r", "hand_l", "hand_r") > 0.3:
                return False
            if ctx.w(i, *TORSO_GROUPS) < 0.5 or z < L.waist + 0.01:
                return False
            # The plates stop below the collarbone (the straps, below, go over the shoulders: grown
            # from the body there they make slivers Solidify spikes out of).
            if z > top:
                return False
            # Cut in at the armholes (a carrier's plates sit on the chest and back): out toward the
            # shoulder the shirt moves with the arm, and a carrier riding it there bent and peeled
            # as he raised his arms.
            if armholes and abs(x) > ARMHOLE * abs(L.shoulder_l.x) and z > L.chest:
                return False
        return True

    # Further out behind, over the modelled shirt's looser back (easing round the ribs), most at the
    # small of the back, where the shirt bunches as he leans into a run.
    vest = g.shell(ctx, "vest", carrier, lambda co, n: 0.032 + (0.028 + 0.026 * smoothstep(L.waist + 0.16, L.waist + 0.01, co.z)) * smoothstep(-0.2, 0.6, n.y), smooth=8,
                   thickness=0.012, snap=g.z_snap(L.waist + 0.01, top))
    items.append((tag(vest, (0.42, 0.35, 0.24)), "cover"))
    # Shoulder straps: from the front plate's top up over the shoulder to the back plate's, each
    # point's outward normal turning from the front through up to the back.
    crest = L.clav + 0.075
    for side, x in (("l", 0.09), ("r", -0.09)):
        f, b = L.surface(x, top, front=True, push=0.05), L.surface(x, top, front=False, push=0.075)
        path, normals = [], []
        for i in range(9):
            t = i / 8
            a = math.pi * t
            p = f.lerp(b, t)
            p.z = top + (crest - top) * math.sin(a)
            path.append(p)
            normals.append(Vector((0.0, -math.cos(a), math.sin(a))))
        items.append((tag(g.strip(ctx, f"vest_strap_{side}", path, [0.05] * 9, normals=normals, thickness=0.008), (0.42, 0.35, 0.24)), "accessory"))
    # Pouches on the front plate's lower half: short, deep boxes standing off it.
    for k, x in enumerate((-0.075, 0.0, 0.075)):
        # Each point on the plate's face (the body's surface out by the vest's offset and
        # thickness), the pouch standing off it by its own depth.
        path = [L.surface(x, L.waist + 0.17 - 0.10 * t / 4, push=0.044 + 0.015) for t in range(5)]
        items.append((tag(g.strip(ctx, f"pouch{k}", path, [0.062] * 5, thickness=0.03), (0.38, 0.32, 0.22)), "accessory"))
    belt = g.shell(ctx, "tac_belt", lambda v: all_in(ctx, v, LEG_GROUPS + ("spine_02",), L.waist - 0.07, L.waist - 0.025),
                   lambda co, n: 0.022, smooth=4, thickness=0.008, snap=g.z_snap(L.waist - 0.07, L.waist - 0.025))
    items.append((tag(belt, (0.07, 0.07, 0.07)), "cover"))
    for side in "lr":
        items.append((tag(g.band(ctx, f"kneepad_{side}", f"calf_{side}", 0.0, 0.12, 0.024, thickness=0.012), (0.10, 0.10, 0.10)), "accessory"))
    return items


def male_tactical(ctx):
    """Over the modelled shirt and trousers (retinted charcoal and olive: tools/textures/
    mh_clothes.py) and boots: the tactical kit, a steel kada."""
    items = tactical_kit(ctx, [], armholes=True)
    jewellery(ctx, items, armbands=False, kadas=1, neck=False, metal=STEEL, metal_kind="silver")
    return items


def female_tactical(ctx):
    """Over the modelled tee and trousers (retinted olive and charcoal) and boots: the tactical
    kit, two glass bangles."""
    items = tactical_kit(ctx, [])
    jewellery(ctx, items, armbands=False, kadas=2, anklets=False, neck=False)
    return items


def section(c, hw, hd, n=32, e=2.6):
    """A rounded-rectangle ring (a superellipse, exponent `e`) round `c` in the XY plane:
    `hw` across, `hd` deep (half sizes)."""
    out = []
    for k in range(n):
        a = 2 * math.pi * k / n
        ca, sa = math.cos(a), math.sin(a)
        out.append(c + Vector((hw * math.copysign(abs(ca) ** (2 / e), ca), hd * math.copysign(abs(sa) ** (2 / e), sa), 0.0)))
    return out


def loft(ctx, name, rings, *, only=None, cap_bottom=False, thickness=0.004):
    """A garment lofted through `rings` (lists of points, all the same length, top to bottom):
    its own shape, not the body's, skinned from the nearest body vertices (`only` those groups)."""
    bm = bmesh.new()
    vs = [[bm.verts.new(p) for p in r] for r in rings]
    n = len(rings[0])
    for a, b in zip(vs, vs[1:]):
        for k in range(n):
            bm.faces.new((a[k], a[(k + 1) % n], b[(k + 1) % n], b[k]))
    if cap_bottom:
        bm.faces.new(list(reversed(vs[-1])))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    uv = bm.loops.layers.uv.new("UVMap")
    for f in bm.faces:
        for loop in f.loops:
            loop[uv].uv = (loop.vert.co.x, loop.vert.co.z)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = g.new_object(name, me, ctx)
    # Smoothed now (one subdivision, applied): the cel band's edge follows the surface's normals,
    # and over a coarse loft it zigzagged across the facets in blotches. Applied here, so the
    # hems' trim (finish.py, from the distance to the open edges) is measured on the smooth mesh.
    sub = obj.modifiers.new("Smooth", "SUBSURF")
    sub.levels = sub.render_levels = 1
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_move_to_index(modifier="Smooth", index=0)
    bpy.ops.object.modifier_apply(modifier="Smooth")
    g.transfer_weights(obj, ctx, only=only)
    g.finish_cloth(obj, thickness)
    return obj


def rounded_box(ctx, name, c, size, only, bevel=0.012):
    """A box with its edges rounded (`bevel` m, three segments): a pocket or a flap."""
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation(c) @ Matrix.Diagonal((*size, 1.0)))
    bmesh.ops.bevel(bm, geom=list(bm.edges), offset=min(bevel, min(size) * 0.45), segments=3, affect="EDGES", profile=0.5)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    o = g.new_object(name, me, ctx)
    g.transfer_weights(o, ctx, only=only)
    return o


def extent(ctx, z, groups, dz=0.02):
    """The body's (x min, x max, y min, y max) in a slab round height `z`, over `groups`."""
    pts = [p for i, p in enumerate(ctx.co) if abs(p[2] - z) < dz and ctx.w(i, *groups) > 0.5]
    if not pts:
        return -0.1, 0.1, -0.1, 0.1
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    return min(xs), max(xs), min(ys), max(ys)


def anime_street(ctx):
    """The anime character's: a loose hoodie over loose cargo trousers, modelled as garments in
    their own right (lofted sections sized off the body's widest and deepest points and held
    there, straight), so they never trace the figure."""
    L = Landmarks(ctx)
    items = []
    torso = TORSO_GROUPS
    # The torso at its widest and deepest anywhere from the chest to the hips: the hoodie's barrel.
    zs = [L.chest + 0.05 - k * (L.chest + 0.05 - L.hip + 0.04) / 8 for k in range(9)]
    ext = [extent(ctx, z, torso + ("thigh_l", "thigh_r")) for z in zs]
    hw = max(max(abs(e[0]), abs(e[1])) for e in ext) + 0.04
    front = min(e[2] for e in ext) - 0.035
    back = max(e[3] for e in ext) + 0.035
    yc, hd = (front + back) / 2, (back - front) / 2
    neck = extent(ctx, L.clav + 0.04, ("neck_01", "spine_03"))
    ny = (neck[2] + neck[3]) / 2
    sx = abs(L.shoulder_l.x) + 0.05
    hem = L.hip - 0.07
    rings = [
        section(Vector((0, ny, L.clav + 0.05)), 0.078, 0.075),
        section(Vector((0, (ny + yc) / 2, L.clav + 0.005)), sx * 0.82, hd * 0.85, e=2.2),
        section(Vector((0, yc, L.clav - 0.05)), sx, hd),
        section(Vector((0, yc, L.chest)), hw, hd),
        section(Vector((0, yc, (L.chest + L.waist) / 2)), hw, hd),
        section(Vector((0, yc, L.waist)), hw, hd),
        section(Vector((0, yc, hem + 0.06)), hw, hd),
        # The ribbed hem: gathered in a little.
        section(Vector((0, yc, hem + 0.05)), hw - 0.012, hd - 0.012),
        section(Vector((0, yc, hem)), hw - 0.014, hd - 0.014),
    ]
    items.append((loft(ctx, "anime_hoodie", rings, only=torso + ("upperarm_l", "upperarm_r", "thigh_l", "thigh_r")), "cover"))
    # Sleeves: roomy tubes down the arms to ribbed cuffs at the wrist.
    for side in ("l", "r"):
        sh, _ = ctx.bone(f"upperarm_{side}")
        el, _ = ctx.bone(f"lowerarm_{side}")
        wr, _ = ctx.bone(f"hand_{side}")
        # Rooted inside the body, narrow, the root riding the collarbone (below), so the sleeve
        # leaves the hoodie's shoulder cleanly and covers the shoulder's top.
        root = sh + (sh - el).normalized() * 0.07

        def ring_at(p, d, r):
            # A circle round `p` across direction `d`.
            u = d.cross(Vector((0, 0, 1)))
            u = u.normalized() if u.length > 1e-3 else Vector((1, 0, 0))
            v = d.cross(u).normalized()
            return [p + (u * math.cos(2 * math.pi * k / 16) + v * math.sin(2 * math.pi * k / 16)) * r for k in range(16)]
        path = [(root, 0.045), (sh, 0.07), (sh.lerp(el, 0.5), 0.07), (el, 0.064), (el.lerp(wr, 0.6), 0.058), (wr - (wr - el).normalized() * 0.05, 0.054),
                (wr - (wr - el).normalized() * 0.045, 0.043), (wr - (wr - el).normalized() * 0.005, 0.042)]
        rs = []
        for k, (p, r) in enumerate(path):
            a = path[max(k - 1, 0)][0]
            b = path[min(k + 1, len(path) - 1)][0]
            rs.append(ring_at(p, (b - a).normalized(), r))
        sleeve = loft(ctx, f"anime_hoodie_sleeve_{side}", rs, only=(f"upperarm_{side}", f"lowerarm_{side}", f"hand_{side}", f"clavicle_{side}", "spine_03"))
        # The buried root rides the collarbone (with the torso), not the arm, which would swing it
        # out through the hoodie's back.
        root_vs = [v.index for v in sleeve.data.vertices if (v.co - root).length < 0.06]
        for grp in sleeve.vertex_groups:
            grp.remove(root_vs)
        sleeve.vertex_groups[f"clavicle_{side}"].add(root_vs, 1.0, "REPLACE")
        items.append((sleeve, "cover"))
    # The hood, down: a soft roll round the back of the neck, standing proud of the back (its ends
    # sunk into the shoulders), not lying flat against it.
    rings = []
    for k in range(9):
        a = math.pi * (0.1 + 0.8 * k / 8)
        c = Vector((0.11 * math.cos(a), ny + 0.075 * math.sin(a), L.clav + 0.035 + 0.025 * math.sin(a)))
        r = 0.035 + 0.02 * math.sin(a)
        out = Vector((math.cos(a), math.sin(a), 0.0))
        rings.append([c + (out * math.cos(2 * math.pi * q / 12) + Vector((0, 0, 1)) * math.sin(2 * math.pi * q / 12)) * r for q in range(12)])
    items.append((loft(ctx, "anime_hoodie_hood", rings, only=("spine_03", "neck_01", "clavicle_l", "clavicle_r"), thickness=0.0), "accessory"))
    # The drawstrings. (No front pocket: a box on the belly stood off it like a plate when she bent.)
    for side in (1.0, -1.0):
        top = Vector((side * 0.035, neck[2] - 0.02, L.clav + 0.02))
        path = [top + Vector((0, -0.01 * t, -0.16 * t)) for t in (0.0, 0.5, 1.0)]
        items.append((g.strip(ctx, f"anime_hoodie_string{int(side)}", path, [0.012] * 3, thickness=0.004), "accessory"))
    # Cargo trousers: a waist and seat section, then each leg a straight loose tube down its bones
    # to a gathered ankle; a pocket on each thigh.
    wx = extent(ctx, L.waist - 0.02, LEG_GROUPS)
    hx = extent(ctx, L.hip, LEG_GROUPS)
    seat = [section(Vector((0, (wx[2] + wx[3]) / 2, L.waist - 0.02)), max(abs(wx[0]), wx[1]) + 0.015, (wx[3] - wx[2]) / 2 + 0.015),
            section(Vector((0, (hx[2] + hx[3]) / 2, L.hip)), max(abs(hx[0]), hx[1]) + 0.03, (hx[3] - hx[2]) / 2 + 0.03),
            section(Vector((0, (hx[2] + hx[3]) / 2, L.hip - 0.09)), max(abs(hx[0]), hx[1]) + 0.025, (hx[3] - hx[2]) / 2 + 0.025)]
    items.append((loft(ctx, "anime_cargo_seat", seat, only=LEG_GROUPS), "cover"))
    for side in ("l", "r"):
        th, _ = ctx.bone(f"thigh_{side}")
        kn, _ = ctx.bone(f"calf_{side}")
        an, _ = ctx.bone(f"foot_{side}")
        # From the hip joint (inside the seat, no gap between them) down.
        pts = [(th + Vector((0, 0, 0.02)), 0.1), (th.lerp(kn, 0.35), 0.098), (th.lerp(kn, 0.7), 0.088), (kn, 0.082), (kn.lerp(an, 0.5), 0.076),
               (an + Vector((0, 0, 0.09)), 0.07), (an + Vector((0, 0, 0.05)), 0.05), (an + Vector((0, 0, 0.03)), 0.05)]
        rs = []
        for k, (p, r) in enumerate(pts):
            # Straight down the leg (not round its curves): each section level, centred on the bone.
            rs.append(section(Vector((p.x, p.y, p.z)), r, r * 0.95, n=24, e=2.2))
        items.append((loft(ctx, f"anime_cargo_leg_{side}", rs, only=(f"thigh_{side}", f"calf_{side}", f"foot_{side}", "pelvis")), "cover"))
        # The thigh pocket, on the outside, set into the leg, rounded, under a flap a little wider.
        sgn = 1.0 if side == "l" else -1.0
        c = th.lerp(kn, 0.5) + Vector((sgn * 0.088, 0, 0))
        leg = (f"thigh_{side}", f"calf_{side}")
        items.append((rounded_box(ctx, f"anime_cargo_pocket_{side}", c, (0.028, 0.115, 0.13), leg), "accessory"))
        items.append((rounded_box(ctx, f"anime_cargo_flap_{side}", c + Vector((sgn * 0.005, 0, 0.07)), (0.03, 0.125, 0.035), leg, bevel=0.01), "accessory"))
    return items


# ------------------------------------------------------------------ Vivad: the ancient warriors

def male_warrior(ctx):
    L = Landmarks(ctx)
    items = []
    waistline = L.waist - 0.035
    hem = L.knee - 0.14

    # Dhoti: fitted at the waist, billowing through the thighs, gathered at mid-calf.
    def dhoti_offset(co, n):
        t = (waistline - co.z) / (waistline - hem)
        loose = 0.009 + 0.05 * math.sin(math.pi * min(max(t, 0.0) * 0.85, 1.0))
        inner = 1.0 - 0.6 * max(0.0, -n.x * math.copysign(1.0, co.x)) * smoothstep(L.hip, L.knee, co.z)
        return loose * inner

    dhoti = g.shell(ctx, "dhoti", lambda v: all_in(ctx, v, LEG_GROUPS, hem, waistline), dhoti_offset, smooth=6,
                    snap=g.z_snap(hem, waistline))
    items.append((tag(dhoti, DHOTI_WHITE), "cover"))
    # The pleated front fold (kachha) hanging from the tuck to just above the knees.
    top = L.surface(0.0, waistline - 0.03, push=0.035)
    fold = [top + Vector((0.0, -0.045 * s * s, -(waistline - L.knee - 0.02) * s)) for s in [i / 10 for i in range(11)]]
    items.append((tag(g.strip(ctx, "dhoti_fold", fold, [0.10 + 0.08 * i / 10 for i in range(11)], thickness=0.004), DHOTI_WHITE), "accessory"))

    # Kamarband: a sash wound around the waist over the dhoti, tails hanging at the front left.
    kb = g.shell(ctx, "kamarband", lambda v: all_in(ctx, v, LEG_GROUPS + ("spine_02",), waistline - 0.075, waistline + 0.012),
                 lambda co, n: 0.024, smooth=4, thickness=0.006, snap=g.z_snap(waistline - 0.075, waistline + 0.012))
    items.append((tag(kb, MAROON), "cover"))
    # Two knotted tails falling free at the front left.
    for k, (dx, length) in enumerate(((0.075, 0.34), (0.115, 0.27))):
        tail_top = L.surface(dx, waistline - 0.05, push=0.05)
        tail = [tail_top + Vector((0.012 * s, -0.025 * s * s, -length * s)) for s in [i / 9 for i in range(10)]]
        items.append((tag(g.strip(ctx, f"kamarband_tail{k}", tail, [0.07 + 0.025 * i / 9 for i in range(10)], thickness=0.004), MAROON), "accessory"))

    # Angavastram over the left shoulder, across the chest to the right hip.
    items.append((tag(diagonal_sash(ctx, L, "angavastram", half_width=0.06, offset=0.012, z_min=waistline - 0.02), SAFFRON), "accessory"))

    jewellery(ctx, items, armbands=True, kadas=1, neck=True)

    tb, _ = g.turban(ctx, "pagdi", brow_top=ctx.brow_top)
    items.append((tag(tb, TURBAN_RED, kind="turban"), "hair_replace"))
    return items


def female_warrior(ctx):
    L = Landmarks(ctx)
    items = []
    waistline = L.waist - 0.02
    hem = L.ankle + 0.07

    # Nauvari (nine-yard) saree worn dhoti-style: free to fight in, ankle length, fuller at the knee.
    def nauvari_offset(co, n):
        t = (waistline - co.z) / (waistline - hem)
        loose = 0.008 + 0.034 * math.sin(math.pi * min(max(t, 0.0) * 1.0, 1.0)) + 0.012 * t
        inner = 1.0 - 0.6 * max(0.0, -n.x * math.copysign(1.0, co.x)) * smoothstep(L.hip, L.knee, co.z)
        return loose * inner

    saree = g.shell(ctx, "nauvari", lambda v: all_in(ctx, v, LEG_GROUPS, hem, waistline), nauvari_offset, smooth=6,
                    snap=g.z_snap(hem, waistline))
    items.append((tag(saree, SAREE_GREEN), "cover"))
    # Front pleats (nirya) falling from the waist.
    top = L.surface(0.0, waistline - 0.02, push=0.03)
    pleat = [top + Vector((0.0, -0.05 * s * s, -(waistline - L.knee + 0.06) * s)) for s in [i / 10 for i in range(11)]]
    items.append((tag(g.strip(ctx, "nirya", pleat, [0.13 + 0.07 * i / 10 for i in range(11)], thickness=0.004), SAREE_GREEN), "accessory"))

    # Choli: fitted blouse with short sleeves and a scooped neckline.
    under_bust = L.chest + 0.005

    def choli_pred(vids):
        for i in vids:
            x, y, z = ctx.co[i]
            arm = ctx.w(i, "upperarm_l", "upperarm_r")
            if arm > 0.5:
                side = "l" if x > 0 else "r"
                s, _ = g.bone_param(ctx, f"upperarm_{side}", Vector(ctx.co[i]))
                if s > 0.36:
                    return False
                continue
            if ctx.w(i, *TORSO_GROUPS) < 0.5 or z < under_bust:
                return False
            neck_cut = L.clav - (0.075 if y < 0 else 0.035) + 0.04 * min(abs(x) / 0.12, 1.0) ** 2
            if z > neck_cut:
                return False
        return True

    choli = g.shell(ctx, "choli", choli_pred, lambda co, n: 0.007, smooth=2, thickness=0.003, edge_iters=20)
    items.append((tag(choli, CHOLI_MAROON), "cover"))

    # Pallu across the body from the right waist over the left shoulder, then falling down the back.
    items.append((tag(diagonal_sash(ctx, L, "pallu", half_width=0.085, offset=0.014, z_min=waistline - 0.03, smooth=9), SAREE_GREEN), "cover"))
    # The end of the pallu falls from the left shoulder down the back, draped against the body.
    back_top = L.surface(0.10, L.clav - 0.02, front=False, push=0.02)
    fall = [back_top + Vector((0.02 * s, 0.0, -(L.clav - L.knee) * 0.72 * s)) for s in [i / 14 for i in range(15)]]
    items.append((tag(g.strip(ctx, "pallu_fall", fall, [0.17 + 0.08 * i / 14 for i in range(15)], normals=[Vector((0, 1, 0))] * 15,
                              thickness=0.004, shrink_to=ctx.proxy, shrink_offset=0.03), SAREE_GREEN), "accessory"))

    # Gold kamarbandh (waist chain/belt).
    kb = g.shell(ctx, "kamarbandh", lambda v: all_in(ctx, v, LEG_GROUPS + ("spine_02",), waistline - 0.03, waistline + 0.008),
                 lambda co, n: 0.02, smooth=3, thickness=0.004, snap=g.z_snap(waistline - 0.03, waistline + 0.008))
    items.append((tag(kb, GOLD, True, "gold"), "accessory"))
    jewellery(ctx, items, armbands=True, kadas=3, anklets=True, neck=True)
    return items

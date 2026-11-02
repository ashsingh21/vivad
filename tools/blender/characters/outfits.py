"""Outfits: each returns [(object, role)], role in {"cover", "accessory", "hair_replace"}.

Panga's are MakeHuman community clothes fitted to the body (`worn`), chosen from what Delhi's streets
wear (research/characters/README.md). The Vivad warriors' are grown from the body: colours there are
review colours, and their cloth patterns (zari borders, bandhani dots, weave) are procedural and
baked in finish.py.
"""

import math
import os

import bmesh
import bpy
from mathutils import Vector
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


# ------------------------------------------------------------------ Panga: real Delhi street clothes

# MakeHuman community packs, fetched by research/characters/scripts/fetch_mh.sh; credits in
# docs/references.md.
PACKS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "research", "characters", "downloads", "makehuman")

# Strong plain colours: they read at distance and don't shimmer.
POLO_MAROON = (0.52, 0.09, 0.14)
JEANS_INDIGO = (0.16, 0.23, 0.38)
SHOE_WHITE = (0.80, 0.79, 0.76)
BAG_BLACK = (0.10, 0.11, 0.13)
KURTA_MAGENTA = (0.60, 0.06, 0.30)
SALWAR_CREAM = (0.90, 0.86, 0.76)
FLATS_BROWN = (0.34, 0.19, 0.12)


def worn(ctx, pack, name, *, tint=None, detail=1.0, rough=0.8, normal=None):
    """A MakeHuman community garment on this body. MPFB fits it from its .mhclo, which ties each
    vertex to three body vertices, and copies the body's bone weights through the same tie, so it
    follows any shape and every clip. See finish.fitted_material for `tint`, `detail` and `rough`;
    `normal` names a normal map beside the garment that its .mhmat leaves out."""
    from bl_ext.blender_org.mpfb.services.humanservice import HumanService
    path = os.path.join(PACKS, pack, "x", "clothes", name, f"{name}.mhclo")
    obj = HumanService.add_mhclo_asset(path, ctx.body, asset_type="Clothes", subdiv_levels=0, material_type="MAKESKIN")
    for m in list(obj.modifiers):
        if m.type != "ARMATURE":
            obj.modifiers.remove(m)
    tag(obj, tint or (0.8, 0.8, 0.8), kind="fitted")
    obj["detail"], obj["rough"] = detail, rough
    if tint:
        obj["tint"] = tint
    if normal:
        obj["normal_map"] = bpy.data.images.load(os.path.join(os.path.dirname(path), normal), check_existing=True).name
    return obj


def tuck(obj, L, lo, hi, gap=0.006):
    """Pull cloth in against the body from height `lo` up (all the way by `hi`), so a layer worn over
    it doesn't get poked through: two garments fitted to one body each think they sit on the skin."""
    for v in obj.data.vertices:
        t = smoothstep(lo, hi, v.co.z)
        if t > 0.0:
            hit, normal = L.bvh.find_nearest(v.co)[:2]
            v.co = v.co.lerp(hit + normal * gap, t)


def hem_of(obj):
    return min(v.co.z for v in obj.data.vertices)


def flare(obj, L, lo, hi, out):
    """Let cloth hang off the body: pushed out from its middle by `out` at height `lo`, by nothing
    from `hi` up. Straight out, not along the normals, or a folded hem's two layers part."""
    for v in obj.data.vertices:
        away = Vector((v.co.x, v.co.y - L.hip_l.y, 0.0)).normalized()
        v.co += away * out * smoothstep(hi, lo, v.co.z)


def drop_fold(obj, L, top):
    """A folded hem's inner layer pokes through the outer one once fitted: take it off, up to `top`."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    inner = []
    for f in bm.faces:
        c = f.calc_center_median()
        if c.z < top and f.normal.dot(Vector((c.x, c.y - L.hip_l.y, 0.0))) < 0.0:
            inner.append(f)
    bmesh.ops.delete(bm, geom=inner, context="FACES")
    bm.to_mesh(obj.data)
    bm.free()


def cut_above(obj, z):
    """Take off the part of a garment that a layer over it hides for good."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if min(v.co.z for v in f.verts) > z], context="FACES")
    bm.to_mesh(obj.data)
    bm.free()


def block(ctx, name, centre, size, bone):
    """A soft-cornered box of `size` (x, y, z), rigid on `bone`."""
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=2.0)
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=3, use_grid_fill=True)
    soft = 0.4
    for v in bm.verts:
        # Part of the way from a cube to a sphere.
        x, y, z = (c * c for c in v.co)
        rounded = (math.sqrt(1 - soft * (b + c) / 2 + soft * soft * b * c / 3) * a for a, b, c in ((v.co.x, y, z), (v.co.y, x, z), (v.co.z, x, y)))
        v.co = Vector(s * r / 2 for s, r in zip(size, rounded)) + centre
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = g.new_object(name, me, ctx)
    g.set_weight(obj, bone)
    g.finish_cloth(obj, 0)
    return obj


def backpack(ctx, L, shirt):
    """A school bag on the back, the thing the player looks at all game: rigid on the upper spine,
    with straps over `shirt` that follow the body."""
    back = L.surface(0.1, L.chest, front=False).y
    front = L.surface(0.12, L.chest).y
    top, bottom = L.clav - 0.02, L.waist - 0.01
    mid = Vector((0.0, back + 0.085, (top + bottom) / 2))
    size = (0.30, 0.14, top - bottom)
    items = [(tag(block(ctx, "backpack", mid, size, "spine_03"), BAG_BLACK), "accessory")]
    pocket = mid + Vector((0.0, 0.075, -0.075))
    items.append((tag(block(ctx, "backpack_pocket", pocket, (0.23, 0.07, 0.17), "spine_03"), BAG_BLACK), "accessory"))
    for side in (1.0, -1.0):
        # Up from the bag, over the shoulder, down the chest and back under the arm.
        marks = ((0.08, back + 0.03, top - 0.03), (0.10, back - 0.01, L.clav + 0.03), (0.115, back + 0.5 * (front - back), L.clav + 0.06), (0.125, front + 0.01, L.clav + 0.02),
                 (0.14, front, L.clav - 0.07), (0.16, front, L.clav - 0.17), (0.20, back + 0.4 * (front - back), L.clav - 0.24), (0.17, back + 0.02, bottom + 0.09))
        path, normals = [], []
        centre = Vector((0.1, (front + back) / 2, L.chest))
        for a, b in zip(marks, marks[1:]):
            for k in range(4):
                p = Vector(a).lerp(Vector(b), k / 4)
                path.append(Vector((side * p.x, p.y, p.z)))
                normals.append(Vector((side * (p.x - centre.x), p.y - centre.y, p.z - centre.z)).normalized())
        strap = g.strip(ctx, f"backpack_strap_{'l' if side > 0 else 'r'}", path, [0.05] * len(path), normals=normals, thickness=0.005, shrink_to=shirt, shrink_offset=0.008)
        items.append((tag(strap, BAG_BLACK), "accessory"))
    return items


def male_street(ctx):
    """The young man the street is full of: plain polo, jeans, running shoes, a backpack."""
    L = Landmarks(ctx)
    polo = worn(ctx, "shirts01", "namuhekam_male_polo_shirt", tint=POLO_MAROON, normal="Polo_Normal_OpenGL.png")
    jeans = worn(ctx, "pants02", "punkduck_male_classic_jeans", tint=JEANS_INDIGO)
    tuck(jeans, L, hem_of(polo) - 0.06, hem_of(polo) - 0.01)
    cut_above(jeans, hem_of(polo) + 0.015)
    drop_fold(polo, L, hem_of(polo) + 0.07)
    flare(polo, L, hem_of(polo), L.waist + 0.04, 0.014)
    items = [(polo, "cover"), (jeans, "cover"),
             (worn(ctx, "shoes02", "punkduck_running_shoes_01", tint=SHOE_WHITE, detail=0.7), "cover")]
    return items + backpack(ctx, L, polo)


def as_kurta(obj, L):
    """The flapper dress is nearly a kurta: elbow sleeves, a straight fall. Take off its side sashes
    (the strip of its texture left of u=0.25), cut the hem at the knee and open the sides up to the
    hips, front and back panels parted a little, so they swing apart over a running leg."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    uv = bm.loops.layers.uv.active
    sash = [f for f in bm.faces if sum(c[uv].uv.x for c in f.loops) / len(f.loops) < 0.25]
    bmesh.ops.delete(bm, geom=sash, context="FACES")
    knee, top, seam = L.knee + 0.03, L.hip - 0.04, L.hip_l.y
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=(0, 0, knee), plane_no=(0, 0, 1), clear_inner=True)
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=(0, seam, 0), plane_no=(0, 1, 0))
    slit = [e for e in bm.edges if all(abs(v.co.y - seam) < 1e-4 and v.co.z < top for v in e.verts)]
    bmesh.ops.split_edges(bm, edges=slit)
    for v in bm.verts:
        if v.co.z < top:
            side = sum(f.calc_center_median().y - seam for f in v.link_faces)
            v.co.y += math.copysign(0.025, side) * smoothstep(top, knee, v.co.z) * smoothstep(0.08, 0.0, abs(v.co.y - seam))
    bm.to_mesh(obj.data)
    bm.free()
    return obj


def neat_plait(ctx, hair, L):
    """MakeHuman's braid has a thick side fringe over one eye. Bring its cap in toward the skull and
    comb what falls past the brow back up to a soft hairline, lower at the temples; the plait down
    the back stays."""
    skull = ctx.bone("head")[0]
    for v in hair.data.vertices:
        t = smoothstep(skull.z - 0.08, skull.z, v.co.z) * 0.4
        if t > 0.0:
            v.co = v.co.lerp(L.bvh.find_nearest(v.co)[0], t)
        line = ctx.brow_top + 0.02 - 1.1 * max(0.0, abs(v.co.x) - 0.04)
        v.co.z += max(0.0, line - v.co.z) * smoothstep(skull.y + 0.01, skull.y - 0.035, v.co.y)


def female_street(ctx):
    """The commonest woman on the street: a plain kurta over a salwar, flat shoes, a long plait."""
    L = Landmarks(ctx)
    kurta = as_kurta(worn(ctx, "dress01", "aethelraed_flapper_dress", tint=KURTA_MAGENTA, detail=0.05), L)
    flare(kurta, L, hem_of(kurta), L.waist, 0.035)
    salwar = worn(ctx, "pants01", "toigo_harem_pants", tint=SALWAR_CREAM, detail=0.05)
    tuck(salwar, L, hem_of(kurta) - 0.1, hem_of(kurta) - 0.01)
    cut_above(salwar, L.hip - 0.03)
    neat_plait(ctx, ctx.parts["hair"], L)
    return [(kurta, "cover"), (salwar, "cover"), (worn(ctx, "shoes01", "toigo_mj_cloth_shoes", tint=FLATS_BROWN, detail=0.8), "cover")]


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

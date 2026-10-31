"""Outfits: each returns [(object, role)], role in {"cover", "accessory", "hair_replace"}.

Colours here are review colours; final cloth patterns (zari borders, bandhani dots, weave) are
procedural and baked in finish.py.
"""

import math

import bmesh
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


# ------------------------------------------------------------------ Panga: today's street clothes

SHIRT_BLUE = (0.20, 0.33, 0.60)
DENIM = (0.13, 0.19, 0.33)
GAMCHHA_RED = (0.78, 0.10, 0.10)
KAMEEZ_PINK = (0.74, 0.10, 0.38)
SALWAR_CREAM = (0.92, 0.88, 0.78)
DUPATTA_YELLOW = (0.94, 0.70, 0.12)
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
    """A half-sleeve checked shirt worn out over jeans and a belt, a chequered gamchha over the
    right shoulder, a steel kada; his own hair."""
    L = Landmarks(ctx)
    items = []
    waistline = L.waist - 0.03
    shirt_hem = L.hip - 0.07
    ankle = L.ankle + 0.035

    def jeans_offset(co, n):
        t = (waistline - co.z) / (waistline - ankle)
        return 0.008 + 0.006 * t + 0.008 * smoothstep(0.75, 1.0, t)

    jeans = g.shell(ctx, "jeans", lambda v: all_in(ctx, v, LEG_GROUPS, ankle, waistline), jeans_offset, smooth=4,
                    snap=g.z_snap(ankle, waistline))
    items.append((tag(jeans, DENIM), "cover"))
    belt = g.shell(ctx, "belt", lambda v: all_in(ctx, v, LEG_GROUPS, waistline - 0.035, waistline), lambda co, n: 0.016, smooth=3,
                   thickness=0.005, snap=g.z_snap(waistline - 0.035, waistline))
    items.append((tag(belt, (0.22, 0.12, 0.06)), "accessory"))

    # The shirt, loose, hanging over the belt and the top of the hips.
    def shirt_offset(co, n):
        below = smoothstep(waistline + 0.05, shirt_hem, co.z)
        return 0.011 + 0.022 * below
    shirt = g.shell(ctx, "shirt", upper_body(ctx, L, shirt_hem, 0.42, 0.06, 0.02), shirt_offset, smooth=3, thickness=0.003,
                    edge_iters=20)
    items.append((tag(shirt, SHIRT_BLUE), "cover"))
    # The button placket down the front.
    top_z = L.clav - 0.06
    placket = [L.surface(0.0, top_z - (top_z - shirt_hem) * s, push=0.017) for s in [i / 10 for i in range(11)]]
    items.append((tag(g.strip(ctx, "shirt_placket", placket, [0.028] * 11, thickness=0.003), SHIRT_BLUE), "accessory"))

    # Gamchha thrown over the right shoulder: one end down the chest, the other down the back.
    for k, front in enumerate((True, False)):
        top = L.surface(-0.115, L.clav + 0.005, front=front, push=0.03)
        length = 0.42 if front else 0.36
        path = [top + Vector((0.01 * s, 0.0, -length * s)) for s in [i / 9 for i in range(10)]]
        items.append((tag(g.strip(ctx, f"gamchha{k}", path, [0.16 + 0.03 * i / 9 for i in range(10)],
                                  normals=[Vector((0, -1 if front else 1, 0))] * 10, thickness=0.003,
                                  shrink_to=ctx.proxy, shrink_offset=0.028), GAMCHHA_RED), "accessory"))

    jewellery(ctx, items, armbands=False, kadas=1, neck=False, metal=STEEL, metal_kind="silver")
    return items


def female_street(ctx):
    """Salwar kameez: a kameez with its front and back panels falling to the knee (slit at the
    sides), a full salwar gathered at the ankles, a dupatta across the chest from the right hip over
    the left shoulder and down the back; glass bangles and payal."""
    L = Landmarks(ctx)
    items = []
    waistline = L.waist - 0.02
    hem = L.ankle + 0.05

    def salwar_offset(co, n):
        t = (waistline - co.z) / (waistline - hem)
        loose = 0.01 + 0.04 * math.sin(math.pi * min(max(t, 0.0) * 0.95, 1.0))
        inner = 1.0 - 0.6 * max(0.0, -n.x * math.copysign(1.0, co.x)) * smoothstep(L.hip, L.knee, co.z)
        return loose * inner

    salwar = g.shell(ctx, "salwar", lambda v: all_in(ctx, v, LEG_GROUPS, hem, waistline), salwar_offset, smooth=6,
                     snap=g.z_snap(hem, waistline))
    items.append((tag(salwar, SALWAR_CREAM), "cover"))

    # The kameez: fitted through the body to the hips, three-quarter sleeves, round neck...
    kameez_hem = L.hip - 0.05

    def kameez_offset(co, n):
        return 0.009 + 0.028 * smoothstep(waistline, kameez_hem, co.z)
    kameez = g.shell(ctx, "kameez", upper_body(ctx, L, kameez_hem, 1.0, 0.07, 0.03, lower_sleeve=0.45), kameez_offset,
                     smooth=3, thickness=0.003, edge_iters=20)
    items.append((tag(kameez, KAMEEZ_PINK), "cover"))
    # ...and below the hips to the knee, over the salwar (just outside it), with a narrow panel
    # front and back closing the gap between the legs.
    skirt = g.shell(ctx, "kameez_skirt", lambda v: all_in(ctx, v, LEG_GROUPS, L.knee + 0.03, kameez_hem + 0.03),
                    lambda co, n: salwar_offset(co, n) + 0.014, smooth=6, snap=g.z_snap(L.knee + 0.03, kameez_hem + 0.03))
    items.append((tag(skirt, KAMEEZ_PINK), "cover"))
    for k, front in enumerate((True, False)):
        sgn = -1.0 if front else 1.0
        top = L.surface(0.0, kameez_hem, front=front) + Vector((0.0, sgn * 0.045, 0.02))
        drop = kameez_hem - L.knee - 0.01
        path = [top + Vector((0.0, sgn * 0.035 * s, -drop * s)) for s in [i / 9 for i in range(10)]]
        items.append((tag(g.strip(ctx, f"kameez_panel{k}", path, [0.16 + 0.04 * i / 9 for i in range(10)],
                                  normals=[Vector((0, -1 if front else 1, 0))] * 10, thickness=0.003), KAMEEZ_PINK), "accessory"))

    items.append((tag(diagonal_sash(ctx, L, "dupatta", half_width=0.075, offset=0.016, z_min=waistline - 0.03, smooth=9),
                      DUPATTA_YELLOW), "cover"))
    back_top = L.surface(0.10, L.clav - 0.02, front=False, push=0.02)
    fall = [back_top + Vector((0.02 * s, 0.0, -(L.clav - L.knee) * 0.62 * s)) for s in [i / 14 for i in range(15)]]
    items.append((tag(g.strip(ctx, "dupatta_fall", fall, [0.16 + 0.06 * i / 14 for i in range(15)], normals=[Vector((0, 1, 0))] * 15,
                              thickness=0.003, shrink_to=ctx.proxy, shrink_offset=0.03), DUPATTA_YELLOW), "accessory"))

    jewellery(ctx, items, armbands=False, kadas=4, anklets=True, neck=False)
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

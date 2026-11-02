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
from mathutils.geometry import intersect_point_line

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

# A top is baked pale and takes its colour in the game: three to a look (characters.rs::TOPS).
PALE = (0.9, 0.9, 0.9)
# Strong plain colours: they read at distance and don't shimmer.
JEANS_INDIGO = (0.16, 0.23, 0.38)
SHOE_WHITE = (0.80, 0.79, 0.76)
BAG_BLACK = (0.10, 0.11, 0.13)
SALWAR_CREAM = (0.90, 0.86, 0.76)
FLATS_BROWN = (0.34, 0.19, 0.12)


def pack_path(rel):
    """pack:<pack>/<kind>/<name>: a community .mhclo, as a look names it."""
    pack, kind, name = rel.removeprefix("pack:").split("/")
    return os.path.join(PACKS, pack, "x", kind, name, f"{name}.mhclo")


def worn(ctx, pack, name, *, tint=None, detail=1.0, rough=0.8, normal=None, tris=None, top=False, folds=0.0):
    """A MakeHuman community garment on this body. MPFB fits it from its .mhclo, which ties each
    vertex to three body vertices, and copies the body's bone weights through the same tie, so it
    follows any shape and every clip. See finish.fitted_material for `tint`, `detail` and `rough`;
    `normal` names a normal map beside the garment that its .mhmat leaves out. `tris` cuts a heavy
    one down. `top` makes it the look's top, which the game colours (finish.split_top)."""
    folds = folds or (0.7 if top else 0.0)
    from bl_ext.blender_org.mpfb.services.humanservice import HumanService
    path = pack_path(f"{pack}/clothes/{name}")
    obj = HumanService.add_mhclo_asset(path, ctx.body, asset_type="Clothes", subdiv_levels=0, material_type="MAKESKIN")
    print(f"[worn] {obj.name}")
    obj.name = name
    for m in list(obj.modifiers):
        if m.type != "ARMATURE":
            obj.modifiers.remove(m)
    if tris:
        d = obj.modifiers.new("Decimate", "DECIMATE")
        d.decimate_type = "COLLAPSE"
        d.ratio = tris / max(1, sum(len(p.vertices) - 2 for p in obj.data.polygons))
        d.use_collapse_triangulate = True
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.modifier_move_to_index(modifier=d.name, index=0)
        bpy.ops.object.modifier_apply(modifier=d.name)
    tag(obj, tint or (0.8, 0.8, 0.8), kind="fitted")
    obj["detail"], obj["rough"], obj["top"], obj["folds"] = detail, rough, top, folds
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


def taper(obj, ctx, L, amount):
    """Bring flared trouser legs in toward the shin: not at all from the knee up, by `amount` of
    their width at the hem."""
    for side in "lr":
        knee, ankle = ctx.bone(f"calf_{side}")[0], ctx.bone(f"foot_{side}")[0]
        for v in obj.data.vertices:
            if (v.co.x > 0) == (side == "l") and v.co.z < knee.z:
                t = (knee.z - v.co.z) / (knee.z - ankle.z)
                axis = knee.lerp(ankle, min(t, 1.0))
                k = 1.0 - amount * smoothstep(0.0, 1.0, t)
                v.co.x = axis.x + (v.co.x - axis.x) * k
                v.co.y = axis.y + (v.co.y - axis.y) * k
    return obj


def hang(obj, L, lo, hi, passes=30, gap=0.014):
    """Let a loose top hang from the chest: a fitted garment follows every curve of the body, and a
    plain colour shows them all, so a full figure reads as expecting. Between heights `lo` and `hi`,
    away from the arms: smooth the body's bumps out of the cloth, then in each sector round the body
    run it straight from bulge to bulge over the hollows, then lift it back off the skin wherever
    that sank it."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    reach = L.shoulder_l.x + 0.03
    # The hems stay; an open edge up on the chest is a pocket's mouth, which goes with the cloth.
    verts = [v for v in bm.verts if lo < v.co.z < hi and abs(v.co.x) < reach and (not v.is_boundary or v.co.z > L.waist + 0.05)]
    start = {v: v.co.copy() for v in verts}
    smooth = lambda n: [bmesh.ops.smooth_vert(bm, verts=verts, factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=False) for _ in range(n)]
    smooth(passes)
    axis_y, sectors, step = L.hip_l.y, 36, 0.02
    at = lambda v: (int((math.atan2(v.co.y - axis_y, v.co.x) + math.pi) / (2 * math.pi) * sectors) % sectors, round(v.co.z / step))
    reach_of = lambda v: math.hypot(v.co.x, v.co.y - axis_y)
    widest = {}
    for v in verts:
        key = at(v)
        widest[key] = max(widest.get(key, 0.0), reach_of(v))
    envelope = {}
    for s in range(sectors):
        points = sorted((level, r) for (q, level), r in widest.items() if q == s)
        hull = []
        for p in points:
            while len(hull) > 1 and (hull[-1][0] - hull[-2][0]) * (p[1] - hull[-2][1]) - (hull[-1][1] - hull[-2][1]) * (p[0] - hull[-2][0]) >= 0:
                hull.pop()
            hull.append(p)
        for a, b in zip(hull, hull[1:]):
            for level in range(a[0], b[0] + 1):
                envelope[(s, level)] = a[1] + (b[1] - a[1]) * (level - a[0]) / (b[0] - a[0])
    for v in verts:
        key = at(v)
        lift = envelope.get(key, widest[key]) - widest[key]
        away = Vector((v.co.x, v.co.y - axis_y, 0.0)).normalized()
        v.co += away * lift
    smooth(6)
    for v in verts:
        v.co = start[v].lerp(v.co, smoothstep(lo, lo + 0.06, start[v].z) * smoothstep(hi, hi - 0.04, start[v].z))
        hit, normal = L.bvh.find_nearest(v.co)[:2]
        if (v.co - hit).dot(normal) < gap:
            v.co = hit + normal * gap
    # Hems go with the cloth beside them.
    for v in bm.verts:
        if v.is_boundary and lo < v.co.z < hi and abs(v.co.x) < reach:
            moved = [o.co - start[o] for o in (e.other_vert(v) for e in v.link_edges) if o in start]
            if moved:
                v.co += sum(moved, Vector()) / len(moved)
    bm.to_mesh(obj.data)
    bm.free()
    return obj


def flatten_pockets(obj, L, gap=0.013):
    """Lay the chest of a shirt on the body: its patch pockets are folds in the one mesh, which pinch
    into dark spots once the cloth hangs, and carry a strap over them in two."""
    for v in obj.data.vertices:
        if 0.02 < abs(v.co.x) < 0.17 and L.chest - 0.09 < v.co.z < L.clav - 0.03 and v.co.y < L.hip_l.y:
            hit, normal = L.bvh.find_nearest(v.co)[:2]
            v.co = hit + normal * gap
    return obj


def over_trousers(top, bottom, L, out=0.014):
    """`top` worn out over `bottom`: the trousers pulled in under its hem and cut off above the
    hem's highest point (a shirt's tail curves up at the sides), the hem's folded lining off, the
    hem let out a little."""
    hem = hem_of(top)
    bm = bmesh.new()
    bm.from_mesh(top.data)
    side = max(v.co.z for v in bm.verts if v.is_boundary and v.co.z < L.waist and abs(v.co.x) < L.shoulder_l.x)
    bm.free()
    tuck(bottom, L, hem - 0.06, hem - 0.01)
    cut_above(bottom, side + 0.02)
    drop_fold(top, L, hem + 0.07)
    flare(top, L, hem, L.waist + 0.04, out)


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
    polo = hang(worn(ctx, "shirts01", "namuhekam_male_polo_shirt", tint=PALE, normal="Polo_Normal_OpenGL.png", top=True), L, L.hip, L.chest + 0.08)
    jeans = worn(ctx, "pants02", "punkduck_male_classic_jeans", tint=JEANS_INDIGO)
    over_trousers(polo, jeans, L)
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


def narrow_neck(obj, ctx, L, k=0.3):
    """The flapper dress has a boat neck, out to the shoulders: bring its rim `k` of the way in to the
    neck, and the cloth near it with it, lying on the skin. A kurta shows no collarbones."""
    neck = ctx.bone("neck_01")[0]
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    rim = [v.co.copy() for v in bm.verts if v.is_boundary and v.co.z > L.clav - 0.12 and abs(v.co.x) < L.shoulder_l.x]
    for v in bm.verts:
        if v.co.z < L.clav - 0.2 or abs(v.co.x) > L.shoulder_l.x + 0.03:
            continue
        near = min((v.co - c).length for c in rim)
        t = k * smoothstep(0.16, 0.0, near)
        if t > 0.0:
            v.co.x += (neck.x - v.co.x) * t
            v.co.y += (neck.y - v.co.y) * t
            hit, normal = L.bvh.find_nearest(v.co)[:2]
            if (v.co - hit).dot(normal) < 0.006:
                v.co = hit + normal * 0.006
    bm.to_mesh(obj.data)
    bm.free()


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


def low_bun(ctx, hair, L):
    """Wind the plait into a low bun: everything below the nape goes round a spiral on the back of the
    head, keeping its strands' shape, and rides the head. braid01 has no bun, and the packs' buns sit
    on the crown or come with a fringe over one eye."""
    skull, neck = ctx.bone("head")[0], ctx.bone("neck_01")[0]
    nape = neck.z + 0.035
    verts = [v for v in hair.data.vertices if v.co.z < nape and v.co.y > skull.y]
    bins = {}
    for v in verts:
        bins.setdefault(round(v.co.z / 0.02), []).append(v)
    axis = {k: (sum(v.co.x for v in vs) / len(vs), sum(v.co.y for v in vs) / len(vs)) for k, vs in bins.items()}
    back = max(v.co.y for v in hair.data.vertices if abs(v.co.z - nape) < 0.03)
    centre = Vector((0.0, back + 0.012, nape - 0.012))
    # The spiral, inward from its rim, at steps of `ds` along the plait.
    ds = 0.004
    theta, r, table = -0.5 * math.pi, 0.058, []
    for _ in range(int(0.8 / ds)):
        table.append((theta, r))
        theta += ds / max(r, 0.012)
        r = max(0.012, r - 0.0065 * ds / max(r, 0.012))
    for v in verts:
        k = round(v.co.z / 0.02)
        dx, dy = v.co.x - axis[k][0], v.co.y - axis[k][1]
        dz = v.co.z - k * 0.02
        a, rr = table[min(int((nape - v.co.z) * 1.15 / ds), len(table) - 1)]
        out = Vector((math.cos(a), 0.0, math.sin(a)))
        along = Vector((-math.sin(a), 0.0, math.cos(a)))
        dome = 0.03 * (1.0 - rr / 0.058)
        v.co = centre + out * (rr + dx * 0.8) + along * dz + Vector((0.0, dome + dy * 0.8, 0.0))
    ids = [v.index for v in verts]
    for group in hair.vertex_groups:
        if group.name != "head":
            group.remove(ids)
    hair.vertex_groups["head"].add(ids, 1.0, "REPLACE")


def female_street(ctx):
    """The commonest woman on the street: a plain kurta over a salwar, flat shoes, a long plait."""
    L = Landmarks(ctx)
    kurta = as_kurta(worn(ctx, "dress01", "aethelraed_flapper_dress", tint=PALE, detail=0.02, top=True), L)
    narrow_neck(kurta, ctx, L)
    flare(kurta, L, hem_of(kurta), L.waist, 0.035)
    salwar = worn(ctx, "pants01", "toigo_harem_pants", tint=SALWAR_CREAM, detail=0.05, folds=0.7)
    tuck(salwar, L, hem_of(kurta) - 0.1, hem_of(kurta) - 0.01)
    cut_above(salwar, L.hip - 0.03)
    neat_plait(ctx, ctx.parts["hair"], L)
    return [(kurta, "cover"), (salwar, "cover"), (worn(ctx, "shoes01", "toigo_mj_cloth_shoes", tint=FLATS_BROWN, detail=0.8), "cover")]


def short_sleeves(obj, ctx, bone, s):
    """Cut both sleeves across the arm at fraction `s` of `bone` (upperarm, lowerarm): a shirt turned
    back to the elbow, or a short-sleeved one."""
    bones = ("upperarm", "lowerarm", "hand")
    # MPFB hid the arm under the sleeve from the body: what still needs hiding, delete_hidden_body does.
    mask = ctx.proxy.modifiers.get(f"Delete.{obj.name}")
    if mask:
        ctx.proxy.modifiers.remove(mask)
    for side in "lr":
        h, t = ctx.bone(f"{bone}_{side}")
        axis = (t - h).normalized()
        at = h + (t - h) * s
        line = [at, *(ctx.bone(f"{b}_{side}")[1] for b in bones[bones.index(bone):])]

        def near(v):
            if (v.co - at).dot(axis) < -0.04:
                return False
            for a, b in zip(line, line[1:]):
                point, k = intersect_point_line(v.co, a, b)
                if (v.co - (a if k < 0 else b if k > 1 else point)).length < 0.1:
                    return True
            return False

        bm = bmesh.new()
        bm.from_mesh(obj.data)
        faces = [f for f in bm.faces if all(near(v) for v in f.verts)]
        geom = faces + list({e for f in faces for e in f.edges}) + list({v for f in faces for v in f.verts})
        bmesh.ops.bisect_plane(bm, geom=geom, plane_co=at, plane_no=axis, clear_outer=True)
        bm.to_mesh(obj.data)
        bm.free()
    return obj


def side_of(L, x, zs):
    """How far back the body goes at `x`, over the heights `zs`."""
    return max(L.surface(x, z, front=False).y for z in zs)


def strap(ctx, L, cloth, name, marks, width, colour):
    """A strap through `marks` (x, y, z), lying on the garment `cloth`."""
    front, back = L.surface(0.12, L.chest).y, L.surface(0.1, L.chest, front=False).y
    centre = Vector((0.0, (front + back) / 2, L.chest))
    path, normals = [], []
    for a, b in zip(marks, marks[1:]):
        for k in range(4):
            p = Vector(a).lerp(Vector(b), k / 4)
            path.append(p)
            normals.append((p - centre).normalized())
    s = g.strip(ctx, name, path, [width] * len(path), normals=normals, thickness=0.005, shrink_to=cloth, shrink_offset=0.008)
    # Even thickness spikes wherever a strap folds over itself, and the collar and seams leave it
    # rough.
    s.modifiers["Solidify"].use_even_offset = False
    bm = bmesh.new()
    bm.from_mesh(cloth.data)
    tree = BVHTree.FromBMesh(bm)
    bm.free()
    bm = bmesh.new()
    bm.from_mesh(s.data)
    bmesh.ops.smooth_vert(bm, verts=bm.verts[:], factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    bmesh.ops.smooth_vert(bm, verts=bm.verts[:], factor=0.5, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    # Smoothing pulls a strap in on a curved chest, under the cloth. Out is away from the body: a
    # garment's own normals aren't all outward.
    for v in bm.verts:
        hit = tree.find_nearest(v.co)[0]
        out = hit - L.bvh.find_nearest(hit)[0]
        if out.length > 1e-4 and (v.co - hit).dot(out.normalized()) < 0.01:
            v.co = hit + out.normalized() * 0.01
    bm.to_mesh(s.data)
    bm.free()
    return tag(s, colour)


def crossbody(ctx, L, cloth, name, bag_x, bag_top, width, colour, shoulder=-1.0):
    """A long strap from a bag at the small of the back over the shoulder on side `shoulder` (+1 is
    the left), across the chest and back round the far side to the bag."""
    f, b = L.surface(0.12, L.chest).y, L.surface(0.1, L.chest, front=False).y
    s = shoulder
    marks = ((bag_x, b + 0.02, bag_top), (0.03 * -s, b, L.chest), (0.09 * s, b, L.clav - 0.06), (0.125 * s, b - 0.01, L.clav + 0.04), (0.13 * s, (f + b) / 2, L.clav + 0.065),
             (0.125 * s, f + 0.015, L.clav + 0.02), (0.07 * s, f, L.clav - 0.12), (-0.05 * s, f, L.chest - 0.03), (-0.15 * s, f + 0.02, L.waist + 0.05),
             (-0.19 * s, (f + b) / 2, L.waist), (bag_x + 0.1 * -s, b - 0.01, bag_top - 0.02))
    return strap(ctx, L, cloth, name, marks, width, colour)


def loop_strap(ctx, L, cloth, name, side, bag_x, bag_top, width, colour):
    """A short strap: from the bag up the back, over the shoulder on `side` (+1 is the left), down the
    chest and under the arm to the bag."""
    f, b = L.surface(0.12, L.chest).y, L.surface(0.1, L.chest, front=False).y
    s = side
    marks = ((bag_x, b + 0.015, bag_top), (0.13 * s, b, L.clav - 0.08), (0.125 * s, b - 0.01, L.clav + 0.04), (0.13 * s, (f + b) / 2, L.clav + 0.065),
             (0.125 * s, f + 0.015, L.clav + 0.02), (0.15 * s, f, L.clav - 0.09), (0.19 * s, f + 0.03, L.clav - 0.2), (0.2 * s, (f + b) / 2, L.waist + 0.08), (bag_x + 0.05 * s, b, bag_top))
    return strap(ctx, L, cloth, name, marks, width, colour)


def on_the_back(ctx, L, name, x, z, size, bone, colour, gap=0.012):
    """A bag of `size` hanging flat against the body's back at (x, z), rigid on `bone`."""
    y = side_of(L, x, [z - size[2] / 2, z, z + size[2] / 2]) + size[1] / 2 + gap
    return tag(block(ctx, name, Vector((x, y, z)), size, bone), colour)


LAPTOP_BLACK = (0.09, 0.09, 0.10)
STRAP_DARK = (0.07, 0.07, 0.08)
TAN = (0.36, 0.20, 0.11)


TROUSER_CHARCOAL = (0.13, 0.14, 0.17)
OXFORD_BLACK = (0.07, 0.07, 0.08)
SNEAKER_WHITE = (0.82, 0.81, 0.78)
COTTON_WHITE = (0.86, 0.84, 0.79)
WORK_BROWN = (0.25, 0.20, 0.15)
CHAPPAL_BROWN = (0.26, 0.15, 0.09)
JEANS_BLUE = (0.20, 0.30, 0.47)
SALWAR_BEIGE = (0.76, 0.68, 0.52)


def office_man(ctx):
    """The commuter: a pale shirt, dark trousers, black formal shoes, a laptop bag slung behind."""
    L = Landmarks(ctx)
    shirt = flatten_pockets(worn(ctx, "shirts02", "elvs_male_shirt_untucked_bd1", tint=PALE, detail=0.35, top=True), L)
    hang(shirt, L, hem_of(shirt) - 0.07, L.clav - 0.01)
    trousers = taper(worn(ctx, "pants02", "elvs_male_trouser", tint=TROUSER_CHARCOAL, detail=0.7, folds=0.5), ctx, L, 0.28)
    over_trousers(shirt, trousers, L)
    shoes = worn(ctx, "shoes02", "mindfront_shoes_oxford_male", tint=OXFORD_BLACK, detail=0.6, rough=0.4, tris=3000)
    z = L.waist - 0.11
    bag = on_the_back(ctx, L, "laptop", 0.05, z, (0.37, 0.07, 0.27), "pelvis", LAPTOP_BLACK)
    flap = on_the_back(ctx, L, "laptop_flap", 0.05, z + 0.06, (0.36, 0.09, 0.15), "pelvis", LAPTOP_BLACK)
    band = crossbody(ctx, L, shirt, "strap_dark", 0.19, z + 0.12, 0.04, STRAP_DARK)
    return [(shirt, "cover"), (trousers, "cover"), (shoes, "cover"), (bag, "accessory"), (flap, "accessory"), (band, "accessory")]


def college_girl(ctx):
    """A student: a plain top, tight jeans, white sneakers, a ponytail, a sling bag."""
    L = Landmarks(ctx)
    top = worn(ctx, "shirts01", "joepal_crude_t-shirt_female", tint=PALE, detail=0.6, top=True)
    jeans = worn(ctx, "pants02", "punkduck_female_tight_jeans", tint=JEANS_BLUE, detail=0.8)
    over_trousers(top, jeans, L, 0.018)
    shoes = worn(ctx, "shoes02", "punkduck_tennis_shoes", tint=SNEAKER_WHITE, detail=0.8)
    z = L.hip + 0.03
    bag = on_the_back(ctx, L, "sling", 0.11, z, (0.2, 0.06, 0.15), "pelvis", TAN)
    band = crossbody(ctx, L, top, "strap_tan", 0.2, z + 0.07, 0.022, TAN)
    return [(top, "cover"), (jeans, "cover"), (shoes, "cover"), (bag, "accessory"), (band, "accessory")]


def gamchha(ctx, L, shirt):
    """The driver's towel on his left shoulder, ends down the chest and back."""
    f, b = L.surface(0.12, L.chest).y, L.surface(0.1, L.chest, front=False).y
    marks = ((0.135, f, L.chest + 0.02), (0.13, f + 0.02, L.clav + 0.02), (0.125, (f + b) / 2, L.clav + 0.07), (0.115, b - 0.02, L.clav + 0.02), (0.11, b, L.clav - 0.1), (0.105, b, L.chest - 0.02))
    return strap(ctx, L, shirt, "gamchha", marks, 0.13, (0.70, 0.17, 0.10))


def auto_driver(ctx):
    """An auto driver: a grey work shirt with the sleeves cut short, old brown trousers, rubber
    chappals on bare feet, a towel on the shoulder."""
    L = Landmarks(ctx)
    shirt = short_sleeves(worn(ctx, "shirts02", "elvs_male_shirt_untucked_bd1", tint=PALE, detail=0.35, top=True), ctx, "upperarm", 0.72)
    hang(flatten_pockets(shirt, L), L, hem_of(shirt) - 0.07, L.clav - 0.01)
    trousers = worn(ctx, "pants02", "mindfront_male_trousers_1", tint=WORK_BROWN, detail=0.12, folds=0.5)
    # Its deep creases read as camouflage from a distance.
    trousers["plain"] = True
    over_trousers(shirt, trousers, L)
    feet = worn(ctx, "shoes02", "elvs_male_flip_flop_sandals1", tint=CHAPPAL_BROWN, detail=0.4, tris=1600)
    return [(shirt, "cover"), (trousers, "cover"), (feet, "accessory"), (gamchha(ctx, L, shirt), "accessory")]


def aunty_suit(ctx):
    """A plain salwar suit on a fuller body, flat shoes, a handbag on the left shoulder."""
    L = Landmarks(ctx)
    kurta = as_kurta(worn(ctx, "dress01", "aethelraed_flapper_dress", tint=PALE, detail=0.02, top=True), L)
    hang(kurta, L, L.hip - 0.1, L.chest + 0.08)
    narrow_neck(kurta, ctx, L)
    flare(kurta, L, hem_of(kurta), L.waist, 0.03)
    salwar = worn(ctx, "pants01", "toigo_harem_pants", tint=SALWAR_BEIGE, detail=0.05, folds=0.7)
    tuck(salwar, L, hem_of(kurta) - 0.1, hem_of(kurta) - 0.01)
    cut_above(salwar, L.hip - 0.03)
    shoes = worn(ctx, "shoes01", "toigo_mj_cloth_shoes", tint=FLATS_BROWN, detail=0.8)
    z = L.waist - 0.08
    bag = on_the_back(ctx, L, "handbag", 0.13, z, (0.26, 0.085, 0.2), "spine_01", TAN)
    band = loop_strap(ctx, L, kurta, "strap_tan", 1.0, 0.12, z + 0.1, 0.03, TAN)
    neat_plait(ctx, ctx.parts["hair"], L)
    low_bun(ctx, ctx.parts["hair"], L)
    return [(kurta, "cover"), (salwar, "cover"), (shoes, "cover"), (bag, "accessory"), (band, "accessory")]


def elder_kurta(ctx):
    """An old man in a white kurta pyjama and leather sandals."""
    L = Landmarks(ctx)
    kurta = as_kurta(worn(ctx, "dress01", "aethelraed_flapper_dress", tint=PALE, detail=0.02, top=True), L)
    hang(kurta, L, L.hip - 0.1, L.chest + 0.1)
    narrow_neck(kurta, ctx, L)
    flare(kurta, L, hem_of(kurta), L.waist, 0.03)
    pyjama = worn(ctx, "pants01", "toigo_wool_pants", tint=COTTON_WHITE, detail=0.1, folds=0.7)
    tuck(pyjama, L, hem_of(kurta) - 0.1, hem_of(kurta) - 0.01)
    cut_above(pyjama, L.hip - 0.03)
    feet = worn(ctx, "shoes02", "elvs_male_flip_flop_sandals1", tint=CHAPPAL_BROWN, detail=0.4, tris=1600)
    return [(kurta, "cover"), (pyjama, "cover"), (feet, "accessory")]


def office_woman(ctx):
    """An office woman: a pale shirt with the sleeves turned back, dark trousers, formal shoes, a big
    tote on the right shoulder."""
    L = Landmarks(ctx)
    shirt = short_sleeves(worn(ctx, "shirts02", "elvs_male_shirt_untucked_bd1", tint=PALE, detail=0.35, top=True), ctx, "lowerarm", 0.3)
    hang(flatten_pockets(shirt, L), L, hem_of(shirt) - 0.07, L.clav - 0.01)
    trousers = worn(ctx, "pants01", "toigo_wool_pants", tint=TROUSER_CHARCOAL, detail=0.5, folds=0.5)
    over_trousers(shirt, trousers, L)
    shoes = worn(ctx, "shoes02", "mindfront_shoes_oxford_female", tint=OXFORD_BLACK, detail=0.6, rough=0.4, tris=3000)
    z = L.hip + 0.1
    bag = on_the_back(ctx, L, "tote", -0.13, z, (0.34, 0.09, 0.3), "spine_01", (0.70, 0.60, 0.44))
    band = loop_strap(ctx, L, shirt, "strap_canvas", -1.0, -0.12, z + 0.15, 0.035, (0.62, 0.53, 0.38))
    return [(shirt, "cover"), (trousers, "cover"), (shoes, "cover"), (bag, "accessory"), (band, "accessory")]


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

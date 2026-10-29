"""Carved figures for the architecture kit, sculpted from an MPFB (MakeHuman, CC0) body: posed
with the game_engine rig, fused into one stone-smooth mesh by voxel remeshing, and decimated.

  buddha_seated   seated in padmasana, hands in dhyana mudra, with an ushnisha
  vishnu          standing, four-armed: lower right in abhaya, lower left resting on the mace,
                  the upper pair raised to hold the discus and conch
  darpana         an apsara in tribhanga looking into a mirror held up in her right hand, the
                  left on her hip
  nartaki         an apsara dancing: one arm curved overhead, the other across, a foot crossed
  anjali          a devotee standing with palms joined at the chest
  venugopala      Krishna playing the flute, legs crossed at the ankle

Writes tools/blender/architecture/figures/<name>_{hi,lo}.ply (unit height, base at z 0, facing
-Y) and <name>.json (where the kit hangs attributes: hands, head top). The kit scripts
(buddhist.py, stepwell.py) import these; they run with --factory-startup, this one needs MPFB:

blender -b -P tools/blender/architecture/figures.py [-- name ...]
"""

import json
import math
import os

import bmesh
import bpy
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "figures")
MPFB_DATA = os.path.expanduser("~/.config/blender/5.2/mpfb/data")

FEMALE = {"gender": 0.0, "age": 0.4, "muscle": 0.35, "weight": 0.55, "proportions": 0.9, "height": 0.45,
          "cupsize": 0.65, "firmness": 0.6, "race": {"asian": 0.4, "caucasian": 0.3, "african": 0.3}}
MACRO = {"gender": 1.0, "age": 0.45, "muscle": 0.42, "weight": 0.6, "proportions": 0.9, "height": 0.5,
         "cupsize": 0.5, "firmness": 0.5, "race": {"asian": 0.4, "caucasian": 0.3, "african": 0.3}}

# Triangle budgets: the shrine and pavilion figures are seen close; niches repeat many times.
BUDGET = {"hi": 7000, "lo": 1400}


def build(female=False):
    from bl_ext.blender_org.mpfb.services.humanservice import HumanService as hs
    bpy.ops.wm.read_homefile(use_empty=True)
    body = hs.create_human(macro_detail_dict=FEMALE if female else MACRO)
    rig = hs.add_builtin_rig(body, "game_engine")
    kind = "female_generic" if female else "male_generic"
    proxy = hs.add_mhclo_asset(os.path.join(MPFB_DATA, f"proxymeshes/{kind}/{kind}.proxy"), body,
                               asset_type="Proxymeshes", subdiv_levels=0)
    for m in list(proxy.modifiers):
        if m.type != "ARMATURE":
            proxy.modifiers.remove(m)
    body.hide_set(True)
    return rig, proxy


def rest(rig):
    for pb in rig.pose.bones:
        pb.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()


def aim(rig, name, direction, twist=0.0):
    """Turn bone `name` (minimal rotation about its head) so it points along world `direction`,
    then roll it `twist` radians about its own axis."""
    pb = rig.pose.bones[name]
    inv = rig.matrix_world.inverted().to_3x3()
    want = (inv @ Vector(direction)).normalized()
    M = pb.matrix.copy()
    cur = (M.to_3x3() @ Vector((0, 1, 0))).normalized()
    R = cur.rotation_difference(want).to_matrix().to_4x4()
    if twist:
        R = Matrix.Rotation(twist, 4, want) @ R
    head = M.translation
    pb.matrix = Matrix.Translation(head) @ R @ Matrix.Translation(-head) @ M
    bpy.context.view_layer.update()


def reach(rig, side, target, pole):
    """Two-bone IK: bend the arm so the wrist lands on world `target`, the elbow toward `pole`."""
    S = bone_head(rig, f"upperarm_{side}")
    a = (bone_tail(rig, f"upperarm_{side}") - S).length
    b = (bone_tail(rig, f"lowerarm_{side}") - bone_head(rig, f"lowerarm_{side}")).length
    W = Vector(target)
    d = min((W - S).length, a + b - 1e-3)
    u = (W - S).normalized()
    p = Vector(pole) - u * u.dot(Vector(pole))
    p.normalize()
    ca = (a * a + d * d - b * b) / (2 * a * d)
    elbow = S + a * (ca * u + math.sqrt(max(0.0, 1 - ca * ca)) * p)
    aim(rig, f"upperarm_{side}", elbow - S)
    aim(rig, f"lowerarm_{side}", W - bone_head(rig, f"lowerarm_{side}"))


def fingers(rig, side, direction):
    """Lay the four fingers straight and together along `direction` (carved hands, not splayed)."""
    for finger in ("index", "middle", "ring", "pinky"):
        for k in (1, 2, 3):
            aim(rig, f"{finger}_0{k}_{side}", direction)


def dhoti(rig, hem_drop=0.16):
    """The draped lower garment: a skirt of cloth from the waist to mid-shin (lower with a
    bigger `hem_drop`), wider at the hips, with a pleated fall down the front. Remeshed into the
    legs it reads as carved drapery."""
    waist = bone_head(rig, "spine_01").z
    knee = bone_head(rig, "calf_l").z
    hem = knee - hem_drop
    cx, cy = bone_head(rig, "pelvis").x, bone_head(rig, "pelvis").y
    bm = bmesh.new()
    seg = 32
    rings = []
    for z, rx, ry in ((waist + 0.02, 0.16, 0.12), (waist - 0.08, 0.2, 0.14), (knee + 0.12, 0.2, 0.13),
                      (hem + 0.02, 0.19, 0.12), (hem, 0.18, 0.11)):
        ring = []
        for k in range(seg):
            a = 2 * math.pi * k / seg
            # Folds: shallow ripples round the hem growing toward the bottom.
            f = 1.0 + 0.04 * math.sin(a * 9) * (1.0 - (z - hem) / (waist - hem))
            ring.append(bm.verts.new((cx + rx * f * math.cos(a), cy + ry * f * math.sin(a), z)))
        rings.append(ring)
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(seg):
            bm.faces.new((r0[k], r0[(k + 1) % seg], r1[(k + 1) % seg], r1[k]))
    bm.faces.new(rings[0])
    bm.faces.new(list(reversed(rings[-1])))
    # The pleated front fall, and a sash knotted at the hip.
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((cx, cy - 0.12, (waist + hem) / 2 - 0.03))
                          @ Matrix.Diagonal((0.09, 0.05, waist - hem + 0.06, 1)))
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((cx, cy, waist - 0.06))
                          @ Matrix.Diagonal((0.42, 0.3, 0.05, 1)))
    me = bpy.data.meshes.new("dhoti")
    bm.to_mesh(me)
    o = bpy.data.objects.new("dhoti", me)
    bpy.context.scene.collection.objects.link(o)
    return o


def bone_head(rig, name):
    return rig.matrix_world @ rig.pose.bones[name].head


def bone_tail(rig, name):
    return rig.matrix_world @ rig.pose.bones[name].tail


def posed_mesh(proxy, name):
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(proxy.evaluated_get(dg))
    me.transform(proxy.matrix_world)
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    return o


def arm_vertices(proxy, side):
    """Indices of proxy vertices that follow one arm (upper arm down to the fingertips)."""
    groups = {g.index for g in proxy.vertex_groups
              if g.name.endswith(f"_{side}") and g.name.split("_")[0] in
              ("upperarm", "lowerarm", "hand", "index", "middle", "ring", "pinky", "thumb")}
    out = set()
    for v in proxy.data.vertices:
        if sum(g.weight for g in v.groups if g.group in groups) > 0.6:
            out.add(v.index)
    return out


def add_sphere(name, center, radius, squash=(1, 1, 1)):
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=16, v_segments=10, radius=radius)
    for v in bm.verts:
        v.co = Vector((v.co.x * squash[0], v.co.y * squash[1], v.co.z * squash[2])) + Vector(center)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    return o


def add_cylinder(name, a, b, radius, seg=12):
    a, b = Vector(a), Vector(b)
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=radius, radius2=radius, depth=(b - a).length)
    R = (b - a).to_track_quat("Z", "Y").to_matrix().to_4x4()
    bm.transform(Matrix.Translation((a + b) / 2) @ R)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    return o


def join(objs, name):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    objs[0].name = name
    return objs[0]


def stone(obj, voxel):
    """Fuse into one closed surface, soften like weathered stone."""
    m = obj.modifiers.new("remesh", "REMESH")
    m.mode = "VOXEL"
    m.voxel_size = voxel
    m.adaptivity = 0.0
    s = obj.modifiers.new("smooth", "CORRECTIVE_SMOOTH")
    s.iterations = 6
    s.smooth_type = "LENGTH_WEIGHTED"
    s.use_only_smooth = True
    bpy.context.view_layer.objects.active = obj
    for mod in list(obj.modifiers):
        bpy.ops.object.modifier_apply(modifier=mod.name)


def normalise(obj, pivot_xy):
    """Base at z 0, centred on `pivot_xy`, unit height. Returns the transform applied."""
    zs = [v.co.z for v in obj.data.vertices]
    z0, z1 = min(zs), max(zs)
    T = Matrix.Scale(1.0 / (z1 - z0), 4) @ Matrix.Translation((-pivot_xy[0], -pivot_xy[1], -z0))
    obj.data.transform(T)
    return T


def export_lods(obj, name, meta):
    os.makedirs(OUT, exist_ok=True)
    tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)
    for lod, budget in BUDGET.items():
        o = obj.copy()
        o.data = obj.data.copy()
        bpy.context.scene.collection.objects.link(o)
        d = o.modifiers.new("dec", "DECIMATE")
        d.ratio = min(1.0, budget / tris)
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_apply(modifier=d.name)
        bpy.ops.object.select_all(action="DESELECT")
        o.select_set(True)
        bpy.ops.wm.ply_export(filepath=os.path.join(OUT, f"{name}_{lod}.ply"), export_selected_objects=True,
                              apply_modifiers=False, export_normals=False, export_uv=False, export_colors="NONE",
                              export_triangulated_mesh=True, ascii_format=False)
        print(f"[figures] {name}_{lod}: {sum(len(p.vertices) - 2 for p in o.data.polygons)} tris")
    with open(os.path.join(OUT, f"{name}.json"), "w") as f:
        json.dump(meta, f, indent=1)


def to_list(T, p):
    return [round(c, 4) for c in (T @ Vector(p))]


# ------------------------------------------------------------------ figures

def buddha_seated(rig, proxy):
    rest(rig)
    # Padmasana: thighs forward and out, shins crossed in front with the feet on the thighs.
    for sx, side in ((1, "l"), (-1, "r")):
        aim(rig, f"thigh_{side}", (sx * 0.66, -0.75, -0.06))
        aim(rig, f"calf_{side}", (-sx * 0.9, -0.28 - (0.1 if side == "l" else 0.0), 0.08 if side == "l" else -0.02))
        aim(rig, f"foot_{side}", (-sx * 0.85, 0.2, 0.4), twist=sx * 0.8)
    # A slight forward settle of the spine and a gently bowed head.
    aim(rig, "spine_01", (0, -0.04, 1))
    aim(rig, "neck_01", (0, -0.12, 1))
    aim(rig, "head", (0, -0.18, 1))
    # Dhyana mudra: upper arms hang a little forward, forearms come in to the lap, the right
    # hand resting palm up in the left.
    for sx, side in ((1, "l"), (-1, "r")):
        aim(rig, f"upperarm_{side}", (sx * 0.22, -0.32, -0.92))
        aim(rig, f"lowerarm_{side}", (-sx * 0.72, -0.62, -0.3))
        aim(rig, f"hand_{side}", (-sx * 0.95, -0.15, 0.05 if side == "r" else 0.0), twist=sx * 1.4)
        fingers(rig, side, (-sx * 0.95, -0.15, 0.05 if side == "r" else 0.0))
    body = posed_mesh(proxy, "buddha")
    top = bone_tail(rig, "head")
    head = bone_head(rig, "head")
    up = (top - head).normalized()
    # Ushnisha (the cranial knot), a cap of tight curls over the scalp, long ear lobes.
    parts = [body, add_sphere("ushnisha", top - up * 0.012 + Vector((0, 0.012, 0)), 0.052, (1, 1, 0.85)),
             add_sphere("curls", top - up * 0.085 + Vector((0, 0.018, 0)), 0.1, (0.9, 1.0, 0.8))]
    for sx in (1, -1):
        parts.append(add_sphere("lobe", head + up * 0.02 + Vector((sx * 0.075, 0.02, 0)), 0.014, (0.6, 1, 2.4)))
    fig = join(parts, "buddha_seated")
    stone(fig, 0.007)
    pelvis = bone_head(rig, "pelvis")
    T = normalise(fig, (pelvis.x, pelvis.y + 0.04))
    export_lods(fig, "buddha_seated", {"lap": to_list(T, (bone_tail(rig, "hand_l") + bone_tail(rig, "hand_r")) / 2)})


def vishnu(rig, proxy):
    # Pose A: the lower arms. Abhaya with the right (forearm raised, palm out); the left hand
    # rests on the head of the mace planted at its side.
    rest(rig)
    aim(rig, "upperarm_r", (-0.22, -0.08, -0.97))
    aim(rig, "lowerarm_r", (-0.08, -0.62, 0.78))
    aim(rig, "hand_r", (0.0, -0.15, 1.0), twist=-1.2)
    aim(rig, "upperarm_l", (0.3, 0.02, -0.95))
    aim(rig, "lowerarm_l", (0.25, -0.4, -0.88))
    aim(rig, "hand_l", (0.2, -0.3, -0.93))
    fingers(rig, "r", (0.0, -0.15, 1.0))
    fingers(rig, "l", (0.2, -0.3, -0.93))
    lower = posed_mesh(proxy, "vishnu")
    meta_hands = {"mace": bone_tail(rig, "hand_l").copy()}
    # Pose B: the upper pair, out to the sides at shoulder height with the forearms raised, the
    # fingers together to bear the discus and conch.
    rest(rig)
    for sx, side in ((1, "l"), (-1, "r")):
        aim(rig, f"upperarm_{side}", (sx * 0.92, 0.15, -0.08))
        aim(rig, f"lowerarm_{side}", (sx * 0.3, 0.02, 0.95))
        aim(rig, f"hand_{side}", (sx * 0.1, -0.05, 1.0), twist=sx * 1.5)
        fingers(rig, side, (sx * 0.1, -0.05, 1.0))
    upper = posed_mesh(proxy, "upper")
    meta_hands["conch"] = bone_tail(rig, "hand_l").copy()
    meta_hands["discus"] = bone_tail(rig, "hand_r").copy()
    keep = arm_vertices(proxy, "l") | arm_vertices(proxy, "r")
    bm = bmesh.new()
    bm.from_mesh(upper.data)
    bm.verts.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if v.index not in keep], context="VERTS")
    # Cap the cut at the shoulders: voxel remeshing drops open tubes.
    bmesh.ops.holes_fill(bm, edges=bm.edges, sides=0)
    bm.to_mesh(upper.data)
    # Shift the extra arms up and back a touch so their shoulders sit behind the first pair.
    upper.data.transform(Matrix.Translation((0, 0.035, 0.03)))
    for k in ("conch", "discus"):
        meta_hands[k] += Vector((0, 0.035, 0.03))
    top = bone_tail(rig, "head")
    fig = join([lower, upper, dhoti(rig)], "vishnu")
    stone(fig, 0.007)
    T = normalise(fig, (0.0, 0.0))
    meta = {k: to_list(T, v) for k, v in meta_hands.items()}
    meta["head_top"] = to_list(T, top)
    export_lods(fig, "vishnu", meta)


def tribhanga(rig, weight="r"):
    """The thrice-bent stance: the weight on one straight leg pushes the hips that way, the
    other knee eases forward, the torso leans back over the hips and the head tilts again."""
    sx = -1 if weight == "r" else 1          # +X is the figure's left
    free = "l" if weight == "r" else "r"
    aim(rig, f"thigh_{weight}", (-sx * 0.07, 0.0, -1.0))
    aim(rig, f"calf_{weight}", (-sx * 0.03, 0.02, -1.0))
    aim(rig, f"thigh_{free}", (-sx * 0.1, -0.2, -0.97))
    aim(rig, f"calf_{free}", (-sx * 0.12, 0.14, -0.98))
    aim(rig, "spine_01", (-sx * 0.16, 0.0, 1.0))
    aim(rig, "spine_03", (-sx * 0.08, 0.02, 1.0))
    aim(rig, "neck_01", (sx * 0.05, -0.05, 1.0))
    aim(rig, "head", (sx * 0.16, -0.08, 1.0))


def ornaments(rig, female):
    """Hair (a bun at the nape for the women) and a necklace hanging on the chest."""
    head, top = bone_head(rig, "head"), bone_tail(rig, "head")
    up = (top - head).normalized()
    parts = []
    if female:
        parts.append(add_sphere("bun", head + up * 0.06 + Vector((0, 0.1, 0)), 0.065, (1.1, 0.9, 0.9)))
    neck = bone_head(rig, "neck_01")
    for k in range(9):
        a = (k - 4) / 4 * 1.1
        parts.append(add_sphere("bead", neck + Vector((math.sin(a) * 0.085, -0.075 - 0.02 * math.cos(a), -0.09 - 0.05 * math.cos(a))), 0.016))
    return parts


def finish_figure(rig, name, parts, meta_world):
    fig = join(parts, name)
    stone(fig, 0.007)
    T = normalise(fig, (0.0, 0.0))
    meta = {k: to_list(T, v) for k, v in meta_world.items()}
    meta["head_top"] = to_list(T, bone_tail(rig, "head"))
    export_lods(fig, name, meta)


def darpana(rig, proxy):
    rest(rig)
    tribhanga(rig, "r")
    # Right forearm raised before the face, the hand holding the mirror up; left hand on the hip.
    aim(rig, "upperarm_r", (-0.6, -0.35, -0.2))
    aim(rig, "lowerarm_r", (0.25, -0.55, 0.8))
    aim(rig, "hand_r", (0.25, -0.2, 0.95), twist=-1.0)
    fingers(rig, "r", (0.25, -0.2, 0.95))
    aim(rig, "upperarm_l", (0.55, 0.12, -0.83))
    aim(rig, "lowerarm_l", (-0.55, -0.25, -0.5))
    aim(rig, "hand_l", (-0.5, -0.2, -0.8))
    fingers(rig, "l", (-0.5, -0.2, -0.8))
    body = posed_mesh(proxy, "darpana")
    h = bone_tail(rig, "hand_r")
    face = bone_head(rig, "head") + Vector((0, -0.1, 0.05))
    n = (face - h).normalized()
    mirror = add_cylinder("mirror", h + n * 0.005, h - n * 0.02, 0.085, 20)
    finish_figure(rig, "darpana", [body, mirror, dhoti(rig, 0.06)] + ornaments(rig, True), {"mirror": h})


def nartaki(rig, proxy):
    rest(rig)
    # Weight on the left leg; the right crossed in front, its toe pointed down.
    aim(rig, "thigh_l", (-0.05, 0.0, -1.0))
    aim(rig, "calf_l", (-0.02, 0.02, -1.0))
    aim(rig, "thigh_r", (0.12, -0.3, -0.95))
    aim(rig, "calf_r", (0.14, 0.22, -0.97))
    aim(rig, "foot_r", (0.0, -0.5, -0.86))
    aim(rig, "spine_01", (0.12, 0.0, 1.0))
    aim(rig, "spine_03", (0.04, 0.02, 1.0))
    aim(rig, "head", (-0.15, -0.06, 1.0))
    # Left arm curved overhead, right arm out and the forearm swung across (gaja hasta).
    aim(rig, "upperarm_l", (0.45, -0.05, 0.89))
    aim(rig, "lowerarm_l", (-0.65, -0.1, 0.75))
    aim(rig, "hand_l", (-0.95, -0.1, 0.1), twist=1.2)
    fingers(rig, "l", (-0.95, -0.1, 0.1))
    aim(rig, "upperarm_r", (-0.88, -0.2, -0.3))
    aim(rig, "lowerarm_r", (0.55, -0.7, -0.1))
    aim(rig, "hand_r", (0.6, -0.4, -0.65))
    fingers(rig, "r", (0.6, -0.4, -0.65))
    body = posed_mesh(proxy, "nartaki")
    finish_figure(rig, "nartaki", [body, dhoti(rig, 0.06)] + ornaments(rig, True), {})


def anjali(rig, proxy):
    rest(rig)
    # Palms pressed together before the chest, fingers up, elbows out.
    aim(rig, "head", (0.0, -0.12, 1.0))
    chest = bone_head(rig, "neck_01") + Vector((0, -0.17, -0.24))
    for sx, side in ((1, "l"), (-1, "r")):
        reach(rig, side, chest + Vector((sx * 0.03, 0, 0)), (sx, 0.4, -0.8))
        aim(rig, f"hand_{side}", (-sx * 0.1, -0.15, 1.0), twist=sx * 1.5)
        fingers(rig, side, (-sx * 0.05, -0.12, 1.0))
    body = posed_mesh(proxy, "anjali")
    finish_figure(rig, "anjali", [body, dhoti(rig)] + ornaments(rig, False), {})


def venugopala(rig, proxy):
    rest(rig)
    # Weight on the left leg, the right crossed over in front with the toe down.
    aim(rig, "thigh_l", (-0.06, 0.0, -1.0))
    aim(rig, "calf_l", (-0.02, 0.02, -1.0))
    aim(rig, "thigh_r", (0.14, -0.22, -0.96))
    aim(rig, "calf_r", (0.12, 0.2, -0.97))
    aim(rig, "foot_r", (0.0, -0.45, -0.89))
    aim(rig, "spine_01", (0.1, 0.0, 1.0))
    aim(rig, "head", (-0.2, -0.05, 1.0))
    # The flute held level out to his right from the lips, both hands under it, the right
    # farther along.
    head = bone_head(rig, "head")
    mouth = head + Vector((-0.01, -0.1, 0.0))
    d = Vector((-0.88, -0.38, -0.12)).normalized()
    for side, along, pole in (("r", 0.36, (-0.6, 0.3, -1.0)), ("l", 0.15, (0.8, 0.2, -1.0))):
        reach(rig, side, mouth + d * along + Vector((0, 0, -0.07)), pole)
        aim(rig, f"hand_{side}", d + Vector((0, 0, 0.3)), twist=1.4 if side == "l" else -1.4)
        fingers(rig, side, d)
    body = posed_mesh(proxy, "venugopala")
    flute = add_cylinder("flute", mouth - d * 0.03, mouth + d * 0.55, 0.013)
    finish_figure(rig, "venugopala", [body, flute, dhoti(rig)] + ornaments(rig, False), {})


MALE_FIGS = {"buddha_seated": buddha_seated, "vishnu": vishnu, "anjali": anjali, "venugopala": venugopala}
FEMALE_FIGS = {"darpana": darpana, "nartaki": nartaki}


def main():
    import sys
    want = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else list(MALE_FIGS) + list(FEMALE_FIGS)
    for figs, female in ((MALE_FIGS, False), (FEMALE_FIGS, True)):
        names = [n for n in figs if n in want]
        if names:
            rig, proxy = build(female)
            for n in names:
                figs[n](rig, proxy)


if __name__ == "__main__":
    main()

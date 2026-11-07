"""A character made in VRoid Studio (a VRM), brought onto the game's skeleton: the anime heroine.

blender -b -P tools/blender/characters/vroid.py -- <model.vrm> [variant] [--out <glb>] [--drop a,b] [--fit woman|man]

`--fit`: the model fitted to one of the skeletons everyone shares (skeletons/<name>.json: Seed-san's
for the women, the male's for the men): its joints moved to that skeleton's, the mesh deformed to
follow (limbs lengthened or shortened, the spine straightened; head, hands and feet kept their size),
and every shared bone's frame made that skeleton's. It then plays that skeleton's clips (anime_anims
or male_anims) as they are: no clips of its own to build or check.

`--height`: her height in metres (default 1.60, the female's): the guns are full size, and a
smaller model's left hand falls short of the fore-end. `--drop`: meshes, and faces in materials, whose names contain one of these are left out (a
model's extras: props, wings, a backpack).

Needs the VRM add-on (blender -c extension install vrm). Steps:
- import the VRM; its expressions (shape keys) flattened to the neutral face (the game doesn't
  animate faces); its outline modifiers dropped (ours is drawn instead);
- its humanoid bones renamed onto ours (skeleton.py), VRM 1.0 or 0.x; a third spine bone added
  where the model has no upper chest; its other bones (hair, skirt) kept, unanimated;
- its arms posed from VRM's T-pose to our rig's rest angles (the female's, so the procedural
  moves and gun holds start where they expect) and that applied as its rest;
- its MToon materials rebuilt as plain textured ones named toon_* (cel shaded in game,
  crate::rim), cut-outs alpha-tested;
- its meshes joined, LODs made, the ink outline hulls (finish.outline, not on the eyes and brows)
  and the weapon bone added; saved as build/char_<variant>_final.blend (what anims.py builds clips
  from) and exported as assets/models/characters/<variant>.glb.
"""

import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import export  # noqa: E402

# VRM's humanoid bones (snake case) for ours.
SPINE = {"hips": "pelvis", "spine": "spine_01", "chest": "spine_02", "upper_chest": "spine_03", "neck": "neck_01", "head": "head"}
LIMBS = {"shoulder": "clavicle", "upper_arm": "upperarm", "lower_arm": "lowerarm", "hand": "hand",
         "upper_leg": "thigh", "lower_leg": "calf", "foot": "foot", "toes": "ball"}
FINGERS = {"index": "index", "middle": "middle", "ring": "ring", "little": "pinky", "thumb": "thumb"}
JOINTS = {"proximal": "01", "intermediate": "02", "distal": "03"}
# VRM 1.0's thumb is metacarpal, proximal, distal; 0.x's proximal, intermediate, distal.
THUMB1 = {"metacarpal": "01", "proximal": "02", "distal": "03"}
# Our rig's bone frames at rest (rig_frames.py, from the female): each bone's direction and roll
# axis. A VRM stands in a T-pose, its bones rolled its own way; our clips and the game's arm code
# turn bones about their own axes, so the arms are posed to our rest directions and every mapped
# bone rolled to match (else the arms went up over the head).
import json  # noqa: E402
FRAMES = json.load(open(os.path.join(HERE, "rig_frames.json")))
# Kept off the ink outline (they sit in the face; a hull round them would ink over them).
NO_OUTLINE = re.compile(r"eye|brow|mouth|lash|iris|highlight|tooth|teeth|tongue", re.I)
# LOD0's budget (VRoid models run 30-80k; the game's characters are held to about this).
BUDGET = 40000


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def ours(vrm_name, spec):
    """Our bone for a VRM humanoid bone name (`left_upper_arm`, 0.x `leftUpperArm`), or None."""
    n = snake(vrm_name)
    if n in SPINE:
        return SPINE[n]
    m = re.match(r"(left|right)_(.+)", n)
    if not m:
        return None
    side, part = "l" if m.group(1) == "left" else "r", m.group(2)
    if part in LIMBS:
        return f"{LIMBS[part]}_{side}"
    f = re.match(r"(index|middle|ring|little|thumb)_(\w+)", part)
    if f:
        joints = THUMB1 if f.group(1) == "thumb" and spec == "1.0" else JOINTS
        if f.group(2) in joints:
            return f"{FINGERS[f.group(1)]}_{joints[f.group(2)]}_{side}"
    return None


def humanoid(arm):
    """{our bone: the armature's bone name} from the VRM's humanoid map."""
    ext = arm.data.vrm_addon_extension
    out = {}
    if ext.spec_version == "1.0":
        hb = ext.vrm1.humanoid.human_bones
        for key in dir(hb):
            b = getattr(hb, key, None)
            name = getattr(getattr(b, "node", None), "bone_name", "") if b is not None else ""
            o = ours(key, "1.0")
            if o and name:
                out[o] = name
    else:
        for b in ext.vrm0.humanoid.human_bones:
            o = ours(b.bone, "0.x")
            if o and b.node.bone_name:
                out[o] = b.node.bone_name
    return out


def rename(arm, mapping):
    """The armature's humanoid bones renamed to ours (their vertex groups follow)."""
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    for ours_name, theirs in mapping.items():
        if theirs == ours_name:
            continue
        for o in meshes:
            g = o.vertex_groups.get(theirs)
            if g:
                g.name = ours_name
        arm.data.bones[theirs].name = ours_name


def add_upper_chest(arm):
    """A spine_03 between the chest and the neck when the model has none: the clips turn three
    spine bones. It weighs nothing; the neck and shoulders hang from it."""
    if "spine_03" in arm.data.bones:
        return
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    chest, neck = eb["spine_02"], eb["neck_01"]
    b = eb.new("spine_03")
    b.head = chest.head.lerp(neck.head, 0.55)
    b.tail = neck.head
    b.roll = chest.roll
    b.parent = chest
    for c in list(chest.children):
        if c.name != "spine_03" and c.head.z > b.head.z - 0.01:
            c.parent = b
    bpy.ops.object.mode_set(mode="OBJECT")
    print("[vroid] added spine_03 (the model has no upper chest)")


# Each bone's tail onto the joint it carries on to, as our rig's are (a VRM's tails point anywhere,
# and the clips aim bones by their tails: the forearm aimed by a stray tail bent the elbow wrong
# and the gun went over her shoulder).
CHAIN = {"pelvis": "spine_01", "spine_01": "spine_02", "spine_02": "spine_03", "spine_03": "neck_01", "neck_01": "head",
         **{f"{a}_{s}": f"{b}_{s}" for s in "lr" for a, b in (("clavicle", "upperarm"), ("upperarm", "lowerarm"), ("lowerarm", "hand"),
                                                              ("hand", "middle_01"), ("thigh", "calf"), ("calf", "foot"), ("foot", "ball"))},
         **{f"{f}_0{k}_{s}": f"{f}_0{k + 1}_{s}" for s in "lr" for f in ("thumb", "index", "middle", "ring", "pinky") for k in (1, 2)}}


def joint_tails(arm):
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm.data.edit_bones
    # Our bones hang from ours: a mapped bone under a helper of the model's own (a forearm twist
    # bone between the forearm and the hand) is moved up to the nearest of ours above it (the
    # helper stays, riding along). The clips find each bone's line by its child among ours: with
    # the hand under a twist bone the forearm was measured along the upper arm, and the forearm,
    # hand and gun came out turned.
    ourset = set(CHAIN) | set(CHAIN.values()) | {"head"}
    moved = 0
    for b in eb:
        if b.name in ourset and b.parent is not None and b.parent.name not in ourset:
            a = b.parent
            while a is not None and a.name not in ourset:
                a = a.parent
            if a is not None:
                b.use_connect = False
                b.parent = a
                moved += 1
    if moved:
        print(f"[vroid] {moved} of our bones moved up past the model's helper bones")
    done = 0
    for a, b in CHAIN.items():
        if a in eb and b in eb and (eb[b].head - eb[a].head).length > 1e-3:
            eb[a].tail = eb[b].head
            done += 1
    # End bones (the head, fingertips, toes) keep their own line but no stray length.
    bpy.ops.object.mode_set(mode="OBJECT")
    print(f"[vroid] {done} bones' tails set on their joints")


# Shoulder to wrist at 1.60 m (Seed-san's, whose left hand reaches the full-size guns' fore-ends):
# VRoid's own arms are 7-10 cm shorter, too short for the hand to reach (the IK in crate::arms
# stops at the arm's length).
ARM_REACH = 0.536


def to_our_rest(arm, height, stretch=True):
    """The arms posed from the T-pose to our rest angles, lengthened to ARM_REACH where they're
    shorter, and that made the rest pose (the meshes deformed to it first)."""
    meshes = [o for o in bpy.data.objects if o.type == "MESH" and any(m.type == "ARMATURE" for m in o.modifiers)]
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    W = arm.matrix_world
    held = []
    for side in ("l", "r"):
        for name in ("clavicle", "upperarm", "lowerarm", "hand"):
            bone = f"{name}_{side}"
            if bone not in arm.pose.bones or bone not in FRAMES:
                continue
            pb = arm.pose.bones[bone]
            bpy.context.view_layer.update()
            head, tail = W @ pb.head, W @ pb.tail
            want = Vector(FRAMES[bone]["y"])
            R = (tail - head).normalized().rotation_difference(want.normalized()).to_matrix().to_4x4()
            pb.matrix = W.inverted() @ (Matrix.Translation(head) @ R @ Matrix.Translation(-head) @ (W @ pb.matrix))
    # Each hand turned about its own line until its palm faces as ours does (from the knuckles):
    # the clips and the gun's place in the hand assume our palm's facing, and a T-pose's hand
    # brought down to our arms' angle is left twisted (the gun came out over the shoulder).
    for side in ("l", "r"):
        need = (f"hand_{side}", f"index_01_{side}", f"pinky_01_{side}")
        if not all(n in arm.pose.bones and n in FRAMES for n in need):
            continue
        bpy.context.view_layer.update()
        P = lambda n: W @ arm.pose.bones[n].head
        F = lambda n: Vector(FRAMES[n]["head"])
        axis = (P(f"middle_01_{side}") - P(f"hand_{side}")).normalized()
        mine = (P(f"index_01_{side}") - P(f"hand_{side}")).cross(P(f"pinky_01_{side}") - P(f"hand_{side}"))
        want = (F(f"index_01_{side}") - F(f"hand_{side}")).cross(F(f"pinky_01_{side}") - F(f"hand_{side}"))
        mine, want = (mine - axis * mine.dot(axis)).normalized(), (want - axis * want.dot(axis)).normalized()
        ang = mine.angle(want)
        if mine.cross(want).dot(axis) < 0:
            ang = -ang
        pb = arm.pose.bones[f"hand_{side}"]
        head = P(f"hand_{side}")
        R = Matrix.Rotation(ang, 4, axis)
        pb.matrix = W.inverted() @ (Matrix.Translation(head) @ R @ Matrix.Translation(-head) @ (W @ pb.matrix))
        print(f"[vroid] hand_{side} turned {ang * 57.3:+.0f} deg to our palm's facing")
    # Arms too short stretched along their length (not thicker): the upper arm and forearm by the
    # same factor, their children not taking the scale (they only move out with the joint).
    bpy.context.view_layer.update()
    for side in ("l", "r"):
        names = [f"upperarm_{side}", f"lowerarm_{side}", f"hand_{side}"]
        if not all(n in arm.pose.bones for n in names):
            continue
        P = lambda n: W @ arm.pose.bones[n].head
        have = (P(names[1]) - P(names[0])).length + (P(names[2]) - P(names[1])).length
        k = ARM_REACH * height / 1.60 / have
        if k <= 1.0 or not stretch:
            continue
        for n in names[:2]:
            for c in arm.data.bones[n].children:
                held.append((c, c.inherit_scale))
                c.inherit_scale = "NONE"
            arm.pose.bones[n].scale.y *= k
        print(f"[vroid] arm_{side} lengthened x{k:.2f} ({have * 100:.1f} cm to {have * k * 100:.1f})")
    bpy.context.view_layer.update()
    bpy.ops.object.mode_set(mode="OBJECT")
    for o in meshes:
        mod = next(m for m in o.modifiers if m.type == "ARMATURE")
        name = mod.name
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_copy(modifier=name)
        bpy.ops.object.modifier_apply(modifier=name)
        o.modifiers[-1].name = name
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    bpy.ops.pose.armature_apply(selected=False)
    for b, mode in held:
        b.inherit_scale = mode
    # Every bone we share rolled as ours is (its direction kept).
    bpy.ops.object.mode_set(mode="EDIT")
    rolled = 0
    for eb in arm.data.edit_bones:
        if eb.name in FRAMES:
            eb.align_roll(Vector(FRAMES[eb.name]["z"]))
            rolled += 1
    bpy.ops.object.mode_set(mode="OBJECT")
    print(f"[vroid] {rolled} bones rolled to our rig's")


def flat_material(name, img, factor, cut):
    """A plain textured material (`img` times `factor`), alpha-tested if `cut`, named toon_*."""
    m = bpy.data.materials.new(f"toon_{name}")
    m.use_nodes = True
    nt = m.node_tree
    p = nt.nodes["Principled BSDF"]
    p.inputs["Roughness"].default_value = 0.8
    if img is not None:
        tex = nt.nodes.new("ShaderNodeTexImage")
        tex.image = img
        nt.links.new(tex.outputs["Color"], p.inputs["Base Color"])
        if cut:
            g = nt.nodes.new("ShaderNodeMath")
            g.operation = "GREATER_THAN"
            g.inputs[1].default_value = 0.5
            nt.links.new(tex.outputs["Alpha"], g.inputs[0])
            nt.links.new(g.outputs[0], p.inputs["Alpha"])
    else:
        p.inputs["Base Color"].default_value = tuple(factor)
    return m


def rebuild_materials(meshes):
    done = {}
    for o in meshes:
        for slot in o.material_slots:
            m = slot.material
            if m is None:
                continue
            if m.name not in done:
                mt = m.vrm_addon_extension.mtoon1
                src = mt.pbr_metallic_roughness.base_color_texture.index.source
                done[m.name] = flat_material(re.sub(r"\W+", "_", m.name), src, mt.pbr_metallic_roughness.base_color_factor, mt.alpha_mode != "OPAQUE")
            slot.material = done[m.name]


# The limbs fitted, parents first: each chain's bones turned and stretched so the next joint lands
# where the skeleton has it (the mesh deformed with them); a leg's first joint (the hip) moved onto
# the skeleton's too. The last of each (hand, ball) is only carried there; the feet are turned, not
# stretched (a shoe a third longer shows). The joints inside the torso (pelvis, spine, neck, head)
# are only moved in the rig, not the mesh: where a rig puts them is its maker's habit (VRoid's
# pelvis sits 10 cm above Seed-san's), not the body's shape, and stretching the hips to it shows.
FIT_CHAINS = [([f"{b}_{s}" for b in chain], place) for s in ("l", "r")
              for chain, place in ((("clavicle", "upperarm", "lowerarm", "hand"), False), (("thigh", "calf", "foot", "ball"), True))]
KEEP_LENGTH = ("foot_l", "foot_r")


def apply_pose_as_rest(arm):
    """The meshes deformed to the armature's pose, and that pose made its rest."""
    meshes = [o for o in bpy.data.objects if o.type == "MESH" and any(m.type == "ARMATURE" for m in o.modifiers)]
    bpy.ops.object.mode_set(mode="OBJECT")
    for o in meshes:
        name = next(m for m in o.modifiers if m.type == "ARMATURE").name
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_copy(modifier=name)
        bpy.ops.object.modifier_apply(modifier=name)
        o.modifiers[-1].name = name
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    bpy.ops.pose.armature_apply(selected=False)
    bpy.ops.object.mode_set(mode="OBJECT")


def fit(arm, skeleton, chains=FIT_CHAINS, every_bone=False, own_frames=False, aims=None):
    """The model onto a shared skeleton (`--fit`; see the top): `skeleton` a name in skeletons/ or
    one as a dict. `every_bone`: each of its bones placed (made where the model hasn't it, in its
    parent's frame), not only ours. `own_frames`: the bones only moved to the skeleton's joints, each
    keeping its own axis and roll (motus.py: the pack's clips are carried over as each bone's turn
    from rest, which joints in the same places make exact whatever the axes; the game's arm code
    turns our bones about our axes). `aims`: {bone: point} the chains bring the body's joint to
    instead of the skeleton's, the bone itself still placed on the skeleton's (motus.py: his neck)."""
    ref = skeleton if isinstance(skeleton, dict) else json.load(open(os.path.join(HERE, "skeletons", f"{skeleton}.json")))
    W = arm.matrix_world
    bpy.context.view_layer.objects.active = arm
    # A connected bone can't be moved off its parent's tail (the pelvis wasn't).
    bpy.ops.object.mode_set(mode="EDIT")
    for eb in arm.data.edit_bones:
        eb.use_connect = False
    bpy.ops.object.mode_set(mode="POSE")
    held = [(b, b.inherit_scale) for b in arm.data.bones]
    for b in arm.data.bones:
        b.inherit_scale = "NONE"
    head = lambda n: W @ arm.pose.bones[n].head
    aims = aims or {}
    want = lambda n: W @ Vector(aims.get(n, ref[n]["head"]))
    moved = []
    for chain, place in chains:
        chain = [n for n in chain if n in arm.pose.bones and n in ref]
        if place:
            pb = arm.pose.bones[chain[0]]
            bpy.context.view_layer.update()
            moved.append(f"{chain[0]} moved {(want(chain[0]) - head(chain[0])).length * 100:.1f} cm")
            pb.matrix = W.inverted() @ Matrix.Translation(want(chain[0]) - head(chain[0])) @ W @ pb.matrix
        for n, nxt in zip(chain, chain[1:]):
            bpy.context.view_layer.update()
            pb = arm.pose.bones[n]
            h, have, goal = head(n), head(nxt) - head(n), want(nxt) - head(n)
            R = have.normalized().rotation_difference(goal.normalized()).to_matrix().to_4x4()
            pb.matrix = W.inverted() @ (Matrix.Translation(h) @ R @ Matrix.Translation(-h) @ (W @ pb.matrix))
            k = 1.0 if n in KEEP_LENGTH else goal.length / max(have.length, 1e-6)
            pb.scale.y *= k
            moved.append(f"{n} x{k:.2f}")
    bpy.context.view_layer.update()
    errs = {n: (head(n) - want(n)).length for chain, place in chains for n in chain[0 if place else 1:] if n in arm.pose.bones and n in ref and not n.startswith("ball")}
    off = max(errs.values())
    for n, e in errs.items():
        if e > 0.01:
            print(f"[vroid] fit: {n} {e * 100:.1f} cm off")
    apply_pose_as_rest(arm)
    for b, mode in held:
        b.inherit_scale = mode
    # Every shared bone's place and frame the skeleton's (moving or turning a bone at rest moves no
    # vertex): the limbs' joints are there already; the torso's move to it.
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    framed = 0
    eds = arm.data.edit_bones
    for name, r in ref.items():
        if name not in FRAMES and not every_bone:
            continue
        # The bone over the pelvis (the male's Root) made if the model hasn't one.
        had = eds.get(name)
        eb = had or (eds.new(name) if every_bone or name == ref.get("pelvis", {}).get("parent") else None)
        if eb is None:
            continue
        like = had or eds.get(r.get("parent") or "")
        if own_frames and like is not None:
            y, z, length = like.y_axis.copy(), like.z_axis.copy(), like.length
        else:
            y, z, length = Vector(r["y"]), Vector(r["z"]), r["length"]
        eb.head = Vector(r["head"])
        eb.tail = Vector(r["head"]) + y * length
        eb.align_roll(z)
        framed += 1
    # And the skeleton's hierarchy: a bone's turn in a clip is in its parent's frame, so a parent the
    # skeleton hasn't (VRM's Position or its own Root over the pelvis) turns the body with it.
    for name, r in ref.items():
        if name in eds and (name in FRAMES or every_bone):
            eds[name].parent = eds.get(r["parent"]) if r.get("parent") else None
    bpy.ops.object.mode_set(mode="OBJECT")
    print(f"[vroid] fitted to the {skeleton if isinstance(skeleton, str) else 'given'}'s skeleton: joints within {off * 100:.2f} cm, {framed} bones framed; " + ", ".join(moved))


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    path = args[0]
    variant = args[1] if len(args) > 1 and not args[1].startswith("--") else "anime"
    out = args[args.index("--out") + 1] if "--out" in args else os.path.join(ROOT, "assets", "models", "characters", f"{variant}.glb")
    bpy.ops.wm.read_factory_settings(use_empty=True)
    import addon_utils
    addon_utils.enable("bl_ext.blender_org.vrm", default_set=True)
    bpy.ops.import_scene.vrm(filepath=path)
    arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    drop = [d.lower() for d in args[args.index("--drop") + 1].split(",")] if "--drop" in args else []
    for o in [o for o in bpy.data.objects if o.type == "MESH" and any(d in o.name.lower() for d in drop)]:
        bpy.data.objects.remove(o)
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    # A VRM 0.x model's meshes come parented to the rig through a half turn (the parent inverse),
    # their own coordinates facing the other way from the world's: baked in, so a mesh's vertices
    # are where the rig's bones are (desi.py measures the body against them).
    for o in meshes:
        if o.parent is not None:
            o.data.transform(o.matrix_parent_inverse @ o.matrix_basis)
            o.matrix_parent_inverse.identity()
            o.matrix_basis.identity()
    if drop:
        import bmesh
        for o in meshes:
            bm = bmesh.new()
            bm.from_mesh(o.data)
            mats = o.data.materials
            bmesh.ops.delete(bm, geom=[f for f in bm.faces if mats and mats[f.material_index] and any(d in mats[f.material_index].name.lower() for d in drop)], context="FACES")
            bm.to_mesh(o.data)
            bm.free()
    for o in meshes:
        # The add-on's own outline (a geometry nodes modifier) goes; ours is drawn.
        for m in [m for m in o.modifiers if m.type == "NODES"]:
            o.modifiers.remove(m)
        if o.data.shape_keys:
            bpy.context.view_layer.objects.active = o
            o.shape_key_clear()
    # To her height (the head bone's top, eyes about), scaled whole, the scale applied.
    want = float(args[args.index("--height") + 1]) if "--height" in args else 1.60
    tall = max((arm.matrix_world @ b.tail_local).z for b in arm.data.bones if b.name.lower().startswith(("head", "j_bip_c_head")) or "head" in b.name.lower())
    k = want / max(tall, 0.5)
    bpy.ops.object.select_all(action="DESELECT")
    for o in [arm] + meshes:
        o.select_set(True)
    arm.scale = arm.scale * k
    bpy.context.view_layer.objects.active = arm
    bpy.context.view_layer.update()
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    for o in meshes:
        o.select_set(True)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    print(f"[vroid] scaled x{k:.3f} to {want:.2f} m")
    mapping = humanoid(arm)
    missing = [b for b in ("pelvis", "spine_01", "spine_02", "neck_01", "head", "upperarm_l", "lowerarm_l", "hand_l", "thigh_l", "calf_l", "foot_l",
                           "middle_01_l", "middle_01_r") if b not in mapping]
    if missing:
        sys.exit(f"[vroid] the model lacks humanoid bones: {missing}")
    rename(arm, mapping)
    add_upper_chest(arm)
    joint_tails(arm)
    skeleton = args[args.index("--fit") + 1] if "--fit" in args else None
    to_our_rest(arm, want, stretch=skeleton is None)
    if skeleton:
        fit(arm, skeleton)
    arm.name = arm.data.name = "Human.rig"
    rebuild_materials(meshes)
    # A desi look (desi.py): skin and hair retinted, the clothes dropped (--drop cloth) replaced.
    if "--skin" in args or "--hair" in args or "--outfit" in args:
        import desi
        rgb = lambda flag: tuple(float(x) for x in args[args.index(flag) + 1].split(","))
        tinted = set()
        for o in meshes:
            for slot in o.material_slots:
                m = slot.material
                img = next((n.image for n in m.node_tree.nodes if n.type == "TEX_IMAGE"), None) if m and m.node_tree else None
                if img is None or img.name in tinted:
                    continue
                if "--skin" in args and "skin" in m.name.lower():
                    desi.retint(img, rgb("--skin"))
                elif "--hair" in args and "hair" in m.name.lower():
                    desi.retint(img, rgb("--hair"), keep_light=0.35)
                else:
                    continue
                tinted.add(img.name)
        print(f"[vroid] retinted {len(tinted)} textures")
        if "--outfit" in args:
            outfit = args[args.index("--outfit") + 1]
            body = next(o for o in meshes if any("skin" in (s.material.name.lower() if s.material else "") for s in o.material_slots) and "face" not in o.name.lower())
            ctx = desi.body_context(body, arm)
            made = desi.OUTFITS[outfit](ctx)
            clav = (arm.matrix_world @ arm.data.bones["clavicle_l"].head_local).z
            hidden = desi.hide_covered(body, desi.COVERS[outfit], below=clav + desi.NECKLINE[outfit])
            hidden += desi.trim_hair([o for o in meshes if any("hair" in (s.material.name.lower() if s.material else "") for s in o.material_slots)], ctx)
            meshes += made
            print(f"[vroid] dressed in {outfit}: {len(made)} garments; {hidden} covered skin faces dropped")
    # One body.
    bpy.ops.object.select_all(action="DESELECT")
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.join()
    body = bpy.context.view_layer.objects.active
    body.name = body.data.name = f"{variant}_LOD0"
    export.triangulate(body)
    tris = sum(len(p.vertices) - 2 for p in body.data.polygons)
    if tris > BUDGET:
        d = body.modifiers.new("Budget", "DECIMATE")
        d.ratio = BUDGET / tris
        bpy.context.view_layer.objects.active = body
        bpy.ops.object.modifier_move_to_index(modifier="Budget", index=0)
        bpy.ops.object.modifier_apply(modifier="Budget")
    lods = export.make_lods(body, (0.45, 0.18))
    for lod in lods[1:]:
        lod.parent = arm
    import finish
    import bmesh
    hulls = []
    for lod, w in zip(lods[:2], (0.004, 0.009)):
        # The hull from all but the face's painted parts.
        src = lod.copy()
        src.data = lod.data.copy()
        bpy.context.scene.collection.objects.link(src)
        bm = bmesh.new()
        bm.from_mesh(src.data)
        mats = src.data.materials
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if mats[f.material_index] and NO_OUTLINE.search(mats[f.material_index].name)], context="FACES")
        bm.to_mesh(src.data)
        bm.free()
        src.name = lod.name
        h = finish.outline(src, arm, w)
        bpy.data.objects.remove(src)
        h.name = h.data.name = lod.name.replace("_LOD", "_outline_LOD")
        hulls.append(h)
    import skeleton
    skeleton.add_weapon_bone(arm)
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ROOT, "build", f"char_{variant}_final.blend"))
    bpy.ops.object.select_all(action="DESELECT")
    for o in [arm] + lods + hulls:
        o.select_set(True)
    bpy.context.view_layer.objects.active = arm
    os.makedirs(os.path.dirname(out), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=out, export_format="GLB", use_selection=True, export_yup=True, export_apply=False,
        export_skins=True, export_def_bones=True, export_animations=False, export_tangents=True, export_image_format="AUTO",
    )
    counts = {o.name: sum(len(p.vertices) - 2 for p in o.data.polygons) for o in lods + hulls}
    height = max((arm.matrix_world @ b.head_local).z for b in arm.data.bones)
    print(f"[vroid] {variant}: {len(mapping)} humanoid bones mapped, {height:.2f} m tall; exported {out}: {counts}")


if __name__ == "__main__":
    main()

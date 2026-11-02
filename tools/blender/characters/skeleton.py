"""The game's skeleton, in one place: its bone names (the game, the clips and the build all use
these), how each animation source's bones map onto them, and the weapon bone a long gun rides.

Bodies: every character is built on this skeleton (human.py: MPFB's game_engine rig, its arms and
shoulders shaped to the mannequin the gun clips were captured on, so a captured hold puts both
hands where the gun is). Animation sources: one bone map each (MIXAMO_BONES, MOTUS_BONES); a new
pack is a new map. Weapons: `weapon_r`, a bone in the right hand placed where MoCap Online's rifle
pack holds its rifle, so a long gun attached there with no offset sits as captured.
"""

import math
import os

import bpy
from mathutils import Matrix, Quaternion, Vector

HERE = os.path.dirname(os.path.abspath(__file__))

# Mixamo's names for ours (and through them MoCap Online's, the same without the prefix).
MIXAMO_BONES = {
    "pelvis": "mixamorig:Hips", "spine_01": "mixamorig:Spine", "spine_02": "mixamorig:Spine1", "spine_03": "mixamorig:Spine2",
    "neck_01": "mixamorig:Neck", "head": "mixamorig:Head",
    **{f"{a}_{s}": f"mixamorig:{S}{b}" for s, S in (("l", "Left"), ("r", "Right"))
       for a, b in (("clavicle", "Shoulder"), ("upperarm", "Arm"), ("lowerarm", "ForeArm"), ("hand", "Hand"),
                    ("thigh", "UpLeg"), ("calf", "Leg"), ("foot", "Foot"), ("ball", "ToeBase"))},
}
MIXAMO_BONES.update({f"{f}_0{k}_{s}": f"mixamorig:{S}Hand{F}{k}" for s, S in (("l", "Left"), ("r", "Right"))
                     for f, F in (("index", "Index"), ("middle", "Middle"), ("ring", "Ring"), ("pinky", "Pinky"), ("thumb", "Thumb"))
                     for k in (1, 2, 3)})
MOTUS_BONES = {k: v.removeprefix("mixamorig:") for k, v in MIXAMO_BONES.items()}

# MoCap Online's Rifle Basic pack (not in the repository: its licence).
PACK = os.path.join(HERE, "mocaponline", "FBX_Rifle_Basic_27A2")
# The pack's rifle on its weapon bone (hand_r_wep, fixed in the right hand in every clip): the
# rifle as imported, a quarter turn about X then half turns about Y and Z (fitted: the captured
# left hand closes on the handguard, run and aim alike).
GUN_ON_BONE = Matrix.Rotation(math.pi / 2, 4, "X") @ Matrix.Rotation(math.pi, 4, "Y") @ Matrix.Rotation(math.pi, 4, "Z")
# Our long guns' geometry is the imported rifle turned half round Z (weapons/pack_guns.py).
GUN_TURN = Matrix.Rotation(math.pi, 4, "Z")
# glTF's axes (Y up) from Blender's: the exporter writes a bone's frame unconverted under a
# converted root, so a bone meant to carry converted geometry with no offset is turned back.
TO_GLTF = Matrix.Rotation(-math.pi / 2, 4, "X")
WEAPON = "weapon_r"
# `weapon_r` relative to `hand_r`, in Blender's bone axes, for a Mac without the pack: translation,
# quaternion (w, x, y, z), read from the baked rigs (male.glb, female.glb), which the clips are on.
WEAPON_LOCAL = {
    "male": ((-0.035374418, 0.094053075, 0.015550658), (0.705908000, 0.618134141, 0.322162718, -0.125759333)),
    "female": ((-0.034811616, 0.094522685, 0.013884805), (0.712207437, 0.620924354, 0.305053234, -0.118979387)),
}


def load_pack_rest():
    """The pack's mannequin rig at rest (imported; the caller removes it, its action already gone)."""
    objs, acts = set(bpy.data.objects), set(bpy.data.actions)
    bpy.ops.import_scene.fbx(filepath=os.path.join(PACK, "MotusMan", "MotusMan_v55.fbx"))
    new = [o for o in bpy.data.objects if o not in objs]
    arm = next(o for o in new if o.type == "ARMATURE")
    arm.animation_data_clear()
    for a in [a for a in bpy.data.actions if a not in acts]:
        bpy.data.actions.remove(a)
    for pb in arm.pose.bones:
        pb.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    return arm, new


def add_weapon_bone(rig, sex):
    """`weapon_r` under our right hand, where the pack's rifle sits relative to it: the pack's hand
    turned onto ours the way the clips are carried over (anims.transfer: the wrist-to-middle-knuckle
    line lined up), the gun's offset from the hand kept at full size (the guns are the pack's own).
    Without the pack, where the pack put it on this rig: WEAPON_LOCAL."""
    if not os.path.isdir(PACK):
        place_weapon_bone(rig, sex)
        return True
    mode = bpy.context.object.mode if bpy.context.object else "OBJECT"
    if mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    src, new = load_pack_rest()
    W = src.matrix_world
    pb = src.pose.bones
    s_hand = (W @ pb[MOTUS_BONES["hand_r"]].matrix)
    s_dir = (W @ pb[MOTUS_BONES["middle_01_r"]].head) - s_hand.translation
    loc, rot, _ = (W @ pb["hand_r_wep"].matrix).decompose()
    gun = Matrix.LocRotScale(loc, rot, None) @ GUN_ON_BONE @ GUN_TURN.inverted()
    for o in new:
        bpy.data.objects.remove(o)
    M = rig.matrix_world
    bones = rig.data.bones
    t_hand = M @ bones["hand_r"].head_local
    t_dir = (M @ bones["middle_01_r"].head_local) - t_hand
    align = s_dir.normalized().rotation_difference(t_dir.normalized()).to_matrix().to_4x4()
    at = t_hand + align.to_3x3() @ (gun.translation - s_hand.translation)
    frame = Matrix.Translation(at) @ align @ gun.to_3x3().to_4x4() @ TO_GLTF.inverted()
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode="EDIT")
    eb = rig.data.edit_bones
    b = eb.get(WEAPON) or eb.new(WEAPON)
    b.parent = eb["hand_r"]
    b.head = (0, 0, 0)
    b.tail = (0, 0.1, 0)
    b.matrix = M.inverted() @ frame
    # Exported with the deforming bones (it weighs nothing).
    b.use_deform = True
    bpy.ops.object.mode_set(mode="OBJECT")
    off = (at - t_hand).length
    print(f"[skeleton] {WEAPON}: {off * 100:.1f} cm from the right wrist, barrel {tuple(round(v, 2) for v in gun.to_3x3() @ Vector((0, 1, 0)))}")
    if mode != "OBJECT":
        bpy.ops.object.mode_set(mode=mode)
    return True


def place_weapon_bone(rig, sex):
    """The bone from WEAPON_LOCAL, no pack needed."""
    if bpy.context.object and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    t, q = WEAPON_LOCAL[sex]
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode="EDIT")
    eb = rig.data.edit_bones
    b = eb.get(WEAPON) or eb.new(WEAPON)
    b.parent = eb["hand_r"]
    b.head = (0, 0, 0)
    b.tail = (0, 0.1, 0)
    b.matrix = eb["hand_r"].matrix @ Matrix.LocRotScale(Vector(t), Quaternion(q), None)
    b.use_deform = True
    bpy.ops.object.mode_set(mode="OBJECT")
    print(f"[skeleton] {WEAPON}: from WEAPON_LOCAL, {Vector(t).length * 100:.1f} cm from the right wrist")

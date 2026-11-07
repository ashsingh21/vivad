"""The male on the rifle pack's own mannequin's skeleton (MoCap Online's Motus Man v55), so the pack's
clips play on him exactly as bought: no retargeting, no corrections (anims.py copies them bone for
bone, `exact`).

  blender -b -t 1 build/char_male_final.blend -P tools/blender/characters/motus.py

Run on the built male (build/char_male_final.blend, from `just characters`; finish.py calls `fit_body`
itself when it builds him). Steps:
- the mannequin's rest skeleton under our names (skeleton.motus_skeleton: its bones, its twist
  bones, its root);
- his limbs and fingers turned and stretched so every joint lands on the mannequin's (vroid.fit:
  the mesh deformed with them; he stands in its T), then every bone moved onto the mannequin's
  joint, keeping our axis and roll (the game's arm code turns bones about ours; anims.transfer
  carries a turn from rest over whatever the axes, exactly where the joints coincide), its twist
  bones added;
- his skin weights the mannequin's: from its own mesh, nearest surface, so he bends about its joints
  as it does (his own weights were made for MakeHuman's joints: its chest joint 18 cm lower);
- the weapon bone again (skeleton.add_weapon_bone: on the same hand, the pack's rifle where the pack
  holds it); saved over the build and exported as assets/models/characters/male.glb.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import skeleton  # noqa: E402
import vroid  # noqa: E402

# Each limb, then each finger, parents first (vroid.fit: a chain's bones turned and stretched so the
# next joint lands on the mannequin's; `True`: its first joint moved there too). The hand and the
# fingers' last joints are only carried.
FINGERS = ("thumb", "index", "middle", "ring", "pinky")
CHAINS = vroid.FIT_CHAINS + [([f"{f}_0{k}_{s}" for k in (1, 2, 3)], True) for s in ("l", "r") for f in FINGERS]


# The weights smoothed over this (m, a Gaussian's sigma; out to twice it): the layers of clothes over
# one spot of skin, a few centimetres apart, bend alike (from the nearest point each, the vest's and
# the shirt's under it differed where the weights change fast, at the shoulders, and bending, the
# shirt came through). Not the hands (the fingers would blur into each other).
SMOOTH = 0.025
HANDS = ("hand_", "thumb_", "index_", "middle_", "ring_", "pinky_")


def fit_body(rig):
    """`rig` and every mesh it deforms onto the mannequin's skeleton (see the top). False without the
    pack; nothing done twice."""
    if rig.get("skeleton") == "motus":
        print("[motus] already on the mannequin's skeleton")
        return True
    ref = skeleton.motus_skeleton()
    if ref is None:
        print("[motus] no rifle pack: left on its own skeleton")
        return False
    bpy.context.view_layer.objects.active = rig
    vroid.fit(rig, ref, CHAINS, every_bone=True, own_frames=True)
    meshes = [o for o in bpy.data.objects if o.type == "MESH" and any(m.type == "ARMATURE" and m.object == rig for m in o.modifiers)]
    take_weights(meshes)
    rig["skeleton"] = "motus"
    off = max((rig.matrix_world @ rig.data.bones[n].head_local - Vector(r["head"])).length for n, r in ref.items())
    print(f"[motus] {len(ref)} bones on the mannequin's (worst {off * 1000:.2f} mm); weights from its mesh on {len(meshes)} meshes")
    return True


def take_weights(meshes):
    """The mannequin's skin weights onto `meshes`: its mesh at rest (where theirs now stand), each
    vertex the weights of its nearest point on that surface, under our bone names."""
    objs = set(bpy.data.objects)
    src, new = skeleton.load_pack_rest()
    body = next(o for o in new if o.type == "MESH")
    names = skeleton.MOTUS_OURS
    dropped = 0.0
    for g in list(body.vertex_groups):
        if g.name in names:
            g.name = names[g.name]
        else:
            # Bones we don't carry (the fingertips' ends, the weapon bones): their weight, if any,
            # to the bone above (the hand, the last finger joint).
            bone = src.data.bones.get(g.name)
            up = bone.parent if bone else None
            while up is not None and up.name not in names:
                up = up.parent
            if up is not None:
                dst = body.vertex_groups.get(names[up.name]) or body.vertex_groups.get(up.name) or body.vertex_groups.new(name=up.name)
                for v in body.data.vertices:
                    for e in v.groups:
                        if e.group == g.index and e.weight > 0:
                            dst.add([v.index], e.weight, "ADD")
                            dropped += e.weight
            body.vertex_groups.remove(g)
    bones = {b.name for b in meshes[0].find_armature().data.bones} if meshes else set()
    for o in meshes:
        for g in [g for g in o.vertex_groups if g.name in bones]:
            o.vertex_groups.remove(g)
        for g in body.vertex_groups:
            o.vertex_groups.new(name=g.name)
        m = o.modifiers.new("motus_weights", "DATA_TRANSFER")
        m.object = body
        m.use_vert_data = True
        m.data_types_verts = {"VGROUP_WEIGHTS"}
        m.vert_mapping = "POLYINTERP_NEAREST"
        m.layers_vgroup_select_src = "ALL"
        m.layers_vgroup_select_dst = "NAME"
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_move_to_index(modifier=m.name, index=0)
        bpy.ops.object.modifier_apply(modifier=m.name)
        smooth_weights(o)
        # Every vertex's weights summing to one; groups left empty dropped.
        bpy.ops.object.mode_set(mode="WEIGHT_PAINT")
        bpy.ops.object.vertex_group_normalize_all(lock_active=False)
        bpy.ops.object.vertex_group_clean(group_select_mode="ALL", limit=0.001)
        bpy.ops.object.mode_set(mode="OBJECT")
    for o in [o for o in bpy.data.objects if o not in objs]:
        bpy.data.objects.remove(o)
    print(f"[motus] fingertip and weapon-bone weights moved up: {dropped:.1f} in all")


def smooth_weights(o):
    """`o`'s bone weights averaged over SMOOTH, by place (every vertex near, whatever it's joined to),
    four strongest kept, summing to one."""
    import numpy as np
    from mathutils.kdtree import KDTree
    verts = o.data.vertices
    n, groups = len(verts), list(o.vertex_groups)
    w = np.zeros((n, len(groups)), dtype=np.float32)
    for v in verts:
        for e in v.groups:
            w[v.index, e.group] = e.weight
    hand = np.array([g.name.startswith(HANDS) for g in groups])
    in_hand = w[:, hand].sum(axis=1) > 0.5
    tree = KDTree(n)
    for v in verts:
        tree.insert(v.co, v.index)
    tree.balance()
    out = w.copy()
    for v in verts:
        if in_hand[v.index]:
            continue
        near = [(i, d) for _, i, d in tree.find_range(v.co, 2 * SMOOTH) if not in_hand[i]]
        idx = np.array([i for i, _ in near])
        k = np.exp(-0.5 * (np.array([d for _, d in near]) / SMOOTH) ** 2)
        out[v.index] = k @ w[idx] / k.sum()
    # The four strongest (what the game skins with), summing to one.
    keep = np.argsort(-out, axis=1)[:, :4]
    mask = np.zeros_like(out, dtype=bool)
    np.put_along_axis(mask, keep, True, axis=1)
    out = np.where(mask & (out > 1e-3), out, 0.0)
    out /= np.maximum(out.sum(axis=1, keepdims=True), 1e-9)
    for g in groups:
        g.remove(range(n))
    for gi, g in enumerate(groups):
        for i in np.nonzero(out[:, gi])[0]:
            g.add([int(i)], float(out[i, gi]), "REPLACE")


def main():
    # Opened on the command line (opened from here, Blender 5.2 crashes in its allocator now and then).
    if os.path.basename(bpy.data.filepath) != "char_male_final.blend":
        bpy.ops.wm.open_mainfile(filepath=os.path.join(ROOT, "build", "char_male_final.blend"))
    rig = bpy.data.objects["Human.rig"]
    if not fit_body(rig):
        return
    skeleton.add_weapon_bone(rig)
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ROOT, "build", "char_male_final.blend"))
    out = os.path.join(ROOT, "assets", "models", "characters", "male.glb")
    bpy.ops.object.select_all(action="DESELECT")
    for o in [rig] + [o for o in bpy.data.objects if o.type == "MESH" and o.find_armature() == rig]:
        o.select_set(True)
    bpy.context.view_layer.objects.active = rig
    # As finish.py exports the characters.
    bpy.ops.export_scene.gltf(
        filepath=out, export_format="GLB", use_selection=True, export_yup=True, export_apply=False,
        export_skins=True, export_def_bones=True, export_animations=False, export_tangents=True,
        export_image_format="AUTO",
    )
    print(f"[motus] exported {out}")


if __name__ == "__main__":
    main()

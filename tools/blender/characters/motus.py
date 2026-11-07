"""The male on the rifle pack's own mannequin's skeleton (MoCap Online's Motus Man v55), so the pack's
clips play on him exactly as bought: no retargeting, no corrections (anims.py copies them bone for
bone, `exact`).

  blender -b -t 1 build/char_male_final.blend -P tools/blender/characters/motus.py

Run on the built male (build/char_male_final.blend, from `just characters`; finish.py calls `fit_body`
itself when it builds him). Steps:
- the mannequin's arms posed down as his rest holds his (arms_down; kept on the rig as
  `motus_rest`, skeleton.rest_pose), and its skeleton in that pose under our names
  (skeleton.motus_skeleton: its bones, its twist bones, its root). He keeps his own rest, arms down,
  his shoulders as modelled: fitted up into the mannequin's T, his own weights squashed them;
- his torso, limbs and fingers turned and stretched so every joint lands on the mannequin's
  (vroid.fit: the mesh deformed with them), then every bone moved onto the mannequin's joint,
  keeping our axis and roll (the game's arm code turns bones about ours; anims.transfer carries a
  turn from that rest over whatever the axes, exactly where the joints coincide), its twist bones
  added; his neck and head skin kept his own height over his shoulders (neck_aims: on the
  mannequin's joints his shoulders came up round his ears), the bones still on its joints;
- his skin weights the mannequin's: from its own mesh in that pose, nearest surface, so he bends
  about its joints as it does (his own weights were made for MakeHuman's joints: its chest joint
  18 cm lower);
- the weapon bone again (skeleton.add_weapon_bone: on the same hand, the pack's rifle where the pack
  holds it); saved over the build and exported as assets/models/characters/male.glb.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import json  # noqa: E402

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import skeleton  # noqa: E402
import vroid  # noqa: E402

# The torso, each limb, then each finger, parents first (vroid.fit: a chain's bones turned and
# stretched so the next joint lands on the mannequin's; `True`: its first joint moved there too). The
# mannequin stands with its chest further back over its hips than ours (6 cm at the shoulders): the
# torso leant back from the small of the back and the head on the neck, so his body sits round the
# mannequin's joints as its own does; then each arm from the collarbone moved onto the mannequin's
# (swung back to reach it instead, the collarbone squashed the shoulder in). The spine's middle joints
# are only moved (the mannequin's sit 11-18 cm higher: where the back bends, which its weights carry).
# The hand and the fingers' last joints are only carried.
FINGERS = ("thumb", "index", "middle", "ring", "pinky")
TORSO = [(["spine_01", "neck_01"], False), (["neck_01", "head"], True)]
ARMS = [([f"{b}_{s}" for b in ("clavicle", "upperarm", "lowerarm", "hand")], True) for s in ("l", "r")]
LEGS = [c for c in vroid.FIT_CHAINS if c[0][0].startswith("thigh")]
CHAINS = TORSO + ARMS + LEGS + [([f"{f}_0{k}_{s}" for k in (1, 2, 3)], True) for s in ("l", "r") for f in FINGERS]


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
    if not os.path.isdir(skeleton.PACK):
        print("[motus] no rifle pack: left on its own skeleton")
        return False
    src, new = skeleton.load_pack_rest()
    pose = arms_down(rig, src)
    for o in new:
        bpy.data.objects.remove(o)
    rig["motus_rest"] = json.dumps(pose)
    ref = skeleton.motus_skeleton(pose)
    aims = neck_aims(rig, ref)
    rig["motus_neck"] = json.dumps({n: list(Vector(p) - Vector(ref[n]["head"])) for n, p in aims.items()})
    bpy.context.view_layer.objects.active = rig
    vroid.fit(rig, ref, CHAINS, every_bone=True, own_frames=True, aims=aims)
    meshes = [o for o in bpy.data.objects if o.type == "MESH" and any(m.type == "ARMATURE" and m.object == rig for m in o.modifiers)]
    take_weights(meshes, rig)
    rig["skeleton"] = "motus"
    off = max((rig.matrix_world @ rig.data.bones[n].head_local - Vector(r["head"])).length for n, r in ref.items())
    print(f"[motus] {len(ref)} bones on the mannequin's (worst {off * 1000:.2f} mm); weights from its mesh on {len(meshes)} meshes")
    return True


def neck_aims(rig, ref):
    """Where his neck and head go ({bone: point}, vroid.fit's aims): his own neck. The mannequin's
    shoulders sit 2 cm higher on its chest than his and its neck is short (its helmet over it): fitted
    onto its joints, his shoulders came up round his ears. His neck's base as high over his shoulder
    joints as his own, the mannequin's neck leaning as it does, his own length; the bones still on the
    mannequin's joints (his head turns about its own, a few centimetres down his neck)."""
    B = rig.data.bones
    mid = lambda f: (f("upperarm_l") + f("upperarm_r")) / 2
    his = lambda n: B[n].head_local
    theirs = lambda n: Vector(ref[n]["head"])
    lift = (his("neck_01").z - mid(his).z) - (theirs("neck_01").z - mid(theirs).z)
    neck = theirs("neck_01") + Vector((0, 0, max(lift, 0.0)))
    head = neck + (theirs("head") - theirs("neck_01")).normalized() * (his("head") - his("neck_01")).length
    print(f"[motus] his neck: its base {(neck - theirs('neck_01')).length * 100:.1f} cm over the mannequin's, his head {(head - theirs('head')).length * 100:.1f} cm")
    return {"neck_01": neck, "head": head}


def arms_down(rig, src):
    """The mannequin `src`'s arms posed the way `rig`'s rest holds its own (down, an A; the
    mannequin's rest is a T): each of the upper arm, forearm and hand turned about its joint so the
    line to the next joint, and the bend after it, lie as his do. Its pose ({its bone: quaternion},
    skeleton.rest_pose): his body then rests as it was made, his shoulders as modelled (fitted out
    to a T, his own weights squashed them, and the clips bent them from there)."""
    W, M = src.matrix_world, rig.matrix_world
    names = skeleton.MOTUS_BONES
    ours = lambda n: M @ rig.data.bones[n].head_local
    theirs = lambda n: W @ src.pose.bones[names[n]].head

    def frame(p, q):
        x = p.normalized()
        y = (q - x * q.dot(x)).normalized()
        return Matrix((x, y, x.cross(y))).transposed()

    pose = {}
    for s in ("l", "r"):
        line = [f"upperarm_{s}", f"lowerarm_{s}", f"hand_{s}", f"middle_01_{s}"]
        for i, n in enumerate(line[:3]):
            # The bend after: the next bone's line, or across the knuckles for the hand.
            nxt = (line[i + 1], line[i + 2]) if i < 2 else (f"index_01_{s}", f"pinky_01_{s}")
            want = frame(ours(line[i + 1]) - ours(n), ours(nxt[1]) - ours(nxt[0]))
            have = frame(theirs(line[i + 1]) - theirs(n), theirs(nxt[1]) - theirs(nxt[0]))
            R = (want @ have.transposed()).to_4x4()
            pb = src.pose.bones[names[n]]
            h = theirs(n)
            pb.matrix = W.inverted() @ Matrix.Translation(h) @ R @ Matrix.Translation(-h) @ W @ pb.matrix
            bpy.context.view_layer.update()
            pb.rotation_mode = "QUATERNION"
            pose[pb.name] = list(pb.matrix_basis.to_quaternion())
    return pose


def take_weights(meshes, rig):
    """The mannequin's skin weights onto `meshes`: its mesh in his rest (where theirs now stand),
    each vertex the weights of its nearest point on that surface, under our bone names."""
    objs = set(bpy.data.objects)
    src, new = skeleton.load_pack_rest(rig)
    body = next(o for o in new if o.type == "MESH")
    # Its neck and head lifted as his are (neck_aims), so his neck takes its neck's weights.
    W = src.matrix_world
    below = Vector()
    for n, off in json.loads(rig.get("motus_neck", "{}")).items():
        pb = src.pose.bones[skeleton.MOTUS_BONES[n]]
        pb.matrix = W.inverted() @ Matrix.Translation(Vector(off) - below) @ W @ pb.matrix
        bpy.context.view_layer.update()
        below = Vector(off)
    # Its mesh as posed (the arms down), its weights kept.
    bpy.context.view_layer.objects.active = body
    for m in [m for m in body.modifiers if m.type == "ARMATURE"]:
        bpy.ops.object.modifier_apply(modifier=m.name)
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
        ride_layers(o)
        # Every vertex's weights summing to one; groups left empty dropped.
        bpy.ops.object.mode_set(mode="WEIGHT_PAINT")
        bpy.ops.object.vertex_group_normalize_all(lock_active=False)
        bpy.ops.object.vertex_group_clean(group_select_mode="ALL", limit=0.001)
        bpy.ops.object.mode_set(mode="OBJECT")
    for o in [o for o in bpy.data.objects if o not in objs]:
        bpy.data.objects.remove(o)
    print(f"[motus] fingertip and weapon-bone weights moved up: {dropped:.1f} in all")


# What's worn over the clothes (finish.py's `layer`: 2 the kit, 3 what's on it) within RIDE_NEAR of
# the layer under it takes that layer's weights there (fading to its own by RIDE_FAR), and stands
# RIDE_CLEAR off it at rest: the mannequin's weights change fast over the shoulders (made for its
# hard suit), so the vest's thin straps, a centimetre over the shirt with a few points to the shirt's
# many, bent apart from it and the shirt came through them as he raised his arms.
RIDE_NEAR, RIDE_FAR, RIDE_CLEAR = 0.02, 0.04, 0.005


def ride_layers(o):
    """`o`'s kit and its accessories (the `layer` attribute) riding the layer under them (above)."""
    import numpy as np
    from mathutils.bvhtree import BVHTree
    me = o.data
    if "layer" not in me.attributes:
        print(f"[motus] {o.name}: no layers marked, left as weighted")
        return
    n = len(me.vertices)
    layer = np.zeros(n, dtype=np.int32)
    me.attributes["layer"].data.foreach_get("value", layer)
    co = np.zeros(n * 3, dtype=np.float32)
    me.vertices.foreach_get("co", co)
    co = co.reshape(n, 3).astype(np.float64)
    groups = list(o.vertex_groups)
    w = np.zeros((n, len(groups)), dtype=np.float32)
    for v in me.vertices:
        for e in v.groups:
            w[v.index, e.group] = e.weight
    me.calc_loop_triangles()
    tris = [t.vertices[:] for t in me.loop_triangles]
    rode = pushed = 0
    for top in (2, 3):
        # The surface under: every triangle of the layers below (a triangle across two layers
        # can't be; parts aren't joined).
        under = [t for t in tris if layer[t[0]] < top]
        if not under:
            continue
        tree = BVHTree.FromPolygons([Vector(c) for c in co], under, all_triangles=True)
        for i in np.where(layer == top)[0]:
            p, nrm, fi, d = tree.find_nearest(Vector(co[i]), RIDE_FAR)
            if p is None:
                continue
            a, b, c = (co[k] for k in under[fi])
            e0, e1, e2 = b - a, c - a, np.array(p) - a
            d00, d01, d11, d20, d21 = e0 @ e0, e0 @ e1, e1 @ e1, e2 @ e0, e2 @ e1
            den = d00 * d11 - d01 * d01 or 1e-12
            v1, v2 = (d11 * d20 - d01 * d21) / den, (d00 * d21 - d01 * d20) / den
            bary = np.clip([1 - v1 - v2, v1, v2], 0, 1)
            k = 1.0 if d <= RIDE_NEAR else (RIDE_FAR - d) / (RIDE_FAR - RIDE_NEAR)
            w[i] = (1 - k) * w[i] + k * (w[list(under[fi])] * (bary / bary.sum())[:, None]).sum(0)
            rode += 1
            off = (Vector(co[i]) - p).dot(nrm)
            # (Over 1.5 cm behind it is another fold's face, not this one through it.)
            if d <= RIDE_NEAR and -0.015 < off < RIDE_CLEAR:
                co[i] += np.array(nrm) * (RIDE_CLEAR - off)
                pushed += 1
    me.vertices.foreach_set("co", co.astype(np.float32).ravel())
    for g in groups:
        col = w[:, g.index]
        for i in np.where(col > 1e-4)[0]:
            g.add([int(i)], float(col[i]), "REPLACE")
        gone = np.where(col <= 1e-4)[0]
        if len(gone):
            g.remove([int(i) for i in gone])
    me.update()
    print(f"[motus] {o.name}: {rode} vertices of the kit ride the layer under them, {pushed} stood off it")


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

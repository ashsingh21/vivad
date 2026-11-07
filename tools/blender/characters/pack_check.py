"""How exactly the male plays the rifle pack's clips (he's on its mannequin's skeleton, motus.py): each
clip's every joint, and the rifle's bone, against the mannequin's own playing the file as bought, in
world space, every frame: where each is (millimetres) and how far each bone has turned from its rest
(degrees: an end bone, the toes, a fingertip, shows only there). Exact is under a millimetre and a
tenth of a degree; where the file's own loop doesn't close (anims.transfer eases that bone shut
over the cycle's last frames: the pack's toes) those frames are left out for it, and said; the hips' place on the ground is
left out (the game plays them on the spot), the clip's timing matched as anims.py retimes and turns
it (the same poses, from the frame its cycle starts on).

  blender -b -t 1 build/anims_male.blend -P tools/blender/characters/pack_check.py [-- clip,clip]
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import json  # noqa: E402
import math  # noqa: E402

import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import anims  # noqa: E402
import skeleton  # noqa: E402

args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
only = args[0].split(",") if args else None
arm = bpy.data.objects["Human.rig"]
if arm.get("skeleton") != "motus":
    sys.exit("[pack] the male isn't on the mannequin's skeleton (motus.py)")
sc = bpy.context.scene
names = {**skeleton.MOTUS_BONES, **skeleton.MOTUS_TWIST}
# The rifle's bone: ours on the hand where the pack's is (skeleton.add_weapon_bone).
names[skeleton.WEAPON] = "hand_r_wep"
worst_all = worst_deg = 0.0
for name, file, speed, opts in anims.MOTUS_CLIPS:
    if (only and name not in only) or not bpy.data.actions.get(name, {}).get("exact"):
        continue
    got = anims.load_mixamo(file, anims.MOTUS)
    src, objs, acts, fps = got
    act = acts[0]
    ours = bpy.data.actions[name]
    arm.animation_data.action = ours
    bones = [n for n in names if n in arm.pose.bones and names[n] in src.pose.bones]
    W, M = src.matrix_world, arm.matrix_world
    f0, f1 = opts.get("window") or map(int, act.frame_range)
    frames = f1 - f0
    o0, o1 = map(int, ours.frame_range)
    out = o1 - o0

    # Each bone's turn from its rest, in world space (whatever the two rigs' bone axes). The
    # mannequin's rest the male's (skeleton.rest_pose: its arms down as his are).
    turn = lambda R, b, rest: (R @ b.matrix).to_quaternion() @ (R @ rest).to_quaternion().inverted()
    src.animation_data.action = None
    skeleton.set_pose(src, skeleton.rest_pose(arm))
    s_rest = {n: src.pose.bones[names[n]].matrix.copy() for n in bones}

    def pose_src(t):
        src.animation_data.action = act
        sc.frame_set(f0 + int(t), subframe=t - int(t))
        return {n: (W @ src.pose.bones[names[n]].head, turn(W, src.pose.bones[names[n]], s_rest[n])) for n in bones}

    def pose_ours(f):
        sc.frame_set(f)
        return {n: (M @ arm.pose.bones[n].head, turn(M, arm.pose.bones[n], arm.data.bones[n].matrix_local)) for n in bones}

    # Our frame i is the source's at (i + start) * frames / out, there and back for a pingpong.
    def src_time(i, start):
        if opts.get("pingpong"):
            k = i if i <= out // 2 else out - i
            return k * frames / (out // 2)
        return ((i + start) % out) * frames / out

    O = [pose_ours(o0 + i) for i in range(out + 1)]
    seam = json.loads(ours.get("seam", "{}"))
    eased, (s0, s1) = seam.get("bones", {}), seam.get("frames", (0, 0))
    # The right fingers closed on our gun's grip (anims.close_right_hand), not the pack's M4's.
    fitted = set(json.loads(ours.get("fitted", "[]")))
    # And what hangs from them (the gun from the eased hand).
    below = {n for n in bones if any(p.name in eased for p in arm.data.bones[n].parent_recursive)} | set(eased)

    def error(start, sample):
        worst, at, worst_deg, at_deg = 0.0, ("", 0), 0.0, ("", 0)
        for i in sample:
            S = pose_src(src_time(i, start))
            # The hips' place on the ground: the game's (in place).
            d = O[i]["pelvis"][0] - S["pelvis"][0]
            shift = Vector((d.x, d.y, 0.0))
            at_seam = s0 <= (i + start) % out < s1
            for n in bones:
                if (at_seam and n in below) or n in fitted:
                    continue
                e = (O[i][n][0] - S[n][0] - shift).length
                if e > worst:
                    worst, at = e, (n, i)
                a = math.degrees(O[i][n][1].rotation_difference(S[n][1]).angle)
                a = min(a, 360 - a)
                if a > worst_deg:
                    worst_deg, at_deg = a, (n, i)
        return worst, at, worst_deg, at_deg

    starts = [0] if opts.get("pingpong") else range(out)
    start = min(starts, key=lambda s: error(s, range(0, out, max(1, out // 4)))[0])
    worst, (bone, frame), deg, (dbone, dframe) = error(start, range(out + 1))
    worst_all, worst_deg = max(worst_all, worst), max(worst_deg, deg)
    flag = "" if worst < 0.001 and deg < 0.1 else "  !"
    print(f"[pack] {name:22} {file:30} {len(bones)} bones x {out + 1} frames: worst {worst * 1000:6.2f} mm ({bone}, frame {frame}), "
          f"{deg:5.2f} deg ({dbone}, frame {dframe}){flag}" + (f"; the file's open loop eased shut on {eased} over {s1 - s0} frames" if eased else "")
          + (f"; {len(fitted)} right finger joints closed on our grip" if fitted else ""))
    anims.drop_mocap(objs, acts, arm)
print(f"[pack] worst over every clip: {worst_all * 1000:.2f} mm, {worst_deg:.2f} deg")

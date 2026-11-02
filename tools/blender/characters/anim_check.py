"""Numbers, not pictures: check every clip in a variant's animation file, one text line each.

blender -b -P tools/blender/characters/anim_check.py -- male|female [clip,clip...]

For each clip, sampled frame by frame on the exported rig (assets/models/characters/<variant>_anims.glb):

  feet   the planted foot's ground speed against the clip's (a locomotion cycle plays at the
         character's speed, so a planted foot should slide back at exactly it: ratio 1.00, else the
         feet skate), how far the feet sink below or float over the ground, and how fast a planted
         foot drifts sideways (skating across the step)
  knees  bent backward (the knee behind the hip-to-ankle line)
  elbows bent the wrong way (the elbow pointing forward, ahead of the shoulder-to-wrist line)
  hands  a hand inside the torso; for gun clips, how far apart the hands are (both on the gun)
  pops   the biggest jump in any bone's turn from one frame to the next, against the median
         (a pop or a bad loop seam stands out as a large ratio)
  height the head's and hips' range (a slump, a crouch that's too deep, a pose that's flying)

Lines with a problem end in `!` and the flags; the rest end `ok`. Thresholds are rough: a flag
means look at that clip (a picture of just it), not that it's wrong.
"""

import math
import os
import sys

import bpy
from mathutils import Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
FPS = 30
# Ground speed per clip (m/s), as the game plays them (crates/shared/src/sim/movement.rs).
SPEEDS = {"walk": 1.8, "run": 5.6, "sprint": 8.6, "crouch": 1.4}
# A long gun's gaits by direction (f, b, l, r): the rifle pack's recorded speeds (anims.py RIFLE_GAIT).
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
RIFLE = {"walk": (1.35, 1.12, 1.23, 1.27), "run": (5.16, 2.72, 4.36, 4.88), "crouch": (0.93, 0.92, 1.12, 1.02), "sprint": (6.2,) * 4}
RIFLE_AIM = {"walk": (1.29, 1.19, 1.14, 1.10), "run": (3.02, 2.52, 3.50, 3.97), "crouch": (0.88, 0.84, 1.09, 0.98)}
# Ground-contact clips (the feet should touch the ground; the rest are in the air or lying down).
AIR = ("jump", "fall", "freefall", "glide", "death", "slide")
# Arms-only clips (the game lays them over the legs): the legs aren't checked.
UPPER = ("melee_", "bow_draw", "rifle_aim", "pistol_aim", "consume")


def speed_of(name):
    base = name.split("_", 1)[1] if name.startswith(("rifle_", "rifleaim_", "pistol_")) else name
    kind = base.split("_")[0]
    if base[-2:] not in ("_f", "_b", "_l", "_r"):
        return 0.0
    table = RIFLE_AIM if name.startswith("rifleaim_") else RIFLE if name.startswith("rifle_") else None
    if table:
        return table.get(kind, (0.0,) * 4)["fblr".index(base[-1])]
    return SPEEDS.get(kind, 0.0)


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    variant = args[0] if args else "male"
    only = set(args[1].split(",")) if len(args) > 1 else None
    bpy.ops.wm.read_factory_settings(use_empty=True)
    # Before the import: the importer keys at the scene's frame rate.
    bpy.context.scene.render.fps = FPS
    bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, f"assets/models/characters/{variant}_anims.glb"))
    arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    sc = bpy.context.scene
    pb = arm.pose.bones
    W = arm.matrix_world
    P = lambda n: W @ pb[n].head
    arm.animation_data_create()
    # The rest pose's ball height: the ground, as the clips were authored on it.
    arm.animation_data.action = None
    for b in pb:
        b.matrix_basis.identity()
    bpy.context.view_layer.update()
    ground = min(P("ball_l").z, P("ball_r").z)
    # The ankle over a flat foot: lower than this, the heel is in the ground.
    ankle = min(P("foot_l").z, P("foot_r").z)
    torso = ("pelvis", "spine_01", "spine_02", "spine_03", "neck_01", "head")
    print(f"[check] {variant}: {len(bpy.data.actions)} clips; ground at ball height {ground:.3f} m")
    print(f"{'clip':22s} {'frm':>3s} {'feet x':>6s} {'sink':>5s} {'float':>5s} {'skid':>4s} {'hands':>5s} {'pop':>5s} {'head':>9s} {'hips':>9s}")
    names = sorted(a.name for a in bpy.data.actions)
    for name in names:
        if only and name not in only:
            continue
        act = bpy.data.actions[name]
        arm.animation_data.action = act
        f0, f1 = map(int, act.frame_range)
        upper = name.startswith(UPPER)
        air = name.startswith(AIR)
        frames = []
        for f in range(f0, f1 + 1):
            sc.frame_set(f)
            frames.append({
                "p": {n: P(n) for n in ("pelvis", "head", "spine_03", "thigh_l", "thigh_r", "calf_l", "calf_r", "foot_l", "foot_r",
                                        "ball_l", "ball_r", "upperarm_l", "upperarm_r", "lowerarm_l", "lowerarm_r", "hand_l", "hand_r")},
                "q": {b.name: (W @ b.matrix).to_quaternion() for b in pb},
            })
        n = len(frames)
        flags = []
        # Pops: each frame's biggest bone turn (degrees), the loop's seam included.
        steps, movers = [], []
        for i in range(1, n):
            a, b = frames[i - 1]["q"], frames[i]["q"]
            # The shorter way round (q and -q are the same turn).
            turns = {k: math.degrees(min(d, 2 * math.pi - d)) for k, d in ((k, a[k].rotation_difference(b[k]).angle) for k in a)}
            worst = max(turns, key=turns.get)
            steps.append(turns[worst])
            movers.append(worst)
        med = sorted(steps)[len(steps) // 2] if steps else 0.0
        pop = max(steps) / max(med, 0.5) if steps else 0.0
        if steps and max(steps) > 25 and pop > 4:
            k = steps.index(max(steps))
            flags.append(f"pop {max(steps):.0f}deg at f{k + 1} ({movers[k]})")
        # Feet.
        speed = speed_of(name)
        feet_x = sink = flt = skid = float("nan")
        if not upper:
            sink = max(0.0, max(max(ground - fr["p"][f"ball_{s}"].z, ankle - fr["p"][f"foot_{s}"].z) for fr in frames for s in "lr"))
            flt = max(0.0, min(min(fr["p"]["ball_l"].z, fr["p"]["ball_r"].z) for fr in frames) - ground)
            if not air:
                if sink > 0.04:
                    flags.append(f"feet sink {sink * 100:.0f}cm")
                if flt > 0.06:
                    flags.append(f"feet float {flt * 100:.0f}cm")
                # Planted: the ball within 2.5 cm of its lowest, this frame and the next.
                vs, side = [], []
                for s in "lr":
                    zmin = min(fr["p"][f"ball_{s}"].z for fr in frames)
                    down = [fr["p"][f"ball_{s}"].z < zmin + 0.025 for fr in frames]
                    for i in range(n - 1):
                        if down[i] and down[i + 1]:
                            d = frames[i + 1]["p"][f"ball_{s}"] - frames[i]["p"][f"ball_{s}"]
                            vs.append(Vector((d.x, d.y)).length * FPS)
                            side.append(abs(d.x) * FPS if name.endswith(("_f", "_b")) else abs(d.y) * FPS)
                if vs:
                    v = sorted(vs)[len(vs) // 2]
                    skid = sorted(side)[len(side) // 2]
                    if speed:
                        feet_x = v / speed
                        if abs(feet_x - 1) > 0.15:
                            flags.append(f"feet skate x{feet_x:.2f}")
                    else:
                        feet_x = v
                        if v > 0.15:
                            flags.append(f"standing feet slide {v:.2f}m/s")
                    if skid > 0.35 + 0.1 * speed:
                        flags.append(f"feet skid sideways {skid:.2f}m/s")
            # Knees bent backward.
            bad = 0
            for fr in frames:
                for s in "lr":
                    h, k, a = fr["p"][f"thigh_{s}"], fr["p"][f"calf_{s}"], fr["p"][f"foot_{s}"]
                    line = (a - h).normalized()
                    off = (k - h) - line * (k - h).dot(line)
                    if off.length > 0.03 and off.normalized().dot(Vector((0, -1, 0))) < -0.6:
                        bad += 1
            if bad:
                flags.append(f"knee backward x{bad}")
        # Elbows forward, hands in the body, hands apart on a gun.
        bad, inside, apart = 0, 0, []
        for fr in frames:
            p = fr["p"]
            for s in "lr":
                sh, el, wr = p[f"upperarm_{s}"], p[f"lowerarm_{s}"], p[f"hand_{s}"]
                line = (wr - sh).normalized()
                off = (el - sh) - line * (el - sh).dot(line)
                # Only a hanging arm's (one held out has no clear front).
                if off.length > 0.04 and off.normalized().dot(Vector((0, -1, 0))) > 0.6 and line.z < -0.7:
                    bad += 1
                # The torso as a capsule from the hips to the chest, ~11 cm round.
                a, b = p["pelvis"], p["spine_03"]
                t = max(0.0, min(1.0, (wr - a).dot(b - a) / max((b - a).length_squared, 1e-6)))
                if (wr - (a + (b - a) * t)).length < 0.09:
                    inside += 1
            apart.append((p["hand_l"] - p["hand_r"]).length)
        if bad > n // 4:
            flags.append(f"elbow forward {bad}/{2 * n}")
        if inside:
            flags.append(f"hand in body x{inside}")
        hands = sorted(apart)[len(apart) // 2]
        if name.startswith(("rifle_", "pistol_")) and hands > (0.6 if name.startswith("rifle_") else 0.2):
            flags.append(f"hands apart {hands:.2f}m")
        head = [fr["p"]["head"].z - ground for fr in frames]
        hips = [fr["p"]["pelvis"].z - ground for fr in frames]
        fmt = lambda x: "  -  " if x != x else f"{x:5.2f}"
        print(f"{name:22s} {n:3d} {fmt(feet_x):>6s} {fmt(sink * 100 if sink == sink else sink):>5s} {fmt(flt * 100 if flt == flt else flt):>5s} "
              f"{fmt(skid)[1:]:>4s} {hands:5.2f} {pop:5.1f} {min(head):4.2f}-{max(head):4.2f} {min(hips):4.2f}-{max(hips):4.2f} "
              + ("! " + "; ".join(flags) if flags else "ok"))


main()

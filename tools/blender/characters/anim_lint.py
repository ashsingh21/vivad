"""Every clip's motion as numbers and events, from the built animations (build/anims_<variant>.blend,
written by anims.py), so a twist in a jump or a sliding foot is caught by reading, not by eye.
Writes build/animcheck/<variant>.json; tools/anim_report.py lays the sets side by side and calls
out where one differs from the others (`just animcompare`).

  blender -b -t 1 build/anims_male.blend -P tools/blender/characters/anim_lint.py -- male [names...]

Per frame (degrees, cm):
  twist   the shoulders turned against the hips (+ left), about up
  turn    the hips turned from where the clip starts
  head    the head turned against the chest
  lean    the torso's lean, pelvis to neck (+ forward)
  arch    the chest bent back from the waist (+ a swayback: pelvis-to-chest lean minus chest-to-neck)
  feet    each foot's height, and its drift while planted (low and still)
  pop     the most any bone turned since the last frame (a pop at a seam or in the data)
"""

import json
import math
import os
import sys

import bpy
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
variant = args[0] if args else "male"
names = args[1:]

arm = bpy.data.objects["Human.rig"]
W = arm.matrix_world
FWD = Vector((0, -1, 0))
# Clips that loop: their last frame should come back to their first.
LOOPS = ("idle", "run", "walk", "crouch_", "sprint", "glide", "freefall", "guard")
# A planted foot: within this of its lowest in the clip, and moving less than this a frame.
PLANT_H, PLANT_V = 0.04, 0.012


def yaw(v):
    return math.degrees(math.atan2(v.y, v.x))


def wrap(a):
    return (a + 180.0) % 360.0 - 180.0


out = {}
for act in sorted(bpy.data.actions, key=lambda a: a.name):
    name = act.name.split("|")[-1]
    if names and not any(n in name for n in names):
        continue
    arm.animation_data.action = act
    f0, f1 = map(int, act.frame_range)
    bones = arm.pose.bones
    try:
        need = [bones[n] for n in ("pelvis", "thigh_l", "thigh_r", "upperarm_l", "upperarm_r", "spine_03", "neck_01", "head", "foot_l", "foot_r")]
    except KeyError:
        continue
    frames = []
    prev = None
    for f in range(f0, f1 + 1):
        bpy.context.scene.frame_set(f)
        h = lambda n: W @ bones[n].head
        hips = h("thigh_r") - h("thigh_l")
        shoulders = h("upperarm_r") - h("upperarm_l")
        # The head's side-to-side axis (its bone's x) against the shoulders' line.
        across = (W @ bones["head"].matrix).to_3x3() @ Vector((1, 0, 0))
        body = h("neck_01") - h("pelvis")
        lean = math.degrees(math.atan2(body.dot(FWD), body.z))
        pitch = lambda v: math.degrees(math.atan2(v.dot(FWD), v.z))
        arch = pitch(h("spine_03") - h("pelvis")) - pitch(h("neck_01") - h("spine_03"))
        rots = {b.name: b.matrix_basis.to_quaternion() for b in bones}
        pop, pop_bone = 0.0, ""
        if prev:
            for n, q in rots.items():
                d = math.degrees(q.rotation_difference(prev[n]).angle)
                d = min(d, 360 - d)
                if d > pop:
                    pop, pop_bone = d, n
        prev = rots
        frames.append({
            "twist": wrap(yaw(shoulders) - yaw(hips)),
            "hips": yaw(hips),
            "head": (lambda t: t - math.copysign(180.0, t) if abs(t) > 90 else t)(wrap(yaw(Vector((across.x, across.y, 0))) - yaw(shoulders))),
            "lean": lean,
            "arch": arch,
            "feet": [[c for c in h("foot_l")], [c for c in h("foot_r")]],
            "pop": pop,
            "pop_bone": pop_bone,
        })
    if len(frames) < 2:
        continue
    turn0 = frames[0]["hips"]
    for fr in frames:
        fr["turn"] = wrap(fr["hips"] - turn0)
    # Feet: drift while planted, and the lowest they go.
    feet = []
    for side in (0, 1):
        zs = [fr["feet"][side][2] for fr in frames]
        low = min(zs)
        slide, run = 0.0, None
        for i in range(1, len(frames)):
            p, q = Vector(frames[i - 1]["feet"][side]), Vector(frames[i]["feet"][side])
            planted = q.z < low + PLANT_H and (q - p).length < PLANT_V
            if planted:
                run = run or p
                slide = max(slide, (Vector((q.x, q.y)) - Vector((run.x, run.y))).length)
            else:
                run = None
        feet.append({"low": low, "slide": slide})
    loop = any(k in name for k in LOOPS)
    seam = 0.0
    if loop:
        bpy.context.scene.frame_set(f0)
        a = {b.name: b.matrix_basis.to_quaternion() for b in bones}
        bpy.context.scene.frame_set(f1)
        for b in bones:
            d = math.degrees(b.matrix_basis.to_quaternion().rotation_difference(a[b.name]).angle)
            seam = max(seam, min(d, 360 - d))
    out[name] = {
        "frames": len(frames),
        "loop": loop,
        "seam": seam,
        "twist": [round(fr["twist"], 1) for fr in frames],
        "turn": [round(fr["turn"], 1) for fr in frames],
        "head": [round(fr["head"], 1) for fr in frames],
        "lean": [round(fr["lean"], 1) for fr in frames],
        "arch": [round(fr["arch"], 1) for fr in frames],
        "pop": [round(fr["pop"], 1) for fr in frames],
        "pop_bone": [fr["pop_bone"] for fr in frames],
        "feet": feet,
    }

dest = os.path.join(os.path.dirname(bpy.data.filepath), "animcheck")
os.makedirs(dest, exist_ok=True)
path = os.path.join(dest, f"{variant}.json")
json.dump(out, open(path, "w"))
print(f"[animcheck] {variant}: {len(out)} clips -> {path}")

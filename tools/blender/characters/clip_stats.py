"""Each clip's body by the numbers, from the built animations (build/anims_<variant>.blend, written
by anims.py) or a mocap source: how far the hips bob, sway, twist and tilt, how the torso leans
and arches. Cheap to read, to catch a bouncy run or a swayback before it's in the game.

  blender -b -t 1 build/anims_male.blend -P tools/blender/characters/clip_stats.py -- [names...]
  blender -b -t 1 --factory-startup -P tools/blender/characters/clip_stats.py -- --source ual [names...]

Columns (cm and degrees, 5th to 95th percentile over the clip):
  bob    the pelvis up and down        sway   the pelvis side to side
  twist  the hips turned (about up)    roll   the hips tilted (one side higher)
  pitch  the torso's lean (pelvis to neck, + forward)
  arch   the chest bent back from the waist (+ a swayback; `just gripcheck`'s measure)
"""

import math
import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
source = args[args.index("--source") + 1] if "--source" in args else None
names = [a for i, a in enumerate(args) if a != "--source" and (i == 0 or args[i - 1] != "--source")]

if source:
    import anims
    arm, _, acts = anims.load_mocap(source)
    bones = lambda n: arm.pose.bones.get(n)
else:
    arm = bpy.data.objects["Human.rig"]
    acts = list(bpy.data.actions)
    bones = lambda n: arm.pose.bones.get(n)

W = arm.matrix_world
FWD, UP = Vector((0, -1, 0)), Vector((0, 0, 1))


def pct(v, p):
    v = sorted(v)
    return v[min(len(v) - 1, max(0, int(round(p / 100 * (len(v) - 1)))))]


def span(v):
    return f"{pct(v, 5):+4.0f}..{pct(v, 95):+4.0f}"


print(f"[clip] {'clip':28} {'frm':>4} {'bob':>5} {'sway':>5} {'twist':>11} {'roll':>11} {'pitch':>11} {'arch':>11}")
for act in sorted(acts, key=lambda a: a.name):
    name = act.name.split("|")[-1]
    if names and not any(n in name for n in names):
        continue
    arm.animation_data.action = act
    f0, f1 = map(int, act.frame_range)
    rows = []
    for f in range(f0, f1 + 1):
        bpy.context.scene.frame_set(f)
        h = lambda n: W @ bones(n).head
        try:
            pel, tl, tr, s3, nk = h("pelvis"), h("thigh_l"), h("thigh_r"), h("spine_03"), h("neck_01")
        except AttributeError:
            break
        hip = tr - tl
        pitch = lambda v: math.degrees(math.atan2(v.dot(FWD), v.z))
        rows.append((pel.z, pel.x, math.degrees(math.atan2(hip.y, -hip.x)), math.degrees(math.atan2(hip.z, Vector((hip.x, hip.y)).length)),
                     pitch(nk - pel), pitch(s3 - pel) - pitch(nk - s3)))
    if len(rows) < 2:
        continue
    col = lambda i: [r[i] for r in rows]
    print(f"[clip] {name:28} {len(rows):4} {100 * (pct(col(0), 95) - pct(col(0), 5)):5.1f} {100 * (pct(col(1), 95) - pct(col(1), 5)):5.1f} "
          f"{span(col(2)):>11} {span(col(3)):>11} {span(col(4)):>11} {span(col(5)):>11}")

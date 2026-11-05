"""Dump our rig's bone frames (each bone's direction and roll axis at rest, armature space) from a
built character to tools/blender/characters/rig_frames.json (and its joints' places, for the palms' facing): what vroid.py lines a VRoid model's
bones up to, so our clips' and arms' bone-local turns mean the same on it.

blender -b build/char_female_final.blend -P tools/blender/characters/rig_frames.py
"""

import json
import os

import bpy

arm = bpy.data.objects["Human.rig"]
out = {}
for b in arm.data.bones:
    m = b.matrix_local.to_3x3()
    out[b.name] = {"y": [round(v, 5) for v in m.col[1]], "z": [round(v, 5) for v in m.col[2]], "head": [round(v, 5) for v in b.head_local],
                   "length": round(b.length, 5)}
path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "rig_frames.json")
json.dump(out, open(path, "w"), indent=1, sort_keys=True)
print(f"[rig_frames] {len(out)} bones -> {path}")

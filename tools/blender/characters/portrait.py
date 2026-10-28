"""Portrait renders of the exported characters (the GLB the game loads, LOD0 and the hair, brow and lash cards): front and
three-quarter head shots, to judge faces.

blender -b --factory-startup -P tools/blender/characters/portrait.py -- <out_dir> [male female] [samples]
"""

import math
import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
import bake  # noqa: E402

sys.path.insert(0, os.path.join(HERE, ".."))
from render_preview import setup_world  # noqa: E402


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    out = args[0]
    variants = [a for a in args[1:] if a in ("male", "female")] or ["male", "female"]
    samples = next((int(a) for a in args[1:] if a.isdigit()), 64)
    os.makedirs(out, exist_ok=True)
    for v in variants:
        bpy.ops.wm.read_homefile(use_empty=True)
        bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, "assets", "models", "characters", f"{v}.glb"))
        for o in list(bpy.data.objects):
            if o.type == "MESH" and ("LOD1" in o.name or "LOD2" in o.name):
                bpy.data.objects.remove(o)
        arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
        head = arm.matrix_world @ arm.pose.bones["head"].head
        # Face centre: a little above and in front of the head joint (the GLB faces +Y here).
        meshes = [o for o in bpy.data.objects if o.type == "MESH"]
        top = max((o.matrix_world @ Vector(c)).z for o in meshes for c in o.bound_box)
        fwd = 1.0 if max((o.matrix_world @ Vector(c)).y for o in meshes for c in o.bound_box) > -min((o.matrix_world @ Vector(c)).y for o in meshes for c in o.bound_box) else -1.0
        centre = head + Vector((0, 0.06 * fwd, 0.1))
        sc = bpy.context.scene
        bake.setup_cycles()
        sc.cycles.samples = samples
        sc.render.resolution_x = sc.render.resolution_y = 640
        sc.view_settings.view_transform = "AgX"
        setup_world()
        # Afternoon key from the side, like the game's sun.
        sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
        sun.data.energy = 3.0
        sun.rotation_euler = Vector((-1.0 * fwd, -0.6, 0.6)).to_track_quat("-Z", "Y").to_euler()
        sc.collection.objects.link(sun)
        cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
        cam.data.lens = 85
        sc.collection.objects.link(cam)
        sc.camera = cam
        for name, yaw in (("front", 0.0), ("three_q", 0.6)):
            d = Vector((math.sin(yaw), fwd * math.cos(yaw), 0.0))
            cam.location = centre + d * 0.85
            cam.rotation_euler = (centre - cam.location).to_track_quat("-Z", "Y").to_euler()
            sc.render.filepath = os.path.join(out, f"{v}_{name}.png")
            bpy.ops.render.render(write_still=True)
            print(f"[portrait] {sc.render.filepath}")
        del top


main()

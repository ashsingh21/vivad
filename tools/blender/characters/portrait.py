"""Portrait renders of the exported characters (the GLB the game loads, LOD0 and the hair, brow and lash cards): front,
three-quarter and close head shots, to judge faces, and a full-body shot.

blender -b --factory-startup -P tools/blender/characters/portrait.py -- <out_dir> [male female] [samples] [parts] [lod=<k>]

`parts` renders <variant>_parts.glb instead of the old one-mesh <variant>.glb; lod=<k> shows that LOD.
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
    suffix = "_parts" if "parts" in args[1:] else ""
    lod = next((a[4:] for a in args[1:] if a.startswith("lod=")), "0")
    os.makedirs(out, exist_ok=True)
    for v in variants:
        bpy.ops.wm.read_homefile(use_empty=True)
        bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, "assets", "models", "characters", f"{v}{suffix}.glb"))
        for o in list(bpy.data.objects):
            if o.type == "MESH" and "LOD" in o.name and not o.name.endswith(f"LOD{lod}"):
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
        waist = Vector((0, 0, top / 2))
        shots = (("front", 0.0, centre, 0.85), ("three_q", 0.6, centre, 0.85), ("close", 0.3, centre - Vector((0, 0, 0.02)), 0.36), ("full", 0.6, waist, 4.6))
        for name, yaw, at, away in shots:
            d = Vector((math.sin(yaw), fwd * math.cos(yaw), 0.0))
            cam.location = at + d * away
            cam.rotation_euler = (at - cam.location).to_track_quat("-Z", "Y").to_euler()
            sc.render.filepath = os.path.join(out, f"{v}_{name}.png")
            bpy.ops.render.render(write_still=True)
            print(f"[portrait] {sc.render.filepath}")


main()

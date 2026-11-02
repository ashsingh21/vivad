"""Turntable review renders of a model from several angles, in its own materials.

blender -b --factory-startup -P tools/blender/orbit.py -- <file.glb> <out_dir> [samples]
Writes front three-quarter, side, rear three-quarter, front, back and high views; the model's
front is +Y in Blender (-Z in the game).
"""

import math
import os
import sys

import bpy
from mathutils import Vector


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    path, out = argv[0], argv[1]
    samples = int(argv[2]) if len(argv) > 2 else 32
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=path)
    scene = bpy.context.scene
    # The nearest LOD only.
    for o in list(scene.objects):
        if o.type == "MESH" and "LOD" in o.name and "LOD0" not in o.name:
            bpy.data.objects.remove(o, do_unlink=True)
    pts = [o.matrix_world @ v.co for o in scene.objects if o.type == "MESH" for v in o.data.vertices]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    centre, size = (lo + hi) / 2, (hi - lo).length
    bpy.ops.mesh.primitive_plane_add(size=size * 20, location=(centre.x, centre.y, lo.z))
    g = bpy.data.materials.new("ground")
    g.use_nodes = True
    g.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.42, 0.40, 0.37, 1)
    bpy.context.active_object.data.materials.append(g)
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 4.0
    sun.data.angle = math.radians(8)
    sun.rotation_euler = (math.radians(50), 0, math.radians(-35))
    scene.collection.objects.link(sun)
    world = bpy.data.worlds.new("w")
    world.color = (0.55, 0.62, 0.75)
    scene.world = world
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.lens = 50
    scene.collection.objects.link(cam)
    scene.camera = cam
    scene.render.engine = "CYCLES"
    scene.cycles.samples = samples
    scene.render.resolution_x, scene.render.resolution_y = 900, 640
    scene.view_settings.view_transform = "AgX"
    views = [("1_front34", 35, 15), ("2_side", 90, 8), ("3_rear34", 145, 15), ("4_front", 0, 10), ("5_back", 180, 10), ("6_high", 50, 45)]
    os.makedirs(out, exist_ok=True)
    for name, az, el in views:
        a, e = math.radians(az), math.radians(el)
        d = size * 1.45
        cam.location = centre + Vector((d * math.cos(e) * math.sin(a), d * math.cos(e) * math.cos(a), d * math.sin(e)))
        cam.rotation_euler = (centre - cam.location).to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = os.path.join(out, f"{name}.png")
        bpy.ops.render.render(write_still=True)
    print(f"[orbit] {out}")


main()

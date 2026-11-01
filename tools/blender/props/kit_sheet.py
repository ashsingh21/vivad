"""Review render of street-kit pieces side by side, three-quarter view, in their vertex colours.

blender -b --factory-startup -P tools/blender/props/kit_sheet.py -- <out.png> [piece ...] [--side]
(no pieces: all of them; --side: a side view instead)
"""

import math
import os
import sys

import bpy
from mathutils import Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
KIT = os.path.join(ROOT, "assets", "models", "props", "street_kit.glb")


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    side = "--side" in argv
    argv = [a for a in argv if a != "--side"]
    out, names = argv[0], argv[1:]
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=KIT)
    scene = bpy.context.scene
    keep = [o for o in scene.objects if o.type == "MESH" and o.name.endswith("_LOD0") and (not names or o.name[:-5] in names)]
    for o in list(scene.objects):
        if o not in keep:
            bpy.data.objects.remove(o, do_unlink=True)
    keep.sort(key=lambda o: names.index(o.name[:-5]) if names else o.name)
    mat = bpy.data.materials.new("vc")
    mat.use_nodes = True
    nt = mat.node_tree
    attr = nt.nodes.new("ShaderNodeVertexColor")
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.6
    nt.links.new(attr.outputs["Color"], bsdf.inputs["Base Color"])
    x = 0.0
    for o in keep:
        o.data.materials.clear()
        o.data.materials.append(mat)
        o.rotation_euler = (0, 0, 0)
        w = max(v.co.x for v in o.data.vertices) - min(v.co.x for v in o.data.vertices)
        d = max(v.co.y for v in o.data.vertices) - min(v.co.y for v in o.data.vertices)
        span = (d if side else max(w, d)) + 0.6
        o.location = (0, x + span / 2, 0) if side else (x + span / 2, 0, 0)
        x += span
    bpy.ops.mesh.primitive_plane_add(size=200)
    ground = bpy.context.active_object
    gm = bpy.data.materials.new("ground")
    gm.use_nodes = True
    gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.5, 0.5, 0.5, 1)
    ground.data.materials.append(gm)
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 4.0
    sun.rotation_euler = (math.radians(45), 0, math.radians(35))
    scene.collection.objects.link(sun)
    world = bpy.data.worlds.new("w")
    world.color = (0.55, 0.6, 0.7)
    scene.world = world
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = x * 1.05
    centre = Vector((0, x / 2, 0.6)) if side else Vector((x / 2, 0, 0.6))
    view = Vector((-1, 0, 0.12)) if side else Vector((-0.55, -1, 0.55))
    cam.location = centre + view.normalized() * 20
    cam.rotation_euler = (-view).to_track_quat("-Z", "Y").to_euler()
    scene.collection.objects.link(cam)
    scene.camera = cam
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 24
    top = max((o.matrix_world @ v.co).z for o in keep for v in o.data.vertices)
    if side:
        # Fit the tallest piece: the frame as high as the tallest top plus a margin.
        centre.z = top / 2
        cam.location = centre + view.normalized() * 20
        scene.render.resolution_x, scene.render.resolution_y = 1800, max(200, int(1800 * (top + 0.5) / (x * 1.05)))
    else:
        scene.render.resolution_x, scene.render.resolution_y = 1800, max(300, int(1800 * 2.2 / x * 0.7))
    scene.render.filepath = out
    bpy.ops.render.render(write_still=True)
    print(f"[kit_sheet] {out}")


main()

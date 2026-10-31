"""Review renders for props/trees: side, three-quarter, low hero angle, top. Framing scales to bounds.

blender -b -P tools/blender/render_prop.py -- <file.glb> <out_dir> [samples] [air]

`air` is for things that fly (the vimana): no ground, and the views a player gets of it: riding it
(behind and above), from the ground below, and broadside.
"""

import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "lib"))
import bake  # noqa: E402
from render_character import frame_bounds  # noqa: E402
from render_preview import camera, setup_world  # noqa: E402


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    src, out_dir = argv[0], argv[1]
    samples = int(argv[2]) if len(argv) > 2 else 48
    air = len(argv) > 3 and argv[3] == "air"
    name = os.path.splitext(os.path.basename(src))[0]
    os.makedirs(out_dir, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=src)
    scene = bpy.context.scene
    meshes = [o for o in scene.objects if o.type == "MESH" and "LOD0" in o.name]
    for o in scene.objects:
        if o.type == "MESH" and o not in meshes:
            o.hide_render = True
    bake.setup_cycles(samples)
    scene.cycles.use_denoising = True
    scene.view_settings.view_transform = "AgX"
    setup_world()
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 3.5
    sun.angle = 0.05
    so = bpy.data.objects.new("sun", sun)
    so.rotation_euler = (0.9, 0.2, 0.8)
    scene.collection.objects.link(so)
    lo, hi = frame_bounds(meshes)
    size = max(hi.x - lo.x, hi.y - lo.y, hi.z - lo.z)
    mid = (lo + hi) / 2
    d = size * 1.6
    up = Vector((0, 0, 1))
    if not air:
        bpy.ops.mesh.primitive_plane_add(size=size * 3, location=(0, 0, 0))
        ground = bpy.context.active_object
        gm = bpy.data.materials.new("ground")
        gm.use_nodes = True
        gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.30, 0.27, 0.20, 1)
        ground.data.materials.append(gm)
    views = {
        "1_side": camera("side", mid + Vector((d, 0, 0)), mid, up, lens=50),
        "2_ride": camera("ride", Vector((8.0, -24.0, 13.0)), Vector((0, 0, 4.0)), up, lens=35),
        "3_below": camera("below", Vector((14.0, 20.0, -30.0)), Vector((0, 0, -2.0)), up, lens=50),
        "4_bow": camera("bow", Vector((-9.0, 22.0, 3.0)), Vector((0, 4.0, 3.0)), up, lens=35),
    } if air else {
        "1_side": camera("side", mid + Vector((0, -d, 0)), mid, up, lens=50),
        "2_three_quarter": camera("tq", mid + Vector((d * 0.75, -d * 0.75, d * 0.25)), mid, up, lens=50),
        "3_hero_low": camera("low", Vector((d * 0.35, -d * 0.45, 1.6)), mid + Vector((0, 0, size * 0.15)), up, lens=28),
        "4_canopy": camera("canopy", mid + Vector((size * 0.25, -size * 0.55, size * 0.25)), Vector((0, 0, hi.z * 0.8)), up, lens=50),
    }
    for view, cam in views.items():
        scene.camera = cam
        scene.render.resolution_x, scene.render.resolution_y = (1200, 1200)
        scene.render.filepath = os.path.join(out_dir, f"{name}_{view}.png")
        bpy.ops.render.render(write_still=True)


if __name__ == "__main__":
    main()

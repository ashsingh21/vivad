"""Review renders for characters: front, three-quarter, side, back, face and hands close-ups.

blender -b -P tools/blender/render_character.py -- <file.glb|file.blend> <out_dir> [samples]
"""

import math
import os
import sys

import bpy
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "lib"))
import bake  # noqa: E402
from render_preview import setup_world, camera  # noqa: E402


def frame_bounds(objs):
    pts = [o.matrix_world @ Vector(c) for o in objs for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return lo, hi


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    src, out_dir = argv[0], argv[1]
    samples = int(argv[2]) if len(argv) > 2 else 48
    name = os.path.splitext(os.path.basename(src))[0]
    os.makedirs(out_dir, exist_ok=True)
    if src.endswith(".blend"):
        bpy.ops.wm.open_mainfile(filepath=src)
    else:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.gltf(filepath=src)
    scene = bpy.context.scene
    # Only LOD0 / visible meshes count.
    meshes = [o for o in scene.objects if o.type == "MESH" and not o.hide_render and "LOD1" not in o.name and "LOD2" not in o.name]
    for o in scene.objects:
        if o.type == "MESH" and ("LOD1" in o.name or "LOD2" in o.name):
            o.hide_render = True
    for o in list(scene.objects):
        if o.type in {"CAMERA", "LIGHT"}:
            bpy.data.objects.remove(o)

    bake.setup_cycles(samples)
    scene.cycles.use_denoising = True
    scene.view_settings.view_transform = "AgX"
    scene.view_settings.look = "AgX - Medium High Contrast"
    setup_world()
    # Key + rim lights shaped like a simple portrait setup.
    for nm, loc, energy, size in (("key", (1.6, -2.2, 2.4), 260, 1.6), ("rim", (-1.8, 2.0, 2.2), 180, 1.0), ("fill", (-2.4, -1.6, 1.2), 60, 2.5)):
        d = bpy.data.lights.new(nm, "AREA")
        d.energy, d.size = energy, size
        ob = bpy.data.objects.new(nm, d)
        ob.location = loc
        ob.rotation_euler = (Vector((0, 0, 1.0)) - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
        scene.collection.objects.link(ob)
    # Floor disc for grounding.
    bpy.ops.mesh.primitive_circle_add(vertices=64, radius=1.6, fill_type="NGON", location=(0, 0, 0))
    floor = bpy.context.active_object
    fm = bpy.data.materials.new("floor")
    fm.diffuse_color = (0.2, 0.2, 0.2, 1)
    fm.use_nodes = True
    fm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.16, 0.16, 0.17, 1)
    fm.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.7
    floor.data.materials.append(fm)

    lo, hi = frame_bounds(meshes)
    h = hi.z - lo.z
    mid = Vector((0, 0, lo.z + h * 0.52))
    head = Vector((0, 0, lo.z + h * 0.93))
    up = Vector((0, 0, 1))
    full, sq = (1000, 1400), (1100, 1100)
    # MakeHuman faces -Y in Blender (glTF +Z).
    views = {
        "1_front": camera("front", mid + Vector((0, -5.2, 0.1)), mid, up, lens=70),
        "2_three_quarter": camera("tq", mid + Vector((3.4, -3.9, 0.4)), mid, up, lens=70),
        "3_side": camera("side", mid + Vector((5.2, 0, 0.1)), mid, up, lens=70),
        "4_back": camera("back", mid + Vector((-1.2, 5.1, 0.3)), mid, up, lens=70),
        "5_face": camera("face", head + Vector((0.45, -0.95, 0.02)), head, up, lens=85),
        "6_torso": camera("torso", Vector((0.5, -1.7, lo.z + h * 0.62)), Vector((0, 0, lo.z + h * 0.6)), up, lens=60),
    }
    for view, cam in views.items():
        scene.camera = cam
        res = sq if view[0] in "56" else full
        scene.render.resolution_x, scene.render.resolution_y = res
        scene.render.filepath = os.path.join(out_dir, f"{name}_{view}.png")
        bpy.ops.render.render(write_still=True)


if __name__ == "__main__":
    main()

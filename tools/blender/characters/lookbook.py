"""Front, side and back of a split character in one of the game's own clips, with the same camera
and light every time: for judging a look, and for today-against-new sheets.

blender -b --factory-startup -P tools/blender/characters/lookbook.py -- <out_dir> <tag> <parts.glb> <anims.glb> [samples] [clip:t] [lod=<k>]

Writes <out_dir>/<tag>_{front,side,back,head,head_side}.png.
"""

import math
import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, os.path.join(HERE, ".."))
import bake  # noqa: E402
from render_preview import setup_world  # noqa: E402

# The characters face -Y here, their left is +X.
FULL = (("front", 0.0), ("side", math.pi / 2), ("back", math.pi))
HEAD = (("head", 0.0), ("head_side", 1.1))


def load(parts, anims, lod):
    """The character's meshes on LOD `lod`, driven by the rig of the clips file."""
    bpy.ops.wm.read_homefile(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=parts)
    for o in list(bpy.data.objects):
        if o.type == "MESH" and "LOD" in o.name and not o.name.endswith(f"LOD{lod}"):
            bpy.data.objects.remove(o)
    rig = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=anims)
    arm = next(o for o in set(bpy.data.objects) - before if o.type == "ARMATURE")
    for o in meshes:
        for m in o.modifiers:
            if m.type == "ARMATURE":
                m.object = arm
    rig.hide_render = True
    return arm


def pose(arm, clip, t):
    act = bpy.data.actions.get(clip) or next(a for a in bpy.data.actions if a.name.startswith(clip))
    ad = arm.animation_data or arm.animation_data_create()
    for tr in list(ad.nla_tracks):
        ad.nla_tracks.remove(tr)
    ad.action = act
    if hasattr(ad, "action_slot") and len(act.slots):
        ad.action_slot = act.slots[0]
    a, b = act.frame_range
    bpy.context.scene.frame_set(int(a + (b - a) * t))
    bpy.context.view_layer.update()


def stage(samples):
    sc = bpy.context.scene
    bake.setup_cycles()
    sc.cycles.samples = samples
    sc.cycles.use_denoising = True
    sc.view_settings.view_transform = "AgX"
    sc.view_settings.look = "AgX - Medium High Contrast"
    setup_world()
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.2
    sun.data.angle = math.radians(2)
    sun.rotation_euler = Vector((0.75, -0.8, 1.0)).to_track_quat("Z", "Y").to_euler()
    sc.collection.objects.link(sun)
    bpy.ops.mesh.primitive_plane_add(size=120, location=(0, 0, 0))
    ground = bpy.context.active_object
    m = bpy.data.materials.new("ground")
    m.use_nodes = True
    m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.32, 0.31, 0.29, 1)
    m.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.8
    ground.data.materials.append(m)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.sensor_fit = "VERTICAL"
    cam.data.sensor_height = 24
    sc.collection.objects.link(cam)
    sc.camera = cam
    return sc, cam


def shoot(sc, cam, at, yaw, away, path):
    cam.location = at + Vector((math.sin(yaw), -math.cos(yaw), 0.03)) * away
    cam.rotation_euler = (at - cam.location).to_track_quat("-Z", "Y").to_euler()
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print(f"[lookbook] {path}")


def main():
    a = sys.argv[sys.argv.index("--") + 1:]
    out, tag, parts, anims = a[:4]
    rest = a[4:]
    samples = next((int(x) for x in rest if x.isdigit()), 64)
    clip, t = next((x.split(":") for x in rest if ":" in x), ("idle", "0.25"))
    lod = next((x[4:] for x in rest if x.startswith("lod=")), "0")
    os.makedirs(out, exist_ok=True)
    arm = load(parts, anims, lod)
    sc, cam = stage(samples)
    pose(arm, clip, float(t))
    cam.data.lens = 60
    sc.render.resolution_x, sc.render.resolution_y = 720, 1000
    for name, yaw in FULL:
        shoot(sc, cam, Vector((0, 0, 0.93)), yaw, 5.0, os.path.join(out, f"{tag}_{name}.png"))
    head = arm.matrix_world @ arm.pose.bones["head"].head
    cam.data.lens = 85
    sc.render.resolution_x, sc.render.resolution_y = 640, 640
    for name, yaw in HEAD:
        shoot(sc, cam, head + Vector((0, -0.02, 0.07)), yaw, 0.62 * 85 / 50, os.path.join(out, f"{tag}_{name}.png"))


main()

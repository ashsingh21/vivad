"""Quick face iteration: the MPFB body with its face targets, eyes, brows, lashes and hair, the
game's skin and card materials, no garments and no bake. Renders front and three-quarter head
shots in about a minute per variant.

blender -b -P tools/blender/characters/face_lab.py -- <out_dir> male|female [hair=ponytail01] [off | x2.5 (scale the targets)] [samples]

`off` renders without the face targets, for comparison.
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, os.path.join(HERE, ".."))

import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import bake  # noqa: E402
import face  # noqa: E402
import finish  # noqa: E402
import garments  # noqa: E402
import human  # noqa: E402
from render_preview import setup_world  # noqa: E402


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    out, variant = args[0], args[1]
    cfg = dict(human.VARIANTS[variant])
    for a in args[2:]:
        if a.startswith("hair="):
            cfg["hair"] = f"hair/{a[5:]}/{a[5:]}.mhclo"
        if a == "off":
            cfg["face"] = {}
        if a.startswith("x"):
            cfg["face"] = {k: min(1.0, w * float(a[1:])) for k, w in cfg["face"].items()}
    samples = next((int(a) for a in args[2:] if a.isdigit()), 32)
    tag = next((a for a in args[2:] if a == "off" or a.startswith("x") or a.startswith("hair=")), "on").replace("=", "_")
    human.clear_scene()
    body, rig, parts = human.build_body(cfg)
    body.hide_render = True
    proxy = parts["proxy"]
    ctx = garments.BodyContext(proxy, rig, human.bone_weight_map(proxy))
    for o, _ in face.decorate(variant, proxy, parts, ctx):
        o.data.materials.append(finish.hair_mesh_material(o.name))
    skin_img = finish.find_image(proxy)
    for o in [proxy, parts["eyes"]]:
        finish.prepare_uvs(o)
    marks = {"beard": "beard" in proxy.data.attributes, "eyes": proxy.get("eyes"), "bindi": proxy.get("bindi")}
    proxy.data.materials.clear()
    proxy.data.materials.append(finish.skin_material(variant, skin_img, marks))
    eye_img = finish.find_image(parts["eyes"])
    parts["eyes"].data.materials.clear()
    parts["eyes"].data.materials.append(finish.eye_material("eyes", eye_img))
    for key, tint, strength in finish.CARDS:
        o = parts.get(key)
        if o is not None:
            img = finish.find_image(o)
            o.data.materials.clear()
            o.data.materials.append(finish.alpha_card_material(key, img, tint, strength))

    head = rig.matrix_world @ rig.pose.bones["head"].head
    centre = head + Vector((0, -0.06, 0.1))
    sc = bpy.context.scene
    bake.setup_cycles()
    sc.cycles.samples = samples
    sc.render.resolution_x = sc.render.resolution_y = 520
    sc.view_settings.view_transform = "AgX"
    setup_world()
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.0
    sun.rotation_euler = Vector((1.0, 0.6, 0.6)).to_track_quat("-Z", "Y").to_euler()
    sc.collection.objects.link(sun)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.lens = 85
    sc.collection.objects.link(cam)
    sc.camera = cam
    os.makedirs(out, exist_ok=True)
    for name, yaw in (("front", 0.0), ("three_q", 0.6)):
        cam.location = centre + Vector((math.sin(yaw), -math.cos(yaw), 0.0)) * 0.85
        cam.rotation_euler = (centre - cam.location).to_track_quat("-Z", "Y").to_euler()
        sc.render.filepath = os.path.join(out, f"{variant}_{tag}_{name}.png")
        bpy.ops.render.render(write_still=True)
        print(f"[face_lab] {sc.render.filepath}")


main()

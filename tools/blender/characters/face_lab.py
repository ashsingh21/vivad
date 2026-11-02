"""Quick face iteration: the MPFB body with its face targets, split and subdivided head, high-poly
eyes, brows, lashes and hair, the game's skin and card materials, no garments and no bake. Renders
front, three-quarter and close-eye shots in about a minute per variant.

blender -b -P tools/blender/characters/face_lab.py -- <out_dir> male|female [option...] [samples]

  look=<k>       the look's person (k % 8, the game's order) instead of the variant
  hair=<style>   an MPFB hair style, e.g. ponytail01
  age=<0..1>     the MakeHuman age macro (0.5 is 25 years), which also ages the skin's creases
  eyes=<colour>  an MPFB eye material, e.g. brownlight
  skin=<name>    an MPFB skin, e.g. middleage_asian_male
  subdiv=0       the head as the proxy has it
  off            without the face targets, for comparison
  x2.5           the face targets scaled
  --sheet        also lay the shots out in one <variant>_<tag>_sheet.png
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, os.path.join(HERE, ".."))

import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector  # noqa: E402

import bake  # noqa: E402
import face  # noqa: E402
import finish  # noqa: E402
import garments  # noqa: E402
import head as head_build  # noqa: E402
import human  # noqa: E402
from render_preview import setup_world  # noqa: E402


def option(args, name):
    return next((a[len(name) + 1:] for a in args if a.startswith(f"{name}=")), None)


def sheet(paths, out):
    """The renders side by side, without leaving Blender's Python."""
    images = [bpy.data.images.load(p) for p in paths]
    w, h = images[0].size
    joined = bpy.data.images.new("sheet", w * len(images), h)
    px = np.zeros((h, w * len(images), 4), dtype=np.float32)
    for i, img in enumerate(images):
        a = np.empty(w * h * 4, dtype=np.float32)
        img.pixels.foreach_get(a)
        px[:, i * w:(i + 1) * w] = a.reshape(h, w, 4)
    joined.pixels.foreach_set(px.ravel())
    joined.filepath_raw = out
    joined.file_format = "PNG"
    joined.save()
    print(f"[face_lab] {out}")


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    out, variant = args[0], args[1]
    opts = args[2:]
    if option(opts, "look") is not None:
        variant = list(human.VARIANTS)[int(option(opts, "look")) % 8]
    cfg = dict(human.VARIANTS[variant])
    for a in opts:
        if a == "off":
            cfg["face"] = {}
        if a[0] == "x" and a[1:].replace(".", "").isdigit():
            cfg["face"] = {k: min(1.0, w * float(a[1:])) for k, w in cfg["face"].items()}
    if option(opts, "hair"):
        cfg["hair"] = f"hair/{option(opts, 'hair')}/{option(opts, 'hair')}.mhclo"
    if option(opts, "skin"):
        cfg["skin"] = f"skins/{option(opts, 'skin')}/{option(opts, 'skin')}.mhmat"
    age = float(option(opts, "age") or cfg.get("flesh", {}).get("age", cfg["macro"]["age"]))
    cfg["flesh"] = {**cfg.get("flesh", {}), "age": age}
    samples = next((int(a) for a in opts if a.isdigit()), 32)
    tag = "_".join(a.replace("=", "_") for a in opts if not a.isdigit() and not a.startswith("-")) or "on"
    human.clear_scene()
    body, rig, parts = human.build_body(cfg)
    proxy = parts["proxy"]
    ctx = garments.BodyContext(proxy, rig, human.bone_weight_map(proxy))
    ctx.brow_top = max((parts["eyebrows"].matrix_world @ v.co).z for v in parts["eyebrows"].data.vertices)
    face.decorate(cfg, proxy, parts, body, ctx)
    body.hide_render = True
    skin_img = finish.find_image(proxy)
    eye_img = finish.find_image(parts["eyes"])
    if option(opts, "eyes"):
        eye_img = bpy.data.images.load(os.path.join(human.MPFB_DATA, "eyes", "materials", f"{option(opts, 'eyes')}_eye.png"))
    eyes, cornea = head_build.eye_parts(parts["eyes"], "cornea")
    cornea.data.materials.clear()
    cornea.data.materials.append(finish.cornea_material("cornea"))
    for o in [proxy, eyes]:
        finish.prepare_uvs(o)
    marks = finish.skin_marks(proxy)
    proxy.data.materials.clear()
    head, skin = finish.split_skin(proxy, rig)
    if option(opts, "subdiv") != "0":
        head_build.subdivide(head, skin)
    head.data.materials.append(finish.head_material(variant, cfg, skin_img, marks))
    skin.data.materials.append(finish.body_material(variant, cfg, skin_img, marks))
    eyes.data.materials.clear()
    eyes.data.materials.append(finish.eye_material("eyes", eye_img, head_build.mid_height(eyes)))
    for key, tint, strength, cut in finish.cards(cfg):
        o = parts.get(key)
        if o is not None:
            img = finish.find_image(o)
            o.data.materials.clear()
            o.data.materials.append(finish.alpha_card_material(key, img, tint, strength, cut))

    bone = rig.matrix_world @ rig.pose.bones["head"].head
    centre = bone + Vector((0, -0.06, 0.1))
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
    shots = []
    for name, yaw, at, away in (("front", 0.0, centre, 0.85), ("three_q", 0.6, centre, 0.85), ("eye", 0.25, face.Face(head, eyes).eye_l, 0.3)):
        cam.location = at + Vector((math.sin(yaw), -math.cos(yaw), 0.0)) * away
        cam.rotation_euler = (at - cam.location).to_track_quat("-Z", "Y").to_euler()
        sc.render.filepath = os.path.join(out, f"{variant}_{tag}_{name}.png")
        bpy.ops.render.render(write_still=True)
        shots.append(sc.render.filepath)
        print(f"[face_lab] {sc.render.filepath}")
    if "--sheet" in opts:
        sheet(shots, os.path.join(out, f"{variant}_{tag}_sheet.png"))


main()

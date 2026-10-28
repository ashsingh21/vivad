"""Bake procedural materials down to one texture set: basecolor, ORM (AO/rough/metal) and normal."""

import os

import bpy
import numpy as np


def setup_cycles(samples=8):
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    prefs = bpy.context.preferences.addons["cycles"].preferences
    for backend in ("OPTIX", "CUDA", "HIP", "METAL", "ONEAPI"):
        try:
            prefs.compute_device_type = backend
            prefs.get_devices()
            gpus = [d for d in prefs.devices if d.type == backend]
        except TypeError:
            continue
        if gpus:
            for d in prefs.devices:
                d.use = d.type == backend
            scene.cycles.device = "GPU"
            break
    else:
        scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = False
    return scene.cycles.device


def new_image(name, res, non_color):
    img = bpy.data.images.get(name)
    if img:
        bpy.data.images.remove(img)
    img = bpy.data.images.new(name, res, res, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def _target_nodes(obj, img):
    nodes = []
    for slot in obj.material_slots:
        nt = slot.material.node_tree
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = img
        nt.nodes.active = n
        nodes.append((nt, n))
    return nodes


def _emission_rewire(obj, channel):
    """Route one Principled input into an Emission shader so EMIT bakes that channel exactly."""
    undo = []
    for slot in obj.material_slots:
        nt = slot.material.node_tree
        bsdf, out = nt.nodes["BSDF"], nt.nodes["OUT"]
        sock = bsdf.inputs[channel]
        em = nt.nodes.new("ShaderNodeEmission")
        em.inputs["Strength"].default_value = 1.0
        if sock.is_linked:
            nt.links.new(sock.links[0].from_socket, em.inputs["Color"])
        else:
            v = sock.default_value
            em.inputs["Color"].default_value = tuple(v)[:3] + (1.0,) if hasattr(v, "__len__") else (v, v, v, 1.0)
        nt.links.new(em.outputs["Emission"], out.inputs["Surface"])
        undo.append((nt, em, bsdf, out))
    return undo


def bake(obj, img, kind, channel=None, samples=8, margin=12):
    scene = bpy.context.scene
    scene.cycles.samples = samples
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    targets = _target_nodes(obj, img)
    undo = _emission_rewire(obj, channel) if kind == "EMIT" else []
    scene.render.bake.margin = margin
    scene.render.bake.use_clear = True
    scene.render.bake.target = "IMAGE_TEXTURES"
    kw = {"normal_space": "TANGENT"} if kind == "NORMAL" else {}
    bpy.ops.object.bake(type=kind, **kw)
    for nt, em, bsdf, out in undo:
        nt.nodes.remove(em)
        nt.links.new(bsdf.outputs["BSDF"], out.inputs["Surface"])
    for nt, n in targets:
        nt.nodes.remove(n)


def pixels(img):
    a = np.empty(img.size[0] * img.size[1] * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(img.size[1], img.size[0], 4)


def save(img, path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()


def bake_texture_set(obj, out_dir, prefix, res=2048):
    """Bake the full set and return {'basecolor','orm','normal'} image datablocks saved as PNG."""
    base = new_image(f"{prefix}_basecolor", res, non_color=False)
    rough = new_image(f"{prefix}_rough_tmp", res, non_color=True)
    metal = new_image(f"{prefix}_metal_tmp", res, non_color=True)
    ao = new_image(f"{prefix}_ao_tmp", res, non_color=True)
    normal = new_image(f"{prefix}_normal", res, non_color=True)

    bake(obj, base, "EMIT", "Base Color", samples=16)
    bake(obj, rough, "EMIT", "Roughness", samples=16)
    bake(obj, metal, "EMIT", "Metallic", samples=16)
    bake(obj, normal, "NORMAL", samples=16)
    bake(obj, ao, "AO", samples=256)

    orm = new_image(f"{prefix}_orm", res, non_color=True)
    p = np.ones((res, res, 4), dtype=np.float32)
    # Soften the AO so it reads as contact shadow, not dirt.
    p[..., 0] = 0.25 + 0.75 * pixels(ao)[..., 0]
    p[..., 1] = pixels(rough)[..., 0]
    p[..., 2] = pixels(metal)[..., 0]
    orm.pixels.foreach_set(p.ravel())

    out = {"basecolor": base, "orm": orm, "normal": normal}
    for key, img in out.items():
        save(img, os.path.join(out_dir, f"{prefix}_{key}.png"))
    for tmp in (rough, metal, ao):
        bpy.data.images.remove(tmp)
    return out

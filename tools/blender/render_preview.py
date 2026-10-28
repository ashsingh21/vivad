"""Render review images of an exported weapon GLB (renders the export, not the source scene).

blender -b --factory-startup -P tools/blender/render_preview.py -- <glb> <out_dir> [samples]

Outputs <name>_<view>.png: 4 full views, close-ups and a topology (wire) view.
"""

import math
import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(HERE, "lib"))
import bake  # noqa: E402

HDRI = os.path.join(ROOT, "assets_src", "hdri", os.environ.get("PREVIEW_HDRI", "studio_small_08_2k.hdr"))
BG = (0.18, 0.18, 0.185)


def setup_world():
    w = bpy.data.worlds.new("studio")
    bpy.context.scene.world = w
    nt = w.node_tree
    nt.nodes.clear()
    env = nt.nodes.new("ShaderNodeTexEnvironment")
    env.image = bpy.data.images.load(HDRI)
    mapping = nt.nodes.new("ShaderNodeMapping")
    mapping.inputs["Rotation"].default_value = (0, 0, math.radians(60))
    nt.links.new(nt.nodes.new("ShaderNodeTexCoord").outputs["Generated"], mapping.inputs["Vector"])
    nt.links.new(mapping.outputs["Vector"], env.inputs["Vector"])
    bg_light = nt.nodes.new("ShaderNodeBackground")
    bg_light.inputs["Strength"].default_value = 1.0
    nt.links.new(env.outputs["Color"], bg_light.inputs["Color"])
    bg_cam = nt.nodes.new("ShaderNodeBackground")
    bg_cam.inputs["Color"].default_value = (*BG, 1)
    lp = nt.nodes.new("ShaderNodeLightPath")
    mix = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(lp.outputs["Is Camera Ray"], mix.inputs["Fac"])
    nt.links.new(bg_light.outputs["Background"], mix.inputs[1])
    nt.links.new(bg_cam.outputs["Background"], mix.inputs[2])
    out = nt.nodes.new("ShaderNodeOutputWorld")
    nt.links.new(mix.outputs["Shader"], out.inputs["Surface"])


def key_light():
    """Soft rim light so blade edges and bevels read."""
    data = bpy.data.lights.new("rim", "AREA")
    data.energy = 60
    data.size = 1.5
    obj = bpy.data.objects.new("rim", data)
    obj.location = (-1.2, -0.8, 1.6)
    obj.rotation_euler = Vector((1.2, 0.8, -1.2)).to_track_quat("Z", "Y").to_euler()
    bpy.context.scene.collection.objects.link(obj)


def camera(name, loc, target, up, lens=50.0, ortho=None):
    data = bpy.data.cameras.new(name)
    data.lens = lens
    data.clip_start = 0.01
    if ortho:
        data.type = "ORTHO"
        data.ortho_scale = ortho
    cam = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(cam)
    loc, target, up = Vector(loc), Vector(target), Vector(up)
    fwd = (target - loc).normalized()
    right = fwd.cross(up).normalized()
    true_up = right.cross(fwd)
    from mathutils import Matrix
    rot = Matrix((right, true_up, -fwd)).transposed()
    cam.matrix_world = Matrix.Translation(loc) @ rot.to_4x4()
    return cam


def render(cam, path, res):
    scene = bpy.context.scene
    scene.camera = cam
    scene.render.resolution_x, scene.render.resolution_y = res
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    glb, out_dir = argv[0], argv[1]
    samples = int(argv[2]) if len(argv) > 2 else 64
    name = os.path.splitext(os.path.basename(glb))[0]
    os.makedirs(out_dir, exist_ok=True)

    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=glb)
    objs = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    lod0 = next(o for o in objs if o.name.endswith("LOD0"))
    for o in objs:
        o.hide_render = o is not lod0

    scene = bpy.context.scene
    bake.setup_cycles(samples)
    scene.cycles.use_denoising = True
    scene.view_settings.view_transform = "AgX"
    scene.view_settings.look = "AgX - Medium High Contrast"
    scene.render.film_transparent = False
    setup_world()
    key_light()

    lo = Vector(min(v[i] for v in (lod0.matrix_world @ Vector(c) for c in lod0.bound_box)) for i in range(3))
    hi = Vector(max(v[i] for v in (lod0.matrix_world @ Vector(c) for c in lod0.bound_box)) for i in range(3))
    centre = (lo + hi) / 2
    length = hi.z - lo.z
    print(f"[preview] {name}: bounds {tuple(round(x, 4) for x in lo)} -> {tuple(round(x, 4) for x in hi)}")

    wide = (1920, 800)
    sq = (1200, 1200)
    z = Vector((0, 0, 1))
    y = Vector((0, 1, 0))
    views = {
        # Blade horizontal across the frame (camera up = -Y or +Y so +Z runs left/right).
        "1_side": camera("side", centre + Vector((2.4, 0, 0)), centre, y, lens=85, ortho=length * 1.08),
        "2_back": camera("back", centre + Vector((-2.4, 0, 0)), centre, y, ortho=length * 1.08),
        "3_three_quarter": camera("tq", centre + Vector((1.5, 0.9, 0.5)), centre, y, lens=70),
        "4_edge_on": camera("edge", centre + Vector((0.05, 2.4, 0)), centre, Vector((1, 0, 0)),
                            ortho=length * 1.08),
        "5_close_hilt": camera("hilt", Vector((0.22, 0.20, 0.03)), Vector((0, 0.02, -0.012)), z, lens=60),
        "6_close_plate_tip": camera("plate", Vector((0.17, -0.02, 0.50)), Vector((0, 0.0, 0.48)), y, lens=60),
        "7_close_tip": camera("tip", Vector((0.20, 0.05, 0.84)), Vector((0, 0.004, 0.84)), y, lens=60),
    }
    if name != "khanda":
        for v in ("5_close_hilt", "6_close_plate_tip", "7_close_tip"):
            del views[v]
        # Other models: close-ups on the upper end, the middle and the lower end of the long axis,
        # framed from the bounding box.
        size = hi - lo
        d = max(size.x, size.y, 0.06) * 2.2 + 0.12
        span = max(size.z, 0.05)
        for k, (label, f) in enumerate((("top", 0.85), ("mid", 0.5), ("low", 0.15))):
            target = Vector((centre.x, centre.y, lo.z + span * f))
            views[f"{5 + k}_close_{label}"] = camera(label, target + Vector((d * 0.8, -d * 0.55, d * 0.25)), target, z,
                                                     lens=60)
    for view, cam in views.items():
        render(cam, os.path.join(out_dir, f"{name}_{view}.png"), sq if "close" in view else wide)

    # Topology check: wire over clay.
    clay = bpy.data.materials.new("clay")
    clay.diffuse_color = (0.6, 0.6, 0.6, 1)
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "SINGLE"
    scene.display.shading.single_color = (0.55, 0.55, 0.56)
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    wire = lod0.copy()
    wire.data = lod0.data.copy()
    scene.collection.objects.link(wire)
    mod = wire.modifiers.new("w", "WIREFRAME")
    mod.thickness = 0.00012
    mod.use_replace = True
    mod.use_even_offset = False
    scene.display.shading.color_type = "OBJECT"
    lod0.color = (0.62, 0.62, 0.63, 1)
    wire.color = (0.05, 0.05, 0.06, 1)
    closes = [v for v in views if "close" in v]
    render(views[closes[0]], os.path.join(out_dir, f"{name}_8_wire_a.png"), sq)
    render(views[closes[1]], os.path.join(out_dir, f"{name}_9_wire_b.png"), sq)


if __name__ == "__main__":
    main()

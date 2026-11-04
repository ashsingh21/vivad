"""MoCap Online's Motus Man (the mannequin its rifle pack was captured on; the pack is gitignored,
tools/blender/characters/mocaponline) dressed as our street warrior: brown skin on the face, neck
and hands (the mannequin's own texture kept underneath for its features), a dark striped full-sleeve
shirt, washed jeans, navy sneakers. Each part of the body is told apart by the bone that moves it
(its strongest vertex group), painted procedurally and baked to one texture set on the mannequin's
own UVs. The pack's animations then play on it as captured, nothing converted.

blender -b -P tools/blender/characters/mannequin.py [-- --preview <dir>]
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402

import bake  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
PACK = os.path.join(HERE, "mocaponline", "FBX_Rifle_Basic_27A2")
RES = 1024

SKIN = (0.36, 0.21, 0.13)
SHIRT = (0.11, 0.065, 0.06)
STRIPE = (0.55, 0.16, 0.16)
JEANS = (0.16, 0.27, 0.45)
SHOES = (0.06, 0.08, 0.20)
SOLE = (0.80, 0.80, 0.78)


def region(group):
    """Which garment (or skin) a bone's part of the body wears."""
    g = group.lower()
    if "head" in g or "neck" in g or "hand" in g or "wep" in g:
        return "skin"
    if "foot" in g or "toe" in g:
        return "shoes"
    if "upleg" in g or g.endswith("leg") or "legroll" in g or "hips" in g or g == "root":
        return "jeans"
    return "shirt"


def paint(mesh_obj):
    """Per-vertex region colours and the shirt's stripe coordinate (arc length round the torso or
    the arm, in stripe widths), as attributes the material reads."""
    me = mesh_obj.data
    names = {vg.index: vg.name for vg in mesh_obj.vertex_groups}
    col = me.color_attributes.new("region", "FLOAT_COLOR", "POINT")
    stripe = me.attributes.new("stripe", "FLOAT", "POINT")
    counts = {}
    for v in me.vertices:
        g = max(v.groups, key=lambda e: e.weight, default=None)
        r = region(names[g.group]) if g else "shirt"
        x, y, z = v.co
        if r == "shirt":
            # Arms stand out to the sides in the rest pose: stripes run along them; the torso's run
            # up and down. Arc length round each, a stripe every 1.4 cm.
            if abs(x) > 0.22:
                s = math.atan2(z - 1.45, y) * 0.045
            else:
                s = math.atan2(y, x) * 0.15
            stripe.data[v.index].value = s / 0.014
        if r == "shoes" and z < 0.035:
            r = "sole"
        counts[r] = counts.get(r, 0) + 1
        c = {"skin": SKIN, "shirt": SHIRT, "jeans": JEANS, "shoes": SHOES, "sole": SOLE}[r]
        col.data[v.index].color = (*c, 1.0 if r == "skin" else 0.0)
    print(f"[mannequin] regions: {counts}")


def material(mesh_obj, diffuse):
    """Region colour; stripes on the shirt; the mannequin's own shading detail (eyes, brows, folds)
    multiplied into the skin; denim mottling on the jeans."""
    m = bpy.data.materials.new("mannequin")
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    # The names lib/bake.py rewires by.
    bsdf.name = "BSDF"
    nt.nodes["Material Output"].name = "OUT"
    attr = nt.nodes.new("ShaderNodeAttribute")
    attr.attribute_name = "region"
    stripe = nt.nodes.new("ShaderNodeAttribute")
    stripe.attribute_name = "stripe"
    # Thin stripes: frac(s) under 0.18.
    frac = nt.nodes.new("ShaderNodeMath")
    frac.operation = "FRACT"
    nt.links.new(stripe.outputs["Fac"], frac.inputs[0])
    thin = nt.nodes.new("ShaderNodeMath")
    thin.operation = "LESS_THAN"
    thin.inputs[1].default_value = 0.18
    nt.links.new(frac.outputs[0], thin.inputs[0])
    # Only on the shirt: its colour is SHIRT exactly.
    is_shirt = nt.nodes.new("ShaderNodeMath")
    is_shirt.operation = "COMPARE"
    is_shirt.inputs[1].default_value = SHIRT[0]
    is_shirt.inputs[2].default_value = 0.004
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(attr.outputs["Color"], sep.inputs["Color"])
    nt.links.new(sep.outputs["Red"], is_shirt.inputs[0])
    mask = nt.nodes.new("ShaderNodeMath")
    mask.operation = "MULTIPLY"
    nt.links.new(thin.outputs[0], mask.inputs[0])
    nt.links.new(is_shirt.outputs[0], mask.inputs[1])
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    nt.links.new(mask.outputs[0], mix.inputs["Factor"])
    nt.links.new(attr.outputs["Color"], mix.inputs["A"])
    mix.inputs["B"].default_value = (*STRIPE, 1)
    # The mannequin's own texture as shading detail, normalised round mid-grey, on the skin.
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = diffuse
    uv = nt.nodes.new("ShaderNodeUVMap")
    uv.uv_map = "UVMap"
    nt.links.new(uv.outputs["UV"], tex.inputs["Vector"])
    detail = nt.nodes.new("ShaderNodeMix")
    detail.data_type = "RGBA"
    detail.blend_type = "MULTIPLY"
    detail.inputs["Factor"].default_value = 1.0
    nt.links.new(mix.outputs["Result"], detail.inputs["A"])
    lift = nt.nodes.new("ShaderNodeMix")
    lift.data_type = "RGBA"
    lift.blend_type = "MULTIPLY"
    lift.inputs["Factor"].default_value = 1.0
    nt.links.new(tex.outputs["Color"], lift.inputs["A"])
    lift.inputs["B"].default_value = (1.6, 1.6, 1.6, 1)
    skin_only = nt.nodes.new("ShaderNodeMix")
    skin_only.data_type = "RGBA"
    nt.links.new(attr.outputs["Alpha"], skin_only.inputs["Factor"])
    skin_only.inputs["A"].default_value = (1, 1, 1, 1)
    nt.links.new(lift.outputs["Result"], skin_only.inputs["B"])
    nt.links.new(skin_only.outputs["Result"], detail.inputs["B"])
    # Denim mottling, everywhere faint.
    noise = nt.nodes.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 60.0
    mott = nt.nodes.new("ShaderNodeMix")
    mott.data_type = "RGBA"
    mott.blend_type = "OVERLAY"
    mott.inputs["Factor"].default_value = 0.12
    nt.links.new(detail.outputs["Result"], mott.inputs["A"])
    nt.links.new(noise.outputs["Color"], mott.inputs["B"])
    nt.links.new(mott.outputs["Result"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.7
    bsdf.inputs["Metallic"].default_value = 0.0
    mesh_obj.data.materials.clear()
    mesh_obj.data.materials.append(m)


def preview(arm, mesh_obj, out_dir):
    """Front, back and side renders: the rest pose and a frame of the pack's rifle run."""
    os.makedirs(out_dir, exist_ok=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_EEVEE_NEXT" if "BLENDER_EEVEE_NEXT" in [e.identifier for e in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items] else "BLENDER_EEVEE"
    sc.render.resolution_x, sc.render.resolution_y = 360, 480
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.55, 0.6, 0.7, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.0
    sc.world = world
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.0
    sun.rotation_euler = (0.8, 0.2, 0.6)
    sc.collection.objects.link(sun)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    sc.collection.objects.link(cam)
    sc.camera = cam
    from mathutils import Vector
    runs = [None]
    run = os.path.join(PACK, "Animation", "In-Place", "W2_Run_F_Loop_IPC.fbx")
    objs = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=run)
    new = [o for o in bpy.data.objects if o not in objs]
    act = next((o.animation_data.action for o in new if o.type == "ARMATURE" and o.animation_data), None)
    for o in new:
        bpy.data.objects.remove(o)
    runs.append(act)
    for pose in runs:
        arm.animation_data_create()
        arm.animation_data.action = pose
        if pose is None:
            for pb in arm.pose.bones:
                pb.matrix_basis.identity()
        sc.frame_set(8)
        for view, (dx, dy) in {"front": (0, -1), "back": (0, 1), "side": (1, 0)}.items():
            target = Vector((0, 0, 0.95))
            cam.location = target + Vector((dx * 3.4, dy * 3.4, 0.25))
            cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
            sc.render.filepath = os.path.join(out_dir, f"{'rest' if pose is None else 'run'}_{view}.png")
            bpy.ops.render.render(write_still=True)
    print(f"[mannequin] previews in {out_dir}")


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.fbx(filepath=os.path.join(PACK, "MotusMan", "MotusMan_v55.fbx"))
    arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    body = next(o for o in bpy.data.objects if o.type == "MESH")
    body.data.uv_layers[0].name = "UVMap"
    diffuse = next(n.image for n in body.data.materials[0].node_tree.nodes if n.type == "TEX_IMAGE" and "diff" in n.image.filepath.lower())
    paint(body)
    material(body, diffuse)
    bake.setup_cycles()
    images = bake.bake_texture_set(body, os.path.join(ROOT, "build", "textures", "mannequin"), "mannequin", res=RES)
    import export
    body.data.materials.clear()
    body.data.materials.append(export.game_material("mannequin", images))
    if "--preview" in sys.argv:
        preview(arm, body, sys.argv[sys.argv.index("--preview") + 1])


main()

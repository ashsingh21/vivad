"""Finish a character: procedural materials -> one baked opaque texture set (+ alpha hair cards)
-> LODs -> skinned GLB.

Opaque parts (skin, eyes, garments, jewellery, turban) are joined into one mesh with one material.
Hair, brows and lashes stay as alpha-tested cards with their own small textures.
"""

import os

import bpy
import numpy as np

import bake
import export
from nodes import Graph, new_material

# Target skin tone (sRGB) for South Asian warm brown, per variant.
SKIN_TONE = {"male": (0.66, 0.44, 0.31), "female": (0.71, 0.48, 0.35)}

CLOTH = {
    # name prefix: (base sRGB, border sRGB or None, pattern, border width m)
    "dhoti_fold": ((0.88, 0.85, 0.77), (0.72, 0.52, 0.18), "weave", 0.012),
    "dhoti": ((0.88, 0.85, 0.77), (0.72, 0.52, 0.18), "weave", 0.035),
    "kamarband": ((0.40, 0.05, 0.08), (0.80, 0.60, 0.22), "weave", 0.008),
    "angavastram": ((0.93, 0.47, 0.06), (0.55, 0.06, 0.05), "weave", 0.014),
    "pagdi": ((0.60, 0.05, 0.04), None, "bandhani", 0.0),
    "nauvari": ((0.05, 0.32, 0.18), (0.80, 0.60, 0.22), "buti", 0.04),
    "nirya": ((0.05, 0.32, 0.18), (0.80, 0.60, 0.22), "weave", 0.015),
    "pallu_fall": ((0.05, 0.32, 0.18), (0.80, 0.60, 0.22), "buti", 0.025),
    "pallu": ((0.05, 0.32, 0.18), (0.80, 0.60, 0.22), "buti", 0.022),
    "choli": ((0.46, 0.05, 0.11), (0.80, 0.60, 0.22), "weave", 0.01),
}


def srgb_to_lin(c):
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


def image_mean(img):
    a = np.empty(img.size[0] * img.size[1] * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    a = a.reshape(-1, 4)
    # Ignore near-black padding outside UV islands.
    lit = a[a[:, :3].sum(axis=1) > 0.05][:, :3]
    return lit.mean(axis=0)


def find_image(obj):
    for m in obj.data.materials:
        if m and m.node_tree:
            for n in m.node_tree.nodes:
                if n.type == "TEX_IMAGE" and n.image:
                    return n.image
    return None


# ------------------------------------------------------------------ materials

def skin_material(variant, img):
    m = new_material(f"{variant}_skin")
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img)
    mean = image_mean(img)
    target = srgb_to_lin(SKIN_TONE[variant])
    # Keep the texture's local variation (lips, cheeks, creases) but re-centre it on our tone.
    ratio = g.vec("MULTIPLY", tex.outputs["Color"], g.combine(*(t / max(mu, 1e-3) for t, mu in zip(target, mean))))
    co = g.texcoord("Object")
    pores = g.noise(co, 900.0, detail=2.0, rough=0.6)
    mottle = g.noise(co, 25.0, detail=3.0)
    base = g.node("ShaderNodeMix", data_type="RGBA", clamp_factor=True)
    g.set(base.inputs[0], g.mul(g.sub(mottle, 0.5), 0.15))
    g.set(base.inputs[6], ratio)
    base.inputs[7].default_value = (*srgb_to_lin((0.45, 0.24, 0.16)), 1)
    rough = g.add(0.48, g.mul(pores, 0.12))
    normal = g.bump(g.add(g.mul(pores, 0.6), g.mul(mottle, 0.2)), 0.08, 0.0006)
    g.principled(base.outputs[2], 0.0, rough, normal)
    return m


def textured_material(name, img, rough=0.3):
    m = new_material(name)
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img)
    g.principled(tex.outputs["Color"], 0.0, rough)
    return m


def cloth_material(name, base_rgb, border_rgb, pattern, border_w):
    m = new_material(f"cloth_{name}")
    g = Graph(m)
    co = g.texcoord("Object")
    base = srgb_to_lin(base_rgb)
    # Cotton weave: two crossed fine stripes and slub noise.
    warp = g.wave(g.mapping(co, scale=(1, 1, 1)), 1400.0, 0.5, detail=0.0, direction="Z")
    weft = g.wave(g.mapping(co, scale=(1, 1, 1)), 1400.0, 0.5, detail=0.0, direction="X")
    weave = g.mul(g.add(warp, weft), 0.5)
    slub = g.noise(g.mapping(co, scale=(1, 1, 6)), 160.0, detail=3.0)
    col = g.mixc(g.mul(g.sub(slub, 0.5), 0.25), (*base, 1), tuple(x * 0.8 for x in base) + (1,))
    col = g.mixc(g.mul(weave, 0.08), col, tuple(min(1.0, x * 1.15) for x in base) + (1,))
    gold = (*srgb_to_lin((0.80, 0.60, 0.22)), 1)
    metal = g.value(0.0)
    rough = g.add(0.78, g.mul(slub, 0.1))
    if pattern == "bandhani":
        # Tie-dye: rings of small pale dots.
        dots = g.voronoi(g.mapping(co, scale=(1, 1, 1)), 140.0, randomness=0.25)
        dot = g.smooth(dots, 0.12, 0.06)
        col = g.mixc(g.mul(dot, 0.9), col, (*srgb_to_lin((0.98, 0.85, 0.45)), 1))
    elif pattern == "buti":
        dots = g.voronoi(g.mapping(co, scale=(1, 1, 1)), 55.0, randomness=0.3)
        dot = g.smooth(dots, 0.10, 0.06)
        col = g.mixc(dot, col, gold)
        metal = g.mix(dot, metal, 0.85)
        rough = g.mix(dot, rough, 0.35)
    if border_rgb is not None:
        # Zari / contrast border along every hem and opening.
        ed = g.attr("edge_d")
        band = g.smooth(ed, border_w, border_w * 0.8)
        thin = g.mul(g.smooth(ed, border_w * 1.15, border_w * 1.1), g.smooth(ed, border_w * 1.3, border_w * 1.35))
        border = (*srgb_to_lin(border_rgb), 1)
        col = g.mixc(band, col, border)
        col = g.mixc(thin, col, border)
        is_gold = border_rgb[0] > 0.7 and border_rgb[2] < 0.3
        if is_gold:
            metal = g.mix(band, metal, 0.85)
            rough = g.mix(band, rough, 0.32)
    normal = g.bump(g.add(g.mul(weave, 0.5), g.mul(slub, 0.5)), 0.12, 0.0004)
    g.principled(col, metal, rough, normal)
    return m


def metal_material(name, rgb, rough=0.28, metallic=1.0):
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    wear = g.noise(co, 120.0, detail=3.0)
    edge = g.edge_mask(0.002, 2.0)
    base = srgb_to_lin(rgb)
    col = g.mixc(g.mul(g.sub(wear, 0.4), 0.3), (*base, 1), tuple(x * 0.7 for x in base) + (1,))
    r = g.mix(edge, g.add(rough, g.mul(wear, 0.15)), rough * 0.5)
    g.principled(col, metallic, r, g.bump(wear, 0.05, 0.0003))
    return m


def alpha_card_material(name, img, tint_srgb, strength):
    """Hair / brows / lashes: texture recoloured toward `tint`, alpha-tested."""
    m = new_material(name)
    g = Graph(m)
    tex = g.node("ShaderNodeTexImage", image=img)
    bw = g.node("ShaderNodeRGBToBW", {"Color": tex.outputs["Color"]}).outputs["Val"]
    tint = srgb_to_lin(tint_srgb)
    col = g.mixc(strength, tex.outputs["Color"], g.combine(*(g.mul(bw, t * 2.2) for t in tint)))
    p = g.principled(col, 0.0, 0.45)
    cut = g.math("GREATER_THAN", tex.outputs["Alpha"], 0.5)
    g.set(p.inputs["Alpha"], cut)
    return m


# ------------------------------------------------------------------ assembly

def apply_non_armature_modifiers(obj):
    bpy.context.view_layer.objects.active = obj
    for mod in list(obj.modifiers):
        if mod.type != "ARMATURE":
            bpy.ops.object.modifier_apply(modifier=mod.name)


def prepare_uvs(obj):
    """Rename the object's UV to 'src' (shader lookup) and add 'UVMap' (bake atlas)."""
    me = obj.data
    if not me.uv_layers:
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.select_all(action="DESELECT")
        obj.select_set(True)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.smart_project(angle_limit=1.15, island_margin=0.01)
        bpy.ops.object.mode_set(mode="OBJECT")
    src = me.uv_layers[0]
    src.name = "src"
    atlas = me.uv_layers.new(name="UVMap")
    for a, b in zip(src.data, atlas.data):
        b.uv = a.uv
    # Keep 'src' first so the join keeps both layers consistently by name.
    me.uv_layers.active = atlas


def pack_atlas(obj, margin=0.004):
    me = obj.data
    me.uv_layers.active = me.uv_layers["UVMap"]
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.average_islands_scale()
    bpy.ops.uv.pack_islands(udim_source="CLOSEST_UDIM", rotate=True, scale=True, margin_method="FRACTION",
                            margin=margin, shape_method="CONCAVE")
    bpy.ops.object.mode_set(mode="OBJECT")


def finish_character(variant, rig, proxy, parts, items, root):
    # --- materials per source object
    skin_img = find_image(proxy)
    proxy.data.materials.clear()
    proxy.data.materials.append(skin_material(variant, skin_img))
    eyes = parts["eyes"]
    eye_img = find_image(eyes)
    eyes.data.materials.clear()
    eyes.data.materials.append(textured_material(f"{variant}_eyes", eye_img, 0.12))
    opaque = [proxy, eyes]
    for obj, role in items:
        kind = obj.get("kind", "cloth")
        obj.data.materials.clear()
        if kind == "gold":
            obj.data.materials.append(metal_material("gold", (1.0, 0.76, 0.36)))
        elif kind == "silver":
            obj.data.materials.append(metal_material("silver", (0.86, 0.87, 0.89), 0.25))
        elif kind == "lacquer":
            obj.data.materials.append(metal_material("lacquer", (0.62, 0.06, 0.08), 0.18, 0.0))
        else:
            key = next((k for k in CLOTH if obj.name.startswith(k)), "dhoti")
            base, border, pattern, bw = CLOTH[key]
            from garments import edge_distance_attr
            edge_distance_attr(obj)
            obj.data.materials.append(cloth_material(key, base, border, pattern, bw))
        opaque.append(obj)

    for o in opaque:
        apply_non_armature_modifiers(o)
        prepare_uvs(o)

    # --- join opaque parts into one skinned mesh
    bpy.ops.object.select_all(action="DESELECT")
    for o in opaque:
        o.select_set(True)
    bpy.context.view_layer.objects.active = proxy
    bpy.ops.object.join()
    body = bpy.context.view_layer.objects.active
    body.name = body.data.name = f"{variant}_LOD0"
    pack_atlas(body)

    # --- bake (Cycles needs the rest pose; the armature is in rest anyway)
    device = bake.setup_cycles()
    tris = sum(len(p.vertices) - 2 for p in body.data.polygons)
    print(f"[{variant}] baking on {device}; opaque tris={tris}")
    tex_dir = os.path.join(root, "build", "textures", variant)
    images = bake.bake_texture_set(body, tex_dir, variant, res=int(os.environ.get("BAKE_RES", "2048")))
    body.data.materials.clear()
    body.data.materials.append(export.game_material(variant, images))
    body.data.uv_layers.remove(body.data.uv_layers["src"])

    # --- alpha cards: hair, brows, lashes
    cards = []
    for key, tint, strength in (("hair", (0.05, 0.035, 0.03), 0.92), ("eyebrows", (0.05, 0.035, 0.03), 0.6),
                                ("eyelashes", (0.03, 0.02, 0.02), 0.5)):
        o = parts.get(key)
        if o is None or o.name not in bpy.data.objects:
            continue
        img = find_image(o)
        o.data.materials.clear()
        o.data.materials.append(alpha_card_material(f"{variant}_{key}", img, tint, strength))
        apply_non_armature_modifiers(o)
        cards.append(o)

    # --- LODs (decimate keeps vertex groups, so skinning survives). LOD0 is held to ~30k tris.
    export.triangulate(body)
    tris = sum(len(p.vertices) - 2 for p in body.data.polygons)
    if tris > 30000:
        d = body.modifiers.new("Budget", "DECIMATE")
        d.ratio = 30000 / tris
        bpy.context.view_layer.objects.active = body
        bpy.ops.object.modifier_move_to_index(modifier="Budget", index=0)
        bpy.ops.object.modifier_apply(modifier="Budget")
    lods = export.make_lods(body, (0.45, 0.18))
    for lod in lods[1:]:
        lod.parent = rig
    out = os.path.join(root, "assets", "models", "characters", f"{variant}.glb")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(root, "build", f"char_{variant}_final.blend"))
    bpy.ops.object.select_all(action="DESELECT")
    for o in [rig] + lods + cards:
        o.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.export_scene.gltf(
        filepath=out, export_format="GLB", use_selection=True, export_yup=True, export_apply=False,
        export_skins=True, export_def_bones=True, export_animations=False, export_tangents=True,
        export_image_format="AUTO",
    )
    counts = {o.name: sum(len(p.vertices) - 2 for p in o.data.polygons) for o in lods + cards}
    print(f"[{variant}] exported {out}: {counts}")

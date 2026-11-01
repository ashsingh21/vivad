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
# Alpha cards: (part, tint sRGB, how far the texture is pulled to the tint).
CARDS = (("hair", (0.045, 0.032, 0.026), 0.7), ("eyebrows", (0.035, 0.025, 0.02), 0.85), ("eyelashes", (0.03, 0.02, 0.02), 0.6))

SKIN_TONE = {"male": (0.66, 0.44, 0.31), "female": (0.71, 0.48, 0.35)}

CLOTH = {
    # name prefix: (base sRGB, border sRGB or None, pattern, border width m). Longer prefixes first:
    # a garment takes the first that its name starts with.
    # Panga: modern desi clothes.
    "shirt_placket": ((0.80, 0.46, 0.04), None, "check", 0.0),
    "shirt": ((0.80, 0.46, 0.04), None, "check", 0.0),
    # Indigo denim (its twill and fading lighten it a good deal), so jeans read as jeans.
    "jeans": ((0.10, 0.18, 0.38), None, "denim", 0.0),
    "belt": ((0.22, 0.12, 0.06), None, "weave", 0.0),
    "gamchha": ((0.78, 0.10, 0.10), None, "check_fine", 0.0),
    "kurti_panel": ((0.74, 0.10, 0.38), None, "buti", 0.0),
    "kurti": ((0.74, 0.10, 0.38), (0.86, 0.64, 0.24), "buti", 0.012),
    "kameez": ((0.74, 0.10, 0.38), (0.86, 0.64, 0.24), "buti", 0.012),
    "salwar": ((0.92, 0.88, 0.78), None, "weave", 0.0),
    "dupatta_fall": ((0.94, 0.70, 0.12), (0.74, 0.10, 0.38), "weave", 0.03),
    "dupatta": ((0.94, 0.70, 0.12), (0.74, 0.10, 0.38), "weave", 0.025),
    # Vivad: the ancient warriors' clothes.
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

def skin_material(variant, img, marks=None):
    """The MakeHuman skin texture re-centred on our tone, with pores, mottling and the face marks
    `face.decorate` stored on the proxy (`marks`): stubble (the `beard` attribute), kajal round
    the eyes, a bindi."""
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
    # The texture's freckle speckle reads as grain at game scale: keep only part of it.
    ratio = g.mixc(0.3, ratio, (*target, 1))
    base = g.node("ShaderNodeMix", data_type="RGBA", clamp_factor=True)
    # Polished stylised: an even tone, only a hint of mottling.
    g.set(base.inputs[0], g.mul(g.sub(mottle, 0.5), 0.05))
    g.set(base.inputs[6], ratio)
    base.inputs[7].default_value = (*srgb_to_lin((0.45, 0.24, 0.16)), 1)
    col = base.outputs[2]
    rough = g.add(0.55, g.mul(pores, 0.05))
    marks = marks or {}
    if marks.get("beard"):
        # Shaved stubble: fine dark dots over a faint blue-brown shadow.
        dots = g.smooth(g.noise(co, 2600.0, detail=0.0), 0.5, 0.62)
        amount = g.mul(g.attr("beard"), g.add(0.5, g.mul(dots, 0.4)))
        col = g.mixc(amount, col, (*srgb_to_lin((0.075, 0.058, 0.055)), 1))
        rough = g.add(rough, g.mul(g.attr("beard"), 0.1))
    if marks.get("eyes"):
        # Kajal: a soft dark line hugging each eye, heavier along the upper lid.
        e = list(marks["eyes"])
        el, er, rad = e[0:3], e[3:6], e[6]
        xyz = g.node("ShaderNodeSeparateXYZ", {"Vector": co}).outputs
        for c in (el, er):
            # Elliptical distance with the lid opening (about 0.8 x 0.4 of the eyeball radius) at 1.
            dx = g.mul(g.sub(xyz["X"], c[0]), 1.0 / (0.8 * rad))
            dz = g.sub(xyz["Z"], c[2])
            d = g.vec("LENGTH", g.combine(dx, g.mul(dz, 1.0 / (0.42 * rad)), 0.0))
            line = g.band(d, 0.92, 1.12, 0.12)
            # Bold along the upper lid, a fine line beneath.
            line = g.mul(line, g.add(0.35, g.mul(g.smooth(dz, -0.1 * rad, 0.2 * rad), 0.55)))
            col = g.mixc(line, col, (*srgb_to_lin((0.045, 0.03, 0.028)), 1))
    if marks.get("bindi"):
        d = g.vec("DISTANCE", co, tuple(marks["bindi"]))
        dot = g.smooth(d, 0.0036, 0.0029)
        col = g.mixc(dot, col, (*srgb_to_lin((0.55, 0.02, 0.05)), 1))
        rough = g.mix(dot, rough, 0.45)
    normal = g.bump(g.add(g.mul(pores, 0.6), g.mul(mottle, 0.2)), 0.015, 0.0005)
    g.principled(col, 0.0, rough, normal)
    return m


def eye_material(name, img):
    """MakeHuman's brown iris pulled toward a deep, less red brown; wet and glossy."""
    m = new_material(name)
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img)
    hsv = g.node("ShaderNodeHueSaturation", {"Color": tex.outputs["Color"]})
    hsv.inputs["Hue"].default_value = 0.52
    hsv.inputs["Saturation"].default_value = 0.7
    hsv.inputs["Value"].default_value = 0.9
    g.principled(hsv.outputs["Color"], 0.0, 0.06)
    return m


def hair_mesh_material(name):
    """Sculpted facial hair: near-black brown with fine strands running down the lip."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    strands = g.noise(g.mapping(co, scale=(1, 1, 0.06)), 2200.0, detail=1.0)
    col = g.mixc(g.mul(strands, 0.7), (*srgb_to_lin((0.045, 0.033, 0.027)), 1), (*srgb_to_lin((0.13, 0.095, 0.075)), 1))
    g.principled(col, 0.0, g.add(0.62, g.mul(strands, 0.2)), g.bump(strands, 0.6, 0.0005))
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
    # Crisp, clean cloth (polished stylised): the weave and slub only just there.
    col = g.mixc(g.mul(g.sub(slub, 0.5), 0.1), (*base, 1), tuple(x * 0.85 for x in base) + (1,))
    col = g.mixc(g.mul(weave, 0.04), col, tuple(min(1.0, x * 1.1) for x in base) + (1,))
    gold = (*srgb_to_lin((0.80, 0.60, 0.22)), 1)
    metal = g.value(0.0)
    rough = g.add(0.78, g.mul(slub, 0.1))
    if pattern in ("check", "check_fine"):
        # Madras check / gamchha: pale stripes crossing both ways, darker where they cross.
        k = 30.0 if pattern == "check" else 18.0
        sx = g.smooth(g.wave(g.mapping(co), k, 0.0, detail=0.0, direction="X"), 0.9, 0.95)
        sz = g.smooth(g.wave(g.mapping(co), k, 0.0, detail=0.0, direction="Z"), 0.9, 0.95)
        pale = (0.95, 0.93, 0.88) if pattern == "check_fine" else tuple(min(1.0, x * 1.8 + 0.15) for x in base_rgb)
        col = g.mixc(g.maxf(sx, sz), col, (*srgb_to_lin(pale), 1))
        col = g.mixc(g.mul(g.mul(sx, sz), 0.6), col, tuple(x * 0.55 for x in base) + (1,))
    elif pattern == "denim":
        # Twill: fine diagonal ribs, faded on the raised slubs.
        twill = g.wave(g.mapping(co, scale=(1, 1, 1)), 900.0, 0.0, detail=0.0, direction="DIAGONAL")
        col = g.mixc(g.mul(twill, 0.25), col, (*srgb_to_lin((0.55, 0.62, 0.75)), 1))
        col = g.mixc(g.mul(g.smooth(slub, 0.55, 0.75), 0.5), col, (*srgb_to_lin((0.45, 0.55, 0.72)), 1))
    elif pattern == "bandhani":
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
    normal = g.bump(g.add(g.mul(weave, 0.5), g.mul(slub, 0.5)), 0.05, 0.0004)
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


def tinted_image(name, img, tint_srgb, strength):
    """A copy of a card texture pulled toward `tint` (keeping its strand shading), packed so the
    glTF export carries the colour: the exporter can't follow a node recolour."""
    import numpy as np
    w, h = img.size
    px = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    lum = px[:, :3] @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    # Byte images hand back their stored (sRGB-encoded) values, so tint in sRGB.
    tint = np.array(tint_srgb, dtype=np.float32)
    px[:, :3] = px[:, :3] * (1 - strength) + np.outer(lum * 1.8, tint) * strength
    out = bpy.data.images.new(f"{name}_tinted", w, h, alpha=True)
    out.colorspace_settings.name = img.colorspace_settings.name
    out.pixels.foreach_set(px.ravel())
    out.pack()
    return out


def alpha_card_material(name, img, tint_srgb, strength):
    """Hair / brows / lashes: texture recoloured toward `tint`, alpha-tested."""
    m = new_material(name)
    g = Graph(m)
    tex = g.node("ShaderNodeTexImage", image=tinted_image(name, img, tint_srgb, strength))
    p = g.principled(tex.outputs["Color"], 0.0, 0.45)
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


# How freely each garment moves (crate::rim's vertex shader sways it with the warrior's motion):
# name prefix -> (where the flex starts, as a fraction down the piece from its top, full by its
# foot, and how much). Everything else (body, jeans, belts, jewellery) stays put.
FLEX = {
    "kurti_skirt": (0.0, 1.0),
    "kurti_panel": (0.0, 1.0),
    "kurti": (0.82, 0.45),
    "kameez": (0.82, 0.45),
    "gamchha": (0.1, 1.0),
    "shirt_placket": (0.85, 0.35),
    "shirt": (0.85, 0.35),
    "dupatta": (0.2, 1.0),
}


def flex_uvs(obj, flex_of):
    """A second UV layer 'Flex' (TEXCOORD_1 in the export): u is how far the mesh may sway there.
    Nothing else reads a second UV set (not the materials, not a preview), unlike a vertex colour
    whose alpha would cut alpha-tested hair away."""
    me = obj.data
    if "UVMap" not in me.uv_layers:
        me.uv_layers.new(name="UVMap")
    layer = me.uv_layers.new(name="Flex")
    for poly in me.polygons:
        for li in poly.loop_indices:
            layer.data[li].uv = (flex_of[me.loops[li].vertex_index], 0.0)
    me.uv_layers.active = me.uv_layers["UVMap"]
    me.uv_layers["UVMap"].active_render = True


def flex_layer(obj):
    """How far the cloth may sway at each vertex (0 where it's fastened, rising to the free hem),
    into the 'Flex' UV layer (flex_uvs)."""
    me = obj.data
    key = next((k for k in FLEX if obj.name.startswith(k)), None)
    zs = [(obj.matrix_world @ v.co).z for v in me.vertices]
    top, bottom = (max(zs), min(zs)) if zs else (0.0, 0.0)
    flex = [0.0] * len(me.vertices)
    for v, z in zip(me.vertices, zs):
        if key:
            start, amount = FLEX[key]
            down = (top - z) / max(top - bottom, 1e-4)
            t = min(max((down - start) / max(1.0 - start, 1e-4), 0.0), 1.0)
            flex[v.index] = amount * t * t * (3 - 2 * t)
    flex_uvs(obj, flex)


def hair_flex(obj):
    """The hair cards' 'Flex' colour layer: 0 at the crown, rising to the tips; long hair (well
    below the neck) swings freely, short hair only a little."""
    me = obj.data
    zs = [(obj.matrix_world @ v.co).z for v in me.vertices]
    top, bottom = max(zs), min(zs)
    amount = 1.0 if top - bottom > 0.3 else 0.35
    flex = [amount * min(max((top - z) / max(top - bottom, 1e-4), 0.0), 1.0) ** 1.5 for z in zs]
    flex_uvs(obj, flex)


def finish_character(variant, rig, proxy, parts, items, root):
    # --- materials per source object
    skin_img = find_image(proxy)
    proxy.data.materials.clear()
    marks = {"beard": "beard" in proxy.data.attributes, "eyes": proxy.get("eyes"), "bindi": proxy.get("bindi")}
    proxy.data.materials.append(skin_material(variant, skin_img, marks))
    eyes = parts["eyes"]
    eye_img = find_image(eyes)
    eyes.data.materials.clear()
    eyes.data.materials.append(eye_material(f"{variant}_eyes", eye_img))
    opaque = [proxy, eyes]
    for obj, role in items:
        kind = obj.get("kind", "cloth")
        obj.data.materials.clear()
        if kind == "gold":
            obj.data.materials.append(metal_material("gold", (1.0, 0.76, 0.36)))
        elif kind == "silver":
            obj.data.materials.append(metal_material("silver", (0.86, 0.87, 0.89), 0.25))
        elif kind == "hairmesh":
            obj.data.materials.append(hair_mesh_material(obj.name))
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
        flex_layer(o)

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
    # UVMap first (TEXCOORD_0, the texture's), Flex second.
    body.data.uv_layers.active = body.data.uv_layers["UVMap"]
    body.data.uv_layers["UVMap"].active_render = True

    # --- alpha cards: hair, brows, lashes
    cards = []
    for key, tint, strength in CARDS:
        o = parts.get(key)
        if o is None or o.name not in bpy.data.objects:
            continue
        img = find_image(o)
        o.data.materials.clear()
        o.data.materials.append(alpha_card_material(f"{variant}_{key}", img, tint, strength))
        apply_non_armature_modifiers(o)
        if key == "hair":
            # Hair sways like the cloth (crate::rim): still at the scalp, free toward the tips,
            # long hair most.
            hair_flex(o)
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

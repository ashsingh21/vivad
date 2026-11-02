"""Finish a character: procedural materials -> one baked texture set per part (+ alpha hair cards)
-> four LODs -> skinned GLB, <variant>_parts.glb.

Parts: head, body (the skin below the neck), outfit (garments and jewellery), eyes, cornea (a glass
cap, unbaked), mouth (teeth and tongue). Each is its own mesh and material on each LOD, named
<variant>_<part>_LOD<k>, all skinned to the one rig. Hair, brows and lashes stay alpha-tested cards
with their own small textures, named the same way.

Head and body are one surface, so they are decimated together and only cut apart afterwards, with
the welded surface's normals frozen into them: the neck has no crack and no shading seam on any LOD.
"""

import os
import time

import bmesh
import bpy
import numpy as np
from mathutils.bvhtree import BVHTree

import bake
import export
import head as head_build
from nodes import Graph, new_material

# Target skin tone (sRGB) for South Asian warm brown, per variant.
# Alpha cards: (part, tint sRGB, how far the texture is pulled to the tint, alpha cut).
HAIR_TINT = (0.06, 0.045, 0.038)
CARDS = (("hair", HAIR_TINT, 1.0, 0.5), ("eyebrows", (0.035, 0.025, 0.02), 0.85, 0.2), ("eyelashes", (0.03, 0.02, 0.02), 0.6, 0.5))

SKIN_TONE = {"male": (0.66, 0.44, 0.31), "female": (0.71, 0.48, 0.35)}
LIP_TONE = {"male": (0.42, 0.22, 0.20), "female": (0.48, 0.19, 0.20)}

CLOTH = {
    # name prefix: (base sRGB, border sRGB or None, pattern, border width m). Longer prefixes first:
    # a garment takes the first that its name starts with.
    # Panga: what MakeHuman's clothes don't bring.
    "backpack_strap": ((0.09, 0.10, 0.12), None, "weave", 0.0),
    "backpack_pocket": ((0.15, 0.17, 0.21), None, "weave", 0.0),
    "backpack": ((0.10, 0.11, 0.13), None, "weave", 0.0),
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


def find_maps(obj):
    """(colour, normal) images of a MakeSkin material: either can come first, or be missing."""
    colour, normal = None, None
    for m in obj.data.materials:
        for n in (m.node_tree.nodes if m and m.node_tree else ()):
            if n.type == "TEX_IMAGE" and n.image:
                names = f"{n.name} {n.label} {n.image.name}".lower()
                if "norm" in names or "nrm" in names:
                    normal = normal or n.image
                elif "diffuse" in names or colour is None:
                    colour = n.image
    return colour, normal


def image_brightness(img):
    """The mean brightness of a texture as the shader reads it: byte images hand back sRGB."""
    px = np.empty(img.size[0] * img.size[1] * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    rgb = px[px[:, 3] > 0.5][:, :3]
    if not img.is_float:
        rgb = np.where(rgb <= 0.04045, rgb / 12.92, ((rgb + 0.055) / 1.055) ** 2.4)
    return float((rgb @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)).mean())


# ------------------------------------------------------------------ materials

def skin_base(g, variant, img, keep=0.7):
    """The MakeHuman skin texture re-centred on our tone: its local variation (lips, cheeks,
    creases) stays, `keep` of it; the rest of its freckle speckle would read as grain."""
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img)
    mean = image_mean(img)
    target = srgb_to_lin(SKIN_TONE[variant])
    ratio = g.vec("MULTIPLY", tex.outputs["Color"], g.combine(*(t / max(mu, 1e-3) for t, mu in zip(target, mean))))
    return g.mixc(1.0 - keep, ratio, (*target, 1))


def pore_height(g, co, scale):
    """Skin relief: pits at about 1.3 mm over fine crosshatched creases. 1 at the skin's top."""
    pits = g.smooth(g.voronoi(co, 760.0 * scale, randomness=0.9), 0.06, 0.34)
    a = g.noise(g.mapping(co, scale=(1, 1, 0.3)), 1500.0 * scale, detail=1.0)
    b = g.noise(g.mapping(co, scale=(0.3, 1, 1)), 1500.0 * scale, detail=1.0)
    return pits, g.mul(g.add(a, b), 0.5)


def stubble(g, co, col, rough):
    """Shaved stubble over `col` and `rough`, from the `beard` attribute: fine dark dots over a
    faint blue-brown shadow. It runs past the cut between head and body."""
    dots = g.smooth(g.noise(co, 2600.0, detail=0.0), 0.5, 0.62)
    amount = g.mul(g.attr("beard"), g.add(0.5, g.mul(dots, 0.4)))
    return g.mixc(amount, col, (*srgb_to_lin((0.075, 0.058, 0.055)), 1)), g.add(rough, g.mul(g.attr("beard"), 0.1))


def body_material(variant, img, marks=None):
    """Skin below the neck: tone, a hint of mottling, finer pores than the face."""
    m = new_material(f"{variant}_body_skin")
    g = Graph(m)
    co = g.texcoord("Object")
    mottle = g.noise(co, 25.0, detail=3.0)
    col = g.mixc(g.mul(g.sub(mottle, 0.5), 0.05), skin_base(g, variant, img), (*srgb_to_lin((0.45, 0.24, 0.16)), 1))
    pits, creases = pore_height(g, co, 1.2)
    rough = g.add(0.52, g.mul(creases, 0.08))
    if (marks or {}).get("beard"):
        col, rough = stubble(g, co, col, rough)
    g.principled(col, 0.0, rough, g.bump(g.add(g.mul(pits, 0.5), g.mul(creases, 0.3)), 0.15, 0.0004))
    return m


def age_lines(age):
    """How much the face's creases show, from the MakeHuman age macro (0.5 is 25 years, 1 is 90)."""
    return 0.2 + 0.8 * min(1.0, max(0.0, (age - 0.5) / 0.4))


def head_material(variant, img, marks=None, age=0.5):
    """The face: `skin_base` and the region masks of `face.regions` (lips, blush over the blood-rich
    parts, darker sockets, shine on the T zone, pores coarse on the nose and cheeks and all but gone
    on the lids), then the marks `face.decorate` stored on the proxy (`marks`): stubble, kajal round
    the eyes, the scalp under the hair."""
    m = new_material(f"{variant}_head_skin")
    g = Graph(m)
    co = g.texcoord("Object")
    mottle = g.noise(co, 25.0, detail=3.0)
    col = g.mixc(g.mul(g.sub(mottle, 0.5), 0.06), skin_base(g, variant, img), (*srgb_to_lin((0.45, 0.24, 0.16)), 1))
    col = g.mixc(g.mul(g.attr("blush"), 0.22), col, (*srgb_to_lin((0.72, 0.30, 0.22)), 1))
    col = g.mixc(g.mul(g.attr("socket"), 0.32), col, (*srgb_to_lin((0.26, 0.16, 0.15)), 1))
    lines = g.mul(g.attr("lines"), age_lines(age))
    col = g.mixc(g.mul(lines, 0.3), col, (*srgb_to_lin((0.30, 0.17, 0.13)), 1))
    lip = g.attr("lip")
    col = g.mixc(g.mul(lip, 0.7), col, (*srgb_to_lin(LIP_TONE[variant]), 1))

    pits, creases = pore_height(g, co, 1.0)
    pores = g.attr("pores")
    height = g.add(g.mul(g.sub(pits, 1.0), g.add(0.18, g.mul(pores, 0.82))), g.mul(creases, 0.3))
    height = g.sub(height, g.mul(lines, 0.25))
    # Lips: fine vertical furrows instead of pores.
    furrows = g.noise(g.mapping(co, scale=(1, 1, 0.05)), 1100.0, detail=1.0)
    height = g.mix(lip, height, g.mul(furrows, 0.6))
    rough = g.add(0.5, g.mul(creases, 0.08))
    rough = g.sub(rough, g.mul(g.attr("oily"), 0.1))
    rough = g.mix(lip, rough, 0.43)
    marks = marks or {}
    if marks.get("beard"):
        col, rough = stubble(g, co, col, rough)
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
    col = g.mixc(g.smooth(g.attr("scalp"), 0.15, 0.6), col, (*srgb_to_lin(HAIR_TINT), 1))
    if marks.get("brows"):
        uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
        brow = g.node("ShaderNodeTexImage", {"Vector": uv}, image=bpy.data.images[marks["brows"]]).outputs["Color"]
        col = g.mixc(g.mul(brow, 0.8), col, (*srgb_to_lin((0.05, 0.036, 0.03)), 1))
        rough = g.mix(brow, rough, 0.8)
    g.principled(col, 0.0, rough, g.bump(height, 0.18, 0.0005))
    return m


def eye_material(name, img, lid_z):
    """MakeHuman's brown iris pulled toward a deep, less red brown; wet and glossy. The upper lid's
    shadow falls on it from `lid_z` up (object space): the game's lights don't see the lid."""
    m = new_material(name)
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img)
    hsv = g.node("ShaderNodeHueSaturation", {"Color": tex.outputs["Color"]})
    hsv.inputs["Hue"].default_value = 0.52
    hsv.inputs["Saturation"].default_value = 0.7
    hsv.inputs["Value"].default_value = 0.75
    z = g.node("ShaderNodeSeparateXYZ", {"Vector": g.texcoord("Object")}).outputs["Z"]
    shade = g.smooth(z, lid_z - 0.001, lid_z + 0.006)
    g.principled(g.mixc(g.mul(shade, 0.65), hsv.outputs["Color"], (0.0, 0.0, 0.0, 1.0)), 0.0, 0.06)
    return m


def cornea_material(name):
    """The glass over the eye: nothing of its own but a sharp wet highlight. glTF Blend, not baked."""
    m = new_material(name)
    g = Graph(m)
    p = g.principled((0.0, 0.0, 0.0, 1.0), 0.0, 0.02)
    p.inputs["Alpha"].default_value = 0.08
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


def fitted_material(name, img, normal, tint, detail, rough):
    """A fitted garment from its own texture. With `tint` the texture gives only its light and dark:
    its brightness over its own mean, so a dark texture can't drag the colour down (a plain multiply
    turned the trial's red tee brown), then `detail` of that contrast over the tint; a printed
    garment at a low `detail` goes plain."""
    m = new_material(f"cloth_{name}")
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    col = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img).outputs["Color"]
    if tint is not None:
        shade = g.mul(g.node("ShaderNodeRGBToBW", {"Color": col}).outputs["Val"], detail / image_brightness(img))
        shade = g.add(shade, 1.0 - detail)
        col = g.vec("MULTIPLY", g.combine(shade, shade, shade), srgb_to_lin(tint))
    bent = None
    if normal is not None:
        normal.colorspace_settings.name = "Non-Color"
        tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=normal)
        bent = g.node("ShaderNodeNormalMap", {"Color": tex.outputs["Color"]}, uv_map="src").outputs["Normal"]
    g.principled(col, 0.0, rough, bent)
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
    w, h = img.size
    px = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    lum = px[:, :3] @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    # Its brightness over its own mean: blond or dark, every texture lands on the tint. Byte images
    # hand back their stored (sRGB-encoded) values, so tint in sRGB.
    solid = px[:, 3] > 0.5
    shade = np.clip(lum / max(float(lum[solid].mean()) if solid.any() else 0.5, 1e-3), 0, 3.5)
    tint = np.array(tint_srgb, dtype=np.float32)
    px[:, :3] = px[:, :3] * (1 - strength) + np.outer(shade, tint) * strength
    out = bpy.data.images.new(f"{name}_tinted", w, h, alpha=True)
    out.colorspace_settings.name = img.colorspace_settings.name
    out.pixels.foreach_set(px.ravel())
    out.pack()
    return out


def alpha_card_material(name, img, tint_srgb, strength, cut):
    """Hair / brows / lashes: texture recoloured toward `tint`, alpha-tested at `cut`."""
    m = new_material(name)
    g = Graph(m)
    tex = g.node("ShaderNodeTexImage", image=tinted_image(name, img, tint_srgb, strength))
    p = g.principled(tex.outputs["Color"], 0.0, 0.55)
    g.set(p.inputs["Alpha"], g.math("GREATER_THAN", tex.outputs["Alpha"], cut))
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


# Texture size baked per part (BAKE_RES caps them, for quick test builds). The file carries the head
# at GLB_HEAD_RES; its full-size colour and normal go through tools/textures/character_pack.txt for
# the Ultra level.
PART_RES = {"head": 4096, "body": 2048, "outfit": 2048, "eyes": 512, "mouth": 512}
GLB_HEAD_RES = 2048

# Triangles per LOD; None keeps what the part has. Head, body and outfit together follow the budgets
# in docs/PLAN.md: LOD1 ~25k, LOD2 ~12k, LOD3 ~5k.
LOD_TRIS = {
    "head": (18000, 6500, 3200, 1200),
    "body": (None, 8000, 3800, 1200),
    "outfit": (None, 10500, 5000, 2600),
    # The MakeHuman teeth are 7k triangles behind closed lips. The mouth goes from LOD2 on.
    "mouth": (1500, 600),
    # Past LOD1 the eyes are about what the low-poly MakeHuman eyes were; the cornea is gone.
    "eyes": (None, None, 180, 180),
    "cornea": (None, None),
}
# Which LODs show the cards, and the hair's triangles on the last one (no scalp cap yet to stand in
# for it, so it keeps its shape). Levels past the tuple's end have no such part.
CARD_LODS = {"hair": 4, "eyebrows": 3, "eyelashes": 2}
HAIR_LOD3 = 0.55


def tri_count(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def select_only(obj):
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj


def join(name, objs):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    if len(objs) > 1:
        bpy.ops.object.join()
    out = bpy.context.view_layer.objects.active
    out.name = out.data.name = name
    return out


def delete_faces(obj, doomed):
    """Delete the faces `doomed(mean z)` picks, from the object's world-space mean height."""
    mw = obj.matrix_world
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    gone = [f for f in bm.faces if doomed(sum((mw @ v.co).z for v in f.verts) / len(f.verts))]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bm.to_mesh(obj.data)
    bm.free()


def split_skin(proxy, rig):
    """Cut the proxy across the neck, above any collar, into (head, body)."""
    bones = rig.data.bones
    mw = rig.matrix_world
    neck, skull = (mw @ bones["neck_01"].head_local).z, (mw @ bones["head"].head_local).z
    cut = neck + 0.55 * (skull - neck)
    head = proxy.copy()
    head.data = proxy.data.copy()
    bpy.context.scene.collection.objects.link(head)
    delete_faces(head, lambda z: z < cut)
    delete_faces(proxy, lambda z: z >= cut)
    return head, proxy


def bake_part(variant, part, obj, root):
    """Pack the part's own atlas, bake its set and swap in the game material. The rest of the
    character stays in the scene, so the baked occlusion still sees the clothes on the skin."""
    pack_atlas(obj)
    res = PART_RES[part]
    if os.environ.get("BAKE_RES"):
        res = min(res, int(os.environ["BAKE_RES"]))
    start = time.monotonic()
    # A wide margin: decimation leaves some LOD triangles with corners off the islands, in what
    # would otherwise be the black between them.
    images = bake.bake_texture_set(obj, os.path.join(root, "build", "textures", variant), f"{variant}_{part}", res=res, margin=res // 8)
    if part == "head" and res > GLB_HEAD_RES:
        for img in images.values():
            img.scale(GLB_HEAD_RES, GLB_HEAD_RES)
    me = obj.data
    me.materials.clear()
    me.materials.append(export.game_material(f"{variant}_{part}", images))
    me.polygons.foreach_set("material_index", [0] * len(me.polygons))
    me.uv_layers.remove(me.uv_layers["src"])
    print(f"[{variant}] baked {part} at {res}: {tri_count(obj)} tris, {time.monotonic() - start:.0f} s")


def weld_seam(obj):
    """Sew the two halves of the skin back together along the cut."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=[v for v in bm.verts if v.is_boundary], dist=1e-5)
    bm.to_mesh(obj.data)
    bm.free()


def lod_copy(src, name):
    o = src.copy()
    o.data = src.data.copy()
    o.name = o.data.name = name
    bpy.context.scene.collection.objects.link(o)
    return o


def decimate(o, tris, keep_slot=None, hold=None):
    """Cut `o` to `tris` triangles, still skinned. With `keep_slot`, the faces of that material are
    left alone (a vertex group's zero weight stops the decimator on them) but for their edge; so are
    the vertices the point attribute `hold` marks."""
    have = tri_count(o)
    if have <= tris:
        return
    d = o.modifiers.new("Decimate", "DECIMATE")
    d.decimate_type = "COLLAPSE"
    d.ratio = tris / have
    d.use_collapse_triangulate = True
    if keep_slot is not None:
        kept = np.zeros(len(o.data.vertices), dtype=bool)
        for p in o.data.polygons:
            if p.material_index == keep_slot:
                kept[list(p.vertices)] = True
        for p in o.data.polygons:
            if p.material_index != keep_slot:
                kept[list(p.vertices)] = False
        if hold is not None:
            marked = np.empty(len(o.data.vertices), dtype=np.float32)
            o.data.attributes[hold].data.foreach_get("value", marked)
            kept |= marked > 0.5
        g = o.vertex_groups.new(name="_may_collapse")
        g.add(np.flatnonzero(~kept).tolist(), 1.0, "REPLACE")
        d.vertex_group = g.name
        d.vertex_group_factor = 1.0
    select_only(o)
    bpy.ops.object.modifier_move_to_index(modifier=d.name, index=0)
    bpy.ops.object.modifier_apply(modifier=d.name)
    if keep_slot is not None:
        o.vertex_groups.remove(o.vertex_groups["_may_collapse"])


def drop_covered_islands(obj, cloth, slot, gap=0.08):
    """Delete pieces of the `slot` skin that are mostly under cloth: the scraps `delete_hidden_body`
    left. The far LODs' cloth shrinks onto them and they'd show through. Returns the faces gone."""
    bm = bmesh.new()
    bm.from_mesh(cloth.data)
    tree = BVHTree.FromBMesh(bm)
    bm.free()
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    todo = {f for f in bm.faces if f.material_index == slot}
    total = len(todo)
    doomed = []
    while todo:
        stack = [todo.pop()]
        piece = []
        while stack:
            f = stack.pop()
            piece.append(f)
            for e in f.edges:
                for g in e.link_faces:
                    if g in todo:
                        todo.remove(g)
                        stack.append(g)
        verts = {v for f in piece for v in f.verts}
        hidden = sum(tree.ray_cast(v.co + v.normal * 0.002, v.normal, gap)[0] is not None for v in verts)
        if hidden * 2 > len(verts) and len(piece) * 4 < total:
            doomed += piece
    bmesh.ops.delete(bm, geom=doomed, context="FACES")
    bm.to_mesh(obj.data)
    bm.free()
    return len(doomed)


def skin_lod(skin, name, head_slot, head_tris, body_tris, hold=None):
    """The welded skin at (head, body) triangles: the body cut with the head held, then the
    other way round. None keeps a part as it is; `hold` as for `decimate`, on the head's cut."""
    o = lod_copy(skin, name)
    slots = np.zeros(len(o.data.polygons), dtype=np.int32)
    if body_tris is not None:
        o.data.polygons.foreach_get("material_index", slots)
        decimate(o, int((slots == head_slot).sum()) + body_tris, keep_slot=head_slot)
        slots = np.zeros(len(o.data.polygons), dtype=np.int32)
    o.data.polygons.foreach_get("material_index", slots)
    decimate(o, head_tris + int((slots != head_slot).sum()), keep_slot=1 - head_slot, hold=hold)
    return o


def freeze_normals(obj):
    """Store the surface's normals on it, so cutting it up doesn't change its shading."""
    me = obj.data
    n = np.empty(len(me.loops) * 3, dtype=np.float32)
    me.corner_normals.foreach_get("vector", n)
    me.normals_split_custom_set(n.reshape(-1, 3))


def cut_by_material(obj, names):
    """Separate a mesh by material into {material name: object}, renamed by `names`."""
    select_only(obj)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.separate(type="MATERIAL")
    bpy.ops.object.mode_set(mode="OBJECT")
    pieces = [o for o in bpy.context.selected_objects if o.type == "MESH"]
    out = {}
    for o in pieces:
        mat = o.data.materials[o.data.polygons[0].material_index].name
        o.name = o.data.name = names[mat]
        out[mat] = o
    return out


def card_lod(variant, key, src, k):
    """Cards on LOD k: the same mesh, except the last LOD's hair."""
    name = f"{variant}_{key}_LOD{k}"
    if key == "hair" and k == 3:
        o = lod_copy(src, name)
        decimate(o, int(tri_count(src) * HAIR_LOD3))
        return o
    o = src.copy()
    o.name = name
    bpy.context.scene.collection.objects.link(o)
    return o


def skin_marks(proxy):
    return {"beard": "beard" in proxy.data.attributes, "eyes": proxy.get("eyes"), "brows": proxy.get("brows")}


def finish_character(variant, rig, proxy, parts, items, root, age=0.5):
    # --- materials per source object
    skin_img = find_image(proxy)
    proxy.data.materials.clear()
    eye_img = find_image(parts["eyes"])
    eyes, cornea = head_build.eye_parts(parts["eyes"], f"{variant}_cornea_mesh")
    cornea.data.materials.clear()
    cornea.data.materials.append(cornea_material(f"{variant}_cornea"))
    apply_non_armature_modifiers(cornea)
    eyes.data.materials.clear()
    eyes.data.materials.append(eye_material(f"{variant}_iris", eye_img, head_build.mid_height(eyes)))
    mouth = [parts["teeth"], parts["tongue"]]
    for o, rough in zip(mouth, (0.2, 0.4)):
        img = find_image(o)
        o.data.materials.clear()
        o.data.materials.append(textured_material(o.name, img, rough))
    outfit = []
    for obj, role in items:
        kind = obj.get("kind", "cloth")
        colour, normal = find_maps(obj)
        obj.data.materials.clear()
        if kind == "fitted":
            if "normal_map" in obj:
                normal = bpy.data.images[obj["normal_map"]]
            obj.data.materials.append(fitted_material(obj.name, colour, normal, obj.get("tint"), obj["detail"], obj["rough"]))
        elif kind == "gold":
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
        outfit.append(obj)

    for o in [proxy, eyes, *mouth, *outfit]:
        apply_non_armature_modifiers(o)
        prepare_uvs(o)

    # --- one mesh per part
    head, body = split_skin(proxy, rig)
    head_build.subdivide(head, body)
    head.data.materials.append(head_material(variant, skin_img, skin_marks(proxy), age))
    body.data.materials.append(body_material(variant, skin_img, skin_marks(proxy)))
    meshes = {
        "head": join(f"{variant}_head", [head]),
        "body": join(f"{variant}_body", [body]),
        "outfit": join(f"{variant}_outfit", outfit),
        "eyes": join(f"{variant}_eyes_mesh", [eyes]),
        "mouth": join(f"{variant}_mouth_mesh", mouth),
    }

    # --- bake (Cycles needs the rest pose; the armature is in rest anyway)
    device = bake.setup_cycles()
    print(f"[{variant}] baking on {device}")
    for part, obj in meshes.items():
        bake_part(variant, part, obj, root)

    # --- alpha cards: hair, brows, lashes
    cards = {}
    for key, tint, strength, cut in CARDS:
        o = parts.get(key)
        if o is None or o.name not in bpy.data.objects:
            continue
        img = find_maps(o)[0]
        o.data.materials.clear()
        o.data.materials.append(alpha_card_material(f"{variant}_{key}", img, tint, strength, cut))
        apply_non_armature_modifiers(o)
        cards[key] = o

    # --- LODs (decimate keeps vertex groups, so skinning survives)
    out_objs = []
    skin = join(f"{variant}_skin", [meshes["head"], meshes["body"]])
    head_mat, body_mat = f"{variant}_head", f"{variant}_body"
    head_slot = next(i for i, m in enumerate(skin.data.materials) if m.name == head_mat)
    weld_seam(skin)
    for o in (skin, meshes["outfit"], meshes["mouth"]):
        export.triangulate(o)
    print(f"[{variant}] {drop_covered_islands(skin, meshes['outfit'], 1 - head_slot)} faces of skin scraps under the cloth removed")
    for k, (head_tris, body_tris) in enumerate(zip(LOD_TRIS["head"], LOD_TRIS["body"])):
        o = skin_lod(skin, f"{variant}_skin_LOD{k}", head_slot, head_tris, body_tris, hold="detail" if k == 0 else None)
        freeze_normals(o)
        cut = cut_by_material(o, {head_mat: f"{variant}_head_LOD{k}", body_mat: f"{variant}_body_LOD{k}"})
        out_objs += cut.values()
    meshes["cornea"] = cornea
    for part in ("outfit", "mouth", "eyes", "cornea"):
        for k, tris in enumerate(LOD_TRIS[part]):
            o = lod_copy(meshes[part], f"{variant}_{part}_LOD{k}")
            if tris is not None:
                decimate(o, tris)
            out_objs.append(o)
    for key, o in cards.items():
        out_objs += [card_lod(variant, key, o, k) for k in range(CARD_LODS[key])]

    # The sources are done with.
    for o in [skin, meshes["outfit"], meshes["mouth"], meshes["eyes"], cornea, *cards.values()]:
        bpy.data.objects.remove(o)
    for o in out_objs:
        o.parent = rig

    # anims.py opens this for the rig (and the nose, on the head).
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(root, "build", f"char_{variant}_final.blend"))
    out = os.path.join(root, "assets", "models", "characters", f"{variant}_parts.glb")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    bpy.ops.object.select_all(action="DESELECT")
    for o in [rig, *out_objs]:
        o.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.export_scene.gltf(
        filepath=out, export_format="GLB", use_selection=True, export_yup=True, export_apply=False,
        export_skins=True, export_def_bones=True, export_animations=False, export_tangents=True,
        export_image_format="AUTO",
    )
    for k in range(4):
        row = {o.name.split("_")[1]: tri_count(o) for o in out_objs if o.name.endswith(f"_LOD{k}")}
        print(f"[{variant}] LOD{k}: {sum(row.values())} tris {row}")
    print(f"[{variant}] exported {out}")

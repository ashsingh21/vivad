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
CARDS = (("hair", (0.075, 0.055, 0.045), 0.92), ("eyebrows", (0.035, 0.025, 0.02), 0.85), ("eyelashes", (0.03, 0.02, 0.02), 0.6))

SKIN_TONE = {"male": (0.66, 0.44, 0.31), "female": (0.71, 0.48, 0.35), "anime": (0.74, 0.51, 0.38)}

# The anime character's (crate::rim's cel shading): its materials are named toon_* so the game
# shades them in flat bands; its colours are flat, its features painted as line and shape.
ANIME = "anime"
INK = (0.10, 0.05, 0.045)


def toon_name(variant, name):
    return f"toon_{name}" if variant == ANIME else name

CLOTH = {
    # name prefix: (base sRGB, border sRGB or None, pattern, border width m). Longer prefixes first:
    # a garment takes the first that its name starts with.
    # The anime character's (outfits.anime_street): a loose teal kurta bordered cream over indigo
    # salwar; flat colour, cel shaded.
    # The kurta's body (anime_kurtab) unbordered (its hem is hidden under the skirt's top), the
    # skirt and panels bordered cream at the hem; salwar deep indigo (well clear of skin).
    "anime_kurtab": ((0.08, 0.45, 0.48), None, "flat", 0.0),
    "anime_kurta": ((0.08, 0.45, 0.48), (0.97, 0.92, 0.80), "flat", 0.03),
    "anime_salwar": ((0.16, 0.20, 0.38), None, "flat", 0.0),
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
    # The tactical kit (outfits.tactical_kit): coyote nylon, black webbing.
    "vest": ((0.42, 0.35, 0.24), None, "weave", 0.0),
    "pouch": ((0.36, 0.30, 0.21), None, "weave", 0.0),
    "tac_belt": ((0.07, 0.07, 0.07), None, "weave", 0.0),
    "kneepad": ((0.10, 0.10, 0.10), None, "weave", 0.0),
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


def anime_skin_material(img, marks):
    """Anime skin: one flat warm-brown tone (no pores, no mottling, the texture only for its lips
    and cheeks, faintly), a soft blush across the cheeks and a bold ink lash line along each upper
    lid, sweeping out at the corner, with a fine one beneath."""
    m = new_material(f"{ANIME}_skin")
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img)
    mean = image_mean(img)
    target = srgb_to_lin(SKIN_TONE[ANIME])
    ratio = g.vec("MULTIPLY", tex.outputs["Color"], g.combine(*(t / max(mu, 1e-3) for t, mu in zip(target, mean))))
    col = g.mixc(0.85, ratio, (*target, 1))
    co = g.texcoord("Object")
    xyz = g.node("ShaderNodeSeparateXYZ", {"Vector": co}).outputs
    e = list(marks["eyes"])
    el, er, rad = e[0:3], e[3:6], e[6]
    for c in (el, er):
        # Blush: a soft pink oval below and out from each eye.
        side = 1.0 if c[0] > 0 else -1.0
        bx = g.mul(g.sub(xyz["X"], c[0] + side * 0.4 * rad), 1.0 / (1.6 * rad))
        bz = g.mul(g.sub(xyz["Z"], c[2] - 2.0 * rad), 1.0 / (0.8 * rad))
        bd = g.vec("LENGTH", g.combine(bx, bz, 0.0))
        col = g.mixc(g.mul(g.smooth(bd, 1.0, 0.2), 0.35), col, (*srgb_to_lin((0.86, 0.42, 0.40)), 1))
        # The lash line: an ellipse round the lid opening, thick above (and out past the corner).
        dx = g.mul(g.sub(xyz["X"], c[0]), 1.0 / (0.85 * rad))
        dz = g.sub(xyz["Z"], c[2])
        d = g.vec("LENGTH", g.combine(dx, g.mul(dz, 1.0 / (0.5 * rad)), 0.0))
        upper = g.smooth(dz, -0.05 * rad, 0.15 * rad)
        hi = g.add(1.02, g.mul(upper, 0.28))
        line = g.mul(g.smooth(d, 0.84, 0.9), g.smooth(d, g.add(hi, 0.06), hi))
        line = g.mul(line, g.add(0.45, g.mul(upper, 0.55)))
        col = g.mixc(line, col, (*srgb_to_lin(INK), 1))
    g.principled(col, 0.0, 0.6)
    return m


def anime_eye_image(path, size=1024):
    """The anime eye, painted on MakeHuman's eye layout (two eyeballs, irises at uv (0.70, 0.70)
    and (0.29, 0.29)): warm white, a big tall iris (dark across its top under the lid, warm amber
    below, a deep rim), a large pupil, two white catch-lights."""
    yy, xx = np.mgrid[0:size, 0:size].astype(np.float32) / size
    v = 1.0 - yy
    rgb = np.ones((size, size, 3), np.float32) * np.array([0.97, 0.94, 0.90], np.float32)
    top, low, rim = np.array([0.16, 0.07, 0.03]), np.array([0.78, 0.42, 0.10]), np.array([0.07, 0.03, 0.02])
    for cu, cv in ((0.703, 0.702), (0.288, 0.292)):
        du, dv = (xx - cu) / 0.13, (v - cv) / 0.16
        r = np.sqrt(du * du + dv * dv)
        iris = np.clip((1.0 - r) / 0.03, 0, 1)
        t = np.clip(dv * 0.5 + 0.5, 0, 1)[..., None]
        col = low * (1 - t) + top * t
        col = col * (1 - np.clip((r - 0.78) / 0.22, 0, 1))[..., None] + rim * np.clip((r - 0.78) / 0.22, 0, 1)[..., None]
        pupil = np.clip((0.42 - np.sqrt((du / 0.85) ** 2 + dv * dv)) / 0.04, 0, 1)[..., None]
        col = col * (1 - pupil) + rim * pupil
        for hu, hv, hr in ((-0.38, 0.40, 0.24), (0.36, -0.42, 0.11)):
            h = np.clip((hr - np.sqrt((du - hu) ** 2 + (dv - hv) ** 2)) / 0.03, 0, 1)[..., None]
            col = col * (1 - h) + np.array([1.0, 1.0, 1.0]) * h
        rgb = rgb * (1 - iris[..., None]) + col * iris[..., None]
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img = bpy.data.images.new(os.path.basename(path), size, size, alpha=True)
    rgba = np.dstack([np.clip(rgb, 0, 1), np.ones((size, size), np.float32)])
    # Painted in sRGB terms; Blender holds a byte image's pixels as they are, bottom row first.
    img.pixels.foreach_set(np.ascontiguousarray(rgba[::-1]).ravel().astype(np.float32))
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()
    return img


def anime_eye_decal(path, w=512, h=384):
    """The painted anime eye for face.anime_eyes' cards, inner corner (u 0) to outer (u 1): an
    almond opening under a high arched upper lid, the white shaded lilac under the lid, a tall
    amber iris (dark across its top) with a deep pupil and two catch-lights, a thick ink lash line
    along the lid flicking up at the outer corner, a short fine one beneath it outside. Alpha
    clear elsewhere (the skin shows)."""
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    u, v = xx / w, 1.0 - yy / h
    px = 1.5 / h
    # Both lids meet at pointed corners (inner low, outer a little higher); the upper arches high,
    # its peak a little outward, the lower is a shallow curve.
    t = np.clip((u - 0.05) / 0.92, 0, 1)
    line = 0.40 + 0.16 * t
    top = line + 0.42 * np.sin(np.pi * t ** 0.85) ** 0.6
    bot = line - 0.24 * np.sin(np.pi * t) ** 0.9
    inside = np.clip((top - v) / px, 0, 1) * np.clip((v - bot) / px, 0, 1) * ((u > 0.05) & (u < 0.97))
    rgb = np.ones((h, w, 3), np.float32) * np.array([0.97, 0.95, 0.92], np.float32)
    lid = np.clip((v - (top - 0.16)) / 0.16, 0, 1)[..., None]
    rgb = rgb * (1 - lid * 0.8) + np.array([0.70, 0.66, 0.86]) * lid * 0.8
    du, dv = (u - 0.52) / 0.21, (v - 0.47) / 0.33
    r = np.sqrt(du * du + dv * dv)
    iris = np.clip((1.0 - r) * 40, 0, 1)[..., None]
    t = np.clip(dv * 0.5 + 0.5, 0, 1)[..., None]
    icol = np.array([0.86, 0.48, 0.12]) * (1 - t) + np.array([0.20, 0.08, 0.03]) * t
    ring = np.clip((r - 0.8) / 0.2, 0, 1)[..., None]
    icol = icol * (1 - ring) + np.array([0.08, 0.035, 0.02]) * ring
    pupil = np.clip((0.4 - np.sqrt((du / 0.8) ** 2 + dv * dv)) * 30, 0, 1)[..., None]
    icol = icol * (1 - pupil) + np.array([0.05, 0.02, 0.015]) * pupil
    for hu, hv, hr in ((-0.35, 0.38, 0.26), (0.34, -0.4, 0.12)):
        hl = np.clip((hr - np.sqrt((du - hu) ** 2 + (dv - hv) ** 2)) * 30, 0, 1)[..., None]
        icol = icol * (1 - hl) + hl
    rgb = rgb * (1 - iris) + icol * iris
    # The lash line over the lid, thickening outward, flicking up past the outer corner.
    thick = 0.05 + 0.06 * u
    lash = np.clip((v - (top - 0.02)) / px, 0, 1) * np.clip((top + thick - v) / px, 0, 1) * np.clip((u - 0.02) / 0.03, 0, 1)
    lash = lash * (u < 0.98)
    # The flick: on past the outer corner, up and out.
    fu = np.clip((u - 0.9) / 0.1, 0, 1)
    flick = np.clip((0.03 * (1.0 - fu) + 0.006 - np.abs(v - (0.56 + 0.02 + fu * 0.12))) / px, 0, 1) * (u > 0.9)
    lower = np.clip((bot - v) / px, 0, 1) * np.clip((v - (bot - 0.022)) / px, 0, 1) * np.clip((u - 0.55) / 0.08, 0, 1) * (u < 0.95)
    ink = np.clip(lash + flick + lower, 0, 1)
    rgb = rgb * (1 - ink[..., None]) + np.array(INK) * ink[..., None]
    alpha = np.clip(inside + ink, 0, 1)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img = bpy.data.images.new(os.path.basename(path), w, h, alpha=True)
    rgba = np.dstack([np.clip(rgb, 0, 1), alpha])
    img.pixels.foreach_set(np.ascontiguousarray(rgba[::-1]).ravel().astype(np.float32))
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()
    return img


def anime_hair_material(name, ring=True):
    """Anime hair: flat near-black brown; on the crown's shell (`ring`), a lighter, cooler band
    where the sky catches it (the "angel ring"), soft-edged in height. The locks are plain."""
    m = new_material(name)
    g = Graph(m)
    base = (*srgb_to_lin((0.10, 0.06, 0.055)), 1)
    if not ring:
        g.principled(base, 0.0, 0.5)
        return m
    co = g.texcoord("Object")
    z = g.node("ShaderNodeSeparateXYZ", {"Vector": co}).outputs["Z"]
    band = g.band(z, ANIME_RING[0], ANIME_RING[1], 0.005)
    col = g.mixc(g.mul(band, 0.7), base, (*srgb_to_lin((0.26, 0.19, 0.18)), 1))
    g.principled(col, 0.0, 0.5)
    return m


# The angel ring's height (m, rest pose), set from the head when the hair is built (face.anime_hair).
ANIME_RING = [1.55, 1.57]


def decal_material(name, img):
    """An alpha-tested card showing `img` as painted, matte (the anime eyes)."""
    m = new_material(name)
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="UVMap").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=img)
    p = g.principled(tex.outputs["Color"], 0.0, 0.5)
    g.set(p.inputs["Alpha"], g.math("GREATER_THAN", tex.outputs["Alpha"], 0.5))
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
    if pattern == "flat":
        # The anime character's: one flat colour (cel shading draws the folds), its border only.
        col = (*base, 1)
        rough = g.value(0.8)
    elif pattern in ("check", "check_fine"):
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
    normal = None if pattern == "flat" else g.bump(g.add(g.mul(weave, 0.5), g.mul(slub, 0.5)), 0.05, 0.0004)
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
    # Strand shading kept moderate round the texture's own mean (±55%), so light strands don't
    # turn into pale stripes: hair reads as a dark mass with sheen, not a painted sheet.
    seen = px[:, 3] > 0.5
    mean = float(lum[seen].mean()) if seen.any() else 0.5
    shade = np.clip(1.0 + 0.55 * (lum / max(mean, 1e-3) - 1.0), 0.45, 1.45)
    px[:, :3] = px[:, :3] * (1 - strength) + np.outer(shade, tint) * strength
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
    # Soft sheen, not a plastic gloss.
    p = g.principled(tex.outputs["Color"], 0.0, 0.62)
    cut = g.math("GREATER_THAN", tex.outputs["Alpha"], 0.4)
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


def outline(lod, rig, width):
    """`lod`'s inverted hull: a copy `width` out along its normals with its faces reversed, in the
    one `toon_outline` material (drawn flat ink in game, crate::rim)."""
    import bmesh
    o = lod.copy()
    o.data = lod.data.copy()
    o.name = o.data.name = lod.name.replace("_LOD", "_outline_LOD")
    bpy.context.scene.collection.objects.link(o)
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bm.normal_update()
    for v in bm.verts:
        v.co += v.normal * width
    bmesh.ops.reverse_faces(bm, faces=bm.faces)
    bm.to_mesh(o.data)
    bm.free()
    o.data.materials.clear()
    m = new_material("toon_outline")
    g = Graph(m)
    g.principled((*srgb_to_lin(INK), 1), 0.0, 1.0)
    o.data.materials.append(m)
    o.parent = rig
    return o


def mh_material(name, root, variant=None):
    """A MakeHuman library garment in its own textures: its colour (a cleaned copy from
    tools/textures/mh_clothes.py when there is one) and its normal map, so the bake carries its
    sculpted folds into the atlas."""
    lib = os.path.expanduser(f"~/.config/blender/5.2/mpfb/data/clothes/{name}")
    cleaned = os.path.join(root, "build", "clothes", f"{name}_diffuse.png")
    # The anime character's flat recolour (tools/textures/mh_clothes.py), without the folds' normal
    # map: cel shading draws the form.
    flat = os.path.join(root, "build", "clothes", f"anime_{name}_diffuse.png")
    if variant == ANIME and os.path.exists(flat):
        cleaned = flat
    diffuse = bpy.data.images.load(cleaned if os.path.exists(cleaned) else os.path.join(lib, f"{name}_diffuse.png"))
    m = new_material(f"mh_{name}")
    g = Graph(m)
    uv = g.node("ShaderNodeUVMap", uv_map="src").outputs["UV"]
    tex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=diffuse)
    # Not every garment has a normal map (the shoes don't).
    nmap = None
    path = os.path.join(lib, f"{name}_normal.png")
    if os.path.exists(path) and variant != ANIME:
        normal = bpy.data.images.load(path)
        normal.colorspace_settings.name = "Non-Color"
        ntex = g.node("ShaderNodeTexImage", {"Vector": uv}, image=normal)
        nmap = g.node("ShaderNodeNormalMap", {"Color": ntex.outputs["Color"]}, uv_map="src").outputs["Normal"]
    g.principled(tex.outputs["Color"], 0.0, 0.85, nmap)
    return m


def finish_character(variant, rig, proxy, parts, items, root):
    # --- materials per source object
    skin_img = find_image(proxy)
    proxy.data.materials.clear()
    marks = {"beard": "beard" in proxy.data.attributes, "eyes": proxy.get("eyes"), "bindi": proxy.get("bindi")}
    anime = variant == ANIME
    proxy.data.materials.append(anime_skin_material(skin_img, marks) if anime else skin_material(variant, skin_img, marks))
    eyes = parts["eyes"]
    eye_img = anime_eye_image(os.path.join(root, "build", "textures", variant, "anime_eye.png")) if anime else find_image(eyes)
    eyes.data.materials.clear()
    eyes.data.materials.append(textured_material(f"{variant}_eyes", eye_img, 0.35) if anime else eye_material(f"{variant}_eyes", eye_img))
    opaque = [proxy, eyes]
    decals = [obj for obj, _ in items if obj.get("kind") == "decal"]
    for obj, role in items:
        kind = obj.get("kind", "cloth")
        if kind == "decal":
            continue
        obj.data.materials.clear()
        if kind == "gold":
            obj.data.materials.append(metal_material("gold", (1.0, 0.76, 0.36)))
        elif kind == "silver":
            obj.data.materials.append(metal_material("silver", (0.86, 0.87, 0.89), 0.25))
        elif kind == "hairmesh":
            obj.data.materials.append(hair_mesh_material(obj.name))
        elif kind == "toonhair":
            cap = obj.name.startswith("anime_hair_cap")
            obj.data.materials.append(anime_hair_material("anime_hair_cap" if cap else "anime_hair", ring=cap))
        elif kind == "lacquer":
            obj.data.materials.append(metal_material("lacquer", (0.62, 0.06, 0.08), 0.18, 0.0))
        elif kind == "mh":
            obj.data.materials.append(mh_material(obj.name, root, variant))
        else:
            key = next((k for k in CLOTH if obj.name.startswith(k)), "dhoti")
            base, border, pattern, bw = CLOTH[key]
            from garments import edge_distance_attr
            edge_distance_attr(obj)
            obj.data.materials.append(cloth_material(key, base, border, pattern, bw))
        opaque.append(obj)

    from mathutils.kdtree import KDTree
    for o in opaque:
        # The garment as grown, before its thickness is applied: Solidify moves a vertex millimetres,
        # but at a sharp degenerate corner (an armpit) it can fling one a metre out. Anything that
        # ends up over 5 cm from the shape it was grown as goes back to its nearest original point.
        kd = None
        if anime and o is not proxy and any(m.type == "SOLIDIFY" for m in o.modifiers):
            kd = KDTree(len(o.data.vertices))
            for v in o.data.vertices:
                kd.insert(v.co, v.index)
            kd.balance()
        apply_non_armature_modifiers(o)
        if kd is not None:
            pulled = 0
            for v in o.data.vertices:
                near, _, d = kd.find(v.co)
                if d > 0.05:
                    v.co = near
                    pulled += 1
            if pulled:
                print(f"[{variant}] {o.name}: {pulled} stray vertices pulled back")
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
    body.data.materials.append(export.game_material(toon_name(variant, variant), images))
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
        o.data.materials.append(alpha_card_material(toon_name(variant, f"{variant}_{key}"), img, tint, strength))
        apply_non_armature_modifiers(o)
        if key == "hair":
            # Hair sways like the cloth (crate::rim): still at the scalp, free toward the tips,
            # long hair most.
            hair_flex(o)
        cards.append(o)

    # The anime eyes: painted cards, alpha-tested.
    for o in decals:
        img = anime_eye_decal(os.path.join(root, "build", "textures", variant, "anime_eye_decal.png"))
        o.data.materials.clear()
        o.data.materials.append(decal_material(toon_name(variant, f"{variant}_eyes"), img))
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
    if variant == ANIME:
        # The ink outline: each near LOD's copy pushed out along its normals and turned inside
        # out (only its far side draws, a rim round the silhouette), skinned as the body is.
        lods += [outline(lod, rig, w) for lod, w in zip(lods[:2], (0.006, 0.01))]
    out = os.path.join(root, "assets", "models", "characters", f"{variant}.glb")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    # The weapon bone a long gun rides (skeleton.py), in the saved rig the clips are made on too.
    import skeleton
    skeleton.add_weapon_bone(rig)
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

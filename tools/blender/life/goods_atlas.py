"""The goods' one texture (assets/models/life/goods_atlas.png): a grid of small tiles, so the whole
market is one material. Heaps of produce rendered from above out of hundreds of real little fruit,
cloth from Poly Haven's CC0 fabric scans (build/life_src/fabric, see docs/references.md), and plain
worked surfaces drawn from noise: tarp, cardboard, wood, steel, spice.

blender -b --factory-startup -P tools/blender/life/goods_atlas.py
"""

import math
import os
import random

import bpy
import numpy as np
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
FABRIC = os.path.join(ROOT, "build", "life_src", "fabric")
TILES = os.path.join(ROOT, "build", "life_src", "tiles")
OUT = os.path.join(ROOT, "assets", "models", "life", "goods_atlas.png")
TILE = 170
SIDE = 1024
COLS = 6

rng = random.Random(11)
np_rng = np.random.default_rng(11)

# The order goods.py finds them by.
NAMES = [
    "white", "orange", "apple", "tomato", "onion", "potato",
    "guava", "banana", "cauli", "greens", "coconut", "brinjal",
    "chilli", "turmeric", "coriander", "rice", "lentil", "jute",
    "gingham", "floral", "linen", "quatrefoil", "poplin", "denim",
    "terlenka", "tarp", "card", "wood", "steel", "plastic",
]


def fbm(n, octaves=5, start=4, gain=0.55, stretch=(1, 1)):
    """Value noise summed over octaves, 0 to 1, n x n."""
    out = np.zeros((n, n))
    amp, total = 1.0, 0.0
    for k in range(octaves):
        cells = (start * 2 ** k * stretch[0], start * 2 ** k * stretch[1])
        g = np_rng.random((int(cells[0]) + 2, int(cells[1]) + 2))
        y = np.linspace(0, cells[0], n, endpoint=False)
        x = np.linspace(0, cells[1], n, endpoint=False)
        yi, xi = y.astype(int), x.astype(int)
        yf, xf = (y - yi)[:, None], (x - xi)[None, :]
        yf, xf = yf * yf * (3 - 2 * yf), xf * xf * (3 - 2 * xf)
        a = g[yi][:, xi]
        b = g[yi][:, xi + 1]
        c = g[yi + 1][:, xi]
        d = g[yi + 1][:, xi + 1]
        out += amp * (a * (1 - xf) * (1 - yf) + b * xf * (1 - yf) + c * (1 - xf) * yf + d * xf * yf)
        total += amp
        amp *= gain
    return out / total


def powder(rgb, spread=0.2):
    """Ground spice or a heap of grain: a dry mound, fine and uneven."""
    base = np.array(rgb)
    fine = fbm(TILE, 4, 40)
    big = fbm(TILE, 3, 3)
    k = 0.72 + 0.56 * (fine * 0.7 + big * 0.3)
    speck = (np_rng.random((TILE, TILE)) - 0.5) * spread
    return np.clip(base[None, None, :] * (k + speck)[:, :, None], 0, 1)


def grains(rgb, rgb2, size=2):
    """Small separate grains: rice, lentils."""
    n = TILE // size + 1
    pick = np_rng.random((n, n))
    pick = np.kron(pick, np.ones((size, size)))[:TILE, :TILE]
    a, b = np.array(rgb), np.array(rgb2)
    shade = 0.76 + 0.3 * fbm(TILE, 3, 6)
    return np.clip((a[None, None, :] * (1 - pick[:, :, None]) + b[None, None, :] * pick[:, :, None]) * shade[:, :, None], 0, 1)


def plain_tiles():
    t = {}
    t["white"] = np.ones((TILE, TILE, 3))
    t["chilli"] = powder((0.60, 0.12, 0.06))
    t["turmeric"] = powder((0.82, 0.60, 0.10))
    t["coriander"] = powder((0.45, 0.34, 0.17))
    t["rice"] = grains((0.82, 0.79, 0.72), (0.92, 0.90, 0.84))
    t["lentil"] = grains((0.66, 0.40, 0.14), (0.78, 0.50, 0.20), 3)
    # Tarp: pale, woven, creased: the vertex colour says what colour it is.
    weave = (np.indices((TILE, TILE)).sum(0) % 2) * 0.03
    crease = fbm(TILE, 5, 3, 0.6)
    t["tarp"] = np.repeat((0.62 + 0.55 * crease + weave)[:, :, None], 3, axis=2).clip(0, 1)
    # Cardboard: brown, faint flutes, scuffed.
    flutes = 0.95 + 0.05 * np.sin(np.arange(TILE) * 0.9)[None, :]
    scuff = fbm(TILE, 5, 5)
    card = np.array((0.50, 0.37, 0.24))[None, None, :] * (0.72 + 0.5 * scuff)[:, :, None] * flutes[:, :, None]
    t["card"] = card.clip(0, 1)
    # Wood: slats along, each its own shade, grain running through.
    grain = fbm(TILE, 5, 3, 0.6, (5, 0.5))
    slats = np.repeat(np_rng.random(6), TILE // 6 + 1)[:TILE][:, None]
    gaps = (np.arange(TILE) % (TILE // 6) < 2)[:, None] * 0.55
    wood = np.array((0.42, 0.29, 0.17))[None, None, :] * ((0.6 + 0.7 * grain) * (0.75 + 0.45 * slats) * (1 - gaps))[:, :, None]
    t["wood"] = wood.clip(0, 1)
    # Steel: brushed, with soft bright streaks.
    streak = fbm(TILE, 5, 3, 0.6, (0.4, 7))
    t["steel"] = np.repeat((0.42 + 0.62 * streak)[:, :, None], 3, axis=2).clip(0, 1)
    # Plastic: nearly plain, a little scuffed, tinted by the vertex colour.
    t["plastic"] = np.repeat((0.84 + 0.2 * fbm(TILE, 4, 5))[:, :, None], 3, axis=2).clip(0, 1)
    return t


def box_shrink(a, n):
    """Averages an image down to n x n, the middle square of it."""
    h, w = a.shape[:2]
    side = min(h, w)
    a = a[(h - side) // 2:(h - side) // 2 + side, (w - side) // 2:(w - side) // 2 + side]
    f = side // n
    a = a[: f * n, : f * n]
    return a.reshape(n, f, n, f, a.shape[2]).mean(axis=(1, 3))


def fabric(name, tile):
    """A Poly Haven scan, shrunk to a tile: a metre or so of cloth."""
    img = bpy.data.images.load(os.path.join(FABRIC, name + ".jpg"))
    w, h = img.size
    px = np.array(img.pixels[:]).reshape(h, w, 4)[::-1, :, :3]
    bpy.data.images.remove(img)
    return box_shrink(px, tile)


def shrunk_png(path):
    img = bpy.data.images.load(path)
    px = np.array(img.pixels[:]).reshape(TILE * 2, TILE * 2, 4)[::-1, :, :3]
    bpy.data.images.remove(img)
    return box_shrink(px, TILE)


def render_pile(name, make, ground, span=0.7):
    """A heap seen from straight above, out of hundreds of small things."""
    path = os.path.join(TILES, name + ".png")
    if os.path.exists(path):
        return shrunk_png(path)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    world = bpy.data.worlds.new("w")
    scene.world = world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (1, 1, 1, 1)
    world.node_tree.nodes["Background"].inputs[1].default_value = 0.9
    sun = bpy.data.lights.new("s", "SUN")
    sun.energy = 2.6
    so = bpy.data.objects.new("s", sun)
    so.rotation_euler = (0.55, 0.3, 0.9)
    scene.collection.objects.link(so)
    mat = bpy.data.materials.new("m")
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    info = nt.nodes.new("ShaderNodeObjectInfo")
    nt.links.new(info.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.42
    bpy.ops.mesh.primitive_plane_add(size=span * 3, location=(0, 0, -0.1))
    plane = bpy.context.active_object
    pm = bpy.data.materials.new("p")
    pm.use_nodes = True
    pm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*ground, 1)
    plane.data.materials.append(pm)
    make(scene, mat, span)
    cam = bpy.data.cameras.new("c")
    cam.type = "ORTHO"
    cam.ortho_scale = span
    co = bpy.data.objects.new("c", cam)
    co.location = (0, 0, 2)
    scene.collection.objects.link(co)
    scene.camera = co
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 28
    scene.cycles.device = "CPU"
    scene.render.resolution_x = scene.render.resolution_y = TILE * 2
    scene.view_settings.view_transform = "Standard"
    os.makedirs(TILES, exist_ok=True)
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    return shrunk_png(path)


def scatter(scene, mat, span, shape, n, r, colours, squash=(1, 1, 1), layers=3, jitter=0.1):
    """`n` things dropped over the view in loose layers, each its own colour and turn."""
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=3, radius=1)
    src = bpy.context.active_object
    src.data.materials.append(mat)
    for p in src.data.polygons:
        p.use_smooth = True
    if shape == "lumpy":
        for v in src.data.vertices:
            k = 1 + 0.12 * math.sin(v.co.x * 5 + 1) * math.sin(v.co.y * 4) + 0.08 * math.sin(v.co.z * 7)
            v.co *= k
    hide = src
    for k in range(n):
        o = bpy.data.objects.new(f"o{k}", src.data)
        scene.collection.objects.link(o)
        layer = k % layers
        o.location = (rng.uniform(-span * 0.62, span * 0.62), rng.uniform(-span * 0.62, span * 0.62), layer * r * 0.75)
        s = r * rng.uniform(0.85, 1.15)
        o.scale = (s * squash[0], s * squash[1], s * squash[2])
        o.rotation_euler = (rng.uniform(0, 6.28), rng.uniform(0, 6.28), rng.uniform(0, 6.28))
        c = rng.choice(colours)
        k2 = 1 + rng.uniform(-jitter, jitter)
        o.color = (c[0] * k2, c[1] * k2 * (1 + rng.uniform(-jitter, jitter) * 0.6), c[2] * k2, 1)
    hide.hide_render = True


def lin(c):
    return tuple(v ** 2.2 for v in c)


def piles():
    t = {}
    dark = (0.02, 0.016, 0.012)
    t["orange"] = render_pile("orange", lambda s, m, w: scatter(s, m, w, "ball", 230, 0.04, [lin((0.86, 0.50, 0.10)), lin((0.82, 0.42, 0.08))]), dark)
    t["apple"] = render_pile("apple", lambda s, m, w: scatter(s, m, w, "ball", 220, 0.042, [lin((0.62, 0.08, 0.09)), lin((0.50, 0.06, 0.08)), lin((0.72, 0.34, 0.14))]), dark)
    t["tomato"] = render_pile("tomato", lambda s, m, w: scatter(s, m, w, "ball", 300, 0.032, [lin((0.72, 0.13, 0.06)), lin((0.62, 0.10, 0.05)), lin((0.76, 0.30, 0.08))], (1, 1, 0.8)), dark)
    t["onion"] = render_pile("onion", lambda s, m, w: scatter(s, m, w, "ball", 260, 0.036, [lin((0.60, 0.26, 0.26)), lin((0.72, 0.42, 0.32)), lin((0.52, 0.20, 0.24))], (1, 1, 0.82)), dark)
    t["potato"] = render_pile("potato", lambda s, m, w: scatter(s, m, w, "lumpy", 200, 0.046, [lin((0.58, 0.44, 0.28)), lin((0.50, 0.38, 0.24)), lin((0.64, 0.50, 0.34))], (1.3, 0.9, 0.8), jitter=0.16), dark)
    t["guava"] = render_pile("guava", lambda s, m, w: scatter(s, m, w, "ball", 230, 0.038, [lin((0.60, 0.68, 0.30)), lin((0.72, 0.74, 0.34)), lin((0.50, 0.60, 0.26))]), dark)
    t["banana"] = render_pile("banana", lambda s, m, w: scatter(s, m, w, "ball", 240, 0.026, [lin((0.84, 0.70, 0.18)), lin((0.76, 0.66, 0.20)), lin((0.62, 0.62, 0.20))], (4.2, 0.95, 0.9), jitter=0.06), dark)
    t["cauli"] = render_pile("cauli", lambda s, m, w: scatter(s, m, w, "lumpy", 190, 0.048, [lin((0.84, 0.82, 0.70)), lin((0.76, 0.74, 0.60)), lin((0.28, 0.42, 0.20))], (1, 1, 0.9), jitter=0.08), (0.05, 0.08, 0.03))
    t["greens"] = render_pile("greens", lambda s, m, w: scatter(s, m, w, "ball", 640, 0.024, [lin((0.20, 0.40, 0.12)), lin((0.16, 0.32, 0.10)), lin((0.28, 0.46, 0.14))], (1.9, 0.8, 0.22), layers=5, jitter=0.2), (0.015, 0.03, 0.01))
    t["coconut"] = render_pile("coconut", lambda s, m, w: scatter(s, m, w, "lumpy", 90, 0.07, [lin((0.30, 0.20, 0.12)), lin((0.24, 0.16, 0.10)), lin((0.36, 0.26, 0.16))], (1.1, 1, 0.95), jitter=0.2), dark)
    t["brinjal"] = render_pile("brinjal", lambda s, m, w: scatter(s, m, w, "ball", 180, 0.04, [lin((0.20, 0.08, 0.22)), lin((0.16, 0.06, 0.18)), lin((0.24, 0.10, 0.26))], (1.5, 0.9, 0.9), jitter=0.1), dark)
    return t


def main():
    tiles = plain_tiles()
    tiles.update(piles())
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.view_settings.view_transform = "Standard"
    scene.render.image_settings.color_mode = "RGB"
    for name, file in (("jute", "hessian_230"), ("gingham", "gingham_check"), ("floral", "floral_jacquard"), ("linen", "rough_linen"), ("quatrefoil", "quatrefoil_jacquard_fabric"),
                       ("poplin", "stretch_poplin"), ("denim", "denim_fabric"), ("terlenka", "terlenka")):
        tiles[name] = fabric(file, TILE)
    atlas = np.ones((SIDE, SIDE, 4))
    for k, name in enumerate(NAMES):
        col, row = k % COLS, k // COLS
        x, y = col * TILE, row * TILE
        px = tiles[name]
        atlas[y:y + TILE, x:x + TILE, :3] = np.clip(px, 0, 1)
    # Bottom row first in Blender's pixels.
    img = bpy.data.images.new("goods_atlas", SIDE, SIDE, alpha=False)
    img.colorspace_settings.name = "Non-Color"
    img.pixels.foreach_set(atlas[::-1].ravel().astype(np.float32))
    img.update()
    img.file_format = "PNG"
    img.save_render(OUT)
    print(f"[goods_atlas] {OUT}: {len(NAMES)} tiles, {os.path.getsize(OUT) // 1024} KB")


main()

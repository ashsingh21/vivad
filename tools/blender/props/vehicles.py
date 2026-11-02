"""The drivable vehicles: free models from research/vehicles/models, made ready for the game.

Each model is turned to face +Y (the game's -Z), stood on the ground, scaled to its real length,
cleared of number plates and lettering, reduced if heavy, and written to
assets/models/vehicles/<name>.glb with one mesh `body` and a mesh for each wheel that could be found
(`wheel_f`, `wheel_fl`, `wheel_fr`, `wheel_r`, `wheel_rl`, `wheel_rr`), origin on its axle, so the game can spin them.
The `gaadi_` models also go through what their rows ask for: parts stripped, a right-hand drive, reduction
part by part, one baked texture set, weathering.

blender -b --factory-startup -P tools/blender/props/vehicles.py -- [--render <dir>] [names...]

All are CC-BY: docs/references.md carries the credits.
"""

import colorsys
import math
import os
import re
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
# They are not in git: VIVAD_MODELS says where they are, from a worktree without them.
MODELS = os.environ.get("VIVAD_MODELS", os.path.join(ROOT, "research", "vehicles", "models"))
OUT = os.path.join(ROOT, "assets", "models", "vehicles")


def hsv_arrays(rgb):
    mx, mn = rgb.max(1), rgb.min(1)
    d = mx - mn + 1e-6
    r, g, b = rgb[:, 0], rgb[:, 1], rgb[:, 2]
    h = np.where(mx == r, ((g - b) / d) % 6, np.where(mx == g, (b - r) / d + 2, (r - g) / d + 4)) / 6
    return h, d / (mx + 1e-6), mx


def rgb_arrays(h, s, v):
    i = np.floor(h * 6) % 6
    f = h * 6 - np.floor(h * 6)
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    out = np.zeros((len(h), 3))
    for k, (a, b, c) in enumerate(((v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q))):
        m = i == k
        out[m, 0], out[m, 1], out[m, 2] = a[m], b[m], c[m]
    return out


def pixels(img):
    w, h = img.size
    a = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(h, w, 4)


def put_pixels(img, a):
    img.pixels.foreach_set(a.reshape(-1))
    img.update()


def image_named(part):
    if part in bpy.data.images:
        return bpy.data.images[part]
    for i in bpy.data.images:
        if part.lower() in i.name.lower():
            return i
    raise KeyError(part)


def clone(a, box, dx, dy, soft=8, oval=False):
    """Cover a box (x0, y0, x1, y1, from the top left of the texture) with the pixels (dx, dy) from it,
    so what is painted over keeps its grain, fading in over `soft` pixels inside the box's edge."""
    x0, y0, x1, y1 = box
    h = a.shape[0]
    ys, xs = np.mgrid[y0:y1, x0:x1]
    if oval:
        k = np.clip((1 - np.hypot((xs + 0.5 - (x0 + x1) / 2) * 2 / (x1 - x0), (ys + 0.5 - (y0 + y1) / 2) * 2 / (y1 - y0))) * (x1 - x0) / 2 / soft, 0, 1)
    else:
        k = np.clip(np.minimum.reduce((xs - x0, x1 - 1 - xs, ys - y0, y1 - 1 - ys)) / soft, 0, 1)
    k = k[::-1, :, None]
    to = a[h - y1 : h - y0, x0:x1, :3]
    to[:] = to * (1 - k) + a[h - y1 - dy : h - y0 - dy, x0 + dx : x1 + dx, :3] * k


def even(a, mask, colour, reach=40, keep=0.2):
    """Paint of one colour on a model textured piece by piece. Each pixel of `mask` moves by how far the
    area round it stands from `colour`, so scratches and grime stay and the panels come to match."""
    def blur(x):
        c = np.cumsum(np.cumsum(x, 0), 1)
        c = np.pad(c, ((1, 0), (1, 0), (0, 0)))
        h, w = x.shape[:2]
        y0, y1 = np.clip(np.arange(h) - reach, 0, h), np.clip(np.arange(h) + reach + 1, 0, h)
        x0, x1 = np.clip(np.arange(w) - reach, 0, w), np.clip(np.arange(w) + reach + 1, 0, w)
        return c[y1][:, x1] - c[y0][:, x1] - c[y1][:, x0] + c[y0][:, x0]

    m = mask[..., None].astype(np.float64)
    near = blur(a[..., :3].astype(np.float64) * m) / np.maximum(blur(m), 1)
    gain = (np.array(colour) / np.maximum(near, 0.01)) ** (1 - keep)
    a[mask, :3] = np.clip(a[..., :3] * gain, 0, 1)[mask]


def texels(img, o, keep):
    """The pixels of `img` under the faces of `o` whose points all pass `keep` (an array of xyz in, an
    array of bools out): a part of the model found by where it is, not by where its texture happens to be."""
    w, h = img.size
    co = np.empty(len(o.data.vertices) * 3)
    o.data.vertices.foreach_get("co", co)
    ok = keep(co.reshape(-1, 3))
    uv = np.empty(len(o.data.loops) * 2)
    o.data.uv_layers.active.data.foreach_get("uv", uv)
    uv = uv.reshape(-1, 2) * (w, h)
    mask = np.zeros((h, w), dtype=bool)
    for p in o.data.polygons:
        if not ok[list(p.vertices)].all():
            continue
        pts = uv[list(p.loop_indices)]
        for i in range(1, len(pts) - 1):
            a, b, c = pts[0], pts[i], pts[i + 1]
            x0, y0 = np.floor(np.minimum(np.minimum(a, b), c)).astype(int) - 1
            x1, y1 = np.ceil(np.maximum(np.maximum(a, b), c)).astype(int) + 2
            x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, w), min(y1, h)
            if x0 >= x1 or y0 >= y1:
                continue
            xs, ys = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
            cross = lambda p, q: (q[0] - p[0]) * (ys - p[1]) - (q[1] - p[1]) * (xs - p[0])
            d = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
            if abs(d) < 1e-9:
                continue
            s = np.sign(d)
            # A pixel's width outside the edge counts too, or filtering shows the old colour.
            m = 1.5 * np.array([np.hypot(*(q - p)) for p, q in ((a, b), (b, c), (c, a))])
            mask[y0:y1, x0:x1] |= (s * cross(a, b) >= -m[0]) & (s * cross(b, c) >= -m[1]) & (s * cross(c, a) >= -m[2])
    return mask


def in_box(*box):
    lo, hi = np.array(box[:3]), np.array(box[3:])
    return lambda co: ((co >= lo) & (co <= hi)).all(1)


def spots(img, o):
    """Where each pixel of `img` lies on the model: an array of xyz, NaN off it."""
    w, h = img.size
    co = np.empty(len(o.data.vertices) * 3)
    o.data.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    uv = np.empty(len(o.data.loops) * 2)
    o.data.uv_layers.active.data.foreach_get("uv", uv)
    uv = uv.reshape(-1, 2) * (w, h)
    at = np.full((h, w, 3), np.nan)
    for p in o.data.polygons:
        pts, xyz = uv[list(p.loop_indices)], co[list(p.vertices)]
        for i in range(1, len(pts) - 1):
            t = pts[[0, i, i + 1]]
            d = (t[1, 0] - t[0, 0]) * (t[2, 1] - t[0, 1]) - (t[1, 1] - t[0, 1]) * (t[2, 0] - t[0, 0])
            if abs(d) < 1e-9:
                continue
            x0, y0 = np.floor(t.min(0)).astype(int) - 1
            x1, y1 = np.ceil(t.max(0)).astype(int) + 2
            x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, w), min(y1, h)
            if x0 >= x1 or y0 >= y1:
                continue
            xs, ys = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
            b1 = ((xs - t[0, 0]) * (t[2, 1] - t[0, 1]) - (ys - t[0, 1]) * (t[2, 0] - t[0, 0])) / d
            b2 = ((t[1, 0] - t[0, 0]) * (ys - t[0, 1]) - (t[1, 1] - t[0, 1]) * (xs - t[0, 0])) / d
            b0 = 1 - b1 - b2
            m = (b0 > -0.02) & (b1 > -0.02) & (b2 > -0.02)
            at[y0:y1, x0:x1][m] = (b0[..., None] * xyz[0] + b1[..., None] * xyz[i] + b2[..., None] * xyz[i + 1])[m]
    return at


def noise(h, w, cell, rng):
    """Smooth noise, 0 to 1, in blobs `cell` pixels across."""
    g = rng.random((h // cell + 2, w // cell + 2))
    ys, xs = np.arange(h) / cell, np.arange(w) / cell
    y0, x0 = ys.astype(int), xs.astype(int)
    fy, fx = (ys - y0)[:, None], (xs - x0)
    fy, fx = fy * fy * (3 - 2 * fy), fx * fx * (3 - 2 * fx)
    return (g[y0][:, x0] * (1 - fx) + g[y0][:, x0 + 1] * fx) * (1 - fy) + (g[y0 + 1][:, x0] * (1 - fx) + g[y0 + 1][:, x0 + 1] * fx) * fy


def grime(h, w, seed=7):
    rng = np.random.default_rng(seed)
    return 0.6 * noise(h, w, 90, rng) + 0.3 * noise(h, w, 24, rng) + 0.1 * noise(h, w, 6, rng)


def flat(name, colour, rough, metal=0.0):
    """A plain material."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*colour, 1.0)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = metal
    return m


def road_dust(a, at, mask, colour=(0.47, 0.42, 0.35), high=0.7, amount=0.8):
    """Years of Delhi's roads: dust thick at the sills, thinning up the body, in blotches."""
    n = grime(*a.shape[:2])
    low = np.nan_to_num(np.clip((high - at[..., 2]) / (high - 0.15), 0, 1))
    k = (low * amount * (0.3 + 1.1 * n) + 0.1 * n)[..., None]
    a[..., :3] = np.where(mask[..., None], a[..., :3] * (1 - k) + np.array(colour) * k, a[..., :3])


# The line a Delhi auto carries on the back of its hood: "Ghar kab aaoge", when will you come home
# (assets/data/lines.ron, paint.1), white on black in Mukta Bold. Where it stands on the hood, seen
# from behind, in metres, under the rear window, and the red of the paint.
HOOD_LINE = os.path.join(os.path.dirname(__file__), "hood_line.png")
HOOD_BOX = ((-0.40, 0.40), (1.07, 1.25))
# The back of the hood lies behind this. In front of it is the cloth's inside, seen through the rear glass.
HOOD_BACK = -1.0
HOOD_RED = np.array((0.56, 0.02, 0.02))


def hood_line(img, material):
    """Stencils the line straight on from behind, onto every part of `material` that faces back: it
    reads level whatever the cloth's texture coordinates do, and its folds still show through."""
    stencil = pixels(bpy.data.images.load(HOOD_LINE))[..., 0]
    sh, sw = stencil.shape
    a = pixels(img)
    h, w = a.shape[:2]
    (x0, x1), (z0, z1) = HOOD_BOX
    painted = 0
    for o in meshes():
        i = o.data.materials.find(material.name)
        if i < 0:
            continue
        me = o.data
        me.calc_loop_triangles()
        uv = me.uv_layers.active.data
        for t in me.loop_triangles:
            if t.material_index != i or t.normal.y > -0.5:
                continue
            co = np.array([me.vertices[v].co[:] for v in t.vertices])
            st = np.array([uv[k].uv[:] for k in t.loops]) * (w, h)
            m = np.array([st[1] - st[0], st[2] - st[0]]).T
            if abs(np.linalg.det(m)) < 1e-6:
                continue
            lo, hi = np.floor(st.min(0)).astype(int), np.ceil(st.max(0)).astype(int) + 1
            xs, ys = (g.ravel() for g in np.meshgrid(np.arange(lo[0], hi[0]), np.arange(lo[1], hi[1])))
            b = np.linalg.solve(m, np.stack([xs + 0.5 - st[0, 0], ys + 0.5 - st[0, 1]]))
            p = co[0] + np.outer(b[0], co[1] - co[0]) + np.outer(b[1], co[2] - co[0])
            s, r = (p[:, 0] - x0) / (x1 - x0), (p[:, 2] - z0) / (z1 - z0)
            on = (b[0] >= -0.02) & (b[1] >= -0.02) & (b[0] + b[1] <= 1.02) & (s >= 0) & (s < 1) & (r >= 0) & (r < 1) & (p[:, 1] < HOOD_BACK)
            if not on.any():
                continue
            col, row = np.mod(xs[on], w), np.mod(ys[on], h)
            ink = stencil[(r[on] * sh).astype(int), (s[on] * sw).astype(int)][:, None] * 0.97
            fold = np.clip(a[row, col, 1:2] / 0.62, 0.7, 1.1)
            a[row, col, :3] = a[row, col, :3] * (1 - ink) + HOOD_RED * fold * ink
            painted += int(on.sum())
    put_pixels(img, a)
    print(f"[hood] {painted} texels of the line, box {HOOD_BOX}")


def paint_auto():
    """Delhi's colours: a green body under a yellow hood, with the hood's line. The street kit's parked
    autos stand under a grey tarp, so this is the look of one you can drive."""
    img = image_named("Body_01")
    a = pixels(img)
    rgb = a[..., :3].reshape(-1, 3).astype(np.float64)
    h, s, v = hsv_arrays(rgb)
    # The paint is yellow; rust is redder and stays.
    paint = (h > 0.09) & (h < 0.17) & (s > 0.45)
    h[paint] = 0.40
    s[paint] *= 0.82
    v[paint] *= 0.62
    a[..., :3] = rgb_arrays(h, s, v).reshape(a.shape[0], a.shape[1], 3)
    put_pixels(img, a)
    # The hood is black cloth: yellow cloth, its folds kept.
    img = image_named("material_baseColor")
    a = pixels(img)
    lum = a[..., :3].mean(2, keepdims=True)
    a[..., :3] = np.clip(np.array((0.95, 0.62, 0.05)) * (0.55 + 2.5 * lum), 0, 1)
    # A new image (the old one is greyscale, and would come back grey) on a new material: the seats
    # share the texture, and stay black.
    new = bpy.data.images.new("hood", img.size[0], img.size[1], alpha=True)
    put_pixels(new, a)
    old = bpy.data.materials["material"]
    hood = old.copy()
    hood.name = "hood"
    for node in hood.node_tree.nodes:
        if node.type == "TEX_IMAGE" and node.image == img:
            node.image = new
    for o in meshes():
        if old.name in o.data.materials:
            i = o.data.materials.find(old.name)
            o.data.materials.append(hood)
            for p in o.data.polygons:
                # Not the panel behind the driver.
                if p.material_index == i and p.center.z > 0.8 and not (abs(p.normal.y) > 0.9 and abs(p.center.y) < 0.1):
                    p.material_index = len(o.data.materials) - 1
            back = [p for p in o.data.polygons if p.material_index == len(o.data.materials) - 1 and p.normal.y < -0.5]
            if back:
                cs = np.array([p.center[:] for p in back])
                print(f"[hood] back of it: {len(back)} faces, x {cs[:, 0].min():.2f} to {cs[:, 0].max():.2f}, y {cs[:, 1].min():.2f} to {cs[:, 1].max():.2f}, z {cs[:, 2].min():.2f} to {cs[:, 2].max():.2f}")
    hood_line(new, hood)


def paint_tractor():
    """Nine materials, nine colour maps and nine normal maps, for one tractor. Only how each part takes
    the light is set here: `atlas` puts the colours on one sheet."""
    for name, rough, metal in (("hullM", 0.5, 0), ("bodyM", 0.5, 0), ("roofM", 0.5, 0), ("wheelFlapM", 0.5, 0), ("sitBoxM", 0.6, 0), ("pipesM", 0.5, 0.5), ("wheelSitM", 0.7, 0), ("frontTireM", 0.85, 0), ("backTireM", 0.85, 0)):
        b = bpy.data.materials[name].node_tree.nodes["Principled BSDF"]
        b.inputs["Roughness"].default_value = rough
        b.inputs["Metallic"].default_value = metal


def weather_tractor(a, base, rough, metal):
    """Rubber is black, whatever the painter made it."""
    for w in (o for o in meshes() if o.name.startswith("wheel_") and "atlas" in o.data.uv_layers):
        at = spots(base, w)
        on = ~np.isnan(at[..., 0])
        r = np.nan_to_num(np.hypot(at[..., 1], at[..., 2]))
        tyre = on & (r > 0.72 * r[on].max())
        a[tyre, :3] = a[tyre, :3].mean(1, keepdims=True) * 0.42


def paint_car():
    """A maker's badge on the nose and a brand's label on each sill: painted over."""
    img = image_named("Image_11")
    a = pixels(img)
    h = a.shape[0]
    white = a[h - 500, 500, :3].copy()
    for x0, y0, x1, y1 in ((824, 866, 848, 890), (940, 760, 958, 880)):
        a[h - y1 : h - y0, x0:x1, :3] = white
    put_pixels(img, a)
    img = image_named("Image_1")
    a = pixels(img)
    h = a.shape[0]
    box = a[h - 700 : h - 445, 785:852]
    tan = np.median(a[..., :3].reshape(-1, 3), axis=0)
    box[..., :3] = tan
    put_pixels(img, a)


def paint_ambassador():
    """The white of a government car in place of a bottle green polished like a mirror, which showed every
    lump in the model. Old chrome on the bumpers and grille. A maker's mark off the hub caps, and the
    number plate gone into the paint."""
    base, maps, bump = image_named("Image_0"), image_named("Image_1"), image_named("Image_2")
    a, m, n = pixels(base), pixels(maps), pixels(bump)
    h = a.shape[0]
    body = next(o for o in meshes() if "blinn1" in o.name and "Body" in o.name)
    hue, sat, val = hsv_arrays(a[..., :3].reshape(-1, 3).astype(np.float64))
    green = ((hue > 0.2) & (hue < 0.36) & (sat > 0.5) & (val < 0.35)).reshape(a.shape[:2])
    plate = texels(base, body, in_box(-0.33, -2.11, 0.47, 0.33, -1.98, 0.81))
    paint = green | plate
    a[paint, :3] = np.array((0.76, 0.75, 0.71))[None] * (0.72 + 0.28 * m[paint, 0])[:, None]
    m[paint, 1], m[paint, 2] = 0.42, 0.0
    # Door gaps and pressed lines, which the normal map has and the colour had not.
    def near(x, r=2):
        c = np.cumsum(np.cumsum(np.pad(x, ((r + 1, r), (r + 1, r))), 0), 1)
        k = 2 * r + 1
        return (c[k:, k:] - c[:-k, k:] - c[k:, :-k] + c[:-k, :-k]) / k ** 2
    lines = np.clip((np.hypot(n[..., 0] - near(n[..., 0]), n[..., 1] - near(n[..., 1])) - 0.08) * 7, 0, 1)
    a[paint, :3] *= (1 - 0.24 * lines[paint])[:, None]
    # Its bake leaves the panels lumpy: under a low sun they looked like cloth.
    n[paint, :3] = n[paint, :3] * 0.45 + np.array((0.5, 0.5, 1.0)) * 0.55
    road_dust(a, spots(base, body), paint)
    # Bumpers and their guards, the grille, the strip down the bonnet.
    chrome = (texels(base, body, in_box(-1, 1.93, 0.27, 1, 2.2, 0.57)) | texels(base, body, in_box(-1, -2.2, 0.22, 1, -1.9, 0.53)) | texels(base, body, in_box(-0.14, 0.9, 0.85, 0.14, 2.0, 1.2))) & ~paint
    lum = a[..., :3].mean(2)[chrome]
    a[chrome, :3] = np.clip(0.42 + 0.9 * lum, 0, 0.8)[:, None]
    m[chrome, 1], m[chrome, 2] = 0.3, 1.0
    grille = texels(base, body, in_box(-0.62, 1.95, 0.54, 0.62, 2.2, 0.76)) & ~paint
    a[grille, :3] = np.clip(a[grille, :3] * 2.0, 0, 0.8)
    m[grille, 1], m[grille, 2] = 0.34, 1.0
    # The hub cap's mark, in colour and in relief.
    ys, xs = np.mgrid[0 : h, 0 : h]
    r = np.hypot(xs - 779.5, h - 1 - ys - 717.5)
    plain = np.median(a[(r > 33) & (r < 39), :3], axis=0)
    a[r < 37, :3] = plain
    k = np.clip((33 - r) / 3, 0, 1)[..., None]
    n[..., :3] = n[..., :3] * (1 - k) + np.array((0.5, 0.5, 1.0)) * k
    put_pixels(base, a)
    put_pixels(maps, m)
    put_pixels(bump, n)
    for o in meshes():
        if "blinn2" in o.name:
            o.data.materials.clear()
            o.data.materials.append(glass(alpha=0.55))


def paint_scooter():
    """One grey material on the model: a maroon shell, black rubber, dull metal, by part."""
    shell = flat("shell", (0.23, 0.015, 0.03), 0.35)
    rubber = flat("rubber", (0.015, 0.015, 0.017), 0.85)
    metal = flat("metal", (0.45, 0.46, 0.48), 0.4, 0.8)
    for o in meshes():
        name = o.name.lower()
        o.data.materials.clear()
        size = Vector(o.dimensions)
        o.data.materials.append(shell if "polysurface1_" in name else rubber if max(size) > 0.3 or "pcube" in name else metal)


def paint_truck():
    """Flat colours on every part: its four textures look like stock photographs, the tyre's still carrying
    a picture library's watermark, so none is used. Faded blue cab, steel bed, black trim. The windscreen
    of the model stands clear of its frame, so a new one is cut to the frame."""
    fill_hole(bpy.data.objects["Object_11"], (0, 1.33, 1.55))
    # It came with no steering wheel: one on the right.
    bpy.ops.mesh.primitive_torus_add(major_radius=0.17, minor_radius=0.013, major_segments=14, minor_segments=4, location=(0.31, 1.17, 1.33), rotation=(math.radians(62), 0, 0))
    bpy.context.active_object.name = "Object_19_ring"
    bpy.ops.mesh.primitive_cylinder_add(vertices=6, radius=0.02, depth=0.5, location=(0.31, 1.28, 1.20), rotation=(math.radians(62), 0, 0))
    bpy.context.active_object.name = "Object_19_column"
    for o in meshes():
        if o.name.startswith("Object_19_"):
            o.data.transform(o.matrix_world)
            o.matrix_world = Matrix.Identity(4)
    table = (
        (r"^Object_(11|13|25|27|96|98)$", flat("cab", (0.095, 0.17, 0.26), 0.42)),
        (r"^Object_15$", flat("bed", (0.15, 0.17, 0.19), 0.6, 0.3)),
        (r"^Object_(84|86|7|78|106|19|19_ring|19_column|82)$", flat("trim", (0.018, 0.018, 0.02), 0.6)),
        (r"^Object_108$", flat("seat", (0.05, 0.045, 0.04), 0.75)),
        (r"^Object_(17|80|100|102)$", flat("under", (0.03, 0.03, 0.03), 0.65, 0.4)),
        (r"^Object_88$", flat("lamp", (0.72, 0.74, 0.76), 0.22, 0.7)),
        (r"^Object_94$", flat("amber", (0.8, 0.28, 0.02), 0.25)),
        (r"^Object_110$", flat("red", (0.55, 0.02, 0.015), 0.25)),
        (r"^Object_(115|120|125|130)$", flat("tyre", (0.03, 0.03, 0.03), 0.8)),
        (r"^Object_(117|122|127|132)$", flat("rim", (0.42, 0.43, 0.44), 0.45, 0.7)),
        (r"^Object_(21|23|104)$", glass(alpha=0.42)),
    )
    for o in meshes():
        for pattern, m in table:
            if re.search(pattern, o.name):
                o.data.materials.clear()
                o.data.materials.append(m)
                if m.name == "pane":
                    o.name = f"glass_{o.name}"
                break


def weather_truck(a, base, rough, metal):
    """Dust, a rusting bed, its pressed ribs, ribbed lamp glass, and a tread cut in the rubber: the tyre's
    picture went with the watermark."""
    body, wheel = bpy.data.objects["body"], next(o for o in meshes() if o.name.startswith("wheel_"))
    h, w = a.shape[:2]
    at = spots(base, body)
    on = ~np.isnan(at[..., 0])
    x, y, z = (np.nan_to_num(at[..., i]) for i in range(3))
    # Ribs along the bed's sides and tail.
    side = on & (z > 0.52) & (z < 1.03) & (y < 0.28) & ((np.abs(x) > 0.72) | (y < -1.86))
    for z0 in (0.68, 0.84):
        a[side & (z > z0 - 0.016) & (z < z0), :3] *= 0.68
        a[side & (z >= z0) & (z < z0 + 0.016), :3] = np.clip(a[side & (z >= z0) & (z < z0 + 0.016), :3] * 1.18, 0, 1)
    # The bed floor, worn to rust where loads drag.
    n = grime(h, w, 11)
    floor = on & (z > 0.5) & (z < 0.72) & (y < 0.28) & (y > -1.86) & (np.abs(x) < 0.72)
    k = (np.clip((n - 0.42) * 5, 0, 1) * floor * 0.75)[..., None]
    a[..., :3] = a[..., :3] * (1 - k) + np.array((0.30, 0.20, 0.13)) * k
    road_dust(a, at, on, high=0.95, amount=0.7)
    # Lamp glass, ribbed.
    lens = on & (y > 1.55) & (z > 0.8) & (z < 1.12) & (a[..., :3].mean(2) > 0.55)
    a[lens, :3] *= (0.88 + 0.12 * np.sin(x * 300) * np.sin(z * 170))[lens][:, None]
    at = spots(base, wheel)
    on = ~np.isnan(at[..., 0])
    r = np.nan_to_num(np.hypot(at[..., 1], at[..., 2]))
    wide = at[on][:, 0].max() - at[on][:, 0].min()
    x = np.nan_to_num(at[..., 0]) - (at[on][:, 0].max() + at[on][:, 0].min()) / 2
    th = np.nan_to_num(np.arctan2(at[..., 2], at[..., 1]))
    tread = on & (r > 0.93 * r[on].max()) & (np.abs(x) < 0.48 * wide)
    cuts = (np.abs(np.abs(x) - 0.2 * wide) < 0.022) | (np.abs(x) < 0.012) | ((np.abs(x) > 0.2 * wide) & (np.sin(th * 44 + 40 * np.abs(x)) > 0.72))
    a[tread & cuts, :3] *= 0.35
    # The hub, dark steel and not a white ball.
    a[on & (r < 0.5 * r[on].max()), :3] *= 0.5


def paint_bullet():
    """The tank says one maker's name, the side panels another's, the plate is a real plate: gone."""
    img = image_named("Image_0")
    a = pixels(img)
    h = a.shape[0]
    # Blender's rows run bottom up; these boxes are (x0, y0, x1, y1) from the top left of the 1024 px
    # texture. Only the white lettering goes: the tank's yellow line stays as a pinstripe.
    for x0, y0, x1, y1 in ((195, 570, 300, 625), (185, 855, 285, 910), (630, 435, 745, 530), (620, 695, 740, 785)):
        box = a[h - y1 : h - y0, x0:x1]
        white = box[..., 2] > 0.25
        box[white, :3] = a[h - 660, 100, :3]
    a[h - 990 : h - 822, 636:842, :3] = 0.86
    put_pixels(img, a)


def paint_jeep():
    """An army star on the bonnet, its serial numbers either side and the maker's name in a corner go.
    The plank of a bumper becomes the steel one a real one has, the tyres black rubber."""
    img = image_named("Image_0")
    a = pixels(img)
    h = a.shape[0]
    clone(a, (24, 515, 128, 619), 84, 0, oval=True)
    clone(a, (78, 478, 142, 510), 0, 32)
    clone(a, (78, 628, 142, 660), 0, -32)
    a[h - 990 : h - 850, 930:990, :3] = a[h - 900, 1010, :3]
    # The bonnet came dark and the tub light.
    hue, sat, val = hsv_arrays(a[..., :3].reshape(-1, 3).astype(np.float64))
    olive = ((hue > 0.17) & (hue < 0.38) & (sat > 0.16) & (val > 0.12) & (val < 0.8)).reshape(a.shape[:2])
    even(a, olive, (0.31, 0.34, 0.23))
    hull = next(o for o in meshes() if o.name.startswith("hull"))
    plank = texels(img, hull, in_box(-0.8, 1.3, 0.37, 0.8, 1.7, 0.6))
    lum = a[..., :3].mean(2)[plank]
    a[plank, :3] = np.array((0.070, 0.078, 0.052))[None] * (0.45 + 1.6 * lum)[:, None]
    wheel = next(o for o in meshes() if o.name.startswith("wheel"))
    co = np.array([v.co for v in wheel.data.vertices])
    mid = (co.min(0) + co.max(0)) / 2
    r = (co.max(0) - co.min(0))[1] / 2
    tyre = texels(img, wheel, lambda c: np.hypot(c[:, 1] - mid[1], c[:, 2] - mid[2]) > 0.71 * r)
    a[tyre, :3] = a[tyre, :3].mean(1, keepdims=True) * 0.34
    put_pixels(img, a)


CFG = {
    "auto": dict(src="auto/tuk_tuk_rikshaw__0bdfdf/scene.gltf", turn=180, length=2.72, body=9000, paint=paint_auto),
    # A toon shell, a shadow card, and three guns on the bonnet and wings (the model is a war game's).
    # It drives on the left, too.
    "gaadi_khuli": dict(src="open_jeep/jeep_willys__a28600/model.glb", turn=0, length=3.33, body=9000, drop=r"^(Outline|Plane001)", wheels=r"^wheel\d", paint=paint_jeep, sides=28, rough=0.55, mirror=True, strip=(
        (-0.40, 0.28, 1.012, 0.40, 0.62, 1.30),
        (-0.40, 0.28, 1.0335, 0.40, 1.52, 1.30),
        (-0.28, 1.15, 1.01, -0.20, 1.32, 1.05),
        (0.20, 1.15, 1.01, 0.28, 1.32, 1.05),
        (-0.82, 0.48, 0.856, -0.47, 1.45, 1.13),
        (0.47, 0.48, 0.856, 0.82, 1.45, 1.13),
        (-0.82, 0.63, 0.75, -0.59, 0.72, 0.98),
        (0.59, 0.63, 0.75, 0.82, 0.72, 0.98),
    ), spare=((-0.65, -1.70, 0.55, 0.10, -1.445, 1.34), (-0.2675, -1.5575, 0.9429)),
        panes=(((-0.5129, 0.2919, 1.1849), (-0.0232, 0.2919, 1.1849), (-0.0232, 0.1738, 1.5085), (-0.5129, 0.1738, 1.5085)),
               ((0.0232, 0.2919, 1.1849), (0.5129, 0.2919, 1.1849), (0.5129, 0.1738, 1.5085), (0.0232, 0.1738, 1.5085)))),
    "scooter": dict(src="scooter/honda_smart_dio_z4_2006_1__ef6060/model.glb", turn=-90, length=1.78, body=11000, paint=paint_scooter),
    # An engine, gearbox and battery nobody can see are most of its 153,000 triangles.
    "gaadi_minitruck": dict(src="props/mini_truck__71e499/model.glb", turn=0, length=3.8, narrow=0.92, body=11000, paint=paint_truck, atlas=weather_truck, maps=("Metallic", "Roughness"), small=("rough",), sides=30, sides_from=0.85,
                            drop=r"^Object_(5|9|30|32|34|36|38|40|42|44|46|48|50|52|54|56|58|60|62|64|66|68|70|72|74|76|90|92|112)$",
                            wheels=r"^Object_1(15|17|20|22|25|27|30|32)$",
                            parts=((r"^Object_11$", 2500), (r"^Object_(25|27)$", 900), (r"^Object_15$", 950), (r"^Object_84$", 500), (r"^Object_86$", 500), (r"^Object_108$", 500), (r"^Object_13$", 300), (r"^Object_106$", 300),
                                   (r"^Object_88$", 170), (r"^Object_94$", 90), (r"^Object_(96|98)$", 150), (r"^Object_17$", 400), (r"^Object_80$", 100), (r"^Object_(100|102)$", 100), (r"Object_(21|23)$", 60),
                                   (r"^Object_(115|120|125|130)$", 250), (r"^Object_(117|122|127|132)$", 230))),
    "commuter": dict(src="commuter_bike/pulsar_135_ls_2__accbf2/model.glb", turn=0, length=2.02, body=12000, drop=r"^placa", level=True),
    # Its wheels are the size of a big tractor's, so length and wheelbase go by those; it was too wide as well.
    "gaadi_tractor": dict(src="tractor/tractor__278fc1/model.glb", turn=90, length=3.25, narrow=0.82, body=9000, paint=paint_tractor, atlas=weather_tractor, maps=("Metallic", "Roughness"), small=("rough",),
                          wheels=r"^(frontTire|backTire)", wheel_tris=2400,
                          parts=((r"^tubeStuff", 1500), (r"^backTire", 1500, 900), (r"^frontTire", 500, 500), (r"^wheel_wheelSit", 420), (r"^midBit", 900), (r"^hull", 900), (r"^pipe", 300), (r"^seat", 200), (r"^tireFlap", 400))),
    "bullet": dict(src="heavy_bike/classic_motorcycle__f246ff/model.glb", turn=None, length=2.14, body=14000, paint=paint_bullet, level=True),
    "car": dict(src="small_car/stylized_suzuki_alto_works__467f2e/model.glb", turn=180, length=3.4, body=7000, paint=paint_car),
    # Modelled a tenth too wide for its length.
    "gaadi_ambassador": dict(src="small_car/hindustan_ambassador__a84ef7/model.glb", turn=90, length=4.33, narrow=0.9, body=11000, wheels=r"Wheel[LR]_", paint=paint_ambassador,
                             maps=("Normal", "Metallic", "Roughness"), small=("Image_1",)),
}
WHEEL_TRIS = 500
BOXES = {}


def meshes():
    return [o for o in bpy.context.scene.objects if o.type == "MESH"]


def world_points():
    return np.concatenate([np.array([o.matrix_world @ v.co for v in o.data.vertices]) for o in meshes()])


def bake_transforms():
    """Every mesh on its own, in world coordinates, unparented."""
    for o in meshes():
        o.data = o.data.copy()
        m = o.matrix_world.copy()
        o.parent = None
        o.data.transform(m)
        o.matrix_world = Matrix.Identity(4)
    for o in list(bpy.context.scene.objects):
        if o.type != "MESH":
            bpy.data.objects.remove(o)


def move_all(m):
    for o in meshes():
        o.data.transform(m)
    bpy.context.view_layer.update()


def face_forward(turn):
    if turn is not None:
        move_all(Matrix.Rotation(math.radians(turn), 4, "Z"))
        return
    # Parked askew on its stand: the thinnest spread of the points is the bike's side, the longest
    # its length. Stand that frame up: length along Y, side along X.
    p = world_points()
    p = p - p.mean(0)
    w, v = np.linalg.eigh(np.cov(p.T))
    side, along = Vector(v[:, 0]), Vector(v[:, 2])
    if side.dot(Vector((1, 0, 0))) < 0:
        side = -side
    if along.dot(Vector((0, 1, 0))) < 0:
        along = -along
    up = side.cross(along).normalized()
    along = up.cross(side).normalized()
    r = Matrix((side, along, up)).to_4x4()
    move_all(r)


def stand_up(length):
    p = world_points()
    lo, hi = p.min(0), p.max(0)
    k = length / (hi[1] - lo[1])
    move_all(Matrix.Scale(k, 4) @ Matrix.Translation((-(lo[0] + hi[0]) / 2, -(lo[1] + hi[1]) / 2, -lo[2])))


def islands(me):
    """Connected parts of a mesh: an array of labels, one per vertex."""
    n = len(me.vertices)
    e = np.empty(len(me.edges) * 2, dtype=np.int64)
    me.edges.foreach_get("vertices", e)
    e = e.reshape(-1, 2)
    labels = np.arange(n)
    while len(e):
        m = np.minimum(labels[e[:, 0]], labels[e[:, 1]])
        before = labels.copy()
        np.minimum.at(labels, e[:, 0], m)
        np.minimum.at(labels, e[:, 1], m)
        labels = labels[labels]
        if (labels == before).all():
            break
    return labels


def find_wheels(reach=0.08):
    """Round parts standing on the ground are tyres (within `reach` of it, on a model not yet level):
    each makes a wheel with whatever fits inside it."""
    parts = []
    for o in meshes():
        co = np.empty(len(o.data.vertices) * 3)
        o.data.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        labels = islands(o.data)
        for lab in np.unique(labels):
            m = labels == lab
            parts.append((o, m, co[m].min(0), co[m].max(0)))
    tyres = []
    for o, m, lo, hi in parts:
        s = hi - lo
        if 0.3 < s[1] < 0.9 and 0.03 < s[0] < 0.75 * s[1] and 0.7 * s[1] < s[2] < 1.15 * s[1] and lo[2] < reach:
            tyres.append((lo, hi))
    # A tyre can be several parts, inner and outer: one wheel for those on one axle and side.
    wheels = []
    for lo, hi in sorted(tyres, key=lambda t: -(t[1][2] - t[0][2])):
        for w in wheels:
            if abs((lo[1] + hi[1]) - (w[0][1] + w[1][1])) < 0.2 and min(hi[0], w[1][0]) - max(lo[0], w[0][0]) > -0.3:
                w[0], w[1] = np.minimum(lo, w[0]), np.maximum(hi, w[1])
                break
        else:
            wheels.append([lo.copy(), hi.copy()])
    # Radius by length: a tyre's top can be missing under a mudguard.
    wheels = [(np.array(((lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, lo[2] + (hi[1] - lo[1]) / 2)), (hi[1] - lo[1]) / 2, (hi[0] - lo[0]) / 2) for lo, hi in wheels]
    print(f"[wheels] {[(np.round(c, 2).tolist(), round(float(r), 2), round(float(w), 2)) for c, r, w in wheels]}")
    picks = {i: {} for i in range(len(wheels))}
    for o, m, lo, hi in parts:
        for i, (c, r, w) in enumerate(wheels):
            rad = max(np.hypot(*(np.array(q) - c[1:])) for q in ((lo[1], lo[2]), (lo[1], hi[2]), (hi[1], lo[2]), (hi[1], hi[2])))
            if rad < r * 1.04 and lo[0] > c[0] - w * 1.25 and hi[0] < c[0] + w * 1.25:
                d = picks[i].setdefault(o.name, np.zeros(len(o.data.vertices), dtype=bool))
                d[m] = True
                break
    return wheels, picks


def named_wheels(pattern):
    """Wheels the model already has as meshes of their own, found by name: both sides of an axle may
    be in one mesh, and a tyre and its rim in two."""
    found = []
    for o in meshes():
        if re.search(pattern, o.name):
            co = np.empty(len(o.data.vertices) * 3)
            o.data.vertices.foreach_get("co", co)
            co = co.reshape(-1, 3)
            for i, side in enumerate((co[:, 0] < 0, co[:, 0] >= 0)):
                if side.any():
                    # As it was before it was reduced, if it was.
                    found.append((o.name, side, *BOXES.get((o.name, i), (co[side].min(0), co[side].max(0)))))
    groups = []
    for name, mask, lo, hi in found:
        for g in groups:
            if abs((lo[1] + hi[1]) - (g[0][1] + g[1][1])) < 0.2 and lo[0] * g[0][0] > 0:
                g[0], g[1] = np.minimum(lo, g[0]), np.maximum(hi, g[1])
                g[2][name] = g[2][name] | mask if name in g[2] else mask
                break
        else:
            groups.append([lo.copy(), hi.copy(), {name: mask}])
    wheels = [(np.array(((lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, lo[2] + (hi[1] - lo[1]) / 2)), (hi[1] - lo[1]) / 2, (hi[0] - lo[0]) / 2) for lo, hi, _ in groups]
    print(f"[wheels] {[(np.round(c, 2).tolist(), round(float(r), 2), round(float(w), 2)) for c, r, w in wheels]}")
    return wheels, {i: g[2] for i, g in enumerate(groups)}


def level(cfg):
    """A bike parked on its stand stands level on both tyres."""
    wheels, _ = find_wheels(0.3)
    if len(wheels) < 2:
        return
    wheels.sort(key=lambda w: w[0][1])
    (a, ar, _), (b, br, _) = wheels[0], wheels[-1]
    tilt = math.atan2((b[2] - br) - (a[2] - ar), b[1] - a[1])
    print(f"[level] {math.degrees(tilt):.1f} degrees")
    move_all(Matrix.Rotation(-tilt, 4, "X"))
    stand_up(cfg["length"])


def name_wheels(wheels):
    fy = max(c[1] for c, _, _ in wheels)
    names = []
    for c, _, _ in wheels:
        end = "f" if c[1] > fy - 0.4 else "r"
        twin = sum(1 for d, _, _ in wheels if abs(d[1] - c[1]) < 0.4) > 1
        names.append(f"wheel_{end}" + (("r" if c[0] > 0 else "l") if twin else ""))
    return names


def cut(o, keep):
    """Delete the vertices of `o` where `keep` is false."""
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bm.verts.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[bm.verts[i] for i in np.nonzero(~keep)[0]], context="VERTS")
    bm.to_mesh(o.data)
    bm.free()


def strip(boxes):
    """Delete every face lying wholly inside one of these boxes (x0, y0, z0, x1, y1, z1, in the model's
    own coordinates): parts of one mesh that no name or gap sets apart."""
    for o in meshes():
        co = np.empty(len(o.data.vertices) * 3)
        o.data.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bm.verts.ensure_lookup_table()
        gone = []
        for b in boxes:
            lo, hi = np.array(b[:3]), np.array(b[3:])
            ok = ((co >= lo) & (co <= hi)).all(1)
            gone += [f for f in bm.faces if all(ok[v.index] for v in f.verts)]
        print(f"[strip] {o.name}: {len(set(gone))} of {len(bm.faces)} faces")
        bmesh.ops.delete(bm, geom=list(set(gone)), context="FACES")
        bm.to_mesh(o.data)
        bm.free()


def mirror():
    """Across the model's middle: a left-hand drive becomes a right-hand one, as on every road here."""
    for o in meshes():
        me = o.data
        n = np.empty(len(me.loops) * 3)
        me.corner_normals.foreach_get("vector", n)
        n = n.reshape(-1, 3) * (-1, 1, 1)
        kept = [{v: n[i] for i, v in zip(p.loop_indices, p.vertices)} for p in me.polygons]
        me.transform(Matrix.Scale(-1, 4, (1, 0, 0)))
        me.flip_normals()
        me.update()
        out = np.zeros_like(n)
        for p, k in zip(me.polygons, kept):
            for i, v in zip(p.loop_indices, p.vertices):
                out[i] = k[v]
        me.normals_split_custom_set(out.tolist())


def baked_wheels(wheels):
    """The wheels worth a place on the sheet: one, or one per axle if the axles' wheels differ."""
    front = next(w for w in wheels if w[0].startswith("wheel_f"))
    rear = next(w for w in wheels if w[0].startswith("wheel_r"))
    return [front, rear] if abs(front[3] - rear[3]) > 0.03 else [front]


def share_wheels(wheels, baked):
    """A baked wheel for all on its axle, turned end for end for the far side, so its face stays outward."""
    for name, w, c, r in wheels:
        first = next((b for b in baked if b[0][:7] == name[:7]), baked[0])
        if w is not first[1]:
            me = first[1].data.copy()
            if (c[0] > 0) != (first[2][0] > 0):
                me.transform(Matrix.Rotation(math.pi, 4, "Z"))
            w.data = me
        w.data.name = name
        w.rotation_euler = (0, 0, 0)


def spare(box, at, wheels):
    """The spare on the tail: its own flat-sided one out, a copy of a road wheel in, its face to the rear."""
    strip((box,))
    w = next(w[1] for w in wheels if w[2][0] > 0)
    o = w.copy()
    o.data = w.data.copy()
    o.name = "spare"
    bpy.context.scene.collection.objects.link(o)
    o.data.transform(Matrix.Translation(at) @ Matrix.Rotation(math.radians(-90), 4, "Z"))
    o.matrix_world = Matrix.Identity(4)


def glass(tint=(0.012, 0.018, 0.018), alpha=0.32, rough=0.08):
    if "pane" in bpy.data.materials:
        return bpy.data.materials["pane"]
    m = bpy.data.materials.new("pane")
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*tint, 1.0)
    b.inputs["Alpha"].default_value = alpha
    b.inputs["Roughness"].default_value = rough
    m.surface_render_method = "BLENDED"
    return m


def panes(quads):
    """Glass in a frame that came without it: in the model's own coordinates, as it stands in the end."""
    verts = [v for q in quads for v in q]
    me = bpy.data.meshes.new("panes")
    me.from_pydata(verts, [], [tuple(range(i, i + 4)) for i in range(0, len(verts), 4)])
    me.materials.append(glass())
    me.uv_layers.new(name="UVMap")
    o = bpy.data.objects.new("panes", me)
    bpy.context.scene.collection.objects.link(o)


def fill_hole(o, near):
    """Glass across the open rim in `o` that passes nearest `near`: the model's own pane does not fit."""
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.0006)
    rim = {e for e in bm.edges if len(e.link_faces) == 1}
    loops = []
    while rim:
        first = rim.pop()
        chain = [first.verts[0], first.verts[1]]
        while True:
            e = next((x for x in chain[-1].link_edges if x in rim), None)
            if e is None:
                break
            rim.discard(e)
            chain.append(e.other_vert(chain[-1]))
        loops.append([v.co.copy() for v in chain[:-1]])
    bm.free()
    at = Vector(near)
    best = min((c for c in loops if len(c) > 8), key=lambda c: (sum(c, Vector()) / len(c) - at).length)
    me = bpy.data.meshes.new("pane")
    me.from_pydata(best, [], [list(range(len(best)))])
    me.materials.append(glass(alpha=0.42))
    me.uv_layers.new(name="UVMap")
    g = bpy.data.objects.new("glass_pane", me)
    bpy.context.scene.collection.objects.link(g)
    me.shade_smooth()


def round_wheel(w, sides, least=0.05):
    """A tyre of a dozen flat sides looks cut from card. Every edge running round the axle (+X, the wheel
    centred on its origin), from `least` of the way out, is split until the ring has `sides` corners, and
    the new points go out onto its circle, which is what a chord's points are not."""
    bm = bmesh.new()
    bm.from_mesh(w.data)
    step = 2 * math.pi / sides
    ring = []
    least *= max(math.hypot(v.co.y, v.co.z) for v in bm.verts)
    for e in bm.edges:
        a, b = e.verts[0].co, e.verts[1].co
        ra, rb = math.hypot(a.y, a.z), math.hypot(b.y, b.z)
        if min(ra, rb) < least or abs(ra - rb) > 0.04 * ra or abs(a.x - b.x) > 0.25 * (a - b).length:
            continue
        gap = (math.atan2(b.z, b.y) - math.atan2(a.z, a.y) + math.pi) % (2 * math.pi) - math.pi
        if abs(gap) > step * 1.3:
            ring.append((e, a.copy(), b.copy(), gap))
    if not ring:
        bm.free()
        return
    cuts = max(1, round(abs(np.median([g for *_, g in ring])) / step) - 1)
    A = np.array([r[1] for r in ring])
    B = np.array([r[2] for r in ring])
    done = bmesh.ops.subdivide_edges(bm, edges=[r[0] for r in ring], cuts=cuts, use_grid_fill=True)
    for v in done["geom_split"]:
        if not isinstance(v, bmesh.types.BMVert):
            continue
        p = np.array(v.co)
        # Which chord it was cut from, and how far along.
        t = np.clip(((p - A) * (B - A)).sum(1) / ((B - A) ** 2).sum(1), 0, 1)
        d = np.linalg.norm(A + (B - A) * t[:, None] - p, axis=1)
        i = int(d.argmin())
        if d[i] > 1e-4:
            continue
        a, gap = ring[i][1], ring[i][3]
        b = ring[i][2]
        ra, rb = math.hypot(a.y, a.z), math.hypot(b.y, b.z)
        r = ra + (rb - ra) * t[i]
        th = math.atan2(a.z, a.y) + gap * t[i]
        v.co.y, v.co.z = r * math.cos(th), r * math.sin(th)
    bm.to_mesh(w.data)
    bm.free()
    print(f"[round] {w.name}: {len(ring)} ring edges, cut {cuts} times, {tris(w)} tris")


def thin(parts):
    """Reduce a heavy model part by part, (name pattern, triangles) in turn, so what shows keeps what it
    needs. Points are welded first, or the cuts stop at every seam, and shading is worked out afresh.
    Wheels with tyre and rim in one mesh, (name pattern, tread, disc), get a budget for each: left to
    itself the reduction spends all on the lugs and the rim's edge goes square."""
    for o in meshes():
        entry = next((p for p in parts if re.search(p[0], o.name)), None)
        if entry is None:
            continue
        co = np.empty(len(o.data.vertices) * 3)
        o.data.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        for i, side in enumerate((co[:, 0] < 0, co[:, 0] >= 0)):
            if side.any():
                BOXES[o.name, i] = (co[side].min(0), co[side].max(0))
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.0006)
        bm.to_mesh(o.data)
        bm.free()
        if len(entry) > 2:
            thin_wheels(o, *entry[1:])
        else:
            decimate(o, entry[1])
        shade(o)


def thin_wheels(o, tread, disc):
    def radii(x):
        co = np.empty(len(x.data.vertices) * 3)
        x.data.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        mid = (co.min(0) + co.max(0)) / 2
        return np.hypot(co[:, 1] - mid[1], co[:, 2] - mid[2]), np.abs(co[:, 0])

    def group(x, pick):
        g = x.vertex_groups.new(name="keep")
        g.add(np.nonzero(pick)[0].tolist(), 1.0, "REPLACE")
        return g.name

    def part(pick):
        x = o.copy()
        x.data = o.data.copy()
        bpy.context.scene.collection.objects.link(x)
        cut(x, pick)
        return x

    r, side = radii(o)
    labels = islands(o.data)
    parts = np.unique(labels)
    tyres = np.isin(labels, [i for i in parts if r[labels == i].max() > 0.85 * r.max()])
    behind = ~tyres & np.isin(labels, [i for i in parts if side[labels == i].mean() < side[tyres].mean()])
    t, back = part(tyres), part(behind)
    cut(o, ~tyres & ~behind)
    # The side nobody sees gets little. The bead of the tyre is what shows as round or not.
    decimate(back, disc // 3)
    decimate(o, disc)
    r, _ = radii(t)
    bead = 0.77 * r.max()
    lip = sum(1 for p in t.data.polygons if all(r[v] < bead for v in p.vertices))
    decimate(t, tread + lip, group(t, r < bead))
    t.vertex_groups.clear()
    join([o, t, back], o.name)


def shade(o, angle=38):
    """Smooth, but for edges sharper than `angle`, whatever normals the source had."""
    me = o.data
    if "custom_normal" in me.attributes:
        me.attributes.remove(me.attributes["custom_normal"])
    me.shade_smooth()
    me.set_sharp_from_angle(angle=math.radians(angle))


def bake_pass(objs, img, kind, socket=None, samples=1):
    """One pass into `img` on the `atlas` texture coordinates: `socket` of the materials as they stand,
    or ambient occlusion."""
    scene = bpy.context.scene
    scene.cycles.samples = samples
    done = []
    for m in {m for o in objs for m in o.data.materials}:
        nt = m.node_tree
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = img
        nt.nodes.active = n
        out = next(x for x in nt.nodes if x.type == "OUTPUT_MATERIAL")
        was = out.inputs["Surface"].links[0].from_socket
        em = None
        if socket:
            b = next(x for x in nt.nodes if x.type == "BSDF_PRINCIPLED")
            em = nt.nodes.new("ShaderNodeEmission")
            i = b.inputs[socket]
            if i.is_linked:
                nt.links.new(i.links[0].from_socket, em.inputs["Color"])
            else:
                v = i.default_value
                em.inputs["Color"].default_value = tuple(v) if hasattr(v, "__len__") else (v, v, v, 1.0)
            nt.links.new(em.outputs["Emission"], out.inputs["Surface"])
        done.append((nt, n, em, out, was))
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.bake(type=kind, uv_layer="atlas", use_clear=True, margin=6, margin_type="EXTEND", target="IMAGE_TEXTURES")
    for nt, n, em, out, was in done:
        nt.nodes.remove(n)
        if em:
            nt.nodes.remove(em)
        nt.links.new(was, out.inputs["Surface"])


def atlas(body, wheels, size, finish):
    """The many materials of a model into one set of textures, drawn with one call: colour with its
    shading in the creases, and metal and roughness. `wheels` go on the same sheet. `finish` gets the
    colours and the images to weather."""
    objs = [body, *wheels]
    for o in objs:
        o.data.uv_layers.new(name="atlas")
        o.data.uv_layers.active = o.data.uv_layers["atlas"]
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = body
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(55), island_margin=0.004)
    bpy.ops.object.mode_set(mode="OBJECT")
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.render.threads_mode = "FIXED"
    scene.render.threads = 6
    scene.world = bpy.data.worlds.new("bake")
    scene.world.light_settings.distance = 0.7
    image = lambda n, res, linear: (lambda i: (setattr(i.colorspace_settings, "name", "Non-Color" if linear else "sRGB"), i)[1])(bpy.data.images.new(n, res, res))
    base, ao, rough, metal = image("base", size, False), image("ao", size, True), image("rough", size // 2, True), image("metal", size // 2, True)
    bake_pass(objs, base, "EMIT", "Base Color")
    bake_pass(objs, rough, "EMIT", "Roughness")
    bake_pass(objs, metal, "EMIT", "Metallic")
    bake_pass(objs, ao, "AO", samples=48)
    a = pixels(base)
    a[..., :3] *= (0.35 + 0.65 * pixels(ao)[..., :1])
    finish(a, base, rough, metal)
    # Between the islands: their average, or the small mip levels fade the model to black far off.
    empty = a[..., :3].max(2) == 0
    a[empty, :3] = a[~empty, :3].mean(0)
    put_pixels(base, a)
    m = pixels(rough)
    m[..., 2] = pixels(metal)[..., 0]
    put_pixels(rough, m)
    bpy.data.images.remove(ao)
    bpy.data.images.remove(metal)
    mat = bpy.data.materials.new("atlas")
    mat.use_nodes = True
    nt = mat.node_tree
    b = nt.nodes["Principled BSDF"]
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image = base
    nt.links.new(t.outputs["Color"], b.inputs["Base Color"])
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image = rough
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(t.outputs["Color"], sep.inputs["Color"])
    nt.links.new(sep.outputs["Green"], b.inputs["Roughness"])
    nt.links.new(sep.outputs["Blue"], b.inputs["Metallic"])
    for o in objs:
        o.data.materials.clear()
        o.data.materials.append(mat)
        for uv in [u for u in o.data.uv_layers if u.name != "atlas"]:
            o.data.uv_layers.remove(uv)


def join(objs, name):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    if len(objs) > 1:
        bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    o.name = name
    o.data.name = name
    return o


def tris(o):
    return sum(len(p.vertices) - 2 for p in o.data.polygons)


def decimate(o, target, keep=None):
    """To `target` triangles, the points of the vertex group `keep` last of all."""
    n = tris(o)
    if n <= target:
        return
    bpy.context.view_layer.objects.active = o
    d = o.modifiers.new("d", "DECIMATE")
    d.ratio = target / n
    if keep:
        d.vertex_group = keep
        d.invert_vertex_group = True
    bpy.ops.object.modifier_apply(modifier=d.name)


def split_wheels(cfg):
    wheels, picks = named_wheels(cfg["wheels"]) if "wheels" in cfg else find_wheels()
    names = name_wheels(wheels)
    made = []
    for i, name in enumerate(names):
        parts = []
        for oname, mask in picks[i].items():
            o = bpy.data.objects[oname]
            w = o.copy()
            w.data = o.data.copy()
            bpy.context.scene.collection.objects.link(w)
            cut(w, mask)
            parts.append(w)
        if parts:
            made.append((name, parts, wheels[i][0], wheels[i][1]))
    # The copies above used the masks' vertex numbers, so the originals lose those parts last.
    gone = {}
    for i in range(len(names)):
        for oname, mask in picks[i].items():
            gone[oname] = gone.get(oname, np.zeros(len(mask), dtype=bool)) | mask
    for oname, mask in gone.items():
        cut(bpy.data.objects[oname], ~mask)
    out = []
    for name, parts, centre, r in made:
        w = join(parts, name)
        decimate(w, cfg.get("wheel_tris", WHEEL_TRIS))
        w.data.transform(Matrix.Translation(tuple(-centre)))
        w.location = tuple(centre)
        # A front wheel parked turned: its mesh squared up, the turn kept on the node, which is what
        # the game spins about.
        co = np.empty(len(w.data.vertices) * 3)
        w.data.vertices.foreach_get("co", co)
        axle = np.linalg.eigh(np.cov(co.reshape(-1, 3).T))[1][:, 0]
        yaw = math.atan2(axle[1], axle[0]) if axle[0] > 0 else math.atan2(-axle[1], -axle[0])
        w.data.transform(Matrix.Rotation(-yaw, 4, "Z"))
        w.rotation_euler = (0, 0, yaw)
        out.append((name, w, centre, r))
    return out


def tidy_materials(size, maps=(), small=()):
    """Base colour and any of these other `maps`, as PNG, no bigger than `size` (half that for the images
    named in `small`)."""
    for m in bpy.data.materials:
        if not m.use_nodes:
            continue
        for node in m.node_tree.nodes:
            if node.type != "BSDF_PRINCIPLED":
                continue
            for sock in ("Normal", "Metallic", "Roughness", "Emission Color", "Emission Strength"):
                i = node.inputs.get(sock)
                if i is not None and sock not in maps:
                    for l in list(i.links):
                        m.node_tree.links.remove(l)
            if m.name != "pane":
                node.inputs["Roughness"].default_value = max(0.4, node.inputs["Roughness"].default_value)
    used = set()
    for m in bpy.data.materials:
        if m.use_nodes:
            for node in m.node_tree.nodes:
                if node.type == "TEX_IMAGE" and node.image:
                    used.add(node.image.name)
    for img in list(bpy.data.images):
        if img.name not in used:
            bpy.data.images.remove(img)
    # The game reads PNG only: every image is written out fresh, whatever it came in as.
    for img in list(bpy.data.images):
        k = min(1.0, (size // 2 if img.name.rsplit(".", 1)[0] in small else size) / max(img.size))
        w, h = int(img.size[0] * k), int(img.size[1] * k)
        if k < 1.0:
            img.scale(w, h)
        new = bpy.data.images.new(img.name.rsplit(".", 1)[0] + "_png", w, h, alpha=True)
        new.colorspace_settings.name = img.colorspace_settings.name
        put_pixels(new, pixels(img))
        new.file_format = "PNG"
        for m in bpy.data.materials:
            for node in m.node_tree.nodes if m.use_nodes else ():
                if node.type == "TEX_IMAGE" and node.image == img:
                    node.image = new
        bpy.data.images.remove(img)


def build(name):
    cfg = CFG[name]
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, cfg["src"]))
    bake_transforms()
    if "drop" in cfg:
        for o in meshes():
            if re.search(cfg["drop"], o.name):
                bpy.data.objects.remove(o)
    if "strip" in cfg:
        strip(cfg["strip"])
    if cfg.get("mirror"):
        mirror()
    face_forward(cfg["turn"])
    stand_up(cfg["length"])
    if "narrow" in cfg:
        move_all(Matrix.Diagonal((cfg["narrow"], 1, 1, 1)))
    if cfg.get("level"):
        level(cfg)
    for o in meshes():
        bpy.context.view_layer.update()
        print(f"[part] {name} {o.name[:34]:34s} tris {tris(o):6d} size {[round(x, 2) for x in o.dimensions]} mats {[m.name for m in o.data.materials][:3]}")
    if "paint" in cfg:
        cfg["paint"]()
    if "parts" in cfg:
        thin(cfg["parts"])
    wheels = split_wheels(cfg)
    wnames = {w[0] for w in wheels}
    if "sides" in cfg:
        for _, w, c, _ in wheels:
            round_wheel(w, cfg["sides"], cfg.get("sides_from", 0.05))
            # Its flats stood on the ground; now its corners would go through it.
            r = max(math.hypot(v.co.y, v.co.z) for v in w.data.vertices)
            if r > c[2]:
                w.data.transform(Matrix.Diagonal((1, c[2] / r, c[2] / r, 1)))
    if "spare" in cfg:
        spare(*cfg["spare"], wheels)
    if "panes" in cfg:
        panes(cfg["panes"])
    panes_of = [o for o in meshes() if o.name.startswith("glass_")]
    body = join([o for o in meshes() if o.name not in wnames and o not in panes_of and len(o.data.polygons)], "body")
    decimate(body, cfg["body"])
    for o in [o for o in meshes() if not len(o.data.polygons)]:
        bpy.data.objects.remove(o)
    if "atlas" in cfg:
        baked = baked_wheels(wheels)
        atlas(body, [b[1] for b in baked], 1024, cfg["atlas"])
        share_wheels(wheels, baked)
    if panes_of:
        body = join([body, *panes_of], "body")
    tidy_materials(1024, cfg.get("maps", ()), cfg.get("small", ()))
    # Names from the source go: they can hold a maker's.
    if name.startswith("gaadi_"):
        for i, m in enumerate(o for o in bpy.data.materials if o.users and o.name != "pane"):
            m.name = f"body_{i}"
            if "rough" in cfg:
                m.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = cfg["rough"]
    # One set of texture coordinates is all the game reads.
    for o in meshes():
        for uv in list(o.data.uv_layers)[1:]:
            o.data.uv_layers.remove(uv)
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, f"{name}.glb")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", export_yup=True, export_apply=False, export_image_format="AUTO",
                              export_animations=False, export_skins=False, export_cameras=False, export_lights=False, use_selection=False,
                              export_tangents="Normal" in cfg.get("maps", ()))
    p = world_points()
    print(f"[done] {name}: body {tris(body)} tris, wheels {[(w[0], tris(w[1]), round(w[3], 3), [round(float(x), 2) for x in w[2]]) for w in wheels]}, "
          f"size {np.round(p.max(0) - p.min(0), 2).tolist()}, {os.path.getsize(path) / 1e6:.1f} MB")


def render(name, out):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.join(OUT, f"{name}.glb"))
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "TEXTURE"
    scene.render.resolution_x, scene.render.resolution_y = 640, 440
    world = bpy.data.worlds.new("w")
    world.color = (0.7, 0.7, 0.72)
    scene.world = world
    cam = bpy.data.cameras.new("c")
    cam.lens = 70
    co = bpy.data.objects.new("c", cam)
    scene.collection.objects.link(co)
    scene.camera = co
    p = world_points()
    centre = Vector((p.min(0) + p.max(0)) / 2)
    r = Vector(p.max(0) - p.min(0)).length * 1.5
    os.makedirs(out, exist_ok=True)
    for tag, az, el in (("a", 55, 15), ("b", 0, 2), ("c", 90, 2), ("d", 270, 2)):
        a, e = math.radians(az), math.radians(el)
        co.location = centre + Vector((math.cos(a) * math.cos(e), math.sin(a) * math.cos(e), math.sin(e))) * r
        co.rotation_mode = "QUATERNION"
        co.rotation_quaternion = (centre - co.location).to_track_quat("-Z", "Y")
        scene.render.filepath = os.path.join(out, f"{name}_{tag}.png")
        bpy.ops.render.render(write_still=True)


args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
out = None
if "--render" in args:
    i = args.index("--render")
    out = args[i + 1]
    del args[i : i + 2]
for name in args or list(CFG):
    build(name)
    if out:
        render(name, out)

"""The room kit: 14 pieces of everyday Delhi furniture for the hollow buildings' rooms, shops, offices,
godowns and dhabas. Each is one mesh under 300 triangles, and all share one material and one texture
atlas, so a whole cell's furniture merges into one mesh and one draw.

Nothing here is flat colour. Each piece has a detailed version (real CC0 scans of wood, steel,
plastic, jute and velvet, seams, slots, bolts, wear on the edges, grime in the hollows), which is
baked down onto the low mesh: base colour, normal, and occlusion, roughness and metal in one ORM map.

The pieces, by their mesh names in room_kit.glb, are the keys of KIT below, with their sizes.

Every piece is centred on its origin, stands on it (z = 0 is the floor) and faces Blender +Y, which
is game -Z: put its back to a wall by turning it. Backs and undersides get less of the atlas. The fan
is the one exception to all that: it hangs, so its origin is the point on the ceiling.

Using it in the game:
- Each mesh is `file.named_meshes["charpai"]`, primitive 0. They hold positions, normals, tangents and
  one UV set, so any of them merge.
- The one material is `file.named_materials["room_kit"]`, a GltfMaterial: make it a StandardMaterial
  once, with `standard_material_from_gltf_material`, as `resolve_gltf_materials` does.
- The meshes are merged on the CPU, so `gpu_only_meshes` (crate::buildings) has to keep them, as it
  does the street kit's: add `room_kit` to its test.
- The textures are inside the file, so `gpu_only_images` gives them no mips. Add
  `|| path.path().ends_with("room_kit.glb")` to its test and it will.

Sources, all CC0 (rows in docs/references.md). Raw files stay out of git, in build/src; anything
missing is fetched there when this runs:
- Kenney Furniture Pack (table, stool, counter, desk, TV unit), Quaternius Ultimate House Interior
  Pack (sofa), through OpenGameArt's copies.
- Poly Haven and ambientCG scans, listed in SETS.
Built here, as nothing free fits a Delhi room: charpai, almirah, plastic chair, shelf, sacks, gas
stove, water drum and fan. The rope weave on the charpai is drawn in `rope_maps`.

blender -b --factory-startup -P tools/blender/props/room_kit.py
Environment: ROOM_RES=1024 bakes smaller, for trying things out. ROOM_ONLY=charpai,sofa builds some.
"""

import math
import os
import random
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from bpy_extras.bmesh_utils import bmesh_linked_uv_islands  # noqa: E402
from mathutils import Matrix, Vector, noise  # noqa: E402
from mathutils.bvhtree import BVHTree  # noqa: E402

import bake  # noqa: E402
import export  # noqa: E402
import geo  # noqa: E402
from nodes import Graph, new_material  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "models", "props", "room_kit.glb")
SRC = os.path.join(ROOT, "build", "src")
WORK = os.path.join(ROOT, "build", "room_kit")
PREVIEWS = os.path.join(ROOT, "previews", "room-kit")

RES = int(os.environ.get("ROOM_RES", 2048))  # of the bake, and of the colour map that ships
ONLY = [n for n in os.environ.get("ROOM_ONLY", "").split(",") if n]
MAX_TRIS = 300
GAP = 8 / 2048  # UV space between islands; the bake bleeds half of it
MARGIN = max(2, RES // 512)
CAGE = 0.006  # how far above a hard piece's surface the bake looks for detail
ROUND_CAGE = 0.016  # a piece with round parts: a low facet's middle lies this far inside the true circle
SOFT_CAGE = 0.07  # a stuffed or thinned piece, whose low mesh lies further from the real shape

# Mesh name: width (x), depth (y), height (z) in metres. The free models are stretched to these.
KIT = {
    "charpai": (1.95, 0.94, 0.50),
    "almirah": (0.91, 0.53, 1.83),
    "table": (1.20, 0.75, 0.75),
    "plastic_chair": (0.61, 0.56, 0.84),
    "stool": (0.33, 0.33, 0.45),
    "shelf": (0.92, 0.40, 1.95),
    "counter": (1.60, 0.60, 0.95),
    "sacks": (1.04, 1.02, 0.72),
    "gas_stove": (0.62, 0.45, 0.87),
    "water_drum": (0.61, 0.62, 0.92),
    "desk": (1.40, 0.70, 0.75),
    "sofa": (1.60, 0.78, 0.82),
    "tv_unit": (1.50, 0.42, 1.14),
    "fan": (1.20, 1.20, 0.46),
}

KENNEY = "kenney/pack/Models"
QUATERNIUS = "quaternius/pack/Ultimate House Interior Pack - June 2020/OBJ"
ZIPS = {
    "kenney": "https://opengameart.org/sites/default/files/kenney_furniturePack.zip",
    "quaternius": "https://opengameart.org/sites/default/files/ultimate_house_interior_pack_-_june_2020.zip",
}

# Texture sets, 1K: colour, roughness, normal (+Y up). `ph` Poly Haven, `acg` ambientCG.
SETS = {
    "rosewood": ("ph", "rosewood_veneer1"),
    "pine": ("ph", "stained_pine"),
    "teak": ("ph", "teak_veneer"),
    "ply": ("ph", "plywood"),
    "olive": ("ph", "green_metal_rust"),
    "jute": ("ph", "hessian_380"),
    "velvet": ("ph", "velour_velvet"),
    "brushed": ("acg", "Metal009"),
    "red": ("acg", "PaintedMetal004"),
    "white": ("acg", "Plastic010"),
    "blue": ("acg", "Plastic008"),
    "black": ("acg", "Plastic006"),
}


def fetch():
    """Whatever isn't in build/src yet, with a plain tool name and nothing else."""

    def curl(url, path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        subprocess.run(["curl", "-sS", "-L", "-f", "-A", "curl/8.7.1", "-o", path, url], check=True)

    for name, url in ZIPS.items():
        if not os.path.isdir(os.path.join(SRC, name, "pack")):
            path = os.path.join(SRC, name, name + ".zip")
            curl(url, path)
            subprocess.run(["unzip", "-q", "-o", path, "-d", os.path.join(SRC, name, "pack")], check=True)
    for host, slug in SETS.values():
        folder = os.path.join(SRC, "tex", slug)
        if host == "ph":
            for m in ("diff", "rough", "nor_gl"):
                path = os.path.join(folder, f"{slug}_{m}_1k.jpg")
                if not os.path.exists(path):
                    curl(f"https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/{slug}/{slug}_{m}_1k.jpg", path)
        elif not os.path.exists(os.path.join(folder, f"{slug}_1K-JPG_Color.jpg")):
            path = os.path.join(folder, slug + ".zip")
            curl(f"https://ambientcg.com/get?file={slug}_1K-JPG.zip", path)
            # Only the pictures: the archives also hold .blend files, never opened here.
            subprocess.run(["unzip", "-q", "-o", path, "*.jpg", "-d", folder], check=True)


def maps(key):
    host, slug = SETS[key]
    folder = os.path.join(SRC, "tex", slug)
    if host == "ph":
        return tuple(os.path.join(folder, f"{slug}_{m}_1k.jpg") for m in ("diff", "rough", "nor_gl"))
    return tuple(os.path.join(folder, f"{slug}_1K-JPG_{m}.jpg") for m in ("Color", "Roughness", "NormalGL"))


# ------------------------------------------------------------------ surfaces


def lin(rgb):
    return tuple(c ** 2.2 for c in rgb)


def rope_maps(res=1024, tile=0.32):
    """A charpai's bed: rope in a 2/2 twill on the diagonal, colour and height, tiling. Drawn here as
    no free scan shows it; the fibre comes from noise, the strands from the weave."""
    pitch = tile / 16  # strands per tile must be whole, or it won't tile
    y, x = np.mgrid[0:res, 0:res] * (tile / res)
    u, v = (x + y) / (pitch * 2), (x - y) / (pitch * 2)
    iu, iv = np.floor(u).astype(int), np.floor(v).astype(int)
    fu, fv = u - iu, v - iv
    top_a = ((iu + iv) % 4) < 2
    across = np.where(top_a, fv, fu) * 2 - 1
    along = np.where(top_a, fu, fv) * 2 - 1
    height = np.sqrt(np.clip(1 - across ** 2, 0, 1)) * (0.72 + 0.28 * np.cos(along * math.pi / 2))
    height = np.where(top_a, height, height * 0.86)
    rng = np.random.default_rng(7)
    shades = rng.uniform(0.86, 1.12, 64)
    shade = shades[(np.where(top_a, iv, iu)) % 64]
    twist = 0.5 + 0.5 * np.sin((along + across * 0.6) * 9.0 + shade * 30)
    colour = np.empty((res, res, 4), dtype=np.float32)
    straw = np.array(lin((0.66, 0.56, 0.40)), dtype=np.float32)
    k = (0.30 + 0.70 * height) * shade * (0.86 + 0.14 * twist)
    colour[..., :3] = straw * k[..., None]
    colour[..., 3] = 1
    h = np.empty((res, res, 4), dtype=np.float32)
    h[..., :3] = (height * 0.9 + 0.1 * twist * height)[..., None]
    h[..., 3] = 1

    def image(name, px):
        img = bpy.data.images.new(name, res, res, alpha=False, float_buffer=True)
        img.colorspace_settings.name = "Non-Color"
        img.pixels.foreach_set(px[::-1].ravel())
        img.pack()
        return img

    return image("rope_colour", colour), image("rope_height", h)


def surface(name, key=None, tile=1.0, tint=(1, 1, 1), flat=None, rough=(0.0, 1.0), metal=0.0, bump=1.0, grain_v=False, box=False, sat=1.0,
            wear=0.0, wear_rgb=(0.6, 0.5, 0.4), wear_rough=0.6, wear_metal=None, grime=1.0, bevel=0.005, dust=1.0):
    """One real material for the detailed meshes. `key` picks the scan, laid `tile` metres to a repeat,
    brought to saturation `sat` and multiplied by `tint`; `flat` is a plain colour instead. `rough`
    remaps the scan's roughness.
    `wear` shows `wear_rgb` where edges rub, `grime` darkens the hollows and low down, and `dust`
    settles on what faces up. Everything but the wear and the scan is shared."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    nor = None
    if key:
        c_path, r_path, n_path = maps(key)
        src = co if box else g.node("ShaderNodeUVMap", uv_map="uv_mat").outputs["UV"]
        map_ = g.node("ShaderNodeMapping")
        g.set(map_.inputs["Vector"], src)
        map_.inputs["Scale"].default_value = (1 / tile, 1 / tile, 1 / tile)
        if grain_v:
            map_.inputs["Rotation"].default_value = (0, 0, math.pi / 2)
        coord = map_.outputs[0]

        def tex(path, non_color):
            img = bpy.data.images.load(path, check_existing=True)
            if non_color:
                img.colorspace_settings.name = "Non-Color"
            n = g.node("ShaderNodeTexImage", image=img)
            if box:
                n.projection, n.projection_blend = "BOX", 0.25
            g.set(n.inputs["Vector"], coord)
            return n

        base = tex(c_path, False).outputs["Color"]
        if sat != 1.0:
            base = g.node("ShaderNodeHueSaturation", inputs={"Saturation": sat, "Color": base}).outputs["Color"]
        r = g.remap(tex(r_path, True).outputs["Color"], 0.0, 1.0, rough[0], rough[1])
        nm = g.node("ShaderNodeNormalMap", uv_map="uv_mat", inputs={"Color": tex(n_path, True).outputs["Color"], "Strength": bump})
        nor = nm.outputs["Normal"]
        multiply = g.node("ShaderNodeMix", data_type="RGBA", blend_type="MULTIPLY", clamp_factor=True)
        multiply.inputs[0].default_value = 1.0
        g.set(multiply.inputs[6], base)
        g.set(multiply.inputs[7], (*tint, 1.0))
        base = multiply.outputs[2]
    else:
        base = (*lin(flat), 1.0)
        r = rough[1]

    rounded = g.node("ShaderNodeBevel", samples=6)
    rounded.inputs["Radius"].default_value = bevel
    if nor is not None:
        g.set(rounded.inputs["Normal"], nor)

    edge = g.edge_mask(bevel * 1.6, 2.6)
    cavity = g.sub(1.0, g.ao(0.05))
    patch = g.noise(co, 9.0, 5.0, 0.55)
    speck = g.noise(co, 55.0, 4.0, 0.6)

    # Edges rubbed through, in patches.
    rub = g.mul(g.smooth(g.add(edge, g.mul(g.sub(speck, 0.5), 1.8)), 0.55, 0.8), wear)
    worn = g.mixc(rub, base, (*lin(wear_rgb), 1.0))
    z = g.node("ShaderNodeSeparateXYZ", inputs={"Vector": co}).outputs["Z"]
    low = g.smooth(g.add(z, g.mul(g.sub(patch, 0.5), 0.5)), 0.30, 0.0)
    dirt = g.mul(g.clamp(g.add(g.mul(cavity, 1.6), g.mul(low, 0.45))), grime)
    dark = g.node("ShaderNodeMix", data_type="RGBA", blend_type="MULTIPLY", clamp_factor=True)
    g.set(dark.inputs[0], g.mul(dirt, 0.75))
    g.set(dark.inputs[6], worn)
    dark.inputs[7].default_value = (*lin((0.42, 0.37, 0.32)), 1.0)
    # Dust lies on what faces up: Delhi's, pale and warm.
    up = g.smooth(g.node("ShaderNodeSeparateXYZ", inputs={"Vector": g.geometry("Normal")}).outputs["Z"], 0.55, 0.95)
    settled = g.mul(g.mul(up, g.smooth(patch, 0.32, 0.72)), 0.13 * dust)
    final = g.mixc(settled, dark.outputs[2], (*lin((0.62, 0.56, 0.47)), 1.0))

    r2 = g.mix(rub, r, wear_rough)
    r3 = g.clamp(g.add(r2, g.add(g.mul(dirt, 0.14), g.mul(settled, 0.5))))
    metal_v = metal if wear_metal is None else g.mix(rub, metal, wear_metal)
    g.principled(final, metal_v, r3, normal=rounded.outputs["Normal"])
    return m


def surfaces():
    colour, height = rope_maps()
    surface("sheesham", "rosewood", 1.6, (0.52, 0.46, 0.42), rough=(0.25, 0.55), grain_v=True, wear=0.8, wear_rgb=(0.50, 0.33, 0.20), wear_rough=0.6)
    surface("frame_wood", "pine", 0.8, (1.2, 1.08, 0.86), sat=0.75, rough=(0.35, 0.7), wear=1.0, wear_rgb=(0.56, 0.43, 0.30), wear_rough=0.7)
    surface("worn_teak", "teak", 0.8, (0.46, 0.38, 0.32), sat=0.8, rough=(0.30, 0.60), wear=0.9, wear_rgb=(0.55, 0.42, 0.29), wear_rough=0.65)
    surface("laminate", "ply", 0.9, (1.0, 0.97, 0.82), sat=0.12, rough=(0.22, 0.40), bump=0.1, wear=0.7, wear_rgb=(0.34, 0.26, 0.19), wear_rough=0.7, bevel=0.003, dust=1.6)
    surface("plywood", "ply", 0.6, (0.74, 0.64, 0.52), rough=(0.32, 0.6), bump=0.6, wear=0.9, wear_rgb=(0.66, 0.54, 0.38), wear_rough=0.75)
    surface("kraft", "jute", 0.9, (0.76, 0.62, 0.50), sat=0.85, rough=(0.70, 0.90), bump=0.08, box=True, wear=0.3, wear_rgb=(0.72, 0.60, 0.45), wear_rough=0.9, bevel=0.003)
    surface("steel_olive", "olive", 1.3, (0.80, 0.84, 0.82), sat=0.62, rough=(0.25, 0.62), bump=0.7, wear=1.0, wear_rgb=(0.30, 0.25, 0.20), wear_rough=0.55, bevel=0.003)
    surface("steel_grey", "olive", 0.8, (1.0, 0.9, 1.0), sat=0.25, rough=(0.3, 0.6), bump=0.5, wear=1.0, wear_rgb=(0.34, 0.28, 0.24), wear_rough=0.6, bevel=0.002)
    surface("brushed", "brushed", 0.5, rough=(0.2, 0.55), metal=1.0, bump=0.4, grime=1.3, bevel=0.002)
    surface("chrome", "brushed", 0.3, rough=(0.1, 0.3), metal=1.0, bump=0.2, grime=0.7, bevel=0.002)
    surface("cast_iron", "black", 0.3, (1.4, 1.4, 1.4), rough=(0.5, 0.8), metal=0.35, grime=0.4, bevel=0.002)
    surface("cylinder_red", "red", 0.45, (0.50, 0.26, 0.24), rough=(0.25, 0.55), wear=0.8, wear_rgb=(0.30, 0.27, 0.25), wear_rough=0.6, wear_metal=0.6, bevel=0.003)
    surface("plastic_white", "white", 0.5, (1.0, 0.97, 0.88), rough=(0.34, 0.5), bump=0.6, wear=0.5, wear_rgb=(0.92, 0.91, 0.86), wear_rough=0.6, grime=1.3, bevel=0.008)
    surface("plastic_blue", "blue", 0.6, (0.74, 0.80, 0.90), rough=(0.30, 0.48), bump=0.7, wear=0.5, wear_rgb=(0.42, 0.56, 0.74), wear_rough=0.6, bevel=0.004)
    surface("plastic_black", "black", 0.4, (1.6, 1.6, 1.6), rough=(0.3, 0.5), bump=0.4, grime=0.5, bevel=0.002)
    surface("ivory", "white", 0.5, (1.12, 1.06, 0.93), rough=(0.3, 0.45), wear=0.5, wear_rgb=(0.60, 0.58, 0.54), wear_rough=0.5, wear_metal=0.5, grime=1.2, dust=5.0, bevel=0.002)
    surface("jute", "jute", 0.27, (0.92, 0.84, 0.74), rough=(0.7, 0.95), bump=1.2, box=True, grime=1.4, bevel=0.0)
    surface("sack_white", "jute", 0.14, (1.25, 1.42, 1.75), sat=0.55, rough=(0.5, 0.75), bump=0.9, box=True, grime=1.4, bevel=0.0)
    surface("velvet", "velvet", 0.28, (0.42, 0.85, 0.55), rough=(0.6, 0.9), bump=1.0, box=True, grime=1.1, bevel=0.012, dust=0.5)
    surface("screen", flat=(0.012, 0.013, 0.015), rough=(0.1, 0.1), grime=0.0, dust=0.6, bevel=0.001)
    surface("gap", flat=(0.02, 0.02, 0.02), rough=(0.9, 0.9), grime=0.0, dust=0.0, bevel=0.0)
    surface("brass", "brushed", 0.3, (0.85, 0.68, 0.38), rough=(0.25, 0.5), metal=1.0, bump=0.2, grime=0.9, bevel=0.002)

    # The rope's own: drawn maps instead of a scan.
    m = new_material("rope")
    g = Graph(m)
    uv = g.mapping(g.node("ShaderNodeUVMap", uv_map="uv_mat").outputs["UV"], scale=(1 / 0.32, 1 / 0.32, 1))
    co = g.texcoord("Object")
    base = g.node("ShaderNodeTexImage", image=colour, inputs={"Vector": uv}).outputs["Color"]
    h = g.node("ShaderNodeTexImage", image=height, inputs={"Vector": uv}).outputs["Color"]
    stain = g.smooth(g.noise(co, 4.0, 6.0, 0.6), 0.35, 0.68)
    shade = g.node("ShaderNodeMix", data_type="RGBA", blend_type="MULTIPLY", clamp_factor=True)
    g.set(shade.inputs[0], g.mul(stain, 0.55))
    g.set(shade.inputs[6], base)
    shade.inputs[7].default_value = (*lin((0.56, 0.50, 0.44)), 1.0)
    g.principled(shade.outputs[2], 0.0, 0.85, normal=g.bump(h, 1.0, 0.007))
    return m


SURFACES = ["sheesham", "frame_wood", "worn_teak", "laminate", "plywood", "kraft", "steel_olive", "steel_grey", "brushed", "chrome", "cast_iron", "cylinder_red",
            "plastic_white", "plastic_blue", "plastic_black", "ivory", "jute", "sack_white", "velvet", "screen", "gap", "brass", "rope"]
INDEX = {name: i for i, name in enumerate(SURFACES)}


# ------------------------------------------------------------------ building a piece


def flat_uv(t, rng):
    """Metric UVs for the scans: each face laid flat, U along its longest edge, so grain runs along
    the board. A random start, so two boards don't show the same wood."""
    uv = t.loops.layers.uv.get("uv_mat") or t.loops.layers.uv.new("uv_mat")
    start = rng.random() * 5, rng.random() * 5
    t.normal_update()
    for f in t.faces:
        e = max(f.edges, key=lambda e: e.calc_length())
        u = (e.verts[1].co - e.verts[0].co).normalized()
        v = f.normal.cross(u)
        for loop in f.loops:
            loop[uv].uv = (loop.vert.co.dot(u) + start[0], loop.vert.co.dot(v) + start[1])


def revolve(profile, seg, matrix, surf_uv_start, caps=(True, True), hard=40.0):
    """A turned shape: `profile` is (radius, height) rings, spun about Z. Corners sharper than `hard`
    degrees stay crisp, the rest is smooth."""
    if profile[0][1] > profile[-1][1]:  # always upward, or the faces turn inward
        profile, caps = profile[::-1], caps[::-1]
    t = bmesh.new()
    uv = t.loops.layers.uv.new("uv_mat")
    rings = []
    for r, z in profile:
        ring = []
        for k in range(seg):
            a = 2 * math.pi * k / seg
            ring.append(t.verts.new(matrix @ Vector((r * math.cos(a), r * math.sin(a), z))))
        rings.append(ring)
    s = surf_uv_start
    mean_r = sum(r for r, _ in profile) / len(profile)
    arc = 2 * math.pi * mean_r / seg
    for i in range(len(profile) - 1):
        ds = math.dist(profile[i], profile[i + 1])
        for k in range(seg):
            n = (k + 1) % seg
            f = t.faces.new((rings[i][k], rings[i][n], rings[i + 1][n], rings[i + 1][k]))
            f.smooth = True
            for loop, (a, b) in zip(f.loops, ((s, k), (s, k + 1), (s + ds, k + 1), (s + ds, k))):
                loop[uv].uv = (a, b * arc)
        s += ds
    for flag, ring in zip(caps, (list(reversed(rings[0])), rings[-1])):
        if flag:
            f = t.faces.new(ring)
            f.smooth = True
            for loop in f.loops:
                c = loop.vert.co
                loop[uv].uv = (c.x + s, c.y)
    t.edges.ensure_lookup_table()
    # Where the profile turns hard, and round the caps.
    dirs = [Vector((b[0] - a[0], b[1] - a[1])) for a, b in zip(profile, profile[1:])]
    sharp = {i + 1 for i, (a, b) in enumerate(zip(dirs, dirs[1:])) if a.length > 1e-6 and b.length > 1e-6 and a.angle(b, 0.0) > math.radians(hard)}
    if caps[0]:
        sharp.add(0)
    if caps[1]:
        sharp.add(len(profile) - 1)
    for i in sharp:
        ring = set(rings[i])
        for e in t.edges:
            if e.verts[0] in ring and e.verts[1] in ring:
                e.smooth = False
    return t


def hidden_faces(bm):
    """Faces nobody can see: pressed against another part, or inside one, all over."""
    bvh = BVHTree.FromBMesh(bm)
    ways = [Vector(v) for v in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))]

    def buried(point, normal):
        start = point + normal * 2e-4
        if bvh.ray_cast(start, normal, 0.008)[0] is not None:
            return True
        for d in ways:
            hit, n, _, _ = bvh.ray_cast(start, d, 4.0)
            if hit is None or n.dot(d) < 0:
                return False
        return True

    bm.normal_update()
    out = []
    for f in bm.faces:
        c = f.calc_center_median()
        if all(buried(c.lerp(v.co, t), f.normal) for v in f.verts for t in (0.0, 0.5, 0.95)):
            out.append(f)
    return out


class Piece:
    """One kit piece: `lo` is what ships, `hi` what's baked from. A part goes to both unless it's
    `detail`, which is baked only."""

    def __init__(self, name, cage=CAGE):
        self.name = name
        self.cage = cage
        self.lo = bmesh.new()
        self.hi = bmesh.new()
        self.rng = random.Random(name)

    def _emit(self, t, surf, lo=True, hi=True):
        for f in t.faces:
            f.material_index = INDEX[surf]
        me = bpy.data.meshes.new("part")
        t.to_mesh(me)
        t.free()
        if lo:
            self.lo.from_mesh(me)
        if hi:
            self.hi.from_mesh(me)
        bpy.data.meshes.remove(me)

    def box(self, mn, mx, surf, detail=False):
        t = bmesh.new()
        c = [(a + b) / 2 for a, b in zip(mn, mx)]
        s = [b - a for a, b in zip(mn, mx)]
        bmesh.ops.create_cube(t, size=1.0, matrix=Matrix.Translation(c) @ Matrix.Diagonal((*s, 1.0)))
        for e in t.edges:
            e.smooth = False
        for f in t.faces:
            f.smooth = True
        flat_uv(t, self.rng)
        self._emit(t, surf, lo=not detail)

    def hull(self, pts, surf, detail=False, smooth=False):
        t = bmesh.new()
        new = [t.verts.new(p) for p in pts]
        res = bmesh.ops.convex_hull(t, input=new)
        loose = [v for v in res.get("geom_interior", []) if isinstance(v, bmesh.types.BMVert)]
        bmesh.ops.delete(t, geom=loose, context="VERTS")
        bmesh.ops.dissolve_limit(t, angle_limit=math.radians(2), verts=t.verts[:], edges=t.edges[:])
        for f in t.faces:
            f.smooth = True
        t.normal_update()
        if not smooth:
            for e in t.edges:
                if len(e.link_faces) == 2 and e.calc_face_angle() > math.radians(35):
                    e.smooth = False
        flat_uv(t, self.rng)
        self._emit(t, surf, lo=not detail)

    def tube(self, points, w, h, surf, up=(0, 0, 1), side=None, caps=(True, True), detail=False, smooth=False, curve=0):
        """A rectangular bar along `points`, `w` across and `h` in the `up` direction, each a number or
        one per point. A path that turns through `up` keeps a fixed `side` for its width instead. With
        `curve` the detailed version follows a smooth line through them, in that many rings."""
        pts = [Vector(p) for p in points]
        up = Vector(up)
        fixed = Vector(side) if side else None
        n = len(pts)
        ws = list(w) if isinstance(w, (list, tuple)) else [w] * n
        hs = list(h) if isinstance(h, (list, tuple)) else [h] * n

        def bar(path, across, tall):
            t = bmesh.new()
            rings = []
            for i, p in enumerate(path):
                d = (path[min(i + 1, len(path) - 1)] - path[max(i - 1, 0)]).normalized()
                out = fixed if fixed else d.cross(up).normalized()
                top = out.cross(d).normalized()
                rings.append([t.verts.new(p + out * x * across[i] / 2 + top * z * tall[i] / 2) for x, z in ((-1, -1), (1, -1), (1, 1), (-1, 1))])
            for a, b in zip(rings, rings[1:]):
                for k in range(4):
                    m = (k + 1) % 4
                    t.faces.new((a[k], a[m], b[m], b[k]))
            if caps[0]:
                t.faces.new(list(reversed(rings[0])))
            if caps[1]:
                t.faces.new(rings[-1])
            for f in t.faces:
                f.smooth = True
            bmesh.ops.recalc_face_normals(t, faces=t.faces[:])
            t.normal_update()
            # The long edges stay crisp, and the rounding comes from the bake; rings stay smooth.
            for e in t.edges:
                if not smooth and len(e.link_faces) == 2 and e.calc_face_angle() > math.radians(50):
                    e.smooth = False
            flat_uv(t, self.rng)
            return t

        if curve:
            at = [i * (n - 1) / (curve - 1) for i in range(curve)]
            self._emit(bar(geo.catmull_rom(pts, curve), list(np.interp(at, range(n), ws)), list(np.interp(at, range(n), hs))), surf, lo=False)
            self._emit(bar(pts, ws, hs), surf, hi=False)
        else:
            self._emit(bar(pts, ws, hs), surf, lo=not detail)

    def turned(self, centre, hi, lo, surf, seg=8, hi_seg=32, caps=(True, True), axis=None, detail=False):
        """A lathe piece on `centre`, along Z or from there along `axis`. `hi` is the true profile,
        `lo` the few rings that ship."""
        m = Matrix.Translation(centre)
        if axis is not None:
            m = m @ Vector(axis).to_track_quat("Z", "Y").to_matrix().to_4x4()
        s = self.rng.random() * 5
        self._emit(revolve(hi, hi_seg, m, s, caps), surf, lo=False)
        if lo and not detail:
            self._emit(revolve(lo, seg, m, s, caps), surf, hi=False)

    def rod(self, a, b, r, surf, seg=6, hi_seg=16, r2=None, caps=(True, True), detail=False):
        a, b = Vector(a), Vector(b)
        d = b - a
        prof = [(r, 0.0), (r if r2 is None else r2, d.length)]
        self.turned(a, prof, prof, surf, seg, hi_seg, caps, axis=d, detail=detail)

    def pillow(self, centre, size, surf, yaw=0.0, roll=0.0, pitch=0.0, wrinkle=1.0, lo_cuts=1, hi_cuts=40):
        """A soft stuffed shape: a box drawn in at the corners, thinner to its ends, wrinkled in the
        detailed version."""
        m = Matrix.Translation(centre) @ Matrix.Rotation(yaw, 4, "Z") @ Matrix.Rotation(roll, 4, "X") @ Matrix.Rotation(pitch, 4, "Y")
        lx, ly, lz = size

        def build(cuts, detail):
            t = bmesh.new()
            bmesh.ops.create_cube(t, size=2.0)
            bmesh.ops.subdivide_edges(t, edges=t.edges[:], cuts=cuts, use_grid_fill=True)
            for f in t.faces:
                f.smooth = True
            seed = Vector((self.rng.random() * 50, self.rng.random() * 50, self.rng.random() * 50))
            for v in t.verts:
                x, y, z = v.co
                ends = 1 - abs(x) ** 3
                sides = 1 - abs(y) ** 3
                thick = (0.42 + 0.58 * ends) * (0.60 + 0.40 * sides)
                p = Vector((x * lx / 2 * (1 + 0.05 * (1 - z * z)), y * ly / 2 * (0.94 + 0.08 * ends) * (1 + 0.05 * (1 - z * z)), z * lz / 2 * thick))
                if detail:
                    q = Vector((x * lx, y * ly, z * lz)) + seed
                    fold = abs(noise.noise(q * 5.0)) * 0.018 + abs(noise.noise(q * 13.0)) * 0.006
                    lump = noise.noise(q * 2.2) * 0.022
                    p += p.normalized() * (lump - fold) * wrinkle
                v.co = m @ p
            flat_uv(t, self.rng)
            return t

        self._emit(build(hi_cuts, True), surf, lo=False)
        self._emit(build(lo_cuts, False), surf, hi=False)

    def imported(self, path, fit, mats, turn=0.0, at=(0, 0, 0), ratio=None, warp=None, paint=None):
        """A free model's mesh: its material names mapped to ours, turned, then stretched to `fit` and
        stood at `at`. `warp` moves its points after that, and `paint` may give a face at some point
        another surface. `ratio` thins the shipped mesh; the detailed one keeps every triangle."""
        before = set(bpy.data.objects)
        bpy.ops.wm.obj_import(filepath=path, forward_axis="NEGATIVE_Z", up_axis="Y")
        objs = [o for o in bpy.data.objects if o not in before and o.type == "MESH"]
        t = bmesh.new()
        names = []
        for o in objs:
            me = o.data
            part = bmesh.new()
            part.from_mesh(me)
            part.transform(Matrix.Rotation(turn, 4, "Z") @ o.matrix_world)
            slots = [s.material.name if s.material else "" for s in o.material_slots]
            for f in part.faces:
                f.material_index = len(names) + f.material_index
            names += slots
            tmp = bpy.data.meshes.new("tmp")
            part.to_mesh(tmp)
            part.free()
            t.from_mesh(tmp)
            bpy.data.meshes.remove(tmp)
        for o in objs:
            me, mts = o.data, [s.material for s in o.material_slots]
            bpy.data.objects.remove(o)
            bpy.data.meshes.remove(me)
            for mt in mts:
                if mt and mt.users == 0:
                    bpy.data.materials.remove(mt)
        bmesh.ops.remove_doubles(t, verts=t.verts, dist=1e-4)
        # Slivers between T-junctions have no area and spoil the tangents around them.
        bmesh.ops.dissolve_degenerate(t, dist=1e-4, edges=t.edges[:])
        for f in t.faces:
            f.material_index = INDEX[next(v for k, v in mats.items() if names[f.material_index].startswith(k))]
        bmesh.ops.dissolve_limit(t, angle_limit=math.radians(1.0), verts=t.verts[:], edges=t.edges[:], delimit={"MATERIAL"})
        lo = Vector([min(v.co[k] for v in t.verts) for k in range(3)])
        hi = Vector([max(v.co[k] for v in t.verts) for k in range(3)])
        s = [f / (b - a) for f, a, b in zip(fit, lo, hi)]
        for v in t.verts:
            c = v.co
            v.co = Vector(((c.x - (lo.x + hi.x) / 2) * s[0] + at[0], (c.y - (lo.y + hi.y) / 2) * s[1] + at[1], (c.z - lo.z) * s[2] + at[2]))
            if warp:
                v.co = warp(v.co)
        t.normal_update()
        if paint:
            for f in t.faces:
                name = paint(f.calc_center_median())
                if name:
                    f.material_index = INDEX[name]
        for f in t.faces:
            f.smooth = True
        for e in t.edges:
            if len(e.link_faces) == 2 and e.calc_face_angle() > math.radians(35):
                e.smooth = False
        flat_uv(t, self.rng)
        me = bpy.data.meshes.new("part")
        t.to_mesh(me)
        t.free()
        self.hi.from_mesh(me)
        if not ratio:
            self.lo.from_mesh(me)
            bpy.data.meshes.remove(me)
            return
        # Thinned on a copy; the full mesh is what's baked from.
        o = bpy.data.objects.new("thin", me)
        bpy.context.scene.collection.objects.link(o)
        d = o.modifiers.new("thin", "DECIMATE")
        d.ratio = ratio
        bpy.context.view_layer.objects.active = o
        bpy.ops.object.modifier_apply(modifier=d.name)
        self.lo.from_mesh(o.data)
        bpy.data.objects.remove(o)
        bpy.data.meshes.remove(me)

    def finish(self):
        """The two objects: `name`, low, with a blank UV map, and `name_hi`, every surface in order."""
        bmesh.ops.triangulate(self.lo, faces=self.lo.faces[:], quad_method="SHORT_EDGE", ngon_method="BEAUTY")
        self.lo.normal_update()
        floor = [f for f in self.lo.faces if -0.01 < min(v.co.z for v in f.verts) and max(v.co.z for v in f.verts) < 0.02 and f.normal.z < -0.5]
        for f in floor + hidden_faces(self.lo):
            if f.is_valid:
                self.lo.faces.remove(f)
        bmesh.ops.delete(self.lo, geom=[v for v in self.lo.verts if not v.link_faces], context="VERTS")
        # Triangles with no area spoil the tangents around them: where three points of a T-junction
        # made one, flip its long edge.
        for _ in range(4):
            for f in [f for f in self.lo.faces if f.calc_area() < 1e-7]:
                if f.is_valid:
                    bmesh.ops.rotate_edges(self.lo, edges=[max(f.edges, key=lambda e: e.calc_length())], use_ccw=False)
        for f in [f for f in self.lo.faces if f.calc_area() < 1e-7]:
            self.lo.faces.remove(f)
        self.lo.normal_update()
        low = bpy.data.meshes.new(self.name)
        self.lo.to_mesh(low)
        self.lo.free()
        for layer in list(low.uv_layers):
            low.uv_layers.remove(layer)
        low.uv_layers.new(name="UVMap")
        for poly in low.polygons:
            poly.material_index = 0
        lo = bpy.data.objects.new(self.name, low)
        bpy.context.scene.collection.objects.link(lo)

        high = bpy.data.meshes.new(f"{self.name}_hi")
        self.hi.to_mesh(high)
        self.hi.free()
        for s in SURFACES:
            high.materials.append(bpy.data.materials[s])
        hi = bpy.data.objects.new(high.name, high)
        bpy.context.scene.collection.objects.link(hi)
        return lo, hi, self.cage


# ------------------------------------------------------------------ the pieces


def charpai():
    """The string bed: four turned legs, four rails, rope woven across. Long side on X."""
    p = Piece("charpai", cage=ROUND_CAGE)
    lx, ly, top = 1.95, 0.95, 0.46
    hi_leg = [(0.030, 0.0), (0.038, 0.02), (0.046, 0.07), (0.031, 0.115), (0.031, 0.13), (0.045, 0.17), (0.045, 0.21), (0.032, 0.245),
              (0.047, 0.285), (0.047, 0.47), (0.040, 0.49), (0.012, 0.50)]
    lo_leg = [(0.032, 0.0), (0.046, 0.08), (0.046, 0.47), (0.012, 0.50)]
    corners = [(sx * (lx / 2 - 0.047), sy * (ly / 2 - 0.047)) for sx in (-1, 1) for sy in (-1, 1)]
    for x, y in corners:
        p.turned((x, y, 0), hi_leg, lo_leg, "frame_wood", seg=6, hi_seg=24)
    h = 0.075
    rail_z = top - h / 2
    for sy in (-1, 1):
        p.box((-lx / 2 + 0.06, sy * (ly / 2 - 0.047) - 0.03, rail_z - h / 2), (lx / 2 - 0.06, sy * (ly / 2 - 0.047) + 0.03, rail_z + h / 2), "frame_wood")
    for sx in (-1, 1):
        p.box((sx * (lx / 2 - 0.047) - 0.03, -ly / 2 + 0.06, rail_z - h / 2), (sx * (lx / 2 - 0.047) + 0.03, ly / 2 - 0.06, rail_z + h / 2), "frame_wood")

    # The bed: sagging a little, chevrons meeting down the middle.
    bx, by = lx / 2 - 0.075, ly / 2 - 0.075

    def bed(nx, ny, emit_hi):
        t = bmesh.new()
        uv = t.loops.layers.uv.new("uv_mat")
        for side in (1, -1):
            grid = []
            for i in range(nx + 1):
                x = -bx + 2 * bx * i / nx
                row = []
                for j in range(ny + 1):
                    y = -by + 2 * by * j / ny
                    sag = 0.038 * (1 - (x / bx) ** 2) * (1 - (y / by) ** 2)
                    row.append(t.verts.new((x, y, top - 0.004 - sag + (0 if side > 0 else (-0.014 if emit_hi else -0.024)))))
                grid.append(row)
            for i in range(nx):
                for j in range(ny):
                    corners = (grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1])
                    f = t.faces.new(corners if side > 0 else corners[::-1])
                    f.smooth = True
                    for loop in f.loops:
                        c = loop.vert.co
                        loop[uv].uv = (abs(c.x), c.y)
        p._emit(t, "rope", lo=not emit_hi, hi=emit_hi)

    bed(72, 36, True)
    bed(6, 2, False)
    # Where the rope goes round the rails, close turns hide the wood.
    for sy in (-1, 1):
        for k in range(int((2 * bx) / 0.022)):
            x = -bx + 0.011 + 0.022 * k
            y = sy * (ly / 2 - 0.047)
            p.tube([(x, y - 0.036, rail_z + 0.040), (x, y + 0.036, rail_z + 0.040)], 0.018, 0.012, "rope", detail=True)
            p.tube([(x, y + sy * 0.034, rail_z + 0.040), (x, y + sy * 0.034, rail_z - 0.042)], 0.018, 0.012, "rope", up=(0, 1, 0), detail=True)
    for sx in (-1, 1):
        for k in range(int((2 * by) / 0.022)):
            y = -by + 0.011 + 0.022 * k
            x = sx * (lx / 2 - 0.047)
            p.tube([(x - 0.036, y, rail_z + 0.040), (x + 0.036, y, rail_z + 0.040)], 0.018, 0.012, "rope", up=(0, 0, 1), detail=True)
            p.tube([(x + sx * 0.034, y, rail_z + 0.040), (x + sx * 0.034, y, rail_z - 0.042)], 0.018, 0.012, "rope", up=(1, 0, 0), detail=True)
    return p


def almirah():
    """The steel cupboard every home has: two doors, a handle with a lock, a plinth, vent slits."""
    p = Piece("almirah")
    w, d, h = 0.91, 0.50, 1.83
    back, door_face = -d / 2, d / 2
    body_front = door_face - 0.022
    p.box((-w / 2 + 0.012, back + 0.012, 0.0), (w / 2 - 0.012, body_front - 0.03, 0.075), "gap")
    p.box((-w / 2, back, 0.07), (w / 2, body_front, h), "steel_olive")
    gap = 0.003
    for sx in (-1, 1):
        a, b = sorted((sx * gap, sx * (w / 2 - 0.012)))
        p.box((a, body_front, 0.095), (b, door_face, h - 0.022), "steel_olive")
    # Handle and lock on the right door.
    p.box((0.038, door_face, 0.90), (0.062, door_face + 0.012, 1.10), "chrome")
    p.hull([(x, door_face + y, z) for x in (0.042, 0.058) for z in (0.915, 1.085) for y in (0.012, 0.034)], "chrome")
    # Pressed panels, seams, hinges, vents: baked only.
    for sx in (-1, 1):
        a, b = sorted((sx * 0.055, sx * (w / 2 - 0.06)))
        p.box((a, door_face - 0.001, 0.18), (b, door_face + 0.0022, h - 0.12), "steel_olive", detail=True)
        p.box((a + 0.015, door_face + 0.0015, 0.195), (b - 0.015, door_face + 0.0036, h - 0.135), "steel_olive", detail=True)
        for k in range(6):
            z = 0.115 + k * 0.009
            xa, xb = sorted((sx * 0.16, sx * 0.30))
            p.box((xa, door_face - 0.0005, z), (xb, door_face + 0.0016, z + 0.004), "gap", detail=True)
        x = sx * (w / 2 - 0.004)
        for z in (0.30, h - 0.30, h / 2):
            p.rod((x, door_face - 0.022, z - 0.04), (x, door_face - 0.022, z + 0.04), 0.009, "chrome", detail=True)
    p.box((0.066, door_face - 0.0005, 0.985), (0.112, door_face + 0.0035, 1.05), "chrome", detail=True)
    p.rod((0.089, door_face + 0.0030, 1.018), (0.089, door_face + 0.0052, 1.018), 0.012, "brass", detail=True)
    p.box((0.0875, door_face + 0.0049, 1.008), (0.0905, door_face + 0.0058, 1.028), "gap", detail=True)
    return p


def table():
    """A dhaba's table: a wooden frame and a sunmica top."""
    p = Piece("table")
    top = KIT["table"][2] - 0.075
    p.imported(os.path.join(SRC, KENNEY, "table.obj"), KIT["table"], {"wood": "frame_wood"}, paint=lambda c: "laminate" if c.z > top else None)
    return p


def plastic_chair():
    """The monobloc: moulded in one, arms and all. It's the one chair in every dhaba and wedding."""
    p = Piece("plastic_chair", cage=ROUND_CAGE)
    seat = 0.44
    s = "plastic_white"
    # Seat: a rounded plate, tipped back.
    outline = [(-0.21, -0.20), (-0.13, -0.235), (0.13, -0.235), (0.21, -0.20), (0.235, -0.10), (0.24, 0.12),
               (0.20, 0.215), (0.08, 0.235), (-0.08, 0.235), (-0.20, 0.215), (-0.24, 0.12), (-0.235, -0.10)]
    p.hull([(x, y, seat + 0.06 * y + z) for x, y in outline for z in (-0.022, 0.0)], s)
    # Legs: four flared channels, wider at the top.
    for sx in (-1, 1):
        p.tube([(sx * 0.215, 0.20, seat - 0.015), (sx * 0.25, 0.235, 0.002)], [0.072, 0.05], [0.066, 0.046], s, caps=(False, False))
        p.tube([(sx * 0.20, -0.19, seat - 0.02), (sx * 0.24, -0.26, 0.002)], [0.072, 0.05], [0.066, 0.046], s, caps=(False, False))

    # Back: two uprights, a curved top rail and two wide slats.
    def back(x, z):
        return (x, -0.21 - 0.06 * (z - seat) / 0.40 + 0.075 * (x / 0.24) ** 2, z)

    for sx in (-1, 1):
        p.tube([back(sx * 0.222, z) for z in (seat - 0.02, 0.828)], 0.056, 0.03, s, up=(0, 1, 0), caps=(False, True))
    # They end inside the uprights, and are thinner, so no two faces lie in one plane.
    for z, h in ((0.795, 0.078), (0.70, 0.062), (0.61, 0.062)):
        p.tube([back(x, z) for x in (-0.222, -0.09, 0.09, 0.222)], 0.02, h, s, caps=(False, False), curve=12)
    # Arms: from the uprights, forward, down into the front legs.
    for sx in (-1, 1):
        path = [(sx * 0.232, -0.20, 0.655), (sx * 0.268, -0.06, 0.668), (sx * 0.275, 0.10, 0.66), (sx * 0.262, 0.205, 0.605), (sx * 0.226, 0.212, seat - 0.02)]
        p.tube(path, [0.05, 0.056, 0.06, 0.06, 0.066], 0.022, s, side=(1, 0, 0), caps=(False, False), curve=24)
    return p


def stool():
    """A wooden stool, 43 cm high once its seat is pressed down."""
    p = Piece("stool")
    # Its seat cushion, pressed down to a plank's thickness.
    p.imported(os.path.join(SRC, KENNEY, "stoolBarSquare.obj"), KIT["stool"], {"wood": "frame_wood", "carpet": "worn_teak"},
               warp=lambda c: Vector((c.x, c.y, 0.415 + (c.z - 0.415) * 0.5 if c.z > 0.415 else c.z)))
    return p


def shelf():
    """A slotted-angle rack, the steel kind in every godown and shop, with cartons and tins on it."""
    p = Piece("shelf", cage=ROUND_CAGE)
    w, d, h = 0.92, 0.40, 1.95
    a, t = 0.04, 0.006
    for sx in (-1, 1):
        for sy in (-1, 1):
            x, y = sx * (w / 2), sy * (d / 2)
            # An L: one flange along each side, its corner outward.
            for flange in (((x, y), (x - sx * a, y), (x - sx * a, y - sy * t), (x - sx * t, y - sy * t)), ((x, y), (x, y - sy * a), (x - sx * t, y - sy * a), (x - sx * t, y))):
                p.hull([(fx, fy, z) for fx, fy in flange for z in (0.0, h)], "steel_grey")
            # Punched slots, both flanges.
            for k in range(int(h / 0.075)):
                z = 0.03 + k * 0.075
                p.box((x - sx * 0.030 - 0.009, y - sy * 0.001, z), (x - sx * 0.030 + 0.009, y + sy * 0.0012, z + 0.03), "gap", detail=True)
                p.box((x - sx * 0.0005, y - sy * 0.030 - 0.009, z), (x + sx * 0.0012, y - sy * 0.030 + 0.009, z + 0.03), "gap", detail=True)
    levels = (0.10, 0.65, 1.20, 1.75)
    for z in levels:
        p.box((-w / 2 + 0.002, -d / 2 + 0.002, z - 0.03), (w / 2 - 0.002, d / 2 - 0.002, z), "steel_grey")
        for sx in (-1, 1):
            for sy in (-1, 1):
                x = sx * (w / 2 - 0.02)
                p.rod((x, sy * (d / 2 + 0.0002), z - 0.016), (x, sy * (d / 2 + 0.004), z - 0.016), 0.006, "chrome", hi_seg=6, detail=True)
    # What's on it: plain cartons, taped, and tins.
    cartons = ((-0.40, -0.06, -0.15, 0.14, 0.10, 0.30), (-0.02, 0.40, -0.17, 0.15, 0.10, 0.27), (-0.38, -0.02, -0.16, 0.15, 0.65, 0.92),
               (0.06, 0.38, -0.15, 0.13, 1.20, 1.42), (-0.40, -0.14, -0.13, 0.14, 1.20, 1.36))
    for x0, x1, y0, y1, z0, top in cartons:
        p.box((x0, y0, z0), (x1, y1, top), "kraft")
        p.box((x0 - 0.0008, (y0 + y1) / 2 - 0.022, z0 + 0.002), (x1 + 0.0008, (y0 + y1) / 2 + 0.022, top + 0.0008), "plywood", detail=True)
    for x, z in ((0.14, 0.65), (0.30, 0.65)):
        tin = [(0.075, 0.0), (0.078, 0.004), (0.078, 0.20), (0.074, 0.205), (0.074, 0.212)]
        p.turned((x, 0.0, z), tin, [(0.078, 0.0), (0.078, 0.205)], "ivory", seg=7, hi_seg=24, caps=(False, True))
    return p


def counter():
    """A shop's counter: varnished ply, a sheesham moulding, a sunmica top."""
    p = Piece("counter")
    p.imported(os.path.join(SRC, KENNEY, "kitchenBar.obj"), KIT["counter"], {"woodDark": "sheesham", "wood": "plywood", "metal": "laminate"})
    return p


def sacks():
    """Fifty-kilo sacks, stacked as a godown stacks them: two, two across, one. Jute, and one white
    plastic one."""
    p = Piece("sacks", cage=SOFT_CAGE)
    lx, ly, lz = 0.92, 0.48, 0.29
    p.pillow((0.04, -0.24, lz * 0.5), (lx, ly, lz), "jute", yaw=0.02)
    p.pillow((-0.02, 0.25, lz * 0.5), (lx, ly, lz), "jute", yaw=-0.03)
    p.pillow((-0.23, 0.01, lz * 1.28), (lx, ly, lz), "jute", yaw=math.pi / 2 + 0.02, roll=0.02)
    p.pillow((0.26, -0.02, lz * 1.28), (lx, ly, lz), "jute", yaw=math.pi / 2 - 0.05, roll=-0.03)
    p.pillow((0.02, 0.0, lz * 2.0), (lx * 0.97, ly, lz * 0.95), "sack_white", yaw=0.12)
    return p


def gas_stove():
    """Two burners on a steel stand, the red cylinder under it, a hose between."""
    p = Piece("gas_stove", cage=ROUND_CAGE)
    w, d, h = 0.62, 0.42, 0.78
    for sx in (-1, 1):
        for sy in (-1, 1):
            p.box((sx * (w / 2 - 0.02) - 0.017, sy * (d / 2 - 0.02) - 0.017, 0.0), (sx * (w / 2 - 0.02) + 0.017, sy * (d / 2 - 0.02) + 0.017, h), "brushed")
    # Hob, with a black fascia.
    p.box((-w / 2, -d / 2, h), (w / 2, d / 2, h + 0.05), "brushed")
    p.box((-w / 2 + 0.01, d / 2 - 0.0005, h + 0.006), (w / 2 - 0.01, d / 2 + 0.0015, h + 0.044), "plastic_black", detail=True)
    top = h + 0.05
    for x in (-0.145, 0.145):
        hi = [(0.096, 0.0), (0.076, 0.006), (0.071, 0.014), (0.066, 0.018), (0.060, 0.022), (0.052, 0.024), (0.0005, 0.026)]
        p.turned((x, 0.0, top), hi, [(0.088, 0.0), (0.066, 0.022)], "cast_iron", seg=6, hi_seg=48, caps=(False, True))
        for k in range(12):
            a = k / 12 * 2 * math.pi
            c, n = (x + 0.067 * math.cos(a), 0.067 * math.sin(a)), 0.0022
            p.box((c[0] - n, c[1] - n, top + 0.012), (c[0] + n, c[1] + n, top + 0.019), "gap", detail=True)
        # The pan support: a cross of iron, resting on the burner.
        for dx, dy in ((1, 0), (0, 1)):
            r = 0.105
            p.tube([(x - dx * r, -dy * r, top + 0.03), (x + dx * r, dy * r, top + 0.03)], 0.013, 0.015, "cast_iron")
            for k in (-1, 1):
                foot = (x + k * dx * r * 0.93, k * dy * r * 0.93)
                p.tube([(*foot, top + 0.03), (*foot, top)], 0.012, 0.012, "cast_iron", up=(1, 0, 0), detail=True)
        # Knob.
        knob = [(0.021, 0.0), (0.021, 0.018), (0.016, 0.028), (0.0005, 0.03)]
        p.turned((x * 0.55, d / 2 - 0.0005, h + 0.025), knob, [(0.021, 0.0), (0.015, 0.028)], "plastic_black", seg=6, hi_seg=24, caps=(False, True), axis=(0, 1, 0))
    # The cylinder: 14.2 kg, a foot ring, a collar round the valve.
    cx, cy = 0.0, -0.01
    body = [(0.135, 0.0), (0.150, 0.004), (0.150, 0.05), (0.138, 0.055), (0.157, 0.06), (0.157, 0.09), (0.157, 0.50), (0.150, 0.535),
            (0.128, 0.565), (0.095, 0.585), (0.045, 0.60), (0.040, 0.615)]
    p.turned((cx, cy, 0.0), body, [(0.150, 0.0), (0.157, 0.07), (0.157, 0.50), (0.128, 0.565), (0.045, 0.60)], "cylinder_red", seg=8, hi_seg=48, caps=(False, False))
    # The collar: a ring on three stays. It ships as a plain drum, the stays baked on it.
    p.turned((cx, cy, 0.585), [(0.101, 0.0), (0.101, 0.07)], [(0.106, 0.0), (0.106, 0.07)], "gap", seg=8, hi_seg=32, caps=(False, True))
    p.turned((cx, cy, 0.642), [(0.106, 0.0), (0.107, 0.004), (0.107, 0.024), (0.106, 0.028)], None, "cylinder_red", hi_seg=32, caps=(False, False))
    for k in range(3):
        a = k * 2.094 + 0.5
        for j in (-1, 1):
            x, y = cx + 0.104 * math.cos(a + j * 0.42), cy + 0.104 * math.sin(a + j * 0.42)
            p.box((x - 0.016, y - 0.016, 0.585), (x + 0.016, y + 0.016, 0.65), "cylinder_red", detail=True)
    # Regulator and hose.
    p.turned((cx, cy, 0.655), [(0.024, 0.0), (0.03, 0.01), (0.03, 0.036), (0.02, 0.046)], [(0.03, 0.0), (0.03, 0.04)], "brushed", seg=6, hi_seg=20)
    p.tube([(cx + 0.02, cy, 0.68), (0.13, 0.05, 0.71), (0.27, 0.17, 0.78)], 0.016, 0.016, "plastic_black", caps=(False, False), up=(0, 1, 0), smooth=True)
    return p


def water_drum():
    """The blue 200 litre drum: rolled hoops, a lid held by a clamp ring."""
    p = Piece("water_drum", cage=ROUND_CAGE)
    s = "plastic_blue"
    hi = [(0.27, 0.0), (0.285, 0.015), (0.285, 0.03), (0.292, 0.04), (0.292, 0.28), (0.300, 0.295), (0.300, 0.325), (0.292, 0.34),
          (0.292, 0.58), (0.300, 0.595), (0.300, 0.625), (0.292, 0.64), (0.292, 0.85), (0.288, 0.865)]
    lo = [(0.285, 0.0), (0.293, 0.035), (0.293, 0.865)]
    p.turned((0, 0, 0), hi, lo, s, seg=14, hi_seg=64, caps=(False, False))
    # Lid, with a ring round it.
    lid = [(0.285, 0.0), (0.292, 0.006), (0.296, 0.012), (0.296, 0.032), (0.282, 0.04), (0.27, 0.046), (0.26, 0.05), (0.26, 0.056), (0.25, 0.058), (0.0005, 0.058)]
    p.turned((0, 0, 0.865), lid, [(0.296, 0.03), (0.262, 0.05), (0.25, 0.058)], s, seg=14, hi_seg=64, caps=(False, True))
    ring = [(0.298, 0.0), (0.304, 0.004), (0.304, 0.05), (0.298, 0.054)]
    p.turned((0, 0, 0.84), ring, ring, "cast_iron", seg=14, hi_seg=64, caps=(False, False))
    p.box((0.04, 0.296, 0.845), (0.10, 0.325, 0.89), "brushed")
    return p


def desk():
    """An office desk with one drawer, its knee hole to the front."""
    p = Piece("desk")
    p.imported(os.path.join(SRC, KENNEY, "desk.obj"), KIT["desk"], {"wood": "worn_teak", "metal": "chrome"})
    return p


def sofa():
    """A Delhi drawing room's two-seater: a sheesham frame and maroon velvet cushions."""
    p = Piece("sofa", cage=SOFT_CAGE)
    p.imported(os.path.join(SRC, QUATERNIUS, "Couch_Medium2.obj"), KIT["sofa"], {"Couch_Mustard": "velvet", "Wood": "sheesham"}, turn=math.pi, ratio=0.40)
    return p


def tv_unit():
    """A sheesham cabinet with a flat TV standing on it."""
    p = Piece("tv_unit")
    w, d, h = KIT["tv_unit"]
    base = 0.50
    p.imported(os.path.join(SRC, KENNEY, "cabinetTelevisionDoors.obj"), (w, d, base), {"wood": "sheesham", "metal": "chrome"})
    p.imported(os.path.join(SRC, KENNEY, "televisionModern.obj"), (0.98, 0.20, h - base), {"metalDark": "screen", "metal": "plastic_black"}, at=(0, -0.05, base))
    return p


def fan():
    """The ceiling fan: canopy, down-rod, motor, three blades. It hangs from its origin."""
    p = Piece("fan", cage=ROUND_CAGE)
    canopy = [(0.0005, 0.07), (0.05, 0.07), (0.075, 0.06), (0.078, 0.05), (0.07, 0.02), (0.02, 0.0)]
    p.turned((0, 0, -0.07), canopy, [(0.0005, 0.07), (0.077, 0.055), (0.02, 0.0)], "ivory", seg=8, hi_seg=32, caps=(False, False))
    p.rod((0, 0, -0.07), (0, 0, -0.30), 0.011, "ivory", seg=5, hi_seg=16, caps=(False, False))
    hi = [(0.022, 0.0), (0.04, 0.01), (0.09, 0.03), (0.118, 0.05), (0.126, 0.07), (0.126, 0.11), (0.116, 0.13), (0.095, 0.145), (0.05, 0.155), (0.0005, 0.158)]
    lo = [(0.022, 0.0), (0.10, 0.035), (0.126, 0.07), (0.126, 0.11), (0.09, 0.148), (0.0005, 0.158)]
    p.turned((0, 0, -0.455), [(r, 0.158 - z) for r, z in hi], [(r, 0.158 - z) for r, z in lo], "ivory", seg=10, hi_seg=48, caps=(False, False))
    for k in range(3):
        a = k * 2 * math.pi / 3 + 0.35
        d = Vector((math.cos(a), math.sin(a), 0))
        s = Vector((-math.sin(a), math.cos(a), 0))
        z = -0.385
        # A bent arm to the rim, then the long blade, pitched and narrowing.
        p.tube([d * 0.11 + Vector((0, 0, z)), d * 0.19 + Vector((0, 0, z + 0.008))], [0.05, 0.04], 0.008, "ivory", caps=(False, False))
        pitch = 0.20
        pts = []
        for r, width in ((0.17, 0.082), (0.60, 0.105)):
            for sgn in (-1, 1):
                for th in (0.0, 0.004):
                    pts.append(d * r + s * sgn * width / 2 + Vector((0, 0, z + 0.006 + sgn * math.sin(pitch) * width / 2 + th)))
        p.hull(pts, "ivory")
    return p


BUILDERS = (charpai, almirah, table, plastic_chair, stool, shelf, counter, sacks, gas_stove, water_drum, desk, sofa, tv_unit, fan)


# ------------------------------------------------------------------ atlas


def unwrap(lows):
    """All the low meshes laid out in one atlas: flat faces together, tops larger, backs and undersides
    smaller."""
    bpy.ops.object.select_all(action="DESELECT")
    for o in lows:
        o.select_set(True)
    bpy.context.view_layer.objects.active = lows[0]
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(50), island_margin=0.0, area_weight=0.0, correct_aspect=True, scale_to_bounds=False)
    for o in lows:
        bm = bmesh.from_edit_mesh(o.data)
        bm.normal_update()
        uv = bm.loops.layers.uv.verify()
        for island in bmesh_linked_uv_islands(bm, uv):
            area = sum(f.calc_area() for f in island) or 1.0
            n = sum((f.normal * f.calc_area() for f in island), Vector()) / area
            scale = 1.0
            if n.z < -0.6:
                scale = 0.4
            elif n.z > 0.7:
                scale = 1.3  # the camera is above most of what it looks at
            elif n.y < -0.7 and o.name not in ("sofa", "plastic_chair", "stool", "table", "fan", "water_drum", "sacks", "charpai"):
                scale = 0.5
            if scale != 1.0:
                pts = [loop[uv].uv.copy() for f in island for loop in f.loops]
                c = sum(pts, Vector((0, 0))) / len(pts)
                for f in island:
                    for loop in f.loops:
                        loop[uv].uv = c + (loop[uv].uv - c) * scale
        bmesh.update_edit_mesh(o.data)
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(udim_source="CLOSEST_UDIM", rotate=True, scale=True, margin_method="FRACTION", margin=GAP, shape_method="CONCAVE")
    bpy.ops.object.mode_set(mode="OBJECT")
    # What the atlas came to. Most faces kept full size, so the median is the density that shows.
    world, flat, density = 0.0, 0.0, []
    for o in lows:
        me = o.data
        for poly in me.polygons:
            pts = [me.uv_layers["UVMap"].uv[i].vector for i in poly.loop_indices]
            area = 0.5 * abs((pts[1] - pts[0]).cross(pts[2] - pts[0]))
            world += poly.area
            flat += area
            density.append(math.sqrt(area / max(poly.area, 1e-9)) * 2048)
    print(f"[room_kit] atlas: {world:.1f} m2 in {flat * 100:.0f}% of the square, {np.median(density):.0f} texels a metre at 2048")


def layout(i, name):
    """Where a piece stands for the bake: apart, so nothing shades its neighbour. The fan hangs."""
    x = (i % 4) * 7.0
    y = (i // 4) * 7.0
    return Vector((x, y, 3.2 if name == "fan" else 0.0))


def blank(name, colour, non_color, res=RES):
    img = bpy.data.images.new(name, res, res, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    px = np.empty((res, res, 4), dtype=np.float32)
    px[...] = (*colour, 1.0)
    img.pixels.foreach_set(px.ravel())
    return img


def bake_pass(lo, hi, img, kind, channel=None, samples=8, cage=CAGE):
    scene = bpy.context.scene
    scene.cycles.samples = samples
    bpy.ops.object.select_all(action="DESELECT")
    hi.select_set(True)
    lo.select_set(True)
    bpy.context.view_layer.objects.active = lo
    targets = bake._target_nodes(lo, img)
    undo = bake._emission_rewire(hi, channel) if kind == "EMIT" else []
    kw = {"normal_space": "TANGENT"} if kind == "NORMAL" else {}
    bpy.ops.object.bake(type=kind, use_selected_to_active=True, cage_extrusion=cage, max_ray_distance=cage * 3 + 0.02, margin=MARGIN, margin_type="EXTEND",
                        use_clear=False, target="IMAGE_TEXTURES", **kw)
    for nt, em, bsdf, out in undo:
        nt.nodes.remove(em)
        nt.links.new(bsdf.outputs["BSDF"], out.inputs["Surface"])
    for nt, n in targets:
        nt.nodes.remove(n)


def bake_atlas(items):
    """Bakes every group of pieces with its own cage. Returns the four maps at RES."""
    scene = bpy.context.scene
    bake.setup_cycles(8)
    scene.cycles.device = "CPU"
    scene.render.threads_mode = "FIXED"
    scene.render.threads = 8
    scene.cycles.use_denoising = False
    world = bpy.data.worlds.new("bake")
    scene.world = world
    world.light_settings.distance = 0.5

    floor_me = bpy.data.meshes.new("floor")
    floor_me.from_pydata([(-10, -10, 0), (40, -10, 0), (40, 40, 0), (-10, 40, 0)], [], [(0, 1, 2, 3)])
    floor = bpy.data.objects.new("floor", floor_me)
    scene.collection.objects.link(floor)
    ceiling_me = bpy.data.meshes.new("ceiling")
    ceiling_me.from_pydata([(-10, -10, 3.2), (-10, 40, 3.2), (40, 40, 3.2), (40, -10, 3.2)], [], [(0, 1, 2, 3)])
    ceiling = bpy.data.objects.new("ceiling", ceiling_me)
    scene.collection.objects.link(ceiling)

    target = bpy.data.materials.new("target")
    target.use_nodes = True
    sets = {}
    for lo, hi, cage in items:
        sets.setdefault(cage, []).append((lo, hi))
    images = {
        "base": blank("kit_base", (0.20, 0.17, 0.15), False),
        "rough": blank("kit_rough", (0.6, 0.6, 0.6), True),
        "metal": blank("kit_metal", (0.0, 0.0, 0.0), True),
        "normal": blank("kit_normal", (0.5, 0.5, 1.0), True),
        "ao": blank("kit_ao", (1.0, 1.0, 1.0), True),
    }
    for cage, group in sets.items():
        lows, highs = [], []
        for lo, hi in group:
            k = next(i for i, (a, _, _) in enumerate(items) if a is lo)
            at = Matrix.Translation(layout(k, lo.name))
            lcopy = lo.copy()
            lcopy.data = lo.data.copy()
            scene.collection.objects.link(lcopy)
            lcopy.matrix_world = at
            lows.append(lcopy)
            hi.matrix_world = at
            highs.append(hi)
        bpy.context.view_layer.update()
        lo_all = geo.join(lows, "bake_lo") if len(lows) > 1 else lows[0]
        hi_all = geo.join(highs, "bake_hi") if len(highs) > 1 else highs[0]
        for s in list(lo_all.data.materials):
            lo_all.data.materials.pop()
        lo_all.data.materials.append(target)
        passes = (("EMIT", "base", "Base Color", 16), ("EMIT", "rough", "Roughness", 8), ("EMIT", "metal", "Metallic", 1), ("NORMAL", "normal", None, 16), ("AO", "ao", None, 96))
        for pass_kind, img, channel, samples in passes:
            t0 = time.time()
            bake_pass(lo_all, hi_all, images[img], pass_kind, channel, samples, cage)
            print(f"[room_kit] bake {img} cage {cage}: {time.time() - t0:.1f}s")
        bpy.data.objects.remove(lo_all)
        bpy.data.objects.remove(hi_all)
    bpy.data.objects.remove(floor)
    bpy.data.objects.remove(ceiling)
    return images


def down(a, size):
    """Box filter to `size`."""
    k = a.shape[0] // size
    if k <= 1:
        return a
    return a[: size * k, : size * k].reshape(size, k, size, k, -1).mean(axis=(1, 3))


def finish_maps(images, data_res=1024):
    """What ships: colour as baked, the rest at `data_res`. The occlusion also goes into the colour a
    little, as direct light would otherwise wash the contact shadows out."""
    px = {k: bake.pixels(v) for k, v in images.items()}
    ao = px["ao"][..., 0]
    soft = 0.25 + 0.75 * ao
    base = images["base"]
    srgb = px["base"][..., :3]
    shade = (0.45 + 0.55 * ao)[..., None]
    out = np.ones_like(px["base"])
    out[..., :3] = srgb * shade ** (1 / 2.2)
    base.pixels.foreach_set(out.ravel())

    def save(img, name):
        bake.save(img, os.path.join(WORK, name + ".png"))
        return img

    # A ray that started inside a neighbour reports its far side: flat is better than backwards.
    n = px["normal"][..., :3] * 2 - 1
    n[n[..., 2] < 0.2] = (0, 0, 1)
    px["normal"][..., :3] = n * 0.5 + 0.5

    def resized(name, a, res):
        small = down(a, res)
        if name == "normal":
            n = small[..., :3] * 2 - 1
            n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-5)
            small[..., :3] = n * 0.5 + 0.5
        img = bpy.data.images.new(f"room_kit_{name}", res, res, alpha=False, float_buffer=False)
        img.colorspace_settings.name = "Non-Color"
        img.pixels.foreach_set(np.dstack((small[..., :3], np.ones((res, res)))).astype(np.float32).ravel())
        return img

    orm = np.ones_like(px["base"])
    orm[..., 0] = soft
    orm[..., 1] = px["rough"][..., 0]
    orm[..., 2] = px["metal"][..., 0]
    base.name = "room_kit_basecolor"
    return {
        "basecolor": save(base, "room_kit_basecolor"),
        "orm": save(resized("orm", orm, min(data_res, RES)), "room_kit_orm"),
        "normal": save(resized("normal", px["normal"], min(data_res, RES)), "room_kit_normal"),
    }


def sheet(cell=640, columns=5):
    """The labelled sheet, from the exported file: each piece framed, with its name, triangles and
    size, in previews/room-kit/. `blender ... -- sheet` redoes it without baking."""
    sys.path.insert(0, os.path.join(HERE, ".."))
    from render_preview import setup_world

    export.reset_scene()
    bpy.ops.import_scene.gltf(filepath=OUT)
    scene = bpy.context.scene
    pieces = sorted((o for o in scene.objects if o.type == "MESH"), key=lambda o: list(KIT).index(o.name))
    bake.setup_cycles(48 if RES >= 2048 else 16)
    scene.cycles.device = "CPU"
    scene.render.threads_mode = "FIXED"
    scene.render.threads = 8
    scene.cycles.use_denoising = True
    scene.view_settings.view_transform = "AgX"
    scene.render.resolution_x = scene.render.resolution_y = cell
    setup_world()
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 2.5
    sun.angle = 0.1
    so = bpy.data.objects.new("sun", sun)
    so.rotation_euler = (0.9, 0.2, 2.4)
    scene.collection.objects.link(so)

    ground = bpy.data.meshes.new("ground")
    ground.from_pydata([(-20, -20, 0), (20, -20, 0), (20, 20, 0), (-20, 20, 0)], [], [(0, 1, 2, 3)])
    gm = bpy.data.materials.new("ground")
    gm.use_nodes = True
    gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.16, 0.15, 0.14, 1)
    gm.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.7
    ground.materials.append(gm)
    floor = bpy.data.objects.new("ground", ground)
    scene.collection.objects.link(floor)

    cam = bpy.data.cameras.new("cam")
    cam.lens = 60
    cam.clip_start = 0.02
    co = bpy.data.objects.new("cam", cam)
    scene.collection.objects.link(co)
    scene.camera = co

    white = bpy.data.materials.new("label")
    white.use_nodes = True
    nt = white.node_tree
    nt.nodes.clear()
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Strength"].default_value = 1.2
    nt.links.new(em.outputs[0], nt.nodes.new("ShaderNodeOutputMaterial").inputs["Surface"])
    black = white.copy()
    black.node_tree.nodes["Emission"].inputs["Strength"].default_value = 0.0

    near = 0.2
    half = near * 18 / cam.lens
    bar_me = bpy.data.meshes.new("bar")
    bar_me.from_pydata([(-half, -half, 0), (half, -half, 0), (half, -half * 0.74, 0), (-half, -half * 0.74, 0)], [], [(0, 1, 2, 3)])
    bar_me.materials.append(black)
    bar = bpy.data.objects.new("bar", bar_me)
    bar.parent = co
    bar.location = (0, 0, -near - 0.0005)
    scene.collection.objects.link(bar)

    def text(body, x, y, size):
        c = bpy.data.curves.new("label", "FONT")
        c.body = body
        c.size = size
        c.materials.append(white)
        o = bpy.data.objects.new("label", c)
        o.parent = co
        o.location = (x, y, -near)
        scene.collection.objects.link(o)
        return o

    cells = []
    os.makedirs(os.path.join(WORK, "cells"), exist_ok=True)
    tris = {o.name: geo.tri_count(o) for o in pieces}
    for o in pieces:
        for other in pieces:
            other.hide_render = other is not o
        bpy.context.view_layer.update()
        pts = [o.matrix_world @ Vector(c) for c in o.bound_box]
        lo = Vector([min(q[k] for q in pts) for k in range(3)])
        hi = Vector([max(q[k] for q in pts) for k in range(3)])
        mid, size = (lo + hi) / 2, (hi - lo).length
        d = size * 2.0
        hangs = o.name == "fan"
        floor.hide_render = hangs
        view = Vector((0.55, 0.80, -0.22 if hangs else 0.38)).normalized()
        co.location = mid + view * (d * 0.7 if hangs else d)
        co.rotation_euler = (mid + Vector((0, 0, -size * 0.04)) - co.location).to_track_quat("-Z", "Y").to_euler()
        w, de, h = hi - lo
        a = text(o.name, -half * 0.94, -half * 0.83, half * 0.082)
        b = text(f"{tris[o.name]} tris   {w:.2f} x {de:.2f} x {h:.2f} m", -half * 0.94, -half * 0.94, half * 0.056)
        scene.render.filepath = os.path.join(WORK, "cells", o.name + ".png")
        bpy.ops.render.render(write_still=True)
        cells.append(scene.render.filepath)
        for t in (a, b):
            curve = t.data
            bpy.data.objects.remove(t)
            bpy.data.curves.remove(curve)
    rows = (len(cells) + columns - 1) // columns
    px = np.full((rows * cell, columns * cell, 4), 0.1, dtype=np.float32)
    px[..., 3] = 1
    for i, path in enumerate(cells):
        img = bpy.data.images.load(path)
        y = (rows - 1 - i // columns) * cell
        x = (i % columns) * cell
        px[y : y + cell, x : x + cell] = bake.pixels(img)
        bpy.data.images.remove(img)
    out = bpy.data.images.new("sheet", columns * cell, rows * cell, alpha=False)
    out.pixels.foreach_set(px.ravel())
    bake.save(out, os.path.join(PREVIEWS, "sheet.png"))
    print(f"[room_kit] sheet: {os.path.join(PREVIEWS, 'sheet.png')}")


def main():
    fetch()
    os.makedirs(WORK, exist_ok=True)
    os.makedirs(PREVIEWS, exist_ok=True)
    export.reset_scene()
    surfaces()
    t0 = time.time()
    items = [b().finish() for b in BUILDERS if not ONLY or b.__name__ in ONLY]
    print(f"[room_kit] built in {time.time() - t0:.1f}s")
    for lo, _, _ in items:
        n = geo.tri_count(lo)
        print(f"[room_kit] {lo.name}: {n} tris")
    lows = [lo for lo, _, _ in items]
    unwrap(lows)
    images = bake_atlas(items)
    maps_ = finish_maps(images)
    atlas = export.game_material("room_kit", maps_)
    atlas.use_backface_culling = True  # every side that shows is modelled; open ends hide in joints
    for o in lows:
        o.data.materials.append(atlas)
    tris = export.export_glb(lows, OUT)
    over = {k: v for k, v in tris.items() if v >= MAX_TRIS}
    print(f"[room_kit] {OUT}: {sum(tris.values())} tris, {tris}")
    if over:
        print(f"[room_kit] OVER BUDGET: {over}")
    sheet()


if "sheet" in sys.argv:
    sheet()
else:
    main()

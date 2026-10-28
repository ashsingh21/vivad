"""Indian trees: neem, peepal, banyan, coconut palm. Branch tubes + alpha leaf cards, 2 materials
(bark, leaves), LOD0/LOD1, exported as GLB with the trunk base at the origin.

blender -b --factory-startup -P tools/blender/nature/trees.py -- [species ...]
"""

import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Quaternion, Vector  # noqa: E402

import export  # noqa: E402
import geo  # noqa: E402
import textures  # noqa: E402

TEX_DIR = os.path.join(ROOT, "build", "textures", "trees")


# ------------------------------------------------------------------ materials

def bark_material(name, tint):
    path = os.path.join(TEX_DIR, f"bark_{name}.png")
    textures.tileable_bark(path, tint=tint, seed=hash(name) % 97)
    m = bpy.data.materials.new(f"bark_{name}")
    m.use_nodes = True
    nt = m.node_tree
    p = nt.nodes["Principled BSDF"]
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = bpy.data.images.load(path)
    nt.links.new(tex.outputs["Color"], p.inputs["Base Color"])
    ntex = nt.nodes.new("ShaderNodeTexImage")
    ntex.image = bpy.data.images.load(path.replace(".png", "_normal.png"))
    ntex.image.colorspace_settings.name = "Non-Color"
    nm = nt.nodes.new("ShaderNodeNormalMap")
    nt.links.new(ntex.outputs["Color"], nm.inputs["Color"])
    nt.links.new(nm.outputs["Normal"], p.inputs["Normal"])
    p.inputs["Roughness"].default_value = 0.9
    return m


def leaf_material(atlas_path):
    m = bpy.data.materials.new("leaves")
    m.use_nodes = True
    m.use_backface_culling = False
    nt = m.node_tree
    p = nt.nodes["Principled BSDF"]
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = bpy.data.images.load(atlas_path)
    nt.links.new(tex.outputs["Color"], p.inputs["Base Color"])
    cut = nt.nodes.new("ShaderNodeMath")
    cut.operation = "GREATER_THAN"
    cut.inputs[1].default_value = 0.5
    nt.links.new(tex.outputs["Alpha"], cut.inputs[0])
    nt.links.new(cut.outputs[0], p.inputs["Alpha"])
    p.inputs["Roughness"].default_value = 0.6
    return m


# ------------------------------------------------------------------ branches

class Tree:
    def __init__(self, seed, lod):
        self.rng = random.Random(seed)
        self.lod = lod
        self.bark_parts = []
        self.tips = []            # (position, direction, size)
        self.ring_segs = 10 if lod == 0 else 6

    def branch(self, start, direction, length, r0, r1, *, bend=0.0, gravity=0.0, wobble=0.15, segments=10, flare=0.0):
        """Curved tube; returns the sampled centreline."""
        d = direction.normalized()
        pts = [start.copy()]
        p = start.copy()
        step = length / segments
        for i in range(segments):
            jitter = Vector((self.rng.uniform(-1, 1), self.rng.uniform(-1, 1), self.rng.uniform(-0.3, 0.3))) * wobble
            d = (d + jitter * 0.3 + Vector((0, 0, -gravity)) * 0.2 + Vector((0, 0, bend)) * 0.1).normalized()
            p = p + d * step
            pts.append(p.copy())
        pts = geo.resample_even(geo.catmull_rom(pts, segments * 4), max(3, segments if self.lod == 0 else segments // 2 + 1))
        up = Vector((0, 0, 1)) if abs(direction.normalized().z) < 0.95 else Vector((1, 0, 0))
        frames = geo.path_frames(pts, up)
        rings = []
        n = len(pts)
        for i in range(n):
            t = i / (n - 1)
            r = r0 + (r1 - r0) * t
            if flare and t < 0.12:
                # Buttressed base: lobed, wider.
                k = (0.12 - t) / 0.12
                rings.append([(x * (1 + flare * k * (0.6 + 0.4 * math.cos(5 * math.atan2(y, x)))),
                               y * (1 + flare * k * (0.6 + 0.4 * math.cos(5 * math.atan2(y, x)))))
                              for x, y in geo.ellipse(r, r, self.ring_segs)])
            else:
                rings.append(geo.ellipse(r, r * 0.95, self.ring_segs))
        obj = geo.loft("branch", frames, rings, cap_end="fan")
        self.bark_parts.append(obj)
        return pts

    def leaves(self, species, tip, direction, size, count):
        self.tips.append((tip, direction, size, count, species))


def leaf_cards(tree, name, canopy_center):
    """Build all leaf cards into one mesh. Normals point away from the canopy centre (soft, volumetric
    lighting instead of flat card shading)."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    rng = tree.rng
    for tip, d, size, count, species in tree.tips:
        u0, v0, u1, v1 = textures.quadrant_uv(species)
        n = count if tree.lod == 0 else max(1, count // 2)
        s_mul = 1.0 if tree.lod == 0 else 1.35
        for _ in range(n):
            s = size * s_mul * rng.uniform(0.75, 1.15)
            c = tip + Vector((rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-0.6, 0.8))) * size * 0.6
            q = Quaternion(Vector((rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-1, 1))).normalized(), rng.uniform(0, math.pi))
            ax, ay = q @ Vector((s / 2, 0, 0)), q @ Vector((0, s / 2, 0))
            vs = [bm.verts.new(c - ax - ay), bm.verts.new(c + ax - ay), bm.verts.new(c + ax + ay), bm.verts.new(c - ax + ay)]
            f = bm.faces.new(vs)
            for loop, (uu, vv) in zip(f.loops, ((u0, v0), (u1, v0), (u1, v1), (u0, v1))):
                loop[uv].uv = (uu, vv)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    for p in me.polygons:
        p.use_smooth = True
    # Spherical normals from the canopy centre.
    normals = []
    for loop in me.loops:
        v = me.vertices[loop.vertex_index].co
        normals.append(((v - canopy_center) * Vector((1, 1, 1.4))).normalized())
    me.normals_split_custom_set(normals)
    return obj


def palm_fronds(tree, crown, count, length, droop):
    """Arching fronds as V-folded strips textured with the palm leaflet tile."""
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    u0, v0, u1, v1 = textures.quadrant_uv("palm")
    rng = tree.rng
    seg = 12 if tree.lod == 0 else 6
    for i in range(count):
        az = 2 * math.pi * i / count + rng.uniform(-0.15, 0.15)
        elev = rng.uniform(0.55, 1.15)
        L = length * rng.uniform(0.85, 1.1)
        horiz = Vector((math.cos(az), math.sin(az), 0))
        side = Vector((-math.sin(az), math.cos(az), 0))
        rows = []
        for k in range(seg + 1):
            t = k / seg
            # Arch: rise, then droop under its own weight.
            p = crown + horiz * (L * t) + Vector((0, 0, L * (math.sin(elev) * t - droop * t * t)))
            w = 0.55 * L * 0.18 * (math.sin(math.pi * min(t * 1.2, 1.0)) * 0.85 + 0.15)
            fold = Vector((0, 0, -w * 0.35))
            rows.append([bm.verts.new(p - side * w + fold), bm.verts.new(p), bm.verts.new(p + side * w + fold)])
        for k in range(seg):
            for j in range(2):
                f = bm.faces.new((rows[k][j], rows[k][j + 1], rows[k + 1][j + 1], rows[k + 1][j]))
                for loop in f.loops:
                    kk = next(ki for ki in (k, k + 1) if loop.vert in rows[ki])
                    jj = rows[kk].index(loop.vert)
                    loop[uv].uv = (u0 + (u1 - u0) * jj / 2, v0 + (v1 - v0) * kk / seg)
    me = bpy.data.meshes.new("fronds")
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new("fronds", me)
    bpy.context.scene.collection.objects.link(obj)
    for p in me.polygons:
        p.use_smooth = True
    return obj


# ------------------------------------------------------------------ species

def grow_neem(tree, species="neem", scale=1.0):
    """Short trunk, several upright scaffold limbs, dense rounded crown."""
    rng = tree.rng
    s = scale
    trunk = tree.branch(Vector((0, 0, 0)), Vector((0.05, 0.02, 1)), 2.6 * s, 0.30 * s, 0.22 * s, flare=0.5, segments=8)
    top = trunk[-1]
    limbs = 5
    for i in range(limbs):
        az = 2 * math.pi * i / limbs + rng.uniform(-0.3, 0.3)
        d = Vector((math.cos(az) * 0.7, math.sin(az) * 0.7, 1.0))
        pts = tree.branch(top - Vector((0, 0, 0.3 * i / limbs)), d, rng.uniform(2.6, 3.4) * s, 0.16 * s, 0.07 * s, gravity=0.15, segments=8)
        for j in range(6):
            base = pts[max(2, int(len(pts) * (0.35 + 0.65 * j / 5)) - 1)]
            az2 = az + rng.uniform(-1.4, 1.4)
            d2 = Vector((math.cos(az2), math.sin(az2), rng.uniform(0.2, 0.9)))
            sub = tree.branch(base, d2, rng.uniform(1.3, 2.3) * s, 0.06 * s, 0.022 * s, gravity=0.25, segments=6)
            for p in sub[1:]:
                tree.leaves(species, p, d2, 1.7 * s, 4)
        for p in pts[len(pts) // 2:]:
            tree.leaves(species, p, d, 1.9 * s, 3)
    return Vector((0, 0, top.z + 2.4 * s))


def grow_banyan(tree, scale=1.0):
    """Massive buttressed trunk, long near-horizontal limbs dropping aerial prop roots."""
    rng = tree.rng
    s = scale
    trunk = tree.branch(Vector((0, 0, 0)), Vector((0, 0, 1)), 5.5 * s, 1.5 * s, 0.85 * s, flare=0.9, segments=10, wobble=0.05)
    top = trunk[-1]
    limbs = 8
    for i in range(limbs):
        az = 2 * math.pi * i / limbs + rng.uniform(-0.25, 0.25)
        d = Vector((math.cos(az), math.sin(az), 0.22))
        start = top - Vector((0, 0, rng.uniform(0.5, 2.2) * s))
        pts = tree.branch(start, d, rng.uniform(11, 15) * s, 0.5 * s, 0.14 * s, gravity=0.06, segments=14, wobble=0.08)
        # Prop roots: from points along the limb straight down to the ground.
        for k in range(2, len(pts) - 1):
            p = pts[k]
            if rng.random() < 0.6:
                thick = rng.uniform(0.04, 0.18) * s * (1.6 if k < len(pts) // 2 else 1.0)
                # Young roots hang thin and kinked; old ones have thickened into pillars.
                tree.branch(p, Vector((rng.uniform(-0.08, 0.08), rng.uniform(-0.08, 0.08), -1)), p.z + 0.3, thick * 0.45, thick * 1.2,
                            segments=6, wobble=0.12)
                if rng.random() < 0.5:
                    tree.branch(p + Vector((0.15, 0.1, 0)), Vector((0.1, -0.05, -1)), p.z * rng.uniform(0.4, 0.8), 0.02 * s, 0.015 * s,
                                segments=4, wobble=0.25)
        # Secondary branches up into the canopy.
        for k in range(1, len(pts)):
            az2 = az + rng.uniform(-1.5, 1.5)
            d2 = Vector((math.cos(az2), math.sin(az2), rng.uniform(0.25, 0.9)))
            sub = tree.branch(pts[k], d2, rng.uniform(2.5, 4.5) * s, 0.10 * s, 0.03 * s, gravity=0.15, segments=6)
            for p in sub[1:]:
                tree.leaves("banyan", p, d2, 2.3 * s, 5)
            tree.leaves("banyan", pts[k] + Vector((0, 0, 1.2 * s)), d2, 2.6 * s, 4)
    return Vector((0, 0, top.z + 2.5 * s))


def grow_palm(tree, scale=1.0):
    rng = tree.rng
    s = scale
    lean = Vector((rng.uniform(-0.25, 0.25), rng.uniform(-0.25, 0.25), 1.0))
    # Slender trunk, slightly swollen base, ringed (leaf-scar) radius modulation.
    h = rng.uniform(9.0, 11.5) * s
    seg = 22 if tree.lod == 0 else 10
    pts = [Vector((0, 0, 0))]
    d = lean.normalized()
    for i in range(seg):
        d = (d + Vector((0, 0, 0.06))).normalized()
        pts.append(pts[-1] + d * (h / seg))
    pts = geo.resample_even(geo.catmull_rom(pts, seg * 3), seg + 1)
    frames = geo.path_frames(pts, Vector((1, 0, 0)))
    rings = []
    for i in range(len(pts)):
        t = i / (len(pts) - 1)
        r = (0.21 - 0.07 * t + 0.1 * max(0.0, 0.08 - t) / 0.08) * s
        r *= 1 + (0.06 * (i % 2) if tree.lod == 0 else 0)
        rings.append(geo.ellipse(r, r, tree.ring_segs))
    tree.bark_parts.append(geo.loft("trunk", frames, rings, cap_end="fan"))
    crown = pts[-1]
    # Coconuts in a cluster under the crown.
    for k in range(6):
        a = 2 * math.pi * k / 6
        c = crown + Vector((math.cos(a) * 0.25, math.sin(a) * 0.25, -0.35)) * s
        bpy.ops.mesh.primitive_uv_sphere_add(segments=10, ring_count=6, radius=0.13 * s, location=c)
        tree.bark_parts.append(bpy.context.active_object)
    return crown, palm_fronds(tree, crown, 18 if tree.lod == 0 else 10, 4.6 * s, 0.62)


def grow_bush(tree, species, *, radius=1.1, height=1.4, stems=5, cards=90, card_size=0.75):
    """Multi-stemmed shrub: woody canes from the base, leaf cards filling an ellipsoid."""
    rng = tree.rng
    for i in range(stems):
        az = 2 * math.pi * i / stems + rng.uniform(-0.4, 0.4)
        d = Vector((math.cos(az) * 0.5, math.sin(az) * 0.5, 1.0))
        tree.branch(Vector((math.cos(az) * 0.08, math.sin(az) * 0.08, 0)), d, height * rng.uniform(0.8, 1.1), 0.035, 0.012,
                    gravity=0.3, segments=5, wobble=0.25)
    center = Vector((0, 0, height * 0.55))
    for _ in range(cards):
        # Uniform in an ellipsoid, biased to the surface where light reaches.
        while True:
            p = Vector((rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-1, 1)))
            if 0.35 < p.length <= 1.0:
                break
        pos = center + Vector((p.x * radius, p.y * radius, p.z * height * 0.5))
        if pos.z < 0.12:
            pos.z = 0.12 + rng.uniform(0, 0.2)
        tree.leaves(species, pos, p, card_size, 1)
    return center


def grow_bougainvillea(tree):
    """Arching canes spilling magenta bracts: taller and looser than a shrub."""
    rng = tree.rng
    for i in range(7):
        az = 2 * math.pi * i / 7 + rng.uniform(-0.3, 0.3)
        d = Vector((math.cos(az) * 0.6, math.sin(az) * 0.6, 1.0))
        pts = tree.branch(Vector((0, 0, 0)), d, rng.uniform(2.2, 3.0), 0.04, 0.012, gravity=0.55, segments=8, wobble=0.2)
        for p in pts[2:]:
            tree.leaves("bougainvillea", p, d, 0.9, 3)
    return Vector((0, 0, 1.4))


SPECIES = {
    "neem": dict(bark=(0.30, 0.26, 0.22), seed=11),
    "peepal": dict(bark=(0.48, 0.44, 0.38), seed=23),
    "banyan": dict(bark=(0.42, 0.38, 0.33), seed=37),
    "palm": dict(bark=(0.46, 0.40, 0.33), seed=41),
    "lantana": dict(bark=(0.35, 0.30, 0.24), seed=53),
    "bougainvillea": dict(bark=(0.38, 0.32, 0.26), seed=61),
}


def build(species, lod, atlas):
    cfg = SPECIES[species]
    tree = Tree(cfg["seed"], lod)
    fronds = None
    if species == "neem":
        crown = grow_neem(tree)
    elif species == "peepal":
        crown = grow_neem(tree, "peepal", scale=1.25)
    elif species == "banyan":
        crown = grow_banyan(tree)
    elif species == "lantana":
        crown = grow_bush(tree, "lantana")
    elif species == "bougainvillea":
        crown = grow_bougainvillea(tree)
    else:
        crown, fronds = grow_palm(tree)
    bark = geo.join(tree.bark_parts, f"{species}_bark_LOD{lod}")
    bark.data.materials.append(bpy.data.materials.get(f"bark_{species}") or bark_material(species, cfg["bark"]))
    for p in bark.data.polygons:
        p.use_smooth = True
    parts = [bark]
    if tree.tips:
        parts.append(leaf_cards(tree, f"{species}_leaves_LOD{lod}", crown))
    if fronds is not None:
        parts.append(fronds)
    leafmat = bpy.data.materials.get("leaves") or leaf_material(atlas)
    for p in parts[1:]:
        p.data.materials.append(leafmat)
    obj = geo.join(parts, f"{species}_LOD{lod}")
    return obj


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    wanted = argv or list(SPECIES)
    atlas = os.path.join(TEX_DIR, "leaf_atlas.png")
    for species in wanted:
        export.reset_scene()
        textures.draw_atlas(atlas)
        lods = [build(species, lod, atlas) for lod in (0, 1)]
        prefix = "bush" if species in ("lantana", "bougainvillea") else "tree"
        out = os.path.join(ROOT, "assets", "models", "props", f"{prefix}_{species}.glb")
        counts = export.export_glb(lods, out)
        print(f"[tree {species}] {counts}")


if __name__ == "__main__":
    main()

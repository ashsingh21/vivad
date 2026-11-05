"""The style guide's trees and bushes (docs/style-guide.md): a chunky trunk and a crown of a few big
lumpy leaf masses, opaque, shaded by vertex colour from the palette (light on top, dark underneath
and inside, a soft variation across), their normals bent out from the crown's centre so it lights
as one soft volume. No alpha cards: cheaper to draw than the old trees (no alpha test, far fewer
triangles at a distance) and it reads at a sprint. Delhi's maps use these
(shared::map::delhi); the ancient map keeps tools/blender/nature/trees.py's until it's restyled.

blender -b --factory-startup -P tools/blender/nature/stylised_trees.py -- [species ...]
writes assets/models/props/stylised/{tree,bush}_<species>.glb, LOD0..LOD2 (bushes LOD0..LOD1).
"""

import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", ".."))

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import export  # noqa: E402
import geo  # noqa: E402
import style  # noqa: E402
from trees import Tree  # noqa: E402

OUT = os.path.join(ROOT, "assets", "models", "props", "stylised")


def mix(a, b, t):
    t = min(max(t, 0.0), 1.0)
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def noise(p, seed):
    """Smooth, slow lumps: a few crossed sines (deterministic, no texture)."""
    s = seed * 1.37
    return (math.sin(p.x * 1.3 + s) * math.sin(p.y * 1.1 - s * 0.7) + math.sin(p.z * 1.7 + p.x * 0.6 + s * 1.9)) * 0.5


class Crown:
    """Leaf masses: (centre, radii) ellipsoids, built into one mesh."""

    def __init__(self, centre, radii, palette, seed):
        self.centre, self.radii, self.palette, self.seed = centre, radii, palette, seed
        self.blobs = []
        self.tufts = []  # (position, outward normal, size): leaf clumps over the surface
        self.dots = []  # (position, radius, colour token)
        self.trunk_top = 0.0  # bark below this is ridged

    def leafy(self, rng, count, size):
        """Thick pointed leaf clumps over the masses' exposed surface: a leafy outline, not a ball."""
        weights = [r.x * r.y for _, r in self.blobs]
        tries = 0
        while len(self.tufts) < count and tries < count * 30:
            tries += 1
            c, r = rng.choices(self.blobs, weights)[0]
            d = Vector((rng.gauss(0, 1), rng.gauss(0, 1), rng.gauss(0, 1) * 0.8 + 0.25)).normalized()
            p = c + Vector((r.x * d.x, r.y * d.y, r.z * d.z))
            # Exposed: inside no other mass.
            if any(o is not c and (Vector(((p.x - o.x) / q.x, (p.y - o.y) / q.y, (p.z - o.z) / q.z)).length < 0.97) for o, q in self.blobs):
                continue
            n = Vector((d.x / r.x, d.y / r.y, d.z / r.z)).normalized()
            self.tufts.append((p, n, size * rng.uniform(0.8, 1.2)))

    def blob(self, c, r):
        self.blobs.append((c, r))

    def build(self, name, subdiv, lump):
        bm = bmesh.new()
        for k, (c, r) in enumerate(self.blobs):
            ret = bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=1.0, matrix=Matrix.Identity(4))
            for v in ret["verts"]:
                d = v.co.normalized()
                rr = Vector((r.x * d.x, r.y * d.y, r.z * d.z))
                # Lumpy: pushed out and in along the normal, a tenth of the blob's size.
                v.co = c + rr * (1.0 + lump * noise(c + rr * 1.6, self.seed + k))
        for p, n, sz in self.tufts:
            # A fat leaf: a squashed blob lying along the surface, its tip pulled out and drooping.
            t = (Vector((0, 0, -1)) - n * -n.z).normalized() if abs(n.z) < 0.98 else Vector((1, 0, 0))
            b = n.cross(t)
            m = Matrix((t, b, n)).transposed().to_4x4()
            m.translation = p + n * sz * 0.12
            ret = bmesh.ops.create_icosphere(bm, subdivisions=1, radius=1.0, matrix=Matrix.Identity(4))
            for v in ret["verts"]:
                x, y, z = v.co
                tip = max(x, 0.0) ** 2 * 0.35
                v.co = m @ Vector((x * sz * (1.0 + tip), y * sz * 0.8, z * sz * 0.22 + tip * sz * 0.12))
        for p, rad, _ in self.dots:
            ret = bmesh.ops.create_icosphere(bm, subdivisions=2 if rad > 0.2 else 1, radius=rad, matrix=Matrix.Translation(p))
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me)
        bm.free()
        obj = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(obj)
        for poly in me.polygons:
            poly.use_smooth = True
        return obj


def ridge(p):
    """The bark's ridges, 0 in a groove to 1 on a ridge: vertical, a little twisted."""
    # Seven broad plates with narrow grooves between (squared off: smooth shading blurs a plain cosine).
    c = math.cos(7 * math.atan2(p.y, p.x) + 0.5 * p.z)
    return min(max(0.5 + 0.9 * c, 0.0), 1.0)


def colour_and_normals(obj, crown, bark_tok, toks):
    """Vertex colours (linear, per corner) and, on the leaves, normals bent out from the crown."""
    me = obj.data
    col = me.color_attributes.new("Col", "FLOAT_COLOR", "CORNER")
    me.color_attributes.active_color = col
    me.color_attributes.render_color_index = me.color_attributes.active_color_index
    dark, mid, light = (style.linear(t) for t in crown.palette)
    bark = style.linear(bark_tok)
    c, rad = crown.centre, crown.radii
    zlo, zhi = c.z - rad.z, c.z + rad.z
    normals = []
    corner = me.corner_normals
    kinds = me.attributes["kind"].data
    for poly in me.polygons:
        kind = kinds[poly.index].value
        for li in poly.loop_indices:
            vi = me.loops[li].vertex_index
            p = me.vertices[vi].co
            if kind == 0:
                # Bark: darker at the foot, a shade lighter up the limbs.
                k = 0.72 + 0.28 * min(p.z / max(c.z - rad.z, 0.5), 1.0)
                if p.z < crown.trunk_top:
                    k *= 0.55 + 0.45 * ridge(p)
                col.data[li].color = (bark[0] * k, bark[1] * k, bark[2] * k, 1.0)
                normals.append(corner[li].vector.copy())
                continue
            if kind >= 2:
                # A flower: its own flat colour, a little darker underneath.
                k = 0.85 + 0.15 * max(corner[li].vector.z, 0.0)
                fc = style.linear(toks[kind - 2])
                col.data[li].color = (fc[0] * k, fc[1] * k, fc[2] * k, 1.0)
                normals.append(corner[li].vector.copy())
                continue
            # Leaves: the sky's light from above, the crown's own shade inside and underneath.
            q = Vector(((p.x - c.x) / rad.x, (p.y - c.y) / rad.y, (p.z - c.z) / rad.z))
            up = (p.z - zlo) / (zhi - zlo)
            inner = min(q.length, 1.0)
            g = mix(dark, mid, up * 1.5)
            g = mix(g, light, (up - 0.6) * 2.2)
            g = mix(g, dark, (0.75 - inner) * 1.6)
            v = 0.96 + 0.08 * noise(p * 0.8, crown.seed + 50)
            col.data[li].color = (g[0] * v, g[1] * v, g[2] * v, 1.0)
            own = corner[li].vector
            out = q.normalized() if q.length > 1e-4 else Vector((0, 0, 1))
            normals.append((own * 0.4 + out * 0.6).normalized())
    me.normals_split_custom_set(normals)


def make(species, lod):
    cfg = SPECIES[species]
    rng = random.Random(cfg["seed"])
    tree = Tree(cfg["seed"], lod)
    # Enough sides round the trunk for its ridges.
    tree.ring_segs = (28, 14, 6)[lod]
    crown = cfg["grow"](tree, rng, lod)
    crown.leafy(rng, (cfg["tufts"], cfg["tufts"] * 2 // 5, 0)[lod], cfg["tuft"])
    # Bushes are small on screen: a level less.
    subdiv, lump = (((3, 0.12), (2, 0.08), (2, 0.0)) if cfg["kind"] == "tree" else ((2, 0.1), (2, 0.0)))[lod]
    if lod == 2:
        # The far distance: the crown's three biggest masses.
        crown.blobs = sorted(crown.blobs, key=lambda b: -b[1].x)[:3]
        crown.dots = []
    if lod > 0:
        crown.dots = crown.dots[::3]
    # Every face tagged with what it is (0 bark, 1 leaves, 2+k the k'th flower token): integer
    # attributes survive the join, unlike material slots. One material: the colour is per vertex.
    toks = sorted({d[2] for d in crown.dots})
    mat = material(f"{species}_stylised")
    parts = []
    if tree.bark_parts:
        bark = geo.join(tree.bark_parts, f"{species}_bark")
        if lod < 2:
            for v in bark.data.vertices:
                if v.co.z < crown.trunk_top:
                    k = 1.0 + 0.16 * (ridge(v.co) - 0.5)
                    v.co.x *= k
                    v.co.y *= k
        tag(bark, lambda k: 0)
        parts.append(bark)
    leaves = crown.build(f"{species}_leaves", subdiv, lump)
    blob_faces = len(crown.blobs) * 20 * 4 ** (subdiv - 1) + len(crown.tufts) * 20
    sizes = [80 if d[1] > 0.2 else 20 for d in crown.dots]
    starts = [blob_faces + sum(sizes[:k]) for k in range(len(sizes))]

    def kind(f):
        if f < blob_faces:
            return 1
        k = max(i for i, a in enumerate(starts) if a <= f)
        return 2 + toks.index(crown.dots[k][2])
    tag(leaves, kind)
    parts.append(leaves)
    obj = geo.join(parts, f"{species}_LOD{lod}") if len(parts) > 1 else parts[0]
    obj.name = f"{species}_LOD{lod}"
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    for p in obj.data.polygons:
        p.material_index = 0
        p.use_smooth = True
    colour_and_normals(obj, crown, cfg["bark"], toks)
    return obj


def tag(obj, kind):
    a = obj.data.attributes.new("kind", "INT", "FACE")
    for f in range(len(obj.data.polygons)):
        a.data[f].value = kind(f)


def material(name):
    m = bpy.data.materials.get(name)
    if m:
        return m
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    p = nt.nodes["Principled BSDF"]
    vc = nt.nodes.new("ShaderNodeVertexColor")
    vc.layer_name = "Col"
    nt.links.new(vc.outputs["Color"], p.inputs["Base Color"])
    p.inputs["Roughness"].default_value = 0.88
    return m


# ------------------------------------------------------------------ species

def limbs(tree, rng, top, targets, r0, r1):
    for t in targets:
        d = t - top
        tree.branch(top, d, d.length, r0, r1, segments=5, wobble=0.05)


def grow_neem(tree, rng, lod, s=1.0, ring=6, tokens=("foliage.neem_dark", "foliage.neem", "foliage.neem_light")):
    """Neem: a short stout trunk, limbs up into a round crown of seven or so masses."""
    # Thick, as the style guide's chunky: a fat flared trunk, stout limbs.
    tree.branch(Vector((0, 0, 0)), Vector((0.04, 0.02, 1)), 2.7 * s, 0.5 * s, 0.36 * s, flare=0.8, segments=6, wobble=0.04)
    top = Vector((0.0, 0.0, 2.5 * s))
    c = Vector((0, 0, 5.7 * s))
    crown = Crown(c, Vector((3.4, 3.4, 2.3)) * s, tokens, tree.rng.randrange(1000))
    crown.trunk_top = 2.4 * s
    crown.blob(c + Vector((0, 0, 0.4)) * s, Vector((2.1, 2.1, 1.8)) * s)
    targets = []
    for i in range(ring):
        a = 2 * math.pi * i / ring + rng.uniform(-0.25, 0.25)
        d = rng.uniform(1.7, 2.1) * s
        p = c + Vector((math.cos(a) * d, math.sin(a) * d, rng.uniform(-0.5, 0.2) * s))
        crown.blob(p, Vector((1.5, 1.5, 1.25)) * s * rng.uniform(0.9, 1.1))
        if i % 2 == 0:
            targets.append(p - Vector((0, 0, 0.4)) * s)
    for i in range(2):
        a = rng.uniform(0, 2 * math.pi)
        crown.blob(c + Vector((math.cos(a) * 0.9, math.sin(a) * 0.9, 1.5)) * s, Vector((1.3, 1.3, 1.1)) * s)
    # A fuller crown: a lower ring of masses between the main ones, hanging a little.
    for i in range(ring // 2):
        a = 2 * math.pi * (i + 0.5) / (ring // 2) + rng.uniform(-0.3, 0.3)
        crown.blob(c + Vector((math.cos(a) * 1.5, math.sin(a) * 1.5, -0.9)) * s, Vector((1.3, 1.3, 0.9)) * s)
    limbs(tree, rng, top, targets, 0.24 * s, 0.12 * s)
    return crown


def grow_peepal(tree, rng, lod):
    """Peepal: taller and broader than a neem, paler bark, a lighter, brighter green."""
    return grow_neem(tree, rng, lod, s=1.25, ring=7, tokens=("foliage.peepal_dark", "foliage.peepal", "foliage.peepal_light"))


def grow_bush(tree, rng, lod, tokens, flowers, blobs=5, radius=1.0, height=1.3, dots=26):
    c = Vector((0, 0, height * 0.5))
    crown = Crown(c, Vector((radius, radius, height * 0.55)), tokens, tree.rng.randrange(1000))
    crown.blob(c + Vector((0, 0, 0.1)), Vector((0.7, 0.7, 0.6)) * radius)
    for i in range(blobs - 1):
        a = 2 * math.pi * i / (blobs - 1) + rng.uniform(-0.3, 0.3)
        p = c + Vector((math.cos(a) * 0.45 * radius, math.sin(a) * 0.45 * radius, rng.uniform(-0.15, 0.2) * height))
        crown.blob(p, Vector((0.55, 0.55, 0.48)) * radius * rng.uniform(0.9, 1.1))
    for i in range(dots):
        # On the crown's upper surface, where the sun is.
        a = rng.uniform(0, 2 * math.pi)
        e = rng.uniform(0.1, 1.2)
        d = Vector((math.cos(a) * math.cos(e) * radius * 0.98, math.sin(a) * math.cos(e) * radius * 0.98, math.sin(e) * height * 0.52))
        crown.dots.append((c + d, 0.09 * radius, flowers[i % len(flowers)]))
    return crown


def grow_lantana(tree, rng, lod):
    return grow_bush(tree, rng, lod, ("foliage.neem_dark", "foliage.neem", "foliage.neem_light"), ["foliage.flower_orange", "foliage.flower_pink", "accent.yellow"])


def grow_bougainvillea(tree, rng, lod):
    """Bougainvillea: green masses heaped with magenta bracts."""
    crown = grow_bush(tree, rng, lod, ("foliage.neem_dark", "foliage.neem", "foliage.neem_light"), ["foliage.bougainvillea"],
                      blobs=6, radius=1.1, height=1.6, dots=0)
    # Bract clusters: magenta masses over the top of the green.
    for i in range(5):
        a = 2 * math.pi * i / 5 + rng.uniform(-0.3, 0.3)
        p = crown.centre + Vector((math.cos(a) * 0.55, math.sin(a) * 0.55, rng.uniform(0.25, 0.55)))
        crown.dots.append((p, 0.36, "foliage.bougainvillea"))
    return crown


SPECIES = {
    "neem": dict(grow=grow_neem, bark="foliage.bark", seed=11, kind="tree", tufts=150, tuft=0.3),
    "peepal": dict(grow=grow_peepal, bark="foliage.bark_pale", seed=23, kind="tree", tufts=170, tuft=0.34),
    "lantana": dict(grow=grow_lantana, bark="foliage.bark", seed=53, kind="bush", tufts=40, tuft=0.13),
    "bougainvillea": dict(grow=grow_bougainvillea, bark="foliage.bark", seed=61, kind="bush", tufts=40, tuft=0.14),
}


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    os.makedirs(OUT, exist_ok=True)
    for species in argv or list(SPECIES):
        export.reset_scene()
        kind = SPECIES[species]["kind"]
        lods = [make(species, lod) for lod in ((0, 1) if kind == "bush" else (0, 1, 2))]
        path = os.path.join(OUT, f"{kind}_{species}.glb")
        bpy.ops.object.select_all(action="DESELECT")
        for o in lods:
            o.select_set(True)
        bpy.context.view_layer.objects.active = lods[0]
        bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True, export_yup=True, export_apply=True,
                                  export_normals=True, export_texcoords=False, export_materials="EXPORT",
                                  export_vertex_color="ACTIVE", export_all_vertex_colors=False)
        print(f"[stylised {species}] {path}: " + ", ".join(f"{o.name} {geo.tri_count(o)}" for o in lods))


if __name__ == "__main__":
    main()

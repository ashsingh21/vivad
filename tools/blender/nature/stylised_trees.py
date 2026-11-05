"""The style guide's trees and bushes (docs/style-guide.md): a thick trunk with bark plates, and a
crown of a few soft leaf masses covered in small painted leaf cards. The masses are opaque and give
the crown its body and shade; the cards (one cut-out texture of neem twigs, near-white, tinted per
vertex from the palette) give it a fine, leafy outline. Shaded by vertex colour: light on top, dark
inside and underneath, normals bent out from the crown's centre so it lights as one soft volume.
Delhi's maps use these (shared::map::delhi); the ancient map keeps tools/blender/nature/trees.py's
until it's restyled.

blender -b --factory-startup -P tools/blender/nature/stylised_trees.py -- [species ...]
writes assets/models/props/stylised_{tree,bush}_<species>.gltf (its texture in models/textures,
where the game gives cut-outs their alpha-keeping mipmaps), LOD0..LOD2 (bushes LOD0..LOD1).
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
import numpy as np  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import export  # noqa: E402
import geo  # noqa: E402
import style  # noqa: E402
import textures  # noqa: E402
from trees import Tree  # noqa: E402

OUT = os.path.join(ROOT, "assets", "models", "props")
LEAF_TEX = os.path.join(ROOT, "build", "textures", "stylised", "leaf_twigs.png")

# The look, "dense" (the owner's pick, 2026-11-05, from a sheet of four): each leaf clump lit as
# its own, cards of its colour packed till the masses hardly show. VIVAD_TREE_VARIANT builds the
# others to compare: "clump" (fewer cards), "solid" (clumps alone), "fur" (one shading over the
# crown, the first leaf-card version).
VARIANT = os.environ.get("VIVAD_TREE_VARIANT", "dense")

# Face kinds (an integer attribute, which survives joins): bark, leaf mass, leaf card, then flowers.
BARK, MASS, CARD, FLOWER = 0, 1, 2, 3


def mix(a, b, t):
    t = min(max(t, 0.0), 1.0)
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def noise(p, seed):
    """Smooth, slow lumps: a few crossed sines (deterministic, no texture)."""
    s = seed * 1.37
    return (math.sin(p.x * 1.3 + s) * math.sin(p.y * 1.1 - s * 0.7) + math.sin(p.z * 1.7 + p.x * 0.6 + s * 1.9)) * 0.5


# ------------------------------------------------------------------ the leaf texture

def draw_leaves(path, size=512):
    """Three pinnate neem twigs fanning up from the bottom middle: almond leaflets in pairs along
    each stem and one at its tip, painted (style guide section 4): a flat light base, lighter to
    the tip, a soft darker midrib and rim. Near-white; the vertex colour tints it."""
    yy, xx = np.mgrid[0:size, 0:size].astype(np.float32) / size
    val = np.zeros((size, size), np.float32)
    alpha = np.zeros((size, size), np.float32)
    px = 1.5 / size

    def leaflet(cx, cy, ang, length, width, shade):
        # Local frame: u along the leaf (0 at its base, 1 at its tip), v across (1 at the edge).
        dx, dy = xx - cx, yy - cy
        ca, sa = math.cos(ang), math.sin(ang)
        u = (dx * ca + dy * sa) / length
        v = (-dx * sa + dy * ca) / width
        # Almond: widest a third along, pointed at the tip.
        half = np.clip(np.sin(np.clip(u, 0, 1) * math.pi), 0, None) ** 0.8 * np.clip(1.15 - 0.3 * u, 0, None)
        d = np.abs(v) - half
        inside = np.clip(0.5 - d * width / px, 0, 1) * ((u > 0) & (u < 1))
        tone = shade * (0.82 + 0.18 * np.clip(u, 0, 1))
        tone = tone * (1.0 - 0.18 * np.clip(1 - np.abs(v) / 0.12, 0, 1))  # midrib
        tone = tone * (1.0 - 0.12 * np.clip((np.abs(v) - half + 0.25) / 0.25, 0, 1))  # rim
        val[:] = np.where(inside > 0, tone * inside + val * (1 - inside), val)
        np.maximum(alpha, inside, out=alpha)

    def stem(x0, y0, x1, y1, w):
        dx, dy = x1 - x0, y1 - y0
        t = np.clip(((xx - x0) * dx + (yy - y0) * dy) / (dx * dx + dy * dy), 0, 1)
        d = np.hypot(xx - (x0 + t * dx), yy - (y0 + t * dy))
        a = np.clip(0.5 - (d - w) / px, 0, 1)
        val[:] = np.where(a > 0, 0.55 * a + val * (1 - a), val)
        np.maximum(alpha, a, out=alpha)

    rng = random.Random(7)
    base = (0.5, 0.98)
    for fan in (-0.55, 0.0, 0.55):
        ang = -math.pi / 2 + fan  # up the image (y grows down)
        L = 0.78 if fan == 0 else 0.62
        tip = (base[0] + math.cos(ang) * L, base[1] + math.sin(ang) * L)
        stem(base[0], base[1], tip[0], tip[1], 0.006)
        n = 6
        for i in range(n):
            t = 0.22 + 0.72 * i / (n - 1)
            sx, sy = base[0] + math.cos(ang) * L * t, base[1] + math.sin(ang) * L * t
            ll = 0.2 * (1.0 - 0.35 * t)
            for side in (-1, 1):
                leaflet(sx, sy, ang + side * 0.95, ll, ll * 0.34, rng.uniform(0.9, 1.0))
        leaflet(tip[0], tip[1], ang, 0.16, 0.055, 1.0)
    # Bleed the colour out under the transparent texels (mips mustn't fringe dark).
    val = np.where(alpha > 0.01, val, 0.85)
    rgba = np.dstack([val, val, val, alpha]).astype(np.float32)
    return textures.save_rgba(path, rgba)


# ------------------------------------------------------------------ the crown

class Crown:
    """Leaf masses ((centre, radii) ellipsoids) and the cards over them, built into one mesh."""

    def __init__(self, centre, radii, palette, seed):
        self.centre, self.radii, self.palette, self.seed = centre, radii, palette, seed
        self.blobs = []
        self.cards = []  # (position on a mass, outward normal, size, spin, tilt)
        self.dots = []  # (position, radius, colour token)
        self.trunk_top = 0.0  # bark below this is ridged

    def blob(self, c, r):
        self.blobs.append((c, r))

    def leafy(self, rng, count, size):
        """Cards over the masses' exposed surface."""
        weights = [r.x * r.y for _, r in self.blobs]
        tries = 0
        while len(self.cards) < count and tries < count * 40:
            tries += 1
            bi = rng.choices(range(len(self.blobs)), weights)[0]
            c, r = self.blobs[bi]
            d = Vector((rng.gauss(0, 1), rng.gauss(0, 1), rng.gauss(0, 1) * 0.8 + 0.2)).normalized()
            p = c + Vector((r.x * d.x, r.y * d.y, r.z * d.z))
            if any(o is not c and (Vector(((p.x - o.x) / q.x, (p.y - o.y) / q.y, (p.z - o.z) / q.z)).length < 0.95) for o, q in self.blobs):
                continue
            n = Vector((d.x / r.x, d.y / r.y, d.z / r.z)).normalized()
            self.cards.append((p, n, size * rng.uniform(0.8, 1.2), rng.uniform(0, 2 * math.pi), rng.uniform(0.35, 1.0), bi))

    def flower_toks(self):
        return sorted({d[2] for d in self.dots})

    def build(self, name, subdiv, lump):
        bm = bmesh.new()
        uvl = bm.loops.layers.uv.new("UVMap")
        kinds, owners = [], []
        for k, (c, r) in enumerate(self.blobs):
            ret = bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=1.0, matrix=Matrix.Identity(4))
            for v in ret["verts"]:
                d = v.co.normalized()
                rr = Vector((r.x * d.x, r.y * d.y, r.z * d.z))
                v.co = c + rr * (1.0 + lump * noise(c + rr * 1.6, self.seed + k))
            owners += [k] * (len(bm.faces) - len(kinds))
            kinds += [MASS] * (len(bm.faces) - len(kinds))
        for p, n, sz, spin, tilt, bi in self.cards:
            # A card rooted just inside the mass, growing out along it and tilted off it: its tip
            # past the surface, so the outline is leaves. Both sides, wound apart, each lit as the
            # crown (no back face turned dark).
            t = Vector((0, 0, 1)) - n * n.z
            t = t.normalized() if t.length > 1e-3 else Vector((1, 0, 0))
            t = (Matrix.Rotation(spin, 3, n) @ t).normalized()
            grow = (t * (1 - tilt) + n * tilt).normalized()
            side = grow.cross(n).normalized()
            root = p - n * sz * 0.15 - grow * sz * 0.1
            corners = [root - side * sz * 0.5, root + side * sz * 0.5, root + side * sz * 0.5 + grow * sz, root - side * sz * 0.5 + grow * sz]
            uvs = [(0, 0), (1, 0), (1, 1), (0, 1)]
            for order in ((0, 1, 2, 3), (3, 2, 1, 0)):
                f = bm.faces.new([bm.verts.new(corners[i]) for i in order])
                for loop, i in zip(f.loops, order):
                    loop[uvl].uv = uvs[i]
                kinds.append(CARD)
                owners.append(bi)
        toks = self.flower_toks()
        for p, rad, tok in self.dots:
            bmesh.ops.create_icosphere(bm, subdivisions=2 if rad > 0.2 else 1, radius=rad, matrix=Matrix.Translation(p))
            owners += [0] * (len(bm.faces) - len(kinds))
            kinds += [FLOWER + toks.index(tok)] * (len(bm.faces) - len(kinds))
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me)
        bm.free()
        obj = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(obj)
        tag(obj, lambda f: kinds[f])
        tag(obj, lambda f: owners[f], "blob")
        return obj


def ridge(p):
    """The bark's ridges, 0 in a groove to 1 on a plate: seven broad plates, narrow grooves."""
    c = math.cos(7 * math.atan2(p.y, p.x) + 0.5 * p.z)
    return min(max(0.5 + 0.9 * c, 0.0), 1.0)


def colour_and_normals(obj, crown, bark_tok):
    """Vertex colours (linear, per corner) and, on the leaves, normals bent out from the crown."""
    me = obj.data
    col = me.color_attributes.new("Col", "FLOAT_COLOR", "CORNER")
    me.color_attributes.active_color = col
    me.color_attributes.render_color_index = me.color_attributes.active_color_index
    dark, mid, light = (style.linear(t) for t in crown.palette)
    bark = style.linear(bark_tok)
    toks = crown.flower_toks()
    c, rad = crown.centre, crown.radii
    zlo, zhi = c.z - rad.z, c.z + rad.z
    normals = []
    corner = me.corner_normals
    kinds = me.attributes["kind"].data
    owner = me.attributes["blob"].data
    for poly in me.polygons:
        kind = kinds[poly.index].value
        bc, br = crown.blobs[min(owner[poly.index].value, len(crown.blobs) - 1)]
        for li in poly.loop_indices:
            p = me.vertices[me.loops[li].vertex_index].co
            own = corner[li].vector
            if kind == BARK:
                # Darker at the foot, a shade lighter up the limbs, dark in the grooves.
                k = 0.72 + 0.28 * min(p.z / max(c.z - rad.z, 0.5), 1.0)
                if p.z < crown.trunk_top:
                    k *= 0.55 + 0.45 * ridge(p)
                col.data[li].color = (bark[0] * k, bark[1] * k, bark[2] * k, 1.0)
                normals.append(own.copy())
                continue
            if kind >= FLOWER:
                k = 0.85 + 0.15 * max(own.z, 0.0)
                fc = style.linear(toks[kind - FLOWER])
                col.data[li].color = (fc[0] * k, fc[1] * k, fc[2] * k, 1.0)
                normals.append(own.copy())
                continue
            # Leaves: the sky's light from above, the crown's own shade inside and underneath; the
            # masses a step darker than the cards over them, so the cards read as leaves on top.
            q = Vector(((p.x - c.x) / rad.x, (p.y - c.y) / rad.y, (p.z - c.z) / rad.z))
            out = q.normalized() if q.length > 1e-4 else Vector((0, 0, 1))
            if VARIANT == "fur":
                up = (p.z - zlo) / (zhi - zlo)
                g = mix(dark, mid, up * 1.5)
                g = mix(g, light, (up - 0.6) * 2.2)
                g = mix(g, dark, (0.75 - min(q.length, 1.0)) * 1.6)
                v = (0.96 + 0.08 * noise(p * 0.8, crown.seed + 50)) * (1.0 if kind == CARD else 0.7)
                col.data[li].color = (g[0] * v, g[1] * v, g[2] * v, 1.0)
                normals.append((own * 0.4 + out * 0.6).normalized() if kind == MASS else out)
                continue
            # Each clump lit as its own: light across its top, dark under it (Fortnite's read: the
            # crown is a stack of clumps, each catching the sun), the whole crown a little darker
            # low down and inside; the cards take their clump's colour.
            bq = Vector(((p.x - bc.x) / br.x, (p.y - bc.y) / br.y, (p.z - bc.z) / br.z))
            lup = min(max(bq.z * 0.5 + 0.5, 0.0), 1.0)
            g = mix(dark, mid, lup * 1.8)
            g = mix(g, light, (lup - 0.62) * 2.6)
            cup = (p.z - zlo) / (zhi - zlo)
            v = (0.82 + 0.2 * min(max(cup, 0.0), 1.0)) * (0.96 + 0.08 * noise(p * 0.8, crown.seed + 50))
            col.data[li].color = (g[0] * v, g[1] * v, g[2] * v, 1.0)
            bout = bq.normalized() if bq.length > 1e-4 else Vector((0, 0, 1))
            normals.append((bout * 0.65 + out * 0.35).normalized())
    me.normals_split_custom_set(normals)


def tag(obj, kind, name="kind"):
    a = obj.data.attributes.new(name, "INT", "FACE")
    for f in range(len(obj.data.polygons)):
        a.data[f].value = kind(f)


def solid_material(name):
    m = bpy.data.materials.get(name)
    if m:
        return m
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    m.use_backface_culling = True
    m.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.88
    return m


def card_material(name, leaf_img):
    """The cards: the twig texture, cut out at half alpha (the colour comes from COLOR_0)."""
    m = bpy.data.materials.get(name)
    if m:
        return m
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    # Single sided: each card has both its faces modelled, lit alike.
    m.use_backface_culling = True
    nt = m.node_tree
    p = nt.nodes["Principled BSDF"]
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = leaf_img
    nt.links.new(tex.outputs["Color"], p.inputs["Base Color"])
    cut = nt.nodes.new("ShaderNodeMath")
    cut.operation = "GREATER_THAN"
    cut.inputs[1].default_value = 0.5
    nt.links.new(tex.outputs["Alpha"], cut.inputs[0])
    nt.links.new(cut.outputs[0], p.inputs["Alpha"])
    p.inputs["Roughness"].default_value = 0.8
    return m


def make(species, lod, leaf_img):
    cfg = SPECIES[species]
    rng = random.Random(cfg["seed"])
    tree = Tree(cfg["seed"], lod)
    # Enough sides round the trunk for its plates.
    tree.ring_segs = (28, 14, 6)[lod]
    crown = cfg["grow"](tree, rng, lod)
    many = {"dense": 2.2, "solid": 0.0}.get(VARIANT, 1.0)
    cards = int((cfg["cards"], cfg["cards"] * 2 // 5, 0)[lod] * many)
    crown.leafy(rng, cards, cfg["card"] * (1.0 if lod == 0 else 1.35) * (1.2 if VARIANT == "dense" else 1.0))
    # Bushes are small on screen: a level less.
    subdiv, lump = (((3, 0.08), (2, 0.05), (2, 0.0)) if cfg["kind"] == "tree" else ((2, 0.06), (2, 0.0)))[lod]
    if VARIANT == "solid":
        lump *= 2.0
    if lod == 2:
        # The far distance: the crown's three biggest masses.
        crown.blobs = sorted(crown.blobs, key=lambda b: -b[1].x)[:3]
        crown.dots = []
    if lod > 0:
        crown.dots = crown.dots[::3]
    parts = []
    if tree.bark_parts:
        bark = geo.join(tree.bark_parts, f"{species}_bark")
        if lod < 2:
            for v in bark.data.vertices:
                if v.co.z < crown.trunk_top:
                    k = 1.0 + 0.16 * (ridge(v.co) - 0.5)
                    v.co.x *= k
                    v.co.y *= k
        tag(bark, lambda f: BARK)
        tag(bark, lambda f: 0, "blob")
        for name in [u.name for u in bark.data.uv_layers]:
            bark.data.uv_layers.remove(bark.data.uv_layers[name])
        bark.data.uv_layers.new(name="UVMap")
        parts.append(bark)
    parts.append(crown.build(f"{species}_leaves", subdiv, lump))
    obj = geo.join(parts, f"{species}_LOD{lod}") if len(parts) > 1 else parts[0]
    obj.name = f"{species}_LOD{lod}"
    me = obj.data
    me.materials.clear()
    me.materials.append(solid_material("stylised_solid"))
    kinds = me.attributes["kind"].data
    if any(kinds[p.index].value == CARD for p in me.polygons):
        me.materials.append(card_material("stylised_leaves", leaf_img))
    for p in me.polygons:
        p.material_index = 1 if kinds[p.index].value == CARD else 0
        p.use_smooth = True
    colour_and_normals(obj, crown, cfg["bark"])
    return obj


# ------------------------------------------------------------------ species

def limbs(tree, rng, top, targets, r0, r1):
    for t in targets:
        d = t - top
        tree.branch(top, d, d.length, r0, r1, segments=5, wobble=0.05)


def grow_neem(tree, rng, lod, s=1.0, ring=6, tokens=("foliage.neem_dark", "foliage.neem", "foliage.neem_light")):
    """Neem: a short fat trunk, stout limbs up into a round crown of a dozen or so masses."""
    thick = 1.0 if VARIANT == "fur" else 0.8
    tree.branch(Vector((0, 0, 0)), Vector((0.04, 0.02, 1)), 2.7 * s, 0.5 * s * thick, 0.36 * s * thick, flare=0.8, segments=6, wobble=0.04)
    top = Vector((0.0, 0.0, 2.5 * s))
    c = Vector((0, 0, 5.7 * s))
    crown = Crown(c, Vector((3.4, 3.4, 2.3)) * s, tokens, tree.rng.randrange(1000))
    crown.trunk_top = 2.4 * s
    crown.blob(c + Vector((0, 0, 0.4)) * s, Vector((2.0, 2.0, 1.7)) * s)
    targets = []
    for i in range(ring):
        a = 2 * math.pi * i / ring + rng.uniform(-0.25, 0.25)
        d = rng.uniform(1.6, 2.0) * s
        p = c + Vector((math.cos(a) * d, math.sin(a) * d, rng.uniform(-0.5, 0.2) * s))
        crown.blob(p, Vector((1.4, 1.4, 1.15)) * s * rng.uniform(0.9, 1.1))
        if i % 2 == 0:
            targets.append(p - Vector((0, 0, 0.4)) * s)
    for i in range(2):
        a = rng.uniform(0, 2 * math.pi)
        crown.blob(c + Vector((math.cos(a) * 0.9, math.sin(a) * 0.9, 1.4)) * s, Vector((1.2, 1.2, 1.0)) * s)
    for i in range(ring // 2):
        a = 2 * math.pi * (i + 0.5) / (ring // 2) + rng.uniform(-0.3, 0.3)
        crown.blob(c + Vector((math.cos(a) * 1.5, math.sin(a) * 1.5, -0.9)) * s, Vector((1.2, 1.2, 0.85)) * s)
    limbs(tree, rng, top, targets, 0.24 * s, 0.12 * s)
    return crown


def grow_peepal(tree, rng, lod):
    """Peepal: taller and broader than a neem, paler bark, a lighter, brighter green."""
    return grow_neem(tree, rng, lod, s=1.25, ring=7, tokens=("foliage.peepal_dark", "foliage.peepal", "foliage.peepal_light"))


def grow_bush(tree, rng, lod, tokens, flowers, blobs=5, radius=1.0, height=1.3, dots=26):
    c = Vector((0, 0, height * 0.5))
    crown = Crown(c, Vector((radius, radius, height * 0.55)), tokens, tree.rng.randrange(1000))
    crown.blob(c + Vector((0, 0, 0.1)), Vector((0.65, 0.65, 0.55)) * radius)
    for i in range(blobs - 1):
        a = 2 * math.pi * i / (blobs - 1) + rng.uniform(-0.3, 0.3)
        p = c + Vector((math.cos(a) * 0.45 * radius, math.sin(a) * 0.45 * radius, rng.uniform(-0.15, 0.2) * height))
        crown.blob(p, Vector((0.5, 0.5, 0.44)) * radius * rng.uniform(0.9, 1.1))
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
    for i in range(5):
        a = 2 * math.pi * i / 5 + rng.uniform(-0.3, 0.3)
        p = crown.centre + Vector((math.cos(a) * 0.55, math.sin(a) * 0.55, rng.uniform(0.25, 0.55)))
        crown.dots.append((p, 0.36, "foliage.bougainvillea"))
    return crown


SPECIES = {
    "neem": dict(grow=grow_neem, bark="foliage.bark", seed=11, kind="tree", cards=380, card=0.9),
    "peepal": dict(grow=grow_peepal, bark="foliage.bark", seed=23, kind="tree", cards=440, card=1.05),
    "lantana": dict(grow=grow_lantana, bark="foliage.bark", seed=53, kind="bush", cards=100, card=0.38),
    "bougainvillea": dict(grow=grow_bougainvillea, bark="foliage.bark", seed=61, kind="bush", cards=100, card=0.42),
}


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    for species in argv or list(SPECIES):
        export.reset_scene()
        leaf_img = draw_leaves(LEAF_TEX)
        kind = SPECIES[species]["kind"]
        lods = [make(species, lod, leaf_img) for lod in ((0, 1) if kind == "bush" else (0, 1, 2))]
        path = os.path.join(OUT, f"stylised_{kind}_{species}.gltf")
        export.export_glb(lods, path, export_vertex_color="ACTIVE", export_all_vertex_colors=False)
        print(f"[stylised {species}] {path}: " + ", ".join(f"{o.name} {geo.tri_count(o)}" for o in lods))


if __name__ == "__main__":
    main()

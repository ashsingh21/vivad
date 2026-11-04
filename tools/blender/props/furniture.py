"""The indoor furniture with a baked texture (street_kit.py builds it in vertex colour): every piece
from street_kit.FURNITURE joined, unwrapped into one shared square, each face given a procedural
material by what it's made of (wood grain, velvet, rope webbing, cotton weave, enamelled steel,
chrome, black plastic, glazed tiles, the stove's black stone), baked to one texture set, then
split back into its pieces. One material for all of them, so a room of it merges into one draw.

blender -b --factory-startup -P tools/blender/props/furniture.py   -> assets/models/props/{furniture,racks}.glb
(<piece>_LOD0, <piece>_LOD1; drawn merged per chunk by crate::street like the kit, with this material)
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402

import bake  # noqa: E402
import export  # noqa: E402
import street_kit  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
PROPS = os.path.join(ROOT, "assets", "models", "props")
TEX = os.path.join(ROOT, "build", "furniture_tex")
RES = 2048

# What each piece is mostly made of; its wood, metal and black parts are found by their colour.
BODY = {"charpai": "wood", "almirah": "enamel", "dining": "wood", "sofa": "velvet", "tv": "wood", "fridge": "enamel",
        "fan": "enamel", "dari": "cotton", "desk": "laminate", "stove": "tile",
        "rack_kirana": "print", "rack_bartan": "metal", "rack_kapda": "cotton", "rack_dabba": "kraft"}


def kind(piece, rgb, made_of=0):
    """A face's material: what the piece marked it as (Piece.made_of), else from its piece and
    its (linear) vertex colour."""
    if made_of:
        return street_kit.SURFACES[made_of]
    near = lambda name: all(abs(a - b) < 0.02 for a, b in zip(rgb, street_kit.lin(street_kit.C[name])))  # noqa: E731
    if near("chrome") or near("steel") or near("kettle"):
        return "metal"
    if max(rgb) < 0.02:
        return "stone" if piece == "stove" else "plastic"
    if near("wood") or near("wood_lt"):
        return "wood"
    if near("rope"):
        return "rope"
    if piece == "charpai":
        return "cotton"
    if piece == "stove" and rgb[0] > 0.5 and rgb[1] < 0.1:
        return "enamel"
    return BODY[piece]


def material(name):
    """The vertex colour worked over by a pattern for `name` (in object space, so it's the same
    scale on every piece); roughness, metal and a little relief to go with it."""
    m = bpy.data.materials.new(f"furn_{name}")
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    N, L = nt.nodes.new, nt.links.new
    out = N("ShaderNodeOutputMaterial")
    out.name = "OUT"
    bsdf = N("ShaderNodeBsdfPrincipled")
    bsdf.name = "BSDF"
    L(bsdf.outputs["BSDF"], out.inputs["Surface"])
    col = N("ShaderNodeAttribute")
    col.attribute_name = "Col"
    tc = N("ShaderNodeTexCoord")

    def tex(node, scale, **kw):
        n = N(node)
        n.inputs["Scale"].default_value = scale
        for k, v in kw.items():
            if k in n.inputs:
                n.inputs[k].default_value = v
            else:
                setattr(n, k, v)
        L(tc.outputs["Object"], n.inputs["Vector"])
        return n

    def maths(op, a, b):
        n = N("ShaderNodeMath")
        n.operation = op
        for i, v in enumerate((a, b)):
            if isinstance(v, (int, float)):
                n.inputs[i].default_value = v
            else:
                L(v, n.inputs[i])
        return n.outputs[0]

    def shade(fac, lo, hi):
        """The colour scaled between lo and hi by `fac` (0-1)."""
        r = N("ShaderNodeMapRange")
        r.inputs["To Min"].default_value = lo
        r.inputs["To Max"].default_value = hi
        L(fac, r.inputs["Value"])
        mul = N("ShaderNodeVectorMath")
        mul.operation = "SCALE"
        L(col.outputs["Color"], mul.inputs[0])
        L(r.outputs[0], mul.inputs["Scale"])
        L(mul.outputs[0], bsdf.inputs["Base Color"])

    def relief(fac, strength):
        b = N("ShaderNodeBump")
        b.inputs["Strength"].default_value = strength
        b.inputs["Distance"].default_value = 0.003
        L(fac, b.inputs["Height"])
        L(b.outputs["Normal"], bsdf.inputs["Normal"])

    rough, metal = 0.6, 0.0
    if name == "wood":
        # Grain: long rings, wavering, with darker streaks.
        w = tex("ShaderNodeTexWave", 6.0, wave_type="RINGS", rings_direction="Z", Distortion=6.0, **{"Detail": 3.0})
        n = tex("ShaderNodeTexNoise", 40.0)
        g = maths("MULTIPLY", w.outputs["Fac"], maths("ADD", 0.6, maths("MULTIPLY", n.outputs["Fac"], 0.6)))
        shade(g, 0.72, 1.12)
        relief(g, 0.15)
        rough = 0.55
    elif name == "laminate":
        w = tex("ShaderNodeTexWave", 3.0, wave_type="BANDS", bands_direction="X", Distortion=3.0)
        shade(w.outputs["Fac"], 0.9, 1.05)
        rough = 0.4
    elif name == "velvet":
        # A fine pile, mottled where it's been sat on.
        n = tex("ShaderNodeTexNoise", 9.0, **{"Detail": 6.0})
        f = tex("ShaderNodeTexNoise", 300.0)
        shade(maths("ADD", maths("MULTIPLY", n.outputs["Fac"], 0.7), maths("MULTIPLY", f.outputs["Fac"], 0.3)), 0.75, 1.15)
        relief(f.outputs["Fac"], 0.25)
        rough = 0.95
    elif name in ("rope", "cotton"):
        # A basket weave: two sets of bands across each other.
        a = tex("ShaderNodeTexWave", 70.0 if name == "rope" else 140.0, wave_type="BANDS", bands_direction="X")
        b = tex("ShaderNodeTexWave", 70.0 if name == "rope" else 140.0, wave_type="BANDS", bands_direction="Y")
        weave = maths("MULTIPLY", maths("ADD", a.outputs["Fac"], b.outputs["Fac"]), 0.5)
        shade(weave, 0.7, 1.08)
        relief(weave, 0.5 if name == "rope" else 0.25)
        rough = 0.95
    elif name == "enamel":
        # Painted steel: smooth, faint orange-peel, a few scuffs.
        n = tex("ShaderNodeTexNoise", 60.0)
        s = tex("ShaderNodeTexNoise", 6.0, **{"Detail": 8.0})
        shade(maths("ADD", maths("MULTIPLY", n.outputs["Fac"], 0.3), maths("MULTIPLY", s.outputs["Fac"], 0.7)), 0.88, 1.06)
        relief(n.outputs["Fac"], 0.05)
        rough = 0.32
    elif name == "glass":
        # Glass over what's in the jar: glossy, a faint streak of reflection, the contents grainy.
        n = tex("ShaderNodeTexNoise", 150.0)
        shade(n.outputs["Fac"], 0.8, 1.1)
        relief(n.outputs["Fac"], 0.1)
        rough = 0.06
    elif name == "print":
        # Printed card and plastic: fine bands of lettering and a mottle of ink.
        w = tex("ShaderNodeTexWave", 90.0, wave_type="BANDS", bands_direction="Z", Distortion=2.0)
        n = tex("ShaderNodeTexNoise", 25.0)
        shade(maths("ADD", maths("MULTIPLY", w.outputs["Fac"], 0.5), maths("MULTIPLY", n.outputs["Fac"], 0.5)), 0.78, 1.08)
        rough = 0.35
    elif name == "kraft":
        # Corrugated card: its fibres, flutes showing faintly, scuffs.
        f = tex("ShaderNodeTexNoise", 220.0, **{"Detail": 4.0})
        w = tex("ShaderNodeTexWave", 160.0, wave_type="BANDS", bands_direction="Z")
        s = tex("ShaderNodeTexNoise", 5.0)
        mix = maths("ADD", maths("ADD", maths("MULTIPLY", f.outputs["Fac"], 0.4), maths("MULTIPLY", w.outputs["Fac"], 0.15)), maths("MULTIPLY", s.outputs["Fac"], 0.45))
        shade(mix, 0.78, 1.1)
        relief(w.outputs["Fac"], 0.15)
        rough = 0.9
    elif name == "metal":
        n = tex("ShaderNodeTexNoise", 120.0)
        shade(n.outputs["Fac"], 0.9, 1.05)
        rough, metal = 0.28, 1.0
    elif name == "plastic":
        n = tex("ShaderNodeTexNoise", 200.0)
        shade(n.outputs["Fac"], 0.9, 1.1)
        rough = 0.45
    elif name == "stone":
        n = tex("ShaderNodeTexNoise", 30.0, **{"Detail": 10.0})
        shade(n.outputs["Fac"], 0.6, 2.2)
        rough = 0.25
    elif name == "tile":
        # Glazed 15 cm tiles in grout lines.
        b = tex("ShaderNodeTexBrick", 6.6, offset=0.0, **{"Mortar Size": 0.03})
        b.inputs["Color1"].default_value = (1, 1, 1, 1)
        b.inputs["Color2"].default_value = (0.92, 0.92, 0.92, 1)
        b.inputs["Mortar"].default_value = (0.55, 0.55, 0.55, 1)
        bw = N("ShaderNodeRGBToBW")
        L(b.outputs["Color"], bw.inputs["Color"])
        shade(bw.outputs["Val"], 0.0, 1.0)
        relief(b.outputs["Fac"], 0.3)
        rough = 0.2
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    return m


def build(makers, prefix):
    """The pieces `makers` build, as one unwrapped object baked to one texture set (`prefix`),
    split back into their pieces."""
    bake.setup_cycles(16)
    pieces = []
    for make in makers:
        made = make()
        for o in made:
            if o.name.endswith("_LOD0"):
                pieces.append(o)
            else:
                bpy.data.objects.remove(o, do_unlink=True)
    names = [o.name.replace("_LOD0", "") for o in pieces]
    mats = {}
    for i, o in enumerate(pieces):
        piece = names[i]
        me = o.data
        me.materials.clear()
        bm = bmesh.new()
        bm.from_mesh(me)
        layer = bm.loops.layers.float_color["Col"]
        surface = bm.faces.layers.int.get("surface")
        tag = bm.faces.layers.int.new("piece")
        kinds = []
        for f in bm.faces:
            rgb = tuple(sum(lp[layer][c] for lp in f.loops) / len(f.loops) for c in range(3))
            k = kind(piece, rgb, f[surface] if surface else 0)
            if k not in kinds:
                kinds.append(k)
            f.material_index = kinds.index(k)
            f[tag] = i
        bm.to_mesh(me)
        bm.free()
        for k in kinds:
            me.materials.append(mats.setdefault(k, material(k)))
    # One object, one unwrap, one texture set.
    bpy.ops.object.select_all(action="DESELECT")
    for o in pieces:
        o.select_set(True)
    bpy.context.view_layer.objects.active = pieces[0]
    bpy.ops.object.join()
    whole = pieces[0]
    whole.name = whole.data.name = f"{prefix}_LOD0"
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.15, island_margin=0.003)
    bpy.ops.object.mode_set(mode="OBJECT")
    whole.data.uv_layers[0].name = "UVMap"
    export.pack_uvs(whole)
    images = bake.bake_texture_set(whole, TEX, prefix, res=RES)
    whole.data.materials.clear()
    whole.data.materials.append(export.game_material(prefix, images))
    for p in whole.data.polygons:
        p.material_index = 0
    export.strip_bake_data(whole)
    for c in list(whole.data.color_attributes):
        whole.data.color_attributes.remove(c)
    # Split back into the pieces, each keeping its origin (all were built about their own).
    out = []
    for i, name in enumerate(names):
        o = whole.copy()
        o.data = whole.data.copy()
        o.name = o.data.name = f"{name}_LOD0"
        bpy.context.scene.collection.objects.link(o)
        bm = bmesh.new()
        bm.from_mesh(o.data)
        tag = bm.faces.layers.int["piece"]
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if f[tag] != i], context="FACES")
        bm.faces.layers.int.remove(tag)
        bm.to_mesh(o.data)
        bm.free()
        out.append(o)
    bpy.data.objects.remove(whole, do_unlink=True)
    return out


def main():
    # The furniture, and the shops' racks apart (drawn near only and casting no shadow: crate::street).
    for makers, prefix in ((street_kit.FURNITURE, "furniture"), ((street_kit.racks,), "racks")):
        export.reset_scene()
        lods = []
        for lod0 in build(makers, prefix):
            lods += export.make_lods(lod0, (0.4,))
        out = os.path.join(PROPS, f"{prefix}.glb")
        tris = export.export_glb(lods, out)
        print(f"[furniture] {out}: {tris}")


if __name__ == "__main__":
    main()

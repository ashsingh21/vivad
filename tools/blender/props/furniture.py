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

    # The style guide's painted recipe (docs/style-guide.md section 4), the same for every
    # material: the colour, lighter toward the top of the piece and darker at its foot, its rounded
    # edges lit (from the bevels' curvature), and only broad, soft pattern over that.
    sep = N("ShaderNodeSeparateXYZ")
    L(tc.outputs["Object"], sep.inputs["Vector"])
    grad = N("ShaderNodeMapRange")
    grad.inputs["From Min"].default_value, grad.inputs["From Max"].default_value = 0.0, 1.8
    grad.inputs["To Min"].default_value, grad.inputs["To Max"].default_value = 0.9, 1.06
    L(sep.outputs["Z"], grad.inputs["Value"])
    geo = N("ShaderNodeNewGeometry")
    edge = N("ShaderNodeMapRange")
    edge.inputs["From Min"].default_value, edge.inputs["From Max"].default_value = 0.5, 0.58
    edge.inputs["To Min"].default_value, edge.inputs["To Max"].default_value = 1.0, 1.16
    L(geo.outputs["Pointiness"], edge.inputs["Value"])
    light = maths("MULTIPLY", grad.outputs[0], edge.outputs[0])

    def shade(fac, lo, hi):
        """The colour scaled between lo and hi by `fac` (0-1), then lit by the recipe."""
        r = N("ShaderNodeMapRange")
        r.inputs["To Min"].default_value = lo
        r.inputs["To Max"].default_value = hi
        L(fac, r.inputs["Value"])
        mul = N("ShaderNodeVectorMath")
        mul.operation = "SCALE"
        L(col.outputs["Color"], mul.inputs[0])
        L(maths("MULTIPLY", r.outputs[0], light), mul.inputs["Scale"])
        L(mul.outputs[0], bsdf.inputs["Base Color"])

    def relief(fac, strength):
        b = N("ShaderNodeBump")
        b.inputs["Strength"].default_value = strength
        b.inputs["Distance"].default_value = 0.003
        L(fac, b.inputs["Height"])
        L(b.outputs["Normal"], bsdf.inputs["Normal"])

    def soft(scale, lo, hi):
        """Big slow patches only: a smooth noise `scale` a metre, between lo and hi."""
        n = tex("ShaderNodeTexNoise", scale, **{"Detail": 0.0})
        shade(n.outputs["Fac"], lo, hi)
        return n

    rough, metal = 0.6, 0.0
    if name == "wood":
        # Painted wood: broad soft streaks along the grain, no rings.
        w = tex("ShaderNodeTexWave", 3.0, wave_type="BANDS", bands_direction="X", Distortion=1.5, **{"Detail": 0.0})
        shade(w.outputs["Fac"], 0.9, 1.04)
        relief(w.outputs["Fac"], 0.05)
        rough = 0.6
    elif name == "laminate":
        soft(1.5, 0.96, 1.03)
        rough = 0.45
    elif name == "velvet":
        soft(2.0, 0.92, 1.04)
        rough = 0.9
    elif name in ("rope", "cotton"):
        # A weave you can see at 5 m: broad soft bands, not the threads.
        a = tex("ShaderNodeTexWave", 22.0 if name == "rope" else 9.0, wave_type="BANDS", bands_direction="X", **{"Detail": 0.0})
        shade(a.outputs["Fac"], 0.9, 1.03)
        relief(a.outputs["Fac"], 0.15)
        rough = 0.92
    elif name == "enamel":
        soft(1.5, 0.97, 1.02)
        rough = 0.35
    elif name == "glass":
        soft(2.0, 0.9, 1.1)
        rough = 0.08
    elif name == "print":
        w = tex("ShaderNodeTexWave", 30.0, wave_type="BANDS", bands_direction="Z", Distortion=1.0, **{"Detail": 0.0})
        shade(w.outputs["Fac"], 0.88, 1.04)
        rough = 0.4
    elif name == "kraft":
        w = tex("ShaderNodeTexWave", 40.0, wave_type="BANDS", bands_direction="Z", **{"Detail": 0.0})
        shade(w.outputs["Fac"], 0.92, 1.03)
        relief(w.outputs["Fac"], 0.08)
        rough = 0.9
    elif name == "metal":
        soft(2.0, 0.94, 1.04)
        rough, metal = 0.3, 1.0
    elif name == "plastic":
        soft(2.0, 0.95, 1.05)
        rough = 0.45
    elif name == "stone":
        soft(3.0, 0.85, 1.25)
        rough = 0.3
    elif name == "tile":
        # Glazed 15 cm tiles in soft grout.
        b = tex("ShaderNodeTexBrick", 6.6, offset=0.0, **{"Mortar Size": 0.04, "Mortar Smooth": 0.6})
        b.inputs["Color1"].default_value = (1, 1, 1, 1)
        b.inputs["Color2"].default_value = (0.95, 0.95, 0.95, 1)
        b.inputs["Mortar"].default_value = (0.7, 0.7, 0.7, 1)
        bw = N("ShaderNodeRGBToBW")
        L(b.outputs["Color"], bw.inputs["Color"])
        shade(bw.outputs["Val"], 0.0, 1.0)
        relief(b.outputs["Fac"], 0.2)
        rough = 0.25
    else:
        shade(grad.outputs[0], 1.0, 1.0)
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
    # The joined pieces' meshes linger unused under their names: gone, so the split pieces get them
    # (the game finds a piece by its mesh's name; a ".001" left it the old vertex-coloured one).
    for m in list(bpy.data.meshes):
        if m.users == 0:
            bpy.data.meshes.remove(m)
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

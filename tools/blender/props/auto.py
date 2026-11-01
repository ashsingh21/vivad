"""The street auto-rickshaw with a baked texture (the rest of the street kit is vertex colour only):
its shape from street_kit.auto(), unwrapped, and a texture set baked from a procedural material:
glossy painted panels with seam lines, canvas weave and stitched seams on the canopy and its back,
road dust climbing the lower body, worn light edges, and the shading of its own nooks.

blender -b --factory-startup -P tools/blender/props/auto.py   -> assets/models/props/auto.glb
(auto_LOD0, auto_LOD1; drawn merged per chunk by crate::street like the kit, with this material)
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402

import bake  # noqa: E402
import export  # noqa: E402
import street_kit  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
OUT = os.path.join(ROOT, "assets", "models", "props", "auto.glb")
TEX = os.path.join(ROOT, "build", "auto_tex")
RES = 1024


def detail_material():
    """Vertex colour (the paint, with its baked AO) worked over by position: canvas above the
    tub's waist and on the back, paint below it; dust from the ground up; edges worn light."""
    m = bpy.data.materials.new("auto_bake")
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
    xyz = N("ShaderNodeSeparateXYZ")
    L(tc.outputs["Object"], xyz.inputs["Vector"])

    def math(op, a, b=None, clamp=False):
        n = N("ShaderNodeMath")
        n.operation = op
        n.use_clamp = clamp
        for i, v in enumerate((a, b)):
            if v is None:
                continue
            if isinstance(v, (int, float)):
                n.inputs[i].default_value = v
            else:
                L(v, n.inputs[i])
        return n.outputs[0]

    def ramp(v, lo, hi):
        n = N("ShaderNodeMapRange")
        n.inputs["From Min"].default_value = lo
        n.inputs["From Max"].default_value = hi
        L(v, n.inputs["Value"])
        return n.outputs[0]

    x, y, z = xyz.outputs["X"], xyz.outputs["Y"], xyz.outputs["Z"]
    # Fabric: the canopy (above 1.55 m) and its back (behind -1.15 m, above the tub).
    canopy = ramp(z, 1.55, 1.6)
    back = math("MULTIPLY", ramp(y, -1.12, -1.17), ramp(z, 0.92, 0.97))
    fabric = math("MAXIMUM", canopy, back)
    # Canvas: a fine weave, and stitched seams running front to back every 35 cm.
    wave = N("ShaderNodeTexWave")
    wave.wave_type = "BANDS"
    wave.inputs["Scale"].default_value = 260.0
    wave.inputs["Distortion"].default_value = 0.0
    L(tc.outputs["Object"], wave.inputs["Vector"])
    wave2 = N("ShaderNodeTexWave")
    wave2.wave_type = "BANDS"
    wave2.bands_direction = "Y"
    wave2.inputs["Scale"].default_value = 260.0
    L(tc.outputs["Object"], wave2.inputs["Vector"])
    weave = math("MULTIPLY", math("ADD", wave.outputs["Fac"], wave2.outputs["Fac"]), 0.5)
    seam = ramp(math("ABSOLUTE", math("SINE", math("MULTIPLY", x, 9.0))), 0.06, 0.0)
    # Paint: panel seams round the tub at its waist and skirt, and down its side behind the driver.
    p1 = ramp(math("ABSOLUTE", math("SUBTRACT", z, 0.86)), 0.008, 0.0)
    p2 = ramp(math("ABSOLUTE", math("SUBTRACT", z, 0.47)), 0.008, 0.0)
    p3 = ramp(math("ABSOLUTE", math("SUBTRACT", y, 0.32)), 0.01, 0.0)
    panel = math("MULTIPLY", math("MAXIMUM", math("MAXIMUM", p1, p2), p3), ramp(z, 1.5, 1.45))
    # Dust from the ground up, blotchy; worn light edges where the shape turns sharpest.
    noise = N("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 4.0
    L(tc.outputs["Object"], noise.inputs["Vector"])
    dust = math("MULTIPLY", ramp(z, 0.62, 0.15), math("ADD", 0.45, math("MULTIPLY", noise.outputs["Fac"], 0.8)), clamp=True)
    geo = N("ShaderNodeNewGeometry")
    edge = ramp(geo.outputs["Pointiness"], 0.52, 0.6)

    # Colour: weave darkens fabric a little, seams more; panels darken paint at their lines; edges
    # lighten; dust pulls toward road brown.
    shade = math("SUBTRACT", 1.0, math("ADD", math("MULTIPLY", fabric, math("ADD", math("MULTIPLY", weave, 0.12), math("MULTIPLY", seam, 0.35))),
                                        math("MULTIPLY", panel, 0.45)))
    lift = math("ADD", 1.0, math("MULTIPLY", edge, 0.25))
    tone = math("MULTIPLY", shade, lift)
    mul = N("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs["Factor"].default_value = 1.0
    L(col.outputs["Color"], mul.inputs["A"])
    comb = N("ShaderNodeCombineColor")
    for c in ("Red", "Green", "Blue"):
        L(tone, comb.inputs[c])
    L(comb.outputs["Color"], mul.inputs["B"])
    dusty = N("ShaderNodeMix")
    dusty.data_type = "RGBA"
    # Dust shows on paint and canvas, not on black rubber and chrome-dark parts.
    lum = N("ShaderNodeRGBToBW")
    L(col.outputs["Color"], lum.inputs["Color"])
    on = ramp(lum.outputs["Val"], 0.004, 0.03)
    L(math("MULTIPLY", math("MULTIPLY", dust, 0.55), on), dusty.inputs["Factor"])
    L(mul.outputs["Result"], dusty.inputs["A"])
    dusty.inputs["B"].default_value = (0.42, 0.33, 0.22, 1.0)
    L(dusty.outputs["Result"], bsdf.inputs["Base Color"])
    # Gloss: paint shines (less where dusty), canvas doesn't.
    rough = math("ADD", 0.32, math("ADD", math("MULTIPLY", fabric, 0.55), math("MULTIPLY", dust, 0.4)), clamp=True)
    L(rough, bsdf.inputs["Roughness"])
    bsdf.inputs["Metallic"].default_value = 0.0
    # Relief: weave and seams on the canvas, panel lines in the paint.
    height = math("SUBTRACT", math("MULTIPLY", fabric, math("ADD", math("MULTIPLY", weave, 0.4), math("MULTIPLY", seam, -1.0))), panel)
    bump = N("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.6
    bump.inputs["Distance"].default_value = 0.004
    L(height, bump.inputs["Height"])
    L(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def main():
    export.reset_scene()
    bake.setup_cycles(16)
    lod0, lod1 = street_kit.auto()
    bpy.data.objects.remove(lod1, do_unlink=True)
    lod0.data.materials.clear()
    lod0.data.materials.append(detail_material())
    # Unwrap: islands by angle, packed into the square.
    bpy.ops.object.select_all(action="DESELECT")
    lod0.select_set(True)
    bpy.context.view_layer.objects.active = lod0
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.15, island_margin=0.004)
    bpy.ops.object.mode_set(mode="OBJECT")
    lod0.data.uv_layers[0].name = "UVMap"
    export.pack_uvs(lod0)
    images = bake.bake_texture_set(lod0, TEX, "auto", res=RES)
    lod0.data.materials.clear()
    lod0.data.materials.append(export.game_material("auto", images))
    export.strip_bake_data(lod0)
    lods = export.make_lods(lod0, (0.4,))
    tris = export.export_glb(lods, OUT)
    print(f"[auto] {OUT}: {tris}")


main()

"""Tiny builder for shader node trees so materials read as math, not node plumbing."""

import bpy


class Graph:
    def __init__(self, material):
        self.mat = material
        self.nt = material.node_tree
        self.nt.nodes.clear()
        self.x = 0

    # -- core ------------------------------------------------------------------------------------
    def node(self, kind, inputs=None, **props):
        n = self.nt.nodes.new(kind)
        n.location = (self.x, 0)
        self.x += 40
        for k, v in props.items():
            setattr(n, k, v)
        for key, val in (inputs or {}).items():
            self.set(n.inputs[key], val)
        return n

    def set(self, socket, val):
        if isinstance(val, bpy.types.NodeSocket):
            self.nt.links.new(val, socket)
        elif val is not None:
            socket.default_value = val

    # -- sources ---------------------------------------------------------------------------------
    def attr(self, name):
        return self.node("ShaderNodeAttribute", attribute_name=name, attribute_type="GEOMETRY").outputs["Fac"]

    def texcoord(self, which="Object"):
        return self.node("ShaderNodeTexCoord").outputs[which]

    def geometry(self, which):
        return self.node("ShaderNodeNewGeometry").outputs[which]

    def value(self, v):
        n = self.node("ShaderNodeValue")
        n.outputs[0].default_value = v
        return n.outputs[0]

    # -- math ------------------------------------------------------------------------------------
    def math(self, op, a, b=0.0, c=0.0, clamp=False):
        n = self.node("ShaderNodeMath", operation=op, use_clamp=clamp)
        self.set(n.inputs[0], a)
        self.set(n.inputs[1], b)
        self.set(n.inputs[2], c)
        return n.outputs[0]

    def add(self, a, b): return self.math("ADD", a, b)
    def sub(self, a, b): return self.math("SUBTRACT", a, b)
    def mul(self, a, b): return self.math("MULTIPLY", a, b)
    def maxf(self, a, b): return self.math("MAXIMUM", a, b)
    def minf(self, a, b): return self.math("MINIMUM", a, b)
    def absf(self, a): return self.math("ABSOLUTE", a)
    def clamp(self, a): return self.math("ADD", a, 0.0, clamp=True)

    def smooth(self, x, e0, e1):
        """smoothstep(e0, e1, x); e0 > e1 gives an inverted step."""
        n = self.node("ShaderNodeMapRange", interpolation_type="SMOOTHSTEP", clamp=True)
        self.set(n.inputs["Value"], x)
        self.set(n.inputs["From Min"], e0)
        self.set(n.inputs["From Max"], e1)
        return n.outputs["Result"]

    def remap(self, x, a0, a1, b0, b1, clamp=True):
        n = self.node("ShaderNodeMapRange", clamp=clamp)
        for k, v in (("Value", x), ("From Min", a0), ("From Max", a1), ("To Min", b0), ("To Max", b1)):
            self.set(n.inputs[k], v)
        return n.outputs["Result"]

    def band(self, x, lo, hi, aa):
        """1 inside [lo, hi] with soft edges of width aa."""
        return self.mul(self.smooth(x, lo - aa, lo), self.smooth(x, hi + aa, hi))

    def mix(self, fac, a, b):
        """Float lerp."""
        n = self.node("ShaderNodeMix", data_type="FLOAT", clamp_factor=True)
        self.set(n.inputs[0], fac)
        self.set(n.inputs[2], a)
        self.set(n.inputs[3], b)
        return n.outputs[0]

    def mixc(self, fac, a, b):
        """Colour lerp (a, b are RGBA tuples or colour sockets)."""
        n = self.node("ShaderNodeMix", data_type="RGBA", clamp_factor=True)
        self.set(n.inputs[0], fac)
        self.set(n.inputs[6], a if not isinstance(a, tuple) or len(a) == 4 else (*a, 1.0))
        self.set(n.inputs[7], b if not isinstance(b, tuple) or len(b) == 4 else (*b, 1.0))
        return n.outputs[2]

    def vec(self, op, a, b=None, scale=None):
        n = self.node("ShaderNodeVectorMath", operation=op)
        self.set(n.inputs[0], a)
        if b is not None:
            self.set(n.inputs[1], b)
        if scale is not None:
            self.set(n.inputs["Scale"], scale)
        out = "Value" if op in ("DOT_PRODUCT", "LENGTH", "DISTANCE") else "Vector"
        return n.outputs[out]

    def combine(self, x, y, z):
        n = self.node("ShaderNodeCombineXYZ")
        self.set(n.inputs[0], x)
        self.set(n.inputs[1], y)
        self.set(n.inputs[2], z)
        return n.outputs[0]

    # -- textures --------------------------------------------------------------------------------
    def noise(self, vector, scale, detail=4.0, rough=0.55, distortion=0.0, out="Fac", dims="3D"):
        n = self.node("ShaderNodeTexNoise", noise_dimensions=dims)
        self.set(n.inputs["Vector"], vector)
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        n.inputs["Distortion"].default_value = distortion
        return n.outputs[out]

    def voronoi(self, vector, scale, feature="F1", out="Distance", randomness=1.0):
        n = self.node("ShaderNodeTexVoronoi", feature=feature)
        self.set(n.inputs["Vector"], vector)
        n.inputs["Scale"].default_value = scale
        n.inputs["Randomness"].default_value = randomness
        return n.outputs[out]

    def wave(self, vector, scale, distortion, detail=3.0, direction="Z", profile="SIN"):
        n = self.node("ShaderNodeTexWave", wave_type="BANDS", bands_direction=direction, wave_profile=profile)
        self.set(n.inputs["Vector"], vector)
        n.inputs["Scale"].default_value = scale
        n.inputs["Distortion"].default_value = distortion
        n.inputs["Detail"].default_value = detail
        return n.outputs["Fac"]

    def mapping(self, vector, scale=(1, 1, 1), location=(0, 0, 0)):
        n = self.node("ShaderNodeMapping")
        self.set(n.inputs["Vector"], vector)
        n.inputs["Scale"].default_value = scale
        n.inputs["Location"].default_value = location
        return n.outputs[0]

    # -- shading helpers -------------------------------------------------------------------------
    def edge_mask(self, radius, gain):
        """Convex/concave edge detector: 1 - dot(bevelled normal, true normal), scaled."""
        bev = self.node("ShaderNodeBevel", samples=8)
        bev.inputs["Radius"].default_value = radius
        d = self.vec("DOT_PRODUCT", bev.outputs["Normal"], self.geometry("Normal"))
        return self.math("MULTIPLY", self.sub(1.0, d), gain, clamp=True)

    def ao(self, distance, only_local=True):
        n = self.node("ShaderNodeAmbientOcclusion", samples=16, only_local=only_local)
        n.inputs["Distance"].default_value = distance
        return n.outputs["AO"]

    def bump(self, height, strength, distance, normal=None):
        n = self.node("ShaderNodeBump")
        self.set(n.inputs["Height"], height)
        n.inputs["Strength"].default_value = strength
        n.inputs["Distance"].default_value = distance
        if normal is not None:
            self.set(n.inputs["Normal"], normal)
        return n.outputs["Normal"]

    def principled(self, base, metallic, roughness, normal=None):
        p = self.node("ShaderNodeBsdfPrincipled", name="BSDF")
        self.set(p.inputs["Base Color"], base)
        self.set(p.inputs["Metallic"], metallic)
        self.set(p.inputs["Roughness"], roughness)
        self.set(p.inputs["Normal"], normal)
        out = self.node("ShaderNodeOutputMaterial", name="OUT")
        self.nt.links.new(p.outputs["BSDF"], out.inputs["Surface"])
        return p


def new_material(name):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    if m.node_tree is None:
        m.use_nodes = True
    return m

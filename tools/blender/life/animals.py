"""Delhi's street animals: an Indian street dog and a humped white cow, from Quaternius's Ultimate
Animated Animals (CC0, see docs/references.md), reworked from flat low-poly to something that reads
as a real animal at street distance.

The files are in build/life_src/ua (raw downloads, not in git). For each animal this script:

- welds the flat-shaded faces and smooths them, so the planes go and the form stays;
- reshapes the body in its rest pose, which the skin follows: a lean pariah dog from the Shiba Inu,
  a zebu from the cow, with its hump, dewlap, narrow head and long thin legs;
- paints each coat from the model's own colour regions, softened, with hair streaks, dirt on the
  legs and ambient occlusion baked in, into a texture of its own;
- exports the mesh, the skin and the clips the game uses as assets/models/life/<name>.glb, with the
  other coats beside it as <name>_coat_<k>.png.

crates/game/src/life bakes these into flip books at load, the same way as the people.

blender -b --factory-startup -P tools/blender/life/animals.py [-- dog cow] [--preview DIR]
"""

import math
import os
import sys

import bmesh
import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
SRC = os.path.join(ROOT, "build", "life_src", "ua")
OUT = os.path.join(ROOT, "assets", "models", "life")
SIDE = 512


def smooth(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def soft(v, centre, radius):
    """How much a point at `v` feels a push centred at `centre`, 1 there and 0 past `radius`
    (a Vector per axis stretches it)."""
    d = Vector(((v.x - centre[0]) / radius[0], (v.y - centre[1]) / radius[1], (v.z - centre[2]) / radius[2])).length
    return 1 - smooth(0.0, 1.0, d)


def weld(mesh):
    """One smooth skin: the flat faces' split corners joined. Each face keeps its material."""
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.002)
    bm.to_mesh(mesh)
    bm.free()


def region_colours(obj, colours):
    """A colour attribute holding each face's material as a colour, averaged at shared corners and
    blurred along the mesh, so regions fade into each other as a real coat does."""
    me = obj.data
    n = len(me.vertices)
    sums = [Vector((0.0, 0.0, 0.0)) for _ in range(n)]
    hits = [0] * n
    for p in me.polygons:
        c = Vector(colours[obj.material_slots[p.material_index].name])
        for v in p.vertices:
            sums[v] += c * p.area ** 0.5
            hits[v] += p.area ** 0.5
    col = [s / max(h, 1e-6) for s, h in zip(sums, hits)]
    links = [set() for _ in range(n)]
    for e in me.edges:
        links[e.vertices[0]].add(e.vertices[1])
        links[e.vertices[1]].add(e.vertices[0])
    keep = [obj.material_slots[p.material_index].name in SHARP for p in me.polygons]
    fixed = set()
    for p, k in zip(me.polygons, keep):
        if k:
            fixed.update(p.vertices)
    for _ in range(2):
        col = [c if k in fixed or not links[k] else (c * 0.4 + sum((col[j] for j in links[k]), Vector((0, 0, 0))) / len(links[k]) * 0.6) for k, c in enumerate(col)]
    attr = me.color_attributes.new("region", "FLOAT_COLOR", "POINT")
    for k, c in enumerate(col):
        attr.data[k].color = (c.x, c.y, c.z, 1.0)


EYES = {"Eye_Black", "Eye_White", "Eyes_Black", "Eyes_White", "Eyes_Pupil"}
# Hard edges in life: these keep their colour to the line.
SHARP = EYES | {"Hooves", "Muzzle", "Horns", "Black"}


def coat_material(name, params):
    """Emission of the coat: regions, big soft patches, hair streaks running along the body, dust
    on the legs, the occlusion of the body on itself."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(em.outputs[0], out.inputs["Surface"])
    region = nt.nodes.new("ShaderNodeVertexColor")
    region.layer_name = "region"
    tc = nt.nodes.new("ShaderNodeTexCoord")

    def noise(scale, detail, stretch=(1, 1, 1), rough=0.55):
        map_ = nt.nodes.new("ShaderNodeMapping")
        map_.inputs["Scale"].default_value = stretch
        nt.links.new(tc.outputs["Object"], map_.inputs["Vector"])
        tex = nt.nodes.new("ShaderNodeTexNoise")
        tex.inputs["Scale"].default_value = scale
        tex.inputs["Detail"].default_value = detail
        tex.inputs["Roughness"].default_value = rough
        nt.links.new(map_.outputs[0], tex.inputs["Vector"])
        return tex.outputs[0]

    def mathn(op, a, b):
        n = nt.nodes.new("ShaderNodeMath")
        n.operation = op
        for i, v in enumerate((a, b)):
            if isinstance(v, (int, float)):
                n.inputs[i].default_value = v
            else:
                nt.links.new(v, n.inputs[i])
        return n.outputs[0]

    def mix(mode, a, b, fac=1.0):
        n = nt.nodes.new("ShaderNodeMix")
        n.data_type = "RGBA"
        n.blend_type = mode
        for sock, v in ((n.inputs["A"], a), (n.inputs["B"], b)):
            if isinstance(v, (tuple, list)):
                sock.default_value = tuple(v) + (1.0,) if len(v) == 3 else tuple(v)
            else:
                nt.links.new(v, sock)
        if isinstance(fac, (int, float)):
            n.inputs[0].default_value = fac
        else:
            nt.links.new(fac, n.inputs[0])
        return n.outputs[2]

    # Patches of a second colour, a mottled coat's big shapes.
    patch = noise(params["patch_scale"], 4)
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].position = params["patch_at"]
    ramp.color_ramp.elements[1].position = params["patch_at"] + 0.06
    nt.links.new(patch, ramp.inputs[0])
    col = mix("MIX", region.outputs[0], params["patch"], mathn("MULTIPLY", ramp.outputs[0], params["patch_share"]))
    # Hair: fine streaks along the body, and darker tips where it clumps.
    streak = noise(params["hair"], 6, (1.0, 0.16, 1.0))
    fine = mathn("ADD", mathn("MULTIPLY", streak, 0.62), 0.69)
    clump = mathn("ADD", mathn("MULTIPLY", noise(params["hair"] * 0.2, 5), 0.5), 0.75)
    col = mix("MULTIPLY", col, mix("MULTIPLY", (1, 1, 1), (1, 1, 1)), 0.0)
    fac = nt.nodes.new("ShaderNodeCombineColor")
    for i in range(3):
        nt.links.new(mathn("MULTIPLY", fine, clump), fac.inputs[i])
    col = mix("MULTIPLY", col, fac.outputs[0], 1.0)
    # Street dust: legs and belly warmer and paler toward the ground.
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["Object"], sep.inputs[0])
    dust = mathn("MULTIPLY", mathn("SUBTRACT", 1.0, mathn("MULTIPLY", sep.outputs["Z"], params["dust_fall"])), params["dust"])
    dust = mathn("MULTIPLY", mathn("MAXIMUM", dust, 0.0), mathn("ADD", mathn("MULTIPLY", noise(6, 4), 1.0), 0.5))
    col = mix("MIX", col, params["dust_colour"], mathn("MINIMUM", dust, 0.7))
    # The body's shade on itself.
    ao = nt.nodes.new("ShaderNodeAmbientOcclusion")
    ao.samples = 16
    ao.inputs["Distance"].default_value = params["ao"]
    ao_f = mathn("ADD", 0.45, mathn("MULTIPLY", ao.outputs["AO"], 0.55))
    fac2 = nt.nodes.new("ShaderNodeCombineColor")
    for i in range(3):
        nt.links.new(ao_f, fac2.inputs[i])
    col = mix("MULTIPLY", col, fac2.outputs[0], 1.0)
    nt.links.new(col, em.inputs["Color"])
    return m


def bake_coat(obj, params, path):
    img = bpy.data.images.new(os.path.basename(path), SIDE, SIDE, alpha=False, float_buffer=False)
    img.colorspace_settings.name = "sRGB"
    mat = coat_material("coat", params)
    node = mat.node_tree.nodes.new("ShaderNodeTexImage")
    node.image = img
    mat.node_tree.nodes.active = node
    me = obj.data
    me.materials.clear()
    me.materials.append(mat)
    for p in me.polygons:
        p.material_index = 0
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 24
    scene.cycles.device = "CPU"
    bpy.ops.object.bake(type="EMIT", margin=6, margin_type="EXTEND", use_clear=True)
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()
    bpy.data.materials.remove(mat)
    return img


def final_material(img):
    m = bpy.data.materials.new("coat")
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    tex = m.node_tree.nodes.new("ShaderNodeTexImage")
    tex.image = img
    m.node_tree.links.new(tex.outputs[0], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.9
    return m


def smooth_skin(obj, passes, factor=0.5, keep=()):
    """Relaxes the low-poly planes, leaving the eyes and the like alone."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.faces.ensure_lookup_table()
    held = set()
    for f in bm.faces:
        if obj.material_slots[f.material_index].name in keep:
            held.update(f.verts)
    verts = [v for v in bm.verts if v not in held]
    for _ in range(passes):
        bmesh.ops.smooth_vert(bm, verts=verts, factor=factor, use_axis_x=True, use_axis_y=True, use_axis_z=True)
    bm.to_mesh(obj.data)
    bm.free()


def subdivide(obj, cuts=1):
    """One level of smooth subdivision through a modifier, which carries the skin weights."""
    m = obj.modifiers.new("sub", "SUBSURF")
    m.levels = cuts
    m.render_levels = cuts
    m.subdivision_type = "CATMULL_CLARK"
    # The armature modifier from the import must not be applied.
    obj.modifiers.move(obj.modifiers.find("sub"), 0)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier="sub")


def thin_to(obj, tris):
    r = tris / max(sum(len(p.vertices) - 2 for p in obj.data.polygons), 1)
    if r >= 1.0:
        return
    m = obj.modifiers.new("dec", "DECIMATE")
    m.ratio = r
    obj.modifiers.move(obj.modifiers.find("dec"), 0)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier="dec")


def reshape(obj, edit):
    for v in obj.data.vertices:
        edit(v.co)


def dog_shape(c):
    """A pariah: lean, deep-chested, tucked up at the loin, narrow long muzzle; the Shiba's cuteness
    off the head."""
    # Tuck the belly and narrow the waist.
    w = soft(c, (0, 0.35, 0.95), (1.0, 0.75, 0.75))
    c.z += 0.20 * w
    c.x *= 1.0 - 0.28 * soft(c, (0, 0.3, 1.5), (1.4, 0.8, 1.1))
    # A narrower head and a longer, thinner muzzle.
    head = soft(c, (0, -2.0, 2.7), (0.9, 0.7, 0.65))
    c.x *= 1.0 - 0.22 * head
    muzzle = soft(c, (0, -2.35, 2.48), (0.5, 0.4, 0.36))
    c.y -= 0.17 * muzzle
    c.z -= 0.03 * muzzle
    c.x *= 1.0 - 0.18 * muzzle
    # Thinner neck ruff.
    ruff = soft(c, (0, -1.3, 2.1), (1.2, 0.7, 0.95))
    c.x *= 1.0 - 0.18 * ruff
    # Long thin legs.
    if c.z < 1.1:
        for side in (-1, 1):
            for y in (-0.95, 0.95):
                pass
    low = 1 - smooth(0.5, 1.2, c.z)
    c.x = c.x * (1 - 0.0 * low)


def cow_shape(c):
    """A zebu: a muscled hump over the shoulders, a dewlap hanging under the neck, a lean flank
    with the hips showing, a narrow long head, thin legs."""
    # The hump, leaning back a little as a real one does.
    hump = soft(c, (0, -1.8, 4.15), (0.8, 0.8, 0.85))
    c.z += 0.62 * hump
    c.y += 0.16 * hump
    # The dewlap: the loose skin from throat to brisket.
    dew = soft(c, (0, -3.0, 2.25), (0.5, 1.5, 0.75))
    c.z -= 0.45 * dew
    c.x *= 1.0 - 0.45 * dew
    # Lean: narrow the barrel, tuck the flank, leave the hip bones standing.
    barrel = soft(c, (0, 0.3, 2.9), (1.6, 2.2, 1.6))
    c.x *= 1.0 - 0.26 * barrel
    flank = soft(c, (0, 1.0, 1.75), (1.2, 1.1, 0.7))
    c.z += 0.42 * flank
    hip = soft(c, (0, 1.85, 4.15), (1.3, 0.8, 0.6))
    c.z += 0.10 * hip
    # A narrow, long face.
    head = soft(c, (0, -4.55, 3.5), (1.1, 1.05, 1.2))
    c.x *= 1.0 - 0.24 * head
    nose = soft(c, (0, -4.95, 2.95), (0.7, 0.55, 0.6))
    c.y -= 0.25 * nose
    c.z -= 0.08 * nose
    # Thin legs below the knee.
    low = 1 - smooth(0.9, 2.2, c.z)
    for ax in ("x",):
        for mid in (-0.72, 0.72):
            if (c.x > 0) == (mid > 0):
                setattr(c, ax, mid + (getattr(c, ax) - mid) * (1 - 0.22 * low))


ANIMALS = {
    "dog": {
        "src": "ShibaInu.gltf",
        "shape": dog_shape,
        "smooth": 1,
        "tris": 2600,
        "clips": ["Idle", "Idle_2", "Idle_2_HeadLow", "Walk", "Gallop", "Eating", "Death"],
        "regions": {"Main": (0.30, 0.20, 0.10), "Main_Light": (0.62, 0.52, 0.38), "Black": (0.015, 0.012, 0.01), "Eyes_Black": (0.01, 0.01, 0.01), "Eyes_White": (0.04, 0.03, 0.02), "Eyes_Pupil": (0.01, 0.008, 0.006)},
        "coats": [
            {},
            {"Main": (0.018, 0.016, 0.014), "Main_Light": (0.20, 0.13, 0.07), "patch": (0.02, 0.017, 0.015), "patch_share": 0.3},
            {"Main": (0.60, 0.53, 0.42), "Main_Light": (0.70, 0.64, 0.54), "patch": (0.13, 0.08, 0.04), "patch_share": 1.0},
            {"Main": (0.16, 0.085, 0.04), "Main_Light": (0.40, 0.29, 0.18), "patch": (0.08, 0.04, 0.02), "patch_share": 0.4},
        ],
        "base": {"patch": (0.20, 0.13, 0.07), "patch_scale": 0.9, "patch_at": 0.56, "patch_share": 0.5, "hair": 34.0, "dust": 0.7, "dust_fall": 1.3, "dust_colour": (0.34, 0.27, 0.19), "ao": 0.7},
    },
    "cow": {
        "src": "Cow.gltf",
        "shape": cow_shape,
        "smooth": 1,
        "tris": 3400,
        "clips": ["Idle", "Idle_2", "Idle_Headlow", "Walk", "Gallop", "Eating", "Death"],
        "regions": {"Main": (0.44, 0.43, 0.40), "Main_Light": (0.52, 0.51, 0.47), "Hooves": (0.012, 0.011, 0.01), "Muzzle": (0.03, 0.028, 0.028), "Eye_Black": (0.006, 0.006, 0.006), "Eye_White": (0.02, 0.02, 0.02), "Horns": (0.07, 0.06, 0.05)},
        "coats": [
            {},
            {"Main": (0.20, 0.20, 0.20), "Main_Light": (0.30, 0.30, 0.29), "patch": (0.10, 0.10, 0.10), "patch_share": 0.5},
            {"Main": (0.40, 0.33, 0.25), "Main_Light": (0.50, 0.44, 0.35), "patch": (0.20, 0.13, 0.08), "patch_share": 0.5},
        ],
        "base": {"patch": (0.24, 0.23, 0.21), "patch_scale": 0.42, "patch_at": 0.5, "patch_share": 0.5, "hair": 26.0, "dust": 0.8, "dust_fall": 0.55, "dust_colour": (0.26, 0.21, 0.15), "ao": 1.3},
    },
}


def build(name, preview):
    a = ANIMALS[name]
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.join(SRC, a["src"]))
    scene = bpy.context.scene
    arm = next(o for o in scene.objects if o.type == "ARMATURE")
    obj = next(o for o in scene.objects if o.type == "MESH" and o.parent == arm)
    for o in list(scene.objects):
        if o not in (arm, obj):
            bpy.data.objects.remove(o)
    # Only the clips the game shows.
    arm.animation_data.action = None
    for t in list(arm.animation_data.nla_tracks):
        arm.animation_data.nla_tracks.remove(t)
    for act in list(bpy.data.actions):
        if act.name not in a["clips"]:
            bpy.data.actions.remove(act)
    for act in bpy.data.actions:
        act.use_fake_user = True

    weld(obj.data)
    reshape(obj, a["shape"])
    smooth_skin(obj, a["smooth"], keep=EYES)
    if a.get("subdivide"):
        subdivide(obj)
        thin_to(obj, a["tris"])
    for p in obj.data.polygons:
        p.use_smooth = True
    os.makedirs(OUT, exist_ok=True)

    # Unwrap, then paint each coat.
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(55), island_margin=0.012)
    bpy.ops.object.mode_set(mode="OBJECT")
    slots = [s.name for s in obj.material_slots]
    faces = [p.material_index for p in obj.data.polygons]
    first = None
    for k, coat in enumerate(a["coats"]):
        # The material list is rebuilt by each bake; put the regions back to read them.
        obj.data.materials.clear()
        for s in slots:
            m = bpy.data.materials.get(s) or bpy.data.materials.new(s)
            obj.data.materials.append(m)
        for p, i in zip(obj.data.polygons, faces):
            p.material_index = i
        colours = {**a["regions"], **{r: c for r, c in coat.items() if r in a["regions"]}}
        if "region" in obj.data.color_attributes:
            obj.data.color_attributes.remove(obj.data.color_attributes["region"])
        region_colours(obj, colours)
        params = {**a["base"], **{r: c for r, c in coat.items() if r not in a["regions"]}}
        path = os.path.join(OUT, f"{name}_coat_{k}.png")
        img = bake_coat(obj, params, path)
        if k == 0:
            first = img
    obj.data.materials.clear()
    obj.data.materials.append(final_material(first))
    for p in obj.data.polygons:
        p.material_index = 0
    obj.data.color_attributes.remove(obj.data.color_attributes["region"])
    tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)

    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    arm.select_set(True)
    out = os.path.join(OUT, f"{name}.glb")
    bpy.ops.export_scene.gltf(filepath=out, export_format="GLB", use_selection=True, export_yup=True, export_apply=False, export_normals=True,
                              export_texcoords=True, export_tangents=False, export_materials="EXPORT", export_image_format="AUTO", export_skins=True,
                              export_animations=True, export_animation_mode="ACTIONS", export_force_sampling=True, export_frame_step=2, export_morph=False,
                              export_all_influences=False, export_def_bones=False, export_vertex_color="NONE")
    print(f"[animals] {out}: {tris} tris, {len(obj.data.vertices)} verts, {os.path.getsize(out) // 1024} KB")
    if preview:
        render(name, arm, obj, preview, a)


def render(name, arm, obj, out, a):
    scene = bpy.context.scene
    os.makedirs(out, exist_ok=True)
    act = bpy.data.actions.get("Walk")
    arm.animation_data_create()
    arm.animation_data.action = act
    if arm.animation_data.action_slot is None and len(act.slots):
        arm.animation_data.action_slot = act.slots[0]
    scene.frame_set(8)
    lo = Vector((1e9,) * 3)
    hi = Vector((-1e9,) * 3)
    dg = bpy.context.evaluated_depsgraph_get()
    em = obj.evaluated_get(dg).to_mesh()
    for v in em.vertices:
        p = obj.matrix_world @ v.co
        for k in range(3):
            lo[k] = min(lo[k], p[k])
            hi[k] = max(hi[k], p[k])
    mid = (lo + hi) / 2
    size = max(hi - lo)
    bpy.ops.mesh.primitive_plane_add(size=size * 8, location=(0, 0, lo.z))
    g = bpy.context.active_object
    gm = bpy.data.materials.new("g")
    gm.use_nodes = True
    gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.26, 0.25, 0.24, 1)
    g.data.materials.append(gm)
    w = bpy.data.worlds.new("w")
    scene.world = w
    w.use_nodes = True
    w.node_tree.nodes["Background"].inputs[0].default_value = (0.75, 0.78, 0.82, 1)
    w.node_tree.nodes["Background"].inputs[1].default_value = 0.8
    sun = bpy.data.lights.new("s", "SUN")
    sun.energy = 3.5
    so = bpy.data.objects.new("s", sun)
    so.rotation_euler = (0.9, 0.2, 0.8)
    scene.collection.objects.link(so)
    for k, off in enumerate(((1.0, -1.5, 0.35), (2.0, 0.0, 0.25), (-1.0, -1.2, 0.1))):
        cam = bpy.data.cameras.new("c")
        co = bpy.data.objects.new("c", cam)
        scene.collection.objects.link(co)
        co.location = mid + Vector(off) * size * 0.8
        co.rotation_euler = (mid - co.location).to_track_quat("-Z", "Y").to_euler()
        cam.lens = 45
        scene.camera = co
        scene.render.engine = "CYCLES"
        scene.cycles.samples = 16
        scene.cycles.device = "CPU"
        scene.cycles.use_denoising = False
        scene.view_settings.view_transform = "AgX"
        scene.render.resolution_x, scene.render.resolution_y = 640, 420
        scene.render.filepath = os.path.join(out, f"{name}_{k}.png")
        bpy.ops.render.render(write_still=True)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    preview = None
    if "--preview" in argv:
        i = argv.index("--preview")
        preview = argv[i + 1]
        argv = argv[:i] + argv[i + 2:]
    for name in argv or list(ANIMALS):
        build(name, preview)


main()

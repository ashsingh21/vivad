"""The modern and old-Indian long guns, made from free models on disk and a few lofted parts: Ladi-9
and Teen Tigada (Flat Guns East, CC0), Thaana SLR (Flat Guns West, CC0, in wood), Saintalis (AK by
loafbrr_1, CC0), Thri-Not-Thri (Lee Enfield by Lucian Pavel, CC0) and the Toradar (lofted here: no
free matchlock was on disk). Sources are in research/sniper-guns/models (see docs/references.md).

Axes as in desi.py: barrel Blender +Y, up +Z, origin where the hand closes, about 0.4 m of stock
behind it (the rifle_aim clip puts the butt in the shoulder there).

blender -b --factory-startup -P tools/blender/weapons/street_guns.py [-- name ...]
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import arsenal as A  # noqa: E402
import bake  # noqa: E402
import desi as D  # noqa: E402
import export  # noqa: E402
import finishes as F  # noqa: E402
import geo  # noqa: E402
from nodes import new_material  # noqa: E402

X, Y, Z = Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1))
SRC = os.path.join(A.ROOT, "research", "sniper-guns", "models")
FLAT_EAST = os.path.join(SRC, "oga_flat_family", "east", "Flat Guns East", "GLB")
FLAT_WEST = os.path.join(SRC, "oga_flat_guns_west", "unzipped_flat_guns_west", "Flat Guns West", "GLB")

GUNMETAL = D.GUNMETAL
RUST = D.RUST


def only_meshes(keep=None):
    """Drop what the import brought besides meshes; apply each mesh's transform."""
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH" and (keep is None or keep(o))]
    for o in list(bpy.context.scene.objects):
        if o not in meshes:
            bpy.data.objects.remove(o, do_unlink=True)
    for o in meshes:
        for m in list(o.modifiers):
            o.modifiers.remove(m)
        o.data.transform(o.matrix_world)
        o.matrix_world = Matrix.Identity(4)
        o.parent = None
    return meshes


def place(obj, rot, scale, grip):
    """Turn and scale a model, then move `grip` (in its turned, scaled frame) to the origin."""
    obj.data.transform(Matrix.Translation(-grip) @ Matrix.Scale(scale, 4) @ rot)


def finish(name, parts, res=1024, keep_uv=None):
    """Like arsenal.finish, for parts that may read a source texture through `keep_uv`."""
    lod0 = geo.join(parts, f"{name}_LOD0")
    export.pack_uvs(lod0)
    bake.setup_cycles()
    images = bake.bake_texture_set(lod0, os.path.join(A.ROOT, "build", "textures", name), name, res=res)
    lod0.data.materials.clear()
    lod0.data.materials.append(export.game_material(name, images))
    export.strip_bake_data(lod0)
    if keep_uv and keep_uv in lod0.data.uv_layers:
        lod0.data.uv_layers.remove(lod0.data.uv_layers[keep_uv])
    export.triangulate(lod0)
    lods = export.make_lods(lod0, (0.35,))
    counts = export.export_glb(lods, os.path.join(A.OUT, f"{name}.glb"))
    print(f"[{name}] exported tris: {counts}")


# ------------------------------------------------------------------ the scope, taped on

def scope(prefix, y, z, foot, length, tube, bell, mats):
    """A telescope along Y centred at (y, z) above the gun, on two clamps down to height `foot`,
    with turns of black tape round it: jugaad, not a factory mount."""
    steel, tape, glass = mats
    L = length / 2
    o = Vector((0, y, z))
    parts = [A.lathe(f"{prefix}tube", [(0.0, -L - 0.004), (bell * 0.82, -L - 0.004), (bell, -L), (bell, -L + 0.022), (tube, -L + 0.05),
                                       (tube, L - 0.075), (bell * 1.25, L - 0.012), (bell * 1.3, L), (bell * 1.1, L + 0.002), (0.0, L + 0.002)],
                     steel, seg=16, axis=Y, origin=o)]
    parts.append(A.lathe(f"{prefix}lens", [(0.0, L + 0.0022), (bell * 1.05, L + 0.0022)], glass, seg=16, axis=Y, origin=o))
    parts.append(A.lathe(f"{prefix}knob", [(0.0, tube + 0.016), (0.009, tube + 0.015), (0.0095, tube), (0.0, tube)], steel, seg=10, axis=Z, origin=o))
    for k, s in enumerate((-0.42, 0.34)):
        at = y + s * length
        parts.append(A.lathe(f"{prefix}wrap{k}", [(tube + 0.0005, -0.016), (tube + 0.003, -0.014), (tube + 0.003, 0.014), (tube + 0.0005, 0.016)],
                             tape, seg=14, axis=Y, origin=Vector((0, at, z)), extra={"kv": lambda *a: 0.0}))
        parts.append(D.box(f"{prefix}clamp{k}", (-0.009, at - 0.011, foot), (0.009, at + 0.011, z - tube + 0.002), steel))
    return parts


def scope_mats(name):
    return (F.metal(f"{name}_scope", (0.05, 0.05, 0.055), rough=0.35, tarnish=RUST), F.wrap(f"{name}_tape", (0.03, 0.03, 0.03), (0.07, 0.07, 0.07), pitch=0.013),
            F.plain(f"{name}_lens", (0.10, 0.22, 0.34), rough=0.08))


# ------------------------------------------------------------------ Flat Guns East

def flat_gun(name, file, length, wood=False, scope_at=None):
    """A rigged Flat Guns model: frozen in its rest pose, flat colours replaced by the armoury's
    steel, polymer and wood, unwrapped, baked. The rig's bones mark the trigger and the muzzle.
    `wood` turns the stock, grip and fore-end to wood: (fore-end start, fore-end end) along Y."""
    bpy.ops.import_scene.gltf(filepath=file)
    arm = next(o for o in bpy.context.scene.objects if o.type == "ARMATURE")
    head = lambda b: arm.matrix_world @ arm.data.bones[b].head_local
    trigger, muzzle, top = head("Trigger"), head("Attach_Muzzle"), head("Attach_Scope")
    meshes = only_meshes(lambda o: not o.name.startswith("Icosphere"))
    gun = geo.join(meshes, f"{name}_src") if len(meshes) > 1 else meshes[0]
    # Barrel to +Y: a half turn about Z when the model points the other way.
    rot = Matrix.Identity(4) if (muzzle - trigger).y > 0 else Matrix.Rotation(math.pi, 4, "Z")
    ys = [(rot @ v.co).y for v in gun.data.vertices]
    k = length / (max(ys) - min(ys))
    grip = (rot @ trigger) * k + Vector((0, -0.05, -0.05))
    place(gun, rot, k, grip)
    top = (rot @ top) * k - grip
    steel = F.metal(f"{name}_steel", (0.12, 0.12, 0.13), rough=0.42, tarnish=RUST)
    dark = F.metal(f"{name}_dark", (0.05, 0.05, 0.055), rough=0.55, tarnish=(0.04, 0.03, 0.03))
    poly = F.plain(f"{name}_poly", (0.03, 0.03, 0.035), rough=0.7)
    bare = F.metal(f"{name}_bare", GUNMETAL, rough=0.35, tarnish=RUST)
    tree = F.plain(f"{name}_wood", (0.24, 0.105, 0.045), rough=0.5)
    chosen = {"Black": poly, "Lighter": dark, "Darker": dark, "Dark": dark, "Light": steel, "White": bare, "Bole": tree}
    me = gun.data
    tags = []
    for i, slot in enumerate(gun.material_slots):
        tag = next((t for t in ("Lighter", "Darker", "Bole", "White", "Light", "Dark", "Black") if t in slot.material.name), "Dark")
        tags.append(tag)
        me.materials[i] = chosen[tag]
    if wood:
        slot = len(me.materials)
        me.materials.append(tree)
        stock = gun.vertex_groups.get("Stock")
        for p in me.polygons:
            if tags[p.material_index] not in ("Black", "Lighter", "Darker", "Dark"):
                continue
            c = p.center
            in_stock = stock is not None and all(any(g.group == stock.index and g.weight > 0.5 for g in me.vertices[i].groups) for i in p.vertices)
            if in_stock or (wood[0] < c.y < wood[1] and c.z < (rot @ top).z * k - grip.z - 0.03) or (-0.12 < c.y < 0.05 and c.z < -0.005):
                p.material_index = slot
    # The pack has no UVs: unwrap, then the common packer sets the islands.
    me.uv_layers.new(name="UVMap")
    bpy.context.view_layer.objects.active = gun
    gun.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(50), island_margin=0.01, scale_to_bounds=False)
    bpy.ops.object.mode_set(mode="OBJECT")
    gun.select_set(False)
    parts = [gun]
    if scope_at:
        length_s, tube, bell, lift, shift = scope_at
        parts += scope("s_", top.y + shift, top.z + lift, top.z - 0.012, length_s, tube, bell, scope_mats(name))
    finish(name, parts)


# ------------------------------------------------------------------ models with their own textures

def textured(mat_name, base, orm, normal):
    """A bakeable material from a model's own images, read through the `src_uv` layer."""
    m = new_material(mat_name)
    nt = m.node_tree
    nt.nodes.clear()
    bsdf = nt.nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.name = "BSDF"
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    out.name = "OUT"
    nt.links.new(bsdf.outputs["BSDF"], out.inputs["Surface"])

    def tex(img, non_color):
        n = nt.nodes.new("ShaderNodeTexImage")
        n.image = bpy.data.images.load(img, check_existing=True)
        n.image.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
        uv = nt.nodes.new("ShaderNodeUVMap")
        uv.uv_map = "src_uv"
        nt.links.new(uv.outputs["UV"], n.inputs["Vector"])
        return n

    nt.links.new(tex(base, False).outputs["Color"], bsdf.inputs["Base Color"])
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(tex(orm, True).outputs["Color"], sep.inputs["Color"])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nt.links.new(sep.outputs["Blue"], bsdf.inputs["Metallic"])
    nmap = nt.nodes.new("ShaderNodeNormalMap")
    nmap.uv_map = "src_uv"
    nt.links.new(tex(normal, True).outputs["Color"], nmap.inputs["Color"])
    nt.links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def keep_source_uv(obj):
    """The model's own UVs stay for sampling its textures; 'UVMap' becomes the one to pack."""
    me = obj.data
    layer = me.uv_layers[0]
    layer.name = "src_uv"
    new = me.uv_layers.new(name="UVMap")
    for a, b in zip(layer.data, new.data):
        b.uv = a.uv
    me.uv_layers.active = new
    me.uv_layers["src_uv"].active_render = False
    new.active_render = True


def drop_islands(obj, unwanted):
    """Delete the mesh islands `unwanted(centre, size)` picks (loose bullets and the like)."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    seen, gone = set(), []
    for v in bm.verts:
        if v in seen:
            continue
        stack, island = [v], []
        seen.add(v)
        while stack:
            q = stack.pop()
            island.append(q)
            for e in q.link_edges:
                o = e.other_vert(q)
                if o not in seen:
                    seen.add(o)
                    stack.append(o)
        lo = Vector((min(p.co.x for p in island), min(p.co.y for p in island), min(p.co.z for p in island)))
        hi = Vector((max(p.co.x for p in island), max(p.co.y for p in island), max(p.co.z for p in island)))
        if unwanted((lo + hi) / 2, hi - lo):
            gone += island
    bmesh.ops.delete(bm, geom=gone, context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()


def save_map(name, rgb, non_color):
    """Write an HxWx3 array (display values) where the materials above can load it."""
    h, w = rgb.shape[:2]
    im = bake.new_image(name, w, non_color=non_color)
    im.pixels.foreach_set(np.dstack([rgb, np.ones((h, w))]).astype(np.float32).ravel())
    path = os.path.join(A.ROOT, "build", "textures", "street_guns", f"{name}.png")
    bake.save(im, path)
    return path


def pixels(path):
    im = bpy.data.images.load(path, check_existing=True)
    return bake.pixels(im)[..., :3]


def lee_maps(folder):
    """The Lee Enfield comes as diffuse and specular: metal where the colour is grey, its colour
    taken from the specular map; roughness from the specular map; AO flat. Returns base and ORM."""
    dif = pixels(os.path.join(folder, "FP Hunting Rifle.png"))
    spc = pixels(os.path.join(folder, "FP Hunting Rifle SPEC.png"))
    sat = dif.max(axis=2) - dif.min(axis=2)
    metal = np.clip((0.10 - sat) / 0.05, 0.0, 1.0)
    s = spc.mean(axis=2)
    base = dif * (1.0 - metal[..., None]) + np.clip(spc * 1.1 + dif * 0.5, 0.0, 1.0) * metal[..., None]
    orm = np.stack([np.ones_like(s), np.clip(0.78 - 0.75 * s, 0.22, 0.85), metal], axis=2)
    return save_map("lee_base", base, False), save_map("lee_orm", orm, True)


def thri_not_thri():
    folder = os.path.join(SRC, "oga_lucian_lee_enfield", "unzipped_Lee_Enfield")
    bpy.ops.wm.fbx_import(filepath=os.path.join(folder, "FP Hunting Rifle.fbx"))
    gun = only_meshes()[0]
    gun.name = "lee"
    # The file holds a cartridge lying under the rifle, in pieces; muzzle along -X, 9.3 units long.
    drop_islands(gun, lambda c, s: abs(c.z + 0.37) < 0.02 and -0.4 < c.x < 0.05)
    rot = Matrix.Rotation(-math.pi / 2, 4, "Z")
    ys = [(rot @ v.co).y for v in gun.data.vertices]
    k = 1.13 / (max(ys) - min(ys))
    # The hand closes on the wrist of the stock, just behind the trigger guard.
    place(gun, rot, k, Vector((0, -1.85, -0.05)) * k)
    keep_source_uv(gun)
    base, orm = lee_maps(folder)
    gun.data.materials.clear()
    gun.data.materials.append(textured("lee", base, orm, os.path.join(folder, "FP Hunting Rifle NRM.png")))
    finish("thri_not_thri", [gun] + scope("s_", 0.115, 0.086, 0.045, 0.27, 0.0125, 0.021, scope_mats("thri")), keep_uv="src_uv")


def saintalis():
    folder = os.path.join(SRC, "oga_loafbrr_ak", "unzipped_ak_0")
    bpy.ops.import_scene.gltf(filepath=os.path.join(folder, "GLTF", "AK.gltf"))
    meshes = only_meshes(lambda o: o.name in ("AK", "Bolt", "Lock", "Trigger", "Magazine.001"))

    def skin(kind):
        tex = lambda m: os.path.join(folder, "Textures", kind, f"{kind}_{m}.png")
        rough, metal = pixels(tex("Roughness")), pixels(tex("Metalness"))
        orm = np.stack([np.ones_like(rough[..., 0]), rough[..., 0], metal[..., 0]], axis=2)
        return textured(f"ak_{kind}", tex("Albedo"), save_map(f"ak_{kind}_orm", orm, True), tex("Normal"))

    body, mag = skin("AK_2"), skin("Ammunition")
    for o in meshes:
        keep_source_uv(o)
        o.data.materials.clear()
        o.data.materials.append(mag if o.name.startswith("Magazine") else body)
    gun = geo.join(meshes, "ak")
    place(gun, Matrix.Rotation(math.pi / 2, 4, "Z"), 1.0, Vector((0, 0.03, 0.03)))
    finish("saintalis", [gun], keep_uv="src_uv")


def toradar():
    """The Mughal matchlock: a long eight-sided barrel in brass bands, a slim lacquered stock with
    gold vines, a pan and serpentine; a modern 6x scope taped on, the joke of the gun."""
    steel = F.metal("toradar_steel", (0.18, 0.17, 0.17), rough=0.4, tarnish=RUST)
    brass = F.metal("toradar_brass", A.GOLD, rough=0.3, chase=True)
    lac = F.lacquer("toradar_lacquer", base=(0.30, 0.04, 0.03))
    axis = 0.03
    parts = [A.lathe("barrel", [(0.0, -0.06), (0.0135, -0.06), (0.0125, 0.45), (0.0105, 0.92), (0.0115, 0.965), (0.0165, 0.995), (0.0165, 1.015),
                                (0.0085, 1.016), (0.0085, 1.0), (0.0, 1.0)], steel, seg=8, axis=Y, origin=Vector((0, 0, axis)))]
    S, seg = 14, 12
    frames, rings = [], []
    ys = [-0.46, -0.40, -0.30, -0.20, -0.11, -0.04, 0.02, 0.10, 0.25, 0.40, 0.55, 0.70, 0.82, 0.86]
    for s, y in enumerate(ys):
        u = (y + 0.46) / 1.32
        if y < -0.04:
            f = (-0.04 - y) / 0.42
            top, bottom, hw = axis - 0.012 - 0.05 * f, axis - 0.035 - 0.115 * f ** 0.9, 0.015 + 0.012 * f
        else:
            top, bottom, hw = axis - 0.006, axis - 0.03 + 0.012 * min(1.0, (y + 0.04) / 0.5), 0.0155 - 0.003 * u
        frames.append((Vector((0, y, (top + bottom) / 2)), X.copy(), Z.copy()))
        rings.append(geo.ellipse(hw, (top - bottom) / 2, seg))
    parts.append(geo.loft("stock", frames, rings, cap_start="fan", cap_end="fan", material=lac, merge=True, corner_attrs={
        "al": lambda si, ri, c: A.along(c, si, ri), "ac": A.ring_ac(seg), "hw": lambda si, ri, c: 0.02, "kv": lambda *a: 1.0}))
    for k, y in enumerate((0.13, 0.32, 0.5, 0.68, 0.84)):
        parts.append(A.lathe(f"band{k}", [(0.0142, y - 0.009), (0.0162, y - 0.007), (0.0162, y + 0.007), (0.0142, y + 0.009)], brass, seg=8, axis=Y, origin=Vector((0, 0, axis))))
    parts.append(A.lathe("butt", [(0.0, -0.468), (0.022, -0.466), (0.03, -0.46), (0.03, -0.455)], brass, seg=8, axis=Y, origin=Vector((0, 0, axis - 0.12))))
    parts.append(D.box("pan", (0.012, 0.0, axis - 0.006), (0.032, 0.03, axis + 0.004), brass))
    parts.append(D.bent("serpentine", [(0.024, -0.05, axis - 0.03), (0.026, -0.03, axis + 0.018), (0.024, 0.002, axis + 0.026), (0.024, 0.012, axis + 0.01)], 0.003, steel))
    parts.append(D.bent("guard", [(0, 0.03, axis - 0.03), (0, 0.022, axis - 0.052), (0, -0.012, axis - 0.052), (0, -0.018, axis - 0.03)], 0.0022, brass))
    parts += scope("s_", 0.14, axis + 0.058, axis + 0.009, 0.34, 0.0135, 0.024, scope_mats("toradar"))
    A.finish("toradar", parts, res=1024, hard=("pan",))


BUILDERS = {
    "vajra9": lambda: flat_gun("vajra9", os.path.join(FLAT_EAST, "SMG_Full_East.glb"), 0.62),
    "trishul_carbine": lambda: flat_gun("trishul_carbine", os.path.join(FLAT_EAST, "Rifle_Assault_East.glb"), 0.84),
    "thaana_slr": lambda: flat_gun("thaana_slr", os.path.join(FLAT_WEST, "Rifle_Battle_West.glb"), 1.02, wood=(0.17, 0.47), scope_at=(0.15, 0.0125, 0.02, 0.05, -0.03)),
    "thri_not_thri": thri_not_thri,
    "saintalis": saintalis,
    "toradar": toradar,
}


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else list(BUILDERS)
    for name in names:
        export.reset_scene()
        BUILDERS[name]()


if __name__ == "__main__":
    main()

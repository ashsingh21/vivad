"""The guns from MoCap Online's Rifle Basic pack (tools/blender/characters/mocaponline, gitignored:
its licence), the ones its rifle animations were captured holding: the rifle as the bandook, the
pistol as the tamancha. Imported as they are, their own textures cut to 1024 for phones, and
exported like the rest of the armoury (arsenal.py): one mesh, one texture set, LOD0 + LOD1.

Each model's origin is the pack's weapon bone in the right hand, the grip the animations close on.
Turned to the armoury's axes (docs/references.md): the barrel along Blender +Y (glTF -Z), up +Z.

blender -b --factory-startup -P tools/blender/weapons/pack_guns.py [-- name ...]
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402
from mathutils import Matrix  # noqa: E402

import export  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
PACK = os.path.join(ROOT, "tools", "blender", "characters", "mocaponline", "FBX_Rifle_Basic_27A2")
OUT = os.path.join(ROOT, "assets", "models", "weapons")
RES = 1024

# Our name: (the pack's file, its textures: base colour, ambient occlusion, normal, specular (None:
# not in the pack), its metalness).
GUNS = {
    "bandook": ("M4/M4_Rifle_01.fbx", ("M4DF2048.jpg", "M4AO2048.jpg", "M4NM2048.jpg", "M4SP2048.jpg"), 0.6),
    "tamancha": ("Pistol/1911_01.fbx", ("1911.jpg", None, None, None), 0.6),
}


def image(path, name, colour):
    """The pack's texture cut to RES (or a flat stand-in when it has none)."""
    srgb = name.endswith("basecolor")
    size = RES if path and os.path.exists(path) else 4
    img = bpy.data.images.new(name, size, size, is_data=not srgb)
    # The colour space set before the pixels: setting it after clears a generated image.
    img.colorspace_settings.name = "sRGB" if srgb else "Non-Color"
    if size == RES:
        src = bpy.data.images.load(path)
        # Loaded lazily: read it before scaling, or the scale works on nothing.
        assert src.size[0] > 0 and len(src.pixels) > 0, path
        src.scale(RES, RES)
        img.pixels = src.pixels[:]
        bpy.data.images.remove(src)
    else:
        img.pixels = list(colour) * 16
    return img


def orm(name, ao, spec, metal):
    """Occlusion, roughness (rougher where the specular map is dark) and metalness in one image."""
    img = bpy.data.images.new(f"{name}_orm", RES, RES)
    img.colorspace_settings.name = "Non-Color"
    a = list(ao.pixels) if ao else None
    s = list(spec.pixels) if spec else None
    px = [0.0] * (RES * RES * 4)
    for i in range(RES * RES):
        k = i * 4
        px[k] = a[k] if a else 1.0
        px[k + 1] = (1.0 - 0.7 * s[k]) if s else 0.55
        px[k + 2] = metal
        px[k + 3] = 1.0
    img.pixels = px
    return img


def build(name):
    file, (df, ao, nm, sp), metal = GUNS[name]
    path = os.path.join(PACK, file)
    tex = os.path.splitext(path)[0] + ".fbm"
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.fbx(filepath=path)
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    for o in bpy.data.objects:
        if o.type != "MESH":
            bpy.data.objects.remove(o)
    bpy.ops.object.select_all(action="DESELECT")
    for o in meshes:
        o.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    gun = bpy.context.view_layer.objects.active
    gun.name = f"{name}_LOD0"
    # The pack's barrel runs -Y: half a turn about Z puts it along +Y, its origin (the grip) kept.
    gun.data.transform(Matrix.Rotation(3.14159265, 4, "Z") @ gun.matrix_world)
    gun.matrix_world = Matrix.Identity(4)
    lo = [min(v.co[i] for v in gun.data.vertices) for i in range(3)]
    hi = [max(v.co[i] for v in gun.data.vertices) for i in range(3)]
    print(f"[{name}] from {file}: {len(gun.data.polygons)} faces, x {lo[0]:+.3f}..{hi[0]:+.3f}, "
          f"y (barrel) {lo[1]:+.3f}..{hi[1]:+.3f}, z {lo[2]:+.3f}..{hi[2]:+.3f}")
    gun.data.uv_layers[0].name = "UVMap"
    while len(gun.data.uv_layers) > 1:
        gun.data.uv_layers.remove(gun.data.uv_layers[1])
    at = lambda f: os.path.join(tex, f) if f else None
    base = image(at(df), f"{name}_basecolor", (0.2, 0.2, 0.2, 1))
    occ = image(at(ao), f"{name}_ao", (1, 1, 1, 1)) if ao else None
    spec = image(at(sp), f"{name}_spec", (0.5, 0.5, 0.5, 1)) if sp else None
    normal = image(at(nm), f"{name}_normal", (0.5, 0.5, 1.0, 1))
    images = {"basecolor": base, "orm": orm(name, occ, spec, metal), "normal": normal}
    for img in images.values():
        img.filepath_raw = os.path.join(ROOT, "build", "textures", name, f"{img.name}.png")
        img.file_format = "PNG"
        os.makedirs(os.path.dirname(img.filepath_raw), exist_ok=True)
        img.save()
    gun.data.materials.clear()
    gun.data.materials.append(export.game_material(name, images))
    export.triangulate(gun)
    lods = export.make_lods(gun, (0.35,))
    counts = export.export_glb(lods, os.path.join(OUT, f"{name}.glb"))
    print(f"[{name}] exported tris: {counts}")


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else list(GUNS)
    for n in names:
        build(n)


main()

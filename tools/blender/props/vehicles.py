"""The drivable vehicles: free models from research/vehicles/models, made ready for the game.

Each model is turned to face +Y (the game's -Z), stood on the ground, scaled to its real length,
cleared of number plates and lettering, reduced if heavy, and written to
assets/models/vehicles/<name>.glb with one mesh `body` and a mesh for each wheel that could be found
(`wheel_f`, `wheel_fl`, `wheel_fr`, `wheel_r`, `wheel_rl`, `wheel_rr`), origin on its axle, so the game can spin them.

blender -b --factory-startup -P tools/blender/props/vehicles.py -- [--render <dir>] [names...]

All are CC-BY: docs/references.md carries the credits.
"""

import colorsys
import math
import os
import re
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
MODELS = os.path.join(ROOT, "research", "vehicles", "models")
OUT = os.path.join(ROOT, "assets", "models", "vehicles")


def hsv_arrays(rgb):
    mx, mn = rgb.max(1), rgb.min(1)
    d = mx - mn + 1e-6
    r, g, b = rgb[:, 0], rgb[:, 1], rgb[:, 2]
    h = np.where(mx == r, ((g - b) / d) % 6, np.where(mx == g, (b - r) / d + 2, (r - g) / d + 4)) / 6
    return h, d / (mx + 1e-6), mx


def rgb_arrays(h, s, v):
    i = np.floor(h * 6) % 6
    f = h * 6 - np.floor(h * 6)
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    out = np.zeros((len(h), 3))
    for k, (a, b, c) in enumerate(((v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q))):
        m = i == k
        out[m, 0], out[m, 1], out[m, 2] = a[m], b[m], c[m]
    return out


def pixels(img):
    w, h = img.size
    a = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(h, w, 4)


def put_pixels(img, a):
    img.pixels.foreach_set(a.reshape(-1))
    img.update()


def image_named(part):
    if part in bpy.data.images:
        return bpy.data.images[part]
    for i in bpy.data.images:
        if part.lower() in i.name.lower():
            return i
    raise KeyError(part)


def paint_auto():
    """Delhi's colours: a green body under a yellow hood, as the street kit's parked autos."""
    img = image_named("Body_01")
    a = pixels(img)
    rgb = a[..., :3].reshape(-1, 3).astype(np.float64)
    h, s, v = hsv_arrays(rgb)
    # The paint is yellow; rust is redder and stays.
    paint = (h > 0.09) & (h < 0.17) & (s > 0.45)
    h[paint] = 0.40
    s[paint] *= 0.82
    v[paint] *= 0.62
    a[..., :3] = rgb_arrays(h, s, v).reshape(a.shape[0], a.shape[1], 3)
    put_pixels(img, a)
    # The hood is black cloth: yellow cloth, its folds kept.
    img = image_named("material_baseColor")
    a = pixels(img)
    lum = a[..., :3].mean(2, keepdims=True)
    a[..., :3] = np.clip(np.array((0.95, 0.62, 0.05)) * (0.55 + 2.5 * lum), 0, 1)
    # A new image (the old one is greyscale, and would come back grey) on a new material: the seats
    # share the texture, and stay black.
    new = bpy.data.images.new("hood", img.size[0], img.size[1], alpha=True)
    put_pixels(new, a)
    old = bpy.data.materials["material"]
    hood = old.copy()
    hood.name = "hood"
    for node in hood.node_tree.nodes:
        if node.type == "TEX_IMAGE" and node.image == img:
            node.image = new
    for o in meshes():
        if old.name in o.data.materials:
            i = o.data.materials.find(old.name)
            o.data.materials.append(hood)
            for p in o.data.polygons:
                # Not the panel behind the driver.
                if p.material_index == i and p.center.z > 0.8 and not (abs(p.normal.y) > 0.9 and abs(p.center.y) < 0.1):
                    p.material_index = len(o.data.materials) - 1


def paint_car():
    """A maker's badge on the nose and a brand's label on each sill: painted over."""
    img = image_named("Image_11")
    a = pixels(img)
    h = a.shape[0]
    white = a[h - 500, 500, :3].copy()
    for x0, y0, x1, y1 in ((824, 866, 848, 890), (940, 760, 958, 880)):
        a[h - y1 : h - y0, x0:x1, :3] = white
    put_pixels(img, a)
    img = image_named("Image_1")
    a = pixels(img)
    h = a.shape[0]
    box = a[h - 700 : h - 445, 785:852]
    tan = np.median(a[..., :3].reshape(-1, 3), axis=0)
    box[..., :3] = tan
    put_pixels(img, a)


def paint_scooter():
    """One grey material on the model: a maroon shell, black rubber, dull metal, by part."""
    def flat(name, colour, rough, metal=0.0):
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        b = m.node_tree.nodes["Principled BSDF"]
        b.inputs["Base Color"].default_value = (*colour, 1.0)
        b.inputs["Roughness"].default_value = rough
        b.inputs["Metallic"].default_value = metal
        return m

    shell = flat("shell", (0.23, 0.015, 0.03), 0.35)
    rubber = flat("rubber", (0.015, 0.015, 0.017), 0.85)
    metal = flat("metal", (0.45, 0.46, 0.48), 0.4, 0.8)
    for o in meshes():
        name = o.name.lower()
        o.data.materials.clear()
        size = Vector(o.dimensions)
        o.data.materials.append(shell if "polysurface1_" in name else rubber if max(size) > 0.3 or "pcube" in name else metal)


def paint_bullet():
    """The tank says one maker's name, the side panels another's, the plate is a real plate: gone."""
    img = image_named("Image_0")
    a = pixels(img)
    h = a.shape[0]
    # Blender's rows run bottom up; these boxes are (x0, y0, x1, y1) from the top left of the 1024 px
    # texture. Only the white lettering goes: the tank's yellow line stays as a pinstripe.
    for x0, y0, x1, y1 in ((195, 570, 300, 625), (185, 855, 285, 910), (630, 435, 745, 530), (620, 695, 740, 785)):
        box = a[h - y1 : h - y0, x0:x1]
        white = box[..., 2] > 0.25
        box[white, :3] = a[h - 660, 100, :3]
    a[h - 990 : h - 822, 636:842, :3] = 0.86
    put_pixels(img, a)


CFG = {
    "auto": dict(src="auto/tuk_tuk_rikshaw__0bdfdf/scene.gltf", turn=180, length=2.72, body=9000, paint=paint_auto),
    "scooter": dict(src="scooter/honda_smart_dio_z4_2006_1__ef6060/model.glb", turn=-90, length=1.78, body=11000, paint=paint_scooter),
    "commuter": dict(src="commuter_bike/pulsar_135_ls_2__accbf2/model.glb", turn=0, length=2.02, body=12000, drop=r"^placa", level=True),
    "bullet": dict(src="heavy_bike/classic_motorcycle__f246ff/model.glb", turn=None, length=2.14, body=14000, paint=paint_bullet, level=True),
    "car": dict(src="small_car/stylized_suzuki_alto_works__467f2e/model.glb", turn=180, length=3.4, body=7000, paint=paint_car),
}
WHEEL_TRIS = 500


def meshes():
    return [o for o in bpy.context.scene.objects if o.type == "MESH"]


def world_points():
    return np.concatenate([np.array([o.matrix_world @ v.co for v in o.data.vertices]) for o in meshes()])


def bake_transforms():
    """Every mesh on its own, in world coordinates, unparented."""
    for o in meshes():
        o.data = o.data.copy()
        m = o.matrix_world.copy()
        o.parent = None
        o.data.transform(m)
        o.matrix_world = Matrix.Identity(4)
    for o in list(bpy.context.scene.objects):
        if o.type != "MESH":
            bpy.data.objects.remove(o)


def move_all(m):
    for o in meshes():
        o.data.transform(m)
    bpy.context.view_layer.update()


def face_forward(turn):
    if turn is not None:
        move_all(Matrix.Rotation(math.radians(turn), 4, "Z"))
        return
    # Parked askew on its stand: the thinnest spread of the points is the bike's side, the longest
    # its length. Stand that frame up: length along Y, side along X.
    p = world_points()
    p = p - p.mean(0)
    w, v = np.linalg.eigh(np.cov(p.T))
    side, along = Vector(v[:, 0]), Vector(v[:, 2])
    if side.dot(Vector((1, 0, 0))) < 0:
        side = -side
    if along.dot(Vector((0, 1, 0))) < 0:
        along = -along
    up = side.cross(along).normalized()
    along = up.cross(side).normalized()
    r = Matrix((side, along, up)).to_4x4()
    move_all(r)


def stand_up(length):
    p = world_points()
    lo, hi = p.min(0), p.max(0)
    k = length / (hi[1] - lo[1])
    move_all(Matrix.Scale(k, 4) @ Matrix.Translation((-(lo[0] + hi[0]) / 2, -(lo[1] + hi[1]) / 2, -lo[2])))


def islands(me):
    """Connected parts of a mesh: an array of labels, one per vertex."""
    n = len(me.vertices)
    e = np.empty(len(me.edges) * 2, dtype=np.int64)
    me.edges.foreach_get("vertices", e)
    e = e.reshape(-1, 2)
    labels = np.arange(n)
    while len(e):
        m = np.minimum(labels[e[:, 0]], labels[e[:, 1]])
        before = labels.copy()
        np.minimum.at(labels, e[:, 0], m)
        np.minimum.at(labels, e[:, 1], m)
        labels = labels[labels]
        if (labels == before).all():
            break
    return labels


def find_wheels(reach=0.08):
    """Round parts standing on the ground are tyres (within `reach` of it, on a model not yet level):
    each makes a wheel with whatever fits inside it."""
    parts = []
    for o in meshes():
        co = np.empty(len(o.data.vertices) * 3)
        o.data.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        labels = islands(o.data)
        for lab in np.unique(labels):
            m = labels == lab
            parts.append((o, m, co[m].min(0), co[m].max(0)))
    tyres = []
    for o, m, lo, hi in parts:
        s = hi - lo
        if 0.3 < s[1] < 0.9 and 0.03 < s[0] < 0.75 * s[1] and 0.7 * s[1] < s[2] < 1.15 * s[1] and lo[2] < reach:
            tyres.append((lo, hi))
    # A tyre can be several parts, inner and outer: one wheel for those on one axle and side.
    wheels = []
    for lo, hi in sorted(tyres, key=lambda t: -(t[1][2] - t[0][2])):
        for w in wheels:
            if abs((lo[1] + hi[1]) - (w[0][1] + w[1][1])) < 0.2 and min(hi[0], w[1][0]) - max(lo[0], w[0][0]) > -0.3:
                w[0], w[1] = np.minimum(lo, w[0]), np.maximum(hi, w[1])
                break
        else:
            wheels.append([lo.copy(), hi.copy()])
    # Radius by length: a tyre's top can be missing under a mudguard.
    wheels = [(np.array(((lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, lo[2] + (hi[1] - lo[1]) / 2)), (hi[1] - lo[1]) / 2, (hi[0] - lo[0]) / 2) for lo, hi in wheels]
    print(f"[wheels] {[(np.round(c, 2).tolist(), round(float(r), 2), round(float(w), 2)) for c, r, w in wheels]}")
    picks = {i: {} for i in range(len(wheels))}
    for o, m, lo, hi in parts:
        for i, (c, r, w) in enumerate(wheels):
            rad = max(np.hypot(*(np.array(q) - c[1:])) for q in ((lo[1], lo[2]), (lo[1], hi[2]), (hi[1], lo[2]), (hi[1], hi[2])))
            if rad < r * 1.04 and lo[0] > c[0] - w * 1.25 and hi[0] < c[0] + w * 1.25:
                d = picks[i].setdefault(o.name, np.zeros(len(o.data.vertices), dtype=bool))
                d[m] = True
                break
    return wheels, picks


def level(cfg):
    """A bike parked on its stand stands level on both tyres."""
    wheels, _ = find_wheels(0.3)
    if len(wheels) < 2:
        return
    wheels.sort(key=lambda w: w[0][1])
    (a, ar, _), (b, br, _) = wheels[0], wheels[-1]
    tilt = math.atan2((b[2] - br) - (a[2] - ar), b[1] - a[1])
    print(f"[level] {math.degrees(tilt):.1f} degrees")
    move_all(Matrix.Rotation(-tilt, 4, "X"))
    stand_up(cfg["length"])


def name_wheels(wheels):
    fy = max(c[1] for c, _, _ in wheels)
    names = []
    for c, _, _ in wheels:
        end = "f" if c[1] > fy - 0.4 else "r"
        twin = sum(1 for d, _, _ in wheels if abs(d[1] - c[1]) < 0.4) > 1
        names.append(f"wheel_{end}" + (("r" if c[0] > 0 else "l") if twin else ""))
    return names


def cut(o, keep):
    """Delete the vertices of `o` where `keep` is false."""
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bm.verts.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[bm.verts[i] for i in np.nonzero(~keep)[0]], context="VERTS")
    bm.to_mesh(o.data)
    bm.free()


def join(objs, name):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    if len(objs) > 1:
        bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    o.name = name
    o.data.name = name
    return o


def tris(o):
    return sum(len(p.vertices) - 2 for p in o.data.polygons)


def decimate(o, target):
    n = tris(o)
    if n <= target:
        return
    bpy.context.view_layer.objects.active = o
    d = o.modifiers.new("d", "DECIMATE")
    d.ratio = target / n
    bpy.ops.object.modifier_apply(modifier=d.name)


def split_wheels():
    wheels, picks = find_wheels()
    names = name_wheels(wheels)
    made = []
    for i, name in enumerate(names):
        parts = []
        for oname, mask in picks[i].items():
            o = bpy.data.objects[oname]
            w = o.copy()
            w.data = o.data.copy()
            bpy.context.scene.collection.objects.link(w)
            cut(w, mask)
            parts.append(w)
        if parts:
            made.append((name, parts, wheels[i][0], wheels[i][1]))
    # The copies above used the masks' vertex numbers, so the originals lose those parts last.
    gone = {}
    for i in range(len(names)):
        for oname, mask in picks[i].items():
            gone[oname] = gone.get(oname, np.zeros(len(mask), dtype=bool)) | mask
    for oname, mask in gone.items():
        cut(bpy.data.objects[oname], ~mask)
    out = []
    for name, parts, centre, r in made:
        w = join(parts, name)
        decimate(w, WHEEL_TRIS)
        w.data.transform(Matrix.Translation(tuple(-centre)))
        w.location = tuple(centre)
        # A front wheel parked turned: its mesh squared up, the turn kept on the node, which is what
        # the game spins about.
        co = np.empty(len(w.data.vertices) * 3)
        w.data.vertices.foreach_get("co", co)
        axle = np.linalg.eigh(np.cov(co.reshape(-1, 3).T))[1][:, 0]
        yaw = math.atan2(axle[1], axle[0]) if axle[0] > 0 else math.atan2(-axle[1], -axle[0])
        w.data.transform(Matrix.Rotation(-yaw, 4, "Z"))
        w.rotation_euler = (0, 0, yaw)
        out.append((name, w, centre, r))
    return out


def tidy_materials(size):
    """Base colour only, as PNG, no bigger than `size`."""
    for m in bpy.data.materials:
        if not m.use_nodes:
            continue
        for node in m.node_tree.nodes:
            if node.type != "BSDF_PRINCIPLED":
                continue
            for sock in ("Normal", "Metallic", "Roughness", "Emission Color", "Emission Strength"):
                i = node.inputs.get(sock)
                if i is not None:
                    for l in list(i.links):
                        m.node_tree.links.remove(l)
            node.inputs["Roughness"].default_value = max(0.4, node.inputs["Roughness"].default_value)
    used = set()
    for m in bpy.data.materials:
        if m.use_nodes:
            for node in m.node_tree.nodes:
                if node.type == "TEX_IMAGE" and node.image:
                    used.add(node.image.name)
    for img in list(bpy.data.images):
        if img.name not in used:
            bpy.data.images.remove(img)
    # The game reads PNG only: every image is written out fresh, whatever it came in as.
    for img in list(bpy.data.images):
        k = min(1.0, size / max(img.size))
        w, h = int(img.size[0] * k), int(img.size[1] * k)
        if k < 1.0:
            img.scale(w, h)
        new = bpy.data.images.new(img.name.rsplit(".", 1)[0] + "_png", w, h, alpha=True)
        new.colorspace_settings.name = img.colorspace_settings.name
        put_pixels(new, pixels(img))
        new.file_format = "PNG"
        for m in bpy.data.materials:
            for node in m.node_tree.nodes if m.use_nodes else ():
                if node.type == "TEX_IMAGE" and node.image == img:
                    node.image = new
        bpy.data.images.remove(img)


def build(name):
    cfg = CFG[name]
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, cfg["src"]))
    bake_transforms()
    if "drop" in cfg:
        for o in meshes():
            if re.search(cfg["drop"], o.name):
                bpy.data.objects.remove(o)
    face_forward(cfg["turn"])
    stand_up(cfg["length"])
    if cfg.get("level"):
        level(cfg)
    for o in meshes():
        bpy.context.view_layer.update()
        print(f"[part] {name} {o.name[:34]:34s} tris {tris(o):6d} size {[round(x, 2) for x in o.dimensions]} mats {[m.name for m in o.data.materials][:3]}")
    if "paint" in cfg:
        cfg["paint"]()
    wheels = split_wheels()
    wnames = {w[0] for w in wheels}
    body = join([o for o in meshes() if o.name not in wnames and len(o.data.polygons)], "body")
    decimate(body, cfg["body"])
    for o in [o for o in meshes() if not len(o.data.polygons)]:
        bpy.data.objects.remove(o)
    tidy_materials(1024)
    # One set of texture coordinates is all the game reads.
    for o in meshes():
        for uv in list(o.data.uv_layers)[1:]:
            o.data.uv_layers.remove(uv)
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, f"{name}.glb")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", export_yup=True, export_apply=False, export_image_format="AUTO",
                              export_animations=False, export_skins=False, export_cameras=False, export_lights=False, use_selection=False)
    p = world_points()
    print(f"[done] {name}: body {tris(body)} tris, wheels {[(w[0], tris(w[1]), round(w[3], 3), [round(float(x), 2) for x in w[2]]) for w in wheels]}, "
          f"size {np.round(p.max(0) - p.min(0), 2).tolist()}, {os.path.getsize(path) / 1e6:.1f} MB")


def render(name, out):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.join(OUT, f"{name}.glb"))
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.display.shading.light = "STUDIO"
    scene.display.shading.color_type = "TEXTURE"
    scene.render.resolution_x, scene.render.resolution_y = 640, 440
    world = bpy.data.worlds.new("w")
    world.color = (0.7, 0.7, 0.72)
    scene.world = world
    cam = bpy.data.cameras.new("c")
    cam.lens = 70
    co = bpy.data.objects.new("c", cam)
    scene.collection.objects.link(co)
    scene.camera = co
    p = world_points()
    centre = Vector((p.min(0) + p.max(0)) / 2)
    r = Vector(p.max(0) - p.min(0)).length * 1.5
    os.makedirs(out, exist_ok=True)
    for tag, az, el in (("a", 55, 15), ("b", 0, 2), ("c", 90, 2), ("d", 270, 2)):
        a, e = math.radians(az), math.radians(el)
        co.location = centre + Vector((math.cos(a) * math.cos(e), math.sin(a) * math.cos(e), math.sin(e))) * r
        co.rotation_mode = "QUATERNION"
        co.rotation_quaternion = (centre - co.location).to_track_quat("-Z", "Y")
        scene.render.filepath = os.path.join(out, f"{name}_{tag}.png")
        bpy.ops.render.render(write_still=True)


args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
out = None
if "--render" in args:
    i = args.index("--render")
    out = args[i + 1]
    del args[i : i + 2]
for name in args or list(CFG):
    build(name)
    if out:
        render(name, out)

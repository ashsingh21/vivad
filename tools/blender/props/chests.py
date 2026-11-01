"""The two loot chests, each a body and a lid (the lid hinged open by the game, crate::visuals):

- peti: the painted steel trunk every Indian household keeps under the bed: deep teal paint with a
  pinstriped frame and a bouquet painted on its front and lid, pressed ribs round it, brass corner
  caps, hinges and hasp, a brass padlock, steel handles at the ends; the paint chipped to bare steel
  on its edges and dusty in its corners.
- kalash: the purna kalash (the armoury's amrita vessel, tools/blender/weapons/arsenal.py) at the
  size of a chest, on a brass thali: a chased gold pot, mauli at its neck, mango leaves and a coconut
  on top (the lid, lifted off when opened).

Each is baked to one texture set. The lid's mesh has its origin on the hinge (the peti's back top
edge; the kalash's mouth), so the game turns it about there.

blender -b --factory-startup -P tools/blender/props/chests.py   -> assets/models/props/chest_{peti,kalash}.glb
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, os.path.join(HERE, "..", "weapons"))

import bmesh  # noqa: E402
import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import arsenal  # noqa: E402
import bake  # noqa: E402
import export  # noqa: E402
import finishes as F  # noqa: E402
import geo  # noqa: E402
from nodes import Graph, new_material  # noqa: E402

OUT = os.path.join(ROOT, "assets", "models", "props")
TEX = os.path.join(ROOT, "build", "textures")
BRASS = (0.86, 0.62, 0.30)

# The trunk, metres (Blender: x across, y front (-) to back (+), z up).
W, D, H, LID = 1.1, 0.62, 0.5, 0.17


def painted_steel(name, paint):
    """Enamel paint over pressed steel: a pinstriped frame and a bouquet where `deco` is set (1 on
    the front, its centre at `panel_z`; 2 on the lid's top), chipped to steel on the edges, rust
    in the deeper chips, dust in the corners."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    xyz = g.node("ShaderNodeSeparateXYZ", {"Vector": co})
    x, y, z = xyz.outputs["X"], xyz.outputs["Y"], xyz.outputs["Z"]
    deco = g.attr("deco")
    top = g.smooth(deco, 1.5, 1.6)
    # 3: inside, where the open lid shows it: lined with faded red paper.
    inside = g.smooth(deco, 2.5, 2.6)
    on = g.mul(g.smooth(deco, 0.5, 0.6), g.sub(1.0, inside))
    # Panel coordinates: across, and up the front or back across the lid.
    pu = x
    pv = g.mix(top, g.sub(z, H * 0.5), y)
    hh = g.mix(top, 0.15, 0.22)
    # The frame: two pinstripes inset from the panel's edge.
    d = g.maxf(g.sub(g.absf(pu), 0.4), g.sub(g.absf(pv), hh))
    stripe = g.maxf(g.band(d, -0.006, -0.002, 0.0015), g.band(d, -0.022, -0.019, 0.0015))
    # The bouquet: a big six-petalled flower in the middle, two smaller either side, dots round.
    def flower(cx, size, petals):
        du, dv = g.sub(pu, cx), pv
        r = g.math("SQRT", g.add(g.mul(du, du), g.mul(dv, dv)))
        th = g.math("ARCTAN2", dv, du)
        edge = g.mul(size, g.add(0.55, g.mul(0.45, g.absf(g.math("COSINE", g.mul(th, petals * 0.5))))))
        petal = g.sub(1.0, g.smooth(g.sub(r, edge), -0.002, 0.002))
        heart = g.sub(1.0, g.smooth(g.math("DIVIDE", r, size), 0.28, 0.32))
        return petal, heart
    big, big_c = flower(0.0, g.mix(top, 0.075, 0.1), 6)
    s1, s1_c = flower(-0.2, 0.045, 5)
    s2, s2_c = flower(0.2, 0.045, 5)
    # Leaves: ellipses angled out from the big flower toward the small ones.
    def leaf(cx, cy, ang):
        c, s = math.cos(ang), math.sin(ang)
        du, dv = g.sub(pu, cx), g.sub(pv, cy)
        a = g.add(g.mul(du, c), g.mul(dv, s))
        b = g.sub(g.mul(dv, c), g.mul(du, s))
        e = g.add(g.mul(g.mul(a, a), 1.0 / 0.035 ** 2), g.mul(g.mul(b, b), 1.0 / 0.013 ** 2))
        return g.sub(1.0, g.smooth(e, 0.85, 1.15))
    leaves = g.maxf(g.maxf(leaf(-0.11, 0.035, 0.5), leaf(0.11, 0.035, -0.5)), g.maxf(leaf(-0.11, -0.035, -0.5), leaf(0.11, -0.035, 0.5)))
    paint_col = paint
    col = g.mixc(g.mul(on, stripe), paint_col, (0.92, 0.78, 0.42))
    col = g.mixc(g.mul(on, leaves), col, (0.16, 0.45, 0.16))
    col = g.mixc(g.mul(on, g.maxf(s1, s2)), col, (0.95, 0.55, 0.12))
    col = g.mixc(g.mul(on, big), col, (0.80, 0.12, 0.20))
    col = g.mixc(g.mul(on, g.maxf(big_c, g.maxf(s1_c, s2_c))), col, (0.98, 0.85, 0.25))
    # Enamel: faint mottle, a slightly lighter sheen high up.
    mottle = g.noise(co, 6.0, detail=4.0)
    col = g.mixc(g.mul(mottle, 0.25), col, tuple(c * 0.8 for c in paint))
    # Chips: on the edges, broken up by noise; the deepest rusted.
    edge = g.edge_mask(0.008, 2.4)
    chipn = g.noise(co, 22.0, detail=6.0, rough=0.7)
    chip = g.smooth(g.add(g.mul(edge, 1.1), g.mul(g.sub(chipn, 0.5), 1.4)), 0.62, 0.72)
    rust = g.mul(chip, g.smooth(g.noise(co, 9.0, detail=3.0), 0.55, 0.7))
    steel = (0.30, 0.29, 0.28)
    col = g.mixc(chip, col, steel)
    col = g.mixc(rust, col, (0.34, 0.16, 0.07))
    # Scratches: fine streaks across the faces.
    streak = g.smooth(g.noise(g.mapping(co, scale=(60.0, 60.0, 2.0)), 3.0, detail=2.0), 0.68, 0.72)
    col = g.mixc(g.mul(streak, 0.35), col, tuple(min(1.0, c * 1.5 + 0.06) for c in paint))
    # Dust in the corners and on the lower edge.
    cavity = g.sub(1.0, g.ao(0.06))
    dust = g.maxf(g.smooth(cavity, 0.15, 0.6), g.mul(g.smooth(z, 0.08, 0.0), 0.6))
    col = g.mixc(g.mul(dust, 0.5), col, (0.42, 0.36, 0.28))
    paper = g.noise(co, 40.0, detail=5.0)
    lining = g.mixc(g.mul(paper, 0.5), (0.42, 0.09, 0.08), (0.28, 0.06, 0.06))
    col = g.mixc(inside, col, lining)
    chip = g.mul(chip, g.sub(1.0, inside))
    rust = g.mul(rust, g.sub(1.0, inside))
    metal = g.mul(chip, g.sub(1.0, rust))
    rough = g.add(0.38, g.mul(mottle, 0.1))
    rough = g.mix(chip, rough, 0.5)
    rough = g.mix(rust, rough, 0.85)
    rough = g.mix(g.mul(dust, 0.5), rough, 0.9)
    rough = g.mix(inside, rough, 0.85)
    height = g.sub(g.mul(chip, -1.0), g.mul(streak, 0.2))
    g.principled(col, metal, rough, g.bump(height, 0.4, 0.0008))
    return m


def brass_fitting(name):
    """Brass at a chest's size: polished on the edges, tarnished and dark in the hollows."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    edge = g.edge_mask(0.004, 2.4)
    cavity = g.sub(1.0, g.ao(0.03))
    mottle = g.noise(co, 30.0, detail=5.0, rough=0.6)
    col = g.mixc(g.mul(mottle, 0.35), BRASS, tuple(c * 0.75 for c in BRASS))
    grime = g.mul(g.smooth(cavity, 0.1, 0.55), g.smooth(g.noise(co, 20.0, detail=3.0), 0.35, 0.6))
    col = g.mixc(grime, col, (0.16, 0.11, 0.05))
    col = g.mixc(g.mul(edge, 0.6), col, (1.0, 0.82, 0.5))
    rough = g.mix(edge, g.add(0.38, g.mul(mottle, 0.15)), 0.18)
    rough = g.mix(grime, rough, 0.75)
    g.principled(col, g.mix(grime, 1.0, 0.4), rough, g.bump(g.mul(mottle, 0.3), 0.1, 0.0005))
    return m


def dark_steel(name):
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    edge = g.edge_mask(0.003, 2.4)
    mottle = g.noise(co, 25.0, detail=4.0)
    col = g.mixc(g.mul(mottle, 0.4), (0.16, 0.16, 0.17), (0.10, 0.09, 0.09))
    col = g.mixc(g.mul(edge, 0.7), col, (0.45, 0.45, 0.44))
    g.principled(col, 0.85, g.add(0.42, g.mul(mottle, 0.15)), g.bump(mottle, 0.1, 0.0004))
    return m


def box(name, mn, mx, mat, bevel=0.006, cuts=0, dome=0.0):
    """A box, its edges rounded; `dome` bows its top up toward the middle front to back."""
    bm = bmesh.new()
    size = Vector(mx) - Vector(mn)
    centre = (Vector(mx) + Vector(mn)) * 0.5
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation(centre) @ Matrix.Diagonal((*size, 1.0)))
    if cuts:
        bmesh.ops.subdivide_edges(bm, edges=[e for e in bm.edges if abs((e.verts[0].co - e.verts[1].co).normalized().y) > 0.9], cuts=cuts, use_grid_fill=True)
    if dome:
        for v in bm.verts:
            if v.co.z > mx[2] - 1e-4:
                t = (v.co.y - centre.y) / (size.y * 0.5)
                v.co.z += dome * (1.0 - t * t)
    # A metric box projection, each face unwrapped along its main axis (pack_uvs packs it).
    uv = bm.loops.layers.uv.new("UVMap")
    for f in bm.faces:
        n = f.normal
        k = max(range(3), key=lambda i: abs(n[i]))
        a, b = [(1, 2), (0, 2), (0, 1)][k]
        for loop in f.loops:
            loop[uv].uv = (loop.vert.co[a], loop.vert.co[b])
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    me.materials.append(mat)
    if bevel:
        b = o.modifiers.new("bevel", "BEVEL")
        b.width = bevel
        b.segments = 3
        b.limit_method = "ANGLE"
        b.angle_limit = math.radians(30)
        b.use_clamp_overlap = True
    for p in me.polygons:
        p.use_smooth = True
    return o


def tag(o, name, value, faces=None):
    """A per-face float attribute (as a face-corner one, which the bake's attribute node reads)."""
    me = o.data
    a = me.attributes.get(name) or me.attributes.new(name, "FLOAT", "CORNER")
    for p in me.polygons:
        if faces is None or faces(p):
            for li in p.loop_indices:
                a.data[li].value = value


def mark_lid(objs):
    for o in objs:
        me = o.data
        a = me.attributes.get("lid") or me.attributes.new("lid", "FLOAT", "CORNER")
        for d in a.data:
            d.value = 1.0


def bake_split(name, parts, hinge, scale=1.0, res=1024):
    """Join and bake the parts to one texture set, then split them back into body and lid (the lid
    with its origin at `hinge`), scaled by `scale`, and export both."""
    for p in parts:
        geo.apply_all_modifiers(p)
    whole = geo.join(parts, f"{name}_whole")
    export.pack_uvs(whole)
    bake.setup_cycles()
    images = bake.bake_texture_set(whole, os.path.join(TEX, name), name, res=res)
    whole.data.materials.clear()
    whole.data.materials.append(export.game_material(name, images))
    export.triangulate(whole)
    # Which faces are the lid's, kept as a face attribute (the corner ones go with the bake data).
    me = whole.data
    corner = me.attributes["lid"].data
    face = me.attributes.new("lidf", "INT", "FACE")
    for p in me.polygons:
        face.data[p.index].value = int(corner[p.loop_indices[0]].value > 0.5)
    export.strip_bake_data(whole)
    out = []
    for part, keep_lid in (("body", False), ("lid", True)):
        o = whole.copy()
        o.data = whole.data.copy()
        o.name = o.data.name = f"{name}_{part}"
        bpy.context.scene.collection.objects.link(o)
        bm = bmesh.new()
        bm.from_mesh(o.data)
        lidf = bm.faces.layers.int.get("lidf")
        drop = [f for f in bm.faces if bool(f[lidf]) != keep_lid]
        bmesh.ops.delete(bm, geom=drop, context="FACES")
        origin = Vector(hinge) if keep_lid else Vector()
        for v in bm.verts:
            v.co = (v.co - origin) * scale
        bm.to_mesh(o.data)
        bm.free()
        for a in [a for a in o.data.attributes if a.name in ("lid", "lidf", "deco")]:
            o.data.attributes.remove(a)
        out.append(o)
    bpy.data.objects.remove(whole, do_unlink=True)
    counts = export.export_glb(out, os.path.join(OUT, f"chest_{name}.glb"))
    print(f"[chests] chest_{name}.glb {counts}")


def peti():
    paint = painted_steel("peti_paint", (0.05, 0.27, 0.30))
    brass = brass_fitting("peti_brass")
    steel = dark_steel("peti_steel")
    body, lid = [], []
    hw, hd = W / 2, D / 2
    shell = box("shell", (-hw, -hd, 0.04), (hw, hd, H), paint, bevel=0.012)
    tag(shell, "deco", 0.0)
    tag(shell, "deco", 1.0, lambda p: p.normal.y < -0.9)
    tag(shell, "deco", 3.0, lambda p: p.normal.z > 0.9)
    body.append(shell)
    # A steel skid under it, pressed ribs round it.
    body.append(box("skid", (-hw + 0.01, -hd + 0.01, 0.0), (hw - 0.01, hd - 0.01, 0.045), steel, bevel=0.008))
    for z in (0.1, H - 0.06):
        body.append(box("rib", (-hw - 0.006, -hd - 0.006, z - 0.012), (hw + 0.006, hd + 0.006, z + 0.012), paint, bevel=0.008))
    # Brass caps on the eight corners of the body (the top four just under the lid's skirt).
    c = 0.075
    for sx in (-1, 1):
        for sy in (-1, 1):
            for z0, z1 in ((0.0, 0.1), (H - 0.1, H - 0.005)):
                x0, x1 = sorted((sx * (hw + 0.008), sx * (hw - c)))
                y0, y1 = sorted((sy * (hd + 0.008), sy * (hd - c)))
                body.append(box("cap", (x0, y0, z0), (x1, y1, z1), brass, bevel=0.01))
    # Handles at the ends: a steel bar bent out from two brass plates.
    for sx in (-1, 1):
        x = sx * (hw + 0.006)
        for py in (-0.09, 0.09):
            body.append(box("plate", (min(x, x + sx * 0.012), py - 0.025, 0.33), (max(x, x + sx * 0.012), py + 0.025, 0.4), brass, bevel=0.004))
        pts = [Vector((x + sx * 0.012, -0.09, 0.365)), Vector((x + sx * 0.045, -0.075, 0.37)), Vector((x + sx * 0.055, 0.0, 0.372)),
               Vector((x + sx * 0.045, 0.075, 0.37)), Vector((x + sx * 0.012, 0.09, 0.365))]
        pts = geo.catmull_rom(pts, 14)
        body.append(arsenal.tube("handle", pts, lambda t: 0.009, steel, seg=10, up=Vector((0, 0, 1))))
    # The staple and the padlock hanging from it.
    fy = -hd - 0.008
    body.append(box("staple", (-0.03, fy - 0.01, H - 0.11), (0.03, fy, H - 0.05), brass, bevel=0.003))
    body.append(box("lock", (-0.045, fy - 0.045, H - 0.2), (0.045, fy - 0.012, H - 0.12), brass, bevel=0.01))
    arc = [Vector((0.028 * math.cos(a), fy - 0.03, H - 0.12 + 0.045 * math.sin(a))) for a in [math.pi * k / 12 for k in range(13)]]
    body.append(arsenal.tube("shackle", arc, lambda t: 0.006, steel, seg=8, up=Vector((0, 1, 0))))
    body.append(box("keyhole", (-0.008, fy - 0.0465, H - 0.185), (0.008, fy - 0.044, H - 0.15), steel, bevel=0.002))
    # Hinges: brass straps down the back, body halves.
    for x in (-0.3, 0.3):
        body.append(box("hinge", (x - 0.04, hd, H - 0.12), (x + 0.04, hd + 0.008, H - 0.004), brass, bevel=0.003))
    # The lid: its skirt overhanging the body, its top bowed; a painted bouquet on top, brass caps
    # on its corners, the hasp hanging over the staple, the hinges' upper halves.
    top = box("lid", (-hw - 0.012, -hd - 0.012, H), (hw + 0.012, hd + 0.012, H + LID), paint, bevel=0.012, cuts=6, dome=0.035)
    tag(top, "deco", 0.0)
    tag(top, "deco", 2.0, lambda p: p.normal.z > 0.7)
    tag(top, "deco", 3.0, lambda p: p.normal.z < -0.9)
    lid.append(top)
    for z in (H + 0.035,):
        lid.append(box("lid_rib", (-hw - 0.018, -hd - 0.018, z - 0.01), (hw + 0.018, hd + 0.018, z + 0.01), paint, bevel=0.007))
    for sx in (-1, 1):
        for sy in (-1, 1):
            x0, x1 = sorted((sx * (hw + 0.02), sx * (hw - c)))
            y0, y1 = sorted((sy * (hd + 0.02), sy * (hd - c)))
            lid.append(box("lid_cap", (x0, y0, H + 0.002), (x1, y1, H + LID - 0.01), brass, bevel=0.012))
    lid.append(box("hasp", (-0.04, fy - 0.016, H - 0.075), (0.04, fy - 0.006, H + 0.11), brass, bevel=0.004))
    lid.append(box("hasp_slot", (-0.012, fy - 0.0175, H - 0.07), (0.012, fy - 0.015, H - 0.04), steel, bevel=0.001))
    for x in (-0.3, 0.3):
        lid.append(box("hinge_top", (x - 0.04, hd + 0.012, H + 0.004), (x + 0.04, hd + 0.02, H + LID - 0.02), brass, bevel=0.003))
        lid.append(arsenal.tube("pin", [Vector((x - 0.04, hd + 0.014, H)), Vector((x + 0.04, hd + 0.014, H))], lambda t: 0.009, brass, seg=10,
                                up=Vector((0, 0, 1))))
    mark_lid(lid)
    for o in body:
        if "lid" not in o.data.attributes:
            o.data.attributes.new("lid", "FLOAT", "CORNER")
        if "deco" not in o.data.attributes:
            tag(o, "deco", 0.0)
    for o in lid:
        if "deco" not in o.data.attributes:
            tag(o, "deco", 0.0)
    bake_split("peti", body + lid, hinge=(0.0, hd + 0.014, H), res=1024)


def kalash():
    """The amrita vessel's design at the armoury's scale (so its finishes read as they do there),
    on a thali standing on the ground (a ring: the pot, its foot set into it, hides its middle), scaled up to a chest's size after baking."""
    gold = F.metal("kalash_gold", arsenal.GOLD, rough=0.2, chase=True)
    plate = F.metal("kalash_thali", BRASS, rough=0.3)
    leaf_m = F.leaf("kalash_leaf", (0.12, 0.32, 0.08))
    nut = F.plain("kalash_coconut", (0.36, 0.23, 0.12), rough=0.85)
    thread = F.wrap("kalash_thread", (0.70, 0.08, 0.04), (0.90, 0.70, 0.15))
    Z = Vector((0, 0, 1))
    lathe = arsenal.lathe
    prof = [(0.0, 0.0), (0.04, 0.0), (0.045, 0.005), (0.04, 0.012), (0.062, 0.03), (0.08, 0.07), (0.077, 0.11),
            (0.055, 0.14), (0.034, 0.155), (0.032, 0.17), (0.05, 0.18), (0.052, 0.186), (0.046, 0.188)]
    body = [lathe("pot", prof, gold, seg=32, caps=(None, "fan"))]
    body.append(lathe("mauli", [(0.0335, 0.15), (0.037, 0.153), (0.037, 0.163), (0.0335, 0.166)], thread, seg=24))
    body.append(lathe("bandA", [(0.0785, 0.066), (0.0805, 0.068), (0.0805, 0.075), (0.0785, 0.077)], gold, seg=32))
    body.append(lathe("thali", [(0.035, 0.006), (0.1, 0.006), (0.118, 0.013), (0.122, 0.012), (0.118, 0.004), (0.1, 0.0), (0.035, 0.0), (0.035, 0.006)],
                      plate, seg=40))
    # A closed ring: its faces turned outward whichever way the profile ran.
    thali = body[-1]
    bm = bmesh.new()
    bm.from_mesh(thali.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(thali.data)
    bm.free()
    lid = []
    for k in range(7):
        a = 2 * math.pi * k / 7
        out = Vector((math.cos(a), math.sin(a), 0))
        S = 10
        frames, rings = [], []
        base = Vector((0, 0, 0.182)) + out * 0.03
        tip_dir = (out * 0.9 + Z * 0.2).normalized()
        nrm = Z.cross(out).normalized()
        for s in range(S):
            u = s / (S - 1)
            w = 0.08 * 0.17 * math.sin(math.pi * u) ** 0.7 + 0.0005
            c = base + tip_dir * (0.08 * u) - Z * (0.035 * u * u)
            frames.append((c, nrm, tip_dir.cross(nrm).normalized()))
            rings.append([(w, 0.0), (0.0, 0.0009), (-w, 0.0), (0.0, -0.0009)])
        lid.append(geo.loft(f"leaf{k}", frames, rings, cap_start="fan", cap_end="fan", material=leaf_m, merge=True,
                            corner_attrs={"u": lambda si, ri, c: si / 9,
                                          "v": lambda si, ri, c: {0: 1.0, 1: 0.0, 2: -1.0, 3: 0.0}[ri % 4]}))
    # The coconut: an egg, smooth enough at a chest's size, a little pointed at its tip.
    egg = [(0.042 * math.sin(math.pi * k / 16) * (1.0 - 0.18 * k / 16), 0.18 + 0.041 * (1.0 - math.cos(math.pi * k / 16)))
           for k in range(17)]
    egg[-1] = (0.0, egg[-1][1] + 0.004)
    lid.append(lathe("coconut", egg, nut, seg=28))
    for o in body + lid:
        if "lid" not in o.data.attributes:
            o.data.attributes.new("lid", "FLOAT", "CORNER")
    mark_lid(lid)
    # About 1.1 m tall on its thali.
    bake_split("kalash", body + lid, hinge=(0.0, 0.0, 0.182), scale=4.2, res=1024)


def main():
    for build in (peti, kalash):
        export.reset_scene()
        build()


if __name__ == "__main__":
    main()

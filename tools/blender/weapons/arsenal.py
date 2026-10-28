"""The rest of the armoury: the three divine bows, the arrow and its quiver, the two thrown astras and
the three healing vessels. Each model is lofted, given procedural materials, baked to one texture
set and exported like the khanda. See docs/references.md ("Bows, astras and vessels").

Axes follow the weapon convention (crates/shared/src/weapon.rs): origin where the hand closes,
business end Blender +Z. Bows are the exception the convention leaves room for: the limbs run
along Z with the grip at the origin, the target is Blender +Y (glTF -Z) and the string side -Y.
Bow strings are drawn by the game (they bend with the draw), from the anchor points this script
writes to assets/data/bows.ron.

blender -b --factory-startup -P tools/blender/weapons/arsenal.py [-- name ...]
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import bake  # noqa: E402
import export  # noqa: E402
import finishes as F  # noqa: E402
import geo  # noqa: E402
from geo import catmull_rom, ellipse, interp_keys, lerp, resample_even  # noqa: E402

OUT = os.path.join(ROOT, "assets", "models", "weapons")
X, Y, Z = Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1))

GOLD = (0.95, 0.70, 0.32)
SILVER = (0.80, 0.80, 0.78)
COPPER = (0.88, 0.48, 0.30)


def attrs(**fns):
    """corner_attrs from per-station / per-ring-index functions of (si, ri, ctx)."""
    return fns


def along(ctx, si, ri):
    return ctx["col_arc"][si][ri % ctx["n"]]


def ring_ac(n):
    """Across coordinate for an ellipse ring: sin of the ring angle (-1..1 across its width)."""
    return lambda si, ri, ctx: math.sin(2 * math.pi * ri / n)


def tube(name, points, radius, mat, *, seg=10, up=X, caps=(None, None), extra=None, rx=None):
    """Loft a (possibly elliptical) tube along `points`; radius(t) / rx(t) take t = 0..1 along it."""
    frames = geo.path_frames(points, up)
    S = len(points)
    rings = [ellipse(rx(i / (S - 1)) if rx else radius(i / (S - 1)), radius(i / (S - 1)), seg) for i in range(S)]
    ca = {"al": lambda si, ri, c: along(c, si, ri), "ac": ring_ac(seg),
          "hw": lambda si, ri, c: radius(si / (S - 1))}
    ca.update(extra or {})
    return geo.loft(name, frames, rings, cap_start=caps[0], cap_end=caps[1], corner_attrs=ca, material=mat, merge=True)


def lathe(name, profile, mat, *, seg=16, axis=Z, origin=Vector(), extra=None, caps=(None, None)):
    ca = {"al": lambda si, ri, c: along(c, si, ri), "ac": ring_ac(seg), "hw": lambda si, ri, c: profile[si][0]}
    ca.update(extra or {})
    return geo.lathe(name, profile, seg, z_axis=axis, origin=origin, corner_attrs=ca, material=mat,
                     cap_start=caps[0], cap_end=caps[1])


def finish(name, parts, res=1024, hard=()):
    """Bevel the hard parts, join, bake one texture set and export LOD0 + LOD1."""
    for p in parts:
        if p.name.split(".")[0] in hard:
            geo.finish_hard_surface(p, bevel_width=0.0004)
        geo.apply_all_modifiers(p)
    lod0 = geo.join(parts, f"{name}_LOD0")
    export.pack_uvs(lod0)
    bake.setup_cycles()
    images = bake.bake_texture_set(lod0, os.path.join(ROOT, "build", "textures", name), name, res=res)
    lod0.data.materials.clear()
    lod0.data.materials.append(export.game_material(name, images))
    export.strip_bake_data(lod0)
    export.triangulate(lod0)
    lods = export.make_lods(lod0, (0.35,))
    counts = export.export_glb(lods, os.path.join(OUT, f"{name}.glb"))
    print(f"[{name}] exported tris: {counts}")


# ------------------------------------------------------------------ bows

BOWS = {
    # Arjuna's bow: an Indo-Persian composite, crimson lacquer painted with gold vines.
    "gandiva": dict(knee=(0.54, 0.19), ear=(0.15, 0.95), width=(0.040, 0.042, 0.030, 0.020), thick=(0.036, 0.026, 0.021, 0.017),
                    lacquer=dict(base=(0.30, 0.03, 0.025)), ear_mat="lacquer", fit=GOLD,
                    wrap=((0.42, 0.035, 0.03), (0.85, 0.62, 0.25))),
    # Vishnu's bow of horn: short and deeply recurved, indigo lacquer, horn ears, a gold chakra.
    "sharanga": dict(knee=(0.43, 0.18), ear=(0.13, 1.1), width=(0.044, 0.046, 0.034, 0.022), thick=(0.040, 0.029, 0.023, 0.019),
                     lacquer=dict(base=(0.025, 0.04, 0.16)), ear_mat="horn", fit=GOLD,
                     wrap=((0.10, 0.07, 0.05), (0.85, 0.62, 0.25))),
    # Shiva's great bow: charcoal limbs striped with ash (the tripundra), silver, Vasuki coiled round
    # the grip and a trishula at each tip.
    "pinaka": dict(knee=(0.70, 0.23), ear=(0.17, 0.85), width=(0.046, 0.048, 0.036, 0.024), thick=(0.042, 0.030, 0.024, 0.020),
                   lacquer=dict(base=(0.035, 0.035, 0.04), paint=(0.80, 0.80, 0.78), band_rgb=(0.78, 0.76, 0.70)),
                   ear_mat="lacquer", fit=SILVER, wrap=((0.16, 0.08, 0.05), None)),
}


def bow_centerline(spec, n_half=34):
    """Strung profile, upper half mirrored: a flat grip, limbs bending back toward the archer in a
    widening arc down to the knee, then the stiff ear (siyah) angled forward to the nock."""
    zk, dk = spec["knee"]
    ear_len, ear_turn = spec["ear"]
    ctrl = []
    for u in (0.0, 0.1, 0.25, 0.4, 0.55, 0.7, 0.85, 1.0):
        w = max(0.0, u - 0.1) / 0.9
        ctrl.append(Vector((0, -dk * w ** 1.8, zk * u)))
    tan = (ctrl[-1] - ctrl[-2]).normalized()
    ear_dir = Matrix.Rotation(-ear_turn, 3, X) @ tan
    ctrl += [ctrl[-1] + ear_dir * ear_len * 0.5, ctrl[-1] + ear_dir * ear_len]
    upper = resample_even(catmull_rom(ctrl, 160), n_half)
    lower = [Vector((0, p.y, -p.z)) for p in reversed(upper[1:])]
    return lower + upper


def bow(name):
    spec = BOWS[name]
    zk = spec["knee"][0]
    # Pinaka's ash stripes: three by the grip and three inside each knee (bt runs 0..1 grip to tip).
    tk0 = zk / (zk + spec["ear"][0])
    stripes = [(0.17, 0.19), (0.21, 0.23), (0.25, 0.27), (tk0 - 0.15, tk0 - 0.13), (tk0 - 0.11, tk0 - 0.09), (tk0 - 0.07, tk0 - 0.05)]
    lac = F.lacquer(f"{name}_lacquer", stripes=stripes, **spec["lacquer"])
    fit = F.metal(f"{name}_fit", spec["fit"], rough=0.25, chase=True)
    ear = F.horn(f"{name}_ear") if spec["ear_mat"] == "horn" else lac
    bone = F.horn(f"{name}_bone", dark=(0.62, 0.56, 0.45), light=(0.85, 0.80, 0.68))
    wrap_a, wrap_b = spec["wrap"]
    grip_m = F.wrap(f"{name}_grip", wrap_a, wrap_b)

    pts = bow_centerline(spec)
    N = len(pts)
    mid = N // 2
    half_len = sum((b - a).length for a, b in zip(pts[mid:], pts[mid + 1:]))
    knee_z = spec["knee"][0]
    knee_i = min(range(mid, N), key=lambda i: abs(pts[i].z - knee_z))
    w0, w1, w2, w3 = spec["width"]
    t0, t1, t2, t3 = spec["thick"]
    tk = (knee_i - mid) / (N - 1 - mid)

    def tparam(i):
        return abs(i - mid) / (N - 1 - mid)

    def width(t):
        return interp_keys([(0.0, w0), (0.12, w0), (0.22, w1), (tk, w2), (1.0, w3)], t)

    def thick(t):
        return interp_keys([(0.0, t0), (0.12, t0), (0.22, t1), (tk, t2), (1.0, t3)], t)

    frames = geo.path_frames(pts, X)
    seg = 12

    def part(pname, lo, hi, mat, scale=1.0, pad=0.0, caps=(None, None)):
        idx = list(range(lo, hi + 1))
        rings = [ellipse(thick(tparam(i)) / 2 * scale + pad, width(tparam(i)) / 2 * scale + pad, seg) for i in idx]
        ca = {
            "al": lambda si, ri, c: along(c, si, ri),
            "ac": ring_ac(seg),
            "hw": lambda si, ri, c, idx=idx: width(tparam(idx[si])) / 2,
            "kv": lambda si, ri, c, idx=idx: 1.0 if 0.2 < tparam(idx[si]) < tk - 0.02 else 0.0,
            "bt": lambda si, ri, c, idx=idx: tparam(idx[si]),
        }
        return geo.loft(pname, [frames[i] for i in idx], rings, cap_start=caps[0], cap_end=caps[1], corner_attrs=ca,
                        material=mat, merge=True)

    lo_knee = N - 1 - knee_i
    parts = [
        part("ear_lo", 0, lo_knee, ear, caps=("fan", None)),
        part("limb", lo_knee, knee_i, lac),
        part("ear_hi", knee_i, N - 1, ear, caps=(None, "fan")),
    ]
    # Grip binding over the middle, and fitted bands where limb meets ear and at the grip ends.
    g_lo, g_hi = mid - 3, mid + 3
    parts.append(part("grip", g_lo, g_hi, grip_m, pad=0.002, caps=("fan", "fan")))

    def band(bname, i, pad, half=0.007):
        o, xa, ya = frames[i]
        t = xa.cross(ya)
        th, w = thick(tparam(i)) / 2, width(tparam(i)) / 2
        steps = [(-half, 0.4), (-half + 0.0015, 1.0), (half - 0.0015, 1.0), (half, 0.4)]
        return geo.loft(bname, [(o + t * d, xa, ya) for d, _ in steps],
                        [ellipse(th + pad * k, w + pad * k, seg) for _, k in steps], cap_start="fan", cap_end="fan",
                        corner_attrs={"al": lambda si, ri, c: along(c, si, ri)}, material=fit)

    for k, (i, pad) in enumerate(((lo_knee, 0.0015), (knee_i, 0.0015), (g_lo, 0.0035), (g_hi, 0.0035))):
        parts.append(band(f"band{k}", i, pad))
    # Tips: a bone nock cap carrying a small metal bud.
    for k, i in enumerate((0, N - 1)):
        tan = (pts[i] - pts[i - 1 if i else 1]).normalized()
        r = width(1.0) / 2
        parts.append(lathe(f"nock{k}", [(r * 0.9, -0.035), (r * 1.08, -0.03), (r * 1.1, 0.0), (r * 0.8, 0.008), (0.0, 0.01)],
                           bone, seg=10, axis=tan, origin=pts[i]))
        parts.append(lathe(f"finial{k}", [(0.0, 0.0), (0.007, 0.002), (0.008, 0.011), (0.0035, 0.02), (0.0, 0.026)],
                           fit, seg=10, axis=tan, origin=pts[i] + tan * 0.006))

    if name == "sharanga":
        # Sudarshana chakra boss on the side of the grip.
        r0 = width(0) / 2 + 0.003
        parts.append(lathe("chakra", [(0.0, 0.0), (0.024, 0.0), (0.026, 0.003), (0.020, 0.006), (0.010, 0.008), (0.0, 0.009)],
                           fit, seg=24, axis=X, origin=Vector((r0, 0, 0))))
        parts.append(lathe("chakra_b", [(0.0, 0.0), (0.024, 0.0), (0.026, 0.003), (0.020, 0.006), (0.010, 0.008), (0.0, 0.009)],
                           fit, seg=24, axis=-X, origin=Vector((-r0, 0, 0))))
    if name == "pinaka":
        parts += vasuki(pts, frames, mid, thick, width, tparam)
        for k, i in enumerate((0, N - 1)):
            tan = (pts[i] - pts[i - 1 if i else 1]).normalized()
            parts += trishula(f"trident{k}", pts[i] + tan * 0.02, tan, fit)

    # String anchors (glTF coords): the nock just inside each tip, and the bridge where the string
    # leaves the belly of the recurve at rest.
    def gl(v):
        return (round(v.x, 4), round(v.z, 4), round(-v.y, 4))

    belly = Vector((0, -1, 0))
    nock = pts[N - 3] + belly * thick(tparam(N - 3)) * 0.5
    bridge = pts[knee_i] + belly * thick(tk) * 0.5
    rest = Vector((width(0) / 2 + 0.006, 0.0, 0.075))
    data = dict(nock=gl(nock), bridge=gl(bridge), rest=gl(rest), half_len=round(half_len, 3))
    finish(name, parts, res=1024, hard=("band0", "band1", "band2", "band3", "finial0", "finial1"))
    return data


def vasuki(pts, frames, mid, thick, width, tparam):
    """The serpent wound three times round the Pinaka's grip and lower limb, head raised forward."""
    sk = F.scales("pinaka_snake")
    lo, hi = mid - 7, mid + 2
    path, radial = [], []
    turns = 3.5
    for k in range(120):
        u = k / 119
        f = lo + (hi - lo) * u
        i0 = int(math.floor(f))
        i1 = min(i0 + 1, len(pts) - 1)
        a = f - i0
        c = pts[i0].lerp(pts[i1], a)
        _, xa, ya = frames[i0]
        R = max(thick(tparam(i0)), width(tparam(i0))) * 0.5 + 0.0105 + (0.002 if abs(i0 - mid) <= 3 else 0.0)
        phi = 2 * math.pi * turns * u
        r = xa * math.cos(phi) + ya * math.sin(phi)
        path.append(c + r * R)
        radial.append((c + r * R, r))
    # The neck lifts off the bow and the head looks along it toward the target.
    end = path[-1]
    out = (end - pts[hi]).normalized()
    neck = [end + out * 0.016 + Z * 0.02, end + out * 0.026 + Z * 0.05 + Y * 0.012, end + out * 0.024 + Z * 0.075 + Y * 0.04,
            end + out * 0.02 + Z * 0.085 + Y * 0.07]
    full = resample_even(catmull_rom(path + neck, 180), 90)

    def rad(t):
        return interp_keys([(0.0, 0.002), (0.06, 0.0085), (0.78, 0.0095), (0.84, 0.008), (0.9, 0.0065), (0.95, 0.0062), (0.975, 0.0078), (1.0, 0.0)], t)

    def hood(t):
        # The cobra hood spreads sideways behind the head.
        return interp_keys([(0.0, 1.0), (0.86, 1.0), (0.91, 2.0), (0.95, 1.2), (1.0, 1.0)], t)

    # Belly (ac < 0) faces the bow: each frame's y axis is the outward radial of the nearest coil
    # point (the raised neck keeps the last one).
    frames, rings = [], []
    S = len(full)
    for i, p in enumerate(full):
        t = (full[min(i + 1, S - 1)] - full[max(i - 1, 0)]).normalized()
        r = min(radial, key=lambda q: (q[0] - p).length)[1]
        y = (r - t * r.dot(t)).normalized()
        frames.append((p, y.cross(t), y))
        u = i / (S - 1)
        rings.append(ellipse(rad(u) * 0.85, rad(u) * hood(u), 10))
    body = geo.loft("snake", frames, rings, corner_attrs={"al": lambda si, ri, c: along(c, si, ri), "ac": ring_ac(10)},
                    material=sk, merge=True)
    return [body]


def trishula(name, base, tan, mat):
    """A small trident on the bow tip: a central blade and two curling side prongs."""
    side = X
    parts = [tube(f"{name}_c", [base + tan * (0.11 * k / 7) for k in range(8)],
                  lambda t: lerp(0.0065, 0.0006, t ** 1.4), mat, seg=8, up=side, caps=("fan", "fan"), rx=lambda t: lerp(0.003, 0.0004, t))]
    for s in (-1, 1):
        pts = []
        for k in range(10):
            u = k / 9
            pts.append(base + tan * (0.004 + 0.075 * u) + side * s * (0.042 * math.sin(u * math.pi * 0.62) - 0.012 * u * u))
        parts.append(tube(f"{name}_{s}", pts, lambda t: lerp(0.005, 0.0006, t ** 1.3), mat, seg=8, up=tan.cross(side),
                          caps=("fan", "fan")))
    parts.append(lathe(f"{name}_hub", [(0.0, -0.006), (0.010, -0.003), (0.010, 0.008), (0.0, 0.012)], mat, seg=12, axis=tan,
                       origin=base))
    return parts


# ------------------------------------------------------------------ arrow and quiver

ARROW_LEN = 0.82


def arrow_parts(prefix="", origin=Vector(), axis=Z, mats=None):
    """Cane shaft from the nock (origin) to a leaf-shaped steel head at ARROW_LEN along `axis`;
    three barred feathers. `mats` shares materials between the arrows of a quiver."""
    shaft_m, steel, bone, thread, fl = mats
    side = X if abs(axis.dot(X)) < 0.9 else Y
    side = (side - axis * side.dot(axis)).normalized()
    other = axis.cross(side)

    def at(z):
        return origin + axis * z

    parts = [lathe(f"{prefix}shaft", [(0.0, 0.0), (0.0042, 0.004), (0.0045, 0.3), (0.0042, ARROW_LEN - 0.07), (0.0, ARROW_LEN - 0.065)],
                   shaft_m, seg=8, axis=axis, origin=origin)]
    parts.append(lathe(f"{prefix}nock", [(0.0, -0.014), (0.0050, -0.012), (0.0058, -0.004), (0.0050, 0.012), (0.0040, 0.016)],
                       bone, seg=8, axis=axis, origin=origin))
    for z0, z1 in ((0.026, 0.036), (0.168, 0.18), (ARROW_LEN - 0.085, ARROW_LEN - 0.07)):
        parts.append(lathe(f"{prefix}bind{z0:.2f}", [(0.0047, z0), (0.0052, z0 + 0.002), (0.0052, z1 - 0.002), (0.0047, z1)],
                           thread, seg=8, axis=axis, origin=origin))
    # Feathers: thin vanes from the shaft, tallest just behind the middle.
    for k in range(3):
        phi = 2 * math.pi * k / 3 + 0.3
        rad = side * math.cos(phi) + other * math.sin(phi)
        tng = axis.cross(rad)
        S = 14
        frames, rings = [], []
        for s in range(S):
            u = s / (S - 1)
            z = lerp(0.034, 0.17, u)
            h = 0.0015 + 0.0155 * math.sin(math.pi * min(1.0, u * 1.15)) ** 0.7 * (1 - 0.35 * u)
            frames.append((at(z), rad, tng))
            rings.append([(0.0043, -0.00035), (0.0043 + h, -0.00035), (0.0043 + h, 0.00035), (0.0043, 0.00035)])
        parts.append(geo.loft(f"{prefix}feather{k}", frames, rings, corner_attrs={
            "u": lambda si, ri, c: si / (S - 1),
            "v": lambda si, ri, c: -1.0 if ri % 4 in (0, 3) else 1.0}, material=fl, merge=True))
    # Head: a socket, then a flat leaf blade with a raised midrib.
    parts.append(lathe(f"{prefix}socket", [(0.0046, ARROW_LEN - 0.07), (0.0052, ARROW_LEN - 0.06), (0.0040, ARROW_LEN - 0.045)],
                       steel, seg=8, axis=axis, origin=origin))
    S = 10
    frames, rings = [], []
    for s in range(S):
        u = s / (S - 1)
        z = lerp(ARROW_LEN - 0.05, ARROW_LEN, u)
        w = max(0.0004, 0.0095 * math.sin(math.pi * min(1.0, 0.25 + u * 0.85)) ** 0.8 * (1 - u ** 3))
        th = max(0.0003, 0.0022 * (1 - u))
        frames.append((at(z), side, other))
        rings.append([(w, 0.0), (w * 0.5, th * 0.6), (0.0, th), (-w * 0.5, th * 0.6), (-w, 0.0), (-w * 0.5, -th * 0.6),
                      (0.0, -th), (w * 0.5, -th * 0.6)])
    parts.append(geo.loft(f"{prefix}head", frames, rings, cap_start="fan", cap_end="fan", material=steel, merge=True,
                          corner_attrs={"al": lambda si, ri, c: along(c, si, ri)}))
    return parts


def arrow_mats():
    return (F.reed("arrow_shaft"), F.metal("arrow_steel", (0.55, 0.55, 0.56), rough=0.3, tarnish=(0.15, 0.13, 0.11)),
            F.horn("arrow_nock", dark=(0.62, 0.56, 0.45), light=(0.85, 0.80, 0.68)),
            F.wrap("arrow_thread", (0.55, 0.08, 0.05)), F.feather("arrow_feather", (0.55, 0.50, 0.44)))


def arrow():
    finish("arrow", arrow_parts(mats=arrow_mats()), res=512, hard=())


def tarkash():
    """Leather quiver with gold-embroidered bands and a brass mouth, full of arrows."""
    lea = F.leather("tarkash_leather", (0.28, 0.06, 0.05))
    brass = F.metal("tarkash_brass", GOLD, rough=0.3, chase=True)
    L = 0.6
    body = lathe("tube", [(0.0, 0.0), (0.040, 0.004), (0.044, 0.03), (0.050, L - 0.04), (0.052, L)], lea, seg=20)
    # Embroidered bands: separate thin rings so the band attribute has a clean region.
    parts = [body]
    for z in (0.12, 0.42):
        parts.append(lathe(f"emb{z}", [(0.0462, z), (0.0475, z + 0.004), (0.0478, z + 0.05), (0.0466, z + 0.054)], lea, seg=20,
                           extra={"band": lambda si, ri, c: 1.0}))
    parts.append(lathe("mouth", [(0.049, L - 0.035), (0.055, L - 0.03), (0.056, L + 0.004), (0.050, L + 0.008)], brass, seg=20))
    parts.append(lathe("foot", [(0.0, -0.012), (0.03, -0.012), (0.043, -0.002), (0.045, 0.03), (0.041, 0.034)], brass, seg=20))
    mats = arrow_mats()
    for k in range(7):
        a = 2 * math.pi * k / 7
        r = 0.026 if k else 0.0
        o = Vector((r * math.cos(a), r * math.sin(a), L + 0.3 - (k % 3) * 0.03))
        # Fletchings fan out at the mouth; heads gather toward the bottom of the tube.
        tilt = Vector((-math.cos(a) * 0.028, -math.sin(a) * 0.028, -1.0)).normalized()
        parts += arrow_parts(prefix=f"a{k}_", origin=o, axis=tilt, mats=mats)
    # The game holds the quiver at its middle.
    for p in parts:
        p.data.transform(Matrix.Translation(Vector((0, 0, -0.35))))
    finish("tarkash", parts, res=1024, hard=("mouth", "foot"))


# ------------------------------------------------------------------ astras

def astra(name):
    """A short ornamented javelin; Agneyastra carries a three-tongued flame head, Vayavyastra a
    twisted wind-vane head and feathered tail."""
    fire = name == "agneyastra"
    lac = F.lacquer(f"{name}_lac", base=(0.35, 0.05, 0.02) if fire else (0.05, 0.16, 0.24))
    gold = F.metal(f"{name}_gold", GOLD, rough=0.25, chase=True)
    head_m = F.metal(f"{name}_head", COPPER if fire else SILVER, rough=0.22)
    parts = [lathe("shaft", [(0.0, -0.21), (0.009, -0.205), (0.0095, 0.0), (0.0085, 0.24), (0.0, 0.245)], lac, seg=12,
                   extra={"kv": lambda *a: 1.0})]
    for z0 in (-0.21, -0.06, 0.05, 0.215):
        parts.append(lathe(f"ferrule{z0}", [(0.0098, z0), (0.0118, z0 + 0.004), (0.0118, z0 + 0.022), (0.0098, z0 + 0.026)],
                           gold, seg=12))
    parts.append(lathe("pommel", [(0.0, -0.25), (0.008, -0.248), (0.014, -0.236), (0.012, -0.222), (0.0095, -0.212)], gold, seg=12))
    if fire:
        for k in range(3):
            S = 16
            frames, rings = [], []
            for s in range(S):
                u = s / (S - 1)
                # A flame tongue: swells, licks sideways and twists up to a curled point.
                phi = 2 * math.pi * k / 3 + 0.9 * u * u
                out = X * math.cos(phi) + Y * math.sin(phi)
                z = 0.235 + 0.21 * u * (1 - 0.12 * k)
                w = 0.03 * math.sin(math.pi * min(1.0, u * 1.3)) ** 0.6 * (1 - u ** 2) + 0.0005
                lean = 0.022 * math.sin(u * math.pi * 2.0) * u
                c = Vector((0, 0, z)) + out * (0.006 + w * 0.5 + lean)
                frames.append((c, out, out.cross(Z) * -1))
                rings.append([(w * 0.5, 0.0), (0.0, 0.0018), (-w * 0.5, 0.0), (0.0, -0.0018)])
            parts.append(geo.loft(f"flame{k}", frames, rings, cap_start="fan", cap_end="fan", material=head_m, merge=True,
                                  corner_attrs={"al": lambda si, ri, c: along(c, si, ri)}))
        parts.append(lathe("core", [(0.0, 0.23), (0.012, 0.235), (0.010, 0.33), (0.0, 0.42)], head_m, seg=12))
    else:
        # Four vanes twisted half a turn round a spindle.
        for k in range(4):
            S = 18
            frames, rings = [], []
            for s in range(S):
                u = s / (S - 1)
                z = 0.235 + 0.16 * u
                phi = 2 * math.pi * k / 4 + u * math.pi * 0.9
                out = X * math.cos(phi) + Y * math.sin(phi)
                h = 0.022 * math.sin(math.pi * min(1.0, 0.15 + u)) ** 0.6 * (1 - u ** 3) + 0.0008
                frames.append((Vector((0, 0, z)) + out * 0.004, out, out.cross(Z) * -1))
                rings.append([(0.0, -0.0012), (h, -0.0006), (h, 0.0006), (0.0, 0.0012)])
            parts.append(geo.loft(f"vane{k}", frames, rings, cap_start="fan", cap_end="fan", material=head_m, merge=True,
                                  corner_attrs={"al": lambda si, ri, c: along(c, si, ri)}))
        parts.append(lathe("spindle", [(0.0, 0.23), (0.007, 0.235), (0.005, 0.38), (0.0, 0.41)], head_m, seg=10))
        fl = F.feather(f"{name}_feather", (0.85, 0.85, 0.82), bar=(0.25, 0.35, 0.45))
        for k in range(3):
            phi = 2 * math.pi * k / 3
            rad = X * math.cos(phi) + Y * math.sin(phi)
            S = 10
            frames, rings = [], []
            for s in range(S):
                u = s / (S - 1)
                h = 0.004 + 0.03 * math.sin(math.pi * min(1.0, u * 1.1)) ** 0.7
                frames.append((Vector((0, 0, lerp(-0.2, -0.09, u))), rad, Z.cross(rad)))
                rings.append([(0.0095, -0.0005), (0.0095 + h, -0.0005), (0.0095 + h, 0.0005), (0.0095, 0.0005)])
            parts.append(geo.loft(f"fin{k}", frames, rings, material=fl, merge=True, corner_attrs={
                "u": lambda si, ri, c: si / 9, "v": lambda si, ri, c: -1.0 if ri % 4 in (0, 3) else 1.0}))
    finish(name, parts, res=1024, hard=("pommel",))


# ------------------------------------------------------------------ vessels

def sanjeevani():
    """A bundle of the glowing herb: five leafy sprigs bound with red thread."""
    stem_m = F.plain("sanj_stem", (0.22, 0.30, 0.10), rough=0.6)
    leaf_m = F.leaf("sanj_leaf", (0.16, 0.42, 0.10), vein=(0.70, 0.85, 0.35))
    thread = F.wrap("sanj_thread", (0.60, 0.05, 0.04), (0.85, 0.65, 0.15))
    parts = []
    for k in range(5):
        a = 2 * math.pi * k / 5 + 0.4
        lean = Vector((math.cos(a), math.sin(a), 0)) * (0.04 + 0.01 * (k % 2))
        h = 0.2 + 0.03 * ((k * 7) % 3) / 2
        ctrl = [Vector((math.cos(a) * 0.004, math.sin(a) * 0.004, -0.06)), Vector((0, 0, 0.0)) + lean * 0.15,
                Vector((0, 0, h * 0.6)) + lean * 0.6, Vector((0, 0, h)) + lean]
        stem = resample_even(catmull_rom(ctrl, 40), 14)
        parts.append(tube(f"stem{k}", stem, lambda t: lerp(0.0028, 0.0012, t), stem_m, seg=6, caps=("fan", "fan")))
        for j in range(6):
            t = 0.35 + 0.6 * j / 5
            i = int(t * 13)
            base = stem[i]
            tan = (stem[min(i + 1, 13)] - stem[i]).normalized()
            b = a + (j % 2) * math.pi + j * 0.7
            out = (Vector((math.cos(b), math.sin(b), 0)) + tan * 0.6).normalized()
            L = 0.05 - 0.004 * j
            S = 9
            frames, rings = [], []
            nrm = out.cross(tan).normalized()
            for s in range(S):
                u = s / (S - 1)
                w = L * 0.22 * math.sin(math.pi * u) ** 0.8 + 0.0004
                droop = -0.01 * u * u
                c = base + out * (L * u) + Z * droop
                frames.append((c, nrm, out.cross(nrm).normalized()))
                rings.append([(w, 0.0), (0.0, 0.0008), (-w, 0.0), (0.0, -0.0008)])
            parts.append(geo.loft(f"leaf{k}_{j}", frames, rings, cap_start="fan", cap_end="fan", material=leaf_m, merge=True,
                                  corner_attrs={"u": lambda si, ri, c: si / 8,
                                                "v": lambda si, ri, c: {0: 1.0, 1: 0.0, 2: -1.0, 3: 0.0}[ri % 4]}))
    parts.append(lathe("thread", [(0.0, -0.035), (0.010, -0.034), (0.0115, -0.03), (0.0115, -0.005), (0.010, -0.001), (0.0, 0.0)],
                       thread, seg=12))
    finish("sanjeevani", parts, res=1024)


def soma():
    """A small chased-silver lota with a domed stopper."""
    silver = F.metal("soma_silver", SILVER, rough=0.22, chase=True)
    gold = F.metal("soma_gold", GOLD, rough=0.25)
    prof = [(0.0, -0.06), (0.028, -0.06), (0.03, -0.055), (0.026, -0.05), (0.04, -0.035), (0.056, -0.008), (0.054, 0.015),
            (0.036, 0.036), (0.019, 0.05), (0.018, 0.06), (0.025, 0.066), (0.024, 0.07)]
    parts = [lathe("body", prof, silver, seg=24, caps=(None, "fan"))]
    parts.append(lathe("stopper", [(0.0, 0.068), (0.02, 0.069), (0.021, 0.075), (0.012, 0.086), (0.006, 0.09), (0.008, 0.096),
                                   (0.0, 0.102)], gold, seg=16))
    parts.append(lathe("bandA", [(0.0555, -0.006), (0.0575, -0.004), (0.0575, 0.003), (0.0555, 0.005)], gold, seg=24))
    finish("soma", parts, res=1024, hard=("bandA",))


def amrita():
    """The purna kalash: a gold pot with mango leaves round the mouth and a coconut on top, its neck
    tied with mauli thread."""
    gold = F.metal("amrita_gold", GOLD, rough=0.2, chase=True)
    leaf_m = F.leaf("amrita_leaf", (0.12, 0.32, 0.08))
    nut = F.wrap("amrita_coconut", (0.32, 0.20, 0.10), (0.24, 0.14, 0.07), pitch=0.004)
    thread = F.wrap("amrita_thread", (0.70, 0.08, 0.04), (0.90, 0.70, 0.15))
    prof = [(0.0, -0.11), (0.04, -0.11), (0.045, -0.105), (0.04, -0.098), (0.062, -0.08), (0.08, -0.04), (0.077, 0.0),
            (0.055, 0.03), (0.034, 0.045), (0.032, 0.06), (0.05, 0.07), (0.052, 0.076), (0.046, 0.078)]
    parts = [lathe("pot", prof, gold, seg=28, caps=(None, "fan"))]
    parts.append(lathe("mauli", [(0.0335, 0.04), (0.037, 0.043), (0.037, 0.053), (0.0335, 0.056)], thread, seg=20))
    for k in range(5):
        a = 2 * math.pi * k / 5
        out = Vector((math.cos(a), math.sin(a), 0))
        S = 10
        frames, rings = [], []
        base = Vector((0, 0, 0.072)) + out * 0.03
        tip_dir = (out * 0.9 + Z * 0.15).normalized()
        nrm = Z.cross(out).normalized()
        for s in range(S):
            u = s / (S - 1)
            w = 0.075 * 0.17 * math.sin(math.pi * u) ** 0.7 + 0.0005
            c = base + tip_dir * (0.075 * u) - Z * (0.03 * u * u)
            frames.append((c, nrm, tip_dir.cross(nrm).normalized()))
            rings.append([(w, 0.0), (0.0, 0.0009), (-w, 0.0), (0.0, -0.0009)])
        parts.append(geo.loft(f"leaf{k}", frames, rings, cap_start="fan", cap_end="fan", material=leaf_m, merge=True,
                              corner_attrs={"u": lambda si, ri, c: si / 9,
                                            "v": lambda si, ri, c: {0: 1.0, 1: 0.0, 2: -1.0, 3: 0.0}[ri % 4]}))
    parts.append(lathe("coconut", [(0.0, 0.07), (0.03, 0.074), (0.042, 0.095), (0.04, 0.12), (0.025, 0.142), (0.008, 0.15),
                                   (0.0, 0.152)], nut, seg=16))
    finish("amrita", parts, res=1024)


BUILDERS = {
    "gandiva": None, "sharanga": None, "pinaka": None,
    "arrow": arrow, "tarkash": tarkash, "agneyastra": lambda: astra("agneyastra"), "vayavyastra": lambda: astra("vayavyastra"),
    "sanjeevani": sanjeevani, "soma": soma, "amrita": amrita,
}


def write_bows(data):
    path = os.path.join(ROOT, "assets", "data", "bows.ron")
    old = {}
    if os.path.exists(path):
        # Keep entries for bows not rebuilt this run.
        import re
        for m in re.finditer(r'"(\w+)": \((.*?)\),\n', open(path).read()):
            old[m.group(1)] = m.group(2)
    lines = ["// Written by tools/blender/weapons/arsenal.py: string anchors per bow in its glTF frame",
             "// (limbs along Y, target -Z). The bottom limb mirrors y. Do not edit by hand.", "{"]
    for name in BOWS:
        if name in data:
            d = data[name]
            body = (f"nock: {d['nock']}, bridge: {d['bridge']}, rest: {d['rest']}, half_len: {d['half_len']}")
            lines.append(f'    "{name}": ({body}),')
        elif name in old:
            lines.append(f'    "{name}": ({old[name]}),')
    lines.append("}")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else list(BUILDERS)
    bow_data = {}
    for name in names:
        export.reset_scene()
        if name in BOWS:
            bow_data[name] = bow(name)
        else:
            BUILDERS[name]()
    if bow_data:
        write_bows(bow_data)


if __name__ == "__main__":
    main()

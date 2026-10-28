"""Khanda: broad straight Indian sword with a spatulate tip, riveted spine reinforcement and a
Hindu basket hilt (disc pommel with spike, broad knuckle guard). See docs/references.md.

Blender axes (exported Y-up): blade +Z, cutting edge +Y, spine -Y, flats +-X, origin = grip centre.
Run: blender -b --factory-startup -P tools/blender/weapons/khanda.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import bake  # noqa: E402
import export  # noqa: E402
import geo  # noqa: E402
import materials  # noqa: E402
from geo import catmull_rom, clamp01, interp_keys, lerp, smoothstep  # noqa: E402

NAME = "khanda"

# ------------------------------------------------------------------ dimensions (metres)
GUARD_TOP = 0.058          # top of quillon block; blade emerges here
BLADE_ROOT = 0.050         # blade tang shoulder, hidden inside the block
BLADE_LEN = 0.830          # visible ~0.822 (refs: 0.838-0.84)
TIP_ROUND_T = 0.925        # where the spatulate tip starts rounding to its point
PLATE_END_T = 0.50         # reinforcement runs half the blade
PLATE_W = 0.0125
PLATE_TH = 0.0013
FULLER_C = 0.0150          # fuller centre measured from the spine

# (t, spine y, edge y): spine stays straight, the edge narrows mid-blade then flares to the tip.
OUTLINE = [(0.00, -0.0200, 0.0262), (0.07, -0.0200, 0.0256), (0.55, -0.0195, 0.0205),
           (0.80, -0.0208, 0.0252), (0.93, -0.0222, 0.0284), (1.00, -0.0224, 0.0290)]


def outline(t):
    ys = interp_keys([(k[0], k[1]) for k in OUTLINE], t)
    ye = interp_keys([(k[0], k[2]) for k in OUTLINE], t)
    return ys, ye


def blade_params(t):
    return {
        "hs": lerp(0.0027, 0.0015, clamp01(t / 0.9)),                       # spine half-thickness (distal taper)
        "hsh": lerp(0.00105, 0.00072, t),                                   # half-thickness at edge-bevel shoulder
        "e": interp_keys([(0, 0.0006), (0.06, 0.0006), (0.10, 0.00022), (1, 0.00018)], t),  # edge half-thickness
        "b": interp_keys([(0, 0.0045), (0.10, 0.0060), (1, 0.0055)], t),    # edge bevel width
        "fe": smoothstep(0.76, 0.86, t),                                    # false edge on the spine near the tip
        "df": interp_keys([(0, 0), (0.07, 0), (0.13, 0.0010), (0.60, 0.0010), (0.71, 0)], t),  # fuller depth
    }


def half_profile(t, W):
    """(y from spine, x) points on the +X flat, from spine to edge (14 points)."""
    p = blade_params(t)
    hs, hsh, e, b, fe, df = p["hs"], p["hsh"], p["e"], p["b"], p["fe"], p["df"]
    flat_spine = [(0.0, 0.62 * hs), (0.00035, 0.93 * hs), (0.0011, hs)]
    false_edge = [(0.0, 1.3 * e), (0.0025, 0.55 * hs), (0.0055, 0.92 * hs)]
    spine = [(lerp(a[0], c[0], fe), lerp(a[1], c[1], fe)) for a, c in zip(flat_spine, false_edge)]
    ya, xa = spine[-1]
    yb = W - b
    R = max(yb - ya, 1e-5)
    fixed = [0.0095, FULLER_C - 0.0034, FULLER_C - 0.0019, FULLER_C, FULLER_C + 0.0019, FULLER_C + 0.0034]
    groove = [0.0, 0.0, 0.75, 1.0, 0.75, 0.0]
    k = min(1.0, R / 0.0185 * 0.92) if R < 0.0185 else 1.0
    offs = [d * k for d in fixed] + [lerp(fixed[-1] * k, R, 0.4), lerp(fixed[-1] * k, R, 0.75)]
    grooves = groove + [0.0, 0.0]
    bulge = lerp(0.00012, 0.0006, fe)
    flat = []
    for d, gr in zip(offs, grooves):
        u = clamp01(d / R)
        x = lerp(xa, hsh, u) + bulge * math.sin(math.pi * u) - gr * df
        flat.append((ya + d, x))
    bevel = [(yb, hsh), (yb + b * 0.5, lerp(hsh, e, 0.5) + 0.00006), (W, e)]
    return spine + flat + bevel


def blade_x_at(t, y_rel):
    """Blade half-thickness at a distance from the spine (for seating plates and langets)."""
    ys, ye = outline(t)
    prof = half_profile(t, ye - ys)
    for (y0, x0), (y1, x1) in zip(prof, prof[1:]):
        if y_rel <= y1:
            return lerp(x0, x1, clamp01((y_rel - y0) / max(y1 - y0, 1e-9)))
    return prof[-1][1]


def blade_stations():
    ts = [i / 56 * TIP_ROUND_T for i in range(57)]
    for k in range(1, 15):
        ts.append(TIP_ROUND_T + (1 - TIP_ROUND_T) * math.sin(math.pi / 2 * k / 14))
    return ts


def build_blade(mat):
    frames, rings, tvals, roles = [], [], [], None
    ys_r, ye_r = outline(TIP_ROUND_T)
    y_point = lerp(ys_r, ye_r, 0.45)
    for t in blade_stations():
        ys, ye = outline(min(t, TIP_ROUND_T))
        W = ye - ys
        half = half_profile(min(t, TIP_ROUND_T), W)
        apex = W + 0.8 * half[-1][1]
        ring = [(0.0, ys)] + [(x, ys + y) for y, x in half] + [(0.0, ys + apex)] + \
               [(-x, ys + y) for y, x in reversed(half)]
        if t > TIP_ROUND_T:
            s = (t - TIP_ROUND_T) / (1 - TIP_ROUND_T)
            f = max(0.012, max(0.0, 1 - s ** 1.7) ** (1 / 1.7))  # ogival; ends in a ~0.6 mm rounded point
            g = f ** 0.6
            ring = [(a * g, y_point + (b - y_point) * f) for a, b in ring]
        z = BLADE_ROOT + t * BLADE_LEN
        frames.append((Vector((0, 0, z)), Vector((1, 0, 0)), Vector((0, 1, 0))))
        rings.append(ring)
        tvals.append(t)
    n = len(rings[0])
    nh = 14
    # Ring roles: 0 spine centre, 1..14 +X half, 15 edge apex, 16..29 -X half.
    edge_idx = {12, 13, 14, 15, 16, 17, 18}
    spine_idx = {0, 1, 2, 28, 29}

    def ez(si, ri, ctx):
        ri %= n
        fe = blade_params(tvals[si])["fe"]
        if ri in edge_idx:
            return 1.0 if ri not in (12, 18) else 0.5
        if ri in spine_idx:
            return fe
        return 0.0

    splits = [next(i for i, t in enumerate(tvals) if t >= f) for f in (0.34, 0.67)]
    obj = geo.loft("blade", frames, rings, cap_start="fan", cap_end="fan", uv_breaks=(0, nh + 1), v_splits=splits,
                   corner_attrs={"ez": ez, "bt": lambda si, ri, c: tvals[si]}, material=mat, merge=True)
    return obj


def slab_ring(x_in_fn, ys, th, outward):
    """Thin plate cross-section over the blade surface. ys: across samples low->high (4+).
    x_in_fn(y) gives the surface it sits on. Returns a CCW ring (mirrored for -X)."""
    y0, y3 = ys[0], ys[-1]
    w = y3 - y0
    c = min(0.0004, w * 0.2)
    xin = [x_in_fn(y) for y in ys]
    xo = [x + th for x in xin]
    xm0, xm3 = xin[0] + th * 0.5, xin[-1] + th * 0.5
    ring = [(xin[i], ys[i]) for i in reversed(range(len(ys)))]
    ring += [(xm0, y0 - c * 0.5)]
    ring += [(xo[0] - c * 0.5, y0 + c * 0.2), (xo[0], y0 + c)]
    ring += [(xo[i], ys[i]) for i in range(1, len(ys) - 1)]
    ring += [(xo[-1], y3 - c), (xo[-1] - c * 0.5, y3 - c * 0.2)]
    ring += [(xm3, y3 + c * 0.5)]
    if outward < 0:
        ring = [(-a, b) for a, b in reversed(ring)]
    return ring


def plate_width(t):
    """Cusped (multi-lobed) terminal like a carved arch; collapses to a point at the end."""
    lobe_start = PLATE_END_T - 0.055
    if t <= lobe_start:
        return PLATE_W
    s = clamp01((t - lobe_start) / (PLATE_END_T - lobe_start))
    return PLATE_W * (1 - s) ** 0.65 * (0.72 + 0.28 * abs(math.cos(3 * math.pi * s)))


def build_plates(mat):
    objs = []
    z_start = BLADE_ROOT + 0.002
    zs = [lerp(z_start, BLADE_ROOT + PLATE_END_T * BLADE_LEN, (i / 30)) for i in range(31)]
    # Denser stations through the lobes.
    lobe_z = BLADE_ROOT + (PLATE_END_T - 0.055) * BLADE_LEN
    zs = sorted(set([z for z in zs if z < lobe_z] + [lerp(lobe_z, zs[-1], i / 24) for i in range(25)]))
    for side in (1, -1):
        frames, rings, meta = [], [], []
        for z in zs:
            t = (z - BLADE_ROOT) / BLADE_LEN
            ys, _ = outline(t)
            w = max(plate_width(t), 1e-6)
            across = [ys - 0.0002 + w * k / 3 for k in range(4)]
            ring = slab_ring(lambda y: blade_x_at(t, y - ys) - 0.00005, across, PLATE_TH, side)
            frames.append((Vector((0, 0, z)), Vector((1, 0, 0)), Vector((0, 1, 0))))
            rings.append(ring)
            meta.append((ys - 0.0002 + w * 0.5, w * 0.5))

        def ac(si, ri, ctx, rings=rings, meta=meta):
            b = rings[si][ri % len(rings[si])][1]
            c, hw = meta[si]
            return max(-1.0, min(1.0, (b - c) / max(hw, 1e-6)))

        obj = geo.loft(f"plate_{'p' if side > 0 else 'n'}", frames, rings, cap_start="fan", merge=True,
                       material=mat, corner_attrs={
                           "ac": ac, "hw": lambda si, ri, c, meta=meta: meta[si][1],
                           "al": lambda si, ri, c, zs=zs: zs[si] - zs[0],
                           "koft": lambda *a: 1.0, "kv": lambda *a: 1.0})
        objs.append(obj)
    return objs


def build_rivets(mat):
    objs = []
    for t in (0.13, 0.27, 0.41):
        ys, _ = outline(t)
        y = ys + 0.0060
        z = BLADE_ROOT + t * BLADE_LEN
        x = blade_x_at(t, 0.006) + PLATE_TH - 0.0002
        for side in (1, -1):
            prof = [(0.0024, -0.0002), (0.0023, 0.0002), (0.0019, 0.00058), (0.0011, 0.00082), (0.0, 0.0009)]
            objs.append(geo.lathe(f"rivet_{t}_{side}", prof, 16, cap_start="fan", material=mat,
                                  z_axis=Vector((side, 0, 0)), origin=Vector((side * x, y, z))))
    return objs


def build_langets(mat):
    objs = []
    length = 0.062
    zs = [GUARD_TOP - 0.006 + length * (1 - math.cos(math.pi / 2 * i / 22)) for i in range(23)]
    for side in (1, -1):
        frames, rings, meta = [], [], []
        for z in zs:
            t = (z - BLADE_ROOT) / BLADE_LEN
            ys, ye = outline(t)
            s = clamp01((z - GUARD_TOP) / (zs[-1] - GUARD_TOP))
            # Ogee tongue: full width, slight waist, then a drawn-out point.
            w = 0.031 * max(0.0, (1 - s ** 2.2)) ** 0.9 * (1 - 0.12 * math.sin(math.pi * min(s * 1.6, 1)))
            w = max(w, 1e-6)
            yc = 0.5 * (ys + ye) - 0.001
            across = [yc - w / 2 + w * k / 5 for k in range(6)]
            pw = PLATE_W

            def seat(y, t=t, ys=ys, pw=pw):
                plate = PLATE_TH * smoothstep(ys + pw + 0.0015, ys + pw, y)
                return blade_x_at(t, max(y - ys, 0.0)) + plate - 0.00005

            rings.append(slab_ring(seat, across, 0.0016, side))
            frames.append((Vector((0, 0, z)), Vector((1, 0, 0)), Vector((0, 1, 0))))
            meta.append((yc, w / 2))

        def ac(si, ri, ctx, rings=rings, meta=meta):
            b = rings[si][ri % len(rings[si])][1]
            return max(-1.0, min(1.0, (b - meta[si][0]) / max(meta[si][1], 1e-6)))

        objs.append(geo.loft(f"langet_{side}", frames, rings, cap_start="fan", merge=True, material=mat,
                             corner_attrs={"ac": ac, "hw": lambda si, ri, c, meta=meta: meta[si][1],
                                           "al": lambda si, ri, c, zs=zs: zs[si] - zs[0],
                                           "koft": lambda *a: 1.0, "kv": lambda *a: 1.0}))
    return objs


def rounded_rect(hx, hy, r, per=4):
    pts = []
    for cx, cy, a0 in ((hx - r, hy - r, 0), (-hx + r, hy - r, 90), (-hx + r, -hy + r, 180), (hx - r, -hy + r, 270)):
        for k in range(per):
            a = math.radians(a0 + 90 * k / (per - 1))
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    # rotate start so index 0 is mid-side (cleaner seams)
    return pts


def build_guard(mat):
    # Quillon block: rounded box along Y, frames x_axis=Z, y_axis=X (tangent +Y).
    ys = [-0.031 + 0.062 * i / 30 for i in range(31)]
    frames, rings = [], []
    zc = 0.5 * (0.0445 + GUARD_TOP)
    for y in ys:
        s = abs(y) / 0.031
        # Plan: pointed oval swelling over the grip; elevation: thins toward the ends.
        hx = 0.0072 + 0.0088 * max(0.0, 1 - s * s) ** 0.75
        hz = 0.0068 - 0.0022 * s * s
        k = max(0.0, 1 - max(0.0, (s - 0.88) / 0.12) ** 2) ** 0.5
        k = max(k, 0.05)
        rings.append([(a * k, b * (0.8 + 0.2 * k)) for a, b in rounded_rect(hz, hx, min(0.0028, hz * 0.6))])
        frames.append((Vector((0, y, zc)), Vector((0, 0, 1)), Vector((1, 0, 0))))
    hw_block = 0.0155

    def ac_ring(si, ri, ctx, rings=rings):
        b = rings[si][ri % len(rings[si])][1]
        return max(-1.0, min(1.0, b / hw_block))

    block = geo.loft("guard_block", frames, rings, cap_start="fan", cap_end="fan", material=mat,
                     corner_attrs={"ac": ac_ring, "hw": lambda *a: hw_block,
                                   "al": lambda si, ri, c, ys=ys: ys[si], "koft": lambda *a: 1.0,
                                   "kv": lambda *a: 0.0})

    # Spine-side quillon ending in a flattened bud; tangent -Y => x_axis=X, y_axis=Z.
    qy = [-0.022 - 0.040 * i / 28 for i in range(29)]
    frames, rings = [], []
    for y in qy:
        s = (-0.022 - y) / 0.040
        r = interp_keys([(0, 0.0058), (0.30, 0.0046), (0.42, 0.0046), (0.72, 0.0084), (0.86, 0.0072)], s)
        r += 0.0011 * math.exp(-((s - 0.36) / 0.035) ** 2)            # collar
        if s > 0.86:
            r *= max(0.0, 1 - ((s - 0.86) / 0.14) ** 2) ** 0.5
        frames.append((Vector((0, y, zc)), Vector((1, 0, 0)), Vector((0, 0, 1))))
        rings.append(geo.ellipse(r, r * 0.72, 18))
    quillon = geo.loft("quillon", frames, rings, cap_start="fan", merge=True, material=mat,
                       corner_attrs={"ac": lambda si, ri, c, qy=qy: lerp(-1, 1, si / (len(qy) - 1)),
                                     "hw": lambda *a: 0.02, "al": lambda si, ri, c: c["ring_arc"][si][ri],
                                     "koft": lambda *a: 1.0, "kv": lambda *a: 0.0})
    return [block, quillon]


def build_knuckle_guard(mat):
    ctrl = [Vector(p) for p in ((0, 0.024, 0.0515), (0, 0.040, 0.0490), (0, 0.055, 0.0380), (0, 0.0645, 0.016),
                                (0, 0.0660, -0.008), (0, 0.0610, -0.030), (0, 0.0505, -0.0455),
                                (0, 0.0365, -0.0535))]
    pts = geo.resample_even(catmull_rom(ctrl, 200), 42)
    frames = geo.path_frames(pts, Vector((1, 0, 0)))
    rings, meta = [], []
    th = 0.0034
    for i in range(len(pts)):
        s = i / (len(pts) - 1)
        w = 0.016 + 0.030 * math.sin(math.pi * s) ** 1.3
        hw = w / 2
        outer = []
        for k in range(9):
            b = -hw + w * (k + 0.5) / 9
            u = b / hw
            rib = 0.0007 * math.exp(-(u / 0.16) ** 2)
            outer.append((th / 2 + 0.0004 * (1 - u * u) + rib, b))
        inner = [(-th / 2, hw - w * (k + 0.5) / 5) for k in range(5)]
        c = 0.0007
        ring = inner + [(-th / 2 + 0.0004, -hw - c * 0.4), (0.0, -hw - c), (th / 2 - 0.0002, -hw - c * 0.5)] + \
            outer + [(th / 2 - 0.0002, hw + c * 0.5), (0.0, hw + c), (-th / 2 + 0.0004, hw + c * 0.4)]
        rings.append(ring)
        meta.append(hw)
    arc = [0.0]
    for a, b in zip(pts, pts[1:]):
        arc.append(arc[-1] + (b - a).length)

    def ac(si, ri, ctx, rings=rings, meta=meta):
        return max(-1.0, min(1.0, rings[si][ri % len(rings[si])][1] / meta[si]))

    return geo.loft("knuckle_guard", frames, rings, cap_start="fan", cap_end="fan", material=mat,
                    v_splits=(21,),
                    corner_attrs={"ac": ac, "hw": lambda si, ri, c, meta=meta: meta[si],
                                  "al": lambda si, ri, c, arc=arc: arc[si],
                                  "koft": lambda *a: 1.0, "kv": lambda *a: 1.0})


def around_attr(periods, period):
    """'al' around a closed ring, scaled so a whole number of scroll periods fits (no seam)."""
    return lambda si, ri, c: c["ring_arc"][si][ri] / max(c["ring_len"][si], 1e-9) * periods * period


def build_grip(mat):
    z0, z1 = -0.049, 0.047
    zs = [lerp(z0, z1, i / 34) for i in range(35)]
    rings = []
    for z in zs:
        s = (z - z0) / (z1 - z0)
        r = 0.0118 + 0.0036 * math.sin(math.pi * s) ** 1.4
        for zc, amp, wid in ((-0.0062, 0.0009, 0.0011), (0.0062, 0.0009, 0.0011), (-0.0435, 0.0008, 0.0013),
                             (0.0415, 0.0008, 0.0013)):
            r += amp * math.exp(-((z - zc) / wid) ** 2)
        rings.append(geo.ellipse(r, r * 1.12, 24))
    frames = geo.axis_frames([(0, 0, z) for z in zs])
    zc, hl = 0.5 * (z0 + z1), 0.5 * (z1 - z0)
    return geo.loft("grip", frames, rings, cap_start="fan", cap_end="fan", material=mat, uv_breaks=(0,),
                    corner_attrs={"ac": lambda si, ri, c: (zs[si] - zc) / hl, "hw": lambda *a: hl,
                                  "al": around_attr(9, 0.0085), "koft": lambda *a: 1.0, "kv": lambda *a: 0.0,
                                  "kb": lambda *a: 1.0})


def build_pommel(mat):
    prof = [(0.0085, -0.0715), (0.0120, -0.0680), (0.0200, -0.0635), (0.0290, -0.0600), (0.0350, -0.0578),
            (0.0385, -0.0562), (0.0398, -0.0548), (0.0396, -0.0534), (0.0384, -0.0526), (0.0360, -0.0530),
            (0.0280, -0.0540), (0.0200, -0.0538), (0.0140, -0.0525), (0.0115, -0.0505), (0.0108, -0.0480)]
    r_in, r_out = 0.0115, 0.0398

    def ac(si, ri, c):
        return lerp(-1, 1, clamp01((prof[si][0] - r_in) / (r_out - r_in)))

    return geo.lathe("pommel_disc", prof, 40, cap_start="fan", cap_end="fan", material=mat,
                     corner_attrs={"ac": ac, "hw": lambda *a: (r_out - r_in) / 2, "al": around_attr(9, 0.019),
                                   "koft": lambda *a: 1.0, "kv": lambda *a: 1.0})


def build_spike(mat):
    ctrl = [Vector(p) for p in ((0, 0, -0.0690), (0, 0.0008, -0.090), (0, 0.0035, -0.110), (0, 0.0072, -0.1285))]
    pts = geo.resample_even(catmull_rom(ctrl, 100), 28)
    frames = geo.path_frames(pts, Vector((1, 0, 0)))
    rings, svals = [], []
    for i in range(len(pts)):
        s = i / (len(pts) - 1)
        r = interp_keys([(0, 0.0064), (0.55, 0.0036), (0.70, 0.0034), (0.86, 0.0056), (0.93, 0.0050)], s)
        r += 0.0013 * math.exp(-((s - 0.07) / 0.03) ** 2) + 0.0007 * math.exp(-((s - 0.66) / 0.025) ** 2)
        if s > 0.93:
            r *= max(0.0, 1 - ((s - 0.93) / 0.07) ** 2) ** 0.5
        rings.append(geo.rounded_polygon(8, r, r * 0.22, per_corner=2, phase=math.pi / 8))
        svals.append(s)
    return geo.loft("spike", frames, rings, cap_start="fan", merge=True, material=mat,
                    corner_attrs={"ac": lambda si, ri, c: lerp(-1, 1, svals[si]), "hw": lambda *a: 0.03,
                                  "al": around_attr(2, 0.019), "koft": lambda *a: 1.0, "kv": lambda *a: 0.0})


def main():
    export.reset_scene()
    steel = materials.wootz_steel(f"{NAME}_blade")
    iron = materials.koftgari_iron(f"{NAME}_hilt")

    parts = [build_blade(steel)]
    parts += build_plates(iron) + build_rivets(iron) + build_langets(iron)
    parts += build_guard(iron) + [build_knuckle_guard(iron), build_grip(iron), build_pommel(iron), build_spike(iron)]
    for p in parts:
        geo.finish_hard_surface(p, bevel_width=0.0003 if p.name.startswith("rivet") else 0.0004)
        geo.apply_all_modifiers(p)
    print(f"[{NAME}] part tris: " + ", ".join(f"{p.name}={geo.tri_count(p)}" for p in parts
                                               if not p.name.startswith("rivet")))
    lod0 = geo.join(parts, f"{NAME}_LOD0")
    export.pack_uvs(lod0)

    device = bake.setup_cycles()
    print(f"[{NAME}] baking on {device}, LOD0 tris={geo.tri_count(lod0)}")
    tex_dir = os.path.join(ROOT, "build", "textures", NAME)
    images = bake.bake_texture_set(lod0, tex_dir, NAME, res=int(os.environ.get("BAKE_RES", "2048")))

    lod0.data.materials.clear()
    lod0.data.materials.append(export.game_material(NAME, images))
    export.strip_bake_data(lod0)
    export.triangulate(lod0)
    lods = export.make_lods(lod0, (0.40, 0.15))
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ROOT, "build", f"{NAME}.blend"))
    counts = export.export_glb(lods, os.path.join(ROOT, "assets", "models", "weapons", f"{NAME}.glb"))
    print(f"[{NAME}] exported tris: {counts}")


if __name__ == "__main__":
    main()

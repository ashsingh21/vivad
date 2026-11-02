"""Two-wheelers for the street kit (built and exported by street_kit.py): five motorcycles and a
bicycle of the kinds every Indian street is lined with. Shapes only, no makers' names or badges:

  thumper    the big single: round tank, sprung saddle, wire wheels, long chrome peashooter
  commuter   the 100 cc everyman: flat seat, small tank, visor lamp, black alloys
  discover   the commuter-sport: tank shrouds, blue, alloys
  twostroke  the little 2-stroke: teardrop tank, chrome guards, fat expansion-chamber pipe
  sport150   the 150 cc sports commuter: sharp shrouds, twin pilot lamps, belly pan, upswept pipe
  roadster   the black roadster bicycle: 28-inch wheels, rod brakes, carrier, sprung saddle

Motorcycles stand parked on their side stands, leaning 7 degrees to the left. Model frame: +Y
forward, +Z up, metres; the ground at z = 0.

First pass (blockout): proportions and silhouettes; detail and polish come later.
"""

import math

import bmesh
from mathutils import Vector

TYRE = (0.05, 0.05, 0.05)
CHROME = (0.80, 0.80, 0.82)
BLACK = (0.07, 0.07, 0.08)
ALLOY = (0.22, 0.22, 0.24)
ENGINE = (0.48, 0.49, 0.50)
RED_LAMP = (0.80, 0.08, 0.06)
LAMP = (1.0, 0.95, 0.80)
PLATE = (0.95, 0.94, 0.90)
SEAT = (0.09, 0.08, 0.08)


def _new(p, colour):
    """Paint the faces just made, then turn them outward (which uses the same face tags paint does)."""
    fresh = [f for f in p.bm.faces if not f.tag]
    p.paint(colour)
    bmesh.ops.recalc_face_normals(p.bm, faces=fresh)
    for f in fresh:
        f.tag = True


def torus(p, y, z, R, r, colour, x=0.0, seg=24, ring=6):
    """A ring in the YZ plane (a tyre, a rim) round (x, y, z)."""
    bm = p.bm
    rings = []
    for i in range(seg):
        a = 2 * math.pi * i / seg
        loop = []
        for j in range(ring):
            b = 2 * math.pi * j / ring
            rr = R + r * math.cos(b)
            loop.append(bm.verts.new((x + r * math.sin(b), y + rr * math.cos(a), z + rr * math.sin(a))))
        rings.append(loop)
    for i in range(seg):
        for j in range(ring):
            bm.faces.new((rings[i][j], rings[i][(j + 1) % ring], rings[(i + 1) % seg][(j + 1) % ring], rings[(i + 1) % seg][j]))
    _new(p, colour)


def arc_band(p, y, z, r0, r1, a0, a1, hw, colour, seg=10):
    """A curved band round (y, z) between radii r0 and r1, from angle a0 to a1 (degrees, 0 = +Y,
    90 = up), hw either side of x = 0: a mudguard, a chain guard's curve."""
    bm = p.bm
    secs = []
    for i in range(seg + 1):
        a = math.radians(a0 + (a1 - a0) * i / seg)
        c, s = math.cos(a), math.sin(a)
        secs.append([bm.verts.new((x, y + r * c, z + r * s)) for (x, r) in ((-hw, r0), (hw, r0), (hw, r1), (-hw, r1))])
    for i in range(seg):
        a, b = secs[i], secs[i + 1]
        for k in range(4):
            bm.faces.new((a[k], a[(k + 1) % 4], b[(k + 1) % 4], b[k]))
    bm.faces.new(secs[0])
    bm.faces.new(list(reversed(secs[-1])))
    _new(p, colour)


def _spline(secs, steps):
    """Sections resampled along a Hermite spline (centre, half-width, half-height and squareness
    alike): `steps` rings per span, so a body curves smoothly through its sections instead of
    kinking at each one. Each span's tangents are Catmull-Rom's, clamped to that span's own
    length, so a short span next to a long one doesn't overshoot (it buried the auto's tail lamps
    inside its tub)."""
    vals = [(Vector(c), hw, hh, sq) for c, hw, hh, sq in secs]
    if len(vals) < 3 or steps <= 1:
        return vals

    def tangent(a, b, span):
        m = (b - a) * 0.5
        size = m.length if isinstance(m, Vector) else abs(m)
        limit = span.length if isinstance(span, Vector) else abs(span)
        return m * (limit / size) if size > limit and size > 0 else m

    out = []
    for i in range(len(vals) - 1):
        v0, v1, v2, v3 = vals[max(i - 1, 0)], vals[i], vals[i + 1], vals[min(i + 2, len(vals) - 1)]
        for k in range(steps):
            t = k / steps
            t2, t3 = t * t, t * t * t
            h00, h10, h01, h11 = 2 * t3 - 3 * t2 + 1, t3 - 2 * t2 + t, -2 * t3 + 3 * t2, t3 - t2

            def mix(j):
                span = v2[j] - v1[j]
                return h00 * v1[j] + h10 * tangent(v0[j], v2[j], span) + h01 * v2[j] + h11 * tangent(v1[j], v3[j], span)
            out.append((mix(0), max(mix(1), 0.001), max(mix(2), 0.001), min(max(mix(3), 0.2), 1.0)))
    out.append(vals[-1])
    return out


def loft(p, secs, colour, seg=24, steps=4):
    """A smooth body through a run of sections: each (centre, half-width, half-height, squareness),
    the section square-ish at squareness 0.3, elliptical at 1, set across the path (its width
    along X; its height across the path's direction and X), splined between them. Ends capped.
    Tanks, panels, bodywork."""
    bm = p.bm
    secs = _spline(secs, steps)
    pts = [Vector(c) for c, *_ in secs]
    rings = []
    for i, (c, hw, hh, sq) in enumerate(secs):
        t = (pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]).normalized()
        up = Vector((1, 0, 0)).cross(t).normalized()
        ring = []
        for j in range(seg):
            a = 2 * math.pi * j / seg
            ca, sa = math.cos(a), math.sin(a)
            ring.append(bm.verts.new(pts[i] + Vector((1, 0, 0)) * hw * math.copysign(abs(ca) ** sq, ca) + up * hh * math.copysign(abs(sa) ** sq, sa)))
        rings.append(ring)
    for i in range(len(rings) - 1):
        for j in range(seg):
            bm.faces.new((rings[i][j], rings[i][(j + 1) % seg], rings[i + 1][(j + 1) % seg], rings[i + 1][j]))
    bm.faces.new(rings[0])
    bm.faces.new(list(reversed(rings[-1])))
    _new(p, colour)


def arch(p, secs, thick, colour, a0=0.15, seg=20, steps=4):
    """An open arched shell (a canopy): the top of each section only, from angle a0 up over and
    down to pi - a0, `thick` deep, splined between sections like a loft. Open underneath."""
    bm = p.bm
    secs = _spline(secs, steps)
    rings = []
    for c, hw, hh, sq in secs:
        c = Vector(c)
        outer, inner = [], []
        for j in range(seg + 1):
            a = a0 + (math.pi - 2 * a0) * j / seg
            ca, sa = math.cos(a), math.sin(a)
            px, pz = math.copysign(abs(ca) ** sq, ca), math.copysign(abs(sa) ** sq, sa)
            outer.append(bm.verts.new(c + Vector((hw * px, 0, hh * pz))))
            inner.append(bm.verts.new(c + Vector(((hw - thick) * px, 0, (hh - thick) * pz))))
        rings.append((outer, inner))
    for (o0, i0), (o1, i1) in zip(rings, rings[1:]):
        for j in range(seg):
            bm.faces.new((o0[j], o0[j + 1], o1[j + 1], o1[j]))
            bm.faces.new((i0[j + 1], i0[j], i1[j], i1[j + 1]))
        for j in (0, seg):
            bm.faces.new((o0[j], i0[j], i1[j], o1[j]))
    for o, i in (rings[0], rings[-1]):
        for j in range(seg):
            bm.faces.new((o[j], i[j], i[j + 1], o[j + 1]))
    _new(p, colour)


def arch_panel(p, y, base, c, hw, hh, sq, thick, colour, seg=20):
    """A panel closing an arch's end: its outline the arch's top over straight sides down to
    `base`, `thick` deep along Y (the canvas back of an auto's canopy)."""
    bm = p.bm
    outline = [(hw, base)]
    for j in range(seg + 1):
        a = math.pi * j / seg
        ca, sa = math.cos(a), math.sin(a)
        outline.append((hw * math.copysign(abs(ca) ** sq, ca), c + hh * math.copysign(abs(sa) ** sq, sa)))
    outline.append((-hw, base))
    front = [bm.verts.new((x, y + thick / 2, z)) for x, z in outline]
    back = [bm.verts.new((x, y - thick / 2, z)) for x, z in outline]
    bm.faces.new(front)
    bm.faces.new(list(reversed(back)))
    n = len(outline)
    for j in range(n):
        bm.faces.new((front[j], back[j], back[(j + 1) % n], front[(j + 1) % n]))
    _new(p, colour)


def cushion(p, y0, y1, z, w0, w1, h0, h1, colour, seg=14, n=10, crown=0.35):
    """A padded seat from y0 (its nose) to y1 (its tail) sitting on z: half-width w and height h
    blending from nose to tail, a rounded-square section (fuller over the top: `crown`), the ends
    rounded off. Smooth-shaded, so it reads as foam and vinyl, not a box."""
    bm = p.bm
    rings = []
    for i in range(n + 1):
        u = i / n
        # Ends drawn in like a cushion's: a quarter-circle over the last 15% each end.
        e = min(u, 1 - u) / 0.15
        end = math.sqrt(max(0.0, 1 - (1 - min(e, 1.0)) ** 2)) if e < 1 else 1.0
        end = 0.25 + 0.75 * end
        y = y0 + (y1 - y0) * u
        w = (w0 + (w1 - w0) * u) * end
        h = (h0 + (h1 - h0) * u) * (0.4 + 0.6 * end)
        ring = []
        for j in range(seg):
            t = 2 * math.pi * j / seg
            c, sn = math.cos(t), math.sin(t)
            # Superellipse: square-ish below, rounder on top.
            px = w * math.copysign(abs(c) ** 0.5, c)
            pz = math.copysign(abs(sn) ** (0.5 if sn < 0 else 0.5 + crown), sn)
            ring.append(bm.verts.new((px, y, z + h * 0.5 * (1 + pz))))
        rings.append(ring)
    for i in range(n):
        for j in range(seg):
            bm.faces.new((rings[i][j], rings[i][(j + 1) % seg], rings[i + 1][(j + 1) % seg], rings[i + 1][j]))
    bm.faces.new(rings[0])
    bm.faces.new(list(reversed(rings[-1])))
    _new(p, colour)


def wheel(p, y, R, tyre, wire, rim, hub_r=0.06, spokes=36):
    torus(p, y, R, R - tyre * 0.5, tyre * 0.5, TYRE)
    rim_r = R - tyre - 0.008
    torus(p, y, R, rim_r, 0.014, rim, ring=4)
    p.cyl((-0.06, y, R), (0.06, y, R), hub_r, rim if not wire else CHROME, 12)
    if wire:
        for k in range(spokes):
            a = 2 * math.pi * (k + 0.5 * (k % 2)) / spokes
            side = 0.045 if k % 2 else -0.045
            p.cyl((side, y + hub_r * 0.8 * math.cos(a + 0.3), R + hub_r * 0.8 * math.sin(a + 0.3)),
                  (0.0, y + rim_r * math.cos(a), R + rim_r * math.sin(a)), 0.0035, CHROME, 3)
    else:
        for k in range(5):
            a = 2 * math.pi * k / 5
            p.cyl((0.0, y + hub_r * math.cos(a), R + hub_r * math.sin(a)), (0.0, y + rim_r * math.cos(a), R + rim_r * math.sin(a)), 0.016, rim, 4)


def lean(p, degrees=7.0):
    """Tilt the whole piece to the left about the line where the tyres meet the ground."""
    t = math.radians(degrees)
    c, s = math.cos(t), math.sin(t)
    for v in p.bm.verts:
        x, y, z = v.co
        v.co = Vector((x * c - z * s, y, x * s + z * c))


def motorbike(Piece, name, k):
    p = Piece(name)
    R, wb = k["R"], k["wb"]
    yr, yf = -wb / 2, wb / 2
    wheel(p, yr, R, k["tyre_r"], k["wire"], k["rim"], k.get("hub", 0.06))
    wheel(p, yf, R, k["tyre_f"], k["wire"], k["rim"], k.get("hub", 0.06))
    # Fork: from the front axle up and back at the rake to the headstock.
    rake = math.radians(k.get("rake", 27))
    up = Vector((0.0, -math.sin(rake), math.cos(rake)))
    axle = Vector((0.0, yf, R))
    head = axle + up * k["fork"]
    for x in (-0.085, 0.085):
        p.cyl(axle + Vector((x, 0, 0)), head + Vector((x, 0, -0.05)), 0.02, CHROME, 8)
        p.cyl(axle + Vector((x, 0, 0)), axle + up * 0.30 + Vector((x, 0, 0)), 0.03, k.get("slider", ALLOY), 8)
    p.cyl(head + Vector((-0.1, 0, -0.06)), head + Vector((0.1, 0, -0.06)), 0.03, BLACK, 6)
    # Handlebar and grips.
    # Two risers up from the top clamp, the bar through their tops: straight across the middle, then
    # swept back and a little up to the grips.
    bar = head + Vector((0, -0.04, 0.09))
    hw = k.get("bar", 0.36)
    bar_col = CHROME if k.get("chrome_bar", True) else BLACK
    for x in (-0.05, 0.05):
        p.cyl(head + Vector((x, 0, -0.07)), bar + Vector((x, 0, 0)), 0.014, CHROME, 6)
    p.cyl(bar + Vector((-0.12, 0, 0)), bar + Vector((0.12, 0, 0)), 0.013, bar_col, 6)
    for x in (-1, 1):
        bend = bar + Vector((x * 0.12, 0, 0))
        end = bar + Vector((x * hw, -0.07, 0.03))
        p.ball(bend, 0.013, bar_col, 1)
        p.cyl(bend, end, 0.013, bar_col, 6)
        p.cyl(bar + Vector((x * hw * 0.74, -0.05, 0.022)), bar + Vector((x * hw * 1.04, -0.072, 0.031)), 0.019, BLACK, 8)
    # Mirrors on stalks, angled out.
    for x in (-1, 1):
        root = bar + Vector((x * hw * 0.6, -0.04, 0.02))
        head_m = root + Vector((x * 0.07, 0.02, 0.22))
        p.cyl(root, head_m, 0.006, BLACK, 4)
        p.cyl(head_m + Vector((0, -0.012, 0.03)), head_m + Vector((0, 0.008, 0.03)), 0.05, BLACK, 12)
        p.cyl(head_m + Vector((0, -0.016, 0.03)), head_m + Vector((0, -0.012, 0.03)), 0.044, CHROME, 12)
    # Headlamp, held off the fork legs by a bracket each side.
    lamp = head + Vector((0, 0.12, -0.10))
    style = k["lamp"]
    s_lamp = (lamp.z - R) / up.z
    for x in (-0.085, 0.085):
        p.cyl(axle + up * s_lamp + Vector((x, 0, 0)), lamp + Vector((x * 0.9, -0.06, 0)), 0.012, BLACK, 4)
    if style == "round":
        p.cyl(lamp + Vector((0, -0.08, 0)), lamp, 0.095, k.get("lamp_shell", CHROME), 16)
        p.cyl(lamp, lamp + Vector((0, 0.012, 0)), 0.085, LAMP, 16)
    elif style == "visor":
        # A cowl round the lamp, its visor peaked over it.
        loft(p, [(lamp + Vector((0, -0.10, 0.01)), 0.09, 0.10, 0.5), (lamp + Vector((0, -0.02, 0.01)), 0.11, 0.11, 0.45),
                 (lamp + Vector((0, 0.03, 0.02)), 0.10, 0.10, 0.5)], k["paint"])
        loft(p, [(lamp + Vector((0, -0.02, 0.10)), 0.08, 0.025, 0.5), (lamp + Vector((0, 0.06, 0.12)), 0.06, 0.012, 0.6)], k["paint"])
        p.cyl(lamp + Vector((0, 0.02, -0.01)), lamp + Vector((0, 0.045, -0.01)), 0.075, LAMP, 16)
    else:  # twin pilot lamps under a sharp cowl
        # A sharp fairing, its nose low, raked back to a small screen.
        loft(p, [(lamp + Vector((0, 0.06, -0.06)), 0.10, 0.07, 0.4), (lamp + Vector((0, -0.02, 0.02)), 0.14, 0.14, 0.35),
                 (lamp + Vector((0, -0.10, 0.10)), 0.13, 0.12, 0.4)], k["paint"])
        loft(p, [(lamp + Vector((0, -0.04, 0.14)), 0.09, 0.02, 0.6), (lamp + Vector((0, -0.12, 0.24)), 0.07, 0.01, 0.6)], (0.18, 0.20, 0.24))
        p.box(tuple(lamp + Vector((-0.07, 0.07, -0.08))), tuple(lamp + Vector((0.07, 0.09, -0.01))), LAMP)
        for x in (-0.1, 0.1):
            p.box(tuple(lamp + Vector((x - 0.025, 0.04, 0.03))), tuple(lamp + Vector((x + 0.025, 0.06, 0.06))), LAMP)
    # Backbone, tank, seat.
    seat_end = Vector((0.0, yr + k["tail"], k["seat_z"] - 0.06))
    p.cyl(head + Vector((0, 0, -0.04)), seat_end, 0.025, BLACK, 6)
    t = k["tank"]  # (y front, y back, bottom z, top z, half width)
    ty0, ty1, tz0, tz1, thw = t
    # Lofted: a teardrop in plan, domed on top, rounded in section (squarer for a sharp tank).
    tank = []
    n = 8
    for i in range(n + 1):
        f = i / n
        y = ty0 + (ty1 - ty0) * f
        swell = math.sin(math.pi * (0.08 + 0.84 * f)) ** 0.6
        top = tz1 + k.get("tank_bulge", 0.02) * math.sin(math.pi * f) - 0.04 * f * f
        bot = tz0 + 0.03 * (1 - f)
        tank.append(((0.0, y, (top + bot) / 2), thw * (0.35 + 0.65 * swell), (top - bot) / 2 * (0.6 + 0.4 * swell), k.get("tank_sq", 0.55)))
    loft(p, tank, k["paint"])
    if k.get("tank_stripe"):
        for x in (-1, 1):
            p.hull([(x * (thw * 0.98 + 0.004), y, z) for (y, z) in ((ty0 - 0.08, tz0 + 0.06), (ty0 - 0.1, tz1 - 0.05), (ty1 + 0.08, tz1 - 0.07), (ty1 + 0.08, tz0 + 0.05))]
                   + [(x * (thw * 0.98 + 0.012), (ty0 + ty1) / 2, (tz0 + tz1) / 2)], k["tank_stripe"])
    if k.get("shroud"):
        for x in (-1, 1):
            p.hull([(x * thw * 1.05, ty0 + 0.06, tz1 - 0.02), (x * thw * 1.05, ty0 + 0.02, tz0 - 0.06), (x * thw * 1.12, ty0 - 0.14, tz0 - 0.02),
                    (x * thw * 0.9, ty0 - 0.10, tz1), (x * thw * 1.2, ty0 - 0.02, (tz0 + tz1) / 2)], k["shroud"])
    sy0, sy1 = ty1 - 0.02, yr + k["tail"]
    sz, shw = k["seat_z"], k.get("seat_w", 0.13)
    if k.get("sprung"):
        # The rider's sprung saddle, narrow at the nose, and a separate pillion pad behind.
        cushion(p, sy0 + 0.02, sy0 - 0.30, sz - 0.01, 0.06, 0.15, 0.08, 0.10, SEAT)
        cushion(p, sy0 - 0.36, sy1, sz - 0.03, 0.12, 0.12, 0.09, 0.08, SEAT)
        for x in (-0.09, 0.09):
            p.cyl((x, sy0 - 0.27, sz - 0.08), (x, sy0 - 0.27, sz + 0.02), 0.018, CHROME, 6)
    else:
        step = k.get("seat_step", 0.0)
        if step:
            # Split seat: the rider's lower in front, the pillion's raised behind.
            mid = (sy0 + sy1) / 2
            cushion(p, sy0 + 0.01, mid, sz - 0.04, shw * 0.75, shw, 0.10, 0.10, SEAT)
            cushion(p, mid + 0.02, sy1, sz - 0.04 + step, shw * 0.95, shw * 0.8, 0.09, 0.08, SEAT)
        else:
            cushion(p, sy0 + 0.01, sy1, sz - 0.04, shw * 0.8, shw, 0.10, 0.09, SEAT)
    # Side panel under the seat, and the tail.
    loft(p, [((0, sy0 - 0.03, sz - 0.14), 0.12, 0.08, 0.45), ((0, sy0 - 0.15, sz - 0.15), 0.145, 0.10, 0.4),
             ((0, sy0 - 0.28, sz - 0.13), 0.13, 0.08, 0.45)], k.get("panel", k["paint"]))
    rg = k["rear_guard"]
    if k.get("tail_cowl"):
        # The tail: from under the seat, sweeping back and curving up to a rounded lamp, tapering.
        t0, t1 = sy0 - 0.20, sy1 - 0.16
        tail = []
        for i in range(9):
            f = i / 8
            tail.append(((0, t0 + (t1 - t0) * f, sz - 0.11 + 0.15 * f * f), 0.13 * (1 - 0.55 * f), 0.075 * (1 - 0.45 * f), 0.5))
        loft(p, tail, k["paint"])
        end = Vector((0, t1, sz + 0.04))
        loft(p, [(end + Vector((0, 0.04, -0.005)), 0.06, 0.032, 0.6), (end + Vector((0, -0.025, 0.0)), 0.05, 0.026, 0.75)], RED_LAMP)
        # Grab rail following the tail's curve, and the plate hung under it.
        for x in (-1, 1):
            prev = None
            for i in range(6):
                f = i / 5
                q = Vector((x * (0.15 - 0.05 * f), t0 + 0.12 + (t1 + 0.06 - t0 - 0.12) * f, sz - 0.06 + 0.12 * f * f))
                if prev:
                    p.cyl(prev, q, 0.011, k.get("rail", BLACK), 6)
                prev = q
        p.cyl((0, t1 + 0.06, sz - 0.07), (0, t1 - 0.02, sz - 0.20), 0.01, BLACK, 4)
        p.box((-0.10, t1 - 0.05, sz - 0.32), (0.10, t1 - 0.035, sz - 0.19), PLATE)
    else:
        # A classic tail: the mudguard runs on round the wheel, its lamp on the end, the plate below.
        a = math.radians(rg[2] - 30)
        r = R + 0.05
        lamp_at = Vector((0, yr + r * math.cos(a), R + r * math.sin(a)))
        out = Vector((0, math.cos(a), math.sin(a)))
        p.cyl(lamp_at - out * 0.01, lamp_at + out * 0.05, 0.048, CHROME, 14)
        p.cyl(lamp_at + out * 0.05, lamp_at + out * 0.07, 0.042, RED_LAMP, 14, r2=0.03)
        b = math.radians(rg[2] - 8)
        plate = Vector((0, yr + (R + 0.04) * math.cos(b), R + (R + 0.04) * math.sin(b)))
        p.box(tuple(plate + Vector((-0.10, -0.006, -0.12))), tuple(plate + Vector((0.10, 0.006, 0.0))), PLATE)
    # Rear suspension and swingarm.
    pivot = Vector((0, 0.0, 0.40))
    for x in (-0.11, 0.11):
        p.cyl(pivot + Vector((x, 0, 0)), Vector((x, yr, R)), 0.022, k.get("frame", BLACK), 6)
        p.cyl((x * 1.15, yr + 0.08, R + 0.06), (x * 1.15, yr + 0.22, sz - 0.08), 0.028, k.get("shock", CHROME), 8)
    # Engine: crankcase, the cylinder raked forward, its cooling fins.
    e = k["engine"]  # (y back, y front, bottom z, top z, half width)
    ey0, ey1, ez0, ez1, ehw = e
    zc, hh = (ez0 + ez1) / 2, (ez1 - ez0) / 2
    loft(p, [((0, ey0, zc + 0.02), ehw * 0.85, hh * 0.8, 0.45), ((0, ey0 + 0.04, zc + 0.01), ehw, hh, 0.4),
             ((0, ey1 - 0.04, zc - 0.01), ehw, hh, 0.4), ((0, ey1, zc - 0.02), ehw * 0.85, hh * 0.75, 0.45)], ENGINE)
    # Clutch and magneto covers bulging from each side, the gear lever, and the frame's down tube to
    # the engine's front: it hangs from the frame, not in mid-air.
    cover = Vector((0, (ey0 + ey1) / 2 - 0.02, zc - 0.01))
    for x in (-1, 1):
        p.cyl(cover + Vector((x * ehw, 0, 0)), cover + Vector((x * (ehw + 0.035), 0, 0)), min(hh * 0.95, 0.13), k.get("cover", ENGINE), 16, r2=min(hh * 0.8, 0.11))
    p.cyl(head + Vector((0, 0.0, -0.12)), Vector((0, ey1 - 0.02, ez1 - 0.03)), 0.022, k.get("frame", BLACK), 6)
    p.cyl(Vector((0, ey1 - 0.02, ez1 - 0.03)), Vector((0, ey1, ez0 + 0.04)), 0.022, k.get("frame", BLACK), 6)
    cy_base = Vector((0, ey1 - 0.08, ez1))
    cy_dir = Vector((0, math.sin(math.radians(k.get("cyl_tilt", 20))), math.cos(math.radians(k.get("cyl_tilt", 20)))))
    cyl_len = k.get("cyl_len", 0.22)
    p.cyl(cy_base, cy_base + cy_dir * cyl_len, k.get("cyl_r", 0.06), ENGINE, 10)
    for i in range(k.get("fins", 6)):
        c = cy_base + cy_dir * (cyl_len * (0.15 + 0.8 * i / max(k.get("fins", 6) - 1, 1)))
        p.cyl(c - cy_dir * 0.006, c + cy_dir * 0.006, k.get("fin_r", 0.10), k.get("fin_col", ENGINE), 12)
    # Exhaust.
    port = cy_base + cy_dir * (cyl_len * 0.6) + Vector((0.07, 0.05, 0))
    ex = k["exhaust"]
    low = Vector((0.13, ey1 + 0.02, 0.26))
    p.cyl(port, low, 0.024, CHROME, 8)
    if ex == "peashooter":
        mid = Vector((0.15, yr + 0.35, 0.30))
        p.cyl(low, mid, 0.026, CHROME, 8)
        p.cyl(mid, Vector((0.16, yr - 0.15, 0.36)), 0.045, CHROME, 12, r2=0.035)
    elif ex == "expansion":
        p.cyl(low, Vector((0.14, ey0, 0.24)), 0.07, BLACK, 12, r2=0.075)
        p.cyl(Vector((0.14, ey0, 0.24)), Vector((0.16, yr + 0.05, 0.32)), 0.075, CHROME, 12, r2=0.035)
        p.cyl(Vector((0.16, yr + 0.05, 0.32)), Vector((0.16, yr - 0.10, 0.34)), 0.02, CHROME, 6)
    elif ex == "upswept":
        p.cyl(low, Vector((0.14, ey0 - 0.05, 0.30)), 0.03, BLACK, 8)
        p.cyl(Vector((0.14, ey0 - 0.05, 0.30)), Vector((0.17, yr + 0.02, 0.52)), 0.06, ALLOY, 12, r2=0.045)
    else:  # commuter: a short silencer under the pillion footrest
        p.cyl(low, Vector((0.14, ey0 - 0.05, 0.27)), 0.026, BLACK, 8)
        p.cyl(Vector((0.14, ey0 - 0.05, 0.27)), Vector((0.16, yr + 0.05, 0.36)), 0.055, CHROME, 12, r2=0.04)
    if k.get("belly"):
        p.hull([(x, y, z) for x in (-0.15, 0.15) for (y, z) in ((ey1 + 0.05, ez0 + 0.12), (ey0, ez0 - 0.03), (ey1, ez0 - 0.04))] + [(0, (ey0 + ey1) / 2, ez0 - 0.07)], k["paint"])
    # Chain guard, footrests, kick and side stand.
    arc_band(p, yr, R, 0.03, 0.06, -30, 10, 0.012, BLACK, 4)
    p.hull([(-0.135 + dx, y, z) for dx in (0.0, 0.02) for (y, z) in ((0.0, 0.42), (0.0, 0.34), (yr + 0.05, R + 0.08), (yr + 0.05, R - 0.02))], k.get("chain_guard", BLACK))
    for x in (-0.22, 0.22):
        p.cyl((x * 0.5, 0.0, 0.30), (x, 0.0, 0.30), 0.012, BLACK, 4)
        p.cyl((x * 0.55, -0.42, 0.40), (x * 0.95, -0.42, 0.40), 0.01, BLACK, 4)
    p.cyl((-0.11, -0.05, 0.30), (-0.30, -0.10, 0.03), 0.012, BLACK, 4)
    # Mudguards.
    fg = k["front_guard"]  # (colour, from deg, to deg)
    arc_band(p, yf, R, R + 0.02, R + 0.03, fg[1], fg[2], k["tyre_f"] * 0.5 + 0.025, fg[0], 10)
    arc_band(p, yr, R, R + 0.02, R + 0.03, rg[1], rg[2], k["tyre_r"] * 0.5 + 0.03, rg[0], 10)
    for x in (-1, 1):
        p.box((x * 0.13 - 0.02, yf + 0.0, R + 0.12), (x * 0.13 + 0.02, yf + 0.04, R + 0.16), (0.98, 0.55, 0.06))
    # The front number plate, upright on the mudguard's nose (blank).
    a = math.radians(fg[1] + 25)
    plate = Vector((0, yf + (R + 0.04) * math.cos(a), R + (R + 0.04) * math.sin(a)))
    p.box(tuple(plate + Vector((-0.09, -0.005, 0.0))), tuple(plate + Vector((0.09, 0.005, 0.09))), PLATE)
    lean(p)
    return p.finish(0.025)


def thumper(Piece):
    return motorbike(Piece, "thumper", dict(
        R=0.34, tyre_f=0.09, tyre_r=0.10, wb=1.37, wire=True, rim=CHROME, hub=0.085, fork=0.72, rake=28,
        lamp="round", lamp_shell=BLACK, paint=(0.06, 0.06, 0.07), tank=(0.40, 0.05, 0.80, 0.98, 0.15), tank_bulge=0.04,
        tank_stripe=CHROME, sprung=True, seat_z=0.80, tail=0.12, panel=(0.06, 0.06, 0.07),
        engine=(-0.10, 0.28, 0.24, 0.52, 0.12), cyl_tilt=10, cyl_len=0.26, cyl_r=0.07, fins=8, fin_r=0.11,
        exhaust="peashooter", front_guard=((0.06, 0.06, 0.07), 10, 140), rear_guard=((0.06, 0.06, 0.07), 45, 215),
        chain_guard=(0.06, 0.06, 0.07), bar=0.38))


def commuter(Piece):
    return motorbike(Piece, "commuter", dict(
        R=0.31, tyre_f=0.07, tyre_r=0.08, wb=1.24, wire=False, rim=BLACK, fork=0.68, rake=26,
        lamp="visor", paint=(0.07, 0.07, 0.08), tank=(0.32, 0.02, 0.74, 0.90, 0.12), tank_stripe=(0.80, 0.10, 0.10),
        seat_z=0.79, tail=0.10, tail_cowl=True, rail=(0.70, 0.70, 0.72), panel=(0.80, 0.10, 0.10), engine=(-0.08, 0.22, 0.22, 0.44, 0.10), cyl_tilt=80,
        cyl_len=0.16, cyl_r=0.05, fins=5, fin_r=0.08, exhaust="commuter", front_guard=((0.07, 0.07, 0.08), 30, 120),
        rear_guard=((0.07, 0.07, 0.08), 70, 160), chain_guard=(0.07, 0.07, 0.08), bar=0.34, slider=(0.70, 0.70, 0.72)))


def discover(Piece):
    blue = (0.10, 0.24, 0.62)
    return motorbike(Piece, "discover", dict(
        R=0.30, tyre_f=0.08, tyre_r=0.09, wb=1.30, wire=False, rim=BLACK, fork=0.70, rake=26,
        lamp="visor", paint=blue, tank=(0.34, 0.02, 0.76, 0.93, 0.14), tank_sq=0.45, tank_stripe=(0.80, 0.82, 0.86),
        seat_z=0.80, tail=0.08, tail_cowl=True, panel=blue, engine=(-0.08, 0.24, 0.22, 0.45, 0.11), cyl_tilt=75,
        cyl_len=0.17, cyl_r=0.05, fins=5, fin_r=0.085, exhaust="commuter", front_guard=(BLACK, 30, 115),
        rear_guard=(BLACK, 80, 150), bar=0.35, chrome_bar=False))


def twostroke(Piece):
    red = (0.72, 0.06, 0.06)
    return motorbike(Piece, "twostroke", dict(
        R=0.31, tyre_f=0.07, tyre_r=0.08, wb=1.22, wire=True, rim=CHROME, hub=0.07, fork=0.66, rake=27,
        lamp="round", paint=red, tank=(0.30, 0.04, 0.76, 0.90, 0.11), tank_bulge=0.03, tank_stripe=(0.95, 0.95, 0.92),
        seat_z=0.77, tail=0.10, panel=BLACK, engine=(-0.06, 0.22, 0.24, 0.44, 0.10), cyl_tilt=25, cyl_len=0.15,
        cyl_r=0.055, fins=6, fin_r=0.085, fin_col=(0.62, 0.63, 0.65), exhaust="expansion",
        front_guard=(CHROME, 20, 130), rear_guard=(CHROME, 50, 210), bar=0.34))


def sport150(Piece):
    black, red = (0.06, 0.06, 0.07), (0.78, 0.08, 0.06)
    return motorbike(Piece, "sport150", dict(
        R=0.31, tyre_f=0.09, tyre_r=0.12, wb=1.32, wire=False, rim=BLACK, fork=0.70, rake=25,
        lamp="twin", paint=black, tank=(0.36, 0.0, 0.78, 0.99, 0.16), tank_bulge=0.03, shroud=red,
        tank_stripe=(0.75, 0.76, 0.80), seat_z=0.80, tail=0.04, seat_step=0.06, tail_cowl=True, panel=black,
        engine=(-0.10, 0.26, 0.22, 0.48, 0.12), cyl_tilt=30, cyl_len=0.18, cyl_r=0.06, fins=7, fin_r=0.095,
        exhaust="upswept", belly=True, front_guard=(black, 40, 110), rear_guard=(black, 100, 140), bar=0.33,
        chrome_bar=False, shock=(0.80, 0.12, 0.08), slider=(0.15, 0.15, 0.16)))


def roadster(Piece):
    """The black roadster bicycle."""
    p = Piece("roadster")
    R, frame = 0.355, BLACK
    yr, yf = -0.56, 0.60
    for y in (yr, yf):
        torus(p, y, R, R - 0.02, 0.02, TYRE, ring=5)
        torus(p, y, R, R - 0.045, 0.008, CHROME, ring=4)
        p.cyl((-0.05, y, R), (0.05, y, R), 0.025, CHROME, 8)
        for k in range(32):
            a = 2 * math.pi * k / 32
            side = 0.035 if k % 2 else -0.035
            p.cyl((side, y + 0.02 * math.cos(a + 0.4), R + 0.02 * math.sin(a + 0.4)), (0.0, y + (R - 0.05) * math.cos(a), R + (R - 0.05) * math.sin(a)), 0.0022, CHROME, 3)
    bb = Vector((0, 0.0, 0.30))
    seat_top = Vector((0, -0.17, 0.92))
    head_top, head_bot = Vector((0, 0.44, 0.96)), Vector((0, 0.49, 0.80))
    tubes = [(seat_top, head_top), (bb, head_bot), (bb, seat_top), (head_bot, head_top)]
    for a, b in tubes:
        p.cyl(a, b, 0.016, frame, 6)
    for x in (-0.06, 0.06):
        p.cyl(bb, Vector((x, yr, R)), 0.011, frame, 6)
        p.cyl(seat_top + Vector((0, 0, -0.05)), Vector((x, yr, R)), 0.010, frame, 6)
        p.cyl(head_bot, Vector((x, yf, R)), 0.012, frame, 6)
    # Handlebar swept back, grips, bell; saddle on its springs.
    stem = head_top + Vector((0, -0.03, 0.14))
    p.cyl(head_top, stem, 0.012, CHROME, 6)
    for x in (-1, 1):
        p.cyl(stem, stem + Vector((x * 0.24, -0.06, 0.02)), 0.010, CHROME, 6)
        p.cyl(stem + Vector((x * 0.24, -0.06, 0.02)), stem + Vector((x * 0.28, -0.22, 0.0)), 0.010, CHROME, 6)
        p.cyl(stem + Vector((x * 0.28, -0.14, 0.005)), stem + Vector((x * 0.28, -0.24, 0.0)), 0.017, BLACK, 8)
        # Rod brakes: a rod down from each lever.
        p.cyl(stem + Vector((x * 0.18, -0.08, 0.0)), head_top + Vector((x * 0.03, 0.0, -0.05)), 0.004, CHROME, 3)
    p.ball(stem + Vector((0.12, -0.02, 0.04)), 0.025, CHROME, 1)
    p.cyl(seat_top, seat_top + Vector((0, 0, 0.08)), 0.012, CHROME, 6)
    sad = seat_top + Vector((0, 0, 0.10))
    p.hull([sad + Vector((x * w, y, z)) for (y, w) in ((0.12, 0.03), (-0.10, 0.11)) for x in (-1, 1) for z in (0.0, 0.05)], (0.20, 0.12, 0.07))
    for x in (-0.07, 0.07):
        p.cyl(sad + Vector((x, -0.08, -0.07)), sad + Vector((x, -0.08, 0.0)), 0.012, CHROME, 6)
    # Chainwheel, chain guard, pedals and cranks.
    torus(p, 0.0, 0.30, 0.10, 0.008, CHROME, x=0.07, seg=20, ring=4)
    p.hull([(dx, y, z) for dx in (0.09, 0.10) for (y, z) in ((0.12, 0.36), (0.12, 0.22), (yr + 0.02, R + 0.06), (yr + 0.02, R - 0.04))], frame)
    for x, d in ((0.11, 1), (-0.11, -1)):
        foot = bb + Vector((x, 0.12 * d, -0.08 * d))
        p.cyl(bb + Vector((x * 0.8, 0, 0)), foot, 0.008, CHROME, 4)
        p.box((foot.x - 0.04 * (x < 0) - 0.0, foot.y - 0.04, foot.z - 0.01), (foot.x + 0.04 * (x > 0) + 0.0, foot.y + 0.04, foot.z + 0.01), BLACK, round=False)
    # Mudguards, carrier over the back wheel, the stand.
    arc_band(p, yf, R, R + 0.02, R + 0.025, 10, 150, 0.03, frame, 12)
    arc_band(p, yr, R, R + 0.02, R + 0.025, 30, 175, 0.03, frame, 12)
    p.box((-0.07, yr - 0.08, 0.78), (0.07, -0.22, 0.79), CHROME, round=False)
    for x in (-0.06, 0.06):
        p.cyl((x, yr - 0.06, 0.78), (x, yr, R), 0.006, CHROME, 4)
        p.cyl((x, -0.24, 0.78), seat_top + Vector((x * 0.3, 0, -0.08)), 0.006, CHROME, 4)
        p.cyl((x * 1.6, yr + 0.02, R), (x * 1.8, yr + 0.08, 0.0), 0.008, frame, 4)
    p.cyl((-0.04, yf + 0.04, R + 0.38), (0.04, yf + 0.04, R + 0.38), 0.04, BLACK, 10)
    return p.finish(0.0)


BIKES = (thumper, commuter, discover, twostroke, sport150, roadster)

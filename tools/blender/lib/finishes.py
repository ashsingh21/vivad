"""Procedural materials for the bows, arrows, astras and vessels. Like materials.py they exist only to
be baked into one texture set per model.

Geometry attributes (face-corner floats written by geo.loft, missing ones read as 0):
  ac   across-part coordinate -1..1 (painted borders sit near |ac| = 1)
  al   along-part distance in metres (vines, wraps and bands run along it)
  hw   half-width of the part in metres at that point
  kv   1 where a painted vine may appear (borders only otherwise)
  band 1 inside a painted cross band (Pinaka's ash stripes)
  u, v leaf / feather coordinates: u along 0..1 from the base, v across -1..1
"""

import math

from nodes import Graph, new_material


def _wear(g, co, radius=0.0012):
    """(edge, cavity) masks for rubbing on high edges and grime in hollows."""
    edge = g.edge_mask(radius, 2.4)
    cavity = g.sub(1.0, g.ao(0.008))
    return edge, cavity


def painted_vine(g, ac, al, hw, period=0.05, width=0.0016):
    """Gold-painted border lines plus a scrolling vine with leaves between them (0..1 mask)."""
    aca = g.absf(ac)
    border = g.band(aca, 0.84, 0.89, 0.015)
    y = g.mul(ac, hw)
    amp = g.mul(hw, 0.42)
    phase = g.mul(al, 2 * math.pi / period)
    wave_y = g.mul(g.math("SINE", phase), amp)
    vine = g.smooth(g.absf(g.sub(y, wave_y)), width, width * 0.5)
    xf = g.math("FRACT", g.math("DIVIDE", al, period))

    def leaf(xc, sign):
        dx = g.mul(g.sub(xf, xc), period)
        dy = g.sub(y, g.mul(g.sub(amp, g.mul(hw, 0.12)), sign))
        rx = g.add(g.mul(dx, 0.8), g.mul(dy, 0.6 * sign))
        ry = g.sub(g.mul(dy, 0.8), g.mul(dx, 0.6 * sign))
        d = g.vec("LENGTH", g.combine(g.mul(rx, 0.5), ry, 0.0))
        size = g.minf(0.0045, g.mul(hw, 0.24))
        return g.smooth(d, size, g.mul(size, 0.6))

    leaves = g.maxf(leaf(0.25, 1.0), leaf(0.75, -1.0))
    inner = g.smooth(aca, 0.72, 0.64)
    return g.maxf(border, g.mul(g.mul(g.maxf(vine, leaves), inner), g.attr("kv")))


def lacquer(name, base, paint=(0.86, 0.62, 0.26), under=(0.32, 0.20, 0.11), band_rgb=None, stripes=()):
    """Lacquered bow limb: deep glossy colour with fine crackle, gold-painted vines and borders,
    rubbed through to the wood on high edges. `band_rgb` paints cross stripes at the `bt` ranges in
    `stripes` (bt = 0 at the grip, 1 at the tips)."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    ac, al, hw = g.attr("ac"), g.attr("al"), g.attr("hw")
    edge, cavity = _wear(g, co)

    mottle = g.noise(co, 40.0, detail=5.0, rough=0.6)
    crackle = g.smooth(g.voronoi(g.mapping(co, scale=(1.0, 1.0, 0.5)), 260.0, feature="DISTANCE_TO_EDGE"), 0.025, 0.0)
    crackle = g.mul(crackle, g.smooth(g.noise(co, 9.0, detail=2.0), 0.45, 0.6))
    col = g.mixc(g.mul(mottle, 0.5), base, tuple(c * 0.72 for c in base))
    col = g.mixc(g.mul(crackle, 0.5), col, tuple(c * 0.45 for c in base))
    if band_rgb is not None:
        bt = g.attr("bt")
        st = None
        for a, b in stripes:
            s = g.band(bt, a, b, 0.003)
            st = s if st is None else g.maxf(st, s)
        col = g.mixc(g.mul(st, g.smooth(g.noise(co, 140.0, detail=3.0), 0.25, 0.4)), col, band_rgb)

    paint_m = painted_vine(g, ac, al, hw)
    rub = g.add(g.noise(co, 70.0, detail=4.0), g.mul(edge, 0.6))
    paint_m = g.mul(paint_m, g.smooth(rub, 0.9, 0.74))
    col = g.mixc(paint_m, col, paint)

    worn = g.mul(edge, g.smooth(g.noise(co, 110.0, detail=3.0), 0.4, 0.62))
    col = g.mixc(worn, col, under)
    col = g.mixc(g.mul(g.smooth(cavity, 0.2, 0.6), 0.6), col, (0.05, 0.035, 0.025))

    rough = g.add(0.22, g.mul(mottle, 0.1))
    rough = g.mix(crackle, rough, 0.45)
    rough = g.mix(paint_m, rough, 0.32)
    rough = g.mix(worn, rough, 0.6)
    metal = g.mul(paint_m, 0.85)
    height = g.add(g.mul(paint_m, 0.3), g.add(g.mul(crackle, -0.4), g.mul(mottle, 0.15)))
    g.principled(col, metal, rough, g.bump(height, 0.12, 0.0003))
    return m


def metal(name, rgb, rough=0.3, tarnish=(0.10, 0.08, 0.05), chase=False):
    """Gold, brass, silver or steel fittings: polished on edges, tarnished in hollows; `chase` adds a
    chased (punched) dot band pattern across the `al` direction."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    edge, cavity = _wear(g, co, 0.0008)
    mottle = g.noise(co, 80.0, detail=5.0, rough=0.6)
    hammer = g.voronoi(co, 420.0)
    col = g.mixc(g.mul(mottle, 0.35), rgb, tuple(c * 0.8 for c in rgb))
    grime = g.mul(g.smooth(cavity, 0.1, 0.55), g.smooth(g.noise(co, 60.0, detail=3.0), 0.35, 0.6))
    col = g.mixc(grime, col, tarnish)
    col = g.mixc(g.mul(edge, 0.5), col, tuple(min(1.0, c * 1.12) for c in rgb))
    r = g.add(rough, g.mul(mottle, 0.12))
    r = g.mix(edge, r, rough * 0.5)
    r = g.mix(grime, r, 0.7)
    height = g.add(g.mul(hammer, 0.25), g.mul(mottle, 0.1))
    if chase:
        dots = g.smooth(g.voronoi(g.mapping(co, scale=(1.0, 1.0, 1.0)), 900.0, randomness=0.3), 0.18, 0.05)
        rows = g.band(g.math("FRACT", g.math("DIVIDE", g.attr("al"), 0.006)), 0.35, 0.65, 0.08)
        height = g.add(height, g.mul(g.mul(dots, rows), -0.5))
    metal_v = g.mix(grime, 1.0, 0.3)
    g.principled(col, metal_v, r, g.bump(height, 0.1, 0.0002))
    return m


def wrap(name, a, b=None, pitch=0.0028):
    """Thread or sinew binding wound round the part: ridges along `al`, optional two-colour bands."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    al = g.attr("al")
    t = g.math("FRACT", g.math("DIVIDE", al, pitch))
    ridge = g.math("SINE", g.mul(t, math.pi))
    fibre = g.noise(g.mapping(co, scale=(3.0, 3.0, 0.3)), 900.0, detail=3.0)
    col = a if b is None else g.mixc(g.band(g.math("FRACT", g.math("DIVIDE", al, pitch * 10)), 0.0, 0.12, 0.01), a, b)
    col = g.mixc(g.mul(g.sub(1.0, ridge), 0.55), col, (0.04, 0.03, 0.02))
    col = g.mixc(g.mul(fibre, 0.25), col, (0.0, 0.0, 0.0))
    rough = g.add(0.62, g.mul(fibre, 0.2))
    height = g.add(ridge, g.mul(fibre, 0.2))
    g.principled(col, 0.0, rough, g.bump(height, 0.35, 0.0006))
    return m


def horn(name, dark=(0.05, 0.035, 0.03), light=(0.30, 0.20, 0.12), rough=0.28):
    """Polished horn or bone: long streaks along the part with translucent light patches."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    streak = g.noise(g.mapping(co, scale=(1.0, 1.0, 0.08)), 120.0, detail=6.0, rough=0.65)
    blot = g.smooth(g.noise(co, 18.0, detail=3.0), 0.5, 0.7)
    col = g.mixc(g.maxf(g.smooth(streak, 0.45, 0.7), blot), dark, light)
    edge, cavity = _wear(g, co, 0.0008)
    col = g.mixc(g.mul(cavity, 0.4), col, (0.02, 0.015, 0.01))
    r = g.add(rough, g.mul(streak, 0.1))
    g.principled(col, 0.0, r, g.bump(g.mul(streak, 0.3), 0.08, 0.0002))
    return m


def leather(name, rgb, stitch=(0.85, 0.62, 0.25)):
    """Dyed leather or velvet with gold-embroidered bands (where `band` is set) and grain."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    grain = g.voronoi(co, 650.0)
    mottle = g.noise(co, 25.0, detail=5.0)
    edge, cavity = _wear(g, co, 0.0015)
    col = g.mixc(g.mul(mottle, 0.4), rgb, tuple(c * 0.6 for c in rgb))
    emb_field = g.attr("band")
    al, ac = g.attr("al"), g.attr("ac")
    stitchwork = g.band(g.math("FRACT", g.add(g.math("DIVIDE", al, 0.004), g.mul(ac, 2.0))), 0.15, 0.55, 0.05)
    emb = g.mul(emb_field, stitchwork)
    col = g.mixc(emb, col, stitch)
    col = g.mixc(g.mul(edge, 0.35), col, tuple(min(1.0, c * 1.5) for c in rgb))
    col = g.mixc(g.mul(cavity, 0.5), col, (0.03, 0.02, 0.015))
    rough = g.mix(emb, g.add(0.6, g.mul(grain, 0.25)), 0.35)
    height = g.add(g.mul(grain, 0.3), g.mul(emb, 0.6))
    g.principled(col, g.mul(emb, 0.8), rough, g.bump(height, 0.2, 0.0004))
    return m


def reed(name):
    """Cane arrow shaft: straw-brown with darker growth nodes and long fibres."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    al = g.attr("al")
    node = g.band(g.math("FRACT", g.math("DIVIDE", al, 0.19)), 0.48, 0.52, 0.02)
    fibre = g.noise(g.mapping(co, scale=(4.0, 4.0, 0.05)), 500.0, detail=4.0)
    col = g.mixc(fibre, (0.55, 0.42, 0.24), (0.42, 0.30, 0.16))
    col = g.mixc(node, col, (0.20, 0.12, 0.06))
    g.principled(col, 0.0, g.add(0.45, g.mul(fibre, 0.15)), g.bump(g.add(fibre, node), 0.1, 0.0002))
    return m


def feather(name, rgb, bar=(0.08, 0.06, 0.05)):
    """Fletching: barred vane with a pale rachis along u at v = -1."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    u, v = g.attr("u"), g.attr("v")
    bars = g.band(g.math("FRACT", g.mul(g.add(u, g.mul(v, 0.15)), 4.0)), 0.0, 0.35, 0.06)
    barbs = g.math("SINE", g.mul(g.add(g.mul(u, 1.0), g.mul(v, 0.35)), 260.0))
    col = g.mixc(g.mul(bars, 0.85), rgb, bar)
    col = g.mixc(g.mul(g.add(barbs, 1.0), 0.08), col, (0.0, 0.0, 0.0))
    rachis = g.smooth(v, -0.75, -0.9)
    col = g.mixc(rachis, col, (0.82, 0.78, 0.68))
    g.principled(col, 0.0, 0.75, g.bump(barbs, 0.2, 0.0002))
    return m


def leaf(name, rgb, vein=(0.75, 0.80, 0.45)):
    """Leaf with a midrib along u (v = 0) and side veins."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    u, v = g.attr("u"), g.attr("v")
    mid = g.smooth(g.absf(v), 0.06, 0.0)
    side = g.mul(g.smooth(g.absf(g.math("SINE", g.mul(g.sub(u, g.mul(g.absf(v), 0.25)), 60.0))), 0.12, 0.0),
                 g.smooth(g.absf(v), 0.9, 0.6))
    veins = g.maxf(mid, g.mul(side, 0.6))
    mottle = g.noise(co, 60.0, detail=4.0)
    col = g.mixc(g.mul(mottle, 0.5), rgb, tuple(c * 0.65 for c in rgb))
    col = g.mixc(g.mul(veins, 0.6), col, vein)
    col = g.mixc(g.mul(g.smooth(u, 0.85, 1.0), 0.4), col, (0.35, 0.30, 0.10))
    g.principled(col, 0.0, g.add(0.5, g.mul(mottle, 0.15)), g.bump(g.mul(veins, -1.0), 0.25, 0.0003))
    return m


def scales(name, back=(0.08, 0.11, 0.14), belly=(0.55, 0.52, 0.40), sheen=(0.25, 0.35, 0.40)):
    """Snake skin: overlapping scales on the back, pale belly where `ac` < -0.6."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    ac, al = g.attr("ac"), g.attr("al")
    cell = g.voronoi(g.combine(g.math("DIVIDE", al, 0.006), g.math("DIVIDE", ac, 0.35), 0.0), 1.0, randomness=0.15)
    rim = g.smooth(cell, 0.32, 0.5)
    col = g.mixc(g.mul(g.noise(co, 50.0), 0.6), back, sheen)
    col = g.mixc(g.mul(rim, 0.6), col, (0.02, 0.025, 0.03))
    col = g.mixc(g.smooth(ac, -0.55, -0.75), col, belly)
    g.principled(col, 0.0, g.add(0.3, g.mul(rim, 0.25)), g.bump(g.sub(1.0, rim), 0.3, 0.0005))
    return m


def plain(name, rgb, rough=0.6, metal_v=0.0):
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    mottle = g.noise(co, 50.0, detail=4.0)
    col = g.mixc(g.mul(mottle, 0.4), rgb, tuple(c * 0.75 for c in rgb))
    g.principled(col, metal_v, g.add(rough, g.mul(mottle, 0.1)), g.bump(mottle, 0.05, 0.0003))
    return m

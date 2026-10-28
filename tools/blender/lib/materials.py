"""Procedural high-detail materials. These exist only to be baked into one texture set per weapon.

Geometry attributes they read (face-corner floats written by geo.loft):
  ez   blade edge-zone 0..1 (1 on the honed edge bevel)
  bt   normalised position along the blade 0..1
  ac   across-part coordinate, normalised -1..1 (koftgari borders sit near |ac| = 1)
  al   along-part distance in metres (koftgari scroll runs along it)
  hw   half-width of the part in metres at that point
  koft 1 where gold koftgari decoration is allowed
  kv   1 where the scrolling vine may appear (borders only otherwise)
  kb   1 where the buti (rosette) field may appear
"""

import math

from nodes import Graph, new_material


def wootz_steel(name="khanda_blade"):
    """Watered (wootz) crucible steel, brushed along the blade, honed bright at the edge."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    ez = g.attr("ez")
    bt = g.attr("bt")

    # Watered (wootz) pattern: fine, low-contrast carbide swirls stretched along the blade.
    stretched = g.mapping(co, scale=(1.0, 1.0, 0.40))
    warp = g.noise(stretched, 55.0, detail=6.0, rough=0.62, out="Color")
    wp = g.vec("ADD", stretched, g.vec("SCALE", warp, scale=0.0045))
    wootz = g.wave(wp, 240.0, 16.0, detail=6.0, direction="Y")
    wootz = g.smooth(wootz, 0.35, 0.68)
    wootz_fine = g.noise(stretched, 900.0, detail=3.0, rough=0.7)

    # Brushing: fine streaks running along the blade.
    brush = g.noise(g.mapping(co, scale=(1.0, 1.0, 0.012)), 1100.0, detail=3.0, rough=0.6)
    scratches = g.smooth(g.noise(g.mapping(co, scale=(1.0, 1.0, 0.05)), 480.0, detail=2.0), 0.68, 0.73)

    # Age: dark staining and fine pitting, heavier near the hilt.
    stain_n = g.noise(co, 30.0, detail=6.0, rough=0.62)
    stain = g.mul(g.smooth(stain_n, 0.62, 0.72), g.mix(bt, 0.7, 0.15))
    pits = g.smooth(g.voronoi(co, 900.0), 0.07, 0.0)
    pits = g.mul(pits, g.smooth(g.noise(co, 18.0, detail=3.0), 0.56, 0.62))

    light = (0.58, 0.58, 0.59)
    dark = (0.47, 0.47, 0.48)
    base = g.mixc(g.add(g.mul(wootz, 0.6), g.mul(wootz_fine, 0.12)), light, dark)
    hone = g.noise(g.mapping(co, scale=(1.0, 1.0, 0.02)), 2200.0, detail=2.0)
    base = g.mixc(g.mul(ez, 0.85), base, g.mixc(hone, (0.70, 0.70, 0.71), (0.62, 0.62, 0.63)))
    base = g.mixc(g.mul(stain, 0.8), base, (0.22, 0.20, 0.18))
    base = g.mixc(pits, base, (0.12, 0.11, 0.10))

    rough = g.add(0.12, g.add(g.mul(brush, 0.09), g.mul(wootz, 0.05)))
    rough = g.mix(ez, rough, g.add(0.06, g.mul(hone, 0.08)))
    rough = g.mix(scratches, rough, 0.30)
    rough = g.mix(stain, rough, 0.50)
    rough = g.mix(pits, rough, 0.75)

    metal = g.mix(g.maxf(g.mul(stain, 0.5), pits), 1.0, 0.5)

    height = g.add(g.mul(brush, 0.35), g.add(g.mul(wootz, 0.12), g.mul(pits, -1.0)))
    height = g.add(height, g.mul(scratches, -0.3))
    normal = g.bump(height, 0.18, 0.00025)
    g.principled(base, metal, rough, normal)
    return m


def koftgari_iron(name="khanda_hilt"):
    """Blackened iron with gold koftgari (overlaid gold) borders and scrolling vine, worn at the edges."""
    m = new_material(name)
    g = Graph(m)
    co = g.texcoord("Object")
    ac, al, hw = g.attr("ac"), g.attr("al"), g.attr("hw")
    koft, kv = g.attr("koft"), g.attr("kv")
    aca = g.absf(ac)

    # Borders: a broad band and a fine outer line, in normalised across-space.
    border = g.maxf(g.band(aca, 0.70, 0.80, 0.015), g.band(aca, 0.87, 0.91, 0.012))

    # Scrolling vine in metric space: y = across in metres, x = along.
    y = g.mul(ac, hw)
    period = 0.019
    phase = g.mul(al, 2 * math.pi / period)
    amp = g.mul(hw, 0.40)
    wave_y = g.mul(g.math("SINE", phase), amp)
    vine = g.smooth(g.absf(g.sub(y, wave_y)), 0.00062, 0.00034)
    # Leaf buds tucked inside each bend of the vine.
    xf = g.math("FRACT", g.math("DIVIDE", al, period))
    bud_y = g.sub(amp, 0.0032)
    # Leaves: ellipses elongated along the vine and tilted, inside each bend.
    def leaf(xc, sign):
        dx = g.mul(g.sub(xf, xc), period)
        dy = g.sub(y, g.mul(bud_y, sign))
        rx = g.add(g.mul(dx, 0.80), g.mul(dy, 0.60 * sign))
        ry = g.sub(g.mul(dy, 0.80), g.mul(dx, 0.60 * sign))
        d = g.vec("LENGTH", g.combine(g.mul(rx, 0.55), ry, 0.0))
        size = g.minf(0.0013, g.mul(hw, 0.14))   # leaves shrink where the part narrows
        return g.smooth(d, size, g.mul(size, 0.68))
    buds = g.maxf(leaf(0.25, 1.0), leaf(0.75, -1.0))
    # Keep the vine inside the border band.
    inner = g.smooth(aca, 0.66, 0.60)
    scroll = g.mul(g.mul(g.maxf(vine, buds), inner), kv)

    # Buti field: staggered four-petal rosettes (used where a vine would distort, e.g. the grip).
    cell = 0.0085
    cu = g.math("DIVIDE", al, cell)
    row = g.math("FLOOR", cu)
    cv = g.add(g.math("DIVIDE", y, cell), g.mul(g.math("FRACT", g.mul(row, 0.5)), 1.0))
    lx = g.mul(g.sub(g.math("FRACT", cu), 0.5), cell)
    ly = g.mul(g.sub(g.math("FRACT", cv), 0.5), cell)
    r = g.vec("LENGTH", g.combine(lx, ly, 0.0))
    theta = g.math("ARCTAN2", ly, lx)
    petal_r = g.mul(0.0021, g.add(0.45, g.mul(g.absf(g.math("COSINE", g.mul(theta, 2.0))), 0.55)))
    rosette = g.smooth(g.sub(r, petal_r), 0.0002, -0.0001)
    buti = g.mul(g.mul(rosette, g.smooth(aca, 0.64, 0.58)), g.attr("kb"))

    gold = g.mul(g.maxf(g.maxf(border, scroll), buti), koft)

    # Wear: gold rubbed away on high edges and in patches; iron polished bright on edges.
    edge = g.edge_mask(0.0012, 2.2)
    cavity = g.sub(1.0, g.ao(0.006))
    rub = g.add(g.noise(co, 90.0, detail=4.0), g.mul(edge, 0.55))
    gold = g.mul(gold, g.smooth(rub, 0.86, 0.72))
    wear = g.mul(edge, g.smooth(g.noise(co, 140.0, detail=3.0), 0.35, 0.55))

    mottle = g.noise(co, 55.0, detail=5.0, rough=0.6)
    iron = g.mixc(mottle, (0.055, 0.052, 0.050), (0.10, 0.095, 0.088))
    grime = g.mul(g.smooth(cavity, 0.15, 0.6), g.smooth(g.noise(co, 70.0, detail=4.0), 0.4, 0.6))

    base = g.mixc(wear, iron, (0.42, 0.41, 0.39))
    base = g.mixc(grime, base, (0.11, 0.065, 0.035))
    base = g.mixc(gold, base, (0.88, 0.64, 0.30))

    rough = g.add(0.42, g.mul(mottle, 0.16))
    rough = g.mix(wear, rough, 0.28)
    rough = g.mix(grime, rough, 0.85)
    rough = g.mix(gold, rough, 0.30)

    metal = g.mix(grime, 0.95, 0.15)
    metal = g.mix(gold, metal, 1.0)

    pits = g.smooth(g.voronoi(co, 700.0), 0.1, 0.0)
    height = g.add(g.mul(mottle, 0.4), g.add(g.mul(gold, 0.25), g.mul(pits, -0.6)))
    normal = g.bump(height, 0.15, 0.0002)
    g.principled(base, metal, rough, normal)
    return m

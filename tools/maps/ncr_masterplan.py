#!/usr/bin/env python3
"""Validate and draw the proposed NCR map. Does not build or launch the game."""
from __future__ import annotations

import argparse
import html
import json
import math
import random
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs/ncr/masterplan"
PLAN = OUT / "ncr-4km.plan.json"
COLORS = {
    "ground": "#e9e6db", "ink": "#263d40", "secondary": "#687772",
    "delhi": "#b97049", "gurugram": "#547b8a", "noida": "#789368",
    "ghaziabad": "#8d7a69", "river": "#99bbb8", "bank": "#c6d4c0",
    "green": "#9fae83", "pave": "#d4d1c8", "building": "#9fa6a0",
    "road": "#737d7b", "curb": "#f7f5ed", "grid": "#c4c9bf",
    "metro": "#337b9c", "yellow": "#ba8d40", "slice": "#a3533e",
}


def esc(value):
    return html.escape(str(value), quote=True)


def attrs(**kwargs):
    return " ".join(f'{k.replace("_", "-")}="{esc(v)}"' for k, v in kwargs.items() if v is not None)


def element(tag, content=None, **kwargs):
    a = attrs(**kwargs)
    return f"<{tag} {a}/>" if content is None else f"<{tag} {a}>{content}</{tag}>"


def pts(points):
    return " ".join(f"{x:.1f},{z:.1f}" for x, z in points)


def distance(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def segment_distance(p, a, b):
    dx, dz = b[0] - a[0], b[1] - a[1]
    denom = dx * dx + dz * dz
    t = max(0, min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / denom)) if denom else 0
    return distance(p, (a[0] + dx * t, a[1] + dz * t))


def river_x(plan, z):
    points = plan["river"]["centerline"]
    for (ax, az), (bx, bz) in zip(points, points[1:]):
        if az <= z <= bz:
            return ax + (bx - ax) * (z - az) / (bz - az)
    return points[0 if z < points[0][1] else -1][0]


def road_segments(plan):
    return [(a, b) for road in plan["roads"] for a, b in zip(road["points"], road["points"][1:])]


def compound_positions(plan):
    """Original, reproducible small-compound locations in the inter-POI space."""
    rng = random.Random(41)
    candidates = []
    for a, b in road_segments(plan):
        length = distance(a, b)
        for i in range(1, max(2, int(length / 80))):
            t = i / max(2, int(length / 80))
            side = rng.choice([-1, 1])
            offset = rng.uniform(45, 85)
            p = [a[0] + (b[0] - a[0]) * t - side * (b[1] - a[1]) / length * offset,
                 a[1] + (b[1] - a[1]) * t + side * (b[0] - a[0]) / length * offset]
            if not (-1650 < p[0] < 1750 and -1800 < p[1] < 1800):
                continue
            if abs(p[0] - river_x(plan, p[1])) < 190:
                continue
            if any(distance(p, q["center"]) < q["radius_m"] + 65 for q in plan["pois"]):
                continue
            candidates.append([round(p[0]), round(p[1])])
    rng.shuffle(candidates)
    chosen = []
    for p in candidates:
        if all(distance(p, q) > 180 for q in chosen):
            chosen.append(p)
        if len(chosen) == 24:
            return sorted(chosen, key=lambda p: (p[1], p[0]))
    raise ValueError(f"Only {len(chosen)} suitable micro-compounds; need 24")


def validate(plan):
    assert plan["status"] == "design_blueprint_not_runtime_map"
    assert plan["confirmed_size_m"] == [4000, 4000]
    assert plan["coordinate_system"]["bounds"] == [-2000, -2000, 2000, 2000]
    pois = plan["pois"]
    assert len(pois) == 16 and len({p["id"] for p in pois}) == 16
    assert Counter(p["district"] for p in pois) == {d["id"]: 4 for d in plan["districts"]}
    for p in pois:
        assert all(abs(v) + p["radius_m"] <= 2000 for v in p["center"]), p["id"]
        assert min(segment_distance(p["center"], a, b) for a, b in road_segments(plan)) <= p["radius_m"], p["id"]
        assert p["hero_buildings"] >= 2 and p["planned_chests"] > 0
        assert p["routes"] and p["fight"]
    assert len(plan["crossings"]) == 5
    assert len({c["id"] for c in plan["crossings"]}) == 5
    for c in plan["crossings"]:
        assert abs(c["z_m"]) < 2000 and abs(river_x(plan, c["z_m"])) < 2000
        assert c["width_m"] >= 8
        if c["mode"] == "underground":
            assert c["deck_y_m"] < plan["river"]["bed_y_m"] - 6
        else:
            assert any(any(a[1] == c["z_m"] == b[1] and min(a[0], b[0]) <= river_x(plan, c["z_m"]) <= max(a[0], b[0])
                           for a, b in zip(r["points"], r["points"][1:])) for r in plan["roads"]), c["id"]
    # Shared road junctions connect every road to one network; no isolated destination spine.
    connected = {0}
    while True:
        old = set(connected)
        for i, r in enumerate(plan["roads"]):
            if any(set(map(tuple, r["points"])) & set(map(tuple, plan["roads"][j]["points"])) for j in connected):
                connected.add(i)
        if connected == old:
            break
    assert len(connected) == len(plan["roads"]), "Disconnected road centreline network"
    for r in plan["roads"]:
        assert all(abs(v) <= 1900 for p in r["points"] for v in p)
        assert all(distance(a, b) > 0 for a, b in zip(r["points"], r["points"][1:]))
    s = plan["streaming"]
    assert s["cell_m"] * s["cells_per_axis"] == s["grid_span_m"] == 4096
    v = plan["vertical_slice"]
    assert all(abs(x) + w / 2 < 2000 for x, w in zip(v["center"], v["size_m"]))
    for t in plan["performance_targets"]:
        assert abs(1000 / t["fps_target"] - t["frame_budget_ms"]) < 0.01
        assert t["detail_radius_m"] < t["mid_lod_radius_m"] < t["hlod_radius_m"]
    compounds = compound_positions(plan)
    return {"status": "passed", "runtime_integrated": False, "area_km2": 16,
            "major_pois": len(pois), "micro_compounds": len(compounds),
            "crossings": len(plan["crossings"]), "roads_connected": True,
            "planned_hero_buildings": sum(p["hero_buildings"] for p in pois),
            "planned_chests_major_pois": sum(p["planned_chests"] for p in pois),
            "note": "Offline blueprint checks only; no physics, rendering or FPS validation."}


def geometry(plan, theme=False):
    def c(key):
        if not theme:
            return COLORS[key]
        return {
            "ground": "var(--background)", "ink": "var(--foreground)",
            "secondary": "var(--muted-foreground)", "delhi": "var(--viz-series-1)",
            "gurugram": "var(--viz-series-2)", "noida": "var(--viz-series-3)",
            "ghaziabad": "var(--viz-series-4)", "river": "color-mix(in srgb, var(--blue) 35%, var(--background))",
            "bank": "color-mix(in srgb, var(--green) 15%, var(--background))",
            "green": "color-mix(in srgb, var(--green) 25%, var(--background))",
            "pave": "color-mix(in srgb, var(--foreground) 7%, var(--background))",
            "building": "color-mix(in srgb, var(--foreground) 33%, var(--background))",
            "road": "color-mix(in srgb, var(--foreground) 40%, var(--background))",
            "curb": "var(--background)", "grid": "var(--border)",
            "metro": "var(--blue)", "yellow": "var(--orange)", "slice": "var(--foreground)"
        }[key]
    out = [element("rect", x=-2000, y=-2000, width=4000, height=4000, fill=c("ground"))]
    out.append(element("polygon", points=pts(plan["ridge"]["polygon"]), fill=c("green")))
    for d in plan["districts"]:
        out.append(element("polygon", points=pts(d["polygon"]), fill=c(d["id"]), opacity="0.08"))
    river = plan["river"]["centerline"]
    for width, color in [(215, c("bank")), (140, c("river"))]:
        out.append(element("polyline", points=pts(river), fill="none", stroke=color, stroke_width=width, stroke_linejoin="round"))
    # Fine cartographic grain: gardens, courtyards and footprint clusters at their intended metre scale.
    for p in plan["pois"]:
        x, z = p["center"]
        r = p["radius_m"]
        color = c(p["district"])
        out.append(element("circle", cx=x, cy=z, r=r, fill=color, opacity="0.07"))
        rng = random.Random(p["id"])
        if p["form"] in ["park", "wetland"]:
            for _ in range(38):
                a, rr = rng.random() * math.tau, math.sqrt(rng.random()) * r
                out.append(element("ellipse", cx=round(x + math.cos(a) * rr, 1), cy=round(z + math.sin(a) * rr, 1), rx=rng.randint(9, 25), ry=rng.randint(7, 20), fill=c("green"), opacity="0.8"))
        else:
            step = 34 if p["form"] == "lanes" else 55
            for dx in range(-r + 20, r - 19, step):
                for dz in range(-r + 20, r - 19, step):
                    if math.hypot(dx, dz) > r - 25:
                        continue
                    if abs(dx) < 12 or abs(dz) < 12:
                        continue
                    if p["form"] == "ring" and any(abs(math.hypot(dx, dz) - rr) < 15 for rr in [55, 110, 190]):
                        continue
                    w = rng.randint(9, 20) if step == 34 else rng.randint(18, 34)
                    h = rng.randint(12, 23) if step == 34 else rng.randint(20, 36)
                    if p["form"] in ["yards", "rail", "mall"]:
                        w, h = rng.randint(25, 46), rng.randint(20, 32)
                    out.append(element("rect", x=x + dx - w / 2, y=z + dz - h / 2, width=w, height=h, fill=c("building"), opacity="0.8"))
            if p["form"] == "ring":
                for radius in [55, 110, 190]:
                    out.append(element("circle", cx=x, cy=z, r=radius, fill="none", stroke=c("road"), stroke_width=8))
            elif p["form"] == "rail":
                for dz in range(-120, 121, 20):
                    out.append(element("path", d=f"M{x-225},{z+dz} h450", stroke=c("road"), stroke_width=2, fill="none"))
        # Local access crosses connect each compound to its perimeter and larger road network.
        for a, b in [((x-r, z), (x+r, z)), ((x, z-r), (x, z+r))]:
            out.append(element("polyline", points=pts([a, b]), stroke=c("pave"), stroke_width=11, fill="none"))
            out.append(element("polyline", points=pts([a, b]), stroke=c("road"), stroke_width=6, fill="none"))
    for x, z in compound_positions(plan):
        out.append(element("rect", x=x-28, y=z-22, width=56, height=44, fill=c("pave")))
        for dx, dz in [(-18,-13),(5,-13),(-18,5)]:
            out.append(element("rect", x=x+dx, y=z+dz, width=17, height=12, fill=c("building")))
    for road in plan["roads"]:
        for width, color in [(road["width_m"] + 5, c("curb")), (road["width_m"], c("road"))]:
            out.append(element("polyline", points=pts(road["points"]), fill="none", stroke=color, stroke_width=width, stroke_linejoin="round"))
    for crossing in plan["crossings"]:
        if crossing["mode"] == "underground":
            continue
        x, z = river_x(plan, crossing["z_m"]), crossing["z_m"]
        out.append(element("rect", x=x-110, y=z-crossing["width_m"]/2-5, width=220, height=crossing["width_m"]+10, fill=c("curb")))
        out.append(element("rect", x=x-110, y=z-crossing["width_m"]/2, width=220, height=crossing["width_m"], fill=c("road")))
    for line in plan["metro_lines"]:
        out.append(element("polyline", points=pts(line["points"]), fill="none", stroke=c("metro" if line["id"] == "blue" else "yellow"), stroke_width=5, stroke_dasharray="16 10", opacity="0.85"))
    for n in range(-1500, 2000, 500):
        out.append(element("path", d=f"M{n},-2000 V2000 M-2000,{n} H2000", fill="none", stroke=c("grid"), stroke_width=1, opacity="0.7"))
    v = plan["vertical_slice"]
    out.append(element("rect", x=v["center"][0]-300, y=v["center"][1]-300, width=600, height=600, fill="none", stroke=c("slice"), stroke_width=5, stroke_dasharray="18 12"))
    return "\n".join(out)


def static_atlas(plan):
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1310" viewBox="0 0 1600 1310" role="img" aria-labelledby="atlas-title atlas-desc">',
           '<title id="atlas-title">NCR four-city masterplan, 4 by 4 kilometres</title>',
           '<desc id="atlas-desc">Original fictional level design: Delhi northwest, Gurugram southwest, Ghaziabad northeast, Noida southeast. Sixteen major destinations, five river crossings and a 600 metre Delhi production slice.</desc>',
           '<rect width="1600" height="1310" fill="#f7f5ed"/>',
           '<g font-family="Arial, sans-serif" fill="#263d40">',
           '<text x="64" y="54" font-size="17" letter-spacing="3">VIVAD / KALI YUGA</text>',
           '<text x="64" y="102" font-size="38" font-weight="600">NCR: four cities, four kinds of fight.</text>',
           '<text x="64" y="132" font-size="17" fill="#687772">4 × 4 km · 16 km² · Original fictional level design · Blueprint v1</text>']
    left, top, side = 64, 184, 1016
    scale = side / 4000
    out.append(f'<defs><clipPath id="atlas-map-clip"><rect x="{left}" y="{top}" width="{side}" height="{side}"/></clipPath></defs>')
    out.append('<g clip-path="url(#atlas-map-clip)">')
    out.append(f'<g transform="translate({left+side/2},{top+side/2}) scale({scale})">')
    out.append(geometry(plan))
    out.append('</g></g>')
    def screen(p):
        return left + (p[0]+2000)*scale, top+(p[1]+2000)*scale
    for i in range(8):
        out.append(f'<text x="{left+(i+.5)*side/8}" y="{top-14}" text-anchor="middle" font-size="14">{chr(65+i)}</text>')
        out.append(f'<text x="{left-18}" y="{top+(i+.5)*side/8+5}" text-anchor="middle" font-size="14">{i+1}</text>')
    for p in plan["pois"]:
        x, y = screen(p["center"])
        out.append(element("circle", cx=x, cy=y, r=11, fill=COLORS[p["district"]], stroke="#f7f5ed", stroke_width=2))
        out.append(f'<text x="{x}" y="{y+4}" text-anchor="middle" font-size="10" font-weight="600" fill="#ffffff">{p["id"]}</text>')
        out.append(f'<text x="{x}" y="{y+29}" text-anchor="middle" font-size="13" font-weight="600" paint-order="stroke" stroke="#f7f5ed" stroke-width="3">{esc(p["name"])}</text>')
    for c in plan["crossings"]:
        x, y = screen([river_x(plan,c["z_m"]),c["z_m"]])
        out.append(f'<text x="{x+13}" y="{y-9}" font-size="12" font-weight="600" paint-order="stroke" stroke="#f7f5ed" stroke-width="3">{c["id"]}</text>')
    y = 204
    for d in plan["districts"]:
        out.append(f'<rect x="1120" y="{y-17}" width="5" height="24" fill="{COLORS[d["id"]]}"/>')
        out.append(f'<text x="1138" y="{y+2}" font-size="23" font-weight="600">{esc(d["name"])}</text>')
        y += 36
        for p in (p for p in plan["pois"] if p["district"] == d["id"]):
            out.append(f'<text x="1138" y="{y}" font-size="15">{p["id"]}  {esc(p["name"])}</text>')
            y += 26
        y += 28
    out.extend([
        '<text x="1120" y="944" font-size="18" font-weight="600">Five ways across the river</text>',
        '<text x="1120" y="976" font-size="15">C1  North Rail Walk</text>',
        '<text x="1120" y="1002" font-size="15">C2  North Boulevard</text>',
        '<text x="1120" y="1028" font-size="15">C3  Central Interchange</text>',
        '<text x="1120" y="1054" font-size="15">C4  Southern Causeway</text>',
        '<text x="1120" y="1080" font-size="15">C5  Blue Underpass</text>',
        '<path d="M1120 1120h42" stroke="#337b9c" stroke-width="3" stroke-dasharray="7 4"/>',
        '<text x="1173" y="1125" font-size="15">Metro pedestrian corridors</text>',
        '<path d="M1120 1155h42" stroke="#a3533e" stroke-width="2" stroke-dasharray="7 4"/>',
        '<text x="1173" y="1160" font-size="15">600 m first production slice</text>',
        '<text x="64" y="1243" font-size="17">N ↑</text>',
        '<path d="M148 1233h254m-254-6v12m127-12v12m127-12v12" stroke="#263d40" stroke-width="2"/>',
        '<text x="148" y="1260" font-size="14">0</text><text x="259" y="1260" font-size="14">500 m</text><text x="385" y="1260" font-size="14">1 km</text>',
        '<text x="570" y="1242" font-size="15">Design complete for review · Runtime construction and FPS validation pending</text>',
        '<text x="570" y="1268" font-size="14" fill="#687772">Same gameplay geometry on Mac and Android; detail changes with distance.</text>',
        '</g></svg>'
    ])
    return "\n".join(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inline-path", type=Path)
    parser.add_argument("--check", action="store_true", help="Validate blueprint without writing outputs")
    args = parser.parse_args()
    plan = json.loads(PLAN.read_text())
    report = validate(plan)
    if not args.check:
        (OUT / "ncr-4km-atlas.svg").write_text(static_atlas(plan))
        (OUT / "validation.json").write_text(json.dumps(report, indent=2) + "\n")
        (OUT / "micro-compounds.json").write_text(json.dumps(compound_positions(plan), indent=2) + "\n")
        if args.inline_path:
            fragment = (Path(__file__).with_name("ncr_masterplan.fragment.html")).read_text()
            fragment = fragment.replace("__MAP_DATA__", json.dumps(plan, ensure_ascii=False).replace("<", "\\u003c"))
            fragment = fragment.replace("__MAP_GEOMETRY__", geometry(plan, theme=True))
            args.inline_path.parent.mkdir(parents=True, exist_ok=True)
            args.inline_path.write_text(fragment)
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

"""Panga's Delhi maps from OpenStreetMap.

python3 tools/osm/delhi.py fetch [area ...]   download areas from the Overpass API to build/osm/<area>_raw.json
python3 tools/osm/delhi.py [area ...]         convert them to assets/data/maps/delhi_<area>.json (all by default)

Areas (AREAS below): cp (Connaught Place and the head of Janpath), chandni (Chandni Chowk), hauzkhas
(Hauz Khas village and lake), indiagate (India Gate and Kartavya Path).

Each map is an 800 m square round its centre (CP's: the middle of Central Park, shifted south so it
holds the inner circle and the head of Janpath). Game axes: x east, z south (north is -z), metres.
Buildings are footprints (outer ring, simplified, counter-clockwise seen from above) with a height
from building:levels or height, else a guess by type; roads are centrelines with a width by class
(footways and paths too, for lanes and park paths); areas are parks, water, pitches and car parks;
trees are OSM's mapped trees; landmarks the game builds itself (India Gate) are placed where mapped.
Where OpenStreetMap has few of an area's buildings (the old city's lanes, a village), the gaps are
filled, deterministically, with houses facing the nearest street (kind "infill").

Map data (c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright).
"""

import json
import math
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
# Kept out to here (the playable square is +-400; buildings beyond it make the skyline).
KEEP = 450.0
HALF = 400.0

# Where each map is (its centre, or a park whose middle anchors it, shifted south), and where to
# fill gaps in the mapped buildings: "all" the land, only built-up "landuse", or nowhere.
AREAS = {
    "cp": dict(title="Connaught Place", anchor="Rajiv Chowk Central Park", south_shift=40.0, fill=None),
    "chandni": dict(title="Chandni Chowk", center=(28.6557, 77.2318), fill="all"),
    "hauzkhas": dict(title="Hauz Khas", center=(28.5540, 77.1937), fill="landuse"),
    "indiagate": dict(title="India Gate", center=(28.6129, 77.2283), fill=None),
}
raw_of = lambda area: os.path.join(ROOT, "build", "osm", f"{area}_raw.json")
out_of = lambda area: os.path.join(ROOT, "assets", "data", "maps", f"delhi_{area}.json")

ROAD_WIDTH = {
    "primary": 16.0, "secondary": 14.0, "tertiary": 10.0, "secondary_link": 8.0, "tertiary_link": 7.0,
    "residential": 7.0, "unclassified": 7.0, "service": 5.0, "pedestrian": 6.0, "living_street": 6.0,
    "footway": 2.5, "path": 2.5, "steps": 2.5, "track": 3.0, "cycleway": 2.5,
}
PATHS = ("footway", "path", "steps", "track", "cycleway")
AREA_KIND = {
    ("leisure", "park"): "park", ("leisure", "garden"): "park", ("landuse", "grass"): "park",
    ("leisure", "pitch"): "pitch", ("amenity", "parking"): "parking", ("leisure", "playground"): "park",
    ("natural", "water"): "water", ("water", "pond"): "water", ("water", "lake"): "water", ("landuse", "basin"): "water",
    ("water", "canal"): "water", ("water", "reflecting_pool"): "water", ("natural", "wood"): "park", ("landuse", "forest"): "park",
    ("leisure", "nature_reserve"): "park", ("landuse", "recreation_ground"): "park", ("landuse", "village_green"): "park",
}
BUILT_UP = ("residential", "commercial", "retail")
# Landmarks the game builds itself, found by name.
LANDMARKS = {"India Gate": "india_gate"}


def fetch(area):
    cfg = AREAS[area]
    lat, lon = cfg.get("center", (28.6328, 77.2197))
    dlat = (KEEP + 30.0) / 111320.0
    dlon = (KEEP + 30.0) / (111320.0 * math.cos(math.radians(lat)))
    bb = f"{lat - dlat:.6f},{lon - dlon:.6f},{lat + dlat:.6f},{lon + dlon:.6f}"
    q = (f'[out:json][timeout:90];(way["building"]({bb});way["highway"]({bb});way["leisure"]({bb});way["landuse"]({bb});'
         f'way["natural"]({bb});node["natural"="tree"]({bb});node["name"]({bb});way["amenity"]({bb});way["historic"]({bb});'
         f'way["water"]({bb});relation["natural"="water"]({bb});relation["leisure"]({bb});relation["landuse"]({bb}););'
         f'out body;>;out skel qt;')
    os.makedirs(os.path.dirname(raw_of(area)), exist_ok=True)
    subprocess.run(["curl", "-s", "-m", "150", "-A", "vivad-map-builder/0.1", "-H", "Accept: application/json", "-o", raw_of(area),
                    "--data-urlencode", f"data={q}", "https://overpass-api.de/api/interpreter"], check=True)


def simplify(pts, tol):
    """Douglas-Peucker on an open polyline."""
    if len(pts) < 3:
        return pts
    (ax, az), (bx, bz) = pts[0], pts[-1]
    dx, dz = bx - ax, bz - az
    L = math.hypot(dx, dz) or 1e-9
    far, idx = 0.0, 0
    for i, (px, pz) in enumerate(pts[1:-1], 1):
        d = abs(dz * (px - ax) - dx * (pz - az)) / L
        if d > far:
            far, idx = d, i
    if far <= tol:
        return [pts[0], pts[-1]]
    return simplify(pts[:idx + 1], tol)[:-1] + simplify(pts[idx:], tol)


def area(ring):
    return 0.5 * sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(ring, ring[1:] + ring[:1]))


def ring_of(way, xy):
    pts = [xy[n] for n in way["nodes"] if n in xy]
    if len(pts) > 2 and pts[0] == pts[-1]:
        pts = pts[:-1]
    if len(pts) < 3:
        return None
    # Simplify as a loop: split at the point farthest from the first.
    far = max(range(len(pts)), key=lambda i: math.dist(pts[0], pts[i]))
    pts = simplify(pts[:far + 1], 0.35)[:-1] + simplify(pts[far:] + [pts[0]], 0.35)[:-1]
    if len(pts) < 3 or abs(area(pts)) < 4.0:
        return None
    # Counter-clockwise seen from above (+Y), which with x east and z south is clockwise in (x, z).
    if area(pts) > 0:
        pts.reverse()
    return [[round(x, 2), round(z, 2)] for x, z in pts]


def height_of(tags):
    for k in ("height", "building:height"):
        try:
            return float(tags[k].split()[0])
        except (KeyError, ValueError):
            pass
    try:
        return float(tags["building:levels"]) * 3.4 + 0.8
    except (KeyError, ValueError):
        pass
    kind = tags.get("building", "yes")
    return {"commercial": 11.0, "retail": 10.5, "office": 14.0, "hotel": 16.0, "house": 7.0, "residential": 9.0,
            "mosque": 9.0, "temple": 8.0, "school": 10.0, "public": 10.0, "roof": 4.2}.get(kind, 9.0)


def underground(tags):
    return tags.get("layer", "0").startswith("-") or tags.get("location") == "underground" or tags.get("building:levels") == "0" \
        or tags.get("building") == "train_station"


def point_in(pt, ring):
    x, z = pt
    inside = False
    for (ax, az), (bx, bz) in zip(ring, ring[1:] + ring[:1]):
        if (az > z) != (bz > z) and x < ax + (z - az) * (bx - ax) / (bz - az):
            inside = not inside
    return inside


def seg_dist(p, a, b):
    ax, az = a
    dx, dz = b[0] - ax, b[1] - az
    L2 = dx * dx + dz * dz or 1e-9
    t = max(0.0, min(1.0, ((p[0] - ax) * dx + (p[1] - az) * dz) / L2))
    return math.hypot(p[0] - ax - t * dx, p[1] - az - t * dz)


def outer_rings(rel, ways):
    """A multipolygon relation's outer rings, joined end to end from its member ways."""
    parts = [list(ways[m["ref"]]) for m in rel.get("members", []) if m["type"] == "way" and m.get("role", "outer") in ("outer", "") and m["ref"] in ways]
    rings = []
    while parts:
        ring = parts.pop(0)
        grown = True
        while grown and ring[0] != ring[-1]:
            grown = False
            for k, p in enumerate(parts):
                if p[0] == ring[-1]:
                    ring += p[1:]
                elif p[-1] == ring[-1]:
                    ring += p[::-1][1:]
                elif p[-1] == ring[0]:
                    ring = p + ring[1:]
                elif p[0] == ring[0]:
                    ring = p[::-1] + ring[1:]
                else:
                    continue
                parts.pop(k)
                grown = True
                break
        if ring[0] == ring[-1] and len(ring) > 3:
            rings.append(ring)
    return rings


def centroid(pts):
    return (sum(p[0] for p in pts) / len(pts), sum(p[1] for p in pts) / len(pts))


def fill(buildings, roads, areas, built_up, everywhere):
    """Houses in the gaps: a grid of 12 m lots over the built-up land; a lot clear of roads (by
    their width and a pavement), paths, parks, water and mapped buildings gets a house of 2 to 5
    storeys, 8.5 m square, turned to face the nearest street. Seeded by the lot, so it's the same
    every run."""
    lot, size = 12.0, 8.5
    segs = [(r, a, b) for r in roads for a, b in zip(r["pts"], r["pts"][1:])]
    blocked = [a["pts"] for a in areas]
    boxes = [(b["pts"], min(p[0] for p in b["pts"]) - 3, max(p[0] for p in b["pts"]) + 3,
              min(p[1] for p in b["pts"]) - 3, max(p[1] for p in b["pts"]) + 3) for b in buildings]
    out = []
    n = int((HALF + 30) // lot)
    for i in range(-n, n + 1):
        for j in range(-n, n + 1):
            c = (i * lot, j * lot)
            if not everywhere and not any(point_in(c, u) for u in built_up):
                continue
            if any(point_in(c, p) for p in blocked):
                continue
            near_b = False
            for pts, x0, x1, z0, z1 in boxes:
                if x0 < c[0] < x1 and z0 < c[1] < z1 and (point_in(c, pts) or any(math.dist(c, q) < size for q in pts)):
                    near_b = True
                    break
            if near_b:
                continue
            best, facing = 1e9, 0.0
            clear = True
            for r, a, b in segs:
                d = seg_dist(c, a, b)
                need = r["w"] / 2 + (0.6 if r["kind"] in PATHS else 2.5) + size * 0.5
                if d < need:
                    clear = False
                    break
                if r["kind"] not in PATHS and d < best:
                    best, facing = d, math.atan2(b[1] - a[1], b[0] - a[0])
            if not clear:
                continue
            # A deterministic hash of the lot: storeys, and a little give in size.
            h = (i * 73856093 ^ j * 19349663) & 0xFFFFFFFF
            levels = 2 + h % 4
            hx, hz = size / 2 * (0.9 + (h >> 4) % 3 * 0.05), size / 2 * (0.9 + (h >> 6) % 3 * 0.05)
            ca, sa = math.cos(facing), math.sin(facing)
            corners = [(c[0] + ca * x - sa * z, c[1] + sa * x + ca * z) for x, z in ((hx, -hz), (hx, hz), (-hx, hz), (-hx, -hz))]
            if area(corners) > 0:
                corners.reverse()
            out.append({"pts": [[round(x, 2), round(z, 2)] for x, z in corners], "h": round(levels * 3.3 + 0.8, 1), "kind": "infill"})
    return out


def convert(name):
    cfg = AREAS[name]
    d = json.load(open(raw_of(name)))
    els = d["elements"]
    lonlat = {e["id"]: (e["lon"], e["lat"]) for e in els if e["type"] == "node"}
    shift = cfg.get("south_shift", 0.0)
    if "anchor" in cfg:
        park = next(e for e in els if e["type"] == "way" and e.get("tags", {}).get("name") == cfg["anchor"])
        ring = [lonlat[n] for n in park["nodes"] if n in lonlat]
        lon0, lat0 = sum(p[0] for p in ring) / len(ring), sum(p[1] for p in ring) / len(ring)
    else:
        lat0, lon0 = cfg["center"]
    my = 111320.0
    mx = my * math.cos(math.radians(lat0))
    xy = {k: ((lon - lon0) * mx, -(lat - lat0) * my - shift) for k, (lon, lat) in lonlat.items()}
    inside = lambda pts: all(abs(x) < KEEP and abs(z) < KEEP for x, z in pts)
    near = lambda pts: any(abs(x) < KEEP and abs(z) < KEEP for x, z in pts)

    buildings, roads, areas, trees, pois, landmarks, built_up = [], [], [], [], [], [], []
    for e in els:
        t = e.get("tags", {})
        if t.get("name") in LANDMARKS:
            pts = [xy[e["id"]]] if e["type"] == "node" else [xy[n] for n in e.get("nodes", []) if n in xy]
            if pts and not any(l["kind"] == LANDMARKS[t["name"]] for l in landmarks):
                cx, cz = centroid(pts)
                landmarks.append({"kind": LANDMARKS[t["name"]], "x": round(cx, 2), "z": round(cz, 2)})
            if e["type"] == "way" and "building" in t:
                continue
        # Ruins (Hauz Khas's madrasa and tombs) stand as low stone buildings.
        if e["type"] == "way" and t.get("historic") in ("ruins", "building", "tomb", "fort") and "building" not in t and e["nodes"][0] == e["nodes"][-1]:
            r = ring_of(e, xy)
            if r and inside(r) and abs(area(r)) < 3000:
                buildings.append({"pts": r, "h": 7.5, "kind": "ruins", **({"name": t["name"]} if "name" in t else {})})
            continue
        if e["type"] == "way" and "building" in t:
            if underground(t):
                continue
            r = ring_of(e, xy)
            if r and inside(r):
                h = height_of(t)
                b = {"pts": r, "h": round(h, 1), "kind": t.get("building", "yes")}
                if t.get("building") == "roof":
                    b["base"] = round(h - 0.4, 1)
                if t.get("name"):
                    b["name"] = t["name"]
                buildings.append(b)
        elif e["type"] == "way" and t.get("highway") in ROAD_WIDTH:
            pts = [xy[n] for n in e["nodes"] if n in xy]
            if len(pts) >= 2 and near(pts):
                w = ROAD_WIDTH[t["highway"]]
                try:
                    w = max(w, float(t["lanes"]) * 3.4)
                except (KeyError, ValueError):
                    pass
                kind = "path" if t["highway"] in PATHS else t["highway"]
                roads.append({"kind": kind, "w": w, "pts": [[round(x, 2), round(z, 2)] for x, z in simplify(pts, 0.3)],
                              **({"name": t["name"]} if "name" in t else {})})
        elif e["type"] == "way":
            if t.get("landuse") in BUILT_UP:
                r = ring_of(e, xy)
                if r:
                    built_up.append(r)
            kind = next((AREA_KIND[(k, t[k])] for k in ("leisure", "landuse", "amenity", "natural", "water") if (k, t.get(k)) in AREA_KIND), None)
            if kind:
                r = ring_of(e, xy)
                if r and near(r):
                    areas.append({"kind": kind, "pts": r, **({"name": t["name"]} if "name" in t else {})})
        elif e["type"] == "node" and t.get("natural") == "tree":
            x, z = xy[e["id"]]
            if abs(x) < KEEP and abs(z) < KEEP:
                trees.append([round(x, 2), round(z, 2)])
    # Areas mapped as multipolygons (a lake, a park): their outer rings.
    way_nodes = {e["id"]: e["nodes"] for e in els if e["type"] == "way"}
    for e in els:
        t = e.get("tags", {})
        if e["type"] != "relation":
            continue
        kind = next((AREA_KIND[(k, t[k])] for k in ("leisure", "landuse", "amenity", "natural", "water") if (k, t.get(k)) in AREA_KIND), None)
        for nodes in outer_rings(e, way_nodes) if kind or t.get("landuse") in BUILT_UP else []:
            r = ring_of({"nodes": nodes}, xy)
            if not r:
                continue
            if t.get("landuse") in BUILT_UP:
                built_up.append(r)
            if kind and near(r):
                areas.append({"kind": kind, "pts": r, **({"name": t["name"]} if "name" in t else {})})
    # A landmark the game builds is often also mapped as a plain building (India Gate as a 40 m
    # block): that one goes.
    buildings = [b for b in buildings if not any(math.dist(centroid(b["pts"]), (l["x"], l["z"])) < 15.0 for l in landmarks)]
    if cfg["fill"]:
        added = fill(buildings, roads, areas, built_up, cfg["fill"] == "all")
        buildings += added
        print(f"[delhi {name}] filled {len(added)} lots with houses")

    def poi(n, x, z, r):
        if abs(x) < HALF - 15 and abs(z) < HALF - 15 and not any(p["name"] == n for p in pois):
            pois.append({"name": n, "x": round(x, 1), "z": round(z, 1), "r": r})

    if name == "cp":
        # Connaught Place's lettered blocks are mapped as retail areas (whole sectors, roads and
        # car parks included): only their names are used. The game builds the inner circle's
        # blocks itself, at the real radii (crates/shared/src/map/delhi.rs).
        poi("Rajiv Chowk", 0.0, -shift, 110.0)
        for a in areas:
            if a.get("name") == "Palika Park":
                poi("Palika Bazaar", *centroid(a["pts"]), 40.0)
        for e in els:
            t = e.get("tags", {})
            if e["type"] == "way" and t.get("landuse") == "retail" and t.get("name", "").replace(" ", "-").endswith("-Block"):
                r = ring_of(e, xy)
                if r:
                    poi(t["name"].replace(" ", "-"), *centroid(r), 40.0)
        for r in roads:
            if r.get("name") == "Janpath":
                poi("Janpath", *min(r["pts"], key=lambda q: abs(q[1] - 200)), 40.0)
    else:
        # Named parks, water, landmarks and big named buildings, then the main roads.
        named = [(a["name"], centroid(a["pts"]), max(20.0, min(70.0, math.sqrt(abs(area(a["pts"]))) * 0.6))) for a in areas if a.get("name")]
        named += [(b["name"], centroid(b["pts"]), 30.0) for b in buildings if b.get("name") and abs(area(b["pts"])) > 300]
        named += [(l_name, (l["x"], l["z"]), 40.0) for l_name, k in LANDMARKS.items() for l in landmarks if l["kind"] == k]
        for e in els:
            t = e.get("tags", {})
            if e["type"] == "node" and t.get("name") and (t.get("amenity") == "place_of_worship" or t.get("historic") or t.get("tourism") == "attraction"):
                named.append((t["name"], xy[e["id"]], 30.0))
        for n, (x, z), r in named:
            if len(pois) < 9:
                poi(n, x, z, r)
        for r in sorted((r for r in roads if r.get("name") and r["kind"] not in PATHS), key=lambda r: -r["w"]):
            if len(pois) < 12:
                p = min(r["pts"], key=lambda q: math.hypot(*q))
                poi(r["name"], p[0], p[1], 35.0)
    out = {
        "attribution": "Map data (c) OpenStreetMap contributors, ODbL 1.0 (openstreetmap.org/copyright). Converted by tools/osm/delhi.py.",
        "title": cfg["title"],
        "origin": [round(lat0, 6), round(lon0, 6)],
        "south_shift": shift,
        "buildings": buildings, "roads": roads, "areas": areas, "trees": trees, "pois": pois, "landmarks": landmarks,
    }
    os.makedirs(os.path.dirname(out_of(name)), exist_ok=True)
    with open(out_of(name), "w") as f:
        json.dump(out, f, separators=(",", ":"))
    print(f"[delhi {name}] {len(buildings)} buildings, {len(roads)} roads and paths, {len(areas)} areas, {len(trees)} trees, "
          f"{len(landmarks)} landmarks, {len(pois)} places -> {out_of(name)} ({os.path.getsize(out_of(name)) // 1024} KB)")
    print(f"[delhi {name}] places:", ", ".join(p["name"] for p in pois))


if __name__ == "__main__":
    args = sys.argv[1:]
    if args[:1] == ["fetch"]:
        for a in args[1:] or list(AREAS):
            fetch(a)
    else:
        for a in args or list(AREAS):
            convert(a)

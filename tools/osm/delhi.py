"""Panga's Delhi map from OpenStreetMap: Connaught Place and the head of Janpath.

python3 tools/osm/delhi.py fetch     download the area from the Overpass API to build/osm/cp_raw.json
python3 tools/osm/delhi.py           convert it to assets/data/maps/delhi_cp.json

The map's origin is the middle of Central Park (Rajiv Chowk), shifted south so the 500 m square
holds the inner circle and the head of Janpath. Game axes: x east, z south (north is -z), metres.
Buildings are footprints (outer ring, simplified, counter-clockwise seen from above) with a height
from building:levels or height, else a guess by building type; roads are centrelines with a width
by class; areas are parks, lawns, pitches and car parks; trees are OSM's mapped trees.

Map data (c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright).
"""

import json
import math
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
RAW = os.path.join(ROOT, "build", "osm", "cp_raw.json")
OUT = os.path.join(ROOT, "assets", "data", "maps", "delhi_cp.json")
BBOX = "28.62974,77.21663,28.63514,77.22277"
# Kept out to here (the playable square is +-250; buildings beyond it make the skyline).
KEEP = 300.0
SOUTH_SHIFT = 40.0

ROAD_WIDTH = {
    "primary": 16.0, "secondary": 14.0, "tertiary": 10.0, "secondary_link": 8.0, "tertiary_link": 7.0,
    "residential": 7.0, "unclassified": 7.0, "service": 5.0, "pedestrian": 6.0, "living_street": 6.0,
}
AREA_KIND = {
    ("leisure", "park"): "park", ("leisure", "garden"): "park", ("landuse", "grass"): "park",
    ("leisure", "pitch"): "pitch", ("amenity", "parking"): "parking", ("leisure", "playground"): "park",
}


def fetch():
    q = (f'[out:json][timeout:90];(way["building"]({BBOX});way["highway"]({BBOX});way["leisure"]({BBOX});'
         f'way["landuse"]({BBOX});way["natural"]({BBOX});node["natural"="tree"]({BBOX});node["name"]({BBOX});'
         f'way["amenity"]({BBOX}););out body;>;out skel qt;')
    os.makedirs(os.path.dirname(RAW), exist_ok=True)
    subprocess.run(["curl", "-s", "-m", "150", "-A", "vivad-map-builder/0.1", "-H", "Accept: application/json", "-o", RAW,
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


def convert():
    d = json.load(open(RAW))
    els = d["elements"]
    lonlat = {e["id"]: (e["lon"], e["lat"]) for e in els if e["type"] == "node"}
    # Origin: the middle of Rajiv Chowk's Central Park, then shifted south.
    park = next(e for e in els if e["type"] == "way" and e.get("tags", {}).get("name") == "Rajiv Chowk Central Park")
    ring = [lonlat[n] for n in park["nodes"] if n in lonlat]
    lon0 = sum(p[0] for p in ring) / len(ring)
    lat0 = sum(p[1] for p in ring) / len(ring)
    my = 111320.0
    mx = my * math.cos(math.radians(lat0))
    xy = {k: ((lon - lon0) * mx, -(lat - lat0) * my - SOUTH_SHIFT) for k, (lon, lat) in lonlat.items()}
    inside = lambda pts: all(abs(x) < KEEP and abs(z) < KEEP for x, z in pts)
    near = lambda pts: any(abs(x) < KEEP and abs(z) < KEEP for x, z in pts)

    buildings, roads, areas, trees, pois = [], [], [], [], []
    for e in els:
        t = e.get("tags", {})
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
                roads.append({"kind": t["highway"], "w": w, "pts": [[round(x, 2), round(z, 2)] for x, z in simplify(pts, 0.3)],
                              **({"name": t["name"]} if "name" in t else {})})
        elif e["type"] == "way":
            kind = next((AREA_KIND[(k, t[k])] for k in ("leisure", "landuse", "amenity") if (k, t.get(k)) in AREA_KIND), None)
            if kind:
                r = ring_of(e, xy)
                if r and near(r):
                    areas.append({"kind": kind, "pts": r, **({"name": t["name"]} if "name" in t else {})})
        elif e["type"] == "node" and t.get("natural") == "tree":
            x, z = xy[e["id"]]
            if abs(x) < KEEP and abs(z) < KEEP:
                trees.append([round(x, 2), round(z, 2)])
    # Connaught Place's lettered blocks are mapped as retail areas (whole sectors, roads and car
    # parks included): only their names are used, for places on the map. The game builds the
    # inner circle's blocks itself, at the real radii (crates/shared/src/map/delhi.rs).
    centroid = lambda pts: (sum(p[0] for p in pts) / len(pts), sum(p[1] for p in pts) / len(pts))
    block_names = []
    for e in els:
        t = e.get("tags", {})
        if e["type"] == "way" and t.get("landuse") == "retail" and t.get("name", "").replace(" ", "-").endswith("-Block"):
            r = ring_of(e, xy)
            if r:
                block_names.append((t["name"].replace(" ", "-"), centroid(r)))
    # Named places for the map: the circles and markets, sized to what they cover.
    def poi(name, x, z, r):
        pois.append({"name": name, "x": round(x, 1), "z": round(z, 1), "r": r})
    poi("Rajiv Chowk", 0.0, -SOUTH_SHIFT, 110.0)
    for a in areas:
        if a.get("name") in ("Palika Park",):
            cx = sum(p[0] for p in a["pts"]) / len(a["pts"])
            cz = sum(p[1] for p in a["pts"]) / len(a["pts"])
            poi("Palika Bazaar", cx, cz, 40.0)
    for name, (cx, cz) in sorted(block_names):
        if abs(cx) < 235 and abs(cz) < 235:
            poi(name, cx, cz, 40.0)
    for r in roads:
        if r.get("name") == "Janpath":
            p = min(r["pts"], key=lambda q: abs(q[1] - 200))
            if abs(p[0]) < 240 and abs(p[1]) < 240:
                poi("Janpath", p[0], p[1], 40.0)
                break
    out = {
        "attribution": "Map data (c) OpenStreetMap contributors, ODbL 1.0 (openstreetmap.org/copyright). Converted by tools/osm/delhi.py.",
        "origin": [round(lat0, 6), round(lon0, 6)],
        "south_shift": SOUTH_SHIFT,
        "buildings": buildings, "roads": roads, "areas": areas, "trees": trees, "pois": pois,
    }
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(out, f, separators=(",", ":"))
    print(f"[delhi] {len(buildings)} buildings, {len(roads)} roads, {len(areas)} areas, {len(trees)} trees, "
          f"{len(pois)} places -> {OUT} ({os.path.getsize(OUT) // 1024} KB)")
    print("[delhi] places:", ", ".join(p["name"] for p in pois))


if __name__ == "__main__":
    if sys.argv[1:] == ["fetch"]:
        fetch()
    else:
        convert()

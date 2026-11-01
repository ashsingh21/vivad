"""Panga's NCR districts from OpenStreetMap, beside the four Delhi maps.

python3 tools/osm/ncr.py fetch [area ...]   download to build/osm/ncr_<area>_raw.json
python3 tools/osm/ncr.py [area ...]         convert to assets/data/maps/ncr_<area>.json

It reuses tools/osm/delhi.py's converter and never rewrites that tool's four delhi_*.json files.
New here: fast roads (NH-48, the DND), a height range for buildings OSM has no height for, and a
list of holy sites, so a district arrives with its no-loot places already marked.

Map data (c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright).
"""

import json
import math
import os
import sys
import time
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(__file__))
import delhi  # noqa: E402

# Centre, title, and metres of height for a building OSM gives none for.
AREAS = {
    "karolbagh": dict(title="Karol Bagh", center=(28.6470, 77.1895), tall=(9.0, 18.0)),
    "janakpuri": dict(title="Janakpuri", center=(28.6295, 77.0779), tall=(9.0, 15.0)),
    "nehruplace": dict(title="Nehru Place", center=(28.5494, 77.2517), tall=(14.0, 40.0)),
    "saket": dict(title="Saket", center=(28.5286, 77.2190), tall=(9.0, 24.0)),
    "cybercity": dict(title="Cyber City", center=(28.4962, 77.0900), tall=(60.0, 100.0)),
    "mgroad": dict(title="MG Road", center=(28.4805, 77.0825), tall=(14.0, 45.0)),
    "golfcourse": dict(title="Golf Course Road", center=(28.4277, 77.1052), tall=(60.0, 150.0)),
    "laxminagar": dict(title="Laxmi Nagar", center=(28.6306, 77.2775), tall=(9.0, 18.0)),
    "anandvihar": dict(title="Anand Vihar", center=(28.6480, 77.3155), tall=(9.0, 24.0)),
    "vaishali": dict(title="Vaishali", center=(28.6497, 77.3396), tall=(30.0, 75.0)),
    "indirapuram": dict(title="Indirapuram", center=(28.6380, 77.3644), tall=(40.0, 75.0)),
    "ghaziabad": dict(title="Ghaziabad", center=(28.6680, 77.4400), tall=(8.0, 14.0)),
    "noida18": dict(title="Sector 18", center=(28.5698, 77.3236), tall=(14.0, 60.0)),
    "filmcity": dict(title="Film City", center=(28.5683, 77.3158), tall=(10.0, 20.0)),
    "greaternoida": dict(title="Pari Chowk", center=(28.4631, 77.5081), tall=(30.0, 75.0)),
    "expressway": dict(title="Expressway Towers", center=(28.5084, 77.3791), tall=(40.0, 110.0)),
    "aerocity": dict(title="Aerocity", center=(28.5500, 77.1215), tall=(14.0, 40.0)),
}
for a in AREAS.values():
    a.setdefault("fill", None)
delhi.AREAS.update(AREAS)
delhi.raw_of = lambda area: os.path.join(delhi.ROOT, "build", "osm", f"ncr_{area}_raw.json")
delhi.out_of = lambda area: os.path.join(delhi.ROOT, "assets", "data", "maps", f"ncr_{area}.json")
delhi.ROAD_WIDTH.update({"motorway": 24.0, "trunk": 20.0, "motorway_link": 8.0, "trunk_link": 8.0, "primary_link": 8.0})

ENDPOINTS = ["https://overpass-api.de/api/interpreter", "https://overpass.private.coffee/api/interpreter",
             "https://overpass.kumi.systems/api/interpreter"]


def query(area):
    lat, lon = AREAS[area]["center"]
    dlat = 330.0 / 111320.0
    dlon = 330.0 / (111320.0 * math.cos(math.radians(lat)))
    bb = f"{lat - dlat:.6f},{lon - dlon:.6f},{lat + dlat:.6f},{lon + dlon:.6f}"
    return (f'[out:json][timeout:120];(way["building"]({bb});relation["building"]({bb});way["highway"]({bb});way["leisure"]({bb});'
            f'way["landuse"]({bb});way["natural"]({bb});node["natural"="tree"]({bb});node["name"]({bb});way["amenity"]({bb});'
            f'way["historic"]({bb});way["water"]({bb});way["railway"]({bb});relation["natural"="water"]({bb});'
            f'relation["leisure"]({bb});relation["landuse"]({bb}););out body;>;out skel qt;')


def fetch(area, tries=8):
    path = delhi.raw_of(area)
    if os.path.exists(path) and os.path.getsize(path) > 2000:
        return f"{area}: already have {os.path.getsize(path) // 1024} KB"
    os.makedirs(os.path.dirname(path), exist_ok=True)
    body = urllib.parse.urlencode({"data": query(area)}).encode()
    last = ""
    for k in range(tries):
        url = ENDPOINTS[k % len(ENDPOINTS)]
        try:
            req = urllib.request.Request(url, body, {"User-Agent": "vivad-map-builder/0.1", "Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=170) as r:
                data = r.read()
            if json.loads(data).get("elements"):
                with open(path, "wb") as f:
                    f.write(data)
                return f"{area}: {len(data) // 1024} KB from {url.split('/')[2]}"
            last = "empty"
        except Exception as e:
            last = repr(e)[:90]
        time.sleep(4 + 3 * k)
    return f"{area}: FAILED {last}"


def holy_of(els, xy):
    """Places of worship, graves and memorials: world x, z and a radius of the site plus 20 m."""
    out = []
    for e in els:
        t = e.get("tags", {})
        if not (t.get("amenity") == "place_of_worship" or t.get("landuse") in ("cemetery", "religious") or t.get("amenity") == "grave_yard"
                or t.get("historic") in ("tomb", "memorial", "wayside_shrine", "mosque", "temple") or t.get("building") in ("mosque", "temple", "church", "gurdwara", "shrine")):
            continue
        pts = [xy[e["id"]]] if e["type"] == "node" and e["id"] in xy else [xy[n] for n in e.get("nodes", []) if n in xy]
        if pts:
            c = delhi.centroid(pts)
            r = max(25.0, max(math.dist(c, p) for p in pts) + 20.0)
            out.append([round(c[0], 1), round(c[1], 1), round(r, 1)])
    return out


def spread_heights(area):
    """Give buildings with no mapped height one from the district's range, the same every run."""
    path = delhi.out_of(area)
    d = json.load(open(path))
    els = json.load(open(delhi.raw_of(area)))["elements"]
    tagged = sum(1 for e in els if e.get("tags", {}).get("building") and ("height" in e["tags"] or "building:levels" in e["tags"]))
    lo, hi = AREAS[area]["tall"]
    guessed = 0
    for b in d["buildings"]:
        if b["h"] in (7.0, 8.0, 9.0, 10.0, 10.5, 11.0, 14.0, 16.0) and b["kind"] not in ("roof", "ruins", "house", "mosque", "temple"):
            x, z = delhi.centroid(b["pts"])
            k = (int(x * 7) * 73856093 ^ int(z * 7) * 19349663) & 0xFFFF
            b["h"] = round(lo + (hi - lo) * k / 0xFFFF, 1)
            guessed += 1
    lat0, lon0 = d["origin"]
    my = 111320.0
    mx = my * math.cos(math.radians(lat0))
    xy = {e["id"]: ((e["lon"] - lon0) * mx, -(e["lat"] - lat0) * my) for e in els if e["type"] == "node"}
    d["holy"] = holy_of(els, xy)
    with open(path, "w") as f:
        json.dump(d, f, separators=(",", ":"))
    print(f"[ncr {area}] heights guessed for {guessed} of {len(d['buildings'])} (OSM tags {tagged}); {len(d['holy'])} holy sites")


if __name__ == "__main__":
    args = sys.argv[1:]
    if args[:1] == ["fetch"]:
        with ThreadPoolExecutor(3) as pool:
            for line in pool.map(fetch, args[1:] or list(AREAS)):
                print(line, flush=True)
    else:
        for a in args or list(AREAS):
            delhi.convert(a)
            spread_heights(a)

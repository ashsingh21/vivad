"""Panga's NCR districts from OpenStreetMap, beside the four Delhi maps.

python3 tools/osm/ncr.py fetch [area ...]   download to build/osm/ncr_<area>_raw.json
python3 tools/osm/ncr.py [area ...]         convert to assets/data/maps/ncr_<area>.json
python3 tools/osm/ncr.py holy [area ...]    write only the reviewed holy sites into those files

It reuses tools/osm/delhi.py's converter and never rewrites that tool's four delhi_*.json files.
New here: fast roads (NH-48, the DND), a height range for buildings OSM has no height for, and a
reviewed list of holy sites, so a district arrives with its no-loot places already marked.

Map data (c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright).
Some holy sites come from Overture Places (CDLA Permissive 2.0); see docs/ncr/HOLY-SITES.md.
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

# Reviewed holy sites, the ones whose circle reaches a district's 500 m square. Nothing is shown for
# them: no loot lands inside a circle, no name goes on screen, and no zone ends near one. An empty
# list means no site turned up in any source, not that nobody looked. docs/ncr/HOLY-SITES.md has the
# sources, the doubts and the sites just outside the squares.
#
# Each row: lat, lon, radius in metres, what it is, its name, where the position came from, how sure.
# Position: "osm" or "overture" is that data's own point. "placed" is our own choice of a point on an
# OpenStreetMap road or building beside the site, for a site known only from a web map or an address.
# Web map positions are never copied in here: these files are ODbL data and those maps' terms forbid it.
# How sure: "sure" has two sources that agree, "likely" has one good source, "doubt" is weak and kept
# because a wrong circle costs a little loot, and a missing one puts loot in a temple.
HOLY = {
    "karolbagh": [
        (28.649134, 77.191063, 30, "mandir", "Arya Samaj Mandir, 13/39 Arya Samaj Road", "overture", "sure"),
        (28.648506, 77.190098, 30, "mandir", "Shree Shakti Peeth Jai Mata Mandir, Padam Singh Road", "overture", "likely"),
        (28.646341, 77.191414, 30, "mandir", "Shiv Mandir, Gurudwara Road", "overture", "likely"),
        (28.647188, 77.189480, 30, "ashram", "Sai Baba Ashram", "overture", "likely"),
        (28.646417, 77.188045, 25, "ashram", "Ramakrishna Ashram Kutir", "osm", "likely"),
        (28.645965, 77.191936, 25, "memorial", "War memorial, no name mapped", "osm", "likely"),
        (28.649395, 77.188705, 30, "satsang", "Radhasoami Satsang Dayalbagh, 12/6 Padam Singh Road", "overture", "sure"),
        (28.647413, 77.189715, 30, "mandir", "Aarya Samaj Mandir, 26/5 Ajmal Khan Road", "placed", "likely"),
        (28.645536, 77.188927, 45, "mandir", "Shree Hanuman Mandir, Ajmal Khan Road", "placed", "sure"),
        (28.645176, 77.187135, 40, "mandir", "Shiv Temple, by Maharana Pratap Road", "placed", "likely"),
        (28.645868, 77.192080, 35, "mandir", "Karumariamman Temple, Abdul Aziz Road", "placed", "likely"),
        (28.646111, 77.188456, 35, "puja ground", "Karol Bagh Pooja Park, block 15A", "placed", "doubt"),
    ],
    "janakpuri": [
        (28.627449, 77.075748, 30, "mandir", "Shri Hare Ram Hanuman Mandir, Major Deepak Tyagi Marg", "overture", "likely"),
    ],
    "nehruplace": [
        (28.548762, 77.251690, 30, "masjid", "Masjid, one listing with no reviews", "placed", "doubt"),
        (28.549966, 77.250330, 35, "mandir", "Nishchhal Temple, 33 Nehru Place Road", "placed", "doubt"),
    ],
    "saket": [
        (28.527100, 77.218574, 105, "graves", "Qabristan", "osm", "sure"),
        (28.530590, 77.219642, 25, "mandir", "Prachin Shiv Shakti Durga Mandir", "osm", "sure"),
        (28.531058, 77.218432, 25, "mandir", "Hanuman Mandir, Khirki", "osm", "sure"),
        (28.531231, 77.218334, 48, "mandir", "Sai Baba Temple, Khirki Main Road", "osm", "sure"),
    ],
    "cybercity": [],
    "mgroad": [],
    "golfcourse": [
        (28.427547, 77.104577, 40, "mandir", "Temple at Jawahar Apartment-1, GH-54", "placed", "doubt"),
        (28.425948, 77.102493, 40, "mandir", "Technical Paradise Temple", "placed", "doubt"),
    ],
    "laxminagar": [
        (28.632582, 77.275980, 59, "vihara", "Buddha Mandir (Bodh Ashram), J-100", "osm", "sure"),
        (28.632865, 77.275561, 35, "mandir", "Vishal Shakti Mandir", "osm", "sure"),
        (28.629389, 77.275222, 30, "gurdwara", "Gurudwara Shri Guru Singh Sabha, Lalita Park", "osm", "sure"),
        (28.629609, 77.275624, 25, "mandir", "Shri Shiv Mandir", "osm", "likely"),
        (28.628632, 77.275919, 45, "mandir", "Pracheen Durga Mandir", "osm", "sure"),
        (28.628530, 77.276428, 41, "mandir", "Manokamana Siddh Durga Mandir", "osm", "sure"),
        (28.631577, 77.275944, 50, "church", "Sharon Church of Christ and Hanuman Temple, D-264 to D-267", "overture", "sure"),
        (28.632374, 77.279499, 30, "mandir", "Shree Pancheshwar Shiv Mandir", "overture", "likely"),
        (28.631380, 77.278419, 45, "mandir", "Upasana Mandir, Vikas Marg", "overture", "likely"),
        (28.629818, 77.276623, 30, "ashram", "Satnami Sadh Ashram, S-501 School Block", "overture", "likely"),
        (28.632145, 77.279189, 45, "mandir", "Shri Sai Baba Mandir, Vikas Marg", "placed", "sure"),
        (28.630465, 77.278042, 45, "mandir", "Shree Ram Mandir, Ram Mandir Gali", "placed", "sure"),
        (28.629890, 77.277684, 35, "gurdwara", "Gurudwara Sri Guru Singh Sabha, Gurudwara Street", "placed", "sure"),
        (28.631786, 77.278810, 30, "mandir", "Puncheswar Temple, a second pin for the Pancheshwar name", "placed", "doubt"),
        (28.629172, 77.279588, 35, "mandir", "Chatri Wala Shiv Goraksh Durgesh Mandir", "placed", "doubt"),
    ],
    "anandvihar": [
        (28.646985, 77.316032, 45, "mandir", "Small temple by the ISBT, its listing has a nonsense name", "placed", "doubt"),
    ],
    "vaishali": [
        (28.648482, 77.338624, 32, "mandir", "Shri Radhakrishna Mandir, Sector 4", "osm", "sure"),
        (28.648203, 77.341668, 45, "mandir", "Gopeshwar Mahadev Mandir", "osm", "sure"),
        (28.649632, 77.342125, 30, "mandir", "Shiv Mandir", "overture", "likely"),
    ],
    "indirapuram": [
        (28.638717, 77.366151, 33, "mandir", "Shiv Shakti Temple, Dr Sushila Naiyar Marg", "osm", "sure"),
        (28.638323, 77.362414, 35, "mandir", "Aditya Mega City Temple", "placed", "sure"),
        (28.637533, 77.366058, 40, "mandir", "Krishna Apra Sapphire Temple", "placed", "likely"),
    ],
    "ghaziabad": [
        (28.669860, 77.438137, 45, "mandir", "Shri Vedmata Gayatri Dhaam, Basant Road", "placed", "likely"),
        (28.666419, 77.438536, 30, "mandir", "Gopal, a temple pin with no other detail", "placed", "doubt"),
    ],
    "noida18": [
        (28.567796, 77.323685, 64, "gurdwara", "Gurudwara Sri Guru Singh Sabha, Sector 18", "osm", "sure"),
        (28.569073, 77.320789, 30, "mandir", "Shiv Shani Mandir", "overture", "sure"),
        (28.568716, 77.322392, 30, "mandir", "Sai Temple", "overture", "doubt"),
    ],
    "filmcity": [
        (28.566108, 77.317559, 50, "statue", "Giant Shiv statue", "placed", "sure"),
        (28.566656, 77.313396, 95, "memorial", "Rashtriya Dalit Prerna Sthal, its east tip", "osm", "sure"),
        (28.567879, 77.314545, 30, "mandir", "Shiv Nandi, Express Trade Tower", "overture", "doubt"),
    ],
    "greaternoida": [
        (28.462460, 77.508255, 40, "mandir", "Shiv Parvati Mandir", "overture", "sure"),
        (28.462920, 77.508478, 40, "mandir", "Jhandewalan Temple", "placed", "likely"),
        (28.462453, 77.508938, 35, "mandir", "PanchMukhi Hanuman Mandir", "placed", "likely"),
        (28.461878, 77.508182, 40, "mandir", "Shree Ram Mandir", "placed", "likely"),
        (28.461717, 77.507793, 45, "mandir", "Jhande Wale Baba Mandir and the temple beside it", "placed", "sure"),
        (28.464600, 77.506782, 40, "mandir", "Temple on Shiv Mandir Road, Tugalpur", "placed", "likely"),
    ],
    "expressway": [],
    "aerocity": [],
}

ENDPOINTS =["https://overpass-api.de/api/interpreter", "https://overpass.private.coffee/api/interpreter",
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


def to_xz(area):
    """Turn a lat and lon into world x and z, the way delhi.py turns the map's own points."""
    lat0, lon0 = AREAS[area]["center"]
    my = 111320.0
    mx = my * math.cos(math.radians(lat0))
    return lambda lat, lon: ((lon - lon0) * mx, -(lat - lat0) * my)


def holy_of(area):
    """A district's reviewed sites as world x, z and a radius."""
    xz = to_xz(area)
    return [[round(xz(s[0], s[1])[0], 1), round(xz(s[0], s[1])[1], 1), float(s[2])] for s in HOLY[area]]


def osm_gaps(area, els):
    """Anything OpenStreetMap marks as holy in the square that no reviewed circle covers. New raw
    data can bring new sites, so this stops them going unnoticed."""
    xz = to_xz(area)
    xy = {e["id"]: xz(e["lat"], e["lon"]) for e in els if e["type"] == "node"}
    gaps = []
    for e in els:
        t = e.get("tags", {})
        if not (t.get("amenity") in ("place_of_worship", "grave_yard") or t.get("landuse") in ("cemetery", "religious")
                or t.get("historic") in ("tomb", "memorial", "wayside_shrine", "mosque", "temple") or t.get("building") in ("mosque", "temple", "church", "gurdwara", "shrine")):
            continue
        pts = [xy[e["id"]]] if e["type"] == "node" and e["id"] in xy else [xy[n] for n in e.get("nodes", []) if n in xy]
        if pts:
            c = delhi.centroid(pts)
            if max(abs(c[0]), abs(c[1])) <= delhi.HALF and not any(math.dist(c, s[:2]) < s[2] for s in holy_of(area)):
                gaps.append(t.get("name", f"{e['type']} {e['id']}"))
    return gaps


def write_holy(area):
    """Put the reviewed sites into a district's file and change nothing else in it."""
    path = delhi.out_of(area)
    with open(path) as f:
        d = json.load(f)
    d["holy"] = holy_of(area)
    with open(path, "w") as f:
        json.dump(d, f, separators=(",", ":"))
    with open(delhi.raw_of(area)) as f:
        gaps = osm_gaps(area, json.load(f)["elements"])
    print(f"[ncr {area}] {len(d['holy'])} holy sites; on OpenStreetMap but in no circle: {', '.join(gaps) or 'none'}")


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
    d["holy"] = holy_of(area)
    with open(path, "w") as f:
        json.dump(d, f, separators=(",", ":"))
    print(f"[ncr {area}] heights guessed for {guessed} of {len(d['buildings'])} (OSM tags {tagged}); {len(d['holy'])} holy sites")


if __name__ == "__main__":
    args = sys.argv[1:]
    if args[:1] == ["fetch"]:
        with ThreadPoolExecutor(3) as pool:
            for line in pool.map(fetch, args[1:] or list(AREAS)):
                print(line, flush=True)
    elif args[:1] == ["holy"]:
        for a in args[1:] or list(AREAS):
            write_holy(a)
    else:
        for a in args or list(AREAS):
            delhi.convert(a)
            spread_heights(a)

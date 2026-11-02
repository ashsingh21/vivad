"""Cut the NCR districts out of Geofabrik's northern India OpenStreetMap file.

build/osm/venv/bin/python tools/osm/ncr_extract.py build/osm/northern-zone.osm.pbf build/osm/central-zone.osm.pbf

Delhi and Haryana are in the northern zone's file, Uttar Pradesh in the central zone's. Reading
local files replaces a slow public server, and it writes the same raw JSON the Overpass
query gave, one build/osm/ncr_<area>_raw.json a district, so tools/osm/ncr.py converts it as usual.
Map data (c) OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright).
"""

import json
import math
import os
import re
import sys

import osmium

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SRC = open(os.path.join(os.path.dirname(__file__), "ncr.py")).read()
AREAS = {m[0]: (float(m[1]), float(m[2])) for m in re.findall(r'"(\w+)": dict\(title="[^"]+", center=\(([\d.]+), ([\d.]+)\)', SRC)}
REACH = 330.0
KEYS = ("building", "highway", "leisure", "landuse", "natural", "amenity", "historic", "water", "railway")


def box(lat, lon):
    dlat = REACH / 111320.0
    dlon = REACH / (111320.0 * math.cos(math.radians(lat)))
    return lat - dlat, lon - dlon, lat + dlat, lon + dlon


BOXES = {a: box(*c) for a, c in AREAS.items()}
ALL = (min(b[0] for b in BOXES.values()) - 0.03, min(b[1] for b in BOXES.values()) - 0.03,
       max(b[2] for b in BOXES.values()) + 0.03, max(b[3] for b in BOXES.values()) + 0.03)


def inside(lat, lon, b):
    return b[0] <= lat <= b[2] and b[1] <= lon <= b[3]


def main(paths):
    # First pass: every node in the region; second: the ways and relations that touch a district.
    nodes, tagged = {}, {}
    print("reading nodes", flush=True)
    for path in paths:
        for n in osmium.FileProcessor(path, osmium.osm.NODE):
            lat, lon = n.location.lat, n.location.lon
            if inside(lat, lon, ALL):
                nodes[n.id] = (lat, lon)
                if len(n.tags):
                    tagged[n.id] = dict(n.tags)
    print(len(nodes), "nodes", flush=True)
    per = {a: {"ways": {}, "rels": {}, "nodes": set()} for a in AREAS}
    ways = {}
    print("reading ways", flush=True)
    for path in paths:
        for w in osmium.FileProcessor(path, osmium.osm.WAY):
            refs = [r.ref for r in w.nodes]
            if not any(r in nodes for r in refs):
                continue
            ways[w.id] = (refs, dict(w.tags))
            if not any(k in w.tags for k in KEYS):
                continue
            for a, b in BOXES.items():
                if any(r in nodes and inside(*nodes[r], b) for r in refs):
                    per[a]["ways"][w.id] = True
    print(len(ways), "ways", flush=True)
    print("reading relations", flush=True)
    for path in paths:
        for r in osmium.FileProcessor(path, osmium.osm.RELATION):
            tags = dict(r.tags)
            if not ("building" in tags or tags.get("natural") == "water" or "leisure" in tags or "landuse" in tags):
                continue
            members = [{"type": {"n": "node", "w": "way", "r": "relation"}[m.type], "ref": m.ref, "role": m.role} for m in r.members]
            for a, b in BOXES.items():
                if any(m["type"] == "way" and m["ref"] in ways and any(q in nodes and inside(*nodes[q], b) for q in ways[m["ref"]][0]) for m in members):
                    per[a]["rels"][r.id] = (tags, members)
    for a, b in BOXES.items():
        els, seen = [], set()
        p = per[a]
        ids = set(p["ways"])
        for tags, members in p["rels"].values():
            ids |= {m["ref"] for m in members if m["type"] == "way" and m["ref"] in ways}
        for i in ids:
            refs, tags = ways[i]
            refs = [r for r in refs if r in nodes]
            seen.update(refs)
            els.append({"type": "way", "id": i, "nodes": refs, "tags": tags})
        for i, (lat, lon) in nodes.items():
            has_tags = i in tagged and ("name" in tagged[i] or tagged[i].get("natural") == "tree" or "amenity" in tagged[i] or "historic" in tagged[i])
            if i in seen or (has_tags and inside(lat, lon, b)):
                e = {"type": "node", "id": i, "lat": lat, "lon": lon}
                if i in tagged:
                    e["tags"] = tagged[i]
                els.append(e)
        for i, (tags, members) in p["rels"].items():
            els.append({"type": "relation", "id": i, "members": members, "tags": tags})
        out = os.path.join(ROOT, "build", "osm", f"ncr_{a}_raw.json")
        with open(out, "w") as f:
            json.dump({"elements": els}, f, separators=(",", ":"))
        print(a, len(els), "elements", os.path.getsize(out) // 1024, "KB", flush=True)


if __name__ == "__main__":
    main(sys.argv[1:])

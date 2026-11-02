#!/usr/bin/env python3
"""Checks the split character files (assets/models/characters/<variant>_parts.glb) without Blender
or the game, from the files alone:

  rig      same joints in the same order, same hierarchy and rest pose as the clips
           (<variant>_anims.glb) and the old one-mesh <variant>.glb, same inverse bind matrices;
           every clip target resolves to a node
  parts    head, body, outfit, eyes, cornea, mouth and the cards, each its own mesh and material on the
           LODs it should be on, named <variant>_<part>_LOD<k>, skinned to the one skin
  skin     joint indices in range, weights sum to 1
  budget   triangles per LOD and part against docs/PLAN.md, the LOD0 head's 15-20k
  neck     head and body meet: no gap, no normal break and the same bone weights along the cut,
           on every LOD
  shape    per LOD, against the old LOD0: bounding box, and how far the surface moved
  texture  no triangle sits on the unbaked black between the UV islands
  size     texture sizes, file size, GPU memory, the Ultra head textures on disk

  python3 tools/check_characters.py [male female]    exit status 1 on any failure
"""

import io
import json
import os
import re
import struct
import sys

import numpy as np
from PIL import Image

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
DIR = os.path.join(ROOT, "assets", "models", "characters")

# Which LODs each part is on (the rest must be absent) and the part's largest count there.
PARTS = {"head": 4, "body": 4, "outfit": 4, "eyes": 4, "cornea": 2, "mouth": 2, "hair": 4, "eyebrows": 3, "eyelashes": 2}
# Head + body + outfit, per LOD: (most triangles, slack). LOD0 is the sum of its sources for now.
BUDGET = [60000, 25000 * 1.1, 12000 * 1.1, 5000 * 1.1]
HEAD_TRIS = (15000, 20000)
# Texture size by material suffix.
TEX = {"head": 2048, "body": 2048, "outfit": 2048, "eyes": 512, "mouth": 512}

COMPONENTS = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


class Glb:
    def __init__(self, path):
        with open(path, "rb") as f:
            self.bytes = f.read()
        n = struct.unpack("<I", self.bytes[12:16])[0]
        self.j = json.loads(self.bytes[20:20 + n])
        self.bin = 20 + n + 8

    def accessor(self, i):
        a = self.j["accessors"][i]
        v = self.j["bufferViews"][a["bufferView"]]
        kind = np.dtype(COMPONENTS[a["componentType"]])
        w = WIDTH[a["type"]]
        start = self.bin + v.get("byteOffset", 0) + a.get("byteOffset", 0)
        stride = v.get("byteStride", kind.itemsize * w)
        raw = np.frombuffer(self.bytes, dtype=np.uint8, count=stride * a["count"], offset=start)
        rows = raw.reshape(a["count"], stride)[:, :kind.itemsize * w]
        return np.frombuffer(rows.tobytes(), dtype=kind).reshape(a["count"], w).astype(np.float64 if kind == np.float32 else np.int64)

    def nodes(self):
        return self.j["nodes"]

    def paths(self):
        """Node name -> names from the scene root down, as the engine builds them."""
        out = {}
        stack = [(i, ()) for i in self.j["scenes"][0]["nodes"]]
        while stack:
            i, path = stack.pop()
            n = self.j["nodes"][i]
            path = path + (n["name"],)
            out[n["name"]] = path
            stack += [(c, path) for c in n.get("children", [])]
        return out

    def mesh_nodes(self):
        return [n for n in self.j["nodes"] if "mesh" in n]

    def geometry(self, node):
        """World-free positions and normals (the rig root carries no transform), triangles."""
        pos, nrm, tris = [], [], []
        base = 0
        for p in self.j["meshes"][node["mesh"]]["primitives"]:
            a = p["attributes"]
            pos.append(self.accessor(a["POSITION"]))
            nrm.append(self.accessor(a["NORMAL"]))
            tris.append(self.accessor(p["indices"]).reshape(-1, 3) + base)
            base += len(pos[-1])
        return np.vstack(pos), np.vstack(nrm), np.vstack(tris)

    def influences(self, node, count):
        """Per vertex, its weight on each joint (same vertex order as `geometry`)."""
        rows = []
        for p in self.j["meshes"][node["mesh"]]["primitives"]:
            joints = self.accessor(p["attributes"]["JOINTS_0"])
            weights = self.accessor(p["attributes"]["WEIGHTS_0"])
            if weights.max() > 1.5:   # normalised integers
                weights = weights / weights.max()
            out = np.zeros((len(joints), count))
            np.add.at(out, (np.arange(len(joints))[:, None], joints), weights)
            rows.append(out)
        return np.vstack(rows)

    def uvs(self, node):
        return np.vstack([self.accessor(p["attributes"]["TEXCOORD_0"]) for p in self.j["meshes"][node["mesh"]]["primitives"]])

    def texels(self, name):
        """A material's occlusion-roughness-metal texture as an array. Nothing bakes a roughness of
        exactly 0, so 0 there is empty texture (base colour can't tell: pupils are black)."""
        m = next(m for m in self.j["materials"] if m["name"] == name)
        tex = self.j["textures"][m["pbrMetallicRoughness"]["metallicRoughnessTexture"]["index"]]
        view = self.j["bufferViews"][self.j["images"][tex["source"]]["bufferView"]]
        data = self.bytes[self.bin + view.get("byteOffset", 0):][:view["byteLength"]]
        return np.asarray(Image.open(io.BytesIO(data)).convert("RGB"))

    def png_size(self, image):
        view = self.j["bufferViews"][image["bufferView"]]
        head = self.bytes[self.bin + view.get("byteOffset", 0):][:24]
        return struct.unpack(">II", head[16:24]) if head[:4] == b"\x89PNG" else None


class Report:
    def __init__(self):
        self.failed = 0

    def check(self, ok, what):
        print(f"  {'ok  ' if ok else 'FAIL'} {what}")
        self.failed += not ok


def compare_rig(new, other, label, rep):
    a, b = new.j["skins"][0], other.j["skins"][0]
    names = lambda g, s: [g.nodes()[i]["name"] for i in s["joints"]]
    rep.check(names(new, a) == names(other, b), f"{label}: {len(a['joints'])} joints, same order")
    pa, pb = new.paths(), other.paths()
    bones = names(other, b)
    rep.check(all(pa.get(n) == pb.get(n) for n in bones), f"{label}: same bone paths from Human.rig")
    worst = 0.0
    for i, j in zip(a["joints"], b["joints"]):
        x, y = new.nodes()[i], other.nodes()[j]
        for key, default in (("translation", [0, 0, 0]), ("rotation", [0, 0, 0, 1]), ("scale", [1, 1, 1])):
            worst = max(worst, float(np.abs(np.array(x.get(key, default)) - np.array(y.get(key, default))).max()))
    rep.check(worst < 1e-4, f"{label}: rest pose differs by at most {worst:.2e}")
    if "inverseBindMatrices" in a and "inverseBindMatrices" in b:
        d = float(np.abs(new.accessor(a["inverseBindMatrices"]) - other.accessor(b["inverseBindMatrices"])).max())
        rep.check(d < 1e-4, f"{label}: inverse bind matrices differ by at most {d:.2e}")


def boundary_vertices(pos, tris):
    """Positions on the surface's open edges; vertices split for UVs or normals count as one."""
    key = np.round(pos * 2e4).astype(np.int64)
    _, weld = np.unique(key, axis=0, return_inverse=True)
    weld = weld.ravel()
    t = weld[tris]
    edges = np.sort(np.vstack((t[:, [0, 1]], t[:, [1, 2]], t[:, [2, 0]])), axis=1)
    uniq, count = np.unique(edges, axis=0, return_counts=True)
    open_ids = np.unique(uniq[count == 1])
    return np.flatnonzero(np.isin(weld, open_ids))


def nearest(a, b, chunk=2000):
    """For each point of `a`: distance to, and index of, the nearest of `b`."""
    dist, index = [], []
    for s in range(0, len(a), chunk):
        d = np.linalg.norm(a[s:s + chunk, None, :] - b[None, :, :], axis=2)
        index.append(d.argmin(axis=1))
        dist.append(d.min(axis=1))
    return np.concatenate(dist), np.concatenate(index)


def check_variant(v, rep):
    path = os.path.join(DIR, f"{v}_parts.glb")
    print(f"{v}: {os.path.relpath(path, ROOT)}")
    if not os.path.exists(path):
        rep.check(False, "file missing")
        return
    new = Glb(path)
    print(" rig")
    compare_rig(new, Glb(os.path.join(DIR, f"{v}_anims.glb")), "clips", rep)
    old_path = os.path.join(DIR, f"{v}.glb")
    old = Glb(old_path) if os.path.exists(old_path) else None
    if old:
        compare_rig(new, old, "old model", rep)
    anims = Glb(os.path.join(DIR, f"{v}_anims.glb"))
    paths, clip_paths = new.paths(), anims.paths()
    targets = {anims.nodes()[c["target"]["node"]]["name"] for a in anims.j["animations"] for c in a["channels"]}
    rep.check(all(paths.get(n) == clip_paths[n] for n in targets), f"{len(anims.j['animations'])} clips: all {len(targets)} target bones resolve")
    joints = {new.nodes()[i]["name"] for i in new.j["skins"][0]["joints"]}
    rep.check(not any(n["name"] in joints for n in new.mesh_nodes()), "no mesh node shares a bone's name")

    print(" parts")
    table = {}
    for n in new.mesh_nodes():
        m = re.fullmatch(rf"{v}_(\w+?)_LOD(\d)", n["name"])
        rep.check(m is not None, f"name {n['name']}") if m is None else None
        if m:
            table[(m.group(1), int(m.group(2)))] = n
    want = {(p, k) for p, lods in PARTS.items() for k in range(lods)}
    rep.check(set(table) == want, f"{len(want)} mesh nodes, each part on its LODs" + (f"; differ: {sorted(set(table) ^ want)}" if set(table) != want else ""))
    rep.check(all(n.get("skin") == 0 for n in table.values()), "all skinned to the one skin")
    materials = new.j["materials"]
    for part in PARTS:
        used = {materials[p["material"]]["name"] for (q, k), n in table.items() if q == part for p in new.j["meshes"][n["mesh"]]["primitives"]}
        rep.check(used == {f"{v}_{part}"}, f"{part}: material {sorted(used)}")

    print(" skin")
    bad_joints, bad_weights = 0, 0
    count = len(new.j["skins"][0]["joints"])
    for n in table.values():
        for p in new.j["meshes"][n["mesh"]]["primitives"]:
            bad_joints += int((new.accessor(p["attributes"]["JOINTS_0"]) >= count).sum())
            w = new.accessor(p["attributes"]["WEIGHTS_0"]).sum(axis=1)
            bad_weights += int((np.abs(w - 1) > 0.01).sum())
    rep.check(bad_joints == 0, "joint indices in range")
    rep.check(bad_weights == 0, "weights sum to 1")

    print(" budget (triangles)")
    geo = {key: new.geometry(n) for key, n in table.items()}
    for k in range(4):
        row = {p: len(geo[(p, k)][2]) for p in PARTS if (p, k) in geo}
        main = sum(row[p] for p in ("head", "body", "outfit"))
        rep.check(main <= BUDGET[k], f"LOD{k}: head+body+outfit {main}, all {sum(row.values())}: {row}")
    rep.check(HEAD_TRIS[0] <= len(geo[("head", 0)][2]) <= HEAD_TRIS[1], f"LOD0 head {len(geo[('head', 0)][2])} triangles")
    glass = next(m for m in materials if m["name"] == f"{v}_cornea")
    rep.check(glass.get("alphaMode") == "BLEND", "cornea: alpha blended")

    print(" neck")
    for k in range(4):
        hp, hn, ht = geo[("head", k)]
        bp, bn, bt = geo[("body", k)]
        # The cut is the head's lowest open edge; eyes and mouth have open edges too, higher up.
        edge = boundary_vertices(hp, ht)
        floor = hp[:, 1].min()
        ring = edge[hp[edge, 1] < floor + 0.06]
        d, i = nearest(hp[ring], bp)
        unit = lambda a: a / np.linalg.norm(a, axis=1, keepdims=True)
        break_deg = np.degrees(np.arccos(np.clip((unit(hn[ring]) * unit(bn[i])).sum(axis=1), -1, 1)))
        tear = np.abs(new.influences(table[("head", k)], count)[ring] - new.influences(table[("body", k)], count)[i]).max()
        rep.check(d.max() < 2e-4 and break_deg.max() < 12.0 and tear < 0.02,
                  f"LOD{k}: {len(ring)} seam vertices, gap {d.max() * 1000:.2f} mm, normal break {break_deg.max():.1f} deg, weights differ by {tear:.3f}")

    print(" shape")
    ref = None
    if old:
        ref = next(old.geometry(n)[0] for n in old.mesh_nodes() if n["name"] == f"{v}_LOD0")
    for k in range(4):
        pts = np.vstack([geo[(p, k)][0] for p in ("head", "body", "outfit", "eyes") if (p, k) in geo])
        lo, hi = pts.min(axis=0), pts.max(axis=0)
        if ref is None:
            print(f"  LOD{k}: size {np.round(hi - lo, 3)}")
            continue
        box = float(max(np.abs(lo - ref.min(axis=0)).max(), np.abs(hi - ref.max(axis=0)).max()))
        # Vertex to nearest old vertex: an upper bound on how far the surface moved.
        d, _ = nearest(pts[::3], ref)
        # New clothes, hair and a bag change the outline; a loose vertex would change it far more.
        rep.check(box < 0.3, f"LOD{k}: box within {box * 100:.1f} cm of the old LOD0; surface moved p99 {np.percentile(d, 99) * 100:.1f} cm")

    print(" texture")
    for part in ("head", "body", "outfit", "eyes", "mouth"):
        tex = new.texels(f"{v}_{part}")
        h, w = tex.shape[:2]
        black = 0
        for k in range(PARTS[part]):
            uv = new.uvs(table[(part, k)])[geo[(part, k)][2]]      # triangles x corners x uv
            spots = np.concatenate((uv, uv.mean(axis=1, keepdims=True)), axis=1).reshape(-1, 2)
            px = tex[np.clip((spots[:, 1] * h).astype(int), 0, h - 1), np.clip((spots[:, 0] * w).astype(int), 0, w - 1)]
            black += int((px.reshape(-1, 4, 3)[:, 3, 1] == 0).sum())
        rep.check(black == 0, f"{part}: {black} triangles centred on empty texture, over its LODs")

    print(" size")
    total = 0
    for im in new.j["images"]:
        size = new.png_size(im)
        if size:
            total += size[0] * size[1] * 4 * 4 // 3
        part = next((p for p in TEX if im["name"] == f"{v}_{p}_basecolor"), None)
        if part and size:
            rep.check(size[0] == TEX[part], f"{im['name']}: {size[0]}x{size[1]}")
    print(f"  {len(new.j['images'])} images, {total / 1e6:.0f} MB in GPU memory uncompressed, file {len(new.bytes) / 1e6:.1f} MB")
    for kind in ("basecolor", "normal"):
        ktx = os.path.join(ROOT, "assets", "textures", "characters", f"{v}_head_4k_{kind}.ktx2")
        rep.check(os.path.exists(ktx), f"{os.path.basename(ktx)}: {os.path.getsize(ktx) / 1e6:.1f} MB" if os.path.exists(ktx) else f"{os.path.basename(ktx)} missing")


def main():
    variants = sys.argv[1:] or ["male", "female"]
    rep = Report()
    for v in variants:
        check_variant(v, rep)
    print("all good" if not rep.failed else f"{rep.failed} failed")
    return 1 if rep.failed else 0


if __name__ == "__main__":
    sys.exit(main())

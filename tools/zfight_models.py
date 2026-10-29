"""Z-fighting between the art models and the rest of the world: for every placed model, find its
faces lying in the same plane as (within `TOL` of) a visible map box face or the terrain, facing
the same way and overlapping. Reports area per model, worst first.

cargo run --release -p shared --bin zfight_check -- --dump /tmp/z.json
python3 tools/zfight_models.py /tmp/z.json
"""

import json
import os
import sys

import numpy as np

ASSETS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")
TOL = 0.001   # planes closer than this count as the same (see zfight_check.rs)


def load_model(rel):
    path = os.path.join(ASSETS, rel)
    j = json.load(open(path))
    b = open(os.path.join(os.path.dirname(path), j["buffers"][0]["uri"]), "rb").read()

    def acc(i):
        a = j["accessors"][i]
        bv = j["bufferViews"][a["bufferView"]]
        n = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
        dt = {5126: np.float32, 5123: np.uint16, 5125: np.uint32}[a["componentType"]]
        arr = np.frombuffer(b, dt, a["count"] * n, bv.get("byteOffset", 0) + a.get("byteOffset", 0))
        return arr.reshape(-1, n) if n > 1 else arr

    tris = []
    for m in j["meshes"]:
        if "LOD" in m.get("name", "") and "LOD0" not in m["name"]:
            continue
        for p in m["primitives"]:
            P = acc(p["attributes"]["POSITION"]).astype(np.float64)
            tris.append(P[acc(p["indices"]).reshape(-1, 3)])
    return np.concatenate(tris) if tris else np.zeros((0, 3, 3))


def quat_mat(q):
    x, y, z, w = q
    return np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                     [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                     [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])


def hidden_by_boxes(boxes, P):
    """Which points lie inside a map box (a face whose front is pressed into a wall can't be seen)."""
    out = np.zeros(len(P), bool)
    if not len(P):
        return out
    lo, hi = P.min(0), P.max(0)
    for pos, rot, size in boxes:
        r = np.linalg.norm(size) / 2
        if (np.array(pos) + r < lo).any() or (np.array(pos) - r > hi).any():
            continue
        R = quat_mat(rot)
        L = (P - np.array(pos)) @ R
        out |= (np.abs(L) < np.array(size) / 2).all(1)
    return out


def main():
    d = json.load(open(sys.argv[1]))
    faces = d["faces"]
    C = np.array([f[0] for f in faces])
    U = np.array([f[1] for f in faces])
    V = np.array([f[2] for f in faces])
    N = np.array([f[6] for f in faces])  # outward
    HU = np.array([f[3] for f in faces])
    HV = np.array([f[4] for f in faces])
    t = d["terrain"]
    n, size = t["n"], t["size"]
    H = np.array(t["h"]).reshape(n, n)
    step = size / (n - 1)

    def terrain_h(x, z):
        fx = np.clip((x + size / 2) / step, 0, n - 1.001)
        fz = np.clip((z + size / 2) / step, 0, n - 1.001)
        i, j = fx.astype(int), fz.astype(int)
        tx, tz = fx - i, fz - j
        a = H[i, j] * (1 - tx) + H[i + 1, j] * tx
        b = H[i, j + 1] * (1 - tx) + H[i + 1, j + 1] * tx
        return a * (1 - tz) + b * tz

    cache, results = {}, {}
    for pl in d["placements"]:
        if pl["model"] not in cache:
            try:
                cache[pl["model"]] = load_model(pl["model"])
            except FileNotFoundError:
                cache[pl["model"]] = np.zeros((0, 3, 3))
        T = cache[pl["model"]]
        if not len(T):
            continue
        R = quat_mat(pl["rot"])
        W = (T * np.array(pl["scale"])) @ R.T + np.array(pl["pos"])
        e1, e2 = W[:, 1] - W[:, 0], W[:, 2] - W[:, 0]
        nrm = np.cross(e1, e2)
        area = np.linalg.norm(nrm, axis=1) / 2
        ok = area > 1e-5
        W, nrm, area = W[ok], nrm[ok] / (2 * area[ok])[:, None], area[ok]
        cen = W.mean(axis=1)
        # Faces whose front (4 cm out) is inside a map box are pressed against it: never seen.
        seen = ~hidden_by_boxes(d.get("boxes", []), cen + nrm * 0.04)
        W, nrm, area, cen = W[seen], nrm[seen], area[seen], cen[seen]
        hit = 0.0
        # Against the terrain: upward faces whose corners all sit within TOL of the ground.
        up = nrm[:, 1] > 0.95
        if up.any():
            dz = np.abs(W[up][:, :, 1] - terrain_h(W[up][:, :, 0], W[up][:, :, 2]))
            hit += area[up][(dz < TOL).all(axis=1)].sum()
        # Against box faces: nearby faces with a parallel normal and the face centre in the plane.
        lo, hi = cen.min(0) - 1, cen.max(0) + 1
        near = np.where(((C > lo - np.maximum(HU, HV)[:, None]) & (C < hi + np.maximum(HU, HV)[:, None])).all(1))[0]
        for k in near:
            nk = N[k] / (np.linalg.norm(N[k]) + 1e-9)
            if nk[1] < -0.9 and C[k][1] < 0.1:
                continue  # undersides resting on the ground are never seen
            par = (nrm @ nk) > 0.999  # same facing: back-to-back faces are hidden, not fighting
            if not par.any():
                continue
            dist = np.abs((cen[par] - C[k]) @ nk)
            rel = cen[par] - C[k]
            inside = (np.abs(rel @ U[k]) < HU[k]) & (np.abs(rel @ V[k]) < HV[k])
            hit += area[par][(dist < TOL) & inside].sum()
        if hit > 0.05:
            key = os.path.basename(pl["model"])
            results.setdefault(key, [0.0, 0, pl["pos"]])
            results[key][0] += hit
            results[key][1] += 1
    # Map blocks against the terrain: upward box faces and cylinder tops lying in the ground.
    ground = {}
    g = np.linspace(-1, 1, 9)
    for k in np.where(N[:, 1] > 0.95)[0]:
        P = C[k] + g[:, None, None] * U[k] * HU[k] + g[None, :, None] * V[k] * HV[k]
        near = np.abs(P[..., 1] - terrain_h(P[..., 0], P[..., 2])) < TOL
        a = near.mean() * 4 * HU[k] * HV[k]
        if a > 0.05:
            e = ground.setdefault(faces[k][5], [0.0, 0, C[k]])
            e[0] += a
            e[1] += 1
    print(f"{'block material':<26} {'m2':>8} {'faces':>10}  e.g. at   (block tops in the ground)")
    for k, (a, c, p) in sorted(ground.items(), key=lambda kv: -kv[1][0]):
        print(f"{k:<26} {a:8.1f} {c:10d}  ({p[0]:.0f}, {p[1]:.1f}, {p[2]:.0f})")
    print()
    print(f"{'model':<26} {'m2':>8} {'placements':>10}  e.g. at")
    for k, (a, c, p) in sorted(results.items(), key=lambda kv: -kv[1][0]):
        print(f"{k:<26} {a:8.1f} {c:10d}  ({p[0]:.0f}, {p[1]:.1f}, {p[2]:.0f})")
    if not results:
        print("no model faces coincide with the terrain or map boxes")


if __name__ == "__main__":
    main()

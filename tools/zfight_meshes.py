"""Z-fighting among the art models themselves: triangles of the placed models (a model against
itself, or against another model placed over it) that lie in the same plane, face the same way
and overlap. Foliage is skipped (its leaf cards overlap by design and are alpha-tested).

python3 tools/zfight_meshes.py build/zfight.json [--model stepwell] [--top 12] [--where]

Reads the placements dumped by zfight_check. Prints overlap area per model pair, worst first, with
a world position to look at.
"""

import json
import os
import sys
from collections import defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from zfight_models import hidden_by_boxes, load_model, quat_mat  # noqa: E402

SKIP = ("tree_", "bush_")
PLANE_TOL = 0.001      # metres: planes closer than this count as the same (see zfight_check.rs)
SAMPLES = np.array([(a, b) for a in np.linspace(0.08, 0.84, 6) for b in np.linspace(0.08, 0.84, 6) if a + b < 0.92])


def inside_solid(W, P, chunk=256):
    """Whether each point is inside the closed surfaces of W: parity of crossings along a skewed
    ray, taking the majority of three rays (models aren't all perfectly closed)."""
    votes = np.zeros(len(P), int)
    e1, e2 = W[:, 1] - W[:, 0], W[:, 2] - W[:, 0]
    for d in (np.array([0.57, 0.61, 0.55]), np.array([-0.62, 0.53, 0.58]), np.array([0.51, -0.6, 0.62])):
        d = d / np.linalg.norm(d)
        pv = np.cross(d, e2)
        det = (e1 * pv).sum(1)
        ok = np.abs(det) > 1e-12
        inv = np.where(ok, 1.0 / np.where(ok, det, 1.0), 0.0)
        for s in range(0, len(P), chunk):
            O = P[s:s + chunk]
            tv = O[:, None, :] - W[None, :, 0]
            u = (tv * pv[None]).sum(2) * inv[None]
            qv = np.cross(tv, e1[None])
            v = (qv @ d) * inv[None]
            t = (qv * e2[None]).sum(2) * inv[None]
            hit = ok[None] & (u >= 0) & (v >= 0) & (u + v <= 1) & (t > 1e-6)
            votes[s:s + chunk] += hit.sum(1) % 2
    return votes >= 2


def cross2(a, b):
    return a[..., 0] * b[..., 1] - a[..., 1] * b[..., 0]


def terrain_sampler(d):
    t = d["terrain"]
    n, size = t["n"], t["size"]
    H = np.array(t["h"]).reshape(n, n)
    step = size / (n - 1)

    def h(x, z):
        fx = np.clip((x + size / 2) / step, 0, n - 1.001)
        fz = np.clip((z + size / 2) / step, 0, n - 1.001)
        i, j = fx.astype(int), fz.astype(int)
        tx, tz = fx - i, fz - j
        return (H[i, j] * (1 - tx) + H[i + 1, j] * tx) * (1 - tz) + (H[i, j + 1] * (1 - tx) + H[i + 1, j + 1] * tx) * tz
    return h


def placed(d, only):
    """Per placement: (name, world triangles that can be seen, all its world triangles)."""
    cache, out = {}, []
    ground = terrain_sampler(d)
    for pl in d["placements"]:
        m = pl["model"]
        if any(s in m for s in SKIP) or (only and only not in m):
            continue
        if m not in cache:
            try:
                cache[m] = load_model(m)
            except FileNotFoundError:
                cache[m] = np.zeros((0, 3, 3))
        T = cache[m]
        if not len(T):
            continue
        W = (T * np.array(pl["scale"])) @ quat_mat(pl["rot"]).T + np.array(pl["pos"])
        n = np.cross(W[:, 1] - W[:, 0], W[:, 2] - W[:, 0])
        area = np.linalg.norm(n, axis=1) / 2
        W, n = W[area > 1e-6], n[area > 1e-6] / (2 * area[area > 1e-6])[:, None]
        # Faces whose front (4 cm out) is inside a map block are pressed against it, and
        # undersides resting on (or in) the ground, and faces wholly buried: never seen.
        full = W
        c = W.mean(1)
        gy = ground(W[..., 0], W[..., 2])
        on_ground = ((n[:, 1] < -0.9) & (c[:, 1] < gy.mean(1) + 0.1)) | (W[..., 1] < gy - 0.02).all(1)
        W = W[~hidden_by_boxes(d.get("boxes", []), c + n * 0.04) & ~on_ground]
        if len(W):
            out.append((os.path.basename(m).replace(".gltf", ""), W, full))
    return out


def clusters(items):
    """Groups of placements whose bounds overlap (only those can fight each other)."""
    lo = np.array([W.reshape(-1, 3).min(0) for _, W, _ in items]) - 0.01
    hi = np.array([W.reshape(-1, 3).max(0) for _, W, _ in items]) + 0.01
    parent = list(range(len(items)))

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i
    for i in range(len(items)):
        for j in np.where(((lo[i] <= hi) & (hi[i] >= lo)).all(1))[0]:
            if j > i:
                parent[find(i)] = find(int(j))
    groups = defaultdict(list)
    for i in range(len(items)):
        groups[find(i)].append(i)
    return list(groups.values())


def main():
    d = json.load(open(sys.argv[1]))
    only = sys.argv[sys.argv.index("--model") + 1] if "--model" in sys.argv else None
    top = int(sys.argv[sys.argv.index("--top") + 1]) if "--top" in sys.argv else 12
    items = placed(d, only)
    names = [it[0] for it in items]
    hits = defaultdict(lambda: [0.0, None])
    detail = defaultdict(float) if "--where" in sys.argv else None
    for group in clusters(items):
        W = np.concatenate([items[i][1] for i in group])
        owner = np.concatenate([np.full(len(items[i][1]), i) for i in group])
        full = np.concatenate([items[i][2] for i in group])
        scan(W, owner, full, hits, detail)
    report(hits, detail, names, top)


def scan(W, owner, full, hits, detail):
    n = np.cross(W[:, 1] - W[:, 0], W[:, 2] - W[:, 0])
    area = np.linalg.norm(n, axis=1) / 2
    n = n / (2 * area)[:, None]
    dist = (W[:, 0] * n).sum(1)
    # Bucket by plane: quantised normal and offset (offsets near a bucket edge also go next door).
    key_n = np.round(n * 50).astype(int)
    q = dist / PLANE_TOL
    buckets = defaultdict(list)
    for i in range(len(W)):
        kn = tuple(key_n[i])
        buckets[kn + (int(np.floor(q[i])),)].append(i)
    found = []
    for key, idx in buckets.items():
        nb = buckets.get(key[:3] + (key[3] + 1,), [])
        group = np.array(idx + nb)
        if len(group) < 2:
            continue
        base = len(idx)
        # 2D frame of the plane.
        nn = n[group[0]]
        u = np.cross(nn, [0, 1, 0] if abs(nn[1]) < 0.9 else [1, 0, 0])
        u /= np.linalg.norm(u)
        v = np.cross(nn, u)
        P = np.stack([W[group] @ u, W[group] @ v], -1)        # (g, 3, 2)
        A, B, Cc = P[:, 0], P[:, 1], P[:, 2]
        S = A[:, None] + SAMPLES[None, :, :1] * (B - A)[:, None] + SAMPLES[None, :, 1:] * (Cc - A)[:, None]   # (g, s, 2)
        lo, hi = P.min(1), P.max(1)
        for a in range(base):
            ia = group[a]
            cand = np.where((hi[:, 0] > lo[a, 0]) & (lo[:, 0] < hi[a, 0]) & (hi[:, 1] > lo[a, 1]) & (lo[:, 1] < hi[a, 1]))[0]
            cand = cand[(cand != a) & (group[cand] > ia) & ((n[group[cand]] @ n[ia]) > 0.999)]
            cand = cand[np.abs(dist[group[cand]] - dist[ia]) < PLANE_TOL]
            if not len(cand):
                continue
            # Fraction of a's interior samples strictly inside each candidate.
            s = S[a]
            for c in cand:
                p0, p1, p2 = P[c]
                d0, d1, d2 = cross2(p1 - p0, s - p0), cross2(p2 - p1, s - p1), cross2(p0 - p2, s - p2)
                inside = ((d0 > 1e-7) & (d1 > 1e-7) & (d2 > 1e-7)) | ((d0 < -1e-7) & (d1 < -1e-7) & (d2 < -1e-7))
                f = inside.mean()
                if f > 0:
                    found.append((ia, group[c], f * min(area[ia], area[group[c]])))
    if not found:
        return
    # Drop overlaps whose front is enclosed by other geometry (a step's side behind its rail):
    # the faces are pressed against solid stone and never seen.
    ia_all = np.array([h[0] for h in found])
    front = W[ia_all].mean(1) + n[ia_all] * 0.01
    enclosed = inside_solid(full, front)
    for (ia, ic, a), e in zip(found, enclosed):
        if e:
            continue
        pair = tuple(sorted((owner[ia], owner[ic])))
        h = hits[pair]
        h[0] += a
        h[1] = W[ia].mean(0)
        if detail is not None:
            c0 = W[ia].mean(0)
            k = (round(c0[0]), round(c0[1], 1), round(c0[2]), tuple(np.round(n[ia], 1)))
            detail[k] += a


def report(hits, detail, names, top):
    rows = sorted(hits.items(), key=lambda kv: -kv[1][0])
    agg = defaultdict(lambda: [0.0, 0, None])
    for (oa, ob), (a, at) in rows:
        k = names[oa] if oa == ob else " + ".join(sorted((names[oa], names[ob])))
        agg[k][0] += a
        agg[k][1] += 1
        if agg[k][2] is None:
            agg[k][2] = at
    print(f"{'model (or pair)':<40} {'m2':>8} {'cases':>6}  e.g. at")
    for k, (a, c, at) in sorted(agg.items(), key=lambda kv: -kv[1][0])[:top]:
        if a >= 0.01:
            print(f"{k:<40} {a:8.2f} {c:6d}  ({at[0]:.1f}, {at[1]:.2f}, {at[2]:.1f})")
    if detail:
        print("\nworst spots (x, y, z, normal):")
        for k, a in sorted(detail.items(), key=lambda kv: -kv[1])[:top]:
            print(f"  {k}  {a:.2f} m2")
    if not agg:
        print("no overlapping coplanar model faces")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Fits every long gun's stock to the length the rifle clips hold (no Blender needed).

The captured hold beds a 21 cm stock in the shoulder, so a longer one comes out through the back of
the shoulder, in full view of the third-person camera. In both LODs of each gun this maps the
vertices with z above FROM so the butt lands at BUTT (BUTT in crates/game/src/arms.rs), turns their
normals and tangents with them, and updates the accessors' min and max. A gun already at BUTT is
left as it is, so it can be run again.

    python3 tools/blender/weapons/fit_stocks.py
"""

import json
import math
import struct
from pathlib import Path

BUTT = 0.21
FROM = 0.06
GUNS = ["vajra9", "trishul_carbine", "saintalis", "thaana_slr", "thri_not_thri", "toradar"]
WEAPONS = Path(__file__).resolve().parents[3] / "assets/models/weapons"


def slot(gltf, index):
    """Byte offset, stride and count of an accessor in the binary chunk."""
    acc = gltf["accessors"][index]
    view = gltf["bufferViews"][acc["bufferView"]]
    return view.get("byteOffset", 0) + acc.get("byteOffset", 0), view.get("byteStride"), acc["count"]


def fit(path):
    data = path.read_bytes()
    json_len = struct.unpack_from("<I", data, 12)[0]
    gltf = json.loads(data[20 : 20 + json_len])
    blob = bytearray(data[20 + json_len + 8 :])
    changed = False
    for mesh in gltf["meshes"]:
        for prim in mesh["primitives"]:
            attrs = prim["attributes"]
            position = gltf["accessors"][attrs["POSITION"]]
            top = position["max"][2]
            if top <= BUTT + 1e-4:
                continue
            k = (BUTT - FROM) / (top - FROM)
            changed = True
            at, stride, count = slot(gltf, attrs["POSITION"])
            stride = stride or 12
            normal = slot(gltf, attrs["NORMAL"])
            tangent = slot(gltf, attrs["TANGENT"])
            lowest = math.inf
            highest = -math.inf
            for i in range(count):
                offset = at + i * stride
                x, y, z = struct.unpack_from("<3f", blob, offset)
                if z > FROM:
                    struct.pack_into("<f", blob, offset + 8, FROM + (z - FROM) * k)
                    # A surface squeezed along z: its normal gains z, its tangent loses it.
                    n_at = normal[0] + i * (normal[1] or 12)
                    nx, ny, nz = struct.unpack_from("<3f", blob, n_at)
                    nz /= k
                    length = math.sqrt(nx * nx + ny * ny + nz * nz) or 1.0
                    struct.pack_into("<3f", blob, n_at, nx / length, ny / length, nz / length)
                    t_at = tangent[0] + i * (tangent[1] or 16)
                    tx, ty, tz = struct.unpack_from("<3f", blob, t_at)
                    tz *= k
                    length = math.sqrt(tx * tx + ty * ty + tz * tz) or 1.0
                    struct.pack_into("<3f", blob, t_at, tx / length, ty / length, tz / length)
                z = struct.unpack_from("<f", blob, offset + 8)[0]
                lowest = min(lowest, z)
                highest = max(highest, z)
            position["min"][2] = lowest
            position["max"][2] = highest
            print(f"[fit_stocks] {path.stem} {mesh.get('name')}: butt {top * 100:.1f} cm to {highest * 100:.1f} cm")
    if not changed:
        print(f"[fit_stocks] {path.stem}: already fits")
        return
    text = json.dumps(gltf, separators=(",", ":")).encode()
    text += b" " * (-len(text) % 4)
    body = struct.pack("<I4s", len(text), b"JSON") + text + struct.pack("<I4s", len(blob), b"BIN\0") + bytes(blob)
    path.write_bytes(struct.pack("<4sII", b"glTF", 2, 12 + len(body)) + body)


for name in GUNS:
    fit(WEAPONS / f"{name}.glb")

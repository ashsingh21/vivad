"""The models' own textures capped in size, in place (GLB files with their images inside): the
characters' and props' at 1024 px, the weapons' at 512 (held in a hand, seen from behind). On
Connaught Place these were 550 MB of GPU memory at full size; the GPU keeps what a model brings,
so they have to be made smaller here, not when the game loads them. Run after every export
(`just assets` does); a file already within its caps is left as it is.

python3 tools/glb_textures.py [file.glb ...]   (all of assets/models/{characters,weapons,props} by default)
"""

import glob
import io
import json
import os
import struct
import sys

from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
CAPS = {"weapons": 512, "characters": 1024, "props": 1024}


# VRoid imports (tools/blender/characters/vroid.py: anime_<name>.glb) bring 16-17 textures each,
# 50 MB of GPU memory at 1024 px: at 512 a face filling the screen looks the same. (Seed-san,
# anime.glb, has four in an atlas and keeps 1024.)
FILE_CAPS = {"characters/anime_": 512}


def cap_for(path):
    path = path.replace(os.sep, "/")
    return next((c for k, c in FILE_CAPS.items() if f"/models/{k}" in path), None) or next((c for k, c in CAPS.items() if f"/models/{k}/" in path), None)


def shrink(path, cap):
    raw = open(path, "rb").read()
    if raw[:4] != b"glTF":
        return 0
    jlen = struct.unpack("<I", raw[12:16])[0]
    doc = json.loads(raw[20:20 + jlen])
    bin_at = 20 + jlen + 8
    blob = raw[bin_at:bin_at + struct.unpack("<I", raw[20 + jlen:24 + jlen])[0]]
    views = doc["bufferViews"]
    new = {}
    for im in doc.get("images", []):
        if "bufferView" not in im:
            continue
        v = views[im["bufferView"]]
        data = blob[v.get("byteOffset", 0):v.get("byteOffset", 0) + v["byteLength"]]
        img = Image.open(io.BytesIO(data))
        if max(img.size) <= cap:
            continue
        k = cap / max(img.size)
        img = img.resize((max(1, round(img.width * k)), max(1, round(img.height * k))), Image.LANCZOS)
        out = io.BytesIO()
        img.save(out, "PNG", optimize=True)
        new[im["bufferView"]] = out.getvalue()
        im["mimeType"] = "image/png"
    if not new:
        return 0
    # The binary chunk laid out again, each view where it now falls (4-byte aligned).
    parts, at = [], 0
    for i, v in enumerate(views):
        data = new.get(i) or blob[v.get("byteOffset", 0):v.get("byteOffset", 0) + v["byteLength"]]
        pad = (-at) % 4
        parts.append(b"\0" * pad)
        at += pad
        v["byteOffset"], v["byteLength"] = at, len(data)
        parts.append(data)
        at += len(data)
    body = b"".join(parts) + b"\0" * ((-at) % 4)
    doc["buffers"][0]["byteLength"] = len(body)
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * ((-len(js)) % 4)
    out = b"".join([b"glTF", struct.pack("<II", 2, 12 + 8 + len(js) + 8 + len(body)), struct.pack("<I", len(js)), b"JSON", js, struct.pack("<I", len(body)), b"BIN\0", body])
    open(path, "wb").write(out)
    return len(new)


if __name__ == "__main__":
    files = sys.argv[1:] or sorted(glob.glob(os.path.join(ROOT, "assets", "models", "*", "*.glb")))
    for f in files:
        cap = cap_for(os.path.abspath(f))
        if cap and (n := shrink(f, cap)):
            print(f"[glb_textures] {os.path.relpath(f, ROOT)}: {n} images to {cap} px")

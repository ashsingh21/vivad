"""Tidy the shared model textures: point every `.gltf` at one file per distinct content (names
end in a content hash, so identical maps exported under different names share one), then delete
textures nothing references any more (left behind when a texture changes, and its name with it).

python3 tools/prune_textures.py
"""

import json
import os

MODELS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models")


def gltfs():
    for root, _, files in os.walk(MODELS):
        for fn in files:
            if fn.endswith(".gltf"):
                yield os.path.join(root, fn)


def main():
    tex_dir = os.path.join(MODELS, "textures")
    # The first name (alphabetically) for each content hash is the one kept.
    canonical = {}
    for fn in sorted(os.listdir(tex_dir)):
        digest = os.path.splitext(fn)[0].rsplit("_", 1)[-1]
        canonical.setdefault(digest, fn)
    used = set()
    for path in gltfs():
        with open(path) as f:
            doc = json.load(f)
        changed = False
        for im in doc.get("images", []):
            fn = os.path.basename(im.get("uri", ""))
            keep = canonical.get(os.path.splitext(fn)[0].rsplit("_", 1)[-1], fn)
            if keep != fn:
                im["uri"] = im["uri"][: -len(fn)] + keep
                changed = True
            used.add(keep)
        if changed:
            with open(path, "w") as f:
                json.dump(doc, f, separators=(",", ":"))
    removed = sorted(fn for fn in os.listdir(tex_dir) if fn not in used)
    for fn in removed:
        os.remove(os.path.join(tex_dir, fn))
    print(f"[prune] {len(used)} textures in use, removed {len(removed)}: {removed}")


if __name__ == "__main__":
    main()

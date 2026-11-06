"""The manifest tools/texpack packs: every PNG of the city's texture sets (assets/textures/panga,
india), as GPU-compressed, mip-mapped KTX2 beside it (crate::tex picks those on the desktop).
Kinds by name: *_normal a normal map (BC5), *_mr / *_orm and the ground detail data (BC7, linear),
the rest colour (BC7, sRGB).

python3 tools/textures/pack_list.py > build/pack.txt && texpack pack build/pack.txt   (just textures-pack)
"""

import glob
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

for png in sorted(glob.glob(os.path.join(ROOT, "assets", "textures", "*", "*.png"))):
    name = os.path.basename(png)[:-4]
    kind = "normal" if name.endswith("_normal") else "data" if name.endswith(("_mr", "_orm")) or name == "ground_detail" else "color"
    print(f"{kind} {png} {png[:-4]}.ktx2")

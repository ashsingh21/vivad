"""Join the frames of a VIVAD_CLIP capture into one animated WebP the owner can open in a browser.

    python3 tools/clip.py previews/cp_clip 8 [out.webp]

The frames are `frame_00000.png` upwards, as `VIVAD_CLIP=<dir>,<from>,<to>,<fps>` writes them; the
second argument is that frame rate. The animation plays at it and loops.
"""

import glob
import os
import sys

from PIL import Image


def main() -> None:
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    folder, fps = sys.argv[1], float(sys.argv[2])
    out = sys.argv[3] if len(sys.argv) > 3 else os.path.join(folder, "clip.webp")
    files = sorted(glob.glob(os.path.join(folder, "frame_*.png")))
    if not files:
        sys.exit(f"no frames in {folder}")
    frames = [Image.open(f).convert("RGB") for f in files]
    frames[0].save(out, save_all=True, append_images=frames[1:], duration=int(1000 / fps), loop=0, quality=80, method=4)
    size = os.path.getsize(out) / 1e6
    print(f"{out}: {len(frames)} frames, {len(frames) / fps:.0f} s at {fps:g} fps, {frames[0].size[0]}x{frames[0].size[1]}, {size:.1f} MB")


if __name__ == "__main__":
    main()

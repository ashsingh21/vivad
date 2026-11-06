"""The animation sets side by side (build/animcheck/<variant>.json, written by
tools/blender/characters/anim_lint.py): per clip a line a set, and the events worth a look, as text.
A set that does what the others don't (its torso twists in a jump where theirs stays square) is
what's flagged, as well as anything wrong on its own: feet sliding while planted, a bone popping
between frames, a loop that doesn't close, a swayback.

  python3 tools/anim_report.py [clips...] [--sets anime,male,female] [--all]
"""

import json
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
args = sys.argv[1:]
sets = args[args.index("--sets") + 1].split(",") if "--sets" in args else ["anime", "male", "female"]
show_all = "--all" in args
names = [a for i, a in enumerate(args) if not a.startswith("--") and (i == 0 or args[i - 1] != "--sets")]

data = {}
for s in sets:
    path = os.path.join(ROOT, "build", "animcheck", f"{s}.json")
    if os.path.exists(path):
        data[s] = json.load(open(path))
sets = [s for s in sets if s in data]

# Clips a body should stay square in: no reason for the shoulders to turn against the hips.
SQUARE = ("jump", "fall", "idle", "freefall", "glide", "death", "consume", "crouch_idle", "aim")
# Limits (degrees, cm) past which a thing is called out on its own.
TWIST_SQUARE, TURN_SQUARE, SLIDE, POP, SEAM, ARCH = 12.0, 10.0, 3.0, 14.0, 4.0, 14.0


def peak(v):
    return max(v, key=abs) if v else 0.0


def at(v, x):
    return v.index(x) if x in v else 0


def spread(v):
    return max(v) - min(v) if v else 0.0


def median(v):
    v = sorted(v)
    return v[len(v) // 2] if v else 0.0


clips = sorted(set().union(*[set(d) for d in data.values()]))
flagged = 0
for clip in clips:
    if names and not any(n in clip for n in names):
        continue
    rows, events = [], []
    have = {s: data[s][clip] for s in sets if clip in data[s]}
    square = any(k in clip for k in SQUARE)
    for s, c in have.items():
        tw, tu, hd, ar = peak(c["twist"]), peak(c["turn"]), peak(c["head"]), max(c.get("arch", [0]))
        slide = max(f["slide"] for f in c["feet"]) * 100
        pop = max(c["pop"])
        pop_f = c["pop"].index(pop)
        pop_med = median(c["pop"][1:])
        rows.append(f"  {s:10} frames {c['frames']:3}  twist {tw:+6.1f} (f{at(c['twist'], tw):<3}) turn {tu:+6.1f}  head {hd:+6.1f}  "
                    f"lean {min(c['lean']):+5.1f}..{max(c['lean']):+5.1f}  arch {ar:+5.1f}  slide {slide:4.1f}  pop {pop:5.1f} (f{pop_f} {c['pop_bone'][pop_f]})"
                    + (f"  seam {c['seam']:4.1f}" if c["loop"] else ""))
        others = [o for o in have if o != s]
        other_tw = [abs(peak(have[o]["twist"])) for o in others]
        if square and abs(tw) > TWIST_SQUARE and (not other_tw or abs(tw) > 2 * max(other_tw)):
            events.append(f"{s}: shoulders turn {tw:+.0f} deg against the hips at frame {at(c['twist'], tw)} of a clip that should stay square"
                          + (f" (the others peak at {max(other_tw):.0f})" if other_tw else ""))
        if square and abs(tu) > TURN_SQUARE:
            events.append(f"{s}: the whole body turns {tu:+.0f} deg by frame {at(c['turn'], tu)}")
        if slide > SLIDE:
            events.append(f"{s}: a planted foot slides {slide:.1f} cm")
        if pop > POP and pop > 4 * max(pop_med, 0.5):
            events.append(f"{s}: {c['pop_bone'][pop_f]} jumps {pop:.0f} deg in one frame at frame {pop_f} (median {pop_med:.1f})")
        if c["loop"] and c["seam"] > SEAM:
            events.append(f"{s}: the loop doesn't close ({c['seam']:.0f} deg between its last frame and its first)")
        other_ar = [max(have[o].get("arch", [0])) for o in others]
        if ar > ARCH and (not other_ar or ar > max(other_ar) + 6):
            events.append(f"{s}: the back arches {ar:.0f} deg (a swayback)" + (f"; the others {max(other_ar):.0f}" if other_ar else ""))
    if events or show_all or names:
        print(clip)
        print("\n".join(rows))
        for e in events:
            print(f"  ! {e}")
        flagged += len(events)
print(f"\n{flagged} things to look at in {len(clips)} clips ({', '.join(sets)})")

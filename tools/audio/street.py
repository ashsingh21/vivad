"""Delhi's street and the fights' sounds, cut from recordings: the CC0 Freesound files and Kenney's
CC0 packs that research/fights-sounds/ collected (not in git; only what this writes is). Every
source was licence-checked one by one (research/fights-sounds/data/freesound_licences.csv).

python3 tools/audio/street.py     build all into assets/audio/, and the list of what plays where
                                  into previews/fights-sounds/sound_list.md

Nobody has listened to these yet. Cuts are chosen by each file's notes and its level: `loud` takes
the file's k-th loudest moment (a horn, a bell), `voices` the k-th moment richest in the voice band
over the rumble under it (a hawker), `calm` the steadiest stretch (a bed). The list says so.

Mono, 44.1 kHz, peak -1 dBFS for one-shots; beds keep their stereo, are made seamless by
cross-fading their tail into their head, and sit at BED_RMS. Written as Ogg Vorbis by soundfile
(libsndfile), since oggenc is not on this Mac.
"""

import csv
import os

import numpy as np
import soundfile as sf
from scipy.signal import butter, resample_poly, sosfiltfilt

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
RESEARCH = os.path.join(ROOT, "research", "fights-sounds")
OUT = os.path.join(ROOT, "assets", "audio")
LIST = os.path.join(ROOT, "previews", "fights-sounds", "sound_list.md")
SR = 44100
BED_RMS = -24.0
ROWS = {r["id"]: r for r in csv.DictReader(open(os.path.join(RESEARCH, "data", "freesound_licences.csv")))}
KENNEY = os.path.join(RESEARCH, "sounds", "kenney", "impact-sounds", "Audio")
made = []


def load(sid):
    """A checked Freesound file, as float frames at SR (stereo as it came)."""
    r = ROWS[sid]
    x, sr = sf.read(os.path.join(RESEARCH, "sounds", "freesound", r["file"]), always_2d=True)
    return x if sr == SR else resample_poly(x, SR, sr, axis=0)


def kenney(name):
    x, sr = sf.read(os.path.join(KENNEY, name + ".ogg"), always_2d=True)
    x = x.mean(axis=1)
    return x if sr == SR else resample_poly(x, SR, sr)


def average(x, n):
    """The mean of `x` over `n` samples round each point."""
    c = np.concatenate([[0.0], np.cumsum(x)])
    lo = np.clip(np.arange(len(x)) - n // 2, 0, len(x))
    hi = np.clip(lo + n, 0, len(x))
    return (c[hi] - c[lo]) / n


def envelope(x, win=0.05):
    m = x.mean(axis=1) if x.ndim > 1 else x
    return np.sqrt(average(m ** 2, int(win * SR)) + 1e-12)


def top_moments(score, sep, k):
    """The k-th highest point of `score`, keeping `sep` seconds between the picks."""
    s = score.copy()
    gap = int(sep * SR)
    at = 0
    for _ in range(k + 1):
        at = int(np.argmax(s))
        s[max(0, at - gap):at + gap] = 0.0
    return at


def onset(env, at):
    """Back from a peak to where the sound began."""
    floor = env[at] * 0.25
    i = at
    while i > max(0, at - int(0.6 * SR)) and env[i] > floor:
        i -= 256
    return max(0, i - int(0.02 * SR))


def cut_loud(x, k, dur):
    env = envelope(x)
    start = onset(env, top_moments(env, max(dur, 1.0) + 0.5, k))
    return x[start:start + int(dur * SR)], start / SR


def cut_voices(x, k, dur):
    m = x.mean(axis=1) if x.ndim > 1 else x
    band = sosfiltfilt(butter(4, [350, 3000], "bandpass", fs=SR, output="sos"), m)
    low = sosfiltfilt(butter(4, 250, "lowpass", fs=SR, output="sos"), m)
    be = average(band ** 2, int(dur * SR))
    le = average(low ** 2, int(dur * SR))
    score = be / (le + 1e-9) * np.sqrt(be)
    score[: int(dur * SR)] = 0.0
    score[-int(dur * SR):] = 0.0
    at = top_moments(score, dur + 1.0, k)
    start = at - int(dur * SR / 2)
    return x[start:start + int(dur * SR)], start / SR


def calm_start(x, dur):
    """Where the steadiest `dur` seconds begin: no one horn repeats loud each time round the loop."""
    env = envelope(x, 0.25)
    n, hop = int(dur * SR), SR // 2
    best, at = 1e9, 0
    for s in range(0, max(1, len(env) - n), hop):
        w = env[s:s + n]
        score = np.percentile(w, 99.5) / (np.median(w) + 1e-9)
        if score < best:
            best, at = score, s
    return at


def shape(x, fade_in=0.008, fade_out=0.06):
    x = x.mean(axis=1) if x.ndim > 1 else x
    x = x - x.mean()
    a, b = int(fade_in * SR), int(fade_out * SR)
    x[:a] *= np.linspace(0.0, 1.0, a)
    x[-b:] *= np.linspace(1.0, 0.0, b) ** 2
    return x


def write(name, x, quality=0.5):
    path = os.path.join(OUT, name + ".ogg")
    sf.write(path, np.clip(x, -1.0, 1.0), SR, format="OGG", subtype="VORBIS", compression_level=1.0 - quality)
    return os.path.getsize(path)


def note(name, source, how, where, x, size):
    secs = len(x) / SR
    made.append((name, source, how, where, secs, size))


def credit(sid):
    r = ROWS[sid]
    return f"[{r['title'][:48]}]({r['page']}) by {r['author']}, {r['licence']}"


def one_shot(name, sid, where, *, pick="loud", k=0, dur=1.5, fade=0.08, pitch=1.0):
    x = load(sid)
    if pick == "loud":
        clip, t = cut_loud(x, k, dur)
        how = f"loudest moment {k + 1}, at {t:.1f} s, {dur} s"
    elif pick == "voices":
        clip, t = cut_voices(x, k, dur)
        how = f"voice-richest moment {k + 1}, at {t:.1f} s, {dur} s"
    else:
        t = float(pick)
        clip = x[int(t * SR):int((t + dur) * SR)]
        how = f"from {t:.1f} s, {dur} s"
    y = shape(clip.copy(), fade_out=fade)
    if pitch != 1.0:
        y = resample_poly(y, 100, int(round(100 * pitch)))
    y *= 10 ** (-1 / 20) / max(1e-6, np.abs(y).max())
    note(name, credit(sid), how, where, y, write(name, y))


def bed(name, sid, where, dur=26.0, xfade=3.0, gain_db=0.0):
    x = load(sid)
    if x.shape[1] == 1:
        x = np.repeat(x, 2, axis=1)
    total = dur + xfade
    start = calm_start(x, total)
    clip = x[start:start + int(total * SR)].copy()
    n = int(xfade * SR)
    # Equal power, so the seam neither dips nor swells.
    t = np.linspace(0.0, 1.0, n)[:, None]
    head = clip[:n] * np.sin(t * np.pi / 2) + clip[-n:] * np.cos(t * np.pi / 2)
    y = np.concatenate([head, clip[n:-n]])
    y *= 10 ** ((BED_RMS + gain_db) / 20) / np.sqrt(np.mean(y ** 2))
    # A lone bang in a far-off bed would come round again every loop: tuck the peaks in.
    knee = 10 ** (-9 / 20)
    y = np.where(np.abs(y) > knee, np.sign(y) * (knee + (1 - knee) * np.tanh((np.abs(y) - knee) / (1 - knee))), y)
    size = write(name, y, quality=0.4)
    seam = 20 * np.log10(np.abs(y[0] - y[-1]).max() + 1e-6)
    note(name, credit(sid), f"steadiest {dur:.0f} s, from {start / SR:.1f} s, looped (seam {seam:.0f} dB)", where, y, size)


def mix(name, parts, where, how, pitch=1.0):
    """Layers Kenney's hits: [(file, gain), ...]."""
    n = max(len(p) for p in (kenney(f) for f, _ in parts))
    y = np.zeros(n)
    for f, g in parts:
        p = kenney(f)
        y[:len(p)] += p * g
    y = shape(y, fade_in=0.001, fade_out=0.03)
    if pitch != 1.0:
        y = resample_poly(y, 100, int(round(100 * pitch)))
    y *= 10 ** (-1 / 20) / np.abs(y).max()
    src = " + ".join(f for f, _ in parts)
    note(name, f"Kenney Impact Sounds ({src}), CC0", how, where, y, write(name, y))


def main():
    os.makedirs(os.path.dirname(LIST), exist_ok=True)

    # ---- beds, by where the player stands
    bed("amb_traffic", "336468", "Bed: on the street, day and evening. Louder on the road, softer indoors, on roofs and in parks")
    bed("amb_market", "263636", "Bed: voices and hawkers, by day, under the colonnades and at the stalls")
    bed("amb_far", "434854", "Bed: far-off city, in parks, on roofs, in the air, and all night")

    # ---- traffic and horns, round the player
    for k in range(4):
        one_shot(f"horn_{k}", "451523", "Street, 20 to 60 m away: horns, most often at rush hours", k=k, dur=1.6, fade=0.15)
    for k in range(3):
        one_shot(f"horn_{k + 4}", "336468", "Street, 20 to 60 m away: horns, most often at rush hours", k=k, dur=1.6, fade=0.15)
    one_shot("horn_bus", "508540", "Street, 30 to 70 m away: the funny melodic bus horn, now and then", k=0, dur=3.2, fade=0.3)
    one_shot("auto_pass", "353664", "Street, 15 to 40 m away: an auto going by", k=0, dur=5.0, fade=0.8)
    one_shot("bike_bell", "188032", "Street, 10 to 30 m away: a cycle bell", pick="0.0", dur=1.2, fade=0.1)
    one_shot("whistle", "846254", "Rush hours, a traffic whistle; late at night, the chowkidar's", k=0, dur=1.3, fade=0.1)

    # ---- people
    for k in range(4):
        one_shot(f"hawker_{k}", "263636", "A hawker's call from the nearest stall or cart (thela, chai) in the map's street kit, by day", pick="voices", k=k, dur=3.5, fade=0.5)
    one_shot("crowd_argue", "537989", "Far off, 25 to 50 m: an argument somewhere in the crowd, now and then by day", k=0, dur=6.0, fade=1.0)
    one_shot("crowd_yell", "834338", "Near a melee fight: people yelling round it, at most every 12 s", k=0, dur=5.0, fade=1.0)
    one_shot("crowd_gasp", "264376", "Near a kill: the street gasps", k=0, dur=2.5, fade=0.5)

    # ---- the day's small sounds
    one_shot("shutter", "407313", "Morning and late evening: a shop's shutter, 20 to 40 m away", k=0, dur=3.5, fade=0.6)
    one_shot("cooker", "536520", "Morning to lunch: a pressure cooker's whistle from a window, sound only", k=0, dur=3.0, fade=0.5)
    one_shot("chai_stir", "433539", "At a chai stall by day, a spoon in a glass, close by", pick="1.0", dur=2.5, fade=0.3)
    one_shot("crow", "361470", "Day: a crow overhead", pick="0.0", dur=0.4, fade=0.05)
    one_shot("crows", "437345", "Day: crows in the trees", k=0, dur=3.0, fade=0.5)
    one_shot("koel", "460466", "Morning: a koel", k=0, dur=3.0, fade=0.5)
    one_shot("myna", "338173", "Morning and dusk: mynas together", pick="0.5", dur=4.0, fade=0.6)
    one_shot("pigeons_up", "528250", "Day: pigeons take off", pick="0.0", dur=3.0, fade=0.5)
    one_shot("dog", "393829", "Night: a street dog, 30 to 60 m away", k=0, dur=2.5, fade=0.4)
    one_shot("dhol", "172446", "Evening: a wedding dhol, far off", k=0, dur=10.0, fade=2.0)

    # ---- swings
    one_shot("swing_light_0", "389590", "Swing: tawa, dhal, talwar", pick="0.0", dur=0.3, fade=0.04)
    one_shot("swing_light_1", "719637", "Swing: tawa, dhal, talwar (the bamboo whoosh also for the hockey stick)", pick="0.0", dur=0.5, fade=0.08)
    one_shot("swing_stick", "352719", "Swing: lathi, bat, hockey stick", pick="0.0", dur=0.6, fade=0.08)
    one_shot("swing_heavy", "794818", "Swing: gada", k=0, dur=0.9, fade=0.2)

    # ---- blows
    mix("hit_bat", [("impactWood_heavy_000", 1.0), ("impactPunch_heavy_000", 0.7)], "Blow: cricket bat", "wood and punch layered")
    one_shot("hit_stick", "319217", "Blow: lathi, hockey stick", k=0, dur=0.35, fade=0.1)
    one_shot("hit_pan", "529567", "Blow: tawa, the joke sound", pick="0.0", dur=0.7, fade=0.15)
    mix("hit_dhal", [("impactPlate_heavy_000", 1.0), ("impactPunch_medium_000", 0.8)], "Blow: dhal, a boom", "plate and punch, pitched down", pitch=0.75)
    one_shot("hit_blade", "441666", "Blow: talwar", k=0, dur=0.6, fade=0.15)
    mix("hit_gada", [("impactPunch_heavy_001", 1.0), ("impactSoft_heavy_000", 0.9)], "Blow: gada, very low", "punch and soft thud, pitched down", pitch=0.72)

    # ---- guards and breaks
    mix("guard_wood", [("impactWood_medium_002", 1.0)], "Guard: wood takes the blow (bat, lathi, hockey stick, gada)", "one Kenney file")
    one_shot("guard_metal", "471095", "Guard: blade or dhal takes the blow", k=0, dur=0.7, fade=0.2)
    one_shot("guard_pan", "401706", "Guard: tawa takes the blow or a bullet", pick="0.0", dur=0.4, fade=0.1)
    mix("break_wood", [("impactPlank_medium_000", 1.0), ("impactWood_light_001", 0.7)], "Break: gada", "plank and wood layered")
    mix("break_metal", [("impactMetal_heavy_000", 1.0), ("impactGlass_light_000", 0.5)], "Break: talwar, dhal", "metal and glass layered")

    with open(LIST, "w") as f:
        f.write("# Which sound plays where\n\n")
        f.write("Nobody has heard these yet. Each was chosen by its notes and its level, not by ear. All sources are CC0, or ours.\n")
        f.write("Left out on purpose: temple crowd 423510, street market 263635 (a pipe plays in it), Metro recordings 620300 and 547722 (real announcements), Burbank night 442637, Kerala streets 585569 and 585570 (clear talk), all CC-BY files.\n")
        f.write("Not made: hawker voice loops on a cart (no recorded lines yet) and the Metro chime (cp has no station object). Hawkers here are short cuts of a real New Delhi recording, played from the map's stalls.\n\n")
        f.write("| Game name | Where it plays | Source | Cut | Seconds | KB |\n|---|---|---|---|---|---|\n")
        for name, source, how, where, secs, size in made:
            f.write(f"| `{name}` | {where} | {source} | {how} | {secs:.1f} | {size // 1024} |\n")
    print(f"{len(made)} sounds, {sum(m[5] for m in made) // 1024} KB")


if __name__ == "__main__":
    main()

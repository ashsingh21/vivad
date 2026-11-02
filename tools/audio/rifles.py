"""The rifle sounds, from free recordings (all CC0, see docs/references.md): a near shot for each gun
and a far one for each family. Trims one shot out of each take, joins the bolt onto the Thri-Not-Thri's
shot, dulls the far takes, and writes mono OGG to assets/audio. Blender's audio library does the
reading and the writing, since oggenc may not be installed:

    blender -b --factory-startup --disable-autoexec -P tools/audio/rifles.py
"""

import os

import aud
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(ROOT, "assets", "audio")
SFX = os.path.join(ROOT, "research", "sniper-guns", "sounds")
LIB = os.path.join(SFX, "oga_free_firearm_library", "extracted", "Prepared SFX Library")
FREE = os.path.join(SFX, "freesound_cc0_previews")
RATE = 44100


def load(path):
    """Mono float samples at RATE."""
    a = aud.Sound(path).resample(RATE, False).data()
    return a.mean(axis=1)


def onsets(x):
    """Sample indices where a loud event starts, at least half a second apart."""
    env = np.abs(x)
    peak = env.max()
    hits, last = [], -RATE
    for i in np.flatnonzero(env > peak * 0.35):
        if i - last > RATE * 0.5:
            hits.append(int(i))
        last = i if i - last > RATE * 0.5 else last
    return hits


def cut(x, at, seconds, fade=0.12):
    """`seconds` from just before sample `at`, with a fade out."""
    a = max(0, at - int(0.004 * RATE))
    y = x[a:a + int(seconds * RATE)].copy()
    n = min(int(fade * RATE), len(y))
    y[-n:] *= np.linspace(1.0, 0.0, n) ** 2
    return y


def low_pass(x, hz):
    """Air and walls take the top off a far shot."""
    f = np.fft.rfft(x)
    freq = np.fft.rfftfreq(len(x), 1 / RATE)
    f *= 1.0 / (1.0 + (freq / hz) ** 4)
    return np.fft.irfft(f, len(x))


def write(name, x, peak=0.89, kbps=96):
    x = x / max(np.abs(x).max(), 1e-6) * peak
    data = np.asarray(x, dtype=np.float32).reshape(-1, 1)
    path = os.path.join(OUT, f"{name}.ogg")
    aud.Sound.buffer(data, RATE).write(path, rate=RATE, channels=aud.CHANNELS_MONO, format=aud.FORMAT_S16, container=aud.CONTAINER_OGG,
                                       codec=aud.CODEC_VORBIS, bitrate=kbps * 1000)
    print(f"[{name}] {len(x) / RATE:.2f} s, {os.path.getsize(path) // 1024} KB")


def main():
    # Thri-Not-Thri: the shot, then the bolt worked as the gun's 1.4 s between shots allows.
    shot = load(os.path.join(FREE, "shot_enfield_superphat.mp3"))
    shot = cut(shot, onsets(shot)[0], 1.5)
    bolt = load(os.path.join(FREE, "bolt_mosin_fast_rammbostein.mp3"))
    print("bolt onsets", [round(i / RATE, 2) for i in onsets(bolt)][:8])
    bolt = cut(bolt, onsets(bolt)[0], 0.9, fade=0.08)
    bolt *= 0.32 * np.abs(shot).max() / max(np.abs(bolt).max(), 1e-6)
    mix = np.zeros(int(1.95 * RATE))
    mix[:len(shot)] += shot
    start = int(0.75 * RATE)
    n = min(len(bolt), len(mix) - start)
    mix[start:start + n] += bolt[:n]
    write("gun_thri_not_thri", mix)
    # The heavy rifle far off: the mid-distance Mosin, its long tail, dulled.
    far = load(os.path.join(LIB, "Mosin Nagant", "M_26P.wav"))
    print("mosin far onsets", [round(i / RATE, 2) for i in onsets(far)][:6])
    write("gun_rifle_far", low_pass(cut(far, onsets(far)[0], 3.0, fade=0.8), 2600))
    # Thaana SLR.
    near = load(os.path.join(LIB, "SKS", "U_14P.wav"))
    print("sks onsets", [round(i / RATE, 2) for i in onsets(near)][:6])
    write("gun_thaana_slr", cut(near, onsets(near)[0], 1.1, fade=0.3))
    # Saintalis, near and (for all the automatic rifles) far.
    near = load(os.path.join(LIB, "AK-47", "C_28P.wav"))
    print("ak onsets", [round(i / RATE, 2) for i in onsets(near)][:6])
    write("gun_saintalis", cut(near, onsets(near)[0], 0.8, fade=0.25))
    far = load(os.path.join(LIB, "AK-47", "C_31P.wav"))
    print("ak far onsets", [round(i / RATE, 2) for i in onsets(far)][:6])
    write("gun_auto_far", low_pass(cut(far, onsets(far)[0], 1.8, fade=0.6), 3000))
    # A bullet going past the ear.
    crack = load(os.path.join(FREE, "crack_bullet_morganpurkis.mp3"))
    write("bullet_crack", cut(crack, onsets(crack)[0], 0.4, fade=0.1), peak=0.8)


main()

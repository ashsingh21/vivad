"""Engine loops from real recordings (research/vehicles/references/sounds, all CC0), and synthesized horns.

python3 -I tools/audio/vehicles.py    writes assets/audio/engine_*.ogg and horn_*.ogg

Each loop is the steadiest stretch of its recording: the window with the least change in level and the
least sharp sound over the engine's own band (a horn, a shout, a door), its tail cross-faded
into its head so it repeats without a seam. Needs numpy and soundfile.
"""

import os

import numpy as np
import soundfile as sf

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SOUNDS = os.path.join(ROOT, "research", "vehicles", "references", "sounds")
OUT = os.path.join(ROOT, "assets", "audio")

# name: (recording, seconds of loop, range of the recording to look in, gain)
LOOPS = {
    "engine_auto": ("tuktuk_ride_bengaluru_832083.mp3", 2.6, (2.0, 94.0)),
    "engine_bike": ("bike_firing_350cc_274962.mp3", 2.0, (1.0, 11.0)),
    "engine_thump": ("bull1_159326.mp3", 2.6, (3.0, 34.0)),
}


def band_energy(x, sr, lo, hi, frame):
    """Energy in a band, per frame."""
    n = len(x) // frame
    out = np.zeros(n)
    f = np.fft.rfftfreq(frame, 1 / sr)
    m = (f >= lo) & (f < hi)
    win = np.hanning(frame)
    for i in range(n):
        s = np.abs(np.fft.rfft(x[i * frame : (i + 1) * frame] * win)) ** 2
        out[i] = s[m].sum()
    return out


def steadiest(x, sr, length, reach):
    frame = int(sr * 0.05)
    low = np.sqrt(band_energy(x, sr, 40, 500, frame))
    high = np.sqrt(band_energy(x, sr, 900, 4500, frame))
    k = int(length / 0.05)
    best, at = 1e9, 0
    for i in range(int(reach[0] / 0.05), min(len(low) - k, int(reach[1] / 0.05)), 4):
        a, b = low[i : i + k], high[i : i + k]
        # Steady, loud in the engine's band, with no sharp peaks above it.
        score = a.std() / (a.mean() + 1e-9) + (b.max() / (a.mean() + 1e-9)) * 0.6 + (b.std() / (b.mean() + 1e-9)) * 0.3
        if score < best and a.mean() > 0.3 * low.mean():
            best, at = score, i
    return at * frame, best


def make_loop(x, sr, start, length):
    n, fade = int(length * sr), int(0.35 * sr)
    seg = x[start : start + n + fade].copy()
    seg -= seg.mean()
    # The extra `fade` samples past the end fade out over the head fading in: the end meets the start.
    t = np.linspace(0, 1, fade)
    head = seg[:fade] * np.sqrt(t) + seg[n : n + fade] * np.sqrt(1 - t)
    loop = np.concatenate([head, seg[fade:n]])
    return loop / (np.abs(loop).max() + 1e-9) * 0.89


def horn(freqs, length, pulses, sr=44100):
    """An electric horn: buzzing notes, clipped a little, and pulsed."""
    t = np.arange(int(length * sr)) / sr
    y = np.zeros_like(t)
    for f in freqs:
        for k in range(1, 14):
            y += np.sin(2 * np.pi * f * k * t + k) / k**0.9
    y = np.tanh(y * 1.2)
    env = np.zeros_like(t)
    width = length / pulses
    for p in range(pulses):
        a, b = int(p * width * sr), int((p + 0.82) * width * sr)
        e = np.ones(b - a)
        ramp = int(0.008 * sr)
        e[:ramp] = np.linspace(0, 1, ramp)
        e[-ramp * 3 :] = np.linspace(1, 0, ramp * 3)
        env[a:b] = e
    y = y * env
    X = np.fft.rfft(y)
    f = np.fft.rfftfreq(len(y), 1 / sr)
    X *= 1 / (1 + (f / 3200) ** 4) * (1 - 1 / (1 + (f / 260) ** 4))
    y = np.fft.irfft(X, len(y))
    return y / (np.abs(y).max() + 1e-9) * 0.89, sr


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, (src, length, reach) in LOOPS.items():
        x, sr = sf.read(os.path.join(SOUNDS, src), always_2d=True)
        x = x.mean(1)
        start, score = steadiest(x, sr, length, reach)
        loop = make_loop(x, sr, start, length)
        path = os.path.join(OUT, f"{name}.ogg")
        sf.write(path, loop, sr, format="OGG", subtype="VORBIS")
        print(f"{name}: {src} from {start / sr:.2f} s, {length} s, score {score:.2f}, {os.path.getsize(path) / 1024:.0f} kB")
    for name, (freqs, length, pulses) in {
        "horn_auto": ((415.0, 523.0), 0.5, 2),
        "horn_bike": ((494.0, 622.0), 0.42, 2),
        "horn_car": ((370.0, 466.0), 0.55, 1),
    }.items():
        y, sr = horn(freqs, length, pulses)
        sf.write(os.path.join(OUT, f"{name}.ogg"), y, sr, format="OGG", subtype="VORBIS")
        print(f"{name}: {length} s")


if __name__ == "__main__":
    main()

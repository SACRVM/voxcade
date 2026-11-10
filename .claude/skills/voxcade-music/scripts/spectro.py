#!/usr/bin/env python3
"""A song or sound you cannot hear, looked at: a spectrogram PNG (0 to
--top Hz, log loudness, a red line every --section seconds) and the
numbers that tell a mix's trouble: length, peak, loudness per section, the
share of energy above 4 kHz (harshness), the DC offset (thumps, lost
headroom), left against right, and the seam where a looping song starts
over. numpy and Pillow.

usage: spectro.py SONG.wav OUT.png [--section SECONDS] [--top HZ]
       (a section of 8 bars lasts 8 * 4 * 60 / tempo seconds)
"""
import sys
import wave

import numpy as np
from PIL import Image, ImageDraw


def main(args):
    if len(args) < 2:
        raise SystemExit(__doc__)
    path, out = args[0], args[1]
    section = float(args[args.index('--section') + 1]) if '--section' in args else 0.0
    top = float(args[args.index('--top') + 1]) if '--top' in args else 6000.0
    w = wave.open(path)
    rate, ch = w.getframerate(), w.getnchannels()
    x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
    x = x.reshape(-1, ch)
    mono = x.mean(axis=1)
    total = len(mono) / rate
    print(f"{total:.1f} s, {ch} channels at {rate} Hz, peak {np.abs(x).max():.2f}, rms {np.sqrt((mono ** 2).mean()):.3f}")
    print(f"DC offset {mono.mean():+.4f} (above 0.005 something carries a pulse off duty 0.5 or a click)")
    if ch == 2:
        l, r = np.sqrt((x[:, 0] ** 2).mean()), np.sqrt((x[:, 1] ** 2).mean())
        print(f"left {l:.3f}, right {r:.3f}")
    n, hop = 4096, 1024
    if len(mono) < n:
        raise SystemExit("too short for a spectrogram")
    frames = np.lib.stride_tricks.sliding_window_view(mono, n)[::hop] * np.hanning(n)
    spec = np.abs(np.fft.rfft(frames, axis=1)) ** 2
    freqs = np.fft.rfftfreq(n, 1 / rate)
    print(f"above 4 kHz: {100 * spec[:, freqs > 4000].sum() / spec.sum():.2f} % of the energy")
    if section > 0:
        k = int(section * rate)
        parts = [mono[i:i + k] for i in range(0, len(mono), k)]
        print("rms per section: " + ' '.join(f"{np.sqrt((p ** 2).mean()):.3f}" for p in parts if len(p)))
    edge = int(0.05 * rate)
    print(f"seam: last 50 ms rms {np.sqrt((mono[-edge:] ** 2).mean()):.3f}, first 50 ms {np.sqrt((mono[:edge] ** 2).mean()):.3f}")
    keep = freqs <= top
    db = 10 * np.log10(spec[:, keep] + 1e-12)
    db = np.clip((db - (db.max() - 70)) / 70, 0, 1)
    img = (255 * db.T[::-1]).astype(np.uint8)
    im = Image.fromarray(img).resize((min(2400, img.shape[1]), 400)).convert('RGB')
    d = ImageDraw.Draw(im)
    if section > 0:
        t = 0.0
        while t < total:
            xx = int(t / total * im.width)
            d.line([(xx, 0), (xx, im.height)], fill=(255, 60, 60))
            t += section
    for f in (250, 500, 1000, 2000, 4000):
        if f < top:
            y = int((1 - f / top) * im.height)
            d.line([(0, y), (10, y)], fill=(80, 160, 255))
            d.text((12, y - 6), f"{f}", fill=(80, 160, 255))
    im.save(out)
    print(f"wrote {out}")


if __name__ == '__main__':
    main(sys.argv[1:])

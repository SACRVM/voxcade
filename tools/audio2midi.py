#!/usr/bin/env python3
"""Turns audio stems (Suno's stem export: Drums, Bass, Synth, ...) into MIDI
files for tools/midi2mml.py, for songs whose tempo is known and steady.

    python3 tools/audio2midi.py --bpm 120 --out games/moreblood/music/move_on \\
        ~/Downloads/stems/*.mp3

Every stem becomes `<name>.mid` in `--out` (the name in lower case, without
a leading track number: "1 Bass.mp3" -> bass.mid). Everything sits on a
16th grid starting at time 0, the grid found from the drums.

    drums, percussion   onsets by band: kick 36, snare 38, closed hat 42,
                        open hat 46 (rings on), crash 49 (rings long), on
                        MIDI channel 10
    everything else     the strongest pitches of every 16th (harmonics
                        summed, up to --voices at once), held while they
                        sound and struck again on an onset

Reads anything ffmpeg decodes. Stems with nothing in them (bleed from the
separation) come out empty or nearly so; leave them out of the import.
"""

import argparse
import re
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np

DRUM_NAMES = ("drums", "percussion")


def decode(path, rate):
    """Mono float samples at `rate`, through ffmpeg."""
    raw = subprocess.run(["ffmpeg", "-nostdin", "-loglevel", "error", "-i", str(path), "-ac", "1", "-ar", str(rate),
                          "-f", "f32le", "-"], capture_output=True, check=True).stdout
    return np.frombuffer(raw, np.float32)


def stem_name(path):
    name = re.sub(r"^\d+\s*", "", Path(path).stem).strip().lower()
    return re.sub(r"\W+", "_", name)


def spectrogram(a, n, hop):
    frames = np.lib.stride_tricks.sliding_window_view(a, n)[::hop]
    window = np.hanning(n).astype(np.float32)
    out = np.empty((len(frames), n // 2 + 1), np.float32)
    for i in range(0, len(frames), 2048):
        out[i:i + 2048] = np.abs(np.fft.rfft(frames[i:i + 2048] * window, axis=1))
    return out


def grid_phase(a, rate, step):
    """Where the 16th grid lies: the offset (seconds) that puts the most
    onset strength on it."""
    hop, n = 128, 1024
    mag = spectrogram(a, n, hop)
    flux = np.concatenate([[0.0], np.maximum(0, np.diff(np.log1p(mag * 10), axis=0)).sum(axis=1)])
    t = (np.arange(len(flux)) * hop + n / 2) / rate
    best = (-1.0, 0.0)
    for phase in np.linspace(0, step, 64, endpoint=False):
        dev = (t - phase) / step
        dev = np.abs(dev - np.round(dev))
        score = float((flux * np.exp(-(dev / 0.08) ** 2)).sum())
        best = max(best, (score, phase))
    return best[1]


def drums(a, rate, step, phase, steps):
    """(step, length, note, velocity) for the drum hits: a band's level
    right after a grid point against just before it."""
    hop, n = 128, 1024
    mag = spectrogram(a, n, hop)
    freqs = np.fft.rfftfreq(n, 1 / rate)
    t = (np.arange(len(mag)) * hop + n / 2) / rate

    def band(lo, hi):
        return mag[:, (freqs >= lo) & (freqs < hi)].sum(axis=1) ** 2 + 1e-9

    def levels(e, frm, to):
        """Mean of `e` from `frm` to `to` seconds around every grid point."""
        out = np.zeros(steps)
        idx = np.searchsorted(t, phase + np.arange(steps)[:, None] * step + np.array([frm, to]))
        for k, (i, j) in enumerate(idx):
            out[k] = e[i:max(j, i + 1)].mean() if i < len(e) else 0.0
        return out

    hits = []
    found = {}
    # Low sounds take longer to build up: a kick's boom can peak 50 ms after
    # its click.
    windows = {"low": (35, 120, 0.07), "mid": (150, 450, 0.04), "noise": (1500, 5000, 0.04), "high": (7000, 16000, 0.025)}
    for name, (lo, hi, span) in windows.items():
        e = band(lo, hi)
        after = levels(e, 0.0, span)
        before = levels(e, -0.04, -0.005)
        tail = levels(e, 0.5 * step, 0.9 * step)
        loud = np.percentile(after, 95)
        found[name] = (after, after / np.maximum(before, 1e-9), tail, loud)

    def onset(name, ratio, share):
        after, rise, _, loud = found[name]
        return (rise > ratio) & (after > share * loud)

    kick = onset("low", 3.0, 0.25)
    # The snare's body or rattle; on a kick (whose beater clicks in the same
    # bands) only when the body is loud on its own.
    snare = (onset("mid", 3.0, 0.3) | onset("noise", 2.5, 0.5)) & ~kick
    snare |= kick & onset("mid", 3.0, 0.9) & onset("noise", 2.5, 0.8)
    # A snare's rattle reaches up into the hats' band: on a snare only a
    # clear hat of its own counts.
    hat = onset("high", 1.5, 0.3) & (~snare | onset("high", 3.0, 0.6))
    after, _, tail, loud = found["high"]
    common = np.percentile(after, 90)
    vel = lambda x: int(np.clip(40 + 87 * x, 1, 127))
    for k in range(steps):
        if kick[k]:
            hits.append((k, 1, 36, vel(found["low"][0][k] ** 0.5 / found["low"][3] ** 0.5)))
        if snare[k]:
            hits.append((k, 1, 38, vel(max(found["mid"][0][k] / found["mid"][3], found["noise"][0][k] / found["noise"][3]) ** 0.5)))
        if hat[k]:
            ringing = tail[k] > 0.6 * after[k]
            long_ring = k + 8 < steps and np.all(after[k + 1:k + 8] > 0.8 * common)
            if ringing and after[k] > 1.6 * common and long_ring:
                note = 49
            elif ringing:
                note = 46
            else:
                note = 42
            hits.append((k, 1, note, vel((after[k] / loud) ** 0.5)))
    return hits


def midi_freq(m):
    return 440.0 * 2 ** ((np.asarray(m, np.float64) - 69) / 12)


def pitched(a, rate, step, phase, steps, voices, low, high):
    """(step, length, note, velocity) for the pitches that sound."""
    size = 16384
    freqs = np.fft.rfftfreq(size, 1 / rate)
    notes = np.arange(low, high + 1)
    f0 = midi_freq(notes)
    harmonics = np.arange(1, 11)
    weights = 0.85 ** (harmonics - 1)
    # For every note and harmonic, the bins within 40 cents of it.
    spans = []
    for f in f0:
        row = []
        for h in harmonics:
            fh = f * h
            if fh > rate * 0.45 or fh > 6000:
                row.append(None)
                continue
            lo = np.searchsorted(freqs, fh * 2 ** (-0.4 / 12))
            hi = max(lo + 1, np.searchsorted(freqs, fh * 2 ** (0.4 / 12)))
            row.append((lo, hi))
        spans.append(row)

    def salience(mag):
        out = np.zeros(len(notes))
        for i, row in enumerate(spans):
            out[i] = sum(w * mag[s[0]:s[1]].max() for w, s in zip(weights, row) if s is not None)
        return out

    def odd_share(mag, i):
        """How much of a note's harmonic sum is in its odd harmonics (the
        fundamental among them): little means it is an octave too low."""
        odd = even = 0.0
        for h, w, s in zip(harmonics, weights, spans[i]):
            if s is not None:
                v = w * mag[s[0]:s[1]].max()
                if h % 2:
                    odd += v
                else:
                    even += v
        return odd / max(odd + even, 1e-9)

    def remove(mag, i):
        for s in spans[i]:
            if s is not None:
                mag[s[0]:s[1]] *= 0.1

    def spectrum(frm, to):
        """The magnitude spectrum of part of every 16th (fractions of it),
        with the noise floor (and the bleed of other stems) taken out."""
        window = np.hanning(int((to - frm) * step * rate)).astype(np.float32)
        band = (freqs > 30) & (freqs < 4000)

        def at(k):
            begin = int((phase + (k + frm) * step) * rate)
            seg = a[begin:begin + len(window)]
            if len(seg) < len(window):
                return None
            mag = np.abs(np.fft.rfft(seg * window, size))
            return np.maximum(mag - 2.0 * np.median(mag[band]), 0.0)
        return at

    whole, early, late = spectrum(0.1, 0.95), spectrum(0.05, 0.5), spectrum(0.5, 0.95)

    # The stem's onsets (spectral flux at each grid point): in a one-voice
    # stem every onset is a new note.
    spec = spectrogram(a, 2048, 256)
    flux = np.concatenate([[0.0], np.maximum(0, np.diff(np.log1p(spec * 10), axis=0)).sum(axis=1)])
    t = (np.arange(len(flux)) * 256 + 1024) / rate
    onset = np.zeros(steps)
    for k in range(steps):
        near = (t >= phase + (k - 0.3) * step) & (t < phase + (k + 0.3) * step)
        if near.any():
            onset[k] = flux[near].max()
    onset /= max(np.percentile(onset, 95), 1e-9)
    picks, full, attack, tail, peak = [], [], [], [], []
    for k in range(steps):
        mag = whole(k)
        if mag is None:
            picks.append([])
            full.append(np.zeros(len(notes)))
            attack.append(np.zeros(len(notes)))
            tail.append(np.zeros(len(notes)))
            peak.append(0.0)
            continue
        full.append(salience(mag))
        attack.append(salience(early(k)))
        tail.append(salience(late(k)))
        found = []
        first = None
        for _ in range(voices):
            s = salience(mag)
            i = int(np.argmax(s))
            while i + 12 < len(notes) and odd_share(mag, i) < 0.3:
                i += 12
            if first is None:
                first = s[i]
            elif s[i] < 0.45 * first:
                break
            # Not an echo of a lower pick (an octave, a fifth above it).
            if all((notes[i] - notes[j]) not in (12, 19, 24) for j, _ in found):
                found.append((i, s[i]))
            remove(mag, i)
        picks.append(found)
        peak.append(first or 0.0)
    loud = np.percentile([p for p in peak if p > 0], 90) if any(peak) else 1.0

    # Notes start where they are picked, go on while they still sound
    # clearly (a held chord is not picked whole every 16th), and start again
    # when their own sound jumps up at the start of a 16th (a repeated note).
    out = []
    active = {}
    for k in range(steps + 1):
        now = {}
        if k < steps:
            for i, s in picks[k]:
                if s > 0.12 * loud:
                    now[i] = s
        for i in list(active):
            sounding = k < steps and (i in now or (full[k][i] > 0.3 * peak[k] and full[k][i] > 0.08 * loud))
            again = k < steps and (attack[k][i] > 1.4 * tail[k - 1][i] + 0.02 * loud
                                   or (voices == 1 and onset[k] > 0.35))
            if not sounding or again:
                start, s = active.pop(i)
                out.append((start, k - start, int(notes[i]), int(np.clip(30 + 90 * s / loud, 1, 127))))
                if sounding:
                    active[i] = (k, full[k][i])
        for i, s in now.items():
            if i not in active:
                active[i] = (k, s)
        # No more at once than the stem has voices: held notes give way to
        # picked ones, the faintest first (the harmonics of a new note keep
        # old ones "sounding").
        while k < steps and len(active) > voices:
            held = [i for i in active if i not in now] or list(active)
            i = min(held, key=lambda i: full[k][i])
            start, s = active.pop(i)
            out.append((start, k - start, int(notes[i]), int(np.clip(30 + 90 * s / loud, 1, 127))))
    return out


def write_midi(path, notes, bpm, channel):
    """A format-0 file: `notes` as (step, length, note, velocity) in 16ths."""
    ticks = 480
    per_step = ticks // 4
    events = []
    for step, length, note, vel in notes:
        events.append((step * per_step, 1, note, vel))
        events.append(((step + length) * per_step - (1 if channel == 9 else 0), 0, note, 0))
    events.sort(key=lambda e: (e[0], e[1]))

    def varlen(v):
        out = [v & 0x7F]
        v >>= 7
        while v:
            out.insert(0, (v & 0x7F) | 0x80)
            v >>= 7
        return bytes(out)

    track = bytearray(varlen(0) + b"\xff\x51\x03" + struct.pack(">I", round(60_000_000 / bpm))[1:])
    now = 0
    for at, on, note, vel in events:
        track += varlen(at - now) + bytes(((0x90 if on else 0x80) | channel, note, vel))
        now = at
    track += varlen(0) + b"\xff\x2f\x00"
    path.write_bytes(b"MThd" + struct.pack(">IHHH", 6, 0, 1, ticks) + b"MTrk" + struct.pack(">I", len(track)) + track)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bpm", type=float, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--voices", type=int, default=3, help="pitches at once in a melodic stem")
    parser.add_argument("stems", nargs="+")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    step = 60 / args.bpm / 4

    stems = {stem_name(p): p for p in args.stems}
    clock = next((p for name, p in stems.items() if name == "drums"), args.stems[0])
    a = decode(clock, 44100)
    phase = grid_phase(a, 44100, step)
    steps = int((len(a) / 44100 - phase) / step)
    print(f"grid: {args.bpm} BPM, 16ths from {phase * 1000:.0f} ms, {steps // 16} bars (from {Path(clock).name})")

    for name, path in stems.items():
        if name in DRUM_NAMES:
            notes = drums(decode(path, 44100), 44100, step, phase, steps)
            channel = 9
        else:
            low, high = (24, 60) if name == "bass" else (36, 96)
            voices = 1 if name == "bass" else args.voices
            notes = pitched(decode(path, 22050), 22050, step, phase, steps, voices, low, high)
            channel = 0
        target = args.out / f"{name}.mid"
        write_midi(target, notes, args.bpm, channel)
        print(f"  {target}: {len(notes)} notes")


if __name__ == "__main__":
    sys.exit(main())

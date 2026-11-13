#!/usr/bin/env python3
"""Turns MIDI files into Voxcade MML: music the game plays on its SID-like
voices, written as text you can edit afterwards.

Made for MIDI from audio (Suno stems through Basic Pitch and the like):
notes are timed in seconds through the file's tempo map, the real tempo is
measured from the onsets, and everything is snapped to a 16th-note grid.
Each MML channel is one voice, so chords become a single line: the highest
note (melodies), the lowest (bass), or the drums by kind.

What goes where is said in an import file (TOML):

    title = "Jungle Pulse"
    out = "jungle_pulse.mml"     # next to the import file
    # bpm = 144                  # instead of measuring it
    # drift_window = 5           # follow tempo drift over this many seconds (0: off)
    # crop = [8, 55]             # only these bars become the song (counted after the
    #                            # bar lines are found): leave out a free intro, an ending
    # timing = "drums"           # measure tempo and drift from the drum parts only
    #                            # (when the pitched stems are too smeared to keep time)
    tempo = 144                  # what the MML says (default: the measured tempo)

    [instruments]                # copied into the MML as @name lines
    bass = "square duty=0.5 decay=0.12 sustain=0.6 lowpass=1100 volume=0.5"

    [[part]]
    channel = "B"                # MML channel letter
    file = "midi/Bass.mid"       # relative to the import file
    midi_channels = [1]          # which MIDI channels (default: all)
    bars = [0, 39]               # only these bars (default: all)
    pick = "low"                 # "high", "low", "chords" or "drums"
    instrument = "bass"
    octave = 0                   # move it up or down
    fold = [36, 47]              # keep it within these MIDI notes, by octaves
    volume = 12                  # MML v0..v15
    pan = 0
    min_velocity = 0             # leave out quieter notes (bleed between stems)
    pitch = [55, 62]             # only notes in this MIDI range (split riff from melody)
    legato = false               # hold every note until the next (up to a bar)
    fill_gaps = 1                # or only close gaps up to this many 16ths (holes from audio)
    gate = 7                     # MML q1..q8: how much of each note sounds
    snap = 4                     # pull every note to the nearest multiple of this
                                 # many 16ths (4: the beat, for a steady kick)
    delay = 3                    # play it this many 16ths late: with a second part
                                 # on the same file, quieter, the C64 echo (what runs
                                 # past the end comes round to the start, for the loop)

    [[part]]                     # chords: each chord becomes its root, played
    channel = "C"                # by an arpeggio instrument for its kind, the
    file = "midi/Keys.mid"       # C64 way to play chords on one voice
    pick = "chords"
    chords = { major = "maj", minor = "min", fifth = "fifth", note = "stab" }
    octave = 0                   # roots sit in octave 3, moved by this

    [[part]]                     # drums: GM note numbers to instruments,
    channel = "D"                # earlier ones win when two hit at once
    file = "midi/Drums.mid"
    midi_channels = [9]
    pick = "drums"
    drums = { 36 = "kick", 38 = "snare", 40 = "snare" }

The bar lines are found from the drums (kick and crash on the one, snare
on two and four); `downbeat = 3` in the import file moves them by hand
(in 16ths).

usage: python3 tools/midi2mml.py games/moreblood/music/jungle_pulse.import.toml
"""

import bisect
import struct
import sys
import tomllib
from pathlib import Path

import numpy as np

NAMES = ["c", "c+", "d", "d+", "e", "f", "f+", "g", "g+", "a", "a+", "b"]
STEPS_PER_BAR = 16


def varlen(data, i):
    v = 0
    while True:
        b = data[i]
        i += 1
        v = (v << 7) | (b & 0x7F)
        if not b & 0x80:
            return v, i


def read_midi(path):
    """Notes as (start seconds, end seconds, channel, pitch, velocity)."""
    data = Path(path).read_bytes()
    if data[:4] != b"MThd":
        raise SystemExit(f"{path}: not a MIDI file")
    _, ntracks, division = struct.unpack(">HHH", data[8:14])
    if division & 0x8000:
        raise SystemExit(f"{path}: SMPTE timing is not supported")
    i = 8 + struct.unpack(">I", data[4:8])[0]
    tempos, raw = [], []
    for _ in range(ntracks):
        if data[i:i + 4] != b"MTrk":
            raise SystemExit(f"{path}: broken track")
        end = i + 8 + struct.unpack(">I", data[i + 4:i + 8])[0]
        j, tick, status, on = i + 8, 0, 0, {}
        while j < end:
            delta, j = varlen(data, j)
            tick += delta
            b = data[j]
            if b == 0xFF:
                ln, k = varlen(data, j + 2)
                if data[j + 1] == 0x51:
                    tempos.append((tick, int.from_bytes(data[k:k + ln], "big")))
                j = k + ln
                continue
            if b in (0xF0, 0xF7):
                ln, k = varlen(data, j + 1)
                j = k + ln
                continue
            if b & 0x80:
                status = b
                j += 1
            kind, ch = status & 0xF0, status & 0x0F
            if kind in (0xC0, 0xD0):
                j += 1
                continue
            a, v = data[j], data[j + 1]
            j += 2
            if kind == 0x90 and v > 0:
                on[(ch, a)] = (tick, v)
            elif kind == 0x80 or (kind == 0x90 and v == 0):
                if (ch, a) in on:
                    t0, vel = on.pop((ch, a))
                    raw.append((t0, tick, ch, a, vel))
        i = end
    tempos.sort()
    if not tempos or tempos[0][0] > 0:
        tempos.insert(0, (0, 500_000))
    ticks = [t for t, _ in tempos]
    secs = [0.0]
    for (t0, us), (t1, _) in zip(tempos, tempos[1:]):
        secs.append(secs[-1] + (t1 - t0) / division * us / 1e6)

    def seconds(tick):
        k = bisect.bisect_right(ticks, tick) - 1
        return secs[k] + (tick - ticks[k]) / division * tempos[k][1] / 1e6

    return [(seconds(a), seconds(b), ch, p, v) for a, b, ch, p, v in raw]


def measure_grid(onsets):
    """The tempo (60..200 BPM) and offset (seconds) whose 16th-note grid fits
    the onsets best, and how far off they are on average (share of a 16th)."""
    t = np.asarray(onsets)

    def fit(bpms, phases_per_step):
        best = (1e9, 120.0, 0.0)
        for bpm in bpms:
            step = 60 / bpm / 4
            phases = np.arange(phases_per_step) * step / phases_per_step
            d = (t[None, :] - phases[:, None]) / step
            err = np.abs(d - np.round(d)).mean(axis=1)
            k = int(err.argmin())
            if err[k] < best[0]:
                best = (float(err[k]), float(bpm), float(phases[k]))
        return best

    _, coarse, _ = fit(np.arange(60, 200, 0.1), 16)
    return fit(np.arange(coarse - 0.15, coarse + 0.15, 0.005), 64)


def mono_line(notes, pick, legato=False, fill_gaps=0):
    """One voice from notes on the grid, (start, end, pitch): at every onset
    the highest (or lowest) note starting there; each lasts until its end or
    the next onset (legato: until the next onset, up to a bar; fill_gaps:
    until the next onset if that is at most this many 16ths after its end)."""
    starts = {}
    for s, e, p in notes:
        cur = starts.get(s)
        if cur is None or (p > cur[1] if pick == "high" else p < cur[1]):
            starts[s] = (e, p)
    keys = sorted(starts)
    line = []
    for k, s in enumerate(keys):
        e, p = starts[s]
        if k + 1 < len(keys):
            nxt = keys[k + 1]
            if legato:
                e = min(nxt, s + STEPS_PER_BAR)
            elif nxt - e <= fill_gaps:
                e = nxt
            else:
                e = min(e, nxt)
        line.append((s, max(e, s + 1), p))
    return line


def chord_line(notes, kinds, octave_shift, fill_gaps):
    """One voice from chords: at every onset with two or more notes the
    root and whether it is major, minor or a bare fifth; each lasts until
    the chord's end or the next chord. Single notes between chords (broken
    chords, bleed) are left out, so the arpeggio runs on."""
    groups = {}
    for s, e, p in notes:
        groups.setdefault(s, []).append((e, p))
    groups = {s: g for s, g in groups.items() if len(g) >= 2}
    keys = sorted(groups)
    line = []
    for k, s in enumerate(keys):
        group = groups[s]
        pcs = {p % 12 for _, p in group}
        low = min(p for _, p in group)
        kind, root = "note", low % 12
        if len(group) >= 2:
            # Try the bass note as the root first, then the others.
            for r in sorted(pcs, key=lambda pc: (pc - low) % 12):
                if {(r + 4) % 12, (r + 7) % 12} <= pcs:
                    kind, root = "major", r
                    break
                if {(r + 3) % 12, (r + 7) % 12} <= pcs:
                    kind, root = "minor", r
                    break
            else:
                for r in sorted(pcs, key=lambda pc: (pc - low) % 12):
                    if (r + 7) % 12 in pcs:
                        kind, root = "fifth", r
                        break
        # A chord on the same root that only lost a note (a bare fifth, a
        # lone note) stays what it was, so the arpeggio does not flicker.
        if line and kind in ("fifth", "note"):
            _, _, (last_pitch, last_kind) = line[-1]
            if last_pitch % 12 == root and last_kind in (kinds["major"], kinds["minor"]):
                kind = "major" if last_kind == kinds["major"] else "minor"
        e = max(e for e, _ in group)
        if k + 1 < len(keys):
            nxt = keys[k + 1]
            e = nxt if nxt - e <= fill_gaps else min(e, nxt)
        # Roots (and lone notes in octaves) sit in octave 3, moved by octave_shift.
        pitch = 48 + root + 12 * octave_shift
        line.append((s, max(e, s + 1), (pitch, kinds[kind])))
    return line


def delayed(line, by, total):
    """`line` played `by` 16ths late; what runs past `total` comes round to
    the start, so the echo of the end sounds as the song loops. One voice:
    a note ends where the next begins."""
    moved = []
    for s, e, w in line:
        s, e = s + by, e + by
        if s >= total:
            moved.append((s - total, e - total, w))
        else:
            moved.append((s, min(e, total), w))
            if e > total:
                moved.append((0, e - total, w))
    moved.sort()
    return [(s, min(e, moved[i + 1][0]) if i + 1 < len(moved) else e, w)
            for i, (s, e, w) in enumerate(moved) if i + 1 >= len(moved) or moved[i + 1][0] > s]


def drum_line(notes, drums):
    """One drum voice: at every step the first kind in the mapping's order."""
    order = list(drums.values())
    hits = {}
    for s, _, p in notes:
        kind = drums.get(str(p))
        if kind is None:
            continue
        if s not in hits or order.index(kind) < order.index(hits[s]):
            hits[s] = kind
    keys = sorted(hits)
    return [(s, (keys[k + 1] if k + 1 < len(keys) else s + 1), hits[s]) for k, s in enumerate(keys)]


LENGTHS = [(16, "1"), (12, "2."), (8, "2"), (6, "4."), (4, "4"), (3, "8."), (2, "8"), (1, "16")]


def lengths(steps):
    out = []
    while steps > 0:
        for n, text in LENGTHS:
            if n <= steps:
                out.append(text)
                steps -= n
                break
    return out


def write_channel(line, total, drums, instrument, volume, pan, octave_shift, gate=7):
    """MML for one voice: a bar line every 16 steps, a new line every 4 bars,
    notes held across a bar line tied over it."""
    tokens = [] if drums or instrument is None else [f"@{instrument}"]
    tokens += [f"v{volume}", "q8" if drums else f"q{gate}", "l16"]
    if pan:
        tokens.append(f"p{pan}")
    octave, current, pos = None, None, 0

    def pieces(until):
        """Spans from pos to `until`, split at bar lines."""
        nonlocal pos
        while pos < until:
            bar_end = (pos // STEPS_PER_BAR + 1) * STEPS_PER_BAR
            span = min(until, bar_end) - pos
            pos += span
            yield span, pos % STEPS_PER_BAR == 0

    for s, e, what in line:
        if s < pos:
            continue
        for span, bar in pieces(s):
            tokens.extend("r" + l for l in lengths(span))
            if bar:
                tokens.append("|")
        if drums:
            if what != current:
                tokens.append(f"@{what}")
                current = what
            name = "c"
        else:
            if isinstance(what, tuple):
                # A chord: its root, already in place, and its instrument.
                what, ins = what
                if ins != current:
                    tokens.append(f"@{ins}")
                    current = ins
                p = what
            else:
                p = what + 12 * octave_shift
            if p // 12 - 1 != octave:
                octave = p // 12 - 1
                tokens.append(f"o{octave}")
            name = NAMES[p % 12]
        spans = list(pieces(min(e, total)))
        for k, (span, bar) in enumerate(spans):
            tied = "&" if k + 1 < len(spans) else ""
            tokens.append(name + "&".join(lengths(span)) + tied)
            if bar:
                tokens.append("|")
    for span, bar in pieces(total):
        tokens.extend("r" + l for l in lengths(span))
        if bar:
            tokens.append("|")

    rows, row, bars = [], [], 0
    for t in tokens:
        row.append(t)
        if t == "|":
            bars += 1
            if bars % 4 == 0:
                rows.append(" ".join(row[:-1]))
                row = []
    if row and row[-1] == "|":
        row.pop()
    if row:
        rows.append(" ".join(row))
    return rows


def main():
    if len(sys.argv) == 2 and sys.argv[1] in ("-h", "--help"):
        print(__doc__)
        print("usage: midi2mml.py IMPORT.toml")
        return
    if len(sys.argv) != 2 or sys.argv[1].startswith("-"):
        raise SystemExit("usage: midi2mml.py IMPORT.toml (see --help)")
    spec_path = Path(sys.argv[1])
    spec = tomllib.loads(spec_path.read_text())
    base = spec_path.parent
    parts = spec["part"]
    loaded = {}
    for part in parts:
        f = part["file"]
        if f not in loaded:
            loaded[f] = read_midi(base / f)

    def chosen(part):
        chans = part.get("midi_channels")
        lo = part.get("min_velocity", 0)
        low, high = part.get("pitch", [0, 127])
        return [n for n in loaded[part["file"]]
                if (chans is None or n[2] in chans) and n[4] >= lo and low <= n[3] <= high]

    timekeepers = [part for part in parts if spec.get("timing") != "drums" or part.get("pick") == "drums"]
    onsets = sorted({round(n[0], 4) for part in timekeepers for n in chosen(part)})
    if "bpm" in spec:
        bpm = float(spec["bpm"])
        step = 60 / bpm / 4
        phase = min(onsets) % step if onsets else 0.0
        err = None
    else:
        err, bpm, phase = measure_grid(onsets[:1500])
        step = 60 / bpm / 4

    # Songs from audio drift a little against any fixed tempo: follow it by
    # taking out the median offset of the onsets within 5 seconds.
    window = spec.get("drift_window", 5.0)
    times = np.asarray(onsets)
    residual = (times - phase) / step
    residual = residual - np.round(residual)

    def drift(t):
        if not window:
            return 0.0
        near = residual[np.abs(times - t) <= window]
        return float(np.median(near)) if len(near) >= 8 else 0.0

    marks = np.arange(0.0, (times[-1] if len(times) else 0.0) + 1.0, 0.25)
    table = np.array([drift(t) for t in marks]) if len(times) else np.zeros(1)
    if len(times):
        d = (times - phase) / step - np.interp(times, marks, table)
        err = float(np.abs(d - np.round(d)).mean())

    def to_step(t):
        correction = float(np.interp(t, marks, table)) if len(times) else 0.0
        return max(0, round((t - phase) / step - correction))

    # Where the bar starts: the 16th where kick and crash land and that puts
    # the snare on two and four, from all drum parts.
    count = [0.0] * STEPS_PER_BAR
    for part in parts:
        if part.get("pick") != "drums":
            continue
        for s, _, _, p, _ in chosen(part):
            k = to_step(s) % STEPS_PER_BAR
            if p in (35, 36):
                count[k] += 1
            elif p in (49, 57):
                count[k] += 2
            elif p in (38, 40):
                count[(k - 4) % STEPS_PER_BAR] += 0.5
                count[(k - 12) % STEPS_PER_BAR] += 0.5
    if "downbeat" in spec:
        downbeat = spec["downbeat"] % STEPS_PER_BAR
    else:
        downbeat = max(range(STEPS_PER_BAR), key=lambda k: count[k]) if any(count) else 0
    # Shift so that 16th is a bar start, by a pickup at the front.
    shift = (STEPS_PER_BAR - downbeat) % STEPS_PER_BAR

    crop = spec.get("crop")
    first = crop[0] * STEPS_PER_BAR if crop else 0

    voices = []
    for part in parts:
        notes = []
        for s, e, _, p, _ in chosen(part):
            a, b = to_step(s) + shift - first, to_step(e) + shift - first
            if a < 0:
                continue
            if "snap" in part:
                # Halves round up (Python's round() would go to even and
                # pull every other offbeat backwards).
                moved = int(a / part["snap"] + 0.5) * part["snap"] - a
                a, b = a + moved, b + moved
            notes.append((a, max(b, a + 1), p))
        if "bars" in part:
            low_bar, high_bar = part["bars"]
            notes = [n for n in notes if low_bar * STEPS_PER_BAR <= n[0] < (high_bar + 1) * STEPS_PER_BAR]
        pick = part.get("pick", "high")
        if pick == "drums":
            line = drum_line(notes, part["drums"])
        elif pick == "chords":
            line = chord_line(notes, part["chords"], part.get("octave", 0), part.get("fill_gaps", 0))
        else:
            line = mono_line(notes, pick, part.get("legato", False), part.get("fill_gaps", 0))
            if "fold" in part:
                low, high = part["fold"]
                shift_by = 12 * part.get("octave", 0)

                def folded(p):
                    q = p + shift_by
                    while q > high:
                        q -= 12
                    while q < low:
                        q += 12
                    return q - shift_by
                line = [(s, e, folded(p)) for s, e, p in line]
        voices.append((part, line))
    end = max((l[-1][1] for _, l in voices if l), default=STEPS_PER_BAR)
    # Whole bars; a tail of one 16th into the next bar is cut, not padded
    # with a bar of silence before the song starts over.
    total = max(1, -(-(end - 1) // STEPS_PER_BAR)) * STEPS_PER_BAR
    if crop:
        total = (crop[1] - crop[0] + 1) * STEPS_PER_BAR
        voices = [(part, [(s, min(e, total), w) for s, e, w in line if s < total]) for part, line in voices]
    voices = [(part, delayed(line, part["delay"], total) if part.get("delay") else line) for part, line in voices]

    tempo = spec.get("tempo", round(bpm))
    out = [f"; {spec.get('title', 'Imported')}: made by tools/midi2mml.py from {spec_path.name};",
           "; run it again after changing the import file (this file is overwritten).",
           f"; measured {bpm:.2f} BPM" + (f", notes {err:.0%} of a 16th off the grid on average" if err is not None else ""),
           "", f"#title {spec.get('title', 'Imported')}", f"#tempo {tempo}", ""]
    width = max(len(n) for n in spec.get("instruments", {"x": 0}))
    for name, text in spec.get("instruments", {}).items():
        out.append(f"@{name.ljust(width)} {text}")
    out.append("")
    for part, line in voices:
        instrument = None if part.get("pick") == "chords" else part.get("instrument", "default")
        rows = write_channel(line, total, part.get("pick") == "drums", instrument,
                             part.get("volume", 12), part.get("pan", 0), part.get("octave", 0), part.get("gate", 7))
        out.append(f"; {part['channel']}: {part['file']} channels {part.get('midi_channels', 'all')}, {part.get('pick', 'high')}, {len(line)} notes")
        out += [f"{part['channel']}: {row}" for row in rows if row]
        out.append("")
    target = base / spec.get("out", spec_path.stem.replace(".import", "") + ".mml")
    target.write_text("\n".join(out))
    print(f"wrote {target}: {total // STEPS_PER_BAR} bars at {bpm:.2f} BPM (tempo {tempo}), bars start "
          f"{shift} 16ths in" + (f", bars {crop[0]}..{crop[1]} of the original" if crop else "")
          + (f", grid error {err:.0%}" if err is not None else ""))
    for part, line in voices:
        print(f"  {part['channel']}: {len(line)} notes from {part['file']}")


if __name__ == "__main__":
    # A file that is not there, or an import file that is no TOML, is a
    # line saying so, not a traceback.
    try:
        main()
    except FileNotFoundError as e:
        raise SystemExit(f"midi2mml.py: {e.filename}: no such file")
    except tomllib.TOMLDecodeError as e:
        raise SystemExit(f"midi2mml.py: {sys.argv[1]}: {e}")

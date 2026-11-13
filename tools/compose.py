#!/usr/bin/env python3
"""Makes a song from chord progressions: background music for a game, as
MML (src/audio/music.rs). A recipe (NAME.compose.toml) names the style,
the key, the tempo, the moods or the progressions and a seed; the same
recipe always makes the same song. Plain Python.

Music meant to be heard for long without wearing on the ears: the melody
plays in some sections only, the chords are soft pads of three voices
(the C64's trilling arpeggio stays a quiet accent in dark and action
songs), every voice is low-passed, pulse waves keep duty 0.5 (others carry
a DC offset), and the texture changes every 8 bars over two to three
minutes before the song starts over.

The progressions come from free-midi-chords by Ludovic Drolez (MIT,
tools/progressions.toml).

recipe:

    title = "Crates at Dawn"
    out = "crates_at_dawn.mml"        # beside the recipe; a list writes copies
    style = "calm"                    # calm, mysterious, dark, action, bright, space
    key = "f"                         # the tonic: c c+ d ... b (c# and db too)
    tempo = 84
    seed = 1
    moods = ["Peaceful", "Tender"]    # progressions tagged so ...
    scale = "major"                   # ... in this list (major, minor, modal)
    # progressions = ["I vi IV V", "IV I V vi"]   # or these, over `scale`
    melody = "some"                   # none, some (the themes), most
    variation = "some"                # none (unset), some, most: see below
    adaptive = true                   # layers by the game's tension: see below

Variation over time, against fatigue within a section (Strudel's and
TidalCycles' lessons: a rule that runs, not notes written out). `some`:
every fourth bar the bass walks to the next chord and the drums play a
soft pickup, the broken chords start a beat later each bar (the action
style's stabs syncopate every fourth bar instead), and from the third
theme on its second phrase answers the first backwards, rhythm and
steps. `most` adds soft ghost notes on the hi-hat, drawn from the seed.
Its numbers are its own: the song under it is the one the seed makes
without it.

Adaptive (`adaptive = true`): every section has every part and the song
says which sound when (`#layer`): the pads and the bass always, the
drums from a tension of 0.3, the broken chords from 0.55, the melody
(in every theme and bridge) from 0.8. A game sets its tension
(`music.intensity`) and the layers come and go over a bar.

usage: python3 tools/compose.py music/crates_at_dawn.compose.toml [...]
       python3 tools/compose.py --find Dark Fearful    (lists progressions)
"""

import random
import re
import sys
import tomllib
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
NAMES = ["c", "c+", "d", "d+", "e", "f", "f+", "g", "g+", "a", "a+", "b"]
CHORD_NAMES = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"]
BAR = 16  # steps of a sixteenth note
SECTION = 8  # bars
MAJOR = [0, 2, 4, 5, 7, 9, 11]
MINOR = [0, 2, 3, 5, 7, 8, 10]
DEGREE = {"i": 0, "ii": 1, "iii": 2, "iv": 3, "v": 4, "vi": 5, "vii": 6}


# --- chords -------------------------------------------------------------------

def tonic(name):
    """A key's tonic in semitones over C: c, c+ or c#, d- or db ..."""
    m = re.fullmatch(r"([a-g])([+#-]|b)?", name.strip().lower())
    if not m:
        raise SystemExit(f"key = {name!r}: a note from c to b, with + # - or b")
    shift = {"": 0, None: 0, "+": 1, "#": 1, "-": -1, "b": -1}[m.group(2)]
    return (NAMES.index(m.group(1)) + shift) % 12


def chord(token, scale):
    """A chord written as free-midi-chords writes it (I, vi, bIIM7, ivm9,
    I/bVII ...): its root and bass in semitones over the tonic and its
    tones in semitones over the root. Upper case is major, lower case
    minor; a lower case numeral the scale makes diminished stays so."""
    head, _, over = token.partition("/")
    m = re.fullmatch(r"([b#]?)(vii|iii|ii|iv|vi|v|i)(.*)", head, re.I)
    if not m:
        raise SystemExit(f"{token}: not a chord (a Roman numeral like I, vi, bVII)")
    accidental, numeral, suffix = m.groups()
    steps = MINOR if scale == "minor" else MAJOR
    degree = DEGREE[numeral.lower()]
    root = (steps[degree] + {"": 0, "b": -1, "#": 1}[accidental]) % 12
    diatonic = not accidental

    def above(n):
        """The scale's note n degrees over the root, in semitones from it."""
        return (steps[(degree + n) % 7] - steps[degree]) % 12

    third = 4 if numeral.isupper() else 3
    fifth = 7
    if not suffix and diatonic and numeral.islower() and scale != "modal" and above(4) == 6:
        fifth = 6
    seventh = above(6) if diatonic else 10
    rest = suffix
    if rest.startswith("dim"):
        third, fifth, seventh, rest = 3, 6, 10, rest[3:]
    elif rest.startswith("M-5"):
        third, fifth, rest = 4, 6, rest[3:]
    elif rest.startswith("dom7"):
        third, seventh, rest = 4, 10, "7" + rest[4:]
    elif rest.startswith("M"):
        third, seventh, rest = 4, 11, rest[1:]
    elif rest.startswith("m"):
        third, seventh, rest = 3, 10, rest[1:]
    tones = {
        "": [0, third, fifth],
        "7": [0, third, fifth, seventh],
        "6": [0, third, fifth, 9],
        "69": [0, third, fifth, 9, 14],
        "9": [0, third, fifth, seventh, 14],
        "add9": [0, third, fifth, 14],
        "sus2": [0, 2, fifth],
        "sus4": [0, 5, fifth],
        "5": [0, 7],
    }.get(rest)
    if tones is None:
        raise SystemExit(f"{token}: tools/compose.py does not know the chord '{suffix}'")
    bass = chord(over, scale)["root"] if over else root
    return {"token": token, "root": root, "tones": tones, "bass": bass}


def chord_name(c, key):
    """A chord as a lead sheet names it: F, Dm, Bdim, Cmaj7, Gsus4 ..."""
    root = CHORD_NAMES[(key + c["root"]) % 12]
    t = c["tones"]
    kind = {
        (0, 4, 7): "", (0, 3, 7): "m", (0, 3, 6): "dim", (0, 4, 6): "(b5)", (0, 2, 7): "sus2",
        (0, 5, 7): "sus4", (0, 7): "5", (0, 4, 7, 10): "7", (0, 4, 7, 11): "maj7",
        (0, 3, 7, 10): "m7", (0, 3, 6, 10): "m7b5", (0, 4, 7, 9): "6", (0, 3, 7, 9): "m6",
        (0, 4, 7, 14): "add9", (0, 3, 7, 14): "madd9", (0, 4, 7, 11, 14): "maj9",
        (0, 4, 7, 10, 14): "9", (0, 3, 7, 10, 14): "m9", (0, 4, 7, 9, 14): "6/9",
    }.get(tuple(t), "?")
    bass = "" if c["bass"] == c["root"] else "/" + CHORD_NAMES[(key + c["bass"]) % 12]
    return root + kind + bass


def progressions():
    """free-midi-chords' progressions: (scale, chords, moods)."""
    data = tomllib.loads((TOOLS / "progressions.toml").read_text())
    out = []
    for scale in ("major", "minor", "modal"):
        for line in data[scale]:
            chords, _, moods = line.partition("=")
            out.append((scale, chords.split(), moods.split()))
    return out


def pick(recipe, rng):
    """Two progressions (the A and B sections'): the recipe's own, or two
    from the library with one of its moods."""
    scale = recipe.get("scale")
    if "progressions" in recipe:
        chosen = [(scale or "major", p.split()) for p in recipe["progressions"]]
    else:
        moods = set(recipe.get("moods", []))
        if not moods:
            raise SystemExit("the recipe needs moods (or progressions)")
        found = [(s, c) for s, c, m in progressions()
                 if moods & set(m) and "Cadence" not in m and 3 <= len(c) <= 8 and scale in (None, s)]
        if not found:
            raise SystemExit(f"no progression is tagged {sorted(moods)}" + (f" in {scale}" if scale else ""))
        chosen = rng.sample(found, min(2, len(found)))
    if len(chosen) == 1:
        chosen.append(chosen[0])
    return [(s, [chord(t, s) for t in c]) for s, c in chosen[:2]]


# --- styles -------------------------------------------------------------------

STYLES = {
    # Soft and slow: triangles, no drums, chords changing every two bars.
    "calm": {
        "instruments": """\
@lead   triangle attack=0.02 decay=0.4 sustain=0.55 release=0.35 vibrato=0.008 vibrato_speed=5 vibrato_delay=0.3 volume=0.45
@echo   triangle attack=0.02 decay=0.4 sustain=0.5 release=0.4 lowpass=1400 volume=0.45
@pad    triangle attack=0.5 decay=0.8 sustain=0.75 release=0.9 lowpass=1600 volume=0.16
@harp   triangle attack=0.005 decay=0.5 sustain=0.2 release=0.4 lowpass=2200 volume=0.2
@bass   triangle attack=0.01 decay=0.3 sustain=0.7 release=0.2 volume=0.6""",
        "accomp": "harp", "bass": {"sparse": [(0, 16, 0)], "normal": [(0, 8, 0), (8, 8, 7)]},
        "bass_range": (38, 49), "drums": {}, "drone": False, "slow_chords": True,
        "melody": (60, 79, 67), "echo": 4,
        "rhythms": [[4, 4, 8, 8, 4, -4], [8, 4, 4, 12, -4], [6, 2, 8, 4, 4, 8], [4, 4, 4, 4, 12, -4], [-4, 4, 4, 4, 8, 8]],
    },
    # Hushed and modal: a filtered pulse lead, a dark saw pad, a ticking clock.
    "mysterious": {
        "instruments": """\
@lead   square duty=0.5 attack=0.03 decay=0.4 sustain=0.5 release=0.4 lowpass=1300 vibrato=0.01 vibrato_speed=4.5 vibrato_delay=0.35 volume=0.32
@echo   square duty=0.5 attack=0.03 decay=0.4 sustain=0.45 release=0.5 lowpass=900 volume=0.32
@pad    saw attack=0.8 decay=1.0 sustain=0.7 release=1.0 lowpass=700 volume=0.13
@harp   triangle attack=0.005 decay=0.6 sustain=0.15 release=0.5 lowpass=1800 volume=0.18
@bass   triangle attack=0.01 decay=0.35 sustain=0.65 release=0.25 volume=0.6
@kick   triangle freq=90 sweep=-24 sweep_time=0.1 attack=0.001 decay=0.18 sustain=0 release=0.02 volume=0.55
@tick   noise freq=9000 attack=0.001 decay=0.03 sustain=0 release=0.01 lowpass=5000 volume=0.07
@rim    noise freq=4000 attack=0.001 decay=0.08 sustain=0 release=0.02 lowpass=3000 volume=0.16""",
        "accomp": "harp", "bass": {"sparse": [(0, 16, 0)], "normal": [(0, 6, 0), (6, 6, 0), (12, 4, -5)]},
        "bass_range": (36, 47), "drone": False, "slow_chords": True,
        "drums": {1: [(0, "kick"), (4, "tick"), (8, "tick"), (12, "tick")],
                  2: [(0, "kick"), (4, "tick"), (8, "rim"), (10, "kick"), (12, "tick")]},
        "melody": (57, 76, 66), "echo": 4,
        "rhythms": [[6, 2, 4, 4, 8, -8], [4, 6, 2, 4, 12, -4], [-4, 4, 4, 4, 6, 2, 8], [2, 2, 4, 8, 4, 4, -8]],
    },
    # More Blood's march: a pulsing bass with the flat second, a drone,
    # a military snare.
    "dark": {
        "instruments": """\
@lead   square duty=0.5 attack=0.01 decay=0.3 sustain=0.55 release=0.25 lowpass=1600 vibrato=0.012 vibrato_speed=5.5 vibrato_delay=0.2 volume=0.3
@echo   square duty=0.5 attack=0.01 decay=0.3 sustain=0.5 release=0.3 lowpass=1000 volume=0.3
@pad    saw attack=0.6 decay=0.8 sustain=0.7 release=0.9 lowpass=600 volume=0.12
@bass   square duty=0.5 attack=0.002 decay=0.12 sustain=0.6 release=0.05 lowpass=1100 volume=0.45
@drone  saw attack=0.6 decay=0.5 sustain=0.8 release=0.8 lowpass=420 vibrato=0.004 vibrato_speed=0.5 volume=0.3
@kick   triangle freq=120 sweep=-30 sweep_time=0.09 attack=0.001 decay=0.12 sustain=0 release=0.01 volume=0.85
@snare  noise freq=5500 attack=0.001 decay=0.13 sustain=0 release=0.02 lowpass=4500 volume=0.34
@roll   noise freq=5000 attack=0.001 decay=0.06 sustain=0 release=0.01 lowpass=4500 volume=0.22
@hat    noise freq=14000 attack=0.001 decay=0.03 sustain=0 release=0.01 lowpass=6000 volume=0.1""",
        "accomp": "arp", "arp": "attack=0.01 decay=0.4 sustain=0.45 release=0.3 lowpass=2000 arp_speed=50 volume=0.14",
        "arp_rhythm": "held",
        "bass": {"sparse": [(0, 4, 0), (4, 4, 0), (8, 4, 0), (12, 4, 0)],
                 "normal": [(2 * k, 2, n) for k, n in enumerate([0, 0, 12, 0, 1, 0, -2, 0])]},
        "bass_range": (36, 47), "drone": True, "slow_chords": False,
        "drums": {1: [(0, "kick")] + [(s, "hat") for s in range(4, 16, 2)],
                  2: [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (8, "kick"), (10, "kick"), (12, "snare"), (14, "hat")],
                  "fill": [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (8, "roll"), (9, "roll"), (10, "roll"), (11, "roll"), (12, "snare"), (14, "snare")]},
        "melody": (55, 74, 64), "echo": 3,
        "rhythms": [[6, 2, 4, 4, 6, 2, 8], [4, 2, 2, 4, 4, 8, 4, -4], [2, 2, 4, 2, 2, 4, 12, -4], [6, 6, 4, 4, 4, 8]],
    },
    # Fast and driving: octave bass, offbeat chord stabs, a pumping kick.
    "action": {
        "instruments": """\
@lead   square duty=0.5 attack=0.005 decay=0.2 sustain=0.6 release=0.15 lowpass=2200 vibrato=0.01 vibrato_speed=6 vibrato_delay=0.15 volume=0.3
@echo   square duty=0.5 attack=0.005 decay=0.2 sustain=0.55 release=0.2 lowpass=1300 volume=0.3
@pad    saw attack=0.3 decay=0.6 sustain=0.6 release=0.6 lowpass=900 volume=0.1
@bass   saw attack=0.002 decay=0.15 sustain=0.5 release=0.05 cutoff=700 cutoff_end=260 cutoff_time=0.12 resonance=0.3 volume=0.42
@kick   triangle freq=130 sweep=-34 sweep_time=0.08 attack=0.001 decay=0.11 sustain=0 release=0.01 volume=0.85 sidechain=0.25
@snare  noise freq=6000 attack=0.001 decay=0.12 sustain=0 release=0.02 lowpass=5000 volume=0.34 duck=0
@roll   noise freq=5500 attack=0.001 decay=0.05 sustain=0 release=0.01 lowpass=5000 volume=0.22 duck=0
@hat    noise freq=14000 attack=0.001 decay=0.025 sustain=0 release=0.01 lowpass=6500 volume=0.09 duck=0""",
        "accomp": "arp", "arp": "attack=0.002 decay=0.15 sustain=0.3 release=0.1 lowpass=2400 arp_speed=60 volume=0.13",
        "arp_rhythm": "stabs",
        "bass": {"sparse": [(2 * k, 2, 0) for k in range(8)],
                 "normal": [(2 * k, 2, n) for k, n in enumerate([0, 12, 0, 12, 0, 12, 7, 12])]},
        "bass_range": (33, 44), "drone": False, "slow_chords": False,
        "drums": {1: [(0, "kick"), (2, "hat"), (4, "kick"), (6, "hat"), (8, "kick"), (10, "hat"), (12, "kick"), (14, "hat")],
                  2: [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (7, "kick"), (8, "kick"), (10, "hat"), (12, "snare"), (14, "hat"), (15, "hat")],
                  "fill": [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (8, "roll"), (9, "roll"), (10, "roll"), (11, "roll"), (12, "roll"), (13, "roll"), (14, "snare"), (15, "snare")]},
        "melody": (60, 81, 69), "echo": 3,
        "rhythms": [[2, 2, 4, 2, 2, 4, 4, 4, 8], [4, 2, 2, 4, 4, 2, 2, 4, 8], [2, 2, 2, 2, 4, 4, 6, 2, 8], [4, 4, 2, 2, 4, 8, -8]],
    },
    # Space: a filtered saw bass pulsing in eighths, a square broken into
    # sixteenths, a soft saw pad, four on the floor with a pumping kick.
    "space": {
        "instruments": """\
@lead   square duty=0.5 attack=0.01 decay=0.3 sustain=0.55 release=0.25 lowpass=2600 vibrato=0.01 vibrato_speed=5.5 vibrato_delay=0.25 volume=0.27
@echo   square duty=0.5 attack=0.01 decay=0.3 sustain=0.5 release=0.3 lowpass=1500 volume=0.27
@pad    saw attack=0.5 decay=0.8 sustain=0.7 release=0.9 lowpass=700 volume=0.12
@harp   square duty=0.5 attack=0.002 decay=0.16 sustain=0.1 release=0.1 lowpass=2100 volume=0.12
@bass   saw attack=0.002 decay=0.18 sustain=0.45 release=0.06 cutoff=900 cutoff_end=280 cutoff_time=0.15 resonance=0.35 volume=0.4
@kick   triangle freq=120 sweep=-30 sweep_time=0.08 attack=0.001 decay=0.13 sustain=0 release=0.01 volume=0.8 sidechain=0.2
@snare  noise freq=5000 attack=0.001 decay=0.15 sustain=0 release=0.02 lowpass=4500 volume=0.28 duck=0
@roll   noise freq=5000 attack=0.001 decay=0.05 sustain=0 release=0.01 lowpass=4500 volume=0.18 duck=0
@hat    noise freq=13000 attack=0.001 decay=0.02 sustain=0 release=0.01 lowpass=7000 volume=0.07 duck=0""",
        "accomp": "harp", "harp_step": 1,
        "bass": {"sparse": [(0, 8, 0), (8, 8, 0)],
                 "normal": [(2 * k, 2, n) for k, n in enumerate([0, 0, 12, 0, 0, 0, 12, 7])]},
        "bass_range": (36, 47), "drone": False, "slow_chords": False,
        "drums": {1: [(0, "kick"), (2, "hat"), (4, "kick"), (6, "hat"), (8, "kick"), (10, "hat"), (12, "kick"), (14, "hat")],
                  2: [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (8, "kick"), (10, "hat"), (12, "snare"), (14, "hat")],
                  "fill": [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (8, "roll"), (9, "roll"), (10, "roll"), (11, "roll"), (12, "snare"), (14, "snare")]},
        "melody": (60, 79, 67), "echo": 3,
        "rhythms": [[4, 2, 2, 4, 4, 8, -8], [2, 2, 4, 4, 4, 4, 12], [6, 2, 4, 4, 8, 8], [4, 4, 2, 2, 4, 8, -8]],
    },
    # Cheerful and bouncy, a sunny jump & run: a soft pulse lead, a
    # bouncing triangle bass (root, octave, fifth), broken chords, light
    # drums.
    "bright": {
        "instruments": """\
@lead   square duty=0.5 attack=0.005 decay=0.25 sustain=0.5 release=0.2 lowpass=2400 vibrato=0.008 vibrato_speed=6 vibrato_delay=0.2 volume=0.27
@echo   square duty=0.5 attack=0.005 decay=0.25 sustain=0.45 release=0.25 lowpass=1400 volume=0.27
@pad    triangle attack=0.2 decay=0.6 sustain=0.6 release=0.5 lowpass=1800 volume=0.14
@harp   triangle attack=0.003 decay=0.3 sustain=0.15 release=0.25 lowpass=2600 volume=0.2
@bass   triangle attack=0.005 decay=0.2 sustain=0.6 release=0.1 volume=0.6
@kick   triangle freq=110 sweep=-28 sweep_time=0.08 attack=0.001 decay=0.12 sustain=0 release=0.01 volume=0.6
@snare  noise freq=6000 attack=0.001 decay=0.08 sustain=0 release=0.02 lowpass=5000 volume=0.2
@roll   noise freq=5500 attack=0.001 decay=0.05 sustain=0 release=0.01 lowpass=5000 volume=0.15
@hat    noise freq=13000 attack=0.001 decay=0.025 sustain=0 release=0.01 lowpass=6500 volume=0.07""",
        "accomp": "harp", "bass": {"sparse": [(0, 8, 0), (8, 8, 7)], "normal": [(0, 4, 0), (4, 4, 12), (8, 4, 7), (12, 4, 12)]},
        "bass_range": (36, 48), "drone": False, "slow_chords": False,
        "drums": {1: [(0, "kick"), (4, "hat"), (8, "kick"), (12, "hat")],
                  2: [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (8, "kick"), (10, "hat"), (12, "snare"), (14, "hat")],
                  "fill": [(0, "kick"), (2, "hat"), (4, "snare"), (6, "hat"), (8, "kick"), (10, "roll"), (11, "roll"), (12, "snare"), (14, "snare")]},
        "melody": (62, 81, 71), "echo": 3,
        "rhythms": [[4, 2, 2, 4, 4, 8, -8], [2, 2, 4, 4, 4, 4, 12], [4, 4, 2, 2, 4, 4, 8, 4], [6, 2, 4, 4, 8, 8]],
    },
}

# What each kind of section plays: which progression, the pad's volume,
# the bass's pattern, the accompaniment, the drums (0 none, 1 light,
# 2 full), a fill in its last bar.
SECTIONS = {
    "intro": {"prog": 0, "pad": 8, "bass": "sparse", "accomp": False, "drums": 1, "fill": False},
    "theme": {"prog": 0, "pad": 10, "bass": "normal", "accomp": False, "drums": 2, "fill": True},
    "bridge": {"prog": 1, "pad": 10, "bass": "normal", "accomp": True, "drums": 2, "fill": True},
    "break": {"prog": 0, "pad": 11, "bass": "sparse", "accomp": False, "drums": 0, "fill": False},
    "turn": {"prog": 1, "pad": 9, "bass": "normal", "accomp": True, "drums": 1, "fill": True},
}


def form(tempo):
    """The sections, 8 bars each: two to three minutes before it starts over."""
    sections = ["intro", "theme", "theme", "bridge", "bridge", "break", "theme", "turn"]
    seconds = SECTION * 4 * 60 / tempo
    while len(sections) * seconds < 130:
        sections[-1:-1] = ["bridge", "theme"]
    return sections


def fit(prog, slow):
    """A progression's chords over 8 bars: each a bar (two in slow songs
    when it is short), a cycle filled up to 2, 4 or 8 bars by holding its
    last chords longer, repeated."""
    per = 2 if slow and len(prog) <= 4 else 1
    durations = [per] * len(prog)
    cycle = next(n for n in (1, 2, 4, 8) if n >= sum(durations))
    k = len(durations) - 1
    while sum(durations) < cycle:
        durations[k] += 1
        k = k - 1 if k > 0 else len(durations) - 1
    return [(c, d) for _ in range(SECTION // cycle) for c, d in zip(prog, durations)]


# --- the parts ----------------------------------------------------------------

def pitches(pcs, lo, hi):
    return [p for p in range(lo, hi + 1) if p % 12 in pcs]


def voicing(c, key, previous):
    """Three pad voices for a chord, moving as little as they can from the
    last: the root left to the bass when there are four tones or more."""
    tones = [(key + c["root"] + t) % 12 for t in c["tones"]]
    if len(tones) == 2:
        tones = [tones[0], tones[1], tones[0]]
    elif len(tones) >= 4:
        tones = tones[1:]
        if len(tones) > 3:
            del tones[1]
    best, cost = None, None
    options = [pitches({pc}, 52, 77) for pc in tones]
    for a in options[0]:
        for b in options[1]:
            for d in options[2]:
                v = sorted((a, b, d))
                if len(set(v)) < 3 or v[2] - v[0] > 15:
                    continue
                if previous:
                    c_ = sum(abs(x - y) for x, y in zip(v, previous))
                else:
                    c_ = abs(sum(v) / 3 - 64)
                c_ += 0.1 * abs(sum(v) / 3 - 64)
                if cost is None or c_ < cost:
                    best, cost = v, c_
    return best


def local_scale(c, key, base):
    """The notes a melody may use over a chord: the chord's own, and the
    key's scale but for a note a semitone from one of them (nothing rubs:
    over the tonic chord a major key gives its pentatonic)."""
    tones = {(key + c["root"] + t) % 12 for t in c["tones"]}
    pcs = {(key + s) % 12 for s in base}
    return tones | {p for p in pcs if (p + 1) % 12 not in tones and (p - 1) % 12 not in tones}


def step(p, moves, scale, lo, hi):
    """`moves` steps along the scale from p (first onto it)."""
    ladder = pitches(scale, lo - 12, hi + 12)
    i = min(range(len(ladder)), key=lambda k: abs(ladder[k] - p))
    return ladder[max(0, min(len(ladder) - 1, i + moves))]


def nearest(p, pcs, lo, hi):
    return min(pitches(pcs, lo, hi), key=lambda q: (abs(q - p), q))


def toward(p, pcs, lo, hi, way):
    """The nearest of pcs from p on in the way the melody moves."""
    near = pitches(pcs, lo, hi)
    if way > 0:
        return min((q for q in near if q >= p), default=max(near))
    if way < 0:
        return max((q for q in near if q <= p), default=min(near))
    return nearest(p, pcs, lo, hi)


class Song:
    def __init__(self, recipe):
        self.recipe = recipe
        self.style = STYLES.get(recipe.get("style", "calm"))
        if self.style is None:
            raise SystemExit(f"style = {recipe.get('style')!r}: calm, mysterious, dark, action, bright or space")
        self.rng = random.Random(recipe.get("seed", 1))
        self.key = tonic(recipe.get("key", "c"))
        self.tempo = recipe.get("tempo", 100)
        self.melody_in = recipe.get("melody", "some")
        if self.melody_in not in ("none", "some", "most"):
            raise SystemExit(f"melody = {self.melody_in!r}: none, some or most")
        self.vary = recipe.get("variation", "none")
        if self.vary not in ("none", "some", "most"):
            raise SystemExit(f"variation = {self.vary!r}: none, some or most")
        self.adaptive = recipe.get("adaptive", False) is True
        # The variation draws from numbers of its own, so the song under it
        # stays the one the seed makes without it.
        self.vary_rng = random.Random(f"{recipe.get('seed', 1)} variation")
        self.progs = pick(recipe, self.rng)
        self.sections = form(self.tempo)
        self.total = len(self.sections) * SECTION * BAR
        # The chords in time: (start, end, chord, section index).
        self.chords = []
        t = 0
        for n, kind in enumerate(self.sections):
            scale, prog = self.progs[SECTIONS[kind]["prog"]]
            for c, bars in fit(prog, self.style["slow_chords"]):
                self.chords.append((t, t + bars * BAR, c, n))
                t += bars * BAR
        # The melody's scale: the progression's; a modal one's is the minor
        # when its tonic chord is minor.
        first_scale, first = self.progs[0]
        home = next((c for c in first if c["root"] == 0), first[0])
        modal_minor = first_scale == "modal" and home["tones"][1] == 3
        self.base = MINOR if first_scale == "minor" or modal_minor else MAJOR

    def chord_at(self, t):
        return next(c for s, e, c, _ in self.chords if s <= t < e)

    def section_span(self, n):
        return n * SECTION * BAR, (n + 1) * SECTION * BAR

    def pad(self):
        voices = [[], [], []]
        previous = None
        for s, e, c, n in self.chords:
            v = voicing(c, self.key, previous)
            previous = v
            level = SECTIONS[self.sections[n]]["pad"]
            for k in range(3):
                line = voices[k]
                # A note the next chord keeps rings on.
                if line and line[-1][1] == s and line[-1][2] == v[k] and line[-1][4] == level:
                    line[-1] = (line[-1][0], e, v[k], None, level)
                else:
                    line.append((s, e, v[k], None, level))
        return voices

    def bass(self):
        lo, hi = self.style["bass_range"]
        line = []
        for s, e, c, n in self.chords:
            root = nearest((lo + hi) // 2, {(self.key + c["bass"]) % 12}, lo, hi)
            kind = SECTIONS[self.sections[n]]["bass"]
            pattern = self.style["bass"][kind]
            for bar in range(s, e, BAR):
                notes = [(bar + at, bar + at + length, root + interval) for at, length, interval in pattern]
                if self.vary != "none" and kind == "normal" and (bar // BAR) % 4 == 3:
                    notes = self.walk(notes, bar, root, pattern)
                line += [(a, b, p, None, None) for a, b, p in notes]
        return line

    def walk(self, notes, bar, root, pattern):
        """The last beat of a bar walks to the next bar's bass note: a step
        of the key's scale under it (the dark style creeps up by
        semitones), in eighths where the pattern moves in eighths."""
        lo, hi = self.style["bass_range"]
        after = self.chord_at((bar + BAR) % self.total)
        target = nearest(root, {(self.key + after["bass"]) % 12}, lo - 2, hi)
        scale = {(self.key + d) % 12 for d in self.base}
        if self.style is STYLES["dark"]:
            steps = [target - 2, target - 1]
        else:
            below = step(target - 1, 0, scale, lo - 12, hi)
            below = below if below < target else step(target, -1, scale, lo - 12, hi)
            steps = [step(below, -1, scale, lo - 12, hi), below]
        beat = bar + 12
        kept = [(a, min(b, beat), p) for a, b, p in notes if a < beat]
        if min(length for _, length, _ in pattern) <= 2:
            return kept + [(beat, beat + 2, steps[0]), (beat + 2, beat + 4, steps[1])]
        return kept + [(beat, beat + 4, steps[1])]

    def drone(self):
        p = nearest(38, {self.key}, 33, 44)
        return [(s, e, p, None, None) for s, e in (self.section_span(n) for n in range(len(self.sections)))]

    def section(self, kind):
        """What a section plays; adaptive, every part (the tension picks)."""
        sec = SECTIONS[kind]
        if not self.adaptive:
            return sec
        return {**sec, "accomp": True, "drums": 1 if kind in ("intro", "break") else 2}

    def accompaniment(self):
        """Calm and mysterious songs break their chords into slow eighths
        (harp); dark and action ones play them as the C64 did, one voice
        trilling through the chord (arp), quietly."""
        line, kinds = [], {}
        for s, e, c, n in self.chords:
            if not self.section(self.sections[n])["accomp"]:
                continue
            if self.style["accomp"] == "harp":
                tones = sorted(pitches({(self.key + c["root"] + t) % 12 for t in c["tones"][:3]}, 55, 70))[:4]
                order = [0, 1, 2, 1, 0, 1, 2, 3 if len(tones) > 3 else 1]
                # Eighths, or sixteenths in a style that says so.
                step = self.style.get("harp_step", 2)
                order = order * (2 // step)
                for bar in range(s, e, BAR):
                    # Varied, the figure starts a beat later each bar, round
                    # in four (Tidal's iter).
                    turn = (4 // step) * ((bar // BAR) % 4) if self.vary != "none" else 0
                    for k, i in enumerate(order[turn:] + order[:turn]):
                        line.append((bar + step * k, bar + step * k + step, tones[min(i, len(tones) - 1)], "harp", None))
            else:
                arp = tuple(sorted({t % 12 for t in c["tones"]}))
                name = kinds.setdefault(arp, f"arp{len(kinds) + 1}")
                root = nearest(57, {(self.key + c["root"]) % 12}, 52, 63)
                if self.style["arp_rhythm"] == "held":
                    line.append((s, e, root, name, None))
                else:
                    for bar in range(s, e, BAR):
                        # Varied, every fourth bar's stabs push ahead.
                        stabs = (2, 6, 11, 14) if self.vary != "none" and (bar // BAR) % 4 == 3 else (2, 6, 10, 14)
                        for at in stabs:
                            line.append((bar + at, bar + at + 2, root, name, None))
        return line, kinds

    def drums(self):
        """The drums, a section's pattern bar by bar and a fill in its last.
        Varied, every fourth bar of the full pattern ends on a soft pickup,
        and with `most` some bars get a soft ghost note on the hi-hat."""
        pats = self.style["drums"]
        vary = self.vary != "none"
        loud, soft = (12, 6) if vary else (None, None)
        ghost = next((d for d in ("hat", "tick") if f"@{d} " in self.style["instruments"]), None)
        pickup = next((d for d in ("roll", "snare", "rim") if f"@{d} " in self.style["instruments"]), None)
        line = []
        for n, kind in enumerate(self.sections):
            sec = self.section(kind)
            level = sec["drums"]
            if not level or level not in pats:
                continue
            start, end = self.section_span(n)
            for bar in range(start, end, BAR):
                last = bar + BAR == end
                filled = last and sec["fill"] and "fill" in pats
                pattern = pats["fill"] if filled else pats[level]
                hits = [(at, drum, loud) for at, drum in pattern]
                if vary and not filled:
                    taken = {at for at, _, _ in hits}
                    if level == 2 and pickup and (bar // BAR) % 4 == 3 and 15 not in taken:
                        hits.append((15, pickup, soft))
                    if self.vary == "most" and ghost:
                        free = [at for at in range(1, BAR, 2) if at not in taken and at != 15]
                        if free and self.vary_rng.random() < 0.4:
                            hits.append((self.vary_rng.choice(free), ghost, soft))
                line += [(bar + at, drum, vol) for at, drum, vol in hits]
        line.sort(key=lambda h: (h[0], h[1]))
        return [(s, line[k + 1][0] if k + 1 < len(line) and line[k + 1][0] < (s // BAR + 1) * BAR else (s // BAR + 1) * BAR, d, None, v)
                for k, (s, d, v) in enumerate(line)]

    def tune(self):
        """The melody's material: two rhythms of two bars and a contour of
        scale steps for each, made once and played in every theme."""
        rhythms = self.rng.sample(self.style["rhythms"], 2)

        def contour(rhythm):
            """An arch: up in the first half, down in the second, by
            steps mostly and a third now and then."""
            n = sum(1 for d in rhythm if d > 0)
            moves = [0]
            for k in range(1, n):
                moves.append(self.rng.choice([1, 1, 2] if k < n / 2 else [-1, -1, -2]))
            return moves

        return [(r, contour(r)) for r in rhythms]

    def phrase(self, rhythm, moves, start, previous, lift, ending):
        """Two bars of melody from `start`: the contour walked along the
        scale, the first note of a bar and long ones on the chord; the last
        of a section's phrases comes home to the chord's root."""
        lo, hi, centre = self.style["melody"]
        if ending:
            first_bar, at = [], 0
            for d in rhythm:
                if at >= BAR:
                    break
                first_bar.append(d)
                at += abs(d)
            rhythm = first_bar + [12, -4]
        notes, t, p, i, before = [], start, None, 0, None
        for d in rhythm:
            if d < 0:
                t -= d
                continue
            before = p
            c = self.chord_at(t)
            scale = local_scale(c, self.key, self.base)
            tones = {(self.key + c["root"] + x) % 12 for x in c["tones"]}
            if p is None:
                p = nearest((previous or centre) + 3 * lift, tones, lo, hi)
            else:
                way = moves[i] if i < len(moves) else 0
                p = step(p, way, scale, lo, hi)
                if t % BAR == 0 or d >= 12:
                    p = toward(p, tones, lo, hi, way)
                    if p == before and way:
                        p = toward(p + way, tones, lo, hi, way)
            if (p > hi or p < lo) and before is not None:
                p = step(before, -(moves[i] if i < len(moves) else 0), scale, lo, hi)
            if p > hi or p < lo:
                p = nearest(centre, tones, lo, hi)
            last = ending and i == sum(1 for x in rhythm if x > 0) - 1
            if last:
                p = nearest(p, {(self.key + c["root"]) % 12}, lo, hi)
            notes.append((t, t + d, p, None, None))
            t += d
            i += 1
        return notes

    def melody(self):
        """The themes carry the tune (the second a third higher, its third
        phrase turned upside down); with `most` the second bridge answers."""
        if self.melody_in == "none":
            return []
        (ra, ca), (rb, cb) = self.tune()
        line, themes, bridges = [], 0, 0
        for n, kind in enumerate(self.sections):
            start, _ = self.section_span(n)
            plan = None
            if kind == "theme":
                lift = 1 if themes % 3 == 1 else 0
                # Varied, from the third theme on the second phrase answers
                # the first backwards, rhythm and steps (Tidal's rev).
                answer = (ra[::-1], [0] + [-m for m in reversed(ca[1:])]) if self.vary != "none" and themes >= 2 else (ra, ca)
                plan = [(ra, ca, lift, False), (*answer, lift, False),
                        (rb, cb if lift == 0 else [-m for m in cb], lift, False), (ra, ca, lift, True)]
                themes += 1
            elif kind == "bridge":
                if (self.melody_in == "most" and bridges % 2 == 1) or (self.adaptive and self.melody_in != "none"):
                    plan = [(rb, cb, 0, False), (rb, [-m for m in cb], 0, False), (ra, ca, -1, False), (rb, cb, 0, True)]
                bridges += 1
            if plan is None:
                continue
            previous = None
            for k, (r, moves, lift, ending) in enumerate(plan):
                notes = self.phrase(r, moves, start + 2 * BAR * k, previous, lift, ending)
                if notes:
                    previous = notes[-1][2]
                line += notes
        return line


# --- MML ----------------------------------------------------------------------

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


def channel(letter, events, total, head, drums=False):
    """MML lines for one voice: events (start, end, pitch or drum,
    instrument, volume) in sixteenths; a bar line every bar, four bars a
    line, notes held over a bar line tied across it."""
    tokens = list(head)
    octave = instrument = volume = None
    pos = 0

    def pieces(until):
        nonlocal pos
        while pos < until:
            bar_end = (pos // BAR + 1) * BAR
            span = min(until, bar_end) - pos
            pos += span
            yield span, pos % BAR == 0

    for s, e, what, ins, vol in sorted(events, key=lambda ev: ev[0]):
        if s < pos:
            continue
        for span, bar in pieces(s):
            tokens.extend("r" + n for n in lengths(span))
            if bar:
                tokens.append("|")
        if vol is not None and vol != volume:
            tokens.append(f"v{vol}")
            volume = vol
        if drums:
            if what != instrument:
                tokens.append(f"@{what}")
                instrument = what
            name = "c"
        else:
            if ins is not None and ins != instrument:
                tokens.append(f"@{ins}")
                instrument = ins
            if what // 12 - 1 != octave:
                octave = what // 12 - 1
                tokens.append(f"o{octave}")
            name = NAMES[what % 12]
        spans = list(pieces(min(e, total)))
        for k, (span, bar) in enumerate(spans):
            tokens.append(name + "&".join(lengths(span)) + ("&" if k + 1 < len(spans) else ""))
            if bar:
                tokens.append("|")
    for span, bar in pieces(total):
        tokens.extend("r" + n for n in lengths(span))
        if bar:
            tokens.append("|")
    lines, row, bars = [], [], 0
    for t in tokens:
        if t == "|":
            bars += 1
            if bars % 4 == 0:
                lines.append(f"{letter}: " + " ".join(row))
                row = []
                continue
        row.append(t)
    if row:
        lines.append(f"{letter}: " + " ".join(row))
    return lines


def delayed(line, by, total):
    """`line` played `by` sixteenths late; what runs past the end comes
    round to the start, so the echo of the last notes sounds as the song
    starts over (as tools/midi2mml.py does)."""
    moved = []
    for s, e, *rest in line:
        s, e = s + by, e + by
        if s >= total:
            moved.append((s - total, e - total, *rest))
        else:
            moved.append((s, min(e, total), *rest))
            if e > total:
                moved.append((0, e - total, *rest))
    moved.sort(key=lambda ev: ev[0])
    return [(s, min(e, moved[i + 1][0]) if i + 1 < len(moved) else e, *rest)
            for i, (s, e, *rest) in enumerate(moved) if i + 1 >= len(moved) or moved[i + 1][0] > s]


REPO = Path(__file__).resolve().parent.parent


def named(source):
    """The recipe as the song's first line names it: from the repository's
    root when it lies in it (the same line from any folder), else its
    file's name (never a path of one machine)."""
    path = Path(source).resolve()
    try:
        return path.relative_to(REPO).as_posix()
    except ValueError:
        return path.name


def write(recipe, source, check=False):
    """Makes the song and writes it where the recipe's `out` says; with
    `check`, writes nothing and says whether each file there is the song
    byte for byte (False if one is not)."""
    song = Song(recipe)
    st = song.style
    title = recipe.get("title", Path(source).stem)
    key_name = CHORD_NAMES[song.key] + (" minor" if song.base == MINOR else " major")
    out = [f"; {title}: made by tools/compose.py from {named(source)}",
           f"; ({recipe.get('style', 'calm')}, {key_name}, {song.tempo} bpm, seed {recipe.get('seed', 1)}). Run the recipe",
           "; again after changing it, or edit this file by hand. Chords from",
           "; free-midi-chords by Ludovic Drolez (MIT, tools/progressions.toml).",
           ";"]
    for letter, (scale, prog) in zip("AB", song.progs):
        names = " ".join(chord_name(c, song.key) for c in prog)
        out.append(f"; {letter}: {' '.join(c['token'] for c in prog)} ({scale}): {names}")
    out.append("; Form, 8 bars each: " + " | ".join(f"{k} {'AB'[SECTIONS[k]['prog']]}" for k in song.sections))
    if song.vary != "none":
        out.append(f"; Variation: {song.vary} (every fourth bar walks and pushes, the broken chords turn,"
                   + (" ghost notes on the hi-hat," if song.vary == "most" else "") + " later themes answer backwards).")
    out += ["", f"#title {title}", f"#tempo {song.tempo}"]
    if song.adaptive:
        out.append("; Layers by the game's tension (music.intensity): the pads and the bass always.")
        out += (["#layer D 0.3"] if st["drums"] else []) + ["#layer C 0.55"] + (["#layer A E 0.8"] if song.melody_in != "none" else [])
    out += ["", st["instruments"]]
    accomp, kinds = song.accompaniment()
    for tones, name in kinds.items():
        out.append(f"@{name:<6} square duty=0.5 arp={','.join(map(str, tones))} {st['arp']}")
    out.append("")
    total = song.total
    melody = song.melody()
    if melody:
        out.append("; The melody, in some sections only, and its echo a little later.")
        out += channel("A", melody, total, ["@lead", "v12", "q7", "l16", "p-15"])
        out += channel("E", delayed(melody, st["echo"], total), total, ["@echo", "v4", "q7", "l16", "p40"])
        out.append("")
    out.append("; The chords as a pad of three voices.")
    for letter, voice, pan in zip("PQR", song.pad(), (-30, 0, 30)):
        out += channel(letter, voice, total, ["@pad", "q8", "l16", f"p{pan}"])
    out.append("")
    out.append("; The bass.")
    out += channel("B", song.bass(), total, ["@bass", "v12", "q6", "l16"])
    if accomp:
        out.append("")
        out.append("; The chords broken, in the bridges." if st["accomp"] == "harp" else "; The chords as arpeggios, in the bridges.")
        head = ["v7", "q8", "l16", "p-40"] if st["accomp"] == "harp" else ["v8", "q8" if st["arp_rhythm"] == "held" else "q4", "l16", "p-35"]
        out += channel("C", accomp, total, head)
    if st["drone"]:
        out.append("")
        out.append("; A drone on the tonic.")
        out += channel("F", song.drone(), total, ["@drone", "v11", "q8", "l16", "p25"])
    drums = song.drums()
    if drums:
        out.append("")
        out.append("; Drums.")
        out += channel("D", drums, total, ["v12", "q8", "l16", "p10"], drums=True)
    text = "\n".join(out) + "\n"
    outs = recipe.get("out", Path(source).stem.replace(".compose", "") + ".mml")
    same = True
    for o in [outs] if isinstance(outs, str) else outs:
        path = (Path(source).parent / o).resolve()
        if check:
            there = path.read_text() if path.exists() else None
            same &= there == text
            print(f"{path}: {'the same' if there == text else 'missing' if there is None else 'differs'}")
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        seconds = total / BAR * 4 * 60 / song.tempo
        print(f"wrote {path} ({seconds:.0f} s, {len(song.sections)} sections)")
    return same


def main(args):
    if not args or args[0] in ("-h", "--help"):
        print(__doc__)
        print("usage: compose.py RECIPE.compose.toml [RECIPE ...]\n       compose.py --check RECIPE ...   (writes nothing: are the songs there what the recipes make?)\n       compose.py --find [MOOD ...]\n       compose.py --help")
        return
    if args[0] == "--check":
        same = all([write(tomllib.loads(Path(source).read_text()), source, check=True) for source in args[1:]])
        raise SystemExit(0 if same else 1)
    if args[0].startswith("-") and args[0] != "--find":
        raise SystemExit(f"compose.py: unknown option {args[0]} (see --help)")
    if args[0] == "--find":
        moods = set(args[1:])
        for scale, chords, tags in progressions():
            if moods <= set(tags):
                print(f"{scale:6} {' '.join(chords):40} {' '.join(tags)}")
        return
    for source in args:
        write(tomllib.loads(Path(source).read_text()), source)


if __name__ == "__main__":
    main(sys.argv[1:])

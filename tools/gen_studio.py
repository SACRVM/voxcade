#!/usr/bin/env python3
"""Makes the Studio's first machine (games/studio), a drum machine as a
figure of parts one plays with the mouse through the model: a low wide
case, sixteen step buttons along its front edge in four colors by fours,
a lamp (an LED) behind each; a column for each voice (its button, its
lamp, its level knob); two pattern buttons, a start button, a tempo and a
volume knob. Each control is a part named for the code (`step_1` ..
`step_16`, `led_1` .. `led_16`, `pick_kick`, `knob_kick`, `pattern_a`,
`start`, `knob_tempo` ...). Lamps are parts too: main.lua lights one or
puts it out by its glow (a pose's `glow`), so nothing on the machine is
drawn by Lua. The labels
and the nameplate are painted into the case's top in a small alphabet of
3 by 5 voxels (the nameplate twice as big). Then the table it stands on
and the level, levels/studio.toml.

It makes what is missing and keeps every file there: once made, the
machine is the owner's, shaped in the voxel editor (a file deleted is
made again). main.lua knows the parts by their names only. Plain Python,
no numpy."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import figures  # noqa: E402
from figures import Fig  # noqa: E402

GAME = Path(__file__).resolve().parent.parent / "games" / "studio"
figures.KEEP_EXISTING = True

# The machine's name on its nameplate: invented, a placeholder until the
# owner names it.
NAME = "HARALD RG-80"

# One color a material; the light is the engine's.
CASE = (52, 52, 58)          # the case, dark plastic
PLATE = (84, 82, 80)         # the top plate, grey plastic
BAND = (28, 28, 32)          # the band the steps sit in
SOCKET = (16, 16, 18)        # a lamp's socket, empty
PAINT = (214, 212, 204)      # the labels and the name, painted on
STEPS = [(208, 58, 42), (224, 122, 32), (224, 192, 48), (232, 228, 216)]
PICK = (216, 212, 200)       # the voices' buttons
PATTERN = (90, 106, 120)     # the pattern buttons
START = (200, 56, 42)        # start and stop
KNOB = (30, 30, 34)          # the knobs
MARK = (236, 232, 220)       # a knob's pointer line
LAMP = (255, 64, 40)         # a lamp lit
GLOW = {"_type": "_emit", "_emit": "1.0"}
figures.METALS[LAMP] = GLOW  # the toolkit's table of materials by color

# The case: x across, y deep (0 the front edge), z up; its top at z 9.
W, D, TOP = 151, 77, 9

# The letters, 3 by 5 (M and N 5 and 4 wide, K 4: narrower they read as
# H, n and h), the top row first.
LETTERS = {
    "A": [".#.", "#.#", "###", "#.#", "#.#"],
    "B": ["##.", "#.#", "##.", "#.#", "##."],
    "C": [".##", "#..", "#..", "#..", ".##"],
    "D": ["##.", "#.#", "#.#", "#.#", "##."],
    "E": ["###", "#..", "##.", "#..", "###"],
    "G": [".##", "#..", "#.#", "#.#", ".##"],
    "H": ["#.#", "#.#", "###", "#.#", "#.#"],
    "I": ["###", ".#.", ".#.", ".#.", "###"],
    "K": ["#..#", "#.#.", "##..", "#.#.", "#..#"],
    "L": ["#..", "#..", "#..", "#..", "###"],
    "M": ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
    "N": ["#..#", "##.#", "#.##", "#..#", "#..#"],
    "O": ["###", "#.#", "#.#", "#.#", "###"],
    "P": ["##.", "#.#", "##.", "#..", "#.."],
    "R": ["##.", "#.#", "##.", "#.#", "#.#"],
    "S": [".##", "#..", ".#.", "..#", "##."],
    "T": ["###", ".#.", ".#.", ".#.", ".#."],
    "U": ["#.#", "#.#", "#.#", "#.#", "###"],
    "V": ["#.#", "#.#", "#.#", "#.#", ".#."],
    "0": ["###", "#.#", "#.#", "#.#", "###"],
    "8": ["###", "#.#", "###", "#.#", "###"],
    "-": ["...", "...", "###", "...", "..."],
    " ": ["..", "..", "..", "..", ".."],
}


def text_width(text, scale=1):
    return sum((len(LETTERS[c][0]) + 1) * scale for c in text) - scale


def paint(case, text, middle, top, scale=1):
    """Paints `text` into the case's top, centred across on `middle`, its
    top row at y `top` (the far side: letters read from the front)."""
    x = middle - text_width(text, scale) // 2
    for c in text:
        rows = LETTERS[c]
        for r, row in enumerate(rows):
            for k, sign in enumerate(row):
                if sign == "#":
                    for dx in range(scale):
                        for dy in range(scale):
                            p = (x + k * scale + dx, top - r * scale - dy, TOP)
                            if p in case.v:
                                case.v[p] = PAINT
        x += (len(rows[0]) + 1) * scale


def step_x(i):
    """Where step i (from 0) begins across: 8 apart, a wider gap every four."""
    return 8 + i * 8 + (i // 4) * 3


# Buttons: (name, x0, y0, width, depth, color, label); a lamp behind each,
# its label in front of it.
BUTTONS = [(f"step_{i + 1}", step_x(i), 4, 6, 5, STEPS[i // 4], None) for i in range(16)]
BUTTONS += [
    ("pick_kick", 8, 26, 9, 6, PICK, "KICK"),
    ("pick_snare", 32, 26, 9, 6, PICK, "SNARE"),
    ("pick_hat", 56, 26, 9, 6, PICK, "HAT"),
    ("pattern_a", 102, 26, 9, 6, PATTERN, "A"),
    ("pattern_b", 116, 26, 9, 6, PATTERN, "B"),
    ("start", 132, 26, 11, 6, START, "START"),
]
# Knobs: (name, x0, y0), 9 by 9, 5 tall over the plate: a voice's level
# over its button, the tempo and the volume at the right.
KNOBS = [("knob_kick", 8, 39), ("knob_snare", 32, 39), ("knob_hat", 56, 39),
         ("knob_tempo", 104, 39), ("knob_volume", 131, 39)]
# Labels over knobs: (text, the middle across).
OVER_KNOBS = [("LEVEL", 36), ("TEMPO", 108), ("VOLUME", 135)]


def lamp_of(name, x0, y0, width, depth):
    """A button's lamp: 2 by 2, in the middle behind it, 2 voxels back."""
    lamp = name.replace("step_", "led_").replace("pick_", "led_").replace("pattern_", "led_")
    if lamp == name:
        lamp = "led_" + name
    lx = x0 + (width - 2) // 2
    return lamp, lx, y0 + depth + 2


def machine():
    case = Fig((W, D, TOP + 1))
    case.rbox((0, 0, 0), (W - 1, D - 1, TOP), CASE, bevel=1)
    # The top: the band of steps in front, a groove, the plate behind.
    case.recolour(lambda p: p[2] == TOP and 2 <= p[1] <= 15 and 1 <= p[0] <= W - 2, BAND)
    case.recolour(lambda p: p[2] == TOP and p[1] >= 18 and 1 <= p[0] <= W - 2 and p[1] <= D - 2, PLATE)
    for x in range(2, W - 2):
        case.v.pop((x, 16, TOP), None)
    # A grille of grooves between the voices and the patterns.
    for y in range(24, 52, 4):
        for x in range(72, 97):
            case.v.pop((x, y, TOP), None)
    # The labels: a voice's under its button, LEVEL, TEMPO and VOLUME over
    # their knobs, the name at the top left.
    for name, x0, _, w, _, _, label in BUTTONS:
        if label:
            paint(case, label, x0 + w // 2, 23)
    for label, middle in OVER_KNOBS:
        paint(case, label, middle, 58)
    paint(case, NAME, 8 + text_width(NAME, 2) // 2, 72, scale=2)
    parts = [("case", case)]
    for name, x0, y0, w, d, color, _ in BUTTONS:
        # A well a voxel deep, the button standing in it, two over the top.
        for x in range(x0, x0 + w):
            for y in range(y0, y0 + d):
                case.v.pop((x, y, TOP), None)
        button = Fig((W, D, TOP + 3))
        button.rbox((x0, y0, TOP), (x0 + w - 1, y0 + d - 1, TOP + 2), color, bevel=1, axes=2)
        parts.append((name, button))
        lamp, lx, ly = lamp_of(name, x0, y0, w, d)
        for x in (lx, lx + 1):
            for y in (ly, ly + 1):
                case.v.pop((x, y, TOP), None)
                case.put((x, y, TOP - 1), SOCKET)
        led = Fig((W, D, TOP + 2))
        led.box((lx, ly, TOP), (lx + 1, ly + 1, TOP + 1), LAMP)
        parts.append((lamp, led))
    for name, x0, y0 in KNOBS:
        knob = Fig((W, D, TOP + 6))
        knob.rbox((x0, y0, TOP + 1), (x0 + 8, y0 + 8, TOP + 5), KNOB, bevel=2, axes=2)
        # Its pointer: a line from the middle to the far edge on top.
        for y in range(y0 + 5, y0 + 9):
            knob.put((x0 + 4, y, TOP + 5), MARK)
        parts.append((name, knob))
    figures.save_parts(GAME / "machine.vvx", parts)


def table():
    path = GAME / "table.vvx"
    if figures.kept(path):
        return
    top = Fig((211, 121, 2))
    top.box((0, 0, 0), (210, 120, 1), (92, 70, 52))
    figures.save(path, top.size, [top])


LEVEL = """format = "voxcade level 1"
size = [211, 121, 40]

[[thing]]
name = "table"
model = "table.vvx"
at = [105, 60, 0]
facing = "north"

[[thing]]
name = "machine"
model = "machine.vvx"
at = [105, 64, 2]
facing = "north"

[[place]]
name = "start"
at = [105, 58, 2]
"""


def level():
    path = GAME / "levels" / "studio.toml"
    if figures.kept(path):
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(LEVEL)
    print(f"{path}: written")


if __name__ == "__main__":
    machine()
    table()
    level()

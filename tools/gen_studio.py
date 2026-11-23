#!/usr/bin/env python3
"""Makes the Studio's machine (games/studio), the HARALD RG-80: a drum
machine after the classic analog one of 1980 in layout and function, as a
figure of parts one plays with the mouse through the model. A wide, low
case: a column for each voice (ACCENT, then BD, SD, LT, MT, HT, RS, CP,
CB, CY, OH, CH), its LEVEL knob at the top and under it the knobs that
shape that voice (TONE, DECAY, SNAPPY, TUNING) or a switch to its other
instrument (LT to LC and so on); a lamp beside each column's name; sixteen
step buttons along the front edge in four colors by fours, a lamp behind
each; at the left the voice selector (a rotary switch, a dot for each of
its twelve positions), TEMPO and VOLUME, START, the patterns A and B.

Each control is a part named for the code: `step_1` .. `step_16`,
`led_1` .. `led_16`, `bd_level`, `bd_tone`, `lt_tuning`, `lt_switch`,
`led_bd`, `selector`, `tempo`, `volume`, `start`, `pattern_a`, `led_a`
and so on. Lamps are parts: main.lua lights one or puts it out by its
glow (a pose's `glow`), so nothing on the machine is drawn by Lua. The
labels and the nameplate are painted into the case's top in a small
alphabet of 3 by 5 voxels (the nameplate twice as big). Then the table it
stands on and the level, levels/studio.toml.

It makes what is missing and keeps every file there: once made, the
machine is the owner's, shaped in the voxel editor (a file deleted is
made again). main.lua knows the parts by their names only. Plain Python,
no numpy."""

import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import figures  # noqa: E402
from figures import Fig  # noqa: E402

GAME = Path(__file__).resolve().parent.parent / "games" / "studio"
figures.KEEP_EXISTING = True

# The machine's name on its nameplate: the owner's invented maker and its
# Rhythmus-Geraet.
NAME = "HARALD RG-80"

# One color a material; the light is the engine's.
CASE = (52, 52, 58)          # the case, dark plastic
PLATE = (84, 82, 80)         # the top plate, grey plastic
BAND = (28, 28, 32)          # the band the steps sit in
SOCKET = (16, 16, 18)        # a lamp's socket, a switch's slot
PAINT = (214, 212, 204)      # the labels and the name, painted on
STEPS = [(208, 58, 42), (224, 122, 32), (224, 192, 48), (232, 228, 216)]
LEVER = (196, 194, 188)      # a switch's lever
PATTERN = (90, 106, 120)     # the pattern buttons
START = (200, 56, 42)        # start and stop
KNOB = (30, 30, 34)          # the knobs and the selector
MARK = (236, 232, 220)       # a knob's pointer line
LAMP = (255, 64, 40)         # a lamp lit
GLOW = {"_type": "_emit", "_emit": "1.0"}
figures.METALS[LAMP] = GLOW  # the toolkit's table of materials by color

# The letters, 3 by 5 (M 5 wide, N and K 4: narrower they read as H, n
# and h), the top row first.
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
    "V": ["#.#", "#.#", "#.#", ".#.", ".#."],
    "Y": ["#.#", "#.#", ".#.", ".#.", ".#."],
    "0": ["###", "#.#", "#.#", "#.#", "###"],
    "8": ["###", "#.#", "###", "#.#", "###"],
    "-": ["...", "...", "###", "...", "..."],
    " ": ["..", "..", "..", "..", ".."],
}

# The voices' columns, left to right: the name over the column, then the
# knobs under LEVEL (a name each), then a switch to the other instrument
# (its name on the switch's right) or none. ACCENT is the selector's first
# position: its row of steps plays every voice louder.
COLUMNS = [
    ("ac", "ACCENT", [], None),
    ("bd", "BD", ["TONE", "DECAY"], None),
    ("sd", "SD", ["TONE", "SNAPPY"], None),
    ("lt", "LT", ["TUNING"], "LC"),
    ("mt", "MT", ["TUNING"], "MC"),
    ("ht", "HT", ["TUNING"], "HC"),
    ("rs", "RS", [], "CL"),
    ("cp", "CP", [], "MA"),
    ("cb", "CB", [], None),
    ("cy", "CY", ["TONE", "DECAY"], None),
    ("oh", "OH", ["DECAY"], None),
    ("ch", "CH", [], None),
]

# The case's top at z TOP; across x, deep y (0 the front edge).
TOP = 9
KNOB_SIZE = 9                 # a voice's knob, 5 tall over the plate
LEFT = 43                     # where the voices' columns begin across
# Each knob row's front edge, LEVEL first, then towards the front. A label
# lies in front of its knob: a knob hides a few voxels behind it from the
# camera (more where the view looks flatter, near the screen's bottom),
# so nothing is painted there.
ROW_Y = [74, 53, 32]
NAME_Y = 93                   # the top row of the columns' names
PLATE_FRONT = 24              # the plate begins behind the band of steps
D = 110                       # the case's depth


def text_width(text, scale=1):
    return sum((len(LETTERS[c][0]) + 1) * scale for c in text) - scale


def paint(case, text, middle, top, scale=1, left=None):
    """Paints `text` into the case's top, centred across on `middle` (or
    from `left`), its top row at y `top` (the far side: letters read from
    the front)."""
    x = left if left is not None else middle - text_width(text, scale) // 2
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


def column_width(name, knobs, other):
    """A column as wide as its widest label, and some room between."""
    switch = text_width(name) + text_width(other) + 17 if other else 0
    return max(24, text_width(name) + 4, text_width("LEVEL") + 5, switch + 3,
               *(text_width(k) + 5 for k in knobs))


def columns():
    """Each column's (code, name, knobs, other, middle across, left, right)."""
    out, x = [], LEFT
    for code, name, knobs, other in COLUMNS:
        w = column_width(name, knobs, other)
        out.append((code, name, knobs, other, x + w // 2, x, x + w))
        x += w
    return out


W = columns()[-1][6] + 4      # the case's width


def well(case, x0, y0, w, d):
    """A well a voxel deep in the top for a button to stand in."""
    for x in range(x0, x0 + w):
        for y in range(y0, y0 + d):
            case.v.pop((x, y, TOP), None)


def lamp(case, name, x0, y0, w=2, d=2):
    """A lamp of `w` by `d` in its socket: the socket sunk into the top
    (dark), the lamp a part standing in it, two voxels tall."""
    for x in range(x0, x0 + w):
        for y in range(y0, y0 + d):
            case.v.pop((x, y, TOP), None)
            case.put((x, y, TOP - 1), SOCKET)
    led = Fig((W, D, TOP + 2))
    led.box((x0, y0, TOP), (x0 + w - 1, y0 + d - 1, TOP + 1), LAMP)
    return (name, led)


def button(case, name, x0, y0, w, d, color):
    """A button standing in its well, two voxels over the top."""
    well(case, x0, y0, w, d)
    b = Fig((W, D, TOP + 3))
    b.rbox((x0, y0, TOP), (x0 + w - 1, y0 + d - 1, TOP + 2), color, bevel=1, axes=2)
    return (name, b)


def knob(name, middle, y0, size=KNOB_SIZE):
    """A knob `size` across, 5 tall, its pointer line on top from the
    middle to the far edge (pointing back at rest: the knob's middle)."""
    x0 = middle - size // 2
    k = Fig((W, D, TOP + 6))
    k.rbox((x0, y0, TOP + 1), (x0 + size - 1, y0 + size - 1, TOP + 5), KNOB, bevel=2, axes=2)
    for y in range(y0 + size // 2 + 1, y0 + size):
        k.put((middle, y, TOP + 5), MARK)
    return (name, k)


def switch(case, name, middle, y_mid):
    """A toggle: a lever on a slot, tilting left or right about its foot
    (a pose's roll); the slot is the case's."""
    for x in range(middle - 3, middle + 4):
        case.v[(x, y_mid, TOP)] = SOCKET
    lever = Fig((W, D, TOP + 9))
    lever.box((middle, y_mid, TOP + 1), (middle, y_mid, TOP + 6), LEVER)
    lever.rbox((middle - 1, y_mid - 1, TOP + 7), (middle + 1, y_mid + 1, TOP + 8), LEVER, bevel=1, axes=2)
    return (name, lever)


def selector(case, middle, y_mid, positions=12, sweep=300, radius=10):
    """The voice selector: a dial 15 across with its pointer line, and a
    dot painted round it for each position, from the front left round to
    the front right (`sweep` degrees, clockwise seen from above)."""
    for i in range(positions):
        a = math.radians(-sweep / 2 + i * sweep / (positions - 1))
        p = (round(middle + radius * math.sin(a)), round(y_mid + radius * math.cos(a)), TOP)
        if p in case.v:
            case.v[p] = PAINT
    dial = Fig((W, D, TOP + 6))
    dial.rbox((middle - 7, y_mid - 7, TOP + 1), (middle + 7, y_mid + 7, TOP + 4), KNOB, bevel=4, axes=2)
    for y in range(y_mid + 1, y_mid + 8):
        dial.put((middle, y, TOP + 4), MARK)
    return ("selector", dial)


def machine():
    case = Fig((W, D, TOP + 1))
    case.rbox((0, 0, 0), (W - 1, D - 1, TOP), CASE, bevel=1)
    # The top: the band of steps in front, a groove, the plate behind.
    case.recolour(lambda p: p[2] == TOP and 2 <= p[1] <= PLATE_FRONT - 3 and 1 <= p[0] <= W - 2, BAND)
    case.recolour(lambda p: p[2] == TOP and PLATE_FRONT <= p[1] <= D - 2 and 1 <= p[0] <= W - 2, PLATE)
    for x in range(2, W - 2):
        case.v.pop((x, PLATE_FRONT - 2, TOP), None)
    parts = []
    cols = columns()
    # The columns, a groove between each two.
    for code, name, knobs, other, middle, left, right in cols:
        if left > LEFT:
            for y in range(PLATE_FRONT + 2, NAME_Y + 2):
                case.v.pop((left, y, TOP), None)
        # Its name, a lamp before it (lit while the selector names it).
        group = text_width(name) + 4
        parts.append(lamp(case, "led_" + code, middle - group // 2, NAME_Y - 3))
        paint(case, name, None, NAME_Y, left=middle - group // 2 + 4)
        for row, label in enumerate(["LEVEL"] + knobs):
            y0 = ROW_Y[row]
            paint(case, label, middle, y0 - 2)
            parts.append(knob(f"{code}_{label.lower()}", middle, y0))
        if other:
            # The switch in the last row: this voice's name on its left,
            # the other's on its right.
            y_mid = ROW_Y[2] + KNOB_SIZE // 2
            paint(case, name, None, y_mid + 2, left=middle - 7 - text_width(name))
            paint(case, other, None, y_mid + 2, left=middle + 8)
            parts.append(switch(case, f"{code}_switch", middle, y_mid))
    # The left panel: the selector, TEMPO and VOLUME, from the front back.
    mid = (4 + LEFT - 4) // 2
    paint(case, "VOICE", mid, 30)
    parts.append(selector(case, mid, 44))
    paint(case, "TEMPO", mid, 65)
    parts.append(knob("tempo", mid, 67, size=13))
    paint(case, "VOLUME", mid, 90)
    parts.append(knob("volume", mid, 92))
    # The nameplate behind the columns.
    paint(case, NAME, None, D - 4, scale=2, left=LEFT + 2)
    # The front band: START and the patterns at the left, then the steps.
    for name, x0, w, color, label in [("start", 6, 13, START, "START"), ("pattern_a", 24, 7, PATTERN, "A"),
                                      ("pattern_b", 33, 7, PATTERN, "B")]:
        parts.append(button(case, name, x0, 4, w, 8, color))
        parts.append(lamp(case, "led_" + name.replace("pattern_", ""), x0 + (w - 2) // 2, 14))
        paint(case, label, x0 + w // 2, 21)
    span = cols[-1][6] - LEFT
    first = LEFT + (span - (15 * 17 + 3 * 6 + 12)) // 2
    for i in range(16):
        x0 = first + i * 17 + (i // 4) * 6
        parts.append(button(case, f"step_{i + 1}", x0, 4, 12, 8, STEPS[i // 4]))
        parts.append(lamp(case, f"led_{i + 1}", x0 + 4, 14, w=4))
    figures.save_parts(GAME / "machine.vvx", [("case", case)] + parts)


def table():
    path = GAME / "table.vvx"
    if figures.kept(path):
        return
    top = Fig((431, 161, 2))
    top.box((0, 0, 0), (430, 160, 1), (92, 70, 52))
    figures.save(path, top.size, [top])


LEVEL = """format = "voxcade level 1"
size = [431, 161, 40]

[[thing]]
name = "table"
model = "table.vvx"
at = [215, 80, 0]
facing = "north"

[[thing]]
name = "machine"
model = "machine.vvx"
at = [215, 82, 2]
facing = "north"

[[place]]
name = "start"
at = [215, 68, 2]
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

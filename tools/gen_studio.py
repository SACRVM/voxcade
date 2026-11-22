#!/usr/bin/env python3
"""Makes the Studio's first machine (games/studio), a drum machine as a
figure of parts one plays with the mouse through the model: a low wide
case, sixteen step buttons along its front edge in four colors by fours,
a lamp (an LED) behind each, three instrument buttons, two pattern
buttons, a start button and three knobs, each a part named for the code
(`step_1` .. `step_16`, `led_1` .. `led_16`, `pick_kick`, `pattern_a`,
`start`, `knob_level` ...). Lamps are parts too: main.lua lights one by
raising it into its socket and puts it out by sinking it into the case
(a pose's offset), so nothing on the machine is drawn by Lua. Then the
table it stands on and the level, levels/studio.toml.

It makes what is missing and keeps every file there: once made, the
machine is the owner's, shaped in the voxel editor (a file deleted is
made again). main.lua knows where the parts lie (its LAYOUT mirrors the
numbers below), so a part moved in the editor needs its line there too.
Plain Python, no numpy."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import figures  # noqa: E402
from figures import Fig  # noqa: E402

GAME = Path(__file__).resolve().parent.parent / "games" / "studio"
figures.KEEP_EXISTING = True

# One color a material; the light is the engine's.
CASE = (52, 52, 58)          # the case, dark plastic
PLATE = (156, 152, 144)      # the top plate, grey plastic
BAND = (28, 28, 32)          # the band the steps sit in
SOCKET = (16, 16, 18)        # a lamp's socket, empty
STEPS = [(208, 58, 42), (224, 122, 32), (224, 192, 48), (232, 228, 216)]
PICK = (216, 212, 200)       # the instrument buttons
PATTERN = (90, 106, 120)     # the pattern buttons
START = (200, 56, 42)        # start and stop
KNOB = (30, 30, 34)          # the knobs
MARK = (236, 232, 220)       # a knob's pointer line
LAMP = (255, 64, 40)         # a lamp lit
GLOW = {"_type": "_emit", "_emit": "1.0"}
figures.METALS[LAMP] = GLOW  # the toolkit's table of materials by color

# The case: x across, y deep (0 the front edge), z up; its top at z 9.
W, D, TOP = 151, 61, 9


def step_x(i):
    """Where step i (from 0) begins across: 8 apart, a wider gap every four."""
    return 8 + i * 8 + (i // 4) * 3


# Buttons: (name, x0, y0, width, depth, color); a lamp behind each.
BUTTONS = [(f"step_{i + 1}", step_x(i), 4, 6, 5, STEPS[i // 4]) for i in range(16)]
BUTTONS += [
    ("pick_kick", 10, 22, 9, 6, PICK),
    ("pick_snare", 24, 22, 9, 6, PICK),
    ("pick_hat", 38, 22, 9, 6, PICK),
    ("pattern_a", 96, 22, 9, 6, PATTERN),
    ("pattern_b", 110, 22, 9, 6, PATTERN),
    ("start", 130, 22, 11, 8, START),
]
# Knobs: (name, x0, y0), 9 by 9, 5 tall over the plate.
KNOBS = [("knob_level", 24, 40), ("knob_tempo", 112, 40), ("knob_volume", 130, 40)]


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
    # A grille of grooves between the instruments and the patterns.
    for y in range(24, 50, 4):
        for x in range(58, 89):
            case.v.pop((x, y, TOP), None)
    parts = [("case", case)]
    for name, x0, y0, w, d, color in BUTTONS:
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

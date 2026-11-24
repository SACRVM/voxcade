#!/usr/bin/env python3
"""Makes the Studio's machine (games/studio), the HARALD RG-80: a drum
machine after the analog one of 1980 in function, as a figure of parts one
plays with the mouse through the model, in fine voxels: the whole device on
the screen at 1280 by 720 (the owner's rules, 2026-11-24: a device fits the
screen whole; pixels limit what reads, not voxels; the sixteen steps and
their lamps in one row; on the device signs of two or three letters, the
full name at the screen's bottom edge for the part under the pointer; the
same margins and gaps on every side).

The case 512 voxels across (a model's box holds 512). Its top: a strip at
the back (the nameplate flush left; ACC, BPM with the tempo's display and
its knob, VOL flush right), eleven voice columns (BD SD LT MT HT RS CP CB CY
OH CH: a lamp and the name, LVL and the knobs that shape the voice, or a
toggle switch to the other instrument, its two signs over it), the voice
selector alone in the left panel (a dial, a ring of twelve lamps, the
signs, ACC in the accent's yellow), and in front the band of keys: START,
A and B, the sixteen steps in four colors by fours, a lamp behind each.
Every place is worked out from a few numbers (M, COL, the knobs, the gaps,
`gap` for the perspective); `margins` prints the result.

Each control is a part named for main.lua: `step_1` .. `step_16`, `led_1`
.. `led_16`, `bd_level`, `bd_tone`, `lt_tuning`, `lt_switch`, `ac_level`,
`tempo`, `volume`, `selector`, `start`, `pattern_a`, `led_a` and so on;
`led_bd` .. `led_ch` the columns' lamps (their hits); `start_play` and
`start_stop` (the signs on START, one shown at a time); `NAME_ptr` (a
knob's pointer line, lit under the pointer) and `NAME_base` (the flush
plate round a knob, a drag on it turning the knob); `XX_mount` and
`XX_plate` (a toggle's round base, and the flush plate round it a click
takes too); `XX_head` (a column's name and lamp, for the bottom line);
`sel_1` .. `sel_12` (the selector's lamps); `NAME_mark` (a key's cursor
mark, a white bar in front of it, hidden until posed); `display`,
`seg_1_a` .. `seg_3_g` and `lit_1_a` .. `lit_3_g` (the tempo's display:
its window, each segment faint and lit). Lamps and segments are parts lit
or put out by main.lua's poses (`glow`, sunk when out), so nothing on the
machine is drawn by Lua. The signs are painted into the case's top in an
alphabet of 6 by 9 voxels (the nameplate twice as big). The table is the
level's own land, a `[terrain]` of one height (levels/studio.toml).

It makes what is missing and keeps every file there: once made, the
machine is the owner's, shaped in the voxel editor (a file deleted is made
again). main.lua knows the parts by their names only. Plain Python, no
numpy."""

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
BAND_COLOR = (28, 28, 32)    # the band the steps sit in
SOCKET = (16, 16, 18)        # a lamp's socket, a switch's slot, a groove
PAINT = (214, 212, 204)      # the labels and the name, painted on
BEZEL = (150, 148, 142)      # the ring round a lamp
STEPS = [(208, 58, 42), (224, 122, 32), (224, 192, 48), (232, 228, 216)]
ACCENT_SIGN = STEPS[2]       # the accent's sign (no instrument): the third group's yellow
LEVER = (196, 194, 188)      # a switch's lever
PATTERN = (90, 106, 120)     # the pattern buttons
START = (200, 56, 42)        # start and stop
KNOB = (30, 30, 34)          # the knobs and the selector
MARK = (236, 232, 220)       # a knob's pointer line
LAMP = (255, 64, 40)         # a lamp lit, a display's segment lit
WINDOW = (22, 12, 12)        # the display's dark window
SEG_OFF = (54, 14, 12)       # a display's segment unlit: faint, as an LED's
GLOW = {"_type": "_emit", "_emit": "1.0"}
figures.METALS[LAMP] = GLOW
TABLE = (92, 70, 52)

# The letters, 6 wide (M, N and V 7) by 9 tall, the top row first: upright
# strokes two voxels, the bars one, as a typeface's thick and thin.
LETTERS = {
    "A": [".####.", "##..##", "##..##", "##..##", "######", "##..##", "##..##", "##..##", "##..##"],
    "B": ["#####.", "##..##", "##..##", "##..##", "#####.", "##..##", "##..##", "##..##", "#####."],
    "C": [".####.", "##..##", "##....", "##....", "##....", "##....", "##....", "##..##", ".####."],
    "D": ["#####.", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", "#####."],
    "E": ["######", "##....", "##....", "##....", "#####.", "##....", "##....", "##....", "######"],
    "G": [".####.", "##..##", "##....", "##....", "##.###", "##..##", "##..##", "##..##", ".####."],
    "H": ["##..##", "##..##", "##..##", "##..##", "######", "##..##", "##..##", "##..##", "##..##"],
    "K": ["##..##", "##..##", "##.##.", "####..", "###...", "####..", "##.##.", "##..##", "##..##"],
    "L": ["##....", "##....", "##....", "##....", "##....", "##....", "##....", "##....", "######"],
    "M": ["##...##", "###.###", "#######", "##.#.##", "##...##", "##...##", "##...##", "##...##", "##...##"],
    "N": ["##...##", "###..##", "###..##", "##.#.##", "##.#.##", "##..###", "##..###", "##...##", "##...##"],
    "O": [".####.", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", ".####."],
    "P": ["#####.", "##..##", "##..##", "##..##", "#####.", "##....", "##....", "##....", "##...."],
    "R": ["#####.", "##..##", "##..##", "##..##", "#####.", "##.##.", "##..##", "##..##", "##..##"],
    "S": [".####.", "##..##", "##....", "##....", ".####.", "....##", "....##", "##..##", ".####."],
    "T": ["######", "..##..", "..##..", "..##..", "..##..", "..##..", "..##..", "..##..", "..##.."],
    "U": ["##..##", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", "##..##", ".####."],
    "V": ["##...##", "##...##", "##...##", ".##.##.", ".##.##.", ".##.##.", "..###..", "..###..", "...#..."],
    "Y": ["##..##", "##..##", "##..##", ".####.", "..##..", "..##..", "..##..", "..##..", "..##.."],
    "0": [".####.", "##..##", "##..##", "##.###", "##..##", "###.##", "##..##", "##..##", ".####."],
    "8": [".####.", "##..##", "##..##", "##..##", ".####.", "##..##", "##..##", "##..##", ".####."],
    "-": ["....", "....", "....", "....", "####", "....", "....", "....", "...."],
    " ": ["...", "...", "...", "...", "...", "...", "...", "...", "..."],
}
# The signs on START: play, and stop while it runs.
PLAY_SIGN = ["#......", "###....", "####...", "######.", "#######", "######.", "####...", "###....", "#......"]
STOP_SIGN = ["#######"] * 7



# The voices' columns, left to right: the name over the column, the knobs
# under LVL, a switch to the other instrument (both signs over the slide).
COLUMNS = [
    ("bd", "BD", [("tone", "TON"), ("decay", "DEC")], None),
    ("sd", "SD", [("tone", "TON"), ("snappy", "SNP")], None),
    ("lt", "LT", [("tuning", "TUN")], "LC"),
    ("mt", "MT", [("tuning", "TUN")], "MC"),
    ("ht", "HT", [("tuning", "TUN")], "HC"),
    ("rs", "RS", [], "CL"),
    ("cp", "CP", [], "MA"),
    ("cb", "CB", [], None),
    ("cy", "CY", [("tone", "TON"), ("decay", "DEC")], None),
    ("oh", "OH", [("decay", "DEC")], None),
    ("ch", "CH", [], None),
]
# The selector's twelve positions: the accent first (no instrument: its
# level is a master in the top strip), then the eleven voices.
SELECTOR_SIGNS = ["ACC"] + [c[1] for c in COLUMNS]

# ------------------------------------------------------------------ the measure
# The layout follows from these few numbers (the owner, 2026-11-24: the same
# margins and gaps on every side), every place below worked out from them.
M = 16                        # the margin: case edge to content, and every gap
                              # between groups (rows, heading, nameplate)
COL = 36                      # a voice's column, its groove included
KNOB_R = 7                    # a voice's knob 14 across
MASTER_R = 9                  # the masters' (ACC, BPM, VOL) 18 across (the voices' 14)
SIDE_GAP = 4                  # a master's sign beside it, this far from its marks
DIGIT_W, DIGIT_H, SEG = 8, 14, 2   # a seven-segment digit, its segments' thickness
DIGIT_GAP, WINDOW_PAD = 3, 3  # between digits; round them in the window
TICK = 3.2                    # a knob's marks reach this far past its edge
LABEL_GAP = 2                 # between a control's marks and its label
LETTER = 9                    # a sign's height
KEY_W, KEY_D = 17, 28         # a step's key
KEY_GAP, GROUP_GAP = 4, 21    # between keys of a group; between groups
LAMP_GAP = 4                  # between a key and its lamp behind it
LAMP_R = 2.6                  # a key's lamp, 5 across
PAIR_GAP = 3                  # between a switch's two signs
MOUNT_R = 5.5                 # a toggle's round base, 11 across
LEVER_W, LEVER_L = 4, 10      # its lever: thick, long (a thicker tip on it)
TOP = 8                       # the top plate's surface (z)
Z = TOP + 18                  # every part's box this high
W = 512                       # the case across (a model's box holds 512)


def reach(r):
    """How far a knob of radius `r` reaches with its marks, whole voxels:
    across and behind it."""
    return math.ceil(r + TICK)


def front(r):
    """How far a knob reaches towards the front: its skirt (its sweep leaves
    the front free of marks, the nearest at 135 degrees lie higher)."""
    return r + 1


# Across, from the right: the last column's content (its knob's marks) a
# margin from the case's right edge; the columns leftwards; the left panel
# between the case's left margin and the first groove, its content as far
# from that groove as a column's content from its own.
PAD = (COL - 1) // 2 - reach(KNOB_R)            # a column's content to its grooves
LAST_MID = W - M - reach(KNOB_R)                # the last column's middle
LEFT = LAST_MID - (COL - 1) // 2 - 1 - (len(COLUMNS) - 1) * COL   # the first groove
PANEL = (M, LEFT - 1 - PAD)                     # the left panel's content, x from..to
PANEL_MID = (PANEL[0] + PANEL[1] + 1) / 2

# The perspective: at the game's pitch a voxel at the back of the case looks
# smaller than one at the front. Measured on the 720 picture (conf.lua's
# camera: pitch 70, fov 25, 640 across) between the blocks of a voice's
# column (the free pixels from a knob's front to the sign in front of it,
# and between the flat signs): 34 pixels for 17 voxels just behind the band
# (2.0 a voxel), 31 for 18 under the nameplate (1.72), about 0.09 % smaller
# a voxel deeper. A gap is given in voxels as it looks at the
# front and made that much wider the further back it lies, so every margin
# and gap looks as wide as M wherever it is.
PERSPECTIVE = 0.0009


def gap(y, size=M):
    """`size` voxels as they look at the front, at depth `y` (its middle)."""
    return round(size * (1 + PERSPECTIVE * y))


# Deep, from the front: the keys a margin from the front edge, their lamps
# behind them, a margin, the band's groove; a margin, the three rows of
# knobs (each its marks, the gap, its label, a gap to the next), a gap, the
# columns' names, a gap, the nameplate, the back margin.
KEY_Y = M
LAMP_Y = KEY_Y + KEY_D + LAMP_GAP + LAMP_R      # a lamp's middle
BAND = math.ceil(LAMP_Y + LAMP_R) + gap(LAMP_Y + M)    # the band's groove (y)


def label_top(cy, r=KNOB_R):
    """A control's sign: its top row, the sign a gap behind the marks."""
    return cy + reach(r) + LABEL_GAP + LETTER


ROW_Y = []
_y = BAND + 1                                   # the first free row
for _k in range(3):
    _cy = _y + gap(_y + M / 2) + front(KNOB_R)
    ROW_Y.append(_cy)
    _y = label_top(_cy) + 1
HEAD = _y + gap(_y + M / 2) + LETTER - 1        # the columns' names: their top row
# The top strip: the nameplate flush left, TEMPO and VOLUME flush right,
# as deep as the taller of the two (a master knob and its marks), each
# centred in it; a gap from the columns' names, the back margin behind it.
_y = HEAD + 1
STRIP_Y = _y + gap(_y + M / 2)                  # the strip's first row
MASTER = MASTER_R + 1 + math.ceil(MASTER_R + TICK)     # a master knob deep, its marks too
STRIP = max(2 * LETTER, MASTER)                 # the strip deep
NAMEPLATE = STRIP_Y + 2 * LETTER - 1           # the nameplate's top row: its lower edge
                                                # on the strip's, as near the names as the masters
MASTER_Y = STRIP_Y + (STRIP - MASTER) // 2 + MASTER_R + 1          # the masters' middles
M_BACK = gap(STRIP_Y + STRIP + M / 2)           # the back margin, as it looks
D = STRIP_Y + STRIP + M_BACK                    # the case deep


def text_width(text, scale=1):
    return sum((len(LETTERS[c][0]) + 1) * scale for c in text) - scale


def paint_rows(fig, rows, left, top, color, z=TOP, scale=1, only_on=True):
    """Paints a picture of rows (the top row first, at y `top`, the far
    side: it reads from the front) from x `left`."""
    for r, row in enumerate(rows):
        for k, sign in enumerate(row):
            if sign == "#":
                for dx in range(scale):
                    for dy in range(scale):
                        p = (left + k * scale + dx, top - r * scale - dy, z)
                        if not only_on or p in fig.v:
                            fig.v[p] = color


def paint(case, text, middle, top, scale=1, left=None, color=PAINT):
    """`text` painted into the case's top, centred on `middle` or from `left`."""
    x = left if left is not None else round(middle - text_width(text, scale) / 2)
    for c in text:
        paint_rows(case, LETTERS[c], x, top, color, scale=scale)
        x += (len(LETTERS[c][0]) + 1) * scale


def disc(cx, cy, r):
    """The cells whose middles lie within `r` of (cx, cy)."""
    out = []
    for x in range(math.floor(cx - r) - 1, math.ceil(cx + r) + 1):
        for y in range(math.floor(cy - r) - 1, math.ceil(cy + r) + 1):
            if (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 <= r * r:
                out.append((x, y))
    return out


def case_shell():
    """The case: a low shell (its top two voxels thick, its walls two), the
    top's edge cut back a voxel; the band of steps, a groove, the plate."""
    case = Fig((W, D, TOP + 1))
    for x in range(W):
        for y in range(D):
            edge = min(x, y, W - 1 - x, D - 1 - y)
            for z in range(TOP + 1):
                if edge >= 2 and z < TOP - 1:
                    continue  # hollow inside
                if z == TOP and edge == 0:
                    continue  # the top's edge cut back
                if 1 <= y <= BAND - 1 and z == TOP and 1 <= x <= W - 2:
                    case.v[(x, y, z)] = BAND_COLOR
                elif y >= BAND + 1 and z == TOP and 1 <= x <= W - 2 and y <= D - 2:
                    case.v[(x, y, z)] = PLATE
                else:
                    case.v[(x, y, z)] = CASE
    groove(case, [(x, BAND) for x in range(2, W - 2)])
    return case



def groove(case, cells):
    """A groove a voxel wide and deep, dark at its bottom."""
    for x, y in cells:
        case.v.pop((x, y, TOP), None)
        case.v[(x, y, TOP - 1)] = SOCKET


def lamp(case, name, cx, cy, r=LAMP_R):
    """A small round lamp in its socket, a thin bezel round it: the socket
    sunk into the top (dark), the lamp a part standing two voxels in it;
    main.lua sinks it to put it out."""
    cells = disc(cx, cy, r)
    for x, y in disc(cx, cy, r + 1.1):
        if (x, y, TOP) in case.v:
            case.v[(x, y, TOP)] = BEZEL
    for x, y in cells:
        case.v.pop((x, y, TOP), None)
        case.v[(x, y, TOP - 1)] = SOCKET
    led = Fig((W, D, Z))
    for x, y in cells:
        led.put((x, y, TOP), LAMP)
        led.put((x, y, TOP + 1), LAMP)
    return (name, led)


def key(case, name, x0, color, sign=None):
    """A slim key standing in its well, three voxels over the top, its
    edges beveled a voxel; `sign` (rows) painted on its top."""
    for x in range(x0, x0 + KEY_W):
        for y in range(KEY_Y, KEY_Y + KEY_D):
            case.v.pop((x, y, TOP), None)
    b = Fig((W, D, Z))
    b.rbox((x0, KEY_Y, TOP), (x0 + KEY_W - 1, KEY_Y + KEY_D - 1, TOP + 3), color, bevel=1)
    if sign:
        sw, sh = len(sign[0]), len(sign)
        paint_rows(b, sign, x0 + (KEY_W - sw) // 2, KEY_Y + (KEY_D + sh) // 2 - 1, PAINT, z=TOP + 3)
    return (name, b)


def underline(name, x0):
    """A white bar on the band in front of a key, the cursor's mark: kept
    inside the case (hidden) until a pose lifts it four voxels onto the
    band (`NAME_mark`)."""
    bar = Fig((W, D, Z))
    for x in range(x0 + 3, x0 + KEY_W - 3):
        for y in (KEY_Y - 4, KEY_Y - 3):
            bar.put((x, y, TOP - 3), MARK)
    return (name + "_mark", bar)


def sign_part(name, rows, x0, z):
    """A sign on a key's top, in its middle, a voxel thick: standing on it,
    or kept inside it until a pose raises it."""
    s = Fig((W, D, Z))
    sw, sh = len(rows[0]), len(rows)
    paint_rows(s, rows, x0 + (KEY_W - sw) // 2, KEY_Y + (KEY_D + sh) // 2 - 1, PAINT, z=z, only_on=False)
    return (name, s)


def flush(case, name, cells):
    """A part made of the plate's own cells (the bare ones, not a painted
    sign): it looks like the plate, and a click or a drag on it is the
    control's it lies round."""
    base = Fig((W, D, Z))
    for x, y in cells:
        if case.v.get((x, y, TOP)) == PLATE:
            del case.v[(x, y, TOP)]
            base.put((x, y, TOP), PLATE)
    return (name, base)


def knob(case, name, cx, cy, r=KNOB_R):
    """A round knob `r` * 2 across and low, a thin skirt under it; its
    pointer a part of its own (`NAME_ptr`, lit while the pointer is over
    the knob), a white line from near the middle to the edge, pointing back
    at rest (the middle of the sweep). Round it a flush plate (`NAME_base`)
    with marks two voxels long at the ends, the middle and the quarters."""
    base = flush(case, name + "_base", disc(cx, cy, r + 4.5))
    for a in (-135, -90, -45, 0, 45, 90, 135):
        t = math.radians(a)
        for k in (0, 1):
            d = r + TICK - 1 + k
            p = (math.floor(cx + d * math.sin(t)), math.floor(cy + d * math.cos(t)), TOP)
            if p in base[1].v:
                base[1].v[p] = PAINT
    k = Fig((W, D, Z))
    tall = 5 if r > KNOB_R else 4
    for x, y in disc(cx, cy, r + 1):
        k.put((x, y, TOP + 1), KNOB)
    for z in range(TOP + 2, TOP + tall):
        for x, y in disc(cx, cy, r):
            k.put((x, y, z), KNOB)
    for x, y in disc(cx, cy, r - 1):
        k.put((x, y, TOP + tall), KNOB)
    ptr = Fig((W, D, Z))
    for (x, y, z) in list(k.v):
        if z == TOP + tall and cx - 1 <= x <= cx and y + 0.5 - cy > 1:
            del k.v[(x, y, z)]
            ptr.put((x, y, z), MARK)
    return [base, (name, k), (name + "_ptr", ptr)]


def switch(case, name, cx, cy):
    """A toggle switch tipping across the view: a round base (`XX_mount`)
    standing on the plate, a lever on it, square and thick, its tip a voxel
    thicker each way, upright in the file and leaned left (this voice) or
    right (the other) by a pose's roll about its foot. Round it, between
    the column's grooves, a flush plate in the plate's color (`XX_plate`)
    takes a click too; the grooves stay whole."""
    code = name.replace("_switch", "")
    left = cx - (COL - 1) // 2 - 1
    plate = flush(case, code + "_plate",
                  [(x, y) for x in range(left + 1, left + COL) for y in range(cy - 10, cy + 10)])
    mount = Fig((W, D, Z))
    for x, y in disc(cx, cy, MOUNT_R):
        mount.put((x, y, TOP + 1), KNOB)
    for x, y in disc(cx, cy, MOUNT_R - 1):
        mount.put((x, y, TOP + 2), KNOB)
    lever = Fig((W, D, Z))
    half = LEVER_W // 2
    lever.box((cx - half, cy - half, TOP + 3), (cx + half - 1, cy + half - 1, TOP + 2 + LEVER_L), LEVER)
    lever.rbox((cx - half - 1, cy - half - 1, TOP + 3 + LEVER_L), (cx + half, cy + half, TOP + 5 + LEVER_L),
               LEVER, bevel=1)
    return [plate, (code + "_mount", mount), (name, lever)]


# The voice selector: its dial, the ring of lamps, the voices' signs round
# them (their middles at SIGN_R); positions from the front left round to the
# front right, clockwise seen from above.
DIAL_R, LAMP_RING, SIGN_R, SWEEP = 12, 22, 36, 300
SIGN_TOP = 8                  # the signs this much further out at the top only
                              # (cos^4 of the angle from the back): there they
                              # stand side by side, at the sides above one another


def ring_places(cx, cy):
    """Each position's angle, its lamp's middle and its sign's middle."""
    out = []
    for i in range(len(SELECTOR_SIGNS)):
        a = math.radians(-SWEEP / 2 + i * SWEEP / (len(SELECTOR_SIGNS) - 1))
        s, c = math.sin(a), math.cos(a)
        r = SIGN_R + SIGN_TOP * max(0.0, c) ** 4
        out.append(((cx + LAMP_RING * s, cy + LAMP_RING * c), (cx + r * s, cy + r * c)))
    return out


def ring_extent():
    """The selector's signs' reach from its middle: left, right, front, back."""
    xs, ys = [], []
    for (_, (sx, sy)), name in zip(ring_places(0, 0), SELECTOR_SIGNS):
        w = text_width(name)
        xs += [round(sx - w / 2), round(sx - w / 2) + w - 1]
        ys += [round(sy) + 4 - (LETTER - 1), round(sy) + 4]
    return min(xs), max(xs), min(ys), max(ys)


def selector(case, cx, cy):
    """The dial with a long white pointer reaching out over the plate to a
    ring of twelve small lamps (`sel_1` .. `sel_12`, the one chosen lit),
    each voice's sign outside its lamp."""
    parts = []
    for i, ((lx, ly), (sx, sy)) in enumerate(ring_places(cx, cy)):
        parts.append(lamp(case, f"sel_{i + 1}", lx, ly, r=2.1))
        paint(case, SELECTOR_SIGNS[i], sx, round(sy) + 4, color=ACCENT_SIGN if i == 0 else PAINT)
    dial = Fig((W, D, Z))
    for x, y in disc(cx, cy, DIAL_R + 1):
        dial.put((x, y, TOP + 1), KNOB)
    for z in range(TOP + 2, TOP + 6):
        for x, y in disc(cx, cy, DIAL_R):
            dial.put((x, y, z), KNOB)
    for x, y in disc(cx, cy, DIAL_R - 1):
        dial.put((x, y, TOP + 6), KNOB)
    ptr = Fig((W, D, Z))
    for (x, y, z) in list(dial.v):
        if z == TOP + 6 and cx - 1 <= x <= cx and y + 0.5 - cy > 1:
            del dial.v[(x, y, z)]
    for y in range(cy + 1, cy + LAMP_RING - 3):
        for x in (cx - 1, cx):
            ptr.put((x, y, TOP + 6), MARK)
    return parts + [("selector", dial), ("selector_ptr", ptr)]


# A digit's segments: which cells of its DIGIT_W by DIGIT_H box (x from its
# left, y from its bottom) each holds; the corners left free, so the
# segments stand apart as on a display of the time.
SEGMENTS = {
    "a": [(x, y) for x in range(SEG, DIGIT_W - SEG) for y in range(DIGIT_H - SEG, DIGIT_H)],
    "b": [(x, y) for x in range(DIGIT_W - SEG, DIGIT_W) for y in range(DIGIT_H // 2 + 1, DIGIT_H - SEG)],
    "c": [(x, y) for x in range(DIGIT_W - SEG, DIGIT_W) for y in range(SEG, DIGIT_H // 2 - 1)],
    "d": [(x, y) for x in range(SEG, DIGIT_W - SEG) for y in range(SEG)],
    "e": [(x, y) for x in range(SEG) for y in range(SEG, DIGIT_H // 2 - 1)],
    "f": [(x, y) for x in range(SEG) for y in range(DIGIT_H // 2 + 1, DIGIT_H - SEG)],
    "g": [(x, y) for x in range(SEG, DIGIT_W - SEG) for y in range(DIGIT_H // 2 - 1, DIGIT_H // 2 + 1)],
}
DISPLAY_W = 3 * DIGIT_W + 2 * DIGIT_GAP + 2 * WINDOW_PAD
DISPLAY_H = DIGIT_H + 2 * WINDOW_PAD


def display(case, x0, cy):
    """The tempo's display: a dark window a voxel deep in the top (its
    floor the part `display`), three digits of seven segments lying in it,
    the first digit the hundreds. Each segment twice: `seg_1_a` .. `seg_3_g`
    in a faint dark red, always there (so a lit 1 stands in its place in
    the digit), and on it `lit_1_a` .. `lit_3_g` in the lamps' red, a voxel
    over it, sunk under the window's floor while the segment is dark."""
    y0 = cy - DISPLAY_H // 2
    window = Fig((W, D, Z))
    for x in range(x0, x0 + DISPLAY_W):
        for y in range(y0, y0 + DISPLAY_H):
            case.v.pop((x, y, TOP), None)
            case.v.pop((x, y, TOP - 1), None)
            window.put((x, y, TOP - 1), WINDOW)
    parts = [("display", window)]
    for d in range(3):
        left = x0 + WINDOW_PAD + d * (DIGIT_W + DIGIT_GAP)
        bottom = y0 + WINDOW_PAD
        for name, cells in SEGMENTS.items():
            seg, lit = Fig((W, D, Z)), Fig((W, D, Z))
            for x, y in cells:
                seg.put((left + x, bottom + y, TOP), SEG_OFF)
                lit.put((left + x, bottom + y, TOP + 1), LAMP)
            parts += [(f"seg_{d + 1}_{name}", seg), (f"lit_{d + 1}_{name}", lit)]
    return parts


def heading(case, code, left):
    """The column's heading (its lamp and name) as a flush part of its own
    (`CODE_head`, every cell of it as painted): the pointer over the name
    or the dark lamp names the column."""
    head = Fig((W, D, Z))
    for x in range(left + 1, left + COL):
        for y in range(HEAD - LETTER + 1, HEAD + 1):
            for z in (TOP, TOP - 1):
                c = case.v.get((x, y, z))
                if c is not None and (z == TOP or c == SOCKET):
                    del case.v[(x, y, z)]
                    head.put((x, y, z), c)
    return (code + "_head", head)


def machine():
    if figures.kept(GAME / "machine.vvx"):
        return
    case = case_shell()
    parts, knobs = [], []
    # The columns, a groove at each one's left from the band to the names (the first parts the left
    # panel from the voices).
    for i, (code, name, shaping, other) in enumerate(COLUMNS):
        left = LEFT + i * COL
        middle = left + 1 + (COL - 1) // 2
        groove(case, [(left, y) for y in range(BAND + 1, HEAD + 1)])
        # Its name, a lamp before it (lit while the selector names it), the
        # two centred in the column together.
        group = 5 + 2 + text_width(name)
        start = middle - group / 2
        parts.append(lamp(case, "led_" + code, start + 2.5, HEAD - 4, r=2.1))
        paint(case, name, None, HEAD, left=round(start) + 7)
        parts.append(heading(case, code, left))
        for row, (word, label) in enumerate([("level", "LVL")] + shaping):
            cy = ROW_Y[2 - row]
            paint(case, label, middle, label_top(cy))
            knobs.append((f"{code}_{word}", middle, cy, KNOB_R))
        if other:
            # The switch in the front row, its middle on the knobs' line,
            # its two signs over it on the labels' line, the pair centred.
            cy = ROW_Y[0]
            pair = text_width(name) + PAIR_GAP + text_width(other)
            x = round(middle - pair / 2)
            paint(case, name, None, label_top(cy), left=x)
            paint(case, other, None, label_top(cy), left=x + text_width(name) + PAIR_GAP)
            parts.extend(switch(case, f"{code}_switch", middle, cy))
    # The left panel holds the selector alone, centred: across between the
    # margin and the first groove's padding, deep between the band's groove
    # and the columns' names' top line, the space before and behind it
    # looking equal (the one behind wider in voxels, by the perspective).
    lo_x, hi_x, lo_y, hi_y = ring_extent()
    sel_x = round(PANEL_MID - (lo_x + hi_x + 1) / 2)
    free = (BAND + 1, HEAD)
    room = free[1] - free[0] + 1 - (hi_y - lo_y + 1)
    before = room / 2
    for _ in range(4):
        after = room - before
        behind = 1 + PERSPECTIVE * (free[1] - after / 2)
        ahead = 1 + PERSPECTIVE * (free[0] + before / 2)
        before = room * ahead / (ahead + behind)
    global PANEL_GAPS
    PANEL_GAPS = [round(before), room - round(before)]
    sel_y = free[0] + round(before) - lo_y
    # The top strip: the nameplate flush left on the margin; ACCENT, TEMPO
    # and VOLUME flush right on it, each its sign left of it, a margin
    # between each two.
    paint(case, NAME, None, NAMEPLATE, scale=2, left=M)
    # From the right: VOLUME (its sign left of it), the tempo (its knob, the
    # display left of it, one sign BPM for both), ACCENT; SIDE_GAP within a
    # group, a margin between groups.
    r = MASTER_R
    groups = [[("knob", "volume"), ("sign", "VOL", PAINT)],
              [("knob", "tempo"), ("display",), ("sign", "BPM", PAINT)],
              [("knob", "ac_level"), ("sign", "ACC", ACCENT_SIGN)]]
    right = W - 1 - M
    for group in groups:
        for item in group:
            if item[0] == "knob":
                cx = right - (math.ceil(r + TICK) - 1)
                knobs.append((item[1], cx, MASTER_Y, r))
                left = cx - math.ceil(r + TICK)
            elif item[0] == "sign":
                left = right - text_width(item[1]) + 1
                paint(case, item[1], None, MASTER_Y + LETTER // 2, left=left, color=item[2])
            else:
                left = right - DISPLAY_W + 1
                parts.extend(display(case, left, MASTER_Y))
            right = left - 1 - SIDE_GAP
        right += SIDE_GAP - M
    # Every sign is painted: now the knobs, their flush plates round them.
    parts.extend(selector(case, sel_x, sel_y))
    for name, cx, cy, r in knobs:
        parts.extend(knob(case, name, cx, cy, r))
    # The band: START, then A and B, then the four groups of steps, a
    # group's gap between each two, the whole row a margin from either edge.
    groups = [[("start", START, None)], [("pattern_a", PATTERN, LETTERS["A"]), ("pattern_b", PATTERN, LETTERS["B"])]]
    groups += [[(f"step_{4 * g + k + 1}", STEPS[g], None) for k in range(4)] for g in range(4)]
    keys = sum(len(g) for g in groups)
    width = keys * KEY_W + (keys - len(groups)) * KEY_GAP + (len(groups) - 1) * GROUP_GAP
    x = M + (W - 2 * M - width) // 2
    for g in groups:
        for name, color, sign in g:
            parts.append(key(case, name, x, color, sign))
            parts.append(underline(name, x))
            lamp_name = "led_" + name.replace("step_", "").replace("pattern_", "")
            parts.append(lamp(case, lamp_name, x + KEY_W / 2, LAMP_Y))
            if name == "start":
                parts.append(sign_part("start_play", PLAY_SIGN, x, TOP + 4))
                # The stop sign rests inside the key (the two cannot share
                # cells): main.lua raises it while the machine runs.
                parts.append(sign_part("start_stop", STOP_SIGN, x, TOP + 1))
            x += KEY_W + KEY_GAP
        x += GROUP_GAP - KEY_GAP
    figures.save_parts(GAME / "machine.vvx", [("case", case)] + parts)
    margins(case, parts)


def margins(case, parts):
    """Prints how far the content (every part but the flush plates, every
    sign and lamp bezel painted on the case) lies from each edge."""
    cells = [p for p, c in case.v.items() if c in (PAINT, BEZEL, ACCENT_SIGN) and p[2] == TOP]
    for name, f in parts:
        if name.endswith("_head"):
            cells += [p for p, c in f.v.items() if c in (PAINT, BEZEL)]
        elif name.endswith("_base") or name.endswith("_plate"):
            cells += [p for p, c in f.v.items() if c == PAINT]
        elif not name.endswith("_mark"):
            cells += list(f.v)
    xs, ys = [p[0] for p in cells], [p[1] for p in cells]
    print(f"case {W} x {D}; margins: left {min(xs)}, right {W - 1 - max(xs)}, "
          f"front {min(ys)}, back {D - 1 - max(ys)} (M = {M}, wider further back for the perspective)")
    gaps = [ROW_Y[0] - front(KNOB_R) - BAND - 1]
    for k in range(2):
        gaps.append(ROW_Y[k + 1] - front(KNOB_R) - label_top(ROW_Y[k]) - 1)
    gaps += [HEAD - LETTER + 1 - label_top(ROW_Y[2]) - 1, STRIP_Y - HEAD - 1]
    print(f"voices' gaps (band to row 1, row to row, row to row, LVL to names, names to the top strip): {gaps}")
    print(f"left panel's gaps (band's groove to the ring, the ring to the names' line): {PANEL_GAPS}")
    print(f"top strip: rows {STRIP_Y}..{STRIP_Y + STRIP - 1} ({STRIP} deep: nameplate {2 * LETTER}, masters {MASTER}); "
          f"back margin {M_BACK}")
    print(f"rows at y {ROW_Y}; band's groove y {BAND}; names' top {HEAD}; "
          f"nameplate's top {NAMEPLATE}; first groove x {LEFT}, column {COL}")


# The table: the level's own voxels, a flat land a voxel thick (`[terrain]`
# of one height), meshed as the world's chunks: as a model of 560 by 290
# voxels it cost the small GPU 8 ms of its 17 at 720.
TW, TD, SPACING = 580, 340, 10
LOOK_AHEAD = 14               # the camera looks this far in front of the case's
                              # middle: more table under the case at the
                              # screen's bottom, over the line of words


def level():
    path = GAME / "levels" / "studio.toml"
    if figures.kept(path):
        return
    mx, my = TW // 2, TD // 2
    row = " ".join(["1"] * (TW // SPACING))
    heights = "\n".join([row] * (TD // SPACING))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f'''format = "voxcade level 1"
size = [{TW}, {TD}, 40]

# The table: a flat land a voxel thick.
[terrain]
spacing = {SPACING}
at = [0, 0, 0]
heights = """
{heights}
"""

[[terrain.material]]
color = "#{TABLE[0]:02x}{TABLE[1]:02x}{TABLE[2]:02x}"

[[thing]]
name = "machine"
model = "machine.vvx"
at = [{mx}, {my}, 1]
facing = "north"

[[place]]
name = "start"
at = [{mx}, {my - LOOK_AHEAD}, 1]
''')
    print(f"{path}: written")


if __name__ == "__main__":
    machine()
    level()

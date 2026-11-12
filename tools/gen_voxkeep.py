#!/usr/bin/env python3
"""Makes Voxkeep (games/voxkeep), a jump & run in a castle at night, as
voxels from the start: no picture drawn out. A cell of the level is 10
voxels each way; the blocks (blocks/*.vvx) and the figures are built here
voxel by voxel, the rooms from the texts below (levels/*.toml: three, a
demo of going from level to level).

It makes what is missing and keeps every file there: once made, the
blocks, the room and the figures are the user's, shaped in the editors
(a file deleted is made again). The figures are small and plain on
purpose, built from boxes: the user found them more charming than a try
at 16-bit size, and they are easy to make. A probe for now: one room,
to judge the look. Plain Python, no numpy."""

import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gen_sokovox import write_vox  # noqa: E402
from vvx import write_level  # noqa: E402

GAME = Path(__file__).resolve().parent.parent / "games" / "voxkeep"
N = 10


def kept(path):
    """Whether `path` is there already: the user's then, kept as it is."""
    if path.exists():
        print(f"{path}: kept (there already: yours, edited in the editor)")
        return True
    return False

# The rooms, row by row from the top: # stone, = a wooden ledge, H a
# ladder, ^ spikes; things: @ the hero, * a gem, D the door, s a slime, b a
# bat, t a torch on the wall behind. Everything stays in one lane: a ledge
# 2 cells over the one below the hero jumps onto, one 3 cells over with
# a second jump in the upper half of the first (he grabs no ledges, and
# no plank or ledge lets him through from below);
# a ladder through a floor takes him onto its top. A floor three cells
# under the next leaves no room to jump far: spikes there lie one cell
# wide. The door opens once every gem is taken and leads to the next room.
HALL = [
    "########################",
    "#......................#",
    "#..*.....t..........tD.#",
    "##########H.......######",
    "#.........H..b.........#",
    "#.t.....*.H......t.....#",
    "#.......##########.....#",
    "#.*....................#",
    "######.................#",
    "#............==========#",
    "#.......t...........*..#",
    "#@......^^.....s.......#",
    "########################",
]

# The crypt: two floors of tall rooms, bats in them: under one while it is
# up, over the spikes while the other is; in at the bottom left, up the
# ladder, out at the top left.
CRYPT = [
    "########################",
    "#......................#",
    "#......................#",
    "#..t......t.......t....#",
    "#......b.......b.......#",
    "#.D....*...^^....*.....#",
    "#####################H##",
    "#....................H.#",
    "#.........b..........H.#",
    "#..t.......t.........H.#",
    "#......b.............H.#",
    "#@..*.....^^..*.....sH.#",
    "########################",
]

# The tower: a pit of spikes crossed on two ferries that meet in the
# middle for a moment; skulls hop on the floor above.
TOWER = [
    "########################",
    "#......................#",
    "#......................#",
    "#..t.......t.......t...#",
    "#......................#",
    "#.D......*..k....k.....#",
    "######################H#",
    "#.....................H#",
    "#..t...........t......H#",
    "#@.........*..........H#",
    "#####..............#####",
    "#####^^^^^^^^^^^^^^#####",
    "########################",
]

# The levers: the one by the start sets the lift going, up there the other
# brings two slabs out of the floor over the spikes (main.lua's LINKS).
LEVERS = [
    "########################",
    "#......................#",
    "#......................#",
    "#..t...........t.......#",
    "#..........b...........#",
    "#*L........D.....*.....#",
    "#####......#########H###",
    "#...................H..#",
    "#..t........t.......H..#",
    "#@L.*.......*.......H..#",
    "########........########",
    "########^^^^^^^^########",
    "########################",
]

# The crates: the hatch over the shaft down shuts when nothing lies on
# the plate, and the plate is too far from it to run: the crate pushed off
# the ledge lands on the plate and holds it down.
CRATES = [
    "########################",
    "#......................#",
    "#*@..C.................#",
    "#######..t........t....#",
    "#......................#",
    "#......P.....*.........#",
    "##################..####",
    "#......................#",
    "#..t........t.......t..#",
    "#......................#",
    "#......................#",
    "#.D..*......k...*......#",
    "########################",
]

# The keep, the last room: a ferry over the spikes, a skull, the lever for
# the lift up; above, bats, and the crate to push onto the plate (a step
# stops it there) that holds open the hatch down to the door's chamber.
KEEP = [
    "########################",
    "#......................#",
    "#..t........t.......t..#",
    "#......................#",
    "#.....b.........b......#",
    "#.......#P..C...*......#",
    "###..################..#",
    "#....#.................#",
    "#....#..t.........t....#",
    "#*D..#@*.....*....k.L..#",
    "########........########",
    "########^^^^^^^^########",
    "########################",
]

# Platforms, which lie in walls and floors or over pits: their kind and
# where (x and z in voxels: their middle across, their underside).
PLATFORMS = {
    "03_tower": [("ferry", 60, 27), ("ferry_left", 180, 27)],
    "04_levers": [("lift", 60, 27), ("slab", 70, 27), ("slab_left", 170, 27)],
    "05_crates": [("hatch", 190, 67)],
    "06_keep": [("ferry", 90, 27), ("lift", 220, 27), ("hatch", 40, 67)],
}

ROOMS = {"01_hall": HALL, "02_crypt": CRYPT, "03_tower": TOWER, "04_levers": LEVERS, "05_crates": CRATES, "06_keep": KEEP}


def shade(rgb, k):
    return tuple(max(0, min(255, int(c * k))) for c in rgb)


# ---------------------------------------------------------------- blocks

def block(fill):
    """A block's voxels: fill(x, y, z) gives a colour or None; y 0 is its
    front."""
    voxels, colours = [], {}
    for z in range(N):
        for y in range(N):
            for x in range(N):
                c = fill(x, y, z)
                if c is None:
                    continue
                i = colours.setdefault(c, len(colours) + 1)
                voxels.append((x, y, z, i))
    return voxels, {i: c for c, i in colours.items()}


def save_block(name, fill):
    if kept(GAME / "blocks" / f"{name}.vvx"):
        return
    voxels, colours = block(fill)
    write_vox(GAME / "blocks" / f"{name}.vvx", (N, N, N), [voxels], colours)


# The wall behind: bricks 5 high and 10 long, every course shifted by half,
# a groove of mortar a voxel deep between them; each brick a shade of its
# own (the same in every block, by where it lies).
WALL = [(58, 62, 82), (64, 67, 88), (54, 58, 77)]
MORTAR = (36, 37, 50)


def brick_of(x, z):
    """Which brick a place on the wall belongs to (across, course), and
    whether it is mortar."""
    course = z // 5
    shift = 5 if course % 2 else 0
    u = (x + shift) % N
    return ((x + shift) // N, course), u == 0 or z % 5 == 0


def wall(x, y, z):
    if y > 2:
        return None
    (b, course), mortar = brick_of(x, z)
    if mortar:
        return MORTAR if y >= 1 else None
    return WALL[(b * 7 + course * 3) % 3]


# The stone the floor and ledges are built of: big blocks of warm grey,
# two courses a cell, grooves between them on the front, a worn lighter
# top to walk on.
STONE = [(118, 108, 96), (126, 116, 102), (110, 101, 90)]
STONE_TOP = (148, 138, 120)
STONE_GROOVE = (74, 67, 60)


def stone(x, y, z):
    (b, course), mortar = brick_of(x, z)
    if z == N - 1:
        return STONE_TOP
    if y == 0 and mortar:
        return None  # the groove: the front a voxel back
    if y == 1 and mortar:
        return STONE_GROOVE
    return STONE[(b * 5 + course) % 3]


# A wooden ledge: three boards along the room, 2 voxels thick, an iron
# bracket under it at the back holding it to the wall.
WOOD = [(132, 86, 48), (142, 94, 54), (124, 80, 44)]
SEAM = (82, 52, 30)
IRON = (58, 58, 64)


def plank(x, y, z):
    if z >= N - 2:
        if y in (3, 6):
            return SEAM if z == N - 1 else WOOD[0]
        return WOOD[(y // 3) % 3] if z == N - 1 else shade(WOOD[(y // 3) % 3], 0.8)
    if x in (2, 7) and y >= 7 and z >= N - 5:
        return IRON  # the brackets: down the back
    if x in (2, 7) and y >= 7 - (N - 3 - z) and z >= N - 5 and y >= 5:
        return IRON  # and slanting forward under the boards
    return None


# A ladder at the back of the lane, just before the wall: two rails and a
# rung every 5 voxels.
RAIL = (112, 74, 42)
RUNG = (150, 104, 60)


def ladder(x, y, z):
    if y not in (8, 9):
        return None
    if x in (1, 8):
        return RAIL
    if 2 <= x <= 7 and z % 5 == 2:
        return RUNG
    return None


# Spikes: an iron plate and two rows of three points, each a step
# pyramid 3 across at its foot, light at its tip.
PLATE = (66, 68, 76)
SPIKE = (150, 156, 168)
TIP = (226, 230, 238)


def spikes(x, y, z):
    if z == 0:
        return PLATE if 1 <= y <= 8 else None
    for cx in (2, 5, 8):
        for cy in (3, 6):
            d = max(abs(x - cx), abs(y - cy))
            height = 2 if d == 1 else 7 if d == 0 else 0
            if 1 <= z <= height:
                return TIP if z >= 6 else SPIKE
    return None


BLOCKS = [("wall", "decor", wall), ("stone", "solid", stone), ("plank", "solid", plank),
          ("ladder", "ladder", ladder), ("spikes", "hazard", spikes)]
CELL = {"#": "stone", "=": "plank", "H": "ladder", "^": "spikes"}


# ---------------------------------------------------------------- figures

# Colours of MagicaVoxel's glass material: how much light they let
# through (the engine draws them see-through) and how rough they are (the
# smoother, the more they mirror the sky and glint in the light).
GLASS = {}


def model(name, size, frames):
    """frames: lists of (x, y, z, colour); colours given by value, those in
    GLASS see-through."""
    if kept(GAME / f"{name}.vvx"):
        return
    index, palette = {}, {}
    out = []
    for f in frames:
        vox = {}
        for x, y, z, c in f:
            if 0 <= x < size[0] and 0 <= y < size[1] and 0 <= z < size[2]:
                vox[(x, y, z)] = c
        frame = []
        for (x, y, z), c in vox.items():
            i = index.setdefault(c, len(index) + 1)
            palette[i] = c
            frame.append((x, y, z, i))
        out.append(frame)
    glass = {i: {"_type": "_glass", "_trans": str(GLASS[c][0]), "_rough": str(GLASS[c][1])} for c, i in index.items() if c in GLASS}
    write_vox(GAME / f"{name}.vvx", size, out, palette, glass)


def box(x0, x1, y0, y1, z0, z1, c):
    return [(x, y, z, c) for x in range(x0, x1 + 1) for y in range(y0, y1 + 1) for z in range(z0, z1 + 1)]


# The hero: a small adventurer 6 across, 7 from back to front, 14 high,
# facing +y (the game turns him to where he runs, so he is seen from his
# side): boots, trousers, a red tunic with a belt, a face with an eye on
# either side, a teal hood with its tip hanging behind, arms that swing.
BOOT, TROUSERS, TUNIC, BELT, BUCKLE = (62, 42, 30), (52, 56, 74), (176, 54, 46), (70, 46, 30), (226, 196, 90)
SKIN, EYE, HOOD = (236, 186, 146), (28, 24, 32), (40, 150, 150)


def legs(z0, stride, tuck=False):
    """stride: how far the near leg is forward (the far one as far back);
    tuck: knees drawn up. The near leg is x 3..4 (the camera sees +x)."""
    v = []
    for (xa, xb), s in (((1, 2), -stride), ((3, 4), stride)):
        y = 3 + s
        if tuck:
            v += box(xa, xb, 3, 3, z0 + 3, z0 + 4, TROUSERS) + box(xa, xb, 3, 4, z0 + 2, z0 + 2, BOOT)
        else:
            v += box(xa, xb, 3, 3, z0 + 3, z0 + 4, TROUSERS)
            v += box(xa, xb, y, y, z0 + 2, z0 + 2, TROUSERS)
            v += box(xa, xb, y, y + 1, z0 + 0, z0 + 1, BOOT)
    return v


def torso_and_head(z0):
    """The body and the head, without arms."""
    v = box(1, 4, 2, 4, z0 + 5, z0 + 8, TUNIC)
    v += box(1, 4, 2, 4, z0 + 5, z0 + 5, BELT)
    v += [(2, 5, z0 + 5, BUCKLE), (3, 5, z0 + 5, BUCKLE)]
    v += box(1, 4, 5, 5, z0 + 6, z0 + 8, TUNIC)
    # Head: a face to the front (an eye at either side of it), the hood
    # over it and down the back, its tip hanging behind.
    v += box(1, 4, 2, 5, z0 + 9, z0 + 11, SKIN)
    v += [(1, 5, z0 + 10, EYE), (4, 5, z0 + 10, EYE)]
    v += box(1, 4, 1, 5, z0 + 12, z0 + 13, HOOD)
    v += box(0, 0, 1, 3, z0 + 9, z0 + 12, HOOD) + box(5, 5, 1, 3, z0 + 9, z0 + 12, HOOD)
    v += box(1, 4, 1, 1, z0 + 9, z0 + 11, HOOD)
    v += box(2, 3, 0, 0, z0 + 8, z0 + 12, HOOD)
    return v


def arm(x, y, z_hand, z_shoulder=8):
    """An arm at x from the shoulder to the hand (above or below it), the
    hand at y."""
    lo, hi = sorted((z_hand, z_shoulder))
    v = box(x, x, 3, 3, z_shoulder - 1, z_shoulder, TUNIC)
    v += [(x, y, z, TUNIC) for z in range(lo, hi + 1) if z not in (z_shoulder - 1, z_shoulder) or y != 3]
    v += [(x, y, z_hand, SKIN)]
    return v


def hero_frame(stride, swing, tuck=False):
    """Running and standing: the near arm swings forward by `swing`, the far
    one against it."""
    v = legs(0, stride, tuck) + torso_and_head(0)
    for x, s in ((0, -swing), (5, swing)):
        v += box(x, x, 3, 3, 7, 8, TUNIC) + box(x, x, 3 + s, 3 + s, 5, 6, TUNIC) + [(x, 3 + s, 4, SKIN)]
    return v


def hanging():
    """From a ledge's edge: both arms straight up in front of the head,
    the hands at 15 (the game hangs him with his head 2 under the top)."""
    v = legs(0, 0) + torso_and_head(0)
    for x in (0, 5):
        v += box(x, x, 3, 3, 7, 8, TUNIC) + box(x, x, 4, 4, 9, 14, TUNIC) + [(x, 4, 15, SKIN)]
    return v


def pulling(k):
    """Pulling himself up: arms bending (0), elbows over the edge with a
    knee up (1), pushing up off his hands (2)."""
    if k == 0:
        v = legs(0, 0) + torso_and_head(0)
        for x in (0, 5):
            v += box(x, x, 3, 3, 7, 8, TUNIC) + box(x, x, 4, 4, 9, 12, TUNIC) + [(x, 5, 13, SKIN)]
    elif k == 1:
        v = legs(0, 1, tuck=True) + torso_and_head(0)
        for x in (0, 5):
            v += box(x, x, 3, 4, 8, 8, TUNIC) + box(x, x, 5, 5, 7, 8, TUNIC) + [(x, 6, 7, SKIN)]
    else:
        v = legs(0, 0, tuck=True) + torso_and_head(0)
        for x in (0, 5):
            v += box(x, x, 4, 4, 5, 8, TUNIC) + [(x, 5, 4, SKIN)]
    return v


def climbing(k):
    """On a ladder, seen from behind (the game turns him to it): one hand
    up on a rung, the other lower, the leg on that side stepping up."""
    v = torso_and_head(0)
    up, down = (0, 5) if k == 0 else (5, 0)
    v += box(up, up, 4, 4, 7, 13, TUNIC) + [(up, 5, 14, SKIN)]
    v += box(down, down, 4, 4, 7, 9, TUNIC) + [(down, 5, 10, SKIN)]
    for (xa, xb), lift in (((1, 2), 2 if up == 0 else 0), ((3, 4), 2 if up == 5 else 0)):
        v += box(xa, xb, 3, 3, lift + 2, 4, TROUSERS) + box(xa, xb, 3, 4, lift, lift + 1, BOOT)
    return v


def hero():
    frames = [
        hero_frame(0, 0), hero_frame(0, 0),                          # 0, 1 idle
        hero_frame(1, -1), hero_frame(0, 0), hero_frame(-1, 1), hero_frame(0, 0),   # 2..5 run
        hero_frame(1, -1, True),                                     # 6 jump
        hero_frame(0, 1, True),                                      # 7 fall
        hanging(),                                                   # 8 hang
        pulling(0), pulling(1), pulling(2),                          # 9..11 pull
        climbing(0), climbing(1),                                    # 12, 13 climb
    ]
    model("hero", (6, 7, 16), frames)


# A slime: a green dome 9 across and 8 long, one green (the engine lights
# it), its eyes on both sides near the front; it squashes and stretches as
# it creeps.
SLIME = (70, 168, 72)


WHITE = (240, 240, 236)


def slime_frame(squash):
    v, tops = [], {}
    h = 6 - squash
    for x in range(9):
        for y in range(8):
            dx, dy = (x - 4) / 4.5, (y - 3.5) / 4.0
            r = dx * dx + dy * dy
            if r > 1.0:
                continue
            top = max(1, round(h * (1.0 - r) ** 0.5))
            tops[(x, y)] = top
            for z in range(top):
                v.append((x, y, z, SLIME))
    # Two big eyes on top near the front, their pupils looking ahead.
    for ex in (2, 5):
        z = max(tops[(ex, 5)], tops[(ex + 1, 5)])
        v += box(ex, ex + 1, 4, 5, z - 1, z, WHITE)
        v += box(ex, ex + 1, 6, 6, z, z, EYE)
    return v


def slime():
    model("slime", (9, 8, 8), [slime_frame(0), slime_frame(1)])


# A bat facing the camera, always (the user: bats look to the front): a
# small body with ears, its two red eyes on its face, the wings spread to
# either side, up in one frame, down in the other. Made facing +y (the
# kind's facing turns that to the camera).
BAT, BAT_WING, BAT_BONE, BAT_EYE = (150, 100, 172), (112, 72, 138), (86, 52, 108), (255, 80, 70)


def bat_frame(up):
    v = box(5, 7, 0, 2, 3, 6, BAT) + [(5, 1, 7, BAT), (7, 1, 7, BAT)]
    v += [(5, 2, 5, BAT_EYE), (7, 2, 5, BAT_EYE)]
    for side in (-1, 1):
        for k in range(1, 6):
            x = 6 + side * (1 + k)
            if up:
                lo, hi = 5 + k // 2, 5 + k
            else:
                lo, hi = 4 - k // 2, 5 - (k + 1) // 2
            v += box(x, x, 1, 1, lo, hi, BAT_BONE if k == 5 else BAT_WING)
    return v


def bat():
    model("bat", (13, 3, 11), [bat_frame(True), bat_frame(False)])


# A gem: a cut stone in the round, the same from every side: a square
# table on top, the crown widening to an eight-sided girdle 7 across, the
# pavilion down to a point. Ruby glass, one colour, a little see-through
# and smooth: it mirrors the sky and glints in the light as it turns
# (conf.lua: spin); bright enough to glow.
GEM = (240, 40, 80)
GLASS[GEM] = (0.25, 0.3)
GEM_LAYERS = [0, 1, 2, 3, 2, 1]   # how far out from the middle column, from the point up


def gem():
    v = []
    for z, r in enumerate(GEM_LAYERS):
        for x in range(-r, r + 1):
            for y in range(-r, r + 1):
                # cut the corners: eight sides, the table square
                if z < len(GEM_LAYERS) - 1 and abs(x) + abs(y) > r + r // 2:
                    continue
                v.append((3 + x, 3 + y, z, GEM))
    model("gem", (7, 7, 6), [v])


# The door out: an arch of stone around boards, shut and open (dark
# inside, a warm light beyond).
DOOR_WOOD, DOOR_DARK, DOOR_IRON, ARCH = (120, 76, 40), (86, 54, 30), (60, 60, 66), (140, 130, 114)


def door_frame(open_):
    v = []
    w, h = 12, 20
    for x in range(w):
        for z in range(h):
            # The arch's outline: a round top over straight sides.
            cx = (w - 1) / 2
            inside_r = ((x - cx) / (cx - 1.5)) ** 2 + ((z - (h - 7)) / 6.0) ** 2
            outer_r = ((x - cx) / (cx + 0.5)) ** 2 + ((z - (h - 7)) / 7.0) ** 2
            in_door = 2 <= x <= w - 3 and (z < h - 7 or inside_r <= 1.0)
            in_arch = z < h - 7 or outer_r <= 1.0
            if in_door:
                if open_:
                    v.append((x, 2, z, (24, 18, 22)))
                else:
                    c = DOOR_DARK if x % 3 == 1 else DOOR_WOOD
                    if z in (4, 13) and 2 <= x <= w - 3:
                        c = DOOR_IRON
                    v.append((x, 1, z, c))
                    v.append((x, 2, z, DOOR_DARK))
            elif in_arch:
                for y in range(3):
                    v.append((x, y, z, ARCH))
    # Facing +y as made: turn the front (y 0) to +y.
    return [(x, 2 - y, z, c) for x, y, z, c in v]


def door():
    model("door", (12, 3, 20), [door_frame(False), door_frame(True)])


# A torch on the wall: an iron plate and arm, a stick, a cup, a flame of
# two shades (bright: it glows), flickering between two shapes. Made facing
# +y, its back (y 0) against the wall.
STICK, FLAME, FLAME_CORE = (96, 62, 36), (255, 140, 30), (255, 232, 140)


def torch_frame(k):
    v = box(1, 3, 0, 0, 2, 5, IRON) + box(2, 2, 1, 1, 3, 3, IRON)
    v += box(2, 2, 2, 2, 2, 7, STICK) + box(1, 3, 1, 3, 7, 7, IRON)
    v += box(1, 3, 1, 3, 8, 9, FLAME) + box(2, 2, 2, 2, 8, 10, FLAME_CORE)
    v += [(2, 2, 11, FLAME), (2 if k else 1, 1 if k else 2, 11, FLAME), (2, 2, 12, FLAME) if k else (3, 2, 10, FLAME)]
    return v


def torch():
    model("torch", (5, 4, 13), [torch_frame(0), torch_frame(1)])


# The puzzles' things, made facing +y (the camera): a lever on the floor,
# leaning left (idle) or right (on); a plate in the floor, up (idle) or
# pressed (down); a crate the hero pushes; a stone slab (platforms: out of
# a wall, ferries, lifts); a wooden hatch (a trapdoor); a skull that hops.
# A plate pressed is gone into the floor (the user: not half seen under a
# crate).
KNOB, BRASS, BEAM = (200, 40, 40), (196, 160, 70), (88, 56, 32)
BONE, SOCKET = (226, 218, 196), (30, 26, 30)


def lever_frame(on):
    v = box(1, 5, 0, 2, 0, 1, IRON)
    stick = [(3, 2), (3, 3), (2, 4), (2, 5), (1, 6)]
    for x, z in stick:
        v.append((6 - x if on else x, 1, z, STICK))
    kx = 5 if on else 0
    v += box(kx, kx + 1, 1, 1, 7, 8, KNOB)
    return v


def lever():
    model("lever", (7, 3, 9), [lever_frame(False), lever_frame(True)])


def plate_frame(down):
    if down:
        return []                                     # pressed: gone into the floor
    return box(0, 9, 0, 7, 0, 0, IRON) + box(1, 8, 1, 6, 1, 2, BRASS)


def plate():
    model("plate", (10, 8, 3), [plate_frame(False), plate_frame(True)])


def crate():
    v = []
    for x in range(8):
        for y in range(8):
            for z in range(8):
                ends = sum(1 for c in (x, y, z) if c in (0, 7))
                if ends >= 2:
                    v.append((x, y, z, BEAM))           # the frame along its edges
                elif ends == 1:
                    # boards, a seam sunk in between every third
                    across = z if x in (0, 7) or y in (0, 7) else x
                    if across % 3 != 0 or z in (0, 7):
                        v.append((x, y, z, WOOD[0]))
                else:
                    v.append((x, y, z, WOOD[0]))
    # a brace across the front and the back
    for k in range(1, 7):
        for y in (0, 7):
            v.append((k, y, k, BEAM))
    model("crate", (8, 8, 8), [v])


def slab():
    v = [(x, y, z, STONE[0]) for x in range(20) for y in range(8) for z in range(3)
         if not ((x in (0, 19)) and (z == 0 or y in (0, 7)))]
    model("slab", (20, 8, 3), [v])


def hatch():
    v = []
    for x in range(20):
        for y in range(8):
            for z in range(3):
                if x % 5 == 4 and z == 2:
                    continue                          # a seam between the boards
                v.append((x, y, z, IRON if x in (3, 16) and z == 2 else DOOR_WOOD))
    model("hatch", (20, 8, 3), [v])


def skull_frame(jaw_down):
    v = box(0, 5, 1, 5, 2, 6, BONE)                   # the head
    v += box(1, 4, 2, 5, 0, 1 if not jaw_down else 0, BONE)   # the jaw
    for x in (0, 5):
        v += box(x, x, 4, 5, 3, 4, SOCKET)            # the sockets at its front corners: seen from the side too
    v += [(2, 5, 2, SOCKET), (3, 5, 2, SOCKET)]       # the nose
    if jaw_down:
        v = [c for c in v if not (c[2] == 1 and c[1] >= 2)]
    return v


def skull():
    model("skull", (6, 6, 7), [skull_frame(False), skull_frame(True)])


# ---------------------------------------------------------------- the room

def level(name, room):
    h, w = len(room), len(room[0])
    assert all(len(r) == w for r in room), f"{name}: every row of the room as long as the first"
    names = [b[0] for b in BLOCKS]
    cells = {}  # (x, y, z): (block, turn); y 0 the lane, 1 the wall
    things = []
    for z in range(h):
        row = room[h - 1 - z]
        for y in range(2):
            for x in range(w):
                ch = row[x]
                if y == 1:
                    cells[(x, y, z)] = (names.index("wall") + 1, 0)
                elif ch in CELL:
                    cells[(x, y, z)] = (names.index(CELL[ch]) + 1, 0)
        for x, ch in enumerate(row):
            mid = x * N + N // 2
            if ch == "@":
                things.append({"kind": "hero", "at": [mid, N // 2, z * N]})
            elif ch == "*":
                things.append({"kind": "gem", "at": [mid, N // 2, z * N + 2]})
            elif ch == "D":
                things.append({"kind": "door", "at": [mid, N - 2, z * N]})
            elif ch == "s":
                things.append({"kind": "slime", "at": [mid, N // 2, z * N]})
            elif ch == "b":
                things.append({"kind": "bat", "at": [mid, N // 2, z * N + 2]})
            elif ch == "t":
                things.append({"kind": "torch", "at": [mid, N - 3, z * N + 1]})
            elif ch in "kLPC":
                kind = {"k": "skull", "L": "lever", "P": "plate", "C": "crate"}[ch]
                things.append({"kind": kind, "at": [mid, N // 2, z * N]})
    for kind, x, z in PLATFORMS.get(name, []):
        things.append({"kind": kind, "at": [x, N // 2, z]})
    path = GAME / "levels" / f"{name}.toml"
    if not kept(path):
        path.parent.mkdir(exist_ok=True)
        write_level(path, N, (w, 2, h), [(n, k) for n, k, _ in BLOCKS], cells, things)


if __name__ == "__main__":
    random.seed(1)
    (GAME / "blocks").mkdir(parents=True, exist_ok=True)
    for name, _, fill in BLOCKS:
        save_block(name, fill)
    hero()
    slime()
    bat()
    gem()
    door()
    torch()
    lever()
    plate()
    crate()
    slab()
    hatch()
    skull()
    for name, room in ROOMS.items():
        level(name, room)

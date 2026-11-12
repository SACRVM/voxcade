#!/usr/bin/env python3
"""Makes BunnyFarm (games/bunnyfarm), a farming game with a tamagotchi at
its heart (docs/bunnyfarm.md), as voxels from the start: the blocks of
the farm (blocks/*.vvx, 16 voxels each way), the farm itself
(levels/01_farm.json: a fenced farm of 24 by 16 cells in a meadow, seen
from above at a slant), the figures and the props.

The second pass (2026-10-24, the user: the first looked careless): every
model built from clean forms with tools/figures.py (boxes with their
edges cut back, nothing a voxel thin that need not be, nothing loose),
calm colours a material, the light the engine's. Paths, the pond and the
fence are pieces that join up (a straight piece, a bend, a junction, an
end, each turned as the neighbours ask); the meadow and the farm are
dotted with flowers and tufts (decor blocks, walked through) and with
trees, bushes, rocks, hay, barrels and a scarecrow (things, placed in the
level: moved in the level editor).

It makes what is missing and keeps every file there: once made, the
blocks, the farm and the figures are the user's, shaped in the editors (a
file deleted is made again). Plain Python, no numpy."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import figures  # noqa: E402
from figures import Fig  # noqa: E402
from gen_sokovox import write_vox  # noqa: E402

GAME = Path(__file__).resolve().parent.parent / "games" / "bunnyfarm"
N = 16
figures.KEEP_EXISTING = True


def kept(path):
    """Whether `path` is there already: the user's then, kept as it is."""
    if path.exists():
        print(f"{path}: kept (there already: yours, edited in the editor)")
        return True
    return False


def noise(x, y, k=0):
    """A fixed scatter (the same every run): 0..99."""
    return ((x * 73856093) ^ (y * 19349663) ^ (k * 83492791)) % 100


def save(name, size, figs):
    figures.save(GAME / f"{name}.vvx", size, figs)


# ---------------------------------------------------------------- colours

GRASS = (124, 188, 86)
GRASS_PATCH = (136, 198, 94)       # patches of other grass, as a meadow has
GRASS_DARK = (88, 152, 66)         # tufts
DIRT = (140, 98, 62)
SOIL = (106, 72, 48)
FALLOW = (170, 136, 96)
SAND = (226, 198, 146)
PEBBLE = (180, 172, 160)
STONE = (158, 152, 144)
STONE_DARK = (122, 116, 110)
WATER = (82, 158, 216)
LILY = (96, 174, 84)
POST = (118, 82, 52)
RAIL = (172, 126, 80)
RED, WHITE, YELLOW, BLUE = (222, 70, 64), (246, 242, 232), (246, 206, 70), (96, 128, 222)
STEM = (92, 150, 64)
BLOSSOM = (244, 164, 190)

# ---------------------------------------------------------------- blocks


def block(fill):
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
    path = GAME / "blocks" / f"{name}.vvx"
    if kept(path):
        return
    voxels, colours = block(fill)
    write_vox(path, (N, N, N), [voxels], colours)


def turf(x, y, z, k=0):
    """The ground of a meadow: grass on top (patches of another grass,
    two by two), earth below."""
    if z == N - 1:
        return GRASS_PATCH if noise(x // 2, y // 2, k) < 12 else GRASS
    return GRASS if z == N - 2 else DIRT


def grass_a(x, y, z):
    return turf(x, y, z, 1)


def grass_b(x, y, z):
    return turf(x, y, z, 2)


def bed_edge(x, y):
    return x in (0, N - 1) or y in (0, N - 1)


def soil(x, y, z):
    """A tilled bed: a strip of grass round it (beds side by side read as
    beds), two ridges along it where the plants stand (x 4 and 11, y 4
    and 11), furrows between them a voxel deep."""
    if bed_edge(x, y):
        return turf(x, y, z, 3)
    if z == N - 1:
        return SOIL if 2 <= y <= 6 or 9 <= y <= 13 else None
    return SOIL if z >= N - 3 else DIRT


FALLOW_STONES = {(4, 5), (5, 5), (11, 10), (10, 3), (3, 12), (12, 12), (13, 12)}
FALLOW_WEEDS = {(7, 9), (8, 9), (8, 4)}


def fallow(x, y, z):
    """A bed not yet bought: dry earth, a few stones and weeds."""
    if bed_edge(x, y):
        return turf(x, y, z, 4)
    if z == N - 1:
        if (x, y) in FALLOW_STONES:
            return PEBBLE
        return GRASS_DARK if (x, y) in FALLOW_WEEDS else FALLOW
    return DIRT


# Pieces that join up: as made, each connects to its neighbours on the
# sides named (E +x, N +y, W -x, S -y); the level turns them.
ROTATIONS = {0: 0, 90: 17, 180: 3, 270: 18}   # anticlockwise seen from above (blocks::Turn)
ORDER = ["E", "N", "W", "S"]


def rotate(conn, deg):
    k = deg // 90
    return frozenset(ORDER[(ORDER.index(c) + k) % 4] for c in conn)


def piece_for(conn, pieces):
    """The piece (name, as made) and the turn that connects as `conn`."""
    for name, made in pieces:
        for deg, turn in ROTATIONS.items():
            if rotate(made, deg) == conn:
                return name, turn
    raise ValueError(f"no piece connects {sorted(conn)}")


PATH_HALF = 5          # a path is 10 voxels wide, in the middle of its cells
PATH_PEBBLES = {(5, 9), (6, 9), (10, 6), (9, 11), (4, 5)}


def path_piece(conn):
    """A sandy path from the middle to each side it joins, its corners cut
    back where it ends or bends, a few pebbles in it."""
    def inside(x, y):
        cx, cy = x - 7.5, y - 7.5
        if abs(cx) <= PATH_HALF and abs(cy) <= PATH_HALF:
            qx, qy = ("E" if cx > 0 else "W"), ("N" if cy > 0 else "S")
            return qx in conn or qy in conn or abs(cx) + abs(cy) <= 7
        return ((("E" in conn and cx > 0) or ("W" in conn and cx < 0)) and abs(cy) <= PATH_HALF) or \
               ((("N" in conn and cy > 0) or ("S" in conn and cy < 0)) and abs(cx) <= PATH_HALF)

    def fill(x, y, z):
        if not inside(x, y):
            return turf(x, y, z, 5)
        if z == N - 1:
            return PEBBLE if (x, y) in PATH_PEBBLES else SAND
        return SAND if z == N - 2 else DIRT
    return fill


PATHS = [("path_end", frozenset("E")), ("path_straight", frozenset("EW")), ("path_bend", frozenset("EN")),
         ("path_join", frozenset("ENW")), ("path_cross", frozenset("ENWS"))]


def yard(x, y, z):
    """The chicken yard's ground: sand trodden flat over the whole cell, a
    pebble and a wisp of straw here and there."""
    if z == N - 1:
        n = noise(x, y, 21)
        return PEBBLE if n < 4 else (STRAW if n > 95 else SAND)
    return SAND if z == N - 2 else DIRT


LILIES = {(5, 5), (6, 5), (7, 5), (5, 6), (6, 6), (7, 6), (6, 7), (11, 10), (12, 10), (11, 11), (12, 11)}


def pond_piece(conn):
    """Water three voxels below the grass, a shore of sand a step down on
    the sides that are not water, its corners round (11 voxels), lily pads
    on it, one in flower."""
    R = 11.0

    def shore(x, y):
        edge = {"W": x, "E": N - 1 - x, "S": y, "N": N - 1 - y}
        open_ = [edge[k] for k in ORDER if k not in conn]
        d = min(open_) if open_ else 99
        for a, b in (("E", "N"), ("N", "W"), ("W", "S"), ("S", "E")):
            if a not in conn and b not in conn and edge[a] < R and edge[b] < R:
                d = min(d, R - ((R - edge[a] - 0.5) ** 2 + (R - edge[b] - 0.5) ** 2) ** 0.5)
        return d

    def fill(x, y, z):
        d = shore(x, y)
        if d < 1.5:
            return turf(x, y, z, 6)
        if d < 2.5:
            return SAND if z <= N - 2 else None
        if z == N - 3:
            return LILY if (x, y) in LILIES and len(conn) >= 3 else WATER
        if z == N - 2 and (x, y) == (6, 6) and len(conn) >= 3:
            return BLOSSOM
        return DIRT if z < N - 3 else None
    return fill


PONDS = [("pond_corner", frozenset("EN")), ("pond_side", frozenset("ENW")), ("pond_middle", frozenset("ENWS"))]


def fence_piece(conn):
    """A post of 2 by 2 in the middle, two rails of 2 by 2 to each side it
    joins."""
    def fill(x, y, z):
        if 7 <= x <= 8 and 7 <= y <= 8 and z <= 12:
            return POST
        if z in (4, 5, 9, 10):
            if 7 <= y <= 8 and (("E" in conn and x > 8) or ("W" in conn and x < 7)):
                return RAIL
            if 7 <= x <= 8 and (("N" in conn and y > 8) or ("S" in conn and y < 7)):
                return RAIL
        return None
    return fill


FENCES = [("fence_straight", frozenset("EW")), ("fence_corner", frozenset("EN")), ("fence_join", frozenset("ENW")),
          ("fence_end", frozenset("E"))]


def flower_voxels(cx, cy, bloom):
    """A flower: a stem two high, a bloom of five on it with a yellow eye."""
    v = [(cx, cy, 0, STEM), (cx, cy, 1, STEM), (cx, cy, 2, YELLOW)]
    v += [(cx + dx, cy + dy, 2, bloom) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))]
    return v


def tuft_voxels(cx, cy):
    """A tuft of grass: three high in the middle, a voxel round it."""
    v = [(cx, cy, z, GRASS_DARK) for z in range(3)]
    v += [(cx + dx, cy + dy, 0, GRASS_DARK) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))]
    return v


def decor(items):
    cells = {}
    for x, y, z, c in items:
        cells[(x, y, z)] = c
    return lambda x, y, z: cells.get((x, y, z))


FLOWERS_A = decor(flower_voxels(4, 4, RED) + flower_voxels(11, 7, WHITE) + flower_voxels(6, 11, RED) + tuft_voxels(12, 12))
FLOWERS_B = decor(flower_voxels(3, 9, YELLOW) + flower_voxels(10, 3, BLUE) + flower_voxels(12, 11, YELLOW) + tuft_voxels(6, 5))
TUFTS = decor(tuft_voxels(4, 4) + tuft_voxels(11, 9) + tuft_voxels(6, 12))

BLOCKS = [("grass_a", "solid", grass_a), ("grass_b", "solid", grass_b), ("soil", "solid", soil), ("fallow", "solid", fallow)]
BLOCKS += [(name, "solid", path_piece(conn)) for name, conn in PATHS]
BLOCKS += [(name, "hazard", pond_piece(conn)) for name, conn in PONDS]
BLOCKS += [(name, "solid", fence_piece(conn)) for name, conn in FENCES]
BLOCKS += [("yard", "solid", yard)]
BLOCKS += [("flowers_a", "decor", FLOWERS_A), ("flowers_b", "decor", FLOWERS_B), ("tufts", "decor", TUFTS)]

# ---------------------------------------------------------------- the farmer

# A bunny on its hind legs, small and plain like Voxkeep's hero: 10 across
# (the head 8, the body 6), 9 deep, 20 high to the ears' tips, facing +y.
# A round head with a white muzzle, a pink nose and big eyes, the long
# ears pink inside, blue dungarees over a red shirt, brown boots, a white
# tail. Three ways to dress it (`HATS`): a straw hat with the ears out
# through its brim, a neckerchief, a cap. Frames: standing, the walk in
# four (left foot forward, passing, right, passing), working (bowed over
# the bed, the paws forward).
FUR = (240, 232, 218)
TAIL = (252, 250, 246)
EAR = (240, 160, 168)
NOSE = (226, 112, 132)
SHIRT = (206, 70, 58)
DENIM = (72, 112, 178)
BOOT = (110, 74, 46)
HAT = (236, 204, 120)
BAND = (206, 70, 58)
KERCHIEF = (236, 196, 72)
CAP = (90, 156, 92)


def farmer(pose, dress):
    f = Fig((10, 9, 22))
    step = {"stand": (0, 0, 0, 0), "left": (1, -1, 0, 0), "right": (-1, 1, 0, 0), "pass": (0, 0, 0, 1),
            "pass2": (0, 0, 1, 0), "work": (0, 0, 0, 0)}.get(pose, (0, 0, 0, 0))
    # Boots and legs: the left x 2..3, the right x 6..7; a foot forward
    # (+y) or back by its step, a foot lifted a voxel when passing.
    for (x0, s, lift) in ((2, step[0], step[2]), (6, step[1], step[3])):
        f.box((x0, 3 + s, lift), (x0 + 1, 5 + s, lift + 1), BOOT)
        if not lift:
            f.box((x0, 3 + s, 2), (x0 + 1, 4 + s, 2), DENIM)
        f.box((x0, 3, 3), (x0 + 1, 4, 3), DENIM)
    # The body: dungarees to the waist, the bib in front and the straps
    # over the shirt.
    f.rbox((2, 2, 4), (7, 5, 8), lambda p, n: DENIM if p[2] <= 5 else SHIRT, bevel=1, axes=2)
    f.box((3, 5, 6), (6, 5, 7), DENIM)
    for x in (3, 6):
        f.box((x, 2, 8), (x, 5, 8), DENIM)
        f.box((x, 2, 6), (x, 2, 8), DENIM)
    f.box((4, 1, 4), (5, 1, 5), TAIL)
    # Arms a voxel across, two deep: the sleeve, the paw; swinging with the
    # steps, forward and down when working.
    swing = {"left": (1, -1), "right": (-1, 1)}.get(pose, (0, 0))
    for x, s in ((1, -swing[0]), (8, -swing[1])):
        if pose == "work":
            f.box((x, 3, 7), (x, 4, 8), SHIRT)
            f.box((x, 5, 6), (x, 6, 7), SHIRT)
            f.box((x, 7, 5), (x, 7, 6), FUR)
        elif pose.startswith(("hammer", "can", "shovel")) and x == 8:
            f.box((x, 3, 7), (x, 4, 8), SHIRT)     # the shoulder: the arm with the tool is tools()'s
        else:
            f.box((x, 3, 7), (x, 4, 8), SHIRT)
            f.box((x, 3 + s, 6), (x, 4 + s, 6), SHIRT)
            f.box((x, 3 + s, 5), (x, 4 + s, 5), FUR)
    # The head: a box of 8 by 6 by 6 with its edges cut back.
    dy, dz = (1, -1) if pose in ("work", "hammer2", "can0", "can1", "shovel0", "shovel1") else (0, 0)
    head = Fig((10, 9, 22))
    head.rbox((1, 1, 9), (8, 6, 14), FUR, bevel=1)
    head.box((4, 7, 10), (5, 7, 10), NOSE)
    for x, out in ((3, 2), (6, 7)):
        head.box((x, 6, 11), (x, 6, 12), figures.EYE)
        head.put((out, 6, 12), figures.GLINT)
    # Ears: 2 across, 2 deep, pink inside on the front.
    for x0 in (2, 6):
        head.box((x0, 3, 15), (x0 + 1, 4, 20), FUR)
        inner = x0 + 1 if x0 == 2 else x0
        head.box((inner, 4, 16), (inner, 4, 19), EAR)
    if dress == "hat":
        # a brim a voxel wider than the head all round, a crown on it with
        # a red band, the ears out through the crown's top
        for x in range(0, 10):
            for y in range(0, 8):
                if not (x in (0, 9) and y in (0, 7)):
                    head.put((x, y, 15), HAT)
        head.rbox((2, 2, 16), (7, 5, 17), lambda p, n: BAND if p[2] == 16 else HAT, bevel=1, axes=2)
        for x0 in (2, 6):
            head.box((x0, 3, 15), (x0 + 1, 4, 21), FUR)
            inner = x0 + 1 if x0 == 2 else x0
            head.box((inner, 4, 18), (inner, 4, 20), EAR)
    elif dress == "cap":
        head.rbox((1, 1, 15), (8, 6, 15), CAP, bevel=1, axes=2)
        head.box((2, 6, 15), (7, 8, 15), CAP)
        for x0 in (2, 6):
            head.box((x0, 3, 15), (x0 + 1, 4, 15), FUR)
    else:
        f.box((2, 6, 8), (7, 6, 8), KERCHIEF)
        f.box((3, 6, 7), (6, 6, 7), KERCHIEF)
        f.box((4, 6, 6), (5, 6, 6), KERCHIEF)
    for (x, y, z), c in head.v.items():
        f.put((x, y + dy, z + dz), c)
    f.tidy()
    return f


HATS = ["hat", "kerchief", "cap"]

# The bunny's model: the farmer's box (10, 9, 22) with room round it for
# what it holds (a hammer raised over its head, swung out in front), its
# middle the same (the model's anchor: every frame stands where it stood)
# and as tall (a kind's body is as tall as its model's box: the camera's
# height follows it).
BUNNY_BOX = (14, 21, 22)
BUNNY_AT = (2, 6, 0)            # where the farmer's box lies in it
HANDLE = (140, 100, 62)
HAMMER = figures.metal((112, 114, 122))


def held(f):
    """A frame of the farmer moved into the bunny's box."""
    g = Fig(BUNNY_BOX)
    for (x, y, z), c in f.v.items():
        g.put((x + BUNNY_AT[0], y + BUNNY_AT[1], z), c)
    return g


def hammering(stage, dress):
    """The farmer mending a fence (wolf.lua): its right arm swings a
    hammer, raised over its head and out past the hat's brim (0), on its
    way down at a slant (1), the blow, low in front where a fence's rail
    is (2). In the farmer's coordinates, beside its box."""
    f = held(farmer(f"hammer{stage}", dress))

    def box(lo, hi, c):
        f.box(tuple(a + b for a, b in zip(lo, BUNNY_AT)), tuple(a + b for a, b in zip(hi, BUNNY_AT)), c)
    if stage == 0:
        box((9, 3, 8), (9, 4, 9), SHIRT)
        box((10, 3, 10), (10, 4, 11), SHIRT)
        box((10, 3, 12), (10, 4, 12), FUR)
        box((10, 4, 13), (10, 4, 18), HANDLE)
        box((10, 2, 19), (11, 6, 20), HAMMER)
    elif stage == 1:
        box((8, 5, 8), (8, 6, 9), SHIRT)
        box((8, 7, 9), (8, 7, 10), FUR)
        for i in range(4):
            box((8, 8 + i, 11 + i), (8, 8 + i, 11 + i), HANDLE)
        for i in range(-2, 3):
            box((8, 13 + i, 16 - i), (9, 13 + i, 17 - i), HAMMER)
    else:
        box((8, 5, 6), (8, 6, 7), SHIRT)
        box((8, 7, 5), (8, 7, 6), FUR)
        box((8, 8, 5), (8, 10, 5), HANDLE)
        box((8, 11, 3), (9, 12, 7), HAMMER)
    return f


CAN = figures.metal((120, 160, 190))   # the watering can's tin, as its picture's in the HUD
SPRAY = (150, 206, 255)           # water falling from its rose
BLADE = figures.metal((150, 152, 160))


def watering(stage, dress):
    """The farmer watering a bed (farm.lua): the can held out at its
    right by the handle (seen from behind too), tipped, water falling
    from its rose before it; two frames, the drops elsewhere, for the
    stream."""
    f = held(farmer(f"can{stage}", dress))

    def box(lo, hi, c):
        f.box(tuple(a + b for a, b in zip(lo, BUNNY_AT)), tuple(a + b for a, b in zip(hi, BUNNY_AT)), c)
    box((9, 3, 8), (9, 4, 9), SHIRT)
    box((10, 4, 10), (10, 4, 10), FUR)         # the paw on the handle
    box((10, 5, 9), (10, 5, 9), CAN)           # the handle over the tin
    box((10, 7, 9), (10, 7, 9), CAN)
    box((10, 5, 10), (10, 7, 10), CAN)
    box((9, 5, 5), (11, 7, 8), CAN)            # the tin
    box((10, 8, 6), (10, 8, 6), CAN)           # the spout, down and forward
    box((10, 9, 5), (10, 9, 5), CAN)
    box((9, 10, 3), (11, 10, 4), CAN)          # the rose
    for x, y, z in (((9, 10, 1), (11, 10, 2), (10, 11, 0)), ((10, 10, 2), (9, 11, 0), (11, 11, 1)))[stage]:
        box((x, y, z), (x, y, z), SPRAY)
    return f


def digging(stage, dress):
    """The farmer sowing (farm.lua): a spade at its right, its grip over
    the shoulder (seen from behind too), the shaft down and forward two
    voxels a step; its blade pushed into the bed (0), then lifted with a
    clod on it (1)."""
    f = held(farmer(f"shovel{stage}", dress))

    def box(lo, hi, c):
        f.box(tuple(a + b for a, b in zip(lo, BUNNY_AT)), tuple(a + b for a, b in zip(hi, BUNNY_AT)), c)
    up = 0 if stage == 0 else 2
    box((8, 5, 7), (8, 5, 8), SHIRT)
    box((9, 6, 7 + up), (9, 6, 8 + up), FUR)       # the paw round the shaft
    box((9, 3, 14 + up), (11, 3, 14 + up), HANDLE)  # the grip
    for i in range(7):                              # the shaft
        box((10, 3 + i, 12 - 2 * i + up), (10, 3 + i, 13 - 2 * i + up), HANDLE)
    box((9, 10, up), (11, 10, 2 + up), BLADE)
    if stage == 1:
        box((9, 10, 5), (11, 10, 5), SOIL)          # a clod on the blade
        box((10, 10, 6), (10, 10, 6), SOIL)
    return f


def bunny(dress="hat", name="bunny"):
    frames = [held(farmer(p, dress)) for p in ("stand", "left", "pass", "right", "pass2", "work")]
    frames += [hammering(k, dress) for k in range(3)]
    frames += [watering(k, dress) for k in range(2)] + [digging(k, dress) for k in range(2)]
    save(name, BUNNY_BOX, frames)


# ---------------------------------------------------------------- crops

# A bed holds four plants on its two ridges (x 3 and 10, y 3 and 10 in
# the model, which lies on the bed's 14 by 14 inside); a crop's frames are
# its stages: sown, small, grown, ripe.
LEAF = (88, 164, 70)
LEAF_DARK = (62, 128, 70)
STRAW = (230, 196, 98)
EAR_GOLD = (204, 156, 64)
CARROT = (238, 128, 44)
BERRY = (72, 92, 190)
TOMATO = (220, 62, 50)
FLOWER = (246, 214, 80)
STAKE = (140, 100, 62)
SPOTS = [(3, 3), (10, 3), (3, 10), (10, 10)]


def sprout(f, cx, cy):
    f.box((cx, cy, 0), (cx, cy, 1), LEAF)
    f.put((cx + 1, cy, 1), LEAF)


def carrot(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    if stage == 1:
        f.box((cx, cy, 0), (cx, cy, 2), LEAF)
        f.put((cx - 1, cy, 2), LEAF)
        f.put((cx + 1, cy, 2), LEAF)
        return
    # A tuft of feathery leaves: stems out four ways, a leaf on each.
    f.box((cx, cy, 0), (cx, cy, 4), LEAF)
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        f.box((cx + dx, cy + dy, 2), (cx + dx, cy + dy, 3), LEAF)
        f.put((cx + 2 * dx, cy + 2 * dy, 3), LEAF)
        f.put((cx + 2 * dx, cy + 2 * dy, 4), LEAF)
    if stage == 3:
        f.box((cx - 1, cy - 1, 0), (cx + 1, cy + 1, 1), CARROT)
        f.box((cx, cy, 0), (cx, cy, 4), LEAF)


def wheat(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    stalks = [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)] if stage >= 2 else [(0, 0), (1, 0), (0, 1)]
    tall = {1: 2, 2: 5, 3: 6}[stage]
    for i, (dx, dy) in enumerate(stalks):
        h = tall - (i % 2)
        f.box((cx + dx, cy + dy, 0), (cx + dx, cy + dy, h), STRAW if stage == 3 else LEAF)
        if stage == 3:
            f.box((cx + dx, cy + dy, h + 1), (cx + dx, cy + dy, h + 3), EAR_GOLD)


def blueberry(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    if stage == 1:
        f.rbox((cx - 1, cy - 1, 0), (cx + 1, cy + 1, 2), LEAF_DARK, bevel=1)
        return
    f.rbox((cx - 2, cy - 2, 0), (cx + 2, cy + 2, 5), LEAF_DARK, bevel=1)
    if stage == 3:
        for x, y, z in ((cx - 2, cy, 3), (cx + 2, cy - 1, 2), (cx, cy - 2, 4), (cx + 1, cy + 2, 3), (cx - 1, cy - 2, 1), (cx, cy, 5)):
            f.put((x, y, z), BERRY)


def tomato(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    if stage == 1:
        f.box((cx, cy, 0), (cx, cy, 2), LEAF)
        f.put((cx - 1, cy, 2), LEAF)
        f.put((cx + 1, cy, 1), LEAF)
        return
    f.box((cx + 1, cy + 1, 0), (cx + 1, cy + 1, 9), STAKE)
    f.box((cx, cy, 0), (cx, cy, 7), LEAF)
    for z, (dx, dy) in ((2, (-1, 0)), (3, (1, 0)), (4, (0, -1)), (5, (-1, 0)), (6, (1, 0)), (7, (0, -1))):
        f.box((cx + dx, cy + dy, z), (cx + dx * 2, cy + dy * 2, z), LEAF)
    if stage == 2:
        f.put((cx - 2, cy, 5), FLOWER)
        f.put((cx, cy - 2, 7), FLOWER)
    else:
        f.box((cx - 2, cy - 1, 2), (cx - 1, cy - 1, 3), TOMATO)
        f.box((cx + 1, cy - 2, 4), (cx + 2, cy - 2, 5), TOMATO)
        f.box((cx - 1, cy + 1, 5), (cx - 1, cy + 2, 6), TOMATO)


RADISH = (226, 70, 108)
ROOT_WHITE = (244, 238, 222)
CURD = (240, 232, 204)
TURNIP_TOP = (140, 72, 160)
CABBAGE = (160, 210, 116)
EGGPLANT = (134, 62, 170)
CUCUMBER = (36, 96, 44)
PUMPKIN_ORANGE = (236, 140, 40)
WET = (74, 48, 32)


def radish(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    f.box((cx, cy, 0), (cx, cy, 2 if stage > 1 else 1), LEAF)
    reach = 1 if stage == 1 else 2
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        f.box((cx + dx, cy + dy, 1), (cx + dx * reach, cy + dy * reach, 1 if stage == 1 else 2), LEAF)
    if stage == 3:
        f.box((cx - 1, cy - 1, 0), (cx, cy, 1), RADISH)


def cauliflower(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    if stage == 1:
        f.rbox((cx - 1, cy - 1, 0), (cx + 1, cy + 1, 1), LEAF, bevel=1)
        return
    f.rbox((cx - 2, cy - 2, 0), (cx + 2, cy + 2, 2), LEAF_DARK, bevel=1)
    if stage == 3:
        f.box((cx - 1, cy - 1, 2), (cx + 1, cy + 1, 3), CURD)
        f.put((cx, cy, 4), CURD)


def turnip(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    tall = {1: 2, 2: 4, 3: 5}[stage]
    base = 2 if stage == 3 else 0
    f.box((cx, cy, base), (cx, cy, base + tall), LEAF)
    for dx, dy in ((1, 0), (-1, 0)):
        f.box((cx + dx, cy + dy, base + 1), (cx + dx, cy + dy, base + tall - 1), LEAF)
    if stage == 3:
        f.box((cx - 1, cy - 1, 0), (cx, cy, 0), ROOT_WHITE)
        f.box((cx - 1, cy - 1, 1), (cx, cy, 1), TURNIP_TOP)


def cabbage(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    if stage == 1:
        f.rbox((cx - 1, cy - 1, 0), (cx + 1, cy + 1, 1), LEAF, bevel=1)
        return
    f.rbox((cx - 2, cy - 2, 0), (cx + 2, cy + 2, 1), LEAF_DARK, bevel=1)
    if stage == 3:
        f.rbox((cx - 2, cy - 2, 1), (cx + 2, cy + 2, 4), CABBAGE, bevel=1)
    else:
        f.rbox((cx - 1, cy - 1, 1), (cx + 1, cy + 1, 2), CABBAGE, bevel=1)


def eggplant(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    tall = 3 if stage == 1 else 7
    f.box((cx, cy, 0), (cx, cy, tall), LEAF)
    for z, (dx, dy) in ((2, (-1, 0)), (3, (1, 0)), (5, (0, -1)), (6, (-1, 0)), (7, (1, 0))):
        if z <= tall:
            f.box((cx + dx, cy + dy, z), (cx + dx * 2, cy + dy * 2, z), LEAF)
    if stage == 2:
        f.put((cx + 2, cy, 4), (180, 140, 220))
    elif stage == 3:
        f.box((cx + 1, cy - 1, 1), (cx + 2, cy - 1, 3), EGGPLANT)
        f.put((cx + 1, cy - 1, 4), LEAF)
        f.box((cx - 2, cy + 1, 2), (cx - 1, cy + 1, 4), EGGPLANT)
        f.put((cx - 1, cy + 1, 5), LEAF)


def cucumber(f, stage, cx, cy):
    if stage == 0:
        return sprout(f, cx, cy)
    reach = 1 if stage == 1 else 2
    f.box((cx, cy, 0), (cx, cy, 2), LEAF)
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        f.box((cx + dx, cy + dy, 0), (cx + dx * reach, cy + dy * reach, 1), LEAF)
    if stage == 2:
        f.put((cx + 1, cy + 1, 1), FLOWER)
    elif stage == 3:
        # cucumbers lying between the vines, in the open corners
        f.box((cx + 1, cy + 1, 0), (cx + 2, cy + 1, 0), CUCUMBER)
        f.box((cx - 1, cy - 2, 0), (cx - 1, cy - 1, 0), CUCUMBER)
        f.put((cx - 1, cy + 1, 1), FLOWER)


def pumpkin_bed(f, stage):
    """The pumpkin: vines over the bed, then one big pumpkin in its middle,
    ribbed, with a stem."""
    if stage < 3:
        for cx, cy in SPOTS:
            if stage == 0:
                sprout(f, cx, cy)
            else:
                f.box((cx, cy, 0), (cx, cy, stage), LEAF)
                f.rbox((cx - stage, cy - 1, 0), (cx + stage, cy + 1, 1), LEAF_DARK, bevel=1)
        return
    for cx, cy in SPOTS:
        f.rbox((cx - 1, cy - 1, 0), (cx + 1, cy + 1, 1), LEAF_DARK, bevel=1)
    f.rbox((3, 3, 0), (10, 10, 6), PUMPKIN_ORANGE, bevel=2)
    for p in list(f.v):
        x, y, z = p
        if f.v[p] == PUMPKIN_ORANGE and (x in (5, 8) or y in (5, 8)) and f.surface(p) and 1 <= z <= 5 and (x in (3, 10) or y in (3, 10)):
            del f.v[p]                      # its ribs, a voxel in
    f.box((6, 6, 7), (7, 7, 8), STAKE)


def wetted(f):
    """The same with its ridges wet: dark earth where no plant stands."""
    g = Fig(f.size)
    g.v = dict(f.v)
    for x in range(14):
        for y in list(range(1, 6)) + list(range(8, 13)):
            g.put((x, y, 0), WET, under=True)
    return g


def crop(name, plant):
    """A crop's model: its four stages dry, then the same four wet."""
    frames = []
    for stage in range(4):
        f = Fig((14, 14, 12))
        if plant is pumpkin_bed:
            pumpkin_bed(f, stage)
        else:
            for cx, cy in SPOTS:
                plant(f, stage, cx, cy)
        frames.append(f)
    save(name, (14, 14, 12), frames + [wetted(f) for f in frames])


# ---------------------------------------------------------------- props

BARK = (126, 88, 58)
CROWN = (92, 166, 76)
PINE = (64, 132, 86)
APPLE = (222, 62, 54)
HAY = (232, 198, 110)
TWINE = (164, 118, 66)
BARREL = (156, 108, 66)
IRON = (96, 96, 104)
CRATE = (190, 146, 94)
BURLAP = (210, 176, 122)
SCARE_SHIRT = (88, 120, 186)
PATCH = (214, 92, 70)


def surface_put(f, x, z, c, front=True):
    """Puts `c` on the outermost voxel at x, z on the front (+y) or back."""
    ys = [y for (xx, y, zz) in f.v if xx == x and zz == z]
    if ys:
        f.v[(x, max(ys) if front else min(ys), z)] = c


LEAF_TONE = (104, 176, 82)       # a second green among the leaves, as stones differ
PINE_TONE = (76, 146, 92)        # and among the needles


def speckle(f, colour, tone, k, notch, keep):
    """Leaves made lively by shape and a little colour, by a fixed
    scatter: a voxel of the surface notched out here and there (the light
    draws the notch; `keep(p)` says which may not go), another of `tone`
    (two greens mixed, as stones differ, not light painted on)."""
    for (x, y, z) in [p for p, c in f.v.items() if c == colour and f.surface(p)]:
        n = noise(x * 7 + z, y * 5 + z * 3, k)
        if n < notch and not keep((x, y, z)):
            del f.v[(x, y, z)]
        elif n > 84:
            f.v[(x, y, z)] = tone


def tree(apples):
    """A broad-leaved tree (2026-10-26, the user: the first trees, cakes
    of terraced boxes, could be better): a trunk 3 across, branches forking
    at 45 degrees, a crown of round clumps of leaves at their ends and over
    the trunk, of different sizes (big enough to be round cleanly),
    notched; apples on it or not."""
    f = Fig((28, 28, 40))
    f.rbox((12, 12, 0), (14, 14, 18), BARK, bevel=1, axes=2)
    for x, y in ((11, 13), (15, 13), (13, 11), (13, 15)):
        f.put((x, y, 0), BARK)
    for i in range(6):
        f.box((11 - i, 13, 13 + i), (11 - i, 13, 14 + i), BARK)
        f.box((15 + i, 13, 14 + i), (15 + i, 13, 15 + i), BARK)
    for i in range(4):
        f.box((13, 15 + i, 15 + i), (13, 15 + i, 16 + i), BARK)
    for c, r in (((6, 13, 21), 5.6), ((21, 13, 22), 5.6), ((13, 19, 23), 5.6), ((13.5, 13, 27), 7.4),
                 ((13, 7, 22), 4.6), ((14, 13, 33), 4.4)):
        f.ellipsoid(c, (r, r, r * 0.9), CROWN)
    f.tidy()
    speckle(f, CROWN, LEAF_TONE, 41, 9, lambda p: (p[0], p[1], p[2] + 1) not in f.v)
    if apples:
        for x, z in ((5, 20), (12, 22), (20, 21), (16, 27), (9, 28), (23, 24)):
            surface_put(f, x, z, APPLE)
        for x, z in ((7, 22), (19, 24)):
            surface_put(f, x, z, APPLE, front=False)
    return f


def pine():
    """A fir: four round tiers, each a cone cut short, its rim drooping a
    voxel as boughs do, on a short trunk; notched."""
    f = Fig((24, 24, 38))
    c = 11.5
    f.box((11, 11, 0), (12, 12, 8), BARK)
    for r0, r1, z0, h in ((10.6, 4.5, 6, 7), (8.6, 3.5, 12, 7), (6.6, 2.5, 18, 6), (4.6, 1.5, 23, 6)):
        for k in range(h):
            r = r0 - (r0 - r1) * k / (h - 1)
            for x in range(24):
                for y in range(24):
                    d = math.hypot(x - c, y - c)
                    if d <= r:
                        f.put((x, y, z0 + k), PINE)
                        if k == 0 and d > r - 1.6:
                            f.put((x, y, z0 - 1), PINE)
    f.box((11, 11, 29), (12, 12, 31), PINE)
    f.tidy()
    speckle(f, PINE, PINE_TONE, 43, 7, lambda p: (p[0], p[1], p[2] + 1) not in f.v)
    return f


BUSH = (78, 148, 70)


def bush():
    """Three clumps of leaves, the middle one highest, a few blossoms."""
    f = Fig((16, 12, 10))
    f.rbox((0, 2, 0), (7, 9, 5), BUSH, bevel=2)
    f.rbox((4, 1, 0), (11, 10, 8), BUSH, bevel=2)
    f.rbox((9, 2, 0), (15, 9, 6), BUSH, bevel=2)
    for x, z in ((2, 3), (7, 6), (13, 4), (9, 2)):
        surface_put(f, x, z, BLOSSOM)
    return f


def rock():
    f = Fig((14, 11, 7))
    f.rbox((0, 1, 0), (9, 9, 5), STONE, bevel=2)
    f.rbox((8, 0, 0), (13, 6, 3), STONE_DARK, bevel=1)
    return f


def hay():
    """A bale of hay with two bands of twine round it."""
    f = Fig((13, 9, 9))
    f.rbox((0, 0, 0), (12, 8, 8), HAY, bevel=1)
    f.recolour(lambda p: p[0] in (3, 9) and f.surface(p), TWINE)
    return f


def barrel():
    f = Fig((10, 10, 12))
    f.rbox((0, 0, 0), (9, 9, 11), BARREL, bevel=2, axes=2)
    f.recolour(lambda p: p[2] in (2, 9) and f.surface(p) and p[2] < 11, IRON)
    return f


def crate():
    f = Fig((10, 10, 9))
    f.box((0, 0, 0), (9, 9, 8), CRATE)
    # boards: a groove a voxel deep between them, on every side
    for p in list(f.v):
        x, y, z = p
        if z in (3, 6) and (x in (0, 9) or y in (0, 9)):
            del f.v[p]
    return f


def scarecrow():
    """On a post: a sack head with a straw hat, a blue shirt with a patch,
    its arms out on the crossbar with straw at the ends."""
    f = Fig((18, 7, 28))
    f.box((8, 3, 0), (9, 4, 14), POST)
    f.rbox((5, 1, 9), (12, 5, 16), SCARE_SHIRT, bevel=1, axes=2)
    f.box((1, 2, 14), (16, 4, 15), SCARE_SHIRT)
    f.box((0, 2, 13), (0, 4, 15), HAY)
    f.box((17, 2, 13), (17, 4, 15), HAY)
    f.box((6, 5, 11), (7, 5, 12), PATCH)
    f.rbox((6, 1, 17), (11, 5, 22), BURLAP, bevel=1)
    for x in (7, 10):
        f.box((x, 5, 19), (x, 5, 20), figures.EYE)
    f.box((8, 5, 18), (9, 5, 18), TWINE)
    for x in range(4, 14):
        for y in range(0, 7):
            if not ((x in (4, 13)) and (y in (0, 6))):
                f.put((x, y, 23), HAY)
    f.rbox((6, 1, 24), (11, 5, 26), HAY, bevel=1, axes=2)
    return f


# ---------------------------------------------------------------- the burrow and the stall

TURF = (98, 162, 74)
DOOR = (150, 96, 56)
FRAME = (104, 70, 44)
KNOB = (236, 196, 90)
WINDOW = (255, 222, 150)
LAMP = (255, 214, 120)
SOOT = (70, 66, 64)


def ring(f, cx, cz, r0, r1, y, colour, under=False):
    """Voxels whose middles lie between r0 and r1 from (cx, cz), at y."""
    for x in range(int(cx - r1 - 1), int(cx + r1 + 2)):
        for z in range(int(cz - r1 - 1), int(cz + r1 + 2)):
            d = ((x + 0.5 - cx) ** 2 + (z + 0.5 - cz) ** 2) ** 0.5
            if r0 <= d <= r1 and z >= 0:
                f.put((x, y, z), colour, under)


def burrow():
    """The bunny's home: a grassy hill with its edges cut far back, a stone
    front set into it with a round door between two round windows, a
    chimney on top, a lantern beside the door. Its front faces +y."""
    f = Fig((46, 34, 30))
    f.rbox((0, 0, 0), (45, 28, 21), TURF, bevel=9)
    # the stone front, flat, with its top stepped in
    for z, (x0, x1) in enumerate([(9, 36)] * 10 + [(10, 35), (12, 33), (15, 30), (18, 27)]):
        for x in range(x0, x1 + 1):
            for y in range(24, 30):
                f.v.pop((x, y, z), None)
            f.put((x, 29, z), STONE)
            for y in range(24, 29):
                f.put((x, y, z), STONE_DARK, under=True)
    # the door: round, set a voxel into the wall, boards and a knob
    for x in range(16, 30):
        for z in range(0, 14):
            d = ((x + 0.5 - 23) ** 2 + max(0.0, z + 0.5 - 6.5) ** 2) ** 0.5
            if d <= 5.5:
                f.v.pop((x, 29, z), None)
                f.put((x, 28 if x not in (20, 25) else 27, z), DOOR)   # boards, a groove between
            elif d <= 6.6:
                f.put((x, 29, z), FRAME)
    f.put((26, 29, 5), KNOB)
    # two round windows, warm light inside
    for cx in (13.5, 32.5):
        ring(f, cx, 7.5, 2.6, 3.7, 29, FRAME)
        for x in range(9, 38):
            for z in range(2, 13):
                if ((x + 0.5 - cx) ** 2 + (z + 0.5 - 7.5) ** 2) ** 0.5 < 2.6:
                    f.v.pop((x, 29, z), None)
                    f.put((x, 28, z), WINDOW)
    # a chimney of stone on the hill
    f.rbox((32, 9, 16), (36, 13, 27), STONE, bevel=1, axes=2)
    f.box((33, 10, 27), (35, 12, 27), SOOT)
    # flowers on the hill
    for x, y in ((8, 12), (14, 6), (30, 18), (38, 14), (20, 10)):
        zs = [z for (xx, yy, z) in f.v if xx == x and yy == y]
        if zs:
            top = max(zs)
            f.put((x, y, top + 1), STEM)
            for dx, dy in ((0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)):
                f.put((x + dx, y + dy, top + 2), YELLOW if (dx, dy) == (0, 0) else (RED if x % 2 else WHITE))
    # the lantern on its post, left of the door
    f.box((5, 31, 0), (5, 31, 10), POST)
    f.box((4, 30, 11), (6, 32, 13), LAMP)
    f.box((4, 30, 14), (6, 32, 14), POST)
    f.tidy()
    save("burrow", (46, 34, 30), [f])


AWNING = [(214, 70, 60), (246, 238, 224)]
WOOD = (172, 122, 76)
WOOD_TOP = (196, 150, 98)


def stall():
    """The market stall: a counter of boards with crates of carrots,
    wheat, tomatoes and blueberries on it, four posts, a striped awning
    sloping to the front with a scalloped edge, a sign with a carrot on
    top. Its front (where the bunny buys) faces +y."""
    f = Fig((36, 22, 32))
    f.box((2, 6, 0), (33, 13, 9), WOOD)
    for x in range(5, 33, 5):
        for z in range(0, 9):
            f.v.pop((x, 13, z), None)
            f.put((x, 12, z), WOOD)
    f.box((1, 5, 10), (34, 14, 10), WOOD_TOP)
    # crates of produce on the counter
    for x0, kind in ((3, "carrot"), (11, "wheat"), (19, "tomato"), (27, "berry")):
        f.box((x0, 7, 11), (x0 + 6, 12, 12), FRAME)
        for x in range(x0 + 1, x0 + 6):
            for y in range(8, 12):
                if kind == "carrot":
                    c = CARROT if (x + y) % 3 else LEAF
                elif kind == "wheat":
                    c = STRAW if (x + y) % 2 else EAR_GOLD
                elif kind == "tomato":
                    c = TOMATO if (x * 3 + y) % 4 else LEAF
                else:
                    c = BERRY
                f.put((x, y, 12), c)
                if (x + y) % 2 == 0:
                    f.put((x, y, 13), c)
    # posts
    for x in (2, 32):
        for y in (6, 12):
            f.box((x, y, 0), (x + 1, y + 1, 25), POST)
    # the awning: stripes along the depth, sloping down to the front,
    # every other stripe a voxel longer at its edge
    for y in range(2, 19):
        z = 27 - (y - 2) // 4
        for x in range(0, 36):
            stripe = (x // 4) % 2
            if y == 18 and stripe == 1:
                continue
            f.put((x, y, z), AWNING[stripe])
    # the sign on top: a board on two legs, a carrot on it
    f.box((14, 8, 27), (14, 8, 29), POST)
    f.box((21, 8, 27), (21, 8, 29), POST)
    f.box((12, 8, 29), (23, 9, 31), WOOD_TOP)
    for x, z, c in ((15, 30, CARROT), (16, 30, CARROT), (17, 30, CARROT), (18, 30, CARROT), (19, 31, LEAF), (20, 30, LEAF)):
        f.put((x, 10, z), c)
    f.tidy()
    save("stall", (36, 22, 32), [f])


# ---------------------------------------------------------------- the kitchen, the mailbox, the cat

EMBER = (255, 158, 70)
POT = (82, 80, 88)
MAIL_RED = (204, 70, 56)
FLAG = (250, 210, 70)
CAT_FUR = (236, 152, 72)
CAT_WHITE = (250, 244, 232)
CAT_PINK = (240, 150, 160)
SATCHEL = (128, 88, 54)


def kitchen_fig(pots):
    """The summer kitchen beside the burrow, its front +y: a stone oven
    with a chimney and embers in its mouth, a counter of boards with
    `pots` pots on it, a wall of boards behind with a shelf of jars (no
    roof: from above it hid the pots)."""
    f = Fig((32, 16, 26))
    f.rbox((20, 3, 0), (30, 12, 11), STONE, bevel=1)
    for x in range(23, 28):
        for z in range(1, 6):
            f.v.pop((x, 12, z), None)
            f.put((x, 11, z), EMBER)
    f.rbox((23, 5, 12), (27, 9, 23), STONE, bevel=1, axes=2)
    f.box((24, 6, 23), (26, 8, 23), SOOT)
    f.box((2, 5, 7), (18, 11, 8), WOOD_TOP)
    for x in (2, 17):
        for y in (5, 10):
            f.box((x, y, 0), (x + 1, y + 1, 6), POST)
    f.box((2, 11, 2), (18, 11, 6), WOOD)
    for k in range(pots):
        x0 = 3 + k * 5
        f.rbox((x0, 6, 9), (x0 + 3, 9, 11), POT, bevel=1, axes=2)
        f.box((x0 + 1, 7, 12), (x0 + 2, 8, 12), POT)
    f.box((2, 3, 0), (18, 4, 17), WOOD)
    f.box((2, 5, 14), (18, 6, 14), WOOD_TOP)
    for x0, c in ((4, (86, 70, 170)), (9, (210, 60, 46)), (14, (90, 150, 70))):
        f.box((x0, 5, 15), (x0 + 1, 6, 16), c)
        f.box((x0, 5, 17), (x0 + 1, 6, 17), POST)
    f.tidy()
    return f


def mailbox_fig(flag_up):
    """A mailbox on a post, its front +y, a flag at its side: up when
    something waits in it."""
    f = Fig((10, 10, 18))
    f.box((4, 4, 0), (5, 5, 9), POST)
    f.rbox((1, 2, 10), (8, 8, 15), MAIL_RED, bevel=1)
    f.box((3, 8, 12), (6, 8, 12), SOOT)
    if flag_up:
        f.box((9, 5, 11), (9, 5, 17), POST)
        f.box((9, 3, 15), (9, 4, 17), FLAG)
    else:
        f.box((9, 5, 11), (9, 5, 12), POST)
        f.box((9, 2, 12), (9, 4, 12), FLAG)
    return f


# ---------------------------------------------------------------- the barn

# The barn's animals (stage 5), each a file a colour (the prototype's
# colours, the rare ones bright), their front +y, the young in the same
# file: chickens 10 voxels tall, sheep 12, cows 15, as the cat is 14.
# Frames: the grown standing, walking (two), at work (pecking, grazing),
# the states that show (a hen brooding on her nest; a sheep's fleece full,
# half grown, shorn; a cow's full udder), the young standing and walking.
LEG_YELLOW = (236, 166, 64)
COMB = (216, 56, 50)
BEAK = (242, 180, 60)
NEST = (214, 172, 96)
CHICKEN_COLOURS = {"white": (244, 240, 228), "brown": (176, 112, 64), "red": (198, 84, 52),
                   "blue": (98, 138, 214), "green": (102, 174, 94)}
RAINBOW = [(222, 64, 60), (242, 150, 50), (244, 210, 70), (96, 180, 84), (84, 128, 220), (150, 96, 200)]
WOOL = {"white": (244, 240, 232), "brown": (156, 108, 72), "black": (70, 64, 70)}
SHEEP_FACE = {"white": (96, 80, 78), "brown": (86, 70, 66), "black": (236, 218, 200)}
HOOF = (82, 70, 66)
COW_COLOURS = {"light": ((238, 230, 212), (72, 66, 66)), "brown": ((152, 98, 62), (244, 238, 226)),
               "pink": ((238, 156, 178), (248, 240, 236)), "purple": ((156, 118, 206), (246, 240, 238)),
               "green": ((122, 192, 112), (246, 242, 230))}
MUZZLE = (240, 192, 180)
HORN = (236, 226, 198)
UDDER = (242, 162, 178)


def chicken_fig(colour, pose):
    """A hen (or a cock: the same bird), her front +y: a round body with
    a wing at each side, tail feathers up behind, a head with a red comb,
    a beak and a wattle, yellow legs. pose: stand, walk_a, walk_b, peck
    (the head down at the ground), brood (sitting on a nest of straw),
    chick, chick_walk. `colour` a feather colour, or "rainbow"."""
    f = Fig((9, 12, 12))
    rainbow = colour == "rainbow"
    body = (250, 248, 240) if rainbow else CHICKEN_COLOURS[colour]

    def feather(p, n):
        return RAINBOW[(p[1] + p[2]) % len(RAINBOW)] if rainbow else body
    if pose in ("chick", "chick_walk"):
        step = 1 if pose == "chick_walk" else 0
        for x, fwd in ((3, step), (5, -step)):
            f.box((x, 6 + fwd, 0), (x, 6 + fwd, 0), LEG_YELLOW)
        f.rbox((2, 4, 1), (6, 8, 4), lambda p, n: body, bevel=1)
        f.rbox((3, 7, 4), (5, 9, 6), lambda p, n: body, bevel=1)
        f.put((4, 10, 5), BEAK)
        for x in (3, 5):
            f.put((x, 9, 5), figures.EYE)
        f.tidy()
        return f
    lift = 0 if pose == "brood" else 2
    if pose == "brood":
        f.rbox((0, 2, 0), (8, 9, 1), NEST, bevel=1)
    else:
        for x, fwd in ((3, {"walk_a": 1, "walk_b": -1}.get(pose, 0)), (5, {"walk_a": -1, "walk_b": 1}.get(pose, 0))):
            f.box((x, 5 + fwd, 1), (x, 5 + fwd, 1), LEG_YELLOW)
            f.box((x, 5 + fwd, 0), (x, 6 + fwd, 0), LEG_YELLOW)
    f.rbox((2, 2, lift), (6, 8, lift + 4), feather, bevel=1)
    for x in (1, 7):
        f.rbox((x, 3, lift + 1), (x, 7, lift + 3), feather, bevel=1)
    f.rbox((3, 1, lift + 3), (5, 2, lift + 7), (lambda p, n: RAINBOW[p[2] % len(RAINBOW)]) if rainbow else body, bevel=1)
    hy, hz = (8, lift + 1) if pose == "peck" else (7, lift + 5)
    f.rbox((3, hy, hz), (5, hy + 2, hz + 3), body, bevel=0)
    f.box((4, hy, hz + 4), (4, hy + 2, hz + 4), COMB)
    f.put((4, hy + 3, hz + 2), BEAK)
    f.put((4, hy + 3, hz + 1), COMB)
    for x in (3, 5):
        f.put((x, hy + 2, hz + 2), figures.EYE)
    f.tidy()
    return f


def sheep_fig(colour, pose):
    """A sheep, her front +y: a cloud of wool (a body with curls standing
    out of its back and sides) on four short dark legs, a darker face
    with ears out to the sides and a tuft of wool on top. pose: full,
    full_walk, half, half_walk, shorn, shorn_walk (her fleece as it grows
    back: the fleece is the timer), graze (her head down), lamb,
    lamb_walk."""
    f = Fig((13, 18, 14))
    wool, face = WOOL[colour], SHEEP_FACE[colour]
    walk = pose.endswith("_walk")
    if pose.startswith("lamb"):
        for x, y in ((4, 6), (7, 6), (4, 10), (7, 10)):
            fwd = (1 if (x == 4) == (y == 6) else -1) if walk else 0
            f.box((x, y + fwd, 0), (x + 1, y + fwd, 2), face)
        f.rbox((3, 5, 3), (9, 11, 7), wool, bevel=1)
        f.rbox((5, 3, 6), (7, 5, 8), wool, bevel=1)
        f.rbox((4, 11, 6), (8, 14, 10), face, bevel=1)
        f.box((5, 11, 10), (7, 12, 11), wool)
        for x in (3, 9):
            f.put((x, 12, 9), face)
        for x in (5, 7):
            f.put((x, 14, 8), figures.EYE)
        f.tidy()
        return f
    fleece = "full" if pose.startswith(("full", "graze")) else ("half" if pose.startswith("half") else "shorn")
    for x, y in ((3, 5), (8, 5), (3, 11), (8, 11)):
        fwd = (1 if (x == 3) == (y == 5) else -1) if walk else 0
        f.box((x, y + fwd, 0), (x + 1, y + 1 + fwd, 3), face)
    if fleece == "shorn":
        f.rbox((3, 4, 3), (9, 13, 8), wool, bevel=1)
    else:
        f.rbox((2, 3, 3), (10, 14, 9), wool, bevel=1)
    if fleece == "full":
        # curls standing out: along the back and down the flanks
        for y0 in (4, 8, 11):
            f.rbox((4, y0, 9), (8, y0 + 2, 11), wool, bevel=1)
        for x0 in (1, 9):
            for y0 in (5, 9):
                f.rbox((x0, y0, 4), (x0 + 2, y0 + 2, 8), wool, bevel=1)
        f.rbox((4, 2, 5), (8, 3, 9), wool, bevel=1)
    hy, hz = (14, 2) if pose == "graze" else (14, 7)
    f.rbox((4, hy, hz), (8, hy + 3, hz + 4), face, bevel=1)
    f.rbox((5, hy, hz + 4), (7, hy + 2, hz + 5), wool, bevel=0)
    for x in (2, 3):
        f.put((x, hy + 1, hz + 3), face)
    for x in (9, 10):
        f.put((x, hy + 1, hz + 3), face)
    for x in (5, 7):
        f.put((x, hy + 3, hz + 2), figures.GLINT if colour != "black" else figures.EYE)
    f.put((6, 2 if fleece != "shorn" else 3, 7), wool)
    f.tidy()
    return f


def cow_fig(colour, pose):
    """A cow, her front +y: a long body narrower than it is long and tall
    (its edges cut back two voxels: no box), big patches on it, four legs
    with hooves under its edges, a neck up to a head with a pale muzzle,
    short horns and ears to the sides, a tail with a tuft. pose: stand,
    walk_a, walk_b, graze (head down), udder (full, to be milked), calf,
    calf_walk. (The first cows were too wide and square, the user.)"""
    f = Fig((11, 23, 16))
    hide, patch = COW_COLOURS[colour]

    def spot(lo, hi):
        """A patch: the hide's voxels in a box with its edges cut back."""
        def inside(p):
            inset = [min(v - a, b - v) for v, a, b in zip(p, lo, hi)]
            return min(inset) >= 0 and sum(max(0, 1 - d) for d in inset) <= 1
        f.recolour(lambda p: f.v[p] == hide and inside(p), patch)
    if pose.startswith("calf"):
        walk = pose == "calf_walk"
        for x, y in ((3, 7), (6, 7), (3, 12), (6, 12)):
            fwd = (1 if (x == 3) == (y == 7) else -1) if walk else 0
            f.box((x, y + fwd, 0), (x + 1, y + fwd, 3), hide)
            f.box((x, y + fwd, 0), (x + 1, y + fwd, 0), HOOF)
        f.rbox((3, 6, 4), (7, 13, 8), hide, bevel=1)
        f.rbox((3, 13, 7), (7, 16, 11), hide, bevel=1)
        spot((5, 8, 5), (8, 11, 9))
        f.box((4, 17, 7), (6, 17, 8), MUZZLE)
        for x in (2, 8):
            f.put((x, 14, 10), hide)
        for x in (4, 6):
            f.put((x, 16, 10), figures.EYE)
        f.tidy()
        return f
    walk = {"walk_a": 1, "walk_b": -1}.get(pose, 0)
    for x, y in ((2, 5), (7, 5), (2, 14), (7, 14)):
        fwd = walk * (1 if (x == 2) == (y == 5) else -1)
        f.box((x, y + fwd, 0), (x + 1, y + 1 + fwd, 5), hide)
        f.box((x, y + fwd, 0), (x + 1, y + 1 + fwd, 0), HOOF)
    f.rbox((2, 3, 5), (8, 16, 11), hide, bevel=2)
    # big patches: over the back and down the left flank, on the right
    # flank at the front, on the rump
    spot((1, 6, 7), (5, 10, 12))
    spot((6, 12, 5), (9, 15, 9))
    spot((3, 3, 8), (7, 5, 12))
    if pose == "udder":
        f.rbox((4, 6, 3), (6, 9, 4), UDDER, bevel=1)
        for x in (4, 6):
            f.put((x, 7, 2), UDDER)
    hy, hz = (16, 3) if pose == "graze" else (17, 9)
    # the neck from the shoulders up (or down) to the head
    f.box((3, 15, min(hz, 8) + 1), (7, 16, max(hz, 8) + 1), hide)
    f.rbox((3, hy, hz), (7, hy + 4, hz + 5), hide, bevel=1)
    spot((2, hy + 1, hz + 3), (5, hy + 5, hz + 6))
    f.box((3, hy + 5, hz), (7, hy + 5, hz + 2), MUZZLE)
    for x in (4, 6):
        f.put((x, hy + 5, hz + 1), HOOF)
    for x in (1, 2, 8, 9):
        f.put((x, hy + 1, hz + 4), hide)
    for x in (3, 7):
        f.put((x, hy + 1, hz + 6), HORN)
    for x in (3, 7):
        f.put((x, hy + 4, hz + 3), figures.EYE)
    f.box((5, 2, 6), (5, 2, 10), hide)
    f.box((5, 1, 5), (5, 1, 6), patch)
    f.tidy()
    return f


DOG_FUR = (204, 158, 102)
DOG_EAR = (124, 84, 52)
DOG_CREAM = (244, 230, 204)
NOSE_BLACK = (44, 36, 34)
BANDANA = (204, 60, 52)
FOX_FUR = (220, 106, 46)
FOX_WHITE = (250, 244, 232)
FOX_DARK = (66, 50, 46)
WAISTCOAT = (78, 120, 84)
COOP_ROOF = (186, 72, 56)
BARN_RED = (184, 64, 52)
TRIM = (244, 238, 224)
SHED_ROOF = (120, 92, 70)
WATER_TROUGH = (90, 150, 210)
GRAIN = (232, 196, 110)


def egg_fig(rainbow):
    """An egg lying in the straw, as the hen has laid it: white, or the
    rainbow hen's in bands of colour."""
    f = Fig((5, 5, 5))
    f.rbox((0, 0, 0), (4, 4, 0), NEST, bevel=1)
    f.rbox((1, 1, 1), (3, 3, 4), (lambda p, n: RAINBOW[p[2] % len(RAINBOW)]) if rainbow else (250, 246, 236), bevel=1)
    f.tidy()
    return f


def gable(f, x0, x1, y0, y1, z, roof, trim=None):
    """A gable roof over x0..x1 from front to back (y0..y1), its eaves at
    z, the ridge running front to back (this camera sees both slopes from
    above), the gables filled with `fill` below; trim along the edges."""
    k = 0
    while x0 + k <= x1 - k:
        f.box((x0 + k, y0, z + k), (x0 + k, y1, z + k), roof)
        f.box((x1 - k, y0, z + k), (x1 - k, y1, z + k), roof)
        if trim:
            for y in (y0, y1):
                f.put((x0 + k, y, z + k), trim)
                f.put((x1 - k, y, z + k), trim)
        k += 1


def coop_fig():
    """The chicken coop (the chicken yard's stable): a little wooden house
    on four short posts, a red gable roof, a round door in its front with
    a ramp down to the yard; its front +y."""
    f = Fig((22, 20, 22))
    for x, y in ((3, 4), (17, 4), (3, 13), (17, 13)):
        f.box((x, y, 0), (x + 1, y + 1, 4), POST)
    f.box((3, 4, 5), (18, 15, 13), WOOD)
    for z in (7, 10):
        f.box((3, 15, z), (18, 15, z), WOOD_TOP)
    k = 0
    while 3 + k <= 18 - k:
        f.box((3 + k, 4, 14 + k), (18 - k, 15, 14 + k), WOOD)
        k += 1
    gable(f, 1, 20, 3, 16, 13, COOP_ROOF)
    for x in range(9, 13):
        for z in range(6, 11):
            if (x - 10.5) ** 2 / 4 + (z - 8) ** 2 / 6.5 <= 1:
                f.v.pop((x, 15, z), None)
                f.put((x, 14, z), SOOT)
    for k in range(5):
        f.box((9, 16 + k, 4 - k), (12, 16 + k, 4 - k), RAIL)
    f.tidy()
    return f


def shed_fig():
    """The sheep's shelter (the meadow's stable): a wooden shed under a
    gable roof, its front open wide (hay bales inside in the dark), its
    front +y."""
    f = Fig((30, 20, 24))
    f.box((2, 2, 0), (27, 16, 12), WOOD)
    k = 0
    while 2 + k <= 27 - k:
        f.box((2 + k, 2, 13 + k), (27 - k, 16, 13 + k), WOOD)
        k += 1
    gable(f, 0, 29, 1, 17, 12, SHED_ROOF, WOOD_TOP)
    for z in (4, 8):
        f.box((2, 16, z), (27, 16, z), WOOD_TOP)
    # the open front: the wall cut away, the dark inside, hay in it
    for x in range(6, 24):
        for z in range(0, 11):
            f.v.pop((x, 16, z), None)
            f.v.pop((x, 15, z), None)
            f.put((x, 14, z), SOOT)
    f.box((5, 16, 0), (5, 16, 11), POST)
    f.box((24, 16, 0), (24, 16, 11), POST)
    f.box((5, 16, 11), (24, 16, 11), POST)
    f.rbox((7, 9, 0), (14, 14, 5), HAY, bevel=1)
    f.rbox((8, 10, 6), (13, 14, 9), HAY, bevel=1)
    f.rbox((17, 10, 0), (22, 14, 4), HAY, bevel=1)
    f.tidy()
    return f


def barn_fig():
    """The cow barn (the pasture's stable): a red barn with white trim,
    a gable roof, big double doors with a white cross on each, its front
    +y."""
    f = Fig((34, 24, 32))
    f.box((1, 2, 0), (32, 21, 17), BARN_RED)
    for k in range(13):
        x0, x1 = 1 + k, 32 - k
        if x0 > x1:
            break
        f.box((x0, 2, 18 + k), (x1, 21, 18 + k), BARN_RED)
    for k in range(14):
        x0, x1 = k, 33 - k
        if x0 > x1:
            break
        for y in (1, 22):
            f.put((x0, y, 17 + k), TRIM)
            f.put((x1, y, 17 + k), TRIM)
        f.box((x0, 2, 17 + k), (x0, 21, 17 + k), SHED_ROOF)
        f.box((x1, 2, 17 + k), (x1, 21, 17 + k), SHED_ROOF)
    for x0 in (10, 17):
        f.box((x0, 22, 0), (x0 + 6, 22, 12), DOOR)
        for k in range(7):
            f.put((x0 + k, 22, k * 2), TRIM)
            f.put((x0 + 6 - k, 22, k * 2), TRIM)
        f.box((x0, 22, 12), (x0 + 6, 22, 12), TRIM)
        f.box((x0, 22, 0), (x0, 22, 12), TRIM)
        f.box((x0 + 6, 22, 0), (x0 + 6, 22, 12), TRIM)
    f.rbox((14, 22, 20), (19, 22, 24), WINDOW, bevel=1)
    f.tidy()
    return f


def sign_fig(animal):
    """A pen's sign at its gate, its front +y: a board on two posts with
    the pen's animal on it (a chicken's head, a sheep's, a cow's)."""
    f = Fig((12, 4, 16))
    for x in (1, 10):
        f.box((x, 1, 0), (x, 2, 10), POST)
    f.rbox((0, 1, 7), (11, 2, 15), WOOD, bevel=1)
    pic = {
        "chicken": ([".R..", "WWW.", "WEWB", "WWW."], {"R": COMB, "W": CHICKEN_COLOURS["white"], "E": figures.EYE, "B": BEAK}),
        "sheep": (["WWWW", "FFFF", "FEFE", "FFFF"], {"W": WOOL["white"], "F": SHEEP_FACE["white"], "E": figures.GLINT}),
        "cow": (["H..H", "BBBB", "BEBE", "MMMM"], {"H": HORN, "B": COW_COLOURS["brown"][0], "E": figures.EYE, "M": MUZZLE}),
    }[animal]
    rows, colours = pic
    for r, row in enumerate(rows):
        for c, ch in enumerate(row):
            if ch != ".":
                f.put((4 + c, 3, 13 - r), colours[ch])
    f.tidy()
    return f


def trough_fig(fill):
    """A wooden trough, water in it for the cows or grain for the hens."""
    f = Fig((14, 6, 5))
    f.rbox((0, 0, 0), (13, 5, 4), WOOD, bevel=1)
    f.box((1, 1, 2), (12, 4, 4), fill)
    f.box((1, 1, 4), (12, 4, 4), fill)
    f.v = {p: c for p, c in f.v.items() if not (1 <= p[0] <= 12 and 1 <= p[1] <= 4 and p[2] == 4)}
    f.box((1, 1, 3), (12, 4, 3), fill)
    f.tidy()
    return f


# The farm's gate (2026-10-26, the user: the bunny goes out through it,
# the road leads out for the trips later): two posts as the fence's, a
# beam across them with a board, the two wings standing open into the
# farm. Odd across (33), so it stays on its cell turned to the camera;
# its front +y, the wings towards -y (into the farm, once turned).
def gate_fig():
    f = Fig((33, 12, 26))
    for x in (0, 30):
        f.box((x, 6, 0), (x + 2, 8, 22), POST)
    f.box((0, 6, 20), (32, 8, 22), POST)
    f.rbox((11, 9, 18), (21, 10, 25), WOOD, bevel=1)
    for x in (3, 29):
        f.box((x, 0, 2), (x, 0, 14), RAIL)
        f.box((x, 5, 2), (x, 5, 14), RAIL)
        for z in (3, 13):
            f.box((x, 1, z), (x, 4, z), RAIL)
        for k in range(4):
            f.put((x, 1 + k, 4 + k * 2), RAIL)
            f.put((x, 1 + k, 5 + k * 2), RAIL)
    f.tidy()
    return f


# The barrier at the road's end, where nobody goes on: a post, a beam
# striped red and white lying across the road on a rest. Odd across (33).
def barrier_fig():
    f = Fig((33, 5, 14))
    f.rbox((0, 1, 0), (3, 4, 13), POST, bevel=1)
    for x in range(4, 33):
        f.box((x, 2, 9), (x, 3, 10), RED if (x // 4) % 2 == 0 else WHITE)
    f.box((29, 2, 0), (30, 3, 8), POST)
    f.tidy()
    return f


# The merchants are people of the farm world as the bunny is (the user:
# sellers and buyers, not pets): upright, dressed, the bunny's build (its
# boots, legs, body, arms and head, a voxel wider for the tail and the
# hat), each with its own head, clothes and tail, their front +y. Frames:
# standing, and a gesture (the cat holds up a recipe, the dog waves, the
# fox shows a coin). The cat had a cook's apron for a while: well meant,
# taken off again (the user).
TEAL = (62, 136, 146)
TROUSERS_BROWN = (112, 82, 60)
TWEED = (112, 98, 72)
VEST_BROWN = (128, 86, 54)
SHIRT_CREAM = (238, 224, 198)
TROUSERS_DARK = (70, 72, 92)
TOPHAT = (46, 42, 50)
SHOE = (40, 36, 40)
SCROLL = (244, 230, 196)
COIN_GOLD = (244, 196, 64)


def merchant_fig(kind, gesture):
    f = Fig((12, 10, 23))
    fur = {"cat": CAT_FUR, "dog": DOG_FUR, "fox": FOX_FUR}[kind]
    light = {"cat": CAT_WHITE, "dog": DOG_CREAM, "fox": FOX_WHITE}[kind]
    top = {"cat": TEAL, "dog": VEST_BROWN, "fox": WAISTCOAT}[kind]
    legs = {"cat": TROUSERS_BROWN, "dog": TROUSERS_DARK, "fox": TROUSERS_DARK}[kind]
    boots = {"cat": BOOT, "dog": BOOT, "fox": SHOE}[kind]
    # boots and legs, the body: trousers to the waist, the top over it
    for x0 in (3, 7):
        f.box((x0, 3, 0), (x0 + 1, 5, 1), boots)
        f.box((x0, 3, 2), (x0 + 1, 4, 3), legs)
    f.rbox((3, 2, 4), (8, 5, 8), lambda p, n: legs if p[2] <= 5 else top, bevel=1, axes=2)
    if kind == "dog":
        # the shirt where the waistcoat is open, a red neckerchief
        f.box((5, 5, 6), (6, 5, 8), SHIRT_CREAM)
        f.box((4, 6, 8), (7, 6, 8), BANDANA)
        f.box((5, 6, 7), (6, 6, 7), BANDANA)
    else:
        # a white shirt front and a gold button on the waistcoat
        f.box((5, 5, 6), (6, 5, 8), FOX_WHITE)
        f.put((4, 5, 7), COIN_GOLD)
    # arms: sleeves in the top's colour, paws in fur; the right one up in
    # the gesture
    for x in (2, 9):
        f.box((x, 3, 7), (x, 4, 8), top)
        if gesture and x == 9:
            if kind == "dog":
                f.box((x, 3, 9), (x, 4, 10), top)
                f.box((x, 3, 11), (x, 4, 11), fur)
            else:
                f.box((x, 5, 7), (x, 6, 8), top)
                f.box((x, 7, 7), (x, 7, 8), fur)
                if kind == "cat":
                    f.box((x - 1, 8, 7), (x + 1, 8, 10), SCROLL)
                    f.put((x, 8, 8), BANDANA)
                else:
                    f.box((x, 8, 8), (x, 8, 9), COIN_GOLD)
        else:
            f.box((x, 3, 6), (x, 4, 6), top)
            f.box((x, 3, 5), (x, 4, 5), fur)
    # the head, a box of 8 by 6 by 6 with its edges cut back, the face
    f.rbox((2, 1, 9), (9, 6, 14), fur, bevel=1)
    for x, out in ((4, 3), (7, 8)):
        f.box((x, 6, 11), (x, 6, 12), figures.EYE)
        f.put((out, 6, 12), figures.GLINT)
    if kind == "cat":
        # white cheeks round a pink nose, the chin orange
        f.box((4, 6, 10), (7, 6, 10), light)
        f.box((5, 7, 10), (6, 7, 10), CAT_PINK)
        for x0, tip in ((3, 3), (7, 8)):
            f.box((x0, 3, 15), (x0 + 1, 4, 15), fur)
            f.box((tip, 3, 16), (tip, 4, 16), fur)
            f.put((x0 + 1 if x0 == 3 else x0, 4, 15), CAT_PINK)
        # the tail curling up beside her
        f.box((7, 1, 3), (7, 1, 3), fur)
        f.box((8 if gesture else 9, 0, 3), (8 if gesture else 9, 0, 8), fur)
    elif kind == "dog":
        # a snout with a black nose at its tip, ears hanging down the
        # sides, a flat cap on top with a short peak
        f.box((4, 7, 9), (7, 7, 10), light)
        f.box((5, 7, 10), (6, 7, 10), NOSE_BLACK)
        for x in (1, 10):
            f.box((x, 2, 10), (x, 4, 13), DOG_EAR)
        f.rbox((3, 2, 15), (8, 5, 15), TWEED, bevel=1, axes=2)
        f.box((3, 6, 15), (8, 6, 15), TWEED)
        f.box((5, 1, 5), (6, 0, 6), fur)
        f.box((7 if gesture else 5, 0, 7), (7 if gesture else 6, 0, 8), fur)
    else:
        # a pointed snout, tall ears with dark tips, a little top hat, a
        # big bushy tail with a white tip beside her
        f.box((4, 6, 9), (7, 6, 10), light)
        f.box((5, 7, 9), (6, 8, 10), light)
        f.box((5, 9, 10), (6, 9, 10), NOSE_BLACK)
        for x0 in (2, 8):
            f.box((x0, 3, 15), (x0 + 1, 4, 16), fur)
            f.box((x0 + (0 if x0 == 2 else 1), 3, 17), (x0 + (0 if x0 == 2 else 1), 4, 17), FOX_DARK)
        f.rbox((4, 2, 15), (7, 5, 18), TOPHAT, bevel=0)
        f.box((4, 2, 15), (7, 5, 15), BANDANA)
        f.rbox((3, 1, 14), (8, 6, 14), TOPHAT, bevel=1)
        lift = 1 if gesture else 0
        f.rbox((9, 0, 1 + lift), (11, 2, 6 + lift), fur, bevel=1)
        f.rbox((9, 0, 7 + lift), (11, 2, 8 + lift), FOX_WHITE, bevel=1)
    f.tidy()
    return f


CHICKEN_POSES = ["stand", "walk_a", "walk_b", "peck", "brood", "chick", "chick_walk"]
SHEEP_POSES = ["full", "full_walk", "half", "half_walk", "shorn", "shorn_walk", "graze", "lamb", "lamb_walk"]
COW_POSES = ["stand", "walk_a", "walk_b", "graze", "udder", "calf", "calf_walk"]


def animals():
    for colour in list(CHICKEN_COLOURS) + ["rainbow"]:
        save(f"chicken_{colour}", (9, 12, 12), [chicken_fig(colour, p) for p in CHICKEN_POSES])
    for colour in WOOL:
        save(f"sheep_{colour}", (13, 18, 14), [sheep_fig(colour, p) for p in SHEEP_POSES])
    for colour in COW_COLOURS:
        save(f"cow_{colour}", (11, 23, 16), [cow_fig(colour, p) for p in COW_POSES])
    save("dog", (12, 10, 23), [merchant_fig("dog", False), merchant_fig("dog", True)])
    save("fox", (12, 10, 23), [merchant_fig("fox", False), merchant_fig("fox", True)])
    save("egg", (5, 5, 5), [egg_fig(False), egg_fig(True)])
    save("coop", (22, 20, 22), [coop_fig()])
    save("shed", (30, 20, 24), [shed_fig()])
    save("barn", (34, 24, 32), [barn_fig()])
    save("pen_sign", (12, 4, 16), [sign_fig(a) for a in ("chicken", "sheep", "cow")])
    save("trough", (14, 6, 5), [trough_fig(WATER_TROUGH), trough_fig(GRAIN)])


# ---------------------------------------------------------------- the logo

# The title's logo (c.title_screen.logo_model): BUNNY over FARM in thick,
# round letters of the game's own (strokes drawn with a round brush along
# a skeleton of lines and arcs), each with a chocolate outline two voxels
# wide set three voxels back, its front edge a voxel back; the letters on
# the line (every other a little higher stood out, the user). Bunny ears
# on the B, the Y a carrot (its leaves the arms, the carrot the stem),
# standing higher. Each letter a part: they drop in one by one.
import math  # noqa: E402

LOGO_CREAM = (250, 244, 228)
LOGO_ORANGE = (244, 142, 42)
LOGO_OUTLINE = (92, 56, 36)
LOGO_PINK = (240, 156, 168)
LOGO_LEAF = (98, 184, 70)
LOGO_CARROT = (240, 126, 34)
DEEP, PLATE = 6, 3         # how deep the letters and their outline are


def seg_dist(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / ((dx * dx + dy * dy) or 1e-9)))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def arc(cx, cy, rx, ry, a0, a1, n=14):
    """An arc of an ellipse as segments, from angle a0 to a1 (degrees)."""
    pts = [(cx + rx * math.cos(math.radians(a0 + (a1 - a0) * k / n)), cy + ry * math.sin(math.radians(a0 + (a1 - a0) * k / n))) for k in range(n + 1)]
    return [(*pts[k], *pts[k + 1]) for k in range(n)]


SKELETONS = {
    "B": lambda: [(0, 0, 0, 1), (0, 1, .42, 1)] + arc(.42, .755, .38, .245, 90, -90) + [(.42, .51, 0, .51), (0, .51, .48, .51)] + arc(.48, .255, .52, .255, 90, -90) + [(.48, 0, 0, 0)],
    "U": lambda: [(0, 1, 0, .42)] + arc(.5, .42, .5, .42, 180, 360) + [(1, .42, 1, 1)],
    "N": lambda: [(0, 0, 0, 1), (0, 1, 1, 0), (1, 0, 1, 1)],
    "F": lambda: [(0, 0, 0, 1), (0, 1, .95, 1), (0, .54, .78, .54)],
    "A": lambda: [(0, 0, .38, 1), (.38, 1, .62, 1), (.62, 1, 1, 0), (.2, .3, .8, .3)],   # flat top: room for its hole
    "R": lambda: [(0, 0, 0, 1), (0, 1, .45, 1)] + arc(.45, .74, .5, .26, 90, -90) + [(.45, .48, 0, .48), (.42, .48, 1, 0)],
    "M": lambda: [(0, 0, 0, 1), (0, 1, .5, .4), (.5, .4, 1, 1), (1, 1, 1, 0)],
}
WIDTHS = {"B": .82, "U": .86, "N": .9, "Y": .94, "F": .74, "A": 1.1, "R": .86, "M": 1.08}


def letter_cells(ch, h):
    """The letter `ch` `h` voxels high: its cells (x, y up) and colours,
    and how wide it is."""
    r = max(2.5, h * 0.13)
    w = round(WIDTHS[ch] * h)
    if ch == "Y":
        return carrot_y(w, h, r), w
    segs = [(r + ax * (w - 2 * r), r + ay * (h - 2 * r), r + bx * (w - 2 * r), r + by * (h - 2 * r)) for ax, ay, bx, by in SKELETONS[ch]()]
    cells = {}
    for x in range(-2, w + 2):
        for y in range(-2, h + 2):
            if min(seg_dist(x + 0.5, y + 0.5, *sg) for sg in segs) <= r:
                cells[(x, y)] = None
    return cells, w


def carrot_y(w, h, r):
    """The Y as a carrot: two leaves for its arms, the carrot for its stem
    (a voxel deeper rings across it), its tip below the line."""
    cells = {}
    cx, top = w / 2, h * 0.6
    for x, y0, x1, y1 in ((cx, top, r, h - r), (cx, top, w - r, h - r)):
        for x_ in range(-2, w + 2):
            for y_ in range(int(top) - 2, h + 2):
                if seg_dist(x_ + 0.5, y_ + 0.5, x, y0, x1, y1) <= r * 0.95:
                    cells[(x_, y_)] = LOGO_LEAF
    tip = -h * 0.16
    for y in range(int(tip) - 1, int(top) + 3):
        t = (y + 0.5 - tip) / (top + 2 - tip)
        half = 1.0 + t * (w * 0.26)
        if t > 1.0:
            half -= (t - 1.0) * 14
        for x in range(-2, w + 2):
            if abs(x + 0.5 - cx) <= half:
                cells[(x, y)] = LOGO_CARROT
    return cells


def logo():
    path = GAME / "logo.vvx"
    if kept(path):
        return
    # At most 70 high: under the top of the screen it ends above the
    # title's menu, four rows of it too (CONTINUE), bobbing.
    lines = [("BUNNY", 22, LOGO_CREAM), ("FARM", 26, LOGO_ORANGE)]
    gap, apart, outline = 3, 4, 2
    built = []                                  # per line: (letters, width)
    for word, h, colour in lines:
        letters, x = [], 0
        for i, ch in enumerate(word):
            cells, w = letter_cells(ch, h)
            lift = 5 if ch == "Y" else 0     # on the line, the carrot higher
            letters.append((ch, {(cx + x, cy + lift): c or colour for (cx, cy), c in cells.items()}))
            x += w + gap + 2 * outline
        built.append((letters, x - gap - 2 * outline, h))
    width = max(w for _, w, _ in built) + 2 * outline + 2
    ears = 7
    height = 120                        # room to build in; trimmed below
    parts = []
    base = height - 30 - built[0][2]
    for n, (letters, w, h) in enumerate(built):
        x0 = (width - w) // 2
        for k, (ch, cells) in enumerate(letters):
            f = Fig((width, DEEP, height))
            face = {(x + x0, y + base) for (x, y) in cells}
            colour = {(x + x0, y + base): c for (x, y), c in cells.items()}
            if n == 0 and k == 0:          # bunny ears on the B
                bx0 = min(x for x, _ in face)
                bw = max(x for x, _ in face) - bx0
                top = max(y for _, y in face)
                for ex, lean in ((bx0 + bw * 0.22, -1.2), (bx0 + bw * 0.66, 1.2)):
                    for x in range(int(ex) - 6, int(ex) + 7):
                        for y in range(top - 3, top + ears + 2):
                            d = seg_dist(x + 0.5, y + 0.5, ex, top - 2, ex + lean * 3, top + ears - 2)
                            if d <= 2.6:
                                face.add((x, y))
                                inner = seg_dist(x + 0.5, y + 0.5, ex, top + 1, ex + lean * 3, top + ears - 3) <= 1.0
                                colour[(x, y)] = LOGO_PINK if inner and y > top else LOGO_CREAM
            ring = set()
            for (x, y) in face:
                for dx in range(-outline, outline + 1):
                    for dy in range(-outline, outline + 1):
                        if dx * dx + dy * dy <= outline * outline + 1 and (x + dx, y + dy) not in face:
                            ring.add((x + dx, y + dy))
            for (x, y) in ring:
                for d in range(PLATE):
                    f.put((x, d, y), LOGO_OUTLINE)
            edge = {(x, y) for (x, y) in face if any((x + dx, y + dy) not in face for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))}
            rings = set()
            if ch == "Y":                   # rings across the carrot, a voxel in
                carrot = [(x, y) for (x, y), c in colour.items() if c == LOGO_CARROT]
                ys = sorted({y for _, y in carrot})
                for k2, ry in enumerate(ys[2:-3:4]):
                    row = sorted(x for x, y in carrot if y == ry)
                    side = row[:3] if k2 % 2 == 0 else row[-3:]
                    rings |= {(x, ry) for x in side}
            for (x, y) in face:
                deep = DEEP - (1 if (x, y) in edge or (x, y) in rings else 0)
                for d in range(deep):
                    f.put((x, d, y), colour[(x, y)])
            parts.append((f"{ch.lower()}{n}{k}", f))
        base -= h + apart + 2 * outline + (built[n + 1][2] - h if n + 1 < len(built) else 0)
    # Trim the room left over below and above: the logo as high as it is.
    low = min(z for _, f in parts for (_, _, z) in f.v)
    high = max(z for _, f in parts for (_, _, z) in f.v)
    trimmed = []
    for name, f in parts:
        g = Fig((width, DEEP, high - low + 1))
        for (x, y, z), c in f.v.items():
            g.put((x, y, z - low), c)
        trimmed.append((name, g))
    print(f"logo: {width} across, {high - low + 1} high")
    figures.save_parts(path, trimmed)


# ---------------------------------------------------------------- the farm

# The farm, 24 by 22 cells inside its fence, in a meadow: 22 cells of
# grass to each side, 7 in front and 13 behind, so the camera (looking
# north at a slant, at least 320 by 200 voxels where the bunny stands,
# 474 across on a 21:9 screen, and wider at the top of the picture) sees
# land to the edges of the picture wherever the bunny goes. Its back six
# rows are the barn's (stage 5): three pens side by side behind the
# burrow, the chicken yard, the sheep's meadow, the cows' pasture, each
# with a gate in its front fence and a sign by it, a path along them.
FW, FD = 24, 22
OX, OY = 22, 7
W, D, H = FW + 2 * OX, FD + OY + 13, 2
FIELD = (8, 5, 6, 4)   # its first cell across and in depth (inside the fence), how many each way
OPEN_ROWS = 2

# Inside the fence, in cells: the paths (from the burrow's door down and
# along to the stall), the pond, where flowers grow.
PENS = {"chicken": (0, 7), "sheep": (7, 15), "cow": (15, 23)}   # each pen's fences across, in cells
PEN_FRONT = 16                                                   # their front fence's row; the farm's back fence closes them
GATES = {"chicken": 3, "sheep": 11, "cow": 19}
STABLES_AT = {"chicken": (88, 328), "sheep": (184, 324), "cow": (338, 320)}   # where each pen's stable goes up, in voxels
YARD_CELLS = {(x, y) for x in range(1, 7) for y in range(PEN_FRONT + 1, FD - 1)}   # the chicken yard, sand
PATH_CELLS = ({(3, y) for y in range(4, 13)} | {(x, 4) for x in range(3, 21)} | {(19, y) for y in range(4, 8)}
              | {(16, y) for y in range(5, 15)} | {(x, 15) for x in range(3, 20)}
              | {(g, PEN_FRONT) for g in GATES.values()} | {(GATES["sheep"], PEN_FRONT + 1), (GATES["cow"], PEN_FRONT + 1)})
POND_CELLS = {(x, y) for x in range(4, 7) for y in range(1, 3)}

# The farm's gate in its front fence (a cell across, inside the fence)
# and the road from the path along the field through it down to the
# meadow's edge (level cells), where a barrier closes it; the wood round
# the meadow along every edge, a clearing for the road (2026-10-26).
GATE_X = 12
ROAD = [(GATE_X + OX, y) for y in range(0, OY + 4)]

# Where things stand, in voxels inside the fence (the farm's low corner).
BURROW_AT = (56, 216)
STALL_AT = (312, 140)
KITCHEN_AT = (112, 210)
MAILBOX_AT = (84, 94)
CAT_AT = (264, 104)
PROPS = [
    ("tree", (26, 120)), ("tree", (352, 220)), ("pine", (232, 222)), ("tree", (24, 170)),
    ("pine", (354, 40)), ("tree", (186, 28)), ("bush", (160, 228)), ("bush", (290, 228)),
    ("bush", (26, 40)), ("bush", (356, 104)), ("rock", (256, 30)), ("rock", (150, 176)),
    ("trough_grain", (40, 300)), ("hay", (146, 300)), ("hay", (154, 288)), ("trough_water", (280, 318)),
    ("hay", (344, 168)), ("hay", (358, 154)), ("barrel", (278, 150)), ("crate", (282, 132)),
    ("scarecrow", (236, 122)), ("rock", (132, 28)),
]


def joined(cell, group):
    """The sides of `cell` whose neighbours are in `group`."""
    x, y = cell
    return frozenset(k for k, (dx, dy) in zip(ORDER, ((1, 0), (0, 1), (-1, 0), (0, -1))) if (x + dx, y + dy) in group)


def forest(skip):
    """The wood round the meadow: two staggered rows of trees and firs
    along every edge, crowns touching (`skip`: the cells kept clear)."""
    out, wv, dv, step = [], W * N, D * N, 26
    for row, inset in enumerate((10, 28)):
        off = row * step // 2
        spots = [(x, y) for x in range(6 + off, wv - 6, step) for y in (inset, dv - inset)]
        spots += [(x, y) for y in range(46 + off, dv - 46, step) for x in (inset, wv - inset)]
        for x, y in spots:
            if (x // N, y // N) in skip:
                continue
            kind = "pine" if noise(x, y, 31) < 50 else "tree"
            out.append({"kind": kind, "at": [x + noise(x, y, 32) % 5 - 2, y + noise(x, y, 33) % 5 - 2, N]})
    return out


def outside(cells, things, places):
    """The farm's gate, the road out and the wood round the meadow, on a
    level's cells (level cells: (x, y, z) to (block, turn)), things and
    places: on a fresh level, and with --gate on the one there."""
    gx = GATE_X + OX
    road = set(ROAD)
    # the fence opens at the gate; its neighbours end there
    cells.pop((gx, OY, 1), None)
    fence = {(x, y) for (x, y, z), (name, _) in cells.items() if z == 1 and name.startswith("fence_")}
    for c in ((gx - 1, OY), (gx + 1, OY)):
        cells[(*c, 1)] = piece_for(joined(c, fence), FENCES)
    # the road, joined to the paths there; nothing grows on it
    paths = {(x, y) for (x, y, z), (name, _) in cells.items() if z == 0 and name.startswith("path_")} | road
    for c in road | {(gx, OY + 4)}:
        conn = joined(c, paths)
        if c[1] == 0:
            conn = conn | {"S"}      # on out of the meadow
        cells[(*c, 0)] = piece_for(conn, PATHS)
        cells.pop((*c, 1), None)
    # nothing growing on or by the road; the gate, the barrier, the wood
    near = lambda t: abs(t["at"][0] - (gx * N + 8)) < 24 and t["at"][1] < (OY + 5) * N  # noqa: E731
    things[:] = [t for t in things if not (t["kind"] in ("tree", "pine", "bush", "rock", "hay") and near(t))]
    things.append({"kind": "farm_gate", "at": [gx * N + 8, OY * N + 8, N]})
    things.append({"kind": "barrier", "at": [gx * N + 8, 40, N]})
    clear = {(x, y) for x in range(gx - 1, gx + 2) for y in range(0, 3)}
    things += forest(clear)
    places[:] = [p for p in places if p["name"] not in ("farm_gate", "road_end")]
    places.append({"name": "farm_gate", "at": [gx * N + 8, (OY + 1) * N + 8, N], "facing": [0, -1]})
    places.append({"name": "road_end", "at": [gx * N + 8, 56, N], "facing": [0, -1]})


def level_text(cells, things, places):
    """A level file as the editor writes it."""
    blocks = [b[0] for b in BLOCKS]
    runs = []
    for z in range(H):
        for y in range(D):
            for x in range(W):
                name, turn = cells.get((x, y, z), (None, 0))
                b = blocks.index(name) + 1 if name else 0
                c = [b, turn if b else 0]
                if runs and runs[-1][1:] == c:
                    runs[-1][0] += 1
                else:
                    runs.append([1, *c])
    j = lambda o: json.dumps(o, separators=(",", ":"))  # noqa: E731
    return (
        "{\n"
        f'  "block": {N},\n'
        f'  "size": [{W}, {D}, {H}],\n'
        '  "blocks": [\n    ' + ",\n    ".join(j({"name": n, "kind": k}) for n, k, _ in BLOCKS) + "\n  ],\n"
        '  "cells": [\n    ' + ",\n    ".join(", ".join(j(r) for r in runs[k:k + 10]) for k in range(0, len(runs), 10)) + "\n  ],\n"
        '  "things": [\n    ' + ",\n    ".join(j(t) for t in things) + "\n  ],\n"
        '  "places": [\n    ' + ",\n    ".join(j(p) for p in places) + "\n  ]\n"
        "}\n"
    )


def gate_on_level():
    """--gate: the gate, the road and the wood put on the level there (the
    user's: only those cells, things and places change)."""
    path = GAME / "levels" / "01_farm.json"
    d = json.loads(path.read_text())
    assert d["size"] == [W, D, H] and [b["name"] for b in d["blocks"]] == [b[0] for b in BLOCKS], "a level of another make"
    if any(t["kind"] == "farm_gate" for t in d["things"]):
        print(f"{path}: has its gate already")
        return
    names = [None] + [b["name"] for b in d["blocks"]]
    cells, i = {}, 0
    for count, b, turn in d["cells"]:
        for _ in range(count):
            if b:
                cells[(i % W, (i // W) % D, i // (W * D))] = (names[b], turn)
            i += 1
    things, places = d["things"], d["places"]
    outside(cells, things, places)
    path.write_text(level_text(cells, things, places))
    print(f"{path}: the gate, the road and the wood, {len(things)} things")


def farm():
    path = GAME / "levels" / "01_farm.json"
    if kept(path):
        return
    cells = {}

    def put(x, y, z, block, turn=0):
        cells[(x + OX, y + OY, z)] = (block, turn)

    # the meadow: two grasses, each turned any way, so no pattern repeats
    for x in range(W):
        for y in range(D):
            n = noise(x, y, 7)
            cells[(x, y, 0)] = ("grass_a" if n % 2 else "grass_b", [0, 1, 2, 3][n // 25])
    fx, fy, fw, fd = FIELD
    for i in range(fw):
        for j in range(fd):
            put(fx + i, fy + j, 0, "soil" if j < OPEN_ROWS else "fallow", 0 if j < OPEN_ROWS else noise(i, j, 8) % 4)

    join = joined

    for c in PATH_CELLS:
        conn = join(c, PATH_CELLS)
        if c == (3, 12):
            conn = conn | {"N"}          # on under the burrow's door
        if c == (GATES["chicken"], PEN_FRONT):
            conn = conn | {"N"}          # on into the chicken yard
        put(*c, 0, *piece_for(conn, PATHS))
    for c in YARD_CELLS:
        put(*c, 0, "yard")
    for c in POND_CELLS:
        put(*c, 0, *piece_for(join(c, POND_CELLS), PONDS))
    fence = {(x, y) for x in range(FW) for y in (0, FD - 1)} | {(x, y) for y in range(FD) for x in (0, FW - 1)}
    fence |= {(x, PEN_FRONT) for x in range(FW)} | {(x, y) for x0, x1 in PENS.values() for x in (x0, x1) for y in range(PEN_FRONT, FD)}
    fence -= {(g, PEN_FRONT) for g in GATES.values()}
    for c in fence:
        put(*c, 1, *piece_for(join(c, fence), FENCES))

    # flowers and tufts on the grass, thicker near the burrow and the pond
    def free(x, y):
        inside = 0 <= x - OX < FW and 0 <= y - OY < FD
        ix, iy = x - OX, y - OY
        if inside and ((ix, iy) in PATH_CELLS or (ix, iy) in POND_CELLS or (ix, iy) in fence or (ix, iy) in YARD_CELLS):
            return False
        if inside and fx <= ix < fx + fw and fy <= iy < fy + fd:
            return False
        if inside and ((0 <= ix <= 6 and iy >= 12) or (17 <= ix <= 21 and 7 <= iy <= 10)):
            return False             # under the burrow and the stall
        if inside and any(abs(ix * N + 8 - px) < 14 and abs(iy * N + 8 - py) < 14 for _, (px, py) in PROPS + [("", KITCHEN_AT), ("", MAILBOX_AT)]):
            return False
        if inside and any(abs(ix * N + 8 - px) < 22 and abs(iy * N + 8 - py) < 16 for px, py in STABLES_AT.values()):
            return False             # where a stable may go up
        return cells.get((x, y, 1)) is None

    near = {(x, y) for x in range(0, 7) for y in range(9, 12)} | {(x, y) for x in range(3, 9) for y in range(0, 4)}
    for x in range(W):
        for y in range(D):
            if not free(x, y):
                continue
            n = noise(x, y, 9)
            ix, iy = x - OX, y - OY
            chance = 45 if (ix, iy) in near else 14
            if n < chance:
                kind = ["flowers_a", "flowers_b", "tufts"][noise(x, y, 10) % 3]
                cells[(x, y, 1)] = (kind, [0, 1, 2, 3][noise(x, y, 11) % 4])

    ox, oy = OX * N, OY * N
    things = [{"kind": "burrow", "at": [ox + BURROW_AT[0], oy + BURROW_AT[1], N]},
              {"kind": "stall", "at": [ox + STALL_AT[0], oy + STALL_AT[1], N]}]
    things += [{"kind": k, "at": [ox + x, oy + y, N]} for k, (x, y) in PROPS]
    things += [{"kind": "kitchen", "at": [ox + KITCHEN_AT[0], oy + KITCHEN_AT[1], N]},
               {"kind": "mailbox", "at": [ox + MAILBOX_AT[0], oy + MAILBOX_AT[1], N]}]
    # each pen's sign, left of its gate, against the fence (off the path)
    things += [{"kind": f"sign_{sp}", "at": [ox + (g - 1) * N + 6, oy + PEN_FRONT * N + 2, N]} for sp, g in GATES.items()]
    # trees round the farm, in the meadow, none against the fence
    for x in range(W):
        for y in range(D):
            ix, iy = x - OX, y - OY
            if -2 <= ix <= FW + 1 and -2 <= iy <= FD + 1:
                continue
            if noise(x, y, 12) < 12:
                kind = "pine" if noise(x, y, 13) < 40 else "tree"
                things.append({"kind": kind, "at": [x * N + 4 + noise(x, y, 14) % 8, y * N + 4 + noise(x, y, 15) % 8, N]})
            elif noise(x, y, 16) < 6:
                things.append({"kind": "bush" if noise(x, y, 17) % 2 else "rock", "at": [x * N + 8, y * N + 8, N]})
    places = [
        {"name": "start", "at": [ox + BURROW_AT[0], oy + BURROW_AT[1] - 30, N], "facing": [0, -1]},
        {"name": "burrow", "at": [ox + BURROW_AT[0], oy + BURROW_AT[1] - 26, N], "facing": [0, 1]},
        {"name": "stall", "at": [ox + STALL_AT[0], oy + STALL_AT[1] - 14, N], "facing": [0, 1]},
        {"name": "kitchen", "at": [ox + KITCHEN_AT[0], oy + KITCHEN_AT[1] - 16, N], "facing": [0, 1]},
        {"name": "mailbox", "at": [ox + MAILBOX_AT[0], oy + MAILBOX_AT[1] - 12, N], "facing": [0, 1]},
        {"name": "cat", "at": [ox + CAT_AT[0], oy + CAT_AT[1], N], "facing": [0, -1]},
    ]
    # the barn's: where the bunny stands at each gate, where each pen's
    # stable goes up (at its back), where the dog and the fox sit
    for sp, g in GATES.items():
        places.append({"name": f"{sp}_gate", "at": [ox + g * N + 8, oy + (PEN_FRONT - 1) * N + 6, N], "facing": [0, 1]})
    for sp, (x, y) in STABLES_AT.items():
        places.append({"name": f"{sp}_stable", "at": [ox + x, oy + y, N], "facing": [0, -1]})
    places.append({"name": "dog", "at": [ox + 120, oy + 234, N], "facing": [0, -1]})
    places.append({"name": "fox", "at": [ox + 336, oy + 234, N], "facing": [0, -1]})
    outside(cells, things, places)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(level_text(cells, things, places))
    print(f"{path}: {W} by {D} cells, {len(things)} things")


if __name__ == "__main__":
    (GAME / "blocks").mkdir(parents=True, exist_ok=True)
    for name, _, fill in BLOCKS:
        save_block(name, fill)
    bunny()
    if "--dresses" in sys.argv:          # the farmer in each way to dress it, to compare
        for dress in HATS:
            bunny(dress, f"bunny_{dress}")
    for name, plant in (("wheat", wheat), ("carrot", carrot), ("blueberry", blueberry), ("tomato", tomato),
                        ("radish", radish), ("cauliflower", cauliflower), ("turnip", turnip), ("cabbage", cabbage),
                        ("eggplant", eggplant), ("cucumber", cucumber), ("pumpkin", pumpkin_bed)):
        crop(name, plant)
    save("tree", (28, 28, 40), [tree(False), tree(True)])
    save("pine", (24, 24, 38), [pine()])
    save("bush", (16, 12, 10), [bush()])
    save("rock", (14, 11, 7), [rock()])
    save("hay", (13, 9, 9), [hay()])
    save("barrel", (10, 10, 12), [barrel()])
    save("crate", (10, 10, 9), [crate()])
    save("scarecrow", (18, 7, 28), [scarecrow()])
    burrow()
    stall()
    save("kitchen", (32, 16, 26), [kitchen_fig(n) for n in (1, 2, 3)])
    save("mailbox", (10, 10, 18), [mailbox_fig(False), mailbox_fig(True)])
    save("cat", (12, 10, 23), [merchant_fig("cat", False), merchant_fig("cat", True)])
    animals()
    save("farm_gate", (33, 12, 26), [gate_fig()])
    save("barrier", (33, 5, 14), [barrier_fig()])
    logo()
    farm()
    if "--gate" in sys.argv:
        gate_on_level()

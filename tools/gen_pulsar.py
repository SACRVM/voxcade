#!/usr/bin/env python3
"""Makes Pulsar (games/pulsar), a space shooter that scrolls up the screen:
the ships and the explosion as voxel models, the level as a map (a field
of stars far below, asteroids drifting at every depth, a space station to
start from and one to fly over), all from the plans and numbers below.

The ships are drawn as plans seen from above, a letter a voxel (the nose
at the top: models face +y), each letter a colour and the layers it fills,
so a hull stands thicker than its wings and a cockpit rises over both.

Plain Python with PIL for the ground's pictures; the same run makes the
same files."""

import json
import math
import random
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gen_sokovox import write_vox  # noqa: E402

GAME = Path(__file__).resolve().parent.parent / "games" / "pulsar"

# The level: 240 voxels across, 8000 long (the ship flies north, +y), 64
# high; the ship flies at FLY (the start station's deck), the asteroids and
# the station drift below it, the stars lie on the floor.
SIZE = (240, 8000, 64)
FLY = 40
START = (120, 60)


def model(path, size, frames):
    """frames: lists of (x, y, z, (r, g, b)); a voxel outside `size` is
    left out."""
    index, palette, out = {}, {}, []
    for f in frames:
        vox = {}
        for x, y, z, c in f:
            if 0 <= x < size[0] and 0 <= y < size[1] and 0 <= z < size[2]:
                vox[(x, y, z)] = c
        frame = []
        for (x, y, z), c in sorted(vox.items()):
            i = index.setdefault(c, len(index) + 1)
            palette[i] = c
            frame.append((x, y, z, i))
        out.append(frame)
    path.parent.mkdir(parents=True, exist_ok=True)
    write_vox(path, size, out, palette)


def plan(rows, kinds, tilt=0.0, lift=0):
    """A ship from its plan seen from above (the first row its nose):
    `kinds` maps a letter to its colour and the layers it fills (lo, hi).
    `tilt` banks it (voxels its wing tips rise and fall, right up when
    positive), `lift` raises it all."""
    depth, width = len(rows), max(len(r) for r in rows)
    middle = (width - 1) / 2
    out = []
    for i, row in enumerate(rows):
        y = depth - 1 - i
        for x, ch in enumerate(row):
            if ch not in kinds:
                continue
            colour, (lo, hi) = kinds[ch]
            bank = round((x - middle) / max(middle, 1) * tilt)
            for z in range(lo, hi + 1):
                out.append((x, y, z + bank + lift, colour))
    return out


def size_of(rows, high):
    return (max(len(r) for r in rows), len(rows), high)


# ---------------------------------------------------------------- the player

WHITE, LIGHT, STEEL, DARK = (226, 232, 240), (168, 182, 200), (104, 116, 136), (52, 58, 72)
GLASS, GLASS_HI = (60, 180, 230), (150, 230, 255)
RED, ORANGE = (220, 60, 50), (250, 150, 40)

SHIP = [
    "......w......",
    ".....www.....",
    ".....wGw.....",
    "....wGgGw....",
    "....wGGGw....",
    "...wwwwwww...",
    "..lwwrwrwwl..",
    ".llwwwwwwwll.",
    "lll.wwwww.lll",
    "ll..swwws..ll",
    "r...ssdss...r",
    "....dd.dd....",
    "....dd.dd....",
]
SHIP_KINDS = {
    "w": (WHITE, (1, 2)), "l": (LIGHT, (1, 1)), "s": (STEEL, (1, 2)), "d": (DARK, (1, 2)),
    "G": (GLASS, (3, 3)), "g": (GLASS_HI, (3, 3)), "r": (RED, (1, 1)),
}


def ship():
    # Under the glass the hull goes on.
    under = {k: v for k, v in SHIP_KINDS.items()}
    frames = []
    for tilt in (0, -1.5, 1.5):
        body = plan(SHIP, {**under, "G": (WHITE, (1, 2)), "g": (WHITE, (1, 2))}, tilt)
        body += plan(SHIP, {"G": SHIP_KINDS["G"], "g": SHIP_KINDS["g"]}, tilt)
        frames.append(body)
    model(GAME / "ship.vvx", size_of(SHIP, 6), frames)


# ---------------------------------------------------------------- enemies

MAROON, CRIMSON, PURPLE, VIOLET, EYE = (110, 30, 44), (176, 44, 60), (90, 50, 130), (150, 90, 200), (255, 220, 80)
TEAL, SEA, MOSS = (40, 140, 130), (80, 200, 170), (30, 80, 70)
IRON, GUNMETAL, RUST, HAZARD = (90, 94, 100), (60, 64, 72), (150, 80, 50), (230, 190, 40)

FIGHTER = [
    ".....c.....",
    "....ccc....",
    "...ccEcc...",
    ".pccccccccp",
    "pcc.ccc.ccp",
    "pc..mmm..cp",
    "p....m....p",
]
FIGHTER_KINDS = {
    "c": (CRIMSON, (1, 2)), "m": (MAROON, (1, 2)), "p": (PURPLE, (1, 1)), "E": (EYE, (2, 3)),
}

SWOOPER = [
    "......s......",
    ".....sEs.....",
    "t...sssss...t",
    "tt.sssssss.tt",
    ".ttsssMsssss.",
    "..tt.sMs.tt..",
    "...t..M..t...",
]
SWOOPER_KINDS = {
    "s": (SEA, (1, 2)), "t": (TEAL, (1, 1)), "M": (MOSS, (1, 2)), "E": (EYE, (2, 3)),
}

HEAVY = [
    ".....ggggg.....",
    "....gIIIIIg....",
    "...gIIhIhIIg...",
    "..gIIIIIIIIIg..",
    ".gIIIIEEEIIIIg.",
    "gIIIIIEEEIIIIIg",
    "gIIrIIIIIIIrIIg",
    "gIIIIIIIIIIIIIg",
    ".gIIIIIIIIIIIg.",
    "..gIIhIIIhIIg..",
    "...gIIIIIIIg...",
    "....g.g.g.g....",
]
HEAVY_KINDS = {
    "g": (GUNMETAL, (1, 2)), "I": (IRON, (1, 3)), "h": (HAZARD, (1, 3)), "r": (RUST, (1, 4)), "E": (EYE, (3, 4)),
}


def enemies():
    model(GAME / "fighter.vvx", size_of(FIGHTER, 5), [plan(FIGHTER, FIGHTER_KINDS)])
    model(GAME / "swooper.vvx", size_of(SWOOPER, 5), [plan(SWOOPER, SWOOPER_KINDS)])
    model(GAME / "heavy.vvx", size_of(HEAVY, 6), [plan(HEAVY, HEAVY_KINDS)])


# The boss: a carrier 48 across and 30 long, its nose to the south when
# the game turns it to the player; three gun turrets along its back and a
# core that glows in the middle (the game lights it).
def boss():
    w, d = 48, 30
    out = []
    rnd = random.Random(7)
    for y in range(d):
        # A wedge: broad at the back (y 0), narrow at the nose.
        half = int(23 - max(0, y - 8) * 0.55)
        for x in range(w):
            dx = abs(x - (w - 1) / 2)
            if dx > half:
                continue
            edge = dx > half - 2 or y < 2
            top = 3 if edge else 5 - (1 if dx > half - 6 else 0)
            colour = GUNMETAL if edge else (IRON if (x // 4 + y // 4) % 2 else (100, 104, 112))
            for z in range(1, top + 1):
                out.append((x, y, z, colour))
            if not edge and rnd.random() < 0.04:
                out.append((x, y, top + 1, HAZARD))
    # The engines along the back.
    for x in range(6, w - 6, 6):
        for dx in range(3):
            for z in range(1, 5):
                out.append((x + dx, 0, z, DARK))
    # The core, the turrets, the bridge.
    for x in range(21, 27):
        for y in range(11, 17):
            for z in range(5, 8):
                out.append((x, y, z, (200, 40, 60) if (x in (21, 26) or y in (11, 16)) else (255, 90, 110)))
    for tx, ty in ((10, 8), (23, 22), (36, 8)):
        for x in range(tx, tx + 3):
            for y in range(ty, ty + 3):
                for z in range(5, 8):
                    out.append((x, y, z, GUNMETAL))
        for z in range(6, 8):
            out.append((tx + 1, ty + 3, z, STEEL))
            out.append((tx + 1, ty + 4, z, STEEL))
    model(GAME / "boss.vvx", (w, d, 10), [out])


# ---------------------------------------------------------------- effects

def boom():
    """An explosion in six frames: a white flash swelling to a ball of fire,
    then breaking up into smoke."""
    n = 17
    c = n // 2
    rnd = random.Random(11)
    frames = []
    looks = [
        (2.0, 1.0, [(255, 255, 230)]),
        (3.6, 1.0, [(255, 240, 160), (255, 210, 90)]),
        (5.0, 0.9, [(255, 190, 60), (250, 140, 40), (255, 230, 120)]),
        (6.2, 0.7, [(240, 110, 40), (200, 70, 30), (250, 160, 60)]),
        (7.2, 0.45, [(150, 60, 40), (90, 80, 80), (200, 90, 40)]),
        (8.0, 0.2, [(80, 76, 76), (60, 58, 60)]),
    ]
    for radius, fill, colours in looks:
        f = []
        for x in range(n):
            for y in range(n):
                for z in range(n):
                    r = math.dist((x, y, z), (c, c, c))
                    if r <= radius and r >= radius - 2.2 and rnd.random() < fill:
                        f.append((x, y, z, rnd.choice(colours)))
        frames.append(f)
    model(GAME / "boom.vvx", (n, n, n), frames)


def powerup():
    """A crystal on a ring: the game turns it round."""
    out = []
    c = 4
    for x in range(9):
        for y in range(9):
            for z in range(9):
                d = abs(x - c) + abs(y - c) + abs(z - c)
                if d <= 4:
                    out.append((x, y, z, (120, 230, 255) if d <= 2 else (40, 120, 230)))
    model(GAME / "power.vvx", (9, 9, 9), [out])


# ---------------------------------------------------------------- the level

def blob(radius, seed, colours):
    """An asteroid: a lumpy ball, its colours in patches and craters."""
    rnd = random.Random(seed)
    n = int(radius * 2 + 3)
    c = (n - 1) / 2
    bumps = [(rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(0.15, 0.35)) for _ in range(7)]
    craters = [(rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(0.2, 1)) for _ in range(4)]
    out = []
    for x in range(n):
        for y in range(n):
            for z in range(n):
                v = ((x - c) / radius, (y - c) / radius, (z - c) / radius)
                r = math.sqrt(sum(a * a for a in v))
                lump = sum(s * max(0.0, 1 - math.dist(v, (bx, by, bz))) for bx, by, bz, s in bumps)
                if r > 0.85 + lump * 0.5:
                    continue
                if any(math.dist(v, (cx, cy, cz)) < 0.32 and r > 0.7 for cx, cy, cz in craters):
                    continue
                shade = (x // 3 + y // 3 + z // 2 + seed) % len(colours)
                out.append((x, y, z, colours[shade]))
    return (n, n, n), out


def station(length):
    """A space station seen from above: a spine of modules with docking
    rings, solar panels to the sides on struts, lights."""
    w, h = 72, 18
    out = []
    panel, frame, hull, hull2, light = (40, 60, 120), (150, 156, 170), (180, 186, 196), (130, 136, 150), (255, 200, 80)
    mid = w // 2
    for y in range(length):
        module = (y // 24) % 2 == 0
        r = 7 if module else 4
        for x in range(mid - r, mid + r + 1):
            for z in range(h // 2 - r, h // 2 + r + 1):
                if (x - mid) ** 2 + (z - h // 2) ** 2 <= r * r:
                    out.append((x, y, z, hull if (y // 6) % 2 else hull2))
        if module and y % 24 == 11:
            for x in range(mid - r - 1, mid + r + 2):
                out.append((x, y, h // 2 + r, light))
        if y % 48 in range(30, 46):
            # Solar wings on a strut.
            for x in list(range(2, mid - 9)) + list(range(mid + 10, w - 2)):
                edge = y % 48 in (30, 45) or x in (2, mid - 10, mid + 10, w - 3)
                out.append((x, y, h // 2, frame if edge or x % 6 == 0 else panel))
        if y % 48 == 38:
            for x in range(mid - 9, mid + 10):
                out.append((x, y, h // 2, frame))
    return (w, length, h), out


def launch_pad():
    """Where the run starts: a deck of 44 by 52 at the height the ships
    fly, on a tower, with lights along its edges and a mark in its middle."""
    w, d, h = 44, 52, FLY
    out = []
    deck, deck2, mark, light, tower = (120, 128, 140), (100, 108, 120), (230, 190, 40), (255, 220, 120), (80, 86, 98)
    for x in range(w):
        for y in range(d):
            edge = x in (0, w - 1) or y in (0, d - 1)
            colour = light if edge and (x + y) % 4 == 0 else (deck if (x // 4 + y // 4) % 2 else deck2)
            if abs(x - w // 2) + abs(y - d // 2) in (9, 10) and not edge:
                colour = mark
            for z in range(h - 3, h):
                out.append((x, y, z, colour))
    for x in range(w // 2 - 5, w // 2 + 5):
        for y in range(d // 2 - 5, d // 2 + 5):
            for z in range(0, h - 3):
                if x in (w // 2 - 5, w // 2 + 4) or y in (d // 2 - 5, d // 2 + 4) or z % 6 == 0:
                    out.append((x, y, z, tower))
    return (w, d, h), out


def level():
    rnd = random.Random(2026)
    prefabs, place = {}, []
    (GAME / "prefabs").mkdir(parents=True, exist_ok=True)

    size, voxels = launch_pad()
    model(GAME / "prefabs" / "launch_pad.vox", size, [voxels])
    prefabs["launch_pad"] = "prefabs/launch_pad.vox"
    place.append({"prefab": "launch_pad", "at": [START[0], START[1], 0]})

    rock = [[(96, 88, 82), (120, 110, 100), (80, 74, 70)], [(110, 96, 80), (140, 120, 96), (90, 78, 66)], [(84, 90, 100), (110, 116, 126), (70, 74, 84)]]
    for k in range(8):
        radius = (4, 6, 8, 10, 13, 5, 7, 16)[k]
        size, voxels = blob(radius, k + 1, rock[k % 3])
        model(GAME / "prefabs" / f"rock_{k}.vox", size, [voxels])
        prefabs[f"rock_{k}"] = f"prefabs/rock_{k}.vox"
    y = 160
    while y < SIZE[1] - 120:
        k = rnd.randrange(8)
        x = rnd.randrange(10, SIZE[0] - 10)
        z = rnd.randrange(2, FLY - 14)
        place.append({"prefab": f"rock_{k}", "at": [x, y, z], "turn": rnd.randrange(4)})
        y += rnd.randrange(30, 90)

    size, voxels = station(240)
    model(GAME / "prefabs" / "station.vox", size, [voxels])
    prefabs["station"] = "prefabs/station.vox"
    place.append({"prefab": "station", "at": [SIZE[0] // 2 - 30, 3400, 6]})

    # The floor: stars, a voxel each, far below.
    w, d = SIZE[0], SIZE[1]
    height = Image.new("L", (w, d), 0)
    pixels = height.load()
    for _ in range(w * d // 180):
        x, y = rnd.randrange(w), rnd.randrange(d)
        pixels[x, d - 1 - y] = 1
    (GAME / "ground").mkdir(parents=True, exist_ok=True)
    height.save(GAME / "ground" / "height.png", optimize=True)
    stars = {"surface": ["#f4f6ff", "#c6d8ff", "#fff0c8", "#9cb8ff", "#ffffff"], "depth": 1, "grain": 1.0}
    data = {
        "size": list(SIZE),
        "ground": {"height": "ground/height.png", "materials": [stars]},
        "prefabs": prefabs,
        "place": place,
    }
    (GAME / "map.json").write_text(json.dumps(data, indent=1) + "\n")


def main():
    ship()
    enemies()
    boss()
    boom()
    powerup()
    level()
    print(f"wrote {GAME}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Pulsar's models at the detail of a 16-bit sprite, in voxels: shapes (a
hull, a canopy, wings, engines) cut out of the grid, every voxel coloured
by the part it belongs to, so the ship is some 31 voxels across where
tools/gen_pulsar.py drew 13. The same run makes the same files:

    python3 make_models.py
"""

import math
import random
import struct
from pathlib import Path

from make_level import noise3

HERE = Path(__file__).resolve().parent

WHITE, LIGHT, STEEL, DARK = (226, 232, 240), (176, 188, 204), (110, 122, 142), (46, 52, 66)
GLASS, GLASS_HI, GLASS_DEEP = (50, 160, 220), (160, 235, 255), (30, 90, 150)
RED, BLUE, ORANGE, HOT = (214, 54, 48), (52, 120, 220), (250, 150, 40), (255, 230, 150)


def build(size, part, turn=0.0):
    """Every voxel of `size` whose middle `part` gives a colour for, the
    model's middle at the origin; `turn` rolls it about its length
    (radians, the right wing up when positive)."""
    w, d, h = size
    cos, sin = math.cos(-turn), math.sin(-turn)
    out = []
    for x in range(w):
        for y in range(d):
            for z in range(h):
                px, py, pz = x - (w - 1) / 2, y - (d - 1) / 2, z - (h - 1) / 2
                # Back from the rolled model to the level one.
                qx, qz = px * cos - pz * sin, px * sin + pz * cos
                c = part(qx, py, qz)
                if c:
                    out.append((x, y, z, c))
    # Only what can be seen: a voxel shut in on all six sides is left out.
    filled = {(x, y, z) for x, y, z, _ in out}
    near = ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))
    return [v for v in out if not all((v[0] + a, v[1] + b, v[2] + c) in filled for a, b, c in near)]


# ---------------------------------------------------------------- the ship

def ship_part(x, y, z):
    """The player's ship, its nose to +y, its middle at 0: a slim hull with
    a canopy, swept wings with fins at their tips, an engine under each."""
    ax = abs(x)
    # The canopy: glass over the hull's front, brighter where it catches
    # the light.
    cy, cz = 5.5, 2.0
    if ((x / 2.3) ** 2 + ((y - cy) / 5.2) ** 2 + ((z - cz) / 2.2) ** 2) <= 1 and z >= 0.8:
        if y > cy + 1.5 and z > cz + 0.6:
            return GLASS_HI
        return GLASS_DEEP if z < 1.6 else GLASS
    # The hull: as wide as it is from the engines back, a point at the nose.
    if -15 <= y <= 17:
        k = 1.0 if y < 2 else max(0.0, (17 - y) / 15) ** 0.6
        hw, hh = 4.6 * k + 0.3, 2.8 * k + 0.4
        if (ax / hw) ** 2.4 + (abs(z + 0.4) / hh) ** 2.4 <= 1:
            if y < -13:
                return DARK
            if y % 7 == 0:
                return STEEL
            if ax > hw - 1.1 and z > -0.5:
                return BLUE if -9 < y < 10 else WHITE
            return WHITE if z > -1.2 else LIGHT
    # The engines under the wings' roots, glowing at their backs.
    ex, ez = 6.6, -1.2
    r = math.hypot(ax - ex, z - ez)
    if -16.5 <= y <= -1 and r <= 2.4 - max(0, y + 3) * 0.5:
        if y < -15:
            return HOT if r < 1.0 else (ORANGE if r < 1.6 else DARK)
        if y > -3.5:
            return DARK
        return LIGHT if z > ez + 0.8 else STEEL
    # The wings: swept back from the hull, thinner towards their tips.
    if 4 <= ax <= 15.5:
        lead = 6 - (ax - 4) * 12 / 11.5
        trail = -11.5 + (ax - 4) * 0.12
        thick = 1.0 - (ax - 4) * 0.035
        if trail <= y <= lead and abs(z + 1.0) <= thick:
            if lead - y < 1.4:
                return RED if ax > 11 else WHITE
            if abs(ax - 10) < 0.5 or abs(y - trail) < 0.9:
                return STEEL
            return WHITE if z > -1.1 else LIGHT
    # A fin at each wing's tip, raked back.
    if 14.5 <= ax <= 15.5 and -11 <= y <= -4:
        top = 2.6 - (y + 11) * 0.35
        if -2 <= z <= top:
            return RED if z > top - 1.2 else LIGHT
    return None


def ship():
    size = (33, 36, 11)
    frames = [build(size, ship_part, t) for t in (0.0, 0.32, -0.32)]
    write_vox_frames(HERE / "ship.vox", size, frames)


# ---------------------------------------------------------------- enemies
# Each faces +y here, as the ship does: the game turns them to the player.

MAROON, CRIMSON, ROSE, PURPLE, VIOLET = (100, 26, 40), (176, 44, 60), (220, 90, 100), (90, 50, 130), (150, 90, 200)
EYE, EYE_HI = (255, 200, 60), (255, 250, 200)
TEAL, SEA, FOAM, MOSS = (36, 130, 122), (70, 196, 166), (150, 240, 210), (26, 74, 66)
IRON, IRON_HI, GUNMETAL, RUST, HAZARD = (96, 100, 108), (132, 136, 144), (58, 62, 70), (150, 80, 50), (230, 190, 40)
GLOW = (255, 120, 90)
LENS, LAMP = (255, 70, 60), (200, 40, 40)
GEM, GEM_HI, GEM_DEEP, GEM_CORE = (70, 190, 255), (190, 245, 255), (40, 110, 210), (255, 255, 255)


def ball(x, y, z, cx, cy, cz, r):
    return math.dist((x, y, z), (cx, cy, cz)) <= r


def fighter_part(x, y, z):
    """A small interceptor: a pod with a yellow eye, crescent wings swept
    forward to purple claws, two little engines behind."""
    ax = abs(x)
    if ball(x, y, z, 0, 6.5, 1.4, 1.7):
        return EYE_HI if math.dist((x, y, z), (0, 7.2, 2.0)) < 0.9 else EYE
    if (x / 3.6) ** 2 + ((y - 1) / 8.5) ** 2 + (z / 2.4) ** 2 <= 1:
        if y % 5 == 0:
            return MAROON
        return CRIMSON if z > 0.3 else MAROON
    if 3 <= ax <= 12.5:
        mid = -3 + (ax - 3) ** 1.6 * 0.28
        half = 3.2 - (ax - 3) * 0.17
        if abs(y - mid) <= half and abs(z + 0.3) <= 0.95 - (ax - 3) * 0.03:
            if ax > 10:
                return PURPLE
            if y > mid + half - 1.1:
                return VIOLET
            return CRIMSON if z > -0.4 else MAROON
    if math.hypot(ax - 2.4, z + 0.6) <= 1.3 and -10 <= y <= -6:
        return GLOW if y < -9 else GUNMETAL
    return None


def swooper_part(x, y, z):
    """A manta: a broad flat body whose wings curl forward at their tips,
    two eyes, fins by its head, a tail."""
    ax = abs(x)
    if ball(ax, y, z, 3.0, 6.4, 1.3, 1.3):
        return EYE
    if ax <= 16.5:
        u = ax / 16.5
        front = 7.5 - u * u * 11
        back = -6 + u * u * 7.5
        thick = 3.6 * (1 - u ** 1.5) + 0.7
        if back <= y <= front and abs(z) <= thick / 2:
            if front - y < 1.0:
                return FOAM
            if ax < 2.6 and z > 0:
                return SEA
            if z < -0.3:
                return MOSS
            return SEA if (front - y) < 3.0 and u < 0.6 else TEAL
    if 2.0 <= ax <= 4.0 and 7.5 <= y <= 10.5 and abs(z) <= 0.7:
        return TEAL
    if ax <= 0.8 and -13 <= y < -5 and abs(z) <= 0.8:
        return MOSS
    return None


def heavy_part(x, y, z):
    """A gunship: an armoured hull of plates, an eye under a dome, three
    guns forward, pods with hazard stripes to its sides, engines behind."""
    ax, ay = abs(x), abs(y)
    if ball(x, y, z, 0, 1, 2.0, 3.8) and z >= 2.0:
        return EYE_HI if ball(x, y, z, 0, 2.2, 4.4, 1.2) else (EYE if math.dist((x, y), (0, 1)) < 2.6 else RUST)
    for gx in (-7, 0, 7):
        if math.hypot(x - gx, z - 1.0) <= 1.1 and 10 <= y <= 17:
            return GUNMETAL if y > 16 else IRON_HI
    if ax <= 13 and ay <= 12 and ax + ay <= 21:
        top = 2.6 - max(0, ax - 10) * 0.7 - max(0, ay - 9) * 0.7
        # A spine along its middle, and an armour plate either side of it.
        plate = 4 <= ax <= 10 and -9 <= y <= 6
        if ax <= 1.5 or plate:
            top += 1.0
        if -2.6 <= z <= top:
            if z > top - 1:
                if plate and (ax in (4, 10) or y in (-9, 6)):
                    return GUNMETAL
                if ax <= 1.5:
                    return GUNMETAL if y % 4 == 0 else IRON
                if 10 < ax or 9 < ay:
                    return IRON
                return IRON_HI
            return IRON
    if 13 <= ax <= 18 and ((ax - 15.5) / 2.6) ** 2 + (y / 8.5) ** 2 <= 1 and abs(z) <= 2.2:
        if y > 4 and (int(x + y) % 4) < 2:
            return HAZARD
        if y < -6 and z > 1.2:
            return RUST
        return GUNMETAL
    if math.hypot(ax - 8, z) <= 2.5 and -16 <= y < -12:
        return GLOW if y < -15 and math.hypot(ax - 8, z) < 1.6 else (40, 44, 52)
    return None


def noise2(seed):
    """Smooth value noise in 2D, about 0..1: weathering, not a pattern."""
    def h(i, j):
        n = (i * 73856093) ^ (j * 19349663) ^ (seed * 83492791)
        n = (n ^ (n >> 13)) * 1274126177 & 0xFFFFFFFF
        return (n & 0xFFFF) / 0xFFFF

    def f(x, y):
        i, j = math.floor(x), math.floor(y)
        u, v = x - i, y - j
        u, v = u * u * (3 - 2 * u), v * v * (3 - 2 * v)
        top = h(i, j) + (h(i + 1, j) - h(i, j)) * u
        bottom = h(i, j + 1) + (h(i + 1, j + 1) - h(i, j + 1)) * u
        return top + (bottom - top) * v
    return f


IRON_DARK, WINDOW, CORE, CORE_HI, CORE_RIM = (78, 82, 92), (255, 210, 110), (255, 80, 100), (255, 210, 220), (150, 30, 50)
WEAR = noise2(5)
BOSS_TURRETS = [(-26, -6), (26, -6), (-15, 17), (15, 17)]


def boss_part(x, y, z):
    """The carrier: a wedge of a hull broad at the back, its deck curving
    down to the sides; a core that glows in its middle, a bridge behind it
    with lit windows, four twin turrets, hangar pods to the sides, a row of
    engines along the back."""
    ax = abs(x)
    # The core: a dome over the deck, a rim, its heart white hot.
    r = math.dist((x, y, z), (0, 5, 2))
    if r <= 8.5 and z >= 2:
        if r > 7.2:
            return CORE_RIM
        return CORE_HI if r < 3.5 and z > 5 else CORE
    # The turrets: a round base and two barrels to the front.
    for tx, ty in BOSS_TURRETS:
        if math.hypot(x - tx, y - ty) <= 4.2 and 2 <= z <= 5.5:
            return GUNMETAL if z < 5 else IRON_HI
        for bx in (tx - 1.3, tx + 1.3):
            if math.hypot(x - bx, z - 4.5) <= 0.9 and ty + 2 <= y <= ty + 11:
                return DARK if y > ty + 10 else STEEL
    # The bridge: tiers rising behind the core, windows along its front.
    if ax <= 9 and -26 <= y <= -9:
        tier = 9 if ax <= 5 and y <= -14 else 6
        if 2 <= z <= tier:
            front = -9 if tier == 6 else -14
            if y >= front - 0.5 and z in (tier - 1, tier - 2) and x % 2 == 0:
                return WINDOW
            return IRON_HI if z == tier else IRON
    # The hull.
    if -35 <= y <= 35:
        hw = 46 if y < -8 else 46 - (y + 8) * 36 / 43
        if ax <= hw:
            top = 2.5 - (ax / hw) ** 2 * 4.5
            if y in (-21, 0, 19) and ax < hw - 3:
                top += 1
            if -4 <= z <= top:
                if ax > hw - 1.5 or z > top - 1 and ax > hw - 3:
                    return RED if z > top - 1 and int(y) % 8 == 0 else GUNMETAL
                if z <= -3:
                    return DARK
                if y in (-21, 0, 19) and z > top - 1.5:
                    return GUNMETAL
                if z > top - 1:
                    w = WEAR(x * 0.12, y * 0.12)
                    return RUST if w > 0.88 else (IRON_DARK if w > 0.62 else IRON)
                return IRON_DARK
    # The hangar pods to the sides, striped at their mouths.
    if 38 <= ax <= 50 and -32 <= y <= -6 and -3 <= z <= 2:
        if y > -9:
            return HAZARD if int(x + y) % 4 < 2 else DARK
        return IRON_HI if z == 2 else GUNMETAL
    # The engines along the back.
    for ex in (-38, -24, -10, 10, 24, 38):
        d = math.hypot(x - ex, z + 0.5)
        if d <= 3.6 and -38 <= y < -34:
            return (HOT if d < 1.5 else GLOW) if y < -37 else (40, 44, 52)
    return None


# ---------------------------------------------------------------- effects

def boom(n, seed):
    """An explosion in eight frames, `n` across: a white flash, a fireball
    swelling with a churning surface, breaking up into smoke and embers
    while bits of debris fly off."""
    c = (n - 1) / 2
    s = n / 31
    churn, tear = noise3(seed), noise3(seed + 50)
    rnd = random.Random(seed)
    debris = [(rnd.gauss(0, 1), rnd.gauss(0, 1), rnd.gauss(0, 0.6)) for _ in range(14)]
    looks = [
        # radius, shell, how much of it is left, colours from inside out
        (3.0, 9, 1.0, [(255, 255, 240), (255, 250, 200)]),
        (5.5, 9, 1.0, [(255, 255, 220), (255, 230, 140), (255, 200, 80)]),
        (8.0, 3.5, 1.0, [(255, 230, 140), (255, 180, 60), (240, 120, 40)]),
        (10.0, 3.0, 0.75, [(255, 190, 70), (240, 120, 40), (200, 70, 30)]),
        (11.5, 2.8, 0.55, [(230, 110, 40), (160, 60, 40), (100, 84, 84, 200)]),
        (12.5, 2.5, 0.38, [(150, 60, 40), (96, 88, 90, 160), (70, 66, 70, 130)]),
        (13.5, 2.2, 0.22, [(90, 84, 86, 120), (66, 62, 66, 100)]),
        (14.2, 2.0, 0.1, [(70, 66, 70, 80), (54, 52, 56, 60)]),
    ]
    frames = []
    for k, (radius, shell, left, colours) in enumerate(looks):
        radius *= s
        f = []
        for x in range(n):
            for y in range(n):
                for z in range(n):
                    p = (x - c, y - c, z - c)
                    r = math.sqrt(sum(a * a for a in p))
                    # The later, the rougher its edge.
                    rough = 0.2 + 0.08 * k
                    if r > radius * (1 + rough) or r < radius * (1 - rough) - shell * math.sqrt(s):
                        continue
                    f_ = 0.32 / math.sqrt(s)
                    t = churn(x * f_ + k * 0.7, y * f_, z * f_)
                    edge = radius * (1 - rough + 2 * rough * t)
                    if r > edge or r < edge - shell * math.sqrt(s):
                        continue
                    # Torn into rags and puffs: finer than the churning.
                    if left < 1:
                        g = 0.5 / math.sqrt(s)
                        if tear(x * g + k * 1.3, y * g, z * g) > left:
                            continue
                    depth = 1 - (edge - r) / max(shell * math.sqrt(s), 0.1)
                    f.append((x, y, z, colours[min(len(colours) - 1, int(depth * len(colours)))]))
        # The debris: bits flying out, glowing, then cooling.
        if k >= 2:
            for dx, dy, dz in debris:
                d = (k - 1) * 2.2 * s
                bx, by, bz = round(c + dx * d), round(c + dy * d), round(c + dz * d)
                f.append((bx, by, bz, (255, 160, 60) if k < 5 else (110, 90, 80)))
        frames.append(f)
    return frames


def mine_part(x, y, z):
    """A space mine: an iron ball with a band of lamps round its middle and
    spikes out of it every way, a lens on top that blinks (main.lua)."""
    r = math.dist((x, y, z), (0, 0, 0))
    if r <= 5.6:
        if z > 4.2 and math.hypot(x, y) < 1.6:
            return LENS
        if abs(z) < 0.8:
            a = math.atan2(y, x) / (2 * math.pi) * 12
            return LAMP if abs(a - round(a)) < 0.22 else GUNMETAL
        return IRON_HI if z > 2.4 else (IRON if z > -1.5 else GUNMETAL)
    # Spikes: eight across, slanting up and down by turns, and one up.
    for k in range(8):
        a = k * math.pi / 4
        d = (math.cos(a) * 0.94, math.sin(a) * 0.94, 0.34 if k % 2 else -0.34)
        along = x * d[0] + y * d[1] + z * d[2]
        if 4 < along < 10.2:
            off = math.dist((x, y, z), (d[0] * along, d[1] * along, d[2] * along))
            if off <= 1.4 - (along - 4) * 0.17:
                return HAZARD if along > 8.6 else IRON_HI
    return None


def power_part(x, y, z):
    """A power-up: a crystal standing on its point, its facets lit by turns,
    a bright heart showing through at its waist."""
    k = abs(x) / 5.6 + abs(y) / 5.6 + abs(z) / 8.0
    if k > 1:
        return None
    if k < 0.55 and abs(z) < 1.5:
        return GEM_CORE
    if k > 0.84:
        facet = (x > 0) ^ (y > 0) ^ (z > 0)
        return GEM_HI if facet else GEM
    return GEM_DEEP


def effects():
    # Each well over the size of what it tears apart: the fighters and
    # mantas, the heavy and the ship, the boss.
    write_vox_frames(HERE / "boom_small.vox", (41, 41, 41), boom(41, 12))
    write_vox_frames(HERE / "boom.vox", (61, 61, 61), boom(61, 11))
    write_vox_frames(HERE / "boom_boss.vox", (101, 101, 101), boom(101, 13))


def enemies():
    write_vox_frames(HERE / "fighter.vox", (25, 23, 9), [build((25, 23, 9), fighter_part)])
    write_vox_frames(HERE / "swooper.vox", (35, 27, 9), [build((35, 27, 9), swooper_part)])
    write_vox_frames(HERE / "heavy.vox", (37, 35, 13), [build((37, 35, 13), heavy_part)])
    write_vox_frames(HERE / "boss.vox", (101, 79, 21), [build((101, 79, 21), boss_part)])
    write_vox_frames(HERE / "mine.vox", (23, 23, 15), [build((23, 23, 15), mine_part)])


def power():
    write_vox_frames(HERE / "power.vox", (13, 13, 17), [build((13, 13, 17), power_part)])


def write_vox_frames(path, size, frames):
    """A .vox of several frames sharing one palette."""
    index = {}
    bodies = []
    for f in frames:
        inside = [v for v in f if 0 <= v[0] < size[0] and 0 <= v[1] < size[1] and 0 <= v[2] < size[2]]
        bodies.append([(x, y, z, index.setdefault(c, len(index) + 1)) for x, y, z, c in inside])
    assert len(index) <= 255

    def chunk(cid, data, kids=b""):
        return cid + struct.pack("<ii", len(data), len(kids)) + data + kids

    kids = b""
    for body in bodies:
        kids += chunk(b"SIZE", struct.pack("<iii", *size))
        kids += chunk(b"XYZI", struct.pack("<i", len(body)) + b"".join(struct.pack("BBBB", *v) for v in body))
    pal = b"".join(struct.pack("BBBB", *(c if len(c) == 4 else (*c, 255))) for c in index) + b"\0\0\0\xff" * (256 - len(index))
    kids += chunk(b"RGBA", pal)
    path.write_bytes(b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", kids))


def main():
    ship()
    enemies()
    power()
    effects()
    print(f"wrote the models in {HERE}")


if __name__ == "__main__":
    main()

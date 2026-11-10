#!/usr/bin/env python3
"""Pulsar's asteroids, launch pad and station (prefabs/*.vox), made anew
over what tools/gen_pulsar.py made: rocks of broken, uneven shapes instead
of balls, the deck and the station's hull without a checker pattern.
map.json places them as before. The same run makes the same files:

    python3 make_level.py
"""

import json
import math
import random
import struct
from pathlib import Path

HERE = Path(__file__).resolve().parent
FLY = 40


def write_vox(path, size, voxels):
    """voxels: (x, y, z, (r, g, b)); one frame, a palette of its colours."""
    index, body = {}, []
    for x, y, z, c in voxels:
        if 0 <= x < size[0] and 0 <= y < size[1] and 0 <= z < size[2]:
            body.append((x, y, z, index.setdefault(c, len(index) + 1)))
    assert len(index) <= 255, f"{path}: {len(index)} colours"

    def chunk(cid, data, kids=b""):
        return cid + struct.pack("<ii", len(data), len(kids)) + data + kids

    pal = b"".join(struct.pack("BBBB", *c, 255) for c in index) + b"\0\0\0\xff" * (256 - len(index))
    kids = chunk(b"SIZE", struct.pack("<iii", *size))
    kids += chunk(b"XYZI", struct.pack("<i", len(body)) + b"".join(struct.pack("BBBB", *v) for v in body))
    kids += chunk(b"RGBA", pal)
    path.write_bytes(b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", kids))


# ---------------------------------------------------------------- noise

def noise3(seed):
    """Smooth value noise in 3D: a function of a point, about 0..1."""
    def h(i, j, k):
        n = (i * 73856093) ^ (j * 19349663) ^ (k * 83492791) ^ (seed * 2654435761)
        n = (n ^ (n >> 13)) * 1274126177 & 0xFFFFFFFF
        return (n & 0xFFFF) / 0xFFFF

    def f(x, y, z):
        i, j, k = math.floor(x), math.floor(y), math.floor(z)
        u, v, w = x - i, y - j, z - k
        u, v, w = u * u * (3 - 2 * u), v * v * (3 - 2 * v), w * w * (3 - 2 * w)

        def lerp(a, b, t):
            return a + (b - a) * t
        return lerp(
            lerp(lerp(h(i, j, k), h(i + 1, j, k), u), lerp(h(i, j + 1, k), h(i + 1, j + 1, k), u), v),
            lerp(lerp(h(i, j, k + 1), h(i + 1, j, k + 1), u), lerp(h(i, j + 1, k + 1), h(i + 1, j + 1, k + 1), u), v),
            w)
    return f


# ---------------------------------------------------------------- asteroids

def rock(radius, seed, colours):
    """An asteroid: a few lumps grown together, stretched and flattened,
    broken off flat in places, its surface rough; its colours in patches
    that follow no grid, darker in its cracks and craters."""
    rnd = random.Random(seed * 7919)
    rough, patch, crack = noise3(seed), noise3(seed + 100), noise3(seed + 200)
    # Its axes: long, middling and flat (seen from above it is never round).
    ax = (radius * rnd.uniform(1.1, 1.45), radius * rnd.uniform(0.6, 0.85), radius * rnd.uniform(0.45, 0.65))
    turn = rnd.uniform(0, math.pi)
    lumps = [((0, 0, 0), 1.0)]
    for _ in range(rnd.randint(2, 4)):
        a = rnd.uniform(0, 2 * math.pi)
        d = rnd.uniform(0.35, 0.7)
        lumps.append(((math.cos(a) * d, math.sin(a) * d, rnd.uniform(-0.25, 0.25)), rnd.uniform(0.45, 0.75)))
    cuts = []
    for _ in range(rnd.randint(2, 4)):
        n = (rnd.gauss(0, 1), rnd.gauss(0, 1), rnd.gauss(0, 0.6))
        length = math.sqrt(sum(c * c for c in n))
        cuts.append((tuple(c / length for c in n), rnd.uniform(0.55, 0.8)))
    craters = [((rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(0.2, 0.8)), rnd.uniform(0.18, 0.3)) for _ in range(3)]

    half = int(max(ax) * 1.6) + 2
    n = 2 * half + 1
    nz = 2 * int(ax[2] * 1.6 + 2) + 1
    out = []
    c, cz = half, nz // 2
    for x in range(n):
        for y in range(n):
            for z in range(nz):
                # Into the rock's own frame: turned, then over its axes.
                px, py, pz = x - c, y - c, z - cz
                qx = px * math.cos(turn) + py * math.sin(turn)
                qy = -px * math.sin(turn) + py * math.cos(turn)
                v = (qx / ax[0], qy / ax[1], pz / ax[2])
                inside = max(1 - math.dist(v, at) / size for at, size in lumps)
                bump = (rough(x * 0.35, y * 0.35, z * 0.35) - 0.5) * 0.45
                if inside + bump * 0.6 <= 0.15:
                    continue
                if any(sum(a * b for a, b in zip(v, nrm)) > d for nrm, d in cuts):
                    continue
                dent = min(math.dist(v, at) / r for at, r in craters)
                if dent < 1 and inside < 0.45:
                    continue
                p = patch(x * 0.18, y * 0.18, z * 0.18)
                tone = 0 if p < 0.4 else (1 if p < 0.62 else 2)
                if abs(crack(x * 0.3, y * 0.3, z * 0.3) - 0.5) < 0.035 or dent < 1.3:
                    tone = 3
                out.append((x, y, z, colours[tone]))
    return (n, n, nz), out


ROCKS = [
    [(104, 96, 88), (124, 114, 104), (88, 82, 76), (62, 58, 54)],
    [(116, 100, 84), (142, 122, 100), (96, 82, 70), (66, 56, 48)],
    [(92, 98, 108), (114, 120, 130), (76, 80, 90), (54, 58, 66)],
]
RADII = (4, 6, 8, 10, 13, 5, 7, 16)


# ---------------------------------------------------------------- stations

def launch_pad():
    """Where the run starts: a deck of 44 by 52 at the height the ships
    fly, on a tower: plain plates with seams between them, lights along
    its edges, a ring to land on in its middle."""
    w, d, h = 44, 52, FLY
    plate, seam, mark, light, rim, tower = (112, 118, 130), (82, 88, 100), (230, 190, 40), (255, 220, 120), (70, 74, 84), (80, 86, 98)
    out = []
    for x in range(w):
        for y in range(d):
            edge = x in (0, w - 1) or y in (0, d - 1)
            colour = plate
            if x % 11 == 0 or y % 13 == 0:
                colour = seam
            if abs(x - w // 2) + abs(y - d // 2) in (9, 10):
                colour = mark
            if edge:
                colour = light if (x + y) % 4 == 0 else rim
            for z in range(h - 3, h):
                out.append((x, y, z, colour))
    for x in range(w // 2 - 5, w // 2 + 5):
        for y in range(d // 2 - 5, d // 2 + 5):
            for z in range(0, h - 3):
                if x in (w // 2 - 5, w // 2 + 4) or y in (d // 2 - 5, d // 2 + 4) or z % 6 == 0:
                    out.append((x, y, z, tower))
    return (w, d, h), out


def station(length):
    """A space station seen from above: a spine of modules with docking
    rings, solar panels to the sides on struts, lights; its hull one
    colour, a seam where one module meets the next, rows of windows."""
    w, h = 72, 18
    panel, frame, hull, seam, window, light = (40, 60, 120), (150, 156, 170), (176, 182, 194), (120, 126, 140), (60, 70, 90), (255, 200, 80)
    mid = w // 2
    out = []
    for y in range(length):
        module = (y // 24) % 2 == 0
        r = 7 if module else 4
        for x in range(mid - r, mid + r + 1):
            for z in range(h // 2 - r, h // 2 + r + 1):
                if (x - mid) ** 2 + (z - h // 2) ** 2 <= r * r:
                    colour = seam if y % 24 in (0, 23) else hull
                    if module and z == h // 2 + r - 1 and x in (mid - 4, mid + 4) and y % 4 == 2 and 3 < y % 24 < 20:
                        colour = window
                    out.append((x, y, z, colour))
        if module and y % 24 == 11:
            for x in range(mid - r - 1, mid + r + 2):
                out.append((x, y, h // 2 + r, light))
        if y % 48 in range(30, 46):
            for x in list(range(2, mid - 9)) + list(range(mid + 10, w - 2)):
                edge = y % 48 in (30, 45) or x in (2, mid - 10, mid + 10, w - 3)
                out.append((x, y, h // 2, frame if edge or x % 6 == 0 else panel))
        if y % 48 == 38:
            for x in range(mid - 9, mid + 10):
                out.append((x, y, h // 2, frame))
    return (w, length, h), out


CLEAR = FLY - 8   # the asteroids' tops stay under this: what flies passes over them


def lower_rocks(tops):
    """Every asteroid in map.json at a height under the ships, some deeper
    than others: they drift by below, nothing runs into them."""
    path = HERE / "map.json"
    level = json.loads(path.read_text())
    rnd = random.Random(31)
    for p in level["place"]:
        if p["prefab"] in tops:
            room = CLEAR - tops[p["prefab"]]
            assert room >= 0, f"{p['prefab']} is too tall to fly over"
            p["at"][2] = rnd.randint(max(0, room - 14), room)
    path.write_text(json.dumps(level, indent=1) + "\n")


def main():
    prefabs = HERE / "prefabs"
    tops = {}
    for k, radius in enumerate(RADII):
        size, voxels = rock(radius, k + 1, ROCKS[k % 3])
        write_vox(prefabs / f"rock_{k}.vox", size, voxels)
        tops[f"rock_{k}"] = max(v[2] for v in voxels) + 1
    lower_rocks(tops)
    write_vox(prefabs / "launch_pad.vox", *launch_pad())
    write_vox(prefabs / "station.vox", *station(240))
    print(f"wrote {prefabs}")


if __name__ == "__main__":
    main()

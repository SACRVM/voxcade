#!/usr/bin/env python3
"""Generates More Blood's first stretch of desert: games/moreblood/.

A canyon running north, four outposts (sandbags, a watchtower on stilts, a
hut that sends reinforcements, a caged hostage), and a helipad as the goal,
guarded by a gunship parked on it.

The level is a map the engine puts together when the game starts:

    map.toml             materials, prefabs, and where everything stands
    ground/height.png    grey: the height of every column in voxels, north up
    ground/material.png  which material every column is (colours in map.toml)
    prefabs/*.vvx        palms, cacti, rocks, buildings, several looks each

Paint the ground in any image editor, change prefabs as text (tools/vvx.py
writes them as `voxcade upgrade` does: their colours in the order the map
numbers them), move things in map.toml. Enemies, hostages, pickups,
checkpoints and the goal are markers: placed by name in map.toml, or as
marker entries in a prefab's palette ({ marker = "guard" }; MARKERS below
are the slots the drawing uses for them), like the guard on the
watchtower.

Drawn at 8-bit sprite scale: the figures are 22 voxels tall, like a C64 or
NES sprite, so everything here has twice the voxels of the arena's scale.
Every length in conf.lua is in these voxels too.

Everything that is built stands on the ground through its faces, edges or
corners (the engine holds diagonal neighbours together), so shooting a palm
trunk or a tower's legs brings the rest down.

The figures, the pickups, the dog tags and the helicopters are written as
MagicaVoxel .vox files beside the game's own models (.vvx), which the game
reads: bring one in with `voxcade import games/moreblood games/moreblood/FILE.vox` (its .vvx
replaced).

The game's files are the user's, shaped by hand since: run this into
another folder (the output-dir), never over the game.

usage: python3 tools/gen_moreblood.py [output-dir]
"""

import math
import random
import struct
import sys
import zlib
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from vvx import write_map, write_scene  # noqa: E402

# A calm stretch at the south end, where the player starts out of every
# enemy's sight (More Blood's enemies see 240 voxels far).
CALM = 320
W, D, H = 640, 2048 + CALM, 80
SEED = 23
GROUND = 6  # top layer of the flat floor; dunes rise above it
FLOOR = GROUND + 1  # first free layer on the flat floor, the height of a flat column
VARIANTS = 6  # looks per kind of scenery; each also stands in four turns

OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "games" / "moreblood"
PREFAB_DIR = OUT / "prefabs"
GROUND_DIR = OUT / "ground"

PALETTE = {
    "bedrock": (98, 74, 52),  # the indestructible bottom: dark rock, not a hole
    "sandstone_dark": (120, 88, 58),
    "sandstone": (150, 112, 72),
    "sand0": (214, 184, 128),
    "sand1": (222, 193, 137),
    "sand2": (205, 175, 120),
    "sand3": (230, 203, 148),
    "sand4": (196, 165, 112),
    "rock0": (150, 108, 78),
    "rock1": (132, 94, 66),
    "rock2": (168, 124, 88),
    "rock3": (115, 82, 58),
    "rock_light": (182, 140, 100),
    "stone0": (158, 142, 120),
    "stone1": (142, 126, 106),
    "stone2": (170, 154, 132),
    "stone_top": (184, 168, 146),
    "stone_dark": (112, 96, 80),
    "grass0": (150, 146, 78),
    "grass1": (128, 132, 70),
    "grass2": (176, 162, 96),
    "grass3": (196, 178, 112),
    "grass_green": (110, 128, 64),
    "sandbag0": (176, 160, 118),
    "sandbag1": (164, 148, 106),
    "sandbag2": (186, 170, 128),
    "sandbag_seam": (132, 118, 84),
    "wood0": (126, 88, 52),
    "wood1": (112, 78, 46),
    "wood2": (138, 98, 58),
    "wood_dark": (84, 58, 36),
    "roof0": (128, 124, 116),
    "roof1": (110, 106, 98),
    "rust": (140, 84, 52),
    "palm_trunk0": (138, 104, 64),
    "palm_trunk1": (120, 90, 56),
    "palm_ring": (98, 72, 44),
    "coconut": (92, 64, 36),
    "palm_heart": (84, 92, 44),
    "frond0": (76, 130, 56),
    "frond1": (64, 114, 50),
    "frond_tip": (98, 146, 64),
    "frond_dead": (150, 124, 72),
    "cactus0": (88, 140, 82),
    "cactus1": (70, 118, 68),
    "cactus2": (104, 156, 94),
    "spine": (220, 214, 170),
    "flower": (236, 120, 150),
    "twig0": (128, 104, 70),
    "twig1": (110, 88, 60),
    "bush_leaf": (130, 136, 78),
    "concrete0": (172, 170, 164),
    "concrete1": (156, 154, 148),
    "concrete_seam": (128, 126, 120),
    "paint": (230, 190, 40),
    "lamp": (220, 60, 40),
    "barrel_red": (170, 50, 40),
    "barrel_red_rim": (128, 36, 30),
    "barrel_olive": (90, 96, 60),
    "barrel_olive_rim": (66, 72, 44),
    "barrel_lid": (150, 150, 140),
    "bars": (96, 66, 40),
    "bars_dark": (70, 48, 30),
}
# Reserved slots for markers, in loud colours so they stand out in an editor.
MARKERS = {
    240: ("rifleman", (255, 0, 255)),
    241: ("grenadier", (255, 128, 255)),
    242: ("hut", (128, 0, 255)),
    243: ("hostage", (0, 255, 255)),
    244: ("machine_gun", (255, 255, 0)),
    245: ("rockets", (255, 160, 0)),
    246: ("grenades", (160, 255, 0)),
    247: ("health", (255, 255, 255)),
    248: ("checkpoint", (0, 128, 255)),
    249: ("goal", (255, 0, 0)),
    250: ("gunship", (0, 255, 128)),
    251: ("guard", (255, 64, 128)),
}
INDEX = {name: i + 1 for i, name in enumerate(PALETTE)}
MARK = {name: index for index, (name, _) in MARKERS.items()}


def lattice(ix, iy):
    h = (np.asarray(ix, np.int64) * 374761393 + np.asarray(iy, np.int64) * 668265263 + SEED * 982451653) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return (h ^ (h >> 16)) / 0xFFFFFFFF


def noise(x, y, scale):
    """Smooth value noise in 0..1; works on numbers and numpy arrays."""
    fx, fy = np.asarray(x, np.float64) / scale, np.asarray(y, np.float64) / scale
    ix, iy = np.floor(fx), np.floor(fy)
    tx, ty = fx - ix, fy - iy
    tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
    ix, iy = ix.astype(np.int64), iy.astype(np.int64)
    a = lattice(ix, iy) + (lattice(ix + 1, iy) - lattice(ix, iy)) * tx
    b = lattice(ix, iy + 1) + (lattice(ix + 1, iy + 1) - lattice(ix, iy + 1)) * tx
    return a + (b - a) * ty


def lattice1(ix, iy):
    h = (ix * 374761393 + iy * 668265263 + SEED * 982451653) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return (h ^ (h >> 16)) / 0xFFFFFFFF


def noise1(x, y, scale):
    """noise() for single numbers, without numpy's per-call cost."""
    fx, fy = x / scale, y / scale
    ix, iy = math.floor(fx), math.floor(fy)
    tx, ty = fx - ix, fy - iy
    tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
    a = lattice1(ix, iy) + (lattice1(ix + 1, iy) - lattice1(ix, iy)) * tx
    b = lattice1(ix, iy + 1) + (lattice1(ix + 1, iy + 1) - lattice1(ix, iy + 1)) * tx
    return a + (b - a) * ty


def shade(prefix, count, value):
    return f"{prefix}{min(count - 1, max(0, int(value * count)))}"


# --- writing .vox ----------------------------------------------------------------


def chunk(tag, content=b"", children=b""):
    return tag + struct.pack("<ii", len(content), len(children)) + content + children


def vstr(s):
    b = s.encode()
    return struct.pack("<i", len(b)) + b


def vdict(d):
    return struct.pack("<i", len(d)) + b"".join(vstr(k) + vstr(v) for k, v in d.items())


def rgba(colors):
    out = bytearray()
    for i in range(256):
        r, g, b = colors.get(i + 1, (0, 0, 0)) if isinstance(colors, dict) else (colors[i] if i < len(colors) else (0, 0, 0))
        out += bytes((r, g, b, 255))
    return chunk(b"RGBA", bytes(out))


def write_vox(path, models, colors, glass=None, metal=None):
    """models: list of ((sx, sy, sz), voxel bytes x,y,z,i..., (tx, ty, tz) centre).
    glass: {colour index: transparency 0..1}, written as MagicaVoxel's glass
    material, which the engine draws see-through. metal: {colour index:
    (metalness, roughness)}, MagicaVoxel's metal material: the engine makes
    it mirror the sky and catch the sun."""
    body = b""
    for size, data, _ in models:
        body += chunk(b"SIZE", struct.pack("<iii", *size))
        body += chunk(b"XYZI", struct.pack("<i", len(data) // 4) + bytes(data))
    if len(models) > 1:
        ids = [2 + 2 * k for k in range(len(models))]
        body += chunk(b"nTRN", struct.pack("<i", 0) + vdict({}) + struct.pack("<iiii", 1, -1, -1, 1) + vdict({}))
        body += chunk(b"nGRP", struct.pack("<i", 1) + vdict({}) + struct.pack("<i", len(ids)) + b"".join(struct.pack("<i", i) for i in ids))
        for k, (_, _, t) in enumerate(models):
            node = ids[k]
            frame = vdict({"_t": f"{t[0]} {t[1]} {t[2]}"})
            body += chunk(b"nTRN", struct.pack("<i", node) + vdict({}) + struct.pack("<iiii", node + 1, -1, 0, 1) + frame)
            body += chunk(b"nSHP", struct.pack("<i", node + 1) + vdict({}) + struct.pack("<ii", 1, k) + vdict({}))
        body += chunk(b"LAYR", struct.pack("<i", 0) + vdict({}) + struct.pack("<i", -1))
    body += rgba(colors)
    for index, trans in (glass or {}).items():
        body += chunk(b"MATL", struct.pack("<i", index) + vdict({"_type": "_glass", "_trans": f"{trans:g}"}))
    for index, (metalness, rough) in (metal or {}).items():
        body += chunk(b"MATL", struct.pack("<i", index) + vdict({"_type": "_metal", "_metal": f"{metalness:g}", "_rough": f"{rough:g}"}))
    data = b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", body)
    path.write_bytes(data)
    return len(data)




def hex_colour(name):
    r, g, b = PALETTE[name]
    return f"#{r:02x}{g:02x}{b:02x}"


WORLD_COLOURS = {i + 1: rgb for i, rgb in enumerate(PALETTE.values())}
WORLD_COLOURS.update({index: rgb for index, (_, rgb) in MARKERS.items()})

rng = random.Random(SEED)


def write_png(path, pixels):
    """pixels: rows of grey bytes (rows x cols) or RGB (rows x cols x 3), top row first."""
    rows, cols = pixels.shape[:2]
    colour = 0 if pixels.ndim == 2 else 2
    raw = b"".join(b"\0" + pixels[r].astype(np.uint8).tobytes() for r in range(rows))

    def part(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

    header = struct.pack(">IIBBBBB", cols, rows, 8, colour, 0, 0, 0)
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + part(b"IHDR", header) + part(b"IDAT", zlib.compress(raw, 9)) + part(b"IEND", b""))


# --- prefabs and placements ----------------------------------------------------


class Prefab:
    """A voxel object drawn around its anchor: (0, 0) is the column it is
    placed on and z 0 the first layer above the ground; below 0 it sinks in."""

    def __init__(self):
        self.cells = {}

    def put(self, x, y, z, name):
        self.cells[(x, y, z)] = INDEX[name]

    def mark(self, x, y, z, name):
        self.cells[(x, y, z)] = MARK[name]

    def box(self, x0, y0, z0, x1, y1, z1, pick):
        """Fills [x0,x1) x [y0,y1) x [z0,z1); pick is a name or f(x, y, z) -> name."""
        for z in range(z0, z1):
            for y in range(y0, y1):
                for x in range(x0, x1):
                    self.put(x, y, z, pick(x, y, z) if callable(pick) else pick)


PREFABS = {}  # name -> (file relative to the map, how deep it sinks, soft)
PLACE = []


def save(name, p, soft=False):
    """Writes a prefab so that the engine's anchor, the middle cell of the
    bottom layer, is the drawing's (0, 0). Soft prefabs (grass, shrubs) are
    walked and shot through."""
    xs, ys, zs = zip(*p.cells)
    hx, hy = max(-min(xs), max(xs)), max(-min(ys), max(ys))
    low = min(zs)
    size = (2 * hx + 1, 2 * hy + 1, max(zs) - low + 1)
    voxels = [(x + hx, y + hy, z - low, i) for (x, y, z), i in p.cells.items()]
    write_scene(PREFAB_DIR / f"{name}.vvx", size, voxels, WORLD_COLOURS, {index: kind for index, (kind, _) in MARKERS.items()})
    PREFABS[name] = (f"prefabs/{name}.vvx", low, soft)


def place(name, x, y, quarters=0):
    """Stands a prefab on the ground at (x, y), sunk in as far as it reaches
    below z 0, turned `quarters` quarter turns anticlockwise (its rotation
    in degrees clockwise)."""
    sink = PREFABS[name][1]
    entry = {"prefab": name, "at": [x, y] if sink == 0 else [x, y, int(heights[y, x]) + sink]}
    if quarters % 4:
        entry["rotation"] = -90 * quarters % 360
    PLACE.append(entry)


def marker(name, x, y):
    PLACE.append({"thing": name, "at": [x, y]})


# --- ground: sand floor with low dunes, canyon walls on both sides ---------

def left_edge(y):
    return 68 + (28 * noise(7, y, 140)).astype(np.int64)


def right_edge(y):
    return W - 68 - (28 * noise(91, y, 140)).astype(np.int64)


OUTPOSTS = [CALM + y for y in (400, 860, 1320, 1720)]
HELIPAD_Y = CALM + 1960
SPAWN = (320, 180)
# Where the outposts sit across the canyon, chosen first so the dunes can
# die down towards the yards.
CENTRES = [W // 2 + rng.randint(-60, 60) for _ in OUTPOSTS]
YARDS = [(cx - 140, oy - 80, cx + 140, oy + 90) for cx, oy in zip(CENTRES, OUTPOSTS)]
YARDS.append((W // 2 - 80, HELIPAD_Y - 80, W // 2 + 80, D - 8))
# The title screen's clearing at the start: flat sand where the giant dog
# tags lie (conf.lua, c.title_screen), nothing growing on it.
TITLE_CLEARING = (W // 2 - 85, 85, W // 2 + 85, 205)
YARDS.append(TITLE_CLEARING)


def in_clearing(x, y, margin=12):
    x0, y0, x1, y1 = TITLE_CLEARING
    return x0 - margin < x < x1 + margin and y0 - margin < y < y1 + margin

ys, xs = np.mgrid[0:D, 0:W]
# Dunes flatten out over the last 32 voxels before a yard.
calm = np.ones((D, W))
for x0, y0, x1, y1 in YARDS:
    dx = np.maximum(np.maximum(x0 - xs, xs - (x1 - 1)), 0)
    dy = np.maximum(np.maximum(y0 - ys, ys - (y1 - 1)), 0)
    calm = np.minimum(calm, np.hypot(dx, dy) / 32)
dune = (5.2 * noise(xs, ys, 92) ** 2 * calm).astype(np.int64)

inside = np.minimum(xs - left_edge(ys), right_edge(ys) - xs)
cliff = inside < 0
# Mesas: terraces 8 high every 6 voxels in, up to an uneven plateau, all in
# steps of 4 so the tops are flat.
plateau = 26 + (40 * noise(xs, ys + 1000, 64)).astype(np.int64)
terrace = 10 + (-inside - 1 + (5 * noise(xs, ys, 18)).astype(np.int64)) // 6 * 8
mesa = np.minimum(plateau, terrace) // 4 * 4

heights = np.where(cliff, FLOOR + mesa, FLOOR + dune)
# 0 sand, 1 rock, 2 rock with sand drifted over the top.
drift = noise(xs, ys + 700, 20) > 0.55
kinds = np.where(cliff, np.where(drift, 2, 1), 0)

SAND = [hex_colour(f"sand{i}") for i in range(5)]
ROCK = [hex_colour(f"rock{i}") for i in range(4)]
MATERIALS = [
    {"key": "#e0c080", "surface": SAND, "depth": 3, "pattern": 14, "grain": 0.45,
     "strata": [hex_colour("sandstone_dark"), hex_colour("sandstone")], "band": 2},
    {"key": "#8c5a3c", "surface": [hex_colour("rock1"), hex_colour("rock0"), hex_colour("rock2"), hex_colour("rock_light")],
     "pattern": 5, "grain": 0.3, "strata": ROCK, "band": 4, "wobble": 3},
    {"key": "#c8a070", "surface": SAND, "pattern": 14, "grain": 0.45, "strata": ROCK, "band": 4, "wobble": 3},
]
KEYS = np.array([[int(m["key"][i:i + 2], 16) for i in (1, 3, 5)] for m in MATERIALS], np.uint8)

# --- scenery -----------------------------------------------------------------


def near_outpost(x, y, margin):
    if abs(y - HELIPAD_Y) < 80 + margin:
        return True
    return any(abs(y - oy) < 110 + margin for oy in OUTPOSTS)


def open_ground(x, y, margin=6):
    return int(left_edge(y)) + 16 + margin < x < int(right_edge(y)) - 16 - margin and y > 80


def palm(p):
    """A curved, ringed trunk under a star of broad fronds arching out and
    down from the crown, a few shorter ones rising in the middle. Each frond
    is one smooth blade: narrow at the base, widest past the middle, pointed
    at the tip, folded slightly along its middle like a real palm leaf."""
    height = rng.randint(32, 40)
    lean = rng.choice([(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1)])
    bend = rng.uniform(5, 8)
    tx = ty = 0
    for z in range(-1, height):
        t = max(0.0, z / height)
        tx = round(lean[0] * bend * t * t)
        ty = round(lean[1] * bend * t * t)
        ring = z % 3 == 2
        wide = z < 4  # a flared foot
        for dx in range(-1, 2) if wide else range(0, 2):
            for dy in range(-1, 2) if wide else range(0, 2):
                if wide and dx and dy:
                    continue
                bark = "palm_ring" if ring else ("palm_trunk0" if (dx + dy + z) % 3 else "palm_trunk1")
                p.put(tx + dx, ty + dy, z, bark)
    tz = height
    # The crown's heart, and coconuts hanging under it.
    p.box(tx, ty, tz - 1, tx + 2, ty + 2, tz + 1, "palm_heart")
    for cx, cy in rng.sample([(-1, 0), (2, 1), (0, 2), (1, -1), (-1, 2), (2, -1)], rng.randint(3, 4)):
        p.box(tx + cx, ty + cy, tz - 3, tx + cx + 2, ty + cy + 2, tz - 1, "coconut")

    def frond(angle, rise, droop, length, width, dead=False):
        """A leaf from the crown: it starts `rise` voxels up per voxel out and
        bends down by `droop`; narrow at the base, widest past the middle,
        pointed at the tip, its edges hanging a little below its middle and
        notched where the leaflets part. Darker at the base, lighter out."""
        fx, fy = math.cos(angle), math.sin(angle)
        s = 0.0
        while s <= length:
            u = s / length
            half = width * math.sin(math.pi * min(1.0, u * 1.1)) ** 0.7
            if int(s * 1.5) % 3 == 0:
                half *= 0.7  # the gaps between leaflets
            x = tx + 0.5 + fx * s
            y = ty + 0.5 + fy * s
            z = tz + 0.5 + rise * s - droop * s * s
            if dead:
                colour = "frond_dead"
            else:
                colour = "frond1" if u < 0.3 else "frond0" if u < 0.75 else "frond_tip"
            o = -half
            while o <= half + 1e-6:
                fz = z - abs(o) * 0.45
                if fz >= 24:  # nothing hangs lower than a head
                    p.put(math.floor(x - fy * o), math.floor(y + fx * o), math.floor(fz), colour)
                o += 0.35
            s += 0.35

    # A star of fronds arching out and down, each a little off the compass
    # point and its own length; two young ones rising in the middle, a spear
    # on top, and an old brown one or two hanging below.
    turn = rng.uniform(0, math.tau)
    for k in range(8):
        a = turn + math.tau * k / 8 + rng.uniform(-0.2, 0.2)
        frond(a, rng.uniform(0.05, 0.15), rng.uniform(1 / 40, 1 / 28), rng.uniform(11, 16), rng.uniform(1.2, 1.6))
    for k in range(2):
        a = turn + math.tau * (k + 0.5) / 2 + rng.uniform(-0.4, 0.4)
        frond(a, 0.9, 1 / 14, rng.uniform(6, 8), 1.0)
    for z in range(tz + 1, tz + 4):
        p.put(tx, ty, z, "frond1")
    for k in range(rng.randint(1, 2)):
        frond(turn + rng.uniform(0, math.tau), -0.6, 1 / 30, rng.uniform(6, 8), 1.0, dead=True)


def cactus(p):
    """A saguaro: a ribbed round trunk with an arm or two."""
    height = rng.randint(14, 22)

    def column(cx, cy, z_from, z_to, r):
        for z in range(z_from, z_to):
            for dx in range(-r, r):
                for dy in range(-r, r):
                    corner = dx in (-r, r - 1) and dy in (-r, r - 1)
                    if corner and r > 1:
                        continue
                    rib = "cactus0" if (dx + dy) % 2 else "cactus1"
                    p.put(cx + dx, cy + dy, z, "spine" if rng.random() < 0.03 else rib)
        p.box(cx - 1, cy - 1, z_to, cx + 1, cy + 1, z_to + 1, "cactus2")

    column(0, 0, -1, height, 2)
    if rng.random() < 0.25:
        p.put(0, 0, height + 1, "flower")
    sides = [(1, 0), (-1, 0), (0, 1), (0, -1)]
    rng.shuffle(sides)
    for dx, dy in sides[: rng.randint(1, 2)]:
        at = rng.randint(5, height - 6)
        out = rng.randint(4, 5)
        # A 2 x 2 stub from the trunk out to the elbow, then up.
        for i in range(2, out + 1):
            along = i if dx + dy > 0 else -1 - i
            for side in (-1, 0):
                for z in (at, at + 1):
                    if dx:
                        p.put(along, side, z, "cactus1")
                    else:
                        p.put(side, along, z, "cactus1")
        column(dx * out, dy * out, at, at + rng.randint(5, 9), 1)


def boulder(p):
    """A weathered desert rock: faces cut at random angles, but uneven, with
    chipped edges, faint layers, blotches of desert varnish and a crack or
    two; darker where it meets the sand. Sunk two voxels in; now and then a
    smaller one beside it, and pebbles around."""
    seed = rng.randint(0, 100_000)
    shades = ["stone_dark", "stone1", "stone0", "stone2", "stone_top"]

    def stone(cx, cy, r, tall):
        planes = [((0.0, 0.0, 1.0), r * tall)]  # the top
        for _ in range(rng.randint(8, 12)):
            a = rng.uniform(0, math.tau)
            lift = rng.uniform(-0.1, 0.85)
            n = (math.cos(lift) * math.cos(a), math.cos(lift) * math.sin(a), math.sin(lift))
            planes.append((n, r * rng.uniform(0.8, 1.0)))
        offsets = [rng.choice([-1, 0, 0, 1]) for _ in planes]
        cracks = [(rng.uniform(0, math.tau), rng.uniform(-r * 0.4, r * 0.4)) for _ in range(rng.randint(0, 2))]
        reach = int(r) + 3
        for z in range(-2, int(r * tall) + 3):
            for y in range(-reach, reach + 1):
                for x in range(-reach, reach + 1):
                    point = (x + 0.5, y + 0.5, z + 0.5 + 2)
                    # Gentle bumps make the faces a little uneven.
                    bump = 0.4 * (noise1(x * 2 + seed, y * 2 + z * 3, 3.5) - 0.5)
                    gaps = [n[0] * point[0] + n[1] * point[1] + n[2] * point[2] - d + bump for n, d in planes]
                    edge = max(gaps)
                    if edge > 0:
                        continue
                    surface = edge > -1.0
                    # Chips off the edges where two faces meet, never at the foot.
                    second = sorted(gaps)[-2]
                    on_edge = surface and edge - second < 0.7
                    if on_edge and z > 0 and noise1(x * 5 + seed, y * 5 + z * 7, 2.0) > 0.62:
                        continue
                    if z < 0:
                        p.put(cx + x, cy + y, z, "stone_dark")
                        continue
                    face = gaps.index(edge)
                    shade = 2 + offsets[face]
                    blotch = noise1(x + seed, y + z * 2, 4.5)
                    shade += (blotch > 0.74) - (blotch < 0.24)
                    if z <= 1:
                        shade -= 1  # grime where it meets the sand
                    if face == 0:
                        shade += 1
                    for angle, at in cracks:
                        if abs(math.cos(angle) * x + math.sin(angle) * y - at) < 0.5 and surface:
                            shade = 0
                    p.put(cx + x, cy + y, z, shades[max(0, min(4, shade))])

    r = rng.uniform(5, 9.5)
    stone(0, 0, r, rng.uniform(0.9, 1.3))
    if rng.random() < 0.4:
        a = rng.uniform(0, math.tau)
        small = r * rng.uniform(0.4, 0.6)
        stone(round(math.cos(a) * (r + small * 0.6)), round(math.sin(a) * (r + small * 0.6)), small, rng.uniform(0.6, 1.0))
    for _ in range(rng.randint(2, 5)):
        a = rng.uniform(0, math.tau)
        d = r + rng.uniform(2, 5)
        px, py = round(math.cos(a) * d), round(math.sin(a) * d)
        p.put(px, py, 0, rng.choice(["stone1", "stone0", "stone_dark"]))
        if rng.random() < 0.5:
            p.put(px + 1, py, 0, "stone1")


def bush(p):
    """A dry shrub: twigs that grow up and out from one root. Soft: you walk
    and shoot through it."""
    p.put(0, 0, 0, "twig1")
    for _ in range(rng.randint(6, 10)):
        px = py = pz = 0
        for step in range(rng.randint(4, 8)):
            if rng.random() < 0.6:
                pz += 1
            if rng.random() < 0.7:
                if rng.random() < 0.5:
                    px += rng.choice((-1, 1))
                else:
                    py += rng.choice((-1, 1))
            p.put(px, py, pz, "twig0" if step % 2 else "twig1")
        p.put(px, py, pz + 1, "bush_leaf")
        p.put(px + rng.choice((-1, 1)), py, pz, "bush_leaf")


def grass(p, dry):
    """A tuft of desert grass: blades a few voxels tall, leaning out from the
    middle. Soft: it hides your feet but stops nothing."""
    colours = ["grass2", "grass3", "grass0"] if dry else ["grass0", "grass1", "grass_green"]
    for _ in range(rng.randint(10, 18)):
        a = rng.uniform(0, math.tau)
        d = rng.uniform(0, 2.5)
        x, y = d * math.cos(a), d * math.sin(a)
        lean = rng.uniform(0.15, 0.45)
        tall = rng.randint(3, 7)
        colour = rng.choice(colours)
        for z in range(tall):
            p.put(math.floor(x + math.cos(a) * lean * z), math.floor(y + math.sin(a) * lean * z), z,
                  "grass3" if z == tall - 1 and not dry else colour)


def barrel(p, red):
    body, rim = ("barrel_red", "barrel_red_rim") if red else ("barrel_olive", "barrel_olive_rim")
    for z in range(9):
        for dy in range(-3, 3):
            for dx in range(-3, 3):
                if (dx + 0.5) ** 2 + (dy + 0.5) ** 2 <= 9.5:
                    p.put(dx, dy, z, rim if z in (1, 4, 7) else body)
    for dy in range(-2, 2):
        for dx in range(-2, 2):
            if (dx + 0.5) ** 2 + (dy + 0.5) ** 2 <= 4.5:
                p.put(dx, dy, 8, "barrel_lid")


# --- outposts ----------------------------------------------------------------


def sandbags(p):
    """A 12-voxel stretch of wall: three courses of bags, two deep, in a
    running bond that carries on into the next stretch."""
    for course in range(3):
        z = course * 2
        shift = 3 if course % 2 else 0
        for row in range(2):
            yy = -2 + row * 2
            for start in range(-6 - shift, 6, 6):
                tone = rng.choice(["sandbag0", "sandbag1", "sandbag2"])
                for x in range(max(start, -6), min(start + 6, 6)):
                    seam = x in (start, start + 5)
                    for dy in range(2):
                        for dz in range(2):
                            if course == 2 and dz == 1 and seam:
                                continue  # rounded ends on top
                            p.put(x, yy + dy, z + dz, "sandbag_seam" if seam else tone)


def tower(p):
    """A watchtower on four braced stilts with a guard on top, a rifleman
    who takes more hits ([things.guard] hp): shoot the legs and it comes down."""
    s, h = 14, 24
    x = y = -s // 2
    for sx in (x, x + s - 2):
        for sy in (y, y + s - 2):
            p.box(sx, sy, 0, sx + 2, sy + 2, h + 8, "wood_dark")
    # Cross braces on every side, only in the top third: below them the
    # legs alone hold it up, so shooting them through brings it down.
    low = h * 2 // 3
    for k in range(s - 2):  # one voxel across at a time, so no brace has gaps
        i = low + round(k * (h - low - 1) / (s - 3))
        for xx, yy in ((x + 1 + k, y), (x + s - 2 - k, y), (x + 1 + k, y + s - 1), (x + s - 2 - k, y + s - 1),
                       (x, y + 1 + k), (x, y + s - 2 - k), (x + s - 1, y + 1 + k), (x + s - 1, y + s - 2 - k)):
            p.put(xx, yy, i, "wood1")
    p.box(x - 1, y - 1, h, x + s + 1, y + s + 1, h + 2, lambda xx, yy, zz: "wood0" if (xx // 2 + yy) % 2 else "wood2")
    # A low parapet, posts at the corners and the middles, and a top rail.
    for xx in range(x - 1, x + s + 1):
        for yy in range(y - 1, y + s + 1):
            if xx in (x - 1, x + s) or yy in (y - 1, y + s):
                p.put(xx, yy, h + 2, "wood1")
                p.put(xx, yy, h + 3, "wood0")
                p.put(xx, yy, h + 8, "wood1")
                if xx in (x - 1, x + s // 2, x + s) and yy in (y - 1, y + s // 2, y + s):
                    p.box(xx, yy, h + 4, xx + 1, yy + 1, h + 8, "wood_dark")
    p.mark(0, 0, h + 2, "guard")


def hut(p, w=32, d=24):
    """Plank walls on corner posts, a door to the south, windows, a
    corrugated roof, and reinforcements waiting inside."""
    wall = 24
    x, y = -w // 2, -d // 2
    for yy in range(y, y + d):
        for xx in range(x, x + w):
            edge = xx in (x, x + 1, x + w - 2, x + w - 1) or yy in (y, y + 1, y + d - 2, y + d - 1)
            if not edge:
                continue
            post = (xx - x in (0, 1, w - 2, w - 1)) and (yy - y in (0, 1, d - 2, d - 1))
            door = yy - y < 2 and abs(xx) <= 4
            for z in range(wall):
                if door and z < 18:
                    continue
                along = xx - x if yy - y in (0, 1, d - 2, d - 1) else yy - y
                window = 11 <= z < 15 and along % 8 in (3, 4, 5, 6) and yy - y >= 2 and not post
                if window:
                    continue
                if post or (door and z == 18):
                    name = "wood_dark"
                else:
                    name = ("wood0", "wood1", "wood2")[(z // 3 + along // 11) % 3]
                p.put(xx, yy, z, name)
    for yy in range(y - 2, y + d + 2):
        for xx in range(x - 2, x + w + 2):
            metal = "rust" if rng.random() < 0.06 else ("roof0" if xx % 4 < 2 else "roof1")
            p.put(xx, yy, wall, metal)
            if xx % 4 < 2:
                p.put(xx, yy, wall + 1, metal)
    p.mark(0, y + 6, 0, "hut")  # inside the door, clear of the wall for a 6-wide body


def cage(p):
    """Bars around a hostage: the hostage is out of reach until they break."""
    s, h = 13, 24
    x = y = -(s // 2)
    for yy in range(y, y + s):
        for xx in range(x, x + s):
            edge = xx in (x, x + s - 1) or yy in (y, y + s - 1)
            if edge and (xx + yy) % 2 == 0:
                p.box(xx, yy, 0, xx + 1, yy + 1, h, "bars")
            if edge:
                p.put(xx, yy, 12, "bars_dark")
            if edge or (xx - x) % 3 == 0 or (yy - y) % 3 == 0:
                p.put(xx, yy, h, "bars_dark" if edge else "bars")
    p.mark(0, 0, 0, "hostage")


def crates(p):
    def crate(x, y, z):
        def plank(xx, yy, zz):
            frame = sum((xx in (x, x + 7), yy in (y, y + 7), zz in (z, z + 7))) >= 2
            return "wood_dark" if frame else ("wood0" if (zz - z) // 2 % 2 else "wood2")
        p.box(x, y, z, x + 8, y + 8, z + 8, plank)

    crate(-8, -5, 0)
    crate(0, -5, 0)
    crate(-4, -3, 8)


def helipad(p):
    """Concrete slabs with a painted H and ring, lamps at the corners, the
    goal, and the gunship that guards it parked in the middle."""
    p.box(-40, -40, 0, 40, 40, 2, lambda x, y, z: "concrete_seam" if x % 16 == 0 or y % 16 == 0 else
          ("concrete0" if (x // 16 + y // 16) % 2 else "concrete1"))
    for y in range(-40, 40):
        for x in range(-40, 40):
            r = math.hypot(x + 0.5, y + 0.5)
            letter = (9 < abs(x + 0.5) < 14 and abs(y + 0.5) < 18) or (abs(x + 0.5) <= 9 and abs(y + 0.5) < 2.5)
            if letter or 29 <= r < 32:
                p.put(x, y, 1, "paint")
    for x, y in ((-40, -40), (39, -40), (-40, 39), (39, 39)):
        p.box(x, y, 2, x + 1, y + 1, 5, "concrete1")
        p.put(x, y, 5, "lamp")
    p.mark(0, 0, 2, "goal")
    p.mark(0, 0, 3, "gunship")


def make(name, draw, *args, soft=False):
    p = Prefab()
    draw(p, *args)
    save(name, p, soft)


PREFAB_DIR.mkdir(parents=True, exist_ok=True)
GROUND_DIR.mkdir(parents=True, exist_ok=True)
for old in PREFAB_DIR.glob("*.vvx"):
    old.unlink()

for kind, draw in (("palm", palm), ("cactus", cactus), ("boulder", boulder)):
    for k in range(VARIANTS):
        make(f"{kind}_{k}", draw)
for k in range(VARIANTS):
    make(f"bush_{k}", bush, soft=True)
    make(f"grass_{k}", grass, k % 2 == 0, soft=True)
for k in range(3):
    make(f"sandbags_{k}", sandbags)
make("barrel_red", barrel, True)
make("barrel_olive", barrel, False)
make("tower", tower)
make("hut", hut)
make("cage", cage)
make("crates", crates)
make("helipad", helipad)

placed = [SPAWN]
for _ in range(2600):
    x, y = rng.randrange(0, W), rng.randrange(80, D - 60)
    if not open_ground(x, y) or near_outpost(x, y, 20):
        continue
    # Groves and open sand to fight on: nothing grows where the noise is low.
    if noise1(x + 3000, y, 150) < 0.45:
        continue
    if any(abs(x - px) < 20 and abs(y - py) < 20 for px, py in placed):
        continue
    placed.append((x, y))
    roll = rng.random()
    kind = "palm" if roll < 0.18 else "cactus" if roll < 0.42 else "boulder" if roll < 0.62 else "bush"
    # The same draws everywhere, so the clearing changes nothing else.
    variant, turn = rng.randrange(VARIANTS), rng.randrange(4)
    if not in_clearing(x, y):
        place(f"{kind}_{variant}", x, y, turn)

for _ in range(420):
    cx, cy = rng.randrange(0, W), rng.randrange(40, D - 40)
    if not open_ground(cx, cy, 0) or near_outpost(cx, cy, -40):
        continue
    for _ in range(rng.randint(3, 8)):
        x, y = cx + rng.randint(-12, 12), cy + rng.randint(-12, 12)
        if open_ground(x, y, 0):
            variant, turn = rng.randrange(VARIANTS), rng.randrange(4)
            if not in_clearing(x, y, 0):
                place(f"grass_{variant}", x, y, turn)

PICKUPS = {0: ["machine_gun"], 1: ["health", "grenades"], 2: ["rockets"], 3: ["health", "grenades"]}

for n, oy in enumerate(OUTPOSTS):
    cx = CENTRES[n]
    # A sandbag line across the approach, with a gap to run through.
    gap_at = cx + rng.randint(-40, 40)
    taken = [(cx - 96, oy - 50, cx + 96, oy - 46)]
    for k in range(16):
        sx = cx - 96 + 6 + 12 * k
        if sx + 6 <= gap_at - 12 or sx - 6 >= gap_at + 12:
            place(f"sandbags_{rng.randrange(3)}", sx, oy - 48)
    place("tower", cx + 71, oy - 5)
    place("hut", cx - 16, oy + 32)
    place("cage", cx - 86, oy - 2)
    place("crates", cx + 28, oy - 31)
    taken += [(cx + 62, oy - 14, cx + 80, oy + 4), (cx - 34, oy + 18, cx + 2, oy + 46),
              (cx - 93, oy - 9, cx - 79, oy + 5), (cx + 19, oy - 37, cx + 37, oy - 26)]
    for _ in range(4):
        # Only on open floor, not on a roof or the crates.
        for _ in range(20):
            bx, by = cx + rng.randint(-110, 110), oy + rng.randint(-30, 70)
            if not any(x0 - 5 < bx < x1 + 5 and y0 - 5 < by < y1 + 5 for x0, y0, x1, y1 in taken):
                place(rng.choice(["barrel_red", "barrel_olive"]), bx, by)
                taken.append((bx - 3, by - 3, bx + 3, by + 3))
                break
    # Riflemen behind the sandbags and around the yard; grenadiers from the second outpost on.
    for i in range(4 + n):
        marker("rifleman", cx + rng.randint(-90, 90), oy - 40 + rng.randint(0, 50))
    for i in range(min(n, 2)):
        marker("grenadier", cx + rng.randint(-60, 60), oy + 10)
    for i, item in enumerate(PICKUPS[n]):
        marker(item, cx - 8 + 16 * i, oy - 60)
    marker("checkpoint", W // 2, oy + 140)

# A few patrols between the outposts.
for oy in [CALM + y for y in (300, 640, 1090, 1520)]:
    for _ in range(3):
        marker("rifleman", rng.randint(int(left_edge(oy)) + 50, int(right_edge(oy)) - 50), oy + rng.randint(-40, 40))

# The helipad, the goal, and two guards.
place("helipad", W // 2, HELIPAD_Y)
for dx in (-50, 50):
    marker("rifleman", W // 2 + dx, HELIPAD_Y - 60)

# --- writing the map -------------------------------------------------------------

# Image rows run north to south.
write_png(GROUND_DIR / "height.png", heights[::-1])
write_png(GROUND_DIR / "material.png", KEYS[kinds][::-1])
ground = {"height": "ground/height.png", "material": "ground/material.png",
          "bedrock": hex_colour("bedrock"), "materials": MATERIALS}
prefab_entries = {n: {"file": file, "soft": soft} for n, (file, _, soft) in PREFABS.items()}
write_map(OUT / "map.toml", (W, D, H), ground, prefab_entries, PLACE)
print(f"wrote {OUT / 'map.toml'}: {W}x{D}x{H}, {len(PREFABS)} prefabs, {len(PLACE)} placements")

# --- models --------------------------------------------------------------------
#
# Actors are 3D sprites: one model per animation frame, facing +y, drawn
# around a centre column so quarter turns keep them in place.

# Per frame: swing of the left leg (the right swings the other way, voxels
# at the foot), and which foot is lifted (0 none, 1 left, 2 right).
WALK = [(0, 0), (3, 0), (0, 2), (-3, 0), (0, 1)]


def legs(cells, c, frame, xs_left, xs_right, y0, y1, top_z, boot_z):
    """Two legs from the hip (top_z) down, tilted by the walk swing."""
    swing, lifted = WALK[frame]
    for side, xs_leg in ((1, xs_left), (2, xs_right)):
        s = swing if side == 1 else -swing
        lift = 2 if lifted == side else 0
        for z in range(0, top_z + 1):
            dy = round(s * (top_z - z) / top_z)
            zz = z + lift if z < top_z - 2 else z
            for x in xs_leg:
                for y in range(y0, y1 + 1):
                    cells[(x, y + dy, zz)] = c["boots"] if z <= boot_z else c["pants"]
            if z <= boot_z:  # the toe
                for x in xs_leg:
                    cells[(x, y1 + 1 + dy, zz)] = c["boots"]


def soldier(c, frame, headgear):
    """13 x 13 x 22, facing +y. Centre column x 6, y 6."""
    cells = {}
    legs(cells, c, frame, (4, 5), (7, 8), 5, 7, 8, 1)
    for x in range(3, 10):
        for y in range(4, 9):
            cells[(x, y, 9)] = c["belt"]
            for z in range(10, 16):
                cells[(x, y, z)] = c["shirt"]
    cells[(6, 8, 9)] = c["buckle"]
    # A rucksack: canvas body with a flap and a buckle, pockets on both
    # sides, a bedroll strapped on top, and straps over the shoulders that
    # show from the front.
    for x in range(4, 9):
        for z in range(10, 16):
            cells[(x, 3, z)] = c["pack"]
            cells[(x, 2, z)] = c["pack"]
        for z in range(13, 16):
            cells[(x, 1, z)] = c["pack_dark"]
    cells[(6, 1, 12)] = c["pack_dark"]
    cells[(6, 0, 13)] = c["buckle"]
    for x in (3, 9):
        for y in (2, 3):
            for z in range(10, 13):
                cells[(x, y, z)] = c["pack_dark"]
    for x in range(3, 10):
        for y, z in ((1, 16), (2, 16), (3, 16), (2, 17), (1, 17), (3, 17)):
            if not (x in (3, 9) and (y, z) in ((1, 17), (3, 17))):
                cells[(x, y, z)] = c["bedroll"]
    for x in (4, 8):
        cells[(x, 2, 18)] = c["strap"]
        for y in range(4, 9):
            cells[(x, y, 15)] = c["strap"]
        for z in range(12, 15):
            cells[(x, 8, z)] = c["strap"]
    # Both hands on the rifle: upper arms down the sides, forearms
    # reaching in to the gun in front of the belly.
    for z in range(12, 16):
        for x in (1, 2, 10, 11):
            for y in range(5, 8):
                cells[(x, y, z)] = c["sleeve"]
    for z in (11, 12):
        for x, y in ((10, 7), (10, 8), (9, 8), (9, 9), (8, 9), (2, 7), (2, 8), (3, 8), (3, 9), (4, 9), (4, 10)):
            cells[(x, y, z)] = c["sleeve"]
        for x, y in ((7, 9), (7, 10), (5, 10), (5, 11)):
            cells[(x, y, z)] = c["skin"]
    # Neck and head.
    for x in range(5, 8):
        for y in range(5, 8):
            cells[(x, y, 16)] = c["skin"]
    for x in range(4, 9):
        for y in range(4, 9):
            for z in range(17, 22):
                cells[(x, y, z)] = c["skin"]
    for x in range(4, 9):
        for y in range(4, 9):
            cells[(x, y, 21)] = c["hair"]
        for z in range(18, 21):
            cells[(x, 4, z)] = c["hair"]
    cells[(5, 8, 19)] = c["eyes"]
    cells[(7, 8, 19)] = c["eyes"]
    cells[(6, 9, 18)] = c["skin"]  # nose
    cells[(6, 8, 17)] = c["mouth"]
    if headgear == "band":
        for x in range(4, 9):
            for y in range(4, 9):
                if x in (4, 8) or y in (4, 8):
                    cells[(x, y, 20)] = c["band"]
        for p in ((6, 3, 20), (6, 2, 19), (7, 1, 18), (5, 2, 19)):
            cells[p] = c["band"]
    else:
        for x in range(4, 9):
            for y in range(4, 9):
                cells[(x, y, 21)] = c["band"]
                if x in (4, 8) or y in (4, 8):
                    cells[(x, y, 20)] = c["band"]
        if headgear == "cap":  # a peak in front
            for x in range(4, 9):
                cells[(x, 9, 20)] = c["band"]
                cells[(x, 10, 20)] = c["band"]
        else:  # a helmet with a rim all round
            for x in range(3, 10):
                for y in range(3, 10):
                    if x in (3, 9) or y in (3, 9):
                        cells[(x, y, 20)] = c["band"]
    # The rifle, held level in the middle; its muzzle is 7.5 voxels ahead
    # and 12.5 up (conf.lua: muzzle).
    cells[(6, 8, 12)] = c["stock"]
    for y in range(9, 13):
        cells[(6, y, 12)] = c["gun"]
    for y in range(8, 11):
        cells[(6, y, 13)] = c["gun"]
    cells[(6, 10, 11)] = c["gun"]
    return cells


def hostage(frame):
    """13 x 9 x 21, a prisoner in rags: frame 0 waits with hands up, the rest run."""
    c = {"skin": 5, "shirt": 1, "pants": 2, "boots": 3, "hair": 4, "eyes": 6, "rag": 7, "mouth": 8}
    cells = {}
    legs(cells, c, frame, (4, 5), (7, 8), 3, 5, 8, 0)
    for x in range(4, 9):
        for y in range(3, 6):
            for z in range(9, 15):
                cells[(x, y, z)] = c["rag"] if (x * 3 + z * 5 + y) % 7 == 0 else c["shirt"]
    for x in range(5, 8):
        for y in range(3, 6):
            cells[(x, y, 15)] = c["skin"]
    for x in range(4, 9):
        for y in range(2, 7):
            for z in range(16, 21):
                cells[(x, y, z)] = c["skin"]
        for y in range(2, 7):
            cells[(x, y, 20)] = c["hair"]
        for z in range(17, 20):
            cells[(x, 2, z)] = c["hair"]
    for x in range(5, 8):
        cells[(x, 6, 16)] = c["hair"]  # beard
    cells[(5, 6, 18)] = c["eyes"]
    cells[(7, 6, 18)] = c["eyes"]
    cells[(6, 6, 17)] = c["mouth"]
    swing = WALK[frame][0]
    for x, side in ((2, 1), (3, 1), (9, -1), (10, -1)):
        for y in (3, 4):
            if frame == 0:
                arm = range(14, 21)
                for z in arm:
                    cells[(x, y, z)] = c["skin"] if z >= 19 or z <= 15 else c["shirt"]
            else:
                for z in range(9, 15):
                    dy = round(-swing * side * (14 - z) / 6)
                    cells[(x, y + dy, z)] = c["skin"] if z <= 10 else c["shirt"]
    return cells


def pickup(kind):
    """11 x 11 x 8: a crate with what it holds on the lid."""
    cells = {}
    crate = {"machine_gun": 1, "rockets": 1, "grenades": 1, "health": 5}[kind]
    for z in range(0, 5):
        for y in range(11):
            for x in range(11):
                frame = sum((x in (0, 10), y in (0, 10), z in (0, 4))) >= 2
                cells[(x, y, z)] = 2 if frame else crate
    if kind == "machine_gun":
        for y in range(1, 10):
            cells[(5, y, 5)] = 3
        for y in range(2, 6):
            cells[(5, y, 6)] = 3
        cells[(4, 4, 5)] = 3
        cells[(6, 4, 5)] = 3
        cells[(5, 6, 6)] = 3
    elif kind == "rockets":
        for y in range(1, 10):
            for x in (2, 3, 7, 8):
                for z in (5, 6):
                    cells[(x, y, z)] = 7 if y == 9 else 4
    elif kind == "grenades":
        for gx in (2, 6):
            for gy in (2, 6):
                for dx in range(3):
                    for dy in range(3):
                        cells[(gx + dx, gy + dy, 5)] = 4
                        if dx == 1 or dy == 1:
                            cells[(gx + dx, gy + dy, 6)] = 4
                cells[(gx + 1, gy + 1, 7)] = 8
    elif kind == "health":
        for i in range(2, 9):
            for j in (4, 5, 6):
                cells[(i, j, 5)] = 6
                cells[(j, i, 5)] = 6
    return cells


def model_bytes(cells):
    data = bytearray()
    for (x, y, z), i in cells.items():
        data += bytes((x, y, z, i))
    return data


SOLDIER_PALETTE = ["skin", "shirt", "sleeve", "pants", "belt", "buckle", "boots", "hair", "eyes", "mouth",
                   "band", "strap", "pack", "pack_dark", "bedroll", "gun", "stock"]


def soldier_file(colors, headgear):
    palette = [colors[name] for name in SOLDIER_PALETTE]
    c = {name: i + 1 for i, name in enumerate(SOLDIER_PALETTE)}
    frames = [soldier(c, k, headgear) for k in range(len(WALK))]
    return [((13, 13, 22), model_bytes(f), (k * 16 + 6, 6, 11)) for k, f in enumerate(frames)], palette




TAN, DARK = (204, 160, 120), (24, 22, 20)
MOUTH = (150, 96, 76)
files = {
    "player.vox": ({"skin": TAN, "shirt": (92, 104, 58), "sleeve": TAN, "pants": (72, 60, 44), "belt": (52, 40, 30),
                    "buckle": (200, 180, 90), "boots": DARK, "hair": (30, 26, 24), "eyes": DARK, "mouth": MOUTH,
                    "band": (190, 40, 36), "strap": (52, 42, 30), "pack": (70, 72, 46), "pack_dark": (52, 54, 34),
                    "bedroll": (128, 106, 74), "gun": (60, 62, 66),
                    "stock": (96, 70, 44)}, "band"),
    "enemy.vox": ({"skin": TAN, "shirt": (188, 164, 116), "sleeve": (188, 164, 116), "pants": (168, 146, 104),
                   "belt": (110, 90, 60), "buckle": (150, 140, 100), "boots": (70, 56, 40), "hair": (48, 36, 28),
                   "eyes": DARK, "mouth": MOUTH, "band": (100, 108, 66), "strap": (96, 78, 52),
                   "pack": (122, 106, 74), "pack_dark": (98, 84, 58), "bedroll": (92, 98, 62),
                   "gun": (60, 62, 66), "stock": (96, 70, 44)}, "cap"),
    "grenadier.vox": ({"skin": TAN, "shirt": (132, 112, 80), "sleeve": (132, 112, 80), "pants": (118, 100, 72),
                       "belt": (90, 96, 60), "buckle": (90, 96, 60), "boots": (60, 48, 36), "hair": (48, 36, 28),
                       "eyes": DARK, "mouth": MOUTH, "band": (150, 40, 36), "strap": (66, 70, 44),
                       "pack": (84, 90, 56), "pack_dark": (64, 68, 42), "bedroll": (140, 120, 86),
                       "gun": (90, 96, 60), "stock": (70, 60, 44)}, "helmet"),
}
for name, (colors, headgear) in files.items():
    models, palette = soldier_file(colors, headgear)
    write_vox(OUT / name, models, palette)
    print(f"wrote {OUT / name}")

hostage_palette = [(222, 220, 210), (70, 90, 130), (50, 40, 32), (60, 44, 32), TAN, DARK, (170, 164, 150), MOUTH]
write_vox(OUT / "hostage.vox", [((13, 9, 21), model_bytes(hostage(k)), (k * 16 + 6, 4, 10)) for k in range(len(WALK))], hostage_palette)
print(f"wrote {OUT / 'hostage.vox'}")

pickup_palette = [(96, 104, 64), (60, 62, 50), (60, 62, 66), (70, 92, 48), (230, 230, 225), (200, 40, 40), (210, 210, 200), (230, 190, 40)]
kinds = ["machine_gun", "rockets", "grenades", "health"]
write_vox(OUT / "pickups.vox", [((11, 11, 8), model_bytes(pickup(k)), (i * 16 + 5, 5, 4)) for i, k in enumerate(kinds)], pickup_palette)
print(f"wrote {OUT / 'pickups.vox'}")


# --- the gunships -------------------------------------------------------------------
# Two models: the Huey that More Blood flies (huey.vox), and a Hind-like
# attack helicopter kept as an alternative (hind.vox; conf.lua says which,
# and where each one's guns and pods are).


HIND_PALETTE = ["olive", "olive_dark", "belly", "glass", "glass_frame", "metal", "blade", "tip", "exhaust",
                "pod", "pod_tip"]
HIND_COLOURS = {
    "olive": (92, 100, 60), "olive_dark": (66, 74, 44), "belly": (126, 128, 94),
    "glass": (62, 92, 100), "glass_frame": (44, 48, 42), "metal": (58, 60, 62),
    "blade": (40, 40, 42), "tip": (206, 172, 62), "exhaust": (28, 26, 24),
    "pod": (74, 80, 52), "pod_tip": (190, 180, 146),
}
HIND_R = 40      # half the model across and along: 81 x 81
HIND_ROTOR = 38  # main rotor radius
HIND_FRAMES = 4  # the main rotor turns an eighth of a blade gap per frame


def hind_profile(dy):
    """The hull's half width, bottom and top at dy (forward is +y), or None."""
    if -20 <= dy <= -17:
        return 5, 9, 18
    if -16 <= dy <= 11:
        return 7, 5, 19
    if 12 <= dy <= 19:
        return 6, 5, 19
    if 20 <= dy <= 28:
        return 5, 6, 15
    if 29 <= dy <= 33:
        return 4 - (dy - 29) // 2, 7, 12 - (dy - 29) // 2
    if dy == 34:
        return 1, 8, 10
    return None


def hind(frame):
    """A gunship facing +y, 81 x 81 x 29, the rotor mast in the middle: a
    hull with a stepped two-seat canopy, stub wings with rocket pods, a chin
    gun, skids, a tail boom with a fin and the tail rotor on its right."""
    c = {name: i + 1 for i, name in enumerate(HIND_PALETTE)}
    cells = {}

    def put(x, y, z, name):
        cells[(x + HIND_R, y + HIND_R, z)] = c[name]

    def camo(x, y, z):
        # Big blotches of dark olive over olive; the belly lighter.
        if z <= 7:
            return "belly"
        return "olive_dark" if noise1(x + 500, y * 0.8 + z * 0.6 + 500, 11) > 0.62 else "olive"

    # Hull, rounded at the top and bottom edges.
    for dy in range(-20, 35):
        prof = hind_profile(dy)
        if prof is None:
            continue
        hw, zb, zt = prof
        for z in range(zb, zt + 1):
            w = hw - (2 if z in (zb, zt) else 1 if z in (zb + 1, zt - 1) else 0)
            for x in range(-max(w, 0), max(w, 0) + 1):
                name = camo(x, dy, z)
                # Two canopies, framed every few voxels.
                if 12 <= dy <= 19 and z >= 15:
                    name = "glass_frame" if z == 15 or dy in (12, 16, 19) or abs(x) == w and z == zt else "glass"
                elif 20 <= dy <= 28 and z >= 11:
                    name = "glass_frame" if z == 11 or dy in (20, 24, 28) or abs(x) == w and z == zt else "glass"
                put(x, dy, z, name)

    # Engine housing on top, intakes in front, exhausts behind.
    for dy in range(-14, 11):
        for z, w in ((20, 5), (21, 4), (22, 3)):
            for x in range(-w, w + 1):
                put(x, dy, z, camo(x, dy, z))
    for x in (-4, -3, 3, 4):
        for z in (20, 21):
            put(x, 11, z, "exhaust")
            put(x, -15, z, "exhaust")
            put(x, -16, z, "exhaust")

    # Tail boom, thinning towards the fin.
    for dy in range(-36, -20):
        t = (-21 - dy) / 15
        hw, zb, zt = round(3 - t), 12 + round(t), 16 - round(t)
        for z in range(zb, zt + 1):
            w = hw - (1 if z in (zb, zt) else 0)
            for x in range(-w, w + 1):
                put(x, dy, z, camo(x, dy, z))
    # Tailplane and fin.
    for dy in (-31, -32, -33):
        for x in range(-8, 9):
            put(x, dy, 14, camo(x, dy, 14))
    for dy in range(-39, -32):
        top = int(18 + (-33 - dy) * 1.4)
        for z in range(14, top + 1):
            put(0, dy, z, camo(0, dy, z))

    # Stub wings drooping outwards, a pylon and a rocket pod under each tip.
    for side in (-1, 1):
        for ax in range(8, 18):
            z = 13 - (ax - 8) // 4
            for dy in range(-2, 5):
                put(side * ax, dy, z, camo(side * ax, dy, z))
                if ax <= 10:
                    put(side * ax, dy, z - 1, camo(side * ax, dy, z - 1))
        px = side * 14
        for dy in (0, 1, 2):
            put(px, dy, 10, "metal")
        for dy in range(-4, 8):
            for ox in range(-2, 3):
                for oz in range(-2, 3):
                    if ox * ox + oz * oz > 5:
                        continue
                    if dy == 7:
                        name = "exhaust" if (ox + oz) % 2 == 0 else "pod_tip"
                    else:
                        name = "pod"
                    put(px + ox, dy, 7 + oz, name)

    # Chin gun: a turret under the nose and its barrel.
    for dy in range(28, 32):
        for x in range(-2, 3):
            for z in (4, 5, 6):
                if abs(x) == 2 and z == 4:
                    continue
                put(x, dy, z, "metal")
    for dy in range(32, 39):
        put(0, dy, 5, "metal")

    # Skids on struts.
    for side in (-1, 1):
        for dy in range(-16, 17):
            put(side * 9, dy, 0, "metal")
        put(side * 9, 17, 1, "metal")
        put(side * 9, 18, 2, "metal")
        for dy in (-10, 9):
            for x, z in ((9, 1), (8, 2), (8, 3), (7, 4), (6, 5)):
                put(side * x, dy, z, "metal")

    # Mast and hub.
    for z in (23, 24):
        put(0, 0, z, "metal")
    for x in (-1, 0, 1):
        for dy in (-1, 0, 1):
            put(x, dy, 25, "metal")

    # Main rotor: four blades, two voxels wide, yellow tips.
    for k in range(4):
        a = math.radians(frame * 90 / HIND_FRAMES + k * 90)
        d, n = (math.sin(a), math.cos(a)), (math.cos(a), -math.sin(a))
        r = 2.0
        while r <= HIND_ROTOR:
            for w in (-0.5, 0.5):
                x = round(d[0] * r + n[0] * w)
                y = round(d[1] * r + n[1] * w)
                put(x, y, 26, "tip" if r > HIND_ROTOR - 3 else "blade")
            r += 0.25

    # Tail rotor on the fin's right: two blades in the fin's plane.
    hub_y, hub_z = -35, 22
    put(1, hub_y, hub_z, "metal")
    for k in range(2):
        a = math.radians(frame * 180 / HIND_FRAMES + k * 180)
        for r in range(1, 6):
            put(2, hub_y + round(math.cos(a) * r), hub_z + round(math.sin(a) * r), "tip" if r == 5 else "blade")

    # Only the shell: voxels with all six neighbours filled are never seen.
    filled = set(cells)
    shell = {p: i for p, i in cells.items()
             if any((p[0] + dx, p[1] + dy, p[2] + dz) not in filled
                    for dx, dy, dz in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)))}
    assert all(0 <= v < 2 * HIND_R + 1 for p in shell for v in p[:2]), "the gunship sticks out of its model"
    return shell


hind_models = []
for k in range(HIND_FRAMES):
    cells = hind(k)
    hind_models.append(((2 * HIND_R + 1, 2 * HIND_R + 1, 1 + max(z for _, _, z in cells)), model_bytes(cells),
                        (k * 90, 0, 14)))
write_vox(OUT / "hind.vox", hind_models, [HIND_COLOURS[name] for name in HIND_PALETTE])
print(f"wrote {OUT / 'hind.vox'} ({len(hind(0))} voxels a frame)")


HUEY_PALETTE = ["olive", "olive_dark", "glass", "frame", "metal", "blade", "tip", "exhaust", "pod", "pod_tip",
                "interior", "uniform", "helmet", "skin", "panel", "seat", "visor", "lamp",
                "vest", "belt", "boots", "brass"]
HUEY_COLOURS = {
    "olive": (86, 92, 58), "olive_dark": (64, 70, 44), "glass": (104, 132, 132), "frame": (44, 48, 40),
    "metal": (58, 60, 62), "blade": (38, 38, 40), "tip": (206, 172, 62), "exhaust": (28, 26, 24),
    "pod": (70, 76, 50), "pod_tip": (190, 180, 146), "interior": (34, 36, 30), "uniform": (96, 104, 66),
    "helmet": (60, 66, 40), "skin": (204, 160, 120), "panel": (46, 48, 50),
    "seat": (92, 80, 60), "visor": (26, 26, 30), "lamp": (226, 170, 64),
    "vest": (74, 78, 54), "belt": (54, 46, 34), "boots": (34, 30, 26), "brass": (196, 160, 70),
}
HUEY_GLASS = 0.6  # how see-through the cockpit glass is
HUEY_R = 42      # half the model across and along: 85 x 85
HUEY_ROTOR = 38
HUEY_FRAMES = 4  # the two-blade rotor turns 45 degrees a frame


def huey_hull(dy):
    """The hull's half width, bottom and top at dy (forward is +y), or None:
    a round nose, a tall boxy cabin, a step down to a thin tail boom."""
    if 18 < dy <= 29:
        # Blunt: the belly stays low, the top falls away to the nose.
        t = (dy - 18) / 11
        return round(8 - 3 * t ** 2), 5 + round(3 * t ** 2), 22 - round(11 * t ** 1.3)
    if -6 <= dy <= 18:
        return 8, 5, 22
    if -12 <= dy < -6:
        t = (-6 - dy) / 6
        return 8 - round(3 * t), 5 + round(9 * t), 22 - round(t)
    if -40 <= dy < -12:
        t = (-12 - dy) / 28
        return round(4 - 2.5 * t), 14 + round(4 * t), 21 - round(t)
    return None


def door_gunner(put, side):
    """A door gunner in the cargo door on `side` (1 right, -1 left), sitting
    on a troop seat facing out, as big as the soldiers below: flak vest,
    helmet, dark glasses, boots on the door sill, both hands on his M60,
    whose mount stands in the doorway and whose muzzle is at (16, 4, 15)
    (the game's [things.gunship] gun)."""
    def at(x, dy, z, name):
        put(side * x, dy, z, name)

    for dy in range(1, 8):
        for x in (2, 3, 4):
            at(x, dy, 10, "seat")
        for z in range(10, 18):
            at(1, dy, z, "seat")
    for x in (2, 4):
        for dy in (1, 7):
            for z in (7, 8, 9):
                at(x, dy, z, "metal")
    for dy in range(2, 7):
        for x in (3, 4, 5):
            for z in (11, 12):
                at(x, dy, z, "uniform")  # hips on the seat
    for dy in (2, 3, 5, 6):
        for x in (5, 6, 7):
            for z in (11, 12):
                at(x, dy, z, "uniform")  # thighs out towards the door
        for x in (7, 8):
            for z in (8, 9, 10):
                at(x, dy, z, "uniform")  # shins
        at(8, dy, 7, "boots")
        at(9, dy, 7, "boots")
    for dy in range(1, 8):
        for x in (3, 4, 5):
            for z in range(13, 18):
                at(x, dy, z, "belt" if z == 13 else "vest")
    for dy in (0, 8):
        for x in (4, 5):
            for z in range(14, 18):
                at(x, dy, z, "uniform")  # upper arms
    # Forearms in their sleeves, from the elbows in to the grips: only the
    # two hands show.
    for dy, inward in ((1, 1), (7, -1)):
        at(6, dy, 14, "uniform")
        at(7, dy, 14, "uniform")
        at(7, dy + inward, 15, "uniform")
        at(8, dy + inward, 15, "uniform")
        at(9, dy + 2 * inward, 15, "skin")
    for x in range(3, 7):
        for dy in range(2, 7):
            for z in range(18, 22):
                face = x == 6 and 3 <= dy <= 5 and z <= 19
                at(x, dy, z, ("skin" if z == 18 else "visor") if face else "helmet")
    # The M60: pintle mount from the sill, receiver, barrel, ammo can, belt.
    for z in range(7, 15):
        at(9, 4, z, "metal")
    for x in range(7, 17):
        at(x, 4, 15, "metal")
    for z in (13, 14):
        for dy in (5, 6):
            at(10, dy, z, "helmet")
    at(10, 5, 15, "brass")


def huey(frame):
    """A Huey gunship facing +y, 85 x 85 x 31, the mast in the middle: round
    nose all glass, the cabin open through both cargo doors with a door
    gunner and his M60 in each, rocket pods beside them, skids on arched
    cross tubes, the roof hump, a long tail boom with the fin and the tail
    rotor on its left, and the two-blade rotor with its stabilizer bar."""
    c = {name: i + 1 for i, name in enumerate(HUEY_PALETTE)}
    cells = {}

    def put(x, y, z, name):
        cells[(x + HUEY_R, y + HUEY_R, z)] = c[name]

    # Hull, its edges rounded a voxel.
    for dy in range(-40, 30):
        hull = huey_hull(dy)
        if hull is None:
            continue
        hw, zb, zt = hull
        for z in range(zb, zt + 1):
            w = hw - (1 if z in (zb, zt) and hw > 2 else 0)
            for x in range(-w, w + 1):
                name = "olive_dark" if z <= zb + 1 else "olive"
                if dy >= 19 and z >= 13:
                    # The windscreen, a post down the middle.
                    name = "frame" if x == 0 or z == 13 or dy == 19 else "glass"
                elif dy >= 23 and z == zb and 0 < abs(x) < w - 1:
                    name = "glass"  # the chin windows, under the pilots' feet
                elif 15 <= dy <= 18 and z == zt and abs(x) <= 5:
                    name = "glass"  # the windows over the pilots
                elif 12 <= dy <= 18 and 13 <= z <= 20 and abs(x) == w:
                    name = "frame" if dy in (12, 18) or z == 13 else "glass"  # pilots' doors
                elif -5 <= dy <= -2 and 15 <= z <= 19 and abs(x) == w:
                    name = "glass"
                put(x, dy, z, name)

    # Cabin and cockpit are one hollow room behind the glass and the doors:
    # everything inside the hull's skin from the cabin's back wall to the
    # nose goes.
    hull = set(cells)
    for (px, py, pz) in list(cells):
        if -5 <= py - HUEY_R <= 27 and all(
                (px + ax, py + ay, pz + az) in hull
                for ax, ay, az in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))):
            del cells[(px, py, pz)]

    # Floor, instrument panel with its glare shield and a few lamps, the
    # console between the seats.
    for dy in range(-5, 27):
        for x in range(-7, 8):
            if (x + HUEY_R, dy + HUEY_R, 6) in hull and (x + HUEY_R, dy + HUEY_R, 6) not in cells:
                put(x, dy, 6, "interior")
    for x in range(-6, 7):
        for z in range(10, 14):
            put(x, 21, z, "panel")
            put(x, 22, z, "panel")
        put(x, 22, 14, "panel")
    for x in (-5, -2, 2, 5):
        put(x, 21, 14, "lamp")
    for dy in range(15, 21):
        for x in (-1, 0, 1):
            for z in (7, 8, 9, 10):
                put(x, dy, z, "panel")

    # Two pilots side by side: seat, flight suit, gloves on the stick, helmet
    # and dark visor, facing ahead.
    for side in (-1, 1):
        xs = [side * x for x in range(2, 7)]
        for x in xs:
            for dy in range(13, 17):
                put(x, dy, 8, "seat")
            for z in range(8, 17):
                put(x, 12, z, "seat")
            for dy in (13, 14):
                for z in range(9, 15):
                    put(x, dy, z, "uniform")
        for x in (side * 2, side * 3, side * 5, side * 6):
            for dy in (15, 16, 17):
                put(x, dy, 9, "uniform")  # thighs
            for z in (7, 8):
                put(x, 18, z, "uniform")  # shins
            put(x, 19, 7, "helmet")       # boots on the pedals
        for x in (side * 2, side * 6):
            put(x, 15, 12, "uniform")     # forearms
            put(x, 16, 11, "uniform")
        put(side * 3, 17, 10, "skin")     # hands on the cyclic
        put(side * 5, 17, 10, "skin")
        for z in (8, 9, 10):
            put(side * 4, 17, z, "metal")
        for x in [side * x for x in range(3, 6)]:
            for dy in (13, 14, 15):
                for z in (15, 16, 17):
                    put(x, dy, z, "skin" if dy == 15 and z in (15, 16) else "visor" if dy == 15 else "helmet")
            for dy in (13, 14, 15):
                put(x, dy, 18, "helmet")

    # The cargo doors stand open on both sides: one can see through the
    # cabin. A door gunner sits in each, facing out, his M60 on its mount
    # pointing out of the door.
    for side in (-1, 1):
        for dy in range(0, 11):
            for z in range(7, 22):
                cells.pop((side * 8 + HUEY_R, dy + HUEY_R, z), None)
        door_gunner(put, side)

    # Rocket pods on pylons under the doors.
    for side in (-1, 1):
        for dy in range(6, 11):
            for x in (9, 10, 11):
                put(side * x, dy, 6, "metal")
        for dy in range(2, 15):
            for ox in range(-2, 3):
                for oz in range(-2, 3):
                    if ox * ox + oz * oz > 5:
                        continue
                    # Seven tubes' mouths, dark on the pale front.
                    hole = abs(ox) <= 1 and abs(oz) <= 1 and (ox + oz) % 2 == 0
                    name = ("exhaust" if hole else "pod_tip") if dy == 14 else "pod"
                    put(side * 13 + ox, dy, 7 + oz, name)

    # The roof hump: transmission in front, the engine behind, its exhaust.
    for dy in range(-10, 13):
        for z, w in ((23, 5), (24, 4), (25, 3), (26, 2)):
            if z == 26 and dy < -2:
                continue
            for x in range(-w, w + 1):
                put(x, dy, z, "olive")
    for dy in (-11, -12):
        for x in (-1, 0, 1):
            put(x, dy, 24, "exhaust")
            put(x, dy, 25, "exhaust")

    # Tail: the elevators halfway, the fin at the end.
    for dy in (-22, -23, -24):
        for x in range(-7, 8):
            put(x, dy, 17, "olive")
    for dy in range(-40, -33):
        top = int(20 + (-34 - dy) * 1.6)
        for z in range(18, top + 1):
            put(0, dy, z, "olive")

    # Skids on arched cross tubes.
    for side in (-1, 1):
        for dy in range(-14, 16):
            put(side * 9, dy, 0, "metal")
        put(side * 9, 16, 1, "metal")
        put(side * 9, 17, 2, "metal")
        put(side * 9, 18, 3, "metal")
        for dy in (-8, 9):
            for x, z in ((9, 1), (9, 2), (8, 3), (8, 4)):
                put(side * x, dy, z, "metal")
    for dy in (-8, 9):
        for x in range(-7, 8):
            put(x, dy, 4, "metal")

    # Mast, hub, the stabilizer bar across the blades, and the two blades,
    # three voxels wide, yellow tips.
    for z in (27, 28):
        put(0, 0, z, "metal")
    a = math.radians(frame * 180 / HUEY_FRAMES)
    d, n = (math.sin(a), math.cos(a)), (math.cos(a), -math.sin(a))
    r = -8.0
    while r <= 8.0:
        put(round(n[0] * r), round(n[1] * r), 29, "metal")
        r += 0.5
    for end in (-8, 8):
        for w in (-1, 0, 1):
            put(round(n[0] * end + d[0] * w), round(n[1] * end + d[1] * w), 29, "metal")
    for k in range(2):
        b = a + math.pi * k
        d = (math.sin(b), math.cos(b))
        n = (math.cos(b), -math.sin(b))
        r = 1.0
        while r <= HUEY_ROTOR:
            for w in (-1, 0, 1):
                put(round(d[0] * r + n[0] * w), round(d[1] * r + n[1] * w), 30, "tip" if r > HUEY_ROTOR - 3 else "blade")
            r += 0.25

    # Tail rotor on the fin's left: two blades in the fin's plane.
    hub_y, hub_z = -37, 24
    put(-1, hub_y, hub_z, "metal")
    for k in range(2):
        b = math.radians(frame * 180 / HUEY_FRAMES + k * 180)
        for r in range(1, 6):
            put(-2, hub_y + round(math.cos(b) * r), hub_z + round(math.sin(b) * r), "tip" if r == 5 else "blade")

    # Only the shell: voxels with all six neighbours filled are never seen;
    # glass hides nothing.
    filled = {p for p, i in cells.items() if i != c["glass"]}
    shell = {p: i for p, i in cells.items()
             if any((p[0] + dx, p[1] + dy, p[2] + dz) not in filled
                    for dx, dy, dz in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)))}
    assert all(0 <= v < 2 * HUEY_R + 1 for p in shell for v in p[:2]), "the Huey sticks out of its model"
    return shell


huey_models = []
for k in range(HUEY_FRAMES):
    cells = huey(k)
    huey_models.append(((2 * HUEY_R + 1, 2 * HUEY_R + 1, 1 + max(z for _, _, z in cells)), model_bytes(cells),
                        (k * 95, 0, 15)))
write_vox(OUT / "huey.vox", huey_models, [HUEY_COLOURS[name] for name in HUEY_PALETTE],
          glass={HUEY_PALETTE.index("glass") + 1: HUEY_GLASS})
print(f"wrote {OUT / 'huey.vox'} ({len(huey(0))} voxels a frame)")


# --- the dog tags of the title screen ---------------------------------------------

def ui_font():
    """The engine's 5 x 7 pixel font (src/ui.rs), so the tags read like the HUD."""
    import re
    text = (Path(__file__).resolve().parent.parent / "src" / "ui.rs").read_text()
    font = {}
    for ch, rows in re.findall(r"\('(\\'|.)', \[((?:\"[.#]{5}\",? ?){7})\]\)", text):
        font[ch.replace("\\'", "'")] = re.findall(r'"([.#]{5})"', rows)
    return font


TAG_PALETTE = ["steel", "rim", "letters", "chain", "blood", "blood_dark"]
TAG_COLOURS = {"steel": (168, 172, 176), "rim": (198, 202, 206), "letters": (88, 92, 98),
               "chain": (146, 150, 154), "blood": (150, 20, 18), "blood_dark": (104, 12, 12)}
# The hero's tags: Nick Pain (named by the user).
TAG_TEXT = ["NICK PAIN", "US 52 064 85", "O POS  T64", "NO PREF"]
TAG_SIZE = (96, 52)  # long and short side, in voxels: a real tag 25 times over


def dogtags():
    """Two giant dog tags lying on the sand, one half on the other, joined
    by their ball chain, the name embossed on each, some blood on them. Flat
    (the scene's camera looks down at them), the text reading left to right
    with its top away from the camera."""
    c = {name: i + 1 for i, name in enumerate(TAG_PALETTE)}
    font = ui_font()
    cells = {}

    def put(x, y, z, name):
        cells[(x, y, z)] = c[name]

    L, S = TAG_SIZE
    corner = 7

    def inside(u, v):
        """Whether (u, v) is on a tag (its own coordinates, 0..L x 0..S),
        rounded corners, the chain's hole at the left end."""
        if not (0 <= u < L and 0 <= v < S):
            return False
        cu = min(u, L - 1 - u)
        cv = min(v, S - 1 - v)
        if cu < corner and cv < corner and (corner - cu) ** 2 + (corner - cv) ** 2 > corner ** 2:
            return False
        return (u - 7) ** 2 + (v - S // 2) ** 2 > 3.5 ** 2

    def tag(ox, oy, base):
        """A tag with its lower left corner at (ox, oy), lying on `base`
        (filled down to the sand where nothing is under it)."""
        for v in range(S):
            for u in range(L):
                if not inside(u, v):
                    continue
                x, y = ox + u, oy + v
                under = max((z for (cx, cy, z) in [(x, y, zz) for zz in range(base)] if (cx, cy, z) in cells), default=-1)
                for z in range(under + 1, base + 2):
                    put(x, y, z, "steel")
                # The rolled rim, a voxel in from the edge.
                edge = not all(inside(u + du, v + dv) for du, dv in ((-3, 0), (3, 0), (0, -3), (0, 3)))
                if edge and all(inside(u + du, v + dv) for du, dv in ((-1, 0), (1, 0), (0, -1), (0, 1))):
                    put(x, y, base + 2, "rim")
        # The text, embossed: 4 lines from the top, after the hole.
        top = S - 8
        for n, line in enumerate(TAG_TEXT):
            y0 = oy + top - n * 11
            for k, ch in enumerate(line):
                glyph = font.get(ch)
                if glyph is None:
                    continue
                for r, row in enumerate(glyph):
                    for col, px in enumerate(row):
                        if px == "#":
                            put(ox + 15 + k * 6 + col, y0 - r, base + 2, "letters")

    tag(0, 0, 0)
    # The second tag on top of the first's lower right, as dropped.
    tag(66, -22, 2)

    def chain(path):
        """Round beads every 4 voxels with little links between, lying on
        whatever is under them."""
        travelled, last = 0.0, path[0]
        for x, y in path:
            travelled += math.dist((x, y), last)
            last = (x, y)
            xi, yi = round(x), round(y)
            z0 = 1 + max((z for z in range(6) if (xi, yi, z) in cells), default=-1)
            if travelled % 4 < 0.6:
                for dx in (-1, 0, 1):
                    for dy in (-1, 0, 1):
                        for dz in (0, 1, 2):
                            if abs(dx) + abs(dy) + abs(dz - 1) <= 2:
                                put(xi + dx, yi + dy, z0 + dz, "chain")
            else:
                put(xi, yi, z0 + 1, "chain")

    # Ball chain: round beads every 4 voxels, little links between, along a
    # loop from the first tag's hole round to the second's and off.
    def spline(points, step=0.25):
        out = []
        for i in range(len(points) - 1):
            p0 = points[max(i - 1, 0)]
            p1, p2 = points[i], points[i + 1]
            p3 = points[min(i + 2, len(points) - 1)]
            t = 0.0
            while t < 1.0:
                t2, t3 = t * t, t * t * t
                out.append(tuple(0.5 * ((2 * p1[j]) + (-p0[j] + p2[j]) * t + (2 * p0[j] - 5 * p1[j] + 4 * p2[j] - p3[j]) * t2
                                       + (-p0[j] + 3 * p1[j] - 3 * p2[j] + p3[j]) * t3) for j in range(2)))
                t += step / max(1.0, math.dist(p1, p2))
        out.append(points[-1])
        return out

    # The long chain through the first tag's hole, curling off to the left
    # and under; the short one through the second's.
    long_chain = spline([(7, S // 2), (-4, 36), (-18, 30), (-24, 10), (-18, -10), (-2, -20), (20, -24), (44, -26)], 0.5)
    short_chain = spline([(73, 4), (62, -10), (66, -28), (84, -34), (104, -30)], 0.5)
    for path in (long_chain, short_chain):
        chain(path)
    # Blood: a smear across the first tag's corner, drops on both and on the sand.
    for i in range(16):
        x, y = 70 + i, 8 + i // 3
        top_z = max(z for z in range(8) if (x, y, z) in cells or z == 0)
        put(x, y, top_z + 1 if (x, y, top_z) in cells else 0, "blood_dark" if i % 4 == 0 else "blood")
    for x, y, r in ((40, 6, 2), (88, -14, 2), (120, 14, 1), (-12, 18, 2), (24, -16, 1), (140, -26, 2), (100, 30, 1)):
        for dx in range(-r, r + 1):
            for dy in range(-r, r + 1):
                if dx * dx + dy * dy <= r * r + 1:
                    zs = [z for z in range(8) if (x + dx, y + dy, z) in cells]
                    put(x + dx, y + dy, max(zs) + 1 if zs else 0, "blood" if (dx + dy) % 3 else "blood_dark")

    # Into the model's own coordinates: from 0 up, centred on the tags.
    xs = [p[0] for p in cells]
    ys = [p[1] for p in cells]
    x0, y0 = min(xs), min(ys)
    size = (max(xs) - x0 + 1, max(ys) - y0 + 1, max(p[2] for p in cells) + 1)
    data = bytearray()
    for (x, y, z), i in cells.items():
        data += bytes((x - x0, y - y0, z, i))
    return size, data


size, data = dogtags()
# Stainless steel: the plate and the chain, the rolled rim shinier, the
# embossed letters duller.
TAG_METAL = {"steel": (1.0, 0.3), "rim": (1.0, 0.18), "letters": (0.8, 0.55), "chain": (1.0, 0.3)}
write_vox(OUT / "dogtags.vox", [(size, data, (0, 0, 0))], [TAG_COLOURS[name] for name in TAG_PALETTE],
          metal={TAG_PALETTE.index(n) + 1: m for n, m in TAG_METAL.items()})
print(f"wrote {OUT / 'dogtags.vox'} ({size[0]} x {size[1]} x {size[2]})")


# --- the display font: big letters (logo, headings, messages) ------------------------

DISPLAY_FONT = Path("/usr/share/fonts/TTF/FiraSansCompressed-Heavy.ttf")  # SIL Open Font License
DISPLAY_CHARS = "".join(chr(c) for c in range(32, 91))  # space to Z
DISPLAY_CAP = 96  # capital height in the atlas, in pixels


def display_font_atlas(path):
    """Renders the display font's glyphs into a grey PNG the engine samples
    down to whatever size it draws them: a row of cells, each glyph standing
    on the cell's baseline (a capital fills the cell from the top to it)."""
    from PIL import Image, ImageDraw, ImageFont
    size = DISPLAY_CAP
    for _ in range(60):
        font = ImageFont.truetype(str(DISPLAY_FONT), size)
        top, bottom = font.getbbox("H")[1], font.getbbox("H")[3]
        if bottom - top == DISPLAY_CAP:
            break
        size += 1 if bottom - top < DISPLAY_CAP else -1
    cell_w = max(font.getbbox(c)[2] for c in DISPLAY_CHARS if c.strip()) + 4
    # Room below the baseline for commas and the like.
    cell_h = DISPLAY_CAP + DISPLAY_CAP // 4
    atlas = Image.new("L", (cell_w * len(DISPLAY_CHARS), cell_h), 0)
    draw = ImageDraw.Draw(atlas)
    for i, ch in enumerate(DISPLAY_CHARS):
        draw.text((i * cell_w + 2, -top), ch, font=font, fill=255)
    path.parent.mkdir(parents=True, exist_ok=True)
    atlas.save(path)
    return cell_w, cell_h


if DISPLAY_FONT.exists():
    w, h = display_font_atlas(OUT / "ui" / "display_font.png")
    print(f"wrote {OUT / 'ui' / 'display_font.png'} ({len(DISPLAY_CHARS)} glyphs, {w} x {h} each)")
else:
    print(f"no {DISPLAY_FONT}: the display font atlas stays as it is")

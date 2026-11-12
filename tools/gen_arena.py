#!/usr/bin/env python3
"""Generates the demo arena: games/arena/world.vvx and games/arena/player.vox.

The level is written as Voxcade's own text (a world's .vvx, src/scene.rs,
through tools/vvx.py): its colours numbered in the palette's order. Re-running
overwrites both files: the game's are the user's, so run it into another
folder (the output-dir). The player is written as player.vox, which the game
does not read: bring it in with `voxcade import games/arena games/arena/player.vox`.

usage: python3 tools/gen_arena.py [output-dir]
"""

import math
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from vvx import write_scene  # noqa: E402

W, D, H = 512, 512, 48
TILE = 256
SEED = 7
GROUND = 4  # first free layer above the ground

PALETTE = {
    "bedrock": (29, 27, 36),
    "dirt_dark": (74, 52, 38),
    "dirt": (94, 66, 48),
    "grass0": (62, 122, 58),
    "grass1": (70, 138, 63),
    "grass2": (79, 148, 69),
    "grass3": (90, 158, 75),
    "grass4": (100, 168, 82),
    "sand0": (200, 169, 110),
    "sand1": (184, 154, 96),
    "sand2": (212, 182, 124),
    "stone0": (125, 127, 134),
    "stone1": (140, 142, 149),
    "stone2": (109, 111, 118),
    "stone3": (154, 156, 163),
    "stone_dark0": (85, 86, 93),
    "stone_dark1": (71, 72, 78),
    "brick0": (161, 74, 59),
    "brick1": (180, 90, 70),
    "brick2": (142, 63, 51),
    "wood0": (138, 90, 50),
    "wood1": (160, 106, 60),
    "wood_dark0": (107, 69, 39),
    "wood_dark1": (122, 82, 48),
    "leaf0": (47, 107, 46),
    "leaf1": (59, 127, 52),
    "leaf2": (74, 145, 64),
    "trunk": (91, 59, 34),
    "tile0": (185, 178, 165),
    "tile1": (166, 159, 146),
    "tile2": (207, 200, 187),
    "tile3": (148, 141, 129),
    "gold": (226, 179, 60),
    "flower_pink": (228, 106, 154),
    "flower_yellow": (242, 210, 75),
    "flower_white": (240, 240, 240),
}
INDEX = {name: i + 1 for i, name in enumerate(PALETTE)}

rng = random.Random(SEED)
vox = bytearray(W * D * H)


def put(x, y, z, name):
    if 0 <= x < W and 0 <= y < D and 0 <= z < H:
        vox[x + y * W + z * W * D] = INDEX[name]


def get(x, y, z):
    if 0 <= x < W and 0 <= y < D and 0 <= z < H:
        return vox[x + y * W + z * W * D]
    return 0


def box(x0, y0, z0, x1, y1, z1, pick):
    """Fills [x0,x1) x [y0,y1) x [z0,z1); pick is a name or f(x, y, z) -> name."""
    for z in range(z0, z1):
        for y in range(y0, y1):
            for x in range(x0, x1):
                put(x, y, z, pick(x, y, z) if callable(pick) else pick)


def lattice(ix, iy):
    h = (ix * 374761393 + iy * 668265263 + SEED * 982451653) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return (h ^ (h >> 16)) / 0xFFFFFFFF


def noise(x, y, scale):
    fx, fy = x / scale, y / scale
    ix, iy = math.floor(fx), math.floor(fy)
    tx, ty = fx - ix, fy - iy
    tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
    a = lattice(ix, iy) + (lattice(ix + 1, iy) - lattice(ix, iy)) * tx
    b = lattice(ix, iy + 1) + (lattice(ix + 1, iy + 1) - lattice(ix, iy + 1)) * tx
    return a + (b - a) * ty


def shade(prefix, count, value):
    return f"{prefix}{min(count - 1, max(0, int(value * count)))}"


# --- ground -------------------------------------------------------------

CX, CY = W // 2, D // 2
PATH = 6


def on_path(x, y):
    return abs(x - CX) < PATH or abs(y - CY) < PATH


for y in range(D):
    for x in range(W):
        put(x, y, 0, "bedrock")
        put(x, y, 1, "dirt_dark")
        put(x, y, 2, "dirt")
        n = noise(x, y, 40) * 0.7 + noise(x, y, 7) * 0.2 + rng.random() * 0.1
        if on_path(x, y):
            put(x, y, 3, shade("sand", 3, n))
        else:
            put(x, y, 3, shade("grass", 5, n))

# --- perimeter wall with crenellations -----------------------------------

WALL, WALL_H = 4, 16
for y in range(D):
    for x in range(W):
        edge_x, edge_y = x < WALL or x >= W - WALL, y < WALL or y >= D - WALL
        if not (edge_x or edge_y):
            continue
        along = x if edge_y else y
        top = GROUND + WALL_H + (3 if (along // 5) % 2 == 0 else 0)
        for z in range(GROUND, top):
            put(x, y, z, shade("stone", 4, rng.random()) if z < top - 1 else "stone_dark0")

# --- central plaza, stepped, with an obelisk -----------------------------

PLAZA = 48
for k in range(3):
    r = PLAZA - k * 4
    box(CX - r, CY - r, GROUND + k, CX + r, CY + r, GROUND + k + 1,
        lambda x, y, z: "tile2" if rng.random() < 0.05 else ("tile0" if (x // 8 + y // 8) % 2 == 0 else "tile1"))

top = GROUND + 3
box(CX - 3, CY - 3, top, CX + 3, CY + 3, top + 26,
    lambda x, y, z: "stone_dark1" if (z - top) % 6 == 5 else "stone_dark0")
box(CX - 4, CY - 4, top + 26, CX + 4, CY + 4, top + 28, "gold")
box(CX - 1, CY - 1, top + 28, CX + 1, CY + 1, top + 31, "gold")
for sx in (-1, 1):
    for sy in (-1, 1):
        px, py = CX + sx * 30, CY + sy * 30
        box(px - 2, py - 2, top, px + 2, py + 2, top + 12, lambda x, y, z: shade("stone", 4, rng.random()))
        box(px - 3, py - 3, top + 12, px + 3, py + 3, top + 13, "gold")

# --- pillar rows along the paths ------------------------------------------

for t in range(WALL + 20, W - WALL - 20, 28):
    if abs(t - CX) < PLAZA + 8:
        continue
    for side in (-10, 10):
        for px, py in ((t, CY + side), (CX + side, t)):
            height = rng.randint(6, 14)
            box(px - 1, py - 1, GROUND, px + 2, py + 2, GROUND + height,
                lambda x, y, z: shade("stone", 4, rng.random()))

# --- ruined buildings ------------------------------------------------------

BUILDINGS = [
    (40, 40, 60, 44, "brick"),
    (330, 50, 70, 50, "stone"),
    (60, 330, 50, 70, "stone"),
    (340, 360, 80, 56, "brick"),
    (140, 120, 40, 30, "brick"),
    (330, 160, 50, 34, "stone"),
    (120, 400, 44, 40, "brick"),
]


def wall_color(kind):
    if kind == "brick":
        return lambda x, y, z: shade("brick", 3, rng.random()) if z % 3 else "stone2"
    return lambda x, y, z: shade("stone", 4, rng.random())


for bx, by, bw, bd, kind in BUILDINGS:
    base_h = rng.randint(10, 15)
    color = wall_color(kind)
    for y in range(by, by + bd):
        for x in range(bx, bx + bw):
            inside = bx + 2 <= x < bx + bw - 2 and by + 2 <= y < by + bd - 2
            if inside:
                put(x, y, 3, "wood0" if (y // 2) % 2 == 0 else "wood1")
                continue
            # broken top: height dips along the wall
            h = base_h - int(noise(x * 3, y * 3, 9) * 9)
            # doorways in the middle of the south and north walls
            door = abs(x - (bx + bw // 2)) < 4 and (y < by + 2 or y >= by + bd - 2)
            for z in range(GROUND, GROUND + max(2, h)):
                if door and z < GROUND + 9:
                    continue
                window = z in (GROUND + 5, GROUND + 6) and (x + y) % 10 < 3
                if window:
                    continue
                put(x, y, z, color(x, y, z))

# --- crates ------------------------------------------------------------------


def blocked(x0, y0, x1, y1, margin):
    if abs((x0 + x1) // 2 - CX) < PLAZA + 12 and abs((y0 + y1) // 2 - CY) < PLAZA + 12:
        return True
    for x in range(x0 - margin, x1 + margin, 2):
        for y in range(y0 - margin, y1 + margin, 2):
            if on_path(x, y) or get(x, y, GROUND) or x < WALL + 4 or y < WALL + 4 or x >= W - WALL - 4 or y >= D - WALL - 4:
                return True
    return False


def crate(x, y, z):
    def pick(px, py, pz):
        edges = (px in (x, x + 4)) + (py in (y, y + 4)) + (pz in (z, z + 4))
        return "wood_dark0" if edges >= 2 else ("wood1" if (px + py + pz) % 2 else "wood0")
    box(x, y, z, x + 5, y + 5, z + 5, pick)


placed = 0
while placed < 20:
    x, y = rng.randrange(20, W - 30), rng.randrange(20, D - 30)
    if blocked(x, y, x + 11, y + 6, 2):
        continue
    crate(x, y, GROUND)
    if rng.random() < 0.6:
        crate(x + 5, y, GROUND)
    if rng.random() < 0.4:
        crate(x + 2, y, GROUND + 5)
    placed += 1

# --- trees -------------------------------------------------------------------

trees = []
for _ in range(400):
    if len(trees) >= 70:
        break
    x, y = rng.randrange(16, W - 16), rng.randrange(16, D - 16)
    if blocked(x - 7, y - 7, x + 9, y + 9, 1) or any((x - tx) ** 2 + (y - ty) ** 2 < 15 ** 2 for tx, ty in trees):
        continue
    trees.append((x, y))
    trunk = rng.randint(7, 11)
    box(x, y, GROUND, x + 2, y + 2, GROUND + trunk, "trunk")
    r = rng.randint(4, 6)
    cx, cy, cz = x + 1, y + 1, GROUND + trunk + 2
    for z in range(cz - r, cz + r + 1):
        for yy in range(cy - r, cy + r + 1):
            for xx in range(cx - r, cx + r + 1):
                d = ((xx - cx) / r) ** 2 + ((yy - cy) / r) ** 2 + ((z - cz) / (r * 0.8)) ** 2
                if d <= 1.0 and not (d > 0.75 and rng.random() < 0.25):
                    put(xx, yy, z, shade("leaf", 3, noise(xx * 4, yy * 4 + z * 7, 5)))

# --- flowers -----------------------------------------------------------------

for _ in range(1400):
    # flowers grow in meadows: only where coarse noise says so
    x, y = rng.randrange(WALL, W - WALL), rng.randrange(WALL, D - WALL)
    if noise(x + 999, y, 30) < 0.62:
        continue
    if not on_path(x, y) and vox[x + y * W + 3 * W * D] >= INDEX["grass0"] and not get(x, y, GROUND):
        put(x, y, GROUND, rng.choice(["flower_pink", "flower_yellow", "flower_white"]))

# --- writing .vox --------------------------------------------------------------


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
        r, g, b = colors[i] if i < len(colors) else (0, 0, 0)
        out += bytes((r, g, b, 255))
    return chunk(b"RGBA", bytes(out))


def write_vox(path, models, colors):
    """models: list of ((sx, sy, sz), voxel bytes x,y,z,i..., (tx, ty, tz) centre)."""
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
    data = b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", body)
    path.write_bytes(data)
    return len(data)


PLAYER_COLORS = [(240, 240, 240), (63, 182, 232), (35, 50, 74), (255, 90, 90), (125, 249, 255), (90, 96, 112)]
WHITE, CYAN, DARK, RED, EYE, GRAY = range(1, 7)

# Model 0 stands still, models 1.. are the walk cycle. Per pose:
# (left leg dy, dz), (right leg dy, dz), left arm dy, right arm dy.
# Forward is +y, so the robot's left side is -x.
PLAYER_POSES = [
    ((0, 0), (0, 0), 0, 0),    # standing
    ((1, 0), (-1, 0), -1, 1),  # left foot forward, right arm forward
    ((0, 0), (0, 1), 0, 0),    # passing, right foot lifted
    ((-1, 0), (1, 0), 1, -1),  # right foot forward, left arm forward
    ((0, 1), (0, 0), 0, 0),    # passing, left foot lifted
]


def robot(left_leg, right_leg, left_arm, right_arm):
    """A small robot, 5 x 4 x 10, facing +y."""
    cells = {}
    for x, (dy, dz) in ((1, left_leg), (3, right_leg)):
        for y in (1, 2):
            for z in range(dz, 3):
                cells[(x, y + dy, z)] = DARK if z == dz else GRAY
    for z in range(3, 6):
        for y in range(4):
            for x in range(1, 4):
                cells[(x, y, z)] = GRAY if y == 0 else (CYAN if z == 4 and y == 3 else WHITE)
    for x, dy in ((0, left_arm), (4, right_arm)):
        for y in (1, 2):
            for z in range(3, 6):
                cells[(x, y + dy, z)] = CYAN
    for z in range(6, 9):
        for y in range(4):
            for x in range(1, 4):
                cells[(x, y, z)] = WHITE
    for x in (1, 2, 3):
        cells[(x, 3, 7)] = DARK
    cells[(1, 3, 7)] = EYE
    cells[(3, 3, 7)] = EYE
    cells[(2, 1, 9)] = RED
    return cells


def player_model():
    """The robot's poses as models side by side in one scene."""
    models = []
    for k, pose in enumerate(PLAYER_POSES):
        data = bytearray()
        for (x, y, z), i in robot(*pose).items():
            data += bytes((x, y, z, i))
        models.append(((5, 4, 10), data, (k * 8 + 2, 2, 5)))
    return models, PLAYER_COLORS


out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "games" / "arena"
out.mkdir(parents=True, exist_ok=True)
voxels = [(i % W, i // W % D, i // (W * D), v) for i, v in enumerate(vox) if v]
write_scene(out / "world.vvx", (W, D, H), voxels, {i + 1: rgb for i, rgb in enumerate(PALETTE.values())}, world=True)
print(f"wrote {out / 'world.vvx'} ({(out / 'world.vvx').stat().st_size // 1024} KiB, {W}x{D}x{H})")
models, colors = player_model()
write_vox(out / "player.vox", models, colors)
print(f"wrote {out / 'player.vox'}")

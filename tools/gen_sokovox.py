#!/usr/bin/env python3
"""Makes Sokovox's models (games/sokovox): the warehouse keeper (3 frames:
standing, two strides) and the crate (2 frames: on the floor, on a goal).
A square of the game is 9 voxels; both sit in the middle of one. Plain Python, no numpy.
The levels are built in Lua from levels/microban.txt (main.lua)."""

import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vvx  # noqa: E402

GAME = Path(__file__).resolve().parent.parent / "games" / "sokovox"


def chunk(tag, content=b"", children=b""):
    return tag + struct.pack("<ii", len(content), len(children)) + content + children


def vstr(s):
    b = s.encode()
    return struct.pack("<i", len(b)) + b


def vdict(d):
    return struct.pack("<i", len(d)) + b"".join(vstr(k) + vstr(v) for k, v in d.items())


def write_vox(path, size, frames, colours, materials=None):
    """frames: lists of (x, y, z, colour index); colours: index -> (r, g, b);
    materials: index -> MagicaVoxel's material keys ({"_type": "_metal",
    "_metal": "0.8", "_rough": "0.3"}), the rest plain. A path ending .vvx
    gets Voxcade's own model (tools/vvx.py), what a game holds; .vox is
    MagicaVoxel's, for a world or a map's prefab."""
    if str(path).endswith(".vvx"):
        return vvx.write_frames(path, size, frames, colours, materials)
    body = b""
    for voxels in frames:
        data = b"".join(struct.pack("<BBBB", x, y, z, i) for x, y, z, i in voxels)
        body += chunk(b"SIZE", struct.pack("<iii", *size))
        body += chunk(b"XYZI", struct.pack("<i", len(voxels)) + data)
    if len(frames) > 1:
        ids = [2 + 2 * k for k in range(len(frames))]
        body += chunk(b"nTRN", struct.pack("<i", 0) + vdict({}) + struct.pack("<iiii", 1, -1, -1, 1) + vdict({}))
        body += chunk(b"nGRP", struct.pack("<i", 1) + vdict({}) + struct.pack("<i", len(ids)) + b"".join(struct.pack("<i", i) for i in ids))
        for k in range(len(frames)):
            node = ids[k]
            frame = vdict({"_t": f"{k * (size[0] + 2)} 0 {size[2] // 2}"})
            body += chunk(b"nTRN", struct.pack("<i", node) + vdict({}) + struct.pack("<iiii", node + 1, -1, 0, 1) + frame)
            body += chunk(b"nSHP", struct.pack("<i", node + 1) + vdict({}) + struct.pack("<ii", 1, k) + vdict({}))
        body += chunk(b"LAYR", struct.pack("<i", 0) + vdict({}) + struct.pack("<i", -1))
    palette = bytearray()
    for i in range(256):
        r, g, b = colours.get(i + 1, (0, 0, 0))
        palette += bytes((r, g, b, 255))
    body += chunk(b"RGBA", bytes(palette))
    for i, keys in sorted((materials or {}).items()):
        body += chunk(b"MATL", struct.pack("<i", i) + vdict(keys))
    path.write_bytes(b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", body))



def write_parts(path, parts, colours, materials=None):
    """A figure of parts: each part (name, size, its voxels as (x, y, z,
    colour index) in its own box, where its box's low corner stands in the
    figure) an object of that name in MagicaVoxel's scene, placed (its
    middle at its translation, as MagicaVoxel places a model); a path
    ending .vvx gets Voxcade's own model."""
    if str(path).endswith(".vvx"):
        return vvx.write_parts(path, parts, colours, materials)
    body = b""
    for _, size, voxels, _ in parts:
        data = b"".join(struct.pack("<BBBB", x, y, z, i) for x, y, z, i in voxels)
        body += chunk(b"SIZE", struct.pack("<iii", *size))
        body += chunk(b"XYZI", struct.pack("<i", len(voxels)) + data)
    ids = [2 + 2 * k for k in range(len(parts))]
    body += chunk(b"nTRN", struct.pack("<i", 0) + vdict({}) + struct.pack("<iiii", 1, -1, -1, 1) + vdict({}))
    body += chunk(b"nGRP", struct.pack("<i", 1) + vdict({}) + struct.pack("<i", len(ids)) + b"".join(struct.pack("<i", i) for i in ids))
    for k, (name, size, _, low) in enumerate(parts):
        node = ids[k]
        at = " ".join(str(low[a] + size[a] // 2) for a in range(3))
        body += chunk(b"nTRN", struct.pack("<i", node) + vdict({"_name": name}) + struct.pack("<iiii", node + 1, -1, 0, 1) + vdict({"_t": at}))
        body += chunk(b"nSHP", struct.pack("<i", node + 1) + vdict({}) + struct.pack("<ii", 1, k) + vdict({}))
    body += chunk(b"LAYR", struct.pack("<i", 0) + vdict({}) + struct.pack("<i", -1))
    palette = bytearray()
    for i in range(256):
        r, g, b = colours.get(i + 1, (0, 0, 0))
        palette += bytes((r, g, b, 255))
    body += chunk(b"RGBA", bytes(palette))
    for i, keys in sorted((materials or {}).items()):
        body += chunk(b"MATL", struct.pack("<i", i) + vdict(keys))
    path.write_bytes(b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", body))

# The keeper, 5 wide, 4 deep and 11 high, facing +y: boots, blue overalls
# over a red shirt, hands, a face with eyes, and a yellow cap with its peak
# to the front.
BOOT, OVERALL, SHIRT, SKIN, EYE, CAP, BUCKLE = range(1, 8)
KEEPER_COLOURS = {BOOT: (54, 38, 30), OVERALL: (46, 82, 150), SHIRT: (196, 52, 44), SKIN: (236, 188, 148), EYE: (30, 26, 34), CAP: (236, 190, 40), BUCKLE: (230, 220, 160)}


def keeper():
    body = []
    for x in range(1, 4):
        body += [(x, 1, 3, OVERALL), (x, 2, 3, OVERALL), (x, 1, 4, OVERALL), (x, 2, 4, OVERALL)]
        body += [(x, 1, 5, SHIRT), (x, 2, 5, SHIRT)]
    body += [(1, 3, 4, BUCKLE), (3, 3, 4, BUCKLE), (2, 3, 4, OVERALL), (2, 3, 3, OVERALL)]
    for x in (0, 4):
        body += [(x, 2, 5, SHIRT), (x, 2, 4, SHIRT), (x, 2, 3, SKIN)]
    for x in range(1, 4):
        for y in range(1, 4):
            body += [(x, y, 6, SKIN), (x, y, 7, SKIN), (x, y, 8, SKIN)]
    body += [(1, 3, 7, EYE), (3, 3, 7, EYE)]
    for x in range(1, 4):
        for y in range(1, 4):
            body += [(x, y, 9, CAP)]
    body += [(2, 2, 10, CAP)]

    def legs(left, right):
        """Each leg's foot a step forward (+1), back (-1) or under him (0)."""
        out = []
        for x, step in ((1, left), (3, right)):
            out += [(x, 2, 2, OVERALL), (x, 2 + step, 1, OVERALL), (x, 2 + step, 0, BOOT), (x, 3 + step, 0, BOOT)]
        out += [(2, 2, 2, OVERALL)]
        return out

    frames = []
    for left, right in ((0, 0), (1, -1), (-1, 1)):
        voxels = list(dict.fromkeys(body + legs(left, right)))
        frames.append([v for v in voxels if 0 <= v[1] < 5])
    write_vox(GAME / "keeper.vvx", (5, 5, 11), frames, KEEPER_COLOURS)


# The crate, 7 by 7 by 7: a dark frame along its edges, planks between,
# a brace across each side. On a goal it glows golden.
FRAME, PLANK, PLANK2, BRACE = 1, 2, 3, 4
GOAL_FRAME, GOAL_PLANK, GOAL_PLANK2, GOAL_BRACE = 5, 6, 7, 8
CRATE_COLOURS = {
    FRAME: (92, 60, 34), PLANK: (168, 118, 66), PLANK2: (150, 102, 56), BRACE: (118, 80, 44),
    GOAL_FRAME: (150, 112, 30), GOAL_PLANK: (246, 204, 92), GOAL_PLANK2: (232, 186, 70), GOAL_BRACE: (210, 160, 50),
}


def crate(home):
    frame, plank, plank2, brace = (GOAL_FRAME, GOAL_PLANK, GOAL_PLANK2, GOAL_BRACE) if home else (FRAME, PLANK, PLANK2, BRACE)
    n = 6
    voxels = []
    for x in range(7):
        for y in range(7):
            for z in range(7):
                edges = (x in (0, n)) + (y in (0, n)) + (z in (0, n))
                if edges == 0:
                    continue  # hollow inside
                if edges >= 2:
                    voxels.append((x, y, z, frame))
                    continue
                # A face: which coordinates run across it.
                if x in (0, n):
                    a, b = y, z
                elif y in (0, n):
                    a, b = x, z
                else:
                    a, b = x, y
                if a == b or a == n - b:
                    colour = brace
                else:
                    colour = plank if b % 2 else plank2
                voxels.append((x, y, z, colour))
    return voxels


if __name__ == "__main__":
    GAME.mkdir(parents=True, exist_ok=True)
    keeper()
    write_vox(GAME / "crate.vvx", (7, 7, 7), [crate(False), crate(True)], CRATE_COLOURS)
    print(f"wrote {GAME}/keeper.vvx, crate.vvx")

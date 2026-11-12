#!/usr/bin/env python3
"""Pixel art to voxels: sprite frames become a model, a frame each (OUT.vvx,
Voxcade's own, tools/vvx.py; OUT.vox writes MagicaVoxel's, to bring in),
for a game seen from the side. Every pixel becomes `scale` by `scale`
voxels, and a column of them through the figure's thickness: thick in its
middle, thin at its edges ("pillow"), as far from the camera as towards
it, so it turns round to face the other way. The picture's dark outline is
left out (its colour taken from the figure beside it); `--outline` keeps
it. A flat thickness (`flat`)
makes a slab instead (platforms, blocks). Only the shell is written: the
voxels inside are never seen.

The model faces +y (the engine turns it to face where it goes): the
picture's left to right runs along y, its bottom to top along z, the
thickness along x. A figure's frames share one box: as wide on both sides
of the frames' middle, its feet on the bottom row, its sizes across odd so
it turns about a voxel's middle without shifting.

usage: python3 tools/sprite2vox.py OUT.vvx SHEET.png:FRAMES [SHEET.png:FRAMES ...]
           [--scale 2] [--depth 7] [--flat N] [--outline] [--across]
A sheet holds its frames side by side, all as wide; the frames of all
sheets follow one another in the model. Uses Pillow.
"""

import struct
import sys
from pathlib import Path

from PIL import Image


def frames_of(path, count):
    """A sheet's frames, left to right."""
    sheet = Image.open(path).convert("RGBA")
    width = sheet.width // count
    return [sheet.crop((k * width, 0, (k + 1) * width, sheet.height)) for k in range(count)]


def opaque(frame, x, y):
    return frame.getpixel((x, y))[3] >= 128


def outline_colour(frames):
    """The colour pixel art draws its outline in: the commonest on the
    edges of the figure, if it is dark (None otherwise)."""
    count = {}
    for f in frames:
        for y in range(f.height):
            for x in range(f.width):
                if opaque(f, x, y) and edge(f, x, y):
                    c = f.getpixel((x, y))[:3]
                    count[c] = count.get(c, 0) + 1
    if not count:
        return None
    c = max(sorted(count), key=lambda c: count[c])
    return c if 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2] < 90 else None


def edge(frame, x, y):
    """Whether a pixel touches the empty space around the figure."""
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        nx, ny = x + dx, y + dy
        if not (0 <= nx < frame.width and 0 <= ny < frame.height) or not opaque(frame, nx, ny):
            return True
    return False


def unline(frame, outline):
    """The figure without its outline: a pixel of the outline's colour on
    its edge takes the colour of the figure beside it (the commonest of
    its neighbours that are not the outline). Lines inside the figure and
    parts all of that colour (dark paws) stay. In voxels light and shade
    draw the edge; a dark rim round a rounded figure looks wrong."""
    out = frame.copy()
    for y in range(frame.height):
        for x in range(frame.width):
            if not opaque(frame, x, y) or frame.getpixel((x, y))[:3] != outline or not edge(frame, x, y):
                continue
            near = {}
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    nx, ny = x + dx, y + dy
                    if (dx or dy) and 0 <= nx < frame.width and 0 <= ny < frame.height and opaque(frame, nx, ny):
                        c = frame.getpixel((nx, ny))
                        if c[:3] != outline:
                            near[c] = near.get(c, 0) + 1
            if near:
                out.putpixel((x, y), max(sorted(near), key=lambda c: near[c]))
    return out


def common_box(frames):
    """The box the frames' pixels fit in: as wide on both sides of their
    middle column, from the highest pixel down to the lowest."""
    width, height = frames[0].size
    middle = width // 2
    reach, top, bottom = 0, height, 0
    for f in frames:
        for y in range(height):
            for x in range(width):
                if opaque(f, x, y):
                    reach = max(reach, abs(x - middle))
                    top, bottom = min(top, y), max(bottom, y)
    return middle - reach, top, middle + reach, bottom


def distances(cells, width, height):
    """Each filled cell's distance (in cells, diagonal steps 1.4) to the
    nearest empty one or the edge: two passes of a chamfer transform."""
    far = 10 ** 6
    d = [[0 if not cells[y][x] else far for x in range(width)] for y in range(height)]
    get = lambda x, y: d[y][x] if 0 <= x < width and 0 <= y < height else 0
    for y in range(height):
        for x in range(width):
            if d[y][x]:
                d[y][x] = min(d[y][x], get(x - 1, y) + 10, get(x, y - 1) + 10, get(x - 1, y - 1) + 14, get(x + 1, y - 1) + 14)
    for y in reversed(range(height)):
        for x in reversed(range(width)):
            if d[y][x]:
                d[y][x] = min(d[y][x], get(x + 1, y) + 10, get(x, y + 1) + 10, get(x + 1, y + 1) + 14, get(x - 1, y + 1) + 14)
    return [[v / 10 for v in row] for row in d]


def voxels(frame, box, scale, depth, flat, palette):
    """One frame's voxels (x, y, z, colour index) in a model `2 * depth + 1`
    (or `flat`) thick; `palette` gathers the colours."""
    x0, top, x1, bottom = box
    width, height = (x1 - x0 + 1) * scale, (bottom - top + 1) * scale
    # The picture at voxel size: rows from the bottom up.
    colour = [[None] * width for _ in range(height)]
    for z in range(height):
        for y in range(width):
            px, py = x0 + y // scale, bottom - z // scale
            if not 0 <= px < frame.width:
                continue  # the box reaches past an even picture's edge
            r, g, b, a = frame.getpixel((px, py))
            if a >= 128:
                colour[z][y] = (r, g, b)
    thick = (flat | 1) if flat else 2 * depth + 1
    centre = thick // 2
    dist = None if flat else distances([[c is not None for c in row] for row in colour], width, height)
    out = []
    for z in range(height):
        for y in range(width):
            c = colour[z][y]
            if c is None:
                continue
            if c not in palette:
                if len(palette) >= 255:
                    raise SystemExit("more than 255 colours")
                palette[c] = len(palette) + 1
            half = centre if flat else min(depth, max(1, round(1.8 * dist[z][y] ** 0.5)))
            for x in range(centre - half, centre + half + 1):
                out.append((x, y, z, palette[c]))
    # Only the shell: a voxel with all six neighbours filled is never seen.
    filled = {(x, y, z) for x, y, z, _ in out}
    around = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
    out = [v for v in out if any((v[0] + dx, v[1] + dy, v[2] + dz) not in filled for dx, dy, dz in around)]
    # Odd across: an even picture gets an empty column at its front.
    return out, (thick, width | 1, height)


def chunk(tag, content=b"", children=b""):
    return tag + struct.pack("<ii", len(content), len(children)) + content + children


def vstr(s):
    b = s.encode()
    return struct.pack("<i", len(b)) + b


def vdict(d):
    return struct.pack("<i", len(d)) + b"".join(vstr(k) + vstr(v) for k, v in d.items())


def write_vox(path, size, frames, colours):
    """A model with one frame a model; frames: lists of (x, y, z, colour
    index); colours: index -> (r, g, b). A .vvx path gets Voxcade's own."""
    if str(path).endswith(".vvx"):
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import vvx
        return vvx.write_frames(path, size, frames, colours)
    body = b""
    for vs in frames:
        body += chunk(b"SIZE", struct.pack("<iii", *size))
        body += chunk(b"XYZI", struct.pack("<i", len(vs)) + b"".join(struct.pack("<BBBB", *v) for v in vs))
    if len(frames) > 1:
        ids = [2 + 2 * k for k in range(len(frames))]
        body += chunk(b"nTRN", struct.pack("<i", 0) + vdict({}) + struct.pack("<iiii", 1, -1, -1, 1) + vdict({}))
        body += chunk(b"nGRP", struct.pack("<i", 1) + vdict({}) + struct.pack("<i", len(ids)) + b"".join(struct.pack("<i", i) for i in ids))
        for k in range(len(frames)):
            node = ids[k]
            place = vdict({"_t": f"{k * (size[0] + 2)} 0 {size[2] // 2}"})
            body += chunk(b"nTRN", struct.pack("<i", node) + vdict({}) + struct.pack("<iiii", node + 1, -1, 0, 1) + place)
            body += chunk(b"nSHP", struct.pack("<i", node + 1) + vdict({}) + struct.pack("<ii", 1, k) + vdict({}))
        body += chunk(b"LAYR", struct.pack("<i", 0) + vdict({}) + struct.pack("<i", -1))
    palette = bytearray()
    for i in range(256):
        r, g, b = colours.get(i + 1, (0, 0, 0))
        palette += bytes((r, g, b, 255))
    body += chunk(b"RGBA", bytes(palette))
    Path(path).write_bytes(b"VOX " + struct.pack("<i", 150) + chunk(b"MAIN", b"", body))


def convert(out, sheets, scale=2, depth=7, flat=None, outline=False, across=False):
    """The frames of `sheets` ((path, frame count), ...) as one model,
    without their outline unless `outline`; `across`: the picture runs
    along x (mirrored, read from +y: a prop's front, which the game turns
    to the camera), its thickness along y; not along y (a figure facing
    +y)."""
    frames = [f for path, count in sheets for f in frames_of(path, count)]
    line = None if outline else outline_colour(frames)
    if line:
        frames = [unline(f, line) for f in frames]
    box = common_box(frames)
    palette = {}
    models, size = [], None
    for f in frames:
        vs, size = voxels(f, box, scale, depth, flat, palette)
        if across:
            # The picture along x, mirrored: a model's front is its +y side,
            # and things turn it to the camera (a half turn).
            vs = [(size[1] - 1 - y, x, z, c) for x, y, z, c in vs]
            size = (size[1], size[0], size[2])
        models.append(vs)
    if any(s > 256 for s in size):
        raise SystemExit(f"{out}: {size} is larger than a .vox model may be (256)")
    write_vox(out, size, models, {i: c for c, i in palette.items()})
    return size, len(frames)


def main(args):
    if len(args) < 2:
        raise SystemExit(__doc__)
    out, rest = args[0], args[1:]
    options = {"--scale": 2, "--depth": 7, "--flat": None}
    outline = across = False
    sheets = []
    while rest:
        a = rest.pop(0)
        if a == "--outline":
            outline = True
        elif a == "--across":
            across = True
        elif a in options:
            options[a] = int(rest.pop(0))
        else:
            path, _, count = a.rpartition(":")
            sheets.append((path, int(count)) if path else (a, 1))
    size, n = convert(out, sheets, options["--scale"], options["--depth"], options["--flat"], outline, across)
    print(f"wrote {out}: {n} frames of {size[0]} by {size[1]} by {size[2]}")


if __name__ == "__main__":
    main(sys.argv[1:])

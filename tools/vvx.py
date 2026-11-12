#!/usr/bin/env python3
"""Voxcade's model files (.vvx, src/vvx.rs) from Python: what the
generators write instead of MagicaVoxel's .vox, and what tools read.
Plain Python (3.11 on, for tomllib); imported, not run.

A model is text: `format = "voxcade voxel 1"`, its `size`, a `[palette]`
of signs (a letter or digit a colour, a table with a material), then
`[[frame]]`s or a figure's `[[part]]`s, each with its `layers` (bottom
layer first, the far row first in a layer, `.` empty, a row ` *N` or a
line `*N` after a layer for repeats). `at` and `joint` are measured from
the pivot: the cell size // 2 across and deep, on the floor.

write_frames(path, size, frames, colours, materials)  frames of a box
write_parts(path, parts, colours, materials)           a figure of parts
read(path)                                             a model's voxels
write_model(path, model, kept=None)                    a model read, written

`colours` map a colour index to (r, g, b), `materials` an index to
MagicaVoxel's material keys ({"_type": "_metal", "_metal": "0.8",
"_rough": "0.3"}; `_glass` with `_trans`, `_emit` with `_emit`), as the
generators kept them for .vox."""

import tomllib

FORMAT = "voxcade voxel 1"
SIGNS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
REPEAT_FROM = 3


def _material(rgb, keys):
    """A palette entry's text and its identity: a colour, or a table."""
    colour = "#%02x%02x%02x" % tuple(rgb)
    keys = keys or {}
    kind = keys.get("_type")
    fields = []
    if kind == "_glass":
        glass = float(keys["_trans"]) if "_trans" in keys else 1.0 - float(keys.get("_alpha", 0.5))
        fields.append(f"glass = {_num(glass)}")
        if "_rough" in keys:
            fields.append(f"rough = {_num(float(keys['_rough']))}")
    elif kind == "_metal":
        fields.append(f"metal = {_num(float(keys.get('_metal', 1.0)))}")
        fields.append(f"rough = {_num(float(keys.get('_rough', 0.3)))}")
    elif kind == "_emit":
        fields.append(f"glow = {_num(float(keys.get('_emit', 1.0)))}")
    if not fields:
        return f'"{colour}"'
    return "{ color = \"%s\", %s }" % (colour, ", ".join(fields))


def _num(v):
    text = f"{v:.4f}".rstrip("0")
    return text + "0" if text.endswith(".") else text


class _Signs:
    """Signs by first use, keeping those a file had (`kept`: sign -> entry
    text) for the same entry."""

    def __init__(self, count, kept=None):
        self.width = 1 if count <= len(SIGNS) else 2
        self.kept = {t: s for s, t in (kept or {}).items() if len(s) == self.width}
        self.of, self.order, self.next = {}, [], 0

    def _nth(self, i):
        return SIGNS[i] if self.width == 1 else SIGNS[i // len(SIGNS)] + SIGNS[i % len(SIGNS)]

    def sign(self, entry):
        if entry in self.of:
            return self.of[entry]
        s = self.kept.get(entry)
        if s is None:
            taken = set(self.kept.values()) | {x for x, _ in self.order}
            while True:
                s = self._nth(self.next)
                self.next += 1
                if s not in taken:
                    break
        self.of[entry] = s
        self.order.append((s, entry))
        return s

    def lines(self):
        return "".join(f"{s} = {entry}\n" for s, entry in self.order)


def _layers(cells, signs):
    """`layers = \"\"\" ... \"\"\"` of voxels {(x, y, z): entry} in their
    own box from 0; and nothing else."""
    if not cells:
        return 'layers = ""\n'
    hi = [max(p[a] for p in cells) + 1 for a in range(3)]
    dots = "." * signs.width
    layers = []
    for z in range(hi[2]):
        rows = []
        for y in reversed(range(hi[1])):
            rows.append("".join(signs.sign(cells[(x, y, z)]) if (x, y, z) in cells else dots for x in range(hi[0])))
        layers.append(rows)

    def runs(items):
        out = []
        for item in items:
            if out and out[-1][0] == item:
                out[-1][1] += 1
            else:
                out.append([item, 1])
        return out

    text = 'layers = """\n'
    first = True
    for layer, times in runs(layers):
        for _ in range(1 if times >= REPEAT_FROM else times):
            if not first:
                text += "\n"
            first = False
            for row, n in runs(layer):
                text += f"{row} *{n}\n" if n >= REPEAT_FROM else f"{row}\n" * n
            if times >= REPEAT_FROM:
                text += f"*{times}\n"
    return text + '"""\n'


def _write(path, size, blocks, kept=None):
    """blocks: (header lines, {(x, y, z): entry} in the model's box) a frame
    or part each; a frame's header None."""
    count = len({e for _, cells in blocks for e in cells.values()})
    signs = _Signs(count, kept)
    pivot = (size[0] // 2, size[1] // 2, 0)
    body = ""
    for head, cells in blocks:
        low = tuple(min(p[a] for p in cells) for a in range(3)) if cells else (0, 0, 0)
        own = {tuple(p[a] - low[a] for a in range(3)): e for p, e in cells.items()}
        at = tuple(low[a] - pivot[a] for a in range(3))
        if head is None:
            body += "\n[[frame]]\n"
            if cells and low != (0, 0, 0):
                body += "at = [%d, %d, %d]\n" % at
        else:
            body += "\n[[part]]\n" + head[0]
            if cells:
                body += "at = [%d, %d, %d]\n" % at
            body += "".join(head[1:])
        body += _layers(own, signs)
    text = f'format = "{FORMAT}"\nsize = [{size[0]}, {size[1]}, {size[2]}]\n\n[palette]\n' + signs.lines() + body
    with open(path, "w") as f:
        f.write(text)


def _entries(colours, materials):
    return {i: _material(rgb, (materials or {}).get(i)) for i, rgb in colours.items()}


def write_frames(path, size, frames, colours, materials=None):
    """A model of frames: each a list of (x, y, z, colour index) in the box
    0..size; a voxel twice in a cell, the first stays."""
    entries = _entries(colours, materials)
    blocks = []
    for voxels in frames:
        cells = {}
        for x, y, z, i in voxels:
            cells.setdefault((x, y, z), entries[i])
        blocks.append((None, cells))
    _write(path, size, blocks)


def write_parts(path, parts, colours, materials=None):
    """A figure of parts: each (name, size, its voxels as (x, y, z, colour
    index) in its own box, where its box's low corner stands in the figure),
    as gen_sokovox.write_parts takes them; the model's box is all of them."""
    entries = _entries(colours, materials)
    placed = []
    for name, _, voxels, low in parts:
        cells = {}
        for x, y, z, i in voxels:
            cells.setdefault((low[0] + x, low[1] + y, low[2] + z), entries[i])
        placed.append((name, cells))
    every = [p for _, cells in placed for p in cells]
    lo = tuple(min(p[a] for p in every) for a in range(3))
    hi = tuple(max(p[a] for p in every) for a in range(3))
    size = tuple(hi[a] - lo[a] + 1 for a in range(3))
    blocks = []
    for name, cells in placed:
        shifted = {tuple(p[a] - lo[a] for a in range(3)): e for p, e in cells.items()}
        blocks.append(([f'name = "{name}"\n'], shifted))
    _write(path, size, blocks)


def read(path):
    """A model file as {"size", "palette": {sign: entry text}, "colours":
    {sign: (r, g, b)}, "frames": [{(x, y, z): sign}], "parts": [(name,
    {(x, y, z): sign}, extra lines)]}, places in the model's box from 0."""
    with open(path, "rb") as f:
        data = tomllib.load(f)
    if data.get("format") != FORMAT:
        raise SystemExit(f"{path}: not a Voxcade model of format 1 (its first line: format = \"{FORMAT}\")")
    size = data["size"]
    pivot = (size[0] // 2, size[1] // 2, 0)
    palette, colours = {}, {}
    for sign, entry in data["palette"].items():
        hexa = entry if isinstance(entry, str) else entry["color"]
        colours[sign] = tuple(int(hexa[k:k + 2], 16) for k in (1, 3, 5))
        if isinstance(entry, str):
            palette[sign] = f'"{entry}"'
        else:
            palette[sign] = "{ " + ", ".join(f'{k} = "{v}"' if isinstance(v, str) else f"{k} = {_num(v)}" for k, v in entry.items()) + " }"
    width = len(next(iter(palette), "a"))

    def cells(table, default_at):
        at = table.get("at", default_at)
        out = {}
        layers, rows, z = [], [], 0
        for line in table.get("layers", "").split("\n") + [""]:
            words = line.split()
            if not words:
                if rows:
                    layers.append((rows, 1))
                    rows = []
                continue
            if words[0].startswith("*"):
                layers.append((rows, int(words[0][1:])))
                rows = []
                continue
            rows += [words[0]] * (int(words[1][1:]) if len(words) > 1 else 1)
        for rows, times in layers:
            for _ in range(times):
                for r, row in enumerate(rows):
                    y = len(rows) - 1 - r
                    for x in range(len(row) // width):
                        s = row[x * width:(x + 1) * width]
                        if s.strip("."):
                            out[(at[0] + pivot[0] + x, at[1] + pivot[1] + y, at[2] + z)] = s
                z += 1
        return out

    box = [-pivot[0], -pivot[1], 0]
    frames = [cells(t, box) for t in data.get("frame", [])]
    parts = []
    for t in data.get("part", []):
        extra = "".join(f'{k} = "{t[k]}"\n' if isinstance(t[k], str) else f"{k} = [{', '.join(str(v) for v in t[k])}]\n" for k in ("parent", "joint") if k in t)
        parts.append((t["name"], cells(t, box), extra))
    return {"size": size, "palette": palette, "colours": colours, "frames": frames, "parts": parts}


def write_model(path, model, kept=None):
    """A model as `read` gives it (its cells naming entries by sign) written
    again; the signs of `kept` (sign: entry text) stay."""
    def entries(cells):
        return {p: model["palette"][s] for p, s in cells.items()}

    if model["parts"]:
        blocks = [([f'name = "{name}"\n', extra], entries(cells)) for name, cells, extra in model["parts"]]
    else:
        blocks = [(None, entries(cells)) for cells in model["frames"]]
    _write(path, model["size"], blocks, kept)

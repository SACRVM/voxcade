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
write_scene(path, size, voxels, colours, markers, world)  a world or a prefab
write_world(path, model, palette_signs(path))          a world or prefab read, written
write_level(path, block, size, blocks, cells, things)  a level of blocks
write_map(path, size, ground, prefabs, put, places)    a map
read_level(path), read_map(path)                       read back

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


def _rest_lines(rotation, pitch, roll):
    """A part's rest rotation (degrees about its joint), each line only
    when not 0, as the engine writes them."""
    def number(v):
        return str(int(v)) if float(v).is_integer() else str(v)
    return "".join(f"{k} = {number(v)}\n" for k, v in (("rotation", rotation), ("pitch", pitch), ("roll", roll)) if v)


def _offset_line(offset):
    """A part's offset (less than a voxel each way from its `at`), only
    when not 0, as the engine writes it (one line after `at`)."""
    def number(v):
        return str(int(v)) if float(v).is_integer() else str(v)
    if not any(offset):
        return ""
    return "offset = [" + ", ".join(number(v) for v in offset) + "]\n"


def write_parts(path, parts, colours, materials=None):
    """A figure of parts: each (name, size, its voxels as (x, y, z, colour
    index) in its own box, where its box's low corner stands in the figure),
    as gen_sokovox.write_parts takes them, and optionally its rest rotation
    (rotation, pitch, roll in degrees about its joint) as a fifth and its
    offset (x, y, z, each within half a voxel) as a sixth; the model's box
    is all of them."""
    entries = _entries(colours, materials)
    placed = []
    for part in parts:
        name, _, voxels, low = part[:4]
        rest = part[4] if len(part) > 4 else (0, 0, 0)
        offset = part[5] if len(part) > 5 else (0, 0, 0)
        cells = {}
        for x, y, z, i in voxels:
            cells.setdefault((low[0] + x, low[1] + y, low[2] + z), entries[i])
        placed.append((name, cells, rest, offset))
    every = [p for _, cells, _, _ in placed for p in cells]
    lo = tuple(min(p[a] for p in every) for a in range(3))
    hi = tuple(max(p[a] for p in every) for a in range(3))
    size = tuple(hi[a] - lo[a] + 1 for a in range(3))
    blocks = []
    for name, cells, rest, offset in placed:
        shifted = {tuple(p[a] - lo[a] for a in range(3)): e for p, e in cells.items()}
        blocks.append(([f'name = "{name}"\n', _offset_line(offset), _rest_lines(*rest)], shifted))
    _write(path, size, blocks)


def read(path):
    """A model file as {"size", "palette": {sign: entry text}, "colours":
    {sign: (r, g, b)}, "frames": [{(x, y, z): sign}], "parts": [(name,
    {(x, y, z): sign}, extra lines)]}, places in the model's box from 0. A
    world's or a prefab's markers ({ marker = "kind" }) as "markers": {sign:
    kind}, their cells in the frame too."""
    with open(path, "rb") as f:
        data = tomllib.load(f)
    if data.get("format") != FORMAT:
        raise SystemExit(f"{path}: not a Voxcade model of format 1 (its first line: format = \"{FORMAT}\")")
    size = data["size"]
    pivot = (size[0] // 2, size[1] // 2, 0)
    palette, colours, markers = {}, {}, {}
    for sign, entry in data["palette"].items():
        if isinstance(entry, dict) and "marker" in entry:
            markers[sign] = entry["marker"]
            continue
        hexa = entry if isinstance(entry, str) else entry["color"]
        colours[sign] = tuple(int(hexa[k:k + 2], 16) for k in (1, 3, 5))
        if isinstance(entry, str):
            palette[sign] = f'"{entry}"'
        else:
            palette[sign] = "{ " + ", ".join(f'{k} = "{v}"' if isinstance(v, str) else f"{k} = {_num(v)}" for k, v in entry.items()) + " }"
    width = len(next(iter(data["palette"]), "a"))

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
        extra = _offset_line(t.get("offset", (0, 0, 0)))
        extra += "".join(f'{k} = "{t[k]}"\n' if isinstance(t[k], str) else f"{k} = [{', '.join(str(v) for v in t[k])}]\n" for k in ("parent", "joint") if k in t)
        extra += _rest_lines(t.get("rotation", 0), t.get("pitch", 0), t.get("roll", 0))
        parts.append((t["name"], cells(t, box), extra))
    return {"size": size, "palette": palette, "colours": colours, "markers": markers, "frames": frames, "parts": parts}


def write_model(path, model, kept=None):
    """A model as `read` gives it (its cells naming entries by sign) written
    again; the signs of `kept` (sign: entry text) stay."""
    def entries(cells):
        return {p: model["palette"][s] for p, s in cells.items()}

    if model["markers"]:
        raise ValueError(f"{path}: a world or a prefab (it has markers): write_world writes it back")
    if model["parts"]:
        blocks = [([f'name = "{name}"\n', extra], entries(cells)) for name, cells, extra in model["parts"]]
    else:
        blocks = [(None, entries(cells)) for cells in model["frames"]]
    _write(path, model["size"], blocks, kept)


def write_world(path, model, signs):
    """A world or a map's prefab as `read` gives it written again, its
    palette in the file's order (`signs`, the palette's signs as the file
    lists them: a world's n-th colour is its number n, so the order is the
    meaning; `write_model` would number them by first use)."""
    number = {s: n + 1 for n, s in enumerate(signs)}
    colours = {number[s]: c for s, c in model["colours"].items()}
    markers = {number[s]: k for s, k in model["markers"].items()}
    cells = sorted(model["frames"][0].items(), key=lambda kv: (kv[0][2], -kv[0][1], kv[0][0]))
    write_scene(path, model["size"], [(x, y, z, number[s]) for (x, y, z), s in cells], colours, markers, world=True)


def palette_signs(path):
    """The signs of a model's or a world's palette, in the file's order."""
    with open(path, "rb") as f:
        return list(tomllib.load(f)["palette"].keys())


# --- worlds and prefabs (src/scene.rs) ---------------------------------------


def write_scene(path, size, voxels, colours, markers=None, world=False):
    """A world or a map's prefab: one frame in a box `size` big. `voxels`
    are (x, y, z, colour index) in the order a .vox would hold them (a
    later one in a cell covers an earlier), `colours` an index's (r, g, b),
    `markers` an index's kind of thing. As `voxcade upgrade` writes them
    (src/upgrade/worlds.rs, `worlds::convert`): a world's colours are its indices
    1, 2, ... in order (its voxels keep their numbers); a prefab's in the
    order of their first use (the order the map takes them); a marker's
    entry names its kind, the things coming in the order of the voxels."""
    markers = markers or {}
    sx, sy, sz = size
    cells, marked, found, first_use = {}, set(), [], []
    for x, y, z, i in voxels:
        if i in markers:
            assert (x, y, z) not in cells and (x, y, z) not in marked, f"{path}: a marker shares cell {x} {y} {z}"
            marked.add((x, y, z))
            found.append(((x, y, z), markers[i]))
        else:
            assert (x, y, z) not in marked, f"{path}: a voxel shares cell {x} {y} {z} with a marker"
            cells[(x, y, z)] = i
            if i not in first_use:
                first_use.append(i)
    numbers = list(range(1, max(cells.values(), default=0) + 1)) if world else first_use
    number = {i: n + 1 for n, i in enumerate(numbers)}
    # Markers: a new entry where one entry's cells in reading order would
    # put its things otherwise.
    kinds, last = [], None
    entry_of = []
    for p, kind in found:
        key = (p[2], -p[1], p[0])
        if not (kinds and kinds[-1] == kind and last is not None and key > last):
            kinds.append(kind)
        entry_of.append(len(kinds) - 1)
        last = key
    total = len(numbers) + len(kinds)
    width = 1 if total <= len(SIGNS) else 2

    def sign(k):
        return SIGNS[k] if width == 1 else SIGNS[k // len(SIGNS)] + SIGNS[k % len(SIGNS)]

    grid = {p: sign(number[i] - 1) for p, i in cells.items()}
    for (p, _), k in zip(found, entry_of):
        grid[p] = sign(len(numbers) + k)
    text = f'format = "{FORMAT}"\nsize = [{sx}, {sy}, {sz}]\n\n'
    text += "# The colours are the world's numbers in this order (the first is 1);\n# a marker places a thing of that kind instead of a voxel.\n[palette]\n"
    for n, i in enumerate(numbers):
        text += f'{sign(n)} = "#%02x%02x%02x"\n' % tuple(colours.get(i, (0, 0, 0)))
    for k, kind in enumerate(kinds):
        text += f'{sign(len(numbers) + k)} = {{ marker = "{kind}" }}\n'
    text += "\n[[frame]]\n"
    if not grid:
        text += 'layers = ""\n'
    else:
        low = tuple(min(p[a] for p in grid) for a in range(3))
        if low != (0, 0, 0):
            text += "at = [%d, %d, %d]\n" % (low[0] - sx // 2, low[1] - sy // 2, low[2])

        class Fixed:
            pass
        signs = Fixed()
        signs.width = width
        signs.sign = lambda s: s
        own = {(p[0] - low[0], p[1] - low[1], p[2] - low[2]): s for p, s in grid.items()}
        text += _layers(own, signs)
    with open(path, "w") as f:
        f.write(text)


# --- levels of blocks (src/grid/, `text`) -------------------------------------


LEVEL_FORMAT = "voxcade level 1"


def _level_order(size, front):
    sx, sy, sz = size
    if front:
        return [[[(x, y, z) for x in range(sx)] for z in reversed(range(sz))] for y in range(sy)]
    return [[[(x, y, z) for x in range(sx)] for y in reversed(range(sy))] for z in range(sz)]


def read_level(path):
    """A level of blocks as {"block", "size", "view", "blocks": [(name,
    kind)], "legend": {sign: (name, orientation)} in the file's order,
    "cells": {(x, y, z): (block number from 1, orientation)}, "things",
    "places"}."""
    with open(path, "rb") as f:
        data = tomllib.load(f)
    if data.get("format") != LEVEL_FORMAT:
        raise SystemExit(f"{path}: not a level of format 1 (its first line: format = \"{LEVEL_FORMAT}\")")
    blocks = list(data.get("blocks", {}).items())
    names = [n for n, _ in blocks]
    legend = {}
    for sign, e in data.get("legend", {}).items():
        legend[sign] = (e, 0) if isinstance(e, str) else (e["block"], e.get("orientation", 0))
    width = len(next(iter(legend), "a"))
    view = data.get("view", "above")
    order = _level_order(data["size"], view == "front")
    layers = []
    rows = []
    for line in data["cells"]["layers"].split("\n") + [""]:
        words = line.split()
        if not words:
            if rows:
                layers.append(rows)
                rows = []
        elif words[0].startswith("*"):
            layers += [layers.pop()] * int(words[0][1:]) if not rows else [rows] * int(words[0][1:])
            rows = []
        else:
            rows += [words[0]] * (int(words[1][1:]) if len(words) > 1 else 1)
    cells = {}
    for layer, places in zip(layers, order):
        for row, ps in zip(layer, places):
            for x, p in enumerate(ps):
                sign = row[x * width:(x + 1) * width]
                if sign.strip("."):
                    name, orientation = legend[sign]
                    cells[p] = (names.index(name) + 1, orientation)
    return {"block": data["block"], "size": data["size"], "view": view, "blocks": blocks, "legend": legend, "cells": cells,
            "things": data.get("thing", []), "places": data.get("place", [])}


def _toml_key(name):
    return name if name and all(ch.isascii() and (ch.isalnum() or ch in "_-") for ch in name) else _quoted(name)


def _quoted(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def _prop_text(v):
    """A prop's value as the engine writes it (`grid::text::prop_text`):
    tables inline, their keys in order (the engine's tables sort them)."""
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, str):
        return _quoted(v)
    if isinstance(v, (list, tuple)):
        return "[" + ", ".join(_prop_text(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{_toml_key(k)} = {_prop_text(v[k])}" for k in sorted(v)) + " }" if v else "{}"
    return repr(float(v)) if isinstance(v, float) else str(v)


def _props_line(props):
    """A thing's own props (`props = {...}`, a dict in its order), as
    the engine writes them: none without any."""
    if not props:
        return ""
    return "props = { " + ", ".join(f"{_toml_key(k)} = {_prop_text(v)}" for k, v in props.items()) + " }\n"


def write_level(path, block, size, blocks, cells, things=(), places=(), kept=None, view=None):
    """A level of voxel blocks: `block` voxels each way, `size` cells (x
    across, y deep from the front, z up), `blocks` [(name, kind)],
    `cells` {(x, y, z): (block number from 1, orientation)}, `things`
    [{"kind", "at", "props"?}] (props: a dict over the kind's, in order),
    `places` [{"name", "at", "facing"?}]. As the editor
    writes it: the signs of `kept` ({sign: (name, orientation)}, a level
    read) and its `view` kept; a new one seen from the front when taller
    than deep, else from above; a new sign a pair of block and
    orientation, its name's letters first."""
    sx, sy, sz = size
    front = view == "front" if view else sz > sy
    order = _level_order(size, front)
    pairs = []
    for layer in order:
        for row in layer:
            for p in row:
                c = cells.get(p)
                if c and c[0] and c not in pairs:
                    pairs.append(c)
    width = 1 if len(pairs) <= len(SIGNS) else 2
    taken, sign_of, legend = [], {}, []
    for sign, (name, orientation) in (kept or {}).items():
        taken.append(sign)
        c = next((c for c in pairs if blocks[c[0] - 1][0] == name and c[1] == orientation), None)
        if len(sign) == width and c and c not in sign_of:
            sign_of[c] = sign
            legend.append(c)
    for c in pairs:
        if c in sign_of:
            continue
        name = blocks[c[0] - 1][0]
        if width == 1:
            wished = [w for ch in name if ch.isascii() and ch.isalnum() for w in (ch.lower(), ch.upper())]
            s = next((w for w in wished if w not in taken), None) or next(ch for ch in SIGNS if ch not in taken)
        else:
            s = next(a + b for a in SIGNS for b in SIGNS if a + b not in taken)
        taken.append(s)
        sign_of[c] = s
        legend.append(c)
    key = _toml_key
    text = f'format = "{LEVEL_FORMAT}"\nblock = {block}            # a block is this many voxels each way\n'
    text += f"size = [{sx}, {sy}, {sz}]    # cells: across (x), deep (y, 0 in front), up (z)\n"
    if front:
        text += 'view = "front"        # each layer the grid seen from the front, the front layer first, its top row first\n'
    else:
        text += 'view = "above"        # each layer the grid seen from above, the bottom layer first, its far row first\n'
    text += "\n[blocks]              # blocks/NAME.vvx, each what it is to the game\n"
    text += "".join(f'{key(n)} = "{k}"\n' for n, k in blocks)
    text += "\n[legend]              # a sign is a block as it lies: its name, or the name and an orientation (0 to 47)\n"
    for c in legend:
        name = blocks[c[0] - 1][0]
        text += f'{sign_of[c]} = "{name}"\n' if c[1] == 0 else f'{sign_of[c]} = {{ block = "{name}", orientation = {c[1]} }}\n'
    dots = "." * width

    def runs(items):
        out = []
        for item in items:
            if out and out[-1][0] == item:
                out[-1][1] += 1
            else:
                out.append([item, 1])
        return out

    layers = [["".join(sign_of[cells[p]] if cells.get(p) and cells[p][0] else dots for p in row) for row in layer] for layer in order]
    text += '\n[cells]\nlayers = """\n'
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
    text += '"""\n'
    for t in things:
        text += '\n[[thing]]\nkind = "%s"\nat = [%d, %d, %d]\n' % (t["kind"], *t["at"])
        text += _props_line(t.get("props"))
    for p in places:
        text += '\n[[place]]\nname = "%s"\nat = [%d, %d, %d]\n' % (p["name"], *p["at"])
        if p.get("facing"):
            text += "facing = [%d, %d]\n" % tuple(p["facing"])
    with open(path, "w") as f:
        f.write(text)


# --- terrain (src/terrain/): a level's [terrain] -----------------------------


TERRAIN_HEAD = "[terrain]             # heights in voxels, the far row first; shapes . SMOOTH ^ LINEAR # STEP\n"
MATERIAL_HEAD = "[[terrain.material]]  # the first that fits paints\n"
LAYER_HEAD = "[[terrain.layer]]     # from the bottom up, again and again upward\n"
SCATTER_HEAD = "[[terrain.scatter]]   # models over the land: as pieces (the editor's SCATTER) or voxels\n"


def _rust_num(v):
    """A number as the engine writes an f64 (`45`, `0.25`)."""
    return str(int(v)) if float(v) == int(v) else repr(float(v))


def _grid(rows, width):
    return '"""\n' + "".join(" ".join(f"{w:>{width}}" for w in row) + "\n" for row in rows) + '"""'


def read_terrain(path):
    """The [terrain] of a level as {"spacing", "at", "heights": [[...]],
    "shapes": [["." ...]], "terrace", "rough", "roughness": [[0..9 ...]] or
    None, "seed", "under", "layers": [{"thick", "hard", "color"}],
    "materials": [{...}]}: rows by y from the front (y 0 first; the file
    writes the far row first), each material's and layer's fields as the
    file gives them (a material's "colors" a list). None without one."""
    with open(path, "rb") as f:
        data = tomllib.load(f)
    t = data.get("terrain")
    if t is None:
        return None
    heights = [[int(w) for w in line.split()] for line in t["heights"].split("\n") if line.split()][::-1]
    if "shapes" in t:
        shapes = [line.split() for line in t["shapes"].split("\n") if line.split()][::-1]
    else:
        shapes = [["."] * len(row) for row in heights]
    roughness = None
    if "roughness" in t:
        roughness = [[int(w) for w in line.split()] for line in t["roughness"].split("\n") if line.split()][::-1]
    return {"spacing": t["spacing"], "at": t.get("at", [0, 0, 0]), "heights": heights, "shapes": shapes,
            "terrace": t.get("terrace"), "rough": t.get("rough"), "roughness": roughness,
            "dip": t.get("dip"), "dip_to": t.get("dip_to"), "fold": t.get("fold"), "fault": t.get("fault"),
            "seed": t.get("seed"), "under": t.get("under"), "layers": [dict(l) for l in t.get("layer", [])],
            "materials": [dict(m) for m in t.get("material", [])], "scatter": [dict(r) for r in t.get("scatter", [])]}


def terrain_text(t):
    """A terrain (as `read_terrain` gives it) as the engine writes it."""
    rows = t["heights"][::-1]
    width = max(len(str(h)) for row in rows for h in row)
    text = TERRAIN_HEAD + f"spacing = {t['spacing']}\n"
    if list(t.get("at") or [0, 0, 0]) != [0, 0, 0]:
        text += "at = [%d, %d, %d]\n" % tuple(t["at"])
    text += f"heights = {_grid(rows, width)}\n"
    shapes = t.get("shapes")
    if shapes and any(s != "." for row in shapes for s in row):
        text += f"shapes = {_grid(shapes[::-1], width)}\n"
    if t.get("terrace") is not None:
        text += f"terrace = {int(t['terrace'])}\n"
    if t.get("rough") is not None:
        text += f"rough = {_rust_num(t['rough'])}\n"
    if t.get("roughness") is not None:
        text += f"roughness = {_grid(t['roughness'][::-1], width)}\n"
    for key in ("dip", "dip_to", "fold", "fault"):
        if t.get(key) is not None:
            text += f"{key} = {_rust_num(t[key])}\n"
    if t.get("seed") is not None:
        text += f"seed = {t['seed']}\n"
    if t.get("under"):
        text += f'under = "{t["under"]}"\n'
    for l in t.get("layers") or []:
        text += "\n" + LAYER_HEAD + f"thick = {int(l['thick'])}\nhard = {_rust_num(l['hard'])}\n" + f'color = "{l["color"]}"\n'
    for m in t["materials"]:
        colors = m.get("colors")
        if colors:
            text += "\n" + MATERIAL_HEAD + "colors = [" + ", ".join(f'"{c}"' for c in colors) + "]\n"
        else:
            text += "\n" + MATERIAL_HEAD + f'color = "{m["color"]}"\n'
        for key in ("above", "below", "steeper", "flatter", "depth", "grain", "patch", "blend"):
            if m.get(key) is not None:
                text += f"{key} = {_rust_num(m[key]) if key in ('steeper', 'flatter', 'grain', 'blend') else int(m[key])}\n"
    for r in t.get("scatter") or []:
        models = r.get("models")
        if models and len(models) > 1:
            text += "\n" + SCATTER_HEAD + "models = [" + ", ".join(f'"{m}"' for m in models) + "]\n"
        else:
            text += "\n" + SCATTER_HEAD + f'model = "{(models or [r["model"]])[0]}"\n'
        text += f"density = {_rust_num(r['density'])}\n"
        for key in ("above", "below", "steeper", "flatter", "blend", "apart", "rotate", "sink", "as"):
            if r.get(key) is not None:
                v = r[key]
                if key in ("rotate", "as"):
                    text += f'{key} = "{v}"\n'
                elif key in ("steeper", "flatter", "blend"):
                    text += f"{key} = {_rust_num(v)}\n"
                else:
                    text += f"{key} = {int(v)}\n"
    return text


def write_terrain_level(path, size, terrain, things=()):
    """A level without blocks, `size` voxels, its land `terrain` (as
    `read_terrain` gives it) and `things` [{"name"?, "kind"? or "model",
    "at", "rotation"?, "solid"?, "shadow_only"?, "props"?}], as the engine
    writes it."""
    text = f'format = "{LEVEL_FORMAT}"\nsize = [%d, %d, %d]    # voxels: across (x), deep (y, 0 in front), up (z); no grid of blocks\n' % tuple(size)
    text += "\n" + terrain_text(terrain)
    for t in things:
        text += "\n[[thing]]\n"
        if t.get("name"):
            text += f'name = "{t["name"]}"\n'
        if t.get("kind"):
            text += f'kind = "{t["kind"]}"\n'
        if t.get("model"):
            text += f'model = "{t["model"]}"\n'
        text += "at = [%d, %d, %d]\n" % tuple(t["at"])
        if t.get("rotation") is not None:
            text += f"rotation = {_rust_num(t['rotation'])}\n"
        if t.get("solid"):
            text += "solid = true\n"
        if t.get("shadow_only"):
            text += "shadow_only = true\n"
        if t.get("scatter"):
            text += f"scatter = {int(t['scatter'])}\n"
        text += _props_line(t.get("props"))
    with open(path, "w") as f:
        f.write(text)


# --- maps (src/map.rs) ---------------------------------------------------------


MAP_FORMAT = "voxcade map 1"
MATERIAL_DEFAULTS = {"depth": 1, "pattern": 0.0, "grain": 1.0, "band": 4, "wobble": 0.0}


def read_map(path):
    """A map as {"size", "ground", "prefabs", "put", "place"}, as
    `write_map` takes them."""
    with open(path, "rb") as f:
        data = tomllib.load(f)
    if data.get("format") != MAP_FORMAT:
        raise SystemExit(f"{path}: not a map of format 1 (its first line: format = \"{MAP_FORMAT}\")")
    return {"size": data["size"], "ground": data["ground"], "prefabs": data.get("prefabs", {}), "put": data.get("put", []), "place": data.get("place", [])}


def write_map(path, size, ground, prefabs, put, places=()):
    """A map: `size` voxels, `ground` {"height", "material"?, "bedrock"?,
    "materials": [{"key"?, "surface", "depth", ...}]}, `prefabs` {name:
    file or {"file", "soft"}}, `put` [{"prefab" or "thing", "at",
    "rotation"?}] in order (rotation: degrees clockwise seen from above, in
    quarter turns), `places` [{"name", "at", "facing"?}]."""
    q = lambda s: '"%s"' % s
    lst = lambda xs: "[" + ", ".join(q(x) for x in xs) + "]"
    num = lambda v: ("%d" % v) if float(v).is_integer() else repr(float(v))
    text = f'format = "{MAP_FORMAT}"\nsize = [{size[0]}, {size[1]}, {size[2]}]    # voxels: across (x), north (y), up (z)\n\n'
    text += "# The ground: each column as high as the height map's grey (its top row\n# the north edge), of the material whose key the material map's colour is.\n[ground]\n"
    for k in ("height", "material", "bedrock"):
        if ground.get(k):
            text += f"{k} = {q(ground[k])}\n"
    for m in ground["materials"]:
        text += "\n[[ground.materials]]\n"
        if m.get("key"):
            text += f"key = {q(m['key'])}\n"
        if m.get("surface"):
            text += f"surface = {lst(m['surface'])}\n"
        for k in ("depth", "pattern", "grain"):
            if k in m and m[k] != MATERIAL_DEFAULTS[k]:
                text += f"{k} = {num(m[k])}\n"
        if m.get("strata"):
            text += f"strata = {lst(m['strata'])}\n"
        for k in ("band", "wobble"):
            if k in m and m[k] != MATERIAL_DEFAULTS[k]:
                text += f"{k} = {num(m[k])}\n"
    text += "\n# The prefabs by name: each its file; a soft one (grass) is passed through.\n[prefabs]\n"
    for name in sorted(prefabs):
        p = prefabs[name]
        if isinstance(p, dict) and p.get("soft"):
            text += f"{name} = {{ file = {q(p['file'])}, soft = true }}\n"
        else:
            text += f"{name} = {q(p['file'] if isinstance(p, dict) else p)}\n"
    text += "\n# What stands on the map, in order: a prefab (rotation: degrees clockwise\n# seen from above, in quarter turns) or a thing of a kind; at x, y on the\n# ground, or x, y, z.\n"
    for e in put:
        text += "[[put]]\n"
        text += f"prefab = {q(e['prefab'])}\n" if "prefab" in e else f"thing = {q(e['thing'])}\n"
        text += "at = [" + ", ".join(str(v) for v in e["at"]) + "]\n"
        if e.get("rotation", 0) % 360:
            text += f"rotation = {e['rotation'] % 360}\n"
        text += "\n"
    for p in places:
        text += '[[place]]\nname = "%s"\nat = [%d, %d, %d]\n' % (p["name"], *p["at"])
        if p.get("facing"):
            text += "facing = [%d, %d]\n" % tuple(p["facing"])
        text += "\n"
    with open(path, "w") as f:
        f.write(text.rstrip() + "\n")

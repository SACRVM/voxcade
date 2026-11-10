#!/usr/bin/env python3
"""Looks at and tidies a Voxcade game's voxel blocks (blocks/NAME.vox) from
the command line; Python and Pillow only.

usage:
  blocks.py used GAME                    the blocks the level lists: kind, cells using them, file
  blocks.py info GAME NAME [NAME ...]    a block: voxels, colours, its relief seen from the front
  blocks.py colours GAME [--near N]      every colour of every block, and pairs closer than N
                                         (the sum of the channels' differences, default 6)
  blocks.py merge GAME R,G,B R,G,B       the first colour becomes the second in every block
                                         (their palette slots folded into one) and in the
                                         tileset pictures (blocks/tiles.png, tiled/*.png)

GAME is a game folder (games/voxkeep). The relief map shows, for each
column of the block's front, how many voxels back the first voxel is
(0 on the front edge, . empty through) and a letter for its colour.
"""
import glob
import json
import os
import struct
import sys


def chunks(b, i, end, out):
    while i < end:
        cid = bytes(b[i:i + 4])
        n, m = struct.unpack('<II', b[i + 4:i + 12])
        out.append((cid, i + 12, n))
        if m:
            chunks(b, i + 12 + n, i + 12 + n + m, out)
        i += 12 + n + m
    return out


def load(path):
    """(size, {(x, y, z): (r, g, b)}) of a block file."""
    b = open(path, 'rb').read()
    size, vox, pal, idx = None, {}, None, []
    for cid, at, n in chunks(b, 8, len(b), []):
        if cid == b'SIZE':
            size = struct.unpack('<III', b[at:at + 12])
        elif cid == b'XYZI':
            c = struct.unpack('<I', b[at:at + 4])[0]
            idx += [tuple(b[at + 4 + 4 * k:at + 8 + 4 * k]) for k in range(c)]
        elif cid == b'RGBA':
            pal = [tuple(b[at + 4 * k:at + 4 * k + 3]) for k in range(256)]
    for x, y, z, ci in idx:
        vox[(x, y, z)] = pal[ci - 1]
    return size, vox


def block_files(game):
    return sorted(glob.glob(os.path.join(game, 'blocks', '*.vox')))


def name_of(path):
    return os.path.splitext(os.path.basename(path))[0]


def rgb(text):
    parts = [int(v) for v in text.split(',')]
    if len(parts) != 3 or not all(0 <= v <= 255 for v in parts):
        raise SystemExit(f"{text}: a colour is R,G,B, each 0 to 255")
    return tuple(parts)


def used(game):
    level = json.load(open(os.path.join(game, 'level.json')))
    count = [0] * (len(level['blocks']) + 1)
    for run, block, _turn in level['cells']:
        count[block] += run
    print(f"{level['size'][0]} by {level['size'][1]} by {level['size'][2]} cells of {level['block']} voxels; "
          f"{len(level['blocks'])} blocks, {len(level.get('things', []))} things")
    for i, b in enumerate(level['blocks']):
        path = os.path.join(game, 'blocks', b['name'] + '.vox')
        state = '' if os.path.exists(path) else '  (no file!)'
        print(f"  {b['name']:>6}  {b['kind']:<7} {count[i + 1]:>5} cells{state}")


def info(game, names):
    for name in names:
        size, vox = load(os.path.join(game, 'blocks', name + '.vox'))
        n = size[0]
        cols = sorted(set(vox.values()), key=lambda c: -sum(1 for v in vox.values() if v == c))
        letter = {c: chr(ord('a') + i) for i, c in enumerate(cols)}
        print(f"block {name}: {size[0]} by {size[1]} by {size[2]}, {len(vox)} voxels, {len(cols)} colours")
        for c in cols:
            print(f"  {letter[c]} {c} {sum(1 for v in vox.values() if v == c)} voxels")
        print("  relief from the front (depth of the first voxel, its colour), top row first:")
        for z in range(n - 1, -1, -1):
            row = ''
            for x in range(n):
                ys = [y for y in range(n) if (x, y, z) in vox]
                row += f"{min(ys) if min(ys) < 10 else '+'}{letter[vox[(x, min(ys), z)]]}" if ys else ' .'
            print('   ' + row)


def colours(game, near):
    where = {}
    for path in block_files(game):
        _, vox = load(path)
        for c in vox.values():
            where.setdefault(c, {}).setdefault(name_of(path), 0)
            where[c][name_of(path)] += 1
    print(f"{len(where)} colours in {len(block_files(game))} blocks (the level holds at most 255,"
          f" a colour once for each way it holds: solid, from above, passed through)")
    for c in sorted(where, key=lambda c: -sum(where[c].values())):
        blocks = ', '.join(sorted(where[c], key=lambda b: -where[c][b])[:8])
        print(f"  {c} {sum(where[c].values()):>6} voxels in {len(where[c])} blocks: {blocks}")
    cs = sorted(where)
    pairs = [(a, b) for i, a in enumerate(cs) for b in cs[i + 1:] if sum(abs(p - q) for p, q in zip(a, b)) <= near]
    print(f"pairs closer than {near + 1}: " + ('; '.join(f"{a} {b}" for a, b in pairs) if pairs else 'none'))


def merge(game, old, new):
    from PIL import Image
    changed = []
    for path in block_files(game):
        b = bytearray(open(path, 'rb').read())
        found = chunks(b, 8, len(b), [])
        rgba = next(at for cid, at, n in found if cid == b'RGBA')
        slots = [k for k in range(256) if b[rgba + 4 * k + 3] and tuple(b[rgba + 4 * k:rgba + 4 * k + 3]) in (old, new)]
        if not any(tuple(b[rgba + 4 * k:rgba + 4 * k + 3]) == old for k in slots):
            continue
        keep = slots[0]
        b[rgba + 4 * keep:rgba + 4 * keep + 3] = bytes(new)
        moved = 0
        for cid, at, n in found:
            if cid != b'XYZI':
                continue
            for k in range(struct.unpack('<I', b[at:at + 4])[0]):
                o = at + 4 + 4 * k + 3
                if b[o] - 1 in slots and b[o] - 1 != keep:
                    b[o] = keep + 1
                    moved += 1
        for k in slots[1:]:
            b[rgba + 4 * k:rgba + 4 * k + 4] = bytes(4)
        open(path, 'wb').write(b)
        changed.append(f"{name_of(path)} ({moved} voxels moved)" if moved else name_of(path))
    print("blocks: " + (', '.join(changed) if changed else 'none had it'))
    for img in [os.path.join(game, 'blocks', 'tiles.png')] + sorted(glob.glob(os.path.join(game, 'tiled', '*.png'))):
        if not os.path.exists(img):
            continue
        im = Image.open(img).convert('RGBA')
        px, n = im.load(), 0
        for y in range(im.height):
            for x in range(im.width):
                r, g, bl, a = px[x, y]
                if (r, g, bl) == old:
                    px[x, y] = (*new, a)
                    n += 1
        if n:
            im.save(img)
        print(f"{img}: {n} pixels")
    print("a running editor still holds the old colour: restart it before saving a block;"
          " the game's replays: voxcade check GAME --refit")


def main(args):
    if len(args) < 2:
        raise SystemExit(__doc__)
    what, game = args[0], args[1]
    if what == 'used':
        used(game)
    elif what == 'info' and len(args) > 2:
        info(game, args[2:])
    elif what == 'colours':
        near = int(args[args.index('--near') + 1]) if '--near' in args else 6
        colours(game, near)
    elif what == 'merge' and len(args) == 4:
        merge(game, rgb(args[2]), rgb(args[3]))
    else:
        raise SystemExit(__doc__)


if __name__ == '__main__':
    main(sys.argv[1:])

#!/usr/bin/env python3
"""Looks at and tidies a Voxcade game's voxel blocks (blocks/NAME.vvx) from
the command line; Python and Pillow only.

usage:
  blocks.py used GAME                    the blocks the level lists: kind, cells using them, file
  blocks.py info GAME NAME [NAME ...]    a block: voxels, colours, its relief seen from the front
  blocks.py colours GAME [--near N]      every colour of every block, and pairs closer than N
                                         (the sum of the channels' differences, default 6)
  blocks.py merge GAME R,G,B R,G,B       the first colour becomes the second in every block
                                         (their palette signs folded into one) and in the
                                         tileset pictures (blocks/tiles.png, tiled/*.png)

GAME is a game folder (games/voxkeep). The relief map shows, for each
column of the block's front, how many voxels back the first voxel is
(0 on the front edge, . empty through) and a letter for its colour.
"""
import glob
import json
import os
import sys


# Voxcade's model files, read and written by the repository's tools/vvx.py.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..', '..', 'tools'))
import vvx  # noqa: E402


def load(path):
    """(size, {(x, y, z): (r, g, b)}) of a block file."""
    m = vvx.read(path)
    cells = m['frames'][0] if m['frames'] else {}
    return tuple(m['size']), {p: m['colours'][s] for p, s in cells.items()}


def block_files(game):
    return sorted(glob.glob(os.path.join(game, 'blocks', '*.vvx')))


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
        path = os.path.join(game, 'blocks', b['name'] + '.vvx')
        state = '' if os.path.exists(path) else '  (no file!)'
        print(f"  {b['name']:>6}  {b['kind']:<7} {count[i + 1]:>5} cells{state}")


def info(game, names):
    for name in names:
        size, vox = load(os.path.join(game, 'blocks', name + '.vvx'))
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
        m = vvx.read(path)
        olds = [k for k, c in m['colours'].items() if c == old]
        if not olds:
            continue
        # The sign the new colour has (plain), else the old sign takes it.
        keep = next((k for k, c in m['colours'].items() if c == new and m['palette'][k].startswith('"')), None)
        moved = 0
        for k in olds:
            if keep is None:
                keep = k
                m['palette'][k] = '"#%02x%02x%02x"' % new
                continue
            for cells in m['frames']:
                for p, sign in cells.items():
                    if sign == k:
                        cells[p] = keep
                        moved += 1
        vvx.write_model(path, m, kept=m['palette'])
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

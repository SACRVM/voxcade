---
name: voxcade-blocks
description: >
  Work with Voxcade's levels of voxel blocks (docs/tiles.md): block files
  (blocks/NAME.vvx, N by N by N), levels/NAME.toml (the grid of cells, the
  blocks' kinds, the things), bringing a Tiled map in once (voxcade
  import), the editor (voxcade edit: blocks and things, tools, keys, what
  it cannot do yet),
  the user's rules and style for shaping blocks, colours and the 255
  limit, how relief reads in the game and touches collisions, and a
  helper to inspect blocks, list colours and merge them everywhere. Use
  this whenever blocks, tiles, Kacheln, a level of blocks, its colours,
  the block editor, a level's .toml or a game's level of blocks come up, before changing
  any block file, and when the user asks how a block looks or what to do
  about one.
---

# Levels of voxel blocks

A level is a 3D grid of cells (x across, y in depth with row 0 at the
front, z up); each cell holds a block in one of 48 turns. A block is a
small voxel model as big as the game's tiles each way (Voxkeep: 10 by 10
by 10; a level from a 2D map: a pixel a voxel). Loading stamps the blocks into the world's
voxels, so physics, destruction and meshing treat them like any voxels.
The concept and its decisions: `docs/tiles.md`.

## Shaped by hand only

The user shapes blocks themself, voxel by voxel, in the editor. Tools that
shaped them on their own (an automatic relief, rounding, primitive models)
were tried and rejected as fake 3D. So:

- Never change a block's shape or colours on your own initiative. Look,
  describe, suggest; edit only what the user asks for, exactly.
- Mechanical edits on request are fine: merging two colours everywhere,
  turning or copying a block, filling a stray hole, clearing a voxel the
  user points at. Say what you changed, block by block.
- The user may be working in the editor while you look: block files
  change under you (compare their times), and a running editor keeps its
  own copy in memory and writes it back on the next save. Before you
  change a block file, ask them to close the editor or restart it after.

## Files

- `blocks/NAME.vvx`: one model of Voxcade's own (text, `src/vvx.rs`; the
  models skill has the format), one frame exactly N voxels each way
  (another size is an error naming the file); colours only, a material
  counts for models, not blocks. A MagicaVoxel block is brought in with
  `voxcade import GAME FILE.vox --as blocks/NAME.vvx`. Imported blocks are
  named by the tile's id as the Tiled map stores it (Tiled's own window
  shows that id less one); new ones are numbered on after the tileset.
  `blocks/tiles.png` is the tileset as whole tiles, for new blocks.
- `levels/NAME.toml` (one level: `level.toml`), text ("voxcade level 1",
  `docs/tiles.md` has an example): `block` (N), `size` (cells across,
  deep, up), `view` (`"front"`: each layer the grid seen from the front,
  the front layer first, its top row first, a side view's room as its
  picture; `"above"`: the bottom layer first, its far row first, a farm
  as its map), `[blocks]` (name = kind, in the editor's order),
  `[legend]` (a sign a block as it lies: `s = "stone"`, turned `S = {
  block = "stone", orientation = 17 }`, orientation 0..47), `[cells]` with `layers`
  (rows of signs, `.` empty, ` *N` and `*N` repeats as in a `.vvx`),
  `[[thing]]` (`kind`, `at = [x, y, z]` in voxels from the grid's low
  corner, y = N/2 the middle of the front row), `[[place]]` (`name`,
  `at`, `facing`: where a player arrives, `world:go_to(level, place)`).
  A game of several levels keeps them in `levels/`, in name order. A
  changed cell is a changed character: change one by hand by its sign,
  and keep the signs (the editor does).
- `conf.lua`: `c.world.blocks = { file = "levels/01_hall.toml", margin, below,
  backdrops = { ... } }` (room left and right, open space under the grid
  to fall out of, pictures far behind).
- In Rust: `src/blocks.rs` (`Block`: load, save at once, drawn out of a
  tile, turned; `Turn` with `TURN`, `ROLL`, `MIRROR` and Tiled's flips),
  `src/grid/` (`GridLevel`: read, save, build; `stamp`; `CellKinds`).

## Kinds: what a block is to the game

| Kind | Its voxels | Rules |
|---|---|---|
| `solid` | block everything | — |
| `oneway` | the top voxel of each column (as the block lies, after its turn) holds from above; the rest is passed through | a 2D map's plank, for a game that wants it; ours are solid (below) |
| `ladder` | passed through | climbed (`World::cells`) |
| `hazard` | solid | what it does is the game's code (`thing:in_block("hazard")`, `level:block`) |
| `decor` | passed through | — |

Planks are `solid` (the user, 2026-10-23: "die Bretter SOLLEN sperren";
in 3D nothing is jumped through from below as in a 2D game). A way up is
something that works in 3D: a ladder (its top holds from above), a
double jump beside the edge, a lift, a crate. Voxfox, brought in from the
Sunny Land pack's Tiled map, was removed for it: its level needed
`oneway` planks everywhere. The kind stays in the engine for a game that
wants it (the user: "wer es will"), and `voxcade import` still makes a
tileset's one-way tiles `oneway`; our own games use solid planks.

The kind belongs to the block in the level's list; the editor's kind word
(SOLID, ...) switches it. Relief on a walkable top is collision: a voxel
sticking up across the lane is a step (bodies climb `c.physics.step`
voxels, figures stand in the front row's middle).
Keep tops flat where bodies walk, or bumps under the step height.

## Colours

- A world holds at most 255 colours, and a colour counts once for each
  way its voxels hold (solid, from above, passed through). Light and
  shadow come from the engine, so blocks need only their materials'
  colours, no painted shading.
- Colours a hair apart waste places and are hard to tell; the tileset
  itself had two greens one step apart, merged by hand.
  `scripts/blocks.py colours GAME` lists them and pairs that are close.

## How the user has shaped blocks so far

Seen in Voxfox's blocks (27, 29, 77; the game since removed), to keep new suggestions consistent, never to
apply by yourself: the front picture stays the tile's; grass on the front
edge; light earth and stones one voxel back, the middle brown two, the
dark three, so the stones stand out in relief; the sides and top in calm
bands (grass, earth, a dark band at the bottom in the cave fill's colour,
so blocks stacked below continue seamlessly); a stone facade one voxel
deep in front of dark fill for wall tiles.

The game's camera looks almost straight on (Voxkeep: 10° down): relief of
one voxel reads there as texture, two or three as form. Judge a block in
the game (`shot`), not only in the editor's close orbit.

## Tools

```bash
voxcade import games/NAME games/NAME/tiled/map.json   # once: a Tiled map as blocks and level.toml
voxcade edit games/NAME                               # the editor, in a window (the user's)
voxcade edit games/NAME --block 27 --view -35,12 --size 900x700 --shot shots/b27.png   # to look
voxcade edit games/NAME --things --size 1600x900 --shot shots/things.png             # the things
.claude/skills/voxcade-blocks/scripts/blocks.py used GAME            # the blocks the level uses
.claude/skills/voxcade-blocks/scripts/blocks.py info GAME 27 29      # colours and relief, from the front
.claude/skills/voxcade-blocks/scripts/blocks.py colours GAME         # all colours, close pairs
.claude/skills/voxcade-blocks/scripts/blocks.py merge GAME 180,222,92 180,222,91   # everywhere
```

`info`'s relief map gives, for each column of the front, how many voxels
back the first voxel lies and a letter for its colour: the quickest way
to read what the user built without a picture.

## The editor (src/editor)

Two levels in one window, every change saved at once; a running game
reloads the level. In the level: PLACE, REMOVE, REPLACE, PICK (A, E, P,
I), TURN, ROLL, MIRROR (T, O, M) for the held block, NEW (N: a tile of
the tileset drawn out, or empty), CLONE (C), DELETE (D: its cells empty,
its file gone; UNDO brings both back). In a block (double click, or the
held block clicked again): ADD, ERASE, PAINT, PICK, DEEP PAINT (N: the
voxel and every one behind it), TURN (the block itself), the block's own
colours on 1..9 and 0, any colour from the picker under COLOUR, BACK (B,
Escape). The panels are the IDE's look (rows with their keys at the end,
the kind a list, pictures along the bottom). Mouse: left the
tool, right drag turns, middle or shift and right drag moves, the wheel
zooms; Ctrl+Z, Ctrl+Y. The level's things (THINGS, H; BLOCKS, K back):
PLACE, REMOVE, MOVE, PICK (A, E, M, I), the kinds of `c.things` along the
bottom; a thing stands on the top the mouse points at, or before the
wall, in the middle of the row's depth.

The same editor shapes a game's models (`voxcade edit GAME --model
hero.vvx`, or EDIT in the IDE; a MagicaVoxel `.vox` it does not shape:
bring it in first): any size, frame by frame (PREV, NEXT or the arrow
keys, COPY, DELETE), the block's tools and colours; ADD beyond the box
grows it, every frame's, and TRIM cuts it to the voxels, both about the
model's pivot, so it stands where it stood (growing to one side may add
an empty column on the other; under the floor it raises the model); TURN
turns all frames. A colour's material (GLASS, METAL, ROUGH, GLOW) sits
under a quiet word beside the colour; the signs of the file stay;
saved at once, each change a step. Figures face +y in their files; the
editor (and the IDE's picture) shows a model as the game does as a run
starts (`view::face_of`, checked against a game shot): the hero looking
right (its +x side), the other figures looking left at him (−x), props
turned to the camera (+y), a block as it lies in the level (y 0); the
level's things stand so too. Its frames play on the panel (PLAY,
PAUSE).

Not yet: growing the grid left, forwards or down, naming blocks. A thing
stands exactly where the level's `.toml` places it; the engine moves none of them
back or forth (a prop behind the lane stands there because it is placed
there).

## After changing blocks

Block files and the level's `.toml` count for the replays' fingerprint: run
`voxcade check games/NAME`. If the change only alters looks, `--refit`
confirms the game plays the same and takes the new fingerprint; if a
replay now plays differently (a block changed where bodies walk), the
level changed: `--update`, and tell the user. The voxcade-verify skill
has the whole pass.

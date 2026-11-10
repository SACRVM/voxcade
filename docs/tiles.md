# Voxel tiles: blocks in a grid

The user's concept (2026-10-16), written down before anything is built.
Levels are made of voxel blocks laid in a 3D grid, like Minecraft's, but a
block is not a cube with six pictures on it: it is a small voxel model,
shaped and coloured by hand, voxel by voxel.

## A block

- A block is N by N by N voxels (Sunny Land: 16). A voxel is a pixel: a
  16 by 16 tile becomes a 16 by 16 by 16 block.
- It starts from a 2D tile. Brought into the block editor, every layer of
  the block, front to back, is the tile's picture: each pixel becomes N
  voxels deep. That is only the start.
- Then it is worked on by hand in the block editor: voxels recoloured,
  deleted, added. Stones that stand out, joints cut in, a round post, a
  grass edge hanging over: all made by hand. Nothing is shaped by the
  engine or a tool on its own (an automatic relief was tried and
  rejected).
- Colours: a world has at most 255 (the palette). That is enough because
  the engine does light and shadow: a block needs no painted shading,
  only its materials' colours.
- What a block is to the game is set once for the block: solid, held from
  above only (a plank), climbed (a ladder), hurting (spikes), or nothing
  (decoration a body passes through).

## A level

- A 3D grid of cells, each N voxels each way: empty, or a block.
- Objects are placed freely besides the grid, anywhere in voxels:
  figures, enemies, pickups, props (a house, trees). The grid is for the
  level's build: walls, floors, ground, caves.
- The engine needs the grid too: a level is a list of block numbers per
  cell instead of millions of voxels (small files, quick to load), a
  block changed shows everywhere it is used (hot reload), and the rules
  read a block's kind once (solid, one-way, ladder, hazard). When the
  level is loaded the blocks are stamped into the world's voxels, so
  everything else (destruction, physics, meshing) stays as it is.
- A 2D Tiled map comes in as a start: each tile its block (the one made
  for it, or one drawn out from its picture), the map one row of cells
  deep or several.

## The block editor

Pulled forward from phase 2 (`ide.md`): the sooner it exists, the sooner
3D tiles can be made. It must not overwhelm (the user on vengi: too much,
hard on the eyes); calm, few tools in view, large and readable.

- One block in the middle, turned and looked at from any side.
- Import a 2D tile (drawn out N deep), paint, erase, add, pick a colour,
  undo; the block's colours as its palette.
- It saves by itself, like the rest of the IDE; the running game shows
  the change at once.

## Decided

- A block is as big as the game's 2D tiles, with the third dimension as
  deep: Sunny Land's 16 by 16 tiles make 16 by 16 by 16 blocks, a pixel
  a voxel. A level brought in from a map is at the pictures' scale
  (Voxfox's was, a game since removed).
- One file a block, in a folder (`blocks/NAME.vox`): the editor saves the
  block being worked on, the game reloads that block.
- A cell may hold its block turned in quarter turns and mirrored (a
  corner used left and right, an edge at the top and the bottom); the
  Sunny Land map already flips and turns 120 of its tiles.

## Built (2026-10-16)

- **Blocks** (`src/blocks.rs`): `blocks/NAME.vox`, a plain `.vox` file of
  exactly N by N by N voxels (MagicaVoxel opens it), written at once
  through a hidden file beside it so a running game never reads half a
  block. A cell lies its block in any of 48 turns: quarter turns about
  the three axes, mirrored or not (`Turn`); Tiled's flips are among them.
- **Levels** (`src/grid.rs`, `c.world.blocks = { file, margin, below,
  backdrops }`): `level.json` holds the block size, the grid's size in
  cells, the blocks by name with what each is to the game (`solid`,
  `oneway`, `ladder`, `hazard`, `decor`), the cells as runs (block, turn)
  and the things placed freely, in voxels. Loading stamps the blocks into
  the world's voxels; of a `oneway` block the top voxel of each column
  holds, as it lies. The kit climbs ladders by cell (`World::cells`);
  what a hazard does is the game's code (`thing:in_block("hazard")`). A block saved, a new one too, rebuilds the running
  game's level.
- **Tiled is only a way in** (`voxcade import <game> <map.json>`, once):
  Tiled draws in 2D, the level lives on in 3D. Every tile the map uses
  becomes a block drawn out of its picture (unless the game has made it
  already), named by the id the map stores (Tiled's own window shows that
  id less one); each tile layer a row of cells, the top layer in front; a
  map of one layer splits into what holds (front) and decoration (a row
  behind); a tile's class in the tileset is its block's kind; objects
  become things. The tileset stays as `blocks/tiles.png` for new blocks.
- **The editor** (`voxcade edit <game>`, `src/editor/`): one window, two
  levels, the game's own light and shadows, seen from any side.
  - Level: PLACE, REMOVE, REPLACE, PICK blocks in the cells; the held
    block shown with its kind (click: the next kind), TURN (about the
    upright), ROLL (about the line of sight), MIRROR; NEW draws a block out
    of a tile of the tileset, or starts an empty one; CLONE copies the
    held block under a new number, DELETE takes it out of the level (its
    cells empty, its file gone; UNDO brings both back). Placing past the
    grid's right, back or top grows it.
  - The level's things (BLOCKS and THINGS at the top switch, K and H):
    PLACE puts the held kind down where the mouse points, standing on a
    top or floating before a wall, in the middle of the row's depth (the
    lane); REMOVE takes one away, MOVE drags one elsewhere, PICK takes up
    its kind (A, E, M, I). The kinds of `c.things` lie along the bottom
    as small pictures (a kind without a model: a pale column as wide and
    tall as its body), the held one see-through where it would stand,
    the one under the mouse tinted by the tool.
  - Block (a double click on a cell, or the held block's picture clicked
    again): ADD, ERASE, PAINT, PICK voxels, DEEP PAINT (the voxel and
    every one behind it, through the block); TURN turns the block itself
    a quarter about the upright (a side shaped as the front brought
    round); CLONE (the copy opens), DELETE; the block's colours big along
    the bottom, the level's other colours small, any colour from the
    picker under COLOUR; BACK (or Escape) returns.
  - The mouse: left works the tool (a stroke works on the surface it began
    on: one layer added or taken off), right drag turns, middle or shift
    and right drag moves, the wheel zooms (over the bar: scrolls).
    Ctrl+Z, Ctrl+Y. Every word has a key, shown at its end (the panels
    are the IDE's look, egui) (a tool the same on both levels: A, E, P, I, and N for DEEP PAINT;
    U and R undo and redo, T, O, M turn, C clones, D deletes, N new, B
    back; among the things M moves); the block's own colours (the big fields) are on 1 to 9 and 0,
    their numbers on them. Every change
    is saved at once.
  - `voxcade edit <game> [--block NAME | --things] --shot FILE [--view
    YAW,PITCH]` draws it without a window.
- **Not yet**: growing the grid left, forwards or down; blocks with names
  of their own.


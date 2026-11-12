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
- One file a block, in a folder (`blocks/NAME.vvx`): the editor saves the
  block being worked on, the game reloads that block.
- A cell may hold its block turned in quarter turns and mirrored (a
  corner used left and right, an edge at the top and the bottom); the
  Sunny Land map already flips and turns 120 of its tiles.

## A level as text (2026-11-12)

A level is text a person reads and git diffs cell by cell (the owner:
everything in a game is code or text):

```toml
format = "voxcade level 1"
block = 10            # a block is this many voxels each way
size = [24, 2, 13]    # cells: across (x), deep (y, 0 in front), up (z)
view = "front"        # each layer the grid seen from the front, the front layer first, its top row first

[blocks]              # blocks/NAME.vvx, each what it is to the game
wall = "decor"
stone = "solid"
spikes = "hazard"

[legend]              # a sign is a block as it lies: its name, or the name and a turn (0 to 47)
s = "stone"
S = "spikes"
w = "wall"
t = { block = "stone", turn = 17 }

[cells]
layers = """
ssssssssssssssssssssssss
s......................s *4
s..........SS..........s
ssssssssssssssssssssssss

wwwwwwwwwwwwwwwwwwwwwwww *13
"""

[[thing]]
kind = "gem"
at = [45, 5, 12]      # where its feet stand, in voxels from the grid's low corner

[[place]]
name = "door"
at = [25, 8, 70]
facing = [0, -1]
```

- **Which way the layers read** is said in the file, the same for every
  game whatever its camera: `view = "front"` (each layer the grid seen
  from the front, the front layer first, in a layer the top row first:
  Voxkeep's room reads as the picture on the screen) or `view = "above"`
  (each layer seen from above, the bottom layer first, the far row first,
  as a model's `.vvx`: BunnyFarm's farm reads as its map). x runs left to
  right in both. A new level reads from the front when it is taller than
  deep, else from above; the editor keeps the file's view.
- **Signs**: one letter or digit a pair of block and turn (62), two each
  when a level needs more (`..` empty); a new sign takes a letter of its
  block's name where it can (`s` for stone, `S` the next stone or the
  next block with an s). The editor keeps every sign the file had, so a
  changed cell is a changed character in git.
- **Repeats** as in a `.vvx`: a row followed by ` *N` stands N times in
  all, a line `*N` after a layer's rows the layer N times.
- `[blocks]` keeps its order (the editor's list of blocks); things,
  places and areas come in their order (things are made in it).
- **Pieces, lights and areas** (2026-11-12): a `[[thing]]` with a
  `model` instead of a kind is a piece, a model set down by its pivot
  (`at`, anywhere: on a wall, in the air), `facing = "east"` (or `turn =
  30` where the game turns things freely, `c.camera.turns = "free"`),
  perhaps with a `light = { at, radius, color }` (or `lights = [ ... ]`
  for several) and `solid = true` (its voxels hold bodies up, quarter
  turns only); one with only a light is a light alone. A level may have
  its own daylight, `[light]` with `c.light`'s words. A `[[thing]]` may have a `name` (one thing's in the
  level). `[[area]]` is a named box (`name`, `at` its low corner,
  `size`). A level without `block` has no grid: `size` is its voxels and
  it holds only what is set down (a stage of models). The editor opens
  such a level among its pieces (`src/editor/room.rs`; a level of blocks
  has PIECES beside BLOCKS and THINGS) in three zones (`stage.rs`): a
  bar along the top (the level, + ADD with MODEL…, LIGHT, PLACE and AREA,
  UNDO, REDO; GAME VIEW, LIGHTS and LEVEL on the right), the outliner on
  the left (what is set down in three groups that fold, a field to find
  by name when there are many, one scroll), the inspector on the right
  (the chosen one as short rows: its name to type over, its model with
  EDIT, POSITION and TURN as three fields each, their letters in the
  handles' colours, SOLID, LIGHTS with a row each; LEVEL: size and
  daylight). In the view the chosen one has handles (`handles.rs`):
  arrows along X, Y and Z (red, green, blue) move it by whole voxels,
  rings turn it (about its own right side, its forward and the upright:
  each ring one of pitch, roll and turn; quarters on the grid, freely
  otherwise, a quarter within 5 degrees taken), an area's faces size it,
  a light of a piece chosen has arrows along the model's own axes; the
  thing itself is dragged over the floor at its height. Beside it COPY,
  REMOVE, EDIT (its model) and LIGHT (on and off); a right click on it,
  or on its row, offers those and more (RENAME, TURN A QUARTER, LAY
  DOWN, SOLID), on the floor what can be added there; a double click
  opens a piece's model. Keys as before (arrows nudge, Page Up and Down
  raise, T turns, Delete removes, F2 names, Escape lets go and puts a
  drag back); the view's own buttons in its corner turn it and bring it
  nearer, for whoever does not know the mouse's ways yet. Every step is saved at once over the file's own text
  (`grid::text::rewrite`): what did not change stays to the character,
  comments too. A level turned off the grid in a game on the grid asks
  first: ALIGN ALL or DON'T OPEN. `grid::text` has the whole format.

## Built (2026-10-16)

- **Blocks** (`src/blocks.rs`): `blocks/NAME.vvx`, a model of Voxcade's
  own (text, `src/vvx.rs`; a `.vox` until 0.6.0, `voxcade upgrade` brings
  them over) of exactly N by N by N voxels, written at once
  through a hidden file beside it so a running game never reads half a
  block. A cell lies its block in any of 48 turns: quarter turns about
  the three axes, mirrored or not (`Turn`); Tiled's flips are among them.
- **Levels** (`src/grid.rs`, `c.world.blocks = { file, margin, below,
  backdrops }`): `levels/NAME.toml` (or `level.toml`; JSON until 0.6.0,
  `voxcade upgrade` brings it over) holds the block size, the grid's size
  in cells, the blocks by name with what each is to the game (`solid`,
  `oneway`, `ladder`, `hazard`, `decor`), the cells as layers of rows of
  signs and the things placed freely, in voxels. Loading stamps the blocks into
  the world's voxels; of a `oneway` block the top voxel of each column
  holds, as it lies. The kit climbs ladders by cell (`World::cells`);
  what a hazard does is the game's code (`thing:in_block("hazard")`). A block saved, a new one too, rebuilds the running
  game's level.
- **Tiled is only a way in** (`voxcade import <game> <tiled-map.json>`, once; it writes `level.toml`, `--file levels/NAME.toml`):
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
    Shift held is PICK, whatever the tool (the tool before comes back
    when it is let go). Ctrl+Z, Ctrl+Y. Every word has a key, shown at its end (the panels
    are the IDE's look, egui) (a tool the same on both levels: A, E, P, I, and N for DEEP PAINT;
    U and R undo and redo, T, O, M turn, C clones, D deletes, N new, B
    back; among the things M moves); the block's own colours (the big fields) are on 1 to 9 and 0,
    their numbers on them. Every change
    is saved at once.
  - `voxcade edit <game> [--block NAME | --things] --shot FILE [--view
    YAW,PITCH]` draws it without a window.
- **Not yet**: growing the grid left, forwards or down; blocks with names
  of their own.


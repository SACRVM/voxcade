# Voxel tiles: blocks in a grid

The user's concept (2026-10-16), written down before anything is built.
Levels are made of voxel blocks laid in a 3D grid, like Minecraft's, but a
block is not a cube with six pictures on it: it is a small voxel model,
shaped and colored by hand, voxel by voxel.

## A block

- A block is N by N by N voxels (Sunny Land: 16). A voxel is a pixel: a
  16 by 16 tile becomes a 16 by 16 by 16 block.
- It starts from a 2D tile. Brought into the block editor, every layer of
  the block, front to back, is the tile's picture: each pixel becomes N
  voxels deep. That is only the start.
- Then it is worked on by hand in the block editor: voxels recolored,
  deleted, added. Stones that stand out, joints cut in, a round post, a
  grass edge hanging over: all made by hand. Nothing is shaped by the
  engine or a tool on its own (an automatic relief was tried and
  rejected).
- Colors: a world has at most 255 (the palette). That is enough because
  the engine does light and shadow: a block needs no painted shading,
  only its materials' colors.
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
- Import a 2D tile (drawn out N deep), paint, erase, add, pick a color,
  undo; the block's colors as its palette.
- It saves by itself, like the rest of the IDE; the running game shows
  the change at once.
- SLICE (2026-11-12, MagicaVoxel's and Goxel's way in): X, Y or Z in
  the inspector and the LAYER (or the arrow along the axis in the view,
  Page Up and Page Down) cut a block, a model's frame or a whole figure
  (every part by the one plane, in the figure's box): what lies on the
  eye's side of the plane is neither drawn nor met by the tools, which
  work on the cut face and what lies behind it. The window's alone, the
  file untouched (`editor/slice.rs`; `--slice z,12` in a shot).
- Shaping in place (2026-11-12): EDIT on a piece in the level editor
  opens its model with the level's other pieces round it, faint and
  see-through, as they stand relative to this piece: the model upright,
  the room turned instead (`editor/context.rs`). CONTEXT in the top bar
  shows it or not; the level's hide marks at the time of EDIT hold;
  only drawn, never met by the tools, never written; the level file read
  anew when it changes. `--context LEVEL:NAME [--context-off]` in a
  shot.
- A figure's parts (2026-11-13): the list begins with the figure whole,
  where it opens: every part solid, the tools on the part they meet, the
  whole kept chosen (a part is chosen by its row, a double click, or
  SELECT in its menu: a click never both chooses and paints). A part
  chosen has MOVE | ROTATE at it (rings about its joint, by 5°, Ctrl by
  1°) and a ROTATE row under POSITION; it is drawn and shaped at its
  rest rotation, the tools in its own grid. ADD PART (on top of it,
  touching), REMOVE, MERGE INTO its parent or WITH… another (a part
  rotated by quarter turns of the other merges as it shows), DUPLICATE,
  MIRROR (pairs named `_left`/`_right` or `_l`/`_r`; SYMMETRY on one of
  a pair lands on the other), RENAME, HIDE, HIDE OTHERS and SHOW ALL are
  in its row's menu, its menu in the view and its bar, a refused one
  quiet with why on hover; + and SHOW ALL at the PARTS heading. No step
  re-hangs a part it did not act on: such a part's parent and joint are
  stated first. POSE (top bar) turns parts on top of their rest with
  the rings, nothing saved, the pose as Lua to COPY; GAME VIEW shows the
  figure as the game draws it (camera, light, shading, outline), posed
  while POSE is lit.
- Without the window (2026-11-12): `voxcade edit GAME --model FILE
  [--frame N | --part NAME] --clear X0,Y0,Z0 --to X1,Y1,Z1` and `--fill
  SIGN|#rrggbb --from X0,Y0,Z0 --to X1,Y1,Z1`, both corners in, from the
  pivot as the file's `at`, in order, write the model exactly as the
  editor does (`ModelFile::change_cells`, `keep_text`): a color new to
  the palette takes the next free sign, a box past the frame's grows it
  (a part about itself, frames about the pivot), every other frame or
  part keeps its text. It prints the cells set and cleared and the box
  grown; `--dry` only says; with `--shot` it only shows.

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

[legend]              # a sign is a block as it lies: its name, or the name and an orientation (0 to 47)
s = "stone"
S = "spikes"
w = "wall"
t = { block = "stone", orientation = 17 }

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

[[thing]]
kind = "tuer"         # a kind without a behavior may face a way (or rotation = degrees)
at = [80, 5, 0]
facing = "east"

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
  (`at`, anywhere: on a wall, in the air), `facing = "east"` (or `rotation =
  30` where the game rotates things freely, `c.camera.rotates = "free"`),
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
  EDIT, POSITION and ROTATE as three fields each on three equal columns,
  their letters in the handles' colors and the numbers right-aligned,
  SOLID, LIGHTS with a row each; LEVEL: size and
  daylight). In the view the chosen one has handles (`handles.rs`):
  arrows along X, Y and Z (red, green, blue) move it by whole voxels,
  rings rotate it (about its own right side, its forward and the upright:
  each ring one of pitch, roll and rotation; quarters on the grid, freely
  otherwise, a quarter within 5 degrees taken), an area's faces size it,
  a light of a piece chosen has arrows along the model's own axes; the
  thing itself is dragged over the floor at its height. Beside it COPY,
  REMOVE, EDIT (its model) and LIGHT (on and off); a right click on it,
  or on its row, offers those and more (RENAME, HIDE or SHOW, HIDE OTHERS,
  SHOW ALL, ROTATE 90°, LAY DOWN, SOLID), on the floor what can be added
  there; a double click opens a piece's model. A click chooses what the
  picture shows under the mouse: the first voxel the ray meets, a small
  piece in the hollow of a big one before the big one (`editor/pick.rs`;
  a box only for a light alone, a place or an area); ADD sets down on
  the face of the voxel met. The mark at a row's left (as the figure
  editor's parts have it) hides a thing in the editor only: not drawn,
  not clicked, nothing written. A thing of a kind without a behavior
  rotates as a piece does (`facing` or `rotation` in the file; a quarter
  unless its kind turns freely), shows its kind under its name, and EDIT
  opens its kind's model. Keys as before (arrows nudge, Page Up and Down
  raise, T rotates, Delete removes, F2 names, Escape lets go and puts a
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
  block. A cell lies its block in any of 48 orientations (`orientation`
  in the legend): quarter turns about the three axes, mirrored or not
  (`blocks::Turn`); Tiled's flips are among them.
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
  levels, the game's own light and shadows, seen from any side. Every
  scope of it (a level of blocks, its things, its pieces, a block, a
  model's frames, a figure of parts) is laid out the same way
  (`zones.rs`, the owner, 2026-11-12: of one cast, one way of working):
  a bar along the top (what is open, + something, UNDO, REDO; on the
  right how the view looks), the outliner on the left (the level's
  blocks or kinds, a model's frames, a figure's parts as their tree, each
  a row with its small picture or sign; a right click its menu), the
  inspector on the right (short rows of a word and its controls: the
  tools in a row, BEVEL, SYMMETRY, a fresh box's DEPTH, the colors as
  swatches and the held one's GLASS, METAL, ROUGH, GLOW; a block's kind
  and turns; a part's PARENT, a menu, and JOINT, three fields); in the
  view a right click's menu (on a voxel PICK COLOR, ERASE, SHAPE the
  part there; on a level's block OPEN, COPY, REMOVE), the chosen part's
  joint as arrows (their heads grab, a drag moves it by halves of a
  voxel, one step; Escape back), a click in the figure whole shapes the
  part under it, and the view's own buttons. No sentences on the panels:
  what a control does is its tooltip, with its key; what the editor did
  or would not is a short line over the view, its whole words on hover.
  (`shaping.rs` for these, `stage.rs` for the pieces.)
  - Level: ADD, ERASE, PAINT, PICK blocks in the cells (the voxel
    editor's words); the held
    block shown with its kind (click: the next kind), ROTATE (about the
    upright), ROLL (about the line of sight), MIRROR; NEW draws a block out
    of a tile of the tileset, or starts an empty one; CLONE copies the
    held block under a new number, DELETE takes it out of the level (its
    cells empty, its file gone; UNDO brings both back). Placing past the
    grid's right, back or top grows it.
  - The level's things (BLOCKS and THINGS at the top switch, K and H):
    ADD puts the held kind down where the mouse points, standing on a
    top or floating before a wall, in the middle of the row's depth (the
    lane); REMOVE takes one away, MOVE drags one elsewhere, PICK takes up
    its kind (A, E, M, I). The kinds of `c.things` lie along the bottom
    as small pictures in the outliner (a kind without a model: a pale column as wide and
    tall as its body), the held one see-through where it would stand,
    the one under the mouse tinted by the tool.
  - Block (a double click on a cell, or the held block's picture clicked
    again): ADD, ERASE, PAINT, PICK voxels, DEEP PAINT (the voxel and
    every one behind it, through the block); TURN turns the block itself
    a quarter about the upright (a side shaped as the front brought
    round); COPY (the copy opens), REMOVE; the block's colors big in
    the inspector, the level's other colors small, any color from the
    picker beside HELD; BACK (or Escape) returns.
  - The mouse: left works the tool (a stroke works on the surface it began
    on: one layer added or taken off), right drag turns, middle or shift
    and right drag moves, the wheel zooms (over the bar: scrolls).
    Shift held is PICK, whatever the tool (the tool before comes back
    when it is let go). Ctrl+Z, Ctrl+Y. Every word has a key, shown at its end (the panels
    are the IDE's look, egui) (a tool the same on both levels: A, E, P, I, and N for DEEP PAINT;
    U and R undo and redo, T, O, M turn, C clones, D deletes, N new, B
    back; among the things M moves); the block's own colors (the big fields) are on 1 to 9 and 0,
    their numbers on them. Every change
    is saved at once.
  - `voxcade edit <game> [--block NAME | --things] --shot FILE [--view
    YAW,PITCH]` draws it without a window.
- **Not yet**: growing the grid left, forwards or down; blocks with names
  of their own.


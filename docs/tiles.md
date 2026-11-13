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
  A block's `.vvx` carries colors alone: glass, metal and glow on a
  color are dropped when it loads (`Block::load_in`), where a scene refuses
  them with a message; which of the two is right is still to be decided.
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
  figure as the game draws it (the game's angle, light, shading,
  outline) framed to fill the view, zoomed and turned as always, posed
  while POSE is lit.
- A selection (2026-11-13): SELECT (L) is a rectangle dragged on the
  screen: the voxels of the active parts whose middles it holds (THROUGH:
  behind too; off: those seen); NEW, ADD, REMOVE beside the tools (Ctrl,
  Alt while dragging), CLEAR and Escape; a double click takes the color
  connected. A part's row dragged onto another's hangs it there, where
  it stands. Its arrows move the voxels (Alt:
  what they land on stays), SELECTION at the inspector's foot and its
  menu DUPLICATE, REMOVE, PAINT,
  FLIP, ROTATE, NEW PART, MOVE TO PART…, COPY, PASTE; in the figure whole
  it may span parts. FILL (W) is the bucket (Ctrl: every voxel of the
  color). A swatch's right click: REPLACE WITH HELD, REMOVE.
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

[legend]              # a sign is a block as it lies: its name, or the name and an orientation (0 to 47, see below)
s = "stone"
S = "spikes"
w = "wall"
t = { block = "stone", orientation = 17 }

[cells]
layers = """
ssssssssssssssssssssssss
s......................s *10
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

## A terrain (2026-11-13)

A level may hold a landscape, `[terrain]`: heights at control points, as
text, no height picture (the owner: "a landscape generator light", round
hills and steep cliffs alike, from the pre-Alps to the Alps to the Grand
Canyon; "nothing too uniform either"). It goes beside pieces, places and
areas, and beside blocks (the blocks' voxels first, the land where they
leave room); a level with it needs no blocks. Every word but `spacing`,
`heights` and one material is optional; what is left out changes nothing.

```toml
[terrain]             # heights in voxels, the far row first; shapes . SMOOTH ^ LINEAR # STEP
spacing = 16          # voxels between points, x and y alike
at = [0, 0, 0]        # the first point's square's low corner; its z the base
heights = """
 4  4  6 10 14 12
 4  5  8 14 20 16
"""
# shapes: the same layout as heights, a sign a point
shapes = """
 .  .  .  ^  ^  .
 .  .  #  #  ^  .
"""
terrace = 8           # STEP's terraces without layers, voxels high
rough = 0.6           # how uneven the land is, 0 to 1
# roughness: each point's share of it, a digit: 0 calm to 9 (all 9 without)
roughness = """
 0  0  1  9  9  2
 0  0  1  9  9  2
"""
seed = 7              # the unevenness, the patches, the grain and the soft borders
under = "#6b5138"     # what lies under the materials (else the last one's color)

[[terrain.layer]]     # from the bottom up, again and again upward
thick = 12
hard = 0.9            # 1 a wall with a ledge on top, 0 a slope of scree
color = "#9c4f2e"
[[terrain.layer]]
thick = 7
hard = 0.15
color = "#d9a46a"

[[terrain.material]]  # the first that fits paints
color = "#eef2f5"
above = 60            # the voxel higher than this over the base
blend = 8             # the border wanders over 8 voxels, lower in hollows
[[terrain.material]]
colors = ["#7d7468", "#6e665c"]   # two or three tones, in patches
patch = 5             # about this many voxels across
steeper = 45          # its column's slope, degrees
[[terrain.material]]
color = "#5c8a3a"     # the last has no conditions: everything else
grain = 0.3
```

- **Points** stand in the middle of their own square of `spacing` by
  `spacing` columns; the land is `spacing` times the points across and
  deep, inside the level's size (a mistake says by how much it is not).
  Heights are whole voxels from 0, the far row first as a level seen from
  above reads; written by the editor as a grid lined up in columns, the
  shapes and the roughness under them in the same columns.
- **Shapes** (the interpolation words of glTF and the animation tools):
  SMOOTH (`.`) a Catmull-Rom curve through the points, bicubic, held
  within the four points round it so it never overshoots (rolling land);
  LINEAR (`^`) straight slopes, bilinear (ridges and peaks); STEP (`#`,
  TERRACED) that land cut into terraces: flat ledges, and where the land
  crosses the next terrace a riser over a short run (a wall with a little
  foot), so a cliff follows the land's contour: it curves with a valley,
  several stack into a banded wall, a single high STEP point is a butte
  with a round outline. Each shape weighs in by the four points round a
  column, each by its bilinear weight: SMOOTH and LINEAR meet without a
  seam, a meadow runs into a cliff. At a point's own column its height
  exactly where the land is even (and, for STEP, on a terrace's line).
  The first STEP (a point's height as a plateau over its square) gave
  right angles along the grid, "more angular than Minecraft" (the owner):
  gone.
- **Roughness**: `rough` adds three octaves of value noise from `seed`
  (swells eight points across, ±4 voxels at 1; bumps a point across, ±4;
  small unevenness five voxels across, ±3: most of it small, so 0.25 to
  0.35 breaks a meadow's contour rings into irregular ground without the
  land looking lumpy from afar), times each point's
  `roughness` (0 to 9, bilinear between them; ROUGHEN and CALM paint it).
  Under STEP the noise is added before the cut, so the terraces' lines
  wander and their ledges vary in width, and their heights shift a little
  (±2 voxels at 1). `rough` unset or 0: the land exactly as the points say.
- **Layers** (`[[terrain.layer]]`, from the bottom up, the stack repeated
  upward): where the land is terraced, the bands are the terraces
  (`terrace` then no longer counts): a band's riser takes the last
  `0.1 + 0.8 (1 - hard)` of its height, bent up at its foot as hard as it
  is, so a hard band stands as a wall over a ledge and a soft one
  weathers back into a slope. The bands lie as rock does, never as a
  cake (the owner: "layers never run quite straight"): the stack is
  measured from a gently warped surface, `dip` degrees of tilt (3) toward
  `dip_to` (degrees clockwise from north, else from the seed), broad folds
  of up to `fold` (0.4) times the thickest layer many points across, a
  waver of a voxel or two; each layer swells and thins along the land
  (`0.6 fold`, a quarter at 0.4), the thinner ones pinching out to nothing
  here and there; `fault` (0 to 1, none at first) lifts the bands on one
  side of one seeded line by up to the thickest layer. Ledges, walls and
  their colors follow it. `dip = 0`, `fold = 0`: flat bands, a cake. A
  terrain without layers has none of it.
- **Materials**: the first whose conditions all hold paints a voxel:
  `above`, `below` (the voxel's height over the base, the top voxel of a
  column h high being h), `steeper`, `flatter` (its column's slope in
  degrees, over three voxels each way: where the land goes on the same
  way the gentler side counts, so a tread beside a riser is flat and a
  single contour step no slope; over a peak or a hollow the mean; a
  wall's face, under its column's top and open over a drop of 3 voxels or
  more, is as steep as its drop); `depth` voxels down from where
  it can be seen (3), `under` below; `grain` 0 to 1 a slight shade voxel
  by voxel; `colors` two or three tones mixed in patches about `patch`
  voxels across (6; two octaves of value noise over a warped plane, so
  their borders are organic, never along the grid; tones close to each
  other read as variation, far apart as camouflage), each with its grain; `blend` lets every condition's
  border wander (value noise: up to half of it either way, voxels of
  height or degrees of slope; the height's lower by up to half again in a
  hollow, so snow reaches down a gully). The last material has no
  conditions; at most 32. **Who wins:** a material whose conditions fit
  paints; where only the last (unconditional) one would, on land STEP
  weighs in on (half or more) in a terrain with layers, the layer's color
  at that height shows instead, but only on walls: a voxel open to the
  side over a drop of 3 voxels or more. Ledges, treads, a plateau and its
  single contour steps show their material (grass by `flatter`), the
  walls their bands. The land's outer
  edge, cut, shows what lies under. The palette: the blocks' colors, then
  each material's tones and their shades in order, `under`, the layers.
- **Built** with the level, as a map's ground: every column filled from
  the world's floor to its height (nothing hangs over, nothing falls
  loose), solid, blasted, meshed in chunks, the heights on several cores
  (`par::in_order`), in plain sums of whole numbers and value noise of our
  own (one tangent, `libm`'s): the same voxels on every system. 640 by 576
  voxels with every word at work (roughness, layers, patches, blend) in
  about 160 ms; 640 by 400 of plain land in about 50. `crate::terrain`;
  `level:ground(x, y)` is its height where things are placed in code;
  `tools/vvx.py` reads and writes the section as the engine does.
- **Scatter** (`[[terrain.scatter]]`, 2026-11-13; the owner: plants as
  real pieces, boulders baked in): `model = "fir.vvx"` (or `models = [..]`,
  taken by chance), `density` (how many on 100 by 100 voxels of ground
  that fits), the materials' conditions (`above`, `below`, `steeper`,
  `flatter`, `blend`; the ground's height and slope), `apart` (the least
  distance between two of the rule, voxels), `rotate` (`"any"`,
  `"quarters"`, the default, `"none"`), `sink` (voxels into the land),
  `as` (`"pieces"`, the default, or `"voxels"`). Where: one candidate in
  each square of a grid 100 / sqrt(density) voxels wide, somewhere in it by
  chance from `seed` and the rule's place, kept where the ground fits and
  none of the rule's kept lies nearer than `apart`: even, no clumps, no
  rows, the same on every system. `as = "voxels"`: baked into the land as
  the level is built (part of the terrain: solid, blasted, no thing each;
  quarter turns only; its colors after the land's in the palette, beyond
  255 a mistake naming the rule). `as = "pieces"`: never made as a level
  starts; the editor's SCATTER sets them into the level once as its own
  `[[thing]]` pieces (named after the model, on from the numbers there
  are, standing on the land, sunk, turned freely only in a game that turns
  things freely), each marked `scatter = N` (the rule's number); from
  then on the owner's, moved or removed one by one. SCATTER is quiet while
  pieces of its rule are there; CLEAR takes them all away, moved ones too
  (the mark stays on them), and SCATTER can go again (after REROLL, say).
  A rule taken out leaves its pieces, unmarked; rules moved, the marks
  follow. `voxcade edit GAME --level FILE --scatter N` sets them without a
  window (written as the editor writes).
- **Things follow the land**: a step that changes its surface (a brush let
  go, a typed height, REROLL ...) moves the things and places standing on
  it (their feet on the land as it was, within a voxel) with it, in the
  same step; what is set in the air or sunk deeper stays.
- **The editor** (`editor/land.rs`): + ADD's TERRAIN makes a flat one over
  the level, TERRAIN beside PIECES (and BLOCKS and THINGS) shows it. The
  outliner holds the terrain's lists instead of the level's things (they
  stay drawn): MATERIALS (one row each: its tones, its conditions in short,
  `>82 <40 >42° <10° ~8`, `else` for the last; ×), LAYERS (one row each:
  the color, THICK, HARD, ×) and SCATTER (one row each: the model's
  picture, density and conditions, P pieces or V voxels, ×); + at a
  heading adds, a row dragged onto another moves it, a click on a row
  shows its fields in the inspector. The points as small squares on the
  land, the one under the mouse lit; POINTER chooses one and drags it up
  or down along its vertical, the point staying under the pointer at any
  zoom (Shift: a third as far), the brush (RADIUS, in points; 1 the point
  alone) carrying the points round it along so a drag raises a hill, the
  land built again as it goes (only the columns the points reach) and
  written at the let-go, one step; SELECT (a rectangle on the screen, NEW,
  ADD, REMOVE) and the selected ones move together or take a typed HEIGHT;
  FLATTEN, SMOOTH, PAINT (the SHAPE lit), ROUGHEN and CALM brush over the
  points. The inspector: TOOL, RADIUS, POINT and HEIGHT, SHAPE, GRID
  (spacing, points across and deep), ROUGH and TERRACE, SEED with REROLL,
  UNDER; then the chosen row's fields: a material's COLORS, GRAIN and
  PATCH, HEIGHT (ABOVE, BELOW) and SLOPE (STEEP, FLAT), each switched by
  its word, BLEND; a scatter rule's MODEL, DENSITY and APART, SINK,
  ROTATE, HEIGHT, SLOPE, BLEND, AS, SCATTER and CLEAR. It fits 1600 by 900
  without a scroll with four materials, five layers and two rules; REMOVE
  TERRAIN at its foot. Only the terrain's lines of the file change. A
  model ADDed lands where the ray meets the land and stays on the mouse
  for the next click (a stamp, until Escape). `voxcade edit GAME --level
  FILE --terrain --shot OUT [--raise X,Y --by N [--radius R]] [--shape X,Y
  smooth|linear|step] [--material N] [--scatter N]` to look.
- **Contour rings** on SMOOTH hills are the land's whole-voxel steps, lit
  face by face. Honest ways round them: `rough` a little (0.2 to 0.4 on
  meadows) breaks the rings into irregular shapes the eye reads as
  ground, and `colors` with `patch` keeps the grass from one tone; a
  steeper or a flatter land has fewer of them. Smoothing the light or
  half-voxel heights would fake the grid away, which the engine does
  not do.

## Built (2026-10-16)

- **Blocks** (`src/blocks.rs`): `blocks/NAME.vvx`, a model of Voxcade's
  own (text, `src/vvx.rs`; a `.vox` until 0.6.0, `voxcade upgrade` brings
  them over) of exactly N by N by N voxels, written at once
  through a hidden file beside it so a running game never reads half a
  block. A cell lies its block in any of 48 orientations (`orientation`
  in the legend): quarter turns about the three axes, mirrored or not
  (`blocks::Turn`); Tiled's flips are among them. The number is
  `8 * axes + flips`, where `axes` (0 to 5) says which axes of the block
  as made lie across, deep and up (0: across, deep, up as made; 1: across,
  up, deep; 2: deep, across, up; 3: deep, up, across; 4: up, across, deep;
  5: up, deep, across) and `flips` adds 1 to read across from the far end,
  2 deep, 4 up. So 0 is as made, 1 mirrored left to right, 17 a quarter
  turn about the upright (anticlockwise seen from above) and 41 a quarter
  turn about the line of sight. The editor's ROTATE and MIRROR set it.
- **Levels** (`src/grid/`, `c.world.blocks = { file, margin, below,
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
    U and R undo and redo, T, O, M turn, C clones, Delete removes a block or a frame (UNDO brings it back, a block's file too), N new, B
    back; among the things M moves); the block's own colors (the big fields) are on 1 to 9 and 0 (a model's by its palette's order: a new color the next number),
    their numbers on them. Every change
    is saved at once.
  - `voxcade edit <game> [--block NAME | --things] --shot FILE [--view
    YAW,PITCH]` draws it without a window.
- **Not yet**: growing the grid left, forwards or down; blocks with names
  of their own.


---
name: voxcade-models
description: >
  Make or improve the voxel models of a Voxcade game: figures, enemies,
  props, pickups, title props; their size, proportions, faces, colours
  (one a material: the engine lights them, never shade with colours),
  clean voxel forms (the voxel charm), materials (metal, glass, glow),
  animation frames or figures of
  parts that the engine turns (swing, pose), the outline round models,
  and how to judge a model (voxcade sheet, shots in the game). Use this
  whenever a .vvx or .vox model, a figure, a sprite, a character, an enemy, an
  animation, a walk cycle, voxel art or tools/gen_*.py figures come up,
  when the user says a model looks blocky, klotzig, coarse, ugly, flat or
  unreadable, ragged, or not cleanly voxelled, or asks for more detail, a
  face, better colours or smoother animation, even if they never say
  "model". Not for level blocks: those
  the user shapes by hand (voxcade-blocks).
---

# Voxel models for Voxcade

The figures are small and plain: the 8-bit charm in clean voxels. The
templates and Voxkeep went to 16-bit size (24 to 48 voxels) for a day,
with faces, hoods and buckles, and came back (the user, 2026-10-23: "the
8-bit variants had more charm", "for us as non-artists 16-bit went a bit
wrong", and small figures are easier to make, which is what the engine
is about). Even the slime, which looked better with more voxels, went
back: everything alike. Small never means careless: the user was let
down more than once by figures that came out as coarse blocks set
without intent. Few voxels, each one on purpose. Two more rules stay
(the user, 2026-10-23): one colour a material, the engine doing light
and shade; and clean voxelling, deliberate forms rather than ragged
round ones: that is the voxel charm.

The templates are the measure: `templates/platformer` (the hero 16
voxels tall in blocks of 10, the same figures as Voxkeep's, built from
boxes by `tools/gen_voxkeep.py`) and `templates/shooter`. Look at them
before making anything.

## The file: `.vvx`

A model is text, Voxcade's own (`src/vvx.rs`, its rules at the top):

```toml
format = "voxcade voxel 1"     # the first line, always
size = [7, 5, 12]              # the box: x across, y deep, z up

[palette]                      # a sign (a-z, A-Z, 0-9) a colour with its material
a = "#3a2a20"
g = { color = "#88ccff", glass = 0.25, rough = 0.3 }
m = { color = "#b0b4b8", metal = 1.0, rough = 0.3 }
e = { color = "#ffd040", glow = 1.0 }

[[frame]]                      # frames in order (or [[part]]s, below)
at = [-2, -1, 0]               # its box's low corner from the pivot (else the model's corner)
layers = """
.aaa.
aaaaa *3
.aaa.

..e..
*4
"""
```

- **Layers**: the bottom layer first, an empty line between; in a layer
  the far row (highest y) first, so it reads as the model seen from
  above; x left to right; `.` empty. A row ending ` *3` stands three
  times, a line `*4` after a layer's rows makes that layer four (counts
  in all; the writer folds from three alike up).
- **From the pivot**: `at` and a part's `joint` are measured from the
  pivot, the cell the game stands the model on (`size / 2` across and
  deep, on the floor, `docs/game-model.md`), its low corner. So the box
  runs from `-(size / 2)` across and deep, from 0 up; a voxel outside
  `size` is a mistake (make `size` bigger). A part that grows changes
  `size` and its own `at`, never another part's `at` or joint, and the
  model keeps standing where it stood.
- **Parts**: `[[part]]` with `name`, `at`, `layers`, and only where set
  by hand `parent = "body"` and `joint = [x, y, z]` (halves: a voxel's
  middle, `2.5` the middle of the voxel from 2 to 3).
- **Signs stay**: written again (the editor, a tool), a model keeps the
  signs its file had and adds new ones; with more than 62 entries every
  sign is two characters. One cell, one voxel per frame or part.
- **Bringing one in**: `voxcade import <game> FILE.vox [--as NAME.vvx]`
  writes a MagicaVoxel model as `.vvx` beside the game's (parts hanging
  as they did: a joint the text's voxel order would move is written by
  hand), leaving the source alone. A whole game made for an older
  Voxcade: `voxcade upgrade <game>` (`--dry` first). Worlds and map
  prefabs are `.vvx` too, of one frame (`src/scene.rs`): their palette's
  order numbers the world's colours and a `{ marker = "kind" }` entry
  places a thing; no glass, metal or glow. They are no models: the
  voxel editor does not shape them (the IDE shows them).
- **Tools**: `tools/vvx.py` writes and reads it from Python
  (`write_frames`, `write_parts`, `read`, `write_model`; worlds and
  prefabs `write_scene`, levels `write_level`, maps `write_map`); the generators
  write `.vvx` for a path ending so (`gen_sokovox.write_vox`,
  `figures.save`).

## Size and proportions

| What | How tall | Notes |
|---|---|---|
| a hero or enemy figure | 10 to 20 voxels | the templates' hero 16, the arena's 10, More Blood's 22 at most; the game's `view_width` makes it read |
| its head | a big share of it, 5 to 7 voxels across at 16 tall | a face of two dark eyes; big heads carry the character |
| small creatures, pickups | 6 to 12 | the template's slime 8, bat 11 |
| props | as the game's blocks | a door 20 in blocks of 10 |

Odd widths keep a middle column (a nose, a buckle, a seam). Lengths and
speeds in `conf.lua` follow the figure (a kind's height scales what it
leaves unset), so a bigger figure is no extra work there.

## Colours: one a material, the light is the engine's

- **Never shade with colours.** No lighter tops, darker undersides,
  rims or ramps of shades painted by where a surface faces: the engine
  lights the model (its sun, shadows and model shading, `model_shading =
  "edges"`), and painted light is doubled, and wrong once a figure faces
  another way or a part turns. The user: that is not done on voxel
  figures. (The first 16-bit templates had hue-shifted ramps; they were
  taken out.)
- **Colours differ for materials and designs only**: skin, cloth,
  leather, metal; eyes, a buckle, a stripe, camouflage, stones a little
  apart in colour as stones are; light itself (a flame, the glow beyond
  a door), bright so it glows (`c.effects.glow`).
- **A line a painter would draw is shape**: a seam or a groove sinks in
  a voxel, a pocket, a flap, a buckle, a rivet, a cuff stands out one.
  The engine's light then draws the line.
- **Contrast between materials**: a red tunic, a teal hood, brown boots,
  dark trousers read apart at game size.
- **Metal**: `metal = 1.0, rough = 0.3` on a palette entry (`metal(colour)`
  in `tools/figures.py`; MagicaVoxel's metal material comes in so; the sky's
  reflection and a highlight move over it: a buckle, a blade, a polished
  gem). **Glass**: `glass = 0.25, rough = 0.3` on an entry (how see-through, how
  rough; MagicaVoxel's glass, `_trans` and `_rough`, comes in so), a cockpit, Voxkeep's gem. Glass with a roughness
  mirrors the sky a little (much at a glancing look) and the sun and the
  lights glint off it, by the slant of the surface where a voxel lies,
  not its faces: a stone cut in voxel steps sparkles facet by facet as it
  turns (the gem: `glass = 0.25, rough = 0.3`, `spin = 120`; 0.7 see-through
  turned it to mist). Glass without `rough` mirrors nothing.
- **Glow**: `glow = 1.0` on an entry (0 to 1 its strength; MagicaVoxel's
  emission material, `_emit`, comes in so) makes every voxel of it
  glow in its own colour whatever the light, as `frame:cube`'s glow does
  (a lamp, eyes, a magic gem). The material belongs to the colour, so only
  voxels of that colour glow; the voxel editor keeps it when it saves,
  and sets it on the held colour (the quiet word beside the colour:
  GLASS, METAL, ROUGH, GLOW).
  `_flux` and `_ldr` are not read. No model of ours glows yet.
- The game's light decides how a colour looks (front faces are often
  darker than tops): judge colours in it (`voxcade sheet`), not in the
  editor's.

## Clean voxelling

A figure is made of deliberate voxel forms:
flat faces, straight runs, clean steps, both sides alike. Round shapes
sampled into voxels (ellipsoids, tapered capsules at angles) leave
ragged steps and bumps, and the engine's model shading turns them into
blotches.

- **Boxes with their edges cut back** (`rbox`, a bevel of one or two
  voxels; `axes=2` cuts only the upright edges: a belt, a hem) for heads,
  chests, skirts, hoods, boots.
- **Limbs of square section** (`limb`): a fixed number of voxels across
  (at 16 tall legs 2, arms 1 or 2), swinging in one plane, with their joints
  on voxel middles: straight a clean column, bent a clean line of steps.
- **Mirror-alike round a middle column** when standing: columns placed
  in pairs (`hero_xs`), legs and arms at the same angles. A stance with
  one leg four degrees ahead makes two different legs.
- **`tidy()`** after a frame is built: holes a voxel wide filled, loose
  voxels gone.
- Ellipsoids and capsules only where a form is truly round and big
  enough to stay smooth (a slime's dome), and look at the result.

## Faces

A face is what makes a figure a character, and the side the game shows
decides how it must read: the jump & run shows figures from the side,
the shooter from above in every way, a title prop from the front.

- Eyes as the user shaped Voxkeep's hero in the editor: a dark pupil
  two voxels tall, its white a single voxel on the outer side in the
  upper row, nothing round the corner (from the side only that white
  shows at the face's edge). A pupil or more white round the corner
  reads as four eyes from the front: rejected by the user. `face()` in
  `tools/figures.py` does it. Two dark 2x2 eyes on a seven-voxel face
  read as goggles; at 16 tall a pupil a voxel and its white beside it
  are enough.
- A nose standing out a voxel gives the profile; brows over the pupils;
  a mouth a single voxel; a blush under the eyes.
- Ears stand out a voxel at the side of the head (the light shows them).
- Hair and hoods are shells over the top and the back half, open as far
  back as the ears, never a helmet that fills the head or a hood open only
  at the front: from the side the face must show (the template hero's
  teal hood).

## Building with the toolkit

`tools/figures.py` (plain Python, imported by a script) shapes figures
in volume, never as a silhouette pulled through the depth; Voxkeep's
small figures are boxes (`box` in `tools/gen_voxkeep.py`):

| Tool | For |
|---|---|
| `Fig(size)`, `put`, `box`, `recolour`, `surface` | voxels by place, later shapes over earlier (`under=True`: only into empty cells) |
| `rbox(lo, hi, colour, bevel, axes)` | clean forms: heads, chests, hoods, belts (a paint function may leave voxels out: a hood open over the face) |
| `limb(xs, a, b, width, colour)` | limbs of square section in the y-z plane, `xs` the columns |
| `joint2`, `joint`, `ik` | a pose as joint angles; elbows and knees bending towards a pole |
| `tidy()` | fill one-voxel holes, drop loose voxels |
| `ellipsoid`, `capsule`, `cylinder` | truly round things only (a slime); they voxel raggedly |
| `metal(colour)` | a colour that shines as metal |
| `face(f, skin, eyes, mouth, nose, brow)` | a face that reads from front and side |
| `save(path, size, frames)` | frames into one model |
| `save_parts(path, [(name, Fig)])` | a figure of parts (below) |

A script imports it and writes a game's models (`save`, `save_parts`);
look at each with `voxcade sheet`. A generator's first result is a
placeholder: say so, and go round again.

The user builds the same way in the voxel editor: BOX (Q) and SPHERE
(S) drag a footprint on a face and let go: it is set right there, fresh
(outlined) until the next action, + and - its depth in place (Alt only
fills empty voxels, Ctrl cuts; Shift held is PICK in every tool), BEVEL (F) cuts a box's
edges as `rbox` does, SYMMETRY (Y) mirrors every tool left to right.
Suggest them when the user shapes a figure by hand.

**A game's models are the user's once they are there.** The user shapes
figures in the voxel editor ("that is what the editor is for"): never
run a generator over a game's model files (`gen_voxkeep.py` keeps every
file there; `figures.KEEP_EXISTING` for a script's own), and change a model there
only when asked, in that file. A file that changed and that you did not
change was changed by the user (or by someone they asked): say so, do
not guess who.

## Animation

Two ways, chosen per figure:

- **Frames** drawn by hand into the model (a jump, a crouch, an impact,
  squash and stretch), picked by what the figure does. A kind's
  `frames = { run = { 2, 5 }, idle = { first = 0, last = 1, fps = 2 },
  walk = { first = 1, last = 4, stride = 6 }, pull = { first = 9, last =
  11, loop = false } }`: each set its own speed, by the way walked, or
  once. An idle that bobs every tenth of a second annoys (the user):
  keep idle frames still, or slow.
- **Parts** the engine turns: a `.vvx` of `[[part]]`s (head, body,
  arm_left ...; a MagicaVoxel file's named objects come in so) is a
  figure of parts. The biggest part is the root;
  each other hangs on the part it touches most and turns about the
  middle of where they meet, so build the parts touching at their
  joints (a shoulder against the body's side, a head on the neck). Where
  that is not what you want (a jaw hinged at its back, an arm apart from
  the body), set it by hand: `parent` (a part's name) and `joint`
  (`[x, y, z]` from the pivot, halves allowed: a voxel's middle is its
  corner + 0.5) in the part's table; `vox::Part` has the rule. (The
  editor's FIGURE, `voxcade edit <game> --model FILE`, sets them there
  too, PARENT and JOINT by hand.) Parts may overlap on purpose
  (legs inside the body that come out in a walk): each keeps its voxels. A
  kind's `swing = { leg_left = 30, leg_right = -30, arm_left = -25,
  arm_right = 25 }` makes a walk without a single walk frame;
  `frame:model(file, thing, nil, nil, { arm_right = { pitch = 90 } })`
  poses parts by name from the script (attacks, waves, looking round).
  A figure of parts has one frame. `save_parts` writes one, a cell
  claimed by a later part taken from the earlier ones.

## The outline

`c.effects.outline = 0.5` (voxels thick; 1 a whole voxel, as a sprite's
pixel line) draws a dark line round the models, `outline_color` its
colour. It lifts figures off dark scenes (a castle at night) and shows
where an arm stands before the body. It is the game's choice: suggest it,
do not switch it on unasked. The user tried 0.5 in Voxkeep and did not
want it (2026-10-23): the figures stand without a line there.

## Judging a model

1. `./target/release/voxcade sheet <game> <model.vvx>` writes one picture
   in the game's light, camera angle and shading: five sides close up,
   every frame from the side (a figure of parts its walk in eight steps),
   and the five sides at the size the game shows them. Read it every
   round; crop and enlarge the head to judge the face.
2. Look at it in the game against its background: `voxcade shot <game>
   --frames N --input ...` (`--size 3840x2160` and crop for detail).
3. Ask, as the user would: from the side the game shows it, does it
   read? At game size, is every voxel there for a reason? Is it blocky anywhere a
   sprite would have a detail? Do the materials stand apart?
4. Changing a model changes the replays' fingerprint, not their play:
   `voxcade check <game> --refit`, and say so.

## Mistakes already made (do not repeat)

- The 16-bit try (2026-10-22/23): figures 28 to 36 voxels tall with
  faces, hoods, pouches and buckles. Not bad, but less charming than the
  small ones, and much harder to get right without being an artist.
- Light and shade painted into the colours (ramps of shades by where a
  surface faces): the engine lights the model.
- Round shapes voxelled raggedly (ellipsoid heads, tapered capsules):
  the voxel charm is clean forms.
- A hood that hides the face from the side: the user liked the hood,
  the fix was to open it to the ears, not to take it away.
- Small figures set as coarse blocks without intent: small is the aim,
  careless never.
- A hood pulled over the head: from the side only a coloured blob.
- Capsules for belts and skirts: round ends make balls; a box cut round
  (`rbox(..., axes=2)`).
- Boots pointing backwards: a foot is the shin's direction plus 90.
- Eyes painted onto the front only: invisible from the side; or as big
  dark blocks: goggles.
- An idle frame that moves the whole body: it bobs ten times a second.
- Changing every figure at once when the user asked for one: a test
  piece first, the rest after the user has seen it.
- Blocks are not models: never reshape level blocks (voxcade-blocks).

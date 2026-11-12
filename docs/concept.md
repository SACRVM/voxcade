# Voxcade concept

Written from the feasibility discussion of 2026-10-14. These are decisions,
not open questions; change them deliberately. The engine's architecture,
scripting, editors and plan are in `engine.md` (2026-10-15).

## Why

Voxatron (Lexaloffle, 2010–2021, last release 0.3.6) proved the idea: a whole
world of destructible voxels, small games, instant fun. It never left alpha
and is no longer developed. Voxcade keeps the idea and the feel, not the
implementation, and is not a 1:1 clone.

## Decisions

**Native, not browser.** Rust, wgpu (Vulkan, Metal, DirectX 12), SDL3 for
window, input, gamepads and audio.

**Fixed camera as a style.** Perspective like Voxatron (an orthographic view
looked flat), one angle per game, set in `conf.lua`. No rotation, and no
change of height: the camera only moves across the ground. Top-down RPG, 2.5D jump & run, arena shooter are
all just different angles. The demo uses 35° pitch, a 40° field of view and
no turn: tilted enough that walls read as walls, square to the screen like
Voxatron. A fixed camera makes the visible area a fixed box
in the world (easy chunk streaming), saves geometry (faces that can never
face the camera or the sun are not built) and keeps controls simple.

**The world fills the screen.** No display cube floating in the void.
`view_width` sets how many voxels fit across the screen, independent of
resolution, so every player sees the same amount of world; or
`view_height` how many fit from the bottom to the top, the width then
following the window (a vertical shooter's field, whole in any window).

**A voxel is a 3D pixel, and a game may turn its models freely**
(changed 2026-10-22, the user's decision; it was "quarter turns only, no
free rotation"). The world is the one voxel grid, and every voxel is the
same size: nothing is scaled. Models turn in quarter turns on the grid
unless the game chooses otherwise: a kind with `rotates = "free"` faces
exactly its way (a diagonal too), and `frame:model` turns a model any way
(a ship banking, something tumbling); its voxels leave the grid then. The
user: Minecraft lives from it, and turning parts is the quick way to
animation where filling every in-between step with voxels is endless
work; the engine is there to have success quickly. Frames drawn voxel by
voxel stay for poses (a crouch, a jump); figures of parts that turn
(Minecraft's way) can come on the same turns.

**Lit like pixel art** (2026-10-15, the user's idea). Face-by-face light
makes a round voxel shape a staircase of light and dark bands, which is why
voxel games draw square heads. A model can instead be lit a whole voxel at a
time by the direction of the surface around it, so a face shades softly as
in 2D pixel art and only the outline stays blocky; where two flat surfaces
meet, the faces keep their own light, so edges stay hard. The voxels stay
cubes on the grid. Chosen per game and per model.

**Each game picks its grid, and counts in voxels.** How many voxels a
figure is tall is the game's choice. The figures are small and plain,
the 8-bit charm in clean voxels: about 10 to 20 voxels tall, few colours,
every voxel deliberate. The templates and Voxkeep went to 16-bit size
(24 to 48 voxels) for a day and came back (2026-10-23, the user: the
8-bit variants had more charm, and they are easier to make, for people
who are not artists too). One colour a
material: the engine does light and shade (2026-10-23, the user: shading
with colours is not done on voxel figures), and the forms stay cleanly
voxelled, deliberate boxes and lines rather than ragged round shapes, at
any size: that is the voxel charm. The arena's figures are 10 voxels
tall, More Blood's 22, the templates' hero 16. Every
length in `conf.lua` is in voxels and every speed in voxels per second
(a jump 17 high is 17 voxels high); there is no unit conversion, each game
states its own values.

**Pieces hold together the way they look.** After a blast, whatever no
longer connects to the ground falls apart. Voxels touching by a face, an edge
or a corner count as connected, since diagonal lines are everywhere in voxel
art (palm fronds, leaning trunks, stairs).

**Even the UI is voxels.** The HUD and messages are a second voxel scene
drawn over the world (the user's choice, half the Voxatron way): a pixel
font and pixel-art icons where every pixel is a voxel a couple deep, seen
in perspective with the vanishing point in the middle of the screen (the
user's choice, over an oblique view) so the sides facing the middle show,
on their own grid so they are equally
sharp at any resolution. They behave like the game: a lost heart bursts
into voxels, changed digits hop, messages drop in letter by letter.

**Editors that feel like a game** (changed 2026-10-15, see `engine.md`; it
was "no editor inside the game"). Models are Voxcade's own `.vvx`, text
(layers of letters, a palette; 0.6.0, the owner: everything in a game is
code or text), shaped in the voxel editor or by hand; MagicaVoxel, vengi or
Goxel files (`.vox`) are only brought in (`voxcade import`). The running
game reloads a model on save. A level
is one world (a `.vvx` of one frame, its palette numbering the world's
colours, markers named in it) or, for anything big, a map the engine puts
together at start: a height map and a material map (PNG, any paint
program) for the ground, `.vvx` prefabs placed on it, and markers, all
listed in `map.toml`; or a level of voxel blocks (`levels/NAME.toml`,
layers of signs). That keeps a 640x2368x80 level at about 300 KB instead
of 90 MB. A Tiled map comes in once (`voxcade import`); Voxcade's own
editors write the same text.

**Arcade feel is a hard requirement.** Fixed 60 Hz deterministic simulation,
interpolated rendering at the display rate, input read right before each
tick, low-latency presentation, instant startup and reload. Juice is built in:
screen shake, hit stop, debris, flashes.

**Sound like the old machines, made from text.** Effects are synthesized
from a few numbers each (sfxr-style); recordings are not taken (the owner,
2026-11-12: everything in a game is code or text). Music is MML, played by SID-like voices. All
of it is text in the game folder, so an agent can write and change it, and
the game plays the change on save.

**Built for agents.** The goal is that an agent helps someone write a good
game with the engine. So: a scripting API whose definition file doubles as
documentation (Lua 5.4 with LuaLS annotations, changed 2026-10-15 from the
planned TypeScript on QuickJS-ng; see `engine.md`), a CLI that can run a
game without a window and save frames (`shot`, more to come: state dumps,
replays), plain folders as projects, deterministic replays for regression
tests.

## Rendering

- World: sparse 32³ chunks of palette indices, meshed per chunk with per-vertex
  ambient occlusion. Only face directions some view ray or the sun can hit
  are built; with this camera and a sun from the front-left that is four of
  six (never the back or the underside).
- Moving things (player, bullets, debris, sparks) are instanced cubes.
- One orthographic sun shadow map (4096²) fitted to what the camera sees,
  moving in whole texels so shadows stand still while scrolling; only chunks
  inside it are drawn into it.
- Colours stay in palette space (non-sRGB target) for the flat retro look.
- Effects on top of the voxels, never instead of them: the scene goes to a
  colour and a glow target (16-bit float), both are shrunk to a quarter and
  blurred; the final pass adds the glow, blurs towards the top and bottom of
  the screen and lays the white flash of big blasts over everything.
  Flashes and explosions are short-lived point lights (up to 16), fire,
  smoke and dust are see-through cubes drawn far to near, haze fades the
  far part of the view. All of it is set per game in `[effects]`.

## Roadmap

The first roadmap, kept for the record; steps 1, 2 and most of 6 are done.
The plan from 2026-10-15 on is in `engine.md`.

1. **Arena prototype**: movement, shooting, destruction, debris that
   settles, shadows, hot reload, `shot`.
2. **Feel:** gamepad, enemies, hit stop, sound effects (sfxr-style parameters),
   player death and respawn. Decide here whether it feels like an arcade.
3. **Scripting:** TypeScript on QuickJS-ng, `voxcade.d.ts`, game logic moves
   out of Rust.
4. **Agent tooling:** state dump, input replays, `check` command, genre
   templates (top-down RPG, 2.5D jump & run, arena).
5. **Bigger worlds:** chunk streaming, a compact vertex format, LDtk/Tiled
   import onto the map format.
6. **UI and audio:** pixel font, speech bubbles, menus; MIDI import onto
   the MML voices, a SAM-style text-to-speech voice.
7. **Packaging:** builds for Linux, Windows, macOS; single-file game export.

Estimate from the discussion: a solid 1.0 in roughly 9–14 months of full-time
solo work.

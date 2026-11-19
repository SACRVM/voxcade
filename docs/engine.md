# Voxcade engine architecture

Written from the discussion of 2026-10-15, after More Blood had become a
fun demo, and kept up as the steps land (step 4, Lua, on 2026-10-16).
These are the user's decisions and the plan built on them; the open
questions at the end are still open. `concept.md` holds the decisions
about the look and feel, which all still apply; `ide.md` the IDE of
phase 2.

## Goal

Every kind of game there was in 2D, lifted into the third dimension with
our voxels: shooters (top-down, vertical and horizontal scrollers), jump &
runs and side scrollers, RPGs, adventures, real-time strategy, simple
racing games, and simulations like SimCity or Anno.

Anyone should be able to write a great game with it, with or without an
agent's help. So the engine carries many of the basics itself, ready to use
and set, instead of leaving every game to write them.

More Blood stays the proving ground: it is turned into a full game as the
engine grows, and every step of the engine is checked against it. Games
planned besides it: a jump & run (the second genre, step 5 below) and a
Doom homage (the user loved Doom).

## What stays

- **Text and plain files.** A game is a folder: `conf.lua`, `main.lua`,
  `.vvx` models, worlds and prefabs (text; MagicaVoxel's `.vox` only
  brought in), levels and maps as TOML (`levels/NAME.toml`, `map.toml`;
  JSON is gone from a game), MML music, sounds as numbers (no recordings). People,
  agents and editors all work on the same files.
- **Deterministic simulation.** Fixed ticks, randomness only from the
  simulation's generator. That is what makes `shot`, replays and regression
  tests possible, and what lets an agent check a game without playing it.
  Few engines can do that; it is not given up for anything.
- **Each game picks its grid** and states every length in voxels.
- **A voxel is a 3D pixel:** the world on the grid, every voxel the same
  size; models in quarter turns unless a game turns them freely
  (`concept.md`).
- **Arcade feel and juice** built in, hot reload of everything.

## Three layers

```
  games      Lua (the default)              Rust (Voxcade as a library)
             conf.lua, main.lua             a crate implementing voxcade::Game
                      \                       /
                       \                     /
  building   things and their parts (body, model, health, weapon,
  blocks     behavior, trigger, inventory), cameras, movement, aiming,
             pathfinding, grids, dialogue, saving, scenes
                                  |
  core       voxel world and chunks, rendering, light, effects, sound,
             synth, MML music, input, voxel UI and fonts, assets,
             hot reload, fixed ticks, headless tools (shot, sounds, music)
```

### One API, two doors

The engine is a Rust library (`voxcade`) with one game API. A game is
either:

- **Lua** (the default, the user's choice: easy to learn, no types
  needed). The `voxcade` program runs a game folder: `conf.lua` sets its
  numbers and assets, `main.lua` holds its rules and flow.
- **Rust**, for whoever wants something big or fast: a crate that uses
  Voxcade as a library and implements the same API directly.

The Lua runner is itself a game written on the Rust API. So the two doors
can never drift apart: what Lua can do, Rust can, and every Lua feature is
first a Rust feature.

The API ships as a Lua definition file with LuaLS annotations
(`lua/voxcade.lua`: `---@class`, `---@param`): it is the documentation, and
editors and agents get completion from it. Nobody has to write a type.

What a Lua game reads like (`games/sokovox/main.lua`, Sokoban in voxels,
shortened; "The Lua door" below has the whole API):

```lua
local LEVELS = parse(world:read("levels/microban.txt"))   -- Skinner's puzzles

function on_world_start()
  world.state.puzzle, world.state.done = 1, {}
end

function on_level_start(how)
  start(world.state.puzzle) -- builds the puzzle with level:fill, spawns things
end

function on_tick()
  local s, n = level.state, world.state.puzzle
  local L = LEVELS[n]
  if s.step and s.step.left == 0 then arrive(s, L) end  -- thing:place
  if input:pressed("restart") then return start(n) end
  if s.step then return slide(s) end                    -- thing:walk
  if input:pressed("undo") then return undo(s, L) end
  local d = input.dir
  if d.x ~= 0 then try_step(s, L, d.x, 0)
  elseif d.y ~= 0 then try_step(s, L, 0, d.y) end
end

function on_draw(frame)
  local s = level.state
  frame:model("keeper.vvx", s.keeper)
  for i, crate in pairs(s.crates) do
    frame:model("crate.vvx", crate, LEVELS[world.state.puzzle].goals[i] and 1 or 0)
  end
  frame:text(("MOVES %d  PUSHES %d"):format(s.moves, s.pushes), 4, 13)
  frame:text("{undo} UNDO", 4, 160)   -- the key the player presses for undo
end
```

More Blood plays with the shooter kit (`c.kit = "shooter"`): its soldiers,
huts, hostages and the gunship run by themselves, and its `main.lua` holds
its own rule, the helipad that wins once a gunship is down:

```lua
function on_death(thing, by)
  if thing.kind == "gunship" then
    level.state.gunships_down = (level.state.gunships_down or 0) + 1
  end
end
```

### LÖVE as the model for the Lua door

The user named LÖVE (love2d.org, 2026-10-15), the well-known 2D Lua
framework many indie games were made with, as the inspiration. What it does
right and Voxcade takes over:

- **A tiny way in.** A whole LÖVE game is `main.lua` with `love.load()`,
  `love.update(dt)` and `love.draw()`, plus callbacks such as
  `love.keypressed`: ten lines to start, growing from there. Voxcade's Lua
  door offers the same, with the fixed tick instead of `dt`:

  ```lua
  -- main.lua: a whole game, written the LÖVE way
  function on_world_start() ... end      -- once, the run's states
  function on_level_start(how) ... end  -- a level begins: build it
  function on_tick() ... end            -- 60 times a second, deterministic
  function on_draw(frame) ... end       -- every picture, drawn on frame
  ```

  This is the Rust `Game` trait in Lua. A kit (the shooter's behaviors)
  is a building block on top for whoever wants to get there faster, never
  a must: a game may write everything itself.
- **A game is a folder** (LÖVE: `main.lua` and `conf.lua`; Voxcade the
  same, with models, levels and music), and it ships as one file (LÖVE's
  `.love`, a zip of the folder, fused with the program into one
  executable): the model for packaging.
- **Modules by area** (`love.graphics`, `love.audio`, `love.keyboard`):
  easy to find, easy to learn.
- **A page for every function** in its wiki, and a big ecosystem of small
  libraries. Voxcade's counterpart: the API definition file (LuaLS) and its
  docs.

What stays different on purpose: LÖVE steps with a varying `dt` and is not
deterministic; Voxcade keeps the fixed tick, or replays, `shot` and
multiplayer are lost. And Voxcade carries building blocks (things,
behaviors, title, menus, effects) that LÖVE leaves to libraries.

### The game model

- **Things with parts.** Everything that acts is a thing made of parts:
  body (movement, collision), model (`.vvx` frames or parts), health, weapon,
  behavior, trigger, pickup, inventory, and whatever a genre needs. Kinds
  of things are defined in data (`c.things.rifleman` in `conf.lua`: model, hit
  points, speed, weapon, behavior) and spawned by markers, scripts or
  other things. Today's hard-wired kinds (rifleman, grenadier, guard,
  hostage, hut, gunship, pickups) become such definitions over the engine's
  built-in parts.
- **Behaviors** are built-in (Rust, set by numbers: "shooter", "patrol",
  "chase", "turret") or written in Lua. Lua gets events (`on_tick`,
  `on_hit`, `on_death`, `on_enter`, `on_use`, timers) and gives orders.
- **Scenes** (title, levels, cutscenes, menus) and the flow between them,
  with saving and loading of the simulation's state.
- **The world** is the voxel grid as now, plus layers a genre may need: a
  tile grid for buildings and fields, a walk grid for pathfinding.

**Built** (2026-10-15, step 3 of the plan):

- **Things** (`sim::things`): one list in the order things came into the
  world, each with an id that stays with it, its kind, the parts the engine
  knows (a body, health, the white flash after a hit) and a brain, the
  game's own state for it (a type it chooses, the shooter's `Brain`). A
  game runs its behaviors as systems, each over its own things in that
  order, so every run goes the same.
- **Kinds in data**: `c.things.NAME` in `conf.lua`, with a `behavior`, a
  `model`, `hp` and the behavior's numbers; `like = "other"` builds on
  another kind (More Blood's guard is like the rifleman with `hp = 4`).
  Markers place things by their kind's name. A mistake says where it
  stands; a reload keeps every thing by its kind's name.
- **The shooter kit**: the behaviors soldier, spawner, captive, gunship,
  pickup (checkpoints and goals are the game's things), and weapons as
  data, `c.weapons.NAME` (cooldown, speed, life, damage, blast, spread,
  burst, throw, ammo, reach, sound), the game's alone (`src/shooter/kit.toml` writes the kit's
  weapons and every behavior's kind out to copy from). The player's
  attacks are bound to actions (`c.player.attacks`), a run's arsenal is
  `c.state.player`. To scripts the player is a thing (`player`, its
  figure the shooter's own); inside the kit he is still its own struct:
  multiplayer and the jump & run's controller will rework him.
- **State kept and brought back**: `Session::snapshot`/`restore`. The
  world's chunks sit behind a shared pointer and are copied only when one
  side writes, so keeping More Blood takes 34 µs and bringing it back
  0.2 ms; brought back at tick 600, a run plays every tick to 1200 exactly
  as the first time (test `a_restored_game_plays_on_the_same`). The
  foundation for rollback over a network, going back in replays and saving.
- **Proved the same**: More Blood's recorded runs (the user's 148 s and
  120 s, the scripted ones) play out bit for bit as before, every second's
  state checked, and pins (`src/shooter/pins.rs`) cover what they do not
  (the kit's own numbers, a gunship firing nose-on, aim assist off). New
  data that leaves the game as it was gets the data's fingerprint by
  `voxcade check <game> --refit`, which writes it only when every replay
  runs exactly as recorded. Habits of the old code are kept for now, to be
  tidied in their own commits (re-recording the checks): a second round
  on a man who fell this tick lands on the one who moved up into his place
  among the enemies, the gunships slide on through hit stop.

### Lua decides, Rust computes

Pathfinding, physics, collisions, rendering and hundreds of RTS units run
in Rust. Lua is called on events and for decisions, not for the inner
loops, so a city builder stays fast.

### Determinism with Lua

Lua is embedded (Lua 5.4 through the `mlua` crate, built in) so that a
script cannot break the simulation's determinism (built in step 4,
`src/lua/sandbox.rs`, `prelude.lua`):

- Only `table`, `string`, `math` and `utf8`: no `os`, `io`, `load`,
  coroutines, `collectgarbage`, nothing with a clock. `require` runs the
  game's own Lua files, while it starts only (`docs/game-model.md`,
  stage E).
- `math.random` draws from the simulation's generator, only in
  `on_world_start` and `on_tick`. The maths go through `libm`, the same bits on every system;
  `^` calls the system's `pow`, which differs in the last bit, so the
  loader refuses it but for squares (`x ^ 2`; `math.pow` instead).
- `pairs` and `next` walk the keys in order (numbers, strings, booleans):
  Lua's own order changes with its string hash seed and after a restore.
- `tostring` and `string.format` show no memory addresses; metatables with
  `__gc` or `__mode` (the garbage collector's timing) are refused.
- The game's state lives in its states (`world.state`, `level.state`,
  `player.state`, `thing.state`), plain data the engine copies, hashes and
  brings back, checked every tick (the prelude's `plain`, about 90 ns an
  entry: a state of 80 000 entries costs 7 ms a tick, so big grids belong
  in strings or flat lists of numbers, not a table per cell);
  globals are frozen after `on_world_start`, `on_draw` and the kit's
  questions only look. A local of `main.lua` that its
  functions change escapes all of that: `voxcade check` plays a second
  twice from a snapshot and names it.
- An instruction budget stops endless loops (even inside `pcall`), a
  memory limit runaway tables.
- Scripts run inside the fixed tick, never on render frames.

## The game API

**Built** (2026-10-15, step 2 of the plan): the Rust door. Voxcade is a
library; the engine knows no shooter words, and More Blood is the first
game on it, playing exactly as before (every replay bit for bit, every
checked scene pixel for pixel).

```
src/lib.rs        the library `voxcade`: every module, the window and the
                  headless tools included, so a game in Rust has them all
src/main.rs       the `voxcade` program: cli::main::<Voxcade>, Lua games
                  with the shooter kit (Voxcade = LuaGame<Shooter>)
src/cli/          the command line any game gets (run, shot, check, film,
                  replay, replays, sounds, music, inspect, api, mcp)
src/game.rs       trait Game: what a game gives the engine
src/session/       Session<G>: a loaded game folder and the frame it makes
src/sim/          the simulation's core every game shares (Sim)
src/lua/          the Lua door: LuaGame<K>, the sandbox, the world API,
                  conf.lua, the API's register (step 4)
src/mcp.rs        voxcade mcp: the tools for an agent
src/shooter/      the shooter kit: its behaviors and weapons (More Blood its first game)
examples/rust_digger/  a second game in ~270 lines, the engine alone
```

**What the engine carries** (`Session<G>`, the hosts): the game folder
(`conf.lua`, the level from a map or a world, models and their bank
on the GPU, the display font, sounds, music kept in time with a game
that picks its own songs), the title
with its logo, props and flyby, the menus and settings, hot reload, the
fingerprint and the replays, attract mode, and the frame: the world's
chunks, debris and fire, the title, then what the game draws, then the HUD
in the UI's layer. The window (`app::run`), `shot` and `film`
(`headless`), `check` and `replay` are generic over the game.

**The core** (`sim::Sim`), held by every game: the world, the random
generator, ticks, hit stop, the camera following its target and the
shake, explosions with their debris, fire, smoke and light, pieces cut
loose falling, bodies that walk, jump and fall (`Body`), sounds reported
by name. A game's tick calls it in a fixed order:

```rust
fn step(&mut self, input: &Input) {
    if !self.sim.begin_tick() { return; }   // hit stop: everything holds still
    // ... the game's own things move, blast (self.sim.explode), make sounds
    self.sim.update_particles(keep_clear);  // debris settles, never in the player
    self.sim.update_flares();
    self.sim.follow(player_pos);            // the camera across the ground
}
```

**What a game gives** (`trait Game`, all but five with a default):

- Its words: `actions` (the input's bits besides the four directions; keys,
  gamepad buttons, replays and `shot` scripts name them), `behaviors` (what
  its kinds of things, `c.things.NAME`, may do; markers place them by the
  kind's name), `sounds` (events by name with their
  default synth), `sections` (its own tables of the config, which count for
  its replays; an unknown one is an error), `models` (by name, and
  whether their shape counts for the simulation), `default_keys` and
  `default_pad`.
- Its config: `conf` (by default the folder's `conf.lua`; a Rust game may
  set it in code) and `config_file` (the file hot reload reads anew).
- Its level, optionally: `level` builds one in code instead of loading
  `c.world.file` (the Rust Digger's hills).
- Required: `load` (from the world, markers, models and config),
  `sim`/`sim_mut`, `step` (one tick with the keys held), `state_hash`
  (everything that carries it forward, the core's with it: replays check
  it every second), `snapshot`/`restore` (its whole state kept and brought
  back; a Rust game whose state is a value clones itself).
- A game with code in its folder: `code` (what of it counts for replays),
  `reload_code`, `halted` (the error that stopped it), `hidden` (what its
  code keeps outside its state, for `check`'s guard), `inspect` (its things
  and state as JSON for tools).
- Drawing: `draw` into `Drawing` (models from the bank, single cubes,
  chunky blobs, lights; on the grid, the first drawn keeps a cell; a
  model turned freely, `model_turned` with `session::turning` or
  `session::facing`, leaves the grid and claims no cell) and
  `hud` (voxels in the UI's layer, from `ui`'s text and icons: a Lua
  game's is what its `on_draw` drew there; the kits draw none).
- What the engine asks: `focus` (the body the camera keeps on screen),
  `outcome` (won or lost), `scored`/`score` (the high score), `music` (the
  song it picks and how far in: the audio keeps time with it), `rumble`, `summary` (the window title), `report` (`shot`'s lines),
  `place_player` (`shot --at`), and the hot reload's `apply_config`,
  `replace_world` and `reshape`.

A game in Rust is a crate (or an example) with one line of `main`:

```rust
fn main() -> anyhow::Result<()> {
    voxcade::cli::main::<Digger>(Some(Path::new("examples/rust_digger")))
}
```

`cargo run --release --example rust_digger` opens Digger's window; with
arguments it is the whole command line (`-- shot examples/rust_digger
--input ...`, `-- check examples/rust_digger`), and `cargo test` plays its
replays.

`Session::snapshot` keeps a game's state as it stands and
`Session::restore` brings it back (step 3, above; explicit since step 4,
as a Lua VM cannot be copied by cloning a handle to it).

## The Lua door

**Built** (2026-10-16, step 4 of the plan): Lua is the way in. The
`voxcade` program runs every game folder as a Lua game (`Voxcade` =
`lua::LuaGame<Shooter>`); More Blood, the Arena and the Bench are Lua games
with the shooter kit, `games/sokovox` (Sokoban in voxels) one without. `game.toml` is gone: a
game's settings are code (the user's choice: "nur im Code, fertig").

- **A game is a folder.** `conf.lua` (`function conf(c) ... end`) sets the
  settings over the engine's defaults: `c.title`, `c.physics.gravity`,
  `c.things.rifleman = { ... }`, `c.weapons.rifle = { ... }`, `c.actions`,
  `c.keys`. It runs in the sandbox; a misspelt
  setting or a wrong type is an error that names it (`c.things.gem.hit`).
  `main.lua` holds its handlers, `on_world_start()` (once, the run's
  states, no level yet), `on_level_start(how)` (a level begins: build it,
  place things; `how` is `"start"` until levels come), `on_tick()` (60
  times a second, after the kit's behaviors) and `on_draw(frame)`; both
  files
  are optional. (Version 2 of the API, `docs/game-model.md`: the
  old `load(world)`, `tick(world, input)` and `draw(world, out)` are
  refused with their new names.)
- **Sounds** (`src/audio/sound.rs`): a sound is a file of the game,
  `sounds/NAME.toml` (a few numbers a line: `wave`, `freq`, `sweep` and
  `jump` in semitones, `attack`, `hold`, `decay` in seconds, `volume`;
  only what differs from the defaults; no recordings: a `.wav` in
  `sounds/` is a plain mistake), called by its file's name (`level:sound("coin", at)`, a kind's `sound =
  "coin"`). Its words are one register with MML's instruments
  (`src/audio/words.rs`), a word both know meaning the same in both. A
  mistake names the file, the line and the word. The engine's and the
  kits' own sounds are files of the same kind built in
  (`src/audio/sounds/`, `src/shooter/sounds/`, `src/platformer/sounds/`);
  a game's file of the same name replaces one, `silent = true` silences
  it. `c.sounds` in `conf.lua` is refused (`voxcade sounds GAME
  --to-files` writes an older game's as files). Sounds count for no
  replay: they are in neither the fingerprint nor the state hash.
- **Music** (`src/audio/music/`): a song is MML text (the format at the
  top of `src/audio/music/mod.rs`, HELP's "A SONG IN MML"), played by
  SID-like voices and by a voice that sings: `@singer voice`, a formant
  synthesizer after Klatt (`src/audio/music/voice.rs`), a quoted text
  before a note the syllable that note sings, in phonemes split by
  spaces (`"s ae ng" e4`: the vowel on the beat, the consonants before
  it just before, the ones after it ending where the note is let go; a
  note without a text holds the vowel). Every instrument takes `drive`
  (decibels into a soft clipper, before the filters) and a bit crusher
  (`crush_rate`, `crush_bits`).
- **Levels** (`src/levels.rs`): the files in a game's `levels/` folder,
  all read when it loads, in the order of their names (numbers as
  numbers); a level of blocks, a map or a world (`.toml`, `.toml`, `.vvx`) as `c.world` says,
  with named places. `world:go_to(level, place)`, `world:restart(place)`,
  `world:win()`, `world:lose()` ask, and at the tick's end one ask counts
  (going on before starting over): `on_level_end(why)`, the next level
  cloned from what was read (`Kit::enter_level` keeps a ready-made run),
  `on_level_start(how)`. In a window the change fades (`c.transition`,
  `Session::fade`): no ticks run then, so recordings and replays never see
  it.
- **The game model's objects** (`src/lua/world/`; the whole list:
  `voxcade api`), globals in every script: `world` (the run:
  `world.state`, `world.tick`, `world:win`, `world:lose`), `level` (the
  level being played: `level.state`, `level:spawn(kind, x, y, { z,
  facing, state })`, `level:thing(id)`, `level:things(kind)`,
  `level:count(kind)`, `level:explode(at, r)`, `level:sound(name, at)`,
  `world:hitstop(ticks)`, voxels
  (`level:set_voxel`, `level:fill`, by colour `"#rrggbb"` or palette
  index), `level:solid`, the ground, `level.size`, `level.camera:follow`,
  `level.camera:look_at(x, y)`), `player` (a thing that goes from level
  to level: the kit's player, or a thing of `c.player.kind`; its
  `player.state` holds the score and `inventory`, items of `c.items`),
  things (`id`, `kind`, `x`, `y`, `z`, `facing`, `state` with `hp`
  where the kind declares it, `walk`, `place`, `face`, `jump`, `ahead`,
  `near`, `remove`, at the tick's end), `input` in
  `on_tick` (`dir`, `held`, `pressed`, `released` by the game's action
  names), `controls` (what the player plays with: `device`, `key` as
  their keyboard or pad names it; from their machine, so replays carry
  it like the keys held, `replay::Recorder`) and `screen` (`shake`,
  `flash`) everywhere; `on_draw(frame)` draws on `frame` (`model`,
  `cube`, `light`, `text`, an action in braces its key). The engine's own numbers sit in the states
  (`player.state.score`, `lives` and `hp` with a kit, `thing.state.hp`):
  read and written through to where the engine keeps them, so the
  replays' hashes stayed. A thing in Lua is one handle per thing (the
  same value wherever it comes from), kept in `level.state` or
  `thing.state`; `world.state` and `player.state` outlive it and refuse
  it. `world:read(name)` gives a file of the game
  folder while `main.lua` starts (Sokovox's levels); the files read count
  for the replays. `c.world.size` without a map makes an empty world the
  script builds.
- **The kit** (`c.kit = "shooter"`, `lua::host::Kit`): the shooter's
  behaviors run by themselves; the script sees its things through one
  trait (`lua::host::Host`), counts them, takes their hits, keeps the
  score. Kinds without a behavior are the script's, moved by `on_tick`.
  What its behaviors do to things it reports (`Sim::events`), and the
  script hears it (`on_hit`, `on_death`, `on_fall_out`) with the touches
  the engine finds (`on_touch`): what they mean is the game's code
  (points, pickups, checkpoints, a life lost, the helipad in More Blood's
  `main.lua`), the kit decides none of it.
- **Errors halt.** A script's error stops the game at its tick with file
  and line over the frozen frame; saving the mended file goes on where it
  stopped, the state kept. `shot` writes the frame with the error and
  ends with it, `check` and `film` end with it, a stopped run is not kept.
- **Hot reload.** Saving `main.lua` takes the new functions in and keeps
  the states (`on_world_start` and `on_level_start` do not run again; a
  start that a mistake stopped begins afresh once the file is mended). Saving `conf.lua` applies what can
  change while running; the kit, the actions and keys, the level, the
  markers and the player's start wait for a restart (F5, a maker's key) and say so.
- **One source for the API** (`src/lua/api.rs`): `voxcade api` lists it,
  `voxcade api --lua lua/voxcade.lua` writes the definitions for editors
  (LuaLS: completion and checks in VS Code, Zed, Neovim, `Conf` for
  `conf.lua` from the engine's defaults); every game folder has a
  `.luarc.json` pointing at them. A test keeps the register in step with
  what scripts see, another the file with the register.
- **Tools for agents.** `voxcade inspect <game> --frames N --input ...`
  plays without a window and prints how the game stands as JSON (tick,
  outcome, score, report, sounds, the error, the things and
  `world.state`). `voxcade mcp` serves `shot` (the frame as an image),
  `check`, `run`, `things` and `api` over MCP on stdin/stdout, each tool
  running the program itself. Claude Code takes it with
  `claude mcp add voxcade -- /path/to/voxcade mcp` (or, for one game
  folder, a `.mcp.json` with `{"mcpServers": {"voxcade": {"command":
  "voxcade", "args": ["mcp"]}}}`).
- **Proved the same:** moving More Blood, the Arena and the Bench to
  `conf.lua` and the Lua runner changed no replay's fingerprint (the
  effective settings are the same as before but for the kit), every
  replay played the same without a refit, sounds and music came out byte
  for byte, the frames pixel for pixel. The goal rule's move to `main.lua`
  changed only the fingerprint (`--refit`, every second the same, the
  user's two won runs among them).

## Replays, attract mode and films

Because the simulation is deterministic, a game is fully described by its
start and the input of every tick. Recording that input is a replay, a few
kilobytes a minute (Doom's demo files worked the same way, and the arcade
machines' attract mode).

- **Recording:** the engine writes down, tick by tick, what the player
  presses. A replay carries a fingerprint of the game's data (level,
  settings, scripts, models); a replay whose fingerprint no longer
  matches is skipped, not played wrong (a changed level or speed would make
  it run apart, as Doom demos did between versions).
- **Attract mode:** when nobody starts a game for a while, the title plays
  one of the game's replays, PRESS START blinking over it, as the arcade
  machines did. Any key goes back to the title.
- **Tests and bug reports:** a recorded run is a regression test (after a
  change it must play out exactly the same) and a bug report ("at 1:23 it
  goes wrong"); `shot` can stop a replay at any tick and show it.
- **Films:** a replay rendered without a window, frame by frame at 60 or 30
  per second, with its music and sounds mixed alongside, perfectly in step:
  a video for the website (MP4 or WebM through ffmpeg) or a short animated
  GIF for promotion (built in, no ffmpeg needed).
- Later: ghosts in racing games, replays after a boss.

**Built** (2026-10-15, step 1 of the plan, `src/replay.rs`):

- A replay is a TOML file: the keys held at every step in `shot`'s script
  format, the fingerprint of the game's data (the simulation's part of
  the settings in effect, `main.lua` as tokens, the level's files, the
  figures; a change to the look or the music, a comment or the layout of
  the code leaves it) and the simulation's state every second. The user's
  first run, won in 148 seconds, takes 22 KB. Since 2026-11-13 (the
  owner's D7) also the state the run ended in (`final_state`, after its
  last step and the console lines run after it), so the last fraction of
  a second is checked too, and the fingerprint's parts by name
  (`fingerprint_parts`: "conf.lua", each level, model and Lua file), so a
  replay recorded on other data says which files changed. Both are
  optional: an older replay plays as before, `check` saying how many of
  its last ticks went unchecked.
- The same on every machine: the simulation's sines and cosines come from
  the `libm` crate, not the system's maths library, which differs in the
  last bit between Linux, Windows and macOS. The builds on GitHub play
  every game's replays on all three.
- Every run played in the window is recorded: runs of 10 s or more are
  kept beside the settings (the newest 20), F9 (`voxcade run --maker`) puts one in the game's
  `replays/`. `voxcade replay` shows one in a window.
- `voxcade check` plays a game's replays without a window, the game
  drawn after every tick (a mistake in `on_draw` stops it as in a
  window), and says where one runs apart: the regression check for every later step
  (`replays/tests/` holds scripted runs; `cargo test` plays them all).
  A replay with nothing to check it by (no second's check, no final
  state) fails; one that cannot be read or played is said, the next one
  played, and the mistakes are the error at the end. `--update` records
  no run the hidden-state guard refuses; `--refit` writes the parts anew
  with the fingerprint where a replay has them.
- The high score table (2026-10-16): the ten best runs with three
  letters, picked like at an arcade machine after a run that beats a
  line; on the title the round goes title, table, replay.
- Attract mode: 30 s on the title without a key and the game's replays
  play under the title music, PRESS START blinking.
- `voxcade film`: a replay as MP4 or WebM through ffmpeg, its music and
  sounds mixed alongside, or as a GIF built in; `--max-mb` keeps a video
  under a chat's upload limit. The user's 148-second run: 1080p at 60
  frames a second in 81 seconds (275 MB), 720p under 50 MB in 38.

## 16-bit scale

More Blood draws at 8-bit sprite scale (figures 22 voxels tall). Round
shapes and the pixel-art shading (`model_shading`) pay off on bigger
models: rounded heads at 22 voxels were tried and rejected, a 5-voxel head
has too few voxels to round. The goal is 16-bit sprite scale (SNES, Mega
Drive, Amiga: figures 32 to 48 tall), at twice the resolution 8 times the
voxels per figure, and 8 times as many in the world if it gets finer too.

**Measured first** (2026-10-15, `shot` prints `frame: cpu ... | gpu ...`,
the GPU time per pass from timestamp queries; `games/bench`, made by
`tools/gen_bench.py`, is 40 of More Blood's figures and the Huey at twice
the resolution):

| | cubes a frame | CPU, building the frame | GPU, drawing it | loading |
|---|---|---|---|---|
| More Blood, playing | 41,000 | 2.3 ms | 1.2 ms (shadows 0.7) | 0.26 s |
| More Blood, title | 79,000 + 25,000 UI | 7.4 ms | 1.4 ms | |
| Bench, 16-bit figures | 246,000 | 21.7 ms | 3.1 ms (shadows 1.8) | 1.6 s |

The GPU has room to spare; the CPU does not (16.7 ms a frame at 60 Hz).
Every model voxel becomes a cube every frame, and of the bench's 21.7 ms,
10 go into checking each cube against the world and a hash set (no two
cubes in one cell). What to do, roughly by how much it buys:

1. The cell bookkeeping as a bit per cell instead of a hash set (as
   `render::hide_inner_faces` does): most of those 10 ms.
2. Only the voxels on a model's surface become cubes, found once when it
   loads: 56% fewer cubes for a 16-bit figure (33% at 8-bit, 11% for the
   Huey).
3. The quarter-turned shading of every model voxel worked out at load, not
   every frame.
4. Figures off screen are not built at all (More Blood builds all 40-odd
   enemies every frame, wherever they are).
5. Later, if still needed: each model frame uploaded to the GPU once and
   drawn per actor with its position and turn, so the CPU sends one entry
   per actor instead of thousands of cubes. The one-cube-per-cell rule then
   needs another answer.
6. The world at a finer grid: faces of the same colour merged in the
   meshes, a compact vertex format, chunks loaded as the view scrolls; the
   shadow pass is the GPU's biggest cost so far.
7. Loading: the edge shading takes most of the 1.6 s; it can be cached on
   disk next to the model or computed faster.

**After 1 to 4** (2026-10-15, done): grid cells hashed with a fast fixed
hash (`world::CellMap`, also for the world's chunks, which sped up the
simulation too: 1,000 ticks of More Blood in 95 ms instead of 245), only a
model's visible voxels drawn, the turned shading looked up, and models
outside the view and the shadow box skipped. Every frame checked
pixel-identical against before.

| | cubes a frame | CPU, building the frame | GPU, drawing it |
|---|---|---|---|
| More Blood, playing | 4,600 (was 41,000) | 0.2 ms (was 2.3) | 1.0 ms |
| More Blood, title | 31,000 + 25,000 UI | 3.0 ms (was 7.4) | 1.2 ms |
| Bench, 16-bit figures | 132,000 (was 246,000) | 6.8 ms (was 21.7) | 1.5 ms (was 3.1) |

The 16-bit scale now fits a frame with room to spare. Next where it
counts: the title's UI is rebuilt every frame (the logo, 25,000 cubes),
and points 5 to 7 above.

**Point 5 done** (2026-10-15): every model's opaque voxels, all frames,
lie on the GPU once (`render::ModelBank`); a frame sends one entry per
model (where, which quarter turn, flashing or not) and the vertex shader
places its voxels. Glass and models fading out stay cubes built every
frame (they are sorted far to near). The one-cube-per-cell rule is now the
drawing order: world, models, cubes, and at equal depth the first drawn
stays. Six scenes pixel-identical to before; in the glide from the title
Nick's soles now show on the fading dog tags (they used to lose the cell).

| | cubes built a frame | CPU, building the frame | GPU |
|---|---|---|---|
| Bench, 16-bit figures | 0 (41 models from the bank) | 2.0 ms (was 6.5) | 1.5 ms; 610M 11.3 (was 10.7) |
| More Blood, title | 0 (13 models) + 25,000 UI | 2.3 ms (was 2.9) | 1.1 ms |
| More Blood, playing | a few dozen | 0.0 to 0.1 ms (was 0.2) | 0.9 ms |

The small GPU pays half a millisecond for it (a draw per model instead of
one for all); worth four and a half on the CPU.

**Baked models** (2026-11-17, `c.effects.bake_models`, on by default):
each run of the bank (a frame, a figure's part) is listed once as the
faces of its voxels that are drawn, 12 bytes a face against a cube's 32
a voxel, kept with its model so a bank made anew bakes only what is new
(`render::baked`). A frame draws a face's four corners instead of a
cube's 24 a voxel, the scene only the ways that can face the eye; the
picture is the same (in the owner's big scene one pixel of two million
differs, by 5 levels: two faces at one depth drawn in the other order).
A run whose faces would take more than its cubes (thin ones: posts, a
voxel alone) stays cubes, so the bank never takes more memory than
before. 1920 x 1080, GPU a frame, the median of five, off against on:

| | RX 9070 XT | Radeon 610M | the bank on the GPU |
|---|---|---|---|
| BunnyFarm | 1.38 to 0.87 ms | 15.3 to 11.9 ms | 2.90 to 2.19 MB |
| Voxkeep (small, thin figures) | 0.27 to 0.27 ms | 3.95 to 3.94 ms | 0.70 to 0.65 MB |
| More Blood, playing | 0.74 to 0.73 ms | 8.3 to 8.0 ms | 2.88 to 2.34 MB |
| The owner's big scene (4.3 M model voxels drawn) | 16.9 to 5.3 ms | 135.4 to 39.5 ms | 76.9 to 37.5 MB |

(The 9070 XT also ran the big scene at a higher clock in some runs, 9.0
to 2.9 ms: the same third.) The CPU's frame is unchanged; the editors
keep their cubes.

Then the title (2026-10-15): the logo's letters, built every frame with
their rounding, grain and cracks (25,000 cubes), lie in the bank too and
are placed as they fall; the title's props found the ground under them
every frame, searching each column from the top, now once. Building a
title frame: More Blood 0.1 ms (was 2.9), the bench 0.1 ms (was 2.0), both
pixel-identical. What the CPU still builds every frame is small: the HUD,
the menus' pages and headings, bullets, debris and smoke, the props fading
in the glide from the title (1.0 ms for its 1.2 seconds).

**Blasts** (`shot` times every tick, the simulation and the chunk meshes
it changed rebuilt as the game does, and prints the slowest; `--at x,y`
puts the player anywhere): five grenades thrown at a watchtower and at a
hut.

| | slowest tick | simulation | remeshing |
|---|---|---|---|
| More Blood, tower | 1.2 ms | 0.24 ms | 0.97 ms (7 chunks) |
| More Blood, hut | 2.0 ms | 1.15 ms | 0.88 ms (6 chunks) |
| Bench (16-bit), tower | 2.5 ms | 1.81 ms | 0.71 ms (7 chunks) |
| Bench (16-bit), hut | 8.0 ms | 3.23 ms | 4.74 ms (45 chunks), then 5.9 to 6.8 ms (51 to 53 chunks) |

Harmless at 8-bit scale. At 16-bit, rebuilding chunk meshes was the cost
(about 0.13 ms a chunk): the debris settling back into the world touches
some fifty chunks in the ticks after a blast. The chunks are independent,
so they are now meshed on all the CPU's cores (2026-10-15; uploaded in
their order, so every run builds the same): the 45 chunks of the hut's
blast in 1.8 ms instead of 4.7, the fifty after it under 1 ms, More
Blood's whole level at the start in 68 ms instead of 310. The slowest
16-bit tick is now 5.0 ms instead of 8.0, most of it the simulation's own
spike (3.2 ms: the blast, the check for what no longer holds, the
debris).

That spike is down to 1.2 ms (2026-10-15, the same results to the pixel):
the blast first reads what it takes, then takes it in the same order (the
random numbers for the debris come in the same order); a chunk is looked
up once for its neighbours (`World::reader`); the check for what no longer
holds starts only from the cells that still stand next to the crater, each
once, and takes a cell standing on an unbroken column down to the ground
as held without searching; the cells use the fast fixed hash. Both reading
passes run on several cores for a big blast (`par::in_order`, results in
the order of the input, so the simulation stays the same): the check
gains a third, the ball's scan nothing measurable at this size (it got
cheap; starting threads costs as much), more for bigger blasts. The rest
must run in order: removing the voxels, drawing the debris's random
numbers, following what came loose. The slowest 16-bit tick is now about
2.9 ms (1.2 simulation, 1.7 remeshing), 2.2 since the uploads (see
Hardware). Chunks out of view could also wait a frame.

**From 2D sprites to voxel figures:** a converter that carves a voxel
figure from sprites seen from the front, the side and the back (the
silhouette of each view cuts the block, the colours come from the
sprites), frame by frame for the walk. Pixel art, drawn or generated, then
becomes a figure; it fits "everything 2D lifted into 3D".

## Hardware

The aim, the user's words: a "damn, this voxel engine is fast" moment,
the way Voxatron ran on a dual-core Mac without a graphics card. The more
older hardware runs the games without great loss, the better; what an
older machine cannot carry, quality levels take off (coarser shadows
first), never the game itself.

The development machine is far above that (Ryzen 9 9900X, 12 cores at up
to 5.6 GHz; Radeon RX 9070 XT), so every timing taken on it flatters. Two
checks bring it down to earth:

- The CPU's own graphics, a Radeon 610M with two compute units (weaker
  than a Steam Deck or an ordinary laptop's): put
  `MESA_VK_DEVICE_SELECT=1002:13c0 MESA_VK_DEVICE_SELECT_FORCE_DEFAULT_DEVICE=1`
  in front of `shot`.
- Fewer cores: `taskset -c 0` (one thread), `taskset -c 0-3`. Slower cores
  cannot be had without changing the system's settings: a core in an older
  or cheap laptop does roughly a half to a third of one here.

Measured (2026-10-15, 1920 x 1080):

| | here | one thread | Radeon 610M |
|---|---|---|---|
| More Blood, GPU a frame | 0.9 ms | 0.9 ms | 8.8 ms (shadows 3.8) |
| Bench, building the frame (CPU) | 6.5 ms | 6.7 ms | 6.7 ms |
| Bench, GPU a frame | 1.5 ms | 1.5 ms | 10.7 ms (shadows 3.8, scene 5.5) |
| Bench, hut blast, slowest tick | 1.1 + 1.1 ms | 1.4 + 5 ms | 1.1 + 1.0 to 1.5 ms |
| More Blood, meshing the level | 70 ms | 300 ms | 70 ms |

(The blast: simulation + remeshing.) What it says:

- More Blood keeps 60 frames a second on the smallest graphics with half
  the frame to spare.
- At 16-bit scale the CPU is the limit on older machines: building the
  frame (132,000 model cubes) ran on one thread however many cores there
  are, and a core at half the speed needed 13 of the 16.7 ms. Point 5
  above cured it: 2.0 ms here (so about 4 to 6 on such a core).
- Meshing scales with the cores (one thread: three times as long); two
  threads of one core gain nothing over one.
- On the small GPU the sun's shadow map (4096 x 4096) is the biggest
  pass: the first quality level, now in the settings (SHADOWS: high 4096,
  medium 2048, low 1024; `shot --shadows`). On the 610M More Blood's frame
  takes 9.2 ms on high, 7.3 on medium, 6.6 on low (the shadow pass 4.0,
  2.3, 1.5 ms: it also draws every face facing the sun, whatever the map's
  size); the shadows' voxel steps soften from medium on. Blur and
  composite cost 1.3 ms there, 15 times as much as here: the next level.
  On an integrated GPU the shadows start at MEDIUM when the settings
  file has no `shadows` key (`Settings::load_for`: what the player chose
  wins). VIDEO also has a FRAME RATE (UNLIMITED, VSYNC, 60, 30:
  `settings::FrameRate`, paced by `window::Pacer`).
- A blast's chunk meshes went to the GPU in new buffers every time; now a
  chunk keeps its buffers and the new mesh is written over the old, with
  room to grow when it outgrows them (`Renderer::upload`): a quarter less
  remeshing, and GPU memory no longer churns. A 10 ms spike first measured
  on the 610M came from `shot` itself: it sent the uploads only at the
  end, so those of all ticks piled up and the driver fetched and cleared
  new blocks of GPU memory. It now sends them every tick, as the game does
  every frame.

## Light in the grid

Built for 0.7 (the owner's names: `falloff`, `cone`, `blend`, `rotation`,
`pitch`, `shadows`, `c.light.lamp_shadows`; `docs/game-model.md`): lamps
walls stop, spots, a falloff of their own, and more than 16 lights a
picture (`src/render/lamps.rs`, `volume.rs`, `src/lamps.wgsl`,
`src/light.wgsl`, `movers.rs`). A light's `shadows` takes words,
`"static"`, `"all"` (moving things too, below) or `"none"`
(`render::LampShadows`); unset, the light takes the game's `c.light.lamp_shadows`
(`"static"` unless set), and a light's own word beats the game's. The
lamps' volume is built only when some light is stopped. The test hall
`games/lights` shows each of these, a subject a scenario (lamps at
walls, a diagonal wall, spots, the three words side by side, fog and
beams, glass and water, dancers, a posed figure, a walk through a beam,
out of view, headlights, 16 lamps and 50 figures); four are picture
tests.

- **Who pays.** A picture with at most 16 lights (`FORWARD_LIGHTS`) and
  none walls stop is drawn as before, by the scene's shader alone: a game
  without lamps (More Blood, BunnyFarm by day) pays nothing, byte for
  byte the same. The kits' flashes and explosions are bursts
  (`Light::burst`): round, through walls, as ever.
- **The lamps' pass.** Otherwise the opaque scene is drawn without its
  lights (`fs_main_lamps`), each pixel's surface written beside its
  colour (a 128-bit target: the colour the light falls on, the cell in
  front of its face, the way it is lit by); then the screen in tiles of
  32 pixels, each drawn once with the lights whose box can reach it (up
  to 32 a draw, `render::lamps::tiles`), each pixel's surface read once
  and its lights added by the scene's own sum (`light.wgsl`, shared);
  then the see-through cubes onto it, by the 16 nearest, through walls.
  Light volumes a light each read the surfaces once a light and blended
  as often: on the Radeon 610M that cost 9 ms at the station's 14 lamps,
  tiles 4.
- **Walls.** The part of the level its lamps reach (`volume::Reach`, a
  block of 64 voxels more each way) as a bit a voxel in a 3D texture
  (R8Uint, 2 by 2 by 2 voxels a texel) with five coarser copies, a bit
  for each child holding anything: the level's opaque voxels and its
  pieces' (`FrameData::lamp_pieces`, every one in view or not; a piece
  that brings a light stops none). Made when a picture first has a lamp
  walls stop (the station's 1280 by 1200 by 288 voxels in 92 ms on all
  cores, 60 MB; Voxkeep's 1 ms), brought up to date chunk by chunk where
  the world changes (a blast: the blocks round it, every copy, only those
  sent), made anew when a lamp reaches past it, the pieces change or
  another level comes (not when a model drawn the first time joins the
  model bank: `ModelBank::made`); at most 128 MB, past it said once and the lamps
  shine through. A face asks whether a lamp reaches it from the middle of
  the cell in front of it (lit or dark as a whole, as the sun's voxel
  shadows): a walk from the lamp past empty space by the coarse copies,
  begun at the copy as long as the way, down a copy into a cell holding
  anything, up one only once out of the cell above the one it left
  (`walk`; back into a cell found solid, a way half a voxel over a floor
  went down and up at every voxel and gave up after 256 steps taken as
  clear: a slab behind a wall 136 voxels on was lit; test
  `a_way_grazing_a_long_surface_still_meets_the_wall`); bounded by its
  cells, past them stopped. A lamp sitting in solid voxels walks from the
  nearest open voxel's middle within `HOUSING` (8 voxels), settled once a
  picture (`Volume::start`; buried deeper it lights nothing): each way
  leaving on its own went through a wall up to 8 thick, a lamp flush
  with a wall lit the room behind (test
  `a_lamp_flush_with_a_wall_lights_its_own_side_only`); a walk passing exactly through an edge or a corner
  where two of the cells across it are solid (a diagonal wall of voxels
  touching only at their edges) is closed there, one solid it only grazes
  (test `a_lamp_does_not_leak_through_a_diagonal_wall`); a face off the grid looks past the half voxel its
  turned piece grows by. A face of the level turned away from a lamp
  gets none of it (a way to the lamp would only go through a gap);
  anything else keeps a burst's little round the back.
- **The faces' memory.** Each face's answer for each lamp is kept
  (`seen`, a table of two words an entry by the cell, the lamp's exact
  place and the volume's state; room for a quarter of the picture's
  pixels to begin with, 4 MB at 1920 by 1080, where every subject of
  `games/lights` fits 512 KB), asked again when the volume changes or after a picture
  unasked (everything a picture shows is asked every picture; a lamp that
  moves asks under a new key each picture, so its old entries make room at
  once: they stayed 8 pictures until 2026-11-18): a still scene walks its
  ways once. The fog's beams' points (below) are kept in it too (a point
  and the face in front of its voxel ask the same); what they take comes
  by the voxels along the beams more than by the pixels (a hall of 14
  lamps in fog, 400 voxels across, took 4 million at any picture size).
  Room for them up front, 16 a pixel of the beams' target, took 134 MB
  for a small hall at 1920 by 1080 (until 2026-11-19). An entry's slot
  is its key mixed (a mask: the memory is a power of two), the next one
  tried a step apart its second key gives, 32 at most, the first empty
  one ending a lookup (a stale one does not: ended there, an answer
  behind it was walked again and kept twice, and the beams of a hall in
  fog took a quarter longer); a stale slot whose key another face's
  holds is taken back. Found crowded (an answer found no room within 8
  slots, `GROW_AT`) while it holds fewer than 8 answers a pixel, past
  that when one found none at all (the shader counts both), it is made
  twice as large from the next picture on (read every 8 pictures), up to
  2^26 entries (512 MB, `lamps::MOST_FACES`) or the GPU's largest
  buffer, and only past that said once (`Renderer::too_small`; `shot`
  lets it grow until it fits before it times and says its size; tests
  `the_lamps_memory_grows_until_the_beams_fit`,
  `the_lamps_memory_starts_small_and_is_let_go`). Grown only when none
  found room, a memory standing crowded drew the beams of a hall in fog
  10 to 15 % slower than with room to spare; so a hall in fog takes 67 or
  134 MB at 1920 by 1080, FOG HIGH, a level of 1600 by 900 voxels with 16
  lamps of radius 300 268 MB, both as fast as with the room they had
  before. Everything of the lamps (the memory, the volume and its thin
  things, the targets, the moving things' room) is let go after 600
  pictures in a row without their pass (`Lamps::used`, `LET_GO`: ten
  seconds at 60 a second, so a lamp flickering on and off keeps it). Every
  device asks the adapter for a storage buffer and a buffer that large,
  never past what it gives (`render::device_limits`; wgpu's defaults
  held 2^23, 64 MB; test
  `the_device_holds_the_lamps_memory_as_far_as_the_gpu_allows`): a level
  of 1600 by 900 voxels in fog with 16 lamps of radius 300 to 375 takes
  134 MB at 1280 by 720 and 268 MB at 1920 by 1080, FOG HIGH. Every answer is
  its walk's, so `shot` draws the same to the byte. Without it the
  station's lamps took 23 ms on the 610M, with it 4. A lamp new to the
  memory (its first picture, or moved since the last) is asked for first,
  by a pixel every 4 each way (the lamps' asking, `ASK`): all the pixels
  of a face asking at once in the pass would each walk, as an entry being
  written is no answer yet. Two headlights moved every picture cost the
  610M's lamps 4.4 ms still, 11.6 moving, 6.7 with the asking (1.2 of it
  the asking); a lamp that stays is never asked again.
- **Moving things** stop no `"static"` light: they move every picture
  (written into the level's volume each picture and every face's memory
  with them, as an earlier trial did, it cost a hitch a frame), and a
  figure's backside keeps a little of a lamp in front of it.
- **`"all"`** (the owner's word, 2026-11-18): moving things stop the
  lamps whose word it is too, so figures, the kits' things, models drawn
  by `frame:model` (turned, posed, parts at rest) and opaque
  `frame:cube`s cast their shadows from those lamps on floor, walls and
  each other and cut their beams in fog; a game affords it for chosen
  lamps. What moves is gathered every picture in view or not
  (`Drawing::movers`, from the first picture with such a lamp on), each
  thing's draws together, and kept where an "all" lamp reaches
  (`FrameData::lamp_movers`): a figure off the screen whose shadow falls
  into it keeps it, no pop as it leaves (test
  `what_moves_out_of_view_still_stops_a_lamp_whose_shadows_are_all`).
  Each model drawn (a figure's part each) and each opaque cube is a
  solid of its own (`render::movers`, `Solid`): its voxels as bits in
  its own grid (a model's shared by its draws, sent only when they
  change) and the map from the level into that grid, the draw's turn and
  place exactly as the shader places its voxels; not the level's pieces,
  which are the level's volume; glowing and see-through voxels are in
  none. A box round each thing (cubes boxed by blocks of 8) and each
  "all" lamp's list of the boxes it can light (its reach and, a spot,
  its cone; `Movers::lists`, its place in the light's `edge.y`). After a
  face's kept answer from the walls such a lamp walks, afresh every
  picture (`movers_stop`), each solid of the boxes its way meets, the
  way taken into the solid's grid and walked through its cells as the
  level's (`solid_stops`); a beam's ray first finds which boxes can
  shade any of its points (the triangle of the lamp and the ray's
  stretch against each box, `shading`), and its points walk only those.
  A shadow is the exact shape of what casts it, turned as it is drawn,
  and a face is lit or not as a whole, as by the walls: a face of the
  level asks from the middle of the cell in front of it, so a figure's
  feet shade the floor at their side (no peter-panning there); any other
  face from its own middle a quarter voxel out (`FACE_OUT`), a voxel
  shaded smooth a voxel out of its middle along the way it is lit by,
  exactly (a turned face's point written beside its cell by
  `fs_main_lamps`, in 256ths; `movers_from`), and a face turned from the
  lamp is in its own shadow (`movers_stop_face`). So a part turning
  slowly moves its shadow's edge smoothly and its own faces' points
  smoothly: a face changes once as an edge passes (test
  `a_part_turning_slowly_shades_each_face_once`). Until 2026-11-19 the
  things filled a second bit volume laid out as the level's, a cell
  where its middle lay in a turned voxel, and a moving face asked from
  the cell in front of it a voxel out, looking past what moved right in
  front of it: a thin turned arm left gaps between its cells that came
  and went, a face's start jumped a voxel as it moved and went in and out
  of the part beside it, and faces blinked for a picture where a part
  nearly touched another (a figure turning slowly at a quarter degree a
  picture under a spot, 120 pictures at 960 by 540: 276 pixels blinked,
  185 of them inside a face; after, 21, none inside a face, on edges
  between two faces; in the test 170 inside a face, after none). A lamp
  inside what moves (a car's headlight in its body) leaves it first
  (`HOUSING`, 8 voxels). Chosen over a depth map per lamp (a cube map
  per round lamp, six passes of every moving model, a bias to tune per
  lamp, soft by its texels, not on the grid): the solids cost a small
  upload a picture whatever the number of lamps, and their shadows are
  the walls' own. Long shadows from low lamps (a car's headlights) are
  true and busy: a figure's legs drawn out over the floor. Costs at
  1080p (the test hall, spots straight down over the floor, figures
  walking; GPU ms of the picture, without fog / with FOG HIGH, "static"
  against "all"): 4 lamps and 50 figures 0.56/0.74 against 0.54/1.05 on
  the strong GPU, 9.0/12.9 against 10.1/18.5 on the Radeon 610M (FOG
  LOW, its default: 11.3 against 14.2); 16 lamps and 50 figures
  0.72/1.09 against 0.83/2.31, and 16.5/25.7 against 19.8/43.2 on the
  610M (LOW: 20.3 against 29.9; measured with the volume; the solids,
  2026-11-19, the scenario `load`: the beams 1.31 to 1.53 ms on the
  strong GPU, 21.3 to 23.1 on the 610M, the faces the same). The beams
  walk them at every point where a ray passes a thing's shadow, unkept;
  most of the cost. Limits: solids and bits past `movers::MOST_BYTES`
  (32 MB) stop no lamp, said once; past `MOST_BOXES` things are boxed
  together by blocks, past `MOST_LISTED` listed boxes a lamp walks every
  box; see-through and glowing voxels cast nothing. Not built: the
  faces' memory keeping moving answers, a cheaper beam (a walk shared by
  neighbouring points).
- **Fallbacks, said once:** a GPU without storage written from a fragment
  shader draws its lamps through walls; lamps reaching further apart than
  the volume's bound too. What the GPU cannot give (`lamps::made`: out
  of memory, past what the device takes) is said, not the end of the
  program (wgpu's default): the memory stays as it was, walls not had let
  the lamps shine through, moving things without room stop no lamp
  (test `what_the_gpu_cannot_give_the_lamps_is_said_and_drawn_without`).
  A pass with walls that would not build is said in wgpu's words, and a
  test fails (a mistake in its shader once drew no walls, quietly).
- **A light at a thing** (`frame:light`'s `at` a thing, `Light::of`) is
  not stopped by that thing's own solids with `"all"`, as a piece's own
  voxels stop none of its lights; its faces turned from the light are
  lit as by `"static"`; other things still cast its shadow (test
  `a_lamp_a_thing_carries_is_not_stopped_by_it`).
- **A spot's tiles**, the lamps' and its beams', are those its cone can
  light (`Light::bounds`, the apex and the cap the cone cuts from its
  ball), and its cone is tested before the rest of its light: the test
  hall's `spots` 6.8 to 3.6 ms of lamps on the Radeon 610M at 1920 by
  1080, `load` 17.1 to 10.2, the pictures the same.

Later, not built: light flowing through the grid from glowing voxels
(Minecraft's block light), sky light kept out of rooms, mirror glass (all
tried in a trial branch, 2026-11-17); the sun's beams in fog, moving
things cutting beams, fog in the editors' views and the IDE's picture.

**Fog and beams** (2026-11-18, the owner's names: `c.light.fog_density`,
`fog_color`, `fog_anisotropy`, `fog_glow`, `frame:fog`, a light's `fog`; the
classic smooth way, the owner's choice over beams in voxel cells;
`render::fog`, `src/fog.wgsl`, `fog_beams.wgsl`, `fog_composite.wgsl`).

- **Who pays.** A picture with fog (a density above 0) is drawn by the
  lamps' pass, which keeps the scene's depth; a picture without fog runs
  no pass and no pipeline of it, byte for byte as before, and the
  player's FOG OFF draws none at all.
- **The beams** go into a target half (FOG HIGH) or a quarter (LOW, the
  start on the processor's own graphics, `Settings::load_for`) the
  picture's size, after the see-through cubes, by the lamps' tiles and
  walls: along each pixel's ray, the stretch within a light's ball and a
  spot's cone, the light each point gets times what the fog sends on to
  the eye (Henyey-Greenstein, `fog_anisotropy`), less what the fog took
  on the way. The fog takes its share by the density alone, the beams'
  own light too; `fog_glow` scales what the fog sends on to the eye (1 as
  the air would): above 1 a beam shows brighter, up to the brightest light
  the lamp gives along the ray (times its `fog`), never brighter, so a
  higher glow never makes any part of a beam fainter; a stage's haze
  (`fog_glow = 4`, a quarter of the density) has bright beams in a clear
  room. Until 2026-11-18 the beams also took their own light by the
  density times the glow: seen through much air (the length of a hall)
  a glow above 2 made every beam fainter, at 16 gone (games/lights' fog
  stage, measured; test `a_higher_glow_never_makes_a_beam_fainter`). No noise: the points are where the ray crosses planes
  across the camera's way every 4 voxels (8, 16 for a light wider than
  48 of them), fixed in the level, so a scrolling view and a turning spot
  take the same points; the spot's hard core's edges are points too; the
  light between two points is added up as a straight line; a lamp walls
  stop lights a point only where its walk reaches the middle of its voxel
  (the faces' memory asked, under keys of its own), and where that changes
  between two points four halvings find where: a wall's shadow in the fog
  steps voxel by voxel, as the faces' light does, fixed in the level (the
  shadow's very edge walked from the point itself was clean but cost the
  610M 4.4 ms more at LOW, 12 at HIGH). A wall a voxel thin stops a beam
  even slanting (a line of voxels touching at their edges, the walk's rule
  below; test `a_wall_a_voxel_thin_stops_a_beam`). A shadow thinner along
  the ray than the planes' spacing, between two points that both see the
  lamp (a post's, seen edge on), showed as dashes or not at all: points a
  voxel apart everywhere cost the 610M 15 ms of beams at LOW instead of 3.
  So the thin things are found (`render::thin`, 2026-11-18): a voxel is
  thin where its run of solid voxels is shorter than 4 one way and
  shorter than 16 another (a post, a rail, a short
  wall; a long wall or a floor casts a thin shadow only from a lamp in its
  plane and is left to the planes; what lies within 16 of solid voxels
  both ways along x and along y counts as solid, since the volume holds a
  piece's shell, the voxels it draws), read from the lamps' volume whole rows
  of voxels at a time, kept by blocks of 8 voxels, made the first time a
  picture of fog asks (a level 1024 by 1024 by 64 in 52 ms on all cores),
  brought up to date where the volume changes. Each light's row lists the
  balls round the blocks it lights (at most 16, more taken together in
  bigger blocks, each with the spacing its thinnest voxels need: a voxel
  for a run of 1, 2 for 2 or 3), and along a ray within the cone from the
  lamp round any of them the points lie on planes that far apart, at most
  16 more a ray a lamp (past that the spacing doubles), fixed in the
  level too (test `a_thin_rail_darkens_the_beam_behind_it`). Only at FOG
  HIGH (`render::Fog::thin`): at LOW the beams are drawn without that
  work, the picture and the time as before thin things were looked for
  (a thin thing's shadow in a beam as dashes again; the scratch hall's 14
  lamps and two spots on the 610M at 1920 by 1080 10.8 ms of beams at LOW
  against 15.4 with the thin things' points). Things 4 to
  15 voxels thin are left to the planes even for a wide lamp whose points
  lie 8 or 16 apart (taking them too cost two spots of radius 260 in the
  scratch hall 2.5 times their beams on the 610M). A picture none of
  whose lamps lights a thin thing draws its beams with a pipeline without
  that work (it cost a ray that never meets one too: 16 lamps' beams on
  the 610M 10.4 ms against 8.8), and costs what it did. Where a lamp
  lights one, it costs its points: the test hall's fog stage with two
  posts in a beam, the 610M's beams 4.9 ms at HIGH against 6.3 (1.5
  against 2.0 at LOW); the scratch hall of 14 lamps and two spots
  (2026-11-19, low walls 3 thick and a diagonal wall a voxel thin within
  their reach) 33 against 48 ms at 1920 by 1080 HIGH, 11.2 against 16.4
  LOW; its two spots alone 6.3 against 13.1. Not yet: thin moving things
  (an arm) in an "all" lamp's beam keep the planes.
- **Laid over** by the composite's own pipeline for fog (before the UI,
  which is never fogged): the scene times what the fog left of it, the
  fog's colour for the rest, the beams from the four nearest of their
  pixels weighed by how alike the depth they were worked out at is. What
  glows (the blurred glow target) is dimmed as the scene is, by what the
  fog left at the pixel it shines on (`post.wgsl`'s `finish_seen`): a
  torch in thin fog still blooms, in thick fog far less.
- **Glass and water** (the see-through cubes) in a picture of fog: their
  depth is drawn first (`glass_pass`, the scene's `vs_inst`, nothing
  else), the beams are worked out before the nearer see-through cube and
  behind it apart (`fs_beam_glass`, two targets), the fog behind it is
  laid over the scene before the cubes are drawn onto it
  (`fog_far.wgsl`), so they tint and dim it, and the composite lays the
  fog before them (test `a_beam_behind_glass_is_seen_through_it`). Beside
  the glass the picture is the one without it, byte for byte; a picture
  of fog without see-through cubes runs none of it.
- **Measured** (the trial hall, five lamps walls stop, 1920 by 1080): the
  Radeon 610M without fog 9.6 ms, LOW 14.2 (beams 3.1, the composite
  2.0 against 0.5), HIGH 21.2 (beams 10.1); Voxkeep's torches in fog 7.1,
  LOW 10.7 (its flames are see-through cubes: the fog behind them 0.15),
  HIGH 14.1. The strong GPU: 0.6, LOW 0.8, HIGH 0.9. Two headlights moved
  every picture: the lamps 6.7 ms (4.4 standing), the beams 6.7 at LOW
  (3.1 standing) and 20.2 at HIGH (9.9): each point of a moving lamp's
  beam walks anew, not yet asked for first.

## Multiplayer

Decided with the user (2026-10-29); the plan in stages, with making a
game together in the IDE (GAME JAM), is `docs/multiplayer.md`.

- **On one screen, as at an arcade machine:** 2 to 4 players at one
  machine, no split screen. One camera follows the group; the camera's
  height stays fixed (no zoom), so the screen's edge holds the players
  together as Gauntlet's and the Turtles' did, scrolling only as far as
  everyone can follow. One keyboard can drive two players (WASD and the
  arrows), so one person can try it.
- **Over the network: 8 as the aim, not a wall.** Only inputs travel, a
  few bytes a player and tick; the host gathers and sends them out, which
  carries more than 8. What grows with players is rollback (more guesses
  go wrong), so a game with many players or slow lines can play in
  lockstep with a few ticks of input delay instead, as strategy games do.
  Players at one machine and players over the network mix: every source
  of input is a player's place.
- **Co-op, versus, teams: the game decides,** the engine carries every
  form: each player may be on a team of their own, and the kits' rule of
  who hurts whom follows the players' teams; the high score table takes
  the team's score or each player's, as the game says.
- **Reaching each other:** first an open port on the host (a LAN, or a
  port forwarded). Later what everyone does: a small server of our own
  that brings players together (lobby, matchmaking) and, where two
  machines cannot reach each other directly, relays their packets; most
  connections then go direct after it introduced them.
- **Tried without a window:** scripted input for each player in `shot`,
  `inspect` and replays, and two or more sessions in one process over a
  simulated network (delay, loss, jitter); `check` plays every replay
  with forced rollbacks too, which must come out the same.

The plan as it was written first:

Up to 8 players; one of them hosts the game (plays and serves at once),
over a LAN or the internet.

- **Inputs, not the world.** The simulation is deterministic, so every
  machine runs the same game from the same start, and only the inputs
  travel: the host gathers each tick's input from every player and sends
  them out, a few bytes a player and tick, however much is blown up. (The
  other way, sending what changed, would mean sending the voxels of a
  destructible world.) Strategy games like Age of Empires played over the
  net this way.
- **Rollback hides the delay**, as fighting games do (GGPO): each machine
  guesses the others' inputs (the last ones again) and plays on; when the
  real ones differ, it goes back to the last tick everyone agreed on and
  simulates forward again. A tick costs 0.08 ms at 8-bit scale, so going
  back eight ticks costs under a millisecond; blasts and the chunk meshes
  they change need care there.
- **What it asks of the engine, to be decided early:** the simulation
  bit for bit the same on every machine (no platform maths in it: sin,
  cos, atan2 and powf differ between systems, so the simulation gets its
  own); its whole state saved and restored quickly (which also gives save
  games and seeking in replays); the same game data everywhere (the
  fingerprint, as for replays); a checksum every few ticks to catch a game
  that ran apart.
- **The same machinery as replays:** a replay is the recorded inputs of
  one player, a network game the live inputs of several. Joining a running
  game: the host sends a saved state, then the inputs.
- Several players on one screen with gamepads (couch co-op) comes almost
  for free. Lua scripts already run inside the deterministic simulation.
- Open: the transport (UDP; getting through routers, a relay, or a
  platform's networking), the lobby.

## Building blocks by genre

| Genre | Camera | Movement | Systems |
|---|---|---|---|
| Top-down shooter (More Blood) | follows the player | 8-way, strafe | aiming rule, enemy behaviors, spawners, pickups, bosses |
| Vertical and horizontal scroller (1942, R-Type) | scrolls by itself | ship in the screen | waves, bullet patterns, power-ups |
| Jump & run, side scroller | from the side, follows | platformer controller | moving platforms, collectibles, checkpoints |
| RPG, adventure, dungeon | follows, or screen by screen | 4/8-way, on a grid | dialogue, inventory, stats, quests, saving, mouse pointing, light in the grid |
| Real-time strategy | free panning | units | selecting, orders, pathfinding, fog of war |
| Simulation, city builder (SimCity, Anno) | free panning | none | tile grid, buildings, economy tick, overlays |
| Racing | follows, or from above | vehicles | tracks, checkpoints, laps, drivers |

Shared by most: the camera kinds, mouse input with picking into the voxel
world, pathfinding, the tile grid, dialogue and saving.

## The IDE (phase 2)

Voxcade gets its own IDE (the user's vision of 2026-10-16, replacing the
earlier idea of separate editors that feel like a game; the research and
the estimates are in `ide.md`):

- A very stripped-down VS Code for Lua games only, without distraction:
  "I am making a game, not learning an IDE".
- On the left only Logic (the Lua code) and Assets; no tabs, but it
  remembers what was open and where, above all unsaved work; one save for
  everything, done by itself.
- Built-in editors for voxels, for the height and material maps, and for
  levels; Play and Build.
- A console running Claude Code or another agent, with the IDE's MCP
  server: the agent sees what the user sees and can show things.
- Whoever writes Rust uses the library and needs no IDE.

Recommended by the research: a native app on the engine (SDL3, wgpu,
egui; a Lua editor talking to LuaLS; a terminal from `alacritty_terminal`
and `portable-pty`), beginning with a skeleton (code, terminal, MCP, play,
build) of about 3 to 4 person-months, then the level editor and the voxel
editor. The editors write the same files as everything else (`map.toml`,
PNGs, `.vvx`), never `conf.lua`.

## Plan

Phase 1, the engine:

1. **Replays** (done, 2026-10-15): recording, playing back, the attract
   mode on More Blood's title, films (video and GIF), the same on every
   system. Every later step is checked against the recorded runs.
2. **Library and program apart**, the game API drawn (done, 2026-10-15,
   "The game API" above): the engine knows no shooter words, More Blood
   plays as before (its replays bit for bit), and Digger is a second game
   in Rust on the engine alone.
3. **Things and parts** (done, 2026-10-15, "The game model" above): More
   Blood's kinds are definitions over built-in parts and behaviors (a
   shooter kit), its weapons data; the whole state is kept and brought back
   in memory. The recorded replays play out exactly the same.
4. **Lua 5.4** (done, 2026-10-16, "The Lua door" above): sandboxed,
   deterministic, hot reloaded, errors halting the game; `conf.lua`
   instead of `game.toml`; a whole game in `main.lua` (Sokovox) or a
   game with a kit (More Blood, whose goal rule moved to Lua); one source
   for the API (LuaLS definitions, `voxcade api`) and `voxcade mcp`.
5. **Jump & run** (done, 2026-10-17) as the second genre and the proof of
   flexibility: a side camera (its height follows the player out of a
   dead zone, `c.camera.follow_height`; it stops at the level's edges),
   the platformer kit (`src/platformer/`, `c.kit = "platformer"`: a hero
   with coyote time, a jump buffer and a jump cut, ladders (turned to
   them), riding, ledges caught and climbed (`grab`);
   walkers, hoppers, flyers, pickups, moving platforms, props; since
   2026-10-22 it only moves things, what a stomp, a pickup, the door,
   spikes, a fall and a life mean is Voxfox's `main.lua`) and Voxfox, a
   small level from the Sunny Land pack (removed 2026-10-23: its level
   was built for planks jumped through from below, which voxels do not
   have; Voxkeep is the kit's example since). Its
   level is built from voxel tiles, blocks in a 3D grid shaped by hand in
   Voxcade's own editor (levels and blocks), pulled forward from phase 2
   (`tiles.md`).

Phase 2, the IDE (`ide.md`): the skeleton first (the shell with Logic and
Assets, the Lua editor, the terminal with an agent, MCP, play and build),
then the level editor and the voxel editor; mouse and picking pulled
forward from phase 3.

Alongside, as needed: the 16-bit scale (the list above, measured with
`shot` and `games/bench`, on the small GPU too, see Hardware), quality
levels when a game needs them, and the sprite converter. Early, because
everything after builds on it: the simulation bit for bit the same on
every machine and its state saved and restored (multiplayer, save games,
replays); done with step 3 in memory (snapshots), the file format waits
for save games.

Phase 3, more genres and multiplayer (up to 8, one hosting): mouse and
picking, pathfinding, the tile grid, then
an RTS or city builder demo, a scroller, a racer; starter templates per
genre (the IDE's NEW, from `templates/`), a guide for people and
agents, packaging.

More Blood grows into a full game alongside, on whatever the engine has
by then.

## Open questions

- **A turned camera** for RTS and city builders: players expect to turn the
  view, and actors must turn with it (see the camera yaw rule in
  `CLAUDE.md`). Decide before phase 3.
- **The Doom homage's view:** from the first person (the engine knows only
  cameras from outside so far) or a voxel Doom seen from above.
- **Mouse input** arrives with phase 3 at the latest (RTS, sims,
  adventures, editors).
- **Generators:** levels are generated by Python scripts today. Lua could
  write them too, so a game maker needs one language.
- **Save format**, **packaging** (the model: LÖVE's `.love`, the game
  folder zipped into one file, fused with the program into one executable)
  and **licence**.

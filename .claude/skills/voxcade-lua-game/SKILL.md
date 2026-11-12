---
name: voxcade-lua-game
description: >
  Make or change a game for Voxcade in Lua: a new game folder, conf.lua
  (kinds of things, the kit, the level, keys, camera, light, title, high
  scores, sounds, music), main.lua (on_world_start, on_level_start,
  on_tick, on_draw, the
  globals world, level, player, input, screen, controls, the states),
  the deterministic sandbox's rules, and how to see and check the result.
  Use this whenever a game's rules, things, enemies, pickups, levels,
  controls or settings are made or changed, when a new game is started
  (ein neues Spiel), when a Lua error stops a game, or when the user asks
  how something is done in a game's conf.lua or main.lua, even if they
  never say "Lua".
---

# Games in Lua for Voxcade

A game is a folder under `games/`, after LÖVE: `conf.lua` sets it up,
`main.lua` is its own rules, `.vox` models beside them. The `voxcade`
program runs every folder as a Lua game. More Blood, the Arena and the
Bench play with the shooter kit, Voxkeep with the platformer kit, Sokovox
is all Lua (about 350 lines). Look at those for working examples.

The engine is deterministic so that replays, `shot`, `inspect` and
`check` give the same result every time and on every system. Most rules
below exist for that; keep them and you get replays and tests for free.

## A new game

Copy the template and rename it:

```bash
cp -r .claude/skills/voxcade-lua-game/assets/new-game games/NAME
./target/release/voxcade shot games/NAME --frames 90 --input "right:40;right+jump:10;none:40" --out shots/NAME.png
```

It runs as it is: a world made in code, a floor, the player (a thing of
`c.player.kind`, drawn as a column of voxels until it has a model) that
walks and jumps. Keep its
`.luarc.json`: it points LuaLS (VS Code, Zed, Neovim) at
`lua/voxcade.lua` for completion and checks. Credit any art or music
from elsewhere in the game's `README.txt` (and on screen when the licence
asks, like Sokovox's levels).

## conf.lua: the settings

`function conf(c) ... end` sets values over the engine's defaults; a
misspelt key or a wrong type is an error that names the table
(`c.things.gem.hit`). Where to find every setting:

- `src/config.rs`: every section and field with its doc comment (the
  source of truth). `lua/voxcade.lua` (class `Conf`) lists the common ones.
- `src/shooter/kit.toml`, `src/platformer/kit.toml`: the kits' kinds (and
  the shooter's weapons) written out with every number and a comment, to
  copy; a game has only what its `conf.lua` writes (a kind's unset numbers
  are its behaviour's defaults, lengths and speeds scaled to the kind's
  height; a weapon is all the game's). `voxcade kinds <game>` prints every
  number in effect.
- The kit, the actions and keys, the level, the markers and the player's
  start only take effect after a restart (F5); the rest while it runs.

Lengths and speeds are in voxels and voxels a second, and each game picks
its own scale (Arena figures are 10 voxels tall, More Blood's 22,
Voxkeep's 14). Nothing is converted; set gravity, speeds and sizes to fit.

### Models: small, plain, every voxel on purpose

The figures are small and plain, the 8-bit charm in clean voxels: about
10 to 20 voxels tall (the templates' hero 16 in blocks of 10, Voxkeep's;
the arena's 10, More Blood's 22). A try at 16-bit size (24 to 48 voxels)
had less charm and was harder to make (the user, 2026-10-23). Small does
not mean careless: the user hated coarse figures made without intent.
When you make a model (in a script or by hand in the voxel editor):

- Pick `c.camera.view_width` so it reads; `c.camera.view_height` fixes
  the height shown instead, the width following the window (a vertical
  shooter's field).
- One colour a material: the engine lights the model (sun, shadows, its
  model shading); never paint light and shade into the colours (the
  user). Colours differ for materials and designs (eyes, a stripe, a
  glowing light); a line a painter would draw stands out or sinks in.
- Clean voxel forms, the voxel charm: bevelled boxes and limbs of square
  section rather than ragged round shapes, mirror-alike when standing,
  parts one voxel thin where they belong (antennas, fins, edges). The
  voxcade-models skill has the method and the tools.
- Look at it close up in the voxel editor and from the game's camera
  (`shot`), several rounds. What a generator makes at first is a
  placeholder: say so.

### The level (`c.world`)

| How | Setting | For |
|---|---|---|
| made in code | `c.world.size = { x, y, z }`, then `level:fill` in `on_level_start` | small or rule-built worlds (Sokovox) |
| one scene | `c.world.file = "world.vox"`, things placed by marker colours (`c.markers.rifleman = 240`) | a small hand-made level (the Arena) |
| a map | `c.world.file = "map.json"`: ground from height and material PNGs, `.vox` prefabs | big levels (More Blood, written by `tools/gen_moreblood.py`) |
| voxel blocks | `c.world.blocks = { file = "level.json", margin, below, backdrops }` | tile-built levels (Voxkeep's `levels/`; `voxcade import`, `voxcade edit`) |

### Kinds of things (`c.things.NAME`)

```lua
c.things.opossum = { behaviour = "walker", model = "opossum.vox", width = 14, height = 16,
                     frames = { walk = { 0, 5 } }, effect = "poof.vox" }
c.things.guard = { like = "rifleman", hp = 4 }     -- a kind built on another
c.things.crate = { model = "crate.vox" }           -- no behaviour: main.lua moves it
```

- Turning: models stand in quarter turns on the grid unless the game
  chooses otherwise. `turns = "free"` in a kind draws its things facing
  exactly their way (a diagonal too); `frame:model(file, thing, index,
  { turn = 45, pitch = 10, roll = 20 })` turns a model further in
  degrees about its middle (round clockwise from above, the nose up, the
  right side down: a ship banking with its speed across). Only the
  picture turns; the voxels leave the grid, never change size.
- A kind with a `behaviour` belongs to the kit, which moves it; without
  one it is the script's (`thing:walk`, `jump`, `face`, `place` in
  `on_tick`). `thing:enable("walker", false)` switches a kit thing's
  behaviour off (the hero's too: a stun, an end walk): physics and the
  script move it then, until it is switched on again or the level ends.
  `thing:launch(way, speed, rise?)` sets a thing moving (a dash, a wall
  jump, a knockback; the kit steers on unless switched off),
  `thing.wall` is the way to a wall beside it or nil.
- Poses: any figure of the platformer shows `thing.state.pose` (a name of
  its `frames`) over its own animation while set, `nil` gives its own
  back (`player.state.pose` for the hero); frames a behaviour does not
  play are poses. A set of frames goes round at the kind's `fps` (10), or
  as it says: `idle = { first = 0, last = 1, fps = 2 }` (a slow breath
  beside a quick run), `walk = { first = 1, last = 4, stride = 6 }` (a
  frame every 6 voxels walked: the steps match the speed), `pull = {
  first = 9, last = 11, loop = false }` (once, its last frame held); a
  pose plays from when it was set. The shooter's figures play `idle`,
  `walk`, `jump` (the player's in `c.player.frames`), a gunship `idle`
  and `fly`; `frame:model(file, thing)` without a frame plays the kind's
  `idle`, `walk`, `jump` the same way. Sets left out are the model's own
  layout (0 standing, 1 in the air, the rest walking). A kind's `effect =
  { model = "poof.vox", fps = 20 }` says its speed too; a shooter pickup
  bobs `bob` voxels, `bob_ticks` each way; a soldier's steps raise dust
  (`stride`, `step_dust`, `landing`, `landing_dust`; the player's in
  `c.player`), and the shooter's player and the jump & run's hero blink
  while nothing hurts them (`blink`: ticks shown, as many not; 0 steady).
- Figures of parts: a `.vox` whose objects have names (head, body,
  arm_left ...; MagicaVoxel's object names, `tools/figures.py`'s
  `save_parts`) is drawn part by part, each turning about where it meets
  the part it hangs on (found by the engine: the biggest part is the
  root). A kind's `swing = { leg_left = 30, leg_right = -30, arm_left =
  -25, arm_right = 25 }` swings them as it walks (degrees forward first;
  less than 0: back first): a walk without walk frames. In `on_draw`,
  `frame:model(file, thing, nil, nil, { head = { turn = 20 }, arm_right =
  { pitch = 90 } })` poses them by name (an attack, a wave, a look round),
  over the swing. A name the model lacks is a mistake naming its parts.
- The player: with a kit, the kit's (the platformer's hero, the
  shooter's own); without one, `c.player.kind = "hero"` makes it a thing
  of that kind, placed where each level starts and going along from
  level to level (`player:walk(input.dir, 40)`, `player.x`). It is never
  in `level:things()` and has no id; without `c.player.kind` it stands
  nowhere (Sokovox moves its own hero thing).
- Items: `c.items.key = { model = "key.vox", stack = 3 }`;
  `player.state.inventory.key` counts them (an empty inventory from the
  start), `world.items` lists them in order to draw. The code takes them:
  `player:take(b.props.item)` in `on_touch`.
- The ready-made rules (kits) move things; what a touch, a hit, a death or
  a fall means is `main.lua`'s (`docs/game-model.md`, stage 7): `on_touch(a,
  b, where)` for what begins (a stomp when `where == "top"`, a pickup, a
  door), `player:touching(...)` in `on_tick` for what lasts (an enemy's
  contact, spikes; `player:hurt` counts nothing while the player may not be
  hurt), `on_death` and `on_fall_out` for a life lost. What the code reads
  of a kind is its `props` (`props = { worth = 100 }`), what kinds belong
  together their `family`; a kind's `effect` and `sound` show as one of
  its things goes. The platformer template's `main.lua` has the classic
  rules written out: copy and change them. `effect = "burst"` bursts a
  thing into its own voxels as it goes (they fall, lie a moment and are
  gone; Voxkeep's enemies); `spin = 120` turns a kind's model round,
  degrees a second (Voxkeep's gem), only the picture; `fixed = true` keeps
  a kind's things where they are spawned or placed (no gravity; a sign
  hung in the air), for kinds without a behaviour.
- The shooter kit (`c.kit = "shooter"`): soldier, spawner, captive
  (caged until the code sets `thing.state.freed = true`, then runs off),
  gunship, pickup (lies there, touched within `reach`); weapons in
  `c.weapons.NAME` (a blow by `reach` and `cone`, a throw by `throw`,
  else a shot; `auto`, `noise`, `ammo`; `damage` to everyone in a
  blast, `friendly_fire = false` spares the team that set it off,
  `falloff`). Who hurts whom: a kind's `team` (its own are spared; the
  player is `"player"`) and `immune = { "round", "melee", "blast" }`;
  `thing:hurt` hits anything with hit points. The actions are the game's:
  `c.player.attacks = { fire = { "knife", "held", "rifle" } }` (the first
  weapon that can; `held` the gun in hand), `c.player.moves = { "jump",
  "strafe" }`; `thing:attack(weapon, target)` for attacks the code
  decides. Points for a kill, what a pickup gives (`player.state.weapon`
  by its name, `player:take(gun, rounds)`, `hp`), checkpoints, a life lost
  and the goal are the code's: More Blood's `main.lua` and the shooter
  template's have them. The player down lies there, out of the fight,
  until the code sets `player.state.hp` above 0 (`player:place` and
  `player:face` first; `level:clear_rounds("enemies")` takes their rounds
  in flight) or ends the run. Ways are data: a hut's `ahead = { 0, 1 }`,
  a hostage's `flee` (a way, `"away"`, a place), a gunship's `offset`,
  `c.player.facing`.
- The platformer kit (`c.kit = "platformer"`): hero, walker, hopper,
  flyer, pickup (spins), platform (to and fro; `thing.state.goal =
  "end"` or `"start"` sends it to an end to stay: out of a wall, a
  trapdoor, a lift), prop (stands; `stands = true` on the floor below,
  its pose `thing.state.pose` picks its frames), crate (the hero pushes
  it, `push` its speed; it falls, is stood on, walkers turn at it); a
  platform coming down onto a body waits above it, one with `crushes =
  true` hurts the hero instead; the hero may jump again in the air
  (`air_jumps = 1`, `air_jump` its height: a double jump); a ladder's
  top holds from above (down climbs down);
  action jump. A door, a checkpoint, spikes, a lever, a plate are props
  the code answers (a plate pressed: `plate:touching()` holds the player
  or a crate).
  A body's `width` is also its depth: in a 16-voxel lane it must stay
  under 16, or the kit lifts it out of the world into the sky.
- Making the models themselves (sizes, faces, shading, frames or parts,
  judging them with `voxcade sheet`): the voxcade-models skill.
- Models face +y as made; figures turn to where they go, everything else
  is turned to the camera. Give a marker room for the whole body, or the
  engine lifts the thing onto whatever is above.

### Around the game

- Languages: `c.languages = { "de", "en", "fr" }` puts LANGUAGE in the
  settings (the system's at first); `world.language` says which, and may
  change while playing: keep every language's texts and pick by it each
  picture (HELP's SPEAK THE PLAYER'S LANGUAGE); the engine's menus follow.
- Keys: `c.actions = { "jump" }`, `c.keys = { jump = { "Space", "Z" } }`
  (places on a US keyboard, shown as the player's keyboard prints them),
  `c.pad = { jump = { "south" } }`. Pause (Escape, P, Start) gives way to
  a game's own keys.
- Title: `c.title_screen` (`at`, `view_width`, `logo`, `logo_colors`,
  `light_menu` for light letters over a dark scene, `music`).
- A run starts as `c.state` says: `c.state.player = { hp = 5, lives =
  3, inventory = { grenade = 5 } }`, `c.state.world = { ... }`. The
  engine's keys (`hp`, `score`, the shooter's `weapon`) go where it keeps
  them; lives and the like are the game's own; a number a kit needs that
  the game leaves out it has none of (no `hp`: the first hit counts). The
  shooter's rounds and grenades are items of the inventory (a weapon
  with `ammo` is an item of its name): `player:take("machine_gun", 150)`.
- Scores: the code counts in `player.state.score`; `c.scored = true`
  gives the game a high score table (with a kit or without). `c.high_scores` is the table to beat:
  ten lines `{ "ABC", 1000 }`.
- Sounds are files, `sounds/NAME.toml` (or `sounds/NAME.wav` as it is),
  played by the file's name with `level:sound(NAME, at)` or a kind's
  `sound = "NAME"`; a file of a kit's sound's name replaces it, `silent =
  true` silences it. A line a number, only what differs from the
  defaults: `wave = "square"` (saw, triangle, sine, noise), `freq` (Hz),
  `sweep` (semitones the pitch slides over the sound, -12 an octave down),
  `attack`, `hold`, `decay` (seconds), `jump` and `jump_at` (one jump in
  semitones: 5 a fourth), `punch`, `vibrato`, `lowpass`, `volume` (0 to
  1). `voxcade sounds GAME --list` shows them all; a mistake names the
  file, the line and the word. `c.sounds` in conf.lua is refused.
- The HUD is the game's, drawn in `on_draw` (the kits draw none):
  `frame.width`/`frame.height` (UI voxels, from the top left),
  `frame:text(text, x, y, color, size)` (size 2 and up: the big letters;
  drawn as written: lower case, German and French letters, a few signs;
  the engine's own menus stay in capitals),
  `frame:text_width`, `frame:box`, `frame:bar` (a share: a boss's
  health), `frame:icon(name, x, y)` from `c.icons.NAME = { rows = {...},
  colors = { R = { 214, 36, 44 } } }`, `frame:drop_in` (letters dropping
  in, ticks since it began), `frame.alpha` for smooth motion,
  `frame:to_screen(thing or point, up)` where it shows on the HUD (a name
  over a figure's head, a number rising from a harvest);
  `screen:burst(icon, x, y)` in `on_tick` (a heart lost). What changes
  over time (when a message began, digits that hop) goes into a state in
  `on_tick`; More Blood's and the templates' `main.lua` have whole HUDs.
- Menus (a shop, an inventory, a choice): what is open and the cursor
  in `level.state`, the level held while open, moved in `on_tick` with
  `input:repeated` (repeats while held); drawn with `frame:menu{ title,
  tabs, tab, rows, at, hint }` and `frame:grid{ cells, columns, at }` in
  the engine's look (HELP's A MENU AND A BAG).
- Day and night: `frame:daylight{ sun =, sun_color =, ambient =, shadow =,
  sky = }` in `on_draw`, worked out from the game's clock every picture
  (HELP's DAY AND NIGHT); lamps with `frame:light`.
- Feedback on hits and pickups: `screen:shake(amount)`,
  `world:hitstop(ticks)`, `screen:flash(strength)`, `frame:light`; how much
  and when is the voxcade-game-feel skill's subject.
- The sound's pressure: `c.audio.drive` (0 to 12, default 0), how hard the
  game's sound is pushed into the limiter, in decibels: 0 as it is, more
  makes it denser and louder; music you listen to for long tires sooner
  when dense.
- Music: `c.audio.music = { "music/song.mml" }`. For a new song write a
  recipe for `tools/compose.py` (styles calm, mysterious, dark, action, space,
  bright) or pick one from the library `music/`; copy it into the game.
  The game picks its own song with `music:play("song", "bar")` (by the
  file's name, from the next bar; `music:play()` gives the player's
  choice back) and goes with it: `on_beat(beat)` before `on_tick`,
  `music.beat`, `music.bar` (waves on the bar, shots on the beat). Its
  clock is the game's, so replays play it the same. `music.intensity`
  (0..1) is the game's tension: a song's layers (`adaptive = true` in a
  recipe) come and go with it (Pulsar sets it from the enemies on the
  screen). Music is never a kit's: which song plays when is the game's
  code.

## main.lua: the rules

```lua
function on_world_start() end   -- once, at the start: the run's states (world.state, player.state); no level yet
function on_level_start(how) end -- a level begins: build it, spawn, give level.state its first values
function on_tick() end          -- 60 times a second, after the kit's behaviours
function on_draw(frame) end     -- every picture, only looking: frame:model, frame:cube, frame:text ("PRESS {jump}": the player's key), the HUD
function on_touch(a, b, where) end -- two things begin to touch (once; a is the player; where: "top" landed on b, "bottom", "side"); on_touch_end(a, b) when they part
function on_death(thing, by, weapon) end -- hit points ran out (once); also on_hit(thing, by, damage, weapon), on_pickup(thing, item), on_interact(thing, by), on_fall_out(thing)
```

Several players (`docs/game-model.md`, "Several players"; HELP's TWO
PLAYERS; every template plays two): `c.players = 2`
gives seats, `c.player.kind = { "red", "blue" }` a figure for each (with
the shooter kit kinds without behaviour, its keys move them, each its
own hit points, `p.state.weapon` and rounds in `p.state.inventory`),
`c.camera.hold =
{ 120, 80 }` keeps them on one screen. `player` is seat 1 only: a rule
for every player loops over `level:players()` (those playing) and tells
a player by `thing.seat`; each has `p.input`, `p.controls`, `p.state`
(score and the game's own). `world.players[n]` is seat n's player or
`false` (a HUD's PRESS START). `on_player_join(p, how)` hears a seat
taken (its figure comes in at the tick's end: `p:place`, `p.state.kind`
or `p.state.out = true` there), `on_player_leave(p)` one left;
`p.state.out = true` takes a player out of play (no lives left, beaten
in a round). Players may be kept in any state (`world.state.best = p`).
Teams: `p.state.team` (`"player"` unless set): all on one is co-op, two
names two sides, each its own everyone against everyone; the shooter
kit's rounds, blows and blasts hurt players of another team (the aim
assist finds them, `by` in `on_hit` and `on_death` is the one who fired:
a versus score), the jump & run kit's touches are the game's to answer.
At one machine a second player presses START on a pad or Return at the
keyboard: the whole keyboard with the game's keys while player 1 is on a
pad, else the halves (WASD with F G, the arrows with K L); the
settings' CONTROLS (in a game of several) set who plays with what.
`--input2 "away:60;right:40"` plays seat 2 in `shot` and `inspect`.

Levels: the files in `levels/` (named by their file, in name order:
`01_meadow.json`, `02_cave.json`; without any, one level as `c.world`
says), each with named places (`places` in the file).
`world:go_to(level.next)` or `world:go_to("cave", "west")`,
`world:restart()` (to `player.state.checkpoint` if set),
`world:win()`, `world:lose()` ask for the end of the tick; one ask counts
(going on before starting over). `on_level_end(why)` copies what should
stay out of `level.state`, `on_level_start(how)` builds and places;
`on_world_end(result)` hears the end. `level.kept` lasts the run;
`c.transition` sets the fade (window only).

Save games: `c.saves = { slots = 3, version = 1 }` puts SAVE and LOAD in
the pause menu and CONTINUE on the title (the newest save); `world:save(slot)` at the tick's end (save points),
`world:load(slot)` like a level change (`"load"` for `on_level_end` and
`on_level_start`; `level.state` as saved, things in it nil: spawn them
again from what the state says), `world.saves[slot]` (`level`,
`seconds`, or nil), `on_save_loaded(version)` brings an older save's
states up to date when `c.saves.version` went up. Slots live in the
player's config folder; `shot`, `check` and `inspect` keep theirs for the
run only, and a replay carries the saves it loaded.

The handlers below are optional and take no parameters (but
`on_level_start(how)`, `on_level_end(why)`, `on_world_end(result)`): the game model's objects are
globals everywhere (`docs/game-model.md`). `world` is the run
(`world.state`, `world.tick`, `world:win()`, `world:lose()`), `level`
the level being played (voxels, `level:spawn`, `level:things`,
`level:sound`, effects, `level.camera:follow(thing)`), `player` the one
who plays (a thing with its own `player.state`: the score and the
inventory in it), `input` the keys in
`on_tick`, `screen` what `on_draw` draws on. The old names (`load(world)`,
`tick(world, input)`, `draw(world, out)`) are refused with the new ones.
`voxcade api` lists the whole API (`voxcade api thing` only what names
"thing"); the same text is in `lua/voxcade.lua`. `world:read("levels/x.txt")`
reads a data file of the game folder while `main.lua` starts (at its top
or in a handler at the start); it counts for the replays. Growing the API
itself is the voxcade-api skill's subject.

## The sandbox's rules, and why

- **Everything that changes lives in a state**: `world.state` (the run),
  `level.state` (the level being played, and the things in it),
  `player.state` (the player, the score in it, and with a kit lives and
  hp), `thing.state` (a thing's life, its hits in `hp`), as plain data
  (numbers, strings, booleans, things, tables). Snapshots, replays and
  the state hash see only those. Globals are frozen after
  `on_world_start` (top-level locals as constants are fine); `voxcade
  check` names a local of `main.lua` that keeps state anyway.
- **`on_draw` and a kit's questions only look**: changing the game there
  is an error. Change it in `on_tick`.
- **Randomness only from `math.random`** (the game's own seeded
  generator). `pairs` and `next` walk keys in order. There is no `os`
  or `io`, and no clock.
- **A bigger game splits its code into files:** `require("market")` runs
  `market.lua` of the game folder once as the game starts (dots between
  folders: `require("lang.de")` is `lang/de.lua`) and gives back what it
  returns. Only at the top of `main.lua` and of the files it requires;
  they share `main.lua`'s globals and count as its code (replays, hot
  reload). HELP's YOUR OWN LUA FILES shows it.
- **`x ^ y` only as a square** (`x ^ 2`); otherwise `math.pow(x, y)`.
  The systems' own power differs in the last bit between Linux, Windows
  and macOS, and replays would part ways.
- A thing in Lua is only its id: keep the thing (or `thing.id`) in
  `level.state` (never `world.state` or `player.state`: they outlive it,
  and refuse it); `thing:remove()` takes it away at the tick's end; it may be
  gone (`nil`).
- An endless loop runs out of an instruction budget and stops with a
  message; memory is limited too.

## Trying it

- **Errors halt.** A script's error stops the game with file, line and
  tick over the frame; `shot` writes that frame and `inspect` reports it
  as `halted`. Save the mended file and it goes on where it stopped.
- **Hot reload.** Saving `main.lua` takes the new functions in and keeps
  the states (`on_world_start` and `on_level_start` do not run again); saving `conf.lua` applies what it
  can and names what waits for F5.
- **Seeing it without a window:** `voxcade shot games/NAME --frames N
  --input "right:40;right+jump:10"` and read the PNG; `voxcade inspect
  games/NAME --frames N --input ...` prints the things with their
  `state`, `world.state`, `level.state`, `player.state` and the outcome as JSON.
  `inspect` draws nothing, so a mistake in `on_draw` only shows in a
  `shot` or in `voxcade check`, which draws after every tick. Keys in `--input` are the
  four directions and the game's actions, joined with `+`.
- **Scenarios: every game has them.** What shows late in a game (a
  boss, the last room, day 40 of a farm, a full inventory, an event) is
  tried from a scenario, never by playing there first: `scenarios/NAME.lua`
  gives back `{ about = "...", level = "...", place = "...", setup =
  function() ... end }` (all but setup optional); setup runs once before
  the first tick, as `on_tick` could, `require` giving the game's
  modules. Where the states are many, give the scenarios words of the
  game's (BunnyFarm's `setup.lua`: `set.day(12, "21:30")`,
  `set.animal("sheep", "f", "white", { ready = true })`, `set.gap("sheep")`)
  that keep the states consistent. `voxcade scenarios games/NAME` lists
  them; `shot`, `inspect` and `run` take `--scenario NAME`. Something new
  that only shows late gets a scenario when it is built, and a test
  replay from it: `shot games/NAME --scenario NAME --input "..." --frames N
  --record games/NAME/replays/tests/NAME.toml`.
- **The console** runs a line of Lua in the game between two ticks:
  `--do "player.state.score = 5000"` (again for more) for `shot`,
  `inspect` and `run`; `require("wolf").night()` calls the game's own
  functions; an expression answers with its value. In the window the
  key left of 1 opens it; in the IDE the line under OUTPUT; over MCP
  `eval`. Lines run are part of the run and its replay.
- **A test replay:** once the game plays, record a scripted run with
  `shot ... --record games/NAME/replays/tests/NAME.toml`; `voxcade check
  games/NAME` then guards it. The voxcade-verify skill has the rest
  (`--refit`, `--update`, frame times).
- **What every example game has** (the user's wish: about the same
  features in each): a title screen with its music, a score with
  `c.high_scores` of its own, key hints in the HUD (`{action}`), test
  replays in `replays/tests/`, a demo run for the title in `replays/`
  itself (`demo.toml`: attract mode plays it after 30 s without a key;
  `shot --replay games/NAME/replays/demo.toml --screen attract` shows
  it), scenarios in `scenarios/` (a test asks for them) and a
  `.luarc.json`. The settings and pause menus are the engine's. A new
  example game gets the same.
- The user plays in a window (`cargo run --release -- run games/NAME`);
  you do not open windows. How it feels is for them to say.

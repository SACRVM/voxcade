# The game model and the Lua API, version 2

The model the Lua API is built on: what a game's scripts see, when the
engine calls them, and why. `voxcade api` (from the register,
`src/lua/api.rs`) lists every name with its words, then every `conf.lua`
setting (its type, default and range, read from `src/config.rs`; the
ranges are `config::RANGES`, checked as `conf.lua` is read) and every key
of a kind of thing (the engine's, and each kit's behaviors' from their
`kit.toml`); `voxcade api NAME` finds a name as it is printed, in part
and in any case. `lua/voxcade.lua` gives them to editors (LuaLS: a kind
is a `Kind`, or a behavior's `Kind.platformer.hero` …), and HELP in the
IDE shows each by a whole game, its REFERENCE all of them; this file is
the model behind those names. What is not built yet is said where it
would go ("not yet").

## Why

The aim, in the owner's words: the best and most logical, yet still most
flexible game API there is. A simple, generic model that carries every
kind of game and every cross between them (a jump & run RPG, Zak McKracken
meets Double Dragon, an RPG's zones, doors to a given level), so that the
API need not break again when the next genre comes. A game writes some
code: the engine offers the parts, the script decides.

## Principles

1. **Who does what is plain.** Each object has one job: `world` the whole
   run, `level` the place being played, `player` the one who plays, a
   thing what acts. A name says what it does.
2. **Everything that changes while playing lives in a state.** The engine's
   own changing numbers too (lives, score, hit points, the inventory,
   abilities gained, a hold). What does not change is a read-only
   property. A save game, a snapshot, a replay check and the agent's
   `inspect` therefore see the same thing: the states.
3. **States hold data, never live things across their lifetime.** A
   thing lives in its level; what outlives the level (`world.state`,
   `player.state`, `level.kept`, a save) names it instead (`thing.name`).
4. **The engine calls, the script decides.** Events have handlers named
   `on_` and what happened; the script decides with its states. No
   game-shaped questions, no blocking calls, no goal the engine knows of.
5. **Global objects, few parameters.** `world`, `level`, `player`,
   `input` and `screen` are there everywhere (LÖVE's way); a handler gets
   as parameters only what its event is about. No more globals than these.
6. **Data in `conf.lua`, code in `main.lua`.** Kinds, items, their props
   and defaults: data, checked when loaded. Rules: code.
7. **Names, never numbers or coordinates, link things.** Levels, places,
   items, kinds and placed things are linked by name, and every link is
   checked when the game loads.
8. **Deterministic and in a written order.** Events come in a fixed,
   documented order, none dropped; anything that changes the game's course
   (a level change, a restart, a save, a load, a removal, the end) happens
   at the tick's end, and one ask of a tick counts: going on outranks
   starting over, among equals the first.
9. **A run starts from the files.** Every new run, restart and level
   entered is built from the game's files and its states, never from a
   leftover: nothing leaks from one run or visit into the next.
10. **One API at a time.** When it changes, every game, template and HELP
    example in the repository moves with it in the same commit.

## The objects

The engine's objects (`world`, `level`, `player` and every thing,
`input`, `screen`, `controls`, `music`, `frame`, an area, `level.camera`)
have exactly the names below and in the register (`src/lua/api.rs`): reading
a name one does not have is a mistake at its line, never a silent nil (it
names the object, the name and the one meant: `level.szie` says "level
has no szie: did you mean level.size?"; `input.jump` says an action is asked by its
name, `input:held("jump")`), and setting one is too (the game's own data
goes in the states). The states, `props` and the tables a field gives
(`level.places`, `level.areas`) are plain tables of the game's: a key
nobody set reads nil there. A field that may be empty by design reads nil
where nothing is (`thing.name`, `thing.wall`, `level.next`, `music.song`).

### `world`: the whole run

| Name | What |
|---|---|
| `world.state` | the run's state |
| `world.tick` | ticks the game has run in this run (60 a second): every tick `on_tick` runs; none during the engine's pause, a hit stop or a level change |
| `world.levels` | the levels' names, in order |
| `world.players` | every seat (`c.players`, 1 unless set): `world.players[n]` is seat n's player, or `false` while nobody sits there |
| `world.saves` | what lies in each save slot, `{ level, seconds }` (the level it was saved in, the play time), nil for an empty one; read-only |
| `world.leaving` | nil, or why the level is about to be left (`"go_to"`, `"restart"`, `"load"`, `"won"`, `"lost"`): an ask is pending; nil again once the change is done (in `on_level_start`) |
| `world:go_to(level, place?)` | leave this level for another (a name, or a number in `world.levels`) at the tick's end; arrive at `place`, or where the level starts |
| `world:restart(place?)` | enter this level afresh at the tick's end: at `place`, else at the checkpoint, else where it starts |
| `world:win()`, `world:lose()` | the run ends at the tick's end (`on_world_end` says how) |
| `world:hitstop(ticks)` | the whole game holds still for a few ticks, `on_tick` and the time too: the punch of a hit; 60 (a second) at most, more is cut to 60 |
| `world:read(name)` | a file of the game folder as text, while `main.lua` starts |
| `world:save(slot)`, `world:load(slot)` | at the tick's end (save games, below); loading an empty slot is a mistake with its line (`world.saves[slot]` tells first) |

### `level`: the level being played

| Name | What |
|---|---|
| `level.name`, `level.number` | which one: its name, its place in `world.levels` |
| `level.next` | the next level's name, nil after the last |
| `level.state` | this visit's state: fresh each time the level is entered |
| `level.kept` | what this level keeps across visits (a run long, saved): for things a game tracks by hand |
| `level.visits` | how often it has been entered in this run, restarts included (1 on the first) |
| `level.size` | its size in voxels |
| `level.places` | its named places: `level.places.west` is `{ x, y, z, facing }`; `level.places.start` is always there, where the level starts |
| `level.areas` | its named areas, boxes set in its file: `level.areas.by_the_stove:holds(player)` (a thing where its feet stand, or a point `{ x, y, z }`, z left out: any height); only to read |
| `level.arrived` | where the player came into this level (a place, with its name): a game without the engine's figure for the player stands its hero there |
| `level:voxel(x, y, z)`, `level:solid(x, y, z)`, `level:set_voxel(…)`, `level:fill(…)`, `level:ground(x, y)` | its voxels (`solid`: whether bodies stand on it; soft voxels are not) |
| `level:spawn(kind, x, y, options?)`, `level:thing(name or id)`, `level:things(kind or family?)`, `level:count(kind or family?)` | its things; `options`: `{ z =, name =, facing =, props =, state = }`; x and y inside the level (only a kind with `fixed = true` may wait outside, coming in from off the screen); a thing that died is no longer listed or counted (only its picture lingers until its rules take it away) |
| `level:explode(at, radius)`, `level:sound(name, at, volume?)` | what happens in it (a sound by its file's name: `sounds/NAME.toml`, or one the engine or a kit brings) |
| `level.camera:follow(thing)`, `level.camera:look_at(x, y)` | where the camera looks (one picture's camera: `frame:camera` in `on_draw`) |

### `player`: the one who plays

A thing (everything a thing has) that belongs to no level: it goes from
level to level and keeps its `player.state`. `player` is
`world.players[1]`; `level:things()` does not list it, but touches and hits
report it; `player:remove()` is a mistake; it has no `id` (it is no
level's thing, `player` itself is the handle). Its figure is the
ready-made rules' player (the platformer's hero, the shooter's own of no
kind), or, without them, a thing of `c.player.kind`, placed where each
level starts before `on_level_start` and moved like any thing the script
moves (`player:walk`). A game without a figure for the player (Sokovox)
still has `player`, for its state and score, standing nowhere: reading
where it stands is a mistake that says so. Abilities gained in play (a double jump, more speed, a key's
door) are keys in `player.state`, read by the behaviors, never global
settings. Several players: `player` is seat 1, the others are in
`world.players` ("Several players" below).

### A thing

| Name | What |
|---|---|
| `thing.id` | which one, for this run (read-only; never given to another) |
| `thing.name` | its name in its level, set in the level editor (`"door_west"`, or made up: `"door_3"`), unique in the level (checked at load); spawned things have none unless given |
| `thing.kind`, `thing.families` | of what kind, in which families (read-only); `"piece"` for a model or light a level's file sets down |
| `thing.state` | its state, as long as it lives |
| `thing.props` | what the level editor set on this one thing (a door's `to` and `at`), typed and defaulted by its kind; read-only |
| `thing.x`, `.y`, `.z`, `.on_ground`, `.facing`, `.wall` | its body, read-only: the physics' (`wall`: the way to a wall right beside it, or nil) |
| `thing:walk(way, speed)`, `:face(way)`, `:jump(height)`, `:place(x, y, z?)` or `:place(place)` | moving things the script moves: kinds without behaviors, and things whose behaviors are all switched off; placed at a place, a thing faces its facing |
| `thing:launch(way, speed, rise?)` | sets it moving, standing or not (a dash, a wall jump, a knockback): the script's things, those switched off, and the figures of ready-made rules, which steer on from there |
| `thing:near(other, distance)`, `thing:ahead(distance)` | asking |
| `thing:enable(behavior, on)` | a behavior switched on or off for this one thing, until it leaves its level (the player: until the next level is entered) |
| `thing:remove()` | it goes at the tick's end (still readable until then); events that already happened this tick are still delivered, no new ones come |

### `input`, `controls`, `screen`, `music`, and `on_draw`'s `frame`

- `input`: the keys held this tick (`input.dir`, `input:held`,
  `input:pressed`, `input:released`), in `on_tick` only (reading it in
  `on_draw` or at the start is a mistake that says so). With several players each has its own
  (`p.input`), `input` staying seat 1's; later a pointer for
  point-and-click games.
- `controls`: what the player plays with, there everywhere:
  `controls.device` (`"keyboard"` or `"gamepad"`, the one used last; nil
  before the run's first tick and where nobody plays) and
  `controls:key(action)` (the key or button as the player sees it: a
  German keyboard's Y for the binding's Z, a pad's CIRCLE). It comes from
  the player's machine, not the game, so a replay writes it down whenever
  it changes and plays it back, like the keys held: a game may go by it
  (an aim assist on the pad) and still replays exactly. With several
  players each has its own (`p.controls`), `controls` staying seat 1's.
- `screen`: the picture's effects, from anywhere the game changes
  (`screen:shake(amount)`, `screen:flash(strength)`): they are part of
  the game, replays see them.
- `music`: the song the game plays of its own choice and its clock, so
  the game can go with the music (waves on the beat, a shooter's enemies
  coming in time). `music:play(song, from)` plays a song of
  `c.audio.music` by its file's name from the next bar of the one playing
  (`"beat"`, `"now"`; at once when none of the game's plays), at its beat
  0; without a song the player's own comes back (as the settings chose).
  `music.song`, `music.beat`, `music.bar` (`#meter` beats to a bar) read
  it; `on_beat(beat)` tells each beat. The clock is the game's: a step
  every tick the game goes, a hit stop's too (so the music never waits),
  in the snapshot and the hash of a game whose code names `music` or
  `on_beat`, the songs' tempo and meter in its fingerprint; the audio
  keeps time with it (unheard nudges, a jump after a pause, which holds
  it). A level change's fade in a window holds no song: its frames step
  the clock as ticks would (no handler runs; beats that began in it are
  told at the first tick after, in order), so a song asked for again in
  the next level plays on through it without a gap; the run's replay
  says it faded (`fades = true`) and takes the same steps at each level
  change without a picture. A song changed (another asked, or the
  player's back) ends as the new begins, the rest of a level's fade
  silent between them (a short pause), unless it crossfades:
  `music:play(song, { from = "now", crossfade = 3 })`, or
  `c.audio.crossfade` for every change, fades the old out as the new
  comes in over the seconds, equal in power (as loud together as either
  alone), through a level's fade too; `music:volume` and `music:muffle`
  hold over both. The clock is the new song's from the moment it begins
  (`music.beat`, `on_beat` count its beats), the old one only heard as it
  goes; there is no `music:stop` (a game that wants silence plays a
  silent song). The game's song wins over the player's choice; music
  belongs to no kit. `music.intensity` (0..1, 1 unless set) is the
  game's tension: a song's layers (`#layer D C 0.35` in it; `compose.py`
  writes them with `adaptive = true`) come in above their threshold and
  go below it, each over a bar (Pulsar: the drums with a few enemies,
  the melody with many). `music:volume(level, seconds)` (0..1, a part
  of the song's own loudness, under the player's volume) and
  `music:muffle(amount, seconds)` (0 clear to 1 very dull: a low-pass
  over the whole song, as through a wall; the game's sounds stay clear)
  glide there over the seconds (0.3 unless said; 0 at once, still
  without a click) and hold until changed, from level to level, whichever
  song plays (the game's own or the player's); without a value each
  gives the one asked last. Only heard: kept in the snapshot (a game
  brought back sounds as it did), in neither the hash, the replays nor a
  save (a game loaded from one sets them in `on_level_start`); the
  title's and the settings' songs never hear them. Later: a tempo the game sets, stingers, sounds
  that play in the song's chord (`shots/reviews/music-plan.md`).
- `frame`, `on_draw`'s parameter: this picture alone (`frame:text`,
  `frame:cube`, `frame:model`, `frame:light`). An action in braces in a
  text is its key as the player sees it (`"PRESS {jump}"`, as
  `controls:key`), `{{` a brace.
  `frame:model` rotates a model further (`rotation`) and poses a figure
  of parts (`pose`: each part by its name, a rotation about its joint): only the
  picture, decided anew each frame from the states, as a kind's `swing`
  (parts swinging with its steps) is decided from its body.
  `frame:model` draws any model file of the game by its path in the game
  folder (`"sheet.vvx"`, `"props/sheet.vvx"`), named nowhere else (the
  owner, 2026-11-14: "ich gebe den ort an wo das model geladen wird -
  wieso muss es irgendwo angemeldet werden?"; `c.models` is no more,
  `voxcade upgrade` takes it out): what a level names (its pieces') is
  loaded as the level is entered, any other model the first time it is
  drawn, and let go when the level is left, so a sheet one scene shows
  does not lie in memory the whole run. A file that is not there, or no
  model, stops the game with a mistake at the line that draws it. Only
  the picture: a model only drawn counts for no replay, hash or save.
- **A model's pivot** is the cell `size / 2` (whole voxels, rounded
  down) across and deep of the model's own box, on the floor: a thing's
  position is that cell, and a model turns about its middle. For an odd
  size that is the middle cell and the model fills the same cells at
  every turn. For an even size (4 across: its own cells -2, -1, 0, 1 from
  the pivot) the pivot lies one cell off the middle, so a turn that flips
  an axis (facing -y flips both ways, facing +x or -x one) fills the
  cells one voxel higher on it (-1 to 2): facing -y a 4 by 4 model lies
  one voxel up and to the right of facing +y. Free turns (`rotates =
  "free"`, `rotation`) land on the same cells. To keep an even model in one
  place whichever way it faces, give it an odd size, or place it by its
  middle and allow the voxel. So where a model lies in the world (its low
  corner `lo`) and its pivot cell (`at`, a piece's) differ by the way it
  faces: facing north (+y, the model's own forward) `at = lo + size / 2`
  on each axis; facing south (-y, a piece's way unless its file says),
  both axes flipped, `at = lo + (size - 1) / 2` (halves rounded down;
  the same for an odd size). A model 4 across and 1 deep, its own cells
  -2 to 1 from its pivot, with `at = [10, 10, 0]`:

  | facing | the cells it fills |
  |---|---|
  | north | x 8 to 11, y 10 |
  | east | x 10, y 9 to 12 |
  | south | x 9 to 12, y 10 |
  | west | x 10, y 8 to 11 |

  (pinned by `an_even_sized_model_shifts_a_voxel_when_it_turns`). Turned
  freely (`frame:model`'s rotation, a piece's `rotation`, `pitch`, `roll`) a
  model turns about the middle of the voxel at half its height over the
  pivot cell (`render::turn_centre`, z `size.z / 2 + 0.5`, halves
  rounded down: a voxel's middle, so a quarter turn about any axis puts
  every voxel on the grid again). A
  model's file (`.vvx`, `src/vvx.rs`) measures its frames' and parts'
  `at` and the joints from this cell, so growing a part moves nothing
  else and the model keeps standing where it stood. A part may rest
  turned about its joint (`rotation`, `pitch`, `roll` on its `[[part]]`,
  degrees, the root's too; written only when not 0): the words, sense
  and order of a pose entry, and every pose (`frame:model`'s, a piece's
  `thing.state.parts`, a kind's `swing`) turns on from it: `pose · rest`
  about the same joint, the rest first (`{ rotation = 10 }` on a part
  resting at `rotation = 45` faces it 55 round). Only the picture: the
  simulation, a solid piece's cells, bursting into voxels and which
  voxels can be seen see the parts unturned.
- **A game's version**: `conf.lua` says which Voxcade its files are for,
  `c.voxcade = "0.6.0"` (read as that line; in neither the fingerprint
  nor the hash). A game for an older Voxcade, or without the line, does
  not open (`run`, `shot`, `check`, the IDE, the launcher) until `voxcade
  upgrade` has brought its files up to date (`src/upgrade/`: a step
  for each change of a format, 0.6.0's the models as `.vvx`); one for a
  newer Voxcade does not open either. Games given to the engine in tests
  as text are not asked.

## The game's files, pictures, menus and languages

What a world that goes on needs (a farming game with game days, menus
and three languages, BunnyFarm, was the first), for every game. No new
idea of the model: a menu is a sequence in `level.state` with the level
held, as dialogue and shops always were; light is the picture's, so
`on_draw` sets it.

- **The game's own Lua files.** `require("market")` runs `market.lua` from
  the game folder once and gives back what it returns (`require("lang.de")`
  is `lang/de.lua`; a second `require` of the same gives the same value).
  Lua's own name, as LÖVE has it, rather than an engine's (the one free
  function besides the handlers: it is the language's, like `pairs`).
  Only while the game starts (the top of `main.lua` and of the files it
  requires), as `world:read`; the files are the game's code for the
  fingerprint (as tokens) and for hot reload (saving any of them swaps the
  functions, the states stay), and `check`'s guard names a local of any
  of them that keeps state.
- **Letters.** The pixel font has lower case (descenders a row or
  two below the line), the letters of German and French with their
  accents (ä ö ü ß à â ç è é ê ë î ï ô û ù œ and the capitals), and a few
  signs (· × → ← ▲ ▼ ✓ … „ “ ’ « » • &). The engine's own text (menus,
  the logo, a key's name in a game's text) is drawn in capitals
  (`ui::engine_case`); a game's text is drawn as written. Accented capitals keep their full height, the accent in two
  rows above (ZURÜCK reads as capitals in a German menu); lower case with
  an acute, grave or circumflex gets a row of air under it.
- **Light the game sets.** `frame:daylight{ sun =, sun_color =, ambient =,
  shadow =, sky = }` in `on_draw`: this picture's daylight, each key as
  `c.light` names it (`sun_color` white unless set), the
  ones left out as `c.light` says. A day and night is the game's code: it
  works the light out from its clock every picture. Only the picture: the
  simulation does not see it, replays need nothing. A game whose code
  names `daylight` has every face a sun above the horizon can light in its
  meshes (others keep only what their one sun and the camera can hit).
  Colours as the other frame calls take them (`{ r, g, b }`, 0 to 255);
  HELP's DAY AND NIGHT shows a day of 40 seconds.
- **A picture's camera.** `frame:camera{ look_at =, width =,
  height =, pitch =, fov = }` in `on_draw`: this picture's camera, for a
  scene the engine films (the point-and-click games' way: zoom in on
  something and out again, a cut, a slow pan; otherwise the camera stays
  put or follows, `level.camera`). What it leaves out is `c.camera`'s:
  without `look_at` the view keeps on what the game's camera follows
  (`{ width = 80 }` zooms in on the player and follows on), with it the
  view centres on that point (`{ x, y, z }`, z left out the camera's
  height; or a thing, where it is drawn) as given, the screen's shake on
  top, never rounded or eased. Nothing is kept: the next picture without
  it is the game's camera's, so a scene calls it in every picture and
  works a pan out from its clock (`world.tick + frame.alpha`,
  `music.beat`). Only the picture, as `frame:daylight`: the simulation,
  the state hash, replays, the sounds' places and `level.camera` do not
  see it (a replay draws every tick and comes out the same);
  `frame:to_screen` after it goes by it; on the title the title's camera
  stays. The camera does not turn round yet (`yaw` is refused: a turned view
  is not what scenes need now). Faces: a
  picture moved or zoomed looks along the very rays of the game's camera
  (perspective at its pitch and field of view: moving and zooming change
  only where the eye is), so the meshes stay as they are and cost
  nothing; a picture at another `pitch` or `fov` adds the faces its rays
  can hit (a low angle, the undersides), from then on, the level built
  anew with them once (a hitch of a mesh build, More Blood's 44 ms, the
  first time). Models out of view are left out once the picture's camera
  is known (a game whose code calls `frame:camera`). Mistakes name the
  field (an unknown key, a number missing, width or height 0 or less,
  both, pitch outside -89 to 90, fov outside 10 to 100). HELP's A SCENE
  WITH BARS shows a pan and a cut.
- **The picture's shape.** `c.camera.aspect = 2.39` in
  `conf.lua`: every picture is drawn into the largest rectangle of that
  width over height the window holds, in its middle, black bars on the
  other two sides (letterbox or pillarbox; a film's bars carry nothing).
  The camera's `view_width` (and `frame:camera`'s `width`) is across that
  rectangle; the HUD (`frame.width`, `frame.height`), the menus and the
  console lie inside it; `shot`, `film` and the IDE's play area alike.
  Unset, the window's shape. Not per picture (the HUD is laid
  out before `on_draw` runs).
- **Menus are the game's, drawn in the engine's look.** The game keeps
  what is open and where the cursor stands in `level.state` (held while
  it is open) and moves it with the keys; the engine draws:
  `frame:menu{ x, y, width, title, tabs, tab, rows, at, hint }` (rows of
  a text, a value at the right, an icon of `c.icons`, dimmed when
  `dim = true`; the row `at` marked as the engine's menus mark theirs,
  scrolled into view) and `frame:grid{ x, y, columns, cells, at }` (an
  inventory's slots: an icon and a count each). `input:repeated(action)`
  is true when the key is pressed and again and again while it is held
  (as the engine's menus move), for cursors and walking on a grid: after
  20 ticks held, every 6, counted per key in the game and kept in its
  snapshots so a restored second repeats the same. `shown` caps the rows
  a menu shows (a longer list scrolls, centred on `at`, with a bar at its
  edge); a slot is `false` when empty. HELP's A MENU AND A BAG is a
  market stall and a bag.
- **CONTINUE on the title.** A game with `c.saves` offers CONTINUE first
  when a slot holds a save: the newest is loaded, as LOAD in the pause
  menu does (the run begins from that save; replays carry it). START
  begins a new run as always; which slot it saves into is the game's
  (`world.saves` says which are free). The title's list grows upwards, its
  last item where it always was; `shot --screen continue` shows it.
- **The player's language.** `c.languages = { "de", "en", "fr" }`: the
  game speaks these. SETTINGS gets LANGUAGE (only with more than one);
  the first time it is the system's language if the game speaks it, else
  the first listed. `world.language` says which (`"de"`), everywhere, and
  changes when the player changes it; it comes from the player's machine,
  so replays write it down as they do `controls`. The engine's own menus
  speak it too (German, English, French). The game's texts are its own
  tables (`require("lang." .. world.language)` is not it: the language
  can change during a run; keep all and pick by `world.language`).
  The player's choice is kept in their settings (`Settings::language`,
  the system's from SDL in a window only, so `shot`, `check` and
  `inspect` speak the game's first on every machine), sent with the
  controls (`Controls::language`) every picture; the engine's words in
  `src/lang.rs`; HELP's SPEAK THE PLAYER'S LANGUAGE.

## States

| State | Lives | Holds, among the game's own |
|---|---|---|
| `world.state` | the whole run | what the game knows across levels: quests, the story so far |
| `level.state` | one visit of one level; fresh each time it is entered, a restart too | crates moved, enemies left, time in the level, a sequence's steps |
| `level.kept` | the whole run, one per level | what this level keeps across visits that the game tracks itself |
| `player.state` | the whole run, from level to level | `lives`, `score`, `hp`, `inventory`, `checkpoint`, abilities |
| `thing.state` | as long as the thing lives | `hp`, a chest's or a trader's `inventory`, a piece's `light` (0 off to 1), a piece's `parts` (its figure's pose) |

- **Engine keys.** The engine reads and writes fixed keys, and the script
  uses the same: a hit lowers `thing.state.hp`; a pickup raises
  `player.state.inventory.key`; points raise `player.state.score`;
  `level.state.held` holds the level (below); `thing.state.invulnerable`
  keeps hits away, a number of ticks counted down or `true` until set
  back; `player.state.checkpoint` names the place or thing a restart goes
  to (the game's code sets it as the player passes one; the engine clears
  it when another level is entered by `go_to` or `load`). A
  kind has `hp` only if it declares it (`c.things.NAME.hp`); the others
  have none and never die. The IDE offers the engine's keys after
  `level.state.` and the like, so a misspelt one is seen.
  The ready-made rules keep their run in `player.state` too (the
  shooter's `weapon`; its rounds and grenades are items of the
  inventory, `inventory.machine_gun`, `inventory.grenade`); lives are the
  game's own.
- **Defaults are data.** `conf.lua` declares what a new run starts with
  and what a state holds when it is new:
  `c.state.world = { quests = {} }`, `c.state.player = { hp = 5, lives = 3,
  inventory = { grenade = 5 } }` (built: the engine's keys among them
  go where the engine keeps them; a ready-made rule's number the game
  leaves out it has none of: no `hp`, the first hit counts),
  `c.things.chest.state = { open = false }` (not yet). The same defaults
  fill in keys an old save lacks (not yet).
- **Plain data.** States hold numbers, strings, booleans and tables of
  those, each table once. `level.state` and `thing.state` may hold things
  of the same visit besides. A thing written into `world.state`,
  `player.state` or `level.kept` is a mistake with its line: they outlive
  the thing; its `name` goes there instead. A table with a metatable (a
  view, another state) is no plain data either.
- **Static, read-only.** A thing's kind, families, name and props, a
  level's name and places, everything in `conf.lua`.
- **The body is the physics'.** Where a thing stands and how fast it
  moves changes too, but it is the engine's: read through properties,
  changed through methods, kept in snapshots and saves, not in a state.
- **Sequences are data.** A cutscene, an end-of-level tally, a death
  animation before the restart, a timer: steps and counters in a state,
  moved on by `on_tick`. Nothing blocks and nothing waits inside a handler
  (no coroutines): a sequence in a state is saved, snapshotted and
  replayed like the rest. Skipping one is the script's: a key that jumps
  its state to the end.

## Things that stay gone

Most games want a key taken to stay taken and a chest opened to stay
open when the level is entered again. A game keeps them by hand: in
`level.kept` (a lever's position, a puzzle solved, which keys are taken),
read in `on_level_start`; it lasts the run and goes into its saves. A
`keep = true` on a kind (the engine keeping removed and changed placed
things by their `thing.name`) is planned for 0.7: it is not built, and a
kind that names it is refused like any unknown key.

Everything else of the level is as its file has it on each visit and
each restart (enemies back, crates where they started): the default
stays a fresh level.

## The flow of a run

This is the heart of the model: one path from the title to the end, the
same for every kind of game. None of the eleven engines studied knows a
"goal" or "level complete"; in all of them a goal is a thing or an area
whose touch the game's code answers with a level change. They differ in
what they leave to each game to guard by hand, and those guards are the
engine's here.

```
title (the engine's) ── NEW GAME
  │
  ▼
on_world_start()                     the run's states from c.state; no level yet
  │
  ▼
the first level from its file ─► on_level_start("start")
  │
  ▼
ticks ◄──────────────────────────────────────────────────────────────┐
  │ an ask: world:go_to, world:restart, world:win, world:lose,       │
  │ world:load (one counts: going on before starting over; tick end) │
  ▼                                                                   │
fade out ─► on_level_end(why) ─► the next level ─► on_level_start(how) ─► fade in
  │ won, lost, quit
  ▼
fade out ─► on_level_end(why) ─► on_world_end(result) ─► high scores (the engine's) ─► title
```

### Starting

- A run always starts from the files: the states from `c.state`, the
  first level from its file. Nothing of an earlier run is left (no
  globals, no statics, no snapshot to restart from).
- `on_world_start()` runs first, once: the run's own states. There is no
  level yet; `level` used there is a mistake that says so.
- The first level is the first in `world.levels` (`--level NAME` for
  `run`, `shot`, `inspect` and the IDE's PLAY starts at another: the same
  order, `on_world_start` first).
- `on_level_start("start")` places what the level needs (a level made in
  code is built here), gives `level.state` its first values and starts any
  intro as a sequence. The screen is still dark: it only sets things up.
- A run begun from a scenario (below) goes on from here: to its level,
  then its `setup()`; a run begun from a save, from the save.
- Then the first tick runs (from the title, the view glides from the
  title's camera to the player; no fade).

### Inside a level

Every tick, in order (detailed under "A tick, in order"): input, the
ready-made behaviors, their events to the script, `on_tick`, the tick's
end. `on_draw` runs for every picture and only looks.

### Leaving a level

- **Asks.** `world:go_to(level, place)`, `world:restart(place)`,
  `world:win()`, `world:lose()` and `world:load(slot)` ask; nothing
  happens at once. The handler runs on, and `world.leaving` says what is
  pending.
- **One ask counts.** Going on (`go_to`, `win`) outranks starting over
  (`restart`, `lose`, `load`): the flag and a Goomba touched in the same
  tick win the level, as in every classic. Among asks of the same rank the
  first counts: a goal touched twice, two exits in one tick. Every
  engine's official samples guard this by hand. An ask that loses is
  noted in OUTPUT with its line; only a second ask of the same rank and
  kind (the same exit twice) is quiet.
- **An end sequence guards itself.** The walk to the castle or a tally
  runs before the ask, while the level still runs: the player's rules off
  (`player:enable("hero", false)`) and `player.state.invulnerable = true`,
  two lines the templates show, keep it from being hit meanwhile. Under a
  hold nothing of the ready-made rules hits anyone anyway.
- **The change, at the tick's end, between two ticks:**
  1. The engine fades out over the level as it stands (no ticks run;
     `on_draw` goes on, so the game's own picture fades with it).
  2. `on_level_end(why)` runs: `why` is `"go_to"`, `"restart"`,
     `"load"`, `"reload"`, `"won"`, `"lost"` or `"quit"`. Copy from
     `level.state` what should outlive the visit into `world.state`,
     `player.state` or `level.kept` (those three are kept anyway).
  3. The level's things go (no `on_death` for them), its `level.state`
     with them, and events still pending for the next tick are dropped.
  4. The next level is built from its file and what it kept; the player
     stands at the place, its feet at the place's height (lifted out of
     anything there, else down onto what is below), facing its facing if
     it has one. A player arriving with no
     hit points left gets its kind's back (a death in the tick of a win
     does not carry over); a living one keeps what it has.
  5. `on_level_start(how)` runs: `how` is `"start"`, `"go_to"`,
     `"restart"`, `"load"` or `"reload"` (`"go_to"` may be the same
     level: Sokovox's next puzzle).
  6. Things the player already overlaps count as touching: no `on_touch`
     until they part (a player arriving on a door does not go through it
     again). The same holds after `thing:place`.
  7. The camera follows the player again unless `on_level_start` said
     otherwise; the level's music plays (going on if it is the same song);
     the sounds of the last tick play out.
  8. The engine fades in; then the next tick.
- **Fades are the picture's, not the game's.** No ticks run and no input
  is read during them, so their length changes nothing in replays, and
  `shot`, `check` and `inspect` skip them; `on_draw` runs throughout.
  Only the music's clock goes on through them (the song plays on); a
  replay recorded where they faded takes those steps without a picture.
  `c.transition = { fade = 0.3, color = "#000000", restart = 0 }`: seconds
  out and in, 0 for a cut, with a length of its own for a reason
  (`go_to`, `restart`, `won`, `lost`) where a game wants it (a puzzle
  game's quick restart). The engine's pause opens only between ticks and
  waits until a fade is over, so QUIT TO TITLE never meets a pending ask.
- **"Next" and the last level.** `world:go_to(level.next)` goes on in
  order; after the last level `level.next` is nil, and `go_to(nil)` is a
  mistake with its line, never a silent win. A game wins with
  `world:win()`.

### Death and starting over

- **Death is an event, the rest is the game's.** `on_death(thing, by)`
  runs once when a thing's `state.hp` reaches 0, whoever lowered it
  (a hit, a fall, the script). After it the thing gets no more hits or
  touches and is no longer counted. Its death show (a fall, a burst, a
  hop off the screen) plays on even under a hold; the ready-made rules
  remove enemies afterwards; the player stays, lying still.
- **Back to life.** A thing whose hit points are raised above 0 lives
  again: hit, touched and counted, its rules running, `on_death` possible
  again.
- **Several deaths in one tick** are all uncounted before the first
  `on_death` is delivered: two enemies killed by one grenade both see
  `level:count("enemies") == 0`. A game that acts on "the last one"
  keeps its own flag.
- **Three ways on, all written in a few lines:**
  - *Start the level over:* `world:restart()`: everything of the level
    from its file again, fresh `level.state`; the player
    at the checkpoint if there is one, else at the start.
  - *Back to a checkpoint:* the script sets `player.state.checkpoint` to
    a place's or a thing's name as the player passes it;
    `world:restart()` goes there. The engine forgets it when another level is entered, so
    a checkpoint never points into the wrong level.
  - *Respawn in place* (the level runs on): `player:place(
    level.places.start)`, the hit points back (it lives again), a few
    ticks of `player.state.invulnerable`.
- **Nothing heals by itself.** A restart or a level change keeps
  `player.state` as it is, the hit points too: the game gives them back
  where it wants (`on_level_start`: `if how == "restart" then
  player.state.hp = 3 end`). The templates show it. Only a player who
  would arrive dead gets its kind's hit points (step 4 above).
- **Lives are optional.** A game with lives keeps them in
  `player.state.lives` and decides in `on_death`: another try, or
  `world:lose()`. A game without lives restarts forever (instant retry).
  A continue screen is a sequence in `level.state` with the level held.
- **A death animation before the restart** is a sequence: `on_death`
  holds the level (`level.state.held = true`) and sets a counter;
  `on_tick` counts it down and then asks for the restart.
- **Without a handler nothing happens:** the player lies there. The
  templates bring the few lines a game starts from.

### Holds

Three ways to stop things, none of them a stack of scenes:

| Hold | What stops | What runs | For |
|---|---|---|---|
| The engine's pause (Escape) | everything: no ticks | the pause menu; nothing is called on return | pausing |
| `level.state.held = true` | the ready-made behaviors, physics, touches, timers of the level; a dying thing's death show plays on | `on_tick`, `on_draw`, `input`; `world.tick` counts | dialogue, an inventory, a shop, a tally, a continue screen, a death animation, an RPG's battle over the map |
| `world:hitstop(ticks)` | everything, `on_tick` too, a second at most | the picture | the punch of a hit |

`thing:enable(behavior, false)` stops one thing's rules (the player's
during an end walk, while the world runs on), and a thing whose rules are
all off is moved by the script. A hold and the switches belong to the
visit: the next level starts without them. An overlay's own keys are the
script's. A press the ready-made rules used this tick (the action button
that started an `on_interact`) is used up: `input:pressed` is false for
it in that tick's `on_tick`, so the press that opens a dialogue cannot
also close it.

### The end of a run

- `world:win()` or `world:lose()`, at the tick's end: the fade out,
  `on_level_end("won")` or `("lost")`, then `on_world_end(result)`
  (a last chance to change the states for the record; no asks). The
  ready-made rules never end a run themselves (since stage 7): their
  goals and lives are the game's code.
- QUIT TO TITLE in the pause menu ends the run the same way with
  `"quit"`.
- Then the engine's part: initials if the score beats the table, the
  table, the title. An ending, credits or a final tally before that is a
  sequence the game plays before it asks to win.

### What each handler may do

| Where | changes the game | asks (`go_to`, `restart`, `win`, `lose`, `save`, `load`) | `math.random` | `input` | `level` |
|---|---|---|---|---|---|
| the top of `main.lua` | no: constants only | no | no | no | no |
| `on_world_start()` | the states | no | yes | no | no: there is none yet |
| `on_save_loaded(version)` | the states | no | no | no | no: it is built after |
| `on_level_start(how)` | yes | yes: it counts at once, before the level's first tick (no fade in) | yes | no | yes |
| the events (`on_touch` …) | yes | yes | yes | this tick's | yes |
| `on_player_join(p, how)` | yes | yes | yes | this tick's | yes |
| `on_player_leave(p)` | yes | yes | yes | this tick's | yes |
| `on_tick()` | yes | yes | yes | yes | yes |
| `on_level_end(why)` | the states | no: the level is already being left | yes | no | to read |
| `on_world_end(result)` | the states | no | yes | no | to read |
| `on_draw(frame)` | no: it only looks | no | no | the last tick's | to read |
| a scenario's `setup()` | yes | yes: done before the first tick | yes | no | yes |
| a console line | yes | yes: done at once, before the next tick | yes | no | yes |

A mistake here names the handler and the line. A level that asks to be
left in `on_level_start` again and again (a hub that sends on, a loop
by mistake) stops with a mistake after a hundred changes without a
tick.

### What each change keeps

| | a new run | `go_to` | `restart` | `load` | `reload` (a level file saved while playing) |
|---|---|---|---|---|---|
| `world.state`, `player.state` | from `c.state` | kept | kept | from the save | kept |
| `player.state.checkpoint` | none | forgotten | kept | from the save | kept |
| `level.kept` | none | kept | kept | from the save | kept |
| `level.state` | fresh | fresh | fresh | from the save | fresh |
| the level's things and voxels | from its file | from its file | from its file | from its file and the save | from its file |
| holds, switches, hit stop, shake | none | none | none | the level's from the save | none |
| events pending for the next tick | none | dropped | dropped | dropped | dropped |
| the player | at the level's start | at the place | at the place, the checkpoint or the start | where it stood | where it stood |
| the camera | follows the player | follows the player | follows the player | follows the player | as it was |
| the music | the level's | the level's (going on if the same, its volume and muffle kept) | going on | the level's | going on |
| keys held | as held | as held (no new press) | as held | as held | as held |

A test keeps this true: a level entered, left and entered again starts
with the same things, voxels and `level.state` as the first time; a run
restarted twice plays as a fresh one. Saving `main.lua` while playing
changes no state (the functions are swapped); either kind of reload marks
the run as no longer starting from the files, so it is not kept as a
replay.

## Events

The engine calls a handler if the script has it:

| Handler | When |
|---|---|
| `on_world_start()` | once, when a run begins, before the first level; not when a save is loaded (the run goes on) |
| `on_world_end(result)` | the run ends: `"won"`, `"lost"` or `"quit"`; after `on_level_end`; the end cannot be undone here |
| `on_level_start(how)` | a level was entered: `"start"`, `"go_to"`, `"restart"`, `"load"` or `"reload"`; its things in place, `level.state` fresh (or from the save), the player at its place |
| `on_level_end(why)` | the level is about to be left: `"go_to"`, `"restart"`, `"load"`, `"reload"`, `"won"`, `"lost"`, `"quit"`; copy from `level.state` what should outlive the visit |
| `on_beat(beat)` | a beat of the game's own song begins (`music:play`; from 0 at its start, on through its loops), before `on_tick`; none while the player's song plays |
| `on_tick()` | every tick, after the engine's own rules |
| `on_draw(frame)` | every picture (as often as the screen shows one, apart from the ticks); only looks, draws on `frame` |
| `on_touch(a, b, where)` | two things begin to touch: once per pair, not every tick; the player is always `a`, else the thing that came first; `where` is the face of `b` that `a` met (`"top"`, `"bottom"`, `"side"`). What lasts while touched is asked in `on_tick` (`thing:touching`) |
| `on_touch_end(a, b)` | they stop touching (not when one is removed or the level is left) |
| `on_interact(thing, by)` | `by` acts on it on purpose (the game's action called `interact`, pressed while the player touches it; the nearest of several): talk, open, read, apart from bumping into it |
| `on_hit(thing, by, damage, weapon)` | something hurt it (its `state.hp` already lowered), with what if known; a hit is refused before, by `thing.state.invulnerable` |
| `on_death(thing, by, weapon)` | its `state.hp` reached 0; once; never because its level was left. For the player the ready-made rules only show the fall: a life less, a restart, the end are the game's |
| `on_fall_out(thing)` | it fell out of the level (`c.physics.lost_below` voxels under it, 48 unless set); once until it is back; other things are taken away after it, the player waits for the game |
| `on_pickup(thing, item)` | it took an item (its `state.inventory` already raised) |
| `on_save_loaded(version)` | a save was loaded, before its level is built: `version` is the game's `c.saves.version` that made it; bring an older one's states up to date |
| `on_player_join(p, how)`, `on_player_leave(p)` | a seat is taken or left during the run ("Several players") |

There is no goal event: a goal is a thing the script answers in
`on_touch` and its states, and since stage 7 the ready-made rules have no
goals of their own either (Voxkeep's door is a prop its `main.lua` opens
once every gem is taken, `thing.state.pose = "open"`, and the touch of
it leads on to the next room or wins; More Blood's
helipad: `on_death` counts the gunships down,
`on_touch` and `on_touch_end` say whether the player stands there, `on_tick`
wins once both are so; `on_touch` alone would miss a gunship falling while
the player already stands there). After the run's end (`on_world_end`)
the script hears nothing more: no events, no `on_tick`.

Later, in the same style: `on_use(user, item, target)` (an adventure's
verbs, with a fallback for uses nobody handles: "I can't do that"),
dialogue, and handlers per kind and per family (where they live is
decided before stage 6).

Not yet, each one an addition that breaks nothing (found by writing
seven kinds of game against this model, `shots/research/crosstest.lua`):
a pointer in `input` for point-and-click games, the camera held between
bounds for a beat 'em up's waves (`level.camera:limit`), timers by name
(`on_timer`), ray casts (`level:cast`), and a kind's
numbers that follow a state (enemies faster on the second loop).

### A tick, in order

1. Input is read (with several players each seat's, and the seats
   taken and left this tick).
2. The ready-made behaviors run, kind by kind in their list's order
   (not while the level is held).
3. The events go to the script, every one, none dropped: first seats
   taken and left (`on_player_join`, `on_player_leave`, under a hold too);
   then what the
   script caused last tick (`thing:hurt`, `thing:take`: `on_hit`,
   `on_death`, `on_pickup`; its own things fallen out of the level:
   `on_fall_out`) and its things whose hit points it set to 0
   (`on_death`); then what the ready-made rules did this tick, in the
   order it happened (`on_hit`, `on_death`, `on_fall_out`); then touches
   begun and ended (`on_touch`, `on_touch_end`) and `on_interact`. A
   thing removed this tick gets no more events. What a handler causes (a
   spawn in `on_touch`, a blast in `on_hit`, hit points set to 0 in
   `on_tick`) is delivered in the next tick's step 3.
4. The beats of the game's song begun since the last tick, each
   (`on_beat`); the music's clock went on before step 1, every tick, a
   hit stop's too, and a song asked for began there at its moment.
5. `on_tick` runs.
6. The tick's end, in this order: things removed go; the things the
   script moves move (`thing:walk`); a `world:save`; the bound ask, if
   any (a level change, a restart, a load, a win or a loss: "Leaving a
   level" above).

The set of things touching each other is state: snapshots carry it, so
"once" stays once after a restore; it is made from the bodies, which the
state hash takes. A save does not keep it: a load arrives, and what
touches there touches without `on_touch`.

## Levels and places

- **Where levels lie.** A game's levels are the files in its `levels/`
  folder ending `.toml` (levels of blocks, maps) or `.vvx` (worlds)
  (`levels/hall.toml`), named by the
  file's stem and ordered by name, numbers counted as numbers (`2` before
  `10`; `01_hall`, `02_tower`). That order is `world.levels`, the first
  starts, `level.number` and `level.next` follow it. No list in
  `conf.lua`: the script decides where to go, the files say what exists.
  Other files there (Sokovox's puzzle text) are not levels.
- **A game without level files** has one level, its world as `c.world`
  says. Sokovox stays so: the puzzle number in `world.state`,
  `world:go_to(level.name)` enters the level afresh and `on_level_start`
  builds the next puzzle.
- **What a level file is.** As the game's `c.world` says: a level of
  voxel blocks (`c.world.blocks`: margin, backdrops for every level), a
  map, or a world (`.vvx`, `crate::scene`).
- **Settings per level** (its title, its music, the player's behavior
  in it: a top-down overworld with jump & run dungeons, GB Studio's proven
  cross) live in the level file, set in the level editor.
- **Places.** A level holds named places, each with a facing if it
  wants one (else the player faces as its rules turn it), set in the
  level editor; a named thing is a place too (arriving at `"door_west"`
  stands in front of that door). A door's props name the level and the
  place it leads to; both are checked when the game loads, a wrong name is
  a mistake with its place. Without a place, `go_to` arrives where the
  level starts: its place called `start`, else where its ready-made rules
  stand the player (the hero's marker), else `c.player.spawn`; the first
  level of a run starts there too. Levels of blocks and maps hold places;
  a world (`.vvx`) has no room for names, so it has only its start.
- **Pieces and areas.** A level's file sets down pieces (models fixed
  where it puts them, lights, solid ones holding bodies up), each a thing
  of the kind `"piece"`, names areas (`level.areas`) and may set its own
  daylight (`[light]`). Its things may have names of the file's;
  a name is one thing's in the level, checked as the game loads, as a
  piece's model is (a missing file names the level and the line).
- **Replays** start at the first level (or at the save or `--level` they
  began from); every level's files count for their fingerprint, the
  pieces' models among them.

### Pieces, lights and areas

Pieces are for scenes built as stages (a room of models,
lamps, named spots; a jump & run's room and an adventure's alike): a
level is one format (`grid::text`, "voxcade level 1"), with blocks or
without (`size` in voxels then), and what is set down in it. A
`[[thing]]` with a `model` and no kind is a piece: a thing of the
engine's own kind `"piece"` (last among the kinds, so the game's keep
their places; a game's own kind may not take the name), fixed where the
file puts its pivot, drawn by the engine, its model any `.vvx` of the
game the level names (part of the fingerprint as a level file is).
`facing` names one of the four ways; `rotation` (a bearing, clockwise
from north) needs `c.camera.rotates = "free"`, the game's choice for every
level and the editor's, else the level does not load and the mistake
names each thing turned off the grid and both ways out (allow free turns,
or align them in the editor: `grid::text::align`, a tie going clockwise).
A thing of a kind takes `facing` or `rotation` too: it starts
facing that way, as `level:spawn`'s `facing` turns it (`thing:face`;
`lua::host::Stage::name_things` turns each thing the file turns, its
kind's markers in order). Only a kind without a behavior, which the
script moves: one with a behavior turns by its ready-made rules, and
its level does not load (`check_facings`). A kind with `rotates = "free"`
faces the bearing exactly, any other the nearest of the four ways;
`pitch` and `roll` stay the pieces'.
A piece's `at` is its pivot cell (the model's cell `size / 2` across and
deep, on its floor; "A model's pivot" below), and `thing.x`, `thing.y`
read `at + 0.5`, the middle of that cell (the grid's low corner added):
a cube put relative to a piece goes at `math.floor(thing.x) + dx`.
A piece may be tipped besides: `pitch` (its nose up, about
its own right side) and `roll` (its right side down, about its own
forward), degrees, the words and the order `frame:model`'s rotation has
(`session::turning`): the rotation is `Rz(-rotation) · Rx(pitch) ·
Ry(roll)`, roll taken first, then pitch, then the rotation round, so
`rotation` (or `facing`) stays the bearing seen from above whatever the other two,
and a piece is drawn exactly as `frame:model` draws a thing facing north
with `{ rotation = rotation, pitch = pitch, roll = roll }`. All three turn about
the same point, the middle of the voxel at half the model's height over
the pivot cell (`render::turn_centre`: a model 30 tall turns about 15.5
up, one 21 tall about 10.5): a voxel's middle, so a quarter turn about
any axis puts every voxel on the grid again. Written only when not 0, on
the grid only 0, 90, 180 or 270 each (the level names a piece off them,
as it does a `rotation`; ALIGN ALL takes the nearest quarter, a 0 taking its
line out). A solid piece may lie any of the 24 ways a box can; its cells
are exactly where it is drawn (`session::piece_cells`, test
`a_solid_piece_turned_any_way_is_where_it_is_drawn`). Laid down, a piece
lies round its middle, so its `at` (where the standing model's pivot
was) goes under it: a stove 20 across, 14 deep and 30 tall, `at = [40,
150, 0]`, `facing = "north"`, `pitch = 90` lies on its back in x 30 to
49, y 136 to 165 and z 8 to 21; the editor keeps a piece on the floor
it stood on as it tips it (its lowest voxel stays where it was), so it
writes `at = [40, 150, -8]` and the stove lies in z 0 to 13 (`at`'s z
below the floor is in order for a piece; it does not fall out). Its Z
arrow (or the number) moves it up from there.
A piece may bring a light (`light = { at, radius, color }`, `at` from the
pivot in the model's voxels, turning with it) or several (`lights = [ {
... }, { ... } ]`: a lamp's flame and its wider glow; `light =` is a list
of one, both on one thing a mistake), or be a light alone; all its
lights move and turn with it, and `thing.state.light` (0 to 1, 1 at
first) dims them together (an engine key kept in `Stage::lights`:
snapshots, saves by the piece's name, and the state hash only where a
level has lights, so earlier replays stay as they were). Not yet: a light of
its own each, as a list, `thing.state.lights[k]` (1 at first, multiplied
with `light`), so `light` keeps its meaning. A piece
whose model is a figure of parts (`[[part]]`s: a door's frame and its
leaf hung on the hinge by `parent` and `joint`) is posed through its
state: `thing.state.parts = { leaf = { rotation = 100 },
handle = { roll = 20 } }`, exactly the pose `frame:model` takes (each
part by its name, `rotation`, `pitch`, `roll` in degrees about its
joint, `offset = { x, y, z }` in voxels (moved after the rotation, its
joint along: a part's `offset` in the file and the pose's add up), what
hangs on it moving along; `turn` refused with its new name), and the engine draws the piece so every picture, its own
rotation, pitch, roll and lights as before (its lights do not follow a
part). nil or an empty table: at rest, drawn exactly as an unposed
piece. It is the script's data in the thing's state, an engine key only
in that a mistake is said: as it is set (the line that set it: a part
the figure lacks, a model that is no figure, a light alone, a rotation
that is no number, a pose that is no table), and at every tick's end (a
part changed inside the table), naming the piece and the part; a
mistake halts the game, the piece standing at rest meanwhile. As every
`thing.state`, it is in the state hash (only where something is set, so
earlier replays stay), snapshots and `inspect`, and lasts as long as the
thing lives in this visit of the level: saves do not keep it, nor does
entering the level again. What must outlast the visit (a door left open)
the game keeps in `world.state` and sets again in `on_level_start`. A solid piece
blocks in the cells of its model at rest: a pose moves no cell that
blocks (later work). Only pieces: a thing of a kind without a behavior
is drawn by the script (`frame:model`, which takes a pose), and the
kits' own figures by their rules (the jump & run's `thing.state.pose`
is a set of frames). A picture
shows the 16 lights nearest the middle of the view
(`render::MAX_LIGHTS`), each light of a piece counting. A piece with
`solid = true` holds bodies up and stops them, rounds and debris too,
exactly in the cells its model is drawn in (`session::quarter_cells`, its
first frame, glass blocking as the rest): the level's world keeps them
beside its voxels (`world::Solids`, consulted by `World::solid`,
`blocks` and `hold`), so its voxels stay empty (`level:voxel` says 0,
`level:solid` true), the model draws them once, and the world's palette
is not touched. Only in quarter turns: a solid piece turned off them
does not load, in a game that turns freely too. It does not move; taken
away (`thing:remove`) its cells are free at the tick's end with it; a
blast does not blow it away, and it holds no voxels of the world up
(they fall as before). Which were taken away is in the state hash only
where a level has solid pieces. A piece with `shadow_only = true`
casts its shadow and is not drawn (a ceiling over a room the camera
looks into from above, closing it against a low sun): its opaque voxels
go into the sun's shadow map exactly as the same piece's would, posed
by its state too (`ModelDraw::shadow_only`, `Drawing::shadow_only`),
never into the scene, the outline or the glow; its glass casts nothing,
as always; one above the world raises the shadows' box to it. It is a
thing by its name as any piece; `thing:remove` takes its shadow away.
It brings no light (refused, plainly: a light alone does that), and
blocks only with `solid = true` as well. Only the picture: nothing of
it is in the hash. A level's file may have its own
daylight, `[light]` with `c.light`'s words (`sun`, `sun_color`,
`ambient`, `shadow`, `sky`; colours `"#rrggbb"` or three numbers from 0
to 1, as in `c.light`, which takes both too): over the game's for
that level, under `frame:daylight` in `on_draw`, the picture only (no
hash); the meshes keep the faces every level's sun lights (`Game::suns`).
`[[area]]` is a named box: `level.areas.NAME`,
`area:holds(thing or point)`. Names given in the file are the things'
names (`level:thing(name)`); unnamed ones keep their kind and number. Not
yet: areas' events, setting a light's colour or radius in code.

## Items and the inventory

- Items are data: `c.items.key = { model = "key.vvx", stack = 1, order =
  1 }` (`stack`: how many one inventory holds, 1 unless said; `order`:
  where it comes in `world.items`, then by name). `world.items` lists
  them in that order, each `{ name, model, stack }`, for drawing an
  inventory.
- An inventory is state: `player.state.inventory` (a chest's or a
  trader's: `thing.state.inventory`), a table of name → count:
  `if player.state.inventory.key then … end`. A game with items starts
  every run with an empty one for the player. `inventory` is the engine's
  key in every game, with items or without: a game's own table by that
  name is checked as one. What one piece holds of its
  own (wear, charges) goes into the owner's state beside it.
- A pickup counts into the inventory by the game's code: touched
  (`on_touch`), `player:take(b.props.item)` (a full stack takes no more;
  the code removes the pickup or leaves it). An inventory naming an item
  not in `c.items`, or counting outside 0 to its stack, stops the game at
  the end of the tick, naming the key.

## Kinds, behaviors and families

- **Behaviors are a list per kind**, run in that order:
  `c.things.thug = { behaviors = { "brawler", "flash" }, … }`. They come
  from any of the engine's sets (jump & run, shooter, later a brawler's,
  an adventure's); `c.kit` goes. Each behavior's numbers are the kind's,
  overridden per placed thing by its props. `thing:enable(name, on)`
  switches one off for one thing.
- **Families** group kinds: `c.things.thug = { family = { "enemies" } }`.
  `level:things("enemies")` and `level:count("enemies")` take a family;
  later a family has handlers and state defaults its kinds share and may
  override.
- **`fixed = true`** (`c.things.NAME.fixed`): a thing of the kind stays
  exactly where it is spawned or placed (`level:spawn(..., { z = 12 })`,
  `thing:place`): no gravity, no landing on voxels, `thing:walk`, `push`
  and `jump` move nothing. For kinds the script moves, so the ones
  without a behavior (and the same in a game with a kit); on a kind with
  a behavior it is a mistake, for its ready-made rules move it.
- **`like = "other"`** builds a kind on another, as today.
- **Props are declared** with their type and default:
  `c.things.door = { props = { to = "level", at = "place", locked = false } }`.
  The editor shows them as fields; a misspelt or missing one is a mistake
  with its place (`level:spawn{ props = }` checks them already: a key
  the kind does not declare, or of another type, stops the game naming
  `c.things.NAME.props`). `thing.props` and `thing.families` are only to
  read.

## Save games

- **What a save holds:** `world.state`, every `player.state`, every
  level's `level.kept` and kept things, how often each level was entered,
  the level being played, its `level.state` with its holds, the touching
  set, where the player stands, the numbers the engine keeps for the
  player and the ready-made rules' run (the shooter's weapon, ammo and
  grenades, the platformer's hearts), the play time and the random
  generator. The level's file is read afresh when loaded, so saves survive
  updates of the game; what the level's file would not bring back (holes
  blown into it, colours a script added) is saved as the changed voxels.
  It is TOML a person can read, the states in it written as Lua tables.
- **Loading** asks like a level change (`why` and `how` are `"load"`):
  the level from its file and what it kept, `level.state` back, the player
  where they stood, then `on_level_start("load")`. Things of the level
  named in `level.state` are found again by name; handles to things it
  spawned come back as nil, and a game that spawns in code (Sokovox's
  crates) places them again from `level.state`.
- **Versions.** `c.saves = { slots = 3, version = 2 }` (at most 9 slots: a
  menu shows them all). A save carries
  the version it was made with; every load is handed to
  `on_save_loaded(version)` before the level is built: an older save is
  brought up to date there (and, later, filled in from the defaults,
  `c.state`). Names a save holds that the game no
  longer has (an item, a level) are dropped with a warning, never a failed
  load.
- **In the game:** with slots, the pause menu has SAVE and LOAD, each slot
  with its level, play time and how long ago it was saved (the clock is
  the menu's: a script sees the level and the play time only). LOAD loads
  the game afresh from its files, as RESTART does, and goes on from the
  save at once: the fresh run's `on_world_start` and first
  `on_level_start` run, then `on_level_end("load")` and
  `on_level_start("load")`, and what they set gives way to the save's
  states. Scripts: `world:save(slot)` (save points, saving on its own in
  `on_level_start` after a level change), `world:load(slot)` (an empty
  slot is a mistake with its line), `world.saves`. A save is written
  at the tick's end before an ask is done, so a save and a `go_to` in one
  tick save the level being left.
- **Where:** in the player's config folder (`saves/<game>/1.toml`), never
  in the game folder, like the settings and the high scores.
- **Replays:** a run begun from a save carries it, and so does every
  save it loads on the way (its content, at its tick), so `check` plays
  it from the replay, never from the slots as they are today. LOAD from
  the pause menu ends the run being recorded; the loaded one is recorded
  anew, beginning from that save. `shot`, `check` and `inspect` never
  write the player's slots: their saves live for that run only.
- **Snapshots** (`check`'s guard plays a second twice) carry the level
  being played, every level's `level.kept`, `level.visits` and the
  touching set besides the states, so a second with a level change in it
  plays the same.

## Scenarios and the console

What happens late in a game (day 30 on a farm with every pen full, the
fifth room with its lever pulled, a wolf at a broken fence) must be easy
to reach for the one who makes the game and for an agent testing it:
without playing there first, without a save written by hand from the
states' inner tables. So both are part of every game: the console is the
engine's and always there, and every game keeps scenarios for its late
situations (the engine's own games and templates all have some, a test
says so).

- **A scenario** is a named start of a run, written as code:
  `scenarios/NAME.lua` in the game folder gives back a table, all of it
  optional but that it says where the run begins (`level`, `place`) or
  sets it up (`setup`): a scenario of a level alone, `return { level =
  "cellar" }`, begins the run there:

  ```lua
  -- scenarios/wolf_night.lua
  local set = require("setup")            -- the game's own words for it
  return {
    about = "Day 12, 21:30: hens and sheep, a gap in the sheep's fence",
    level = nil, place = nil,             -- where the run begins, if not the first level
    setup = function()
      set.day(12, "21:30")
      set.animals("sheep", { "f", "m" })
      set.gap("sheep")
    end,
  }
  ```

  The run starts as every run does (`on_world_start`, the first level's
  `on_level_start("start")`); a scenario with a `level` then goes there as
  `world:go_to` would (`on_level_end("go_to")`, `on_level_start("go_to")`
  at `place`); then `setup()` runs once, before the first tick, as
  `on_tick` could (the states, things, the player's place; `input` is not
  there). Asks it makes are done before the first tick. Its file may
  `require` the game's files (the same modules `main.lua` has, and its
  own: a game's helpers for scenarios); its globals are `main.lua`'s,
  frozen. A mistake halts the game with its line, as one in
  `on_level_start` does.
- **Its words are the game's.** Plain state writes work, but a scenario
  reads and lasts best when the game gives it verbs (BunnyFarm's
  `setup.lua`: a day and an hour, the level, animals, a gap in a fence,
  the bunny at a place), which keep the states consistent with each other.
- **Where:** `run`, `shot` and `inspect` with `--scenario NAME`, the IDE's
  SCENARIOS (each played with PLAY), the MCP tools likewise; `voxcade
  scenarios GAME` lists them with their `about`.
- **Replays:** a run begun from a scenario carries its name, and the
  scenario's file and what it required belong to the code the replay's
  fingerprint is made of. `check` plays it from the scenario as it is now:
  a test that outlives a new layout of the states, where a save written
  by hand would need bringing up to date.
- **The console** runs a line of Lua in the game between two ticks, as a
  scenario's `setup` does (`require` gives the game's files: `require
  ("wolf").night()`): in the window Quake's key (left of 1: `` ` `` on a
  US keyboard, `^` on a German one) slides it in at the bottom, a few
  lines high, pushing the game up, and out again (Escape too); under OUTPUT
  in the IDE while the game plays there; `--do "LINE"` for `run`,
  `shot` and `inspect` (before the first tick, after a scenario), `eval`
  over MCP for the game the IDE plays. It shows what the line gives back
  (an expression's value) and what it printed; a mistake is shown and the
  game goes on (a state left holding what is no plain data stops it, as
  at a tick's end). The game stands still while the console is open.
- **Recorded:** a console line is part of the run. A replay carries each
  with the number of steps it came after (`evals`) and runs it there
  again, so a run with a wolf called in is kept, put in `replays/` with
  F9 and checked like any other.
- **In the players' hands too:** the console is there in every build (the user, 2026-10-29: whoever wants to cheat may); the maker's keys F5 and F9 are not, a player of a packaged game has them only behind `voxcade run --maker` (the IDE's PLAY always, 2026-11-13). A run
  it helped (a line run in it, or begun from a scenario: `Session::helped`)
  goes into the high score table as GOD, no initials asked: the table
  says so. Switching the console on from the command line only stays an
  idea.

## Several players

For games without ready-made rules and with both kits', at one screen
and over the network (`docs/multiplayer.md`). Two to four players at one screen as at an arcade machine (no split
screen), eight and more over the network; co-op, versus or teams as the
game decides. A game of one changes nothing: `c.players` unset is 1,
`player` is the one who plays, and its replays and hashes stay as they are
(a test: nothing new enters the hash until it is set or a second seat is
taken).

### Seats

- **`c.players = 4`:** the seats a game has, how many play at once.
- **`world.players[n]` is seat n,** for the whole run: its player, or
  `false` while nobody sits there (as `frame:grid`'s empty slots).
  `#world.players` is the number of seats; `ipairs` walks them all, so a
  HUD draws PRESS START where a seat is `false`.
- **`p.seat`:** a player's seat; a level's thing has none, so
  `if thing.seat then` asks whether a thing is a player, in any handler.
- **`player` is seat 1 only,** and `input` and `controls` are seat 1's:
  in a game for several, a rule for every player loops over
  `level:players()` and tells a player by `thing.seat` (`a ~= player`
  leaves out seats 2 to 4). Over the network `player` is the host's on
  every machine, never "mine".
- **Seat 1 is always taken,** by whoever started the run (the host's
  player over the network): the run is theirs. One who stops playing
  goes out of play as the game's rules say (below); a seat 1 walked away
  from would hold the others back, so a game where that matters lets an
  idle player go out (its code). The host's machine leaving ends the
  game for everyone (the network's, not the model's).
- **Taking a seat:** at one machine, START on a pad no seat has (seat 1
  plays the keyboard and the pad that began the run); the keyboard takes
  a seat with Return (unless the game binds Return): while seat 1 plays
  on a pad, the whole keyboard with the game's own keys (one on the pad,
  the other on WASD and the game's keys, the user's wish), else it is
  shared, each half a player's with keys of the engine's layout
  (`src/seating.rs`: the left half WASD and F G R T V B for the game's
  actions in order, the right half the arrows and K L I O , .); the
  settings' CONTROLS in a game of several (the players page) give each
  seat its part of the keyboard and its pad and each player its own keys
  (the halves' directions too), and what it shows plays from a run's
  start; over the
  network, a machine connecting to the
  host. Whenever a seat is free, during a level too: the arcade's
  way, whoever comes plays. **Leaving:** LEAVE in the pause menu (any
  seat but the first), or the machine leaving the network game; a pad
  unplugged keeps its seat, holding nothing.
- **A player is plain data in every state** (a seat outlives every
  level, unlike a level's thing): a shot remembers who fired it
  (`state = { by = p }`), a match its champion (`world.state.champion =
  p`). It is written and hashed as its seat, and there is one handle per
  seat, so `==` holds across a save and a seat taken again. A seat that
  left reads on: `p.state` is kept, the player is out of play.

### What each player has

Everything a thing has (`p.x`, `p:walk`, `p:place`, `p:hurt`, `p:take`,
`p:enable`, `p:touching` …), and:

| Name | What |
|---|---|
| `p.seat` | its seat, 1 to `c.players` (read-only) |
| `p.state` | its run's state, as `player.state`: `score`, `hp`, `inventory`, `checkpoint`, `invulnerable`, the game's own (lives, bombs), each player's own; a seat taken starts from a copy of `c.state.player` |
| `p.input` | what it holds and presses, as `input` (in `on_tick`; the last tick's in `on_draw`) |
| `p.controls` | what it plays with, as `controls` (`p.controls.device`, `p.controls:key("fire")`) |
| `p.state.kind` | an engine key: the kind of its figure, `c.player.kind`'s unless set (a kind, or a list by seat: `{ "hero", "hero_blue" }`, the last for the seats after it); set while it is not standing (a character select), the figure comes in as that kind; changed while it stands, the figure becomes that kind where it stands, at the tick's end |
| `p.state.out` | an engine key: `true` takes the player out of play (below) |
| `p.state.team` | an engine key: its team, `"player"` unless set; the ready-made rules' "who hurts whom" follows it |

The engine keeps `kind`, `out` and `team` where it keeps its other
numbers, read and written through: nothing of them is in a state's table
until set. With ready-made rules every player's figure is theirs: a hero
each, a soldier each (the shooter's own figure by `c.player.model`, a
model or a list by seat, as `c.player.kind`).

### In play and out

- **`p.state.out = true`** takes a player out of play: out of
  `level:players()` at once (as a thing taken away is out of the level's
  lists at once), its figure off the level at the tick's end; no touches,
  hits or holding the others; the ready-made rules pass it by. Its
  `p.state` stays and its input is still read, so it can press to come
  back. For a co-op player with no lives left (the other plays on), a
  versus player beaten in this round, one joining while a round runs.
- **`false` (or nil) brings it back:** it comes into the level as every
  figure does (below).
- **A level entered** keeps who is out (`p.state` is kept): the game lets
  them back in where it wants (a versus game's next round, a co-op game's
  next level).
- **`level:players()`:** the players in play in this level (taken, not
  out), in seat order, without gaps; lying down with no hit points still
  counts (a friend may revive it). `world.players` is every seat.

### Coming into a level

One rule for a seat taken, a player back in play and a level entered:
the figure stands from the tick's end (or, entering a level, before
`on_level_start`), so no ready-made rule, touch or hit meets it before
the game's code has seen it. Where:

1. where `p:place` put it in that tick (a figure not standing yet is
   placed where it will come in);
2. else, as a level is entered, at its arrival place: the place
   `NAME_n` for seat n when the level has one (`start_2`, `west_3`), else
   `NAME` (players pass through each other unless their rules say
   otherwise, so standing in one another is fine);
3. else, during a level, where the first player in play stands (seat
   order).

Asking where it stands before then is a mistake that says so.

### Joining and leaving

| Handler | When |
|---|---|
| `on_player_join(p, how)` | a seat is taken during the run (not the seats there as it begins: those start from `c.state.player`, as seat 1 always did); in its tick, first among the events, under a hold too; `how` is `"new"` (`p.state` a copy of `c.state.player`) or `"back"` (the seat's state from before it left, or from a save); its figure comes in at the tick's end, so here it may be placed, given a kind, or kept out (`p.state.out = true`) |
| `on_player_leave(p)` | a seat is left: in its tick, first among the events, under a hold too; out of play from the tick's first step (the ready-made rules pass it by), its figure gone at the tick's end; it hears nothing more. `p.state` is kept for the seat: whoever takes it again in this run comes `"back"` with it, and a save keeps it |

Seats taken and left come from the machines, so they are input: read in
the tick's first step, written into replays with their ticks, stamped by
the host over the network. A game cannot refuse a seat; it lets the
newcomer wait (`out`). A join in a tick that also asks for another level
comes in there, at its arrival place. A game wanting a fresh start for a
newcomer resets its state in `on_player_join` when `how` is `"back"`.

### Checkpoints and asks

- **Checkpoints** are each player's (`p.state.checkpoint`). A co-op game
  sets them for everyone as one passes (`for _, q in ipairs(
  level:players()) do q.state.checkpoint = c.name end`), and brings a
  fallen player back beside a friend (`p:place(friend)`), not at a
  checkpoint behind the others.
- **The asks are the run's:** `world:go_to`, `restart`, `win`, `lose`,
  `save` and `load` take everyone, each to its arrival place, its
  checkpoint or its start.

### The camera and the hold

- **`c.camera.hold = { 240, 150 }`:** how far apart the players in play
  and alive may be, across and deep (voxels; `{ 240 }` across only, a
  side view, whose camera never follows up). The rule is the
  simulation's, over every player in play, the same on every machine:
  each player's move is cut short as it moves (the ready-made rules and
  `p:walk` alike, players moving in seat order) where it would make the
  span between the two players furthest apart wider than the hold, so it
  never ends in a wall and the later seat gives way; a move that narrows
  the span is never stopped; players further apart than the hold (one
  placed far off) are only kept from going further. `p:place` is not
  stopped. Set it smaller than the view, with room for the figures; the
  window's size never changes it (a bigger window shows more around them).
- **Where the camera looks is the picture's:** the middle of the span of
  the players it follows, snapped as one player's is (anchored on the
  first player's drawn voxel, the way to the middle in whole voxels: a
  game of one draws as today, byte for byte), its height never changing
  (no zoom). With a hold, every machine looks at the middle of all the
  players in play and alive; without one, each machine at its own (over
  the network players may then roam apart). What it follows is the
  game's: `level.camera:follow(thing)` turns every screen to that thing
  (the boss coming in, a door opening), `level.camera:follow()` gives it
  the players back; the hold holds them meanwhile.
- **`frame.players`:** the players at the machine this picture is drawn
  for, in seat order: their HUD (each bunny's bag at its own machine,
  both at one). A key in braces in a text (`"PRESS {fire}"`) and
  `controls` in `on_draw` are the first of them's (seat 1's at a machine
  where nobody plays: it watches). `on_draw` only looks, so what differs between machines
  changes nothing in the game.
- **Each machine its own language** (the user, 2026-10-30): in `on_draw`
  `world.language` is the language of the machine drawing, in the
  simulation (`on_tick`, the events) the host's, so the game goes the same
  everywhere. A text the game keeps in a state is therefore written in
  the host's language: keep what it says (a key, a number) in the state
  and choose the words in `on_draw`. At one machine both are the same.

### Touches, hits and teams

- **`on_touch(a, b, where)`:** a player is always `a`. Of two players,
  the one that came down onto the other when one did (`where` is then
  `"top"`: a stomp, a hop off a friend's head), else the lower seat. Two
  players touching (a tag, a revive) is a touch like any.
- **Who did it:** `by` in `on_hit` and `on_death` is whoever did it, a
  player's rounds and blows its own, so the one who shot scores
  (`by.state.score = by.state.score + worth`); `on_interact(thing, by)`
  comes once for each player pressing `interact`, in seat order, each
  press used up for its own seat.
- **Teams:** `p.state.team`. All on `"player"` is co-op (the ready-made
  rules spare their own team); each on a team of their own is everyone
  against everyone; two names, two sides. The shooter's rounds, blows
  and blasts hurt players of another team than theirs (a weapon with
  `friendly_fire = false` spares its own; the aim assist finds players
  of another team as it finds enemies; `by` is the one who fired, one's
  own blast nobody's); the jump & run's touches are the game's to
  answer, by the teams if it likes. The ready-made rules' enemies aim at
  the nearest player in play, a hut sends men only while it lies ahead
  of every one.
- **Holds** are the level's: `level.state.held` stops every player's
  rules. A menu for one player that lets the others play on is the game's
  (a key of its own in `p.state`, its walk skipped while it is open).
- **The engine's pause** stops everyone: at one machine any player's
  START (unless the game binds START, as now); over the network the
  host's alone, its clock standing still, so every machine stops at the
  same tick (the others' START opens their own menu without stopping the
  game: stage 4).

### Scores, saves and the end

- **High scores:** each player's score is a line of its own (the
  arcade's way): at the run's end every player who beats one picks
  initials, one after another (PLAYER n over them), after the last
  tick. With
  `c.team_score = true` the run is one line instead, every seat's score
  added (those who left too), its initials picked by seat 1. Over the
  network each machine enters the lines of its own players into its own
  table. A run helped by the console or a scenario enters every line as
  GOD.
- **Saves** hold every seat's state (those away too) and where each
  player in play stands; a save of one reads as before. Loaded, each
  seat present gets its own back, and a seat saved but away waits for
  whoever takes it (`"back"`). Over the network a save is the host's and
  a load travels from the host with its tick (stage 4).
- **The end** is the run's: `world:win()` and `world:lose()` end it for
  everyone; who won a versus match is the game's to show.

### Replays, the hash, a game in Rust

- A replay carries each seat's input and controls, and every seat taken
  and left with its tick; `shot`, `inspect` and `check` take input for
  each seat (stage 2).
- Each seat is hashed by its number, seat 1 exactly as `player` is today.
- A game in Rust gets each seat's input through the `Game` trait
  (`examples/rust_digger` stays a game of one).

### Not in this model, each an addition later

- Standing on a friend (New Super Mario Bros.' lift up a ledge): the
  platformer kit's, a hero's own number in stage 3, no name in the API.
- Split screen: not wanted (the user: as at an arcade machine).

### The cross test

Five games written against this design
(`shots/research/crosstest-players.lua`, outside the repository): More
Blood for two (a life each, the killer scores, the helipad once every
living player stands there, a continue when out), Pulsar with two ships
(shots remembering their owner), Voxkeep with two heroes (the door leads
both on, a hop off a friend's head), a versus game for four (rounds as
restarts, wins in each `p.state`, waiting as `out`) and BunnyFarm for two
(one farm, a bag each, menus that hold nobody else, the night once both
are in bed, a save waiting for the second bunny). What they and the
reviews found is in the design above: `p.seat` to tell a player, seats as
fixed places with `false` where free, `out` leaving `level:players()` at
once, one rule for a figure coming in at the tick's end, players as plain
data, the hold as a span over every player in play, the camera the
picture's, `frame.players`, a character select through `p.state.kind`.

## Crosses, worked through

- **A Mario-like jump & run:** levels `01_meadow` … in `levels/`.
  `on_touch(player, flag)`: `level.state.clear = 0`, the player's hero
  behavior off; `on_tick` walks the player to the castle, counts the time
  into `player.state.score`, then `world:go_to(level.next)` or, after the
  last, `world:win()`. Checkpoints set `player.state.checkpoint`;
  `on_death(player)`: hold the level, a counter, then
  `world:restart(player.state.checkpoint)` or `world:lose()` with no lives
  left.
- **A jump & run RPG** (Voxkeep grown): zones are levels, the overworld
  top-down (the player's behavior set in that level's file), dungeons
  jump & run. Doors carry `to` and `at`; `on_touch` sends the player
  through. The hero has `player.state.hp`, an inventory (keys, potions)
  and abilities (`player.state.double_jump`); a locked door checks
  `player.state.inventory.key`. A chest's opening is kept in `level.kept`: opened stays
  opened. A save point calls `world:save(1)`; dialogue holds the level.
- **Zak McKracken meets Double Dragon:** rooms are levels; items in the
  inventory; `on_interact` for talking and opening, `on_use(user, item,
  target)` for "use key on door" (later; with a pointer for
  point-and-click); thugs `behaviors = { "brawler" }` in the family
  `enemies`, `on_hit` and `on_death` for the fights; `world.state` for the
  story so far; save games on.
- **Sokovox:** one level made in code; `world.state.puzzle` says which;
  `on_level_start` builds it (or brings it back from `level.state` after a
  load); `level.state` holds the moves and the undo history; R is
  `world:restart()`, a solved puzzle sets `world.state.done[n]` and goes
  on with `world:go_to(level.name)`.
- **More Blood:** one level, its map. `on_death(thing, by, weapon)`
  scores the kill (the knife's worth more) and counts the gunships down
  in `level.state`; a pickup touched gives what its props say; past a
  checkpoint `player.state.checkpoint` names it; the player down comes
  back there after a while, a life less, facing north, the enemies'
  rounds in flight cleared (`level:clear_rounds`); the helipad, a thing without
  rules, is touched within its reach (`on_touch`, `on_touch_end` note
  it); `on_tick` wins once both are so. No `goal_open`.
- **Idioms the model carries without new names:** an RPG's battle as its
  own level that returns to where the player stood
  (`world.state.back = { level = level.name, x = player.x, y = player.y,
  z = player.z }`, then `player:place` in `on_level_start`), or as a hold
  over the map with the battle in `level.state` (RPG Maker's way, the map
  untouched); a level select as a level whose name sorts first
  (`00_select`); a roguelike's levels made in `on_level_start` with
  `math.random`, the seed in `world.state`; a metroidvania's rooms
  entered from four sides through places at the edges (the speed is not
  carried over: `place` sets where and which way, an option later).

## What other engines taught

Studied on 2026-10-20: for the data model Adventure Game Studio,
GameMaker, RPG Maker MZ, Construct 3, Godot 4 and GB Studio; for the flow
of a run those again, Unity, Phaser 3, HaxeFlixel, PICO-8 (Celeste
Classic and Celeste Classic 2) and LÖVE (hump, roomy, Mari0, Hawkthorne),
with the classics' conventions (Super Mario Bros., Mega Man, Sonic,
Shovel Knight, Super Meat Boy).

| Lesson | Seen in | Here |
|---|---|---|
| Doors with level numbers and raw coordinates break when a level is edited; none of the six checks them | all six | names for levels and places, places with a facing, checked at load |
| "Taken stays taken" is every game's chore, built by hand from ids | GameMaker, Godot, RPG Maker (self switches), GB Studio | `thing.name` from the editor, `level.kept` (kept kinds: 0.7) |
| Levels that only ever reset are not enough; a whole persistent level is too much | Construct (Persist), AGS (rooms keep state), GameMaker (its own tutorial drops persistent rooms) | fresh `level.state` by default, `level.kept` by choice |
| Live object references in global state dangle after a level change | Godot, GameMaker | states outlive things only by name |
| Collision events every step cause double pickups | GameMaker, Phaser, HaxeFlixel | `on_touch` once, `on_touch_end` |
| Undefined or dropped event order breaks replays | GameMaker, AGS | one written order, nothing dropped |
| Level changes at once break code still running; at the frame's or tick's end, written down, works | Godot (redone in 4.2), hump (issue 46), GB Studio (aborts its scripts), GameMaker, Construct, AGS, Phaser, HaxeFlixel | everything at the tick's end, the handler runs on |
| No engine has a goal or "level complete"; a goal is an object or area the game's code answers | all eleven | no goal event; `on_touch` and the game's states |
| A goal or exit touched twice, a death after the win: every official sample guards by hand, keeps the first exit, and never takes a life once the level is won | Unity (Chop Chop, LEGO, 3D Game Kit; the FPS Microgame, by its code, has no guard), HaxeFlixel demos, Phaser, Mari0, Celeste 2, Hawkthorne, Construct | one ask counts: going on outranks starting over, among equals the first; a player never arrives dead |
| The world running under a fade lets the player die or exit again mid-fade; games add flags | Construct, GameMaker, Phaser, HaxeFlixel (Mode's `_fading`); frozen in GB Studio, Hawkthorne, HaxeFlixel's transition addon | fades run no ticks and read no input |
| A declared "next" order mixes menus with levels, crashes or does nothing past the end | Construct (Kiwi Story hard-codes), GameMaker (an error past the last room); the others only name targets | levels are the files in order; `level.next` nil after the last; the engine owns the menus |
| A restart re-runs everything; what leaks through it (statics, globals, timers, listeners, music) is the bug | Unity, Godot, Phaser, HaxeFlixel, GameMaker (`game_restart` keeps globals), Construct (five lifetimes), AGS (restart from a snapshot) | a run, a level and a restart start from the files and the states; a test |
| A restart has to say why: the end of a room cannot tell a death from a win | GameMaker (checkpoint tutorial), Construct (`isEndingLayout`) | `on_level_end(why)`, `on_level_start(how)` |
| A player arriving on a door trigger fires it at once | GB Studio; RPG Maker fires touches only after a step | contacts at arrival count as touching |
| Checkpoints live past a death but not past the next level | Celeste 2, Mari0, Shovel Knight, Unity's 3D Game Kit | `player.state.checkpoint`, a place name the engine forgets on the next level; `world:restart()` goes there |
| Lives are optional; instant retry is first class | Super Meat Boy, Celeste, Shovel Knight; Mari0 sets lives per map pack | lives are the game's, in `player.state` |
| Overlays as a scene stack rebuild or lose the world behind them | GB Studio (the stack reloads the scene), Hawkthorne (enter runs again), RPG Maker (works only because its state lives apart) | no stack: the engine's pause, a level hold, behavior switches |
| A hold that stops the world but not the overlay's code is the common need | Godot (process modes), HaxeFlixel (sub-states), GB Studio (pause logic), AGS (`PauseGame`), Unity (Chop Chop's states) | `level.state.held` |
| Time scales and timers that run on under a pause, or a pause that leaks into the next scene | Unity (`timeScale`), Godot (timers), HaxeFlixel (global tweens), Construct (the player can still fire) | the hold stops the rules, not the script; it belongs to the visit |
| A key held across a level change counted as a new press loops restarts | HaxeFlixel (issue 2270) | held keys stay held, no new press |
| Blocking scripts cannot be saved or replayed | AGS, GB Studio, RPG Maker | sequences as data in states |
| `on_death` fired when a level unloads | Construct (`isEndingLayout`) | only for deaths, once |
| Saves as memory dumps or project hashes break on every update | GB Studio, AGS, GameMaker (`game_save` deprecated), RPG Maker | saves are states by name, a version, defaults and `on_save_loaded` |
| Mixable behaviors per object type, in list order, switchable per instance; families | Construct | behavior lists, `thing:enable`, families |
| A player controller per scene is a proven cross | GB Studio | the player's behavior per level, in the level file |
| Global tuning for abilities leaks between scenes and is not saved | GB Studio (engine fields) | abilities in `player.state` |
| Typed per-instance variables shown in the editor | GameMaker (Variable Definitions), Godot (`@export`) | declared props |
| Autoload or global sprawl | Godot | five globals, nothing more |
| Talking and opening kept apart from bumping; a fallback for unhandled verbs | AGS, GB Studio | `on_interact`; `on_use` later with a fallback |
| Starting at any level needs its own path | Unity (Chop Chop's cold start), Godot | `--level`: the same order as a run |

Sources: the engines' manuals (adventuregamestudio.github.io/ags-manual,
manual.gamemaker.io, rpgmakerofficial.com/product/MZ_help-en,
construct.net/en/make-games/manuals/construct-3,
docs.godotengine.org/en/stable, docs.unity3d.com, docs.phaser.io,
haxeflixel.com/documentation, gbstudio.dev/docs, the PICO-8 manual,
hump.readthedocs.io), their source code and official samples (Godot's
scene_tree.cpp and demo projects, Unity's open project Chop Chop and the
3D Game Kit, Scirra's example projects, GameMaker's templates, gbvm,
RPG Maker MZ's core scripts, the HaxeFlixel demos, EXOK's Celeste
Classic 2), the open games Mari0 and Hawkthorne, and Yacht Club's and
Team Meat's design writing.

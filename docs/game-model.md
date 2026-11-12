# The game model and the Lua API, version 2

Status: a design, approved in outline on 2026-10-20 and revised the same
day twice: after a study of six engines' data models, and after a study
of how eleven engines and the classic games run a game from start to end
(below, "What other engines taught"; the reports are kept in
`shots/research/`, outside the repository). "The flow of a run" was
approved on 2026-10-21. Built in stages (the plan's
stages 2 to 8). Until a stage is built, `docs/engine.md` ("The Lua door")
and `voxcade api` describe what runs.

Built so far: stage 2 (2026-10-20/21), the core: the globals `world`,
`level`, `level.camera`, `player`, `input`, `screen`; the handlers
`on_world_start` (no level yet), `on_level_start("start")`, `on_tick`,
`on_draw`; `world.state`, `level.state`, `player.state`, `thing.state`,
with the engine's keys `score` (and with ready-made rules `lives`, `hp`)
and `thing.state.hp` where the kind declares it; one handle per thing
and one view per state; things refused in `world.state` and
`player.state`, views refused in any state, checked every tick;
`thing:remove()` at the tick's end; `level:spawn(kind, x, y, options)`;
`level:solid`; `thing.facing`. The engine keeps its numbers where they
were and reads and writes them through, so the replays' checks stayed
the same (`--refit`), but for Sokovox, whose state moved into
`level.state` (`--update`, the same play).

Stage 3 (2026-10-21), levels: the files in `levels/` (`src/levels.rs`, all
read when the game loads, in natural order; a level of blocks, a map or a
world as `c.world` says, text since 0.6.0: `.toml` and `.vvx`), named
places in level files (`[[place]]` with `name`, `at = [x, y, z]`,
`facing = [x, y]`), `world.levels`,
`world.leaving`, `world:go_to`, `world:restart`, `world:win` and
`world:lose` as asks (one counts, at the tick's end; asks in the first
`on_level_start` at once; a hundred changes without a tick a mistake),
`on_level_end(why)`, `on_level_start(how)`, `on_world_end(result)` (the
ready-made rules' own ends and QUIT TO TITLE too), `level.name`,
`.number`, `.next`, `.visits`, `.kept`, `.places`, `.arrived` (where the
player came in: a game without the engine's figure stands its hero there),
`thing:place(place)`,
`player.state.checkpoint` (forgotten on `go_to`, where `restart` goes),
the ready-made rules' `enter_level` (their run kept, a dead player back on
its feet), fades in a window (`c.transition`; no ticks, replays cut them),
a level file saved while playing re-entered (`"reload"`). Not yet: holds
and arrival touches (with the events, stage 6), named things as places and
`--level` (with the editor, stage 7), `"load"` (save games, stage 5).

Stage 4 (2026-10-21), the player and items: `player` is a thing handle
(everything a thing has, `player.id` nil, `player.kind` nil for the
shooter's own figure), its figure the ready-made rules' player or, without
them, a thing of `c.player.kind` that the engine places where each level
starts and keeps from level to level; `level:things()` and `level:count()`
leave it out, `player:remove()` is a mistake, and without a figure
`player.x` says so. `world.players`; `c.items` (`model`, `stack`,
`order`), `world.items`; `player.state.inventory` (there from the start
when the game has items) and `thing.state.inventory`, checked at every
tick's end; the ready-made `pickup` with `item = "NAME"` counts into the
player's inventory (a full stack takes no more). `inspect` shows the
player apart from the level's things.

Stage 5 (2026-10-21), save games: `c.saves = { slots, version }`,
`world:save(slot)` (at the tick's end, before an ask, or at the end of
the `on_level_start` that asked), `world:load(slot)` (in `on_level_start`
too: going on from a save as the game starts)
(an ask like `restart`: `"load"` for `on_level_end` and `on_level_start`),
`world.saves` (`{ level, seconds }` or nil per slot), `on_save_loaded(version)`;
slots in the player's config folder (`saves/<game>/N.toml`: TOML with
the states as Lua tables, `src/lua/save.rs`, `src/lua/data.rs`), for one
run only in `shot`, `check`, `inspect` and replays; SAVE and LOAD in the
pause menu (each slot: its level, the play time, how long ago); replays
carry the save they began from, the slots as they began, every save they
loaded and the menu's saves (`shot --screen save|load` shows the pages).
Not yet: kept kinds and the touching set in saves (with the events, stage
6), things in `level.state` found again by name (stage 7), `c.state`
defaults filling in what an old save lacks.

Stage 6 (2026-10-21), events: `on_touch(a, b)` and `on_touch_end(a, b)` (once
per pair as it begins and ends, the player first, else the thing that
came first; boxes, or a kind's reach, the larger of two (2026-10-22: the
player's own no longer hides a power-up's); arriving in
a level and `thing:place` touch silently; not for the dead), `on_hit`,
`on_death` and `on_pickup` from the ready-made rules (`Sim::events`; a
thing they took away as it died is read as it was until the tick's end),
`on_death` too when the script takes a kind's `hp` to 0, once until it
lives again (no longer listed or counted meanwhile), `on_interact` (the
action called `interact` on the nearest thing the player touches; the
press used up), holds (`level.state.held`: the ready-made rules, physics
and touches wait, `on_tick` and the time go on, from the tick it is set),
`player.state.invulnerable` with ready-made rules (ticks, or `true`), the
ready-made goals' `wins = false` (gone in stage 7, goals are the code's); `goal_open` and the kits' questions are
gone, More Blood wins in its `main.lua` (its replays recorded anew with
`--update`: the same play, the kit's state the same at every second, the
script's counter now in the hash). After the run's end the script hears
nothing more. Not yet: the lists of behaviours (stage 8: `thing:enable`
switches the one behaviour a kind has since stage D), a death show
playing on under a hold, `thing.state.invulnerable` for things other
than the player.

After the review of the API's list by Fable and Grok (2026-10-21,
`shots/reviews/fable-api.md`, `grok-api.md`): the list (REFERENCE in
HELP, `voxcade api`, `lua/voxcade.lua`) names the engine's keys
(`level.state.held`, `player.state.checkpoint`, `invulnerable`,
`inventory`), the asks' order in one place (`world.leaving`), what a save
keeps, that `math.random` is the game's own, and where `screen` draws;
renamed: `on_leave` is `on_touch_end` (it read like leaving the level),
`level:hitstop` is `world:hitstop` (it holds the run's time), `read` is
`world:read` (no free function besides the handlers); the old handler and
`read` are refused with the new names. `screen` is there everywhere for
the picture's effects (`screen:shake`, `screen:flash`, from `level:`);
drawing is on `on_draw(frame)`'s frame, only there: what is not there
cannot be called in the wrong place. What the player plays with is
`controls`, there everywhere (`controls.device`, `controls:key`): it
comes from their machine, so replays carry it like the keys held; a
text names a key by the action in braces (`frame:text("PRESS {jump}")`). New: `thing:hurt(damage, by?)` and
`thing:take(item, count?)`, so the
script hurts and gives as the ready-made rules do (their events told in
the next tick), and the directions in `input:held`, `pressed` and
`released`.

Stage 7 (2026-10-21/22), the game's code decides what a touch does (the
user: "otherwise it is too much magic and limits what you can do with the
engine"; the draft and Fable's review in `shots/reviews/touch-draft.md`,
`fable-touch.md`). The ready-made rules move things; what a touch, a hit
or a fall means is the game's code. The engine gives the facts:
`on_touch(a, b, where)` (`"top"`, `"bottom"`, `"side"`: the face of `b`
that `a` met, judged from where both stood when touches were last looked
for, `Stage::seen_at`, part of the snapshot: a fast fall onto a slime is
`"top"`), `thing:touching(kind or family)` for what lasts while touched,
`level:block(x, y, z)` and `thing:in_block(kind)` for the level's
blocks, `on_fall_out(thing)` (once; other things taken away after it),
the weapon in `on_hit(thing, by, damage, weapon)` and
`on_death(thing, by, weapon)` (`knife`, a `c.weapons` name, what
`thing:hurt(damage, by, weapon)` named). And the means: `thing:bounce(height)`,
`level:effect(model, at, fps)`, `player:place(x | place | thing)` for the
ready-made rules' player too (`Host::put_player`), names (a placed
thing is its kind and number, `door_1`; `level:spawn{ name = }`;
`level:thing(name)`; `player.state.checkpoint` names a place or a thing,
`world:restart` stands the player there), families
(`c.things.NAME.family`, `thing.families` as a set,
`level:things(family)`), props (`c.things.NAME.props`, read-only
`thing.props`, `level:spawn{ props = }`), keys the ready-made rules
keep in a thing's state (`Host::thing_keys`). A thing taken away
(`thing:remove`, a death) is out of the level's lists, counts and
touches at once; its handle reads on until the tick's end. The shooter
since then: a kill is worth what the code adds (More Blood's soldiers
carry `props.worth` and `knife_worth`), a pickup is a thing touched
(its props say what it gives; the code sets `player.state.weapon` by
its name in `c.player.weapons`, `ammo`, `grenades`, `hp`: keys the
ready-made rules keep, so the HUD and the gun follow), checkpoints and
the helipad are things without rules, the player down lies there until
the code brings `player.state.hp` above 0 (`player:place` at the
checkpoint first) or ends the run. Its replays and pins played on the
same, move for move (only the game's code now counts, `--update`).
Hostages too: touched within their reach, freed by the code
(`thing.state.freed = true`: the cage bursts, the HUD counts them, they
run off). The cage now bursts at the tick's end instead of amid the
ready-made rules, so More Blood plays differently from its first hostage
on (its own recorded run no longer wins; the user agreed, 2026-10-22,
and added a rule of More Blood's own: a hostage killed loses the run at
once, `on_death`).

Stage D (2026-10-22), the kits are building blocks for many games, not
one game's rules (the user: "we are writing an engine, not a moddable
game"; More Blood was a prototype). What a run starts with is the
game's (`c.state`: hit points, the gun in hand, the inventory, its own
lives), and one inventory holds rounds and grenades. Attacks are bound
by the game (`c.player.attacks`: an action's weapons in order, the first
that can; `thing:attack` for the code's own), a blow is a weapon (`reach`,
`cone`: More Blood's knife written out in its `conf.lua`). The HUD is the
game's `on_draw`, the engine helping with where and how (`frame.width`,
text sizes, `frame:box`, `frame:bar`, `frame:icon` from `c.icons`,
`frame:drop_in`, `screen:burst`; `frame:to_screen` where a point or a
thing of the level shows, for labels and numbers over it). Who hurts whom is data (`team`,
`immune`) and a blast does its weapon's damage (`friendly_fire`,
`falloff`). Ways are data (a hut's `ahead`, a hostage's `flee`, a
gunship's `offset`, `c.player.facing`; `level:clear_rounds` for a
mercy or a bomb). Figures have the means stage 8 builds on
(`thing.state.pose` on any, `thing:launch`, `thing:enable`,
`thing.wall`). `kit.toml` is a library to copy, merged into no game; a
kind's unset lengths and speeds follow its height from the figure they
were tuned on, and `voxcade kinds` prints every number in effect.
More Blood, Voxfox and Voxkeep play as before, move for move (traces and
pins compared at every step); where a check was recorded anew only what
its hash holds had changed. Test `the_kits_keep_no_game_in_them` keeps
a game's rules out of the kits.

Stage E (2026-10-23, built), for a world that goes on: what BunnyFarm
(`docs/bunnyfarm.md`, a farming game with game days, menus and three
languages) needs, built for every game. No new idea of the model: a menu
is a sequence in `level.state` with the level held, as dialogue and shops
always were; light is the picture's, so `on_draw` sets it.

- **The game's own Lua files** (built). `require("market")` runs `market.lua` from
  the game folder once and gives back what it returns (`require("lang.de")`
  is `lang/de.lua`; a second `require` of the same gives the same value).
  Lua's own name, as LÖVE has it, rather than an engine's (the one free
  function besides the handlers: it is the language's, like `pairs`).
  Only while the game starts (the top of `main.lua` and of the files it
  requires), as `world:read`; the files are the game's code for the
  fingerprint (as tokens) and for hot reload (saving any of them swaps the
  functions, the states stay), and `check`'s guard names a local of any
  of them that keeps state.
- **More letters** (built). The pixel font gets lower case (descenders a row or
  two below the line), the letters of German and French with their
  accents (ä ö ü ß à â ç è é ê ë î ï ô û ù œ and the capitals), and a few
  signs (· × → ← ▲ ▼ ✓ … „ “ ’ « » • &). Capitals and digits stay as they
  are, and the engine's own text (menus, the logo, a key's name in a
  game's text) is still drawn in capitals (`ui::engine_case`): every
  picture so far is the same, pixel for pixel. A game's text is drawn as
  written. Accented capitals keep their full height, the accent in two
  rows above (ZURÜCK reads as capitals in a German menu); lower case with
  an acute, grave or circumflex gets a row of air under it.
- **Light the game sets** (built). `frame:daylight{ sun =, sun_color =, ambient =,
  shadow =, sky = }` in `on_draw`: this picture's daylight, each key as
  `c.light` names it (`sun_color` new there too, white unless set), the
  ones left out as `c.light` says. A day and night is the game's code: it
  works the light out from its clock every picture. Only the picture: the
  simulation does not see it, replays need nothing. A game whose code
  names `daylight` has every face a sun above the horizon can light in its
  meshes (others keep only what their one sun and the camera can hit).
  Colours as the other frame calls take them (`{ r, g, b }`, 0 to 255);
  HELP's DAY AND NIGHT shows a day of 40 seconds.
- **Menus are the game's, drawn in the engine's look** (built). The game keeps
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
- **CONTINUE on the title** (built). A game with `c.saves` offers CONTINUE first
  when a slot holds a save: the newest is loaded, as LOAD in the pause
  menu does (the run begins from that save; replays carry it). START
  begins a new run as always; which slot it saves into is the game's
  (`world.saves` says which are free). The title's list grows upwards, its
  last item where it always was; `shot --screen continue` shows it.
- **The player's language** (built). `c.languages = { "de", "en", "fr" }`: the
  game speaks these. SETTINGS gets LANGUAGE (only with more than one);
  the first time it is the system's language if the game speaks it, else
  the first listed. `world.language` says which (`"de"`), everywhere, and
  changes when the player changes it; it comes from the player's machine,
  so replays write it down as they do `controls`. The engine's own menus
  speak it too (German, English, French). The game's texts are its own
  tables (`require("lang." .. world.language)` is not it: the language
  can change during a run; keep all and pick by `world.language`).
  Built as: the player's choice in their settings (`Settings::language`,
  the system's from SDL in a window only, so `shot`, `check` and
  `inspect` speak the game's first on every machine), sent with the
  controls (`Controls::language`) every picture; the engine's words in
  `src/lang.rs`; HELP's SPEAK THE PLAYER'S LANGUAGE.

Several players (2026-10-30, `docs/multiplayer.md` stages 2 and 3),
built for games without ready-made rules and with both kits' (a hero
for each seat; a soldier for each, its gun and rounds its own, the
enemies after the nearest player, its rounds and grenades naming it):
seats (`c.players`, `world.players[n]` or `false`, `p.seat`), each
player's `p.state` (score and hit points the engine's; `out`, `team`,
`kind` the engine's keys, nil until set), `p.input`, `p.controls`,
`level:players()`, `on_player_join(p, how)` and `on_player_leave(p)`,
figures per seat (`c.player.kind` a list by seat) coming in at the
tick's end, places `NAME_n`, each player's checkpoint, `on_interact` per
seat, players as plain data in every state (a save writes
`world.players[2]`), saves with every seat, `c.camera.hold` and the
camera on the players' middle, `frame.players`, high score lines per
player (entered in turn) or the team's (`c.team_score`). At one machine
pads take seats with START and the keyboard's right half with Return
(`src/seating.rs`); replays carry each seat's input (`away` while free),
`shot` and `inspect` take `--input2` and on, MCP's tools `players`.

Pieces (2026-11-12), for scenes built as stages (a room of models,
lamps, named spots; a jump & run's room and an adventure's alike): a
level is one format (`grid::text`, "voxcade level 1"), with blocks or
without (`size` in voxels then), and what is set down in it. A
`[[thing]]` with a `model` and no kind is a piece: a thing of the
engine's own kind `"piece"` (last among the kinds, so the game's keep
their places; a game's own kind may not take the name), fixed where the
file puts its pivot, drawn by the engine, its model any `.vvx` of the
game the level names (part of the fingerprint as a level file is).
`facing` names one of the four ways; `turn` (a bearing, clockwise from
north) needs `c.camera.turns = "free"`, the game's choice for every
level and the editor's, else the level does not load and the mistake
names each thing turned off the grid and both ways out (allow free turns,
or align them in the editor: `grid::text::align`, a tie going clockwise).
A piece may bring a light (`light = { at, radius, color }`, `at` from the
pivot in the model's voxels, turning with it), or be a light alone; its
brightness is `thing.state.light` (0 to 1, 1 at first; an engine key
kept in `Stage::lights`: snapshots, saves by the piece's name, and the
state hash only where a level has lights, so earlier replays stay as
they were). A picture shows the 16 lights nearest the middle of the view
(`render::MAX_LIGHTS`). `[[area]]` is a named box: `level.areas.NAME`,
`area:holds(thing or point)`. Names given in the file are the things'
names (`level:thing(name)`); unnamed ones keep their kind and number. Not
yet: a piece that holds bodies up (its voxels solid), areas' events,
setting a light's colour or reach in code, the editor shaping pieces.

## Why

The Lua API grew with the first games and thinks too short:

- `world` is both the whole game and the voxels of its one level.
- There are no levels; Sokovox builds its own from text.
- The player is no object with a state; there is no inventory.
- Game-shaped hooks crept in (`goal_open`, the shooter's alone).
- A game picks one set of ready-made rules (`c.kit`), so they cannot mix.

The aim, in the user's words: the best and most logical, yet still most
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

### `world`: the whole run

| Name | What |
|---|---|
| `world.state` | the run's state |
| `world.tick` | ticks the game has run in this run (60 a second): every tick `on_tick` runs; none during the engine's pause, a hit stop or a level change |
| `world.levels` | the levels' names, in order |
| `world.players` | every seat: `world.players[n]` is seat n's player, or `false` while nobody sits there (one seat until several players are built) |
| `world.saves` | what lies in each save slot (level, play time, when), read-only |
| `world.leaving` | nil, or why the level is about to be left (`"go_to"`, `"restart"`, `"load"`, `"won"`, `"lost"`): an ask is pending; nil again once the change is done (in `on_level_start`) |
| `world:go_to(level, place?)` | leave this level for another (a name, or a number in `world.levels`) at the tick's end; arrive at `place`, or where the level starts |
| `world:restart(place?)` | enter this level afresh at the tick's end: at `place`, else at the checkpoint, else where it starts |
| `world:win()`, `world:lose()` | the run ends at the tick's end (`on_world_end` says how) |
| `world:hitstop(ticks)` | the whole game holds still for a few ticks, `on_tick` and the time too: the punch of a hit |
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
| `level:spawn(kind, x, y, options?)`, `level:thing(name or id)`, `level:things(kind or family?)`, `level:count(kind or family?)` | its things; `options`: `{ z =, name =, facing =, props =, state = }`; a thing that died is no longer listed or counted (only its picture lingers until its rules take it away) |
| `level:explode(at, radius)`, `level:sound(name, at, volume?)` | what happens in it (a sound by its file's name: `sounds/NAME.toml`, or one the engine or a kit brings) |
| `level.camera:follow(thing)`, `level.camera:look_at(x, y)` | where the camera looks |

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
door) are keys in `player.state`, read by the behaviours, never global
settings. Several players: `player` is seat 1, the others are in
`world.players` ("Several players" below: a design, not built yet).

### A thing

| Name | What |
|---|---|
| `thing.id` | which one, for this run (read-only; never given to another) |
| `thing.name` | its name in its level, set in the level editor (`"door_west"`, or made up: `"door_3"`), unique in the level (checked at load); spawned things have none unless given |
| `thing.kind`, `thing.families` | of what kind, in which families (read-only); `"piece"` for a model or light a level's file sets down |
| `thing.state` | its state, as long as it lives |
| `thing.props` | what the level editor set on this one thing (a door's `to` and `at`), typed and defaulted by its kind; read-only |
| `thing.x`, `.y`, `.z`, `.on_ground`, `.facing`, `.wall` | its body, read-only: the physics' (`wall`: the way to a wall right beside it, or nil) |
| `thing:walk(way, speed)`, `:face(way)`, `:jump(height)`, `:place(x, y, z?)` or `:place(place)` | moving things the script moves: kinds without behaviours, and things whose behaviours are all switched off; placed at a place, a thing faces its facing |
| `thing:launch(way, speed, rise?)` | sets it moving, standing or not (a dash, a wall jump, a knockback): the script's things, those switched off, and the figures of ready-made rules, which steer on from there |
| `thing:near(other, distance)`, `thing:ahead(distance)` | asking |
| `thing:enable(behaviour, on)` | a behaviour switched on or off for this one thing, until it leaves its level (the player: until the next level is entered) |
| `thing:remove()` | it goes at the tick's end (still readable until then); events that already happened this tick are still delivered, no new ones come |

### `input`, `controls`, `screen`, `music`, and `on_draw`'s `frame`

- `input`: the keys held this tick (`input.dir`, `input:held`,
  `input:pressed`, `input:released`), in `on_tick`; in `on_draw` the last
  tick's, only to look at. With several players each has its own
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
  keeps time with it (unheard nudges, a jump after a pause or a fade,
  which hold it). The game's song wins over the player's choice; music
  belongs to no kit. `music.intensity` (0..1, 1 unless set) is the
  game's tension: a song's layers (`#layer D C 0.35` in it; `compose.py`
  writes them with `adaptive = true`) come in above their threshold and
  go below it, each over a bar (Pulsar: the drums with a few enemies,
  the melody with many). Later: a tempo the game sets, stingers, sounds
  that play in the song's chord (`shots/reviews/music-plan.md`).
- `frame`, `on_draw`'s parameter: this picture alone (`frame:text`,
  `frame:cube`, `frame:model`, `frame:light`). An action in braces in a
  text is its key as the player sees it (`"PRESS {jump}"`, as
  `controls:key`), `{{` a brace.
  `frame:model` turns a model further (`turn`) and poses a figure of
  parts (`pose`: each part by its name, a turn about its joint): only the
  picture, decided anew each frame from the states, as a kind's `swing`
  (parts swinging with its steps) is decided from its body.
  `frame:model` of a file the game does not know (no kind's `model`, no
  `c.models`, item or effect names it) stops the game with a mistake.
- **A model's pivot** is the cell `size / 2` (whole voxels, rounded
  down) across and deep, on the floor: a thing's position is that cell,
  and a model turns about its middle. For an odd size that is the middle
  cell and the model fills the same cells at every turn. For an even size
  (4 across: cells -2, -1, 0, 1 from the pivot) the pivot lies one cell
  off the middle, so a turn that flips an axis (facing -y flips both
  ways, facing +x or -x one) fills the cells one voxel higher on it
  (-1 to 2): facing -y a 4 by 4 model lies one voxel up and to the right
  of facing +y. Free turns (`turns = "free"`, `turn`) land on the same
  cells. To keep an even model in one place whichever way it faces, give
  it an odd size, or place it by its middle and allow the voxel. A
  model's file (`.vvx`, `src/vvx.rs`) measures its frames' and parts'
  `at` and the joints from this cell, so growing a part moves nothing
  else and the model keeps standing where it stood.
- **A game's version**: `conf.lua` says which Voxcade its files are for,
  `c.voxcade = "0.6.0"` (read as that line; in neither the fingerprint
  nor the hash). A game for an older Voxcade, or without the line, does
  not open (`run`, `shot`, `check`, the IDE, the launcher) until `voxcade
  upgrade` has brought its files up to date (`src/upgrade.rs`: a step
  for each change of a format, 0.6.0's the models as `.vvx`); one for a
  newer Voxcade does not open either. Games given to the engine in tests
  as text are not asked.

## States

| State | Lives | Holds, among the game's own |
|---|---|---|
| `world.state` | the whole run | what the game knows across levels: quests, the story so far |
| `level.state` | one visit of one level; fresh each time it is entered, a restart too | crates moved, enemies left, time in the level, a sequence's steps |
| `level.kept` | the whole run, one per level | what this level keeps across visits that the game tracks itself |
| `player.state` | the whole run, from level to level | `lives`, `score`, `hp`, `inventory`, `checkpoint`, abilities |
| `thing.state` | as long as the thing lives | `hp`, a chest's or a trader's `inventory`, a piece's `light` (0 off to 1) |

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
open when the level is entered again. Two ways, both kept with the run
and its saves:

- **By kind:** `c.things.key = { keep = true, … }`. A placed thing of a
  kept kind that is removed stays removed in later visits and restarts;
  one that stays keeps its `thing.state` and where it stands (a chest
  stays open, a pushed crate where it was pushed). Matched by
  `thing.name`; a spawned thing is kept only if it was given a name.
- **By hand:** anything else in `level.kept` (a lever's position, a
  puzzle solved), read in `on_level_start`.

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
ready-made behaviours, their events to the script, `on_tick`, the tick's
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
    from its file again, fresh `level.state`, kept kinds kept; the player
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
| `level.state.held = true` | the ready-made behaviours, physics, touches, timers of the level; a dying thing's death show plays on | `on_tick`, `on_draw`, `input`; `world.tick` counts | dialogue, an inventory, a shop, a tally, a continue screen, a death animation, an RPG's battle over the map |
| `world:hitstop(ticks)` | everything, `on_tick` too, a second at most | the picture | the punch of a hit |

`thing:enable(behaviour, false)` stops one thing's rules (the player's
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
| `level.kept`, kept kinds | none | kept | kept | from the save | kept |
| `level.state` | fresh | fresh | fresh | from the save | fresh |
| the level's things and voxels | from its file | from its file | from its file | from its file and the save | from its file |
| holds, switches, hit stop, shake | none | none | none | the level's from the save | none |
| events pending for the next tick | none | dropped | dropped | dropped | dropped |
| the player | at the level's start | at the place | at the place, the checkpoint or the start | where it stood | where it stood |
| the camera | follows the player | follows the player | follows the player | follows the player | as it was |
| the music | the level's | the level's (going on if the same) | going on | the level's | going on |
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
| `on_player_join(p, how)`, `on_player_leave(p)` | a seat is taken or left during the run ("Several players": a design, not built yet) |

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
2. The ready-made behaviours run, kind by kind in their list's order
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
- **Settings per level** (its title, its music, the player's behaviour
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
  where it puts them, lights), each a thing of the kind `"piece"`, and
  names areas (`level.areas`). Its things may have names of the file's;
  a name is one thing's in the level, checked as the game loads, as a
  piece's model is (a missing file names the level and the line).
- **Replays** start at the first level (or at the save or `--level` they
  began from); every level's files count for their fingerprint, the
  pieces' models among them.

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

## Kinds, behaviours and families

- **Behaviours are a list per kind**, run in that order:
  `c.things.thug = { behaviours = { "brawler", "flash" }, … }`. They come
  from any of the engine's sets (jump & run, shooter, later a brawler's,
  an adventure's); `c.kit` goes. Each behaviour's numbers are the kind's,
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
  without a behaviour (and the same in a game with a kit); on a kind with
  a behaviour it is a mistake, for its ready-made rules move it.
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
  but `setup` optional:

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
- **In the players' hands too:** the console is there in every build, as
  F5 and F9 are (the user, 2026-10-29: whoever wants to cheat may). A run
  it helped (a line run in it, or begun from a scenario: `Session::helped`)
  goes into the high score table as GOD, no initials asked: the table
  says so. Switching the console on from the command line only stays an
  idea.

## Several players

Status: designed 2026-10-29 (stage 1 of `docs/multiplayer.md`; written
against five games below, attacked by Fable and Grok,
`shots/reviews/fable-players.md`, `grok-players.md`, and revised), built
2026-10-30 for games without ready-made rules (stage 2; the kits in
stage 3, the network in stage 4). Two to four players at one screen as at an arcade machine (no split
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
  behaviour off; `on_tick` walks the player to the castle, counts the time
  into `player.state.score`, then `world:go_to(level.next)` or, after the
  last, `world:win()`. Checkpoints set `player.state.checkpoint`;
  `on_death(player)`: hold the level, a counter, then
  `world:restart(player.state.checkpoint)` or `world:lose()` with no lives
  left.
- **A jump & run RPG** (Voxkeep grown): zones are levels, the overworld
  top-down (the player's behaviour set in that level's file), dungeons
  jump & run. Doors carry `to` and `at`; `on_touch` sends the player
  through. The hero has `player.state.hp`, an inventory (keys, potions)
  and abilities (`player.state.double_jump`); a locked door checks
  `player.state.inventory.key`. Chests are a kept kind: opened stays
  opened. A save point calls `world:save(1)`; dialogue holds the level.
- **Zak McKracken meets Double Dragon:** rooms are levels; items in the
  inventory; `on_interact` for talking and opening, `on_use(user, item,
  target)` for "use key on door" (later; with a pointer for
  point-and-click); thugs `behaviours = { "brawler" }` in the family
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
| "Taken stays taken" is every game's chore, built by hand from ids | GameMaker, Godot, RPG Maker (self switches), GB Studio | `thing.name` from the editor, kept kinds, `level.kept` |
| Levels that only ever reset are not enough; a whole persistent level is too much | Construct (Persist), AGS (rooms keep state), GameMaker (its own tutorial drops persistent rooms) | fresh `level.state` by default, kept kinds and `level.kept` by choice |
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
| Overlays as a scene stack rebuild or lose the world behind them | GB Studio (the stack reloads the scene), Hawkthorne (enter runs again), RPG Maker (works only because its state lives apart) | no stack: the engine's pause, a level hold, behaviour switches |
| A hold that stops the world but not the overlay's code is the common need | Godot (process modes), HaxeFlixel (sub-states), GB Studio (pause logic), AGS (`PauseGame`), Unity (Chop Chop's states) | `level.state.held` |
| Time scales and timers that run on under a pause, or a pause that leaks into the next scene | Unity (`timeScale`), Godot (timers), HaxeFlixel (global tweens), Construct (the player can still fire) | the hold stops the rules, not the script; it belongs to the visit |
| A key held across a level change counted as a new press loops restarts | HaxeFlixel (issue 2270) | held keys stay held, no new press |
| Blocking scripts cannot be saved or replayed | AGS, GB Studio, RPG Maker | sequences as data in states |
| `on_death` fired when a level unloads | Construct (`isEndingLayout`) | only for deaths, once |
| Saves as memory dumps or project hashes break on every update | GB Studio, AGS, GameMaker (`game_save` deprecated), RPG Maker | saves are states by name, a version, defaults and `on_save_loaded` |
| Mixable behaviours per object type, in list order, switchable per instance; families | Construct | behaviour lists, `thing:enable`, families |
| A player controller per scene is a proven cross | GB Studio | the player's behaviour per level, in the level file |
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

## From version 1

| Version 1 | Version 2 |
|---|---|
| `function load(world)` | `function on_world_start()`; a level built in code in `on_level_start(how)` |
| `function tick(world, input)` | `function on_tick()` |
| `function draw(world, out)` | `function on_draw(frame)` |
| `function goal_open(world)` | `on_touch` and the game's own state; no goal event |
| `world.state` | `world.state` |
| `world:fill`, `world:voxel`, `world:set_voxel`, `world:ground`, `world.size` | `level:…` |
| `world:spawn(kind, x, y, z)`, `world:thing`, `world:things`, `world:count` | `level:spawn(kind, x, y, { z = … })`, `level:…` |
| `world:remove(thing)` | `thing:remove()`, at the tick's end |
| `world:explode`, `world:sound` | `level:…` |
| `world:shake`, `world:flash` | `screen:shake`, `screen:flash` |
| `world:hitstop` | unchanged |
| `world:focus`, `world:look_at` | `level.camera:follow`, `level.camera:look_at` |
| `world.score` | `player.state.score` |
| `world:win`, `world:lose`, `world.tick` | unchanged (they wait for the tick's end) |
| `thing.data` | `thing.state` |
| `thing.hp` | `thing.state.hp` |
| `thing.look` | `thing.facing` |
| `out:…` | `frame:…`; `out:key`, `out.device`: `controls:key`, `controls.device` (everywhere), or the action in braces in `frame:text` |
| `c.kit = "platformer"`, `behaviour = "walker"` | `behaviours = { "walker" }` (the kit implied by the behaviours) |

The ready-made rules' score, lives and hit points (the platformer's
hearts) came into `player.state` and `thing.state` in stage 2 without
changing what the replays' checks hash: the engine keeps them where it
did and reads and writes them through. What moves for real later (the
weapons, the inventory, the ready-made rules' own deaths and respawns
handed to `on_death`) changes the hash: that stage records the checks
anew (`voxcade check --update`, the same input), and says so.

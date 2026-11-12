---
name: voxcade-api
description: >
  Grow or change Voxcade's Lua API (the engine side: src/lua/, the
  register src/lua/api.rs, the ready-made rules' hooks, levels, states,
  events, items, save games) by the game model in docs/game-model.md.
  Use this before adding or renaming any Lua name, field, method,
  handler or conf.lua setting, before giving the scripts a new hook, when
  a game needs something the API lacks, when levels, the player, an
  inventory, save games or events are built, and when the user asks
  whether the API can carry a kind of game (ein Genre, ein Crossover).
  Writing a game with the API as it is: voxcade-lua-game instead.
---

# Growing Voxcade's Lua API

The model is `docs/game-model.md`; read it first. This skill is the
checklist that keeps the API from growing short-sighted again: the user
asked for a simple but generic system that carries every genre and every
cross (a jump & run RPG, Zak McKracken meets Double Dragon), so that it
need not break later. A name added carelessly is a name every game carries
forever.

## Before adding a name

1. **Whose job is it?** `world` is the whole run, `level` the level
   being played, `player` the one who plays, a thing what acts, `input`
   what is held, `controls` what the player plays with, `screen` the
   picture's effects, `on_draw`'s `frame` what is drawn. Whatever comes
   from the player's machine is input: a replay must carry it, or the
   game must not see it. A name goes to the object whose
   job it is. If none fits, the model is missing something: say so to the
   user before inventing an object.
2. **Does it change while playing?** Then it lives in a state:
   `world.state` (the run), `level.state` (this visit of the level),
   `player.state` (the run, from level to level), `thing.state` (the
   thing's life). The engine's own changing numbers too (lives, score,
   hit points, inventory): fixed keys in those states, not fields of
   their own. Static things (kind, props, places, definitions) are
   read-only properties. Save games, snapshots, replay checks and
   `inspect` see the states; something changing outside them is lost by
   all four. A state that outlives a thing (`world.state`,
   `player.state`, `level.kept`, a save) holds its name, never the thing.
   Its default is data in `conf.lua` (`c.state`, a kind's `state`).
3. **Is it an event?** A handler named `on_` and what happened
   (`on_touch`, `on_level_end`), called by the engine; its parameters are
   only what the event is about. The script decides with its states.
   Say exactly when it fires (`on_touch` once when contact begins, not
   every tick; `on_death` never because a level was left) and where it
   sits in the tick's written order. Nothing blocks or waits inside a
   handler: a sequence is steps in a state.
4. **Is it game-shaped?** A hook that only one game or one set of
   ready-made rules needs (as `goal_open` was) is wrong. Find the generic
   event or state it is a case of. There is no goal, no "level complete":
   a goal is a thing the script answers in `on_touch` (none of eleven
   engines studied has one).
5. **The cross test.** Would it work in a game that mixes genres (a
   jump & run RPG, an adventure with fights, a puzzle game with levels made
   in code)? If it only works for one kit, it is not generic yet.
6. **When does it happen?** Anything a script asks for that changes the
   game's course (a level change, a restart, a save, a load, a removal,
   the end) takes effect at a fixed point of the tick (its end), never in
   the middle: replays rely on it. The handler runs on after asking; one
   ask of a tick counts (going on before starting over); say which
   handlers may ask (the table "What each handler may do"). Fades and the
   engine's pause run no ticks. "The flow of a run" in
   `docs/game-model.md` is the whole path; a new name fits into it or
   changes it there first.
7. **Linked by name and checked.** Levels, places, items, kinds, families
   and placed things are linked by name (never numbers or coordinates),
   and every link is checked when the game loads: a wrong name is a
   mistake with its place.
8. **Will old saves still load?** A new state key gets a default; a
   renamed one needs `on_save_loaded`; a save never holds code or engine
   internals, only states by name.

## When adding it

- **The register first:** `src/lua/api.rs` (the one source; test
  `the_register_is_what_the_scripts_see`), then `voxcade api --lua
  lua/voxcade.lua` (test `the_editors_definitions_are_up_to_date`).
- **Plain English** in its doc line, for people who do not know the
  engine: no "kit", no "behavior" without saying what it means (the
  user's rule; the IDE shows these lines in HELP and on hover).
- **A HELP example** when it is something a game typically does
  (`src/ide/help.rs`): a whole `main.lua` to copy, run as a game by
  `every_example_runs`.
- **Determinism:** state goes into the hash (`LuaGame::state_hash`),
  snapshots (`LuaSnapshot`) and the replay guard; a new kind of state is
  mixed into the hash only when it is used, so games without it keep
  their replays.
- **One API at a time:** a rename moves every game in `games/`, the
  templates in `templates/` and the HELP examples in the same commit.
  Names only: `voxcade check <game> --refit`. What the checks hash
  changed: `--update`, and say so to the user.
- **Docs:** `docs/game-model.md` (the model) and the voxcade-lua-game
  skill (how games use it) follow the change.

## Words

The user's words, and the API's: world (the whole game), level (a place
to play, an RPG's zone too), state (what changes), props (what the level
editor set on one thing), place (a named spot in a level), item, inventory,
ready-made rules (behaviors, a list per kind; families group kinds),
kept (a kind or a level's memory across visits). Handlers say what
happened: `on_world_start`, `on_world_end(result)`,
`on_level_start(how)`, `on_level_end(why)`, `on_tick`, `on_draw`,
`on_touch`, `on_touch_end`, `on_interact`, `on_hit`, `on_death`, `on_fall_out`,
`on_pickup`, `on_save_loaded`, `on_beat`, `on_player_join(p, how)`,
`on_player_leave(p)`. Objects: `world`, `level`, `player`,
a thing, `input`, `screen`, `controls`, `music` (the game's own song and
its clock). Several players (designed, not built): seats
(`world.players[n]`, `false` where free; `p.seat` tells a player from a
thing; `player` is seat 1 only), out of play (`p.state.out`), a team
(`p.state.team`), a figure's kind (`p.state.kind`), `level:players()`
(those in play), `frame.players` (those at the machine drawing). A new
name for players asks the cross test with two or four: does it work for
seat 3, over the network, in a replay? Asks: `world:go_to`, `world:restart`,
`world:win`, `world:lose`, `world:save`, `world:load`. Holds: the
engine's pause, `level.state.held`, `level:hitstop`. Scenario (a run
begun late, set up in code: `scenarios/NAME.lua`) and console (a line
of Lua between two ticks): every game has both, and a new name is tried
through them.

What eleven other engines and the classic games taught (AGS, GameMaker,
RPG Maker MZ, Construct 3, Godot 4, GB Studio, Unity, Phaser 3,
HaxeFlixel, PICO-8, LÖVE; Mario, Celeste, Shovel Knight …) is the table
at the end of `docs/game-model.md`, the reports behind it in
`shots/research/` (git-ignored: this machine only): check a new idea
against them before building it. Write the kinds of game it is for
against the design first (`shots/research/crosstest.lua` is how), and
let other models attack it (Fable, Grok) before it goes to the user.

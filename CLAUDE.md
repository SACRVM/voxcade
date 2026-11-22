# Voxcade

Voxel arcade engine in Rust (wgpu + SDL3). Read `docs/concept.md` for the
design decisions and `docs/engine.md` for the engine's architecture and plan
(every classic 2D genre in voxels, games in Lua or Rust, an IDE for Lua
games in phase 2: `docs/ide.md`) before changing anything architectural.
The Lua API grows by the game model in `docs/game-model.md` (world,
level, player, things; everything that changes in a state; `on_` events;
levels, items, save games; the voxcade-api skill): read it before adding
any name to it.

Build what the user asks for. Effects, features or behavior they did not ask
for are suggestions to make in the reply, not changes to ship (an unasked
x-ray silhouette was removed again).

Voxcade is free under the zlib licence, as LÖVE (the user, 2026-11-10;
`LICENSE.txt`, and `NOTICE.txt` for what it covers: the program, the
games that come with it and the templates; games made with it are their makers'). Money, if any, comes
from donations and from services for commercial games, never from the
engine. The source is public since 2026-11-10 (SACRVM/voxcade, its history
begun anew; the earlier one is the private SACRVM/voxcade-archive, the
remote `archive`, never pushed to the public one), and its Builds
workflow makes the releases the website offers.

Pull requests from others are welcome (the user, 2026-11-10; the
repository is public, `CONTRIBUTING.md` says how). The author's own work
still goes straight to `main`.

## Layout

Voxcade is a library (`src/lib.rs`) with one game API (`docs/engine.md`,
"The game API"): `src/game.rs` the `Game` trait, `src/session/`
`Session<G>` (a loaded game folder, its title, music, hot reload, replays'
fingerprint, the frame, `snapshot`/`restore`; `mod.rs` `Session`, `Tick`,
`Event`, `Snapshot`, the fingerprint's parts, `load.rs` `load`, `fingerprint`,
`reload*`, `apply`, `play.rs` `step`, `play`, `snapshot`, `restore`, seats,
saves, scenarios, `eval`, `sound.rs` `feed_music`, `play_sounds`, `view.rs`
`sun`, `face_mask`, `view_target`, `PictureCamera`, `Viewing`, `Daylight`,
`compose.rs` `compose`, `menu_frame`, `still`, `look`, `drawing.rs` `Drawing`,
`Voxels`, `figures.rs` `part_chain`, `model_drawn`, `swinging`, `rest_turn`,
`part_at_rest`, `models.rs` `Models`; paths like `crate::session::Drawing`
stay), `src/sim/` the simulation's
core every game shares (`Sim`; `sim::things`: things in one list with
ids, kinds from `c.things.NAME` with `like`, parts), `src/cli/` the
command line any game gets (`table.rs` its one table of commands and
options, the help written from it; `parse.rs` the parsers, which take
nothing else; `mod.rs` each command's work; tests
`the_help_says_every_option_the_parsers_take`,
`the_parsers_take_every_option_the_help_says`). `src/lua/` is the Lua door (`LuaGame<K>`: the
sandbox, the world API over `lua::host::Host`, `conf.lua`, errors that
halt; `api.rs` the API's register; `mod.rs` the game and `impl Game`, its
own work by concern in `start.rs`, `calls.rs`, `course.rs`, `events.rs`,
`seats.rs`, `saving.rs`, `hashing.rs`; `memory.rs` levels that
remember), `src/mcp.rs` the MCP server.
`src/vvx.rs` is the model format, `.vvx` (text: the format line, `size`,
a `[palette]` of signs, `[[frame]]`s or `[[part]]`s with `layers`);
`src/vox/` the models as the engine holds them (`VoxModel`, `Part`) and
MagicaVoxel's `.vox` (`magica.rs`), only read (`import_model`, an older game's worlds
for the upgrade; never written); `src/scene.rs` a world or a map's prefab
as a `.vvx` (`Scene`: its colours the world's numbers in the palette's
order, markers as palette entries naming a kind); `src/upgrade/` brings a game made for an older
Voxcade up to date (its steps, `c.voxcade`, `opens`, `standing` for the
IDE's upgrade page, `src/ide/wizard.rs`).
`src/files.rs` is a game's files as the engine reads them (`Files`: the
folder, or bytes laid over it; `Store` by BLAKE3; `docs/multiplayer.md`
stage 6): every loader reads through the session's `files`, never the
disk itself, so a game can play sealed files while its folder changes.
`src/net/` is network play in lockstep on a LAN (`docs/multiplayer.md`,
stage 4): `Host` and `Guest` state machines without I/O over `wire`'s
datagrams, `SimNet` a simulated network for the tests, `soak` its stub
game, `play` a session played by bundles (`Hosting`, `Joining`,
`Locals`), `udp` the socket, `machine` what a machine is to the others (its id, name, build; `machine::random` the OS random source, `getrandom`; `docs/multiplayer.md` has the trust model), `quic` iroh across the internet (a thread
of its own, channels; stage 6), `link` either as one transport (`Where`,
machines asking in let in by a spoken code), `trial` iroh's measures
(`voxcade nettest`; the four signs both screens show to let a machine in
are `link::code`; the game codes' words, six signs for a ticket, come
from the `matchmaker-protocol` crate); `src/network.rs` the window's
side of it. Our relay and the game codes are `matchmaker`, a project of
its own beside this one (`../matchmaker`, to be public as
SACRVM/matchmaker, the user's: "mach ein extra Projekt draus"), run on
our server, a TLS proxy in front.
`src/shooter/` is a shooter kit, building blocks for many shooters (More
Blood, its first game, was a prototype): `Shooter`, its behaviors
soldier, spawner, captive, gunship, pickup as systems over its `Brain`s;
the kinds and weapons as rules (`kinds.rs`; `kit.toml` a library of
kinds and weapons to copy, merged into no game); drawing; `script.rs`
the kit as Lua games use it. What a kill, a pickup, a checkpoint, a
life, the HUD and the helipad mean is each game's `main.lua` (the kits'
rule: test `the_kits_keep_no_game_in_them`). `src/main.rs` is the `voxcade` program, `cli::main::<Voxcade>`
(`lib.rs`: `Voxcade = LuaGame<Either<Shooter, Platformer>>`, the kit
`c.kit` names): every game folder is a Lua game,
More Blood, the Arena and the Bench with `c.kit = "shooter"`,
`games/sokovox` (Sokoban in voxels) and `games/pulsar` (a space shooter
whose waves come on the music's bars, `music:play`, `on_beat`) in Lua
alone, `games/voxkeep` (a jump & run in a castle at night) levels of
voxel blocks (`src/blocks.rs`, `src/grid/`; `src/tiled.rs` brings a
Tiled map in once; `src/editor/` the editor, `src/window.rs` the window
it shares with the game) with `c.kit = "platformer"` (`src/platformer/`,
a jump & run kit like the
shooter's: platform, crate, hero (then the crates it pushes), walker,
hopper, flyer as systems over its `Brain`s in that order, pickups and props standing (any figure's pose
the game sets, `thing.state.pose`), its kinds to copy in its `kit.toml`,
tests in `tests.rs`; it moves things, what a touch means is the game's
`main.lua`, the platformer template's has the classic rules written
out). `examples/rust_digger/` is a game in Rust on
the engine alone (hills made in code, `Game::level`). The engine names no
shooter type outside `lib.rs` and `main.rs`: whatever a game needs from
it goes through the `Game` trait (a kit's through `lua::host::Kit`), the
shooter's things stay in `src/shooter/`.

## Commands

- Build: `cargo build --release`. In the main checkout build with
  `CARGO_TARGET_DIR=target-work cargo build --release` and run
  `./target-work/release/voxcade` (the same for `test` and `clippy`):
  never rebuild `target/release` while the owner's IDE runs from it (it
  crashed once); the binary is handed over to `target/release` by copy
  and rename after a commit that is verified. An agent in its own
  worktree uses that worktree's own `target/`.
- Play: `cargo run --release -- run games/arena` (opens a window; the user
  plays it, you don't). F5 (play again on the files as they are), F9
  (keep the run) and 1 to 9 (the game's songs) are the maker's keys:
  `run` has them only with `--maker`, a player of a packaged game never
  (the owner's D4, 2026-11-13), the IDE's PLAY always, `voxcade replay`
  keeps F5. Nothing of `run`'s own is meant to be tried here by an agent
  without `--shot`.
- See a frame: `cargo run --release -- shot games/arena --frames N --input "keys:ticks;..." --out shots/<name>.png`,
  then Read the PNG. Keys: the directions `left right up down`, the game's actions
  (More Blood: `jump fire grenade strafe`) and `none`, joined with `+`.
- Regenerate a game's level and models: `python3 tools/gen_arena.py`,
  `python3 tools/gen_moreblood.py OUT` (numpy; writes More Blood's map.toml,
  ground PNGs, prefabs and actors into OUT: the game's are the user's)
- Play More Blood: `cargo run --release -- run games/moreblood`; Sokovox:
  `-- run games/sokovox` (its models: `python3 tools/gen_sokovox.py`; a
  `shot --input "next:1;none:1;..."` reaches any level); Voxkeep (a
  castle at night, six rooms in
  `levels/`, a demo of going from level to level and of the jump & run
  kit's puzzles: bats to time, ferries over spikes, a lever and a lift,
  slabs out of the floor, a crate on a plate holding a hatch open; the
  door opens once every gem is taken and leads on; a lever's and a
  plate's `props = { works = [...] }` in the level say what each works,
  checked as the game loads by `names = { works = "things" }`; its replay `all_six_rooms` plays through): `-- run games/voxkeep` (blocks,
  figures and the rooms first made by `python3 tools/gen_voxkeep.py`,
  which now only makes what is
  missing: the files there are the user's, shaped in the editors; never
  regenerate over them); Pulsar (a space shooter scrolling up,
  in Lua alone): `-- run games/pulsar` (models, stars, asteroids and
  stations from `python3 tools/gen_pulsar.py`; its song from
  `music/pulsar_drive.compose.toml`; keys the arrows, fire, bomb; two
can fly, `c.players = 2`: a second player joins with START on a pad or
Return at the keyboard, its ship `ship2.vvx` in gold and green, each
ship its shield, lives, bombs and score, the enemies aiming at the
nearest; its replay `tests/two_ships`);
  BunnyFarm (a farming game, `docs/bunnyfarm.md`, built in stages; the
  bunny sows, harvests, sells, cooks, fills orders, keeps animals, eats
  and sleeps through days of 6:00 to 2:00; in Lua alone over several
  files, `main.lua` requiring `data`, `clock`, `calendar` (the week's
  events), `farm`, `market`, `kitchen`, `mail`, `barn` (the pens, the
  animals, the dog and the fox), `wolf` (gaps in the pens' back fence,
  mended with wood from the stall in three blows of a hammer, the bunny's
  frames 6 to 8, as its can pouring 9 and 10 and its spade 11 and 12,
  chosen in `main.lua`'s `bunny_frame` from the bed in front; the shy wolf at night, a pen frightened for a day and
  an animal run off, fetched round the fence through the farm's gate and
  following the bunny home), `year` (the farm year: its reckoning,
  the collection, the score; a year's end saves the farm and wins the
  run, CONTINUE goes on), `bunny`, `hud`, `fx`, `text` and `lang/`): `-- run
  games/bunnyfarm` (blocks, farm, figures first made by `python3
  tools/gen_bunnyfarm.py`, which only makes what is missing; keys the
  arrows, act, back, prev/next the seed, bag; its replay
  `a_day_on_the_farm` sows, harvests to level 2, trades and finds the
  burrow will not have it before 20:00, `to_bed_from_eight` sleeps then,
  `fertilizer_doubles_the_harvest` the stage 2 tools,
  `the_mailbox_pays_for_an_order` stage 3's gift and orders,
  `rain_grows_the_crops_faster` and `the_full_moon_gives_moonberries`
  stage 4's calendar, `the_barn_gives_an_egg` stage 5's barn (it begins
  from a save: a replay's `from`, a level-3 farm written from `inspect`'s
  states), `the_year_ends_with_its_reckoning` stage 6's year end (from a
  save on the year's last night), `the_can_is_filled_at_the_pond` (the
  can holds water for four beds, filled at the pond), `harvest_all_at_the_scarecrow` the
  prototype's harvest all (from a save with six ripe beds; `fx.lua` the
  prototype's small signs: what a bed gave rising from it, coins and
  goods flying to the HUD, hearts over a mother),
  `the_wolf_comes_through_a_gap` and `wood_mends_the_fence` the wolf
  (from an evening with two hens and a gap),
  `the_dog_sells_a_chick_and_a_hen_is_bred` (a stable built too) and
  `the_fox_buys_a_grown_hen` the barn's merchants,
  `the_wolf_comes_in_the_night` and `a_gap_is_mended_with_wood`, these
  four begun from scenarios; its scenarios (`scenarios/`: the fox, the
  dog, the wolf's night, a fence to mend, a hen run off, harvest time,
  the year's end, a full farm at level 9) speak `setup.lua`'s words (a
  day and an hour, the level, goods, crops, animals, a stable, a gap, the
  bunny at a place), which keep the states as the game has them; older saves and a
  farm running while its code changed are brought up to date in
  `main.lua`'s `bring_up_to_date`, never left to crash)
- A game's kinds and weapons with every number in effect (the defaults it
  leaves unset too): `voxcade kinds <game>`.
- A model as the game draws it (its light, camera angle, shading), alone:
  `voxcade sheet <game> <model>` writes a sheet (five sides close up,
  every frame from the side, the five sides at the game's size;
  `Session::still`). Making figures: the voxcade-models skill.
- The Lua API: `voxcade api [NAME]` lists it, then every `conf.lua`
  setting (type, default, range), what a script has of Lua's own library
  (`api::LUA`: every `math` name, test
  `the_reference_lists_the_maths_a_script_has`) and every key of a kind (the engine's,
  each kit's behaviors' from its `kit.toml`, `api::KIT_WORDS`: the words of
  every kit key, each a sentence that stands alone), the shooter kit's
  `c.weapons` and keys of `c.player` (read from the structs that read them,
  `api::kit_settings`, test `the_shooters_settings_are_what_it_reads`), the
  types the API names (`voxcade api Light`, from `api::TYPES`), and at the
  top what a mistake is, the conventions and the order of a tick
  (`api::MISTAKES`, `CONVENTIONS`, `A_TICK`, which HELP shows too); NAME as printed (`thing:walk`, `world.state`,
  `c.camera.fov`), in part, any case; an unknown one exits 1 naming the
  nearest. A number of `conf.lua` has its range in `config::RANGES` (or
  is one of `config::FREE`), checked as `conf.lua` is read (test
  `every_number_of_conf_lua_has_its_range`). After changing the API,
  `voxcade api --lua lua/voxcade.lua` writes the editors' definitions
  anew (a test fails until then). `voxcade inspect <game> --frames N
  --input ...` plays without a window and prints the game's state as JSON
  (things with their states, `world.state`, `level.state`,
  `player.state`, the error). Scenarios and the console (every game's,
  the rule below): `voxcade scenarios <game>` lists a game's;
  `run`, `shot` and `inspect` begin from one with `--scenario NAME` and
  run lines of Lua first with `--do LINE`; `shot --screen console` shows
  the console. `voxcade mcp` serves shot, check,
  run, things, scenarios and api to an agent over MCP on stdio (shot,
  run and things with scenario and do; a tool's game is the
  folder it runs in unless named), and the IDE's tools (view_state, open,
  game_frame, errors, play, stop, eval: a line of Lua in the game played
  there) passed on to the IDE running on the game
  (`ide/bridge.rs`: 127.0.0.1, a port and a token in
  `config_dir()/ide/<folder>.lock`, the user's alone). The IDE's MCP button
  writes the game folder's `.mcp.json` when the user presses it; else the
  user adds it to Claude Code themselves (`claude mcp add voxcade --
  /path/to/voxcade mcp`), never edit their config.
- Trying the IDE's agent from this session (the user's idea, 2026-11-11:
  the games we make with the IDE live in `~/Projects/repos/voxcade-games/`,
  the IDE's projects folder here, each with its own agent, out of this
  session's way): `voxcade ide <game> --remote` answers two more tools,
  `console_send` (a prompt typed into the console, for the agent there)
  and `console_read` (what the console shows), listed only by `voxcade
  mcp --remote`. `python3 tools/ide_call.py <game> TOOL ['{json}']` calls
  any of the IDE's tools: `play`, `console_send '{"text": "..."}'`,
  `console_read` until the agent is done, `game_frame`. Typing into the
  console is running commands in the user's shell, past the agent's
  prompts: the game's own agent never gets these tools (its
  `--mcp-config` is `voxcade mcp` without the flag, and an IDE started
  without it refuses). Start the IDE for it with the user's own shell
  and without this session's marks, or the console shows another shell
  and Claude Code keeps no transcript: `env -u CLAUDE_CODE_CHILD_SESSION
  SHELL=/bin/fish voxcade ide <game> --remote` (a window on the user's
  desktop: say so). The same channel gives the IDE a person's hands
  (`ide/remote.rs`): `windows` (ids, titles, sizes), `window_shot` (a
  PNG of any window, the main one or an editor's, as drawn now, after a
  tooltip's delay if the pointer rests), `pointer` (move, click,
  double_click, press, release, drag; buttons, modifiers), `scroll` and
  `keys` (a chord, or `text`; `hold_ms` holds it for the game too). The
  input is real SDL events pushed into SDL's queue with the window's id
  (mouse id `SYNTHETIC`, so a real mouse takes the arrow marking the
  synthetic pointer away); x and y are pixels in that window's picture.
  Click the game once for it to have the keys, as a person would.
- Models from MagicaVoxel: `voxcade import <game> FILE.vox [--as
  NAME.vvx]` writes each as a `.vvx` in the game, leaving the source; a
  game made for an older Voxcade: `voxcade upgrade <game> [--dry]` (every
  step it needs, then `c.voxcade`; it commits nothing; a game not in git
  with everything committed has its files kept first in
  `config_dir()/before-upgrade/<game>-for-<version>/`, `upgrade::committed`:
  a folder git ignores is not committed, test
  `a_game_git_ignores_is_kept_before_the_upgrade`; `upgrade_keeping`). The IDE (and the
  launcher's EDIT and PLAY) opens such a game on its upgrade page
  (`ide/wizard.rs`, the user's: a wizard, not a command): the version it
  was made for and this one, the steps' dry run in their words (only the
  steps that change something, the rest one quiet line "N steps have
  nothing to change", `wizard::shown_lines`), a word on changes git has
  not committed and A COPY IS KEPT FIRST or GIT KEEPS THE OLD FILES by
  UPGRADE (`wizard::keeping`, as `upgrade::committed`), UPGRADE on a
  thread, what was done (COPY; not the line naming `voxcade check
  --refit`, which REFIT THE REPLAYS below does), "N replays recorded on
  the old files" and REFIT THE REPLAYS (`replay::refit_saying`), then the game; a
  game of a newer Voxcade is told to get the update. A `conf.lua` stating
  no `c.voxcade` is taken as made before 0.6.0, but where no step finds
  anything to change it opens (`Standing::Current`), the missing line said
  plainly (`upgrade::unstated`; `voxcade upgrade` then writes only the
  line); else `opens` names the first thing to change. `voxcade ide <game>
  --shot FILE [--upgraded]` draws the page, as it opens or as after an
  UPGRADE run dry (nothing changed).
- Levels of voxel blocks (`docs/tiles.md`): `voxcade import <game>
  <tiled-map.json>` brings a Tiled map in once (Tiled draws in 2D: it is
  only a way in); `voxcade edit <game>` opens the editor in a window (the
  user's), `--shot FILE [--block NAME | --things] [--view YAW,PITCH]`
  draws it without one, to look at. Every scope of it (a level of
  blocks, its pieces, a block, a model, a figure) has the same three
  zones (`src/editor/zones.rs`, the shared parts; `stage.rs` the pieces',
  `shaping.rs` the voxel editor's and the level of blocks'; the owner,
  2026-11-12: no stacked column, no sentences, never a scroll inside a
  scroll, one way of working in both editors): a top bar, the outliner
  left, the inspector right (a grid of short rows); in the view handles
  on the chosen one (`src/editor/handles.rs`, knowing nothing of levels:
  arrows X red, Y green, Z blue, rings, an area's faces), buttons beside
  it, a right-click menu whose entries are each also a visible control
  (`stage::menu_entries`). A piece turns by `rotation`, `pitch` and `roll`
  (`docs/game-model.md`).
- The IDE (`src/ide/`, `docs/ide.md`; phase 2, built in stages; `ide/shell.rs`
  is the shell's state, frame, open/flush/tick, the rest in `src/ide/shell/`:
  `bar`, `list`, `new_files` (`make_file`), `main_area`, `sound_ui`,
  `finding`, `game`, `errors` (`game_error`), `below`, `agent`, `view_state`,
  `manage`, `tests`):
  `voxcade ide <game>` opens it in a window (the user's), `--shot FILE
  [--open PATH] [--console CMD] [--size WxH]` draws it without one
  (`--console`: a command run in the console and waited for). egui 0.37 on the
  engine's window and GPU (`src/gui/`, shared with the editors: `input.rs`
  the glue from SDL3, `fonts.rs`, the style, the painter, the list row), one
  monospace typeface in two sizes (the user's choice: the retro feel),
  calm colours. On the left the logic and each sort of asset under a
  heading that folds (arrow on the right, one level only), the whole row
  clicked, its rows in the order of what they show (case ignored, numbers
  as numbers, a folder not counting; the levels as the game plays them;
  `Files::order`, `by_label`; the level a run begins in says "start" at its row's end, the game's own title "title", from `conf.lua` as it is saved, `list::Starts`); a FILTER field above the headings
  (Ctrl+P, the owner: "sortieren und ne suchfunktion"; names only, SEARCH
  does the files' insides; a heading counts its matches and the others
  hide, folds neither read nor written meanwhile, NOTHING FOUND, Enter
  opens the first row, Escape or the ✕ clears; rows worked out only when
  text or files change, `list::Filter`; `--shot --filter TEXT`); in the main area the one file open (no tabs). What was open
  and how far scrolled, the cursor in each text, live in the config
  folder (`ide/<key>.toml`, `kept::Identity`).
  Below the main area the console: the user's shell in the game folder
  (`ide/terminal.rs` over `alacritty_terminal`, `ide/console.rs` the
  panel), CLAUDE looks for Claude Code's program first (`claude::Finder`; missing:
  nothing is typed, the console says what an agent is and where to get
  Claude Code, LOOK AGAIN; started with the IDE the console stays folded
  and the bar says NO AGENT FOUND), then starts it as the IDE's settings say (the gear
  at the top right, `ide/prefs.rs`, `config_dir()/ide/settings/ide.toml`:
  the console shown or not at all, the agent, and `ide/claude.rs`: with
  the IDE, going on with each game's conversation by its session id,
  seeing the IDE through `--mcp-config`; NEW CONVERSATION; told its part,
  the game in its folder, `claude::brief`: inside the engine's source
  without the engine's CLAUDE.md and engine skills, with its own memory);
  a program that asks for the mouse (Claude Code's full
  screen) hears it, Shift held it is the console's; signs Hack lacks from
  the system's fonts (`gui/fonts.rs`). A path in the console under
  the mouse is underlined and a click opens it in the main area (the
  user's wish: nobody leaves the IDE to see what the agent made): a file
  of the game at its line (`main.lua:12`), a picture from anywhere,
  shown only (`console::word_at`, `resolve`,
  `Shell::open_from_console`); the agent's brief tells it to name such
  paths on a line of their own.
  Editors open in windows of their own (the user's choice), not in the
  main area, on the IDE's GPU (`window::Gpu`): a level and its blocks
  the level editor (at the block; every level in `levels/` listed under
  LEVELS, each in a window of its own; `voxcade edit --level FILE`),
  other models (`.vvx`) the voxel editor (`editor::frames::ModelFile`:
  what a `.vvx` holds, read and written through `vvx::read_shape` and
  `vvx::write_shape`: the palette, each colour with its sign and
  material, the frames as boxes of voxels as big as the model,
  `blocks::Block` of any size, or a figure's parts each in its own box
  placed from the pivot, parents and joints by hand; saved unchanged the
  same bytes, an edit changes only the lines it must: the signs stay, a
  new colour takes the next free one, a colour no voxel has keeps its
  sign until a right click on it takes it out; the box grows and trims
  about the pivot, so the model stands where it stood; the held colour's
  material, GLASS, METAL, ROUGH, GLOW, under a quiet word beside the
  colour, `frames::with_word`; a MagicaVoxel `.vox` is not shaped: the
  editor says to bring it in, nothing writes `.vox`; `voxcade edit <game>
  --model FILE`; in a
  block or a model BOX and SPHERE besides the voxel tools, the user's
  idea so people build as the agents' scripts do: a footprint dragged on
  a face's plane is set right where it was drawn as the mouse lets go
  (Alt: only where empty, Ctrl: cut; its depth the last one used, 1 at
  first; the owner: a box stuck to the mouse is not what anyone expects),
  and stays fresh, outlined, until the next action: + and - its depth in
  place, BEVEL its edges, still the one step UNDO takes away whole
  (`editor::model::Fresh`); no stamping it again;
  Shift held is PICK in every tool (the owner: PICK while Shift is held, the
  tool before back when it is let go; `Editor::shift_pick`, the window
  losing focus lets go; `--shot --shift`);
  BEVEL cuts a box's edges, SYMMETRY mirrors every tool left to right;
  `editor::model::form_cells`, `--shot --tool box --held X,Y,Z` sets one
  that big under the mouse, fresh, `--stamp` no more fresh); among a
  level's blocks BOX (of the block held, as it is turned; the level grows
  on its right, back and top to hold it, `Editor::place_level_cells`) and
  FILL (the cells of the block clicked joined face to face take the held
  one, Ctrl all of it, `fill_cells`) mean the same, same keys and words
  (the owner: one set of tools in both editors); LEVEL's SIZE of a level
  of blocks grows or cuts by any number on the side ANCHOR leaves free
  (LEFT/RIGHT, FRONT/BACK, BOTTOM/TOP; `cells::reframe`: cells kept where
  they stay, what is set down moved with them; one step, a cut that loses
  blocks too); a model's colors keep their number keys in the palette's
  order (a new one the next number, only REMOVE renumbers); Delete, never
  a plain letter, takes a frame or a level's block out (its file too; one
  UNDO brings both back, test
  `delete_removes_a_frame_or_a_block_and_undo_brings_it_back`); FRONT,
  SIDE, TOP in the voxel editor's view corner look straight at a model's
  front (+y), its right side and from above (a level's block: its y 0
  side), the one lit pressed again the free view as it was
  (`view::STRAIGHT`, `View::straight_view`; no keys: the number keys are
  the colors); a level's list, name field (its quiet hint), FIND and the
  shot's facts say the names the game gives its things (`lever_2`, a
  piece's `piece_3`; the file's own where it has one; the thing that
  becomes the player has none, its kind shows: `sim::things::PLAYER_BEHAVIOR`,
  `levels::thing_names`, the one rule of the editor, the load check and
  `Stage::name_things`; test
  `the_thing_that_becomes_the_player_has_no_name_in_the_file_or_in_play`;
  `Editor::thing_names` kept until a kind or name changes), nothing
  written for them, a name typed the thing's own and cleared the game's
  again; `--choose`, `--hide`, `--menu`, `--context` take such a name,
  and still an unnamed thing by its kind or model (`torch`, `torch#3`
  the third; `room::listed`); `--scatter N` without such a rule names the rules there
  are; a shot draws the panels as they are after two seconds where egui
  never settles (ADD MODEL's find in focus; `shot::settle`), and
  `--picker` opens among the pieces;
  `editor::EditorWindow` takes its window's events (`ide::route`). A
  double click in the list or EDIT above the file opens one; the main
  area shows it as a picture (`ide/preview.rs`, the editor's view
  without panels, turned by dragging; a figure of parts whole; a `.vox`
  world or prefab too, to look at; a model sits on a mid gray or the voxel
  editor's BACKGROUND), read afresh when the IDE's window
  is focused again. A `.vvx` is text too: TEXT beside EDIT shows it to
  read and change in the main area as a sound's (`ide/code.rs`
  `Syntax::Model`, read by `vvx::read` after every change, its mistake
  "LINE n: why"), PICTURE back (`--open text:FILE` in a shot); a level
  too (TEXT beside EDIT, `Syntax::Level` told by its format line, read
  by `GridLevel::from_text`, its line of what it is above), and a map
  (`map.toml`, no editor) is always text to change (`Syntax::Map`,
  `MapData::read`; `Shell::is_map`, `Files::level_text`). The logic's files are changed in the main area
  (`ide/code.rs`: coloured by `lua::lex::spans`, of which `tokens`, the
  fingerprint, is made, the same byte for byte, test
  `highlighting_keeps_the_fingerprint`; Lua reads the text after every
  change and marks the line it cannot; the API's names offered after
  `world:` and the like, from `lua::api`; Enter keeps the line's indent,
  two spaces more after `then`, `do`, `else`, `repeat`, a function's head
  or an open bracket, Tab and Shift+Tab indent the lines chosen by two
  spaces or take them away, each one step of the text's undo; the
  bracket at the cursor marked with its partner; the line numbers
  painted where the view begins, so they stay when a long line scrolls
  sideways; the text's and the numbers' layouts kept while they show the
  same, `code::Laid`: a still frame of 3,000 lines 0.6 ms, was 3.1),
  saved a second after the last
  change (`ide/buffer.rs`, a journal in the config folder, `ide/<folder>.journal/`,
  naming the file's whole path and offered on no other, against
  crashes; a change on disk comes in, or the IDE asks whose to keep).
  PLAY plays the game in the main area (`ide/play.rs`, no title or
  menus; its keys on a line under it, `play::keys_of`, the keyboard's
  names; AGAIN beside STOP begins it anew, a game held behind the file
  has BACK TO THE GAME in place of PLAY; a file saved comes in at once, and where the game would now
  begin otherwise (`Session::start_mark`: the world's voxels and the
  states as a fresh load has them, or a restart-only setting) the fresh
  game takes over with a line in OUTPUT, so what an agent builds in
  `on_level_start` shows without F5; a change to the rules alone keeps
  the run; Escape back to the file, F5 over, F9 keeps the run (the maker's
  keys, always on in PLAY), STOP; runs recorded as in `voxcade run`; a script's error with
  TO THE LINE, the line marked in the file until mended; PLAY draws no
  error panel over the game, `Session::problem_shown`, one plain line
  under the script's mistake, `play::plain_words`, and OUTPUT gets the
  whole traceback); `--shot
  --play TICKS` draws it (with `--open` the file over the game held; a game
  that would not start with its red line, test
  `a_shot_of_a_game_that_would_not_start_shows_the_red_line`).
  The headings LOGIC, SCENARIOS, MODELS and MUSIC are always there, each with a plus
  left of the arrow: a name typed in place makes a small Lua file, scenario,
  model or song to change (`Shell::make_file`; no leaving the IDE for a first
  file). The scenarios under a heading of their own below the logic, always
  there (a plus in the heading names a new one), PLAY SCENARIO over one plays
  the game from it (F5 begins it again); while the game plays, a line
  under OUTPUT runs Lua in it (the console), its answer in OUTPUT.
  SOUNDS (`sounds/NAME.toml`, always there, a plus makes a
  blip to change) lists the game's sounds, and below them in the quiet
  colour, "built in", the ones the engine and the ready-made rules bring
  that it has no file for (`Game::sounds`, passed to `Shell::with_sounds`);
  one opens in the main area read-only with MAKE IT YOURS, which copies
  it into `sounds/`. A sound is text to change in the main area (the
  editor of the Lua files without the Lua parts: `ide/code.rs` `Syntax`,
  read after every change by `audio::sound::read`, its mistake "LINE n:
  why" with the line marked, comments, strings and numbers coloured, no
  completion yet), a line of words of what it is (`ide/sounds.rs`), PLAY
  SOUND only where the maker is not over the text (one PLAY for a sound;
  the text as it stands rendered by `SoundBank::put`, through a
  speaker of its own at the game's sound volume and the player's) and
  under the text USED: the lines of the game's Lua that name it in quotes,
  a click opening the file there (`sounds::used`, where songs naming a
  sound as an instrument are to come). Between the line and the text the
  sound maker (`ide/maker.rs`, the sfxr family's way): COIN, JUMP, SHOT,
  BLAST, HIT, POWER UP, BLIP roll a fresh sound of the sort from a small
  table of ranges and play it (a dice before them their sign; the same
  sort again rolls another), PLAY, LOOP (again
  and again with a short pause, the user's idea) and UNDO a step back
  before them, the sorts on a row of their own where the room is short;
  a roll is rendered and measured before it is shown
  (`maker::good`), the faders PITCH, LENGTH, SLIDE, BRIGHT, PUNCH move
  words of their own, as many a row as the room takes (three and two at
  1024 wide; test `the_sound_page_has_one_play_and_its_faders_fit`); every change rewrites the text through `toml_edit`
  (comments kept) into the buffer as typing does, one step of the text
  edit's undo (UNDO and Ctrl+Z alike). A song (`.mml`) is text to change
  too, parsed after every change (`music::parse`), PLAY SONG plays the text
  as it stands. Every text one can change says SAVED or NOT SAVED YET above
  it, left of the buttons (the path and the line of facts take the room left
  of them and are cut with "…", full text on hover: `main_area::header_row`),
  and Ctrl+S saves at once. RENAME and DELETE
  (`ide/shell/manage.rs`; a right click on a row, and small beside SAVED
  over the open file) for the logic, scenarios, models, sounds, songs,
  images and the rest, a map's prefabs too (`prefabs/*.vvx`), not
  `main.lua`/`conf.lua`, levels or blocks (off with the reason on hover):
  the name typed in place as the plus's, the lines of the game's Lua
  naming the file (found by `ide/refs.rs`: Lua tokens, by where a quoted
  name stands) and of its levels and maps (`refs::scan_data`, by the
  format line: a piece's and a scatter's `model`/`models` from the game
  folder, a map's `[prefabs]` and `[ground]` pictures from the map's;
  levels and maps name no sound or song; only the quoted name rewritten,
  test `a_model_is_renamed_in_the_levels_and_maps_too`) ticked one by one in a modal
  (certain ones ticked, MAYBE ones unticked; RENAME, ONLY THE FILE,
  CANCEL) and changed only for the ticked; USED lists MAYBE apart; DELETE asks nothing, the system's trash (`trash`
  crate, else `config_dir()/ide/deleted/<key>/`) and UNDO in the bar for 20
  s (`manage::UNDO_FOR`) from the bytes kept in memory (up to 8 MB). Ctrl+F
  (or FIND beside SAVED) shows the find row above the open text
  (`ide/find.rs`; the owner's D9): FIND, MATCH CASE, "3 OF 12",
  PREVIOUS/NEXT (Enter, Shift+Enter, F3, Shift+F3), CLOSE (Escape), every
  match marked and the current one scrolled to; Ctrl+H or its arrow shows
  REPLACE WITH, REPLACE and REPLACE ALL, each one step of the text's undo.
  Ctrl+Shift+F or SEARCH in the bar searches the game in the list's place
  (the panel a third of the window wide meanwhile, up to 560 points; on a thread, the files the list shows that open as text: logic,
  scenarios, sounds, songs, models, levels and maps, not a world or a
  prefab), a line clicked opens its file there with the word current (a
  model's or a level's in its TEXT); `--shot --find WORD [--replace TEXT]`,
  `--search WORD [--hit N]`. (Go to definition, references and rename are
  not built: no LSP.) Songs play from the main area (PLAY SONG). Reading the keys
  (`controls::read_input`), the speaker (`audio::speaker`) and the
  player's options (`Settings::options`) are shared with `voxcade run`.
  BUILD packs the program and the game into the game's own
  `builds/<game>-<system>.zip` on a thread of its own (`ide/export.rs`,
  git-ignored, left out of the list and of the next build; no hidden
  files, no `replays/tests/`); the bar says BUILDING…, then where it went
  with SHOW IN FOLDER, or BUILD FAILED in red with COPY. What is wrong
  with the game (a halted script, a file that would not load, a game that
  would not start) is a red line over the main area whatever it shows
  (`Shell::game_error`: TO THE LINE, SHOW, COPY). The list follows the
  folder (names polled once a second, `Shell::look_at_folder`: what the
  agent makes appears; an open file that goes is said, its text kept).
  The list, the USED and rename searches and BUILD leave out what belongs
  to no game (the owner's idea: test renders filled the lists): `shots/`,
  `builds/` and hidden files always, and whatever the game's `.gitignore`
  files ignore (its own, its subfolders', and those above it up to its
  git repository's root; `gitignore::Ignore`, our own small matcher, no
  `git` program), BUILD saying which ignored files the game still names
  in quotes; the engine itself (`Files`, the sealed files) never
  filters, a file the game names loads ignored or not, and an ignored
  file open in the main area stays open. The
  console is folded to its bar for a game opened the first time, the
  shell started only on the first unfolding; folded and height are kept in
  `ide/<key>.toml`, `CONSOLE •` says news while folded; an agent that
  starts with the IDE has it open.
  OPEN beside the game's name (games opened last, another folder) and
  NEW (from a template into the projects folder, `~/Voxcade` or the
  settings'; every new game a git repository with its first commit,
  where git is there, `templates::begin_repository`; with the form's
  tick WITH CLAUDE CODE, on when the console is shown, the game's own
  notes for its agent, `CLAUDE.md` from `ide/game_claude.md`: the user,
  2026-11-11: nobody is forced to use Claude, other agents may come; the
  notes are no part of the game, left out of BUILD and of the sealed
  files); `voxcade ide` without a game shows the start page; one
  window shows one game after another (`ide/projects.rs`). HELP shows the
  Lua API by example (`ide/help.rs`: plain English, whole files to copy,
  each run as a game by `every_example_runs`). Below, CONSOLE and OUTPUT
  switch (`ide/output.rs`: BUILD, files that would not load, the game's
  errors with their line, the editors' troubles, the script's `print`;
  kept per game in `config_dir()/ide/output/`). The templates
  in `templates/` are built into the program (`ide/templates.rs`): a file
  added there goes into its list too (a test says which). Their figures
  are small and plain, the 8-bit charm (the jump & run's hero 16 voxels
  tall in blocks of 10, Voxkeep's from `tools/gen_voxkeep.py`; more in
  the same way with `tools/figures.py`; the voxcade-models skill):
  every new game starts from them.
- The Rust Digger: `cargo run --release --example rust_digger` (a window); with
  arguments it is the same command line (`-- shot examples/rust_digger
  --input ...`, `-- check examples/rust_digger`); keys `left right up down
  jump fire`. `cargo test` plays its replays too.
- Hear without playing: `cargo run --release -- sounds <game>` (every sound
  in a row; `--list` says each one's file or that it is built in, its
  length and peak) and `-- music <game>` (the song; or `-- music FILE.mml`, one
  song) write WAVs to `shots/`, through the limiter as the game plays
  them: its `c.audio.drive` and `c.audio.ceiling` (the limiter's
  ceiling in dB true peak, -6 to 0, -1 unless set, live as the drive;
  BS.1770's 4x meter holds it to 0.01 dB, a finer 8x meter sees up to a
  few tenths more on hard-edged waves, test
  `a_ceiling_a_tenth_under_full_scale_holds`); a song file alone at the
  engine's defaults, which it says. You
  cannot listen: check them as a spectrogram (numpy + PIL), and the
  printed lengths, peaks and channel-length warnings. `shot` prints which
  sounds a run made.

- Measure a frame: `shot` prints `frame: cpu ... | gpu ...` (building the
  frame, and the GPU per pass) and `ticks: ... slowest ...` (simulation
  and chunk remeshing per tick: blasts); `--at x,y` starts the player
  anywhere. Load test at 16-bit sprite scale:
  `python3 tools/gen_bench.py`, then `shot games/bench --screen title`.
  Numbers and the plan for the 16-bit scale: `docs/engine.md`.
- The lamps and the fog: `games/lights`, a test hall as the Bench is a
  load test (no cover, so the launcher lists it not; the packages leave
  it out; `voxcade check` plays its replay `tests/every_subject`): a
  level of blocks 4 voxels each way (`levels/hall.toml`), its lamps that
  stand still as the level's lights (dark but in their subject), its thin
  walls, glass and water as pieces; `main.lua`'s `SUBJECTS`, Q and E
  stepping through them, the arrows walking the hero, each a scenario
  (`voxcade shot games/lights --scenario fog`: walls, diagonal_wall,
  spots, shadows, fog, glass, dancers, posed, beam, out_of_view,
  headlights, load); four of them are picture tests (`lights_*` in
  `tests/pictures.rs`).
- Songs a game writes: `games/drums`, a drum machine as Lights is a test
  hall (no cover, not listed, not packaged; `GAMES_OF_VOXCADE` names it):
  sixteen steps in three rows (kick, snare, hat) over the studio
  `music/kit.mml`, two patterns, the arrows move the cursor, TOGGLE
  (Space, Z, the pad's south) sets a step laid "now", PATTERN (Tab, X,
  north) switches at the next bar, a running light from
  `music.position`, all drawn with `frame:box` and `frame:text`; its
  scenarios `second_pattern` and `empty_machine`; its replay
  `tests/toggle_and_switch` toggles steps, switches the pattern, calls a
  switch off before the bar and edits a pattern while it waits.
- A machine played through its model (a trial, 2026-11-22): `games/studio`
  (no cover, not listed, not packaged; `GAMES_OF_VOXCADE` names it), the
  drum machine as a level's piece, `machine.vvx`, a figure of 50 parts
  (`tools/gen_studio.py` made it and keeps it: the owner's now; main.lua
  knows its parts by name only): `step_1`..`step_16`, a column a voice
  (`pick_kick`/`_snare`/`_hat` under its level knob `knob_kick`/`_snare`/
  `_hat`), `pattern_a`/`_b`, `start`, `knob_tempo` (shown only),
  `knob_volume` (`music:volume`) and a lamp `led_*` for each button; its
  labels and the nameplate (`NAME`, HARALD RG-80: the owner's invented
  maker and its Rhythmus-Gerät, RM in English) painted into the case's top in 3 by 5 voxel letters. A knob is
  one value turned by the hand alone (the owner: a knob that turns by
  itself cannot be). `c.pointer = "system"`: a click reads
  `input.pointer.part`; the machine answers by `thing.state.parts` set
  every tick (a button held sinks by `offset`, a knob turns) and by
  `frame:pose` each picture (a lamp lit or out by its `glow`, the part
  under the pointer, else the keys' cursor, lit a little); STOP
  plays the silent studio `music/off.mml`. Its scenarios `empty_machine`
  and `stopped`; its replay `tests/click_the_machine` clicks steps, the
  edge of one and the case between two, the voices, a pattern, drags and
  wheels the knobs, stops and starts.
- A part lit or out, a pose for one picture (the owner's yes,
  2026-11-22, to `shots/reviews/machines-as-models-draft.md` as its
  review narrowed it): a pose entry (one type, `PoseEntry`, in
  `frame:model`'s pose, a piece's `thing.state.parts` and `frame:pose`;
  `session::PoseEntry`, read by `lua::world::pose_of`) takes `glow`, 0 to
  1, every voxel of the part in place of its colors' own
  (`PartPose::glow`, `ModelDraw::glow`, the shader's `Actor.extra.y`,
  below 0 none: `model_glow` in `vs_model` and `vs_face`; the cubes, the
  bank's `cubes` too, `render::glow_byte`); a glow alone moves nothing
  (`PartPose::moves`: no depth taken nearer, the box as at rest); what
  stops a lamp keeps the file's glow (`volume`, `movers`) and
  `volume::pieces_key` leaves it out. `frame:pose(thing, pose)` poses a
  level's piece for the picture over its state's pose (`session::posing`,
  the rule `frame:model`'s pose has over a swing: a rotation on top, an
  offset added, a glow in place), two calls laid per part and key
  (`LuaGame::picture_pose`); `on_draw` runs before the pieces' loop
  (`LuaGame::run_on_draw`, its commands after the effects as before); a
  thing of a kind is a mistake saying so. Tests
  `a_glow_in_a_pose_lights_a_part_and_moves_nothing`,
  `frame_pose_poses_a_piece_for_the_picture_alone`,
  `the_pointer_meets_a_part_where_frame_pose_drew_it`,
  `a_part_posed_to_glow_shows_brighter_on_the_gpu`.
- Tests on every push (the owner's D1, 2026-11-13;
  `.github/workflows/tests.yml`, push to `main` and pull requests, Linux
  only, mesa's lavapipe standing in for the GPU): the release build,
  `cargo test --release`, clippy with `-D warnings`, `voxcade check` of
  every game but the Bench and of every template, and
  `tools/check_tmp.sh`, all on Rust 1.99 (the development machine's, not
  `stable`: a newer clippy's lints must not move under us; raise it with
  the machine); its job `msrv` checks the build with Cargo.toml's
  `rust-version` (1.95, what egui 0.37 asks; not blocking until it has
  passed once); `.github/workflows/audit.yml` runs `cargo audit` weekly
  and by hand, blocking nothing. A test that draws needs a GPU or lavapipe and
  fails without one, it never skips. Picture tests (`tests/pictures.rs`,
  their references as text in `tests/pictures.txt`, the owner's D12): the
  mean color of an 8 by 8 grid of five pictures within 6 levels
  (`VOXCADE_PICTURE_TOLERANCE`; the workflow sets 12 for lavapipe); renew
  them on purpose with `VOXCADE_PICTURES=renew cargo test --release --test
  pictures` and say so in the commit; `every_letter_reads_as_it_did`
  draws every character of the pixel font and More Blood's display font
  on a plain sky, its means in cells of 8 by 8 pixels against
  `tests/letters.txt` (a wrong letter moves cells; renewed alike).
  `tests/docs_examples.rs` reads
  the level, map and model examples of `docs/tiles.md` with the engine's
  own readers; `a_picture_drawn_twice_is_the_same_to_the_byte` (in
  `tests/pictures.rs`) draws each picture twice. `tests/docs_names.rs`
  checks that the `src/` paths and test names the docs cite exist, and
  that every `voxcade …` example in README.md, CONTRIBUTING.md and
  `dist/*.txt` parses with the real parser (`cli::takes`) and names
  files and scenarios of the repo that are there
  (`the_command_examples_in_the_docs_run`).
- Builds for other machines: GitHub
  Actions `Builds` (`.github/workflows/builds.yml`), started by hand
  (`gh workflow run builds.yml`) or by a `v*` tag, never on every push,
  on the Rust `.github/rust-toolchain` names, as Tests and Audit (one
  line to raise; `the_workflows_build_with_one_rust`)
  (macOS minutes count ten times on the private repo; the internet step
  is `continue-on-error`, a warning in the run's summary, and no longer
  holds the packages back): Linux and Windows
  (macOS Intel only started by hand with it), SDL3 built in (`--features
  static-sdl`); `tools/package.py` puts the folder together (the games
  but the Arena, the Bench, Lights, Drums and Studio, `dist/README.txt` and `LIESMICH.txt`, players first; on
  Linux `start-voxcade.sh`; `THIRD-PARTY-LICENSES.txt` from
  `tools/third_party_licenses.py`: the licences of every crate linked,
  SDL, Lua and egui's fonts). The program started without anything opens
  the launcher (`ide/projects.rs`, `StartPage`: the games beside the
  program and the user's own, PLAY in a window of its own, EDIT in the
  IDE (on a game beside the program it asks over the picture first: MAKE
  A COPY into the projects folder, `projects::copy_game`, the copy
  opened, or EDIT HERE, which an update replaces; `ide --shot --open
  edit:FOLDER`), NEW GAME (and a tile + NEW GAME under YOUR GAMES while
  there are none), the line about new versions; each game a card with
  its picture, `cover.png` in its folder, 480 by 270, a shot of the
  game; without one the card shows its name; of the games beside the
  program only those with a cover are listed, and the package leaves
  the Arena, the Bench, Lights, Drums and Studio out: only the games we show, the user). New versions
  (`src/update.rs`, `voxcade update [--install]`): the feed is the
  website's `release.toml` (the version, each system's package and its
  SHA-256; `VOXCADE_UPDATE_FEED` another, a file's path too), nothing
  fetched but the feed until UPDATE NOW; the package is taken only with
  the feed's checksum and swapped in where the program lies, what was
  there going to `before-update/<version>/`, the games folder game by
  game; every generation is kept, none deleted by a later update (a game
  beside the program the launcher's EDIT changed in place is in one),
  `update --install` saying how many and their size (`update::kept`).
  The icon
  (`tools/make_icon.py`: `src/icon.png`, `dist/voxcade.ico`) is on every
  window (`src/desktop.rs`), in the Windows program (`build.rs`), and on
  Linux the launcher keeps a desktop entry and the icon in
  `~/.local/share` (a Wayland desktop takes a window's icon from the
  entry named as its app id, `voxcade`). A
  mistake at the start of a window is said in a window in plain words
  (`cli::plainly`, English, German, French) only where the command opens
  one (`Call::opens_a_window`); `edit --fill/--clear/--scatter` and any
  `--shot` print to stderr, `edit --dry` alone is refused, and a command
  refuses trailing words it ignored before; an option with one value
  given twice is refused (`table::REPEATABLE` names those that may come
  again: `--do`, the editor's steps); a number option given a word says
  which option wants what and what it got (`parse::number`, test
  `every_number_option_says_what_it_wants`); `inspect --record` without
  `--host`/`--join` is refused (`shot --record` records). A crash of a command that opened a
  window is no silent close (`cli/crash.rs`, its panic hook): the
  message, where and a backtrace go to `config_dir()/crashes/` (the newest
  10), and the main thread's crash says in a message window where the
  report lies (tests `a_crash_report_is_written_and_the_newest_kept`,
  `a_crash_is_said_in_the_systems_language`; never provoke a real
  box). The windows take their picture through `Screen::texture`: an
  outdated surface configured again, a lost one made anew from the
  window, a lost GPU device (wgpu's callback) ending the window with a
  plain message (`window::DEVICE_LOST`). Fetch builds with `gh run
  download`. A `v*` tag makes a release of them here; the public
  downloads are the same files as releases of the website's repo
  (`../voxca.de`, its README says how,
  with the blog post that goes with each version). Apple silicon later.
- Replays (`src/replay.rs`): `voxcade check <game>` plays every replay in
  the game's `replays/` folder without a window, several at once (each its
  own session, up to six, `VOXCADE_CHECKS` another number; the lines in
  the files' order, test `replays_checked_at_once_are_said_in_order`;
  `--update`, `--refit` and `--rollback` one after another), drawing the game after
  every tick as a window does (`Session::look`: the kits' things and
  `on_draw`, no picture; a mistake in `on_draw` stops it as in a window,
  test `check_draws_every_tick`), and says whether each runs as recorded
  (a replay also keeps the state its run ended in, `final_state`, and the
  fingerprint's parts by name, `fingerprint_parts`: its last fraction of
  a second is checked, and one recorded on other data names the files
  that changed; an older replay plays, `check` saying how many of its
  last ticks went unchecked, and one with nothing to check it by fails;
  `check` goes on after a replay that cannot be played and ends with the
  mistakes; `replays/tests/` holds scripted regression runs made with
  `shot --input ... --record FILE`; `shot --replay FILE` shows one). Run it
  after any change to the simulation. When a change was meant to alter
  the game (tougher enemies, a new level), `voxcade check <game> --update`
  records the checks anew (it runs the hidden-state guard and records no
  run it refuses): say so in the reply, the user's own recorded runs may
  then show something different. When the game's files change
  but the game should not (a new layout of `conf.lua`, renamed markers),
  `voxcade check <game> --refit` plays the replays on the new data and,
  only if every one runs exactly as recorded, writes the new fingerprint
  and its parts into them (nothing else). `voxcade check <game> --rollback K` goes back K
  ticks again and again and plays on, as rollback over a network would
  (`docs/multiplayer.md`, stage 0): each replay must still come out as
  recorded, and it says what keeping and bringing back the game cost; the
  Builds workflow checks every system so (test
  `going_back_and_playing_on_comes_out_the_same`). Every run played in the window
  is recorded (`replay::Runs`): runs of 10 s or more are kept in the
  config folder beside the settings (the newest 20; `voxcade replays
  <game>` lists them), F9 puts the run being played
  or the last one in the game's `replays/`; `voxcade replay <game> <file>`
  shows one in a window. A run with a hot reload in it is not kept
  (`Session::touched`: it no longer starts from the files). Attract mode
  (`replay::Attract`): after `c.title_screen.attract_after` seconds (30;
  0 never) on the title without a key, the game's replays in `replays/`
  itself play in turn (not `replays/tests/`), under the title music with
  the game's sounds at a third (the user's choice; every example game
  has one, `replays/demo.toml` or More Blood's own run), PRESS START blinking
  (`ui::press_start`); any key, the end or a check running apart goes back
  to the title. `shot --replay FILE --screen attract` shows it. Films:
  `voxcade film <game> <replay> [--from S --to S] [--max-mb N] --out
  x.mp4|x.webm|x.gif` (a sound pass mixing the game's music and sounds,
  then every frame drawn without a window and piped to ffmpeg; GIFs built
  in, `gif` crate). H.264 at CRF 23 (hardly to be told from 18 on the
  sand's grain, half the size); for a chat with a
  size limit `--size 1280x720 --max-mb 49` stays under 50 MB.
- Measure on weak hardware too (the user's aim: older machines run the
  games without great loss, quality levels take off the rest): this
  machine is high-end. The CPU's own small GPU (Radeon 610M):
  `MESA_VK_DEVICE_SELECT=1002:13c0 MESA_VK_DEVICE_SELECT_FORCE_DEFAULT_DEVICE=1`
  before `shot`; fewer cores: `taskset -c 0` or `taskset -c 0-3`. `shot`
  sends chunk uploads to the GPU every tick, as the game does every frame
  (held back they pile up and distort the timings).

## Verify visually

Any change to rendering, the simulation or a level is checked with `shot` and
by looking at the PNG. The stats line it prints (chunks, vertices, particles,
player position, gunship state) is the quick numeric check. `shots/` is git-ignored.

## Rules

- Tests take their folders from `scratch::Scratch` (`src/scratch.rs`,
  `voxcade-NAME-PID-N`, removed when dropped, a panic included), never from
  `std::env::temp_dir()` by hand: the temp folder here is a tmpfs in RAM and
  leftovers once filled it (25,800 of them, 14 GB) and stopped the machine.
  Keep the guard in a binding that lives as long as the test; code outside
  tests that writes to the temp folder removes what it made, on error too;
  `tools/check_tmp.sh` runs the tests and fails if `voxcade-*` entries grew.
- Everything in a game is code or text (the owner, 2026-11-12): models,
  figures and blocks are `.vvx` (`src/vvx.rs`; the models skill has the
  format: the format line first, signs a colour with its material,
  layers bottom first and the far row first, repeats `*N`, `at` and
  joints from the pivot, a part's rest rotation `rotation`, `pitch`,
  `roll` about its joint, written only when not 0; a part `like` another
  shows that part's voxels, `mirror` `"x"`/`"y"`/`"z"` within the
  source's box, no layers, its `at` its own: the owner's, 2026-11-15, a
  car's wheels, a mirrored half body; a source has layers of its own, a
  clone counts as its own voxels; resolved as the file is read,
  `vvx::clone_voxels`, so nothing downstream knows; tests
  `a_clone_is_its_source_written_out`, `a_clones_mistakes_say_their_line`,
  `the_python_writer_keeps_clones_as_the_engine`; a `[[part]]` with
  neither `layers` nor `like` is an empty part, Blender's Empty, the
  owner's 2026-11-16: no voxels, a node the others hang on, its `at`
  where it stands (the pivot unless set; `vvx::Piece::empty`,
  `vox::Part::empty`), the first naming no parent the root (`parts_of`,
  turning about its cell's middle), a part on it turning where it turns;
  no clone of one, no figure of empties only; tests
  `an_empty_part_is_a_node_the_others_hang_on`,
  `an_empty_parts_mistakes_say_their_line`,
  `an_empty_roots_parts_turn_with_it`). MagicaVoxel's `.vox` is only brought in
  (`voxcade import`, `voxcade upgrade`); JSON is gone from a game (the
  owner: it only gets in the way of git's diffs), but for `.mcp.json`
  and LuaLS's `.luarc.json` (other programs' files) and a Tiled map read
  once by `voxcade import`. A model's voxels come out in the text's
  reading order: More Blood's men burst into theirs in that order. A
  model, a level and a map state their format and version on the first
  line (a save `format = 1`, a replay `version`; sounds, songs and
  scenarios state none);
  every game states its Voxcade (`c.voxcade`); a format that changes
  brings a step in `src/upgrade/` (a step that comes while a version is
  being made also comes to the games already stating it, by `left`: their
  files still have what it changes), and the repo's games and templates
  are brought over with `voxcade upgrade` itself. `tools/vvx.py` writes
  and reads `.vvx`, levels and maps from Python, as the engine writes
  them; the generators write those (byte for byte what the upgrade made
  of the games' files; gen_moreblood, gen_arena's player and pulsar's
  `make_models.py` still write `.vox` figures, to import): `write_world(path,
  model, palette_signs(path))` for a world or prefab, `write_model` for
  models only (test `the_python_writer_writes_what_the_engine_writes`). A
  model's box holds at most `vvx::MOST_VOXELS` (512 by 512 by 128);
  `sheet` says what it cannot draw; a block's colors are plain (glass,
  metal, glow are refused with the line). A level's things, places and
  areas beside its ground plan or under its floor are a mistake at load
  (`grid::outside_mistake`; the sky above is open; the editor still reads
  such a level), a map's `[[put]]` and `[[place]]` the same.
- Names say what other tools say (the owner, 2026-11-12: "turn" meant
  five things; LÖVE's and the common words, American spelling): a
  bearing is a `rotation` (degrees clockwise seen from above, north 0: a
  level's thing, a map's `[[put]]` and a title prop in quarter turns,
  `frame:model`'s `{ rotation, pitch, roll }`, type `Rotation`), a
  block's way of lying in a level's legend its `orientation` (0 to 47),
  a kind's or the camera's `rotates = "free"`, a kind's `behavior`;
  `--rotate` and `--color` on the command line. The old names are
  refused with the new one; the upgrade step "rotation and behavior"
  brings a game over (a map's `turn` q is `rotation = -90 q mod 360`). A
  new name an author writes or reads follows suit (no colour, centre,
  grey in a key, a field or a flag); prose may stay as it is.
- What a file asks for is counted before it is made (an `*99999999999`
  once asked for 2.4 TB and aborted): a layers text's rows and layers
  against its box's room (`vvx::Room`, `layer_list`), read by one reader
  for models, worlds and prefabs (`vvx::read_cells`); a model's box
  `vvx::MOST_VOXELS`, its frames together `MOST_IN_ALL`, `MOST_PARTS`
  parts; a world or prefab `scene::MOST_VOXELS` (1024 by 1024 by 128);
  maps and levels `world::too_big` (65536 each way, 4096 by 4096 columns,
  2^30 voxels; More Blood's map is 640 by 2368 by 80), all in whole
  numbers that cannot overflow. A plain mistake with its line, never a
  panic (test `hostile_texts_are_mistakes_never_a_crash`). Every number
  read from a file is range-checked before a cast, never wrapped
  (`vvx::Reader::ints` exactly, at most `vvx::MOST_WHOLE` either way,
  past it "past the limit"; `Reader::whole` an integer in a named range;
  test `numbers_past_their_range_are_mistakes_never_wrapped`); no game uses
  `[terrain]` yet, so `a_terrain_level_builds_through_load_at_as_pinned`
  takes one through `grid::load_at` and pins its voxels.
- Levels and worlds are text (`docs/tiles.md`, 2026-11-12). A level of
  blocks is `levels/NAME.toml` ("voxcade level 1", `grid::text`): `block`,
  `size`, `view`, `location`, `persistent` (levels that remember, below),
  `[blocks]` (name = kind), `[legend]` (a sign a block
  and its orientation: `s = "stone"`, `S = { block = "stone", orientation = 17 }`),
  `[cells]` with `layers` as a `.vvx`'s, `[[thing]]` and `[[place]]`; a
  thing of a kind may carry `props = { to = "02_crypt" }` over its kind's
  declared ones (the owner, 2026-11-14: never a piece's), checked as the
  game loads by the check `level:spawn`'s go through
  (`sim::things::check_prop`, `levels::Made::check_props`; again when
  `conf.lua` changes), `thing.props` reading them alike, PROPS in the
  level editor's rows (`stage::prop_rows`, `Editor::set_prop`, `edit
  --prop KEY=VALUE`); a kind's `names = { to = "level", at = "place",
  works = "things" }` (the owner's words, 2026-11-14: `"level"`,
  `"place"`, `"thing"`, `"things"`; `sim::things::Named`) says which name
  something, checked with every level read as the game loads, a level
  file or `conf.lua` read anew and at `level:spawn`, one check
  (`levels::Levels::check_named`, `lua::checks::check_names`: a place is
  one `go_to` takes, `"start"` or the level's places, in the level the
  `"level"` prop names; a thing by `level:thing`'s name, the file's or
  `levels::called`'s `lift_1`); PROPS offers the names there are
  (`stage::name_choice`, `Editor::name_choices`, found anew only when
  their sources change; `--menu prop:KEY`; a thing chosen by the name the
  game gives it, `lever_2`, gets it as its own in the same step,
  `Editor::names_to_fix`, so taking out `lever_1` moves no prop, the
  numbers passing over names taken, `levels::unused`), a name not there in WARNING,
  and RENAME rewrites the props naming the thing or place in its step
  (`Editor::props_renamed`).
  `view = "front"`: each layer the grid seen from the front, the front
  layer first, its top row first (a side view's room reads as the
  picture); `"above"`: the bottom layer first, its far row first (a farm
  from above); the editor keeps a file's view and signs (`Legend`), a new
  one is seen from the front when taller than deep. A map is `map.toml`
  ("voxcade map 1", `map::MapData`): `size`, `[ground]` (the height and
  material PNGs, `[[ground.materials]]`), `[prefabs]` (name = file, or
  `{ file, soft = true }`), `[[put]]` in order (`prefab` with `at` and
  `rotation`, degrees clockwise in quarter turns, or `thing` by kind),
  `[[place]]`. A world or a prefab is a
  `.vvx` of one frame (`src/scene.rs`): its n-th palette colour is the
  world's number n (state hash, replays and saves keep numbers, so the
  palette's order is the meaning, never the voxels' order), `r = { marker
  = "rifleman" }` an entry that places a thing of that kind (the things in
  the order of the marker entries, each one's cells in reading order);
  `c.markers` is no more (the upgrade moves it into the files). The height
  and material maps stay PNG pictures.
- A level may hold a landscape, `[terrain]` (`src/terrain/`,
  `docs/tiles.md`, 2026-11-13; the owner's "landscape generator light",
  text, no height picture): `spacing`, `at`, `heights` (whole voxels at
  control points, the far row first, lined up), `shapes` (`.` SMOOTH a
  clamped Catmull-Rom, `^` LINEAR, `#` STEP: that land cut into terraces
  whose cliffs follow its contours; the first STEP, a plateau over each
  point's square, was "more angular than Minecraft" and is gone; shapes
  blend by the four points' bilinear weights), `terrace`, `rough` and
  `roughness` (three octaves of value noise, a digit a point),
  `[[terrain.layer]]` (bands of rock from the bottom up, `thick`, `hard`:
  a wall or a slope of scree, their colors on walls only, a voxel open
  over a drop of 3 or more, where only the unconditional material would
  paint; the owner saw red specks on a plateau before; the bands dip,
  fold, swell, thin and pinch out, `dip`, `dip_to`, `fold`, `fault`,
  never a layer cake unless `dip = 0, fold = 0`), `seed`, `under`,
  `[[terrain.material]]` (the first whose `above`, `below`, `steeper`,
  `flatter` all hold paints, the slope over three voxels each way, the
  gentler side, so a contour step is no slope; `blend` lets the borders
  wander, lower in hollows; `colors` two or three tones in patches of
  warped noise, `patch`; `depth`,
  `grain` as real palette shades; the last has no conditions). Every word
  optional, unset changing nothing (test
  `a_terrain_without_the_later_words_builds_as_before` pins the voxels).
  Built with the level (`grid::build`, after the blocks, their colors
  first): every column from the world's floor up, so nothing hangs and
  nothing falls loose, in plain sums and our own value noise (`libm`'s
  tangent; test `the_simulation_does_its_own_math` scans it), 640 by 576
  voxels with every word in about 160 ms (640 by 400 plain about 50). The level editor's
  TERRAIN (`editor/model/land.rs`): points on the land dragged up and down with
  a brush (RADIUS), SELECT, FLATTEN, SMOOTH, PAINT, ROUGHEN, CALM, SHAPE,
  SPACING, POINTS, TERRACE, ROUGH, SEED with REROLL (`gui::dice`),
  LAYERS, MATERIALS, UNDER; the view built again only where points
  changed (`Shown::update_land`); only its lines of the file change.
  `tools/vvx.py` reads and writes it (`read_terrain`,
  `write_terrain_level`). `[[terrain.scatter]]` rules (`model`/`models`,
  `density` per 100 by 100 voxels of fitting ground, the materials'
  conditions, `apart`, `rotate`, `sink`, `as`): a jittered grid from
  `seed` (`Terrain::scatter`); `as = "voxels"` baked into the land at
  build (`grid::build`'s `ModelsOf`), `as = "pieces"` set into the level
  once by the editor's SCATTER (marked `scatter = N`, the owner's from
  then on; CLEAR takes them away; `voxcade edit --scatter N` without a
  window). Things standing on the land follow it when it changes, in the
  same step (`Editor::follow_land`). In TERRAIN the outliner holds
  MATERIALS, LAYERS and SCATTER (a row each), the inspector the tool, the
  numbers and the chosen row's fields; a point drags along its vertical,
  under the pointer (`land::height_along`), Shift a third. A model ADDed
  among the pieces stays on the mouse for the next click until Escape
  (`Editor::stamp`: firs by the dozen).

- The simulation (`src/sim/` and a game's rules, `src/shooter/`) is deterministic: fixed tick (`DT`), randomness
  only from `Sim::rng`, no clock, no iteration over `HashMap` in game logic.
  `shot` and replays rely on this, on every system alike: sines, cosines
  and the like only through `sim::math` (the `libm` crate; the system's
  maths library differs in the last bit between Linux, Windows and macOS),
  test `the_simulation_does_its_own_math` (it scans `src/sim/`,
  `src/shooter/`, `src/platformer/` and `src/lua/`, not the shooter's
  `draw.rs` nor the tests). A replay
  carries the game data's fingerprint (`Session::fingerprint`: the
  simulation's part of the settings in effect, the game's own sections
  (`Game::sections`), `main.lua` as tokens (`Game::code`: comments and
  layout do not count), the level files, the figures' models) and the state
  every second (`Game::state_hash`, the shooter's takes in the core's);
  anything new that carries the game forward goes into `state_hash`. A
  game of sealed files (`Files::seal`, `Session::load_from`) changes only
  by `Session::apply` (APPLY, an event of the tick; a snapshot holds the
  files of its time), and its replay is version 2, naming its files by
  hash (`files`, `applies`; the bytes in `Store::local()` or `carried`,
  which F9 puts in). The
  shooter's behaviors run as systems in a fixed order (player, soldiers,
  gunships, hostages, grenades, bullets, debris, level, camera), each over
  its own things in the order they came, and its hash takes the things
  grouped the same way: keep both orders, or every random draw moves. A
  game keeps its whole state with `Game::snapshot` (a Rust game whose
  state is a value clones itself: configs and shapes behind an `Arc`, the
  world's chunks shared until written; a Lua game copies its states,
  `world.state`, `player.state` and `thing.state`, out of Lua):
  `Session::snapshot` keeps it, `restore`
  brings it back (tests `a_restored_game_plays_on_the_same`,
  `a_restored_lua_game_plays_on_the_same`). Pins (`src/shooter/pins.rs`) hold
  the state every second along paths More Blood's replays do not take (the
  kit's numbers, a nose-on gunship, aim assist off); a change meant to
  alter the game pins them anew and says so. Kept on purpose until tidied
  in their own commits (with `check --update`): a second round on a man
  who fell this tick lands on the one who moved up among the enemies, the
  gunships slide on through hit stop.
- Several players (`docs/game-model.md`, "Several players";
  `docs/multiplayer.md`, its stages): `c.players` seats, each player a
  handle by its seat (`host::seat_handle`, seat 1 `PLAYER`), its state,
  keys (`p.input`), figure (Plain's `seats`) and the engine's keys
  (`host::SeatState`); seats taken and left are input
  (`Session::set_seat`, `Game::set_seats`; the window's `src/seating.rs`:
  a pad's START, the keyboard with Return: whole while seat 1 is on a pad,
  else its right half; the players page, a game of several's CONTROLS in
  the settings, the user's wish for more say in who plays with what: each seat's part of
  the keyboard and its pad, SWAP, each player's own keys, `Page::Players`
  and `Page::PlayerControls`, the halves' `Settings::left_keys` and
  `right_keys` with their directions, a seat's own pad `seat_pads`; what
  the page shows plays from a run's start, `Settings::players`, pads kept
  by name, `seating::SeatPlan`), replays carry
  each seat (`SeatRun`, `away` while free), `shot`/`inspect --input2`.
  Nothing of it enters a game of one's hash, saves or `pairs`: every
  game's replays came out the same. The platformer kit plays several
  (`Kit::plays_several`: a hero for each seat, `seat_heroes`, each its
  keys and hearts, `Host::put_seat`, `hold_back`), the shooter kit too
  (every seat's player a figure of `c.player.kind`'s, a kind without
  behavior, `Brain::Player` with its `Seated`: hit points, gun, rounds,
  down; seat 1's as well, the kit's own `Player` only in a game of one,
  so More Blood and the pins stay byte for byte; the systems go for the
  nearest player in the fight, `Shooter::fighters`; a round or a blast
  names the player it came from, `Bullet::from`, `Grenade::by`; teams,
  `p.state.team`, each seat's passed in every tick, `Kit::set_seat_teams`:
  rounds, blows and blasts hurt players of another team, `Fighter::team`,
  `Shooter::team_of`, test `the_shooters_players_of_two_teams_hurt_each_other`). Pulsar
  and every template play two. Over a LAN (stage 4, `src/net/`; the user:
  nobody waits): the host builds every tick's bundle from one slot per
  seat (a late input goes into the next tick, a missing one repeats
  6 ticks then holds nothing), its own keys held back by the guests'
  delay; guests play the bundles at the host's pace, a guest coming late
  gets the recording and plays it through (in pieces, a few bytes a tick;
  one longer than `wire::MAX_BLOB`, 66 MB, is turned away saying so); a
  machine joining under a name taken is numbered ("Maschine 2",
  `host::unique_name`); `Session::play` takes a
  `session::Tick` whole, every machine's recording is the host's byte
  for byte; `game::Local` is what a machine draws of its own
  (`frame.players`, its language in `on_draw`, its high score lines); the
  state hash in parts (`Game::state_sections`, `Session::sections`) is
  compared every 10 ticks, a desync named by its part and reported in the
  config folder (`desyncs/`). A varint has one encoding (overlong ones
  refused, test `a_varint_has_one_encoding`); a HELLO with a playing
  machine's id and a new nonce replaces it only once it has been silent
  `QUIET` (a restart), so no forged packet kicks a player (test
  `a_hello_with_a_known_id_kicks_no_one_who_plays`; the LAN has no
  authentication still: a spoofed BYE or address needs crypto, 0.7). `voxcade run <game> --host [PORT]` and
  `--join ADDRESS`, NETWORK on the title of a game of several; `inspect
  --host/--join --record` without a window (two programs over 127.0.0.1
  record the same run); windows tried hands-free: `run <game> --input
  SCRIPT` plays player 1's keys from a script, the window prints what the
  network does (joins, seats, leaving, desyncs), a guest keeps its run
  as the host does (`-guest` in the kept runs), a virtual pad (python
  evdev on /dev/uinput, which the user may write) drives the menus of
  the focused window only; `voxcade soak N --digest FILE` (the Builds
  workflow compares it with `.github/soak-1000.txt` on every system;
  after a change to the protocol, write it anew). The host's firewall
  must let UDP 26500 in (with ufw: `sudo ufw allow 26500/udp`; a
  firewall may drop the LAN's search too). Across the internet:
  `voxcade nettest [internet | relayed | direct]` measures iroh at 60 Hz
  with a host and three guests in one process (n0's public relays: for
  our own trials only), `nettest host` / `nettest join TICKET` between
  two machines. A network game across the internet: HOST ON THE INTERNET
  (the ticket, then the code of the game's room at our matchmaker, to
  the clipboard; the room shows its seats taken; the host lets each
  machine in by the four signs both screens show), JOIN lists the games
  on the internet beside the LAN's (`network::NetFinder`), ANY GAME ON
  THE INTERNET, TYPE A CODE, PASTE A TICKET OR CODE (these with a
  matchmaker known, `Menu::internet`); a game is known there as
  `voxcade/<folder>` and its fingerprint (`network::app_of`: only the
  same meet). `run --host internet`, `--join CODE|TICKET`, `inspect` the
  same (letting everyone in; `--join any`), `voxcade rooms <game>` the
  list; `VOXCADE_REACH=relayed | direct` for tests, `VOXCADE_RELAY` (and
  `_TOKEN`) another relay than ours (`OUR_RELAY`),
  `VOXCADE_MATCHMAKER` another matchmaker's key (`MATCHMAKER`); a local
  `matchmaker --config FILE` (`--new` makes one; `../matchmaker`, whose
  `matchmaker-protocol` the game takes in by git at a fixed `rev`: a
  change there is pushed first, then the `rev` here) serves both on
  127.0.0.1:7340.
- The pointer, the sticks and typed text (the owner's "bau es" to
  `shots/reviews/input-draft.md`, 2026-11-19; `docs/game-model.md`,
  "input"): `src/input.rs` (`Input` holds `moves`, `aim` in 127ths, a
  `Pointer` and `Typed`; it is `Clone`, no longer `Copy`), `c.pointer =
  "system" | "own"` (restart-only, in the fingerprint only when set; the
  mouse's key names `input::MOUSE_KEYS` in `c.keys` only with it). The
  tick never sees pixels: the machine answers from the picture it last
  drew (`Session::point`, `Game::panel_at`, the panels `on_draw` named:
  `frame:hud`, named `frame:menu`/`frame:grid`, `ui::menu_place`,
  `grid_place`), the tick checks (`Pointer::checked`: another level's
  picture, `input::picture_mark` of `Game::level_mark`, is nil whole).
  The window gathers the mouse and the text between ticks
  (`src/app/mouse.rs`: a click carries the answers of its moment, a
  wheel notch a press with a tick between, the text a sort a tick,
  `TypedQueue`); the engine's menus take the mouse (`ui::menu_rows`,
  `Menu::choose`). Typing is a player's state key, `p.state.typing`
  (`SeatState::typing`, hashed and saved only when set). Wire version 2
  (`wire::input`: keys alone as before, a flag bit for more; a guest's
  text goes as `Ask::Typed`, once and in order; the host keeps the first
  press's pointer of a tick, `Slot::pressed`, and makes a bundle fit with
  the inputs as they are, the text as long as a tick's can be,
  `wire::widest`). Scripts (`replay::SCRIPT_WORDS`): `move=X,Y`,
  `aim=X,Y`, `pointer=X,Y` (answered before the step: `Session::answer`;
  replays write `picture=`, `hud=`, `index=`, `uv=`), `text=` (`%3B`
  escapes), `erase=N`, `typed=done|cancel`, `mouse_left` and the like as
  the game binds them; `inspect` prints `"pointer"` (with `moved`).
  `input.pointer.x`, `y` (the HUD's measure, as the reading carried it
  from the start) and `moved` (elsewhere than the tick before's, or none
  then: `Input::pointer_moved`, raw places, so another level's picture or
  a thing walking under it is no move; the owner's yes, 2026-11-21; test
  `the_pointer_says_where_it_is_and_whether_it_moved`): no new word, wire
  version or replay field. The level's answers
  (`input.pointer.thing`, `part`, `area`, `at`, `cell`; `frame.pointer`'s
  `thing`, `part`, `area`; `input.aim` from the mouse, `Session::mouse_aim`):
  `src/session/pick.rs` from the picture as composed or `look`ed
  (`PickScene`: its camera and `Drawing::picks`, what `of_thing` drew;
  `figures::each_voxel` where `draw` puts each voxel, test
  `a_figures_voxels_are_picked_where_they_are_drawn`), the thing by the key
  it was drawn under (`Stage::thing_of_key`: gone, nil), a part by name,
  the area by its place, the point in sixteenths with its face; under a
  panel none (`wire::carried` keeps a pointer to what the wire carries,
  the host's bundles too); a replay writes `thing=`, `part=`, `area=`,
  `at=X,Y,Z,FACE`. Tests `src/lua/tests/pointer.rs`,
  `a_click_carries_its_answer_and_text_comes_once_over_a_bad_network`,
  `a_pointer_is_answered_quickly_on_a_big_scene`.
  Not yet: the IDE's PLAY takes no mouse or typing.
- Every game can be tried late without playing there (the user: without
  it testing a game is hardly possible): scenarios and the console are
  part of every game (`docs/game-model.md`, "Scenarios and the console").
  A scenario, `scenarios/NAME.lua`, begins a run late (`about`, `level`,
  `place`, `setup = function() ... end`, run once before the first tick,
  `require` giving the game's modules); a scenario's `level` is where the
  run begins, as if `c.start` named it, the start level never entered
  (`Game::start_scenario` names it, `Session::start_scenario` begins the
  game anew there, then its setup runs; the owner, 2026-11-14); a game
  gives its scenarios words
  of its own where its states are many (BunnyFarm's `setup.lua`). Every
  game in `games/` (the Bench aside, a load test) and every template has
  some, each starting and playing on (test
  `every_game_and_template_has_scenarios`); something new that only shows
  late in a game gets a scenario, and a test replay begun from it. The
  console is the engine's, in every Lua game, a game cannot leave it out:
  Quake's key (left of 1, `^` on a German keyboard) slides it in at the
  bottom of the window, pushing the game up; the IDE's line under
  OUTPUT, `--do`, MCP's `eval`. A scenario and every console line are
  part of the run: replays carry them (`scenario`, `evals`) and play them
  again, and the scenario's file counts for the fingerprint (checked once
  a replay has begun as its run did).
- Games are Lua (`src/lua/`, `docs/engine.md` "The Lua door"). A game's
  config is code only: `conf.lua` (`function conf(c)`), no `game.toml`
  and never a data file beside it (the user's choice); a setting takes
  effect live or after a restart (F5, `GameConfig::restart_only`: the kit,
  actions and keys, the level, markers, the player's start). The sandbox
  keeps Lua deterministic (`sandbox.rs`, `prelude.lua`, `lex.rs`; `table.sort` is the prelude's, Lua's
  quicksort without the clock, since Lua 5.4's own takes a pivot from
  `clock()`/`time()` on a badly partitioned big list, test
  `table_sort_is_luas_without_the_clock`; a global function a letter or
  two off a handler is a mistake at load, `a_misspelt_handler_is_named`;
  `level:set_voxel` outside the level is a mistake; errors from the
  engine's calls start with file:line and name the parameter,
  `sandbox::error_text`; `conf.lua` mistakes name `c.section.key`,
  `conf::culprit`; the register's `Conf` classes come from `config.rs`'s
  fields and doc comments, test `the_settings_are_what_conf_lua_sets`: a
  new setting's `///` line is its documentation; `inspect` draws every
  tick and, like `shot`, warns of hidden locals, `"warnings"`): only
  table, string, math, utf8 (and `require` for the game's own Lua files
  while it starts); maths through libm; `^` refused but for
  squares; `pairs`/`next` in key order; `math.random` from `Sim::rng`; no
  addresses in `tostring`/`format`; no `__gc`/`__mode`; an instruction
  budget and a memory limit, each stop naming the line last seen running
  (`sandbox::running_line`); a mistake inside the prelude's wrappers is
  said at the script's line under the name it called (`said_by`, test
  `a_mistake_in_a_wrapper_is_said_at_the_scripts_line`), the prelude and
  mlua's frames left out of tracebacks. The engine's objects have only
  the register's names: reading or setting another is a mistake naming
  the object and the one meant (`world::no_such`, `not_settable`; their
  `__name` the script's, `world`, not `WorldView`; test
  `a_name_an_object_lacks_is_a_mistake`); the states, `props` and the
  tables fields give stay plain tables. A table a script hands an
  engine call is read as the script sees it: a read-only view (a
  state's in `on_draw`, a global after the start, `thing.props`) as the
  table it shows, which a raw read found empty (`src/lua/given.rs`:
  `Given`, `GivenValue` for a reader's arguments and the values read out
  of them; only read, never written, kept or handed back; test
  `every_frame_method_reads_a_view_as_its_literal`). `level:spawn` outside the level
  is a mistake but for a kind with `fixed = true` (one coming in from off
  the screen). A register entry's doc is a short
  first sentence, then short sentences, a `\n` a part of its own and `- `
  a list's item (each kit's keys apart); `voxcade api`, HELP and LuaLS
  show the lines. The API is version 2 (`docs/game-model.md`,
  built in stages): the globals `world`, `level`, `player`, `input` (in
  `on_tick`), `screen` (effects: shake, flash), `controls` (the
  player's device and keys; replays carry it, `Session::set_controls`)
  and `music` (the game's own song, `music:play`, and its clock: a step
  every tick the game goes, hit stop too, `on_beat` before `on_tick`,
  `music.beat`, `music.bar`; `lua::host::Music`, in the snapshot, and in
  the hash and the fingerprint (the songs' `#tempo`, `#meter`, a
  `#loop` where one has it) of a game
  whose code names `music` or `on_beat`, `Game::uses_music`; the audio
  follows it, `Session::feed_music`, `music::Player::follow`;
  `music.intensity` the game's tension, which a song's layers follow,
  `#layer` in the MML; `music:volume` and `music:muffle`, how the song
  in play sounds (a part of its loudness; a low-pass as through a wall),
  gliding a step a tick (`music::Glide`, `Music::mix`, `Game::music_mix`):
  in the snapshot, never in the hash, a replay or a save, the audio
  gliding too (`music::Player::follow_mix`, by `Glide::key`; the title's
  and the settings' songs never hear it); `music:mix(letter, { volume,
  mute, pan } | false, seconds)` a channel's strip alike (`Mix::strips`
  behind an `Arc`, kept by letter through every song, stop and level;
  the player's `lanes`, a fixed array made with it, started as they are
  by `start_mix`; a voice's own `strip`; letters checked against a song
  file's only); a level's fade holds no song, its
  frames step the clock (`Game::music_step`; a replay recorded so says
  `fades = true` and takes the steps without a picture,
  `Session::fade_clock`), a song changed crossfades when asked
  (`music:play`'s `crossfade`, `c.audio.crossfade`; `Mixer::crossfade_music`),
  else ends with the rest of the fade silent; `music:stop(from | { from,
  fade })` silence, the player's own not coming back, `host::Music::silent`,
  `Game::music_silent`, the session noticing (song, silent) after every
  step, a stop fading 5 ms at least and however young the song,
  `Mixer::crossfade_music`; a play over a pending stop replaces it;
  music is never a kit's),
  the handlers `on_world_start`
  (no level yet), `on_level_start(how)` (build and place there),
  `on_tick`, `on_draw(frame)` (drawing on `frame` only there;
  the old `load`/`tick`/`draw` refused with their
  new names). A script's state lives only in its states, `world.state`,
  `level.state`, `player.state` and `thing.state` (plain data the engine
  copies, hashes and checks every tick; things only in `level.state` and
  `thing.state`; the engine's own numbers in them, `player.state.score`,
  `thing.state.hp`, are read and written through to where the engine
  keeps them, so hashes stay); `thing:remove` waits for the tick's end;
  levels are the files in `levels/` (`src/levels.rs`, all read at load,
  natural order, places in the files; a run begins in the first or in
  `c.start`'s, `Levels::first`, restart-only, in the fingerprint only when
  set), changed by asks at the tick's end
  (`world:go_to`, `world:restart`, `world:win`, `world:lose`; one counts,
  going on before starting over) through `on_level_end(why)` and
  `on_level_start(how)`, faded in a window only (`c.transition`, no ticks
  run, recordings skip it: `Session::ticked`; an ask's `{ fade = seconds
  }` that change's own, 0 a cut, `Ask::fade`, `game::Leaving`); holds (`level.state.held`),
  events and save games as `docs/game-model.md` has them; globals are
  frozen after
  the start (`on_world_start`, the first `on_level_start`; a mistake there
  stops the game, and the mended file starts it afresh,
  `Game::start_failed`), `on_draw` only looks; `check`
  plays a second twice from a snapshot every 5 s and names a local of
  `main.lua` that keeps state anyway (`replay::guard`, `Game::hidden`). A
  script's error halts the game (file, line, tick) until the file is
  mended; `shot`, `check` and `inspect` end with it (exit 1, `inspect`
  printing its JSON first). The kits report what their
  behaviors do to things (`Sim::events`: hits, deaths, pickups) and the
  engine tells the script after their rules, with the touches it finds
  itself (`on_hit`, `on_death`, `on_fall_out`, `on_pickup`, `on_touch`,
  `on_touch_end`, `on_interact`; `LuaGame::tell_events`); after the run's end the script
  hears nothing more. The kits move and fight; what a touch, a kill or a
  fall means is the game's code (`docs/game-model.md`, "Events"): points,
  pickups, checkpoints, a life lost, winning and losing (More Blood's
  `main.lua`, the templates'). The pins, pure Rust, keep More Blood's
  rules in Rust (`pins::Classic`, `gunships_guard`). The Lua API has one source, `src/lua/api.rs`: a field
  or method new in `src/lua/world/` (`run.rs`, `level.rs`, `thing.rs`,
  `input.rs`, `menus.rs`, `music.rs`, `screen.rs`, `frame.rs`, `controls.rs`,
  `pose.rs`; `mod.rs` `impl Stage`) goes into the register too (test
  `the_register_is_what_the_scripts_see`), then `voxcade api --lua
  lua/voxcade.lua` (test `the_editors_definitions_are_up_to_date`). Tests:
  `src/lua/tests/` (a file a topic).
- Blocks are shaped by hand only (the user rejected relief, rounding and
  primitive models made by tools as fake 3D): an imported tile is drawn
  out through its depth, every layer its picture, nothing more (a block's
  `.vvx` with glass, metal or glow is refused naming the line,
  `Block::load_in`, as a scene's). The
  editor stays calm (the user on vengi: too much, hard on the eyes): few
  tools in view, big readable words, the game's own light. Its panels are
  the IDE's look, egui through `crate::gui` (`editor/panels.rs`; the
  user's choice: of a piece with the IDE), the 3D view centred in the
  room they leave.
- Sokovox's puzzles are David W. Skinner's Microban, free to pass on
  "provided they remain properly credited" (his words): keep his name in
  the game (the HUD's last line), `games/sokovox/levels/README.txt`, and
  his file as he published it.
- Kinds of things are data (`c.things.NAME`: `behavior`, `model`, `hp`,
  the behavior's numbers; `like = "other"`), weapons too
  (`c.weapons.NAME`, the game's alone: `src/shooter/kit.toml` has the
  kit's to copy; `c.player.attacks` binds them to actions, `c.state.player`
  says what a run starts with). Markers place things by their kind's
  name (`{ marker = "rifleman" }` in a world's or prefab's palette, a
  map's `[[put]]` with `thing = "rifleman"`). A new number a
  behavior reads goes into its kind's rules (`shooter::kinds`, a default
  in the spec and in `kit.toml`, test
  `the_kits_kinds_are_the_behaviors_defaults`), not into a section of
  its own. A kind without a behavior takes the engine's keys (`model`,
  `hp`, `fixed`, `width`, `height`, `facing`, `props`, `names`, `family`,
  `effect`, `sound`, `reach`, `team`, `immune`, `rotates`, `spin`,
  `frames`, `fps`, `swing`, `persistent`) and no others: an unknown key is refused at
  load, naming the key, the table and a near name (the owner's D2, test
  `an_unknown_key_of_a_kind_is_refused`). `c.actions` holds 32 at most
  (`lua::conf::MOST_ACTIONS`). The planned `keep = true` on a kind is
  built as `persistent = true` (the owner's word, 2026-11-20; below).
  Mistakes name the table that makes them; a reload keeps things
  by their kind's name (test `a_reload_keeps_things_by_their_kind`).
- Levels that remember (`docs/game-model.md`, the owner's "ja, bau es"
  to `shots/reviews/places-draft.md`, 2026-11-20; `src/lua/memory.rs`): a
  level's file says `location = "station"`, and while the player changes
  among the levels of one location each is as it was left and they share
  one `level.state` (carried as plain data, things nil; fresh when the
  location is entered from outside; a hold goes on with it); leaving the
  location forgets that but for what is persistent, `persistent = true` in
  a level's file (all it sets down and its voxels, the whole run) or on a
  kind (its things a file sets down, any level; not the hero that becomes
  the player). A record (`memory::Record`, by the levels' names in
  `Stage::records`, each behind an `Arc` and hashed once as made) holds
  each thing the file set down by its name (`Here::filed`): taken out (or
  died), where it stood, its rules switched off, its `thing.state` with
  the engine's keys; and the changed voxels. The level is built from its
  file as ever, the record laid over it (`LuaGame::lay`), then
  `on_level_start(how)`. `world:restart()` and a level file read anew give
  it as entered this visit (`again`; in a location `level.state` as
  entered, `Stage::entered`); `go_to` the same level of a location keeps
  it; `{ fresh = true }` on `go_to`/`restart` drops the record (new, not
  seen by the owner yet); the checkpoint is still forgotten on every
  `go_to`. `level.ticks_away`: the ticks the laid record was left (0 without
  one and after a load). One form in memory, a save (`here`, `records`,
  `entered`; an older save has none and loads as before), `inspect`
  (`"records"`) and a scenario's `as_left` (new, not seen by the owner
  yet). In the hash (part `levels`) only where a game has records, so
  every other game's replays stay. The level editor's LEVEL: LOCATION and
  PERSISTENT (`edit --location NAME --persistent`); tools/vvx.py writes
  both. Tests `src/lua/tests/remember.rs`. The picture keeps a location's
  levels built (`src/render/kept.rs`, `docs/engine.md` "Levels kept
  built"): `Renderer::update_level` in place of `update_chunks` in the
  window, the IDE's PLAY and `headless`, `Session::location` (each
  level's faces by its own yaw, a level taken up only with its own); meshes, lamps'
  walls and memory put aside and swapped back, what changed since built
  anew, the location's other levels meshed ahead on a thread, the sun's
  faces' memory emptied at such a change, `KEPT_BYTES` said once when
  passed; a level of no location as before.
- The simulation only reports sounds (`Sim::sounds`: events by name); the
  engine turns them into audio (`Session::play_sounds`) so the simulation
  stays deterministic. A sound is a file of the game (the owner,
  2026-11-11; `src/audio/sound.rs`): `sounds/NAME.toml`, a few numbers a
  line, only what differs; its name is the file's (two files of one name
  are a mistake). No recordings (the owner, 2026-11-12: "nein zu wav",
  everything is code or text): a `.wav` (or `.ogg`, `.mp3` ...) in
  `sounds/`, or a sound's `file = ...`, is a plain mistake at load
  saying so (`sound::NO_RECORDINGS`), and the upgrade step "no recorded
  sounds" lists an older game's without touching them; WAV files written
  for listening (`voxcade sounds`, `music`, films) are output, not a
  game's. Its words are one register with
  MML's instruments (`src/audio/words.rs`, test
  `a_word_means_the_same_in_a_sound_and_an_instrument`): `hold` (not
  `sustain`, MML's level), `sweep` and `jump`/`jump_at` in semitones
  (the synth goes on in Hz and ratios inside, exactly). A mistake names
  the file, the line and the word; ranges are checked; a sound lasts 10
  s at most, fades over 5 ms at least, a pulse off duty 0.5 plays round
  zero (in MML's instruments too). The engine's and the kits' own
  sounds are such files, built in (`src/audio/sounds/`,
  `src/shooter/sounds/`, `src/platformer/sounds/`; test
  `the_kits_sound_files_are_their_names`); a game's file of the same
  name replaces one, `silent = true` silences it. `c.sounds` is refused
  (`voxcade sounds GAME --to-files` writes an older game's). A file in
  `sounds/` saved, new or gone while playing makes only that sound anew
  (`Session::reload_sounds`). Sounds are in neither the fingerprint nor
  the state hash. The built-in sounds are pinned as 12-bit integer
  digests (`tests/sounds.txt`, `src/audio/digest_tests.rs`): renew with
  `VOXCADE_SOUNDS=renew cargo test --release built_in_sounds` and say so
  in the commit when a sound is meant to change. Open: the sounds with
  `jump = 7.01955` (a 3:2 ratio from `sounds --to-files`) in games'
  files. The speaker (`audio::Speaker`) keeps its mixer
  behind a lock of its own that SDL's callback holds only while mixing,
  `audio::MIX_BLOCK` samples at a time from buffers made once (the audio
  thread takes no memory and gives none back, counted by the test's
  allocator, test `the_audio_thread_takes_no_memory`: a finished
  crossfade's song, `Mixer::spent`, and a sound that ended are let go of
  on the main thread as the next is handed; a song is read and made
  before the lock is taken, `Session::new_music`, then `hand_music`);
  the window and the IDE never lock SDL's audio stream, never pause its
  device, and close it on a thread of their own and leave SDL unquit at
  the end (a Bluetooth headset dropping out and PipeWire started anew
  left SDL's stream locked: BunnyFarm froze and could not be quit). The engine knows no shooter words: input is the four
  directions plus the game's actions by name (`input::Input`,
  `Game::actions`, More Blood's `shooter::ACTIONS`; keys bound by those
  names, `Settings::adopt`), sound events and behaviors are names the
  game lists (`Game::sounds`, `Game::behaviors`), markers the names of its
  kinds of things. A new shooter event needs an
  `Sfx` variant, a name, and its file in `src/shooter/sounds/` (listed in
  `shooter::SOUND_FILES`).
- The game opens on a title screen (`c.title_screen`: its own camera
  position and width, props, flyby, logo, music, `light_menu` for light
  letters over a dark scene like Sokovox's; or a logo of its own as a
  model, `logo_model`, its named parts dropping in one by one, built
  into the model bank like the letters, `ui::logo_model_parts`, test
  `a_logo_model_drops_in_part_by_part`: BunnyFarm's thick letters with
  bunny ears on the B and a carrot for the Y; `logo_bob`, the user's
  wish for it: landed letters bob gently, a smooth wave through them
  (whole voxels looked like stuttering, the user), `ui::bob`, test
  `the_logo_bobs_once_landed`, 0 unless set), or, for a story game with
  save games, a title of its own (the owner, 2026-11-14):
  `c.title_screen.level` names a level of `levels/` (never the start;
  `Levels::title`, `levels::start_in`) that the window opens in
  (`Session::begin_title`, `app::own_title`), no engine title, table or
  initials, a run's end and QUIT TO TITLE back there; a run of its own
  that counts for nothing (`Session::on_title`: `Runs::start` records
  none, no `world:save`, no pause); its code asks `world:new_game()` or
  `world:load(slot)` (save games: `world:save(slot, { label })`, `world:delete_save(slot)` at the tick's end through `Game::saving`'s `saves::Change`, `world.saves.newest` by the order saves were made in the run, `lua::host::Stage::newest_save`; `Game::anew`: the session begins the run in its
  place, `Session::new_run`, `begun` for the window to record),
  `world:open("settings")`, `world:quit()` and `world:to_title()` (in a
  run, any game: back to the title as QUIT TO TITLE, `Command::ToTitle`;
  on the title a mistake) (`game::Request`, the
  machine's, never the run's: `Session::requests`, `Front::asked`;
  `shot`/`inspect` end at a quit or `to_title`, the IDE's PLAY stops); PLAY, `shot`,
  `inspect` begin past it, `shot --screen title` shows it (tests in
  `src/lua/tests/title.rs`); the menus (`src/menu.rs`,
  drawn in `src/ui/menus.rs`) work with keyboard and gamepad alike, Escape pauses
  instead of quitting. Key bindings are places on the keyboard by their US
  names (More Blood's Z and X side by side on any layout); the menus label
  them as the keyboard in use does (`Settings::key_names`: a German
  keyboard's Y for the bindings' Z), and typed initials are the letters
  the keys say. A Lua game names them too (`controls.device`, the
  device used last; `controls:key(action)`; in a text the action in
  braces, `frame:text("{undo} UNDO")`); they come from the player's
  machine, so the window sets them before its ticks and replays write
  them down when they change (`replay::ControlsAt`; nobody plays in
  replays from before):
  Sokovox's keys show after the first step, the keyboard's or the pad's
  (the user's idea); `shot --pad xbox|nintendo|playstation` shows a
  pad's. Pausing (Escape, P, Start) gives way to a game's actions: a key
  one of them has, by default or saved before, pauses no more
  (`Bindings::pause_gives_way`, test `pausing_gives_way_to_the_game`). Player settings, the high score table and save games
  (`src/saves.rs`, `saves/<key>/N.toml`; `shot`, `check`, `inspect` and
  replays keep theirs for the run only) live in the user's config folder
  (`src/kept.rs`; `$XDG_CONFIG_HOME` or `~/.config`, macOS too for
  now; on Windows `%APPDATA%`, an older Voxcade's folder under `HOME`
  kept in use while `%APPDATA%` has none), never in the game folder.
  Everything kept of a game there goes by its key (`kept::Identity`,
  `Kept`; review round 4: a copy of Voxkeep shared Voxkeep's saves and the
  IDE's journals): a game that comes with Voxcade (its folder in the
  `games` of a package or of Voxcade's source, `GAMES_OF_VOXCADE`) keeps
  its bare folder name, so its scores survive an update and a package
  unpacked elsewhere; any other game is its folder's name and 8 signs of
  the BLAKE3 of its canonical path (`test-1a2b3c4d`). The journals and
  the MCP lock are always the folder's own. Adoption (`kept::settle`,
  before the window plays a game, the IDE opens one, `voxcade replays`):
  a game not yet in `folders/index.toml` takes what an older Voxcade kept
  under its bare name (settings, saves, kept runs, the IDE's memory,
  OUTPUT, deleted files), moved, unless a game that comes with Voxcade
  has that name; then it is in the index and adopts nothing again. A
  folder moved or renamed starts clean, its old things stay under the
  old key. Network play still names a game by its folder
  (`network::app_of`). Test `a_game_adopts_what_its_bare_name_kept_once`.
  The table is the arcade machines' (the user's wish): the ten best runs
  with three letters, lines to beat before anyone played
  (`c.high_scores`); a run that beats one after its end or QUIT TO TITLE
  picks its initials (`Page::Initials`: up/down, right/fire, left, or
  typed), then the table shows (`Page::HighScores`, the new line
  blinking); F5 and closing the window enter nothing; a run the console
  or a scenario helped goes in as GOD, no initials asked
  (`Session::helped`, `session::HELPED`; the user: whoever wants to cheat
  may, the table tells); the title's round is title, table, replay. A Lua game without a kit scores with
  `c.scored`; Sokovox gives 1000 less a point a move (100 at least) a
  level, once in a run.
  `shot` uses the defaults, so shots stay the same. SETTINGS is a short
  page of AUDIO (the volumes, the song), VIDEO (fullscreen, shadows, blur,
  shake, flashes, the frame rate), CONTROLS (a game of one's keys and
  rumble; a game of several's players page) and LANGUAGE (the user's
  wish: the one page of every setting was too full; `Page::Audio`,
  `Page::Video`). SHOW FPS (the user's
  wish, off by default) puts the frame rate small in the bottom right
  corner over game and menus (`ui::fps`). SHADOWS sets the shadow map's
  size (`settings::Shadows`: high 4096, medium 2048, low 1024; the user's
  idea, quality levels for older hardware), `shot --shadows` the same; on
  an integrated GPU they start at MEDIUM when the settings file has no
  `shadows` key (`Settings::load_for`, the owner's D5). FRAME RATE
  (UNLIMITED, VSYNC, 60, 30: `settings::FrameRate`, paced by
  `window::Pacer`). Settings are written at once and whole
  (`blocks::save_at_once`, `Settings::save`); an unreadable settings file
  is kept as `NAME.toml.broken` before defaults are saved, and a failed
  SAVE or LOAD, a settings error or an unreadable save is said on the
  screen (`parting` in `src/app/mod.rs`; the window's front end is
  `src/app/`: `App`'s steps, `front::Front` the front-end state, `keys.rs`
  `play_key` and `maker_key`, `events.rs`, `commands.rs`, `network.rs`,
  `draw.rs`; tests `pausing_gives_way_to_the_games_keys`,
  `the_title_plays_its_round_after_a_while`).
  `shot --screen` takes the names in `headless::SCREENS` (`voxcade shot
  --help` lists them). Check menus with
  `shot --screen title|continue|settings|audio|video|controls|players|player-controls|network|find|join|joining|intro|pause|host-pause|guest-pause|save|load|attract`
  (a game with a save offers CONTINUE first on the title: the newest
  save, `Slots::newest`; `continue`, `save` and `load` are refused for
  a game without slots, which has none of them,
  `a_game_without_slots_has_no_save_screen`). More Blood's title:
  giant dog tags lying in a clearing at the start (generated, the user's
  wish), stamped with the hero's name: Nick Pain (the user's choice), metal (MagicaVoxel metal material: sky reflection, highlight,
  `metal_sweep`). Big letters use `c.ui.display_font`, a glyph atlas from a
  bold typeface (`src/font.rs`); the user wanted real letter shapes, not
  the 5 x 7 font blown up. What its atlas lacks it makes
  (`DisplayFont::glyph`): ß as SS, Ç's cedilla below the line, Œ and Æ
  joined, the signs (→ « » …) blown up from the pixel font; a character
  neither font has is left out in both (test
  `the_display_font_writes_every_letter_the_pixel_font_writes`). For depth (the user's wish) they have rounded
  front edges (`c.ui.round`), uneven colour (`grain`) and, in the logo,
  cracks (`logo_cracks`, mostly in BLOOD); `ui::extrude` builds them, test
  `big_letters_are_rounded_and_cracked`. The pixel font (`ui::FONT`, 5
  wide, two rows below the line) has lower case, the letters of German
  and French and a few signs (BunnyFarm's three languages): a game's text
  is drawn as written, the engine's own in capitals as always
  (`ui::engine_case`, `text` against `written`), so older pictures stay
  the same pixel for pixel; an accented capital keeps the plain capital's
  height, in both fonts: the accent in the top rows, the letter's body
  squeezed under it (`ui::shown_row`, `font::DisplayFont::accented`; a
  character the atlas lacks stays "?"); tests `the_font_writes_german_and_french`,
  `an_accented_capital_keeps_the_height_of_the_plain_one`,
  `accented_capitals_are_their_letters_with_the_accent_over_them`. A menu
  hint wider than the screen goes on over several lines (`ui::wrap_hint`,
  test `hints_stay_within_the_screen`). Every word in capitals in the
  menus' files is in the table or named as no word (test
  `every_menu_word_has_its_german_and_french`, which reads the files).
  Still untranslated: the action names on CONTROLS (FIRE, JUMP in French;
  for 0.7, the game giving its words). A
  game speaks the languages it lists (`c.languages`; SETTINGS offers
  LANGUAGE, the system's at first in a window, the game's first in
  `shot`; `world.language`; carried with the controls in replays), and
  the engine's own menus speak them too: German and French beside English
  (`src/lang.rs`, every word by its English; a game that speaks others
  sees English).
  Shaders are WGSL compiled at run time:
  `the_shaders_are_valid` parses them in `cargo test`. The numbers the
  scene's shader shares with Rust (`MAX_LIGHTS`, `FACES`, the lanes of
  `Instance::fx`: `render::FX_GLOW`, `FX_OPACITY`, `FX_COVERED`,
  `FX_METAL`; the faces' order of `mesh::DIRS`; metal's packing; the
  lamps' and the fog's: `LAMP_ASK`, `LAMP_FROM`, `HOUSING`, the lamps'
  memory's layout, a lamp's list of boxes, the beams' planes and thin
  things) are written ahead of `shader.wgsl`, `lamps.wgsl` and
  `fog_beams.wgsl` by `render::shader::prelude` from the Rust constants,
  never a literal in the WGSL, nor defined again there (test
  `the_shader_and_the_rust_side_share_one_table`); every struct written into
  a buffer is compared with naga's layout of its WGSL struct, member by
  member (test `the_rust_structs_are_laid_out_as_the_shaders`), the vertex
  buffers with the entry points' inputs (test
  `the_vertex_inputs_are_the_rust_structs`).
- The UI is voxels too (`src/ui/`, the user's choice; `menus.rs` the
  engine's menus, `hud.rs` a game's `frame:menu` and `frame:grid`,
  `logo.rs` big letters and the logo, `panels.rs` the error and the
  console's prompt, `pixel.rs` the pixel font): a second scene
  drawn into its own layer after the world and laid over it after the
  blur, so the tilt-shift never softens it. Its camera is a one-point
  perspective with the vanishing point in the middle of the screen (the
  user's choice, it replaced an oblique view): `c.ui.slant` sets how
  strong, the plane `c.ui.depth` fills the screen exactly. The title's
  settings play the chosen song (`Menu::previews_song`). The HUD is the
  game's (the user: "we are writing an engine, not a moddable game"):
  its `on_draw` draws it (`frame.width`/`height`/`alpha`, `frame:text`
  with a size, `text_width`, `box`, `bar`, `icon` from `c.icons`,
  `drop_in`; `draw`, a model file as a grid's slot shows it, one voxel a
  UI voxel, its `frame` and `rotation` in quarter turns, no scale on the
  HUD, the owner's 2026-11-21: `menus::front`, `ui::hud_model`;
  `screen:burst`, the voxels only shown, `HudState`; what is drawn
  first keeps a shared voxel); the
  engine helps with how and where, the kits draw none. Its cubes go to
  the UI pass through `Session::hud_cubes`: covered faces hidden, the
  cubes nothing of which shows from the UI's eye left out
  (`render::leave_out_unseen`: a plate's back layer, half of BunnyFarm's
  HUD), worked out once while `on_draw` gives the same.
- Music is MML (`src/audio/music/`: `song.rs` `Song`, `Instrument`,
  `Timing`; `parse.rs` MML to `Song`, refusing an empty song, a channel
  with no notes and a non-letter channel; `player.rs` `Player`, voices,
  sidechain, bus; `mix.rs` `Glide`, `Mix`, `muffle_cutoff`; `tests.rs`,
  `loop_tests.rs`), text so the user and agents can
  write it. `#loop 33 80` (the owner's, 2026-11-20: songs with 32 bars
  of intro and outro for mixing) plays a song from its start once, then
  bars 33 to 80 round and round (counted from 1, both in; `#loop 33` to
  the end; `#loop no` once), the rest never reached; a note that sounds
  where a song goes round (its loop's end or its own) or ends is let go
  there, its release ringing on in its channel's tail (`Player::tails`,
  made with the song; a voice holds its own copy of its note) while the
  next round begins on time (the owner, 2026-11-22: for every song, it
  was cut dead; test `a_note_rings_over_the_seam`; a song played once is
  over when it has rung out); `Song::span` is exact, a fraction of a
  sample too, its ends `LOOP_SLACK` samples before the bar lines so a
  note on the line, a sum of lengths, is never a sample off (a loop of
  the song's first bars so begins a little before 0); `Player::within`
  folds a place into the song exactly; the game's clock counts on
  through it as through a whole song going round, the audio following
  the clock's beats (`Player::follow`) and going round exactly as long
  as the bars, so a one-bar loop stays within a sample of the clock for
  hours (tests `the_audio_keeps_with_the_clock_round_a_loop`,
  `a_short_loop_never_drifts_from_the_clock`); `voxcade
  music` writes such a song to the loop's end and round into its first
  four bars again, the seam in the file (`Song::once`), and says where.
  Songs from a game's data (stage 1; `shots/reviews/music-from-data-draft.md`
  and its review): a file of `c.audio.music` without a channel is a
  studio (`Song::studio`, `music::read`; `Timing::studio` its whole text,
  in the fingerprint as a part of its own, `"music/kit.mml"`), no song a
  player picks (`Session::player_songs`: the maker's keys, SETTINGS, the
  title, `voxcade music GAME`; a studio that does not read or has a
  `#loop` is a mistake at load, `song_timings`). `music:play(song, {
  channels = { K = "mml" }, from })` lays the whole set over it
  (`music::Laid`, sorted, its digest BLAKE3 of the length-prefixed
  texts; `music::lay` reads them against the studio: whole bars counted
  exactly, `parse::Beats`, no `t`, no voice); a change within the song
  that plays is `host::SongAsk` with the same song, its clock going on;
  asked the same as pending or playing, nothing is read. The hash's
  `music` part takes the digests playing and pending only where a game
  lays some. The cue (`MusicCue::laid`, `next` with its due beat) is
  compared with what the mixer reports (`Mixer::music_laid`) at every
  feed (`Session::laid_swap`): a `music::Swap` made on the main thread is
  handed ahead, called off, or swapped in at once; the player swaps at
  its unwrapped `position` (`LOOP_SLACK` early), a byte-equal channel
  untouched, a changed one's note ringing on, a channel gone ringing out
  in a tail, the old song kept in `Player::spent` for the main thread
  (`laid_tests.rs`, `src/lua/tests/laid.rs`). `music.position` the beats
  with the fraction. The studio saved: its instruments read anew, the
  laid channels read again (`Game::set_songs`), halting where they no
  longer read. `games/drums` is its test game (below).
  A song begun at a bar: `music:play(song, { at = 33 })`, its beat 0
  there (`lua::host::Music`'s `at`, in the hash only when not 1;
  `MusicCue::start`), and the engine's title's song at
  `c.title_screen.music_at` (checked as the game loads, `check_title_bar`;
  a title level of the game's own uses `at`; the settings' preview plays
  the chosen song from its start, as the game plays the player's song);
  a bar past the song's end, or after its `#loop` (which it never
  leaves), is a mistake (`Timing::bar_mistake`). MIDI comes in through `tools/midi2mml.py` and an import file
  (`games/moreblood/music/end_boss.import.toml`); the user makes music
  in Suno (Pro) and exports MIDI or audio stems. Audio stems go through
  `tools/audio2midi.py` first (the tempo must be known, e.g. from the zip's
  name); check its result by drawing the notes over the stem's spectrogram,
  not only by counts. Leave out stems that only echo others (doubles and
  bleed are typical of Suno, the user agrees): compare their onsets, pitch
  and rhythm, against a shifted copy as the chance level. The melodies get
  a C64 echo instead (`delay` in the import file), quiet: a third of the
  line's volume (two thirds sounded like the melody doubled, too much for
  the user).
  Two voices sing: `singer` (`src/audio/music/clear.rs`, the clear one,
  the owner's wish 2026-11-20: words understood on first hearing; a
  glottal pulse, five formants, loci and voice onset times, a parallel
  branch for the hiss; its name is `clear::WAVE`) and `voice`
  (`src/audio/music/voice.rs`, the first, kept to the sample: test
  `the_voice_sings_as_pinned`; `singer`'s clusters, the owner's
  2026-11-20 "Schwester" and t, g: a coda ending where the next onset
  begins, a v as Klatt's voiced fricative, no step that clicks; then
  round 4 on round 1's bursts and timing, the owner hearing grau as
  blau: a g's release long and compact, no voice bar after silence, r
  the German r, uvular, in `singer` alone; every value from a paper
  names it in a comment, the rest says it is ours (the owner: "fair play
  und kein AI slop"), tests `the_singer_sings_every_consonant_of_a_cluster`,
  `the_singer_tells_g_from_b_and_r_from_l`): a quoted text before a note is its syllable
  in phonemes (`"s ae ng" e4`, one table,
  `music::phonemes`, HELP lists it), the onset before the beat, a note
  without a text holding the vowel, `consonants` (dB) their hiss against
  the vowels; `drive` (dB into tanh, a DC blocker
  after; a voice's breath and hiss join past it, raised as it raises a
  vowel, or a drive lifted the consonants to the vowels' level and they
  hurt: the owner, 2026-11-19; the consonants as far under the vowels as
  in speech, test `consonants_lie_under_the_vowels`) and
  `crush_rate`/`crush_bits` on every wave, MML only. Every
  older song renders byte for byte as before (test
  `the_instruments_play_as_pinned`).
  The MML pulse wave is centred (no DC offset off duty 0.5, the owner's D6,
  2026-11-13; it changed the sound of End Boss slightly), so no song has
  thumps or lost headroom. MML refuses out-of-range values and names its
  own line (tempo 1 to 999, lengths 1 to 192, octaves 0 to 9, an
  instrument's options by the ranges of `words.rs`, near words suggested;
  at most 200,000 notes a channel); `decay` differs from a sound file's:
  MML's falls to the sustain level, a sound's fades to silence; a voice
  the mixer steals fades over 5 ms (`STEAL_FADE`). HELP has the topic "A
  SONG IN MML". Rebuild the release binary
  before checking a parser change with `voxcade music` (`cargo test` alone
  does not). More Blood's mood, the user's words: dark, menacing,
  atmospheric, C64 style; its four songs (Move On, Blood Rush, Storm Is
  Coming, End Boss) belong together, the user's choice; earlier songs stay
  in the repo, out of the list.
  Simple background music comes from chord progressions instead
  (`tools/compose.py`, the user's wish: Suno was much work, and C64 sound
  for half an hour "makes you want to run amok"): a recipe
  `NAME.compose.toml` (style calm, mysterious, dark, action, bright or space; key, tempo,
  moods or progressions, seed) makes the same song every time (`compose.py
  --check` writes nothing and says whether the songs there are the
  recipes'; `audio/tools_tests.rs` runs it on every recipe, and
  `midi2mml.py` on a tune made as MIDI, skipped without python3 or numpy), from
  free-midi-chords' progressions (Ludovic Drolez, MIT:
  `tools/progressions.toml` and its licence; keep the credit, in
  `music/README.md` too). Its rules against fatigue: the melody in some
  sections only (the user: "teils teils"), chords as soft pads (the
  C64 arpeggio a quiet accent in dark and action), everything low-passed,
  pulse at duty 0.5, the texture changing every 8 bars over two to three
  minutes. The library `music/` (eight songs, for the editor's package
  later) holds recipes and their songs; a game plays a copy in its own
  folder (the recipe's `out` writes both). Sokovox plays the two calm
  ones. Voxfox, a jump & run from the Sunny Land pack's Tiled map, was
  removed (the user, 2026-10-23): its level was built for planks jumped
  through from below, which voxels do not have (in 3D they block);
  Voxkeep is the jump & run kit's example.
- Every length and speed is in voxels and comes from the game's `conf.lua`
  (`c.physics`, `c.player`, `c.weapons.NAME`, `c.things.NAME`), never a
  constant in the engine (a game's own sections are its own: the
  shooter's in `shooter::config` and `shooter::kinds`): games differ in how fine their grid is (arena figures 10
  voxels tall, More Blood 22, chosen by the user: "everything in voxels, each
  game sets its own"). The figures are small and plain, the 8-bit charm in
  clean voxels: about 10 to 20 voxels tall, a few colours, every voxel
  deliberate (the user, 2026-10-23, after the templates and Voxkeep went
  to 16-bit size for a day: "the 8-bit variants had more charm", and they
  are easier to make, which is what the engine is about; even the slime,
  which looked better in 16-bit, went back, everything alike). Never
  coarse and careless: few voxels, each set on purpose. One colour a material: light and shade are the engine's, never
  painted into the colours (the user, 2026-10-23: that is not done on
  voxel figures; a painted light also stays where it was when a part
  turns). And cleanly voxelled at any size, the voxel charm (the user):
  deliberate forms (bevelled boxes, limbs of square section with their
  joints on voxel middles), mirror-alike when standing, not ragged round
  shapes. No unit conversion. Counts and times may stay
  constants. Effect speeds scale with `physics.body_width`
  (`Sim::effect_scale`). A kind's unset lengths and speeds are its
  behavior's defaults scaled from the figure they were tuned on
  (`TUNED_ON` in the kits' `kinds.rs`: the shooter's 10 voxels, Sunny
  Land's fox, opossum, frog and eagle) to the kind's own height; counts,
  times and the weapons' numbers are not scaled. `voxcade kinds` prints
  what is in effect.
- Pieces cut off from the ground fall (`drop_loose`); connectivity is over
  26 neighbours (faces, edges, corners), the user's choice, so diagonal lines
  hold. Test: `diagonal_lines_hold_together`.
- Soft voxels (`World::soft`, set by soft prefabs in a map) are drawn and
  can be blown away but never block: movement, bullets, grenades and debris
  use `World::blocks`/`World::solid`, never `get() != 0`. The user wants
  grass and shrubs you can walk through. Test:
  `soft_voxels_let_bodies_and_bullets_through`.
- A level's solid pieces (`solid = true` on a `[[thing]]` with a model)
  block in the cells their models are drawn in, kept beside the voxels
  (`world::Solids`, quarter turns only): `World::solid`, `blocks` and
  `hold` see them, `get` and the meshes do not (the model draws them, the
  palette is untouched), a blast leaves them, `thing:remove` frees them at
  the tick's end. Tests: `a_solid_piece_holds_bodies_up_and_stops_them`,
  `the_hero_stands_on_a_solid_piece`, `rounds_stop_at_a_solid_piece`.
  A kind's `solid = true` (the owner's, 2026-11-20; only `fixed` and
  without a behavior, with a model) means the same for its things: their
  cells in `Solids` by id (`World::put_solid_thing`, `ThingCells`: its
  pivot's cell and quarter turn), set, moved and freed at the tick's end
  and as a level begins (`Stage::settle_solids`, the kinds' cells
  `host::solid_cells`), a body set into one pushed out sideways
  (`host::free_beside`, `Host::shove`), a place or a moving thing the
  file sets inside one a mistake (`levels::check_solid_things`); in the
  hash only where a game has them; a level's record (`lua::memory`) is
  laid before they settle, so one moved or taken out stays so on return
  (test `a_solid_thing_a_level_remembers_blocks_where_it_was_left`).
  Tests `src/lua/tests/solid.rs`.
- Strafing aims the way the player's model looks, along it, never
  diagonally (the user: shots went off at a slant after running
  diagonally). Test: `strafing_fires_the_way_the_model_looks`.
- Shooting is one rule for the player and the enemies (`combat::aim`,
  the user: "either it is on and player and computer use it, or it is off
  and only 45 degrees"): everyone faces one of the 8 ways; with
  `c.weapons.aim_assist` (the game decides, on by default) a shot finds the
  nearest target in a narrow cone around that line (`AIM_CONE`) and heads
  straight for it, slightly off the 8 ways; off, it keeps to the line
  (rising or dipping only to a target right on it) and nothing scatters.
  Riflemen have a nervous finger (the user: rounds should fly round your
  ears): they fire as often as ever, along their nearest line towards the
  player even when he is not quite in it (the rounds are slow, he may run
  into them), and step aside onto a line meanwhile; the gunship fires its
  bursts along its nearest line too. In return for the aim assist
  More Blood's enemies take more hits (`hp` in `c.things.rifleman` and
  `c.things.grenadier`; the watchtower men are `c.things.guard`, like a
  rifleman, taking 4), and a hit that does not kill sprays blood. Tests:
  `shots_find_an_enemy_near_the_line`, `without_aim_assist_shots_keep_to_the_eight_ways`,
  `tougher_enemies_take_more_hits`, `guards_take_more_hits`,
  `a_hit_that_does_not_kill_sprays_blood`,
  `shots_reach_an_enemy_up_on_a_tower`, `a_guard_falls_with_his_tower`,
  `the_watchtower_falls_when_its_legs_are_shot`.
- More Blood starts in a calm stretch (`CALM` in the generator) out of every
  enemy's sight; widening `sight` means moving the first enemies further
  north. Test: `more_blood_starts_out_of_sight`.
- A spawner (a hut) holds two men, out one after another, and only while
  it lies ahead of the player: nobody spawns next to or behind him (the
  user: no shots in the back from a fresh spawn). Test:
  `a_hut_sends_a_few_men_and_none_behind_the_player`.
- Generated structures must stay connected voxel by voxel (a gap in a brace
  leaves pieces hanging in the air after a collapse), and markers need room
  for a whole body or the engine lifts the actor onto the roof (a spawn off
  the level's ground is lifted to the level's top + 8 today; a mistake for
  it is planned for 0.7).
- The camera is fixed per game and always perspective (`conf.lua`: pitch,
  fov; an orthographic view looked flat and was rejected). How much it
  shows: `view_width` across, or `view_height` high (Pulsar), or
  `view_at_least = { 320, 200 }` both ways, more as the window's shape
  gives (BunnyFarm; the user's measure: at least what a 320 by 200 game
  showed; test `a_view_at_least_shows_that_much_in_any_window`).
  `c.camera.stop_at_edges` stops the view at the level's edges, a side
  view's (Voxkeep) and a view from above (BunnyFarm's meadow: the ground
  the screen shows stays on the level, `camera::ground_extent`; test
  `a_view_from_above_stops_at_the_level_edges`). Meshes only
  contain faces that some view ray or the sun can hit (`mesh::face_mask`
  over `camera::corner_rays`, a window up to `camera::WIDEST_KEPT`, 21:9;
  a wider picture adds the faces its edges see to `seen_faces` and the
  meshes are built anew once, test
  `a_window_wider_than_the_faces_were_chosen_for_adds_its_faces`); if you
  change camera or sun handling, rebuild all meshes. A game sets its camera per picture (`frame:camera` in
  `on_draw`: `look_at`, `width`/`height`, `pitch`, `fov`, no yaw yet;
  `session::PictureCamera`, `Viewing`; only the picture): moved or zoomed
  it looks along the same rays (no more faces); another pitch or fov
  adds its rays' faces from then on (`Session::seen_faces`, the window,
  `shot` and the IDE rebuild the meshes once when `face_mask` grows), and
  a game whose code calls it leaves models out of view only once the
  camera is known (`Drawing::cull`). `c.camera.aspect` draws the picture
  into the largest rectangle of that shape, black bars round it
  (`Renderer::set_shape`, `render::letterbox`; the callers pass
  `Session::picture_aspect`). A game sets its daylight per picture (`frame:daylight` in
  `on_draw`: the sun's way and colour, ambient, shadow, sky; the rest
  `c.light`'s, `session::Daylight`); a game whose code names `daylight`
  keeps every face a sun above the horizon can light
  (`mesh::face_mask_any_sun`, `Game::moves_sun`; a reload that changes it
  rebuilds the meshes), and its shadows are fitted to the last picture's
  sun, then to this one's. The shadow map is fitted to the view (`camera::light_view_proj`)
  and, seen from above, never wider than the world as the sun sees it (test
  `the_sun_is_fitted_to_the_level_not_to_the_horizon`); the shadow pass takes
  only the opaque cubes (`FrameData::shadow_cubes`); the map is snapped to whole texels so shadows stay still while scrolling. A
  game that moves its sun turns the map every picture, which lays its
  texels anew: at a shadow's edge a face, decided at its middle, went
  from lit to shaded and back (the user saw it in BunnyFarm). Its faces
  remember their shadow (`FrameData::remember`, the shader's
  `remembered`: a table of faces in a storage buffer, by where a face is
  and the way it looks, not by pixel, so a face stays whole): how much in
  the sun lately and whether lit, changing only a margin past the
  middle, so a face changes once as the edge passes and the shadows move
  with the sun face by face. Only the pipelines of a game that moves its
  sun carry it (`render::plain_shader` takes the shader's MEMORY marks out
  for every other pipeline, `render::Memory` is made the first time a
  frame asks, a GPU without fragment storage writes says so and draws
  plain; test `a_moving_sun_turns_each_face_once`; the scene pass of a
  game that does not move its sun is 10 to 30 % quicker). Only faces at an edge make a memory; a clear
  face looks whether it has one and lets it fade. Tried and dropped: the
  sun in steps of 0.4, 1.5 or 3 degrees (flickered less, or jumped; the
  user: "3 degree jumps are not ok"), the map's texels laid on the
  ground (raised faces still flickered), two maps of fixed suns blended
  (a face is lit or not, so every face between them changed at once
  halfway: a jump moved in time). Measured with `film` frame by frame at
  three times of BunnyFarm's morning: no face goes back and forth; one
  or two faces change a picture. It costs the small GPU (Radeon 610M)
  about 2.5 ms (test `a_moving_sun_has_the_faces_remember_their_shadow`).
- Lamps (0.7, the owner's names; `docs/engine.md` "Light in the grid",
  `docs/game-model.md`): a light's `falloff`, `cone`, `blend`,
  `rotation`, `pitch`, `shadows` (a level's `light = {...}`,
  `frame:light{ ... }`; `render::LightShape`, `LIGHT_WORDS` the ranges,
  `Light::lamp`; the kits' flashes `Light::burst`, through walls as ever),
  `c.light.lamp_shadows`; `shadows` takes words, `"static"` (walls
  stop it), `"all"` (moving things too: `render::movers`, each model and
  opaque cube drawn a solid of its own, its voxels as bits in its own
  grid and the map into it as the shader places them (`movers::Solid`),
  gathered in view or not where an "all" lamp reaches,
  `Drawing::movers`, made every picture; walked afresh after the walls'
  kept answer, each solid in its own grid, exactly, only those of the
  boxes the lamp can light, `Movers::lists`, a beam's ray only those
  that can shade it, `shading`; a face's way begins exactly at its
  middle `FACE_OUT` out, a turned face's written by `fs_main_lamps`,
  `movers_from`, a face turned from the lamp dark, `movers_stop_face`;
  `HOUSING`; a light at a thing, `Light::of` (`frame:light`'s `at` a
  thing; `Drawing::of_thing` keys what is drawn, `Stage::drawn_key`), is
  not stopped by that thing's own solids, its faces lit as by
  `"static"`, test `a_lamp_a_thing_carries_is_not_stopped_by_it`; a
  part turning slowly shades each face once, test
  `a_part_turning_slowly_shades_each_face_once` (until 2026-11-19 a
  second bit volume, cells filled by their middles and asked from the
  cell in front of a face: turned faces blinked); tests
  `a_moving_thing_stops_a_lamp_whose_shadows_are_all`,
  `a_post_of_turned_cubes_stops_a_lamp_whose_shadows_are_all`,
  `what_moves_out_of_view_still_stops_a_lamp_whose_shadows_are_all`) or `"none"`
  (`render::LampShadows`; `true`/`false` are refused naming the word to
  write). `c.light.lamp_shadows` is the game's word for every
  light that leaves `shadows` unset (`"static"` unless set; unset is
  carried as `None` in `LightShape`, `ThingLight` and `Light` until
  `Light::stated` lays the game's word over it); a light's own word
  beats the game's (game `"none"` and a light's `"static"`: the walls
  stop it; test `the_games_word_is_the_default_and_a_lights_own_word_beats_it`).
  The editor's SHADOWS row shows an unset light's inherited word quiet,
  a light's own in the accent; a click on the lit own word clears it. Walls stop a lamp (the level's opaque voxels
  and its pieces but those bringing a light, `Drawing::piece`;
  `FrameData::lamp_world`, `lamp_pieces`); a lamp in solid voxels walks
  from the nearest open voxel's middle within `volume::HOUSING` (8),
  settled once a picture on the CPU (`Volume::start`, a column of its
  row, `LampRow`; buried deeper it lights nothing), so a lamp flush with
  a wall lights its own side only (test
  `a_lamp_flush_with_a_wall_lights_its_own_side_only`). A walk climbs a
  copy only once out of the cell above the one it left and is bounded
  by its cells, past them stopped, never clear (test
  `a_way_grazing_a_long_surface_still_meets_the_wall`). A spot's tiles,
  the lamps' and its beams', are its cone's (`Light::bounds`). A
  picture with up to `FORWARD_LIGHTS`
  (16) lights, none stopped, is drawn as before by the scene's shader
  alone, byte for byte; else the lamps' pass (`render::lamps`, tiles of
  32 pixels, a face's answer kept, `lamps.wgsl`; `light.wgsl` the sum both
  share), up to `MOST_LIGHTS` (4096). Only the picture: test
  `lamps_change_no_run`. A lamp new to the faces' memory (moved) is asked
  for first by a pixel every `lamps::ASK` (`fs_lamp_ask`), so a face's
  pixels do not each walk (test
  `a_lamp_new_to_the_faces_memory_is_asked_for_first`); the walls
  changing while drawn (a wall taken away, set again) light and stop as
  a fresh renderer draws them (test
  `a_wall_taken_away_and_set_again_lets_a_lamp_through_and_stops_it`). The upgrade step "lamps stopped by walls"
  (0.7.0) says which lights of a game and how to keep the old look; it
  changes nothing. Moving things stop no `"static"` lamp.
- Fog (0.7, the owner's names, the classic smooth way; `docs/engine.md`
  "Fog and beams"): `c.light.fog_density` (0 none unless set),
  `fog_color`, `fog_anisotropy`, `fog_glow` (0 to 20, 1: what the fog
  sends on to the eye of the lamps' light, times it, the dimming the
  density's alone; above 1 capped at the brightest light the lamp gives
  along the ray, so a higher glow is never fainter: test
  `a_higher_glow_never_makes_a_beam_fainter`), `frame:fog{
  density, color, anisotropy, glow }` per picture, a light's `fog` (0 to
  10, 1; `LIGHT_WORDS`, a level's
  light and the level editor's FOG row too). Air in the level's box, not
  between camera and level: lamps' beams in it, walls stopping them as
  their light, what lies further faded to its colour. A picture with fog
  goes through the lamps' pass; after its see-through cubes the beams
  (`render::fog`, `fog_beams.wgsl`: points on planes across the camera's
  way fixed in the level, the spot's core edges, halvings where a wall's
  shadow begins, walls asked voxel by voxel, the faces' walk and memory;
  no noise, so no flicker
  scrolling or turning; where a ray can pass the shadow of a thin thing,
  a post or a rail under 4 voxels thin of the level's voxels or pieces,
  `render::thin`, its points as far apart as it is thin, at most 16 more
  a ray a lamp, so its shadow shows however thin along the ray, at FOG
  HIGH only (`Fog::thin`; LOW draws the beams without that work, as
  before thin things, `Stats::thin`), test
  `a_thin_rail_darkens_the_beam_behind_it`; a picture none of whose
  lamps lights one draws with a pipeline without that work, as before;
  the lamps' memory's slots by a mixed key and a second key's step,
  `stopped`, or its odd slots crowded and said it full at 42 in 100)
  into a
  target half or a quarter the size, then the composite's own pipeline
  (`fog_composite.wgsl`) lays it under the UI; a picture without fog is
  drawn byte for byte as before, at no cost. VIDEO's FOG (`settings::Fog`:
  HIGH half, LOW quarter, OFF none drawn; LOW on the processor's own
  graphics until chosen; `shot --fog`). Only the picture: test
  `fog_changes_no_run`; GPU test `fog_shows_a_beam_the_wall_stops`.
  Glass and water: with see-through cubes on screen their depth is drawn
  first (`glass_pass`), the beams before and behind the nearer one apart,
  the fog behind it laid over the scene before they are drawn
  (`fog_far.wgsl`), so they tint it (test
  `a_beam_behind_glass_is_seen_through_it`); without them none of it runs.
  Not built: the sun's beams, moving things cutting beams, fog in the
  editors and the IDE's picture.
- The renderer is `src/render/`: `mod.rs` the `Renderer` and `Instance`,
  `bank.rs` the model bank, `baked.rs` its runs baked into faces and the
  bank on the GPU, `cubes.rs` the CPU passes over a frame's
  cubes (`hide_inner_faces`, `join_see_through`), `frame.rs` `FrameData`,
  `shader.rs` the shader text and its uniforms, `pipelines.rs` the
  renderer made, `chunks.rs` the chunk meshes, `passes.rs` a frame drawn,
  `limits.rs` the faces' memory's count, `lamps.rs` the lamps' pass,
  `volume.rs` the level's solid voxels it walks, `thin.rs` the thin
  things among them, `fog.rs` the fog's beams and its composite. `Renderer::render` is
  `write_globals`, `upload_cubes`, `upload_models`, `draw_models`,
  `shadow_pass`, `scene_pass` (or with lamps `lit_scene_pass`,
  `lamp_pass`, `see_through_pass`, with fog `beams_pass`), `ui_pass`,
  `post_passes`, `composite_pass`. A limit a frame goes past is said once, never silent:
  lights past `MOST_LIGHTS` (`Session::warnings`, counted in `look` too),
  the lamps' limits (a GPU without fragment storage writes, lamps
  reaching past `volume::MOST_BYTES`, their memory full past
  `lamps::MOST_FACES` (2^26, 512 MB) or the GPU's largest buffer: `lamps.rs`,
  a quarter of the picture's pixels to begin with, `faces_for`, made twice
  as large when crowded below `CROWDED_UP_TO` answers a pixel, past that
  when an answer found no room, `Renderer::too_small`, read every
  `GROWN_EVERY` pictures, `shot` letting it grow until it fits before it
  times and saying its size; every device asks the adapter for that
  much, never past its limits, `render::device_limits`; everything of the
  lamps let go after `lamps::LET_GO` pictures without their pass,
  `Lamps::used`; what the GPU cannot give them, their memory, walls or
  the moving things' room, `lamps::made`, kept as it was or drawn
  without, said once, and a pass with walls that would not build said
  too (a test fails); tests
  `the_lamps_memory_grows_until_the_beams_fit`,
  `the_lamps_memory_starts_small_and_is_let_go`,
  `what_the_gpu_cannot_give_the_lamps_is_said_and_drawn_without`,
  `the_device_holds_the_lamps_memory_as_far_as_the_gpu_allows`),
  the faces' memory full (the shader counts faces that found no room in
  the word after it, `render::limits`, read without waiting every 60
  pictures, at once by `shot`; `Renderer::warnings`), the model bank past
  the GPU's largest buffer (`baked::plan`: faces past it drawn as cubes,
  cubes past it not at all; test
  `what_the_gpu_cannot_hold_falls_back_and_is_said`); `shot` prints
  "warning: ...", `inspect` lists those `look` finds under "warnings" (it
  draws no picture on the GPU, so not the GPU's), the IDE's PLAY
  puts them in OUTPUT (test `lights_past_the_limit_are_said_once`).
  Models are baked (`c.effects.bake_models`, on unless the game says;
  live, the bank made anew; only the picture, never in the fingerprint):
  `ModelBank::bake` lists each run's faces that are drawn
  (`render::baked::Face`, 12 bytes against a cube's 32: the cell from the
  pivot in 9 bits each, the way, lit, first; colour and glow; shade and
  metal), by their own way, on all cores in the runs' order, kept with the
  model (`VoxModel::baked`, `Part::baked`: a bank made anew, grown by
  `bank_fresh` or after a level change bakes only models new to it; test
  `a_bank_made_anew_bakes_only_what_is_new`). Every pass draws a baked
  run's faces as the unit cube's own, its four corners by the first
  face's triangles (`vs_face`, `vs_face_shadow`; the prelude's
  `CUBE_CORNERS`), the outline each voxel's grown cube from its first face
  (`vs_face_outline`); the scene leaves out the ways no face of a draw on
  the axes can show the eye (`baked::facing`), the shadow pass none. The
  same picture (test `baked_models_draw_the_picture_their_cubes_draw`;
  faces in another order than the cubes' may win a tie of depth the other
  way: one pixel of two million, by 5 levels, in the owner's big scene),
  that scene 17 to 5.3 ms of GPU (135 to 39 on the Radeon 610M), its
  bank 77 to 37 MB, BunnyFarm 15.3 to 11.9 ms on the 610M. Cubes still:
  glass and fading models, `frame:cube`s, the logo's letters, a run a
  face cannot carry (a voxel past 512 by 512 by 512 from the pivot) or
  whose faces would take more memory than its cubes (thin: a post, a
  voxel alone), the editors' views (their banks are never baked,
  `Shown`). Static models meshed together is 0.7's.
- Rendering goes scene (HDR colour + glow targets) -> quarter-size blur
  passes -> composite onto the surface (`src/post.wgsl`; a window takes
  a surface format that stores what it is given, `render::surface_format`,
  else an sRGB one whose composite decodes once, `fs_composite_srgb`, or
  the colours were brightened twice). Instances are
  opaque first, then see-through ones sorted far to near
  (`FrameData::see_through`); see-through cubes write no depth and cast no
  shadow. See-through cubes that touch, flush in neighbouring cells,
  draw as one body (`render::join_see_through`, in `Voxels::finish`):
  the faces between them go, but for a skin a layer (the user's idea,
  2026-11-11: several skins on top of each other, not one), so water
  made of `frame:cube`s is no grid of darker stripes and still shows
  how deep it is. The layers lie across the body where it is clearly
  thin at that cube (water lies flat, glass may stand: the shortest run
  of see-through cubes, half the next or less), else across the way the
  view looks most (a block, Voxkeep's gem); of a seam's two faces the
  one towards the eye stays. For everything see-through, a model's
  glass too. Actor models get see-through voxels from MagicaVoxel's glass
  (`_trans`) or blend (`_alpha`) material on a colour (`vox::see_through`,
  carried as the colour's alpha); the Huey's cockpit is the example (the
  user's idea). Glass with a roughness (`_rough`) mirrors and glints (the
  user wanted a reflection on Voxkeep's gem): uncoloured, Fresnel's 4 %
  head on and much at a glancing look, the sun and the lights about
  glinting by the slant of the surface where the voxel lies
  (`VoxModel::facets`, carried in a cube's shade direction with no faces
  lit by it), not by its faces; packed as metal with metalness 0
  (`vox::metal::pack`). Glass without `_rough` mirrors nothing, as
  before. Effects are set per game in `c.effects`; the user wants them
  ("light effects, blur and other enhancers are fine"), but the voxels
  themselves stay on the grid. Prefer effects that respect the grid: the
  user chose voxel-exact shadows (`c.light.shadow_style = "voxel"`, each
  face lit or shaded as a whole) over smooth 3D-engine shadows ("looks so
  much more retro"). Models can be lit like pixel art instead of face by
  face (the user's idea, `c.light.model_shading`, `vox::VoxModel::shade`):
  "edges" lights a whole voxel by its surface's direction and keeps hard
  edges hard; fine one-voxel relief (the dog tags' letters) and thin
  machinery (the Huey) blur with it, so they stay "faces"
  (`model_shading_for`). Models only so far, not the world's chunks.
  A game may draw a dark line round its models as round a sprite
  (`c.effects.outline`, voxels thick, 0 none by default; `outline_color`):
  their voxels drawn again grown, far faces only, pushed back along the
  view rays (`vs_model_outline`, `vs_face_outline`), so it shows at the
  silhouette and where a part stands well before another, not on every
  relief step.
  Rounded heads for More Blood's 22-voxel figures were tried and rejected
  (the user: "the old figures were prettier"): a 5-voxel head has too few
  voxels to round, the rounding pays off on bigger models. Tests:
  `smooth_shading_keeps_hard_edges`, `more_blood_shades_its_figures_smooth`.
- Camera yaw stays 0 unless the user asks for it: quarter turns only. A
  level may turn it (the owner's, 2026-11-20: a level seen from its other
  side): `[camera] yaw` in its file (`grid::LevelCamera`, read and written
  by `grid::text`, a quarter turn, its line in a mistake; a view from the
  side, `camera::from_above` false, or a kit whose rules walk across the
  screen, `Kit::sees_from_the_side`, by halves only: 90 and 270 refused,
  `levels::quarter_mistake`, `checks::check_yaws`), the level's world's
  `World::yaw`, so the fingerprint (the level file), snapshots and every
  machine have it; `Sim::yaw`, `yaw_sin_cos` (exact for a level's),
  `Session::camera` (`c.camera` with it: the picture, `face_mask`, the
  title, `look`), `level.camera.yaw`. The meshes' faces are the level's
  (the window, `shot` and the IDE compare `face_mask` every picture and
  build anew once on entering a level turned otherwise); the side view's
  shadow fit looks the camera's way (`camera::light_view_proj`); sounds
  pan by the screen's right (`Session::screen_right`); the title's flyby
  crosses its screen. The shooter's keys turn by it, the jump & run's
  left and right swap seen from behind (`read_keys`' `behind`), the hold
  swaps across and deep at 90 and 270; `input` stays the keys, raw in
  replays, a Lua game turns them itself (HELP's "A LEVEL SEEN FROM
  BEHIND"). Ways stated stay the world's (a kind's `facing`, a file's
  `rotation`, places, pieces: a piece without one faces south); a thing
  whose way nothing states rests facing the camera (`Sim::rest_way`,
  `Sim::body` at every spawn of the kits and the plain host; the level
  editor's `Figures::key_in` alike; tests in `src/lua/tests/camera.rs`,
  `the_shooters_keys_turn_with_a_level_seen_from_behind`, the stillness
  tests at 180 too). The level editor's LEVEL has YAW, GAME VIEW looks
  that way. Levels of one location may look different ways: the record
  of one left brings back a thing's way over its rest facing (test
  `levels_of_one_location_turned_two_ways`). Not built: a picture's own
  yaw (`frame:camera`), 90 and 270 from the side.
- World voxels are palette indices (0 empty). Moving things are instanced
  cubes built in `Session::compose` and the game's `Game::draw` (through
  `session::Drawing`), on the voxel grid by default: one cell each,
  quarter turns (a kind's `spin`, degrees a second, turns its model round
  its upright axis as it is drawn: Voxkeep's gem; `session::spinning`, the
  picture only). A game may turn models freely, its choice (the user's
  decision 2026-10-22; it was Voxatron's quarter-turns-only rule:
  Minecraft lives from it, and turning parts is the quick way to
  animation): a kind's `rotates = "free"` faces exactly its way,
  `frame:model`'s rotation (degrees round, pitch, roll) and
  `Drawing::model_turned` any way, about the pivot cell's middle at half
  the model's height (`render::turned_centre`, the shader's `rotate`,
  `ModelDraw::rot` and `shift`, `Instance::turn`); unturned models draw
  exactly as before (tests `a_free_quarter_turn_is_the_quarter_turn`,
  `models_from_the_bank_match_their_cubes`,
  `a_game_turns_its_models_freely`). Figures of parts (the user's wish:
  animation by turning parts): a `.vvx` of `[[part]]`s is one
  (`vox::Part`; a MagicaVoxel file's named objects come in so; else its
  models are frames); the biggest part
  is the root, each other hangs on the part it touches most and turns
  about the middle of where they meet (`parts_of`); a kind's `swing`
  swings parts with its steps (`session::swinging`), `frame:model`'s pose
  turns them by name (`Drawing::model_posed`, `part_chain`); a part may
  rest turned (`rotation`, `pitch`, `roll` on its `[[part]]`, the root's
  too: `vox::Part::rest`, the owner, 2026-11-12: "die grund figur muss
  nicht im voxelraster sitzen"), every pose turning on from it (`pose ·
  rest` about the joint; `session::rest_turn`, `part_at_rest` for the
  editors), only the picture (the simulation, solid cells, bursts and
  `visible` see the parts unturned; test
  `a_part_turned_at_rest_draws_as_the_same_pose`); a part a draw
  in the bank; posed, a part's depth is taken a little nearer the eye the
  further out it hangs (`ModelDraw::nearer`, the shader's `PART_NEARER`),
  so a bent limb's faces in one plane with its parent's do not flicker.
  Only the picture: the simulation sees the whole frame
  (tests `a_figure_hangs_each_part_on_what_it_touches`,
  `a_figures_parts_turn_about_their_joints`). Nothing is scaled: every
  voxel the same size. With `snap` on, positions are whole voxels and the
  camera's x/y sit on the drawn player's voxel (`Session::view_target`, the body `Game::focus` names). Never
  round player and camera separately (the player flickers while running) and
  never let a snapped camera lag behind (it catches up in hitches after every
  stop). The camera height never changes (the user's choice): it
  keeps the height it starts with, whatever the player climbs or jumps;
  coupling it to the player shakes the whole view. Tests:
  `running_player_keeps_still_on_screen`, `stopping_stops_the_view`,
  `jumping_keeps_the_view_still`.
- Interpolate anything that gets drawn with `session::between` (or
  `Drawing::between`), never
  `Vec3::lerp`: glam's lerp is inexact when both ends are equal, and flooring
  the result to the voxel grid then makes resting things flicker a voxel
  (test: `standing_on_the_plaza_draws_the_same_frame`).
- wgpu is 30.x and glam 0.34: camera matrices come from `glam::camera::rh`
  (`proj::directx` matches wgpu's depth range), `queue.present(frame)` presents.
  Culling tests corners against clip planes with w, never `project_point3`
  (perspective puts chunks behind the eye).
  Check the crate sources in `~/.cargo/registry` rather than relying on memory
  of older APIs.
- Grid cells as keys go in `world::CellMap`/`CellSet` (a fast fixed hash):
  the standard hash costs most of a lookup and walks in a different order
  every run, which made the renderer's chunk order and so `shot` differ
  between runs. Models draw only their visible voxels
  (`VoxModel::visible`: open space or glass beside them; the simulation
  still bursts bodies into all of them), and a frame skips models outside the
  view and the shadow box (`Drawing::model`). Their opaque voxels lie on
  the GPU once (`render::ModelBank`, `Session::rebuild_bank`): a frame
  sends one `ModelDraw` per model (where, quarter turn, flash) and the
  shader's `vs_model` places the voxels (`vs_face` a baked run's faces)
  as `ModelBank::cubes` does (test
  `models_from_the_bank_match_their_cubes`), the faces another voxel
  covers left out, in the shadow pass too (`vs_model_shadow`: BunnyFarm's
  shadows 11.5 to 5.5 ms on the small GPU at HIGH); glass and fading models stay
  cubes built every frame. The title logo's letters lie there too
  (`ui::logo_letters`; the game builds the bank anew when a model, the
  font or `conf.lua` changes, `Session::rebuild_bank`; test
  `the_logo_from_the_bank_matches_its_cubes`), and the title's props find
  their ground once (`Session::place_props`). No model is registered (the
  owner, 2026-11-14: "ich gebe den ort an wo das model geladen wird -
  wieso muss es irgendwo angemeldet werden?"; `c.models` is refused, the
  upgrade step "no list of models" takes it out): `frame:model` draws
  any model file of the game by its path through the session's `files`
  (`session::Needed`, sealed files too); the kinds', items', effects' and
  title's models are kept the whole run (`Models::actors`, `props`),
  every other model is loaded when needed: a level's pieces' as it is
  entered (`Session::enter_level`, on `Game::level_mark` moving on, on
  all cores), the rest the first time it is drawn (drawn as cubes, put
  in the bank, `Session::bank_fresh`, and the picture made again), and
  let go at the next level change, out of the bank (rebuilt whole; a fade
  is when it comes). What the simulation reads of a model never depends
  on it: the kept ones (`Models::get`, `shape`) and the levels' pieces'
  frames, heights and parts, read once as the game loads
  (`Models::pieces`); a model only drawn is no part of the fingerprint
  (test `a_model_only_drawn_never_changes_the_run`). A model file saved
  or new comes in at the next draw, an `on_draw` stopped for want of it
  draws again (`Game::models_came`). Where two things share a cell, the one drawn
  first keeps it at equal depth: the world, then models, then cubes.
  Changed chunks are meshed
  on all cores (`Renderer::remesh_all`) and uploaded in the order given,
  so every run builds the same. Work in the simulation may go to several
  cores only through `par::in_order` (results in the order of the input)
  and only where it reads; writes, random numbers and anything whose order
  matters stay on one thread. `par::cores()` caches the core count. Runs of
  reads go through `World::reader` (a chunk looked up once for its
  neighbours). Tests:
  `a_model_draws_only_what_can_be_seen`, `models_out_of_view_are_not_built`.
- Hot reload polls the game folder and its subfolders (`src/watch.rs`) on
  purpose: the user's system has no inotify watches to spare. It tells
  what changed in the order of the paths (test
  `changed_files_are_told_in_order`). A map level
  rebuilds when any file it was made from changes (`Session::level_files`);
  a level made in code (`Game::level`) when `conf.lua` changes, which
  also reloads the models (kinds may have come, gone or changed theirs).
- Big levels are maps (`src/map.rs`, the user's choice): ground from a
  height map and a material map, `.vvx` prefabs placed in `map.toml`. Never
  go back to one huge world for them (More Blood's was 90 MB, over
  GitHub's file limit). A prefab's anchor is the middle cell of its bottom
  layer (size / 2); `rotation` is degrees clockwise seen from above, in
  quarter turns (as a level's thing's). Test:
  `a_map_builds_ground_prefabs_and_markers`.
- A figure of parts in the voxel editor (`editor/figure.rs`; asked by
  a game of the owner's, the user's plan 2026-11-12) is shaped whole, never part by
  part in a view of its own (the user: "kein fan von parts separat
  bearbeiten"): the tree of parts on the right (indented by parent,
  warnings in words: touches nothing, could as well hang on another), the
  chosen part drawn as it is and the rest see-through, the joints as
  rings, the tools working on the chosen part where it stands. Parts are
  models of their own that overlap (legs inside a body, a neck in a
  chest): a tool never takes or refuses a cell another part holds, and
  what flickers when built so is its maker's to mend by a slight turn in
  the pose. PARENT and JOINT set by hand are kept in the `.vvx`, the
  part's `parent` and `joint` (from the pivot, halves allowed;
  `vox::parts_of`: stated wins, unset as worked out before, test
  `a_figure_without_a_rig_hangs_as_before`). A figure saved stays a
  figure, each part where it stood (`ModelFile::parts`, its `at` from the
  pivot; a part grown past the box grows `size` about the pivot, a part
  under the floor raises every part). A part moves as a piece in a level
  does (the owner: "ich will objekte verschieben"; no mode to switch):
  its arrows at its middle, heads only, or POSITION, by whole voxels, one
  step a drag; what hangs on it moves along, hand-set joints too
  (`Editor::move_part`; the root moves the whole figure). The joint is a
  grip of its own, the ring with a line to the part's middle: dragged,
  it goes across the view by halves (the axis most along the view left
  to the JOINT field; `grab_joint`), tests
  `a_part_moves_with_what_hangs_on_it`,
  `a_joint_is_dragged_by_its_ring_in_one_step`. `voxcade edit GAME
  --model FILE --shot OUT [--part NAME --parent NAME --joint X,Y,Z
  --move-part NAME --by X,Y,Z --move-joint NAME --by X,Y,Z --tool box
  --held X,Y,Z --stamp --view YAW,PITCH]` to look. SLICE cuts a block,
  a frame or a figure as a scan does (the owner's CT scan): a plane on
  each axis, each on or off on its own, a voxel cut by any of them
  neither drawn nor touched (`editor/slice.rs`, `Slices`); a plane's
  side is the eye's as it is switched on and stays until its FLIP (it
  no longer follows the camera: with three planes that confused); a
  row a plane, its LAYER, FLIP, OFF; Page Up/Down the plane switched on
  last; `--slice z,12 --slice x,20,flip`). FLOOR beside GRID (lit by
  default, kept in `editors.toml`, `Backgrounds::floor`; `--no-floor`):
  off, the plate under a block, model or figure is neither drawn nor
  met (`model::pick_over`): rays go on under the grid, so a model is
  worked on from below, and ADD on empty space sets nothing. Glass is
  glass in the voxel editor (the owner: opaque, one could not see how
  it would look): a frame's or part's glass colors are see-through
  cubes beside the opaque world (`Shown::glass`, `view::glass_cubes`,
  joined as the game's), the figure whole's carry metal and facets as
  the game's; the tools still meet them. EDIT on a piece shapes its
  model in place, the level's other pieces faint round it as they stand
  (`editor/context.rs`, CONTEXT, `--context LEVEL:NAME`); `--clear` and
  `--fill` change a model's cells without the window, written as the
  editor writes (`ModelFile::change_cells`; docs/tiles.md). The parts'
  list begins with the figure whole (the state it opens in): every part
  solid, the tools working on the part they meet while it stays chosen
  (`Editor::begin_whole_stroke`); a part is chosen by its row, a double
  click in the view or SELECT in its menu, never by a click that paints.
  A part rotates at rest about its joint (MOVE | ROTATE at it, its
  rings by 5°, Ctrl by 1°; the ROTATE row; `rotation`/`pitch`/`roll` in
  its `[[part]]`): the view looks through the chosen part's rest
  (`View::rest`, `Editor::rest_view`), so the tools, the slice and its
  handles work in its own grid and the rest of the figure is drawn
  turned round it (`Figure::rest_map`, `session::part_at_rest`). Parts
  as objects (`editor/model/parts.rs`; a model of one frame gets them
  too, the owner: "die hat man ja selten von Anfang an": + PART at
  FRAMES, ADD PART in a frame's menu or NEW PART on a selection makes
  it a figure in the same step, `parts::made_figure`, its frame the
  root part named as the file or `body`, every voxel where it stood,
  UNDO the one frame byte for byte; several frames, an animation, are
  refused, `Editor::cannot_have_parts`): ADD PART (one voxel of the color held,
  on top of the part chosen or the figure, touching it, its name to
  type), REMOVE (what hung on it hangs on its parent where it stands;
  not the root while parts hang on it), MERGE INTO its parent or WITH…
  another (the target's voxels win; a part rotated by quarter turns of
  the other merges as it shows, `merge_map`), DUPLICATE, MIRROR (a pair
  `NAME_left`/`NAME_right`, or `_l`/`_r`: the partner made or made anew
  across the figure's middle, voxels, place, joint, rest rotation; with
  SYMMETRY a stroke on one lands on the other, `land_on_partner`),
  RENAME (F2, a word), HIDE, HIDE OTHERS, SHOW ALL (always at the PARTS
  heading, quiet when nothing is hidden); each in the row's menu, the
  part's menu in the view and its bar, refused ones quiet with why on
  hover (`zones::Entry`). No step re-hangs or re-hinges a part it did
  not act on (`Editor::pin_rig`: such a part's parent and joint are
  stated in the file first; test `no_step_rehangs_a_part_it_did_not_act_on`;
  a part acted on acts on its source and every clone of that);
  the JOINT row says the file's numbers (from the pivot). Clones
  (`like`, the owner's 0.6 feature): a clone's frame holds its source's
  mirrored, as big, and the file keeps the link (never its voxels);
  every step and stroke ends in `ModelFile::follow_links` (what changed
  in a clone laid on its source through the mirror, every clone made
  anew standing where it stood: `at` is where the clone's box stands,
  so a source grown on its low side moves an unmirrored clone's `at`
  and a mirrored one's not, or the other way on the high side), live in a
  stroke (`Editor::follow_links`, `history.linked`); CLONE and CLONE
  MIRRORED beside DUPLICATE (`clone_part`, a pair's partner's name),
  LIKE (`link_part`, a source's list `possible_sources`; a part with
  clones cannot become one) with MAKE UNIQUE, MIRROR X Y Z
  (`set_link_mirror`), the row's quiet source name; REMOVE or MERGE of
  a source hands down to its first clone (`ModelFile::hand_down`, the
  others' mirrors put together; two axes: made unique), a clone merged
  into or away is made unique, RENAME rewrites the `like`s; TRIM of a
  clone trims its source; SYMMETRY's partner that is the same voxels
  takes nothing twice (`land_on_partner`); `--clone NAME [--mirrored]`,
  `--make-unique NAME`, `--like NAME=SOURCE`; tests in
  `editor/model/link_tests.rs`. An empty part in the editor
  (`Placed::empty`, a frame of one voxel left empty): its own sign in
  the tree (`zones::Sort::Empty`), never warned of as loose, nor what
  hangs on it; + EMPTY beside + PART on a line under PARTS (and under
  FRAMES), ADD EMPTY in the menus (`Editor::add_empty`, named `group`):
  on the part chosen at its joint, with the figure whole the new root at
  the pivot, the old root hanging on it as it stood (a model of one frame
  becomes a figure so); the tools set nothing in it
  (`Editor::shaping_empty`), CLONE, LIKE, MIRROR, MERGE quiet with why,
  an empty root has JOINT and its ring; the last part with voxels is not
  removed; `--add-empty [NAME]`; tests `an_empty_part_becomes_the_root_and_nothing_rehangs`,
  `an_empty_part_on_a_part_holds_no_voxels`. The parts' hide marks
  stand in one column at the left edge, the rows indented after it: the
  figure whole outermost, the root one step in under it (test
  `the_tree_reads_as_a_tree_and_an_empty_part_takes_no_voxel`). POSE (the top
  bar): the rings and the POSE row turn parts on top of their rest,
  nothing written (`Editor::pose`, `Figure::pose`, `session::part_posed`),
  the pose as Lua for `frame:model` with COPY and RESET; off, the rest
  is drawn and the pose kept until RESET. GAME VIEW (the top bar): the
  figure whole from the game's angle (yaw, pitch, fov) but framed to
  fill the view, not at the game's distance (the owner: a speck one
  cannot zoom into), shaded, lit and outlined as the game draws it, from
  a model bank (`session::model_drawn`), posed while POSE is lit, only
  looked at; the wheel, the corner buttons and the right button zoom
  and turn it and it stays on (`View::leave_game_view`: a level's GAME
  VIEW, the game's camera, ends), pressed again the editor's look as it
  was, on again the game's angle afresh. A number field never widens
  (`zones::number`: its room exactly, a long number in the small type,
  too long whole; test `a_wide_number_never_moves_the_inspector`); the
  bar at the chosen one stands clear of its arrows and rings alike.
  `--rotate-part NAME --by DEG [--about rotation|pitch|roll]`,
  `--pose-part NAME --by DEG [--about ...]`, `--handles rotate`,
  `--add-part`, `--merge-into NAME`, `--mirror`, `--game-view`,
  `--move-part NAME --by X,Y,Z` (tenths into `offset`), `--menu
  part:NAME` in a shot. SELECT (L; `editor/model/selection.rs`; the owner
  2026-11-13: select 2D on the screen) in a block, a frame, a part or
  across the parts of the figure whole: a rectangle dragged on the
  screen (an outline while dragging, from anywhere in the view) takes
  the voxels whose drawn middles it holds, of the active parts only (the
  part chosen alone; with the figure whole every part shown; never the
  context room or what SLICE cuts away), THROUGH (lit, the default)
  every one behind it, off only those seen (by cells of the screen a
  voxel big: the nearest there); NEW | ADD | REMOVE beside the tools
  (Ctrl: ADD, Alt: REMOVE while dragging), CLEAR, Escape; a click: the
  voxel seen there, on empty space (NEW) none; a double click the color
  connected; outlined (`Editor::select_rect`, `View::marquee`). A part's
  row dragged onto another's hangs it there where it stands
  (`Editor::reparent`: its rest rotation, `at`, `offset` and joint made
  anew so it draws the same, `parts::euler`; refused on itself, its own
  descendant, the root and the figure whole's row, said); a drag of a
  row does nothing else. At it its arrows (MOVE, whole voxels, from where
  the drag began, Alt keeping what they land on; a part's or frame's box
  grows, a block's keeps its size; no bar at it), SELECTION at the
  inspector's foot (nothing above it moves) and its menu: DUPLICATE (the
  next move leaves the voxels), REMOVE (Del), PAINT, FLIP X/Y/Z, ROTATE
  X/Y/Z, NEW PART (hanging on the part most of it came from; a model
  of one frame becomes a figure) and in a figure MOVE TO PART…, COPY and PASTE (Ctrl+C, Ctrl+V: where it
  lay); each one step. FILL (W): the connected voxels of the color
  clicked, Ctrl every one of it. A swatch's right-click menu: REPLACE
  WITH HELD, REMOVE (a model's unused color).
  The selection is part of UNDO and REDO (the owner, 2026-11-15): a
  change of it alone (the rectangle let go, a double click, CLEAR,
  Escape, a click on empty space) is a step that writes nothing
  (`Step::Selected`, `Editor::selecting`), and every other step keeps
  the selection as it found and left it (the newest 16,
  `History::selections`, `Editor::seal`), so REMOVE taken back is
  selected again; test `selecting_is_a_step_to_take_back`.
  `--select X0,Y0,Z0 --to
  X1,Y1,Z1 [--selection remove|paint|duplicate|flip-x|rotate-z|new-part|move-to:PART|copy|paste|move:X,Y,Z]`,
  `--menu selection`, `--tool select|fill` in a shot; in a figure whole
  a part turned at rest is selected where the file has its voxels. A bar
  at an object (both editors, `zones::bar_at`) that comes under the
  pointer takes no click until the pointer moved 4 points or 300 ms
  passed (test `a_new_bar_takes_no_click_at_once`): a double click never
  lands on the REMOVE it brought. The root part shows no PARENT or JOINT.
  HELP has THE LEVEL EDITOR and THE VOXEL EDITOR (`ide/help.rs`, a line
  `# WORD` a heading). BACKGROUND (a swatch at the right of both top
  bars; the owner: black on black): the view's clear color and the
  floor's tones (`editor/background.rs`: four to choose, the first the
  game's sky as always, and a picker), the marks over the view deeper on
  a light one (`Marks`, test `the_marks_read_on_every_preset`); kept for
  the person in `config_dir()/ide/settings/editors.toml`, one for each
  editor, never in a game; GAME VIEW and the IDE's pictures keep their
  own; `--shot` the dark one unless `--background #rrggbb`.
  F1 (and Esc) shows the keys of what is open on one page
  (`editor/keys.rs`, the owner's D8): the buttons' keys with their words,
  the scope's own, those everywhere, the mouse's; each small button's
  tooltip ends with its key; `--shot --menu keys`. UNDO and REDO are
  Ctrl+Z and Ctrl+Y as everywhere (`zones::main_keys`: their tooltips and
  F1 say those; U and R still work, unsaid on the buttons). GRID (beside
  the view's corner buttons, lit when the voxel editor opens; never in
  GAME VIEW or a level): faint lines along the voxels' edges on the faces
  the eye sees (light ones on a dark voxel, `cell_lines::dark`, test
  `dark_voxels_are_ruled_light`), where a voxel spans 7 points or more
  (`editor/cell_lines.rs`, egui lines over the view, an edge drawn where
  its middle is seen; kept while the view and the voxels stay; not over a
  figure whose parts are turned or posed). A level's blocks are pictured
  from above and the front at once, as the level is seen
  (`view::above_front`: a level seen from above its tops whole, else its
  fronts), so grass, a path and a pond differ; among the pieces each row
  ends in its place, quiet (two gems told apart). ROTATE's fields run X,
  Y, Z as POSITION's, in the handles' colors: pitch, roll, rotation (the
  file's words, on hover); a new piece faces the camera, rotation 180
  (north 0). A dark swatch keeps a light edge (`shaping::swatch_edge`).
  Undo keeps at most `KEPT_STEPS` steps and `KEPT_BYTES` bytes of them
  (each holds the level or model before and after), the oldest going
  (`History::bound`; `Editor::steps` still counts them); test
  `undo_keeps_a_bounded_number_of_steps_and_bytes`. `--joint` sets a
  joint from the pivot, as the file and the printed tree have it (test
  `the_joint_flag_is_from_the_pivot`). A kind's `facing` is
  how the editor draws it too (`view::start_way`), the title's logo opens
  from its y-0 side (`Face::Logo`), a joint set by hand is never LOOSE.
  In the voxel editor the level round a model (CONTEXT) and a selection's
  outline are kept between frames and made again only when what they
  are made of changes (`Shown::room`, `selection_edges_kept`); a stroke
  sets only its voxels (`Shown::retouch`); `editor/big.rs` has the timing
  tests for a 169x12x116 wall, and for a figure like the owner's car
  (`car_text`: a big part with a mirrored clone, glass, a wheel with
  three clones; `big_figure_timings`; tests
  `a_figures_parts_round_the_chosen_one_are_joined_once`,
  `a_big_figure_with_clones_is_quick_to_shape`): the see-through parts
  round the one chosen are joined once and again only when they change
  (`Shown::join`), the clones follow a voxel without listing the source's
  cells unless a clone was stroked (`ModelFile::follow_links`), a figure
  is made over `world::DenseCells`, a stroke's let-go takes the rig the
  stroke began with (`pin_rig`) and makes nothing when nothing changed.
  Still slow on such a figure (53,000 voxels): a stroke whose clones
  follow makes the figure anew at every voxel (5 ms) and finds its
  see-through body again (6 ms; a frame of 8 to 17 ms), a step saves the
  whole text (6 ms), a let-go, UNDO or a part moved takes 15 to 25 ms.
- A figure's box is never larger than its parts need (the owner's
  `size` stuck at 2001 after a part flew off): every step's end leaves
  `size` the tightest about the pivot (`ModelFile::tight_size`; a drag's
  at its let-go); opening changes nothing, TRIM (the figure's SIZE row,
  a part's own SIZE row, a model of frames' MODEL row; quiet "ALREADY
  TIGHT") does it at once; a model of frames' box and a part's own box
  grow by strokes and shrink only by TRIM; a level's block keeps N×N×N.
  Test `a_figures_box_shrinks_back_and_trim_makes_it_tight`.
- A part's `offset` (`[[part]]` `offset = [x, y, z]`, within ±0.5, one
  line after `at`, only when not 0; refused on a frame; `tools/vvx.py`
  too): drawn moved after its rest rotation, its joint and what hangs on
  it along; only the picture (`session::part_chain`). A pose entry takes
  it too, `{ bust = { pitch = 10, offset = { 0, 1, 0 } } }`
  (`session::PartPose`, `PoseEntry`). In the editor Shift is the fine
  step on every handle: arrows by tenths (whole voxels into `at`, the
  rest into `offset`, folded within ±0.5: `Editor::move_part_by`), rings
  by 1°, the joint ring by tenths; never PICK while a handle is dragged.
  POSITION shows and takes tenths (in POSE it sets the pose's offset).
  MOVE at a part turned as it stands: LOCAL (its own axes after the
  whole chain, the default) | GLOBAL (the figure's) at its bar; a
  rotated part's worked-out joint is stated before it moves, so it turns
  about the same point. A level's piece turned off the axes has the same
  switch (it lands on whole voxels). Choosing a part never turns the
  view: the camera and the figure stay, the chosen part (turned) drawn
  as cubes where it stands, its tools' marks with it, the tools' rays
  taken into its grid (`View::view_proj` of the block; `view::frame`
  draws with the camera alone; no ALIGN VIEW built). Tests
  `a_part_keeps_its_offset`, `a_part_moves_by_tenths_into_its_offset`,
  `local_moves_along_the_parts_own_axes`,
  `choosing_a_turned_part_keeps_the_view`.
- The window counts a button as held only if it went down in its view
  (`View::held`): a motion carrying a button held from elsewhere (a press
  on a panel, another window, a release lost, the remote pointer's stale
  state) drags nothing and turns the camera nowhere; a lost release lets
  egui's button go too (`Input::release_buttons`). A row chosen again
  stays chosen. A figure opens framed by its voxels, not its stated box.
  `pin_rig` never rewrites a joint the file states. An `EditorWindow`
  runs without a window for tests (`EditorWindow::headless`: its events
  fed, its frames run, nothing drawn); tests
  `clicks_on_the_rows_choose_and_change_nothing`,
  `pointer_trim_and_a_local_move_change_only_their_own`.
- Where things are in the editor (the `Editor`'s state is in sub-structs:
  `ed.history`, `ed.forms`, `ed.pieces`, `ed.rigging`, `ed.selecting`):
  `editor/model/tools.rs` `Editor::shift_pick`, `end_drag`, `release`; `rig.rs`
  `follow_figure`, `move_part`, `move_part_by`, `grab_joint`, `rest_view`,
  `pin_rig`; `forms.rs` BOX and SPHERE; `history.rs` undo and redo;
  `shot.rs` `voxcade edit --shot` (`ShotOptions`, `shot` and its flags'
  steps: `choose_part`, `shape_parts`, `try_pieces`, `set_view`,
  `pointer_flags`, `print_facts`) and the windowless writes;
  `window.rs` `EditorWindow` (`handle` by the event's kind: `key_down`,
  `Here::motion`, `Here::left_down`, `Here::right_up`; `draw`);
  `cell_lines.rs` GRID;
  `cells.rs`; `model_file.rs`; `picture.rs` `view::frame`; `shown.rs`
  `Shown::update_land`; `figures.rs` `Figures`; `actions.rs` `act`;
  `aim.rs`; `marks.rs`. A level's GAME VIEW looks where the game's camera
  looks as a run starts (`view::GameStart`, `session::within`: the place
  `start`, else the hero, else `c.player.spawn`; `stop_at_edges`); lights
  a game sets with `frame:light` it cannot know. A figure's GAME VIEW
  faces by `Face::look` (the logo unturned, from the front). A quiet axis
  marker sits in the view's corner (`panels::axis_marker`); TERRAIN shows
  every point faintly. ADD MODEL marks a model that does not read with "!"
  and says why on hover (`Thumbs::model`). `edit --shot` prints joints from
  the pivot (`Figure::tree_lines`, `Figure::file_joint`). A stroke on a
  figure's part remakes the figure at let-go (`Figure::only_chosen_voxels_changed`,
  `rigging.made_figure` via `pin_rig`).
- POINTER (the owner kept setting voxels with a click meant to choose a
  part): the first tool and the one every editor opens with, Escape
  coming back to it; it changes nothing: a click chooses a part (the
  figure whole, or another part seen through; empty space: the figure
  whole), in a level of blocks it holds the block clicked; handles and
  the menu work. Test `the_pointer_changes_nothing`.
- A drag is one step or none (the owner lost a part to a drag whose
  let-go never came): a press while one is open ends it first as let go
  (`Editor::end_drag`, its step made); the focus lost, the window hidden,
  another button or Escape cancel it (all as before, no step); the left
  button found up for 200 ms while one is open ends it as let go
  (`EditorWindow::draw` asks SDL's global mouse state; not for the remote
  channel's pointer), and a release heard by another window ends it too
  (`mouse_up_elsewhere`, the IDE's routing and `voxcade edit`'s). A
  handle moves nothing until the pointer has gone `handles::TRAVEL`
  pixels; a drag's amount is finite and at most `handles::MOST`, a part
  goes no further than `model::FAR` from the pivot. Tests
  `a_drag_never_let_go_cleanly_is_ended_or_undone`,
  `a_part_drag_never_let_go_cleanly_is_ended_or_undone`.
- A level of pieces (`grid::text`, `editor/model/room.rs`; asked by a game of
  the owner's, 2026-11-12): a level's `[[thing]]` can be a piece (a
  `.vvx` set down anywhere, fixed, named for the code:
  `level:thing(name)`, kind "piece"), bring a light or be a light alone
  (`thing.state.light` its brightness), beside `[[place]]` and `[[area]]`
  (`level.areas.NAME`, `area:holds`); a level needs no blocks. Pieces
  are things, not voxels: they hold nobody up. A piece with
  `shadow_only = true` (the owner, 2026-11-14: a ceiling of voxels that
  does not block the view but blocks the sun) casts as it would drawn
  and is not drawn (`ModelDraw::shadow_only`: the shadow pass only;
  `Drawing::shadow_only`, its box raising the shadows' fit above the
  world, `cast_top`), no light with it (refused), solid only with `solid
  = true`; the editor draws it as a ghost and casts it
  (`FrameData::cast_only`, `editor::view::GHOST`), SHADOW ONLY in its
  rows, GAME VIEW and the IDE's picture casting only (`View::ghosts`);
  tests `a_piece_only_casting_a_shadow_is_not_drawn`,
  `a_shadow_only_ceiling_shades_the_room_and_is_not_drawn`. `c.camera.rotates =
  "free"` lets things turn off the quarter turns; a game on the grid
  names such things, and the editor asks before opening their level
  (ALIGN ALL, DON'T OPEN: the user's wording). The level editor shapes
  it without code: the list on the right, ADD MODEL from the game's
  models, drag over the floor, X/Y/Z rows, ROTATE, COPY, REMOVE, RENAME,
  EDIT MODEL, a light tuned by looking, ADD PLACE, ADD AREA, GAME VIEW;
  every step saved, a change touching only its lines of the file
  (`grid::text::rewrite`, comments kept). A click chooses what the ray
  meets first voxel by voxel (`editor/pick.rs`: a small piece in a big
  one's hollow before the big one; boxes only for lights, places,
  areas; a piece only casting a shadow, a ghost, only where nothing
  drawn lies behind it), ADD sets down on the face met; the outliner's
  right edge is dragged wider, up to half the window (kept for the
  person in `editors.toml`); the mark at a row's left hides
  a thing in the editor only (HIDE, HIDE OTHERS, SHOW ALL; nothing
  written), the padlock beside it (on a group's row, a locked row and the chosen one) LOCKs it (a click in the view passes
  through, `hit_along`; its row still chooses it; nothing written). A
  `[[thing]]`'s `group = "forest"` (the editor's alone: no Lua name, not
  hashed; its line in the file's bytes as any) lists it under a group's
  row in the outliner (`editor/model/groups.rs`, `stage::things_listed`:
  where its first thing lies), which folds, hides and locks its things;
  GROUP in a thing's rows, RENAME and UNGROUP one step each; `edit --shot
  --group NAME --fold G --lock G`. A thing of a kind without a behavior takes `facing`/`rotation`
  in the file as a piece does (it starts so in the game:
  `Stage::name_things`; a kind with a behavior is refused,
  `check_facings`; `rotates = "free"` kinds exactly, others the nearest
  way), ROTATE and EDIT (its kind's model) in the editor. The words a
  person reads: the terms of LÖVE, Blender, Godot, Unity, MagicaVoxel
  (ROTATE, RADIUS, COLOR, POSITION; one set of tools in both editors,
  ADD, ERASE, PAINT, PICK), American English; the inspector's X/Y/Z
  fields on three equal columns, numbers right-aligned (`zones::col`).
  `voxcade edit GAME --level FILE --shot OUT [--choose NAME --add MODEL
  --at X,Y,Z --hide NAME --click X,Y ...]` to look; the terrain with
  `--terrain [--raise X,Y --by N --radius R] [--shape X,Y step]`.
- An editor never writes an older state over a newer file (the level
  editor once wrote its level as it had opened it over an agent's
  changes, the owner's lost work, 2026-11-12; `editor/model/disk.rs`): it
  remembers each file it holds (the level, its blocks, the model) by a
  BLAKE3 of what it last read or wrote, and looks once a second, when
  its window gets the focus, and right before every write. A file
  changed outside is read anew (what was chosen kept by name, a drag let
  go of, the steps before dropped, a line over the view says so); a step
  about to be written over a changed file is laid on the file as it is
  now (things, places, areas and parts by name, cells and voxels one by
  one, the file's other text kept), or, where it no longer fits, not
  written. A file gone is written anew by the next step. The editors
  save every step, so nothing is held unsaved but a drag (let go of);
  only a write that failed leaves something, and then a change on disk
  asks KEEP MINE or TAKE THE FILE'S. A model saved keeps each unchanged
  part's or frame's text byte for byte (`frames::keep_text`).

## Firepit inbox

At the start of a session, read any pending messages in `.firepit/inbox/*.md` — cross-project notes Firepit routes here. Act on each, then mark it done with the `firepit_inbox_complete` MCP tool, passing the message's filename as the `id`.

## Firepit knowledge

Before researching something that may already be known, query the knowledge base with the `firepit_knowledge_search` MCP tool (scope `both` covers this project plus the global base). Save durable findings with `firepit_knowledge_add` — written in English, per the indexing convention. The created markdown files live under `.firepit/knowledge/` and are committed like any other file.

## Firepit pinned knowledge

@.firepit/knowledge-pinned.md

The import above auto-loads the knowledge docs marked `pin: true` in their frontmatter — always-on rules that apply every session without a search. Firepit regenerates the file from the pinned docs; don't edit it directly. Pin/unpin via the pinned flag on `firepit_knowledge_add` / `firepit_knowledge_update`, and keep the pinned set small — everything else stays reachable through `firepit_knowledge_search`.

## Firepit conventions

<!-- claude-firepit-fragments -->

@../.firepit/projects/claude.md

The two imports above are shared files in the Firepit central repo — edit them there and every project follows. They carry policy; the tools themselves are described by Firepit's MCP server at the handshake, so nothing is duplicated between the two.

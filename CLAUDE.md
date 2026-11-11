# Voxcade

Voxel arcade engine in Rust (wgpu + SDL3). Read `docs/concept.md` for the
design decisions and `docs/engine.md` for the engine's architecture and plan
(every classic 2D genre in voxels, games in Lua or Rust, an IDE for Lua
games in phase 2: `docs/ide.md`) before changing anything architectural.
The Lua API grows by the game model in `docs/game-model.md` (world,
level, player, things; everything that changes in a state; `on_` events;
levels, items, save games; the voxcade-api skill): read it before adding
any name to it.

Build what the user asks for. Effects, features or behaviour they did not ask
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
"The game API"): `src/game.rs` the `Game` trait, `src/session.rs`
`Session<G>` (a loaded game folder, its title, music, hot reload, replays'
fingerprint, the frame, `snapshot`/`restore`), `src/sim/` the simulation's
core every game shares (`Sim`; `sim::things`: things in one list with
ids, kinds from `c.things.NAME` with `like`, parts), `src/cli.rs` the
command line any game gets. `src/lua/` is the Lua door (`LuaGame<K>`: the
sandbox, the world API over `lua::host::Host`, `conf.lua`, errors that
halt; `api.rs` the API's register), `src/mcp.rs` the MCP server.
`src/files.rs` is a game's files as the engine reads them (`Files`: the
folder, or bytes laid over it; `Store` by BLAKE3; `docs/multiplayer.md`
stage 6): every loader reads through the session's `files`, never the
disk itself, so a game can play sealed files while its folder changes.
`src/net/` is network play in lockstep on a LAN (`docs/multiplayer.md`,
stage 4): `Host` and `Guest` state machines without I/O over `wire`'s
datagrams, `SimNet` a simulated network for the tests, `soak` its stub
game, `play` a session played by bundles (`Hosting`, `Joining`,
`Locals`), `udp` the socket, `quic` iroh across the internet (a thread
of its own, channels; stage 6), `link` either as one transport (`Where`,
machines asking in let in by a spoken code), `trial` iroh's measures
(`voxcade nettest`), `codes_wire` the game codes' words (six signs for a
ticket, a copy of matchmaker's protocol); `src/network.rs` the window's
side of it. Our relay and the game codes are `matchmaker`, a project of
its own beside this one (`../matchmaker`, to be public as
SACRVM/matchmaker, the user's: "mach ein extra Projekt draus"), run on
our server, a TLS proxy in front.
`src/shooter/` is a shooter kit, building blocks for many shooters (More
Blood, its first game, was a prototype): `Shooter`, its behaviours
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
voxel blocks (`src/blocks.rs`, `src/grid.rs`; `src/tiled.rs` brings a
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

- Build: `cargo build --release`
- Play: `cargo run --release -- run games/arena` (opens a window; the user plays it, you don't)
- See a frame: `cargo run --release -- shot games/arena --frames N --input "keys:ticks;..." --out shots/<name>.png`,
  then Read the PNG. Keys: the directions `left right up down`, the game's actions
  (More Blood: `jump fire grenade strafe`) and `none`, joined with `+`.
- Regenerate a game's level and models: `python3 tools/gen_arena.py`,
  `python3 tools/gen_moreblood.py` (numpy; writes More Blood's map.json,
  ground PNGs, prefabs and actors)
- Play More Blood: `cargo run --release -- run games/moreblood`; Sokovox:
  `-- run games/sokovox` (its models: `python3 tools/gen_sokovox.py`; a
  `shot --input "next:1;none:1;..."` reaches any level); Voxkeep (a
  castle at night, six rooms in
  `levels/`, a demo of going from level to level and of the jump & run
  kit's puzzles: bats to time, ferries over spikes, a lever and a lift,
  slabs out of the floor, a crate on a plate holding a hatch open; the
  door opens once every gem is taken and leads on; `main.lua`'s LINKS
  says which lever works what; its replay `all_six_rooms` plays through): `-- run games/voxkeep` (blocks,
  figures and the rooms first made by `python3 tools/gen_voxkeep.py`,
  which now only makes what is
  missing: the files there are the user's, shaped in the editors; never
  regenerate over them); Pulsar (a space shooter scrolling up,
  in Lua alone): `-- run games/pulsar` (models, stars, asteroids and
  stations from `python3 tools/gen_pulsar.py`; its song from
  `music/pulsar_drive.compose.toml`; keys the arrows, fire, bomb; two
can fly, `c.players = 2`: a second player joins with START on a pad or
Return at the keyboard, its ship `ship2.vox` in gold and green, each
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
- The Lua API: `voxcade api [NAME]` lists it; after changing it,
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
  `config_dir()/ide/<game>.lock`, the user's alone). The IDE's MCP button
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
  desktop: say so).
- Levels of voxel blocks (`docs/tiles.md`): `voxcade import <game>
  <tiled-map.json>` brings a Tiled map in once (Tiled draws in 2D: it is
  only a way in); `voxcade edit <game>` opens the editor in a window (the
  user's), `--shot FILE [--block NAME | --things] [--view YAW,PITCH]`
  draws it without one, to look at.
- The IDE (`src/ide/`, `docs/ide.md`; phase 2, built in stages):
  `voxcade ide <game>` opens it in a window (the user's), `--shot FILE
  [--open PATH] [--console CMD] [--size WxH]` draws it without one
  (`--console`: a command run in the console and waited for). egui 0.37 on the
  engine's window and GPU (`src/gui/`, shared with the editors: `input.rs`
  the glue from SDL3, `fonts.rs`, the style, the painter, the list row), one
  monospace typeface in two sizes (the user's choice: the retro feel),
  calm colours. On the left the logic and each sort of asset under a
  heading that folds (arrow on the right, one level only), the whole row
  clicked; in the main area the one file open (no tabs). What was open
  and how far scrolled, the cursor in each text, live in the config
  folder (`ide/<game>.toml`).
  Below the main area the console: the user's shell in the game folder
  (`ide/terminal.rs` over `alacritty_terminal`, `ide/console.rs` the
  panel), CLAUDE starts Claude Code as the IDE's settings say (the gear
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
  other `.vox` the voxel editor
  (`editor::frames::ModelFile`: a model's frames as boxes of voxels,
  `blocks::Block` of any size; `voxcade edit <game> --model FILE`; in a
  block or a model BOX and SPHERE besides the voxel tools, the user's
  idea so people build as the agents' scripts do: a footprint dragged on
  a face's plane, + and - its depth, a click sets it (Shift: only where
  empty, Ctrl: cut), then it stays on the mouse to stamp, Escape lets go;
  BEVEL cuts a box's edges, SYMMETRY mirrors every tool left to right;
  `editor::model::form_cells`, `--shot --tool box --held X,Y,Z`);
  `editor::EditorWindow` takes its window's events (`ide::route`). A
  double click in the list or EDIT above the file opens one; the main
  area shows it as a picture (`ide/preview.rs`, the editor's view
  without panels, turned by dragging), read afresh when the IDE's window
  is focused again. The logic's files are changed in the main area
  (`ide/code.rs`: coloured by `lua::lex::spans`, of which `tokens`, the
  fingerprint, is made, the same byte for byte, test
  `highlighting_keeps_the_fingerprint`; Lua reads the text after every
  change and marks the line it cannot; the API's names offered after
  `world:` and the like, from `lua::api`), saved a second after the last
  change (`ide/buffer.rs`, a journal in the config folder against
  crashes; a change on disk comes in, or the IDE asks whose to keep).
  PLAY plays the game in the main area (`ide/play.rs`, no title or
  menus; a file saved comes in at once, and where the game would now
  begin otherwise (`Session::start_mark`: the world's voxels and the
  states as a fresh load has them, or a restart-only setting) the fresh
  game takes over with a line in OUTPUT, so what an agent builds in
  `on_level_start` shows without F5; a change to the rules alone keeps
  the run; Escape back to the file, F5 over, F9 keeps the run, STOP; runs
  recorded as in `voxcade run`; a script's error with
  TO THE LINE, the line marked in the file until mended); `--shot
  --play TICKS` draws it (with `--open` the file over the game held).
  The scenarios under a heading of their own below the logic, always
  there (NEW SCENARIO a file to fill in), PLAY SCENARIO over one plays
  the game from it (F5 begins it again); while the game plays, a line
  under OUTPUT runs Lua in it (the console), its answer in OUTPUT.
  Songs play from the main area (PLAY SONG). Reading the keys
  (`controls::read_input`), the speaker (`audio::speaker`) and the
  player's options (`Settings::options`) are shared with `voxcade run`.
  BUILD packs the program and the game into `builds/<game>-<system>.zip`
  (`ide/export.rs`, git-ignored; no hidden files, no `replays/tests/`).
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
  in a row) and `-- music <game>` (the song; or `-- music FILE.mml`, one
  song) write WAVs to `shots/`. You
  cannot listen: check them as a spectrogram (numpy + PIL), and the
  printed lengths, peaks and channel-length warnings. `shot` prints which
  sounds a run made.

- Measure a frame: `shot` prints `frame: cpu ... | gpu ...` (building the
  frame, and the GPU per pass) and `ticks: ... slowest ...` (simulation
  and chunk remeshing per tick: blasts); `--at x,y` starts the player
  anywhere. Load test at 16-bit sprite scale:
  `python3 tools/gen_bench.py`, then `shot games/bench --screen title`.
  Numbers and the plan for the 16-bit scale: `docs/engine.md`.
- Builds for other machines: GitHub
  Actions `Builds` (`.github/workflows/builds.yml`), started by hand
  (`gh workflow run builds.yml`) or by a `v*` tag, never on every push
  (macOS minutes count ten times on the private repo): Linux and Windows
  (macOS Intel only started by hand with it), SDL3 built in (`--features
  static-sdl`); `tools/package.py` puts the folder together (the games
  but the Bench, `dist/README.txt` and `LIESMICH.txt`, players first; on
  Linux `start-voxcade.sh`; `THIRD-PARTY-LICENSES.txt` from
  `tools/third_party_licenses.py`: the licences of every crate linked,
  SDL, Lua and egui's fonts). The program started without anything opens
  the launcher (`ide/projects.rs`, `StartPage`: the games beside the
  program and the user's own, PLAY in a window of its own, EDIT in the
  IDE, NEW GAME, the line about new versions; each game a card with
  its picture, `cover.png` in its folder, 480 by 270, a shot of the
  game; without one the card shows its name; of the games beside the
  program only those with a cover are listed, and the package leaves
  the Arena and the Bench out: only the games we show, the user). New versions
  (`src/update.rs`, `voxcade update [--install]`): the feed is the
  website's `release.toml` (the version, each system's package and its
  SHA-256; `VOXCADE_UPDATE_FEED` another, a file's path too), nothing
  fetched but the feed until UPDATE NOW; the package is taken only with
  the feed's checksum and swapped in where the program lies, what was
  there going to `before-update/`, the games folder game by game. The icon
  (`tools/make_icon.py`: `src/icon.png`, `dist/voxcade.ico`) is on every
  window (`src/desktop.rs`), in the Windows program (`build.rs`), and on
  Linux the launcher keeps a desktop entry and the icon in
  `~/.local/share` (a Wayland desktop takes a window's icon from the
  entry named as its app id, `voxcade`). A
  mistake at the start of a window is said in a window in plain words
  (`cli::plainly`, English, German, French). Fetch builds with `gh run
  download`. A `v*` tag makes a release of them here; the public
  downloads are the same files as releases of the website's repo
  (`../voxca.de`, its README says how,
  with the blog post that goes with each version). Apple silicon later.
- Replays (`src/replay.rs`): `voxcade check <game>` plays every replay in
  the game's `replays/` folder without a window, drawing the game after
  every tick as a window does (`Session::look`: the kits' things and
  `on_draw`, no picture; a mistake in `on_draw` stops it as in a window,
  test `check_draws_every_tick`), and says whether each runs as recorded
  (`replays/tests/` holds scripted regression runs made with
  `shot --input ... --record FILE`; `shot --replay FILE` shows one). Run it
  after any change to the simulation. When a change was meant to alter
  the game (tougher enemies, a new level), `voxcade check <game> --update`
  records the checks anew: say so in the reply, the user's own recorded
  runs may then show something different. When the game's files change
  but the game should not (a new layout of `conf.lua`, renamed markers),
  `voxcade check <game> --refit` plays the replays on the new data and,
  only if every one runs exactly as recorded, writes the new fingerprint
  into them (nothing else). `voxcade check <game> --rollback K` goes back K
  ticks again and again and plays on, as rollback over a network would
  (`docs/multiplayer.md`, stage 0): each replay must still come out as
  recorded, and it says what keeping and bringing back the game cost; the
  Builds workflow checks every system so (test
  `going_back_and_playing_on_comes_out_the_same`). Every run played in the window
  is recorded (`replay::Runs`): runs of 10 s or more are kept in the
  config folder beside the settings (the newest 20; `voxcade replays
  <game>` lists them), F9 (a maker key, like F5) puts the run being played
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
  shooter's behaviours run as systems in a fixed order (player, soldiers,
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
  behaviour, `Brain::Player` with its `Seated`: hit points, gun, rounds,
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
  gets the recording and plays it through; `Session::play` takes a
  `session::Tick` whole, every machine's recording is the host's byte
  for byte; `game::Local` is what a machine draws of its own
  (`frame.players`, its language in `on_draw`, its high score lines); the
  state hash in parts (`Game::state_sections`, `Session::sections`) is
  compared every 10 ticks, a desync named by its part and reported in the
  config folder (`desyncs/`). `voxcade run <game> --host [PORT]` and
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
- Every game can be tried late without playing there (the user: without
  it testing a game is hardly possible): scenarios and the console are
  part of every game (`docs/game-model.md`, "Scenarios and the console").
  A scenario, `scenarios/NAME.lua`, begins a run late (`about`, `level`,
  `place`, `setup = function() ... end`, run once before the first tick,
  `require` giving the game's modules); a game gives its scenarios words
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
  keeps Lua deterministic (`sandbox.rs`, `prelude.lua`, `lex.rs`): only
  table, string, math, utf8 (and `require` for the game's own Lua files
  while it starts); maths through libm; `^` refused but for
  squares; `pairs`/`next` in key order; `math.random` from `Sim::rng`; no
  addresses in `tostring`/`format`; no `__gc`/`__mode`; an instruction
  budget and a memory limit. The API is version 2 (`docs/game-model.md`,
  built in stages): the globals `world`, `level`, `player`, `input` (in
  `on_tick`), `screen` (effects: shake, flash), `controls` (the
  player's device and keys; replays carry it, `Session::set_controls`)
  and `music` (the game's own song, `music:play`, and its clock: a step
  every tick the game goes, hit stop too, `on_beat` before `on_tick`,
  `music.beat`, `music.bar`; `lua::host::Music`, in the snapshot, and in
  the hash and the fingerprint (the songs' `#tempo`, `#meter`) of a game
  whose code names `music` or `on_beat`, `Game::uses_music`; the audio
  follows it, `Session::feed_music`, `music::Player::follow`;
  `music.intensity` the game's tension, which a song's layers follow,
  `#layer` in the MML; music is never a kit's),
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
  natural order, places in the files), changed by asks at the tick's end
  (`world:go_to`, `world:restart`, `world:win`, `world:lose`; one counts,
  going on before starting over) through `on_level_end(why)` and
  `on_level_start(how)`, faded in a window only (`c.transition`, no ticks
  run, recordings skip it: `Session::ticked`); holds (`level.state.held`),
  events and save games as `docs/game-model.md` has them; globals are
  frozen after
  the start (`on_world_start`, the first `on_level_start`; a mistake there
  stops the game, and the mended file starts it afresh,
  `Game::start_failed`), `on_draw` only looks; `check`
  plays a second twice from a snapshot every 5 s and names a local of
  `main.lua` that keeps state anyway (`replay::guard`, `Game::hidden`). A
  script's error halts the game (file, line, tick) until the file is
  mended; `shot` and `check` end with it. The kits report what their
  behaviours do to things (`Sim::events`: hits, deaths, pickups) and the
  engine tells the script after their rules, with the touches it finds
  itself (`on_hit`, `on_death`, `on_fall_out`, `on_pickup`, `on_touch`,
  `on_touch_end`, `on_interact`; `LuaGame::tell_events`); after the run's end the script
  hears nothing more. The kits move and fight; what a touch, a kill or a
  fall means is the game's code (`docs/game-model.md`, stage 7): points,
  pickups, checkpoints, a life lost, winning and losing (More Blood's
  `main.lua`, the templates'). The pins, pure Rust, keep More Blood's
  rules in Rust (`pins::Classic`, `gunships_guard`). The Lua API has one source, `src/lua/api.rs`: a field
  or method new in `world.rs` goes into the register too (test
  `the_register_is_what_the_scripts_see`), then `voxcade api --lua
  lua/voxcade.lua` (test `the_editors_definitions_are_up_to_date`). Tests:
  `src/lua/tests.rs`.
- Blocks are shaped by hand only (the user rejected relief, rounding and
  primitive models made by tools as fake 3D): an imported tile is drawn
  out through its depth, every layer its picture, nothing more. The
  editor stays calm (the user on vengi: too much, hard on the eyes): few
  tools in view, big readable words, the game's own light. Its panels are
  the IDE's look, egui through `crate::gui` (`editor/panels.rs`; the
  user's choice: of a piece with the IDE), the 3D view centred in the
  room they leave.
- Sokovox's puzzles are David W. Skinner's Microban, free to pass on
  "provided they remain properly credited" (his words): keep his name in
  the game (the HUD's last line), `games/sokovox/levels/README.txt`, and
  his file as he published it.
- Kinds of things are data (`c.things.NAME`: `behaviour`, `model`, `hp`,
  the behaviour's numbers; `like = "other"`), weapons too
  (`c.weapons.NAME`, the game's alone: `src/shooter/kit.toml` has the
  kit's to copy; `c.player.attacks` binds them to actions, `c.state.player`
  says what a run starts with). Markers place things by their kind's
  name (`c.markers.rifleman = 240`, map.json likewise). A new number a
  behaviour reads goes into its kind's rules (`shooter::kinds`, a default
  in the spec and in `kit.toml`, test
  `the_kits_kinds_are_the_behaviours_defaults`), not into a section of
  its own. Mistakes name the table that makes them; a reload keeps things
  by their kind's name (test `a_reload_keeps_things_by_their_kind`).
- The simulation only reports sounds (`Sim::sounds`: events by name); the
  engine turns them into audio (`Session::play_sounds`) so the simulation
  stays deterministic. The speaker (`audio::Speaker`) keeps its mixer
  behind a lock of its own that SDL's callback holds only while mixing;
  the window and the IDE never lock SDL's audio stream, never pause its
  device, and close it on a thread of their own and leave SDL unquit at
  the end (a Bluetooth headset dropping out and PipeWire started anew
  left SDL's stream locked: BunnyFarm froze and could not be quit). The engine knows no shooter words: input is the four
  directions plus the game's actions by name (`input::Input`,
  `Game::actions`, More Blood's `shooter::ACTIONS`; keys bound by those
  names, `Settings::adopt`), sound events and behaviours are names the
  game lists (`Game::sounds`, `Game::behaviours`), markers the names of its
  kinds of things. A new shooter event needs an
  `Sfx` variant, a name, and a sound in `shooter::default_sound`.
- The game opens on a title screen (`c.title_screen`: its own camera
  position and width, props, flyby, logo, music, `light_menu` for light
  letters over a dark scene like Sokovox's; or a logo of its own as a
  model, `logo_model`, its named parts dropping in one by one, built
  into the model bank like the letters, `ui::logo_model_parts`, test
  `a_logo_model_drops_in_part_by_part`: BunnyFarm's thick letters with
  bunny ears on the B and a carrot for the Y; `logo_bob`, the user's
  wish for it: landed letters bob gently, a smooth wave through them
  (whole voxels looked like stuttering, the user), `ui::bob`, test
  `the_logo_bobs_once_landed`, 0 unless set); the menus (`src/menu.rs`,
  drawn in `src/ui.rs`) work with keyboard and gamepad alike, Escape pauses
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
  (`src/saves.rs`, `saves/<game>/N.toml`; `shot`, `check`, `inspect` and
  replays keep theirs for the run only) live in the user's config folder
  (`src/settings.rs`), never in the game folder.
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
  idea, quality levels for older hardware), `shot --shadows` the same.
  Check menus with
  `shot --screen title|continue|settings|audio|video|controls|players|player-controls|network|find|join|joining|intro|pause|host-pause|guest-pause|save|load|attract`
  (a game with a save offers CONTINUE first on the title: the newest
  save, `Slots::newest`). More Blood's title:
  giant dog tags lying in a clearing at the start (generated, the user's
  wish), stamped with the hero's name: Nick Pain (the user's choice), metal (MagicaVoxel metal material: sky reflection, highlight,
  `metal_sweep`). Big letters use `c.ui.display_font`, a glyph atlas from a
  bold typeface (`src/font.rs`); the user wanted real letter shapes, not
  the 5 x 7 font blown up. For depth (the user's wish) they have rounded
  front edges (`c.ui.round`), uneven colour (`grain`) and, in the logo,
  cracks (`logo_cracks`, mostly in BLOOD); `ui::extrude` builds them, test
  `big_letters_are_rounded_and_cracked`. The pixel font (`ui::FONT`, 5
  wide, two rows below the line) has lower case, the letters of German
  and French and a few signs (BunnyFarm's three languages): a game's text
  is drawn as written, the engine's own in capitals as always
  (`ui::engine_case`, `text` against `written`), so older pictures stay
  the same pixel for pixel; accented capitals at full height, the accent
  above (`ui::accented`); test `the_font_writes_german_and_french`. A
  game speaks the languages it lists (`c.languages`; SETTINGS offers
  LANGUAGE, the system's at first in a window, the game's first in
  `shot`; `world.language`; carried with the controls in replays), and
  the engine's own menus speak them too: German and French beside English
  (`src/lang.rs`, every word by its English; a game that speaks others
  sees English).
  Shaders are WGSL compiled at run time:
  `the_shaders_are_valid` parses them in `cargo test`.
- The UI is voxels too (`src/ui.rs`, the user's choice): a second scene
  drawn into its own layer after the world and laid over it after the
  blur, so the tilt-shift never softens it. Its camera is a one-point
  perspective with the vanishing point in the middle of the screen (the
  user's choice, it replaced an oblique view): `c.ui.slant` sets how
  strong, the plane `c.ui.depth` fills the screen exactly. The title's
  settings play the chosen song (`Menu::previews_song`). The HUD is the
  game's (the user: "we are writing an engine, not a moddable game"):
  its `on_draw` draws it (`frame.width`/`height`/`alpha`, `frame:text`
  with a size, `text_width`, `box`, `bar`, `icon` from `c.icons`,
  `drop_in`; `screen:burst`, the voxels only shown, `HudState`); the
  engine helps with how and where, the kits draw none.
- Music is MML (`src/audio/music.rs`), text so the user and agents can
  write it. MIDI comes in through `tools/midi2mml.py` and an import file
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
  Pulse waves off duty 0.5 carry a DC offset (thumps, lost headroom): the
  three songs from audio stems avoid them, End Boss's riff and lead still
  use them. Rebuild the release binary
  before checking a parser change with `voxcade music` (`cargo test` alone
  does not). More Blood's mood, the user's words: dark, menacing,
  atmospheric, C64 style; its four songs (Move On, Blood Rush, Storm Is
  Coming, End Boss) belong together, the user's choice; earlier songs stay
  in the repo, out of the list.
  Simple background music comes from chord progressions instead
  (`tools/compose.py`, the user's wish: Suno was much work, and C64 sound
  for half an hour "makes you want to run amok"): a recipe
  `NAME.compose.toml` (style calm, mysterious, dark, action, bright or space; key, tempo,
  moods or progressions, seed) makes the same song every time, from
  free-midi-chords' progressions (Ludovic Drolez, MIT:
  `tools/progressions.toml` and its licence; keep the credit, in
  `music/README.md` too). Its rules against fatigue: the melody in some
  sections only (the user: "teils teils"), chords as soft pads (the
  C64 arpeggio a quiet accent in dark and action), everything low-passed,
  pulse at duty 0.5, the texture changing every 8 bars over two to three
  minutes. The library `music/` (seven songs, for the editor's package
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
  behaviour's defaults scaled from the figure they were tuned on
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
  for a whole body or the engine lifts the actor onto the roof.
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
  over `camera::corner_rays`); if you change camera or sun handling, rebuild
  all meshes. A game sets its daylight per picture (`frame:daylight` in
  `on_draw`: the sun's way and colour, ambient, shadow, sky; the rest
  `c.light`'s, `session::Daylight`); a game whose code names `daylight`
  keeps every face a sun above the horizon can light
  (`mesh::face_mask_any_sun`, `Game::moves_sun`; a reload that changes it
  rebuilds the meshes), and its shadows are fitted to the last picture's
  sun, then to this one's. The shadow map is fitted to the view (`camera::light_view_proj`)
  and snapped to whole texels so shadows stay still while scrolling. A
  game that moves its sun turns the map every picture, which lays its
  texels anew: at a shadow's edge a face, decided at its middle, went
  from lit to shaded and back (the user saw it in BunnyFarm). Its faces
  remember their shadow (`FrameData::remember`, the shader's
  `remembered`: a table of faces in a storage buffer, by where a face is
  and the way it looks, not by pixel, so a face stays whole): how much in
  the sun lately and whether lit, changing only a margin past the
  middle, so a face changes once as the edge passes and the shadows move
  with the sun face by face. Only faces at an edge make a memory; a clear
  face looks whether it has one and lets it fade. Tried and dropped: the
  sun in steps of 0.4, 1.5 or 3 degrees (flickered less, or jumped; the
  user: "3 degree jumps are not ok"), the map's texels laid on the
  ground (raised faces still flickered), two maps of fixed suns blended
  (a face is lit or not, so every face between them changed at once
  halfway: a jump moved in time). Measured with `film` frame by frame at
  three times of BunnyFarm's morning: no face goes back and forth; one
  or two faces change a picture. It costs the small GPU (Radeon 610M)
  about 2.5 ms (test `a_moving_sun_has_the_faces_remember_their_shadow`).
- Rendering goes scene (HDR colour + glow targets) -> quarter-size blur
  passes -> composite onto the surface (`src/post.wgsl`). Instances are
  opaque first, then see-through ones sorted far to near
  (`FrameData::see_through`); see-through cubes write no depth and cast no
  shadow. Actor models get see-through voxels from MagicaVoxel's glass
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
  view rays (`vs_model_outline`), so it shows at the silhouette and where
  a part stands well before another, not on every relief step.
  Rounded heads for More Blood's 22-voxel figures were tried and rejected
  (the user: "the old figures were prettier"): a 5-voxel head has too few
  voxels to round, the rounding pays off on bigger models. Tests:
  `smooth_shading_keeps_hard_edges`, `more_blood_shades_its_figures_smooth`.
- Camera yaw stays 0 unless the user asks for it. A turned camera needs the
  actors turned with it: their cubes are drawn axis-aligned to the world and
  their resting direction is world south, so with yaw they look skewed. Turn
  the instance cubes and face actors relative to the camera before shipping
  any yaw.
- World voxels are palette indices (0 empty). Moving things are instanced
  cubes built in `Session::compose` and the game's `Game::draw` (through
  `session::Drawing`), on the voxel grid by default: one cell each,
  quarter turns (a kind's `spin`, degrees a second, turns its model round
  its upright axis as it is drawn: Voxkeep's gem; `session::spinning`, the
  picture only). A game may turn models freely, its choice (the user's
  decision 2026-10-22; it was Voxatron's quarter-turns-only rule:
  Minecraft lives from it, and turning parts is the quick way to
  animation): a kind's `turns = "free"` faces exactly its way,
  `frame:model`'s turn (degrees round, pitch, roll) and
  `Drawing::model_turned` any way, about the pivot cell's middle at half
  the model's height (`render::turned_centre`, the shader's `rotate`,
  `ModelDraw::rot` and `shift`, `Instance::turn`); unturned models draw
  exactly as before (tests `a_free_quarter_turn_is_the_quarter_turn`,
  `models_from_the_bank_match_their_cubes`,
  `a_game_turns_its_models_freely`). Figures of parts (the user's wish:
  animation by turning parts): a `.vox` whose objects have names is one
  (`vox::Part`; without names its models are frames); the biggest part
  is the root, each other hangs on the part it touches most and turns
  about the middle of where they meet (`parts_of`); a kind's `swing`
  swings parts with its steps (`session::swinging`), `frame:model`'s pose
  turns them by name (`Drawing::model_posed`, `part_chain`); a part a draw
  in the bank. Only the picture: the simulation sees the whole frame
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
  the GPU once (`render::ModelBank`, filled in `Models::load`): a frame
  sends one `ModelDraw` per model (where, quarter turn, flash) and the
  shader's `vs_model` places the voxels as `ModelBank::cubes` does (test
  `models_from_the_bank_match_their_cubes`); glass and fading models stay
  cubes built every frame. The title logo's letters lie there too
  (`ui::logo_letters`; the game builds the bank anew when a model, the
  font or `conf.lua` changes, `Session::rebuild_bank`; test
  `the_logo_from_the_bank_matches_its_cubes`), and the title's props find
  their ground once (`Session::place_props`). Where two things share a cell, the one drawn
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
  purpose: the user's system has no inotify watches to spare. A map level
  rebuilds when any file it was made from changes (`Session::level_files`);
  a level made in code (`Game::level`) when `conf.lua` changes, which
  also reloads the models (kinds may have come, gone or changed theirs).
- Big levels are maps (`src/map.rs`, the user's choice): ground from a
  height map and a material map, `.vox` prefabs placed in `map.json`. Never
  go back to one huge `world.vox` for them (More Blood's was 90 MB, over
  GitHub's file limit). A prefab's anchor is the middle cell of its bottom
  layer (size / 2); `turn` is anticlockwise quarter turns. Test:
  `a_map_builds_ground_prefabs_and_markers`.

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

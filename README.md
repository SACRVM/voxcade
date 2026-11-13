# Voxcade

A voxel arcade engine. Everything is made of little cubes, everything can be
blown apart, and it has to feel like an arcade machine. Every classic 2D
genre, played in voxels: a game is a folder of Lua (or a crate in Rust),
it plays the same every time, and the next one is yours to make: in an
IDE with its own voxel and level editors, an agent along if you like.

Voxcade picks up where [Voxatron](https://www.lexaloffle.com/voxatron.php)
left off: the world fills the whole screen instead of sitting in a box,
the GPU does the drawing, and the games are written like
[LÖVE](https://love2d.org)'s, deterministic.

> **Status: early, and in use.** Version 0.6, for Windows and Linux (macOS
> builds from source). Five games come with it, three templates to begin
> new ones from, a launcher, the IDE. The Lua API may still change before
> 1.0. Where it is going: [docs/concept.md](docs/concept.md) and
> [docs/engine.md](docs/engine.md).

**Get it:** [voxca.de](https://voxca.de/) has the downloads, [how to
install](https://voxca.de/install.html) step by step and a [quick
start](https://voxca.de/quickstart.html) to a first game of your own.
Voxcade is free, under the zlib licence ([LICENSE.txt](LICENSE.txt),
[NOTICE.txt](NOTICE.txt) for what it covers), as
LÖVE is: a game you make with it is yours, to give away or to sell.

## The games

| Game | What it is | Built on |
|---|---|---|
| **More Blood** (`games/moreblood`) | A desert run'n'gun at 8-bit sprite scale: soldiers, huts, hostages, a watchtower to bring down, a gunship guarding the helipad | the shooter kit |
| **Voxkeep** (`games/voxkeep`) | A jump & run in a castle at night: six rooms of voxel blocks, bats to time, ferries over spikes, levers, a lift, a crate holding a hatch open | the platformer kit |
| **Sokovox** (`games/sokovox`) | Sokoban in voxels, David W. Skinner's 155 Microban puzzles | Lua alone |
| **Pulsar** (`games/pulsar`) | A space shooter scrolling up the screen, its waves coming on the song's bars | Lua alone |
| **BunnyFarm** (`games/bunnyfarm`) | A farming game with a tamagotchi at its heart: crops, a market, a kitchen, orders by mail, a barn, a shy wolf, a farm year with its reckoning; German, English and French | Lua alone, over several files |
| **The Arena** (`games/arena`) | The shooter kit's playground: a yard to blow apart | the shooter kit, no code |
| **Rust Digger** (`examples/rust_digger`) | A game in Rust on the engine alone | Rust |

`games/bench` is no game: a load test at 16-bit sprite scale.

## Run it from source

Needs Rust (stable), SDL3 and a Vulkan, Metal or DirectX 12 GPU.

```bash
cargo run --release                           # the launcher: every game to PLAY or EDIT, NEW GAME
cargo run --release -- run games/moreblood
cargo run --release -- run games/bunnyfarm
cargo run --release --example rust_digger
```

A game opens on its title screen; keyboard and gamepads work everywhere
(plugged in while it runs too), and the menus name the keys as the
keyboard in use prints them and the buttons as the pad in hand does. Each
game's keys are its own, shown in its HUD and in Settings > Controls.
Everywhere:

| Key | |
|---|---|
| Escape, P, Start | pause (settings, save and load where the game has slots, quit) |
| F11 | fullscreen |
| `^` (left of 1; `` ` `` on a US keyboard) | the console (below) |

The maker's keys are not a player's: `voxcade run <game> --maker` (and
the IDE's PLAY, always) brings F5 (start again on the files as they are),
F9 (keep the run being played as a replay in the game's `replays/`) and
1 .. 9 (the game's songs).

`voxcade pads --seconds 15` shows what SDL sees of a controller that does
nothing. Settings, high scores and save games are the player's, in the
config folder (`~/.config/voxcade`, `%APPDATA%` on Windows), never in the
game folder.

## The IDE

```bash
cargo run --release -- ide games/bunnyfarm
```

The game's logic, its scenarios and its assets on the left; the one file
open in the middle (Lua with its mistakes marked as you type and the API's
names offered, levels and models as pictures to turn); below, the console:
your shell in the game folder. PLAY plays the game right there (PLAY
SCENARIO from a scenario), OUTPUT shows its errors with a way to the line
and takes lines of Lua while it plays. Levels and models open in editors
of their own (blocks, voxels, BOX and SPHERE, BEVEL, SYMMETRY). BUILD
packs the program and the game into a zip for each system; HELP shows the
Lua API by example; NEW begins a game from a template (empty, jump & run,
shooter). See [docs/ide.md](docs/ide.md).

An agent along, if you like: CLAUDE in the console starts Claude Code
with this game's conversation. It sees what you see (a frame, the game's
state, a line of Lua tried in the running game, through MCP) and helps
where you ask; the game stays yours, its rules a few readable files of
Lua, its figures and levels shaped in the editors. Claude Code is
Anthropic's, to install yourself; it needs a Claude subscription or an
API key, and asks you to sign in the first time it starts in the console.
Nothing in Voxcade needs it: the agent starts only when you ask, and the
IDE's settings leave the whole console out.

## Write a game in Lua

A game is a folder: `conf.lua` sets it up, `main.lua` is its rules,
`.vvx` models beside them (Voxcade's own, text). `conf.lua` says which
Voxcade the game is for (`c.voxcade = "0.6.0"`); a game made for an older
one is brought up to date with `voxcade upgrade <game>`.

```lua
-- conf.lua
function conf(c)
  c.voxcade = "0.6.0"                    -- the Voxcade it is for
  c.title = "Walk"
  c.world.size = { 128, 128, 16 }        -- a world made in code
  c.actions = { "jump" }
  c.keys = { jump = { "Space" } }
  c.things.keeper = { model = "keeper.vvx" }
end

-- main.lua
function on_level_start(how)              -- the level begins
  level:fill(0, 0, 0, 127, 127, 0, "#3b4150")
  level.state.keeper = level:spawn("keeper", 64, 64)
  level.camera:look_at(64, 64)
end

function on_tick()                        -- 60 times a second
  local keeper = level.state.keeper
  keeper:walk(input.dir, 40)
  if input:pressed("jump") then keeper:jump(8) end
end

function on_draw(frame)                   -- every picture, only looking
  frame:model("keeper.vvx", level.state.keeper)
  frame:text("WALK ABOUT  {jump} JUMPS", 4, 4)   -- {jump}: the player's key for it
end
```

- `world` (the run), `level` (the level being played), `player`,
  `screen`, `controls` and `music` are there in every handler, `input` in
  `on_tick`; the engine tells the script what happened (`on_touch`,
  `on_hit`, `on_death`, `on_pickup` ...). Levels, items, events and save
  games are one model: [docs/game-model.md](docs/game-model.md).
- What changes lives in a state: `world.state`, `level.state`,
  `player.state`, `thing.state`. Globals freeze after the start, `on_draw`
  only looks, `math.random` is the game's own, `pairs` walks in order:
  so every run of the same input plays the same, and replays, `shot` and
  `check` work for every game.
- `require` brings in the game's own Lua files; `os`, `io` and the clock
  are not there.
- A mistake stops the game with its file and line over the picture; save
  the mended file and it goes on. Saving any file while it runs takes it
  in (hot reload).
- Ready-made rules for whole genres: `c.kit = "shooter"` (soldiers,
  huts, hostages, pickups, a gunship, weapons as data) or `"platformer"`
  (a hero with coyote time and ledges, walkers, hoppers, flyers,
  platforms, crates). They move and fight; what a touch or a kill means is
  the game's code. Their kinds to copy: `src/shooter/kit.toml`,
  `src/platformer/kit.toml`; `voxcade kinds <game>` prints every number in
  effect.
- `voxcade api` lists the whole API; each game's `.luarc.json` points the
  Lua language server (VS Code, Zed, Neovim) at `lua/voxcade.lua`.
- Lengths are voxels and speeds voxels a second, each game its own scale;
  figures small and plain, the 8-bit charm, one colour a material (the
  engine lights them).

## Try it late: scenarios and the console

A game can hardly be tested by playing to day 40 or the fifth room first,
so every game keeps **scenarios**: runs begun late, set up in code.

```lua
-- games/bunnyfarm/scenarios/wolf_night.lua
local set = require("setup")           -- BunnyFarm's own words for it
return {
  about = "Day 12, 21:30: hens and sheep, a gap in the sheep's fence",
  setup = function()
    set.day(12, "21:30")
    set.animal("sheep", "f", "white")
    set.gap("sheep")
    set.bunny_at("burrow")
  end,
}
```

```bash
voxcade scenarios games/bunnyfarm                      # what there is
voxcade run games/bunnyfarm --scenario wolf_night      # straight into it
voxcade inspect games/bunnyfarm --scenario fox_buys --do "player.state.coins = 0"
```

The **console** runs a line of Lua in the game between two ticks: the key
left of 1 slides it in at the bottom of the window (`require("wolf").night()`,
`player.state.coins = 9999`, an expression shows its value); in the IDE a
line under OUTPUT; `--do` for `run`, `shot` and `inspect`. Both are part
of the run: a replay carries its scenario and its console lines and plays
them again, and a run they helped goes into the high score table as GOD. Every game and template here has scenarios (a test asks for
them); something new that only shows late gets one, and a test replay
from it.

## See and check it without a window

Everything plays the same every time, so a picture, a state or a whole
run can be made from the command line:

```bash
voxcade shot games/voxkeep --frames 120 --input "right:60;right+jump:20" --out shots/x.png
voxcade inspect games/sokovox --frames 300 --input "up:20;left:40"     # the states as JSON
voxcade check games/bunnyfarm                                          # every replay as recorded?
voxcade film games/voxkeep games/voxkeep/replays/demo.toml --out shots/demo.mp4
voxcade sheet games/bunnyfarm bunny.vvx                                # a model as the game shows it
```

- `shot` writes one frame and says what it cost (CPU and GPU per pass,
  the slowest ticks); `--screen` shows a menu (title, settings, pause,
  console ...; `voxcade shot --help` lists them all), `--record` keeps
  the run as a replay.
- `voxcade COMMAND --help` says every option a command takes, a
  person's first, the ones for pictures and tests under their own
  heading; `voxcade help all` says everything.
- `check` plays every replay in a game's `replays/` and says whether each
  runs as recorded (a hash of the states every second and the state its
  run ended in), drawing after every tick as a window does; `--refit` takes new game files that play
  the same, `--update` records them anew after a change meant to alter
  the game. `replays/tests/` holds the regression runs, `replays/` itself
  the demos the title plays after 30 s.
- `film` renders a replay with its music and sounds to MP4, WebM or GIF.
- Every run played in the window is kept in the config folder
  (`voxcade replays <game>`); `voxcade replay <game> <file>` shows one.

## For agents

`voxcade mcp` serves the engine to an agent over MCP: `shot` (a frame as
a picture), `run` and `things` (how the game stands), `check`,
`scenarios`, `api`; shot, run and things begin from a scenario and run
lines of Lua first. When the IDE runs on the game, `view_state`, `open`,
`game_frame`, `errors`, `play`, `stop` and `eval` (a line in the game
being played) reach it. The IDE's MCP button writes the game folder's
`.mcp.json`; else:

```bash
claude mcp add voxcade -- /path/to/voxcade mcp
```

## Making what a game is made of

- **Models:** Voxcade's own `.vvx`, text (layers of letters, a palette
  of colours and materials), shaped in the IDE's voxel editor or written
  by hand: frames for animation, or named parts the engine turns (a
  kind's `swing`, `frame:model`'s pose); glass, metal and glow
  materials. A MagicaVoxel `.vox` (from MagicaVoxel, vengi, Goxel) is
  brought in with `voxcade import <game> FILE.vox`. Python generators in `tools/` (`figures.py`,
  `gen_*.py`) make the games' first figures; after that they are the
  user's, shaped by hand.
- **Levels:** text too. A map (`map.toml`: ground from height and
  material images, `.vvx` prefabs placed on it; More Blood, Pulsar's
  space), levels of voxel blocks (`levels/*.toml`, layers of signs a
  person reads and git diffs cell by cell, shaped in the level editor; a
  Tiled map brought in once with `voxcade import`;
  [docs/tiles.md](docs/tiles.md)), one world (`world.vvx`, the Arena: its
  palette numbers its colours, markers named in it), or a world made in
  code (Sokovox).
- **Sound:** every sound a file of the game, `sounds/NAME.toml`, a few
  numbers for the synthesizer (like sfxr; no recordings: everything in a
  game is code or text); `voxcade sounds <game>` writes them to a WAV to
  listen to.
- **Music:** MML text on SID-like voices (pulse, saw, triangle, noise,
  the SID's filter, arpeggios, ducking), played anew on every save. Songs
  from chord progressions with `tools/compose.py` (the library in
  `music/`), MIDI and Suno stems through `tools/midi2mml.py` and
  `tools/audio2midi.py`; a game picks its song and goes with its beats
  (`music:play`, `on_beat`). `voxcade music <game>` writes a WAV.

## The look

A fixed perspective camera per game, voxel-exact shadows (each face lit
or shaded as a whole) from a sun a game may move through its day, glow,
lights, haze and a tilt-shift blur laid over the picture, metal that
mirrors the sky, see-through glass, a dark outline round figures if a game
wants one. Blown-off pieces fall when nothing holds them; debris settles
back into the world. The HUD and the menus are voxels too, in a scene of
their own over the game.

## Builds

GitHub Actions `Builds` (started by hand, or by a `v*` tag, which makes
a release): Linux and Windows, macOS on Intel when asked for, SDL3 built
in. `tools/package.py` puts the folder together: the program, the games,
`dist/`'s README and the licences. The program started without anything
opens the launcher, which also fetches new versions (`src/update.rs`).

## Where things are

| Path | What |
|---|---|
| `src/game.rs`, `src/session/` | The game API (`Game`) and a loaded game folder (hot reload, title, music, the frame, scenarios, the console, snapshots) |
| `src/sim/` | The simulation's core every game shares: fixed 60 Hz, deterministic, things with ids and kinds |
| `src/lua/` | Lua games: the sandbox, the API (`api.rs` its register), `conf.lua`, saves |
| `src/shooter/`, `src/platformer/` | The two kits, ready-made rules for whole genres |
| `src/render/`, `src/*.wgsl`, `src/mesh.rs` | The renderer: chunk meshes, models on the GPU, shadows, post passes |
| `src/ui/`, `src/menu.rs`, `src/prompt.rs` | The voxel UI, the menus, the console |
| `src/app/`, `src/headless.rs`, `src/cli/` | The window, everything without one (`shot`, `film`, `sounds`, `music`), the command line |
| `src/replay.rs` | Recording, replays, `check`, attract mode |
| `src/ide/`, `src/gui/`, `src/editor/` | The IDE, its egui glue, the level and voxel editors |
| `src/audio/` | The synthesizer, MML music, the mixer |
| `src/mcp.rs` | `voxcade mcp` |
| `games/`, `templates/`, `examples/` | The games, the templates new games begin from, the game in Rust |
| `tools/`, `music/` | Generators for models and levels, music tools, the song library |
| `docs/` | The concept, the engine, the game model, the IDE, levels of blocks, BunnyFarm's design |
| `.claude/skills/` | How an agent makes games, models, music and checks a change here |

## Credits

- Sokovox's puzzles: Microban by David W. Skinner, "freely distributed
  provided they remain properly credited" (`games/sokovox/levels/README.txt`).
- `tools/compose.py`'s chord progressions: free-midi-chords by Ludovic
  Drolez, MIT (`tools/progressions.LICENSE`).
- More Blood's big letters: Fira Sans Compressed Heavy, SIL Open Font
  License. The IDE's letters: Hack, through egui.

## Contributing

Pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) says what
to check before sending one. For anything bigger than a fix, open an
issue first, so nobody builds what will not fit.

## Supporting it

Voxcade is free and stays free. If it is useful to you, you can support
its making: [github.com/sponsors/SACRVM](https://github.com/sponsors/SACRVM).

## Licence

zlib ([LICENSE.txt](LICENSE.txt)): the engine, the games that come with
it and the templates. What is others' keeps its own terms (Credits
above; the libraries built in: `tools/third_party_licenses.py` lists
them for a build).

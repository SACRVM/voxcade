# Contributing to Voxcade

Pull requests are welcome: fixes, games, templates, kinds for the kits,
translations of the menus, docs.

## Before you start

- For anything bigger than a fix, open an issue first and say what you
  want to build. Voxcade has a plan (`docs/engine.md`, `docs/game-model.md`)
  and a few firm rules; a short talk saves work that will not fit.
- The Lua API grows by the game model in `docs/game-model.md`: one
  generic system for every genre, no hook for one game alone. A new name
  in it needs a reason there.
## The rules that matter

These are the ones a change most often breaks. `CLAUDE.md` holds every
rule with its reason, as the maintainer and his agent keep them; it is
long, read the part about what you change.

- **The simulation is deterministic, on every system.** In `src/sim/`,
  the kits (`src/shooter/`, `src/platformer/`) and `src/lua/`: a fixed
  tick (`DT`), no clock, randomness only from `Sim::rng`, sines, roots and
  the like only through `sim::math` (`libm`; the system's maths differs in
  the last bit), no iteration over a `HashMap` (grid cells go in
  `world::CellMap`/`CellSet`), work on several cores only through
  `par::in_order`. Anything new that carries the game forward goes into
  its state hash. Replays and network play depend on it.
- **Every game's replays still play out as recorded** (`voxcade check`).
- **No `cargo fmt`.** The code is formatted by hand; a run of it rewrites
  some forty files.
- **Tests take their folders from `scratch::Scratch`**, never from
  `std::env::temp_dir()` by hand: the guard removes them, a panic
  included. `tools/check_tmp.sh` runs the tests and fails if anything
  named `voxcade-*` is left in the temp folder.
- **Everything in a game is code or text.** Models, figures and blocks
  are `.vvx`, levels and maps `.toml`, sounds `sounds/NAME.toml`, music
  MML; no WAV, no JSON (but other programs' own files). MagicaVoxel's
  `.vox` and Tiled's maps only come in through `voxcade import`. A file
  of Voxcade's own says its format and version on its first line; a
  format that changes brings a step in `src/upgrade.rs`.
- **Names are the ones other tools use**, in American English:
  `rotation`, `color`, `radius`, `behavior`; in the code a reader sees,
  in flags, fields and the Lua API.
- **The engine knows no game.** Lengths and speeds come from the game's
  `conf.lua`, in voxels; a kit is building blocks for many games, never
  one game's rules (what a kill or a pickup means is the game's
  `main.lua`).
- **The Lua API has one source**, `src/lua/api.rs`; a new name in it
  needs its reason in `docs/game-model.md`, then `voxcade api --lua
  lua/voxcade.lua`.
- **A command-line option goes into the table in `src/cli/table.rs`**; its
  parser takes nothing else, and its help is written from it.

## Before you send it

```bash
cargo build --release
cargo test --release
cargo clippy --release --all-targets -- -D warnings
for game in games/*/ templates/*/; do ./target/release/voxcade check "${game%/}"; done
```

The Tests workflow (`.github/workflows/tests.yml`) runs the same on Linux
for every push to `main` and every pull request: build, `cargo test`,
clippy with warnings as errors, and the replays of every game and
template. Some tests draw a frame and need a GPU or a software Vulkan
(mesa's lavapipe); one without fails, it does not skip.

- A replay that no longer plays out as recorded: if your change only
  alters a game's files and not how it plays, `voxcade check <game>
  --refit` takes the new fingerprint; if it is meant to alter the game,
  `--update` records them anew: say so in the pull request.
- A change to how something looks: look at it (`voxcade shot`, then the
  PNG; `voxcade shot --help` lists the menus it shows). The
  picture tests (`tests/pictures.rs`) keep the mean color of an 8 by 8
  grid of a few pictures as text in `tests/pictures.txt`; when a change is
  meant to alter them, `VOXCADE_PICTURES=renew cargo test --release --test
  pictures` writes the numbers anew: look at the diff and say so in the
  pull request.

## Licence

By sending a pull request you agree that your contribution goes under
the project's licence, zlib (`LICENSE.txt`). Do not send what is not
yours to give: art, music or code from elsewhere needs a licence that
allows it, and its credit.

## Bugs and ideas

An issue with what you did, what happened and what you expected. For a
game that plays wrong, the replay helps most: F9 keeps the run in the
game's `replays/` folder (`voxcade run <game> --maker` has the key).

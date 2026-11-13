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
- `CLAUDE.md` holds the project's rules as the maintainer and his agent
  keep them. It is long; the parts on determinism and on the kits matter
  most.

## Before you send it

```bash
cargo build --release
cargo test --release
cargo clippy --release --all-targets
for game in games/*/ templates/*/; do ./target/release/voxcade check "${game%/}"; done
```

The Tests workflow (`.github/workflows/tests.yml`) runs the same on Linux
for every push to `main` and every pull request: build, `cargo test`,
clippy with warnings as errors, and the replays of every game and
template. Some tests draw a frame and need a GPU or a software Vulkan
(mesa's lavapipe); one without fails, it does not skip.

- Every game's replays must play out as recorded (`voxcade check`). If
  your change only alters a game's files and not how it plays, `voxcade
  check <game> --refit` takes the new fingerprint; if it is meant to
  alter the game, `--update` records them anew: say so in the pull
  request.
- The simulation is deterministic on every system: no clock, randomness
  only from `Sim::rng`, sines and the like through `sim::math`.
- The code is formatted by hand; please do not run `cargo fmt` over
  files you do not otherwise change.
- A change to how something looks: add a picture (`voxcade shot`). The
  picture tests (`tests/pictures.rs`) keep the mean colour of an 8 by 8
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
game's `replays/` folder.

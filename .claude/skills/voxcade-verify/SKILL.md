---
name: voxcade-verify
description: >
  Check a change to Voxcade before calling it done: the release build,
  tests, clippy, every game's replays (and what to do when one no longer
  runs as recorded: --refit, --update, pins), shots of the game, menus and
  editor looked at as images, frame times on the small GPU, and an honest
  report of what could not be checked. Use this after any edit to the
  engine, a kit, a game's files, the editor, shaders or levels, before a
  commit, and whenever the user asks to check, test, verify, prüfen,
  testen or "schau ob es noch geht", even if they do not name a tool.
---

# Verifying a change in Voxcade

The user plays the games and uses the editor; you do not open windows.
What you hand over is evidence: tests that pass, replays that still play
bit for bit, and frames you have actually looked at. The engine is
deterministic, so the same command always gives the same picture and the
same replay; a difference always means something changed.

## 1. What the change touches decides what to run

| Changed | Run |
|---|---|
| simulation, a kit (`src/sim`, `src/shooter`, `src/platformer`), a game's `conf.lua`/`main.lua`, a level or its blocks | the standard pass (replays!), a `shot` of the place |
| rendering, shaders, meshing, camera, UI | the standard pass, shots before and after, frame times |
| the editor (`src/editor`) | the standard pass, `edit --shot` of both levels and of the things (`--things`) |
| the Lua API (`world.rs`, `api.rs`) | `voxcade api --lua lua/voxcade.lua`, then the standard pass |
| music or sounds | `voxcade music`/`sounds` to WAV, a spectrogram |
| docs only | nothing to run |

## 2. The standard pass

```bash
.claude/skills/voxcade-verify/scripts/verify.sh            # every game
.claude/skills/voxcade-verify/scripts/verify.sh voxkeep    # one game's replays
```

It builds the release binary (every `voxcade` command below uses it, so a
stale binary shows old behavior), runs `cargo test --release`, lists
clippy's warnings as `file:line` and plays every game's replays. Clippy
stands at 0 warnings (the Tests workflow runs it with `-D warnings`): a
warning is yours. In the main checkout build with `CARGO_TARGET_DIR=target-work`
and use `./target-work/release/voxcade` for every command below, never a
rebuild of `target/release` while the owner's IDE runs from it; an agent
in its own worktree uses that worktree's `target/`. Never run `cargo fmt` (the code is
formatted by hand; it would rewrite dozens of files).

After a change that adds tests or touches the temp folder, run
`tools/check_tmp.sh`: it counts `voxcade-*` entries in the temp folder
(RAM here) before and after `cargo test --release` and fails if they grew.
Tests take their folders from `scratch::Scratch`, never `temp_dir()`.

## 3. When a replay no longer runs as recorded

A replay carries a fingerprint of the game's data (with its parts by
name, so `check` names the files that changed), its state every second
and the state its run ended in. Read what `check` says:

- **"recorded on other game data"**: files the game is made of changed
  (code in `main.lua`, `conf.lua`'s simulation settings, level files,
  models). If the game should play exactly as before (a comment, a new
  draw call, colours merged), run `voxcade check games/NAME --refit`: it
  plays every replay on the new data and writes the new fingerprint only
  if all of them run the same. If refit refuses, the game did change.
- **"made for an older Voxcade"** (any command; "an earlier Voxcade
  0.6.0 (before: ...)" for a game stating this version whose files a step
  of it still changes): the game's `c.voxcade` is older than a format
  step (`src/upgrade.rs`). `voxcade upgrade games/NAME --dry` says what it
  would do, without `--dry` it does it; then `--refit` its replays. A
  change of a format brings its own step and runs it over every game and
  template here; levels, maps and worlds are text since 0.6.0 (a world's
  colours keep their numbers, so the replays stay "same" after a refit;
  a picture taken before and after is the same pixel for pixel).
- **"differs" at a tick**: the simulation plays differently. If that was
  the point (tougher enemies, a new rule, a new level), record anew with
  `voxcade check games/NAME --update` and tell the user in your reply:
  their own recorded runs may now play out differently. If it was not the
  point, it is a bug; find it (`voxcade inspect` at ticks before the
  difference shows the state), do not paper over it with `--update`.
- More Blood's pins (`src/shooter/pins.rs`, run by the tests) cover paths
  its replays do not take; they fail the same way and are pinned anew only
  for a meant change, said in the reply.

## 4. Look at it

What shows late in a game is reached from a scenario (`voxcade
scenarios games/NAME` lists them; `--scenario NAME`), never by playing
there first, and asked with `--do "LINE"` (the console: any state, the
game's functions by `require`). A path no replay takes yet gets one,
recorded from its scenario. `check` draws after every tick, so a
mistake in `on_draw` stops a replay there.

`shots/` is git-ignored: put every frame there and read the PNG with the
Read tool. Look for what the change should show, and for what it should
not have changed.

```bash
./target/release/voxcade shot games/voxkeep --frames 120 --input "right:60;right+jump:20;none:40" --out shots/x.png
./target/release/voxcade shot games/moreblood --at X,Y --frames 20 --out shots/y.png   # start anywhere (the shooter)
./target/release/voxcade shot games/voxkeep --screen title --frames 120 --out shots/t.png
./target/release/voxcade edit games/voxkeep --block stone --view -35,12 --size 900x700 --shot shots/e.png
./target/release/voxcade inspect games/voxkeep --frames 300 --input "..."   # the state as JSON, no picture
# a game of several players: --input2 (seat 2, 'away:ticks' while nobody sits there), --input3 ...
./target/release/voxcade shot games/bunnyfarm --scenario wolf_night --frames 60 --out shots/w.png   # late in the game at once
./target/release/voxcade inspect games/bunnyfarm --scenario fox_buys --do "player.state.coins" --frames 1   # ask it anything (the console)
```

- Input: the directions `left right up down`, the game's actions (More
  Blood `jump fire grenade strafe`, Voxkeep `jump restart interact`), `none`, joined with
  `+`, as `keys:ticks;...`. `--screen` takes `title settings controls
  intro pause attract highscores initials`; animated screens need enough
  frames to settle (the high score table some 300).
- The stats line after a shot (position, on the ground, score, chunks,
  vertices) is the quick numeric check: compare it with what you expect.
- Before and after: take the "before" shot first, then change the code.
  Since shots are deterministic, `scripts/tile.py OUT before.png after.png
  --diff` counts the pixels that differ and marks where; a change meant
  to leave the look alone must give 0. `--crop X0,Y0,X1,Y1` and
  `--width W` zoom in or put several angles side by side.
- To try a setting without touching the user's game (their files may hold
  work in progress): copy the game folder to the scratchpad, change the
  copy, shot the copy.

## 5. Measure on weak hardware

`shot` prints `frame: cpu ... | gpu ... (shadows, scene, ui, blur,
composite)` and the slowest ticks. This machine is fast; the user's aim is
older machines, so for anything that could cost frames measure on the
CPU's small GPU too, and with fewer cores:

```bash
MESA_VK_DEVICE_SELECT=1002:13c0 MESA_VK_DEVICE_SELECT_FORCE_DEFAULT_DEVICE=1 ./target/release/voxcade shot games/moreblood --frames 200 --input "up:200" --out shots/m.png
taskset -c 0-3 ./target/release/voxcade shot ...
```

At 60 fps a frame has 16.7 ms. Numbers for the 16-bit scale are in
`docs/engine.md`.

## 6. What you cannot check, and say so

How a jump or the camera feels, what music and sounds sound like (render
them with `voxcade music`/`sounds` and look at a spectrogram, but you have
not heard them), how the editor handles with mouse and keys, a hot reload
in a running game. Name these in the report and give the user the command
to try it (`cargo run --release -- run games/NAME`, `-- edit games/NAME`;
the Firepit bar has Run and Edit buttons).

## 7. Report

Short and in the user's language, with the numbers: tests (how many
passed), replays (same; or refit/updated and why), which shots you looked
at and what they show, frame times where they matter, and what is left
for the user to try. Commit and push only when the user says so.

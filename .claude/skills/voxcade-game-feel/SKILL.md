---
name: voxcade-game-feel
description: >
  Make actions in a Voxcade game feel good ("juice"): what the engine has
  for it (sounds, hit stop, screen shake, screen flash, lights, glow,
  particles, a thing's white flash, effect models, hand-drawn frames), what
  belongs in the simulation and what only in the picture, the player's
  shake and flash switches, recipes for jumps, landings, stomps, hits,
  pickups, explosions and goals, and tuning a platformer's controller.
  Use this whenever the user says something feels flat, weak, floaty,
  lahm, ohne Wumms, or asks for impact, feedback, polish, juice, hit stop,
  shake, flashes, particles or better sounds on an action, and before
  adding any such effect to the shooter or platformer kit or a Lua game.
---

# Game feel in Voxcade

A mechanic that works and one that feels good differ in feedback: a few
small, brief, slightly exaggerated responses fired together within about a
tenth of a second (a sound, a puff, a pause, a light), then everything back
at rest. In Voxcade that feedback has to respect two things the engine is
built on: the voxel grid, and a simulation that plays the same every time.

**Ask first.** Effects the user did not ask for are suggestions for the
reply, not changes to ship (an unasked x-ray silhouette had to be taken
out again). When they ask for more feel, propose a small set per event,
then build what they pick.

## Scale feedback to the event

| Tier | Events | Channels |
|---|---|---|
| quiet | footsteps, menu moves | at most a soft sound; usually nothing |
| small | jump, pickup, checkpoint | a sound, a small puff or sparkle, maybe a light |
| medium | stomp or kill, hurt, landing from high | sound with punch, effect model or dust, 2–3 ticks hit stop, a little shake |
| large | explosion, boss down, goal | fireball and debris, lights, screen flash, strong shake, a jingle |

Hit stop and shake are the strongest and most abused: never on routine
actions, always short.

## What the engine has

| Means | In a kit (Rust) | From Lua | Simulation or picture | Player switch |
|---|---|---|---|---|
| sound event | `self.sim.sound(name, at, vol)`; sounds are files, `sounds/NAME.toml` | `level:sound(name, at, vol)` | picture (audio) | volume |
| hit stop | `self.sim.freeze = self.sim.freeze.max(ticks)`; `c.effects.hitstop` | `world:hitstop(ticks)` (60 at most) | **simulation** | — |
| screen shake | `self.sim.shake = (self.sim.shake + x).min(1.0)` (0..1, dies down by itself) | `screen:shake(amount)`; `level:explode` shakes too | picture: the camera only | SHAKE |
| screen flash | `self.sim.screen_flash` (0..1, fades) | `screen:flash(strength)` (times `c.effects.screen_flash`) | picture | FLASHES |
| light | `self.sim.flares.push(Flare {..})` | `frame:light(at, radius, color)` in `on_draw`, each picture | picture | — |
| glow | bright colours glow (`c.effects.glow`) | `frame:cube(x, y, z, color, glow)` | picture | — |
| particles | `sim.dust`, `sparks`, `fireball`, `blood_spray`, `flash` (muzzle) | `level:explode` (debris, fire) | **simulation** (random from `Sim::rng`) | — |
| white flash on a thing | `thing.flash = ticks` (the shooter tints it) | — | picture | — |
| effect model | a kind's `effect = "poof.vvx"` (played once where a thing went; `{ model = "poof.vvx", fps = 20 }` its speed) | `frame:model(file, thing, index)` with an index from `world.tick` | picture | — |
| poses | a kind's `frames = { jump = {..}, crouch = {..} }`, drawn by hand, each set at its own `fps` (or `stride`, by the way walked) and once (`loop = false`) if it says | `frame:model(..., index)` | picture | — |

Effect speeds scale with `c.physics.body_width` (`Sim::effect_scale`), so
the same call fits a 10-voxel and a 22-voxel figure; do not tune them per
game by hand.

## Rules that come from the grid and the determinism

- **On the grid, or turned freely.** Models are never scaled: every
  voxel the same size. They stand in quarter turns unless the game turns
  them freely (the user's decision: turning is the quick way to motion):
  `frame:model`'s turn (`{ roll = 20 }`: a ship banking with its speed
  across, a spin on a kill, a wreck tumbling) and a kind's `turns =
  "free"` (facing exactly its way). Squash and stretch, anticipation,
  impact poses and landing crouches are frames drawn by hand into the
  model (the fox's crouch and jump frames), chosen by state.
- **The picture never moves the game.** Shake moves the camera target in
  `Session::view_target`, never a body; `on_draw` and `screen` only look. Keep
  new visual effects there.
- **What touches the simulation changes the game.** Hit stop, knockback,
  bounce and particles (they use `Sim::rng`, so every later random number
  shifts) change how replays play: the change is then meant, the replays
  are recorded anew with `voxcade check --update`, and the reply says so
  (the voxcade-verify skill). `c.effects.hitstop` is part of the replays'
  fingerprint. A new number a behaviour reads goes into the kit's rules
  (`kinds.rs`, `kit.toml`), with the test that keeps the two the same.
- **The player's switches.** SHAKE and FLASHES in the settings turn those
  off (motion sickness, flashing lights). Shake through `sim.shake` and
  flash through `sim.screen_flash` so the switches keep working; never a
  second path around them.
- **Hit stop holds everything,** the script's `on_tick` and the shake too
  (it stands still and dies down after). In Lua these calls work in
  `on_world_start` and `on_tick`; `on_draw` only looks. New feedback the API lacks is new API
  (`src/lua/world.rs`, `src/lua/api.rs`, then `voxcade api --lua
  lua/voxcade.lua`): propose it, do not work around it.

## Recipes

Built from the means above; take two or three per event, not all.

- **Jump:** a short rising sound (square, `sweep` up 12, an octave), a
  small dust puff at the feet (`sim.dust(feet, 4, ground colour)`), the
  jump frame.
- **Landing from high** (falling faster than about the jump's take-off
  speed): dust scaled by the fall, a crouch frame for a few ticks, a soft
  thud (noise, low `lowpass`). Not on every step off a kerb.
- **Stomp or kill:** 2–3 ticks hit stop, the puff effect (the kind's
  `effect`, shown as it goes), a falling square with `punch` (its
  `sound`), shake 0.1–0.2, the bounce (`player:bounce` in the game's
  `on_touch`).
- **Hurt:** the hero's blink (the platformer has it) or white flash, the
  knockback, a low saw, 1–2 ticks hit stop, small shake.
- **Pickup:** a bright rising arpeggio (triangle, `jump` 7, a fifth, at
  `jump_at` 0.04), the sparkle, a short light in the pickup's colour, the
  HUD count.
- **Explosion:** `level:explode`/`fireball` with flares, screen flash and
  shake 0.35 or more scaled by size, debris.
- **Goal:** a jingle (arpeggio steps), a light; then the run is won.

## A platformer's controller

Feel lives in the numbers first. The platformer kit's hero has (in
`src/platformer/kit.toml`, each with a comment): `speed`, `accel`,
`air_accel`, `jump` (height in voxels), `jump_cut`, `coyote`, `buffer`
(ticks), `climb`; `fall_gravity` (heavier on the way down, 1.5: snappy,
1: as floaty as the rise), `hang` and `hang_speed` (lighter at the top of
the jump while jump is held: time to steer), `corner` (a head clipping a
ceiling's edge by up to this many voxels slips past), `look_ahead`
with `look_ahead_speed` (the view leads where the hero runs, easing round
when it turns; `Sim::lead`, drawn in `Session::view_target`), `grab` and
`pull` (Prince of Persia's way, everything in one lane: a ledge's edge
within `grab` voxels above the head and `grab_ahead` beyond the body is
caught and hung from, the head `hang_below` under it, until up or jump
pulls the hero up in `pull` ticks, down lets go; an edge below the head
it nearly made it pulls itself onto at once; 0: off), `air_jumps` and
`air_jump` (jumps again in the air, from where it is, this high: a
double jump; the user found the grab too easy for Voxkeep, where a
ledge three cells up now takes the first jump held and the second in
its upper half: an air jump of 12 there left a window of a tick or two,
and the user could not get up; 18 leaves about a third of a second,
four cells still out of reach) and `face_ladder` (climbing, it turns its back to the camera;
frames seen from behind). A ladder's top holds from above like a plank:
up a ladder through a floor the hero stands on it, down climbs down.
Voxkeep shows them (a hero 14 tall: jump 22, air_jumps 1, air_jump 18,
corner 8, no grab); the platformer template keeps the grab (jump 22,
grab 6, grab_ahead 3, hang_below 2). Tune by what
the player sees: how many voxels high, how many ticks to the top, how far
a run-up jump reaches; the kit's tests measure these. Changing them
changes the game: the replays are recorded anew and the reply says so.

## Sounds

A sound is a file, `sounds/NAME.toml`, called by its name; a file of a
kit's sound's name replaces the kit's (`voxcade sounds games/NAME --list`
says which there are and where each comes from):

```toml
# a jump: up an octave
wave = "square"
freq = 330
sweep = 12
hold = 0.05
decay = 0.08
volume = 0.22
```

The words (`src/audio/words.rs`, one register with MML's instruments):
`wave` square, saw, sine, triangle, noise; pitch `freq` (Hz) sliding
`sweep` semitones over the sound; `attack`, `hold`, `decay` (seconds, 10
at most together); `punch` (a loud start); `jump` and `jump_at` (one jump
in pitch, semitones: coins, jingles); `vibrato`, `vibrato_speed`;
`lowpass` to `lowpass_end` (rumble, muffling); `volume` (0 to 1);
`silent = true` for none. No recordings: everything in a game is code or
text (a `.wav` in `sounds/` is a mistake at load). A pulse off
`duty` 0.5 sounds thinner; the engine plays it round zero (no DC offset).
`voxcade sounds games/NAME --only NAME` writes one to `shots/sounds.wav`;
you cannot hear it, so look at it (length, peak, a spectrogram) and let
the user listen.

## Checking it

- Shots at the event's ticks: the frame of the hit and two or six ticks
  later (`shot --frames N`), side by side with the voxcade-verify skill's
  `tile.py`. `shot` also lists the sounds a run made.
- Replays: visual effects leave them the same; anything in the simulation
  changes them (see above).
- How it feels is for the user to say: give them the moment to try (the
  command, where, what to do) and the numbers to turn.

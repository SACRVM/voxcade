-- Drums: a drum machine of sixteen steps, the test of songs a game makes
-- from its own data (music:play's channels laid over a file of
-- instruments, music/kit.mml), as Lights is the lamps' test hall. Not a
-- game to show: it has no cover, so the launcher does not list it, and the
-- packages leave it out.

function conf(c)
  c.voxcade = "0.7.0"
  c.title = "Drums"

  -- No level to speak of: the machine is the HUD (main.lua's on_draw).
  c.world.size = { 16, 16, 4 }
  c.player.spawn = { 8, 8 }
  c.light.sky = "#101018"

  -- The kit: a file of instruments, no song of its own.
  c.audio.music = { "music/kit.mml" }

  -- The arrows move the cursor; TOGGLE sets or clears a step, PATTERN
  -- switches to the other pattern at the next bar.
  c.actions = { "toggle", "pattern" }
  c.keys = { toggle = { "Space", "Z" }, pattern = { "Tab", "X" } }
  c.pad = { toggle = { "south" }, pattern = { "north" } }

  c.title_screen.logo = { "DRUMS" }
  c.title_screen.attract_after = 0
end

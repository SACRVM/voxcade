-- Studio: a trial of a music machine as a voxel model one plays with the
-- mouse through the model (its buttons and knobs are parts of a figure,
-- machine.vvx), the drum machine of games/drums as an object. Not a game
-- to show: it has no cover, so the launcher does not list it, and the
-- packages leave it out.

function conf(c)
  c.voxcade = "0.7.0"
  c.title = "Studio"

  -- The level is levels/studio.toml: the machine on a table, close up.
  c.world.size = nil
  c.camera.pitch = 62
  c.camera.view_width = 180
  c.light.sun = { 0.3, 0.5, -1.0 }
  c.light.ambient = 0.45
  c.light.sky = "#18161a"

  -- The kit: a file of instruments, no song of its own (as Drums').
  c.audio.music = { "music/kit.mml" }

  -- The mouse plays the machine's parts; the keys and the pad still work:
  -- the arrows move the cursor along the steps (up and down choose the
  -- instrument), TOGGLE sets or clears a step, PATTERN switches at the
  -- next bar, START starts and stops.
  c.pointer = "system"
  c.actions = { "click", "toggle", "pattern", "start", "turn_up", "turn_down" }
  c.keys = {
    click = { "mouse_left" }, toggle = { "Space", "Z" }, pattern = { "Tab", "X" },
    start = { "Return", "C" }, turn_up = { "wheel_up" }, turn_down = { "wheel_down" },
  }
  c.pad = { toggle = { "south" }, pattern = { "north" }, start = { "west" } }

  c.title_screen.logo = { "STUDIO" }
  c.title_screen.attract_after = 0
end

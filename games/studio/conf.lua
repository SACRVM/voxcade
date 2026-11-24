-- Studio: a trial of a music machine as a voxel model one plays with the
-- mouse through the model (its buttons, knobs and switches are parts of a
-- figure, machine.vvx): the HARALD RG-80, a drum machine after the analog
-- one of 1980. Not a game to show: it has no cover, so the launcher does
-- not list it, and the packages leave it out.

function conf(c)
  c.voxcade = "0.7.0"
  c.title = "Studio"

  -- The level is levels/studio.toml: the machine (512 voxels across, in
  -- fine voxels) on a table, all of it in view with a margin at 1280 by
  -- 720 as at 1920 by 1080; a narrow field of view, so its front row is
  -- hardly bigger than its back (2.14 and 1.9 pixels a voxel at 720).
  c.world.size = nil
  c.camera.pitch = 70
  c.camera.fov = 25
  c.camera.view_width = 640
  c.light.sun = { 0.3, 0.5, -1.0 }
  c.light.ambient = 0.45
  c.light.sky = "#18161a"

  -- The kit: the library's RG-80, a file of instruments with no song of
  -- its own; main.lua lays its channels over it.
  c.audio.music = { "music/rg80.mml" }

  -- The mouse plays the machine's parts; the keys and the pad still work:
  -- the arrows move the cursor along the steps (up and down turn the
  -- voice selector), TOGGLE sets or clears a step, PATTERN switches at
  -- the next bar, START starts and stops.
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

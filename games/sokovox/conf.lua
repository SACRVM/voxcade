-- Sokovox: Sokoban in voxels. Push every crate onto a goal: crates go
-- only where they are pushed, one at a time. The puzzles are David W.
-- Skinner's Microban (levels/README.txt). The levels are built in code
-- (main.lua); the models come from tools/gen_sokovox.py.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "Sokovox"
  c.languages = { "en", "de", "fr" }   -- the menus speak them
  c.world.size = { 288, 180, 24 }   -- a world made in code: 32 by 20 squares of 9 voxels
  c.player.spawn = { 144, 90 }      -- where the camera starts: the middle

  -- A step slides a square at a steady speed and stops dead.
  c.player.accel = 100000
  c.player.air_accel = 100000
  c.physics.body_width = 6

  c.camera.pitch = 72               -- steeply from above: nothing tall hides the square behind it
  c.camera.view_width = 207         -- 20 by 12 squares fit; bigger levels follow the keeper
  c.camera.snap = true
  c.title_screen.at = { 116, 97 }   -- the first level right of the menu, under the logo
  c.title_screen.view_width = 169
  c.title_screen.light_menu = true   -- the menu lies over the dark sky: light letters

  c.light.sun = { 0.35, 0.55, -1.0 }
  c.light.ambient = 0.55
  c.light.shadow = 0.5
  c.light.sky = { 0.05, 0.05, 0.07 }
  c.light.shadow_style = "voxel"
  c.light.model_shading = "faces"
  c.effects.tilt_shift = 0.0

  c.actions = { "undo", "restart", "next", "previous" }
  c.keys = {
    undo = { "Z", "Y", "Backspace" },   -- Z and Y: the key says Z on US and German keyboards alike
    restart = { "R" },
    next = { "N" },
    previous = { "B" },               -- back (P pauses)
  }
  c.pad = {
    undo = { "east" },
    restart = { "north" },
    next = { "rightshoulder" },
    previous = { "leftshoulder" },
  }

  -- Points for levels solved (main.lua); the high score table to beat.
  c.scored = true
  c.high_scores = {
    { "BOX", 15000 }, { "PSH", 12000 }, { "CRT", 10000 }, { "MOV", 8000 }, { "GOL", 6000 },
    { "SOK", 5000 }, { "VOX", 4000 }, { "UND", 3000 }, { "KEP", 2000 }, { "TRY", 1000 },
  }

  c.things.keeper = { model = "keeper.vvx" }
  c.things.crate = { model = "crate.vvx" }

  -- Calm songs from the library (music/, made by tools/compose.py); keys
  -- 1 and 2 switch, the title plays the first.
  c.audio.music = { "music/crates_at_dawn.mml", "music/quiet_warehouse.mml" }
  c.audio.music_volume = 0.4

  -- the sounds are files, sounds/NAME.toml, played by their names (level:sound)
end

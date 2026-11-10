-- Sokovox: Sokoban in voxels. Push every crate onto a goal: crates go
-- only where they are pushed, one at a time. The puzzles are David W.
-- Skinner's Microban (levels/README.txt). The levels are built in code
-- (main.lua); the models come from tools/gen_sokovox.py.

function conf(c)
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

  c.things.keeper = { model = "keeper.vox" }
  c.things.crate = { model = "crate.vox" }

  -- Calm songs from the library (music/, made by tools/compose.py); keys
  -- 1 and 2 switch, the title plays the first.
  c.audio.music = { "music/crates_at_dawn.mml", "music/quiet_warehouse.mml" }
  c.audio.music_volume = 0.4

  c.sounds.step = { wave = "noise", freq = 2400, freq_end = 900, sustain = 0.005, decay = 0.04, lowpass = 2000, volume = 0.12 }
  c.sounds.push = { wave = "noise", freq = 700, freq_end = 300, attack = 0.01, sustain = 0.08, decay = 0.08, lowpass = 900, volume = 0.3 }
  c.sounds.home = { wave = "triangle", freq = 784, sustain = 0.06, decay = 0.25, arp = 1.5, arp_at = 0.05, volume = 0.35 }
  c.sounds.solved = { wave = "square", freq = 523, duty = 0.3, sustain = 0.3, decay = 0.6, arp = 1.26, arp_at = 0.08, vibrato = 0.01, volume = 0.25 }
  c.sounds.undo = { wave = "square", freq = 660, freq_end = 330, duty = 0.25, sustain = 0.02, decay = 0.06, volume = 0.12 }
end

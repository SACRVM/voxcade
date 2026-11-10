-- Voxkeep: a jump & run in a castle at night, made as voxels from the
-- start (tools/gen_voxkeep.py first built the blocks, the figures and the
-- rooms). A demo of six rooms, one after the other (levels/), with
-- puzzles and timing. Everything stays in one lane: what is too high for
-- one jump takes a second in the air, at the top of the first.
function conf(c)
  c.title = "Voxkeep"
  c.languages = { "en", "de", "fr" }   -- the menus speak them; the keys shown at the start too
  c.kit = "platformer"
  c.scored = true                 -- it keeps a score: the high score table
  c.high_scores = {               -- the table to beat before anyone played (all six rooms: about 8600)
    { "KEY", 10000 }, { "GEM", 9000 }, { "BAT", 8000 }, { "OWL", 7000 }, { "ORB", 6000 },
    { "ARK", 5000 }, { "RUN", 4000 }, { "HOP", 3000 }, { "MOS", 2000 }, { "RAT", 1000 },
  }
  c.state.player = { hp = 3, lives = 3, taken = 0 }  -- what a run starts with: hit points (the kit's), lives and pickups taken (main.lua's)

  -- The rooms: levels of voxel blocks 10 each way (levels/*.json, played
  -- in the order of their names; blocks/), the lane in front, the wall
  -- behind; the camera rests on all of one. The editor opens the first.
  c.world.blocks = { file = "levels/01_hall.json" }
  c.player.spawn = { 15, 5 }

  -- A room started over (a crate pushed where it helps no more): R, or
  -- the pad's back button; a lever pulled: E, or the pad's left button
  -- (main.lua).
  c.actions = { "restart", "interact" }
  c.keys = { restart = { "R" }, interact = { "E" } }
  c.pad = { restart = { "back" }, interact = { "west" } }

  c.physics.gravity = 400
  c.physics.max_fall = 250
  c.physics.body_width = 6

  -- The hero jumps two cells high; once more in the air (air_jumps), 18
  -- voxels from where he is then: three cells with the first jump held
  -- and the second in its upper half, four never. He grabs no ledges.
  c.things.hero = {
    behaviour = "hero", model = "hero.vox", height = 14,
    speed = 60, accel = 900, air_accel = 700, jump = 22, look_ahead = 0, air_jumps = 1, air_jump = 18,
    climb = 40, corner = 8,
    knockback = { 100, 100 }, hang_speed = 30, look_ahead_speed = 90,   -- as on the fox (unset: scaled to 14 high)
    frames = { idle = { 0, 1 }, run = { 2, 5 }, jump = { 6, 6 }, fall = { 7, 7 },
               hang = { 8, 8 }, pull = { first = 9, last = 11, loop = false }, climb = { 12, 13 } },
  }
  -- What a touch means is main.lua's: the families and props say what a
  -- thing is to it, sound and effect what a thing does as it goes (the
  -- enemies burst into their voxels, which fall and are gone).
  c.things.slime = { behaviour = "walker", model = "slime.vox", width = 8, height = 7, speed = 25,
                     family = "enemies", props = { worth = 100 }, sound = "stomp", effect = "burst",
                     frames = { walk = { 0, 1 } }, fps = 4 }
  c.things.bat = { behaviour = "flyer", model = "bat.vox", width = 8, height = 10, range = 12, period = 90,
                   family = "enemies", props = { worth = 100 }, sound = "stomp", effect = "burst",
                   face_hero = false, facing = { 0, -1 },   -- it looks to the front, always, wings spread
                   frames = { fly = { 0, 1 } }, fps = 8 }
  c.things.gem = { behaviour = "pickup", model = "gem.vox", width = 6, height = 7,
                   spin = 120,   -- it turns round, a third of a turn a second
                   family = "pickups", props = { worth = 100 }, sound = "pickup" }
  c.things.door = { behaviour = "prop", stands = true, model = "door.vox", width = 8, height = 20,
                    frames = { shut = { 0, 0 }, open = { 1, 1 } }, props = { worth = 1000 } }
  c.things.torch = { behaviour = "prop", model = "torch.vox", width = 2, height = 13,
                     frames = { idle = { 0, 1 } }, fps = 6 }
  c.things.skull = { behaviour = "hopper", model = "skull.vox", width = 6, height = 7, every = 45, hop = { 40, 120 },
                     family = "enemies", props = { worth = 150 }, sound = "stomp", effect = "burst",
                     frames = { idle = { 0, 0 }, jump = { 1, 1 } } }

  -- The puzzles (main.lua says what they do): a lever (lever_N, by the
  -- order they lie in) sets the slabs and lifts of its number to work; a
  -- plate (plate_N) holds the hatch of its number open while something
  -- lies on it, the hero or a crate he pushed there.
  c.things.lever = { behaviour = "prop", stands = true, model = "lever.vox", width = 6, height = 9,
                     frames = { idle = { 0, 0 }, on = { 1, 1 } } }
  c.things.plate = { behaviour = "prop", stands = true, model = "plate.vox", width = 10, height = 3,
                     frames = { idle = { 0, 0 }, down = { 1, 1 } } }
  c.things.crate = { behaviour = "crate", model = "crate.vox", width = 8, height = 8, push = 25 }
  c.things.slab = { behaviour = "platform", model = "slab.vox", travel = { 30, 0 }, speed = 40, pause = 0 }  -- out of a wall
  c.things.slab_left = { like = "slab", travel = { -30, 0 } }
  c.things.lift = { behaviour = "platform", model = "slab.vox", travel = { 0, 40 }, speed = 25, pause = 45 }
  c.things.ferry = { behaviour = "platform", model = "slab.vox", travel = { 50, 0 }, speed = 35, pause = 12 }
  c.things.ferry_left = { like = "ferry", travel = { -50, 0 } }
  c.things.hatch = { behaviour = "platform", model = "hatch.vox", travel = { -20, 0 }, speed = 60, pause = 0 }

  c.title_screen.at = { 120, 5 }
  c.title_screen.view_width = 280
  c.title_screen.light_menu = true
  c.title_screen.logo = { "VOXKEEP" }
  c.title_screen.logo_colors = { "#e8a040" }

  c.audio.music = { "music/under_the_dunes.mml" }
  c.audio.music_volume = 0.4

  -- Seen from a little above, the whole room at once.
  c.camera.pitch = 10
  c.camera.fov = 40
  c.camera.view_width = 270
  c.camera.stop_at_edges = true

  -- Night: a dim cool light from the front, the torches warm (main.lua).
  c.light.sun = { 0.3, 0.45, -1.0 }
  c.light.ambient = 0.3
  c.light.shadow = 0.55
  c.light.sky = { 0.03, 0.03, 0.05 }
  c.light.shadow_style = "voxel"
  c.effects.lights = 2.0

  -- the HUD's pictures (main.lua draws the HUD): a letter a colour, . empty
  c.icons.heart = {
    rows = { ".RR.RR.", "RWRRRRR", "RRRRRRR", ".RRRRR.", "..RRR..", "...R..." },
    colors = { R = { 214, 36, 44 }, W = { 255, 176, 170 } },
  }
  c.icons.empty_heart = {
    rows = { ".KK.KK.", "K..K..K", "K.....K", ".K...K.", "..K.K..", "...K..." },
    colors = { K = { 96, 70, 64 } },
  }
end

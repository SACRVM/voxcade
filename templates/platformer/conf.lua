-- A jump & run with the platformer kit: one room of voxel blocks
-- (level.json and blocks/, the level editor's: EDIT on level.json), a
-- hero who runs, jumps and climbs, a bat, a slime, gems and the door out.
-- Saved while the game runs, a setting takes effect at once; the kit, the
-- level and where the hero starts after a restart (F5). Lengths are in
-- voxels, speeds in voxels a second.
function conf(c)
  c.title = "My Game"
  c.kit = "platformer"
  c.players = 2                   -- two can play: a second joins with START on a pad (or Return)
  c.scored = true                 -- it keeps a score: the high score table
  c.state.player = { hp = 3, lives = 3, taken = 0 }  -- what a run starts with: hit points (the kit's), lives and pickups taken (main.lua's)

  -- The room: a level of voxel blocks 10 each way (level.json, blocks/),
  -- the lane in front, the wall behind; the camera rests on all of it.
  c.world.blocks = { file = "level.json" }
  c.player.spawn = { 15, 5 }

  c.physics.gravity = 400
  c.physics.max_fall = 250
  c.physics.body_width = 6

  -- The hero jumps two cells high; a ledge three cells up he catches and
  -- pulls himself onto at once, one four cells up he hangs from until up
  -- pulls him up (grab: how far above his head his hands reach; grab_ahead:
  -- how far ahead of him he finds the edge; hang_below: his head under it).
  c.things.hero = {
    behaviour = "hero", model = "hero.vox", height = 14,
    speed = 60, accel = 900, air_accel = 700, jump = 22, look_ahead = 0, grab = 6, grab_ahead = 3, hang_below = 2, pull = 24,
    climb = 40, corner = 8,
    knockback = { 100, 100 }, hang_speed = 30, look_ahead_speed = 90,   -- as on the fox (unset: scaled to 14 high)
    frames = { idle = { 0, 1 }, run = { 2, 5 }, jump = { 6, 6 }, fall = { 7, 7 },
               hang = { 8, 8 }, pull = { first = 9, last = 11, loop = false }, climb = { 12, 13 } },
  }
  -- The second player's hero: the same, in other colours.
  c.things.hero2 = { like = "hero", model = "hero2.vox" }
  c.player.kind = { "hero", "hero2" }
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

  c.title_screen.at = { 120, 5 }
  c.title_screen.view_width = 280
  c.title_screen.light_menu = true
  c.title_screen.logo = { "MY GAME" }
  c.title_screen.logo_colors = { "#e8a040" }

  c.audio.music = { "music/under_the_dunes.mml" }
  c.audio.music_volume = 0.4

  -- Seen from a little above, the whole room at once.
  c.camera.pitch = 10
  c.camera.fov = 40
  c.camera.view_width = 270
  c.camera.stop_at_edges = true
  c.camera.hold = { 230 }         -- two heroes stay this far apart at most: both on the screen

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

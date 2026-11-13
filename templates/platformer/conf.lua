-- The game's settings: a jump & run made with the platformer kit.
-- One room built from voxel blocks (level.toml and blocks/; press EDIT
-- on level.toml to change it). A hero who runs, jumps and climbs, a bat,
-- a slime, gems and a door.
-- Save while the game runs and most settings change at once. The kit,
-- the level and where the hero starts change after a restart (F5).
-- Lengths are in voxels. Speeds are in voxels per second.
function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade version these files are for
  c.title = "My Game"
  c.kit = "platformer"
  c.players = 2                   -- up to two players
  c.scored = true                 -- keep a high score table
  c.state.player = { hp = 3, lives = 3, taken = 0 }  -- each player at the start: hearts (main.lua's HEARTS), lives, gems taken

  -- The room: a level of voxel blocks, each 10 voxels wide (level.toml,
  -- blocks/). The players walk in front of the back wall.
  c.world.blocks = { file = "level.toml" }
  c.player.spawn = { 15, 5 }

  c.physics.gravity = 400
  c.physics.max_fall = 250
  c.physics.body_width = 6

  -- The hero jumps two blocks high. He grabs a ledge three blocks up and
  -- pulls himself onto it. From a ledge four blocks up he hangs until you
  -- press up. grab: how far above his head he reaches. grab_ahead: how
  -- far in front he finds an edge. hang_below: how far his head hangs
  -- below the edge.
  c.things.hero = {
    behavior = "hero", model = "hero.vvx", height = 14,
    speed = 60, accel = 900, air_accel = 700, jump = 22, look_ahead = 0, grab = 6, grab_ahead = 3, hang_below = 2, pull = 24,
    climb = 40, corner = 8,
    knockback = { 100, 100 }, hang_speed = 30, look_ahead_speed = 90,   -- left out, these grow with the hero's height
    frames = { idle = { 0, 1 }, run = { 2, 5 }, jump = { 6, 6 }, fall = { 7, 7 },
               hang = { 8, 8 }, pull = { first = 9, last = 11, loop = false }, climb = { 12, 13 } },
  }
  -- The second player's hero: the same, in other colors.
  c.things.hero2 = { like = "hero", model = "hero2.vvx" }
  c.player.kind = { "hero", "hero2" }
  -- What a touch does is up to main.lua. family and props tell it what
  -- a thing is. sound and effect play when a thing is beaten: enemies
  -- burst into voxels that fall away.
  c.things.slime = { behavior = "walker", model = "slime.vvx", width = 8, height = 7, speed = 25,
                     family = "enemies", props = { worth = 100 }, sound = "stomp", effect = "burst",
                     frames = { walk = { 0, 1 } }, fps = 4 }
  c.things.bat = { behavior = "flyer", model = "bat.vvx", width = 8, height = 10, range = 12, period = 90,
                   family = "enemies", props = { worth = 100 }, sound = "stomp", effect = "burst",
                   face_hero = false, facing = { 0, -1 },   -- always faces the camera, wings spread
                   frames = { fly = { 0, 1 } }, fps = 8 }
  c.things.gem = { behavior = "pickup", model = "gem.vvx", width = 6, height = 7,
                   spin = 120,   -- spins: 120 degrees a second
                   family = "pickups", props = { worth = 100 }, sound = "pickup" }
  c.things.door = { behavior = "prop", stands = true, model = "door.vvx", width = 8, height = 20,
                    frames = { shut = { 0, 0 }, open = { 1, 1 } }, props = { worth = 1000 } }
  c.things.torch = { behavior = "prop", model = "torch.vvx", width = 2, height = 13,
                     frames = { idle = { 0, 1 } }, fps = 6 }

  c.title_screen.at = { 120, 5 }
  c.title_screen.view_width = 280
  c.title_screen.light_menu = true
  c.title_screen.logo = { "MY GAME" }
  c.title_screen.logo_colors = { "#e8a040" }

  c.audio.music = { "music/under_the_dunes.mml" }
  c.audio.music_volume = 0.4

  -- The camera looks from a little above and shows the whole room.
  c.camera.pitch = 10
  c.camera.fov = 40
  c.camera.view_width = 270
  c.camera.stop_at_edges = true
  c.camera.hold = { 230 }         -- how far apart two heroes can get, so both stay on screen

  -- Night: a dim, cool light from the front. The torches are warm
  -- (main.lua lights them).
  c.light.sun = { 0.3, 0.45, -1.0 }
  c.light.ambient = 0.3
  c.light.shadow = 0.55
  c.light.sky = { 0.03, 0.03, 0.05 }
  c.light.shadow_style = "voxel"
  c.effects.lights = 2.0

  -- Small pictures for the HUD (main.lua draws them). Each letter is a
  -- color; a dot is empty.
  c.icons.heart = {
    rows = { ".RR.RR.", "RWRRRRR", "RRRRRRR", ".RRRRR.", "..RRR..", "...R..." },
    colors = { R = { 214, 36, 44 }, W = { 255, 176, 170 } },
  }
  c.icons.empty_heart = {
    rows = { ".KK.KK.", "K..K..K", "K.....K", ".K...K.", "..K.K..", "...K..." },
    colors = { K = { 96, 70, 64 } },
  }
end

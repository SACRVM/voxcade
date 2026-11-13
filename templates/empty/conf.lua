-- The game's settings. There is no kit: main.lua makes all the rules.
-- One or two can play. A second player joins with START on a gamepad or
-- Return on the keyboard.
-- Save while the game runs and most settings change at once. The players
-- and where they start change after a restart (F5).
-- Lengths are in voxels. Speeds are in voxels per second.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade version these files are for
  c.title = "My Game"
  c.players = 2                   -- up to two players

  -- The world is built in code (main.lua's on_level_start). Its size:
  c.world.size = { 96, 64, 32 }
  c.player.spawn = { 48, 32 }     -- where the players start: x, y on the ground

  -- Kinds of things. Each has a model you can reshape in the voxel editor
  -- (double-click the file). A thing is as big as its model.
  c.things.hero = { model = "hero.vvx" }
  c.things.hero2 = { model = "hero2.vvx" }
  -- Each player is a thing of one of these kinds.
  c.player.kind = { "hero", "hero2" }

  c.camera.pitch = 50
  c.camera.view_width = 120
  c.camera.hold = { 100, 50 }     -- how far apart two players can get: across, deep

  c.title_screen.logo = { "MY GAME" }
end

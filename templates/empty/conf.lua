-- A game in Lua alone: no kit, the script makes the rules (main.lua).
-- One plays, or two (a second player joins with START on a pad, or
-- Return at the keyboard). Saved while the game runs, a setting takes
-- effect at once; the players and where they start after a restart (F5).
-- Lengths are in voxels, speeds in voxels a second.

function conf(c)
  c.voxcade = "0.6.0"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)
  c.title = "My Game"
  c.players = 2                   -- seats: a second player may join

  -- A world made in code: this big, built by main.lua's on_level_start.
  c.world.size = { 96, 64, 32 }
  c.player.spawn = { 48, 32 }     -- x, y on the ground

  -- Kinds of things: these have a model, a small figure to reshape in the
  -- voxel editor (a double click on the file); a thing is as big as its
  -- model. main.lua draws them (frame:model).
  c.things.hero = { model = "hero.vvx" }
  c.things.hero2 = { model = "hero2.vvx" }
  -- The players: each a thing of its kind, standing where each level starts.
  c.player.kind = { "hero", "hero2" }

  c.camera.pitch = 50
  c.camera.view_width = 120
  c.camera.hold = { 100, 50 }     -- how far apart two players may be, across and deep

  c.title_screen.logo = { "MY GAME" }
end

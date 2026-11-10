-- A game in Lua alone: no kit, the script makes the rules (main.lua).
-- One plays, or two (a second player joins with START on a pad, or
-- Return at the keyboard). Saved while the game runs, a setting takes
-- effect at once; the players and where they start after a restart (F5).
-- Lengths are in voxels, speeds in voxels a second.

function conf(c)
  c.title = "My Game"
  c.players = 2                   -- seats: a second player may join

  -- A world made in code: this big, built by main.lua's on_level_start.
  c.world.size = { 96, 64, 24 }
  c.player.spawn = { 48, 32 }

  -- Kinds of things: this one has no model, main.lua draws it.
  c.things.hero = {}
  -- The players: each a thing of this kind, standing where each level starts.
  c.player.kind = "hero"

  c.camera.pitch = 50
  c.camera.view_width = 120
  c.camera.hold = { 100, 50 }     -- how far apart two players may be, across and deep

  c.title_screen.logo = { "MY GAME" }
end

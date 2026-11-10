-- NEW GAME: what it is, in a line or two. (Copied from the voxcade-lua-game
-- skill's template: a world made in code, the player, walked by main.lua.)
function conf(c)
  c.title = "New Game"
  c.world.size = { 128, 128, 24 }     -- a world made in code: main.lua builds it
  c.player.spawn = { 64, 64 }         -- where the player starts
  c.camera.pitch = 50
  c.camera.view_width = 140

  c.actions = { "jump" }              -- besides the four directions
  c.keys = { jump = { "Space", "Z" } } -- places on a US keyboard, labelled as the player's prints them
  c.pad = { jump = { "south" } }

  c.things.hero = { hp = 3 }          -- a kind without a behaviour: main.lua moves it
  c.player.kind = "hero"              -- the player is one: player.state.hp is its hits
end

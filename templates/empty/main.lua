-- An empty game: some ground, and players who walk around on it.
-- Anything that changes during play is kept in a state:
--   world.state   for the whole run
--   level.state   for the level being played
--   player.state  for each player (p.state)
--   thing.state   for each thing
-- HELP (top right) has whole examples to copy. "voxcade api" lists every
-- name. Ready for more? Start a new game from the JUMP & RUN template:
-- it adds jumping, enemies, pickups and a goal.

-- The ground, in layers from the bottom up: a color and a thickness in
-- voxels. Digging or a blast uncovers the layer below.
local GROUND = {
  { "#6f6f73", 4 },   -- stone
  { "#6b5138", 3 },   -- earth
  { "#5a6e4a", 1 },   -- grass
}

function on_level_start(how)
  local z = 0
  for _, layer in ipairs(GROUND) do
    local color, thick = layer[1], layer[2]
    level:fill(0, 0, z, level.size.x - 1, level.size.y - 1, z + thick - 1, color)
    z = z + thick
  end
end

function on_tick()
  -- level:players() lists everyone playing. p.input.dir is that
  -- player's arrow keys (or stick) as a direction.
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
end

-- Draw each player with its model (hero.vvx, hero2.vvx). Double-click a
-- model in the list to change it. A free seat says how to join.
function on_draw(frame)
  for _, p in ipairs(level:players()) do
    frame:model(p.seat == 2 and "hero2.vvx" or "hero.vvx", p)
  end
  for seat, p in ipairs(world.players) do
    if not p then
      local text = "PLAYER " .. seat .. ": PRESS START"
      frame:text(text, frame.width - 3 - frame:text_width(text), 4)
    end
  end
end

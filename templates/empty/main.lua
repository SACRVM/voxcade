-- A ground, and the players their keys walk about. Everything that
-- changes while playing goes into a state: world.state for the run,
-- level.state for the level being played and its things, player.state
-- (p.state: each player's), thing.state; voxcade api lists what there is.

-- The ground in layers, from the bottom up: each a colour and how many
-- voxels thick. Whatever digs or blasts into it lays the next one bare.
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
  -- level:players(): those playing, each with its own keys; p.input.dir
  -- is its arrow keys (or its stick) as a direction
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
end

-- Each player's figure, a model (hero.vox, hero2.vox: reshape them in
-- the voxel editor): frame:model draws it where the player is on its way
-- from tick to tick, so it glides. PLAYER 2: PRESS START at the top right while a
-- seat is free.
function on_draw(frame)
  for _, p in ipairs(level:players()) do
    frame:model(p.seat == 2 and "hero2.vox" or "hero.vox", p)
  end
  for seat, p in ipairs(world.players) do
    if not p then
      local text = "PLAYER " .. seat .. ": PRESS START"
      frame:text(text, frame.width - 3 - frame:text_width(text), 4)
    end
  end
end

-- New Game's rules. Everything that changes while playing lives in a
-- state (world.state for the run, level.state for the level and its
-- things, player.state, thing.state); top-level locals are constants.

local SPEED = 40          -- voxels a second
local JUMP = 8            -- voxels high
local HERO = { 230, 140, 60 }

function on_level_start(how)
  level:fill(0, 0, 0, 127, 127, 0, "#3b4150")      -- the floor
  level:fill(20, 20, 1, 27, 27, 6, "#8d9093")      -- a block to jump on
end

function on_tick()
  player:walk(input.dir, SPEED)
  if input:pressed("jump") then player:jump(JUMP) end
end

function on_draw(frame)
  -- No model yet: a 3 by 3 by 6 column of voxels where it stands.
  for z = 0, 5 do
    for dx = -1, 1 do
      for dy = -1, 1 do
        frame:cube(player.x + dx, player.y + dy, player.z + z, HERO)
      end
    end
  end
  frame:text("NEW GAME", 4, 4)
end

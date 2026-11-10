-- A floor, and the players their keys walk about. Everything that
-- changes while playing goes into a state: world.state for the run,
-- level.state for the level being played and its things, player.state
-- (p.state: each player's), thing.state; voxcade api lists what there is.

-- Each seat's colour: player 1 red, player 2 blue.
local COLORS = { { 200, 90, 60 }, { 60, 110, 200 } }

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
end

function on_tick()
  -- level:players(): those playing, each with its own keys; p.input.dir
  -- is its arrow keys (or its stick) as a direction
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
end

-- Each player as cubes, a voxel each: a body in its seat's colour and a
-- head; PRESS START at the top right while a seat is free.
function on_draw(frame)
  for _, p in ipairs(level:players()) do
    for dx = -1, 1 do
      for dy = -1, 1 do
        for z = 0, 4 do
          frame:cube(p.x + dx, p.y + dy, p.z + z, COLORS[p.seat])
        end
        for z = 5, 7 do
          frame:cube(p.x + dx, p.y + dy, p.z + z, { 240, 210, 180 })
        end
      end
    end
  end
  for _, p in ipairs(world.players) do
    if not p then
      local text = "PRESS START"
      frame:text(text, frame.width - 3 - frame:text_width(text), 4)
    end
  end
end

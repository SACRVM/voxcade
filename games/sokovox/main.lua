-- Sokovox: push every crate onto a goal. The levels are David W. Skinner's
-- Microban, read from levels/microban.txt as the game starts and built in
-- voxels when each begins. What the run remembers (the puzzle being
-- played, the puzzles solved) lives in world.state, the score in
-- player.state; the puzzle being played (where the keeper and the crates
-- stand, the moves, what undo takes back, whether he moved yet) in
-- level.state. A puzzle solved scores 1000 less a point a move (100 at
-- least), once in a run. The keys show after the first step, the
-- keyboard's or the pad's, whichever moved him.

local TILE = 9                   -- voxels a square: a 7 wide crate and the 5 wide keeper sit in the middle
local WALL = 9                   -- how high walls stand
local STEP = 9                   -- ticks a step slides
local SPEED = TILE * 60 / STEP   -- voxels a second: one a tick
local VIEW_W, VIEW_H = 20, 12    -- squares the camera shows whole (146 of the 155 levels)
local WORLD_W, WORLD_H = 32, 20  -- the world in squares (c.world.size)
local SOLVED = 90                -- ticks "SOLVED" shows before the next level
local UNDO_FIRST, UNDO_AGAIN = 15, 4  -- a held undo waits, then repeats
local HOLD = 15                  -- ticks a held direction waits before the keeper walks on
local POINTS, LEAST = 1000, 100  -- a level solved: this less a point a move, at least that

local FLOOR = { "#3b4150", "#343947" }
local GOAL = "#e2b33c"
-- Walls of grey concrete, lighter on top.
local CONCRETE = { "#8d9093", "#878a8d", "#93969a", "#8a8d90" }
local CROWN = { "#a6a9ac", "#9fa2a5" }

local WAYS = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } }

-- A puzzle from its rows (# wall, space floor, . goal, $ crate, * crate on a
-- goal, @ keeper, + keeper on a goal), the first row the north. Squares
-- are numbered y * w + x from the south-west corner.
local function puzzle(name, rows)
  local L = { w = 0, h = #rows, walls = {}, goals = {}, crates = {} }
  local number, title = name:match("^(%d+)%s*(.*)$")
  L.number, L.title = tonumber(number), title:gsub("'", ""):upper()
  for _, row in ipairs(rows) do
    L.w = math.max(L.w, #row)
  end
  for r, row in ipairs(rows) do
    local y = L.h - r
    for c = 1, #row do
      local ch, i = row:sub(c, c), y * L.w + c - 1
      if ch == "#" then L.walls[i] = true end
      if ch == "." or ch == "*" or ch == "+" then L.goals[i] = true end
      if ch == "$" or ch == "*" then L.crates[#L.crates + 1] = i end
      if ch == "@" or ch == "+" then L.keeper = i end
    end
  end
  -- The floor: every square the keeper can reach.
  L.floor = { [L.keeper] = true }
  local todo = { L.keeper }
  while #todo > 0 do
    local i = table.remove(todo)
    local x, y = i % L.w, i // L.w
    for _, d in ipairs(WAYS) do
      local nx, ny = x + d[1], y + d[2]
      local n = ny * L.w + nx
      if nx >= 0 and nx < L.w and ny >= 0 and ny < L.h and not L.walls[n] and not L.floor[n] then
        L.floor[n] = true
        todo[#todo + 1] = n
      end
    end
  end
  return L
end

-- Skinner's file: "; NAME" before each level, then its rows.
local function parse(text, author)
  local levels, name, rows = {}, nil, nil
  local function finish()
    if rows and #rows > 0 then
      local L = puzzle(name, rows)
      L.author = author
      levels[#levels + 1] = L
      rows = nil
    end
  end
  for line in (text .. "\n"):gmatch("([^\n]*)\n") do
    line = line:gsub("\r$", "")
    if line:sub(1, 1) == ";" then
      finish()
      name, rows = line:sub(2):gsub("^%s+", ""), {}
    elseif line:match("^%s*$") then
      finish()
    elseif rows then
      rows[#rows + 1] = line
    end
  end
  finish()
  return levels
end

local LEVELS = parse(world:read("levels/microban.txt"), "DAVID W. SKINNER")

-- The square at x, y of a level, if it lies inside.
local function square(L, x, y)
  if x >= 0 and x < L.w and y >= 0 and y < L.h then return y * L.w + x end
end

-- Where a level lies in the world (its south-west corner, in voxels): in
-- the middle.
local function origin(L)
  return (WORLD_W - L.w) // 2 * TILE, (WORLD_H - L.h) // 2 * TILE
end

-- The middle of square i, in voxels.
local function centre(L, i)
  local ox, oy = origin(L)
  return ox + i % L.w * TILE + TILE / 2, oy + i // L.w * TILE + TILE / 2
end

-- A speck of grey for a voxel, the same every time.
local function speck(x, y, z)
  local h = (x * 374761393 + y * 668265263 + z * 1274126177) & 0x7fffffff
  h = ((h ~ (h >> 13)) * 1274126177) & 0x7fffffff
  return h ~ (h >> 16)
end

-- A wall's voxel at (x, y, z): speckled, the crown lighter.
local function concrete(x, y, z)
  local s = speck(x, y, z)
  if z == WALL then return CROWN[s % #CROWN + 1] end
  return CONCRETE[s % #CONCRETE + 1]
end

-- Lays a level's floor, goals and walls.
local function build(L)
  local ox, oy = origin(L)
  for i in pairs(L.floor) do
    local x, y = ox + i % L.w * TILE, oy + i // L.w * TILE
    local floor = FLOOR[(i % L.w + i // L.w) % 2 + 1]
    level:fill(x, y, 0, x + TILE - 1, y + TILE - 1, 0, floor)
    if L.goals[i] then
      -- A goal: a yellow frame as big as a crate.
      level:fill(x + 1, y + 1, 0, x + TILE - 2, y + TILE - 2, 0, GOAL)
      level:fill(x + 2, y + 2, 0, x + TILE - 3, y + TILE - 3, 0, floor)
    end
  end
  for i in pairs(L.walls) do
    local x, y = ox + i % L.w * TILE, oy + i // L.w * TILE
    level:fill(x, y, 0, x + TILE - 1, y + TILE - 1, WALL, CONCRETE[1])
    -- Only the shell shows: its voxels get the pattern.
    for lx = 0, TILE - 1 do
      for ly = 0, TILE - 1 do
        local edge = lx == 0 or ly == 0 or lx == TILE - 1 or ly == TILE - 1
        for z = edge and 1 or WALL, WALL do
          level:set_voxel(x + lx, y + ly, z, concrete(x + lx, y + ly, z))
        end
      end
    end
  end
end

-- Starts puzzle n: the last one's voxels and things go, this one is built.
local function start(n)
  local s = level.state
  if s.box then
    level:fill(s.box[1], s.box[2], 0, s.box[3], s.box[4], WALL, nil)
  end
  for _, t in ipairs(level:things()) do
    t:remove()
  end
  local L = LEVELS[n]
  build(L)
  local ox, oy = origin(L)
  s.box = { ox, oy, ox + L.w * TILE - 1, oy + L.h * TILE - 1 }
  world.state.puzzle = n
  s.moves, s.pushes, s.undo_wait = 0, 0, 0
  s.queued = nil
  s.history, s.step, s.solved = {}, nil, nil
  s.at = L.keeper
  s.keeper = level:spawn("keeper", centre(L, L.keeper))
  s.keeper:face({ x = 0, y = -1 })
  s.crates = {}
  for _, i in ipairs(L.crates) do
    s.crates[i] = level:spawn("crate", centre(L, i))
  end
  if L.w <= VIEW_W and L.h <= VIEW_H then
    level.camera:look_at(ox + L.w * TILE / 2, oy + L.h * TILE / 2)
  else
    level.camera:follow(s.keeper)
  end
end

local function solved(s, L)
  for i in pairs(L.goals) do
    if not s.crates[i] then return false end
  end
  return true
end

-- A step ends: the keeper and the crate stand exactly on their squares.
local function arrive(s, L)
  local step = s.step
  s.keeper:place(centre(L, s.at))
  if step.crate then
    step.crate:place(centre(L, step.to))
    if L.goals[step.to] then level:sound("home", step.crate) end
  end
  s.step = nil
  if solved(s, L) then
    s.solved = SOLVED
    level:sound("solved", s.keeper)
    local done = world.state.done
    if not done[world.state.puzzle] then
      done[world.state.puzzle] = true
      player.state.score = player.state.score + math.max(LEAST, POINTS - s.moves)
    end
  end
end

-- One tick of a step's slide.
local function slide(s)
  local step = s.step
  local way = { x = step.dx, y = step.dy }
  s.keeper:walk(way, SPEED)
  if step.crate then step.crate:walk(way, SPEED) end
  step.left = step.left - 1
end

-- The keeper steps the way given, pushing the crate there if the square
-- beyond is free; into a wall he only turns.
local function try_step(s, L, dx, dy)
  s.moved = true
  s.keeper:face({ x = dx, y = dy })
  local x, y = s.at % L.w, s.at // L.w
  local to = square(L, x + dx, y + dy)
  if not to or not L.floor[to] then return end
  local crate, beyond = s.crates[to], nil
  if crate then
    beyond = square(L, x + 2 * dx, y + 2 * dy)
    if not beyond or not L.floor[beyond] or s.crates[beyond] then return end
    s.crates[to], s.crates[beyond] = nil, crate
    s.pushes = s.pushes + 1
    level:sound("push", crate)
  else
    level:sound("step", s.keeper, 0.6)
  end
  s.at = to
  s.moves = s.moves + 1
  s.history[#s.history + 1] = { dx = dx, dy = dy, pushed = crate ~= nil }
  s.step = { dx = dx, dy = dy, left = STEP, crate = crate, to = beyond }
  slide(s)
end

-- Takes the last step back, the crate it pushed with it.
local function undo(s, L)
  local last = table.remove(s.history)
  if not last then return end
  local x, y = s.at % L.w, s.at // L.w
  if last.pushed then
    local at = square(L, x + last.dx, y + last.dy)
    local crate = s.crates[at]
    s.crates[at], s.crates[s.at] = nil, crate
    crate:place(centre(L, s.at))
    s.pushes = s.pushes - 1
  end
  s.at = square(L, x - last.dx, y - last.dy)
  s.keeper:place(centre(L, s.at))
  s.keeper:face({ x = last.dx, y = last.dy })
  s.moves = s.moves - 1
  level:sound("undo", s.keeper, 0.5)
end

function on_world_start()
  world.state.puzzle, world.state.done = 1, {}
end

function on_level_start(how)
  level.state.way_x, level.state.way_y, level.state.held_for = 0, 0, 0
  start(world.state.puzzle)
end

-- The way held this tick (one axis), how long it has been held, and a
-- press during a slide kept for when it ends: a press is one step, a
-- held way walks on after a moment.
local function steer(s)
  local d = input.dir
  local x, y = d.x, 0
  if x == 0 then y = d.y end
  if (x ~= 0 or y ~= 0) and x == s.way_x and y == s.way_y then
    s.held_for = s.held_for + 1
  else
    s.held_for = 0
  end
  s.way_x, s.way_y = x, y
  if s.held_for == 0 and (x ~= 0 or y ~= 0) and s.step then
    s.queued = { x = x, y = y }
  end
end

function on_tick()
  local s, n = level.state, world.state.puzzle
  local L = LEVELS[n]
  steer(s)
  if s.step and s.step.left == 0 then arrive(s, L) end
  if s.solved then
    s.solved = s.solved - 1
    if s.solved > 0 then return end
    if n == #LEVELS then
      world:win()
      return
    end
    return start(n + 1)
  end
  if input:pressed("restart") then return start(n) end
  if input:pressed("next") then return start(n % #LEVELS + 1) end
  if input:pressed("previous") then return start((n - 2) % #LEVELS + 1) end
  if s.step then return slide(s) end
  if input:pressed("undo") then
    s.undo_wait = UNDO_FIRST
    return undo(s, L)
  elseif input:held("undo") then
    s.undo_wait = s.undo_wait - 1
    if s.undo_wait <= 0 then
      s.undo_wait = UNDO_AGAIN
      undo(s, L)
    end
    return
  end
  if s.queued then
    local q = s.queued
    s.queued = nil
    return try_step(s, L, q.x, q.y)
  end
  if (s.way_x ~= 0 or s.way_y ~= 0) and (s.held_for == 0 or s.held_for >= HOLD) then
    try_step(s, L, s.way_x, s.way_y)
  end
end

function on_draw(frame)
  local s = level.state
  local L = LEVELS[world.state.puzzle]
  frame:model("keeper.vvx", s.keeper)
  for i, crate in pairs(s.crates) do
    local moving = s.step and s.step.crate == crate
    frame:model("crate.vvx", crate, (L.goals[i] and not moving) and 1 or 0)
  end
  local title = L.title ~= "" and "  " .. L.title or ""
  frame:text(("LEVEL %d/%d%s"):format(L.number, #LEVELS, title), 4, 4)
  frame:text(("MOVES %d  PUSHES %d"):format(s.moves, s.pushes), 4, 13)
  frame:text(("SCORE %06d"):format(player.state.score), 4, 22, { 255, 214, 90 })
  if s.solved then frame:text("SOLVED!", 4, 35, { 255, 214, 90 }) end
  -- The keys once the first step shows what the player plays with: the
  -- keyboard's or the pad's.
  if s.moved then
    frame:text("{undo} UNDO   {restart} RESTART   {previous}/{next} LEVEL", 4, 160, { 190, 190, 200 })
  end
  frame:text("LEVEL BY " .. L.author, 4, 170, { 140, 140, 152 })
end

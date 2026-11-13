-- A jump & run: run, jump and climb. Land on enemies to beat them.
-- Take every gem, and the door opens. Go through it to win.
-- One or two can play. A second player joins with START on a gamepad
-- or Return on the keyboard.

-- These are the game's rules. Change anything you like.
-- The kit (c.kit in conf.lua) moves the heroes and the things.
-- This file decides what a touch does, what a life is worth and when
-- the level is won.
-- level:players() lists everyone playing. thing.seat is set only on a
-- player, so it tells players from other things.

local BOUNCE = 50        -- how high a hero bounces off an enemy, in voxels
local HEARTS = 3         -- full hearts; keep it the same as hp in c.state.player (conf.lua)
local SAFE = 60          -- ticks a hero cannot be hurt after losing a life

-- Losing a life: back to the last checkpoint (or the start) with full
-- hearts. With no lives left the player is out. When everyone is out,
-- the game is over.
local function lose_life(p)
  p.state.lives = p.state.lives - 1
  level:sound("lose", p)
  if p.state.lives <= 0 then
    p.state.out = true
    if #level:players() == 0 then world:lose() end
    return
  end
  local back = p.state.checkpoint and level:thing(p.state.checkpoint)
  p:place(back or level.arrived)
  p.state.hp = HEARTS
  p.state.invulnerable = SAFE
end

-- A hero touches something: an enemy (from above), a pickup, a
-- checkpoint or the door. This runs once, when the touch begins.
function on_touch(a, b, where)
  if not a.seat then return end
  if b.families.enemies and where == "top" then
    b:hurt(1, a)
    a:bounce(BOUNCE)
    level.state.stomped = world.tick
  elseif b.families.pickups then
    a.state.score = a.state.score + b.props.worth
    a.state.taken = a.state.taken + 1
    b:remove()
  elseif b.kind == "checkpoint" and a.state.checkpoint ~= b.name then
    a.state.checkpoint = b.name
    level:sound("checkpoint", b)
  elseif b.kind == "door" and level.state.open and not level.state.out then
    level.state.out = true
    a.state.score = a.state.score + b.props.worth
    world:win()
  end
end

-- A beaten enemy gives its points to the player who beat it. A hero
-- with no hearts left loses a life.
function on_death(thing, by)
  if thing.seat then
    lose_life(thing)
  elseif thing.families.enemies and by and by.seat then
    by.state.score = by.state.score + thing.props.worth
  end
end

function on_fall_out(thing)
  if thing.seat then lose_life(thing) end
end

-- Every tick (60 a second): open the door when no gem is left, and hurt
-- a hero who touches an enemy from the side, spikes, or a hazard block.
-- After a hit the hero is safe for a moment (p.state.invulnerable).
function on_tick()
  if not level.state.open and level:count("gem") == 0 then
    level.state.open = true
    for _, door in ipairs(level:things("door")) do
      door.state.pose = "open"
      level:sound("door", door)
    end
  end
  if level.state.stomped == world.tick then return end
  for _, p in ipairs(level:players()) do
    local hurt_by
    for _, t in ipairs(p:touching()) do
      if t.families.enemies or t.families.hazards then hurt_by = t end
    end
    if hurt_by then
      p:hurt(1, hurt_by)
    elseif level:block(p.x, p.y, p.z + 1) == "hazard" then
      p:hurt(1)
    end
  end
end

-- The run is over: remember how it ended, for the HUD.
function on_world_end(result)
  world.state.over = { result = result, at = world.tick }
end

-- The HUD, drawn over the game: each player's hearts and lives in a top
-- corner (player 1 left, player 2 right) with the score and the gems
-- below. A free seat shows PLAYER 2: PRESS START. At the end, STAGE
-- CLEAR or GAME OVER drops in.
local TEXT, NUMBERS, RED = { 240, 230, 204 }, { 255, 214, 90 }, { 230, 58, 48 }

local function drop(frame, text, y, ticks, color, size)
  local x = math.floor((frame.width - frame:text_width(text, size)) / 2 + 0.5)
  frame:drop_in(text, x, y, ticks, color, size)
end

-- One player's corner. With `right` it goes to the right edge.
local function corner(frame, p, right)
  local function at(width) return right and frame.width - 3 - width or 3 end
  if p.state.out then
    frame:text("GAME OVER", at(frame:text_width("GAME OVER")), 4, RED)
    return
  end
  for i = 0, HEARTS - 1 do
    frame:icon(i < p.state.hp and "heart" or "empty_heart", at(9 * HEARTS - 2) + 9 * i, 4)
  end
  local lives = "x" .. p.state.lives
  frame:text(lives, at(frame:text_width(lives)), 13)
  local score, taken = string.format("%06d", p.state.score), p.state.taken .. " TAKEN"
  frame:text(score, at(frame:text_width(score)), 23, NUMBERS)
  frame:text(taken, at(frame:text_width(taken)), 33)
end

-- The keys, shown for the first ten seconds: the keyboard's or the
-- gamepad's (whichever the player uses), in the player's language.
local KEYS = {
  en = { keys = "ARROWS", pad = "STICK", "%s RUN AND CLIMB   {jump} JUMP" },
  de = { keys = "PFEILTASTEN", pad = "STICK", "%s LAUFEN UND KLETTERN   {jump} SPRINGEN" },
  fr = { keys = "FLÈCHES", pad = "STICK", "%s COURIR ET GRIMPER   {jump} SAUT" },
}
local function show_keys(frame)
  if world.tick >= 600 then return end
  local t = KEYS[world.language] or KEYS.en
  local move = controls.device == "gamepad" and t.pad or t.keys
  for i, line in ipairs(t) do
    line = line:format(move)
    frame:text(line, math.floor((frame.width - frame:text_width(line)) / 2 + 0.5), frame.height - 22 + 10 * i, TEXT)
  end
end

local function hud(frame)
  show_keys(frame)
  local half = frame.height / 2
  if #world.players == 1 then
    -- One player: hearts and lives on the left, score and gems right.
    local p = player.state
    for i = 0, HEARTS - 1 do
      frame:icon(i < p.hp and "heart" or "empty_heart", 3 + 9 * i, 4)
    end
    frame:text("x" .. p.lives, 3, 13)
    local score, taken = string.format("%06d", p.score), p.taken .. " TAKEN"
    frame:text(score, frame.width - 3 - frame:text_width(score), 3, NUMBERS)
    frame:text(taken, frame.width - 3 - frame:text_width(taken), 13)
  else
    for seat, p in ipairs(world.players) do
      if p then
        corner(frame, p, seat == 2)
      elseif world.tick // 30 % 2 == 0 then
        local text = "PLAYER " .. seat .. ": PRESS START"
        frame:text(text, seat == 2 and frame.width - 3 - frame:text_width(text) or 3, 4)
      end
    end
  end
  local over = world.state.over
  local score = player.state.score
  for seat = 2, #world.players do
    local p = world.players[seat]
    if p then score = score + p.state.score end
  end
  if over and over.result == "won" then
    local t = world.tick - over.at + frame.alpha
    drop(frame, "STAGE CLEAR", half - 22, t, NUMBERS, 2)
    drop(frame, "SCORE " .. score, half + 3, t - 30, TEXT, 1)
    drop(frame, "PRESS START", half + 17, t - 60, TEXT, 1)
  elseif over and over.result == "lost" then
    local t = world.tick - over.at + frame.alpha
    drop(frame, "GAME OVER", half - 28, t, RED, 3)
    drop(frame, "PRESS START", half + 7, t - 40, TEXT, 1)
  end
end


-- Every torch lights the wall around it and flickers a little.
function on_draw(frame)
  hud(frame)
  for _, t in ipairs(level:things("torch")) do
    local flicker = 4 * math.sin(world.tick * 0.31 + t.x) + 2 * math.sin(world.tick * 0.77 + t.z)
    frame:light({ x = t.x, y = t.y - 3, z = t.z + 10 }, 60 + flicker, { 255, 160, 70 })
  end
end

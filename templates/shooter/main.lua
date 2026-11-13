-- The yard: sand, walls to hide behind, and riflemen further up.
-- The kit (c.kit in conf.lua) moves the players and the soldiers.
-- This file builds the level and decides what a kill is worth and what
-- going down costs. Change anything you like.
-- level:players() lists everyone playing. thing.seat is set only on a
-- player, so it tells players from other things.

local MAX_HP = 5          -- full hearts; keep it the same as hp in c.state.player (conf.lua)
local DOWN = 149          -- ticks a player stays down before coming back
local SAFE = 150          -- ticks a player cannot be hurt after coming back

-- A soldier falls: points for the player who got him, more with the
-- knife. A player who falls stays down for a while.
function on_death(thing, by, weapon)
  if thing.seat then
    thing.state.down_at = world.tick
  elseif thing.families.soldiers and by and by.seat then
    local worth = weapon == "knife" and thing.props.knife_worth or thing.props.worth
    by.state.score = by.state.score + worth
  end
end

-- The run is over: remember how it ended, for the HUD.
function on_world_end(result)
  world.state.over = { result = result, at = world.tick }
end

-- Coming back after going down costs a life. The player comes back next
-- to a teammate who is still fighting (or at the start) with full hearts.
-- With no lives left the player is out. When everyone is out, the game
-- is over.
local function come_back(p)
  p.state.down_at = nil
  p.state.lives = p.state.lives - 1
  if p.state.lives <= 0 then
    p.state.out = true
    if #level:players() == 0 then world:lose() end
    return
  end
  local beside
  for _, q in ipairs(level:players()) do
    if q ~= p and not q.state.down_at then beside = q end
  end
  p:place(beside or level.arrived)
  p:face({ x = 0, y = 1 })      -- up the level again
  p.state.hp = MAX_HP
  level:clear_rounds("enemies")  -- to be fair: the enemies' flying bullets vanish
  p.state.invulnerable = SAFE
end

-- Where a player's part of the HUD goes: player 1 left, player 2 right.
local function left_of(frame, seat, width)
  return seat == 2 and frame.width - 3 - width or 3
end

-- When player 1 loses a heart or throws a grenade, its picture in the
-- HUD bursts. (Only on the left: the ticks never see how wide the window
-- is.)
local function follow_hud()
  local h, p = level.state.hud, player.state
  if h then
    for i = p.hp, h.hp - 1 do screen:burst("heart", 3 + 9 * i, 4) end
    for i = (p.inventory.grenade or 0), math.min(h.grenades, 9) - 1 do screen:burst("grenade", 3 + 7 * i, screen.height - 10) end
  end
  level.state.hud = { hp = p.hp, grenades = p.inventory.grenade or 0 }
end

function on_tick()
  follow_hud()
  for _, p in ipairs(world.players) do
    if p and not p.state.out then
      -- Out of ammo: back to the rifle (it needs none).
      if p.state.weapon ~= "rifle" and (p.state.inventory[p.state.weapon] or 0) == 0 then
        p.state.weapon = "rifle"
      end
      if p.state.down_at and world.tick - p.state.down_at >= DOWN then come_back(p) end
    end
  end
end

-- The HUD, drawn over the game: each player's hearts and lives at the
-- top with the score below, the gun and grenades at the bottom (player 1
-- left, player 2 right). A free seat shows PLAYER 2: PRESS START.
-- Messages drop in.
local TEXT, NUMBERS, RED = { 240, 230, 204 }, { 255, 214, 90 }, { 230, 58, 48 }

local function drop(frame, text, y, ticks, color, size)
  local x = math.floor((frame.width - frame:text_width(text, size)) / 2 + 0.5)
  frame:drop_in(text, x, y, ticks, color, size)
end

local function corner(frame, seat, p)
  local s = p.state
  local function at(width) return left_of(frame, seat, width) end
  if s.out then
    frame:text("GAME OVER", at(frame:text_width("GAME OVER")), 4, RED)
    return
  end
  for i = 0, MAX_HP - 1 do
    frame:icon(i < s.hp and "heart" or "empty_heart", at(9 * MAX_HP - 2) + 9 * i, 4)
  end
  local lives = "x" .. math.max(s.lives, 0)
  frame:text(lives, at(frame:text_width(lives)), 13)
  local score = string.format("%06d", s.score)
  frame:text(score, at(frame:text_width(score)), 23, NUMBERS)
  local gun = string.upper(s.weapon or "")
  if s.weapon ~= "rifle" then gun = gun .. " " .. (s.inventory[s.weapon] or 0) end
  frame:text(gun, at(frame:text_width(gun)), frame.height - 20)
  local grenades = math.min(s.inventory.grenade or 0, 9)
  for i = 0, grenades - 1 do frame:icon("grenade", at(7 * 9 - 2) + 7 * i, frame.height - 10) end
  if s.down_at then
    frame:text("DOWN!", at(frame:text_width("DOWN!")), 33, RED)
  end
end

-- The keys, shown for the first ten seconds: the keyboard's or the
-- gamepad's (whichever the player uses), in the player's language.
local KEYS = {
  en = { keys = "ARROWS", pad = "STICK", "%s MOVE   {jump} JUMP   {strafe} STRAFE", "{fire} FIRE   {grenade} GRENADE" },
  de = { keys = "PFEILTASTEN", pad = "STICK", "%s LAUFEN   {jump} SPRINGEN   {strafe} SEITWÄRTS", "{fire} FEUER   {grenade} GRANATE" },
  fr = { keys = "FLÈCHES", pad = "STICK", "%s BOUGER   {jump} SAUT   {strafe} DE CÔTÉ", "{fire} TIR   {grenade} GRENADE" },
}
local function show_keys(frame)
  if world.tick >= 600 then return end
  local t = KEYS[world.language] or KEYS.en
  local move = controls.device == "gamepad" and t.pad or t.keys
  for i, line in ipairs(t) do
    line = line:format(move)
    local w, y = frame:text_width(line), frame.height - 46 + 11 * i
    local x = math.floor((frame.width - w) / 2 + 0.5)
    frame:text(line, x, y, TEXT)
    frame:box(x - 3, y - 2, w + 6, 11, { 20, 18, 16 })   -- a dark box behind the text (what is drawn first stays in front)
  end
end

function on_draw(frame)
  show_keys(frame)
  local half = frame.height / 2
  for seat, p in ipairs(world.players) do
    if p then
      corner(frame, seat, p)
    elseif world.tick // 30 % 2 == 0 then
      local text = "PLAYER " .. seat .. ": PRESS START"
      frame:text(text, left_of(frame, seat, frame:text_width(text)), 4)
    end
  end
  local over = world.state.over
  if over and over.result == "lost" then
    local t = world.tick - over.at + frame.alpha
    drop(frame, "GAME OVER", half - 28, t, RED, 3)
    drop(frame, "PRESS START", half + 7, t - 40, TEXT, 1)
  elseif #world.players == 1 and player.state.down_at then
    drop(frame, "DOWN!", half - 16, world.tick - player.state.down_at + frame.alpha, RED, 2)
  end
end

function on_level_start(how)
  local w, d = level.size.x, level.size.y
  level:fill(0, 0, 0, w - 1, d - 1, 3, "#c2a878")
  for i = 0, 5 do
    local y = 120 + i * 60
    local x = 40 + (i % 2) * 120
    level:fill(x, y, 4, x + 50, y + 5, 14, "#8a7a62")
  end
  for i = 0, 3 do
    level:spawn("rifleman", 60 + i * 45, 260 + i * 40)
  end
end

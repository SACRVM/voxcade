-- More Blood's rules. The shooter kit (c.kit in conf.lua) moves and fights:
-- its soldiers, huts, hostages and the gunship act by themselves, rounds
-- and blasts hurt. What it all means is written here: what a kill and a
-- pickup are worth, freeing hostages (one killed loses the run), the
-- checkpoints, a life lost, the helipad.

local MAX_HP = 5          -- the player's hit points, full
local DOWN = 149          -- ticks the player lies there before coming back
local SAFE = 150          -- ticks nothing hurts after coming back

-- Points for every soldier that falls (more for the knife), the gunship's,
-- and the gunships down counted for the helipad (a gunship that has not
-- come yet is not one that is down). The player down lies there a while.
-- A hostage killed ends the run at once.
function on_death(thing, by, weapon)
  if thing == player then
    level.state.down_at = world.tick
  elseif thing.kind == "hostage" then
    level:sound("game_over", player)
    world:lose()
  elseif thing.families.soldiers then
    local worth = weapon == "knife" and thing.props.knife_worth or thing.props.worth
    player.state.score = player.state.score + worth
  elseif thing.kind == "gunship" then
    player.state.score = player.state.score + thing.props.worth
    level.state.gunships_down = (level.state.gunships_down or 0) + 1
  end
end

-- What a pickup gives, and standing at the helipad.
function on_touch(a, b)
  if a ~= player then return end
  if b.families.pickups and not level.state.down_at then
    local p = b.props
    if p.weapon then
      player.state.weapon = p.weapon
      player:take(p.weapon, p.ammo)       -- its rounds: an item of its name
    end
    if p.grenades then player:take("grenade", p.grenades) end
    if p.heal then player.state.hp = math.min(player.state.hp + p.heal, MAX_HP) end
    player.state.score = player.state.score + p.worth
    level:sound("pickup", player)
    b:remove()
  elseif b.kind == "goal" then
    level.state.at_goal = true
  end
end

function on_touch_end(a, b)
  if b.kind == "goal" then level.state.at_goal = false end
end

-- Back from going down: a life less, at the last checkpoint passed (or
-- where the level began) facing north, every hit point back, the enemies'
-- rounds and grenades in flight gone; with the last life the run is over.
local function back_up()
  level.state.down_at = nil
  player.state.lives = player.state.lives - 1
  if player.state.lives <= 0 then
    level:sound("game_over", player)
    world:lose()
    return
  end
  local back = player.state.checkpoint and level:thing(player.state.checkpoint)
  player:place(back or level.arrived)
  player:face({ x = 0, y = 1 })
  player.state.hp = MAX_HP
  level:clear_rounds("enemies")
  player.state.invulnerable = SAFE
end

-- The checkpoint furthest along the level the player has passed.
local function checkpoints()
  local at = player.state.checkpoint and level:thing(player.state.checkpoint)
  local reached = at and at.y or level.arrived.y
  local passed = false
  for _, c in ipairs(level:things("checkpoint")) do
    if player.y >= c.y and c.y > reached then
      player.state.checkpoint = c.name
      reached = c.y
      passed = true
    end
  end
  if passed then level:sound("checkpoint", player) end
end

-- Hostages the player reaches (through the bars) are freed: the cage
-- bursts and they run off.
local function hostages()
  for _, h in ipairs(player:touching("hostage")) do
    if not h.state.freed then
      h.state.freed = true
      level.state.freed = (level.state.freed or 0) + 1
      player.state.score = player.state.score + h.props.worth
    end
  end
end

-- What the HUD counts from the level's start: the hostages, the gunship's
-- full health.
function on_level_start(how)
  level.state.hostages = level:count("hostage")
  local gunship = level:things("gunship")[1]
  level.state.gunship_hp = gunship and gunship.state.hp or 1
end

-- The grenades the player holds.
local function grenades()
  return player.state.inventory.grenade or 0
end

-- The HUD's moving parts, tick by tick: a heart lost or a grenade thrown
-- bursts, the score's digits that changed hop, the gunship's warning.
local function follow_hud()
  local h = level.state.hud
  local p = player.state
  if h then
    for i = p.hp, h.hp - 1 do screen:burst("heart", 3 + 9 * i, 4) end
    for i = grenades(), math.min(h.grenades, 9) - 1 do screen:burst("grenade", 3 + 7 * i, screen.height - 10) end
    local old, new = string.format("%06d", h.score), string.format("%06d", p.score)
    for k = 1, math.min(#old, #new, 8) do
      if old:sub(-k, -k) ~= new:sub(-k, -k) then h.hops[k] = world.tick end
    end
  else
    h = { hops = {} }
    level.state.hud = h
  end
  h.hp, h.grenades, h.score = p.hp, grenades(), p.score
  for _, g in ipairs(level:things("gunship")) do
    if g.state.flying and not level.state.gunship_at then level.state.gunship_at = world.tick end
  end
end

-- The gunship's song while one is in the air (at once), the player's own
-- again from the next bar once none is.
local function boss_music()
  local up = false
  for _, g in ipairs(level:things("gunship")) do
    if g.state.flying then up = true end
  end
  if up and music.song ~= "end_boss" then
    music:play("end_boss")
  elseif not up and music.song == "end_boss" then
    music:play()
  end
end

-- Every tick: the music, coming back, hostages, the checkpoints, and
-- standing at the helipad when the last gunship falls wins too.
function on_tick()
  follow_hud()
  boss_music()
  -- A gun whose rounds ran out: the rifle in hand again (it needs none).
  if player.state.weapon ~= "rifle" and (player.state.inventory[player.state.weapon] or 0) == 0 then
    player.state.weapon = "rifle"
  end
  if level.state.down_at then
    if world.tick - level.state.down_at >= DOWN then back_up() end
    return
  end
  hostages()
  checkpoints()
  if level.state.at_goal and (level.state.gunships_down or 0) > 0 then
    player.state.score = player.state.score + level:thing("goal_1").props.worth
    level:sound("win", player)
    world:win()
  end
end

-- How a run ended, for the HUD's last words.
function on_world_end(result)
  world.state.over = { result = result, at = world.tick }
end

-- The HUD: hearts and lives top left, the score top right (its digits
-- hopping), the hostages and the flying gunship's health at the top, the
-- gun and the grenades bottom left, and the messages dropping in.
local TEXT, NUMBERS, RED = { 240, 230, 204 }, { 255, 214, 90 }, { 230, 58, 48 }
local LABELS = { rifle = "RIFLE", machine_gun = "MG", rockets = "ROCKETS" }

local function middle(frame, text, size)
  return math.floor((frame.width - frame:text_width(text, size)) / 2 + 0.5)
end

local function drop(frame, text, y, ticks, color, size)
  frame:drop_in(text, middle(frame, text, size), y, ticks, color, size)
end

-- The keys, shown as a run begins: the keyboard's or the pad's, as the
-- player plays, in the player's language.
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
    frame:box(x - 3, y - 2, w + 6, 11, { 20, 18, 16 })   -- dark behind it (drawn after: what comes first stays in front)
  end
end

function on_draw(frame)
  show_keys(frame)
  local p, h, half = player.state, level.state.hud or { hops = {} }, frame.height / 2
  for i = 0, MAX_HP - 1 do
    frame:icon(i < p.hp and "heart" or "empty_heart", 3 + 9 * i, 4)
  end
  frame:icon("head", 3, 13)
  frame:text("x" .. math.max(p.lives, 0), 10, 13)

  local score = string.format("%06d", p.score)
  local x = frame.width - 3 - frame:text_width(score)
  for k = 1, #score do
    local at, lift = h.hops[#score - k + 1], 0
    local t = at and world.tick - at + 1 + frame.alpha
    if t and t < 10 then lift = math.max(0, math.floor(3 * math.sin(math.pi * t / 10) + 0.5)) end
    frame:text(score:sub(k, k), x + (k - 1) * 6, 3 - lift, NUMBERS)
  end

  if (level.state.hostages or 0) > 0 then
    local line = "HOSTAGES " .. (level.state.freed or 0) .. "/" .. level.state.hostages
    frame:text(line, middle(frame, line, 1), 3)
  end
  for _, g in ipairs(level:things("gunship")) do
    if g.state.flying then
      frame:bar(math.floor((frame.width - 64) / 2 + 0.5), 19, 64, 2, g.state.hp / level.state.gunship_hp, RED)
      break
    end
  end

  local gun = LABELS[p.weapon] or string.upper(p.weapon)
  if p.weapon ~= "rifle" then gun = gun .. " " .. (p.inventory[p.weapon] or 0) end
  frame:text(gun, 3, frame.height - 20)
  for i = 0, math.min(grenades(), 9) - 1 do frame:icon("grenade", 3 + 7 * i, frame.height - 10) end
  if grenades() > 9 then frame:text("x" .. grenades(), 66, frame.height - 10) end

  local over = world.state.over
  if over and over.result == "won" then
    local t = world.tick - over.at + frame.alpha
    drop(frame, "MISSION COMPLETE", half - 22, t, NUMBERS, 2)
    drop(frame, "SCORE " .. p.score, half + 3, t - 30, TEXT, 1)
    drop(frame, "PRESS START", half + 17, t - 60, TEXT, 1)
  elseif over and over.result == "lost" then
    local t = world.tick - over.at + frame.alpha
    drop(frame, "GAME OVER", half - 28, t, RED, 3)
    drop(frame, "PRESS START", half + 7, t - 40, TEXT, 1)
  elseif level.state.down_at then
    drop(frame, "DOWN!", half - 16, world.tick - level.state.down_at + frame.alpha, RED, 2)
  elseif level.state.gunship_at and world.tick - level.state.gunship_at < 150 then
    drop(frame, "GUNSHIP!", half - 36, world.tick - level.state.gunship_at + frame.alpha, RED, 2)
  end
end

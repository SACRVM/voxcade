-- Voxkeep: run, jump, climb, land on enemies, take the gems, out through
-- the door: it opens once every gem is taken and leads to the next room
-- (levels/), out of the last the run is won.

-- The rules of the jump & run, written out. The ready-made rules
-- (conf.lua: c.kit) move the hero and the things; what a touch means,
-- what a life is worth and when the level is won is decided here:
-- change what you like.

local BOUNCE = 50        -- how high the hero bounces off an enemy, voxels
local HEARTS = 3         -- hearts back after a life is lost (c.things.hero.hearts)
local SAFE = 60          -- ticks nothing hurts it after (c.things.hero.invulnerable)

-- The puzzles. Things are named by their kind and the order they lie in
-- (lever_1, slab_1, lift_1, plate_1, hatch_1). A lever thrown sets its
-- platforms to work for good (a slab out of its wall, a lift going up and
-- down); a plate holds its hatch open while the hero or a crate lies on
-- it, and lets it shut again. Which are whose: as LINKS says for a room,
-- else those of its number.
local LINKS = {
  ["04_levers"] = { lever_1 = { "lift_1" }, lever_2 = { "slab_1", "slab_left_1" } },
}
local LEVERED, PLATED = { "slab", "slab_left", "lift" }, { "hatch" }

local function number(thing)
  return thing.name:match("_(%d+)$")
end

local function works(thing, kinds)
  local out, named = {}, LINKS[level.name] and LINKS[level.name][thing.name]
  if named then
    for _, name in ipairs(named) do out[#out + 1] = level:thing(name) end
    return out
  end
  for _, kind in ipairs(kinds) do
    for _, t in ipairs(level:things(kind)) do
      if number(t) == number(thing) then out[#out + 1] = t end
    end
  end
  return out
end

local function throw(lever)
  if lever.state.pose == "on" then return end
  lever.state.pose = "on"
  level:sound("door", lever)
  for _, t in ipairs(works(lever, LEVERED)) do
    if t.kind == "lift" then t.state.goal = nil else t.state.goal = "end" end
  end
end

local function plates()
  for _, plate in ipairs(level:things("plate")) do
    local down = #plate:touching("crate") > 0
    for _, t in ipairs(player:touching("plate")) do
      if t == plate then down = true end
    end
    local was = plate.state.pose == "down"
    if down ~= was then
      plate.state.pose = down and "down" or "idle"
      level:sound("door", plate)
    end
    for _, hatch in ipairs(works(plate, PLATED)) do hatch.state.goal = down and "end" or "start" end
  end
end

-- A life lost: back at the checkpoint (or where the level began) with
-- every heart, or the run is over with the last.
local function lose_life()
  player.state.lives = player.state.lives - 1
  level:sound("lose", player)
  if player.state.lives <= 0 then
    world:lose()
    return
  end
  local back = player.state.checkpoint and level:thing(player.state.checkpoint)
  player:place(back or level.arrived)
  player.state.hp = HEARTS
  player.state.invulnerable = SAFE
end

-- What happens once, as the hero touches something: landing on an
-- enemy, a pickup, a checkpoint, the door.
function on_touch(a, b, where)
  if a ~= player then return end
  if b.families.enemies and where == "top" then
    b:hurt(1, player)
    player:bounce(BOUNCE)
    level.state.stomped = world.tick
  elseif b.families.pickups then
    player.state.score = player.state.score + b.props.worth
    player.state.taken = player.state.taken + 1
    b:remove()
  elseif b.kind == "checkpoint" and player.state.checkpoint ~= b.name then
    player.state.checkpoint = b.name
    level:sound("checkpoint", b)
  elseif b.kind == "door" and level.state.open and not level.state.out then
    level.state.out = true
    player.state.score = player.state.score + b.props.worth
    if level.next then world:go_to(level.next) else world:win() end
  end
end

-- A lever is pulled with its key (interact), standing at it.
function on_interact(thing, by)
  if thing.kind == "lever" then throw(thing) end
end

-- An enemy is worth its points when it goes; the hero without a heart
-- loses a life.
function on_death(thing, by)
  if thing == player then
    lose_life()
  elseif thing.families.enemies then
    player.state.score = player.state.score + thing.props.worth
  end
end

function on_fall_out(thing)
  if thing == player then lose_life() end
end

-- A room begins: its number drops in (the HUD), the hero has every heart
-- again in a new one; started over (restart), it is as it was entered:
-- the score and the gems taken then. The slabs and lifts that have a
-- lever wait for it, the hatches are shut.
function on_level_start(how)
  level.state.began = world.tick
  if how == "start" or how == "go_to" then
    world.state.entered = { score = player.state.score, taken = player.state.taken }
  end
  if how == "go_to" or how == "restart" then player.state.hp = HEARTS end
  if how == "restart" and world.state.entered then
    player.state.score = world.state.entered.score
    player.state.taken = world.state.entered.taken
  end
  for _, lever in ipairs(level:things("lever")) do
    for _, t in ipairs(works(lever, LEVERED)) do t.state.goal = "start" end
  end
  for _, hatch in ipairs(level:things("hatch")) do hatch.state.goal = "start" end
end

-- Every tick: the door opening once no gem is left, and what hurts as
-- long as the hero touches it (an enemy from the side, spikes, a hazard
-- block under its feet). Hurt, it is safe a moment
-- (player.state.invulnerable), so this hurts again only after.
function on_tick()
  -- The room over again (a crate where it helps no more).
  if input:pressed("restart") then world:restart() end
  plates()
  if not level.state.open and level:count("gem") == 0 then
    level.state.open = true
    for _, door in ipairs(level:things("door")) do
      door.state.pose = "open"
      level:sound("door", door)
    end
  end
  if level.state.stomped == world.tick then return end
  local hurt_by
  for _, t in ipairs(player:touching()) do
    if t.families.enemies or t.families.hazards then hurt_by = t end
  end
  if hurt_by then
    player:hurt(1, hurt_by)
  elseif level:block(player.x, player.y, player.z + 1) == "hazard" then
    player:hurt(1)
  end
end

-- How a run ended, for the HUD's last words.
function on_world_end(result)
  world.state.over = { result = result, at = world.tick }
end

-- The HUD: hearts and lives top left, the score and the pickups taken top
-- right; at the end STAGE CLEAR or GAME OVER dropping in.
local TEXT, NUMBERS, RED = { 240, 230, 204 }, { 255, 214, 90 }, { 230, 58, 48 }

local function drop(frame, text, y, ticks, color, size)
  local x = math.floor((frame.width - frame:text_width(text, size)) / 2 + 0.5)
  frame:drop_in(text, x, y, ticks, color, size)
end

-- The keys, shown as a run begins: the keyboard's or the pad's, as the
-- player plays, in the player's language.
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
  local p, half = player.state, frame.height / 2
  for i = 0, HEARTS - 1 do
    frame:icon(i < p.hp and "heart" or "empty_heart", 3 + 9 * i, 4)
  end
  frame:text("x" .. p.lives, 3, 13)
  local score, taken = string.format("%06d", p.score), p.taken .. " TAKEN"
  frame:text(score, frame.width - 3 - frame:text_width(score), 3, NUMBERS)
  frame:text(taken, frame.width - 3 - frame:text_width(taken), 13)
  local over = world.state.over
  local since = world.tick - (level.state.began or 0) + frame.alpha
  if not over and since < 150 then
    drop(frame, "LEVEL " .. level.number, half - 22, since, NUMBERS, 2)
    drop(frame, "{restart} STARTS THE ROOM AGAIN", half + 3, since - 20, TEXT, 1)
  end
  -- At a lever not yet pulled: its key.
  for _, lever in ipairs(player:touching("lever")) do
    if lever.state.pose ~= "on" and not over then
      local hint = "{interact} PULLS THE LEVER"
      frame:text(hint, math.floor((frame.width - frame:text_width(hint)) / 2 + 0.5), frame.height - 12, TEXT)
    end
  end
  if over and over.result == "won" then
    local t = world.tick - over.at + frame.alpha
    drop(frame, "STAGE CLEAR", half - 22, t, NUMBERS, 2)
    drop(frame, "SCORE " .. p.score, half + 3, t - 30, TEXT, 1)
    drop(frame, "PRESS START", half + 17, t - 60, TEXT, 1)
  elseif over and over.result == "lost" then
    local t = world.tick - over.at + frame.alpha
    drop(frame, "GAME OVER", half - 28, t, RED, 3)
    drop(frame, "PRESS START", half + 7, t - 40, TEXT, 1)
  end
end


-- Every torch lights the wall around it, flickering a little.
function on_draw(frame)
  hud(frame)
  for _, t in ipairs(level:things("torch")) do
    local flicker = 4 * math.sin(world.tick * 0.31 + t.x) + 2 * math.sin(world.tick * 0.77 + t.z)
    frame:light({ x = t.x, y = t.y - 3, z = t.z + 10 }, 60 + flicker, { 255, 160, 70 })
  end
end

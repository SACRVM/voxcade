-- The bunny: its needs (player.state.food, energy, mood, 0 to 100), its
-- pace by its condition, its XP and level, its bag (a kind of item a slot),
-- eating and sleeping.
local D = require("data")
local clock = require("clock")
local text = require("text")
local calendar = require("calendar")
local bunny = {}

local N = D.NEEDS

local function add(need, amount)
  local p = player.state
  p[need] = math.max(0, math.min(100, p[need] + amount))
end

-- How the bunny is: "exhausted", "hungry", "bored", "tired", "fine" or
-- "thriving" (the worst first).
function bunny.condition()
  local p = player.state
  if p.energy < N.exhausted then
    return "exhausted"
  elseif p.food < N.hungry then
    return "hungry"
  elseif p.mood < N.bored then
    return "bored"
  elseif p.energy < N.tired then
    return "tired"
  elseif p.food >= 80 and p.energy >= 70 and p.mood >= 70 then
    return "thriving"
  end
  return "fine"
end

-- Its walking speed by its condition (the one thing a poor condition does).
function bunny.speed()
  local c = bunny.condition()
  if c == "exhausted" then
    return D.WALK * N.pace.exhausted
  elseif c == "hungry" or c == "bored" or c == "tired" then
    return D.WALK * N.pace.tired
  end
  return D.WALK * N.pace.normal
end

-- Whether the bunny stands at a place of the farm (the stall, the burrow).
function bunny.at(place)
  local p = level.places[place]
  return math.abs(player.x - p.x) < 14 and math.abs(player.y - p.y) < 14
end

-- A minute gone by: hunger and boredom grow, energy comes back by day
-- (on a lazy day boredom slower, energy faster).
function bunny.minute()
  add("food", N.per_hour.food / 60)
  add("mood", N.per_hour.mood / 60 * calendar.mod("mood", 1))
  if clock.daytime(world.state.now) then
    add("energy", N.per_hour.energy / 60 * calendar.mod("rest", 1))
  end
end

-- A piece of work done (sowing, harvesting): tiring, a little hungry
-- making, and fun.
function bunny.work()
  for need, amount in pairs(N.work) do
    add(need, amount)
  end
  level.state.working = world.tick
end

-- XP gained; a level reached opens what it opens.
function bunny.gain(xp)
  local p = player.state
  p.xp = p.xp + xp
  while p.xp >= D.xp_for(p.level) do
    p.xp = p.xp - D.xp_for(p.level)
    p.level = p.level + 1
    local opened = {}
    for _, crop in ipairs(D.CROP_ORDER) do
      if D.CROPS[crop].level == p.level then
        opened[#opened + 1] = { "crops", crop }
      end
    end
    for _, zone in ipairs(D.BARN.zone_order) do
      if D.BARN.zones[zone].level == p.level then
        opened[#opened + 1] = { "pens", zone }
      end
    end
    level.state.level_up = { level = p.level, opened = opened, at = world.tick }
    level:sound("levelup", player)
  end
end

-- Whether the bag has room for one more of an item: a slot it has, or a
-- free one (player.state.bag slots, more bought at the stall).
function bunny.can_hold(item)
  local inv, kinds = player.state.inventory, 0
  if (inv[item] or 0) > 0 then
    return true
  end
  for _, count in pairs(inv) do
    if count > 0 then
      kinds = kinds + 1
    end
  end
  return kinds < player.state.bag
end

-- Eating a carrot: full of food again and some energy (less by night).
function bunny.eat()
  local p = player.state
  if p.carrots < 1 then
    text.note("no_carrot")
    level:sound("error", player)
    return false
  elseif p.food >= 100 then
    text.note("full")
    level:sound("error", player)
    return false
  end
  p.carrots = p.carrots - 1
  add("food", N.eat.food)
  add("energy", N.eat.energy * (clock.daytime(world.state.now) and 1 or N.eat.night))
  text.note("ate")
  level:sound("eat", player)
  return true
end

-- Sleeping till 6:00 of the next day, in the burrow or where the bunny
-- fell asleep at 2:00 (it wakes in the burrow either way, at no cost):
-- energy is full, the bunny wakes hungry, and the game is saved. Nothing
-- grows or recovers in the hours slept (world.state.awake stands).
function bunny.sleep(fell)
  world.state.now, world.state.ticks = clock.next_morning(world.state.now), 0
  local p = player.state
  p.energy = 100
  add("food", -N.wake_hunger)
  player:place(level.places.burrow)
  player:face({ x = 0, y = -1 })
  level.state.morning = { at = world.tick, fell = fell }
  level:sound("sleep", player)
  world:save(1)
end

return bunny

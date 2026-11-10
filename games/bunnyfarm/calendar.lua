-- The farm's week: an event a day by the weekday (data.lua's EVENTS),
-- the prototype's world events on the game's calendar. Each is a set of
-- changes where they matter (core.js: activeEvent, evMod, evAffects):
-- prices for some crops, more XP for a harvest, a lazier bunny, crops
-- growing faster in the rain, the full moon's berries by night. Nothing
-- of it is kept: the day says it all.
local D = require("data")
local clock = require("clock")
local calendar = {}

local E = D.EVENTS

-- The event of a moment's day (now by default): its name and changes.
function calendar.today(now)
  now = now or world.state.now
  local weekday = clock.weekday(now)
  local id = E.week[weekday]
  local week = (clock.day(now) - 1) // D.DAYS_A_WEEK + 1
  if weekday == E.sale.weekday and week % 2 == 0 then
    id = E.sale.event
  end
  return id, E[id]
end

-- One of the day's changes, or `default` when it changes nothing there.
function calendar.mod(field, default)
  local _, event = calendar.today()
  local v = event[field]
  if v == nil then
    return default
  end
  return v
end

-- Whether the day's event changes a crop's prices.
function calendar.affects(crop)
  local _, event = calendar.today()
  local crops = event.crops
  if crops == "all" then
    return true
  end
  for _, c in ipairs(crops or {}) do
    if c == crop then
      return true
    end
  end
  return false
end

-- What a crop harvested now gives instead of itself (the full moon's
-- berries by night), or nil.
function calendar.special(crop)
  local moon = calendar.mod("moon", nil)
  if moon and moon.crop == crop and clock.night(world.state.now) then
    return moon.gives
  end
  return nil
end

-- How many minutes the crops grow in this minute awake: one, or in the
-- rain now and then two (`growth` a minute on the whole).
function calendar.growing(awake)
  local g = calendar.mod("growth", 1)
  return math.floor(awake * g) - math.floor((awake - 1) * g)
end

-- Whether a full moon shines now (the moon's night of its day).
function calendar.full_moon()
  return calendar.mod("weather", nil) == "moon" and clock.night(world.state.now)
end

local function mix(a, b, t)
  return { math.floor(a[1] + (b[1] - a[1]) * t + 0.5), math.floor(a[2] + (b[2] - a[2]) * t + 0.5), math.floor(a[3] + (b[3] - a[3]) * t + 0.5) }
end

-- The day's weather laid over the clock's light (clock.daylight's, at
-- `now` with the tick's share): rain greys the sky and softens the
-- shadows; the full moon lights the night silver, with shadows.
function calendar.light(light, now)
  local weather = calendar.mod("weather", nil)
  if weather == "rain" then
    light.sun_color = mix(light.sun_color, { 112, 118, 132 }, 0.65)
    light.sky = mix(light.sky, { 104, 112, 126 }, 0.75)
    light.ambient = light.ambient + 0.06
    light.shadow = light.shadow * 0.3
  elseif weather == "moon" then
    local hours = clock.minute(math.floor(now)) / 60 + (now - math.floor(now)) / 60
    local t = math.max(0, math.min(1, hours - 19.5))   -- as the dusk goes
    light.sun_color = mix(light.sun_color, { 128, 142, 196 }, t * 0.6)
    light.ambient = light.ambient + 0.02 * t
    light.shadow = light.shadow + 0.15 * t
  end
  return light
end

return calendar

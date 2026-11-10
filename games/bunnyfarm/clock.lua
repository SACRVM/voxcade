-- The farm's clock: world.state.now counts game minutes from midnight
-- before day 1 (day 1 begins at 6:00), world.state.ticks the ticks into
-- the minute. The light follows the clock: a low warm sun at dawn, white at
-- noon, orange at dusk, a dim blue night.
local D = require("data")
local clock = {}

-- The day, from 1, of a moment (a day lasts from 6:00 to 2:00).
function clock.day(now)
  return (now - D.DAY_START) // D.MINUTES_A_DAY + 1
end

-- Minutes after midnight of the day's start, 360 (6:00) to 1560 (2:00).
function clock.minute(now)
  return (now - D.DAY_START) % D.MINUTES_A_DAY + D.DAY_START
end

-- The hour and minute on the clock face.
function clock.time(now)
  local m = clock.minute(now)
  return (m // 60) % 24, m % 60
end

-- The weekday, 1 (Monday) to 7, and the season, 1 to 4, of a moment.
function clock.weekday(now)
  return (clock.day(now) - 1) % D.DAYS_A_WEEK + 1
end

function clock.season(now)
  return (clock.day(now) - 1) // D.DAYS_A_SEASON % D.SEASONS + 1
end

-- The farm year of a moment, from 1 (a year: four seasons of 14 days),
-- and the day of its season, 1 to 14.
function clock.year(now)
  return (clock.day(now) - 1) // (D.DAYS_A_SEASON * D.SEASONS) + 1
end

function clock.season_day(now)
  return (clock.day(now) - 1) % D.DAYS_A_SEASON + 1
end

-- 6:00 of the next day, from a moment of this one.
function clock.next_morning(now)
  return clock.day(now) * D.MINUTES_A_DAY + D.DAY_START
end

-- Whether the sun gives energy back: by day, 7:00 to 18:00.
function clock.daytime(now)
  local m = clock.minute(now)
  return m >= 7 * 60 and m < 18 * 60
end

-- Whether it is night (20:00 on): lamps on, the night's song.
function clock.night(now)
  return clock.minute(now) >= 20 * 60
end

local function mix(a, b, t)
  return a + (b - a) * t
end

local function mix_color(a, b, t)
  return { math.floor(mix(a[1], b[1], t) + 0.5), math.floor(mix(a[2], b[2], t) + 0.5), math.floor(mix(a[3], b[3], t) + 0.5) }
end

-- The light at hours of the day: sun colour, the light every face gets,
-- how dark shadows are, the sky. Between two the light blends.
local KEYS = {
  { at = 6, sun = { 255, 168, 120 }, ambient = 0.32, shadow = 0.35, sky = { 236, 160, 128 } },
  { at = 8, sun = { 255, 240, 222 }, ambient = 0.46, shadow = 0.42, sky = { 128, 168, 220 } },
  { at = 13, sun = { 255, 252, 244 }, ambient = 0.5, shadow = 0.45, sky = { 115, 158, 217 } },
  { at = 17, sun = { 255, 230, 196 }, ambient = 0.46, shadow = 0.42, sky = { 124, 160, 210 } },
  { at = 19, sun = { 255, 140, 80 }, ambient = 0.34, shadow = 0.36, sky = { 226, 128, 92 } },
  { at = 20.5, sun = { 62, 76, 132 }, ambient = 0.1, shadow = 0.25, sky = { 30, 36, 70 } },
  { at = 26, sun = { 52, 64, 118 }, ambient = 0.08, shadow = 0.25, sky = { 16, 20, 44 } },
}

-- The picture's daylight at a moment (minutes, with the tick's share).
function clock.daylight(now)
  local hours = clock.minute(math.floor(now)) / 60 + (now - math.floor(now)) / 60
  local a, b = KEYS[1], KEYS[#KEYS]
  for i = 1, #KEYS - 1 do
    if hours >= KEYS[i].at and hours < KEYS[i + 1].at then
      a, b = KEYS[i], KEYS[i + 1]
    end
  end
  local t = math.max(0, math.min(1, (hours - a.at) / (b.at - a.at)))
  -- The sun goes from east (6:00) over the south (13:00) to the west
  -- (20:00); by night the moon stands high in the south-west.
  local sun
  if hours < 20 then
    local angle = (hours - 6) / 14 * math.pi
    sun = { -math.cos(angle) * 0.9, 0.55, -(0.25 + math.sin(angle) * 0.85) }
  else
    sun = { 0.3, 0.5, -1 }
  end
  return {
    sun = sun,
    sun_color = mix_color(a.sun, b.sun, t),
    ambient = mix(a.ambient, b.ambient, t),
    shadow = mix(a.shadow, b.shadow, t),
    sky = mix_color(a.sky, b.sky, t),
  }
end

return clock

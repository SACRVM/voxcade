-- The farm set up in words, for the scenarios (scenarios/*.lua) and the
-- console: a day and an hour, the bunny's level, coins and goods, crops in
-- the beds, animals in the pens, a stable, a gap in a fence, the bunny at
-- a place. Each keeps the states as the game would have them: the clock's
-- jump moves what counts the minutes awake with it, an animal's age and
-- its feeding count from there, the pens' things follow the barn
-- (docs/game-model.md, "Scenarios and the console"; the game itself never
-- requires this file).
local D = require("data")
local farm = require("farm")
local barn = require("barn")
local wolf = require("wolf")
local year = require("year")
local set = {}

-- The clock at `day` (from 1), at `time` ("H:MM": 6:00 to 1:59 the night
-- after). The minutes awake go on by as much, so what grows or recovers
-- by them stands as it did; the merchants are there as the hour has them.
function set.day(day, time)
  local h, m = tostring(time):match("^(%d+):(%d%d)$")
  assert(h, "set.day: the time as \"H:MM\" (\"21:30\"; \"1:59\" the night after)")
  local minute = tonumber(h) * 60 + tonumber(m)
  if minute < D.DAY_START then
    minute = minute + D.MINUTES_A_DAY      -- after midnight: still the same farm day
  end
  assert(minute < D.DAY_END, "set.day: from 2:00 to 6:00 the bunny sleeps")
  local s = world.state
  local now = (day - 1) * D.MINUTES_A_DAY + minute
  s.awake, s.now, s.ticks = s.awake + (now - s.now), now, 0
  barn.merchants()
end

-- The bunny's level, none of its XP towards the next yet; the merchants
-- who come from a level there if it is their hour.
function set.level(n)
  player.state.level, player.state.xp = n, 0
  barn.merchants()
end

function set.coins(n)
  player.state.coins = n
end

function set.carrots(n)
  player.state.carrots = n
end

-- Goods into the bag (no more than an item's stack): set.give("wheat", 40).
-- The bag has room for `slots` kinds if given.
function set.give(item, count, slots)
  local inv = player.state.inventory
  inv[item] = (inv[item] or 0) + count
  if slots then
    player.state.bag = math.max(player.state.bag, slots)
  end
end

-- The field's rows bought (D.FIELD.rows at most), tilled.
function set.rows(n)
  world.state.rows = math.min(n, D.FIELD.rows)
  farm.build()
end

-- Every bed of the open rows sown with `crop`, grown by `share` (1, the
-- default: ripe), and the crops standing in them.
function set.crops(crop, share)
  local s = world.state
  local grow = D.CROPS[crop].grow
  for i in ipairs(s.beds) do
    local _, _, open = farm.bed_cell(i)
    if open then
      s.beds[i] = { crop = crop, total = grow, left = math.floor(grow * (1 - (share or 1))), watered = false, fert = 0 }
    end
  end
  farm.build()
end

-- An animal into its pen, grown unless `how.young`: `how.ready` its product
-- ready, `how.hungry` (else fed and making it), `how.carrying` young (due
-- at once with `how.due`), `how.out` run off to where it waits ({ x, y }).
-- The barn's gift counts as given. Gives back its record.
function set.animal(sp, sex, colour, how)
  how = how or {}
  local b, def, now = world.state.barn, D.BARN.animals[sp], world.state.awake
  local a = { id = b.next, sp = sp, sex = sex, colour = colour, born = how.young and now or now - def.grow, fed = false, carrying = false }
  if how.ready then
    a.fed = now - def.cycle
  elseif not how.hungry and not how.young and sex ~= "m" then
    a.fed = now
  end
  if how.carrying then
    a.fed, a.carrying, a.father = false, how.due and now - def.gestation or now, how.father
  end
  a.out = how.out
  b.next, b.intro = b.next + 1, true
  b.animals[#b.animals + 1] = a
  year.collect(sp .. "_" .. (sex == "r" and "rainbow" or colour))
  barn.sync()
  return a
end

-- A pen's stable built.
function set.stable(sp)
  world.state.barn.stables[sp] = true
  barn.sync()
end

-- A gap in a pen's back fence (in its cell `cell` across, else its first
-- place for one), broken in at once.
function set.gap(sp, cell)
  local b = world.state.barn
  b.gaps = b.gaps or {}
  b.gaps[sp] = cell or D.WOLF.cells[sp][1]
  wolf.paint()
end

-- The bunny at a place of the level, or by the first thing of a kind
-- (the scarecrow), `dx`, `dy` voxels from it, looking the way given (up
-- the screen unless `look` says { x, y }).
function set.bunny_at(where, dx, dy, look)
  local at = level.places[where]
  if not at then
    local t = level:things(where)[1]
    assert(t, "set.bunny_at: no place or thing called " .. tostring(where))
    at = { x = t.x, y = t.y, z = t.z }
  end
  player:place({ x = at.x + (dx or 0), y = at.y + (dy or 0), z = at.z })
  player:face(look or { x = 0, y = -1 })
end

-- The bunny inside a pen at its gap (set.gap first), looking at it.
function set.bunny_at_gap(sp)
  local cell = world.state.barn.gaps[sp]
  player:place({ x = cell * D.CELL + 8, y = D.WOLF.row * D.CELL - 8, z = D.FIELD.top })
  player:face({ x = 0, y = 1 })
end

return set

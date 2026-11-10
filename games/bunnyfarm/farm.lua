-- The field: beds of the farm level's cells (data.lua's FIELD). What each
-- holds is in world.state.beds, a table a bed: the crop growing there (or
-- false), how many minutes of its time are left and how many it had,
-- when it was last watered, how many harvests its fertilizer still has.
-- Crops grow while the bunny is awake (`farm.minute`): the hours it
-- sleeps do not count (else sowing the dearest and sleeping would always
-- pay best, the user). Each crop is a thing of the kind crop that main.lua
-- draws in its stage (level.state.crops, rebuilt from the beds whenever
-- the level is entered: a loaded save too). The rows past the first two
-- are fallow until bought (world.state.rows), then tilled. The day's
-- event may make them grow faster (rain), pay more XP (the harvest
-- festival) or turn into a special (the full moon's berries).
local D = require("data")
local text = require("text")
local bunny = require("bunny")
local market = require("market")
local mail = require("mail")
local calendar = require("calendar")
local year = require("year")
local fx = require("fx")
local farm = {}

local F = D.FIELD
local SOIL = "#6a4830"            -- a tilled bed's earth (as its block's)
local WET = "#4a3020"             -- watered earth
local GRANULE = "#ceb8e8"         -- fertilizer in the earth: the bag's lilac
-- Where a fertilized bed's granules lie in its ridges (in the bed's cell),
-- three for every harvest it still has.
local GRANULES = { { 8, 3 }, { 1, 12 }, { 14, 10 }, { 14, 5 }, { 7, 13 }, { 1, 2 }, { 7, 6 }, { 14, 13 }, { 1, 9 } }

-- An empty bed, its fertilizer `fert` harvests still.
function farm.empty(fert)
  return { crop = false, left = 0, total = 0, watered = false, fert = fert or 0 }
end

-- A bed's number, from 1, at a point in voxels, or nil off the field.
function farm.bed_at(x, y)
  local cx, cy = math.floor(x / D.CELL) - F.x, math.floor(y / D.CELL) - F.y
  if cx < 0 or cx >= F.across or cy < 0 or cy >= F.rows then
    return nil
  end
  return cy * F.across + cx + 1
end

-- A bed's cell (its low corner in voxels), whether it is open yet, and
-- its row (from 1).
function farm.bed_cell(i)
  local cx, cy = (i - 1) % F.across, (i - 1) // F.across
  return (F.x + cx) * D.CELL, (F.y + cy) * D.CELL, cy < world.state.rows, cy + 1
end

-- The bed in front of the bunny, or nil.
function farm.ahead()
  local p = player:ahead(10)
  return farm.bed_at(p.x, p.y)
end

-- How far a bed's crop has grown, 0 to 1 (1: ripe), and its stage 0 to 3.
function farm.growth(bed)
  if bed.left <= 0 then
    return 1, 3
  end
  local share = 1 - bed.left / bed.total
  return share, math.min(2, math.floor(share * 3))
end

-- Whether a bed shows wet (watered not long ago, or in the rain).
function farm.wet(bed)
  return (bed.watered and world.state.awake - bed.watered < D.CAN.wet) or calendar.mod("wet", false)
end

-- A minute awake: every crop's time goes on (in the rain now and then a
-- minute more).
function farm.minute()
  local minutes = calendar.growing(world.state.awake)
  for _, bed in ipairs(world.state.beds) do
    if bed.crop and bed.left > 0 then
      bed.left = math.max(0, bed.left - minutes)
    end
  end
end

-- A crop's thing on its bed. Its model (14 voxels each way, made for the
-- ridges) is turned to the camera, half a turn about its middle voxel,
-- which carries an even size a voxel further on: placed a voxel back from
-- the bed's middle it lies on the ridges (one further, a wet bed's dark
-- earth stood out over its edge, the user saw it).
local function spawn_crop(i)
  local x, y = farm.bed_cell(i)
  level.state.crops[i] = level:spawn("crop", x + D.CELL / 2 - 1, y + D.CELL / 2 - 1, { z = F.top })
end

-- A bought row's beds tilled: its fallow earth turned into two ridges
-- with furrows between, as the open beds are.
local function till(row)
  for cx = 0, F.across - 1 do
    local x, y = (F.x + cx) * D.CELL, (F.y + row - 1) * D.CELL
    local z = F.top - 1
    level:fill(x + 1, y + 1, z - 1, x + 14, y + 14, z - 1, SOIL)
    level:fill(x + 1, y + 1, z, x + 14, y + 14, z, nil)
    level:fill(x + 1, y + 2, z, x + 14, y + 6, z, SOIL)
    level:fill(x + 1, y + 9, z, x + 14, y + 13, z, SOIL)
  end
end

-- How a bed's earth is to look: wet or dry, and its fertilizer.
local function look(bed)
  return (farm.wet(bed) and "wet " or "dry ") .. bed.fert
end

-- A bed's ridges coloured in the earth itself, as a texture: dark where
-- it is wet, the fertilizer's granules in it (thinning out as harvests
-- use it up; every other bed's mirrored, so the field is not stamped
-- alike).
local function paint(i, bed)
  local x, y = farm.bed_cell(i)
  local z = F.top - 1
  local earth = farm.wet(bed) and WET or SOIL
  level:fill(x + 1, y + 2, z, x + 14, y + 6, z, earth)
  level:fill(x + 1, y + 9, z, x + 14, y + 13, z, earth)
  for k = 1, math.min(#GRANULES, bed.fert * 3) do
    local g = GRANULES[k]
    level:set_voxel(x + (i % 2 == 0 and 15 - g[1] or g[1]), y + g[2], z, GRANULE)
  end
end

-- Every open bed's earth as its state says, coloured anew where that
-- changed (watered, dried, fertilized, a harvest used some up; what each
-- shows is in level.state.painted).
function farm.paint()
  level.state.painted = level.state.painted or {}
  for i, bed in ipairs(world.state.beds) do
    local _, _, open = farm.bed_cell(i)
    if open and level.state.painted[i] ~= look(bed) then
      paint(i, bed)
      level.state.painted[i] = look(bed)
    end
  end
end

-- The field as the states say (on entering the level): the bought rows
-- tilled, the beds' earth coloured, the crops' things made anew.
function farm.build()
  for row = F.open + 1, world.state.rows do
    till(row)
  end
  level.state.painted = {}
  farm.paint()
  for _, t in ipairs(level:things("crop")) do
    t:remove()
  end
  level.state.crops = {}
  for i, bed in ipairs(world.state.beds) do
    level.state.crops[i] = false
    if bed.crop then
      spawn_crop(i)
    end
  end
end

-- What the act key can work with on the field, in the order the keys
-- step through it: the seeds owned, the watering can, fertilizer.
function farm.hands()
  local p, hands = player.state, {}
  for _, crop in ipairs(D.CROP_ORDER) do
    if (p.inventory["seed_" .. crop] or 0) > 0 then
      hands[#hands + 1] = crop
    end
  end
  if p.can then
    hands[#hands + 1] = "can"
  end
  if (p.inventory.fertilizer or 0) > 0 then
    hands[#hands + 1] = "fertilizer"
  end
  return hands
end

-- The hand one way (+1) or the other (-1).
function farm.choose(step)
  local hands = farm.hands()
  if #hands == 0 then
    return
  end
  local at = 0
  for i, h in ipairs(hands) do
    if h == player.state.hand then
      at = i
    end
  end
  player.state.hand = at == 0 and hands[1] or hands[(at - 1 + step) % #hands + 1]
end

-- Whether the hand still holds something: a seed or fertilizer used up
-- leaves it for the next thing in it; with nothing else in the bag, the
-- hand is empty again (carrot seeds, none of them).
local function still_held()
  local p = player.state
  if p.hand == "fertilizer" and (p.inventory.fertilizer or 0) == 0 then
    farm.choose(1)
    if p.hand == "fertilizer" then
      p.hand = "carrot"
    end
  elseif D.CROPS[p.hand] and (p.inventory["seed_" .. p.hand] or 0) == 0 then
    farm.choose(1)
  end
end

-- What the act key would do on a bed: "harvest", "sow", "water",
-- "fertilize", "buy_row"; or why not: "growing", "no_seed", "watered",
-- "nothing_to_water", "fertilized", "no_fertilizer", "row_first"; nil
-- off the field.
function farm.would(i)
  if not i then
    return nil
  end
  local _, _, open, row = farm.bed_cell(i)
  if not open then
    return row == world.state.rows + 1 and "buy_row" or "row_first"
  end
  local bed, hand = world.state.beds[i], player.state.hand
  if bed.crop and bed.left <= 0 then
    return "harvest"
  elseif hand == "can" then
    if not bed.crop then
      return "nothing_to_water"
    elseif bed.watered and world.state.awake - bed.watered < D.CAN.every then
      return "watered"
    end
    return (player.state.water or 0) > 0 and "water" or "can_empty"
  elseif hand == "fertilizer" then
    if (player.state.inventory.fertilizer or 0) == 0 then
      return "no_fertilizer"
    end
    return bed.fert > 0 and "fertilized" or "fertilize"
  elseif bed.crop then
    return "growing"
  elseif (player.state.inventory["seed_" .. hand] or 0) > 0 then
    return "sow"
  end
  return "no_seed"
end

-- The act key on a bed. Whether the bunny did some work (its pose); a
-- row to buy opens its board instead (main.lua: "buy_row").
function farm.act(i)
  local what = farm.would(i)
  local bed = world.state.beds[i]
  local x, y = farm.bed_cell(i)
  local at = { x = x + D.CELL / 2, y = y + D.CELL / 2, z = F.top }
  local p = player.state
  if what == "sow" then
    local seed = "seed_" .. p.hand
    p.inventory[seed] = p.inventory[seed] - 1
    if p.inventory[seed] == 0 then
      p.inventory[seed] = nil
    end
    local grow = D.CROPS[p.hand].grow
    bed.crop, bed.watered = p.hand, false
    bed.total = bed.fert > 0 and math.floor(grow * D.FERTILIZER.speed) or grow
    bed.left = bed.total
    market.demand(bed.crop)
    still_held()
    spawn_crop(i)
    bunny.work()
    level:sound("sow", at)
    return true
  elseif what == "harvest" then
    local crop = D.CROPS[bed.crop]
    local yield = bed.fert > 0 and D.FERTILIZER.yield or 1
    local special = calendar.special(bed.crop)
    local item = special or bed.crop
    if crop.carrots then
      p.carrots = p.carrots + crop.carrots * yield
    elseif not bunny.can_hold(item) or player:take(item, yield) < yield then
      text.note("bag_full")
      level:sound("error", at)
      return false
    end
    if special then
      text.note("special", { "specials", special }, yield)
    elseif yield > 1 then
      text.note("double")
    end
    if crop.carrots then
      fx.rise(at, "carrot", crop.carrots * yield)
    else
      fx.rise(at, item, yield)
    end
    world.state.beds[i] = farm.empty(math.max(0, bed.fert - 1))
    if level.state.crops[i] then
      level.state.crops[i]:remove()
      level.state.crops[i] = false
    end
    bunny.work()
    bunny.gain(math.floor(crop.xp * calendar.mod("xp", 1) + 0.5))
    mail.bump("harvest")
    year.count("harvests")
    year.collect(item)
    level:sound("harvest", at)
    return true
  elseif what == "water" then
    bed.left = bed.left - math.min(bed.left // 2, D.CAN.most)
    bed.watered = world.state.awake
    p.water = p.water - 1
    fx.rise(at, "drop")
    bunny.work()
    mail.bump("water")
    level:sound("water", at)
    return true
  elseif what == "fertilize" then
    p.inventory.fertilizer = p.inventory.fertilizer - 1
    if p.inventory.fertilizer == 0 then
      p.inventory.fertilizer = nil
    end
    bed.fert = D.FERTILIZER.charges
    if bed.crop and bed.left > 0 then
      bed.left = math.floor(bed.left * D.FERTILIZER.speed)
    end
    fx.rise(at, "fertilizer")
    still_held()
    bunny.work()
    level:sound("sow", at)
    return true
  elseif what == "growing" then
    local left = math.max(1, bed.left)
    text.note("growing", left // 60, left % 60)
  elseif what then
    text.note(what)
  end
  level:sound("error", at)
  return false
end

-- Whether the bunny faces the pond (where the can is filled).
function farm.at_pond()
  local p, P = player:ahead(10), D.POND
  local cx, cy = math.floor(p.x / D.CELL) - P.x, math.floor(p.y / D.CELL) - P.y
  return cx >= 0 and cx < P.across and cy >= 0 and cy < P.rows
end

-- The can filled at the pond: water for D.CAN.holds beds. Whether there
-- was a can to fill and room in it.
function farm.fill()
  local p = player.state
  if not p.can then
    text.note("no_can")
    return false
  elseif p.water >= D.CAN.holds then
    text.note("can_full")
    return false
  end
  local ahead = player:ahead(10)
  fx.rise({ x = ahead.x, y = ahead.y, z = F.top }, "drop", D.CAN.holds - p.water)
  p.water = D.CAN.holds
  text.note("can_filled")
  return true
end

-- The beds ripe now, in order.
function farm.ripe()
  local list = {}
  for i, bed in ipairs(world.state.beds) do
    local _, _, open = farm.bed_cell(i)
    if open and bed.crop and bed.left <= 0 then
      list[#list + 1] = i
    end
  end
  return list
end

-- Whether the bunny stands by the scarecrow (the field's helper: every
-- ripe bed at once).
function farm.by_scarecrow()
  local s = level:things("scarecrow")[1]
  return s ~= nil and player:near(s, 24)
end

-- Harvest all, the prototype's, at the scarecrow: the bunny hops from
-- ripe bed to ripe bed and harvests each as by hand, D.REAP.fee coins a
-- bed (the time saved costs a little), until all are done, the bag is
-- full or the coins run out; the back key stops it. Its steps live in
-- level.state.reaping, a bed every D.REAP.step ticks. Whether it began.
function farm.reap()
  local beds = farm.ripe()
  if #beds == 0 then
    text.note("nothing_ripe")
    return false
  end
  level.state.reaping = { beds = beds, next = world.tick, done = 0 }
  return true
end

-- A tick of harvesting all: the next bed when its time has come.
function farm.reap_step()
  local r = level.state.reaping
  local over = input:pressed("back")
  if not over and world.tick >= r.next then
    local i = table.remove(r.beds, 1)
    local bed = world.state.beds[i]
    r.next = world.tick + D.REAP.step
    if bed.crop and bed.left <= 0 then
      if player.state.coins < D.REAP.fee then
        text.note("no_coins")
        level:sound("error", player)
        level.state.reaping = nil
        return
      end
      local x, y = farm.bed_cell(i)
      player:place({ x = x + D.CELL / 2, y = y - 2, z = F.top })
      player:face({ x = 0, y = 1 })
      player.state.coins = player.state.coins - D.REAP.fee
      if not farm.act(i) then
        player.state.coins = player.state.coins + D.REAP.fee   -- the bag was full: no fee
        level.state.reaping = nil
        return
      end
      r.done = r.done + 1
    end
    over = #r.beds == 0
  end
  if over then
    if r.done > 0 then
      text.note("reaped", r.done, r.done * D.REAP.fee)
    end
    level.state.reaping = nil
  end
end

-- The next row bought: its beds open and tilled. Whether there were the
-- coins for it.
function farm.buy_row()
  local row = world.state.rows + 1
  local price = D.ROWS[row]
  if not price or player.state.coins < price then
    text.note("no_coins")
    return false
  end
  player.state.coins = player.state.coins - price
  world.state.rows = row
  till(row)
  text.note("row_bought")
  return true
end

return farm

-- The farm year (a run of 56 days): the bunny's year counted as it goes
-- (world.state.year: its number, the farm's worth as it began, what was
-- done in it), the collection over every year (world.state.collection:
-- each kind of crop, dish, special, product and coloured animal the farm
-- ever had), the reckoning when the bunny wakes into a new year
-- (world.state.reckoning, until it is read), and the year's score for
-- the high score table: what the farm's worth grew by in it. Then the
-- farm is saved as the new year begins and the run ends with that
-- score: CONTINUE goes on from there (docs/bunnyfarm.md, stage 6).
local D = require("data")
local clock = require("clock")
local year = {}

-- What the year counts, in this order on the reckoning.
year.STATS = { "harvests", "cooked", "orders", "sold", "fed", "born", "earned" }

-- Everything the collection can hold, in the order it is shown: the
-- crops, the moonberry, the dishes, the barn's products, every animal in
-- every colour.
function year.collectables()
  local list = {}
  for _, crop in ipairs(D.CROP_ORDER) do
    list[#list + 1] = crop
  end
  for _, s in ipairs(D.SPECIAL_ORDER) do
    list[#list + 1] = s
  end
  for _, dish in ipairs(D.RECIPE_ORDER) do
    list[#list + 1] = dish
  end
  for _, product in ipairs(D.BARN.product_order) do
    list[#list + 1] = product
  end
  for _, sp in ipairs(D.BARN.zone_order) do
    for _, colour in ipairs(D.BARN.animals[sp].colours) do
      list[#list + 1] = sp .. "_" .. colour
    end
    if sp == "chicken" then
      list[#list + 1] = "chicken_rainbow"
    end
  end
  return list
end

-- What a thing in the bag is worth (its base price: a crop's or seed's
-- at the market's base, a dish's, a product's), or 0.
local function value(item)
  local crop = D.CROPS[item]
  if crop then
    return crop.sells or 0
  elseif item:sub(1, 5) == "seed_" then
    local c = D.CROPS[item:sub(6)]
    return c.seed or c.seed_carrots * D.MARKET.carrot
  elseif item == "fertilizer" then
    return D.FERTILIZER.price
  elseif item == "jar" then
    return D.KITCHEN.jar
  elseif item == "wood" then
    return D.WOLF.wood
  end
  return D.fixed_price(item) or 0
end

-- The farm's worth now, and its parts: the coins; the goods in the bag
-- and the carrots; the animals (grown at the fox's base, a rare colour
-- for more; the young at the dog's price); the farm itself, what was
-- paid for what lasts (the fields' rows, the pots, the bag's slots, the
-- can, the recipes, the stables).
function year.worth()
  local p, s = player.state, world.state
  local goods = p.carrots * D.MARKET.carrot
  for item, n in pairs(p.inventory) do
    goods = goods + n * value(item)
  end
  local animals = 0
  for _, a in ipairs(s.barn and s.barn.animals or {}) do
    local def = D.BARN.animals[a.sp]
    if s.awake - a.born >= def.grow then
      animals = animals + math.floor(def.sells * (def.rare[a.colour] and D.BARN.rare_sells or 1) + 0.5)
    else
      animals = animals + def.buy
    end
  end
  local farm = 0
  for row = D.FIELD.open + 1, s.rows or D.FIELD.open do
    farm = farm + D.ROWS[row]
  end
  for pots = D.KITCHEN.pots, (s.kitchen and s.kitchen.pots or D.KITCHEN.pots) - 1 do
    farm = farm + D.pot_price(pots)
  end
  for slots = D.BAG.start, (p.bag or D.BAG.start) - 1 do
    farm = farm + D.slot_price(slots)
  end
  if p.can then
    farm = farm + D.CAN.price
  end
  for id in pairs(s.recipes or {}) do
    farm = farm + D.RECIPES[id].price
  end
  for zone, built in pairs(s.barn and s.barn.stables or {}) do
    if built then
      farm = farm + D.BARN.zones[zone].stable
    end
  end
  local parts = { coins = p.coins, goods = goods, animals = animals, farm = farm }
  return p.coins + goods + animals + farm, parts
end

-- A new year's book: its number, the worth it starts from, nothing done.
local function begin(number)
  local stats = {}
  for _, k in ipairs(year.STATS) do
    stats[k] = 0
  end
  world.state.year = { number = number, start = (year.worth()), stats = stats }
end

-- A new farm's: year 1, an empty collection.
function year.fresh()
  begin(1)
  world.state.collection = {}
  world.state.reckoning = false
end

-- An older farm's (from before the year was kept): the year it is in,
-- counted from now.
function year.bring_up_to_date()
  if not world.state.year then
    begin(clock.year(world.state.now))
    world.state.collection = world.state.collection or {}
    world.state.reckoning = false
  end
end

-- Something done this year (`n` times, coins for "earned").
function year.count(stat, n)
  local stats = world.state.year.stats
  stats[stat] = stats[stat] + (n or 1)
end

-- Something the farm had for the first time goes into the collection.
function year.collect(key)
  world.state.collection[key] = true
end

-- How many of the collection the farm has, of how many.
function year.collected()
  local n, all = 0, year.collectables()
  for _, key in ipairs(all) do
    if world.state.collection[key] then
      n = n + 1
    end
  end
  return n, #all
end

-- After a night's sleep: a new year begun? Then its reckoning waits to be
-- read (the year's worth, its parts and what it grew by, what was done),
-- frozen as the old year ended. Whether it did.
function year.check()
  local y = world.state.year
  if clock.year(world.state.now) <= y.number or world.state.reckoning then
    return false
  end
  local worth, parts = year.worth()
  local stats = {}
  for k, v in pairs(y.stats) do
    stats[k] = v
  end
  world.state.reckoning = { number = y.number, worth = worth, parts = parts, gain = math.max(0, worth - y.start), stats = stats }
  return true
end

-- The reckoning read: the year's score goes to the high score table, the
-- next year's book is opened, the farm is saved as it begins it, and the
-- run ends (the engine's initials, its table, the title; CONTINUE goes
-- on into the new year).
function year.close()
  local r = world.state.reckoning
  if not r then
    return
  end
  player.state.score = r.gain
  begin(r.number + 1)
  world.state.reckoning = false
  world:save(1)
  world:win()
end

return year

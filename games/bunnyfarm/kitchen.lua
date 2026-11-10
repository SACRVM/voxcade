-- The kitchen beside the burrow: pots cooking the recipes the bunny owns
-- from what it grew (world.state.kitchen: how many pots, what each cooks
-- and how many minutes it has left; world.state.recipes: those owned).
-- Cooking counts the minutes awake, as growing does; a dish done waits in
-- its pot until it is taken out. The prototype's (its core.js: canCook,
-- startCook, collectCook, buyPot).
local D = require("data")
local text = require("text")
local bunny = require("bunny")
local mail = require("mail")
local year = require("year")
local kitchen = {}

local K = D.KITCHEN

-- A new farm's kitchen: one pot, bread to cook.
function kitchen.fresh()
  world.state.kitchen = { pots = K.pots, jobs = {} }
  for i = 1, K.most do
    world.state.kitchen.jobs[i] = false
  end
  world.state.recipes = { bread = true }
end

-- Whether the kitchen is open yet.
function kitchen.open()
  return player.state.level >= K.level
end

-- A minute awake: every pot cooks on; a dish done says so.
function kitchen.minute()
  for _, job in ipairs(world.state.kitchen.jobs) do
    if job and job.left > 0 then
      job.left = job.left - 1
      if job.left == 0 then
        text.note("cooked", { "dishes", job.recipe })
        level:sound("levelup", player)
      end
    end
  end
end

-- A free pot's number, or nil.
local function free_pot()
  local k = world.state.kitchen
  for i = 1, k.pots do
    if not k.jobs[i] then
      return i
    end
  end
  return nil
end

-- Why a recipe cannot be cooked now ("locked", "not_owned", "no_pot",
-- "missing", "no_jar"), or nil when it can.
function kitchen.why_not(id)
  local r, inv = D.RECIPES[id], player.state.inventory
  if not kitchen.open() then
    return "kitchen_locked"
  elseif not world.state.recipes[id] then
    return "not_owned"
  elseif not free_pot() then
    return "no_pot"
  end
  for item, n in pairs(r.needs) do
    if (inv[item] or 0) < n then
      return "missing"
    end
  end
  if r.jar and (inv.jar or 0) < 1 then
    return "no_jar"
  end
  return nil
end

-- A recipe put on in a free pot: what it needs (and a jar) used up.
-- Whether it could be.
function kitchen.cook(id)
  local why = kitchen.why_not(id)
  if why then
    text.note(why == "kitchen_locked" and "kitchen_locked" or why, K.level)
    return false
  end
  local r, inv = D.RECIPES[id], player.state.inventory
  for item, n in pairs(r.needs) do
    inv[item] = inv[item] - n > 0 and inv[item] - n or nil
  end
  if r.jar then
    inv.jar = inv.jar > 1 and inv.jar - 1 or nil
  end
  world.state.kitchen.jobs[free_pot()] = { recipe = id, left = r.cook, total = r.cook }
  bunny.work()
  return true
end

-- A dish done taken out of pot `i` into the bag. Whether it could be.
function kitchen.take(i)
  local job = world.state.kitchen.jobs[i]
  if not job then
    return false
  elseif job.left > 0 then
    text.note("growing", job.left // 60, job.left % 60)
    return false
  elseif not bunny.can_hold(job.recipe) or player:take(job.recipe) == 0 then
    text.note("bag_full")
    return false
  end
  world.state.kitchen.jobs[i] = false
  bunny.gain(D.RECIPES[job.recipe].xp)
  mail.bump("cook")
  year.count("cooked")
  year.collect(job.recipe)
  return true
end

-- Another pot bought. Whether there was room and the coins for it.
function kitchen.buy_pot()
  local k, p = world.state.kitchen, player.state
  if k.pots >= K.most then
    return false
  end
  local price = D.pot_price(k.pots)
  if p.coins < price then
    text.note("no_coins")
    return false
  end
  p.coins, k.pots = p.coins - price, k.pots + 1
  text.note("pot_bought")
  return true
end

-- The kitchen board's rows: each pot (what cooks, how long yet, or done),
-- each recipe owned (what it needs), another pot to buy.
function kitchen.rows()
  local k, rows = world.state.kitchen, {}
  for i = 1, k.pots do
    rows[#rows + 1] = { pot = i, job = k.jobs[i] }
  end
  for _, id in ipairs(D.RECIPE_ORDER) do
    if world.state.recipes[id] then
      rows[#rows + 1] = { recipe = id, why = kitchen.why_not(id) }
    end
  end
  if k.pots < K.most then
    rows[#rows + 1] = { buy_pot = D.pot_price(k.pots) }
  end
  return rows
end

-- The act key on a row of the kitchen's board.
function kitchen.act(row)
  if row.pot then
    return kitchen.take(row.pot)
  elseif row.recipe then
    return kitchen.cook(row.recipe)
  end
  return kitchen.buy_pot()
end

-- The cat's recipes on offer: those not owned that the bunny's level
-- opens.
function kitchen.offers()
  local list = {}
  for _, id in ipairs(D.RECIPE_ORDER) do
    if not world.state.recipes[id] and player.state.level >= D.RECIPES[id].level then
      list[#list + 1] = id
    end
  end
  return list
end

-- A recipe bought from the cat. Whether there were the coins.
function kitchen.buy_recipe(id)
  local p, price = player.state, D.RECIPES[id].price
  if p.coins < price then
    text.note("no_coins")
    return false
  end
  p.coins, world.state.recipes[id] = p.coins - price, true
  text.note("recipe_bought", { "dishes", id })
  return true
end

return kitchen

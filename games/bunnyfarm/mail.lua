-- The mailbox: orders, the day's goals and the morning gift
-- (world.state.mail). An order asks for goods (crops, dishes) and pays
-- more than the stall would; delivered or turned down, its slot fills
-- again some minutes awake later. The day's goals are rolled each morning
-- and paid as they are done, a bonus when all are. The prototype's rules
-- (its core.js: genOrder, goalsNow, bumpGoal), in game days.
local D = require("data")
local text = require("text")
local bunny = require("bunny")
local year = require("year")
local mail = {}

local O, G = D.ORDERS, D.GOALS

local function between(range)
  return math.random(range[1], range[2])
end

-- What one of an item would sell for at its base: a crop's price, a
-- dish's, a special's.
function mail.worth(item)
  local crop = D.CROPS[item]
  return (crop and crop.sells) or D.fixed_price(item) or 0
end

-- How many of an item an order asks for, from its kind's range.
local function how_many(item)
  if D.RECIPES[item] then
    return between(O.dishes)
  elseif D.SPECIALS[item] then
    return between(O.special)
  elseif D.BARN.products[item] then
    return between(O.products)
  end
  return between(O.crops)
end

-- A new order: goods the bunny can grow or cook by now, more kinds of
-- them the higher its level, and the pay for them.
local function new_order()
  local level, pool = player.state.level, {}
  for _, crop in ipairs(D.CROP_ORDER) do
    if D.CROPS[crop].sells and level >= D.CROPS[crop].level then
      for _ = 1, 3 do
        pool[#pool + 1] = crop
      end
    end
  end
  for _, dish in ipairs(D.RECIPE_ORDER) do
    if level >= D.KITCHEN.level and level >= D.RECIPES[dish].level then
      pool[#pool + 1] = dish
      pool[#pool + 1] = dish
    end
  end
  if level >= O.specials then
    for _, special in ipairs(D.SPECIAL_ORDER) do
      pool[#pool + 1] = special
    end
  end
  for _, sp in ipairs(D.BARN.zone_order) do
    local product = D.BARN.animals[sp].product
    if level >= D.BARN.zones[sp].level and not D.BARN.products[product].unordered then
      pool[#pool + 1] = product
      pool[#pool + 1] = product
    end
  end
  local kinds = level < 3 and 1 or (level < 6 and 2 or 3)
  local want = math.random(1, kinds)
  local goods, worth, count = {}, 0, 0
  for _ = 1, 40 do
    if count >= want then
      break
    end
    local item = pool[math.random(1, #pool)]
    if not goods[item] then
      local n = how_many(item)
      goods[item], worth, count = n, worth + mail.worth(item) * n, count + 1
    end
  end
  local coins = math.max(10, math.floor(worth * O.reward * (1 + O.combo * (count - 1)) + 0.5))
  local list = {}
  for _, item in ipairs(D.CROP_ORDER) do
    if goods[item] then
      list[#list + 1] = { item = item, count = goods[item] }
    end
  end
  for _, order in ipairs({ D.RECIPE_ORDER, D.SPECIAL_ORDER, D.BARN.product_order }) do
    for _, item in ipairs(order) do
      if goods[item] then
        list[#list + 1] = { item = item, count = goods[item] }
      end
    end
  end
  return { goods = list, coins = coins, xp = math.floor(coins * O.xp + 0.5) }
end

-- The day's goals: `count` kinds the bunny can do, in a fixed order made
-- random, each asking a number it pays for.
function mail.roll_goals()
  local kinds = {}
  for _, kind in ipairs(G.order) do
    local needs = G.kinds[kind].needs
    local barn = world.state.barn
    if not needs or (needs == "can" and player.state.can) or (needs == "kitchen" and player.state.level >= D.KITCHEN.level)
        or (needs == "barn" and player.state.level >= D.BARN.level and barn and #barn.animals > 0) then
      kinds[#kinds + 1] = kind
    end
  end
  for i = #kinds, 2, -1 do
    local j = math.random(1, i)
    kinds[i], kinds[j] = kinds[j], kinds[i]
  end
  local goals = {}
  for i = 1, math.min(G.count, #kinds) do
    local k = G.kinds[kinds[i]]
    local need = between(k.count)
    local share = (need - k.count[1]) / math.max(1, k.count[2] - k.count[1])
    local pay = function(range) return math.floor(range[1] + (range[2] - range[1]) * share + 0.5) end
    goals[i] = { kind = kinds[i], need = need, got = 0, coins = pay(k.coins), xp = pay(k.xp) }
  end
  world.state.mail.goals, world.state.mail.bonus = goals, false
end

-- A new farm's mailbox: every order to come at once, a gift and goals.
function mail.fresh()
  world.state.mail = { orders = {}, refill = {}, goals = {}, bonus = false, gift = true }
  for i = 1, O.slots do
    world.state.mail.orders[i], world.state.mail.refill[i] = false, 0
  end
  mail.minute()
  mail.roll_goals()
end

-- A minute awake: empty slots whose time has come get an order.
function mail.minute()
  local m = world.state.mail
  for i = 1, O.slots do
    if not m.orders[i] and world.state.awake >= m.refill[i] then
      m.orders[i] = new_order()
    end
  end
end

-- A new morning: a gift waits, the goals are rolled anew.
function mail.morning()
  world.state.mail.gift = true
  mail.roll_goals()
end

-- Whether the bunny has all an order asks for.
function mail.can_deliver(i)
  local order = world.state.mail.orders[i]
  if not order then
    return false
  end
  for _, g in ipairs(order.goods) do
    if (player.state.inventory[g.item] or 0) < g.count then
      return false
    end
  end
  return true
end

-- Something done for the day's goals: `kind` once more (or `n` times).
function mail.bump(kind, n)
  local m = world.state.mail
  for _, goal in ipairs(m.goals) do
    if goal.kind == kind and goal.got < goal.need then
      goal.got = math.min(goal.need, goal.got + (n or 1))
      if goal.got >= goal.need then
        player.state.coins = player.state.coins + goal.coins
        year.count("earned", goal.coins)
        text.note("goal_done", goal.coins)
        level:sound("coin", player)
        bunny.gain(goal.xp)
        local all = true
        for _, g in ipairs(m.goals) do
          all = all and g.got >= g.need
        end
        if all and not m.bonus then
          m.bonus = true
          player.state.coins = player.state.coins + G.bonus.coins
          year.count("earned", G.bonus.coins)
          text.note("goals_all", G.bonus.coins)
          bunny.gain(G.bonus.xp)
        end
      end
      return
    end
  end
end

-- An order delivered: the goods go, the coins and XP come, the slot
-- fills again later. Whether it could be.
function mail.deliver(i)
  local m, p = world.state.mail, player.state
  local order = m.orders[i]
  if not mail.can_deliver(i) then
    text.note(order and "missing" or "no_order")
    return false
  end
  for _, g in ipairs(order.goods) do
    local left = p.inventory[g.item] - g.count
    p.inventory[g.item] = left > 0 and left or nil
  end
  p.coins = p.coins + order.coins
  year.count("orders")
  year.count("earned", order.coins)
  m.orders[i], m.refill[i] = false, world.state.awake + O.refill
  text.note("order_done", order.coins)
  bunny.gain(order.xp)
  mail.bump("order")
  return true
end

-- An order turned down: its slot fills again later, at no cost.
function mail.reject(i)
  local m = world.state.mail
  if not m.orders[i] then
    return false
  end
  m.orders[i], m.refill[i] = false, world.state.awake + O.refill
  text.note("order_dropped")
  return true
end

-- The morning's gift taken, if there is one waiting. Whether there was.
function mail.take_gift()
  local m, p = world.state.mail, player.state
  if not m.gift then
    return false
  end
  m.gift = false
  p.coins, p.carrots = p.coins + D.GIFT.coins, p.carrots + D.GIFT.carrots
  year.count("earned", D.GIFT.coins)
  text.note("gift", D.GIFT.coins, D.GIFT.carrots)
  return true
end

-- Whether something waits in the mailbox: the gift, or an order the bunny
-- can deliver (its flag is up).
function mail.waiting()
  if world.state.mail.gift then
    return true
  end
  for i = 1, O.slots do
    if mail.can_deliver(i) then
      return true
    end
  end
  return false
end

return mail

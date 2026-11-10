-- The market stall: seeds bought, crops sold, prices that move with what
-- the bunny does (world.state.market: each crop's buy and sell price as a
-- share of its base) and drift back every minute, and with the day's
-- event (a sale, a day a crop is wanted). Its menu: two tabs, buying and
-- selling, a row a good.
local D = require("data")
local text = require("text")
local bunny = require("bunny")
local mail = require("mail")
local calendar = require("calendar")
local year = require("year")
local market = {}

local M = D.MARKET

-- Every price at its base (a new farm).
function market.fresh()
  local m = { buy = {}, sell = {} }
  for _, crop in ipairs(D.CROP_ORDER) do
    m.buy[crop], m.sell[crop] = 1, 1
  end
  return m
end

local function clamp(share)
  return math.max(M.low, math.min(M.high, share))
end

-- What a crop's seed costs now, and what the crop sells for (whole coins,
-- one at least), the day's event taken in. The pumpkin's seed costs
-- carrots, always the same.
function market.seed_price(crop)
  local c = D.CROPS[crop]
  if c.seed_carrots then
    return c.seed_carrots
  end
  local share = world.state.market.buy[crop]
  if calendar.affects(crop) then
    share = share * calendar.mod("seeds", 1)
  end
  return math.max(1, math.floor(c.seed * share + 0.5))
end

function market.sell_price(crop)
  local share = world.state.market.sell[crop]
  if calendar.affects(crop) and calendar.mod("sells", nil) then
    share = (calendar.mod("fixed", false) and 1 or share) * calendar.mod("sells", 1)
  end
  return math.max(1, math.floor(D.CROPS[crop].sells * share + 0.5))
end

-- How the day's event moves a price, as a sign by it: "▲" dearer, "▼"
-- cheaper, nil not at all (`what`: "seeds" or "sells").
function market.moved(crop, what)
  local by = calendar.affects(crop) and calendar.mod(what, 1) or 1
  if what == "seeds" and D.CROPS[crop].seed_carrots then
    by = 1
  end
  return by > 1 and "▲" or (by < 1 and "▼" or nil)
end

-- A seed sown: the demand pushes its price up.
function market.demand(crop)
  local m = world.state.market
  m.buy[crop] = clamp(m.buy[crop] + M.buy_push)
end

-- Minutes gone by: every price closes on its base.
function market.recover(minutes)
  local keep = math.pow(1 - M.recover, minutes)
  local m = world.state.market
  for _, crop in ipairs(D.CROP_ORDER) do
    m.buy[crop] = 1 + (m.buy[crop] - 1) * keep
    m.sell[crop] = 1 + (m.sell[crop] - 1) * keep
  end
end

-- The rows of a tab: what each is (a carrot, a seed, a tool, a slot of
-- the bag, a crop) and how it shows; `pay` "carrot" when it costs carrots.
-- Selling, the crops opened so far.
function market.rows(tab)
  local rows = {}
  local p = player.state
  if tab == 1 then
    rows[1] = { good = "carrot", text = text.say("carrot"), value = M.carrot, icon = "carrot", dim = p.coins < M.carrot }
    for _, crop in ipairs(D.CROP_ORDER) do
      local price = market.seed_price(crop)
      local open = p.level >= D.CROPS[crop].level
      local carrots = D.CROPS[crop].seed_carrots ~= nil
      rows[#rows + 1] = { good = "seed", crop = crop, text = text.of("seeds", crop), value = open and price or text.say("locked_short", D.CROPS[crop].level),
                          icon = "seed_" .. crop, pay = carrots and "carrot" or nil, dim = not open or (carrots and p.carrots or p.coins) < price,
                          moved = open and market.moved(crop, "seeds") or nil }
    end
    -- The can stays in the list once bought (a row going would move the
    -- one under the cursor), dimmed.
    rows[#rows + 1] = { good = "can", text = text.of("tools", "can"), value = p.can and text.say("owned_can") or D.CAN.price, icon = "can",
                        dim = p.can or p.coins < D.CAN.price }
    rows[#rows + 1] = { good = "fertilizer", text = text.of("tools", "fertilizer"), value = D.FERTILIZER.price, icon = "fertilizer", dim = p.coins < D.FERTILIZER.price }
    if p.level >= D.KITCHEN.level then
      rows[#rows + 1] = { good = "jar", text = text.of("tools", "jar"), value = D.KITCHEN.jar, icon = "jar", dim = p.coins < D.KITCHEN.jar }
    end
    if p.bag < D.BAG.most then
      local price = D.slot_price(p.bag)
      rows[#rows + 1] = { good = "slot", text = text.of("tools", "slot"), value = price, icon = "slot", dim = p.coins < price }
    end
    -- wood to mend the pens' fence, once there are pens (wolf.lua)
    if p.level >= D.BARN.level then
      rows[#rows + 1] = { good = "wood", text = text.of("tools", "wood"), value = D.WOLF.wood, icon = "wood", dim = p.coins < D.WOLF.wood }
    end
  else
    for _, crop in ipairs(D.CROP_ORDER) do
      if D.CROPS[crop].sells and p.level >= D.CROPS[crop].level then
        local held = p.inventory[crop] or 0
        rows[#rows + 1] = { good = "crop", crop = crop, text = text.of("crops", crop) .. " ×" .. held, value = market.sell_price(crop),
                            icon = crop, dim = held == 0, moved = market.moved(crop, "sells") }
      end
    end
    -- Dishes, specials and the barn's products, at their fixed price,
    -- those the bunny has.
    for _, list in ipairs({ { D.RECIPE_ORDER, "dishes" }, { D.SPECIAL_ORDER, "specials" }, { D.BARN.product_order, "products" } }) do
      for _, item in ipairs(list[1]) do
        local held = p.inventory[item] or 0
        if held > 0 then
          rows[#rows + 1] = { good = "goods", crop = item, names = list[2], text = text.of(list[2], item) .. " ×" .. held, value = D.fixed_price(item), icon = item }
        end
      end
    end
  end
  return rows
end

-- Paying `price` coins, or saying they are not there.
local function pay(price)
  local p = player.state
  if p.coins < price then
    text.note("no_coins")
    return false
  end
  p.coins = p.coins - price
  return true
end

-- The act key on a row: buy or sell one.
function market.trade(row)
  local p = player.state
  if row.good == "carrot" then
    if not pay(M.carrot) then
      return false
    end
    p.carrots = p.carrots + 1
    text.note("bought", { "crops", "carrot" })
  elseif row.good == "seed" then
    local price = market.seed_price(row.crop)
    local item = "seed_" .. row.crop
    if p.level < D.CROPS[row.crop].level then
      text.note("locked", D.CROPS[row.crop].level)
      return false
    elseif (row.pay == "carrot" and p.carrots or p.coins) < price then
      text.note(row.pay == "carrot" and "no_carrot" or "no_coins")
      return false
    elseif not bunny.can_hold(item) or player:take(item) == 0 then
      text.note("bag_full")
      return false
    end
    if row.pay == "carrot" then
      p.carrots = p.carrots - price
    else
      p.coins = p.coins - price
    end
    if D.CROPS[p.hand] and (p.inventory["seed_" .. p.hand] or 0) == 0 then
      p.hand = row.crop                -- an empty hand takes the seeds bought
    end
    text.note("bought", { "seeds", row.crop })
  elseif row.good == "can" then
    if p.can then
      text.note("owned_can")
      return false
    elseif not pay(D.CAN.price) then
      return false
    end
    p.can, p.water = true, D.CAN.holds    -- full as it comes
    text.note("bought", { "tools", "can" })
  elseif row.good == "jar" then
    if p.coins < D.KITCHEN.jar then
      text.note("no_coins")
      return false
    elseif not bunny.can_hold("jar") or player:take("jar") == 0 then
      text.note("bag_full")
      return false
    end
    p.coins = p.coins - D.KITCHEN.jar
    text.note("bought", { "tools", "jar" })
  elseif row.good == "fertilizer" or row.good == "wood" then
    local price = row.good == "wood" and D.WOLF.wood or D.FERTILIZER.price
    if p.coins < price then
      text.note("no_coins")
      return false
    elseif not bunny.can_hold(row.good) or player:take(row.good) == 0 then
      text.note("bag_full")
      return false
    end
    p.coins = p.coins - price
    text.note("bought", { "tools", row.good })
  elseif row.good == "slot" then
    if p.bag >= D.BAG.most then
      text.note("full_bag")
      return false
    elseif not pay(D.slot_price(p.bag)) then
      return false
    end
    p.bag = p.bag + 1
    text.note("bag_grown", p.bag)
  else
    local held = p.inventory[row.crop] or 0
    if held == 0 then
      text.note("none_to_sell")
      return false
    end
    local fixed = row.good == "goods"
    local price = fixed and D.fixed_price(row.crop) or market.sell_price(row.crop)
    p.inventory[row.crop] = held > 1 and held - 1 or nil
    p.coins = p.coins + price
    if not fixed then
      local m = world.state.market
      m.sell[row.crop] = clamp(m.sell[row.crop] - M.sell_push)
    end
    text.note("sold", { fixed and row.names or "crops", row.crop }, price)
    mail.bump("sell")
    year.count("sold")
    year.count("earned", price)
  end
  return true
end

return market

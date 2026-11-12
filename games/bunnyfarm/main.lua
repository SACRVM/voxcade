-- BunnyFarm: you are the bunny. Sow and harvest the field, sell at the
-- market stall, eat a carrot when hungry, sleep in the burrow
-- (docs/bunnyfarm.md). The rules live in the files beside this one:
-- data.lua the numbers, clock.lua the day and its light, farm.lua the
-- field, market.lua the stall, bunny.lua the bunny's needs, kitchen.lua
-- the pots, mail.lua the orders, goals and gift, calendar.lua the week's
-- events, barn.lua the pens, their animals and the dog and the fox,
-- wolf.lua the pens' fence and the wolf at night, year.lua the farm year
-- and its reckoning, hud.lua what the screen shows, fx.lua its small
-- signs, text.lua and lang/ the words in three languages.
local D = require("data")
local text = require("text")
local clock = require("clock")
local bunny = require("bunny")
local market = require("market")
local farm = require("farm")
local kitchen = require("kitchen")
local mail = require("mail")
local calendar = require("calendar")
local barn = require("barn")
local year = require("year")
local hud = require("hud")
local fx = require("fx")
local wolf = require("wolf")

-- A new farm: day 1 at 6:00, every bed empty, every price at its base,
-- the bunny with a few coins, a carrot and some seeds. world.state.awake
-- counts the minutes the bunny has been awake: crops grow and prices
-- recover by it, not by the clock, which sleep moves on.
function on_world_start()
  local s = world.state
  s.now, s.ticks, s.awake = D.DAY_START, 0, D.DAY_START
  s.rows, s.beds = D.FIELD.open, {}
  for i = 1, D.FIELD.across * D.FIELD.rows do
    s.beds[i] = farm.empty()
  end
  s.market = market.fresh()
  local p = player.state
  p.coins, p.carrots, p.xp, p.level = D.START.coins, D.START.carrots, 0, 1
  p.bag, p.can, p.hand = D.BAG.start, false, "carrot"
  for need, value in pairs(D.NEEDS.start) do
    p[need] = value
  end
  for item, count in pairs(D.START.seeds) do
    p.inventory[item] = count
  end
  kitchen.fresh()
  mail.fresh()
  barn.fresh()
  year.fresh()
end

-- The states of an older farm brought up to date: version 1 had no awake
-- clock (crops were sown by the clock: it starts where the clock stands),
-- version 2 no rows to buy, bag slots, watering can, hand (a bed was false
-- or { crop, sown }; the hand was the chosen seed) or prices for the crops
-- that came since, version 3 no kitchen or mailbox, version 4 no barn,
-- version 5 no year.
local function bring_up_to_date()
  local s, p = world.state, player.state
  s.awake = s.awake or s.now
  if p.can and not p.water then
    p.water = D.CAN.holds                     -- a can from before it had to be filled
  end
  if not s.kitchen then
    kitchen.fresh()
  end
  if not s.barn then
    barn.fresh()
  end
  if s.rows then
    year.bring_up_to_date()
  end
  if not s.mail and s.rows then
    mail.fresh()
  end
  if not s.market.buy.pumpkin then            -- crops new since: prices at their base
    for _, crop in ipairs(D.CROP_ORDER) do
      s.market.buy[crop], s.market.sell[crop] = s.market.buy[crop] or 1, s.market.sell[crop] or 1
    end
  end
  if s.rows then
    return
  end
  s.rows = D.FIELD.open
  for i, bed in ipairs(s.beds) do
    if bed then
      local grow = D.CROPS[bed.crop].grow
      s.beds[i] = { crop = bed.crop, total = grow, left = math.max(0, grow - (s.awake - bed.sown)), watered = false, fert = 0 }
    else
      s.beds[i] = farm.empty()
    end
  end
  p.hand, p.seed = p.seed or "carrot", nil
  p.bag, p.can = D.BAG.start, false
  mail.fresh()
end

function on_save_loaded(version)
  if version < 6 then
    bring_up_to_date()
  end
end


-- The year's reckoning, read before the farm goes on (time stands still).
local function open_reckoning()
  level.state.menu = { kind = "reckoning", at = 1, tab = 1, top = 1 }
  level.state.held = true
end

-- The farm entered (a new run, a loaded save): the crops from the beds.
function on_level_start(how)
  bring_up_to_date()
  farm.build()
  barn.build_level()
  wolf.build()
  level.state.menu, level.state.cat, level.state.dog, level.state.fox = false, false, false, false
  if level.state.held then
    level.state.held = nil    -- saved with a menu open (paused there): the menu closed, the farm let go
  end
  barn.merchants()
  if how == "start" then
    player:face({ x = 0, y = -1 })
    level.state.morning = { at = world.tick, fell = false }   -- the first day says its event too
  elseif how == "load" then
    -- a farm is saved as the bunny wakes in the burrow: it goes on from
    -- there (a save from an older farm, laid out differently, had it
    -- elsewhere, outside the fence even)
    player:place(level.places.burrow)
    player:face({ x = 0, y = -1 })
  end
  if world.state.reckoning then
    open_reckoning()          -- a save made as the year ended: its reckoning is still to be read
  end
end

-- Whether the bunny may not stand at x, y: in the pond (its blocks are
-- hazards), or within the footprint of something standing there (its
-- kind's props.solid, half each way).
local function blocked(x, y)
  if level:block(x, y, D.FIELD.top - 1) == "hazard" then
    return true
  end
  for _, t in ipairs(level:things()) do
    local half = t.props.solid
    if half and math.abs(x - t.x) < half[1] + 3 and math.abs(y - t.y) < half[2] + 3 then
      return true
    end
  end
  return false
end

-- Walking the 8 ways at the bunny's pace, round what stands about.
local function walk()
  local d = input.dir
  if d.x == 0 and d.y == 0 then
    return
  end
  local x, y = d.x, d.y
  if x ~= 0 and blocked(player.x + x * 3, player.y) then
    x = 0
  end
  if y ~= 0 and blocked(player.x + x * 3, player.y + y * 3) then
    y = 0
  end
  if x == 0 and y == 0 then
    player:face(d)
    return
  end
  player:walk({ x = x, y = y }, bunny.speed())
end

local function open(kind)
  level.state.menu = { kind = kind, at = 1, tab = 1, top = 1 }
  level.state.held = true
  level:sound("open", player)
end

local function close()
  level.state.menu = false
  level.state.held = false
end

-- To sleep till the morning: the hours skipped count for nothing that
-- grows or recovers; the night's news (the wolf, a gap in a fence) said
-- in the morning.
local function sleep(fell)
  close()
  bunny.sleep(fell)
  level.state.morning.news = wolf.night()
  mail.morning()
  barn.merchants()
  if year.check() then
    open_reckoning()
  end
end

-- A row up or down in a list of count rows, round at the ends.
local function move(menu, count)
  if input:repeated("up") then
    menu.at = (menu.at - 2) % count + 1
  elseif input:repeated("down") then
    menu.at = menu.at % count + 1
  end
end

-- Each menu's keys, while it is open (the time stands still).
local MENUS = {
  market = function(menu)
    if input:pressed("left") or input:pressed("right") or input:pressed("prev") or input:pressed("next") then
      menu.tab, menu.at = 3 - menu.tab, 1
    end
    local rows = market.rows(menu.tab)
    menu.at = math.max(1, math.min(menu.at, #rows))   -- the last of a dish sold, its row goes
    move(menu, #rows)
    if input:pressed("act") then
      local r = rows[menu.at]
      if market.trade(r) then
        level:sound("coin", player)
        -- the coins to the purse for a sale; a buy's goods to where they are
        -- kept, a place in the bag paid for with coins from the purse
        local row = { row = menu.at }
        if menu.tab == 2 then
          fx.fly("coin", row, "coins")
        elseif r.good == "carrot" then
          fx.fly("carrot", row, "carrots")
        elseif r.good == "slot" then
          fx.fly("coin", "coins", row)
        else
          fx.fly(r.icon, row, "bag")
        end
      else
        level:sound("error", player)
      end
    end
  end,
  burrow = function(menu)
    move(menu, 2)
    if input:pressed("act") then
      if menu.at == 1 then
        bunny.eat()
      elseif clock.minute(world.state.now) < D.BEDTIME then
        text.note("too_early", D.BEDTIME // 60)
        level:sound("error", player)
      else
        sleep(false)
      end
    end
  end,
  bag = function(menu)
    local slots, across = player.state.bag, hud.bag_columns()
    if input:repeated("left") then
      menu.at = (menu.at - 2) % slots + 1
    elseif input:repeated("right") then
      menu.at = menu.at % slots + 1
    elseif input:repeated("up") then
      menu.at = (menu.at - 1 - across) % slots + 1
    elseif input:repeated("down") then
      menu.at = (menu.at - 1 + across) % slots + 1
    end
    -- Seeds or fertilizer chosen there go into the hand.
    local slot = hud.bag()[menu.at]
    if input:pressed("act") and slot and (slot.item:sub(1, 5) == "seed_" or slot.item == "fertilizer") then
      player.state.hand = slot.item == "fertilizer" and "fertilizer" or slot.item:sub(6)
      level:sound("open", player)
      close()
    end
  end,
  -- The kitchen: take a dish out, put a recipe on, buy a pot.
  kitchen = function(menu)
    local rows = kitchen.rows()
    move(menu, #rows)
    if input:pressed("act") then
      local r = rows[menu.at]
      local done = kitchen.act(r)
      level:sound(done and "open" or "error", player)
      if done and r.buy_pot then
        fx.fly("coin", "coins", { row = menu.at })   -- the pot paid
      end
    end
  end,
  -- The mailbox: its orders (deliver, or turn one down with the bag key)
  -- and the day's goals, two tabs.
  mailbox = function(menu)
    if input:pressed("left") or input:pressed("right") or input:pressed("prev") or input:pressed("next") then
      menu.tab, menu.at = 3 - menu.tab, 1
    end
    if menu.tab == 1 then
      move(menu, D.ORDERS.slots)
      if input:pressed("act") then
        level:sound(mail.deliver(menu.at) and "coin" or "error", player)
      elseif input:pressed("bag") then
        level:sound(mail.reject(menu.at) and "open" or "error", player)
      end
    end
  end,
  -- The cat: buy a recipe.
  cat = function(menu)
    local offers = kitchen.offers()
    if #offers == 0 then
      return
    end
    menu.at = math.min(menu.at, #offers)
    move(menu, #offers)
    if input:pressed("act") then
      level:sound(kitchen.buy_recipe(offers[menu.at]) and "coin" or "error", player)
    end
  end,
  -- A pen: an animal fed, or its product taken (the act key); the bag key
  -- on a male makes him the father, on a female gives her young (by the
  -- father chosen, or the pen's first grown male); the stable built.
  pen = function(menu)
    local rows = barn.pen_rows(menu.zone)
    menu.at = math.max(1, math.min(menu.at, #rows))
    move(menu, #rows)
    local row = rows[menu.at]
    if input:pressed("act") then
      if row.animal then
        level:sound(barn.act(row.animal) and "open" or "error", player)
      else
        level:sound(barn.build(menu.zone) and "coin" or "error", player)
        barn.sync()
      end
    elseif input:pressed("bag") and row.animal then
      local a = row.animal
      if a.sex == "m" then
        menu.sire = a.id
        level:sound("open", player)
      else
        local m = (menu.sire and barn.animal(menu.sire)) or barn.first_male(a.sp)
        level:sound(barn.breed(a, m) and "levelup" or "error", player)
      end
    end
  end,
  -- The dog: a young animal bought into its pen.
  dog = function(menu)
    local stock = barn.roll_stock()
    local count = stock and #stock.items or 0
    if count == 0 then
      return
    end
    menu.at = math.max(1, math.min(menu.at, count))
    move(menu, count)
    if input:pressed("act") then
      level:sound(barn.buy(menu.at) and "coin" or "error", player)
      barn.sync()
    end
  end,
  -- The fox: a grown animal sold to her.
  fox = function(menu)
    local list = barn.for_sale()
    if #list == 0 then
      return
    end
    menu.at = math.max(1, math.min(menu.at, #list))
    move(menu, #list)
    if input:pressed("act") then
      level:sound(barn.sell(list[menu.at]) and "coin" or "error", player)
      barn.sync()
    end
  end,
  -- The year's reckoning: the act key goes on into the next year (the
  -- score into the table, the farm saved, the run ended; CONTINUE goes
  -- on from there).
  reckoning = function(menu)
    if input:pressed("act") then
      close()
      level.state.morning = { at = world.tick, fell = false }
      level:sound("levelup", player)
      year.close()
    end
  end,
  -- A fallow row: buy it, or not.
  row = function(menu)
    move(menu, 2)
    if input:pressed("act") then
      if menu.at == 1 then
        level:sound(farm.buy_row() and "coin" or "error", player)
      end
      close()
    end
  end,
}

-- The pen whose gate the bunny stands at, or nil.
local function at_gate()
  for _, z in ipairs(D.BARN.zone_order) do
    if bunny.at(z .. "_gate") then
      return z
    end
  end
  return nil
end

-- The merchant the bunny stands by, or nil.
local function by_merchant()
  for _, m in ipairs(barn.MERCHANTS) do
    local t = level.state[m[1]]
    if t and player:near(t, 22) then
      return m[1]
    end
  end
  return nil
end

-- The act key: a place opens (the mailbox gives its gift first, the
-- chicken yard the barn's the first time), a merchant talks, an animal
-- run off is fetched, a gap in a fence mended, else the bed in front.
local function act()
  local gate, merchant, stray, gap = at_gate(), by_merchant(), wolf.stray(), wolf.gap_here()
  if bunny.at("stall") then
    open("market")
  elseif bunny.at("burrow") then
    open("burrow")
  elseif bunny.at("kitchen") then
    if kitchen.open() then
      open("kitchen")
    else
      text.note("kitchen_locked", D.KITCHEN.level)
      level:sound("error", player)
    end
  elseif bunny.at("mailbox") then
    if mail.take_gift() then
      level:sound("coin", player)
      local box = level.places.mailbox
      local from = { x = box.x, y = box.y, z = box.z + 12 }
      fx.fly("coin", from, "coins")
      fx.fly("carrot", from, "carrots")
    end
    open("mailbox")
  elseif gate then
    if barn.open(gate) then
      if gate == "chicken" and barn.intro() then
        level:sound("coin", player)
        barn.sync()
      end
      open("pen")
      level.state.menu.zone = gate
    else
      text.note("pen_locked", D.BARN.zones[gate].level)
      level:sound("error", player)
    end
  elseif merchant then
    open(merchant)
  elseif stray then
    wolf.lead(stray)
    level:sound("open", player)
  elseif gap then
    if not wolf.mend(gap) then
      level:sound("error", player)
    end
  else
    local i = farm.ahead()
    if farm.would(i) == "buy_row" then
      open("row")
    elseif i then
      farm.act(i)
    elseif farm.by_scarecrow() then
      level:sound(farm.reap() and "open" or "error", player)
    elseif farm.at_pond() then
      level:sound(farm.fill() and "water" or "error", player)
    end
  end
end

-- A tick of game time: a minute every 42 ticks; at 2:00 the bunny falls
-- asleep.
local function pass_time()
  local s = world.state
  s.ticks = s.ticks + 1
  if s.ticks < D.TICKS_A_MINUTE then
    return
  end
  s.ticks, s.now, s.awake = 0, s.now + 1, s.awake + 1
  bunny.minute()
  farm.minute()
  kitchen.minute()
  mail.minute()
  barn.minute()
  barn.merchants()
  market.recover(1)
  if clock.minute(s.now) >= D.DAY_END then
    sleep(true)
  end
end

function on_tick()
  bring_up_to_date()        -- a farm whose code was saved over it while playing
  if not (level.state.herd and level.state.eggs and level.state.stables) then
    barn.build_level()      -- the barn's things, for a level entered before it came
  end
  farm.paint()              -- the beds' earth as their states say (wet, fertilized)
  wolf.paint()              -- the pens' fence too (its gaps)
  fx.tick()
  local menu = level.state.menu
  if menu then
    if menu.kind ~= "reckoning" and (input:pressed("back") or (menu.kind == "bag" and input:pressed("bag"))) then
      close()
    else
      MENUS[menu.kind](menu)
    end
    return
  end
  if level.state.reaping then
    farm.reap_step()          -- harvesting all: the bunny hops from bed to bed
  elseif level.state.mending then
    wolf.mend_step()          -- mending a fence: three blows of the hammer
  else
    walk()
    if input:pressed("prev") then
      farm.choose(-1)
    elseif input:pressed("next") then
      farm.choose(1)
    end
    if input:pressed("bag") then
      open("bag")
      return
    end
    if input:pressed("act") then
      act()
    end
  end
  pass_time()
  barn.sync()
  barn.wander()
  wolf.tick()               -- an animal run off following the bunny home
  local song = clock.night(world.state.now) and "quiet_warehouse" or "sunny_hills"
  if music.song ~= song then
    music:play(song, "bar")
  end
  if level.state.menu and level.state.menu.kind == "dog" then
    barn.roll_stock()         -- the dog's board just opened: his animals before it is drawn
  end
end

-- Corner marks round the bed the act key works on: yellow to harvest,
-- light blue to water, white otherwise.
local MARKS = { harvest = { 255, 226, 110 }, water = { 150, 206, 255 } }

local function mark_bed(frame)
  local i = farm.ahead()
  local would = farm.would(i)
  if not would or would == "row_first" then
    return
  end
  local x, y = farm.bed_cell(i)
  local z, color = D.FIELD.top, MARKS[would] or { 246, 240, 226 }
  for _, c in ipairs({ { 0, 0, 1, 1 }, { 15, 0, -1, 1 }, { 0, 15, 1, -1 }, { 15, 15, -1, -1 } }) do
    for k = 0, 2 do
      frame:cube(x + c[1] + c[3] * k, y + c[2], z, color)
      frame:cube(x + c[1], y + c[2] + c[4] * k, z, color)
    end
  end
end

-- The kitchen with its pots (as many as bought), steam rising from those
-- that cook, the oven glowing while one does.
local function kitchen_look(frame, t)
  local k = world.state.kitchen
  frame:model("kitchen.vvx", t, k.pots - 1)
  local cooking = false
  for i = 1, k.pots do
    local job = k.jobs[i]
    if job and job.left > 0 then
      cooking = true
      -- the pots stand along the counter, 5 voxels apart, beside the oven
      local px, py = t.x + 11 - (i - 1) * 5, t.y
      for n = 0, 2 do
        local rise = (world.tick + n * 23 + i * 11) % 60 + frame.alpha
        frame:cube(px + ((n + i) % 2), py, t.z + 12 + rise / 6, { 250, 250, 250, 220 - math.floor(rise * 3.5) })
      end
    end
  end
  if cooking then
    frame:light({ x = t.x - 9, y = t.y - 5, z = t.z + 3 }, 30, { 255, 150, 70 })
  end
end

-- An animal as it is now: the young small; walking, pecking or grazing
-- now and then while it stands; a hen brooding on her nest, a sheep's
-- fleece as it grows back (full: ready to be shorn; a ram keeps half his),
-- a cow's udder full; frames of its colour's model (tools/gen_bunnyfarm.py).
local function animal_look(frame, t)
  local a = barn.animal(t.state.animal)
  if not a then
    return
  end
  local s = barn.status(a)
  local walking = t.state.to and t.state.wait == 0
  local step = (world.tick // 8) % 2
  local idle = (world.tick // 40 + a.id) % 4 == 0
  local index
  if a.sp == "chicken" then
    if not s.grown then
      index = walking and 6 or 5
    elseif s.carrying then
      index = 4
    else
      index = walking and 1 + step or (idle and 3 or 0)
    end
  elseif a.sp == "sheep" then
    if not s.grown then
      index = walking and 8 or 7
    else
      local fleece = s.ready and 0 or ((a.sex == "m" or s.share >= 0.5) and 2 or 4)
      index = walking and fleece + 1 or ((fleece == 0 and idle) and 6 or fleece)
    end
  else
    if not s.grown then
      index = walking and 6 + step or 5
    elseif s.ready then
      index = 4
    else
      index = walking and 1 + step or (idle and 3 or 0)
    end
  end
  frame:model(a.sp .. "_" .. a.colour .. ".vvx", t, index)
end

-- Rain over what the camera sees: drops, each a short streak of
-- see-through voxels, falling FALL ticks from HIGH voxels up and landing
-- somewhere new each time (a scatter of their number and round: the
-- picture draws no random numbers of the game's).
local DROPS, FALL, HIGH, LONG = 280, 30, 130, 4
local RAIN = { 206, 222, 244, 190 }

local function scatter(n)
  for _ = 1, 2 do
    n = (n * 1103515245 + 12345) % 2147483648
  end
  return n // 16
end

local function rain(frame)
  local t = world.tick + frame.alpha
  for i = 1, DROPS do
    local phase = scatter(i) % FALL
    local fall = (t + phase) % FALL
    local spot = scatter(i * 7919 + (world.tick + phase) // FALL)
    local x = player.x - 210 + spot % 420
    local y = player.y - 170 + (spot // 420) % 340
    local z = D.FIELD.top + HIGH * (1 - fall / FALL)
    for k = 0, LONG - 1 do
      frame:cube(x + fall * 0.25, y, z + k, RAIN)
    end
  end
end

-- On the full moon's night a ripe crop that would turn into a special
-- shimmers, a hint at what harvesting it gives (calendar.special): a
-- faint silver light, and its berries glint, each now and then for
-- SPARK of every TWINKLE ticks. BERRIES: the empty cell above or beside
-- each berry of the ripe blueberry model (x, y, z in it; the model is
-- 14 across and turned to face the camera).
local TWINKLE, SPARK = 60, 10
local SILVER = { 226, 234, 255 }
local BERRIES = {
  { 0, 3, 3 }, { 0, 10, 3 }, { 1, 1, 1 }, { 1, 8, 1 }, { 3, 1, 5 }, { 3, 3, 6 }, { 3, 8, 5 }, { 3, 10, 6 },
  { 5, 5, 3 }, { 5, 12, 3 }, { 6, 2, 2 }, { 6, 9, 2 }, { 7, 3, 3 }, { 7, 10, 3 }, { 8, 1, 1 }, { 8, 8, 1 },
  { 10, 1, 5 }, { 10, 3, 6 }, { 10, 8, 5 }, { 10, 10, 6 }, { 12, 5, 3 }, { 12, 12, 3 }, { 13, 2, 2 }, { 13, 9, 2 },
}

local function shimmer(frame, t, i)
  frame:light({ x = t.x, y = t.y, z = t.z + 6 }, 20, { 150, 175, 255 })
  for k, b in ipairs(BERRIES) do
    if (world.tick + scatter(i * 24 + k)) % TWINKLE < SPARK then
      frame:cube(t.x + 6 - b[1], t.y + 6 - b[2], t.z + b[3], SILVER, 1)
    end
  end
end

-- The bunny's frame for the work it does (only the picture, read from
-- the bed in front): the can pouring (its model's 9 and 10, in turn)
-- while a bed it watered this minute lies before it, the spade (11 in
-- the earth, 12 lifted) while one it has just sown; the hammer at a fence
-- (wolf.lua); else its work pose (5) a moment after any work.
local TOOL, POSE = 30, 16                 -- ticks a tool shows, the work pose

local function bunny_frame()
  local hammer = wolf.hammer()
  if hammer then
    return hammer
  end
  local since = level.state.working and world.tick - level.state.working
  if not since or since >= TOOL then
    return nil
  end
  local i = farm.ahead()
  local bed, hand = i and world.state.beds[i], player.state.hand
  if bed and hand == "can" and bed.watered and world.state.awake - bed.watered <= 1 then
    return 9 + (world.tick // 5) % 2
  elseif bed and bed.crop == hand and not bed.watered and bed.total - bed.left <= 1 then
    return since < 10 and 11 or 12
  end
  return since < POSE and 5 or nil
end

function on_draw(frame)
  local s = world.state
  if not s.rows then
    return                  -- an older farm, brought up to date at the next tick
  end
  local moving = level.state.menu and 0 or frame.alpha
  local now = s.now + (s.ticks + moving) / D.TICKS_A_MINUTE
  frame:daylight(calendar.light(clock.daylight(now), now))
  if calendar.mod("weather", nil) == "rain" then
    rain(frame)
  end

  frame:model("bunny.vvx", player, bunny_frame())
  -- What stands about, each by its kind's model (trees with apples or
  -- without, by their number); the burrow's lantern and windows light up
  -- by dusk.
  local hour = clock.minute(s.now) / 60
  for _, t in ipairs(level:things()) do
    local kind = t.kind     -- read once: each read asks the engine (hundreds of things a picture)
    if kind == "kitchen" then
      kitchen_look(frame, t)
    elseif kind == "mailbox" then
      frame:model("mailbox.vvx", t, mail.waiting() and 1 or 0)
    elseif kind == "cat" or kind == "dog" or kind == "fox" then
      frame:model(kind .. ".vvx", t, (world.tick // 90 + t.id) % 3 == 0 and 1 or 0)
    elseif D.BARN.animals[kind] then
      animal_look(frame, t)
    elseif kind == "egg" then
      frame:model("egg.vvx", t, t.state.rainbow and 1 or 0)
    elseif t.props.picture then
      frame:model(kind:match("^sign_") and "pen_sign.vvx" or "trough.vvx", t, t.props.picture)
    elseif kind ~= "crop" then
      frame:model(kind .. ".vvx", t, kind == "tree" and t.id % 2 or 0)
    end
    if kind == "burrow" and (hour >= 19 or hour < 7) then
      frame:light({ x = t.x + 18, y = t.y - 15, z = t.z + 12 }, 56, { 255, 190, 110 })
      frame:light({ x = t.x, y = t.y - 16, z = t.z + 8 }, 40, { 255, 200, 130 })
    end
  end
  for i, t in ipairs(level.state.crops) do
    local bed = s.beds[i]
    if t and bed.crop then
      local _, stage = farm.growth(bed)
      frame:model(bed.crop .. ".vvx", t, stage)
      if bed.left <= 0 and calendar.special(bed.crop) then
        shimmer(frame, t, i)
      end
    end
  end
  if not level.state.menu then
    mark_bed(frame)
  end
  local spots = hud.spots(frame)
  fx.draw(frame, spots, "over")
  hud.draw(frame)
  fx.draw(frame, spots, "under")
  wolf.draw(frame)
end

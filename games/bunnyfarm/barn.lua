-- The barn: three pens behind the burrow, the animals in them, the dog
-- who sells young ones and the fox who buys grown ones (world.state.barn;
-- the prototype's rules, its core.js: animalState, settleBarn, feedAnimal,
-- collectAnimal, canBreed, breedAnimal, rollDogStock, buyAnimal,
-- sellAnimal, buyStable, barnIntro). Its times count the minutes awake,
-- as the crops' do. Each animal: { id, sp, sex ("m", "f", or "r" the
-- rainbow hen's), colour, born, fed (the minute it was fed, or false),
-- carrying (the minute it began, or false), father (his colour), out
-- (where it waits after running off from the wolf, wolf.lua) }.
local D = require("data")
local clock = require("clock")
local text = require("text")
local bunny = require("bunny")
local mail = require("mail")
local year = require("year")
local fx = require("fx")
local barn = {}

local B = D.BARN

-- A new farm's barn: no animals, no stables, the gift to come.
function barn.fresh()
  world.state.barn = { animals = {}, next = 1, stables = {}, bred = {}, last_buy = -99, stock = false, intro = false }
  for _, z in ipairs(B.zone_order) do
    world.state.barn.stables[z] = false
  end
end

local function awake()
  return world.state.awake
end

-- Whether the barn, or a pen, is open by the bunny's level.
function barn.open(zone)
  return player.state.level >= (zone and B.zones[zone].level or B.level)
end

-- The animals of a pen (a species), in the order they came.
function barn.animals(zone)
  local list = {}
  for _, a in ipairs(world.state.barn.animals) do
    if a.sp == zone then
      list[#list + 1] = a
    end
  end
  return list
end

-- An animal by its id, or nil.
function barn.animal(id)
  for _, a in ipairs(world.state.barn.animals) do
    if a.id == id then
      return a
    end
  end
  return nil
end

-- How many animals a pen holds (twice as many with its stable), how many
-- more it can take.
function barn.places(zone)
  return world.state.barn.stables[zone] and B.zones[zone].most or B.zones[zone].places
end

function barn.room(zone)
  return barn.places(zone) - #barn.animals(zone)
end

-- How long a pen's animals are frightened yet (minutes of the clock), or
-- nil (the wolf's visit, wolf.lua).
function barn.scared(zone)
  local till = world.state.barn.scared and world.state.barn.scared[zone]
  return till and till > world.state.now and till - world.state.now or nil
end

-- How an animal is now: grown or how long yet; hungry (a grown female
-- not fed, not carrying), making its product and how long yet, the
-- product ready (not while it is frightened or away), carrying young and
-- how long yet, due (waiting for room), away (run off), scared (how long
-- yet).
function barn.status(a)
  local def, now = B.animals[a.sp], awake()
  local s = { grown = now - a.born >= def.grow }
  s.grow_left = math.max(0, def.grow - (now - a.born))
  s.carrying = a.carrying ~= false
  s.carry_left = s.carrying and math.max(0, def.gestation - (now - a.carrying)) or 0
  s.due = s.carrying and s.carry_left == 0
  s.away, s.scared = a.out ~= nil, barn.scared(a.sp)
  local working = s.grown and not s.carrying and a.fed ~= false
  s.working = working
  s.ready = working and not s.scared and not s.away and now - a.fed >= def.cycle
  s.making = working and not s.ready
  s.left = working and math.max(0, def.cycle - (now - a.fed)) or 0
  s.share = working and math.min(1, (now - a.fed) / def.cycle) or 0
  s.hungry = s.grown and not s.carrying and a.fed == false and a.sex ~= "m" and not s.away
  s.product = a.sex == "r" and "rainbow_egg" or def.product
  s.rare = def.rare[a.colour] == true
  return s
end

-- A new animal in a pen.
local function add(sp, sex, colour, born)
  local b = world.state.barn
  local a = { id = b.next, sp = sp, sex = sex, colour = colour, born = born, fed = false, carrying = false }
  b.next = b.next + 1
  b.animals[#b.animals + 1] = a
  year.collect(sp .. "_" .. colour)
  return a
end

-- A minute awake: a mother due gives birth if her pen has room (else she
-- waits): a young one of a parent's colour (or now and then any of its
-- colours), a boy or a girl, twice the product's XP.
function barn.minute()
  for _, a in ipairs(world.state.barn.animals) do
    local def = B.animals[a.sp]
    if a.carrying and not a.out and awake() - a.carrying >= def.gestation and barn.room(a.sp) > 0 then
      local colour = math.random(1, 2) == 1 and a.colour or (a.father or a.colour)
      if math.random(1, 100) <= B.breeding.mutation then
        colour = def.colours[math.random(1, #def.colours)]
      end
      a.carrying, a.father = false, nil
      add(a.sp, math.random(1, 2) == 1 and "m" or "f", colour, awake())
      world.state.barn.bred[a.sp] = true
      year.count("born")
      bunny.gain(2 * B.products[def.product].xp)
      text.note("born", { "young", a.sp })
      level:sound("levelup", player)
    end
  end
end

-- Feeding a hungry animal: its wheat from the bag, then it makes its
-- product. Whether it could be.
function barn.feed(a)
  local s, inv = barn.status(a), player.state.inventory
  local cost = B.animals[a.sp].feed
  if not s.hungry then
    text.note(s.grown and "not_hungry" or "too_young")
    return false
  elseif (inv.wheat or 0) < cost then
    text.note("no_wheat", cost)
    return false
  end
  inv.wheat = inv.wheat > cost and inv.wheat - cost or nil
  a.fed = awake()
  bunny.work()
  mail.bump("feed")
  year.count("fed")
  return true
end

-- Taking an animal's product (the egg, the wool, the milk) into the bag;
-- it is hungry again. Whether it could be.
function barn.collect(a)
  local s = barn.status(a)
  if not s.ready then
    return false
  elseif not bunny.can_hold(s.product) or player:take(s.product) == 0 then
    text.note("bag_full")
    return false
  end
  a.fed = false
  bunny.work()
  bunny.gain(B.products[s.product].xp)
  year.collect(s.product)
  text.note("collected", { "products", s.product })
  return true
end

-- The act key on an animal: its product taken if ready, else fed if
-- hungry, else what it is doing said.
function barn.act(a)
  local s = barn.status(a)
  if s.away then
    text.note("away")
  elseif s.ready then
    return barn.collect(a)
  elseif s.hungry then
    return barn.feed(a)
  elseif s.scared and s.working then
    text.note("scared")
  elseif not s.grown then
    text.note("too_young")
  elseif s.carrying then
    text.note("carrying")
  elseif a.sex == "m" then
    text.note("male_no_feed")
  else
    text.note("making", { "products", s.product }, s.left // 60, s.left % 60)
  end
  return false
end

-- Why a female and a male cannot have young now, or nil when they can
-- (and what it costs).
function barn.why_not_breed(f, m)
  local s = barn.status(f)
  if f.out or (m and m.out) then
    return "away"
  elseif f.sex == "r" then
    return "rainbow_no_young"
  elseif f.sex ~= "f" then
    return "breed_female"
  elseif not s.grown then
    return "too_young"
  elseif s.carrying then
    return "carrying"
  elseif s.ready then
    return "collect_first"
  elseif not world.state.barn.stables[f.sp] then
    return "needs_stable"
  elseif not m or m.sp ~= f.sp or m.sex ~= "m" or not barn.status(m).grown then
    return "no_male"
  elseif (player.state.inventory.wheat or 0) < B.animals[f.sp].feed * B.breeding.feed then
    return "no_wheat"
  end
  return nil
end

-- A female given young by a male: wheat for both, her making stops while
-- she carries (a hen broods). Whether it could be.
function barn.breed(f, m)
  local why = barn.why_not_breed(f, m)
  local cost = B.animals[f.sp].feed * B.breeding.feed
  if why then
    text.note(why, cost, B.zones[f.sp].stable)
    return false
  end
  local inv = player.state.inventory
  inv.wheat = inv.wheat > cost and inv.wheat - cost or nil
  f.fed, f.carrying, f.father = false, awake(), m.colour
  bunny.work()
  text.note("bred", { "young", f.sp })
  local herd = level.state.herd
  if herd and herd[f.id] then
    fx.hearts(herd[f.id])                -- the prototype's hearts over her
  end
  return true
end

-- The first grown male of a pen (the father when none is chosen), or nil.
function barn.first_male(zone)
  for _, a in ipairs(barn.animals(zone)) do
    if a.sex == "m" and barn.status(a).grown then
      return a
    end
  end
  return nil
end

-- A pen's stable built: twice the room. Whether there were the coins.
function barn.build(zone)
  local price = B.zones[zone].stable
  if world.state.barn.stables[zone] then
    return false
  elseif player.state.coins < price then
    text.note("no_coins")
    return false
  end
  player.state.coins = player.state.coins - price
  world.state.barn.stables[zone] = true
  text.note("stable_built", barn.places(zone))
  return true
end

-- The barn's gift the first time the chicken yard is opened (from its
-- level): a grown white hen and some wheat. Whether it came now.
function barn.intro()
  local b = world.state.barn
  if b.intro or not barn.open("chicken") then
    return false
  end
  b.intro = true
  add("chicken", "f", "white", awake() - B.animals.chicken.grow)
  player:take("wheat", B.intro.wheat)
  text.note("barn_gift", B.intro.wheat)
  return true
end

-- ------------------------------------------------------------ merchants

-- The merchants who come to the farm in their hours from their level:
-- the cat (recipes), the dog (young animals), the fox (she buys grown
-- ones); each a thing of the level's at its place while it is here
-- (level.state.cat, .dog, .fox).
barn.MERCHANTS = { { "cat", D.CAT }, { "dog", D.DOG }, { "fox", D.FOX } }

-- The merchants there as the hour has them: one come, one gone (its
-- board shut if open).
function barn.merchants()
  for _, m in ipairs(barn.MERCHANTS) do
    local name = m[1]
    local here, thing = barn.visit(m[2]) ~= nil, level.state[name]
    if here and not thing then
      local at = level.places[name]
      level.state[name] = level:spawn(name, at.x, at.y, { facing = { x = 0, y = -1 } })
    elseif thing and not here then
      thing:remove()
      level.state[name] = false
      if level.state.menu and level.state.menu.kind == name then
        level.state.menu, level.state.held = false, false
      end
    end
  end
end

-- Which of a merchant's visits it is now (the day's first or second
-- window), or nil when it is away; `m` its data (D.DOG, D.FOX).
function barn.visit(m)
  if player.state.level < m.level then
    return nil
  end
  local hour = clock.minute(world.state.now) / 60
  for i, h in ipairs(m.hours) do
    if hour >= h[1] and hour < h[2] then
      return clock.day(world.state.now) * 2 + i
    end
  end
  return nil
end

local function counts(sp)
  local m, f = 0, 0
  for _, a in ipairs(barn.animals(sp)) do
    if barn.status(a).grown then
      if a.sex == "m" then
        m = m + 1
      elseif a.sex == "f" then
        f = f + 1
      end
    end
  end
  return m, f
end

-- The dog's stock this visit, rolled as he comes (the prototype's
-- rollDogStock): up to `most` young animals of the pens open.
local function roll(visit)
  local S, b = D.DOG, world.state.barn
  local open = {}
  for _, sp in ipairs(B.zone_order) do
    if barn.open(sp) then
      open[#open + 1] = sp
    end
  end
  local items = {}
  local n = math.random(0, S.most)
  local pity = #b.animals <= 1 and visit - b.last_buy > S.pity
  if (pity or math.random(1, 100) <= S.chicken) and n == 0 then
    n = 1
  end
  for i = 1, n do
    local sp
    if i == 1 and (pity or math.random(1, 100) <= S.chicken) then
      sp = "chicken"
    else
      local pool = {}
      for _, s in ipairs(open) do
        for _ = 1, S.weights[s] do
          pool[#pool + 1] = s
        end
      end
      sp = pool[math.random(1, #pool)] or "chicken"
    end
    local def = B.animals[sp]
    local sex = math.random(1, 2) == 1 and "m" or "f"
    local m, f = counts(sp)
    if m + f >= 2 and (m == 0 or f == 0) then
      sex = m == 0 and "m" or "f"            -- all of one sex: the other comes
    elseif b.bred[sp] and math.random(1, 100) <= B.breeding.minority then
      sex = m < f and "m" or (f < m and "f" or sex)
    end
    local rare = math.random(1, 100) <= S.rare
    local colours = {}
    for _, c in ipairs(def.colours) do
      if (def.rare[c] == true) == rare then
        colours[#colours + 1] = c
      end
    end
    local colour = colours[math.random(1, #colours)]
    if sp == "chicken" and math.random(1, 1000) <= B.rainbow * 10 then
      sex, colour = "r", "rainbow"
    end
    items[#items + 1] = { sp = sp, sex = sex, colour = colour, bought = false }
  end
  return { visit = visit, items = items }
end

-- The dog's stock while he is here, as rolled at his visit's first look
-- (barn.roll_stock), or nil: only read, so the HUD may ask.
function barn.stock()
  local visit = barn.visit(D.DOG)
  local stock = world.state.barn.stock
  return visit and stock and stock.visit == visit and stock or nil
end

-- The dog's stock rolled for this visit if it is not yet (in on_tick
-- only: it draws random numbers, which a picture may not), and returned.
function barn.roll_stock()
  local visit = barn.visit(D.DOG)
  if not visit then
    return nil
  end
  local b = world.state.barn
  if not b.stock or b.stock.visit ~= visit then
    b.stock = roll(visit)
  end
  return b.stock
end

-- Why one of the dog's animals cannot be bought, or nil.
function barn.why_not_buy(item)
  local def = B.animals[item.sp]
  if item.bought then
    return "sold_out"
  elseif not barn.open(item.sp) then
    return "pen_locked"
  elseif barn.room(item.sp) < 1 then
    return "pen_full"
  elseif player.state.coins < def.buy then
    return "no_coins"
  end
  return nil
end

-- Buying one of the dog's young animals into its pen.
function barn.buy(i)
  local stock = barn.stock()
  local item = stock and stock.items[i]
  if not item then
    return false
  end
  local why = barn.why_not_buy(item)
  if why then
    text.note(why, B.zones[item.sp].level)
    return false
  end
  player.state.coins = player.state.coins - B.animals[item.sp].buy
  item.bought = true
  world.state.barn.last_buy = stock.visit
  add(item.sp, item.sex, item.colour, awake())
  text.note("bought_animal", { "young", item.sp })
  return true
end

-- The fox's mood this visit, 0.9 to 1.2 (from the visit's number, as the
-- prototype's foxMood).
function barn.fox_mood(visit)
  local M = D.FOX.mood
  local h = (visit * 2654435761) % 4294967296 % 1000
  return M[1] + h / 999 * (M[2] - M[1])
end

-- What the fox pays for an animal now, or nil (she takes grown animals
-- not carrying young, a rare colour for half as much again).
function barn.price(a)
  local visit = barn.visit(D.FOX)
  local s = barn.status(a)
  if not visit or not s.grown or s.carrying or s.away then
    return nil
  end
  local def = B.animals[a.sp]
  return math.max(1, math.floor(def.sells * barn.fox_mood(visit) * (s.rare and B.rare_sells or 1) + 0.5))
end

-- The animals the fox would buy, all pens in order.
function barn.for_sale()
  local list = {}
  for _, a in ipairs(world.state.barn.animals) do
    if barn.price(a) then
      list[#list + 1] = a
    end
  end
  return list
end

-- Selling an animal to the fox.
function barn.sell(a)
  local price = a and barn.price(a)
  if not price then
    return false
  end
  local list = world.state.barn.animals
  for i, x in ipairs(list) do
    if x.id == a.id then
      table.remove(list, i)
      break
    end
  end
  player.state.coins = player.state.coins + price
  year.count("earned", price)
  text.note("sold_animal", price)
  return true
end

-- A pen's board's rows: its animals, then its stable to build (or a
-- word that it is empty).
function barn.pen_rows(zone)
  local rows = {}
  for _, a in ipairs(barn.animals(zone)) do
    rows[#rows + 1] = { animal = a }
  end
  if not world.state.barn.stables[zone] then
    rows[#rows + 1] = { build = B.zones[zone].stable }
  elseif #rows == 0 then
    rows[1] = { empty = true }
  end
  return rows
end

-- ------------------------------------------------------------ the pens

-- Where a pen's animals walk: its inside, its back kept clear of the
-- stable once it stands.
local function area(zone)
  local a = B.areas[zone]
  return a[1], a[2], a[3], world.state.barn.stables[zone] and a[4] - a.stable or a[4]
end

local function somewhere(zone)
  local x0, y0, x1, y1 = area(zone)
  return { x = math.random(x0, x1), y = math.random(y0, y1) }
end

-- The pens as the states say (on entering the level, and each tick after
-- an animal came or went): a thing an animal (level.state.herd, by the
-- animal's id), each somewhere in its pen; the stables built.
function barn.sync()
  local herd, alive = level.state.herd, {}
  for _, a in ipairs(world.state.barn.animals) do
    alive[a.id] = true
    if not herd[a.id] then
      local at = a.out or somewhere(a.sp)
      herd[a.id] = level:spawn(a.sp, at.x, at.y, { state = { animal = a.id, to = false, wait = math.random(0, 120) } })
    end
  end
  for id, t in pairs(herd) do
    if not alive[id] then
      t:remove()
      herd[id] = nil
    end
  end
  -- an egg lies in the straw where a hen laid it, until it is taken
  local eggs = level.state.eggs
  for id, t in pairs(herd) do
    local a = barn.animal(id)
    local laid = a and a.sp == "chicken" and barn.status(a).ready
    if laid and not eggs[id] then
      eggs[id] = level:spawn("egg", t.x - 5, t.y - 4, { state = { rainbow = a.sex == "r" } })
    end
  end
  for id, egg in pairs(eggs) do
    local a = barn.animal(id)
    if not (a and barn.status(a).ready) then
      egg:remove()
      eggs[id] = nil
    end
  end
  for _, z in ipairs(B.zone_order) do
    if world.state.barn.stables[z] and not level.state.stables[z] then
      local at = level.places[z .. "_stable"]
      level.state.stables[z] = level:spawn(B.zones[z].building, at.x, at.y, { facing = { x = 0, y = -1 } })
    end
  end
end

function barn.build_level()
  for _, kind in ipairs({ "chicken", "sheep", "cow", "egg", "coop", "shed", "barn" }) do
    for _, t in ipairs(level:things(kind)) do
      t:remove()
    end
  end
  level.state.herd, level.state.stables, level.state.eggs = {}, {}, {}
  barn.sync()
end

-- The animals walk about their pens: to a spot, a while there (pecking,
-- grazing), on to another; the young a little quicker. A hen brooding
-- sits still.
function barn.wander()
  for id, t in pairs(level.state.herd) do
    local a, s = barn.animal(id), t.state
    local st = a and barn.status(a)
    if not a then
      -- gone: barn.sync takes it away
    elseif a.out then
      -- run off: it waits out there, or follows the bunny (wolf.lua)
    elseif a.sp == "chicken" and st.carrying then
      s.to = false
    elseif s.wait > 0 then
      s.wait = s.wait - 1
    elseif s.to then
      local dx, dy = s.to.x - t.x, s.to.y - t.y
      local d = math.sqrt(dx * dx + dy * dy)
      if d < 2 then
        s.to, s.wait = false, math.random(90, 360)
      else
        t:walk({ x = dx / d, y = dy / d }, B.animals[a.sp].speed * (st.grown and 1 or 1.25))
      end
    elseif math.random(1, 100) <= 60 then
      s.to = somewhere(a.sp)
    else
      s.wait = math.random(60, 240)
    end
  end
end

return barn

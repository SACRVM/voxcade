-- BunnyFarm's numbers in one place, to tune in play (the prototype's
-- data.js, its timers carried into game time: docs/bunnyfarm.md). Times
-- are game minutes; a game minute is 42 ticks (0.7 seconds), a day runs
-- from 6:00 to 2:00 the next morning.
local D = {}

D.TICKS_A_MINUTE = 42
D.DAY_START = 6 * 60          -- minutes after midnight
D.DAY_END = 26 * 60           -- 2:00 the next morning: the bunny falls asleep
D.BEDTIME = 20 * 60           -- to bed from 20:00 only (the user: patience; each morning brings a gift and new goals)
D.MINUTES_A_DAY = 24 * 60
D.DAYS_A_WEEK = 7
D.DAYS_A_SEASON = 14
D.SEASONS = 4

-- A run starts meagre, as the prototype's: a few coins, one carrot, a few seeds.
D.START = { coins = 40, carrots = 1, seeds = { seed_carrot = 3, seed_wheat = 2 } }

-- The crops, the prototype's eleven: minutes awake to grow (a "day" of
-- the prototype's long ones is a played day, 16 hours), the seed's price
-- (in carrots for the pumpkin, fixed), what the harvest sells for (a
-- carrot is money of its own: the bunny's food, not sold), the XP a
-- harvest brings, the farm level that opens it.
local DAY = 16 * 60
D.CROPS = {
  carrot = { grow = 120, seed = 3, carrots = 1, xp = 6, level = 1 },
  wheat = { grow = 90, seed = 2, sells = 7, xp = 6, level = 1 },
  blueberry = { grow = 300, seed = 7, sells = 17, xp = 11, level = 2 },
  tomato = { grow = 480, seed = 12, sells = 24, xp = 16, level = 2 },
  radish = { grow = 180, seed = 5, sells = 14, xp = 8, level = 3 },
  cauliflower = { grow = 720, seed = 20, sells = 36, xp = 26, level = 3 },
  turnip = { grow = DAY, seed = 30, sells = 49, xp = 38, level = 4 },
  cabbage = { grow = DAY * 3 // 2, seed = 45, sells = 68, xp = 56, level = 5 },
  eggplant = { grow = DAY * 5 // 2, seed = 70, sells = 95, xp = 78, level = 6 },
  cucumber = { grow = DAY * 2, seed = 100, sells = 125, xp = 105, level = 7 },
  pumpkin = { grow = DAY * 4, seed_carrots = 3, sells = 145, xp = 150, level = 9 },
}
D.CROP_ORDER = { "carrot", "wheat", "blueberry", "tomato", "radish", "cauliflower", "turnip", "cabbage", "eggplant", "cucumber", "pumpkin" }

-- The watering can, bought once: watering a growing bed takes half the
-- time it has left off, at most `most` minutes, once a bed every `every`
-- minutes; the bed shows wet for `wet` minutes. It holds water for
-- `holds` beds, then it is filled at the pond (the user: a little work
-- to do, 2026-10-26; player.state.water, what is left).
D.CAN = { price = 120, most = 90, every = 480, wet = 240, holds = 4 }

-- The pond the can is filled at, in cells as the field: its corner, how
-- many across and deep.
D.POND = { x = 26, y = 8, across = 3, rows = 2 }

-- Fertilizer, bought by the bag: a bed fertilized grows its crops in
-- `speed` of the time (what grows on it already, the rest of its time)
-- and gives `yield` times the harvest, for `charges` harvests.
D.FERTILIZER = { price = 25, speed = 0.6, yield = 2, charges = 3 }
-- Harvest all at the scarecrow (the prototype's): the coins a ripe bed
-- costs, the ticks the bunny takes from bed to bed.
D.REAP = { fee = 1, step = 14 }

-- The field: beds in cells of the farm level (16 voxels each), the first
-- rows open, the others fallow until bought (`rows`: what the third and
-- the fourth cost), then tilled.
D.FIELD = { x = 30, y = 12, across = 6, rows = 4, open = 2, top = 16 }
D.ROWS = { [3] = 200, [4] = 800 }
D.CELL = 16

-- The bunny's needs, 0 to 100: an idle bunny gets bored within two or
-- three hours, a day of work wants a carrot or two. Energy comes back by
-- day only (7:00 to 18:00), and all of it in a night's sleep, after which
-- the bunny wakes hungry. A poor condition only slows its walk.
D.NEEDS = {
  start = { food = 80, energy = 90, mood = 70 },
  per_hour = { food = -2.5, mood = -15, energy = 3 },
  work = { food = -0.5, mood = 10, energy = -1 },
  eat = { food = 32, energy = 16, night = 0.4 },
  wake_hunger = 22,
  tired = 35, exhausted = 20, hungry = 25, bored = 30,
  pace = { normal = 1, tired = 0.62, exhausted = 0.42 },
}
D.WALK = 52                   -- voxels a second, rested

-- The market stall: buying a seed pushes its price up, selling a crop
-- pushes that one down, each by a share of its base, kept between low and
-- high times the base; every minute a share of the gap to the base closes
-- (most of it within a day). A carrot is bought at a fixed premium.
D.MARKET = { low = 0.45, high = 1.9, buy_push = 0.05, sell_push = 0.09, recover = 0.002, carrot = 8 }

-- The bag's slots, a kind of item a slot: 8 at first, more bought at the
-- stall (each `growth` times the one before), 24 at most.
D.BAG = { start = 8, most = 24, price = 60, growth = 1.6 }

-- What the next slot of the bag costs, with `slots` there now.
function D.slot_price(slots)
  return math.floor(D.BAG.price * math.pow(D.BAG.growth, slots - D.BAG.start) + 0.5)
end

-- The kitchen (open from `level`): pots cooking side by side, one at
-- first, more bought (each `growth` times the one before) up to `most`;
-- recipes owned (bread from the start, the others from the cat); a jarred
-- recipe needs an empty jar (`jar` coins at the stall). Cooking counts
-- the minutes awake, as growing does. The prototype's recipes, their
-- times from the concept; a dish sells for its fixed price.
D.KITCHEN = { level = 2, pots = 1, most = 3, pot_price = 180, pot_growth = 2.2, jar = 6 }
D.RECIPES = {
  bread = { needs = { wheat = 4 }, cook = 120, sells = 42, xp = 14, level = 1, price = 0 },
  jam = { needs = { blueberry = 3 }, jar = true, cook = 240, sells = 95, xp = 22, level = 2, price = 120 },
  sauce = { needs = { tomato = 3 }, jar = true, cook = 480, sells = 150, xp = 30, level = 3, price = 250 },
  soup = { needs = { cabbage = 2, turnip = 1, cauliflower = 1 }, jar = true, cook = 720, sells = 620, xp = 55, level = 5, price = 900 },
  spread = { needs = { eggplant = 3 }, jar = true, cook = DAY, sells = 950, xp = 80, level = 6, price = 1500 },
  pickles = { needs = { cucumber = 3 }, jar = true, cook = DAY + 120, sells = 1300, xp = 110, level = 7, price = 2200 },
}
D.RECIPE_ORDER = { "bread", "jam", "sauce", "soup", "spread", "pickles" }

-- What the next pot costs, with `pots` there now.
function D.pot_price(pots)
  return math.floor(D.KITCHEN.pot_price * math.pow(D.KITCHEN.pot_growth, pots - D.KITCHEN.pots) + 0.5)
end

-- The cat who sells recipes: from `level`, east of the field in these
-- hours of the clock every day.
D.CAT = { level = 2, hours = { { 10, 12 }, { 16, 18 } } }

-- The mailbox's orders: `slots` of them, a slot freed (delivered or
-- turned down) filled again `refill` minutes awake later. An order asks
-- for one kind of goods (two from level 3, three from level 6), crops
-- 3 to 8 of each, dishes 1 to 3; it pays what they would sell for at
-- their base times `reward`, a quarter more for each kind past the first,
-- and XP `xp` a coin (the prototype's). Specials (the full moon's
-- berries), 2 to 4, come into them from level `specials`, rarely; the
-- barn's products (2 to 5) once their pen is open.
D.ORDERS = { slots = 3, refill = 90, reward = 2.2, combo = 0.25, xp = 0.06, crops = { 3, 8 }, dishes = { 1, 3 },
             specials = 4, special = { 2, 4 }, products = { 2, 5 } }

-- The day's goals, `count` of them rolled each morning from these kinds
-- (a kind only once the bunny can do it), each paying more the more it
-- asks; all of them done, a bonus besides.
D.GOALS = {
  count = 3,
  bonus = { coins = 100, xp = 100 },
  kinds = {
    harvest = { count = { 6, 14 }, coins = { 30, 60 }, xp = { 20, 40 } },
    water = { count = { 4, 10 }, coins = { 30, 55 }, xp = { 20, 35 }, needs = "can" },
    cook = { count = { 1, 3 }, coins = { 40, 80 }, xp = { 25, 60 }, needs = "kitchen" },
    order = { count = { 1, 3 }, coins = { 45, 80 }, xp = { 30, 60 } },
    feed = { count = { 2, 6 }, coins = { 35, 70 }, xp = { 25, 50 }, needs = "barn" },
    sell = { count = { 5, 15 }, coins = { 30, 60 }, xp = { 20, 40 } },
  },
  order = { "harvest", "water", "cook", "order", "feed", "sell" },
}

-- A gift in the mailbox every morning (the prototype's daily reward came
-- once a real day; a game day is 14 minutes of play, so it is smaller).
D.GIFT = { coins = 20, carrots = 1 }

-- The week: an event a day, by the weekday from Monday (the prototype's
-- world events, on the game's calendar). Every second week the seed
-- sale takes Tuesday's place (in the prototype Sunday's cake day kept it
-- from ever coming). An event is a set of changes where they matter:
-- `crops` ("all" or a list) whose prices change, `sells` and `seeds`
-- times their price (`fixed`: the base's, not the market's of the
-- moment), `xp` times a harvest's XP, `mood` times the boredom and
-- `rest` times the energy coming back, `growth` minutes of growing a
-- minute awake (the rain's), `wet` beds without watering, `moon` a crop
-- harvested by night that turns into a special. `icon` its picture,
-- `weather` what the picture shows of it (rain falling, the full moon's
-- light by night).
D.EVENTS = {
  week = { "harvest_fest", "lazy_day", "full_moon", "rain", "blueberry_day", "market_day", "cake_sunday" },
  sale = { weekday = 2, event = "seed_sale" },
  harvest_fest = { icon = "pumpkin", crops = "all", sells = 1.15, xp = 1.5 },
  lazy_day = { icon = "lazy", mood = 0.5, rest = 1.3 },
  full_moon = { icon = "full_moon", weather = "moon", moon = { crop = "blueberry", gives = "moonberry" } },
  rain = { icon = "rain", weather = "rain", growth = 1.25, wet = true },
  blueberry_day = { icon = "blueberry", crops = { "blueberry" }, sells = 1.5, fixed = true, seeds = 1.4 },
  market_day = { icon = "coin", crops = "all", sells = 1.2 },
  cake_sunday = { icon = "cake", crops = { "wheat" }, sells = 1.4, fixed = true },
  seed_sale = { icon = "sprout", crops = "all", seeds = 0.6 },
}

-- The barn (the prototype's, its timers in minutes awake as the crops'
-- are): three pens behind the burrow, each open from its level with a
-- few places for animals, twice as many once its stable stands. Animals
-- come from the dog, go to the fox, are born of a pair in a pen with a
-- stable. A grown female, fed `feed` wheat, makes her `product` in
-- `cycle` minutes and keeps it until it is taken (an egg in the straw,
-- a full fleece, a full udder), hungry again then; males make nothing
-- and eat nothing. The young grow up in `grow`, a mother carries one for
-- `gestation` (a hen broods) and gives birth when the pen has room; a
-- birth brings twice the product's XP. Colours: the rare ones sell for
-- `rare_sells` times as much at the fox. Nothing dies, nothing starves:
-- an animal not fed only waits.
D.BARN = {
  level = 3,
  zones = {
    chicken = { level = 3, places = 3, most = 6, stable = 750, building = "coop" },
    sheep = { level = 5, places = 2, most = 4, stable = 1500, building = "shed" },
    cow = { level = 7, places = 1, most = 3, stable = 2500, building = "barn" },
  },
  zone_order = { "chicken", "sheep", "cow" },
  -- where each pen's animals walk (its inside, in voxels: x, y to x, y;
  -- the level's), and how far from its back a stable keeps them
  areas = {
    chicken = { 374, 390, 458, 442, stable = 18 },
    sheep = { 488, 392, 584, 440, stable = 22 },
    cow = { 618, 394, 710, 438, stable = 28 },
  },
  animals = {
    chicken = { buy = 60, feed = 2, product = "egg", cycle = 180, grow = DAY * 3 // 2, gestation = DAY * 5 // 2, sells = 65,
                colours = { "white", "brown", "red", "blue", "green" }, rare = { blue = true, green = true }, speed = 14 },
    sheep = { buy = 250, feed = 4, product = "wool", cycle = DAY, grow = DAY * 3, gestation = DAY * 4, sells = 280,
              colours = { "white", "brown", "black" }, rare = { black = true }, speed = 10 },
    cow = { buy = 450, feed = 5, product = "milk", cycle = 600, grow = DAY * 7 // 2, gestation = DAY * 5, sells = 500,
            colours = { "light", "brown", "pink", "purple", "green" }, rare = { pink = true, purple = true, green = true }, speed = 9 },
  },
  products = { egg = { sells = 24, xp = 10 }, rainbow_egg = { sells = 48, xp = 14, unordered = true },
               wool = { sells = 130, xp = 55 }, milk = { sells = 85, xp = 40 } },
  product_order = { "egg", "rainbow_egg", "wool", "milk" },
  -- breeding: wheat `feed` times the species' feeding; a young one takes
  -- a parent's colour, or with `mutation` % chance any of its colours
  breeding = { feed = 2, mutation = 10, minority = 65 },
  rare_sells = 1.5,
  rainbow = 1.5,          -- % a chicken of the dog's is the rainbow hen (no sex, no young, rainbow eggs)
  intro = { wheat = 6 },  -- the first time the chicken yard is opened: a grown white hen and wheat
}

-- The pens' back fence and the wolf (wolf.lua): a night may break a gap
-- into the back fence of a pen with animals (`gap` % a pen, a cell of
-- `cells`, the level's cells across in its back row `row`; none behind
-- where a stable goes up). The wolf is shy: he comes only while the
-- bunny sleeps, at `hour` o'clock, to a pen whose gap is still open
-- (`comes` %); its animals are frightened for `fright` minutes of the
-- clock (they give nothing) and one runs off into the meadow behind,
-- where the bunny fetches it: out through the farm's gate, round the
-- fence, and it follows the bunny home to its pen's gate. Wood from the
-- stall (`wood` coins) mends a gap.
D.WOLF = { gap = 25, comes = 100, hour = 3, fright = 24 * 60, wood = 20, row = 28,
           cells = { chicken = { 23, 24, 25 }, sheep = { 30, 31, 35, 36 }, cow = { 38, 40, 41 } } }

-- The dog sells animals (young ones), the fox buys them (grown ones, not
-- carrying young), each on the farm in these hours of the clock from the
-- barn's level, the cat between them. The dog's stock is rolled as he
-- comes: up to `most` animals, the first a chicken `chicken` % of the
-- time, the others by `weights` among the pens open; a rare colour
-- `rare` % of the time; a farm with hardly any animal that has not bought
-- for `pity` visits gets a chicken. The fox pays the grown animal's price
-- times her mood that visit (`mood`, from her visit's number).
D.DOG = { level = 3, hours = { { 8, 10 }, { 14, 16 } }, most = 3, chicken = 75, weights = { chicken = 6, sheep = 3, cow = 1 },
          rare = 10, pity = 4 }
D.FOX = { level = 3, hours = { { 12, 14 }, { 18, 20 } }, mood = { 0.9, 1.2 } }

-- Specials: rare kinds of a crop that only come under some condition
-- (the full moon's berries), sold at the stall for a fixed price and
-- asked for in orders from `D.ORDERS.specials` on.
D.SPECIALS = { moonberry = { of = "blueberry", sells = 70 } }
D.SPECIAL_ORDER = { "moonberry" }

-- What a dish, a special or an animal's product sells for (they keep
-- their price).
function D.fixed_price(item)
  local thing = D.RECIPES[item] or D.SPECIALS[item] or D.BARN.products[item]
  return thing and thing.sells
end

-- XP to go on from a level: 30 times the level to the power of 1.9.
function D.xp_for(level)
  return math.floor(30 * math.pow(level, 1.9) + 0.5)
end

return D

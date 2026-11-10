# BunnyFarm in Voxcade: the concept

A cozy farming game with a tamagotchi at its heart, carried over from the
browser prototype "My Farm" (`bunnybot-web-io/public/farm/`, its code the
source of truth, not its docs) into Voxcade. This document is to be agreed
with the user before anything is built; it says what the game is, what
changes from the prototype and why, what the engine must learn first, and
in which stages it is built.

## Decided (the user, 2026-10-23)

- **A Lua game in the Voxcade repo**, `games/bunnyfarm`, on the engine as
  every Lua game is; it may move to a repo of its own once it is a
  product. Whatever it needs from the engine is built as API for every
  Lua game, never for the farm alone.
- **Standalone.** Releasing it with the Discord bot is off: no shared
  wallet, no global market, no online leaderboard, no login.
- **Game days, not the wall clock.** The world moves while the game is
  played and stands still while it is closed (Harvest Moon, Stardew).
- **A farm year is a run** with a reckoning at its end, and the farm goes
  on after it: it need not stop at one year.
- **The bunny is steered directly, keyboard and gamepad only, no mouse.**
- **Figures are our own**, small and plain like Voxkeep's (the 8-bit charm,
  one colour a material, clean voxel forms). Sprout Lands' licence forbids
  "AI generation involving these assets": its sprites are never converted
  or used as input; they are only the prototype's look.
- **Later:** delivery trips (the prototype's planned "dungeon": the bunny
  brings carrots to its family) and arcade interludes (driving wolves off,
  dodging obstacles on the road): what was hard in a browser is Voxcade's
  strength.

## The game in one paragraph

You are the bunny. You till, sow, water and harvest a small field, sell at
the market stall, cook what you grow into things worth more, fill orders
from the mailbox, and later keep chickens, sheep and cows. Doing nothing
makes the bunny bored, working makes it happy but tired and a little
hungry; carrots feed it, sleep in the burrow restores it. A poor condition
never costs money or lives, the bunny only walks slower (the prototype's
rule: forgiveness, not punishment). Nothing dies or withers.

## What changes from the prototype

| Prototype (browser, bot-ready) | BunnyFarm in Voxcade | Why |
|---|---|---|
| Timers against the wall clock, crops finish while the tab is closed | Timers in game hours and days; sleeping skips the night and everything goes on through it | a desktop game is started on purpose; determinism, replays and saves stay as they are; no clock in the engine |
| Clicking a plot, the bunny walks there and acts | Arrows or stick walk the bunny; an action key acts on what is in front of it | keyboard and pad only; the condition's one effect (walking slower) is felt directly |
| Five tabs (Farm, Barn, Burrow, Market, Inventory) and modal dialogs | Places on one farm (the field, the burrow, the market stall, the mailbox, the kitchen, the barn's pens, the road where merchants come) and menus opened there | a voxel diorama to walk around instead of screens |
| Global market moved by all players, owned by the bot | A local market with the same supply and demand; the village (later) moves prices too | standalone |
| Shared wallet, leaderboard, Discord login | none; the arcade high score table scores farm years | standalone; Voxcade's own table |
| Daily bonus with a streak (a retention lever for real days) | A morning gift in the mailbox | there are no real days to come back on |
| Daily event by UTC date (in the code a fixed weekly schedule) | The same schedule on the game's weekdays | a calendar of the game's own |
| Merchants on wall-clock windows (cat every 6 h, dog every 5 h, fox every 4 h) | Merchants on the game's calendar and clock (days and hours they come) | as above |
| Goals per UTC day | Goals per game day | as above |
| Day/night: a 2-hour real cycle, a tint over the page | The game's day with real light: the sun's direction, colour and strength, the sky, lamps at night | light is what voxels are made for |
| `Math.random` unseeded | `math.random` (the game's seeded generator) | replays and saves get it for free |
| localStorage, saved on every action | Saved when the bunny goes to sleep (Stardew's rhythm), CONTINUE on the title | the engine's save games |
| Emoji in the text (🪙 alone 51 times) | Voxel icons (`c.icons`) | the engine's text is voxels |

Fixed on the way, found in the prototype's code: breeding cannot be
started (a layer swallows the click), resetting breaks the state until a
reload, the seed sale never comes (Sunday's cake day overrides its
weekday), "not enough coins" says "not enough for the harvest fee"
everywhere, the hungry note speaks of income it no longer costs.

## Time

- **A day runs from 6:00 to 2:00** (20 game hours). A game minute is 0.7
  seconds (42 ticks): a day is 14 minutes of play, Stardew's measure.
- **Phases:** dawn 6-7, day 7-18, dusk 18-20, night 20-2 (the prototype's
  fractions laid on the clock). Light follows: low warm sun at dawn, white
  at noon, orange at dusk, a dark blue night with lamps and the moon.
- **Sleeping** in the burrow, from 20:00 (the user, 2026-10-24: patience;
  every morning brings a gift and new goals, so sleeping early would
  print them; `D.BEDTIME`, the burrow says from when): the clock goes to
  6:00 of the next day; the hours slept count for nothing that grows or recovers (crops,
  prices; later pots and animals): they run on the minutes the bunny is
  awake (`world.state.awake`), at night too. The user (2026-10-24): else
  everyone sows the dearest and sleeps till it is ripe. Energy full, the
  bunny wakes hungry (food -22), as in the prototype. The game is saved.
- **At 2:00** the bunny falls asleep wherever it stands and wakes in the
  burrow, at no cost (the user's choice: forgiveness).
- **A week of 7 days** with the prototype's schedule: Monday harvest
  festival, Tuesday lazy day, Wednesday full moon, Thursday rain, Friday
  blueberry day, Saturday market day, Sunday cake Sunday; the seed sale
  takes the lazy day's place every second week (stage 4).
- **Seasons and the year:** 4 seasons of 14 days, a year of 56 days (about
  13 hours of play; the user's choice), the clock naming the season and
  its day. Whether seasons change what grows is open (not in the
  prototype; later at the earliest).

### The timers in game time

The prototype's bands keep their order and their meaning: fast crops are
food for the same session (a few hours), medium ones come back later that
day or the next morning, long ones are the anchors that grow over the
nights. Raw profit per hour still falls up the tiers; wealth comes from
cooking and orders, as in the prototype.

| Crop | Prototype | Game time | Seed | Sells | Level |
|---|---|---|---|---|---|
| Wheat | 3 min | 1.5 h | 2 | 7 | 1 |
| Carrot | 4 min | 2 h | 3 | (carrots) | 1 |
| Radish | 8 min | 3 h | 5 | 14 | 3 |
| Blueberry | 18 min | 5 h | 7 | 17 | 2 |
| Tomato | 40 min | 8 h | 12 | 24 | 2 |
| Cauliflower | 90 min | 12 h | 20 | 36 | 3 |
| Turnip | 3 h | 1 day | 30 | 49 | 4 |
| Cabbage | 5 h | 1.5 days | 45 | 68 | 5 |
| Cucumber | 7 h | 2 days | 100 | 125 | 7 |
| Eggplant | 8 h | 2.5 days | 70 | 95 | 6 |
| Pumpkin | 16 h | 4 days | 3 carrots | 145 | 9 |

Cooking likewise (bread 20 min to 2 h, jam 45 min to 4 h, sauce 90 min to
8 h, soup 150 min to 12 h, spread and pickles 4 to 4.5 h to a day); the
animals' cycles, growing and gestation when the barn comes (stage 5).
Watering skips half the remaining time, at most 1.5 game hours, once per
plot every 8 game hours. The market's prices recover most of the way to
their base within a day. The bunny's needs drain and recover per game
hour, tuned so an idle bunny gets bored within two or three game hours and
a day of work wants one or two carrots. Every one of these numbers lives
in the game's data table, as in the prototype's `data.js`: tuned in play.

## The farm

- **Seen from above at a slant** (More Blood's camera, a little steeper:
  about 50°), following the bunny over a farm a little bigger than the
  screen: at least 320 by 200 voxels where the bunny stands, more across
  or more high as the screen's shape gives (`c.camera.view_at_least`;
  the user: the games of old had at least 320 by 200 pixels, at 160
  across everything looked far too big), so the bunny is to the picture
  what a sprite was then. The meadow round the farm is wide enough for a
  21:9 screen.
- **A level of voxel blocks** (`levels/`, shaped in the level editor by
  the user): grass, paths, the tilled field, fences, water. 16 voxels a
  cell; the field a grid of 6 by 4 beds, two rows open at the start, the
  others behind a fence until bought (200 and 800 coins).
- **Places:** the field; the burrow (sleep, eat, the bunny's condition);
  the market stall (buy and sell); the mailbox (orders, the morning gift);
  the kitchen (stage 3); the barn's three pens (stage 5); the road along
  the edge where the cat, the dog and the fox come.
- **The bunny** a farmer (the user: a farmer bunny, not a plain one),
  small and plain like Voxkeep's hero: on its hind legs, a round head
  with big eyes and a pink nose, a straw hat with a red band and the
  ears out through its crown, a red shirt, blue dungarees, brown boots;
  20 voxels to the ears' tips, walking in four frames, working in a
  fifth. Crops 4 to 12 voxels in their four stages, each a model with
  frames.
- **The look, second pass** (2026-10-24, the user: the first looked
  careless): every model of clean forms (`tools/figures.py`: boxes with
  their edges cut back, nothing a voxel thin that need not be), calm
  colours a material. Beds with a strip of grass round each, two ridges
  where the plants stand; paths, the pond and the fence as pieces that
  join up (a straight piece, a bend, a junction, an end, turned as the
  neighbours ask); flowers and tufts as decor blocks, walked through;
  trees, firs, bushes, rocks, hay, barrels, a crate and a scarecrow as
  things placed in the level (the level editor moves them), their
  footprints walked round (`props.solid`). The burrow a grassy hill with
  a stone front, a round door between two lit round windows, a chimney,
  a lantern; the stall with crates of produce, a striped awning and a
  sign.
- **The HUD** of one family: cream paper with dark brown writing on
  wooden boards, keys as key caps, pictures of 7 by 7. The stall's menu
  a board under its own awning (tabs, a row a good with its picture,
  what the bunny has, the price in coins), the burrow's and the bag's
  boards alike.
- **A meadow round the farm**, wide enough that the camera, looking north
  at a slant and seeing further at the top of the picture, shows land to
  the picture's edges wherever the bunny goes; trees round it.
- **The farm's gate and the road out** (2026-10-26, the user: the bunny
  can always go out through the gate, for the trips later and to fetch
  an animal that ran off): an open gate in the middle of the front
  fence, a road from the path along the field down to the meadow's
  edge, a barrier across it there (nobody goes on); a wood of two rows
  round the meadow, a clearing for the road; the view stops at the
  meadow's edges (`c.camera.stop_at_edges`). Put on the level by
  `tools/gen_bunnyfarm.py --gate` (only those cells, things and places).
- **The fence and the wolf** (2026-10-26, the user: once there are
  animals the fence wants mending, else wolves come at night; the
  peaceful way, an animal runs off and is fetched back; the wolf is shy,
  he never comes while the bunny is up; `wolf.lua`, `D.WOLF`). A night
  may break a gap into the back fence of a pen with animals (a quarter
  of the time a pen; its top rail falls into the pen, the lower rail
  stays: nobody passes). Wood from the stall (20) mends it: the act key
  at the gap, inside the pen or behind it, and three blows of a hammer,
  each knocking a third of the fence back (the bunny's frames 6 to 8,
  the arm raised with the hammer, on its way down, the blow). A gap still
  open when the bunny sleeps brings the wolf: that pen's animals are
  frightened for 24 hours of the clock (they give nothing: no egg in the
  straw, no wool, no milk; feeding goes on) and one runs off into the
  meadow behind the fence, a question mark over it. The bunny fetches it
  out through the farm's gate and round the fence; by it the act key, and
  it follows the bunny's trail at its pace, in through its pen's gate
  (where the bunny stands at it, or by itself). Nothing dies: an animal
  still led as the bunny falls asleep finds its own way home. The
  morning says what the night did (the wolf, a new gap).
- **The bunny's tools** (2026-10-26, the user: with a hammer, a can and a
  spade too): frames of its own figure, held at its right so they show
  from behind as well (the beds lie north of it, it mostly turns its back
  to the camera): the can held out by its handle, tipped, water falling
  from its rose, while it waters; a spade in the bed, then lifted with a
  clod, as it sows; the hammer raised, on its way down, the blow, as it
  mends a fence. Only the picture: `main.lua` reads which from the bed
  in front (watered this minute, sown this minute), no state of its own.

## Controls (keyboard and pad)

| Action | Keyboard | Pad | Does |
|---|---|---|---|
| walk | arrows | stick, d-pad | 8 ways; the bunny faces the way it walks |
| act | Space, Z | south | on the bed in front: sow the chosen seed, water, harvest (the prototype's click rules); at a place: open it (the stall's menu, the burrow, the mailbox) |
| seed | Q / E | shoulders | the chosen seed, one way or the other (only seeds owned) |
| bag | Tab, I | north | the inventory and the bunny's condition |
| back | X, Backspace | east | out of a menu |
| pause | Escape, P | start | the engine's pause (settings, save, load, quit) |

Menus hold the level while open (`level.state.held`): time stands still in
them, as in Stardew.

## The basics (stage 1): a day on the farm (2026-10-23, built)

Playable on its own, everything else builds on it:

- The farm level with the field, the burrow and the market stall.
- The bunny steered as above; the act key on beds and places.
- Four crops (wheat, carrot, blueberry, tomato) in four stages each.
- The day: the clock, its light, sleeping, falling asleep at 2:00, the
  bunny's needs (eat a carrot, sleep) and its pace.
- The market stall: buy seeds, sell the harvest, prices moving with
  supply and demand and recovering; carrots bought at their premium.
- The inventory of 8 slots (more bought later), coins, carrots, XP and
  levels with what each level opens.
- The HUD: coins, carrots, level and XP, the day, weekday and clock, the
  three needs, the chosen seed with its count and beside it the bag (its key
  and how many of its places are taken: the user, nobody knew there was a
  bag).
- Saving at sleep, CONTINUE and NEW FARM on the title.
- Sounds (the synth's: sow, water, harvest, coin, level up, eat, sleep,
  error) and calm music from the library by day, another by night.
- A test replay and a demo run for the title, like every example game.

## What the engine must learn first

Each as API for every Lua game, designed by the game model
(`docs/game-model.md`, the voxcade-api skill), with tests, HELP examples
and docs; each a commit of its own:

1. **The game's own Lua files.** `require` is gone from the sandbox, and a
   game this size does not fit one `main.lua` (Pulsar's is the biggest
   today, 792 lines). A game's modules from its folder, loaded once,
   deterministic, part of the fingerprint as tokens, reloaded with the
   rest.
2. **Text in more letters.** The pixel font has capitals, digits and a few
   signs only. Lower case, the letters of German and French (ä ö ü ß à â ç
   è é ê î û œ and their capitals) and the few signs the game writes (· ×
   → ▲ ▼ ✓), in the pixel font and in the big letters.
3. **Light the game sets.** Today the sun's direction, the ambient light,
   the shadow and the sky are `conf.lua` settings. A game must set them as
   it runs, with the sun's colour and strength besides (a picture only:
   the simulation does not see it).
4. **Menus for the game.** Lists, tabs and grids moved through with the
   keys and the pad, in the engine's look (the market, the inventory, later
   the kitchen and the orders), for RPGs and adventures too. How a game
   declares and answers them is to be designed.
5. **CONTINUE on the title.** The title offers START, SETTINGS and QUIT;
   saves load only from the pause menu. A game whose world goes on wants
   CONTINUE (the last save) and a new start that asks before it overwrites.
6. **The player's language:** the game speaks German, English and French
   (the user's choice, the prototype's 323 strings in each), so a setting
   for it (the system's language at first) and a way for the game to read
   it; the engine's own menus in the same languages.

## Stages after the basics

2. **The whole field** (2026-10-24, built): all eleven crops (the
   prototype's prices, XP and levels; its long timers in waking hours, a
   "day" a played day of 16 hours; the pumpkin's seed for 3 carrots),
   buying the fallow rows (the act key on one opens a board: the third
   for 200, the fourth for 800; bought, its beds are tilled), the
   watering can (120 once: half the time left off, at most 1.5 hours,
   once a bed every 8 hours; the bed shows wet a while; it holds water
   for four beds and is filled at the pond, the hand showing its water
   as a gauge: the user's wish, 2026-10-26, a little work to do),
   fertilizer (25
   a bag: 60 % of the time, double harvests, for three harvests; lilac
   granules in the bed's earth, three a harvest left, and standing at
   it the hint says how many; wet earth and granules are coloured into
   the ridges themselves, as a texture), bag
   slots bought at the stall (60, then each 1.6 times, up to 24: eleven
   crops do not fit in 8), level-ups with the crops they open. What the
   act key works with on the field is the hand: Q and E step through
   the seeds owned, the can and fertilizer; a ripe bed is harvested
   whatever is in hand. The stall's board scrolls (six rows shown).
   Harvest all (the prototype's, carried over 2026-10-26): by the
   scarecrow the act key sends the bunny hopping from ripe bed to ripe
   bed, harvesting each as by hand for a coin a bed (`D.REAP`), until
   all are done, the bag is full or the coins run out; the back key
   stops it. The prototype's small signs came along too (`fx.lua`): what
   a bed gives rising from it (the harvest's picture and count, a drop
   of water, the fertilizer), coins and goods flying between the stall's
   rows and the purse or the bag, the morning's gift flying from the
   mailbox, hearts rising over a mother as she pairs; drawn where the
   level's points show (`frame:to_screen`), only for the eye.
   Each bed keeps its time left, counted down by the minutes awake.
3. **Kitchen and mailbox** (2026-10-24, built), the prototype's rules
   in game time:
   - **The kitchen** beside the burrow, open from level 2: a stone oven
     and a counter of pots, without a roof (from above it hid the pots).
     One pot at first, more for 180 and then each 2.2 times, up to three.
     Bread is known from the start; the other five recipes (jam, sauce,
     soup, spread, pickles) take an empty jar (6 at the stall). The pots
     cook on the minutes awake, as the crops grow, and steam while they
     do. A dish waits in its pot until the act key takes it out, and
     sells at the stall for its fixed price.
   - **The cat** (upright and dressed as the bunny is, in a teal shirt:
     the merchants are people of the farm world, the user, not pets)
     sells the recipes. From level 2 she sits east of the
     field from 10 to 12 and from 16 to 18; her board offers what the
     bunny's level opens.
   - **The mailbox** holds three orders. Each asks for goods the bunny
     can grow or cook by now (more kinds from level 3 and 6) and pays
     2.2 times what they would sell for, a quarter more for each kind
     past the first. An order delivered or turned down (TAB) fills its
     slot again 90 minutes awake later. Its flag is up while something
     waits.
   - **The day's goals**, three a morning from harvest, water, cook,
     deliver and sell (each only once the bunny can do it), are paid as
     each is done, with 100 coins besides when all are.
   - **The morning gift** is 20 coins and a carrot, smaller than the
     prototype's daily reward (a game day is 14 minutes of play).
   - The test replay `the_mailbox_pays_for_an_order` takes the gift,
     grows four wheat on fertilizer, delivers them for 62 coins (the
     stall would pay 28), turns an order down and waits for both slots
     to fill again.
4. **The calendar** (2026-10-24, built; `calendar.lua`), the prototype's
   events and their numbers (its `data.js` and `core.js`), one a day by
   the weekday, nothing of it kept in the states (the day says it all):
   - **Monday, harvest festival:** harvests bring half as much XP again,
     crops sell 15 % higher.
   - **Tuesday, lazy day:** boredom comes at half the speed, energy back
     by day 1.3 times as fast. Every second week the **seed sale** takes
     its place: every seed 40 % cheaper (the pumpkin's carrots stay).
   - **Wednesday, full moon:** blueberries harvested by night (20:00 to
     2:00) are moonberries, silver, sold at the stall for a fixed 70;
     orders ask for 2 to 4 of them from level 4, rarely. The night is
     lit silver, with shadows; the clock shows the full moon; ripe
     blueberries shimmer that night (their berries glint in turn, a
     faint silver light about them: the user's choice of hint).
   - **Thursday, rain:** crops grow 1.25 minutes a minute awake (the
     prototype's 0.8 of the time) and show wet; watering still helps.
     Drops fall over the picture, the light is grey and the shadows
     soft, the clock shows a cloud.
   - **Friday, blueberry day:** blueberries sell for 1.5 times their
     base, whatever the market did; their seeds cost 1.4 times as much.
   - **Saturday, market day:** every crop sells 20 % higher.
   - **Sunday, cake Sunday:** wheat sells for 1.4 times its base.
   - The day's event shows under the clock (its picture and name), and
     the morning says it with what it does (the first morning too). At
     the stall a price the day moves has a sign by it: green when it
     is good for the bunny, red when not.
   - Test replays (both from a save, as the bunny may not sleep through
     the days to get there): `rain_grows_the_crops_faster` begins on
     Thursday morning and harvests wheat after 73 minutes (90 on a dry
     day); `the_full_moon_gives_moonberries` begins on Wednesday morning
     at level 2, sows a blueberry, harvests it after 20:00 as a
     moonberry and sells it for 70. `to_bed_from_eight` begins at 19:50:
     the burrow says it is too early, at 20:00 the bunny sleeps.
5. **The barn** (2026-10-24, built; `barn.lua`), the prototype's rules
   (its `core.js`) with its timers in minutes awake, as the crops':
   - **The farm grew six rows to the north:** behind the burrow and the
     kitchen three pens side by side, each fenced with a gate and its
     sign, a path along their fronts with a branch from the main path.
     The chicken yard is sand, the sheep's meadow has hay, the cows'
     pasture a water trough.
   - **The pens open with levels**, the chicken yard at 3, the sheep at
     5, the cows at 7. Each holds 3, 2 and 1 animals, twice as many once
     its stable stands (750, 1500, 2500): a coop on posts, a shed, a red
     barn. The first time the chicken yard is opened a grown white hen
     and 6 wheat are the barn's gift.
   - **The animals walk about their pens**, each in its colour (the
     prototype's, the rare ones bright: blue and green hens, black
     sheep, pink, purple and green cows, slim and long after a first
     try too wide and square; the rainbow hen, 1.5 % of the
     dog's chickens, lays rainbow eggs and has no young).
   - **Tending:** a grown female fed with wheat (2, 4, 5) makes her
     product: an egg in 3 hours, wool in a day, milk in 10 hours. It
     shows: the egg lies in the straw where she laid it, the fleece
     grows back from shorn over half to full, the udder fills. Taken,
     it brings XP, and she is hungry again. Males eat and make nothing.
     Nothing starves: an animal not fed only waits.
   - **Young:** with the stable built, a grown female and a male (the
     bag key on him chooses him, else the pen's first) have young for
     twice the feeding's wheat. She carries it 2.5 days (a hen broods on
     her nest), 4 for a ewe, 5 for a cow, and gives birth when the pen
     has room: a parent's colour, now and then (10 %) any colour, twice
     the product's XP. The young grow up in 1.5, 3 and 3.5 days.
   - **The dog** (a dealer in a flat cap and a neckerchief; 8 to 10 and
     14 to 16, from level 3) sells young
     animals: up to three a visit, mostly chickens, a rare colour one
     time in ten, a chicken for sure when the farm has hardly any
     animal. **The fox** (a gentleman in a waistcoat and a little top
     hat; 12 to 14 and 18 to 20) buys grown animals not
     carrying young, at their price times her mood that visit (0.9 to
     1.2), a rare colour for half as much again. The cat comes between
     them.
   - Eggs (24), rainbow eggs (48), wool (130) and milk (85) sell at the
     stall for their price; orders ask for them once their pen is open
     (not the rainbow egg); a day's goal may be to feed animals.
   - The test replay `the_barn_gives_an_egg` begins from a save (a
     level-3 farm on Tuesday morning): the hen and the wheat, fed, the
     coop built, a chick bought from the dog, the egg taken three hours
     later.
6. **The farm year** (2026-10-24, built; `year.lua`; the prototype has
   none, so this is the game's own):
   - **The clock** says the season and its day ("Frühling 12 ·
     Montag"); the morning says when a season begins, the year's last
     day and a new year. Seasons change nothing else (yet).
   - **The reckoning** when the bunny wakes into a new year (after the
     56th night, slept or fallen asleep at 2:00): the farm's worth and
     its parts (the coins; the goods in the bag and the carrots at
     their base price; the animals, grown at the fox's base, the young
     at the dog's; what lasts, at what it cost: the bought rows, pots,
     bag slots, the can, the recipes, the stables), what the year added
     to it, what the bunny did (harvested, cooked, orders, sold, fed,
     born, earned), and the collection.
   - **The collection** keeps every kind the farm ever had, over all
     its years: the eleven crops, the moonberry, the six dishes, the
     four products, every animal in every colour (36), the others as
     empty places on the reckoning.
   - **The score** of a year is what it added to the farm's worth (each
     year a run of its own, measured by what was done in it; a later
     year starts richer, but a year's gain stays a year's). The act key
     on the reckoning goes on: the score into the arcade's table (ten
     lines to beat, 40000 down to 500), the farm saved as the new year
     begins, the run ended (initials, the table, the title); CONTINUE
     goes on into the new year. No engine change: `world:save` then
     `world:win()` in one tick.
   - The test replay `the_year_ends_with_its_reckoning` begins from a
     save on the year's last night (1:50), falls asleep at 2:00, reads
     the reckoning and goes on: the run is won with the year's gain.
7. **Out into the world:** delivery trips, arcade interludes. Put off
   for now (the user, 2026-10-29); what follows is the plan as it stands.

   *Draft (2026-10-26), parked by the user the same day; nothing of it
   is built.* The prototype's planned "dungeon" (the bunny brings carrots
   to its family) and the arcade interludes the first draft named
   (driving wolves off, dodging obstacles on the road), made of what
   Voxcade does well and the browser could not: levels to travel to, a
   view that scrolls, things that move and collide. Forgiveness as
   everywhere: a trip or an interlude gone wrong costs time and some of
   what was carried, never coins, animals or crops.

   - **The family** lives in a burrow beyond the woods. Their letter
     comes with the mail on Saturday (market day: the bunny can buy what
     it lacks) and asks for a few things the farm makes: carrots, a dish,
     eggs or wool once the barn has them, three to five lines, more as
     the levels rise. The trip is made on Sunday (cake Sunday: the
     family's day), from 7:00 to 18:00; a letter not answered is simply
     replaced next Saturday.
   - **The road, a level of its own** (`levels/02_road.json`, first
     made by the generator, then the user's in the level editor): the
     farm's gate at the edge where the merchants come is a place
     (`world:go_to("road", "farm")`), the road winds north through
     meadow and woods over three or four screens to the family's burrow,
     and back. The bunny pulls a small cart with the family's things (the
     bag's lines that the letter asks for go into it as the trip
     starts). The clock runs on the road as on the farm; crops grow.
   - **On the road**, things to walk round or answer with the act key:
     puddles that slow the bunny, a fallen branch to lift aside, crows
     that peck at the cart (one thing lost unless shooed off), a river
     crossed on a ferry pulled by a rope.
   - **Arcade interludes**, short (30 to 60 seconds) and scored, the
     best on the year's reckoning:
     - *The downhill run* (on the road, the way there): the cart rolls
       down the long hill by itself, faster and faster, the view
       scrolling with it (as Pulsar's does); left and right dodge rocks,
       molehills and puddles, clover picked up gives XP; a hit spills
       one thing from the cart onto the road (a later run can find it).
     - *Wolves at the pens* (at the farm, the full moon's night, once
       the barn has animals): wolves creep out of the woods towards the
       pens; the bunny runs at each with its lantern lit (act) and it
       turns tail. A wolf that reaches a pen frightens the animals: no
       eggs, wool or milk the next morning, nothing worse.
   - **At the family's burrow** (a small level, the family as figures
     of their own: mother, father, two little ones): handing over gives
     XP, hearts for the collection (a new row: the family's thanks) and,
     for a letter answered in full, a family recipe the cat does not
     sell (carrot cake for cake Sunday, a winter stew). Then the way
     back; at 18:00 the bunny is home whatever happened (the road is
     not walked twice in the dark), at 2:00 it falls asleep as anywhere.
   - **No new engine:** levels and places, `world:go_to`, `level.state`
     and holds are the game model's (stages 3 to 5); the downhill run is
     a level whose camera the game moves (`level.camera:look_at`, as
     Pulsar's); the wolves are things the game moves, as the barn's
     animals are. Saves stay at sleep: the trip is
     always over by then.
   - **For the user to decide:** how often (every Sunday, or whenever a
     letter came); which interludes (the downhill run, the wolves, both,
     or others: catching the crows, a fishing game at the river); one
     road level or a level for each part; what the family gives
     (recipes, XP, seeds no one sells, a decoration for the farm).

## Decided after the first draft (the user, 2026-10-23)

- German, English and French from the start.
- A year of 56 days.
- Falling asleep at 2:00 costs nothing.

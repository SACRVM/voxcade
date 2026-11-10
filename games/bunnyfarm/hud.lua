-- What the HUD shows, in one look: cream paper with dark brown writing on
-- wooden boards, keys as little key caps, pictures of 7 by 7. Coins,
-- carrots, the level and its XP (top left), the clock and the day (top
-- right), the bunny's three needs (bottom left), the chosen seed (bottom
-- right), what the act key does here and the last note (bottom middle);
-- the stall's board under its awning, the burrow's and the bag's.
--
-- A HUD voxel belongs to what is drawn there first: every piece draws
-- what lies on top first (writing, pictures), then the paper, then the
-- board round it. A line of writing takes 9 rows (7 and the two that
-- g, p and y reach below it); a strip of paper round one is 14 high, the
-- writing 3 below its top, so a row of paper stays under the letters.
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
local wolf = require("wolf")
local hud = {}

local PAPER, EDGE = { 246, 234, 206 }, { 120, 82, 52 }
local INK, INK_DIM = { 74, 48, 30 }, { 156, 128, 98 }
local BOARD, BOARD_EDGE = { 156, 106, 64 }, { 92, 60, 38 }
local CREAM = { 250, 242, 222 }
local KEY, KEY_EDGE = { 92, 66, 46 }, { 56, 38, 26 }
local CHOSEN = { 255, 220, 124 }
local GOLD = { 236, 180, 48 }
local GOOD, BAD = { 70, 150, 60 }, { 200, 60, 50 }
local SLOT, SLOT_EDGE = { 122, 84, 52 }, { 82, 54, 34 }
local BAR_BACK = { 200, 180, 150 }
local WATER, GAUGE = { 86, 156, 230 }, 16   -- the can's water, its gauge's width
local NEED_COLORS = { food = { 236, 126, 44 }, energy = { 240, 190, 50 }, mood = { 232, 96, 120 } }
local AWNING = { { 214, 70, 60 }, { 246, 238, 224 } }
local NOTE_TICKS = 150
local SHOW_TICKS = 220
local MORNING_TICKS = 330          -- the morning says the day's event too

-- A sheet or board: filled, a border round it, its corners left out.
local function panel(frame, x, y, w, h, fill, edge)
  frame:box(x + 1, y + 1, w - 2, h - 2, fill)
  frame:box(x + 1, y, w - 2, 1, edge)
  frame:box(x + 1, y + h - 1, w - 2, 1, edge)
  frame:box(x, y + 1, 1, h - 2, edge)
  frame:box(x + w - 1, y + 1, 1, h - 2, edge)
end

-- An action's key as a key cap; how wide it is.
local function keycap(frame, x, y, action)
  local label = "{" .. action .. "}"
  local w = frame:text_width(label) + 4
  frame:text(label, x + 2, y + 1, CREAM)
  panel(frame, x, y, w, 9, KEY, KEY_EDGE)
  return w
end

local function keycaps_width(frame, actions)
  local w = 0
  for _, a in ipairs(actions) do
    w = w + frame:text_width("{" .. a .. "}") + 5
  end
  return w
end

-- A line of keys and what they do ({ { "act" }, "kaufen" } ...), centred
-- on x, at y; the words in `color`.
local function key_line(frame, items, cx, y, color)
  local total = 0
  for i, it in ipairs(items) do
    total = total + keycaps_width(frame, it[1]) + 2 + frame:text_width(it[2]) + (i < #items and 10 or 0)
  end
  local x = math.floor(cx - total / 2 + 0.5)
  for _, it in ipairs(items) do
    for _, a in ipairs(it[1]) do
      x = x + keycap(frame, x, y, a) + 1
    end
    x = x + 2
    frame:text(it[2], x, y + 1, color)
    x = x + frame:text_width(it[2]) + 10
  end
end

-- A strip of paper with a line on it, centred, at y; above the game.
local function tag(frame, s, y, keys)
  local kw = keys and keycaps_width(frame, keys) + 2 or 0
  local w = kw + frame:text_width(s) + 8
  local x = math.floor((frame.width - w) / 2 + 0.5)
  local tx = x + 4
  for _, a in ipairs(keys or {}) do
    tx = tx + keycap(frame, tx, y + 2, a) + 1
  end
  frame:text(s, tx + (keys and 1 or 0), y + 3, INK)
  panel(frame, x, y, w, 14, PAPER, EDGE)
end

-- The items in the bag, in c.items' order: { item, count } a slot.
function hud.bag()
  local slots = {}
  for _, item in ipairs(world.items) do
    local count = player.state.inventory[item.name] or 0
    if count > 0 then
      slots[#slots + 1] = { item = item.name, count = count }
    end
  end
  return slots
end

-- How many slots of the bag a row of its board holds.
function hud.bag_columns()
  return player.state.bag <= 8 and 4 or 6
end

local function name_of(item)
  if item:sub(1, 5) == "seed_" then
    return text.of("seeds", item:sub(6))
  elseif item == "fertilizer" or item == "jar" or item == "wood" then
    return text.of("tools", item)
  elseif D.RECIPES[item] then
    return text.of("dishes", item)
  elseif D.SPECIALS[item] then
    return text.of("specials", item)
  elseif D.BARN.products[item] then
    return text.of("products", item)
  end
  return text.of("crops", item)
end

-- Top left: coins and carrots, the level and its XP.
local function purse(frame)
  local p = player.state
  local x, y = 4, 4
  local coins, carrots = tostring(p.coins), tostring(p.carrots)
  frame:icon("coin", x + 4, y + 3)
  frame:text(coins, x + 13, y + 3, INK)
  local cx = x + 13 + frame:text_width(coins) + 7
  frame:icon("carrot", cx, y + 3)
  frame:text(carrots, cx + 9, y + 3, INK)
  local lv = text.say("level", p.level)
  frame:text(lv, x + 4, y + 13, INK)
  local lw = frame:text_width(lv)
  frame:bar(x + 8 + lw, y + 15, 28, 3, p.xp / D.xp_for(p.level), GOLD, BAR_BACK)
  local w = math.max(cx + 9 + frame:text_width(carrots) + 4 - x, 8 + lw + 28 + 4)
  panel(frame, x, y, w, 24, PAPER, EDGE)
end

-- A day as the clock says it: its season, the day of the season, the
-- weekday ("Frühling 12 · Montag").
local function day_line(now)
  return text.say("day_line", text.of("seasons", clock.season(now)), clock.season_day(now), text.of("weekdays", clock.weekday(now)))
end

-- Top right: the time with the sun or the moon, the day and weekday.
local function clock_panel(frame)
  local now = world.state.now
  local h, m = clock.time(now)
  local time = string.format("%d:%02d", h, m)
  local day = day_line(now)
  local w = math.max(frame:text_width(day), 9 + frame:text_width(time)) + 8
  local x, y = frame.width - 4 - w, 4
  local sky = calendar.mod("weather", nil) == "rain" and "rain" or "sun"
  frame:icon(clock.night(now) and (calendar.full_moon() and "full_moon" or "moon") or sky, x + 4, y + 3)
  frame:text(time, x + 13, y + 3, INK)
  frame:text(day, x + 4, y + 13, INK_DIM)
  panel(frame, x, y, w, 24, PAPER, EDGE)
end

-- How wide the needs' names are, with the room after them.
local function labels_width(frame)
  local lw = 0
  for _, need in ipairs({ "food", "energy", "mood" }) do
    lw = math.max(lw, frame:text_width(text.of("needs", need)) + 4)
  end
  return lw
end

-- Under the clock: the day's event, its picture and its name.
local function event_tag(frame)
  local id, event = calendar.today()
  local s = text.of("events", id)
  local w = frame:text_width(s) + 17
  local x, y = frame.width - 4 - w, 30
  frame:icon(event.icon, x + 4, y + 3)
  frame:text(s, x + 13, y + 3, INK)
  panel(frame, x, y, w, 14, PAPER, EDGE)
end

-- Bottom left: the three needs, and how the bunny is when it is not well.
local function needs(frame, x, y, labels)
  local lw = labels and labels_width(frame) or 0
  for i, need in ipairs({ "food", "energy", "mood" }) do
    local ry = y + 3 + (i - 1) * 9
    frame:icon(need, x + 4, ry)
    if labels then
      frame:text(text.of("needs", need), x + 13, ry, INK)
    end
    frame:bar(x + 14 + lw, ry + 2, 34, 3, player.state[need] / 100, NEED_COLORS[need], BAR_BACK)
  end
  return 52 + lw, 31
end

local function needs_panel(frame)
  local x, y = 4, frame.height - 35
  local w, h = needs(frame, x, y)
  panel(frame, x, y, w, h, PAPER, EDGE)
  -- How the bunny is, when it is not well: over the needs (beside them a
  -- narrow screen's seed panel would be in the way).
  local c = bunny.condition()
  if c ~= "fine" and c ~= "thriving" then
    local s = text.of("condition", c)
    frame:text(s, x + 4, y - 13, INK)
    panel(frame, x, y - 16, frame:text_width(s) + 8, 14, PAPER, EDGE)
  end
end

-- Beside the hand: the bag, how many of its places are taken, the key
-- that opens it (without it nobody knew there was a bag, the user). On
-- the bottom row between the needs and the hand when there is room, else
-- over the hand. `hand_x`: where the hand's panel begins.
local function bag_places()
  local taken = 0
  for _, n in pairs(player.state.inventory) do
    if n > 0 then
      taken = taken + 1
    end
  end
  return taken
end

-- Where the bag's tag stands (left of the hand, else over it): its corner,
-- its words, the width of its key.
local function bag_at(frame, hand_x)
  local s = text.say("bag_places", bag_places(), player.state.bag)
  local kw = keycaps_width(frame, { "bag" })
  local w = kw + 9 + frame:text_width(s) + 12
  local x, y = hand_x - 4 - w, frame.height - 18
  if x < 4 + 52 + 4 then
    x, y = frame.width - 4 - w, frame.height - 34
  end
  return x, y, w, s, kw
end

local function bag_tag(frame, hand_x)
  local taken = bag_places()
  local x, y, w, s, kw = bag_at(frame, hand_x)
  keycap(frame, x + 3, y + 2, "bag")
  frame:icon("bag", x + 4 + kw + 1, y + 3)
  frame:text(s, x + 4 + kw + 10, y + 3, taken >= player.state.bag and BAD or INK)
  panel(frame, x, y, w, 14, PAPER, EDGE)
end

-- Bottom right: what is in hand (seeds, the watering can, fertilizer)
-- between the keys that change it.
local function hand_at(frame)
  local p = player.state
  local icon, label, count = "can", text.of("tools", "can"), 1
  if p.hand == "fertilizer" then
    count = p.inventory.fertilizer or 0
    icon, label = "fertilizer", text.of("tools", "fertilizer") .. " " .. text.say("owned", count)
  elseif p.hand ~= "can" then
    count = p.inventory["seed_" .. p.hand] or 0
    icon, label = "seed_" .. p.hand, text.of("seeds", p.hand) .. " " .. text.say("owned", count)
  end
  local w = keycaps_width(frame, { "prev" }) + keycaps_width(frame, { "next" }) + 9 + frame:text_width(label) + 12
  if p.hand == "can" then
    w = w + GAUGE + 4                     -- the can's water beside its name
  end
  return frame.width - 4 - w, w, icon, label, count
end

local function hand_panel(frame)
  local x, w, icon, label, count = hand_at(frame)
  local y = frame.height - 18
  local kx = x + 3
  kx = kx + keycap(frame, kx, y + 2, "prev") + 3
  frame:icon(icon, kx, y + 3)
  frame:text(label, kx + 9, y + 3, count > 0 and INK or INK_DIM)
  if player.state.hand == "can" then
    local water = player.state.water or 0
    frame:bar(kx + 9 + frame:text_width(label) + 4, y + 5, GAUGE, 3, water / D.CAN.holds, WATER, BAR_BACK)
  end
  keycap(frame, x + w - 3 - keycaps_width(frame, { "next" }) + 1, y + 2, "next")
  panel(frame, x, y, w, 14, PAPER, EDGE)
  return x
end

-- Over the bottom row, in the middle: what the act key does here, at a
-- fertilized bed for how many harvests yet, and the last note above.
local function hints(frame)
  local hint, fertilized, keyless
  if level.state.mending then
    -- hammering at a fence: no hint over it
  elseif bunny.at("stall") then
    hint = text.say("at_stall")
  elseif bunny.at("burrow") then
    hint = text.say("at_burrow")
  elseif bunny.at("kitchen") then
    hint = text.say("at_kitchen")
  elseif bunny.at("mailbox") then
    hint = text.say("at_mailbox")
  elseif bunny.at("chicken_gate") or bunny.at("sheep_gate") or bunny.at("cow_gate") then
    for _, z in ipairs(D.BARN.zone_order) do
      if bunny.at(z .. "_gate") then
        hint = text.of("pens", z)
      end
    end
  elseif level.state.cat and player:near(level.state.cat, 22) then
    hint = text.say("at_cat")
  elseif level.state.dog and player:near(level.state.dog, 22) then
    hint = text.say("at_dog")
  elseif level.state.fox and player:near(level.state.fox, 22) then
    hint = text.say("at_fox")
  elseif wolf.stray() then
    hint = text.say("bring_home")
  elseif wolf.gap_here() then
    if (player.state.inventory.wood or 0) > 0 then
      hint = text.say("mend")
    else
      hint, keyless = text.say("gap_wood"), true
    end
  else
    local i = farm.ahead()
    local would = farm.would(i)
    if would == "sow" then
      hint = text.say("sow", text.of("crops", player.state.hand))
    elseif would == "harvest" or would == "water" or would == "fertilize" or would == "buy_row" then
      hint = text.say(would)
    elseif would == "can_empty" then
      hint, keyless = text.say("can_empty"), true
    end
    -- a fertilized bed says for how many harvests yet
    local bed = i and world.state.beds[i]
    if bed and bed.fert > 0 then
      fertilized = text.say(bed.fert == 1 and "fert_left_one" or "fert_left", bed.fert)
    end
    -- at the pond with the can: fill it (full: says so)
    if not i and player.state.can and farm.at_pond() then
      if player.state.water < D.CAN.holds then
        hint = text.say("fill_can")
      else
        hint, keyless = text.say("can_full"), true
      end
    end
    -- by the scarecrow: every ripe bed at once, for a fee (none ripe: says so)
    if not i and not level.state.reaping and farm.by_scarecrow() then
      local n = #farm.ripe()
      if n > 0 then
        hint = text.say("reap", n, n * D.REAP.fee)
      else
        hint, keyless = text.say("nothing_ripe"), true
      end
    end
  end
  -- an animal run off following the bunny: where to
  local leading = wolf.leading()
  if leading and not hint then
    hint, keyless = text.say("lead", text.of("pens", leading)), true
  end
  -- one over the other from the bottom row up: what the act key does, the
  -- bed's fertilizer, the note
  local y = frame.height - 34
  if hint then
    tag(frame, hint, y, not keyless and { "act" } or nil)
    y = y - 16
  end
  if fertilized then
    tag(frame, fertilized, y)
    y = y - 16
  end
  local note = level.state.note
  if note and world.tick - note.at < NOTE_TICKS then
    tag(frame, text.noted(note), y)
  end
end

local function dropped(frame, s, y, ticks, color, size)
  frame:drop_in(s, math.floor((frame.width - frame:text_width(s, size)) / 2 + 0.5), y, ticks, color, size)
end

-- A new level, a new morning: big words dropping in on a strip of paper.
local function moments(frame)
  local half = frame.height / 2
  local lines
  local up, morning = level.state.level_up, level.state.morning
  if up and world.tick - up.at < SHOW_TICKS then
    local names = {}
    for i, o in ipairs(up.opened) do
      names[i] = type(o) == "table" and text.of(o[1], o[2]) or text.of("crops", o)
    end
    lines = { up.at, text.say("level_up", up.level), #names > 0 and text.say("opened", table.concat(names, ", ")) or nil }
  elseif morning and world.tick - morning.at < MORNING_TICKS then
    local now = world.state.now
    local id = calendar.today()
    -- the morning says the year's turns: a new year, a new season, the
    -- year's last day
    local greeting = text.say("morning")
    local days = D.DAYS_A_SEASON * D.SEASONS
    if morning.fell then
      greeting = text.say("fell_asleep")
    elseif clock.season_day(now) == 1 and clock.season(now) == 1 and clock.year(now) > 1 then
      greeting = text.say("new_year", clock.year(now))
    elseif clock.season_day(now) == 1 then
      greeting = text.of("season_begin", clock.season(now))
    elseif (clock.day(now) - 1) % days == days - 1 then
      greeting = text.say("last_day")
    end
    lines = { morning.at, day_line(now), greeting, text.say("event_line", text.of("events", id), text.of("event_says", id)) }
    for _, n in ipairs(morning.news or {}) do
      lines[#lines + 1] = text.noted(n)      -- the night's news (the wolf, a gap in a fence)
    end
  end
  if not lines then
    return
  end
  local t = world.tick - lines[1] + frame.alpha
  dropped(frame, lines[2], half - 30, t, INK, 2)
  local w, h = frame:text_width(lines[2], 2), 30
  for k = 3, #lines do
    dropped(frame, lines[k], half - 8 + (k - 3) * 11, t - 20 * (k - 2), INK, 1)
    w = math.max(w, frame:text_width(lines[k]))
    h = h + (k == 3 and 12 or 11)
  end
  w = w + 24
  panel(frame, math.floor((frame.width - w) / 2), half - 38, w, h, PAPER, EDGE)
end

-- A board in the middle of the screen with a sign on top saying its
-- title, its paper `paper_h` high inside, keys and what they do below.
-- `body(px, py, pw)` draws what lies on the paper; `above(frame, x, y, w)`
-- hangs over the board (the stall's awning).
local function board_at(frame, w, paper_h)
  local h = paper_h + 32
  return math.floor((frame.width - w) / 2), math.floor((frame.height - h) / 2) + 6, h
end

local function board(frame, title, w, paper_h, keys, body, above)
  local x, y, h = board_at(frame, w, paper_h)
  -- the sign with the title, over the board's top edge
  local sw = frame:text_width(title) + 12
  local sx = math.floor((frame.width - sw) / 2)
  frame:text(title, sx + 6, y - 7, CREAM)
  panel(frame, sx, y - 10, sw, 14, BOARD, BOARD_EDGE)
  if above then
    above(frame, x, y, w)
  end
  body(x + 6, y + 8, w - 12)
  panel(frame, x + 6, y + 8, w - 12, paper_h, PAPER, EDGE)
  key_line(frame, keys, frame.width / 2, y + h - 14, CREAM)
  panel(frame, x, y, w, h, BOARD, BOARD_EDGE)
end

-- The stall's awning over its board: stripes, every other one longer.
local function awning(frame, x, y, w)
  local ax, aw = x - 6, w + 12
  for i = 0, aw // 8 do
    local sx = ax + i * 8
    local sw = math.min(8, ax + aw - sx)
    if sw > 0 then
      local red = i % 2 == 0
      frame:box(sx, y - 12, sw, red and 15 or 13, AWNING[red and 1 or 2])
    end
  end
end

-- A row of goods: its picture, name, what the bunny has, its price (in
-- coins, or in what `pay` names) with the day's event's sign by it
-- (`moved`, green when it is good for the bunny, red when not).
-- A text cut to fit `w` UI voxels, an ellipsis at its end when cut.
local function fit(frame, s, w)
  if frame:text_width(s) <= w then
    return s
  end
  for n = utf8.len(s) - 1, 1, -1 do
    local cut = s:sub(1, utf8.offset(s, n + 1) - 1) .. "…"
    if frame:text_width(cut) <= w then
      return cut
    end
  end
  return "…"
end

local function row(frame, px, ry, pw, r, chosen)
  local ink = r.dim and INK_DIM or INK
  if r.icon then
    frame:icon(r.icon, px + 5, ry + 2)
  end
  -- the name, cut where it would run into what stands at the right
  local right = 0
  if type(r.value) == "number" then
    right = frame:text_width(tostring(r.value)) + 9 + (r.moved and 8 or 0)
  elseif r.value then
    right = frame:text_width(r.value)
  end
  local owned = r.owned and frame:text_width(r.owned) + 4 or 0
  local label = fit(frame, r.text, pw - 26 - right - owned)
  frame:text(label, px + 15, ry + 2, ink)
  if r.owned then
    frame:text(r.owned, px + 19 + frame:text_width(label), ry + 2, INK_DIM)
  end
  if type(r.value) == "number" then
    local v = tostring(r.value)
    local vx = px + pw - 5 - frame:text_width(v)
    frame:text(v, vx, ry + 2, ink)
    frame:icon(r.pay or "coin", vx - 9, ry + 2)
    if r.moved then
      frame:text(r.moved, vx - 17, ry + 2, r.moved_bad and BAD or GOOD)
    end
  elseif r.value then
    frame:text(r.value, px + pw - 5 - frame:text_width(r.value), ry + 2, r.urgent and INK or INK_DIM)
  end
  if chosen then
    frame:box(px + 2, ry, pw - 4, 12, CHOSEN)
  end
end

-- Tabs at the top of a board's paper, each a strip of its own, the
-- chosen one yellow, the others the board's wood. The rows below them
-- start at TABBED (from the paper's top); the coins beside them stand on
-- their line, at TAB_LINE.
local TABBED, TAB_LINE = 19, 6

local function tabs_bar(frame, px, py, names, chosen)
  local tx = px + 4
  for i, t in ipairs(names) do
    local tw = frame:text_width(t) + 10
    frame:text(t, tx + 5, py + TAB_LINE, i == chosen and INK or CREAM)
    panel(frame, tx, py + TAB_LINE - 3, tw, 14, i == chosen and CHOSEN or BOARD, i == chosen and EDGE or BOARD_EDGE)
    tx = tx + tw + 3
  end
end

-- The bunny's coins at the top right of a board's paper, their line at y.
local function coins_corner(frame, px, y, pw)
  local coins = tostring(player.state.coins)
  frame:text(coins, px + pw - 5 - frame:text_width(coins), y, INK)
  frame:icon("coin", px + pw - 14 - frame:text_width(coins), y)
end

-- A list of `count` rows 12 high from y0, six shown at a time round the
-- chosen one (`at`; more and the stall's board reaches the panels at the
-- bottom), a scroll bar at the right when there are more; `draw(i, ry,
-- rw)` draws row i at ry, rw wide.
local SHOWN = 6

local function shown_of(count)
  return math.min(count, SHOWN)
end

-- The first row a list shows, scrolled so the chosen one is in view.
local function list_top(count, at)
  return math.max(1, math.min(at - SHOWN // 2, count - SHOWN + 1))
end

local function list(frame, px, y0, pw, count, at, draw)
  local shown = shown_of(count)
  local top = list_top(count, at)
  local rw = count > shown and pw - 5 or pw
  if count > shown then
    local h = shown * 12 - 1
    local thumb = math.max(4, math.floor(h * shown / count + 0.5))
    local ty = y0 + math.floor((h - thumb) * (top - 1) / (count - shown) + 0.5)
    frame:box(px + pw - 5, ty, 2, thumb, EDGE)
    frame:box(px + pw - 5, y0, 2, h, BAR_BACK)
  end
  for i = top, top + shown - 1 do
    draw(i, y0 + (i - top) * 12, rw)
  end
end

-- What a row needs, right-aligned from x: each picture with its count.
local function needs_right(frame, x, ry, needs, ink)
  for k = #needs, 1, -1 do
    local n = needs[k]
    if n.count then
      local c = tostring(n.count)
      x = x - frame:text_width(c)
      frame:text(c, x, ry + 2, ink)
      x = x - 1
    end
    x = x - 8
    frame:icon(n.icon, x, ry + 2)
    x = x - 4
  end
  return x
end

-- The stall's board: its tabs, the bunny's coins, the goods.
local function market_board(frame, m)
  local rows = market.rows(m.tab)
  board(frame, text.say("stall"), 220, TABBED + 3 + shown_of(#rows) * 12,
        { { { "act" }, text.of("keys", m.tab == 1 and "buy" or "sell") }, { { "prev", "next" }, text.of("keys", "tab") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          tabs_bar(frame, px, py, { text.say("buy"), text.say("sell") }, m.tab)
          coins_corner(frame, px, py + TAB_LINE, pw)
          list(frame, px, py + TABBED, pw, #rows, m.at, function(i, ry, rw)
            local r = rows[i]
            local look = { text = r.text, value = r.value, icon = r.icon, dim = r.dim, pay = r.pay, moved = r.moved,
                           moved_bad = r.moved and (r.good == "seed") == (r.moved == "▲") }
            local inv = player.state.inventory
            local n = r.good == "seed" and (inv["seed_" .. r.crop] or 0) or (r.good == "fertilizer" and (inv.fertilizer or 0))
              or (r.good == "jar" and (inv.jar or 0)) or (r.good == "wood" and (inv.wood or 0))
              or (r.good == "carrot" and player.state.carrots) or 0
            look.owned = n > 0 and text.say("owned", n) or nil
            row(frame, px, ry, rw, look, i == m.at)
          end)
        end,
        awning)
end

-- What a recipe needs, as pictures with counts (needs_right's).
local function recipe_needs(id)
  local rec, needs = D.RECIPES[id], {}
  for _, item in ipairs(D.CROP_ORDER) do
    if rec.needs[item] then
      needs[#needs + 1] = { icon = item, count = rec.needs[item] }
    end
  end
  if rec.jar then
    needs[#needs + 1] = { icon = "jar" }
  end
  return needs
end

-- How wide needs_right draws `needs`.
local function needs_width(frame, needs)
  local w = 0
  for _, n in ipairs(needs) do
    w = w + 12 + (n.count and frame:text_width(tostring(n.count)) + 1 or 0)
  end
  return w
end

-- A kitchen row's words, and how much room its right side takes (the
-- time and bar of a pot cooking, the tick of one done, what a recipe
-- needs, a pot's price).
local function kitchen_row(frame, r)
  if r.pot then
    local job = r.job
    if not job then
      return text.say("pot_empty", r.pot), 0
    elseif job.left <= 0 then
      return text.say("pot_done", r.pot, text.of("dishes", job.recipe)), 12
    end
    return text.say("pot_cooking", r.pot, text.of("dishes", job.recipe)), 40 + frame:text_width("00:00")
  elseif r.recipe then
    return text.of("dishes", r.recipe), 5 + needs_width(frame, recipe_needs(r.recipe))
  end
  return text.say("buy_pot"), 14 + frame:text_width(tostring(r.buy_pot))
end

-- The kitchen's board: its pots (what cooks and how long yet, or done),
-- the recipes owned with what they need, another pot to buy. As wide as
-- its longest row needs (a pot cooking a long dish's name), at most the
-- screen.
local function kitchen_width(frame, rows)
  local width = 230
  for _, r in ipairs(rows) do
    local words, right = kitchen_row(frame, r)
    width = math.max(width, 12 + 15 + frame:text_width(words) + 6 + right + (#rows > SHOWN and 5 or 0))
  end
  return math.min(width, frame.width - 8)
end

local function kitchen_board(frame, m)
  local rows = kitchen.rows()
  local width = kitchen_width(frame, rows)
  board(frame, text.say("kitchen"), width, 18 + shown_of(#rows) * 12,
        { { { "act" }, text.of("keys", "cook") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          coins_corner(frame, px, py + 3, pw)
          list(frame, px, py + 15, pw, #rows, m.at, function(i, ry, rw)
            local r, chosen = rows[i], i == m.at
            local words = kitchen_row(frame, r)
            if r.pot then
              local job = r.job
              if not job then
                row(frame, px, ry, rw, { icon = "pot", text = words, dim = true }, chosen)
              elseif job.left <= 0 then
                frame:icon("check", px + rw - 12, ry + 2)
                row(frame, px, ry, rw, { icon = job.recipe, text = words }, chosen)
              else
                local left = string.format("%d:%02d", job.left // 60, job.left % 60)
                frame:text(left, px + rw - 5 - frame:text_width(left), ry + 2, INK_DIM)
                frame:bar(px + rw - 40 - frame:text_width(left), ry + 4, 30, 3, 1 - job.left / job.total, GOLD, BAR_BACK)
                row(frame, px, ry, rw, { icon = job.recipe, text = words }, chosen)
              end
            elseif r.recipe then
              needs_right(frame, px + rw - 5, ry, recipe_needs(r.recipe), r.why and INK_DIM or INK)
              row(frame, px, ry, rw, { icon = r.recipe, text = words, dim = r.why ~= nil }, chosen)
            else
              row(frame, px, ry, rw, { icon = "pot", text = words, value = r.buy_pot, dim = player.state.coins < r.buy_pot }, chosen)
            end
          end)
        end)
end

-- The mailbox's board: its orders (what each asks, what the bunny has of
-- it, what it pays) and the day's goals, two tabs.
local function mail_board(frame, m)
  local box = world.state.mail
  local count = m.tab == 1 and D.ORDERS.slots or #box.goals + 1
  local keys = m.tab == 1
    and { { { "act" }, text.of("keys", "deliver") }, { { "bag" }, text.of("keys", "reject") }, { { "prev", "next" }, text.of("keys", "tab") }, { { "back" }, text.of("keys", "back") } }
    or { { { "prev", "next" }, text.of("keys", "tab") }, { { "back" }, text.of("keys", "back") } }
  board(frame, text.say("mailbox"), 290, TABBED + 3 + count * 12, keys, function(px, py, pw)
    tabs_bar(frame, px, py, { text.say("orders_tab"), text.say("goals_tab") }, m.tab)
    coins_corner(frame, px, py + TAB_LINE, pw)
    for i = 1, count do
      local ry, chosen = py + TABBED + (i - 1) * 12, m.tab == 1 and i == m.at
      if m.tab == 1 then
        local order = box.orders[i]
        if not order then
          local left = math.max(0, box.refill[i] - world.state.awake)
          row(frame, px, ry, pw, { icon = "letter", text = text.say("order_empty", left // 60, left % 60), dim = true }, chosen)
        else
          local can = mail.can_deliver(i)
          local x = px + 15
          for _, g in ipairs(order.goods) do
            local have = math.min(player.state.inventory[g.item] or 0, g.count)
            frame:icon(g.item, x, ry + 2)
            local c = have .. "/" .. g.count
            frame:text(c, x + 9, ry + 2, have >= g.count and INK or INK_DIM)
            x = x + 9 + frame:text_width(c) + 8
          end
          row(frame, px, ry, pw, { icon = can and "check" or "letter", text = "", value = order.coins, dim = not can }, chosen)
        end
      elseif i <= #box.goals then
        local goal = box.goals[i]
        local done = goal.got >= goal.need
        local progress = goal.got .. "/" .. goal.need
        frame:text(progress, px + pw - 40 - frame:text_width(progress), ry + 2, INK_DIM)
        row(frame, px, ry, pw, { icon = done and "check" or nil, text = string.format(text.of("goals", goal.kind), goal.need), value = goal.coins, dim = done }, false)
      else
        row(frame, px, ry, pw, { icon = box.bonus and "check" or nil, text = text.say("all_goals"), value = D.GOALS.bonus.coins, dim = box.bonus }, false)
      end
    end
  end)
end

-- The cat's board: the recipes she has for the bunny now.
local function cat_board(frame, m)
  local offers = kitchen.offers()
  local count = math.max(1, #offers)
  board(frame, text.say("cat_title"), 230, 18 + count * 12,
        { { { "act" }, text.of("keys", "buy") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          coins_corner(frame, px, py + 3, pw)
          if #offers == 0 then
            row(frame, px, py + 15, pw, { text = text.say("no_offers"), dim = true }, false)
            return
          end
          for i, id in ipairs(offers) do
            local price = D.RECIPES[id].price
            row(frame, px, py + 15 + (i - 1) * 12, pw, { icon = id, text = text.of("dishes", id), value = price, dim = player.state.coins < price }, i == m.at)
          end
        end)
end

-- An animal: its picture, its name (a hen, a cock, a chick...) and its
-- colour, what it is doing.
local function animal_icon(a)
  return a.sp .. "_" .. a.colour
end

local function cap(s)
  return s:sub(1, 1):upper() .. s:sub(2)
end

local function animal_name(a, grown)
  if a.sex == "r" then
    return text.say("rainbow_hen")
  elseif not grown then
    return cap(text.of("young", a.sp)) .. " · " .. text.of("colours", a.colour) .. " · " .. text.of("sexes", a.sex)
  end
  return text.of(a.sex == "m" and "male" or "female", a.sp) .. " · " .. text.of("colours", a.colour)
end

local function animal_doing(a, s, sire)
  local function hm(m) return m // 60, m % 60 end
  if s.away then
    return text.say("st_away")
  elseif not s.grown then
    return text.say("st_growing", hm(s.grow_left))
  elseif s.due then
    return text.say("st_waiting")
  elseif s.carrying then
    return text.say(a.sp == "chicken" and "st_brooding" or "st_carrying", hm(s.carry_left))
  elseif s.scared and s.working then
    return text.say("st_scared", hm(s.scared))
  elseif s.ready then
    return text.say("st_ready", text.of("products", s.product))
  elseif s.making then
    return text.say("st_making", text.of("products", s.product), hm(s.left))
  elseif s.hungry then
    return text.say("st_hungry", D.BARN.animals[a.sp].feed)
  end
  return sire and text.say("st_sire") or nil
end

-- A pen's board: how many places are taken, the bag's wheat and the
-- coins; each animal (fed or its product taken by the act key, the bag
-- key for young), the stable to build.
local function pen_board(frame, m)
  local zone = m.zone
  local rows = barn.pen_rows(zone)
  board(frame, text.of("pens", zone), 270, 18 + shown_of(#rows) * 12,
        { { { "act" }, text.of("keys", "tend") }, { { "bag" }, text.of("keys", "breed") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          frame:text(text.say("pen_places", #barn.animals(zone), barn.places(zone)), px + 5, py + 3, INK)
          coins_corner(frame, px, py + 3, pw)
          local wheat = tostring(player.state.inventory.wheat or 0)
          local cw = frame:text_width(tostring(player.state.coins)) + 14
          frame:text(wheat, px + pw - 5 - cw - 8 - frame:text_width(wheat), py + 3, INK)
          frame:icon("wheat", px + pw - 5 - cw - 17 - frame:text_width(wheat), py + 3)
          list(frame, px, py + 15, pw, #rows, m.at, function(i, ry, rw)
            local r, chosen = rows[i], i == m.at
            if r.animal then
              local a = r.animal
              local s = barn.status(a)
              row(frame, px, ry, rw, { icon = animal_icon(a), text = animal_name(a, s.grown), value = animal_doing(a, s, m.sire == a.id),
                                       urgent = s.ready or s.hungry }, chosen)
            elseif r.build then
              row(frame, px, ry, rw, { icon = "stable", text = text.say("build_stable", D.BARN.zones[zone].most), value = r.build,
                                       dim = player.state.coins < r.build }, chosen)
            else
              row(frame, px, ry, rw, { text = text.say("pen_empty"), dim = true }, chosen)
            end
          end)
        end)
end

-- The dog's board: the young animals he has this visit, their price.
local function dog_board(frame, m)
  local stock = barn.stock()
  local items = stock and stock.items or {}
  board(frame, text.say("dog_title"), 290, 18 + math.max(1, shown_of(#items)) * 12,
        { { { "act" }, text.of("keys", "buy") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          coins_corner(frame, px, py + 3, pw)
          if #items == 0 then
            row(frame, px, py + 15, pw, { text = text.say("dog_empty"), dim = true }, false)
            return
          end
          list(frame, px, py + 15, pw, #items, m.at, function(i, ry, rw)
            local it = items[i]
            local name = animal_name(it, false)
            if it.sex ~= "r" and D.BARN.animals[it.sp].rare[it.colour] then
              name = name .. " · " .. text.say("rare")
            end
            local why = barn.why_not_buy(it)
            row(frame, px, ry, rw, { icon = it.sp .. "_" .. it.colour, text = name,
                                     value = it.bought and text.say("sold_out") or D.BARN.animals[it.sp].buy, dim = why ~= nil }, i == m.at)
          end)
        end)
end

-- The fox's board: her mood this visit, the grown animals she would take
-- and what she pays for each.
local function fox_board(frame, m)
  local list_ = barn.for_sale()
  board(frame, text.say("fox_title"), 260, 18 + math.max(1, shown_of(#list_)) * 12,
        { { { "act" }, text.of("keys", "sell") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          local visit = barn.visit(D.FOX)
          if visit then
            frame:text(text.say("fox_mood", string.format("%.2f", barn.fox_mood(visit))), px + 5, py + 3, INK_DIM)
          end
          coins_corner(frame, px, py + 3, pw)
          if #list_ == 0 then
            row(frame, px, py + 15, pw, { text = text.say("fox_empty"), dim = true }, false)
            return
          end
          list(frame, px, py + 15, pw, #list_, m.at, function(i, ry, rw)
            local a = list_[i]
            row(frame, px, ry, rw, { icon = animal_icon(a), text = animal_name(a, true), value = barn.price(a) }, i == m.at)
          end)
        end)
end

local function burrow_board(frame, m)
  local p = player.state
  local rows = {
    { text = text.say("eat"), icon = "carrot", owned = text.say("owned", p.carrots), dim = p.carrots < 1 },
    { text = text.say("sleep"), icon = "moon" },
  }
  if clock.minute(world.state.now) < D.BEDTIME then
    rows[2].dim, rows[2].value = true, text.say("from_hour", D.BEDTIME // 60)
  end
  -- as wide as the longest word for how the bunny is needs beside the needs
  local cw = 0
  for _, c in ipairs({ "thriving", "fine", "tired", "exhausted", "hungry", "bored" }) do
    cw = math.max(cw, frame:text_width(text.of("condition", c)))
  end
  local width = math.max(200, 12 + 52 + labels_width(frame) + 8 + cw + 5)
  -- and as its rows need: a row's picture, its words, room, its value
  for _, r in ipairs(rows) do
    local value = r.value and frame:text_width(tostring(r.value)) + 16 or 0
    width = math.max(width, 12 + 15 + frame:text_width(r.text) + (r.owned and frame:text_width(r.owned) + 4 or 0) + value + 5)
  end
  board(frame, text.say("burrow"), width, 6 + #rows * 12 + 34,
        { { { "act" }, text.of("keys", "choose") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          for i, r in ipairs(rows) do
            row(frame, px, py + 3 + (i - 1) * 12, pw, r, i == m.at)
          end
          local w = needs(frame, px, py + 30, true)
          frame:text(text.of("condition", bunny.condition()), px + w + 8, py + 40, INK_DIM)
        end)
end

-- A fallow row's board: buy it for its price, or not.
local function row_board(frame, m)
  local price = D.ROWS[world.state.rows + 1] or 0
  local rows = {
    { text = text.say("buy_the_row"), icon = "field", value = price, dim = player.state.coins < price },
    { text = text.say("cancel") },
  }
  board(frame, text.say("more_field"), 200, 6 + #rows * 12,
        { { { "act" }, text.of("keys", "choose") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          for i, r in ipairs(rows) do
            row(frame, px, py + 3 + (i - 1) * 12, pw, r, i == m.at)
          end
        end)
end

local function bag_board(frame, m)
  local slots = hud.bag()
  local size, gap = 18, 3
  local cols = hud.bag_columns()
  local lines = (player.state.bag + cols - 1) // cols
  local paper_h = lines * (size + gap) - gap + 66
  board(frame, text.say("bag"), 200, paper_h,
        { { { "act" }, text.of("keys", "seed") }, { { "back" }, text.of("keys", "back") } },
        function(px, py, pw)
          local gx = px + math.floor((pw - (cols * size + (cols - 1) * gap)) / 2)
          for i = 1, player.state.bag do
            local sx = gx + ((i - 1) % cols) * (size + gap)
            local sy = py + 5 + ((i - 1) // cols) * (size + gap)
            local s = slots[i]
            if s then
              local n = tostring(s.count)
              frame:text(n, sx + size - 2 - frame:text_width(n), sy + size - 9, CREAM)
              frame:icon(s.item, sx + 3, sy + 3)
            end
            panel(frame, sx, sy, size, size, SLOT, i == m.at and CHOSEN or SLOT_EDGE)
            if i == m.at then
              panel(frame, sx - 1, sy - 1, size + 2, size + 2, CHOSEN, CHOSEN)
            end
          end
          local s = slots[m.at]
          local name = s and name_of(s.item) or text.say("empty_slot")
          local ny = py + 5 + lines * (size + gap) - gap + 4
          frame:text(name, px + math.floor((pw - frame:text_width(name)) / 2), ny, s and INK or INK_DIM)
          needs(frame, px + 30, ny + 10, true)
          local c = text.of("condition", bunny.condition())
          frame:text(c, px + math.floor((pw - frame:text_width(c)) / 2), ny + 44, INK_DIM)
        end)
end

-- The year's reckoning, the whole screen's: the farm's worth and its
-- parts and what the year added to it (the score); what the bunny did
-- this year; the collection, every kind the farm ever had, the others
-- as empty places.
local function reckoning_board(frame)
  local r = world.state.reckoning
  local pictures = year.collectables()
  local across = 18
  local lines = math.max(5, #year.STATS)
  local paper_h = 14 + lines * 10 + 14 + ((#pictures + across - 1) // across) * 9 + 4
  board(frame, text.say("year_title", r.number), 300, paper_h, { { { "act" }, text.say("next_year", r.number + 1) } },
        function(px, py, pw)
          local half = math.floor(pw / 2)
          local function line(x, y, label, value, ink)
            frame:text(label, x, y, ink or INK)
            local v = tostring(value)
            frame:text(v, x + half - 14 - frame:text_width(v), y, ink or INK)
          end
          frame:text(text.say("the_farm"), px + 5, py + 4, INK_DIM)
          for i, part in ipairs({ "coins", "goods", "animals", "farm" }) do
            line(px + 5, py + 4 + i * 10, text.of("worth_parts", part), r.parts[part])
          end
          frame:box(px + 5, py + 4 + 5 * 10 - 2, half - 14, 1, INK_DIM)
          line(px + 5, py + 4 + 5 * 10, text.say("worth"), r.worth)
          line(px + 5, py + 4 + 6 * 10, text.say("gain"), "+" .. r.gain)
          frame:text(text.say("the_year"), px + half + 5, py + 4, INK_DIM)
          for i, stat in ipairs(year.STATS) do
            local v = r.stats[stat] or 0
            frame:text(text.of("year_stats", stat), px + half + 5, py + 4 + i * 10, INK)
            frame:text(tostring(v), px + pw - 5 - frame:text_width(tostring(v)), py + 4 + i * 10, INK)
          end
          local cy = py + 4 + (lines + 1) * 10 + 4
          local have, all = year.collected()
          frame:text(text.say("collection", have, all), px + 5, cy, INK_DIM)
          local gx = px + math.floor((pw - across * 9 + 2) / 2)
          for i, key in ipairs(pictures) do
            local x, y = gx + ((i - 1) % across) * 9, cy + 11 + ((i - 1) // across) * 9
            if world.state.collection[key] then
              frame:icon(key, x, y)
            else
              frame:box(x + 1, y + 1, 5, 5, BAR_BACK)
            end
          end
        end)
end

-- Where the HUD keeps what flies to it this picture (fx.lua): the coins'
-- and the carrots' pictures, the bag's, and the rows of the stall's or
-- the kitchen's board when open (by their number, at their right end).
function hud.spots(frame)
  local hand_x = hand_at(frame)
  local bx, by, _, _, kw = bag_at(frame, hand_x)
  local spots = {
    coins = { x = 8, y = 7 },
    carrots = { x = 4 + 13 + frame:text_width(tostring(player.state.coins)) + 7, y = 7 },
    bag = { x = bx + 4 + kw + 1, y = by + 3 },
    rows = {},
  }
  local m = level.state.menu
  local rows, w, paper_h, y0
  if m and m.kind == "market" then
    rows, w = market.rows(m.tab), 220
    paper_h, y0 = TABBED + 3 + shown_of(#rows) * 12, TABBED
  elseif m and m.kind == "kitchen" then
    rows = kitchen.rows()
    w = kitchen_width(frame, rows)
    paper_h, y0 = 18 + shown_of(#rows) * 12, 15
  end
  if rows then
    local x, y = board_at(frame, w, paper_h)
    local top = list_top(#rows, m.at)
    for i = top, math.min(#rows, top + SHOWN - 1) do
      spots.rows[i] = { x = x + w - 24, y = y + 8 + y0 + (i - top) * 12 + 2 }
    end
  end
  return spots
end

function hud.draw(frame)
  local m = level.state.menu
  if m and m.kind == "reckoning" then
    reckoning_board(frame)
    return
  end
  purse(frame)
  clock_panel(frame)
  if m then
    if m.kind == "market" then
      market_board(frame, m)
    elseif m.kind == "burrow" then
      burrow_board(frame, m)
    elseif m.kind == "row" then
      row_board(frame, m)
    elseif m.kind == "kitchen" then
      kitchen_board(frame, m)
    elseif m.kind == "mailbox" then
      mail_board(frame, m)
    elseif m.kind == "cat" then
      cat_board(frame, m)
    elseif m.kind == "pen" then
      pen_board(frame, m)
    elseif m.kind == "dog" then
      dog_board(frame, m)
    elseif m.kind == "fox" then
      fox_board(frame, m)
    else
      bag_board(frame, m)
    end
  else
    event_tag(frame)
    hints(frame)
    moments(frame)
  end
  needs_panel(frame)
  local hand_x = hand_panel(frame)
  if not m then
    bag_tag(frame, hand_x)
  end
end

return hud

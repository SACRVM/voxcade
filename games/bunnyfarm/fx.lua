-- Small signs of what happened, as the prototype showed them: what a bed
-- gave rising from it (a harvest's picture and count, a drop of water,
-- the fertilizer), coins and goods flying to where the HUD keeps them (a
-- sale and a purchase at the stall, a pot or a place in the bag paid,
-- the morning's gift) and hearts rising over a mother as she pairs. Only
-- for the eye: they live in level.state.fx (on_tick adds and ends them,
-- on_draw draws them where the level's points show, frame:to_screen, or
-- where the HUD's counters and the open board's rows are, hud.spots).
-- What rises from the level waits while a board covers it.
local fx = {}

local RISE, FLY, HEARTS = 50, 36, 80        -- ticks each lasts
local LIFT = 18                              -- how far a sign rises (UI voxels)
local INK = { 70, 48, 34 }

local function add(e)
  local list = level.state.fx or {}
  e.began = world.tick
  list[#list + 1] = e
  level.state.fx = list
end

-- What a bed gave, rising from it: `count` of the picture `icon` (no
-- count: the picture alone).
function fx.rise(at, icon, count)
  add({ kind = "rise", at = { x = at.x, y = at.y, z = at.z }, icon = icon, count = count })
end

-- A picture flying from `from` to `to`: a point of the level, a row of
-- the open board ({ row = n }) or a counter of the HUD ("coins",
-- "carrots", "bag").
function fx.fly(icon, from, to)
  add({ kind = "fly", icon = icon, from = fx.place(from), to = fx.place(to) })
end

-- A place as an effect keeps it: a copy of its own (a state is a tree,
-- no table in it twice).
function fx.place(p)
  if type(p) ~= "table" then
    return p
  end
  return { x = p.x, y = p.y, z = p.z, row = p.row }
end

-- Hearts rising over an animal's thing.
function fx.hearts(thing)
  add({ kind = "hearts", thing = thing })
end

local function life(e)
  return e.kind == "rise" and RISE or e.kind == "fly" and FLY or HEARTS
end

-- A tick: what is over ends; what rises from the level waits while a
-- board is open over it.
function fx.tick()
  local list = level.state.fx
  if not list then
    return
  end
  local keep = {}
  for _, e in ipairs(list) do
    if e.kind ~= "fly" and level.state.menu then
      e.began = e.began + 1
    end
    if world.tick - e.began < life(e) and (e.kind ~= "hearts" or e.thing) then
      keep[#keep + 1] = e
    end
  end
  level.state.fx = #keep > 0 and keep or nil
end

-- Where a place of an effect shows on the HUD this picture, or nil.
local function where(frame, spots, p)
  if type(p) == "string" then
    local s = spots[p]
    return s and s.x, s and s.y
  elseif p.row then
    local s = spots.rows and spots.rows[p.row]
    return s and s.x, s and s.y
  end
  return frame:to_screen(p)
end

local function ease(t)
  return t * t * (3 - 2 * t)
end

-- The effects of one sort: what flies to the HUD ("over", drawn before
-- the HUD so it shows over its panels: a HUD voxel is the first's to
-- claim it), or what rises from the level ("under", drawn after it, so
-- the panels stand before it).
function fx.draw(frame, spots, sort)
  local list = level.state.fx
  if not list then
    return
  end
  for _, e in ipairs(list) do
    local t = world.tick - e.began + frame.alpha
    if (e.kind == "fly") ~= (sort == "over") then
      -- the other sort's
    elseif e.kind == "rise" and not level.state.menu and t >= 0 then
      local x, y = frame:to_screen(e.at, 18)
      if x then
        y = y - LIFT * ease(math.min(1, t / RISE))
        local s = e.count and ("+" .. e.count) or ""
        local w = frame:text_width(s) + (s ~= "" and 2 or 0) + 7
        local left = math.floor(x - w / 2)
        if s ~= "" then
          frame:text(s, left, math.floor(y), INK)
        end
        frame:icon(e.icon, left + w - 7, math.floor(y))
      end
    elseif e.kind == "fly" then
      local fx0, fy0 = where(frame, spots, e.from)
      local fx1, fy1 = where(frame, spots, e.to)
      if fx0 and fx1 then
        local k = ease(math.min(1, t / FLY))
        frame:icon(e.icon, math.floor(fx0 + (fx1 - fx0) * k), math.floor(fy0 + (fy1 - fy0) * k - 10 * math.sin(k * math.pi)))
      end
    elseif e.kind == "hearts" and not level.state.menu and t >= 0 then
      local x, y = frame:to_screen(e.thing, 12)
      if x then
        for k = 0, 2 do
          local tk = t - k * 12
          if tk >= 0 and tk < HEARTS - 24 then
            local rise = LIFT * ease(math.min(1, tk / (HEARTS - 24)))
            frame:icon("heart", math.floor(x - 3 + (k - 1) * 6), math.floor(y - rise))
          end
        end
      end
    end
  end
end

return fx

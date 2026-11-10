-- The pens' back fence and the wolf (data.lua's WOLF). A night may break
-- a gap into the back fence of a pen with animals (world.state.barn.gaps:
-- a pen's gap, the cell across); wood from the stall mends it. The wolf
-- is shy: he comes only while the bunny sleeps, to a pen whose gap is
-- still open. Its animals are frightened for a day (barn.scared: they
-- give nothing) and one runs off through the gap into the meadow behind
-- (its `out`: where it waits). The bunny fetches it: out through the
-- farm's gate and round the fence (the gap keeps its lower rail: nobody
-- passes it), the act key by it, and it follows the bunny's trail home
-- to its pen's gate (level.state.lead). Nothing dies.
local D = require("data")
local text = require("text")
local bunny = require("bunny")
local barn = require("barn")
local wolf = {}

local W, B, N = D.WOLF, D.BARN, D.CELL
local FENCE_Z = D.FIELD.top          -- the fence's cells lie on the ground's
local STEP, BEHIND = 4, 5            -- a point of the trail every STEP voxels; the animal keeps BEHIND points back
local REACH = 20                     -- how near the bunny comes to fetch an animal
local TALL = { chicken = 10, sheep = 12, cow = 15 }   -- the animals' heights (conf.lua's)

-- ------------------------------------------------------------ the fence

-- The middle of a pen's gap (a cell across in the back row), in voxels.
local function gap_middle(cell)
  return cell * N + 8, W.row * N + 8
end

-- A gap broken into a cell of the back fence: its top rail gone and its
-- post broken off, the lower rail left (no body passes over it). The
-- voxels taken, to put back when it is mended: a flat list of where in
-- the cell (x + 16 y + 256 z) and what each was.
local function break_cell(cell)
  local x0, y0, taken = cell * N, W.row * N, {}
  for z = 7, N - 1 do
    for y = 0, N - 1 do
      for x = 0, N - 1 do
        local stump = (x == 7 and z <= 8) or (x == 8 and z <= 7)
        local v = level:voxel(x0 + x, y0 + y, FENCE_Z + z)
        if v ~= 0 and not stump then
          taken[#taken + 1] = x + 16 * y + 256 * z
          taken[#taken + 1] = v
          level:set_voxel(x0 + x, y0 + y, FENCE_Z + z, nil)
        end
      end
    end
  end
  return { cell = cell, taken = taken }
end

local function mend_cell(drawn)
  local x0, y0 = drawn.cell * N, W.row * N
  for i = 1, #drawn.taken, 2 do
    local at = drawn.taken[i]
    level:set_voxel(x0 + at % 16, y0 + at // 16 % 16, FENCE_Z + at // 256, drawn.taken[i + 1])
  end
end

-- The fence as the states say (every tick; what is broken now in
-- level.state.fences, nil while the fence is whole, so a farm without
-- gaps keeps its states as they were).
function wolf.paint()
  local gaps = world.state.barn.gaps
  local drawn = level.state.fences
  if not gaps and not drawn then
    return
  end
  drawn = drawn or {}
  for _, z in ipairs(B.zone_order) do
    local want = gaps and gaps[z] or false
    local have = drawn[z] and drawn[z].cell or false
    if want ~= have then
      if have then
        mend_cell(drawn[z])
        drawn[z] = nil
      end
      if want then
        drawn[z] = break_cell(want)
      end
    end
  end
  level.state.fences = next(drawn) and drawn or nil
end

-- The pen whose gap the bunny stands by (inside the pen or outside), or nil.
function wolf.gap_here()
  local gaps = world.state.barn.gaps
  if not gaps then
    return nil
  end
  for _, z in ipairs(B.zone_order) do
    if gaps[z] then
      local x, y = gap_middle(gaps[z])
      if math.abs(player.x - x) < 14 and math.abs(player.y - y) < 20 then
        return z
      end
    end
  end
  return nil
end

-- Mending a pen's gap: a piece of wood from the bag, then three blows of
-- the hammer (level.state.mending; the bunny turned to the fence, held
-- there), each knocking a third of the fence back in, the post first.
-- Whether it could begin.
local STRIKES, STRIKE, RAISED, HALF = 3, 14, 6, 2   -- blows; ticks a blow, raised, half down (then the blow)
local THIRDS = { { 6, 9 }, { 0, 5 }, { 10, 15 } }  -- what each blow puts back (across the cell)

function wolf.mend(zone)
  local inv = player.state.inventory
  if (inv.wood or 0) < 1 then
    text.note("gap_wood")
    return false
  end
  inv.wood = inv.wood > 1 and inv.wood - 1 or nil
  local _, y = gap_middle(world.state.barn.gaps[zone])
  player:face({ x = 0, y = y > player.y and 1 or -1 })
  level.state.mending = { zone = zone, at = world.tick }
  return true
end

local function mended(zone)
  local gaps = world.state.barn.gaps
  gaps[zone] = nil
  if next(gaps) == nil then
    world.state.barn.gaps = nil
  end
  level.state.mending = nil
  wolf.paint()
end

-- The level entered: its fence made whole (the gaps are broken in anew),
-- nobody led, a mending done. A saved farm comes back with its gaps
-- already broken into the voxels: what they took (its fences as saved) is
-- put back first, or breaking them anew would find nothing to keep for
-- the mending.
function wolf.build()
  local drawn = level.state.fences
  if drawn then
    for _, z in ipairs(B.zone_order) do
      if drawn[z] then
        mend_cell(drawn[z])
      end
    end
  end
  level.state.fences, level.state.lead = nil, nil
  if level.state.mending then
    mended(level.state.mending.zone)    -- saved with the hammer raised: done all the same
  else
    wolf.paint()
  end
end

-- A tick of mending: a blow knocks, puts its third back; after the last
-- the fence is whole.
function wolf.mend_step()
  local m = level.state.mending
  local t = world.tick - m.at
  if t % STRIKE == RAISED + HALF then
    local k = t // STRIKE + 1
    local drawn = level.state.fences and level.state.fences[m.zone]
    if drawn then
      local x0, y0 = drawn.cell * N, W.row * N
      for i = 1, #drawn.taken, 2 do
        local at = drawn.taken[i]
        if at % 16 >= THIRDS[k][1] and at % 16 <= THIRDS[k][2] then
          level:set_voxel(x0 + at % 16, y0 + at // 16 % 16, FENCE_Z + at // 256, drawn.taken[i + 1])
        end
      end
    end
    local x, y = gap_middle(world.state.barn.gaps[m.zone])
    level:sound("mend", { x = x, y = y, z = FENCE_Z + 6 })
    level.state.working = world.tick
  end
  if t >= STRIKES * STRIKE then
    mended(m.zone)
    bunny.work()
    text.note("mended")
  end
end

-- The bunny's frame now, mending (its model's 6 to 8: the hammer
-- raised, half down, the blow), or nil when it is not.
function wolf.hammer()
  local m = level.state.mending
  if not m then
    return nil
  end
  local t = (world.tick - m.at) % STRIKE
  return t < RAISED and 6 or (t < RAISED + HALF and 7 or 8)
end

-- ------------------------------------------------------------ the night

-- Where an animal run off through a gap waits: in the meadow behind it,
-- clear of what stands there (the first free of a few spots, from one
-- drawn at random).
local SPOTS = { { 0, 72 }, { 18, 56 }, { -18, 56 }, { 30, 40 }, { -30, 40 }, { 0, 30 } }

local function spot(cell)
  local gx, gy = gap_middle(cell)
  local first = math.random(1, #SPOTS)
  for k = 0, #SPOTS - 1 do
    local s = SPOTS[(first + k - 1) % #SPOTS + 1]
    local x, y = gx + s[1], gy + s[2]
    local free = true
    for _, t in ipairs(level:things()) do
      local half = t.props.solid
      if half and math.abs(x - t.x) < half[1] + 10 and math.abs(y - t.y) < half[2] + 10 then
        free = false
        break
      end
    end
    if free then
      return { x = x, y = y }
    end
  end
  return { x = gx, y = gy + 24 }
end

-- An animal led home now, or nil: its thing, its record. Only reads (the
-- HUD asks it too); wolf.tick lets go of one no longer out there.
local function led()
  local lead = level.state.lead
  if not lead then
    return nil
  end
  local t, a = level.state.herd[lead.id], barn.animal(lead.id)
  if not (t and a and a.out) then
    return nil
  end
  return t, a
end

-- Where an animal goes in through its pen's gate: the gate (where the
-- bunny stands at it), then a little inside.
local function way_in(sp)
  local gate = level.places[sp .. "_gate"]
  return { x = gate.x, y = gate.y }, { x = gate.x, y = gate.y + 34 }
end

-- An animal home again: in through its gate, then about its pen.
local function home(t, a)
  a.out = nil
  level.state.lead = nil
  t.state.to, t.state.wait = false, 60
end

-- The night, as the bunny sleeps (after bunny.sleep: the clock says the
-- morning). An animal still led finds its own way home; the wolf comes to
-- the pens whose gap is still open, frightens them and one runs off; new
-- gaps break into fences of pens with animals. What the morning says of
-- it ({ key, args } each, as text.note's), or nil.
function wolf.night()
  local b, now, news = world.state.barn, world.state.now, {}
  if level.state.mending then
    mended(level.state.mending.zone)    -- fell asleep with the hammer raised: done all the same
  end
  local t, a = led()
  if t then
    local _, inside = way_in(a.sp)
    t:place(inside.x, inside.y)
    home(t, a)
  end
  if b.scared then
    for _, z in ipairs(B.zone_order) do
      if b.scared[z] and b.scared[z] <= now then
        b.scared[z] = nil
      end
    end
    if next(b.scared) == nil then
      b.scared = nil
    end
  end
  for _, z in ipairs(B.zone_order) do
    local gap = b.gaps and b.gaps[z]
    local animals = gap and barn.animals(z) or {}
    if #animals > 0 and math.random(1, 100) <= W.comes then
      b.scared = b.scared or {}
      b.scared[z] = now - now % D.MINUTES_A_DAY + W.hour * 60 + W.fright   -- from this night's hour
      local inside = {}
      for _, x in ipairs(animals) do
        if not x.out then
          inside[#inside + 1] = x
        end
      end
      if #inside > 0 then
        local a = inside[math.random(1, #inside)]
        a.out = spot(gap)
        local t = level.state.herd and level.state.herd[a.id]
        if t then
          t:place(a.out.x, a.out.y)
          t.state.to = false
        end
      end
      news[#news + 1] = { key = "wolf_came", args = { { "pens", z } } }
      news[#news + 1] = { key = "wolf_did", args = {} }
    end
  end
  for _, z in ipairs(B.zone_order) do
    if #barn.animals(z) > 0 and not (b.gaps and b.gaps[z]) and math.random(1, 100) <= W.gap then
      b.gaps = b.gaps or {}
      b.gaps[z] = W.cells[z][math.random(1, #W.cells[z])]
      news[#news + 1] = { key = "gap_found", args = { { "pens", z } } }
    end
  end
  return #news > 0 and news or nil
end

-- ------------------------------------------------------------ fetching

-- The thing of an animal run off that the bunny stands by (not led yet),
-- or nil.
function wolf.stray()
  local lead = level.state.lead
  for id, t in pairs(level.state.herd or {}) do
    local a = barn.animal(id)
    if a and a.out and not (lead and lead.id == id) and player:near(t, REACH) then
      return t
    end
  end
  return nil
end

-- The animal run off follows the bunny from now on.
function wolf.lead(t)
  local a = barn.animal(t.state.animal)
  level.state.lead = { id = a.id, trail = { { x = player.x, y = player.y } }, stuck = 0, home = false }
  t.state.to = false
end

-- The pen of the animal led now, or nil.
function wolf.leading()
  local _, a = led()
  return a and a.sp
end

-- A tick: the bunny's trail grows as it goes; the animal led walks it,
-- BEHIND points back, at the bunny's pace (quicker when it lags); by its
-- pen's gate, or close behind the bunny standing there, it goes in on
-- its own. An animal stuck on a corner skips to
-- the next point.
function wolf.tick()
  local t, a = led()
  if not t then
    level.state.lead = nil
    return
  end
  local lead = level.state.lead
  local trail = lead.trail
  local last = trail[#trail]
  if not lead.home and (not last or math.abs(player.x - last.x) + math.abs(player.y - last.y) >= STEP) then
    trail[#trail + 1] = { x = player.x, y = player.y }
  end
  local gate, inside = way_in(a.sp)
  local by_gate = math.abs(t.x - gate.x) < 16 and math.abs(t.y - gate.y) < 16
  if not lead.home and (by_gate or (bunny.at(a.sp .. "_gate") and player:near(t, 40))) then
    lead.home, lead.trail = true, { gate, inside }
    trail = lead.trail
  end
  while trail[1] and math.abs(trail[1].x - t.x) < 1.5 and math.abs(trail[1].y - t.y) < 1.5 do
    table.remove(trail, 1)
  end
  if lead.home and #trail == 0 then
    home(t, a)
    text.note("back_home", { "pens", a.sp })
    level:sound("levelup", t)
    return
  end
  if not lead.home and #trail <= BEHIND then
    lead.stuck = 0
    return
  end
  local p = trail[1]
  local dx, dy = p.x - t.x, p.y - t.y
  local d = math.sqrt(dx * dx + dy * dy)
  local from = { x = t.x, y = t.y }
  t:walk({ x = dx / d, y = dy / d }, bunny.speed() * (#trail > BEHIND + 4 and 1.3 or 1.05))
  lead.at = lead.at or from
  if math.abs(lead.at.x - from.x) + math.abs(lead.at.y - from.y) < 0.2 then
    lead.stuck = lead.stuck + 1
    if lead.stuck > 40 then
      t:place(p.x, p.y)
      table.remove(trail, 1)
      lead.stuck = 0
    end
  else
    lead.stuck = 0
  end
  lead.at = from
end

-- Before each gap its top rail, fallen into the pen and lying aslant in
-- the grass (only drawn); a question mark over each animal run off and
-- not fetched yet, bobbing.
local RAIL = { 172, 126, 80 }

function wolf.draw(frame)
  local mending = level.state.mending
  for z, drawn in pairs(level.state.fences or {}) do
    local x0, y0 = drawn.cell * N, W.row * N
    if not (mending and mending.zone == z) then   -- being mended: the rail picked up
      for i = 0, 11 do
        for w = 0, 1 do
          frame:cube(x0 + 2 + i, y0 + 1 + i // 4 + w, FENCE_Z, RAIL)
        end
      end
    end
  end
  if level.state.menu then
    return
  end
  local lead = level.state.lead
  for id, t in pairs(level.state.herd or {}) do
    local a = barn.animal(id)
    if a and a.out and not (lead and lead.id == id) then
      local x, y = frame:to_screen(t, TALL[a.sp] + 8)
      if x then
        local bob = math.sin((world.tick + frame.alpha) / 12) * 1.5
        frame:text("?", math.floor(x - 2), math.floor(y - 7 + bob), { 255, 236, 120 })
      end
    end
  end
end

return wolf

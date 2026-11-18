-- Lights: one subject at a time, each a part of the hall with its lamps,
-- its figures and a camera on it. Q and E step through them, the arrows
-- walk the hero. scenarios/ begin at each one.

-- The subjects, in the order Q and E go through them:
--   lamps:  the level's lamps it lights (levels/hall.toml, by name)
--   look:   the camera, { x, y, width, pitch, fov }; none: it follows the hero
--   hero:   where the hero is put, { x, y }
--   fog:    the air, { density, glow, anisotropy }; none: clear
--   things: what is set down, { kind, x, y, move }: move "ring" walks it
--           round, a way { x, y, speed } straight on, none stands
--   moving: lamps it draws itself every picture (moving_lamps below)
local SPOTS = { "spot_falloff_4", "spot_falloff_2", "spot_falloff_1", "spot_soft", "spot_narrow", "spot_pitched", "spot_rotated" }
-- (the hero waits in a far corner of the stages or of the hall, out of view)
local AWAY, AWAY_HALL = { 390, 150 }, { 20, 150 }
local SUBJECTS = {
  { name = "hall", label = "THE HALL", lamps = { "door_lamp", "pillar_lamp" }, hero = { 60, 30 },
    fog = { 0.004, 1, 0 } },
  { name = "walls", label = "LAMPS AT WALLS", lamps = { "door_lamp", "pillar_lamp" }, hero = AWAY,
    look = { 120, 100, 160, 80, 25 } },
  { name = "diagonal_wall", label = "A DIAGONAL WALL", lamps = { "diagonal_lamp" }, hero = AWAY_HALL,
    look = { 214, 22, 100, 70, 25 } },
  { name = "spots", label = "SPOTS", lamps = SPOTS, hero = AWAY_HALL,
    look = { 322, 46, 135, 65, 25 } },
  { name = "shadows", label = "NONE  STATIC  ALL", lamps = { "bay_none", "bay_static", "bay_all" }, hero = AWAY_HALL,
    look = { 190, 110, 110, 55, 20 },
    things = { { "guard", 165, 104 }, { "guard", 190, 104 }, { "guard", 215, 104 } } },
  { name = "fog", label = "FOG AND BEAMS", lamps = { "fog_cut", "fog_thin", "fog_free" }, hero = AWAY_HALL,
    look = { 285, 120, 150, 75, 25 }, fog = { 0.01, 2, 0.3 } },
  { name = "glass", label = "GLASS AND WATER", lamps = { "glass_spot", "water_spot" }, hero = AWAY_HALL,
    look = { 358, 118, 140, 60, 25 }, fog = { 0.008, 3, 0 } },
  { name = "dancers", label = "DANCERS", lamps = {}, hero = AWAY, moving = "dancers",
    look = { 80, 96, 150, 55 }, fog = { 0.006, 1, 0 },
    things = { { "dancer", 70, 80, "ring" }, { "dancer", 84, 100, "ring" }, { "dancer", 108, 112, "ring" },
               { "guard", 46, 98, "ring" }, { "guard", 118, 84, "ring" } } },
  { name = "posed", label = "A POSED FIGURE", lamps = { "posed_spot" }, hero = AWAY_HALL,
    look = { 216, 82, 80, 60 }, things = { { "posed", 214, 80 } } },
  { name = "beam", label = "THROUGH A BEAM", lamps = { "beam_spot" }, hero = AWAY,
    look = { 80, 92, 180, 65 }, fog = { 0.006, 3, 0 },
    things = { { "dancer", 40, 92, { 1, 0, 10 } } } },
  { name = "out_of_view", label = "OUT OF VIEW", lamps = { "headlight_left", "headlight_right" }, hero = AWAY,
    look = { 100, 26, 140, 60 }, things = { { "dancer", 40, 19, { -1, 0, 10 } } } },
  { name = "headlights", label = "HEADLIGHTS", lamps = {}, hero = AWAY, moving = "headlights",
    look = { 75, 44, 200, 40 }, fog = { 0.004, 1, 0 },
    things = { { "guard", 50, 40 }, { "dancer", 70, 44 }, { "guard", 90, 40 }, { "dancer", 110, 44 } } },
  { name = "load", label = "16 LAMPS 50 FIGURES", lamps = {}, hero = AWAY, moving = "load",
    look = { 82, 92, 210, 50 }, fog = { 0.006, 1, 0 } },
}

-- Every lamp the level has (each subject lights its own, the rest dark).
local LAMPS = { "door_lamp", "pillar_lamp", "diagonal_lamp", "bay_none", "bay_static", "bay_all",
  "fog_cut", "fog_thin", "fog_free", "glass_spot", "water_spot", "posed_spot", "beam_spot",
  "headlight_left", "headlight_right" }
for _, name in ipairs(SPOTS) do
  table.insert(LAMPS, name)
end

-- The figures' kinds set down (not the hero), each drawn with its model
-- in figures/.
local FIGURES = { dancer = true, guard = true, posed = true }

-- The load's figures: 50 walking round, spread over the floor.
SUBJECTS[#SUBJECTS].things = {}
for i = 1, 50 do
  table.insert(SUBJECTS[#SUBJECTS].things, { i % 2 == 0 and "dancer" or "guard", 30 + (i * 37) % 100, 64 + (i * 23) % 70, "ring" })
end

local function subject_named(name)
  for i, s in ipairs(SUBJECTS) do
    if s.name == name then
      return i, s
    end
  end
  return 1, SUBJECTS[1]
end

-- Sets the subject up: its lamps on and the rest dark, its figures, the
-- hero beside it.
local function show(s)
  for _, th in ipairs(level:things()) do
    if FIGURES[th.kind] then
      th:remove()
    end
  end
  for _, name in ipairs(LAMPS) do
    level:thing(name).state.light = 0
  end
  for _, name in ipairs(s.lamps) do
    level:thing(name).state.light = 1
  end
  for _, t in ipairs(s.things or {}) do
    local th = level:spawn(t[1], t[2], t[3])
    if t[4] == "ring" then
      th.state.ring = true
    elseif t[4] then
      th.state.way = { x = t[4][1], y = t[4][2] }
      th.state.speed = t[4][3]
    end
  end
  player:place(s.hero[1], s.hero[2])
  level.state.shown = s.name
end

function on_tick()
  local s = world.state
  local i = subject_named(s.subject or "hall")
  if input:pressed("next") then
    i = i % #SUBJECTS + 1
  elseif input:pressed("prev") then
    i = (i - 2) % #SUBJECTS + 1
  end
  s.subject = SUBJECTS[i].name
  if level.state.shown ~= s.subject then
    show(SUBJECTS[i])
  end
  player:walk(input.dir, 40)
  for _, th in ipairs(level:things()) do
    if th.state.ring then
      local a = world.tick * 0.04 + th.id
      th:walk({ x = math.cos(a), y = math.sin(a) }, 24)
    elseif th.state.way then
      th:walk(th.state.way, th.state.speed)
    end
  end
end

-- The lamps a subject moves itself, every picture (t: seconds).
local moving_lamps = {
  -- four coloured spots turning over the dancers
  dancers = function(frame, t)
    local spots = {
      { 45, 95, 90, -60, { 255, 60, 60 }, 0.6 },
      { 80, 138, 180, -55, { 70, 130, 255 }, -0.45 },
      { 112, 90, 270, -65, { 80, 255, 120 }, 0.8 },
      { 78, 92, 0, -90, { 255, 220, 140 }, 0 },
    }
    for _, p in ipairs(spots) do
      local rotation = (p[3] + p[6] * 60 * t) % 360
      frame:light{ at = { x = p[1], y = p[2], z = 52 }, radius = 150, color = p[5], cone = 24, blend = 0.2,
        rotation = rotation, pitch = p[4], falloff = 0.7, shadows = "all", fog = 3 }
    end
  end,
  -- two headlights driving east past the figures
  headlights = function(frame, t)
    local x = 10 + (30 * t) % 110
    for _, y in ipairs({ 36, 48 }) do
      frame:light{ at = { x = x, y = y, z = 10 }, radius = 120, color = { 255, 240, 200 }, cone = 30, blend = 0.2,
        rotation = 90, pitch = -4, falloff = 0.7, shadows = "all", fog = 2 }
    end
  end,
  -- sixteen spots straight down over the floor, four by four
  load = function(frame, t)
    for i = 0, 15 do
      local x, y = 36 + (i % 4) * 22, 70 + math.floor(i / 4) * 17
      frame:light{ at = { x = x, y = y, z = 52 }, radius = 90, color = { 255, 230, 200 }, cone = 40, blend = 0.2,
        pitch = -90, falloff = 0.7, shadows = "all", fog = 2 }
    end
  end,
}

function on_draw(frame)
  local _, s = subject_named(world.state.subject or "hall")
  local t = (world.tick + frame.alpha) / 60
  frame:model("figures/hero.vvx", player)
  for _, th in ipairs(level:things()) do
    if th.kind == "posed" then
      -- a figure of parts turned off the grid, its arms, head and legs
      -- swinging
      local w = math.sin(t * 3)
      frame:model("figures/posed.vvx", th, nil, { rotation = 25 + 30 * w }, {
        arm_left = { pitch = 80 + 60 * w }, arm_right = { pitch = -60 * w, roll = 20 },
        head = { rotation = 40 * w }, leg_left = { pitch = 25 * w }, leg_right = { pitch = -25 * w } })
    elseif FIGURES[th.kind] then
      frame:model("figures/" .. th.kind .. ".vvx", th)
    end
  end
  if s.moving then
    moving_lamps[s.moving](frame, t)
  end
  if s.look then
    frame:camera{ look_at = { x = s.look[1], y = s.look[2], z = 4 }, width = s.look[3], pitch = s.look[4], fov = s.look[5] }
  end
  if s.fog then
    frame:fog{ density = s.fog[1], glow = s.fog[2], anisotropy = s.fog[3], color = { 6, 6, 10 } }
  end
  frame:text(s.label, 4, 4)
  local keys = "{prev} {next}"
  frame:text(keys, frame.width - 4 - frame:text_width(keys), 4)
end

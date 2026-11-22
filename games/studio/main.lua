-- Studio: a drum machine one plays through its model. machine.vvx is a
-- figure of parts (tools/gen_studio.py made it; the editor shapes it
-- now): a click names the part under the pointer (input.pointer.part),
-- and the machine answers on itself, posed by its state each tick: a
-- button held sinks a voxel, a lamp lit stands in its socket (put out, it
-- sinks into the case), a knob turns. As in games/drums each row is a
-- channel of MML laid over the instruments of music/kit.mml; the machine
-- edits one instrument at a time, its sixteen buttons that one's steps.
-- A knob is one value, turned by the hand alone (a drag or the wheel on
-- it): each voice has its own level knob, so picking another voice moves
-- none.

local ROWS = {
  { name = "kick", channel = "K" },
  { name = "snare", channel = "S" },
  { name = "hat", channel = "H" },
}

-- Where the parts lie on the machine, in its own voxels from its box's
-- low corner (tools/gen_studio.py's numbers; the API gives no part's
-- place): a button's corner, width and depth. Only for the mark under the
-- pointer.
local W, D, TOP = 151, 77, 9
local LAYOUT = {
  pick_kick = { 8, 26, 9, 6 }, pick_snare = { 32, 26, 9, 6 }, pick_hat = { 56, 26, 9, 6 },
  pattern_a = { 102, 26, 9, 6 }, pattern_b = { 116, 26, 9, 6 }, start = { 132, 26, 11, 6 },
  knob_kick = { 8, 39, 9, 9 }, knob_snare = { 32, 39, 9, 9 }, knob_hat = { 56, 39, 9, 9 },
  knob_tempo = { 104, 39, 9, 9 }, knob_volume = { 131, 39, 9, 9 },
}
local KNOBS = { "knob_kick", "knob_snare", "knob_hat", "knob_tempo", "knob_volume" }
for i = 1, 16 do
  LAYOUT["step_" .. i] = { 8 + (i - 1) * 8 + (i - 1) // 4 * 3, 4, 6, 5 }
end

local function steps(picture)
  local row = {}
  for i = 1, 16 do row[i] = picture:sub(i, i) == "x" end
  return row
end

function on_world_start()
  world.state.patterns = {
    { steps("x...x...x...x..."), steps("....x.......x..."), steps("..x...x...x...x.") },
    { steps("x..x..x...x.x..."), steps("....x..x....x..x"), steps("x.x.x.x.x.x.x.xx") },
  }
  world.state.shown = 1      -- the pattern edited, and asked for
  world.state.playing = 1    -- the pattern the music plays
  world.state.row = 1        -- the instrument the step buttons edit
  world.state.step = 1       -- the keys' cursor
  world.state.running = true
  -- The knobs by their parts' names, 0 to 1: each voice's level, the
  -- tempo (shown only: a game cannot set a song's tempo yet), the volume.
  world.state.knobs = { knob_kick = 0.8, knob_snare = 0.8, knob_hat = 0.8, knob_tempo = 0.5, knob_volume = 1 }
end

-- The pattern shown as channels of MML, a row each: its instrument and
-- level, sixteenths.
local function channels()
  local s, out = world.state, {}
  for r, row in ipairs(ROWS) do
    local notes = { "@" .. row.name .. " v" .. math.floor(s.knobs["knob_" .. row.name] * 15 + 0.5) .. " o4 l16" }
    for i = 1, 16 do notes[#notes + 1] = s.patterns[s.shown][r][i] and "c" or "r" end
    out[row.channel] = table.concat(notes, " ")
  end
  return out
end

-- The pattern shown, laid: at once when it is the one playing, else
-- from the next bar. Stopped, nothing is laid (start lays it).
function lay_pattern()
  local s = world.state
  if not s.running then return end
  music:play("kit", { from = s.shown == s.playing and "now" or "bar", channels = channels() })
end

-- Starts or stops the machine (a global, for the scenarios): stopped, a
-- silent studio plays; started, the kit begins anew at its first step.
function start_stop()
  local s = world.state
  s.running = not s.running
  if s.running then
    s.playing = s.shown
    music:play("kit", { from = "now", channels = channels() })
  else
    music:play("off", { from = "now", channels = { R = "@rest v0 c1" } })
  end
end

function on_level_start(how)
  world.state.playing = world.state.shown
  lay_pattern()
  music:volume(world.state.knobs.knob_volume, 0)
end

function on_beat(beat)
  if beat % 4 == 0 then world.state.playing = world.state.shown end
end

-- A knob turned by the hand: its value, and what it changes.
local function turn(knob, by)
  local s = world.state
  s.knobs[knob] = math.max(0, math.min(1, s.knobs[knob] + by))
  if knob == "knob_volume" then
    music:volume(s.knobs[knob], 0.05)
  elseif knob ~= "knob_tempo" then
    lay_pattern()
  end
end

-- What pressing a part does (the mouse's click, or the keys').
local function press(part)
  local s = world.state
  local n = part:match("^step_(%d+)$")
  if n then
    local row = s.patterns[s.shown][s.row]
    s.step = tonumber(n)
    row[s.step] = not row[s.step]
    lay_pattern()
  elseif part:match("^pick_") then
    for r, row in ipairs(ROWS) do
      if part == "pick_" .. row.name then s.row = r end
    end
  elseif part == "pattern_a" or part == "pattern_b" then
    s.shown = part == "pattern_a" and 1 or 2
    lay_pattern()
  elseif part == "start" then
    start_stop()
  end
end

-- The step sounding now (nil while stopped).
local function sounding()
  if world.state.running and music.position then
    return math.floor(music.position * 4) % 16 + 1
  end
end

-- The machine's pose: buttons held down, lamps lit or sunk, knobs turned.
local function pose()
  local s = world.state
  local parts = {}
  local function lamp(name, lit)
    if not lit then parts[name] = { offset = { 0, 0, -3 } } end
  end
  local light, live = sounding(), s.shown == s.playing
  for i = 1, 16 do
    -- A set step lit; the running light turns the step it passes over.
    local on = s.patterns[s.shown][s.row][i]
    if live and i == light then on = not on end
    lamp("led_" .. i, on)
  end
  for r, row in ipairs(ROWS) do lamp("led_" .. row.name, r == s.row) end
  -- The pattern shown lit; the one still playing blinks until the bar.
  local blink = music.position and math.floor(music.position * 2) % 2 == 0
  for p, name in ipairs({ "a", "b" }) do
    lamp("led_" .. name, p == s.shown or (p == s.playing and blink))
  end
  lamp("led_start", s.running)
  for _, knob in ipairs(KNOBS) do
    parts[knob] = { rotation = (s.knobs[knob] - 0.5) * 270 }
  end
  if s.held then parts[s.held] = { offset = { 0, 0, -1 } } end
  return parts
end

-- The part a pointer is over, as a control: a lamp is its button's.
local LAMPS = { led_kick = "pick_kick", led_snare = "pick_snare", led_hat = "pick_hat",
  led_a = "pattern_a", led_b = "pattern_b", led_start = "start" }
local function control(p)
  local part = p and p.thing and p.thing.name == "machine" and p.part
  if not part then return nil end
  local n = part:match("^led_(%d+)$")
  return n and "step_" .. n or LAMPS[part] or (LAYOUT[part] and part)
end

function on_tick()
  local s = world.state
  local p = input.pointer
  local over = control(p)

  -- The mouse: a press on a part, a knob dragged up or down or wheeled.
  if input:pressed("click") and over then
    if over:match("^knob_") then
      s.drag = { knob = over, y = p.y, from = s.knobs[over] }
    else
      s.held = over
      press(over)
    end
  end
  if s.drag then
    if input:held("click") and p then
      turn(s.drag.knob, s.drag.from + (s.drag.y - p.y) / 40 - s.knobs[s.drag.knob])
    else
      s.drag = nil
    end
  end
  if over and over:match("^knob_") then
    if input:pressed("turn_up") then turn(over, 1 / 15) end
    if input:pressed("turn_down") then turn(over, -1 / 15) end
  end
  if s.held and not input:held("click") and not input:held("toggle") then s.held = nil end

  -- The keys: the cursor along the steps, up and down the instruments.
  if input:repeated("left") then s.step = (s.step - 2) % 16 + 1 end
  if input:repeated("right") then s.step = s.step % 16 + 1 end
  if input:repeated("up") then s.row = (s.row - 2) % #ROWS + 1 end
  if input:repeated("down") then s.row = s.row % #ROWS + 1 end
  if input:pressed("toggle") then
    s.held = "step_" .. s.step
    press(s.held)
  end
  if input:pressed("pattern") then press(s.shown == 1 and "pattern_b" or "pattern_a") end
  if input:pressed("start") then press("start") end

  level:thing("machine").state.parts = pose()
end

-- Where a part's top middle lies in the level (from LAYOUT; the machine
-- faces north, its pivot the middle of its box).
local function top_of(part)
  local m, l = level:thing("machine"), LAYOUT[part]
  return {
    x = math.floor(m.x) - W // 2 + l[1] + l[3] / 2,
    y = math.floor(m.y) - D // 2 + l[2] + l[4] / 2,
    z = math.floor(m.z) + TOP + 5,
  }
end

function on_draw(frame)
  -- The part under the pointer, else the keys' cursor, marked by a small
  -- light over it.
  local mark = control(frame.pointer) or "step_" .. world.state.step
  frame:light{ at = top_of(mark), radius = 9, color = "#c8dcff", shadows = "none" }
  frame:text("ARROWS {toggle} STEP  {pattern} PATTERN  {start} START", 4, frame.height - 9, "#8a8690")
end

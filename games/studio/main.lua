-- Studio: a drum machine one plays through its model. machine.vvx is a
-- figure of parts (tools/gen_studio.py made it; the editor shapes it
-- now): a click names the part under the pointer (input.pointer.part),
-- and the machine answers on itself, posed by its state each tick (a
-- button held sinks a voxel, a knob turns) and for each picture (a lamp
-- lit or out by its glow, the part under the pointer lit a little:
-- frame:pose). As in games/drums each row is a
-- channel of MML laid over the instruments of music/kit.mml; the machine
-- edits one instrument at a time, its sixteen buttons that one's steps.
-- A knob is one value, turned by the hand alone (a drag or the wheel on
-- it): each voice has its own level knob, so picking another voice moves
-- none. The game sets the sound from the knobs (music:mix, music.tempo,
-- music:volume), never the knobs from the sound.

local ROWS = {
  { name = "kick", channel = "K" },
  { name = "snare", channel = "S" },
  { name = "hat", channel = "H" },
}

-- The tempo knob's 0 to 1 is 90 to 180 beats a minute; the song file's
-- #tempo (124) is where it starts.
local SLOWEST, FASTEST, FILE_TEMPO = 90, 180, 124

-- The parts one plays, by their names in machine.vvx.
local KNOBS = { "knob_kick", "knob_snare", "knob_hat", "knob_tempo", "knob_volume" }
local CONTROLS = { pick_kick = true, pick_snare = true, pick_hat = true, pattern_a = true, pattern_b = true, start = true }
for _, knob in ipairs(KNOBS) do CONTROLS[knob] = true end
for i = 1, 16 do CONTROLS["step_" .. i] = true end

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
  -- tempo, the volume.
  world.state.knobs = { knob_kick = 0.8, knob_snare = 0.8, knob_hat = 0.8,
    knob_tempo = (FILE_TEMPO - SLOWEST) / (FASTEST - SLOWEST), knob_volume = 1 }
end

-- The pattern shown as channels of MML, a row each: its instrument,
-- sixteenths (the level is the channel's fader, music:mix).
local function channels()
  local s, out = world.state, {}
  for r, row in ipairs(ROWS) do
    local notes = { "@" .. row.name .. " o4 l16" }
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

-- The sound as the knobs stand (a global, for the scenarios): each voice's
-- fader, the tempo and the volume.
function apply_knobs(seconds)
  local k = world.state.knobs
  for _, row in ipairs(ROWS) do
    music:mix(row.channel, { volume = k["knob_" .. row.name] }, seconds or 0.02)
  end
  music.tempo = SLOWEST + k.knob_tempo * (FASTEST - SLOWEST)
  music:volume(k.knob_volume, seconds or 0.05)
end

-- Starts or stops the machine (a global, for the scenarios): stopped, the
-- music is stopped; started, the kit begins anew at its first step.
function start_stop()
  local s = world.state
  s.running = not s.running
  if s.running then
    s.playing = s.shown
    music:play("kit", { from = "now", channels = channels() })
  else
    music:stop()
  end
end

function on_level_start(how)
  world.state.playing = world.state.shown
  apply_knobs(0)
  if world.state.running then lay_pattern() else music:stop() end
end

function on_beat(beat)
  if beat % 4 == 0 then world.state.playing = world.state.shown end
end

-- A knob turned by the hand: its value, and what it changes.
local function turn(knob, by)
  local s = world.state
  s.knobs[knob] = math.max(0, math.min(1, s.knobs[knob] + by))
  apply_knobs()
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

-- The machine's pose by its state: knobs turned, the button held down.
local function pose()
  local s = world.state
  local parts = {}
  for _, knob in ipairs(KNOBS) do
    parts[knob] = { rotation = (s.knobs[knob] - 0.5) * 270 }
  end
  if s.held then parts[s.held] = { offset = { 0, 0, -1 } } end
  return parts
end

-- The lamps, each lit (glow 1) or out (glow 0): only for the eye.
local function lamps()
  local s = world.state
  local parts = {}
  local function lamp(name, lit)
    parts[name] = { glow = lit and 1 or 0 }
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
  return parts
end

-- The part a pointer is over, as a control: a lamp is its button's.
local LAMPS = { led_kick = "pick_kick", led_snare = "pick_snare", led_hat = "pick_hat",
  led_a = "pattern_a", led_b = "pattern_b", led_start = "start" }
local function control(p)
  local part = p and p.thing and p.thing.name == "machine" and p.part
  if not part then return nil end
  local n = part:match("^led_(%d+)$")
  return n and "step_" .. n or LAMPS[part] or (CONTROLS[part] and part)
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

function on_draw(frame)
  local machine = level:thing("machine")
  frame:pose(machine, lamps())
  -- The part under the pointer, else the keys' cursor, lit a little (lit,
  -- not moved: frame.pointer is of the picture before, and a part moved
  -- could slip out from under it). A black knob shines only in its white
  -- line: fully.
  local mark = control(frame.pointer) or "step_" .. world.state.step
  frame:pose(machine, { [mark] = { glow = mark:match("^knob_") and 1 or 0.5 } })
  frame:text("ARROWS {toggle} STEP  {pattern} PATTERN  {start} START", 4, frame.height - 9, "#8a8690")
end

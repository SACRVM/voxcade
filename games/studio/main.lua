-- Studio: the HARALD RG-80, a drum machine one plays through its model.
-- machine.vvx is a figure of parts (tools/gen_studio.py made it; the
-- editor shapes it now): a click names the part under the pointer
-- (input.pointer.part), and the machine answers on itself, posed by its
-- state each tick (a button held sinks a voxel, a knob or the selector
-- turns, a switch flips) and for each picture (a lamp lit or out, the
-- part under the pointer lit a little: frame:pose).
--
-- A column a voice, as the classic machine has them: its LEVEL knob and
-- the knobs that shape it, or a switch to its other instrument (the toms
-- to congas, the rim shot to claves, the clap to maracas). Each knob is
-- one value, turned by the hand alone (a drag or the wheel on it): the
-- game sets the sound from the knobs (music:mix on the voice's channels,
-- music.tempo, music:volume), never the knobs from the sound. The voice
-- selector chooses whose sixteen steps the buttons show and edit; its
-- first position is ACCENT, a row of steps on which every voice plays
-- louder. The voices are the instruments of music/rg80.mml (the library's
-- kit), each row of steps a channel of MML laid over them.

-- The kit's own numbers, read from its file (an instrument's own line, its
-- first layer's): a knob in its middle plays the instrument as the kit
-- has it.
local KIT = {}
for name, words in ("\n" .. world:read("music/rg80.mml")):gmatch("\n@(%S+)([^\n]*)") do
  KIT[name] = {}
  for key, value in words:gmatch("([%w_]+)=([%d.]+)") do KIT[name][key] = tonumber(value) end
end

-- The selector's positions, left to right as the columns: each voice's
-- channels by letter and the instrument each plays, and with its switch
-- flipped the other (a letter the other leaves out rests).
local VOICES = {
  { code = "ac" },
  { code = "bd", plays = { K = "kick" } },
  { code = "sd", plays = { S = "snare", N = "snare_noise" } },
  { code = "lt", plays = { L = "tom_low" }, other = { L = "conga_low" } },
  { code = "mt", plays = { M = "tom_mid" }, other = { M = "conga_mid" } },
  { code = "ht", plays = { T = "tom_high" }, other = { T = "conga_high" } },
  { code = "rs", plays = { R = "rim", r = "rim_low" }, other = { R = "clave" } },
  { code = "cp", plays = { C = "clap" }, other = { C = "maracas" } },
  { code = "cb", plays = { B = "cowbell", b = "cowbell_low" } },
  { code = "cy", plays = { Y = "cymbal" } },
  { code = "oh", plays = { O = "open_hat" } },
  { code = "ch", plays = { H = "hat" } },
}
local CLOSED = #VOICES

-- A note's volume (v, of 15) on a step without and with the accent: the
-- ACCENT knob adds up to 5.
local SOFT, ACCENT = 10, 5

-- The tempo knob's 0 to 1 is 90 to 180 beats a minute.
local SLOWEST, FASTEST = 90, 180

-- The selector's positions, from the front left round to the front
-- right, clockwise seen from above (the dots painted round it).
local SWEEP = 300

local function steps(picture)
  local row = {}
  for i = 1, 16 do row[i] = picture:sub(i, i) == "x" end
  return row
end

-- Rows of steps by the voices' order, the accent first.
local function pattern(rows)
  local out = {}
  for r, v in ipairs(VOICES) do out[r] = steps(rows[v.code] or "................") end
  return out
end

function on_world_start()
  world.state.patterns = {
    pattern({ ac = "x.......x.......", bd = "x...x...x...x...", sd = "....x.......x...",
              oh = "..............x.", ch = "x.x.x.x.x.x.x..." }),
    pattern({ ac = "x..x......x.....", bd = "x..x..x...x.x...", sd = "....x..x....x..x",
              lt = "..........x.....", mt = "............x...", cp = "....x.......x...",
              ch = "x.x.x.x.x.x.x.xx" }),
  }
  world.state.shown = 1      -- the pattern edited, and asked for
  world.state.playing = 1    -- the pattern the music plays
  world.state.row = 2        -- the selector's position: the voice edited
  world.state.step = 1       -- the keys' cursor
  world.state.running = true
  world.state.other = {}     -- the switches flipped, by the voice's code
  -- The knobs by their parts' names, 0 to 1.
  local knobs = { ac_level = 0.6, bd_tone = 0.25, bd_decay = 0.5, sd_tone = 0.5, sd_snappy = 0.7,
    lt_tuning = 0.5, mt_tuning = 0.5, ht_tuning = 0.5, cy_tone = 0.5, cy_decay = 0.5, oh_decay = 0.5,
    tempo = (124 - SLOWEST) / (FASTEST - SLOWEST), volume = 1 }
  for r = 2, #VOICES do knobs[VOICES[r].code .. "_level"] = 0.8 end
  world.state.knobs = knobs
end

-- The instruments a voice plays now, by letter.
local function instruments(v)
  return world.state.other[v.code] and v.other or v.plays
end

-- The pattern shown as channels of MML, one a letter, sixteenths: a step
-- set plays its instrument, louder on a step of the accent's. The open
-- hat's channel takes a silent closed hat where only the closed one
-- plays: a new note stops the one before, as the closed hat stops the
-- open one.
local function channels()
  local s = world.state
  local rows, out = s.patterns[s.shown], {}
  local loud = SOFT + math.floor(ACCENT * s.knobs.ac_level + 0.5)
  for r = 2, #VOICES do
    local v = VOICES[r]
    local play = instruments(v)
    for letter in pairs(v.plays) do
      local notes = { "o4 l16" }
      for i = 1, 16 do
        if play[letter] and rows[r][i] then
          notes[i + 1] = "@" .. play[letter] .. " v" .. (rows[1][i] and loud or SOFT) .. " c"
        elseif letter == "O" and rows[CLOSED][i] then
          notes[i + 1] = "@hat v0 c"
        else
          notes[i + 1] = "r"
        end
      end
      out[letter] = table.concat(notes, " ")
    end
  end
  return out
end

-- The pattern shown, laid: at once when it is the one playing, else
-- from the next bar. Stopped, nothing is laid (start lays it).
function lay_pattern()
  local s = world.state
  if not s.running then return end
  music:play("rg80", { from = s.shown == s.playing and "now" or "bar", channels = channels() })
end

-- A knob's value between base / factor (0) and base * factor (1), the
-- base in its middle.
local function around(base, factor, value)
  return base * math.pow(factor, 2 * value - 1)
end

-- A decay knob: the instrument's decay scaled, its release as the kit
-- sets it (the same slope on to silence after a sixteenth).
local function decay(instrument, factor, value, words)
  words.decay = around(KIT[instrument].decay, factor, value)
  words.release = math.max(0.005, words.decay - 0.105)
  return words
end

-- The sound as the knobs and switches stand (a global, for the
-- scenarios), gliding there over `seconds`.
function apply_knobs(seconds)
  local s, k = world.state, world.state.knobs
  local function strip(letter, words) music:mix(letter, words, seconds or 0.02) end
  for r = 2, #VOICES do
    for letter in pairs(VOICES[r].plays) do strip(letter, { volume = k[VOICES[r].code .. "_level"] }) end
  end
  -- The bass drum falls exponentially (curve=exp): its release the same
  -- fall as its decay, which goes on as it went; its drive the sum's of
  -- its layers (the kit's = line states one).
  local bd = around(KIT.kick.decay, 4, k.bd_decay)
  strip("K", { decay = bd, release = bd, drive = 12 * k.bd_tone })
  strip("S", { cutoff = around(KIT.snare.cutoff, 2, k.sd_tone) })
  strip("N", { volume = k.sd_level * k.sd_snappy })
  for r = 4, 6 do
    local letter, instrument = next(instruments(VOICES[r]))
    local tuning = k[VOICES[r].code .. "_tuning"]
    strip(letter, { freq = KIT[instrument].freq * math.pow(2, (tuning - 0.5) * 8 / 12) })
  end
  strip("Y", decay("cymbal", 2, k.cy_decay, { cutoff = around(KIT.cymbal.cutoff, 1.6, k.cy_tone) }))
  strip("O", decay("open_hat", 2.5, k.oh_decay, {}))
  local tempo = SLOWEST + k.tempo * (FASTEST - SLOWEST)
  if not music.tempo or math.abs(music.tempo - tempo) > 0.001 then music.tempo = tempo end
  music:volume(k.volume, seconds or 0.05)
end

-- Starts or stops the machine (a global, for the scenarios): stopped, the
-- music is stopped; started, the kit begins anew at its first step.
function start_stop()
  local s = world.state
  s.running = not s.running
  if s.running then
    s.playing = s.shown
    music:play("rg80", { from = "now", channels = channels() })
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
  local value = math.max(0, math.min(1, s.knobs[knob] + by))
  if value == s.knobs[knob] then return end
  s.knobs[knob] = value
  apply_knobs()
  if knob == "ac_level" then lay_pattern() end
end

-- The selector turned to a position (clicked, dragged round or wheeled).
local function select(row)
  world.state.row = math.max(1, math.min(#VOICES, row))
end

-- The selector's position nearest where the pointer is round its dial
-- (its panel's u and v, on_draw), nil at its very middle.
local function position_at(p)
  local dx, dy = p.u - 0.5, 0.5 - p.v
  if dx * dx + dy * dy < 0.01 then return nil end
  local a = math.deg(math.atan(dx, dy))
  return math.floor((a + SWEEP / 2) / (SWEEP / (#VOICES - 1)) + 0.5) + 1
end

-- What pressing a part does (the mouse's click, or the keys').
local function press(part)
  local s = world.state
  local n = part:match("^step_(%d+)$")
  local code = part:match("^(%a+)_switch$")
  if n then
    local row = s.patterns[s.shown][s.row]
    s.step = tonumber(n)
    row[s.step] = not row[s.step]
    lay_pattern()
  elseif code then
    s.other[code] = not s.other[code] or nil
    apply_knobs()
    lay_pattern()
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

-- The machine's pose by its state: knobs and the selector turned,
-- switches flipped, the button held down.
local function pose()
  local s = world.state
  local parts = {}
  for knob, value in pairs(s.knobs) do parts[knob] = { rotation = (value - 0.5) * 270 } end
  parts.selector = { rotation = -SWEEP / 2 + (s.row - 1) * SWEEP / (#VOICES - 1) }
  for _, v in ipairs(VOICES) do
    if v.other then parts[v.code .. "_switch"] = { roll = s.other[v.code] and 35 or -35 } end
  end
  if s.held then parts[s.held] = { offset = { 0, 0, -1 } } end
  return parts
end

-- The lamps, each lit (standing in its socket, glow 1) or out (sunk into
-- the case, so its dark socket shows): only for the eye.
local function lamps()
  local s = world.state
  local parts = {}
  local function lamp(name, lit)
    parts[name] = lit and { glow = 1 } or { glow = 0, offset = { 0, 0, -3 } }
  end
  local light, live = sounding(), s.shown == s.playing
  for i = 1, 16 do
    -- A set step lit; the running light turns the step it passes over.
    local on = s.patterns[s.shown][s.row][i]
    if live and i == light then on = not on end
    lamp("led_" .. i, on)
  end
  for r, v in ipairs(VOICES) do lamp("led_" .. v.code, r == s.row) end
  -- The pattern shown lit; the one still playing blinks until the bar.
  local blink = music.position and math.floor(music.position * 2) % 2 == 0
  for p, name in ipairs({ "a", "b" }) do
    lamp("led_" .. name, p == s.shown or (p == s.playing and blink))
  end
  lamp("led_start", s.running)
  return parts
end

-- The part a pointer is over, as a control: a lamp is its button's.
local LAMPS = { led_a = "pattern_a", led_b = "pattern_b", led_start = "start" }
local function control(p)
  if p and p.hud == "selector" then return "selector" end
  local part = p and p.thing and p.thing.name == "machine" and p.part
  if not part then return nil end
  local n = part:match("^led_(%d+)$")
  if n then return "step_" .. n end
  if LAMPS[part] or world.state.knobs[part] or part == "selector" or part:match("_switch$")
    or part:match("^step_") or part:match("^pattern_") or part == "start" then
    return LAMPS[part] or part
  end
end

function on_tick()
  local s = world.state
  local p = input.pointer
  local over = control(p)

  -- The mouse: a press on a part, a knob dragged up or down, the
  -- selector dragged round, either wheeled.
  if input:pressed("click") and over then
    if s.knobs[over] then
      s.drag = { knob = over, y = p.y, from = s.knobs[over] }
    elseif over == "selector" then
      s.drag = { selector = true }
    else
      s.held = over
      press(over)
    end
  end
  if s.drag then
    if not input:held("click") or not p then
      s.drag = nil
    elseif s.drag.knob then
      turn(s.drag.knob, s.drag.from + (s.drag.y - p.y) / 40 - s.knobs[s.drag.knob])
    elseif p.hud == "selector" then
      local row = position_at(p)
      if row then select(row) end
    end
  end
  if over and s.knobs[over] then
    if input:pressed("turn_up") then turn(over, 1 / 15) end
    if input:pressed("turn_down") then turn(over, -1 / 15) end
  elseif over == "selector" then
    if input:pressed("turn_up") then select(s.row + 1) end
    if input:pressed("turn_down") then select(s.row - 1) end
  end
  if s.held and not input:held("click") and not input:held("toggle") then s.held = nil end

  -- The keys: the cursor along the steps, up and down the selector.
  if input:repeated("left") then s.step = (s.step - 2) % 16 + 1 end
  if input:repeated("right") then s.step = s.step % 16 + 1 end
  if input:repeated("up") then select(s.row - 1) end
  if input:repeated("down") then select(s.row + 1) end
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
  -- The selector's dial as a panel of the HUD, twice its size about its
  -- middle: its u and v say where round the dial the pointer is (the
  -- pointer names the part it is over, but not where on it).
  local dial = machine:part("selector")
  local x0, y0 = frame:to_screen({ x = dial.low.x, y = dial.low.y, z = dial.high.z })
  local x1, y1 = frame:to_screen({ x = dial.high.x, y = dial.high.y, z = dial.high.z })
  if x0 and x1 then
    local w, h = x1 - x0, y0 - y1
    frame:hud("selector", x0 - w / 2, y1 - h / 2, w * 2, h * 2)
  end
  -- The part under the pointer, else the keys' cursor, lit a little (lit,
  -- not moved: frame.pointer is of the picture before, and a part moved
  -- could slip out from under it). A black knob shines only in its white
  -- line: fully.
  local mark = control(frame.pointer) or "step_" .. world.state.step
  local black = world.state.knobs[mark] or mark == "selector"
  frame:pose(machine, { [mark] = { glow = black and 1 or 0.5 } })
  frame:text("ARROWS {toggle} STEP  {pattern} PATTERN  {start} START", 4, frame.height - 9, "#8a8690")
end

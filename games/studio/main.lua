-- Studio: the HARALD RG-80, a drum machine one plays through its model,
-- in fine voxels, the whole machine on the screen at once
-- (tools/gen_studio.py made machine.vvx; the editor shapes it now). Its
-- signs are two or three letters; the screen's bottom edge names the part
-- under the pointer (or under the keys' cursor): its full name, its value
-- with its unit, its MML word.
--
-- machine.vvx is a figure of parts: a click names the part under the
-- pointer (input.pointer.part), and the machine answers on itself, posed
-- by its state each tick (a key held sinks a voxel, a knob or the selector
-- turns, a toggle switch tips, START shows play or stop) and for each
-- picture (a lamp lit or out, the tempo's display, the part under the
-- pointer marked: frame:pose).
--
-- A column a voice, as the classic machine has them: its lamp (lit by the
-- voice's hits), its LVL knob and the knobs that shape it, or a switch to
-- its other instrument (the toms to congas, the rim shot to claves, the
-- clap to maracas). Each knob is one value, turned by the hand alone (a
-- drag or the wheel on it): the game sets the sound from the knobs
-- (music:mix on the voice's channels, music.tempo, music:volume), never the
-- knobs from the sound. The masters stand in the top strip: ACC (how much
-- louder an accented step is), BPM (the tempo, its display beside the
-- knob), VOL. The voice selector chooses whose sixteen steps the keys show
-- and edit, its ring's lamp showing which; its first position is ACC, a
-- row of steps on which every voice plays louder. SOLO under it mutes
-- every other voice (music:mix's mute) while it is on. The voices are the
-- instruments of music/rg80.mml (the library's kit), each row of steps a
-- channel of MML laid over them.

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
-- flipped the other (a letter the other leaves out rests); the names the
-- bottom line says.
local VOICES = {
  { code = "ac", name = "ACCENT" },
  { code = "bd", name = "BASS DRUM", plays = { K = "kick" } },
  { code = "sd", name = "SNARE DRUM", plays = { S = "snare", N = "snare_noise" } },
  { code = "lt", name = "LOW TOM", plays = { L = "tom_low" }, other = { L = "conga_low" }, other_name = "LOW CONGA" },
  { code = "mt", name = "MID TOM", plays = { M = "tom_mid" }, other = { M = "conga_mid" }, other_name = "MID CONGA" },
  { code = "ht", name = "HIGH TOM", plays = { T = "tom_high" }, other = { T = "conga_high" }, other_name = "HIGH CONGA" },
  { code = "rs", name = "RIM SHOT", plays = { R = "rim", r = "rim_low" }, other = { R = "clave" }, other_name = "CLAVES" },
  { code = "cp", name = "HAND CLAP", plays = { C = "clap" }, other = { C = "maracas" }, other_name = "MARACAS" },
  { code = "cb", name = "COWBELL", plays = { B = "cowbell", b = "cowbell_low" } },
  { code = "cy", name = "CYMBAL", plays = { Y = "cymbal" } },
  { code = "oh", name = "OPEN HI-HAT", plays = { O = "open_hat" } },
  { code = "ch", name = "CLOSED HI-HAT", plays = { H = "hat" } },
}
local CLOSED = #VOICES
local BY_CODE = {}
for _, v in ipairs(VOICES) do BY_CODE[v.code] = v end

-- A note's volume (v, of 15) on a step without and with the accent: the
-- ACCENT knob adds up to 5.
local SOFT, ACCENT = 10, 5

-- The tempo knob's 0 to 1 is 90 to 180 beats a minute.
local SLOWEST, FASTEST = 90, 180

-- The selector's positions, from the front left round to the front
-- right, clockwise seen from above (the dots painted round it).
local SWEEP = 300

-- A toggle's lever leans this far left (this voice) or right (the other).
local LEAN = 40

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
  world.state.solo = false   -- only the voice the selector names sounds
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

-- A note's volume on a step of the accent's.
local function loud()
  return SOFT + math.floor(ACCENT * world.state.knobs.ac_level + 0.5)
end

-- The pattern shown as channels of MML, one a letter, sixteenths: a step
-- set plays its instrument, louder on a step of the accent's. The open
-- hat's channel takes a silent closed hat where only the closed one
-- plays: a new note stops the one before, as the closed hat stops the
-- open one.
local function channels()
  local s = world.state
  local rows, out = s.patterns[s.shown], {}
  local strong = loud()
  for r = 2, #VOICES do
    local v = VOICES[r]
    local play = instruments(v)
    for letter in pairs(v.plays) do
      local notes = { "o4 l16" }
      for i = 1, 16 do
        if play[letter] and rows[r][i] then
          notes[i + 1] = "@" .. play[letter] .. " v" .. (rows[1][i] and strong or SOFT) .. " c"
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

-- What the shaping knobs set, as the numbers the MML words take: the
-- sound and the bottom line read the same.
local function decay_of(instrument, factor, value)
  return around(KIT[instrument].decay, factor, value)
end
local function tuned(v, tuning)
  local _, instrument = next(instruments(v))
  return KIT[instrument].freq * math.pow(2, (tuning - 0.5) * 8 / 12)
end
local function tempo_of(value)
  return SLOWEST + value * (FASTEST - SLOWEST)
end

-- A decay knob: the instrument's decay scaled, its release as the kit
-- sets it (the same slope on to silence after a sixteenth).
local function decay(instrument, factor, value, words)
  words.decay = decay_of(instrument, factor, value)
  words.release = math.max(0.005, words.decay - 0.105)
  return words
end

-- The sound as the knobs and switches stand (a global, for the
-- scenarios), gliding there over `seconds`.
function apply_knobs(seconds)
  local k = world.state.knobs
  local function strip(letter, words) music:mix(letter, words, seconds or 0.02) end
  for r = 2, #VOICES do
    for letter in pairs(VOICES[r].plays) do strip(letter, { volume = k[VOICES[r].code .. "_level"] }) end
  end
  strip("K", decay("kick", 4, k.bd_decay, { drive = 12 * k.bd_tone }))
  strip("S", { cutoff = around(KIT.snare.cutoff, 2, k.sd_tone) })
  strip("N", { volume = k.sd_level * k.sd_snappy })
  for r = 4, 6 do
    local letter = next(instruments(VOICES[r]))
    strip(letter, { freq = tuned(VOICES[r], k[VOICES[r].code .. "_tuning"]) })
  end
  strip("Y", decay("cymbal", 2, k.cy_decay, { cutoff = around(KIT.cymbal.cutoff, 1.6, k.cy_tone) }))
  strip("O", decay("open_hat", 2.5, k.oh_decay, {}))
  local tempo = tempo_of(k.tempo)
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

-- SOLO (a global, for the scenarios): while it is on, every voice but the
-- one the selector names is muted, all of its channels; on ACC (no voice)
-- nothing is. It follows the selector, so it is applied again whenever
-- either changes. A mute fades over 20 ms: no click.
function apply_solo()
  local s = world.state
  for r = 2, #VOICES do
    local muted = s.solo and s.row > 1 and r ~= s.row
    for letter in pairs(VOICES[r].plays) do music:mix(letter, { mute = muted }, 0.02) end
  end
end

function on_level_start(how)
  world.state.playing = world.state.shown
  apply_knobs(0)
  apply_solo()
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
  row = math.max(1, math.min(#VOICES, row))
  if row == world.state.row then return end
  world.state.row = row
  if world.state.solo then apply_solo() end
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
  elseif part == "solo" then
    s.solo = not s.solo
    apply_solo()
  end
end

-- The step sounding now (nil while stopped).
local function sounding()
  if world.state.running and music.position then
    return math.floor(music.position * 4) % 16 + 1
  end
end

-- How long a column's lamp stays lit after its voice's hit, in seconds
-- (about 5 ticks).
local HIT = 0.09

-- The seconds since the step now sounding began, and that step (nil while
-- stopped): from the music's clock, as the running light.
function hit_time()
  local step = sounding()
  if not step then return nil end
  local sixteenths = music.position * 4
  local seconds = 15 / tempo_of(world.state.knobs.tempo)
  return (sixteenths - math.floor(sixteenths)) * seconds, step
end

-- The machine's pose by its state: knobs and the selector turned,
-- switches flipped, the button held down, START's sign.
local function pose()
  local s = world.state
  local parts = {}
  for knob, value in pairs(s.knobs) do parts[knob] = { rotation = (value - 0.5) * 270 } end
  parts.selector = { rotation = -SWEEP / 2 + (s.row - 1) * SWEEP / (#VOICES - 1) }
  for _, v in ipairs(VOICES) do
    if v.other then parts[v.code .. "_switch"] = { roll = s.other[v.code] and LEAN or -LEAN } end
  end
  if s.held then parts[s.held] = { offset = { 0, 0, -1 } } end
  -- START shows play while stopped and stop while it runs: the stop sign
  -- rests inside the button and rises onto it, the play sign sinks in.
  if s.running then
    parts.start_play = { offset = { 0, 0, -3 } }
    parts.start_stop = { offset = { 0, 0, 3 } }
  end
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
  -- The selector's ring: the voice the keys edit. The columns' lamps: a
  -- voice's hits, lit as its step begins, fading over HIT seconds (an
  -- accented hit brighter).
  local since, step = hit_time()
  local rows = s.patterns[s.playing]
  for r, v in ipairs(VOICES) do
    lamp("sel_" .. r, r == s.row)
    if r > 1 then
      if since and since < HIT and rows[r][step] then
        local fade = 1 - since / HIT
        parts["led_" .. v.code] = { glow = (rows[1][step] and 0.55 or 0.3) + (rows[1][step] and 0.45 or 0.4) * fade }
      else
        lamp("led_" .. v.code, false)
      end
    end
  end
  -- The pattern shown lit; the one still playing blinks until the bar.
  local blink = music.position and math.floor(music.position * 2) % 2 == 0
  for p, name in ipairs({ "a", "b" }) do
    lamp("led_" .. name, p == s.shown or (p == s.playing and blink))
  end
  lamp("led_start", s.running)
  lamp("led_solo", s.solo)
  return parts
end

-- The part a pointer is over, as a control: a lamp is its button's, a
-- sign on START is START, a knob's pointer line and the flush plate round
-- a knob or a switch are the knob's or the switch's.
local LAMPS = { led_a = "pattern_a", led_b = "pattern_b", led_start = "start",
  start_play = "start", start_stop = "start", led_solo = "solo" }
local function control(p)
  if p and p.hud == "selector" then return "selector" end
  local part = p and p.thing and p.thing.name == "machine" and p.part
  if not part then return nil end
  local n = part:match("^led_(%d+)$")
  if n then return "step_" .. n end
  local owner = part:match("^(.+)_ptr$") or part:match("^(.+)_base$") or part:match("^(.+)_mark$")
    or part:match("^(.+)_plate$") or part:match("^(.+)_mount$")
  if owner then
    if world.state.knobs[owner] or owner == "selector" or owner:match("^step_")
      or owner:match("^pattern_") or owner == "start" or owner == "solo" then return owner end
    return owner .. "_switch"
  end
  if part:match("^sel_%d+$") then return "selector" end
  if part == "display" or part:match("^seg_%d_%a$") or part:match("^lit_%d_%a$") then return "display" end
  if LAMPS[part] or world.state.knobs[part] or part == "selector" or part:match("_switch$")
    or part:match("^step_") or part:match("^pattern_") or part == "start" or part == "solo" then
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
    elseif over ~= "display" then
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
  if over and (s.knobs[over] or over == "display") then
    -- The wheel over the display turns the tempo, as over its knob.
    local knob = over == "display" and "tempo" or over
    if input:pressed("turn_up") then turn(knob, 1 / 15) end
    if input:pressed("turn_down") then turn(knob, -1 / 15) end
  elseif over == "selector" then
    if input:pressed("turn_up") then select(s.row + 1) end
    if input:pressed("turn_down") then select(s.row - 1) end
  end
  if s.held and not input:held("click") and not input:held("toggle") then s.held = nil end

  -- The keys: the cursor along the steps, up and down the selector. What
  -- they moved last is what the bottom line names until the pointer moves.
  if p and p.moved then s.keys = nil end
  if input:repeated("left") then s.step = (s.step - 2) % 16 + 1; s.keys = "step" end
  if input:repeated("right") then s.step = s.step % 16 + 1; s.keys = "step" end
  if input:repeated("up") then select(s.row - 1); s.keys = "selector" end
  if input:repeated("down") then select(s.row + 1); s.keys = "selector" end
  if input:pressed("toggle") then
    s.held = "step_" .. s.step
    s.keys = "step"
    press(s.held)
  end
  if input:pressed("pattern") then press(s.shown == 1 and "pattern_b" or "pattern_a") end
  if input:pressed("start") then press("start") end
  if input:pressed("solo") then press("solo") end

  level:thing("machine").state.parts = pose()
end

-- ------------------------------------------------- the line at the bottom

-- Each shaping knob: its name after the voice's, the MML word it sets and
-- its value with the unit, from the knob's 0 to 1.
local SHAPING = {
  bd_tone = { "TONE", "drive", function(k) return string.format("%.1f dB", 12 * k) end },
  bd_decay = { "DECAY", "decay", function(k) return string.format("%.2f s", decay_of("kick", 4, k)) end },
  sd_tone = { "TONE", "cutoff", function(k) return string.format("%d Hz", around(KIT.snare.cutoff, 2, k) // 1) end },
  sd_snappy = { "SNAPPY", "volume", function(k) return string.format("%.2f", world.state.knobs.sd_level * k) end },
  cy_tone = { "TONE", "cutoff", function(k) return string.format("%d Hz", around(KIT.cymbal.cutoff, 1.6, k) // 1) end },
  cy_decay = { "DECAY", "decay", function(k) return string.format("%.2f s", decay_of("cymbal", 2, k)) end },
  oh_decay = { "DECAY", "decay", function(k) return string.format("%.2f s", decay_of("open_hat", 2.5, k)) end },
}
for _, code in ipairs({ "lt", "mt", "ht" }) do
  SHAPING[code .. "_tuning"] = { "TUNING", "freq", function(k)
    return string.format("%.1f Hz", tuned(BY_CODE[code], k)) end }
end

-- The instruments a voice plays now, as MML names them (without the @
-- before them: the HUD's letters have none).
local function played(v)
  if not v.plays then return "v" .. loud() end
  local names = {}
  for _, instrument in pairs(instruments(v)) do names[#names + 1] = instrument end
  table.sort(names)
  return table.concat(names, " ")
end

-- A voice's name as it plays now (a tom switched to its conga).
local function voice_name(v)
  return (v.other and world.state.other[v.code]) and v.other_name or v.name
end

-- A voice's column lamp under the pointer, as "column_CODE" (it shows the
-- voice's hits; nothing to press).
function column_lamp(p)
  local part = p and p.hud ~= "selector" and p.thing and p.thing.name == "machine" and p.part
  local code = part and (part:match("^led_(%a+)$") or part:match("^(%a+)_head$"))
  if code and BY_CODE[code] and code ~= "ac" then return "column_" .. code end
end

-- What the bottom line says of a control: its full name, its value with
-- its unit, its MML word ("" where none).
local function describe(part)
  local s, k = world.state, world.state.knobs
  local column = part:match("^column_(%a+)$")
  if column then
    local v = BY_CODE[column]
    return voice_name(v), VOICES[s.row] == v and "SELECTED" or "", played(v)
  end
  local n = part:match("^step_(%d+)$")
  if n then
    local v = VOICES[s.row]
    local on = s.patterns[s.shown][s.row][tonumber(n)]
    local word = v.plays and (on and "c" or "r") or "v" .. (on and loud() or SOFT)
    return "STEP " .. n .. "  " .. voice_name(v), on and "ON" or "OFF", word
  end
  local code = part:match("^(%a+)_switch$")
  if code then
    local v = BY_CODE[code]
    return v.name .. " OR " .. v.other_name, voice_name(v), played(v)
  end
  if part == "selector" then return "VOICE", voice_name(VOICES[s.row]), played(VOICES[s.row]) end
  if part == "tempo" or part == "display" then return "TEMPO", string.format("%d BPM", tempo_of(k.tempo) // 1), "#tempo" end
  if part == "volume" then return "VOLUME", string.format("%.2f", k.volume), "" end
  if part == "ac_level" then return "ACCENT LEVEL", "v" .. loud(), "v" end
  if part == "solo" then
    if not s.solo then return "SOLO", "OFF", "mute" end
    return "SOLO", s.row > 1 and voice_name(VOICES[s.row]) or "EVERY VOICE", "mute"
  end
  if part == "start" then return "START / STOP", s.running and "PLAYING" or "STOPPED", "" end
  if part == "pattern_a" or part == "pattern_b" then
    local p = part == "pattern_a" and 1 or 2
    local said = {}
    if s.shown == p then said[#said + 1] = "EDITED" end
    if s.running and s.playing == p then said[#said + 1] = "PLAYING" end
    return "PATTERN " .. (p == 1 and "A" or "B"), table.concat(said, ", "), ""
  end
  local voice = part:match("^(%a+)_level$")
  if voice then return voice_name(BY_CODE[voice]) .. " LEVEL", string.format("%.2f", k[part]), "volume" end
  local shaping = SHAPING[part]
  if shaping then
    return voice_name(BY_CODE[part:match("^(%a+)_")]) .. " " .. shaping[1], shaping[3](k[part]), shaping[2]
  end
end

-- A digit's segments (a to g, clockwise from the top, g the middle).
local DIGITS = { [0] = "abcdef", "bc", "abdeg", "abcdg", "bcfg", "acdfg", "acdefg", "abc", "abcdefg", "abcdfg" }

-- The display: the tempo in whole beats a minute, its hundreds blank under
-- 100; a dark segment's bright part sinks under the window's floor, its
-- faint one showing.
local function display()
  local bpm = tempo_of(world.state.knobs.tempo) // 1
  local shown = { bpm >= 100 and bpm // 100 or nil, bpm // 10 % 10, bpm % 10 }
  local parts = {}
  for d = 1, 3 do
    local lit = shown[d] and DIGITS[shown[d]] or ""
    for seg in ("abcdefg"):gmatch(".") do
      if not lit:find(seg, 1, true) then parts["lit_" .. d .. "_" .. seg] = { offset = { 0, 0, -3 } } end
    end
  end
  return parts
end

function on_draw(frame)
  local machine = level:thing("machine")
  frame:pose(machine, lamps())
  frame:pose(machine, display())
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
  -- The part under the pointer, else what the keys moved last (until the
  -- pointer moves), marked (only a
  -- voxel moved: frame.pointer is of the picture before, and a part moved
  -- far could slip out from under it).
  local s = world.state
  local over = control(frame.pointer)
  -- A knob or the selector lights its pointer line and the plate round it;
  -- a key (a step, START, a pattern) shows a white bar in front of it; a
  -- switch lights its cap.
  local mark = over or (s.keys == "step" and "step_" .. s.step) or (s.keys == "selector" and "selector")
  if not mark then
    -- Nothing marked: the pointer over no control, the keys not used.
  elseif s.knobs[mark] then
    frame:pose(machine, { [mark .. "_ptr"] = { glow = 1 }, [mark .. "_base"] = { glow = 0.35 } })
  elseif mark == "selector" then
    frame:pose(machine, { selector_ptr = { glow = 1 } })
  elseif mark == "display" then
    -- A display: nothing to mark.
  elseif mark:match("_switch$") then
    frame:pose(machine, { [mark] = { glow = 0.6 } })
  else
    frame:pose(machine, { [mark .. "_mark"] = { offset = { 0, 0, 4 } } })
  end
  -- The bottom line: what the pointer is over, else what the keys moved
  -- last; empty otherwise.
  local named = over or (s.keys == "step" and "step_" .. s.step) or (s.keys == "selector" and "selector")
    or column_lamp(frame.pointer)
  local name, value, word
  if named then name, value, word = describe(named) end
  if name then
    local x, y = 6, frame.height - 10
    frame:text(name, x, y, "#e8e4d8")
    x = x + frame:text_width(name) + 12
    if value ~= "" then
      frame:text(value, x, y, "#ff8a5a")
      x = x + frame:text_width(value) + 12
    end
    if word ~= "" then frame:text(word, x, y, "#9a96a0") end
  end
end

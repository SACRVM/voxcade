-- Drums: sixteen steps a bar, three rows, two patterns. Each row is
-- written as a channel of MML and laid over the instruments of
-- music/kit.mml (music:play's channels). The arrows move the cursor;
-- TOGGLE sets or clears a step, which sounds when the running light comes
-- to it; PATTERN switches to the other pattern at the next bar.

local ROWS = {
  { name = "KICK", channel = "K", instrument = "kick", color = "#e05a48" },
  { name = "SNARE", channel = "S", instrument = "snare", color = "#e0a040" },
  { name = "HAT", channel = "H", instrument = "hat", color = "#d8d070" },
}

-- A row of steps from a picture of it: x a hit, anything else a rest.
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
  world.state.shown = 1     -- the pattern edited, and asked for
  world.state.playing = 1   -- the pattern the music plays
  world.state.row, world.state.step = 1, 1
end

-- A row of a pattern as MML: its instrument, sixteenths, a hit or a rest.
local function written(pattern, r)
  local out = { "@" .. ROWS[r].instrument .. " o4 l16" }
  for i = 1, 16 do out[#out + 1] = world.state.patterns[pattern][r][i] and "c" or "r" end
  return table.concat(out, " ")
end

-- The pattern shown, laid (a global, for the scenarios): at once when it
-- is the one playing (a step edited sounds when the light comes to it),
-- else from the next bar.
function lay_pattern()
  local s = world.state
  local channels = {}
  for r, row in ipairs(ROWS) do channels[row.channel] = written(s.shown, r) end
  music:play("kit", { from = s.shown == s.playing and "now" or "bar", channels = channels })
end

function on_level_start(how)
  world.state.playing = world.state.shown
  lay_pattern()
end

-- A bar begins: the pattern asked for plays from here.
function on_beat(beat)
  if beat % 4 == 0 then world.state.playing = world.state.shown end
end

function on_tick()
  local s = world.state
  if input:repeated("left") then s.step = (s.step - 2) % 16 + 1 end
  if input:repeated("right") then s.step = s.step % 16 + 1 end
  if input:repeated("up") then s.row = (s.row - 2) % #ROWS + 1 end
  if input:repeated("down") then s.row = s.row % #ROWS + 1 end
  if input:pressed("toggle") then
    local row = s.patterns[s.shown][s.row]
    row[s.step] = not row[s.step]
    lay_pattern()
  end
  if input:pressed("pattern") then
    s.shown = 3 - s.shown
    lay_pattern()
  end
end

local CELL, GAP, GROUP = 9, 2, 4

-- Where step i's cell begins, from the grid's left.
local function across(i)
  return (i - 1) * (CELL + GAP) + math.floor((i - 1) / 4) * GROUP
end

function on_draw(frame)
  local s = world.state
  local width = across(16) + CELL + 40
  local x0 = math.floor((frame.width - width) / 2) + 40
  local y0 = math.floor(frame.height / 2) - 30
  -- The step sounding now, while the pattern shown plays.
  local light = music.position and math.floor(music.position * 4) % 16 + 1
  local live = s.shown == s.playing

  -- (What is drawn first lies on top: the cursor's frame after its cell.)
  local name = s.shown == 1 and "A" or "B"
  frame:text("PATTERN " .. name, x0 - 40, y0 - 22, "#f0e8d8")
  frame:text(live and "PLAYING" or "FROM THE NEXT BAR", x0 + 40, y0 - 22, live and "#80d080" or "#e0c060")
  for r, row in ipairs(ROWS) do
    local y = y0 + (r - 1) * (CELL + 5)
    frame:text(row.name, x0 - 40, y + 1, "#a0a0b0")
    for i = 1, 16 do
      local on = s.patterns[s.shown][r][i]
      local lit = live and i == light
      local color = on and (lit and "#ffffff" or row.color) or (lit and "#606070" or "#2a2a34")
      frame:box(x0 + across(i), y, CELL, CELL, color)
      if r == s.row and i == s.step then
        frame:box(x0 + across(i) - 1, y - 1, CELL + 2, CELL + 2, "#f0f0f0")
      end
    end
  end
  -- The running light under the grid, whichever pattern is shown.
  if light then
    frame:box(x0 + across(light), y0 + #ROWS * (CELL + 5), CELL, 2, "#ffd040")
  end
  frame:text("ARROWS MOVE  {toggle} STEP  {pattern} PATTERN", x0 - 40, y0 + #ROWS * (CELL + 5) + 12, "#8a8a98")
end

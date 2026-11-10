-- Pulsar's rules. The screen scrolls up the level by itself and the ship
-- with it: the arrows steer it across, push it ahead or let it fall back
-- (within a band, so it never sticks to one line), fire shoots, bomb
-- clears the enemies' shots. The enemies come in waves on the music's
-- bars, one after another on its beats, and fire on its beats: the game
-- goes with the song (music:play, on_beat). Two can fly: each ship its
-- own keys, shield, lives, bombs and score; the enemies aim at the nearest.

local SCROLL = 40          -- voxels a second the screen climbs
local FLY = 40             -- the height everything flies at
-- The field is 320 across, from EDGE in the level: the level goes on 80
-- to either side of it, under the panels, where enemies fly off and out.
local EDGE = 80
local MIDDLE = EDGE + 160    -- the field's middle across
local LEFT, RIGHT = EDGE + 22, EDGE + 298   -- the ship's middle; it is 33 across, and the panels close in low on the screen
local AHEAD, BACK = 70, -125  -- how far the ship may push up or fall back from the screen's middle (its bottom edge is at -161)
local SIDE, PUSH = 120, 70   -- its speeds across and up or down the screen
local EASE = 0.3             -- how quickly it reaches them (a share a tick)
local TOP = 265              -- where enemies come in, over the screen's middle (its top edge is at 239)
local SHOT_SPEED = 320
local COOL = 7               -- ticks between shots
local SAFE = 150             -- ticks nothing hurts the ship after it comes back
local SHIELD = 3             -- hits a ship takes; the last of them blows it up
local SHAKEN = 45            -- ticks nothing hurts it after its shield takes a hit
local ARM = 50               -- how near the ship sets a mine off
local FUSE = 40              -- ticks from then until it blows
local BURST = 40             -- how near its blast hurts the ship
local REACH = 270            -- how far a bomb's blast spreads from the ship
local SPREAD = 540           -- how fast it spreads (voxels a second)

-- The waves, by the song's bar they begin on (4 beats to a bar, a bar
-- every 2 seconds): what comes, along which path, how many, and every how
-- many beats the next one; xs across the field, from its left edge.
local WAVES = {
  { bar = 2, kind = "fighter", path = "line", xs = { 80, 120, 160, 200, 240 }, every = 1 },
  { bar = 4, kind = "mine", path = "mine", xs = { 70, 250, 160 }, every = 2 },
  { bar = 5, kind = "swooper", path = "swoop", from = "left", count = 6, every = 1 },
  { bar = 7, kind = "swooper", path = "swoop", from = "right", count = 6, every = 1 },
  { bar = 9, kind = "fighter", path = "snake", xs = { 107, 107, 107, 107, 107, 107 }, every = 1 },
  { bar = 11, kind = "fighter", path = "snake", xs = { 213, 213, 213, 213, 213, 213 }, every = 1 },
  { bar = 13, kind = "heavy", path = "hover", xs = { 160 }, every = 1 },
  { bar = 14, kind = "mine", path = "mine", xs = { 60, 260, 110, 210 }, every = 2 },
  { bar = 16, kind = "fighter", path = "vee", xs = { 160, 140, 180, 120, 200, 100, 220 }, every = 1 },
  { bar = 18, kind = "swooper", path = "swoop", from = "left", count = 8, every = 1 },
  { bar = 18, kind = "swooper", path = "swoop", from = "right", count = 8, every = 1 },
  { bar = 21, kind = "heavy", path = "hover", xs = { 93, 227 }, every = 4 },
  { bar = 24, kind = "fighter", path = "line", xs = { 53, 267, 93, 227, 133, 187, 160 }, every = 1 },
  { bar = 26, kind = "mine", path = "mine", xs = { 160, 90, 230, 50, 270 }, every = 1 },
  { bar = 27, kind = "fighter", path = "snake", xs = { 80, 240, 80, 240, 80, 240 }, every = 1 },
  { bar = 30, kind = "swooper", path = "swoop", from = "left", count = 10, every = 1 },
  { bar = 32, kind = "heavy", path = "hover", xs = { 160, 80, 240 }, every = 4 },
  { bar = 36, kind = "fighter", path = "vee", xs = { 160, 140, 180, 120, 200, 100, 220, 80, 240 }, every = 1 },
  { bar = 38, kind = "swooper", path = "swoop", from = "right", count = 10, every = 1 },
  { bar = 41, kind = "fighter", path = "line", xs = { 67, 253, 107, 213, 147, 173, 67, 253 }, every = 1 },
  { bar = 43, kind = "mine", path = "mine", xs = { 80, 240, 160, 120, 200 }, every = 2 },
  { bar = 44, kind = "heavy", path = "hover", xs = { 107, 213 }, every = 2 },
  { bar = 46, kind = "swooper", path = "swoop", from = "left", count = 8, every = 1 },
  { bar = 46, kind = "swooper", path = "swoop", from = "right", count = 8, every = 1 },
  { bar = 50, kind = "fighter", path = "snake", xs = { 80, 160, 240, 80, 160, 240, 80, 160, 240 }, every = 1 },
  { bar = 54, kind = "boss", path = "boss", xs = { 160 }, every = 1 },
}

-- --------------------------------------------------------------- the run

function on_world_start()
  player.state.score = 0
end

-- A ship as it starts: how it moves, its guns cooling, the ticks nothing
-- hurts it, the hits its shield still takes.
local function new_ship()
  return { vx = 0, vy = 0, cool = 0, safe = SAFE, shield = SHIELD }
end

function on_level_start(how)
  level.state.y = 90                 -- the screen's middle, climbing
  level.state.ships = {}             -- each player's ship, by its seat
  for _, p in ipairs(world.players) do
    if p then level.state.ships[p.seat] = new_ship() end
  end
  level.state.queue = {}
  level.state.flashes = {}
  level.state.message = { text = "STAGE 1", at = world.tick }
  music:play("pulsar_drive", "now")
  player:face({ x = 0, y = 1 })
end

-- A second player joins: its ship comes in low beside the screen's middle.
function on_player_join(p, how)
  level.state.ships[p.seat] = new_ship()
  p:place(MIDDLE + 40, level.state.y - 40, FLY)
  level.state.message = { text = "PLAYER " .. p.seat, at = world.tick }
end

-- Its ship's state.
local function ship(p)
  return level.state.ships[p.seat]
end

-- The ships flying (not blown up and waiting to come back).
local function flying()
  local up = {}
  for _, p in ipairs(level:players()) do
    if not ship(p).down then table.insert(up, p) end
  end
  return up
end

-- The camera rests over the screen's middle, climbing with it.
local function look()
  level.camera:look_at(MIDDLE, level.state.y)
end

-- Where a point is from the screen's middle, and the other way round.
local function on_screen(t)
  return t.y - level.state.y
end

-- A thing goes towards a point this tick (the things move as fast as they
-- are told: c.player.accel).
local function go_to(t, x, y)
  local dx, dy = x - t.x, y - t.y
  local d = math.sqrt(dx * dx + dy * dy)
  if d < 0.001 then
    t:walk({ x = 0, y = 0 }, 0)
  else
    t:walk({ x = dx / d, y = dy / d }, d * 60)
  end
end

-- --------------------------------------------------------------- the ship

local function fire(pl)
  local s = ship(pl)
  s.cool = COOL
  local p = pl.state.power
  local xs = p >= 3 and { -12, 0, 12 } or (p == 2 and { -6, 6 } or { 0 })
  for _, dx in ipairs(xs) do
    level:spawn("shot", pl.x + dx, pl.y + 18, { z = FLY + 4, state = { dx = p >= 3 and dx * 2 or 0, by = pl } })
  end
  level:sound("shot", pl)
  s.muzzle = world.tick
end

local function steer(p)
  local s = ship(p)
  if s.down then
    p:walk({ x = 0, y = 1 }, SCROLL)
    return
  end
  local dir = p.input.dir
  s.vx = s.vx + (dir.x * SIDE - s.vx) * EASE
  s.vy = s.vy + (dir.y * PUSH - s.vy) * EASE
  local ry = on_screen(p)
  if (ry >= AHEAD and s.vy > 0) or (ry <= BACK and s.vy < 0) then s.vy = 0 end
  if (p.x <= LEFT and s.vx < 0) or (p.x >= RIGHT and s.vx > 0) then s.vx = 0 end
  local vx, vy = s.vx, SCROLL + s.vy
  local v = math.sqrt(vx * vx + vy * vy)
  p:walk({ x = vx / v, y = vy / v }, v)
  p:face({ x = 0, y = 1 })
  s.cool = s.cool - 1
  if p.input:held("fire") and s.cool <= 0 then fire(p) end
  s.safe = math.max(0, s.safe - 1)
end

-- A bomb: a blast that spreads from the ship over nearly the whole
-- screen (it moves with the screen), clearing the bolts it passes and
-- tearing apart the enemies it reaches; a heavy too, the boss it hurts.
local function bomb(p)
  if p.state.bombs <= 0 or ship(p).down then return end
  p.state.bombs = p.state.bombs - 1
  level.state.blast = { x = p.x, ry = on_screen(p), at = world.tick, by = p }
  screen:flash(0.8)
  screen:shake(0.5)
  level:sound("bomb", p)
end

local function spread_blast()
  local b = level.state.blast
  if not b then return end
  local r = SPREAD * (world.tick - b.at) / 60
  if r > REACH then
    level.state.blast = nil
    return
  end
  local y = level.state.y + b.ry
  local function near(t, more)
    local dx, dy = t.x - b.x, t.y - y
    return dx * dx + dy * dy < (r + more) * (r + more)
  end
  for _, bolt in ipairs(level:things("bolt")) do
    if near(bolt, 0) then bolt:remove() end
  end
  for _, e in ipairs(level:things("enemies")) do
    if e.state.bombed ~= b.at and near(e, 10) then
      e.state.bombed = b.at
      e:hurt(e.kind == "boss" and 12 or 8, b.by)
    end
  end
end

-- A ship is hit: an explosion, a life less, back after a while.
local function lose_ship(p)
  local s = ship(p)
  if s.down or s.safe > 0 or world.state.over then return end
  s.down = world.tick
  level:effect("boom.vox", p, 14)
  level:explode({ x = p.x, y = p.y, z = FLY + 3 }, 13)
  world:hitstop(6)
  level:sound("die", p)
  table.insert(level.state.flashes, { x = p.x, y = p.y, at = world.tick, size = 60 })
  screen:shake(0.9)
  screen:flash(0.5)
  p.state.lives = p.state.lives - 1
  p.state.power = math.max(1, p.state.power - 1)
  for _, b in ipairs(level:things("bolt")) do b:remove() end
end

-- A ship is hit: its shield takes it, flaring up, and holds a moment
-- (the ship blinks); the last hit it does not take.
local function hurt_ship(p)
  local s = ship(p)
  if s.down or s.safe > 0 or world.state.over then return end
  s.shield = s.shield - 1
  if s.shield <= 0 then return lose_ship(p) end
  -- Every hit costs the guns a power level: the triple shot is kept by
  -- flying well.
  p.state.power = math.max(1, p.state.power - 1)
  s.safe = SHAKEN
  s.flared = world.tick
  world:hitstop(2)
  screen:shake(0.3)
  level:sound("shield", p)
end

-- A ship lost comes back after a while; with no ship left its player is
-- out, and with every player out the run is.
local function come_back(p)
  local s = ship(p)
  if not s.down or world.tick - s.down < 100 or world.state.over then return end
  if p.state.lives <= 0 then
    p.state.out = true
    if #level:players() == 0 then
      world.state.over = { result = "lost", at = world.tick }
      world:lose()
    end
    return
  end
  s.down = nil
  s.vx, s.vy, s.safe, s.shield = 0, 0, SAFE, SHIELD
  p:place(MIDDLE, level.state.y - 40, FLY)
end

-- --------------------------------------------------------------- enemies

-- Where an enemy wants to be, `t` seconds after it came, from the screen's
-- middle: x across the level, y up the screen.
local PATHS = {
  -- As a plane flies, at one speed: straight down at the ship, then a
  -- wide turn out to its own side and away up the screen.
  line = function(e, t)
    local x0, v, low, turn = e.state.x0, 160, 80, 70
    local side = x0 < MIDDLE and -1 or 1
    local d = v * t - (TOP - low)
    if d < 0 then return x0, low - d end
    local a = math.min(d / turn, 2 * math.pi / 3)
    local x, y = x0 + side * turn * (1 - math.cos(a)), low - turn * math.sin(a)
    local on = d - a * turn
    return x + side * on * math.sin(a), y - on * math.cos(a)
  end,
  snake = function(e, t) return e.state.x0 + 40 * math.sin(t * 2.1), TOP - 65 * t end,
  -- The vee down the screen, swaying gently from side to side as one.
  vee = function(e, t) return e.state.x0 + 30 * math.sin(t * 1.4), TOP - 65 * t end,
  swoop = function(e, t)
    local side = e.state.side
    return MIDDLE - side * 147 + side * 100 * t, TOP - 95 * t + 26 * t * t
  end,
  hover = function(e, t)
    local y = math.max(70, TOP - 30 * t)
    if t > 14 then y = 70 - 40 * (t - 14) end
    return e.state.x0 + 35 * math.sin(t * 0.9), y
  end,
  boss = function(e, t)
    return MIDDLE + 80 * math.sin(t * 0.45), math.max(120, TOP + 40 - 24 * t)
  end,
  -- Where it was laid: the screen climbs past it.
  mine = function(e, t) return e.state.x0, TOP - SCROLL * t end,
}

local function spawn(entry)
  local x = EDGE + (entry.x or 160)
  local e = level:spawn(entry.kind, x, level.state.y + TOP, { z = FLY, state = {
    path = entry.path, born = world.tick, x0 = x, side = entry.from == "right" and -1 or 1,
  } })
  if entry.kind == "boss" then
    level.state.boss = e
    level.state.boss_hp = e.state.hp
    level.state.message = { text = "WARNING", at = world.tick }
  end
end

-- A wave begins on its bar: its members come one after another on the
-- beats that follow.
local function begin_waves(beat, bar)
  for _, w in ipairs(WAVES) do
    if w.bar == bar then
      local n = w.xs and #w.xs or w.count
      for i = 1, n do
        table.insert(level.state.queue, {
          beat = beat + (i - 1) * w.every, kind = w.kind, path = w.path, from = w.from,
          x = w.xs and w.xs[i] or nil,
        })
      end
    end
  end
end

-- Those that turn freely face the way they fly over the screen, the
-- others down it.
local TURNS = { fighter = true, swooper = true }

local function move_enemies()
  for _, e in ipairs(level:things("enemies")) do
    local path = PATHS[e.state.path]
    local t = (world.tick - e.state.born) / 60
    local x, y = path(e, t)
    go_to(e, x, level.state.y + y)
    if TURNS[e.kind] then
      -- Its nose the way it flies over the screen.
      local x1, y1 = path(e, t + 0.05)
      if (x1 - x) ^ 2 + (y1 - y) ^ 2 > 0.0001 then e:face({ x = x1 - x, y = y1 - y }) end
    else
      e:face({ x = 0, y = -1 })
    end
    if y < -190 or y > TOP + 80 or x < EDGE - 60 or x > EDGE + 380 then e:remove() end
  end
end

-- A mine the ship comes near is set off and blows after its fuse
-- (on_death: its blast).
local function mines()
  for _, m in ipairs(level:things("mine")) do
    for _, p in ipairs(flying()) do
      local dx, dy = m.x - p.x, m.y - p.y
      if not m.state.armed and dx * dx + dy * dy < ARM * ARM then
        m.state.armed = world.tick
        level:sound("arm", m)
      end
    end
    if m.state.armed and world.tick - m.state.armed >= FUSE then
      m.state.fused = true
      m:hurt(m.state.hp)
    end
  end
end

-- The ship flying nearest to a point, if one flies.
local function nearest(x, y)
  local best, far
  for _, p in ipairs(flying()) do
    local d = (p.x - x) ^ 2 + (p.y - y) ^ 2
    if not far or d < far then best, far = p, d end
  end
  return best
end

-- A bolt from an enemy towards the nearest ship (turned by `turn`
-- radians), from its front or from `x, y`.
local function bolt(e, turn, speed, x, y)
  x, y = x or e.x, y or e.y - 12
  local target = nearest(x, y)
  if not target then return end
  local dx, dy = target.x - x, target.y - y
  local a = math.atan(dy, dx) + (turn or 0)
  local v = speed or 70
  level:spawn("bolt", x, y, { z = FLY + 3, state = { vx = v * math.cos(a), vy = v * math.sin(a) } })
end

-- The boss's four turrets, where their barrels end (it faces the ship).
local GUNS = { { -26, -16 }, { 26, -16 }, { -15, -29 }, { 15, -29 } }

-- On its beats every enemy on the screen that fires does: a fighter
-- aimed, a heavy three at once, the boss in fans.
local function enemies_fire(beat)
  if #flying() == 0 then return end
  for _, e in ipairs(level:things("enemies")) do
    local every = e.props.fires
    local ry = on_screen(e)
    if every > 0 and ry < 200 and ry > -60 and (beat + e.id) % every == 0 then
      if e.kind == "heavy" then
        bolt(e, -0.3) bolt(e, 0) bolt(e, 0.3)
      elseif e.kind == "boss" then
        -- In turns: the front turrets in fans, the back ones aimed.
        if beat % 2 == 0 then
          for i = 3, 4 do
            for k = 1, 3 do bolt(e, (k - 2) * 0.25, 80, e.x + GUNS[i][1], e.y + GUNS[i][2]) end
          end
        else
          for i = 1, 2 do bolt(e, 0, 90, e.x + GUNS[i][1], e.y + GUNS[i][2]) end
        end
      else
        bolt(e)
      end
      level:sound("zap", e)
    end
  end
end

function on_beat(beat)
  if beat % 4 == 0 then begin_waves(beat, beat // 4) end
  local rest = {}
  for _, q in ipairs(level.state.queue) do
    if q.beat == beat then spawn(q) else table.insert(rest, q) end
  end
  level.state.queue = rest
  enemies_fire(beat)
  level.state.pulse = world.tick
end

-- --------------------------------------------------------------- touches

-- A shot hits an enemy: the one who fired it hit it.
local function hit(e, shot)
  shot:remove()
  e:hurt(1, shot.state.by)
end

-- A power-up taken.
local function take(power, p)
  p.state.power = math.min(3, p.state.power + 1)
  p.state.score = p.state.score + 500
  level:sound("power", p)
  power:remove()
end

function on_touch(a, b)
  if a.seat then
    if b.kind == "bolt" then
      b:remove()
      hurt_ship(a)
    elseif b.kind == "power" then
      if not ship(a).down then take(b, a) end
    elseif b.kind == "mine" then
      b.state.fused = true
      b:hurt(b.state.hp)
    elseif b.families.enemies then
      hurt_ship(a)
    end
    return
  end
  if a.kind == "shot" and b.families.enemies then hit(b, a)
  elseif b.kind == "shot" and a.families.enemies then hit(a, b) end
end

function on_hit(thing, by, damage)
  if thing.families.enemies then
    level:sound("hit", thing)
    thing.state.hit = world.tick
  end
end

-- How big each one blows up, and how long the game holds its breath.
local BLAST = { fighter = 8, swooper = 9, heavy = 13, boss = 14, mine = 10 }
local HOLD = { heavy = 3, boss = 8 }

function on_death(thing, by)
  if not thing.families.enemies then return end
  -- A mine blown by its fuse brings nothing; shot or bombed, its worth.
  if not thing.state.fused and by and by.seat then by.state.score = by.state.score + thing.props.worth end
  if thing.kind == "mine" then
    for _, p in ipairs(level:players()) do
      local dx, dy = thing.x - p.x, thing.y - p.y
      if dx * dx + dy * dy < BURST * BURST then hurt_ship(p) end
    end
  end
  table.insert(level.state.flashes, { x = thing.x, y = thing.y, at = world.tick, size = thing.kind == "boss" and 120 or (thing.kind == "heavy" and 90 or 60) })
  level:explode({ x = thing.x, y = thing.y, z = FLY + 2 }, BLAST[thing.kind])
  if HOLD[thing.kind] then world:hitstop(HOLD[thing.kind]) end
  screen:shake(thing.kind == "boss" and 1 or (thing.kind == "heavy" and 0.4 or 0.15))
  if thing.kind == "heavy" then
    level:spawn("power", thing.x, thing.y, { z = FLY })
  end
  if thing.kind == "boss" then
    level.state.boss = nil
    world.state.over = { result = "won", at = world.tick }
    level.state.won_at = world.tick
    -- It goes on blowing up a while, here and there over its hull.
    level.state.wreck = { x = thing.x, y = thing.y, at = world.tick }
  end
  thing:remove()
end

-- --------------------------------------------------------------- the tick

-- The song's layers follow the fight (music.intensity, each over a bar):
-- with the screen clear only its pads and bass, the drums with a few
-- enemies, the broken chords with more, the melody with many or the
-- boss; quiet a moment when the ship is lost.
local function tension()
  local n = 0
  for _, e in ipairs(level:things("enemies")) do
    local ry = on_screen(e)
    if ry < 240 and ry > -161 then n = n + 1 end
  end
  local t = level.state.boss and 1 or math.min(1, 0.15 + 0.12 * n)
  if #flying() == 0 then t = 0.2 end
  music.intensity = t
end

local function move_shots()
  for _, s in ipairs(level:things("shot")) do
    local dx = s.state.dx or 0
    local v = math.sqrt(dx * dx + SHOT_SPEED * SHOT_SPEED)
    s:walk({ x = dx / v, y = SHOT_SPEED / v }, v)
    if on_screen(s) > 255 then s:remove() end
  end
  for _, b in ipairs(level:things("bolt")) do
    local vx, vy = b.state.vx, SCROLL + b.state.vy
    local v = math.sqrt(vx * vx + vy * vy)
    if v > 0 then b:walk({ x = vx / v, y = vy / v }, v) end
    local ry = on_screen(b)
    if ry < -175 or ry > 255 or b.x < EDGE - 10 or b.x > EDGE + 330 then b:remove() end
  end
  for _, p in ipairs(level:things("power")) do
    p:walk({ x = 0, y = 1 }, SCROLL - 20)
    local a = world.tick * 0.06
    p:face({ x = math.cos(a), y = math.sin(a) })
    if on_screen(p) < -175 then p:remove() end
  end
end

function on_tick()
  level.state.y = level.state.y + SCROLL / 60
  look()
  local wreck = level.state.wreck
  if wreck and world.tick - wreck.at < 120 and (world.tick - wreck.at) % 10 == 0 then
    local x, y = wreck.x + math.random(-45, 45), wreck.y + math.random(-30, 30) + SCROLL * (world.tick - wreck.at) / 60
    level:explode({ x = x, y = y, z = FLY + 3 }, math.random(6, 11))
    table.insert(level.state.flashes, { x = x, y = y, at = world.tick, size = 70 })
    level:effect("boom.vox", { x = x, y = y, z = FLY - 6 }, 14)
  end
  if world.state.over then
    for _, p in ipairs(level:players()) do
      p:walk({ x = 0, y = 1 }, SCROLL + (world.state.over.result == "won" and 60 or 0))
    end
    move_shots()
    move_enemies()
    if level.state.won_at and world.tick - level.state.won_at == 240 then world:win() end
    return
  end
  for _, p in ipairs(level:players()) do
    steer(p)
    if p.input:pressed("bomb") then bomb(p) end
    come_back(p)
  end
  move_shots()
  move_enemies()
  spread_blast()
  mines()
  tension()
  -- Flashes of light fade.
  local keep = {}
  for _, f in ipairs(level.state.flashes) do
    if world.tick - f.at < 20 then table.insert(keep, f) end
  end
  level.state.flashes = keep
end

-- --------------------------------------------------------------- drawing

local TEXT, GOLD, RED, CYAN = { 236, 240, 255 }, { 255, 214, 90 }, { 240, 70, 60 }, { 120, 220, 255 }
local DIM, EMPTY, EDGE, BACKING = { 130, 140, 170 }, { 50, 56, 70 }, { 70, 84, 120 }, { 12, 14, 24 }
local VIEW = 340             -- the height the screen shows (conf: view_height)
local FIELD = 320            -- the field across, between the panels
-- How much of the screen's width each side panel takes: the field fills
-- what is between them where the ship flies (low on the screen, where the
-- field looks a little wider than in its middle).
local function panel(frame)
  local across = VIEW * frame.width / frame.height * 0.96
  return math.max(0, math.floor(frame.width * (1 - FIELD / across) / 2))
end

local function middle_x(frame, text, size)
  return math.floor((frame.width - frame:text_width(text, size)) / 2 + 0.5)
end

-- --------------------------------------------------------------- the stars

-- The stars are drawn, not in the level: layers of them at depths far
-- below the ship, so the camera's perspective glides each by at a speed of
-- its own (the deeper the slower), and the nearest just under the ship,
-- streaks fading behind them. Each layer is a tile of
-- stars repeated over and over; their own random numbers, the game's
-- (math.random) are not drawing's to touch.
local TILE = 256
local CAMERA = VIEW * 0.5 / math.tan(20 * math.pi / 180)   -- how far the camera is from the ship's height (conf: view_height, fov)
local AHEAD_OF = 0.53       -- how far ahead the camera sees a voxel lower down (its pitch, 62°)
local DEEP = {
  { z = -220, count = 100, glow = 0.2, colors = { { 70, 78, 110 }, { 90, 90, 120 }, { 60, 70, 100 } } },
  { z = -110, count = 55, glow = 0.4, colors = { { 130, 140, 180 }, { 150, 150, 170 }, { 120, 130, 170 } } },
  { z = 0, count = 28, glow = 0.7, colors = { { 230, 236, 255 }, { 190, 210, 255 }, { 255, 236, 200 } } },
}
local STREAKS = { count = 1, lo = 8, hi = 30 }     -- how far under the ship

local function scatter(seed, count)
  local s, out = seed, {}
  local function rnd()
    s = (s * 1103515245 + 12345) % 2147483648
    return s / 2147483648
  end
  for i = 1, count do out[i] = { x = rnd() * TILE, y = rnd() * TILE, r = rnd() } end
  return out
end
for i, l in ipairs(DEEP) do l.stars = scatter(i * 7717, l.count) end
STREAKS.stars = scatter(4242, STREAKS.count)

-- Every tile under the window around x, y, and in it every star there.
local function each_star(stars, x0, x1, y0, y1, draw)
  for ty = math.floor(y0 / TILE), math.floor(y1 / TILE) do
    for tx = math.floor(x0 / TILE), math.floor(x1 / TILE) do
      for _, st in ipairs(stars) do
        local x, y = tx * TILE + st.x, ty * TILE + st.y
        if x >= x0 and x <= x1 and y >= y0 and y <= y1 then draw(x, y, st) end
      end
    end
  end
end

local function draw_stars(frame)
  local cy = level.state.y + SCROLL * frame.alpha / 60
  local eye = FLY + 2
  for _, l in ipairs(DEEP) do
    local d = eye - l.z
    local k = (CAMERA + d) / CAMERA
    local mid = cy + d * AHEAD_OF
    each_star(l.stars, MIDDLE - 260 * k, MIDDLE + 260 * k, mid - 190 * k, mid + 330 * k, function(x, y, st)
      frame:cube(x, y, l.z, l.colors[1 + math.floor(st.r * #l.colors)], l.glow)
    end)
  end
  -- The streaks: the nearer the ship, the longer.
  local mid = cy + STREAKS.hi * AHEAD_OF
  each_star(STREAKS.stars, MIDDLE - 260, MIDDLE + 260, mid - 190, mid + 330, function(x, y, st)
    local down = STREAKS.lo + st.r * (STREAKS.hi - STREAKS.lo)
    local length = math.floor(3 + (1 - st.r) * 5)
    for j = 0, length do
      frame:cube(x, y + j, eye - down, { 200, 225, 255, math.floor(200 * (1 - j / (length + 1))) }, 0.5)
    end
  end)
end

-- A player's ship, banking as it goes across, blinking while nothing
-- hurts it, its flame, the flash at its guns, its shield flaring up.
local function draw_ship(frame, p, s)
  if not s.down and (s.safe == 0 or world.tick // 4 % 2 == 0) then
    local bank = s.vx < -40 and 1 or (s.vx > 40 and 2 or 0)
    frame:model(p.seat == 1 and "ship.vox" or "ship2.vox", p, bank)
    -- The model is drawn between the last tick and this one, frame:cube
    -- where it is told: so what goes with the ship goes back by what is
    -- left of this tick's move.
    local back = (1 - frame.alpha) / 60
    local x, y = p.x - s.vx * back, p.y - (SCROLL + s.vy) * back
    -- The engines' flame, out of the two nozzles: hot at the nozzle,
    -- fading to red behind.
    local flame = 4 + (world.tick % 3)
    for k = 1, flame do
      local f = k / (flame + 1)
      local c = { 255, math.floor(230 - 170 * f), math.floor(150 - 130 * f), math.floor(255 * (1 - f * 0.8)) }
      for _, nx in ipairs({ -7, -6, 6, 7 }) do
        frame:cube(x + nx, y - 17 - k, FLY + 4, c, 1)
      end
    end
    frame:light({ x = x, y = y - 16, z = FLY + 4 }, 24, { 255, 170, 90 })
    -- A flash at the guns as they fire.
    if s.muzzle and world.tick - s.muzzle < 3 then
      frame:cube(x, y + 19, FLY + 4, { 220, 250, 255 }, 1)
      frame:light({ x = x, y = y + 19, z = FLY + 5 }, 16, { 150, 220, 255 })
    end
  end
  -- The shield flaring up round the ship as it takes a hit, fading.
  local flare = s.flared and world.tick - s.flared + frame.alpha or 99
  if not s.down and flare < 24 then
    local back = (1 - frame.alpha) / 60
    local x, y = p.x - s.vx * back, p.y - (SCROLL + s.vy) * back
    local fade = 1 - flare / 24
    local r = 20 + flare / 4
    for i = 0, 63 do
      local a = i / 64 * 2 * math.pi
      frame:cube(x + r * math.cos(a), y + r * math.sin(a), FLY + 4, { 120, 230, 255, math.floor(220 * fade) }, 1)
    end
    frame:light({ x = x, y = y, z = FLY + 6 }, 34 * fade, { 120, 220, 255 })
  end
end

-- The keys, shown as a run begins: the keyboard's or the pad's, as the
-- player plays, in the player's language.
local KEYS = {
  en = { keys = "ARROWS", pad = "STICK", "%s FLY   {fire} FIRE   {bomb} BOMB" },
  de = { keys = "PFEILTASTEN", pad = "STICK", "%s FLIEGEN   {fire} FEUER   {bomb} BOMBE" },
  fr = { keys = "FLÈCHES", pad = "STICK", "%s VOLER   {fire} TIR   {bomb} BOMBE" },
}
local function show_keys(frame)
  if world.tick >= 600 then return end
  local t = KEYS[world.language] or KEYS.en
  local move = controls.device == "gamepad" and t.pad or t.keys
  for i, line in ipairs(t) do
    line = line:format(move)
    frame:text(line, math.floor((frame.width - frame:text_width(line)) / 2 + 0.5), frame.height - 22 + 10 * i, TEXT)
  end
end

function on_draw(frame)
  draw_stars(frame)
  local ships = level.state.ships
  if not ships then return end
  show_keys(frame)
  for _, p in ipairs(level:players()) do
    draw_ship(frame, p, ships[p.seat])
  end
  -- A bomb's blast: a ring of fire spreading from where it went off, a
  -- fainter one behind it, both fading as they go.
  local blast = level.state.blast
  if blast then
    local r = SPREAD * (world.tick - blast.at + frame.alpha) / 60
    local y = level.state.y + SCROLL * frame.alpha / 60 + blast.ry
    local fade = math.max(0, 1 - r / REACH)
    -- Its front white-hot, glowing orange behind, three voxels deep.
    local rings = {
      { r, { 255, 245, 210, math.floor(255 * fade) } },
      { r - 1.5, { 255, 200, 110, math.floor(230 * fade) } },
      { r - 3, { 255, 130, 50, math.floor(170 * fade) } },
      { r * 0.8, { 255, 110, 50, math.floor(110 * fade) } },
    }
    for _, ring in ipairs(rings) do
      local rr = math.max(0, ring[1])
      local n = math.floor(rr * 2.2) + 8
      for i = 0, n - 1 do
        local a = i / n * 2 * math.pi
        frame:cube(blast.x + rr * math.cos(a), y + rr * math.sin(a), FLY + 3, ring[2], 1)
      end
    end
    frame:light({ x = blast.x, y = y, z = FLY + 10 }, (60 + r) * fade, { 255, 190, 110 })
  end
  -- The enemies, each its kind's model, lit up on the music's beats; a
  -- hit lights them white a moment.
  local beat = level.state.pulse and world.tick - level.state.pulse or 99
  for _, e in ipairs(level:things("enemies")) do
    frame:model(e.kind .. ".vox", e)
    if e.kind == "mine" then
      -- Its lens blinks slowly; set off, fast, and a ring shows how far
      -- its blast reaches.
      local armed = e.state.armed and world.tick - e.state.armed + frame.alpha
      local on = armed and (world.tick // 4) % 2 == 0 or (not armed and world.tick % 60 < 8)
      if on then
        frame:cube(e.x, e.y, e.z + 15, { 255, 90, 70 }, 1)
        frame:light({ x = e.x, y = e.y, z = e.z + 16 }, armed and 30 or 14, { 255, 60, 40 })
      end
      if armed then
        local k = math.min(1, armed / FUSE)
        for i = 0, 79 do
          local a = i / 80 * 2 * math.pi
          frame:cube(e.x + BURST * math.cos(a), e.y + BURST * math.sin(a), FLY + 2, { 255, 70, 50, math.floor(60 + 140 * k) }, 0.8)
        end
      end
    elseif e.state.hit and world.tick - e.state.hit < 4 then
      frame:light({ x = e.x, y = e.y, z = FLY + 8 }, 26, { 255, 255, 255 })
    elseif beat < 8 then
      frame:light({ x = e.x, y = e.y, z = FLY + 4 }, (e.kind == "boss" and 40 or 18) * (1 - beat / 8), { 255, 200, 90 })
    end
  end
  for _, p in ipairs(level:things("power")) do
    frame:model("power.vox", p)
    frame:light(p, 16, { 90, 180, 255 })
  end
  -- The ship's shots: beams of light, white at the head, blue behind;
  -- drawn back by what is left of the tick, as the models are.
  local back = (1 - frame.alpha) / 60
  for _, sh in ipairs(level:things("shot")) do
    local x, y = sh.x - (sh.state.dx or 0) * back, sh.y - SHOT_SPEED * back
    for j = 0, 7 do
      local c = j < 2 and { 235, 252, 255 } or { 90, 200, 255, math.floor(255 * (1 - (j - 2) / 7)) }
      frame:cube(x, y - j, sh.z, c, 1)
      frame:cube(x + 1, y - j, sh.z, c, 1)
    end
  end
  -- The enemies' bolts: glowing balls, a white heart in a pink glow.
  for _, b in ipairs(level:things("bolt")) do
    local x, y = b.x - b.state.vx * back, b.y - (SCROLL + b.state.vy) * back
    frame:cube(x, y, b.z, { 255, 230, 240 }, 1)
    for _, d in ipairs({ { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } }) do
      frame:cube(x + d[1], y + d[2], b.z, { 255, 80, 150 }, 1)
      frame:cube(x + d[1] * 2, y + d[2] * 2, b.z, { 255, 60, 130, 120 }, 1)
    end
    for _, d in ipairs({ { 1, 1 }, { -1, 1 }, { 1, -1 }, { -1, -1 } }) do
      frame:cube(x + d[1], y + d[2], b.z, { 255, 60, 130, 170 }, 1)
    end
  end
  for _, f in ipairs(level.state.flashes) do
    local k = 1 - (world.tick - f.at) / 20
    frame:light({ x = f.x, y = f.y, z = FLY + 4 }, f.size * k, { 255, 180, 90 })
  end
  local boss = level.state.boss
  if boss then frame:light({ x = boss.x, y = boss.y - 5, z = FLY + 12 }, 45, { 255, 60, 90 }) end

  -- The HUD in the panels to the sides, as in the arcades: the score and
  -- the guns' power to the left, the ships, the shield and the bombs to
  -- the right; with two players each one's all in a panel of its own.
  -- Over the field only the boss's health and the messages.
  local w = panel(frame)
  local right = frame.width - w
  local function label(text, x, y, color)
    frame:text(text, x + math.floor((w - frame:text_width(text)) / 2), y, color or DIM)
  end
  local function score_power(pl, x, y)
    local p = pl.state
    label("SCORE", x, y)
    label(string.format("%07d", p.score), x, y + 10, GOLD)
    label("POWER", x, y + 28)
    for i = 0, 2 do
      frame:box(x + math.floor((w - 40) / 2) + 14 * i, y + 38, 12, 3, i < p.power and CYAN or EMPTY)
    end
  end
  local function ships_shield_bombs(pl, x, y)
    local p = pl.state
    -- The ships to come: a few as pictures, more as a number.
    label("SHIPS", x, y)
    local spare = math.max(p.lives - 1, 0)
    if spare > 4 then
      label("x" .. spare, x, y + 10, TEXT)
    else
      local x0 = x + math.floor((w - (9 * spare - 2)) / 2)
      for i = 0, spare - 1 do frame:icon("life", x0 + 9 * i, y + 10) end
    end
    -- The shield: a piece a hit it still takes, green, yellow, red and
    -- blinking at the last.
    label("SHIELD", x, y + 28)
    local s = ships[pl.seat]
    local shield = (not s or s.down) and 0 or s.shield
    local tint = shield >= 3 and { 90, 230, 140 } or (shield == 2 and { 250, 210, 70 } or RED)
    for i = 0, SHIELD - 1 do
      local lit = i < shield and (shield > 1 or world.tick // 15 % 2 == 0)
      frame:box(x + math.floor((w - 40) / 2) + 14 * i, y + 38, 12, 3, lit and tint or EMPTY)
    end
    label("BOMBS", x, y + 56)
    if p.bombs > 5 then
      label("x" .. p.bombs, x, y + 66, TEXT)
    else
      local x0 = x + math.floor((w - (7 * p.bombs - 2)) / 2)
      for i = 0, p.bombs - 1 do frame:icon("bomb", x0 + 7 * i, y + 66) end
    end
  end
  local second = world.players[2]
  if second then
    for seat, x in ipairs({ 0, right }) do
      local pl = world.players[seat]
      label(seat .. "UP", x, 6, seat == 1 and CYAN or GOLD)
      if pl and pl.state.out then
        label("GAME", x, 30, RED)
        label("OVER", x, 40, RED)
      elseif pl then
        score_power(pl, x, 18)
        ships_shield_bombs(pl, x, 74)
      end
    end
  else
    score_power(player, 0, 8)
    ships_shield_bombs(player, right, 8)
    -- A second player may join: START on a pad, Return at the keyboard.
    if world.tick // 30 % 2 == 0 then
      label("2UP", right, frame.height - 30, GOLD)
      label("PRESS START", right, frame.height - 20, DIM)
    end
  end
  -- The panels themselves, after what is on them (a HUD voxel drawn first
  -- keeps its cell): an edge towards the field, dark behind.
  frame:box(w - 1, 0, 1, frame.height, EDGE)
  frame:box(right, 0, 1, frame.height, EDGE)
  frame:box(0, 0, w - 1, frame.height, BACKING)
  frame:box(right + 1, 0, w - 1, frame.height, BACKING)
  if boss then
    frame:bar(math.floor((frame.width - 80) / 2 + 0.5), 11, 80, 2, boss.state.hp / level.state.boss_hp, RED)
  end
  local m = level.state.message
  if m and world.tick - m.at < 180 then
    frame:drop_in(m.text, middle_x(frame, m.text, 2), frame.height / 2 - 20, world.tick - m.at + frame.alpha, m.text == "WARNING" and RED or CYAN, 2)
  end
  local over = world.state.over
  if over then
    local t = world.tick - over.at + frame.alpha
    local text = over.result == "won" and "MISSION COMPLETE" or "GAME OVER"
    frame:drop_in(text, middle_x(frame, text, 2), frame.height / 2 - 20, t, over.result == "won" and GOLD or RED, 2)
  end
  -- The run is over and the engine waits for a key to go on to the high
  -- scores (no tick runs any more, so nothing drops in or blinks now).
  if world.state.ended then
    local score = "SCORE " .. player.state.score
    if second then score = "1UP " .. player.state.score .. "   2UP " .. second.state.score end
    local go = controls.device == "gamepad" and "PRESS {pause}" or "PRESS ENTER"
    frame:text(score, middle_x(frame, score, 1), frame.height / 2 + 4, GOLD)
    frame:text(go, middle_x(frame, go, 1), frame.height / 2 + 16, TEXT)
  end
end

function on_world_end(result)
  world.state.ended = result
end

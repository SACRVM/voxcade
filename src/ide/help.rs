//! HELP: the Lua API by example, in the main area. A topic is a few plain
//! sentences, the lines it needs in `conf.lua` and a whole `main.lua` to
//! copy, as simple as can be; first of them the reference, every name the
//! API has, from its register (`lua::api`). Every example runs as a game
//! in a test, so what is copied works.

use egui::{RichText, TextStyle};

use crate::gui::{ACCENT, PANEL, QUIET, TEXT};

/// A topic: its title, what it says, what goes into `conf.lua` (inside
/// `function conf(c)`; may be empty) and `main.lua` (empty where a topic
/// is about the IDE itself: Claude in the console).
pub struct Topic {
    pub title: &'static str,
    pub text: &'static str,
    pub conf: &'static str,
    pub code: &'static str,
    /// The game's own files besides main.lua (Lua files it `require`s, a
    /// sound in `sounds/`): name, text.
    pub files: &'static [(&'static str, &'static str)],
}

pub const TOPICS: &[Topic] = &[
    Topic {
        title: "THE HANDLERS",
        text: "A game is two files. conf.lua sets things up: the title, the level, the kinds of things. main.lua is the game itself: functions the engine calls for you. on_world_start runs once when a game begins, on_level_start when a level begins, on_tick 60 times a second, on_draw for every picture, which it draws on frame. world, level, player, screen and controls are there in all of them, input in on_tick.",
        conf: "",
        files: &[],
        code: r##"-- Runs once, when a game begins (there is no level yet).
function on_world_start()
  world.state.time = 0
end

-- Runs when a level begins: build it, put things in it.
function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
end

-- Runs 60 times a second: the rules of your game.
function on_tick()
  world.state.time = world.state.time + 1
end

-- Runs for every picture: what to show. It only looks, it changes nothing.
function on_draw(frame)
  frame:text("SECONDS " .. math.floor(world.state.time / 60), 4, 4)
end
"##,
    },
    Topic {
        title: "MOVE WITH THE ARROWS",
        text: "The player is a thing: c.player.kind says of which kind, and it stands where each level starts. Walk it in on_tick: input.dir is the arrow keys (or the stick) as a direction. A thing without a model is invisible, so draw it yourself as cubes.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  -- a floor to stand on
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
end

function on_tick()
  -- 40 voxels a second
  player:walk(input.dir, 40)
end

function on_draw(frame)
  for z = 0, 7 do
    frame:cube(player.x, player.y, player.z + z, { 200, 90, 60 })
  end
end
"##,
    },
    Topic {
        title: "A BUTTON OF YOUR OWN",
        text: "Name your actions in conf.lua and give each a key and a pad button. input:pressed is true on the tick the button goes down, input:held as long as it is down; the directions work the same (input:pressed(\"left\"): one step a press, for a grid or a menu). New actions need a restart: F5.",
        conf: r##"c.actions = { "jump" }
c.keys = { jump = { "Space" } }
c.pad = { jump = { "south" } }
c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
end

function on_tick()
  player:walk(input.dir, 40)
  if input:pressed("jump") then
    -- 12 voxels high, if it stands on something
    player:jump(12)
  end
end

function on_draw(frame)
  for z = 0, 7 do
    frame:cube(player.x, player.y, player.z + z, { 200, 90, 60 })
  end
end
"##,
    },
    Topic {
        title: "REMEMBER THINGS",
        text: "Everything that changes while playing goes into a state: world.state for the whole game, level.state for the level being played and the things in it, player.state for the player (it goes with them from level to level), thing.state for one thing. Plain variables in main.lua cannot change once the game runs: replays and saves must see all of it.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.lamp = {}"##,
        files: &[],
        code: r##"function on_world_start()
  player.state.lives = 3
end

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level.state.lamp = level:spawn("lamp", 70, 32)
end

function on_tick()
  player:walk(input.dir, 40)
  -- the lamp stays lit once the player came by
  local lamp = level.state.lamp
  if player:near(lamp, 8) then
    lamp.state.lit = true
  end
end

function on_draw(frame)
  local lamp = level.state.lamp
  if lamp.state.lit then
    frame:cube(lamp.x, lamp.y, lamp.z + 4, { 255, 220, 120 }, 1)
  else
    frame:cube(lamp.x, lamp.y, lamp.z + 4, { 60, 60, 70 })
  end
  frame:text("LIVES " .. player.state.lives, 4, 4)
end
"##,
    },
    Topic {
        title: "PICK THINGS UP",
        text: "Put a few things in the level, look for those near the player, remove them and count. level:things(\"coin\") is every coin, in the order they came; a coin removed is gone at the end of the tick. math.random gives the same numbers every time the game is played the same way.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.coin = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level.state.coins_left = 5
  for i = 1, 5 do
    level:spawn("coin", math.random(8, 88), math.random(8, 56))
  end
end

function on_tick()
  player:walk(input.dir, 40)
  for _, coin in ipairs(level:things("coin")) do
    if player:near(coin, 6) and coin:remove() then
      player.state.score = player.state.score + 10
      level.state.coins_left = level.state.coins_left - 1
    end
  end
  if level.state.coins_left == 0 then
    world:win()
  end
end

function on_draw(frame)
  for _, coin in ipairs(level:things("coin")) do
    -- the last number makes it glow
    frame:cube(coin.x, coin.y, coin.z + 1, { 255, 210, 60 }, 1)
  end
  frame:text("SCORE " .. player.state.score, 4, 4)
end
"##,
    },
    Topic {
        title: "CARRY ITEMS",
        text: "Items are what the player carries: name them in conf.lua (c.items), each with how many fit (stack). player:take(\"key\") puts one into player.state.inventory (no more than the stack; it says how many it took) and tells on_pickup; the inventory counts them by name, world.items lists them in order, to draw. Here keys lie about and a door takes one. With ready-made rules (c.kit) it is the same: a pickup touched (on_touch) gives what its props say, player:take(b.props.item).",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.key = {}
c.things.door = {}
c.items.key = { stack = 3 }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level:spawn("key", 30, 20)
  level:spawn("key", 66, 44)
  level.state.door = level:spawn("door", 88, 32)
end

function on_tick()
  player:walk(input.dir, 40)
  for _, key in ipairs(level:things("key")) do
    -- taken only if there is room for it
    if player:near(key, 6) and player:take("key") == 1 then
      key:remove()
    end
  end
  local carried = player.state.inventory
  -- the door opens for a key, and keeps it
  local door = level.state.door
  if door and player:near(door, 8) and (carried.key or 0) > 0 and door:remove() then
    carried.key = carried.key - 1
    level.state.door = nil
  end
end

function on_draw(frame)
  for _, key in ipairs(level:things("key")) do
    frame:cube(key.x, key.y, key.z + 1, { 255, 210, 60 }, 1)
  end
  local door = level.state.door
  if door then
    for z = 0, 9 do
      frame:cube(door.x, door.y, door.z + z, { 120, 80, 50 })
    end
  end
  frame:text("KEYS " .. (player.state.inventory.key or 0), 4, 4)
end
"##,
    },
    Topic {
        title: "TOUCH THINGS",
        text: "on_touch(a, b, where) runs once when two things begin to touch, on_touch_end(a, b) once when they part: not every tick. The player is always a. where is the face of b that a met: \"top\" when a came down onto it (landed on it), \"bottom\", or \"side\". For what lasts while touched ask player:touching(kind) in on_tick: wading through water. What already touches where a level begins counts as touching, so a player who arrives on a door does not go through it again.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.slime = { hp = 1 }
c.things.water = {}
c.things.door = {}
c.actions = { "jump" }
c.keys = { jump = { "Space" } }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level:spawn("slime", 50, 32)
  level:spawn("water", 70, 32)
  level:spawn("door", 100, 32)
end

-- Once, as a touch begins: landed on, the slime goes and the hero
-- bounces off it; the door wins.
function on_touch(a, b, where)
  if a ~= player then return end
  if b.kind == "slime" and where == "top" then
    b:hurt(1, player)
    player:bounce(6)
  elseif b.kind == "door" then
    world:win()
  end
end

function on_death(thing)
  thing:remove()
end

-- Every tick, for as long as it lasts: slow in the water.
function on_tick()
  local speed = 40
  if #player:touching("water") > 0 then speed = 15 end
  player:walk(input.dir, speed)
  if input:pressed("jump") then player:jump(10) end
end

function on_draw(frame)
  for _, t in ipairs(level:things()) do
    local color = ({ slime = { 90, 200, 90 }, water = { 60, 120, 220 }, door = { 120, 80, 50 } })[t.kind]
    frame:cube(t.x, t.y, t.z, color)
  end
end
"##,
    },
    Topic {
        title: "HITS AND DEATHS",
        text: "A kind with hp has hit points: thing.state.hp. thing:hurt(damage, by, weapon) takes some off and tells on_hit(thing, by, damage, weapon); when they run out, on_death(thing, by, weapon) runs once and the thing is no longer counted. Count in on_death what happened: counting what is left (level:count) cannot tell a slime that died from one that has not come yet. The ready-made rules (c.kit) tell their hits the same way.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.slime = { hp = 2 }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  for i = 1, 3 do
    level:spawn("slime", 20 + i * 18, 32)
  end
end

-- each touch hurts the slime: a hit point off (on_hit runs too)
function on_touch(a, b)
  if b.kind == "slime" then
    b:hurt(1, a)
  end
end

function on_death(thing, by)
  if thing.kind == "slime" then
    thing:remove()
    world.state.slimes_down = (world.state.slimes_down or 0) + 1
    if world.state.slimes_down == 3 then world:win() end
  end
end

function on_tick()
  player:walk(input.dir, 40)
end

function on_draw(frame)
  for _, slime in ipairs(level:things("slime")) do
    frame:cube(slime.x, slime.y, slime.z + 1, { 90, 200, 90 }, slime.state.hp / 2)
  end
  frame:text("SLIMES DOWN " .. (world.state.slimes_down or 0), 4, 4)
end
"##,
    },
    Topic {
        title: "TALK AND HOLD",
        text: "Name an action interact in conf.lua: pressed while the player touches something, on_interact(thing, player) runs for it (talk, open, read), and that press is used up for this tick. level.state.held = true holds the level (the rules, the physics, the touches) while on_tick and on_draw go on: for a message, a shop, a tally.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.sign = {}
c.actions = { "interact" }
c.keys = { interact = { "E" } }
c.pad = { interact = { "south" } }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level:spawn("sign", 70, 32)
end

function on_interact(thing, by)
  if thing.kind == "sign" then
    level.state.held = true
  end
end

function on_tick()
  if level.state.held then
    -- the next press closes the message
    if input:pressed("interact") then level.state.held = false end
    return
  end
  player:walk(input.dir, 40)
end

function on_draw(frame)
  for _, sign in ipairs(level:things("sign")) do
    frame:cube(sign.x, sign.y, sign.z + 2, { 200, 170, 90 })
  end
  if level.state.held then
    frame:text("BEWARE OF THE SLIMES", 30, 40, { 255, 230, 160 })
  end
end
"##,
    },
    Topic {
        title: "SCORE, WIN AND LOSE",
        text: "player.state.score is the score. world:win() and world:lose() end the game. With c.scored = true the high score table counts it. world.tick counts the ticks: 60 make a second.",
        conf: r##"c.scored = true"##,
        files: &[],
        code: r##"function on_tick()
  -- a point every second
  if world.tick % 60 == 0 then
    player.state.score = player.state.score + 1
  end
  if player.state.score >= 30 then
    world:win()
  end
end
"##,
    },
    Topic {
        title: "TWO PLAYERS",
        text: "c.players gives the game seats: a second player joins with START on another pad, or Return at the keyboard (the whole keyboard with the game's keys while player 1 plays on a pad; else each half is a player's: WASD with F and G, the arrows with K and L); In a game of several, SETTINGS and CONTROLS give each player its keyboard or pad and its own keys. world.players[n] is seat n's player or false; level:players() the players playing, each with its own keys (p.input) and state (p.state). c.camera.hold keeps them within reach of each other on one screen; the camera looks at their middle.",
        conf: r##"c.players = 2
c.player.kind = { "red", "blue" }
c.things.red = {}
c.things.blue = {}
c.camera.hold = { 120, 80 }
c.scored = true"##,
        files: &[],
        code: r##"local COLORS = { { 200, 90, 60 }, { 60, 110, 200 } }

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
end

-- Someone took a seat: it comes in beside the first player.
function on_player_join(p, how)
  level.state.hello, level.state.hello_at = p, world.tick
end

function on_tick()
  -- every player playing, each with its own keys and score
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
    if world.tick % 60 == 0 then
      p.state.score = p.state.score + 1
    end
  end
end

function on_draw(frame)
  for _, p in ipairs(level:players()) do
    for z = 0, 7 do
      frame:cube(p.x, p.y, p.z + z, COLORS[p.seat])
    end
  end
  -- a score for each seat, PRESS START where nobody sits
  for seat, p in ipairs(world.players) do
    local x = seat == 1 and 4 or frame.width - 70
    frame:text(p and ("P" .. seat .. " " .. p.state.score) or "PRESS START", x, 4)
  end
  local hello = level.state.hello
  if hello and world.tick - level.state.hello_at < 120 then
    frame:text("PLAYER " .. hello.seat .. " JOINS", frame.width / 2 - 40, 30)
  end
end
"##,
    },
    Topic {
        title: "LEVEL AFTER LEVEL",
        text: "A game's levels are the files in its levels/ folder, in the order of their names (01_meadow.toml, 02_cave.toml); without any it has one, made in code like this one. world:go_to(level.next) goes on to the next at the end of the tick, world:win() wins after the last, world:restart() starts this one over. on_level_start(how) says how a level was entered; level.state is fresh each time, world.state and player.state go along, and so does the player.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  -- every level begins at the left
  player:place(8, 32)
  level.state.time = 20 * 60
  if how == "restart" then
    player.state.tries = (player.state.tries or 0) + 1
  end
end

function on_tick()
  player:walk(input.dir, 40)
  -- at the far right: on to the next level, after the last the game is won
  if player.x > level.size.x - 4 then
    if level.next then world:go_to(level.next) else world:win() end
  end
  -- out of time: this level again from its start
  level.state.time = level.state.time - 1
  if level.state.time == 0 then world:restart() end
end

function on_draw(frame)
  frame:text("TIME " .. level.state.time // 60 .. "  TRIES " .. (player.state.tries or 0), 4, 4)
end
"##,
    },
    Topic {
        title: "SAVE THE GAME",
        text: "Give the game save slots in conf.lua (c.saves): the pause menu then has SAVE and LOAD. world:save(slot) saves at the end of the tick, here at a flag; world:load(slot) goes on from a save like a level change, and on_level_start then says \"load\". world.saves tells what lies in each slot. A save keeps the states and where the player stands; the level comes from its file, its things too (a thing in level.state comes back as nil). Saves live in the player's config folder, never in the game's.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.flag = {}
c.saves = { slots = 3 }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level.state.flag = level:spawn("flag", 70, 32)
  if how == "load" then
    world.state.loads = (world.state.loads or 0) + 1
  end
end

function on_tick()
  player:walk(input.dir, 40)
  -- reaching the flag saves into slot 1, once until the player walks off
  local near = player:near(level.state.flag, 8)
  if near and not level.state.at_flag then
    world:save(1)
  end
  level.state.at_flag = near
end

function on_draw(frame)
  local flag = level.state.flag
  for z = 0, 9 do
    frame:cube(flag.x, flag.y, flag.z + z, { 220, 60, 60 })
  end
  local save = world.saves[1]
  if save then
    frame:text("SAVED IN " .. save.level .. " AT " .. save.seconds .. " S", 4, 4)
  end
end
"##,
    },
    Topic {
        title: "YOUR OWN LUA FILES",
        text: "A bigger game splits its code into files of its own. require(\"market\") runs market.lua once as the game starts and gives back what the file returns, here a table with the prices and a function; asked again it gives the same. Dots go into folders: require(\"lang.de\") is lang/de.lua. Call it at the top of main.lua or of a file it requires, never later. The files share main.lua's globals (world, level, player and the rest are there; a handler may live in any of them) and are your game's code as main.lua is: saving one runs the code anew. What changes still goes in a state, never in a file's own locals.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[("market.lua", r##"-- The prices, and what a sale gives.
local M = {}

M.prices = { carrot = 9, wheat = 7 }

function M.sell(what)
  world.state.coins = world.state.coins + M.prices[what]
end

return M
"##)],
        code: r##"local market = require("market")

function on_world_start()
  world.state.coins = 0
end

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
end

function on_tick()
  player:walk(input.dir, 40)
  -- a carrot sold every second
  if world.tick % 60 == 0 then
    market.sell("carrot")
  end
end

function on_draw(frame)
  frame:text("COINS " .. world.state.coins .. "  A CARROT " .. market.prices.carrot, 4, 4)
end
"##,
    },
    Topic {
        title: "PLAY A SOUND",
        text: "A sound is a file of your game in the folder sounds (the plus beside SOUNDS makes one, PLAY SOUND plays it; over its text COIN, JUMP and the other sorts roll a fresh one each time they are pressed, UNDO goes back, and the faders change it), a few numbers a line: a wave (square, saw, triangle, sine or noise), a pitch in Hz, how long it holds and fades in seconds, how loud from 0 to 1. Only what differs from the defaults is written; sweep slides the pitch and jump lifts it once, both in semitones (12 an octave). The file's name is the sound's: play it by that name at a place, a thing or a point. Recordings (.wav) are not played: everything in a game is code or text.",
        conf: "",
        files: &[("sounds/beep.toml", "# a short high beep\nwave = \"square\"\nfreq = 880\nhold = 0.05\ndecay = 0.2\nvolume = 0.3\n")],
        code: r##"function on_tick()
  -- a beep every second, from the middle of the level
  if world.tick % 60 == 0 then
    local middle = { x = level.size.x / 2, y = level.size.y / 2, z = 0 }
    level:sound("beep", middle)
  end
end
"##,
    },
    Topic {
        title: "GO WITH THE MUSIC",
        text: "Play a song of c.audio.music as your game's own (music:play, by its file's name) and the game can go with it: on_beat runs on each of its beats, before on_tick, and music.bar counts its bars. Here a blip comes on the first beat of every bar and the screen glows on each beat. The song keeps time with the game (a pause holds both), so replays play it the same.",
        conf: r##"c.audio.music = { "music/drive.mml" }   -- your song: music:play calls it "drive"
c.player.kind = "hero"
c.things.hero = {}
c.things.blip = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#30343c")
  music:play("drive", "now")
end

function on_beat(beat)
  screen:flash(0.05)
  -- 4 beats to a bar: the first of each brings a blip
  if beat % 4 == 0 then
    level:spawn("blip", 8 + (beat * 7) % (level.size.x - 16), level.size.y - 4)
  end
end

function on_tick()
  player:walk(input.dir, 40)
  for _, b in ipairs(level:things("blip")) do
    b:walk({ x = 0, y = -1 }, 20)
    if b.y < 4 then b:remove() end
  end
end

function on_draw(frame)
  for _, b in ipairs(level:things("blip")) do
    frame:cube(b.x, b.y, b.z, { 120, 220, 255 })
  end
  frame:cube(player.x, player.y, player.z, { 200, 90, 60 })
end
"##,
    },
    Topic {
        title: "BUILD THE LEVEL",
        text: "A level made in code (c.world.size in conf.lua) starts empty. Fill boxes of voxels when it begins, from one corner to the other: a color as \"#rrggbb\", or nil to empty them again.",
        conf: r##"c.world.size = { 96, 64, 24 }"##,
        files: &[],
        code: r##"function on_level_start(how)
  local w, d = level.size.x, level.size.y
  -- the floor
  level:fill(0, 0, 0, w - 1, d - 1, 0, "#5a6e4a")
  -- a wall
  level:fill(10, 20, 1, 40, 22, 6, "#8a7a62")
  -- a door in it
  level:fill(24, 20, 1, 27, 22, 5, nil)
  -- one voxel
  level:set_voxel(60, 40, 1, "#ffcc00")
end
"##,
    },
    Topic {
        title: "TEXT AND KEYS ON SCREEN",
        text: "on_draw(frame) draws this picture: frame:text writes on the screen, counted from its top left corner; an action in braces, {jump}, is written as the key the player presses for it, the way their keyboard or pad prints it. controls is what the player plays with, in every handler: controls.device (keyboard or gamepad) and controls:key(action), the key alone.",
        conf: r##"c.actions = { "jump" }
c.keys = { jump = { "Space" } }"##,
        files: &[],
        code: r##"function on_draw(frame)
  frame:text("SCORE " .. player.state.score, 4, 4)
  frame:text("PRESS {jump} TO JUMP", 4, 14, { 255, 220, 120 })
end
"##,
    },
    Topic {
        title: "DRAW A HUD",
        text: "The HUD is your game's: on_draw draws it on the frame, counted from the top left in UI voxels; frame.width and frame.height say how big it is (the width follows the screen's shape). Small pictures are c.icons, a letter a color; frame:icon draws one, screen:burst makes one fly apart. frame:text takes a size (2 and up: big letters), frame:text_width measures for centering, frame:bar shows a share (a boss's health), frame:drop_in lets letters drop in, frame:to_screen says where a thing of the level shows (a name over its head). What changes over time (a heart lost, when a message began) goes into a state in on_tick.",
        conf: r##"c.player.kind = "hero"
c.things.hero = { hp = 3 }
c.icons.heart = {
  rows = { ".RR.RR.", "RRRRRRR", ".RRRRR.", "..RRR..", "...R..." },
  colors = { R = { 214, 36, 44 } },
}
c.icons.empty = {
  rows = { ".KK.KK.", "K..K..K", ".K...K.", "..K.K..", "...K..." },
  colors = { K = { 96, 70, 64 } },
}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level.state.began = world.tick
end

function on_tick()
  player:walk(input.dir, 40)
  -- a heart lost every two seconds, to see it burst
  if world.tick % 120 == 0 and player.state.hp > 0 then
    player.state.hp = player.state.hp - 1
    screen:burst("heart", 4 + 9 * player.state.hp, 4)
  end
  player.state.score = world.tick
end

function on_draw(frame)
  for i = 0, 2 do
    frame:icon(i < player.state.hp and "heart" or "empty", 4 + 9 * i, 4)
  end
  local score = string.format("%06d", player.state.score)
  frame:text(score, frame.width - 4 - frame:text_width(score), 4, { 255, 214, 90 })
  frame:bar(4, 14, 40, 2, player.state.hp / 3)
  local title = "LEVEL ONE"
  local x = (frame.width - frame:text_width(title, 2)) / 2
  frame:drop_in(title, x, frame.height / 2 - 16, world.tick - level.state.began, { 230, 58, 48 }, 2)
  -- a name over the hero's head, wherever it walks
  local hx, hy = frame:to_screen(player, 16)
  if hx then
    frame:text("YOU", hx - frame:text_width("YOU") / 2, hy - 8)
  end
end
"##,
    },
    Topic {
        title: "LIGHT, SHAKE AND FLASH",
        text: "frame:light puts a light into this picture. level:explode blows a hole into the level, screen:shake shakes the screen, screen:flash flashes it, world:hitstop holds the game still for a few ticks.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 1, "#5a6e4a")
end

function on_tick()
  player:walk(input.dir, 40)
  -- every 3 seconds: a blast where the player stands
  if world.tick % 180 == 179 then
    level:explode(player, 6)
    screen:shake(0.35)
    screen:flash(0.5)
    world:hitstop(3)
  end
end

function on_draw(frame)
  -- a warm light that walks with the player
  frame:light(player, 40, { 255, 180, 90 })
end
"##,
    },
    Topic {
        title: "DAY AND NIGHT",
        text: "frame:daylight sets this picture's daylight: where the sun's light comes from (sun, traveling down), its color and strength (sun_color), the light every face gets (ambient), how dark shadows are (shadow) and the sky. Work it out in on_draw from your game's clock, every picture: here a day of 40 seconds, the sun rising in the east, white at noon, warm low down, and a blue night with a lamp (frame:light). What you leave out is c.light's. It is only the picture: the game's rules do not see it, so the clock that drives it is the game's own (world.tick here, or a state).",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[],
        code: r##"local DAY = 40 * 60   -- a day and a night, in ticks

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level:fill(56, 36, 1, 60, 40, 10, "#a08060")   -- a tower, for a shadow
end

function on_tick()
  player:walk(input.dir, 40)
end

function on_draw(frame)
  local t = (world.tick + frame.alpha) % DAY / DAY   -- 0 dawn, 0.5 dusk
  if t < 0.5 then
    -- the sun from the east over the top to the west, warm when low
    local a = t * 2 * math.pi
    local high = math.sin(a)
    local low = 1 - high
    frame:daylight{
      sun = { -math.cos(a), 0.3, -math.max(high, 0.15) },
      sun_color = { 255, math.floor(255 - 90 * low), math.floor(255 - 160 * low) },
      ambient = 0.35 + 0.15 * high,
      sky = { math.floor(40 + 60 * high), math.floor(50 + 80 * high), math.floor(90 + 100 * high) },
    }
  else
    -- a pale moon, a dark sky, and a lamp by the tower
    frame:daylight{ sun = { 0.3, 0.5, -1 }, sun_color = { 70, 90, 150 }, ambient = 0.15, sky = { 8, 10, 30 } }
    frame:light({ x = 62, y = 38, z = 6 }, 30, { 255, 190, 110 })
  end
end
"##,
    },
    Topic {
        title: "A MENU AND A BAG",
        text: "A menu is your game's: what is open and where its cursor stands live in level.state, the level held while it is open (level.state.held = true: the world waits, on_tick goes on), and on_tick moves the cursor with input:repeated, which repeats while a key is held. frame:menu draws it in the engine's look: a title, tabs, rows with a value at the right and an icon, the chosen one marked, dimmed rows you cannot afford, a hint with the keys. frame:grid draws slots, here the bag. Walk to the stall and press {act}.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.actions = { "act", "back", "bag" }
c.keys = { act = { "Space" }, back = { "X" }, bag = { "Tab" } }
c.pad = { act = { "south" }, back = { "east" }, bag = { "north" } }
c.icons.coin = { rows = { ".YY.", "YYYY", "YYYY", ".YY." }, colors = { Y = { 240, 200, 60 } } }"##,
        files: &[],
        code: r##"local GOODS = {
  { name = "carrot", text = "Carrot seed", price = 3, sells = 9 },
  { name = "wheat", text = "Wheat seed", price = 2, sells = 7 },
  { name = "tomato", text = "Tomato seed", price = 12, sells = 24 },
}

function on_world_start()
  world.state.coins = 20
  world.state.bag = { carrot = 0, wheat = 0, tomato = 0 }
end

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level:fill(60, 30, 1, 66, 34, 6, "#a06040")   -- the stall
end

local function near_stall()
  return math.abs(player.x - 63) < 12 and math.abs(player.y - 32) < 12
end

function on_tick()
  local menu = level.state.menu
  if menu then
    -- up and down choose (again and again while held), left and right
    -- switch between buying and selling
    if input:repeated("down") then menu.at = menu.at % #GOODS + 1 end
    if input:repeated("up") then menu.at = (menu.at - 2) % #GOODS + 1 end
    if input:pressed("left") or input:pressed("right") then menu.tab = 3 - menu.tab end
    local good, bag = GOODS[menu.at], world.state.bag
    if input:pressed("act") then
      if menu.tab == 1 and world.state.coins >= good.price then
        world.state.coins = world.state.coins - good.price
        bag[good.name] = bag[good.name] + 1
      elseif menu.tab == 2 and bag[good.name] > 0 then
        bag[good.name] = bag[good.name] - 1
        world.state.coins = world.state.coins + good.sells
      end
    end
    if input:pressed("back") then
      level.state.menu = nil
      level.state.held = false
    end
    return
  end
  player:walk(input.dir, 40)
  level.state.bag_open = input:held("bag")
  if input:pressed("act") and near_stall() then
    level.state.menu = { at = 1, tab = 1 }
    level.state.held = true
  end
end

function on_draw(frame)
  frame:text("Coins " .. world.state.coins, 4, 4)
  local menu, bag = level.state.menu, world.state.bag
  if menu then
    local rows = {}
    for i, good in ipairs(GOODS) do
      if menu.tab == 1 then
        rows[i] = { text = good.text, value = good.price, icon = "coin", dim = world.state.coins < good.price }
      else
        rows[i] = { text = good.text, value = bag[good.name] .. " for " .. good.sells, icon = "coin", dim = bag[good.name] == 0 }
      end
    end
    frame:menu{ title = "Market", tabs = { "Buy", "Sell" }, tab = menu.tab, rows = rows, at = menu.at,
                hint = "{act} " .. (menu.tab == 1 and "buys" or "sells") .. "   {back} closes" }
  elseif level.state.bag_open then
    local cells = {}
    for i, good in ipairs(GOODS) do
      cells[i] = { icon = "coin", count = bag[good.name], dim = bag[good.name] == 0 }
    end
    for i = #GOODS + 1, 8 do cells[i] = false end
    frame:grid{ title = "Bag", columns = 4, cells = cells }
  elseif near_stall() then
    frame:text("{act} opens the stall", 4, frame.height - 12)
  end
end
"##,
    },
    Topic {
        title: "SPEAK THE PLAYER'S LANGUAGE",
        text: "List the languages your game speaks in conf.lua (c.languages, two small letters each). SETTINGS then offers LANGUAGE, the player's system language at first, and the engine's own menus speak it too (German, English and French). world.language says which the player chose; it can change while playing, so pick your texts by it every picture. Lower case, the letters of German and French and signs such as → or … are all there.",
        conf: r##"c.languages = { "en", "de", "fr" }"##,
        files: &[],
        code: r##"local TEXTS = {
  en = { hello = "Welcome to the farm!", coins = "Coins" },
  de = { hello = "Willkommen auf dem Hof!", coins = "Münzen" },
  fr = { hello = "Bienvenue à la ferme !", coins = "Pièces" },
}

function on_draw(frame)
  local t = TEXTS[world.language]
  frame:text(t.hello, 4, 4)
  frame:text(t.coins .. " → 12", 4, 14)
end
"##,
    },
    Topic {
        title: "ROTATE A MODEL",
        text: "Models stand on the grid in quarter turns unless your game rotates them. A kind with rotates = \"free\" faces exactly the way it goes, a diagonal too. frame:model rotates a model further, in degrees about its middle: rotation (round, clockwise seen from above), pitch (its nose up) and roll (its right side down, as a ship banks to the right). ship.vvx stands for a model of yours.",
        conf: r##"c.player.kind = "ship"
c.things.ship = { model = "ship.vvx", rotates = "free" }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#2a3040")
end

function on_tick()
  player:walk(input.dir, 40)
  -- it leans into the way it goes across
  player.state.lean = input.dir.x * 25
end

function on_draw(frame)
  -- banked by its lean; every 3 seconds a roll right round
  local t = world.tick % 180
  local roll = (player.state.lean or 0) + (t < 30 and t * 12 or 0)
  frame:model("ship.vvx", player, 0, { roll = roll })
end
"##,
    },
    Topic {
        title: "THE CAMERA",
        text: "The camera follows the player. level.camera:follow(thing) makes it follow another thing, level.camera:look_at(x, y) points it at a place, level.camera:follow(player) brings it back.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.flag = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level.state.flag = level:spawn("flag", 90, 32)
end

function on_tick()
  player:walk(input.dir, 40)
  -- every other second the camera looks over to the flag
  if world.tick % 120 == 0 then
    level.camera:follow(level.state.flag)
  elseif world.tick % 120 == 60 then
    level.camera:follow(player)
  end
end
"##,
    },
    Topic {
        title: "A ROOM OF PIECES",
        text: "A level can be a file of pieces: models set down where they stand (on the floor, on a wall or in the air), facing a way, and lights. levels/room.toml says them, each a [[thing]] with its name, its model (or only a light), where it stands and the way it faces; [[place]] is where the player arrives, [[area]] a named box. A piece with solid = true holds the player up and stops it, as the floor and the stove here do; light = { ... } gives it a light, lights = [ ... ] several (the lamp's flame and its glow). The pieces stay where they are; level:thing(name) finds one, and thing.state.light is how bright its lights shine (0 off, 1 on). level.areas.NAME:holds(player) says whether the player stands in the area.",
        conf: r##"c.world.size = nil             -- no world made in code: the level is levels/room.toml
c.actions = { "interact" }
c.keys = { interact = { "E" } }
c.pad = { interact = { "west" } }"##,
        files: &[
            ("levels/room.toml", r##"format = "voxcade level 1"
size = [96, 64, 32]

[[thing]]
name = "floor"
model = "room/floor.vvx"
at = [48, 32, 0]
facing = "north"
solid = true

[[thing]]
name = "stove"
model = "room/stove.vvx"
at = [20, 48, 1]
facing = "south"
solid = true

[[thing]]
name = "lamp"
model = "room/lamp.vvx"
at = [70, 46, 1]
lights = [
  { at = [0, 0, 12], radius = 12, color = "#ffb060" },
  { at = [0, 0, 12], radius = 40, color = "#ffd9a0" },
]

[[place]]
name = "start"
at = [48, 20, 1]
facing = "north"

[[area]]
name = "by_the_stove"
at = [8, 30, 0]
size = [28, 26, 20]
"##),
            ("room/floor.vvx", "format = \"voxcade voxel 1\"\nsize = [96, 64, 1]\n\n[palette]\na = { color = \"#5b4a3a\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *64\n\"\"\"\n"),
            ("room/stove.vvx", "format = \"voxcade voxel 1\"\nsize = [7, 5, 9]\n\n[palette]\ni = { color = \"#3a3d44\" }\nf = { color = \"#e8742c\" }\n\n[[frame]]\nlayers = \"\"\"\niiiiiii *5\n*2\n\niifffii\niiiiiii *4\n*3\n\niiiiiii *5\n*2\n\n.......\n.......\n...i...\n.......\n.......\n*2\n\"\"\"\n"),
            ("room/lamp.vvx", "format = \"voxcade voxel 1\"\nsize = [3, 3, 14]\n\n[palette]\nb = { color = \"#2b2b2f\" }\ns = { color = \"#e9d8a6\" }\n\n[[frame]]\nlayers = \"\"\"\n.b.\nbbb\n.b.\n\n...\n.b.\n...\n*10\n\nsss *3\n*3\n\"\"\"\n"),
        ],
        code: r##"-- The room is levels/room.toml: a floor and a stove that hold the
-- player up, a lamp with its two lights, where the player starts, and an
-- area by the stove.
function on_tick()
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
  -- E by the lamp switches it: how bright its light is, 0 off, 1 on
  local lamp = level:thing("lamp")
  if input:pressed("interact") and player:near(lamp, 16) then
    lamp.state.light = lamp.state.light > 0 and 0 or 1
  end
end

function on_draw(frame)
  for _, p in ipairs(level:players()) do
    frame:model(p.seat == 2 and "hero2.vvx" or "hero.vvx", p)
  end
  if level.areas.by_the_stove:holds(player) then
    frame:text("WARM BY THE STOVE", 4, 4)
  end
end
"##,
    },
    Topic {
        title: "A DOOR THAT OPENS",
        text: "A piece whose model is a figure of parts moves its parts as its state says. door.vvx here has two: its frame, and its leaf, hung on the frame's left post (parent and joint in the file: the hinge). thing.state.parts names each part to move with a rotation about its joint, in degrees, as frame:model's pose: { leaf = { rotation = -90 } } swings the leaf open; nil puts it back at rest. The state lasts as long as you stay in the level, so a door that must stand open the next time you come is kept in world.state and set again in on_level_start.",
        conf: r##"c.world.size = nil             -- no world made in code: the level is levels/hall.toml"##,
        files: &[
            ("levels/hall.toml", r##"format = "voxcade level 1"
size = [64, 48, 24]

[[thing]]
name = "floor"
model = "hall/floor.vvx"
at = [32, 24, 0]
solid = true

[[thing]]
name = "door"
model = "hall/door.vvx"
at = [32, 30, 1]
facing = "north"

[[place]]
name = "start"
at = [32, 12, 1]
facing = "north"
"##),
            ("hall/floor.vvx", "format = \"voxcade voxel 1\"\nsize = [64, 48, 1]\n\n[palette]\na = { color = \"#5b4a3a\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *48\n\"\"\"\n"),
            ("hall/door.vvx", r##"format = "voxcade voxel 1"
size = [9, 1, 12]

[palette]
p = "#6a4a30"
l = "#a07040"
k = "#d0b040"

[[part]]
name = "frame"
at = [-4, 0, 0]
layers = """
p.......p
*11

ppppppppp
"""

[[part]]
name = "leaf"
at = [-3, 0, 0]
parent = "frame"
joint = [-3, 0.5, 5.5]
layers = """
lllllll
*5

lllllkl

lllllll
*5
"""
"##),
        ],
        code: r##"-- The hall is levels/hall.toml: a floor and the door, a piece whose
-- model hall/door.vvx is a figure of two parts, its frame and its leaf.
local function swing(door, angle)
  door.state.angle = angle
  -- each part by its name, turned about its joint: the leaf about the hinge
  door.state.parts = { leaf = { rotation = -angle } }
end

function on_level_start(how)
  -- thing.state lasts this visit of the level, world.state the whole run:
  -- a door opened once stands open whenever the level begins again
  if world.state.door_open then
    swing(level:thing("door"), 90)
  end
end

function on_tick()
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
  -- the player near: it swings open, 3 degrees a tick
  local door = level:thing("door")
  local angle = door.state.angle or 0
  if angle < 90 and player:near(door, 24) then
    swing(door, math.min(angle + 3, 90))
    if door.state.angle == 90 then
      world.state.door_open = true
    end
  end
end

function on_draw(frame)
  for _, p in ipairs(level:players()) do
    frame:model(p.seat == 2 and "hero2.vvx" or "hero.vvx", p)
  end
end
"##,
    },
    Topic {
        title: "THE LEVEL EDITOR",
        text: "EDIT on a level (or a double click on it under LEVELS) opens it in a window of its own.
# THE THREE ZONES
The top bar: BLOCKS, THINGS and PIECES (a level of blocks), + ADD, UNDO, REDO, and on the right LEVEL and GAME VIEW.
The outliner on the left: THINGS, PLACES and AREAS, each folding at its heading.
The inspector on the right: the chosen one's rows, POSITION, ROTATE, its light.
# CHOOSING AND MOVING
A click in the view chooses what it meets, voxel by voxel; a click on empty floor chooses nothing.
The bar at the chosen one: MOVE shows its arrows, drag one to move it along that axis.
ROTATE shows its rings: drag one to rotate it about that axis. EDIT opens its model in the voxel editor.
The number fields take a drag sideways, or a click and typing (Enter sets, Escape leaves it).
# THE MENU
A right click on a thing or on the floor opens its menu; every entry is also a button somewhere.
# ADDING
+ ADD (or a right click on the floor): a model of the game, a light, a place, an area.
# HIDE MARKS
The mark at a row's left hides that thing in the editor only; SHOW ALL at the heading shows them again.
# GAME VIEW
The level as the game's camera sees it; press it again to go back.
# SAVING
Every step is saved at once. UNDO takes it back, REDO does it again.
A file changed by another program (an agent, your text editor) is read anew; the editor never writes an older state over it.",
        conf: "",
        files: &[],
        code: "",
    },
    Topic {
        title: "THE VOXEL EDITOR",
        text: "EDIT on a model (.vvx) opens it in a window of its own: a model of frames, or a figure of parts.
# THE THREE ZONES
The top bar: the model's name, UNDO, REDO, and on the right POSE and GAME VIEW (a figure).
The outliner on the left: the frames, or the figure whole and its parts by what they hang on.
The inspector on the right: the part chosen, the tools, the colors and the held color's material.
# TOOLS
ADD, ERASE and PAINT work on the voxel clicked; FILL paints the connected voxels of its color (Ctrl: all of it).
BOX and SPHERE: drag a footprint on a face; + and - make it deeper while it is fresh. Alt: only where empty, Ctrl: cut.
PICK (or Shift held) takes up a color. SYMMETRY mirrors every tool left to right.
# SELECT
Drag a box over voxels, or double click one for its color connected; Ctrl adds, Escape clears.
Its arrows move the voxels; SELECTION in the inspector and a right click on them hold the rest: DUPLICATE, REMOVE, PAINT, FLIP, ROTATE, NEW PART, MOVE TO PART, COPY, PASTE.
# PARTS
A click on a part's row (or a double click in the view) chooses it; the figure whole is the first row.
MOVE | ROTATE at a part: its arrows move it with what hangs on it, its rings rotate it about its joint.
PARENT and JOINT in the inspector set what it hangs on and where it turns.
A right click on a row: ADD PART, DUPLICATE, MIRROR (for NAME_left and NAME_right), RENAME, MERGE, REMOVE.
# POSE
The rings turn parts on top of their rest, nothing saved. COPY gives the pose as Lua for frame:model.
# GAME VIEW
The figure as the game draws it: its camera, light, shading and outline.
# SLICE AND CONTEXT
SLICE cuts the model across an axis to reach inside; drag its arrow or use Page Up and Page Down.
CONTEXT (a model opened from a level's piece) shows the level round it, faint.
# HIDE MARKS
The mark at a part's row hides it from the picture; SHOW ALL at the PARTS heading shows every part.
# COLORS
A click on a swatch holds it; a right click: REPLACE WITH HELD, or REMOVE for a color no voxel has.
# SAVING
Every step is saved at once. UNDO takes it back, REDO does it again.
A file changed by another program is read anew; the editor never writes an older state over it.",
        conf: "",
        files: &[],
        code: "",
    },
    Topic {
        title: "CLAUDE IN THE CONSOLE",
        text: "CLAUDE in the console below starts Claude Code, an AI agent that helps you make your game: it reads and changes the game's files and sees the IDE. It is Anthropic's program, not a part of Voxcade: install it yourself (claude.com/claude-code). It needs a Claude subscription or an API key; you sign in the first time it starts in the console, and /login there changes the account later. The gear at the top right says whether it starts with the IDE.",
        conf: "",
        files: &[],
        code: "",
    },
];

/// The first topic's title: every name of the API with what it does.
const ALL: &str = "REFERENCE";

/// HELP in `ui`: the topics on the left, the one `chosen` (its place in
/// that list: 0 every name of the API, then TOPICS from 1) beside them.
pub fn ui(ui: &mut egui::Ui, chosen: &mut usize) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(250.0);
            ui.label(RichText::new("HELP").small().color(QUIET));
            egui::ScrollArea::vertical().id_salt("help topics").auto_shrink([false, false]).content_margin(egui::Margin { right: 6, bottom: 14, ..egui::Margin::ZERO }).show(ui, |ui| {
                // The reference first, then the examples.
                for (i, title) in std::iter::once(ALL).chain(TOPICS.iter().map(|t| t.title)).enumerate() {
                    let on = *chosen == i;
                    if crate::gui::row(ui, title, TextStyle::Body, if on { ACCENT } else { TEXT }, on, None).clicked() {
                        *chosen = i;
                    }
                }
            });
        });
        ui.add_space(20.0);
        ui.vertical(|ui| {
            egui::ScrollArea::vertical().id_salt(("help", *chosen)).auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO }).show(ui, |ui| match chosen.checked_sub(1).and_then(|n| TOPICS.get(n)) {
                Some(topic) => {
                    ui.label(RichText::new(topic.title).color(ACCENT));
                    ui.add_space(8.0);
                    // A line "# WORD" is a heading (the editors' pages:
                    // headings and one line each).
                    for line in topic.text.lines() {
                        match line.strip_prefix("# ") {
                            Some(heading) => {
                                ui.add_space(8.0);
                                ui.label(RichText::new(heading).small().color(QUIET));
                            }
                            None => {
                                ui.label(line);
                            }
                        }
                    }
                    ui.add_space(12.0);
                    if !topic.conf.is_empty() {
                        ui.label(RichText::new("IN CONF.LUA, INSIDE function conf(c)").small().color(QUIET));
                        code(ui, topic.conf);
                        ui.add_space(12.0);
                    }
                    // (A topic about the IDE itself has no code.)
                    if !topic.code.is_empty() {
                        ui.label(RichText::new("MAIN.LUA").small().color(QUIET));
                        code(ui, topic.code);
                    }
                    for (name, text) in topic.files {
                        ui.add_space(12.0);
                        ui.label(RichText::new(name.to_uppercase()).small().color(QUIET));
                        code(ui, text);
                    }
                }
                None => {
                    ui.label(RichText::new(ALL).color(ACCENT));
                    ui.add_space(8.0);
                    ui.label("Every name the API has, with what it does. Hold the mouse over a name in your code for the same.");
                    ui.add_space(12.0);
                    // Each name as it is written, what it does under it.
                    for item in crate::lua::api::API {
                        ui.label(RichText::new(crate::lua::api::signature(item)).monospace().color(TEXT));
                        ui.label(RichText::new(item.doc).color(QUIET));
                        ui.add_space(8.0);
                    }
                }
            });
        });
    });
}

/// A block of Lua to read, select and copy (COPY: all of it).
fn code(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(PANEL).corner_radius(4.0).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            let font = TextStyle::Monospace.resolve(ui.style());
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| ui.fonts_mut(|f| f.layout_job(super::code::highlight(text.as_str(), &font, super::code::Syntax::Lua)));
            let mut shown = text;
            ui.add(egui::TextEdit::multiline(&mut shown).code_editor().frame(egui::Frame::NONE).desired_rows(1).desired_width(ui.available_width() - 80.0).layouter(&mut layouter));
            if ui.button("COPY").on_hover_text("Copies all of it, to paste into your file.").clicked() {
                ui.ctx().copy_text(text.to_string());
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every example runs as a game: the empty template's conf.lua with
    /// the topic's lines, the topic's main.lua, played with the arrows and
    /// the actions held now and then, drawn every second, without an error.
    #[test]
    fn every_example_runs() {
        let base = include_str!("../../templates/empty/conf.lua");
        let at = base.rfind("end").expect("conf ends");
        for (i, topic) in TOPICS.iter().enumerate() {
            let dir = crate::scratch::Scratch::new(&format!("help-{i}"));
            let conf = format!("{}  {}\n{}", &base[..at], topic.conf.replace('\n', "\n  "), &base[at..]);
            std::fs::write(dir.join("conf.lua"), conf).unwrap();
            // The template's figures, as a game begun from it has them.
            for model in ["hero.vvx", "hero2.vvx"] {
                std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/empty").join(model), dir.join(model)).unwrap();
            }
            std::fs::write(dir.join("main.lua"), topic.code).unwrap();
            for (name, text) in topic.files {
                std::fs::create_dir_all(dir.join(name).parent().unwrap()).unwrap();
                std::fs::write(dir.join(name), text).unwrap();
            }
            // A model the example names stands for one of the reader's.
            for model in topic.conf.split('"').filter(|w| w.ends_with(".vvx")) {
                std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena/player.vvx"), dir.join(model)).unwrap();
            }
            let mut game = crate::session::Session::<crate::Voxcade>::load(&dir).unwrap_or_else(|e| panic!("{}: {e:#}", topic.title));
            for tick in 0..240 {
                let mut input = crate::input::Input { right: tick % 90 < 45, up: tick % 70 < 30, ..Default::default() };
                input.hold(0, tick % 50 < 3);
                // A second player, where the game has seats for one: in,
                // walking, then gone again.
                game.set_seat(2, (30..200).contains(&tick).then_some(crate::input::Input { left: tick % 80 < 40, ..Default::default() }));
                game.step(&input);
                // a picture after every tick, as a window draws: on_draw
                // only looks, and a mistake there shows in the frame after
                // the tick that set it up (BunnyFarm's dog's board did)
                let _ = game.compose(0.0, 16.0 / 9.0, crate::session::Scene::Play, None);
            }
            assert!(game.trouble().is_none(), "{}: {:?}", topic.title, game.trouble());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

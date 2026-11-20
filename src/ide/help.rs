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
  level:spawn("door", 90, 32)
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
        text: "A game's levels are the files in its levels/ folder, in the order of their names (01_meadow.toml, 02_cave.toml), a run beginning in the first unless c.start = \"02_cave\" in conf.lua names another; without any it has one, made in code like this one. world:go_to(level.next) goes on to the next at the end of the tick, world:win() wins after the last, world:restart() starts this one over. on_level_start(how) says how a level was entered; level.state is fresh each time, world.state and player.state go along, and so does the player. Between two levels the picture fades out and in (c.transition: 0.3 seconds each way unless set); an ask's last argument says it for that one change: { fade = 1 } a slow fade, as between chapters, { fade = 0 } a cut, the next picture already the new level.",
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
  -- at the far right: on to the next level slowly, after the last the game is won
  if player.x > level.size.x - 4 then
    if level.next then world:go_to(level.next, { fade = 1 }) else world:win() end
  end
  -- out of time: this level again from its start, a cut
  level.state.time = level.state.time - 1
  if level.state.time == 0 then world:restart({ fade = 0 }) end
end

function on_draw(frame)
  frame:text("TIME " .. level.state.time // 60 .. "  TRIES " .. (player.state.tries or 0), 4, 4)
end
"##,
    },
    Topic {
        title: "DOORS WITH PROPS",
        text: "A kind declares its props in conf.lua, each with its default: c.things.door.props = { to = \"01_hall\", at = \"start\", locked = false }. A door set down in a level's file gives its own over those, props = { to = \"02_crypt\", at = \"stairs\" } on its [[thing]] (in the level editor: PROPS in its rows), and thing.props reads them, the kind's where it gives none. Each must be one its kind declares, of the same type (a number, a string, true or false): a misspelt one is a mistake when the game loads, naming the file and the line. The kind's names say which props name something: to a level, at a place in the level to names (\"start\" is every level's); \"thing\" and \"things\" would name things of the door's own level. A name that is not there is a mistake when the game loads too, with the nearest there is, and the level editor offers the names there are. Here each door leads where its props say, and a locked one stays shut.",
        conf: r##"c.world.size = nil             -- no world made in code: the levels are in levels/
c.things.door = { props = { to = "01_hall", at = "start", locked = false },
                  names = { to = "level", at = "place" } }"##,
        files: &[
            ("levels/01_hall.toml", r##"format = "voxcade level 1"
size = [64, 48, 16]

[[thing]]
name = "ground"
model = "ground.vvx"
at = [32, 24, 0]
solid = true

[[thing]]
kind = "door"
at = [56, 24, 1]
props = { to = "02_crypt", at = "stairs" }

[[thing]]
kind = "door"
at = [8, 24, 1]
props = { to = "02_crypt", locked = true }

[[place]]
name = "start"
at = [32, 24, 1]
"##),
            ("levels/02_crypt.toml", r##"format = "voxcade level 1"
size = [64, 48, 16]

[[thing]]
name = "ground"
model = "ground.vvx"
at = [32, 24, 0]
solid = true

[[thing]]
kind = "door"
at = [8, 24, 1]

[[place]]
name = "stairs"
at = [16, 24, 1]
facing = "east"
"##),
            ("ground.vvx", "format = \"voxcade voxel 1\"\nsize = [64, 48, 1]\n\n[palette]\na = { color = \"#5a5a66\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *48\n\"\"\"\n"),
        ],
        code: r##"function on_touch(a, b)
  if b.kind ~= "door" then return end
  if b.props.locked then
    level.state.said = "LOCKED"
  else
    -- where this door leads: its own props, else its kind's
    world:go_to(b.props.to, b.props.at)
  end
end

function on_tick()
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
end

function on_draw(frame)
  for _, d in ipairs(level:things("door")) do
    local colour = d.props.locked and { 150, 60, 60 } or { 200, 160, 90 }
    for z = 0, 9 do frame:cube(d.x, d.y, d.z + z, colour) end
  end
  for _, p in ipairs(level:players()) do
    frame:model(p.seat == 2 and "hero2.vvx" or "hero.vvx", p)
  end
  frame:text(string.upper(level.name) .. "  " .. (level.state.said or ""), 4, 4)
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
        title: "YOUR OWN TITLE",
        text: "A game with save games may open on a title of its own: a level of the game (levels/title.toml here) that c.title_screen.level names, in place of the engine's title with its high scores. The window opens in it, and what happens there is your code: here a menu drawn with frame:menu and moved with the arrows. world:new_game() begins a run in the level a run begins in (the first but the title, or c.start's), world:load(slot) a run from a save (world.saves says what lies in the slots, world.saves.newest which was saved last: CONTINUE is world:load(world.saves.newest), nil while there is none), world:save(slot, { label = \"Meadow\" }) tells a save by a line of your own (world.saves[slot].label), world:delete_save(slot) empties a slot at the tick's end, world:open(\"settings\") shows the engine's SETTINGS over the game, world:quit() ends the program, and in a run world:to_title() goes back to the title (your own QUIT TO TITLE). The title is no run: it is never recorded, nothing is saved there, Escape is yours. level.name says where the game is. A run's end and QUIT TO TITLE come back to it; PLAY in the IDE begins the run at once (a scenario with level = \"title\" plays the title).",
        conf: r##"c.world.size = nil
c.title_screen.level = "title"
c.saves = { slots = 1 }
c.actions = { "act", "back" }
c.keys = { act = { "Space" }, back = { "X" } }
c.pad = { act = { "south" }, back = { "east" } }"##,
        files: &[
            ("levels/title.toml", "format = \"voxcade level 1\"\nsize = [96, 64, 32]\n"),
            ("levels/meadow.toml", "format = \"voxcade level 1\"\nsize = [96, 64, 32]\n"),
        ],
        code: r##"local ROWS = { "New game", "Continue", "Settings", "Quit" }

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  if level.name == "title" then
    level.state.at = 1
  end
end

-- The title's menu: up and down choose, act does what the row says.
local function title_tick()
  local at = level.state.at
  if input:repeated("down") then at = at % #ROWS + 1 end
  if input:repeated("up") then at = (at - 2) % #ROWS + 1 end
  level.state.at = at
  if input:pressed("act") then
    if at == 1 then world:new_game()
    elseif at == 2 and world.saves.newest then world:load(world.saves.newest)
    elseif at == 3 then world:open("settings")
    elseif at == 4 then world:quit() end
  end
end

function on_tick()
  if level.name == "title" then return title_tick() end
  player:walk(input.dir, 40)
  if input:pressed("act") then world:save(1, { label = "Meadow" }) end
  if input:pressed("back") then world:to_title() end
end

function on_draw(frame)
  if level.name == "title" then
    local rows = {}
    for i, text in ipairs(ROWS) do
      rows[i] = { text = text, dim = i == 2 and not world.saves.newest }
    end
    frame:menu{ title = "My Game", rows = rows, at = level.state.at, hint = "{act} chooses" }
  else
    frame:text("{act} saves, {back} to the title", 4, 4)
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
        text: "A sound is a file of your game in the folder sounds (the plus beside SOUNDS makes one; over its text PLAY plays it and LOOP again and again, COIN, JUMP and the other sorts roll a fresh one each time they are pressed, UNDO goes back, and the faders change it), a few numbers a line: a wave (square, saw, triangle, sine or noise), a pitch in Hz, how long it holds and fades in seconds, how loud from 0 to 1. Only what differs from the defaults is written; sweep slides the pitch and jump lifts it once, both in semitones (12 an octave). The file's name is the sound's: play it by that name at a place, a thing or a point. Recordings (.wav) are not played: everything in a game is code or text.",
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
        text: "Play a song of c.audio.music as your game's own (music:play, by its file's name) and the game can go with it: on_beat runs on each of its beats, before on_tick, and music.bar counts its bars. Here a blip comes on the first beat of every bar and the screen glows on each beat. The song keeps time with the game (a pause holds both), so replays play it the same. music:play(\"drive\", { at = 33 }) begins a song at its bar 33, past an intro, its beat 0 there; the beats count on and on as the song goes round, round its #loop too. c.title_screen.music_at = 33 begins the title's song so.",
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
        title: "A SONG IN MML",
        text: "A song is a text file in the folder music, ending in .mml (the plus beside MUSIC makes one; PLAY SONG plays the text as it stands). Lines starting with # are settings (#tempo is beats a minute, 1 to 999, #meter the beats in a bar, #loop 33 80 plays the song from its start once and then bars 33 to 80 round and round, counted from 1, what comes after them never reached, #loop 33 goes round from bar 33 to the end and #loop no plays it once; a note held over the loop's end is cut there), lines starting with @ make an instrument (a wave, square, saw, triangle, sine or noise, then options like attack, decay, sustain and release, written name=number; a number out of its range is a mistake saying the range, a word one slip off says which it meant), and a line like A: is a channel of notes, one voice each (@lead in it picks the instrument; without one it plays a plain square). Notes are c d e f g a b (or C D E F G A B), + or - for sharp and flat (++ or -- twice), then a length (4 a quarter, 8 an eighth, a dot makes it longer), & holds a note on by another length (c4&8, or c4&c8: the same pitch), r a rest, o4 the octave (0 to 9), < and > one octave down and up, l8 the default length, v0 to v15 the volume, [ ... ]3 repeats three times, $name = ... a short piece to use again, | a bar line (ignored). A channel may go on over several lines; a mistake says the line it is on. Every channel should be as long as the others, or the short ones fall silent before the song starts over. Name the file in c.audio.music and play it by its file's name, without the folder and .mml: music:play(\"drive\"). decay in an instrument is the fall from the peak to the sustain level, not the fade to silence a sound file's decay is. An instrument can sing: @singer voice in place of a wave. Then a text in quotes before a note is the syllable that note sings, its phonemes split by spaces, in small letters: \"l a\" c4, \"s ae ng\" e4 (consonants, one vowel, consonants). The vowel starts on the note's beat, the consonants before it sound just before the beat, the ones after it end where the note is let go. A voice note without a text holds the vowel before it (a melisma). The phonemes: vowels a ae e eh i ih o u ax oe ue, diphthongs aw ow ai oi ei, consonants m n ng l r w j f v s z sh ch h t k p d g b (ae as in cat, eh bed, ih bit, ax the a in sofa, oe and ue the German ö and ü, ng sing, j yes, sh she, ch the German ich). Every instrument, a voice too, takes drive (decibels, 0 to 36: pushed into a soft clipper, 12 rough, 20 a shout through a megaphone with filter=bandpass cutoff=1150) and a bit crusher, crush_rate (the sound held at that many Hz, 6000 the speech of old games) and crush_bits (1 to 16 bits).",
        conf: r##"c.audio.music = { "music/drive.mml" }
c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[(
            "music/drive.mml",
            "#title Drive\n#tempo 120\n#meter 4\n\n; the instruments\n@lead square duty=0.25 attack=0.005 decay=0.15 sustain=0.6 release=0.1 volume=0.4\n@bass triangle volume=0.5\n\n; a piece to use twice\n$riff = c8 c8 g8 c8 a+8 c8 g8 c8\n\n; a voice, rough and through a megaphone's band\n@singer voice attack=0.005 decay=0 sustain=1 release=0.05 vibrato=0.006 vibrato_speed=5.5 drive=12 filter=bandpass cutoff=1150 volume=0.4\n\n; every channel four bars long (16 beats)\nA: @lead o5 v12 l8 [e g a g e d c d]4\nB: @bass o2 v13 [$riff]4\n; the voice from bar 3: la la la, hey hey, sing\nV: @singer o4 r1 r1 \"l a\" e4 \"l a\" g4 \"l a\" a2 | \"h ei\" g4 \"h ei\" e4 \"s ih ng\" d2\n",
        )],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#30343c")
  -- by the file's name, without the folder and .mml
  music:play("drive", "now")
end

function on_tick()
  player:walk(input.dir, 40)
end

function on_draw(frame)
  frame:cube(player.x, player.y, player.z, { 200, 90, 60 })
end
"##,
    },
    Topic {
        title: "MUSIC THROUGH A WALL",
        text: "How the music sounds where the player is. music:volume(level, seconds) makes the song quieter, a part of its own loudness from 0 (silent) to 1 (as loud as it is), and music:muffle(amount, seconds) makes it dull, as heard through a wall or a door, from 0 (clear) to 1 (very dull). Each glides there over the seconds (0.3 unless you say; 0 at once) and stays until you change it, from level to level too. Here the street's song is quiet and dull outside and clear in the house (an area of levels/street.toml). The shop has a song of its own: music:play(\"shop\", { from = \"now\", crossfade = 3 }) fades the street's song out as the shop's comes in over three seconds, and back again as you leave (c.audio.crossfade sets it for every change). A song asked for while it plays goes on, through a level's fade too. Your game's sounds stay clear.",
        conf: r##"c.world.size = nil             -- no world made in code: the level is levels/street.toml
c.audio.music = { "music/street.mml", "music/shop.mml" }"##,
        files: &[
            ("levels/street.toml", r##"format = "voxcade level 1"
size = [64, 48, 16]

[[thing]]
name = "ground"
model = "street/ground.vvx"
at = [32, 24, 0]
solid = true

[[place]]
name = "start"
at = [32, 4, 1]
facing = "north"

[[area]]
name = "house"
at = [4, 12, 0]
size = [24, 24, 16]

[[area]]
name = "shop"
at = [36, 12, 0]
size = [24, 24, 16]
"##),
            ("street/ground.vvx", "format = \"voxcade voxel 1\"\nsize = [64, 48, 1]\n\n[palette]\na = { color = \"#4f5a3c\" }\nh = { color = \"#8a6a4a\" }\ns = { color = \"#4a5f8a\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *12\naaaahhhhhhhhhhhhhhhhhhhhhhhhaaaaaaaassssssssssssssssssssssssaaaa *24\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *12\n\"\"\"\n"),
            ("music/street.mml", "#tempo 100\n@pad triangle attack=0.05 release=0.3 volume=0.5\n@bass square duty=0.25 volume=0.3\nA: @pad o4 l2 c e g e\nB: @bass o2 l4 c c g g c c g g\n"),
            ("music/shop.mml", "#tempo 120\n@bell sine attack=0.01 decay=0.4 sustain=0.2 release=0.3 volume=0.5\n@bass triangle volume=0.4\nA: @bell o5 l4 e g a g e d c d\nB: @bass o3 l2 a f c g\n"),
        ],
        code: r##"-- Where the player stands says what plays and how: outside, the
-- street's song quiet and dull through the walls; in the house, loud and
-- clear; in the shop, the shop's own song, crossfading in.
local function room_of(p)
  if level.areas.house:holds(p) then return "house" end
  if level.areas.shop:holds(p) then return "shop" end
  return "outside"
end

function on_level_start(how)
  music:play("street", "now")
  level.state.room = "outside"
  music:volume(0.5, 0)   -- at once: the level begins outside
  music:muffle(0.7, 0)
end

function on_tick()
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
  local room = room_of(player)
  if room ~= level.state.room then
    level.state.room = room
    -- the shop's song in the shop, else the street's (going on if it plays)
    local song = room == "shop" and "shop" or "street"
    music:play(song, { from = "now", crossfade = 3 })
    if room == "outside" then
      music:volume(0.5, 1)   -- over a second
      music:muffle(0.7, 1)
    else
      music:volume(1, 1)
      music:muffle(0, 1)
    end
  end
end

function on_draw(frame)
  for _, p in ipairs(level:players()) do
    frame:model(p.seat == 2 and "hero2.vvx" or "hero.vvx", p)
  end
  frame:text(string.upper(level.state.room), 4, 4)
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
        text: "frame:light puts a light into this picture: walls stop it (the level's voxels and its pieces). With a table it takes more words: cone makes a spot (its opening angle in degrees), blend how soft the spot's edge is (0 to 1, 0.15 unless set), rotation and pitch point it (pitch below 0 points down), falloff says how it fades (2 unless set), fog how strongly it shows in fog (1 unless set, FOG AND BEAMS), shadows says what stops it: \"none\" lets it shine through walls, \"static\" has the level's voxels and its pieces stop it, \"all\" the figures, things and models drawn too, so they cast its shadow (it costs more: for a few lamps), but a light at a thing is not stopped by that thing's own models (the lantern a figure carries); left out, c.light.lamp_shadows says, \"static\" unless set. A lamp that stays in one place belongs in the level's file: LAMPS IN A LEVEL. level:explode blows a hole into the level, screen:shake shakes the screen, screen:flash flashes it, world:hitstop holds the game still for a few ticks.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 1, "#5a6e4a")
  level:fill(40, 20, 2, 41, 44, 10, "#a08060")   -- a wall: it stops the lamps
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
  -- a spot by the wall, shining down and south
  frame:light{ at = { x = 44, y = 40, z = 14 }, radius = 30, color = { 140, 200, 255 }, cone = 50, rotation = 180, pitch = -60 }
end
"##,
    },
    Topic {
        title: "FOG AND BEAMS",
        text: "c.light.fog_density fills the level's air with fog: the lamps' light shows in it, a spot's cone as a beam, a lamp's ball as a glow, and what lies further fades to c.light.fog_color. Walls stop a beam where they stop the lamp, so a pillar cuts a dark slot into it. 0.004 is a light haze in a big hall, 0.03 thick fog in a small room. By day give c.light.fog_color the sky's color, or what lies further fades to a dark veil; here it is night, so a dark fog is right. A light's fog says how strongly it shows (1 unless set, 0 not at all, up to 10 for a stage spot). frame:fog sets the fog for one picture (density, color, anisotropy, glow), what it leaves out c.light's: here the haze thickens and thins every few seconds. Above 0 anisotropy makes a beam brighter looking towards its lamp. c.light.fog_glow shows the beams more brightly than the fog dims: 4 with a quarter of the density is a stage's haze, bright beams in a clear room. What makes the beams show in this example is the low ambient light (a dark room) and the fog_glow of 3. Only the picture: the game's rules do not see it, and a player can turn it off (SETTINGS, VIDEO, FOG).",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.light.ambient = 0.15
c.light.sun_color = { 0.3, 0.35, 0.5 }
c.light.fog_density = 0.004
c.light.fog_glow = 3
c.light.fog_color = { 0.04, 0.04, 0.06 }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 1, "#3a3a44")
  level:fill(46, 30, 2, 49, 33, 24, "#706870")   -- a pillar in the beam
end

function on_tick()
  player:walk(input.dir, 40)
end

function on_draw(frame)
  -- a spot high up, shining down past the pillar, bright in the fog
  frame:light{ at = { x = 36, y = 32, z = 30 }, radius = 70, color = { 255, 220, 160 },
    cone = 40, rotation = 90, pitch = -50, fog = 3 }
  -- the haze thickening and thinning
  local t = (world.tick + frame.alpha) / 60
  frame:fog{ density = 0.004 + 0.002 * math.sin(t) }
end
"##,
    },
    Topic {
        title: "LAMPS IN A LEVEL",
        text: "A lamp that stays where it is belongs in the level's file: a [[thing]] with light = { ... } and no model is a light alone, and a piece (a model set down) may bring one too, or several as lights = [ { ... }, { ... } ]. In the level editor the same words are in the rows of a light. The words, all but radius optional: at is where it shines from, three whole numbers in voxels from the thing's place (from the model's pivot for a piece, turning with it), [0, 0, 0] unless set. radius is how far it reaches, voxels, above 0, needed. color is \"#rrggbb\", white unless set. falloff is how it fades towards its radius, from 0 to 16, 2 unless set: lower reaches further bright, higher is a small bright core. cone makes a spot: its whole opening angle in degrees, from 1 to 360; left out it shines every way, as the first lamp here. blend is how much of the cone's edge is soft, from 0 to 1, 0.15 unless set. rotation is the way a spot points round, degrees clockwise seen from above, north (+y) 0, east 90, south 180, from -360 to 360, 0 unless set. pitch is its way up or down in degrees from -90 to 90: below 0 points down, -90 straight down, 0 level, above 0 up. shadows says what stops it: \"static\" the level's voxels and its pieces (the wall here casts a shadow), \"all\" moving things too (costly: for a few lamps), \"none\" nothing (it shines through walls); left out, c.light.lamp_shadows says, \"static\" unless set. fog from 0 to 10, 1 unless set, is how strongly its light shows in fog (0 not at all; the example has a little fog, c.light.fog_density, so the spot's beam shows). Glass neither stops a lamp nor tints it. Lights that move are frame:light in on_draw (LIGHT, SHAKE AND FLASH). Only the picture: the game's rules do not see lights.",
        conf: r##"c.world.size = nil             -- no world made in code: the level is levels/hall.toml
c.light.ambient = 0.2
c.light.sun_color = { 0.25, 0.28, 0.4 }
c.light.fog_density = 0.004
c.light.fog_glow = 3
c.light.fog_color = { 0.04, 0.04, 0.06 }"##,
        files: &[
            ("levels/hall.toml", r##"format = "voxcade level 1"
size = [96, 64, 32]

[[thing]]
name = "ground"
model = "ground.vvx"
at = [48, 32, 0]
solid = true

[[thing]]
name = "wall"
model = "wall.vvx"
at = [56, 32, 1]
solid = true

# A lamp shining every way: no cone. Every word it takes, but the spot's.
[[thing]]
name = "lamp"
at = [30, 32, 12]
light = { at = [0, 0, 3], radius = 40, color = "#ffb870", falloff = 2, shadows = "static", fog = 1 }

# A spot: a cone 40 degrees wide with a soft edge, pointing south and
# down at the middle of the floor, its beam bright in the fog.
[[thing]]
name = "spot"
at = [20, 56, 26]
light = { at = [0, 0, 0], radius = 70, color = "#a8c8ff", falloff = 1, cone = 40, blend = 0.2, rotation = 180, pitch = -50, shadows = "static", fog = 3 }
"##),
            ("ground.vvx", "format = \"voxcade voxel 1\"\nsize = [96, 64, 1]\n\n[palette]\na = { color = \"#5a5a66\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *64\n\"\"\"\n"),
            ("wall.vvx", "format = \"voxcade voxel 1\"\nsize = [3, 24, 14]\n\n[palette]\na = { color = \"#8a7a70\" }\n\n[[frame]]\nlayers = \"\"\"\naaa *24\n*14\n\"\"\"\n"),
        ],
        code: r##"function on_tick()
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 40)
  end
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
        title: "A SCENE WITH BARS",
        text: "c.camera.aspect = 2.39 gives every picture a film's shape: the widest rectangle of it the window holds, black bars on the rest, the HUD and the menus inside it. frame:camera in on_draw sets this picture's camera: look_at the point the view centers on, width how many voxels it shows across there, pitch how steep it looks down; what you leave out is c.camera's, so { width = 60 } alone zooms in on the player and keeps following. Each picture is its own: call it in every picture the scene lasts and work a pan out from your clock; a picture without it is the game's camera again. Here a slow pan along the meadow, a cut to the house's door, then the game's camera on the player, and round again. Only the picture: the game's rules do not see it.",
        conf: r##"c.player.kind = "hero"
c.things.hero = { model = "hero.vvx" }
c.camera.aspect = 2.39"##,
        files: &[],
        code: r##"local SCENE = 20 * 60   -- the whole scene, in ticks

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level:fill(30, 40, 1, 40, 48, 8, "#c8b090")    -- a little house
  level:fill(34, 40, 1, 36, 40, 4, nil)          -- its door
  level:fill(29, 39, 9, 41, 49, 10, "#a04030")   -- its roof
  level:fill(66, 30, 1, 67, 31, 9, "#6b5138")    -- a tree
  level:fill(63, 27, 10, 70, 34, 15, "#3f7a3a")
end

function on_tick()
  player:walk(input.dir, 40)
end

function on_draw(frame)
  frame:model("hero.vvx", player)
  local t = (world.tick + frame.alpha) % SCENE / 60   -- seconds into the scene
  if t < 8 then
    -- a slow pan along the meadow, wide
    frame:camera{ look_at = { 16 + t * 8, 40, 0 }, width = 90 }
  elseif t < 12 then
    -- a cut: close on the house's door, a little steeper
    frame:camera{ look_at = { 35, 40, 3 }, width = 30, pitch = 60 }
  end
  -- the rest of the scene: the game's own camera, following the player
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
        title: "POINT AND CLICK",
        text: "In a game with a pointer (c.pointer) input.pointer says what the pointer is over in the level too: thing (the thing whose drawn voxels are under it, a figure as posed, a level's piece, a model drawn with frame:model), part (that figure's part by name), area (the level's area it enters first, a door in a wall), at and cell (where it meets the level: the ground to walk to, a cell of a grid). A click carries what was under the pointer as the button went down, and a thing gone since is nil. In on_draw frame.pointer says the same of the picture before: a name over a thing while the pointer rests on it. input.aim is the way from the player's figure to the pointer. Click the ground to walk, the chest to open it.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.things.chest = {}
c.pointer = "system"
c.actions = { "click" }
c.keys = { click = { "mouse_left" } }
c.pad = { click = { "south" } }"##,
        files: &[],
        code: r##"function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
  level:spawn("chest", 60, 40)
  level.state.said = "Click the ground to walk, the chest to open it"
end

function on_tick()
  local p = input.pointer
  if input:pressed("click") and p then
    if p.thing and p.thing.kind == "chest" then
      level.state.said = p.thing.state.open and "It is empty" or "Gold!"
      p.thing.state.open = true
    elseif p.cell then
      level.state.walk_to = { x = p.cell.x, y = p.cell.y }
    end
  end
  -- walking to the point clicked, straight
  local to = level.state.walk_to
  if to then
    local dx, dy = to.x - player.x, to.y - player.y
    if math.abs(dx) + math.abs(dy) < 2 then
      level.state.walk_to = nil
    else
      player:walk({ x = dx, y = dy }, 40)
    end
  end
end

function on_draw(frame)
  frame:model("hero.vvx", player)
  for _, t in ipairs(level:things()) do
    if t.kind == "chest" then frame:model("hero2.vvx", t) end
  end
  frame:text(level.state.said, 4, 4)
  local p = frame.pointer
  if p and p.thing and p.thing.kind == "chest" then frame:text("open", p.x + 6, p.y) end
end
"##,
    },
    Topic {
        title: "CLICKS AND A NAME",
        text: "c.pointer = \"system\" gives your game a pointer: the mouse moves it, and its buttons and wheel are keys you bind as any (c.keys.click = { \"mouse_left\" }). frame:hud names a rectangle of the screen, and a menu or grid with a name is one too: input.pointer.hud says which the pointer is over, index its row or slot, u and v where in it. A click carries what was under the pointer as the button went down. Typing: player.state.typing = true, then input.typed says what came each tick (text, erase, done, cancel), and the keys that type do nothing else meanwhile. c.pointer = \"own\" hides the arrow while the game plays: draw your own from frame.pointer. input.move is the left stick (or the directions) at any angle.",
        conf: r##"c.player.kind = "hero"
c.things.hero = {}
c.pointer = "system"
c.actions = { "click", "next" }
c.keys = { click = { "mouse_left" }, next = { "wheel_down", "Tab" } }
c.pad = { click = { "south" }, next = { "rightshoulder" } }"##,
        files: &[],
        code: r##"local SEEDS = { "Carrot", "Wheat", "Tomato" }

function on_world_start()
  world.state.name = ""
  world.state.seed = 1
  world.state.planted = 0
  player.state.typing = true   -- a name first
end

function on_level_start(how)
  level:fill(0, 0, 0, level.size.x - 1, level.size.y - 1, 0, "#5a6e4a")
end

function on_tick()
  local typed = input.typed
  if typed then
    -- the letters typed this tick, Backspace, Enter
    local name = world.state.name .. typed.text
    for _ = 1, typed.erase do name = name:sub(1, -2) end
    world.state.name = name:sub(1, 12)
    if typed.done and world.state.name ~= "" then player.state.typing = false end
    return
  end
  -- walking at any angle, with a stick or the arrows
  local move = input.move
  player:walk({ x = move.x, y = move.y }, 40)
  if input:pressed("next") then world.state.seed = world.state.seed % #SEEDS + 1 end
  local at = input.pointer
  if input:pressed("click") and at and at.hud == "seeds" and at.index then
    world.state.seed = at.index
  elseif input:pressed("click") and at and at.hud == "field" then
    world.state.planted = world.state.planted + 1
  end
end

function on_draw(frame)
  if player.state.typing then
    frame:text("Your name: " .. world.state.name .. "_", 8, 8)
    frame:text("Enter when done", 8, 20)
    return
  end
  frame:text(world.state.name .. " planted " .. world.state.planted, 8, 8)
  frame:menu{ name = "seeds", x = 8, y = 24, rows = SEEDS, at = world.state.seed, hint = "click or {next}" }
  -- the field: a rectangle the pointer can be over
  frame:box(frame.width - 88, 24, 80, 50, { 90, 70, 40 })
  frame:hud("field", frame.width - 88, 24, 80, 50)
  local p = frame.pointer
  if p and p.hud == "field" then frame:text("plant " .. SEEDS[world.state.seed], frame.width - 88, 78) end
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
        title: "A LEVEL SEEN FROM BEHIND",
        text: "[camera] with yaw = 180 in a level's file shows that level from its other side: the camera looks south, so the screen's right is the world's west. The level stays as it is; things the file and the game do not turn face the camera there, as they do everywhere. The keys stay the keys: input.dir.y is 1 with up held, whichever way the camera looks. A game whose up walks away from the camera turns them by level.camera.yaw, as here. A key held through a door into a level seen from behind means the way back there: take it once the player let go, if your game needs that. A quarter turn (90, 270) works for a game seen from above; a jump & run turns by halves (0 or 180).",
        conf: r##"c.world.size = nil             -- no world made in code: the level is levels/yard.toml
c.camera.pitch = 50"##,
        files: &[
            ("levels/yard.toml", r##"format = "voxcade level 1"
size = [96, 64, 32]

[camera]
yaw = 180

[[thing]]
name = "floor"
model = "yard/floor.vvx"
at = [48, 32, 0]
solid = true

[[place]]
name = "start"
at = [48, 32, 1]
"##),
            ("yard/floor.vvx", "format = \"voxcade voxel 1\"\nsize = [96, 64, 1]\n\n[palette]\na = { color = \"#5a6e4a\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *64\n\"\"\"\n"),
        ],
        code: r##"-- The way the camera looks in this level (levels/yard.toml says 180).
local function on_screen(dir)
  if level.camera.yaw == 180 then
    return { x = -dir.x, y = -dir.y }
  end
  return dir
end

function on_tick()
  for _, p in ipairs(level:players()) do
    -- up walks away from the camera, right to the screen's right
    p:walk(on_screen(p.input.dir), 40)
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
SHADOW ONLY in a piece's rows: it casts its shadow and is not drawn in the game (a ceiling the camera looks through). Here it shows as a ghost; GAME VIEW leaves it out.
PROPS in a thing's rows: a row for each prop its kind declares (c.things.NAME.props), its own value or else its kind's in the quiet color. Type a string, drag or type a number, tick a box; its kind's value takes it out of the file. A prop its kind names (c.things.NAME.names) offers the names there are: the levels, the places, the level's things (a list of things ticked one by one); a name that is not there shows in the warning color. RENAME of a thing or a place changes the props of this level's things that name it, in the same step. A thing chosen by the name the game gives it (lever_2) gets that name as its own, so it stays the same lever when another is removed. A table is only shown: change it in TEXT.
# GROUPS
GROUP in a thing's rows lists it under a group in the outliner: one there is, or a new name typed; empty takes it out. The game reads nothing of it.
A group's row folds with a click; its marks hide or lock all its things. RENAME and UNGROUP (in a grouped thing's rows, or the group's menu) change every thing of it at once.
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
The padlock beside it (on a group, a locked row and the chosen one; LOCK in the menu) locks it: a click in the view passes through it to what lies behind, its row still chooses it. Neither is written to the file.
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
Drag a rectangle on the screen: the voxels of the active parts in it (the part chosen, or every part shown). Double click: the color connected.
NEW, ADD and REMOVE say what the rectangle does (Ctrl: ADD, Alt: REMOVE); THROUGH takes the voxels behind too; CLEAR or Escape: none.
Its arrows move the voxels; SELECTION in the inspector and a right click on them: NEW PART, REMOVE, PAINT, DUPLICATE, FLIP, ROTATE, MOVE TO PART, COPY, PASTE.
# PARTS
A click on a part's row (or a double click in the view) chooses it; the figure whole is the first row.
MOVE | ROTATE at a part: its arrows move it with what hangs on it, its rings rotate it about its joint.
PARENT and JOINT in the inspector set what it hangs on and where it turns; or drag its row onto another part's row: it hangs there, staying where it stands.
A right click on a row: ADD PART, ADD EMPTY, DUPLICATE, CLONE, CLONE MIRRORED, MIRROR (for NAME_left and NAME_right), RENAME, MERGE, REMOVE.
CLONE makes a part like another: the same voxels, a stroke on either shows on both (four wheels, a mirrored half body). LIKE in the inspector links a part to another, MAKE UNIQUE gives a clone its voxels back, MIRROR X Y Z mirrors them.
+ EMPTY makes an empty part: no voxels, a node other parts hang on and turn with (a car's two halves side by side on an empty car). On the figure whole it becomes the root.
A model of one frame gets parts too: + PART at FRAMES, or NEW PART on a selection, makes it a figure in one step, its frame the root part (named as the file), everything where it stood. A model of several frames is an animation and has none.
# POSE
The rings turn parts on top of their rest, nothing saved. COPY gives the pose as Lua for frame:model.
# GAME VIEW
The figure as the game lights and shades it, outline too, from the game's angle and filling the view. The wheel and the right button zoom and turn it as always; GAME VIEW again goes back to the editor's look.
# SLICE AND CONTEXT
SLICE cuts the model across X, Y and Z, each plane on or off on its own, as a scan; what lies on a plane's far side from where you looked when you switched it on is hidden and untouched. Each plane's row: its layer (or drag its arrow), FLIP for the other side, OFF. Page Up and Page Down move the plane switched on last.
FLOOR beside GRID hides the plate under the model, to see and work on it from below.
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
    Topic {
        title: "FIND AND SEARCH",
        text: "The open text (a Lua file, a sound, a song, a model's TEXT) is changed in the main area.
# FIND
Ctrl+F (or FIND above the text) shows the find row over the text, with the selected word in it. Every match is marked, the current one brighter; the row says which of how many.
Enter or F3: the next match. Shift+Enter or Shift+F3: the one before. Both go round at the ends.
MATCH CASE: upper and lower case count as different letters (off at first).
Escape or CLOSE puts the row away; the text has the keys again.
# REPLACE
Ctrl+H (or the arrow left of FIND) shows REPLACE WITH under it. REPLACE (or Enter there) replaces the current match and goes on to the next; REPLACE ALL replaces every one. Ctrl+Z takes either back in one step.
# SEARCH THE GAME
Ctrl+Shift+F (or SEARCH at the top) finds the word in every text of the game: its Lua files, scenarios, sounds, songs and models; not what its .gitignore leaves out. The lines found stand in the list's place under their files; a click opens the file at that line with the word marked. Enter searches again, Escape or CLOSE shows the list again.
# FILTER THE LIST
Ctrl+P (or a click in FILTER at the top of the list) types a name: only the files whose names hold it stay in the list, each heading counting them. Enter opens the first one, Escape or the cross clears the text. It looks at names only; SEARCH looks inside the files.
# OTHER KEYS
Ctrl+S saves at once (a text saves itself a second after the last change). Ctrl+Z goes back a step.",
        conf: "",
        files: &[],
        code: "",
    },
];

/// The first topic's title: every name of the API with what it does.
const ALL: &str = "REFERENCE";

/// The reference's groups in the order a game meets them: whose names
/// (`Item::of`; "" with `callbacks` the functions a game writes, else the
/// ones every script is given), a heading, and what they are in plain
/// words.
const GROUPS: &[(&str, bool, &str, &str)] = &[
    ("", true, "YOUR FUNCTIONS", "Functions you write in your Lua files. The engine calls each one when its moment comes: on_tick 60 times a second, on_draw for every picture, on_touch when two things meet."),
    ("", false, "FUNCTIONS YOU CALL", "Given to every script."),
    ("world", false, "WORLD", "The whole run, from start to end: the time, what is kept for the run, going from level to level, winning and losing."),
    ("level", false, "LEVEL", "The level being played: its things, its blocks, sounds and effects in it."),
    ("level.camera", false, "LEVEL.CAMERA", "What the camera looks at."),
    ("area", false, "AREAS", "A box set in a level (ADD AREA in the level editor), to ask who is inside: level.areas.NAME."),
    ("player", false, "PLAYER", "The player's figure: a thing like any other, so every name under THING works on it too."),
    ("thing", false, "THING", "Anything in a level: the player, an enemy, a pickup, a piece. Where it is, what it remembers, how it moves."),
    ("input", false, "INPUT", "What the player presses, in on_tick."),
    ("controls", false, "CONTROLS", "Which keys or pad buttons the player uses, to show them in your game's words."),
    ("screen", false, "SCREEN", "Effects over the whole picture."),
    ("music", false, "MUSIC", "Your game's song and its beat."),
    ("frame", false, "FRAME", "Drawing a picture, in on_draw: models, cubes, lights, text and the HUD. Only drawn, never part of the game."),
];

/// A name's doc as the reference shows it: its first sentence, then the
/// rest (None when it is one sentence). A sentence ends at a full stop and
/// a space outside brackets (not `{ ... }` nor `(e.g. this)`).
fn first_sentence(doc: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    let b = doc.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            b'.' if depth <= 0 && matches!(b.get(i + 1), Some(b' ' | b'\n')) && i > 0 && b[i - 1] != b'.' => {
                return (&doc[..=i], Some(doc[i + 2..].trim()));
            }
            // A line of its own (a list's items) begins the rest.
            b'\n' => return (doc[..i].trim_end(), Some(doc[i + 1..].trim())),
            _ => {}
        }
    }
    (doc, None)
}

/// Every name of the API under its group's heading: the name as it is
/// written, its first sentence under it, the rest of what the register
/// says quieter below.
fn reference(ui: &mut egui::Ui) {
    use crate::lua::api::{API, What};
    ui.label(RichText::new(ALL).color(ACCENT));
    ui.add_space(8.0);
    ui.label("Every name you can use in your game's Lua, by what it belongs to; at the end every setting of conf.lua and every key of a kind of thing. Resting the mouse on a name in your code shows the same.");
    for (of, callbacks, heading, plain) in GROUPS {
        let items: Vec<_> = API.iter().filter(|i| i.of == *of && (!of.is_empty() || matches!(i.what, What::Callback { .. }) == *callbacks)).collect();
        if items.is_empty() {
            continue;
        }
        ui.add_space(20.0);
        ui.label(RichText::new(*heading).color(ACCENT));
        ui.label(*plain);
        ui.add_space(10.0);
        for item in items {
            ui.label(RichText::new(crate::lua::api::signature(item)).monospace().color(TEXT));
            let (first, rest) = first_sentence(item.doc);
            ui.label(RichText::new(first).color(TEXT));
            if let Some(rest) = rest {
                ui.label(RichText::new(rest).small().color(QUIET));
            }
            ui.add_space(10.0);
        }
    }
    settings_part(ui);
    kinds_part(ui);
}

/// One line of the reference's settings or kinds: how it is written, then
/// its words.
fn entry(ui: &mut egui::Ui, head: &str, doc: &str) {
    ui.label(RichText::new(head).monospace().color(TEXT));
    if !doc.is_empty() {
        ui.label(RichText::new(doc).color(TEXT));
    }
    ui.add_space(8.0);
}

/// The reference's SETTINGS: every setting of conf.lua by its section,
/// with its type, default, range and words (`lua::api::settings`).
fn settings_part(ui: &mut egui::Ui) {
    ui.add_space(20.0);
    ui.label(RichText::new(SETTINGS).color(ACCENT));
    ui.label("Set in conf.lua, inside function conf(c): c.camera.pitch = 50. What you leave out keeps the default shown after =; a number out of its range is a mistake.");
    let mut section = String::new();
    for s in crate::lua::api::settings() {
        let now = s.path.split(['.', '[']).next().unwrap_or("").to_string();
        if now != section && s.path.contains(['.', '[']) {
            ui.add_space(10.0);
            ui.label(RichText::new(format!("c.{now}")).small().color(QUIET));
        }
        section = now;
        entry(ui, &s.head(), &s.doc);
    }
}

/// The reference's KINDS: what every kind of thing may set, then what
/// each behavior of the ready-made rules reads (`lua::api::kind_keys`).
fn kinds_part(ui: &mut egui::Ui) {
    ui.add_space(20.0);
    ui.label(RichText::new(KINDS).color(ACCENT));
    ui.label("A kind of thing is c.things.NAME = { ... } in conf.lua. Every kind may set the first keys; a kind with a behavior of the ready-made rules (c.kit) reads its behavior's too, each unset its default.");
    let mut part = None;
    for k in crate::lua::api::kind_keys() {
        let now = (k.kit, k.behavior.clone());
        if part.as_ref() != Some(&now) {
            ui.add_space(10.0);
            let heading = if k.kit.is_empty() { "EVERY KIND".to_string() } else { format!("behavior = \"{}\" (c.kit = \"{}\")", k.behavior, k.kit) };
            ui.label(RichText::new(heading).small().color(QUIET));
            part = Some(now);
        }
        let default = if k.default.is_empty() { String::new() } else { format!(" = {}", k.default) };
        entry(ui, &format!("{}: {}{default}", k.key, k.ty), &k.doc);
    }
}

/// The reference's parts after the API's names.
const SETTINGS: &str = "SETTINGS (CONF.LUA)";
const KINDS: &str = "KINDS OF THINGS (C.THINGS)";

/// The topics' column: as wide as its longest title, with a row's insets,
/// the list's margin and its scroll bar, so no title is cut; at most half
/// the room in a narrow window.
fn topics_width(ui: &egui::Ui) -> f32 {
    let font = TextStyle::Body.resolve(ui.style());
    let widest = std::iter::once(ALL).chain(TOPICS.iter().map(|t| t.title)).map(|t| ui.painter().layout_no_wrap(t.to_string(), font.clone(), TEXT).size().x).fold(0.0, f32::max);
    let width = widest + 2.0 * crate::gui::INSET + 6.0 + ui.spacing().scroll.allocated_width() + ui.spacing().scroll.bar_width;
    width.min(ui.available_width() / 2.0).ceil()
}

/// HELP in `ui`: the topics on the left, the one `chosen` (its place in
/// that list: 0 every name of the API, then TOPICS from 1) beside them.
pub fn ui(ui: &mut egui::Ui, chosen: &mut usize) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(topics_width(ui));
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
                None => reference(ui),
            });
        });
    });
}

/// A block of Lua to read, select and copy (COPY: all of it).
fn code(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new().fill(PANEL).corner_radius(4.0).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            let font = TextStyle::Monospace.resolve(ui.style());
            // Long lines wrap: the example is read whole in a narrow window
            // (COPY takes the text as written).
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                let mut job = super::code::highlight(text.as_str(), &font, super::code::Syntax::Lua);
                job.wrap.max_width = wrap;
                ui.fonts_mut(|f| f.layout_job(job))
            };
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

    /// The topics' column is as wide as its longest title: none is cut at
    /// its edge (it was a fixed 250 pixels, cutting "SPEAK THE PLAYER'S
    /// LANGUAGE" even 1920 wide).
    #[test]
    fn every_topic_title_fits_its_column() {
        let ctx = egui::Context::default();
        crate::gui::style(&ctx);
        let mut widths = None;
        // Twice: the fonts are there from the second frame on.
        for _ in 0..2 {
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0))), ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| {
                let font = TextStyle::Body.resolve(ui.style());
                let titles: Vec<f32> = std::iter::once(ALL).chain(TOPICS.iter().map(|t| t.title)).map(|t| ui.painter().layout_no_wrap(t.to_string(), font.clone(), TEXT).size().x).collect();
                widths = Some((topics_width(ui), titles));
            });
        }
        let (column, titles) = widths.unwrap();
        for (title, width) in std::iter::once(ALL).chain(TOPICS.iter().map(|t| t.title)).zip(titles) {
            assert!(width + 2.0 * crate::gui::INSET <= column, "{title}: {width} in a column of {column}");
        }
    }

    /// Every name of the register stands under one of the reference's
    /// groups (a new owner in the register needs a group here).
    #[test]
    fn every_name_has_a_group_in_the_reference() {
        use crate::lua::api::{API, What};
        for item in API {
            let groups = GROUPS.iter().filter(|(of, callbacks, _, _)| item.of == *of && (!of.is_empty() || matches!(item.what, What::Callback { .. }) == *callbacks)).count();
            assert_eq!(groups, 1, "{}.{}", item.of, item.name);
        }
        assert_eq!(first_sentence("Plays it. Then more."), ("Plays it.", Some("Then more.")));
        assert_eq!(first_sentence("One: and so on"), ("One: and so on", None));
        assert_eq!(first_sentence("Sets c.things.NAME = { ... }. Then more."), ("Sets c.things.NAME = { ... }.", Some("Then more.")));
        assert_eq!(first_sentence("Its keys:\n- hp: hits.\n- pose: a name."), ("Its keys:", Some("- hp: hits.\n- pose: a name.")));
        assert_eq!(first_sentence("Its data.\n- hp: hits."), ("Its data.", Some("- hp: hits.")));
    }

    /// HELP's songs read, their channels are as long as each other (what
    /// the text promises) and the MML topic is there.
    #[test]
    fn the_songs_help_writes_read_and_line_up() {
        let mut seen = 0;
        for topic in TOPICS {
            for (name, text) in topic.files.iter().filter(|(n, _)| n.ends_with(".mml")) {
                let song = crate::audio::music::parse(text).unwrap_or_else(|e| panic!("{name}: {e:#}"));
                let lengths = song.channel_lengths();
                assert!(lengths.iter().all(|(_, l)| (l - lengths[0].1).abs() < 0.01), "{name}: {lengths:?}");
                seen += 1;
            }
        }
        assert!(seen >= 3, "HELP has its songs ({seen})");
        let mml = TOPICS.iter().find(|t| t.title == "A SONG IN MML").expect("the MML topic");
        // Every phoneme a voice sings, as the table has them.
        assert!(mml.text.contains(&crate::audio::music::phonemes()), "HELP's phonemes are not the table's: {}", crate::audio::music::phonemes());
        assert!(mml.files.iter().any(|(_, song)| song.contains("voice") && song.contains('"')), "the example sings");
    }

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
            let load = || crate::session::Session::<crate::Voxcade>::load(&dir).unwrap_or_else(|e| panic!("{}: {e:#}", topic.title));
            // A game with a title of its own plays a run, then its title.
            let mut games = vec![load()];
            if games[0].cfg.title_screen.level.is_some() {
                let mut titled = load();
                titled.begin_title().unwrap();
                games.push(titled);
            }
            for mut game in games {
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
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

---@meta voxcade
-- Voxcade's Lua API for editors (LuaLS), written by `voxcade api --lua`
-- from the engine's register of it (src/lua/api.rs): made anew, not edited.

---@class Point A place in voxels.
---@field x number
---@field y number
---@field z number

---@class Way A way on the ground: x east, y north (z is 0).
---@field x number
---@field y number

---@alias Color integer[] Red, green, blue (and alpha), 0 to 255.

---@class Place Where a player arrives in a level.
---@field x number
---@field y number
---@field z number
---@field facing? Way The way it faces there, if the level says.

---@class Item A kind of item (c.items).
---@field name string Its name, the key in an inventory.
---@field model? string Its model, to draw it (frame:model).
---@field stack integer How many one inventory holds at most.

---@class Daylight A picture's daylight (frame:daylight); what it leaves out is c.light's.
---@field sun? number[] The way the sunlight travels, { x, y, z } with z below 0 (towards the ground).
---@field sun_color? Color The sunlight's colour and strength: white at noon, warm at dusk, a pale blue moon.
---@field ambient? number Light every face gets, 0 to 1.
---@field shadow? number How much a shadow darkens, 0 to 1.
---@field sky? Color The background's colour.

---@class MenuRow A row of a menu (frame:menu).
---@field text string What it says.
---@field value? string|number At its right: a price, a count.
---@field icon? string An icon of c.icons before the text.
---@field dim? boolean Darker: it cannot be had now.

---@class Menu A menu in the engine's look (frame:menu).
---@field rows (string|MenuRow)[] Its rows, from the top.
---@field at? integer The chosen row, from 1.
---@field title? string Above the rows.
---@field tabs? string[] Above the rows, one chosen.
---@field tab? integer The chosen tab, from 1.
---@field hint? string Below: what the keys do, {act} BUY.
---@field shown? integer How many rows show at most.
---@field x? number Its top left on the HUD (else in the middle).
---@field y? number
---@field width? number How wide (else as wide as it needs).

---@class GridCell A slot (frame:grid).
---@field icon? string An icon of c.icons.
---@field count? integer A number at its corner.
---@field dim? boolean Darker.

---@class Grid Slots in rows (frame:grid).
---@field cells (GridCell|false)[] The slots, false for an empty one.
---@field columns? integer Across (up to 8 unless said).
---@field cell? number A slot's size in UI voxels (18).
---@field at? integer The chosen slot, from 1.
---@field title? string Above the slots.
---@field x? number Its top left on the HUD (else in the middle).
---@field y? number

---@class Turn How much further a model is turned, in degrees (frame:model).
---@field turn? number Round, clockwise seen from above (90: a quarter to the right).
---@field pitch? number Its nose up (below 0: down).
---@field roll? number Its right side down, as when banking right (below 0: its left).

---@class SaveInfo What lies in a save slot.
---@field level string The level it was saved in.
---@field seconds integer The play time when it was saved.

---@class SpawnOptions How a thing comes into the level.
---@field z? number The height of its feet (else it stands on the ground).
---@field facing? Way The way it faces.
---@field state? table Its first state (copied).
---@field name? string Its name in this level (level:thing(name) finds it).
---@field props? table What the game's code reads of this one thing, over its kind's props: only those its kind declares (c.things.NAME.props), each a number, string, boolean or table as there.

---@class World The whole run: its state, its time, how it ends.
---@field state table The run's state, from its start to its end: numbers, strings, booleans and tables of those (each table once). No things: they live in their level (keep them in level.state). Snapshots, replays, saves and the state hash see it.
---@field tick integer Ticks since the run began (60 a second; none while the game holds still).
---@field language string The language the player chose of the ones your game speaks (c.languages = { "de", "en", "fr" }; SETTINGS offers them, the system's at first): "de", two small letters; without c.languages "en". It can change while playing: pick your texts by it every picture (texts[world.language].hello). Replays carry it. Over the network each machine draws in its own: in on_draw it is the language of the machine drawing, in on_tick and the events the host's, so keep what a text says in a state and choose its words in on_draw.
---@field levels string[] The levels' names in order: the files in levels/ sorted by name, numbers as numbers (01_hall, 02_tower); one level when there are none.
---@field leaving string|nil Why the level is about to be left at the end of this tick ("go_to", "restart", "load", "won", "lost"), or nil: the ask that counts so far. One ask counts in a tick: going on (go_to, win) before starting over (restart, load, lose); among those of one rank the first asked. (Leaving by QUIT TO TITLE or a level file saved while playing is no ask: on_level_end hears "quit" and "reload".)
---@field saves table<integer, SaveInfo> What lies in each save slot (c.saves.slots of them): world.saves[1] is nil while slot 1 is empty, else { level, seconds }.
---@field players (Player|false)[] Every seat of the game (c.players, 1 unless set), in order: world.players[n] is the player on seat n, or false while nobody sits there (#world.players is how many seats; world.players[1] is player). A HUD shows PRESS START where a seat is false. Players in play in this level: level:players().
---@field items Item[] The kinds of item (c.items) in their order (c.items.NAME.order, then by name), each { name, model, stack }: for drawing an inventory.
local World = {}

---Leaves this level for another at the end of the tick (the code after it still runs): by name or number (world:go_to(level.next) goes on in order), arriving at the place called place, else where the level starts. player.state.checkpoint is forgotten. One ask counts in a tick: world.leaving.
---@param level string|integer
---@param place? string
function World:go_to(level, place) end

---Enters this level afresh at the end of the tick: everything from its file again, level.state fresh; the player at place, else at the place player.state.checkpoint names, else where it starts. One ask counts in a tick: world.leaving.
---@param place? string
function World:restart(place) end

---The text of a file in the game folder, by its path there (world:read("levels/microban.txt")): only while main.lua starts: at its top, in on_world_start or the first on_level_start. The files read count for the replays, and saving one runs main.lua anew.
---@param name string
---@return string
function World:read(name) end

---Saves the game into a slot (1 to c.saves.slots) at the end of the tick (or of the on_level_start that asks): world.state, player.state, every level's level.kept, the level being played with its level.state and the voxels changed in it (blown away, built), and where the player stands. The level's things come from its file when it is loaded (a thing kept in level.state comes back as nil: keep what you need to place them again). For save points, or saving on its own in on_level_start. The player can save from the pause menu too.
---@param slot integer
function World:save(slot) end

---Goes on from the save in a slot at the end of the tick, like a level change (on_level_end("load"), then on_level_start("load")): the states as they were saved, the level from its file with the voxels changed in it as they were, the player where it stood. An empty slot is a mistake: world.saves tells first. One ask counts in a tick: world.leaving.
---@param slot integer
function World:load(slot) end

---Holds the game still for this many ticks (a hit 2 or 3, 60 at most): everything stops, on_tick too. It is part of the game, so replays see it.
---@param ticks integer
function World:hitstop(ticks) end

---The run is won at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: world.leaving.
function World:win() end

---The run is lost at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: a win or a go_to in the same tick counts before it (world.leaving).
function World:lose() end

---@class Level The level being played: its voxels, its things, what happens in it, its camera.
---@field state table This visit's state: what the level knows while it is played (the things in it too, moves, time). Fresh each time the level is entered, a restart too; after world:load as it was saved (its things nil). The engine reads one key: level.state.held = true holds the level (the ready-made rules, the physics and touches wait; on_tick, on_draw and the time go on): for a message, a shop, a tally.
---@field name string Its name: its file's name without the ending (hall for levels/hall.toml); main for a game's one level made in code.
---@field number integer Its place among world.levels, from 1.
---@field next string|nil The next level's name, nil after the last one.
---@field visits integer How often it was entered in this run, restarts too: 1 the first time.
---@field kept table What this level keeps across visits for the whole run (a lever pulled, a puzzle solved): read it in on_level_start. Plain data, no things.
---@field arrived Place Where the player arrived in this level, with its name (a place's, or start): a game without a figure of the engine's stands its hero there in on_level_start.
---@field places table<string, Place> Its named places, set in its file (level.places.west): where a player arrives; start is always there (the place called start, else where the ready-made rules stand the player, else c.player.spawn).
---@field size Point The level's size in voxels.
---@field camera Camera Where the camera looks: level.camera:follow(thing), level.camera:look_at(x, y).
local Level = {}

---The voxel at a cell: its palette index, 0 empty.
---@param x integer
---@param y integer
---@param z integer
---@return integer
function Level:voxel(x, y, z) end

---Sets the voxel at a cell: a colour "#rrggbb" (taken into the level's palette), a palette index, or nil or 0 for empty.
---@param x integer
---@param y integer
---@param z integer
---@param voxel integer|string|nil
function Level:set_voxel(x, y, z, voxel) end

---Fills the box from one corner cell to the other, both included, like set_voxel; nil or 0 empties it. A level made in code (c.world.size) is built this way.
---@param x0 integer
---@param y0 integer
---@param z0 integer
---@param x1 integer
---@param y1 integer
---@param z1 integer
---@param voxel integer|string|nil
function Level:fill(x0, y0, z0, x1, y1, z1, voxel) end

---Whether the voxel at a cell stops bodies (soft voxels, grass and shrubs, do not).
---@param x integer
---@param y integer
---@param z integer
---@return boolean
function Level:solid(x, y, z) end

---The height of the ground at x, y: the cell above the column's top voxel.
---@param x integer
---@param y integer
---@return integer
function Level:ground(x, y) end

---Places a thing of a kind (c.things) at x, y, on the ground there unless options.z is given; options.facing turns it, options.state is its first state, options.name its name (one thing's in the level), options.props what your code reads of it over its kind's props. A kind without a behaviour falls to the ground; with fixed = true (c.things.NAME.fixed) it stays exactly where it is put. With ready-made rules (c.kit) their kinds stand where a marker there would place them. The camera follows the player; in a game without a figure for it (no c.player.kind), the first thing placed.
---@param kind string
---@param x number
---@param y number
---@param options? SpawnOptions
---@return Thing
function Level:spawn(kind, x, y, options) end

---The thing called name (thing.name: door_1), or with this id, if it is still there.
---@param name string|integer|Thing
---@return Thing|nil
function Level:thing(name) end

---Every thing (of a kind, or of a family: c.things.NAME.family), in the order they came: not the player (it belongs to no level), not the dead, not what was taken away this tick.
---@param kind? string
---@return Thing[]
function Level:things(kind) end

---The players playing in this level, in seat order: someone sits there, not out of play (state.out), and its figure stands (one taking a seat comes in at the tick's end). Loop over it for every player's rules: for _, p in ipairs(level:players()) do p:walk(p.input.dir, 40) end. world.players is every seat, the out too (a HUD, a continue).
---@return Player[]
function Level:players() end

---How many things (of a kind or family) there are: not the player, not the dead, not what was taken away this tick (count deaths in on_death).
---@param kind? string
---@return integer
function Level:count(kind) end

---The kind of the level's block at a voxel: "solid", "oneway", "ladder", "hazard", or nil (nothing, decoration, a level not made of blocks). What a block does is your code's: a hazard hurts because you say so. For a thing's whole body: thing:in_block.
---@param x number
---@param y number
---@param z number
---@return string|nil
function Level:block(x, y, z) end

---Plays a model's frames once (a puff, a sparkle), its feet where a thing stands or at a point, fps frames a second (12). Only shown: not part of the game.
---@param model string
---@param at Thing|Point
---@param fps? number
function Level:effect(model, at, fps) end

---Blows a hole: voxels fly as debris, pieces cut off fall.
---@param at Thing|Point
---@param radius number
function Level:explode(at, radius) end

---Takes every round in flight away, grenades and rockets too before they go off; only those fired by a team if you name one ("enemies", "player"). A bomb that clears the screen, or mercy for a player coming back.
---@param team? string
function Level:clear_rounds(team) end

---Plays a sound at a place, by its name: a file of the game, sounds/NAME.toml, or one the engine or the ready-made rules bring.
---@param name string
---@param at Thing|Point
---@param volume? number
function Level:sound(name, at, volume) end

---@class Camera Where the camera looks.
local Camera = {}

---The camera follows this thing from now on; following the player, ready-made rules lead the view as theirs do (the jump & run's look ahead). Every level starts following the player.
---@param thing Thing
function Camera:follow(thing) end

---The camera rests over this point from now on, following nothing, whatever spawns (follow follows a thing again): a room, a boss arena, a scene, a field that scrolls by itself.
---@param x number
---@param y number
function Camera:look_at(x, y) end

---@class Player : Thing The one who plays: a thing that belongs to no level and goes from level to level (everything a thing has); its state is the run's. Its figure is the ready-made rules' player, a thing of c.player.kind, or none (it then stands nowhere).
---@field state table The player's state for the whole run, from level to level, started from c.state.player. Keys the engine keeps (a script reads and sets them like the rest): score (a whole number), inventory (items by name and how many: player.state.inventory.key; with the ready-made shooter rules also the rounds of its guns and its grenades, items of their names), checkpoint (the name of a place or of a thing in this level where world:restart goes; forgotten on world:go_to); with a figure that has hit points, hp; with ready-made rules hp and invulnerable (ticks left without hits, or true until set back; hp set above 0 brings the shooter's player down back to life); with the ready-made shooter rules (c.kit = "shooter") also weapon (the gun in hand, by its name in c.player.weapons). Lives and the like are your game's own keys. Plain data, no things.
local Player = {}

---@class Thing A thing in the level, by its id.
---@field id integer|nil Its number, never given to another thing in the same run; nil for the player, who belongs to no level.
---@field kind string|nil The name of its kind (c.things); nil for a player of no kind (the one the ready-made shooter rules make from c.player).
---@field state table Its state while it lives: thing.state.hp, the hits it still takes, where its kind has hp (c.things.NAME.hp at the start; the engine keeps it, a script reads and sets it: at 0 on_death runs), inventory (a chest's, a trader's: items by name and how many), pose for a figure of the jump & run's ready-made rules (which set of its frames it shows, c.things.NAME.frames: a prop's, a door's or a sign's always, "open", "idle" the first; another's while you set one, over its rules' own animation, from the tick it was set: "attack", then nil for its own again; the hero's is player.state.pose), goal for a platform of the jump & run's ready-made rules ("end" or "start": it goes to that end of its travel and stays there, a platform out of a wall, a trapdoor, a lift; nil: to and fro again), freed for a hostage of the ready-made shooter rules (set true: the cage bursts, it runs off), flying for its gunship (up in the air fighting; only to read), and anything of the game's own. It goes when the thing goes.
---@field x number Where it stands: the middle of its feet.
---@field y number Where it stands: the middle of its feet.
---@field z number The height of its feet.
---@field on_ground boolean Whether it stands on something.
---@field wall Way|nil The way to a wall right beside it (within half a voxel), higher than a step (the way it faces first), or nil: a wall jump, a wall slide.
---@field facing Way The way it faces, one of the 8 ways.
---@field seat integer|nil A player's seat (1 for player, 2 and on for the others); nil for a level's thing, so if thing.seat then asks whether a thing is a player, in any handler (a ~= player leaves out seats 2 and on).
---@field input Input A player's keys this tick, as input is seat 1's: p.input.dir, p.input:pressed("fire"). Only in on_tick, as input.
---@field controls Controls What a player plays with, as controls is seat 1's: p.controls.device, p.controls:key("fire") (a key in braces in a text is the first of frame.players's).
---@field name string|nil Its name in this level: a thing placed by the level file is called its kind and its number among them (door_1, door_2 in the file's order), a spawned one what level:spawn named it. nil for the player and for things the ready-made rules bring later. player.state.checkpoint and level:thing take it.
---@field families table<string, boolean> The families of its kind (c.things.NAME.family = { "enemies" }), as a set: if b.families.enemies then ... end. Only to read.
---@field props table What your code reads of it, fixed while it lives: its kind's props (c.things.NAME.props = { worth = 100 }) and what level:spawn gave it. Only to read; what changes goes in thing.state.
local Thing = {}

---Walks this tick the way given at this speed (voxels a second), facing it; it falls, steps up ledges and lands by itself. Only things your script moves (a kind with no behaviour in c.things, or one whose behaviour thing:enable switched off).
---@param way Way
---@param speed number
function Thing:walk(way, speed) end

---Turns it to the nearest of the 8 ways: things your script moves, and the player with ready-made rules (it turns again as it moves; the jump & run's hero only left or right).
---@param way Way
function Thing:face(way) end

---Jumps this high (voxels) if it stands on something. Only things your script moves.
---@param height number
function Thing:jump(height) end

---The point this far ahead of its feet, a voxel up.
---@param distance number
---@return Point
function Thing:ahead(distance) end

---Whether the other thing stands closer than this.
---@param other Thing
---@param distance number
---@return boolean
function Thing:near(other, distance) end

---Puts it exactly there, standing still (on the ground unless z is given): the end of a slide on a grid (a kind with fixed = true stays wherever it is put). Given a place (level.places.west) it stands there, facing its way if it has one; given a thing, where that thing stands (a checkpoint). Things your script moves, and the player: the ready-made rules stand theirs there, still and on its feet, facing the way it did (a respawn; thing:face turns it).
---@param x number|Place|Thing
---@param y? number
---@param z? number
function Thing:place(x, y, z) end

---It attacks with a weapon of c.weapons (with the ready-made shooter rules, c.kit = "shooter"): a blow at the target (or the first in reach in front), a shot at it (or the way it faces; the player's finds a target with the aim assist), a throw to it (or its full throw ahead). It starts at the tick's end; then it waits the weapon's cooldown before the next. The player spends the weapon's rounds from player.state.inventory and attacks only while it has them. An action named in c.player.attacks attacks by itself; use another action for attacks your code decides (a charge shot, a combo).
---@param weapon string
---@param target? Thing|Point
function Thing:attack(weapon, target) end

---Hurts it (by a thing, with a weapon, if given): anything with hit points loses them (thing.state.hp, player.state.hp; at 0 on_death runs), and the ready-made rules' own take it as their hits (a soldier falls, a gunship spins down, the player is thrown back). A kind's team and immune do not stop it: your code decides whom it hurts. While it may not be hurt (player.state.invulnerable) nothing happens. on_hit runs in the next tick's events, as for the ready-made rules' hits; a thing it kills is out of the level's lists and counts at once.
---@param damage integer
---@param by? Thing
---@param weapon? string
function Thing:hurt(damage, by, weapon) end

---The things it touches now (of a kind or family), in the order they came: for what lasts while touched (spikes, an enemy's contact, lava), asked in on_tick, where it stays true every tick. Not the dead, nor what was taken away this tick.
---@param kind? string
---@return Thing[]
function Thing:touching(kind) end

---Whether any part of its body is in a block of the level of this kind: "hazard", "ladder", "oneway" or "solid" (level:block for one voxel).
---@param kind string
---@return boolean
function Thing:in_block(kind) end

---Sends it up this high (voxels), standing or in the air: off an enemy it landed on, a spring. Always the full height; the jump button plays no part. Things your script moves, and the jump & run's hero (c.kit = "platformer").
---@param height number
function Thing:bounce(height) end

---Sets it moving the way given at this speed (voxels a second), and up as high as rise (voxels) if given, standing or in the air: a dash (player:launch(player.facing, 240)), a wall jump, a knockback. Physics carries it on, and ready-made rules steer it on from there (the hero as the player presses, a soldier as he means to go); switch them off for a while with thing:enable to keep them out of it. Things your script moves, things whose behaviour is off, the player, the hero, walkers, hoppers, soldiers and hostages.
---@param way Way
---@param speed number
---@param rise? number
function Thing:launch(way, speed, rise) end

---Switches its ready-made behaviour (its kind's, c.things.NAME.behaviour: "hero", "walker", "soldier", ...) off or on again. Off, it does nothing by itself: it still falls, is hit and touched, and your script moves it like its own (thing:walk, thing:jump, thing:launch). A stunned hero, an end walk, a frozen enemy. Until it leaves its level (the hero: until the next level). Not yet the shooter's player, who is no thing of a kind.
---@param behaviour string
---@param on boolean
function Thing:enable(behaviour, on) end

---It takes an item of c.items (one, or count): into its state.inventory (the player's: player.state.inventory), no more than the item's stack; how many it took. on_pickup runs in the next tick's events.
---@param item string
---@param count? integer
---@return integer
function Thing:take(item, count) end

---Takes it out of the level at the end of this tick: it can still be read until then, but it is out of the level's lists, counts and touches at once. Whether it was there to remove. Not the player: it goes from level to level.
---@return boolean
function Thing:remove() end

---@class Input The keys held this tick, by the game's action names: only in on_tick.
---@field left boolean Held this tick (input:pressed("left") whether it went down this tick).
---@field right boolean Held this tick (input:pressed("right") whether it went down this tick).
---@field up boolean Held this tick (input:pressed("up") whether it went down this tick).
---@field down boolean Held this tick (input:pressed("down") whether it went down this tick).
---@field dir Way The directions held as a way: x and y each -1, 0 or 1.
local Input = {}

---Whether an action (c.actions, one the ready-made rules bring, or a direction: "left", "right", "up", "down") is held this tick. The ready-made rules read the same keys and use none of them up.
---@param action string
---@return boolean
function Input:held(action) end

---Whether an action or a direction went down this tick: one step a press on a grid, one line in a menu. A press of interact that on_interact took is used up.
---@param action string
---@return boolean
function Input:pressed(action) end

---Whether an action or a direction went up this tick.
---@param action string
---@return boolean
function Input:released(action) end

---True as the key goes down, and again and again while it is held (after a third of a second, ten times a second), as a menu's cursor moves on a held key: for menus, lists and stepping on a grid. Directions too: input:repeated("down").
---@param action string
---@return boolean
function Input:repeated(action) end

---@class Screen The picture's effects (a shake, a flash, an icon bursting), from anywhere the game changes.
---@field height number How many UI voxels the HUD is from top to bottom (c.ui.height), for a burst near its bottom; on_draw's frame has its width too.
local Screen = {}

---Shakes the screen: 0..1 more (a small hit 0.1, a blast 0.35, 1 at most); it dies down by itself. Only the camera moves, and players can turn shaking off.
---@param amount number
function Screen:shake(amount) end

---An icon of c.icons bursts on the HUD at x, y (its top left): its voxels fly off and fall, a heart lost, a grenade thrown. Only shown, not part of the game.
---@param icon string
---@param x number
---@param y number
function Screen:burst(icon, x, y) end

---Flashes the screen white, 0..1 (times c.effects.screen_flash); it fades by itself. Players can turn flashes off.
---@param strength number
function Screen:flash(strength) end

---@class Frame The picture on_draw makes (function on_draw(frame)): only there, only for this picture.
---@field players Player[] The players at the machine this picture is drawn for, in seat order (at one machine every taken seat, the out too; over the network that machine's own): their HUD. A key in braces in a text, and controls in on_draw, are the first of them's (seat 1's where none plays there); another player's is p.controls:key.
---@field width number How many UI voxels the HUD is across: it follows the screen's shape, so put things at the right edge from it (frame.width - 3 - frame:text_width(score)).
---@field height number How many UI voxels the HUD is from top to bottom (c.ui.height).
---@field alpha number How far into the tick this picture is, 0 to 1: add it to a count of ticks for motion that runs smooth between them.
local Frame = {}

---Draws a model (a kind's or one in c.models) where the thing stands in the level, facing its way; index is which of its frames (from 0); without it, its kind's frames for what it does (c.things.NAME.frames: idle standing, walk, jump in the air), else the model's own: frame 0 standing, 1 in the air, the rest walking, a frame every half its height walked. A model's pivot is its cell size / 2 across and deep (the middle cell of an odd size): of an even size it lies one voxel off the middle, so facing -y or sideways it fills cells one voxel higher on the axis a turn flips than facing +y (use odd sizes where that matters). turn turns it further, freely, in degrees about its middle: { roll = 20 } banks a ship to the right, { turn = 45 } faces a diagonal, { pitch = 10 } lifts its nose. Turned, its voxels leave the grid; a kind with turns = "free" in c.things faces exactly its way by itself (a diagonal too), without, its quarter turns. pose moves a figure made of parts (a model whose objects have names in the file: head, body, arm_left ...): each part by its name, a turn about its joint, what hangs on it moving along: { head = { turn = 30 }, arm_right = { pitch = 90 } } raises the right arm forward. Over its kind's swing (c.things.NAME.swing: parts swinging as it walks).
---@param file string
---@param thing Thing
---@param index? integer
---@param turn? Turn
---@param pose? table<string, Turn>
function Frame:model(file, thing, index, turn, pose) end

---Draws one voxel in the level, at x, y, z in the level's voxels (alpha below 255: see-through; glow from 0 to 1).
---@param x number
---@param y number
---@param z number
---@param color? Color
---@param glow? number
function Frame:cube(x, y, z, color, glow) end

---A light in the level this frame, at a thing or a point.
---@param at Thing|Point
---@param radius number
---@param color? Color
function Frame:light(at, radius, color) end

---This picture's daylight: the sun's way (sun = { x, y, z }, its light travelling down), its colour and strength (sun_color, white unless set), the light every face gets (ambient, 0 to 1), how dark shadows are (shadow, 0 to 1) and the sky's colour (sky); what you leave out is c.light's. Work it out from your game's clock every picture for a day and a night: the sun low and warm at dawn, white at noon, orange at dusk, a dim blue at night with frame:light lamps. Only the picture: the game's rules do not see it. A game whose code names daylight has every face a sun above the horizon can light in its level.
---@param light Daylight
function Frame:daylight(light) end

---A menu in the engine's look, on a plate: a title, tabs (tab: the chosen one, from 1), rows (a text, or { text =, value =, icon =, dim = }: a value at the right, an icon of c.icons before the text, dimmed when it cannot be had), the chosen row (at, from 1) yellow with the engine's red marker, and a hint below; an action in braces is its key. In the middle of the screen unless x and y say where (its top left), as wide as it needs unless width says; shown = how many rows show at most, a longer list scrolled to keep at in view. What is open and where the cursor stands is your game's: keep it in level.state, hold the level while it is open (level.state.held = true) and move it in on_tick (input:repeated).
---@param menu Menu
function Frame:menu(menu) end

---Slots in rows on a plate, an inventory's: cells, each { icon =, count =, dim = } (an icon of c.icons, a number at its corner, darker when dim) or false for an empty slot; columns across (up to 8 unless said), cell its size in UI voxels (18), at the chosen slot (from 1) edged in yellow, an optional title. In the middle unless x and y say where.
---@param grid Grid
function Frame:grid(grid) end

---Text on the HUD, over the level: x and y its top left, from the HUD's top left in UI voxels (frame.width across, frame.height down). size 1 the small letters (7 voxels high), 2 to 4 the big ones (c.ui.display_font). An action in braces is its key as the player sees it: "PRESS {jump}" writes PRESS X, or the pad's button (controls:key); {{ writes a brace. Written as you write it: lower case, the letters of German and French (ä ö ü ß é è ê à ç œ …) and signs such as · × → ▲ ▼ ✓ … „ « » too, in the small letters and the big ones drawn from them (a display font has capitals only); a key's name in capitals.
---@param text string
---@param x number
---@param y number
---@param color? Color
---@param size? integer
function Frame:text(text, x, y, color, size) end

---Where a point of the level shows in this picture, in the HUD's measure (x and y as frame:text takes them, from the top left), or nothing when it lies behind the camera. A thing's point is where its feet are drawn, gliding as it does; up lifts it that many voxels (over a figure's head). For names, health bars and numbers rising from where something happened.
---@param at Thing|Point
---@param up? number
---@return number?, number?
function Frame:to_screen(at, up) end

---How many UI voxels wide the text is at that size (for centring and putting it at the right edge).
---@param text string
---@param size? integer
---@return number
function Frame:text_width(text, size) end

---A filled box on the HUD, its top left at x, y.
---@param x number
---@param y number
---@param width number
---@param height number
---@param color? Color
function Frame:box(x, y, width, height, color) end

---A bar on the HUD filled the fraction (0 to 1) of its width: a boss's health, a timer. color the filled part (red unless given), back the rest.
---@param x number
---@param y number
---@param width number
---@param height number
---@param fraction number
---@param color? Color
---@param back? Color
function Frame:bar(x, y, width, height, fraction, color, back) end

---A small picture of c.icons on the HUD, its top left at x, y: c.icons.heart = { rows = { ".RR.RR.", ... }, colors = { R = { 214, 36, 44 } } }.
---@param name string
---@param x number
---@param y number
function Frame:icon(name, x, y) end

---Text whose letters drop in one after another and bounce, ticks after it began (keep the tick it began in a state): GAME OVER, a stage's name. Before 0 nothing shows.
---@param text string
---@param x number
---@param y number
---@param ticks number
---@param color? Color
---@param size? integer
function Frame:drop_in(text, x, y, ticks, color, size) end

---@class Controls What the player plays with: the device and the keys as the player sees them (their keyboard's, their pad's). Replays carry it, so the game may go by it anywhere. Seat 1's; in on_draw the first player's at the machine drawing (frame.players).
---@field device string|nil What the player plays with, the one used last: "keyboard" or "gamepad"; nil before the run's first tick and where nobody plays (tests, a replay recorded without it).
local Controls = {}

---The first key or button of an action (c.actions, one the ready-made rules bring, or pause) as the player sees it: the keyboard's key as it is printed on it (Y where the binding is Z on a German keyboard), the pad's button as it is printed on the pad (B, CIRCLE). Empty if nothing is bound or nobody plays.
---@param action string
---@return string
function Controls:key(action) end

---@class Music The song your game plays of its own choice, and its beats and bars: the game can go with the music (waves on the beat), and replays play it the same.
---@field song string|nil The song your game plays (music:play), by its file's name; nil while the player's own plays.
---@field beat integer|nil Beats since your game's song began, from 0 (its #tempo); nil while the player's own song plays.
---@field intensity number How tense the game is, from 0 (calm) to 1 (all of the song; 1 unless you set it): the layers of your game's song (#layer in the song: drums from 0.35, the melody from 0.8) come in above their tension and go below it, each over a bar. Set it in on_tick from your states: enemies on the screen, a boss, a hit.
---@field bar integer|nil Bars since your game's song began, from 0 (its #meter beats to a bar, 4 unless the song says); nil while the player's own song plays.
local Music = {}

---Plays a song of c.audio.music by its file's name ("storm" for music/storm.mml) as your game's own, from the next bar of the one playing (from: "bar", "beat" or "now"; at once when none of yours plays): it starts at its beat 0 and keeps time with the game, held while the game pauses. Asked for while it plays, it goes on. Without a song: the player's own again (as the settings chose), from the next bar. Your game's song wins over the player's choice.
---@param song? string
---@param from? string
function Music:play(song, from) end

-- The game model's objects, there in every script.
---@type World
world = {}
---@type Level
level = {}
---@type Player
player = {}
---@type Input
input = {}
---@type Screen
screen = {}
---@type Controls
controls = {}
---@type Music
music = {}

-- The engine's functions every script has.

---A Lua file of your game folder, run once as the game starts: require("market") runs market.lua, require("lang.de") lang/de.lua (dots between folders), and gives back what the file returns (true if nothing); asked again, the same value. Only while the game starts: at the top of main.lua and of the files it requires. The files share main.lua's globals (a handler may live in any of them) and are your game's code as main.lua is: saving one runs the game's code anew, replays count it.
---@param name string
---@return any
function require(name) end

-- The functions a game defines; the engine calls them.

---conf.lua: sets the game's settings over the engine's defaults (c.title, c.physics.gravity, c.things.NAME = { ... }). Saved while the game runs, a setting takes effect at once; the ready-made rules (c.kit), the actions and keys, the level, the markers, the player's start and kind after a restart (F5).
---@param c Conf
function conf(c) end

---main.lua: runs once when a run begins, before its first level is there: give the run's states their first values (world.state, player.state). Globals set until the first level has begun stay as they are from then on: what changes goes in a state.
function on_world_start() end

---Runs when a level is entered, before its first tick: build a level made in code, place things, give level.state its first values. how is "start" when the run begins, "go_to" after world:go_to (the same level too), "restart" after world:restart, "load" after world:load (level.state as it was saved), "reload" when its file was saved while playing. The player already stands at level.arrived: do not spawn it here when c.player.kind gives it a figure. An ask here (world:go_to …) counts at once, before the level's first tick.
---@param how string
function on_level_start(how) end

---Runs when the level is about to be left: why is "go_to", "restart", "load", "won", "lost", "quit" (QUIT TO TITLE) or "reload". Copy from level.state what should outlive the visit (into world.state, player.state or level.kept). Nothing more can be asked here.
---@param why string
function on_level_end(why) end

---Runs when the run ends, after on_level_end: result is "won", "lost" or "quit" (QUIT TO TITLE). The end cannot be undone here; a last chance to change the states for the record.
---@param result string
function on_world_end(result) end

---Someone takes a seat during the run (a pad's START, the keyboard's second half, a machine over the network), in its tick, first among the events; not the seats taken as the run begins. how is "new" (p.state a copy of c.state.player) or "back" (the seat's state from before it left, or from a save). Its figure comes in at the tick's end: here it may be placed (p:place), given a kind (p.state.kind) or kept waiting (p.state.out = true); else it comes in where the first player in play stands.
---@param p Player
---@param how string
function on_player_join(p, how) end

---A player left its seat (LEAVE in the pause menu, its machine gone): in its tick, first among the events. It is out of play at once, its figure goes at the tick's end; p.state is kept for its seat, and whoever takes it again comes back with it (on_player_join's "back").
---@param p Player
function on_player_leave(p) end

---Runs whenever a save is loaded (world:load, LOAD in the pause menu), before its level is entered, with the version of the game that made it (its c.saves.version then). Count c.saves.version up when what a save holds changes (a key renamed), and bring older saves up to date here: if version < 2 then player.state.coins = player.state.gold end. There is no level here yet.
---@param version integer
function on_save_loaded(version) end

---Runs 60 times a second: the game's own rules (after the ready-made ones, if the game uses some, and after the events of this tick: on_touch, on_hit …). input holds the keys. math.random draws from the game's own random numbers, the same every time a run is played the same way (replays and saves keep them).
function on_tick() end

---Runs for every picture (as often as the screen shows one, apart from the 60 ticks a second): what the game draws besides the level and the things that draw themselves, on frame, this picture alone (frame:text, frame:cube, frame:model, frame:light). Everything else is only to look at here.
---@param frame Frame
function on_draw(frame) end

---Two things begin to touch: once, until they part (on_touch_end), not every tick. The player is always a, else the thing that came first. where is the face of b that a met: "top" (a came down onto b: landed on it, however fast), "bottom" (a came up under b, or b came down onto a), "side" (anything else, and always for a touch within a reach). What happens is your code's: a stomp, a pickup, a door. For what lasts while touched (spikes, an enemy's contact) ask thing:touching in on_tick. Things already touching where a level begins (or where thing:place puts one) count as touching: a player arriving on a door does not go through it again. A kind with a reach (c.things.NAME.reach: a pickup, a helipad, a hostage) is touched within it, whatever touches it (two with a reach: within the larger).
---@param a Thing
---@param b Thing
---@param where string
function on_touch(a, b, where) end

---Two things that touched part: the end of a touch on_touch began, the player a again (not when one is taken away or dies, nor when the level is left).
---@param a Thing
---@param b Thing
function on_touch_end(a, b) end

---The player pressed the action called interact (c.actions) while touching this thing (the nearest, if several): talk, open, read. That press is used up: input:pressed("interact") is false in this tick's on_tick.
---@param thing Thing
---@param by Thing
function on_interact(thing, by) end

---A thing was hurt (its hit points already lowered): by the ready-made rules, or by thing:hurt (told in the next tick). by is who did it and weapon with what (a weapon of c.weapons, knife, what thing:hurt named), if known.
---@param thing Thing
---@param by Thing|nil
---@param damage integer
---@param weapon string|nil
function on_hit(thing, by, damage, weapon) end

---A thing's hit points ran out, whoever lowered them (the ready-made rules, or your script setting thing.state.hp to 0); once, until it lives again (hit points above 0). by and weapon as on_hit. It is no longer listed, counted or touched; its kind and where it was can still be read in this tick. What it means is your code's: points for it, a life less for the player (the ready-made rules only show the fall). Count here what happened (a gunship down): counting what is left cannot tell one that died from one that has not come yet.
---@param thing Thing
---@param by Thing|nil
---@param weapon string|nil
function on_death(thing, by, weapon) end

---A beat of the song your game plays begins (music:play): beat counts from 0 at the song's start, on and on through its loops (music.bar for the bar). Runs before on_tick, in the tick the beat begins: waves, shots, a light that pulse with the music. Its clock is the game's, so replays play it the same; while the player's own song plays (none of yours), no beats.
---@param beat integer
function on_beat(beat) end

---A thing fell out of the level (c.physics.lost_below voxels under it, 48 unless set, or down on the world's floor below): once, until it is back in. Things other than the player are taken away after it; the player waits for what your code decides (a life less, and back at the checkpoint: player:place, player.state.hp). Without it the player falls on.
---@param thing Thing
function on_fall_out(thing) end

---A thing took an item of c.items with thing:take (told in the next tick). Its inventory already counts it.
---@param thing Thing
---@param item string
function on_pickup(thing, item) end

---@class Conf The game's settings (conf.lua); ready-made rules (c.kit) add their own (c.weapons).
---@field [string] table
---@field actions any[]
---@field audio Conf.audio
---@field camera Conf.camera
---@field debris Conf.debris
---@field effects Conf.effects
---@field high_scores any[]
---@field keys table<string, any>
---@field light Conf.light
---@field markers table<string, any>
---@field models any[]
---@field pad table<string, any>
---@field physics Conf.physics
---@field player Conf.player
---@field scored boolean
---@field things table<string, any>
---@field title string
---@field title_screen Conf.title_screen
---@field transition Conf.transition
---@field ui Conf.ui
---@field world Conf.world
---@field items table<string, { model?: string, stack?: integer, order?: integer }> What can be carried, by name: an inventory counts these.
---@field saves { slots?: integer, version?: integer } Save games: how many slots, and the version of what a save holds.
---@field voxcade string The Voxcade the game's files are for ("0.6.0"): a game for an older one is brought up to date by voxcade upgrade, one for a newer one does not open.
---@field languages string[] The languages the game speaks, two small letters each ({ "de", "en", "fr" }): SETTINGS offers them, world.language says which the player chose.

---@class Conf.audio
---@field [string] any
---@field drive number How hard the game's sound is pushed into the limiter, in decibels (0 to 12): 0 as it is, more makes it denser and louder; music you listen to for long tires sooner when dense.
---@field music_volume number
---@field volume number

---@class Conf.camera
---@field [string] any
---@field follow number
---@field fov number
---@field pitch number
---@field snap boolean
---@field view_width number
---@field yaw number

---@class Conf.debris
---@field [string] any
---@field gore number
---@field max_particles integer
---@field settle number

---@class Conf.effects
---@field [string] any
---@field glow number
---@field haze number
---@field haze_color number[]
---@field hitstop integer
---@field lights number
---@field outline number
---@field outline_color number[]
---@field screen_flash number
---@field tilt_shift number

---@class Conf.light
---@field [string] any
---@field ambient number
---@field edge_tolerance number
---@field metal_sweep number
---@field model_shading string
---@field model_shading_for table<string, any>
---@field reflection number[]
---@field reflection_horizon number[]
---@field shadow number
---@field shadow_style string
---@field sky number[]
---@field sun number[]
---@field sun_color number[]

---@class Conf.physics
---@field [string] any
---@field body_width number
---@field gravity number
---@field max_fall number
---@field step integer

---@class Conf.player
---@field [string] any
---@field accel number
---@field air_accel number
---@field jump number
---@field model string
---@field spawn number[]
---@field speed number

---@class Conf.title_screen
---@field [string] any
---@field attract_after number
---@field flyby_altitude number
---@field flyby_every number
---@field light_menu boolean
---@field logo any[]
---@field logo_bob number
---@field logo_colors any[]
---@field logo_cracks any[]
---@field logo_heights any[]
---@field props any[]

---@class Conf.transition
---@field [string] any
---@field color string
---@field fade number

---@class Conf.ui
---@field [string] any
---@field depth integer
---@field grain number
---@field height number
---@field round number
---@field slant number

---@class Conf.world
---@field [string] any
---@field bedrock boolean
---@field file string

---@class Conf.player
---@field kind string The kind of thing (c.things) the player's figure is, without ready-made rules.

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
---@field sun_color? Color The sunlight's color and strength: white at noon, warm at dusk, a pale blue moon.
---@field ambient? number Light every face gets, 0 to 1.
---@field shadow? number How much a shadow darkens, 0 to 1.
---@field sky? Color The background's color.

---@class PictureCamera What a picture's camera sets (frame:camera); what it leaves out is c.camera's.
---@field look_at? Point|Thing|number[] The point the view centers on, { x, y, z } in voxels (z left out: the camera's height), or a thing; left out, the view follows what the camera follows.
---@field width? number How many voxels the picture shows across where it looks.
---@field height? number Or how many it shows from bottom to top there (not both).
---@field pitch? number Degrees below the horizon, -89 (looking up) to 90 (straight down).
---@field fov? number The field of view up and down, 10 to 100 degrees.

---@class MenuRow A row of a menu (frame:menu).
---@field text string What it says.
---@field value? string|number At its right: a price, a count.
---@field icon? string An icon of c.icons before the text.
---@field dim? boolean Darker: it cannot be had now.

---@class MusicPlay How a song begins (music:play).
---@field from? string When it begins: at the next bar of the one playing (bar, unless said), beat or now.
---@field crossfade? number Seconds the old song fades out as this one fades in, 0 to 60 (0: the old ends, this begins); unless said, c.audio.crossfade.

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

---@class Rotation How much further a model is rotated, in degrees (frame:model).
---@field rotation? number Round, clockwise seen from above (90: a quarter to the right).
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
---@field state table Your game's data for the whole run, from its start to its end. It holds numbers, strings, booleans and tables of those (each table once). No things: they live in their level (keep them in level.state). Snapshots, replays, saves and the state hash see it.
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

---Saves the game into a slot (1 to c.saves.slots) at the end of the tick, or of the on_level_start that asks: for save points, or saving on its own. The player can save from the pause menu too.
---- Kept: world.state, player.state, every level's level.kept, the level being played with its level.state and the voxels changed in it (blown away, built), and where the player stands.
---- Not kept: the level's things. They come from its file when it is loaded; a thing kept in level.state comes back as nil, so keep what you need to place them again.
---@param slot integer
function World:save(slot) end

---Goes on from the save in a slot at the end of the tick, like a level change (on_level_end("load"), then on_level_start("load")): the states as they were saved, the level from its file with the voxels changed in it as they were, the player where it stood. An empty slot is a mistake: world.saves tells first. One ask counts in a tick: world.leaving.
---@param slot integer
function World:load(slot) end

---Holds the game still for this many ticks: a hit 2 or 3. Everything stops, on_tick too. 60 (a second) at most: more is cut to 60. It is part of the game, so replays see it.
---@param ticks integer
function World:hitstop(ticks) end

---The run is won at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: world.leaving.
function World:win() end

---The run is lost at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: a win or a go_to in the same tick counts before it (world.leaving).
function World:lose() end

---@class Level The level being played: its voxels, its things, what happens in it, its camera.
---@field state table Your game's data for this level while it is played: the things in it too, moves, time. Fresh each time the level is entered, a restart too; after world:load as it was saved (its things nil). The engine reads one key: level.state.held = true holds the level (the ready-made rules, the physics and touches wait; on_tick, on_draw and the time go on): for a message, a shop, a tally.
---@field name string Its name: its file's name without the ending (hall for levels/hall.toml); main for a game's one level made in code.
---@field number integer Its place among world.levels, from 1.
---@field next string|nil The next level's name, nil after the last one.
---@field visits integer How often it was entered in this run, restarts too: 1 the first time.
---@field kept table What this level keeps across visits for the whole run (a lever pulled, a puzzle solved): read it in on_level_start. Plain data, no things.
---@field arrived Place Where the player arrived in this level, with its name (a place's, or start): a game without a figure of the engine's stands its hero there in on_level_start.
---@field places table<string, Place> Its named places, set in its file (level.places.west): where a player arrives; start is always there (the place called start, else where the ready-made rules stand the player, else c.player.spawn).
---@field areas table<string, Area> Its named areas, boxes set in its file ([[area]] with name, at and size): level.areas.by_the_stove:holds(player) says whether the player stands in it. Only to read; a level without areas has an empty table.
---@field size Point The level's size in voxels.
---@field camera Camera Where the camera looks: level.camera:follow(thing), level.camera:look_at(x, y).
local Level = {}

---The voxel at a cell: its palette index, 0 empty (a solid piece's cells too: its model draws them, level:solid says they block).
---@param x integer
---@param y integer
---@param z integer
---@return integer
function Level:voxel(x, y, z) end

---Sets the voxel at a cell: a color "#rrggbb" (taken into the level's palette), a palette index, or nil or 0 for empty. A cell outside the level (level.size) is a mistake.
---@param x integer
---@param y integer
---@param z integer
---@param voxel integer|string|nil
function Level:set_voxel(x, y, z, voxel) end

---Fills the box from one corner cell to the other, both included, like set_voxel (only what of it lies inside the level); nil or 0 empties it. A level made in code (c.world.size) is built this way.
---@param x0 integer
---@param y0 integer
---@param z0 integer
---@param x1 integer
---@param y1 integer
---@param z1 integer
---@param voxel integer|string|nil
function Level:fill(x0, y0, z0, x1, y1, z1, voxel) end

---Whether the cell stops bodies: a voxel there (soft voxels, grass and shrubs, do not), or a solid piece's (solid = true in the level's file).
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

---Puts a new thing of a kind (c.things) into the level at x, y, on the ground there unless options.z is given. x and y lie inside the level (level.size); only a kind with fixed = true may wait outside, one coming in from off the screen.
---- options.z: the height of its feet. options.facing: the way it faces.
---- options.state: its first state. options.name: its name, one thing's in the level.
---- options.props: what your code reads of it over its kind's props.
---A kind without a behavior falls to the ground; with fixed = true (c.things.NAME.fixed) it stays exactly where it is put. A kit's kinds stand where a marker there would place them. The camera follows the player; in a game without a figure for it (no c.player.kind), the first thing placed. Pieces ("piece") come only from a level's file.
---@param kind string
---@param x number
---@param y number
---@param options? SpawnOptions
---@return Thing
function Level:spawn(kind, x, y, options) end

---The thing called name (thing.name: the name its level's file gives it, "lamp", else door_1), or with this id, if it is still there.
---@param name string|integer|Thing
---@return Thing|nil
function Level:thing(name) end

---Every thing (of a kind, or of a family: c.things.NAME.family), in the order they came: not the player (it belongs to no level), not the dead, not what was taken away this tick. level:things("piece") is every model and light the level's file set down.
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
---The player's data for the whole run, from level to level. It starts as c.state.player says. Plain data, no things; lives and the like are your game's own keys. A script reads and sets the engine's keys like its own.
---- score: a whole number.
---- inventory: items by name and how many (player.state.inventory.key).
---- checkpoint: the name of a place or a thing in this level where world:restart goes. Forgotten on world:go_to.
---- hp: with a figure that has hit points, or with a kit.
---With a kit (c.kit):
---- invulnerable: ticks left without hits, or true until set back.
---With the jump & run kit (c.kit = "platformer"):
---- pose: the hero's set of frames, as a thing's thing.state.pose.
---With the shooter kit (c.kit = "shooter"):
---- weapon: the gun in hand, by its name in c.player.weapons.
---- inventory also holds the rounds of its guns and its grenades, by their names.
---- hp set above 0 brings a player who is down back to life.
---@field state table
local Player = {}

---@class Thing A thing in the level, by its id.
---@field id integer|nil Its number, never given to another thing in the same run; nil for the player, who belongs to no level.
---@field kind string|nil The name of its kind (c.things); "piece" for a model or a light the level's file set down (it stays exactly where it is put, as a kind with fixed = true); nil for a player of no kind (the one the ready-made shooter rules make from c.player).
---This thing's data while it lives: your game's own keys, and some the engine keeps. It goes when the thing goes. A script reads and sets the engine's keys like its own.
---- hp: the hits it still takes, where its kind has hp (c.things.NAME.hp at the start). At 0 on_death runs.
---- inventory: items by name and how many (a chest's, a trader's).
---With the jump & run kit (c.kit = "platformer"):
---- pose: which set of its frames it shows (c.things.NAME.frames), from the tick it is set. A prop, a door or a sign always shows one ("open"; "idle" the first). Another kind shows it over its own animation ("attack"), nil for its own again. The hero's is player.state.pose.
---- goal: for a platform, "end" or "start": it goes to that end of its travel and stays (a trapdoor, a lift). nil: to and fro again.
---With the shooter kit (c.kit = "shooter"):
---- freed: for a hostage. Set true: the cage bursts, it runs off.
---- flying: for a gunship, up in the air fighting. Only to read.
---For a piece of a level's file:
---- light: a lamp's brightness, all its lights together, 0 (off) to 1 (the start). Their colors, places and reach are the file's.
---- parts: a figure of parts posed, as frame:model's pose: { leaf = { rotation = 90 } } swings a door's leaf open; nil at rest. Drawn so every picture, for this visit of the level: keep what must stay open in world.state and set it again in on_level_start. A solid piece still blocks where its model stands at rest.
---@field state table
---@field x number Where it stands: the middle of its feet. A piece's is the middle of the cell its file's at names (at + 0.5): a cube for it goes at math.floor(thing.x) + dx.
---@field y number Where it stands: the middle of its feet. A piece's is the middle of the cell its file's at names (at + 0.5): a cube for it goes at math.floor(thing.x) + dx.
---@field z number The height of its feet.
---@field on_ground boolean Whether it stands on something.
---@field wall Way|nil The way to a wall right beside it (within half a voxel), higher than a step (the way it faces first), or nil: a wall jump, a wall slide.
---@field facing Way The way it faces, one of the 8 ways.
---@field seat integer|nil A player's seat (1 for player, 2 and on for the others); nil for a level's thing, so if thing.seat then asks whether a thing is a player, in any handler (a ~= player leaves out seats 2 and on).
---@field input Input A player's keys this tick, as input is seat 1's: p.input.dir, p.input:pressed("fire"). Only in on_tick, as input.
---@field controls Controls What a player plays with, as controls is seat 1's: p.controls.device, p.controls:key("fire") (a key in braces in a text is the first of frame.players's).
---@field name string|nil Its name in this level: the name the level's file gives it (name = "lamp"), else a thing placed by the level file is called its kind and its number among them (door_1, door_2 in the file's order), a spawned one what level:spawn named it. nil for the player and for things the ready-made rules bring later. player.state.checkpoint and level:thing take it.
---@field families table<string, boolean> The families of its kind (c.things.NAME.family = { "enemies" }), as a set: if b.families.enemies then ... end. Only to read.
---@field props table Fixed values your code reads of this thing: its kind's props (c.things.NAME.props = { worth = 100 }) and what level:spawn gave it. Only to read; what changes goes in thing.state.
local Thing = {}

---Walks this tick the way given at this speed (voxels a second), facing it. Only the way's direction counts, not its length: { x = 0.3, y = 7 } walks at the full speed. It falls, steps up ledges and lands by itself. Only things your script moves (a kind with no behavior in c.things, or one whose behavior thing:enable switched off).
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

---Puts it exactly there, standing still: the end of a slide on a grid, a respawn. For things your script moves, and the player.
---- x, y, z: on the ground there unless z is given (a kind with fixed = true stays wherever it is put).
---- a place (level.places.west): it stands there, facing its way if it has one.
---- a thing: where that thing stands (a checkpoint).
---A kit stands its player there still and on its feet, facing the way it did (thing:face turns it).
---@param x number|Place|Thing
---@param y? number
---@param z? number
function Thing:place(x, y, z) end

---It attacks with a weapon of c.weapons, with the shooter kit (c.kit = "shooter"). It starts at the tick's end, then waits the weapon's cooldown before the next.
---- A blow: at the target, or the first in reach in front.
---- A shot: at the target, or the way it faces (the player's finds a target with the aim assist).
---- A throw: to the target, or its full throw ahead.
---The player spends the weapon's rounds from player.state.inventory and attacks only while it has them. An action named in c.player.attacks attacks by itself; use another action for attacks your code decides (a charge shot, a combo).
---@param weapon string
---@param target? Thing|Point
function Thing:attack(weapon, target) end

---Hurts it, by a thing and with a weapon if given. Anything with hit points loses them (thing.state.hp, player.state.hp); at 0 on_death runs. A kit's own take it as their hits: a soldier falls, a gunship spins down, the player is thrown back.
---A kind's team and immune do not stop it: your code decides whom it hurts. While it may not be hurt (player.state.invulnerable) nothing happens. on_hit runs in the next tick's events; a thing it kills is out of the level's lists and counts at once.
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

---Sets it moving the way given at this speed (voxels a second), standing or in the air: a dash (player:launch(player.facing, 240)), a wall jump, a knockback. Only the way's direction counts, not its length.
---- rise: how high it goes up too, in voxels.
---Physics carries it on, and a kit's rules steer it on from there (the hero as the player presses, a soldier as he means to go): thing:enable switches them off for a while. For things your script moves, things whose behavior is off, the player, the hero, walkers, hoppers, soldiers and hostages.
---@param way Way
---@param speed number
---@param rise? number
function Thing:launch(way, speed, rise) end

---Switches its ready-made behavior (its kind's, c.things.NAME.behavior: "hero", "walker", "soldier", ...) off or on again. Off, it does nothing by itself: it still falls, is hit and touched, and your script moves it like its own (thing:walk, thing:jump, thing:launch). A stunned hero, an end walk, a frozen enemy. Until it leaves its level (the hero: until the next level). Not yet the shooter's player, who is no thing of a kind.
---@param behavior string
---@param on boolean
function Thing:enable(behavior, on) end

---It takes an item of c.items (one, or count): into its state.inventory (the player's: player.state.inventory), no more than the item's stack; how many it took. on_pickup runs in the next tick's events.
---@param item string
---@param count? integer
---@return integer
function Thing:take(item, count) end

---Takes it out of the level at the end of this tick: it can still be read until then, but it is out of the level's lists, counts and touches at once. Whether it was there to remove. Not the player: it goes from level to level.
---@return boolean
function Thing:remove() end

---@class Area A named box of the level, set in its file (level.areas.NAME): only to read.
---@field name string Its name in the level's file (level.areas.NAME).
---@field x integer Its low corner in voxels: x, y and z where it begins.
---@field y integer Its low corner's y.
---@field z integer Its low corner's z.
---@field size Point How big it is in voxels, across, deep and up.
local Area = {}

---Whether a thing stands inside it (where its feet are, the middle of the cell it stands on) or a point lies inside it ({ x, y, z }; without z at any height). Ask it in on_tick: if level.areas.by_the_stove:holds(player) then ... end.
---@param at Thing|Point
---@return boolean
function Area:holds(at) end

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
---@field height number How many UI voxels the HUD is from top to bottom (c.ui.height), for a burst near its bottom. Its width follows the shape of each player's window, so only on_draw's frame.width says it.
local Screen = {}

---Shakes the screen: 0..1 more (a small hit 0.1, a blast 0.35, 1 at most); it dies down by itself. Only the camera moves, and players can turn shaking off.
---@param amount number
function Screen:shake(amount) end

---Bursts an icon of c.icons on the screen at x, y (its top left): its voxels fly off and fall, a heart lost, a grenade thrown. Only shown, not part of the game.
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

---Draws a model where a thing stands, facing its way. file is a kind's model or one in c.models. The thing is a thing, never a point.
---- index: which of its frames, from 0. Without it, its kind's frames for what it does (c.things.NAME.frames: idle, walk, jump), else the model's own: 0 standing, 1 in the air, the rest walking, a frame every half its height walked.
---- rotation: turns it further, in degrees about its middle (the voxel at half its height over its pivot). { roll = 20 } banks a ship right, { rotation = 45 } faces a diagonal, { pitch = 10 } lifts its nose. Roll is taken first, then pitch, then rotation, as for a level's piece. Off the quarter turns its voxels leave the grid.
---- pose: moves a figure of parts (a model whose parts have names: head, body, arm_left ...), each part by its name, a rotation about its joint, what hangs on it moving along. { head = { rotation = 30 }, arm_right = { pitch = 90 } } raises the right arm forward. offset = { x, y, z } moves a part by voxels (fractions too) after its rotation: { bust = { pitch = 10, offset = { 0, 1, 0 } } }. It goes over its kind's swing (c.things.NAME.swing).
---A kind with rotates = "free" faces exactly its way (a diagonal too), else the nearest quarter turn. A model's pivot is the middle cell of its size across and deep: of an even size one voxel off the middle, so facing -y or sideways it fills cells a voxel further on the flipped axis (use odd sizes where that matters).
---@param file string
---@param thing Thing
---@param index? integer
---@param rotation? Rotation
---@param pose? table<string, Rotation>
function Frame:model(file, thing, index, rotation, pose) end

---Draws one voxel in the level, at x, y, z in the level's voxels (alpha below 255: see-through; glow from 0 to 1).
---@param x number
---@param y number
---@param z number
---@param color? Color
---@param glow? number
function Frame:cube(x, y, z, color, glow) end

---Lights the level round a thing or a point, in this picture.
---@param at Thing|Point
---@param radius number
---@param color? Color
function Frame:light(at, radius, color) end

---Sets the sun and the sky for this picture: a day and a night. What you leave out is c.light's.
---- sun: the way its light travels, { x, y, z } with z below 0.
---- sun_color: its color and strength, white unless set.
---- ambient: the light every face gets, 0 to 1. shadow: how dark shadows are, 0 to 1.
---- sky: the sky's color.
---Work it out from your game's clock every picture: the sun low and warm at dawn, white at noon, orange at dusk, a dim blue at night with frame:light lamps. Only the picture: the game's rules do not see it. A game whose code names daylight has every face a sun above the horizon can light in its level.
---@param light Daylight
function Frame:daylight(light) end

---Moves the camera for this picture, for a scene: a zoom, a cut, a slow pan. What you leave out is the game's camera's (c.camera): { width = 80 } zooms in on what the camera follows and keeps following it.
---- look_at: the point the view centers on, { x, y, z } in voxels (z left out: the camera's height), or a thing.
---- width: how many voxels the picture shows across there; or height, from bottom to top.
---- pitch: degrees below the horizon. fov: the field of view.
---Only this picture: nothing is kept and nothing smoothed. Call it in every picture the scene lasts and work a pan out from your clock (world.tick + frame.alpha, music.beat). The game's rules, the sounds and level.camera do not see it; frame:to_screen after it goes by it. The camera does not turn round yet. Another pitch or fov than c.camera's needs faces the game's own view never shows: the level is built anew with them once, a moment's hitch the first time.
---@param camera PictureCamera
function Frame:camera(camera) end

---Draws a menu in the engine's look, on a plate. In the middle of the screen unless x and y say where (its top left), as wide as it needs unless width says.
---- rows: each a text, or { text =, value =, icon =, dim = }: a value at the right, an icon of c.icons before the text, dimmed when it cannot be had.
---- at: the chosen row, from 1, yellow with the engine's red marker.
---- title, tabs (tab: the chosen one, from 1), hint below; an action in braces is its key.
---- shown: how many rows show at most; a longer list scrolls to keep at in view.
---What is open and where the cursor stands is your game's: keep it in level.state, hold the level while it is open (level.state.held = true) and move it in on_tick (input:repeated).
---@param menu Menu
function Frame:menu(menu) end

---Draws an inventory's slots in rows, on a plate: cells, each { icon =, count =, dim = } (an icon of c.icons, a number at its corner, darker when dim) or false for an empty slot; columns across (up to 8 unless said), cell its size in UI voxels (18), at the chosen slot (from 1) edged in yellow, an optional title. In the middle unless x and y say where.
---@param grid Grid
function Frame:grid(grid) end

---Writes text on the screen, over the level. x and y are its top left, in UI voxels from the screen's top left (frame.width across, frame.height down).
---- size: 1 the small letters (7 voxels high), 2 to 4 the big ones (c.ui.display_font).
---- An action in braces is its key as the player sees it: "PRESS {jump}" writes PRESS X, or the pad's button (controls:key). {{ writes a brace.
---Written as you write it: lower case, the letters of German and French (ä ö ü ß é è ê à ç œ …) and signs such as · × → ▲ ▼ ✓ … „ « ». A display font has capitals only; a key's name is in capitals.
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

---Draws a filled box on the screen, its top left at x, y.
---@param x number
---@param y number
---@param width number
---@param height number
---@param color? Color
function Frame:box(x, y, width, height, color) end

---Draws a bar on the screen filled the fraction (0 to 1) of its width: a boss's health, a timer. color the filled part (red unless given), back the rest.
---@param x number
---@param y number
---@param width number
---@param height number
---@param fraction number
---@param color? Color
---@param back? Color
function Frame:bar(x, y, width, height, fraction, color, back) end

---Draws a small picture of c.icons on the screen, its top left at x, y: c.icons.heart = { rows = { ".RR.RR.", ... }, colors = { R = { 214, 36, 44 } } }.
---@param name string
---@param x number
---@param y number
function Frame:icon(name, x, y) end

---Writes text whose letters drop in one after another and bounce, ticks after it began (keep the tick it began in a state): GAME OVER, a stage's name. Before 0 nothing shows.
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

---@class Music The song your game plays of its own choice, its beats and bars, and how loud and how muffled it sounds: the game can go with the music (waves on the beat), and replays play it the same.
---@field song string|nil The song your game plays (music:play), by its file's name; nil while the player's own plays.
---@field beat integer|nil Beats since your game's song began, from 0 (its #tempo); nil while the player's own song plays.
---@field intensity number How tense the game is, from 0 (calm) to 1 (all of the song; 1 unless you set it): the layers of your game's song (#layer in the song: drums from 0.35, the melody from 0.8) come in above their tension and go below it, each over a bar. Set it in on_tick from your states: enemies on the screen, a boss, a hit.
---@field bar integer|nil Bars since your game's song began, from 0 (its #meter beats to a bar, 4 unless the song says); nil while the player's own song plays.
local Music = {}

---Plays a song of c.audio.music by its file's name ("storm" for music/storm.mml) as your game's own. Your game's song wins over the player's choice.
---- from: when it begins: "bar" (the next bar of the one playing, unless said), "beat" or "now"; at once when none of yours plays.
---- crossfade: { from = "now", crossfade = 3 } fades the old song out as the new comes in over 3 seconds, as loud together as either alone. c.audio.crossfade sets it for every change; else the old ends as the new begins.
---It starts at its beat 0 and keeps time with the game (music.beat and on_beat count its beats from then), held while the game pauses. Asked for while it plays, it goes on, through a level change too. Without a song: the player's own again, from the next bar.
---@param song? string
---@param from? string|MusicPlay
function Music:play(song, from) end

---How loud your game's music sounds, as a part of the song's own loudness: from 0 (silent) to 1 (as loud as the song is, the start), reached smoothly over seconds (0.3 unless you say; 0 at once, still without a click). The player's music volume stays on top. It holds until you change it, from level to level too, whichever song plays: music:volume(0.25, 2) fades it to a quarter in two seconds. Without a level it gives the level you asked for last. Only heard: replays and the game itself do not change with it.
---@param level? number
---@param seconds? number
---@return number|nil
function Music:volume(level, seconds) end

---How dull your game's music sounds, as heard through a wall or a door: from 0 (clear, the start) to 1 (very dull, only the low notes), reached smoothly over seconds (0.3 unless you say; 0 at once, still without a click). Your game's sounds stay clear. It holds until you change it, from level to level too, whichever song plays: music:muffle(0.6, 1) outside a house, music:muffle(0, 1) inside. Without an amount it gives the amount you asked for last. Only heard: replays and the game itself do not change with it.
---@param amount? number
---@param seconds? number
---@return number|nil
function Music:muffle(amount, seconds) end

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

---Runs another Lua file of your game folder once and gives back what it returns. require("market") runs market.lua, require("lang.de") lang/de.lua (dots between folders), and gives back what the file returns (true if nothing); asked again, the same value. Only while the game starts: at the top of main.lua and of the files it requires. The files share main.lua's globals (a handler may live in any of them) and are your game's code as main.lua is: saving one runs the game's code anew, replays count it.
---@param name string
---@return any
function require(name) end

-- The functions a game defines; the engine calls them.

---conf.lua: your game's settings; what you leave out keeps the engine's default. Set them on c: c.title, c.physics.gravity, c.things.NAME = { ... }. Saved while the game runs, a setting takes effect at once; the ready-made rules (c.kit), the actions and keys, the level, the markers, the player's start and kind after a restart (F5).
---@param c Conf
function conf(c) end

---Runs once when a run begins, before the first level is there. Give world.state and player.state their first values here. Globals set until the first level has begun stay as they are from then on: what changes goes in a state.
function on_world_start() end

---Runs each time a level begins, before its first tick. Place things and set level.state here, and build a level made in code.
---- how: why it began. "start" when the run begins, "go_to" after world:go_to (the same level too), "restart" after world:restart, "load" after world:load (level.state as it was saved), "reload" when its file was saved while playing.
---The player already stands at level.arrived: do not spawn it here when c.player.kind gives it a figure. An ask here (world:go_to …) counts at once, before the level's first tick.
---@param how string
function on_level_start(how) end

---Runs just before the level is left. why says why: "go_to", "restart", "load", "won", "lost", "quit" (QUIT TO TITLE) or "reload". Copy from level.state what should outlive the visit (into world.state, player.state or level.kept). Nothing more can be asked here.
---@param why string
function on_level_end(why) end

---Runs when the run ends, after on_level_end. result is "won", "lost" or "quit" (QUIT TO TITLE). The end cannot be undone here; a last chance to change the states for the record.
---@param result string
function on_world_end(result) end

---Runs when another player joins while the game runs: a pad's START, the keyboard's second half, a machine over the network. It runs in the tick they join, first among the events; not for the seats taken as the run begins.
---- how: "new" (p.state a copy of c.state.player) or "back" (the seat's state from before it left, or from a save).
---Its figure comes in at the tick's end, where the first player in play stands. Here it may be placed instead (p:place), given a kind (p.state.kind) or kept waiting (p.state.out = true).
---@param p Player
---@param how string
function on_player_join(p, how) end

---Runs when a player leaves the game: LEAVE in the pause menu, or its machine gone. It runs in that tick, first among the events. It is out of play at once, its figure goes at the tick's end; p.state is kept for its seat, and whoever takes it again comes back with it (on_player_join's "back").
---@param p Player
function on_player_leave(p) end

---Runs whenever a save is loaded, to bring an older save up to date. It runs on world:load and LOAD in the pause menu, before the save's level is entered, with the version of the game that made it (its c.saves.version then). Count c.saves.version up when what a save holds changes (a key renamed), and bring older saves up to date here: if version < 2 then player.state.coins = player.state.gold end. There is no level here yet.
---@param version integer
function on_save_loaded(version) end

---Runs 60 times a second: your game's rules go here. input tells what the player presses. It runs after the ready-made rules, if the game uses some, and after this tick's events (on_touch, on_hit …). math.random draws from the game's own random numbers, the same every time a run is played the same way (replays and saves keep them).
function on_tick() end

---Runs for every picture: draw text, models and lights on frame. Only drawing: change nothing here. It runs as often as the screen shows a picture, apart from the 60 ticks a second; frame is this picture alone (frame:text, frame:cube, frame:model, frame:light), drawn besides the level and the things that draw themselves. Everything else is only to look at here.
---@param frame Frame
function on_draw(frame) end

---Runs when two things start touching: a stomp, a pickup, a door. It runs once, until they part (on_touch_end), not every tick. What happens is your code's.
---- a: the player, always, else the thing that came first.
---- where: the face of b that a met. "top": a came down onto b (landed on it, however fast). "bottom": a came up under b, or b came down onto a. "side": anything else, and always for a touch within a reach.
---For what lasts while touched (spikes, an enemy's contact) ask thing:touching in on_tick. Things already touching where a level begins, or where thing:place puts one, count as touching: a player arriving on a door does not go through it again. A kind with a reach (c.things.NAME.reach: a pickup, a helipad) is touched within it, by anything (two with a reach: within the larger).
---@param a Thing
---@param b Thing
---@param where string
function on_touch(a, b, where) end

---Runs when two things that touched part: the end of a touch on_touch began, the player a again (not when one is taken away or dies, nor when the level is left).
---@param a Thing
---@param b Thing
function on_touch_end(a, b) end

---Runs when the player presses the action called interact (c.actions) while touching this thing (the nearest, if several): talk, open, read. That press is used up: input:pressed("interact") is false in this tick's on_tick.
---@param thing Thing
---@param by Thing
function on_interact(thing, by) end

---Runs when a thing is hurt; its hit points are already lowered. It is hurt by the ready-made rules, or by thing:hurt (told in the next tick). by is who did it and weapon with what (a weapon of c.weapons, knife, what thing:hurt named), if known.
---@param thing Thing
---@param by Thing|nil
---@param damage integer
---@param weapon string|nil
function on_hit(thing, by, damage, weapon) end

---Runs when a thing's hit points run out, whoever lowered them (a kit's rules, or your script setting thing.state.hp to 0). Once, until it lives again (hit points above 0). by and weapon as on_hit.
---It is no longer listed, counted or touched; its kind and where it was can still be read in this tick. What it means is your code's: points for it, a life less for the player (a kit only shows the fall). Count here what happened (a gunship down): counting what is left cannot tell one that died from one that has not come yet.
---@param thing Thing
---@param by Thing|nil
---@param weapon string|nil
function on_death(thing, by, weapon) end

---Runs on every beat of the song your game plays (music:play). beat counts from 0 at the song's start, on and on through its loops (music.bar for the bar). Runs before on_tick, in the tick the beat begins: waves, shots, a light that pulse with the music. Its clock is the game's, so replays play it the same; while the player's own song plays (none of yours), no beats.
---@param beat integer
function on_beat(beat) end

---Runs when a thing falls out of the level (c.physics.lost_below voxels under it, 48 unless set, or down on the world's floor below): once, until it is back in. Things other than the player are taken away after it; the player waits for what your code decides (a life less, and back at the checkpoint: player:place, player.state.hp). Without it the player falls on.
---@param thing Thing
function on_fall_out(thing) end

---Runs when a thing took an item of c.items with thing:take (told in the next tick). Its inventory already counts it.
---@param thing Thing
---@param item string
function on_pickup(thing, item) end

---@class Conf The game's settings (conf.lua); ready-made rules (c.kit) add their own (c.weapons).
---@field [string] table
---@field voxcade string The Voxcade the game's files are for (c.voxcade = "0.6.0"): a game for an older one is brought up to date by voxcade upgrade, one for a newer one does not open. In neither the fingerprint nor the hash.
---@field title string The game's name: the window's title, the title screen's logo unless title_screen.logo says otherwise.
---@field world Conf.world
---@field physics Conf.physics
---@field player Conf.player
---@field title_screen Conf.title_screen
---@field camera Conf.camera
---@field light Conf.light
---@field debris Conf.debris
---@field effects Conf.effects
---@field transition Conf.transition
---@field audio Conf.audio
---@field ui Conf.ui
---@field things table<string, Kind|Kind.platformer.hero|Kind.platformer.walker|Kind.platformer.hopper|Kind.platformer.flyer|Kind.platformer.platform|Kind.platformer.prop|Kind.platformer.crate|Kind.shooter.soldier|Kind.shooter.spawner|Kind.shooter.captive|Kind.shooter.gunship|Kind.shooter.pickup> The kinds of things the game has (c.things.NAME = {... }): each may name a behavior of its ready-made rules, may build on another (like = "other") and says what differs; a level places them by name.
---@field kit string The building kit a Lua game plays with ("shooter"), if any.
---@field actions string[] A Lua game's actions besides the four directions (a kit brings its own), and the keys and gamepad buttons bound to them at first.
---@field keys table The keys bound to each action at first, by their US names (c.keys.jump = { "Z" }); the player may change them.
---@field pad table The gamepad's buttons bound to each action at first (c.pad.jump = { "A" }).
---@field models string[] Model files a Lua game draws besides its kinds' (frame:model).
---@field scored boolean The game keeps a score (player.state.score), with ready-made rules or without: the title shows the best, and runs go into the high score table.
---@field items table<string, table> What can be carried (c.items.key = { model = "key.vvx", stack = 1 }): an inventory (player.state.inventory) counts these by name.
---@field state Conf.state What a run starts with (c.state.world, c.state.player): the states' first keys, the engine's own among them (hp, weapon, inventory) and the game's (lives).
---@field icons table<string, table> Small pictures for the HUD (frame:icon, screen:burst): c.icons.heart = { rows = { ".RR.RR.",... }, colors = { R = { 214, 36, 44 } } }.
---@field saves Conf.saves Save games (c.saves = { slots = 3, version = 1 }): none without slots.
---@field high_scores any[][] The high score table to beat before anyone played: up to ten lines of initials and points ({ { "VOX", 50000 },... }); without them the engine's.
---@field languages string[] The languages the game speaks ({ "de", "en", "fr" }, two letters each): SETTINGS offers them, world.language says which the player chose, the engine's menus speak it. None: English.
---@field players integer The seats the game has, how many play at once (c.players = 4; docs/game-model.md, "Several players"): 1, a game of one, unless set. (from 1 to 16)
---@field team_score boolean The high score table takes the run as one line, every seat's score added, instead of a line for each player.

---@class Conf.world.blocks.backdrops
---@field image string The picture, relative to the game folder.
---@field behind integer How far behind the level's last row, in voxels. (from 0 to 65536)
---@field scale integer Voxels a pixel. (from 1 to 64)
---@field z integer The height of its bottom edge, in voxels. (from -4096 to 65536)

---@class Conf.world.blocks
---@field file string The level, relative to the game folder; its blocks in blocks/.
---@field margin integer Space left and right of the grid, in voxels, for the backdrops. (from 0 to 4096)
---@field below integer Open space under the grid, in voxels: pits deep enough to fall out of. (from 0 to 4096)
---@field backdrops Conf.world.blocks.backdrops[] Pictures far behind the grid (sky, hills): parallax through the perspective.

---@class Conf.world
---@field file string The level's file, relative to the game folder: a world or a map.
---@field bedrock boolean The lowest voxel layer cannot be destroyed.
---@field size integer[] A world made in code: empty, this big (x, y, z); the game's script builds it (level:fill), file is not read. (each from 1 to 65536)
---@field blocks Conf.world.blocks A level of voxel blocks in a grid (grid): file is not read.

---@class Conf.physics
---@field gravity number Downward acceleration, voxels per second². (from 0 to 100000)
---@field max_fall number Fastest fall, voxels per second. (from 0 to 100000)
---@field step integer Ledges up to this many voxels are walked up without jumping. (from 0 to 256)
---@field body_width number Width of the collision box of everything that walks. Effects (sparks, debris, gibs, screen shake) are sized relative to it. (above 0, up to 256)
---@field lost_below number How far under the level a falling body is lost (on_fall_out), in voxels. (At its default it is left out of replays' fingerprint: those from before it play on.) (from 0 to 65536)

---@class Conf.player
---@field [string] any
---@field model string Player model (.vvx), relative to the game folder. Forward is +y.
---@field spawn number[] Start position on the ground (x, y). Negative means world centre.
---@field speed number Running speed. (from 0 to 100000)
---@field accel number How quickly it gets to running speed on the ground, and in the air. (from 0 to 1000000)
---@field air_accel number How quickly it gets to running speed in the air. (from 0 to 1000000)
---@field jump number Height of a full jump (holding the button); letting go early cuts it. (from 0 to 10000)
---@field kind string|string[] A game without ready-made rules: the kind of thing (c.things) the player's figure is (player), placed where each level starts and going along from level to level; with several seats one for every seat or a list by seat ({ "hero", "hero_blue" }). Without it the player has none.

---@class Conf.title_screen.props
---@field model string Its model (.vvx).
---@field at number[] Where it lies on the ground, x and y in voxels.
---@field rotation integer Degrees clockwise seen from above, 0, 90, 180 or 270.

---@class Conf.title_screen
---@field music string What plays on the title (a song of c.audio.music; none: the first).
---@field at number[] The ground point the camera looks at (default: where the player starts).
---@field view_width number Voxels across the screen there (default: c.camera.view_width). (above 0, up to 65536)
---@field logo string[] The logo, a line each (default: the game's title), and their colours as "#rrggbb".
---@field logo_colors string[] Each logo line's colour, "#rrggbb".
---@field logo_heights integer[] How high each logo line's capitals are, in UI voxels. (each from 1 to 1000)
---@field drip integer Which logo line drips (blood). (from 0 to 99)
---@field logo_cracks number[] Cracks per letter in each logo line, on average. (each from 0 to 100)
---@field logo_model string A model of the game's (.vvx) as the logo, in place of its lines of letters: a UI voxel each of its voxels, its front (+y) to the player and its z up, centred under the top of the screen. A figure of parts (its objects named, a letter each) drops in part by part in the file's order, else whole. Lit as the big letters are (the sides a little darker, what lies further back too): its colours are its materials'.
---@field logo_bob number How far the logo's letters (or its model's parts) bob up and down once they have landed, in UI voxels: a slow, smooth wave running through them, coming in over a second; 0 (unless set) they stand still. (from 0 to 100)
---@field props Conf.title_screen.props[] Models lying in the scene.
---@field flyby string A model that flies over the scene now and then (a helicopter), every flyby_every seconds.
---@field flyby_every number Seconds between two flybys (0: none). (from 0 to 3600)
---@field flyby_altitude number How high the flyby crosses over the ground. (from 0 to 65536)
---@field attract_after number Seconds on the title without a key pressed before it plays the game's replays (replays/), as the arcade machines did; 0: never. (from 0 to 3600)
---@field light_menu boolean The scene behind the menu is dark: its rows and hint in light letters (default: dark ones, over bright ground like More Blood's sand).

---@class Conf.camera.follow_height
---@field below number How far the feet may go under the camera's height, in voxels. (from 0 to 4096)
---@field above number How far over it. (from 0 to 4096)

---@class Conf.camera
---@field pitch number Degrees below the horizon. 90 is straight top-down. (from -89 to 90)
---@field yaw number Degrees around the vertical axis. 0 looks north (+y). (from -360 to 360)
---@field fov number Vertical field of view in degrees. Higher means stronger perspective. (from 10 to 100)
---@field view_width number Voxels visible across the screen at the player's distance, independent of resolution. (above 0, up to 65536)
---@field view_height number Voxels visible from the bottom of the screen to the top there instead: the width then follows the window's shape and view_width counts no more (a vertical shooter whose field must show its whole height in any window). Unset, the width is fixed. (above 0, up to 65536)
---@field view_at_least number[] At least this many voxels across and high there, { 320, 200 } as a screen of old had pixels: the window's shape decides which way it shows more (a wide window more across, a narrow one more high). Takes the place of view_width and view_height for the picture (the shooter's reach, part of its rules, keeps view_width). (each above 0, up to 65536)
---@field follow number Fraction of the distance to the player the camera closes per tick (only without snap; snapping keeps the camera on the player). (above 0, up to 1)
---@field snap boolean Everything moving travels in whole voxels and the camera scrolls with the player in whole voxels (the 3D-pixel look). Off: moving things and the camera glide between voxels.
---@field follow_height Conf.camera.follow_height A side view's camera follows the player up and down too, in whole voxels: its height moves only when the feet leave the band from below voxels under it to above over it, so jumps keep the view still. Unset, the height never changes (a view from above).
---@field stop_at_edges boolean The view stops at the edges of the stage instead of showing the dark beyond: a side view's level, or the ground a view from above shows (BunnyFarm's meadow).
---@field hold number[] How far apart the players in play may be, across and deep, in voxels ({ 240, 150 }; { 240 } across only, a side view): a move that would take the two furthest apart further is cut short, as the edge of an arcade machine's screen held them (docs/game-model.md, "Several players"). The simulation's, never the window's. (each above 0, up to 65536)
---@field rotates string How the game sets things down: on the grid in quarter turns (the default), or rotated freely (rotates = "free": a level's pieces at any bearing, rotation = 30). The game's choice for every level, and what the level editor offers; a level turned off the quarter turns in a game on the grid does not load.
---@field aspect number The picture's shape, its width over its height (2.39 a film's): the scene is drawn into the largest rectangle of that shape the window holds, in its middle, black bars on the other two sides; the HUD and the menus lie inside it too. Unset: the window's shape. (from 0.25 to 4)

---@class Conf.light
---@field sun number[] Direction the sunlight travels (towards the ground). (each from -1000 to 1000; [3] from -1000 to below 0)
---@field sun_color number[] The sunlight's colour and strength, 0..1 per channel: white unless set (warm at dusk, a pale blue for the moon). A game's frame:daylight sets it for one picture. (each from 0 to 1)
---@field ambient number Light that reaches every face, 0..1. (from 0 to 1)
---@field shadow number How much a shadow darkens sunlit faces, 0..1. (from 0 to 1)
---@field sky number[] Background colour, 0..1 per channel. (each from 0 to 1)
---@field reflection_horizon number[] The sky metal mirrors: its colour at the horizon and overhead, 0..1 per channel (below the horizon it mirrors the ground, in the haze's colour). (each from 0 to 1)
---@field reflection number[] The sky metal mirrors overhead, 0..1 per channel. (each from 0 to 1)
---@field metal_sweep number A streak of light sweeping over metal now and then, this bright (0: none): metal reads as metal even where the sun cannot glint in it. (from 0 to 10)
---@field shadow_style string How shadow edges look: "smooth" (soft edges across the voxels), "voxel" (every voxel face is either lit or in shadow, edges step along the grid) or "voxel_soft" (as voxel, with half-shadowed faces in between).
---@field model_shading string How models (actors, props) are lit: "faces" (every face of a voxel by its own direction), "smooth" (a whole voxel by the direction of the surface around it, so round shapes shade softly like pixel art) or "edges" (smooth, but where two flat surfaces meet the faces keep their own light, so hard edges stay hard).
---@field model_shading_for table<string, string> Models shaded otherwise, by file (relative to the game folder), e.g. { ["huey.vvx"] = "faces" } for a machine among smooth figures.
---@field edge_tolerance number With "edges": how far, in voxels, a voxel's surroundings may stray from one smooth surface before it counts as an edge. (from 0 to 16)

---@class Conf.debris
---@field settle number Chance that a landed debris voxel becomes part of the world again. (from 0 to 1)
---@field max_particles integer Upper bound for flying debris and sparks. (from 0 to 1000000)
---@field gore number Share of a body's voxels that burst blood-red when it dies, 0..1 (0 keeps their own colours, say for robots). (from 0 to 1)

---@class Conf.effects
---@field glow number How strongly bright things (muzzle flashes, fire, tracers, sparks) glow into their surroundings; 0 turns it off. (from 0 to 10)
---@field lights number How strongly flashes and explosions light up what is around them. (from 0 to 10)
---@field haze number Haze towards the far edge of the view, 0..1, and its colour (0..1). (from 0 to 1)
---@field haze_color number[] The haze's colour, 0..1 per channel. (each from 0 to 1)
---@field tilt_shift number Blur at the top and bottom of the screen, a miniature look, 0..1. (from 0 to 1)
---@field screen_flash number How bright the white flash over the screen on big explosions gets, 0..1. (from 0 to 1)
---@field hitstop integer Ticks the game holds still when an enemy dies. (from 0 to 60)
---@field outline number A dark line round the models (figures, props), as round a sprite: how many voxels thick (0: none; 1 a voxel, as a sprite's pixel), and its colour (0..1). Hidden behind what stands in front, drawn where one part of a model stands before another too. (from 0 to 8)
---@field outline_color number[] The outline's colour, 0..1 per channel. (each from 0 to 1)

---@class Conf.transition
---@field fade number Seconds out and seconds in; 0: a cut. (from 0 to 10)
---@field color string The colour faded through, "#rrggbb".
---@field go_to number A length of its own for a reason the level is left (a puzzle game's quick restart: restart = 0). (from 0 to 10)
---@field restart number Seconds of the fade when the level is entered afresh. (from 0 to 10)
---@field won number Seconds of the fade when the run is won. (from 0 to 10)
---@field lost number Seconds of the fade when the run is lost. (from 0 to 10)

---@class Conf.audio
---@field volume number Loudness of all sounds, 0..1. (from 0 to 1)
---@field music string|string[] Music as MML text, relative to the game folder: one file, or a list the keys 1..9 switch between while playing.
---@field music_volume number Loudness of the music, 0..1, times the player's music volume. (from 0 to 1)
---@field drive number How hard the game's sound is pushed into the limiter, in decibels: 0 as it is, more makes it denser and louder; music you listen to for long tires sooner when dense. 0 to 12. The limiter's drive (the JUST mastering stage's own input gain), on sounds and music together, after the player's volumes; the game's choice, not the player's. (from 0 to 12)
---@field crossfade number Seconds one song crossfades into the next when the game's code changes the song (music:play; one call may say otherwise): the old fading out as the new fades in, equal in power. 0: the old ends and the new begins, a level's fade between them silent. (from 0 to 60)

---@class Conf.ui
---@field height number UI voxels from the bottom of the screen to the top, whatever the resolution (180: a 5x7 letter is a 25th of the screen). (above 0, up to 4096)
---@field depth integer How many voxels deep letters and icons are. (from 1 to 16)
---@field slant number How far the voxels' sides show at the top and bottom edges of the screen, in voxels per voxel of depth (the UI is seen in perspective from in front of the middle, so the sides facing the middle show). (from 0 to 4)
---@field display_font string A glyph atlas (PNG) for big letters: the logo, headings, messages.
---@field round number Big letters' front edges are rounded off with this radius, a share of their capital height (0: square). (from 0 to 1)
---@field grain number How much the colour of big letters varies voxel to voxel, 0..1. (from 0 to 1)

---@class Conf.state
---@field world table world.state at the start.
---@field player table player.state at the start.

---@class Conf.saves
---@field slots integer How many save slots the player has (0: no saving). (from 0 to 9)
---@field version integer The game's own version of what a save holds: count it up when that changes, on_save_loaded(version) brings an older save up to date.

---@class Kind A kind of thing (c.things.NAME = { ... }): what every kind may set; a behavior of a kit (c.kit) reads more.
---@field like? string Another kind this one builds on: its keys first, then this one's (like = "rifleman").
---@field behavior? string Its ready-made behavior, with a kit (c.kit): the jump & run's hero, walker, hopper, flyer, pickup, platform, prop, crate; the shooter's soldier, spawner, captive, gunship, pickup. Without one your script moves it.
---@field model? string Its model, a .vvx of the game.
---@field hp? integer The hits it takes: thing.state.hp at the start, on_death at 0. Without it, it takes none.
---@field fixed? boolean It stays exactly where it is put, in the air too (a kind without a behavior).
---@field width? number Its body across and deep, in voxels (c.physics.body_width unless set).
---@field height? number Its body's height, in voxels (its model's unless set).
---@field facing? number[] The way it faces as it comes in, { x, y } (x east, y north).
---@field props? table What your code reads of it, thing.props: props = { worth = 100 }.
---@field family? string|string[] A family, or a list of them: level:things("enemies") finds them, thing.families says them.
---@field effect? string|table Shown once where one of its things goes: a model ("poof.vvx", or { model = "poof.vvx", fps = 20 }), or "burst": it bursts into its voxels.
---@field sound? string Heard where one of its things goes: a sound's name (sounds/NAME.toml).
---@field reach? number Touched within this many voxels on the ground, not by its box: a pickup, a helipad.
---@field team? string Its team: the rounds, blows and blasts of a team hurt the other teams, not its own.
---@field immune? string[] What does not hurt it: "round", "melee", "blast".
---@field rotates? string "free": it faces exactly its way, a diagonal too; "quarter" (unless set): the nearest quarter turn.
---@field spin? number Its model turns round as it is drawn, degrees a second (a gem).
---@field frames? table Its model's frames for what it does, by name, first to last: frames = { walk = { 1, 4 } }; a set may say { first =, last =, fps =, stride =, loop = false }.
---@field fps? number How fast its frames go where a set does not say, frames a second (10).
---@field swing? table<string, number> A figure of parts: the parts that swing as it walks, degrees forward first: swing = { leg_left = 30, leg_right = -30 }.

---@class Kind.platformer.hero : Kind behavior = "hero" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field speed? number running, voxels a second (150 unless set)
---@field accel? number how quickly it gets there on the ground (1200 unless set)
---@field air_accel? number and in the air (800 unless set)
---@field jump? number how high a jump goes, voxels (29 unless set)
---@field jump_cut? number what is left of the rise when the jump is let go (0.5 unless set)
---@field coyote? number ticks off a ledge a jump still goes (6 unless set)
---@field buffer? number ticks a jump pressed early waits for the ground (6 unless set)
---@field climb? number up and down ladders, voxels a second (without left or right it moves to the ladder's middle; a ladder's top it stands on, down climbs down) (60 unless set)
---@field invulnerable? number ticks after a hit nothing hurts ... (60 unless set)
---@field blink? number and it blinks, this many ticks shown, as many not (0: steady) (4 unless set)
---@field knockback? number[] a hit throws it back and up, voxels a second ({ 100, 100 } unless set)
---@field fall_gravity? number gravity times this on the way down: snappy, not floaty (1: as up) (1.5 unless set)
---@field hang? number gravity times this at the top of a jump while jump is held ... (0.5 unless set)
---@field hang_speed? number "the top": rising or falling slower than this, voxels a second (30 unless set)
---@field corner? number a head clipping a ceiling's edge by up to this many voxels slips past (4 unless set)
---@field look_ahead? number the view leads this far where it runs, voxels (0: centred) ... (40 unless set)
---@field look_ahead_speed? number swinging round at this speed, voxels a second (90 unless set)
---@field grab? number how far above its head its hands catch a ledge's edge, voxels (0: none) ... (0 unless set)
---@field grab_ahead? number how far ahead of its body it finds it, voxels ... (3 unless set)
---@field hang_below? number how far under it its head hangs, voxels (an edge lower it pulls itself onto at once) ... (2 unless set)
---@field pull? number and the ticks pulling itself up (up or jump; down lets go) (24 unless set)
---@field face_ladder? boolean climbing, it turns to the ladder: its back to the camera (true unless set)
---@field air_jumps? number jumps it may make again in the air before landing (1: a double jump) ... (0 unless set)
---@field air_jump? number each this high from where it is then, voxels (20 unless set)

---@class Kind.platformer.walker : Kind behavior = "walker" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field speed? number voxels a second (60 unless set)
---@field ledges? boolean turns at a ledge as at a wall (true unless set)

---@class Kind.platformer.hopper : Kind behavior = "hopper" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field every? number ticks between hops, left and right in turn (120 unless set)
---@field hop? number[] how far and high: voxels a second across and up ({ 100, 200 } unless set)

---@class Kind.platformer.flyer : Kind behavior = "flyer" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field range? number up and down from where it is placed, voxels (50 unless set)
---@field period? number ticks for once up and down (120 unless set)
---@field face_hero? boolean it turns to the hero; false: it keeps the way it was placed (true unless set)

---@class Kind.platformer.platform : Kind behavior = "platform" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field travel? number[] how far it goes from where it is placed, across and up ({ 64, 0 } unless set)
---@field speed? number voxels a second (30 unless set)
---@field pause? number ticks it waits at each end (30 unless set)
---@field crushes? boolean coming down onto the hero: true hurts it (a trap), false waits above it (false unless set)

---@class Kind.platformer.prop : Kind behavior = "prop" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field stands? boolean true: it stands on the floor below where it is placed (a door, spikes) (false unless set)

---@class Kind.platformer.crate : Kind behavior = "crate" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field push? number pushed this fast at most, voxels a second (30 unless set)

---@class Kind.shooter.soldier : Kind behavior = "soldier" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field weapon? string The weapon he fires, a name of c.weapons. ("enemy_rifle" unless set)
---@field speed? number Running once he knows of the player, voxels a second. (24 unless set)
---@field idle_speed? number Walking about before that, voxels a second. (10 unless set)
---@field accel? number How quickly he gets to either, voxels a second per second. (300 unless set)
---@field sight? number How far he sees the player, voxels; a comrade falling he notices as far. (60 unless set)
---@field hearing? number gunfire (rounds hitting near him: half that) (70 unless set)
---@field blast_hearing? number How far he hears an explosion, voxels. (130 unless set)
---@field active? number further from the player he waits (150 unless set)
---@field range? number How far he shoots, voxels. (95 unless set)
---@field distance? number the distance he keeps from the player (32 unless set)
---@field step_dust? number of this many specks (0: none) (3 unless set)
---@field landing? number landing from higher than this, voxels, raises a puff ... (2 unless set)
---@field landing_dust? number of this many specks (10 unless set)
---@field muzzle? number[] Where his shots leave the gun, { ahead, up } of his feet in voxels (unset: from his height).
---@field stride? number A puff of dust every this many voxels he walks (unset: half his height) ...

---@class Kind.shooter.spawner : Kind behavior = "spawner" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field spawns? string The kind of thing it sends, a name of c.things. ("rifleman" unless set)
---@field count? number How many it sends in all. (2 unless set)
---@field first? number Ticks after the player comes within range before the first comes out. (60 unless set)
---@field every? number Ticks between one and the next. (420 unless set)
---@field range? number It sends them while the player is this near, voxels. (120 unless set)
---@field ahead? number[] Only while it lies this way of the player, { x, y } (x east, y north), by more than ahead_by; unset: on any side. ({ 0, 1 } unless set)
---@field not_near? number nobody comes out while he is nearer than this (0 unless set)
---@field ahead_by? number How far that way it must lie, voxels (unset: the player's height).

---@class Kind.shooter.captive : Kind behavior = "captive" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field speed? number Running off once freed, voxels a second. (30 unless set)
---@field accel? number How quickly it gets to that speed. (399 unless set)
---@field flee? string freed, it runs away from the player ([0, -1]: south; or a place's name) ("away" unless set)

---@class Kind.shooter.gunship : Kind behavior = "gunship" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field weapon? string Its gun, a name of c.weapons. ("door_gun" unless set)
---@field rockets? string Its rockets, a name of c.weapons. ("rocket_pods" unless set)
---@field speed? number Flying, voxels a second. (35 unless set)
---@field altitude? number over the pad it took off from (17 unless set)
---@field clearance? number at least this over what stands below (4 unless set)
---@field offset? number[] where it holds from the player (x east, y north) ({ 0, 65 } unless set)
---@field wake? number it lifts off when the player comes this close (130 unless set)
---@field size? number[] hit box: across, along, high ({ 12, 24, 8 } unless set)
---@field gun? number[] the guns' muzzles (across, mirrored; along; up) ({ 0, 12, 1.5 } unless set)
---@field pods? number[] the rocket pods' fronts ({ 5, 2.5, 2.5 } unless set)
---@field side_on? boolean true: it turns its side to the player, the gun on that side fires (false unless set)
---@field crash_blast? number The crater where it crashes, as a weapon's blast. (9 unless set)
---@field crash_damage? number to those near where it crashes (6 unless set)

---@class Kind.shooter.pickup : Kind behavior = "pickup" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field frame? number its model's frame (0 unless set)
---@field bob_ticks? number it bobs up and down, this many ticks each way, by bob voxels (20 unless set)
---@field bob? number It bobs up this many voxels and down again (0: it lies still; unset: a fifth of its model's height).

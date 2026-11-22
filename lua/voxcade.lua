---@meta voxcade
-- Voxcade's Lua API for editors (LuaLS), written by `voxcade api --lua`
-- from the engine's register of it (src/lua/api.rs): made anew, not edited.

---@class Point A point in voxels, { x = 12, y = 30, z = 4 }.
---@field x number Across, east (higher is further east).
---@field y number Deep, north (higher is further north).
---@field z number Up (higher is higher).

---@class Way A way on the ground, { x, y }. Only its direction counts, not its length. { x = 0, y = 1 } is north, { x = 1, y = 0 } east, { x = 0, y = -1 } south.
---@field x number East (below 0: west).
---@field y number North (below 0: south).

---@class Stick A stick's value (input.move, input.aim), never longer than 1.
---@field x number Right (below 0: left), -1 to 1.
---@field y number Up the screen (below 0: down), -1 to 1.

---@class PointerAt Where the pointer is this tick and what it is over (input.pointer).
---@field x integer Where it is across, in UI voxels from the screen's left (as frame.pointer.x).
---@field y integer Where it is down, in UI voxels from the screen's top.
---@field moved boolean The pointer is elsewhere than the tick before, or was not there then.
---@field hud? string The name of the HUD panel under it (frame:hud, a named menu or grid), the topmost; nil over none.
---@field index? integer The menu's row or the grid's cell under it, from 1.
---@field u? number Where in that panel across, 0 at its left to 1 at its right.
---@field v? number Where in that panel down, 0 at its top to 1 at its bottom.
---@field thing? Thing|Player The thing whose drawn voxels it is over (a figure, a level's piece, a model frame:model drew with it); nil once it is gone.
---@field part? string That figure's part under it, by its name.
---@field area? string The name of the level's area it enters first before it meets the level.
---@field at? Point Where it meets the level's voxels and solid pieces (glass, water and things looked through); nil into the sky or off the level.
---@field cell? Point The voxel it meets there, its x, y and z integers.

---@class PointerNow This machine's pointer in this picture (frame.pointer).
---@field x number Where it is across, in UI voxels from the screen's left.
---@field y number Where it is down, in UI voxels from the screen's top.
---@field hud? string The name of the HUD panel under it in the picture before.
---@field index? integer The menu's row or the grid's cell under it, from 1.
---@field thing? Thing|Player The thing under it in the picture before (a name to show on hover).
---@field part? string That figure's part under it.
---@field area? string The level's area it enters first.

---@class Typed What the player typed this tick (input.typed).
---@field text string The letters typed, "" mostly.
---@field erase integer How many letters Backspace took away.
---@field done boolean Enter was pressed.
---@field cancel boolean Escape was pressed.

---@alias Color string|number[] A color: "#rrggbb", or { r, g, b } with numbers from 0 to 255 (a fraction is fine). Where it can be see-through (frame:cube, text, box, bar, drop_in, an icon's letters) a fourth value too, "#rrggbbaa" or { r, g, b, a }: a from 0 (clear) to 255 (opaque, unless given); anywhere else a fourth value is a mistake. A table whose every number is at most 1 reads as the scale from 0 to 1 and is a mistake, but { 0, 0, 0 }, black. conf.lua's colors (c.light, c.effects, c.transition.color, c.title_screen.logo_colors, an icon's letters) and a level's file take the same.

---@class Place A named place of a level (level.places.NAME, level.arrived): where a player arrives, or a thing is put with thing:place.
---@field x number Across, east, in voxels.
---@field y number Deep, north, in voxels.
---@field z number The height of the feet there, in voxels.
---@field facing? Way The way it faces there, if the level's file says; else nil.
---@field name? string Only in level.arrived: the place's name ("start" if it arrived at the start).

---@class Item A kind of item (c.items).
---@field name string Its name, the key in an inventory.
---@field model? string Its model, to draw it (frame:model).
---@field stack integer How many one inventory holds at most.

---@class Daylight A picture's daylight (frame:daylight); what it leaves out is what c.light says. Its colors are Colors.
---@field sun? number[] The way the sunlight travels, { x, y, z } with z below 0 (towards the ground).
---@field sun_color? Color The sunlight's color (a darker one shines weaker): white at noon, warm at dusk, a pale blue moon.
---@field ambient? number Light every face gets, 0 to 1.
---@field shadow? number How much a shadow darkens, 0 to 1.
---@field sky? Color The background's color.

---@class Light A light as frame:light{ ... } writes it, the words a level's light takes too; at and radius are needed.
---@field at Thing|Point|number[] Where it shines from: a thing (its middle) or a point { x, y, z } in voxels.
---@field radius number How far it reaches, voxels.
---@field color? Color Its color (a darker one shines weaker), white unless set.
---@field falloff? number How it fades to its radius: (1 - distance / radius) to this power, 0 to 16 (2 unless set; lower reaches further bright).
---@field cone? number A spot: its cone's whole opening angle, 1 to 360 degrees; left out, it shines every way.
---@field blend? number How much of the cone's edge is soft, 0 to 1 (0.15 unless set).
---@field rotation? number The spot's way round, degrees clockwise seen from above, north (+y) 0, -360 to 360.
---@field pitch? number The spot's way up (above 0) or down (below 0: -90 straight down), -90 to 90 degrees.
---@field shadows? string What stops it: "static" (the level's voxels and its pieces), "all" (moving things too: figures, things, models drawn) or "none" (it shines through). Left out, c.light.lamp_shadows says ("static" unless set); a light's own word beats it.
---@field fog? number How strongly its light shows in fog, 0 to 10 (1 unless set; 0 not at all).

---@class Fog A picture's fog (frame:fog); what it leaves out is what c.light says (fog_density, fog_color, fog_anisotropy, fog_glow). Its color is a Color.
---@field density? number How thick, 0 to 1: the share of light a voxel of air takes (0: no fog).
---@field color? Color The fog's own color where no lamp lights it.
---@field anisotropy? number How much it sends a lamp's light on ahead rather than back, -0.9 to 0.9 (0: every way alike).
---@field glow? number How brightly it shows the lamps' light, 0 to 20; the dimming is the density's alone (1: as the air would; 4 a stage's haze).

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
---@field at? integer The bar it begins at, counted from 1 (its start, unless said): 33 skips an intro of 32 bars. Its beat 0 is there.
---@field channels? table<string, string> Channels of MML laid over a file of instruments, by letter: { K = '@kick o4 l16 c r r r c r r r c r r r c r r r' }. The whole set that plays.

---@class MusicStop How a song stops (music:stop).
---@field from? string When: now (unless said), at the next beat or bar of the song that plays.
---@field fade? number Seconds it fades out over, 0 to 60 (0, unless said: at once, still without a click).

---@class MusicMix A channel's strip on the mixing desk (music:mix).
---@field volume? number|false Its fader, 0 (silent) to 1 (as written); false: 1.
---@field mute? boolean true silences it, the fader kept.
---@field pan? number|false -1 (left) to 1 (right), 0 the middle; false: the middle.

---@class LevelChange How the level is left (world:go_to, world:restart, world:load, world:win, world:lose).
---@field fade? number Seconds the picture fades out through c.transition.color, and as many in, 0 to 10 (0: a cut); unless said, c.transition's.
---@field fresh? boolean world:go_to and world:restart only: true enters the level as its file has it, forgetting what it kept of earlier visits (a level of a location, a persistent one, things of a persistent kind).

---@class Menu A menu in the engine's look (frame:menu).
---@field name? string Its name as the pointer says it (input.pointer.hud).
---@field rows (string|MenuRow)[] Its rows, from the top.
---@field at? integer The chosen row, from 1.
---@field title? string Above the rows.
---@field tabs? string[] Above the rows, one chosen.
---@field tab? integer The chosen tab, from 1.
---@field hint? string Below: what the keys do, {act} BUY.
---@field shown? integer How many rows show at most.
---@field x? number Its top left on the HUD, across, in UI voxels (else it is in the middle).
---@field y? number Its top left on the HUD, down, in UI voxels (else it is in the middle).
---@field width? number How wide, in UI voxels (else as wide as it needs).

---@class GridCell A slot (frame:grid).
---@field icon? string An icon of c.icons.
---@field model? string A model file of the game, its front shown in the slot.
---@field count? integer A number at its corner.
---@field dim? boolean Darker.

---@class Grid Slots in rows (frame:grid).
---@field name? string Its name as the pointer says it (input.pointer.hud).
---@field cells (GridCell|false)[] The slots, false for an empty one.
---@field columns? integer Across (up to 8 unless said).
---@field cell? number A slot's size in UI voxels (18).
---@field at? integer The chosen slot, from 1.
---@field title? string Above the slots.
---@field x? number Its top left on the HUD, across, in UI voxels (else it is in the middle).
---@field y? number Its top left on the HUD, down, in UI voxels (else it is in the middle).

---@class Rotation How much further a model is rotated, in degrees (frame:model).
---@field rotation? number Round, clockwise seen from above (90: a quarter to the right).
---@field pitch? number Its nose up (below 0: down).
---@field roll? number Its right side down, as when banking right (below 0: its left).

---@class PoseEntry How a part of a figure of parts is posed (frame:model's pose, frame:pose, a level's piece's thing.state.parts). Each key may be left out.
---@field rotation? number Turned about its joint, round, clockwise seen from above, in degrees.
---@field pitch? number Its front up about its joint, in degrees (below 0: down).
---@field roll? number Its right side down about its joint, in degrees (below 0: its left).
---@field offset? number[] Moved by voxels after its rotation, { x, y, z }, fractions too. What hangs on it moves along.
---@field glow? number How much every voxel of it shines in its own color, 0 to 1, in place of the file's: 1 a lamp lit, 0 a lamp out. Nil: as the file says. It lights nothing round it (frame:light does).

---@class DrawOptions How a model is drawn on the HUD (frame:draw).
---@field frame? integer Which of its frames, from 0 (0 unless given).
---@field rotation? integer Which side faces you, degrees clockwise seen from above, quarter turns only: 0 its front (+y), 90 its left side, 180 its back, 270 its right side.

---@class SaveInfo What lies in a save slot.
---@field level string The level it was saved in.
---@field seconds integer The play time when it was saved.
---@field label? string The label the game gave it (world:save's options), nil when it has none.

---@class Saves What lies in the save slots: the slots by number, and which was saved last.
---@field [integer] SaveInfo The save in that slot, nil while the slot is empty.
---@field newest? integer The number of the slot saved last, nil while every slot is empty.

---@class SaveOptions What a save is given (world:save).
---@field label? string One line of 80 characters at most to tell the save by (a chapter, a place).

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
---@field leaving string|nil Why the level is about to be left at the end of this tick ("go_to", "restart", "load", "won", "lost", "new_game"), or nil: the ask that counts so far. One ask counts in a tick: going on (go_to, win, new_game) before starting over (restart, load, lose); among those of one rank the first asked. (Leaving by QUIT TO TITLE or a level file saved while playing is no ask: on_level_end hears "quit" and "reload".)
---What lies in each save slot (c.saves.slots of them): world.saves[1] is nil while slot 1 is empty, else { level, seconds, label }.
---- label is what the game gave that save in world:save's options, nil when it gave none (a save made from the pause menu has none).
---- world.saves.newest is the number of the slot saved last, nil while every slot is empty: world:load(world.saves.newest) is what CONTINUE does. It is not met by pairs or ipairs, which go over the numbered slots only.
---- Slots made or emptied by world:save and world:delete_save show from the next tick.
---@field saves Saves
---@field players (Player|false)[] Every seat of the game (c.players, 1 unless set), in order: world.players[n] is the player on seat n, or false while nobody sits there (#world.players is how many seats; world.players[1] is player). A HUD shows PRESS START where a seat is false. Players in play in this level: level:players().
---@field items Item[] The kinds of item (c.items) in their order (c.items.NAME.order, then by name), each { name, model, stack }: for drawing an inventory.
local World = {}

---Leaves this level for another at the end of the tick (the code after it still runs): by name or number (world:go_to(level.next) goes on in order), arriving at the place called place, else where the level starts. player.state.checkpoint is forgotten. One ask counts in a tick: world.leaving.
---- Between levels of one location (location = "station" in their files) each is as it was left and level.state goes on; leaving the location forgets them, but for what is persistent.
---- options: { fade = seconds } for this change alone, out and as many in, 0 to 10. 0 is a cut: world:go_to("b", "door", { fade = 0 }). 1.2 is a slow fade for a new chapter. Unless said, c.transition's. Without a place: world:go_to("b", { fade = 0 }). { fresh = true }: the level as its file has it, what it kept forgotten.
---@param level string|integer
---@param place? string|LevelChange
---@param options? LevelChange
function World:go_to(level, place, options) end

---Enters this level again at the end of the tick, as it was entered this visit: everything from its file again, level.state fresh (a level that remembers: with what it kept then, and in a location level.state as it came in); the player at place, else at the place player.state.checkpoint names, else where it starts. One ask counts in a tick: world.leaving.
---- options: { fade = seconds } for this change alone, as world:go_to's: world:restart({ fade = 0 }) starts over at once. { fresh = true }: the level as its file has it, what it kept forgotten.
---@param place? string|LevelChange
---@param options? LevelChange
function World:restart(place, options) end

---The text of a file in the game folder, by its path there (world:read("levels/microban.txt")): only while main.lua starts: at its top, in on_world_start or the first on_level_start. The files read count for the replays, and saving one runs main.lua anew.
---@param name string
---@return string
function World:read(name) end

---Saves the game into a slot (1 to c.saves.slots) at the end of the tick, or of the on_level_start that asks: for save points, or saving on its own. The player can save from the pause menu too (such a save has no label). Never on your game's own title (c.title_screen.level).
---- options: { label = "Chapter 2: the station" } tells the save by a line of your own, one line of 80 characters at most: world.saves[slot].label reads it back, and the engine's own SAVE and LOAD pages show it in place of the level's name.
---- Kept: world.state, player.state, every level's level.kept, the level being played with its level.state and the voxels changed in it (blown away, built), and where the player stands.
---- Not kept: the level's things, but in a level that remembers (a location's, a persistent one, things of a persistent kind: what the levels kept, by name). They come from its file when it is loaded; a thing kept in level.state comes back as nil, so keep what you need to place them again.
---@param slot integer
---@param options? SaveOptions
function World:save(slot, options) end

---Goes on from the save in a slot at the end of the tick, like a level change (on_level_end("load"), then on_level_start("load")): the states as they were saved, the level from its file with the voxels changed in it as they were, the player where it stood. An empty slot is a mistake: world.saves tells first. On your game's own title (c.title_screen.level) it begins the run from the save, as CONTINUE does. One ask counts in a tick: world.leaving.
---- options: { fade = seconds } for this change alone, as world:go_to's.
---@param slot integer
---@param options? LevelChange
function World:load(slot, options) end

---Empties a save slot for good at the end of the tick (or of the on_level_start that asks), as world:save is done: world.saves[slot] is nil from the next tick on. An empty slot is a mistake: world.saves tells first. Allowed in a run and on your game's own title (c.title_screen.level), for a load menu that lets the player delete a save. Only this game's own slot is emptied, its file is gone and cannot be brought back; shot, check, inspect and replays keep their saves for the run only, so they delete nothing on disk.
---@param slot integer
function World:delete_save(slot) end

---Holds the game still for this many ticks: a hit 2 or 3. Everything stops, on_tick too. 60 (a second) at most: more is cut to 60. It is part of the game, so replays see it.
---@param ticks integer
function World:hitstop(ticks) end

---The run is won at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: world.leaving.
---- options: { fade = seconds } for this change alone, as world:go_to's.
---@param options? LevelChange
function World:win(options) end

---The run is lost at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: a win or a go_to in the same tick counts before it (world.leaving).
---- options: { fade = seconds } for this change alone, as world:go_to's.
---@param options? LevelChange
function World:lose(options) end

---On your game's own title (c.title_screen.level) only: a new run begins at the end of the tick, in the level a run begins in (c.start, else the first but the title), as from the engine's title: on_world_start, on_level_start("start"), the states fresh, tick 0. Nothing of the title is kept. world:load(slot) there goes on from a save instead. One ask counts in a tick: world.leaving.
---- options: { fade = seconds } for this change alone, as world:go_to's.
---@param options? LevelChange
function World:new_game(options) end

---Opens a page of the engine's over the game, the game held while it shows as by the pause: "settings" (SETTINGS: the volumes, the picture, the keys). Closing it goes back to the game. For your own title's menu or pause menu. Only the window opens it: the game plays the same without one (a replay, shot, inspect).
---@param page string
function World:open(page) end

---Ends the program, as QUIT on the engine's title: for your own title's menu. In the IDE's PLAY it stops the game; shot and inspect end the run there. Only the window does it: the game plays the same without one.
function World:quit() end

---Back to the title after this tick, as QUIT TO TITLE in the pause menu: the run ends (on_level_end and on_world_end hear "quit"), a score that beats a line of the high score table asks for initials, then the title shows; a game with a title of its own (c.title_screen.level) goes back to that level. For your own menu's QUIT, a game over screen's button. On the title itself it is a mistake. In the IDE's PLAY it stops the game; shot and inspect end the run there. Only the window does it: the game plays the same without one.
function World:to_title() end

---@class Level The level being played: its voxels, its things, what happens in it, its camera.
---@field state table Your game's data for this level while it is played: the things in it too, moves, time. Fresh each time the level is entered, a restart too; after world:load as it was saved (its things nil). The levels of one location (level.location) share it: it goes on as the player changes among them (things in it nil then), and is fresh when the location is entered from outside. The engine reads one key: level.state.held = true holds the level (the ready-made rules, the physics and touches wait; on_tick, on_draw and the time go on): for a message, a shop, a tally.
---@field name string Its name: its file's name without the ending (hall for levels/hall.toml); main for a game's one level made in code.
---@field number integer Its place among world.levels, from 1.
---@field next string|nil The next level's name, nil after the last one. Your game's own title (c.title_screen.level) is passed over.
---@field visits integer How often it was entered in this run, restarts too: 1 the first time.
---@field location string|nil The location it belongs to, as its file says (location = "station"), nil for none. While the player changes among the levels of one location, each is as it was left (its things by their names: removed ones stay removed, the others where they stood, thing.state, its voxels) and they share level.state. Leaving the location forgets that, but for what is persistent (persistent = true in a level's file, or on a kind). No time passes in a level nobody is in: level.ticks_away.
---@field ticks_away integer The ticks since this level was left, when it comes back as it was left (a level of a location, a persistent one, things of a persistent kind); 0 on a first visit and where it kept nothing. Nothing moves in a level nobody is in: on_level_start can make up what would have happened meanwhile (a timer in thing.state counted down by level.ticks_away).
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

---Sets the voxel at a cell: a Color (taken into the level's palette), a palette index (an integer), or nil or 0 for empty. A cell outside the level (level.size) is a mistake: the game halts.
---@param x integer
---@param y integer
---@param z integer
---@param voxel integer|Color|nil
function Level:set_voxel(x, y, z, voxel) end

---Fills the box from one corner cell to the other, both included, like set_voxel: a Color, a palette index or nil/0 (only what of the box lies inside the level); nil or 0 empties it. A level made in code (c.world.size) is built this way.
---@param x0 integer
---@param y0 integer
---@param z0 integer
---@param x1 integer
---@param y1 integer
---@param z1 integer
---@param voxel integer|Color|nil
function Level:fill(x0, y0, z0, x1, y1, z1, voxel) end

---Whether the cell stops bodies: a voxel there (soft voxels, grass and shrubs, do not), or a solid piece's (solid = true in the level's file), or a solid thing's (c.things.NAME.solid).
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

---Every thing (of a kind, or of a family: c.things.NAME.family), in the order they came: not the player (it belongs to no level), not the dead, not what was removed this tick. level:things("piece") is every model and light the level's file set down.
---@param kind? string
---@return Thing[]
function Level:things(kind) end

---The players playing in this level, in seat order: someone sits there, not out of play (state.out), and its figure stands (one taking a seat comes in at the tick's end). Loop over it for every player's rules: for _, p in ipairs(level:players()) do p:walk(p.input.dir, 40) end. world.players is every seat, the out too (a HUD, a continue).
---@return Player[]
function Level:players() end

---How many things (of a kind or family) there are: not the player, not the dead, not what was removed this tick (count deaths in on_death).
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
---The way the camera looks in this level, degrees clockwise seen from above (0 north): the level file's [camera] yaw, else c.camera.yaw. Only to read.
---The engine turns no keys for a game in Lua alone: input.dir and input.move are the keys as pressed, screen directions. Seen from behind (180), a game whose up walks north turns them: if level.camera.yaw == 180 then x, y = -x, -y end. The ready-made rules turn their own.
---@field yaw number
local Camera = {}

---The camera follows this thing from now on; following the player, ready-made rules lead the view as theirs do (the platformer kit's look ahead). Every level starts following the player.
---@param thing Thing
function Camera:follow(thing) end

---The camera rests over this point from now on, following nothing, whatever spawns (follow follows a thing again): a room, a boss arena, a scene, a field that scrolls by itself.
---@param x number
---@param y number
function Camera:look_at(x, y) end

---@class Player : Thing The one who plays: a thing that belongs to no level and goes from level to level (everything a thing has); its state is the run's. Its figure is the ready-made rules' player, a thing of c.player.kind, or none (it then stands nowhere).
---The player's data for the whole run, from level to level. It starts as c.state.player says. Plain data, no things; lives and the like are your game's own keys. A script reads and sets the engine's keys like its own.
---- score: an integer.
---- inventory: items by name and how many (player.state.inventory.key).
---- checkpoint: the name of a place or a thing in this level where world:restart goes. Forgotten on world:go_to.
---- hp: with a figure that has hit points, or with a kit.
---- typing: true while the player types: the keyboard's letters, Backspace, Enter and Escape hold no action then (Escape does not pause), and input.typed says what came. Every player has it (p.state.typing).
---With a kit (c.kit):
---- invulnerable: ticks left without hits, or true until set back.
---With the platformer kit (c.kit = "platformer"):
---- pose: the hero's set of frames, as a thing's thing.state.pose.
---With the shooter kit (c.kit = "shooter"):
---- weapon: the gun in hand, by its name in c.weapons (held in c.player.attacks).
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
---With the platformer kit (c.kit = "platformer"):
---- pose: which set of its frames it shows (c.things.NAME.frames), from the tick it is set. A prop, a door or a sign always shows one ("open"; "idle" the first). Another kind shows it over its own animation ("attack"), nil for its own again. The hero's is player.state.pose.
---- goal: for a platform, "end" or "start": it goes to that end of its travel and stays (a trapdoor, a lift). nil: to and fro again.
---With the shooter kit (c.kit = "shooter"):
---- freed: for a captive. Set true: the cage bursts, it runs off.
---- flying: for a gunship, up in the air fighting. Only to read.
---For a piece of a level's file:
---- light: a lamp's brightness, all its lights together, 0 (off) to 1 (the start). Their colors, places and reach are the file's.
---- parts: a figure of parts posed, as frame:model's pose: { leaf = { rotation = 90 } } swings a door's leaf open, { lamp = { glow = 0 } } puts a lamp out; nil at rest. Drawn so every picture, for this visit of the level: keep what must stay open in world.state and set it again in on_level_start. A solid piece still blocks where its model stands at rest. Only a level's piece is posed by its state for now, not a thing of a kind. What changes only for the eye (a lamp blinking, the part under the pointer lit) is frame:pose's.
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
---@field props table Fixed values your code reads of this thing: its kind's props (c.things.NAME.props = { worth = 100 }), over them what its level's file gave it (props = { worth = 500 } on its [[thing]], PROPS in the level editor) or level:spawn did. Only to read; what changes goes in thing.state.
local Thing = {}

---Walks this tick the way given at this speed (voxels a second), facing it. Only the way's direction counts, not its length: { x = 0.3, y = 7 } walks at the full speed. It falls, steps up ledges and lands by itself. Only things your script moves (a kind with no behavior in c.things, or one whose behavior thing:enable switched off).
---@param way Way
---@param speed number
function Thing:walk(way, speed) end

---Turns it to the nearest of the 8 ways: things your script moves, and the player with ready-made rules (it turns again as it moves; the platformer kit's hero only left or right).
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
---A kind's team and immune do not stop it: your code decides whom it hurts. While it may not be hurt (player.state.invulnerable) nothing happens. on_hit runs in the next tick's events, and so does on_death when the hit points reach 0, after on_hit. A thing of a kind without a behavior stays in level:things and level:count until its on_death has run.
---@param damage integer
---@param by? Thing
---@param weapon? string
function Thing:hurt(damage, by, weapon) end

---The things it touches now (of a kind or family), in the order they came: for what lasts while touched (spikes, an enemy's contact, lava), asked in on_tick, where it stays true every tick. Not the dead, nor what was removed this tick.
---@param kind? string
---@return Thing[]
function Thing:touching(kind) end

---Whether any part of its body is in a block of the level of this kind: "hazard", "ladder", "oneway" or "solid" (level:block for one voxel).
---@param kind string
---@return boolean
function Thing:in_block(kind) end

---Sends it up this high (voxels), standing or in the air: off an enemy it landed on, a spring. Always the full height; the jump button plays no part. Things your script moves, and the platformer kit's hero (c.kit = "platformer").
---@param height number
function Thing:bounce(height) end

---Sets it moving the way given at this speed (voxels a second), standing or in the air: a dash (player:launch(player.facing, 240)), a wall jump, a knockback. Only the way's direction counts, not its length.
---- rise: how high it goes up too, in voxels.
---Physics carries it on, and a kit's rules steer it on from there (the hero as the player presses, a soldier as he means to go): thing:enable switches them off for a while. For things your script moves, things whose behavior is off, the player, the hero, walkers, hoppers, soldiers and captives.
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

---Takes it out of the level at the end of this tick: it can still be read until then, but it is out of the level's lists, counts and touches at once. Whether it was there to remove. No on_touch_end and no on_death is told for it. Its kind's effect and sound play where it goes. Not the player: it goes from level to level.
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
---@field dir Way The directions held as a way: x and y each -1, 0 or 1. The keys as pressed, whichever way the level's camera looks (level.camera.yaw).
---@field move Stick Where the left stick leans, { x, y }: x right, y up, each -1 to 1, never longer than 1 (a light lean is small, a full one 1). Without a stick it is the directions held: 1 one way, about 0.71 each on a diagonal. A shooter that walks at any angle, a car's steering.
---@field aim Stick Where the right stick leans, { x, y } as input.move; with a mouse in a game with a pointer (c.pointer) the way on the screen from the player's figure to the pointer, length 1; { x = 0, y = 0 } with keys alone. A twin-stick shooter's aim, the same code for the pad and the mouse.
---Where the pointer is this tick and what it is over, in a game with one (c.pointer): nil without, off the picture, for a player on a pad, and when the pad was used last.
---- x, y: where it is, in UI voxels from the screen's top left, as frame:text's and frame.pointer's (frame.width by frame.height). The player's own screen: its width may differ on another machine, so ask hud what it is over.
---- moved: true when it is elsewhere than the tick before, or was not there then (the first tick, the mouse back on the picture or used again after the pad); what it is over changing alone is no move. Show your own cursor again when it moved after the keys were used.
---- hud: the name of the HUD panel under it (frame:hud, or a menu or grid with a name), the topmost; nil over none.
---- index: that menu's row or grid's cell, from 1.
---- u, v: where in the panel, 0 to 1 from its top left (a slider, a map).
---- thing: the thing whose drawn voxels it is over (a figure as drawn and posed, a level's piece, a model frame:model drew with it), part: that figure's part by name; nil once the thing is gone.
---- area: the name of the level's area it enters first before it meets the level (a door in a wall).
---- at: where it meets the level's voxels and solid pieces, { x, y, z } (glass, water and things looked through; nil into the sky or off the level), cell: the voxel it meets there, { x, y, z } integers (a grid game's cell).
---Under a panel the level's answers are nil: a panel covers. It is what the player saw: a click carries what was under the pointer as the button went down. The mouse's buttons are keys (c.keys.click = { "mouse_left" }): input:pressed("click") with input.pointer.hud is a click on a panel. A pointer from the picture of a level left is nil.
---@field pointer PointerAt|nil
---What the player typed this tick while player.state.typing is true (else nil).
---- text: the letters typed, "" mostly; only those the small letters can write.
---- erase: how many letters Backspace took away.
---- done: Enter was pressed. cancel: Escape was pressed.
---The letters come once and in order, in replays and over the network too. A name at the start: typing = true, add text, take erase away, typing = false on done.
---@field typed Typed|nil
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
---@field pointer PointerNow|nil This machine's pointer in this picture, in a game with one (c.pointer): x and y in UI voxels as frame:text's, and hud and index, thing, part and area of what it is over in the picture before (a name over a thing as the pointer rests on it). Nil with the pad used last. To draw your own cursor (c.pointer = "own") or light the row under it. Only for drawing: on_tick reads input.pointer.
---@field players Player[] The players at the machine this picture is drawn for, in seat order (at one machine every taken seat, the out too; over the network that machine's own): their HUD. A key in braces in a text, and controls in on_draw, are the first of them's (seat 1's where none plays there); another player's is p.controls:key.
---@field width number How many UI voxels the HUD is across: it follows the screen's shape, so put things at the right edge from it (frame.width - 3 - frame:text_width(score)).
---@field height number How many UI voxels the HUD is from top to bottom (c.ui.height).
---@field alpha number How far into the tick this picture is, 0 to 1: add it to a count of ticks for motion that runs smooth between them.
local Frame = {}

---Draws a model where a thing stands, facing its way. file is any model file of the game, by its path in the game folder ("sheet.vvx", "props/sheet.vvx"): loaded the first time it is drawn, let go when the level is left; a file that is not there is a mistake. The thing is a thing, never a point.
---- index: which of its frames, from 0. Without it, its kind's frames for what it does (c.things.NAME.frames: idle, walk, jump), else the model's own: 0 standing, 1 in the air, the rest walking, a frame every half its height walked.
---- rotation: turns it further, in degrees about its middle (the voxel at half its height over its pivot). { roll = 20 } banks a ship right, { rotation = 45 } faces a diagonal, { pitch = 10 } lifts its nose. Roll is taken first, then pitch, then rotation, as for a level's piece. Off the quarter turns its voxels leave the grid.
---- pose: moves a figure of parts (a model whose parts have names: head, body, arm_left ...), each part by its name, a rotation about its joint, what hangs on it moving along. { head = { rotation = 30 }, arm_right = { pitch = 90 } } raises the right arm forward. offset = { x, y, z } moves a part by voxels (fractions too) after its rotation: { bust = { pitch = 10, offset = { 0, 1, 0 } } }. glow = 1 makes every voxel of a part shine in its own color, glow = 0 puts out one that glows in the file (a PoseEntry). It goes over its kind's swing (c.things.NAME.swing).
---A kind with rotates = "free" faces exactly its way (a diagonal too), else the nearest quarter turn. A model's pivot is the middle cell of its size across and deep: of an even size one voxel off the middle, so facing -y or sideways it fills cells a voxel further on the flipped axis (use odd sizes where that matters).
---@param file string
---@param thing Thing
---@param index? integer
---@param rotation? Rotation
---@param pose? table<string, PoseEntry>
function Frame:model(file, thing, index, rotation, pose) end

---Poses a level's piece for this picture only, each part by its name, over the pose its state holds (thing.state.parts): a rotation turned on top, an offset added, a glow in place of the state's. For what changes only for the eye: a lamp blinking, the part under the pointer lit. frame:pose(machine, { led_3 = { glow = 1 } }) lights a lamp.
---Called twice for one thing, each part keeps what both give: a glow from one and a rotation from the other; a key given twice, the later.
---The game's rules never see it: not in a state, a replay or a save. The pointer meets a part where this picture drew it. frame.pointer.part is of the picture before: light the part under the pointer, but move it from on_tick, or it can slip out from under the pointer and back.
---Only a level's piece for now, not a thing of a kind.
---@param thing Thing
---@param pose table<string, PoseEntry>
function Frame:pose(thing, pose) end

---Draws one voxel in the level, at x, y, z in the level's voxels. Its color is a Color (white unless given), with a fourth value below 255 see-through. glow is 0 (none) to 1.
---@param x number
---@param y number
---@param z number
---@param color? Color
---@param glow? number
function Frame:cube(x, y, z, color, glow) end

---Lights the level round a thing or a point, in this picture: frame:light(at, radius, color), or frame:light{ at =, radius =, ... } with the words a level's light takes too. These two forms only: more words go in the table.
---- at: where it shines from, in voxels: a thing (its middle, half its height over its feet) or a point, { x, y, z } or { x =, y =, z = }.
---- radius: how far it reaches, voxels, above 0.
---- color: its color, a Color (a darker one shines weaker), white unless set.
---- falloff: how it fades to its radius, (1 - distance / radius) to this power, 0 to 16, 2 unless set. Lower reaches further bright.
---- cone: a spot, its whole opening angle in degrees, 1 to 360; left out (or 360), it shines every way.
---- blend: how much of the cone's edge is soft, 0 to 1, 0.15 unless set.
---- rotation: the spot's way round, degrees clockwise seen from above, north (+y) 0, -360 to 360.
---- pitch: the spot's way up or down, degrees, -90 to 90. Above 0 up, below 0 down: -90 straight down, 0 level.
---- shadows: what stops it, "static", "all" or "none" (it shines through walls); left out, c.light.lamp_shadows says ("static" unless set). A wall is the level's voxels and its pieces, not a piece that brings a light of its own (a lantern round its flame); with "all" the figures, things, models and cubes drawn stop it too, so they cast its shadow (it costs more: for a few chosen lamps), but a light at a thing is never stopped by that thing's own models (a lantern a figure carries). Glass stops no light and tints none.
---- fog: how strongly it shows in fog (frame:fog), 0 to 10, 1 unless set.
---Only the picture: the game's rules do not see it.
---@param at Thing|Point|Light
---@param radius? number
---@param color? Color
function Frame:light(at, radius, color) end

---Sets the sun and the sky for this picture: a day and a night. What you leave out is what c.light says (its sun, sun_color, ambient, shadow, sky).
---- sun: the way its light travels, { x, y, z } with z below 0.
---- sun_color: its color, a Color (a darker one shines weaker), white unless set.
---- ambient: the light every face gets, 0 to 1. shadow: how dark shadows are, 0 to 1.
---- sky: the sky's color, a Color.
---Work it out from your game's clock every picture: the sun low and warm at dawn, white at noon, orange at dusk, a dim blue at night with frame:light lamps. Only the picture: the game's rules do not see it. A game whose code names daylight has every face a sun above the horizon can light in its level.
---@param light Daylight
function Frame:daylight(light) end

---Fills the level's air with fog for this picture: the lamps' light shows in it, a spot as a beam, and what lies further fades to the fog's color. What you leave out is what c.light says (fog_density, fog_color, fog_anisotropy, fog_glow).
---- density: how thick, 0 to 1 (0.004 a light haze in a big hall, 0.03 thick; 0 none).
---- color: the fog's own color where no lamp lights it, a Color (dark at night).
---- anisotropy: above 0 a beam is brighter looking towards its lamp, below 0 looking away (0: alike).
---- glow: how brightly it shows the lamps' light, 0 to 20; the dimming is the density's alone (1 as the air would; 4 a stage's haze: bright beams, the room clear; 0 it only dims). Higher is never fainter, and a beam never gets brighter than its lamp's light (times the light's fog).
---Walls stop a lamp's beam where they stop its light (shadows); a light's fog says how strongly it shows. The fog fills the level's box, not the air between the camera and the level. Only the picture: the game's rules do not see it.
---@param fog Fog
function Frame:fog(fog) end

---Moves the camera for this picture, for a scene: a zoom, a cut, a slow pan. What you leave out is the game's camera's (c.camera): { width = 80 } zooms in on what the camera follows and keeps following it.
---- look_at: the point the view centers on, { x, y, z } in voxels (z left out: the camera's height), or a thing.
---- width: how many voxels the picture shows across there; or height, from bottom to top.
---- pitch: degrees below the horizon. fov: the field of view.
---Only this picture: nothing is kept and nothing smoothed. Call it in every picture the scene lasts and work a pan out from your clock (world.tick + frame.alpha, music.beat). The game's rules, the sounds and level.camera do not see it; frame:to_screen after it goes by it. The camera does not turn round yet. Another pitch or fov than c.camera's needs faces the game's own view never shows: the level is built anew with them once, a moment's hitch the first time.
---@param camera PictureCamera
function Frame:camera(camera) end

---Names a rectangle of this picture's HUD the pointer can be over: x and y its top left, in UI voxels as frame:text's. Nothing is drawn. In on_tick input.pointer.hud is its name while the pointer is over it, u and v where in it. One named later lies on top. A name is a word of letters, digits and _, up to 16 long. A menu or grid with a name is one too.
---@param name string
---@param x number
---@param y number
---@param width number
---@param height number
function Frame:hud(name, x, y, width, height) end

---Draws a menu in the engine's look, on a plate. In the middle of the screen unless x and y say where (its top left), as wide as it needs unless width says.
---- rows: each a text, or { text =, value =, icon =, dim = }: a value at the right, an icon of c.icons before the text, dimmed when it cannot be had.
---- at: the chosen row, from 1, yellow with the engine's red marker.
---- title, tabs (tab: the chosen one, from 1), hint below; an action in braces is its key.
---- shown: how many rows show at most; a longer list scrolls to keep at in view.
---What is open and where the cursor stands is your game's: keep it in level.state, hold the level while it is open (level.state.held = true) and move it in on_tick (input:repeated).
---A menu is a panel the pointer can be over: with a name (name = "shop") input.pointer.hud says it, index the row.
---@param menu Menu
function Frame:menu(menu) end

---Draws an inventory's slots in rows, on a plate: cells, each { icon =, model =, count =, dim = } (an icon of c.icons, or a model file of the game shown from the front, a number at its corner, darker when dim) or false for an empty slot; columns across (up to 8 unless said), cell its size in UI voxels (18), at the chosen slot (from 1) edged in yellow, an optional title. In the middle unless x and y say where. With a name it is a panel the pointer says (input.pointer.hud), index the slot.
---@param grid Grid
function Frame:grid(grid) end

---Writes text on the screen, over the level. x and y are its top left, in UI voxels from the screen's top left (frame.width across, frame.height down).
---- color: a Color, a fourth value see-through.
---- size: 1 the small letters (7 voxels high), 2 to 4 the big ones (c.ui.display_font).
---- An action in braces is its key as the player sees it: "PRESS {jump}" writes PRESS X, or the pad's button (controls:key). {{ writes a brace.
---Written as you write it: lower case, the letters of German and French (ä ö ü ß é è ê à ç œ …) and signs such as · × → ▲ ▼ ✓ … „ « ». A display font has capitals only; a key's name is in capitals.
---What is drawn first on the HUD lies on top: where two things share a voxel, the first keeps it (a box drawn later does not cover an icon drawn before).
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

---Draws a filled box on the screen, its top left at x, y, in UI voxels. color: a Color, a fourth value see-through. What is drawn first on the HUD lies on top: a box drawn later does not cover an icon or text drawn before.
---@param x number
---@param y number
---@param width number
---@param height number
---@param color? Color
function Frame:box(x, y, width, height, color) end

---Draws a bar on the screen filled the fraction (0 to 1) of its width: a boss's health, a timer. color the filled part (red unless given), back the rest, each a Color, a fourth value see-through. What is drawn first on the HUD lies on top: a bar drawn later does not cover an icon or text drawn before.
---@param x number
---@param y number
---@param width number
---@param height number
---@param fraction number
---@param color? Color
---@param back? Color
function Frame:bar(x, y, width, height, fraction, color, back) end

---Draws a small picture of c.icons on the screen, its top left at x, y: c.icons.heart = { rows = { ".RR.RR.", ... }, colors = { R = { 214, 36, 44 } } }: a letter's color is a Color, a fourth value see-through. What is drawn first on the HUD lies on top: draw an icon before the box it sits on.
---@param name string
---@param x number
---@param y number
function Frame:icon(name, x, y) end

---Draws a model file of the game on the screen, its top left at x, y, in UI voxels: a key in the corner, a face beside what a figure says. file is any model file of the game by its path in the game folder ("items/key.vvx"), loaded the first time it is drawn; a file that is not there is a mistake. One voxel of the model is one of the HUD's, seen from one side and 2 voxels deep, as a grid's slot shows it (frame:grid): not lit by the level, its see-through voxels left out.
---- frame: which of its frames, from 0.
---- rotation: which side faces you, degrees clockwise seen from above, quarter turns only: 0 its front (+y), 90 its left side, 180 its back, 270 its right side.
---What is drawn first on the HUD lies on top: draw the model before the box it sits on. With c.ui.depth above 2, what is drawn deeper stands in front of it.
---@param file string
---@param x number
---@param y number
---@param options? DrawOptions
function Frame:draw(file, x, y, options) end

---Writes text whose letters drop in one after another and bounce, ticks after it began (keep the tick it began in a state): GAME OVER, a stage's name. Before 0 nothing shows. color and size as frame:text's.
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
---@field song string|nil The song your game plays (music:play), by its file's name; nil while the player's own plays or after music:stop.
---@field beat integer|nil Beats since your game's song began, from 0 (its #tempo), on and on as it goes round; nil while the player's own song plays or after music:stop.
---@field position number|nil Beats since your game's song began, with the fraction: 6.5 is halfway through beat 6. On and on as it goes round, as music.beat; nil while the player's own song plays or after music:stop. A running light over 16 sixteenths: math.floor(music.position * 4) % 16.
---@field tempo number|nil The beats a minute of your game's song, 1 to 999: its #tempo until you set it. Set, the song plays faster or slower from the next tick, and music.beat, on_beat and "bar" go with it. Only its timing changes: its sounds keep their length, pitch and a singer's vowels stay as they are, sung consonants go with it. It holds for every song of yours, through a stop and from level to level, until you set nil (each song its own #tempo again): set it only to change it. Kept to a thousandth (124.4567 reads 124.457). nil while none of yours plays and you set none. A channel's own t in a song plays as much faster as the song.
---@field intensity number How tense the game is, from 0 (calm) to 1 (all of the song; 1 unless you set it): the layers of your game's song (#layer in the song: drums from 0.35, the melody from 0.8) come in above their tension and go below it, each over a bar. Set it in on_tick from your states: enemies on the screen, a boss, a hit.
---@field bar integer|nil Bars since your game's song began, from 0 (its #meter beats to a bar, 4 unless the song says); nil while the player's own song plays or after music:stop.
local Music = {}

---Plays a song of c.audio.music by its file's name ("storm" for music/storm.mml) as your game's own. Your game's song wins over the player's choice.
---- from: when it begins: "bar" (the next bar of the one playing, unless said), "beat" or "now"; at once when none of yours plays.
---- crossfade: { from = "now", crossfade = 3 } fades the old song out as the new comes in over 3 seconds, as loud together as either alone. c.audio.crossfade sets it for every change; else the old ends as the new begins.
---- at: the bar it begins at, counted from 1: { at = 33 } skips an intro of 32 bars; a bar past the song's end, or after its #loop (which it never leaves), is a mistake.
---- channels: notes your game writes, laid over a file of instruments (a .mml of c.audio.music with @instruments and #settings and no channel, which no player picks alone): { channels = { K = "@kick o4 l16 c r r r c r r r c r r r c r r r", H = "..." } }, a letter and its MML each, the whole set that plays. Every channel fills as many whole bars; no t, no singing voice. A mistake in one halts the game at your line, naming the channel.
---It starts at its beat 0 (its first bar, or the bar at says) and keeps time with the game (music.beat and on_beat count its beats from then, on and on as it goes round, #loop's bars too), held while the game pauses. Asked for while it plays, it goes on, through a level change too. Without a song: the player's own again, from the next bar.
---Asked again with other channels while its file of instruments plays, only they change, at from, the clock going on: a channel the same plays on untouched, a changed one's sounding note rings on until its next note, its new notes from then. from = "now" for a step edited (it sounds when the music comes to it), "bar" for the next pattern. Channels of other bars go round by the beats since the song began: music.bar % 4 is a four-bar pattern's bar. at and crossfade are for a song that begins, a mistake on such a change. Without channels the ones laid play on. The same channels again cost only a compare; ask when a step changes.
---@param song? string
---@param from? string|MusicPlay
function Music:play(song, from) end

---Silence in place of your game's song: a drum machine's STOP, a cutscene's quiet.
---- from: "now" (unless said), "beat" or "bar" of the song that plays.
---- fade: { from = "bar", fade = 2 } fades it out over 2 seconds; at once still without a click.
---The player's own song does not come back (music:play() without a song brings it). music.song, music.beat, music.bar and music.position are nil and on_beat is quiet until music:play begins a song, from its beat 0 (a song stopped and played again begins anew). A play asked before a stop comes replaces it: the song that plays goes on, another comes at its own from. nil in music.song alone does not tell a stop from the player's own song: your game knows which it asked.
---@param from? string|MusicStop
function Music:stop(from) end

---How a channel of your game's song sounds, as on a mixing desk: the channel is the song's letter ("K" for its K: line).
---- volume: its fader, from 0 (silent) to 1 (as written, the start).
---- mute: true silences it, the fader kept; false lets it sound again.
---- pan: from -1 (left) to 1 (right), 0 the middle: the other side turned down, never a side louder than written.
---It gets there smoothly over seconds (0.3 unless you say; 0 at once, still without a click), a mute too: music:mix("K", { mute = true }, 0). A key you leave out stays as it was; false for volume or pan gives it back (1, the middle), and music:mix("K", false) the whole strip. Without settings it gives what you asked for last. It holds by letter, whichever song plays, through a stop and from level to level: give it back (music:mix("D", false)) when you change to a song whose letters mean something else. A letter your song file does not have is a mistake; over a file of instruments, or before any song of yours plays, any letter is taken and waits for its channel. Only heard: replays and the game itself do not change with it.
---@param channel string
---@param settings? MusicMix|false
---@param seconds? number
---@return MusicMix|nil
function Music:mix(channel, settings, seconds) end

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

---Runs 60 times a second: your game's rules go here. input tells what the player presses. It runs after the ready-made rules, if the game uses some, after this tick's events (on_touch, on_hit …) and after on_beat. At the tick's end the things you removed go, the things you moved with thing:walk move, saves are written and a level change you asked for is carried out. math.random draws from the game's own random numbers, the same every time a run is played the same way (replays and saves keep them).
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

---Runs when two things that touched part: the end of a touch on_touch began, the player a again (not when one is removed or dies, nor when the level is left).
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

---Runs when a thing falls out of the level (c.physics.lost_below voxels under it, 48 unless set, or down on the world's floor below): once, until it is back in. Things other than the player are removed after it; the player waits for what your code decides (a life less, and back at the checkpoint: player:place, player.state.hp). Without it the player falls on.
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
---@field kit string The ready-made rules the game plays with: "shooter" or "platformer". Without one your script moves everything; each kit brings behaviors for kinds of things (c.things.NAME.behavior) and its own settings. Takes effect after a restart (F5).
---@field actions string[] A Lua game's actions besides the four directions (a kit brings its own), and the keys and gamepad buttons bound to them at first.
---@field keys table The keys bound to each action at first, by their US names (c.keys.jump = { "Z" }); the player may change them.
---@field pad table The gamepad's buttons bound to each action at first (c.pad.jump = { "A" }).
---@field scored boolean The game keeps a score (player.state.score), with ready-made rules or without: the title shows the best, and runs go into the high score table.
---@field start string The level a run begins in (c.start = "hut_outside"), a file in levels/ by the name world:go_to takes. Unset: the first of world.levels, in the order of the file names. A scenario's level, a save and a replay begin elsewhere as they say. Takes effect after a restart (F5).
---@field items table<string, table> What can be carried (c.items.key = { model = "key.vvx", stack = 1 }): an inventory (player.state.inventory) counts these by name.
---@field state Conf.state What a run starts with (c.state.world, c.state.player): the states' first keys, the engine's own among them (hp, weapon, inventory) and the game's (lives).
---@field icons table<string, table> Small pictures for the HUD (frame:icon, screen:burst): c.icons.heart = { rows = { ".RR.RR.",... }, colors = { R = { 214, 36, 44 } } }.
---@field saves Conf.saves Save games (c.saves = { slots = 3, version = 1 }): none without slots.
---@field high_scores any[][] The high score table to beat before anyone played: up to ten lines of initials and points ({ { "VOX", 50000 },... }); without them the engine's.
---@field languages string[] The languages the game speaks ({ "de", "en", "fr" }, two letters each): SETTINGS offers them, world.language says which the player chose, the engine's menus speak it. None: English.
---@field players integer The seats the game has, how many players can play at once (c.players = 4): 1, a game of one, unless set. Each seat is a player (world.players[n]); players take a seat with START on a pad or Return at the keyboard, or join over the network. (from 1 to 16)
---@field team_score boolean The high score table takes the run as one line, every seat's score added, instead of a line for each player.
---@field pointer string A pointer the mouse moves over the game (c.pointer = "system": the system's arrow; "own": none shown, the game draws its own in on_draw from frame.pointer): input.pointer says what it is over, the mouse's buttons and wheel are keys (c.keys.click = { "mouse_left" }), the engine's menus take it. Unset: no pointer. Takes effect after a restart (F5).
---@field weapons Conf.weapons The shooter kit's weapons (c.kit = "shooter"): aim_assist, and each weapon by its name, c.weapons.NAME = { ... }.

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
---@field speed number Running speed, in voxels a second. (from 0 to 100000)
---@field accel number How quickly it gets to running speed while it stands on something, in voxels per second per second. In the air air_accel applies instead. (from 0 to 1000000)
---@field air_accel number How quickly it gets to running speed while it is in the air, in voxels per second per second (accel on the ground). (from 0 to 1000000)
---@field jump number Height of a full jump (holding the button); letting go early cuts it. (from 0 to 10000)
---@field kind string|string[] A game without ready-made rules: the kind of thing (c.things) the player's figure is (player), placed where each level starts and going along from level to level; with several seats one for every seat or a list by seat ({ "hero", "hero_blue" }). Without it the player has none.
---@field muzzle? number[] Where shots leave the gun, { ahead, up } from the middle of the feet in voxels. Unset: just in front of the body at half its height. (c.kit = "shooter")
---@field invulnerable? integer Ticks after a hit during which nothing hurts the player (0: none). (c.kit = "shooter") (0 unless set)
---@field attacks? table<string, string[]> What each of the game's actions attacks with, by the action's name: its weapons in order, the first that can. A weapon is a name of c.weapons, or "held", the gun in hand (player.state.weapon). An action named here is the ready-made rules'; one not named is your code's. (c.kit = "shooter") ({} unless set)
---@field facing? number[] The way the player faces as a run starts, { x, y } (x east, y north). Unset: south, { 0, -1 }. (c.kit = "shooter")
---@field moves? string[] The moves the player has, by the action that does each: "jump", "strafe". None unless named. (c.kit = "shooter") ({} unless set)
---@field frames? table The player's model's frames for what it does (idle, walk, jump), as a kind's (c.things.NAME.frames). (c.kit = "shooter") ({} unless set)
---@field fps? number How many frames a second the player's sets go where a set does not say its own, as a kind's fps. Unset: 10. (c.kit = "shooter")
---@field swing? table<string, number> For a model of parts: the parts that swing as it walks, with how many degrees, as a kind's (c.things.NAME.swing). (c.kit = "shooter") ({} unless set)
---@field blink? integer While nothing hurts the player it blinks: shown this many ticks, then hidden as many (0: it does not blink). (c.kit = "shooter") (4 unless set)
---@field stride? number A puff of dust rises every this many voxels the player walks. Unset: half its height. (c.kit = "shooter")
---@field step_dust? integer How many specks of dust each puff of its steps has (0: no dust). (c.kit = "shooter") (3 unless set)
---@field landing_height? number Landing from higher than this, in voxels, raises a puff of dust. (c.kit = "shooter") (2 unless set)
---@field landing_dust? integer How many specks of dust a landing puff has. (c.kit = "shooter") (10 unless set)

---@class Conf.title_screen.props
---@field model string Its model (.vvx).
---@field at number[] Where it lies on the ground, x and y in voxels.
---@field rotation integer Degrees clockwise seen from above, 0, 90, 180 or 270.

---@class Conf.title_screen
---@field music string What plays on the title (a song of c.audio.music; none: the first).
---@field music_at integer The bar the title's song begins at, counted from 1 (its start): 33 for a song with 32 bars of intro. It goes round as the song says (#loop). The engine's title only: a title level of the game's own picks with music:play's at. (from 1 to 9999)
---@field at number[] The ground point the camera looks at (default: where the player starts).
---@field view_width number Voxels across the screen there (default: c.camera.view_width). (above 0, up to 65536)
---@field logo string[] The logo, a line each (default: the game's title); logo_colors colors them.
---@field logo_colors Color[] Each logo line's color, a Color: "#rrggbb" or { r, g, b } from 0 to 255.
---@field logo_heights integer[] How high each logo line's capitals are, in UI voxels. (each from 1 to 1000)
---@field logo_drip integer Which logo line drips (blood). (from 0 to 99)
---@field logo_cracks number[] Cracks per letter in each logo line, on average. (each from 0 to 100)
---@field logo_model string A model of the game's (.vvx) as the logo, in place of its lines of letters: a UI voxel each of its voxels, its front (+y) to the player and its z up, centred under the top of the screen. A figure of parts (its objects named, a letter each) drops in part by part in the file's order, else whole. Lit as the big letters are (the sides a little darker, what lies further back too): its colours are its materials'.
---@field logo_bob number How far the logo's letters (or its model's parts) bob up and down once they have landed, in UI voxels: a slow, smooth wave running through them, coming in over a second; 0 (unless set) they stand still. (from 0 to 100)
---@field props Conf.title_screen.props[] Models lying in the scene.
---@field flyby string A model that flies over the scene now and then (a helicopter), every flyby_every seconds.
---@field flyby_every number Seconds between two flybys (0: none). (from 0 to 3600)
---@field flyby_altitude number How high the flyby crosses over the ground. (from 0 to 65536)
---@field attract_after number Seconds on the title without a key pressed before it plays the game's replays (replays/), as the arcade machines did; 0: never. (from 0 to 3600)
---@field light_menu boolean The scene behind the menu is dark: its rows and hint in light letters (default: dark ones, which read over bright ground).
---@field level string A level of the game as its own title (c.title_screen.level = "title"), a file in levels/, in place of the engine's title. The window opens in it; its code draws the menu and begins a run with world:new_game() or world:load(slot). It is played before a run and never counts: not recorded, no saving there. Never the level a run begins in. Takes effect after a restart (F5).

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
---@field stop_at_edges boolean The view stops at the edges of the stage instead of showing the dark beyond: a side view's level, or the ground a view from above shows (a farm seen from above).
---@field hold number[] How far apart the players in play may be, across and deep, in voxels ({ 240, 150 }; { 240 } across only, a side view): a move that would take the two furthest apart further is cut short, as the edge of an arcade machine's screen held them. The game's rules hold them apart, whatever the window's size. (each above 0, up to 65536)
---@field rotates string How the game sets things down: on the grid in quarter turns (the default), or rotated freely (rotates = "free": a level's pieces at any bearing, rotation = 30). The game's choice for every level, and what the level editor offers; a level turned off the quarter turns in a game on the grid does not load.
---@field aspect number The picture's shape, its width over its height (2.39 a film's): the scene is drawn into the largest rectangle of that shape the window holds, in its middle, black bars on the other two sides; the HUD and the menus lie inside it too. Unset: the window's shape. (from 0.25 to 4)

---@class Conf.light
---@field sun number[] Direction the sunlight travels (towards the ground). (each from -1000 to 1000; [3] from -1000 to below 0)
---@field sun_color Color The sunlight's color, a Color: "#rrggbb" or { r, g, b } from 0 to 255 (a darker one shines weaker); white unless set (warm at dusk, a pale blue for the moon). frame:daylight sets it for one picture. (each from 0 to 255)
---@field ambient number Light that reaches every face, 0..1. (from 0 to 1)
---@field shadow number How much a shadow darkens sunlit faces, 0..1. (from 0 to 1)
---@field sky Color The background's color, a Color: "#rrggbb" or { r, g, b } from 0 to 255. (each from 0 to 255)
---@field reflection_horizon Color The sky metal mirrors: its color at the horizon, a Color: "#rrggbb" or { r, g, b } from 0 to 255 (below the horizon it mirrors the ground, in the haze's color). (each from 0 to 255)
---@field reflection Color The sky metal mirrors overhead: its color, a Color: "#rrggbb" or { r, g, b } from 0 to 255. (each from 0 to 255)
---@field metal_sweep number A streak of light sweeping over metal now and then, this bright (0: none): metal reads as metal even where the sun cannot glint in it. (from 0 to 10)
---@field shadow_style string How shadow edges look: "smooth" (soft edges across the voxels), "voxel" (every voxel face is either lit or in shadow, edges step along the grid) or "voxel_soft" (as voxel, with half-shadowed faces in between).
---@field model_shading string How models (actors, props) are lit: "faces" (every face of a voxel by its own direction), "smooth" (a whole voxel by the direction of the surface around it, so round shapes shade softly like pixel art) or "edges" (smooth, but where two flat surfaces meet the faces keep their own light, so hard edges stay hard).
---@field model_shading_for table<string, string> Models shaded otherwise, by file (relative to the game folder), e.g. { ["tank.vvx"] = "faces" } for a machine among smooth figures.
---@field edge_tolerance number With "edges": how far, in voxels, a voxel's surroundings may stray from one smooth surface before it counts as an edge. (from 0 to 16)
---@field lamp_shadows string What stops a lamp that states no shadows of its own (a level's lights, frame:light): "static" (the level's voxels and its pieces), "all" (moving things too: figures, things, models drawn; costly, for a few lamps) or "none": it shines through walls, as before 0.7 (older hardware). A light's own shadows beats it, whatever it says.
---@field fog_density number Fog in the level's air, how thick: 0 none, else a voxel of air takes this share of the light passing through (0.01: a third is left after 100 voxels). The lamps' light shows in it, a spot as a beam, and what lies further fades to the fog's color (0.004 is a light haze in a big hall). By day give fog_color the sky's color, or what lies further fades to a dark veil. A game's frame:fog sets it for one picture. (from 0 to 1)
---@field fog_color Color The fog's own color where no lamp lights it, a Color: "#rrggbb" or { r, g, b } from 0 to 255. frame:fog sets it for one picture. (each from 0 to 255)
---@field fog_anisotropy number How much the fog sends a lamp's light on ahead rather than back, -0.9 to 0.9: above 0 a beam is brighter looking towards its lamp (0: every way alike). (from -0.9 to 0.9)
---@field fog_glow number How brightly the fog shows the lamps' light, 0..20; the dimming is the density's alone: 1 as the air would (thicker fog, brighter beams), 4 a stage's haze (bright beams while the room stays clear), 0 a fog that only dims. Higher is never fainter, and however high, a lamp's glow in the fog never gets brighter than its own light (times the light's fog). (from 0 to 20)

---@class Conf.debris
---@field settle number Chance that a landed debris voxel becomes part of the world again. (from 0 to 1)
---@field max_particles integer Upper bound for flying debris and sparks. (from 0 to 1000000)
---@field gore number Share of a body's voxels that burst blood-red when it dies, 0..1 (0 keeps their own colours, say for robots). (from 0 to 1)

---@class Conf.effects
---@field glow number How strongly bright things (muzzle flashes, fire, tracers, sparks) glow into their surroundings; 0 turns it off. (from 0 to 10)
---@field lights number How strongly flashes and explosions light up what is around them. (from 0 to 10)
---@field haze number Haze towards the far edge of the view, 0 (none) to 1 (thick); haze_color is its color. (from 0 to 1)
---@field haze_color Color The haze's color, a Color: "#rrggbb" or { r, g, b } from 0 to 255. (each from 0 to 255)
---@field tilt_shift number Blur at the top and bottom of the screen, a miniature look, 0..1. (from 0 to 1)
---@field screen_flash number How bright the white flash over the screen on big explosions gets, 0..1. (from 0 to 1)
---@field hitstop integer Ticks the game holds still when an enemy dies. (from 0 to 60)
---@field outline number A dark line round the models (figures, props), as round a sprite: how many voxels thick (0: none; 1 a voxel, as a sprite's pixel), and its colour (0..1). Hidden behind what stands in front, drawn where one part of a model stands before another too. (from 0 to 8)
---@field outline_color Color The outline's color, a Color: "#rrggbb" or { r, g, b } from 0 to 255. (each from 0 to 255)
---@field bake_models boolean Models drawn from lists of their faces that can be seen, made once, instead of a cube a voxel: the same picture, quicker and in less of the GPU's memory. false draws every voxel a cube.

---@class Conf.transition
---@field fade number Seconds out and seconds in; 0: a cut. (from 0 to 10)
---@field color Color The color faded through, a Color: "#rrggbb" or { r, g, b } from 0 to 255.
---@field go_to number A length of its own for a reason the level is left (a puzzle game's quick restart: restart = 0). (from 0 to 10)
---@field restart number Seconds of the fade when the level is entered afresh. (from 0 to 10)
---@field won number Seconds of the fade when the run is won. (from 0 to 10)
---@field lost number Seconds of the fade when the run is lost. (from 0 to 10)

---@class Conf.audio
---@field volume number Loudness of all sounds, 0..1. (from 0 to 1)
---@field music string|string[] Music as MML text, relative to the game folder: one file, or a list the keys 1..9 switch between while playing.
---@field music_volume number Loudness of the music, 0..1, times the player's music volume. (from 0 to 1)
---@field drive number How hard the game's sound is pushed into the limiter, in decibels: 0 as it is, more makes it denser and louder; music you listen to for long tires sooner when dense. 0 to 12. The limiter's drive (the JUST mastering stage's own input gain), on sounds and music together, after the player's volumes; the game's choice, not the player's. (from 0 to 12)
---@field ceiling number The loudest the game's sound may get, in decibels of true peak (between the samples too), -6 to 0; -1 unless set. The limiter holds everything under it, sounds and music together, after the drive; 0 is full scale. Hard-edged waves (squares, noise) can still peak a few tenths of a decibel over it between samples, so -1 leaves room. (from -6 to 0)
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

---@class Conf.weapons
---@field aim_assist? boolean Whether shots find a target near their line, the player's and the enemies' alike. true: a shot goes for the nearest target in a narrow cone round the way it was fired, slightly off the 8 ways. false: it keeps to the line (rising or dipping only for a target right on it) and nothing scatters. (c.kit = "shooter") (true unless set)
---@field [string] Weapon

---@class Weapon A weapon of the shooter kit (c.weapons.NAME = { ... }, c.kit = "shooter"): what one attack does.
---@field like? string Another weapon this one builds on: its keys first, then this one's (like = "rifle"). (c.kit = "shooter")
---@field label? string What the HUD and the status line call it. (c.kit = "shooter") ("" unless set)
---@field cooldown? integer Ticks between shots (a gunship's gun: between the rounds of a burst). (c.kit = "shooter") (0 unless set)
---@field jitter? integer A random number of ticks below this is added to the cooldown each time (0: none). (c.kit = "shooter") (0 unless set)
---@field burst? integer How many rounds one burst has (1: a single shot). (c.kit = "shooter") (1 unless set)
---@field pause? integer Ticks after a burst before the next one (a gunship's gun). (c.kit = "shooter") (0 unless set)
---@field speed? number How fast a round flies, in voxels a second. (c.kit = "shooter") (0 unless set)
---@field life? number How long a round flies before it is gone, in seconds. (c.kit = "shooter") (1 unless set)
---@field damage? integer The hits a round takes off what it hits. (c.kit = "shooter") (1 unless set)
---@field blast? number The crater where a round hits the world, in voxels (0: sparks); a rocket's or a grenade's blast. (c.kit = "shooter") (0 unless set)
---@field spread? number How far rounds stray from their line at random, with aim assist (0: not at all). With c.weapons.aim_assist off nothing scatters. (c.kit = "shooter") (0 unless set)
---@field rocket? boolean true: the round blows up where it hits, hurting everyone near. (c.kit = "shooter") (false unless set)
---@field throw? number A full throw, in voxels, for thrown weapons (grenades); 0 for guns. (c.kit = "shooter") (0 unless set)
---@field ammo? integer How many of its rounds an inventory holds: a player's weapon spends the item of its own name (0: it never runs out). (c.kit = "shooter") (0 unless set)
---@field sound? string The sound a shot makes: a name of the ready-made rules' sounds or of a file of the game in sounds/. (c.kit = "shooter") ("shot" unless set)
---@field reach? number Above 0 it is a blow, not a shot: it reaches this far, in voxels along the ground, to one in front. (c.kit = "shooter") (0 unless set)
---@field cone? number A blow reaches those within this many degrees of the way the attacker faces, at its height. (c.kit = "shooter") (90 unless set)
---@field auto? boolean true: it attacks again while its action is held. false: once a press. (c.kit = "shooter") (true unless set)
---@field noise? number How far it is heard, in voxels (unset: as far as a listener hears shots; 0: silent). (c.kit = "shooter")
---@field friendly_fire? boolean true: its blast hurts its own team too. false: it spares them. (c.kit = "shooter") (true unless set)
---@field falloff? boolean true: its blast does less towards the edge. false: its whole damage everywhere in it. (c.kit = "shooter") (false unless set)

---@class Kind A kind of thing (c.things.NAME = { ... }): what every kind may set; a behavior of a kit (c.kit) reads more.
---@field like? string Another kind this one builds on: its keys first, then this one's (like = "rifleman").
---@field behavior? string Its ready-made behavior, with a kit (c.kit): the platformer kit's hero, walker, hopper, flyer, pickup, platform, prop, crate; the shooter kit's soldier, spawner, captive, gunship, pickup. Without one your script moves it.
A behavior's own keys are listed below, each with its default. A length or speed left unset is that default scaled by the kind's height over the height of the figure the default was made for (half as tall: half as fast and high); counts, times and weapons' numbers are not scaled. voxcade kinds GAME prints every number in effect.
---@field model? string Its model, a .vvx of the game.
---@field hp? integer The hits it takes: thing.state.hp at the start, on_death at 0. Without it, it takes none.
---@field fixed? boolean It stays exactly where it is put, in the air too (a kind without a behavior).
---@field solid? boolean It stops bodies as a wall does, in the cells its model is drawn in, where it stands and as it faces in quarter turns: level:solid says so, thing:walk, the ready-made rules, rounds and grenades stop there.
Only for a kind that stands: fixed = true, no behavior, a model. thing:place moves its cells and thing:remove frees them at the tick's end. A body it is set onto is pushed aside.
---@field persistent? boolean Its things a level's file sets down stay as left for the whole run, in any level: removed ones stay removed, the others where they stood, thing.state (a chest, a lever; not the bats). By their names; not the hero that becomes the player.
---@field width? number Its body across and deep, in voxels (c.physics.body_width unless set).
---@field height? number Its body's height, in voxels (its model's unless set).
---@field facing? number[] The way it faces as it comes in, { x, y } (x east, y north).
---@field props? table What your code reads of it, thing.props: props = { worth = 100 }. A level's thing or level:spawn may give one its own of these.
---@field names? table Which of its props name something, checked as the game loads and at level:spawn: names = { to = "level", at = "place", works = "things" }.
- "level": a level of the game.
- "place": a place in the level its "level" prop names, else in its own ("start" always).
- "thing": a named thing of its level; "things": a list of them.
"" (or {}) names nothing. The level editor offers the names there are.
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
---@field speed? number Its running speed, in voxels a second. (150 unless set)
---@field accel? number How quickly it gets up to running speed while it stands on something, in voxels a second per second. In the air air_accel applies. (1200 unless set)
---@field air_accel? number How quickly it gets up to running speed while it is in the air, in voxels a second per second. On the ground accel applies. (800 unless set)
---@field jump? number How high a full jump goes, in voxels: the jump button held until the top. (29 unless set)
---@field jump_cut? number How much of its upward speed it keeps when the jump button is let go while it still rises: 0.5 keeps half, so a short press makes a low jump. (0.5 unless set)
---@field coyote? integer Ticks after it ran off a ledge in which a jump still works. (6 unless set)
---@field buffer? integer Ticks a jump pressed just before landing waits for the ground and then happens. (6 unless set)
---@field climb? number Its speed on a ladder, up and down, in voxels a second. Without left or right it moves to the ladder's middle; it stands on a ladder's top, and down climbs down from there. (60 unless set)
---@field invulnerable? integer Ticks after a hit during which nothing hurts it. (60 unless set)
---@field blink? integer While nothing hurts it, it blinks: it is shown this many ticks, then hidden as many ticks. 0: it does not blink. (4 unless set)
---@field knockback? number[] How a hit throws it, { back, up } in voxels a second: back away from what hit it, and up. ({ 100, 100 } unless set)
---@field fall_gravity? number Gravity is multiplied by this while it falls, so a fall is snappy, not floaty. 1: as strong as on the way up. (1.5 unless set)
---@field apex_gravity? number Gravity is multiplied by this near the top of a jump (its apex) while the jump button is held: below 1 it floats there a moment. "Near the top" is apex_speed. (0.5 unless set)
---@field apex_speed? number In voxels a second: while it rises or falls more slowly than this it is at the top of a jump, for apex_gravity. It catches a ledge (grab) only while it rises more slowly than this too. (30 unless set)
---@field corner? integer A head that hits the edge of a ceiling by up to this many voxels slips past it, so a jump is not stopped by a corner. 0: no such help. (4 unless set)
---@field look_ahead? number How far the camera leads ahead of the hero in the way it runs, in voxels. 0: the camera stays centered on it. (40 unless set)
---@field look_ahead_speed? number How fast the camera swings over to lead the other way when the hero turns round, in voxels a second. (90 unless set)
---@field grab? number How far above its head its hands reach for the edge of a ledge, in voxels. 0: it catches no ledges. (0 unless set)
---@field grab_ahead? number How far ahead of its body it finds a ledge's edge to catch, in voxels. Only when grab is above 0. (3 unless set)
---@field hang_below? number How far under a ledge's edge its head hangs while it holds on, in voxels. A ledge lower than that above its head it pulls itself onto at once. Only when grab is above 0. (2 unless set)
---@field pull? integer How many ticks it takes to pull itself up onto the ledge it hangs from. Up or jump pulls up; down lets go. Only when grab is above 0. (24 unless set)
---@field face_ladder? boolean While it climbs a ladder it turns to the ladder, its back to the camera. false: it keeps the way it faced. (true unless set)
---@field air_jumps? integer How many more jumps it may make in the air before it lands: 0 none, 1 a double jump. (0 unless set)
---@field air_jump_height? number How high each jump in the air goes, in voxels, from where the hero is when it makes it. Only when air_jumps is above 0. (20 unless set)

---@class Kind.platformer.walker : Kind behavior = "walker" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field speed? number Its walking speed, in voxels a second. (60 unless set)
---@field ledges? boolean true: it turns round at a ledge as at a wall. false: it walks off the ledge. (true unless set)

---@class Kind.platformer.hopper : Kind behavior = "hopper" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field every? integer Ticks between one hop and the next. It hops left and right in turn. (120 unless set)
---@field hop? number[] How a hop goes, { across, up } in voxels a second. ({ 100, 200 } unless set)

---@class Kind.platformer.flyer : Kind behavior = "flyer" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field range? number How far it hovers up and down from where it is placed, in voxels. (50 unless set)
---@field period? integer Ticks for one trip up and down. (120 unless set)
---@field face_hero? boolean true: it turns to face the hero. false: it keeps looking the way it was placed. (true unless set)

---@class Kind.platformer.platform : Kind behavior = "platform" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field travel? number[] How far it goes from where it is placed and then back, { across, up } in voxels. ({ 64, 0 } unless set)
---@field speed? number Its speed, in voxels a second. (30 unless set)
---@field pause? integer Ticks it waits at each end of its way. (30 unless set)
---@field crushes? boolean true: coming down onto the hero it hurts it and goes on (a trap). false: it waits above it. (false unless set)

---@class Kind.platformer.prop : Kind behavior = "prop" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field stands? boolean true: it stands on the floor below where it is placed (a door, spikes). false: it stays where it is placed, in the air too (a torch on a wall). (false unless set)

---@class Kind.platformer.crate : Kind behavior = "crate" of the ready-made rules c.kit = "platformer": its keys besides every kind's, unset its defaults.
---@field push? number The fastest the hero pushes it, in voxels a second. (30 unless set)

---@class Kind.shooter.soldier : Kind behavior = "soldier" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field weapon? string The weapon he fires or throws: a name of c.weapons. ("enemy_rifle" unless set)
---@field speed? number His running speed once he knows of the player, in voxels a second. (24 unless set)
---@field idle_speed? number His walking speed while he does not know of the player, in voxels a second. (10 unless set)
---@field accel? number How quickly he gets up to speed, running or walking, in voxels a second per second. (300 unless set)
---@field sight? number How far he sees the player, in voxels. A comrade falling he notices as far. (60 unless set)
---@field hearing? number How far he hears gunfire, in voxels; rounds hitting near him count as half that. (70 unless set)
---@field blast_hearing? number How far he hears an explosion, in voxels. (130 unless set)
---@field wake? number While the player is further from him than this, in voxels, he waits where he is. (150 unless set)
---@field range? number How far he shoots, in voxels. (95 unless set)
---@field distance? number The distance he keeps from the player, in voxels. (32 unless set)
---@field step_dust? integer How many specks of dust each puff of his steps has (see stride). 0: no dust. (3 unless set)
---@field landing_height? number Landing from higher than this, in voxels, raises a puff of dust. (2 unless set)
---@field landing_dust? integer How many specks of dust a landing puff has (see landing_height). (10 unless set)
---@field muzzle? number[] Where his shots leave the gun, { ahead, up } from his feet in voxels. Unset: just in front of him at half his height.
---@field stride? number A puff of dust rises every this many voxels he walks. Unset: half his height.

---@class Kind.shooter.spawner : Kind behavior = "spawner" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field spawns? string The kind of thing it sends out: a name of c.things, a kind with the behavior "soldier". ("rifleman" unless set)
---@field count? integer How many it sends out in all. (2 unless set)
---@field first? integer Ticks after the player comes within wake before the first one comes out. (60 unless set)
---@field every? integer Ticks between one coming out and the next. (420 unless set)
---@field wake? number It sends them out only while the player is this near, in voxels. (120 unless set)
---@field ahead? number[] A way { x, y } (x east, y north): it sends them out only while it lies this way of the player, by more than ahead_by. Unset: on any side. ({ 0, 1 } unless set)
---@field not_near? number Nobody comes out while the player is nearer than this, in voxels. (0 unless set)
---@field ahead_by? number How far it must lie in the way of ahead, in voxels. Unset: the player's height.

---@class Kind.shooter.captive : Kind behavior = "captive" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field speed? number Its running speed once it is freed, in voxels a second. (30 unless set)
---@field accel? number How quickly it gets up to that speed, in voxels a second per second. (399 unless set)
---@field flee? string|number[] Where it runs once it is freed. "away": away from the player. A way { x, y }, x east and y north (so { 0, -1 } runs south). Or a place's name: it runs there (in a level without that place: away). ("away" unless set)

---@class Kind.shooter.gunship : Kind behavior = "gunship" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field weapon? string Its gun: a name of c.weapons. ("door_gun" unless set)
---@field rockets? string Its rockets: a name of c.weapons. ("rocket_pods" unless set)
---@field speed? number Its flying speed, in voxels a second. (35 unless set)
---@field altitude? number How high it flies over the pad it took off from, in voxels. (17 unless set)
---@field clearance? number It flies at least this high over what stands below it, in voxels. (4 unless set)
---@field offset? number[] Where it holds from the player, { x, y } in voxels (x east, y north). ({ 0, 65 } unless set)
---@field wake? number It lifts off when the player comes this close, in voxels. (130 unless set)
---@field size? number[] Its hit box in voxels: { across, along, high }. ({ 12, 24, 8 } unless set)
---@field muzzle? number[] Where its guns' muzzles are in its model, { across, along, up } in voxels; the gun on the other side is mirrored across. ({ 0, 12, 1.5 } unless set)
---@field pods? number[] Where the fronts of its rocket pods are in its model, { across, along, up } in voxels; the pod on the other side is mirrored across. ({ 5, 2.5, 2.5 } unless set)
---@field side_on? boolean true: it turns its side to the player and the gun on that side fires. false: it faces the player and its guns take turns. (false unless set)
---@field crash_blast? number The crater where it crashes, in voxels, as a weapon's blast. (9 unless set)
---@field crash_damage? integer The hits it does to everything near where it crashes. (6 unless set)

---@class Kind.shooter.pickup : Kind behavior = "pickup" of the ready-made rules c.kit = "shooter": its keys besides every kind's, unset its defaults.
---@field frame? integer Which frame of its model it shows, from 0. (0 unless set)
---@field bob_ticks? integer How many ticks it takes to bob up, and as many to bob down again. (20 unless set)
---@field bob? integer How many voxels it bobs up and down. 0: it lies still. Unset: a fifth of its model's height.

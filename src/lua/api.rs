//! The Lua API in one place: what a game's scripts see (the game model's
//! objects `world`, `level`, `player`, things, `input`, `screen`;
//! `docs/game-model.md`) and the functions a game defines that the engine
//! calls.
//! From this register come the definitions for editors (`voxcade api --lua
//! lua/voxcade.lua`, LuaLS: completion and checks in VS Code, Zed, Neovim),
//! the text `voxcade api` and the MCP tool `api` show, and a test keeps it
//! in step with what `world.rs` really offers.

use crate::config::GameConfig;

/// One name of the API.
pub struct Item {
    /// Whose it is: one of `OWNERS`, or "" for a function the game defines
    /// or the engine gives.
    pub of: &'static str,
    pub name: &'static str,
    pub what: What,
    pub doc: &'static str,
}

pub enum What {
    /// A value (its LuaLS type), and whether the script may set it.
    Field { ty: &'static str, set: bool },
    /// A method (`level:spawn(...)`): its parameters (name, type; a `?`
    /// after the name: may be left out) and what it gives back.
    Method { params: &'static [(&'static str, &'static str)], returns: &'static str },
    /// A function the game defines and the engine calls.
    Callback { params: &'static [(&'static str, &'static str)], returns: &'static str },
    /// A function the engine gives every script.
    Function { params: &'static [(&'static str, &'static str)], returns: &'static str },
}

use What::{Callback, Field, Function, Method};

/// The types the API speaks of besides its objects.
const TYPES: &str = "\
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

---@class PictureCamera What a picture's camera sets (frame:camera); what it leaves out is c.camera's.
---@field look_at? Point|Thing|number[] The point the view centres on, { x, y, z } in voxels (z left out: the camera's height), or a thing; left out, the view follows what the camera follows.
---@field width? number How many voxels the picture shows across where it looks.
---@field height? number Or how many it shows from bottom to top there (not both).
---@field pitch? number Degrees below the horizon, -89 (looking up) to 90 (straight down).
---@field fov? number The field of view up and down, 10 to 100 degrees.

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
";

pub const API: &[Item] = &[
    // The functions a game defines.
    Item { of: "", name: "require", what: Function { params: &[("name", "string")], returns: "any" }, doc: "A Lua file of your game folder, run once as the game starts: require(\"market\") runs market.lua, require(\"lang.de\") lang/de.lua (dots between folders), and gives back what the file returns (true if nothing); asked again, the same value. Only while the game starts: at the top of main.lua and of the files it requires. The files share main.lua's globals (a handler may live in any of them) and are your game's code as main.lua is: saving one runs the game's code anew, replays count it." },
    Item { of: "", name: "conf", what: Callback { params: &[("c", "Conf")], returns: "" }, doc: "conf.lua: sets the game's settings over the engine's defaults (c.title, c.physics.gravity, c.things.NAME = { ... }). Saved while the game runs, a setting takes effect at once; the ready-made rules (c.kit), the actions and keys, the level, the markers, the player's start and kind after a restart (F5)." },
    Item { of: "", name: "on_world_start", what: Callback { params: &[], returns: "" }, doc: "main.lua: runs once when a run begins, before its first level is there: give the run's states their first values (world.state, player.state). Globals set until the first level has begun stay as they are from then on: what changes goes in a state." },
    Item { of: "", name: "on_level_start", what: Callback { params: &[("how", "string")], returns: "" }, doc: "Runs when a level is entered, before its first tick: build a level made in code, place things, give level.state its first values. how is \"start\" when the run begins, \"go_to\" after world:go_to (the same level too), \"restart\" after world:restart, \"load\" after world:load (level.state as it was saved), \"reload\" when its file was saved while playing. The player already stands at level.arrived: do not spawn it here when c.player.kind gives it a figure. An ask here (world:go_to …) counts at once, before the level's first tick." },
    Item { of: "", name: "on_level_end", what: Callback { params: &[("why", "string")], returns: "" }, doc: "Runs when the level is about to be left: why is \"go_to\", \"restart\", \"load\", \"won\", \"lost\", \"quit\" (QUIT TO TITLE) or \"reload\". Copy from level.state what should outlive the visit (into world.state, player.state or level.kept). Nothing more can be asked here." },
    Item { of: "", name: "on_world_end", what: Callback { params: &[("result", "string")], returns: "" }, doc: "Runs when the run ends, after on_level_end: result is \"won\", \"lost\" or \"quit\" (QUIT TO TITLE). The end cannot be undone here; a last chance to change the states for the record." },
    Item { of: "", name: "on_player_join", what: Callback { params: &[("p", "Player"), ("how", "string")], returns: "" }, doc: "Someone takes a seat during the run (a pad's START, the keyboard's second half, a machine over the network), in its tick, first among the events; not the seats taken as the run begins. how is \"new\" (p.state a copy of c.state.player) or \"back\" (the seat's state from before it left, or from a save). Its figure comes in at the tick's end: here it may be placed (p:place), given a kind (p.state.kind) or kept waiting (p.state.out = true); else it comes in where the first player in play stands." },
    Item { of: "", name: "on_player_leave", what: Callback { params: &[("p", "Player")], returns: "" }, doc: "A player left its seat (LEAVE in the pause menu, its machine gone): in its tick, first among the events. It is out of play at once, its figure goes at the tick's end; p.state is kept for its seat, and whoever takes it again comes back with it (on_player_join's \"back\")." },
    Item { of: "", name: "on_save_loaded", what: Callback { params: &[("version", "integer")], returns: "" }, doc: "Runs whenever a save is loaded (world:load, LOAD in the pause menu), before its level is entered, with the version of the game that made it (its c.saves.version then). Count c.saves.version up when what a save holds changes (a key renamed), and bring older saves up to date here: if version < 2 then player.state.coins = player.state.gold end. There is no level here yet." },
    Item { of: "", name: "on_tick", what: Callback { params: &[], returns: "" }, doc: "Runs 60 times a second: the game's own rules (after the ready-made ones, if the game uses some, and after the events of this tick: on_touch, on_hit …). input holds the keys. math.random draws from the game's own random numbers, the same every time a run is played the same way (replays and saves keep them)." },
    Item { of: "", name: "on_draw", what: Callback { params: &[("frame", "Frame")], returns: "" }, doc: "Runs for every picture (as often as the screen shows one, apart from the 60 ticks a second): what the game draws besides the level and the things that draw themselves, on frame, this picture alone (frame:text, frame:cube, frame:model, frame:light). Everything else is only to look at here." },
    Item { of: "", name: "on_touch", what: Callback { params: &[("a", "Thing"), ("b", "Thing"), ("where", "string")], returns: "" }, doc: "Two things begin to touch: once, until they part (on_touch_end), not every tick. The player is always a, else the thing that came first. where is the face of b that a met: \"top\" (a came down onto b: landed on it, however fast), \"bottom\" (a came up under b, or b came down onto a), \"side\" (anything else, and always for a touch within a reach). What happens is your code's: a stomp, a pickup, a door. For what lasts while touched (spikes, an enemy's contact) ask thing:touching in on_tick. Things already touching where a level begins (or where thing:place puts one) count as touching: a player arriving on a door does not go through it again. A kind with a reach (c.things.NAME.reach: a pickup, a helipad, a hostage) is touched within it, whatever touches it (two with a reach: within the larger)." },
    Item { of: "", name: "on_touch_end", what: Callback { params: &[("a", "Thing"), ("b", "Thing")], returns: "" }, doc: "Two things that touched part: the end of a touch on_touch began, the player a again (not when one is taken away or dies, nor when the level is left)." },
    Item { of: "", name: "on_interact", what: Callback { params: &[("thing", "Thing"), ("by", "Thing")], returns: "" }, doc: "The player pressed the action called interact (c.actions) while touching this thing (the nearest, if several): talk, open, read. That press is used up: input:pressed(\"interact\") is false in this tick's on_tick." },
    Item { of: "", name: "on_hit", what: Callback { params: &[("thing", "Thing"), ("by", "Thing|nil"), ("damage", "integer"), ("weapon", "string|nil")], returns: "" }, doc: "A thing was hurt (its hit points already lowered): by the ready-made rules, or by thing:hurt (told in the next tick). by is who did it and weapon with what (a weapon of c.weapons, knife, what thing:hurt named), if known." },
    Item { of: "", name: "on_death", what: Callback { params: &[("thing", "Thing"), ("by", "Thing|nil"), ("weapon", "string|nil")], returns: "" }, doc: "A thing's hit points ran out, whoever lowered them (the ready-made rules, or your script setting thing.state.hp to 0); once, until it lives again (hit points above 0). by and weapon as on_hit. It is no longer listed, counted or touched; its kind and where it was can still be read in this tick. What it means is your code's: points for it, a life less for the player (the ready-made rules only show the fall). Count here what happened (a gunship down): counting what is left cannot tell one that died from one that has not come yet." },
    Item { of: "", name: "on_beat", what: Callback { params: &[("beat", "integer")], returns: "" }, doc: "A beat of the song your game plays begins (music:play): beat counts from 0 at the song's start, on and on through its loops (music.bar for the bar). Runs before on_tick, in the tick the beat begins: waves, shots, a light that pulse with the music. Its clock is the game's, so replays play it the same; while the player's own song plays (none of yours), no beats." },
    Item { of: "", name: "on_fall_out", what: Callback { params: &[("thing", "Thing")], returns: "" }, doc: "A thing fell out of the level (c.physics.lost_below voxels under it, 48 unless set, or down on the world's floor below): once, until it is back in. Things other than the player are taken away after it; the player waits for what your code decides (a life less, and back at the checkpoint: player:place, player.state.hp). Without it the player falls on." },
    Item { of: "", name: "on_pickup", what: Callback { params: &[("thing", "Thing"), ("item", "string")], returns: "" }, doc: "A thing took an item of c.items with thing:take (told in the next tick). Its inventory already counts it." },
    // The engine's functions
    // world
    Item { of: "world", name: "state", what: Field { ty: "table", set: false }, doc: "The run's state, from its start to its end: numbers, strings, booleans and tables of those (each table once). No things: they live in their level (keep them in level.state). Snapshots, replays, saves and the state hash see it." },
    Item { of: "world", name: "tick", what: Field { ty: "integer", set: false }, doc: "Ticks since the run began (60 a second; none while the game holds still)." },
    Item { of: "world", name: "language", what: Field { ty: "string", set: false }, doc: "The language the player chose of the ones your game speaks (c.languages = { \"de\", \"en\", \"fr\" }; SETTINGS offers them, the system's at first): \"de\", two small letters; without c.languages \"en\". It can change while playing: pick your texts by it every picture (texts[world.language].hello). Replays carry it. Over the network each machine draws in its own: in on_draw it is the language of the machine drawing, in on_tick and the events the host's, so keep what a text says in a state and choose its words in on_draw." },
    Item { of: "world", name: "levels", what: Field { ty: "string[]", set: false }, doc: "The levels' names in order: the files in levels/ sorted by name, numbers as numbers (01_hall, 02_tower); one level when there are none." },
    Item { of: "world", name: "leaving", what: Field { ty: "string|nil", set: false }, doc: "Why the level is about to be left at the end of this tick (\"go_to\", \"restart\", \"load\", \"won\", \"lost\"), or nil: the ask that counts so far. One ask counts in a tick: going on (go_to, win) before starting over (restart, load, lose); among those of one rank the first asked. (Leaving by QUIT TO TITLE or a level file saved while playing is no ask: on_level_end hears \"quit\" and \"reload\".)" },
    Item { of: "world", name: "saves", what: Field { ty: "table<integer, SaveInfo>", set: false }, doc: "What lies in each save slot (c.saves.slots of them): world.saves[1] is nil while slot 1 is empty, else { level, seconds }." },
    Item { of: "world", name: "players", what: Field { ty: "(Player|false)[]", set: false }, doc: "Every seat of the game (c.players, 1 unless set), in order: world.players[n] is the player on seat n, or false while nobody sits there (#world.players is how many seats; world.players[1] is player). A HUD shows PRESS START where a seat is false. Players in play in this level: level:players()." },
    Item { of: "world", name: "items", what: Field { ty: "Item[]", set: false }, doc: "The kinds of item (c.items) in their order (c.items.NAME.order, then by name), each { name, model, stack }: for drawing an inventory." },
    Item { of: "world", name: "go_to", what: Method { params: &[("level", "string|integer"), ("place?", "string")], returns: "" }, doc: "Leaves this level for another at the end of the tick (the code after it still runs): by name or number (world:go_to(level.next) goes on in order), arriving at the place called place, else where the level starts. player.state.checkpoint is forgotten. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "restart", what: Method { params: &[("place?", "string")], returns: "" }, doc: "Enters this level afresh at the end of the tick: everything from its file again, level.state fresh; the player at place, else at the place player.state.checkpoint names, else where it starts. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "read", what: Method { params: &[("name", "string")], returns: "string" }, doc: "The text of a file in the game folder, by its path there (world:read(\"levels/microban.txt\")): only while main.lua starts: at its top, in on_world_start or the first on_level_start. The files read count for the replays, and saving one runs main.lua anew." },
    Item { of: "world", name: "save", what: Method { params: &[("slot", "integer")], returns: "" }, doc: "Saves the game into a slot (1 to c.saves.slots) at the end of the tick (or of the on_level_start that asks): world.state, player.state, every level's level.kept, the level being played with its level.state and the voxels changed in it (blown away, built), and where the player stands. The level's things come from its file when it is loaded (a thing kept in level.state comes back as nil: keep what you need to place them again). For save points, or saving on its own in on_level_start. The player can save from the pause menu too." },
    Item { of: "world", name: "load", what: Method { params: &[("slot", "integer")], returns: "" }, doc: "Goes on from the save in a slot at the end of the tick, like a level change (on_level_end(\"load\"), then on_level_start(\"load\")): the states as they were saved, the level from its file with the voxels changed in it as they were, the player where it stood. An empty slot is a mistake: world.saves tells first. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "hitstop", what: Method { params: &[("ticks", "integer")], returns: "" }, doc: "Holds the game still for this many ticks (a hit 2 or 3, 60 at most): everything stops, on_tick too. It is part of the game, so replays see it." },
    Item { of: "world", name: "win", what: Method { params: &[], returns: "" }, doc: "The run is won at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "lose", what: Method { params: &[], returns: "" }, doc: "The run is lost at the end of the tick (on_level_end, then on_world_end); after that the script hears nothing more. One ask counts in a tick: a win or a go_to in the same tick counts before it (world.leaving)." },
    // level
    Item { of: "level", name: "state", what: Field { ty: "table", set: false }, doc: "This visit's state: what the level knows while it is played (the things in it too, moves, time). Fresh each time the level is entered, a restart too; after world:load as it was saved (its things nil). The engine reads one key: level.state.held = true holds the level (the ready-made rules, the physics and touches wait; on_tick, on_draw and the time go on): for a message, a shop, a tally." },
    Item { of: "level", name: "name", what: Field { ty: "string", set: false }, doc: "Its name: its file's name without the ending (hall for levels/hall.toml); main for a game's one level made in code." },
    Item { of: "level", name: "number", what: Field { ty: "integer", set: false }, doc: "Its place among world.levels, from 1." },
    Item { of: "level", name: "next", what: Field { ty: "string|nil", set: false }, doc: "The next level's name, nil after the last one." },
    Item { of: "level", name: "visits", what: Field { ty: "integer", set: false }, doc: "How often it was entered in this run, restarts too: 1 the first time." },
    Item { of: "level", name: "kept", what: Field { ty: "table", set: false }, doc: "What this level keeps across visits for the whole run (a lever pulled, a puzzle solved): read it in on_level_start. Plain data, no things." },
    Item { of: "level", name: "arrived", what: Field { ty: "Place", set: false }, doc: "Where the player arrived in this level, with its name (a place's, or start): a game without a figure of the engine's stands its hero there in on_level_start." },
    Item { of: "level", name: "places", what: Field { ty: "table<string, Place>", set: false }, doc: "Its named places, set in its file (level.places.west): where a player arrives; start is always there (the place called start, else where the ready-made rules stand the player, else c.player.spawn)." },
    Item { of: "level", name: "areas", what: Field { ty: "table<string, Area>", set: false }, doc: "Its named areas, boxes set in its file ([[area]] with name, at and size): level.areas.by_the_stove:holds(player) says whether the player stands in it. Only to read; a level without areas has an empty table." },
    Item { of: "level", name: "size", what: Field { ty: "Point", set: false }, doc: "The level's size in voxels." },
    Item { of: "level", name: "camera", what: Field { ty: "Camera", set: false }, doc: "Where the camera looks: level.camera:follow(thing), level.camera:look_at(x, y)." },
    Item { of: "level", name: "voxel", what: Method { params: &[("x", "integer"), ("y", "integer"), ("z", "integer")], returns: "integer" }, doc: "The voxel at a cell: its palette index, 0 empty (a solid piece's cells too: its model draws them, level:solid says they block)." },
    Item { of: "level", name: "set_voxel", what: Method { params: &[("x", "integer"), ("y", "integer"), ("z", "integer"), ("voxel", "integer|string|nil")], returns: "" }, doc: "Sets the voxel at a cell: a colour \"#rrggbb\" (taken into the level's palette), a palette index, or nil or 0 for empty." },
    Item { of: "level", name: "fill", what: Method { params: &[("x0", "integer"), ("y0", "integer"), ("z0", "integer"), ("x1", "integer"), ("y1", "integer"), ("z1", "integer"), ("voxel", "integer|string|nil")], returns: "" }, doc: "Fills the box from one corner cell to the other, both included, like set_voxel; nil or 0 empties it. A level made in code (c.world.size) is built this way." },
    Item { of: "level", name: "solid", what: Method { params: &[("x", "integer"), ("y", "integer"), ("z", "integer")], returns: "boolean" }, doc: "Whether the cell stops bodies: a voxel there (soft voxels, grass and shrubs, do not), or a solid piece's (solid = true in the level's file)." },
    Item { of: "level", name: "ground", what: Method { params: &[("x", "integer"), ("y", "integer")], returns: "integer" }, doc: "The height of the ground at x, y: the cell above the column's top voxel." },
    Item { of: "level", name: "spawn", what: Method { params: &[("kind", "string"), ("x", "number"), ("y", "number"), ("options?", "SpawnOptions")], returns: "Thing" }, doc: "Places a thing of a kind (c.things) at x, y, on the ground there unless options.z is given; options.facing turns it, options.state is its first state, options.name its name (one thing's in the level), options.props what your code reads of it over its kind's props. A kind without a behavior falls to the ground; with fixed = true (c.things.NAME.fixed) it stays exactly where it is put. With ready-made rules (c.kit) their kinds stand where a marker there would place them. The camera follows the player; in a game without a figure for it (no c.player.kind), the first thing placed. Pieces (\"piece\") come only from a level's file." },
    Item { of: "level", name: "thing", what: Method { params: &[("name", "string|integer|Thing")], returns: "Thing|nil" }, doc: "The thing called name (thing.name: the name its level's file gives it, \"lamp\", else door_1), or with this id, if it is still there." },
    Item { of: "level", name: "things", what: Method { params: &[("kind?", "string")], returns: "Thing[]" }, doc: "Every thing (of a kind, or of a family: c.things.NAME.family), in the order they came: not the player (it belongs to no level), not the dead, not what was taken away this tick. level:things(\"piece\") is every model and light the level's file set down." },
    Item { of: "level", name: "players", what: Method { params: &[], returns: "Player[]" }, doc: "The players playing in this level, in seat order: someone sits there, not out of play (state.out), and its figure stands (one taking a seat comes in at the tick's end). Loop over it for every player's rules: for _, p in ipairs(level:players()) do p:walk(p.input.dir, 40) end. world.players is every seat, the out too (a HUD, a continue)." },
    Item { of: "level", name: "count", what: Method { params: &[("kind?", "string")], returns: "integer" }, doc: "How many things (of a kind or family) there are: not the player, not the dead, not what was taken away this tick (count deaths in on_death)." },
    Item { of: "level", name: "block", what: Method { params: &[("x", "number"), ("y", "number"), ("z", "number")], returns: "string|nil" }, doc: "The kind of the level's block at a voxel: \"solid\", \"oneway\", \"ladder\", \"hazard\", or nil (nothing, decoration, a level not made of blocks). What a block does is your code's: a hazard hurts because you say so. For a thing's whole body: thing:in_block." },
    Item { of: "level", name: "effect", what: Method { params: &[("model", "string"), ("at", "Thing|Point"), ("fps?", "number")], returns: "" }, doc: "Plays a model's frames once (a puff, a sparkle), its feet where a thing stands or at a point, fps frames a second (12). Only shown: not part of the game." },
    Item { of: "level", name: "explode", what: Method { params: &[("at", "Thing|Point"), ("radius", "number")], returns: "" }, doc: "Blows a hole: voxels fly as debris, pieces cut off fall." },
    Item { of: "level", name: "clear_rounds", what: Method { params: &[("team?", "string")], returns: "" }, doc: "Takes every round in flight away, grenades and rockets too before they go off; only those fired by a team if you name one (\"enemies\", \"player\"). A bomb that clears the screen, or mercy for a player coming back." },
    Item { of: "level", name: "sound", what: Method { params: &[("name", "string"), ("at", "Thing|Point"), ("volume?", "number")], returns: "" }, doc: "Plays a sound at a place, by its name: a file of the game, sounds/NAME.toml, or one the engine or the ready-made rules bring." },
    // level.camera
    Item { of: "level.camera", name: "follow", what: Method { params: &[("thing", "Thing")], returns: "" }, doc: "The camera follows this thing from now on; following the player, ready-made rules lead the view as theirs do (the jump & run's look ahead). Every level starts following the player." },
    Item { of: "level.camera", name: "look_at", what: Method { params: &[("x", "number"), ("y", "number")], returns: "" }, doc: "The camera rests over this point from now on, following nothing, whatever spawns (follow follows a thing again): a room, a boss arena, a scene, a field that scrolls by itself." },
    // player
    Item { of: "player", name: "state", what: Field { ty: "table", set: false }, doc: "The player's state for the whole run, from level to level, started from c.state.player. Keys the engine keeps (a script reads and sets them like the rest): score (a whole number), inventory (items by name and how many: player.state.inventory.key; with the ready-made shooter rules also the rounds of its guns and its grenades, items of their names), checkpoint (the name of a place or of a thing in this level where world:restart goes; forgotten on world:go_to); with a figure that has hit points, hp; with ready-made rules hp and invulnerable (ticks left without hits, or true until set back; hp set above 0 brings the shooter's player down back to life); with the ready-made shooter rules (c.kit = \"shooter\") also weapon (the gun in hand, by its name in c.player.weapons). Lives and the like are your game's own keys. Plain data, no things." },
    // thing
    Item { of: "thing", name: "id", what: Field { ty: "integer|nil", set: false }, doc: "Its number, never given to another thing in the same run; nil for the player, who belongs to no level." },
    Item { of: "thing", name: "kind", what: Field { ty: "string|nil", set: false }, doc: "The name of its kind (c.things); \"piece\" for a model or a light the level's file set down (it stays exactly where it is put, as a kind with fixed = true); nil for a player of no kind (the one the ready-made shooter rules make from c.player)." },
    Item { of: "thing", name: "state", what: Field { ty: "table", set: false }, doc: "Its state while it lives: thing.state.hp, the hits it still takes, where its kind has hp (c.things.NAME.hp at the start; the engine keeps it, a script reads and sets it: at 0 on_death runs), inventory (a chest's, a trader's: items by name and how many), pose for a figure of the jump & run's ready-made rules (which set of its frames it shows, c.things.NAME.frames: a prop's, a door's or a sign's always, \"open\", \"idle\" the first; another's while you set one, over its rules' own animation, from the tick it was set: \"attack\", then nil for its own again; the hero's is player.state.pose), goal for a platform of the jump & run's ready-made rules (\"end\" or \"start\": it goes to that end of its travel and stays there, a platform out of a wall, a trapdoor, a lift; nil: to and fro again), freed for a hostage of the ready-made shooter rules (set true: the cage bursts, it runs off), flying for its gunship (up in the air fighting; only to read), light for a piece that brings light (a lamp the level's file set down: how bright all its lights shine together, 0 off to 1, 1 at first; their colours, places and reach are the file's), parts for a piece whose model is a figure of parts (a door with its leaf, a chest with its lid: each part by its name, a rotation about its joint in degrees and an offset in voxels, as frame:model's pose, { leaf = { rotation = 90 } } swings the leaf open; nil for at rest; the engine draws it so every picture; it lasts this visit of the level, so what must stay open the next time goes in world.state and is set again in on_level_start; a solid piece still blocks where its model stands at rest), and anything of the game's own. It goes when the thing goes." },
    Item { of: "thing", name: "x", what: Field { ty: "number", set: false }, doc: "Where it stands: the middle of its feet. A piece's is the middle of the cell its file's at names (at + 0.5): a cube for it goes at math.floor(thing.x) + dx." },
    Item { of: "thing", name: "y", what: Field { ty: "number", set: false }, doc: "Where it stands: the middle of its feet. A piece's is the middle of the cell its file's at names (at + 0.5): a cube for it goes at math.floor(thing.x) + dx." },
    Item { of: "thing", name: "z", what: Field { ty: "number", set: false }, doc: "The height of its feet." },
    Item { of: "thing", name: "on_ground", what: Field { ty: "boolean", set: false }, doc: "Whether it stands on something." },
    Item { of: "thing", name: "wall", what: Field { ty: "Way|nil", set: false }, doc: "The way to a wall right beside it (within half a voxel), higher than a step (the way it faces first), or nil: a wall jump, a wall slide." },
    Item { of: "thing", name: "facing", what: Field { ty: "Way", set: false }, doc: "The way it faces, one of the 8 ways." },
    Item { of: "thing", name: "seat", what: Field { ty: "integer|nil", set: false }, doc: "A player's seat (1 for player, 2 and on for the others); nil for a level's thing, so if thing.seat then asks whether a thing is a player, in any handler (a ~= player leaves out seats 2 and on)." },
    Item { of: "thing", name: "input", what: Field { ty: "Input", set: false }, doc: "A player's keys this tick, as input is seat 1's: p.input.dir, p.input:pressed(\"fire\"). Only in on_tick, as input." },
    Item { of: "thing", name: "controls", what: Field { ty: "Controls", set: false }, doc: "What a player plays with, as controls is seat 1's: p.controls.device, p.controls:key(\"fire\") (a key in braces in a text is the first of frame.players's)." },
    Item { of: "thing", name: "walk", what: Method { params: &[("way", "Way"), ("speed", "number")], returns: "" }, doc: "Walks this tick the way given at this speed (voxels a second), facing it; it falls, steps up ledges and lands by itself. Only things your script moves (a kind with no behavior in c.things, or one whose behavior thing:enable switched off)." },
    Item { of: "thing", name: "face", what: Method { params: &[("way", "Way")], returns: "" }, doc: "Turns it to the nearest of the 8 ways: things your script moves, and the player with ready-made rules (it turns again as it moves; the jump & run's hero only left or right)." },
    Item { of: "thing", name: "jump", what: Method { params: &[("height", "number")], returns: "" }, doc: "Jumps this high (voxels) if it stands on something. Only things your script moves." },
    Item { of: "thing", name: "ahead", what: Method { params: &[("distance", "number")], returns: "Point" }, doc: "The point this far ahead of its feet, a voxel up." },
    Item { of: "thing", name: "near", what: Method { params: &[("other", "Thing"), ("distance", "number")], returns: "boolean" }, doc: "Whether the other thing stands closer than this." },
    Item { of: "thing", name: "place", what: Method { params: &[("x", "number|Place|Thing"), ("y?", "number"), ("z?", "number")], returns: "" }, doc: "Puts it exactly there, standing still (on the ground unless z is given): the end of a slide on a grid (a kind with fixed = true stays wherever it is put). Given a place (level.places.west) it stands there, facing its way if it has one; given a thing, where that thing stands (a checkpoint). Things your script moves, and the player: the ready-made rules stand theirs there, still and on its feet, facing the way it did (a respawn; thing:face turns it)." },
    Item { of: "thing", name: "attack", what: Method { params: &[("weapon", "string"), ("target?", "Thing|Point")], returns: "" }, doc: "It attacks with a weapon of c.weapons (with the ready-made shooter rules, c.kit = \"shooter\"): a blow at the target (or the first in reach in front), a shot at it (or the way it faces; the player's finds a target with the aim assist), a throw to it (or its full throw ahead). It starts at the tick's end; then it waits the weapon's cooldown before the next. The player spends the weapon's rounds from player.state.inventory and attacks only while it has them. An action named in c.player.attacks attacks by itself; use another action for attacks your code decides (a charge shot, a combo)." },
    Item { of: "thing", name: "hurt", what: Method { params: &[("damage", "integer"), ("by?", "Thing"), ("weapon?", "string")], returns: "" }, doc: "Hurts it (by a thing, with a weapon, if given): anything with hit points loses them (thing.state.hp, player.state.hp; at 0 on_death runs), and the ready-made rules' own take it as their hits (a soldier falls, a gunship spins down, the player is thrown back). A kind's team and immune do not stop it: your code decides whom it hurts. While it may not be hurt (player.state.invulnerable) nothing happens. on_hit runs in the next tick's events, as for the ready-made rules' hits; a thing it kills is out of the level's lists and counts at once." },
    Item { of: "thing", name: "touching", what: Method { params: &[("kind?", "string")], returns: "Thing[]" }, doc: "The things it touches now (of a kind or family), in the order they came: for what lasts while touched (spikes, an enemy's contact, lava), asked in on_tick, where it stays true every tick. Not the dead, nor what was taken away this tick." },
    Item { of: "thing", name: "in_block", what: Method { params: &[("kind", "string")], returns: "boolean" }, doc: "Whether any part of its body is in a block of the level of this kind: \"hazard\", \"ladder\", \"oneway\" or \"solid\" (level:block for one voxel)." },
    Item { of: "thing", name: "bounce", what: Method { params: &[("height", "number")], returns: "" }, doc: "Sends it up this high (voxels), standing or in the air: off an enemy it landed on, a spring. Always the full height; the jump button plays no part. Things your script moves, and the jump & run's hero (c.kit = \"platformer\")." },
    Item { of: "thing", name: "launch", what: Method { params: &[("way", "Way"), ("speed", "number"), ("rise?", "number")], returns: "" }, doc: "Sets it moving the way given at this speed (voxels a second), and up as high as rise (voxels) if given, standing or in the air: a dash (player:launch(player.facing, 240)), a wall jump, a knockback. Physics carries it on, and ready-made rules steer it on from there (the hero as the player presses, a soldier as he means to go); switch them off for a while with thing:enable to keep them out of it. Things your script moves, things whose behavior is off, the player, the hero, walkers, hoppers, soldiers and hostages." },
    Item { of: "thing", name: "enable", what: Method { params: &[("behavior", "string"), ("on", "boolean")], returns: "" }, doc: "Switches its ready-made behavior (its kind's, c.things.NAME.behavior: \"hero\", \"walker\", \"soldier\", ...) off or on again. Off, it does nothing by itself: it still falls, is hit and touched, and your script moves it like its own (thing:walk, thing:jump, thing:launch). A stunned hero, an end walk, a frozen enemy. Until it leaves its level (the hero: until the next level). Not yet the shooter's player, who is no thing of a kind." },
    Item { of: "thing", name: "take", what: Method { params: &[("item", "string"), ("count?", "integer")], returns: "integer" }, doc: "It takes an item of c.items (one, or count): into its state.inventory (the player's: player.state.inventory), no more than the item's stack; how many it took. on_pickup runs in the next tick's events." },
    Item { of: "thing", name: "remove", what: Method { params: &[], returns: "boolean" }, doc: "Takes it out of the level at the end of this tick: it can still be read until then, but it is out of the level's lists, counts and touches at once. Whether it was there to remove. Not the player: it goes from level to level." },
    Item { of: "thing", name: "name", what: Field { ty: "string|nil", set: false }, doc: "Its name in this level: the name the level's file gives it (name = \"lamp\"), else a thing placed by the level file is called its kind and its number among them (door_1, door_2 in the file's order), a spawned one what level:spawn named it. nil for the player and for things the ready-made rules bring later. player.state.checkpoint and level:thing take it." },
    Item { of: "thing", name: "families", what: Field { ty: "table<string, boolean>", set: false }, doc: "The families of its kind (c.things.NAME.family = { \"enemies\" }), as a set: if b.families.enemies then ... end. Only to read." },
    Item { of: "thing", name: "props", what: Field { ty: "table", set: false }, doc: "What your code reads of it, fixed while it lives: its kind's props (c.things.NAME.props = { worth = 100 }) and what level:spawn gave it. Only to read; what changes goes in thing.state." },
    // input
    Item { of: "input", name: "left", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"left\") whether it went down this tick)." },
    Item { of: "input", name: "right", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"right\") whether it went down this tick)." },
    Item { of: "input", name: "up", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"up\") whether it went down this tick)." },
    Item { of: "input", name: "down", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"down\") whether it went down this tick)." },
    Item { of: "input", name: "dir", what: Field { ty: "Way", set: false }, doc: "The directions held as a way: x and y each -1, 0 or 1." },
    Item { of: "input", name: "held", what: Method { params: &[("action", "string")], returns: "boolean" }, doc: "Whether an action (c.actions, one the ready-made rules bring, or a direction: \"left\", \"right\", \"up\", \"down\") is held this tick. The ready-made rules read the same keys and use none of them up." },
    Item { of: "input", name: "pressed", what: Method { params: &[("action", "string")], returns: "boolean" }, doc: "Whether an action or a direction went down this tick: one step a press on a grid, one line in a menu. A press of interact that on_interact took is used up." },
    Item { of: "input", name: "released", what: Method { params: &[("action", "string")], returns: "boolean" }, doc: "Whether an action or a direction went up this tick." },
    Item { of: "input", name: "repeated", what: Method { params: &[("action", "string")], returns: "boolean" }, doc: "True as the key goes down, and again and again while it is held (after a third of a second, ten times a second), as a menu's cursor moves on a held key: for menus, lists and stepping on a grid. Directions too: input:repeated(\"down\")." },
    // screen
    Item { of: "music", name: "play", what: Method { params: &[("song?", "string"), ("from?", "string")], returns: "" }, doc: "Plays a song of c.audio.music by its file's name (\"storm\" for music/storm.mml) as your game's own, from the next bar of the one playing (from: \"bar\", \"beat\" or \"now\"; at once when none of yours plays): it starts at its beat 0 and keeps time with the game, held while the game pauses. Asked for while it plays, it goes on. Without a song: the player's own again (as the settings chose), from the next bar. Your game's song wins over the player's choice." },
    Item { of: "music", name: "song", what: Field { ty: "string|nil", set: false }, doc: "The song your game plays (music:play), by its file's name; nil while the player's own plays." },
    Item { of: "music", name: "beat", what: Field { ty: "integer|nil", set: false }, doc: "Beats since your game's song began, from 0 (its #tempo); nil while the player's own song plays." },
    Item { of: "music", name: "intensity", what: Field { ty: "number", set: true }, doc: "How tense the game is, from 0 (calm) to 1 (all of the song; 1 unless you set it): the layers of your game's song (#layer in the song: drums from 0.35, the melody from 0.8) come in above their tension and go below it, each over a bar. Set it in on_tick from your states: enemies on the screen, a boss, a hit." },
    Item { of: "music", name: "bar", what: Field { ty: "integer|nil", set: false }, doc: "Bars since your game's song began, from 0 (its #meter beats to a bar, 4 unless the song says); nil while the player's own song plays." },
    Item { of: "screen", name: "shake", what: Method { params: &[("amount", "number")], returns: "" }, doc: "Shakes the screen: 0..1 more (a small hit 0.1, a blast 0.35, 1 at most); it dies down by itself. Only the camera moves, and players can turn shaking off." },
    Item { of: "screen", name: "burst", what: Method { params: &[("icon", "string"), ("x", "number"), ("y", "number")], returns: "" }, doc: "An icon of c.icons bursts on the HUD at x, y (its top left): its voxels fly off and fall, a heart lost, a grenade thrown. Only shown, not part of the game." },
    Item { of: "screen", name: "height", what: Field { ty: "number", set: false }, doc: "How many UI voxels the HUD is from top to bottom (c.ui.height), for a burst near its bottom; on_draw's frame has its width too." },
    Item { of: "screen", name: "flash", what: Method { params: &[("strength", "number")], returns: "" }, doc: "Flashes the screen white, 0..1 (times c.effects.screen_flash); it fades by itself. Players can turn flashes off." },
    Item { of: "frame", name: "model", what: Method { params: &[("file", "string"), ("thing", "Thing"), ("index?", "integer"), ("rotation?", "Rotation"), ("pose?", "table<string, Rotation>")], returns: "" }, doc: "Draws a model (a kind's or one in c.models) where the thing stands in the level, facing its way; index is which of its frames (from 0); without it, its kind's frames for what it does (c.things.NAME.frames: idle standing, walk, jump in the air), else the model's own: frame 0 standing, 1 in the air, the rest walking, a frame every half its height walked. A model's pivot is its cell size / 2 across and deep (the middle cell of an odd size): of an even size it lies one voxel off the middle, so facing -y or sideways it fills cells one voxel higher on the axis a turn flips than facing +y (use odd sizes where that matters). rotation turns it further, freely, in degrees about its middle (the voxel at half its height over its pivot): { roll = 20 } banks a ship to the right, { rotation = 45 } faces a diagonal, { pitch = 10 } lifts its nose; roll is taken first, then pitch, then rotation (a level's piece with rotation, pitch and roll is drawn so). Rotated, its voxels leave the grid, but for quarter turns (90 degrees each, any of the three); a kind with rotates = \"free\" in c.things faces exactly its way by itself (a diagonal too), without, its quarter turns. pose moves a figure made of parts (a model whose objects have names in the file: head, body, arm_left ...): each part by its name, a rotation about its joint, what hangs on it moving along: { head = { rotation = 30 }, arm_right = { pitch = 90 } } raises the right arm forward; offset = { x, y, z } moves a part by voxels (fractions too) after its rotation, its joint along: { bust = { pitch = 10, offset = { 0, 1, 0 } } }. Over its kind's swing (c.things.NAME.swing: parts swinging as it walks)." },
    Item { of: "frame", name: "cube", what: Method { params: &[("x", "number"), ("y", "number"), ("z", "number"), ("color?", "Color"), ("glow?", "number")], returns: "" }, doc: "Draws one voxel in the level, at x, y, z in the level's voxels (alpha below 255: see-through; glow from 0 to 1)." },
    Item { of: "frame", name: "light", what: Method { params: &[("at", "Thing|Point"), ("radius", "number"), ("color?", "Color")], returns: "" }, doc: "A light in the level this frame, at a thing or a point." },
    Item { of: "frame", name: "daylight", what: Method { params: &[("light", "Daylight")], returns: "" }, doc: "This picture's daylight: the sun's way (sun = { x, y, z }, its light travelling down), its colour and strength (sun_color, white unless set), the light every face gets (ambient, 0 to 1), how dark shadows are (shadow, 0 to 1) and the sky's colour (sky); what you leave out is c.light's. Work it out from your game's clock every picture for a day and a night: the sun low and warm at dawn, white at noon, orange at dusk, a dim blue at night with frame:light lamps. Only the picture: the game's rules do not see it. A game whose code names daylight has every face a sun above the horizon can light in its level." },
    Item { of: "frame", name: "camera", what: Method { params: &[("camera", "PictureCamera")], returns: "" }, doc: "This picture's camera, for a scene shot by the engine: zoom in on something and out again, a cut, a slow pan. look_at is the point the view centres on ({ x, y, z } in voxels, z left out: the camera's height; or a thing), width how many voxels the picture shows across there (or height, how many from bottom to top), pitch degrees below the horizon, fov the field of view; what you leave out is the game's camera's (c.camera), so { width = 80 } zooms in on what the camera follows and keeps following it. Only this picture: the next one without it is the game's camera again, nothing is kept, so call it in every picture the scene lasts and work a pan out from your clock (world.tick + frame.alpha, music.beat); the engine smooths nothing. Only the picture: the game's rules, the sounds and level.camera do not see it; frame:to_screen after it goes by it. The camera does not turn round yet. A picture at another pitch or fov than c.camera's needs faces of the level the game's own view never shows: the level is built anew with them once, a moment's hitch the first time." },
    Item { of: "frame", name: "menu", what: Method { params: &[("menu", "Menu")], returns: "" }, doc: "A menu in the engine's look, on a plate: a title, tabs (tab: the chosen one, from 1), rows (a text, or { text =, value =, icon =, dim = }: a value at the right, an icon of c.icons before the text, dimmed when it cannot be had), the chosen row (at, from 1) yellow with the engine's red marker, and a hint below; an action in braces is its key. In the middle of the screen unless x and y say where (its top left), as wide as it needs unless width says; shown = how many rows show at most, a longer list scrolled to keep at in view. What is open and where the cursor stands is your game's: keep it in level.state, hold the level while it is open (level.state.held = true) and move it in on_tick (input:repeated)." },
    Item { of: "frame", name: "grid", what: Method { params: &[("grid", "Grid")], returns: "" }, doc: "Slots in rows on a plate, an inventory's: cells, each { icon =, count =, dim = } (an icon of c.icons, a number at its corner, darker when dim) or false for an empty slot; columns across (up to 8 unless said), cell its size in UI voxels (18), at the chosen slot (from 1) edged in yellow, an optional title. In the middle unless x and y say where." },
    Item { of: "frame", name: "text", what: Method { params: &[("text", "string"), ("x", "number"), ("y", "number"), ("color?", "Color"), ("size?", "integer")], returns: "" }, doc: "Text on the HUD, over the level: x and y its top left, from the HUD's top left in UI voxels (frame.width across, frame.height down). size 1 the small letters (7 voxels high), 2 to 4 the big ones (c.ui.display_font). An action in braces is its key as the player sees it: \"PRESS {jump}\" writes PRESS X, or the pad's button (controls:key); {{ writes a brace. Written as you write it: lower case, the letters of German and French (ä ö ü ß é è ê à ç œ …) and signs such as · × → ▲ ▼ ✓ … „ « » too, in the small letters and the big ones drawn from them (a display font has capitals only); a key's name in capitals." },
    Item { of: "frame", name: "players", what: Field { ty: "Player[]", set: false }, doc: "The players at the machine this picture is drawn for, in seat order (at one machine every taken seat, the out too; over the network that machine's own): their HUD. A key in braces in a text, and controls in on_draw, are the first of them's (seat 1's where none plays there); another player's is p.controls:key." },
    Item { of: "frame", name: "width", what: Field { ty: "number", set: false }, doc: "How many UI voxels the HUD is across: it follows the screen's shape, so put things at the right edge from it (frame.width - 3 - frame:text_width(score))." },
    Item { of: "frame", name: "height", what: Field { ty: "number", set: false }, doc: "How many UI voxels the HUD is from top to bottom (c.ui.height)." },
    Item { of: "frame", name: "alpha", what: Field { ty: "number", set: false }, doc: "How far into the tick this picture is, 0 to 1: add it to a count of ticks for motion that runs smooth between them." },
    Item { of: "frame", name: "to_screen", what: Method { params: &[("at", "Thing|Point"), ("up?", "number")], returns: "number?, number?" }, doc: "Where a point of the level shows in this picture, in the HUD's measure (x and y as frame:text takes them, from the top left), or nothing when it lies behind the camera. A thing's point is where its feet are drawn, gliding as it does; up lifts it that many voxels (over a figure's head). For names, health bars and numbers rising from where something happened." },
    Item { of: "frame", name: "text_width", what: Method { params: &[("text", "string"), ("size?", "integer")], returns: "number" }, doc: "How many UI voxels wide the text is at that size (for centring and putting it at the right edge)." },
    Item { of: "frame", name: "box", what: Method { params: &[("x", "number"), ("y", "number"), ("width", "number"), ("height", "number"), ("color?", "Color")], returns: "" }, doc: "A filled box on the HUD, its top left at x, y." },
    Item { of: "frame", name: "bar", what: Method { params: &[("x", "number"), ("y", "number"), ("width", "number"), ("height", "number"), ("fraction", "number"), ("color?", "Color"), ("back?", "Color")], returns: "" }, doc: "A bar on the HUD filled the fraction (0 to 1) of its width: a boss's health, a timer. color the filled part (red unless given), back the rest." },
    Item { of: "frame", name: "icon", what: Method { params: &[("name", "string"), ("x", "number"), ("y", "number")], returns: "" }, doc: "A small picture of c.icons on the HUD, its top left at x, y: c.icons.heart = { rows = { \".RR.RR.\", ... }, colors = { R = { 214, 36, 44 } } }." },
    Item { of: "frame", name: "drop_in", what: Method { params: &[("text", "string"), ("x", "number"), ("y", "number"), ("ticks", "number"), ("color?", "Color"), ("size?", "integer")], returns: "" }, doc: "Text whose letters drop in one after another and bounce, ticks after it began (keep the tick it began in a state): GAME OVER, a stage's name. Before 0 nothing shows." },
    Item { of: "controls", name: "device", what: Field { ty: "string|nil", set: false }, doc: "What the player plays with, the one used last: \"keyboard\" or \"gamepad\"; nil before the run's first tick and where nobody plays (tests, a replay recorded without it)." },
    Item { of: "area", name: "name", what: Field { ty: "string", set: false }, doc: "Its name in the level's file (level.areas.NAME)." },
    Item { of: "area", name: "x", what: Field { ty: "integer", set: false }, doc: "Its low corner in voxels: x, y and z where it begins." },
    Item { of: "area", name: "y", what: Field { ty: "integer", set: false }, doc: "Its low corner's y." },
    Item { of: "area", name: "z", what: Field { ty: "integer", set: false }, doc: "Its low corner's z." },
    Item { of: "area", name: "size", what: Field { ty: "Point", set: false }, doc: "How big it is in voxels, across, deep and up." },
    Item { of: "area", name: "holds", what: Method { params: &[("at", "Thing|Point")], returns: "boolean" }, doc: "Whether a thing stands inside it (where its feet are, the middle of the cell it stands on) or a point lies inside it ({ x, y, z }; without z at any height). Ask it in on_tick: if level.areas.by_the_stove:holds(player) then ... end." },
    Item { of: "controls", name: "key", what: Method { params: &[("action", "string")], returns: "string" }, doc: "The first key or button of an action (c.actions, one the ready-made rules bring, or pause) as the player sees it: the keyboard's key as it is printed on it (Y where the binding is Z on a German keyboard), the pad's button as it is printed on the pad (B, CIRCLE). Empty if nothing is bound or nobody plays." },
];

/// The game model's objects, as scripts name them: the globals and
/// `level.camera`. Their names are offered after a dot or colon.
pub const OWNERS: [&str; 11] = ["world", "level", "level.camera", "player", "thing", "area", "input", "screen", "frame", "controls", "music"];

/// What `of` offers: its own names, and for `player` a thing's too (it is
/// one; its own say more, `player.state`).
pub fn items_of(of: &str) -> impl Iterator<Item = &'static Item> + '_ {
    let own = API.iter().filter(move |i| i.of == of);
    let inherited = API.iter().filter(move |i| of == "player" && i.of == "thing" && !API.iter().any(|p| p.of == "player" && p.name == i.name));
    own.chain(inherited)
}

/// The objects every script has as globals.
const GLOBALS: [&str; 7] = ["world", "level", "player", "input", "screen", "controls", "music"];

/// The class each owner's items belong to.
fn class(of: &str) -> &'static str {
    match of {
        "world" => "World",
        "level" => "Level",
        "level.camera" => "Camera",
        "player" => "Player",
        "thing" => "Thing",
        "area" => "Area",
        "input" => "Input",
        "screen" => "Screen",
        "frame" => "Frame",
        "controls" => "Controls",
        "music" => "Music",
        _ => "",
    }
}

/// What each object is, for the definitions' classes.
fn about(of: &str) -> &'static str {
    match of {
        "world" => "The whole run: its state, its time, how it ends.",
        "level" => "The level being played: its voxels, its things, what happens in it, its camera.",
        "level.camera" => "Where the camera looks.",
        "player" => "The one who plays: a thing that belongs to no level and goes from level to level (everything a thing has); its state is the run's. Its figure is the ready-made rules' player, a thing of c.player.kind, or none (it then stands nowhere).",
        "thing" => "A thing in the level, by its id.",
        "area" => "A named box of the level, set in its file (level.areas.NAME): only to read.",
        "input" => "The keys held this tick, by the game's action names: only in on_tick.",
        "screen" => "The picture's effects (a shake, a flash, an icon bursting), from anywhere the game changes.",
        "frame" => "The picture on_draw makes (function on_draw(frame)): only there, only for this picture.",
        "controls" => "What the player plays with: the device and the keys as the player sees them (their keyboard's, their pad's). Replays carry it, so the game may go by it anywhere. Seat 1's; in on_draw the first player's at the machine drawing (frame.players).",
        "music" => "The song your game plays of its own choice, and its beats and bars: the game can go with the music (waves on the beat), and replays play it the same.",
        _ => "",
    }
}

fn params_text(params: &[(&str, &str)]) -> String {
    params.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ")
}

/// A name of the API as a script writes it, with what it takes and gives
/// (`level:spawn(kind: string, ...) -> Thing`).
pub fn signature(item: &Item) -> String {
    let gives = |returns: &str| if returns.is_empty() { String::new() } else { format!(" -> {returns}") };
    match &item.what {
        Field { ty, set } => format!("{}.{}: {ty}{}", item.of, item.name, if *set { " (settable)" } else { "" }),
        Method { params, returns } => format!("{}:{}({}){}", item.of, item.name, params_text(params), gives(returns)),
        Callback { params, returns } => format!("function {}({}){}", item.name, params_text(params), gives(returns)),
        Function { params, returns } => format!("{}({}){}", item.name, params_text(params), gives(returns)),
    }
}

/// The API as text, one name a line with what it does; `filter` keeps the
/// names that contain it.
pub fn text(filter: Option<&str>) -> String {
    let mut out = String::new();
    for item in API.iter().filter(|i| filter.is_none_or(|f| i.name.contains(f) || i.of.contains(f))) {
        out += &format!("{}\n    {}\n", signature(item), item.doc);
    }
    out
}

/// The definitions for LuaLS (`lua/voxcade.lua`).
pub fn definitions() -> String {
    let mut out = String::from(
        "---@meta voxcade\n-- Voxcade's Lua API for editors (LuaLS), written by `voxcade api --lua`\n-- from the engine's register of it (src/lua/api.rs): made anew, not edited.\n\n",
    );
    out += TYPES;
    for of in OWNERS {
        let class = class(of);
        let parent = if of == "player" { " : Thing" } else { "" };
        out += &format!("\n---@class {class}{parent} {}\n", about(of));
        for item in API.iter().filter(|i| i.of == of) {
            if let Field { ty, .. } = item.what {
                out += &format!("---@field {} {ty} {}\n", item.name, item.doc);
            }
        }
        out += &format!("local {class} = {{}}\n");
        for item in API.iter().filter(|i| i.of == of) {
            if let Method { params, returns } = &item.what {
                out += &function(&format!("{class}:{}", item.name), item.doc, params, returns);
            }
        }
    }
    out += "\n-- The game model's objects, there in every script.\n";
    for of in GLOBALS {
        out += &format!("---@type {}\n{of} = {{}}\n", class(of));
    }
    out += "\n-- The engine's functions every script has.\n";
    for item in API {
        if let Function { params, returns } = &item.what {
            out += &function(item.name, item.doc, params, returns);
        }
    }
    out += "\n-- The functions a game defines; the engine calls them.\n";
    for item in API {
        if let Callback { params, returns } = &item.what {
            out += &function(item.name, item.doc, params, returns);
        }
    }
    out += &conf_classes();
    out
}

fn function(name: &str, doc: &str, params: &[(&str, &str)], returns: &str) -> String {
    let mut out = format!("\n---{doc}\n");
    for (n, t) in params {
        out += &format!("---@param {n} {t}\n");
    }
    if !returns.is_empty() {
        out += &format!("---@return {returns}\n");
    }
    let names: Vec<&str> = params.iter().map(|(n, _)| n.trim_end_matches('?')).collect();
    out + &format!("function {name}({}) end\n", names.join(", "))
}

/// What a key of `conf.lua` means, where its name alone does not say
/// enough: section, key, its line in the definitions.
const CONF_DOCS: &[(&str, &str, &str)] = &[(
    "audio",
    "drive",
    "How hard the game's sound is pushed into the limiter, in decibels (0 to 12): 0 as it is, more makes it denser and louder; music you listen to for long tires sooner when dense.",
)];

/// `Conf` and its sections from the engine's defaults: completion for
/// `c.` in `conf.lua` (the ready-made rules' sections and keys are left open).
fn conf_classes() -> String {
    let toml::Value::Table(all) = toml::Value::try_from(GameConfig::default()).expect("the defaults are a table") else { unreachable!() };
    let mut out = String::from("\n---@class Conf The game's settings (conf.lua); ready-made rules (c.kit) add their own (c.weapons).\n---@field [string] table\n");
    let mut sections = String::new();
    for (key, value) in &all {
        match value {
            toml::Value::Table(t) if !t.is_empty() => {
                out += &format!("---@field {key} Conf.{key}\n");
                sections += &format!("\n---@class Conf.{key}\n---@field [string] any\n");
                for (k, v) in t {
                    let doc = CONF_DOCS.iter().find(|(s, n, _)| s == key && n == k).map_or(String::new(), |(_, _, d)| format!(" {d}"));
                    sections += &format!("---@field {k} {}{doc}\n", lua_type(v));
                }
            }
            other => out += &format!("---@field {key} {}\n", lua_type(other)),
        }
    }
    // What is left out while unset.
    out += "---@field items table<string, { model?: string, stack?: integer, order?: integer }> What can be carried, by name: an inventory counts these.\n";
    out += "---@field saves { slots?: integer, version?: integer } Save games: how many slots, and the version of what a save holds.\n";
    out += "---@field voxcade string The Voxcade the game's files are for (\"0.6.0\"): a game for an older one is brought up to date by voxcade upgrade, one for a newer one does not open.\n";
    out += "---@field languages string[] The languages the game speaks, two small letters each ({ \"de\", \"en\", \"fr\" }): SETTINGS offers them, world.language says which the player chose.\n";
    sections += "\n---@class Conf.player\n---@field kind string The kind of thing (c.things) the player's figure is, without ready-made rules.\n";
    out + &sections
}

fn lua_type(v: &toml::Value) -> String {
    match v {
        toml::Value::String(_) => "string".into(),
        toml::Value::Integer(_) => "integer".into(),
        toml::Value::Float(_) => "number".into(),
        toml::Value::Boolean(_) => "boolean".into(),
        toml::Value::Datetime(_) => "string".into(),
        toml::Value::Array(a) => match a.first() {
            Some(first) => format!("{}[]", lua_type(first)),
            None => "any[]".into(),
        },
        toml::Value::Table(_) => "table<string, any>".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The register names exactly what `world.rs` gives the scripts, field
    /// by field and method by method.
    #[test]
    fn the_register_is_what_the_scripts_see() {
        let src = include_str!("world.rs");
        let mut offered: Vec<(String, String)> = Vec::new();
        let mut of = "";
        for line in src.lines() {
            if let Some(rest) = line.strip_prefix("impl UserData for ") {
                of = match rest.trim_end_matches(" {").trim_end_matches(" {}") {
                    "Stage" | "Absent" => "",
                    "WorldView" => "world",
                    "LevelView" => "level",
                    "CameraView" => "level.camera",
                    "ThingRef" => "thing",
                    "AreaView" => "area",
                    "InputView" => "input",
                    "ScreenView" => "screen",
                    "Frame" => "frame",
                    "ControlsView" => "controls",
                    "MusicView" => "music",
                    other => panic!("an object the register does not know: {other}"),
                };
            }
            for call in ["add_field_method_get(\"", "add_field_function_get(\"", "add_method(\"", "add_function(\"", "add_method_mut(\""] {
                if let Some(at) = line.find(call) {
                    let name = line[at + call.len()..].split('"').next().unwrap();
                    assert!(!of.is_empty(), "{name}: offered by an object scripts do not see");
                    offered.push((of.to_string(), name.to_string()));
                }
            }
        }
        offered.sort();
        offered.dedup();
        // `player` is a thing: what the register says of it, a thing offers.
        let of = |i: &Item| if i.of == "player" { "thing".to_string() } else { i.of.to_string() };
        let mut listed: Vec<(String, String)> = API.iter().filter(|i| !i.of.is_empty()).map(|i| (of(i), i.name.to_string())).collect();
        listed.sort();
        listed.dedup();
        assert_eq!(listed, offered);
        for item in API.iter().filter(|i| matches!(i.what, Field { set: true, .. })) {
            assert!(src.contains(&format!("_set(\"{}\"", item.name)), "{}.{} is settable in the register only", item.of, item.name);
        }
    }

    /// `lua/voxcade.lua` is the register as it stands (`voxcade api --lua
    /// lua/voxcade.lua` writes it anew).
    #[test]
    fn the_editors_definitions_are_up_to_date() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lua/voxcade.lua");
        let written = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        assert!(written == definitions(), "lua/voxcade.lua is out of date: voxcade api --lua lua/voxcade.lua");
    }
}

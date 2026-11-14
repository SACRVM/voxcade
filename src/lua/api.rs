//! The Lua API in one place: what a game's scripts see (the game model's
//! objects `world`, `level`, `player`, things, `input`, `screen`;
//! `docs/game-model.md`) and the functions a game defines that the engine
//! calls.
//! From this register come the definitions for editors (`voxcade api --lua
//! lua/voxcade.lua`, LuaLS: completion and checks in VS Code, Zed, Neovim),
//! the text `voxcade api` and the MCP tool `api` show, and a test keeps it
//! in step with what `world/` really offers.

/// One name of the API.
pub struct Item {
    /// Whose it is: one of `OWNERS`, or "" for a function the game defines
    /// or the engine gives.
    pub of: &'static str,
    pub name: &'static str,
    pub what: What,
    /// What it is, for HELP, `voxcade api` and LuaLS: a short first sentence,
    /// then short sentences; a line of its own (after a `\n`) begins a new
    /// part, `- ` a list's item (`thing.state`'s keys, each kit's apart).
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
";

pub const API: &[Item] = &[
    // The functions a game defines.
    Item { of: "", name: "require", what: Function { params: &[("name", "string")], returns: "any" },
        doc: "Runs another Lua file of your game folder once and gives back what it returns. require(\"market\") runs market.lua, \
            require(\"lang.de\") lang/de.lua (dots between folders), and gives back what the file returns (true if nothing); asked again, \
            the same value. Only while the game starts: at the top of main.lua and of the files it requires. The files share main.lua's \
            globals (a handler may live in any of them) and are your game's code as main.lua is: saving one runs the game's code anew, \
            replays count it." },
    Item { of: "", name: "conf", what: Callback { params: &[("c", "Conf")], returns: "" }, doc: "conf.lua: your game's settings; what you \
            leave out keeps the engine's default. Set them on c: c.title, c.physics.gravity, c.things.NAME = { ... }. Saved while the game \
            runs, a setting takes effect at once; the ready-made rules (c.kit), the actions and keys, the level, the markers, the player's \
            start and kind after a restart (F5)." },
    Item { of: "", name: "on_world_start", what: Callback { params: &[], returns: "" }, doc: "Runs once when a run begins, before the first \
            level is there. Give world.state and player.state their first values here. Globals set until the first level has begun stay as \
            they are from then on: what changes goes in a state." },
    Item { of: "", name: "on_level_start", what: Callback { params: &[("how", "string")], returns: "" },
        doc: "Runs each time a level begins, before its first tick. Place things and set level.state here, and build a level made in \
            code.\n- how: why it began. \"start\" when the run begins, \"go_to\" after world:go_to (the same level too), \"restart\" after \
            world:restart, \"load\" after world:load (level.state as it was saved), \"reload\" when its file was saved while playing.\nThe \
            player already stands at level.arrived: do not spawn it here when c.player.kind gives it a figure. An ask here (world:go_to …) \
            counts at once, before the level's first tick." },
    Item { of: "", name: "on_level_end", what: Callback { params: &[("why", "string")], returns: "" },
        doc: "Runs just before the level is left. why says why: \"go_to\", \"restart\", \"load\", \"won\", \"lost\", \"quit\" (QUIT TO \
            TITLE) or \"reload\". Copy from level.state what should outlive the visit (into world.state, player.state or level.kept). \
            Nothing more can be asked here." },
    Item { of: "", name: "on_world_end", what: Callback { params: &[("result", "string")], returns: "" },
        doc: "Runs when the run ends, after on_level_end. result is \"won\", \"lost\" or \"quit\" (QUIT TO TITLE). The end cannot be undone \
            here; a last chance to change the states for the record." },
    Item { of: "", name: "on_player_join", what: Callback { params: &[("p", "Player"), ("how", "string")], returns: "" },
        doc: "Runs when another player joins while the game runs: a pad's START, the keyboard's second half, a machine over the network. It \
            runs in the tick they join, first among the events; not for the seats taken as the run begins.\n- how: \"new\" (p.state a copy \
            of c.state.player) or \"back\" (the seat's state from before it left, or from a save).\nIts figure comes in at the tick's end, \
            where the first player in play stands. Here it may be placed instead (p:place), given a kind (p.state.kind) or kept waiting \
            (p.state.out = true)." },
    Item { of: "", name: "on_player_leave", what: Callback { params: &[("p", "Player")], returns: "" },
        doc: "Runs when a player leaves the game: LEAVE in the pause menu, or its machine gone. It runs in that tick, first among the \
            events. It is out of play at once, its figure goes at the tick's end; p.state is kept for its seat, and whoever takes it again \
            comes back with it (on_player_join's \"back\")." },
    Item { of: "", name: "on_save_loaded", what: Callback { params: &[("version", "integer")], returns: "" },
        doc: "Runs whenever a save is loaded, to bring an older save up to date. It runs on world:load and LOAD in the pause menu, before \
            the save's level is entered, with the version of the game that made it (its c.saves.version then). Count c.saves.version up when \
            what a save holds changes (a key renamed), and bring older saves up to date here: if version < 2 then player.state.coins = \
            player.state.gold end. There is no level here yet." },
    Item { of: "", name: "on_tick", what: Callback { params: &[], returns: "" }, doc: "Runs 60 times a second: your game's rules go here. \
            input tells what the player presses. It runs after the ready-made rules, if the game uses some, and after this tick's events \
            (on_touch, on_hit …). math.random draws from the game's own random numbers, the same every time a run is played the same way \
            (replays and saves keep them)." },
    Item { of: "", name: "on_draw", what: Callback { params: &[("frame", "Frame")], returns: "" },
        doc: "Runs for every picture: draw text, models and lights on frame. Only drawing: change nothing here. It runs as often as the \
            screen shows a picture, apart from the 60 ticks a second; frame is this picture alone (frame:text, frame:cube, frame:model, \
            frame:light), drawn besides the level and the things that draw themselves. Everything else is only to look at here." },
    Item { of: "", name: "on_touch", what: Callback { params: &[("a", "Thing"), ("b", "Thing"), ("where", "string")], returns: "" },
        doc: "Runs when two things start touching: a stomp, a pickup, a door. It runs once, until they part (on_touch_end), not every tick. \
            What happens is your code's.\n- a: the player, always, else the thing that came first.\n- where: the face of b that a met. \
            \"top\": a came down onto b (landed on it, however fast). \"bottom\": a came up under b, or b came down onto a. \"side\": \
            anything else, and always for a touch within a reach.\nFor what lasts while touched (spikes, an enemy's contact) ask \
            thing:touching in on_tick. Things already touching where a level begins, or where thing:place puts one, count as touching: a \
            player arriving on a door does not go through it again. A kind with a reach (c.things.NAME.reach: a pickup, a helipad) is \
            touched within it, by anything (two with a reach: within the larger)." },
    Item { of: "", name: "on_touch_end", what: Callback { params: &[("a", "Thing"), ("b", "Thing")], returns: "" },
        doc: "Runs when two things that touched part: the end of a touch on_touch began, the player a again (not when one is taken away or \
            dies, nor when the level is left)." },
    Item { of: "", name: "on_interact", what: Callback { params: &[("thing", "Thing"), ("by", "Thing")], returns: "" },
        doc: "Runs when the player presses the action called interact (c.actions) while touching this thing (the nearest, if several): talk, \
            open, read. That press is used up: input:pressed(\"interact\") is false in this tick's on_tick." },
    Item { of: "", name: "on_hit", what: Callback { params: &[("thing", "Thing"), ("by", "Thing|nil"), ("damage", "integer"), ("weapon", "string|nil")], returns: "" },
        doc: "Runs when a thing is hurt; its hit points are already lowered. It is hurt by the ready-made rules, or by thing:hurt (told in \
            the next tick). by is who did it and weapon with what (a weapon of c.weapons, knife, what thing:hurt named), if known." },
    Item { of: "", name: "on_death", what: Callback { params: &[("thing", "Thing"), ("by", "Thing|nil"), ("weapon", "string|nil")], returns: "" },
        doc: "Runs when a thing's hit points run out, whoever lowered them (a kit's rules, or your script setting thing.state.hp to 0). \
            Once, until it lives again (hit points above 0). by and weapon as on_hit.\nIt is no longer listed, counted or touched; its kind \
            and where it was can still be read in this tick. What it means is your code's: points for it, a life less for the player (a kit \
            only shows the fall). Count here what happened (a gunship down): counting what is left cannot tell one that died from one that \
            has not come yet." },
    Item { of: "", name: "on_beat", what: Callback { params: &[("beat", "integer")], returns: "" },
        doc: "Runs on every beat of the song your game plays (music:play). beat counts from 0 at the song's start, on and on through its \
            loops (music.bar for the bar). Runs before on_tick, in the tick the beat begins: waves, shots, a light that pulse with the \
            music. Its clock is the game's, so replays play it the same; while the player's own song plays (none of yours), no beats." },
    Item { of: "", name: "on_fall_out", what: Callback { params: &[("thing", "Thing")], returns: "" },
        doc: "Runs when a thing falls out of the level (c.physics.lost_below voxels under it, 48 unless set, or down on the world's floor \
            below): once, until it is back in. Things other than the player are taken away after it; the player waits for what your code \
            decides (a life less, and back at the checkpoint: player:place, player.state.hp). Without it the player falls on." },
    Item { of: "", name: "on_pickup", what: Callback { params: &[("thing", "Thing"), ("item", "string")], returns: "" },
        doc: "Runs when a thing took an item of c.items with thing:take (told in the next tick). Its inventory already counts it." },
    // The engine's functions
    // world
    Item { of: "world", name: "state", what: Field { ty: "table", set: false }, doc: "Your game's data for the whole run, from its start to \
            its end. It holds numbers, strings, booleans and tables of those (each table once). No things: they live in their level (keep \
            them in level.state). Snapshots, replays, saves and the state hash see it." },
    Item { of: "world", name: "tick", what: Field { ty: "integer", set: false }, doc: "Ticks since the run began (60 a second; none while \
            the game holds still)." },
    Item { of: "world", name: "language", what: Field { ty: "string", set: false }, doc: "The language the player chose of the ones your \
            game speaks (c.languages = { \"de\", \"en\", \"fr\" }; SETTINGS offers them, the system's at first): \"de\", two small letters; \
            without c.languages \"en\". It can change while playing: pick your texts by it every picture (texts[world.language].hello). \
            Replays carry it. Over the network each machine draws in its own: in on_draw it is the language of the machine drawing, in \
            on_tick and the events the host's, so keep what a text says in a state and choose its words in on_draw." },
    Item { of: "world", name: "levels", what: Field { ty: "string[]", set: false }, doc: "The levels' names in order: the files in levels/ \
            sorted by name, numbers as numbers (01_hall, 02_tower); one level when there are none." },
    Item { of: "world", name: "leaving", what: Field { ty: "string|nil", set: false }, doc: "Why the level is about to be left at the end of \
            this tick (\"go_to\", \"restart\", \"load\", \"won\", \"lost\"), or nil: the ask that counts so far. One ask counts in a tick: \
            going on (go_to, win) before starting over (restart, load, lose); among those of one rank the first asked. (Leaving by QUIT TO \
            TITLE or a level file saved while playing is no ask: on_level_end hears \"quit\" and \"reload\".)" },
    Item { of: "world", name: "saves", what: Field { ty: "table<integer, SaveInfo>", set: false },
        doc: "What lies in each save slot (c.saves.slots of them): world.saves[1] is nil while slot 1 is empty, else { level, seconds }." },
    Item { of: "world", name: "players", what: Field { ty: "(Player|false)[]", set: false }, doc: "Every seat of the game (c.players, 1 \
            unless set), in order: world.players[n] is the player on seat n, or false while nobody sits there (#world.players is how many \
            seats; world.players[1] is player). A HUD shows PRESS START where a seat is false. Players in play in this level: \
            level:players()." },
    Item { of: "world", name: "items", what: Field { ty: "Item[]", set: false }, doc: "The kinds of item (c.items) in their order \
            (c.items.NAME.order, then by name), each { name, model, stack }: for drawing an inventory." },
    Item { of: "world", name: "go_to", what: Method { params: &[("level", "string|integer"), ("place?", "string")], returns: "" },
        doc: "Leaves this level for another at the end of the tick (the code after it still runs): by name or number \
            (world:go_to(level.next) goes on in order), arriving at the place called place, else where the level starts. \
            player.state.checkpoint is forgotten. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "restart", what: Method { params: &[("place?", "string")], returns: "" },
        doc: "Enters this level afresh at the end of the tick: everything from its file again, level.state fresh; the player at place, else \
            at the place player.state.checkpoint names, else where it starts. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "read", what: Method { params: &[("name", "string")], returns: "string" },
        doc: "The text of a file in the game folder, by its path there (world:read(\"levels/microban.txt\")): only while main.lua starts: at \
            its top, in on_world_start or the first on_level_start. The files read count for the replays, and saving one runs main.lua anew." },
    Item { of: "world", name: "save", what: Method { params: &[("slot", "integer")], returns: "" },
        doc: "Saves the game into a slot (1 to c.saves.slots) at the end of the tick, or of the on_level_start that asks: for save points, \
            or saving on its own. The player can save from the pause menu too.\n- Kept: world.state, player.state, every level's level.kept, \
            the level being played with its level.state and the voxels changed in it (blown away, built), and where the player stands.\n- \
            Not kept: the level's things. They come from its file when it is loaded; a thing kept in level.state comes back as nil, so keep \
            what you need to place them again." },
    Item { of: "world", name: "load", what: Method { params: &[("slot", "integer")], returns: "" },
        doc: "Goes on from the save in a slot at the end of the tick, like a level change (on_level_end(\"load\"), then \
            on_level_start(\"load\")): the states as they were saved, the level from its file with the voxels changed in it as they were, \
            the player where it stood. An empty slot is a mistake: world.saves tells first. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "hitstop", what: Method { params: &[("ticks", "integer")], returns: "" },
        doc: "Holds the game still for this many ticks: a hit 2 or 3. Everything stops, on_tick too. 60 (a second) at most: more is cut to \
            60. It is part of the game, so replays see it." },
    Item { of: "world", name: "win", what: Method { params: &[], returns: "" }, doc: "The run is won at the end of the tick (on_level_end, \
            then on_world_end); after that the script hears nothing more. One ask counts in a tick: world.leaving." },
    Item { of: "world", name: "lose", what: Method { params: &[], returns: "" }, doc: "The run is lost at the end of the tick (on_level_end, \
            then on_world_end); after that the script hears nothing more. One ask counts in a tick: a win or a go_to in the same tick counts \
            before it (world.leaving)." },
    // level
    Item { of: "level", name: "state", what: Field { ty: "table", set: false }, doc: "Your game's data for this level while it is played: \
            the things in it too, moves, time. Fresh each time the level is entered, a restart too; after world:load as it was saved (its \
            things nil). The engine reads one key: level.state.held = true holds the level (the ready-made rules, the physics and touches \
            wait; on_tick, on_draw and the time go on): for a message, a shop, a tally." },
    Item { of: "level", name: "name", what: Field { ty: "string", set: false }, doc: "Its name: its file's name without the ending (hall for \
            levels/hall.toml); main for a game's one level made in code." },
    Item { of: "level", name: "number", what: Field { ty: "integer", set: false }, doc: "Its place among world.levels, from 1." },
    Item { of: "level", name: "next", what: Field { ty: "string|nil", set: false }, doc: "The next level's name, nil after the last one." },
    Item { of: "level", name: "visits", what: Field { ty: "integer", set: false }, doc: "How often it was entered in this run, restarts too: \
            1 the first time." },
    Item { of: "level", name: "kept", what: Field { ty: "table", set: false }, doc: "What this level keeps across visits for the whole run \
            (a lever pulled, a puzzle solved): read it in on_level_start. Plain data, no things." },
    Item { of: "level", name: "arrived", what: Field { ty: "Place", set: false }, doc: "Where the player arrived in this level, with its \
            name (a place's, or start): a game without a figure of the engine's stands its hero there in on_level_start." },
    Item { of: "level", name: "places", what: Field { ty: "table<string, Place>", set: false },
        doc: "Its named places, set in its file (level.places.west): where a player arrives; start is always there (the place called start, \
            else where the ready-made rules stand the player, else c.player.spawn)." },
    Item { of: "level", name: "areas", what: Field { ty: "table<string, Area>", set: false }, doc: "Its named areas, boxes set in its file \
            ([[area]] with name, at and size): level.areas.by_the_stove:holds(player) says whether the player stands in it. Only to read; a \
            level without areas has an empty table." },
    Item { of: "level", name: "size", what: Field { ty: "Point", set: false }, doc: "The level's size in voxels." },
    Item { of: "level", name: "camera", what: Field { ty: "Camera", set: false }, doc: "Where the camera looks: level.camera:follow(thing), \
            level.camera:look_at(x, y)." },
    Item { of: "level", name: "voxel", what: Method { params: &[("x", "integer"), ("y", "integer"), ("z", "integer")], returns: "integer" },
        doc: "The voxel at a cell: its palette index, 0 empty (a solid piece's cells too: its model draws them, level:solid says they \
            block)." },
    Item { of: "level", name: "set_voxel", what: Method { params: &[("x", "integer"), ("y", "integer"), ("z", "integer"), ("voxel", "integer|string|nil")], returns: "" },
        doc: "Sets the voxel at a cell: a color \"#rrggbb\" (taken into the level's palette), a palette index, or nil or 0 for empty. A cell \
            outside the level (level.size) is a mistake." },
    Item { of: "level", name: "fill", what: Method { params: &[("x0", "integer"), ("y0", "integer"), ("z0", "integer"), ("x1", "integer"), ("y1", "integer"), ("z1", "integer"), ("voxel", "integer|string|nil")], returns: "" },
        doc: "Fills the box from one corner cell to the other, both included, like set_voxel (only what of it lies inside the level); nil or \
            0 empties it. A level made in code (c.world.size) is built this way." },
    Item { of: "level", name: "solid", what: Method { params: &[("x", "integer"), ("y", "integer"), ("z", "integer")], returns: "boolean" },
        doc: "Whether the cell stops bodies: a voxel there (soft voxels, grass and shrubs, do not), or a solid piece's (solid = true in the \
            level's file)." },
    Item { of: "level", name: "ground", what: Method { params: &[("x", "integer"), ("y", "integer")], returns: "integer" },
        doc: "The height of the ground at x, y: the cell above the column's top voxel." },
    Item { of: "level", name: "spawn", what: Method { params: &[("kind", "string"), ("x", "number"), ("y", "number"), ("options?", "SpawnOptions")], returns: "Thing" },
        doc: "Puts a new thing of a kind (c.things) into the level at x, y, on the ground there unless options.z is given. x and y lie \
            inside the level (level.size); only a kind with fixed = true may wait outside, one coming in from off the screen.\n- options.z: \
            the height of its feet. options.facing: the way it faces.\n- options.state: its first state. options.name: its name, one thing's \
            in the level.\n- options.props: what your code reads of it over its kind's props.\nA kind without a behavior falls to the \
            ground; with fixed = true (c.things.NAME.fixed) it stays exactly where it is put. A kit's kinds stand where a marker there would \
            place them. The camera follows the player; in a game without a figure for it (no c.player.kind), the first thing placed. Pieces \
            (\"piece\") come only from a level's file." },
    Item { of: "level", name: "thing", what: Method { params: &[("name", "string|integer|Thing")], returns: "Thing|nil" },
        doc: "The thing called name (thing.name: the name its level's file gives it, \"lamp\", else door_1), or with this id, if it is still \
            there." },
    Item { of: "level", name: "things", what: Method { params: &[("kind?", "string")], returns: "Thing[]" },
        doc: "Every thing (of a kind, or of a family: c.things.NAME.family), in the order they came: not the player (it belongs to no \
            level), not the dead, not what was taken away this tick. level:things(\"piece\") is every model and light the level's file set \
            down." },
    Item { of: "level", name: "players", what: Method { params: &[], returns: "Player[]" }, doc: "The players playing in this level, in seat \
            order: someone sits there, not out of play (state.out), and its figure stands (one taking a seat comes in at the tick's end). \
            Loop over it for every player's rules: for _, p in ipairs(level:players()) do p:walk(p.input.dir, 40) end. world.players is \
            every seat, the out too (a HUD, a continue)." },
    Item { of: "level", name: "count", what: Method { params: &[("kind?", "string")], returns: "integer" },
        doc: "How many things (of a kind or family) there are: not the player, not the dead, not what was taken away this tick (count deaths \
            in on_death)." },
    Item { of: "level", name: "block", what: Method { params: &[("x", "number"), ("y", "number"), ("z", "number")], returns: "string|nil" },
        doc: "The kind of the level's block at a voxel: \"solid\", \"oneway\", \"ladder\", \"hazard\", or nil (nothing, decoration, a level \
            not made of blocks). What a block does is your code's: a hazard hurts because you say so. For a thing's whole body: \
            thing:in_block." },
    Item { of: "level", name: "effect", what: Method { params: &[("model", "string"), ("at", "Thing|Point"), ("fps?", "number")], returns: "" },
        doc: "Plays a model's frames once (a puff, a sparkle), its feet where a thing stands or at a point, fps frames a second (12). Only \
            shown: not part of the game." },
    Item { of: "level", name: "explode", what: Method { params: &[("at", "Thing|Point"), ("radius", "number")], returns: "" },
        doc: "Blows a hole: voxels fly as debris, pieces cut off fall." },
    Item { of: "level", name: "clear_rounds", what: Method { params: &[("team?", "string")], returns: "" },
        doc: "Takes every round in flight away, grenades and rockets too before they go off; only those fired by a team if you name one \
            (\"enemies\", \"player\"). A bomb that clears the screen, or mercy for a player coming back." },
    Item { of: "level", name: "sound", what: Method { params: &[("name", "string"), ("at", "Thing|Point"), ("volume?", "number")], returns: "" },
        doc: "Plays a sound at a place, by its name: a file of the game, sounds/NAME.toml, or one the engine or the ready-made rules bring." },
    // level.camera
    Item { of: "level.camera", name: "follow", what: Method { params: &[("thing", "Thing")], returns: "" },
        doc: "The camera follows this thing from now on; following the player, ready-made rules lead the view as theirs do (the jump & run's \
            look ahead). Every level starts following the player." },
    Item { of: "level.camera", name: "look_at", what: Method { params: &[("x", "number"), ("y", "number")], returns: "" },
        doc: "The camera rests over this point from now on, following nothing, whatever spawns (follow follows a thing again): a room, a \
            boss arena, a scene, a field that scrolls by itself." },
    // player
    Item { of: "player", name: "state", what: Field { ty: "table", set: false }, doc: "The player's data for the whole run, from level to \
            level. It starts as c.state.player says. Plain data, no things; lives and the like are your game's own keys. A script reads and \
            sets the engine's keys like its own.\n- score: a whole number.\n- inventory: items by name and how many \
            (player.state.inventory.key).\n- checkpoint: the name of a place or a thing in this level where world:restart goes. Forgotten on \
            world:go_to.\n- hp: with a figure that has hit points, or with a kit.\nWith a kit (c.kit):\n- invulnerable: ticks left without \
            hits, or true until set back.\nWith the jump & run kit (c.kit = \"platformer\"):\n- pose: the hero's set of frames, as a thing's \
            thing.state.pose.\nWith the shooter kit (c.kit = \"shooter\"):\n- weapon: the gun in hand, by its name in c.player.weapons.\n- \
            inventory also holds the rounds of its guns and its grenades, by their names.\n- hp set above 0 brings a player who is down back \
            to life." },
    // thing
    Item { of: "thing", name: "id", what: Field { ty: "integer|nil", set: false }, doc: "Its number, never given to another thing in the \
            same run; nil for the player, who belongs to no level." },
    Item { of: "thing", name: "kind", what: Field { ty: "string|nil", set: false }, doc: "The name of its kind (c.things); \"piece\" for a \
            model or a light the level's file set down (it stays exactly where it is put, as a kind with fixed = true); nil for a player of \
            no kind (the one the ready-made shooter rules make from c.player)." },
    Item { of: "thing", name: "state", what: Field { ty: "table", set: false }, doc: "This thing's data while it lives: your game's own \
            keys, and some the engine keeps. It goes when the thing goes. A script reads and sets the engine's keys like its own.\n- hp: the \
            hits it still takes, where its kind has hp (c.things.NAME.hp at the start). At 0 on_death runs.\n- inventory: items by name and \
            how many (a chest's, a trader's).\nWith the jump & run kit (c.kit = \"platformer\"):\n- pose: which set of its frames it shows \
            (c.things.NAME.frames), from the tick it is set. A prop, a door or a sign always shows one (\"open\"; \"idle\" the first). \
            Another kind shows it over its own animation (\"attack\"), nil for its own again. The hero's is player.state.pose.\n- goal: for \
            a platform, \"end\" or \"start\": it goes to that end of its travel and stays (a trapdoor, a lift). nil: to and fro again.\nWith \
            the shooter kit (c.kit = \"shooter\"):\n- freed: for a hostage. Set true: the cage bursts, it runs off.\n- flying: for a \
            gunship, up in the air fighting. Only to read.\nFor a piece of a level's file:\n- light: a lamp's brightness, all its lights \
            together, 0 (off) to 1 (the start). Their colors, places and reach are the file's.\n- parts: a figure of parts posed, as \
            frame:model's pose: { leaf = { rotation = 90 } } swings a door's leaf open; nil at rest. Drawn so every picture, for this visit \
            of the level: keep what must stay open in world.state and set it again in on_level_start. A solid piece still blocks where its \
            model stands at rest." },
    Item { of: "thing", name: "x", what: Field { ty: "number", set: false }, doc: "Where it stands: the middle of its feet. A piece's is the \
            middle of the cell its file's at names (at + 0.5): a cube for it goes at math.floor(thing.x) + dx." },
    Item { of: "thing", name: "y", what: Field { ty: "number", set: false }, doc: "Where it stands: the middle of its feet. A piece's is the \
            middle of the cell its file's at names (at + 0.5): a cube for it goes at math.floor(thing.x) + dx." },
    Item { of: "thing", name: "z", what: Field { ty: "number", set: false }, doc: "The height of its feet." },
    Item { of: "thing", name: "on_ground", what: Field { ty: "boolean", set: false }, doc: "Whether it stands on something." },
    Item { of: "thing", name: "wall", what: Field { ty: "Way|nil", set: false }, doc: "The way to a wall right beside it (within half a \
            voxel), higher than a step (the way it faces first), or nil: a wall jump, a wall slide." },
    Item { of: "thing", name: "facing", what: Field { ty: "Way", set: false }, doc: "The way it faces, one of the 8 ways." },
    Item { of: "thing", name: "seat", what: Field { ty: "integer|nil", set: false }, doc: "A player's seat (1 for player, 2 and on for the \
            others); nil for a level's thing, so if thing.seat then asks whether a thing is a player, in any handler (a ~= player leaves out \
            seats 2 and on)." },
    Item { of: "thing", name: "input", what: Field { ty: "Input", set: false }, doc: "A player's keys this tick, as input is seat 1's: \
            p.input.dir, p.input:pressed(\"fire\"). Only in on_tick, as input." },
    Item { of: "thing", name: "controls", what: Field { ty: "Controls", set: false }, doc: "What a player plays with, as controls is seat \
            1's: p.controls.device, p.controls:key(\"fire\") (a key in braces in a text is the first of frame.players's)." },
    Item { of: "thing", name: "walk", what: Method { params: &[("way", "Way"), ("speed", "number")], returns: "" },
        doc: "Walks this tick the way given at this speed (voxels a second), facing it. Only the way's direction counts, not its length: { x \
            = 0.3, y = 7 } walks at the full speed. It falls, steps up ledges and lands by itself. Only things your script moves (a kind \
            with no behavior in c.things, or one whose behavior thing:enable switched off)." },
    Item { of: "thing", name: "face", what: Method { params: &[("way", "Way")], returns: "" },
        doc: "Turns it to the nearest of the 8 ways: things your script moves, and the player with ready-made rules (it turns again as it \
            moves; the jump & run's hero only left or right)." },
    Item { of: "thing", name: "jump", what: Method { params: &[("height", "number")], returns: "" },
        doc: "Jumps this high (voxels) if it stands on something. Only things your script moves." },
    Item { of: "thing", name: "ahead", what: Method { params: &[("distance", "number")], returns: "Point" },
        doc: "The point this far ahead of its feet, a voxel up." },
    Item { of: "thing", name: "near", what: Method { params: &[("other", "Thing"), ("distance", "number")], returns: "boolean" },
        doc: "Whether the other thing stands closer than this." },
    Item { of: "thing", name: "place", what: Method { params: &[("x", "number|Place|Thing"), ("y?", "number"), ("z?", "number")], returns: "" },
        doc: "Puts it exactly there, standing still: the end of a slide on a grid, a respawn. For things your script moves, and the \
            player.\n- x, y, z: on the ground there unless z is given (a kind with fixed = true stays wherever it is put).\n- a place \
            (level.places.west): it stands there, facing its way if it has one.\n- a thing: where that thing stands (a checkpoint).\nA kit \
            stands its player there still and on its feet, facing the way it did (thing:face turns it)." },
    Item { of: "thing", name: "attack", what: Method { params: &[("weapon", "string"), ("target?", "Thing|Point")], returns: "" },
        doc: "It attacks with a weapon of c.weapons, with the shooter kit (c.kit = \"shooter\"). It starts at the tick's end, then waits the \
            weapon's cooldown before the next.\n- A blow: at the target, or the first in reach in front.\n- A shot: at the target, or the \
            way it faces (the player's finds a target with the aim assist).\n- A throw: to the target, or its full throw ahead.\nThe player \
            spends the weapon's rounds from player.state.inventory and attacks only while it has them. An action named in c.player.attacks \
            attacks by itself; use another action for attacks your code decides (a charge shot, a combo)." },
    Item { of: "thing", name: "hurt", what: Method { params: &[("damage", "integer"), ("by?", "Thing"), ("weapon?", "string")], returns: "" },
        doc: "Hurts it, by a thing and with a weapon if given. Anything with hit points loses them (thing.state.hp, player.state.hp); at 0 \
            on_death runs. A kit's own take it as their hits: a soldier falls, a gunship spins down, the player is thrown back.\nA kind's \
            team and immune do not stop it: your code decides whom it hurts. While it may not be hurt (player.state.invulnerable) nothing \
            happens. on_hit runs in the next tick's events; a thing it kills is out of the level's lists and counts at once." },
    Item { of: "thing", name: "touching", what: Method { params: &[("kind?", "string")], returns: "Thing[]" },
        doc: "The things it touches now (of a kind or family), in the order they came: for what lasts while touched (spikes, an enemy's \
            contact, lava), asked in on_tick, where it stays true every tick. Not the dead, nor what was taken away this tick." },
    Item { of: "thing", name: "in_block", what: Method { params: &[("kind", "string")], returns: "boolean" },
        doc: "Whether any part of its body is in a block of the level of this kind: \"hazard\", \"ladder\", \"oneway\" or \"solid\" \
            (level:block for one voxel)." },
    Item { of: "thing", name: "bounce", what: Method { params: &[("height", "number")], returns: "" },
        doc: "Sends it up this high (voxels), standing or in the air: off an enemy it landed on, a spring. Always the full height; the jump \
            button plays no part. Things your script moves, and the jump & run's hero (c.kit = \"platformer\")." },
    Item { of: "thing", name: "launch", what: Method { params: &[("way", "Way"), ("speed", "number"), ("rise?", "number")], returns: "" },
        doc: "Sets it moving the way given at this speed (voxels a second), standing or in the air: a dash (player:launch(player.facing, \
            240)), a wall jump, a knockback. Only the way's direction counts, not its length.\n- rise: how high it goes up too, in \
            voxels.\nPhysics carries it on, and a kit's rules steer it on from there (the hero as the player presses, a soldier as he means \
            to go): thing:enable switches them off for a while. For things your script moves, things whose behavior is off, the player, the \
            hero, walkers, hoppers, soldiers and hostages." },
    Item { of: "thing", name: "enable", what: Method { params: &[("behavior", "string"), ("on", "boolean")], returns: "" },
        doc: "Switches its ready-made behavior (its kind's, c.things.NAME.behavior: \"hero\", \"walker\", \"soldier\", ...) off or on again. \
            Off, it does nothing by itself: it still falls, is hit and touched, and your script moves it like its own (thing:walk, \
            thing:jump, thing:launch). A stunned hero, an end walk, a frozen enemy. Until it leaves its level (the hero: until the next \
            level). Not yet the shooter's player, who is no thing of a kind." },
    Item { of: "thing", name: "take", what: Method { params: &[("item", "string"), ("count?", "integer")], returns: "integer" },
        doc: "It takes an item of c.items (one, or count): into its state.inventory (the player's: player.state.inventory), no more than the \
            item's stack; how many it took. on_pickup runs in the next tick's events." },
    Item { of: "thing", name: "remove", what: Method { params: &[], returns: "boolean" }, doc: "Takes it out of the level at the end of this \
            tick: it can still be read until then, but it is out of the level's lists, counts and touches at once. Whether it was there to \
            remove. Not the player: it goes from level to level." },
    Item { of: "thing", name: "name", what: Field { ty: "string|nil", set: false }, doc: "Its name in this level: the name the level's file \
            gives it (name = \"lamp\"), else a thing placed by the level file is called its kind and its number among them (door_1, door_2 \
            in the file's order), a spawned one what level:spawn named it. nil for the player and for things the ready-made rules bring \
            later. player.state.checkpoint and level:thing take it." },
    Item { of: "thing", name: "families", what: Field { ty: "table<string, boolean>", set: false },
        doc: "The families of its kind (c.things.NAME.family = { \"enemies\" }), as a set: if b.families.enemies then ... end. Only to read." },
    Item { of: "thing", name: "props", what: Field { ty: "table", set: false }, doc: "Fixed values your code reads of this thing: its kind's \
            props (c.things.NAME.props = { worth = 100 }) and what level:spawn gave it. Only to read; what changes goes in thing.state." },
    // input
    Item { of: "input", name: "left", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"left\") whether it \
            went down this tick)." },
    Item { of: "input", name: "right", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"right\") whether it \
            went down this tick)." },
    Item { of: "input", name: "up", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"up\") whether it went down this tick)." },
    Item { of: "input", name: "down", what: Field { ty: "boolean", set: false }, doc: "Held this tick (input:pressed(\"down\") whether it \
            went down this tick)." },
    Item { of: "input", name: "dir", what: Field { ty: "Way", set: false }, doc: "The directions held as a way: x and y each -1, 0 or 1." },
    Item { of: "input", name: "held", what: Method { params: &[("action", "string")], returns: "boolean" },
        doc: "Whether an action (c.actions, one the ready-made rules bring, or a direction: \"left\", \"right\", \"up\", \"down\") is held \
            this tick. The ready-made rules read the same keys and use none of them up." },
    Item { of: "input", name: "pressed", what: Method { params: &[("action", "string")], returns: "boolean" },
        doc: "Whether an action or a direction went down this tick: one step a press on a grid, one line in a menu. A press of interact that \
            on_interact took is used up." },
    Item { of: "input", name: "released", what: Method { params: &[("action", "string")], returns: "boolean" },
        doc: "Whether an action or a direction went up this tick." },
    Item { of: "input", name: "repeated", what: Method { params: &[("action", "string")], returns: "boolean" },
        doc: "True as the key goes down, and again and again while it is held (after a third of a second, ten times a second), as a menu's \
            cursor moves on a held key: for menus, lists and stepping on a grid. Directions too: input:repeated(\"down\")." },
    // music
    Item { of: "music", name: "play", what: Method { params: &[("song?", "string"), ("from?", "string|MusicPlay")], returns: "" },
        doc: "Plays a song of c.audio.music by its file's name (\"storm\" for music/storm.mml) as your game's own. Your game's song wins \
            over the player's choice.\n- from: when it begins: \"bar\" (the next bar of the one playing, unless said), \"beat\" or \"now\"; \
            at once when none of yours plays.\n- crossfade: { from = \"now\", crossfade = 3 } fades the old song out as the new comes in \
            over 3 seconds, as loud together as either alone. c.audio.crossfade sets it for every change; else the old ends as the new \
            begins.\nIt starts at its beat 0 and keeps time with the game (music.beat and on_beat count its beats from then), held while the \
            game pauses. Asked for while it plays, it goes on, through a level change too. Without a song: the player's own again, from the \
            next bar." },
    Item { of: "music", name: "volume", what: Method { params: &[("level?", "number"), ("seconds?", "number")], returns: "number|nil" },
        doc: "How loud your game's music sounds, as a part of the song's own loudness: from 0 (silent) to 1 (as loud as the song is, the \
            start), reached smoothly over seconds (0.3 unless you say; 0 at once, still without a click). The player's music volume stays on \
            top. It holds until you change it, from level to level too, whichever song plays: music:volume(0.25, 2) fades it to a quarter in \
            two seconds. Without a level it gives the level you asked for last. Only heard: replays and the game itself do not change with \
            it." },
    Item { of: "music", name: "muffle", what: Method { params: &[("amount?", "number"), ("seconds?", "number")], returns: "number|nil" },
        doc: "How dull your game's music sounds, as heard through a wall or a door: from 0 (clear, the start) to 1 (very dull, only the low \
            notes), reached smoothly over seconds (0.3 unless you say; 0 at once, still without a click). Your game's sounds stay clear. It \
            holds until you change it, from level to level too, whichever song plays: music:muffle(0.6, 1) outside a house, music:muffle(0, \
            1) inside. Without an amount it gives the amount you asked for last. Only heard: replays and the game itself do not change with \
            it." },
    Item { of: "music", name: "song", what: Field { ty: "string|nil", set: false }, doc: "The song your game plays (music:play), by its \
            file's name; nil while the player's own plays." },
    Item { of: "music", name: "beat", what: Field { ty: "integer|nil", set: false }, doc: "Beats since your game's song began, from 0 (its \
            #tempo); nil while the player's own song plays." },
    Item { of: "music", name: "intensity", what: Field { ty: "number", set: true }, doc: "How tense the game is, from 0 (calm) to 1 (all of \
            the song; 1 unless you set it): the layers of your game's song (#layer in the song: drums from 0.35, the melody from 0.8) come \
            in above their tension and go below it, each over a bar. Set it in on_tick from your states: enemies on the screen, a boss, a \
            hit." },
    Item { of: "music", name: "bar", what: Field { ty: "integer|nil", set: false }, doc: "Bars since your game's song began, from 0 (its \
            #meter beats to a bar, 4 unless the song says); nil while the player's own song plays." },
    // screen
    Item { of: "screen", name: "shake", what: Method { params: &[("amount", "number")], returns: "" },
        doc: "Shakes the screen: 0..1 more (a small hit 0.1, a blast 0.35, 1 at most); it dies down by itself. Only the camera moves, and \
            players can turn shaking off." },
    Item { of: "screen", name: "burst", what: Method { params: &[("icon", "string"), ("x", "number"), ("y", "number")], returns: "" },
        doc: "Bursts an icon of c.icons on the screen at x, y (its top left): its voxels fly off and fall, a heart lost, a grenade thrown. \
            Only shown, not part of the game." },
    Item { of: "screen", name: "height", what: Field { ty: "number", set: false }, doc: "How many UI voxels the HUD is from top to bottom \
            (c.ui.height), for a burst near its bottom. Its width follows the shape of each player's window, so only on_draw's frame.width \
            says it." },
    Item { of: "screen", name: "flash", what: Method { params: &[("strength", "number")], returns: "" },
        doc: "Flashes the screen white, 0..1 (times c.effects.screen_flash); it fades by itself. Players can turn flashes off." },
    Item { of: "frame", name: "model", what: Method { params: &[("file", "string"), ("thing", "Thing"), ("index?", "integer"), ("rotation?", "Rotation"), ("pose?", "table<string, Rotation>")], returns: "" },
        doc: "Draws a model where a thing stands, facing its way. file is any model file of the game, by its path in the game folder \
            (\"sheet.vvx\", \"props/sheet.vvx\"): loaded the first time it is drawn, let go when the level is left; a file that is not there \
            is a mistake. The thing is a thing, never a point.\n- index: which of its frames, from 0. Without it, its kind's frames for what \
            it does (c.things.NAME.frames: idle, walk, jump), else the model's own: 0 standing, 1 in the air, the rest walking, a frame \
            every half its height walked.\n- rotation: turns it further, in degrees about its middle (the voxel at half its height over its \
            pivot). { roll = 20 } banks a ship right, { rotation = 45 } faces a diagonal, { pitch = 10 } lifts its nose. Roll is taken \
            first, then pitch, then rotation, as for a level's piece. Off the quarter turns its voxels leave the grid.\n- pose: moves a \
            figure of parts (a model whose parts have names: head, body, arm_left ...), each part by its name, a rotation about its joint, \
            what hangs on it moving along. { head = { rotation = 30 }, arm_right = { pitch = 90 } } raises the right arm forward. offset = { \
            x, y, z } moves a part by voxels (fractions too) after its rotation: { bust = { pitch = 10, offset = { 0, 1, 0 } } }. It goes \
            over its kind's swing (c.things.NAME.swing).\nA kind with rotates = \"free\" faces exactly its way (a diagonal too), else the \
            nearest quarter turn. A model's pivot is the middle cell of its size across and deep: of an even size one voxel off the middle, \
            so facing -y or sideways it fills cells a voxel further on the flipped axis (use odd sizes where that matters)." },
    Item { of: "frame", name: "cube", what: Method { params: &[("x", "number"), ("y", "number"), ("z", "number"), ("color?", "Color"), ("glow?", "number")], returns: "" },
        doc: "Draws one voxel in the level, at x, y, z in the level's voxels (alpha below 255: see-through; glow from 0 to 1)." },
    Item { of: "frame", name: "light", what: Method { params: &[("at", "Thing|Point"), ("radius", "number"), ("color?", "Color")], returns: "" },
        doc: "Lights the level round a thing or a point, in this picture." },
    Item { of: "frame", name: "daylight", what: Method { params: &[("light", "Daylight")], returns: "" },
        doc: "Sets the sun and the sky for this picture: a day and a night. What you leave out is c.light's.\n- sun: the way its light \
            travels, { x, y, z } with z below 0.\n- sun_color: its color and strength, white unless set.\n- ambient: the light every face \
            gets, 0 to 1. shadow: how dark shadows are, 0 to 1.\n- sky: the sky's color.\nWork it out from your game's clock every picture: \
            the sun low and warm at dawn, white at noon, orange at dusk, a dim blue at night with frame:light lamps. Only the picture: the \
            game's rules do not see it. A game whose code names daylight has every face a sun above the horizon can light in its level." },
    Item { of: "frame", name: "camera", what: Method { params: &[("camera", "PictureCamera")], returns: "" },
        doc: "Moves the camera for this picture, for a scene: a zoom, a cut, a slow pan. What you leave out is the game's camera's \
            (c.camera): { width = 80 } zooms in on what the camera follows and keeps following it.\n- look_at: the point the view centers \
            on, { x, y, z } in voxels (z left out: the camera's height), or a thing.\n- width: how many voxels the picture shows across \
            there; or height, from bottom to top.\n- pitch: degrees below the horizon. fov: the field of view.\nOnly this picture: nothing \
            is kept and nothing smoothed. Call it in every picture the scene lasts and work a pan out from your clock (world.tick + \
            frame.alpha, music.beat). The game's rules, the sounds and level.camera do not see it; frame:to_screen after it goes by it. The \
            camera does not turn round yet. Another pitch or fov than c.camera's needs faces the game's own view never shows: the level is \
            built anew with them once, a moment's hitch the first time." },
    Item { of: "frame", name: "menu", what: Method { params: &[("menu", "Menu")], returns: "" },
        doc: "Draws a menu in the engine's look, on a plate. In the middle of the screen unless x and y say where (its top left), as wide as \
            it needs unless width says.\n- rows: each a text, or { text =, value =, icon =, dim = }: a value at the right, an icon of \
            c.icons before the text, dimmed when it cannot be had.\n- at: the chosen row, from 1, yellow with the engine's red marker.\n- \
            title, tabs (tab: the chosen one, from 1), hint below; an action in braces is its key.\n- shown: how many rows show at most; a \
            longer list scrolls to keep at in view.\nWhat is open and where the cursor stands is your game's: keep it in level.state, hold \
            the level while it is open (level.state.held = true) and move it in on_tick (input:repeated)." },
    Item { of: "frame", name: "grid", what: Method { params: &[("grid", "Grid")], returns: "" },
        doc: "Draws an inventory's slots in rows, on a plate: cells, each { icon =, count =, dim = } (an icon of c.icons, a number at its \
            corner, darker when dim) or false for an empty slot; columns across (up to 8 unless said), cell its size in UI voxels (18), at \
            the chosen slot (from 1) edged in yellow, an optional title. In the middle unless x and y say where." },
    Item { of: "frame", name: "text", what: Method { params: &[("text", "string"), ("x", "number"), ("y", "number"), ("color?", "Color"), ("size?", "integer")], returns: "" },
        doc: "Writes text on the screen, over the level. x and y are its top left, in UI voxels from the screen's top left (frame.width \
            across, frame.height down).\n- size: 1 the small letters (7 voxels high), 2 to 4 the big ones (c.ui.display_font).\n- An action \
            in braces is its key as the player sees it: \"PRESS {jump}\" writes PRESS X, or the pad's button (controls:key). {{ writes a \
            brace.\nWritten as you write it: lower case, the letters of German and French (ä ö ü ß é è ê à ç œ …) and signs such as · × → ▲ \
            ▼ ✓ … „ « ». A display font has capitals only; a key's name is in capitals." },
    Item { of: "frame", name: "players", what: Field { ty: "Player[]", set: false }, doc: "The players at the machine this picture is drawn \
            for, in seat order (at one machine every taken seat, the out too; over the network that machine's own): their HUD. A key in \
            braces in a text, and controls in on_draw, are the first of them's (seat 1's where none plays there); another player's is \
            p.controls:key." },
    Item { of: "frame", name: "width", what: Field { ty: "number", set: false }, doc: "How many UI voxels the HUD is across: it follows the \
            screen's shape, so put things at the right edge from it (frame.width - 3 - frame:text_width(score))." },
    Item { of: "frame", name: "height", what: Field { ty: "number", set: false }, doc: "How many UI voxels the HUD is from top to bottom (c.ui.height)." },
    Item { of: "frame", name: "alpha", what: Field { ty: "number", set: false }, doc: "How far into the tick this picture is, 0 to 1: add it \
            to a count of ticks for motion that runs smooth between them." },
    Item { of: "frame", name: "to_screen", what: Method { params: &[("at", "Thing|Point"), ("up?", "number")], returns: "number?, number?" },
        doc: "Where a point of the level shows in this picture, in the HUD's measure (x and y as frame:text takes them, from the top left), \
            or nothing when it lies behind the camera. A thing's point is where its feet are drawn, gliding as it does; up lifts it that \
            many voxels (over a figure's head). For names, health bars and numbers rising from where something happened." },
    Item { of: "frame", name: "text_width", what: Method { params: &[("text", "string"), ("size?", "integer")], returns: "number" },
        doc: "How many UI voxels wide the text is at that size (for centring and putting it at the right edge)." },
    Item { of: "frame", name: "box", what: Method { params: &[("x", "number"), ("y", "number"), ("width", "number"), ("height", "number"), ("color?", "Color")], returns: "" },
        doc: "Draws a filled box on the screen, its top left at x, y." },
    Item { of: "frame", name: "bar", what: Method { params: &[("x", "number"), ("y", "number"), ("width", "number"), ("height", "number"), ("fraction", "number"), ("color?", "Color"), ("back?", "Color")], returns: "" },
        doc: "Draws a bar on the screen filled the fraction (0 to 1) of its width: a boss's health, a timer. color the filled part (red \
            unless given), back the rest." },
    Item { of: "frame", name: "icon", what: Method { params: &[("name", "string"), ("x", "number"), ("y", "number")], returns: "" },
        doc: "Draws a small picture of c.icons on the screen, its top left at x, y: c.icons.heart = { rows = { \".RR.RR.\", ... }, colors = \
            { R = { 214, 36, 44 } } }." },
    Item { of: "frame", name: "drop_in", what: Method { params: &[("text", "string"), ("x", "number"), ("y", "number"), ("ticks", "number"), ("color?", "Color"), ("size?", "integer")], returns: "" },
        doc: "Writes text whose letters drop in one after another and bounce, ticks after it began (keep the tick it began in a state): GAME \
            OVER, a stage's name. Before 0 nothing shows." },
    Item { of: "controls", name: "device", what: Field { ty: "string|nil", set: false }, doc: "What the player plays with, the one used \
            last: \"keyboard\" or \"gamepad\"; nil before the run's first tick and where nobody plays (tests, a replay recorded without it)." },
    Item { of: "area", name: "name", what: Field { ty: "string", set: false }, doc: "Its name in the level's file (level.areas.NAME)." },
    Item { of: "area", name: "x", what: Field { ty: "integer", set: false }, doc: "Its low corner in voxels: x, y and z where it begins." },
    Item { of: "area", name: "y", what: Field { ty: "integer", set: false }, doc: "Its low corner's y." },
    Item { of: "area", name: "z", what: Field { ty: "integer", set: false }, doc: "Its low corner's z." },
    Item { of: "area", name: "size", what: Field { ty: "Point", set: false }, doc: "How big it is in voxels, across, deep and up." },
    Item { of: "area", name: "holds", what: Method { params: &[("at", "Thing|Point")], returns: "boolean" },
        doc: "Whether a thing stands inside it (where its feet are, the middle of the cell it stands on) or a point lies inside it ({ x, y, \
            z }; without z at any height). Ask it in on_tick: if level.areas.by_the_stove:holds(player) then ... end." },
    Item { of: "controls", name: "key", what: Method { params: &[("action", "string")], returns: "string" },
        doc: "The first key or button of an action (c.actions, one the ready-made rules bring, or pause) as the player sees it: the \
            keyboard's key as it is printed on it (Y where the binding is Z on a German keyboard), the pad's button as it is printed on the \
            pad (B, CIRCLE). Empty if nothing is bound or nobody plays." },
];

/// One of the game model's objects as scripts name it: the class the
/// editors' definitions give it, the Rust type behind it (mlua's name in
/// mistakes; `player` is a thing's), whether every script has it as a
/// global (the prelude refuses a global of the game's by that name), and
/// what it is. The one list of them: `OWNERS`, the definitions, the
/// sandbox's mistakes and the prelude read it.
pub struct Object {
    pub of: &'static str,
    pub class: &'static str,
    pub rust: Option<&'static str>,
    pub global: bool,
    pub about: &'static str,
}

pub const OBJECTS: [Object; 11] = [
    Object { of: "world", class: "World", rust: Some("WorldView"), global: true, about: "The whole run: its state, its time, how it ends." },
    Object { of: "level", class: "Level", rust: Some("LevelView"), global: true, about: "The level being played: its voxels, its things, \
            what happens in it, its camera." },
    Object { of: "level.camera", class: "Camera", rust: Some("CameraView"), global: false, about: "Where the camera looks." },
    Object { of: "player", class: "Player", rust: None, global: true, about: "The one who plays: a thing that belongs to no level and goes \
            from level to level (everything a thing has); its state is the run's. Its figure is the ready-made rules' player, a thing of \
            c.player.kind, or none (it then stands nowhere)." },
    Object { of: "thing", class: "Thing", rust: Some("ThingRef"), global: false, about: "A thing in the level, by its id." },
    Object { of: "area", class: "Area", rust: Some("AreaView"), global: false, about: "A named box of the level, set in its file \
            (level.areas.NAME): only to read." },
    Object { of: "input", class: "Input", rust: Some("InputView"), global: true, about: "The keys held this tick, by the game's action \
            names: only in on_tick." },
    Object { of: "screen", class: "Screen", rust: Some("ScreenView"), global: true, about: "The picture's effects (a shake, a flash, an icon \
            bursting), from anywhere the game changes." },
    Object { of: "frame", class: "Frame", rust: Some("Frame"), global: false, about: "The picture on_draw makes (function on_draw(frame)): \
            only there, only for this picture." },
    Object { of: "controls", class: "Controls", rust: Some("ControlsView"), global: true, about: "What the player plays with: the device and \
            the keys as the player sees them (their keyboard's, their pad's). Replays carry it, so the game may go by it anywhere. Seat 1's; \
            in on_draw the first player's at the machine drawing (frame.players)." },
    Object { of: "music", class: "Music", rust: Some("MusicView"), global: true, about: "The song your game plays of its own choice, its \
            beats and bars, and how loud and how muffled it sounds: the game can go with the music (waves on the beat), and replays play it \
            the same." },
];

/// The game model's objects, as scripts name them: the globals and
/// `level.camera`. Their names are offered after a dot or colon.
pub const OWNERS: [&str; OBJECTS.len()] = {
    let mut owners = [""; OBJECTS.len()];
    let mut i = 0;
    while i < OBJECTS.len() {
        owners[i] = OBJECTS[i].of;
        i += 1;
    }
    owners
};

/// What `of` offers: its own names, and for `player` a thing's too (it is
/// one; its own say more, `player.state`).
pub fn items_of(of: &str) -> impl Iterator<Item = &'static Item> + '_ {
    let own = API.iter().filter(move |i| i.of == of);
    let inherited = API.iter().filter(move |i| of == "player" && i.of == "thing" && !API.iter().any(|p| p.of == "player" && p.name == i.name));
    own.chain(inherited)
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

/// The name an item is written by (`thing:walk`, `world.state`, `on_tick`).
pub fn written(item: &Item) -> String {
    match item.what {
        Field { .. } => format!("{}.{}", item.of, item.name),
        Method { .. } => format!("{}:{}", item.of, item.name),
        Callback { .. } | Function { .. } => item.name.to_string(),
    }
}

/// One entry of what `voxcade api` shows: the API's names, then the
/// settings, then the kinds' keys; its name as written, its head line and
/// its words.
struct Entry {
    name: String,
    head: String,
    doc: String,
}

fn entries() -> Vec<Entry> {
    let mut out: Vec<Entry> = API.iter().map(|i| Entry { name: written(i), head: signature(i), doc: i.doc.to_string() }).collect();
    for s in settings() {
        out.push(Entry { name: format!("c.{}", s.path), head: s.head(), doc: s.doc });
    }
    for k in kind_keys() {
        out.push(Entry { name: format!("c.things.NAME.{}", k.key), head: k.head(), doc: k.doc });
    }
    out
}

/// A name as `voxcade api` compares it: small letters, a method's `:` a
/// dot, no call's brackets (`Thing:Walk()` is `thing.walk`).
fn compared(name: &str) -> String {
    let name = name.trim().split('(').next().unwrap_or("");
    name.to_lowercase().replace(':', ".")
}

/// The API as text, one name a line with what it does, then the settings
/// of `conf.lua` and the keys of a kind (`c.things.NAME`). `filter` keeps
/// the names that hold it, as written or in part, in any case, a method
/// with `:` or `.` (`thing:walk`, `walk`, `world.state`, `view_width`); a
/// player's names are a thing's too (`player:walk`). An unknown name is a
/// mistake naming the nearest.
pub fn lookup(filter: Option<&str>) -> Result<String, String> {
    let all = entries();
    let shown: Vec<&Entry> = match filter.map(compared).filter(|f| !f.is_empty()) {
        None => all.iter().collect(),
        Some(f) => {
            let thing = f.strip_prefix("player.").map(|rest| format!("thing.{rest}"));
            let holds = |e: &Entry| {
                let name = compared(&e.name);
                let last = name.rsplit('.').next().unwrap_or("");
                name.contains(&f) || last.contains(&f) || thing.as_ref().is_some_and(|t| name.contains(t.as_str()))
            };
            let found: Vec<&Entry> = all.iter().filter(|e| holds(e)).collect();
            if found.is_empty() {
                let names: Vec<String> = all.iter().flat_map(|e| [compared(&e.name), compared(&e.name).rsplit('.').next().unwrap_or("").to_string()]).collect();
                let near = crate::audio::words::near(&f, names.iter().map(String::as_str)).map_or(String::new(), |n| format!(" (did you mean {n}?)"));
                return Err(format!("no name of the API, setting or kind's key is called {}{near}; voxcade api lists them all", filter.unwrap_or("").trim()));
            }
            found
        }
    };
    let mut out = String::new();
    let mut part = "";
    for e in shown {
        let now = if e.name.starts_with("c.things.") { "KINDS: c.things.NAME = { ... } in conf.lua" } else if e.name.starts_with("c.") { "SETTINGS: conf.lua, function conf(c)" } else { "" };
        if now != part {
            out += &format!("\n== {now}\n\n");
            part = now;
        }
        out += &format!("{}\n", e.head);
        for line in e.doc.lines() {
            out += &format!("    {line}\n");
        }
    }
    Ok(out)
}

/// `lookup`'s text, or its mistake as text (MCP's `api` tool says it so).
pub fn text(filter: Option<&str>) -> String {
    lookup(filter).unwrap_or_else(|e| e)
}

/// A setting of `conf.lua` (`c.camera.view_width`), as the reference shows
/// it: from `config.rs`'s fields, their words, their defaults and ranges.
pub struct Setting {
    /// Its path under `c` (`camera.view_width`; `items.NAME.stack`,
    /// `title_screen.props[].model`).
    pub path: String,
    /// Its LuaLS type.
    pub ty: String,
    /// Its default as Lua writes it ("" unset).
    pub default: String,
    /// Its range in words ("" none).
    pub range: String,
    pub doc: String,
}

impl Setting {
    /// `c.camera.view_width: number = 200 (above 0, up to 65536)`.
    pub fn head(&self) -> String {
        let default = if self.default.is_empty() { String::new() } else { format!(" = {}", self.default) };
        let range = if self.range.is_empty() { String::new() } else { format!(" ({})", self.range) };
        format!("c.{}: {}{default}{range}", self.path, self.ty)
    }
}

/// A value of the settings as Lua writes it (`{ 0.45, 0.6, -1 }`); the
/// engine's f32s as written, not as f64 spells them.
fn lua_value(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => format!("{s:?}"),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => (*f as f32).to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::Array(a) if a.is_empty() => "{}".into(),
        toml::Value::Array(a) => format!("{{ {} }}", a.iter().map(lua_value).collect::<Vec<_>>().join(", ")),
        toml::Value::Table(t) if t.is_empty() => "{}".into(),
        toml::Value::Table(t) => format!("{{ {} }}", t.iter().map(|(k, v)| format!("{k} = {}", lua_value(v))).collect::<Vec<_>>().join(", ")),
    }
}

/// Every setting of `conf.lua`, section by section in `config.rs`'s order.
pub fn settings() -> Vec<Setting> {
    fn walk(name: &str, path: &str, ranged: &str, defaults: Option<&toml::Value>, out: &mut Vec<Setting>) {
        for k in conf_keys(name) {
            let (path, ranged) = (format!("{path}{}", k.key), format!("{ranged}{}", k.key));
            let default = defaults.and_then(|d| d.get(&k.key));
            let map = k.rust.strip_prefix("BTreeMap<String, ").and_then(|r| r.strip_suffix('>')).filter(|r| !conf_keys(r).is_empty());
            if let Some(inner) = map {
                walk(inner, &format!("{path}.NAME."), &format!("{ranged}.*."), None, out);
                continue;
            }
            if let Some((inner, list)) = inner_struct(&k.rust) {
                let (p, r) = if list { (format!("{path}[]."), format!("{ranged}[].")) } else { (format!("{path}."), format!("{ranged}.")) };
                walk(inner, &p, &r, if list { None } else { default }, out);
                continue;
            }
            let bounds: Vec<&crate::config::Bound> = crate::config::RANGES.iter().filter(|b| b.key == ranged || b.key.strip_prefix(ranged.as_str()).is_some_and(|rest| rest.starts_with('['))).collect();
            let range = bounds.iter().map(|b| if b.key == ranged { b.words() } else { format!("{} {}", if b.key.ends_with("[]") { "each" } else { &b.key[ranged.len()..] }, b.words()) }).collect::<Vec<_>>().join("; ");
            let ty = if path == "things" { kinds_type() } else { lua_of(&k.rust) };
            out.push(Setting { path, ty, default: default.map(lua_value).unwrap_or_default(), range, doc: k.doc });
        }
    }
    let defaults = toml::Value::try_from(crate::config::GameConfig::default()).ok();
    let mut out = Vec::new();
    walk("GameConfig", "", "", defaults.as_ref(), &mut out);
    out
}

/// A key of a kind of thing (`c.things.NAME.hp`): the engine's, of every
/// kind, or a behavior's of a kit (`speed` of the jump & run's hero).
pub struct KindKey {
    /// The kit (`c.kit`) and the behavior it is of; "" the engine's.
    pub kit: &'static str,
    pub behavior: String,
    pub key: String,
    pub ty: String,
    /// Its default as Lua writes it ("" none).
    pub default: String,
    pub doc: String,
}

impl KindKey {
    /// `c.things.NAME.speed: number = 150 (behavior = "hero", c.kit = "platformer")`.
    pub fn head(&self) -> String {
        let default = if self.default.is_empty() { String::new() } else { format!(" = {}", self.default) };
        let of = if self.kit.is_empty() { String::new() } else { format!(" (behavior = \"{}\", c.kit = \"{}\")", self.behavior, self.kit) };
        format!("c.things.NAME.{}: {}{default}{of}", self.key, self.ty)
    }
}

/// The keys every kind has, its own and those its behavior's rules read
/// besides (`sim::things`): name, LuaLS type, words.
pub const KIND_KEYS: &[(&str, &str, &str)] = &[
    ("like", "string", "Another kind this one builds on: its keys first, then this one's (like = \"rifleman\")."),
    ("behavior", "string", "Its ready-made behavior, with a kit (c.kit): the jump & run's hero, walker, hopper, flyer, pickup, platform, prop, crate; the shooter's soldier, spawner, captive, gunship, pickup. Without one your script moves it."),
    ("model", "string", "Its model, a .vvx of the game."),
    ("hp", "integer", "The hits it takes: thing.state.hp at the start, on_death at 0. Without it, it takes none."),
    ("fixed", "boolean", "It stays exactly where it is put, in the air too (a kind without a behavior)."),
    ("width", "number", "Its body across and deep, in voxels (c.physics.body_width unless set)."),
    ("height", "number", "Its body's height, in voxels (its model's unless set)."),
    ("facing", "number[]", "The way it faces as it comes in, { x, y } (x east, y north)."),
    ("props", "table", "What your code reads of it, thing.props: props = { worth = 100 }."),
    ("family", "string|string[]", "A family, or a list of them: level:things(\"enemies\") finds them, thing.families says them."),
    ("effect", "string|table", "Shown once where one of its things goes: a model (\"poof.vvx\", or { model = \"poof.vvx\", fps = 20 }), or \"burst\": it bursts into its voxels."),
    ("sound", "string", "Heard where one of its things goes: a sound's name (sounds/NAME.toml)."),
    ("reach", "number", "Touched within this many voxels on the ground, not by its box: a pickup, a helipad."),
    ("team", "string", "Its team: the rounds, blows and blasts of a team hurt the other teams, not its own."),
    ("immune", "string[]", "What does not hurt it: \"round\", \"melee\", \"blast\"."),
    ("rotates", "string", "\"free\": it faces exactly its way, a diagonal too; \"quarter\" (unless set): the nearest quarter turn."),
    ("spin", "number", "Its model turns round as it is drawn, degrees a second (a gem)."),
    ("frames", "table", "Its model's frames for what it does, by name, first to last: frames = { walk = { 1, 4 } }; a set may say { first =, last =, fps =, stride =, loop = false }."),
    ("fps", "number", "How fast its frames go where a set does not say, frames a second (10)."),
    ("swing", "table<string, number>", "A figure of parts: the parts that swing as it walks, degrees forward first: swing = { leg_left = 30, leg_right = -30 }."),
];

/// Words for a behavior's keys its kit's library says too little of, or
/// leaves out (it sets only what its kinds need): the kit, the behavior,
/// the key, its LuaLS type, its default ("": unset) and what it is.
const KIT_WORDS: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("shooter", "soldier", "weapon", "string", "", "The weapon he fires, a name of c.weapons."),
    ("shooter", "soldier", "speed", "number", "", "Running once he knows of the player, voxels a second."),
    ("shooter", "soldier", "idle_speed", "number", "", "Walking about before that, voxels a second."),
    ("shooter", "soldier", "accel", "number", "", "How quickly he gets to either, voxels a second per second."),
    ("shooter", "soldier", "sight", "number", "", "How far he sees the player, voxels; a comrade falling he notices as far."),
    ("shooter", "soldier", "blast_hearing", "number", "", "How far he hears an explosion, voxels."),
    ("shooter", "soldier", "range", "number", "", "How far he shoots, voxels."),
    ("shooter", "soldier", "muzzle", "number[]", "", "Where his shots leave the gun, { ahead, up } of his feet in voxels (unset: from his height)."),
    ("shooter", "soldier", "stride", "number", "", "A puff of dust every this many voxels he walks (unset: half his height) ..."),
    ("shooter", "spawner", "spawns", "string", "", "The kind of thing it sends, a name of c.things."),
    ("shooter", "spawner", "count", "number", "", "How many it sends in all."),
    ("shooter", "spawner", "first", "number", "", "Ticks after the player comes within range before the first comes out."),
    ("shooter", "spawner", "every", "number", "", "Ticks between one and the next."),
    ("shooter", "spawner", "range", "number", "", "It sends them while the player is this near, voxels."),
    ("shooter", "spawner", "ahead", "number[]", "", "Only while it lies this way of the player, { x, y } (x east, y north), by more than ahead_by; unset: on any side."),
    ("shooter", "spawner", "ahead_by", "number", "", "How far that way it must lie, voxels (unset: the player's height)."),
    ("shooter", "captive", "speed", "number", "", "Running off once freed, voxels a second."),
    ("shooter", "captive", "accel", "number", "", "How quickly it gets to that speed."),
    ("shooter", "gunship", "weapon", "string", "", "Its gun, a name of c.weapons."),
    ("shooter", "gunship", "rockets", "string", "", "Its rockets, a name of c.weapons."),
    ("shooter", "gunship", "speed", "number", "", "Flying, voxels a second."),
    ("shooter", "gunship", "crash_blast", "number", "", "The crater where it crashes, as a weapon's blast."),
    ("shooter", "pickup", "bob", "number", "", "It bobs up this many voxels and down again (0: it lies still; unset: a fifth of its model's height)."),
];

/// The kits whose behaviors' keys the reference lists: their name as
/// `c.kit` writes it, and their library of kinds (`kit.toml`), whose
/// numbers are the behaviors' defaults (test
/// `the_kits_kinds_are_the_behaviors_defaults`).
fn kits() -> [(&'static str, &'static str); 2] {
    [("platformer", crate::platformer::kinds::KIT), ("shooter", crate::shooter::kinds::KIT)]
}

/// Every key of a kind: the engine's (`KIND_KEYS`), then each kit's
/// behaviors' as its `kit.toml` writes them, with their defaults and the
/// words beside them.
pub fn kind_keys() -> Vec<KindKey> {
    let mut out: Vec<KindKey> = KIND_KEYS.iter().map(|(key, ty, doc)| KindKey { kit: "", behavior: String::new(), key: key.to_string(), ty: ty.to_string(), default: String::new(), doc: doc.to_string() }).collect();
    for (kit, text) in kits() {
        let mut behavior: Option<String> = None;
        let mut section_is_kind = false;
        let mut lines: Vec<(String, toml::Value, String)> = Vec::new();
        let flush = |behavior: &Option<String>, lines: &mut Vec<(String, toml::Value, String)>, out: &mut Vec<KindKey>| {
            if let Some(b) = behavior {
                for (key, value, doc) in lines.drain(..) {
                    if !out.iter().any(|k| k.kit == kit && k.behavior == *b && k.key == key) {
                        let ty = match &value {
                            toml::Value::Integer(_) | toml::Value::Float(_) => "number",
                            toml::Value::Boolean(_) => "boolean",
                            toml::Value::String(_) => "string",
                            toml::Value::Array(a) if a.iter().all(|v| v.is_integer() || v.is_float()) => "number[]",
                            toml::Value::Array(_) => "string[]",
                            _ => "table",
                        };
                        out.push(KindKey { kit, behavior: b.clone(), key, ty: ty.to_string(), default: lua_value(&value), doc });
                    }
                }
            }
            lines.clear();
        };
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                flush(&behavior, &mut lines, &mut out);
                (behavior, section_is_kind) = (None, line.starts_with("[things."));
                continue;
            }
            if !section_is_kind {
                continue;
            }
            // A key, or one the library leaves out (`# ahead = [0, 1]  # ...`).
            let (setting, doc) = match line.strip_prefix('#').map(str::trim) {
                Some(commented) => commented.split_once(" #").map_or((commented, ""), |(s, d)| (s, d)),
                None => line.split_once('#').map_or((line, ""), |(s, d)| (s, d)),
            };
            let Ok(table) = setting.trim().parse::<toml::Table>() else { continue };
            let Some((key, value)) = table.into_iter().next() else { continue };
            match key.as_str() {
                "behavior" => behavior = value.as_str().map(str::to_string),
                k if KIND_KEYS.iter().any(|(e, _, _)| *e == k) => {}
                _ => lines.push((key, value, doc.trim().trim_start_matches("...").trim().to_string())),
            }
        }
        flush(&behavior, &mut lines, &mut out);
    }
    for (kit, behavior, key, ty, default, doc) in KIT_WORDS {
        match out.iter_mut().find(|k| k.kit == *kit && k.behavior == *behavior && k.key == *key) {
            Some(k) => k.doc = doc.to_string(),
            None => {
                let at = out.iter().rposition(|k| k.kit == *kit && k.behavior == *behavior).map_or(out.len(), |i| i + 1);
                out.insert(at, KindKey { kit, behavior: behavior.to_string(), key: key.to_string(), ty: ty.to_string(), default: default.to_string(), doc: doc.to_string() });
            }
        }
    }
    out
}

/// The LuaLS class of a kit's behavior's kind (`Kind.platformer.hero`).
fn kind_class(kit: &str, behavior: &str) -> String {
    format!("Kind.{kit}.{behavior}")
}

/// `c.things`'s type: a kind, or one of a kit's behaviors.
fn kinds_type() -> String {
    let mut classes = vec!["Kind".to_string()];
    for k in kind_keys().iter().filter(|k| !k.kit.is_empty()) {
        let class = kind_class(k.kit, &k.behavior);
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    format!("table<string, {}>", classes.join("|"))
}

/// The kinds' classes for LuaLS: `Kind` with the engine's keys, and a
/// class for each kit's behavior with its own.
fn kind_classes() -> String {
    let keys = kind_keys();
    let mut out = String::from("\n---@class Kind A kind of thing (c.things.NAME = { ... }): what every kind may set; a behavior of a kit (c.kit) reads more.\n");
    for k in keys.iter().filter(|k| k.kit.is_empty()) {
        out += &format!("---@field {}? {} {}\n", k.key, k.ty, k.doc);
    }
    let mut done: Vec<String> = Vec::new();
    for k in keys.iter().filter(|k| !k.kit.is_empty()) {
        let class = kind_class(k.kit, &k.behavior);
        if !done.contains(&class) {
            out += &format!("\n---@class {class} : Kind behavior = \"{}\" of the ready-made rules c.kit = \"{}\": its keys besides every kind's, unset its defaults.\n", k.behavior, k.kit);
            done.push(class);
        }
        let default = if k.default.is_empty() { String::new() } else { format!(" ({} unless set)", k.default) };
        out += &format!("---@field {}? {} {}{default}\n", k.key, k.ty, k.doc);
    }
    out
}

/// The definitions for LuaLS (`lua/voxcade.lua`).
pub fn definitions() -> String {
    let mut out = String::from(
        "---@meta voxcade\n-- Voxcade's Lua API for editors (LuaLS), written by `voxcade api --lua`\n-- from the engine's register of it (src/lua/api.rs): made anew, not edited.\n\n",
    );
    out += TYPES;
    for object in &OBJECTS {
        let (of, class) = (object.of, object.class);
        let parent = if of == "player" { " : Thing" } else { "" };
        out += &format!("\n---@class {class}{parent} {}\n", object.about);
        for item in API.iter().filter(|i| i.of == of) {
            if let Field { ty, .. } = item.what {
                // Words of more than one line go above the field, where
                // LuaLS takes them as its description.
                if item.doc.contains('\n') {
                    out += &format!("{}---@field {} {ty}\n", comment_lines(item.doc), item.name);
                } else {
                    out += &format!("---@field {} {ty} {}\n", item.name, item.doc);
                }
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
    for o in OBJECTS.iter().filter(|o| o.global) {
        out += &format!("---@type {}\n{} = {{}}\n", o.class, o.of);
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

/// A doc as LuaLS's comment lines, `---` each.
fn comment_lines(doc: &str) -> String {
    doc.lines().map(|l| format!("---{l}\n")).collect()
}

fn function(name: &str, doc: &str, params: &[(&str, &str)], returns: &str) -> String {
    let mut out = format!("\n{}", comment_lines(doc));
    for (n, t) in params {
        out += &format!("---@param {n} {t}\n");
    }
    if !returns.is_empty() {
        out += &format!("---@return {returns}\n");
    }
    let names: Vec<&str> = params.iter().map(|(n, _)| n.trim_end_matches('?')).collect();
    out + &format!("function {name}({}) end\n", names.join(", "))
}

/// The settings' source: `conf.lua`'s keys are its structs' fields, each
/// with the words written over it, so the register of settings is the
/// settings themselves (a test holds the reading to what serde reads).
const CONFIG_RS: &str = include_str!("../config.rs");

/// Settings `conf.lua` may no longer set (the upgrade takes them out, a
/// game that sets one is told), left out of the definitions.
const RETIRED: [&str; 2] = ["markers", "models"];

/// A setting as `config.rs` declares it: its key, its Rust type and the
/// words over it.
pub struct ConfKey {
    pub key: String,
    pub rust: String,
    pub doc: String,
}

/// The fields of `pub struct NAME` in `config.rs` that `conf.lua` sets
/// (not `#[serde(skip)]`, `flatten` or retired ones), in their order.
pub fn conf_keys(name: &str) -> Vec<ConfKey> {
    let Some(start) = CONFIG_RS.find(&format!("\npub struct {name} {{")) else { return Vec::new() };
    let body = &CONFIG_RS[start..];
    let body = &body[..body.find("\n}").unwrap_or(body.len())];
    let (mut out, mut doc, mut left_out) = (Vec::new(), Vec::<&str>::new(), false);
    for line in body.lines().skip(1).map(str::trim) {
        if let Some(words) = line.strip_prefix("///") {
            doc.push(words.trim());
        } else if line.starts_with("#[serde(") {
            left_out |= line.contains("skip)") || line.contains("skip_serializing)") || line.contains("flatten");
        } else if let Some((key, rust)) = line.strip_prefix("pub ").and_then(|l| l.split_once(": ")) {
            if !left_out && !RETIRED.contains(&key) {
                out.push(ConfKey { key: key.to_string(), rust: rust.trim_end_matches(',').to_string(), doc: plain_doc(&doc.join(" ")) });
            }
            (doc, left_out) = (Vec::new(), false);
        }
    }
    out
}

/// A field's words as a game author reads them: no backticks, no aside
/// naming the engine's Rust (`(audio::limiter)`).
fn plain_doc(doc: &str) -> String {
    let mut out = String::new();
    let mut rest = doc;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')').map(|c| open + c) else { break };
        out += &rest[..open];
        if !rest[open..close].contains("::") {
            out += &rest[open..=close];
        }
        rest = &rest[close + 1..];
    }
    out += rest;
    out.replace('`', "").replace("  ", " ").replace(" ,", ",").replace(" .", ".").trim().to_string()
}

/// The struct a Rust type of `config.rs` names, if it is one there with
/// fields of its own (`CameraConfig`, `Option<FollowHeight>`, `Vec<PropConfig>`).
fn inner_struct(rust: &str) -> Option<(&str, bool)> {
    let (name, list) = match rust.strip_prefix("Vec<").and_then(|r| r.strip_suffix('>')) {
        Some(inner) => (inner, true),
        None => (rust.strip_prefix("Option<").and_then(|r| r.strip_suffix('>')).unwrap_or(rust), false),
    };
    (!conf_keys(name).is_empty()).then_some((name, list))
}

/// A Rust type of a setting as LuaLS writes it.
fn lua_of(rust: &str) -> String {
    let rust = rust.trim();
    if let Some(inner) = rust.strip_prefix("Option<").and_then(|r| r.strip_suffix('>')) {
        return lua_of(inner);
    }
    if let Some(inner) = rust.strip_prefix("Vec<").and_then(|r| r.strip_suffix('>')) {
        return format!("{}[]", lua_of(inner));
    }
    if let Some(inner) = rust.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return format!("{}[]", lua_of(inner.split(';').next().unwrap_or("any")));
    }
    if let Some(inner) = rust.strip_prefix("BTreeMap<String, ").and_then(|r| r.strip_suffix('>')) {
        return format!("table<string, {}>", lua_of(inner));
    }
    match rust {
        "f32" | "f64" => "number".into(),
        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" => "integer".into(),
        "bool" => "boolean".into(),
        "String" => "string".into(),
        "SeatKinds" | "Songs" => "string|string[]".into(),
        r if r.starts_with('(') => "any[]".into(),
        r if r.contains("Table") || r.contains("Bindings") => "table".into(),
        r if !conf_keys(r).is_empty() => "table".into(),
        // The settings' words (`Rotates`, `ShadowStyle`): written as text.
        _ => "string".into(),
    }
}

/// `Conf` and its sections from `config.rs` (every key `conf.lua` sets,
/// set by default or not): completion for `c.` in `conf.lua`; the ready-
/// made rules' own sections and keys (c.weapons, c.player.knife) left open.
fn conf_classes() -> String {
    let mut out = String::from("\n---@class Conf The game's settings (conf.lua); ready-made rules (c.kit) add their own (c.weapons).\n---@field [string] table\n");
    let mut classes = String::new();
    for k in conf_keys("GameConfig") {
        if k.key == "things" {
            out += &format!("---@field {} {} {}\n", k.key, kinds_type(), k.doc);
            continue;
        }
        out += &conf_field(&k, &format!("Conf.{}", k.key), &mut classes);
    }
    out + &classes + &kind_classes()
}

/// A key's line in its class; a key whose type is a struct of its own
/// gets a class (`Conf.camera`, `Conf.world.backdrops`), written into `classes`.
fn conf_field(k: &ConfKey, class: &str, classes: &mut String) -> String {
    let doc = if k.doc.is_empty() { String::new() } else { format!(" {}", k.doc) };
    let Some((name, list)) = inner_struct(&k.rust) else {
        // Its range, as the reference says it (`settings`).
        let path = class.strip_prefix("Conf.").unwrap_or(class);
        let range = settings().into_iter().find(|s| s.path.replace("[]", "") == path).map(|s| s.range).filter(|r| !r.is_empty()).map_or(String::new(), |r| format!(" ({r})"));
        return format!("---@field {} {}{doc}{range}\n", k.key, lua_of(&k.rust));
    };
    // A section the ready-made rules add to (c.player.knife) stays open.
    let open = if name == "PlayerConfig" { "---@field [string] any\n" } else { "" };
    let mut body = format!("\n---@class {class}\n{open}");
    for inner in conf_keys(name) {
        body += &conf_field(&inner, &format!("{class}.{}", inner.key), classes);
    }
    *classes += &body;
    format!("---@field {} {class}{}{doc}\n", k.key, if list { "[]" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GameConfig;

    /// What an object registers with Lua, recorded by name as mlua asks
    /// for its fields and methods (`UserData::add_fields`, `add_methods`):
    /// the names a script can reach, however the binding is written.
    #[derive(Default)]
    struct Recorded {
        names: Vec<String>,
        settable: Vec<String>,
    }

    impl<T> mlua::UserDataFields<T> for Recorded {
        fn add_field<V: mlua::IntoLua + 'static>(&mut self, name: impl Into<String>, _: V) {
            self.names.push(name.into());
        }
        fn add_field_method_get<M, R>(&mut self, name: impl Into<String>, _: M)
        where
            M: Fn(&mlua::Lua, &T) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            R: mlua::IntoLua,
        {
            self.names.push(name.into());
        }
        fn add_field_method_set<M, A>(&mut self, name: impl Into<String>, _: M)
        where
            M: FnMut(&mlua::Lua, &mut T, A) -> mlua::Result<()> + mlua::MaybeSend + 'static,
            A: mlua::FromLua,
        {
            self.settable.push(name.into());
        }
        fn add_field_function_get<F, R>(&mut self, name: impl Into<String>, _: F)
        where
            F: Fn(&mlua::Lua, mlua::AnyUserData) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            R: mlua::IntoLua,
        {
            self.names.push(name.into());
        }
        fn add_field_function_set<F, A>(&mut self, name: impl Into<String>, _: F)
        where
            F: FnMut(&mlua::Lua, mlua::AnyUserData, A) -> mlua::Result<()> + mlua::MaybeSend + 'static,
            A: mlua::FromLua,
        {
            self.settable.push(name.into());
        }
        fn add_meta_field<V: mlua::IntoLua + 'static>(&mut self, _: impl Into<String>, _: V) {}
        fn add_meta_field_with<F, R>(&mut self, _: impl Into<String>, _: F)
        where
            F: FnOnce(&mlua::Lua) -> mlua::Result<R> + 'static,
            R: mlua::IntoLua,
        {
        }
    }

    impl<T> mlua::UserDataMethods<T> for Recorded {
        fn add_method<M, A, R>(&mut self, name: impl Into<String>, _: M)
        where
            M: Fn(&mlua::Lua, &T, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
            self.names.push(name.into());
        }
        fn add_method_mut<M, A, R>(&mut self, name: impl Into<String>, _: M)
        where
            M: FnMut(&mlua::Lua, &mut T, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
            self.names.push(name.into());
        }
        fn add_function<F, A, R>(&mut self, name: impl Into<String>, _: F)
        where
            F: Fn(&mlua::Lua, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
            self.names.push(name.into());
        }
        fn add_function_mut<F, A, R>(&mut self, name: impl Into<String>, _: F)
        where
            F: FnMut(&mlua::Lua, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
            self.names.push(name.into());
        }
        fn add_meta_method<M, A, R>(&mut self, _: impl Into<String>, _: M)
        where
            M: Fn(&mlua::Lua, &T, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
        }
        fn add_meta_method_mut<M, A, R>(&mut self, _: impl Into<String>, _: M)
        where
            M: FnMut(&mlua::Lua, &mut T, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
        }
        fn add_meta_function<F, A, R>(&mut self, _: impl Into<String>, _: F)
        where
            F: Fn(&mlua::Lua, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
        }
        fn add_meta_function_mut<F, A, R>(&mut self, _: impl Into<String>, _: F)
        where
            F: FnMut(&mlua::Lua, A) -> mlua::Result<R> + mlua::MaybeSend + 'static,
            A: mlua::FromLuaMulti,
            R: mlua::IntoLuaMulti,
        {
        }
    }

    /// The names an object registers, and those it lets a script set.
    fn registered<T: mlua::UserData>() -> Recorded {
        let mut r = Recorded::default();
        T::add_fields(&mut r);
        T::add_methods(&mut r);
        r
    }

    /// The register names exactly what `world/` gives the scripts, field
    /// by field and method by method: the names each object registers
    /// with Lua, recorded as it registers them (not read from the source,
    /// so a binding made by a helper or written another way counts too),
    /// and what a script may set. Objects scripts never see register none.
    #[test]
    fn the_register_is_what_the_scripts_see() {
        use super::super::world;
        let objects = [
            ("world", registered::<world::WorldView>()),
            ("level", registered::<world::LevelView>()),
            ("level.camera", registered::<world::CameraView>()),
            ("thing", registered::<world::ThingRef>()),
            ("area", registered::<world::AreaView>()),
            ("input", registered::<world::InputView>()),
            ("screen", registered::<world::ScreenView>()),
            ("frame", registered::<world::Frame>()),
            ("controls", registered::<world::ControlsView>()),
            ("music", registered::<world::MusicView>()),
        ];
        for (name, r) in [("Absent", registered::<world::Absent>()), ("Stage", registered::<super::super::host::Stage>())] {
            assert!(r.names.is_empty() && r.settable.is_empty(), "{name} offers {:?}", r.names);
        }
        let mut offered: Vec<(String, String)> = objects.iter().flat_map(|(of, r)| r.names.iter().map(|n| (of.to_string(), n.clone()))).collect();
        offered.sort();
        offered.dedup();
        // `player` is a thing: what the register says of it, a thing offers.
        let of = |i: &Item| if i.of == "player" { "thing".to_string() } else { i.of.to_string() };
        let mut listed: Vec<(String, String)> = API.iter().filter(|i| !i.of.is_empty()).map(|i| (of(i), i.name.to_string())).collect();
        listed.sort();
        listed.dedup();
        assert_eq!(listed, offered);
        let mut settable: Vec<(String, String)> = objects.iter().flat_map(|(of, r)| r.settable.iter().map(|n| (of.to_string(), n.clone()))).collect();
        settable.sort();
        let mut said: Vec<(String, String)> = API.iter().filter(|i| matches!(i.what, Field { set: true, .. })).map(|i| (of(i), i.name.to_string())).collect();
        said.sort();
        assert_eq!(said, settable, "what a script may set");
    }

    /// `voxcade api` finds a name as it prints it (`thing:walk`,
    /// `thing.walk`, `world.state`), in any case, in part, a player's as a
    /// thing's, a setting and a kind's key; an unknown name is a mistake
    /// naming the nearest (it printed nothing and said all was well).
    #[test]
    fn the_api_finds_names_as_it_prints_them() {
        let found = |f: &str| lookup(Some(f)).unwrap_or_else(|e| panic!("{f}: {e}"));
        for (asked, shows) in [
            ("thing:walk", "thing:walk(way: Way, speed: number)"),
            ("thing.walk", "thing:walk("),
            ("Thing:Walk()", "thing:walk("),
            ("player:walk", "thing:walk("),
            ("world.state", "world.state: table"),
            ("walk", "thing:walk("),
            ("on_tick", "function on_tick()"),
            ("view_width", "c.camera.view_width: number = 200 (above 0, up to 65536)"),
            ("c.camera.fov", "c.camera.fov: number = 40 (from 10 to 100)"),
            ("light.sun", "c.light.sun: number[] = { 0.45, 0.6, -1 } (each from -1000 to 1000; [3] from -1000 to below 0)"),
            ("air_jumps", "c.things.NAME.air_jumps: number = 0 (behavior = \"hero\", c.kit = \"platformer\")"),
            ("fixed", "c.things.NAME.fixed: boolean"),
        ] {
            assert!(found(asked).contains(shows), "{asked}: {}", found(asked));
        }
        assert!(!found("thing:walk").contains("world.state"));
        let e = lookup(Some("wlak")).unwrap_err();
        assert!(e.starts_with("no name of the API, setting or kind's key is called wlak (did you mean walk?)"), "{e}");
        assert!(lookup(Some("zzzzzzzz")).is_err());
        let all = lookup(None).unwrap();
        assert!(all.contains("== SETTINGS") && all.contains("== KINDS") && all.contains("level:spawn("));
    }

    /// The kinds' keys in the reference are those a kind takes: the
    /// engine's every kind has (those `sim::things` reads), and each kit's
    /// behavior's, as its rules take them (serde's list of the keys it
    /// knows, asked with one it does not), each with words.
    #[test]
    fn the_kinds_keys_are_what_a_kind_takes() {
        let keys = kind_keys();
        let engine: Vec<&str> = keys.iter().filter(|k| k.kit.is_empty()).map(|k| k.key.as_str()).collect();
        let mut taken: Vec<&str> = ["like", "behavior", "model", "hp", "fixed", "width", "height", "facing"].into_iter().chain(crate::sim::things::COMMON).collect();
        taken.sort();
        let mut ours = engine.clone();
        ours.sort();
        assert_eq!(ours, taken);
        for k in &keys {
            assert!(!k.doc.is_empty(), "c.things.NAME.{} of {} {}: no words", k.key, k.kit, k.behavior);
        }
        // Each behavior's own keys, as its kit refuses one it does not know.
        let kit_cfg = |kit: &str, behavior: &str| {
            let library: toml::Table = toml::from_str(kits().iter().find(|(k, _)| *k == kit).unwrap().1).unwrap();
            let mut things: std::collections::BTreeMap<String, toml::Table> = library["things"].as_table().unwrap().iter().map(|(k, v)| (k.clone(), v.as_table().unwrap().clone())).collect();
            let probe: toml::Table = toml::from_str(&format!("behavior = \"{behavior}\"\nnot_a_key = 0")).unwrap();
            things.insert("probe".into(), probe);
            let mut cfg = crate::config::GameConfig { things, kit: Some(kit.into()), ..Default::default() };
            if let Some(w) = library.get("weapons") {
                cfg.game.insert("weapons".into(), w.clone());
            }
            cfg
        };
        let mut behaviors: Vec<(&str, &str)> = keys.iter().filter(|k| !k.kit.is_empty()).map(|k| (k.kit, k.behavior.as_str())).collect();
        behaviors.dedup();
        assert!(behaviors.len() >= 12, "{behaviors:?}");
        for (kit, behavior) in behaviors {
            let cfg = kit_cfg(kit, behavior);
            let e = match kit {
                "platformer" => crate::platformer::kinds::Rules::from_game(&cfg, |_| None).err(),
                _ => crate::shooter::config::ShooterConfig::from_game(&cfg).and_then(|own| crate::shooter::kinds::Rules::from_game(&cfg, &own, &|_| None)).err(),
            };
            let e = format!("{:#}", e.unwrap_or_else(|| panic!("{kit} {behavior}: a key it does not know is taken")));
            let Some((_, list)) = e.split_once("expected ") else { panic!("{kit} {behavior}: {e}") };
            let list = list.trim_start_matches("one of ");
            let mut theirs: Vec<String> = list.lines().next().unwrap_or("").replace("` or `", "`, `").split(", ").map(|n| n.trim_matches(|c: char| c == '`' || c == ' ').to_string()).filter(|n| !engine.contains(&n.as_str())).collect();
            theirs.sort();
            let mut ours: Vec<String> = keys.iter().filter(|k| k.kit == kit && k.behavior == behavior).map(|k| k.key.clone()).collect();
            ours.sort();
            assert_eq!(ours, theirs, "{kit} {behavior}");
        }
    }

    /// The settings in the definitions are what `conf.lua` really sets:
    /// for every section (and the tables in it) the keys serde takes, as it
    /// lists them when given one it does not know, are those read from
    /// `config.rs` (but for a retired one it only takes to refuse), and
    /// every setting the engine writes out is one of `Conf`'s.
    #[test]
    fn the_settings_are_what_conf_lua_sets() {
        let defaults = toml::Value::try_from(GameConfig::default()).unwrap();
        // What serde says it takes at `path` (a list of tables: in one).
        let taken = |path: &[&str], list: bool| -> Option<Vec<String>> {
            let mut root = defaults.clone();
            let mut at = &mut root;
            for key in &path[..path.len() - 1] {
                if at.get(*key).is_none_or(|v| !v.is_table()) {
                    at.as_table_mut().unwrap().insert(key.to_string(), toml::Value::Table(Default::default()));
                }
                at = at.get_mut(*key).unwrap();
            }
            let probe: toml::Table = [("not_a_setting".to_string(), toml::Value::Integer(0))].into_iter().collect();
            let probe = if list { toml::Value::Array(vec![toml::Value::Table(probe)]) } else { toml::Value::Table(probe) };
            at.as_table_mut().unwrap().insert(path.last().unwrap().to_string(), probe);
            let e = root.try_into::<GameConfig>().err()?.message().to_string();
            let list = e.split_once("expected ").map(|(_, l)| l.trim_start_matches("one of "))?;
            Some(list.replace("` or `", "`, `").split(", ").map(|n| n.trim_matches('`').to_string()).collect())
        };
        type Taken<'a> = &'a dyn Fn(&[&str], bool) -> Option<Vec<String>>;
        fn walk(name: &str, path: Vec<String>, list: bool, taken: Taken, checked: &mut usize) {
            let ours: Vec<String> = conf_keys(name).iter().map(|k| k.key.clone()).collect();
            if !path.is_empty()
                && let Some(mut theirs) = taken(&path.iter().map(String::as_str).collect::<Vec<_>>(), list)
            {
                theirs.retain(|k| k != "boss_music");
                assert_eq!(ours, theirs, "c.{}: the keys read from config.rs are not those serde takes", path.join("."));
                *checked += 1;
            }
            for k in conf_keys(name) {
                if let Some((inner, list)) = inner_struct(&k.rust) {
                    walk(inner, [path.clone(), vec![k.key]].concat(), list, taken, checked);
                }
            }
        }
        let mut checked = 0;
        walk("GameConfig", Vec::new(), false, &taken, &mut checked);
        assert!(checked >= 12, "only {checked} sections checked");
        let ours: Vec<String> = conf_keys("GameConfig").into_iter().map(|k| k.key).collect();
        for key in defaults.as_table().unwrap().keys().filter(|k| !RETIRED.contains(&k.as_str())) {
            assert!(ours.contains(key), "c.{key} is a setting the register lacks");
        }
        let defs = definitions();
        for (class, key) in [("Conf.camera", "view_at_least"), ("Conf.camera", "view_height"), ("Conf.camera", "stop_at_edges"), ("Conf.camera", "hold"), ("Conf.camera", "rotates"), ("Conf.camera", "aspect"), ("Conf", "players"), ("Conf", "state")] {
            let at = defs.find(&format!("---@class {class}\n")).or_else(|| defs.find(&format!("---@class {class} "))).unwrap_or_else(|| panic!("no {class}"));
            let body = &defs[at..];
            let body = &body[..body[1..].find("\n---@class").map_or(body.len(), |e| e + 1)];
            assert!(body.contains(&format!("---@field {key} ")), "{class} lacks {key}");
        }
        assert!(!defs.contains("---@field markers"), "the retired c.markers is offered");
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

//! The Lua door: a game as a folder with `conf.lua` (its settings, set in
//! code: `function conf(c) ... end`) and `main.lua` (its handlers:
//! `on_world_start()` once, `on_level_start(how)` when a level begins,
//! `on_tick()` 60 times a second, `on_draw(frame)` every picture),
//! after LÖVE but deterministic: replays, `shot` and snapshots work as for
//! every game. The scripts see the game model's objects as globals
//! (`world`, `level`, `player`, `input`, `screen`; `world.rs`,
//! `docs/game-model.md`). A game's state lives in the engine's things and
//! in Lua's states, `world.state`, `level.state`, `player.state` and
//! `thing.state`, plain data the engine copies, hashes, checks every tick
//! and brings back; globals are frozen after the start (`on_world_start`,
//! the first `on_level_start`) and `on_draw` only looks (`sandbox`). What
//! the script removes goes at the tick's end. A script's error
//! stops the game with its file and line until the file is mended
//! (`Game::halted`, `reload_code`).
//!
//! A game may play with ready-made rules (`c.kit`, `host::Kit`): their
//! behaviours run by themselves each tick, the script's `on_tick` after
//! them; without them the world holds only what the script moves
//! (`host::Plain`).

pub mod api;
pub mod conf;
pub mod data;
mod hidden;
pub mod host;
pub mod lex;
mod sandbox;
pub mod save;
#[cfg(test)]
mod tests;
pub mod value;
pub mod world;

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use glam::Vec3;
use mlua::{AnyUserData, Function, Table};

use crate::audio::SoundDef;
use crate::config::{GameConfig, UiConfig};
use crate::controls::Bindings;
use crate::files::Files;
use crate::font::DisplayFont;
use crate::game::{Game, ModelSpec, Outcome, Setup};
use crate::hash::Stable;
use crate::input::Input;
use crate::render::{Instance, Light};
use crate::session::{Drawing, Models};
use crate::sim::things::{self, Id};
use crate::sim::{Body, Marker, Sim, body_hits, spawn_point};
use crate::world::World;
use host::{Ask, Host, Kind, Kit, PLAYER, Plain, Stage};
use sandbox::Sandbox;
use value::Value;
use world::{Absent, Cmd, ControlsView, Frame, InputView, LevelView, ScreenView, WorldView};

/// An inventory (`state.inventory`, in the state at `path`) counts items of
/// `c.items` by name, each from 0 to its stack.
fn check_inventory(state: &Table, path: &str, items: &std::collections::BTreeMap<String, crate::config::ItemConfig>) -> Result<()> {
    let inventory = match state.raw_get::<mlua::Value>("inventory")? {
        mlua::Value::Nil => return Ok(()),
        mlua::Value::Table(t) => t,
        other => anyhow::bail!("{path}.inventory is {}: an inventory is a table of items by name and how many", value::a(other.type_name())),
    };
    let mut entries: Vec<(mlua::Value, mlua::Value)> = inventory.pairs().collect::<mlua::Result<_>>()?;
    entries.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)));
    for (key, count) in entries {
        let mlua::Value::String(name) = &key else { anyhow::bail!("{}: items by name (c.items), not by a {}", value::key_path(&format!("{path}.inventory"), &key), key.type_name()) };
        let name = name.to_string_lossy();
        let at = format!("{path}.inventory.{name}");
        let Some(item) = items.get(&name) else {
            let mut names: Vec<(&i32, &str)> = items.iter().map(|(n, i)| (&i.order, n.as_str())).collect();
            names.sort();
            let names: Vec<&str> = names.into_iter().map(|(_, n)| n).collect();
            if names.is_empty() {
                anyhow::bail!("{at}: there are no items (c.items names them)");
            }
            anyhow::bail!("{at}: no item called '{name}' (c.items: {})", names.join(", "));
        };
        let n = match count {
            mlua::Value::Integer(i) => i,
            mlua::Value::Number(f) if f.fract() == 0.0 => f as i64,
            other => anyhow::bail!("{at} is {}: a count is a whole number", match other { mlua::Value::Number(f) => f.to_string(), o => value::a(o.type_name()) }),
        };
        if n < 0 || n > item.stack as i64 {
            anyhow::bail!("{at} = {n}: from 0 to {} (c.items.{name}.stack)", item.stack);
        }
    }
    Ok(())
}

/// An argument of a handler the engine calls (`LuaGame::tell`).
enum Told {
    Thing(u32),
    Nil,
    Int(i64),
    Str(String),
}

/// What a save brings into the level it enters: its `level.state`, the
/// voxels and colours its file would not bring back.
struct Loaded {
    level_state: Value,
    voxels: Vec<(glam::IVec3, u8)>,
    palette: Vec<(usize, [u8; 4])>,
    /// How bright the pieces' lights were, by the pieces' names.
    lights: Vec<(String, f32)>,
}

/// The most save slots a game may have (a menu shows them all).
const MAX_SLOTS: u32 = 9;

/// White over a thing that was just hit.
const FLASH: [u8; 4] = [255, 255, 255, 255];

/// The parts of a Lua game's state hash, in the order it takes them in
/// (`LuaGame::hash_by_parts`): what a desync names.
const PARTS: [&str; 9] = ["things", "world.state", "player.state", "level.state", "levels", "kept", "thing.state", "seats", "music"];

/// A kind's frames where there is no kind: the model's own layout.
static NO_FRAMES: std::collections::BTreeMap<String, crate::sim::things::Frames> = std::collections::BTreeMap::new();
/// A kind's swing where there is no kind: none.
static NO_SWING: std::collections::BTreeMap<String, f32> = std::collections::BTreeMap::new();

/// A Lua game, with the kit `K` when its `conf.lua` asks for it.
pub struct LuaGame<K: Kit> {
    stage: Stage,
    sandbox: Sandbox,
    tick: Option<Function>,
    draw: Option<Function>,
    /// The script's globals as `on_world_start` left them (the kit's
    /// questions are answered by its functions there).
    globals: Option<Table>,
    /// `main.lua` as tokens (what counts of it for replays).
    code: Vec<u8>,
    /// Whether `main.lua` names `music` or `on_beat`: its music's clock
    /// then counts for replays (`Game::uses_music`).
    uses_music: bool,
    /// Whether its code names `daylight`: it moves its sun
    /// (`Game::moves_sun`).
    moves_sun: bool,
    /// The files `main.lua` read as it started (`read`): they count for
    /// replays too, and saving one runs `main.lua` anew.
    data: Vec<(String, Vec<u8>)>,
    /// The game's files (`crate::files`), its environment once started (the
    /// globals frozen) and what each file required gave back: a scenario
    /// and a console line run there, `require` giving the same modules.
    files: Files,
    env: Option<Table>,
    required: Vec<(String, mlua::Value)>,
    /// The game's Lua files `main.lua` required (`require`), by path, as
    /// tokens: its code as much as `main.lua` is.
    modules: Vec<(String, Vec<u8>)>,
    actions: Arc<Vec<&'static str>>,
    /// The game keeps a score without a kit (`c.scored`).
    scored: bool,
    /// The keys held the tick before (`input:pressed`).
    before: Input,
    /// How many ticks each direction and action has been held
    /// (`input:repeated`).
    held_for: [u16; world::KEYS],
    halted: RefCell<Option<String>>,
    /// Whether `on_world_start` and the first `on_level_start` ran to
    /// their end (else a mended file starts the game afresh).
    started: bool,
    /// Levels left since the last tick (a level that sends on at once,
    /// again and again, is a mistake).
    changes: u32,
    /// Whether `on_draw` stopped the game (it waits for the mended file).
    draw_failed: Cell<bool>,
    /// Whether the state check at a tick's end stopped the game (a change
    /// of `conf.lua`, its items, may mend it too).
    state_failed: Cell<bool>,
    /// The save the run goes on from at once (`Game::start_from`).
    start_text: Option<String>,
    /// The saves made since the session took them (`Game::saving`).
    /// The thing the player arrives at in the level entered next (a
    /// checkpoint by its name).
    arrive_at: Option<String>,
    /// Where a save stood the players of the seats after the first, for the
    /// level it enters.
    seat_places: Vec<(usize, crate::levels::Place)>,
    written: Vec<(u32, String)>,
    /// What `on_draw` asked for the last time it ran (shown while stopped).
    drawn: RefCell<Vec<Cmd>>,
    /// What `on_draw` wrote for the HUD.
    /// What `on_draw` put on the HUD, drawn there (`hud`).
    overlay: RefCell<Vec<Cmd>>,
    kit: PhantomData<K>,
}

/// `thing.state` of every thing that has some, by id (the script's part:
/// the engine's keys stay with the things).
type Data = Vec<(u32, Value)>;
/// What each seat's rules spend, by seat (`seats_stock_in`).
type SeatsStock = Vec<(usize, Vec<(String, u32)>)>;

/// The script's part of the states as plain data: `world.state`,
/// `player.state` (without the engine's keys), `level.state`, every
/// `thing.state`.
#[derive(Default)]
struct Captured {
    state: Option<Value>,
    player: Option<Value>,
    level: Option<Value>,
    /// Every level's `level.kept`, by the level's name.
    kept: Vec<(String, Value)>,
    data: Data,
    /// The states of the seats after the first, by seat.
    seats: Vec<(usize, Value)>,
}

/// A Lua game as it stood at a tick.
pub struct LuaSnapshot {
    stage: Stage,
    script: Captured,
    before: Input,
    held_for: [u16; world::KEYS],
}

/// The old API's handlers, refused with their new names.
const OLD_NAMES: [(&str, &str); 3] = [("load", "on_world_start()"), ("tick", "on_tick()"), ("draw", "on_draw(frame)")];

/// Handlers renamed since, refused with their new names.
const RENAMED: [(&str, &str); 1] = [("on_leave", "on_touch_end")];

/// The handlers whose event brings nothing to say: they take no
/// parameters (one named `world` would hide the global). `on_level_start`
/// takes one, how the level was entered.
const WITHOUT_PARAMETERS: [&str; 2] = ["on_world_start", "on_tick"];

/// Where `input` may be used.
const NO_INPUT: &str = "input: the keys held are only there in on_tick";

/// Whether the config asks for the kit `K`.
fn with<K: Kit>(cfg: &GameConfig) -> bool {
    cfg.kit.as_deref().is_some_and(K::named)
}

/// The voxels each kind that bursts as it goes (`effect = "burst"`) bursts
/// into: its model's first frame, by the kind's place.
fn bursts(kinds: &[Kind], models: &Models) -> std::collections::BTreeMap<usize, crate::sim::Shape> {
    kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| k.effect.as_ref().is_some_and(|e| e.bursts()))
        .filter_map(|(i, k)| Some((i, models.shape(k.model.as_deref()?)?)))
        .collect()
}

impl<K: Kit> LuaGame<K> {
    fn kit(&self) -> Option<&K> {
        (self.stage.host.as_ref() as &dyn Any).downcast_ref::<K>()
    }

    fn kit_mut(&mut self) -> Option<&mut K> {
        (self.stage.host.as_mut() as &mut dyn Any).downcast_mut::<K>()
    }

    fn plain(&self) -> Option<&Plain> {
        (self.stage.host.as_ref() as &dyn Any).downcast_ref::<Plain>()
    }

    fn plain_mut(&mut self) -> Option<&mut Plain> {
        (self.stage.host.as_mut() as &mut dyn Any).downcast_mut::<Plain>()
    }

    /// Runs a call with the game to change (`on_world_start`,
    /// `on_level_start`, `on_tick`).
    fn with_world<R>(&mut self, call: impl FnOnce(&Sandbox, AnyUserData) -> Result<R>) -> Result<R> {
        let sandbox = &self.sandbox;
        let stage = &mut self.stage;
        let lua = &sandbox.lua;
        let mut result = None;
        lua.scope(|scope| {
            let world = scope.create_userdata_ref_mut(stage)?;
            lua.set_named_registry_value("voxcade.world", &world)?;
            lua.set_named_registry_value("voxcade.looking", false)?;
            result = Some(call(sandbox, world));
            lua.unset_named_registry_value("voxcade.world")
        })
        .map_err(sandbox::script_error)?;
        result.expect("the call ran")
    }

    /// The game goes on after an error was mended.
    fn mended(&self) {
        *self.halted.borrow_mut() = None;
        self.draw_failed.set(false);
        self.state_failed.set(false);
    }

    fn halt(&self, e: anyhow::Error) {
        let text = format!("{e:#}");
        let mut halted = self.halted.borrow_mut();
        if halted.as_deref() != Some(text.as_str()) {
            eprintln!("{text}");
        }
        *halted = Some(text);
    }

    /// Checks every state is plain data, as a snapshot would take it, after
    /// every tick and at the start, without copying it (the prelude's
    /// `plain`); the first mistake, named where it sits.
    fn check_states(&self) -> Result<()> {
        let lua = &self.sandbox.lua;
        let plain = self.sandbox.tool("plain")?;
        let is_thing: Function = lua.named_registry_value("voxcade.is_thing")?;
        let seen = lua.create_table()?;
        let check = |table: Table, path: &str, things: bool| -> Result<()> {
            let (why, keys): (Option<String>, Option<Table>) = self.sandbox.call_engine(&plain, (table, &seen, things, &is_thing))?;
            let Some(why) = why else { return Ok(()) };
            let mut at = path.to_string();
            if let Some(keys) = keys {
                let keys: Vec<mlua::Value> = keys.sequence_values().collect::<mlua::Result<_>>()?;
                for k in keys.iter().rev() {
                    at = value::key_path(&at, k);
                }
            }
            Err(value::mistake(&at, &why))
        };
        check(self.registry("voxcade.state")?, "world.state", false)?;
        check(self.registry("voxcade.player")?, "player.state", false)?;
        check(self.registry("voxcade.level")?, "level.state", true)?;
        let kept = self.registry("voxcade.kept")?;
        let mut names: Vec<String> = kept.pairs::<String, mlua::Value>().filter_map(|p| p.ok()).map(|(n, _)| n).collect();
        names.sort();
        for name in names {
            check(kept.raw_get(name.as_str())?, &format!("level.kept of {name}"), false)?;
        }
        let data = self.registry("voxcade.data")?;
        let mut ids: Vec<u32> = data.pairs::<u32, mlua::Value>().filter_map(|p| p.ok()).map(|(id, _)| id).collect();
        ids.sort_unstable();
        let items = &self.stage.host.sim().cfg.items;
        let player = self.registry("voxcade.player")?;
        check_inventory(&player, "player.state", items)?;
        for n in 2..=self.stage.seats.len() {
            if let Some(state) = self.registry("voxcade.seats")?.raw_get::<Option<Table>>(n)? {
                let path = format!("player {n}'s state");
                check(state.clone(), &path, false)?;
                check_inventory(&state, &path, items)?;
            }
        }
        for id in ids {
            let state: Table = data.raw_get(id)?;
            check(state.clone(), &format!("thing {id}'s state"), true)?;
            check_inventory(&state, &format!("thing {id}'s state"), items)?;
        }
        Ok(())
    }

    /// Runs `main.lua` (anew), keeping the game's state: its functions,
    /// globals frozen after `on_world_start` (which runs only the first
    /// time).
    fn run_main(&mut self, files: &Files, first: bool) -> Result<()> {
        let path = files.path("main.lua");
        let src = files.read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let code = lex::tokens("main.lua", &src)?;
        *self.sandbox.reading.borrow_mut() = sandbox::Reading::opened(files);
        let started = self.start_main(&src, first);
        let mut reading = self.sandbox.reading.borrow_mut();
        let given = reading.given();
        reading.close();
        let env = started?;
        self.data = std::mem::take(&mut reading.read);
        self.modules = std::mem::take(&mut reading.modules);
        let modules_name_music = reading.names_music;
        let modules_name_daylight = reading.names_daylight;
        drop(reading);
        let kept = self.sandbox.freeze(&env)?;
        (self.env, self.required) = (Some(env), given);
        self.tick = Sandbox::function(&kept, "on_tick")?;
        self.draw = Sandbox::function(&kept, "on_draw")?;
        self.globals = Some(kept);
        self.code = code;
        self.uses_music = modules_name_music || lex::names(&src, &["music", "on_beat"]);
        self.moves_sun = modules_name_daylight || lex::names(&src, &["daylight"]);
        Ok(())
    }

    /// Runs `main.lua`'s top, and the first time `on_world_start` (no
    /// level yet) and then `on_level_start("start")`. A handler by the old
    /// API's name is refused with its new one.
    fn start_main(&mut self, src: &str, first: bool) -> Result<Table> {
        let env = self.sandbox.run("main.lua", src)?;
        let line = |f: &Function| f.info().line_defined.map(|l| format!(":{l}")).unwrap_or_default();
        for (old, new) in OLD_NAMES {
            if let Some(f) = Sandbox::function(&env, old)? {
                anyhow::bail!("main.lua{}: function {old}(...) is the old API's name; it is {new} now (world, level, player, screen and controls are there everywhere, input in on_tick; docs/game-model.md)", line(&f));
            }
        }
        for (old, new) in RENAMED {
            if let Some(f) = Sandbox::function(&env, old)? {
                anyhow::bail!("main.lua{}: function {old} is called {new} now (the same parameters)", line(&f));
            }
        }
        for name in WITHOUT_PARAMETERS {
            if let Some(f) = Sandbox::function(&env, name)?
                && f.info().num_params > 0
            {
                anyhow::bail!("main.lua{}: {name}() takes no parameters: world, level, player, input and screen are there everywhere", line(&f));
            }
        }
        if first {
            // The handlers called from here on (`on_level_end` of an ask in
            // the first `on_level_start`) are found in what main.lua made.
            self.globals = Some(env.clone());
            // A handler's mistake stops the game rather than its loading:
            // mended, it starts afresh (`start_failed`).
            if let Err(e) = self.start_handlers(&env) {
                self.started = false;
                self.halt(e);
            }
        }
        Ok(env)
    }

    /// `on_world_start` (no level yet), then `on_level_start("start")`,
    /// what they removed gone, their states checked.
    fn start_handlers(&mut self, env: &Table) -> Result<()> {
        self.start_states()?;
        if let Some(start) = Sandbox::function(env, "on_world_start")? {
            let lua = &self.sandbox.lua;
            lua.set_named_registry_value("voxcade.phase", "world_start")?;
            let started = self.with_world(|sb, _| sb.call_start::<()>(&start, ()));
            self.sandbox.lua.unset_named_registry_value("voxcade.phase")?;
            started?;
        }
        if let Some(start) = Sandbox::function(env, "on_level_start")? {
            self.with_world(|sb, _| sb.call_start::<()>(&start, "start"))?;
        }
        self.stage.settle();
        self.arrive();
        self.check_states().map_err(|e| anyhow::anyhow!("{e:#} (at the start)"))?;
        self.write_saves();
        // An ask of the first on_level_start counts at once.
        self.change_levels();
        Ok(())
    }

    /// The states a run starts with (`c.state.world`, `c.state.player`),
    /// the engine's keys among the player's written to where the engine
    /// keeps them (`hp`, `score`, the shooter's `weapon`).
    fn start_states(&mut self) -> Result<()> {
        let start = self.stage.host.sim().cfg.state.clone();
        let lua = &self.sandbox.lua;
        let world = self.registry("voxcade.state")?;
        for (key, v) in &start.world {
            world.raw_set(key.as_str(), world::from_toml(lua, v)?)?;
        }
        let player = self.registry("voxcade.player")?;
        let host = &mut self.stage.host;
        for (key, v) in &start.player {
            let engine = host.player_keys().contains(&key.as_str());
            match v {
                toml::Value::Integer(n) if engine => {
                    host.set_player_number(key, *n);
                }
                toml::Value::String(s) if engine => host.set_player_kept(key, host::Kept::Text(s.clone())).map_err(|e| anyhow::anyhow!("c.state.player.{key}: {e}"))?,
                _ if engine => anyhow::bail!("c.state.player.{key}: the engine keeps it, as a whole number"),
                _ => player.raw_set(key.as_str(), world::from_toml(lua, v)?)?,
            }
        }
        Ok(())
    }

    /// The rounds the ready-made rules spend, from the player's inventory
    /// before their tick (the game's code may have changed them).
    fn stock_in(&mut self) -> Result<Vec<(String, u32)>> {
        let stock = self.stage.host.stock();
        if stock.is_empty() {
            return Ok(stock);
        }
        let inventory = self.registry("voxcade.player")?.raw_get::<Option<Table>>("inventory")?;
        let mut read = Vec::with_capacity(stock.len());
        for (item, _) in stock {
            let n = inventory.as_ref().map(|t| t.raw_get::<Option<i64>>(item.as_str())).transpose()?.flatten().unwrap_or(0).max(0) as u32;
            self.stage.host.set_stock(&item, n);
            read.push((item, n));
        }
        Ok(read)
    }

    /// The same for the seats after the first (`p.state.inventory`): what
    /// each seat's rules spend, read in before their tick.
    fn seats_stock_in(&mut self) -> Result<SeatsStock> {
        let mut all = Vec::new();
        for seat in 2..=self.stage.seats.len() {
            let stock = self.stage.host.seat_stock(seat);
            if stock.is_empty() {
                continue;
            }
            let state = self.registry("voxcade.seats")?.raw_get::<Option<Table>>(seat)?;
            let inventory = state.map(|t| t.raw_get::<Option<Table>>("inventory")).transpose()?.flatten();
            let mut read = Vec::with_capacity(stock.len());
            for (item, _) in stock {
                let n = inventory.as_ref().map(|t| t.raw_get::<Option<i64>>(item.as_str())).transpose()?.flatten().unwrap_or(0).max(0) as u32;
                self.stage.host.set_seat_stock(seat, &item, n);
                read.push((item, n));
            }
            all.push((seat, read));
        }
        Ok(all)
    }

    /// What each seat's rules spent, back into its inventory.
    fn seats_stock_out(&mut self, read: &SeatsStock) -> Result<()> {
        for (seat, read) in read {
            let now = self.stage.host.seat_stock(*seat);
            let changed: Vec<&(String, u32)> = now.iter().filter(|(item, n)| read.iter().find(|(i, _)| i == item).is_none_or(|(_, was)| was != n)).collect();
            if changed.is_empty() {
                continue;
            }
            let Some(state) = self.registry("voxcade.seats")?.raw_get::<Option<Table>>(*seat)? else { continue };
            let inventory = match state.raw_get::<Option<Table>>("inventory")? {
                Some(t) => t,
                None => {
                    let t = self.sandbox.lua.create_table()?;
                    state.raw_set("inventory", &t)?;
                    t
                }
            };
            for (item, n) in changed {
                inventory.raw_set(item.as_str(), *n)?;
            }
        }
        Ok(())
    }

    /// What the rules spent, back into the inventory after their tick
    /// (only what changed: an item the player never had stays out).
    fn stock_out(&mut self, read: &[(String, u32)]) -> Result<()> {
        let now = self.stage.host.stock();
        let changed: Vec<&(String, u32)> = now.iter().filter(|(item, n)| read.iter().find(|(i, _)| i == item).is_none_or(|(_, was)| was != n)).collect();
        if changed.is_empty() {
            return Ok(());
        }
        let player = self.registry("voxcade.player")?;
        let inventory = match player.raw_get::<Option<Table>>("inventory")? {
            Some(t) => t,
            None => {
                let t = self.sandbox.lua.create_table()?;
                player.raw_set("inventory", &t)?;
                t
            }
        };
        for (item, n) in changed {
            inventory.raw_set(item.as_str(), *n)?;
        }
        Ok(())
    }

    /// Calls the script's handler `name` with `arg`, if it has one, the
    /// registry's phase saying where the call is (`level_end`,
    /// `world_end`: no more asks there).
    fn call_handler(&mut self, name: &'static str, arg: &'static str, phase: Option<&str>) -> Result<()> {
        let Some(f) = self.globals.as_ref().map(|g| Sandbox::function(g, name)).transpose()?.flatten() else { return Ok(()) };
        let lua = &self.sandbox.lua;
        match phase {
            Some(phase) => lua.set_named_registry_value("voxcade.phase", phase)?,
            None => lua.unset_named_registry_value("voxcade.phase")?,
        }
        let called = if name == "on_level_start" { self.with_world(|sb, _| sb.call_start::<()>(&f, arg)) } else { self.with_world(|sb, _| sb.call::<()>(&f, arg, name)) };
        self.sandbox.lua.unset_named_registry_value("voxcade.phase")?;
        called
    }

    /// Does what the script asked at the tick's end (`docs/game-model.md`,
    /// "Leaving a level"): `on_level_end(why)`, then another level entered
    /// (or this one afresh) and `on_level_start(how)`, or the run's end. An
    /// ask in that `on_level_start` counts at once; a mistake stops the
    /// game.
    /// Runs `f` with the game's files open to `require` again, in the
    /// game's environment (its globals frozen), `require` giving the
    /// modules it has. What a scenario requires anew counts as the game's
    /// code (`code`: its replays run from it); what a console line does
    /// not (the line itself is in the replay, and the fingerprint is
    /// checked before the first tick).
    fn reopened<R>(&mut self, counts: bool, f: impl FnOnce(&mut Self, &Table) -> Result<R>) -> Result<R> {
        let env = self.env.clone().context("the game has not started (mend main.lua first)")?;
        *self.sandbox.reading.borrow_mut() = sandbox::Reading::reopened(&self.files, env.clone(), self.required.clone());
        let done = f(self, &env);
        let mut reading = self.sandbox.reading.borrow_mut();
        self.required = reading.given();
        let (modules, read) = (std::mem::take(&mut reading.modules), std::mem::take(&mut reading.read));
        reading.close();
        drop(reading);
        if counts {
            for file in modules.into_iter().chain(read) {
                if !self.modules.iter().chain(&self.data).any(|(name, _)| *name == file.0) {
                    self.modules.push(file);
                }
            }
        }
        done
    }

    /// What a tick's end does, after a scenario's setup or a console line:
    /// what the script removed goes, the states are checked (one that is
    /// no plain data stops the game), the attacks asked for start, saves
    /// are written and an ask is carried out.
    fn settle_between(&mut self, after: &str) -> Result<()> {
        self.stage.settle();
        if let Err(e) = self.check_states() {
            let text = format!("{e:#} (after {after})");
            self.halt(anyhow::anyhow!(text.clone()));
            self.state_failed.set(true);
            anyhow::bail!(text);
        }
        self.stage.host.start_attacks();
        self.write_saves();
        self.change_levels();
        match self.halted() {
            Some(e) => anyhow::bail!(e),
            None => Ok(()),
        }
    }

    fn change_levels(&mut self) {
        loop {
            if self.halted.borrow().is_some() || self.stage.ended {
                return;
            }
            let Some(ask) = self.stage.ask.take() else { return };
            self.changes += 1;
            if self.changes > 100 {
                self.halt(anyhow::anyhow!("levels keep being left before a tick was played (a hundred times): does on_level_start ask again and again?"));
                return;
            }
            if let Err(e) = self.call_handler("on_level_end", ask.why(), Some("level_end")) {
                // Nothing has changed yet: the ask waits for the file to be mended.
                self.stage.ask = Some(ask);
                self.halt(e);
                return;
            }
            let entered = match ask {
                Ask::Won => return self.end_run(Outcome::Won),
                Ask::Lost => return self.end_run(Outcome::Lost),
                Ask::GoTo { level, place } => match self.stage.levels.find(&level) {
                    Some(index) => {
                        // A checkpoint belongs to the level it was reached in
                        // (every player's).
                        if let Ok(player) = self.registry("voxcade.player") {
                            let _ = player.raw_set("checkpoint", mlua::Value::Nil);
                        }
                        if let Ok(seats) = self.registry("voxcade.seats") {
                            for (_, state) in seats.pairs::<i64, Table>().flatten() {
                                let _ = state.raw_set("checkpoint", mlua::Value::Nil);
                            }
                        }
                        let place = self.place_in(index, place);
                        self.enter(index, place, "go_to", None)
                    }
                    None => Err(anyhow::anyhow!("world:go_to: the level '{level}' is gone (its file was taken away while playing)")),
                },
                Ask::Restart { place } => {
                    let checkpoint = self.registry("voxcade.player").ok().and_then(|p| p.raw_get::<Option<String>>("checkpoint").ok().flatten());
                    let level = &self.stage.levels.list[self.stage.level];
                    // A checkpoint is a place, or a thing of the level by its
                    // name (stood at once it is there again).
                    let (at_place, at_thing) = match checkpoint {
                        Some(c) if level.places.iter().any(|p| p.name == c) => (Some(c), None),
                        Some(c) => (None, Some(c)),
                        None => (None, None),
                    };
                    let thing = at_thing.filter(|_| place.is_none());
                    let place = self.place_in(self.stage.level, place.or(at_place));
                    self.arrive_at = thing;
                    self.enter(self.stage.level, place, "restart", None)
                }
                Ask::Load { slot: None } => match self.start_text.take() {
                    Some(text) => self.load_save(&text).context("loading a save"),
                    None => Err(anyhow::anyhow!("no save to go on from")),
                },
                // A replay's load reads what its run read then.
                Ask::Load { slot: Some(n) } => match self.stage.replayed.pop_front().or_else(|| self.stage.slot_texts.get(&n).cloned()) {
                    Some(text) => {
                        self.stage.loaded.push(text.clone());
                        self.load_save(&text).with_context(|| format!("loading the save in slot {n}"))
                    }
                    None => Err(anyhow::anyhow!("world:load: slot {n} is empty")),
                },
            };
            if let Err(e) = entered {
                self.halt(e);
                return;
            }
        }
    }

    /// The place called `name` in level `index` (without a name, its place
    /// called start); None: it has none, the player starts where the
    /// level's rules stand it.
    fn place_in(&self, index: usize, name: Option<String>) -> Option<crate::levels::Place> {
        let name = name.unwrap_or_else(|| "start".into());
        self.stage.levels.list[index].places.iter().find(|p| p.name == name).cloned()
    }

    /// Enters level `index` (`how` it came to be entered), the player at
    /// `place` (or where the level starts): its things from its file,
    /// `level.state` fresh (or a save's, with the voxels its file would not
    /// bring back), then `on_level_start(how)`.
    fn enter(&mut self, index: usize, place: Option<crate::levels::Place>, how: &'static str, loaded: Option<Loaded>) -> Result<()> {
        let level = self.stage.levels.list[index].clone();
        let arrived = place.clone().unwrap_or_else(|| self.stage.start(index));
        self.stage.level = index;
        let mut world = level.world.clone();
        if let Some(l) = &loaded {
            for &(i, colour) in &l.palette {
                world.palette[i] = colour;
            }
            for &(p, v) in &l.voxels {
                world.set(p, v);
            }
        }
        match self.kit_mut() {
            Some(kit) => kit.enter_level(world, &level.markers, &level.places, place.map(|p| (p.at, p.facing))),
            None => {
                // The player's figure goes along; the level's things are the
                // new level's.
                let h = self.plain_mut().expect("a plain host without a kit");
                let player = h.player;
                let figures: Vec<Id> = player.into_iter().chain(h.seats.iter().flatten().copied()).collect();
                h.things.retain(|t| figures.contains(&t.id));
                (h.focus, h.looking) = (player, false);
                h.sim.replace_world(world);
                Self::place(h, &level.markers);
                Self::stand_player(h, &arrived);
                (h.sim.cam_target, h.sim.cam_prev) = (arrived.at, arrived.at);
            }
        }
        // A player arriving with no hit points left gets the start's back
        // (`c.state.player.hp`).
        if self.kit().is_some()
            && let Some(hp) = self.stage.host.sim().cfg.state.player_number("hp")
            && self.stage.host.player_number("hp").is_some_and(|n| n <= 0)
        {
            self.stage.host.set_player_number("hp", hp);
        }
        let sim = self.stage.host.sim_mut();
        (sim.freeze, sim.shake, sim.screen_flash) = (0, 0.0, 0.0);
        sim.flares.clear();
        self.stage.arrived = arrived;
        // The seats after the first at their arrival places (`NAME_n`), or
        // where a save stood them.
        let saved = std::mem::take(&mut self.seat_places);
        for n in 2..=self.stage.seats.len() {
            let place = saved.iter().find(|(m, _)| *m == n).map_or_else(|| self.arrival(n), |(_, p)| p.clone());
            if let Some(id) = self.stage.figure(n)
                && let Some(h) = self.plain_mut()
            {
                Self::stand_figure(h, id, &place);
            }
        }
        if how != "reload" && how != "load" {
            self.stage.visits[index] += 1;
        }
        self.stage.gone.clear();
        // The level's things are new: nothing touches, nothing died yet.
        (self.stage.touching, self.stage.dead, self.stage.graveyard, self.stage.placed, self.stage.later, self.stage.killers) = Default::default();
        self.spawn_pieces();
        self.stage.name_things();
        // A save's lights as they were, by the pieces' names.
        if let Some(l) = &loaded {
            for (name, brightness) in &l.lights {
                if let Some(id) = self.stage.named(name).filter(|id| self.stage.lights.contains_key(id)) {
                    self.stage.lights.insert(id, brightness.clamp(0.0, 1.0));
                }
            }
        }
        // Arriving at a thing (a checkpoint by its name): stood where it is.
        if let Some(name) = self.arrive_at.take()
            && let Some(at) = self.stage.named(&name).and_then(|id| self.stage.seen(id)).map(|t| t.body.pos)
        {
            let place = crate::levels::Place::new(&name, at, None);
            match self.plain_mut() {
                Some(h) => Self::stand_player(h, &place),
                None => {
                    let _ = self.stage.host.put_player(at);
                }
            }
            self.stage.arrived = place;
        }
        // Started over, the other players at their own checkpoints (a place,
        // or a thing by its name), if they passed one.
        if how == "restart" {
            for n in 2..=self.stage.seats.len() {
                let checkpoint = self.registry("voxcade.seats").ok().and_then(|t| t.raw_get::<Option<Table>>(n).ok().flatten()).and_then(|t| t.raw_get::<Option<String>>("checkpoint").ok().flatten());
                let Some(name) = checkpoint else { continue };
                let place = level.places.iter().find(|p| p.name == name).cloned().or_else(|| {
                    let at = self.stage.named(&name).and_then(|id| self.stage.seen(id)).map(|t| t.body.pos)?;
                    Some(crate::levels::Place::new(&name, at, None))
                });
                if let (Some(place), Some(id)) = (place, self.stage.figure(n))
                    && let Some(h) = self.plain_mut()
                {
                    Self::stand_figure(h, id, &place);
                }
            }
        }
        let lua = &self.sandbox.lua;
        for name in ["voxcade.level", "voxcade.data", "voxcade.views", "voxcade.props"] {
            lua.set_named_registry_value(name, lua.create_table()?)?;
        }
        if let Some(l) = loaded {
            lua.set_named_registry_value("voxcade.level", value::restore(lua, &l.level_state)?)?;
        }
        self.call_handler("on_level_start", how, None)?;
        self.stage.settle();
        self.seat_figures(true);
        self.built();
        self.arrive();
        // Without them, the camera over the thing it follows, as at the start.
        if let Some(b) = self.focus_body() {
            let target = b.pos + Vec3::Z * b.height * 0.5;
            let sim = self.stage.host.sim_mut();
            (sim.cam_target, sim.cam_prev) = (target, target);
        }
        self.check_states().map_err(|e| anyhow::anyhow!("{e:#} (after on_level_start)"))?;
        // A world:save of on_level_start keeps this level, before an ask
        // of it goes on.
        self.write_saves();
        Ok(())
    }

    /// The run ends (`won` or `lost`, `on_level_end` told already):
    /// `on_world_end(result)`, then the outcome is the host's.
    fn end_run(&mut self, outcome: Outcome) {
        if self.stage.ended {
            return;
        }
        self.stage.ended = true;
        let result = match outcome {
            Outcome::Won => "won",
            Outcome::Lost => "lost",
        };
        if let Err(e) = self.call_handler("on_world_end", result, Some("world_end")) {
            self.halt(e);
        }
        self.stage.host.end(outcome);
    }

    fn registry(&self, name: &str) -> Result<Table> {
        Ok(self.sandbox.lua.named_registry_value(name)?)
    }

    /// `world.state`, `player.state`, `level.state` and every
    /// `thing.state` as plain data (the script's part of them); things only
    /// in the states that cannot outlive them.
    fn capture(&self) -> Result<Captured> {
        let mut seen = value::Seen::default();
        let mut root = |name: &str, path: &str, things: bool| -> Result<Option<Value>> {
            Ok(value::capture(&mlua::Value::Table(self.registry(name)?), path, &mut seen, things)?.filter(|v| !v.is_empty()))
        };
        let state = root("voxcade.state", "world.state", false)?;
        let player = root("voxcade.player", "player.state", false)?;
        let level = root("voxcade.level", "level.state", true)?;
        let mut kept = Vec::new();
        for pair in self.registry("voxcade.kept")?.pairs::<String, mlua::Value>() {
            let (name, v) = pair?;
            if let Some(v) = value::capture(&v, &format!("level.kept of {name}"), &mut seen, false)?
                && !v.is_empty()
            {
                kept.push((name, v));
            }
        }
        kept.sort_by(|a, b| a.0.cmp(&b.0));
        let mut data = Vec::new();
        for pair in self.registry("voxcade.data")?.pairs::<u32, mlua::Value>() {
            let (id, v) = pair?;
            if let Some(v) = value::capture(&v, &format!("thing {id}'s state"), &mut seen, true)?
                && !v.is_empty()
            {
                data.push((id, v));
            }
        }
        data.sort_by_key(|(id, _)| *id);
        let mut seats = Vec::new();
        for n in 2..=self.stage.seats.len() {
            let table: Option<Table> = self.registry("voxcade.seats")?.raw_get(n)?;
            if let Some(t) = table
                && let Some(v) = value::capture(&mlua::Value::Table(t), &format!("player {n}'s state"), &mut seen, false)?
            {
                seats.push((n, v));
            }
        }
        Ok(Captured { state, player, level, kept, data, seats })
    }

    /// Forgets the state of things that are gone, and its view.
    fn prune(&self) -> Result<()> {
        let data = self.registry("voxcade.data")?;
        let views = self.registry("voxcade.views")?;
        let host = &self.stage.host;
        let gone: Vec<u32> = data.pairs::<u32, mlua::Value>().filter_map(|p| p.ok()).map(|(id, _)| id).filter(|id| host.get(Id::from_raw(*id)).is_none()).collect();
        for id in gone {
            data.raw_set(id, mlua::Value::Nil)?;
            views.raw_set(id, mlua::Value::Nil)?;
        }
        Ok(())
    }

    /// The kinds of things the config defines, their heights from their
    /// models (`old`: kept from there while a kind's model stays).
    fn kinds(cfg: &GameConfig, height: impl Fn(&str, Option<&str>) -> f32) -> Result<Vec<Kind>> {
        let defs = things::defs(cfg, &Self::behaviours(cfg))?;
        // A family is asked for by its name where a kind's would do
        // (`level:things("enemies")`): never a kind's name.
        for d in &defs {
            if let Some(f) = d.families.iter().find(|f| defs.iter().any(|k| &k.name == *f)) {
                anyhow::bail!("c.things.{}.family: '{f}' is a kind's name; a family needs one of its own", d.name);
            }
        }
        // The players' figures of ready-made rules that play several (the
        // shooter's, of kinds without behaviour) are theirs, not the
        // script's: their rules move them and keep their hit points.
        let figures: Vec<String> = match &cfg.player.kind {
            Some(k) if with::<K>(cfg) && cfg.players > 1 && K::plays_several(cfg) => k.names().into_iter().map(str::to_string).collect(),
            _ => Vec::new(),
        };
        Ok(defs
            .into_iter()
            .map(|d| {
                let height = height(&d.name, d.model.as_deref());
                let sound = d.sound.map(|s| crate::names::intern(&s));
                let figure = figures.contains(&d.name);
                Kind { name: d.name, model: d.model, height, hp: d.hp.unwrap_or(0), has_hp: d.hp.is_some() || d.behaviour.is_some() || figure, scripted: d.behaviour.is_none() && !figure, props: d.props, families: d.families, effect: d.effect, sound, reach: d.reach, team: d.team, free_turn: d.free_turn, spin: d.spin, fixed: d.fixed, frames: d.frames, swing: d.swing }
            })
            .collect())
    }

    /// The sounds' names: the kit's (or the engine's) and the game's own
    /// files'.
    fn sound_names(cfg: &GameConfig) -> Arc<Vec<&'static str>> {
        Arc::new(crate::audio::sound::all(&Self::sounds(cfg), &cfg.sounds).into_iter().map(|(n, ..)| n).collect())
    }

    /// Places what the level's markers stand for, lifted out of anything
    /// solid.
    fn place(host: &mut Plain, markers: &[Marker]) {
        let half = host.sim.half();
        for m in markers {
            let height = host.kinds[m.kind].height;
            let mut at = m.cell.as_vec3() + Vec3::new(0.5, 0.5, 0.0);
            while body_hits(&host.sim.world, at, half, height) && at.z < host.sim.world.size.z as f32 + 8.0 {
                at.z += 1.0;
            }
            host.spawn(m.kind, at.x, at.y, Some(at.z)).expect("a plain host places every kind");
        }
    }

    /// Sets down the pieces of the level being played (`grid::PIECE`):
    /// each a thing of the piece kind standing exactly where the file puts
    /// its pivot, as tall as its model, facing its way, its light on. The
    /// camera keeps to what it followed.
    fn spawn_pieces(&mut self) {
        let level = self.stage.levels.list[self.stage.level].clone();
        (self.stage.pieces, self.stage.lights) = Default::default();
        let Some(kind) = self.stage.kinds.iter().position(|k| k.name == crate::grid::PIECE) else { return };
        let focus = self.plain().map(|h| h.focus);
        for p in &level.pieces {
            let at = p.cell.as_vec3() + Vec3::new(0.5, 0.5, 0.0);
            let Ok(id) = self.stage.host.spawn(kind, at.x, at.y, Some(at.z)) else { continue };
            let height = p.model.as_ref().and_then(|m| self.stage.heights.get(m)).copied().unwrap_or(1.0);
            let bearing = p.facing.bearing() as f32;
            let (sin, cos) = crate::sim::math::sin_cos(bearing.to_radians());
            if let Some((body, _)) = self.stage.host.get_mut(id).and_then(|t| t.moved) {
                body.height = height;
                // Its way exactly where it is one of the four (no rounding
                // off the grid), else its bearing.
                let way = match p.facing {
                    crate::grid::Facing::Way(w) => glam::IVec2::from(w.pair()).as_vec2().extend(0.0),
                    crate::grid::Facing::Turn(_) => Vec3::new(sin, cos, 0.0),
                };
                body.face(way);
                body.prev = body.pos;
            }
            if !p.lights.is_empty() {
                self.stage.lights.insert(id.raw(), 1.0);
            }
            self.stage.pieces.insert(id.raw(), p.clone());
        }
        if let (Some(h), Some(focus)) = (self.plain_mut(), focus) {
            h.focus = focus;
        }
    }

    /// The kind of the player's figure (`c.player.kind`), for a game
    /// without ready-made rules (theirs have their own player).
    fn player_kind(cfg: &GameConfig, kinds: &[Kind]) -> Result<Option<usize>> {
        let Some(seats) = &cfg.player.kind else { return Ok(None) };
        // Ready-made rules that play several take a kind for each seat's
        // figure (seat 1's their own from the level); the others their own.
        let kit = with::<K>(cfg);
        if kit && (cfg.players <= 1 || !K::plays_several(cfg)) {
            anyhow::bail!("c.player.kind = \"{}\": the ready-made rules (c.kit) bring their own player", seats.of(1));
        }
        for name in seats.names() {
            if name == crate::grid::PIECE || !kinds.iter().any(|k| k.name == name) {
                let names: Vec<&str> = kinds.iter().map(|k| k.name.as_str()).filter(|n| *n != crate::grid::PIECE).collect();
                anyhow::bail!("c.player.kind = \"{name}\": no kind of thing called that (c.things: {})", names.join(", "));
            }
        }
        Ok(kinds.iter().position(|k| k.name == seats.of(1)).filter(|_| !kit))
    }

    /// A kind's `sound` is one the game has (its files in sounds/, the
    /// kit's).
    fn check_kinds_sounds(cfg: &GameConfig, kinds: &[Kind]) -> Result<()> {
        let sounds = Self::sound_names(cfg);
        for k in kinds {
            if let Some(s) = k.sound.filter(|s| !sounds.contains(s)) {
                anyhow::bail!("c.things.{}.sound = \"{s}\": no sound called that (a sound is a file, sounds/{s}.toml; the game's and the ready-made rules': {})", k.name, sounds.join(", "));
            }
        }
        Ok(())
    }

    /// The seats (`c.players`) and the hold (`c.camera.hold`) are of a size
    /// the engine plays; the ready-made rules play one player yet.
    fn check_players(cfg: &GameConfig) -> Result<()> {
        if !(1..=crate::config::MOST_PLAYERS).contains(&cfg.players) {
            anyhow::bail!("c.players = {}: 1 to {}", cfg.players, crate::config::MOST_PLAYERS);
        }
        if cfg.players > 1 && with::<K>(cfg) && !K::plays_several(cfg) {
            anyhow::bail!("c.players = {}: the ready-made rules (c.kit = \"{}\") play one player so far", cfg.players, cfg.kit.as_deref().unwrap_or(""));
        }
        if let Some(hold) = &cfg.camera.hold
            && (hold.is_empty() || hold.len() > 2 || hold.iter().any(|h| h.is_nan() || *h <= 0.0))
        {
            anyhow::bail!("c.camera.hold: how far apart the players may be, across and deep ({{ 240, 150 }}, or {{ 240 }}), each above 0");
        }
        Ok(())
    }

    /// Every item holds at least one; the save slots are a menu's worth.
    fn check_items(cfg: &GameConfig) -> Result<()> {
        for (name, item) in &cfg.items {
            if item.stack == 0 {
                anyhow::bail!("c.items.{name}.stack = 0: an inventory holds at least one");
            }
        }
        for (name, icon) in &cfg.icons {
            icon.check(&format!("c.icons.{name}")).map_err(anyhow::Error::msg)?;
        }
        if cfg.saves.slots > MAX_SLOTS {
            anyhow::bail!("c.saves.slots = {}: at most {MAX_SLOTS}", cfg.saves.slots);
        }
        Ok(())
    }

    /// Stands the player's figure (`c.player.kind`) at `place`, its feet
    /// at the place's height, facing its way if it has one.
    fn stand_player(h: &mut Plain, place: &crate::levels::Place) {
        if let Some(id) = h.player {
            Self::stand_figure(h, id, place);
        }
    }

    /// Stands a figure at `place`, its feet at the place's height, facing
    /// its way if it has one.
    fn stand_figure(h: &mut Plain, id: Id, place: &crate::levels::Place) {
        let half = h.sim.half();
        let Some(t) = h.things.get_mut(id) else { return };
        let at = crate::sim::stand_at(&h.sim.world, place.at, t.body.height, half);
        (t.body.pos, t.body.prev, t.body.vel) = (at, at, Vec3::ZERO);
        if let Some(way) = place.facing {
            t.body.face(way.as_vec2().extend(0.0));
        }
    }

    /// A world the script builds (`c.world.size`) was empty when the player
    /// was placed: it stands on what `on_level_start` built.
    fn built(&mut self) {
        if self.stage.host.sim().cfg.world.size.is_none() {
            return;
        }
        if let Some(kit) = self.kit_mut() {
            return kit.built();
        }
        let h = self.plain_mut().expect("a plain host without a kit");
        let Some(id) = h.player else { return };
        let half = h.sim.half();
        let Some(t) = h.things.get_mut(id) else { return };
        while body_hits(&h.sim.world, t.body.pos, half, t.body.height) && t.body.pos.z < h.sim.world.size.z as f32 + 8.0 {
            t.body.pos.z += 1.0;
        }
        t.body.prev = t.body.pos;
    }

    /// A line for OUTPUT (the IDE's), not a mistake.
    fn note(&self, text: &str) {
        if let Ok(note) = self.sandbox.lua.named_registry_value::<Function>("voxcade.note") {
            let _ = note.call::<()>(text.to_string());
        }
    }

    /// The game as a save (`save::SaveData`): the states, every level's
    /// kept and visits, the level being played and its voxels changed,
    /// where the player stands, the engine's numbers and the ready-made
    /// rules' run, the time and the random generator.
    fn save_data(&self) -> Result<save::SaveData> {
        let captured = self.capture()?;
        let write = |v: &Option<Value>| v.as_ref().map_or_else(|| "{}".to_string(), data::write);
        let stage = &self.stage;
        let level = &stage.levels.list[stage.level];
        let host = &stage.host;
        let sim = host.sim();
        let changed = sim.world.changed_from(&level.world);
        let palette = (1..256)
            .filter(|&i| sim.world.palette[i] != level.world.palette[i])
            .map(|i| {
                let [r, g, b, a] = sim.world.palette[i];
                (i.to_string(), format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
            })
            .collect();
        let figure = host.player_seen().map(|t| save::Figure { at: t.body.pos.to_array(), facing: t.body.look.to_array() });
        // The seats someone sat on (seat 1's own keys where set).
        let seats = (1..=stage.seats.len())
            .filter(|&n| {
                let s = &stage.seats[n - 1];
                (n > 1 && s.been) || s.out || s.team.is_some() || s.kind.is_some()
            })
            .map(|n| {
                let s = &stage.seats[n - 1];
                let standing = (n > 1).then(|| stage.seen(host::seat_handle(n))).flatten();
                save::SavedSeat {
                    seat: n,
                    taken: s.taken,
                    state: captured.seats.iter().find(|(m, _)| *m == n).map(|(_, v)| data::write(v)).unwrap_or_default(),
                    score: s.score,
                    hp: standing.as_ref().map(|t| t.hp).or(s.hp),
                    out: s.out,
                    team: s.team.clone(),
                    kind: s.kind.clone(),
                    figure: standing.map(|t| save::Figure { at: t.body.pos.to_array(), facing: t.body.look.to_array() }),
                }
            })
            .collect();
        Ok(save::SaveData {
            seats,
            format: save::FORMAT,
            game: sim.cfg.title.clone(),
            version: sim.cfg.saves.version,
            level: level.name.clone(),
            ticks: sim.tick,
            rng: format!("{:016x}", sim.rng.state()),
            states: save::States { world: write(&captured.state), player: write(&captured.player), level: write(&captured.level) },
            kept: captured.kept.iter().map(|(n, v)| (n.clone(), data::write(v))).collect(),
            visits: stage.levels.list.iter().zip(&stage.visits).filter(|(_, v)| **v > 0).map(|(l, v)| (l.name.clone(), *v)).collect(),
            numbers: host.player_keys().iter().filter_map(|k| host.player_number(k).map(|n| (k.to_string(), n))).collect(),
            figure,
            run: self.kit().map(|k| k.run()).unwrap_or_default(),
            voxels: save::write_voxels(&changed, sim.world.size),
            size: sim.world.size.to_array(),
            palette,
            lights: stage.lights.iter().filter(|(id, _)| host.get(Id::from_raw(**id)).is_some()).filter_map(|(id, b)| Some((stage.names.get(id)?.clone(), *b))).collect(),
        })
    }

    /// Makes the saves `world:save` asked for, as the game stands now (the
    /// tick's end, the end of an `on_level_start`): the script reads them
    /// at once (`world.saves`, `world:load`), the session writes them into
    /// the slots. A game stopped by a mistake is in no state to keep.
    fn write_saves(&mut self) {
        let slots = std::mem::take(&mut self.stage.saving);
        if slots.is_empty() || self.halted.borrow().is_some() {
            return;
        }
        let saved = self.save_data().and_then(|data| Ok((data.write()?, data)));
        let (text, data) = match saved {
            Ok(saved) => saved,
            Err(e) => return self.halt(e.context("world:save")),
        };
        for n in slots {
            let info = crate::saves::SlotInfo { level: data.level.clone(), ticks: data.ticks, when: 0 };
            self.stage.slots[n as usize - 1] = Some(info);
            self.stage.slot_texts.insert(n, Arc::from(text.as_str()));
            self.written.push((n, text.clone()));
        }
    }

    /// Goes on from a save (`docs/game-model.md`, "Save games"): its states,
    /// kept and visits, time and random generator, the player's numbers and
    /// the ready-made rules' run; `on_save_loaded(version)`; then its level
    /// entered (`"load"`) with its
    /// `level.state`, the player where it stood. Names the game no longer
    /// has (a level, an item) are dropped with a note.
    fn load_save(&mut self, text: &str) -> Result<()> {
        let save = save::SaveData::read(text)?;
        let lua = self.sandbox.lua.clone();
        let read = |text: &str, what: &str| -> Result<Value> { data::read(text).with_context(|| format!("the save's {what}")) };
        // Its level, as it stood; one gone since: the first, as it starts.
        let found = self.stage.levels.find(&save.level);
        if found.is_none() {
            self.note(&format!("the save's level '{}' is gone: the first level instead, from its start", save.level));
        }
        let index = found.unwrap_or(0);
        let (world_state, player_state) = (read(&save.states.world, "world.state")?, read(&save.states.player, "player.state")?);
        let level_state = if found.is_some() { read(&save.states.level, "level.state")? } else { Value::Table(Vec::new()) };
        let mut kept = Vec::new();
        for (name, text) in &save.kept {
            match self.stage.levels.find(name) {
                Some(_) => kept.push((name.clone(), read(text, &format!("level.kept of {name}"))?)),
                None => self.note(&format!("the save's level.kept of '{name}' is dropped: the level is gone")),
            }
        }
        // The voxels its file would not bring back, where the level is the
        // one they were counted in (the same size).
        let size = self.stage.levels.list[index].world.size;
        let fits = found.is_some() && size.to_array() == save.size;
        if found.is_some() && !fits && !save.voxels.is_empty() {
            self.note(&format!("the level '{}' changed its size since the save: what was blown away there is back", save.level));
        }
        let voxels = if fits { save::read_voxels(&save.voxels, size)? } else { Vec::new() };
        let mut palette = Vec::new();
        for (i, colour) in save.palette.iter().filter(|_| fits) {
            let hex = colour.strip_prefix('#').filter(|h| h.len() == 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
            match (i.parse::<usize>().ok().filter(|i| (1..256).contains(i)), hex) {
                (Some(i), Some(hex)) => palette.push((i, [0, 2, 4, 6].map(|k| u8::from_str_radix(&hex[k..k + 2], 16).expect("hex digits")))),
                _ => anyhow::bail!("the save's palette: {i} = \"{colour}\" does not read"),
            }
        }
        let rng = u64::from_str_radix(&save.rng, 16).context("the save's random generator does not read")?;
        // The states; things in them came back as nil (they live in their
        // level, which comes from its file).
        lua.set_named_registry_value("voxcade.state", value::restore(&lua, &world_state)?)?;
        lua.set_named_registry_value("voxcade.player", value::restore(&lua, &player_state)?)?;
        let all = lua.create_table()?;
        for (name, v) in &kept {
            all.raw_set(name.as_str(), value::restore(&lua, v)?)?;
        }
        lua.set_named_registry_value("voxcade.kept", all)?;
        lua.set_named_registry_value("voxcade.views", lua.create_table()?)?;
        let items = self.stage.host.sim().cfg.items.clone();
        if let Ok(inventory) = self.registry("voxcade.player")?.raw_get::<Table>("inventory") {
            let names: Vec<String> = inventory.pairs::<String, mlua::Value>().filter_map(|p| p.ok()).map(|(n, _)| n).collect();
            for name in names.into_iter().filter(|n| !items.contains_key(n)) {
                inventory.raw_set(name.as_str(), mlua::Value::Nil)?;
                self.note(&format!("the save's item '{name}' is dropped: c.items has it no more"));
            }
        }
        self.stage.visits = self.stage.levels.list.iter().map(|l| save.visits.get(&l.name).copied().unwrap_or(0)).collect();
        // The level played was entered at least once.
        self.stage.visits[index] = self.stage.visits[index].max(1);
        let sim = self.stage.host.sim_mut();
        sim.tick = save.ticks;
        sim.rng = crate::rng::Rng::from_state(rng);
        if let Some(kit) = self.kit_mut() {
            kit.set_run(&save.run);
        }
        for (key, n) in &save.numbers {
            self.stage.host.set_player_number(key, *n);
        }
        // Every seat's own back; a seat saved but away now waits for
        // whoever takes it (it comes back with it).
        let seats = self.registry("voxcade.seats")?;
        for saved in &save.seats {
            let Some(s) = self.stage.seats.get_mut(saved.seat.wrapping_sub(1)) else {
                self.note(&format!("the save's player {} is dropped: the game has {} seats (c.players)", saved.seat, self.stage.seats.len()));
                continue;
            };
            (s.out, s.team, s.kind) = (saved.out, saved.team.clone(), saved.kind.clone());
            if saved.seat > 1 {
                (s.been, s.score, s.hp) = (true, saved.score, saved.hp);
                let state = if saved.state.is_empty() { Value::Table(Vec::new()) } else { read(&saved.state, &format!("player {}'s state", saved.seat))? };
                seats.raw_set(saved.seat, value::restore(&lua, &state)?)?;
                if let Some(f) = saved.figure.as_ref().filter(|_| found.is_some()) {
                    self.seat_places.push((saved.seat, crate::levels::Place { name: "saved".into(), at: Vec3::from(f.at), facing: Some(glam::IVec2::from(f.facing)) }));
                }
            }
        }
        // The script hears every load, with the version of the game that
        // made the save (an older one it brings up to date), before its
        // level is built.
        if let Some(f) = self.globals.as_ref().map(|g| Sandbox::function(g, "on_save_loaded")).transpose()?.flatten() {
            lua.set_named_registry_value("voxcade.phase", "world_start")?;
            let called = self.with_world(|sb, _| sb.call::<()>(&f, save.version, "on_save_loaded"));
            lua.unset_named_registry_value("voxcade.phase")?;
            called?;
        }
        let place = save.figure.as_ref().filter(|_| found.is_some()).map(|f| crate::levels::Place { name: "saved".into(), at: Vec3::from(f.at), facing: Some(glam::IVec2::from(f.facing)) });
        let lights = if found.is_some() { save.lights.iter().map(|(n, b)| (n.clone(), *b)).collect() } else { Vec::new() };
        self.enter(index, place, "load", Some(Loaded { level_state, voxels, palette, lights }))
    }

    /// The handle id an event names: the player's for its figure.
    fn fold(&self, who: crate::sim::Who) -> u32 {
        match who {
            crate::sim::Who::Player => PLAYER,
            crate::sim::Who::Thing(id) => self.stage.figure_handle(id).unwrap_or(id.raw()),
        }
    }

    /// Whether the level is held (`level.state.held` set, not false).
    fn held(&self) -> bool {
        self.registry("voxcade.level").ok().and_then(|t| t.raw_get::<mlua::Value>("held").ok()).is_some_and(|v| !matches!(v, mlua::Value::Nil | mlua::Value::Boolean(false)))
    }

    /// Whether the script has a handler called `name`.
    fn handles(&self, name: &str) -> bool {
        self.globals.as_ref().and_then(|g| Sandbox::function(g, name).ok().flatten()).is_some()
    }

    /// Calls the script's handler `name` (if it has one) with `args`.
    fn tell(&mut self, name: &'static str, args: Vec<Told>) -> Result<()> {
        let Some(f) = self.globals.as_ref().map(|g| Sandbox::function(g, name)).transpose()?.flatten() else { return Ok(()) };
        self.sandbox.lua.unset_named_registry_value("voxcade.phase")?;
        self.with_world(|sb, _| {
            let lua = &sb.lua;
            let mut values = Vec::new();
            for arg in args {
                values.push(match arg {
                    Told::Thing(id) => mlua::Value::UserData(world::handle(lua, id)?),
                    Told::Nil => mlua::Value::Nil,
                    Told::Int(n) => mlua::Value::Integer(n),
                    Told::Str(s) => mlua::Value::String(lua.create_string(s)?),
                });
            }
            sb.call::<()>(&f, mlua::MultiValue::from_vec(values), name)
        })
    }

    /// What happened to things since the last tick's events, told to the
    /// script in order (`docs/game-model.md`, "A tick, in order", step 3):
    /// deaths the script caused by setting hit points (and things back to
    /// life), what the ready-made rules and `thing:hurt`/`thing:take` did
    /// (`on_hit`, `on_death`, `on_fall_out`, `on_pickup`), touches that began and
    /// ended, and the action button on what the player touches
    /// (`on_interact`). Returns the action that press used up.
    fn tell_events(&mut self, input: &Input) -> Result<Option<usize>> {
        // Things back to life.
        let stage = &mut self.stage;
        let alive = |stage: &Stage, id: u32| stage.seen(id).is_some_and(|t| t.hp > 0);
        let back: Vec<u32> = stage.dead.iter().copied().filter(|&id| alive(stage, id)).collect();
        for id in back {
            stage.dead.remove(&id);
        }
        // What the script did to things last tick (thing:hurt, thing:take),
        // then the script's things whose hit points ran out, then what the
        // ready-made rules did this tick.
        let later = std::mem::take(&mut self.stage.later);
        let ruled = std::mem::take(&mut self.stage.host.sim_mut().events);
        let mut events = later.into_iter();
        for event in events.by_ref() {
            self.tell_event(event)?;
        }
        let stage = &mut self.stage;
        let mut died = Vec::new();
        for id in stage.host.ids() {
            let Some(t) = stage.host.get(id) else { continue };
            let raw = stage.figure_handle(id).unwrap_or(id.raw());
            let kind = &stage.kinds[t.kind];
            if kind.scripted && kind.has_hp && t.hp <= 0 && !stage.dead.contains(&raw) {
                died.push((raw, t.kind, t.body.pos, t.body.look));
            }
        }
        // A hurt it lived through names nobody's later death.
        self.stage.killers.retain(|id, _| died.iter().any(|d| d.0 == *id));
        for (id, kind, at, look) in died {
            // Its kind's effect and sound, as for the ready-made rules' dead
            // (a player's figure stays).
            if host::handle_seat(id).is_none() {
                self.stage.goes(kind, at, look);
            }
            self.stage.dead.insert(id);
            // Who did it, where thing:hurt did (not hit points set by hand).
            let (by, weapon) = self.stage.killers.remove(&id).unwrap_or_default();
            let by = by.filter(|&b| self.stage.seen(b).is_some() || host::handle_seat(b).is_some()).map_or(Told::Nil, Told::Thing);
            self.tell("on_death", vec![Told::Thing(id), by, weapon.map_or(Told::Nil, Told::Str)])?;
        }
        for event in ruled {
            self.tell_event(event)?;
        }
        // Touches: begun and ended (not with a thing gone or dead).
        let interact = self.actions.iter().position(|a| *a == "interact");
        let pressed = interact.filter(|&a| input.held(a) && !self.before.held(a));
        if self.held() {
            return Ok(None);
        }
        let any_pressed = pressed.is_some() || interact.is_some_and(|a| self.stage.seats.iter().skip(1).any(|s| s.input.held(a) && !s.before.held(a)));
        let found = self.handles("on_touch") || self.handles("on_touch_end") || any_pressed;
        if found {
            let now = self.stage.contacts();
            let ended: Vec<(u32, u32)> = self.stage.touching.difference(&now).copied().collect();
            let placed = std::mem::take(&mut self.stage.placed);
            // How each began, judged from where things stood when touches
            // were last looked for, before anything is told.
            let begun: Vec<(u32, u32, &'static str)> = now
                .difference(&self.stage.touching)
                .filter(|(a, b)| !placed.contains(a) && !placed.contains(b))
                .map(|&(a, b)| match self.stage.met(a, b) {
                    // Of two players, the one that came down onto the other.
                    host::Met::Bottom if host::handle_seat(b).is_some() => (b, a, host::Met::Top.word()),
                    met => (a, b, met.word()),
                })
                .collect();
            self.stage.touching = now;
            self.stage.note_positions();
            for (a, b, met) in begun {
                self.tell("on_touch", vec![Told::Thing(a), Told::Thing(b), Told::Str(met.to_string())])?;
            }
            for (a, b) in ended {
                let there = |id: u32| self.stage.seen(id).is_some() && !self.stage.dead.contains(&id) && !self.stage.gone.contains(&id) && !self.stage.graveyard.iter().any(|(g, ..)| *g == id);
                if there(a) && there(b) {
                    self.tell("on_touch_end", vec![Told::Thing(a), Told::Thing(b)])?;
                }
            }
        }
        // The action button on the nearest thing each player touches, seat
        // by seat; the press used up for its seat.
        let interact = self.actions.iter().position(|a| *a == "interact");
        let mut used = None;
        for n in 1..=self.stage.seats.len() {
            let pressed = match n {
                1 => pressed,
                _ => interact.filter(|&a| self.stage.seats[n - 1].input.held(a) && !self.stage.seats[n - 1].before.held(a)),
            };
            let Some(action) = pressed.filter(|_| self.stage.playing(n)) else { continue };
            if self.interact(host::seat_handle(n))? {
                match n {
                    1 => used = Some(action),
                    _ => self.stage.seats[n - 1].before.hold(action, true),
                }
            }
        }
        Ok(used)
    }

    /// `on_interact` for the nearest thing player `who` touches, if any.
    fn interact(&mut self, who: u32) -> Result<bool> {
        let Some(me) = self.stage.seen(who).map(|t| t.body.pos) else { return Ok(false) };
        let nearest = self
            .stage
            .touching
            .iter()
            .filter_map(|&(a, b)| (a == who).then_some(b).or((b == who).then_some(a)))
            .filter_map(|b| Some((self.stage.seen(b)?.body.pos.truncate().distance_squared(me.truncate()), b)))
            .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        let Some((_, thing)) = nearest else { return Ok(false) };
        self.tell("on_interact", vec![Told::Thing(thing), Told::Thing(who)])?;
        Ok(true)
    }

    /// One event of the ready-made rules' (or of the script's own,
    /// `thing:hurt`, `thing:take`) told to the script.
    fn tell_event(&mut self, event: crate::sim::Event) -> Result<()> {
        match event {
            crate::sim::Event::Hit { who, by, damage, weapon } => {
                let args = vec![Told::Thing(self.fold(who)), by.map_or(Told::Nil, |b| Told::Thing(self.fold(b))), Told::Int(damage as i64), weapon.map_or(Told::Nil, Told::Str)];
                self.tell("on_hit", args)
            }
            crate::sim::Event::FellOut { who } => {
                // Once until it is back in, whoever saw it fall (a kit tells
                // every tick it is out).
                let id = self.fold(who);
                if self.stage.seen(id).is_none() || !self.stage.fallen.insert(id) {
                    return Ok(());
                }
                self.tell("on_fall_out", vec![Told::Thing(id)])?;
                // What nobody can reach any more is taken away; the player
                // waits for what the game decides.
                if host::handle_seat(id).is_none() && !self.stage.gone.contains(&id) {
                    self.stage.gone.push(id);
                }
                Ok(())
            }
            crate::sim::Event::Death { who, by, kind, body, weapon } => {
                let id = self.fold(who);
                if host::handle_seat(id).is_none() {
                    self.stage.goes(kind, body.pos, body.look);
                }
                if self.stage.seen(id).is_none() {
                    let half = self.stage.host.sim().half();
                    self.stage.graveyard.push((id, kind, body, half));
                }
                if !self.stage.dead.insert(id) {
                    // Told already (its hit points ran out by the script's hand).
                    return Ok(());
                }
                let by = by.map_or(Told::Nil, |b| Told::Thing(self.fold(b)));
                self.tell("on_death", vec![Told::Thing(id), by, weapon.map_or(Told::Nil, Told::Str)])
            }
            crate::sim::Event::Pickup { who, item } => {
                self.tell("on_pickup", vec![Told::Thing(self.fold(who)), Told::Str(item)])
            }
        }
    }

    /// The seats after the first as the machines say for this tick: a seat
    /// taken (its state a copy of `c.state.player`, or kept from before),
    /// a seat left (out of play at once), and what each holds
    /// (`docs/game-model.md`, "Several players").
    fn take_seats(&mut self) -> Result<()> {
        for n in 2..=self.stage.seats.len() {
            let s = &mut self.stage.seats[n - 1];
            let next = s.next;
            if next.is_some() && !s.taken {
                (s.taken, s.joined, s.coming) = (true, Some(if s.been { "back" } else { "new" }), None);
                if !std::mem::replace(&mut s.been, true) {
                    self.start_seat(n)?;
                }
            } else if next.is_none() && s.taken {
                (s.taken, s.left) = (false, true);
            }
            let s = &mut self.stage.seats[n - 1];
            (s.before, s.input) = (s.input, next.unwrap_or_default());
            let dirs = s.input.directions();
            for (k, held) in s.held_for.iter_mut().enumerate() {
                let on = if k < 4 { dirs[k] } else { s.input.held(k - 4) };
                *held = if on { held.saturating_add(1) } else { 0 };
            }
        }
        Ok(())
    }

    /// Seat `seat`'s state as a new player's: a copy of `c.state.player`
    /// (the score and hit points the engine's), an empty inventory where
    /// the game has items.
    fn start_seat(&mut self, seat: usize) -> Result<()> {
        let start = self.stage.host.sim().cfg.state.player.clone();
        let has_items = !self.stage.host.sim().cfg.items.is_empty();
        let lua = &self.sandbox.lua;
        let table = lua.create_table()?;
        if has_items {
            table.raw_set("inventory", lua.create_table()?)?;
        }
        // (A key the rules keep as a name, the shooter's gun, theirs.)
        let kept: Vec<&str> = self.stage.host.player_keys().iter().copied().filter(|k| !matches!(*k, "score" | "hp" | "invulnerable")).collect();
        for (key, v) in &start {
            let s = &mut self.stage.seats[seat - 1];
            match (key.as_str(), v) {
                ("score", toml::Value::Integer(n)) => s.score = (*n).max(0) as u32,
                ("hp", toml::Value::Integer(n)) => s.hp = Some(*n as i32),
                ("score" | "hp", _) => anyhow::bail!("c.state.player.{key}: the engine keeps it, as a whole number"),
                (k, toml::Value::String(name)) if kept.contains(&k) => self.stage.host.set_seat_kept(seat, k, host::Kept::Text(name.clone())).map_err(|e| anyhow::anyhow!("c.state.player.{key}: {e}"))?,
                _ => table.raw_set(key.as_str(), world::from_toml(lua, v)?)?,
            }
        }
        self.registry("voxcade.seats")?.raw_set(seat, table)?;
        // A view of the seat's earlier state shows this one now.
        self.registry("voxcade.views")?.raw_set(format!("seat{seat}"), mlua::Value::Nil)?;
        Ok(())
    }

    /// Seats taken and left this tick, told first among the events
    /// (`on_player_join`, `on_player_leave`), seat by seat.
    fn tell_seats(&mut self) -> Result<()> {
        for n in 2..=self.stage.seats.len() {
            let s = &mut self.stage.seats[n - 1];
            let (joined, left) = (s.joined.take(), std::mem::take(&mut s.left));
            if left {
                self.tell("on_player_leave", vec![Told::Thing(host::seat_handle(n))])?;
            }
            if let Some(how) = joined {
                self.tell("on_player_join", vec![Told::Thing(host::seat_handle(n)), Told::Str(how.to_string())])?;
            }
        }
        Ok(())
    }

    /// The players' figures as their seats say, at the tick's end (or as a
    /// level is entered, `entering`): one out of play or whose seat was
    /// left goes; one in play without a figure comes in where `p:place`
    /// put it, else (entering) at its arrival place, else where the first
    /// player in play stands; one whose `state.kind` changed becomes that
    /// kind where it stands. Only without ready-made rules: theirs are
    /// one player's yet.
    fn seat_figures(&mut self, entering: bool) {
        if self.kit().is_some() {
            return self.kit_seat_figures(entering);
        }
        let seats = self.stage.seats.len();
        for n in 1..=seats {
            let want = self.stage.in_play(n);
            let kind = self.stage.seat_kind(n);
            let figure = self.stage.figure(n);
            let h = self.plain_mut().expect("a plain host without a kit");
            let standing = figure.and_then(|id| h.things.get(id)).map(|t| (t.kind, t.body.clone(), t.hp));
            match (standing, want, kind) {
                // Out of play, or nobody sits there: it goes, its hit points kept.
                (Some((_, _, hp)), false, _) => {
                    let id = figure.expect("a figure that stands");
                    h.things.remove(id);
                    Self::set_figure(h, n, None);
                    // The camera on the other players, if any play.
                    if h.focus == Some(id) {
                        h.focus = h.player.into_iter().chain(h.seats.iter().flatten().copied()).next();
                    }
                    self.stage.seats[n - 1].hp = Some(hp);
                    self.stage.seats[n - 1].coming = None;
                }
                // Another kind now: that kind where it stands.
                (Some((was, body, hp)), true, Some(k)) if was != k => {
                    let id = figure.expect("a figure that stands");
                    let mut body = body;
                    body.height = h.kinds[k].height;
                    let new = h.things.spawn(k, body, if h.kinds[k].has_hp { hp } else { 0 }, crate::sim::things::Walk::default());
                    h.things.remove(id);
                    if h.focus == Some(id) {
                        h.focus = Some(new);
                    }
                    Self::set_figure(h, n, Some(new));
                }
                // In play without a figure: it comes in.
                (None, true, Some(k)) => {
                    let coming = self.stage.seats[n - 1].coming.take();
                    let first = (1..=seats).filter(|&m| m != n && self.stage.in_play(m)).find_map(|m| self.stage.seen(host::seat_handle(m)).map(|t| t.body.pos));
                    let arrival = self.arrival(n);
                    let (at, facing) = match (coming, entering, first) {
                        (Some(c), _, _) => c,
                        (None, false, Some(at)) => (at, None),
                        _ => (arrival.at, arrival.facing),
                    };
                    let hp = self.stage.seats[n - 1].hp.take();
                    let h = self.plain_mut().expect("a plain host without a kit");
                    let (height, half) = (h.kinds[k].height, h.sim.half());
                    let at = crate::sim::stand_at(&h.sim.world, at, height, half);
                    let mut body = Body::new(at, height);
                    if let Some(way) = facing {
                        body.face(way.as_vec2().extend(0.0));
                    }
                    let id = h.things.spawn(k, body, hp.unwrap_or(h.kinds[k].hp), crate::sim::things::Walk::default());
                    if n == 1 && !h.looking {
                        h.focus = Some(id);
                    }
                    Self::set_figure(h, n, Some(id));
                    // What it stands in it touches without on_touch.
                    self.stage.placed.push(host::seat_handle(n));
                }
                _ => {}
            }
        }
    }

    /// The ready-made rules' figures for the seats after the first, as
    /// `seat_figures` does a game's own (seat 1's are the rules' own).
    fn kit_seat_figures(&mut self, entering: bool) {
        // Seat 1's too in a game of several (the rules' own player stays
        // in play in a game of one).
        let from = if self.stage.seats.len() > 1 { 1 } else { 2 };
        for n in from..=self.stage.seats.len() {
            let (want, kind, figure) = (self.stage.in_play(n), self.stage.seat_kind(n), self.stage.figure(n));
            let standing = figure.and_then(|id| self.stage.host.get(id)).map(|t| (t.kind, t.body.pos, t.body.look, t.hp));
            let placed = match (standing, want, kind) {
                (Some((_, _, _, hp)), false, _) => {
                    self.stage.seats[n - 1].hp = Some(hp);
                    self.stage.host.put_seat(n, None)
                }
                (Some((was, at, look, hp)), true, Some(k)) if was != k => self.stage.host.put_seat(n, Some((k, at, Some(look), hp))),
                (None, true, Some(k)) => {
                    let coming = self.stage.seats[n - 1].coming.take();
                    let first = (1..=self.stage.seats.len()).filter(|&m| m != n && self.stage.in_play(m)).find_map(|m| self.stage.seen(host::seat_handle(m)).map(|t| t.body.pos));
                    let arrival = self.arrival(n);
                    let (at, facing) = match (coming, entering, first) {
                        (Some(c), _, _) => c,
                        (None, false, Some(at)) => (at, None),
                        _ => (arrival.at, arrival.facing),
                    };
                    let hp = self.stage.seats[n - 1].hp.take().unwrap_or(self.stage.kinds[k].hp);
                    let placed = self.stage.host.put_seat(n, Some((k, at, facing, hp)));
                    self.stage.placed.push(host::seat_handle(n));
                    placed
                }
                _ => Ok(None),
            };
            if let Err(e) = placed {
                self.halt(anyhow::anyhow!("player {n}: {e}"));
            }
        }
    }

    /// Seat `seat`'s figure is `id` (seat 1's the player's).
    fn set_figure(h: &mut Plain, seat: usize, id: Option<Id>) {
        match seat {
            1 => h.player = id,
            _ => h.seats[seat - 2] = id,
        }
    }

    /// Where seat `seat`'s player arrives in the level being played: the
    /// place the level was entered at, `NAME_n` for seat n where the level
    /// has one.
    fn arrival(&self, seat: usize) -> crate::levels::Place {
        let arrived = &self.stage.arrived;
        if seat > 1 {
            let own = format!("{}_{seat}", arrived.name);
            if let Some(p) = self.stage.levels.list[self.stage.level].places.iter().find(|p| p.name == own) {
                return p.clone();
            }
        }
        arrived.clone()
    }

    /// Arriving somewhere (a level entered, the game's start): what touches
    /// now touches without `on_touch`, until it parts.
    fn arrive(&mut self) {
        self.stage.touching = if self.handles("on_touch") || self.handles("on_touch_end") || self.actions.contains(&"interact") { self.stage.contacts() } else { Default::default() };
        self.stage.placed.clear();
        self.stage.note_positions();
    }

    /// The things the script moves that fell out of the level: told in
    /// the next tick (`on_fall_out`, once: `tell_event` notes it in
    /// `fallen`); the kits tell of their own.
    fn fell_out(&mut self) {
        let stage = &mut self.stage;
        // Back in the level (placed again): it may fall out again.
        let fallen: Vec<u32> = stage.fallen.iter().copied().filter(|&raw| stage.seen(raw).is_some_and(|t| crate::sim::fell_out(&stage.host.sim().world, t.body, stage.host.sim().cfg.physics.lost_below))).collect();
        stage.fallen = fallen.into_iter().collect();
        let mut fell = Vec::new();
        for id in stage.host.ids() {
            let Some(t) = stage.host.get(id) else { continue };
            let raw = stage.figure_handle(id).unwrap_or(id.raw());
            // (A piece stays where its level put it, under the floor too: a
            // piece laid on its side has its pivot there.)
            let scripted = stage.kinds.get(t.kind).is_some_and(|k| k.scripted) && !stage.pieces.contains_key(&id.raw());
            if scripted && !stage.fallen.contains(&raw) && crate::sim::fell_out(&stage.host.sim().world, t.body, stage.host.sim().cfg.physics.lost_below) {
                fell.push(raw);
            }
        }
        for raw in fell {
            let who = match host::handle_seat(raw) {
                Some(1) => crate::sim::Who::Player,
                Some(seat) => crate::sim::Who::Thing(stage.figure(seat).expect("a figure that fell")),
                None => crate::sim::Who::Thing(Id::from_raw(raw)),
            };
            stage.later.push(crate::sim::Event::FellOut { who });
        }
    }

    /// The things the script moves go the way they were told; without
    /// ready-made rules, the debris and the camera too.
    fn move_scripted(&mut self) {
        let kinds = self.stage.kinds.clone();
        let fixed = move |kind: usize| kinds.get(kind).is_some_and(|k| k.fixed);
        if let Some(kit) = self.kit_mut() {
            kit.move_scripted(&fixed);
            self.fell_out();
            if let Err(e) = self.prune() {
                self.halt(e);
            }
            return;
        }
        let h = self.plain_mut().expect("a plain host without a kit");
        host::move_things(&mut h.things, &h.sim, &fixed, |w| Some(w));
        self.hold_players();
        self.fell_out();
        if let Err(e) = self.prune() {
            self.halt(e);
        }
        // Several players: the camera goes for the middle of their span.
        let group = if self.follows_players() { self.group_figures() } else { Vec::new() };
        let h = self.plain_mut().expect("a plain host without a kit");
        let half = h.sim.half();
        let mut focus = h.focus.and_then(|id| h.things.get(id)).map(|t| (t.body.pos, t.body.height));
        if group.len() > 1 {
            let at: Vec<Vec3> = group.iter().filter_map(|&id| h.things.get(id)).map(|t| t.body.pos).collect();
            let (lo, hi) = at.iter().fold((at[0], at[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            focus = Some(((lo + hi) * 0.5, focus.map_or(0.0, |f| f.1)));
        }
        let keep_clear = focus.map_or((Vec3::splat(-1.0), Vec3::splat(-1.0)), |(p, height)| (p - Vec3::new(half, half, 0.0), p + Vec3::new(half, half, height)));
        h.sim.update_particles(keep_clear);
        h.sim.update_flares();
        let target = focus.map_or(h.sim.cam_target, |(p, _)| p);
        h.sim.follow(target);
    }

    fn focus_body(&self) -> Option<&Body> {
        let h = self.plain()?;
        h.focus.and_then(|id| h.things.get(id)).map(|t| &t.body)
    }

    /// The players' figures playing and alive, in seat order: those the
    /// camera keeps together and the hold holds (`c.camera.hold`).
    fn group_figures(&self) -> Vec<Id> {
        let host = &self.stage.host;
        let alive = |n: usize, id: Id| match (n, self.kit().is_some()) {
            // Seat 1's hearts are the ready-made rules' run.
            (1, true) => host.player_number("hp").is_none_or(|hp| hp > 0),
            (_, true) => host.get(id).is_some_and(|t| t.hp > 0),
            _ => host.get(id).is_some_and(|t| !self.stage.kinds[t.kind].has_hp || t.hp > 0),
        };
        (1..=self.stage.seats.len()).filter(|&n| self.stage.in_play(n)).filter_map(|n| self.stage.figure(n).map(|id| (n, id))).filter(|&(n, id)| alive(n, id)).map(|(_, id)| id).collect()
    }

    /// Whether the camera follows the players (not a thing of the level
    /// it was told to follow, nor a point).
    fn follows_players(&self) -> bool {
        match self.plain() {
            Some(h) => !h.looking && h.focus.is_none_or(|f| self.stage.figure_handle(f).is_some()),
            None => self.stage.host.follows_players(),
        }
    }

    /// Keeps the players playing and alive within the hold of each other
    /// (`c.camera.hold`), as an arcade machine's screen did: each player's
    /// move, in seat order, cut short where it took the span of the
    /// players further than the hold (and further than it was); a move
    /// that narrows it is never stopped.
    fn hold_players(&mut self) {
        let Some(limit) = self.stage.host.sim().cfg.camera.hold_span() else { return };
        let figures = self.group_figures();
        if figures.len() < 2 {
            return;
        }
        let half = self.stage.host.sim().half();
        for &id in &figures {
            let host = &self.stage.host;
            let others: Vec<Vec3> = figures.iter().filter(|&&o| o != id).filter_map(|&o| host.get(o)).map(|t| t.body.pos).collect();
            let Some(t) = host.get(id) else { continue };
            let (prev, now, height) = (t.body.prev, t.body.pos, t.body.height);
            let mut at = now;
            for axis in 0..2 {
                let (lo, hi) = others.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p[axis]), hi.max(p[axis])));
                let before = hi.max(prev[axis]) - lo.min(prev[axis]);
                let room = limit[axis].max(before);
                at[axis] = at[axis].clamp(hi - room, lo + room);
            }
            if at == now {
                continue;
            }
            // Cut short on the way it came; where that is no room, it stays.
            let at = if crate::sim::body_hits(&host.sim().world, at, half, height) { Vec3::new(prev.x, prev.y, at.z) } else { at };
            match self.plain_mut() {
                Some(h) => {
                    if let Some(t) = h.things.get_mut(id) {
                        for axis in 0..2 {
                            if at[axis] != t.body.pos[axis] {
                                t.body.vel[axis] = 0.0;
                            }
                        }
                        t.body.pos = at;
                    }
                }
                None => self.stage.host.hold_back(id, at),
            }
        }
    }

    /// The plain host's state as one number.
    /// The state hash (`Game::state_hash`), each part's hash kept in
    /// `parts` as it is taken in: the ready-made rules with their things
    /// (or the core's things), `world.state`, `player.state`,
    /// `level.state`, the levels and visits, the kept states, the things'
    /// states, the seats, the music. Parts a game does not use leave the
    /// hash as it was, so every machine names the same parts.
    fn hash_by_parts(&self, mut parts: Option<&mut Vec<(&'static str, u64)>>) -> u64 {
        let mut mark = |name: &'static str, hash: u64| {
            if let Some(parts) = parts.as_deref_mut() {
                parts.push((name, hash));
            }
        };
        // A stopped game's states are not taken again (they made it stop).
        let script = if self.halted.borrow().is_some() {
            Captured::default()
        } else {
            self.capture().unwrap_or_else(|e| {
                self.halt(e);
                Captured::default()
            })
        };
        let mut s = Stable::default();
        match (self.kit(), self.plain()) {
            (Some(kit), _) => {
                let moved_on = self.stage.level != 0 || self.stage.visits.iter().enumerate().any(|(i, &v)| v != (i == 0) as u32);
                let solid = self.stage.host.sim().world.pieces.is_some();
                if script.state.is_none() && script.player.is_none() && script.level.is_none() && script.kept.is_empty() && script.data.is_empty() && !moved_on && !self.uses_music && self.stage.lights.is_empty() && !solid {
                    let only = kit.state_hash();
                    for name in PARTS {
                        mark(name, only);
                    }
                    return only;
                }
                s.u64(kit.state_hash());
            }
            (None, Some(h)) => Self::plain_hash(h, &mut s),
            (None, None) => unreachable!("a host is plain or the kit"),
        }
        mark(PARTS[0], s.finish());
        if let Some(state) = &script.state {
            value::hash(&mut s, state);
        }
        mark(PARTS[1], s.finish());
        if let Some(player) = &script.player {
            s.u64(0x706c_6179);
            value::hash(&mut s, player);
        }
        mark(PARTS[2], s.finish());
        if let Some(level) = &script.level {
            s.u64(0x6c65_7665);
            value::hash(&mut s, level);
        }
        mark(PARTS[3], s.finish());
        // Which level and how often each was entered: only once a game has
        // left its first level, so games of one level keep their replays.
        let stage = &self.stage;
        if stage.level != 0 || stage.visits.iter().enumerate().any(|(i, &v)| v != (i == 0) as u32) {
            s.u64(0x7669_7369).u64(stage.level as u64);
            for &v in &stage.visits {
                s.u64(v as u64);
            }
        }
        mark(PARTS[4], s.finish());
        for (name, v) in &script.kept {
            s.bytes(name.as_bytes());
            value::hash(&mut s, v);
        }
        mark(PARTS[5], s.finish());
        for (id, v) in &script.data {
            s.u64(*id as u64);
            value::hash(&mut s, v);
        }
        // The pieces' lights, in a level that has some (others hash as
        // before).
        for (id, b) in &stage.lights {
            s.u64(0x6c69_6768).u64(*id as u64).f32(*b);
        }
        // The solid pieces taken away, in a level that has some (others
        // hash as before).
        if let Some(solids) = &stage.host.sim().world.pieces {
            s.u64(0x736f_6c69);
            for i in solids.lifted() {
                s.u64(i as u64);
            }
        }
        mark(PARTS[6], s.finish());
        // The seats: once a second was taken, or a player's own keys set
        // (a game of one hashes as it did).
        for (i, seat) in stage.seats.iter().enumerate() {
            let plain = !seat.out && seat.team.is_none() && seat.kind.is_none();
            if (i == 0 || !seat.been) && plain {
                continue;
            }
            s.u64(0x7365_6174).u64(i as u64).u64(seat.taken as u64).u64(seat.out as u64).u64(seat.score as u64).i32(seat.hp.unwrap_or(i32::MIN));
            s.bytes(seat.team.as_deref().unwrap_or("").as_bytes()).bytes(seat.kind.as_deref().unwrap_or("").as_bytes());
            let figure = stage.figure(i + 1).map_or(u64::MAX, |f| f.raw() as u64);
            s.u64(figure);
        }
        for (n, v) in &script.seats {
            s.u64(*n as u64);
            value::hash(&mut s, v);
        }
        mark(PARTS[7], s.finish());
        // The music's clock, for a game that reads or picks its music.
        if self.uses_music {
            let m = &stage.music;
            let next = m.next.map_or(u64::MAX, |(song, when)| song.map_or(u64::MAX - 1, |s| s as u64) * 4 + when as u64);
            s.u64(0x6d75_7369).u64(m.song.map_or(u64::MAX, |s| s as u64)).u64(m.steps).u64(next).u64(m.told.unwrap_or(u64::MAX));
            // The tension only once the game sets it (so earlier checks hold).
            if m.intensity != 1.0 {
                s.f32(m.intensity);
            }
        }
        mark(PARTS[8], s.finish());
        s.finish()
    }

    fn plain_hash(h: &Plain, s: &mut Stable) {
        s.u64(h.sim.tick).u64(h.sim.rng.state()).u64(h.sim.world.changes()).u64(h.sim.freeze as u64);
        s.u64(h.things.len() as u64);
        for t in h.things.iter() {
            let b = &t.body;
            s.u64(t.id.raw() as u64).u64(t.kind as u64).vec3(b.pos).vec3(b.vel).i32(b.look.x).i32(b.look.y).u64(b.on_ground as u64).i32(t.hp);
        }
        s.u64(h.sim.particles.len() as u64);
        for q in &h.sim.particles {
            s.vec3(q.pos);
        }
        s.u64(h.score as u64).u64(match h.outcome {
            None => 0,
            Some(Outcome::Won) => 1,
            Some(Outcome::Lost) => 2,
        });
    }
}

impl<K: Kit> Game for LuaGame<K> {
    type Snapshot = LuaSnapshot;

    /// `conf.lua`, over the engine's defaults.
    fn conf(files: &Files, cfg: &mut GameConfig) -> Result<()> {
        if files.is_file("game.toml") {
            anyhow::bail!("{}: game.toml is no longer read; settings go in conf.lua", files.dir().display());
        }
        conf::run(files, cfg)?;
        if let Some(kit) = &cfg.kit
            && !K::named(kit)
        {
            anyhow::bail!("conf.lua: c.kit = \"{kit}\": there is no such kit (this program's: \"{}\")", K::names().join("\", \""));
        }
        Ok(())
    }

    fn config_file() -> Option<&'static str> {
        Some("conf.lua")
    }

    fn sections(cfg: &GameConfig) -> Vec<&'static str> {
        if with::<K>(cfg) { K::sections(cfg) } else { Vec::new() }
    }

    fn behaviours(cfg: &GameConfig) -> Vec<&'static str> {
        if with::<K>(cfg) { K::behaviours(cfg) } else { Vec::new() }
    }

    /// The kit's actions, then the game's own (`c.actions`).
    fn actions(cfg: &GameConfig) -> Vec<&'static str> {
        let mut actions = if with::<K>(cfg) { K::actions(cfg) } else { Vec::new() };
        for a in &cfg.actions {
            let a = crate::names::intern(a);
            if !actions.contains(&a) {
                actions.push(a);
            }
        }
        actions
    }

    fn default_keys(cfg: &GameConfig) -> Bindings {
        let mut keys = cfg.keys.clone();
        if with::<K>(cfg) {
            keys.fill(&K::default_keys(cfg));
        }
        keys
    }

    fn default_pad(cfg: &GameConfig) -> Bindings {
        let mut pad = cfg.pad.clone();
        if with::<K>(cfg) {
            pad.fill(&K::default_pad(cfg));
        }
        pad
    }

    /// The kit's sounds (or the engine's); the game's files in sounds/
    /// replace and add to them.
    fn sounds(cfg: &GameConfig) -> Vec<(&'static str, SoundDef)> {
        if with::<K>(cfg) { K::sounds(cfg) } else { crate::audio::core_sounds() }
    }

    /// A sound file came or went: `level:sound` and the kinds know the
    /// names there are now.
    fn sounds_changed(&mut self, cfg: &GameConfig) -> Result<()> {
        Self::check_kinds_sounds(cfg, &self.stage.kinds)?;
        self.stage.sounds = Self::sound_names(cfg);
        Ok(())
    }

    /// The kit's models or the kinds' (their shapes count for the
    /// simulation), and the others it draws (`c.models`).
    fn models(cfg: &GameConfig) -> Result<Vec<ModelSpec>> {
        let mut specs: Vec<ModelSpec> = Vec::new();
        if with::<K>(cfg) {
            specs = K::models(cfg)?;
        } else {
            for d in things::defs(cfg, &[])? {
                if let Some(file) = d.model
                    && !specs.iter().any(|s| s.file == file)
                {
                    specs.push(ModelSpec { name: file.clone(), file, shaped: true });
                }
            }
        }
        // What shows when a thing goes: every kind's.
        let effects: Vec<String> = things::defs(cfg, &Self::behaviours(cfg)).map(|defs| defs.into_iter().filter_map(|d| d.effect.filter(|e| !e.bursts()).map(|e| e.model)).collect()).unwrap_or_default();
        for file in cfg.models.iter().chain(cfg.items.values().filter_map(|i| i.model.as_ref())).chain(effects.iter()) {
            if !specs.iter().any(|s| &s.file == file) {
                specs.push(ModelSpec { name: file.clone(), file: file.clone(), shaped: false });
            }
        }
        Ok(specs)
    }

    fn level(cfg: &GameConfig) -> Result<Option<World>> {
        if with::<K>(cfg) { K::level(cfg) } else { Ok(None) }
    }

    fn load(setup: Setup) -> Result<Self> {
        let (cfg, files, models) = (setup.cfg, setup.files, setup.models);
        let kinds = Arc::new(Self::kinds(cfg, |_, model| model.and_then(|m| models.shape(m)).map_or(1.0, |s| s.height()))?);
        let kinds_bursting = Arc::new(bursts(&kinds, models));
        let levels = setup.levels.clone();
        let (setup_slots, setup_texts, setup_replayed) = (setup.slots.clone(), setup.slot_texts.clone(), setup.replayed.clone());
        let songs = setup.songs.clone();
        let player_kind = Self::player_kind(cfg, &kinds)?;
        Self::check_players(cfg)?;
        Self::check_items(cfg)?;
        Self::check_kinds_sounds(cfg, &kinds)?;
        let host: Box<dyn Host> = if with::<K>(cfg) {
            Box::new(K::load(setup)?)
        } else {
            let at = spawn_point(&setup.world, cfg.player.spawn, 1.0, cfg.physics.body_width * 0.5);
            let seats = vec![None; cfg.players.max(1) as usize - 1];
            let mut plain = Plain { sim: Sim::new(setup.world, cfg, at), things: Default::default(), kinds: kinds.clone(), focus: None, looking: false, score: 0, outcome: None, player: None, seats };
            // The player's figure first: the camera follows it.
            if let Some(k) = player_kind {
                plain.player = Some(plain.spawn(k, at.x, at.y, None).expect("a plain host places every kind"));
            }
            Self::place(&mut plain, &setup.markers);
            Box::new(plain)
        };
        let sandbox = Sandbox::new()?;
        for name in ["voxcade.state", "voxcade.player", "voxcade.seats", "voxcade.level", "voxcade.kept", "voxcade.data", "voxcade.views", "voxcade.props"] {
            sandbox.lua.set_named_registry_value(name, sandbox.lua.create_table()?)?;
        }
        let is_thing = sandbox.lua.create_function(|_, v: mlua::Value| {
            Ok(match v {
                mlua::Value::UserData(ref ud) if ud.is::<world::ThingRef>() => Some(if host::handle_seat(ud.borrow::<world::ThingRef>()?.0).is_some() { "player" } else { "thing" }),
                _ => None,
            })
        })?;
        sandbox.lua.set_named_registry_value("voxcade.is_thing", is_thing)?;
        sandbox.provide("world", WorldView)?;
        sandbox.provide("level", LevelView)?;
        // `player`, the one handle of the player's figure, whatever its id.
        sandbox.provide("player", world::handle(&sandbox.lua, PLAYER)?)?;
        if !host.sim().cfg.items.is_empty() {
            sandbox.lua.named_registry_value::<Table>("voxcade.player")?.raw_set("inventory", sandbox.lua.create_table()?)?;
        }
        sandbox.provide("input", Absent(NO_INPUT))?;
        sandbox.provide("screen", ScreenView)?;
        sandbox.provide("controls", ControlsView(0))?;
        sandbox.provide("music", world::MusicView)?;
        let actions = Arc::new(Self::actions(cfg));
        let mut game = Self {
            stage: Stage {
                host,
                kinds,
                sounds: Self::sound_names(cfg),
                gone: Vec::new(),
                visits: (0..levels.list.len()).map(|i| (i == 0) as u32).collect(),
                levels,
                level: 0,
                ask: None,
                arrived: crate::levels::Place::new("start", Vec3::ZERO, None),
                ended: false,
                saving: Vec::new(),
                slots: setup_slots,
                slot_texts: setup_texts,
                replayed: setup_replayed.into(),
                loaded: Vec::new(),
                touching: Default::default(),
                dead: Default::default(),
                graveyard: Vec::new(),
                placed: Vec::new(),
                later: Vec::new(),
                killers: Default::default(),
                controls: crate::controls::Controls::nobody(&actions),
                seats: (1..=cfg.players.max(1))
                    .map(|n| host::SeatState { taken: n == 1, been: n == 1, controls: crate::controls::Controls::nobody(&actions), ..Default::default() })
                    .collect(),
                local: Default::default(),
                ticking: false,
                actions: actions.clone(),
                languages: Arc::new(cfg.languages.clone()),
                names: Default::default(),
                seen_at: Default::default(),
                effects: Vec::new(),
                props: Default::default(),
                fallen: Default::default(),
                frames: Arc::new(models.frame_counts()),
                heights: Arc::new(models.heights()),
                bursts: kinds_bursting,
                parts: Arc::new(models.part_names()),
                hud: host::HudState::new(cfg),
                songs: Arc::new(songs),
                music: Default::default(),
                pieces: Default::default(),
                lights: Default::default(),
            },
            sandbox,
            tick: None,
            draw: None,
            globals: None,
            code: Vec::new(),
            uses_music: false,
            moves_sun: false,
            data: Vec::new(),
            files: files.clone(),
            env: None,
            required: Vec::new(),
            modules: Vec::new(),
            actions,
            scored: cfg.scored,
            before: Input::default(),
            held_for: [0; world::KEYS],
            halted: RefCell::new(None),
            started: true,
            changes: 0,
            draw_failed: Cell::new(false),
            state_failed: Cell::new(false),
            start_text: None,
            arrive_at: None,
            seat_places: Vec::new(),
            written: Vec::new(),
            drawn: RefCell::new(Vec::new()),
            overlay: RefCell::new(Vec::new()),
            kit: PhantomData,
        };
        // The first level's place called start, if it has one, as every
        // entry of it later.
        game.stage.arrived = game.stage.start(0);
        let (start, placed) = (game.stage.arrived.clone(), game.place_in(0, None).is_some());
        let first = game.stage.levels.list[0].clone();
        match game.kit_mut() {
            // The ready-made rules stood their player by their own rules
            // unless the level has a place called start.
            Some(kit) => {
                if placed {
                    kit.enter_level(first.world.clone(), &first.markers, &first.places, Some((start.at, start.facing)));
                }
            }
            None => {
                let h = game.plain_mut().expect("a plain host without a kit");
                Self::stand_player(h, &start);
                (h.sim.cam_target, h.sim.cam_prev) = (start.at, start.at);
            }
        }
        game.spawn_pieces();
        game.stage.name_things();
        // The run's start (`c.state`), a game without code too; the start
        // handlers begin from it again.
        game.start_states()?;
        if files.is_file("main.lua") {
            game.run_main(files, true)?;
        } else {
            // No rules of its own: its scenarios and the console run in an
            // environment of their own all the same.
            let env = game.sandbox.run("console", "")?;
            game.sandbox.freeze(&env)?;
            game.env = Some(env);
        }
        game.built();
        if let Some(b) = game.focus_body() {
            let target = b.pos + Vec3::Z * b.height * 0.5;
            let sim = game.stage.host.sim_mut();
            (sim.cam_target, sim.cam_prev) = (target, target);
        }
        Ok(game)
    }

    fn snapshot(&self) -> Result<LuaSnapshot> {
        Ok(LuaSnapshot { stage: self.stage.clone(), script: self.capture()?, before: self.before, held_for: self.held_for })
    }

    fn restore(&mut self, kept: &LuaSnapshot) {
        self.stage = kept.stage.clone();
        self.before = kept.before;
        self.held_for = kept.held_for;
        let lua = &self.sandbox.lua;
        let restored = (|| -> Result<()> {
            // The views showed the states replaced now (and the props things
            // that may be others now).
            lua.set_named_registry_value("voxcade.views", lua.create_table()?)?;
            lua.set_named_registry_value("voxcade.props", lua.create_table()?)?;
            for (name, kept) in [("voxcade.state", &kept.script.state), ("voxcade.player", &kept.script.player), ("voxcade.level", &kept.script.level)] {
                let state = match kept {
                    Some(v) => value::restore(lua, v)?,
                    None => mlua::Value::Table(lua.create_table()?),
                };
                lua.set_named_registry_value(name, state)?;
            }
            let all = lua.create_table()?;
            for (name, v) in &kept.script.kept {
                all.raw_set(name.as_str(), value::restore(lua, v)?)?;
            }
            lua.set_named_registry_value("voxcade.kept", all)?;
            let data = lua.create_table()?;
            for (id, v) in &kept.script.data {
                data.raw_set(*id, value::restore(lua, v)?)?;
            }
            lua.set_named_registry_value("voxcade.data", data)?;
            let seats = lua.create_table()?;
            for (n, v) in &kept.script.seats {
                seats.raw_set(*n, value::restore(lua, v)?)?;
            }
            lua.set_named_registry_value("voxcade.seats", seats)?;
            Ok(())
        })();
        match restored {
            Ok(()) => self.mended(),
            Err(e) => self.halt(e),
        }
    }

    fn code(&self) -> Vec<(String, Vec<u8>)> {
        let mut code = if self.code.is_empty() { Vec::new() } else { vec![("main.lua".into(), self.code.clone())] };
        code.extend(self.modules.iter().cloned());
        code.extend(self.data.iter().cloned());
        code
    }

    /// `main.lua`, a Lua file it required or a file it read changed:
    /// `main.lua` runs anew.
    fn reload_code(&mut self, files: &Files, file: &Path) -> Result<bool> {
        let Some(changed) = files.name(file) else { return Ok(false) };
        if changed != "main.lua" && !self.modules.iter().chain(&self.data).any(|(name, _)| *name == changed) {
            return Ok(false);
        }
        self.files = files.clone();
        match self.run_main(files, false) {
            Ok(()) => self.mended(),
            Err(e) => self.halt(e),
        }
        Ok(true)
    }

    fn halted(&self) -> Option<String> {
        self.halted.borrow().clone()
    }

    fn moves_sun(&self) -> bool {
        self.moves_sun
    }

    fn daylight(&self) -> crate::session::Daylight {
        self.stage.levels.list.get(self.stage.level).map(|l| l.daylight).unwrap_or_default()
    }

    fn suns(&self) -> Vec<Vec3> {
        self.stage.levels.suns()
    }

    fn start_failed(&self) -> bool {
        !self.started
    }

    /// The levels were read anew (a level file saved while playing): the
    /// level being played is entered afresh from its file (`"reload"`), the
    /// player where it stood.
    fn replace_levels(&mut self, levels: Arc<crate::levels::Levels>) {
        let name = self.stage.levels.list[self.stage.level].name.clone();
        let old = std::mem::replace(&mut self.stage.levels, levels);
        let visits: Vec<u32> = self.stage.levels.list.iter().map(|l| old.find(&l.name).map_or(0, |i| self.stage.visits[i])).collect();
        self.stage.visits = visits;
        let index = self.stage.levels.find(&name).unwrap_or(0);
        // The player stays where its figure stands.
        let here = self.stage.host.player_seen().map(|t| t.body).or_else(|| self.focus());
        let place = match here {
            Some(b) => Some(crate::levels::Place { name: "here".into(), at: b.pos, facing: Some(b.look) }),
            None => self.place_in(index, None),
        };
        // An ask pending (its fade out under way) waits for the fade; an
        // ask of this on_level_start counts at once.
        let pending = self.stage.ask.take();
        let reloaded = self.call_handler("on_level_end", "reload", Some("level_end")).and_then(|()| self.enter(index, place, "reload", None));
        if let Err(e) = reloaded {
            self.halt(e);
        }
        self.change_levels();
        if self.stage.ask.is_none() {
            self.stage.ask = pending;
        }
    }

    fn leaving(&self) -> Option<&'static str> {
        self.stage.ask.as_ref().filter(|_| self.halted.borrow().is_none() && !self.stage.ended).map(Ask::why)
    }

    fn saving(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.written)
    }

    fn loaded(&self) -> &[Arc<str>] {
        &self.stage.loaded
    }

    fn save(&self) -> Result<String> {
        self.save_data()?.write()
    }

    fn start_from(&mut self, text: &str) -> Result<()> {
        // Read first: a save that does not read leaves the run as it is.
        save::SaveData::read(text)?;
        self.stage.ask = Some(Ask::Load { slot: None });
        self.start_text = Some(text.to_string());
        self.change_levels();
        match self.halted() {
            Some(e) => anyhow::bail!("{e}"),
            None => Ok(()),
        }
    }

    fn scenarios(&mut self) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        for name in scenario_names(&self.files) {
            let path = format!("scenarios/{name}.lua");
            let src = self.files.read_to_string(&path).with_context(|| format!("reading {path}"))?;
            let about = self.reopened(false, |game, env| {
                let table = scenario_table(&path, game.sandbox.run_in(&path, &src, env, "the start")?)?;
                Ok(table.raw_get::<Option<String>>("about")?.unwrap_or_default())
            });
            out.push((name, about.unwrap_or_else(|e| format!("(a mistake: {e:#})"))));
        }
        Ok(out)
    }

    fn start_scenario(&mut self, name: &str) -> Result<()> {
        let names = scenario_names(&self.files);
        if !names.iter().any(|n| n == name) {
            match names.is_empty() {
                true => anyhow::bail!("no scenario '{name}': the game has none yet (scenarios/NAME.lua in its folder, docs/game-model.md)"),
                false => anyhow::bail!("no scenario '{name}' (the game's: {})", names.join(", ")),
            }
        }
        if let Some(e) = self.halted() {
            anyhow::bail!("{e}");
        }
        anyhow::ensure!(self.stage.host.sim().tick == 0 && self.start_text.is_none(), "a scenario begins a run: before its first tick, and not after a save");
        let path = format!("scenarios/{name}.lua");
        let src = self.files.read_to_string(&path).with_context(|| format!("reading {path}"))?;
        self.modules.push((path.clone(), lex::tokens(&path, &src)?));
        let set_up = self.reopened(true, |game, env| {
            let table = scenario_table(&path, game.sandbox.run_in(&path, &src, env, "the start")?)?;
            for key in table.pairs::<mlua::Value, mlua::Value>() {
                let (key, _) = key?;
                let key = key.to_string().unwrap_or_else(|_| "?".into());
                anyhow::ensure!(["about", "level", "place", "setup"].contains(&key.as_str()), "{path}: no field called '{key}' (about, level, place, setup)");
            }
            // A scenario that names where the run begins needs no setup.
            let setup: Option<Function> = table.raw_get("setup")?;
            let (level, place): (Option<String>, Option<String>) = (table.raw_get("level")?, table.raw_get("place")?);
            anyhow::ensure!(
                setup.is_some() || level.is_some() || place.is_some(),
                "{path}: a scenario says where the run begins (level = \"name\", place = \"name\") or sets it up: setup is the function that sets the run up: setup = function() ... end"
            );
            match (level, place) {
                (Some(level), place) => {
                    let Some(index) = game.stage.levels.find(&level) else {
                        let all: Vec<&str> = game.stage.levels.list.iter().map(|l| l.name.as_str()).collect();
                        anyhow::bail!("{path}: level = \"{level}\" is none of the game's levels ({})", all.join(", "));
                    };
                    if let Some(place) = &place {
                        anyhow::ensure!(game.place_in(index, Some(place.clone())).is_some(), "{path}: the level {level} has no place called {place}");
                    }
                    game.stage.ask = Some(Ask::GoTo { level, place });
                    game.change_levels();
                    if let Some(e) = game.halted() {
                        anyhow::bail!("{e}");
                    }
                }
                (None, Some(_)) => anyhow::bail!("{path}: a place belongs to a level: level = \"...\", place = \"...\""),
                (None, None) => {}
            }
            match &setup {
                Some(setup) => game.with_world(|sb, _| sb.call_start::<()>(setup, ())),
                None => Ok(()),
            }
        });
        if let Err(e) = set_up {
            self.halt(anyhow::anyhow!("{e:#}"));
            return Err(e);
        }
        self.settle_between(&format!("the scenario {name}"))
    }

    fn eval(&mut self, code: &str) -> Result<String> {
        if let Some(e) = self.halted() {
            anyhow::bail!("the game has stopped: {e}");
        }
        // An expression shows its value, as Lua's own console does.
        let expression = format!("return {code}");
        let chunk = if self.sandbox.lua.load(&expression).into_function().is_ok() { expression } else { code.to_string() };
        // What the line prints is shown with it, and goes to OUTPUT too.
        let before = std::mem::take(&mut *self.sandbox.printed.borrow_mut());
        let ran = self.reopened(false, |game, env| {
            game.with_world(|sb, _| {
                let values = sb.run_in("console", &chunk, env, "the console")?;
                let tostring = sb.tool("tostring")?;
                Ok(values.iter().map(|v| shown(v, &tostring)).collect::<Vec<String>>())
            })
        });
        let printed: Vec<String> = {
            let mut kept = self.sandbox.printed.borrow_mut();
            let said: Vec<String> = kept.drain(..).collect();
            kept.extend(before);
            kept.extend(said.iter().cloned());
            said
        };
        let settled = self.settle_between("the console line");
        let values = ran?;
        settled?;
        let mut lines = printed;
        if !values.is_empty() {
            lines.push(values.join("\t"));
        }
        Ok(lines.join("\n"))
    }

    fn set_seats(&mut self, seats: &[crate::game::Seat]) {
        for (s, seat) in self.stage.seats.iter_mut().skip(1).zip(seats) {
            (s.next, s.controls) = (seat.input, seat.controls.clone());
        }
    }

    fn set_controls(&mut self, controls: &crate::controls::Controls) {
        self.stage.controls = controls.clone();
    }

    fn set_local(&mut self, local: &crate::game::Local) {
        self.stage.local = local.clone();
    }

    fn set_slots(&mut self, slots: Vec<Option<crate::saves::SlotInfo>>, texts: std::collections::BTreeMap<u32, Arc<str>>) {
        self.stage.slots = slots;
        self.stage.slot_texts = texts;
    }

    fn change(&mut self) {
        self.change_levels();
    }

    fn quit(&mut self) {
        if self.stage.ended || self.halted.borrow().is_some() {
            return;
        }
        self.stage.ended = true;
        self.stage.ask = None;
        let told = self.call_handler("on_level_end", "quit", Some("level_end")).and_then(|()| self.call_handler("on_world_end", "quit", Some("world_end")));
        if let Err(e) = told {
            self.halt(e);
        }
    }

    fn printed(&mut self) -> Vec<String> {
        self.sandbox.printed.borrow_mut().drain(..).collect()
    }

    /// The things (where, kind, state with hit points), `world.state`,
    /// `level.state` and `player.state` (with the engine's keys). A state
    /// that is no plain data stops the game, as it would at the next check.
    fn inspect(&self) -> serde_json::Value {
        let Captured { state, player, level, kept, data, seats } = self.capture().unwrap_or_else(|e| {
            self.halt(e);
            Captured::default()
        });
        let host = &self.stage.host;
        // The level's things; the players' figures are the players'.
        let figures: Vec<Id> = (1..=self.stage.seats.len()).filter_map(|n| self.stage.figure(n)).collect();
        let things: Vec<serde_json::Value> = host
            .ids()
            .into_iter()
            .filter(|id| !figures.contains(id))
            .filter_map(|id| {
                let t = host.get(id)?;
                let p = t.body.pos;
                let mut own = data.iter().find(|(i, _)| *i == id.raw()).map(|(_, d)| value::to_json(d)).filter(|j| j.is_object()).unwrap_or_else(|| serde_json::json!({}));
                if self.stage.kinds[t.kind].has_hp {
                    own["hp"] = t.hp.into();
                }
                if let Some(b) = self.stage.lights.get(&id.raw()) {
                    own["light"] = (*b).into();
                }
                let mut thing = serde_json::json!({ "id": id.raw(), "kind": self.stage.kinds[t.kind].name, "x": p.x, "y": p.y, "z": p.z, "state": own });
                if let Some(name) = self.stage.names.get(&id.raw()) {
                    thing["name"] = name.clone().into();
                }
                if let Some(model) = self.stage.pieces.get(&id.raw()).and_then(|p| p.model.clone()) {
                    thing["model"] = model.into();
                }
                Some(thing)
            })
            .collect();
        let mut player = player.as_ref().map(value::to_json).filter(|j| j.is_object()).unwrap_or_else(|| serde_json::json!({}));
        for key in host.player_keys() {
            if let Some(n) = host.player_number(key) {
                player[*key] = n.into();
            }
        }
        let mut player = serde_json::json!({ "state": player });
        if let Some(t) = host.player_seen() {
            let p = t.body.pos;
            (player["x"], player["y"], player["z"]) = (p.x.into(), p.y.into(), p.z.into());
            player["kind"] = self.stage.kinds.get(t.kind).map(|k| k.name.clone()).into();
        }
        let stage = &self.stage;
        let at = serde_json::json!({ "name": stage.levels.list[stage.level].name, "number": stage.level + 1, "visits": stage.visits[stage.level], "levels": stage.levels.names() });
        let kept: serde_json::Map<String, serde_json::Value> = kept.iter().map(|(n, v)| (n.clone(), value::to_json(v))).collect();
        let mut out = serde_json::json!({ "things": things, "state": state.as_ref().map(value::to_json), "level": level.as_ref().map(value::to_json), "at": at, "kept": kept, "player": player });
        // The other seats, a game of several's.
        if stage.seats.len() > 1 {
            let players: Vec<serde_json::Value> = (2..=stage.seats.len())
                .map(|n| {
                    let s = &stage.seats[n - 1];
                    let mut state = seats.iter().find(|(m, _)| *m == n).map(|(_, v)| value::to_json(v)).filter(|j| j.is_object()).unwrap_or_else(|| serde_json::json!({}));
                    state["score"] = s.score.into();
                    let mut p = serde_json::json!({ "seat": n, "taken": s.taken, "out": s.out, "state": state });
                    if let Some(t) = stage.seen(host::seat_handle(n)) {
                        let at = t.body.pos;
                        (p["x"], p["y"], p["z"]) = (at.x.into(), at.y.into(), at.z.into());
                        p["kind"] = stage.kinds.get(t.kind).map(|k| k.name.clone()).into();
                        p["state"]["hp"] = t.hp.into();
                    }
                    p
                })
                .collect();
            out["players"] = players.into();
        }
        out
    }

    /// The locals of `main.lua` its functions keep (upvalues).
    fn hidden(&self) -> Vec<(String, u64)> {
        self.globals.as_ref().and_then(|g| hidden::upvalues(&self.sandbox.lua, g).ok()).unwrap_or_default()
    }

    fn sim(&self) -> &Sim {
        self.stage.host.sim()
    }

    fn sim_mut(&mut self) -> &mut Sim {
        self.stage.host.sim_mut()
    }

    /// A tick: the kit's behaviours (if it has one), the script's
    /// `on_tick`, then the script's things move (the way they were told to walk,
    /// falling, landing); without a kit the debris and the camera too.
    fn step(&mut self, input: &Input) {
        if self.halted.borrow().is_some() {
            return;
        }
        // The music's clock goes on every step, a hit stop's too.
        let songs = Arc::clone(&self.stage.songs);
        self.stage.music.step(&songs);
        // A level held (`level.state.held`): the ready-made rules, physics
        // and touches wait; on_tick and the time go on.
        let held = self.held();
        let stock = if held || self.kit().is_none() { Ok((Vec::new(), Vec::new())) } else { self.stock_in().and_then(|one| Ok((one, self.seats_stock_in()?))) };
        let (stock, seats_stock) = match stock {
            Ok(stock) => stock,
            Err(e) => {
                self.halt(e);
                return;
            }
        };
        let seat_inputs: Vec<Option<Input>> = self.stage.seats.iter().skip(1).map(|s| s.next).collect();
        let seat_teams: Vec<Option<&'static str>> = self.stage.seats.iter().map(|s| s.team.as_deref().map(crate::names::intern)).collect();
        let ticked = match self.kit_mut() {
            Some(kit) if held => kit.held(),
            Some(kit) => {
                let before = Game::sim(kit).tick;
                if !seat_inputs.is_empty() {
                    kit.set_seat_inputs(&seat_inputs);
                }
                kit.set_seat_teams(&seat_teams);
                kit.rules(input);
                let ticked = Game::sim(kit).tick != before;
                // The kit's heroes held together as they moved.
                if ticked {
                    self.hold_players();
                }
                ticked
            }
            None => {
                let h = self.plain_mut().expect("a plain host without a kit");
                let ticked = h.sim.begin_tick();
                for t in h.things.iter_mut() {
                    t.body.prev = t.body.pos;
                }
                ticked
            }
        };
        if let Err(e) = self.stock_out(&stock).and_then(|()| self.seats_stock_out(&seats_stock)) {
            self.halt(e);
            return;
        }
        if !ticked {
            return;
        }
        self.changes = 0;
        if let Err(e) = self.take_seats() {
            self.halt(e);
            return;
        }
        // Pictures played once go on, and end with their last frame.
        let frames = self.stage.frames.clone();
        self.stage.effects.retain_mut(|e| {
            e.age += 1;
            ((e.age as f32 * e.fps / 60.0) as usize) < frames.get(&e.model).copied().unwrap_or(1)
        });
        self.stage.hud.tick();
        // A run that ended tells the script nothing more (its rules may
        // still settle the scene).
        let over = self.stage.ended;
        if !over && let Err(e) = self.tell_seats() {
            self.halt(e);
        }
        let used = match if over { Ok(None) } else { self.tell_events(input) } {
            Ok(used) => used,
            Err(e) => {
                self.halt(e);
                None
            }
        };
        // The beats of the game's song begun since the last tick, each told
        // (`on_beat`), before on_tick.
        let beats = self.stage.music.untold(&songs);
        let on_beat = self.globals.as_ref().and_then(|g| Sandbox::function(g, "on_beat").ok().flatten());
        if let Some(f) = on_beat.filter(|_| !over) {
            for beat in beats {
                if self.halted.borrow().is_some() {
                    break;
                }
                if let Err(e) = self.with_world(|sb, _| sb.call::<()>(&f, beat, "on_beat")) {
                    self.halt(e);
                }
            }
        }
        // How long each key has been held (`input:repeated`).
        let dirs = input.directions();
        for (k, n) in self.held_for.iter_mut().enumerate() {
            let held = if k < 4 { dirs[k] } else { input.held(k - 4) };
            *n = if held { n.saturating_add(1) } else { 0 };
        }
        if let Some(tick) = self.tick.clone().filter(|_| self.halted.borrow().is_none() && !over) {
            // A press an on_interact used is no new press for on_tick.
            let mut before = self.before;
            let mut held_for = self.held_for;
            if let Some(a) = used {
                before.hold(a, true);
                held_for[4 + a] = held_for[4 + a].max(2);
            }
            let view = InputView { now: *input, before, actions: self.actions.clone(), held_for };
            // Seat 1's keys are `player.input` too.
            let first = &mut self.stage.seats[0];
            (first.input, first.before, first.held_for) = (*input, before, held_for);
            self.stage.ticking = true;
            if let Err(e) = self.with_world(|sb, _| {
                sb.provide("input", view)?;
                let ticked = sb.call::<()>(&tick, (), "on_tick");
                sb.provide("input", Absent(NO_INPUT))?;
                ticked
            }) {
                self.halt(e);
            }
            self.stage.ticking = false;
        }
        // The tick's end: what the script removed goes, the players' figures
        // come and go as their seats say; a state that is no plain data
        // stops the game at the tick it became so.
        self.stage.settle();
        self.seat_figures(false);
        if let Err(e) = self.check_states() {
            let tick = self.stage.host.sim().tick;
            self.halt(anyhow::anyhow!("{e:#} (at the end of tick {tick})"));
            self.state_failed.set(true);
        }
        self.before = *input;
        // A hold the script set this tick holds this tick's moves too.
        if !held && !self.held() {
            self.move_scripted();
        }
        // The attacks the code asked for start.
        self.stage.host.start_attacks();
        // A world:save keeps the tick as it ends, before the ask it may
        // have made is carried out.
        self.write_saves();
    }

    /// The simulation's state as one number: the kit's (or the core's, the
    /// things', the score), and the script's states when it keeps some
    /// (each only then, so games without it keep their replays).
    fn state_hash(&self) -> u64 {
        self.hash_by_parts(None)
    }

    fn state_sections(&self) -> Vec<(&'static str, u64)> {
        let mut parts = Vec::new();
        self.hash_by_parts(Some(&mut parts));
        parts
    }

    /// The kit's things, then the script's `on_draw`: models of things,
    /// cubes, lights, text for the HUD; the game only to look at.
    fn draw(&self, out: &mut Drawing) {
        if let Some(kit) = self.kit() {
            kit.draw(out);
        }
        // The pieces the level set down, each its model where its pivot
        // stands, facing its way, and its light as bright as its state says
        // (off at 0). The script draws nothing for them.
        for (&id, p) in &self.stage.pieces {
            let Some(t) = self.stage.seen(id).filter(|_| !self.stage.dead.contains(&id)) else { continue };
            // Tipped too (`pitch`, `roll`): as `frame:model` would turn it
            // with the same three numbers.
            let tipped = p.pitch != 0.0 || p.roll != 0.0;
            if let Some(model) = &p.model {
                let facing = crate::session::facing(true, t.body);
                let turn = if tipped { facing * crate::session::turning(0.0, p.pitch, p.roll) } else { facing };
                out.model_turned(model, 0, out.body_origin(t.body), t.body.look, turn, None);
            }
            // Its lights, all as bright as its state says (one brightness for
            // them all; a light of its own each would be a list of them).
            let brightness = self.stage.lights.get(&id).copied().unwrap_or(0.0);
            if brightness > 0.0 {
                let (sin, cos) = crate::sim::math::sin_cos((p.facing.bearing() as f32).to_radians());
                // A tipped piece's lights turn with it about its turn's
                // centre (the voxel at half its height).
                let size = p.model.as_ref().and_then(|m| out.model_of(m)).map(|m| m.size);
                let tip = size.filter(|_| tipped).map(|size| (crate::session::turning(p.facing.bearing() as f32, p.pitch, p.roll), crate::render::turn_centre(size)));
                for l in &p.lights {
                    // Its offset in the model's own voxels, turned with it (a
                    // bearing clockwise seen from above, the model's forward +y).
                    let o = l.at.as_vec3();
                    let turned = match tip {
                        Some((rot, about)) => rot * (o + Vec3::splat(0.5) - about) + about - Vec3::splat(0.5),
                        None => Vec3::new(o.x * cos + o.y * sin, -o.x * sin + o.y * cos, o.z),
                    };
                    let at = t.body.pos + Vec3::Z * 0.5 + turned;
                    out.light(Light { pos: at, radius: l.radius, color: l.color.map(|c| c * brightness) });
                }
            }
        }
        for e in &self.stage.effects {
            out.model(&e.model, (e.age as f32 * e.fps / 60.0) as usize, e.at.floor(), glam::IVec2::NEG_Y, None);
        }
        // Stopped by `on_tick`, the game is still drawn as it stands;
        // stopped by `on_draw`, as `on_draw` last showed it.
        if let Some(draw) = &self.draw
            && !self.draw_failed.get()
        {
            let (width, height) = out.hud_size();
            let mut rec = Frame {
                width,
                height,
                font: out.font(),
                alpha: out.alpha(),
                view_proj: out.view_proj(),
                snap: out.snaps(),
                parts: self.stage.parts.clone(),
                known: self.stage.frames.clone(),
                ..Frame::default()
            };
            let lua = &self.sandbox.lua;
            let mut result = None;
            let scoped = lua.scope(|scope| {
                let world = scope.create_userdata_ref(&self.stage)?;
                lua.set_named_registry_value("voxcade.world", &world)?;
                lua.set_named_registry_value("voxcade.looking", true)?;
                // The picture is the handler's alone: there is none to keep.
                let frame = scope.create_userdata_ref_mut(&mut rec)?;
                result = Some(self.sandbox.call::<()>(draw, frame, "on_draw"));
                lua.unset_named_registry_value("voxcade.world")?;
                lua.set_named_registry_value("voxcade.looking", false)
            });
            match scoped.map_err(sandbox::script_error).and_then(|()| result.expect("draw ran")) {
                Ok(()) => *self.drawn.borrow_mut() = rec.cmds,
                Err(e) => {
                    self.draw_failed.set(true);
                    if self.halted.borrow().is_none() {
                        self.halt(e);
                    }
                }
            }
        }
        let mut overlay = Vec::new();
        for cmd in self.drawn.borrow().iter().cloned() {
            match cmd {
                Cmd::Model { file, thing, frame, turn, pose } => {
                    let Some(t) = self.stage.seen(thing) else { continue };
                    let kind = self.stage.kinds.get(t.kind);
                    let frame = frame.unwrap_or_else(|| out.figure_frame(&file, kind.map_or(&NO_FRAMES, |k| &k.frames), t.body));
                    let origin = out.body_origin(t.body);
                    // Its kind may face exactly its way and spin; the
                    // script turns it further.
                    let facing = crate::session::facing(kind.is_some_and(|k| k.free_turn), t.body);
                    let spin = crate::session::spinning(kind.map_or(0.0, |k| k.spin), self.stage.host.sim().tick as f32 + out.alpha());
                    let [round, pitch, roll] = turn;
                    let turn = facing * spin * crate::session::turning(round, pitch, roll);
                    // A figure of parts swings as its kind says, the
                    // script's pose on top.
                    let tall = out.model_of(&file).map_or(1.0, |m| m.size.z as f32);
                    let swung = crate::session::swinging(kind.map_or(&NO_SWING, |k| &k.swing), t.body, tall);
                    let posed = crate::session::posing(&pose, swung);
                    out.model_posed(&file, frame, origin, t.body.look, turn, &posed, (t.flash > 0).then_some(FLASH));
                }
                // On the grid when the camera snaps, gliding as the models
                // do otherwise.
                Cmd::Cube { at, color, glow } => out.cube(out.snapped(at), color, glow, color[3] as f32 / 255.0),
                Cmd::Light { at, radius, color } => out.light(Light { pos: at, radius, color }),
                Cmd::Daylight(day) => out.daylight(day),
                hud => overlay.push(hud),
            }
        }
        *self.overlay.borrow_mut() = overlay;
    }

    /// The HUD `on_draw` drew (from the top left in UI voxels), and the
    /// voxels of icons that burst.
    fn hud(&self, cfg: &UiConfig, alpha: f32, aspect: f32, font: Option<&DisplayFont>) -> Vec<Instance> {
        use crate::ui;
        let mut out = Vec::new();
        let layout = ui::Layout::of(cfg, aspect);
        let (top, depth, finish) = (layout.height, cfg.depth, ui::Finish::of(cfg));
        for cmd in self.overlay.borrow().iter() {
            match cmd {
                Cmd::Text { text, x, y, color, size } => {
                    ui::line(&mut out, text, &layout, *x, top - y - ui::line_height(*size, font), *size, *color, depth, font, finish);
                }
                Cmd::Box { x, y, width, height, color } => {
                    let (w, h) = (width.round() as i32, height.round() as i32);
                    for i in 0..w {
                        for j in 0..h {
                            for z in 0..depth {
                                ui::voxel(&mut out, x + i as f32, top - y - h as f32 + j as f32, z as f32, *color);
                            }
                        }
                    }
                }
                Cmd::Icon { name, x, y } => {
                    let Some(icon) = self.stage.hud.icons.get(name) else { continue };
                    for (c, r, color) in icon.voxels() {
                        for z in 0..depth {
                            ui::voxel(&mut out, x + c as f32, top - y - 1.0 - r as f32, z as f32, color);
                        }
                    }
                }
                Cmd::Drop { text, x, y, ticks, color, size } => {
                    ui::dropping(&mut out, text, &layout, Some(*x), top - y - ui::line_height(*size, font), *size, *color, depth, *ticks + alpha, font, finish);
                }
                Cmd::Menu(m) => ui::game_menu(&mut out, m, &layout, depth, &self.stage.hud.icons, self.stage.host.sim().tick),
                Cmd::Grid(g) => ui::game_grid(&mut out, g, &layout, depth, &self.stage.hud.icons),
                _ => {}
            }
        }
        for b in &self.stage.hud.bits {
            let at = crate::session::between(b.prev, b.pos, alpha);
            let size = (b.life / b.span).clamp(0.2, 1.0);
            out.push(Instance { pos_size: (at + 0.5).extend(size).to_array(), color: b.color, fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] });
        }
        out
    }

    fn focus(&self) -> Option<&Body> {
        match self.kit() {
            Some(kit) => kit.focus(),
            None => self.focus_body(),
        }
    }

    fn focus_group(&self) -> Vec<&Body> {
        let group = if self.follows_players() { self.group_figures() } else { Vec::new() };
        match group.len() > 1 {
            true => group.iter().filter_map(|&id| self.stage.host.get(id)).map(|t| t.body).collect(),
            false => self.focus().into_iter().collect(),
        }
    }

    fn outcome(&self) -> Option<Outcome> {
        // The run is over once the script was told (the fade out first).
        if !self.stage.ended {
            return None;
        }
        match self.kit() {
            Some(kit) => kit.outcome(),
            None => self.plain().and_then(|h| h.outcome),
        }
    }

    /// The game's say (`c.scored`), with ready-made rules or without.
    fn scored(&self) -> bool {
        self.scored
    }

    fn score(&self) -> u32 {
        self.stage.host.score()
    }

    /// Every player of the run its line (those who left too), or the
    /// team's one (`c.team_score`); a game of one its player's.
    fn scores(&self) -> Vec<(usize, u32)> {
        let first = self.stage.host.score();
        let others = self.stage.seats.iter().enumerate().skip(1).filter(|(_, s)| s.been).map(|(i, s)| (i + 1, s.score));
        if self.stage.host.sim().cfg.team_score {
            return vec![(1, first + others.map(|(_, n)| n).sum::<u32>())];
        }
        std::iter::once((1, first)).chain(others).collect()
    }

    fn rumble(&self) -> Option<(f32, u32)> {
        self.kit().and_then(|k| k.rumble())
    }

    fn summary(&self) -> Option<String> {
        self.kit().and_then(|k| k.summary())
    }

    fn music(&self) -> Option<crate::game::MusicCue> {
        let m = &self.stage.music;
        Some(crate::game::MusicCue { song: m.song?, beats: m.beats(&self.stage.songs)?, intensity: m.intensity })
    }

    /// A song gone from the list leaves the music to the player's choice.
    fn set_songs(&mut self, songs: &[crate::audio::music::Timing]) {
        self.stage.songs = Arc::new(songs.to_vec());
        let m = &mut self.stage.music;
        if m.song.is_some_and(|s| s >= songs.len()) || m.next.is_some_and(|(s, _)| s.is_some_and(|s| s >= songs.len())) {
            *m = host::Music { intensity: m.intensity, ..host::Music::default() };
        }
    }

    fn uses_music(&self) -> bool {
        self.uses_music
    }

    fn numbers(&self) -> Vec<String> {
        self.kit().map(|k| k.numbers()).unwrap_or_default()
    }

    fn report(&self) -> Vec<String> {
        let mut lines = match (self.kit(), self.plain()) {
            (Some(kit), _) => kit.report(),
            (None, Some(h)) => {
                let mut line = format!("{} things | score {}", h.things.len(), h.score);
                if let Some(b) = self.focus_body() {
                    line += &format!(" | focus at ({:.1}, {:.1}, {:.1})", b.pos.x, b.pos.y, b.pos.z);
                }
                vec![line]
            }
            (None, None) => unreachable!("a host is plain or the kit"),
        };
        if let Some(e) = self.halted() {
            lines.push(format!("halted: {}", e.lines().next().unwrap_or_default()));
        }
        lines
    }

    fn place_player(&mut self, x: f32, y: f32) {
        if let Some(kit) = self.kit_mut() {
            return kit.place_player(x, y);
        }
        let h = self.plain_mut().expect("a plain host without a kit");
        let half = h.sim.half();
        let Some(id) = h.focus else { return };
        let world = &h.sim.world;
        let Some(t) = h.things.get_mut(id) else { return };
        let at = spawn_point(world, [x, y], t.body.height, half);
        (t.body.pos, t.body.prev, t.body.vel) = (at, at, Vec3::ZERO);
        let target = at + Vec3::Z * t.body.height * 0.5;
        (h.sim.cam_target, h.sim.cam_prev) = (target, target);
    }

    /// New settings: things keep their kind by its name, a kind gone takes
    /// its things with it.
    fn apply_config(&mut self, cfg: &GameConfig) -> Result<()> {
        self.stage.languages = Arc::new(cfg.languages.clone());
        let old = self.stage.kinds.clone();
        let kinds = Arc::new(Self::kinds(cfg, |name, model| old.iter().find(|k| k.name == name && k.model.as_deref() == model).map_or(1.0, |k| k.height))?);
        // Checked before anything is taken: a mistake keeps the old config.
        Self::player_kind(cfg, &kinds)?;
        Self::check_items(cfg)?;
        Self::check_kinds_sounds(cfg, &kinds)?;
        // The views of things' states hold their kinds' keys (hp or not),
        // their props are their kinds' anew.
        self.sandbox.lua.set_named_registry_value("voxcade.views", self.sandbox.lua.create_table()?)?;
        self.sandbox.lua.set_named_registry_value("voxcade.props", self.sandbox.lua.create_table()?)?;
        self.stage.hud.apply(cfg);
        match self.kit_mut() {
            Some(kit) => kit.apply_config(cfg)?,
            None => {
                let h = self.plain_mut().expect("a plain host without a kit");
                let now = |kind: usize| kinds.iter().position(|k| k.name == old[kind].name);
                h.things.retain(|t| now(t.kind).is_some());
                for t in h.things.iter_mut() {
                    t.kind = now(t.kind).expect("kept by its kind");
                }
                h.kinds = kinds.clone();
                h.sim.apply_config(cfg);
            }
        }
        self.stage.kinds = kinds;
        self.stage.sounds = Self::sound_names(cfg);
        self.scored = cfg.scored;
        let player = self.registry("voxcade.player")?;
        if !cfg.items.is_empty() && player.raw_get::<mlua::Value>("inventory")?.is_nil() {
            player.raw_set("inventory", self.sandbox.lua.create_table()?)?;
        }
        // A state the config made wrong (an item gone) is mended by it too.
        if self.state_failed.get() && self.check_states().is_ok() {
            self.mended();
        }
        Ok(())
    }

    /// A new level: the kit starts its things over; without one the things
    /// stay, lifted out of anything that is now where they stand.
    fn replace_world(&mut self, world: World, markers: &[Marker]) {
        if let Some(kit) = self.kit_mut() {
            return kit.replace_world(world, markers);
        }
        let h = self.plain_mut().expect("a plain host without a kit");
        h.sim.replace_world(world);
        let half = h.sim.half();
        let world = &h.sim.world;
        for t in h.things.iter_mut() {
            while body_hits(world, t.body.pos, half, t.body.height) && t.body.pos.z < world.size.z as f32 + 8.0 {
                t.body.pos.z += 1.0;
            }
            t.body.prev = t.body.pos;
        }
    }

    /// New models: the kinds' heights follow them.
    fn reshape(&mut self, models: &Models) {
        let kinds: Vec<Kind> = self.stage.kinds.iter().map(|k| Kind { height: k.model.as_deref().and_then(|m| models.shape(m)).map_or(1.0, |s| s.height()), ..k.clone() }).collect();
        let kinds = Arc::new(kinds);
        self.stage.kinds = kinds.clone();
        self.stage.frames = Arc::new(models.frame_counts());
        self.stage.heights = Arc::new(models.heights());
        self.stage.bursts = Arc::new(bursts(&kinds, models));
        self.stage.parts = Arc::new(models.part_names());
        match self.kit_mut() {
            Some(kit) => kit.reshape(models),
            None => self.plain_mut().expect("a plain host without a kit").kinds = kinds,
        }
    }
}

/// The game's scenarios by name (`scenarios/NAME.lua`), in order.
fn scenario_names(files: &Files) -> Vec<String> {
    files.list("scenarios").iter().filter_map(|p| p.file_name()?.to_str()?.strip_suffix(".lua").map(str::to_string)).collect()
}

/// What a scenario's file gave back: a table.
fn scenario_table(path: &str, got: mlua::MultiValue) -> Result<Table> {
    match got.into_iter().next() {
        Some(mlua::Value::Table(t)) => Ok(t),
        _ => anyhow::bail!("{path}: a scenario gives back a table: return {{ about = \"...\", setup = function() ... end }}"),
    }
}

/// A value as the console shows it: plain data as a state is written in
/// a save, anything else as `tostring` gives it (a thing, a function).
fn shown(v: &mlua::Value, tostring: &Function) -> String {
    match v {
        mlua::Value::Table(_) => match value::capture(v, "", &mut value::Seen::default(), false) {
            Ok(Some(plain)) => data::write(&plain),
            _ => tostring.call::<String>(v.clone()).unwrap_or_default(),
        },
        mlua::Value::String(s) => format!("{:?}", s.to_string_lossy()),
        other => tostring.call::<String>(other.clone()).unwrap_or_default(),
    }
}


//! The Lua door: a game as a folder with `conf.lua` (its settings, set in
//! code: `function conf(c) ... end`) and `main.lua` (its handlers:
//! `on_world_start()` once, `on_level_start(how)` when a level begins,
//! `on_tick()` 60 times a second, `on_draw(frame)` every picture),
//! after LÖVE but deterministic: replays, `shot` and snapshots work as for
//! every game. The scripts see the game model's objects as globals
//! (`world`, `level`, `player`, `input`, `screen`; `world/`,
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
//! behaviors run by themselves each tick, the script's `on_tick` after
//! them; without them the world holds only what the script moves
//! (`host::Plain`).

pub mod api;
mod calls;
pub mod conf;
mod course;
pub mod data;
mod events;
mod hashing;
pub mod hidden;
pub mod host;
pub mod lex;
mod sandbox;
pub mod save;
mod saving;
mod seats;
mod start;
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

/// A thing of a kind that its level's file turns (`facing`, `rotation`)
/// must be one the script moves: a kind with a behavior turns by its
/// ready-made rules, as `level:spawn`'s facing says.
fn check_facings(levels: &crate::levels::Levels, kinds: &[Kind]) -> Result<()> {
    for level in &levels.list {
        for (m, f) in level.markers.iter().zip(&level.marker_facings) {
            if let (Some(_), Some(k)) = (f, kinds.get(m.kind))
                && !k.scripted
            {
                anyhow::bail!("level {}: a {} faces as its ready-made rules turn it (its behavior): facing and turn in a level's file are for kinds without one, as level:spawn's facing", level.name, k.name);
            }
        }
    }
    Ok(())
}

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
    /// Whether its code calls `frame:camera`: the models out of view are
    /// left out only once the picture's camera is known
    /// (`Game::moves_camera`).
    moves_camera: bool,
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

/// The handler a global function `on_...` nearly is, if it is none
/// itself: a letter or two off (one for the short ones, `on_tik`), case
/// aside. Other names starting with `on_` are the game's own.
fn near_handler(name: &str) -> Option<&'static str> {
    let rest = name.strip_prefix("on_")?.to_lowercase();
    let handlers = api::API.iter().filter(|i| matches!(i.what, api::What::Callback { .. }) && i.name.starts_with("on_")).map(|i| i.name);
    if handlers.clone().any(|h| h == name) || RENAMED.iter().any(|(old, _)| *old == name) {
        return None;
    }
    handlers
        .map(|h| (h, edits(&rest, &h["on_".len()..])))
        .filter(|(h, d)| *d <= if h.len() - "on_".len() <= 4 { 1 } else { 2 })
        .min_by_key(|(_, d)| *d)
        .map(|(h, _)| h)
}

/// How many letters to put in, take out, change or swap with the next to
/// make `a` into `b` (Damerau-Levenshtein, the optimal string alignment).
fn edits(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// A global function nearly a handler's name (`function on_tik()`) is
/// a mistake at load: nothing would ever call it.
fn misspelt_handler(env: &Table) -> Result<()> {
    let mut found: Vec<(String, Function)> = Vec::new();
    for pair in env.pairs::<mlua::Value, mlua::Value>() {
        if let (mlua::Value::String(k), mlua::Value::Function(f)) = pair? {
            found.push((k.to_string_lossy(), f));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, f) in found {
        if let Some(near) = near_handler(&name) {
            let info = f.info();
            let at = format!("{}:{}", info.short_src.unwrap_or_else(|| "main.lua".into()), info.line_defined.unwrap_or(0));
            anyhow::bail!("{at}: function {name} is no handler the engine calls: did you mean {near}? (a function of your own wants a name further from the handlers')");
        }
    }
    Ok(())
}

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
            // A piece's pose, a part of it changed since it was set too.
            if self.stage.pieces.contains_key(&id) {
                self.piece_pose(id, &state)?;
            }
        }
        Ok(())
    }

    /// The pose of piece `id` its state `state` holds (`thing.state.parts`;
    /// empty: at rest), or the mistake in it, naming the piece and the part.
    fn piece_pose(&self, id: u32, state: &Table) -> Result<Vec<crate::session::PoseEntry>> {
        let v: mlua::Value = state.raw_get(world::PARTS)?;
        if v.is_nil() {
            return Ok(Vec::new());
        }
        let model = self.stage.pieces.get(&id).and_then(|p| p.model.as_deref());
        Ok(world::piece_pose(&v, model, &self.stage.parts, &world::piece_name(&self.stage, id))?)
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

    /// A line for OUTPUT (the IDE's), not a mistake.
    fn note(&self, text: &str) {
        if let Ok(note) = self.sandbox.lua.named_registry_value::<Function>("voxcade.note") {
            let _ = note.call::<()>(text.to_string());
        }
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
        // A section nearly a setting's name (`c.titel`) is a slip: the one
        // meant (the engine says the rest, `session::config_in`).
        let sections = Self::sections(cfg);
        let known: Vec<String> = api::conf_keys("GameConfig").into_iter().map(|k| k.key).chain(sections.iter().map(|s| s.to_string())).collect();
        if let Some((given, near)) = cfg.game.keys().filter(|k| !sections.contains(&k.as_str())).find_map(|k| crate::audio::words::near(k, known.iter().map(String::as_str)).map(|n| (k.clone(), n.to_string()))) {
            anyhow::bail!("conf.lua: c.{given}: no such setting (did you mean `c.{near}`?)");
        }
        Ok(())
    }

    fn config_file() -> Option<&'static str> {
        Some("conf.lua")
    }

    fn sections(cfg: &GameConfig) -> Vec<&'static str> {
        if with::<K>(cfg) { K::sections(cfg) } else { Vec::new() }
    }

    fn behaviors(cfg: &GameConfig) -> Vec<&'static str> {
        if with::<K>(cfg) { K::behaviors(cfg) } else { Vec::new() }
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
        let effects: Vec<String> = things::defs(cfg, &Self::behaviors(cfg)).map(|defs| defs.into_iter().filter_map(|d| d.effect.filter(|e| !e.bursts()).map(|e| e.model)).collect()).unwrap_or_default();
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
        check_facings(&levels, &kinds)?;
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
            moves_camera: false,
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

    fn moves_camera(&self) -> bool {
        self.moves_camera
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

    /// A level read anew turns a thing of a kind with a behavior: refused
    /// as at the load (`check_facings`).
    fn check_levels(&self, levels: &crate::levels::Levels) -> Result<()> {
        check_facings(levels, &self.stage.kinds)
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

    /// A tick: the kit's behaviors (if it has one), the script's
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
                // Its figure's parts posed as its state says (checked at
                // every tick's end: a mistake halts the game, and the
                // piece stands at rest meanwhile).
                let pose = match self.registry("voxcade.data").and_then(|d| Ok(d.raw_get::<Option<Table>>(id)?)) {
                    Ok(Some(state)) => self.piece_pose(id, &state).unwrap_or_else(|e| {
                        if self.halted.borrow().is_none() {
                            self.halt(e);
                        }
                        Vec::new()
                    }),
                    _ => Vec::new(),
                };
                if pose.is_empty() {
                    out.model_turned(model, 0, out.body_origin(t.body), t.body.look, turn, None);
                } else {
                    let posed = crate::session::posing_alone(&pose);
                    out.model_posed(model, 0, out.body_origin(t.body), t.body.look, turn, &posed, None);
                }
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
                viewing: out.viewing().cloned(),
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
                // (Its code names it as Lua writes a call, or what is out
                // of the game's own view would be missing.)
                Cmd::Camera(_) if !self.moves_camera => {
                    if self.halted.borrow().is_none() {
                        self.halt(anyhow::anyhow!("frame:camera: call it as frame:camera({{ ... }}) in the game's code, so the engine knows the game sets its camera"));
                    }
                }
                Cmd::Camera(cam) => out.camera(cam),
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

    fn music_mix(&self) -> crate::audio::music::Mix {
        self.stage.music.mix
    }

    fn music_step(&mut self) {
        if self.halted.borrow().is_none() {
            let songs = Arc::clone(&self.stage.songs);
            self.stage.music.step(&songs);
        }
    }

    fn music_switch(&self) -> crate::game::MusicSwitch {
        let m = &self.stage.music;
        crate::game::MusicSwitch { pending: m.next.map(|(_, _, cross)| cross), began: m.crossed }
    }

    fn music(&self) -> Option<crate::game::MusicCue> {
        let m = &self.stage.music;
        Some(crate::game::MusicCue { song: m.song?, beats: m.beats(&self.stage.songs)?, intensity: m.intensity })
    }

    /// A song gone from the list leaves the music to the player's choice.
    fn set_songs(&mut self, songs: &[crate::audio::music::Timing]) {
        self.stage.songs = Arc::new(songs.to_vec());
        let m = &mut self.stage.music;
        if m.song.is_some_and(|s| s >= songs.len()) || m.next.is_some_and(|(s, _, _)| s.is_some_and(|s| s >= songs.len())) {
            *m = host::Music { intensity: m.intensity, mix: m.mix, ..host::Music::default() };
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


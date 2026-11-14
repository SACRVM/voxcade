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
mod checks;
pub mod conf;
mod console;
mod course;
pub mod data;
mod drawing;
mod events;
mod hashing;
pub mod hidden;
pub mod host;
mod inspect;
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
use checks::*;
use host::{Ask, Host, Kind, Kit, PLAYER, Plain, Stage};
use sandbox::Sandbox;
use value::Value;
use world::{Absent, Cmd, ControlsView, Frame, InputView, LevelView, ScreenView, WorldView};

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

/// The parts of a Lua game's state hash, in the order it takes them in
/// (`LuaGame::hash_by_parts`): what a desync names.
const PARTS: [&str; 9] = ["things", "world.state", "player.state", "level.state", "levels", "kept", "thing.state", "seats", "music"];

/// A Lua game, with the kit `K` when its `conf.lua` asks for it.
pub struct LuaGame<K: Kit> {
    stage: Stage,
    sandbox: Sandbox,
    /// The game's code as `main.lua` last ran (`start::run_main`).
    code: Code,
    /// The game's files (`crate::files`): a scenario and a console line
    /// read theirs there.
    files: Files,
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
    /// Whether the game stands halted by `on_draw` (not by an earlier
    /// mistake): a model file that came may mend it (`models_came`).
    draw_halted: Cell<bool>,
    /// Whether the state check at a tick's end stopped the game (a change
    /// of `conf.lua`, its items, may mend it too).
    state_failed: Cell<bool>,
    /// The save the run goes on from at once (`Game::start_from`).
    start_text: Option<String>,
    /// The thing the player arrives at in the level entered next (a
    /// checkpoint by its name).
    arrive_at: Option<String>,
    /// Where a save stood the players of the seats after the first, for the
    /// level it enters.
    seat_places: Vec<(usize, crate::levels::Place)>,
    /// The saves made since the session took them (`Game::saving`).
    written: Vec<(u32, String)>,
    /// What `on_draw` asked for the last time it ran (shown while stopped):
    /// the picture's part drawn in `draw`, the HUD's in `hud`.
    drawn: RefCell<Vec<Cmd>>,
    kit: PhantomData<K>,
}

/// The game's code as it last ran: its handlers, globals and environment,
/// what of it counts for replays, and what it names.
#[derive(Default)]
struct Code {
    tick: Option<Function>,
    draw: Option<Function>,
    /// The script's globals as `on_world_start` left them (the kit's
    /// questions are answered by its functions there).
    globals: Option<Table>,
    /// `main.lua` as tokens (what counts of it for replays).
    main: Vec<u8>,
    /// The game's Lua files `main.lua` required (`require`), by path, as
    /// tokens: its code as much as `main.lua` is.
    modules: Vec<(String, Vec<u8>)>,
    /// The files `main.lua` read as it started (`read`): they count for
    /// replays too, and saving one runs `main.lua` anew.
    data: Vec<(String, Vec<u8>)>,
    /// Its environment once started (the globals frozen) and what each
    /// file required gave back: a scenario and a console line run there,
    /// `require` giving the same modules.
    env: Option<Table>,
    required: Vec<(String, mlua::Value)>,
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
        self.draw_halted.set(false);
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
    /// `plain`); the first mistake, named where it sits (the path made
    /// only then).
    fn check_states(&self) -> Result<()> {
        let lua = &self.sandbox.lua;
        let plain = self.sandbox.tool("plain")?;
        let is_thing: Function = lua.named_registry_value("voxcade.is_thing")?;
        let seen = lua.create_table()?;
        let check = |table: Table, path: &dyn Fn() -> String, things: bool| -> Result<()> {
            let (why, keys): (Option<String>, Option<Table>) = self.sandbox.call_engine(&plain, (table, &seen, things, &is_thing))?;
            let Some(why) = why else { return Ok(()) };
            let mut at = path();
            if let Some(keys) = keys {
                let keys: Vec<mlua::Value> = keys.sequence_values().collect::<mlua::Result<_>>()?;
                for k in keys.iter().rev() {
                    at = value::key_path(&at, k);
                }
            }
            Err(value::mistake(&at, &why))
        };
        check(self.registry("voxcade.state")?, &|| "world.state".into(), false)?;
        check(self.registry("voxcade.player")?, &|| "player.state".into(), false)?;
        check(self.registry("voxcade.level")?, &|| "level.state".into(), true)?;
        let kept = self.registry("voxcade.kept")?;
        let mut names: Vec<String> = kept.pairs::<String, mlua::Value>().filter_map(|p| p.ok()).map(|(n, _)| n).collect();
        names.sort();
        for name in names {
            check(kept.raw_get(name.as_str())?, &|| format!("level.kept of {name}"), false)?;
        }
        let data = self.registry("voxcade.data")?;
        let mut ids: Vec<u32> = data.pairs::<u32, mlua::Value>().filter_map(|p| p.ok()).map(|(id, _)| id).collect();
        ids.sort_unstable();
        let items = &self.stage.host.sim().cfg.items;
        let player = self.registry("voxcade.player")?;
        check_inventory(&player, &|| "player.state".into(), items)?;
        for n in 2..=self.stage.seats.len() {
            if let Some(state) = self.registry("voxcade.seats")?.raw_get::<Option<Table>>(n)? {
                let path = || format!("player {n}'s state");
                check(state.clone(), &path, false)?;
                check_inventory(&state, &path, items)?;
            }
        }
        for id in ids {
            let state: Table = data.raw_get(id)?;
            let path = || format!("thing {id}'s state");
            check(state.clone(), &path, true)?;
            check_inventory(&state, &path, items)?;
            // A piece's pose, a part of it changed since it was set too.
            if self.stage.here.pieces.contains_key(&id) {
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
        let model = self.stage.here.pieces.get(&id).and_then(|p| p.model.as_deref());
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
    /// simulation), and the items' and the kinds' effects', kept for the
    /// whole run; any other model file `frame:model` loads when it is
    /// first drawn (`session::Needed`).
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
        for file in cfg.items.values().filter_map(|i| i.model.as_ref()).chain(effects.iter()) {
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
        Self::begin(setup)
    }

    fn snapshot(&self) -> Result<LuaSnapshot> {
        // (Taken between ticks: `on_tick` runs in none.)
        let stage = Stage { ticking: false, ..self.stage.clone() };
        Ok(LuaSnapshot { stage, script: self.capture()?, before: self.before, held_for: self.held_for })
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
        let mut code = if self.code.main.is_empty() { Vec::new() } else { vec![("main.lua".into(), self.code.main.clone())] };
        code.extend(self.code.modules.iter().cloned());
        code.extend(self.code.data.iter().cloned());
        code
    }

    /// `main.lua`, a Lua file it required or a file it read changed:
    /// `main.lua` runs anew.
    fn reload_code(&mut self, files: &Files, file: &Path) -> Result<bool> {
        let Some(changed) = files.name(file) else { return Ok(false) };
        if changed != "main.lua" && !self.code.modules.iter().chain(&self.code.data).any(|(name, _)| *name == changed) {
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

    /// The level being played and how often it was entered.
    fn level_mark(&self) -> u64 {
        let visits = self.stage.visits.get(self.stage.level).copied().unwrap_or(0);
        (self.stage.level as u64) << 32 | visits as u64
    }

    /// The models of the level being played's pieces.
    fn level_models(&self) -> Vec<String> {
        let level = self.stage.levels.list.get(self.stage.level);
        let mut out: Vec<String> = Vec::new();
        for m in level.iter().flat_map(|l| l.pieces.iter().filter_map(|p| p.model.clone())) {
            if !out.contains(&m) {
                out.push(m);
            }
        }
        out
    }

    /// A model file came: `on_draw` stopped by `frame:model` naming one
    /// not there (or broken) draws again, and stops again if it still
    /// cannot.
    fn models_came(&mut self) -> bool {
        if !self.draw_failed.get() {
            return false;
        }
        self.draw_failed.set(false);
        if self.draw_halted.take() {
            *self.halted.borrow_mut() = None;
        }
        true
    }

    fn moves_sun(&self) -> bool {
        self.code.moves_sun
    }

    fn moves_camera(&self) -> bool {
        self.code.moves_camera
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
        let reloaded = self.call_handler("on_level_end", "reload", Some(world::Phase::LevelEnd)).and_then(|()| self.enter(index, place, "reload", None));
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
        self.scenarios_about()
    }

    fn start_scenario(&mut self, name: &str) -> Result<()> {
        self.begin_scenario(name)
    }

    fn eval(&mut self, code: &str) -> Result<String> {
        self.console_line(code)
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
        let told = self.call_handler("on_level_end", "quit", Some(world::Phase::LevelEnd)).and_then(|()| self.call_handler("on_world_end", "quit", Some(world::Phase::WorldEnd)));
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
        self.inspected()
    }

    /// The locals of `main.lua` its functions keep (upvalues).
    fn hidden(&self) -> Vec<(String, u64)> {
        self.code.globals.as_ref().and_then(|g| hidden::upvalues(&self.sandbox.lua, g).ok()).unwrap_or_default()
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
        let on_beat = self.code.globals.as_ref().and_then(|g| Sandbox::function(g, "on_beat").ok().flatten());
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
        if let Some(tick) = self.code.tick.clone().filter(|_| self.halted.borrow().is_none() && !over) {
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
        self.draw_game(out)
    }

    /// The HUD `on_draw` drew (from the top left in UI voxels), and the
    /// voxels of icons that burst.
    fn hud(&self, cfg: &UiConfig, alpha: f32, aspect: f32, font: Option<&DisplayFont>) -> Vec<Instance> {
        self.draw_hud(cfg, alpha, aspect, font)
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
        self.code.uses_music
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
        check_props(&self.stage.levels, &kinds)?;
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


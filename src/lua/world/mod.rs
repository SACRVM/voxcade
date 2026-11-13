//! What a Lua game's scripts see, the game model's objects
//! (`docs/game-model.md`): the globals `world` (the run), `level` (the level
//! being played: its voxels, things, effects and camera), `player` (the one
//! who plays), `input` (the keys held, in `on_tick`) and `screen` (what
//! `on_draw` puts in the picture), and thing handles. `world`, `level` and
//! `player` hold nothing themselves: they find the game of the call they are
//! used in (`voxcade.world` in the registry); `input` and `screen` are put
//! in place for their calls (`Sandbox::provide`). A thing handle is only an
//! id, found the same way. In `on_draw` everything is only to look at.
//!
//! Everything that changes while playing is in a state: `world.state`,
//! `level.state`, `player.state` and `thing.state`; the engine's own
//! numbers in them (`player.state.score`, `thing.state.hp`) are read and
//! written through to where the engine keeps them (`engine_state`). There is
//! one handle per thing and one view per state, so a thing is the same
//! value wherever it comes from and `t.state == t.state`.

use std::sync::Arc;

use glam::{IVec3, Vec3};
use mlua::{AnyUserData, Function, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods, Value};

use super::host::{Ask, PLAYER, Seen, Stage, Touched, handle_seat, seat_handle};
use crate::sim::{Event, Who};
use crate::levels::{Level, Place};
use crate::input::Input;
use crate::rng::Rng;
use crate::sim::things::{Id, Walk};
use crate::controls::Controls;

mod run;
mod level;
mod thing;
mod input;
mod menus;
mod music;
mod screen;
mod frame;
mod controls;
mod pose;

pub use run::*;
pub use level::*;
pub use thing::*;
pub use input::*;
pub use menus::*;
pub use music::*;
pub use screen::*;
pub use frame::*;
pub use controls::*;
pub use pose::*;

impl Stage {
    fn kind(&self, name: &str) -> mlua::Result<usize> {
        self.kinds.iter().position(|k| k.name == name).ok_or_else(|| {
            let names: Vec<&str> = self.kinds.iter().map(|k| k.name.as_str()).filter(|n| *n != crate::grid::PIECE).collect();
            mlua::Error::runtime(format!("no kind of thing called '{name}' (c.things: {})", names.join(", ")))
        })
    }

    fn thing(&self, id: u32) -> mlua::Result<Seen<'_>> {
        match self.seen(id) {
            Some(t) => Ok(t),
            None if id == PLAYER => Err(mlua::Error::runtime(self.no_figure())),
            None => match handle_seat(id) {
                Some(seat) => Err(mlua::Error::runtime(self.not_standing(seat))),
                None => Err(mlua::Error::runtime(format!("thing {id} is gone"))),
            },
        }
    }

    /// Why seat `seat`'s player has no figure standing now.
    fn not_standing(&self, seat: usize) -> String {
        match self.seats.get(seat - 1) {
            Some(s) if !s.taken => format!("player {seat}: nobody sits on seat {seat}"),
            Some(s) if s.out => format!("player {seat} is out of play (state.out)"),
            _ => format!("player {seat} is not in the level yet: its figure comes in at the tick's end"),
        }
    }

    /// The thing behind a handle: for a player its figure, a thing of the
    /// level.
    fn real(&self, id: u32, what: &str) -> mlua::Result<u32> {
        match handle_seat(id) {
            None => Ok(id),
            Some(1) => match self.host.player_id() {
                Some(figure) => Ok(figure.raw()),
                None if self.host.player_seen().is_some() => Err(mlua::Error::runtime(format!("{what}: the player moves by its ready-made rules"))),
                None => Err(mlua::Error::runtime(format!("{what}: {}", self.no_figure()))),
            },
            Some(seat) => match self.host.seat_figure(seat) {
                Some(figure) => Ok(figure.raw()),
                None => Err(mlua::Error::runtime(format!("{what}: {}", self.not_standing(seat)))),
            },
        }
    }

    /// What the host acts on for a handle: a thing's id, a player's figure,
    /// or `PLAYER` for ready-made rules' own player; None where a player
    /// has no figure.
    fn target(&self, id: u32) -> Option<u32> {
        match handle_seat(id) {
            None => Some(id),
            Some(1) => self.host.player_id().map(Id::raw).or_else(|| self.host.player_seen().map(|_| PLAYER)),
            Some(seat) => self.host.seat_figure(seat).map(Id::raw),
        }
    }

    /// Who a handle is to the rules' events.
    fn who(&self, id: u32) -> Who {
        match handle_seat(id) {
            Some(1) => Who::Player,
            Some(seat) => self.host.seat_figure(seat).map_or(Who::Player, Who::Thing),
            None => Who::Thing(Id::from_raw(id)),
        }
    }

    /// Why a handle has nothing for the host to act on.
    fn no_target(&self, id: u32, what: &str) -> mlua::Error {
        match handle_seat(id) {
            Some(1) | None => mlua::Error::runtime(format!("{what}: {}", self.no_figure())),
            Some(seat) => mlua::Error::runtime(format!("{what}: {}", self.not_standing(seat))),
        }
    }

    fn no_figure(&self) -> &'static str {
        if (self.host.as_ref() as &dyn std::any::Any).is::<super::host::Plain>() { NO_FIGURE } else { NO_FIGURE_HERE }
    }

    fn thing_mut(&mut self, id: u32) -> mlua::Result<Touched<'_>> {
        let id = self.real(id, "player")?;
        self.host.get_mut(Id::from_raw(id)).ok_or_else(|| mlua::Error::runtime(format!("thing {id} is gone")))
    }

    /// The name of a thing's kind (None: the shooter's player, `c.player`).
    fn kind_name(&self, kind: usize) -> Option<&str> {
        self.kinds.get(kind).map(|k| k.name.as_str())
    }

    /// The body and walk of one of the things the script moves (ready-made
    /// rules move their own).
    fn moved(&mut self, id: u32, what: &str) -> mlua::Result<(&mut crate::sim::Body, &mut Walk)> {
        let id = self.real(id, what)?;
        let kind = self.thing(id)?.kind;
        let name = self.kinds[kind].name.clone();
        self.thing_mut(id)?
            .moved
            .ok_or_else(|| mlua::Error::runtime(format!("{what}: a {name} moves by its ready-made rules (its behavior); the script moves kinds without one, and things whose behavior is off (thing:enable)")))
    }

    /// Takes `new` as what happens at the tick's end if it counts: going on
    /// outranks starting over, among equals the first; a note for OUTPUT
    /// when one of the two did not count (the same ask twice is quiet).
    fn ask(&mut self, new: Ask) -> Option<String> {
        let note = match &self.ask {
            None => None,
            Some(held) if *held == new => return None,
            Some(held) if new.outranks(held) => Some(format!("world:{} takes the place of the world:{} asked before (going on counts before starting over)", verb(&new), verb(held))),
            Some(held) => return Some(format!("world:{} does not count: world:{} was asked first this tick", verb(&new), verb(held))),
        };
        self.ask = Some(new);
        note
    }

    /// Where level `index` starts: its place called `start`, or else where
    /// its ready-made rules stand the player (the platformer's hero marker,
    /// the shooter's `c.player.spawn`), or else `c.player.spawn` on its
    /// ground.
    pub fn start(&self, index: usize) -> Place {
        let level = &self.levels.list[index];
        if let Some(p) = level.places.iter().find(|p| p.name == "start") {
            return p.clone();
        }
        let sim = self.host.sim();
        let at = self.host.start_at(&level.world, &level.markers);
        Place::new("start", at.unwrap_or_else(|| crate::sim::spawn_point(&level.world, sim.cfg.player.spawn, 1.0, sim.half())), None)
    }

    /// A kind or a family by its name (`level:things("gem")`,
    /// `level:things("enemies")`).
    fn pick(&self, name: &str, what: &str) -> mlua::Result<Pick> {
        if let Some(k) = self.kinds.iter().position(|k| k.name == name) {
            return Ok(Pick::Kind(k));
        }
        if self.kinds.iter().any(|k| k.families.iter().any(|f| f == name)) {
            return Ok(Pick::Family(name.to_string()));
        }
        let kinds: Vec<&str> = self.kinds.iter().map(|k| k.name.as_str()).filter(|n| *n != crate::grid::PIECE).collect();
        let mut families: Vec<&str> = self.kinds.iter().flat_map(|k| k.families.iter().map(String::as_str)).collect();
        families.sort_unstable();
        families.dedup();
        let families = if families.is_empty() { String::new() } else { format!("; families: {}", families.join(", ")) };
        Err(mlua::Error::runtime(format!("{what}: no kind of thing or family called '{name}' (c.things: {}{families})", kinds.join(", "))))
    }

    /// Whether thing `id` is of what `pick` names.
    fn is(&self, id: u32, pick: &Pick) -> bool {
        let Some(t) = self.seen(id) else { return false };
        match pick {
            Pick::Kind(k) => t.kind == *k,
            Pick::Family(f) => self.kinds.get(t.kind).is_some_and(|k| k.families.contains(f)),
        }
    }

    /// The level's things (of a kind or family): not the player's figure,
    /// which belongs to no level, nor what died or was taken away.
    fn ids_of(&self, pick: Option<&Pick>) -> Vec<Id> {
        let figures: Vec<Id> = (1..=self.seats.len()).filter_map(|n| self.figure(n)).collect();
        self.host
            .ids()
            .into_iter()
            .filter(|&id| !figures.contains(&id) && !self.dead.contains(&id.raw()) && !self.gone.contains(&id.raw()) && pick.is_none_or(|p| self.is(id.raw(), p)))
            .collect()
    }

    /// Whether thing `id`'s body is in a block of the level whose kind has
    /// `flag` (`grid::HAZARD`, ...): any cell its box reaches.
    fn in_block(&self, id: u32, flag: u8) -> mlua::Result<bool> {
        let t = self.thing(id)?;
        let Some(cells) = self.host.sim().world.cells.as_ref() else { return Ok(false) };
        let (lo, hi) = (t.body.pos - Vec3::new(t.half, t.half, 0.0), t.body.pos + Vec3::new(t.half, t.half, t.body.height));
        let (step, origin) = (cells.cell.as_vec3(), cells.origin.as_vec3());
        let first = ((lo - origin) / step).floor().as_ivec3();
        let last = ((hi - 1e-3 - origin) / step).floor().as_ivec3();
        for z in first.z..=last.z {
            for y in first.y..=last.y {
                for x in first.x..=last.x {
                    let corner = origin + (IVec3::new(x, y, z).as_vec3() + 0.5) * step;
                    if cells.at(corner) & flag != 0 {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

/// What `level:things` and the like ask for: a kind, or a family.
enum Pick {
    Kind(usize),
    Family(String),
}

/// A block's kind by its name in a script (`level:block`), its bits in the
/// level's cells (`grid`).
const BLOCK_KINDS: [(&str, u8); 4] = [("hazard", crate::grid::HAZARD), ("ladder", crate::grid::LADDER), ("oneway", crate::grid::ONEWAY), ("solid", crate::grid::SOLID)];

fn block_flag(kind: &str, what: &str) -> mlua::Result<u8> {
    BLOCK_KINDS.iter().find(|(n, _)| *n == kind).map(|(_, f)| *f).ok_or_else(|| mlua::Error::runtime(format!("{what}: no kind of block called '{kind}' (hazard, ladder, oneway, solid)")))
}

/// A value of `conf.lua` (a kind's props) as Lua has it.
pub(super) fn from_toml(lua: &Lua, v: &toml::Value) -> mlua::Result<Value> {
    Ok(match v {
        toml::Value::String(s) => Value::String(lua.create_string(s)?),
        toml::Value::Integer(i) => Value::Integer(*i),
        toml::Value::Float(f) => Value::Number(*f),
        toml::Value::Boolean(b) => Value::Boolean(*b),
        toml::Value::Datetime(d) => Value::String(lua.create_string(d.to_string())?),
        toml::Value::Array(list) => {
            let t = lua.create_table()?;
            for (i, v) in list.iter().enumerate() {
                t.raw_set(i + 1, from_toml(lua, v)?)?;
            }
            Value::Table(t)
        }
        toml::Value::Table(map) => {
            let t = lua.create_table()?;
            for (k, v) in map {
                t.raw_set(k.as_str(), from_toml(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
}

/// A table only to read, everywhere (`thing.props`, `thing.families`:
/// `name` in its mistakes).
fn readonly(lua: &Lua, t: Table, name: &str) -> mlua::Result<Value> {
    lua.named_registry_value::<Function>("voxcade.readonly")?.call((t, name))
}

/// What `player` says where the player has no figure: without ready-made
/// rules, and with them (a level without their player's marker).
const NO_FIGURE: &str = "the player has no figure in this game (c.player.kind names the kind of thing it is)";
const NO_FIGURE_HERE: &str = "the player has no figure in this level (its ready-made rules place none here: no marker of theirs for it)";

/// The game the current call works on.
fn world(lua: &Lua) -> mlua::Result<AnyUserData> {
    lua.named_registry_value("voxcade.world").map_err(|_| mlua::Error::runtime("there is no game here yet (conf.lua and the top of main.lua run before it; use a handler: on_world_start, on_level_start, on_tick)"))
}

fn look<R>(lua: &Lua, f: impl FnOnce(&Stage) -> mlua::Result<R>) -> mlua::Result<R> {
    world(lua)?.borrow_scoped::<Stage, _>(f)?
}

fn change<R>(lua: &Lua, what: &str, f: impl FnOnce(&mut Stage) -> mlua::Result<R>) -> mlua::Result<R> {
    world(lua)?.borrow_mut_scoped::<Stage, _>(f).map_err(|_| mlua::Error::runtime(format!("{what}: on_draw only looks; change the game in on_tick")))?
}

/// Draws from the simulation's random generator (`math.random`): only in
/// the handlers that change the game (`on_world_start`, `on_level_start`,
/// `on_tick`).
pub fn with_rng<R>(lua: &Lua, f: impl FnOnce(&mut Rng) -> R) -> mlua::Result<R> {
    let w = world(lua).map_err(|_| mlua::Error::runtime("math.random: only where the game changes: on_world_start, on_level_start, on_tick"))?;
    w.borrow_mut_scoped::<Stage, _>(|h| f(&mut h.host.sim_mut().rng)).map_err(|_| mlua::Error::runtime("math.random: only where the game changes: on_world_start, on_level_start, on_tick (on_draw only looks)"))
}

/// The ask's name as a script writes it.
pub fn verb(ask: &Ask) -> &'static str {
    match ask {
        Ask::GoTo { .. } => "go_to",
        Ask::Restart { .. } => "restart",
        Ask::Load { .. } => "load",
        Ask::Won => "win",
        Ask::Lost => "lose",
    }
}

/// A save slot a script names, one of the game's (`c.saves.slots`).
fn slot(h: &Stage, slot: i64, what: &str) -> mlua::Result<u32> {
    match h.slots.len() {
        0 => Err(mlua::Error::runtime(format!("{what}: the game has no save slots (c.saves = {{ slots = 3 }} gives it three)"))),
        n if (1..=n as i64).contains(&slot) => Ok(slot as u32),
        n => Err(mlua::Error::runtime(format!("{what}: no slot {slot}: the game's are 1 to {n} (c.saves.slots)"))),
    }
}

/// `main.lua:LINE: ` for the script's line that called, if it can be told.
fn caller_line(lua: &Lua) -> String {
    lua.inspect_stack(1, |d| d.current_line()).flatten().map(|l| format!("main.lua:{l}: ")).unwrap_or_default()
}

/// Asks for a change of the game's course at the tick's end (`world:go_to`
/// and the like): not where there is no level to leave or it is being left
/// already; a note for OUTPUT when an ask did not count.
fn ask(lua: &Lua, what: &str, new: Ask) -> mlua::Result<()> {
    match lua.named_registry_value::<Option<String>>("voxcade.phase")?.as_deref() {
        Some("world_start") => return Err(mlua::Error::runtime(format!("{what}: there is no level to leave yet in on_world_start"))),
        Some("level_end" | "world_end") => return Err(mlua::Error::runtime(format!("{what}: the level is being left already (on_level_end and on_world_end ask nothing more)"))),
        _ => {}
    }
    // After the run's end nothing more counts (a standing `if … then
    // world:win() end` asks every tick).
    if look(lua, |h| Ok(h.ended))? {
        return Ok(());
    }
    if let Some(note) = change(lua, what, |h| Ok(h.ask(new)))? {
        let line = caller_line(lua);
        lua.named_registry_value::<Function>("voxcade.note")?.call::<()>(format!("{line}{note}"))?;
    }
    Ok(())
}

/// Whether `level` has a place called `name` (`start` always).
fn check_place(level: &Level, name: &str, what: &str) -> mlua::Result<()> {
    if name == "start" || level.places.iter().any(|p| p.name == name) {
        return Ok(());
    }
    let names: Vec<&str> = std::iter::once("start").chain(level.places.iter().map(|p| p.name.as_str())).collect();
    Err(mlua::Error::runtime(format!("{what}: no place called '{name}' in level '{}' (its places: {})", level.name, names.join(", "))))
}

/// A place as a script sees it: `{ x, y, z, facing = { x, y } }`, facing
/// only where the level gives one.
fn place_table(lua: &Lua, p: &Place) -> mlua::Result<Table> {
    let t = point(lua, p.at)?;
    if let Some(way) = p.facing {
        let facing = lua.create_table()?;
        facing.raw_set("x", way.x)?;
        facing.raw_set("y", way.y)?;
        t.raw_set("facing", facing)?;
    }
    Ok(t)
}

/// The longest hit stop a script may ask for, in ticks (a second): the
/// game holds still meanwhile, `on_tick` too.
const MAX_HITSTOP: u32 = 60;

/// Whether the current call only looks (`on_draw`).
fn looking(lua: &Lua) -> bool {
    lua.named_registry_value::<bool>("voxcade.looking").unwrap_or(false)
}

/// `level` is not there yet in `on_world_start`: the run begins before its
/// first level.
fn level_here(lua: &Lua, what: &str) -> mlua::Result<()> {
    match lua.named_registry_value::<Option<String>>("voxcade.phase")? {
        Some(phase) if phase == "world_start" => Err(mlua::Error::runtime(format!("{what}: there is no level yet in on_world_start; build it in on_level_start"))),
        _ => Ok(()),
    }
}

/// The handle of thing `id`: the same userdata every time while anything
/// holds it; the player's figure's is `player`.
pub fn handle(lua: &Lua, id: u32) -> mlua::Result<AnyUserData> {
    let id = folded(lua, id);
    let handles: Table = lua.named_registry_value("voxcade.handles")?;
    if let Some(h) = handles.raw_get::<Option<AnyUserData>>(id)? {
        return Ok(h);
    }
    let h = lua.create_userdata(ThingRef(id))?;
    handles.raw_set(id, &h)?;
    Ok(h)
}

/// `id`, or a player's handle where it is a player's figure.
fn folded(lua: &Lua, id: u32) -> u32 {
    if handle_seat(id).is_some() {
        return id;
    }
    look(lua, |h| Ok(h.figure_handle(Id::from_raw(id)))).ok().flatten().unwrap_or(id)
}

fn point(lua: &Lua, p: Vec3) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("x", p.x)?;
    t.raw_set("y", p.y)?;
    t.raw_set("z", p.z)?;
    Ok(t)
}

/// A point from a thing (the middle of its body) or a table with x, y, z
/// (or the first three numbers).
fn to_point(lua: &Lua, v: &Value, what: &str) -> mlua::Result<Vec3> {
    match v {
        Value::UserData(ud) if ud.is::<ThingRef>() => {
            let id = ud.borrow::<ThingRef>()?.0;
            look(lua, |h| Ok(h.thing(id)?.body.centre()))
        }
        Value::Table(t) => {
            // Through the table's own reading, so a point kept in a state
            // (the engine's view of it) is read too.
            let get = |name: &str, i: i64| -> mlua::Result<f32> {
                match t.get::<Option<f32>>(name)? {
                    Some(v) => Ok(v),
                    None => t.get::<Option<f32>>(i)?.ok_or_else(|| mlua::Error::runtime(format!("{what}: a point has x, y and z"))),
                }
            };
            Ok(Vec3::new(get("x", 1)?, get("y", 2)?, get("z", 3)?))
        }
        _ => Err(mlua::Error::runtime(format!("{what}: a thing or a point {{x=, y=, z=}}"))),
    }
}

/// A way on the ground from a table with x and y (or the first two numbers).
fn to_dir(v: &Value, what: &str) -> mlua::Result<Vec3> {
    let Value::Table(t) = v else { return Err(mlua::Error::runtime(format!("{what}: a way {{x=, y=}}"))) };
    let get = |name: &str, i: i64| -> mlua::Result<f32> { Ok(t.raw_get::<Option<f32>>(name)?.or(t.raw_get::<Option<f32>>(i)?).unwrap_or(0.0)) };
    Ok(Vec3::new(get("x", 1)?, get("y", 2)?, 0.0))
}

fn thing_id(v: &Value, what: &str) -> mlua::Result<u32> {
    match v {
        Value::UserData(ud) if ud.is::<ThingRef>() => Ok(ud.borrow::<ThingRef>()?.0),
        Value::Integer(i) if *i >= 0 => Ok(*i as u32),
        Value::Table(_) => Err(mlua::Error::runtime(format!("{what}: a thing, not a table (a point or a list): it goes by where a thing stands"))),
        other => Err(mlua::Error::runtime(format!("{what}: a thing, not {}", if other.is_nil() { "nil".to_string() } else { format!("a {}", other.type_name()) }))),
    }
}

/// A voxel's palette index from a script: an index (0: empty), nil (empty)
/// or a colour "#rrggbb" (taken into the world's palette).
fn palette_index(world: &mut crate::world::World, v: &Value, what: &str) -> mlua::Result<u8> {
    match v {
        Value::Nil => Ok(0),
        Value::Integer(i) if (0..256).contains(i) => Ok(*i as u8),
        Value::String(s) => {
            let text = s.to_string_lossy();
            let hex = text.strip_prefix('#').filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()));
            let Some(hex) = hex else { return Err(mlua::Error::runtime(format!("{what}: a colour is \"#rrggbb\", not \"{text}\""))) };
            let rgb = [0, 2, 4].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digits"));
            world.colour(rgb).ok_or_else(|| mlua::Error::runtime(format!("{what}: the world's palette is full (255 colours)")))
        }
        _ => Err(mlua::Error::runtime(format!("{what}: a palette index (0 to 255), nil or a colour \"#rrggbb\""))),
    }
}

/// A plain state kept in the registry under `name` (`world.state`,
/// `level.state`), only to look at in `on_draw`; there only while the game
/// runs a call (the top of `main.lua` has none to keep).
fn plain_state(lua: &Lua, name: &str) -> mlua::Result<Value> {
    world(lua)?;
    let t: Table = lua.named_registry_value(name)?;
    if looking(lua) { lua.named_registry_value::<Function>("voxcade.readonly")?.call(t) } else { Ok(Value::Table(t)) }
}

/// The view kept under `key` in the registry's `voxcade.views`, if one was
/// made (a state is read often: its view is made once).
fn view(lua: &Lua, key: impl mlua::IntoLua) -> mlua::Result<Option<Table>> {
    lua.named_registry_value::<Table>("voxcade.views")?.raw_get(key)
}

/// The view of a state the engine keeps some keys of (`keys`, read by `get`
/// and written by `set`), the rest in `backing` (the prelude's
/// `engine_state`), kept in the registry's `voxcade.views` under `key`.
fn engine_state(lua: &Lua, key: Value, backing: Table, keys: &[&str], get: Function, set: Function) -> mlua::Result<Value> {
    let views: Table = lua.named_registry_value("voxcade.views")?;
    let names = lua.create_table()?;
    for k in keys {
        names.raw_set(*k, true)?;
    }
    let is_looking: Function = lua.named_registry_value("voxcade.is_looking")?;
    let view: Table = lua.named_registry_value::<Function>("voxcade.engine_state")?.call((backing, names, get, set, is_looking))?;
    views.raw_set(key, &view)?;
    Ok(Value::Table(view))
}

/// A whole number for one of the engine's keys.
fn whole(v: Value, what: &str) -> mlua::Result<i64> {
    match v {
        Value::Integer(i) => Ok(i),
        Value::Number(n) if n.fract() == 0.0 => Ok(n as i64),
        other => Err(mlua::Error::runtime(format!("{what}: a whole number, not {}", other.type_name()))),
    }
}

/// `player.state.invulnerable = true`: as many ticks as never run out.
const FOREVER: i64 = u32::MAX as i64 / 2;

/// `player.state`: the run's state of the one who plays; its score (and
/// lives and hit points where the ready-made rules keep them) are the
/// engine's.
fn player_state(lua: &Lua) -> mlua::Result<Value> {
    world(lua)?;
    if let Some(v) = view(lua, "player")? {
        return Ok(Value::Table(v));
    }
    // The engine's keys, and the ones it kept before that are elsewhere
    // now (a mistake naming the new place).
    let keys = look(lua, |h| Ok(h.host.player_keys().iter().chain(h.host.moved_player_keys().iter().map(|(k, _)| k)).copied().collect::<Vec<_>>()))?;
    let backing: Table = lua.named_registry_value("voxcade.player")?;
    let moved = |lua: &Lua, key: &str| -> mlua::Result<()> {
        match look(lua, |h| Ok(h.host.moved_player_keys().iter().find(|(k, _)| *k == key).map(|(_, now)| *now)))? {
            Some(now) => Err(mlua::Error::runtime(format!("player.state.{key} is no more: {now}"))),
            None => Ok(()),
        }
    };
    let get = lua.create_function(move |lua, key: String| {
        if let Some(v) = seat_key(lua, 1, &key)? {
            return Ok(v);
        }
        moved(lua, &key)?;
        let (n, kept) = look(lua, |h| Ok((h.host.player_number(&key), h.host.player_kept(&key))))?;
        Ok(match (n, kept) {
            (Some(n), _) if key == "invulnerable" && n >= FOREVER => Value::Boolean(true),
            (Some(n), _) => Value::Integer(n),
            (None, Some(super::host::Kept::Text(t))) => Value::String(lua.create_string(t)?),
            (None, Some(super::host::Kept::Bool(b))) => Value::Boolean(b),
            (None, None) => Value::Nil,
        })
    })?;
    let set = lua.create_function(move |lua, (key, v): (String, Value)| {
        if SEAT_KEYS.contains(&key.as_str()) {
            return set_seat_key(lua, 1, &key, v);
        }
        moved(lua, &key)?;
        let what = format!("player.state.{key}");
        // A key that is no number (the shooter's weapon, the hero's pose):
        // by its name, or nil for none.
        let number = look(lua, |h| Ok(h.host.player_number(&key).is_some()))?;
        let kept = match (&v, number || key == "invulnerable") {
            (Value::String(text), false) => Some(super::host::Kept::Text(text.to_str()?.to_string())),
            (Value::Nil, false) => Some(super::host::Kept::Bool(false)),
            _ => None,
        };
        if let Some(kept) = kept {
            return change(lua, &what, |h| h.host.set_player_kept(&key, kept).map_err(|e| mlua::Error::runtime(format!("{what}: {e}"))));
        }
        // Kept from hits: a number of ticks, or true until set back.
        let v = match (key.as_str(), v) {
            ("invulnerable", Value::Boolean(on)) => if on { u32::MAX as i64 } else { 0 },
            ("invulnerable", Value::Nil) => 0,
            (_, v) => whole(v, &what)?,
        };
        if key == "score" && v < 0 {
            return Err(mlua::Error::runtime(format!("{what}: a score is 0 or more, not {v}")));
        }
        change(lua, &what, |h| {
            h.host.set_player_number(&key, v);
            Ok(())
        })
    })?;
    let keys: Vec<&str> = keys.into_iter().chain(SEAT_KEYS).collect();
    engine_state(lua, Value::String(lua.create_string("player")?), backing, &keys, get, set)
}

/// The engine's keys of every player's state besides its numbers: out of
/// play, its team (the players' own unless set), its figure's kind
/// (`c.player.kind`'s unless set; `thing.kind` is the figure's), each nil
/// until set (`docs/game-model.md`, "Several players").
const SEAT_KEYS: [&str; 3] = ["out", "team", "kind"];

/// One of `SEAT_KEYS` of seat `seat`'s player, None for another key.
fn seat_key(lua: &Lua, seat: usize, key: &str) -> mlua::Result<Option<Value>> {
    if !SEAT_KEYS.contains(&key) {
        return Ok(None);
    }
    // Nil until set, so a game of one sees its state as before (`pairs`).
    let (out, team, kind) = look(lua, |h| {
        let s = &h.seats[seat - 1];
        Ok((s.out, s.team.clone(), s.kind.clone()))
    })?;
    Ok(Some(match key {
        "out" if out => Value::Boolean(true),
        "out" => Value::Nil,
        "team" => team.map(|t| lua.create_string(t)).transpose()?.map_or(Value::Nil, Value::String),
        _ => kind.map(|k| lua.create_string(k)).transpose()?.map_or(Value::Nil, Value::String),
    }))
}

/// Sets one of `SEAT_KEYS` of seat `seat`'s player.
fn set_seat_key(lua: &Lua, seat: usize, key: &str, v: Value) -> mlua::Result<()> {
    let what = if seat == 1 { format!("player.state.{key}") } else { format!("player {seat}'s state.{key}") };
    match key {
        "out" => {
            let out = match v {
                Value::Boolean(b) => b,
                Value::Nil => false,
                other => return Err(mlua::Error::runtime(format!("{what}: true or false, not a {}", other.type_name()))),
            };
            change(lua, &what, |h| {
                if out && h.host.player_seen().is_some() && h.host.player_id().is_none() {
                    return Err(mlua::Error::runtime(format!("{what}: the ready-made rules' player stays in play")));
                }
                h.seats[seat - 1].out = out;
                Ok(())
            })
        }
        "team" => {
            let team = match v {
                Value::String(t) => Some(t.to_str()?.to_string()),
                Value::Nil => None,
                other => return Err(mlua::Error::runtime(format!("{what}: a team's name, not a {}", other.type_name()))),
            };
            change(lua, &what, |h| {
                h.seats[seat - 1].team = team.filter(|t| t != crate::sim::things::PLAYER_TEAM);
                Ok(())
            })
        }
        _ => {
            let kind = match v {
                Value::String(k) => Some(k.to_str()?.to_string()),
                Value::Nil => None,
                other => return Err(mlua::Error::runtime(format!("{what}: a kind's name (c.things), not a {}", other.type_name()))),
            };
            change(lua, &what, |h| {
                if let Some(k) = &kind {
                    h.kind(k).map_err(|e| mlua::Error::runtime(format!("{what}: {e}")))?;
                    if h.host.player_seen().is_some() && h.host.player_id().is_none() {
                        return Err(mlua::Error::runtime(format!("{what}: the ready-made rules' player is theirs")));
                    }
                }
                h.seats[seat - 1].kind = kind;
                Ok(())
            })
        }
    }
}

/// The state of seat `seat`'s player (2 and on): its score and hit points
/// the engine's, as `player.state`'s are, and `SEAT_KEYS`.
fn seat_state(lua: &Lua, seat: usize) -> mlua::Result<Value> {
    let key = format!("seat{seat}");
    if let Some(v) = view(lua, key.as_str())? {
        return Ok(Value::Table(v));
    }
    let seats: Table = lua.named_registry_value("voxcade.seats")?;
    let backing = match seats.raw_get::<Option<Table>>(seat)? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            seats.raw_set(seat, &t)?;
            t
        }
    };
    // Hit points where its kind has them, or a run starts with some.
    let has_hp = look(lua, |h| Ok(h.seat_kind(seat).is_some_and(|k| h.kinds[k].has_hp) || h.host.sim().cfg.state.player_number("hp").is_some()))?;
    let get = lua.create_function(move |lua, key: String| {
        if let Some(v) = seat_key(lua, seat, &key)? {
            return Ok(v);
        }
        let pose = look(lua, |h| Ok(h.host.seat_figure(seat).and_then(|f| h.host.thing_value(f, "pose")).filter(|_| key == "pose")))?;
        if let Some(super::host::Kept::Text(p)) = pose {
            return Ok(Value::String(lua.create_string(p)?));
        }
        // The gun in hand where the rules keep one (the shooter's).
        if key == "weapon" {
            return look(lua, |h| match h.host.seat_kept(seat, "weapon") {
                Some(super::host::Kept::Text(gun)) => Ok(Value::String(lua.create_string(gun)?)),
                _ => Ok(Value::Nil),
            });
        }
        look(lua, |h| {
            let s = &h.seats[seat - 1];
            let figure = h.host.seat_figure(seat);
            Ok(match key.as_str() {
                "score" => Value::Integer(s.score as i64),
                "pose" => Value::Nil,
                // A figure of the ready-made rules: its safe ticks.
                "invulnerable" => figure.and_then(|f| h.host.figure_number(f, "invulnerable")).filter(|&n| n > 0).map_or(Value::Nil, Value::Integer),
                _ => match figure.and_then(|f| h.host.get(f)) {
                    Some(t) => Value::Integer(t.hp as i64),
                    None => s.hp.map_or(Value::Nil, |hp| Value::Integer(hp as i64)),
                },
            })
        })
    })?;
    let set = lua.create_function(move |lua, (key, v): (String, Value)| {
        if SEAT_KEYS.contains(&key.as_str()) {
            return set_seat_key(lua, seat, &key, v);
        }
        let what = format!("player {seat}'s state.{key}");
        if key == "weapon" {
            let gun = match v {
                Value::String(s) => super::host::Kept::Text(s.to_str()?.to_string()),
                Value::Nil => super::host::Kept::Bool(false),
                other => return Err(mlua::Error::runtime(format!("{what}: the name of a weapon (c.weapons), not a {}", other.type_name()))),
            };
            return change(lua, &what, |h| h.host.set_seat_kept(seat, "weapon", gun).map_err(|e| mlua::Error::runtime(format!("{what}: {e}"))));
        }
        if key == "pose" {
            let pose = match v {
                Value::String(s) => super::host::Kept::Text(s.to_str()?.to_string()),
                Value::Nil => super::host::Kept::Bool(false),
                other => return Err(mlua::Error::runtime(format!("{what}: a name of its frames, not a {}", other.type_name()))),
            };
            return change(lua, &what, |h| match h.host.seat_figure(seat) {
                Some(f) => h.host.set_thing_value(f, "pose", pose).map_err(|e| mlua::Error::runtime(format!("{what}: {e}"))),
                None => Ok(()),
            });
        }
        let v = match (key.as_str(), v) {
            ("invulnerable", Value::Boolean(on)) => if on { u32::MAX as i64 } else { 0 },
            ("invulnerable", Value::Nil) => 0,
            (_, v) => whole(v, &what)?,
        };
        change(lua, &what, |h| {
            match key.as_str() {
                "invulnerable" => {
                    if let Some(f) = h.host.seat_figure(seat) {
                        h.host.set_figure_number(f, "invulnerable", v);
                    }
                }
                "score" if v < 0 => return Err(mlua::Error::runtime(format!("{what}: a score is 0 or more, not {v}"))),
                "score" => h.seats[seat - 1].score = v as u32,
                _ => match h.host.seat_figure(seat) {
                    Some(f) => *h.host.get_mut(f).ok_or_else(|| mlua::Error::runtime(format!("{what}: its figure is gone")))?.hp = v as i32,
                    None => h.seats[seat - 1].hp = Some(v as i32),
                },
            }
            Ok(())
        })
    })?;
    let keys: Vec<&str> = if has_hp { vec!["score", "hp"] } else { vec!["score"] };
    // A figure of the ready-made rules: its safe ticks, its pose (the jump
    // & run's) or its gun in hand (the shooter's) too.
    let (hero, own) = look(lua, |h| Ok((h.host.hero_kind().is_some(), h.host.player_keys())))?;
    let figure_keys: Vec<&str> = match (hero, own.contains(&"weapon")) {
        (true, _) => vec!["invulnerable", "pose"],
        (false, true) => vec!["invulnerable", "weapon"],
        _ => Vec::new(),
    };
    let keys: Vec<&str> = keys.into_iter().chain(SEAT_KEYS).chain(figure_keys).collect();
    engine_state(lua, Value::String(lua.create_string(&key)?), backing, &keys, get, set)
}

/// The key of a piece's light in its state (`thing.state.light`).
const LIGHT: &str = "light";

/// The key of a piece's pose in its state (`thing.state.parts`): its
/// figure's parts by name, each a rotation, as `frame:model`'s pose.
pub const PARTS: &str = "parts";

/// What a mistake calls thing `id`: its name, else its number.
pub fn piece_name(h: &Stage, id: u32) -> String {
    h.names.get(&id).map_or_else(|| format!("thing {id}"), |n| format!("'{n}'"))
}

/// The table thing `id`'s state keeps the script's keys in.
fn own_state(lua: &Lua, id: u32) -> mlua::Result<Table> {
    let all: Table = lua.named_registry_value("voxcade.data")?;
    match all.raw_get::<Option<Table>>(id)? {
        Some(t) => Ok(t),
        None => {
            let t = lua.create_table()?;
            all.raw_set(id, &t)?;
            Ok(t)
        }
    }
}

/// `thing.state`: a thing's state while it lives; its hit points (`hp`)
/// are the engine's where its kind has them. The player's is
/// `player.state`.
fn thing_state(lua: &Lua, id: u32) -> mlua::Result<Value> {
    match handle_seat(id) {
        Some(1) => return player_state(lua),
        Some(seat) => return seat_state(lua, seat),
        None => {}
    }
    let (has_hp, own, lit, piece) = look(lua, |h| Ok((h.kinds[h.thing(id)?.kind].has_hp, h.host.thing_keys(Id::from_raw(id)), h.lights.contains_key(&id), h.pieces.contains_key(&id))))?;
    if let Some(v) = view(lua, id)? {
        return Ok(Value::Table(v));
    }
    let all: Table = lua.named_registry_value("voxcade.data")?;
    let backing = match all.raw_get::<Option<Table>>(id)? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            all.raw_set(id, t.clone())?;
            t
        }
    };
    let get = lua.create_function(move |lua, key: String| {
        if key == "hp" {
            return look(lua, |h| Ok(Value::Integer(h.thing(id)?.hp as i64)));
        }
        if key == LIGHT {
            return look(lua, |h| Ok(h.lights.get(&id).map_or(Value::Nil, |b| Value::Number(*b as f64))));
        }
        // A piece's pose is the script's, kept in its state as it is.
        if key == PARTS {
            let v = own_state(lua, id)?.raw_get::<Value>(PARTS)?;
            return if looking(lua) { lua.named_registry_value::<Function>("voxcade.readonly")?.call(v) } else { Ok(v) };
        }
        let kept = look(lua, |h| Ok(h.host.thing_value(Id::from_raw(id), &key)))?;
        Ok(match kept {
            Some(super::host::Kept::Bool(b)) => Value::Boolean(b),
            Some(super::host::Kept::Text(t)) => Value::String(lua.create_string(t)?),
            None => Value::Nil,
        })
    })?;
    let set = lua.create_function(move |lua, (key, v): (String, Value)| {
        let what = format!("thing.state.{key}");
        if key == "hp" {
            let hp = whole(v, &what)?;
            return change(lua, &what, |h| {
                *h.thing_mut(id)?.hp = hp as i32;
                Ok(())
            });
        }
        if key == LIGHT {
            let b = match v {
                Value::Integer(n) => n as f64,
                Value::Number(n) => n,
                other => return Err(mlua::Error::runtime(format!("{what}: how bright its light is, 0 (off) to 1, not a {}", other.type_name()))),
            };
            if !(0.0..=1.0).contains(&b) {
                return Err(mlua::Error::runtime(format!("{what}: how bright its light is, 0 (off) to 1, not {b}")));
            }
            return change(lua, &what, |h| {
                h.lights.insert(id, b as f32);
                Ok(())
            });
        }
        // Checked as it is set (the line that set it named), and again at
        // the tick's end (a part changed inside it); kept as it is.
        if key == PARTS {
            look(lua, |h| piece_pose(&v, h.pieces.get(&id).and_then(|p| p.model.as_deref()), &h.parts, &piece_name(h, id)).map(|_| ()))?;
            return own_state(lua, id)?.raw_set(PARTS, v);
        }
        let value = match v {
            Value::Boolean(b) => super::host::Kept::Bool(b),
            Value::Nil => super::host::Kept::Bool(false),
            Value::String(s) => super::host::Kept::Text(s.to_str()?.to_string()),
            other => return Err(mlua::Error::runtime(format!("{what}: true, false or a name, not a {}", other.type_name()))),
        };
        change(lua, &what, |h| h.host.set_thing_value(Id::from_raw(id), &key, value).map_err(|e| mlua::Error::runtime(format!("{what}: {e}"))))
    })?;
    let mut keys: Vec<&str> = if has_hp { std::iter::once("hp").chain(own.iter().copied()).collect() } else { own.to_vec() };
    // A piece that brings a light: how bright it is.
    if lit {
        keys.push(LIGHT);
    }
    // A piece: how its figure's parts are posed.
    if piece {
        keys.push(PARTS);
    }
    engine_state(lua, Value::Integer(id as i64), backing, &keys, get, set)
}

/// The game as the registry holds it for a call; scripts see it through
/// `world`, `level` and `player`.
impl UserData for Stage {}

/// A key a script read or set on an engine's object, as it wrote it
/// (`tik`, `[1]`).
fn key_shown(key: &Value) -> String {
    match key {
        Value::String(s) => s.to_string_lossy().to_string(),
        Value::Integer(i) => format!("[{i}]"),
        Value::Number(n) => format!("[{n}]"),
        other => format!("[{}]", other.type_name()),
    }
}

/// Reading a name the engine's object `of` (one of `api::OWNERS`, or
/// `player`) does not have is a mistake, not a nil: it names the object,
/// the name, the one meant if one is near (a method with `:`, a field with
/// `.`), else what it has. `own`: where the game's own data goes, if the
/// object keeps a state.
pub(super) fn no_such(of: &str, key: &Value, own: Option<&str>) -> mlua::Error {
    let name = key_shown(key);
    let items: Vec<&crate::lua::api::Item> = crate::lua::api::items_of(of).collect();
    let call = |i: &crate::lua::api::Item| match i.what {
        crate::lua::api::What::Method { .. } => format!("{of}:{}", i.name),
        _ => format!("{of}.{}", i.name),
    };
    let near = near_item(&name, &items);
    let own = own.map_or(String::new(), |o| format!("; the game's own data goes in {o}"));
    match near {
        Some(i) => mlua::Error::runtime(format!("{of} has no `{name}`: did you mean `{}`?", call(i))),
        None => {
            let mut names: Vec<String> = items.iter().map(|i| call(i)).collect();
            names.sort();
            names.dedup();
            mlua::Error::runtime(format!("{of} has no `{name}` (it has {}){own}", names.join(", ")))
        }
    }
}

/// The name of `items` that `name` nearly is: a letter or two off (one
/// for the short ones), case and swapped letters as one.
fn near_item<'a>(name: &str, items: &[&'a crate::lua::api::Item]) -> Option<&'a crate::lua::api::Item> {
    let name = name.to_lowercase();
    items.iter().map(|i| (super::edits(&name, i.name), *i)).filter(|(d, i)| *d <= if i.name.len().min(name.len()) <= 4 { 1 } else { 2 }).min_by_key(|(d, _)| *d).map(|(_, i)| i)
}

/// Setting a name on an engine's object that is not one it lets a script
/// set: its fields are read only.
pub(super) fn not_settable(of: &str, key: &Value, own: Option<&str>) -> mlua::Error {
    let name = key_shown(key);
    let known = crate::lua::api::items_of(of).any(|i| i.name == name);
    let what = if known { format!("{of}.{name} is only to read") } else { format!("{of} has no `{name}` to set") };
    match own {
        Some(o) => mlua::Error::runtime(format!("{what}: the game's own data goes in {o}")),
        None => mlua::Error::runtime(format!("{what}: {of}'s fields are only to read")),
    }
}

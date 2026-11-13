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
        _ => Err(mlua::Error::runtime(format!("{what}: a thing"))),
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

/// `world`: the whole run.
pub struct WorldView;

impl UserData for WorldView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_function_get("state", |lua, _| plain_state(lua, "voxcade.state"));
        f.add_field_function_get("tick", |lua, _| look(lua, |h| Ok(h.host.sim().tick)));
        f.add_field_function_get("language", |lua, _| look(lua, |h| Ok(if looking(lua) { drawn_language(h) } else { language_of(h) })));
        f.add_field_function_get("levels", |lua, _| {
            let names: Vec<String> = look(lua, |h| Ok(h.levels.names().into_iter().map(String::from).collect()))?;
            lua.create_sequence_from(names)
        });
        f.add_field_function_get("leaving", |lua, _| look(lua, |h| Ok(h.ask.as_ref().map(Ask::why))));
        f.add_field_function_get("saves", |lua, _| {
            let slots = look(lua, |h| Ok(h.slots.clone()))?;
            let list = lua.create_table()?;
            for (n, info) in slots.into_iter().enumerate() {
                if let Some(info) = info {
                    let t = lua.create_table()?;
                    t.raw_set("level", info.level)?;
                    t.raw_set("seconds", info.ticks / 60)?;
                    list.raw_set(n + 1, t)?;
                }
            }
            Ok(list)
        });
        // Every seat: its player, or false while nobody sits there.
        f.add_field_function_get("players", |lua, _| {
            world(lua)?;
            let taken = look(lua, |h| Ok(h.seats.iter().map(|s| s.taken).collect::<Vec<_>>()))?;
            let list = lua.create_table()?;
            for (i, taken) in taken.into_iter().enumerate() {
                match taken {
                    true => list.raw_set(i + 1, handle(lua, seat_handle(i + 1))?)?,
                    false => list.raw_set(i + 1, false)?,
                }
            }
            Ok(list)
        });
        f.add_field_function_get("items", |lua, _| {
            let items = look(lua, |h| {
                let cfg = &h.host.sim().cfg;
                Ok(cfg.item_names().into_iter().map(|n| (n.to_string(), cfg.items[n].clone())).collect::<Vec<_>>())
            })?;
            let list = lua.create_table()?;
            for (n, (name, item)) in items.into_iter().enumerate() {
                let t = lua.create_table()?;
                t.raw_set("name", name)?;
                t.raw_set("model", item.model)?;
                t.raw_set("stack", item.stack)?;
                list.raw_set(n + 1, t)?;
            }
            Ok(list)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("world"));
        m.add_function("go_to", |lua, (_, level, place): (AnyUserData, Value, Option<String>)| {
            let index = look(lua, |h| {
                let n = h.levels.list.len();
                let index = match &level {
                    Value::String(name) => {
                        let name = name.to_string_lossy();
                        h.levels.find(&name).ok_or_else(|| mlua::Error::runtime(format!("world:go_to: no level called '{name}' (levels: {})", h.levels.names().join(", "))))?
                    }
                    Value::Integer(i) if *i >= 1 && *i as usize <= n => *i as usize - 1,
                    Value::Number(f) if f.fract() == 0.0 && *f >= 1.0 && *f as usize <= n => *f as usize - 1,
                    Value::Integer(_) | Value::Number(_) => return Err(mlua::Error::runtime(format!("world:go_to: no level {}: there are {n}, numbered from 1", level.to_string()?))),
                    Value::Nil => return Err(mlua::Error::runtime("world:go_to: which level? (nil: after the last level, level.next is nil; a game is won with world:win())")),
                    other => return Err(mlua::Error::runtime(format!("world:go_to: a level's name or number, not a {}", other.type_name()))),
                };
                if let Some(p) = &place {
                    check_place(&h.levels.list[index], p, "world:go_to")?;
                }
                Ok(h.levels.list[index].name.clone())
            })?;
            ask(lua, "world:go_to", Ask::GoTo { level: index, place })
        });
        m.add_function("restart", |lua, (_, place): (AnyUserData, Option<String>)| {
            if let Some(p) = &place {
                look(lua, |h| check_place(&h.levels.list[h.level], p, "world:restart"))?;
            }
            ask(lua, "world:restart", Ask::Restart { place })
        });
        m.add_function("save", |lua, (_, n): (AnyUserData, i64)| {
            if lua.named_registry_value::<Option<String>>("voxcade.phase")?.is_some() {
                return Err(mlua::Error::runtime("world:save: only while a level is played (on_level_start, on_tick): there is none to save here"));
            }
            change(lua, "world:save", |h| {
                let n = slot(h, n, "world:save")?;
                if !h.saving.contains(&n) {
                    h.saving.push(n);
                }
                Ok(())
            })
        });
        m.add_function("load", |lua, (_, n): (AnyUserData, i64)| {
            let n = look(lua, |h| {
                let n = slot(h, n, "world:load")?;
                match h.slots[n as usize - 1] {
                    Some(_) => Ok(n),
                    None => Err(mlua::Error::runtime(format!("world:load: slot {n} is empty (world.saves[{n}] tells before)"))),
                }
            })?;
            ask(lua, "world:load", Ask::Load { slot: Some(n) })
        });
        m.add_function("hitstop", |lua, (_, ticks): (AnyUserData, u32)| {
            change(lua, "world:hitstop", |h| {
                let sim = h.host.sim_mut();
                sim.freeze = sim.freeze.max(ticks.min(MAX_HITSTOP));
                Ok(())
            })
        });
        m.add_function("read", |lua, (_, name): (AnyUserData, String)| lua.named_registry_value::<Function>("voxcade.read")?.call::<mlua::LuaString>(name));
        m.add_function("win", |lua, _: AnyUserData| ask(lua, "world:win", Ask::Won));
        m.add_function("lose", |lua, _: AnyUserData| ask(lua, "world:lose", Ask::Lost));
    }
}

/// `level`: the level being played.
pub struct LevelView;

/// `level:spawn`'s options: `{ z =, facing =, state =, name =, props = }`.
struct SpawnOptions {
    z: Option<f32>,
    facing: Option<Vec3>,
    state: Option<Table>,
    name: Option<String>,
    props: Option<Table>,
}

fn spawn_options(v: Value) -> mlua::Result<SpawnOptions> {
    let mut o = SpawnOptions { z: None, facing: None, state: None, name: None, props: None };
    match v {
        Value::Nil => {}
        Value::Table(t) => {
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair?;
                match k.to_string()?.as_str() {
                    "z" => {
                        o.z = Some(match v {
                            Value::Integer(i) => i as f32,
                            Value::Number(n) => n as f32,
                            _ => return Err(mlua::Error::runtime("level:spawn: z is a number")),
                        })
                    }
                    "facing" => o.facing = Some(to_dir(&v, "level:spawn: facing")?),
                    "state" => match v {
                        Value::Table(s) => o.state = Some(s),
                        _ => return Err(mlua::Error::runtime("level:spawn: state is a table: the thing's first state")),
                    },
                    "name" => match v {
                        Value::String(n) => o.name = Some(n.to_str()?.to_string()),
                        _ => return Err(mlua::Error::runtime("level:spawn: name is a string: what level:thing(name) finds it by")),
                    },
                    "props" => match v {
                        Value::Table(p) => o.props = Some(p),
                        _ => return Err(mlua::Error::runtime("level:spawn: props is a table: what the game's code reads of this one thing, over its kind's")),
                    },
                    other => return Err(mlua::Error::runtime(format!("level:spawn: no option called '{other}' (z, facing, state, name, props)"))),
                }
            }
        }
        Value::Integer(_) | Value::Number(_) => return Err(mlua::Error::runtime("level:spawn: the fourth is a table of options: level:spawn(kind, x, y, { z = 3 })")),
        other => return Err(mlua::Error::runtime(format!("level:spawn: the options are a table, not a {}", other.type_name()))),
    }
    Ok(o)
}

/// A thing's own props are those its kind declares (`c.things.NAME.props`),
/// each of the kind its default is.
fn declared_props(kind: &str, declared: &toml::Table, given: &Table) -> mlua::Result<()> {
    for pair in given.pairs::<Value, Value>() {
        let (key, v) = pair?;
        let key = match key {
            Value::String(s) => s.to_str()?.to_string(),
            other => return Err(mlua::Error::runtime(format!("level:spawn: props are named, not a {} key", other.type_name()))),
        };
        let Some(default) = declared.get(&key) else {
            let names: Vec<&str> = declared.keys().map(String::as_str).collect();
            let has = if names.is_empty() { " declares none".to_string() } else { format!(": {}", names.join(", ")) };
            return Err(mlua::Error::runtime(format!("level:spawn: props.{key}: a {kind} has no such prop (c.things.{kind}.props{has})")));
        };
        let same = matches!(
            (default, &v),
            (toml::Value::Integer(_) | toml::Value::Float(_), Value::Integer(_) | Value::Number(_))
                | (toml::Value::String(_), Value::String(_))
                | (toml::Value::Boolean(_), Value::Boolean(_))
                | (toml::Value::Array(_) | toml::Value::Table(_), Value::Table(_))
        );
        if !same {
            let is = match default {
                toml::Value::Integer(_) | toml::Value::Float(_) => "number",
                toml::Value::String(_) => "string",
                toml::Value::Boolean(_) => "boolean",
                _ => "table",
            };
            return Err(mlua::Error::runtime(format!("level:spawn: props.{key} is a {is} by its kind (c.things.{kind}.props.{key} = {default}), not a {}", v.type_name())));
        }
    }
    Ok(())
}

impl UserData for LevelView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_function_get("state", |lua, _| {
            level_here(lua, "level.state")?;
            plain_state(lua, "voxcade.level")
        });
        f.add_field_function_get("name", |lua, _| {
            level_here(lua, "level.name")?;
            look(lua, |h| Ok(h.levels.list[h.level].name.clone()))
        });
        f.add_field_function_get("number", |lua, _| {
            level_here(lua, "level.number")?;
            look(lua, |h| Ok(h.level + 1))
        });
        f.add_field_function_get("next", |lua, _| {
            level_here(lua, "level.next")?;
            look(lua, |h| Ok(h.levels.list.get(h.level + 1).map(|l| l.name.clone())))
        });
        f.add_field_function_get("visits", |lua, _| {
            level_here(lua, "level.visits")?;
            look(lua, |h| Ok(h.visits[h.level]))
        });
        f.add_field_function_get("kept", |lua, _| {
            level_here(lua, "level.kept")?;
            let name = look(lua, |h| Ok(h.levels.list[h.level].name.clone()))?;
            let all: Table = lua.named_registry_value("voxcade.kept")?;
            let kept = match all.raw_get::<Option<Table>>(name.as_str())? {
                Some(t) => t,
                None => {
                    let t = lua.create_table()?;
                    all.raw_set(name.as_str(), &t)?;
                    t
                }
            };
            if looking(lua) { lua.named_registry_value::<Function>("voxcade.readonly")?.call(kept) } else { Ok(Value::Table(kept)) }
        });
        f.add_field_function_get("arrived", |lua, _| {
            level_here(lua, "level.arrived")?;
            let arrived = look(lua, |h| Ok(h.arrived.clone()))?;
            let t = place_table(lua, &arrived)?;
            t.raw_set("name", arrived.name.as_str())?;
            Ok(t)
        });
        f.add_field_function_get("places", |lua, _| {
            level_here(lua, "level.places")?;
            let (places, start) = look(lua, |h| Ok((h.levels.list[h.level].places.clone(), h.start(h.level))))?;
            let t = lua.create_table()?;
            for p in places.iter().chain([&start]) {
                t.raw_set(p.name.as_str(), place_table(lua, p)?)?;
            }
            Ok(t)
        });
        f.add_field_function_get("areas", |lua, _| {
            level_here(lua, "level.areas")?;
            let areas = look(lua, |h| Ok(h.levels.list[h.level].areas.clone()))?;
            let t = lua.create_table()?;
            for (i, a) in areas.iter().enumerate() {
                t.raw_set(a.name.as_str(), AreaView(i))?;
            }
            readonly(lua, t, "level.areas")
        });
        f.add_field_function_get("size", |lua, _| {
            level_here(lua, "level.size")?;
            let size = look(lua, |h| Ok(h.host.sim().world.size.as_vec3()))?;
            point(lua, size)
        });
        f.add_field_function_get("camera", |lua, _| {
            level_here(lua, "level.camera")?;
            if let Some(camera) = lua.named_registry_value::<Option<AnyUserData>>("voxcade.camera")? {
                return Ok(camera);
            }
            let camera = lua.create_userdata(CameraView)?;
            lua.set_named_registry_value("voxcade.camera", &camera)?;
            Ok(camera)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("level"));
        m.add_function("spawn", |lua, (_, kind, x, y, options): (AnyUserData, String, f32, f32, Value)| {
            level_here(lua, "level:spawn")?;
            let o = spawn_options(options)?;
            // The options are checked before the thing comes.
            let first = match &o.state {
                Some(first) => super::value::capture(&Value::Table(first.clone()), "level:spawn's state", &mut Default::default(), true).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?,
                None => None,
            };
            let props = match &o.props {
                Some(p) => {
                    let declared = look(lua, |h| Ok(h.kinds[h.kind(&kind)?].props.clone()))?;
                    declared_props(&kind, &declared, p)?;
                    super::value::capture(&Value::Table(p.clone()), "level:spawn's props", &mut Default::default(), false).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?
                }
                None => None,
            };
            if let Some(name) = &o.name
                && look(lua, |h| Ok(h.named(name).is_some()))?
            {
                return Err(mlua::Error::runtime(format!("level:spawn: a thing called '{name}' is in the level already (a name is one thing's)")));
            }
            if kind == crate::grid::PIECE {
                return Err(mlua::Error::runtime("level:spawn: \"piece\" is the kind of the models and lights a level's file sets down; spawn a kind of c.things"));
            }
            let id = change(lua, "level:spawn", |h| {
                let k = h.kind(&kind)?;
                if o.facing.is_some() && !h.kinds[k].scripted {
                    return Err(mlua::Error::runtime(format!("level:spawn: facing: a {kind} turns by its ready-made rules (its behavior); facing is for kinds the script moves")));
                }
                let id = h.host.spawn(k, x, y, o.z).map_err(|e| mlua::Error::runtime(format!("level:spawn: {e}")))?;
                if let Some(way) = o.facing {
                    h.moved(id.raw(), "level:spawn: facing")?.0.face(way);
                }
                if let Some(name) = o.name {
                    h.names.insert(id.raw(), name);
                }
                if let Some(props) = props {
                    h.props.insert(id.raw(), props);
                }
                Ok(id.raw())
            })?;
            // The ready-made rules may take it as the player (a hero where
            // none stood): its first state is then the player's.
            let id = folded(lua, id);
            if let Some(first) = first
                && let Value::Table(fresh) = super::value::restore(lua, &first).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?
                && let Value::Table(state) = thing_state(lua, id)?
            {
                for pair in fresh.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    state.set(k, v)?;
                }
            }
            handle(lua, id)
        });
        m.add_function("thing", |lua, (_, v): (AnyUserData, Value)| {
            level_here(lua, "level:thing")?;
            let id = match &v {
                Value::String(name) => {
                    let name = name.to_str()?.to_string();
                    match look(lua, |h| Ok(h.named(&name)))? {
                        Some(id) => id,
                        None => return Ok(None),
                    }
                }
                _ => thing_id(&v, "level:thing")?,
            };
            if look(lua, |h| Ok(h.seen(id).is_some() && !h.gone.contains(&id)))? { Ok(Some(handle(lua, id)?)) } else { Ok(None) }
        });
        m.add_function("things", |lua, (_, kind): (AnyUserData, Option<String>)| {
            level_here(lua, "level:things")?;
            let ids = look(lua, |h| {
                let pick = kind.map(|k| h.pick(&k, "level:things")).transpose()?;
                Ok(h.ids_of(pick.as_ref()))
            })?;
            let list = lua.create_table()?;
            for (n, id) in ids.into_iter().enumerate() {
                list.raw_set(n + 1, handle(lua, id.raw())?)?;
            }
            Ok(list)
        });
        // The players playing in this level, in seat order.
        m.add_function("players", |lua, _: AnyUserData| {
            level_here(lua, "level:players")?;
            let seats = look(lua, |h| Ok((1..=h.seats.len()).filter(|&n| h.playing(n)).collect::<Vec<_>>()))?;
            let list = lua.create_table()?;
            for (n, seat) in seats.into_iter().enumerate() {
                list.raw_set(n + 1, handle(lua, seat_handle(seat))?)?;
            }
            Ok(list)
        });
        m.add_function("count", |lua, (_, kind): (AnyUserData, Option<String>)| {
            level_here(lua, "level:count")?;
            look(lua, |h| {
                let pick = kind.map(|k| h.pick(&k, "level:count")).transpose()?;
                Ok(h.ids_of(pick.as_ref()).len())
            })
        });
        m.add_function("explode", |lua, (_, at, radius): (AnyUserData, Value, f32)| {
            level_here(lua, "level:explode")?;
            let at = to_point(lua, &at, "level:explode")?;
            change(lua, "level:explode", |h| {
                h.host.sim_mut().explode(at, radius);
                Ok(())
            })
        });
        m.add_function("clear_rounds", |lua, (_, team): (AnyUserData, Option<String>)| {
            level_here(lua, "level:clear_rounds")?;
            change(lua, "level:clear_rounds", |h| {
                if let Some(t) = &team
                    && t != crate::sim::things::PLAYER_TEAM
                    && !h.kinds.iter().any(|k| k.team.as_deref() == Some(t.as_str()))
                    && !h.seats.iter().any(|s| s.team.as_deref() == Some(t.as_str()))
                {
                    let seats = h.seats.iter().filter_map(|s| s.team.as_deref());
                    let mut teams: Vec<&str> = h.kinds.iter().filter_map(|k| k.team.as_deref()).chain(seats).chain([crate::sim::things::PLAYER_TEAM]).collect();
                    teams.sort_unstable();
                    teams.dedup();
                    return Err(mlua::Error::runtime(format!("level:clear_rounds: no team called '{t}' (the kinds' teams and the players': {})", teams.join(", "))));
                }
                h.host.clear_rounds(team.as_deref()).map_err(mlua::Error::runtime)
            })
        });
        m.add_function("sound", |lua, (_, name, at, volume): (AnyUserData, String, Value, Option<f32>)| {
            level_here(lua, "level:sound")?;
            let at = to_point(lua, &at, "level:sound")?;
            change(lua, "level:sound", |h| {
                let Some(&event) = h.sounds.iter().find(|s| **s == name) else { return Err(mlua::Error::runtime(format!("no sound called '{name}': a sound is a file, sounds/{name}.toml (the game's: {})", h.sounds.join(", ")))) };
                h.host.sim_mut().sound(event, at, volume.unwrap_or(1.0));
                Ok(())
            })
        });
        m.add_function("effect", |lua, (_, model, at, fps): (AnyUserData, String, Value, Option<f32>)| {
            level_here(lua, "level:effect")?;
            // Where a thing stands, or a point: the model's feet there.
            let at = match &at {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let id = ud.borrow::<ThingRef>()?.0;
                    look(lua, |h| Ok(h.thing(id)?.body.pos))?
                }
                _ => to_point(lua, &at, "level:effect")?,
            };
            change(lua, "level:effect", |h| {
                if !h.frames.contains_key(&model) {
                    return Err(mlua::Error::runtime(format!("level:effect: no model called '{model}' (a kind's model, or one in c.models)")));
                }
                h.effects.push(super::host::Shown { model, at, age: 0, fps: fps.unwrap_or(crate::sim::things::EFFECT_FPS).max(0.1) });
                Ok(())
            })
        });
        m.add_function("block", |lua, (_, x, y, z): (AnyUserData, f32, f32, f32)| {
            level_here(lua, "level:block")?;
            look(lua, |h| {
                let Some(cells) = h.host.sim().world.cells.as_ref() else { return Ok(None) };
                let bits = cells.at(Vec3::new(x, y, z) + 0.5);
                Ok(BLOCK_KINDS.iter().find(|(_, f)| bits & f != 0).map(|(n, _)| *n))
            })
        });
        m.add_function("voxel", |lua, (_, x, y, z): (AnyUserData, i32, i32, i32)| {
            level_here(lua, "level:voxel")?;
            look(lua, |h| Ok(h.host.sim().world.get(IVec3::new(x, y, z))))
        });
        m.add_function("solid", |lua, (_, x, y, z): (AnyUserData, i32, i32, i32)| {
            level_here(lua, "level:solid")?;
            look(lua, |h| Ok(h.host.sim().world.solid(IVec3::new(x, y, z))))
        });
        m.add_function("set_voxel", |lua, (_, x, y, z, v): (AnyUserData, i32, i32, i32, Value)| {
            level_here(lua, "level:set_voxel")?;
            change(lua, "level:set_voxel", |h| {
                let v = palette_index(&mut h.host.sim_mut().world, &v, "level:set_voxel")?;
                h.host.sim_mut().world.set(IVec3::new(x, y, z), v);
                Ok(())
            })
        });
        m.add_function("fill", |lua, (_, x0, y0, z0, x1, y1, z1, v): (AnyUserData, i32, i32, i32, i32, i32, i32, Value)| {
            level_here(lua, "level:fill")?;
            change(lua, "level:fill", |h| {
                let world = &mut h.host.sim_mut().world;
                let v = palette_index(world, &v, "level:fill")?;
                let (lo, hi) = (IVec3::new(x0, y0, z0).min(IVec3::new(x1, y1, z1)).max(IVec3::ZERO), IVec3::new(x0, y0, z0).max(IVec3::new(x1, y1, z1)).min(world.size - 1));
                for z in lo.z..=hi.z {
                    for y in lo.y..=hi.y {
                        for x in lo.x..=hi.x {
                            world.set(IVec3::new(x, y, z), v);
                        }
                    }
                }
                Ok(())
            })
        });
        m.add_function("ground", |lua, (_, x, y): (AnyUserData, i32, i32)| {
            level_here(lua, "level:ground")?;
            look(lua, |h| Ok(h.host.sim().world.column_top(x, y)))
        });
    }
}

/// One of the level's areas (`level.areas.NAME`), by its place in the
/// level's list: a named box, only to read.
#[derive(Clone, Copy)]
pub struct AreaView(pub usize);

impl AreaView {
    fn area(lua: &Lua, i: usize) -> mlua::Result<crate::levels::Area> {
        look(lua, |h| h.levels.list[h.level].areas.get(i).cloned().ok_or_else(|| mlua::Error::runtime("the area is gone: its level was left")))
    }
}

impl UserData for AreaView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_method_get("name", |lua, a| Ok(Self::area(lua, a.0)?.name));
        f.add_field_method_get("x", |lua, a| Ok(Self::area(lua, a.0)?.at.x));
        f.add_field_method_get("y", |lua, a| Ok(Self::area(lua, a.0)?.at.y));
        f.add_field_method_get("z", |lua, a| Ok(Self::area(lua, a.0)?.at.z));
        f.add_field_method_get("size", |lua, a| point(lua, Self::area(lua, a.0)?.size.as_vec3()));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |lua, a, ()| Ok(format!("area {}", Self::area(lua, a.0)?.name)));
        // A thing's place (where its feet stand, its pivot), or a point:
        // inside the box or not; a point without z, at any height.
        m.add_method("holds", |lua, a, v: Value| {
            let area = Self::area(lua, a.0)?;
            let (x, y, z) = match &v {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let id = ud.borrow::<ThingRef>()?.0;
                    let at = look(lua, |h| Ok(h.thing(id)?.body.pos))?;
                    (at.x, at.y, Some(at.z))
                }
                Value::Table(t) => {
                    let n = |k: &str| -> mlua::Result<Option<f32>> { t.get::<Option<f32>>(k) };
                    match (n("x")?, n("y")?) {
                        (Some(x), Some(y)) => (x, y, n("z")?),
                        _ => return Err(mlua::Error::runtime("area:holds: a thing, or a point { x =, y =, z = } (z left out: any height)")),
                    }
                }
                other => return Err(mlua::Error::runtime(format!("area:holds: a thing, or a point {{ x =, y =, z = }}, not a {}", other.type_name()))),
            };
            Ok(area.holds(x, y, z))
        });
    }
}

/// `level.camera`: where the camera looks.
pub struct CameraView;

impl UserData for CameraView {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("camera"));
        m.add_function("follow", |lua, (_, t): (AnyUserData, Value)| {
            level_here(lua, "level.camera:follow")?;
            // Nothing: the players again (the first playing; several, their middle).
            let id = match t {
                Value::Nil => look(lua, |h| Ok((1..=h.seats.len()).filter(|&n| h.playing(n)).find_map(|n| h.target(seat_handle(n)))))?,
                t => Some(thing_id(&t, "level.camera:follow")?),
            };
            let Some(id) = id else { return Ok(()) };
            change(lua, "level.camera:follow", |h| {
                h.thing(id)?;
                let id = h.target(id).unwrap_or(id);
                h.host.follow(Id::from_raw(id)).map_err(|e| mlua::Error::runtime(format!("level.camera:follow: {e}")))
            })
        });
        m.add_function("look_at", |lua, (_, x, y): (AnyUserData, f32, f32)| {
            level_here(lua, "level.camera:look_at")?;
            change(lua, "level.camera:look_at", |h| h.host.look_at(x, y).map_err(|e| mlua::Error::runtime(format!("level.camera:look_at: {e}"))))
        });
    }
}

/// A thing as a script holds it: its id. `player` is one too (`PLAYER`):
/// everything a thing has, its state the run's, its figure the host's
/// from level to level.
#[derive(Clone, Copy, Debug)]
pub struct ThingRef(pub u32);

impl UserData for ThingRef {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_method_get("id", |_, t| Ok(handle_seat(t.0).is_none().then_some(t.0)));
        f.add_field_method_get("seat", |_, t| Ok(handle_seat(t.0)));
        // A player's keys (`input` is seat 1's) and what it plays with.
        f.add_field_method_get("input", |lua, t| {
            let Some(seat) = handle_seat(t.0) else { return Err(mlua::Error::runtime("thing.input: only a player has keys (thing.seat)")) };
            let view = look(lua, |h| {
                if !h.ticking {
                    return Err(mlua::Error::runtime("player.input: the keys held are only there in on_tick"));
                }
                let s = &h.seats[seat - 1];
                Ok(InputView { now: s.input, before: s.before, actions: h.actions.clone(), held_for: s.held_for })
            })?;
            lua.create_userdata(view)
        });
        f.add_field_method_get("controls", |lua, t| match handle_seat(t.0) {
            Some(seat) => lua.create_userdata(ControlsView(seat)),
            None => Err(mlua::Error::runtime("thing.controls: only a player plays with something (thing.seat)")),
        });
        f.add_field_method_get("kind", |lua, t| look(lua, |h| Ok(h.kind_name(h.thing(t.0)?.kind).map(str::to_string))));
        f.add_field_method_get("x", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.pos.x)));
        f.add_field_method_get("y", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.pos.y)));
        f.add_field_method_get("z", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.pos.z)));
        f.add_field_method_get("on_ground", |lua, t| look(lua, |h| Ok(h.thing(t.0)?.body.on_ground)));
        // The way to a wall beside it (within half a voxel: bodies move in
        // steps of less), higher than a step (the way it faces first), or
        // nil.
        f.add_field_method_get("wall", |lua, t| {
            let way = look(lua, |h| {
                let seen = h.thing(t.0)?;
                let (b, half) = (seen.body, seen.half);
                let sim = h.host.sim();
                let lift = (sim.cfg.physics.step.max(0) as f32 + 0.01).min(b.height * 0.5);
                let ways = [b.look, glam::IVec2::X, glam::IVec2::NEG_X, glam::IVec2::Y, glam::IVec2::NEG_Y];
                Ok(ways.into_iter().filter(|w| *w != glam::IVec2::ZERO).find(|w| {
                    let at = b.pos + w.as_vec2().extend(0.0) * 0.5 + Vec3::Z * lift;
                    crate::sim::body_hits(&sim.world, at, half, b.height - lift)
                }))
            })?;
            match way {
                Some(w) => {
                    let t = lua.create_table()?;
                    t.raw_set("x", w.x)?;
                    t.raw_set("y", w.y)?;
                    Ok(Value::Table(t))
                }
                None => Ok(Value::Nil),
            }
        });
        f.add_field_method_get("facing", |lua, t| {
            let way = look(lua, |h| Ok(h.thing(t.0)?.body.look))?;
            let w = lua.create_table()?;
            w.raw_set("x", way.x)?;
            w.raw_set("y", way.y)?;
            Ok(w)
        });
        f.add_field_method_get("state", |lua, t| thing_state(lua, t.0));
        f.add_field_method_get("name", |lua, t| look(lua, |h| Ok(h.names.get(&t.0).cloned())));
        f.add_field_method_get("families", |lua, t| {
            let families = look(lua, |h| Ok(h.kinds.get(h.thing(t.0)?.kind).map(|k| k.families.clone()).unwrap_or_default()))?;
            let set = lua.create_table()?;
            for f in families {
                set.raw_set(f, true)?;
            }
            readonly(lua, set, "thing.families")
        });
        f.add_field_method_get("props", |lua, t| {
            let cache: Table = lua.named_registry_value("voxcade.props")?;
            if let Some(p) = cache.raw_get::<Option<Value>>(t.0)? {
                return Ok(p);
            }
            let (kind, own) = look(lua, |h| Ok((h.kinds.get(h.thing(t.0)?.kind).map(|k| k.props.clone()).unwrap_or_default(), h.props.get(&t.0).cloned())))?;
            let props = lua.create_table()?;
            for (k, v) in &kind {
                props.raw_set(k.as_str(), from_toml(lua, v)?)?;
            }
            if let Some(own) = own
                && let Value::Table(own) = super::value::restore(lua, &own).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?
            {
                for pair in own.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    props.raw_set(k, v)?;
                }
            }
            let props = readonly(lua, props, "thing.props")?;
            cache.raw_set(t.0, &props)?;
            Ok(props)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, a, b: AnyUserData| Ok(b.borrow::<ThingRef>().is_ok_and(|b| b.0 == a.0)));
        m.add_meta_method(MetaMethod::ToString, |lua, t, ()| {
            match handle_seat(t.0) {
                Some(1) => return Ok("player".to_string()),
                Some(seat) => return Ok(format!("player {seat}")),
                None => {}
            }
            let kind = look(lua, |h| Ok(h.host.get(Id::from_raw(t.0)).and_then(|x| h.kind_name(x.kind).map(str::to_string)))).ok().flatten();
            Ok(match kind {
                Some(kind) => format!("thing {} ({kind})", t.0),
                None => format!("thing {}", t.0),
            })
        });
        m.add_method("walk", |lua, t, (dir, speed): (Value, f32)| {
            let wish = to_dir(&dir, "thing:walk")?.normalize_or_zero();
            change(lua, "thing:walk", |h| {
                let (body, walk) = h.moved(t.0, "thing:walk")?;
                *walk = Walk { wish, speed };
                body.face(wish);
                Ok(())
            })
        });
        m.add_method("face", |lua, t, dir: Value| {
            let way = to_dir(&dir, "thing:face")?;
            change(lua, "thing:face", |h| {
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:face")) };
                h.thing(t.0)?;
                h.host.face(Id::from_raw(id), way).map_err(|e| mlua::Error::runtime(format!("thing:face: {e}")))
            })
        });
        m.add_method("jump", |lua, t, height: f32| {
            change(lua, "thing:jump", |h| {
                let gravity = h.host.sim().cfg.physics.gravity;
                let b = h.moved(t.0, "thing:jump")?.0;
                if b.on_ground {
                    b.vel.z = (2.0 * gravity * height.max(0.0)).sqrt();
                    b.on_ground = false;
                }
                Ok(())
            })
        });
        m.add_method("touching", |lua, t, what: Option<String>| {
            let ids = look(lua, |h| {
                h.thing(t.0)?;
                let pick = what.map(|w| h.pick(&w, "thing:touching")).transpose()?;
                let others = h.contacts().into_iter().filter_map(|(a, b)| if a == t.0 { Some(b) } else if b == t.0 { Some(a) } else { None });
                Ok(others.filter(|&o| pick.as_ref().is_none_or(|p| h.is(o, p))).collect::<Vec<u32>>())
            })?;
            let list = lua.create_table()?;
            for (n, id) in ids.into_iter().enumerate() {
                list.raw_set(n + 1, handle(lua, id)?)?;
            }
            Ok(list)
        });
        m.add_method("in_block", |lua, t, kind: String| {
            let flag = block_flag(&kind, "thing:in_block")?;
            look(lua, |h| h.in_block(t.0, flag))
        });
        m.add_method("launch", |lua, t, (dir, speed, rise): (Value, f32, Option<f32>)| {
            let way = to_dir(&dir, "thing:launch")?.normalize_or_zero();
            change(lua, "thing:launch", |h| {
                let gravity = h.host.sim().cfg.physics.gravity;
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:launch")) };
                h.thing(t.0)?;
                let up = rise.map_or(0.0, |r| (2.0 * gravity * r.max(0.0)).sqrt());
                h.host.launch(Id::from_raw(id), way * speed + Vec3::Z * up).map_err(|e| mlua::Error::runtime(format!("thing:launch: {e}")))
            })
        });
        m.add_method("enable", |lua, t, (behavior, on): (String, bool)| {
            change(lua, "thing:enable", |h| {
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:enable")) };
                h.thing(t.0)?;
                h.host.enable(Id::from_raw(id), &behavior, on).map_err(|e| mlua::Error::runtime(format!("thing:enable: {e}")))
            })
        });
        m.add_method("bounce", |lua, t, height: f32| {
            change(lua, "thing:bounce", |h| {
                let gravity = h.host.sim().cfg.physics.gravity;
                let Some(id) = h.target(t.0) else { return Err(h.no_target(t.0, "player:bounce")) };
                h.thing(t.0)?;
                let speed = (2.0 * gravity * height.max(0.0)).sqrt();
                h.host.bounce(Id::from_raw(id), speed).map_err(|e| mlua::Error::runtime(format!("thing:bounce: {e}")))
            })
        });
        m.add_method("ahead", |lua, t, distance: f32| {
            let p = look(lua, |h| {
                let b = h.thing(t.0)?.body;
                Ok(b.pos + b.facing * distance + Vec3::Z)
            })?;
            point(lua, p)
        });
        m.add_method("near", |lua, t, (other, distance): (Value, f32)| {
            let o = thing_id(&other, "thing:near")?;
            look(lua, |h| Ok(h.thing(t.0)?.body.pos.distance(h.thing(o)?.body.pos) < distance))
        });
        m.add_method("place", |lua, t, (first, y, z): (Value, Option<f32>, Option<f32>)| {
            // A place (`level.places.west`: where, and the way it faces), a
            // thing (where it stands) or x, y, z.
            let (x, y, z, facing) = match &first {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let other = ud.borrow::<ThingRef>()?.0;
                    let at = look(lua, |h| Ok(h.thing(other)?.body.pos))?;
                    (at.x, at.y, Some(at.z), None)
                }
                Value::Table(p) => {
                    let at = to_point(lua, &first, "thing:place")?;
                    let facing = p.raw_get::<Option<Value>>("facing")?.map(|f| to_dir(&f, "thing:place: facing")).transpose()?;
                    (at.x, at.y, Some(at.z), facing)
                }
                Value::Integer(_) | Value::Number(_) => {
                    let x = match first {
                        Value::Integer(i) => i as f32,
                        Value::Number(n) => n as f32,
                        _ => unreachable!("a number"),
                    };
                    let y = y.ok_or_else(|| mlua::Error::runtime("thing:place: x and y (and z), or a place"))?;
                    (x, y, z, None)
                }
                other => return Err(mlua::Error::runtime(format!("thing:place: x and y (and z), or a place, not a {}", other.type_name()))),
            };
            change(lua, "thing:place", |h| {
                // A player whose figure is not standing yet: where it comes in.
                if let Some(seat) = handle_seat(t.0).filter(|_| h.target(t.0).is_none())
                    && let Some(kind) = h.seat_kind(seat)
                    && h.seats[seat - 1].taken
                {
                    let (half, height) = (h.host.sim().half(), h.kinds[kind].height);
                    let at = match z {
                        Some(z) => Vec3::new(x, y, z),
                        None => crate::sim::stand_on_ground(&h.host.sim().world, Vec3::new(x, y, 0.0), height, half),
                    };
                    let way = facing.map(|f| glam::IVec2::new(f.x.round() as i32, f.y.round() as i32)).filter(|w| *w != glam::IVec2::ZERO);
                    h.seats[seat - 1].coming = Some((at, way));
                    return Ok(());
                }
                let (half, height) = (h.host.sim().half(), h.thing(t.0)?.body.height);
                let at = match z {
                    Some(z) => Vec3::new(x, y, z),
                    None => crate::sim::stand_on_ground(&h.host.sim().world, Vec3::new(x, y, 0.0), height, half),
                };
                // A seat's figure of the ready-made rules: stood there by them.
                if let Some(seat) = handle_seat(t.0).filter(|&n| n > 1)
                    && let Some(figure) = h.host.seat_figure(seat).filter(|&f| h.host.get_mut(f).is_some_and(|m| m.moved.is_none()))
                {
                    h.host.put_figure(figure, at).map_err(|e| mlua::Error::runtime(format!("player:place: {e}")))?;
                    h.placed.push(t.0);
                    return Ok(());
                }
                // The player of ready-made rules is stood there by them.
                if t.0 == PLAYER && h.host.player_id().is_none_or(|id| h.host.get_mut(id).is_some_and(|m| m.moved.is_none())) {
                    h.host.put_player(at).map_err(|e| mlua::Error::runtime(format!("player:place: {e}")))?;
                    h.placed.push(t.0);
                    return Ok(());
                }
                let body = h.moved(t.0, "thing:place")?.0;
                (body.pos, body.vel) = (at, Vec3::ZERO);
                if let Some(way) = facing {
                    body.face(way);
                }
                // What it touches there it touches without on_touch.
                h.placed.push(t.0);
                Ok(())
            })
        });
        m.add_method("attack", |lua, t, (weapon, target): (String, Option<Value>)| {
            let at = match target.filter(|v| !v.is_nil()) {
                Some(v) => {
                    let point = to_point(lua, &v, "thing:attack")?;
                    let id = match &v {
                        Value::UserData(ud) if ud.is::<ThingRef>() => Some(ud.borrow::<ThingRef>()?.0),
                        _ => None,
                    };
                    Some((point, id))
                }
                None => None,
            };
            change(lua, "thing:attack", |h| {
                h.thing(t.0)?;
                let who = h.who(t.0);
                let at = at.map(|(p, id)| (p, id.and_then(|id| h.target(id)).filter(|&id| id != PLAYER).map(Id::from_raw)));
                h.host.attack(who, &weapon, at).map_err(|e| mlua::Error::runtime(format!("thing:attack: {e}")))
            })
        });
        m.add_method("hurt", |lua, t, (damage, by, weapon): (i64, Option<Value>, Option<String>)| {
            if damage < 1 {
                return Err(mlua::Error::runtime("thing:hurt: damage is 1 or more (hit points back: set thing.state.hp)"));
            }
            let by = by.filter(|v| !v.is_nil()).map(|v| thing_id(&v, "thing:hurt: by")).transpose()?;
            change(lua, "thing:hurt", |h| {
                let who = |raw: u32| h.who(raw);
                let (whom, by_handle) = (who(t.0), by);
                let by = by.map(who);
                let kind = h.thing(t.0)?.kind;
                match h.kinds.get(kind) {
                    // The script's own kinds: their hit points, and the hit told.
                    Some(k) if k.scripted => {
                        if !k.has_hp {
                            let name = k.name.clone();
                            return Err(mlua::Error::runtime(format!("thing:hurt: a {name} has no hit points (c.things.{name}.hp)")));
                        }
                        let hp = h.thing_mut(t.0)?.hp;
                        *hp -= damage as i32;
                        if *hp <= 0 {
                            h.killers.insert(t.0, (by_handle, weapon.clone()));
                        }
                        h.later.push(Event::Hit { who: whom, by, damage: damage as i32, weapon: weapon.clone() });
                    }
                    // The ready-made rules' own, taken by their rules.
                    _ => {
                        h.host.hurt(whom, damage as i32, by, weapon.clone()).map_err(|e| mlua::Error::runtime(format!("thing:hurt: {e}")))?;
                        let told = std::mem::take(&mut h.host.sim_mut().events);
                        h.later.extend(told);
                    }
                }
                Ok(())
            })
        });
        m.add_method("take", |lua, t, (item, count): (String, Option<i64>)| {
            let count = count.unwrap_or(1);
            if count < 1 {
                return Err(mlua::Error::runtime("thing:take: how many is 1 or more"));
            }
            let stack = look(lua, |h| {
                let cfg = &h.host.sim().cfg;
                match cfg.items.get(&item) {
                    Some(i) => Ok(i.stack as i64),
                    None => Err(mlua::Error::runtime(format!("thing:take: no item called '{item}' (c.items: {})", cfg.item_names().join(", ")))),
                }
            })?;
            let Value::Table(state) = thing_state(lua, t.0)? else { return Err(mlua::Error::runtime("thing:take: no state")) };
            let inventory = match state.get::<Value>("inventory")? {
                Value::Table(inv) => inv,
                Value::Nil => {
                    let inv = lua.create_table()?;
                    state.set("inventory", &inv)?;
                    inv
                }
                other => return Err(mlua::Error::runtime(format!("thing:take: its state.inventory is a {}, not a table", other.type_name()))),
            };
            let have = inventory.get::<Option<i64>>(item.as_str())?.unwrap_or(0);
            let now = (have + count).min(stack).max(have);
            inventory.set(item.as_str(), now)?;
            let taken = now - have;
            if taken > 0 {
                change(lua, "thing:take", |h| {
                    let who = h.who(t.0);
                    h.later.push(Event::Pickup { who, item });
                    Ok(())
                })?;
            }
            Ok(taken)
        });
        m.add_method("remove", |lua, t, ()| {
            if handle_seat(t.0).is_some() {
                return Err(mlua::Error::runtime("player:remove: the player belongs to no level; it goes from level to level (out of play: state.out = true)"));
            }
            change(lua, "thing:remove", |h| {
                let there = h.host.get(Id::from_raw(t.0)).is_some() && !h.gone.contains(&t.0);
                if there {
                    h.gone.push(t.0);
                }
                Ok(there)
            })
        });
    }
}

/// `input`: the keys held this tick, by the game's action names.
pub struct InputView {
    pub now: Input,
    pub before: Input,
    pub actions: Arc<Vec<&'static str>>,
    /// How many ticks each direction (the first four) and action has been
    /// held (`input:repeated`).
    pub held_for: [u16; KEYS],
}

/// The four directions and the game's actions, each with its count of
/// ticks held.
pub const KEYS: usize = 4 + 32;
/// `input:repeated`: true as a key goes down, again after this many ticks
/// held, then every `REPEAT_EVERY`, as a menu moves on a held key.
pub const REPEAT_AFTER: u16 = 20;
pub const REPEAT_EVERY: u16 = 6;

/// Whether a key held `n` ticks repeats this tick.
pub fn repeats(n: u16) -> bool {
    n == 1 || (n > REPEAT_AFTER && (n - REPEAT_AFTER - 1).is_multiple_of(REPEAT_EVERY))
}

impl InputView {
    /// An action or a direction by its name: held now, and a tick before.
    fn key(&self, name: &str) -> mlua::Result<(bool, bool)> {
        let i = self.index(name)?;
        let held = |input: &Input| if i < 4 { input.directions()[i] } else { input.held(i - 4) };
        Ok((held(&self.now), held(&self.before)))
    }

    /// An action or a direction by its name, its place among `KEYS`.
    fn index(&self, name: &str) -> mlua::Result<usize> {
        if let Some(d) = crate::input::DIRECTIONS.iter().position(|d| *d == name) {
            return Ok(d);
        }
        match self.actions.iter().position(|a| *a == name) {
            Some(a) => Ok(4 + a),
            None => Err(mlua::Error::runtime(format!("no action called '{name}' (c.actions: {}; or a direction: left, right, up, down)", self.actions.join(", ")))),
        }
    }
}

impl UserData for InputView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_method_get("left", |_, i| Ok(i.now.left));
        f.add_field_method_get("right", |_, i| Ok(i.now.right));
        f.add_field_method_get("up", |_, i| Ok(i.now.up));
        f.add_field_method_get("down", |_, i| Ok(i.now.down));
        f.add_field_method_get("dir", |lua, i| {
            let n = &i.now;
            let x = n.right as i32 - n.left as i32;
            let y = n.up as i32 - n.down as i32;
            let t = lua.create_table()?;
            t.raw_set("x", x)?;
            t.raw_set("y", y)?;
            Ok(t)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("input"));
        m.add_method("held", |_, i, name: String| Ok(i.key(&name)?.0));
        m.add_method("pressed", |_, i, name: String| {
            let (now, before) = i.key(&name)?;
            Ok(now && !before)
        });
        m.add_method("released", |_, i, name: String| {
            let (now, before) = i.key(&name)?;
            Ok(!now && before)
        });
        m.add_method("repeated", |_, i, name: String| Ok(repeats(i.held_for[i.index(&name)?])));
    }
}

/// `input` outside `on_tick`, `screen` outside `on_draw`: using it says
/// where it is.
pub struct Absent(pub &'static str);

impl UserData for Absent {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |_, a, _: Value| -> mlua::Result<Value> { Err(mlua::Error::runtime(a.0)) });
        m.add_meta_method(MetaMethod::ToString, |_, a, ()| Ok(a.0.split(':').next().unwrap_or("").to_string()));
    }
}

/// What `on_draw` asked for, done after it returns.
#[derive(Clone, Debug)]
pub enum Cmd {
    /// `turn`: degrees round, nose up, rolled right (`session::turning`).
    Model { file: String, thing: u32, frame: Option<usize>, turn: [f32; 3], pose: Vec<crate::session::PoseEntry> },
    Cube { at: Vec3, color: [u8; 4], glow: f32 },
    Light { at: Vec3, radius: f32, color: [f32; 3] },
    /// The HUD's, from its top left in UI voxels.
    Text { text: String, x: f32, y: f32, color: [u8; 4], size: i32 },
    Box { x: f32, y: f32, width: f32, height: f32, color: [u8; 4] },
    Icon { name: String, x: f32, y: f32 },
    Drop { text: String, x: f32, y: f32, ticks: f32, color: [u8; 4], size: i32 },
    /// This picture's daylight (`frame:daylight`).
    Daylight(crate::session::Daylight),
    /// A menu in the engine's look (`frame:menu`).
    Menu(Menu),
    /// Slots in a grid, an inventory's (`frame:grid`).
    Grid(Grid),
}

/// `frame:menu`: a list in the engine's look, the game's to move.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    /// Its top left on the HUD (None: in the middle), and how wide.
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<f32>,
    pub title: Option<String>,
    pub tabs: Vec<String>,
    /// The chosen tab and row, from 0.
    pub tab: usize,
    pub rows: Vec<MenuRow>,
    pub at: Option<usize>,
    /// How many rows show at most (scrolled to keep `at` in view).
    pub shown: usize,
    pub hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuRow {
    pub text: String,
    pub value: Option<String>,
    pub icon: Option<String>,
    pub dim: bool,
}

/// `frame:grid`: slots in rows, each an icon and a count.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub columns: usize,
    /// A slot's size in UI voxels.
    pub cell: f32,
    pub title: Option<String>,
    pub cells: Vec<GridCell>,
    pub at: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GridCell {
    pub icon: Option<String>,
    pub count: Option<i64>,
    pub dim: bool,
}

/// A table's keys checked against those a call takes.
fn known_keys(t: &Table, keys: &[&str], what: &str) -> mlua::Result<()> {
    for pair in t.pairs::<Value, Value>() {
        let (k, _) = pair?;
        let ok = matches!(&k, Value::String(s) if keys.contains(&&*s.to_str()?));
        if !ok {
            let name = match &k {
                Value::String(s) => s.to_str()?.to_string(),
                other => format!("{other:?}"),
            };
            return Err(mlua::Error::runtime(format!("{what}: no '{name}' (it takes {})", keys.join(", "))));
        }
    }
    Ok(())
}

/// A chosen entry, counted from 1 by the script, from 0 here.
fn chosen(t: &Table, key: &str, count: usize, what: &str) -> mlua::Result<Option<usize>> {
    match t.raw_get::<Option<i64>>(key)? {
        None => Ok(None),
        Some(n) if n >= 1 && (n as usize) <= count => Ok(Some(n as usize - 1)),
        Some(n) => Err(mlua::Error::runtime(format!("{what}: {key} = {n}, but there are {count} (from 1)"))),
    }
}

/// `frame:menu{ ... }` read and checked, its texts' keys filled in.
fn menu(lua: &Lua, t: &Table) -> mlua::Result<Menu> {
    const WHAT: &str = "frame:menu";
    known_keys(t, &["x", "y", "width", "title", "tabs", "tab", "rows", "at", "shown", "hint"], WHAT)?;
    let fill = |s: String| look(lua, |h| fill_keys(h.drawn_controls(), &s));
    let mut rows = Vec::new();
    if let Some(list) = t.raw_get::<Option<Table>>("rows")? {
        for (n, v) in list.sequence_values::<Value>().enumerate() {
            rows.push(match v? {
                Value::String(s) => MenuRow { text: fill(s.to_str()?.to_string())?, value: None, icon: None, dim: false },
                Value::Table(r) => {
                    known_keys(&r, &["text", "value", "icon", "dim"], &format!("{WHAT}: rows[{}]", n + 1))?;
                    let value = match r.raw_get::<Value>("value")? {
                        Value::Nil => None,
                        Value::String(s) => Some(fill(s.to_str()?.to_string())?),
                        Value::Integer(i) => Some(i.to_string()),
                        Value::Number(x) => Some(x.to_string()),
                        _ => return Err(mlua::Error::runtime(format!("{WHAT}: rows[{}].value is a text or a number", n + 1))),
                    };
                    MenuRow {
                        text: fill(r.raw_get::<Option<String>>("text")?.unwrap_or_default())?,
                        value,
                        icon: r.raw_get("icon")?,
                        dim: r.raw_get::<Option<bool>>("dim")?.unwrap_or(false),
                    }
                }
                _ => return Err(mlua::Error::runtime(format!("{WHAT}: rows[{}] is a text or {{ text =, value =, icon =, dim = }}", n + 1))),
            });
        }
    }
    let tabs: Vec<String> = match t.raw_get::<Option<Table>>("tabs")? {
        Some(list) => list.sequence_values::<String>().map(|s| s.and_then(fill)).collect::<mlua::Result<_>>()?,
        None => Vec::new(),
    };
    let shown = match t.raw_get::<Option<i64>>("shown")? {
        Some(n) if n >= 1 => n as usize,
        Some(n) => return Err(mlua::Error::runtime(format!("{WHAT}: shown = {n}, at least 1 row"))),
        None => rows.len().max(1),
    };
    Ok(Menu {
        x: t.raw_get("x")?,
        y: t.raw_get("y")?,
        width: t.raw_get("width")?,
        title: t.raw_get::<Option<String>>("title")?.map(fill).transpose()?,
        tab: chosen(t, "tab", tabs.len(), WHAT)?.unwrap_or(0),
        at: chosen(t, "at", rows.len(), WHAT)?,
        tabs,
        rows,
        shown,
        hint: t.raw_get::<Option<String>>("hint")?.map(fill).transpose()?,
    })
}

/// `frame:grid{ ... }` read and checked.
fn grid(lua: &Lua, t: &Table) -> mlua::Result<Grid> {
    const WHAT: &str = "frame:grid";
    known_keys(t, &["x", "y", "columns", "cell", "title", "cells", "at"], WHAT)?;
    let mut cells = Vec::new();
    if let Some(list) = t.raw_get::<Option<Table>>("cells")? {
        for (n, v) in list.sequence_values::<Value>().enumerate() {
            cells.push(match v? {
                Value::Table(c) => {
                    known_keys(&c, &["icon", "count", "dim"], &format!("{WHAT}: cells[{}]", n + 1))?;
                    GridCell { icon: c.raw_get("icon")?, count: c.raw_get("count")?, dim: c.raw_get::<Option<bool>>("dim")?.unwrap_or(false) }
                }
                Value::Boolean(false) => GridCell::default(),
                _ => return Err(mlua::Error::runtime(format!("{WHAT}: cells[{}] is {{ icon =, count =, dim = }}, or false for an empty slot", n + 1))),
            });
        }
    }
    let columns = match t.raw_get::<Option<i64>>("columns")? {
        Some(n) if n >= 1 => n as usize,
        Some(n) => return Err(mlua::Error::runtime(format!("{WHAT}: columns = {n}, at least 1"))),
        None => cells.len().clamp(1, 8),
    };
    let cell = match t.raw_get::<Option<f32>>("cell")? {
        Some(c) if c >= 8.0 => c,
        Some(c) => return Err(mlua::Error::runtime(format!("{WHAT}: cell = {c}, at least 8 voxels"))),
        None => 18.0,
    };
    let title = t.raw_get::<Option<String>>("title")?.map(|s| look(lua, |h| fill_keys(h.drawn_controls(), &s))).transpose()?;
    Ok(Grid { x: t.raw_get("x")?, y: t.raw_get("y")?, columns, cell, title, at: chosen(t, "at", cells.len(), WHAT)?, cells })
}

/// `music`: the song the game plays of its own choice, and its beats and
/// bars, so the game can go with it (waves on the beat). Its clock is the
/// game's (a step every tick): replays play it the same.
pub struct MusicView;

impl UserData for MusicView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_function_get("song", |lua, _| look(lua, |h| Ok(h.music.song.and_then(|s| h.songs.get(s)).map(|t| t.name.clone()))));
        f.add_field_function_get("beat", |lua, _| look(lua, |h| Ok(h.music.beats(&h.songs).map(|b| b.floor() as i64))));
        f.add_field_function_get("intensity", |lua, _| look(lua, |h| Ok(h.music.intensity)));
        f.add_field_function_set("intensity", |lua, _, v: f32| {
            if !(0.0..=1.0).contains(&v) {
                return Err(mlua::Error::runtime(format!("music.intensity: from 0 (calm) to 1 (all of the song), not {v}")));
            }
            change(lua, "music.intensity", |h| {
                h.music.intensity = v;
                Ok(())
            })
        });
        f.add_field_function_get("bar", |lua, _| {
            look(lua, |h| {
                let meter = h.music.song.and_then(|s| h.songs.get(s)).map_or(4, |t| t.meter) as f64;
                Ok(h.music.beats(&h.songs).map(|b| (b / meter).floor() as i64))
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("music"));
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, String)| -> mlua::Result<Value> {
            Err(mlua::Error::runtime(format!("music has no {key} (music:play, music.song, music.beat, music.bar, music.intensity)")))
        });
        m.add_function("play", |lua, (_, song, when): (AnyUserData, Option<String>, Option<String>)| {
            change(lua, "music:play", |h| {
                let when = match when.as_deref() {
                    None | Some("bar") => super::host::When::Bar,
                    Some("beat") => super::host::When::Beat,
                    Some("now") => super::host::When::Now,
                    Some(other) => return Err(mlua::Error::runtime(format!("music:play: from \"bar\", \"beat\" or \"now\", not \"{other}\""))),
                };
                let song = match &song {
                    None => None,
                    Some(name) => match h.songs.iter().position(|t| &t.name == name) {
                        Some(s) => Some(s),
                        None => {
                            let names: Vec<&str> = h.songs.iter().map(|t| t.name.as_str()).collect();
                            let list = if names.is_empty() { "none: c.audio.music = { \"music/NAME.mml\" }".to_string() } else { names.join(", ") };
                            return Err(mlua::Error::runtime(format!("music:play: no song called '{name}' in c.audio.music (its songs: {list})")));
                        }
                    },
                };
                // Asked for what plays already: it goes on, nothing else asked.
                h.music.next = if song == h.music.song { None } else { Some((song, when)) };
                Ok(())
            })
        });
    }
}

/// `screen`: the picture's effects, from anywhere the game changes (they
/// are part of the game: replays see them).
pub struct ScreenView;

impl UserData for ScreenView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        // The HUD's height in UI voxels (c.ui.height), for icons that burst
        // near its bottom.
        f.add_field_function_get("height", |lua, _| look(lua, |h| Ok(h.hud.height)));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("screen"));
        // What was drawn on `screen` before is drawn on `on_draw`'s frame.
        m.add_meta_function(MetaMethod::Index, |_, (_, key): (AnyUserData, String)| -> mlua::Result<Value> {
            match key.as_str() {
                "text" | "cube" | "model" | "light" => Err(mlua::Error::runtime(format!("screen.{key} is frame.{key} now: function on_draw(frame) ... frame:{key}(...) ... end"))),
                "key" | "device" => Err(mlua::Error::runtime(format!("screen.{key} is controls.{key} now (or the action in braces in frame:text: \"PRESS {{jump}}\")"))),
                other => Err(mlua::Error::runtime(format!("screen has no {other} (screen:shake, screen:flash, screen:burst, screen.height; drawing is on_draw's frame)"))),
            }
        });
        m.add_function("shake", |lua, (_, amount): (AnyUserData, f32)| {
            change(lua, "screen:shake", |h| {
                let sim = h.host.sim_mut();
                sim.shake = (sim.shake + amount.clamp(0.0, 1.0)).min(1.0);
                Ok(())
            })
        });
        m.add_function("burst", |lua, (_, name, x, y): (AnyUserData, String, f32, f32)| {
            change(lua, "screen:burst", |h| h.hud.burst(&name, x, y).map_err(|e| mlua::Error::runtime(format!("screen:burst: {e}"))))
        });
        m.add_function("flash", |lua, (_, strength): (AnyUserData, f32)| {
            change(lua, "screen:flash", |h| {
                let sim = h.host.sim_mut();
                let flash = strength.clamp(0.0, 1.0) * sim.cfg.effects.screen_flash;
                sim.screen_flash = sim.screen_flash.max(flash).min(1.0);
                Ok(())
            })
        });
    }
}

/// `frame` (`on_draw(frame)`): the picture being made; only there, only
/// for this picture.
#[derive(Default)]
pub struct Frame {
    pub cmds: Vec<Cmd>,
    /// The HUD's grid in UI voxels: across (by the screen's shape) and up.
    pub width: f32,
    pub height: f32,
    /// The big letters' font (`c.ui.display_font`), for `text_width`.
    pub font: Option<std::sync::Arc<crate::font::DisplayFont>>,
    /// How far into the tick this picture is (0 to 1), for smooth motion.
    pub alpha: f32,
    /// The parts of the game's figures of parts, by model (a pose's names).
    pub parts: std::sync::Arc<std::collections::BTreeMap<String, Vec<String>>>,
    /// The game's models by name, with their frame counts: what
    /// `frame:model` may name (an unknown file would draw nothing).
    pub known: std::sync::Arc<std::collections::BTreeMap<String, usize>>,
    /// The camera's projection this picture, and whether moving things are
    /// drawn on the grid (`to_screen`).
    pub view_proj: glam::Mat4,
    pub snap: bool,
}

/// A text size: 1 the pixel font, 2 and up the big letters.
fn text_size(size: Option<i32>, what: &str) -> mlua::Result<i32> {
    match size.unwrap_or(1) {
        n @ 1..=4 => Ok(n),
        n => Err(mlua::Error::runtime(format!("{what}: size {n}: from 1 (the small letters) to 4"))),
    }
}

/// An action's key as the player sees it (`controls`).
fn key_of<'a>(controls: &'a Controls, action: &str, what: &str) -> mlua::Result<&'a str> {
    match controls.keys.get(action) {
        Some(label) => Ok(label),
        None => {
            let known: Vec<&str> = controls.keys.keys().map(String::as_str).collect();
            Err(mlua::Error::runtime(format!("{what}: no action '{action}' (c.actions and pause: {})", known.join(", "))))
        }
    }
}

/// The language the game speaks to the player (`world.language`): the
/// one chosen if the game speaks it, else its first, else English.
pub fn language_of(h: &Stage) -> String {
    h.controls.language.as_ref().filter(|l| h.languages.contains(l)).or(h.languages.first()).cloned().unwrap_or_else(|| "en".into())
}

/// `world.language` as `on_draw` sees it: the machine drawing's (each
/// machine its own over the network), the game's in the simulation.
fn drawn_language(h: &Stage) -> String {
    match h.local.language.as_ref().filter(|l| h.languages.contains(l)) {
        Some(own) => own.clone(),
        None => language_of(h),
    }
}

/// `text` with every `{action}` its key, `{{` a brace.
fn fill_keys(controls: &Controls, text: &str) -> mlua::Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('{') {
            out.push('{');
            rest = after;
            continue;
        }
        let Some(end) = rest.find('}') else {
            return Err(mlua::Error::runtime("frame:text: a '{' without its '}' (an action's key is {jump}, a brace {{)"));
        };
        // A key's name in capitals, as on the keys themselves.
        out.push_str(&crate::ui::engine_case(key_of(controls, &rest[..end], "frame:text")?));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// `controls`: what the player plays with, the device and the keys as the
/// player sees them. It comes from their machine, so replays carry it
/// (`Session::set_controls`) and the game may go by it anywhere. Seat 0 is
/// the global `controls`: seat 1's, in `on_draw` the machine drawing's
/// first player's.
pub struct ControlsView(pub usize);

impl ControlsView {
    fn of<'a>(&self, lua: &Lua, h: &'a Stage) -> &'a Controls {
        match self.0 {
            0 if looking(lua) => h.drawn_controls(),
            seat => h.controls_of(seat),
        }
    }
}

impl Stage {
    /// What seat `seat`'s player plays with (seat 1's: `controls`).
    fn controls_of(&self, seat: usize) -> &Controls {
        match seat {
            0 | 1 => &self.controls,
            _ => &self.seats[seat - 1].controls,
        }
    }

    /// The players at the machine drawing (`frame.players`): its seats
    /// taken, every taken one at one machine.
    pub fn drawn_seats(&self) -> Vec<usize> {
        (1..=self.seats.len()).filter(|&n| self.seats[n - 1].taken && self.local.has(n)).collect()
    }

    /// What the first of them plays with (seat 1's where none sits at the
    /// machine drawing): the keys in braces in `on_draw`'s texts.
    fn drawn_controls(&self) -> &Controls {
        self.controls_of(self.drawn_seats().first().copied().unwrap_or(1))
    }
}

impl UserData for ControlsView {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_method_get("device", |lua, c| look(lua, |h| Ok(c.of(lua, h).device.map(|d| d.name()))));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("controls"));
        m.add_method("key", |lua, c, action: String| look(lua, |h| key_of(c.of(lua, h), &action, "controls:key").map(str::to_owned)));
    }
}

/// `frame:model`'s rotation: degrees round, nose up, rolled (each 0
/// unless given), any other key a mistake (`turn`, its name before, says
/// the new one).
fn turning(t: &Table, what: &str) -> mlua::Result<[f32; 3]> {
    turning_with(t, what, false).map(|(r, _)| r)
}

/// A pose entry: `turning`'s rotation, and with `offset` allowed the
/// part's offset in voxels, `offset = { x, y, z }` (each 0 unless given).
fn turning_with(t: &Table, what: &str, offset: bool) -> mlua::Result<([f32; 3], [f32; 3])> {
    const KEYS: [&str; 3] = ["rotation", "pitch", "roll"];
    for pair in t.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let name = match &key {
            Value::String(k) => k.to_string_lossy(),
            other => format!("{other:?}"),
        };
        if name == "turn" {
            return Err(mlua::Error::runtime(format!("{what}: 'turn' is called 'rotation' now: {{ rotation = 45 }}")));
        }
        if !KEYS.contains(&name.as_str()) && !(offset && name == "offset") {
            let words = if offset { "a pose entry has rotation, pitch and roll in degrees and an offset in voxels ({ roll = 20, offset = { 0, 0.5, 0 } })" } else { "a rotation has rotation, pitch and roll in degrees ({ roll = 20 })" };
            return Err(mlua::Error::runtime(format!("{what}: {words}, not '{name}'")));
        }
    }
    let moved = match t.raw_get::<Value>("offset")? {
        Value::Nil => [0.0; 3],
        Value::Table(o) => {
            let n = |k: i64| -> mlua::Result<f32> {
                match o.raw_get::<Value>(k)? {
                    Value::Nil => Ok(0.0),
                    Value::Integer(i) => Ok(i as f32),
                    Value::Number(x) if x.is_finite() => Ok(x as f32),
                    _ => Err(mlua::Error::runtime(format!("{what}: an offset is three numbers of voxels, {{ x, y, z }}"))),
                }
            };
            if o.raw_len() > 3 {
                return Err(mlua::Error::runtime(format!("{what}: an offset is three numbers of voxels, {{ x, y, z }}")));
            }
            [n(1)?, n(2)?, n(3)?]
        }
        _ => return Err(mlua::Error::runtime(format!("{what}: an offset is three numbers of voxels, {{ x, y, z }}"))),
    };
    let get = |key: &str| -> mlua::Result<f32> {
        match t.raw_get::<Value>(key)? {
            Value::Nil => Ok(0.0),
            Value::Integer(i) => Ok(i as f32),
            Value::Number(n) if n.is_finite() => Ok(n as f32),
            _ => Err(mlua::Error::runtime(format!("{what}: the rotation's {key} is a number of degrees"))),
        }
    };
    Ok(([get("rotation")?, get("pitch")?, get("roll")?], moved))
}

/// A pose of the figure of parts `file` (`frame:model`'s, a piece's
/// `thing.state.parts`): each part by its name, a rotation about its joint
/// (`turning`) and an offset in voxels, in the parts' names' order. `parts` are the figure's parts
/// (none: no figure); `what` names it in a mistake.
pub fn pose_of(pose: &Table, file: &str, parts: Option<&Vec<String>>, what: &str) -> mlua::Result<Vec<crate::session::PoseEntry>> {
    let mut posed = Vec::new();
    for pair in pose.pairs::<Value, Value>() {
        let (part, t) = pair?;
        let (Value::String(part), Value::Table(t)) = (part, t) else {
            return Err(mlua::Error::runtime(format!("{what}: a pose is parts by name, each a rotation: {{ head = {{ rotation = 20 }} }}")));
        };
        let part = part.to_str()?.to_string();
        match parts {
            None => return Err(mlua::Error::runtime(format!("{what}: {file} is no figure of parts (its objects named in the file) to pose"))),
            Some(names) if !names.contains(&part) => return Err(mlua::Error::runtime(format!("{what}: {file} has no part '{part}' (its parts: {})", names.join(", ")))),
            Some(_) => posed.push((part, t)),
        }
    }
    // The first mistake the same every run (a table's own walk is not).
    posed.sort_by(|a, b| a.0.cmp(&b.0));
    posed
        .into_iter()
        .map(|(part, t)| {
            let (turn, offset) = turning_with(&t, &format!("{what}, part '{part}'"), true)?;
            Ok((part, turn, offset))
        })
        .collect()
}

/// A piece's pose as its state holds it (`thing.state.parts`, `v`): none
/// for nil; `model` its model (none: a light alone), `name` what a mistake
/// calls it (its name, or "thing N").
pub fn piece_pose(v: &Value, model: Option<&str>, parts: &std::collections::BTreeMap<String, Vec<String>>, name: &str) -> mlua::Result<Vec<crate::session::PoseEntry>> {
    let what = format!("thing.state.parts of {name}");
    match v {
        Value::Nil => Ok(Vec::new()),
        Value::Table(t) => match model {
            Some(file) => pose_of(t, file, parts.get(file), &what),
            None if t.is_empty() => Ok(Vec::new()),
            None => Err(mlua::Error::runtime(format!("{what}: it has no model to pose (a light alone)"))),
        },
        other => Err(mlua::Error::runtime(format!("{what}: a pose is parts by name, each a rotation ({{ leaf = {{ rotation = 90 }} }}), or nil for at rest, not a {}", other.type_name()))),
    }
}

/// The empty part of `frame:bar`.
const BAR_BACK: [u8; 4] = [64, 46, 40, 255];

fn color(v: Option<Table>, default: [u8; 4]) -> mlua::Result<[u8; 4]> {
    let Some(t) = v else { return Ok(default) };
    let c = |i: i64| -> mlua::Result<u8> { Ok(t.raw_get::<Option<u8>>(i)?.unwrap_or(255)) };
    Ok([c(1)?, c(2)?, c(3)?, c(4)?])
}

/// `frame:daylight{ sun =, sun_color =, ambient =, shadow =, sky = }`: what
/// it sets of this picture's daylight, the rest `c.light`'s.
fn daylight(t: &Table) -> mlua::Result<crate::session::Daylight> {
    let fail = |what: String| mlua::Error::runtime(format!("frame:daylight: {what}"));
    let mut day = crate::session::Daylight::default();
    let rgb = |v: Value, key: &str| -> mlua::Result<[f32; 3]> {
        let Value::Table(c) = v else { return Err(fail(format!("{key} is a colour, {{ r, g, b }} from 0 to 255"))) };
        let [r, g, b, _] = color(Some(c), [255; 4])?;
        Ok([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
    };
    let unit = |v: Value, key: &str| -> mlua::Result<f32> {
        match v {
            Value::Integer(n) if (0..=1).contains(&n) => Ok(n as f32),
            Value::Number(n) if (0.0..=1.0).contains(&n) => Ok(n as f32),
            _ => Err(fail(format!("{key} is a number from 0 to 1"))),
        }
    };
    for pair in t.pairs::<Value, Value>() {
        let (key, v) = pair?;
        let key = match &key {
            Value::String(s) => s.to_str()?.to_string(),
            _ => return Err(fail("its keys are sun, sun_color, ambient, shadow and sky".into())),
        };
        match key.as_str() {
            "sun" => {
                let Value::Table(w) = v else { return Err(fail("sun is the way its light travels, { x, y, z } with z below 0 (towards the ground)".into())) };
                let at = |i: i64, name: &str| -> mlua::Result<f32> { Ok(w.raw_get::<Option<f32>>(i)?.or(w.raw_get::<Option<f32>>(name)?).unwrap_or(0.0)) };
                let way = Vec3::new(at(1, "x")?, at(2, "y")?, at(3, "z")?);
                if way.z >= 0.0 || way.length() < 1e-4 {
                    return Err(fail("sun is the way its light travels, { x, y, z } with z below 0 (towards the ground)".into()));
                }
                day.sun = Some(way);
            }
            "sun_color" => day.sun_color = Some(rgb(v, "sun_color")?),
            "sky" => day.sky = Some(rgb(v, "sky")?),
            "ambient" => day.ambient = Some(unit(v, "ambient")?),
            "shadow" => day.shadow = Some(unit(v, "shadow")?),
            other => return Err(fail(format!("no '{other}': it sets sun, sun_color, ambient, shadow and sky"))),
        }
    }
    Ok(day)
}

impl UserData for Frame {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_method_get("width", |_, o| Ok(o.width));
        // The players at the machine this picture is for (every taken seat
        // at one machine), in seat order.
        f.add_field_function_get("players", |lua, _| {
            let seats = look(lua, |h| Ok(h.drawn_seats()))?;
            let list = lua.create_table()?;
            for (n, seat) in seats.into_iter().enumerate() {
                list.raw_set(n + 1, handle(lua, seat_handle(seat))?)?;
            }
            Ok(list)
        });
        f.add_field_method_get("height", |_, o| Ok(o.height));
        f.add_field_method_get("alpha", |_, o| Ok(o.alpha));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("frame"));
        m.add_method_mut("model", |_, o, (file, thing, frame, rotation, pose): (String, Value, Option<usize>, Option<Table>, Option<Table>)| {
            let thing = thing_id(&thing, "frame:model")?;
            if !o.known.contains_key(&file) {
                return Err(mlua::Error::runtime(format!("frame:model: no model called '{file}' (the game knows a kind's model in c.things, and the files listed in c.models: list it there)")));
            }
            let turn = match rotation {
                Some(t) => turning(&t, "frame:model")?,
                None => [0.0; 3],
            };
            // A figure of parts posed: each part by its name, a rotation.
            let posed = match pose {
                Some(pose) => pose_of(&pose, &file, o.parts.get(&file), "frame:model")?,
                None => Vec::new(),
            };
            o.cmds.push(Cmd::Model { file, thing, frame, turn, pose: posed });
            Ok(())
        });
        m.add_method_mut("cube", |_, o, (x, y, z, rgba, glow): (f32, f32, f32, Option<Table>, Option<f32>)| {
            o.cmds.push(Cmd::Cube { at: Vec3::new(x, y, z), color: color(rgba, [255, 255, 255, 255])?, glow: glow.unwrap_or(0.0) });
            Ok(())
        });
        m.add_method_mut("light", |lua, o, (at, radius, rgb): (Value, f32, Option<Table>)| {
            let at = to_point(lua, &at, "frame:light")?;
            let c = color(rgb, [255, 255, 255, 255])?;
            o.cmds.push(Cmd::Light { at, radius, color: [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0] });
            Ok(())
        });
        m.add_method_mut("menu", |lua, o, t: Table| {
            o.cmds.push(Cmd::Menu(menu(lua, &t)?));
            Ok(())
        });
        m.add_method_mut("grid", |lua, o, t: Table| {
            o.cmds.push(Cmd::Grid(grid(lua, &t)?));
            Ok(())
        });
        m.add_method_mut("daylight", |_, o, t: Table| {
            o.cmds.push(Cmd::Daylight(daylight(&t)?));
            Ok(())
        });
        m.add_method_mut("text", |lua, o, (text, x, y, rgba, size): (String, f32, f32, Option<Table>, Option<i32>)| {
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            let size = text_size(size, "frame:text")?;
            o.cmds.push(Cmd::Text { text, x, y, color: color(rgba, crate::ui::TEXT)?, size });
            Ok(())
        });
        // Where a point of the level shows, in the HUD's measure: a thing
        // where its feet are drawn this picture, as its model is.
        m.add_method("to_screen", |lua, o, (at, up): (Value, Option<f32>)| {
            let at = match &at {
                Value::UserData(ud) if ud.is::<ThingRef>() => {
                    let id = ud.borrow::<ThingRef>()?.0;
                    let feet = look(lua, |h| {
                        let b = &h.thing(id)?.body;
                        Ok(crate::session::between(b.prev, b.pos, o.alpha))
                    })?;
                    if o.snap { feet.floor() } else { feet }
                }
                _ => to_point(lua, &at, "frame:to_screen")?,
            };
            let clip = o.view_proj * (at + Vec3::Z * up.unwrap_or(0.0)).extend(1.0);
            if clip.w <= 1e-6 {
                return Ok((None, None));
            }
            let (x, y) = (clip.x / clip.w, clip.y / clip.w);
            Ok((Some((x + 1.0) * 0.5 * o.width), Some((1.0 - y) * 0.5 * o.height)))
        });
        m.add_method("text_width", |lua, o, (text, size): (String, Option<i32>)| {
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            Ok(crate::ui::line_width(&text, text_size(size, "frame:text_width")?, o.font.as_deref()))
        });
        m.add_method_mut("box", |_, o, (x, y, width, height, rgba): (f32, f32, f32, f32, Option<Table>)| {
            o.cmds.push(Cmd::Box { x, y, width, height, color: color(rgba, crate::ui::TEXT)? });
            Ok(())
        });
        m.add_method_mut("bar", |_, o, (x, y, width, height, fraction, rgba, back): (f32, f32, f32, f32, f32, Option<Table>, Option<Table>)| {
            // The filled part, then the rest beside it (a voxel drawn first
            // keeps its cell).
            let filled = (fraction.clamp(0.0, 1.0) * width).ceil().min(width);
            if filled > 0.0 {
                o.cmds.push(Cmd::Box { x, y, width: filled, height, color: color(rgba, crate::ui::RED)? });
            }
            if filled < width {
                o.cmds.push(Cmd::Box { x: x + filled, y, width: width - filled, height, color: color(back, BAR_BACK)? });
            }
            Ok(())
        });
        m.add_method_mut("icon", |lua, o, (name, x, y): (String, f32, f32)| {
            look(lua, |h| match h.hud.icons.contains_key(&name) {
                true => Ok(()),
                false => {
                    let names: Vec<&str> = h.hud.icons.keys().map(String::as_str).collect();
                    Err(mlua::Error::runtime(format!("frame:icon: no icon called '{name}' (c.icons: {})", names.join(", "))))
                }
            })?;
            o.cmds.push(Cmd::Icon { name, x, y });
            Ok(())
        });
        m.add_method_mut("drop_in", |lua, o, (text, x, y, ticks, rgba, size): (String, f32, f32, f32, Option<Table>, Option<i32>)| {
            let text = look(lua, |h| fill_keys(h.drawn_controls(), &text))?;
            let size = text_size(size, "frame:drop_in")?;
            o.cmds.push(Cmd::Drop { text, x, y, ticks, color: color(rgba, crate::ui::TEXT)?, size });
            Ok(())
        });
    }
}

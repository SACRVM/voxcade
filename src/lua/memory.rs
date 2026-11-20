//! Levels that remember (`docs/game-model.md`, "Levels that remember"): a
//! level's `location` and `persistent`, a kind's `persistent`.
//!
//! As a level is left, what it keeps is taken as a record (`Record`: the
//! things its file set down, by name, and its voxels changed); as it is
//! entered again, it is built from its file as ever, the record laid over
//! it, then `on_level_start(how)` runs. While the player stays among the
//! levels of one location each is as it was left and they share one
//! `level.state`; leaving the location forgets their records but for what
//! is persistent (a persistent level whole, the things of persistent kinds
//! anywhere). A record is names and plain data, one form in memory, in a
//! save, in a scenario's `as_left` and in `inspect` (`Record::value`).

use std::collections::BTreeMap;

use glam::{IVec2, IVec3, Vec2};

use super::*;
use crate::sim::things::Walk;
use value::Key;

/// What a level kept of how it was left.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// The tick it was left at (`world.tick`): `level.away` counts from it.
    pub left: u64,
    /// The things its file sets down, by name, in the order of the names.
    pub things: Vec<(String, Left)>,
    /// Its voxels and colours that differ from its file's (a level that
    /// keeps its voxels), counted in a world of `size`.
    pub voxels: Vec<(IVec3, u8)>,
    pub palette: Vec<(usize, [u8; 4])>,
    pub size: IVec3,
    /// Its hash, taken once (`seal`): the state hash takes it every time.
    hash: u64,
}

/// How one thing was left: taken out, or where it stood (where its feet
/// were, the way it faced, its model's quarter turn, on the ground or not),
/// whether its ready-made rules were on, its `thing.state` as the script
/// sees it (the engine's keys among the game's own). What is None is as
/// the level's file has it (a scenario sets only what it names).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Left {
    pub removed: bool,
    pub at: Option<Vec3>,
    pub facing: Option<Vec2>,
    pub look: Option<IVec2>,
    pub on_ground: Option<bool>,
    pub enabled: Option<bool>,
    pub state: Option<Value>,
}

/// What the level entered next is given: the record laid over it, and the
/// `level.state` it goes on with (a change among a location's levels, a
/// restart there).
#[derive(Default)]
pub(super) struct Laying {
    pub record: Option<Arc<Record>>,
    pub state: Option<Value>,
}

fn key(name: &str) -> Key {
    Key::Str(name.as_bytes().to_vec())
}

fn float(f: f32) -> Value {
    Value::Float((f as f64).to_bits())
}

/// A table of named entries, in the order data keeps keys.
fn table(mut entries: Vec<(Key, Value)>) -> Value {
    entries.sort_by(|a, b| a.0.before(&b.0));
    Value::Table(entries)
}

/// `v` without the things of a level in it (a player is its seat and
/// stays): as a save writes it (`data::write`), so a record, a carried
/// `level.state` and a save are one form. None for such a thing itself.
pub fn without_things(v: &Value) -> Option<Value> {
    match v {
        Value::Thing(id) if host::handle_seat(*id).is_none() => None,
        Value::Table(entries) => Some(Value::Table(entries.iter().filter_map(|(k, v)| Some((k.clone(), without_things(v)?))).collect())),
        other => Some(other.clone()),
    }
}

impl Record {
    /// A record of these things only (a scenario's `as_left`), left at the
    /// run's start.
    pub fn of_things(things: Vec<(String, Left)>) -> Self {
        Record { left: 0, things, voxels: Vec::new(), palette: Vec::new(), size: IVec3::ZERO, hash: 0 }
    }

    /// Its hash taken, shared from now on.
    pub fn seal(mut self) -> Arc<Record> {
        let mut s = Stable::default();
        value::hash(&mut s, &self.value());
        self.hash = s.finish();
        Arc::new(self)
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Whether it keeps nothing.
    pub fn is_empty(&self) -> bool {
        self.things.is_empty() && self.voxels.is_empty() && self.palette.is_empty()
    }

    /// As plain data: `{ left = TICK, things = { NAME = { ... } }, voxels =
    /// "...", size = { x, y, z }, palette = { [i] = "#rrggbbaa" } }`, the
    /// voxels' three only where it keeps some.
    pub fn value(&self) -> Value {
        let mut out = vec![(key("left"), Value::Int(self.left as i64))];
        if !self.things.is_empty() {
            out.push((key("things"), table(self.things.iter().map(|(n, l)| (key(n), l.value())).collect())));
        }
        if !self.voxels.is_empty() || !self.palette.is_empty() {
            out.push((key("voxels"), Value::Str(save::write_voxels(&self.voxels, self.size).into_bytes())));
            out.push((key("size"), Value::Table(self.size.to_array().iter().enumerate().map(|(i, n)| (Key::Int(i as i64 + 1), Value::Int(*n as i64))).collect())));
            let palette = self.palette.iter().map(|(i, [r, g, b, a])| (Key::Int(*i as i64), Value::Str(format!("#{r:02x}{g:02x}{b:02x}{a:02x}").into_bytes()))).collect();
            out.push((key("palette"), Value::Table(palette)));
        }
        table(out)
    }

    /// A record from its plain data (`value`), every mistake named at
    /// `what` (the save's record of a level).
    pub fn from_value(v: &Value, what: &str) -> Result<Record> {
        let entries = fields(v, what, &["left", "things", "voxels", "size", "palette"])?;
        let left = match entries.get("left") {
            None => 0,
            Some(v) => whole(v, &format!("{what}.left"))?.max(0) as u64,
        };
        let things = match entries.get("things") {
            None => Vec::new(),
            Some(v) => things_of(v, &format!("{what}.things"))?,
        };
        let size = match entries.get("size") {
            None => IVec3::ZERO,
            Some(v) => {
                let n = numbers(v, &format!("{what}.size"), 3)?;
                IVec3::new(n[0] as i32, n[1] as i32, n[2] as i32)
            }
        };
        let voxels = match entries.get("voxels") {
            None => Vec::new(),
            Some(Value::Str(s)) if size.cmpgt(IVec3::ZERO).all() => save::read_voxels(&String::from_utf8_lossy(s), size).with_context(|| format!("{what}.voxels"))?,
            Some(_) => anyhow::bail!("{what}.voxels: the voxels changed, as text, with the level's size beside them"),
        };
        let mut palette = Vec::new();
        if let Some(v) = entries.get("palette") {
            let Value::Table(list) = v else { anyhow::bail!("{what}.palette: colours by their number, \"#rrggbbaa\"") };
            for (k, c) in list {
                let i = match k {
                    Key::Int(i) if (1..256).contains(i) => *i as usize,
                    _ => anyhow::bail!("{what}.palette: a colour's number is 1 to 255"),
                };
                let text = match c {
                    Value::Str(s) => String::from_utf8_lossy(s).to_string(),
                    _ => anyhow::bail!("{what}.palette[{i}]: a colour, \"#rrggbbaa\""),
                };
                let hex = text.strip_prefix('#').filter(|h| h.len() == 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
                let Some(hex) = hex else { anyhow::bail!("{what}.palette[{i}] = \"{text}\": a colour is \"#rrggbbaa\"") };
                palette.push((i, [0, 2, 4, 6].map(|k| u8::from_str_radix(&hex[k..k + 2], 16).expect("hex digits"))));
            }
        }
        Ok(Record { left, things, voxels, palette, size, hash: 0 })
    }
}

/// A table's named fields, each one of `known` (a mistake names the one
/// meant).
fn fields<'v>(v: &'v Value, what: &str, known: &[&str]) -> Result<BTreeMap<String, &'v Value>> {
    let Value::Table(entries) = v else { anyhow::bail!("{what}: a table, {{ {} = ... }}", known[0]) };
    let mut out = BTreeMap::new();
    for (k, v) in entries {
        let Key::Str(name) = k else { anyhow::bail!("{what}: its keys are names ({})", known.join(", ")) };
        let name = String::from_utf8_lossy(name).to_string();
        if !known.contains(&name.as_str()) {
            let near = crate::audio::words::near(&name, known.iter().copied()).map(|n| format!("did you mean {n}?")).unwrap_or_else(|| format!("it has {}", known.join(", ")));
            anyhow::bail!("{what}: no {name} ({near})");
        }
        out.insert(name, v);
    }
    Ok(out)
}

fn number(v: &Value, what: &str) -> Result<f64> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) if f64::from_bits(*f).is_finite() => Ok(f64::from_bits(*f)),
        _ => anyhow::bail!("{what}: a number"),
    }
}

fn whole(v: &Value, what: &str) -> Result<i64> {
    match v {
        Value::Int(i) => Ok(*i),
        Value::Float(f) if f64::from_bits(*f).fract() == 0.0 && f64::from_bits(*f).abs() < 1e15 => Ok(f64::from_bits(*f) as i64),
        _ => anyhow::bail!("{what}: a whole number"),
    }
}

/// `n` numbers, as a list `{ 1, 2, 3 }` or by name `{ x =, y =, z = }`.
fn numbers(v: &Value, what: &str, n: usize) -> Result<Vec<f64>> {
    let names = ["x", "y", "z"];
    let Value::Table(entries) = v else { anyhow::bail!("{what}: {n} numbers, {{ {} }}", names[..n].join(", ")) };
    (0..n)
        .map(|i| {
            let found = entries.iter().find(|(k, _)| *k == Key::Int(i as i64 + 1) || *k == key(names[i])).map(|(_, v)| v);
            found.map_or_else(|| anyhow::bail!("{what}: {n} numbers, {{ {} }}", names[..n].join(", ")), |v| number(v, what))
        })
        .collect()
}

/// A level's things as a record keeps them, by name (`Left::value`).
pub fn things_of(v: &Value, what: &str) -> Result<Vec<(String, Left)>> {
    let Value::Table(entries) = v else { anyhow::bail!("{what}: the things by their names, {{ door = {{ ... }} }}") };
    let mut out = Vec::new();
    for (k, v) in entries {
        let Key::Str(name) = k else { anyhow::bail!("{what}: things by their names (thing.name), not by numbers") };
        let name = String::from_utf8_lossy(name).to_string();
        out.push((name.clone(), Left::from_value(v, &format!("{what}.{name}"))?));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

impl Left {
    /// As plain data: `{ removed = true }`, or `{ at = { x, y, z }, facing
    /// = { x, y }, look = { x, y }, on_ground = true, enabled = false,
    /// state = { ... } }`, each only where known.
    pub fn value(&self) -> Value {
        let list = |items: Vec<Value>| Value::Table(items.into_iter().enumerate().map(|(i, v)| (Key::Int(i as i64 + 1), v)).collect());
        let mut out = Vec::new();
        if self.removed {
            out.push((key("removed"), Value::Bool(true)));
        }
        if let Some(at) = self.at {
            out.push((key("at"), list(at.to_array().into_iter().map(float).collect())));
        }
        if let Some(f) = self.facing {
            out.push((key("facing"), list(f.to_array().into_iter().map(float).collect())));
        }
        if let Some(l) = self.look {
            out.push((key("look"), list(l.to_array().into_iter().map(|n| Value::Int(n as i64)).collect())));
        }
        if let Some(g) = self.on_ground {
            out.push((key("on_ground"), Value::Bool(g)));
        }
        if let Some(e) = self.enabled {
            out.push((key("enabled"), Value::Bool(e)));
        }
        if let Some(s) = &self.state {
            out.push((key("state"), s.clone()));
        }
        table(out)
    }

    pub fn from_value(v: &Value, what: &str) -> Result<Left> {
        let f = fields(v, what, &["removed", "at", "facing", "look", "on_ground", "enabled", "state"])?;
        let flag = |name: &str| -> Result<Option<bool>> {
            match f.get(name) {
                None => Ok(None),
                Some(Value::Bool(b)) => Ok(Some(*b)),
                Some(_) => anyhow::bail!("{what}.{name}: true or false"),
            }
        };
        let at = f.get("at").map(|v| numbers(v, &format!("{what}.at"), 3)).transpose()?.map(|n| Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32));
        let facing = f.get("facing").map(|v| numbers(v, &format!("{what}.facing"), 2)).transpose()?.map(|n| Vec2::new(n[0] as f32, n[1] as f32));
        if facing.is_some_and(|w| w.length_squared() == 0.0) {
            anyhow::bail!("{what}.facing: a way, {{ x, y }}, not none");
        }
        let look = f.get("look").map(|v| numbers(v, &format!("{what}.look"), 2)).transpose()?.map(|n| IVec2::new(n[0].signum() as i32, n[1].signum() as i32));
        let state = match f.get("state") {
            None => None,
            Some(v @ Value::Table(_)) => without_things(v),
            Some(_) => anyhow::bail!("{what}.state: a table, as thing.state"),
        };
        Ok(Left { removed: flag("removed")?.unwrap_or(false), at, facing, look, on_ground: flag("on_ground")?, enabled: flag("enabled")?, state })
    }
}

impl<K: Kit> LuaGame<K> {
    /// Whether level `index` keeps everything it sets down and its voxels
    /// as it is left: it belongs to a location, or it is persistent.
    pub(super) fn keeps_whole(&self, index: usize) -> bool {
        let level = &self.stage.levels.list[index];
        level.location.is_some() || level.persistent
    }

    /// What the level being played keeps as it stands now (`whole`: every
    /// thing its file set down and its voxels; else the things of
    /// persistent kinds), left at this tick; None where that is nothing.
    pub(super) fn take_record(&self, whole: bool) -> Result<Option<Record>> {
        let stage = &self.stage;
        let host = &stage.host;
        let data = self.registry("voxcade.data")?;
        let mut things = Vec::new();
        for (id, name, kind) in &stage.here.filed {
            if !whole && !stage.kinds.iter().any(|k| k.name == *kind && k.persistent) {
                continue;
            }
            let gone = stage.here.dead.contains(id) || stage.here.gone.contains(id);
            let Some(t) = host.get(Id::from_raw(*id)).filter(|_| !gone) else {
                things.push((name.clone(), Left { removed: true, ..Left::default() }));
                continue;
            };
            let b = t.body;
            let mut state: Vec<(Key, Value)> = Vec::new();
            let own: mlua::Value = data.raw_get(*id)?;
            if let Some(Value::Table(entries)) = value::capture(&own, &format!("thing {id}'s state"), &mut value::Seen::default(), true)?.and_then(|v| without_things(&v)) {
                state = entries;
            }
            // The engine's keys, as `thing.state` shows them.
            let mut engine: Vec<(Key, Value)> = Vec::new();
            if stage.kinds.get(t.kind).is_some_and(|k| k.has_hp) {
                engine.push((key("hp"), Value::Int(t.hp as i64)));
            }
            if let Some(b) = stage.here.lights.get(id) {
                engine.push((key(world::LIGHT), float(*b)));
            }
            for k in host.thing_keys(Id::from_raw(*id)) {
                match host.thing_value(Id::from_raw(*id), k) {
                    Some(host::Kept::Text(s)) => engine.push((key(k), Value::Str(s.into_bytes()))),
                    Some(host::Kept::Bool(on)) => engine.push((key(k), Value::Bool(on))),
                    None => {}
                }
            }
            state.retain(|(k, _)| !engine.iter().any(|(e, _)| e == k));
            state.extend(engine);
            let state = (!state.is_empty()).then(|| table(state));
            let off = host.rules_off(Id::from_raw(*id));
            let left = Left { removed: false, at: Some(b.pos), facing: Some(b.facing.truncate()), look: Some(b.look), on_ground: Some(b.on_ground), enabled: off.then_some(false), state };
            things.push((name.clone(), left));
        }
        things.sort_by(|a, b| a.0.cmp(&b.0));
        let (mut voxels, mut palette, mut size) = (Vec::new(), Vec::new(), IVec3::ZERO);
        if whole {
            let level = &stage.levels.list[stage.level];
            let world = &host.sim().world;
            if world.size == level.world.size {
                voxels = world.changed_from(&level.world);
                palette = (1..256).filter(|&i| world.palette[i] != level.world.palette[i]).map(|i| (i, world.palette[i])).collect();
                size = world.size;
            }
        }
        let record = Record { left: host.sim().tick, things, voxels, palette, size, hash: 0 };
        Ok((!record.is_empty()).then_some(record))
    }

    /// What of `record`, level `index`'s, lasts the whole run: all of it
    /// for a persistent level, else the things of persistent kinds (its
    /// file read as it is now); None where that is nothing.
    pub(super) fn for_the_run(&self, index: usize, record: &Record) -> Option<Record> {
        let level = &self.stage.levels.list[index];
        if level.persistent {
            return Some(record.clone());
        }
        let kinds = &self.stage.kinds;
        let piece = kinds.iter().position(|k| k.name == crate::grid::PIECE).unwrap_or(usize::MAX);
        let by_name = level.kinds_by_name(|k| kinds.get(k).map_or(("", false), |k| (k.name.as_str(), k.player)), piece);
        let things: Vec<(String, Left)> = record.things.iter().filter(|(n, _)| by_name.get(n).and_then(|k| kinds.get(*k)).is_some_and(|k| k.persistent)).cloned().collect();
        (!things.is_empty()).then(|| Record { left: record.left, things, voxels: Vec::new(), palette: Vec::new(), size: IVec3::ZERO, hash: 0 })
    }

    /// The level being played is left for level `to` (`world:go_to`): what
    /// it keeps is taken; leaving its location (or a level of none), the
    /// records of that location's levels are cut to what lasts the run;
    /// `fresh`: `to`'s forgotten. What `to` is given next (`laying`): its
    /// record, and `level.state` where both lie in one location.
    pub(super) fn leave_for(&mut self, to: usize, fresh: bool) -> Result<()> {
        let levels = Arc::clone(&self.stage.levels);
        let from = self.stage.level;
        let (here, there) = (&levels.list[from], &levels.list[to]);
        let same = here.location.is_some() && here.location == there.location;
        let state = if same { self.level_state()? } else { None };
        let taken = self.take_record(self.keeps_whole(from))?;
        let mut records = (*self.stage.records).clone();
        match taken {
            Some(r) => records.insert(here.name.clone(), r.seal()),
            None => records.remove(&here.name),
        };
        if !same {
            let ended: Vec<String> = records.keys().filter(|n| **n == here.name || here.location.is_some() && levels.find(n).is_some_and(|i| levels.list[i].location == here.location)).cloned().collect();
            for name in ended {
                let cut = levels.find(&name).and_then(|i| self.for_the_run(i, &records[&name]));
                match cut {
                    Some(r) => records.insert(name, r.seal()),
                    None => records.remove(&name),
                };
            }
        }
        if fresh {
            records.remove(&there.name);
        }
        let record = records.get(&there.name).cloned();
        self.stage.records = Arc::new(records);
        self.laying = Some(Laying { record, state });
        Ok(())
    }

    /// The level being played is entered again (`world:restart`, a level
    /// file read anew): as it was entered this visit, what it kept and its
    /// `level.state` then; `fresh`: from its file, what it kept forgotten.
    pub(super) fn again(&mut self, fresh: bool) {
        let name = self.stage.levels.list[self.stage.level].name.clone();
        if fresh && self.stage.records.contains_key(&name) {
            let mut records = (*self.stage.records).clone();
            records.remove(&name);
            self.stage.records = Arc::new(records);
        }
        let record = self.stage.records.get(&name).cloned();
        self.laying = Some(Laying { record, state: self.stage.entered.as_deref().cloned() });
    }

    /// `level.state` as plain data, its things left out (a change among a
    /// location's levels carries it so); None where it is empty.
    pub(super) fn level_state(&self) -> Result<Option<Value>> {
        let v = value::capture(&mlua::Value::Table(self.registry("voxcade.level")?), "level.state", &mut value::Seen::default(), true)?;
        Ok(v.and_then(|v| without_things(&v)))
    }

    /// Lays `record` over the level just entered (its things named, its
    /// `Here` fresh): each thing by its name taken out, stood where it was
    /// left, its rules' switch, its state (the engine's keys where the
    /// engine keeps them); a name its file no longer sets down is dropped
    /// with a note. Its voxels were laid as its world was made (`enter`).
    pub(super) fn lay(&mut self, record: &Record) -> Result<()> {
        let level = self.stage.levels.list[self.stage.level].name.clone();
        let kinds = Arc::clone(&self.stage.kinds);
        let by_name: BTreeMap<String, (u32, usize)> = self.stage.here.filed.iter().map(|(id, n, k)| (n.clone(), (*id, kinds.iter().position(|kind| kind.name == *k).unwrap_or(usize::MAX)))).collect();
        let data = self.registry("voxcade.data")?;
        let lua = self.sandbox.lua.clone();
        for (name, left) in &record.things {
            let Some(&(id, kind)) = by_name.get(name) else {
                self.note(&format!("level {level} kept a thing called '{name}' that its file no longer sets down: dropped"));
                continue;
            };
            let raw = Id::from_raw(id);
            if left.removed {
                if let Some(i) = self.stage.here.pieces.get(&id).filter(|p| p.solid).map(|p| p.index) {
                    self.stage.host.sim_mut().world.lift_piece(i);
                }
                self.stage.here.lights.remove(&id);
                self.stage.host.remove(raw);
                continue;
            }
            let scripted = self.stage.kinds.get(kind).is_none_or(|k| k.scripted);
            if let Some((body, off)) = self.stage.host.thing_body(raw) {
                let at = left.at.unwrap_or(body.pos);
                let on_ground = left.on_ground.unwrap_or(left.at.is_none() && body.on_ground);
                let (facing, look) = (body.facing, body.look);
                body.stand_still(at, facing, look, on_ground);
                if let Some(way) = left.facing {
                    body.face(way.extend(0.0));
                }
                if let Some(look) = left.look {
                    body.look = look;
                }
                if left.enabled == Some(false) && !scripted {
                    *off = Some(Walk::default());
                }
            }
            if left.enabled == Some(false) && scripted {
                self.note(&format!("level {level} kept '{name}' with its rules off, but its kind has none: it is moved by the script anyway"));
            }
            let Some(Value::Table(entries)) = &left.state else { continue };
            let has_hp = self.stage.kinds.get(kind).is_some_and(|k| k.has_hp);
            let own = world::table_at(&lua, &data, id)?;
            for (k, v) in entries {
                let name_of = match k {
                    Key::Str(s) => Some(String::from_utf8_lossy(s).to_string()),
                    _ => None,
                };
                let engine = name_of.as_deref().filter(|n| (*n == "hp" && has_hp) || (*n == world::LIGHT && self.stage.here.lights.contains_key(&id)) || self.stage.host.thing_keys(raw).contains(n));
                let Some(n) = engine else {
                    let key = value::restore(&lua, &match k {
                        Key::Int(i) => Value::Int(*i),
                        Key::Float(f) => Value::Float(*f),
                        Key::Str(s) => Value::Str(s.clone()),
                        Key::Bool(b) => Value::Bool(*b),
                    })?;
                    own.raw_set(key, value::restore(&lua, v)?)?;
                    continue;
                };
                let why = match (n, v) {
                    ("hp", v) => match whole(v, "hp") {
                        Ok(hp) => {
                            if let Some(t) = self.stage.host.get_mut(raw) {
                                *t.hp = hp.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                            }
                            None
                        }
                        Err(_) => Some("hp is a whole number".to_string()),
                    },
                    (world::LIGHT, v) => match number(v, "light") {
                        Ok(b) if (0.0..=1.0).contains(&b) => {
                            self.stage.here.lights.insert(id, b as f32);
                            None
                        }
                        _ => Some("light is 0 (off) to 1".to_string()),
                    },
                    // (Set only where it differs: a key the rules keep only
                    // to read, a gunship's flying, is as it was.)
                    (n, Value::Str(_) | Value::Bool(_)) => {
                        let kept = match v {
                            Value::Str(s) => host::Kept::Text(String::from_utf8_lossy(s).to_string()),
                            _ => host::Kept::Bool(matches!(v, Value::Bool(true))),
                        };
                        match self.stage.host.thing_value(raw, n) == Some(kept.clone()) {
                            true => None,
                            false => self.stage.host.set_thing_value(raw, n, kept).err(),
                        }
                    }
                    (n, _) => Some(format!("{n} is a name, true or false")),
                };
                if let Some(why) = why {
                    self.note(&format!("level {level} kept '{name}' with state.{n} it cannot have now ({why}): as its file has it"));
                }
            }
        }
        Ok(())
    }
}

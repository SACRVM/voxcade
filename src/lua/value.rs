//! A Lua game's state as plain data in Rust: `world.state`, `level.state`,
//! `player.state` and every `thing.state`, copied out of the Lua tables for
//! a snapshot and the state hash, and written back into fresh tables on a
//! restore. Plain data is numbers, strings, booleans and tables of those
//! (each table once, none with a metatable: a view or another state is no
//! data), with numbers, strings or booleans as keys; things only where they
//! cannot outlive their level (`level.state`, `thing.state`). Anything else
//! is refused, naming where it sits.

use anyhow::{Result, anyhow};
use mlua::{Lua, Table, Value as Lv};

use super::world::{ThingRef, handle};
use crate::hash::Stable;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    /// A float's bits (so it compares and hashes exactly).
    Float(u64),
    Str(Vec<u8>),
    /// A thing, by its id.
    Thing(u32),
    /// Keys in order: numbers, then strings, then booleans.
    Table(Vec<(Key, Value)>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Int(i64),
    Float(u64),
    Str(Vec<u8>),
    Bool(bool),
}

impl Key {
    fn rank(&self) -> u8 {
        match self {
            Key::Int(_) | Key::Float(_) => 0,
            Key::Str(_) => 1,
            Key::Bool(_) => 2,
        }
    }

    fn number(&self) -> f64 {
        match self {
            Key::Int(i) => *i as f64,
            Key::Float(f) => f64::from_bits(*f),
            _ => 0.0,
        }
    }

    /// The order keys are walked in: numbers, strings, booleans.
    pub fn before(&self, other: &Key) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            _ if self.rank() != other.rank() => self.rank().cmp(&other.rank()),
            (Key::Int(a), Key::Int(b)) => a.cmp(b),
            (Key::Str(a), Key::Str(b)) => a.cmp(b),
            (Key::Bool(a), Key::Bool(b)) => a.cmp(b),
            _ => self.number().partial_cmp(&other.number()).unwrap_or(Ordering::Equal),
        }
    }
}

/// The tables met so far in one capture of a game's state: a table met
/// twice would come back as two.
pub type Seen = std::collections::HashSet<usize>;

/// What makes the value at `path` no plain data, as the prelude's `plain`
/// names it (`"view"`, `"twice"`, `"thing"`, `"key TYPE"`, `"type TYPE"`).
pub fn mistake(path: &str, why: &str) -> anyhow::Error {
    match why {
        "view" => anyhow!("{path} is a view or another state (a table with a metatable), not plain data: copy into it what you need"),
        "twice" => anyhow!("{path} is a table met before (the same table twice, or one inside itself): state is a tree of plain data"),
        "thing" => anyhow!("{path} is a thing: a thing lives in its level and this state outlives it; keep it in level.state (or, later, keep its name)"),
        w if w.starts_with("key ") => anyhow!("{path}: a key that is a {}; keys are numbers, strings or booleans", &w[4..]),
        w => anyhow!("{path} is a {}: state is numbers, strings, booleans, things and tables of those", w.trim_start_matches("type ")),
    }
}

/// A word with its article: "an integer", "a table".
pub fn a(word: &str) -> String {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) { format!("an {word}") } else { format!("a {word}") }
}

/// The path of the value under `key` in the table at `path`.
pub fn key_path(path: &str, key: &Lv) -> String {
    match key {
        Lv::String(s) => format!("{path}.{}", String::from_utf8_lossy(&s.as_bytes())),
        Lv::Integer(i) => format!("{path}[{i}]"),
        _ => format!("{path}[...]"),
    }
}

/// Copies a Lua value out as plain data; `path` names it in an error;
/// `things`: whether things may be in it (a state that cannot outlive
/// them).
pub fn capture(value: &Lv, path: &str, seen: &mut Seen, things: bool) -> Result<Option<Value>> {
    Ok(Some(match value {
        Lv::Nil => return Ok(None),
        Lv::Boolean(b) => Value::Bool(*b),
        Lv::Integer(i) => Value::Int(*i),
        Lv::Number(n) => Value::Float(n.to_bits()),
        Lv::String(s) => Value::Str(s.as_bytes().to_vec()),
        Lv::UserData(ud) if ud.is::<ThingRef>() => {
            // A player is its seat, plain anywhere; a thing only where it may be.
            let id = ud.borrow::<ThingRef>()?.0;
            if !things && super::host::handle_seat(id).is_none() {
                return Err(mistake(path, "thing"));
            }
            Value::Thing(id)
        }
        Lv::Table(t) => {
            if t.metatable().is_some() {
                return Err(mistake(path, "view"));
            }
            let at = t.to_pointer() as usize;
            if !seen.insert(at) {
                return Err(mistake(path, "twice"));
            }
            let mut entries = Vec::new();
            for pair in t.clone().pairs::<Lv, Lv>() {
                let (k, v) = pair?;
                let key = match &k {
                    Lv::Integer(i) => Key::Int(*i),
                    Lv::Number(n) => Key::Float(n.to_bits()),
                    Lv::String(s) => Key::Str(s.as_bytes().to_vec()),
                    Lv::Boolean(b) => Key::Bool(*b),
                    other => return Err(mistake(path, &format!("key {}", other.type_name()))),
                };
                let name = key_path(path, &k);
                if let Some(v) = capture(&v, &name, seen, things)? {
                    entries.push((key, v));
                }
            }
            entries.sort_by(|a, b| a.0.before(&b.0));
            Value::Table(entries)
        }
        other => return Err(mistake(path, &format!("type {}", other.type_name()))),
    }))
}

/// A fresh Lua value with this data, tables filled in key order.
pub fn restore(lua: &Lua, value: &Value) -> Result<Lv> {
    Ok(match value {
        Value::Bool(b) => Lv::Boolean(*b),
        Value::Int(i) => Lv::Integer(*i),
        Value::Float(f) => Lv::Number(f64::from_bits(*f)),
        Value::Str(s) => Lv::String(lua.create_string(s)?),
        Value::Thing(id) => Lv::UserData(handle(lua, *id)?),
        Value::Table(entries) => Lv::Table(table(lua, entries)?),
    })
}

fn table(lua: &Lua, entries: &[(Key, Value)]) -> Result<Table> {
    let t = lua.create_table()?;
    for (k, v) in entries {
        let key = match k {
            Key::Int(i) => Lv::Integer(*i),
            Key::Float(f) => Lv::Number(f64::from_bits(*f)),
            Key::Str(s) => Lv::String(lua.create_string(s)?),
            Key::Bool(b) => Lv::Boolean(*b),
        };
        t.raw_set(key, restore(lua, v)?)?;
    }
    Ok(t)
}

/// Takes the data into a hash, the same on every machine.
pub fn hash(h: &mut Stable, value: &Value) {
    match value {
        Value::Bool(b) => _ = h.u64(1).u64(*b as u64),
        Value::Int(i) => _ = h.u64(2).u64(*i as u64),
        Value::Float(f) => _ = h.u64(3).u64(*f),
        Value::Str(s) => _ = h.u64(4).bytes(s),
        Value::Thing(id) => _ = h.u64(5).u64(*id as u64),
        Value::Table(entries) => {
            h.u64(6).u64(entries.len() as u64);
            for (k, v) in entries {
                match k {
                    Key::Int(i) => _ = h.u64(2).u64(*i as u64),
                    Key::Float(f) => _ = h.u64(3).u64(*f),
                    Key::Str(s) => _ = h.u64(4).bytes(s),
                    Key::Bool(b) => _ = h.u64(1).u64(*b as u64),
                }
                hash(h, v);
            }
        }
    }
}

impl Value {
    /// An empty table, or none.
    pub fn is_empty(&self) -> bool {
        matches!(self, Value::Table(e) if e.is_empty())
    }
}

/// State as JSON for tools (`voxcade inspect`): tables with names as
/// objects, lists as arrays, a thing as `{"thing": id}`.
pub fn to_json(v: &Value) -> serde_json::Value {
    use serde_json::json;
    match v {
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(f) => json!(f64::from_bits(*f)),
        Value::Str(s) => json!(String::from_utf8_lossy(s)),
        Value::Thing(id) if *id == super::host::PLAYER => json!("player"),
        Value::Thing(id) if let Some(seat) = super::host::handle_seat(*id) => json!(format!("player {seat}")),
        Value::Thing(id) => json!({ "thing": id }),
        Value::Table(entries) => {
            let list = entries.iter().enumerate().all(|(n, (k, _))| *k == Key::Int(n as i64 + 1));
            if list && !entries.is_empty() {
                serde_json::Value::Array(entries.iter().map(|(_, v)| to_json(v)).collect())
            } else {
                serde_json::Value::Object(
                    entries
                        .iter()
                        .map(|(k, v)| {
                            let key = match k {
                                Key::Int(i) => i.to_string(),
                                Key::Float(f) => f64::from_bits(*f).to_string(),
                                Key::Str(s) => String::from_utf8_lossy(s).into_owned(),
                                Key::Bool(b) => b.to_string(),
                            };
                            (key, to_json(v))
                        })
                        .collect(),
                )
            }
        }
    }
}

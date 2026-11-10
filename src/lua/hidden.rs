//! The locals of `main.lua` that its functions keep (upvalues): state a
//! snapshot misses, as the states (`world.state`, `player.state`, `thing.state`) are the only places
//! it copies. The hidden-state guard of `voxcade check` names the ones
//! that changed when a game runs differently from a snapshot.

use std::collections::HashSet;

use mlua::{Function, Lua, Table, Value};

use super::world::ThingRef;
use crate::hash::Stable;

/// The upvalues of every function the script's globals reach (and their
/// upvalues and tables reach), by name, each as one number.
pub fn upvalues(lua: &Lua, globals: &Table) -> mlua::Result<Vec<(String, u64)>> {
    let upvalue: Function = lua.globals().raw_get("debug_getupvalue")?;
    let mut out = Vec::new();
    let mut walked = HashSet::new();
    let mut todo: Vec<Value> = globals.clone().pairs::<Value, Value>().filter_map(Result::ok).map(|(_, v)| v).collect();
    while let Some(v) = todo.pop() {
        match v {
            Value::Function(f) => {
                if !walked.insert(f.to_pointer() as usize) {
                    continue;
                }
                for i in 1.. {
                    let (name, value): (Option<String>, Value) = upvalue.call((f.clone(), i))?;
                    let Some(name) = name else { break };
                    if name == "_ENV" {
                        continue;
                    }
                    out.push((name, sum(&value, &mut Vec::new())));
                    todo.push(value);
                }
            }
            Value::Table(t) if walked.insert(t.to_pointer() as usize) => {
                todo.extend(t.pairs::<Value, Value>().filter_map(Result::ok).map(|(_, v)| v).filter(|v| matches!(v, Value::Function(_) | Value::Table(_))));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// A value as one number: tables by what they hold, in any order.
fn sum(v: &Value, seen: &mut Vec<usize>) -> u64 {
    let mut h = Stable::default();
    match v {
        Value::Nil => _ = h.u64(0),
        Value::Boolean(b) => _ = h.u64(1).u64(*b as u64),
        Value::Integer(i) => _ = h.u64(2).u64(*i as u64),
        Value::Number(n) => _ = h.u64(3).u64(n.to_bits()),
        Value::String(s) => _ = h.u64(4).bytes(&s.as_bytes()),
        Value::Table(t) => {
            let at = t.to_pointer() as usize;
            if seen.contains(&at) {
                h.u64(5);
            } else {
                seen.push(at);
                let mut parts: Vec<u64> = t
                    .clone()
                    .pairs::<Value, Value>()
                    .filter_map(Result::ok)
                    .map(|(k, v)| {
                        let mut p = Stable::default();
                        p.u64(sum(&k, seen)).u64(sum(&v, seen));
                        p.finish()
                    })
                    .collect();
                seen.pop();
                parts.sort_unstable();
                h.u64(6);
                for p in parts {
                    h.u64(p);
                }
            }
        }
        Value::Function(f) => _ = h.u64(7).u64(f.to_pointer() as u64),
        Value::UserData(ud) => match ud.borrow::<ThingRef>() {
            Ok(t) => _ = h.u64(8).u64(t.0 as u64),
            Err(_) => _ = h.u64(9),
        },
        _ => _ = h.u64(10),
    }
    h.finish()
}

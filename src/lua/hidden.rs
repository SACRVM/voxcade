//! The locals of `main.lua` that its functions keep (upvalues): state a
//! snapshot misses, as the states (`world.state`, `player.state`, `thing.state`) are the only places
//! it copies. The hidden-state guard of `voxcade check` names the ones
//! that changed when a game runs differently from a snapshot; `Watch`
//! names those that change while the game is played (the IDE's PLAY).

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
    // A table many functions keep (a module's, a shared local) summed
    // once: a sum begun afresh is the table's alone.
    let mut summed = std::collections::HashMap::new();
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
                    let n = match &value {
                        Value::Table(t) => *summed.entry(t.to_pointer() as usize).or_insert_with(|| sum(&value, &mut Vec::new())),
                        _ => sum(&value, &mut Vec::new()),
                    };
                    out.push((name, n));
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

/// The hidden-state guard while a game is played (the IDE's PLAY): every
/// second of ticks it looks at the locals the game's functions keep
/// (`Game::hidden`, as `voxcade check` does) and names each that changed
/// since it last looked, once a run. `check` plays a second twice and
/// names a local only when the game came out otherwise; this needs no
/// second run, so the author hears of it while playing, before a replay,
/// a save or a game over the network shows it.
#[derive(Default)]
pub struct Watch {
    /// The locals as they were when last looked at (None: look first).
    seen: Option<Vec<(String, u64)>>,
    /// Ticks since.
    ticks: u64,
    /// The names said this run.
    said: std::collections::BTreeSet<String>,
}

impl Watch {
    /// Ticks between two looks: a second.
    pub const EVERY: u64 = crate::replay::CHECK_EVERY;

    /// A run begun: nothing said yet, the locals as they are now how they
    /// start.
    pub fn begin(&mut self, now: Vec<(String, u64)>) {
        *self = Self { seen: Some(now), ..Self::default() };
    }

    /// A file of the code taken in while playing: its functions are new,
    /// so what they keep is looked at afresh (what was said stays said).
    pub fn reloaded(&mut self, now: Vec<(String, u64)>) {
        (self.seen, self.ticks) = (Some(now), 0);
    }

    /// A tick played: once a second the locals as they are now (`look`,
    /// called only then); what to say of those that changed, each name
    /// once a run, in the order of their names.
    pub fn tick(&mut self, look: impl FnOnce() -> Vec<(String, u64)>) -> Vec<String> {
        self.ticks += 1;
        if self.ticks < Self::EVERY {
            return Vec::new();
        }
        self.ticks = 0;
        let now = look();
        let Some(seen) = self.seen.replace(now.clone()) else { return Vec::new() };
        let seen: HashSet<&(String, u64)> = seen.iter().collect();
        let changed: std::collections::BTreeSet<&str> = now.iter().filter(|v| !seen.contains(v)).map(|(name, _)| name.as_str()).collect();
        changed.into_iter().filter(|name| self.said.insert(name.to_string())).map(said).collect()
    }
}

/// What the guard says of a local that changed while the game ran.
pub fn said(name: &str) -> String {
    format!("'{name}' changed while the game ran, a local its functions keep: game state belongs in a state (world.state, player.state, thing.state), where snapshots, saves and replays see it")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn locals(of: &[(&str, u64)]) -> Vec<(String, u64)> {
        of.iter().map(|&(n, v)| (n.to_string(), v)).collect()
    }

    /// The guard looks once a second of ticks, names what changed since
    /// it last looked once a run, in the order of the names, and takes
    /// code taken in or a run begun as the new start.
    #[test]
    fn the_watch_names_what_changed_once_a_run() {
        let mut watch = Watch::default();
        watch.begin(locals(&[("count", 0), ("helper", 7), ("best", 0)]));
        for _ in 1..Watch::EVERY {
            assert!(watch.tick(|| panic!("looked before a second was up")).is_empty());
        }
        let said = watch.tick(|| locals(&[("count", 60), ("helper", 7), ("best", 3)]));
        assert_eq!(said, [super::said("best"), super::said("count")]);
        let quiet = |watch: &mut Watch, now: &[(&str, u64)]| (0..Watch::EVERY).flat_map(|_| watch.tick(|| locals(now))).collect::<Vec<_>>();
        assert!(quiet(&mut watch, &[("count", 120), ("helper", 7), ("best", 3)]).is_empty(), "said once a run");
        watch.reloaded(locals(&[("count", 0), ("helper", 9), ("best", 3)]));
        assert!(quiet(&mut watch, &[("count", 0), ("helper", 9), ("best", 3)]).is_empty(), "new functions are no change");
        watch.begin(locals(&[("count", 0), ("helper", 11)]));
        assert_eq!(quiet(&mut watch, &[("count", 1), ("helper", 11)]), [super::said("count")], "a new run hears it again");
    }
}

//! What a Lua game's files and code are held to as it loads and plays: an
//! inventory's items, the ways a level's file turns things, the handlers'
//! names (the old API's, a misspelt one) and the name nearly meant.

use super::*;

/// An inventory (`state.inventory`, in the state at `path`) counts items of
/// `c.items` by name, each from 0 to its stack.
/// `path` gives where the state sits, made only for a mistake.
pub(super) fn check_inventory(state: &Table, path: &dyn Fn() -> String, items: &std::collections::BTreeMap<String, crate::config::ItemConfig>) -> Result<()> {
    let inventory = match state.raw_get::<mlua::Value>("inventory")? {
        mlua::Value::Nil => return Ok(()),
        mlua::Value::Table(t) => t,
        other => anyhow::bail!("{}.inventory is {}: an inventory is a table of items by name and how many", path(), value::a(other.type_name())),
    };
    let mut entries: Vec<(mlua::Value, mlua::Value)> = inventory.pairs().collect::<mlua::Result<_>>()?;
    entries.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)));
    for (key, count) in entries {
        let mlua::Value::String(name) = &key else { anyhow::bail!("{}: items by name (c.items), not by a {}", value::key_path(&format!("{}.inventory", path()), &key), key.type_name()) };
        let name = name.to_string_lossy();
        let at = || format!("{}.inventory.{name}", path());
        let Some(item) = items.get(&name) else {
            let mut names: Vec<(&i32, &str)> = items.iter().map(|(n, i)| (&i.order, n.as_str())).collect();
            names.sort();
            let names: Vec<&str> = names.into_iter().map(|(_, n)| n).collect();
            if names.is_empty() {
                anyhow::bail!("{}: there are no items (c.items names them)", at());
            }
            anyhow::bail!("{}: no item called '{name}' (c.items: {})", at(), names.join(", "));
        };
        let n = match count {
            mlua::Value::Integer(i) => i,
            mlua::Value::Number(f) if f.fract() == 0.0 => f as i64,
            other => anyhow::bail!("{} is {}: a count is a whole number", at(), match other { mlua::Value::Number(f) => f.to_string(), o => value::a(o.type_name()) }),
        };
        if n < 0 || n > item.stack as i64 {
            anyhow::bail!("{} = {n}: from 0 to {} (c.items.{name}.stack)", at(), item.stack);
        }
    }
    Ok(())
}

/// A thing of a kind that its level's file turns (`facing`, `rotation`)
/// must be one the script moves: a kind with a behavior turns by its
/// ready-made rules, as `level:spawn`'s facing says.
pub(super) fn check_facings(levels: &crate::levels::Levels, kinds: &[Kind]) -> Result<()> {
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

/// The old API's handlers, refused with their new names.
pub(super) const OLD_NAMES: [(&str, &str); 3] = [("load", "on_world_start()"), ("tick", "on_tick()"), ("draw", "on_draw(frame)")];

/// Handlers renamed since, refused with their new names.
pub(super) const RENAMED: [(&str, &str); 1] = [("on_leave", "on_touch_end")];

/// The handlers whose event brings nothing to say: they take no
/// parameters (one named `world` would hide the global). `on_level_start`
/// takes one, how the level was entered.
pub(super) const WITHOUT_PARAMETERS: [&str; 2] = ["on_world_start", "on_tick"];

/// The handler a global function `on_...` nearly is, if it is none
/// itself: a letter or two off (one for the short ones, `on_tik`), case
/// aside. Other names starting with `on_` are the game's own.
pub(super) fn near_handler(name: &str) -> Option<&'static str> {
    let rest = name.strip_prefix("on_")?.to_lowercase();
    let handlers = api::API.iter().filter(|i| matches!(i.what, api::What::Callback { .. }) && i.name.starts_with("on_")).map(|i| i.name);
    if handlers.clone().any(|h| h == name) || RENAMED.iter().any(|(old, _)| *old == name) {
        return None;
    }
    nearest(&rest, handlers.map(|h| (&h["on_".len()..], h)), |h| if h.len() <= 4 { 1 } else { 2 })
}

/// What the name of `among` that `name` nearly is stands for: at most
/// `most(that name)` edits off, the nearest (of equals the first).
pub(super) fn nearest<'a, T>(name: &str, among: impl Iterator<Item = (&'a str, T)>, most: impl Fn(&str) -> usize) -> Option<T> {
    among.map(|(n, t)| (edits(name, n), n, t)).filter(|(d, n, _)| *d <= most(n)).min_by_key(|(d, ..)| *d).map(|(.., t)| t)
}

/// How many letters to put in, take out, change or swap with the next to
/// make `a` into `b` (Damerau-Levenshtein, the optimal string alignment).
pub(super) fn edits(a: &str, b: &str) -> usize {
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
pub(super) fn misspelt_handler(env: &Table) -> Result<()> {
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

//! "key and button names checked" (0.7.0): a binding of `conf.lua` that
//! would do nothing (`c.keys`, `c.pad`: an action the game lacks, a key or
//! a button no name of, as a pad's "a" for "south") loaded in 0.6 and did
//! nothing; since 0.7 it is a mistake as the game loads
//! (`controls::dead_bindings`). The step names each by its line with the
//! name meant where that is clear; which button a letter means is the
//! maker's to choose, so it changes nothing.

use super::words::{line_at, paths, tokens};
use super::*;

/// What the step says of a game's bindings: each that would do nothing,
/// with its line. Nothing for a game whose bindings all work, or whose
/// `conf.lua` does not run.
pub fn plan(dir: &Path) -> Result<Vec<String>> {
    use crate::game::Game;
    let files = crate::files::Files::folder(dir);
    let mut cfg = crate::config::GameConfig::default();
    if crate::lua::conf::run(&files, &mut cfg).is_err() {
        return Ok(Vec::new());
    }
    let actions = crate::Voxcade::actions(&cfg);
    if actions.is_empty() && cfg.kit.is_some() {
        return Ok(Vec::new());
    }
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    let toks = tokens(&conf);
    let found = paths(&toks);
    let mut out = Vec::new();
    for dead in crate::controls::dead_bindings(&cfg.keys, &cfg.pad, &actions, cfg.pointer.is_some()) {
        // The action's name in the file, and the name's quotes in its list.
        let action = found.iter().find(|(_, p)| p.as_slice() == [dead.table, dead.action.as_str()]).map(|(i, _)| *i);
        let at = action.map(|i| match &dead.name {
            Some(name) if toks.get(i + 1).map(|t| t.text) == Some("=") && toks.get(i + 2).map(|t| t.text) == Some("{") => {
                let list = toks[i + 3..].iter().take_while(|t| t.text != "}");
                list.filter(|t| t.text.len() >= 2 && &t.text[1..t.text.len() - 1] == name).map(|t| t.range.start).next().unwrap_or(toks[i].range.start)
            }
            _ => toks[i].range.start,
        });
        let at = at.map_or("conf.lua".to_string(), |at| format!("conf.lua:{}", line_at(&conf, at)));
        out.push(format!("{at}: {}", dead.said()));
    }
    if !out.is_empty() {
        out.push("such a binding did nothing; since Voxcade 0.7 the game does not load with it: write the name meant (a pad's buttons by where they lie) or take it out; nothing here is changed".into());
    }
    Ok(out)
}

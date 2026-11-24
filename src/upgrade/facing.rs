use super::*;

/// What the step says of a game's kinds that state a `facing`
/// (`c.things.NAME.facing`): until 0.7 only the platformer kit read it (the
/// shooter kit refused it), so in a game without a kit its things came in
/// facing the camera; now they face that way. Each kind with its line and
/// how to keep the old look. Nothing for a game of the platformer kit, one
/// whose kinds state none, or one whose `conf.lua` does not run.
pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let files = crate::files::Files::folder(dir);
    let mut cfg = crate::config::GameConfig::default();
    if crate::lua::conf::run(&files, &mut cfg).is_err() || cfg.kit.as_deref() == Some("platformer") {
        return Ok(Vec::new());
    }
    let Ok(kinds) = crate::config::resolve_like("things", &cfg.things) else { return Ok(Vec::new()) };
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    let code: Vec<&str> = conf.lines().map(|l| l.split("--").next().unwrap_or("")).collect();
    let mut out = Vec::new();
    for (name, kind) in &kinds {
        let Some(way) = kind.get("facing") else { continue };
        let written = toml_way(way);
        match cfg.things.get(name).and_then(|own| own.get("facing")) {
            Some(_) => {
                // Its line: the first stating a facing from where the kind
                // begins (`c.things.door = {`, or `door = {` in a table).
                let begins = code.iter().position(|l| l.contains(&format!("things.{name}")) || l.trim_start().starts_with(&format!("{name} ")) || l.trim_start().starts_with(&format!("{name}=")));
                let line = begins.and_then(|b| code[b..].iter().position(|l| l.contains("facing") && !l.contains("c.player")).map(|i| b + i + 1));
                let at = line.map_or("conf.lua".to_string(), |n| format!("conf.lua:{n}"));
                out.push(format!("{at}: c.things.{name}.facing = {written}: its things come in facing that way now; take the facing out to keep them facing the camera"));
            }
            None => {
                let like = cfg.things.get(name).and_then(|own| own.get("like")).and_then(toml::Value::as_str).unwrap_or("another kind");
                out.push(format!("c.things.{name} is like {like} and faces {written} with it now"));
            }
        }
    }
    if !out.is_empty() {
        out.push("a kind's facing turns its things as they come in since Voxcade 0.7 in every game, not only with the platformer kit (a thing's own way, in its level's file or level:spawn's facing, still wins); nothing here is changed".into());
    }
    Ok(out)
}

/// A way as `conf.lua` writes it: `{ 1, 0 }`.
fn toml_way(v: &toml::Value) -> String {
    match v.as_array() {
        Some(list) => format!("{{ {} }}", list.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")),
        None => v.to_string(),
    }
}

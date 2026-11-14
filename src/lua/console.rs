//! Trying a game late without playing there (`docs/game-model.md`,
//! "Scenarios and the console"): a scenario begins a run, a console line
//! runs in the game as it plays.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// `Game::scenarios`: each scenario by name, with its `about`.
    pub(super) fn scenarios_about(&mut self) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        for name in scenario_names(&self.files) {
            let path = format!("scenarios/{name}.lua");
            let src = self.files.read_to_string(&path).with_context(|| format!("reading {path}"))?;
            let about = self.reopened(false, |game, env| {
                let table = scenario_table(&path, game.sandbox.run_in(&path, &src, env, "the start")?)?;
                Ok(table.raw_get::<Option<String>>("about")?.unwrap_or_default())
            });
            out.push((name, about.unwrap_or_else(|e| format!("(a mistake: {e:#})"))));
        }
        Ok(out)
    }

    /// `Game::start_scenario`: the run begun from scenario `name`.
    pub(super) fn begin_scenario(&mut self, name: &str) -> Result<()> {
        let names = scenario_names(&self.files);
        if !names.iter().any(|n| n == name) {
            match names.is_empty() {
                true => anyhow::bail!("no scenario '{name}': the game has none yet (scenarios/NAME.lua in its folder, docs/game-model.md)"),
                false => anyhow::bail!("no scenario '{name}' (the game's: {})", names.join(", ")),
            }
        }
        if let Some(e) = self.halted() {
            anyhow::bail!("{e}");
        }
        anyhow::ensure!(self.stage.host.sim().tick == 0 && self.start_text.is_none(), "a scenario begins a run: before its first tick, and not after a save");
        let path = format!("scenarios/{name}.lua");
        let src = self.files.read_to_string(&path).with_context(|| format!("reading {path}"))?;
        self.modules.push((path.clone(), lex::tokens(&path, &src)?));
        let set_up = self.reopened(true, |game, env| {
            let table = scenario_table(&path, game.sandbox.run_in(&path, &src, env, "the start")?)?;
            for key in table.pairs::<mlua::Value, mlua::Value>() {
                let (key, _) = key?;
                let key = key.to_string().unwrap_or_else(|_| "?".into());
                anyhow::ensure!(["about", "level", "place", "setup"].contains(&key.as_str()), "{path}: no field called '{key}' (about, level, place, setup)");
            }
            // A scenario that names where the run begins needs no setup.
            let setup: Option<Function> = table.raw_get("setup")?;
            let (level, place): (Option<String>, Option<String>) = (table.raw_get("level")?, table.raw_get("place")?);
            anyhow::ensure!(
                setup.is_some() || level.is_some() || place.is_some(),
                "{path}: a scenario says where the run begins (level = \"name\", place = \"name\") or sets it up: setup is the function that sets the run up: setup = function() ... end"
            );
            match (level, place) {
                (Some(level), place) => {
                    let Some(index) = game.stage.levels.find(&level) else {
                        let all: Vec<&str> = game.stage.levels.list.iter().map(|l| l.name.as_str()).collect();
                        anyhow::bail!("{path}: level = \"{level}\" is none of the game's levels ({})", all.join(", "));
                    };
                    if let Some(place) = &place {
                        anyhow::ensure!(game.place_in(index, Some(place.clone())).is_some(), "{path}: the level {level} has no place called {place}");
                    }
                    game.stage.ask = Some(Ask::GoTo { level, place });
                    game.change_levels();
                    if let Some(e) = game.halted() {
                        anyhow::bail!("{e}");
                    }
                }
                (None, Some(_)) => anyhow::bail!("{path}: a place belongs to a level: level = \"...\", place = \"...\""),
                (None, None) => {}
            }
            match &setup {
                Some(setup) => game.with_world(|sb, _| sb.call_start::<()>(setup, ())),
                None => Ok(()),
            }
        });
        if let Err(e) = set_up {
            self.halt(anyhow::anyhow!("{e:#}"));
            return Err(e);
        }
        self.settle_between(&format!("the scenario {name}"))
    }

    /// `Game::eval`: a console line run in the game, what it printed and
    /// its values.
    pub(super) fn console_line(&mut self, code: &str) -> Result<String> {
        if let Some(e) = self.halted() {
            anyhow::bail!("the game has stopped: {e}");
        }
        // An expression shows its value, as Lua's own console does.
        let expression = format!("return {code}");
        let chunk = if self.sandbox.lua.load(&expression).into_function().is_ok() { expression } else { code.to_string() };
        // What the line prints is shown with it, and goes to OUTPUT too.
        let before = std::mem::take(&mut *self.sandbox.printed.borrow_mut());
        let ran = self.reopened(false, |game, env| {
            game.with_world(|sb, _| {
                let values = sb.run_in("console", &chunk, env, "the console")?;
                let tostring = sb.tool("tostring")?;
                Ok(values.iter().map(|v| shown(v, &tostring)).collect::<Vec<String>>())
            })
        });
        let printed: Vec<String> = {
            let mut kept = self.sandbox.printed.borrow_mut();
            let said: Vec<String> = kept.drain(..).collect();
            kept.extend(before);
            kept.extend(said.iter().cloned());
            said
        };
        let settled = self.settle_between("the console line");
        let values = ran?;
        settled?;
        let mut lines = printed;
        if !values.is_empty() {
            lines.push(values.join("\t"));
        }
        Ok(lines.join("\n"))
    }
}

/// The game's scenarios by name (`scenarios/NAME.lua`), in order.
fn scenario_names(files: &Files) -> Vec<String> {
    files.list("scenarios").iter().filter_map(|p| p.file_name()?.to_str()?.strip_suffix(".lua").map(str::to_string)).collect()
}

/// What a scenario's file gave back: a table.
fn scenario_table(path: &str, got: mlua::MultiValue) -> Result<Table> {
    match got.into_iter().next() {
        Some(mlua::Value::Table(t)) => Ok(t),
        _ => anyhow::bail!("{path}: a scenario gives back a table: return {{ about = \"...\", setup = function() ... end }}"),
    }
}

/// A value as the console shows it: plain data as a state is written in
/// a save, anything else as `tostring` gives it (a thing, a function).
fn shown(v: &mlua::Value, tostring: &Function) -> String {
    match v {
        mlua::Value::Table(_) => match value::capture(v, "", &mut value::Seen::default(), false) {
            Ok(Some(plain)) => data::write(&plain),
            _ => tostring.call::<String>(v.clone()).unwrap_or_default(),
        },
        mlua::Value::String(s) => format!("{:?}", s.to_string_lossy()),
        other => tostring.call::<String>(other.clone()).unwrap_or_default(),
    }
}

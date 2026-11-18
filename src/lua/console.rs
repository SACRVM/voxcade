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

    /// `Game::start_scenario`: the run begun from scenario `name`, its
    /// `setup` run; or, where it names a level the run did not begin in
    /// (at its place), nothing done but that level and place said.
    pub(super) fn begin_scenario(&mut self, name: &str) -> Result<Begins> {
        let (path, src) = self.scenario_file(name)?;
        self.code.modules.push((path.clone(), lex::tokens(&path, &src)?));
        let mut elsewhere = None;
        let set_up = self.reopened(true, |game, env| {
            let (begins, setup) = game.scenario_read(&path, &src, env)?;
            if let Some((level, place)) = begins
                && !game.begun_at(&level, place.as_deref())
            {
                elsewhere = Some((level, place));
                return Ok(());
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
        if elsewhere.is_some() {
            return Ok(elsewhere);
        }
        self.settle_between(&format!("the scenario {name}"))?;
        Ok(None)
    }

    /// Whether the run began in `level` at `place` (its start unless named)
    /// and has entered no other level since.
    fn begun_at(&self, level: &str, place: Option<&str>) -> bool {
        self.stage.levels.find(level) == Some(self.stage.level) && self.stage.arrived.name == place.unwrap_or("start") && self.stage.visits.iter().sum::<u32>() == 1
    }

    /// Scenario `name`'s file and text, the run not begun yet.
    fn scenario_file(&self, name: &str) -> Result<(String, String)> {
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
        Ok((path, src))
    }

    /// A scenario's table read and checked: the level and place it begins
    /// in (the level's start unless it names one), its setup.
    fn scenario_read(&mut self, path: &str, src: &str, env: &Table) -> Result<(Begins, Option<Function>)> {
        let table = scenario_table(path, self.sandbox.run_in(path, src, env, "the start")?)?;
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
        let begins = match (level, place) {
            (Some(level), place) => {
                let Some(index) = self.stage.levels.find(&level) else {
                    let all: Vec<&str> = self.stage.levels.list.iter().map(|l| l.name.as_str()).collect();
                    anyhow::bail!("{path}: level = \"{level}\" is none of the game's levels ({})", all.join(", "));
                };
                if let Some(place) = &place {
                    anyhow::ensure!(self.place_in(index, Some(place.clone())).is_some(), "{path}: the level {level} has no place called {place}");
                }
                Some((level, place))
            }
            (None, Some(_)) => anyhow::bail!("{path}: a place belongs to a level: level = \"...\", place = \"...\""),
            (None, None) => None,
        };
        Ok((begins, setup))
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
                let pairs = sb.tool("pairs")?;
                Ok(values.iter().map(|v| shown(&sb.lua, v, &tostring, &pairs)).collect::<Vec<String>>())
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

/// Where a scenario begins the run: its level and the place there (None:
/// the level's start); None: the run's start level.
type Begins = Option<(String, Option<String>)>;

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
/// a save, anything else as `tostring` gives it (a thing, a function). A
/// read-only view (a frozen global, `thing.props`, a top-level state) is
/// shown as the table it shows; a top-level state's engine keys (`score`,
/// `hp`) come with it, read through the script's own `pairs`.
fn shown(lua: &mlua::Lua, v: &mlua::Value, tostring: &Function, pairs: &Function) -> String {
    match v {
        mlua::Value::Table(t) => match table_shown(lua, t, pairs) {
            Ok(Some(plain)) => data::write(&plain),
            _ => tostring.call::<String>(v.clone()).unwrap_or_default(),
        },
        mlua::Value::String(s) => format!("{:?}", s.to_string_lossy()),
        other => tostring.call::<String>(other.clone()).unwrap_or_default(),
    }
}

/// The data a table shows: a plain one as it is, a view by what the
/// script's `pairs` walks in it (its top level only; nested views are
/// read as the table they show).
fn table_shown(lua: &mlua::Lua, t: &Table, pairs: &Function) -> Result<Option<crate::lua::Value>> {
    if t.metatable().is_none() {
        return value::capture(&mlua::Value::Table(t.clone()), "", &mut value::Seen::default(), false);
    }
    let (next, state, first): (Function, mlua::Value, mlua::Value) = pairs.call(t.clone())?;
    let top = lua.create_table()?;
    let mut key = first;
    loop {
        let (k, v): (mlua::Value, mlua::Value) = next.call((state.clone(), key))?;
        if k.is_nil() {
            break;
        }
        top.raw_set(k.clone(), v)?;
        key = k;
    }
    value::capture_given(lua, &mlua::Value::Table(top), "", false)
}

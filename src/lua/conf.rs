//! `conf.lua`: a Lua game's settings, set in code. `conf(c)` gets the
//! engine's defaults in `c` (sections missing there are made on first use:
//! `c.weapons.rifle = {...}`), changes what the game wants, and what comes
//! back is read the way a settings file was: unknown keys and wrong types
//! are mistakes. Lua's integers stay integers and its floats floats, so the
//! settings are exactly what was written.


use anyhow::{Context, Result, bail};
use mlua::{Lua, Table, Value};

use super::sandbox::Sandbox;
use crate::config::GameConfig;
use crate::files::Files;

/// Runs the game's `conf.lua` over `cfg`, if there is one.
pub fn run(files: &Files, cfg: &mut GameConfig) -> Result<()> {
    let path = files.path("conf.lua");
    if !files.is_file(&path) {
        return Ok(());
    }
    let src = files.read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let sandbox = Sandbox::new()?;
    let env = sandbox.run("conf.lua", &src)?;
    let conf = Sandbox::function(&env, "conf")?.context("conf.lua: it defines no `function conf(c)`")?;
    let defaults = toml::Value::try_from(&*cfg)?;
    let c = to_lua(&sandbox.lua, &defaults)?;
    let Value::Table(c) = c else { bail!("the engine's settings are no table") };
    let grow: mlua::Function = sandbox.lua.load("return function(t, k) local v = {} rawset(t, k, v) return v end").eval()?;
    let meta = sandbox.lua.create_table()?;
    meta.raw_set("__index", grow)?;
    c.set_metatable(Some(meta))?;
    sandbox.call_start::<()>(&conf, c.clone())?;
    let value = from_lua(&Value::Table(c), Some(&defaults), "c")?;
    *cfg = value.try_into().map_err(|e: toml::de::Error| anyhow::anyhow!("conf.lua: {}", e.message()))?;
    Ok(())
}

fn to_lua(lua: &Lua, v: &toml::Value) -> Result<Value> {
    Ok(match v {
        toml::Value::String(s) => Value::String(lua.create_string(s)?),
        toml::Value::Integer(i) => Value::Integer(*i),
        toml::Value::Float(f) => Value::Number(*f),
        toml::Value::Boolean(b) => Value::Boolean(*b),
        toml::Value::Datetime(d) => Value::String(lua.create_string(d.to_string())?),
        toml::Value::Array(a) => {
            let t = lua.create_table()?;
            for (i, item) in a.iter().enumerate() {
                t.raw_set(i + 1, to_lua(lua, item)?)?;
            }
            Value::Table(t)
        }
        toml::Value::Table(m) => {
            let t = lua.create_table()?;
            for (k, item) in m {
                t.raw_set(k.as_str(), to_lua(lua, item)?)?;
            }
            Value::Table(t)
        }
    })
}

/// A setting back from Lua; `default` is what the engine had there (an
/// empty table takes its shape: a list or a table).
fn from_lua(v: &Value, default: Option<&toml::Value>, path: &str) -> Result<toml::Value> {
    Ok(match v {
        Value::Boolean(b) => toml::Value::Boolean(*b),
        Value::Integer(i) => toml::Value::Integer(*i),
        Value::Number(n) if n.is_finite() => toml::Value::Float(*n),
        Value::Number(_) => bail!("{path}: not a number"),
        Value::String(s) => toml::Value::String(s.to_str().map(|s| s.to_string()).map_err(|_| anyhow::anyhow!("{path}: text that is no UTF-8"))?),
        Value::Table(t) => table(t, default, path)?,
        other => bail!("{path}: a {} is no setting", other.type_name()),
    })
}

fn table(t: &Table, default: Option<&toml::Value>, path: &str) -> Result<toml::Value> {
    let mut list: Vec<(i64, Value)> = Vec::new();
    let mut named: Vec<(String, Value)> = Vec::new();
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, v) = pair?;
        match k {
            Value::Integer(i) => list.push((i, v)),
            Value::String(s) => named.push((s.to_str().map(|s| s.to_string()).map_err(|_| anyhow::anyhow!("{path}: a name that is no UTF-8"))?, v)),
            other => bail!("{path}: a key that is a {}", other.type_name()),
        }
    }
    if !list.is_empty() && !named.is_empty() {
        bail!("{path}: a list and named settings mixed in one table");
    }
    if !list.is_empty() {
        list.sort_by_key(|(i, _)| *i);
        if list.iter().enumerate().any(|(n, (i, _))| *i != n as i64 + 1) {
            bail!("{path}: a list with gaps (keys 1, 2, 3 ...)");
        }
        let item_default = default.and_then(|d| d.as_array()).and_then(|a| a.first());
        return Ok(toml::Value::Array(list.iter().map(|(i, v)| from_lua(v, item_default, &format!("{path}[{i}]"))).collect::<Result<_>>()?));
    }
    if named.is_empty() && default.is_some_and(|d| d.is_array()) {
        return Ok(toml::Value::Array(Vec::new()));
    }
    let mut out = toml::Table::new();
    for (k, v) in named {
        let child = default.and_then(|d| d.get(&k));
        out.insert(k.clone(), from_lua(&v, child, &format!("{path}.{k}"))?);
    }
    Ok(toml::Value::Table(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conf(src: &str) -> Result<GameConfig> {
        let dir = crate::scratch::Scratch::new("conf");
        std::fs::write(dir.join("conf.lua"), src).unwrap();
        let mut cfg = GameConfig::default();
        run(&crate::files::Files::folder(&dir), &mut cfg).map(|_| cfg)
    }

    /// `conf(c)` changes the engine's settings; what it leaves keeps its
    /// default; numbers keep their kind (an integer stays one).
    #[test]
    fn conf_fills_the_engine_config() {
        let cfg = conf("function conf(c)\n  c.title = 'Sokovox'\n  c.physics.gravity = 520\n  c.camera.pitch = 40.5\n  c.things.gem = { model = 'gem.vox' }\n  c.actions = { 'jump', 'fire' }\n  c.light.sky = { 0.4, 0.6, 0.8 }\nend\n").unwrap();
        assert_eq!(cfg.title, "Sokovox");
        assert_eq!((cfg.physics.gravity, cfg.camera.pitch), (520.0, 40.5));
        assert_eq!(cfg.physics.max_fall, GameConfig::default().physics.max_fall);
        assert_eq!(cfg.things["gem"]["model"].as_str(), Some("gem.vox"));
        assert_eq!(cfg.actions, ["jump", "fire"]);
        assert_eq!(cfg.light.sky, [0.4, 0.6, 0.8]);
        let game = conf("function conf(c) c.weapons.rifle = { speed = 340 } end").unwrap();
        assert_eq!(game.game["weapons"]["rifle"]["speed"].as_integer(), Some(340), "a new section is made on first use");
    }

    /// A misspelt setting, a wrong type or a function is a mistake that
    /// says where.
    #[test]
    fn a_misspelt_setting_is_named() {
        let e = conf("function conf(c) c.physics.gravty = 3 end").unwrap_err().to_string();
        assert!(e.contains("gravty"), "{e}");
        assert!(conf("function conf(c) c.camera.pitch = 'steep' end").is_err());
        let e = conf("function conf(c) c.things.gem = { hit = function() end } end").unwrap_err().to_string();
        assert!(e.contains("c.things.gem.hit"), "{e}");
        assert!(conf("function conf(c) c.title = os.time() end").is_err(), "the sandbox let os in");
        assert!(conf("x = 1").unwrap_err().to_string().contains("conf(c)"));
    }
}

//! `conf.lua`: a Lua game's settings, set in code. `conf(c)` gets the
//! engine's defaults in `c` (sections missing there are made on first use:
//! `c.weapons.rifle = {...}`), changes what the game wants, and what comes
//! back is read the way a settings file was: unknown keys and wrong types
//! are mistakes. Lua's integers stay integers and its floats floats, so the
//! settings are exactly what was written.


use anyhow::{Context, Result, bail};
use mlua::{Table, Value};

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
    let c = super::world::from_toml(&sandbox.lua, &defaults)?;
    let Value::Table(c) = c else { bail!("the engine's settings are no table") };
    let grow: mlua::Function = sandbox.lua.load("return function(t, k) local v = {} rawset(t, k, v) return v end").eval()?;
    let meta = sandbox.lua.create_table()?;
    meta.raw_set("__index", grow)?;
    c.set_metatable(Some(meta))?;
    sandbox.call_start::<()>(&conf, c.clone())?;
    let value = from_lua(&Value::Table(c), Some(&defaults), "c")?;
    renamed(&value)?;
    colors(&value)?;
    actions(&value)?;
    crate::config::check_ranges(&value).map_err(|e| anyhow::anyhow!("conf.lua: {e}"))?;
    *cfg = settings(value, &defaults)?;
    Ok(())
}

/// The settings `conf(c)` gave back, read; a mistake names the setting it
/// is in (`c.camera.pitch`) and says what it wants there.
fn settings(value: toml::Value, defaults: &toml::Value) -> Result<GameConfig> {
    let whole = match value.clone().try_into::<GameConfig>() {
        Ok(cfg) => return Ok(cfg),
        Err(e) => e,
    };
    let Some((path, e)) = culprit(&value, defaults) else { bail!("conf.lua: {}", whole.message()) };
    let shown = std::iter::once("c").chain(path.iter().map(String::as_str)).collect::<Vec<_>>().join(".");
    let parent = shown.rsplit_once('.').map_or("c", |(p, _)| p).to_string();
    bail!("conf.lua: {shown}: {}", plainly(e.message(), &parent, path.last().map_or("", String::as_str)))
}

/// The setting a mistake is in: the deepest one that, set alone over the
/// engine's defaults, is still refused (a mistake only two settings make
/// together is found at the section).
fn culprit(value: &toml::Value, defaults: &toml::Value) -> Option<(Vec<String>, toml::de::Error)> {
    let refused = |path: &[String], v: &toml::Value| -> Option<toml::de::Error> {
        let mut root = defaults.clone();
        let mut at = &mut root;
        for key in &path[..path.len() - 1] {
            if !at.get(key).is_some_and(toml::Value::is_table) {
                at.as_table_mut()?.insert(key.clone(), toml::Value::Table(toml::Table::new()));
            }
            at = at.get_mut(key)?;
        }
        at.as_table_mut()?.insert(path.last()?.clone(), v.clone());
        root.try_into::<GameConfig>().err()
    };
    let (mut path, mut here, mut found) = (Vec::new(), value, None);
    while let toml::Value::Table(t) = here {
        // A setting refused only for a key it lacks when set alone (one of
        // two that go together) is not the mistake: a table that lacks
        // one is, and a wrong value before either.
        let missing = |e: &toml::de::Error| e.message().starts_with("missing field");
        let rank = |v: &toml::Value, e: &toml::de::Error| if !missing(e) { 0 } else if v.is_table() { 1 } else { 2 };
        let Some((k, v, e)) = t
            .iter()
            .filter_map(|(k, v)| {
                let mut p = path.clone();
                p.push(k.clone());
                refused(&p, v).map(|e| (k, v, e)).filter(|(_, v, e)| rank(v, e) < 2)
            })
            .min_by_key(|(_, v, e)| rank(v, e))
        else {
            break;
        };
        path.push(k.clone());
        (here, found) = (v, Some(e));
    }
    found.map(|e| (path, e))
}

/// serde's words for a setting's mistake in a game author's: what is
/// wanted and what was given; an unknown name with the one meant and
/// those there are.
fn plainly(message: &str, parent: &str, name: &str) -> String {
    let names = |list: &str| -> Vec<String> { list.trim_start_matches("one of ").replace("` or `", "`, `").split(", ").map(|n| n.trim_matches('`').to_string()).collect() };
    let near = |all: &[String]| crate::audio::words::near(name, all.iter().map(String::as_str)).map_or(String::new(), |n| format!(" (did you mean `{n}`?)"));
    if let Some((given, wanted)) = message.strip_prefix("invalid type: ").and_then(|m| m.split_once(", expected ")) {
        return format!("{} wanted, not {}", wanted_words(wanted), given_words(given));
    }
    if let Some((_, list)) = message.strip_prefix("unknown field `").and_then(|m| m.split_once("`, expected ")) {
        let all = names(list);
        return format!("not a setting of {parent}{}; it has {}", near(&all), all.join(", "));
    }
    if let Some(key) = message.strip_prefix("missing field `").and_then(|m| m.split('`').next()) {
        return format!("{key} is missing: it goes with the rest of {parent}.{name}");
    }
    if let Some((given, list)) = message.strip_prefix("unknown variant `").and_then(|m| m.split_once("`, expected ")) {
        let all = names(list);
        let near = crate::audio::words::near(given, all.iter().map(String::as_str)).map_or(String::new(), |n| format!(" (did you mean \"{n}\"?)"));
        return format!("\"{given}\" is none of {}{near}", all.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", "));
    }
    message.to_string()
}

/// What serde says a setting expects, in a game author's words.
fn wanted_words(wanted: &str) -> String {
    match wanted {
        "f32" | "f64" | "a float" => "a number".into(),
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" | "an integer" => "a whole number".into(),
        "a boolean" => "true or false".into(),
        "a string" | "a borrowed string" => "text in quotes".into(),
        "a sequence" => "a list { ... }".into(),
        "a map" => "a table { ... }".into(),
        w if w.starts_with("struct ") => "a table { ... }".into(),
        w if w.starts_with("a tuple of size ") || w.starts_with("an array of length ") => format!("a list of {}", w.rsplit(' ').next().unwrap_or("")),
        other => other.to_string(),
    }
}

/// What serde says was given, as it reads in `conf.lua`.
fn given_words(given: &str) -> String {
    let quoted = |s: &str| s.trim_matches('`').to_string();
    if let Some(s) = given.strip_prefix("string ") {
        return s.to_string();
    }
    for prefix in ["integer ", "floating point ", "boolean "] {
        if let Some(v) = given.strip_prefix(prefix) {
            return quoted(v);
        }
    }
    match given {
        "sequence" => "a list".into(),
        "map" => "a table".into(),
        other => other.to_string(),
    }
}

/// The most actions a game names (`c.actions`); the input holds one bit
/// each, in 32.
pub const MOST_ACTIONS: usize = 32;

/// More actions than the input holds is a mistake (the 33rd would press
/// the 1st).
fn actions(c: &toml::Value) -> Result<()> {
    let n = c.get("actions").and_then(toml::Value::as_array).map_or(0, Vec::len);
    if n > MOST_ACTIONS {
        bail!("conf.lua: c.actions names {n} actions; a game has {MOST_ACTIONS} at most");
    }
    Ok(())
}

/// A colour that is neither three numbers nor "#rrggbb" is a mistake
/// naming it.
fn colors(c: &toml::Value) -> Result<()> {
    for (section, name) in crate::config::COLORS {
        let Some(v) = c.get(section).and_then(|t| t.get(name)) else { continue };
        let numbers = v.as_array().is_some_and(|a| a.len() == 3 && a.iter().all(|n| n.is_float() || n.is_integer()));
        if !numbers && !v.as_str().is_some_and(|t| crate::grid::hex(t).is_some()) {
            bail!("conf.lua: c.{section}.{name}: {}", crate::config::COLOR_IS);
        }
    }
    Ok(())
}

/// A setting under the name it had before Voxcade took other tools' words
/// (`turn`, `turns`, `behaviour`) is a mistake naming the new one.
fn renamed(c: &toml::Value) -> Result<()> {
    let say = |old: String, new: String| -> Result<()> { bail!("conf.lua: {old} is called {new} now (voxcade upgrade renames it)") };
    if c.get("camera").and_then(|t| t.get("turns")).is_some() {
        return say("c.camera.turns".into(), "c.camera.rotates".into());
    }
    if let Some(things) = c.get("things").and_then(toml::Value::as_table) {
        for (name, kind) in things {
            for (old, new) in crate::sim::things::RENAMED {
                if kind.get(old).is_some() {
                    return say(format!("c.things.{name}.{old}"), format!("c.things.{name}.{new}"));
                }
            }
        }
    }
    if let Some(props) = c.get("title_screen").and_then(|t| t.get("props")).and_then(toml::Value::as_array) {
        for (i, prop) in props.iter().enumerate() {
            if let Some(turn) = prop.get("turn") {
                let was = turn.as_integer().map_or(String::new(), |q| format!(": turn = {q} is rotation = {}", crate::map::Put::rotation_of(q as i32)));
                bail!("conf.lua: c.title_screen.props[{}].turn (quarter turns anticlockwise) is called rotation now, in degrees clockwise{was} (voxcade upgrade renames it)", i + 1);
            }
        }
    }
    Ok(())
}

/// A setting back from Lua; `default` is what the engine had there (an
/// empty table takes its shape: a list or a table).
pub(crate) fn from_lua(v: &Value, default: Option<&toml::Value>, path: &str) -> Result<toml::Value> {
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
        for (line, says) in [
            ("c.camera.pitch = 'steep'", "conf.lua: c.camera.pitch: a number wanted, not \"steep\""),
            ("c.camera.pitchh = 3", "conf.lua: c.camera.pitchh: not a setting of c.camera (did you mean `pitch`?); it has pitch, yaw"),
            ("c.physics.gravity = true", "conf.lua: c.physics.gravity: a number wanted, not true"),
            ("c.players = 1.5", "conf.lua: c.players: a whole number wanted, not 1.5"),
            ("c.camera.rotates = 'fre'", "conf.lua: c.camera.rotates: \"fre\" is none of"),
            ("c.title = { 1 }", "conf.lua: c.title: text in quotes wanted, not a list"),
        ] {
            let e = conf(&format!("function conf(c) {line} end")).unwrap_err().to_string();
            assert!(e.contains(says), "{line}: {e}");
        }
        let e = conf("function conf(c) c.things.gem = { hit = function() end } end").unwrap_err().to_string();
        assert!(e.contains("c.things.gem.hit"), "{e}");
        assert!(conf("function conf(c) c.title = os.time() end").is_err(), "the sandbox let os in");
        assert!(conf("x = 1").unwrap_err().to_string().contains("conf(c)"));
        let e = conf("function conf(c) c.light.sky = 'blue' end").unwrap_err().to_string();
        assert!(e.contains("c.light.sky: a color is three numbers from 0 to 1"), "{e}");
    }

    /// A color is three numbers from 0 to 1 or "#rrggbb", as a level's
    /// file writes it.
    #[test]
    fn a_color_is_numbers_or_hex() {
        let cfg = conf("function conf(c)\n  c.light.sky = '#20304a'\n  c.light.sun_color = { 1, 0.9, 0.8 }\n  c.effects.outline_color = '#000000'\nend\n").unwrap();
        assert_eq!(cfg.light.sky, [32.0 / 255.0, 48.0 / 255.0, 74.0 / 255.0]);
        assert_eq!((cfg.light.sun_color, cfg.effects.outline_color), ([1.0, 0.9, 0.8], [0.0; 3]));
    }

    /// The settings' names from before Voxcade took other tools' words are
    /// refused with their new names; the new ones are taken.
    #[test]
    fn an_old_name_says_the_new_one() {
        for (line, says) in [
            ("c.camera.turns = 'free'", "c.camera.turns is called c.camera.rotates now"),
            ("c.things.gem = { behaviour = 'pickup' }", "c.things.gem.behaviour is called c.things.gem.behavior now"),
            ("c.things.gem = { turns = 'free' }", "c.things.gem.turns is called c.things.gem.rotates now"),
            ("c.title_screen.props = { { model = 'a.vvx', at = { 1, 2 }, turn = 1 } }", "c.title_screen.props[1].turn (quarter turns anticlockwise) is called rotation now, in degrees clockwise: turn = 1 is rotation = 270"),
        ] {
            let e = conf(&format!("function conf(c) {line} end")).unwrap_err().to_string();
            assert!(e.contains(says) && e.contains("voxcade upgrade"), "{line}: {e}");
        }
        let cfg = conf("function conf(c) c.camera.rotates = 'free'; c.things.gem = { behavior = 'pickup', rotates = 'free' }; c.title_screen.props = { { model = 'a.vvx', at = { 1, 2 }, rotation = 270 } } end").unwrap();
        assert_eq!(cfg.camera.rotates, crate::config::Rotates::Free);
        assert_eq!((cfg.title_screen.props[0].rotation, cfg.title_screen.props[0].look()), (270, glam::IVec2::NEG_X));
        assert!(conf("function conf(c) c.title_screen.props = { { model = 'a.vvx', at = { 1, 2 }, rotation = 45 } } end").unwrap_err().to_string().contains("quarter turns"));
    }

    /// A kind without a behavior takes the engine's keys and no others: an
    /// unknown key is a mistake naming it, the table and a near name; a
    /// value of the wrong type says what it wants; more actions than the
    /// input holds are refused.
    #[test]
    fn an_unknown_key_of_a_kind_is_refused() {
        let kinds = |src: &str| crate::sim::things::defs(&conf(&format!("function conf(c) {src} end")).unwrap(), &[]).map(|_| ());
        for (line, says) in [
            ("c.things.gem = { modle = 'gem.vvx' }", "c.things.gem.modle: not a setting of a kind without a behavior"),
            ("c.things.gem = { modle = 'gem.vvx' }", "did you mean `model`?"),
            ("c.things.key = { keep = true }", "c.things.key.keep: not a setting"),
            ("c.things.gem = { famliy = 'loot' }", "did you mean `family`?"),
            ("c.things.hero = { hp = 'three' }", "c.things.hero.hp: a whole number of hits, not \"three\""),
            ("c.things.hero = { model = 3 }", "c.things.hero.model: a model's file name, in quotes, not 3"),
        ] {
            let e = kinds(line).unwrap_err().to_string();
            assert!(e.contains(says), "{line}: {e}");
        }
        kinds("c.things.a = { model = 'a.vvx', hp = 2, family = 'x', fixed = true, props = { worth = 1 } }; c.things.b = { like = 'a', spin = 90 }").unwrap();
        let many: Vec<String> = (1..=33).map(|i| format!("'a{i}'")).collect();
        let e = conf(&format!("function conf(c) c.actions = {{ {} }} end", many.join(", "))).unwrap_err().to_string();
        assert!(e.contains("33 actions") && e.contains("32 at most"), "{e}");
        let ok: Vec<String> = (1..=32).map(|i| format!("'a{i}'")).collect();
        conf(&format!("function conf(c) c.actions = {{ {} }} end", ok.join(", "))).unwrap();
    }

    /// The numbers `conf.lua` sets, by their paths as `config::RANGES`
    /// writes them, read from `config.rs` as the register reads it.
    fn numbers() -> Vec<String> {
        fn numeric(rust: &str) -> bool {
            matches!(rust, "f32" | "f64" | "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64")
        }
        fn walk(rust: &str, path: String, out: &mut Vec<String>) {
            let rust = rust.trim();
            if let Some(inner) = rust.strip_prefix("Option<").and_then(|r| r.strip_suffix('>')) {
                return walk(inner, path, out);
            }
            if let Some(inner) = rust.strip_prefix("Vec<").and_then(|r| r.strip_suffix('>')) {
                return walk(inner, format!("{path}[]"), out);
            }
            if let Some(inner) = rust.strip_prefix('[').and_then(|r| r.split_once(';')).map(|(t, _)| t) {
                return walk(inner, format!("{path}[]"), out);
            }
            if let Some(inner) = rust.strip_prefix("BTreeMap<String, ").and_then(|r| r.strip_suffix('>')) {
                return walk(inner, format!("{path}.*"), out);
            }
            if let Some(inner) = rust.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
                if inner.split(',').any(|t| numeric(t.trim())) {
                    out.push(format!("{path}[]"));
                }
                return;
            }
            if numeric(rust) {
                return out.push(path);
            }
            for k in crate::lua::api::conf_keys(rust) {
                walk(&k.rust, if path.is_empty() { k.key } else { format!("{path}.{}", k.key) }, out);
            }
        }
        let mut out = Vec::new();
        walk("GameConfig", String::new(), &mut out);
        out
    }

    /// Every number `conf.lua` sets has a range in `config::RANGES`, or is
    /// one of `config::FREE` that may be any number; and every range names
    /// a number there is.
    #[test]
    fn every_number_of_conf_lua_has_its_range() {
        let numbers = numbers();
        assert!(numbers.len() > 60, "{numbers:?}");
        for n in &numbers {
            assert!(crate::config::bound(n).is_some() || crate::config::FREE.contains(&n.as_str()), "{n}: a number of conf.lua without a range (config::RANGES) or a place among the free ones (config::FREE)");
        }
        for b in crate::config::RANGES {
            let general = b.key.replace("[3]", "[]");
            assert!(numbers.contains(&general), "{}: a range for no setting", b.key);
        }
        for f in crate::config::FREE {
            assert!(numbers.iter().any(|n| n == f), "{f}: free, but no setting");
        }
    }

    /// The line of `conf.lua` that sets the number at `key` (as `RANGES`
    /// writes it) to `v`, its siblings as a game would have them.
    fn setting(key: &str, v: &str) -> String {
        if let Some((list, field)) = key.strip_prefix("world.blocks.").and_then(|k| k.split_once("[].")) {
            return format!("c.world.blocks = {{ {list} = {{ {{ {field} = {v} }} }} }}");
        }
        if let Some((list, field)) = key.split_once("[].") {
            return format!("c.{list} = {{ {{ {field} = {v} }} }}");
        }
        if let Some(list) = key.strip_suffix("[]") {
            let n = if list.starts_with("world.size") || list.starts_with("light.") || list.ends_with("color") { 3 } else { 2 };
            return format!("c.{list} = {{ {} }}", vec![v; n].join(", "));
        }
        if let Some(list) = key.strip_suffix("[3]") {
            return format!("c.{list} = {{ 0.4, 0.6, {v} }}");
        }
        if let Some((map, field)) = key.split_once(".*.") {
            return format!("c.{map}.thing = {{ {field} = {v} }}");
        }
        if let Some(side) = key.strip_prefix("camera.follow_height.") {
            let other = if side == "below" { "above" } else { "below" };
            return format!("c.camera.follow_height = {{ {side} = {v}, {other} = 4 }}");
        }
        if let Some(field) = key.strip_prefix("world.blocks.") {
            return format!("c.world.blocks = {{ {field} = {v} }}");
        }
        format!("c.{key} = {v}")
    }

    /// A number of `conf.lua` out of its range is a mistake naming the
    /// setting, its value and the range, never a panic or a hang: each is
    /// set negative, zero, huge, not a number (0/0, 1/0) and text. At each
    /// end of its range the game loads, plays and draws (a negative
    /// `c.camera.view_width` panicked deep in the camera's sums; a world
    /// of 100000 by 100000 voxels went on for minutes without a word).
    #[test]
    fn a_number_out_of_its_range_is_a_plain_mistake() {
        use crate::config::RANGES;
        for b in RANGES {
            let named = format!("c.{}", b.key.split(['[', '*']).next().unwrap().trim_end_matches('.'));
            for v in ["-5", "0", "1e9", "0/0", "1/0", "'x'"] {
                let line = setting(b.key, v);
                match conf(&format!("function conf(c)\n  c.world.size = {{ 32, 32, 8 }}\n  {line}\nend\n")) {
                    Err(e) => assert!(e.to_string().contains(&named), "{line}: {e}"),
                    Ok(_) => assert!(v.parse::<f64>().is_ok_and(|n| b.holds(n)), "{line}: taken out of {}", b.words()),
                }
            }
        }
        let e = conf("function conf(c) c.camera.view_width = -5 end").unwrap_err().to_string();
        assert_eq!(e, "conf.lua: c.camera.view_width = -5: above 0, up to 65536");
        let e = conf("function conf(c) c.world.size = { 100000, 100000, 1000 } end").unwrap_err().to_string();
        assert!(e.starts_with("conf.lua: c.world.size[1] = 100000: from 1 to 65536"), "{e}");
        let e = conf("function conf(c) c.world.size = { 4096, 4096, 4096 } end").unwrap_err().to_string();
        assert!(e.starts_with("conf.lua: c.world.size: 4096 by 4096 by 4096 voxels is too big"), "{e}");
        let e = conf("function conf(c) c.light.sun = { 0.4, 0.6, 1 } end").unwrap_err().to_string();
        assert_eq!(e, "conf.lua: c.light.sun[3] = 1: from -1000 to below 0");
        for (line, says) in [("c.camera.fov = 0", "c.camera.fov = 0: from 10 to 100"), ("c.camera.pitch = 200", "c.camera.pitch = 200: from -89 to 90"), ("c.light.sky = { 0.1, 2, 0.1 }", "c.light.sky[2] = 2: from 0 to 1"), ("c.items.key = { stack = 0 }", "c.items.key.stack = 0: from 1 to 1000000")] {
            let e = conf(&format!("function conf(c) {line} end")).unwrap_err().to_string();
            assert!(e.contains(says), "{line}: {e}");
        }
        // At the ends of each range a game loads, plays and draws.
        let main = "function on_level_start() level:fill(0, 0, 0, 7, 7, 0, '#5a6e4a') end\nfunction on_draw(frame) frame:text('HI', 2, 2) end\n";
        let mut edges = Vec::new();
        for b in RANGES.iter().filter(|b| !b.key.starts_with("world.blocks")) {
            let low = if b.low_open { b.low + 0.25 } else { b.low };
            let high = if b.high_open { b.high - 0.25 } else { b.high };
            edges.extend([(b, low), (b, high)]);
        }
        let (done, heard) = std::sync::mpsc::channel();
        let count = edges.len();
        std::thread::spawn(move || {
            for (b, v) in edges {
                let dir = crate::scratch::Scratch::new("conf-edge");
                let line = setting(b.key, &v.to_string());
                let size = if b.key.starts_with("world.size") { "" } else { "c.world.size = { 32, 32, 8 }" };
                std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  {size}\n  {line}\nend\n")).unwrap();
                std::fs::write(dir.join("main.lua"), main).unwrap();
                match crate::session::Session::<crate::Voxcade>::load(&dir) {
                    // Ends that cannot all hold at once: a world as long
                    // each way as one may be, a sun level with the ground.
                    Err(e) => assert!(["c.world.size", "c.light.sun"].iter().any(|k| b.key.starts_with(&k[2..]) && e.to_string().contains(k)), "{line}: {e:#}"),
                    Ok(mut game) => {
                        for _ in 0..4 {
                            game.step(&crate::input::Input::default());
                            game.look();
                        }
                        let _ = game.compose(0.5, 16.0 / 9.0, crate::session::Scene::Play, None);
                        let _ = game.compose(0.5, 16.0 / 9.0, crate::session::Scene::Title { time: 3.0 }, None);
                        assert!(game.trouble().is_none(), "{line}: {:?}", game.trouble());
                    }
                }
                done.send(()).unwrap();
            }
        });
        for n in 0..count {
            heard.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| panic!("edge {n} of {count}: no answer in a minute (a panic above, or a hang)"));
        }
    }
}

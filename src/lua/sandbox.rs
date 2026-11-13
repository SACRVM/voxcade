//! The Lua state a game's scripts run in, made so they cannot step out of
//! the deterministic simulation: only the `table`, `string`, `math` and
//! `utf8` libraries (no `os`, `io`, `require`, `load`, `coroutine`,
//! `collectgarbage`), the maths of `sim::math` (the same bits on every
//! system), `math.random` drawing from the simulation's generator, keys
//! walked in order, no memory addresses in text, a budget of instructions
//! per call (an endless loop stops with a message) and a memory limit. The
//! Lua side of it is `prelude.lua`.

use std::cell::{Cell, RefCell};
use std::path::{Component, Path};
use std::rc::Rc;

use anyhow::{Context, Result};
use mlua::{Function, HookTriggers, IntoLuaMulti, Lua, LuaOptions, StdLib, Table, Value, VmState};

use crate::files::Files;

const PRELUDE: &str = include_str!("prelude.lua");
/// Instructions between two looks at the budget.
const EVERY: u32 = 10_000;
/// The budget of a tick, a frame's drawing, a question (in instructions),
/// and of the script's start, its first handlers or `conf`.
pub const CALL_BUDGET: u64 = 20_000_000;
pub const START_BUDGET: u64 = 200_000_000;
const MEMORY: usize = 256 << 20;

/// What the instruction budget counts: how much is left, and what runs.
#[derive(Default)]
struct Budget {
    left: Cell<i64>,
    what: Cell<&'static str>,
}

/// The files of the game folder its `main.lua` reads while it starts
/// (`read`) and the Lua files it runs (`require`): open only then, so
/// every run reads the same, and each file counts for the replays.
#[derive(Default)]
pub struct Reading {
    pub files: Option<Files>,
    pub open: bool,
    /// What was read, by name relative to the game folder.
    pub read: Vec<(String, Vec<u8>)>,
    /// The Lua files required, by path in the game folder, as tokens.
    pub modules: Vec<(String, Vec<u8>)>,
    /// Whether one of them names `music` or `on_beat`, and `daylight`.
    pub names_music: bool,
    pub names_daylight: bool,
    /// And whether one calls `frame:camera`.
    pub calls_camera: bool,
    /// The environment `main.lua` runs in: the required files run in it
    /// too, as Lua's modules share their globals.
    env: Option<Table>,
    /// What each required file gave back, by its name.
    given: Vec<(String, Value)>,
    /// The files being required, the outermost first (a loop is a mistake).
    loading: Vec<String>,
}

impl Reading {
    /// Open for a start of the game of these files.
    pub fn opened(files: &Files) -> Self {
        Self { files: Some(files.clone()), open: true, ..Self::default() }
    }

    /// Open again once the game runs (a scenario, a console line): in the
    /// game's environment, `require` giving the modules it has (`given`).
    pub fn reopened(files: &Files, env: Table, given: Vec<(String, Value)>) -> Self {
        Self { files: Some(files.clone()), open: true, env: Some(env), given, ..Self::default() }
    }

    /// What each required file gave back, by its name (kept for
    /// `reopened`).
    pub fn given(&self) -> Vec<(String, Value)> {
        self.given.clone()
    }

    /// Lets go of the Lua values it holds once the game has started.
    pub fn close(&mut self) {
        self.open = false;
        self.env = None;
        self.given.clear();
        self.loading.clear();
    }
}

pub struct Sandbox {
    pub lua: Lua,
    /// The safe functions and libraries scripts see.
    base: Table,
    tools: Table,
    budget: Rc<Budget>,
    pub reading: Rc<RefCell<Reading>>,
    /// What the script printed since last asked (the IDE's OUTPUT), the
    /// newest `PRINTED` lines.
    pub printed: Rc<RefCell<std::collections::VecDeque<String>>>,
}

/// How many printed lines wait to be asked for.
const PRINTED: usize = 500;

impl Sandbox {
    pub fn new() -> Result<Self> {
        // Safe: the debug library stays with the engine (tracebacks, the
        // hidden-state guard); scripts run in their own environment and
        // never see the globals it is in.
        let lua = unsafe { Lua::unsafe_new_with(StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8 | StdLib::DEBUG, LuaOptions::new()) };
        lua.set_memory_limit(MEMORY)?;
        let tools: Table = lua.load(PRELUDE).set_name("=prelude").eval()?;
        let g = lua.globals();
        let base = lua.create_table()?;
        for name in ["assert", "error", "ipairs", "pcall", "xpcall", "select", "tonumber", "type", "rawequal", "rawget", "rawlen", "getmetatable", "_VERSION"] {
            base.raw_set(name, g.raw_get::<Value>(name)?)?;
        }
        for name in ["pairs", "next", "tostring", "setmetatable", "rawset"] {
            base.raw_set(name, tools.raw_get::<Value>(name)?)?;
        }
        let tostring: Function = tools.raw_get("tostring")?;
        let printed = Rc::new(RefCell::new(std::collections::VecDeque::new()));
        let kept = printed.clone();
        base.raw_set(
            "print",
            lua.create_function(move |_, args: mlua::Variadic<Value>| {
                let parts: Vec<String> = args.iter().map(|a| tostring.call::<String>(a.clone()).unwrap_or_default()).collect();
                let line = parts.join("\t");
                eprintln!("[lua] {line}");
                let mut kept = kept.borrow_mut();
                if kept.len() == PRINTED {
                    kept.pop_front();
                }
                kept.push_back(line);
                Ok(())
            })?,
        )?;
        // A note of the engine's for OUTPUT, beside what the script printed
        // (an ask that did not count, with its line).
        let notes = printed.clone();
        lua.set_named_registry_value(
            "voxcade.note",
            lua.create_function(move |_, line: String| {
                eprintln!("[lua] {line}");
                let mut kept = notes.borrow_mut();
                if kept.len() == PRINTED {
                    kept.pop_front();
                }
                kept.push_back(line);
                Ok(())
            })?,
        )?;
        let readonly: Function = tools.raw_get("readonly")?;
        lua.set_named_registry_value("voxcade.readonly", readonly.clone())?;
        lua.set_named_registry_value("voxcade.engine_state", tools.raw_get::<Function>("engine_state")?)?;
        lua.set_named_registry_value("voxcade.is_looking", lua.create_function(|lua, ()| Ok(lua.named_registry_value::<bool>("voxcade.looking").unwrap_or(false)))?)?;
        // One handle per thing (`world::handle`), let go when nothing holds
        // it: a script cannot tell a new handle from an old one it let go.
        let handles = lua.create_table()?;
        let weak = lua.create_table()?;
        weak.raw_set("__mode", "v")?;
        handles.set_metatable(Some(weak))?;
        lua.set_named_registry_value("voxcade.handles", handles)?;
        base.raw_set("math", readonly.call::<Table>(maths(&lua, &g.raw_get::<Table>("math")?)?)?)?;
        for lib in ["string", "table", "utf8"] {
            base.raw_set(lib, readonly.call::<Table>(g.raw_get::<Table>(lib)?)?)?;
        }
        g.raw_set("debug_traceback", g.raw_get::<Table>("debug")?.raw_get::<Function>("traceback")?)?;
        g.raw_set("debug_getupvalue", g.raw_get::<Table>("debug")?.raw_get::<Function>("getupvalue")?)?;
        for gone in ["debug", "dofile", "loadfile", "load", "require", "collectgarbage", "os", "io", "package", "coroutine"] {
            g.raw_set(gone, Value::Nil)?;
        }
        let budget = Rc::new(Budget::default());
        let counter = budget.clone();
        lua.set_hook(HookTriggers::new().every_nth_instruction(EVERY), move |_, _| {
            let left = counter.left.get() - EVERY as i64;
            counter.left.set(left);
            if left < 0 {
                // Stays spent until the next call: a pcall cannot swallow it.
                return Err(mlua::Error::runtime(format!(
                    "{} ran over {} million instructions: an endless loop?",
                    counter.what.get(),
                    if counter.what.get() == "the start" { START_BUDGET } else { CALL_BUDGET } / 1_000_000
                )));
            }
            Ok(VmState::Continue)
        })?;
        let reading = Rc::new(RefCell::new(Reading::default()));
        let files = reading.clone();
        // `world:read` reads; the old global says where it went.
        lua.set_named_registry_value("voxcade.read", lua.create_function(move |lua, name: String| read(lua, &mut files.borrow_mut(), &name))?)?;
        base.raw_set("read", lua.create_function(|_, _: mlua::MultiValue| -> mlua::Result<()> { Err(mlua::Error::runtime("read(name) is world:read(name) now")) })?)?;
        let files = reading.clone();
        base.raw_set("require", lua.create_function(move |lua, name: String| require(lua, &files, &name))?)?;
        Ok(Self { lua, base, tools, budget, reading, printed })
    }

    /// Runs a script (its text, by its file name) in a fresh environment
    /// that sees the safe functions; returns the environment, holding the
    /// globals it made.
    pub fn run(&self, file: &str, src: &str) -> Result<Table> {
        super::lex::tokens(file, src)?;
        let env = self.lua.create_table()?;
        let meta = self.lua.create_table()?;
        meta.raw_set("__index", self.base.clone())?;
        meta.raw_set("__newindex", self.tools.raw_get::<Function>("set_global")?)?;
        env.set_metatable(Some(meta))?;
        self.reading.borrow_mut().env = Some(env.clone());
        self.start("the start", START_BUDGET);
        self.spent(self.lua.load(src).set_name(format!("@{file}")).set_environment(env.clone()).exec().map_err(script_error))?;
        Ok(env)
    }

    /// Runs a chunk in `env` (the game's, its globals frozen) and gives
    /// back what it returns: a scenario's table, a console line's values.
    /// `what` names the run in errors.
    pub fn run_in(&self, file: &str, src: &str, env: &Table, what: &'static str) -> Result<mlua::MultiValue> {
        super::lex::tokens(file, src)?;
        self.start(what, START_BUDGET);
        self.spent(self.lua.load(src).set_name(format!("@{file}")).set_environment(env.clone()).call::<mlua::MultiValue>(()).map_err(script_error))
    }

    /// Gives every script the global `name` (the engine's objects: `world`,
    /// `level`, …; `input` and `screen` change from call to call).
    pub fn provide(&self, name: &str, value: impl mlua::IntoLua) -> Result<()> {
        Ok(self.base.raw_set(name, value)?)
    }

    /// Freezes an environment's globals (writing one is a mistake from now
    /// on); returns the table that keeps them.
    pub fn freeze(&self, env: &Table) -> Result<Table> {
        let freeze: Function = self.tools.raw_get("freeze")?;
        Ok(freeze.call((env.clone(), self.base.clone()))?)
    }

    /// Calls a script's function with a fresh budget; `what` names the call
    /// in errors (`on_tick`, `on_draw`).
    pub fn call<R: mlua::FromLuaMulti>(&self, f: &Function, args: impl IntoLuaMulti, what: &'static str) -> Result<R> {
        self.start(what, CALL_BUDGET);
        self.spent(f.call(args).map_err(script_error))
    }

    /// Calls a script's function with the start's budget (`on_world_start`,
    /// `conf`).
    pub fn call_start<R: mlua::FromLuaMulti>(&self, f: &Function, args: impl IntoLuaMulti) -> Result<R> {
        self.start("the start", START_BUDGET);
        self.spent(f.call(args).map_err(script_error))
    }

    /// One of the prelude's tools (`plain`).
    pub fn tool(&self, name: &str) -> Result<Function> {
        Ok(self.tools.raw_get(name)?)
    }

    /// Calls one of the engine's own Lua functions (the state check) with a
    /// call's budget of its own, so a script's call before it is not
    /// charged.
    pub fn call_engine<R: mlua::FromLuaMulti>(&self, f: &Function, args: impl IntoLuaMulti) -> Result<R> {
        self.start("the state check", CALL_BUDGET);
        self.spent(f.call(args).map_err(script_error))
    }

    /// A call that ran over its budget fails, even when a `pcall` caught
    /// the guard's error inside.
    fn spent<R>(&self, r: Result<R>) -> Result<R> {
        let (left, what) = (self.budget.left.get(), self.budget.what.get());
        match r {
            Ok(_) if left < 0 => {
                let budget = if what == "the start" { START_BUDGET } else { CALL_BUDGET };
                anyhow::bail!("{what} ran over {} million instructions: an endless loop?", budget / 1_000_000)
            }
            r => r,
        }
    }

    fn start(&self, what: &'static str, budget: u64) {
        self.budget.what.set(what);
        self.budget.left.set(budget as i64);
    }
}

/// `read(name)`: a file of the game folder as text, while `main.lua`
/// starts.
fn read(lua: &Lua, files: &mut Reading, name: &str) -> mlua::Result<mlua::LuaString> {
    let (Some(game), true) = (files.files.clone(), files.open) else {
        return Err(mlua::Error::runtime(format!("world:read(\"{name}\"): only while main.lua starts (at its top, in on_world_start or the first on_level_start), so every run reads the same")));
    };
    if name.is_empty() || !Path::new(name).components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(mlua::Error::runtime(format!("world:read(\"{name}\"): a file in the game folder, by its path there (no .. and no / at the start)")));
    }
    let bytes = game.read(name).map_err(|e| mlua::Error::runtime(format!("world:read(\"{name}\"): {e}")))?;
    let name = name.replace('\\', "/");
    if !files.read.iter().any(|(n, _)| *n == name) {
        files.read.push((name, bytes.clone()));
    }
    lua.create_string(&bytes)
}

/// `require(name)`: the game's Lua file `name.lua` (dots between folders:
/// `lang.de` is `lang/de.lua`), run once in the game's environment while
/// it starts; what it gives back (true if nothing), the same every time.
fn require(lua: &Lua, files: &Rc<RefCell<Reading>>, name: &str) -> mlua::Result<Value> {
    let fail = |what: String| Err(mlua::Error::runtime(format!("require(\"{name}\"): {what}")));
    let (game, env, path) = {
        let reading = files.borrow();
        let (Some(game), Some(env), true) = (reading.files.clone(), reading.env.clone(), reading.open) else {
            return fail("only while the game starts (at the top of main.lua and of the files it requires), so every run runs the same code".into());
        };
        if let Some(stem) = name.strip_suffix(".lua") {
            return fail(format!("the file's name without .lua: require(\"{stem}\")"));
        }
        if name.is_empty() || !name.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')) {
            return fail("a Lua file of the game folder by its name, dots between folders (require(\"lang.de\") is lang/de.lua)".into());
        }
        if name == "main" || name == "conf" {
            return fail(format!("{name}.lua is the engine's to run"));
        }
        if let Some((_, given)) = reading.given.iter().find(|(n, _)| n == name) {
            return Ok(given.clone());
        }
        if reading.loading.iter().any(|n| n == name) {
            let chain: Vec<&str> = reading.loading.iter().map(String::as_str).chain([name]).collect();
            return fail(format!("a loop: {}", chain.join(" → ")));
        }
        (game, env, format!("{}.lua", name.replace('.', "/")))
    };
    let Ok(src) = game.read_to_string(&path) else {
        return fail(format!("no {path} in the game folder"));
    };
    let tokens = super::lex::tokens(&path, &src).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?;
    let (music, daylight) = (super::lex::names(&src, &["music", "on_beat"]), super::lex::names(&src, &["daylight"]));
    let camera = super::lex::calls(&src, "camera");
    files.borrow_mut().loading.push(name.to_string());
    // No borrow while it runs: it may read and require in turn.
    let ran = lua.load(&src).set_name(format!("@{path}")).set_environment(env).call::<Value>(());
    let mut reading = files.borrow_mut();
    reading.loading.pop();
    let given = match ran? {
        Value::Nil => Value::Boolean(true),
        other => other,
    };
    reading.given.push((name.to_string(), given.clone()));
    reading.modules.push((path, tokens));
    reading.names_music |= music;
    reading.names_daylight |= daylight;
    reading.calls_camera |= camera;
    Ok(given)
}

/// A script's error as one readable message: where (file:line) and what,
/// without mlua's wrapping.
pub fn script_error(e: mlua::Error) -> anyhow::Error {
    let text = match &e {
        mlua::Error::CallbackError { cause, traceback } => format!("{}\n{traceback}", cause_text(cause)),
        other => cause_text(other),
    };
    anyhow::anyhow!(text.trim_end().to_string())
}

fn cause_text(e: &mlua::Error) -> String {
    match e {
        mlua::Error::CallbackError { cause, .. } => cause_text(cause),
        mlua::Error::RuntimeError(text) | mlua::Error::MemoryError(text) => text.clone(),
        mlua::Error::SyntaxError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

/// The math library with `sim::math`'s functions (the same bits on every
/// system) where the system's would differ, and `random` drawing from the
/// simulation's generator (`math_random` in the registry, set by the game
/// for the calls that may change it).
fn maths(lua: &Lua, native: &Table) -> Result<Table> {
    let m = lua.create_table()?;
    for name in ["abs", "ceil", "floor", "fmod", "huge", "max", "maxinteger", "min", "mininteger", "modf", "pi", "sqrt", "tointeger", "type", "ult"] {
        m.raw_set(name, native.raw_get::<Value>(name)?)?;
    }
    let one = |f: fn(f64) -> f64| lua.create_function(move |_, x: f64| Ok(f(x)));
    m.raw_set("sin", one(libm::sin)?)?;
    m.raw_set("cos", one(libm::cos)?)?;
    m.raw_set("tan", one(libm::tan)?)?;
    m.raw_set("asin", one(libm::asin)?)?;
    m.raw_set("acos", one(libm::acos)?)?;
    m.raw_set("exp", one(libm::exp)?)?;
    m.raw_set("atan", lua.create_function(|_, (y, x): (f64, Option<f64>)| Ok(libm::atan2(y, x.unwrap_or(1.0))))?)?;
    m.raw_set(
        "log",
        lua.create_function(|_, (x, base): (f64, Option<f64>)| {
            Ok(match base {
                None => libm::log(x),
                Some(2.0) => libm::log2(x),
                Some(10.0) => libm::log10(x),
                Some(b) => libm::log(x) / libm::log(b),
            })
        })?,
    )?;
    m.raw_set("pow", lua.create_function(|_, (x, y): (f64, f64)| Ok(libm::pow(x, y)))?)?;
    m.raw_set(
        "random",
        lua.create_function(|lua, args: mlua::Variadic<i64>| {
            let range = match args.as_slice() {
                [] => None,
                &[n] => Some((1, n)),
                &[m, n] => Some((m, n)),
                _ => return Err(mlua::Error::runtime("math.random: (), (n) or (m, n)")),
            };
            if let Some((m, n)) = range
                && !(m <= n && (n - m) < u32::MAX as i64)
            {
                return Err(mlua::Error::runtime(format!("math.random: no numbers from {m} to {n}")));
            }
            super::world::with_rng(lua, |rng| match range {
                None => Value::Number(rng.f32() as f64),
                Some((m, n)) => Value::Integer(m + ((rng.next_u32() as u64 * (n - m + 1) as u64) >> 32) as i64),
            })
        })?,
    )?;
    m.raw_set("randomseed", lua.create_function(|_, ()| -> mlua::Result<()> { Err(mlua::Error::runtime("math.randomseed: the simulation's generator is seeded by the engine")) })?)?;
    Ok(m)
}

impl Sandbox {
    /// The function `name` among a script's globals (frozen or not).
    pub fn function(globals: &Table, name: &str) -> Result<Option<Function>> {
        match globals.raw_get::<Value>(name).context(name.to_string())? {
            Value::Function(f) => Ok(Some(f)),
            Value::Nil => Ok(None),
            other => anyhow::bail!("'{name}' should be a function, not a {}", other.type_name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `pairs` and `next` as they were before big tables kept their order
    /// (a sort with a Lua function on every call, `next` sorting on every
    /// step), word for word but for taking the table walked and the
    /// engine's keys as arguments: the order the new ones must give.
    const BEFORE: &str = r#"
local raw_next, raw_type, raw_rawget, raw_error, sort = next, type, rawget, error, table.sort
local function rank(k)
  local t = raw_type(k)
  if t == "number" then return 1 elseif t == "string" then return 2 elseif t == "boolean" then return 3 end
  raw_error("keys walked in order are numbers, strings or booleans, not a " .. t, 4)
end
local function before(a, b)
  local ra, rb = rank(a), rank(b)
  if ra ~= rb then return ra < rb end
  if ra == 3 then return (not a) and b end
  return a < b
end
local function keys_of(t, also)
  local keys, n = {}, 0
  for k in raw_next, t do
    rank(k)
    n = n + 1
    keys[n] = k
  end
  if also then
    for k in raw_next, also do
      if raw_rawget(t, k) == nil then
        n = n + 1
        keys[n] = k
      end
    end
  end
  sort(keys, before)
  return keys, n
end
local function ordered_pairs(t, source, also)
  local keys, n = keys_of(source, also)
  local i = 0
  return function()
    while i < n do
      i = i + 1
      local k = keys[i]
      local v = t[k]
      if v ~= nil then return k, v end
    end
    return nil
  end, t, nil
end
local function ordered_next(t, key, source, also)
  local keys, n = keys_of(source, also)
  local i = 0
  if key ~= nil then
    for j = 1, n do
      if keys[j] == key then i = j break end
    end
    if i == 0 then raw_error("next: the key is not in the table", 2) end
  end
  for j = i + 1, n do
    local v = t[keys[j]]
    if v ~= nil then return keys[j], v end
  end
  return nil
end
return ordered_pairs, ordered_next
"#;

    /// One round of the comparison, drawn from its seed: two tables alike
    /// (`a` walked as before, `b` as now) of keys of every sort the
    /// sandbox walks, changed alike between walks and while walking, and
    /// walked by `pairs` and `next` plain, through a read-only view and as
    /// an engine's state; an error saying what differs, else the size.
    /// The new functions take the old ones' extra arguments and leave them.
    const ROUND: &str = r#"
local seed, new_pairs, new_next, readonly, engine_state, old_pairs, old_next = ...
local state = seed
local function draw(n)
  state = state * 6364136223846793005 + 1442695040888963407
  return ((state >> 33) % n) + 1
end
local pieces = { "", "a", "A", "b", "_", "1", " ", "\0", "\255", "\195\169", "\195\164", "z", "aa", "Z0", "key" }
local specials = {
  0, -1, 1, 2, 3, 10, 100, -100, math.maxinteger, math.mininteger, math.maxinteger - 1, math.mininteger + 1,
  9007199254740992, 9007199254740993, -9007199254740993,
  0.5, -0.5, 1.5, 1e-300, -1e-300, -1e300, 1e300, math.huge, -math.huge, 2.0 ^ 63, -(2.0 ^ 63), 2.0 ^ 64,
  2.0 ^ 53, 2.0 ^ 53 + 2, 0.1, 3.0, -0.0, 9007199254740993.0,
  true, false,
}
local function key()
  local sort = draw(5)
  if sort == 1 then return draw(400) - 200
  elseif sort == 2 then return specials[draw(#specials)]
  elseif sort == 3 then return (draw(2000) - 1000) / 8
  end
  local s = ""
  for _ = 1, draw(5) - 1 do s = s .. pieces[draw(#pieces)] end
  return s
end
local function show(x)
  if type(x) == "table" then return "table" end
  return string.format("%q", x) .. ":" .. (math.type(x) or type(x))
end
-- The keys of `t` in order, as before: whom to take out while walking.
local function order_of(t)
  local out = {}
  for k in old_pairs(t, t) do out[#out + 1] = k end
  return out
end
local function walk_pairs(p, t, source, also, drop)
  local out = {}
  for k, v in p(t, source, also) do
    out[#out + 1] = show(k) .. "=" .. show(v)
    if drop and draw(5) == 1 then source[drop[draw(#drop)]] = nil end
  end
  return table.concat(out, " ")
end
local function next_of(nx, t, k, source, also)
  local key, v = nx(t, k, source, also)
  return key, v
end
local function walk_next(nx, t, source, also)
  local out, k = {}, nil
  repeat
    local v
    k, v = next_of(nx, t, k, source, also)
    if k ~= nil then out[#out + 1] = show(k) .. "=" .. show(v) end
  until k == nil
  return table.concat(out, " ")
end
-- The same walk before and now, from the same draws: what each gave or
-- said must be the same.
local function both(what, walk_old, walk_new)
  local at = state
  local ok_old, old = pcall(walk_old)
  state = at
  local ok_new, new = pcall(walk_new)
  if ok_old ~= ok_new or old ~= new then
    old, new = tostring(old), tostring(new)
    local i = 1
    while i <= #old and old:sub(i, i) == new:sub(i, i) do i = i + 1 end
    local from = math.max(1, i - 80)
    error(what .. " (seed " .. seed .. "), from byte " .. from .. ":\nbefore " .. old:sub(from, i + 120) .. "\nnow    " .. new:sub(from, i + 120), 0)
  end
end
local sizes = { 0, 1, 2, 5, 30, 63, 64, 65, 100, 300 }
local size = sizes[draw(#sizes)]
local a, b = {}, {}
local function put(k, v) a[k], b[k] = v, v end
for _ = 1, size do put(key(), draw(9)) end
local engine = { score = true, hp = true }
local function seven() return 7 end
local function never() return false end
local view_a, view_b = readonly(a), readonly(b)
local state_a = engine_state(a, engine, seven, put, never)
local state_b = engine_state(b, engine, seven, put, never)
for _ = 1, 8 do
  both("pairs", function() return walk_pairs(old_pairs, a, a) end, function() return walk_pairs(new_pairs, b) end)
  both("pairs of a view", function() return walk_pairs(old_pairs, view_a, a) end, function() return walk_pairs(new_pairs, view_b) end)
  both("pairs of a state", function() return walk_pairs(old_pairs, state_a, a, engine) end, function() return walk_pairs(new_pairs, state_b) end)
  if size <= 100 then
    both("next", function() return walk_next(old_next, a, a) end, function() return walk_next(new_next, b) end)
    both("next of a view", function() return walk_next(old_next, view_a, a) end, function() return walk_next(new_next, view_b) end)
    both("next of a state", function() return walk_next(old_next, state_a, a, engine) end, function() return walk_next(new_next, state_b) end)
  end
  -- `next` from a key the table lacks, and from each it has.
  local missing = key()
  if a[missing] == nil then
    both("next from a missing key", function() return next_of(old_next, a, missing, a) end, function() return next_of(new_next, b, missing) end)
  end
  for _, k in ipairs(order_of(a)) do
    both("next from a key", function() return show(next_of(old_next, a, k, a)) end, function() return show(next_of(new_next, b, k)) end)
    if draw(8) == 1 then break end
  end
  -- Changed alike between walks: keys out, keys in, as many out as in
  -- (the same count, other keys), values only, nothing; or keys taken out
  -- while walking.
  local change, order = draw(6), order_of(a)
  if change == 1 then
    for _ = 1, draw(5) do if #order > 0 then put(order[draw(#order)], nil) end end
  elseif change == 2 then
    for _ = 1, draw(5) do put(key(), draw(9)) end
  elseif change == 3 and #order > 0 then
    local gone = order[draw(#order)]
    put(gone, nil)
    local fresh = key()
    while a[fresh] ~= nil or fresh == gone do fresh = key() end
    put(fresh, 1)
  elseif change == 4 then
    for _, k in ipairs(order) do put(k, draw(9)) end
  elseif change == 5 and #order > 0 then
    both("pairs, keys taken out meanwhile", function() return walk_pairs(old_pairs, a, a, nil, order) end, function() return walk_pairs(new_pairs, b, b, nil, order) end)
  end
end
-- A key of no order among the others: the same mistake, at the same line.
local odd = ({ {}, function() end })[draw(2)]
put(odd, 1)
both("a key of no order", function() return walk_pairs(old_pairs, a, a) end, function() return walk_pairs(new_pairs, b) end)
both("a key of no order to next", function() return walk_next(old_next, a, a) end, function() return walk_next(new_next, b) end)
return size
"#;

    /// `pairs` and `next` keep a big table's order between walks and sort
    /// in C: over tables of every sort of key the sandbox walks (whole
    /// numbers to the ends of their range, fractions, infinities, strings
    /// with zeros and bytes over 127, booleans), changed between walks and
    /// while walking, plain, through a view and as an engine's state, they
    /// walk exactly as before, and say the same of keys they cannot order.
    #[test]
    fn pairs_walks_as_it_always_did() {
        let sandbox = Sandbox::new().unwrap();
        let lua = &sandbox.lua;
        let (old_pairs, old_next): (Function, Function) = lua.load(BEFORE).set_name("=before").eval().unwrap();
        let round = lua.load(ROUND).set_name("@round").into_function().unwrap();
        let tool = |name: &str| sandbox.tool(name).unwrap();
        let mut sizes = std::collections::BTreeSet::new();
        for seed in 1..=300i64 {
            let args = (seed, tool("pairs"), tool("next"), tool("readonly"), tool("engine_state"), old_pairs.clone(), old_next.clone());
            match sandbox.call_start::<i64>(&round, args) {
                Ok(size) => _ = sizes.insert(size),
                Err(e) => panic!("{e:#}"),
            }
        }
        assert_eq!(sizes.len(), 10, "every size drawn: {sizes:?}");
    }

    /// Walking a state of 5,000 keys with `pairs` every tick fits a tick's
    /// budget many times over: the keys are not sorted by a Lua function
    /// on every walk (that took about 80,000 instructions per walk, and
    /// 9 ms a tick in BunnyFarm's measure), only checked against the order
    /// kept.
    #[test]
    fn walking_a_big_table_is_cheap() {
        let sandbox = Sandbox::new().unwrap();
        let chunk = sandbox
            .lua
            .load(
                r#"
local pairs = ...
local big = {}
for i = 1, 5000 do big["key" .. (i * 7919 % 100003)] = i end
return function()
  local sum = 0
  for _ = 1, 50 do
    for _, v in pairs(big) do sum = sum + v end
  end
  return sum
end
"#,
            )
            .into_function()
            .unwrap();
        let walk: Function = sandbox.call_start(&chunk, sandbox.tool("pairs").unwrap()).unwrap();
        let sum: i64 = sandbox.call(&walk, (), "on_tick").unwrap();
        assert_eq!(sum, 50 * 5000 * 5001 / 2);
    }
}

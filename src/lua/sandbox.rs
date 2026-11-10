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

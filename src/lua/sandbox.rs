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

/// What the instruction budget counts: how much is left, and what runs;
/// and the script's line it last saw running (`file:line`), for a call
/// stopped by the budget or the memory limit, which Lua tells no line of.
#[derive(Default)]
struct Budget {
    left: Cell<i64>,
    what: Cell<&'static str>,
    place: RefCell<String>,
}

/// The script's line running now: the innermost frame of the game's own
/// files (not the prelude, mlua's Lua or C), as `main.lua:12`.
fn running_line(lua: &Lua) -> Option<String> {
    (0..64).map_while(|level| lua.inspect_stack(level, |d| (d.source().short_src.map(|s| s.to_string()), d.current_line()))).find_map(|(src, line)| {
        let (src, line) = (src?, line?);
        (!src.starts_with('[') && src != "prelude" && !src.starts_with("__mlua")).then(|| format!("{src}:{line}"))
    })
}

/// "main.lua:12: " for the line `budget` last saw running, if any.
fn at_place(budget: &Budget) -> String {
    let place = budget.place.borrow();
    if place.is_empty() { String::new() } else { format!("{place}: ") }
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

/// A line printed (or the engine's note): on the terminal, and kept for
/// OUTPUT, the newest `PRINTED`.
fn say(kept: &RefCell<std::collections::VecDeque<String>>, line: String) {
    eprintln!("[lua] {line}");
    let mut kept = kept.borrow_mut();
    if kept.len() == PRINTED {
        kept.pop_front();
    }
    kept.push_back(line);
}

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
                say(&kept, parts.join("\t"));
                Ok(())
            })?,
        )?;
        // A note of the engine's for OUTPUT, beside what the script printed
        // (an ask that did not count, with its line).
        let notes = printed.clone();
        lua.set_named_registry_value(
            "voxcade.note",
            lua.create_function(move |_, line: String| {
                say(&notes, line);
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
        lua.set_hook(HookTriggers::new().every_nth_instruction(EVERY), move |lua, _| {
            let left = counter.left.get() - EVERY as i64;
            counter.left.set(left);
            if let Some(place) = running_line(lua) {
                *counter.place.borrow_mut() = place;
            }
            if left < 0 {
                // Stays spent until the next call: a pcall cannot swallow it.
                return Err(mlua::Error::runtime(format!(
                    "{}{} ran over {} million instructions: an endless loop?",
                    at_place(&counter),
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
                anyhow::bail!("{}{what} ran over {} million instructions: an endless loop?", at_place(&self.budget), budget / 1_000_000)
            }
            // Lua's memory error tells no line: the one last seen running.
            Err(e) if e.to_string() == "not enough memory" => anyhow::bail!("{}", out_of_memory(&self.budget.place.borrow())),
            r => r,
        }
    }

    fn start(&self, what: &'static str, budget: u64) {
        self.budget.what.set(what);
        self.budget.left.set(budget as i64);
        self.budget.place.borrow_mut().clear();
    }
}

/// The mistake of a call that ran out of memory: at the line the budget
/// last saw running, if it saw one (Lua's memory error tells none: a
/// string or a table grown too big in one step leaves no line seen).
fn out_of_memory(place: &str) -> String {
    let mb = MEMORY >> 20;
    if place.is_empty() {
        format!("not enough memory: a game's Lua holds {mb} MB at most (Lua tells no line: something grew past it in one step, a string.rep or a table)")
    } else {
        format!("{place}: not enough memory: a game's Lua holds {mb} MB at most (the line last seen running)")
    }
}

/// The libraries and functions Lua has that a game's do not (the sandbox,
/// `Sandbox::new`), and what a game has instead.
const ABSENT: [(&str, &str); 10] = [
    ("os", "no clock and no files, so every run plays the same (world.tick counts the time)"),
    ("io", "no files (world:read reads one of the game's while it starts)"),
    ("debug", "the engine keeps it to itself"),
    ("coroutine", "a game's code runs through each call (keep where a long task stands in a state and go on in the next tick)"),
    ("load", "no code made from text (require(\"name\") runs a Lua file of the game while it starts)"),
    ("loadstring", "no code made from text (require(\"name\") runs a Lua file of the game while it starts)"),
    ("loadfile", "require(\"name\") runs a Lua file of the game while it starts"),
    ("dofile", "require(\"name\") runs a Lua file of the game while it starts"),
    ("collectgarbage", "the engine sweeps up by itself"),
    ("package", "require(\"name\") runs a Lua file of the game while it starts"),
];

/// Lua's words for a global that is not there ("attempt to index a nil
/// value (global 'os')"), said in a game's: a library the sandbox leaves
/// out, or `frame`, which only `on_draw` is given.
fn explained(text: &str) -> String {
    for verb in ["attempt to index a nil value (global '", "attempt to call a nil value (global '"] {
        let Some(at) = text.find(verb) else { continue };
        let rest = &text[at + verb.len()..];
        let Some(end) = rest.find("')") else { continue };
        let name = &rest[..end];
        let said = if name == "frame" {
            "frame is only there in on_draw: function on_draw(frame) is given it, for that picture alone".to_string()
        } else if let Some((_, instead)) = ABSENT.iter().find(|(n, _)| *n == name) {
            format!("{name} is not there in a game (the sandbox): {instead}")
        } else {
            continue;
        };
        return format!("{}{said}{}", &text[..at], &rest[end + 2..]);
    }
    text.to_string()
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

/// A script's error as one readable message: where (file:line) first, as
/// Lua's own errors have it, then what, in the script's words (`level:spawn:
/// x should be a number, not nil`), without mlua's wrapping or the engine's
/// Rust names; the traceback after it.
pub fn script_error(e: mlua::Error) -> anyhow::Error {
    anyhow::anyhow!(error_text(&e))
}

/// `script_error`'s text.
pub fn error_text(e: &mlua::Error) -> String {
    // The innermost call into the engine that failed: its cause, and the
    // script's stack when it did.
    let (mut cause, mut traceback) = (e, None);
    while let mlua::Error::CallbackError { cause: inner, traceback: t } = cause {
        (cause, traceback) = (&**inner, Some(t.as_str()));
    }
    let what = explained(&cause_text(cause));
    // mlua's own Lua (`__mlua_index`, the objects' fallbacks) is no place
    // of the script's: the script's line is the traceback's.
    let what = match engine_position(&what) {
        Some(rest) if traceback.is_some() => rest.to_string(),
        _ => what,
    };
    let text = match traceback {
        Some(t) => {
            let at = if has_position(&what) { String::new() } else { position_in(t).unwrap_or_default() };
            format!("{at}{what}\n{t}")
        }
        None => what,
    };
    // The traceback without the engine's own Lua (the prelude's wrappers,
    // mlua's fallbacks): only the script's frames and the calls into C.
    let lines: Vec<&str> = text.lines().filter(|l| !l.strip_prefix('\t').is_some_and(|l| l.starts_with("prelude:") || l.starts_with("__mlua"))).collect();
    lines.join("\n").trim_end().to_string()
}

fn cause_text(e: &mlua::Error) -> String {
    match e {
        mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => cause_text(cause),
        mlua::Error::RuntimeError(text) | mlua::Error::MemoryError(text) => text.clone(),
        mlua::Error::SyntaxError { message, .. } => message.clone(),
        mlua::Error::BadArgument { to, pos, name, cause } => argument_text(to.as_deref(), *pos, name.as_deref(), cause),
        mlua::Error::FromLuaConversionError { from, to, .. } => format!("{} wanted, not {}", rust_wants(to), lua_kind(from)),
        other => other.to_string(),
    }
}

/// Whether a message begins with where it happened (`main.lua:12: ...`,
/// `console:1: ...`).
fn has_position(text: &str) -> bool {
    let Some((file, rest)) = text.split_once(':') else { return false };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    !file.is_empty() && !file.contains(char::is_whitespace) && digits > 0 && rest[digits..].starts_with(':')
}

/// A message that begins at a place of the engine's own Lua (the prelude,
/// mlua's `__mlua_index`): what follows that place.
fn engine_position(text: &str) -> Option<&str> {
    if !has_position(text) || !(text.starts_with("prelude:") || text.starts_with("__mlua")) {
        return None;
    }
    let (_, rest) = text.split_once(':')?;
    rest.split_once(':').map(|(_, rest)| rest.trim_start())
}

/// Whether a traceback's line is one of the engine's own frames (`[C]`,
/// the prelude, mlua's Lua).
fn engine_frame(line: &str) -> bool {
    line.starts_with('[') || line.starts_with("prelude:") || line.starts_with("__mlua") || line.starts_with('(')
}

/// The script's place a traceback names first (`main.lua:12: `): not the
/// engine's own frames (`[C]`, the prelude).
fn position_in(traceback: &str) -> Option<String> {
    traceback.lines().filter_map(|l| l.strip_prefix('\t')).filter(|l| !engine_frame(l)).find(|l| has_position(l)).map(|l| {
        let (file, rest) = l.split_once(':').expect("has a position");
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        format!("{file}:{digits}: ")
    })
}

/// The script's name of an engine's object (mlua names the Rust type).
fn script_owner(rust: &str) -> Option<&'static str> {
    Some(match rust {
        "WorldView" => "world",
        "LevelView" => "level",
        "CameraView" => "level.camera",
        "ThingRef" => "thing",
        "AreaView" => "area",
        "InputView" => "input",
        "ScreenView" => "screen",
        "Frame" => "frame",
        "ControlsView" => "controls",
        "MusicView" => "music",
        _ => return None,
    })
}

/// A Lua value's kind as a message says it was given.
fn lua_kind(from: &str) -> String {
    match from {
        "nil" => "nil".into(),
        "boolean" => "true or false".into(),
        "userdata" => "an object of the engine's (a thing, level ...)".into(),
        "integer" | "number" => "a number".into(),
        "table" => "a table".into(),
        "string" => "a string".into(),
        other => format!("a {other}"),
    }
}

/// What a Rust type mlua converts into wants, in a script's words.
fn rust_wants(to: &str) -> &'static str {
    match to {
        "f32" | "f64" => "a number",
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" | "isize" => "a whole number",
        "String" | "BString" | "LuaString" | "string" => "a string",
        "bool" => "true or false",
        "table" | "Table" => "a table",
        "function" | "Function" => "a function",
        _ => "something else",
    }
}

/// What a type of the register wants, in a script's words.
fn register_wants(ty: &str) -> String {
    let one = |t: &str| -> String {
        match t.trim() {
            "number" => "a number".into(),
            "integer" => "a whole number".into(),
            "string" => "a string".into(),
            "boolean" => "true or false".into(),
            "table" => "a table".into(),
            "nil" => "nil".into(),
            "Way" => "a way { x =, y = }".into(),
            "Point" => "a point { x =, y =, z = }".into(),
            "Color" => "a color { r, g, b }".into(),
            "Thing" | "Player" => "a thing".into(),
            other if other.ends_with("[]") => "a list".into(),
            other => format!("a {}", other.to_lowercase()),
        }
    };
    let all: Vec<String> = ty.split('|').map(one).collect();
    all.join(" or ")
}

/// A bad argument to one of the engine's functions, said as the script
/// calls it: a method called with a dot, or which parameter wanted what.
fn argument_text(to: Option<&str>, pos: usize, name: Option<&str>, cause: &mlua::Error) -> String {
    let given = match cause {
        mlua::Error::FromLuaConversionError { from, to, .. } => Some((*from, to.as_str())),
        _ => None,
    };
    let method = to.and_then(|to| to.split_once('.')).and_then(|(owner, m)| Some((script_owner(owner)?, m)));
    let Some((of, m)) = method else {
        // A function of the libraries (math.random) or one the register
        // does not name.
        let call = to.map_or(String::new(), |to| format!("{to}: "));
        return match given {
            Some((from, rust)) => format!("{call}argument {pos} should be {}, not {}", rust_wants(rust), lua_kind(from)),
            None => format!("{call}argument {pos}: {}", cause_text(cause)),
        };
    };
    let call = format!("{of}:{m}");
    // The object itself, where a dot left it out: level.spawn(...).
    if pos == 1 || name == Some("self") {
        return format!("{call} is a method: call it with a colon, {call}(...), not {of}.{m}(...) (the colon passes {} itself)", of.rsplit('.').next().unwrap_or(of));
    }
    let item = super::api::items_of(of).find(|i| i.name == m);
    let params: &[(&str, &str)] = match item.map(|i| &i.what) {
        Some(super::api::What::Method { params, .. }) => params,
        _ => &[],
    };
    let shown: Vec<&str> = params.iter().map(|(n, _)| *n).collect();
    let call = if item.is_some() { format!("{call}({})", shown.join(", ")) } else { call };
    let (what, wants) = match params.get(pos - 2) {
        Some((n, ty)) => (n.trim_end_matches('?').to_string(), Some(register_wants(ty))),
        None => (format!("argument {}", pos - 1), None),
    };
    match given {
        Some((from, rust)) => {
            let wants = wants.unwrap_or_else(|| rust_wants(rust).to_string());
            // A number where a whole one was wanted: a fraction, or too big.
            let not = if matches!(from, "number" | "integer") && wants.contains("whole") { "a fraction or one that big".to_string() } else { lua_kind(from) };
            format!("{call}: {what} should be {wants}, not {not}")
        }
        None => format!("{call}: {what}: {}", cause_text(cause)),
    }
}

/// A whole number `math.random` takes: an integer, a float without a
/// fraction, or a string of one (as Lua's own takes them).
fn whole(v: &Value) -> mlua::Result<i64> {
    // 2^63 exactly, the first float past i64's range.
    let float = |f: f64| (f.fract() == 0.0 && f >= i64::MIN as f64 && f < -(i64::MIN as f64)).then_some(f as i64);
    let n = match v {
        Value::Integer(i) => Some(*i),
        Value::Number(f) => float(*f),
        Value::String(s) => s.to_str().ok().and_then(|s| s.trim().parse::<i64>().ok().or_else(|| s.trim().parse::<f64>().ok().and_then(float))),
        _ => None,
    };
    n.ok_or_else(|| {
        let shown = match v {
            Value::Number(f) => format!("{f}"),
            other => lua_kind(other.type_name()),
        };
        mlua::Error::runtime(format!("math.random: whole numbers, (), (n) or (m, n); not {shown}"))
    })
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
    // (Lua's own: one multiplication, the same bits on every system.)
    m.raw_set("rad", one(|x| x * (std::f64::consts::PI / 180.0))?)?;
    m.raw_set("deg", one(|x| x * (180.0 / std::f64::consts::PI))?)?;
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
        lua.create_function(|lua, args: mlua::Variadic<Value>| {
            let range = match args.as_slice() {
                [] => None,
                [n] => Some((1, whole(n)?)),
                [m, n] => Some((whole(m)?, whole(n)?)),
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

    /// Lists of every sort a game sorts, made the same in any Lua state
    /// from a seed: records with ties under the order function, numbers
    /// whole and fractions mixed, a few distinct values many times over,
    /// lists sorted already and backwards; each sorted and written out.
    const SORTS: &str = r#"
local seed, size, sort = ...
local state = seed
local function draw(n)
  state = state * 6364136223846793005 + 1442695040888963407
  return ((state >> 33) % n) + 1
end
local out = {}
local function show(t)
  for i = 1, #t do
    local v = t[i]
    if type(v) == "table" then v = v.k .. "/" .. v.id end
    out[#out + 1] = tostring(v) .. ":" .. (math.type(v) or type(v))
  end
  out[#out + 1] = "|"
end
local records, mixed, few, up, down = {}, {}, {}, {}, {}
for i = 1, size do
  records[i] = { k = draw(5), id = i }
  mixed[i] = draw(2) == 1 and draw(9) or draw(9) + 0.0
  few[i] = draw(3)
  up[i], down[i] = i, size - i
end
sort(records, function(a, b) return a.k < b.k end)
sort(mixed)
sort(few, function(a, b) return a > b end)
sort(up)
sort(down)
show(records) show(mixed) show(few) show(up) show(down)
return table.concat(out, " ")
"#;

    /// `table.sort` sorts as Lua's own whenever Lua's keeps its middle
    /// pivot (records with ties in the same order, whole numbers and
    /// fractions alike), so no game sorts otherwise than before; and the
    /// same every run, where Lua's would draw a pivot from the clock.
    #[test]
    fn table_sort_is_luas_without_the_clock() {
        let sandbox = Sandbox::new().unwrap();
        let ours: Function = sandbox.base.raw_get::<Table>("table").unwrap().get("sort").unwrap();
        let plain = Lua::new();
        let theirs: Function = plain.globals().raw_get::<Table>("table").unwrap().raw_get("sort").unwrap();
        for seed in 1..=40i64 {
            for size in [0, 1, 2, 3, 5, 50, 99, 100, 101, 257, 1000] {
                let a: String = sandbox.call_start(&sandbox.lua.load(SORTS).into_function().unwrap(), (seed, size, ours.clone())).unwrap();
                let b: String = plain.load(SORTS).call((seed, size, theirs.clone())).unwrap();
                assert!(a == b, "seed {seed}, {size} elements: sorted otherwise than Lua's own");
            }
        }
        // Lua's own reads the clock once a big list partitions badly: an
        // organ pipe of ties came out in 200 orders in 200 sorts with Lua
        // 5.4.9's (the same build mlua's vendored Lua is); ours in one, and
        // quickly (a middle pivot alone takes it quadratic, past a tick).
        let call = |src: &str| -> String {
            let f = sandbox.lua.load(src).set_name("@main.lua").set_environment(game_env(&sandbox)).into_function().unwrap();
            match sandbox.call::<String>(&f, (), "on_tick") {
                Ok(s) => s,
                Err(e) => format!("{e:#}"),
            }
        };
        let skewed = "local t, m = {}, 20000 for i = 1, m do t[i] = { k = (i <= m / 2 and i or m - i) // 3, id = i } end\ntable.sort(t, function(a, b) return a.k < b.k end)\nlocal out = {} for i = 1, #t do out[i] = t[i].id end return table.concat(out, ',')";
        let first = call(skewed);
        assert!(first.starts_with(|c: char| c.is_ascii_digit()), "{first}");
        for _ in 0..3 {
            assert!(call(skewed) == first, "a list of ties sorted otherwise from run to run");
        }
        // Its mistakes say where the game sorted.
        assert!(call("local t = {} for i = 1, 300 do t[i] = i % 7 end\ntable.sort(t, function(a, b) return a <= b end)").starts_with("main.lua:2: invalid order function for sorting"), "{}", call("local t = {} for i = 1, 300 do t[i] = i % 7 end\ntable.sort(t, function(a, b) return a <= b end)"));
        assert!(call("table.sort({ 1, 'a', 2 })").starts_with("main.lua:1: attempt to compare"), "{}", call("table.sort({ 1, 'a', 2 })"));
        assert!(call("table.sort({ 3, 1 }, 5)").contains("bad argument #2 to 'sort' (function expected, got number)"));
        assert!(call("local t = { 3, 1, 2 } table.sort(t, function(a, b) error('mine') end)").starts_with("main.lua:1: mine"));
    }

    /// Sorting 10,000 records by a key with an order function, and 10,000
    /// numbers whole and fractions mixed (sorted in Lua, not in C), fits a
    /// tick's budget.
    #[test]
    fn sorting_ten_thousand_fits_a_tick() {
        let sandbox = Sandbox::new().unwrap();
        let src = "local t, u = {}, {} for i = 1, 10000 do t[i] = { k = (i * 7919) % 10007 } u[i] = i % 2 == 0 and (i * 31) % 10007 or ((i * 17) % 10007) + 0.5 end\ntable.sort(t, function(a, b) return a.k < b.k end)\ntable.sort(u)\nreturn t[1].k + u[1]";
        let f = sandbox.lua.load(src).set_environment(game_env(&sandbox)).into_function().unwrap();
        sandbox.call::<f64>(&f, (), "on_tick").unwrap();
    }

    /// An environment as a game's: the safe functions only.
    fn game_env(sandbox: &Sandbox) -> Table {
        let env = sandbox.lua.create_table().unwrap();
        let meta = sandbox.lua.create_table().unwrap();
        meta.raw_set("__index", sandbox.base.clone()).unwrap();
        env.set_metatable(Some(meta)).unwrap();
        env
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

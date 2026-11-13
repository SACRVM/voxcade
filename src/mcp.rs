//! `voxcade mcp`: the engine's tools for an agent (Claude Code and the
//! like) over MCP, JSON-RPC on stdin and stdout, a message a line. Each
//! tool runs the program itself without a window (`shot`, `check`,
//! `inspect`), so nothing a game prints reaches the protocol; `api` reads
//! the Lua API's register (`lua::api`). The IDE's tools (`view_state`,
//! `open`, `game_frame`, `errors`, `play`, `stop`) go to the IDE running on
//! the game, through its bridge (`ide::bridge`). With `--remote` two more
//! (`console_send`, `console_read`) type into the IDE's console and read
//! it, for another session testing the agent there: they run commands in
//! the user's shell, so the game's own agent never gets them (the IDE's
//! `.mcp.json` has no `--remote`, and the IDE answers them only when it
//! was started with `--remote` too). So do the tools that use the IDE as
//! a person does, `windows`, `window_shot`, `pointer`, `scroll` and `keys`
//! (`ide::remote`): a picture of any window and synthetic mouse and
//! keyboard input for it. A tool's game is the
//! folder the server runs in unless named. Claude Code takes it with a
//! `.mcp.json` in the game folder (the IDE's MCP button writes one), or
//! `claude mcp add voxcade -- /path/to/voxcade mcp`.

use std::path::{Path, PathBuf};

use std::io::{BufRead, Write};
use std::process::Command;

use anyhow::Result;
use serde_json::{Value, json};

/// The protocol versions it speaks, the newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Answers requests from stdin until it closes; `remote`: with the console
/// tools.
pub fn serve(remote: bool) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => answer_as(&msg, remote),
            Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("not JSON: {e}") } })),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// The reply to one message of a server without `--remote`.
#[cfg(test)]
fn answer(msg: &Value) -> Option<Value> {
    answer_as(msg, false)
}

/// The reply to one message (none to a notification).
fn answer_as(msg: &Value, remote: bool) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match msg.get("method").and_then(Value::as_str).unwrap_or("") {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked.filter(|v| VERSIONS.contains(v)).unwrap_or(VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "voxcade", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Voxcade games are folders: conf.lua (settings), main.lua (on_world_start, on_level_start, on_tick, on_draw), models and levels. \
                    Look with shot, test with run and things, make sure replays still hold with check; api lists the Lua API. \
                    With the IDE open on the game (voxcade ide): view_state says what the user sees, open shows them a file and line, \
                    play and stop run the game there, game_frame is what they see of it, errors what stops it.",
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools(remote) })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(name, &args, remote) {
                Ok(content) => json!({ "content": content }),
                Err(e) => json!({ "content": [text(&e)], "isError": true }),
            })
        }
        other => Err(json!({ "code": -32601, "message": format!("no method {other}") })),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

fn text(s: &str) -> Value {
    json!({ "type": "text", "text": s })
}

/// A schema's property.
fn prop(kind: &str, about: &str) -> Value {
    json!({ "type": kind, "description": about })
}

fn tools(remote: bool) -> Value {
    let game = prop("string", "The game folder (with conf.lua and main.lua), e.g. games/sokovox; by default the folder this server runs in");
    let at_ide = || json!({ "type": "object", "properties": { "game": game } });
    let frames = |default: u32| prop("integer", &format!("Ticks to play, 60 a second (default {default})"));
    let input = prop("string", "Keys held, segment by segment: 'keys:ticks;...', keys joined by '+' from the directions (left right up down), the game's actions and none. Example: 'right+fire:40;none:20'");
    let others = json!({ "type": "array", "items": { "type": "string" }, "description": "In a game of several players (c.players), the keys of the players after the first, seat 2 first, as input is written; 'away:ticks' while nobody sits there (a seat taken when its keys begin). Example: ['away:60;left:40', 'right:100']" });
    let scenario = prop("string", "Begin the run from this scenario (scenarios/NAME.lua: a late situation set up in code; the scenarios tool lists them)");
    let lines = json!({ "type": "array", "items": { "type": "string" }, "description": "Lines of Lua run in the game before its first tick, after the scenario, as the console runs them (e.g. 'player.state.score = 5000', 'require(\"wolf\").night()'); each one's answer is reported" });
    let mut all = json!([
        {
            "name": "shot",
            "description": "Plays the game for some ticks without a window and returns the last frame as a PNG, with what the engine reports (the player, things, sounds, timings). See what the player sees.",
            "inputSchema": { "type": "object", "properties": {
                "game": game, "frames": frames(90), "input": input, "players": others, "scenario": scenario, "do": lines,
                "screen": prop("string", &format!("A menu or screen instead of the game: {}", crate::headless::SCREENS.iter().map(|(name, what)| format!("{name} ({what})")).collect::<Vec<_>>().join(", "))),
                "at": prop("string", "Where the player starts: 'x,y' in voxels"),
                "pad": prop("string", "Hints for a gamepad instead of the keyboard: xbox, nintendo or playstation"),
                "size": prop("string", "The picture's size, 'WxH' (default 960x540)"),
            } },
        },
        {
            "name": "check",
            "description": "Plays every replay in the game's replays/ folder and says whether each runs as recorded, the state its run ended in too (one recorded on other data names the files that changed); it also catches state a script keeps outside world.state. Run it after changing the game's rules.",
            "inputSchema": { "type": "object", "properties": { "game": game } },
        },
        {
            "name": "run",
            "description": "Plays the game for some ticks without drawing and reports how it stands: tick, outcome, score, the engine's report, the sounds made, and the script's error if it stopped (file, line, tick).",
            "inputSchema": { "type": "object", "properties": { "game": game, "frames": frames(60), "input": input, "players": others, "scenario": scenario, "do": lines } },
        },
        {
            "name": "things",
            "description": "Plays the game for some ticks and returns as JSON the level's things (id, kind, x, y, z, state), the player (its state, where it stands), world.state, level.state and the level it is in.",
            "inputSchema": { "type": "object", "properties": { "game": game, "frames": frames(60), "input": input, "players": others, "scenario": scenario, "do": lines } },
        },
        {
            "name": "scenarios",
            "description": "The game's scenarios (scenarios/NAME.lua): runs begun late, set up in code, to try what happens there without playing to it (a day late in a farm year, the last room, a wolf at the fence). shot, run and things begin from one with scenario.",
            "inputSchema": { "type": "object", "properties": { "game": game } },
        },
        {
            "name": "api",
            "description": "The Lua API: world, level, player, things, input, controls, screen, on_draw's frame, and the functions a game defines (conf, on_world_start, on_level_start, on_tick, on_draw and the rest).",
            "inputSchema": { "type": "object", "properties": { "name": prop("string", "Only the names containing this") } },
        },
        {
            "name": "view_state",
            "description": "What the user sees in the Voxcade IDE open on the game: the open file, the cursor (line, column) and what is selected, whether it has changes not yet saved, the line Lua cannot read; or that the game is being played there, and what stops it.",
            "inputSchema": at_ide(),
        },
        {
            "name": "open",
            "description": "Shows the user a file of the game in the IDE's main area, at a line if given (a script's error, a place to look at).",
            "inputSchema": { "type": "object", "properties": {
                "path": prop("string", "The file, its path in the game folder (main.lua, hero.vvx)"),
                "line": prop("integer", "The line to go to"),
                "game": game,
            }, "required": ["path"] },
        },
        {
            "name": "game_frame",
            "description": "The game being played in the IDE as the user sees it now (PNG). Nothing while no game plays there: play starts one, shot plays without the IDE.",
            "inputSchema": at_ide(),
        },
        {
            "name": "errors",
            "description": "What stops the game played in the IDE (a script's error: file, line, tick) and the line of the open file Lua cannot read.",
            "inputSchema": at_ide(),
        },
        {
            "name": "play",
            "description": "Plays the game in the IDE's main area as its PLAY button does (the text being changed is saved first).",
            "inputSchema": at_ide(),
        },
        {
            "name": "stop",
            "description": "Ends the game played in the IDE.",
            "inputSchema": at_ide(),
        },
        {
            "name": "eval",
            "description": "Runs a line of Lua in the game being played in the IDE, between two ticks, as its console does (states, the game's functions by require; a line asks a level change, a save, at once) and returns its answer; the user sees it in OUTPUT, and it is recorded with the run.",
            "inputSchema": { "type": "object", "properties": { "line": prop("string", "The Lua to run, e.g. 'player.state.score = 5000' or 'world.state.day'"), "game": game }, "required": ["line"] },
        },
    ]);
    if remote {
        all.as_array_mut().expect("a list").extend([
            json!({
                "name": "console_send",
                "description": "Types text into the console of the Voxcade IDE open on the game (the user's shell, or Claude Code, the game's agent, running in it) as if pasted, then presses Enter unless enter is false. This runs commands in the user's shell: for testing the IDE's agent only. Read what it says with console_read.",
                "inputSchema": { "type": "object", "properties": {
                    "text": prop("string", "What to type: a shell command, or a prompt for the agent"),
                    "enter": prop("boolean", "Press Enter after it (default true), so a shell runs it and Claude Code takes it; false leaves it in the line"),
                    "game": game,
                }, "required": ["text"] },
            }),
            json!({
                "name": "console_read",
                "description": "What the console of the Voxcade IDE open on the game shows now, as plain text: the last lines, scrollback included, without blank lines at the end.",
                "inputSchema": { "type": "object", "properties": { "lines": prop("integer", "How many lines from the end (default 200, at most 2000)"), "game": game } },
            }),
            json!({
                "name": "windows",
                "description": "The windows of the Voxcade IDE open on the game: the main one and each editor window open, with an id, the title, the size in pixels (what window_shot's picture has, and the pointer's x and y count in) and whether it has the keyboard.",
                "inputSchema": { "type": "object", "properties": { "game": game } },
            }),
            json!({
                "name": "window_shot",
                "description": "A picture (PNG) of a window of the Voxcade IDE as it is drawn now, the whole window: lists, bars, the main area, the console, popups, a tooltip if the pointer rests on something (after a pointer move it waits for the tooltip's delay). Shows the state after any input sent before it, and an arrow where the synthetic pointer is.",
                "inputSchema": { "type": "object", "properties": { "window": prop("integer", "The window's id from windows (default: the main one)"), "game": game } },
            }),
            json!({
                "name": "pointer",
                "description": "Moves, clicks or drags a synthetic mouse in a window of the Voxcade IDE, as a person would (the real mouse is not touched). x and y are pixels in that window's window_shot picture. A click is a move, a press and a release; a drag presses at x,y, moves in steps and releases at to_x,to_y. Answers once the IDE has taken it in; look with window_shot.",
                "inputSchema": { "type": "object", "properties": {
                    "x": prop("number", "Pixels from the left of the window's picture"),
                    "y": prop("number", "Pixels from the top of the window's picture"),
                    "action": json!({ "type": "string", "enum": ["move", "click", "double_click", "press", "release", "drag"], "description": "What the pointer does (default click). move rests it there (a tooltip shows after its delay)" }),
                    "button": json!({ "type": "string", "enum": ["left", "right", "middle"], "description": "The button (default left)" }),
                    "modifiers": json!({ "type": "array", "items": { "type": "string", "enum": ["shift", "ctrl", "alt"] }, "description": "Keys held through it" }),
                    "to_x": prop("number", "A drag's end, pixels from the left"),
                    "to_y": prop("number", "A drag's end, pixels from the top"),
                    "window": prop("integer", "The window's id from windows (default: the main one)"),
                    "game": game,
                }, "required": ["x", "y"] },
            }),
            json!({
                "name": "scroll",
                "description": "Turns the synthetic mouse's wheel at a point of a window of the Voxcade IDE (lists, text, the console's scrollback, an editor's view).",
                "inputSchema": { "type": "object", "properties": {
                    "x": prop("number", "Pixels from the left of the window's picture"),
                    "y": prop("number", "Pixels from the top of the window's picture"),
                    "dy": prop("number", "Wheel steps, positive up (content moves down), negative down"),
                    "dx": prop("number", "Wheel steps sideways, positive right"),
                    "window": prop("integer", "The window's id from windows (default: the main one)"),
                    "game": game,
                }, "required": ["x", "y", "dy"] },
            }),
            json!({
                "name": "keys",
                "description": "Presses a key or chord in a window of the Voxcade IDE, or types text, as a keyboard would (the real keyboard is not touched). A letter, digit or space also types its character, so it goes into a text field; Ctrl or Alt chords type nothing. Answers once the IDE has taken it in; look with window_shot.",
                "inputSchema": { "type": "object", "properties": {
                    "keys": prop("string", "One key or chord: Escape, F5, Enter, Tab, Up, a, ctrl+s, ctrl+shift+z (Enter, Escape, Tab, Space, Backspace, Delete, Home, End, PageUp, PageDown, arrows, F1 to F12, letters, digits, punctuation by name: plus, minus, comma, period, slash)"),
                    "text": prop("string", "Instead: text to type (text fields, the console); a line break presses Enter"),
                    "hold_ms": prop("integer", "With keys: how long the key is held down, also for the game being played (which reads the keyboard's state); at most 10000; answers after it was let go"),
                    "window": prop("integer", "The window's id from windows (default: the main one)"),
                    "game": game,
                } },
            }),
        ]);
    }
    all
}

/// The IDE's tools, answered by the IDE running on the game.
const IDE_TOOLS: [&str; 7] = ["view_state", "open", "game_frame", "errors", "play", "stop", "eval"];

/// The IDE's console and window tools, listed and passed on only with
/// `--remote`.
const REMOTE_TOOLS: [&str; 7] = ["console_send", "console_read", "windows", "window_shot", "pointer", "scroll", "keys"];

/// A string argument, if given.
fn arg<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Value::as_str)
}

/// Runs a tool; its content, or what went wrong.
fn call(name: &str, args: &Value, remote: bool) -> Result<Vec<Value>, String> {
    call_with(name, args, remote, crate::ide::bridge::lock_path)
}

/// Runs a tool, an IDE's through the lock `lock_for` gives for its game
/// folder.
fn call_with(name: &str, args: &Value, remote: bool, lock_for: impl Fn(&Path) -> PathBuf) -> Result<Vec<Value>, String> {
    // The game named, else the folder the server runs in if it is one.
    let folder = || arg(args, "game").map(str::to_string).or_else(|| Path::new("conf.lua").is_file().then(|| ".".to_string()));
    let game = || folder().ok_or_else(|| "which game? (game: the game folder; this server does not run in one)".to_string());
    if IDE_TOOLS.contains(&name) || (remote && REMOTE_TOOLS.contains(&name)) {
        return crate::ide::bridge::call(&lock_for(Path::new(&game()?)), name, args);
    }
    let frames = |default: u64| args.get("frames").and_then(Value::as_u64).unwrap_or(default).to_string();
    let input = || arg(args, "input").unwrap_or("").to_string();
    // How the run begins: a scenario, lines of Lua (one, or a list).
    let begin = || {
        let mut flags: Vec<String> = arg(args, "scenario").map(|s| vec!["--scenario".into(), s.into()]).unwrap_or_default();
        let lines: Vec<&str> = match args.get("do") {
            Some(Value::String(line)) => vec![line.as_str()],
            Some(Value::Array(all)) => all.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        for line in lines {
            flags.extend(["--do".into(), line.into()]);
        }
        // The players after the first, seat 2 first.
        let others = args.get("players").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
        for (i, keys) in others.enumerate() {
            flags.extend([format!("--input{}", i + 2), keys.into()]);
        }
        flags
    };
    match name {
        "shot" => {
            let out = std::env::temp_dir().join(format!("voxcade-mcp-{}-{}.png", std::process::id(), next()));
            let mut command = vec!["shot".into(), game()?, "--frames".into(), frames(90), "--input".into(), input()];
            command.extend(begin());
            for (flag, key) in [("--screen", "screen"), ("--at", "at"), ("--pad", "pad")] {
                if let Some(v) = arg(args, key) {
                    command.extend([flag.to_string(), v.to_string()]);
                }
            }
            command.extend(["--size".into(), arg(args, "size").unwrap_or("960x540").into(), "--out".into(), out.display().to_string()]);
            let (ok, said) = run(&command)?;
            let png = std::fs::read(&out).ok();
            let _ = std::fs::remove_file(&out);
            match (ok, png) {
                (_, Some(png)) => Ok(vec![text(&said), json!({ "type": "image", "data": base64(&png), "mimeType": "image/png" })]),
                (_, None) => Err(said),
            }
        }
        "check" => match run(&["check".into(), game()?])? {
            (true, said) => Ok(vec![text(&said)]),
            (false, said) => Err(said),
        },
        "run" | "things" => {
            let mut command = vec!["inspect".into(), game()?, "--frames".into(), frames(60), "--input".into(), input()];
            command.extend(begin());
            let (ok, said) = run(&command)?;
            let start = said.find("\n{").map_or(0, |i| i + 1);
            let state: Value = match serde_json::from_str::<Value>(&said[start..]) {
                // (A halted game ends `inspect` with 1 and still prints its
                // state: shown as the game stopped, not as a failed call.)
                Ok(v) if ok || !v["halted"].is_null() => v,
                _ => return Err(said),
            };
            if name == "things" {
                let mut shown = state["game"].clone();
                shown["tick"] = state["tick"].clone();
                return Ok(vec![text(&serde_json::to_string_pretty(&shown).unwrap_or_default())]);
            }
            let mut lines = vec![format!("tick {} | {} | score {}", state["tick"], state["outcome"].as_str().unwrap_or("?"), state["score"])];
            lines.extend(state["report"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
            if let Some(sounds) = state["sounds"].as_object().filter(|s| !s.is_empty()) {
                lines.push(format!("sounds: {}", sounds.iter().map(|(n, c)| format!("{n} {c}")).collect::<Vec<_>>().join(", ")));
            }
            if let Some(error) = state["halted"].as_str() {
                lines.push(format!("stopped by an error:\n{error}"));
            }
            let printed = said[..start].trim();
            if !printed.is_empty() {
                lines.push(format!("printed:\n{printed}"));
            }
            Ok(vec![text(&lines.join("\n"))])
        }
        "api" => Ok(vec![text(&crate::lua::api::text(arg(args, "name")))]),
        "scenarios" => match run(&["scenarios".into(), game()?])? {
            (true, said) => Ok(vec![text(&said)]),
            (false, said) => Err(said),
        },
        other => Err(format!("no tool {other} (shot, check, run, things, scenarios, api, {}{})", IDE_TOOLS.join(", "), if remote { format!(", {}", REMOTE_TOOLS.join(", ")) } else { String::new() })),
    }
}

/// Runs this program with `args`; whether it went well, and what it said
/// (its errors after its output).
fn run(args: &[String]) -> Result<(bool, String), String> {
    let exe = crate::cli::program().map_err(|e| format!("finding the program: {e}"))?;
    let out = Command::new(exe).args(args).output().map_err(|e| format!("running {}: {e}", args.join(" ")))?;
    let mut said = String::from_utf8_lossy(&out.stderr).into_owned();
    said += &String::from_utf8_lossy(&out.stdout);
    Ok((out.status.success(), said.trim().to_string()))
}

/// A number for each picture in this run (their files' names).
fn next() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16 | u32::from(*chunk.get(1).unwrap_or(&0)) << 8 | u32::from(*chunk.get(2).unwrap_or(&0));
        for (k, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if k <= chunk.len() { ABC[(n >> shift) as usize & 63] as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_as_everyone_writes_it() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    /// The handshake, the list of tools and a call, as a client sends them;
    /// a notification gets no answer.
    #[test]
    fn it_speaks_mcp() {
        let init = answer(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } })).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "voxcade");
        assert!(answer(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let list = answer(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["shot", "check", "run", "things", "scenarios", "api", "view_state", "open", "game_frame", "errors", "play", "stop", "eval"]);
        let api = answer(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "api", "arguments": { "name": "spawn" } } })).unwrap();
        assert!(api["result"]["content"][0]["text"].as_str().unwrap().contains("level:spawn(kind: string"));
        let wrong = answer(&json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "fly", "arguments": {} } })).unwrap();
        assert_eq!(wrong["result"]["isError"], true);
        let unknown = answer(&json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    /// The console's tools are listed only for `--remote`, and a server
    /// without it passes none on.
    #[test]
    fn the_console_tools_come_only_with_remote() {
        let list = |remote| {
            let list = answer_as(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }), remote).unwrap();
            list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>()
        };
        let (plain, remote) = (list(false), list(true));
        assert!(!plain.iter().any(|n| n.starts_with("console_")), "{plain:?}");
        assert_eq!(remote[..plain.len()], plain[..], "the others as they are");
        assert_eq!(remote[plain.len()..], ["console_send", "console_read", "windows", "window_shot", "pointer", "scroll", "keys"]);
        for name in &remote[plain.len()..] {
            let never = |_: &Path| -> PathBuf { panic!("no bridge call without --remote") };
            let no = call_with(name, &json!({ "game": "games/voxkeep" }), false, never).unwrap_err();
            assert!(no.contains(&format!("no tool {name}")), "{no}");
        }
        let never = |_: &Path| -> PathBuf { panic!("no bridge call without --remote") };
        let no = call_with("console_send", &json!({ "game": "games/voxkeep", "text": "ls" }), false, never).unwrap_err();
        assert!(no.contains("no tool console_send"), "{no}");
        let said = answer_as(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "console_read", "arguments": { "game": "games/voxkeep" } } }), false).unwrap();
        assert_eq!(said["result"]["isError"], true);
    }

    /// The IDE's tools go to the IDE running on the game, through its
    /// bridge, and come back as its answer; without an IDE they say so.
    #[test]
    fn mcp_forwards_to_a_running_ide() {
        let (bridge, lock, _scratch) = crate::ide::bridge::tests::running("mcp");
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ide = std::thread::spawn({
            let done = done.clone();
            move || {
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    crate::ide::bridge::tests::answer_calls(&bridge);
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
        });
        let args = json!({ "game": "games/voxkeep" });
        let seen = call_with("view_state", &args, false, |_| lock.clone()).unwrap();
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(seen[0]["text"], "main.lua open");
        let none = call_with("view_state", &args, false, |_| std::env::temp_dir().join("voxcade-no-ide.lock")).unwrap_err();
        assert!(none.contains("not running"), "{none}");
        ide.join().unwrap();
    }
}

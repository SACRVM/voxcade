//! The Lua door's rules, each tried on a small game in a temporary folder
//! with a world made in code.

use std::path::{Path, PathBuf};

/// The program's Lua games: with the shooter kit when they ask for it.
type LuaGame = crate::Voxcade;
use crate::game::Game;
use crate::input::Input;
use crate::session::{Scene, Session};

const CONF: &str = r#"
function conf(c)
  c.world.size = { 64, 64, 16 }
  c.player.spawn = { 32, 32 }
  c.actions = { "jump", "fire" }
  c.things.miner = {}
  c.things.gem = { hp = 2 }
end
"#;

/// A game folder with this `main.lua` (and the conf above).
fn folder(name: &str, main: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxcade-lua-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), CONF).unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    dir
}

fn game(name: &str, main: &str) -> Session<LuaGame> {
    game_in(&folder(name, main))
}

fn game_in(dir: &Path) -> Session<LuaGame> {
    Session::<LuaGame>::load(dir).unwrap()
}

/// The first text `on_draw` put on the HUD.
fn first_text(g: &Session<LuaGame>) -> String {
    g.game.overlay.borrow().iter().find_map(|c| match c { super::world::Cmd::Text { text, .. } => Some(text.clone()), _ => None }).expect("a text on the HUD")
}

fn steps(game: &mut Session<LuaGame>, n: usize) {
    for _ in 0..n {
        game.step(&Input::default());
    }
}

fn halted(game: &Session<LuaGame>) -> String {
    game.game.halted().unwrap_or_default()
}

/// What stops a game as it starts: a mistake loading it, or one its first
/// handlers made (it then loads, stopped); empty when it starts.
fn start_error(dir: &Path) -> String {
    match Session::<LuaGame>::load(dir) {
        Err(e) => format!("{e:#}"),
        Ok(g) => halted(&g),
    }
}

/// A number the script kept in `world.state`.
fn state(game: &mut Session<LuaGame>, key: &str) -> String {
    let Some(super::Value::Table(entries)) = game.game.capture().unwrap().state else { return String::new() };
    entries.iter().find(|(k, _)| *k == super::value::Key::Str(key.as_bytes().to_vec())).map(|(_, v)| format!("{v:?}")).unwrap_or_default()
}

/// Globals stay as `on_world_start` left them: a new one or a changed one
/// in `on_tick` is a mistake that says where state belongs; a global table
/// is only to look at.
#[test]
fn globals_are_frozen_after_load() {
    let mut g = game("frozen", "n = 1\nfunction on_world_start() m = 2 end\nfunction on_tick() n = n + 1 end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("'n' is a global variable: what changes belongs in a state (world.state, player.state, thing.state)"), "{}", halted(&g));
    let mut g = game("frozen-table", "T = { n = 0 }\nfunction on_tick() T.n = 1 end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("only to look at"), "{}", halted(&g));
    let mut g = game("rawset", "function on_tick() rawset(_ENV, 'x', 1) end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("rawset"), "{}", halted(&g));
}

/// `on_draw` only looks: changing a state, spawning or drawing a random
/// number there stops the game, naming what was tried.
#[test]
fn draw_only_looks() {
    for (name, line, says) in [
        ("draw-state", "world.state.x = 1", "only to look at"),
        ("draw-spawn", "level:spawn('gem', 10, 10)", "level:spawn: on_draw only looks"),
        ("draw-random", "local r = math.random()", "math.random"),
        ("draw-shake", "screen:shake(0.5)", "screen:shake: on_draw only looks"),
        ("draw-hitstop", "world:hitstop(3)", "world:hitstop: on_draw only looks"),
        ("draw-flash", "screen:flash(1)", "screen:flash: on_draw only looks"),
    ] {
        let g = game(name, &format!("function on_draw() {line} end\n"));
        g.compose(0.0, 1.6, Scene::Play, None);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
}

/// screen:shake and screen:flash shake and whiten the screen and die down by
/// themselves; world:hitstop holds the game still for the ticks asked,
/// on_tick too, a second at most (the shake holds still with it and
/// dies down after).
#[test]
fn a_script_shakes_holds_and_flashes() {
    let main = "function on_world_start() world.state.n = 0 end\n\
                function on_tick()\n\
                  world.state.n = world.state.n + 1\n\
                  if world.state.n == 2 then screen:shake(0.3) screen:flash(0.8) world:hitstop(4) end\n\
                  if world.state.n == 3 then world:hitstop(100000) end\n\
                end\n";
    let mut g = game("juice", main);
    steps(&mut g, 2);
    let sim = g.game.sim();
    assert!(sim.shake > 0.2 && sim.shake <= 0.3, "shake {}", sim.shake);
    assert!(sim.screen_flash > 0.7, "flash {}", sim.screen_flash);
    assert_eq!(sim.freeze, 4);
    steps(&mut g, 4);
    assert_eq!(state(&mut g, "n"), format!("{:?}", super::Value::Int(2)), "held still");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "n"), format!("{:?}", super::Value::Int(3)), "on again");
    assert_eq!(g.game.sim().freeze, 60, "a second at most");
    steps(&mut g, 60);
    assert_eq!(state(&mut g, "n"), format!("{:?}", super::Value::Int(3)), "held still a second");
    assert!(g.game.sim().shake > 0.0, "the shake holds still with the game");
    steps(&mut g, 10);
    let sim = g.game.sim();
    assert_eq!((sim.shake, sim.screen_flash), (0.0, 0.0), "both died down");
}

/// `controls` is what the player plays with, there everywhere: the device
/// used last and how it labels an action's key (the keyboard by its
/// layout, the pad by its letters), in a text as the action in braces too;
/// nobody plays until a window or a replay says who does. An action the
/// game lacks is named, and so is a brace left open.
#[test]
fn controls_name_the_keys_in_hand() {
    use crate::controls::{Bindings, Device, PadStyle};
    let main = "function on_tick() world.state.seen = controls.device or 'nobody' end\n\
                function on_draw(frame) frame:text((controls.device or 'nobody') .. ':' .. controls:key('fire') .. ':{pause}:{{', 0, 0) end\n";
    let mut g = game("keys-in-hand", main);
    let shown = |g: &Session<LuaGame>| {
        g.compose(0.0, 1.6, Scene::Play, None);
        first_text(g)
    };
    assert_eq!(shown(&g), "nobody:::{");
    let mut settings = crate::settings::Settings::default();
    settings.keys = Bindings::new(&[("fire", &["Z"]), ("pause", &["Escape"])]);
    settings.pad = Bindings::new(&[("fire", &["east"]), ("pause", &["start"])]);
    settings.key_names.insert("Z".into(), "Y".into());
    g.set_controls(g.player_controls(Device::Keyboard, PadStyle::Xbox, &settings));
    assert_eq!(shown(&g), "keyboard:Y:ESCAPE:{", "a German keyboard's Y where the binding says Z");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "seen"), format!("{:?}", super::Value::Str(b"keyboard".to_vec())), "on_tick sees it too");
    g.set_controls(g.player_controls(Device::Gamepad, PadStyle::PlayStation, &settings));
    assert_eq!(shown(&g), "gamepad:CIRCLE:OPTIONS:{");
    for (name, line, says) in [
        ("keys-unknown", "frame:text(controls:key('jumpp'), 0, 0)", "controls:key: no action 'jumpp'"),
        ("keys-unknown-text", "frame:text('PRESS {jumpp}', 0, 0)", "frame:text: no action 'jumpp'"),
        ("keys-open-brace", "frame:text('PRESS {fire', 0, 0)", "frame:text: a '{' without its '}'"),
    ] {
        let g = game(name, &format!("function on_draw(frame) {line} end\n"));
        g.compose(0.0, 1.6, Scene::Play, None);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
}

/// A replay carries what the player played with, from the step it
/// changed on: a game that goes by `controls` plays back the same (the
/// guard's second played twice included), and would not without them.
#[test]
fn a_replay_carries_the_controls() {
    use crate::controls::{Device, PadStyle};
    use crate::replay::{Recorder, Replay, Verdict, play_through};
    let main = "function on_world_start() world.state.pad = 0 end\n\
                function on_tick() if controls.device == 'gamepad' then world.state.pad = world.state.pad + 1 end end\n";
    let dir = folder("controls-replay", main);
    let mut g = game_in(&dir);
    let mut settings = crate::settings::Settings::default();
    settings.adopt(g.actions(), &g.default_keys(), &g.default_pad());
    let mut recorder = Recorder::default();
    for tick in 0..360 {
        let device = if (120..320).contains(&tick) { Device::Gamepad } else { Device::Keyboard };
        g.set_controls(g.player_controls(device, PadStyle::Xbox, &settings));
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(state(&mut g, "pad"), format!("{:?}", super::Value::Int(200)));
    let replay = recorder.replay(&g).unwrap();
    assert_eq!(replay.controls.iter().map(|c| c.at).collect::<Vec<_>>(), [0, 120, 320]);
    let path = dir.join("controls.toml");
    replay.save(&path).unwrap();
    let back = Replay::load(&path).unwrap();
    assert_eq!(back, replay, "written and read back");
    assert_eq!(play_through::<LuaGame>(&dir, &back).unwrap(), Verdict::Same);
    let without = Replay { controls: Vec::new(), ..back };
    assert!(matches!(play_through::<LuaGame>(&dir, &without).unwrap(), Verdict::RunsApart(_)), "nobody played the pad");
}

/// `pairs` and `next` walk the keys in order, however they were put in,
/// so a game goes the same after a restore.
#[test]
fn pairs_walks_keys_in_order() {
    let main = r#"
function on_world_start()
  local a, b = {}, {}
  for _, k in ipairs({ "c", 3, "a", 1, true, "b", 2 }) do a[k] = 1 end
  for _, k in ipairs({ 2, "b", true, 1, "a", 3, "c" }) do b[k] = 1 end
  local order = ""
  for k in pairs(a) do order = order .. tostring(k) end
  for k in pairs(b) do order = order .. tostring(k) end
  world.state.order = order
  world.state.first = tostring(next(a))
end
"#;
    let mut g = game("pairs", main);
    assert_eq!(state(&mut g, "order"), format!("{:?}", super::Value::Str(b"123abctrue123abctrue".to_vec())));
    assert_eq!(state(&mut g, "first"), format!("{:?}", super::Value::Str(b"1".to_vec())));
    let dir = folder("pairs-table-key", "function on_world_start() local t = { [{}] = 1 } for k in pairs(t) do end end\n");
    assert!(!start_error(&dir).is_empty(), "a table as a key has no order");
}

/// `math.random` draws from the simulation's generator: the same run gives
/// the same numbers, and drawing moves the generator replays check.
#[test]
fn math_random_draws_from_the_simulation() {
    let main = "function on_world_start() world.state.r = math.random(1, 1000) .. ' ' .. math.random() end\n";
    let (mut a, mut b) = (game("random-a", main), game("random-b", main));
    assert_eq!(state(&mut a, "r"), state(&mut b, "r"));
    let c = game("random-c", "function on_world_start() end\n");
    assert_ne!(a.game.sim().rng.state(), c.game.sim().rng.state());
}

/// Nothing outside the game: no os, io, require, load, coroutines, the
/// garbage collector or string.dump.
#[test]
fn no_os_io_load_require_debug_coroutine() {
    for (name, call) in [
        ("os", "os.time()"),
        ("io", "io.write('x')"),
        ("require", "require('x')"),
        ("load", "load('x = 1')"),
        ("dofile", "dofile('x')"),
        ("coroutine", "coroutine.create(print)"),
        ("collectgarbage", "collectgarbage()"),
        ("dump", "string.dump(print)"),
        ("debug", "debug.traceback()"),
    ] {
        let dir = folder(name, &format!("function on_world_start() {call} end\n"));
        assert!(!start_error(&dir).is_empty(), "{name} was there");
    }
}

/// Tables and functions turn into text without memory addresses.
#[test]
fn tostring_shows_no_addresses() {
    let mut g = game("tostring", "function on_world_start() world.state.s = tostring({}) .. tostring(print) .. string.format('%s', {}) .. ('%s'):format(print) end\n");
    assert_eq!(state(&mut g, "s"), format!("{:?}", super::Value::Str(b"tablefunctiontablefunction".to_vec())));
    let dir = folder("format-p", "function on_world_start() local s = string.format('%p', {}) end\n");
    assert!(!start_error(&dir).is_empty());
}

/// The maths give the same bits on every system (libm).
#[test]
fn maths_go_through_libm() {
    let mut g = game("maths", "function on_world_start() world.state.s = math.sin(1.0) + math.pow(2.0, 0.5) + math.atan(1.0, 2.0) end\n");
    let want = libm::sin(1.0) + libm::pow(2.0, 0.5) + libm::atan2(1.0, 2.0);
    assert_eq!(state(&mut g, "s"), format!("{:?}", super::Value::Float(want.to_bits())));
}

/// A power other than a square is refused where it stands.
#[test]
fn power_is_refused_but_squares_are_not() {
    assert!(start_error(&folder("power-2", "function on_world_start() world.state.x = 3 ^ 2 end\n")).is_empty());
    let e = start_error(&folder("power", "\nfunction on_world_start() world.state.x = 3 ^ 0.5 end\n"));
    assert!(e.contains("main.lua:2"), "{e}");
}

/// An endless loop stops with a message, even inside pcall.
#[test]
fn an_endless_loop_is_stopped() {
    let mut g = game("loop", "function on_tick() pcall(function() while true do end end) world.state.after = 1 end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("an endless loop?"), "{}", halted(&g));
}

/// Metatables that depend on the garbage collector are refused.
#[test]
fn setmetatable_refuses_gc_and_weak() {
    for (name, meta) in [("gc", "__gc"), ("weak", "__mode")] {
        let dir = folder(name, &format!("function on_world_start() setmetatable({{}}, {{ {meta} = 'k' }}) end\n"));
        assert!(!start_error(&dir).is_empty(), "{meta} was let through");
    }
}

/// State that is no plain data (a function, a table twice) is a mistake
/// that names where it sits: when the game starts, or at the tick it went
/// in.
#[test]
fn aliased_cyclic_or_function_state_is_refused() {
    let e = start_error(&folder("function-state", "function on_world_start() world.state.later = function() end end\n"));
    assert!(e.contains("world.state.later is a function"), "{e}");
    let mut g = game("alias-state", "function on_tick() if world.tick == 2 then local t = {} world.state.a = t world.state.b = t end end\n");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    steps(&mut g, 1);
    assert!(halted(&g).contains("met before"), "{}", halted(&g));
}

/// An error names the file and line; tools without a window end with it.
#[test]
fn a_lua_error_names_file_and_line() {
    let mut g = game("error", "function on_tick()\n  local x = nil\n  return x.y\nend\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("main.lua:3:"), "{}", halted(&g));
    assert!(g.stopped().is_err());
}

/// Saving `main.lua` takes the new code in and keeps `world.state`; a
/// mended error lets the game go on where it stopped.
#[test]
fn a_reload_keeps_world_state_and_a_fix_resumes() {
    let dir = folder("reload", "function on_world_start() world.state.n = 0 end\nfunction on_tick() world.state.n = world.state.n + 1 end\n");
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    steps(&mut g, 3);
    std::fs::write(dir.join("main.lua"), "function on_tick() world.state.n = world.state.n + 10 end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "n"), format!("{:?}", super::Value::Int(13)));
    std::fs::write(dir.join("main.lua"), "function on_tick() error('broken') end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 2);
    assert!(halted(&g).contains("broken"));
    std::fs::write(dir.join("main.lua"), "function on_tick() world.state.n = world.state.n + 1 end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "n"), format!("{:?}", super::Value::Int(14)));
}

/// A setting that only takes effect at the start waits for a restart; the
/// others take effect at once.
#[test]
fn restart_only_settings_wait_for_f5() {
    let dir = folder("restart", "function on_world_start() end\n");
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("{ 32, 32 }", "{ 20, 20 }").replace("c.actions", "c.camera.pitch = 50\n  c.actions")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    assert_eq!(g.cfg.player.spawn, [32.0, 32.0], "the start moved without a restart");
    assert_eq!(g.cfg.camera.pitch, 50.0);
}

/// A kind's props changed in conf.lua show at once on its things; a
/// misspelt sound there is a mistake on a reload as at the start.
#[test]
fn a_reload_takes_the_kinds_props_and_checks_their_sounds() {
    let main = "function on_level_start(how) level:spawn(\"box\", 6, 2) end\nfunction on_tick() world.state.worth = level:things(\"box\")[1].props.worth end\n";
    let dir = events_folder("reload-props", main, "  c.things.box.props = { worth = 1 }\n");
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    assert_eq!(g.game.inspect()["state"]["worth"], 1);
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("worth = 1", "worth = 5")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(g.game.inspect()["state"]["worth"], 5, "the kind's props anew");
    std::fs::write(dir.join("conf.lua"), conf.replace("worth = 1 }", "worth = 1 }\n  c.things.box.sound = \"poof\"")).unwrap();
    let e = g.reload(&dir.join("conf.lua")).map(|_| ()).unwrap_err();
    assert!(format!("{e:#}").contains("c.things.box.sound = \"poof\": no sound called that"), "{e:#}");
}

/// Every setting that waits for a restart is kept until then: a config
/// that differs in all of them, kept back, differs in none.
#[test]
fn what_waits_for_a_restart_is_kept_until_then() {
    use crate::config::{BlocksConfig, GameConfig};
    use crate::controls::Bindings;
    let old = GameConfig::default();
    let mut new = GameConfig::default();
    new.kit = Some("shooter".into());
    new.actions = vec!["jump".into()];
    new.keys = Bindings::new(&[("jump", &["Space"])]);
    new.pad = Bindings::new(&[("jump", &["south"])]);
    new.world.file = "other.vox".into();
    new.world.size = Some([8, 8, 8]);
    new.world.blocks = Some(BlocksConfig::default());
    new.markers = toml::from_str("rifleman = 240").unwrap();
    new.player.spawn = [1.0, 2.0];
    assert_eq!(old.restart_only(&new).len(), 9, "{:?}", old.restart_only(&new));
    old.keep_for_restart(&mut new);
    assert!(old.restart_only(&new).is_empty(), "{:?}", old.restart_only(&new));
}

/// A level of blocks is built anew when one of its blocks is saved, a new
/// one too, and its fingerprint follows; the hidden file a block is saved
/// through and other files in the folder do not count.
#[test]
fn saving_a_block_rebuilds_the_level() {
    use crate::blocks::{self, Block};
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind};
    use crate::session::Reloaded;
    use glam::IVec3;
    let dir = std::env::temp_dir().join(format!("voxcade-lua-blocks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.json\" }\n  c.player.spawn = { 2, 2 }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_world_start() end\n").unwrap();
    let mut level = GridLevel::new(4, IVec3::new(2, 1, 1));
    level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    level.cells[0] = Cell { block: 1, turn: Default::default() };
    level.save(&dir.join("level.json")).unwrap();
    let rock = |c: [u8; 3]| {
        let mut b = Block::empty(4);
        b.set(IVec3::ZERO, Some(c)).unwrap();
        b
    };
    rock([1, 2, 3]).save(&blocks::path(&dir, "rock")).unwrap();
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let before = g.fingerprint().unwrap();
    rock([9, 9, 9]).save(&blocks::path(&dir, "rock")).unwrap();
    assert!(matches!(g.reload(&blocks::path(&dir, "rock")).unwrap(), Reloaded::World));
    let world = &g.game.sim().world;
    assert_eq!(world.palette[world.get(IVec3::ZERO) as usize], [9, 9, 9, 255]);
    assert_ne!(g.fingerprint().unwrap(), before);
    rock([5, 5, 5]).save(&blocks::path(&dir, "new")).unwrap();
    assert!(matches!(g.reload(&blocks::path(&dir, "new")).unwrap(), Reloaded::World), "a new block");
    std::fs::write(dir.join("blocks/notes.txt"), "notes").unwrap();
    assert!(matches!(g.reload(&dir.join("blocks/notes.txt")).unwrap(), Reloaded::Nothing));
    assert!(matches!(g.reload(&dir.join("blocks/.rock.vvx.tmp")).unwrap(), Reloaded::Nothing));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Comments, blank lines and the look in conf.lua leave the replays'
/// fingerprint; a changed number in main.lua does not.
#[test]
fn comments_and_the_look_leave_the_fingerprint() {
    let main = "function on_world_start()\n  world.state.n = 1\nend\n";
    let base = game("print-a", main).fingerprint().unwrap();
    assert_eq!(game("print-b", &format!("-- a game\n\n{}", main.replace("  ", "\t"))).fingerprint().unwrap(), base);
    let dir = folder("print-c", main);
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("c.actions", "c.camera.view_width = 300\n  c.light.ambient = 0.7\n  c.actions")).unwrap();
    assert_eq!(Session::<LuaGame>::load(&dir).unwrap().fingerprint().unwrap(), base);
    assert_ne!(game("print-d", &main.replace("= 1", "= 2")).fingerprint().unwrap(), base);
}

/// A Lua game kept at a tick and brought back plays on exactly as the
/// first time, its script's state with it.
#[test]
fn a_restored_lua_game_plays_on_the_same() {
    use crate::replay::{Replay, Script, parse_script};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/sokovox");
    let replay = Replay::load(&dir.join("replays/tests/solve.toml")).unwrap();
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let mut script = Script::new(parse_script(&replay.input, g.actions()).unwrap());
    let inputs: Vec<_> = (0..600).map(|_| script.next_input()).collect();
    for input in &inputs[..300] {
        g.step(input);
    }
    let kept_at = g.game.state_hash();
    let snapshot = g.snapshot().unwrap();
    let mut first = Vec::new();
    for input in &inputs[300..] {
        g.step(input);
        first.push(g.game.state_hash());
    }
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), kept_at);
    for (n, input) in inputs[300..].iter().enumerate() {
        g.step(input);
        assert_eq!(g.game.state_hash(), first[n], "tick {} played out differently", 300 + n + 1);
    }
}

/// A game folder on the Arena's ground with the shooter kit, its own
/// `conf.lua` lines and this `main.lua`.
fn kit_folder(name: &str, conf: &str, main: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxcade-kit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena");
    for file in ["world.vox", "player.vvx"] {
        std::fs::copy(arena.join(file), dir.join(file)).unwrap();
    }
    // The arena's, with the kit's weapons it has not (kit.toml), and the test's.
    let base = std::fs::read_to_string(arena.join("conf.lua")).unwrap();
    let kit: toml::Table = toml::from_str(crate::shooter::kinds::KIT).unwrap();
    let lua = |v: &toml::Value| match v {
        toml::Value::String(s) => format!("{s:?}"),
        toml::Value::Array(a) => format!("{{ {} }}", a.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")),
        other => other.to_string(),
    };
    let weapons: String = kit["weapons"].as_table().unwrap().iter().filter(|(name, _)| !base.contains(&format!("c.weapons.{name} ="))).map(|(name, t)| {
        let keys: Vec<String> = t.as_table().unwrap().iter().map(|(k, v)| format!("{k} = {}", lua(v))).collect();
        format!("  c.weapons.{name} = {{ {} }}\n", keys.join(", "))
    }).collect();
    std::fs::write(dir.join("conf.lua"), base.replace("\nend", &format!("\n{weapons}{conf}\nend"))).unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    dir
}

/// With `c.kit = "shooter"` the kit's things run by themselves; the script
/// counts them, sets their hits, keeps the score, and moves its own kinds
/// (those without a behaviour) after its `on_tick`.
#[test]
fn a_lua_game_plays_with_the_shooter_kit() {
    let conf = "  c.things.crate = { model = \"player.vvx\", hp = 3 }\n  c.things.medkit = { behaviour = \"pickup\", reach = 5 }";
    let main = r#"
function on_level_start(how)
  level.state.crate = level:spawn("crate", 230, 150)
  level.state.medkit = level:spawn("medkit", 300, 150)
  level.state.x = level.state.crate.x
end

function on_tick()
  level.state.crate:walk({ x = 1, y = 0 }, 30)
  world.state.medkits = level:count("medkit")
  level.state.medkit.state.hp = 7
  level.state.crate.state.hp = level.state.crate.state.hp
  player.state.score = world.tick * 10
end
"#;
    let mut g = game_in(&kit_folder("plays", conf, main));
    steps(&mut g, 60);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "medkits"), format!("{:?}", super::Value::Int(1)));
    let Some(super::Value::Table(s)) = g.game.capture().unwrap().level else { panic!("no level state") };
    let x = |key: &str| s.iter().find(|(k, _)| *k == super::value::Key::Str(key.as_bytes().to_vec())).map(|(_, v)| v.clone());
    let Some(super::Value::Thing(crate_id)) = x("crate") else { panic!("no crate") };
    let Some(super::Value::Float(start)) = x("x") else { panic!("no start") };
    let crate_now = g.game.stage.host.get(crate::sim::things::Id::from_raw(crate_id)).unwrap();
    assert!(crate_now.body.pos.x > f64::from_bits(start) as f32 + 10.0, "the crate did not walk: {}", crate_now.body.pos.x);
    assert_eq!(crate_now.hp, 3);
    assert_eq!(g.game.score(), 600, "player.state.score is the kit's score");
    let medkit = g.game.stage.host.ids().into_iter().find_map(|id| g.game.stage.host.get(id).filter(|t| g.game.stage.kinds[t.kind].name == "medkit").map(|t| t.hp));
    assert_eq!(medkit, Some(7), "thing.state.hp is the thing's hits");
}

/// A game plays a song of its own (`music:play`) in time with its ticks:
/// `on_beat` tells each beat before `on_tick` (at 120 a minute one every
/// 30 ticks, a hit stop's ticks counted too), `music.beat` and `music.bar`
/// count them; a song asked for from "bar" begins at the next bar of the
/// one playing, at its beat 0. The session plays it where the game is in
/// it, and the songs' beats count for replays.
#[test]
fn a_game_goes_with_its_music() {
    let dir = std::env::temp_dir().join(format!("voxcade-lua-music-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/drive.mml"), "#tempo 120\n@a square duty=0.5\nA: c1 c1 c1 c1\n").unwrap();
    std::fs::write(dir.join("music/calm.mml"), "#tempo 60\n#meter 3\n@a triangle\nA: c2. c2.\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/drive.mml\", \"music/calm.mml\" }\nend\n").unwrap();
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 15, 15, 0, "#444444")
  world.state.told = ""
  world.state.quiet = music.song == nil and music.beat == nil and music.bar == nil
  music:play("drive", "now")
end

function on_beat(beat)
  world.state.told = world.state.told .. beat .. "@" .. world.tick .. ";"
end

function on_tick()
  if world.tick == 100 then music:play("calm") end
  if world.tick == 130 then
    world.state.now = music.song .. " " .. music.beat .. " " .. music.bar
    world:hitstop(40)
  end
end
"##;
    std::fs::write(dir.join("main.lua"), main).unwrap();
    let mut g = Session::<crate::Voxcade>::load(&dir).unwrap();
    assert!(g.music_changed && !g.steers_music(), "nothing of its own before the first tick");
    let mut mixer = crate::audio::Mixer::default();
    let mut buf = vec![0.0f32; crate::audio::SAMPLE_RATE as usize / 60 * 2];
    let mut snapshot = None;
    let mut hashes = Vec::new();
    for tick in 0..200 {
        g.step(&Input::default());
        g.feed_music(&mut mixer, false);
        mixer.mix(&mut buf);
        if tick == 90 {
            snapshot = Some(g.snapshot().unwrap());
        } else if tick > 90 {
            hashes.push(g.game.state_hash());
        }
    }
    assert_eq!(g.game.halted(), None);
    let text = |g: &Session<crate::Voxcade>, key: &str| match g.game.capture().unwrap().state {
        Some(super::Value::Table(t)) => t.iter().find(|(k, _)| *k == super::value::Key::Str(key.as_bytes().to_vec())).map(|(_, v)| format!("{v:?}")).unwrap_or_default(),
        _ => String::new(),
    };
    assert_eq!(text(&g, "quiet"), format!("{:?}", super::Value::Bool(true)));
    // drive's beats every 30 ticks; calm from drive's next bar (tick 121),
    // its beat 0 there; calm's beat 1 a second later, 40 of them a hit
    // stop's: told at the first tick after it, world.tick 141.
    assert_eq!(text(&g, "told"), format!("{:?}", super::Value::Str(b"0@1;1@31;2@61;3@91;0@121;1@141;".to_vec())));
    assert_eq!(text(&g, "now"), format!("{:?}", super::Value::Str(b"calm 0 0".to_vec())));
    assert!(g.steers_music());
    assert_eq!(mixer.music_title(), Some(""), "the game's song plays (calm has no #title)");
    // Brought back, it plays on the same: the clock and the song asked for
    // are the game's state.
    let told = text(&g, "told");
    g.restore(snapshot.as_ref().unwrap());
    for (n, hash) in hashes.iter().enumerate() {
        g.step(&Input::default());
        assert_eq!(g.game.state_hash(), *hash, "tick {} played out differently", 92 + n);
    }
    assert_eq!(text(&g, "told"), told);

    // Its songs' beats count for its replays.
    let before = g.fingerprint().unwrap();
    std::fs::write(dir.join("music/calm.mml"), "#tempo 61\n#meter 3\n@a triangle\nA: c2. c2.\n").unwrap();
    let again = Session::<crate::Voxcade>::load(&dir).unwrap();
    assert_ne!(again.fingerprint().unwrap(), before, "another tempo, other game data");

    // Mistakes name what is wrong.
    for (line, says) in [
        ("music:play('nope')", "music:play: no song called 'nope' in c.audio.music (its songs: drive, calm)"),
        ("music:play('drive', 'soon')", "music:play: from \"bar\", \"beat\" or \"now\", not \"soon\""),
        ("local v = music.volume", "music has no volume"),
    ] {
        std::fs::write(dir.join("main.lua"), format!("function on_tick()\n  {line}\nend\n")).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{line}: {}", halted(&g));
    }
    // The tension: 1 unless set, 0..1, counted once it is set.
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  if world.tick == 1 then world.state.was = music.intensity end\n  if world.tick == 2 then music.intensity = 0.25 end\n  world.state.now = music.intensity\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    let calm = g.game.state_hash();
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "was"), format!("{:?}", super::Value::Float(1.0f64.to_bits())));
    assert_eq!(state(&mut g, "now"), format!("{:?}", super::Value::Float(0.25f64.to_bits())));
    assert_ne!(calm, g.game.state_hash());
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  music.intensity = 2\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert!(halted(&g).contains("music.intensity: from 0 (calm) to 1"), "{}", halted(&g));
    std::fs::write(dir.join("main.lua"), "music = 3\n").unwrap();
    assert!(start_error(&dir).contains("'music' is the engine's"), "{}", start_error(&dir));
}

/// `music:play()` without a song gives the music back to the player's
/// choice from the next bar: no song of the game's, no beats, the session
/// plays the player's song again (More Blood's gunship song so).
#[test]
fn the_player_gets_the_music_back() {
    let dir = std::env::temp_dir().join(format!("voxcade-lua-music-back-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/mine.mml"), "#tempo 120\nA: c1\n").unwrap();
    std::fs::write(dir.join("music/boss.mml"), "#tempo 120\nA: c1\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/mine.mml\", \"music/boss.mml\" }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  if world.tick == 10 then music:play('boss') end\n  if world.tick == 20 then music:play() end\n  world.state.song = music.song or '-'\nend\n").unwrap();
    let mut g = Session::<crate::Voxcade>::load(&dir).unwrap();
    let mut songs = Vec::new();
    for _ in 0..200 {
        g.step(&Input::default());
        songs.push((g.steers_music(), g.song_file().map(str::to_string)));
    }
    assert_eq!(halted(&g), "");
    assert!(!songs[5].0 && songs[15].0, "the game's from tick 11");
    assert!(songs[100].0, "until the boss song's next bar (tick 131)");
    assert!(!songs[150].0, "the player's again after it");
    assert_eq!(songs[150].1.as_deref(), Some("music/mine.mml"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.audio.boss_music = \"music/boss.mml\"\nend\n").unwrap();
    assert!(start_error(&dir).contains("c.audio.boss_music is no more"), "{}", start_error(&dir));
}

/// `c.audio.drive` pushes the game's sound into the limiter, 0 to 12
/// decibels; outside that it is a mistake that names it.
#[test]
fn the_drive_is_from_0_to_12() {
    let dir = std::env::temp_dir().join(format!("voxcade-lua-drive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.lua"), "function on_tick() end\n").unwrap();
    for bad in ["13", "-1", "0/0"] {
        std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.size = {{ 16, 16, 8 }}\n  c.audio.drive = {bad}\nend\n")).unwrap();
        assert!(start_error(&dir).contains("c.audio.drive"), "{bad}: {}", start_error(&dir));
    }
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.drive = 6\nend\n").unwrap();
    assert_eq!(Session::<crate::Voxcade>::load(&dir).unwrap().cfg.audio.drive, 6.0);
}

/// A sound is a file: `sounds/x.toml` plays as "x"; `c.sounds` in
/// conf.lua is refused with the way out; a name with no file halts the
/// script saying where sounds come from.
#[test]
fn a_sound_is_a_file_of_the_game() {
    let dir = folder("sound-file", "function on_tick()\n  level:sound(\"zap\", { x = 8, y = 8, z = 0 })\nend\n");
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/zap.toml"), "wave = \"saw\"\nfreq = 600\nsweep = 7\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert!(g.game.stage.host.sim().sounds.iter().any(|s| s.name == "zap"), "it plays");
    assert!(g.sounds.get("zap").is_some() && g.sounds.get("explosion").is_some(), "the game's and the engine's");
    std::fs::write(dir.join("conf.lua"), CONF.replace("c.things.miner = {}", "c.things.miner = {}\n  c.sounds.zap = { wave = \"saw\" }")).unwrap();
    let e = start_error(&dir);
    assert!(e.starts_with("c.sounds is no more: a sound is a file now, sounds/NAME.toml") && e.contains("--to-files"), "{e}");
    std::fs::write(dir.join("conf.lua"), CONF).unwrap();
    std::fs::remove_file(dir.join("sounds/zap.toml")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert!(halted(&g).contains("no sound called 'zap': a sound is a file, sounds/zap.toml (the game's: impact, explosion, crumble)"), "{}", halted(&g));
}

/// A sound file saved while the game runs is made anew, and only it; a
/// new one is there to play at once, one taken away falls back to the
/// kit's of its name or is gone. A sealed game plays its sounds from its
/// files, and an applied one is made anew as well.
#[test]
fn a_sound_file_comes_in_while_playing() {
    let dir = kit_folder("sound-reload", "", "function on_tick()\n  if world.state.boom then level:sound(\"boom\", { x = 8, y = 8, z = 0 }) end\nend\n");
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 600\n").unwrap();
    std::fs::write(dir.join("sounds/shot.toml"), "wave = \"sine\"\nhold = 0\ndecay = 0.05\n").unwrap();
    let mut g = game_in(&dir);
    let kits_shot = g.sounds.get("splat").unwrap().clone();
    let zap = g.sounds.get("zap").unwrap().clone();
    let reload = |g: &mut Session<LuaGame>, file: &str| g.reload(&dir.join(file)).unwrap();
    // One changed: only it is made anew.
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 300\ndecay = 0.4\n").unwrap();
    assert_eq!(reload(&mut g, "sounds/zap.toml"), crate::session::Reloaded::Sounds);
    assert!(!std::sync::Arc::ptr_eq(g.sounds.get("zap").unwrap(), &zap), "zap made anew");
    assert!(std::sync::Arc::ptr_eq(g.sounds.get("splat").unwrap(), &kits_shot), "the others kept");
    // One new: the script may play it now.
    std::fs::write(dir.join("sounds/boom.toml"), "wave = \"noise\"\n").unwrap();
    assert_eq!(reload(&mut g, "sounds/boom.toml"), crate::session::Reloaded::Sounds);
    g.eval("world.state.boom = true").unwrap();
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert!(g.game.stage.host.sim().sounds.iter().any(|s| s.name == "boom"), "the new sound plays");
    // A mistake keeps the sounds as they were and says where.
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 300\ndecay = 40\n").unwrap();
    let e = format!("{:#}", g.reload(&dir.join("sounds/zap.toml")).unwrap_err());
    assert_eq!(e, "sounds/zap.toml line 2: decay = 40: from 0 to 10 seconds (fading from full volume to silence at the end, 0.005 at least)");
    assert_eq!(g.trouble(), Some(e));
    // The game's shot taken away: the kit's again.
    let games_shot = g.sounds.get("shot").unwrap().len();
    std::fs::remove_file(dir.join("sounds/shot.toml")).unwrap();
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 300\n").unwrap();
    assert_eq!(reload(&mut g, "sounds/shot.toml"), crate::session::Reloaded::Sounds);
    assert_ne!(g.sounds.get("shot").unwrap().len(), games_shot, "the kit's shot");
    assert_eq!(g.trouble(), None);
    // A file gone elsewhere changes nothing.
    assert_eq!(g.reload(&dir.join("gone.vvx")).unwrap(), crate::session::Reloaded::Nothing);

    // Sealed: the sounds are its files', whatever the folder holds since.
    let store = crate::files::Store::at(&std::env::temp_dir().join(format!("voxcade-sound-store-{}", std::process::id())));
    let (_, files) = crate::files::Files::seal(&dir, &store).unwrap();
    let mut sealed = Session::<LuaGame>::load_from(files, crate::saves::Slots::default()).unwrap();
    assert!(sealed.sounds.get("boom").is_some() && sealed.sounds.get("zap").is_some(), "its files' sounds");
    std::fs::remove_file(dir.join("sounds/boom.toml")).unwrap();
    assert_eq!(sealed.reload(&dir.join("sounds/boom.toml")).unwrap(), crate::session::Reloaded::Nothing, "the folder is none of a sealed game's");
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 1200\ndecay = 0.6\n").unwrap();
    let before = sealed.sounds.get("zap").unwrap().len();
    let changes = sealed.files.changes_since(&store).unwrap();
    assert_eq!(sealed.apply(std::sync::Arc::new(changes)).unwrap(), crate::session::Reloaded::Sounds);
    assert_ne!(sealed.sounds.get("zap").unwrap().len(), before, "applied, made anew");
    assert!(sealed.sounds.get("boom").is_none(), "and the one taken away is gone");
}

/// `voxcade sounds GAME --to-files` writes each of `c.sounds` as a file,
/// with the comments that told about it, the numbers in the file's words,
/// exactly as they sounded.
#[test]
fn the_converter_writes_c_sounds_as_files() {
    let dir = folder("sound-convert", "function on_tick() end\n");
    let conf = CONF.replace("c.things.miner = {}", "c.things.miner = {}\n  -- a laser\n  c.sounds.shot = { wave = \"square\", freq = 1400, freq_end = 500, sustain = 0.01, decay = 0.07, volume = 0.18 }\n  c.sounds.coin = {\n    wave = \"triangle\", freq = 880, arp = 1.5, arp_at = 0.05,\n  }   -- a coin\n  c.sounds.rotor = \"none\"");
    std::fs::write(dir.join("conf.lua"), &conf).unwrap();
    crate::headless::sounds_to_files::<LuaGame>(&dir).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("sounds/shot.toml")).unwrap(), "# a laser\nwave = \"square\"\nfreq = 1400\nsweep = -17.825122\nhold = 0.01\ndecay = 0.07\nvolume = 0.18\n");
    assert_eq!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap(), "# a coin\nwave = \"triangle\"\nfreq = 880\njump = 7.01955\njump_at = 0.05\n");
    assert_eq!(std::fs::read_to_string(dir.join("sounds/rotor.toml")).unwrap(), "silent = true\n");
    let lines: Vec<&str> = conf.lines().collect();
    let found: Vec<(Option<String>, usize, usize)> = crate::headless::sound_statements(&lines).into_iter().map(|s| (s.name, s.from, s.to)).collect();
    let first = lines.iter().position(|l| l.contains("-- a laser")).unwrap();
    assert_eq!(found, [(Some("shot".into()), first, first + 1), (Some("coin".into()), first + 2, first + 4), (Some("rotor".into()), first + 5, first + 5)]);
    // Taken out of conf.lua, the game plays them.
    std::fs::write(dir.join("conf.lua"), CONF).unwrap();
    let g = game_in(&dir);
    assert!(g.sounds.get("shot").is_some() && g.sounds.get("coin").is_some());
}

/// The shooter's things too: a soldier switched off stands where physics
/// leaves him, `thing:launch` moves him and the player; the player's own
/// rules stay on (no thing of a kind yet).
#[test]
fn the_shooters_things_switch_off_and_launch() {
    let main = r#"
function on_level_start(how)
  level.state.man = level:spawn("rifleman", player.x, player.y + 40)
end

function on_tick()
  local man = level.state.man
  if world.tick == 1 then
    man:enable("soldier", false)
  elseif world.tick == 10 then
    world.state.x, world.state.y = man.x, man.y
  elseif world.tick == 60 then
    world.state.stood = man.x == world.state.x and man.y == world.state.y
    man:launch({ x = 1, y = 0 }, 80)
    player:launch({ x = -1, y = 0 }, 80)
    world.state.px = player.x
  elseif world.tick == 64 then
    world.state.moved = man.x > world.state.x and player.x < world.state.px
  end
end
"#;
    let mut g = game_in(&kit_folder("off", "  c.things.rifleman = { behaviour = \"soldier\", model = \"player.vvx\", weapon = \"enemy_rifle\" }", main));
    steps(&mut g, 65);
    assert_eq!(halted(&g), "");
    let yes = format!("{:?}", super::Value::Bool(true));
    for key in ["stood", "moved"] {
        assert_eq!(state(&mut g, key), yes, "{key}");
    }
    let mut g = game_in(&kit_folder("off-player", "", "function on_tick() player:enable('soldier', false) end\n"));
    steps(&mut g, 1);
    assert!(halted(&g).contains("move by their keys"), "{}", halted(&g));
}

/// The kits are building blocks for many games, never one game's rules:
/// outside their tests and their drawing they name no game's weapon or
/// action, keep no hit points, lives, grenades, grace or down time as a
/// number of their own, and turn nothing north or south by themselves. A
/// line that must is marked `// why:` (on it or the line above), saying why.
#[test]
fn the_kits_keep_no_game_in_them() {
    const NAMES: [&str; 10] = ["rifle", "machine_gun", "rockets", "grenade", "knife", "enemy_rifle", "enemy_grenade", "door_gun", "rocket_pods", "fire"];
    const COUNTS: [&str; 6] = ["hp", "max_hp", "lives", "grenades", "grace", "down"];
    const WAYS: [&str; 4] = ["Vec3::Y", "Vec3::NEG_Y", "IVec2::Y", "IVec2::NEG_Y"];
    // A number other than 0 given to one of the counts: `hp: 5`, `lives = 3`.
    let counted = |code: &str| {
        COUNTS.iter().any(|w| {
            code.match_indices(w).any(|(at, _)| {
                let before = code[..at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                let rest = code[at + w.len()..].trim_start();
                let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=').filter(|r| !r.starts_with('=')));
                before && rest.is_some_and(|r| r.trim_start().starts_with(|c: char| c.is_ascii_digit() && c != '0'))
            })
        }) || (code.contains("const ") && ["HP", "LIVES", "GRENADE", "GRACE", "DOWN"].iter().any(|w| code.split("const ").nth(1).is_some_and(|c| c.split(':').next().unwrap_or("").contains(w))))
    };
    let mut found = Vec::new();
    for dir in ["src/shooter", "src/platformer"] {
        for path in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(dir)).unwrap().map(|e| e.unwrap().path()) {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if ["draw.rs", "hud.rs", "tests.rs", "pins.rs"].contains(&name.as_str()) || path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("mod tests") || (n > 0 && lines[n - 1].trim() == "#[cfg(test)]" && line.contains("mod ")) {
                    break;
                }
                let why = line.contains("// why:") || (n > 0 && lines[n - 1].trim_start().starts_with("// why:"));
                let code = line.split("//").next().unwrap_or("");
                // The code without what its strings say (hints may name old keys).
                let bare: String = code.split('"').step_by(2).collect::<Vec<_>>().join("\"\"");
                let named = NAMES.iter().find(|w| code.contains(&format!("\"{w}\"")));
                let way = WAYS.iter().find(|w| bare.match_indices(*w).any(|(at, _)| !bare[at + w.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')));
                let hit = named.map(|w| format!("names {w}")).or(way.map(|w| format!("turns by itself, {w}"))).or(counted(&bare).then(|| "keeps a count".to_string()));
                if let (Some(what), false) = (hit, why) {
                    found.push(format!("{dir}/{name}:{}: {what}: {}", n + 1, line.trim()));
                }
            }
        }
    }
    assert!(found.is_empty(), "a game's rule in a kit (the game's conf.lua or main.lua, or // why: and the reason):\n{}", found.join("\n"));
}

/// A hostage that runs to a place (`flee = "exit"`) needs a level that
/// has one so called.
#[test]
fn a_hostage_runs_to_a_place_some_level_has() {
    let conf = "  c.things.hostage = { behaviour = \"captive\", model = \"player.vvx\", flee = \"exit\" }";
    let e = Session::<LuaGame>::load(&kit_folder("flee", conf, "")).map(|_| ()).unwrap_err();
    assert!(format!("{e:#}").contains("c.things.hostage.flee = \"exit\": no level has a place called exit"), "{e:#}");
}

/// A hostage is freed by the game's code: touched within its reach,
/// `thing.state.freed = true` bursts its cage and it runs away from the
/// player (its kind's `flee`, "away" unless it says otherwise).
#[test]
fn the_code_frees_a_hostage() {
    let conf = "  c.things.hostage = { behaviour = \"captive\", model = \"player.vvx\", reach = 8 }";
    let main = r#"
function on_level_start(how)
  level.state.h = level:spawn("hostage", player.x, player.y + 5)
end

function on_tick()
  local h = level.state.h
  if world.tick == 30 then
    world.state.touched = #player:touching("hostage")
    world.state.caged = not h.state.freed
    h.state.freed = true
    world.state.y = h.y
  end
  world.state.freed = h.state.freed
  world.state.ran = world.state.y and h.y - world.state.y > 5
end
"#;
    let mut g = game_in(&kit_folder("hostage", conf, main));
    steps(&mut g, 60);
    assert_eq!(halted(&g), "");
    let yes = format!("{:?}", super::Value::Bool(true));
    for key in ["caged", "freed", "ran"] {
        assert_eq!(state(&mut g, key), yes, "{key}");
    }
    assert_eq!(state(&mut g, "touched"), format!("{:?}", super::Value::Int(1)));
    assert_eq!(g.game.stage.host.player_number("score"), Some(0), "freeing is worth what the code adds");
}

/// The shooter fights in a level built in code too, without markers (the
/// template's): its riflemen's rounds hurt the player.
#[test]
fn the_shooter_fights_without_markers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/shooter");
    let mut g = game_in(&dir);
    for n in 0..1200 {
        g.step(&Input { up: n < 400, ..Input::default() });
    }
    assert_eq!(halted(&g), "");
    // Down and back up again, a life less (the template's code).
    let lives = g.game.inspect()["player"]["state"]["lives"].as_i64().unwrap();
    assert!(lives < 3, "the player was not hurt: lives {lives}");
}

/// With ready-made rules the game's code steers the camera too (a thing
/// followed, a point looked at, the player again) and turns the player.
#[test]
fn the_kits_camera_and_player_answer_the_code() {
    let main = r#"
function on_level_start(how)
  level.state.box = level:spawn("crate", 230, 150)
end

function on_tick()
  if world.tick == 2 then level.camera:look_at(40, 50) end
  if world.tick == 4 then level.camera:follow(level.state.box) end
  if world.tick == 6 then
    level.camera:follow(player)
    player:face({ x = 1, y = 0 })
    world.state.facing = player.facing.x
  end
end
"#;
    let conf = "  c.things.crate = { model = \"player.vvx\" }";
    let mut g = game_in(&kit_folder("camera", conf, main));
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert!(g.game.focus().is_none(), "looking at a point follows nothing");
    steps(&mut g, 2);
    let crate_at = g.game.focus().map(|b| b.pos.x);
    assert!(crate_at.is_some_and(|x| (x - 230.0).abs() < 1.0), "following the crate: {crate_at:?}");
    steps(&mut g, 2);
    assert_eq!(g.game.inspect()["state"]["facing"], 1, "the player turned by the code");
    assert!(g.game.focus().is_some_and(|b| (b.pos.x - 230.0).abs() > 1.0), "the player again");
}

/// The HUD is the game's: icons from c.icons, a bar's two parts, text
/// widths, an icon bursting; a wrong icon is a mistake with its name.
#[test]
fn the_hud_is_the_games() {
    use super::world::Cmd;
    let conf = "  c.icons.dot = { rows = { \"RR\", \"R.\" }, colors = { R = { 200, 10, 10 } } }";
    let main = r#"
function on_tick()
  if world.tick == 2 then screen:burst("dot", 10, 10) end
end

function on_draw(frame)
  frame:icon("dot", 4, 4)
  frame:bar(4, 20, 10, 2, 0.5)
  frame:text("AB", frame.width - frame:text_width("AB") - 3, 3)
end
"#;
    let mut g = game_in(&kit_folder("hud", conf, main));
    steps(&mut g, 3);
    g.compose(0.0, 1.6, Scene::Play, None);
    assert_eq!(halted(&g), "");
    let cmds = g.game.overlay.borrow().clone();
    assert!(matches!(cmds[0], Cmd::Icon { .. }));
    let boxes: Vec<(f32, f32)> = cmds.iter().filter_map(|c| match c { Cmd::Box { x, width, .. } => Some((*x, *width)), _ => None }).collect();
    assert_eq!(boxes, [(4.0, 5.0), (9.0, 5.0)], "half filled, the rest beside it");
    assert_eq!(g.game.stage.hud.bits.len(), 3 * g.cfg.ui.depth as usize, "the icon burst into its voxels");
    let bad = game_in(&kit_folder("hud-bad", conf, "function on_draw(frame) frame:icon(\"heart\", 0, 0) end\n"));
    bad.compose(0.0, 1.6, Scene::Play, None);
    assert!(halted(&bad).contains("no icon called 'heart' (c.icons: dot)"), "{}", halted(&bad));
    let dir = kit_folder("hud-colour", "  c.icons.dot = { rows = { \"RX\" }, colors = { R = { 1, 2, 3 } } }", "");
    let e = Session::<LuaGame>::load(&dir).map(|_| ()).unwrap_err();
    assert!(format!("{e:#}").contains("c.icons.dot.rows[1]: the letter 'X' has no colour"), "{e:#}");
}

/// `frame:to_screen` says where a point of the level shows, in the HUD's
/// measure: the player's feet below the middle (the camera looks at its
/// middle), higher up when lifted, a point behind the camera nowhere.
#[test]
fn a_point_of_the_level_shows_on_the_screen() {
    use super::world::Cmd;
    let main = r#"
function on_draw(frame)
  frame:text("W", frame.width, frame.height)
  local x, y = frame:to_screen(player)
  frame:text("P", x, y)
  local ux, uy = frame:to_screen(player, 30)
  frame:text("U", ux, uy)
  local bx, by = frame:to_screen({ x = player.x, y = player.y - 100000, z = player.z + 100000 })
  if bx then frame:text("B", bx, by) end
end
"#;
    let mut g = game_in(&kit_folder("to-screen", "", main));
    steps(&mut g, 3);
    g.compose(0.0, 1.6, Scene::Play, None);
    assert_eq!(halted(&g), "");
    let at: std::collections::BTreeMap<String, (f32, f32)> = g
        .game
        .overlay
        .borrow()
        .iter()
        .filter_map(|c| match c { Cmd::Text { text, x, y, .. } => Some((text.clone(), (*x, *y))), _ => None })
        .collect();
    let ((w, h), (px, py), (ux, uy)) = (at["W"], at["P"], at["U"]);
    assert!((px - w / 2.0).abs() < 1.0, "the player across the middle: {px} of {w}");
    assert!(py > h / 2.0 && py < h, "its feet below the middle: {py} of {h}");
    assert!((ux - px).abs() < 1.0 && uy < py - 5.0, "lifted, higher up: {uy} over {py}");
    assert!(!at.contains_key("B"), "a point behind the camera shows nowhere");
}

/// The game's code attacks too (`thing:attack`): a thing of its own fires
/// at the player, the player knifes a man in front of him; a weapon the
/// game has not is a mistake with the names it has.
#[test]
fn the_code_attacks() {
    let conf = "  c.things.turret = { model = \"player.vvx\" }\n  c.things.man = { behaviour = \"soldier\", model = \"player.vvx\", weapon = \"enemy_rifle\", hp = 3, sight = 0, hearing = 0 }";
    let main = r#"
function on_level_start(how)
  level.state.turret = level:spawn("turret", player.x, player.y + 40)
  level.state.man = level:spawn("man", player.x, player.y + 4)
end

function on_tick()
  local turret = level.state.turret
  turret:face({ x = 0, y = -1 })
  if world.tick % 30 == 1 then turret:attack("enemy_rifle", player) end
  player:face({ x = 0, y = 1 })
  if world.tick == 2 then player:attack("knife", level.state.man) end
  world.state.men = level:count("man")
  world.state.hp = player.state.hp
end
"#;
    let mut g = game_in(&kit_folder("attacks", conf, main));
    steps(&mut g, 200);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["state"]["men"], 0, "the knife took the man in front");
    assert!(g.game.inspect()["state"]["hp"].as_i64().unwrap() < 5, "the turret's rounds hurt the player");
    let mut bad = game_in(&kit_folder("attacks-bad", conf, "function on_tick() player:attack(\"bow\") end\n"));
    steps(&mut bad, 2);
    assert!(halted(&bad).contains("thing:attack: no weapon called 'bow'"), "{}", halted(&bad));
}

/// The kit moves its own things: the script may not.
#[test]
fn the_kits_things_move_by_its_rules() {
    let conf = "  c.things.medkit = { behaviour = \"pickup\" }";
    let main = "function on_tick() level:things(\"medkit\")[1]:walk({ x = 1, y = 0 }, 30) end\nfunction on_level_start(how) level:spawn(\"medkit\", 300, 150) end\n";
    let mut g = game_in(&kit_folder("kit-moves", conf, main));
    steps(&mut g, 1);
    assert!(halted(&g).contains("thing:walk: a medkit moves by its ready-made rules"), "{}", halted(&g));
}

/// A kit the program does not have is named; without a kit, a kind with a
/// behaviour is a mistake.
#[test]
fn an_unknown_kit_and_a_behaviour_without_one_are_named() {
    let dir = kit_folder("no-kit", "  c.kit = \"racer\"", "");
    let e = start_error(&dir);
    assert!(e.contains("c.kit = \"racer\": there is no such kit (this program's: \"shooter\", \"platformer\")"), "{e}");
    let dir = folder("behaviour", "function on_world_start() end\n");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("c.things.gem = { hp = 2 }", "c.things.gem = { behaviour = \"soldier\" }")).unwrap();
    let e = start_error(&dir);
    assert!(e.contains("c.things.gem.behaviour = \"soldier\": no behaviours without a kit"), "{e}");
}


/// A local of `main.lua` that `on_tick` changes is state no snapshot keeps:
/// `voxcade check` plays a second twice from a snapshot, sees the game run
/// differently and names the local.
#[test]
fn check_names_state_kept_outside_world_state() {
    use crate::replay::{Recorder, check};
    let clean = "function on_tick() world.state.n = (world.state.n or 0) + 1 end\n";
    let hidden = "local count = 0\nfunction on_tick()\n  count = count + 1\n  if count % 45 == 0 then world.state.n = (world.state.n or 0) + 1 end\nend\n";
    for (name, main, found) in [("guard-clean", clean, None), ("guard-hidden", hidden, Some("'count' (main.lua) changed"))] {
        let dir = folder(name, main);
        let mut g = game_in(&dir);
        let mut recorder = Recorder::default();
        for _ in 0..600 {
            let input = Input::default();
            g.step(&input);
            recorder.record(&g);
        }
        std::fs::create_dir_all(dir.join("replays")).unwrap();
        recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
        match (check::<LuaGame>(&dir, false), found) {
            (Ok(same), None) => assert!(same, "the clean game ran apart"),
            (Err(e), Some(found)) => assert!(e.to_string().contains(found), "{e}"),
            (r, _) => panic!("{name}: {:?}", r.map_err(|e| e.to_string())),
        }
    }
}

/// `check --rollback K` goes back K ticks again and again and plays on, as
/// rollback over a network would: a clean game comes out as recorded, one
/// that keeps state outside its states is found at the tick it went
/// apart.
#[test]
fn going_back_and_playing_on_comes_out_the_same() {
    use crate::replay::{Recorder, check_going_back};
    let clean = "function on_tick() world.state.n = (world.state.n or 0) + 1 end\n";
    let hidden = "local count = 0\nfunction on_tick()\n  count = count + 1\n  if count % 5 == 0 then world.state.n = (world.state.n or 0) + 1 end\nend\n";
    for (name, main, found) in [("back-clean", clean, None), ("back-hidden", hidden, Some("came out differently"))] {
        let dir = folder(name, main);
        let mut g = game_in(&dir);
        let mut recorder = Recorder::default();
        for _ in 0..240 {
            let input = Input::default();
            g.step(&input);
            recorder.record(&g);
        }
        std::fs::create_dir_all(dir.join("replays")).unwrap();
        recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
        match (check_going_back::<LuaGame>(&dir, 8), found) {
            (Ok(same), None) => assert!(same, "the clean game ran apart"),
            (Err(e), Some(found)) => assert!(e.to_string().contains(found), "{e}"),
            (r, _) => panic!("{name}: {:?}", r.map_err(|e| e.to_string())),
        }
    }
}

/// `check` draws the game after every tick, as a window does: a mistake
/// in `on_draw` that shows in a single picture (BunnyFarm's dog's board
/// stopped the game only in the window) stops the replay there, and so
/// does recording it anew.
#[test]
fn check_draws_every_tick() {
    use crate::replay::{Recorder, check};
    let main = "function on_tick() world.state.open = world.tick == 30 end\nfunction on_draw(frame)\n  if world.state.open then math.random(6) end\nend\n";
    let dir = folder("check-draws", main);
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..120 {
        let input = Input::default();
        g.step(&input);
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "", "played without a picture, nothing stopped it");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
    for update in [false, true] {
        let e = check::<LuaGame>(&dir, update).map(|_| ()).unwrap_err().to_string();
        assert!(e.contains("stopped at tick") && e.contains("math.random"), "update {update}: {e}");
    }
}

/// A game folder with a world made in code (`c.world.size`), this
/// `main.lua` and these files beside it.
fn code_world(name: &str, main: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxcade-code-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 32, 32, 8 }\n  c.things.box = {}\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

/// `world:read` gives a file of the game folder while `main.lua` starts,
/// never later and never outside the folder; what it read counts for
/// replays, and saving it runs `main.lua` anew. The old `read` says where
/// it went.
#[test]
fn read_reads_the_game_folder_while_starting() {
    let main = "TEXT = world:read(\"levels.txt\")\nfunction on_world_start() world.state.t = TEXT .. world:read(\"levels.txt\") end\nfunction on_tick() world.state.now = TEXT if world.tick == 3 then world:read(\"levels.txt\") end end\n";
    let dir = code_world("read", main, &[("levels.txt", "ab")]);
    let mut g = game_in(&dir);
    assert_eq!(state(&mut g, "t"), format!("{:?}", super::Value::Str(b"abab".to_vec())));
    let before = g.fingerprint().unwrap();
    std::fs::write(dir.join("levels.txt"), "cd").unwrap();
    assert!(matches!(g.reload(&dir.join("levels.txt")).unwrap(), crate::session::Reloaded::Code));
    assert_ne!(g.fingerprint().unwrap(), before, "the file read counts for replays");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "now"), format!("{:?}", super::Value::Str(b"cd".to_vec())), "saving it ran main.lua anew");
    steps(&mut g, 3);
    assert!(halted(&g).contains("only while main.lua starts"), "{}", halted(&g));
    for bad in ["../conf.lua", "/etc/passwd", ""] {
        let dir = code_world("read-bad", &format!("X = world:read(\"{bad}\")\n"), &[]);
        let e = start_error(&dir);
        assert!(e.contains("a file in the game folder"), "{bad}: {e}");
    }
    let dir = code_world("read-old", "X = read(\"levels.txt\")\n", &[("levels.txt", "ab")]);
    assert!(start_error(&dir).contains("read(name) is world:read(name) now"), "{}", start_error(&dir));
}

/// `require` runs a Lua file of the game folder once while the game starts
/// (dots between folders) and gives back what it returns, the same every
/// time; the files share `main.lua`'s globals, count for replays as
/// tokens (a comment changes nothing) and saving one runs the code anew.
/// Later, a loop, a missing file, a bad name, `main` and `conf` are
/// refused; a mistake in one names its file and line.
#[test]
fn a_game_runs_its_own_lua_files() {
    let main = "local util = require(\"util\")\nlocal de = require(\"lang.de\")\nSAME = require(\"util\") == util\nfunction on_world_start() world.state.t = util.twice(21) .. de.hello .. tostring(SAME) .. tostring(HELPER) end\nfunction on_tick() world.state.now = util.twice(1) if world.tick == 3 then require(\"util\") end end\n";
    let util = "HELPER = true\nlocal M = {}\nfunction M.twice(n) return n * 2 end\nreturn M\n";
    let dir = code_world("require", main, &[("util.lua", util)]);
    std::fs::create_dir_all(dir.join("lang")).unwrap();
    std::fs::write(dir.join("lang/de.lua"), "return { hello = \"hallo\" }\n").unwrap();
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "t"), format!("{:?}", super::Value::Str(b"42hallotruetrue".to_vec())));
    let before = g.fingerprint().unwrap();
    std::fs::write(dir.join("util.lua"), format!("-- a comment\n{util}")).unwrap();
    assert!(matches!(g.reload(&dir.join("util.lua")).unwrap(), crate::session::Reloaded::Code), "saving a required file runs the code anew");
    assert_eq!(g.fingerprint().unwrap(), before, "a comment changes nothing for replays");
    std::fs::write(dir.join("util.lua"), util.replace("n * 2", "n * 3")).unwrap();
    assert!(matches!(g.reload(&dir.join("util.lua")).unwrap(), crate::session::Reloaded::Code));
    assert_ne!(g.fingerprint().unwrap(), before, "a required file is the game's code");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "now"), "Int(3)", "the new code runs");
    assert_eq!(state(&mut g, "t"), format!("{:?}", super::Value::Str(b"42hallotruetrue".to_vec())), "the states stay");
    steps(&mut g, 3);
    assert!(halted(&g).contains("require(\"util\"): only while the game starts"), "{}", halted(&g));
    let refused = |name: &str, main: &str, files: &[(&str, &str)], want: &str| {
        let e = start_error(&code_world(name, main, files));
        assert!(e.contains(want), "{name}: {e}");
    };
    refused("require-loop", "require(\"a\")\n", &[("a.lua", "require(\"b\")\n"), ("b.lua", "require(\"a\")\n")], "require(\"a\"): a loop: a → b → a");
    refused("require-missing", "require(\"nope\")\n", &[], "require(\"nope\"): no nope.lua in the game folder");
    for bad in ["../conf", "a/b", "", "a..b"] {
        refused("require-bad", &format!("require(\"{bad}\")\n"), &[], "a Lua file of the game folder by its name");
    }
    refused("require-dot-lua", "require(\"util.lua\")\n", &[], "the file's name without .lua: require(\"util\")");
    refused("require-main", "require(\"main\")\n", &[], "main.lua is the engine's to run");
    refused("require-conf", "require(\"conf\")\n", &[], "conf.lua is the engine's to run");
    refused("require-error", "require(\"boom\")\n", &[("boom.lua", "local x = 1\nerror(\"bang\")\n")], "boom.lua:2: bang");
    refused("require-power", "require(\"pow\")\n", &[("pow.lua", "return 2 ^ 3\n")], "pow.lua");
}

/// `frame:daylight` sets this picture's daylight, the rest is `c.light`'s;
/// the shadows follow its sun; a game whose code names it has every face a
/// sun above the horizon can light; a wrong key or value says so.
#[test]
fn a_game_sets_its_daylight() {
    use crate::session::Scene;
    let main = "function on_draw(frame) frame:daylight{ sun = { -1, 0, -1 }, sun_color = { 255, 128, 0 }, ambient = 0.2, sky = { 0, 0, 51 } } end\n";
    let mut g = game_in(&code_world("daylight", main, &[]));
    steps(&mut g, 1);
    let f = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(halted(&g), "");
    assert!((f.sun_dir - glam::Vec3::new(-1.0, 0.0, -1.0).normalize()).length() < 1e-5, "{}", f.sun_dir);
    assert_eq!((f.sun_color, f.ambient, f.sky), ([1.0, 128.0 / 255.0, 0.0], 0.2, [0.0, 0.0, 0.2]));
    assert_eq!(f.shadow, g.cfg.light.shadow, "what it leaves out is c.light's");
    let mut plain = game_in(&code_world("daylight-none", "function on_tick() end\n", &[]));
    steps(&mut plain, 1);
    let p = plain.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!((p.sun_color, p.ambient), (plain.cfg.light.sun_color, plain.cfg.light.ambient));
    assert_ne!(f.light_vp, p.light_vp, "the shadows fitted to the game's sun");
    assert!(g.game.moves_sun() && !plain.game.moves_sun());
    let faces = |m: crate::mesh::FaceMask| m.iter().filter(|&&on| on).count();
    assert!(faces(g.face_mask()) >= 5 && faces(g.face_mask()) > faces(plain.face_mask()), "every face a sun above can light");
    for (bad, want) in [
        ("frame:daylight{ sun = { 1, 0, 1 } }", "z below 0"),
        ("frame:daylight{ ambient = 2 }", "ambient is a number from 0 to 1"),
        ("frame:daylight{ moon = 1 }", "no 'moon'"),
        ("frame:daylight{ sky = \"blue\" }", "sky is a colour"),
    ] {
        let mut g = game_in(&code_world("daylight-bad", &format!("function on_draw(frame) {bad} end\n"), &[]));
        steps(&mut g, 1);
        g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
        assert!(halted(&g).contains(want), "{bad}: {}", halted(&g));
    }
}

/// A game's own logo, a model of parts (`c.title_screen.logo_model`:
/// BunnyFarm's, a letter each), takes the place of its lines of letters:
/// its parts come from the model bank, falling in one after another; its
/// voxels face the player in their colours, their sides a little darker.
#[test]
fn a_logo_model_drops_in_part_by_part() {
    use crate::menu::Menu;
    use crate::settings::Settings;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/bunnyfarm");
    let g = Session::<LuaGame>::load(&dir).unwrap();
    let settings = Settings::default();
    let drawn = |ticks: u32| {
        let mut menu = Menu::title();
        for _ in 0..ticks {
            menu.update();
        }
        g.menu_frame(&menu, &settings, 0.0, 16.0 / 9.0, Scene::Title { time: 1.0 }).ui_models.len()
    };
    let model = crate::vox::load_model(&dir.join("logo.vvx")).unwrap();
    assert_eq!(model.parts.len(), 9, "BUNNY FARM, a part a letter");
    assert_eq!(drawn(600), 9, "every letter drawn from the bank");
    let early = drawn(4);
    assert!((1..9).contains(&early), "the letters fall in one after another: {early} after 4 ticks");
    let (size, parts) = crate::ui::logo_model_parts(&model);
    assert_eq!(parts.len(), 9);
    assert_eq!((size.x, size.y, size.z), (model.size.x as f32, model.size.z as f32, model.size.y as f32), "across, up, deep");
    let colours: std::collections::HashSet<[u8; 4]> = model.frames[0].iter().map(|(_, c)| *c).collect();
    let shown: Vec<[u8; 4]> = parts.iter().flat_map(|(cubes, _)| cubes.iter().map(|c| c.color)).collect();
    assert!(shown.iter().any(|c| colours.contains(c)), "fronts in the model's colours");
    assert!(shown.iter().any(|c| !colours.contains(c)), "sides darker");
}

/// Once landed, the logo's letters bob gently (`c.title_screen.logo_bob`,
/// BunnyFarm's 2): a wave running through them, never further than asked,
/// and smooth, a little at a time between the pictures of a tick too;
/// without it they stand still.
#[test]
fn the_logo_bobs_once_landed() {
    use crate::menu::Menu;
    use crate::settings::Settings;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/bunnyfarm");
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let settings = Settings::default();
    let heights_at = |g: &Session<LuaGame>, ticks: u32, alpha: f32| -> Vec<f32> {
        let mut menu = Menu::title();
        for _ in 0..ticks {
            menu.update();
        }
        g.menu_frame(&menu, &settings, alpha, 16.0 / 9.0, Scene::Title { time: 1.0 }).ui_models.iter().map(|m| m.origin.y).collect()
    };
    let heights = |g: &Session<LuaGame>, ticks: u32| heights_at(g, ticks, 0.0);
    assert_eq!(g.cfg.title_screen.logo_bob, 2.0);
    g.cfg.title_screen.logo_bob = 0.0;
    let rest = heights(&g, 600);
    assert!([640, 700, 760].iter().all(|&t| heights(&g, t) == rest), "without it the letters stand still");
    g.cfg.title_screen.logo_bob = 2.0;
    let mut moved = false;
    let mut wave = false;
    for t in (600..760).step_by(10) {
        let up: Vec<f32> = heights(&g, t).iter().zip(&rest).map(|(a, b)| a - b).collect();
        assert!(up.iter().all(|d| d.abs() <= 2.0 + 1e-4), "{t}: {up:?}");
        moved |= up.iter().any(|&d| d.abs() > 0.5);
        wave |= up.iter().any(|&d| (d - up[0]).abs() > 0.5);
        let half: Vec<f32> = heights_at(&g, t, 0.5).iter().zip(heights(&g, t)).map(|(a, b)| (a - b).abs()).collect();
        assert!(half.iter().all(|&d| d < 0.1), "{t}: half a tick moves a letter a little, not a voxel: {half:?}");
    }
    assert!(moved && wave, "the letters bob, one after another");
}

/// `input:repeated` is true as a key goes down and again while it is held
/// (after 20 ticks, every 6); `frame:menu` and `frame:grid` are read and
/// checked (rows as texts or tables, keys in braces filled, the chosen row
/// from 1) and drawn, a long list scrolled; a mistake names what is wanted.
#[test]
fn a_game_draws_menus_and_repeats_keys() {
    let main = r##"
function on_world_start() world.state.down = 0 world.state.act = 0 end
function on_tick()
  if input:repeated("down") then world.state.down = world.state.down + 1 end
  if input:repeated("fire") then world.state.act = world.state.act + 1 end
end
function on_draw(frame)
  local rows = {}
  for i = 1, 10 do rows[i] = { text = "row " .. i, value = i * 3, dim = i == 2 } end
  rows[1] = "Plain"
  frame:menu{ title = "Market", tabs = { "Buy", "Sell" }, tab = 2, rows = rows, at = 7, shown = 4, hint = "{fire} buys" }
  frame:grid{ cells = { { count = 3 }, false, { dim = true } }, at = 2, columns = 2 }
end
"##;
    let mut g = game("menus", main);
    let down = Input { down: true, ..Input::default() };
    for _ in 0..40 {
        g.step(&down);
    }
    let mut fire = Input::default();
    fire.hold(1, true);
    g.step(&fire);
    assert_eq!((state(&mut g, "down"), state(&mut g, "act")), ("Int(5)".into(), "Int(1)".into()), "at 1, 21, 27, 33 and 39 ticks held; a press");
    g.compose(0.0, 16.0 / 9.0, crate::session::Scene::Play, None);
    assert_eq!(halted(&g), "");
    let overlay = g.game.overlay.borrow();
    let menu = overlay.iter().find_map(|c| match c { super::world::Cmd::Menu(m) => Some(m.clone()), _ => None }).expect("a menu");
    assert_eq!((menu.rows.len(), menu.at, menu.tab, menu.shown), (10, Some(6), 1, 4));
    assert_eq!((menu.rows[0].text.as_str(), menu.rows[1].dim, menu.rows[2].value.as_deref()), ("Plain", true, Some("9")));
    assert!(menu.hint.as_deref().is_some_and(|h| h.ends_with(" buys") && !h.contains('{')), "{:?}", menu.hint);
    let grid = overlay.iter().find_map(|c| match c { super::world::Cmd::Grid(g) => Some(g.clone()), _ => None }).expect("a grid");
    assert_eq!((grid.cells.len(), grid.at, grid.cells[0].count, grid.cells[1].icon.is_none(), grid.cells[2].dim), (3, Some(1), Some(3), true, true));
    drop(overlay);
    let ui = crate::config::UiConfig::default();
    assert!(g.game.hud(&ui, 0.0, 16.0 / 9.0, None).len() > 500, "drawn");
    for (bad, want) in [
        ("frame:menu{ rows = { \"a\" }, at = 2 }", "frame:menu: at = 2, but there are 1 (from 1)"),
        ("frame:menu{ rows = { \"a\" }, colour = 1 }", "frame:menu: no 'colour'"),
        ("frame:menu{ rows = { \"a\", 7 } }", "rows[2] is a text or"),
        ("frame:grid{ cells = { 1 } }", "cells[1] is { icon =, count =, dim = }"),
    ] {
        let g = game("menus-bad", &format!("function on_draw(frame) {bad} end\n"));
        g.compose(0.0, 16.0 / 9.0, crate::session::Scene::Play, None);
        assert!(halted(&g).contains(want), "{bad}: {}", halted(&g));
    }
}

/// `world.language` is the player's of the game's languages
/// (`c.languages`), else its first, else English; replays carry it with
/// the controls; a language that is no two small letters, or one named
/// twice, is a mistake.
#[test]
fn a_game_speaks_the_players_language() {
    let main = "function on_tick() world.state.lang = world.language end\n";
    let dir = code_world("lang", main, &[]);
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("c.things.box = {}", "c.things.box = {}\n  c.languages = { \"de\", \"en\", \"fr\" }")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "lang"), format!("{:?}", super::Value::Str(b"de".to_vec())), "the game's first");
    let mut controls = crate::controls::Controls::nobody(&[]);
    controls.language = Some("fr".into());
    g.set_controls(controls.clone());
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "lang"), format!("{:?}", super::Value::Str(b"fr".to_vec())));
    assert_eq!(g.language(), "fr");
    controls.language = Some("es".into());
    g.set_controls(controls.clone());
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "lang"), format!("{:?}", super::Value::Str(b"de".to_vec())), "one it does not speak gives way");
    let text = toml::to_string(&controls).unwrap();
    assert!(text.contains("language = \"es\"") && toml::from_str::<crate::controls::Controls>(&text).unwrap() == controls, "replays carry it: {text}");
    let mut plain = game_in(&code_world("lang-none", main, &[]));
    steps(&mut plain, 1);
    assert_eq!(state(&mut plain, "lang"), format!("{:?}", super::Value::Str(b"en".to_vec())), "English without c.languages");
    for (bad, want) in [("{ \"deu\" }", "c.languages[1]: \"deu\" is no language"), ("{ \"de\", \"de\" }", "c.languages: \"de\" twice")] {
        let dir = code_world("lang-bad", main, &[]);
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        std::fs::write(dir.join("conf.lua"), conf.replace("c.things.box = {}", &format!("c.things.box = {{}}\n  c.languages = {bad}"))).unwrap();
        let e = start_error(&dir);
        assert!(e.contains(want), "{bad}: {e}");
    }
}

/// A world made in code starts empty and is built with colours; saving
/// `conf.lua` keeps what the script built, and a new size waits for F5.
#[test]
fn a_world_made_in_code_is_built_with_colours() {
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 31, 31, 0, "#406080")
  level:fill(2, 2, 1, 4, 4, 3, "#ff0000")
  level:fill(3, 3, 3, 3, 3, 3, nil)
  level:set_voxel(10, 10, 1, "#406080")
  world.state.floor = level:voxel(5, 5, 0)
  world.state.same = level:voxel(10, 10, 1)
  world.state.red = level:voxel(2, 2, 1)
  world.state.hole = level:voxel(3, 3, 3)
end
"##;
    let dir = code_world("built", main, &[]);
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    let w = &g.game.sim().world;
    assert_eq!(w.size, glam::IVec3::new(32, 32, 8));
    let (floor, red) = (w.get(glam::IVec3::new(5, 5, 0)), w.get(glam::IVec3::new(2, 2, 1)));
    assert_eq!(w.palette[floor as usize], [0x40, 0x60, 0x80, 255]);
    assert_eq!(w.palette[red as usize], [255, 0, 0, 255]);
    assert_eq!(state(&mut g, "same"), state(&mut g, "floor"), "one colour, one palette entry");
    assert_eq!(state(&mut g, "hole"), format!("{:?}", super::Value::Int(0)));
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("{ 32, 32, 8 }", "{ 64, 64, 8 }").replace("\nend", "\n  c.camera.pitch = 60\nend")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    assert_eq!(g.cfg.camera.pitch, 60.0);
    assert_eq!(g.game.sim().world.size.x, 32, "a new size waits for a restart");
    assert_ne!(g.game.sim().world.get(glam::IVec3::new(5, 5, 0)), 0, "saving conf.lua kept the world the script built");
    let dir = code_world("bad-colour", "function on_level_start(how) level:set_voxel(1, 1, 1, \"red\") end\n", &[]);
    assert!(start_error(&dir).contains("#rrggbb"));
}

/// `place` puts one of the script's things exactly where it is told,
/// standing still; `level.camera:look_at` rests the camera over a point
/// until `follow` follows a thing again.
#[test]
fn place_and_look_at() {
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 31, 31, 0, "#406080")
  level.state.box = level:spawn("box", 8.5, 8.5)
  level.camera:look_at(20, 24)
end

function on_tick()
  local box = level:thing(level.state.box)
  if world.tick < 20 then box:walk({ x = 1, y = 0 }, 30) end
  if world.tick == 20 then box:place(12.5, 9.5) end
  if world.tick == 40 then level.camera:follow(box) end
end
"##;
    let mut g = game_in(&code_world("place", main, &[]));
    steps(&mut g, 21);
    assert_eq!(halted(&g), "");
    let body = g.game.focus().map(|b| b.pos);
    assert_eq!(body, None, "look_at follows nothing");
    let things = g.game.inspect();
    let x = things["things"][0]["x"].as_f64().unwrap();
    assert_eq!(x, 12.5, "placed exactly");
    let cam = g.game.sim().cam_target;
    assert_eq!((cam.x, cam.y), (20.0, 24.0), "the camera rests");
    steps(&mut g, 30);
    assert!(g.game.focus().is_some(), "focus follows again");
}

/// Sokovox reads all 155 of Skinner's Microban levels and builds each one
/// (N goes on to the next).
#[test]
fn sokovox_builds_every_level() {
    use crate::replay::{Script, parse_script};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/sokovox");
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let mut script = Script::new(parse_script(&["next:1;none:1"; 154].join(";"), g.actions()).unwrap());
    for _ in 0..154 * 2 {
        g.step(&script.next_input());
        assert_eq!(halted(&g), "");
    }
    assert_eq!(state(&mut g, "puzzle"), format!("{:?}", super::Value::Int(155)));
}

/// What a script prints is kept for the IDE (OUTPUT) until asked for.
#[test]
fn what_a_script_prints_is_kept_for_the_ide() {
    let mut g = game("printed", "function on_tick()\n  if world.tick == 3 then print(\"hello\", 42) end\nend\n");
    steps(&mut g, 5);
    assert_eq!(g.printed(), ["hello\t42"]);
    assert!(g.printed().is_empty(), "once");
}

/// A world the script builds (`c.world.size`) with a kit: the shooter's
/// player stands on what `on_level_start` built; the platformer's hero, spawned by
/// the script, stays the one it holds.
#[test]
fn a_kit_stands_on_a_world_the_script_built() {
    let shooter = kit_folder("built", "", "function on_level_start(how)\n  level:fill(0, 0, 0, 511, 511, 3, \"#c2a878\")\nend\n");
    let conf = std::fs::read_to_string(shooter.join("conf.lua")).unwrap().replace("c.world.file = \"world.vox\"", "c.world.size = { 512, 512, 48 }");
    std::fs::write(shooter.join("conf.lua"), conf).unwrap();
    let g = game_in(&shooter);
    assert_eq!(g.game.focus().map(|b| b.pos.z), Some(4.0), "on the sand, not in it");

    let dir = std::env::temp_dir().join(format!("voxcade-lua-built-platformer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3, lives = 3, taken = 0 }\n  c.world.size = { 96, 16, 48 }\n  c.things.hero = { behaviour = \"hero\", height = 14 }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_level_start(how)\n  level:fill(0, 0, 0, 95, 15, 3, \"#776655\")\n  level.state.hero = level:spawn(\"hero\", 40, 8)\nend\nfunction on_tick()\n  world.state.alive = level:thing(level.state.hero) ~= nil\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "alive"), format!("{:?}", super::Value::Bool(true)), "the hero the script holds");
    assert_eq!(g.game.focus().map(|b| b.pos.z), Some(4.0), "on the floor it built");
}

/// A behaviour switched off (`thing:enable`) leaves the thing to physics
/// and the script: the hero no longer follows the keys and the script
/// walks it up to a wall, which `thing.wall` finds; `thing:launch` throws
/// it back; switched on, it follows the keys again; a walker off stands.
#[test]
fn the_code_switches_rules_off_and_launches() {
    let dir = std::env::temp_dir().join(format!("voxcade-lua-enable-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3 }\n  c.world.size = { 128, 16, 64 }\n  c.things.hero = { behaviour = \"hero\", height = 14 }\n  c.things.slug = { behaviour = \"walker\", height = 6 }\nend\n").unwrap();
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 127, 15, 3, "#776655")
  level:fill(100, 0, 4, 127, 15, 60, "#776655")
  level.state.slug = level:spawn("slug", 20, 8)
  level:spawn("hero", 60, 8)
end

function on_tick()
  local slug = level.state.slug
  if world.tick == 5 then
    player:enable("hero", false)
    slug:enable("walker", false)
  elseif world.tick == 15 then
    world.state.x, world.state.slug_x = player.x, slug.x
  elseif world.tick == 30 then
    world.state.kept_still = player.x == world.state.x
    world.state.slug_still = slug.x == world.state.slug_x
  elseif world.tick > 30 and world.tick < 90 then
    player:walk({ x = 1, y = 0 }, 60)
  elseif world.tick == 90 then
    world.state.wall = player.wall and player.wall.x
    world.state.no_wall = slug.wall == nil
    player:launch({ x = -1, y = 0 }, 100, 12)
    world.state.from = player.x
  elseif world.tick == 96 then
    world.state.thrown = player.x < world.state.from and not player.on_ground
  elseif world.tick == 150 then
    player:enable("hero", true)
    world.state.x = player.x
  elseif world.tick == 180 then
    world.state.ran = player.x > world.state.x
  end
end
"##;
    std::fs::write(dir.join("main.lua"), main).unwrap();
    let mut g = game_in(&dir);
    let right = Input { right: true, ..Input::default() };
    for _ in 0..180 {
        g.step(&right);
    }
    assert_eq!(halted(&g), "");
    let yes = format!("{:?}", super::Value::Bool(true));
    for key in ["kept_still", "slug_still", "no_wall", "thrown", "ran"] {
        assert_eq!(state(&mut g, key), yes, "{key}");
    }
    assert_eq!(state(&mut g, "wall"), format!("{:?}", super::Value::Int(1)));

    // Mistakes name what is wrong.
    for (line, says) in [
        ("level.state.slug:enable('hero', false)", "a slug's behaviour is \"walker\", not \"hero\""),
        ("level:spawn('slug', 30, 8):launch({ x = 1, y = 0 }, 10)", ""),
        ("player.state.pose = 'dash'", "a hero has no frames called 'dash'"),
    ] {
        std::fs::write(dir.join("main.lua"), main.replace("  local slug = level.state.slug\n", &format!("  local slug = level.state.slug\n  if world.tick == 1 then {line} end\n"))).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(if says.is_empty() { halted(&g).is_empty() } else { halted(&g).contains(says) }, "{line}: {}", halted(&g));
    }
}

/// The old API's handlers are refused with their new names, a handler
/// that takes parameters (one named `world` would hide the global) and a
/// global named like one of the game model's objects too.
#[test]
fn the_old_names_are_refused_with_the_new_ones() {
    for (name, main, says) in [
        ("old-tick", "\n\nfunction tick(world, input) end\n", "main.lua:3: function tick(...) is the old API's name; it is on_tick() now"),
        ("old-load", "function load(world) end\n", "it is on_world_start() now"),
        ("old-draw", "function draw(world, out) end\n", "it is on_draw(frame) now"),
        ("old-leave", "function on_leave(a, b) end\n", "function on_leave is called on_touch_end now"),
        ("parameters", "\nfunction on_tick(world)\nend\n", "main.lua:2: on_tick() takes no parameters"),
        ("global-level", "level = 1\n", "'level' is the engine's"),
        ("global-screen", "function screen() end\n", "'screen' is the engine's"),
        ("global-controls", "controls = {}\n", "'controls' is the engine's"),
    ] {
        let e = start_error(&folder(name, main));
        assert!(e.contains(says), "{name}: {e}");
    }
}

/// `input` is there in `on_tick` only; drawing is on `on_draw`'s frame,
/// and `screen` says so to a script that draws on it.
#[test]
fn input_and_drawing_belong_to_their_handlers() {
    let e = start_error(&folder("input-at-start", "function on_world_start() local x = input.left end\n"));
    assert!(e.contains("input: the keys held are only there in on_tick"), "{e}");
    for (name, line, says) in [
        ("screen-text", "screen:text('x', 0, 0)", "screen.text is frame.text now"),
        ("screen-key", "screen:key('pause')", "screen.key is controls.key now"),
        ("screen-device", "local d = screen.device", "screen.device is controls.device now"),
    ] {
        let mut g = game(name, &format!("function on_tick() {line} end\n"));
        steps(&mut g, 1);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
    let mut g = game("input-in-tick", "function on_tick() if input.right then world.state.r = true end end\nfunction on_draw(frame) frame:text(tostring(input), 0, 0) end\n");
    g.step(&Input { right: true, ..Input::default() });
    assert_eq!(state(&mut g, "r"), format!("{:?}", super::Value::Bool(true)));
    g.compose(0.0, 1.6, Scene::Play, None);
    assert_eq!(first_text(&g), "input", "outside on_tick it only names itself");
}

/// `player.state` holds the score (the engine's: the title and the high
/// scores see it) beside the game's own keys; `pairs` walks both in order,
/// snapshots keep the game's, and the state hash takes it in only once a
/// game keeps some.
#[test]
fn player_state_holds_the_score_and_the_games_own() {
    let main = r#"
function on_tick()
  player.state.score = player.state.score + 5
  if world.tick == 2 then player.state.coins = 3 end
  local keys = ""
  for k, v in pairs(player.state) do keys = keys .. k .. "=" .. tostring(v) .. " " end
  world.state.keys = keys
end
"#;
    let mut g = game("player-state", main);
    let plain = game("player-none", "function on_tick() end\n").game.state_hash();
    assert_eq!(g.game.state_hash(), plain, "no player state yet: the hash as before");
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.score(), 15);
    assert_eq!(state(&mut g, "keys"), format!("{:?}", super::Value::Str(b"coins=3 score=15 ".to_vec())));
    let inspected = g.game.inspect();
    assert_eq!((inspected["player"]["state"]["score"].as_i64(), inspected["player"]["state"]["coins"].as_i64()), (Some(15), Some(3)));
    let snapshot = g.snapshot().unwrap();
    let kept = g.game.state_hash();
    steps(&mut g, 2);
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), kept, "the coins and the score came back");
    let e = game("player-score-text", "function on_tick() player.state.score = 'many' end\n");
    let mut e = e;
    steps(&mut e, 1);
    assert!(halted(&e).contains("player.state.score: a whole number, not string"), "{}", halted(&e));
}

/// `thing.state` holds the thing's hits (`hp`, the engine's, where its
/// kind declares them) beside the game's own keys, goes with the thing,
/// and `inspect` shows both.
#[test]
fn thing_state_holds_hp_and_the_games_own() {
    let main = r#"
function on_level_start(how)
  level.state.gem = level:spawn("gem", 20, 20)
  level.state.gem.state.shine = 2
  level.state.gem.state.hp = 4
  level.state.miner = level:spawn("miner", 30, 30)
end

function on_tick()
  local gem = level.state.gem
  if gem and world.tick == 2 then
    gem.state.hp = gem.state.hp - 1
    world.state.hp = gem.state.hp
  end
  if gem and world.tick == 3 then
    gem:remove()
    level.state.gem = nil
  end
end
"#;
    let mut g = game("thing-state", main);
    let things = g.game.inspect();
    let gem = &things["things"][0]["state"];
    assert_eq!((gem["hp"].as_i64(), gem["shine"].as_i64()), (Some(4), Some(2)));
    assert!(things["things"][1]["state"].get("hp").is_none(), "a miner's kind declares no hit points");
    let gem_id = things["things"][0]["id"].as_u64().unwrap() as u32;
    let gem_hp = g.game.stage.host.get(crate::sim::things::Id::from_raw(gem_id)).map(|t| t.hp);
    assert_eq!(gem_hp, Some(4), "the engine's hit points, not a key of the game's");
    steps(&mut g, 2);
    assert_eq!(state(&mut g, "hp"), format!("{:?}", super::Value::Int(3)));
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    assert!(g.game.capture().unwrap().data.is_empty(), "its state went with it");
}

/// `level` is not there in `on_world_start` (the run begins before its
/// first level); `world.state` is not there at the top of `main.lua`
/// (nothing would keep it).
#[test]
fn the_level_comes_after_the_worlds_start() {
    let e = start_error(&folder("level-at-start", "function on_world_start()\n  level:fill(0, 0, 0, 1, 1, 0, '#ffffff')\nend\n"));
    assert!(e.contains("main.lua:2") && e.contains("level:fill: there is no level yet in on_world_start; build it in on_level_start"), "{e}");
    let e = start_error(&folder("state-at-top", "world.state.counter = 100\n"));
    assert!(e.contains("there is no game here yet"), "{e}");
    let mut order = game("order", "function on_world_start() world.state.order = 'world' end\nfunction on_level_start(how) world.state.order = world.state.order .. ' level ' .. how end\n");
    assert_eq!(state(&mut order, "order"), format!("{:?}", super::Value::Str(b"world level start".to_vec())));
}

/// Things live in their level: `world.state` and `player.state`, which
/// outlive it, refuse them; a view of another state is no plain data
/// either. Each is named where it sits, at the tick it went in.
#[test]
fn long_lived_states_hold_no_things_and_no_views() {
    let e = start_error(&folder("thing-in-world", "function on_level_start(how)\n  world.state.hero = level:spawn('miner', 10, 10)\nend\n"));
    assert!(e.contains("world.state.hero is a thing"), "{e}");
    let mut g = game("thing-in-player", "function on_level_start(how) level.state.m = level:spawn('miner', 10, 10) end\nfunction on_tick() if world.tick == 3 then player.state.pet = level.state.m end end\n");
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    steps(&mut g, 1);
    assert!(halted(&g).contains("player.state.pet is a thing"), "{}", halted(&g));
    let mut g = game("view-in-state", "function on_tick() world.state.p = player.state end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("world.state.p is a view or another state"), "{}", halted(&g));
}

/// One handle per thing and one view per state: the same thing is the
/// same value wherever it comes from, and `t.state == t.state`.
#[test]
fn a_thing_is_one_value_everywhere() {
    let main = r#"
function on_level_start(how)
  level.state.m = level:spawn("miner", 10, 10)
end

function on_tick()
  local m = level.state.m
  local found = level:things("miner")[1]
  world.state.same = rawequal(m, found) and rawequal(m, level:thing(m.id))
  world.state.views = rawequal(m.state, m.state) and rawequal(player.state, player.state)
end
"#;
    let mut g = game("one-handle", main);
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "same"), format!("{:?}", super::Value::Bool(true)));
    assert_eq!(state(&mut g, "views"), format!("{:?}", super::Value::Bool(true)));
}

/// `thing:remove()` takes the thing away at the tick's end: it can still be
/// read until then, a second remove says it was going already, and it is
/// out of the level's lists and counts at once.
#[test]
fn removal_waits_for_the_ticks_end() {
    let main = r#"
function on_level_start(how)
  level.state.m = level:spawn("miner", 10, 10)
end

function on_tick()
  local m = level.state.m
  if world.tick == 2 then
    world.state.first = m:remove()
    world.state.again = m:remove()
    world.state.x = m.x
    world.state.counted = level:count("miner")
  elseif world.tick == 3 then
    world.state.after = level:count("miner")
    level.state.m = nil
  end
end
"#;
    let mut g = game("remove-later", main);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "first"), format!("{:?}", super::Value::Bool(true)));
    assert_eq!(state(&mut g, "again"), format!("{:?}", super::Value::Bool(false)));
    assert_eq!(state(&mut g, "counted"), format!("{:?}", super::Value::Int(0)), "out of the counts at once");
    assert_eq!(state(&mut g, "after"), format!("{:?}", super::Value::Int(0)));
}

/// `level:spawn` takes its options in a table: the height, the way it
/// faces, its first state (copied); a bare number is the old form, named.
#[test]
fn spawn_takes_a_table_of_options() {
    let main = r#"
function on_level_start(how)
  local first = { mood = "calm" }
  level.state.m = level:spawn("miner", 10, 10, { z = 5, facing = { x = 1, y = 0 }, state = first })
  first.mood = "changed"
end

function on_tick()
  local m = level.state.m
  world.state.mood = m.state.mood
  world.state.facing = m.facing.x
end
"#;
    let mut g = game("spawn-options", main);
    let z = g.game.inspect()["things"][0]["z"].as_f64().unwrap();
    assert_eq!(z, 5.0);
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "mood"), format!("{:?}", super::Value::Str(b"calm".to_vec())), "the first state is a copy");
    assert_eq!(state(&mut g, "facing"), format!("{:?}", super::Value::Int(1)));
    let e = start_error(&folder("spawn-z", "function on_level_start(how) level:spawn('miner', 10, 10, 5) end\n"));
    assert!(e.contains("the fourth is a table of options"), "{e}");
}

/// A mistake in `on_world_start` or the first `on_level_start` stops the
/// game rather than its loading; the mended file starts it afresh from its
/// files, not from the half-made start. A state mistake names its tick.
#[test]
fn a_mistake_at_the_start_stops_and_the_mend_starts_afresh() {
    let dir = folder("start-mend", "function on_world_start() world.state.starts = 1 end\nfunction on_level_start(how)\n  local x = nil\n  return x.y\nend\n");
    let mut g = game_in(&dir);
    assert!(halted(&g).contains("main.lua:4:"), "{}", halted(&g));
    assert!(g.game.start_failed());
    std::fs::write(dir.join("main.lua"), "function on_world_start() world.state.starts = (world.state.starts or 0) + 1 end\nfunction on_level_start(how) world.state.level = how end\n").unwrap();
    assert!(matches!(g.reload(&dir.join("main.lua")).unwrap(), crate::session::Reloaded::World));
    assert_eq!(halted(&g), "");
    assert!(!g.game.start_failed());
    assert_eq!(state(&mut g, "starts"), format!("{:?}", super::Value::Int(1)), "afresh, not on top of the stopped start");
    assert_eq!(state(&mut g, "level"), format!("{:?}", super::Value::Str(b"start".to_vec())));
    let mut g = game("state-tick", "function on_tick() if world.tick == 4 then world.state.f = print end end\n");
    steps(&mut g, 4);
    assert!(halted(&g).contains("world.state.f is a function") && halted(&g).contains("(at the end of tick 4)"), "{}", halted(&g));
}

/// A level's name and its places (name, where).
type LevelPlaces<'a> = (&'a str, &'a [(&'a str, [i32; 3])]);

/// A game folder of levels of blocks in `levels/`, each a floor of rock
/// four cells long with these places, and this `main.lua`.
fn levels_folder(name: &str, main: &str, levels: &[LevelPlaces]) -> PathBuf {
    use crate::blocks::{self, Block};
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind, Place};
    use glam::IVec3;
    let dir = std::env::temp_dir().join(format!("voxcade-levels-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.json\" }\n  c.player.spawn = { 2, 2 }\n  c.things.box = {}\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    for (level_name, places) in levels {
        let mut level = GridLevel::new(4, IVec3::new(4, 1, 1));
        level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
        level.cells.fill(Cell { block: 1, turn: Default::default() });
        level.places = places.iter().map(|(n, at)| Place { name: n.to_string(), at: *at, facing: Some([1, 0]) }).collect();
        level.save(&dir.join("levels").join(format!("{level_name}.json"))).unwrap();
    }
    let mut rock = Block::empty(4);
    rock.set(IVec3::ZERO, Some([90, 80, 70])).unwrap();
    rock.save(&blocks::path(&dir, "rock")).unwrap();
    dir
}

/// The levels are the files in `levels/` in the order of their names;
/// `world:go_to` and `world:restart` change at the tick's end, through
/// `on_level_end(why)` and `on_level_start(how)`, `level.state` fresh each
/// time and `world.state` kept; a place sets where the level is entered;
/// `world:win()` ends the run through `on_level_end` and `on_world_end`.
#[test]
fn a_run_goes_from_level_to_level() {
    let main = r#"
function on_world_start() world.state.log = "" end

function on_level_start(how)
  world.state.log = world.state.log .. level.number .. level.name .. ":" .. how .. ";"
  level.state.fresh = (level.state.fresh or 0) + 1
  world.state.fresh = level.state.fresh
  world.state.visits = level.visits
  world.state.places = level.places.east and "east" or "-"
end

function on_level_end(why) world.state.log = world.state.log .. "end:" .. why .. ";" end
function on_world_end(result) world.state.result = result end

function on_tick()
  if world.tick == 2 or world.tick == 4 then world:go_to(level.next) world.state.asked = world.leaving end
  if world.tick == 6 then world:restart() end
  if world.tick == 8 then world:go_to(1, "east") end
  if world.tick == 10 then world:win() end
end
"#;
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
    let dir = levels_folder("run", main, &[("level10", &[]), ("level2", &[]), ("level1", east)]);
    let mut g = game_in(&dir);
    steps(&mut g, 8);
    assert_eq!(halted(&g), "");
    let cam = g.game.sim().cam_target;
    assert_eq!((cam.x, cam.y), (12.5 + 16.0 * 0.0 + 0.0, 2.5), "entered at the place");
    steps(&mut g, 2);
    assert_eq!(state(&mut g, "log"), format!("{:?}", super::Value::Str(b"1level1:start;end:go_to;2level2:go_to;end:go_to;3level10:go_to;end:restart;3level10:restart;end:go_to;1level1:go_to;end:won;".to_vec())));
    assert_eq!(state(&mut g, "result"), format!("{:?}", super::Value::Str(b"won".to_vec())));
    assert_eq!(state(&mut g, "fresh"), format!("{:?}", super::Value::Int(1)), "level.state is fresh on every entry");
    assert_eq!(state(&mut g, "visits"), format!("{:?}", super::Value::Int(2)));
    assert_eq!(state(&mut g, "asked"), format!("{:?}", super::Value::Str(b"go_to".to_vec())));
    assert_eq!(g.game.outcome(), Some(crate::game::Outcome::Won));
}

/// One ask counts in a tick: going on outranks starting over, among equals
/// the first; the ones that did not count are noted with their line.
#[test]
fn one_ask_counts_and_going_on_wins() {
    let main = "function on_tick()\n  if world.tick == 2 then\n    world:restart()\n    world:go_to(level.next)\n    world:lose()\n    world:go_to(level.next)\n  end\n  world.state.at = level.name\nend\n";
    let dir = levels_folder("asks", main, &[("a", &[]), ("b", &[])]);
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(state(&mut g, "at"), format!("{:?}", super::Value::Str(b"b".to_vec())));
    let notes = g.printed();
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[0].starts_with("main.lua:4: world:go_to takes the place of the world:restart"), "{notes:?}");
    assert!(notes[1].starts_with("main.lua:5: world:lose does not count"), "{notes:?}");
}

/// An ask in `on_level_start` counts at once (no tick between); one that
/// never stops is a mistake; asks where there is no level, or where it is
/// left already, and names that do not exist, are mistakes with their
/// line.
#[test]
fn asks_have_their_places_and_their_names() {
    let start = "function on_level_start(how)\n  world.state.log = (world.state.log or '') .. level.name\n  if level.name == 'a' then world:go_to('b') end\nend\n";
    let mut g = game_in(&levels_folder("at-once", start, &[("a", &[]), ("b", &[])]));
    assert_eq!(state(&mut g, "log"), format!("{:?}", super::Value::Str(b"ab".to_vec())), "before the first tick");
    let looping = "function on_level_start(how) world:restart() end\n";
    assert!(start_error(&levels_folder("looping", looping, &[("a", &[])])).contains("levels keep being left"));
    for (name, main, says) in [
        ("in-world-start", "function on_world_start()\n  world:go_to(1)\nend\n", "world:go_to: there is no level to leave yet"),
        ("in-level-end", "function on_tick() world:go_to(2) end\nfunction on_level_end(why)\n  world:restart()\nend\n", "world:restart: the level is being left already"),
        ("no-level", "function on_tick() world:go_to('nowhere') end\n", "world:go_to: no level called 'nowhere' (levels: a, b)"),
        ("no-place", "function on_tick() world:go_to('b', 'west') end\n", "no place called 'west' in level 'b' (its places: start, east)"),
        ("after-last", "function on_tick() if level.name == 'b' then world:go_to(level.next) else world:go_to(2) end end\n", "world:go_to: which level?"),
    ] {
        let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
        let mut g = game_in(&levels_folder(name, main, &[("a", &[]), ("b", east)]));
        steps(&mut g, 3);
        let e = if halted(&g).is_empty() { start_error(&levels_folder(name, main, &[("a", &[]), ("b", east)])) } else { halted(&g) };
        assert!(e.contains(says), "{name}: {e}");
    }
}

/// `level.kept` lasts the run, a level each; a checkpoint is where a
/// restart goes, forgotten when another level is entered; a game kept at a
/// tick and brought back plays on the same across level changes.
#[test]
fn kept_checkpoints_and_snapshots_cross_levels() {
    let main = r#"
function on_level_start(how)
  level.kept.entered = (level.kept.entered or 0) + 1
  world.state["kept_" .. level.name] = level.kept.entered
  world.state.checkpoint = player.state.checkpoint or "-"
end

function on_tick()
  local t = world.tick
  if t == 2 then player.state.checkpoint = "east" world:restart() end
  if t == 4 then world:go_to("b") end
  if t == 6 then world:go_to("a") end
  if t == 8 then world:restart() end
end
"#;
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
    let mut g = game_in(&levels_folder("kept", main, &[("a", east), ("b", &[])]));
    steps(&mut g, 3);
    assert_eq!(state(&mut g, "checkpoint"), format!("{:?}", super::Value::Str(b"east".to_vec())));
    assert_eq!(g.game.sim().cam_target.x, 12.5, "the restart went to the checkpoint");
    let kept_at = g.game.state_hash();
    let snapshot = g.snapshot().unwrap();
    let mut first = Vec::new();
    for _ in 0..8 {
        steps(&mut g, 1);
        first.push(g.game.state_hash());
    }
    assert_eq!(state(&mut g, "kept_a"), format!("{:?}", super::Value::Int(4)));
    assert_eq!(state(&mut g, "checkpoint"), format!("{:?}", super::Value::Str(b"-".to_vec())), "forgotten on the way to b");
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), kept_at);
    for (n, hash) in first.iter().enumerate() {
        steps(&mut g, 1);
        assert_eq!(g.game.state_hash(), *hash, "tick {} after the snapshot played differently", n + 1);
    }
}

/// Where level changes fade, the fade's frames are no ticks: the game
/// holds still, the change comes between out and in, and a run recorded so
/// plays the same where changes cut (`check`).
#[test]
fn a_fade_is_no_tick() {
    use crate::replay::{Recorder, check};
    let main = "function on_tick()\n  if world.tick == 2 then world:go_to(2) end\n  world.state.at = level.name\nend\n";
    let dir = levels_folder("fade", main, &[("a", &[]), ("b", &[])]);
    let mut g = game_in(&dir);
    g.fades = true;
    let mut recorder = Recorder::default();
    let mut fading = 0;
    for _ in 0..90 {
        g.step(&Input::default());
        fading += !g.ticked as u32;
        recorder.record(&g);
    }
    assert!((34..=40).contains(&fading), "{fading} frames of fade (0.3 s out, 0.3 s in)");
    assert_eq!(g.game.sim().tick, 90 - fading as u64, "no ticks while fading");
    assert_eq!(state(&mut g, "at"), format!("{:?}", super::Value::Str(b"b".to_vec())));
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "the run plays the same without the fade");
}

/// The ready-made rules keep their run from level to level (the score, the
/// lives); the player stands where the next level starts.
#[test]
fn the_ready_made_rules_keep_their_run_across_levels() {
    let main = "function on_tick()\n  if world.tick == 2 then player.state.score = 50 end\n  if world.tick == 3 then world:go_to('b') end\n  world.state.at = level.name\nend\n";
    let dir = kit_folder("kit-levels", "", main);
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    for name in ["a", "b"] {
        std::fs::copy(dir.join("world.vox"), dir.join("levels").join(format!("{name}.vox"))).unwrap();
    }
    let mut g = game_in(&dir);
    let lives = g.game.inspect()["player"]["state"]["lives"].clone();
    let start = g.game.focus().map(|b| b.pos).unwrap();
    steps(&mut g, 2);
    g.game.place_player(start.x + 20.0, start.y + 20.0);
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "at"), format!("{:?}", super::Value::Str(b"b".to_vec())));
    assert_eq!(g.game.score(), 50, "the score went along");
    assert_eq!(g.game.inspect()["player"]["state"]["lives"], lives, "the lives went along");
    let at = g.game.focus().map(|b| b.pos).unwrap();
    assert!((at.x - start.x).abs() < 1.0 && (at.y - start.y).abs() < 1.0, "the player at the level's start, not where it stood: {at} vs {start}");
}

/// A level file saved while playing enters the level afresh (`"reload"`);
/// QUIT TO TITLE tells the script (`"quit"`).
#[test]
fn a_reload_and_a_quit_are_told() {
    let main = "function on_level_start(how) world.state.log = (world.state.log or '') .. 'start:' .. how .. ';' end\nfunction on_level_end(why) world.state.log = world.state.log .. 'end:' .. why .. ';' end\nfunction on_world_end(result) world.state.log = world.state.log .. 'world:' .. result end\n";
    let dir = levels_folder("reload-quit", main, &[("a", &[])]);
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let level = crate::grid::GridLevel::read(&dir.join("levels/a.json")).unwrap();
    level.save(&dir.join("levels/a.json")).unwrap();
    assert!(matches!(g.reload(&dir.join("levels/a.json")).unwrap(), crate::session::Reloaded::World));
    g.game.quit();
    assert_eq!(state(&mut g, "log"), format!("{:?}", super::Value::Str(b"start:start;end:reload;start:reload;end:quit;world:quit".to_vec())));
}

/// After a level change every chunk the level before showed is drawn anew;
/// a level's start is found on its own ground, and `level.arrived` says
/// where the player came in.
#[test]
fn a_level_change_shows_the_next_level_where_it_starts() {
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind};
    let main = "function on_level_start(how)\n  world.state['start_' .. level.name] = level.places.start.z\n  world.state.arrived = level.arrived.name .. ' ' .. level.arrived.x\nend\nfunction on_tick() if world.tick == 2 then world:go_to('b', 'east') end end\n";
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 8])];
    let dir = levels_folder("next-start", main, &[("a", &[]), ("b", east)]);
    let mut taller = GridLevel::new(4, glam::IVec3::new(4, 1, 2));
    taller.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    taller.cells.fill(Cell { block: 1, turn: Default::default() });
    taller.places = vec![crate::grid::Place { name: "east".into(), at: [12, 2, 8], facing: None }];
    taller.save(&dir.join("levels/b.json")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    let _ = g.world_mut().take_dirty();
    steps(&mut g, 2);
    assert!(!g.world_mut().take_dirty().is_empty(), "the chunks drawn anew");
    let (a, b) = (state(&mut g, "start_a"), state(&mut g, "start_b"));
    assert_ne!(a, b, "each level's start on its own ground: {a} {b}");
    assert_eq!(state(&mut g, "arrived"), format!("{:?}", super::Value::Str(b"east 12.5".to_vec())));
}

/// The shooter's player stands on a world the script builds after a
/// restart too, not in it.
#[test]
fn a_restart_stands_the_player_on_what_was_built() {
    let main = "function on_level_start(how)\n  level:fill(0, 0, 0, 511, 511, 3, \"#c2a878\")\nend\nfunction on_tick() if world.tick == 2 then world:restart() end end\n";
    let shooter = kit_folder("built-restart", "", main);
    let conf = std::fs::read_to_string(shooter.join("conf.lua")).unwrap().replace("c.world.file = \"world.vox\"", "c.world.size = { 512, 512, 48 }");
    std::fs::write(shooter.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&shooter);
    steps(&mut g, 10);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.focus().map(|b| b.pos.z), Some(4.0), "on the sand after the restart");
}

/// A platformer level of blocks in `levels/a.json`: a floor of rock, the
/// hero's marker at x 3 and these things more; `conf.lua` with these lines
/// more.
fn platformer_levels(name: &str, main: &str, conf: &str, more: &[(&str, [i32; 3])]) -> PathBuf {
    let dir = levels_folder(name, main, &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = {{ hp = 3, lives = 3, taken = 0 }}\n  c.world.blocks = {{ file = \"level.json\" }}\n  c.things.hero = {{ behaviour = \"hero\", height = 6 }}\n  c.things.door = {{ behaviour = \"prop\", stands = true }}\n{conf}end\n")).unwrap();
    let path = dir.join("levels/a.json");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    for (kind, at) in std::iter::once(&("hero", [3, 2, 4])).chain(more) {
        level.things.push(crate::grid::Thing { kind: kind.to_string(), at: *at });
    }
    level.save(&path).unwrap();
    dir
}

/// A level without a place called start starts where its ready-made rules
/// stand the player: a restart puts the platformer's hero back on its
/// marker, facing as its rules turn it (along the level, not at the
/// camera), and `level.places.start` is there.
#[test]
fn a_restart_without_a_start_place_goes_back_to_the_marker() {
    let main = "function on_level_start(how) world.state.start = level.places.start.x end\nfunction on_tick() if world.tick == 30 then world:restart() end end\n";
    let dir = platformer_levels("marker-restart", main, "", &[]);
    let mut g = game_in(&dir);
    let start = g.game.focus().map(|b| b.pos).unwrap();
    for _ in 0..20 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert!(g.game.focus().unwrap().pos.x > start.x + 2.0, "it ran");
    steps(&mut g, 15);
    assert_eq!(halted(&g), "");
    let back = g.game.focus().unwrap();
    assert_eq!((back.pos.x, back.pos.y), (start.x, start.y), "back on its marker");
    assert_eq!(back.look.y, 0, "facing along the level: {}", back.look);
    assert_eq!(state(&mut g, "start"), format!("{:?}", super::Value::Float(3.5f64.to_bits())));
}

/// A door the code answers ends the run: at the tick's end, through the
/// fade, the script told once; the outcome shows when it was told.
#[test]
fn a_win_goes_through_the_fade() {
    let main = "function on_tick() if world.tick == 5 then world:win() end end\nfunction on_level_end(why) world.state.log = (world.state.log or '') .. 'end:' .. why .. ';' end\nfunction on_world_end(result) world.state.log = world.state.log .. 'world:' .. result end\n";
    let dir = platformer_levels("rules-end", main, "", &[("door", [3, 2, 4])]);
    let mut g = game_in(&dir);
    g.fades = true;
    let mut ticks = 0;
    while !g.fading() && ticks < 120 {
        steps(&mut g, 1);
        ticks += 1;
    }
    assert!(g.fading(), "the fade out of the win");
    assert_eq!(g.game.outcome(), None, "not over before the script is told");
    while g.fading() {
        steps(&mut g, 1);
    }
    assert_eq!(g.game.outcome(), Some(crate::game::Outcome::Won));
    assert_eq!(state(&mut g, "log"), format!("{:?}", super::Value::Str(b"end:won;world:won".to_vec())));
}

/// A mistake in `on_level_end` stops the game before anything changed:
/// the ask waits, and once the file is mended the level is left.
#[test]
fn an_ask_waits_for_a_mended_level_end() {
    let main = "function on_tick() if world.tick == 2 then world:go_to('b') end end\nfunction on_level_end(why) error('broken') end\n";
    let dir = levels_folder("mended-end", main, &[("a", &[]), ("b", &[])]);
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert!(halted(&g).contains("broken"), "{}", halted(&g));
    std::fs::write(dir.join("main.lua"), "function on_level_end(why) end\nfunction on_level_start(how) world.state.at = level.name .. ':' .. how end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "at"), format!("{:?}", super::Value::Str(b"b:go_to".to_vec())));
}

/// The run's end is in the snapshot: a replay whose win falls in the
/// check's guard (a second played twice) runs the same; asks after the end
/// count no more (a standing `world:win()` ends the level once).
#[test]
fn the_end_is_in_the_snapshot_and_asks_after_it_count_no_more() {
    use crate::replay::{Recorder, check};
    let main = "function on_tick()\n  if world.tick >= 330 then world:win() end\nend\nfunction on_level_end(why) world.state.ends = (world.state.ends or 0) + 1 end\n";
    let dir = levels_folder("end-snapshot", main, &[("a", &[])]);
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..400 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(state(&mut g, "ends"), format!("{:?}", super::Value::Int(1)), "once, though asked every tick");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap());
}

/// The player is a thing that belongs to no level: a game's
/// `c.player.kind` is its figure, standing where each level starts and
/// walked like any thing the script moves, the same `player` from level to
/// level with its state; the level's things do not list it, it has no id,
/// and a snapshot keeps it.
#[test]
fn the_player_is_a_thing_from_level_to_level() {
    let main = r#"
function on_world_start() player.state.coins = 3 end

function on_level_start(how)
  world.state.seen = (world.state.seen or "") .. level.name .. ":" .. player.kind .. ":" .. tostring(player.id) .. ":" .. level:count() .. ":" .. level:count("hero") .. ":" .. player.x .. ";"
  world.state.coins = player.state.coins
end

function on_tick()
  player:walk({ x = 1, y = 0 }, 30)
  if level.name == "a" then
    world.state.same = world.players[1] == player and tostring(player) == "player" and level:thing(level:things()[1].id) ~= player
  end
  if world.tick == 20 then world.state.walked = player.x end
  if world.tick == 21 then world:go_to("b", "east") end
end
"#;
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
    let dir = levels_folder("player-thing", main, &[("a", &[]), ("b", east)]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.json\" }\n  c.player.spawn = { 2, 2 }\n  c.player.kind = \"hero\"\n  c.things.hero = {}\n  c.things.box = {}\nend\n").unwrap();
    let path = dir.join("levels/a.json");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    level.things.push(crate::grid::Thing { kind: "box".into(), at: [10, 2, 4] });
    level.save(&path).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 10);
    let snapshot = g.snapshot().unwrap();
    let mut first = Vec::new();
    for _ in 0..20 {
        steps(&mut g, 1);
        first.push(g.game.state_hash());
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "seen"), format!("{:?}", super::Value::Str(b"a:hero:nil:1:0:2.0;b:hero:nil:0:0:12.5;".to_vec())));
    assert_eq!(state(&mut g, "same"), format!("{:?}", super::Value::Bool(true)));
    assert_eq!(state(&mut g, "coins"), format!("{:?}", super::Value::Int(3)), "its state went along");
    assert!(!state(&mut g, "walked").contains("2.0"), "it walked: {}", state(&mut g, "walked"));
    let x = g.game.focus().unwrap().pos.x;
    assert!(x > 12.5 && x < 16.0, "the camera follows it, from b's east on: {x}");
    g.restore(&snapshot);
    for (n, hash) in first.iter().enumerate() {
        steps(&mut g, 1);
        assert_eq!(g.game.state_hash(), *hash, "tick {} after the restore", n + 11);
    }
}

/// The player cannot be removed; without a figure it stands nowhere and
/// says so; with ready-made rules it is their player, moved by them.
#[test]
fn the_player_is_the_rules_figure_or_none() {
    let mut g = game("remove-player", "function on_tick() player:remove() end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("player:remove: the player belongs to no level"), "{}", halted(&g));
    let mut g = game("no-figure", "function on_tick()\n  world.state.score = player.state.score\n  world.state.x = player.x\nend\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("the player has no figure in this game (c.player.kind names the kind of thing it is)"), "{}", halted(&g));

    let main = "function on_tick()\n  world.state.x = player.x\n  world.state.kind = player.kind\n  if world.tick == 3 then player:walk({ x = 1, y = 0 }, 10) end\nend\n";
    let dir = platformer_levels("platformer-player", main, "", &[]);
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let x = g.game.focus().unwrap().pos.x as f64;
    assert_eq!(state(&mut g, "x"), format!("{:?}", super::Value::Float(x.to_bits())), "the hero");
    assert_eq!(state(&mut g, "kind"), format!("{:?}", super::Value::Str(b"hero".to_vec())));
    steps(&mut g, 1);
    assert!(halted(&g).contains("thing:walk: a hero moves by its ready-made rules"), "{}", halted(&g));

    let mut g = game_in(&kit_folder("shooter-player", "", "function on_tick()\n  world.state.x = player.x\n  world.state.kind = tostring(player.kind)\n  if world.tick == 3 then player:walk({ x = 1, y = 0 }, 10) end\nend\n"));
    steps(&mut g, 2);
    let x = g.game.focus().unwrap().pos.x as f64;
    assert_eq!(state(&mut g, "x"), format!("{:?}", super::Value::Float(x.to_bits())), "the shooter's own");
    assert_eq!(state(&mut g, "kind"), format!("{:?}", super::Value::Str(b"nil".to_vec())));
    steps(&mut g, 1);
    assert!(halted(&g).contains("thing:walk: the player moves by its ready-made rules"), "{}", halted(&g));
}

/// A pickup the code takes on touch counts into `player.state.inventory`
/// (a full stack takes no more); `world.items` lists the items in their
/// order.
#[test]
fn pickups_count_into_the_inventory() {
    let main = r#"
function on_touch(a, b)
  if a == player and b.kind == "key" then
    player:take(b.props.item)
    player.state.score = player.state.score + b.props.worth
    b:remove()
  end
end
function on_tick()
  world.state.keys = player.state.inventory.key
  world.state.first = world.items[1].name .. world.items[2].stack
end
"#;
    let conf = "  c.items.key = { stack = 2, order = 2 }\n  c.items.amulet = { order = 1 }\n  c.things.key = { behaviour = \"pickup\", props = { item = \"key\", worth = 50 } }\n";
    let dir = platformer_levels("inventory", main, conf, &[("key", [6, 2, 4]), ("key", [8, 2, 4]), ("key", [10, 2, 4])]);
    let mut g = game_in(&dir);
    for _ in 0..40 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "keys"), format!("{:?}", super::Value::Int(2)), "two of three: its stack");
    assert_eq!(state(&mut g, "first"), format!("{:?}", super::Value::Str(b"amulet2".to_vec())));
    assert_eq!(g.game.score(), 150, "all three taken");
}

/// An inventory holds items of `c.items` by name, from 0 to their stack,
/// the player's and a thing's alike; a mistake stops the game at the end of
/// the tick, naming it.
#[test]
fn inventories_hold_items_by_name_up_to_their_stack() {
    for (name, line, says) in [
        ("unknown", "player.state.inventory.kye = 1", "player.state.inventory.kye: no item called 'kye' (c.items: key)"),
        ("too-many", "player.state.inventory.key = 3", "player.state.inventory.key = 3: from 0 to 2 (c.items.key.stack)"),
        ("half", "player.state.inventory.key = 0.5", "player.state.inventory.key is 0.5: a count is a whole number"),
        ("not-a-table", "player.state.inventory = 1", "player.state.inventory is an integer: an inventory is a table"),
        ("chest", "level.state.chest.state.inventory = { gold = 1 }", "state.inventory.gold: no item called 'gold' (c.items: key)"),
    ] {
        let main = format!("function on_level_start(how) level.state.chest = level:spawn('chest', 8, 8) end\nfunction on_tick() if world.tick == 2 then {line} end end\n");
        let dir = folder(&format!("inventory-{name}"), &main);
        let conf = CONF.replace("  c.things.gem = { hp = 2 }\n", "  c.things.gem = { hp = 2 }\n  c.items.key = { stack = 2 }\n  c.things.chest = {}\n");
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 1);
        assert_eq!(halted(&g), "", "{name}: an empty inventory to begin with");
        steps(&mut g, 1);
        assert!(halted(&g).contains(says) && halted(&g).contains("(at the end of tick 2)"), "{name}: {}", halted(&g));
    }
}

/// What names the player's kind or an item is checked when the game loads.
#[test]
fn the_players_kind_and_the_items_are_checked_at_load() {
    let dir = platformer_levels("bad-item", "", "  c.items.key = {}\n  c.things.key = { behaviour = \"pickup\", item = \"kye\" }\n", &[]);
    assert!(start_error(&dir).contains("c.things.key.item is no more: what a pickup gives is your code's"), "{}", start_error(&dir));
    let dir = platformer_levels("kit-player-kind", "", "  c.player.kind = \"hero\"\n", &[]);
    assert!(start_error(&dir).contains("c.player.kind = \"hero\": the ready-made rules (c.kit) bring their own player"), "{}", start_error(&dir));
    for (name, line, says) in [
        ("unknown-kind", "  c.player.kind = \"hro\"\n", "c.player.kind = \"hro\": no kind of thing called that (c.things: gem, miner)"),
        ("no-stack", "  c.items.key = { stack = 0 }\n", "c.items.key.stack = 0: an inventory holds at least one"),
    ] {
        let dir = folder(name, "");
        std::fs::write(dir.join("conf.lua"), CONF.replace("  c.things.miner = {}\n", &format!("  c.things.miner = {{}}\n{line}"))).unwrap();
        assert!(start_error(&dir).contains(says), "{name}: {}", start_error(&dir));
    }
}

/// What the review of stage 4 found: a level file saved while playing
/// keeps the player where its figure stands (not where the camera looks);
/// a `conf.lua` that takes the player's kind away is refused, the old one
/// kept; an item put back in `conf.lua` mends the inventory it stopped.
#[test]
fn the_player_outlasts_reloads_and_config_changes() {
    let main = "function on_level_start(how)\n  level.state.box = level:spawn('box', 12, 2)\n  level.camera:follow(level.state.box)\nend\nfunction on_tick() player:walk({ x = 1, y = 0 }, 20) end\n";
    let dir = levels_folder("player-reload", main, &[("a", &[])]);
    let conf = "function conf(c)\n  c.world.blocks = { file = \"level.json\" }\n  c.player.spawn = { 2, 2 }\n  c.player.kind = \"hero\"\n  c.things.hero = {}\n  c.things.box = {}\n  c.items.key = {}\nend\n";
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 10);
    let x = g.game.inspect()["player"]["x"].as_f64().unwrap();
    let level = crate::grid::GridLevel::read(&dir.join("levels/a.json")).unwrap();
    level.save(&dir.join("levels/a.json")).unwrap();
    g.reload(&dir.join("levels/a.json")).unwrap();
    assert_eq!(g.game.inspect()["player"]["x"].as_f64(), Some(x), "where the player stood, not at the box");

    std::fs::write(dir.join("conf.lua"), conf.replace("  c.things.hero = {}\n", "")).unwrap();
    let e = g.reload(&dir.join("conf.lua")).err().map(|e| format!("{e:#}")).unwrap_or_default();
    assert!(e.contains("c.player.kind = \"hero\": no kind of thing called that"), "{e}");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "", "the old config kept");

    std::fs::write(dir.join("main.lua"), "function on_tick() player.state.inventory.key = 1 end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("  c.items.key = {}\n", "")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    steps(&mut g, 1);
    assert!(halted(&g).contains("player.state.inventory.key: there are no items"), "{}", halted(&g));
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    assert_eq!(halted(&g), "", "the item back, the game goes on");
}

/// A game of two levels with a player's figure and two save slots.
fn saves_folder(name: &str, main: &str) -> PathBuf {
    let dir = levels_folder(name, main, &[("a", &[]), ("b", &[])]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.json\" }\n  c.player.spawn = { 2, 2 }\n  c.player.kind = \"hero\"\n  c.things.hero = {}\n  c.things.box = {}\n  c.saves = { slots = 2 }\nend\n").unwrap();
    dir
}

const SAVE_AND_LOAD: &str = r#"
function on_world_start() world.state.coins = 0 end

function on_level_start(how)
  world.state.log = (world.state.log or "") .. level.name .. ":" .. how .. ";"
  if how ~= "load" then level.state.seen = 0 end
end

function on_tick()
  player:walk({ x = 1, y = 0 }, 20)
  level.state.seen = level.state.seen + 1
  world.state.coins = world.state.coins + 1
  if world.tick == 10 then world:save(1) end
  if world.tick == 20 then world:go_to("b") end
  if world.tick == 30 and not world.state.loaded then
    world.state.loaded = true
    world.state.slot = world.saves[1].level .. world.saves[1].seconds
    world:load(1)
  end
end
"#;

/// `world:save` keeps the run at the tick's end and `world:load` brings it
/// back like a level change: the states as saved, the level it was saved
/// in with its `level.state`, the player where it stood, the time.
#[test]
fn a_save_brings_the_run_back_as_it_was() {
    let mut g = game_in(&saves_folder("save-load", SAVE_AND_LOAD));
    steps(&mut g, 10);
    let saved_at = g.game.inspect()["player"]["x"].clone();
    steps(&mut g, 20);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["at"]["name"], "a", "back in the level it was saved in");
    assert_eq!(state(&mut g, "coins"), format!("{:?}", super::Value::Int(10)), "world.state as saved");
    assert_eq!(state(&mut g, "log"), format!("{:?}", super::Value::Str(b"a:start;a:load;".to_vec())), "world.state as saved, then the load told");
    assert_eq!(g.game.sim().tick, 10, "the time as saved");
    assert_eq!(g.game.inspect()["player"]["x"], saved_at, "the player where it stood");
    assert_eq!(g.game.inspect()["level"]["seen"], 10, "level.state as saved");
    assert!(!g.slots.on_disk_now(), "a game without a window keeps its saves for the run only");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "coins"), format!("{:?}", super::Value::Int(11)), "and it goes on from there");
}

/// Slots are checked where they are named: one the game lacks, an empty
/// one to load, a game without slots.
#[test]
fn save_slots_are_checked_where_they_are_named() {
    for (name, line, says) in [
        ("no-slot", "world:save(3)", "world:save: no slot 3: the game's are 1 to 2 (c.saves.slots)"),
        ("empty-slot", "world:load(2)", "world:load: slot 2 is empty (world.saves[2] tells before)"),
    ] {
        let dir = saves_folder(&format!("slots-{name}"), &format!("function on_tick() {line} end\n"));
        let mut g = game_in(&dir);
        steps(&mut g, 1);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
    let mut g = game("no-slots", "function on_tick() world:save(1) end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("the game has no save slots"), "{}", halted(&g));
}

/// A run that saves and loads plays back the same from its replay, and so
/// does one that began from a save (LOAD in the pause menu); neither
/// reads or writes the player's slots.
#[test]
fn replays_carry_their_saves() {
    use crate::replay::{Recorder, check};
    let dir = saves_folder("replay-saves", SAVE_AND_LOAD);
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..400 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    let replay = recorder.replay(&g).unwrap();
    // The loaded run reaches tick 30 again: it loads every 20 ticks.
    assert_eq!(replay.loads.len(), 19, "every save it loaded");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/saves.toml")).unwrap();
    let text = g.slots.read(1).unwrap().to_string();

    let mut from = Session::<LuaGame>::load_with(&dir, crate::saves::Slots::holding([(1, text.clone())].into())).unwrap();
    from.start_from(&text).unwrap();
    assert_eq!(from.game.sim().tick, 10);
    let mut recorder = Recorder::default();
    for _ in 0..400 {
        from.step(&Input::default());
        recorder.record(&from);
    }
    let replay = recorder.replay(&from).unwrap();
    assert!(replay.from.is_some(), "it began from the save");
    replay.save(&dir.join("replays/from.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "both play the same");
}

/// A save of an older version of the game is handed to `on_save_loaded`
/// before its level is entered; one from the shooter keeps the level's
/// changed voxels and the ready-made rules' run.
#[test]
fn a_save_outlives_a_new_version_and_keeps_what_changed() {
    let main = "function on_tick()\n  if world.tick == 2 then\n    level:set_voxel(40, 40, 0, 0)\n    player.state.score = 120\n    world:save(1)\n  end\nend\nfunction on_save_loaded(version) world.state.from = version end\nfunction on_level_start(how) world.state.hole = level:voxel(40, 40, 0) end\n";
    let dir = kit_folder("save-version", "  c.saves = { slots = 1 }", main);
    let mut g = game_in(&dir);
    let before = g.game.sim().world.get(glam::IVec3::new(40, 40, 0));
    assert_ne!(before, 0, "the ground there to begin with");
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    let text = g.slots.read(1).unwrap().to_string();
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap().replace("c.saves = { slots = 1 }", "c.saves = { slots = 1, version = 2 }");
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    g.start_from(&text).unwrap();
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "from"), format!("{:?}", super::Value::Int(1)), "told the save's version");
    // A save of the same version is told too.
    std::fs::write(dir.join("conf.lua"), std::fs::read_to_string(dir.join("conf.lua")).unwrap().replace("version = 2", "version = 1")).unwrap();
    let mut same = game_in(&dir);
    same.start_from(&text).unwrap();
    assert_eq!(state(&mut same, "from"), format!("{:?}", super::Value::Int(1)), "every load is told");
    assert_eq!(state(&mut g, "hole"), format!("{:?}", super::Value::Int(0)), "the hole is back");
    assert_eq!(g.game.score(), 120, "the score kept");
}

/// What the review of stage 5 found: a load asked in `on_level_start`
/// works (the first one too: going on from a save as the game starts); a
/// save in the first `on_level_start` is in the replay's slots, so `check`
/// plays it the same; a load brings the random numbers back as they were.
#[test]
fn loads_and_saves_work_from_on_level_start_too() {
    use crate::replay::{Recorder, check};
    let main = r#"
function on_level_start(how)
  world.state.how = (world.state.how or "") .. how .. ";"
  if how == "start" then world:save(1) end
  if how == "go_to" and world.saves[1] then world:load(1) end
end

function on_tick()
  world.state.saw = world.saves[1] and world.saves[1].level or "none"
  world.state.roll = math.random(1000000)
  if world.tick == 8 then world:go_to("b") end
end
"#;
    let dir = saves_folder("load-at-start", main);
    let mut g = game_in(&dir);
    for _ in 0..7 {
        steps(&mut g, 1);
    }
    let roll = state(&mut g, "roll");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["at"]["name"], "a", "on_level_start of b loaded the save");
    assert_eq!(state(&mut g, "how"), format!("{:?}", super::Value::Str(b"start;load;".to_vec())));
    assert_eq!(g.game.sim().tick, 0, "the time of the save: before the first tick");

    // The same random numbers again from the save on.
    for _ in 0..7 {
        steps(&mut g, 1);
    }
    assert_eq!(state(&mut g, "roll"), roll, "math.random goes on as it did");

    // The autosave is in the replay's slots as the run began.
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..300 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/autosave.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap());

    // A game that starts with a save in slot 1 goes on from it at once.
    let text = g.slots.read(1).unwrap().to_string();
    std::fs::write(dir.join("main.lua"), "function on_level_start(how)\n  world.state.how = (world.state.how or '') .. how .. ';'\n  if how == 'start' and world.saves[1] then world:load(1) end\nend\n").unwrap();
    let g = Session::<LuaGame>::load_with(&dir, crate::saves::Slots::holding([(1, text)].into())).unwrap();
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["state"]["how"], "start;load;", "the save's world.state, then the load told");
}

/// A game of one level of blocks (16 voxels long) with the player's figure
/// at x 2, a door at x 12 and these lines more in `conf.lua`.
fn events_folder(name: &str, main: &str, conf: &str) -> PathBuf {
    let dir = levels_folder(name, main, &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.blocks = {{ file = \"level.json\" }}\n  c.player.spawn = {{ 2, 2 }}\n  c.player.kind = \"hero\"\n  c.things.hero = {{ hp = 3 }}\n  c.things.door = {{}}\n  c.things.box = {{ hp = 2 }}\n{conf}end\n")).unwrap();
    let path = dir.join("levels/a.json");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    level.things.push(crate::grid::Thing { kind: "door".into(), at: [12, 2, 4] });
    level.save(&path).unwrap();
    // A solid floor four voxels deep.
    let mut rock = crate::blocks::Block::empty(4);
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..4 {
                rock.set(glam::IVec3::new(x, y, z), Some([90, 80, 70])).unwrap();
            }
        }
    }
    rock.save(&crate::blocks::path(&dir, "rock")).unwrap();
    dir
}

/// Touches are told once when they begin and once when they end, the
/// player first; what touches where the level begins, or where
/// `thing:place` puts a thing, touches without `on_touch`.
#[test]
fn touches_begin_and_end_once() {
    let main = r#"
function on_level_start(how)
  -- a box on the player's spot: touching from the start, told nothing
  level:spawn("box", player.x, player.y)
end

function on_touch(a, b)
  world.state.log = (world.state.log or "") .. "touch " .. tostring(a) .. " " .. b.kind .. " " .. world.tick .. ";"
end

function on_touch_end(a, b)
  world.state.log = (world.state.log or "") .. "leave " .. tostring(a) .. " " .. b.kind .. " " .. world.tick .. ";"
end

function on_tick()
  if world.tick <= 40 then player:walk({ x = 1, y = 0 }, 30) end
  if world.tick > 60 and world.tick <= 100 then player:walk({ x = -1, y = 0 }, 30) end
  -- put back onto the door: no touch told
  if world.tick == 120 then player:place(12.5, 2.5) end
end
"#;
    let mut g = game_in(&events_folder("touches", main, ""));
    steps(&mut g, 130);
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    assert!(log.starts_with("leave player box 9;"), "the box from the start is left, never touched: {log}");
    assert_eq!((log.matches("touch player door").count(), log.matches("leave player door").count()), (1, 1), "{log}");
    assert!(!log.contains("door 120") && !log.contains("door 121"), "placed onto the door: no touch told: {log}");
}

/// A thing's death is told once when its hit points run out, by whoever
/// (here the script); it is no longer counted or listed; hit points back,
/// it lives again and may die again.
#[test]
fn deaths_are_told_once_and_things_live_again() {
    let main = r#"
function on_level_start(how) level.state.box = level:spawn("box", 8, 2) end

function on_death(thing, by)
  world.state.deaths = (world.state.deaths or 0) + 1
  world.state.kind = thing.kind
end

function on_tick()
  local box = level.state.box
  if world.tick == 2 then box.state.hp = 0 end
  if world.tick == 4 then world.state.counted = level:count("box") end
  if world.tick == 5 then box.state.hp = 2 end
  if world.tick == 7 then world.state.back = level:count("box") end
  if world.tick == 8 then box.state.hp = -1 end
end
"#;
    let mut g = game_in(&events_folder("deaths", main, ""));
    steps(&mut g, 12);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "deaths"), format!("{:?}", super::Value::Int(2)), "once each time");
    assert_eq!(state(&mut g, "counted"), format!("{:?}", super::Value::Int(0)), "the dead are not counted");
    assert_eq!(state(&mut g, "back"), format!("{:?}", super::Value::Int(1)), "alive again");
}

/// The ready-made rules' events reach the script: the shooter's rounds hit
/// and kill a soldier (`on_hit`, `on_death` naming its kind though it is
/// gone); an item taken by the code (`on_pickup`, counted in first).
#[test]
fn the_kits_events_reach_the_script() {
    let main = r#"
function on_level_start(how) level:spawn("rifleman", player.x, player.y - 24) end
function on_hit(thing, by, damage) world.state.hits = (world.state.hits or 0) + damage; world.state.by = tostring(by) end
function on_death(thing, by) world.state.dead = thing.kind .. " " .. math.floor(thing.y) end
"#;
    let mut g = game_in(&kit_folder("kit-events", "  c.things.rifleman = { behaviour = \"soldier\", weapon = \"enemy_rifle\" }", main));
    let fire = g.actions().iter().position(|a| *a == "fire").unwrap();
    for _ in 0..240 {
        g.step(&Input { actions: 1 << fire, ..Input::default() });
    }
    assert_eq!(halted(&g), "");
    assert!(state(&mut g, "hits").contains("Int"), "hit: {}", state(&mut g, "hits"));
    assert_eq!(state(&mut g, "by"), format!("{:?}", super::Value::Str(b"player".to_vec())));
    assert!(state(&mut g, "dead").contains("114, 105, 102, 108, 101, 109, 97, 110"), "the rifleman's death: {}", state(&mut g, "dead"));

    let main = "function on_touch(a, b) if b.kind == 'key' then player:take('key') b:remove() end end\nfunction on_pickup(thing, item) world.state.got = tostring(thing) .. ' ' .. item .. ' ' .. player.state.inventory[item] end\n";
    let conf = "  c.items.key = { stack = 3 }\n  c.things.key = { behaviour = \"pickup\" }\n";
    let mut g = game_in(&platformer_levels("kit-pickup-event", main, conf, &[("key", [6, 2, 4])]));
    for _ in 0..20 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert_eq!(state(&mut g, "got"), format!("{:?}", super::Value::Str(b"player key 1".to_vec())));
}

/// A held level (`level.state.held`) stops the rules, physics and touches;
/// on_tick and the time go on. The action called interact acts on what
/// the player touches (`on_interact`), and that press is used up.
#[test]
fn holds_and_interactions() {
    let main = r#"
function on_tick()
  level.state.held = world.tick >= 10 and world.tick < 30
  player:walk({ x = 1, y = 0 }, 30)
  if world.tick == 10 then world.state.before = player.x end
  if world.tick == 29 then world.state.held_x = player.x end
  world.state.pressed = input:pressed("interact") or world.state.pressed
end

function on_interact(thing, by)
  world.state.used = thing.kind .. " " .. tostring(by)
end
"#;
    let dir = events_folder("holds", main, "  c.actions = { \"interact\" }\n  c.keys = { interact = { \"E\" } }\n");
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "before"), state(&mut g, "held_x"), "nothing moved while held");
    assert_eq!(g.game.sim().tick, 30, "the time went on");
    // On to the door, then press.
    steps(&mut g, 60);
    g.step(&Input { actions: 1, ..Input::default() });
    assert_eq!(state(&mut g, "used"), format!("{:?}", super::Value::Str(b"door player".to_vec())));
    assert_eq!(state(&mut g, "pressed"), "", "the press went to on_interact");
}

/// The player's invulnerability with ready-made rules: ticks, or true
/// until set back.
#[test]
fn the_player_can_be_kept_from_hits() {
    let main = "function on_tick()\n  if world.tick == 1 then player.state.invulnerable = true end\n  if world.tick == 2 then world.state.a = player.state.invulnerable; player.state.invulnerable = 30 end\n  if world.tick == 3 then world.state.b = player.state.invulnerable end\nend\n";
    let mut g = game_in(&kit_folder("invulnerable", "", main));
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "a"), format!("{:?}", super::Value::Bool(true)));
    assert_eq!(state(&mut g, "b"), format!("{:?}", super::Value::Int(29)), "counted down a tick");
}

/// A door does nothing by itself; one the player stands on from the start
/// is not touched (told nothing); run into, it is.
#[test]
fn a_door_does_nothing_by_itself() {
    let main = "function on_touch(a, b) if b.kind == 'door' then world.state.at = (world.state.at or '') .. b.name .. ';' end end\n";
    let dir = platformer_levels("door-touch", main, "", &[("door", [3, 2, 4]), ("door", [10, 2, 4])]);
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(g.game.outcome(), None, "the door does not win");
    assert_eq!(state(&mut g, "at"), "", "touching from the start: told nothing");
    for _ in 0..40 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert_eq!((g.game.outcome(), state(&mut g, "at")), (None, format!("{:?}", super::Value::Str(b"door_2;".to_vec()))));
}

/// The directions are pressed and released like actions: one step a press.
#[test]
fn directions_are_pressed_like_actions() {
    let main = "function on_tick()\n  if input:pressed('right') then world.state.steps = (world.state.steps or 0) + 1 end\n  if input:released('right') then world.state.ups = (world.state.ups or 0) + 1 end\n  world.state.held = input:held('right')\nend\n";
    let mut g = game("directions", main);
    let right = Input { right: true, ..Input::default() };
    for input in [right, right, right, Input::default(), right, Input::default()] {
        g.step(&input);
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "steps"), format!("{:?}", super::Value::Int(2)), "two presses, however long held");
    assert_eq!(state(&mut g, "ups"), format!("{:?}", super::Value::Int(2)));
}

/// thing:hurt lowers the script's kinds' hit points and tells on_hit in the
/// next tick, then on_death; the ready-made rules take it as their own hits
/// (a soldier falls). thing:take counts an item in, up to its stack, and
/// tells on_pickup.
#[test]
fn the_script_hurts_and_gives_as_the_rules_do() {
    let main = r#"
function on_level_start(how) level.state.box = level:spawn("box", 8, 2) end
function on_hit(thing, by, damage) world.state.log = (world.state.log or "") .. "hit " .. thing.kind .. " " .. tostring(by) .. " " .. damage .. " " .. world.tick .. ";" end
function on_death(thing, by) world.state.log = (world.state.log or "") .. "death " .. thing.kind .. " " .. world.tick .. ";" end
function on_pickup(thing, item) world.state.log = (world.state.log or "") .. "pickup " .. tostring(thing) .. " " .. item .. " " .. world.tick .. ";" end
function on_tick()
  if world.tick == 2 then level.state.box:hurt(2, player) end
  if world.tick == 4 then world.state.took = player:take("key", 5) end
end
"#;
    let mut g = game_in(&events_folder("hurt-take", main, "  c.items.key = { stack = 3 }\n"));
    steps(&mut g, 6);
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    assert_eq!(log, "hit box player 2 3;death box 3;pickup player key 5;", "{log}");
    assert_eq!(state(&mut g, "took"), format!("{:?}", super::Value::Int(3)), "up to its stack");
    assert_eq!(g.game.inspect()["player"]["state"]["inventory"]["key"], 3);

    let main = r#"
function on_level_start(how) level.state.man = level:spawn("rifleman", player.x + 40, player.y + 40) end
function on_death(thing, by) world.state.fell = thing.kind .. " " .. tostring(by) end
function on_tick() if world.tick == 2 then level.state.man:hurt(5, player) end end
"#;
    let mut g = game_in(&kit_folder("kit-hurt", "  c.things.rifleman = { behaviour = \"soldier\", weapon = \"enemy_rifle\" }", main));
    // His fall holds the game still a few ticks (hit stop) before it is told.
    steps(&mut g, 20);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "fell"), format!("{:?}", super::Value::Str(b"rifleman player".to_vec())));
}

/// A replay recorded before an action came carries no key for it: played
/// back, the game's texts show it unbound (as where nobody plays) and the
/// game draws on (Voxkeep's "{restart} STARTS THE ROOM AGAIN" stopped it
/// on an older replay, in every picture drawn of it).
#[test]
fn an_older_replay_shows_an_action_it_lacks_unbound() {
    use crate::replay::{Playback, Replay, replay_session};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/voxkeep");
    let replay = Replay::load(&dir.join("replays/tests/climb_to_the_door.toml")).unwrap();
    assert!(replay.controls.iter().all(|c| !c.controls.keys.contains_key("restart")), "the replay is from before the restart key");
    let mut g = replay_session::<LuaGame>(&dir, &replay).unwrap();
    let mut p = Playback::new(&replay, &mut g).unwrap();
    for _ in 0..60 {
        let input = p.next_input(&mut g);
        g.step(&input);
        let _ = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    }
    assert_eq!(halted(&g), "");
}

/// A scenario (`scenarios/NAME.lua`) begins a run late: after the start,
/// in its level at its place, its `setup` setting the states; a console
/// line runs between two ticks and answers. A run begun so, with lines run
/// in the middle of it, is recorded and plays back the same; the
/// scenario's file counts for the replay's data.
#[test]
fn a_scenario_and_console_lines_are_part_of_the_run() {
    use crate::replay::{Recorder, Replay, check};
    let main = "function on_world_start() world.state.day = 1 end\nfunction on_level_start(how) level.state.how = how end\nfunction on_tick() world.state.day = world.state.day + 1 end\n";
    let dir = levels_folder("scenario", main, &[("a", &[]), ("b", &[("gate", [8, 2, 4])])]);
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    let late = "return {\n  about = \"Day 30 in b\",\n  level = \"b\", place = \"gate\",\n  setup = function()\n    world.state.day = 30\n    level.state.set = true\n  end,\n}\n";
    std::fs::write(dir.join("scenarios/late.lua"), late).unwrap();
    let mut g = game_in(&dir);
    assert_eq!(g.scenarios().unwrap(), vec![("late".to_string(), "Day 30 in b".to_string())]);
    g.start_scenario("late").unwrap();
    let seen = g.game.inspect();
    assert_eq!((&seen["at"]["name"], &seen["level"]["how"], &seen["level"]["set"]), (&"b".into(), &"go_to".into(), &true.into()), "{seen}");
    assert_eq!(state(&mut g, "day"), format!("{:?}", super::Value::Int(30)));
    let mut recorder = Recorder::default();
    for n in 0..120 {
        if n == 30 {
            assert_eq!(g.eval("world.state.day").unwrap(), "60", "an expression answers");
            assert_eq!(g.eval("world.state.day = 1000").unwrap(), "", "a statement says nothing");
        }
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(state(&mut g, "day"), format!("{:?}", super::Value::Int(1090)));
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    let replay = recorder.replay(&g).unwrap();
    assert_eq!((replay.scenario.as_deref(), replay.evals.len(), replay.evals[1].at), (Some("late"), 2, 30));
    replay.save(&dir.join("replays/late.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "it plays back the same");
    let back = Replay::load(&dir.join("replays/late.toml")).unwrap();
    assert_eq!(back, replay, "written and read back as it was");
    // The scenario changed: the replay was recorded on other data.
    std::fs::write(dir.join("scenarios/late.lua"), late.replace("= 30", "= 31")).unwrap();
    assert!(!check::<LuaGame>(&dir, false).unwrap(), "the scenario's file counts");
}

/// Every game of the engine's and every template keeps scenarios, its
/// late situations set up in code (docs/game-model.md, "Scenarios and the
/// console"): without them a game can hardly be tried. Each says what it is
/// about and begins a run that plays and draws on. The Bench is a load
/// test, no game.
#[test]
fn every_game_and_template_has_scenarios() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut folders: Vec<PathBuf> = ["games", "templates"]
        .iter()
        .flat_map(|parent| std::fs::read_dir(root.join(parent)).unwrap().flatten().map(|e| e.path()))
        .filter(|dir| dir.join("conf.lua").is_file() && !dir.ends_with("games/bench"))
        .collect();
    folders.sort();
    assert!(folders.len() >= 9, "{folders:?}");
    for dir in folders {
        let scenarios = Session::<LuaGame>::load(&dir).unwrap().scenarios().unwrap();
        assert!(!scenarios.is_empty(), "{} has no scenarios (scenarios/NAME.lua)", dir.display());
        for (name, about) in scenarios {
            assert!(!about.is_empty() && !about.starts_with("(a mistake"), "{}: {name}: {about}", dir.display());
            let mut g = Session::<LuaGame>::load(&dir).unwrap();
            g.start_scenario(&name).unwrap_or_else(|e| panic!("{}: {name}: {e:#}", dir.display()));
            for _ in 0..60 {
                g.step(&Input::default());
                g.look();
            }
            assert_eq!(halted(&g), "", "{}: {name}", dir.display());
        }
    }
}

/// A run the console or a scenario helped is told as such (its high score
/// goes in as GOD); a line that went wrong was run all the same.
#[test]
fn a_run_helped_by_the_console_or_a_scenario_is_told() {
    let dir = folder("helped", "function on_tick() end\n");
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/late.lua"), "return { about = \"late\", setup = function() end }\n").unwrap();
    let mut fair = game_in(&dir);
    steps(&mut fair, 10);
    assert!(!fair.helped(), "played as it is");
    let mut begun = game_in(&dir);
    begun.start_scenario("late").unwrap();
    assert!(begun.helped(), "begun from a scenario");
    let mut lined = game_in(&dir);
    steps(&mut lined, 5);
    lined.eval("1 + 1").unwrap();
    assert!(lined.helped(), "a console line");
    let mut wrong = game_in(&dir);
    assert!(wrong.eval("nothing.here()").is_err());
    assert!(wrong.helped(), "a line that went wrong");
}

/// What goes wrong with a scenario or a console line says where: a
/// scenario the game lacks names the ones it has, a scenario's table is
/// checked, a mistake in its setup halts the game with its line. A
/// mistake in a console line is answered and the game goes on; one that
/// leaves a state holding what is no plain data stops it, as at a tick's
/// end.
#[test]
fn scenarios_and_console_lines_say_what_went_wrong() {
    let dir = folder("scenario-mistakes", "function on_tick() end\n");
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    for (name, src) in [
        ("plain", "return 3\n"),
        ("odd", "return { setup = function() end, start = 1 }\n"),
        ("bare", "return { about = \"no setup\" }\n"),
        ("broken", "return {\n  setup = function()\n    world.state.x = nothing.here\n  end,\n}\n"),
    ] {
        std::fs::write(dir.join(format!("scenarios/{name}.lua")), src).unwrap();
    }
    for (name, says) in [
        ("gone", "no scenario 'gone' (the game's: bare, broken, odd, plain)"),
        ("plain", "scenarios/plain.lua: a scenario gives back a table"),
        ("odd", "scenarios/odd.lua: no field called 'start' (about, level, place, setup)"),
        ("bare", "scenarios/bare.lua: setup is the function that sets the run up"),
        ("broken", "scenarios/broken.lua:3:"),
    ] {
        let mut g = game_in(&dir);
        let e = format!("{:#}", g.start_scenario(name).unwrap_err());
        assert!(e.contains(says), "{name}: {e}");
    }
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert!(g.start_scenario("bare").unwrap_err().to_string().contains("before its first tick"), "a scenario begins a run");
    let e = g.eval("nothing.here = 1").unwrap_err().to_string();
    assert!(e.contains("console:1:") && halted(&g).is_empty(), "answered, the game goes on: {e}");
    assert_eq!(g.eval("print('hi') return 1, 'two'").unwrap(), "hi\n1\t\"two\"", "what it printed, then its values");
    assert!(g.eval("world.state.f = function() end").is_err());
    assert!(halted(&g).contains("world.state"), "a state that is no plain data stops it: {}", halted(&g));
}

/// BunnyFarm saved with a gap in a pen's fence: the save brings the
/// level's voxels back with the gap already open, and the game, breaking
/// the gap in anew as the farm is entered, still knows what the gap took,
/// so mending puts the fence back whole (it knew nothing, and the hole
/// stayed for good once mended).
#[test]
fn bunnyfarms_fence_saved_with_a_gap_still_mends_whole() {
    use crate::replay::{Replay, Script, parse_script};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/bunnyfarm");
    // An evening with a gap in the chicken pen's fence, the burrow beside.
    let replay = Replay::load(&dir.join("replays/tests/wood_mends_the_fence.toml")).unwrap();
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    g.start_from(replay.from.as_deref().unwrap()).unwrap();
    let taken = g.game.inspect()["level"]["fences"]["chicken"]["taken"].clone();
    assert!(taken.as_array().is_some_and(|t| t.len() > 20), "the gap broken in from the level's file: {taken}");
    // To bed: the farm is saved as the bunny sleeps.
    let mut script = Script::new(parse_script("none:3;act:1;none:5;down:1;none:3;act:1;none:5", g.actions()).unwrap());
    for _ in 0..19 {
        g.step(&script.next_input());
    }
    assert_eq!(halted(&g), "");
    let text = g.slots.read(1).expect("saved as it slept").to_string();
    let mut loaded = Session::<LuaGame>::load(&dir).unwrap();
    loaded.start_from(&text).unwrap();
    assert_eq!(halted(&loaded), "");
    assert_eq!(loaded.game.inspect()["level"]["fences"]["chicken"]["taken"], taken, "what the gap took, known after loading");
}

/// A trace of how a game plays, a line a second (`TRACE_OUT`): the
/// simulation's tick and random generator, every thing's body and hit
/// points by its kind's name, the kits' numbers. For proving a change
/// leaves the game as it played where the replays' checks cannot (their
/// hash is made anew when what the script keeps changes):
/// `TRACE_OUT=dir cargo test --release trace_the_kit_games -- --ignored`.
#[test]
#[ignore]
fn trace_the_kit_games() {
    use crate::lua::host::Either;
    use crate::replay::{Playback, Replay, replay_session};
    let out = PathBuf::from(std::env::var("TRACE_OUT").expect("TRACE_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (game, file) in [
        ("voxkeep", "replays/tests/climb_to_the_door.toml"),
        ("moreblood", "replays/tests/run_and_gun.toml"),
        ("moreblood", "replays/20261015-171938-14600.toml"),
        ("arena", "replays/tests/walkabout.toml"),
    ] {
        let dir = root.join("games").join(game);
        let replay = Replay::load(&dir.join(file)).unwrap();
        let mut g = replay_session::<LuaGame>(&dir, &replay).unwrap();
        let mut p = Playback::new(&replay, &mut g).unwrap();
        let mut lines = Vec::new();
        let sample = |g: &Session<LuaGame>| -> String {
            let lua = &g.game;
            let host = &lua.stage.host;
            let sim = host.sim();
            let mut line = format!("tick {} rng {} changes {} freeze {}", sim.tick, sim.rng.state(), sim.world.changes(), sim.freeze);
            for id in host.ids() {
                let Some(t) = host.get(id) else { continue };
                let kind = lua.stage.kinds.get(t.kind).map_or("-", |k| k.name.as_str());
                let b = t.body;
                line += &format!(" | {kind} {:?} {:?} {:?} {} hp {}", b.pos, b.vel, b.look, b.on_ground, t.hp);
            }
            // What the game keeps since stage D (lives, taken, the freed
            // hostages), read where it lives now; the rounds from the kit.
            let seen = lua.inspect();
            let plain = |path: [&str; 3]| seen[path[0]][path[1]][path[2]].as_i64().unwrap_or(0);
            match lua.kit() {
                Some(Either::A(s)) => {
                    let st = &s.status;
                    let b = &s.player.body;
                    let rounds = |item: &str| st.stock.get(item).copied().unwrap_or(0);
                    // The gun in hand as the traces wrote it before stage D:
                    // its place in More Blood's arsenal.
                    let held = st.weapon.map(|w| s.rules.weapon_names[w].as_str());
                    let slot = ["rifle", "machine_gun", "rockets"].iter().position(|g| Some(*g) == held).unwrap_or(0);
                    let ammo = if slot == 0 { 0 } else { rounds(held.unwrap()) };
                    line += &format!(" || player {:?} {:?} {:?} {} inv {} | {:?} hp {} lives {} score {} grenades {} weapon {} ammo {} freed {} | bullets {} grenades {}", b.pos, b.vel, b.look, b.on_ground, s.player.invulnerable, st.phase, st.hp, plain(["player", "state", "lives"]), st.score, rounds("grenade"), slot, ammo, seen["level"]["freed"].as_i64().unwrap_or(0), s.bullets.len(), s.grenades.len());
                }
                Some(Either::B(pf)) => {
                    let st = &pf.status;
                    line += &format!(" || score {} lives {} hearts {} taken {} outcome {:?}", st.score, plain(["player", "state", "lives"]), st.hp, plain(["player", "state", "taken"]), st.outcome);
                }
                None => {}
            }
            line
        };
        while !p.done() {
            let _ = p.step(&mut g);
            if p.steps().is_multiple_of(60) {
                lines.push(sample(&g));
            }
        }
        lines.push(format!("end {}", sample(&g)));
        lines.push(format!("outcome {:?} score {}", g.game.outcome(), g.game.score()));
        let name = format!("{game}-{}.trace", Path::new(file).file_stem().unwrap().to_string_lossy());
        std::fs::write(out.join(name), lines.join("\n") + "\n").unwrap();
    }
}

/// How a touch began (`on_touch`'s `where`): walked into, the side;
/// come down onto, however fast, the top; something come down onto the
/// player, its bottom. `thing:touching` says what touches every tick.
#[test]
fn a_touch_says_where_it_began() {
    let main = r#"
function on_level_start(how)
  level.state.a = level:spawn("box", 8, 2, { name = "a" })
end

function on_touch(a, b, where)
  if a == player then
    world.state.log = (world.state.log or "") .. tostring(b.name) .. " " .. where .. ";"
  end
end

function on_tick()
  if world.tick <= 30 then player:walk({ x = 1, y = 0 }, 30) end
  if world.tick <= 30 then world.state.touching = math.max(world.state.touching or 0, #player:touching("box")) end
  if world.tick == 40 then
    level:spawn("box", 13, 2, { name = "low" })
    player:place(13.5, 2.5, 60)
  end
  if world.tick == 90 then
    player:place(4.5, 2.5)
    level:spawn("box", 4.5, 2.5, { z = 20, name = "high" })
  end
end
"#;
    let mut g = game_in(&events_folder("where", main, ""));
    steps(&mut g, 140);
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    assert!(log.starts_with("a side;"), "walked into: {log}");
    assert!(log.contains("low top;"), "come down onto it from high up, fast: {log}");
    assert!(log.contains("high bottom;"), "it came down onto the player: {log}");
    assert_eq!(g.game.inspect()["state"]["touching"], 1, "touching while it lasts");
}

/// Things placed by the level file are named by their kind and number,
/// spawned ones as they were named; kinds have families and props; a
/// checkpoint may name a thing; hits name their weapon; a body may
/// bounce; falling out is told once.
#[test]
fn names_families_props_and_the_rest() {
    let main = r#"
function on_hit(thing, by, damage, weapon)
  world.state.weapon = weapon
end

function on_fall_out(thing)
  world.state.fell = (world.state.fell or 0) + 1
end

function on_level_start(how)
  world.state.how = how
  if how == "start" then
    level.state.b = level:spawn("box", 6, 2, { name = "crate", props = { worth = 9 } })
  end
end

function on_tick()
  local b = level:thing("crate")
  if world.tick == 1 then
    world.state.door = level:thing("door_1") ~= nil
    world.state.crates = level:count("crates")
    world.state.worth = b.props.worth
    world.state.kind_worth = level:spawn("box", 9, 2).props.worth
    world.state.family = b.families.crates
    world.state.block = level:block(player.x, player.y, player.z - 1)
    world.state.in_rock = player:in_block("solid")
    b:hurt(1, player, "sword")
  end
  if world.tick == 3 then player:bounce(10) end
  if world.tick == 4 then world.state.rising = player.z > 4 end
  if world.tick == 20 then player:place(2.5, 2.5, -60) end
  if world.tick == 30 then
    player.state.checkpoint = "door_1"
    world:restart()
  end
  if world.tick == 31 then world.state.at_door = math.floor(player.x) end
end
"#;
    let conf = "  c.things.box.family = \"crates\"\n  c.things.box.props = { worth = 1 }\n";
    let mut g = game_in(&events_folder("names", main, conf));
    steps(&mut g, 35);
    assert_eq!(halted(&g), "");
    let s = &g.game.inspect()["state"];
    assert_eq!((s["door"].as_bool(), s["crates"].as_i64(), s["worth"].as_i64(), s["kind_worth"].as_i64(), s["family"].as_bool()), (Some(true), Some(1), Some(9), Some(1), Some(true)));
    assert_eq!((s["block"].as_str(), s["in_rock"].as_bool()), (Some("solid"), Some(false)));
    assert_eq!(s["weapon"], "sword");
    assert_eq!(s["rising"], true, "bounced up");
    assert_eq!(s["fell"], 1, "fell out once");
    assert_eq!((s["how"].as_str(), s["at_door"].as_i64()), (Some("restart"), Some(12)), "back at the door the checkpoint names");

    // Props are the kind's, of their kind, and only to read.
    for (code, says) in [
        ("level:spawn(\"box\", 6, 2, { props = { wroth = 9 } })", "props.wroth: a box has no such prop (c.things.box.props: worth)"),
        ("level:spawn(\"box\", 6, 2, { props = { worth = \"lots\" } })", "props.worth is a number by its kind (c.things.box.props.worth = 1), not a string"),
        ("level:spawn(\"door\", 6, 2, { props = { worth = 1 } })", "a door has no such prop (c.things.door.props declares none)"),
        ("level:things(\"box\")[1].props.worth = 2", "thing.props.worth: only to read; what changes while playing goes in thing.state"),
        ("player.families.crates = true", "thing.families.crates: only to read"),
    ] {
        let main = format!("function on_level_start(how) level:spawn(\"box\", 6, 2) end\nfunction on_tick() {code} end\n");
        let mut g = game_in(&events_folder("props-checked", &main, conf));
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{code}: {}", halted(&g));
    }
}

/// A kind's effect and sound show as one of its things goes: the
/// script's own kinds too, their hit points set to 0.
#[test]
fn a_kinds_effect_and_sound_show_as_it_goes() {
    let main = r#"
function on_level_start(how)
  level.state.b = level:spawn("box", 6, 2)
end

function on_tick()
  if world.tick == 2 then level.state.b.state.hp = 0 end
end
"#;
    let conf = "  c.things.box.effect = \"poof.vvx\"\n  c.things.box.sound = \"poof\"\n";
    let dir = events_folder("goes", main, conf);
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/poof.toml"), "wave = \"noise\"\nfreq = 700\ndecay = 0.1\n").unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena/player.vvx"), dir.join("poof.vvx")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.stage.effects.iter().map(|e| e.model.as_str()).collect::<Vec<_>>(), ["poof.vvx"], "its effect shows");
    assert!(g.game.stage.host.sim().sounds.iter().any(|s| s.name == "poof"), "its sound plays");
}

/// A kind whose effect is "burst" bursts into its own voxels as one of
/// its things goes: the simulation's pieces, each of its model's colours
/// (no blood without gore), no model played; and they are gone after a
/// few seconds.
#[test]
fn a_kind_that_bursts_goes_in_its_voxels() {
    let main = r#"
function on_level_start(how)
  level.state.b = level:spawn("box", 6, 2)
end

function on_tick()
  if world.tick == 2 then level.state.b.state.hp = 0 end
end
"#;
    let conf = "  c.things.box.model = \"box.vvx\"\n  c.things.box.effect = \"burst\"\n";
    let dir = events_folder("bursts", main, conf);
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena/player.vvx");
    std::fs::copy(&model, dir.join("box.vvx")).unwrap();
    let mut g = game_in(&dir);
    let pieces = |g: &Session<LuaGame>| g.game.stage.host.sim().particles.clone();
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert!(g.game.stage.effects.is_empty(), "no model played");
    let shape = crate::vox::load_model(&model).unwrap();
    let colours: std::collections::BTreeSet<[u8; 4]> = shape.frames[0].iter().map(|(_, c)| *c).collect();
    let burst = pieces(&g);
    assert_eq!(burst.len(), shape.frames[0].len(), "a piece for each of its voxels");
    assert!(burst.iter().all(|p| colours.contains(&p.color)), "only its own colours");
    steps(&mut g, 4 * 60);
    assert!(pieces(&g).is_empty(), "the pieces are gone after a few seconds");
}

/// Falling out of the level is told once, also while the game's code lets
/// the jump & run's hero lie there (the kit sees it out every tick).
#[test]
fn a_fall_is_told_once_while_the_code_waits() {
    use crate::grid::{BlockEntry, GridLevel, Kind};
    let main = "function on_fall_out(t) world.state.fell = (world.state.fell or 0) + 1 end\n";
    let dir = levels_folder("fall-once", main, &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3, lives = 3, taken = 0 }\n  c.world.blocks = { file = \"level.json\", below = 16 }\n  c.things.hero = { behaviour = \"hero\", height = 6 }\nend\n").unwrap();
    // Nothing to stand on.
    let mut level = GridLevel::new(4, glam::IVec3::new(4, 1, 2));
    level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    level.things.push(crate::grid::Thing { kind: "hero".into(), at: [6, 2, 4] });
    level.save(&dir.join("levels/a.json")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 240);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["state"]["fell"], 1, "told once");
}

/// The jump & run's rules as the platformer template writes them, played:
/// a gem taken counts, the checkpoint is noted, a fall into the pit costs
/// a life and brings the hero back at the checkpoint with every heart,
/// and with the last life the run is lost.
#[test]
fn the_jump_and_run_rules_as_code() {
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind};
    let template = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/platformer/main.lua")).unwrap();
    let rules = &template[..template.find("-- Every torch").expect("the template's torches")];
    let main = format!("{rules}\nlocal start, fell, died = on_tick, on_fall_out, on_death\nfunction on_fall_out(t) world.state.why = (world.state.why or '') .. 'fell ' .. tostring(t) .. ' ' .. world.tick .. ' ' .. math.floor(t.z) .. ';' fell(t) end\nfunction on_death(t, by) world.state.why = (world.state.why or '') .. 'died ' .. tostring(t) .. ' ' .. world.tick .. ';' died(t, by) end\nfunction on_tick()\n  start()\n  world.state.log = (world.state.log or '') .. (player.state.checkpoint or '-') .. ',' .. player.state.lives .. ',' .. player.state.taken .. ';'\nend\n");
    let dir = levels_folder("jump-and-run-rules", &main, &[("a", &[])]);
    std::fs::write(
        dir.join("conf.lua"),
        "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3, lives = 3, taken = 0 }\n  c.world.blocks = { file = \"level.json\", below = 16 }\n  c.physics.gravity = 400\n  c.things.hero = { behaviour = \"hero\", height = 6, speed = 60 }\n  c.things.gem = { behaviour = \"pickup\", height = 4, family = \"pickups\", props = { worth = 100 }, sound = \"pickup\" }\n  c.things.checkpoint = { behaviour = \"prop\", stands = true, height = 8 }\n  c.things.door = { behaviour = \"prop\", stands = true, height = 8 }\nend\n",
    )
    .unwrap();
    // A floor of 12 cells, a pit two cells wide in it.
    let mut level = GridLevel::new(4, glam::IVec3::new(12, 1, 2));
    level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    for x in (0..12).filter(|x| !(7..9).contains(x)) {
        let i = level.index(glam::IVec3::new(x, 0, 0)).unwrap();
        level.cells[i] = Cell { block: 1, turn: Default::default() };
    }
    for (kind, x) in [("hero", 2), ("gem", 10), ("checkpoint", 18)] {
        level.things.push(crate::grid::Thing { kind: kind.into(), at: [x, 2, 4] });
    }
    level.save(&dir.join("levels/a.json")).unwrap();
    // Solid rock blocks.
    let mut rock = crate::blocks::Block::empty(4);
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..4 {
                rock.set(glam::IVec3::new(x, y, z), Some([90, 80, 70])).unwrap();
            }
        }
    }
    rock.save(&crate::blocks::path(&dir, "rock")).unwrap();
    let mut g = game_in(&dir);
    for _ in 0..1200 {
        g.step(&Input { right: true, ..Input::default() });
        if g.game.outcome().is_some() {
            break;
        }
        // (The fade of the end does not run here: the run ends at once.)
    }
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    let why = g.game.inspect()["state"]["why"].as_str().unwrap_or_default().to_string();
    assert!(log.contains("-,3,1;"), "the gem taken: {log} {why}");
    assert!(log.contains("checkpoint_1,3,1;") && log.contains("checkpoint_1,2,1;") && log.contains("checkpoint_1,1,1;"), "lives lost in the pit: {log}");
    assert_eq!(g.game.outcome(), Some(crate::game::Outcome::Lost), "the last life ends the run");
    assert_eq!(g.game.score(), 100);
}

/// A figure of parts (a model whose objects have names) is drawn a part a
/// draw; `frame:model`'s pose turns parts by their names, and a kind's
/// `swing` names them too: a name the model lacks is a mistake that says
/// which parts there are.
#[test]
fn a_figure_of_parts_is_posed_by_its_parts_names() {
    let conf = |swing: &str| format!("
function conf(c)
  c.world.size = {{ 64, 64, 16 }}
  c.things.robot = {{ model = \"robot.vvx\"{swing} }}
end
");
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.r = level:spawn("robot", 32, 32)
end

function on_draw(frame)
  frame:model("robot.vvx", level.state.r, nil, nil, { arm = { pitch = 90 } })
end
"##;
    let dir = folder("parts", main);
    crate::vox::write_figure(&dir.join("robot.vvx"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO), ("arm", glam::IVec3::new(1, 2, 5), glam::IVec3::new(4, 1, 3))]);
    std::fs::write(dir.join("conf.lua"), conf(", swing = { arm = 30 }")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 10);
    assert_eq!(halted(&g), "");
    let frame = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(frame.models.len(), 2, "a draw a part");
    assert!(frame.models.iter().any(|m| m.rot != glam::Quat::IDENTITY), "the arm turned");
    std::fs::write(dir.join("main.lua"), main.replace("{ arm =", "{ leg =")).unwrap();
    let g = game_in(&dir);
    let _ = g.frame(1.0, 16.0 / 9.0);
    assert!(halted(&g).contains("robot.vvx has no part 'leg' (its parts: body, arm)"), "{}", halted(&g));
    std::fs::write(dir.join("conf.lua"), conf(", swing = { leg = 30 }")).unwrap();
    let err = Session::<LuaGame>::load(&dir).err().map(|e| format!("{e:#}")).unwrap_or_default();
    assert!(err.contains("c.things.robot.swing.leg: no such part (its parts: body, arm)"), "{err}");
}

/// `frame:model` without a frame shows what the thing does as its kind's
/// frames say (idle, walk, jump), else the model's own layout.
#[test]
fn a_model_without_a_frame_plays_its_kinds_frames() {
    let conf = r#"
function conf(c)
  c.world.size = { 64, 64, 16 }
  c.things.miner = { model = "player.vvx", frames = { idle = { 2, 2 }, walk = { 2, 2 }, jump = { 2, 2 } } }
  c.things.digger = { model = "player.vvx" }
end
"#;
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.a = level:spawn("miner", 16, 16)
  level.state.b = level:spawn("miner", 32, 16)
  level.state.c = level:spawn("digger", 48, 16)
end

function on_draw(frame)
  frame:model("player.vvx", level.state.a)
  frame:model("player.vvx", level.state.b, 2)
  frame:model("player.vvx", level.state.c)
  frame:model("player.vvx", level.state.c, 0)
end
"##;
    let dir = folder("kind_frames", main);
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena/player.vvx"), dir.join("player.vvx")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(halted(&g), "");
    let frame = g.frame(1.0, 16.0 / 9.0);
    let voxels: Vec<_> = frame.models.iter().map(|m| m.voxels.clone()).collect();
    assert_eq!(voxels[0], voxels[1], "its kind's idle: frame 2");
    assert_eq!(voxels[2], voxels[3], "standing, the model's own: frame 0");
    assert_ne!(voxels[0], voxels[2]);
}

/// A game turns its models freely when it chooses: frame:model's turn,
/// or a kind with turns = "free" facing exactly its way (a diagonal);
/// otherwise its models stay on the grid in quarter turns. A turn has
/// only turn, pitch and roll; turns is only "quarter" or "free".
#[test]
fn a_game_turns_its_models_freely() {
    let conf = r#"
function conf(c)
  c.world.size = { 64, 64, 16 }
  c.things.miner = { model = "player.vvx" }
  c.things.rover = { model = "player.vvx", turns = "free" }
end
"#;
    let main = r#"
function on_level_start()
  level.state.a = level:spawn("miner", 16, 16, { facing = { x = 1, y = 1 } })
  level.state.b = level:spawn("miner", 32, 16, { facing = { x = 1, y = 1 } })
  level.state.c = level:spawn("rover", 48, 16, { facing = { x = 1, y = 1 } })
  level.state.d = level:spawn("rover", 48, 40, { facing = { x = 0, y = 1 } })
end

function on_draw(frame)
  frame:model("player.vvx", level.state.a, 0)
  frame:model("player.vvx", level.state.b, 0, { roll = 30, pitch = 5 })
  frame:model("player.vvx", level.state.c, 0)
  frame:model("player.vvx", level.state.d, 0)
end
"#;
    let dir = folder("turning", main);
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena/player.vvx"), dir.join("player.vvx")).unwrap();
    let g = game_in(&dir);
    assert_eq!(halted(&g), "");
    let frame = g.frame(1.0, 16.0 / 9.0);
    let turned: Vec<bool> = frame.models.iter().map(|m| m.rot != glam::Quat::IDENTITY).collect();
    assert_eq!(turned, [false, true, true, false], "on the grid, turned by the code, facing its diagonal, facing straight");
    // Its forward after the quarter turn, turned on freely.
    let (x, y) = crate::render::quarter(0, 1, frame.models[2].turn);
    let diagonal = frame.models[2].rot * glam::Vec3::new(x as f32, y as f32, 0.0);
    assert!((diagonal.x - diagonal.y).abs() < 1e-4 && diagonal.x > 0.0, "a free kind facing x = y = 1 looks there ({diagonal})");

    let wrong = main.replace("{ roll = 30, pitch = 5 }", "{ spin = 3 }");
    std::fs::write(dir.join("main.lua"), wrong).unwrap();
    let g = game_in(&dir);
    let _ = g.frame(1.0, 16.0 / 9.0);
    assert!(halted(&g).contains("a turn has turn, pitch and roll in degrees ({ roll = 20 }), not 'spin'"), "{}", halted(&g));
    std::fs::write(dir.join("conf.lua"), conf.replace("turns = \"free\"", "turns = \"round\"")).unwrap();
    assert!(start_error(&dir).contains("c.things.rover.turns: \"round\" is neither \"quarter\""), "{}", start_error(&dir));
}

/// The camera stays where the game pointed it (level.camera:look_at) while
/// things spawn, until it follows one again; without a word from the game
/// it follows the first thing spawned.
#[test]
fn the_camera_keeps_looking_where_the_game_said() {
    let main = r#"
function on_level_start()
  level.camera:look_at(20, 40)
end

function on_tick()
  level:spawn("miner", 50, 10)
  if world.tick == 30 then level.camera:follow(level:things("miner")[1]) end
end
"#;
    let mut g = game("looking", main);
    steps(&mut g, 20);
    assert!(g.game.focus().is_none(), "a thing spawned took the camera");
    let at = g.game.sim().cam_target;
    assert!((at.x - 20.0).abs() < 0.5 && (at.y - 40.0).abs() < 0.5, "the camera looks at {at}");
    steps(&mut g, 15);
    assert!(g.game.focus().is_some(), "follow takes it back");
    let g = game("first", "function on_level_start() level:spawn(\"miner\", 50, 10) end\n");
    assert!(g.game.focus().is_some(), "without look_at the first thing spawned is followed");
}

/// A touch between two things that both have a reach counts within the
/// larger one: each is touched within its own (a small ship's hit zone
/// beside a big power-up).
#[test]
fn each_is_touched_within_its_own_reach() {
    let main = r#"
function on_level_start()
  level.state.power = level:spawn("power", 30, 20)
end

function on_tick()
  player:walk({ x = 1, y = 0 }, 20)
end

function on_touch(a, b)
  if b.kind == "power" and not world.state.at then world.state.at = math.floor(30 - a.x) end
end
"#;
    let dir = folder("reach", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.kind = \"ship\"\n  c.player.spawn = { 6, 20 }\n  c.things.ship = { reach = 3 }\n  c.things.power = { reach = 9 }\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 120);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "at"), "Int(8)", "touched as it came within the power-up's reach");
}

/// Without snapping, frame:cube glides as the models do: a cube between
/// voxels is drawn there, not on the voxel below it.
#[test]
fn cubes_glide_when_the_camera_does() {
    let main = "function on_draw(frame) frame:cube(10.25, 12.5, 3, { 255, 0, 0 }) end\n";
    let dir = folder("glide", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.camera.snap = false\nend\n").unwrap();
    let g = game_in(&dir);
    let red: Vec<[f32; 4]> = g.frame(1.0, 16.0 / 9.0).cubes().iter().filter(|i| i.color == [255, 0, 0, 255]).map(|i| i.pos_size).collect();
    assert_eq!(red, [[10.75, 13.0, 3.5, 1.0]]);
    let dir = folder("glide-snap", main);
    let g = game_in(&dir);
    let red: Vec<[f32; 4]> = g.frame(1.0, 16.0 / 9.0).cubes().iter().filter(|i| i.color == [255, 0, 0, 255]).map(|i| i.pos_size).collect();
    assert_eq!(red, [[10.5, 12.5, 3.5, 1.0]], "snapping, on the voxel");
}

/// A game for two (`docs/game-model.md`, "Several players"): a hero each,
/// walked by its own keys; joins and leaves noted in `world.state`.
fn two_players(name: &str, main: &str) -> Session<LuaGame> {
    let dir = folder(name, main);
    std::fs::write(
        dir.join("conf.lua"),
        "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\", \"fire\" }\n  c.players = 2\n  c.things.hero = { hp = 3 }\n  c.things.hero_blue = { like = \"hero\" }\n  c.player.kind = { \"hero\", \"hero_blue\" }\n  c.state.player = { lives = 3 }\nend\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    game_in(&dir)
}

const TWO_PLAYERS: &str = r#"
function on_world_start()
  world.state.told = ""
end
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, 1)
end
function on_player_join(p, how)
  world.state.told = world.state.told .. "join " .. p.seat .. " " .. how .. ";"
end
function on_player_leave(p)
  world.state.told = world.state.told .. "leave " .. p.seat .. ";"
end
function on_tick()
  for _, p in ipairs(level:players()) do
    p:walk(p.input.dir, 30)
    if p.input:pressed("fire") then p.state.score = p.state.score + 10 end
  end
end
"#;

/// A second player takes a seat while the game runs: told first among the
/// events, its state a copy of `c.state.player`, its figure in at the
/// tick's end beside the first player, walked by its own keys; it leaves,
/// and comes back with its state.
#[test]
fn a_second_player_takes_a_seat_and_leaves() {
    let mut g = two_players("seats", TWO_PLAYERS);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    assert_eq!(said(&mut g, "return #world.players, world.players[1] == player, world.players[2]"), "2\ttrue\tfalse");
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    g.step(&Input::default());
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return world.state.told"), "\"join 2 new;\"");
    assert_eq!(said(&mut g, "return world.players[2].seat, world.players[2].state.lives, world.players[2].kind, #level:players()"), "2\t3\t\"hero_blue\"\t2");
    assert_eq!(said(&mut g, "return world.players[2].x == player.x"), "true", "beside the first player");
    let right = Input { right: true, ..Input::default() };
    g.set_seat(2, Some(right));
    steps(&mut g, 30);
    assert_eq!(said(&mut g, "return world.players[2].x > player.x + 10, player.x"), "true\t32.0", "seat 1 held nothing and stood");
    let fire = Input { actions: 2, ..Input::default() };
    g.set_seat(2, Some(fire));
    g.step(&Input::default());
    assert_eq!(said(&mut g, "return world.players[2].state.score, player.state.score"), "10\t0");
    g.set_seat(2, None);
    g.step(&Input::default());
    assert_eq!(said(&mut g, "return world.state.told, world.players[2], #level:players()"), "\"join 2 new;leave 2;\"\tfalse\t1");
    g.set_seat(2, Some(Input::default()));
    g.step(&Input::default());
    assert_eq!(said(&mut g, "return world.state.told, world.players[2].state.score"), "\"join 2 new;leave 2;join 2 back;\"\t10");
}

/// A run of two is written down seat by seat (`away` while nobody sat
/// there): played back it comes out the same, going back again and again
/// too, and its seats are in its file.
#[test]
fn a_run_of_two_replays_the_same() {
    use crate::replay::{Recorder, check, check_going_back};
    let mut g = two_players("seats-replay", TWO_PLAYERS);
    let dir = g.dir.clone();
    let mut recorder = Recorder::default();
    let right = Input { right: true, ..Input::default() };
    let fire = Input { actions: 2, ..Input::default() };
    for tick in 0..300 {
        let seat = match tick {
            0..40 => None,
            40..120 => Some(right),
            120..130 => Some(fire),
            130..200 => None,
            _ => Some(Input { left: true, ..Input::default() }),
        };
        g.set_seat(2, seat);
        let input = if tick % 50 < 20 { right } else { Input::default() };
        g.step(&input);
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    let replay = recorder.replay(&g).unwrap();
    assert_eq!(replay.seats.len(), 1);
    assert!(replay.seats[0].input.starts_with("away:40;right:80;fire:10;away:70;left:100"), "{}", replay.seats[0].input);
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/two.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "played back the same");
    assert!(check_going_back::<LuaGame>(&dir, 8).unwrap(), "going back the same");
}

/// Two players are held within `c.camera.hold` of each other, the later
/// seat giving way; moving together nobody is stopped; the camera looks at
/// the middle of them, each in whole voxels against it.
#[test]
fn the_hold_keeps_two_players_together() {
    let mut g = two_players("hold", TWO_PLAYERS);
    let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.players = 2", "c.players = 2\n  c.camera.hold = { 20 }");
    std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
    g = game_in(&g.dir.clone());
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    let right = Input { right: true, ..Input::default() };
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    g.set_seat(2, Some(right));
    steps(&mut g, 150);
    assert_eq!(said(&mut g, "return math.floor(world.players[2].x - player.x + 0.5)"), "20", "held at the hold");
    // Seat 2 back left, seat 1 after it: neither is stopped.
    g.set_seat(2, Some(Input { left: true, ..Input::default() }));
    let x = said(&mut g, "return world.players[2].x").parse::<f32>().unwrap();
    for _ in 0..30 {
        g.step(&Input { left: true, ..Input::default() });
    }
    let now = said(&mut g, "return world.players[2].x").parse::<f32>().unwrap();
    assert!(x - now > 10.0, "moving together, it went on: {x} to {now}");
    // The camera between them, whole voxels from the first.
    let target = g.view_target(1.0);
    let (a, b) = (said(&mut g, "return player.x").parse::<f32>().unwrap(), said(&mut g, "return world.players[2].x").parse::<f32>().unwrap());
    assert!((target.x - (a + b) * 0.5).abs() <= 1.0, "the middle: {} between {a} and {b}", target.x);
    assert_eq!(target.x.fract(), 0.0);
}

/// Each player acts on the thing it touches (`on_interact`, `by` the one
/// who pressed), the press used up for its seat; `frame.players` lists
/// the seats taken at this machine.
#[test]
fn each_player_interacts_and_the_frame_knows_them() {
    let main = r#"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, 1)
  level.state.sign = level:spawn("miner", 40, 32)
end
function on_interact(thing, by)
  world.state.read = (world.state.read or "") .. by.seat .. ";"
end
function on_tick()
  for _, p in ipairs(level:players()) do
    if p.input:pressed("interact") then world.state.unused = (world.state.unused or 0) + 1 end
  end
end
function on_draw(frame)
  frame:text(#frame.players .. " at this screen", 1, 1)
end
"#;
    let mut g = two_players("seats-interact", main);
    let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.actions = { \"jump\", \"fire\" }", "c.actions = { \"jump\", \"interact\" }\n  c.things.miner = {}");
    std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
    g = game_in(&g.dir.clone());
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    said(&mut g, "world.players[2]:place(40, 32)");
    steps(&mut g, 2);
    let press = Input { actions: 2, ..Input::default() };
    g.set_seat(2, Some(press));
    g.step(&Input::default());
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return world.state.read, world.state.unused"), "\"2;\"\tnil", "seat 2 read the sign, the press used up");
    g.look();
    assert_eq!(first_text(&g), "2 at this screen");
}

/// A save keeps every seat: a player's state, its score and where it
/// stood, and players named in states as their seats; loaded with the
/// second seat away, its state waits and comes back with whoever takes it.
#[test]
fn a_save_keeps_every_seat() {
    let mut g = two_players("seats-save", TWO_PLAYERS);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    said(&mut g, "local p = world.players[2]; p.state.score = 70; p.state.lives = 1; p:place(20, 40); world.state.best = p");
    steps(&mut g, 2);
    let text = g.game.save().unwrap();
    assert!(text.contains("world.players[2]"), "{text}");
    let mut from = two_players("seats-save-into", TWO_PLAYERS);
    from.start_from(&text).unwrap();
    assert_eq!(halted(&from), "");
    assert_eq!(said(&mut from, "return world.players[2], world.state.best.seat, world.state.best.state.score"), "false\t2\t70", "away, its state kept");
    from.set_seat(2, Some(Input::default()));
    from.step(&Input::default());
    assert_eq!(said(&mut from, "return world.state.told, world.players[2].state.lives, world.state.best == world.players[2]"), "\"join 2 new;join 2 back;\"\t1\ttrue", "the told from the save, then back");
}

/// A script's own thing hurt to death by `thing:hurt` names who did it in
/// `on_death` (a player of any seat), and with what; hit points set to 0
/// by hand name nobody.
#[test]
fn on_death_names_who_hurt_it() {
    let main = r#"
function on_level_start()
  level.state.a = level:spawn("gem", 10, 10)
  level.state.b = level:spawn("gem", 20, 20)
end
function on_death(thing, by, weapon)
  world.state.said = (world.state.said or "") .. (by and ("seat " .. tostring(by.seat)) or "nobody") .. " " .. tostring(weapon) .. ";"
end
function on_tick()
  if world.tick == 2 then level.state.a:hurt(2, player, "laser") end
  if world.tick == 4 then level.state.b.state.hp = 0 end
end
"#;
    let mut g = game("death-by", main);
    steps(&mut g, 6);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.eval("return world.state.said").unwrap(), "\"seat 1 laser;nobody nil;\"");
}

/// A copy of one of the templates in a temporary folder.
fn template_copy(template: &str, name: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(entry.file_name()));
            } else {
                std::fs::copy(&path, to.join(entry.file_name())).unwrap();
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("voxcade-lua-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates").join(template), &dir);
    dir
}

/// The jump & run's ready-made rules with a hero for a second seat: it
/// comes in beside the first, runs and jumps by its own keys, takes a hit
/// on its own hearts (thrown back, safe a while), and goes as it leaves;
/// seat 1's hero plays as ever.
#[test]
fn the_jump_and_run_has_a_hero_for_each_seat() {
    let dir = template_copy("platformer", "two-heroes");
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 30);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return world.players[2].kind, world.players[2].state.hp, world.players[2].x == player.x"), "\"hero2\"\t3\ttrue", "c.player.kind's for seat 2");
    let x = said(&mut g, "return world.players[2].x").parse::<f32>().unwrap();
    g.set_seat(2, Some(Input { right: true, ..Input::default() }));
    steps(&mut g, 30);
    let now = said(&mut g, "return world.players[2].x, player.x").split('\t').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
    assert!(now[0] > x + 10.0, "it ran right by its own keys: {x} to {}", now[0]);
    assert_eq!(now[1], x, "seat 1's hero stood still");
    let jump = Input { actions: 1, ..Input::default() };
    g.set_seat(2, Some(jump));
    steps(&mut g, 10);
    assert_eq!(said(&mut g, "return world.players[2].on_ground"), "false", "it jumped");
    g.set_seat(2, Some(Input::default()));
    said(&mut g, "world.players[2]:hurt(1)");
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2].state.hp, world.players[2].state.invulnerable ~= nil, player.state.hp"), "2\ttrue\t3");
    g.set_seat(2, None);
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2], #level:players(), level:count()"), format!("false\t1\t{}", said(&mut g, "return level:count()")));
    assert_eq!(halted(&g), "");
}

/// The shooter plays several (`docs/game-model.md`, "Several players"):
/// each seat's player a figure of `c.player.kind`'s with the run's start of
/// its own (hit points, gun, rounds), moved and throwing by its own keys
/// (its grenade its own), hit on its own; it leaves and its figure goes.
#[test]
fn the_shooter_has_a_soldier_for_each_seat() {
    let dir = template_copy("shooter", "two-soldiers");
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 10);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return player.kind, player.state.hp, player.state.weapon, player.state.inventory.grenade"), "\"hero\"\t5\t\"rifle\"\t5");
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    let second = "return world.players[2].kind, world.players[2].state.hp, world.players[2].state.weapon, world.players[2].state.inventory.grenade";
    assert_eq!(said(&mut g, second), "\"hero2\"\t5\t\"rifle\"\t5", "the run's start, its own");
    let x = said(&mut g, "return player.x").parse::<f32>().unwrap();
    g.set_seat(2, Some(Input { right: true, ..Input::default() }));
    steps(&mut g, 20);
    let now = said(&mut g, "return world.players[2].x, player.x").split('\t').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
    assert!(now[0] > x + 10.0, "it ran right by its own keys: {x} to {}", now[0]);
    assert_eq!(now[1], x, "seat 1 stood still");
    let grenade = g.actions().iter().position(|a| *a == "grenade").unwrap();
    g.set_seat(2, Some(Input { actions: 1 << grenade, ..Input::default() }));
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2].state.inventory.grenade, player.state.inventory.grenade"), "4\t5", "its own grenade thrown");
    said(&mut g, "world.players[2]:hurt(1)");
    steps(&mut g, 1);
    assert_eq!(said(&mut g, "return world.players[2].state.hp, world.players[2].state.invulnerable ~= nil, player.state.hp"), "4\ttrue\t5");
    said(&mut g, "world.players[2].state.weapon = 'knife'");
    assert_eq!(said(&mut g, "return world.players[2].state.weapon, player.state.weapon"), "\"knife\"\t\"rifle\"");
    g.set_seat(2, None);
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2], #level:players()"), "false\t1");
    assert_eq!(halted(&g), "");
}

/// The shooter's riflemen go for the nearest player in the fight, and a
/// soldier falling to a player's rounds is that player's kill (the
/// template gives it the points).
#[test]
fn the_shooters_enemies_go_for_the_nearest_player() {
    let dir = template_copy("shooter", "two-soldiers-fight");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap().replacen("c.camera.hold = { 220, 100 }", "c.camera.hold = { 400, 400 }", 1);
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    // Seat 2 up among the riflemen, seat 1 out of their sight.
    said(&mut g, "world.players[2]:place(128, 190)");
    steps(&mut g, 240);
    assert_eq!(halted(&g), "");
    let hp = said(&mut g, "return world.players[2].state.hp, player.state.hp").split('\t').map(|v| v.parse::<i32>().unwrap()).collect::<Vec<_>>();
    assert!(hp[0] < 5, "the nearest player was shot at: {hp:?}");
    assert_eq!(hp[1], 5, "the far one not");
    let fire = g.actions().iter().position(|a| *a == "fire").unwrap();
    for _ in 0..400 {
        g.set_seat(2, Some(Input { up: true, actions: 1 << fire, ..Input::default() }));
        g.step(&Input::default());
    }
    assert_eq!(halted(&g), "");
    let scores = said(&mut g, "return world.players[2] and world.players[2].state.score, player.state.score");
    assert!(scores.ends_with("\t0") && !scores.starts_with("0") && !scores.starts_with("false"), "seat 2's kills its own: {scores}");
}

/// Teams in the shooter (`p.state.team`): players of one team spare each
/// other (the start: co-op), of two they hurt each other, and the hit is
/// the shooter's (`by`); the riflemen's rounds hurt both as before.
#[test]
fn the_shooters_players_of_two_teams_hurt_each_other() {
    let dir = template_copy("shooter", "two-teams");
    let main = std::fs::read_to_string(dir.join("main.lua")).unwrap();
    let main = format!("{main}\nfunction on_hit(thing, by)\n  if thing.seat == 2 and by and by.seat then level.state.hit_by = by.seat end\nend\n");
    std::fs::write(dir.join("main.lua"), main).unwrap();
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    let fire = g.actions().iter().position(|a| *a == "fire").unwrap();
    // Seat 2 a little ahead of seat 1, who faces it and fires.
    let duel = |g: &mut Session<LuaGame>| {
        said(g, "player:place(128, 40); world.players[2]:place(128, 80)");
        g.step(&Input { up: true, ..Input::default() });
        for _ in 0..90 {
            g.step(&Input { actions: 1 << fire, ..Input::default() });
        }
        assert_eq!(halted(g), "");
        said(g, "return world.players[2].state.hp, level.state.hit_by")
    };
    assert_eq!(duel(&mut g), "5\tnil", "one team: spared");
    said(&mut g, "player.state.team = 'red'; world.players[2].state.team = 'blue'");
    assert_eq!(said(&mut g, "return player.state.team, world.players[2].state.team"), "\"red\"\t\"blue\"");
    let after = duel(&mut g);
    let hp = after.split('\t').next().unwrap().parse::<i32>().unwrap();
    assert!(hp < 5, "two teams: hurt ({after})");
    assert!(after.ends_with("\t1"), "seat 1's hit: {after}");
}

/// A shooter run of two plays back the same, and going back every 8
/// ticks as rollback would.
#[test]
fn a_shooter_run_of_two_replays_the_same() {
    use crate::replay::{Recorder, check, check_going_back};
    let dir = template_copy("shooter", "two-soldiers-replay");
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    let fire = 1 << g.actions().iter().position(|a| *a == "fire").unwrap();
    let grenade = 1 << g.actions().iter().position(|a| *a == "grenade").unwrap();
    for tick in 0..420 {
        let seat = match tick {
            0..30 => None,
            30..200 => Some(Input { up: true, right: tick % 60 < 20, actions: if tick % 9 == 0 { fire } else { 0 }, ..Input::default() }),
            200..205 => Some(Input { actions: grenade, ..Input::default() }),
            _ => Some(Input { up: tick % 40 < 25, left: tick % 80 < 10, actions: fire, ..Input::default() }),
        };
        g.set_seat(2, seat);
        let input = Input { up: tick % 50 < 35, actions: if tick % 13 == 0 { fire } else { 0 }, ..Input::default() };
        g.step(&input);
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    let replay = recorder.replay(&g).unwrap();
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/two.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "played back the same");
    assert!(check_going_back::<LuaGame>(&dir, 8).unwrap(), "going back the same");
}

/// The jump & run's heroes are held together as a game's own figures are
/// (`c.camera.hold`), and the camera looks at their middle.
#[test]
fn the_jump_and_runs_heroes_are_held_together() {
    let dir = template_copy("platformer", "two-heroes-held");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap().replacen("c.camera.hold = { 230 }", "c.camera.hold = { 30 }", 1);
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 30);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    g.set_seat(2, Some(Input { right: true, ..Input::default() }));
    steps(&mut g, 60);
    assert_eq!(halted(&g), "");
    let apart = said(&mut g, "return world.players[2].x - player.x").parse::<f32>().unwrap();
    assert!((apart - 30.0).abs() < 0.01, "held at the hold: {apart}");
    let (a, b) = (said(&mut g, "return player.x").parse::<f32>().unwrap(), said(&mut g, "return world.players[2].x").parse::<f32>().unwrap());
    let target = g.view_target(1.0);
    assert!((target.x - (a + b) * 0.5).abs() <= 1.0, "the middle: {} between {a} and {b}", target.x);
}

/// The state hash in parts (`docs/multiplayer.md`, "Desyncs"): each part
/// the hash after it, the last of the game's the state hash itself, then
/// the ticks played; a difference in one state names that part, and what
/// was played stays the same, while a different tick shows in `inputs`.
#[test]
fn the_hash_in_parts_names_where_two_games_differ() {
    let main = "function on_level_start()\n  level:fill(0, 0, 0, 63, 63, 0, 1)\n  level.state.n = 0\nend\nfunction on_tick()\n  world.state.t = world.tick\nend\n";
    let (mut a, mut b) = (game("parts-a", main), game("parts-b", main));
    steps(&mut a, 30);
    steps(&mut b, 30);
    let parts = a.sections();
    let names: Vec<&str> = parts.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["things", "world.state", "player.state", "level.state", "levels", "kept", "thing.state", "seats", "music", "inputs"]);
    assert_eq!(parts[parts.len() - 2].1, a.game.state_hash(), "the last of the game's parts is its hash");
    assert_eq!(a.sections(), b.sections());
    b.game.eval("level.state.n = 1").unwrap();
    let first_apart = |a: &Session<LuaGame>, b: &Session<LuaGame>| a.sections().into_iter().zip(b.sections()).find(|(x, y)| x.1 != y.1).map(|(x, _)| x.0);
    assert_eq!(first_apart(&a, &b), Some("level.state"));
    assert_eq!(a.sections().last(), b.sections().last(), "both played the same");
    let (mut c, mut d) = (game("parts-c", main), game("parts-d", main));
    c.step(&Input::default());
    d.step(&Input { up: true, ..Input::default() });
    assert_ne!(c.sections().last(), d.sections().last(), "another key held shows in what was played");
}

/// Every tick as it was played (`Session::played`), fed again whole as the
/// network will feed it (`Session::play`), plays the same run: the same
/// states after every tick, the same recording byte for byte, a save
/// stamped with the time the tick brought, not the machine's.
#[test]
fn ticks_played_again_whole_give_the_same_run() {
    use crate::replay::Recorder;
    let saving = |name: &str| {
        let g = two_players(name, TWO_PLAYERS);
        let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.players = 2", "c.players = 2\n  c.saves = { slots = 2 }");
        std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
        game_in(&g.dir)
    };
    let mut a = saving("feed-a");
    let (mut ticks, mut hashes, mut recorder) = (Vec::new(), Vec::new(), Recorder::default());
    a.set_clock(1_761_000_000);
    for n in 0..240u64 {
        if (20..200).contains(&n) {
            a.set_seat(2, Some(Input { right: n % 40 < 20, actions: if n % 7 == 0 { 2 } else { 0 }, ..Input::default() }));
        }
        if n == 200 {
            a.set_seat(2, None);
        }
        if n == 100 {
            a.eval("world.state.helped = true").unwrap();
        }
        if n == 150 {
            a.set_clock(1_761_000_123);
            a.save_to(1).unwrap();
            a.eval("world.state.saved = world.tick").unwrap();
        }
        a.step(&Input { left: n % 50 < 10, actions: if n % 11 == 0 { 2 } else { 0 }, ..Input::default() });
        ticks.push(a.played().clone());
        hashes.push(a.game.state_hash());
        recorder.record(&a);
    }
    assert_eq!(halted(&a), "");
    let mut b = saving("feed-b");
    b.set_clock(5);
    let mut again = Recorder::default();
    for (n, tick) in ticks.iter().enumerate() {
        b.play(tick);
        assert_eq!(b.played(), tick, "tick {n}");
        assert_eq!(b.game.state_hash(), hashes[n], "tick {n}");
        again.record(&b);
    }
    assert_eq!(b.sections(), a.sections());
    let text = |r: &Recorder, g: &Session<LuaGame>| toml::to_string(&r.replay(g).unwrap()).unwrap();
    assert_eq!(text(&again, &b), text(&recorder, &a));
    assert!(a.slots.read(1).unwrap().starts_with("when = 1761000123"), "{:?}", a.slots.read(1));
    assert_eq!(b.slots.read(1), a.slots.read(1), "stamped with the time the tick brought");
}

/// What a machine has of the game (`game::Local`): `frame.players` its own
/// seats, a key in braces and `controls` in `on_draw` the first of them's,
/// `world.language` there its own; the simulation goes by seat 1 as
/// before, the game is the same (its hash), and the high score lines are
/// its players'.
#[test]
fn each_machine_draws_its_own_players() {
    let main = r#"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, 1)
end
function on_tick()
  world.state.lang = world.language
end
function on_draw(frame)
  local first = frame.players[1]
  frame:text(#frame.players .. " " .. (first and first.seat or 0) .. " {fire} " .. controls:key("fire") .. " " .. world.language, 1, 1)
end
"#;
    let mut g = two_players("local", main);
    let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.players = 2", "c.players = 2\n  c.languages = { \"en\", \"de\" }");
    std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
    g = game_in(&g.dir.clone());
    let keys = |fire: &str, language: &str| {
        let mut c = crate::controls::Controls::nobody(g.actions());
        c.keys.insert("fire".into(), fire.into());
        c.language = Some(language.into());
        c
    };
    let (one, two) = (keys("X", "en"), keys("K", "de"));
    g.set_controls(one);
    g.set_seat(2, Some(Input::default()));
    g.set_seat_controls(2, two);
    steps(&mut g, 2);
    g.game.eval("player.state.score = 30; world.players[2].state.score = 70").unwrap();
    g.look();
    assert_eq!(first_text(&g), "2 1 X X en", "at one machine every seat, seat 1's keys");
    assert_eq!(g.local_scores(), [(1, 30), (2, 70)]);
    let hash = g.game.state_hash();
    g.set_local(crate::game::Local { seats: Some(vec![2]), language: Some("de".into()) });
    g.look();
    assert_eq!(first_text(&g), "1 2 K K de", "the machine of seat 2");
    assert_eq!(g.local_scores(), [(2, 70)]);
    assert_eq!(g.game.state_hash(), hash, "only the picture");
    steps(&mut g, 1);
    assert_eq!(g.game.eval("return world.state.lang").unwrap(), "\"en\"", "the simulation speaks seat 1's");
    g.set_local(crate::game::Local { seats: Some(Vec::new()), language: None });
    g.look();
    assert_eq!(first_text(&g), "0 0 X X en", "a machine that only watches: seat 1's keys");
}

/// Every key a game or a template binds to start with is one SDL knows by
/// that name (Pulsar's "LShift" was none: Shift never bombed, and only the
/// window's start said so).
#[test]
fn every_games_keys_are_keys() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    for base in ["games", "templates"] {
        for entry in std::fs::read_dir(root.join(base)).unwrap().flatten() {
            let dir = entry.path();
            if !dir.join("conf.lua").is_file() {
                continue;
            }
            let cfg = crate::session::config::<LuaGame>(&dir).unwrap();
            for name in LuaGame::default_keys(&cfg).names() {
                assert!(sdl3::keyboard::Scancode::from_name(name).is_some(), "{}: no key named '{name}' (SDL's names: \"Left Shift\", \"Space\", \"Z\")", dir.display());
                seen += 1;
            }
        }
    }
    assert!(seen > 20, "{seen} keys looked at");
}

/// `frame:model` of a file the game does not know is a mistake that halts
/// the game and says where models come from; it used to draw nothing.
#[test]
fn an_unknown_model_halts_the_game() {
    let conf = |models: &str| format!("
function conf(c)
  c.world.size = {{ 64, 64, 16 }}
  c.things.robot = {{ model = \"robot.vvx\" }}
  {models}
end
");
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.r = level:spawn("robot", 32, 32)
end

function on_draw(frame)
  frame:model("ghost.vvx", level.state.r)
end
"##;
    let dir = folder("unknown-model", main);
    crate::vox::write_figure(&dir.join("robot.vvx"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO)]);
    crate::vox::write_figure(&dir.join("ghost.vvx"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO)]);
    std::fs::write(dir.join("conf.lua"), conf("")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let _ = g.frame(1.0, 16.0 / 9.0);
    let why = halted(&g);
    assert!(why.contains("no model called 'ghost.vvx'") && why.contains("c.models"), "{why}");
    // Listed in `c.models` it is the game's, and draws.
    std::fs::write(dir.join("conf.lua"), conf("c.models = { \"ghost.vvx\" }")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let frame = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(halted(&g), "");
    assert_eq!(frame.models.len(), 1, "the ghost is drawn");
}

/// A kind with `fixed = true` has no gravity and does not land: it stays
/// where it is spawned or placed. One without falls to the ground.
#[test]
fn a_fixed_thing_stays_where_it_is_put() {
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.sign = level:spawn("sign", 20, 20, { z = 10 })
  level.state.rock = level:spawn("rock", 30, 30, { z = 10 })
end

function on_tick()
  world.state.sign = math.floor(level.state.sign.z)
  world.state.rock = math.floor(level.state.rock.z)
  if world.tick == 5 then level.state.sign:place(40, 40, 7) end
end
"##;
    let dir = folder("fixed", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.things.sign = { fixed = true }\n  c.things.rock = {}\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 4);
    assert_eq!(state(&mut g, "sign"), "Int(10)", "the sign hangs in the air");
    assert_ne!(state(&mut g, "rock"), "Int(10)", "the rock falls");
    steps(&mut g, 40);
    assert_eq!(state(&mut g, "sign"), "Int(7)", "placed, it stays there");
    assert_eq!(state(&mut g, "rock"), "Int(1)", "on the floor");
    // With a behaviour it is the ready-made rules' to move: a mistake.
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"shooter\"\n  c.things.sign = { behaviour = \"soldier\", fixed = true }\nend\n").unwrap();
    let err = start_error(&dir);
    assert!(err.contains("c.things.sign.fixed"), "{err}");
}

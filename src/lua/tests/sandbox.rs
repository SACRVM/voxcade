//! The sandbox: globals frozen after the start, on_draw only looking, the
//! safe libraries, ordered pairs, Lua's maths through libm, budgets.

use super::*;

/// Globals stay as `on_world_start` left them: a new one or a changed one
/// in `on_tick` is a mistake that says where state belongs; a global table
/// is only to look at.
#[test]
fn globals_are_frozen_after_load() {
    let mut g = game("frozen", "n = 1\nfunction on_world_start() m = 2 end\nfunction on_tick() n = n + 1 end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("'n' is a global variable: what changes belongs in a state (world.state, player.state, thing.state)"), "{}", halted(&g));
    // A global table read in on_tick and changed there: named as the
    // global it is, down to the key (it once said "only to look at here:
    // change the game in on_tick", in on_tick).
    let mut g = game("frozen-table", "T = { n = 0, inner = { 1 } }\nfunction on_tick() T.inner[1] = 2 end\n");
    steps(&mut g, 1);
    assert!(halted(&g).starts_with("main.lua:2: T.inner[1]: 'T' is a global variable, kept as it was at the start: what changes belongs in a state"), "{}", halted(&g));
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
    assert_eq!(state(&mut g, "n"), format!("{:?}", crate::lua::Value::Int(2)), "held still");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "n"), format!("{:?}", crate::lua::Value::Int(3)), "on again");
    assert_eq!(g.game.sim().freeze, 60, "a second at most");
    steps(&mut g, 60);
    assert_eq!(state(&mut g, "n"), format!("{:?}", crate::lua::Value::Int(3)), "held still a second");
    assert!(g.game.sim().shake > 0.0, "the shake holds still with the game");
    steps(&mut g, 10);
    let sim = g.game.sim();
    assert_eq!((sim.shake, sim.screen_flash), (0.0, 0.0), "both died down");
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
    assert_eq!(state(&mut g, "order"), format!("{:?}", crate::lua::Value::Str(b"123abctrue123abctrue".to_vec())));
    assert_eq!(state(&mut g, "first"), format!("{:?}", crate::lua::Value::Str(b"1".to_vec())));
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
    assert_eq!(state(&mut g, "s"), format!("{:?}", crate::lua::Value::Str(b"tablefunctiontablefunction".to_vec())));
    let dir = folder("format-p", "function on_world_start() local s = string.format('%p', {}) end\n");
    assert!(!start_error(&dir).is_empty());
}

/// The maths give the same bits on every system (libm).
#[test]
fn maths_go_through_libm() {
    let mut g = game("maths", "function on_world_start() world.state.s = math.sin(1.0) + math.pow(2.0, 0.5) + math.atan(1.0, 2.0) end\n");
    let want = libm::sin(1.0) + libm::pow(2.0, 0.5) + libm::atan2(1.0, 2.0);
    assert_eq!(state(&mut g, "s"), format!("{:?}", crate::lua::Value::Float(want.to_bits())));
}

/// Degrees and radians as Lua has them (a scene leans a model by the
/// camera's pitch).
#[test]
fn degrees_and_radians_are_there() {
    let mut g = game("rad", "function on_world_start() world.state.r = math.rad(180) world.state.d = math.deg(math.pi / 2) end\n");
    assert_eq!(state(&mut g, "r"), format!("{:?}", crate::lua::Value::Float(std::f64::consts::PI.to_bits())));
    assert_eq!(state(&mut g, "d"), format!("{:?}", crate::lua::Value::Float(90f64.to_bits())));
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
    assert_eq!(state(&mut g, "r"), format!("{:?}", crate::lua::Value::Bool(true)));
    g.compose(0.0, 1.6, Scene::Play, None);
    assert_eq!(first_text(&g), "input", "outside on_tick it only names itself");
}

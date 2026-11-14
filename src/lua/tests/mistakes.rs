//! The mistakes a script meets, in its words: the file, the line and the
//! name nearly meant.

use super::*;

/// An error names the file and line; tools without a window end with it.
#[test]
fn a_lua_error_names_file_and_line() {
    let mut g = game("error", "function on_tick()\n  local x = nil\n  return x.y\nend\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("main.lua:3:"), "{}", halted(&g));
    assert!(g.stopped().is_err());
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

/// A thing's field is read, never assigned: `player.x = 5` says what to do
/// instead, not "attempt to index a ThingRef value".
#[test]
fn assigning_a_thing_field_says_what_to_do() {
    let dir = folder("assign-field", "function on_level_start() level:spawn('gem', 10, 10) end\nfunction on_tick() level:things('gem')[1].x = 5 end\n");
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let e = halted(&g);
    assert!(e.contains("thing.x cannot be set") && e.contains("thing.state"), "{e}");
}

/// A mistake in a call to the engine is said as the script calls it, the
/// file and line first: a method called with a dot, an argument of the
/// wrong type named by the register's parameter, a fraction to
/// `math.random`, and the engine's own mistakes (no kind of that name) at
/// the line that made them; never mlua's or Rust's names.
#[test]
fn a_call_to_the_engine_says_what_it_wanted_where() {
    for (main, says) in [
        ("function on_tick()\n  level.spawn('miner', 10, 10)\nend\n", "main.lua:2: level:spawn is a method: call it with a colon, level:spawn(...), not level.spawn(...)"),
        ("function on_tick()\n  screen:shake('big')\nend\n", "main.lua:2: screen:shake(amount): amount should be a number, not a string"),
        ("function on_tick()\n  level:spawn('miner', nil, 3)\nend\n", "main.lua:2: level:spawn(kind, x, y, options?): x should be a number, not nil"),
        ("function on_tick()\n  level:voxel(1.5, 2, 3)\nend\n", "main.lua:2: level:voxel(x, y, z): x should be a whole number, not a fraction"),
        ("function on_tick()\n  local n = math.random(1.5)\nend\n", "main.lua:2: math.random: whole numbers, (), (n) or (m, n); not 1.5"),
        ("function on_tick()\n  level:spawn('heroo', 1, 1)\nend\n", "main.lua:2: no kind of thing called 'heroo'"),
        ("function on_draw(frame)\n  frame:text(nil, 1, 1)\nend\n", "main.lua:2: frame:text(text, x, y, color?, size?): text should be a string, not nil"),
    ] {
        let mut g = game("calls", main);
        steps(&mut g, 2);
        g.look();
        let e = halted(&g);
        assert!(e.starts_with(says), "{main}: {e}");
        for internal in ["LevelView", "ScreenView", "Frame.", "userdata", "f32", "i64", "error converting"] {
            assert!(!e.lines().next().unwrap_or("").contains(internal), "{main}: {e}");
        }
    }
    // math.random takes whole numbers however written, as Lua's own.
    let mut g = game("random", "function on_tick() world.state.n = math.random(3.0) + math.random(2, '4') end\n");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
}

/// A global function nearly a handler's name (`on_tik`) is a mistake as
/// the game loads, naming the handler; a function of the game's own that
/// begins with on_ but is no near name is its own.
#[test]
fn a_misspelt_handler_is_named() {
    for (main, says) in [
        ("function on_tik() end\n", "main.lua:1: function on_tik is no handler the engine calls: did you mean on_tick?"),
        ("\nfunction on_level_strat(how) end\n", "main.lua:2: function on_level_strat is no handler the engine calls: did you mean on_level_start?"),
        ("function on_Draw(frame) end\n", "did you mean on_draw?"),
        ("function on_touched(a, b) end\n", "did you mean on_touch?"),
    ] {
        let e = start_error(&folder("near-handler", main));
        assert!(e.contains(says), "{main}: {e}");
    }
    for own in ["function on_coin_taken() end\n", "function on_kill() end\n", "on_tic = 3\n", "local function on_tik() end\n"] {
        assert_eq!(start_error(&folder("own-on", own)), "", "{own}");
    }
}

/// A voxel set outside the level is a mistake naming the level's size
/// (it did nothing, silently); a box filled across the edge fills what
/// lies inside. An action asked of a game that names none says so.
#[test]
fn a_voxel_outside_the_level_is_a_mistake() {
    let mut g = game("outside", "function on_tick()\n  level:set_voxel(1000, 1, 1, 3)\nend\n");
    steps(&mut g, 1);
    assert!(halted(&g).starts_with("main.lua:2: level:set_voxel: 1000, 1, 1 is outside the level (x 0 to 63, y 0 to 63, z 0 to 15"), "{}", halted(&g));
    let mut g = game("fill-edge", "function on_tick() level:fill(60, 60, 0, 70, 70, 0, 3) end\n");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.sim().world.get(glam::IVec3::new(63, 63, 0)), 3);
    let dir = crate::scratch::Scratch::new("lua-no-actions");
    std::fs::write(dir.join("conf.lua"), "function conf(c) c.world.size = { 16, 16, 8 } end\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_tick() input:pressed('jump') end\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert!(halted(&g).contains("no action called 'jump': the game names no actions (c.actions = { \"jump\" } in conf.lua)"), "{}", halted(&g));
}

/// A name an engine's object does not have is a mistake at its line, not a
/// silent nil: the object, the name, the one meant if near (a method with
/// `:`, a field with `.`), else what it has; `input.jump` says how an
/// action is asked; setting one says it is only to read; the objects go by
/// their scripts' names, never the engine's Rust types. Optional fields
/// (`thing.name`, `thing.wall`, `level.next`) stay nil where nothing is.
#[test]
fn a_name_an_object_lacks_is_a_mistake() {
    for (line, says) in [
        ("local a = world.tik", "main.lua:2: world has no `tik`: did you mean `world.tick`?"),
        ("local a = level.szie", "main.lua:2: level has no `szie`: did you mean `level.size`?"),
        ("level:spwan('gem', 1, 1)", "main.lua:2: level has no `spwan`: did you mean `level:spawn`?"),
        ("player:wlak({ x = 1, y = 0 }, 10)", "main.lua:2: player has no `wlak`: did you mean `player:walk`?"),
        ("local a = player.helth", "main.lua:2: player has no `helth` (it has player.controls,"),
        ("local a = input.jump", "main.lua:2: input has no `jump`: an action is asked by its name: input:held(\"jump\")"),
        ("local a = input.shoot", "main.lua:2: input has no `shoot`: an action is asked by its name: input:held(\"shoot\"), input:pressed(\"shoot\"), input:released(\"shoot\") (c.actions: jump, fire)"),
        ("local a = input.lfet", "main.lua:2: input has no `lfet`: did you mean `input.left`?"),
        ("level.camera:folow(player)", "main.lua:2: level.camera has no `folow`: did you mean `level.camera:follow`?"),
        ("local a = controls.devise", "main.lua:2: controls has no `devise`: did you mean `controls.device`?"),
        ("local a = music.baet", "main.lua:2: music has no `baet`: did you mean `music.beat`?"),
        ("screen:shak(1)", "main.lua:2: screen has no `shak`: did you mean `screen:shake`?"),
        ("local a = level:spawn('gem', 1, 1).hp", "main.lua:2: thing has no `hp`"),
        ("world.foo = 1", "main.lua:2: world has no `foo` to set: the game's own data goes in world.state"),
        ("world.tick = 1", "main.lua:2: world.tick is only to read: the game's own data goes in world.state"),
        ("player.x = 1", "main.lua:2: player.x cannot be set"),
        ("music.tempo = 1", "main.lua:2: music has no `tempo` to set"),
        ("local s = 'x' .. world", "main.lua:2: attempt to concatenate a world value"),
    ] {
        let mut g = game("no-such", &format!("function on_tick()\n  {line}\nend\n"));
        steps(&mut g, 1);
        let e = halted(&g);
        assert!(e.starts_with(says), "{line}: {e}");
        for internal in ["WorldView", "LevelView", "ThingRef", "InputView", "__mlua", "prelude:"] {
            assert!(!e.contains(internal), "{line}: {e}");
        }
    }
    let mut g = game("no-such-draw", "function on_draw(frame)\n  local a = frame.widht\nend\n");
    steps(&mut g, 1);
    g.look();
    assert!(halted(&g).starts_with("main.lua:2: frame has no `widht`: did you mean `frame.width`?"), "{}", halted(&g));
    let mut g = game("optional", "function on_tick()\n  local t = level:spawn('gem', 5, 5)\n  world.state.n = tostring(t.name) .. tostring(t.wall) .. tostring(level.next) .. tostring(music.song)\nend\n");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
}

/// A mistake inside one of the sandbox's wrappers (`string.format`,
/// `pairs`, `setmetatable`, `rawset`, `next`) is said at the script's line
/// under the name the script called, never at the prelude's; an endless
/// loop and a script out of memory name the line that ran.
#[test]
fn a_mistake_in_a_wrapper_is_said_at_the_scripts_line() {
    for (line, says) in [
        ("local s = string.format('%d', 1.5)", "main.lua:2: bad argument #2 to 'format' (number has no integer representation)"),
        ("local s = ('%d'):format(1.5)", "main.lua:2: bad argument #2 to 'format'"),
        ("for k in pairs(nil) do end", "main.lua:2: bad argument #1 to 'pairs' (table expected, got nil)"),
        ("local k = next(3)", "main.lua:2: bad argument #1 to 'next' (table expected, got number)"),
        ("setmetatable(1, {})", "main.lua:2: bad argument #1 to 'setmetatable' (table expected, got number)"),
        ("rawset({}, nil, 1)", "main.lua:2: table index is nil"),
        ("table.sort({ 1, 'a' })", "main.lua:2: attempt to compare"),
        ("local s = string.format('%p', {})", "main.lua:2: string.format: %p"),
        ("while true do end", "main.lua:2: on_tick ran over 20 million instructions"),
        ("local ok = pcall(function() while true do end end)", "main.lua:2: on_tick ran over 20 million instructions"),
        ("local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1000) .. i end", "main.lua:2: not enough memory"),
    ] {
        let mut g = game("wrapper", &format!("function on_tick()\n  {line}\nend\n"));
        steps(&mut g, 1);
        let e = halted(&g);
        assert!(e.starts_with(says), "{line}: {e}");
        assert!(!e.contains("prelude:") && !e.contains("raw_"), "{line}: {e}");
    }
}

/// A library the sandbox leaves out (`os`, `io`, `coroutine`, `load` …)
/// is said to be so, and `frame` outside `on_draw` to be on_draw's alone,
/// not "attempt to index a nil value"; out of memory in one step says Lua
/// tells no line rather than promising one it does not show.
#[test]
fn a_global_that_is_not_there_says_why() {
    for (line, says) in [
        ("local t = os.time()", "main.lua:2: os is not there in a game (the sandbox): no clock and no files"),
        ("io.write('x')", "main.lua:2: io is not there in a game (the sandbox): no files"),
        ("coroutine.wrap(function() end)()", "main.lua:2: coroutine is not there in a game (the sandbox)"),
        ("load('x = 1')", "main.lua:2: load is not there in a game (the sandbox): no code made from text"),
        ("collectgarbage()", "main.lua:2: collectgarbage is not there in a game (the sandbox)"),
        ("debug.traceback()", "main.lua:2: debug is not there in a game (the sandbox)"),
        ("frame:text('a', 0, 0)", "main.lua:2: frame is only there in on_draw: function on_draw(frame) is given it"),
        ("nothing_here.x = 1", "main.lua:2: attempt to index a nil value (global 'nothing_here')"),
        ("local s = string.rep('x', 1000000000)", "not enough memory: a game's Lua holds 256 MB at most (Lua tells no line"),
    ] {
        let mut g = game("absent", &format!("function on_tick()\n  {line}\nend\n"));
        steps(&mut g, 1);
        let e = halted(&g);
        assert!(e.starts_with(says), "{line}: {e}");
        assert!(!e.contains("the line is the one last seen"), "{line}: {e}");
    }
}

/// `frame:model` given a point says it draws where a thing stands; a spawn
/// outside the level is a mistake but for a fixed kind, which may wait
/// there (one coming in from off the screen).
#[test]
fn a_model_at_a_point_and_a_spawn_outside_say_so() {
    let mut g = game("model-point", "function on_draw(frame)\n  frame:model('x.vvx', { x = 1, y = 2, z = 3 })\nend\n");
    steps(&mut g, 1);
    g.look();
    assert!(halted(&g).starts_with("main.lua:2: frame:model(file, thing, ...): a model is drawn where a thing stands, so the second is a thing, not a point"), "{}", halted(&g));
    let mut g = game("spawn-outside", "function on_tick()\n  level:spawn('gem', -500, 9999)\nend\n");
    steps(&mut g, 1);
    assert!(halted(&g).starts_with("main.lua:2: level:spawn: -500, 9999 lies outside the level (0 to 64 across, 0 to 64 deep): only a kind with fixed = true"), "{}", halted(&g));
    let dir = folder("spawn-outside-fixed", "function on_tick()\n  if world.tick == 1 then level:spawn('post', -20, 10) end\nend\n");
    std::fs::write(dir.join("conf.lua"), CONF.replace("c.things.miner = {}", "c.things.miner = {}\n  c.things.post = { fixed = true }")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
}

/// A setting's slip at the top of `conf.lua` (`c.titel`) names the one
/// meant; a kind's missing model names the kind's setting.
#[test]
fn a_slip_in_conf_names_the_setting_meant() {
    let dir = folder("conf-slip", "");
    std::fs::write(dir.join("conf.lua"), CONF.replace("c.things.miner = {}", "c.titel = 'X'")).unwrap();
    assert_eq!(start_error(&dir), "conf.lua: c.titel: no such setting (did you mean `c.title`?)");
    std::fs::write(dir.join("conf.lua"), CONF.replace("c.things.miner = {}", "c.things.miner = { model = 'nope.vvx' }")).unwrap();
    assert_eq!(start_error(&dir), "c.things.miner.model: no file \"nope.vvx\" in the game folder");
}

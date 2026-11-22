//! Tables a script hands the engine, read as the script sees them: a
//! state's table in `on_draw` (a read-only view there), a frozen global's,
//! the same as the literal they hold.

use super::*;
use crate::lua::world::Cmd;

/// What `on_draw` asked for, each as text, in order.
fn drawn(g: &Session<LuaGame>) -> Vec<String> {
    g.game.drawn.borrow().iter().map(|c| format!("{c:?}")).collect()
}

/// The reported case: `frame:fog` given a table out of `level.state` in
/// `on_draw` sets the picture's fog as the same literal does (it set
/// nothing: the view looked empty to the engine).
#[test]
fn fog_from_a_state_in_on_draw_is_the_literals() {
    let main = r#"
function on_level_start()
  level.state.steps = { { fog = { density = 0.02, color = { 40, 50, 60 }, glow = 3 } } }
end
function on_draw(frame)
  frame:fog(level.state.steps[1].fog)
  frame:fog{ density = 0.02, color = { 40, 50, 60 }, glow = 3 }
end
"#;
    let mut g = game("fog-state", main);
    steps(&mut g, 1);
    g.compose(0.0, 1.6, Scene::Play, None);
    assert_eq!(halted(&g), "");
    let cmds = g.game.drawn.borrow().clone();
    let fogs: Vec<String> = cmds.iter().filter_map(|c| match c { Cmd::Fog(f) => Some(format!("{f:?}")), _ => None }).collect();
    assert_eq!(fogs.len(), 2);
    assert!(fogs[0].contains("Some(0.02)"), "the state's fog sets its density: {}", fogs[0]);
    assert_eq!(fogs[0], fogs[1], "from the state as from the literal");
}

/// The engine reading a view gives the script nothing new: the view stays
/// a view (empty to `rawget`, refusing a write and `rawset`), no name
/// leads to the table behind it, and the picture changes no state.
#[test]
fn reading_a_view_opens_nothing_to_the_script() {
    let main = r#"
function on_level_start()
  level.state.f = { density = 0.02 }
end
function on_draw(frame)
  local f = level.state.f
  frame:fog(f)
  local wrote = pcall(function() f.density = 1 end)
  local raw = pcall(rawset, f, "density", 1)
  frame:text(tostring(rawget(f, "density")) .. " " .. tostring(wrote) .. " " .. tostring(raw) .. " " .. tostring(shows) .. " " .. tostring(given), 0, 0)
end
"#;
    let mut g = game("given-closed", main);
    steps(&mut g, 1);
    let before = g.game.state_hash();
    g.compose(0.0, 1.6, Scene::Play, None);
    assert_eq!(halted(&g), "");
    assert_eq!(first_text(&g), "nil false false nil nil");
    assert_eq!(g.game.state_hash(), before, "the picture changed no state");
}

/// Outside `on_draw` too: a global's table after the start (a read-only
/// view everywhere), a state's nested table (plain there) and a top-level
/// state (`thing.state`, always the engine's view; read as its plain part,
/// the engine's keys not in it) are read as the script sees them: a way,
/// `level:spawn`'s options and the state in them (a view inside a literal
/// too), an ask's options.
#[test]
fn tables_handed_outside_on_draw_are_read_as_the_script_sees_them() {
    let main = r##"
DIRS = { west = { x = -1, y = 0 } }
START = { route = { 1, 2, 3 }, mood = "calm" }
SPAWN = { state = { route = { 4, 5 } }, name = "second" }
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.r = level:spawn("miner", 32, 32)
  level.state.dir = { x = 0, y = 1 }
  level.state.r.state.x = -1
  level.state.r.state.y = 0
end
function on_tick()
  if world.tick ~= 1 then return end
  local m = level.state.r
  m:face(level.state.dir)
  world.state.nested = m.facing.y
  m:face(DIRS.west)
  world.state.global = m.facing.x
  m:face(level.state.dir)
  m:face(m.state)
  world.state.top = m.facing.x
  local first = level:spawn("miner", 10, 10, { state = START, name = "first" })
  world.state.first = first.name .. " " .. first.state.mood .. " " .. #first.state.route
  local inner = level:spawn("miner", 12, 12, { state = { route = START.route } })
  world.state.inner = #inner.state.route
  local second = level:spawn("miner", 14, 14, SPAWN)
  world.state.second = (level:thing("second") and "found " or "lost ") .. #second.state.route
  local gem = level:spawn("gem", 16, 16, { state = m.state })
  world.state.gem = gem.state.x .. " " .. gem.state.hp
end
"##;
    let mut g = game("given-outside", main);
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    let said = |g: &mut Session<LuaGame>, key: &str| state(g, key);
    let int = |n: i64| format!("{:?}", crate::lua::Value::Int(n));
    let text = |s: &str| format!("{:?}", crate::lua::Value::Str(s.as_bytes().to_vec()));
    assert_eq!(said(&mut g, "nested"), int(1), "a state's nested table, a way");
    assert_eq!(said(&mut g, "global"), int(-1), "a global's table, a way");
    assert_eq!(said(&mut g, "top"), int(-1), "a thing's whole state, a way");
    assert_eq!(said(&mut g, "first"), text("first calm 3"), "a global as level:spawn's state");
    assert_eq!(said(&mut g, "inner"), int(3), "a global's table inside the state given");
    assert_eq!(said(&mut g, "second"), text("found 2"), "a global as level:spawn's options");
    assert_eq!(said(&mut g, "gem"), text("-1 2"), "a thing's state as another's: its plain part, the gem's own hp");
    // An ask's options from a global: read, so a wrong one is a mistake.
    let mut g = game("given-ask", "BAD = { fade = 99 }\nfunction on_tick() world:win(BAD) end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("world:win: fade is seconds"), "{}", halted(&g));
}

/// Every `frame` method that takes a table, called with the table where a
/// script finds it: `T` in the call is where it goes, the literal what it
/// holds. A call may hold `T` inside a literal of its own (a table in a
/// table). The register's methods with a parameter that takes a table
/// must each be here (`every_frame_method_reads_a_view_as_its_literal`).
const CALLS: &[(&str, &str, &str, &str)] = &[
    ("model", "rotation?", "frame:model('fig.vvx', level.state.r, nil, T)", "{ rotation = 30, pitch = 10, roll = 5 }"),
    ("model", "pose?", "frame:model('fig.vvx', level.state.r, nil, nil, T)", "{ arm = { pitch = 20, offset = { 0, 0.5, 0 } } }"),
    ("model", "pose?", "frame:model('fig.vvx', level.state.r, nil, nil, { arm = T })", "{ roll = 15, offset = { 0.25, 0, 0 } }"),
    ("pose", "pose", "frame:pose(level:thing('rig'), T)", "{ arm = { pitch = 20, glow = 1 } }"),
    ("pose", "pose", "frame:pose(level:thing('rig'), { arm = T })", "{ roll = 15, offset = { 0.25, 0, 0 }, glow = 0.5 }"),
    ("cube", "color?", "frame:cube(1, 2, 3, T, 0.5)", "{ 10, 20, 30, 200 }"),
    ("light", "at", "frame:light(T)", "{ at = { 5, 6, 7 }, radius = 30, color = { 255, 128, 0 }, cone = 40, pitch = -30, shadows = 'none', fog = 2 }"),
    ("light", "at", "frame:light(T, 30)", "{ x = 5, y = 6, z = 7 }"),
    ("light", "at", "frame:light{ at = T, radius = 20, color = { 9, 8, 7 } }", "{ 1, 2, 3 }"),
    ("light", "color?", "frame:light({ 5, 6, 7 }, 30, T)", "{ 255, 128, 0 }"),
    ("light", "color?", "frame:light{ at = { 5, 6, 7 }, radius = 30, color = T }", "{ 0, 64, 255 }"),
    ("daylight", "light", "frame:daylight(T)", "{ sun = { -1, 0.5, -1 }, sun_color = { 255, 200, 150 }, ambient = 0.3, shadow = 0.6, sky = { 10, 20, 40 } }"),
    ("daylight", "light", "frame:daylight{ sun = T }", "{ x = 1, y = 0, z = -2 }"),
    ("fog", "fog", "frame:fog(T)", "{ density = 0.02, color = { 40, 50, 60 }, anisotropy = 0.3, glow = 3 }"),
    ("fog", "fog", "frame:fog{ density = 0.01, color = T }", "{ 70, 80, 90 }"),
    ("camera", "camera", "frame:camera(T)", "{ look_at = { 30, 31, 4 }, width = 50, pitch = 40, fov = 30 }"),
    ("camera", "camera", "frame:camera{ look_at = T }", "{ x = 20, y = 21 }"),
    ("menu", "menu", "frame:menu(T)", "{ title = 'SHOP', tabs = { 'A', 'B' }, tab = 2, rows = { 'ONE', { text = 'TWO', value = 3, dim = true } }, at = 2, shown = 2, hint = 'GO', x = 4, y = 5, width = 60 }"),
    ("menu", "menu", "frame:menu{ rows = T, at = 1 }", "{ 'X', { text = 'Y', value = 'Z' } }"),
    ("grid", "grid", "frame:grid(T)", "{ x = 3, y = 4, columns = 2, cell = 10, title = 'BAG', cells = { { count = 3 }, false, { count = 1, dim = true } }, at = 3 }"),
    ("grid", "grid", "frame:grid{ cells = { T } }", "{ count = 7 }"),
    ("text", "color?", "frame:text('HI', 2, 3, T, 2)", "{ 1, 2, 3 }"),
    ("to_screen", "at", "frame:text('P', frame:to_screen(T))", "{ 32, 32, 2 }"),
    ("box", "color?", "frame:box(1, 2, 3, 4, T)", "{ 5, 6, 7, 8 }"),
    ("bar", "color?", "frame:bar(1, 2, 10, 2, 0.5, T)", "{ 50, 60, 70 }"),
    ("bar", "back?", "frame:bar(1, 2, 10, 2, 0.5, nil, T)", "{ 80, 90, 100 }"),
    ("drop_in", "color?", "frame:drop_in('GO', 4, 5, 30, T, 2)", "{ 11, 12, 13 }"),
    ("draw", "options?", "frame:draw('fig.vvx', 4, 5, T)", "{ frame = 0, rotation = 90 }"),
];

/// Where a script finds a table, `{}` the call's number: as a literal, a
/// state's (each a read-only view in `on_draw`), a global's after the
/// start (a read-only view everywhere).
const SOURCES: &[&str] = &["", "level.state.given[{}]", "world.state.given[{}]", "player.state.given[{}]", "level.state.r.state.given[{}]", "GIVEN[{}]"];

/// Whether a parameter's type (LuaLS's, as the register writes it) takes a
/// table: any of its alternatives but the plain values and a thing.
fn takes_a_table(ty: &str) -> bool {
    ty.split('|').any(|t| !matches!(t.trim(), "string" | "number" | "integer" | "boolean" | "Thing" | "nil"))
}

/// Every `frame` method that takes a table draws the same from a state's
/// table in `on_draw` (a read-only view), a thing's, the player's and a
/// global's after the start as from the literal they hold: the engine
/// reads a table handed to it as the script sees it. The calls are the
/// register's: a method that takes a table and is not in `CALLS` fails.
#[test]
fn every_frame_method_reads_a_view_as_its_literal() {
    use crate::lua::api::{API, What};
    let mut wanted: Vec<(String, String)> = API
        .iter()
        .filter(|i| i.of == "frame")
        .filter_map(|i| match &i.what {
            What::Method { params, .. } => Some(params.iter().filter(|(_, ty)| takes_a_table(ty)).map(|(p, _)| (i.name.to_string(), p.to_string())).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    wanted.sort();
    let mut covered: Vec<(String, String)> = CALLS.iter().map(|(m, p, ..)| (m.to_string(), p.to_string())).collect();
    covered.sort();
    covered.dedup();
    assert_eq!(covered, wanted, "every frame method's table parameter is called here (CALLS)");

    let literals: Vec<&str> = CALLS.iter().map(|c| c.3).collect();
    let list = format!("{{ {} }}", literals.join(", "));
    let mut draw = String::from("function on_draw(frame)\n");
    for (n, source) in SOURCES.iter().enumerate() {
        draw += &format!("  {} world.tick == {} then\n", if n == 0 { "if" } else { "elseif" }, n + 1);
        for (i, (_, _, call, literal)) in CALLS.iter().enumerate() {
            let table = if source.is_empty() { literal.to_string() } else { source.replace("{}", &(i + 1).to_string()) };
            draw += &format!("    {}\n", call.replace('T', &table));
        }
    }
    draw += "  end\nend\n";
    let main = format!(
        "GIVEN = {list}\n\
         function on_level_start()\n\
           level:fill(0, 0, 0, 63, 63, 0, \"#808080\")\n\
           level.state.r = level:spawn(\"miner\", 32, 32)\n\
           level.state.given = {list}\n\
           world.state.given = {list}\n\
           player.state.given = {list}\n\
           level.state.r.state.given = {list}\n\
         end\n{draw}"
    );
    let dir = folder("given-frame", &main);
    crate::vox::write_figure(&dir.join("fig.vvx"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO), ("arm", glam::IVec3::new(1, 2, 5), glam::IVec3::new(4, 1, 3))]);
    // A level of a file, for a piece to pose (frame:pose).
    std::fs::write(dir.join("conf.lua"), CONF.replace("c.world.size = { 64, 64, 16 }", "c.world.size = nil")).unwrap();
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("levels/room.toml"), "format = \"voxcade level 1\"\nsize = [64, 64, 16]\n\n[[thing]]\nname = \"rig\"\nmodel = \"fig.vvx\"\nat = [10, 10, 1]\n").unwrap();
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    let mut pictures = Vec::new();
    for source in SOURCES {
        steps(&mut g, 1);
        g.compose(0.0, 1.6, Scene::Play, None);
        assert_eq!(halted(&g), "", "from {source:?}");
        pictures.push(drawn(&g));
    }
    // One command a call, the bar's two parts both.
    assert_eq!(pictures[0].len(), CALLS.len() + 2, "{:#?}", pictures[0]);
    for (source, picture) in SOURCES.iter().zip(&pictures).skip(1) {
        for (i, (want, got)) in pictures[0].iter().zip(picture).enumerate() {
            assert_eq!(got, want, "command {i} from {source} as from the literal");
        }
        assert_eq!(picture.len(), pictures[0].len(), "from {source}");
    }
}

/// `music:play`'s table and the channels in it, read from a global after
/// the start (a read-only view, whole or the channels alone), lay what the literal
/// lays: the same digest, the same mistake for a wrong one.
#[test]
fn music_play_reads_a_view_as_its_literal() {
    let laid = |how: &str| {
        let dir = folder("given-music", &format!("WHOLE = {{ from = 'now', channels = {{ K = '@k c2 c2', H = 'r1' }} }}\nCHANNELS = {{ K = '@k c2 c2', H = 'r1' }}\nBAD = {{ K = 'c4' }}\nfunction on_tick()\n  if world.tick == 1 then {how} end\nend\n"));
        std::fs::create_dir_all(dir.join("music")).unwrap();
        std::fs::write(dir.join("music/drums.mml"), "#tempo 120\n@k square\n").unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/drums.mml\" }\nend\n").unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 3);
        (halted(&g), g.game.music().and_then(|c| c.laid).map(|l| l.digest))
    };
    let literal = laid("music:play('drums', { from = 'now', channels = { K = '@k c2 c2', H = 'r1' } })");
    assert!(literal.0.is_empty() && literal.1.is_some(), "{literal:?}");
    assert_eq!(laid("music:play('drums', WHOLE)"), literal, "the whole table a view");
    assert_eq!(laid("music:play('drums', { from = 'now', channels = CHANNELS })"), literal, "the channels a view");
    assert!(laid("music:play('drums', { from = 'now', channels = BAD })").0.contains("music:play: channel K lasts 1 beat"), "a view's mistake as the literal's");
}

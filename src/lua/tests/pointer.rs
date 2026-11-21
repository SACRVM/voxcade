//! The pointer, the sticks and typed text (`docs/game-model.md`, "input"):
//! `c.pointer`, `input.pointer` answered from the picture and checked by
//! the tick, `frame:hud` and the named menus and grids, `frame.pointer`,
//! `input.move` and `input.aim`, `input.typed` while a player types; a
//! game without a pointer as before.

use super::*;
use crate::input::{Panel, Pointer, Typed};

/// A game with `conf` lines added to the usual and this `main.lua`.
fn pointing(name: &str, conf: &str, main: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("lua-{name}"));
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.size = {{ 64, 64, 16 }}\n  c.player.spawn = {{ 32, 32 }}\n  c.actions = {{ \"click\", \"next\" }}\n{conf}\nend\n")).unwrap();
    std::fs::write(dir.join("main.lua"), main).unwrap();
    dir
}

const POINTER: &str = "  c.pointer = \"system\"\n  c.keys.click = { \"mouse_left\" }\n  c.keys.next = { \"wheel_down\", \"Tab\" }";

/// A game that notes what its input says each tick and draws a named
/// panel, menu and grid.
const NOTES: &str = r#"
function on_world_start()
  world.state.seen = {}
end
function on_tick()
  local p, m, a = input.pointer, input.move, input.aim
  local t = input.typed
  table.insert(world.state.seen, string.format("%s|%s|%s|%.3f,%.3f|%.3f,%.3f|%s|%s",
    p and (p.hud or "-") or "nil", p and tostring(p.index) or "", p and p.u and string.format("%.2f,%.2f", p.u, p.v) or "",
    m.x, m.y, a.x, a.y, input:pressed("click") and "click" or "", t and (t.text .. "/" .. t.erase .. (t.done and "/done" or "")) or ""))
  if world.tick == 3 then player.state.typing = true end
end
function on_draw(frame)
  frame:hud("map", 0, 0, 40, 20)
  frame:menu{ name = "shop", x = 100, y = 30, rows = { "a", "b", "c" } }
  frame:grid{ name = "bag", x = 200, y = 100, cells = { { icon = nil }, false, false } }
  frame:hud("over", 0, 0, 10, 10)
end
"#;

fn seen(g: &mut Session<LuaGame>) -> Vec<String> {
    let said = g.game.eval("return table.concat(world.state.seen, '#')").unwrap();
    said.trim_matches('"').split('#').map(str::to_string).collect()
}

/// A pointer is answered from the picture drawn last: the topmost panel
/// (a later one over an earlier), a menu's row and a grid's cell by index
/// from 1, where in it (u, v); over nothing hud is nil but the pointer is
/// there. The tick takes an answer of this level's picture only.
#[test]
fn the_pointer_is_answered_from_the_picture_and_checked_by_the_tick() {
    let dir = pointing("pointer-answers", POINTER, NOTES);
    let mut g = game_in(&dir);
    g.look();
    let at = |g: &Session<LuaGame>, x: f32, y: f32| g.point(x, y, 16.0 / 9.0);
    assert_eq!(at(&g, 5.0, 5.0).panel.map(|p| p.name), Some("over".into()), "the later lies on top");
    assert_eq!(at(&g, 20.0, 10.0).panel, Some(Panel { name: "map".into(), index: 0, u: 500, v: 500 }));
    let shop = crate::ui::menu_place(&menu_of(&g), &crate::ui::Layout::of(&g.cfg.ui, 16.0 / 9.0), &Default::default());
    let (_, rx, ry, rw, rh) = shop.parts[1];
    let row = at(&g, rx + rw / 2.0, ry + rh / 2.0).panel.unwrap();
    assert_eq!((row.name.as_str(), row.index), ("shop", 2), "the second row");
    let cell = at(&g, 200.0 + 6.0 + 20.0 + 5.0, 100.0 + 6.0 + 5.0).panel.unwrap();
    assert_eq!((cell.name.as_str(), cell.index), ("bag", 2), "the second cell");
    assert_eq!(at(&g, 300.0, 5.0).panel, None);
    let input = Input { pointer: Some(Box::new(at(&g, 20.0, 10.0))), ..Input::default() };
    g.step(&input);
    // From another level's picture: nil whole.
    let mut old = input.clone();
    old.pointer.as_mut().unwrap().picture = Some(old.pointer.as_ref().unwrap().picture.unwrap().wrapping_add(1));
    g.step(&old);
    // A name no panel can have: the panel dropped, the pointer kept.
    let mut odd = input.clone();
    odd.pointer.as_mut().unwrap().panel.as_mut().unwrap().name = "no such!".into();
    g.step(&odd);
    let s = seen(&mut g);
    assert_eq!(s[0], "map|nil|0.50,0.50|0.000,0.000|0.000,0.000||");
    assert_eq!(s[1], "nil|||0.000,0.000|0.000,0.000||");
    assert_eq!(s[2], "-|nil||0.000,0.000|0.000,0.000||");
}

/// `input.pointer.x` and `y` are where the pointer is as its machine gave
/// it, the HUD's measure (what `frame.pointer` says of the tick played);
/// `moved` is true where it is elsewhere than the tick before or was not
/// there, false where it stayed, whatever the tick takes of its answers.
/// Off the picture it is nil, and back on it it moved. A run of it replays
/// and goes back as recorded.
#[test]
fn the_pointer_says_where_it_is_and_whether_it_moved() {
    let main = r#"
function on_world_start()
  world.state.seen = {}
end
function on_tick()
  local p = input.pointer
  table.insert(world.state.seen, p and string.format("%d,%d,%s,%s", p.x, p.y, tostring(p.moved), p.hud or "-") or "nil")
end
function on_draw(frame)
  frame:hud("map", 0, 0, 40, 20)
  local p = frame.pointer
  frame:text(p and (p.x .. "," .. p.y) or "nil", 1, 1)
end
"#;
    let dir = pointing("pointer-moved", POINTER, main);
    let mut g = game_in(&dir);
    g.look();
    let at = |g: &Session<LuaGame>, x: f32, y: f32| Input { pointer: Some(Box::new(g.point(x, y, 16.0 / 9.0))), ..Input::default() };
    let map = at(&g, 20.0, 10.0);
    let mut old = map.clone();
    old.pointer.as_mut().unwrap().picture = Some(map.pointer.as_ref().unwrap().picture.unwrap().wrapping_add(1));
    for input in [&map, &map, &old, &map, &at(&g, 70.4, 39.6), &Input::default(), &at(&g, 70.0, 40.0)] {
        g.step(input);
    }
    assert_eq!(seen(&mut g), ["20,10,true,map", "20,10,false,map", "nil", "20,10,false,map", "70,40,true,-", "nil", "70,40,true,-"]);
    g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(first_text(&g), "70,40", "frame.pointer says the same place");
    // Recorded from a script's pointer=X,Y, played again, gone back.
    let dir = pointing("pointer-moved-replay", POINTER, main);
    let mut g = game_in(&dir);
    let mut script = crate::replay::Script::new(g.script("none:2;pointer=20,10:3;pointer=21,10:1;none:2;pointer=21,10:2;pointer=300,150:40;none:20").unwrap());
    let mut recorder = crate::replay::Recorder::default();
    for _ in 0..70 {
        let mut input = script.next_input();
        g.answer(&mut input, 1, 16.0 / 9.0);
        g.step(&input);
        g.look();
        recorder.record(&g);
    }
    assert_eq!(seen(&mut g)[..10], ["nil", "nil", "20,10,true,map", "20,10,false,map", "20,10,false,map", "21,10,true,map", "nil", "nil", "21,10,true,map", "21,10,false,map"]);
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/moved.toml")).unwrap();
    assert_eq!(crate::replay::play_through::<LuaGame>(&dir, &crate::replay::Replay::load(&dir.join("replays/moved.toml")).unwrap()).unwrap(), crate::replay::Verdict::Same);
    assert!(crate::replay::check_going_back::<LuaGame>(&dir, 3).unwrap());
}

fn menu_of(g: &Session<LuaGame>) -> crate::lua::world::Menu {
    g.game.drawn.borrow().iter().find_map(|c| match c { crate::lua::world::Cmd::Menu(m) => Some(m.clone()), _ => None }).expect("a menu")
}

/// `input.move` is the left stick, else the directions (a diagonal about
/// 0.71 each); `input.aim` the right stick; neither longer than 1 however
/// the bytes come; the text typed reaches the game only while its player
/// types.
#[test]
fn sticks_are_values_and_text_comes_while_typing() {
    let dir = pointing("sticks", POINTER, NOTES);
    let mut g = game_in(&dir);
    let typed = |text: &str| Some(Box::new(Typed::text(text)));
    g.step(&Input { right: true, up: true, typed: typed("a"), ..Input::default() });
    g.step(&Input { moves: [64, -127], aim: [127, 127], ..Input::default() });
    g.step(&Input { moves: [-128, 0], ..Input::default() });
    g.step(&Input { typed: typed("Hi"), ..Input::default() });
    g.step(&Input { typed: Some(Box::new(Typed { erase: 1, done: true, ..Typed::default() })), ..Input::default() });
    let s = seen(&mut g);
    assert_eq!(s[0], "nil|||0.707,0.707|0.000,0.000||", "not typing yet: no text");
    let aim: Vec<f64> = s[1].split('|').nth(4).unwrap().split(',').map(|v| v.parse().unwrap()).collect();
    assert!(s[1].starts_with("nil|||0.449,-0.890|") && (aim[0] * aim[0] + aim[1] * aim[1]) <= 1.0 && aim[0] == aim[1], "shortened along its way: {}", s[1]);
    assert_eq!(s[2], "nil|||-1.000,0.000|0.000,0.000||");
    assert_eq!(s[3], "nil|||0.000,0.000|0.000,0.000||Hi/0", "typing from tick 3 on");
    assert_eq!(s[4], "nil|||0.000,0.000|0.000,0.000||/1/done");
}

/// A game without a pointer is as before: it hears no pointer whatever
/// the input carries, a script's `pointer=` and the mouse's keys are
/// mistakes naming c.pointer, and the pointer adds to the fingerprint only
/// where it is set.
#[test]
fn a_game_without_a_pointer_hears_none_and_counts_as_before() {
    let dir = pointing("no-pointer", "", NOTES);
    let mut g = game_in(&dir);
    assert!(!g.has_pointer());
    let input = Input { pointer: Some(Box::new(Pointer { x: 5, y: 5, picture: Some(crate::input::picture_mark(g.game.level_mark())), ..Pointer::default() })), ..Input::default() };
    g.step(&input);
    assert!(seen(&mut g)[0].starts_with("nil|"), "no pointer");
    let e = format!("{:#}", g.script("pointer=5,5:1").unwrap_err());
    assert!(e.contains("c.pointer"), "{e}");
    let e = format!("{:#}", g.script("mouse_left:1").unwrap_err());
    assert!(e.contains("mouse_left: the game has no pointer"), "{e}");
    let unset = g.fingerprint().unwrap();
    let nil = pointing("pointer-nil", "  c.pointer = nil", NOTES);
    assert_eq!(game_in(&nil).fingerprint().unwrap(), unset, "unset adds nothing");
    let set = pointing("pointer-set", POINTER, NOTES);
    assert_ne!(game_in(&set).fingerprint().unwrap(), unset);
    // Keys alone hash as before the sticks, pointer and text came.
    assert!(!Input { right: true, actions: 3, ..Input::default() }.has_more());
}

/// What `conf.lua` and the drawing get wrong about the pointer is said:
/// a style it has not, the mouse's keys without a pointer, a panel's name
/// that is none.
#[test]
fn pointer_mistakes_are_named() {
    let bad = pointing("pointer-bad", "  c.pointer = \"mouse\"", "");
    let e = start_error(&bad);
    assert!(e.contains("c.pointer") && e.contains("\"system\"") && e.contains("\"own\""), "{e}");
    let mouse = pointing("pointer-mouse", "  c.keys.click = { \"mouse_left\" }", "");
    let e = start_error(&mouse);
    assert!(e.contains("c.keys.click") && e.contains("mouse_left") && e.contains("c.pointer"), "{e}");
    for (draw, want) in [
        ("frame:hud(\"a b\", 0, 0, 5, 5)", "frame:hud: name = \"a b\""),
        ("frame:hud(\"\", 0, 0, 5, 5)", "frame:hud: a name first"),
        ("frame:hud(\"ok\", 0, 0, -5, 5)", "width and height 0 or more"),
        ("frame:menu{ name = \"much_too_long_a_name\", rows = { \"a\" } }", "at most 16 long"),
        ("frame:grid{ cells = { { model = \"nothing.vvx\" } } }", "frame:grid: cells[1]"),
    ] {
        let dir = pointing("pointer-draw", POINTER, &format!("function on_draw(frame) {draw} end\n"));
        let g = game_in(&dir);
        g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
        assert!(halted(&g).contains(want), "{draw}: {}", halted(&g));
    }
}

/// A run with a pointer (answered from the picture as `shot` answers a
/// script's `pointer=X,Y`), a click, the wheel's key, sticks and text
/// plays back as recorded, and going back every few ticks, as rollback
/// would, comes out the same.
#[test]
fn a_run_with_pointer_sticks_and_text_replays_and_goes_back() {
    let dir = pointing("pointer-replay", POINTER, NOTES);
    let mut g = game_in(&dir);
    let script = g.script("none:2;pointer=20,10:2;pointer=20,10+mouse_left:1;pointer=105,40+move=0.5,0.5:3;text=Nick%3B+aim=-1,0:1;erase=2:1;typed=done:1;wheel_down:1;none:100").unwrap();
    let mut script = crate::replay::Script::new(script);
    let mut recorder = crate::replay::Recorder::default();
    for _ in 0..140 {
        let mut input = script.next_input();
        g.answer(&mut input, 1, 16.0 / 9.0);
        g.step(&input);
        g.look();
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    let s = seen(&mut g);
    assert_eq!(s[2], "map|nil|0.50,0.50|0.000,0.000|0.000,0.000||");
    assert_eq!(s[4], "map|nil|0.50,0.50|0.000,0.000|0.000,0.000|click|/0", "typing from tick 3: nothing typed yet");
    assert!(s[5].starts_with("shop|1|") && s[5].contains("|0.504,0.504|"), "{}", s[5]);
    assert_eq!(s[8], "nil|||0.000,0.000|-1.000,0.000||Nick;/0", "a step without the pointer has none");
    assert!(s[9].ends_with("|/2") && s[10].ends_with("|/0/done"), "{} {}", s[9], s[10]);
    let replay = recorder.replay(&g).unwrap();
    assert!(replay.input.contains("text=Nick%3B") && replay.input.contains("hud=shop+index=1"), "{}", replay.input);
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/pointer.toml")).unwrap();
    assert_eq!(crate::replay::play_through::<LuaGame>(&dir, &crate::replay::Replay::load(&dir.join("replays/pointer.toml")).unwrap()).unwrap(), crate::replay::Verdict::Same);
    assert!(crate::replay::check_going_back::<LuaGame>(&dir, 3).unwrap());
}

/// Two seats at one machine, the first typing: its text reaches it alone;
/// the second, not typing, hears none of what its input carries, and both
/// keep their keys.
#[test]
fn of_two_seats_one_types() {
    let main = r#"
function on_world_start()
  world.state.got = {}
end
function on_tick()
  player.state.typing = true
  for _, p in ipairs(level:players()) do
    local t = p.input.typed
    table.insert(world.state.got, p.seat .. "=" .. (t and t.text or "nil") .. (p.input:held("click") and "+click" or ""))
  end
end
"#;
    let dir = pointing("pointer-seats", &format!("{POINTER}\n  c.players = 2\n  c.things.hero = {{}}\n  c.player.kind = \"hero\""), main);
    let mut g = game_in(&dir);
    g.step(&Input::default());
    g.set_seat(2, Some(Input { actions: 1, typed: Some(Box::new(Typed::text("no"))), ..Input::default() }));
    g.step(&Input { typed: Some(Box::new(Typed::text("yes"))), ..Input::default() });
    g.step(&Input { typed: Some(Box::new(Typed::text("yes"))), ..Input::default() });
    let got = g.game.eval("return table.concat(world.state.got, ' ')").unwrap();
    assert!(got.ends_with("1=yes 2=nil+click\""), "{got}");
    assert!(g.typing(1) && !g.typing(2));
}

/// A figure of parts: an empty root, a body, a head on it.
const DOLL: &str = "format = \"voxcade voxel 1\"\nsize = [5, 3, 6]\n\n[palette]\na = \"#5a7aa8\"\nb = \"#d0c8b8\"\n\n[[part]]\nname = \"rig\"\n\n[[part]]\nname = \"body\"\nat = [-2, -1, 0]\nparent = \"rig\"\nlayers = \"\"\"\naaaaa\naaaaa\naaaaa\n*4\n\"\"\"\n\n[[part]]\nname = \"head\"\nat = [-1, -1, 4]\nparent = \"body\"\nlayers = \"\"\"\nbbb\nbbb\nbbb\n*2\n\"\"\"\n";

/// A room for the pointer: a floor (a solid piece), a stove (a piece), an
/// area over the floor, a second level; a crate and a doll spawned and
/// drawn by `frame:model`, the doll's head posed. What `input.pointer`
/// says each tick is noted.
fn pointing_room(name: &str) -> crate::scratch::Scratch {
    let level = "format = \"voxcade level 1\"\nsize = [64, 64, 40]\n\n[[thing]]\nname = \"floor\"\nmodel = \"slab.vvx\"\nat = [32, 32, 0]\nsolid = true\n\n[[thing]]\nname = \"stove\"\nmodel = \"box.vvx\"\nat = [24, 40, 4]\n\n[[place]]\nname = \"start\"\nat = [24, 24, 4]\n\n[[area]]\nname = \"door\"\nat = [40, 26, 4]\nsize = [4, 4, 12]\n";
    let main = r#"
function on_world_start()
  world.state.seen = {}
end
function on_level_start()
  if level.name == "room" then
    level:spawn("crate", 32, 30, { z = 4 })
    level:spawn("doll", 36, 36, { z = 4 })
  end
end
function on_tick()
  local p = input.pointer
  if not p then table.insert(world.state.seen, "none") return end
  table.insert(world.state.seen, string.format("%s|%s|%s|%s|%s", p.thing and p.thing.kind or "nil", p.part or "nil", p.area or "nil",
    p.at and string.format("%g,%g,%g", p.at.x, p.at.y, p.at.z) or "nil", p.cell and (p.cell.x .. "," .. p.cell.y .. "," .. p.cell.z) or "nil"))
end
function on_draw(frame)
  local p = frame.pointer
  if p then frame:text((p.thing and p.thing.kind or "-") .. "/" .. (p.part or "-") .. "/" .. (p.area or "-"), 1, 1) end
  for _, t in ipairs(level:things()) do
    if t.kind == "crate" then frame:model("box.vvx", t) end
    if t.kind == "doll" then frame:model("doll.vvx", t, nil, nil, { head = { rotation = 90 } }) end
  end
end
"#;
    let dir = pieces_folder(name, "  c.pointer = \"system\"\n  c.things.crate = {}\n  c.things.doll = {}\n", level, main);
    std::fs::write(dir.join("slab.vvx"), "format = \"voxcade voxel 1\"\nsize = [20, 20, 4]\n\n[palette]\na = { color = \"#806040\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaa *20\n*4\n\"\"\"\n").unwrap();
    std::fs::write(dir.join("doll.vvx"), DOLL).unwrap();
    std::fs::write(dir.join("levels/zz_other.toml"), "format = \"voxcade level 1\"\nsize = [64, 64, 40]\n").unwrap();
    dir
}

/// The level's answers: the pointer meets a thing's drawn voxels (a
/// model `frame:model` drew with it, a level's piece), a posed figure's
/// part by name, the level's area it enters before the floor, the point
/// and the voxel where it meets the floor (a solid piece). A thing gone
/// between the picture and the tick is nil; after a level change the
/// answer of the old level's picture is nil whole.
#[test]
fn the_pointer_meets_things_parts_areas_and_the_level() {
    let dir = pointing_room("pointer-level");
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    g.look();
    let aspect = 16.0 / 9.0;
    let at = |g: &Session<LuaGame>, p: glam::Vec3| {
        let [x, y] = g.to_hud(p, aspect).expect("in view");
        Input { pointer: Some(Box::new(g.point(x, y, aspect))), ..Input::default() }
    };
    // The crate (3 by 3 by 4 at 32, 30, its top at 8), the stove, the
    // doll's head (its top at 10), the area over the floor, the floor.
    let crate_top = at(&g, glam::Vec3::new(32.5, 30.5, 7.9));
    let stove = at(&g, glam::Vec3::new(24.5, 40.5, 7.9));
    let head = at(&g, glam::Vec3::new(36.5, 36.5, 9.9));
    let door = at(&g, glam::Vec3::new(42.0, 28.0, 4.0));
    let floor = at(&g, glam::Vec3::new(28.0, 24.0, 4.0));
    for input in [&crate_top, &stove, &head, &door, &floor] {
        g.step(input);
    }
    let s = seen(&mut g);
    assert_eq!(s[0].split('|').take(3).collect::<Vec<_>>(), ["crate", "nil", "nil"], "{}", s[0]);
    assert!(s[1].starts_with("piece|nil|nil|"), "{}", s[1]);
    assert!(s[2].starts_with("doll|head|nil|"), "{}", s[2]);
    // frame.pointer: the same of the picture before, the last pointer played.
    g.compose(0.0, aspect, Scene::Play, None);
    assert_eq!(first_text(&g), "piece/-/-", "over the floor");
    // (The floor is a level's piece too: a thing drawn there.)
    assert_eq!(s[3], "piece|nil|door|42,28,4|42,28,3");
    assert_eq!(s[4], "piece|nil|nil|28,24,4|28,24,3");
    // Gone between the picture and the tick: nil.
    said(&mut g, "for _, t in ipairs(level:things()) do if t.kind == 'crate' then t:remove() end end");
    g.step(&Input::default());
    g.step(&crate_top);
    let s = seen(&mut g);
    assert!(s[6].starts_with("nil|nil|nil|"), "the crate gone: {}", s[6]);
    // Another level's picture: nil whole.
    said(&mut g, "world:go_to('zz_other')");
    g.step(&Input::default());
    g.step(&floor);
    let s = seen(&mut g);
    assert_eq!(s[8], "none", "{s:?}");
}

/// A run of the room with the level's answers plays back as recorded and
/// survives going back every few ticks; a replay writes them as words.
#[test]
fn a_run_with_the_levels_answers_replays_and_goes_back() {
    let dir = pointing_room("pointer-level-replay");
    let mut g = game_in(&dir);
    g.look();
    let mut recorder = crate::replay::Recorder::default();
    for tick in 0..130 {
        let p = glam::Vec3::new(26.0 + (tick % 20) as f32, 30.0, 6.0);
        let [x, y] = g.to_hud(p, 16.0 / 9.0).unwrap();
        let input = Input { pointer: Some(Box::new(g.point(x, y, 16.0 / 9.0))), ..Input::default() };
        g.step(&input);
        g.look();
        recorder.record(&g);
    }
    let replay = recorder.replay(&g).unwrap();
    assert!(replay.input.contains("thing=") && replay.input.contains("at="), "{}", &replay.input[..300.min(replay.input.len())]);
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/level.toml")).unwrap();
    assert_eq!(crate::replay::play_through::<LuaGame>(&dir, &crate::replay::Replay::load(&dir.join("replays/level.toml")).unwrap()).unwrap(), crate::replay::Verdict::Same);
    assert!(crate::replay::check_going_back::<LuaGame>(&dir, 4).unwrap());
}

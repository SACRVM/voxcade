//! A kind's way (`c.things.NAME.facing`): its things come in facing it in
//! every game, without a kit and with either, set down by a level's file
//! or spawned; a thing's own way beats it, and without either a thing
//! rests facing the camera (`Sim::rest_way`, `Sim::body_facing`).

use super::*;

/// A room of things of kinds: a crate (no way), a door (its kind faces
/// east), a door the file turns west; the start; `{camera}` in its place.
const ROOM: &str = r#"format = "voxcade level 1"
size = [64, 48, 40]
{camera}
[[thing]]
kind = "crate"
at = [10, 20, 0]

[[thing]]
kind = "door"
at = [20, 20, 0]

[[thing]]
kind = "door"
name = "west"
at = [30, 20, 0]
rotation = 270

[[thing]]
kind = "gate"
at = [40, 20, 0]

[[place]]
name = "start"
at = [32, 10, 0]
"#;

/// Things spawned as the level starts: a door, a door facing north of its
/// own, a crate.
const SPAWNS: &str = r#"
function on_level_start(how)
  level.state.door = level:spawn("door", 50, 20)
  level.state.north = level:spawn("door", 55, 20, { facing = { x = 0, y = 1 } })
  level.state.crate = level:spawn("crate", 45, 30)
end
"#;

/// The ways of the room's things and the spawned ones, x and y each, in
/// the order crate, door, west, gate, spawned door, north, spawned crate.
fn ways(g: &mut Session<LuaGame>) -> String {
    let things = ["level:thing('crate_1')", "level:thing('door_1')", "level:thing('west')", "level:thing('gate_1')", "level.state.door", "level.state.north", "level.state.crate"];
    let all: Vec<String> = things.iter().map(|t| said(g, &format!("return {t}.facing.x .. ',' .. {t}.facing.y"))).collect();
    all.join(" ")
}

/// Without a kit: the kind's way for what the file sets down and what is
/// spawned, a `like` too; the file's and `level:spawn`'s own way win; a
/// kind without one rests facing the camera, south, and north in a level
/// seen from behind, where the kinds' ways stay the world's.
#[test]
fn a_kinds_facing_holds_without_a_kit() {
    let conf = "  c.things.crate = {}\n  c.things.door = { facing = { 1, 0 } }\n  c.things.gate = { like = \"door\" }\n";
    let dir = pieces_folder("kind-facing", conf, &ROOM.replace("{camera}", ""), SPAWNS);
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    assert_eq!(ways(&mut g), "\"0,-1\" \"1,0\" \"-1,0\" \"1,0\" \"1,0\" \"0,1\" \"0,-1\"");
    steps(&mut g, 3);
    assert_eq!(ways(&mut g), "\"0,-1\" \"1,0\" \"-1,0\" \"1,0\" \"1,0\" \"0,1\" \"0,-1\"", "they stay so");

    let dir = pieces_folder("kind-facing-behind", conf, &ROOM.replace("{camera}", "\n[camera]\nyaw = 180\n"), SPAWNS);
    let mut g = game_in(&dir);
    assert_eq!(ways(&mut g), "\"0,1\" \"1,0\" \"-1,0\" \"1,0\" \"1,0\" \"0,1\" \"0,1\"", "seen from behind only the crates turn to the camera");
}

/// A kind's way that is no way is a mistake as the game loads, naming it.
#[test]
fn a_kinds_facing_that_is_no_way_is_a_mistake() {
    for (facing, why) in [("{ 0, 0 }", ""), ("\"east\"", ", not \"east\""), ("{ 1, 0, 0 }", "")] {
        let conf = format!("  c.things.crate = {{}}\n  c.things.door = {{ facing = {facing} }}\n  c.things.gate = {{}}\n");
        let e = start_error(&pieces_folder("kind-facing-wrong", &conf, &ROOM.replace("{camera}", ""), SPAWNS));
        assert!(e.contains(&format!("c.things.door.facing: a way, {{ x, y }} (x east, y north: {{ 1, 0 }} faces east, {{ 0, -1 }} south){why}")), "{facing}: {e}");
    }
}

/// With the shooter kit: a kind without a behavior and a soldier come in
/// facing their kinds' ways (the soldier, far from the player and
/// asleep, keeps it until he wakes); a kind without one faces south.
#[test]
fn a_kinds_facing_holds_with_the_shooter_kit() {
    let conf = "  c.things.door = { facing = { 1, 0 } }\n  c.things.post = {}\n  c.things.guard = { behavior = \"soldier\", weapon = \"enemy_rifle\", facing = { -1, 0 } }\n";
    let main = r#"
function on_level_start(how)
  level.state.door = level:spawn("door", player.x + 10, player.y)
  level.state.north = level:spawn("door", player.x + 20, player.y, { facing = { x = 0, y = 1 } })
  level.state.post = level:spawn("post", player.x - 10, player.y)
  level.state.guard = level:spawn("guard", player.x - 200, player.y)
end
"#;
    let mut g = game_in(&kit_folder("kind-facing", conf, main));
    steps(&mut g, 10);
    assert_eq!(halted(&g), "");
    let way = |g: &mut Session<LuaGame>, t: &str| said(g, &format!("return level.state.{t}.facing.x .. ',' .. level.state.{t}.facing.y"));
    assert_eq!([way(&mut g, "door"), way(&mut g, "north"), way(&mut g, "post"), way(&mut g, "guard")], ["\"1,0\"", "\"0,1\"", "\"0,-1\"", "\"-1,0\""]);
}

/// With the platformer kit (which read a kind's way before): a kind
/// without a behavior faces it set down by the level's file and spawned
/// in code alike (spawned, it came in facing the camera before).
#[test]
fn a_kinds_facing_holds_with_the_platformer_kit() {
    let main = "function on_level_start(how)\n  level.state.sign = level:spawn(\"sign\", 10, 2, { z = 4 })\nend\n";
    let conf = "  c.things.sign = { facing = { 1, 0 }, fixed = true }\n";
    let mut g = game_in(&platformer_levels("kind-facing", main, conf, &[("sign", [8, 2, 4])]));
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return level:thing('sign_1').facing.x, level.state.sign.facing.x"), "1\t1");
}

/// The picture draws a thing of a kind facing east as its model facing
/// east: the first row's first voxel ahead and to the left of the
/// pivot's cell, the first row's middle ahead (x east, y north; as
/// `a_models_rows_and_columns_lie_as_its_facing_says` has it).
#[test]
fn a_thing_of_a_kind_with_a_way_is_drawn_facing_it() {
    let model = "format = \"voxcade voxel 1\"\nsize = [3, 3, 1]\n\n[palette]\nk = \"#ff0000\"\na = \"#00ff00\"\nc = \"#808080\"\n\n[[frame]]\nlayers = \"\"\"\nkac\nccc\nccc\n\"\"\"\n";
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 79, 39, 0, "#404040")
  level.state.mark = level:spawn("mark", 20, 25, { z = 1 })
end

function on_draw(frame)
  frame:camera({ look_at = { 40, 20, 0 }, width = 120 })
  frame:model("mark.vvx", level.state.mark)
end
"##;
    let dir = folder("kind-facing-picture", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 80, 40, 16 }\n  c.player.spawn = { 40, 30 }\n  c.things.mark = { model = \"mark.vvx\", fixed = true, facing = { 1, 0 } }\nend\n").unwrap();
    std::fs::write(dir.join("mark.vvx"), model).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let frame = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(halted(&g), "");
    let cells = |color: [u8; 4]| -> Vec<glam::IVec3> { frame.cubes().iter().filter(|i| i.color == color).map(|i| glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3()).collect() };
    assert_eq!(cells([255, 0, 0, 255]), [glam::IVec3::new(21, 26, 1)], "the first row's first voxel");
    assert_eq!(cells([0, 255, 0, 255]), [glam::IVec3::new(21, 25, 1)], "the first row's middle");
}

/// A level of a free game: a piece at 30 and one at 90, a wisp (a kind that
/// turns freely) at 200 and a crate (a kind on the quarters) at 100.
const FREE_ROOM: &str = r#"format = "voxcade level 1"
size = [64, 48, 40]

[[thing]]
name = "slant"
model = "box.vvx"
at = [10, 20, 0]
rotation = 30

[[thing]]
name = "east"
model = "box.vvx"
at = [20, 20, 0]
rotation = 90

[[thing]]
name = "wisp"
kind = "wisp"
at = [30, 20, 0]
rotation = 200

[[thing]]
name = "crate"
kind = "crate"
at = [40, 20, 0]
rotation = 100

[[place]]
name = "start"
at = [32, 10, 0]
"#;

/// `thing.rotation` is the bearing exactly, degrees clockwise from north
/// below 360, whole on the quarters; `thing.facing` stays the nearest of
/// the four ways. A thing a script turns reads it too, in a tick and in
/// `on_draw`; a game on the grid reads its quarter; it is not to be set.
#[test]
fn a_things_rotation_is_its_exact_bearing() {
    let main = r#"
function on_level_start(how)
  level.state.diagonal = level:spawn("wisp", 50, 20, { facing = { x = 1, y = 1 } })
  level.state.west = level:spawn("wisp", 55, 20, { facing = { x = -1, y = 0 } })
  level.state.crate = level:spawn("crate", 45, 30, { facing = { x = 0, y = 1 } })
end

function on_tick()
  if math.abs(level:thing("slant").rotation - 30) > 0.001 then error("tick") end
  local ok = pcall(function() level:thing("slant").rotation = 5 end)
  if ok then error("set") end
end

function on_draw(frame)
  if math.abs(level:thing("slant").rotation - 30) > 0.001 then error("draw") end
end
"#;
    let conf = "  c.camera.rotates = \"free\"\n  c.things.wisp = { rotates = \"free\" }\n  c.things.crate = {}\n";
    let mut g = game_in(&pieces_folder("rotation", conf, FREE_ROOM, main));
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    let rotation = |g: &mut Session<LuaGame>, t: &str| said(g, &format!("return {t}.rotation")).parse::<f32>().unwrap();
    let near = |a: f32, b: f32| assert!((a - b).abs() < 1e-3, "{a} is not {b}");
    near(rotation(&mut g, "level:thing('slant')"), 30.0);
    assert_eq!(said(&mut g, "level:thing('slant').facing.y"), "1", "facing stays its quarter, north");
    assert_eq!(rotation(&mut g, "level:thing('east')"), 90.0, "whole on a quarter");
    near(rotation(&mut g, "level:thing('wisp')"), 200.0);
    assert_eq!(said(&mut g, "level:thing('wisp').facing.y"), "-1");
    assert_eq!(rotation(&mut g, "level:thing('crate')"), 90.0, "100 on a kind of the quarters: east");
    near(rotation(&mut g, "level.state.diagonal"), 45.0);
    assert_eq!(rotation(&mut g, "level.state.west"), 270.0, "never below 0 or 360");
    assert_eq!(rotation(&mut g, "level.state.crate"), 0.0);
    let _ = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(halted(&g), "", "read alike in a tick and in on_draw, not to set there");
    for line in ["level:thing('slant').rotation = 5", "player.rotation = 5"] {
        let e = said(&mut g, line);
        assert!(e.contains("rotation cannot be set: a thing's fields are only to read"), "{line}: {e}");
    }
    // A game on the grid reads its quarter.
    let grid = FREE_ROOM.replace("rotation = 30", "rotation = 90").replace("rotation = 200", "rotation = 180");
    let mut g = game_in(&pieces_folder("rotation-grid", "  c.things.wisp = {}\n  c.things.crate = {}\n", &grid, ""));
    assert_eq!(halted(&g), "");
    assert_eq!([rotation(&mut g, "level:thing('slant')"), rotation(&mut g, "level:thing('wisp')"), rotation(&mut g, "level:thing('crate')")], [90.0, 180.0, 90.0]);
}

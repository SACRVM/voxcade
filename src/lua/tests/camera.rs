//! A level's own camera (`[camera] yaw` in its file): the level seen from
//! its other side, its things at rest facing the camera there.

use super::*;

/// A room seen from the other side: a crate set down without a way, one
/// turned east, the start.
const TURNED: &str = r#"format = "voxcade level 1"
size = [64, 48, 40]

[camera]
yaw = 180

[[thing]]
kind = "crate"
at = [10, 20, 0]

[[thing]]
kind = "crate"
name = "east"
at = [30, 20, 0]
rotation = 90

[[place]]
name = "start"
at = [32, 10, 0]
"#;

/// The game of `TURNED` (its camera as `level` has it) with these lines
/// more in `conf.lua`.
fn turned(name: &str, level: &str, conf: &str) -> crate::scratch::Scratch {
    pieces_folder(name, &format!("  c.things.crate = {{}}\n{conf}"), level, "function on_tick() end\n")
}

/// The level's yaw is the camera's there, `level.camera.yaw` says it, and
/// what nothing turns rests facing the camera (north at 180); a way the
/// file states stays the world's. Without `[camera]` all is as before.
#[test]
fn a_level_turns_the_camera_its_file_says() {
    let dir = turned("yaw", TURNED, "");
    let mut g = game_in(&dir);
    assert_eq!(said(&mut g, "return level.camera.yaw"), "180");
    assert_eq!(g.camera().yaw, 180.0);
    assert_eq!(said(&mut g, "return player.facing.x, player.facing.y"), "0\t1", "the player's figure faces the camera");
    assert_eq!(said(&mut g, "return level:thing('crate_1').facing.y"), "1", "a crate without a way too");
    assert_eq!(said(&mut g, "return level:thing('east').facing.x"), "1", "a way the file states is the world's");
    let turned_faces = g.face_mask();

    let plain = turned("yaw-none", &TURNED.replace("[camera]\nyaw = 180\n\n", ""), "");
    let mut p = game_in(&plain);
    assert_eq!(said(&mut p, "return level.camera.yaw"), "0", "the game's");
    assert_eq!(said(&mut p, "return player.facing.y, level:thing('crate_1').facing.y"), "-1\t-1", "south, as always");
    assert_ne!(p.face_mask(), turned_faces, "other faces from the other side");
    let own = turned("yaw-game", &TURNED.replace("[camera]\nyaw = 180\n\n", ""), "  c.camera.yaw = 180\n");
    assert_eq!(game_in(&own).face_mask(), turned_faces, "the game's yaw sees the same faces");
}

/// A level going to another turned the other way: the camera and its faces
/// follow as it is entered (the meshes are built anew once, the window's
/// `face_mask` compared), back again too.
#[test]
fn the_camera_turns_with_the_level_entered() {
    let dir = turned("yaw-two", TURNED, "");
    std::fs::write(dir.join("levels/second.toml"), TURNED.replace("[camera]\nyaw = 180\n\n", "")).unwrap();
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  if input:pressed('fire') then world:go_to(level.name == 'room' and 'second' or 'room') end\nend\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.things.hero = {}\n  c.player.kind = \"hero\"\n  c.things.crate = {}\n  c.actions = { \"fire\" }\n  c.start = \"room\"\nend\n").unwrap();
    let mut g = game_in(&dir);
    let faces = g.face_mask();
    let fire = Input { actions: 1, ..Input::default() };
    g.step(&fire);
    g.step(&Input::default());
    assert_eq!((said(&mut g, "return level.name, level.camera.yaw"), g.camera().yaw), ("\"second\"\t0".to_string(), 0.0));
    assert_ne!(g.face_mask(), faces);
    g.step(&fire);
    g.step(&Input::default());
    assert_eq!(said(&mut g, "return level.name, level.camera.yaw"), "\"room\"\t180");
    assert_eq!(g.face_mask(), faces, "back: the faces it had");
}

/// A level turns its camera by quarter turns only, and a view from the
/// side (or the jump & run's rules) by halves only so far: each a mistake
/// naming its line; a quarter turn of a view from above loads.
#[test]
fn a_level_turns_by_quarters_and_a_side_view_by_halves() {
    for (yaw, conf, said) in [
        ("45", "", "line 5: yaw = 45: a level turns its camera by quarter turns, 0, 90, 180 or 270"),
        ("90", "  c.camera.pitch = 12\n", "line 5: [camera] yaw = 90: the game sees its levels from the side (c.camera.pitch 12), which turns by halves only so far"),
    ] {
        let dir = turned(&format!("yaw-{yaw}"), &TURNED.replace("yaw = 180", &format!("yaw = {yaw}")), conf);
        let e = start_error(&dir);
        assert!(e.contains("levels/room.toml") && e.contains(said), "{yaw}: {e}");
    }
    let unknown = turned("yaw-word", &TURNED.replace("yaw = 180", "yaw = 180\nroll = 3"), "");
    assert!(start_error(&unknown).contains("roll"), "{}", start_error(&unknown));
    let above = turned("yaw-above", &TURNED.replace("yaw = 180", "yaw = 90"), "  c.camera.pitch = 60\n");
    let mut g = game_in(&above);
    assert_eq!(said(&mut g, "return level.camera.yaw, player.facing.x, player.facing.y"), "90\t-1\t0", "a view from above turned east: its things face west");

    let dir = platformer_levels("yaw-jump", "", "  c.camera.pitch = 60\n", &[]);
    let path = dir.join("levels/a.toml");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    level.camera = Some(crate::grid::LevelCamera { yaw: 90, line: 0 });
    level.save(&path).unwrap();
    assert!(start_error(&dir).contains("yaw = 90: the jump & run's rules (c.kit) walk across the screen"), "{}", start_error(&dir));
}

/// The jump & run seen from behind (`yaw = 180`): right on the keys walks
/// right on the screen, towards the world's west; the keys as the script
/// reads them stay the keys.
#[test]
fn the_jump_and_run_seen_from_behind_walks_the_way_the_keys_say_on_the_screen() {
    let walked = |yaw: Option<i32>| {
        let dir = platformer_levels(&format!("behind-{yaw:?}"), "function on_tick() world.state.right = input.dir.x end\n", "", &[]);
        let path = dir.join("levels/a.toml");
        let mut level = crate::grid::GridLevel::read(&path).unwrap();
        level.size.x = 12;
        level.cells = vec![level.cells[0]; 12];
        level.things[0].at = [24, 2, 4];
        level.camera = yaw.map(|yaw| crate::grid::LevelCamera { yaw, line: 0 });
        level.save(&path).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 30);
        let x = |g: &Session<LuaGame>| g.game.stage.host.player_seen().map(|t| t.body.pos.x).unwrap();
        let from = x(&g);
        for _ in 0..30 {
            g.step(&Input { right: true, ..Input::default() });
        }
        assert_eq!(state(&mut g, "right"), "Int(1)", "the keys as pressed");
        x(&g) - from
    };
    assert!(walked(None) > 10.0, "from the front right is east");
    assert!(walked(Some(180)) < -10.0, "from behind right is west");
}

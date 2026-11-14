//! Levels of pieces: models set down, lights, areas, solid pieces, poses.

use super::*;

/// A piece is a thing of the kind "piece" that stays exactly where the
/// level's file puts it (in the air too), found by its name, facing its
/// way, as tall as its model; a level file saved while playing sets them
/// down again where it says.
#[test]
fn pieces_stay_put_and_are_found_by_name() {
    let dir = pieces_folder("put", "", ROOM, "function on_tick() player:walk(input.dir, 40) end\n");
    let mut g = game_in(&dir);
    steps(&mut g, 60);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:thing('lamp').z == 12 and level:thing('lamp').x == 40.5 and level:thing('lamp').y == 30.5"), "true", "where its pivot stands, in the air");
    assert_eq!(said(&mut g, "level:thing('stove').kind"), "\"piece\"");
    assert_eq!(said(&mut g, "level:thing('stove').facing.x"), "1", "facing east");
    assert_eq!(said(&mut g, "level:thing('moon').name"), "\"moon\"");
    assert_eq!(said(&mut g, "#level:things('piece')"), "3", "every piece, the light alone too");
    assert!(said(&mut g, "level:spawn('piece', 4, 4)").contains("spawn a kind of c.things"));
    let inspect = g.game.inspect();
    let lamp = inspect["things"].as_array().unwrap().iter().find(|t| t["name"] == "lamp").unwrap().clone();
    assert_eq!((lamp["kind"].as_str(), lamp["model"].as_str(), lamp["state"]["light"].as_f64()), (Some("piece"), Some("box.vvx"), Some(1.0)), "{lamp}");
    // Saved anew while playing: the lamp hangs higher.
    std::fs::write(dir.join("levels/room.toml"), ROOM.replace("at = [40, 30, 12]", "at = [40, 30, 20]")).unwrap();
    g.reload(&dir.join("levels/room.toml")).unwrap();
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "level:thing('lamp').z"), "20.0");
}

/// The lights the picture has near a point (within half a voxel).
fn lights_at(frame: &crate::render::FrameData, at: glam::Vec3) -> Vec<[f32; 3]> {
    frame.lights.iter().filter(|l| l.pos.distance(at) < 0.6).map(|l| l.color).collect()
}

/// `thing.state.light` is how bright a piece's light shines (1 at first,
/// 0 off): the picture's light follows it, a number out of 0 to 1 is a
/// mistake, and it is kept by a snapshot and by a save.
#[test]
fn a_pieces_light_follows_its_state() {
    let mut g = game_in(&pieces_folder("light", "", ROOM, ""));
    let frame = |g: &Session<LuaGame>| g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    // The lamp's light 3 voxels over its pivot, the middle of the voxel.
    let at = glam::Vec3::new(40.5, 30.5, 15.5);
    assert_eq!(lights_at(&frame(&g), at), [[1.0, 128.0 / 255.0, 0.0]]);
    assert_eq!(said(&mut g, "level:thing('lamp').state.light"), "1.0");
    let hash = g.game.state_hash();
    said(&mut g, "level:thing('lamp').state.light = 0.5");
    assert_ne!(g.game.state_hash(), hash, "the light is part of the game");
    assert_eq!(lights_at(&frame(&g), at), [[0.5, 64.0 / 255.0, 0.0]]);
    let kept = g.snapshot().unwrap();
    said(&mut g, "level:thing('lamp').state.light = 0");
    assert!(lights_at(&frame(&g), at).is_empty(), "off");
    assert!(said(&mut g, "level:thing('lamp').state.light = 2").contains("0 (off) to 1, not 2"));
    assert!(said(&mut g, "level:thing('lamp').state.light = 'on'").contains("not a string"));
    g.restore(&kept);
    assert_eq!(said(&mut g, "level:thing('lamp').state.light"), "0.5", "a snapshot keeps it");
    said(&mut g, "world:save(1)");
    said(&mut g, "level:thing('lamp').state.light = 1");
    said(&mut g, "world:load(1)");
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:thing('lamp').state.light"), "0.5", "a save keeps it");
    assert_eq!(said(&mut g, "level:thing('stove').state.light"), "nil", "a piece without a light has none");
}

/// An area holds a thing standing in it (where its feet are) or a point,
/// a point without z at any height; it is only to read.
#[test]
fn an_area_holds_a_thing_or_a_point() {
    let mut g = game_in(&pieces_folder("area", "", ROOM, ""));
    for (line, says) in [
        ("level.areas.corner:holds(player)", "false"),
        ("level.areas.corner:holds({ x = 15.9, y = 0, z = 19 })", "true"),
        ("level.areas.corner:holds({ x = 16, y = 0, z = 0 })", "false"),
        ("level.areas.corner:holds({ x = 5, y = 5 })", "true"),
        ("level.areas.corner:holds({ x = 5, y = 5, z = 20 })", "false"),
        ("level.areas.corner:holds(level:thing('moon'))", "false"),
        ("level.areas.corner.size.x + level.areas.corner.x", "16.0"),
        ("level.areas.nowhere", "nil"),
    ] {
        assert_eq!(said(&mut g, line), says, "{line}");
    }
    said(&mut g, "player:place(8, 8)");
    assert_eq!(said(&mut g, "level.areas.corner:holds(player)"), "true");
    assert!(said(&mut g, "level.areas.corner = 1").contains("only to read"), "{}", said(&mut g, "level.areas.corner = 1"));
}

/// A level of blocks holds pieces as well, set down from the grid's low
/// corner like its things; the ready-made rules' games have them too.
#[test]
fn a_level_of_blocks_holds_pieces_too() {
    let dir = platformer_levels("blocks-pieces", "", "", &[]);
    std::fs::write(dir.join("box.vvx"), BOX).unwrap();
    let path = dir.join("levels/a.toml");
    let text = std::fs::read_to_string(&path).unwrap() + "\n[[thing]]\nname = \"sign\"\nmodel = \"box.vvx\"\nat = [9, 2, 10]\nlight = { radius = 20 }\n";
    std::fs::write(&path, text).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:thing('sign').z == 10 and level:thing('sign').kind == 'piece'"), "true", "it stays in the air");
    assert_eq!(said(&mut g, "level:thing('sign').state.light"), "1.0");
    assert_eq!(said(&mut g, "#level:things('hero')"), "0", "the hero's marker is the player still");
}

/// Lights past the picture's 16 (`render::MAX_LIGHTS`) give way: each
/// picture shows the 16 nearest the middle of the view.
#[test]
fn too_many_lights_keep_the_nearest() {
    let mut level = ROOM.to_string();
    for i in 0..20 {
        level += &format!("\n[[thing]]\nat = [{}, 10, 2]\nlight = {{ radius = 10 }}\n", 2 + i * 3);
    }
    let g = game_in(&pieces_folder("many-lights", "", &level, ""));
    let frame = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(frame.lights.len(), crate::render::MAX_LIGHTS);
    let target = glam::Vec3::new(32.5, 10.5, 0.0);
    let far = |x: f32| frame.lights.iter().any(|l| (l.pos.x - x).abs() < 0.6 && l.pos.y == 10.5);
    assert!(far(32.5) && !far(2.5) && !far(59.5), "the nearest kept: {:?}", frame.lights.iter().map(|l| l.pos).collect::<Vec<_>>());
    let _ = target;
}

/// A piece turned off the quarter turns (`rotation = 30`) is the game's
/// choice: in a game on the grid the level does not load, and the mistake
/// names the things and both ways out; with `c.camera.rotates = "free"` it
/// faces its bearing. A quarter turn written as degrees is on the grid.
#[test]
fn free_turns_are_the_games_choice() {
    let turned = ROOM.replace("facing = \"east\"", "rotation = 30");
    let e = start_error(&pieces_folder("turned", "", &turned, ""));
    assert!(e.contains("'stove' (line 5)") && e.contains("sets things on the grid") && e.contains("c.camera.rotates = \"free\"") && e.contains("editor"), "{e}");
    let mut g = game_in(&pieces_folder("turned-free", "  c.camera.rotates = \"free\"\n", &turned, ""));
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:thing('stove').facing.y"), "1", "its nearest way, north");
    let quarter = ROOM.replace("facing = \"east\"", "rotation = 90");
    let mut g = game_in(&pieces_folder("turned-quarter", "", &quarter, ""));
    assert_eq!(said(&mut g, "level:thing('stove').facing.x"), "1", "90 is east, on the grid");
}

/// A piece's model is the game's file: one missing stops the game as it
/// loads, naming the level's file and the line; a game's own kind may not
/// be called piece.
#[test]
fn a_pieces_model_is_checked_at_load() {
    let e = start_error(&pieces_folder("no-model", "", &ROOM.replace("model = \"box.vvx\"\nat = [40", "model = \"lamp.vvx\"\nat = [40"), ""));
    assert!(e.contains("room.toml") && e.contains("line 11") && e.contains("lamp.vvx"), "{e}");
    let e = start_error(&pieces_folder("own-piece", "  c.things.piece = {}\n", ROOM, ""));
    assert!(e.contains("c.things.piece") && e.contains("engine's own kind"), "{e}");
}

/// A game that begins in a room of pieces has a scenario there (the lamp
/// off, the player by the corner), and a run begun from it, the lamp
/// switched on and off with a key, plays back the same.
#[test]
fn a_run_in_a_room_of_pieces_plays_back_the_same() {
    let main = "function on_tick()\n  player:walk(input.dir, 40)\n  local lamp = level:thing('lamp')\n  if input:pressed('jump') then lamp.state.light = lamp.state.light > 0 and 0 or 0.75 end\n  if level.areas.corner:holds(player) then world.state.cornered = true end\nend\n";
    let dir = pieces_folder("replay", "  c.actions = { \"jump\" }\n", ROOM, main);
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/lamp_off.lua"), "return {\n  about = \"the lamp off, the player by the corner\",\n  setup = function()\n    level:thing('lamp').state.light = 0\n    player:place(20, 20)\n  end,\n}\n").unwrap();
    let mut g = game_in(&dir);
    g.start_scenario("lamp_off").unwrap();
    let mut recorder = crate::replay::Recorder::default();
    for tick in 0..240 {
        let mut input = Input { left: (20..60).contains(&tick), down: (20..50).contains(&tick), ..Input::default() };
        input.hold(0, tick == 100 || tick == 160 || tick == 200);
        g.step(&input);
        g.look();
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "cornered"), format!("{:?}", crate::lua::Value::Bool(true)), "the player walked into the area");
    assert_eq!(said(&mut g, "level:thing('lamp').state.light"), "0.75");
    let replay = recorder.replay(&g).unwrap();
    assert!(replay.scenario.as_deref() == Some("lamp_off"));
    assert!(matches!(crate::replay::play_through::<LuaGame>(&dir, &replay).unwrap(), crate::replay::Verdict::Same));
}

/// A slab 20 across, 20 deep and 4 high, its pivot in the middle of its
/// floor (cells -10 to 9 across and deep).
const SLAB: &str = "format = \"voxcade voxel 1\"\nsize = [20, 20, 4]\n\n[palette]\na = { color = \"#806040\" }\n\n[[frame]]\nlayers = \"\"\"\naaaaaaaaaaaaaaaaaaaa *20\n*4\n\"\"\"\n";

/// A piece with two lights (a flame and its glow) shines with both, each
/// from its own place, turning with the piece; `thing.state.light` dims
/// them together.
#[test]
fn a_pieces_lights_dim_together() {
    let level = ROOM.replace("light = { at = [0, 0, 3], radius = 30, color = \"#ff8000\" }", "lights = [\n  { at = [0, 0, 3], radius = 10, color = \"#ff8000\" },\n  { at = [1, 0, 3], radius = 30, color = \"#ffffff\" },\n]");
    let mut g = game_in(&pieces_folder("two-lights", "", &level, ""));
    assert_eq!(halted(&g), "");
    let frame = |g: &Session<LuaGame>| g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    let (flame, glow) = (glam::Vec3::new(40.5, 30.5, 15.5), glam::Vec3::new(39.5, 30.5, 15.5));
    assert_eq!((lights_at(&frame(&g), flame), lights_at(&frame(&g), glow)), (vec![[1.0, 128.0 / 255.0, 0.0]], vec![[1.0; 3]]), "facing south, the model's +x is the camera's left");
    said(&mut g, "level:thing('lamp').state.light = 0.5");
    assert_eq!((lights_at(&frame(&g), flame), lights_at(&frame(&g), glow)), (vec![[0.5, 64.0 / 255.0, 0.0]], vec![[0.5; 3]]));
    assert_eq!(frame(&g).lights.len(), 3, "the lamp's two and the moon");
}

/// A solid piece holds a body up (in the air too) and stops it, exactly
/// where its model is drawn; taken away (`thing:remove`), its voxels let
/// everything through at the tick's end; a snapshot brings it back; the
/// world's own voxels stay empty there and it is no part of a blast.
#[test]
fn a_solid_piece_holds_bodies_up_and_stops_them() {
    let level = "format = \"voxcade level 1\"\nsize = [64, 64, 40]\n\n[[thing]]\nname = \"table\"\nmodel = \"slab.vvx\"\nat = [32, 32, 10]\nsolid = true\n\n[[thing]]\nname = \"wall\"\nmodel = \"slab.vvx\"\nat = [32, 10, 0]\nsolid = true\n\n[[place]]\nname = \"start\"\nat = [30, 30, 30]\n";
    let dir = pieces_folder("solid", "", level, "function on_tick() player:walk(input.dir, 40) end\n");
    std::fs::write(dir.join("slab.vvx"), SLAB).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 90);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "player.z"), "14.0", "standing on the table's top");
    assert_eq!(said(&mut g, "level:solid(32, 32, 13)"), "true");
    assert_eq!(said(&mut g, "level:voxel(32, 32, 13)"), "0", "no voxel of the world: the model draws it");
    // Facing south (as a piece does unless its file says), an even size
    // flipped: cells -9 to 10 from the pivot, as it is drawn.
    assert_eq!(said(&mut g, "level:solid(22, 32, 13)") + &said(&mut g, "level:solid(23, 32, 13)") + &said(&mut g, "level:solid(42, 32, 13)") + &said(&mut g, "level:solid(43, 32, 13)"), "falsetruetruefalse");
    let kept = g.snapshot().unwrap();
    let hash = g.game.state_hash();
    said(&mut g, "level:explode({ x = 32, y = 32, z = 14 }, 6)");
    assert_eq!(said(&mut g, "level:solid(32, 32, 13)"), "true", "a blast leaves it");
    said(&mut g, "level:thing('table'):remove()");
    assert_eq!(said(&mut g, "level:solid(32, 32, 13)"), "false", "gone at the tick's end");
    assert_ne!(g.game.state_hash(), hash);
    steps(&mut g, 90);
    assert_eq!(said(&mut g, "player.z + 0"), "0.0", "fallen to the floor");
    g.restore(&kept);
    assert_eq!(said(&mut g, "level:solid(32, 32, 13)"), "true", "a snapshot brings it back");
    // Walking into the wall's side: stopped at its face (y 10 + 10).
    said(&mut g, "player:place(32, 40, 0)");
    for _ in 0..90 {
        g.step(&Input { down: true, ..Input::default() });
    }
    let y: f32 = said(&mut g, "player.y").parse().unwrap();
    assert!((21.0..24.0).contains(&y), "stopped by the wall at y {y}");
}

/// A solid piece turned off the quarter turns does not load, in a game
/// that turns things freely too: the mistake names it and the way out.
#[test]
fn a_solid_piece_keeps_to_the_quarter_turns() {
    let level = ROOM.replace("facing = \"east\"", "rotation = 30\nsolid = true");
    let e = start_error(&pieces_folder("solid-turned", "  c.camera.rotates = \"free\"\n", &level, ""));
    assert!(e.contains("'stove' (line 5)") && e.contains("solid but turned 30 degrees") && e.contains("quarter turns"), "{e}");
    let quarter = ROOM.replace("facing = \"east\"", "rotation = 90\nsolid = true");
    let mut g = game_in(&pieces_folder("solid-quarter", "  c.camera.rotates = \"free\"\n", &quarter, ""));
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:solid(10, 20, 3)"), "true");
}

/// The jump & run's hero stands on a solid piece's top and is stopped by
/// its side.
#[test]
fn the_hero_stands_on_a_solid_piece() {
    let dir = crate::scratch::Scratch::new("pieces-solid-hero");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3 }\n  c.things.hero = { behavior = \"hero\", height = 6 }\nend\n").unwrap();
    std::fs::write(dir.join("slab.vvx"), SLAB).unwrap();
    let wall = "format = \"voxcade voxel 1\"\nsize = [4, 20, 20]\n\n[palette]\na = { color = \"#806040\" }\n\n[[frame]]\nlayers = \"\"\"\naaaa *20\n*20\n\"\"\"\n";
    std::fs::write(dir.join("wall.vvx"), wall).unwrap();
    let floor = "format = \"voxcade voxel 1\"\nsize = [128, 16, 2]\n\n[palette]\na = { color = \"#605040\" }\n\n[[frame]]\nlayers = \"\"\"\n".to_string() + &"a".repeat(128) + " *16\n*2\n\"\"\"\n";
    std::fs::write(dir.join("floor.vvx"), floor).unwrap();
    // A floor of a piece too: the hero walks on it (the world below the
    // level would be falling out of it).
    let level = "format = \"voxcade level 1\"\nsize = [128, 16, 60]\n\n[[thing]]\nkind = \"hero\"\nat = [48, 8, 30]\n\n[[thing]]\nname = \"floor\"\nmodel = \"floor.vvx\"\nat = [64, 8, 0]\nfacing = \"north\"\nsolid = true\n\n[[thing]]\nname = \"table\"\nmodel = \"slab.vvx\"\nat = [48, 8, 10]\nsolid = true\n\n[[thing]]\nname = \"wall\"\nmodel = \"wall.vvx\"\nat = [100, 8, 2]\nfacing = \"north\"\nsolid = true\n";
    std::fs::write(dir.join("levels/room.toml"), level).unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    steps(&mut g, 90);
    assert_eq!(said(&mut g, "player.z"), "14.0", "on the table's top");
    // On the floor, running right into the wall (x 98 to 101).
    said(&mut g, "player:place(80, 8, 2)");
    for _ in 0..120 {
        g.step(&Input { right: true, ..Input::default() });
    }
    let (x, z): (f32, f32) = (said(&mut g, "player.x").parse().unwrap(), said(&mut g, "player.z").parse().unwrap());
    assert!(x < 98.0 && x > 90.0 && z == 2.0, "stopped by the wall at {x}, {z}");
}

/// A level of a terrain and pieces: the hero lands on the land where its
/// column's height says (`level:ground` too), and on a solid piece set on
/// it; the land is loaded the same every time.
#[test]
fn the_hero_stands_on_a_terrain() {
    let dir = crate::scratch::Scratch::new("terrain-hero");
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3 }\n  c.things.hero = { behavior = \"hero\", height = 6 }\nend\n").unwrap();
    std::fs::write(dir.join("slab.vvx"), SLAB).unwrap();
    let level = "format = \"voxcade level 1\"\nsize = [128, 64, 80]\n\n[terrain]\nspacing = 16\nheights = \"\"\"\n 4  6 10 20 30 30 20 10\n 4  8 14 24 36 40 20 10\n 4  6 10 20 30 30 20 10\n 2  2  2  2  2  2  2  2\n\"\"\"\nshapes = \"\"\"\n. . . . # # . .\n. . . . # # . .\n. . . . # # . .\n. . . . . . . .\n\"\"\"\n\n[[terrain.material]]\ncolor = \"#7d7468\"\nsteeper = 45\n\n[[terrain.material]]\ncolor = \"#5c8a3a\"\ngrain = 0.3\n\n[[thing]]\nkind = \"hero\"\nat = [24, 40, 60]\n\n[[thing]]\nname = \"slab\"\nmodel = \"slab.vvx\"\nat = [72, 40, 36]\nsolid = true\n";
    std::fs::write(dir.join("levels/hills.toml"), level).unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    let ground = said(&mut g, "level:ground(24, 40)");
    let t = crate::grid::GridLevel::from_text(level).unwrap().terrain.unwrap();
    assert_eq!(ground, t.surface().at(glam::IVec2::new(24, 40)).unwrap().to_string(), "the ground is the land's height there");
    steps(&mut g, 90);
    assert_eq!(said(&mut g, "player.z"), format!("{ground}.0"), "the hero on the land");
    // On the slab set on the terraced land (STEP): its top at 40.
    said(&mut g, "player:place(72, 40, 50)");
    steps(&mut g, 90);
    assert_eq!(said(&mut g, "player.z"), "40.0", "on the slab on the plateau");
    let again = game_in(&dir);
    assert_eq!(g.game.sim().world.content_hash(), again.game.sim().world.content_hash(), "the same voxels every time");
}

/// A thing of a kind its level's file turns (`facing`, `rotation`) starts
/// facing so, as `level:spawn`'s facing turns it: the nearest of the four
/// ways for a kind on the grid, the bearing exactly for one that turns
/// freely; a kind with a behavior turned in a file does not load.
#[test]
fn a_thing_of_a_kind_faces_as_its_file_turns_it() {
    let main = r#"
function on_level_start(how)
  local door, wisp, plain = level:thing("door"), level:thing("wisp"), level:thing("plain")
  world.state.door = door.facing.x * 10 + door.facing.y
  world.state.slant = door.facing.x
  world.state.wisp_x = math.floor(wisp.facing.x * 1000 + 0.5)
  world.state.wisp_y = math.floor(wisp.facing.y * 1000 + 0.5)
  world.state.plain = plain.facing.x * 10 + plain.facing.y
end
"#;
    let dir = levels_folder("kind-facing", main, &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.things.box = {}\n  c.things.wisp = { rotates = \"free\" }\nend\n").unwrap();
    let path = dir.join("levels/a.toml");
    let things = "\n[[thing]]\nname = \"door\"\nkind = \"box\"\nat = [8, 2, 4]\nrotation = 100\n\n[[thing]]\nname = \"plain\"\nkind = \"box\"\nat = [4, 2, 4]\n\n[[thing]]\nname = \"wisp\"\nkind = \"wisp\"\nat = [12, 2, 4]\nrotation = 30\n";
    std::fs::write(&path, std::fs::read_to_string(&path).unwrap() + things).unwrap();
    // The file reads and writes it as it says.
    let level = crate::grid::GridLevel::read(&path).unwrap();
    assert_eq!(level.things.iter().map(|t| t.facing.map(crate::grid::Facing::bearing)).collect::<Vec<_>>(), [Some(100.0), None, Some(30.0)]);
    assert_eq!(crate::grid::GridLevel::from_text(&level.to_text()).unwrap(), level, "round trip");
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    let state = &g.game.inspect()["state"];
    assert_eq!(state["door"], 10, "rotation 100 on the grid: east, {state}");
    assert_eq!(state["plain"], -1, "unturned: as a script's thing stands, {state}");
    assert_eq!((state["wisp_x"].as_i64(), state["wisp_y"].as_i64()), (Some(0), Some(1000)), "its way among the four: north, {state}");
    // Drawn facing exactly 30 degrees (`rotates = "free"` draws by it).
    let wisp = g.game.stage.named("wisp").unwrap();
    let facing = g.game.stage.host.get(crate::sim::things::Id::from_raw(wisp)).unwrap().body.facing;
    assert!((facing.x - 0.5).abs() < 1e-4 && (facing.y - 0.866).abs() < 1e-3, "turned freely: {facing}");
    // A kind with a behavior turns by its rules: refused as the game loads.
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.things.box = { behavior = \"pickup\" }\n  c.things.wisp = { rotates = \"free\" }\nend\n").unwrap();
    let e = start_error(&dir);
    assert!(e.contains("ready-made rules"), "{e}");
}

/// A level file saved while the game runs that turns a thing of a kind
/// with a behavior is refused as at the load: the game keeps the level it
/// plays.
#[test]
fn a_level_read_anew_refuses_a_ruled_kind_turned() {
    let dir = levels_folder("kind-facing-reload", "", &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.things.box = { behavior = \"pickup\" }\nend\n").unwrap();
    let path = dir.join("levels/a.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.clone() + "\n[[thing]]\nkind = \"box\"\nat = [8, 2, 4]\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    std::fs::write(&path, text + "\n[[thing]]\nkind = \"box\"\nat = [8, 2, 4]\nfacing = \"east\"\n").unwrap();
    let e = format!("{:#}", g.reload(&path).unwrap_err());
    assert!(e.contains("ready-made rules"), "{e}");
}

/// A door of three parts: its frame (the root), its leaf hung on the
/// frame's left post (a joint set by hand at the hinge) and a handle on
/// the leaf, in front of it.
const DOOR: &str = r##"format = "voxcade voxel 1"
size = [9, 3, 12]

[palette]
p = "#6a4a30"
l = "#a07040"
h = "#d0b040"

[[part]]
name = "frame"
at = [-4, 0, 0]
layers = """
p.......p
*11

ppppppppp
"""

[[part]]
name = "leaf"
at = [-3, 0, 0]
parent = "frame"
joint = [-3, 0.5, 5.5]
layers = """
lllllll
*11
"""

[[part]]
name = "handle"
at = [2, -1, 5]
parent = "leaf"
layers = """
h
"""
"##;

/// A level of a door, a box (no figure) and a light alone.
const DOORWAY: &str = r##"format = "voxcade level 1"
size = [64, 64, 32]

[[thing]]
name = "door"
model = "door.vvx"
at = [32, 32, 0]

[[thing]]
name = "box"
model = "box.vvx"
at = [10, 50, 0]

[[thing]]
name = "lamp"
at = [5, 5, 20]
light = { radius = 30 }

[[place]]
name = "start"
at = [32, 10, 0]
"##;

/// A game in `DOORWAY` with this `main.lua`.
fn doorway(name: &str, main: &str) -> Session<LuaGame> {
    let dir = pieces_folder(name, "", DOORWAY, main);
    std::fs::write(dir.join("door.vvx"), DOOR).unwrap();
    game_in(&dir)
}

/// The door's draws in a picture (the box's last: the pieces in the order
/// the level sets them down, the script's after them).
fn door_draws(g: &Session<LuaGame>) -> Vec<crate::render::ModelDraw> {
    g.compose(0.0, 16.0 / 9.0, Scene::Play, None).models
}

/// `thing.state.parts` poses a piece whose model is a figure of parts, as
/// `frame:model`'s pose would: the same draws as `frame:model` of its
/// model with that pose where it stands, a part turned about its joint
/// with what hangs on it; nil or an empty table draws it at rest exactly as
/// before. It is the thing's state: in its hash (only while it is set),
/// in a snapshot, in `inspect`; only to look at in `on_draw`.
#[test]
fn a_pieces_parts_are_posed_by_its_state() {
    let pose = "{ leaf = { rotation = -90 }, handle = { roll = 20 } }";
    let mut g = doorway("parts", "");
    assert_eq!(halted(&g), "");
    let rest = door_draws(&g);
    assert_eq!(rest.len(), 4, "a draw a part of the door, and the box");
    assert!(rest.iter().all(|m| m.rot == glam::Quat::IDENTITY), "at rest");
    let hash = g.game.state_hash();
    said(&mut g, &format!("level:thing('door').state.parts = {pose}"));
    assert_eq!(halted(&g), "");
    let posed = door_draws(&g);
    assert_eq!(posed.len(), 4);
    assert_eq!(posed[0], rest[0], "the frame stays");
    assert_eq!(posed[3], rest[3], "the box is no part of it");
    assert!(posed[1].rot != glam::Quat::IDENTITY && posed[2].rot != posed[1].rot, "the leaf turned, the handle further");
    assert_ne!(g.game.state_hash(), hash, "the pose is part of the game");
    let inspect = g.game.inspect();
    let door = inspect["things"].as_array().unwrap().iter().find(|t| t["name"] == "door").unwrap().clone();
    assert_eq!(door["state"]["parts"]["leaf"]["rotation"].as_i64(), Some(-90), "{door}");
    assert_eq!(said(&mut g, "level:thing('door').state.parts.handle.roll"), "20");

    // As frame:model of the same model and pose on the same thing draws it,
    // after the piece at rest.
    let drawn = doorway("parts-frame", &format!("function on_draw(frame)\n  frame:model('door.vvx', level:thing('door'), nil, nil, {pose})\nend\n"));
    assert_eq!(halted(&drawn), "");
    let both = door_draws(&drawn);
    assert_eq!(&both[..4], &rest[..], "the piece at rest");
    assert_eq!(&both[4..], &posed[..3], "frame:model's pose is the piece's");

    // A snapshot keeps it.
    let kept = g.snapshot().unwrap();
    said(&mut g, "level:thing('door').state.parts = nil");
    assert_eq!(door_draws(&g), rest, "nil: at rest");
    assert_eq!(g.game.state_hash(), hash, "unset, the hash is as before");
    said(&mut g, "level:thing('door').state.parts = {}");
    assert_eq!(door_draws(&g), rest, "an empty pose: at rest");
    g.restore(&kept);
    assert_eq!(door_draws(&g), posed, "a snapshot keeps the pose");

    // Changed in on_tick a part at a time, read in on_draw (only to look).
    let mut g = doorway("parts-tick", "function on_tick()\n  local door = level:thing('door')\n  door.state.parts = door.state.parts or { leaf = { rotation = 0 } }\n  door.state.parts.leaf.rotation = door.state.parts.leaf.rotation - 3\nend\nfunction on_draw(frame)\n  frame:text(tostring(level:thing('door').state.parts.leaf.rotation), 4, 4)\nend\n");
    steps(&mut g, 30);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:thing('door').state.parts.leaf.rotation"), "-90");
    assert_eq!(door_draws(&g)[1], posed[1], "the leaf as posed by the console");
    assert_eq!(first_text(&g), "-90");
    let g = doorway("parts-draw", "function on_draw(frame)\n  level:thing('door').state.parts = { leaf = { rotation = 10 } }\nend\n");
    let _ = door_draws(&g);
    assert!(halted(&g).contains("only to look at here (parts)"), "{}", halted(&g));
}

/// A piece only casting a shadow (`shadow_only = true`): its draws are the
/// same as without the key, marked to go into the sun's shadow map alone
/// (the renderer leaves them out of the scene and the outline); it is a
/// thing by its name as any piece, its parts posed by its state still move
/// what casts, `thing:remove` takes its shadow away, and nothing of it is
/// in the game's state; solid too, it blocks as a solid piece does.
#[test]
fn a_piece_only_casting_a_shadow_is_not_drawn() {
    let casting = DOORWAY.replace("at = [32, 32, 0]\n", "at = [32, 32, 0]\nshadow_only = true\n");
    let dir = pieces_folder("shadow-only", "", &casting, "");
    std::fs::write(dir.join("door.vvx"), DOOR).unwrap();
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    let plain = doorway("shadow-only-plain", "");
    let (drawn, as_drawn) = (door_draws(&g), door_draws(&plain));
    assert_eq!(drawn.len(), 4, "the door's three parts and the box");
    assert!(drawn[..3].iter().all(|m| m.shadow_only) && !drawn[3].shadow_only, "only the door's draws cast alone");
    let unmarked: Vec<_> = drawn.iter().cloned().map(|m| crate::render::ModelDraw { shadow_only: false, ..m }).collect();
    assert_eq!(unmarked, as_drawn, "in the shadow map as the same piece without the key");
    assert_eq!(g.game.state_hash(), plain.game.state_hash(), "nothing of it in the game's state");
    assert_eq!(said(&mut g, "level:thing('door').kind"), "\"piece\"");
    // Posed: what casts moves.
    said(&mut g, "level:thing('door').state.parts = { leaf = { rotation = -90 } }");
    let posed = door_draws(&g);
    assert!(posed[1].shadow_only && posed[1].rot != glam::Quat::IDENTITY, "the leaf turned in the shadow map");
    // Taken away: no shadow.
    said(&mut g, "level:thing('door'):remove()");
    steps(&mut g, 1);
    let left = door_draws(&g);
    assert!(left.len() == 1 && !left[0].shadow_only, "only the box: {left:?}");
    // Solid as well: it blocks where it would be drawn.
    let solid = DOORWAY.replace("name = \"box\"\nmodel = \"box.vvx\"\nat = [10, 50, 0]\n", "name = \"box\"\nmodel = \"box.vvx\"\nat = [10, 50, 0]\nsolid = true\nshadow_only = true\n");
    let dir = pieces_folder("shadow-only-solid", "", &solid, "");
    std::fs::write(dir.join("door.vvx"), DOOR).unwrap();
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:solid(10, 50, 1)"), "true");
    assert!(door_draws(&g).last().is_some_and(|m| m.shadow_only));
}

/// A pose a piece cannot take is a mistake that names the piece and the
/// part: set in a handler, with the line that set it; changed inside it, at
/// the tick's end. A part the figure lacks, a model that is no figure, a
/// light alone, a rotation that is no number, the old word `turn`, a pose
/// that is no table.
#[test]
fn a_pieces_pose_is_checked() {
    for (line, says) in [
        ("level:thing('door').state.parts = { leef = { rotation = 9 } }", "thing.state.parts of 'door': door.vvx has no part 'leef' (its parts: frame, leaf, handle)"),
        ("level:thing('box').state.parts = { lid = { pitch = 9 } }", "thing.state.parts of 'box': box.vvx is no figure of parts"),
        ("level:thing('lamp').state.parts = { leaf = { pitch = 9 } }", "thing.state.parts of 'lamp': it has no model to pose"),
        ("level:thing('door').state.parts = { leaf = { rotation = 'open' } }", "thing.state.parts of 'door', part 'leaf': the rotation's rotation is a number of degrees"),
        ("level:thing('door').state.parts = { leaf = { turn = 90 } }", "thing.state.parts of 'door', part 'leaf': 'turn' is called 'rotation' now"),
        ("level:thing('door').state.parts = { leaf = 90 }", "thing.state.parts of 'door': a pose is parts by name, each a rotation"),
        ("level:thing('door').state.parts = 'open'", "thing.state.parts of 'door': a pose is parts by name, each a rotation ({ leaf = { rotation = 90 } }), or nil for at rest, not a string"),
    ] {
        let mut g = doorway("pose-wrong", &format!("function on_tick()\n  {line}\nend\n"));
        steps(&mut g, 1);
        assert!(halted(&g).contains(says) && halted(&g).contains("main.lua:2"), "{line}: {}", halted(&g));
    }
    let mut g = doorway("pose-inside", "function on_tick()\n  local door = level:thing('door')\n  door.state.parts = { leaf = { rotation = 9 } }\n  door.state.parts.leaf.pitch = 'up'\nend\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("thing.state.parts of 'door', part 'leaf': the rotation's pitch is a number of degrees") && halted(&g).contains("at the end of tick"), "{}", halted(&g));
    // Nil or an empty pose: at rest, for any piece.
    let mut g = doorway("pose-none", "");
    said(&mut g, "level:thing('box').state.parts = {}");
    said(&mut g, "level:thing('lamp').state.parts = nil");
    assert_eq!(said(&mut g, "level:thing('box').state.parts ~= nil"), "true");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
}

/// A part turned at rest in its file shows wherever the figure is drawn,
/// as the unturned figure posed so: `voxcade sheet`'s pictures
/// (`Session::still`) and a level's piece.
#[test]
fn a_part_turned_at_rest_shows_in_the_sheet_and_the_level() {
    let mut plain = doorway("rest-plain", "");
    let dir = pieces_folder("rest-turned", "", DOORWAY, "");
    std::fs::write(dir.join("door.vvx"), DOOR.replace("joint = [-3, 0.5, 5.5]\n", "joint = [-3, 0.5, 5.5]\nrotation = -60\n")).unwrap();
    let rested = game_in(&dir);
    let leaf = crate::session::turning(-60.0, 0.0, 0.0);
    let posed = |part: &str| if part == "leaf" { leaf } else { glam::Quat::IDENTITY };
    let sheet = |g: &Session<LuaGame>, pose: &dyn Fn(&str) -> glam::Quat| g.still("door.vvx", 0, glam::IVec2::NEG_Y, glam::Quat::IDENTITY, &|p| crate::session::PartPose::turned(pose(p)), 40.0, 1.0).unwrap().models;
    assert_eq!(sheet(&rested, &|_| glam::Quat::IDENTITY), sheet(&plain, &posed), "the sheet");
    assert_ne!(sheet(&rested, &|_| glam::Quat::IDENTITY), sheet(&plain, &|_| glam::Quat::IDENTITY));
    said(&mut plain, "level:thing('door').state.parts = { leaf = { rotation = -60 } }");
    assert_eq!(door_draws(&rested), door_draws(&plain), "the level's piece");
}

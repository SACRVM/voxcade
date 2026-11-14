//! The picture: daylight, the camera, models posed and turned, the logo,
//! the HUD and the language.

use super::*;

/// The HUD is the game's: icons from c.icons, a bar's two parts, text
/// widths, an icon bursting; a wrong icon is a mistake with its name.
#[test]
fn the_hud_is_the_games() {
    use crate::lua::world::Cmd;
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
    let cmds = g.game.drawn.borrow().clone();
    assert!(matches!(cmds[0], Cmd::Icon { .. }));
    let boxes: Vec<(f32, f32)> = cmds.iter().filter_map(|c| match c { Cmd::Box { x, width, .. } => Some((*x, *width)), _ => None }).collect();
    assert_eq!(boxes, [(4.0, 5.0), (9.0, 5.0)], "half filled, the rest beside it");
    assert_eq!(g.game.stage.hud.bits.len(), 3 * g.cfg.ui.depth as usize, "the icon burst into its voxels");
    let bad = game_in(&kit_folder("hud-bad", conf, "function on_draw(frame) frame:icon(\"heart\", 0, 0) end\n"));
    bad.compose(0.0, 1.6, Scene::Play, None);
    assert!(halted(&bad).contains("no icon called 'heart' (c.icons: dot)"), "{}", halted(&bad));
    let dir = kit_folder("hud-colour", "  c.icons.dot = { rows = { \"RX\" }, colors = { R = { 1, 2, 3 } } }", "");
    let e = Session::<LuaGame>::load(&dir).map(|_| ()).unwrap_err();
    assert!(format!("{e:#}").contains("c.icons.dot.rows[1]: the letter 'X' has no color"), "{e:#}");
}

/// `frame:to_screen` says where a point of the level shows, in the HUD's
/// measure: the player's feet below the middle (the camera looks at its
/// middle), higher up when lifted, a point behind the camera nowhere.
#[test]
fn a_point_of_the_level_shows_on_the_screen() {
    use crate::lua::world::Cmd;
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
        .drawn
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
        ("frame:daylight{ sky = \"blue\" }", "sky is a color"),
    ] {
        let mut g = game_in(&code_world("daylight-bad", &format!("function on_draw(frame) {bad} end\n"), &[]));
        steps(&mut g, 1);
        g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
        assert!(halted(&g).contains(want), "{bad}: {}", halted(&g));
    }
}

/// `frame:camera` sets this picture's camera alone: a picture with it is
/// seen from there (`look_at`, `width`; without `look_at` still on what the
/// game's camera follows; a point without its height at the camera's),
/// the next without it from the game's camera, as if it never was; a wrong
/// key or value, and a turn round (yaw), say so.
#[test]
fn a_picture_sets_its_camera() {
    use crate::camera;
    let main = "function on_draw(frame)\n\
                  if world.tick == 2 then frame:camera{ look_at = { 10, 20, 3 }, width = 40 } end\n\
                  if world.tick == 3 then frame:camera{ width = 60 } end\n\
                  if world.tick == 4 then frame:camera{ look_at = { 10, 20 } } end\n\
                end\n";
    let mut g = game_in(&code_world("camera", main, &[]));
    let plain = |g: &Session<LuaGame>| {
        let cam = &g.cfg.camera;
        camera::view_proj(cam, g.view_target(0.0), 16.0 / 9.0)
    };
    let mut cam = g.cfg.camera.clone();
    steps(&mut g, 1);
    assert_eq!(g.compose(0.0, 16.0 / 9.0, Scene::Play, None).view_proj, plain(&g), "the game's camera");
    steps(&mut g, 1);
    let f = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    cam.show_width(40.0);
    let at = glam::Vec3::new(10.0, 20.0, 3.0);
    assert_eq!(halted(&g), "");
    assert_eq!(f.view_proj, camera::view_proj(&cam, at, 16.0 / 9.0), "seen from where the picture says");
    assert_eq!(f.eye, camera::eye(&cam, at, 16.0 / 9.0));
    steps(&mut g, 1);
    // A width alone: closer, on what the game's camera follows.
    let mut zoomed = g.cfg.camera.clone();
    zoomed.show_width(60.0);
    let f = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(f.view_proj, camera::view_proj(&zoomed, g.view_target(0.0), 16.0 / 9.0), "zoomed in, still following");
    steps(&mut g, 1);
    // A point without its height: the camera's own height there.
    let f = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    let z = g.view_target(0.0).z;
    assert_eq!(f.view_proj, camera::view_proj(&g.cfg.camera, glam::Vec3::new(10.0, 20.0, z), 16.0 / 9.0));
    steps(&mut g, 1);
    assert_eq!(g.compose(0.0, 16.0 / 9.0, Scene::Play, None).view_proj, plain(&g), "the next picture without it: the game's camera");
    assert_eq!(g.face_mask(), game_in(&code_world("camera-none", "function on_tick() end\n", &[])).face_mask(), "moved and zoomed: the same faces");
    for (bad, want) in [
        ("frame:camera{ yaw = 30 }", "does not turn round yet"),
        ("frame:camera{ width = 0 }", "above 0"),
        ("frame:camera{ width = 40, height = 30 }", "not both"),
        ("frame:camera{ pitch = 120 }", "from -89"),
        ("frame:camera{ pitch = \"steep\" }", "pitch is degrees"),
        ("frame:camera{ fov = 5 }", "from 10 to 100"),
        ("frame:camera{ look_at = { 1 } }", "look_at is a point"),
        ("frame:camera{ zoom = 2 }", "no 'zoom'"),
        ("frame:camera{ aspect = 2 }", "c.camera.aspect"),
        ("frame:camera(3)", "a table"),
    ] {
        let mut g = game_in(&code_world("camera-bad", &format!("function on_draw(frame) {bad} end\n"), &[]));
        steps(&mut g, 1);
        g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
        assert!(halted(&g).contains(want), "{bad}: {}", halted(&g));
    }
}

/// `frame:to_screen` after `frame:camera` says where a point shows in the
/// picture's camera: the point looked at in the middle.
#[test]
fn to_screen_goes_by_the_pictures_camera() {
    let main = "function on_draw(frame)\n\
                  frame:camera{ look_at = { 7, 9, 2 }, width = 50 }\n\
                  local x, y = frame:to_screen({ x = 7, y = 9, z = 2 })\n\
                  frame:text(string.format('%d %d', math.floor(x + 0.5), math.floor(y + 0.5)), 1, 1)\n\
                end\n";
    let mut g = game_in(&code_world("camera-screen", main, &[]));
    steps(&mut g, 1);
    g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    let layout = crate::ui::Layout::of(&g.cfg.ui, 16.0 / 9.0);
    assert_eq!(first_text(&g), format!("{} {}", (layout.width * 0.5 + 0.5).floor(), (layout.height * 0.5 + 0.5).floor()));
}

/// Drawing with a camera of the picture's own changes nothing the game
/// keeps: the state hash is the same drawn or not, and a run recorded
/// while drawing plays back the same (a replay draws after every tick).
#[test]
fn a_pictures_camera_leaves_the_game_alone() {
    use crate::replay::{Recorder, Verdict, play_through};
    let main = "function on_world_start() world.state.n = 0 end\n\
                function on_tick() world.state.n = world.state.n + math.random(1, 3) end\n\
                function on_draw(frame) frame:camera{ look_at = { 8 + world.tick % 20, 16, 2 }, width = 30 + world.tick % 50, pitch = 20 } end\n";
    let dir = code_world("camera-replay", main, &[]);
    let (mut drawn, mut not) = (game_in(&dir), game_in(&dir));
    let mut recorder = Recorder::default();
    for _ in 0..240 {
        drawn.step(&Input::default());
        not.step(&Input::default());
        drawn.compose(0.5, 21.0 / 9.0, Scene::Play, None);
        recorder.record(&drawn);
        assert_eq!(drawn.game.state_hash(), not.game.state_hash(), "drawn or not, the same game");
    }
    assert_eq!(halted(&drawn), "");
    let replay = recorder.replay(&drawn).unwrap();
    assert_eq!(play_through::<LuaGame>(&dir, &replay).unwrap(), Verdict::Same);
}

/// The faces a picture's camera needs: one moved or zoomed looks along the
/// same rays (the same faces, the same meshes); one at another pitch adds
/// what its rays can hit (looking up: the faces underneath), from that
/// picture on, and a mesh then holds them.
#[test]
fn a_pictures_pitch_keeps_the_faces_it_sees() {
    use crate::mesh::{DIRS, mesh_chunk};
    let main = "function on_level_start() level:fill(4, 4, 4, 12, 12, 5, '#806040') end\n\
                function on_draw(frame) if world.tick >= 3 then frame:camera{ pitch = -20 } else frame:camera{ look_at = { 4, 4, 2 }, width = 20 } end end\n";
    let mut g = game_in(&code_world("camera-faces", main, &[]));
    let before = g.face_mask();
    let under = DIRS.iter().position(|d| d.z < 0).unwrap();
    assert!(!before[under], "the game's camera never sees under");
    steps(&mut g, 2);
    g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(g.face_mask(), before, "moved and zoomed: the same faces");
    steps(&mut g, 1);
    g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    let after = g.face_mask();
    assert!(after[under] && before.iter().zip(after).all(|(b, a)| !b || a), "looking up: the faces underneath too, {after:?}");
    steps(&mut g, 1);
    assert_eq!(g.face_mask(), after, "kept from then on");
    let (mut v, mut i, mut w, mut j) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    mesh_chunk(g.world_mut(), glam::IVec3::ZERO, &before, &mut v, &mut i);
    mesh_chunk(g.world_mut(), glam::IVec3::ZERO, &after, &mut w, &mut j);
    assert!(w.len() > v.len() && w.iter().any(|x| x.normal[2] < 0), "the slab's underside in the mesh");
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
    assert!(shown.iter().any(|c| colours.contains(c)), "fronts in the model's colors");
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
    assert_eq!(state(&mut g, "lang"), format!("{:?}", crate::lua::Value::Str(b"de".to_vec())), "the game's first");
    let mut controls = crate::controls::Controls::nobody(&[]);
    controls.language = Some("fr".into());
    g.set_controls(controls.clone());
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "lang"), format!("{:?}", crate::lua::Value::Str(b"fr".to_vec())));
    assert_eq!(g.language(), "fr");
    controls.language = Some("es".into());
    g.set_controls(controls.clone());
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "lang"), format!("{:?}", crate::lua::Value::Str(b"de".to_vec())), "one it does not speak gives way");
    let text = toml::to_string(&controls).unwrap();
    assert!(text.contains("language = \"es\"") && toml::from_str::<crate::controls::Controls>(&text).unwrap() == controls, "replays carry it: {text}");
    let mut plain = game_in(&code_world("lang-none", main, &[]));
    steps(&mut plain, 1);
    assert_eq!(state(&mut plain, "lang"), format!("{:?}", crate::lua::Value::Str(b"en".to_vec())), "English without c.languages");
    for (bad, want) in [("{ \"deu\" }", "c.languages[1]: \"deu\" is no language"), ("{ \"de\", \"de\" }", "c.languages: \"de\" twice")] {
        let dir = code_world("lang-bad", main, &[]);
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        std::fs::write(dir.join("conf.lua"), conf.replace("c.things.box = {}", &format!("c.things.box = {{}}\n  c.languages = {bad}"))).unwrap();
        let e = start_error(&dir);
        assert!(e.contains(want), "{bad}: {e}");
    }
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
    // An offset moves the arm's draw by voxels; a key a pose entry lacks
    // is a mistake naming what it takes.
    let shift = |m: &crate::render::ModelDraw| m.shift;
    let before: Vec<glam::Vec3> = frame.models.iter().map(shift).collect();
    std::fs::write(dir.join("main.lua"), main.replace("{ arm = { pitch = 90 } }", "{ arm = { pitch = 90, offset = { 0, 0.5, 0 } } }")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 10);
    let frame = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(halted(&g), "");
    let after: Vec<glam::Vec3> = frame.models.iter().map(shift).collect();
    assert_eq!(before[0], after[0], "the body as it was");
    assert!((after[1] - before[1]).length() > 0.49, "the arm moved by its offset: {before:?} {after:?}");
    std::fs::write(dir.join("main.lua"), main.replace("{ arm = { pitch = 90 } }", "{ arm = { pitch = 90, shift = 1 } }")).unwrap();
    let g = game_in(&dir);
    let _ = g.frame(1.0, 16.0 / 9.0);
    assert!(halted(&g).contains("a pose entry has rotation, pitch and roll in degrees and an offset in voxels"), "{}", halted(&g));
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
/// or a kind with rotates = "free" facing exactly its way (a diagonal);
/// otherwise its models stay on the grid in quarter turns. A rotation has
/// only rotation, pitch and roll (turn, its old name, says the new one);
/// rotates is only "quarter" or "free".
#[test]
fn a_game_turns_its_models_freely() {
    let conf = r#"
function conf(c)
  c.world.size = { 64, 64, 16 }
  c.things.miner = { model = "player.vvx" }
  c.things.rover = { model = "player.vvx", rotates = "free" }
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
    assert!(halted(&g).contains("a rotation has rotation, pitch and roll in degrees ({ roll = 20 }), not 'spin'"), "{}", halted(&g));
    std::fs::write(dir.join("main.lua"), main.replace("{ roll = 30, pitch = 5 }", "{ turn = 30 }")).unwrap();
    let g = game_in(&dir);
    let _ = g.frame(1.0, 16.0 / 9.0);
    assert!(halted(&g).contains("frame:model: 'turn' is called 'rotation' now: { rotation = 45 }"), "{}", halted(&g));
    std::fs::write(dir.join("conf.lua"), conf.replace("rotates = \"free\"", "rotates = \"round\"")).unwrap();
    assert!(start_error(&dir).contains("c.things.rover.rotates: \"round\" is neither \"quarter\""), "{}", start_error(&dir));
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

/// `frame:model` draws any model file of the game by its path, one in a
/// folder too, though nothing else names it (no kind, no list): loaded the
/// first time it is drawn, then in the bank. A file that is not there is a
/// mistake at the line that draws it, naming the file; saved while the
/// game stands halted for it, the game draws on. `c.models` is refused
/// with what replaced it.
#[test]
fn any_model_file_draws_where_it_is_named() {
    let conf = |more: &str| format!("
function conf(c)
  c.world.size = {{ 64, 64, 16 }}
  c.things.robot = {{ model = \"robot.vvx\" }}
  {more}
end
");
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.r = level:spawn("robot", 32, 32)
end

function on_draw(frame)
  frame:model("props/sheet.vvx", level.state.r, nil, nil, { body = { rotation = 10 } })
end
"##;
    let dir = folder("any-model", main);
    std::fs::write(dir.join("conf.lua"), conf("")).unwrap();
    crate::vox::write_figure(&dir.join("robot.vvx"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO)]);
    std::fs::create_dir_all(dir.join("props")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let _ = g.frame(1.0, 16.0 / 9.0);
    let why = halted(&g);
    assert!(why.contains("main.lua:8") && why.contains("frame:model: no model file \"props/sheet.vvx\" in the game folder"), "{why}");
    // The file saved: the game draws on, the model from the bank.
    crate::vox::write_figure(&dir.join("props/sheet.vvx"), &[("body", glam::IVec3::new(2, 1, 3), glam::IVec3::ZERO), ("corner", glam::IVec3::new(1, 1, 1), glam::IVec3::new(2, 0, 2))]);
    assert_eq!(g.reload(&dir.join("props/sheet.vvx")).unwrap(), crate::session::Reloaded::Models);
    let frame = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(halted(&g), "");
    assert_eq!(frame.models.len(), 2, "the sheet is drawn, each part from the bank");
    assert_eq!(g.models.needed().names(), ["props/sheet.vvx"]);
    let held = g.bank().voxels.len();
    assert!(held > 0 && frame.instances.is_empty(), "its voxels in the bank, none as cubes");
    // A file of no model, a path out of the game: mistakes naming them.
    for (file, says) in [("conf.lua", "conf.lua: a model is a .vvx file"), ("../sheet.vvx", "\"../sheet.vvx\" is no file of the game")] {
        std::fs::write(dir.join("main.lua"), main.replace("props/sheet.vvx", file)).unwrap();
        let g = game_in(&dir);
        let _ = g.frame(1.0, 16.0 / 9.0);
        assert!(halted(&g).contains(says), "{file}: {}", halted(&g));
    }
    // A list of models is no more.
    std::fs::write(dir.join("conf.lua"), conf("c.models = { \"robot.vvx\" }")).unwrap();
    let e = start_error(&dir);
    assert!(e.contains("c.models is no more: frame:model draws any model file of the game by its path"), "{e}");
}

/// A model only drawn is the picture's alone: in memory or not, loaded at
/// the first tick or never, let go half way, the simulation plays the same
/// (each tick's state hash), and the game's fingerprint does not count it.
#[test]
fn a_model_only_drawn_never_changes_the_run() {
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.r = level:spawn("robot", 32, 32)
end

function on_tick()
  level.state.r:walk(input.dir, 10)
end

function on_draw(frame)
  frame:model("sheet.vvx", level.state.r, nil, { rotation = 30 })
end
"##;
    let dir = folder("only-drawn", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.things.robot = { model = \"robot.vvx\" }\nend\n").unwrap();
    crate::vox::write_figure(&dir.join("robot.vvx"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO)]);
    crate::vox::write_figure(&dir.join("sheet.vvx"), &[("body", glam::IVec3::new(3, 1, 4), glam::IVec3::ZERO)]);
    let run = |looking: Option<usize>, letting_go: Option<usize>| {
        let mut g = game_in(&dir);
        let mut hashes = Vec::new();
        for tick in 0..120 {
            let input = Input { right: tick % 40 < 20, up: tick % 30 < 10, ..Input::default() };
            g.step(&input);
            if looking.is_some_and(|from| tick >= from) {
                g.look();
            }
            if letting_go == Some(tick) {
                g.models.needed().let_go(&[]);
            }
            hashes.push(g.game.state_hash());
        }
        assert_eq!(halted(&g), "");
        (hashes, g.fingerprint().unwrap(), g.models.needed().names())
    };
    let (drawn, print, held) = run(Some(0), None);
    assert_eq!(held, ["sheet.vvx"], "drawn: in memory");
    let (never, never_print, none) = run(None, None);
    assert!(none.is_empty(), "never drawn: never loaded");
    assert_eq!(drawn, never, "the run is the same drawn or not");
    assert_eq!(print, never_print);
    let (late, ..) = run(Some(60), Some(90));
    assert_eq!(drawn, late, "loaded late and let go half way, the same again");
    // Its file changed: the run's fingerprint stays.
    crate::vox::write_figure(&dir.join("sheet.vvx"), &[("body", glam::IVec3::new(5, 2, 4), glam::IVec3::ZERO)]);
    assert_eq!(run(Some(0), None).1, print, "a model only drawn is no part of the fingerprint");
}

/// Entering a level loads what it names (its pieces' models) and lets go
/// of what the level left named, out of memory and the bank: what one
/// room shows does not lie in memory in the next; models only drawn go at
/// the change too and come again when drawn. The simulation reads the
/// pieces of every level the same (a piece's height).
#[test]
fn a_level_change_lets_go_of_the_models_of_the_level_left() {
    let hall = ROOM.replace("model = \"box.vvx\"\nat = [40, 30, 12]", "model = \"lamp.vvx\"\nat = [40, 30, 12]");
    let attic = "format = \"voxcade level 1\"\nsize = [64, 48, 40]\n\n[[thing]]\nname = \"chest\"\nmodel = \"props/chest.vvx\"\nat = [20, 20, 0]\n";
    let main = r##"
function on_tick()
  if input:pressed("jump") then world:go_to(level.next or 1) end
end

function on_draw(frame)
  if level.name == "1_room" then frame:model("sheet.vvx", level:thing("stove")) end
end
"##;
    let dir = pieces_folder("change", "  c.actions = { \"jump\" }\n", &hall, main);
    std::fs::rename(dir.join("levels/room.toml"), dir.join("levels/1_room.toml")).unwrap();
    std::fs::write(dir.join("levels/2_attic.toml"), attic).unwrap();
    std::fs::create_dir_all(dir.join("props")).unwrap();
    crate::vox::write_figure(&dir.join("lamp.vvx"), &[("body", glam::IVec3::new(3, 3, 6), glam::IVec3::ZERO)]);
    crate::vox::write_figure(&dir.join("props/chest.vvx"), &[("body", glam::IVec3::new(2, 2, 5), glam::IVec3::ZERO)]);
    crate::vox::write_figure(&dir.join("sheet.vvx"), &[("body", glam::IVec3::new(3, 1, 4), glam::IVec3::ZERO)]);
    let mut g = game_in(&dir);
    let held = |g: &Session<LuaGame>| g.models.needed().names();
    assert_eq!(held(&g), ["box.vvx", "lamp.vvx"], "the first room's pieces, not the attic's chest");
    steps(&mut g, 2);
    let _ = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(halted(&g), "");
    assert_eq!(held(&g), ["box.vvx", "lamp.vvx", "sheet.vvx"], "the sheet drawn: loaded");
    let room_bank = g.bank().voxels.len();
    let tall = |g: &mut Session<LuaGame>, name: &str| {
        let id = crate::sim::things::Id::from_raw(said(g, &format!("level:thing('{name}').id")).parse().unwrap());
        g.game.stage.host.get(id).unwrap().body.height
    };
    let stove = tall(&mut g, "stove");
    assert_eq!(stove, 4.0, "a piece as tall as its model");
    // To the attic.
    g.step(&Input { actions: 1, ..Input::default() });
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "level.name"), "\"2_attic\"");
    assert_eq!(held(&g), ["props/chest.vvx"], "the room's models let go, the attic's loaded");
    let attic_bank = g.bank().voxels.len();
    assert!(attic_bank < room_bank, "the bank holds less: {attic_bank} after {room_bank}");
    assert_eq!(tall(&mut g, "chest"), 5.0, "a piece as tall as its model");
    let frame = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(frame.models.len(), 1, "the chest from the bank");
    // And back: the room's again, the sheet when it is drawn.
    g.step(&Input { actions: 1, ..Input::default() });
    steps(&mut g, 2);
    assert_eq!(held(&g), ["box.vvx", "lamp.vvx"]);
    let _ = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(held(&g), ["box.vvx", "lamp.vvx", "sheet.vvx"]);
    assert_eq!(g.bank().voxels.len(), room_bank, "the bank as it was in the room");
    assert_eq!(tall(&mut g, "stove"), stove);
}

/// A game of sealed files draws a model only drawn from its sealed files,
/// whatever the folder holds since.
#[test]
fn a_sealed_game_draws_a_model_from_its_files() {
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.r = level:spawn("robot", 32, 32)
end

function on_draw(frame)
  frame:model("props/sheet.vvx", level.state.r)
end
"##;
    let dir = folder("sealed-model", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.things.robot = {}\nend\n").unwrap();
    std::fs::create_dir_all(dir.join("props")).unwrap();
    crate::vox::write_figure(&dir.join("props/sheet.vvx"), &[("body", glam::IVec3::new(3, 1, 4), glam::IVec3::ZERO)]);
    let store_dir = crate::scratch::Scratch::new("sealed-model-store");
    let store = crate::files::Store::at(&store_dir);
    let (_, files) = crate::files::Files::seal(&dir, &store).unwrap();
    // The folder's file gone: the sealed one draws.
    std::fs::remove_file(dir.join("props/sheet.vvx")).unwrap();
    let g = Session::<LuaGame>::load_from(files, crate::saves::Slots::default()).unwrap();
    let frame = g.frame(1.0, 16.0 / 9.0);
    assert_eq!(halted(&g), "");
    assert_eq!(frame.models.len(), 1, "drawn from the sealed files");
    assert_eq!(g.models.needed().names(), ["props/sheet.vvx"]);
}

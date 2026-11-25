//! A model drawn on the HUD (`frame:draw`): as a grid's slot shows it, one
//! of its voxels one of the HUD's, anywhere; or whole, turned and posed,
//! from the model bank; only the picture.

use super::*;
use crate::lua::world::{Cmd, Thumb};
use crate::render::Instance;

/// A model 4 across, 3 deep and 4 high: its front row (+y) yellow at the
/// corners, its left side (x 0) red, its right green, white between, a red
/// voxel on top at its front left; its second frame all blue.
const SIGN: &str = r##"format = "voxcade voxel 1"
size = [4, 3, 4]

[palette]
r = "#d02020"
g = "#20b040"
y = "#f0d020"
w = "#e0e0e0"
b = "#2040d0"

[[frame]]
layers = """
y..y
rwwg
rwwg
*3

r...
....
....
"""

[[frame]]
layers = """
bbbb
bbbb
bbbb
*4
"""
"##;

/// A chest of two parts: a body 4 across, 3 deep and 3 high, and a lid on
/// it, hinged at its back top edge.
const CHEST: &str = r##"format = "voxcade voxel 1"
size = [4, 3, 4]

[palette]
b = "#8a5a30"
l = "#c08040"

[[part]]
name = "body"
at = [-2, -1, 0]
layers = """
bbbb
bbbb
bbbb
*3
"""

[[part]]
name = "lid"
at = [-2, -1, 3]
parent = "body"
joint = [0, -1, 3]
layers = """
llll
llll
llll
"""
"##;

/// A game whose `on_draw` is `draw`, with the sign and the chest beside it.
fn drawing(name: &str, draw: &str) -> crate::scratch::Scratch {
    let dir = folder(name, &format!("function on_draw(frame)\n  {draw}\nend\n"));
    std::fs::write(dir.join("sign.vvx"), SIGN).unwrap();
    std::fs::write(dir.join("chest.vvx"), CHEST).unwrap();
    dir
}

/// The HUD's voxels in the sign's colours, each where it lies and its
/// colour, in order.
fn sign_voxels(g: &Session<LuaGame>) -> Vec<([i32; 3], [u8; 4])> {
    let palette = [[208, 32, 32, 255], [32, 176, 64, 255], [240, 208, 32, 255], [224, 224, 224, 255], [32, 64, 208, 255]];
    let ui = crate::config::UiConfig::default();
    let mut out: Vec<([i32; 3], [u8; 4])> = g
        .game
        .hud(&ui, 0.0, 16.0 / 9.0, None)
        .0
        .iter()
        .filter(|i| palette.contains(&i.color))
        .map(|i| ([i.pos_size[0].floor() as i32, i.pos_size[1].floor() as i32, i.pos_size[2].floor() as i32], i.color))
        .collect();
    out.sort();
    out
}

/// The model `frame:draw` asked for, as rows of the sign's letters
/// (top row first, `.` empty).
fn seen(g: &Session<LuaGame>) -> Vec<String> {
    let drawn = g.game.drawn.borrow();
    let Some(Cmd::Draw { thumb, .. }) = drawn.iter().find(|c| matches!(c, Cmd::Draw { .. })) else { panic!("a model on the HUD") };
    rows(thumb)
}

fn rows(t: &Thumb) -> Vec<String> {
    let letter = |c: [u8; 4]| match c[..3] {
        [208, 32, 32] => 'r',
        [32, 176, 64] => 'g',
        [240, 208, 32] => 'y',
        [224, 224, 224] => 'w',
        [32, 64, 208] => 'b',
        _ => '?',
    };
    let mut out = vec![".".repeat(t.width as usize); t.height as usize];
    for &(col, row, color) in &t.voxels {
        out[row as usize].replace_range(col as usize..col as usize + 1, &letter(color).to_string());
    }
    out
}

/// `frame:draw` lays the model's voxels on the HUD with its top left where
/// asked, one of its voxels one of the HUD's, two deep: the voxels a
/// grid's slot lays of it, moved to there.
#[test]
fn a_model_on_the_hud_is_what_a_grid_slot_shows_where_asked() {
    // A slot of 18 at the grid's top left (0, 0): its inside from 6 + 1,
    // the sign 4 by 4 in the middle of its 18, from 6 + 7.
    let grid = drawing("hud-grid", "frame:grid{ x = 0, y = 0, cells = { { model = \"sign.vvx\" } } }");
    let grid = game_in(&grid);
    grid.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(halted(&grid), "");
    let in_slot = sign_voxels(&grid);
    let alone = drawing("hud-draw", "frame:draw(\"sign.vvx\", 13, 13)");
    let alone = game_in(&alone);
    alone.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(halted(&alone), "");
    let drawn = sign_voxels(&alone);
    assert!(!drawn.is_empty() && drawn.len() == in_slot.len(), "{} voxels, the slot's {}", drawn.len(), in_slot.len());
    assert_eq!(drawn, in_slot, "the same voxels as the grid's slot lays");
    // Its top left at 13, 13 from the HUD's top left (the HUD's y up from
    // its bottom), two voxels deep.
    let top = crate::ui::Layout::of(&crate::config::UiConfig::default(), 16.0 / 9.0).height as i32;
    let (x0, y1) = (drawn.iter().map(|v| v.0[0]).min(), drawn.iter().map(|v| v.0[1]).max());
    assert_eq!((x0, y1), (Some(13), Some(top - 13 - 1)));
    let depths: std::collections::BTreeSet<i32> = drawn.iter().map(|v| v.0[2]).collect();
    assert_eq!(depths.into_iter().collect::<Vec<_>>(), [0, 1]);
}

/// `rotation` turns the side seen a quarter at a time, clockwise seen from
/// above (90 its left side, the front on the left); `frame` picks a frame
/// from 0; any quarter turn counts, -90 as 270.
#[test]
fn frame_and_rotation_choose_what_is_seen() {
    for (options, want) in [
        ("", &["...r", "ywwy", "ywwy", "ywwy"][..]),
        ("{ rotation = 0 }", &["...r", "ywwy", "ywwy", "ywwy"]),
        ("{ rotation = 90 }", &["r..", "yrr", "yrr", "yrr"]),
        ("{ rotation = 450 }", &["r..", "yrr", "yrr", "yrr"]),
        ("{ rotation = 180 }", &["r...", "rwwg", "rwwg", "rwwg"]),
        ("{ rotation = 270 }", &["..r", "ggy", "ggy", "ggy"]),
        ("{ rotation = -90 }", &["..r", "ggy", "ggy", "ggy"]),
        ("{ frame = 1 }", &["bbbb", "bbbb", "bbbb", "bbbb"]),
        ("{ frame = 1, rotation = 90 }", &["bbb", "bbb", "bbb", "bbb"]),
    ] {
        let want: Vec<String> = want.iter().map(|r| r.to_string()).collect();
        let call = if options.is_empty() { "frame:draw(\"sign.vvx\", 4, 5)".to_string() } else { format!("frame:draw(\"sign.vvx\", 4, 5, {options})") };
        let dir = drawing("hud-sides", &call);
        let g = game_in(&dir);
        g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
        assert_eq!(halted(&g), "", "{call}");
        assert_eq!(seen(&g), want, "{call}");
    }
}

/// What `frame:draw` cannot take is a mistake with the script's line,
/// naming the word: a key it has not (no scale: the HUD has one size), a
/// rotation off the quarter turns, a quarter turn of the flat picture with
/// a pose or an outline, an outline's width out of its range or not whole,
/// missing or a word it has not, a see-through outline, a rotation's or a
/// pose's word it has not, a part the figure has
/// not, a pose of a model of no parts, a frame the model has not, a file
/// that is not there.
#[test]
fn what_frame_draw_cannot_take_is_a_mistake_naming_it() {
    for (call, want) in [
        ("frame:draw(\"sign.vvx\", 4, 5, { scale = 2 })", "main.lua:2: frame:draw: no 'scale' (it takes frame, rotation, pose, outline)"),
        ("frame:draw(\"sign.vvx\", 4, 5, { rotaton = 90 })", "frame:draw: no 'rotaton' (it takes frame, rotation, pose, outline)"),
        ("frame:draw(\"sign.vvx\", 4, 5, { rotation = 45 })", "frame:draw: rotation = 45: quarter turns only (0, 90, 180, 270); to turn it freely, a table: rotation = { rotation = 45 }"),
        ("frame:draw(\"chest.vvx\", 4, 5, { rotation = 90, pose = { lid = { pitch = 60 } } })", "frame:draw: rotation = 90 turns the flat picture; posed, the model is drawn whole and its rotation is a table, rotation = { rotation = 90 }"),
        ("frame:draw(\"sign.vvx\", 4, 5, { rotation = { yaw = 30 } })", "frame:draw: a rotation has rotation, pitch and roll in degrees ({ roll = 20 }), not 'yaw'"),
        ("frame:draw(\"chest.vvx\", 4, 5, { pose = { lidd = { pitch = 60 } } })", "frame:draw: chest.vvx has no part 'lidd' (its parts: body, lid)"),
        ("frame:draw(\"sign.vvx\", 4, 5, { pose = { lid = { pitch = 60 } } })", "frame:draw: sign.vvx is no figure of parts"),
        ("frame:draw(\"chest.vvx\", 4, 5, { pose = { lid = { pitch = \"up\" } } })", "frame:draw, part 'lid': the rotation's pitch is a number of degrees"),
        ("frame:draw(\"chest.vvx\", 4, 5, { rotation = 90, outline = { width = 1 } })", "frame:draw: rotation = 90 turns the flat picture; outlined, the model is drawn whole and its rotation is a table, rotation = { rotation = 90 }"),
        ("frame:draw(\"chest.vvx\", 4, 5, { outline = { width = 0 } })", "frame:draw: outline: width is a whole number of HUD voxels from 1 to 8"),
        ("frame:draw(\"chest.vvx\", 4, 5, { outline = { width = 9 } })", "frame:draw: outline: width is a whole number of HUD voxels from 1 to 8"),
        ("frame:draw(\"chest.vvx\", 4, 5, { outline = { width = 1.5 } })", "frame:draw: outline: width is a whole number of HUD voxels from 1 to 8"),
        ("frame:draw(\"chest.vvx\", 4, 5, { outline = { color = \"#101010\" } })", "frame:draw: outline: width is missing: how many HUD voxels thick, 1 to 8"),
        ("frame:draw(\"chest.vvx\", 4, 5, { outline = 1 })", "frame:draw: outline: a table, { width = 1, color = \"#101010\" }"),
        ("frame:draw(\"chest.vvx\", 4, 5, { outline = { width = 1, thick = 2 } })", "frame:draw: outline: no 'thick' (it takes width, color)"),
        ("frame:draw(\"chest.vvx\", 4, 5, { outline = { width = 1, color = { 0, 0, 0, 128 } } })", "an outline has no transparency"),
        ("frame:draw(\"nope.vvx\", 4, 5, { rotation = {} })", "main.lua:2: frame:draw: no model file \"nope.vvx\" in the game folder"),
        ("frame:draw(\"sign.vvx\", 4, 5, { rotation = 22.5 })", "frame:draw: rotation is a whole number"),
        ("frame:draw(\"sign.vvx\", 4, 5, { frame = 2 })", "frame:draw: frame = 2, but \"sign.vvx\" has 2 (from 0)"),
        ("frame:draw(\"sign.vvx\", 4, 5, { frame = -1 })", "frame:draw: frame = -1: its frames count from 0"),
        ("frame:draw(\"nope.vvx\", 4, 5)", "main.lua:2: frame:draw: no model file \"nope.vvx\" in the game folder"),
    ] {
        let dir = drawing("hud-bad", call);
        let g = game_in(&dir);
        g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
        assert!(halted(&g).contains(want), "{call}: {}", halted(&g));
    }
}

/// A model only drawn on the HUD is no part of the run: the states' hashes
/// and the fingerprint are the same drawn or not.
#[test]
fn a_model_on_the_hud_changes_no_run() {
    let dir = drawing("hud-run", "frame:draw(\"sign.vvx\", 4, 5, { rotation = 90 })");
    let run = |looking: bool| {
        let mut g = game_in(&dir);
        let mut hashes = Vec::new();
        for tick in 0..60 {
            g.step(&Input { right: tick % 20 < 10, ..Input::default() });
            if looking {
                g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
            }
            hashes.push(g.game.state_hash());
        }
        assert_eq!(halted(&g), "");
        (hashes, g.fingerprint().unwrap())
    };
    assert_eq!(run(true), run(false));
}

/// The models `frame:draw` drew whole on the HUD this picture, each voxel's
/// middle and colour as the GPU puts them.
fn whole_voxels(g: &Session<LuaGame>) -> Vec<Vec<(glam::Vec3, [u8; 4])>> {
    let f = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(halted(g), "");
    f.hud_models.iter().map(|m| f.bank.cubes(m).map(|c| (glam::Vec3::from_slice(&c.pos_size[..3]), c.color)).collect()).collect()
}

/// Each column and row of a picture of the HUD (from its bottom left, UI
/// voxels) and the colour nearest the eye there.
fn nearest(voxels: &[(glam::Vec3, [u8; 4])]) -> std::collections::BTreeMap<(i32, i32), [u8; 4]> {
    let mut front: std::collections::BTreeMap<(i32, i32), (f32, [u8; 4])> = std::collections::BTreeMap::new();
    for &(at, color) in voxels {
        let e = front.entry(((at.x - 0.5).round() as i32, (at.y - 0.5).round() as i32)).or_insert((f32::MIN, color));
        if at.z > e.0 {
            *e = (at.z, color);
        }
    }
    front.into_iter().map(|(k, (_, c))| (k, c)).collect()
}

/// A rotation given as a table draws the model whole, from the bank (the
/// flat form none): unturned, `{ rotation = {} }`, its voxels lie on the
/// very columns and rows of the flat picture at the same x, y, the one
/// nearest the eye in each the flat picture's colour, as deep as the
/// model, its middle (`render::turn_centre`) on the HUD's plane
/// (`c.ui.depth`); a quarter turn shows the side the flat form shows.
#[test]
fn a_whole_model_faces_you_where_the_flat_one_lies() {
    let flat = |options: &str| {
        let dir = drawing("hud-flat", &format!("frame:draw(\"sign.vvx\", 4, 5{options})"));
        let g = game_in(&dir);
        assert!(g.compose(0.0, 16.0 / 9.0, Scene::Play, None).hud_models.is_empty(), "the flat form draws nothing whole");
        sign_voxels(&g).into_iter().map(|(at, color)| ((at[0], at[1]), color)).collect::<std::collections::BTreeMap<_, _>>()
    };
    let whole = |rotation: &str| {
        let dir = drawing("hud-whole", &format!("frame:draw(\"sign.vvx\", 4, 5, {{ rotation = {rotation} }})"));
        let g = game_in(&dir);
        let mut models = whole_voxels(&g);
        assert_eq!(models.len(), 1, "one draw from the bank");
        models.remove(0)
    };
    let unturned = whole("{}");
    for (at, _) in &unturned {
        assert!((at.x - at.x.floor() - 0.5).abs() < 1e-4 && (at.y - at.y.floor() - 0.5).abs() < 1e-4, "on the grid: {at}");
    }
    assert_eq!(nearest(&unturned), flat(""), "the flat picture's columns, rows and colours");
    let depth = crate::config::UiConfig::default().depth as f32;
    let mut deep: Vec<i32> = unturned.iter().map(|(at, _)| (at.z - depth).round() as i32).collect();
    deep.sort();
    deep.dedup();
    assert_eq!(deep, [-1, 0, 1], "3 deep about the HUD's plane");
    assert_eq!(nearest(&whole("{ rotation = 90 }")), flat(", { rotation = 90 }"), "a quarter turn shows the side the flat form shows");
}

/// A whole model turns freely (its voxels leave the grid) and a figure of
/// parts poses: the lid opened on its hinge rises, the body stays.
#[test]
fn a_whole_model_turns_and_poses() {
    let whole = |options: &str| {
        let dir = drawing("hud-chest", &format!("frame:draw(\"chest.vvx\", 4, 5, {options})"));
        whole_voxels(&game_in(&dir))
    };
    let (at_rest, open) = (whole("{ rotation = {} }"), whole("{ pose = { lid = { pitch = 90 } } }"));
    assert_eq!((at_rest.len(), open.len()), (2, 2), "a draw a part");
    assert_eq!(at_rest[0], open[0], "the body stays");
    let top = |part: &[(glam::Vec3, [u8; 4])]| part.iter().map(|(at, _)| at.y).fold(f32::MIN, f32::max);
    assert!(top(&open[1]) > top(&at_rest[1]) + 1.5, "the lid opened stands up: {} against {}", top(&open[1]), top(&at_rest[1]));
    // Opened back on its hinge: no nearer the eye than it lay.
    let front = |part: &[(glam::Vec3, [u8; 4])]| part.iter().map(|(at, _)| at.z).fold(f32::MIN, f32::max);
    assert!(front(&open[1]) < front(&at_rest[1]) - 1.0, "back on its hinge: {} against {}", front(&open[1]), front(&at_rest[1]));
    let turned = whole("{ rotation = { rotation = 30, pitch = 10 } }");
    assert!(turned[0].iter().any(|(at, _)| (at.x - at.x.floor() - 0.5).abs() > 0.05), "turned off the grid");
    assert_ne!(turned[0], at_rest[0]);
}

/// Under a menu and while a mistake stops the game, nothing is drawn whole
/// on the HUD, as nothing else of it.
#[test]
fn no_whole_model_under_a_menu_or_a_mistake() {
    let dir = drawing("hud-hidden", "frame:draw(\"chest.vvx\", 4, 5, { rotation = {} })\n  if world.state.boom then error(\"boom\") end");
    let mut g = game_in(&dir);
    assert_eq!(g.compose(0.0, 16.0 / 9.0, Scene::Play, None).hud_models.len(), 2);
    let menu = crate::menu::Menu::pause();
    assert!(menu.is_open());
    assert!(g.menu_frame(&menu, &crate::settings::Settings::default(), 0.0, 16.0 / 9.0, Scene::Play).hud_models.is_empty(), "under a menu");
    said(&mut g, "world.state.boom = true");
    let f = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    assert!(halted(&g).contains("boom"), "{}", halted(&g));
    assert!(f.hud_models.is_empty(), "under the mistake");
}

/// On the GPU: a model drawn whole lies over the flat HUD drawn after it
/// (the flat form keeps its voxels against that box: the first drawn keeps
/// them); turned 30 degrees, its side shows beside its front.
#[test]
fn a_whole_model_lies_over_the_flat_hud() {
    let (w, h) = (1280, 720);
    let mut canvas = crate::headless::Canvas::new(w, h, false).expect("a GPU");
    // The chest across the HUD's middle column (320 across: 158 to 162),
    // the box behind it from 140, 70.
    let picture = |canvas: &mut crate::headless::Canvas, options: &str| {
        let dir = drawing("hud-gpu", &format!("frame:draw(\"chest.vvx\", 158, 80{options})\n  frame:box(140, 70, 40, 30, \"#2040ff\")"));
        let g = game_in(&dir);
        let f = g.compose(0.0, w as f32 / h as f32, Scene::Play, None);
        assert_eq!(halted(&g), "");
        canvas.draw(&f).expect("the picture drawn")
    };
    let chest = |p: &[u8], x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        p[i] as i32 > p[i + 2] as i32 + 30
    };
    // Its middle row, 4 pixels a UI voxel.
    let (x, y) = (w / 2, 82 * 4);
    let wide = |p: &[u8]| (0..w).filter(|&x| chest(p, x, y)).count();
    let flat = picture(&mut canvas, "");
    assert!(chest(&flat, x, y), "the flat form over the box drawn after it");
    let whole = picture(&mut canvas, ", { rotation = {} }");
    assert!(chest(&whole, x, y), "the whole one over it");
    let turned = picture(&mut canvas, ", { rotation = { rotation = 30 } }");
    assert!(chest(&turned, x, y));
    assert!(wide(&turned) > wide(&whole) + 3, "turned, its side shows: {} pixels across against {}", wide(&turned), wide(&whole));
}

/// On the GPU, the HUD's order against the models drawn whole: what is
/// drawn before the first of them lies over them all (a box drawn first
/// shows its color inside the chest's silhouette, turned and outlined),
/// what is drawn after lies under them.
#[test]
fn what_is_drawn_before_a_whole_model_lies_over_it() {
    let (w, h) = (1280, 720);
    let mut canvas = crate::headless::Canvas::new(w, h, false).expect("a GPU");
    let chest = "frame:draw(\"chest.vvx\", 158, 80, { rotation = { rotation = 30 }, outline = { width = 1 } })";
    let mut picture = |draw: &str| {
        let dir = drawing("hud-before", draw);
        let g = game_in(&dir);
        let f = g.compose(0.0, w as f32 / h as f32, Scene::Play, None);
        assert_eq!(halted(&g), "");
        canvas.draw(&f).expect("the picture drawn")
    };
    let blue = |p: &[u8], x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        p[i + 2] as i32 > p[i] as i32 + 60
    };
    // The chest's middle, 4 pixels a UI voxel; the box 2 by 2 over it.
    let (x, y) = (w / 2, 82 * 4);
    let boxed = "frame:box(159, 81, 2, 2, \"#2040ff\")";
    assert!(!blue(&picture(chest), x, y), "the chest alone");
    assert!(blue(&picture(&format!("{boxed}\n  {chest}")), x, y), "the box drawn before it over it");
    assert!(!blue(&picture(&format!("{chest}\n  {boxed}")), x, y), "the box drawn after it under it");
}

/// Only what is drawn before a model drawn whole is set apart for the pass
/// over the models: nothing without one (one list as ever), nothing where
/// nothing comes before it; and each set's faces are hidden by its own
/// cubes alone (a box drawn before, flush against one drawn after, keeps
/// the face they share: they are drawn in passes of their own).
#[test]
fn only_what_is_drawn_before_a_whole_model_lies_apart() {
    let front = |draw: &str| {
        let dir = drawing("hud-front", draw);
        let g = game_in(&dir);
        let f = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
        assert_eq!(halted(&g), "");
        f
    };
    let (before, after) = ("frame:box(100, 10, 4, 4, \"#2040ff\")", "frame:box(104, 10, 4, 4, \"#20ff40\")");
    let whole = "frame:draw(\"chest.vvx\", 20, 10, { rotation = {} })";
    let flat = front(&format!("{before}\n  frame:draw(\"chest.vvx\", 20, 10)\n  {after}"));
    assert!(flat.hud_front.is_empty() && flat.hud_models.is_empty() && !flat.ui.is_empty(), "no model drawn whole: one list");
    assert!(front(&format!("{whole}\n  {before}\n  {after}")).hud_front.is_empty(), "nothing before the model");
    let f = front(&format!("{before}\n  {whole}\n  {after}"));
    let cell = |i: &Instance| i.pos_size[0].floor() as i32;
    let covered = |i: &Instance, dir: usize| i.fx[crate::render::FX_COVERED] >> dir & 1 == 1;
    assert!(!f.hud_front.is_empty() && f.hud_front.iter().all(|i| i.color[..3] == [0x20, 0x40, 0xff]), "the box drawn before apart");
    let edge: Vec<&Instance> = f.hud_front.iter().filter(|i| cell(i) == 103).collect();
    assert!(!edge.is_empty() && edge.iter().all(|i| !covered(i, 0)), "its +x faces kept");
    assert!(f.hud_front.iter().any(|i| cell(i) == 102 && covered(i, 0)), "hidden within it");
    let edge: Vec<&Instance> = f.ui.iter().filter(|i| cell(i) == 104).collect();
    assert!(!edge.is_empty() && edge.iter().all(|i| !covered(i, 1)), "the box after keeps its -x faces");
}

/// `outline` rounds one model drawn whole alone, in its color (the game's,
/// `c.effects.outline_color`, unless given), as thick as said; on the GPU
/// the line rings it where nothing of it lies, and nothing else is
/// outlined: neither the other model drawn whole, nor the scene's.
#[test]
fn an_outline_rounds_one_hud_model_alone() {
    // A chest in the level too, drawn by the scene.
    let dir = drawing(
        "hud-outline",
        "frame:model(\"chest.vvx\", level.state.chest)\n  frame:draw(\"chest.vvx\", 20, 10, { rotation = { rotation = 20 }, outline = { width = 2, color = \"#00ff00\" } })\n  \
         frame:draw(\"chest.vvx\", 60, 10, { rotation = {}, outline = { width = 1 } })\n  frame:draw(\"chest.vvx\", 100, 10, { rotation = {} })",
    );
    let main = std::fs::read_to_string(dir.join("main.lua")).unwrap();
    std::fs::write(dir.join("main.lua"), main + "function on_level_start()\n  level.state.chest = level:spawn(\"miner\", 32, 32)\nend\n").unwrap();
    let g = game_in(&dir);
    let (w, h) = (1280, 720);
    let f = g.compose(0.0, w as f32 / h as f32, Scene::Play, None);
    assert_eq!(halted(&g), "");
    let lines: Vec<Option<[f32; 4]>> = f.hud_models.iter().map(|m| m.outline).collect();
    let [r, gr, b] = g.cfg.effects.outline_color;
    assert_eq!(lines, [Some([0.0, 1.0, 0.0, 2.0]); 2].into_iter().chain([Some([r, gr, b, 1.0]); 2]).chain([None; 2]).collect::<Vec<_>>(), "each part of a chest its draw's line");
    assert!(!f.models.is_empty() && f.models.iter().all(|m| m.outline.is_none()), "the scene's models none of their own");
    let mut canvas = crate::headless::Canvas::new(w, h, false).expect("a GPU");
    let pixels = canvas.draw(&f).expect("the picture drawn");
    let green = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        pixels[i + 1] > 200 && pixels[i] < 60 && pixels[i + 2] < 60
    };
    // 4 pixels a UI voxel: the green chest from 20, 10, 4 across and high.
    let ring: Vec<(u32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| green(x, y)).collect();
    assert!(ring.len() > 100, "the line drawn: {} pixels", ring.len());
    assert!(ring.iter().all(|&(x, y)| (40..200).contains(&x) && (0..140).contains(&y)), "only round the one chest");
    // Not over it: its middle is the chest's own.
    assert!(!green(22 * 4, 12 * 4));
}

/// On the GPU: a model's own outline is as wide on every side wherever it
/// stands, in the screen's corner as in its middle (grown into depth too,
/// the part of it nearer the eye was magnified and slid away from the
/// vanishing point: three times as wide on the outer side at width 4),
/// and as wide as said, 6 pixels a HUD voxel at 1920 by 1080.
#[test]
fn an_own_outline_is_as_wide_on_every_side_anywhere() {
    let (w, h) = (1920u32, 1080u32);
    let mut canvas = crate::headless::Canvas::new(w, h, false).expect("a GPU");
    let mut uneven = Vec::new();
    for (x, y) in [(20, 20), (158, 88), (290, 150)] {
        for width in [1, 2, 4] {
            let dir = drawing("hud-ring", &format!("frame:draw(\"chest.vvx\", {x}, {y}, {{ rotation = {{}}, outline = {{ width = {width}, color = \"#00ff00\" }} }})"));
            let g = game_in(&dir);
            let f = g.compose(0.0, w as f32 / h as f32, Scene::Play, None);
            assert_eq!(halted(&g), "");
            let pixels = canvas.draw(&f).expect("the picture drawn");
            let green = |x: u32, y: u32| {
                let i = ((y * w + x) * 4) as usize;
                pixels[i + 1] > 200 && pixels[i] < 60 && pixels[i + 2] < 60
            };
            let ring: Vec<(u32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| green(x, y)).collect();
            let (x0, x1) = (ring.iter().map(|p| p.0).min().unwrap(), ring.iter().map(|p| p.0).max().unwrap());
            let (y0, y1) = (ring.iter().map(|p| p.1).min().unwrap(), ring.iter().map(|p| p.1).max().unwrap());
            let (mx, my) = ((x0 + x1) / 2, (y0 + y1) / 2);
            // The green run from each edge of the ring inwards, through its middle.
            let run = |from: u32, step: i32, along_x: bool| {
                let mut n = 0;
                let mut at = from as i32;
                while if along_x { green(at as u32, my) } else { green(mx, at as u32) } {
                    n += 1;
                    at += step;
                }
                n as f32
            };
            let sides = [run(x0, 1, true), run(x1, -1, true), run(y0, 1, false), run(y1, -1, false)];
            let said = width as f32 * 6.0;
            if sides.iter().any(|side| (side - said).abs() > 1.5) {
                uneven.push(format!("at {x}, {y}, width {width}: left, right, top, bottom {sides:?} pixels, {said} said"));
            }
        }
    }
    assert!(uneven.is_empty(), "{uneven:#?}");
}

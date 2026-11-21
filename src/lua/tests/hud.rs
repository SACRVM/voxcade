//! A model drawn on the HUD (`frame:draw`): as a grid's slot shows it, one
//! of its voxels one of the HUD's, anywhere; only the picture.

use super::*;
use crate::lua::world::{Cmd, Thumb};

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

/// A game whose `on_draw` is `draw`, with the sign beside it.
fn drawing(name: &str, draw: &str) -> crate::scratch::Scratch {
    let dir = folder(name, &format!("function on_draw(frame)\n  {draw}\nend\n"));
    std::fs::write(dir.join("sign.vvx"), SIGN).unwrap();
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
/// rotation off the quarter turns, a frame the model has not, a file that
/// is not there.
#[test]
fn what_frame_draw_cannot_take_is_a_mistake_naming_it() {
    for (call, want) in [
        ("frame:draw(\"sign.vvx\", 4, 5, { scale = 2 })", "main.lua:2: frame:draw: no 'scale' (it takes frame, rotation)"),
        ("frame:draw(\"sign.vvx\", 4, 5, { rotaton = 90 })", "frame:draw: no 'rotaton' (it takes frame, rotation)"),
        ("frame:draw(\"sign.vvx\", 4, 5, { rotation = 45 })", "frame:draw: rotation = 45: quarter turns only (0, 90, 180, 270)"),
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

//! The editor window driven through its own events, without a window
//! (`EditorWindow::headless`): clicks on rows, drags in the view, the
//! wheel, SELECT's rectangle, a row dragged onto another.

use super::*;
use crate::editor::big;

/// A figure like the owner's: hips; a head on them turned 45; a nose
/// on the head turned -45; hair on the head, its box far out in front
/// and its joint set by hand to many digits; its box stated far too
/// big (a part flew off once and came back). An editor window without a
/// window on it.
pub(in crate::editor) fn owners_like(name: &str) -> (EditorWindow, crate::scratch::Scratch) {
    let dir = crate::scratch::Scratch::new(&format!("window-{name}"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    let part = |name: &str, at: [i32; 3], size: [i32; 3], sign: char, extra: &str| {
        let rows: Vec<String> = (0..size[2]).map(|_| (0..size[1]).map(|_| sign.to_string().repeat(size[0] as usize)).collect::<Vec<_>>().join("\n")).collect();
        format!("\n[[part]]\nname = \"{name}\"\nat = [{}, {}, {}]\n{extra}layers = \"\"\"\n{}\n\"\"\"\n", at[0], at[1], at[2], rows.join("\n\n"))
    };
    let text = format!(
        "format = \"{}\"\nsize = [13, 2001, 42]\n\n[palette]\na = \"#5a7aa8\"\nb = \"#d0c8b8\"\nc = \"#c85a4a\"\nd = \"#6b4423\"\n{}{}{}{}",
        crate::vvx::FORMAT,
        part("hips", [-3, -2, 0], [6, 4, 8], 'a', ""),
        part("head", [-2, -2, 8], [4, 4, 4], 'b', "parent = \"hips\"\njoint = [0, 0, 8]\nrotation = 45\n"),
        part("hair", [-2, -12, 10], [4, 3, 2], 'd', "parent = \"head\"\njoint = [0.5, -15.24295, 38.045776]\n"),
        part("nose", [0, -4, 9], [1, 2, 1], 'c', "parent = \"head\"\nrotation = -45\n"),
    );
    std::fs::write(dir.join("fig.vvx"), text).unwrap();
    let (ed, view, cfg) = open_model::<crate::Voxcade>(&dir, "fig.vvx").unwrap();
    let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
    for _ in 0..3 {
        win.draw().unwrap();
    }
    (win, dir)
}

fn file(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("fig.vvx")).unwrap()
}

/// The camera as it looks: its turn, tilt and distance, and its middle
/// from the model's pivot (the grid shown moves with the part chosen).
fn camera(win: &EditorWindow) -> (f32, f32, f32, glam::Vec3) {
    let o = win.view.block;
    (o.yaw, o.pitch, o.zoom, o.target + win.ed.origin().unwrap_or(glam::IVec3::ZERO).as_vec3())
}

/// The window's events (`big.rs`'s), each followed by a frame: the
/// pointer moved there (`held`: the buttons the motion says are down,
/// SDL's bits), a button down and up.
fn motion(win: &mut EditorWindow, x: f32, y: f32, held: u32) {
    thread_local!(static LAST: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((800.0, 450.0)) });
    let mut last = LAST.with(|l| l.get());
    big::motion(win, x, y, held, &mut last);
    LAST.with(|l| l.set(last));
    win.draw().unwrap();
}

fn button(win: &mut EditorWindow, x: f32, y: f32, button: sdl3::mouse::MouseButton, down: bool) {
    big::button(win, x, y, button, down);
    win.draw().unwrap();
}

fn click(win: &mut EditorWindow, at: egui::Pos2) {
    motion(win, at.x, at.y, 0);
    button(win, at.x, at.y, sdl3::mouse::MouseButton::Left, true);
    button(win, at.x, at.y, sdl3::mouse::MouseButton::Left, false);
    // (The panels' clock past a double click's time: the next click
    // is a click of its own.)
    win.input.later(std::time::Duration::from_millis(350));
    win.draw().unwrap();
}

fn row(win: &EditorWindow, name: &str) -> egui::Pos2 {
    win.rows.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no row {name}: {:?}", win.rows)).1.center()
}

fn chosen(win: &EditorWindow) -> String {
    let f = win.ed.figure.as_ref().unwrap();
    f.selected.map_or(String::new(), |p| f.parts()[p].name.clone())
}

/// TERRAIN in the window: a point under the mouse lit, pressed and
/// dragged up through the window's own events: the land up there as
/// the mouse goes, written at the let-go as one step; Escape lets go
/// of the choice.
#[test]
fn a_terrain_point_dragged_in_the_window_is_one_step() {
    let dir = crate::scratch::Scratch::new("window-land");
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    let rows = ["4 4 4 4 4 4 4 4 4 4"; 6].join("\n");
    std::fs::write(dir.join("levels/land.toml"), format!("format = \"voxcade level 1\"\nsize = [160, 96, 80]\n\n[terrain]\nspacing = 16\nheights = \"\"\"\n{rows}\n\"\"\"\n\n[[terrain.material]]\ncolor = \"#5c8a3a\"\n")).unwrap();
    let (ed, view, cfg) = open::<crate::Voxcade>(&dir, Some("levels/land.toml"), None, false).unwrap();
    let mut win = EditorWindow::headless(ed, view, cfg, &dir).unwrap();
    view::act(&mut win.ed, &mut win.view, view::Action::Layer(Layer::Terrain));
    for _ in 0..3 {
        win.draw().unwrap();
    }
    let start = std::fs::read_to_string(dir.join("levels/land.toml")).unwrap();
    let p = glam::IVec2::new(4, 3);
    let vp = win.view.view_proj(&win.view.level, 1600.0 / 900.0);
    let at = win.ed.points_on_screen(vp, glam::Vec2::new(1600.0, 900.0)).into_iter().find(|(q, _)| *q == p).unwrap().1;
    motion(&mut win, at.x, at.y, 0);
    assert_eq!(win.ed.land.hover, Some(p), "lit under the mouse");
    button(&mut win, at.x, at.y, sdl3::mouse::MouseButton::Left, true);
    for k in 1..=6 {
        motion(&mut win, at.x, at.y - 10.0 * k as f32, 1);
    }
    let t = win.ed.terrain().unwrap();
    let raised = t.heights[t.index(p).unwrap()];
    assert!(raised > 10, "up as the mouse goes: {raised}");
    let now = win.ed.points_on_screen(vp, glam::Vec2::new(1600.0, 900.0)).into_iter().find(|(q, _)| *q == p).unwrap().1;
    assert!((now.y - (at.y - 60.0)).abs() < 4.0, "the point stays under the pointer: {now} for {}", at.y - 60.0);
    // Shift held: a third as far.
    let e = sdl3::event::Event::MouseMotion { timestamp: 0, window_id: 0, which: 0, mousestate: sdl3::mouse::MouseState::from_sdl_state(1), x: at.x, y: at.y - 60.0, xrel: 0.0, yrel: 0.0 };
    assert!(win.handle(&e, true));
    let t = win.ed.terrain().unwrap();
    let fine = t.heights[t.index(p).unwrap()];
    assert!((fine - 4 - (raised - 4) / 3).abs() <= 1, "fine: {fine} for {raised}");
    assert_eq!(std::fs::read_to_string(dir.join("levels/land.toml")).unwrap(), start, "nothing written while dragging");
    button(&mut win, at.x, at.y - 60.0, sdl3::mouse::MouseButton::Left, false);
    assert_eq!(win.ed.steps(), 1, "one step");
    assert!(std::fs::read_to_string(dir.join("levels/land.toml")).unwrap().contains(&format!(" {fine} ")));
    assert_eq!(win.ed.land.chosen, Some(p));
    let esc = sdl3::event::Event::KeyDown { timestamp: 0, window_id: 0, keycode: Some(sdl3::keyboard::Keycode::Escape), scancode: None, keymod: sdl3::keyboard::Mod::NOMOD, repeat: false, which: 0, raw: 0 };
    assert!(win.handle(&esc, false));
    assert_eq!(win.ed.land.chosen, None);
}

/// GAME VIEW on the owner's figure: the figure fills the view from the
/// game's angle; the wheel and the right button zoom and turn it and
/// GAME VIEW stays on; pressed again, the editor's look as it was.
#[test]
fn game_view_frames_the_figure_and_the_wheel_and_right_button_work() {
    let (mut win, _dir) = owners_like("game-view");
    let before = win.view.block;
    view::act(&mut win.ed, &mut win.view, view::Action::GameView);
    win.draw().unwrap();
    let game = win.view.game.clone();
    let framed = win.view.block;
    assert_eq!((framed.yaw, framed.pitch, framed.fov), (game.yaw, game.pitch, game.fov));
    assert!(framed.zoom < 100.0 && framed.zoom < game.view_width, "framed by the figure, not the game's width: {}", framed.zoom);
    let wheel = sdl3::event::Event::MouseWheel { timestamp: 0, window_id: 0, which: 0, x: 0.0, y: 1.0, direction: sdl3::mouse::MouseWheelDirection::Normal, mouse_x: 800.0, mouse_y: 450.0, integer_x: 0, integer_y: 1 };
    assert!(win.handle(&wheel, false));
    win.draw().unwrap();
    assert!(win.view.block.zoom < framed.zoom, "the wheel zooms");
    motion(&mut win, 800.0, 450.0, 0);
    button(&mut win, 800.0, 450.0, sdl3::mouse::MouseButton::Right, true);
    motion(&mut win, 860.0, 470.0, 0b100);
    button(&mut win, 860.0, 470.0, sdl3::mouse::MouseButton::Right, false);
    assert!(win.view.block.yaw != framed.yaw || win.view.block.pitch != framed.pitch, "the right button turns");
    assert!(win.view.before_game.is_some(), "and GAME VIEW stays on");
    view::act(&mut win.ed, &mut win.view, view::Action::GameView);
    assert!(win.view.before_game.is_none());
    assert_eq!(win.view.block, before, "off: the editor's look");
}

/// The owner's case through the window's own events: clicks on the
/// parts' rows (the nose, the head, the nose again, the hair, the
/// figure whole) each choose that row, change nothing of the file and
/// make no step, and the camera stays as it looks; so do motions that
/// carry buttons held from elsewhere (a press on a panel, a release
/// lost). Opening frames the figure by its voxels, not its stated box.
#[test]
fn clicks_on_the_rows_choose_and_change_nothing() {
    let (mut win, dir) = owners_like("rows");
    let start = file(&dir);
    let cam = camera(&win);
    assert!(win.view.block.zoom < 100.0, "framed by its voxels, not a box 2001 deep: {}", win.view.block.zoom);
    for name in ["nose", "head", "nose", "hair", "", "nose"] {
        let at = row(&win, name);
        click(&mut win, at);
        assert_eq!(chosen(&win), name, "a click on the row {name:?} chooses it");
        assert_eq!((file(&dir), win.ed.steps()), (start.clone(), 0), "after the row {name:?}: the file as it was, no step");
        let now = camera(&win);
        assert_eq!((now.0, now.1, now.2), (cam.0, cam.1, cam.2), "after the row {name:?}: the camera turns nowhere");
        assert!((now.3 - cam.3).length() < 1e-3, "after the row {name:?}: its middle stays: {:?} {:?}", now.3, cam.3);
    }
    // Motions carrying the left and right buttons held from elsewhere,
    // across the view: nothing turns, nothing moves.
    for (x, y) in [(600.0, 300.0), (900.0, 500.0), (700.0, 700.0)] {
        motion(&mut win, x, y, 0b101);
    }
    // A press on a row, the pointer dragged on into the view, let go.
    let at = row(&win, "head");
    motion(&mut win, at.x, at.y, 0);
    button(&mut win, at.x, at.y, sdl3::mouse::MouseButton::Left, true);
    for (x, y) in [(400.0, 400.0), (700.0, 450.0)] {
        motion(&mut win, x, y, 1);
    }
    button(&mut win, 700.0, 450.0, sdl3::mouse::MouseButton::Left, false);
    let now = camera(&win);
    assert_eq!((now.0, now.1, now.2), (cam.0, cam.1, cam.2), "the camera turns nowhere");
    assert_eq!((file(&dir), win.ed.steps()), (start, 0), "nothing changed");
}

/// Where a part's voxels show on the window (points), as drawn now.
fn part_on_screen(win: &EditorWindow, name: &str) -> Vec<glam::Vec2> {
    let f = win.ed.figure.as_ref().unwrap();
    let p = f.parts().iter().position(|o| o.name == name).unwrap();
    let vp = win.view.view_proj(&win.view.block, 1600.0 / 900.0);
    let (lo, _) = win.ed.figure_box().unwrap();
    let map = f.rest_map(p, lo);
    f.model.parts[p]
        .voxels
        .iter()
        .map(|&k| {
            let c = vp * map.transform_point3(f.model.frames[0][k as usize].0.as_vec3() + 0.5).extend(1.0);
            glam::Vec2::new((c.x / c.w + 1.0) / 2.0 * 1600.0, (1.0 - c.y / c.w) / 2.0 * 900.0)
        })
        .collect()
}

/// The box about points on the screen, a few points wider.
fn about(points: &[glam::Vec2]) -> (glam::Vec2, glam::Vec2) {
    let (lo, hi) = points.iter().fold((glam::Vec2::MAX, glam::Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    (lo - 3.0, hi + 3.0)
}

/// SELECT's rectangle dragged from `a` to `b`; `held` the modifiers'
/// keys first (Ctrl: ADD, Alt: REMOVE).
fn marquee(win: &mut EditorWindow, a: glam::Vec2, b: glam::Vec2) {
    motion(win, a.x, a.y, 0);
    button(win, a.x, a.y, sdl3::mouse::MouseButton::Left, true);
    for k in 1..=4 {
        let p = a + (b - a) * k as f32 / 4.0;
        motion(win, p.x, p.y, 1);
    }
    button(win, b.x, b.y, sdl3::mouse::MouseButton::Left, false);
}

/// The parts the selection holds voxels of, and how many.
fn selected(win: &EditorWindow) -> (Vec<String>, usize) {
    let Some(s) = win.ed.selection() else { return (Vec::new(), 0) };
    let m = win.ed.model.as_ref().unwrap();
    let names = s.cells.keys().filter_map(|o| match o {
        model::selection::Owner::Frame(f) => m.parts.iter().find(|p| p.frame == *f).map(|p| p.name.clone()),
        model::selection::Owner::Block(_) => None,
    });
    let mut names: Vec<String> = names.collect();
    names.sort();
    (names, s.count())
}

/// SELECT is a rectangle on the screen: over the head and the nose
/// with the figure whole chosen it takes from both, with the head
/// chosen from the head alone, a hidden part never; THROUGH takes the
/// voxels behind too; ADD and REMOVE grow and shrink it; NEW PART,
/// REMOVE and PAINT from it, each undone byte for byte.
#[test]
fn select_is_a_rectangle_on_the_screen() {
    let (mut win, dir) = owners_like("marquee");
    let start = file(&dir);
    view::act(&mut win.ed, &mut win.view, view::Action::Tool(model::Tool::Select));
    win.draw().unwrap();
    let mut both = part_on_screen(&win, "head");
    both.extend(part_on_screen(&win, "nose"));
    let (a, b) = about(&both);
    marquee(&mut win, a, b);
    let (names, _) = selected(&win);
    assert!(names.contains(&"head".to_string()) && names.contains(&"nose".to_string()), "from both: {names:?}");
    // The nose hidden: never.
    let nose = win.ed.figure.as_ref().unwrap().parts().iter().position(|o| o.name == "nose").unwrap();
    view::act(&mut win.ed, &mut win.view, view::Action::Hide(nose));
    marquee(&mut win, a, b);
    assert!(!selected(&win).0.contains(&"nose".to_string()), "a hidden part never");
    view::act(&mut win.ed, &mut win.view, view::Action::Hide(nose));
    // THROUGH: the head's voxels behind too; off: only those seen.
    let (h0, h1) = about(&part_on_screen(&win, "head"));
    marquee(&mut win, h0, h1);
    let through = selected(&win).1;
    view::act(&mut win.ed, &mut win.view, view::Action::SelectThrough);
    marquee(&mut win, h0, h1);
    let seen = selected(&win).1;
    assert!(seen < through && seen > 0, "seen {seen} of {through}");
    view::act(&mut win.ed, &mut win.view, view::Action::SelectThrough);
    // With the head chosen: the head's alone.
    let head = win.ed.figure.as_ref().unwrap().parts().iter().position(|o| o.name == "head").unwrap();
    win.ed.choose_part(head);
    win.view.follow_origin(&win.ed);
    win.draw().unwrap();
    let (a, b) = about(&both);
    marquee(&mut win, a, b);
    assert_eq!(selected(&win).0, ["head"], "a part chosen: its own alone");
    // ADD and REMOVE: a half, the other half added, the first taken out.
    let mid = (h0 + h1) / 2.0;
    marquee(&mut win, h0, glam::Vec2::new(mid.x, h1.y));
    let left = selected(&win).1;
    view::act(&mut win.ed, &mut win.view, view::Action::SelectMode(model::selection::SelectMode::Add));
    marquee(&mut win, glam::Vec2::new(mid.x, h0.y), h1);
    let all = selected(&win).1;
    assert!(all > left, "ADD: more ({all} after {left})");
    view::act(&mut win.ed, &mut win.view, view::Action::SelectMode(model::selection::SelectMode::Remove));
    marquee(&mut win, h0, glam::Vec2::new(mid.x, h1.y));
    let right = selected(&win).1;
    assert!(right < all && right > 0, "REMOVE: fewer ({right} of {all})");
    view::act(&mut win.ed, &mut win.view, view::Action::SelectMode(model::selection::SelectMode::New));
    assert_eq!((file(&dir), win.ed.steps()), (start.clone(), 0), "selecting changes nothing");
    // NEW PART, REMOVE, PAINT: each a step, undone byte for byte.
    for action in [view::Action::SelNewPart, view::Action::SelRemove, view::Action::SelPaint] {
        marquee(&mut win, h0, h1);
        assert!(win.ed.selection().is_some(), "{action:?}: selected");
        view::act(&mut win.ed, &mut win.view, action);
        assert_ne!(file(&dir), start, "{action:?}: done");
        win.ed.undo();
        assert_eq!(file(&dir), start, "{action:?}: undone byte for byte");
        win.ed.choose_part(win.ed.figure.as_ref().unwrap().parts().iter().position(|o| o.name == "head").unwrap());
        win.view.follow_origin(&win.ed);
        win.draw().unwrap();
    }
}

/// Where every voxel of the figure is drawn (from the pivot, to a
/// thousandth), each with its color.
fn drawn(win: &EditorWindow) -> std::collections::BTreeSet<([i64; 3], [u8; 4])> {
    let f = win.ed.figure.as_ref().unwrap();
    let shift = f.low;
    let mut out = std::collections::BTreeSet::new();
    for p in 0..f.parts().len() {
        let map = f.rest_map(p, glam::IVec3::ZERO);
        for &k in &f.model.parts[p].voxels {
            let (at, c) = f.model.frames[0][k as usize];
            let v = map.transform_point3(at.as_vec3() + 0.5) + shift.as_vec3();
            out.insert(((v * 1000.0).round().as_i64vec3().to_array(), c));
        }
    }
    out
}

/// A part's row dragged onto another's hangs it there where it stands:
/// every voxel drawn where it was, only its own lines in the file
/// changed, one step, undone byte for byte; onto its own child or the
/// figure whole refused; a click on a row still only chooses.
#[test]
fn a_row_dragged_onto_another_hangs_the_part_there() {
    let (mut win, dir) = owners_like("reparent");
    let start = file(&dir);
    let before = drawn(&win);
    let (from, to) = (row(&win, "nose"), row(&win, "hips"));
    motion(&mut win, from.x, from.y, 0);
    button(&mut win, from.x, from.y, sdl3::mouse::MouseButton::Left, true);
    for k in 1..=6 {
        let p = from + (to - from) * k as f32 / 6.0;
        motion(&mut win, p.x, p.y, 1);
    }
    button(&mut win, to.x, to.y, sdl3::mouse::MouseButton::Left, false);
    win.draw().unwrap();
    let now = file(&dir);
    assert!(now.contains("name = \"nose\"") && now.split("name = \"nose\"").nth(1).unwrap().split("[[part]]").next().unwrap().contains("parent = \"hips\""), "the nose hangs on the hips: {now}");
    assert_eq!(win.ed.steps(), 1, "one step");
    // (The head of the file aside: the first step makes `size` tight.)
    let others = |t: &str| t.split("[[part]]").skip(1).filter(|p| !p.contains("name = \"nose\"")).map(str::to_string).collect::<Vec<_>>();
    assert_eq!(others(&now), others(&start), "the others' text untouched");
    let after = drawn(&win);
    let near = |a: &std::collections::BTreeSet<([i64; 3], [u8; 4])>, b: &std::collections::BTreeSet<([i64; 3], [u8; 4])>| a.len() == b.len() && a.iter().all(|(p, c)| b.iter().any(|(q, d)| c == d && (0..3).all(|i| (p[i] - q[i]).abs() <= 2)));
    assert!(near(&before, &after), "every voxel drawn where it was");
    win.ed.undo();
    assert_eq!(file(&dir), start, "undone byte for byte");
    // The hair (unturned, on the head turned 45) onto the hips: it
    // takes the head's turn as its own rest, drawn where it was.
    let (from, to) = (row(&win, "hair"), row(&win, "hips"));
    motion(&mut win, from.x, from.y, 0);
    button(&mut win, from.x, from.y, sdl3::mouse::MouseButton::Left, true);
    for k in 1..=6 {
        let p = from + (to - from) * k as f32 / 6.0;
        motion(&mut win, p.x, p.y, 1);
    }
    button(&mut win, to.x, to.y, sdl3::mouse::MouseButton::Left, false);
    win.draw().unwrap();
    let hair = file(&dir).split("name = \"hair\"").nth(1).unwrap().split("[[part]]").next().unwrap().to_string();
    assert!(hair.contains("parent = \"hips\"") && hair.contains("rotation = 45\n"), "{hair}");
    assert!(near(&before, &drawn(&win)), "the hair drawn where it was");
    win.ed.undo();
    assert_eq!(file(&dir), start);
    // Refused: the head onto the nose (its own child), onto the whole.
    let (head, nose) = (row(&win, "head"), row(&win, "nose"));
    for (a, b) in [(head, nose), (nose, row(&win, ""))] {
        motion(&mut win, a.x, a.y, 0);
        button(&mut win, a.x, a.y, sdl3::mouse::MouseButton::Left, true);
        for k in 1..=6 {
            let p = a + (b - a) * k as f32 / 6.0;
            motion(&mut win, p.x, p.y, 1);
        }
        button(&mut win, b.x, b.y, sdl3::mouse::MouseButton::Left, false);
        assert_eq!(file(&dir), start, "refused");
        assert!(win.ed.hint.is_some(), "said why");
    }
    // A click on a row still only chooses.
    let at = row(&win, "nose");
    click(&mut win, at);
    assert_eq!((chosen(&win), file(&dir)), ("nose".to_string(), start));
}

/// POINTER's clicks on parts in the view choose them and change
/// nothing; TRIM changes only `size` (the joint written by hand to its
/// last digit stays); a LOCAL move of the nose changes only the nose.
#[test]
fn pointer_trim_and_a_local_move_change_only_their_own() {
    let (mut win, dir) = owners_like("pointer");
    let start = file(&dir);
    // A click on the head where it shows.
    let aspect = 1600.0 / 900.0;
    let vp = win.view.view_proj(&win.view.block, aspect);
    let head = win.ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == "head").unwrap();
    let f = win.ed.figure.as_ref().unwrap();
    let k = f.model.parts[head].visible[0] as usize;
    let middle = f.rest_map(head, win.ed.figure_box().unwrap().0).transform_point3(f.model.frames[0][k].0.as_vec3() + 0.5);
    let p = vp * middle.extend(1.0);
    let at = egui::pos2((p.x / p.w + 1.0) / 2.0 * 1600.0, (1.0 - p.y / p.w) / 2.0 * 900.0);
    click(&mut win, at);
    assert_eq!((chosen(&win), file(&dir), win.ed.steps()), ("head".to_string(), start.clone(), 0), "POINTER: the head chosen, nothing changed");
    // TRIM: only the size line.
    win.ed.back_to_level();
    win.ed.trim();
    let trimmed = file(&dir);
    let changed: Vec<(&str, &str)> = start.lines().zip(trimmed.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(changed.len(), 1, "only one line: {changed:?}");
    assert!(changed[0].0.starts_with("size") && trimmed.contains("joint = [0.5, -15.24295, 38.045776]"), "{changed:?}");
    // A LOCAL move of the nose: the nose's lines only.
    let nose = win.ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == "nose").unwrap();
    win.ed.choose_part(nose);
    win.ed.move_part_by(glam::Vec3::new(1.0, 0.0, 0.0));
    let moved = file(&dir);
    let others = |t: &str| t.split("[[part]]").filter(|p| !p.contains("name = \"nose\"")).map(str::to_string).collect::<Vec<_>>();
    assert_eq!(others(&moved), others(&trimmed), "only the nose's text changed");
}

//! Clones in the voxel editor (`like`, `mirror`): a stroke on a clone
//! lands on its source and shows on every clone; CLONE, CLONE MIRRORED,
//! MAKE UNIQUE, LIKE, REMOVE of a source, RENAME; UNDO byte for byte.

use std::collections::{BTreeMap, BTreeSet};

use glam::IVec3;

use super::{Editor, Hit, Tool};
use crate::editor::view::{Action, View, act};

fn car(name: &str) -> (Editor, View, crate::scratch::Scratch) {
    let dir = crate::scratch::Scratch::new(&format!("links-{name}"));
    std::fs::write(dir.join("car.vvx"), crate::vvx::tests::car_text()).unwrap();
    let mut ed = Editor::open_model(&dir, "car.vvx").unwrap();
    ed.follow_figure();
    let view = View::new(&ed, &crate::config::GameConfig::default());
    (ed, view, dir)
}

fn text(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("car.vvx")).unwrap()
}

/// Each part's voxels from the pivot with their colours, as the file says.
type Cells = BTreeMap<String, BTreeSet<([i32; 3], [u8; 4])>>;

fn cells(dir: &std::path::Path) -> Cells {
    let (shape, _) = crate::vvx::read_shape(&text(dir)).unwrap();
    shape.pieces.iter().map(|p| (p.name.clone(), p.voxels.iter().map(|(v, m)| (v.to_array(), m.rgba)).collect())).collect()
}

/// Each part's link as the file says.
fn links(dir: &std::path::Path) -> BTreeMap<String, (Option<String>, Option<usize>)> {
    let (shape, _) = crate::vvx::read_shape(&text(dir)).unwrap();
    shape.pieces.iter().map(|p| (p.name.clone(), (p.like.clone(), p.mirror))).collect()
}

fn part(ed: &Editor, name: &str) -> usize {
    crate::editor::model::parts::tests::part(ed, name)
}

/// A voxel added on a clone at a cell of its own grid (its face `normal`).
fn add_on(ed: &mut Editor, view: &mut View, name: &str, cell: IVec3, normal: IVec3) {
    let p = part(ed, name);
    act(ed, view, Action::Part(p));
    ed.choose(Tool::Add);
    ed.colour = [255, 0, 0];
    ed.press(Hit { cell, normal });
    ed.release();
}

const RED: [u8; 4] = [255, 0, 0, 255];

/// A stroke on a clone lands on its source through the mirror and shows on
/// every clone at once; the clone stays a clone in the file (no layers of
/// its own); UNDO gives the file back byte for byte.
#[test]
fn a_stroke_on_a_clone_lands_on_its_source() {
    let (mut ed, mut view, dir) = car("stroke");
    let before = text(&dir);
    let was = cells(&dir);
    // body_right's local (1, 0, 1) is body_left's (-1, -2, 2) mirrored: a
    // voxel on top of it.
    add_on(&mut ed, &mut view, "body_right", IVec3::new(1, 0, 1), IVec3::Z);
    let now = cells(&dir);
    assert!(now["body_left"].contains(&([-1, -2, 3], RED)), "on the source, mirrored: {:?}", now["body_left"]);
    assert!(now["body_right"].contains(&([1, -2, 3], RED)));
    let t = text(&dir);
    assert!(t.contains("name = \"body_right\"\nlike = \"body_left\"\nmirror = \"x\"\nat = [0, -2, 1]\n\n"), "still a clone: {t}");
    // Nothing else moved: before and after differ by the stroke alone.
    for (name, cells) in &was {
        assert!(cells.is_subset(&now[name]), "{name} lost a voxel");
        assert!(now[name].len() - cells.len() == usize::from(name.starts_with("body")), "{name}: {:?}", now[name].difference(cells).collect::<Vec<_>>());
    }
    // The editor's figure is what the file reads as.
    ed.follow_figure();
    let figure = &ed.figure.as_ref().unwrap().model;
    let d = crate::vvx::differences(figure, &crate::vvx::read(&t).unwrap());
    assert!(d.is_empty(), "{d:?}");
    ed.undo();
    assert_eq!(text(&dir), before, "UNDO: byte for byte");
    ed.redo();
    assert_eq!(text(&dir), t);
}

/// A clone grown past its box: the source grows on the side the mirror
/// takes it to, each clone on its own side, every old voxel where it was.
#[test]
fn a_clone_grown_past_its_box_keeps_every_clone_standing() {
    let (mut ed, mut view, dir) = car("grow");
    let was = cells(&dir);
    // wheel_fr (mirrored, at [3, 2, 0]): its local (1, 0, 0) is filled; a
    // voxel to its right, past its box and the figure's.
    add_on(&mut ed, &mut view, "wheel_fr", IVec3::new(1, 0, 0), IVec3::X);
    let now = cells(&dir);
    let new = |name: &str| now[name].difference(&was[name]).copied().collect::<Vec<_>>();
    assert_eq!(new("wheel_fr"), [([5, 2, 0], RED)]);
    assert_eq!(new("wheel_rl"), [([-5, -3, 0], RED)], "the source on its other side");
    assert_eq!(new("wheel_fl"), [([-5, 2, 0], RED)], "a clone not mirrored: as the source");
    assert_eq!(new("wheel_rr"), [([5, -3, 0], RED)], "mirrored: as the clone stroked");
    for (name, cells) in &was {
        assert!(cells.is_subset(&now[name]), "{name} moved");
    }
    let t = text(&dir);
    assert!(t.contains("name = \"wheel_fr\"\nlike = \"wheel_rl\"\nmirror = \"x\"\nat = [3, 2, 0]\n") && t.contains("name = \"wheel_fl\"\nlike = \"wheel_rl\"\nat = [-5, 2, 0]\n"), "{t}");
}

/// CLONE: a part like the chosen one where it stands, named as DUPLICATE
/// names its copy, chosen; CLONE MIRRORED: across the figure's middle,
/// mirrored, a pair's partner named after it and refused while that name
/// is taken; each one step.
#[test]
fn clone_and_clone_mirrored_link_a_new_part() {
    let (mut ed, mut view, dir) = crate::editor::model::parts::tests::figure("clone");
    let fig = dir.join("fig.vvx");
    let read = || std::fs::read_to_string(&fig).unwrap();
    let before = read();
    let arm = crate::editor::model::parts::tests::part(&ed, "arm");
    act(&mut ed, &mut view, Action::ClonePart(arm, false));
    let t = read();
    assert!(t.contains("name = \"arm_2\"\nlike = \"arm\"\nat = [3, -1, 3]\n"), "{t}");
    assert_eq!(ed.figure.as_ref().unwrap().selected, Some(crate::editor::model::parts::tests::part(&ed, "arm_2")), "chosen");
    ed.undo();
    assert_eq!(read(), before);
    ed.rename_part(arm, "arm_left").unwrap();
    let arm = crate::editor::model::parts::tests::part(&ed, "arm_left");
    act(&mut ed, &mut view, Action::ClonePart(arm, true));
    let t = read();
    // The figure is 9 wide about its pivot (x -4 to 4): the arm at x 3 to
    // 4 mirrored to -4 to -3.
    assert!(t.contains("name = \"arm_right\"\nlike = \"arm_left\"\nmirror = \"x\"\nat = [-4, -1, 3]\n"), "{t}");
    // Drawn as MIRROR draws the partner, voxel for voxel.
    let (shape, _) = crate::vvx::read_shape(&t).unwrap();
    let of = |n: &str| shape.pieces.iter().find(|p| p.name == n).unwrap().voxels.iter().map(|(v, m)| (v.to_array(), m.rgba)).collect::<BTreeSet<_>>();
    let cloned = of("arm_right");
    ed.undo();
    let arm = crate::editor::model::parts::tests::part(&ed, "arm_left");
    act(&mut ed, &mut view, Action::MirrorPart(arm));
    let (copied, _) = crate::vvx::read_shape(&read()).unwrap();
    let copied: BTreeSet<_> = copied.pieces.iter().find(|p| p.name == "arm_right").unwrap().voxels.iter().map(|(v, m)| (v.to_array(), m.rgba)).collect();
    assert_eq!(cloned, copied, "CLONE MIRRORED stands where MIRROR's copy stands");
    let arm = crate::editor::model::parts::tests::part(&ed, "arm_left");
    assert!(ed.cannot_clone_mirrored(arm).is_some_and(|w| w.contains("arm_right")), "the partner's name taken");
}

/// MAKE UNIQUE: the clone's voxels its own, written out, drawn the same;
/// LIKE: a part with voxels of its own made a clone (they are thrown away);
/// MIRROR's row; each one step back.
#[test]
fn make_unique_and_like_link_and_unlink() {
    let (mut ed, mut view, dir) = car("unique");
    let before = text(&dir);
    let was = cells(&dir);
    let right = part(&ed, "body_right");
    act(&mut ed, &mut view, Action::Part(right));
    act(&mut ed, &mut view, Action::LinkPart(right, None));
    assert_eq!(links(&dir)["body_right"], (None, None));
    assert_eq!(cells(&dir), was, "drawn the same");
    assert!(text(&dir).contains("name = \"body_right\"\nat = [0, -2, 1]\nlayers = "));
    // Linked again, to the left half: its own voxels gone, the left's
    // shown (not mirrored: the mirror went with MAKE UNIQUE).
    let left = part(&ed, "body_left");
    act(&mut ed, &mut view, Action::LinkPart(right, Some(left)));
    assert_eq!(links(&dir)["body_right"], (Some("body_left".into()), None));
    act(&mut ed, &mut view, Action::LinkMirror(right, Some(0)));
    assert_eq!(text(&dir), before, "mirrored in x again: the file as it was");
    for _ in 0..3 {
        ed.undo();
    }
    assert_eq!(text(&dir), before);
    // A source cannot become a clone, a clone has nothing of its own to
    // offer.
    assert!(ed.cannot_link(left).is_some() && ed.possible_sources(left).is_empty());
    assert!(!ed.possible_sources(right).contains(&"wheel_fr".to_string()), "a clone is no source");
}

/// REMOVE of a source: its first clone becomes the source, the others are
/// like it with their mirrors worked out; every part drawn as before.
/// RENAME of a source: its clones' `like` follows. UNDO byte for byte.
#[test]
fn remove_and_rename_of_a_source_keep_its_clones() {
    let (mut ed, mut view, dir) = car("remove");
    let before = text(&dir);
    let mut was = cells(&dir);
    let gone = part(&ed, "wheel_rl");
    act(&mut ed, &mut view, Action::RemovePart(gone));
    let l = links(&dir);
    assert_eq!(l["wheel_fr"], (None, None), "the first clone: the source now");
    assert_eq!(l["wheel_rr"], (Some("wheel_fr".into()), None), "both mirrored in x: the same");
    assert_eq!(l["wheel_fl"], (Some("wheel_fr".into()), Some(0)));
    was.remove("wheel_rl");
    assert_eq!(cells(&dir), was, "every part drawn as before");
    ed.undo();
    assert_eq!(text(&dir), before);
    ed.follow_figure();
    ed.rename_part(part(&ed, "wheel_rl"), "wheel").unwrap();
    let t = text(&dir);
    assert_eq!(t.matches("like = \"wheel\"\n").count(), 3, "{t}");
    ed.undo();
    assert_eq!(text(&dir), before);
}

/// MERGE: a clone merged into its parent is made its own first; a source
/// merged away hands its voxels down to its first clone. The tree printed
/// marks the clones.
#[test]
fn merging_a_clone_or_a_source_keeps_the_rest_drawn() {
    let (mut ed, mut view, dir) = car("merge");
    let lines = ed.figure.as_ref().unwrap().tree_lines();
    assert!(lines.iter().any(|l| l.contains("body_right") && l.contains("(like body_left, mirrored x)")), "{lines:?}");
    let was = cells(&dir);
    let (src, into) = (part(&ed, "wheel_rl"), part(&ed, "body_left"));
    act(&mut ed, &mut view, Action::MergeInto(src, into));
    let now = cells(&dir);
    assert!(!now.contains_key("wheel_rl"));
    assert_eq!(now["body_right"].len(), was["body_right"].len() + was["wheel_rl"].len(), "merged into a source: on its clone too");
    for name in ["wheel_fr", "wheel_rr", "wheel_fl"] {
        assert_eq!(now[name].len(), was[name].len(), "{name}");
    }
    assert_eq!(links(&dir)["wheel_fr"], (None, None));
}

/// SELECT's steps on a clone's voxels act on its source: REMOVE takes the
/// voxel out of every one.
#[test]
fn a_selection_on_a_clone_acts_on_its_source() {
    let (mut ed, mut view, dir) = car("select");
    let right = part(&ed, "body_right");
    act(&mut ed, &mut view, Action::Part(right));
    ed.select_from_pivot(IVec3::new(1, -2, 2), IVec3::new(1, -2, 2));
    act(&mut ed, &mut view, Action::SelRemove);
    let now = cells(&dir);
    assert!(!now["body_left"].iter().any(|c| c.0 == [-1, -2, 2]), "gone from the source");
    assert!(!now["body_right"].iter().any(|c| c.0 == [1, -2, 2]));
    assert!(now["body_left"].iter().any(|c| c.0 == [0, -2, 2]), "its neighbour stays");
}

/// SYMMETRY on a pair whose halves are one (a clone mirrored across the
/// middle): the stroke shows on the partner once, through the link.
#[test]
fn symmetry_on_a_linked_pair_lands_once() {
    let (mut ed, mut view, dir) = crate::editor::model::parts::tests::figure("symmetry");
    let fig = dir.join("fig.vvx");
    ed.rename_part(crate::editor::model::parts::tests::part(&ed, "arm"), "arm_left").unwrap();
    ed.follow_figure();
    let arm = crate::editor::model::parts::tests::part(&ed, "arm_left");
    act(&mut ed, &mut view, Action::ClonePart(arm, true));
    let arm = crate::editor::model::parts::tests::part(&ed, "arm_left");
    act(&mut ed, &mut view, Action::Part(arm));
    ed.symmetry = true;
    ed.choose(Tool::Add);
    ed.colour = [255, 0, 0];
    // On top of the arm's (0, 0) column: x 3 from the pivot.
    ed.press(Hit { cell: IVec3::new(0, 0, 4), normal: IVec3::Z });
    ed.release();
    let (shape, _) = crate::vvx::read_shape(&std::fs::read_to_string(&fig).unwrap()).unwrap();
    let red = |n: &str| shape.pieces.iter().find(|p| p.name == n).unwrap().voxels.iter().filter(|v| v.1.rgba == RED).map(|v| v.0.to_array()).collect::<Vec<_>>();
    assert_eq!(red("arm_left"), [[3, -1, 8]]);
    assert_eq!(red("arm_right"), [[-3, -1, 8]], "mirrored across the middle, once");
    assert!(std::fs::read_to_string(&fig).unwrap().contains("like = \"arm_left\""));
}

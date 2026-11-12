//! A figure of parts as the editor shows it whole (FIGURE): every part at
//! its place as the game draws the figure unposed, the parts listed by
//! what they hang on, each joint marked, and in plain words what is
//! plainly wrong with the rig. A part chosen is shaped right there, the
//! rest see-through round it, and its parent and joint may be set by hand
//! (`Editor::set_parent`, `move_joint`; kept in the file). Which part
//! hangs on which, and where it turns, are the game's rules
//! (`vox::parts_of`), worked out afresh from the file as it would be
//! saved now, so that what is shown is what the game does. A rig nobody
//! sees goes wrong unseen: an arm hung at its hand because the forearm
//! came as near to the body as the upper arm.

use anyhow::Result;
use glam::{IVec3, Vec3};

use super::frames::{ModelFile, Shaped};
use crate::render::Instance;
use crate::vox::{Hung, Part, VoxModel};

/// A figure shown whole, and what the editor does with it.
pub struct Figure {
    /// As the game makes it of the file (its one frame, the parts).
    pub model: VoxModel,
    /// Each part's frame in the file (the model it is made of).
    pub frames: Vec<usize>,
    /// The parts as the list shows them: the root first, under each part
    /// the parts that hang on it (its place among the parts, how deep).
    pub tree: Vec<(usize, usize)>,
    /// The part chosen in the list: drawn full, the others dimmed.
    pub selected: Option<usize>,
    /// Parts left out of the picture (nothing saved).
    pub hidden: Vec<bool>,
    /// Where its low corner (voxel 0, 0, 0 of `model`) stands from the
    /// model's pivot, as the file places every part and joint.
    pub low: IVec3,
    /// The frames and parts it was made of, to know when to make it again.
    of: Shaped,
}

/// A part touching two hung parts about equally: the runner-up has at
/// least this share of the winner's faces (three quarters).
const NEAR_TIE: (u32, u32) = (3, 4);

/// How opaque the parts not chosen are drawn: the chosen one shows
/// through them.
const DIMMED: u8 = 130;

impl Figure {
    /// The figure the model file makes, if it is one (its objects have
    /// names).
    pub fn of(file: &ModelFile) -> Option<Result<Self>> {
        Some(file.figure()?.map(|(model, frames, low)| {
            let count = model.parts.len();
            Self { tree: tree(&model.parts), model, frames, selected: None, hidden: vec![false; count], low, of: file.shaped() }
        }))
    }

    /// Made again if the file's frames changed since (the choice and what
    /// is hidden kept by the parts' names); a mistake is said.
    pub fn follow(old: Option<Self>, file: &ModelFile) -> (Option<Self>, Option<String>) {
        if old.as_ref().is_some_and(|o| file.is(&o.of)) {
            return (old, None);
        }
        match Self::of(file) {
            None => (None, None),
            Some(Err(e)) => (old, Some(format!("{e:#}"))),
            Some(Ok(mut new)) => {
                if let Some(old) = old {
                    let name = |f: &Self, p: usize| f.model.parts[p].name.clone();
                    let find = |f: &Self, n: &str| f.model.parts.iter().position(|q| q.name == n);
                    new.selected = old.selected.and_then(|p| find(&new, &name(&old, p)));
                    for p in (0..old.hidden.len()).filter(|&p| old.hidden[p]) {
                        if let Some(q) = find(&new, &name(&old, p)) {
                            new.hidden[q] = true;
                        }
                    }
                }
                (Some(new), None)
            }
        }
    }

    pub fn parts(&self) -> &[Part] {
        &self.model.parts
    }

    /// Where part `p` turns, in the figure's voxels (from the low corner
    /// of them all, as the view draws it).
    pub fn joint(&self, p: usize) -> Vec3 {
        let s = self.model.size;
        self.model.parts[p].joint + glam::IVec3::new(s.x / 2, s.y / 2, 0).as_vec3()
    }

    /// What is plainly wrong with how part `p` hangs, in plain words (none:
    /// nothing to say). Only what the rules themselves tell, no guesses
    /// from the names.
    pub fn warnings(&self, p: usize) -> Vec<String> {
        let parts = &self.model.parts;
        let name = |q: usize| parts[q].name.as_str();
        let part = &parts[p];
        let mut said = Vec::new();
        if part.voxels.is_empty() {
            said.push("has no voxels".to_string());
        }
        let parent = part.parent.map_or("", name);
        match &part.hung {
            // Set by hand: the maker's word, nothing to say.
            Hung::Root | Hung::ByHand => {}
            Hung::Nearest { touches, as_near } => {
                let also = as_near.map_or(String::new(), |r| format!(" ({} comes as near)", name(r)));
                if touches.is_empty() {
                    said.push(format!("touches nothing: joined to {parent} where it comes nearest{also}"));
                } else {
                    let with: Vec<&str> = touches.iter().map(|&r| name(r)).collect();
                    said.push(format!("touches nothing but {}: joined to {parent} where it comes nearest{also}", with.join(", ")));
                }
            }
            Hung::Touching { faces, next: Some((r, n)) } if n * NEAR_TIE.1 >= faces * NEAR_TIE.0 => {
                let count = if n == faces { format!("{n} faces each") } else { format!("{n} faces against {faces}") };
                said.push(format!("could as well hang on {} ({count})", name(*r)));
            }
            Hung::Touching { .. } => {}
        }
        // Voxels of two parts in one place are no mistake: the parts are
        // models of their own laid together (`overlaps` keeps the count).
        said
    }

    /// The figure as cubes, every part not hidden at its place moved by
    /// `shift` (into the grid shown), but the chosen one, which the view
    /// draws itself as it is being shaped: the others dimmed while one is
    /// chosen. The opaque ones go into `out`; the see-through ones (dimmed
    /// parts, glass) come back, to be drawn after them far to near.
    pub fn cubes(&self, shift: IVec3, out: &mut Vec<Instance>) -> Vec<Instance> {
        let frame = &self.model.frames[0];
        let mut through = Vec::new();
        for part in (0..self.model.parts.len()).filter(|&p| !self.hidden[p] && self.selected != Some(p)).map(|p| &self.model.parts[p]) {
            let dim = self.selected.is_some();
            // Its own surface (the faces it shares with a part hidden show).
            for &k in &part.visible {
                let (at, c) = frame[k as usize];
                let opacity = if dim { DIMMED.min(c[3]) } else { c[3] };
                let pos = (at + shift).as_vec3() + 0.5;
                let cube = Instance { pos_size: [pos.x, pos.y, pos.z, 1.0], color: [c[0], c[1], c[2], 255], fx: [0, opacity, 0, 0], shade: [0; 4], turn: [0; 4] };
                if opacity < 255 { through.push(cube) } else { out.push(cube) }
            }
        }
        through
    }
}

/// The parts in the list's order: the root first, then depth first the
/// parts hanging on each, in the file's order (their place and how deep).
/// A part hung on nothing besides the root comes after its tree.
pub fn tree(parts: &[Part]) -> Vec<(usize, usize)> {
    fn add(parts: &[Part], p: usize, depth: usize, out: &mut Vec<(usize, usize)>) {
        out.push((p, depth));
        for c in (0..parts.len()).filter(|&c| parts[c].parent == Some(p)) {
            add(parts, c, depth + 1, out);
        }
    }
    let mut tops: Vec<usize> = (0..parts.len()).filter(|&p| parts[p].parent.is_none()).collect();
    // The root first: the biggest, as `parts_of` chose it.
    tops.sort_by_key(|&p| (std::cmp::Reverse(parts[p].voxels.len()), p));
    let mut out = Vec::new();
    for p in tops {
        add(parts, p, 0, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::{Editor, Hit, Scope};
    use crate::editor::view::{Action, View, act};
    use glam::IVec3;

    /// A figure in a folder of its own: a body, a head on it, an arm
    /// apart from it with a hand on the arm, a scarf on body and head
    /// alike.
    fn figure(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("voxcade-figure-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::vox::write_figure(
            &dir.join("figure.vvx"),
            &[
                ("hand", IVec3::new(1, 2, 2), IVec3::new(7, 1, 0)),
                ("body", IVec3::new(4, 4, 8), IVec3::ZERO),
                ("arm", IVec3::new(1, 2, 4), IVec3::new(6, 1, 0)),
                ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)),
                ("scarf", IVec3::new(1, 4, 2), IVec3::new(4, 0, 7)),
            ],
        );
        // Its rig left to the rules, as a figure made in Voxcade has it (the
        // way in from MagicaVoxel writes some by hand).
        let file = dir.join("figure.vvx");
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, text.lines().filter(|l| !l.starts_with("parent =") && !l.starts_with("joint =")).map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        dir
    }

    /// The list has the root first and under each part those hanging on
    /// it, deeper by one; and says in plain words what is plainly wrong.
    #[test]
    fn the_parts_are_listed_by_what_they_hang_on() {
        let dir = figure("list");
        let f = Figure::of(&ModelFile::load(&dir.join("figure.vvx")).unwrap()).unwrap().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let names: Vec<(&str, usize)> = f.tree.iter().map(|&(p, depth)| (f.parts()[p].name.as_str(), depth)).collect();
        assert_eq!(names, [("body", 0), ("arm", 1), ("hand", 2), ("head", 1), ("scarf", 1)]);
        let said = |name: &str| f.warnings(f.parts().iter().position(|p| p.name == name).unwrap());
        assert_eq!(said("arm"), ["touches nothing but hand: joined to body where it comes nearest"]);
        assert_eq!(said("scarf"), ["could as well hang on head (4 faces each)"]);
        assert!(said("body").is_empty() && said("head").is_empty() && said("hand").is_empty());
        // Each part's frame is the model it is made of.
        assert_eq!(f.frames, [0, 1, 2, 3, 4]);
    }

    /// A figure opens whole (FIGURE); a part chosen is shaped in it (its
    /// frame), and BACK (Escape) comes back to the figure with none
    /// chosen; a part hidden is left out of the picture only. A model of
    /// frames opens as before.
    #[test]
    fn a_figure_opens_whole_and_a_part_is_shaped_from_it() {
        let dir = figure("open");
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        assert_eq!(ed.scope, Scope::Figure);
        assert_eq!(ed.grid(), ed.figure.as_ref().unwrap().model.size);
        let all = crate::editor::view::frame(&ed, &view, &crate::editor::view::Shown::new(&ed).unwrap(), &crate::editor::view::Figures::load(&dir, &Default::default()), &Default::default(), 1.5).instances.len();
        let head = ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == "head").unwrap();
        act(&mut ed, &mut view, Action::Hide(head));
        let fewer = crate::editor::view::frame(&ed, &view, &crate::editor::view::Shown::new(&ed).unwrap(), &crate::editor::view::Figures::load(&dir, &Default::default()), &Default::default(), 1.5).instances.len();
        assert!(fewer < all, "the head hidden: {fewer} cubes of {all}");
        act(&mut ed, &mut view, Action::Part(head));
        let marks = crate::editor::view::joint_marks(&ed, &view, 1.5, glam::Vec2::ZERO);
        assert!(marks.iter().all(|m| m.part != head), "a hidden part's joint shows");
        act(&mut ed, &mut view, Action::ShowAll);
        let marks = crate::editor::view::joint_marks(&ed, &view, 1.5, glam::Vec2::ZERO);
        let chosen: Vec<_> = marks.iter().filter(|m| m.chosen).collect();
        assert!(chosen.len() == 1 && chosen[0].part == head && chosen[0].parent.is_some(), "{marks:?}");
        assert!(marks.iter().all(|m| (0.0..=1.0).contains(&m.at.x) && (0.0..=1.0).contains(&m.at.y)), "a joint off the screen: {marks:?}");
        // The chosen part is shaped in the figure: its frame.
        assert_eq!(ed.scope, Scope::Frame(3));
        act(&mut ed, &mut view, Action::Back);
        assert_eq!((ed.scope, ed.figure.as_ref().unwrap().selected), (Scope::Figure, None));
        let _ = std::fs::remove_dir_all(&dir);

        // A model of frames: frames, no figure. A MagicaVoxel file is not
        // shaped: it is said how to bring it in.
        let frames = Editor::open_model(std::path::Path::new("games/voxkeep"), "bat.vvx").unwrap();
        assert!(frames.figure.is_none() && frames.scope == Scope::Frame(0));
        let e = Editor::open_model(std::path::Path::new("games/moreblood"), "prefabs/palm_0.vox").err().unwrap().to_string();
        assert!(e.contains("MagicaVoxel") && e.contains("voxcade import"), "{e}");
    }

    /// A part shaped changes the figure: it is made again from the frames
    /// as they stand, the chosen part kept by its name.
    #[test]
    fn the_figure_follows_its_frames() {
        let dir = figure("follow");
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        act(&mut ed, &mut view, Action::Part(0));
        let before = ed.figure.as_ref().unwrap().model.frames[0].len();
        ed.model.as_mut().unwrap().frames[1].set(IVec3::new(0, 0, 0), None).unwrap();
        ed.follow_figure();
        let f = ed.figure.as_ref().unwrap();
        assert_eq!((f.model.frames[0].len(), f.selected), (before - 1, Some(0)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each part's voxels where they stand in the file's scene, and its
    /// parent's name and joint there (as the game makes the figure of the
    /// file on disk).
    #[allow(clippy::type_complexity)]
    fn on_disk(path: &std::path::Path) -> std::collections::BTreeMap<String, (Vec<[i32; 3]>, Option<String>, [f32; 3], bool)> {
        let (m, _, low) = ModelFile::load(path).unwrap().figure().unwrap().unwrap();
        let pivot = IVec3::new(m.size.x / 2, m.size.y / 2, 0);
        m.parts
            .iter()
            .map(|p| {
                let mut voxels: Vec<[i32; 3]> = p.voxels.iter().map(|&k| (m.frames[0][k as usize].0 + low).to_array()).collect();
                voxels.sort();
                let parent = p.parent.map(|q| m.parts[q].name.clone());
                (p.name.clone(), (voxels, parent, (p.joint + (pivot + low).as_vec3()).to_array(), p.joint_by_hand))
            })
            .collect()
    }

    fn part(ed: &Editor, name: &str) -> usize {
        ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == name).unwrap()
    }

    /// A part is shaped in the figure: ADD beyond its box's low side grows
    /// that part's box alone, every part staying where it stands, every
    /// parent and joint as it was, the file a figure still; the other
    /// frames are not touched, and the controls of frames do nothing.
    #[test]
    fn a_part_is_shaped_in_the_figure_and_stays_in_place() {
        let dir = figure("shape");
        let file = dir.join("figure.vvx");
        let before = on_disk(&file);
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        let head = part(&ed, "head");
        act(&mut ed, &mut view, Action::Part(head));
        assert_eq!(ed.scope, Scope::Frame(3), "the head's frame, shaped in the figure");
        let target = view.block.target;
        let others: Vec<crate::blocks::Block> = ed.frames().iter().enumerate().filter(|&(k, _)| k != 3).map(|(_, f)| f.clone()).collect();
        ed.colour = [10, 200, 30];
        ed.press(Hit { cell: IVec3::ZERO, normal: IVec3::NEG_X });
        ed.release();
        act(&mut ed, &mut view, Action::Undo);
        act(&mut ed, &mut view, Action::Redo);
        assert_eq!(ed.frames()[3].size, IVec3::new(5, 4, 4), "the head's box grew to the left");
        assert_eq!(others, ed.frames().iter().enumerate().filter(|&(k, _)| k != 3).map(|(_, f)| f.clone()).collect::<Vec<_>>(), "another part's box changed");
        assert_eq!(view.block.target - target, glam::Vec3::X, "the view goes with the figure");
        let after = on_disk(&file);
        for (name, (voxels, parent, joint, set)) in &before {
            let now = &after[name];
            assert_eq!((&now.1, now.2, now.3), (parent, *joint, *set), "{name}'s rig");
            if name == "head" {
                // Left of the head's low corner.
                let low = voxels.iter().fold([i32::MAX; 3], |l, v| [l[0].min(v[0]), l[1].min(v[1]), l[2].min(v[2])]);
                let mut grown = voxels.clone();
                grown.push([low[0] - 1, low[1], low[2]]);
                grown.sort();
                assert_eq!(now.0, grown, "the head and its new voxel where they stand");
            } else {
                assert_eq!(&now.0, voxels, "{name} moved or changed");
            }
        }
        // The frames' controls mean nothing in a figure.
        let count = ed.frames().len();
        ed.copy_frame();
        ed.delete_frame();
        ed.step_frame(1);
        assert_eq!((ed.frames().len(), ed.scope), (count, Scope::Frame(3)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Parts are models of their own laid together: ADD in a cell another
    /// part has a voxel in adds it to the part chosen and leaves the other
    /// as it was; both keep theirs, saved and read again.
    #[test]
    fn a_part_takes_room_another_part_holds_and_both_keep_theirs() {
        let dir = figure("overlap");
        let file = dir.join("figure.vvx");
        let before = on_disk(&file);
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        let head = part(&ed, "head");
        act(&mut ed, &mut view, Action::Part(head));
        // Down from the head's underside: the body's top voxel under the
        // head's low corner.
        ed.press(Hit { cell: IVec3::ZERO, normal: IVec3::NEG_Z });
        ed.release();
        assert!(ed.hint.is_none(), "{:?}", ed.hint);
        let after = on_disk(&file);
        assert_eq!(after["body"].0, before["body"].0, "the body is unchanged");
        let low = before["head"].0.iter().fold([i32::MAX; 3], |l, v| [l[0].min(v[0]), l[1].min(v[1]), l[2].min(v[2])]);
        let under = [low[0], low[1], low[2] - 1];
        assert!(after["head"].0.contains(&under) && after["body"].0.contains(&under), "both have a voxel there");
        let again = Editor::open_model(&dir, "figure.vvx").unwrap();
        let f = again.figure.as_ref().unwrap();
        let (head, body) = (part(&again, "head"), part(&again, "body"));
        assert_eq!(f.parts()[head].overlaps, vec![(body, 1)]);
        assert!(f.warnings(head).is_empty(), "overlap is no mistake");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The chosen part's parent and joint set by hand: the parent the next
    /// part clicked, the joint half a voxel a step; saved in the file,
    /// read again the same; every step taken back and done again; a parent
    /// that would make a ring refused, said why.
    #[test]
    fn a_parent_and_a_joint_are_set_by_hand_and_kept() {
        let dir = figure("rig");
        let file = dir.join("figure.vvx");
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        let (arm, head) = (part(&ed, "arm"), part(&ed, "head"));
        let was = on_disk(&file)["arm"].clone();
        assert_eq!((was.1.as_deref(), was.3), (Some("body"), false));
        act(&mut ed, &mut view, Action::Part(arm));
        act(&mut ed, &mut view, Action::PickParent);
        act(&mut ed, &mut view, Action::Part(head));
        assert_eq!((ed.scope, ed.picking_parent), (Scope::Frame(2), false), "the arm still chosen");
        act(&mut ed, &mut view, Action::Joint(2, 1));
        act(&mut ed, &mut view, Action::Joint(2, 1));
        act(&mut ed, &mut view, Action::Joint(0, -1));
        let set = on_disk(&file)["arm"].clone();
        let worked_out = ed.figure.as_ref().unwrap();
        assert_eq!(worked_out.parts()[arm].hung, Hung::ByHand);
        assert_eq!((set.1.as_deref(), set.3), (Some("head"), true));
        let on_head = {
            // Where the rules join the arm to the head, then moved.
            let mut back = Editor::open_model(&dir, "figure.vvx").unwrap();
            let mut v = View::new(&back, &crate::config::GameConfig::default());
            let arm = part(&back, "arm");
            act(&mut back, &mut v, Action::Part(arm));
            act(&mut back, &mut v, Action::JointWorkedOut);
            on_disk(&file)["arm"].2
        };
        assert_eq!(set.2, [on_head[0] - 0.5, on_head[1], on_head[2] + 1.0], "moved from where the rules put it");
        // Read again: the same figure (the file as the steps left it).
        act(&mut ed, &mut view, Action::Joint(2, -1));
        act(&mut ed, &mut view, Action::Joint(2, 1));
        let again = Editor::open_model(&dir, "figure.vvx").unwrap();
        assert_eq!(on_disk(&file)["arm"], set);
        assert_eq!(again.figure.as_ref().unwrap().parts()[arm].joint, ed.figure.as_ref().unwrap().parts()[arm].joint);
        // Every step back: the arm on the body again by the rules.
        for _ in 0..6 {
            act(&mut ed, &mut view, Action::Undo);
        }
        assert_eq!(on_disk(&file)["arm"], was);
        for _ in 0..6 {
            act(&mut ed, &mut view, Action::Redo);
        }
        assert_eq!(on_disk(&file)["arm"], set);
        // The head on the arm, which hangs on the head by hand: a ring.
        act(&mut ed, &mut view, Action::Part(head));
        act(&mut ed, &mut view, Action::PickParent);
        act(&mut ed, &mut view, Action::Part(arm));
        assert!(ed.hint.as_deref().is_some_and(|h| h.contains("ring")), "{:?}", ed.hint);
        assert_eq!(on_disk(&file)["head"].1.as_deref(), Some("body"));
        // Given back to the rules.
        act(&mut ed, &mut view, Action::Part(arm));
        act(&mut ed, &mut view, Action::ParentWorkedOut);
        act(&mut ed, &mut view, Action::JointWorkedOut);
        assert_eq!(on_disk(&file)["arm"], was);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

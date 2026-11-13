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
use glam::{IVec3, Mat4, Quat, Vec3};

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
    /// Each part's pose in the editor (POSE: turned on top of its rest,
    /// nothing written; empty, none), set by the editor.
    pub pose: Vec<crate::session::PartPose>,
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
            Self { tree: tree(&model.parts), model, frames, selected: None, hidden: vec![false; count], low, pose: Vec::new(), of: file.shaped() }
        }))
    }

    /// Made again if the file's frames changed since (the choice and what
    /// is hidden kept by the parts' names); a mistake is said. `made`: the
    /// figure of the file as it is now, if one was made already (`pin_rig`
    /// makes it), taken instead of making it again.
    pub fn follow(old: Option<Self>, file: &ModelFile, made: Option<Self>) -> (Option<Self>, Option<String>) {
        if old.as_ref().is_some_and(|o| file.is(&o.of)) {
            return (old, None);
        }
        let made = made.filter(|m| file.is(&m.of)).map(Ok).map(Some);
        match made.unwrap_or_else(|| Self::of(file)) {
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

    /// Whether it was made of the file as `shaped` holds it.
    pub fn made_of(&self, shaped: &Shaped) -> bool {
        self.of == *shaped
    }

    /// Whether `file` differs from what it was made of only in the voxels
    /// of the part chosen (a stroke going on, drawn from the world shown),
    /// and nothing it draws hangs on the rig: no part turned at rest, no
    /// pose. Then the parts it draws are as they would be made again; the
    /// rig the rules work out (where the chosen part meets the others)
    /// follows when the stroke is let go (`Editor::follow_figure`).
    pub fn only_chosen_voxels_changed(&self, file: &ModelFile) -> bool {
        let Some(k) = self.selected.and_then(|p| self.frames.get(p).copied()) else { return false };
        let of = &self.of;
        self.pose.is_empty()
            && !crate::session::rests_turned(&self.model)
            && file.parts == of.parts
            && file.size == of.size
            && file.palette == of.palette
            && file.frames.len() == of.frames.len()
            && file.frames.iter().zip(&of.frames).enumerate().all(|(f, (now, was))| if f == k { now.size == was.size } else { now == was })
    }

    /// Its rig by part name: what each hangs on (by name) and its joint
    /// from the pivot (`file_joint`).
    pub fn rig(&self) -> std::collections::BTreeMap<String, (Option<String>, Vec3)> {
        self.parts().iter().enumerate().map(|(p, o)| (o.name.clone(), (o.parent.map(|q| self.parts()[q].name.clone()), self.file_joint(p)))).collect()
    }

    /// Where part `p` turns, in the figure's voxels (from the low corner
    /// of them all, as the view draws it).
    pub fn joint(&self, p: usize) -> Vec3 {
        let s = self.model.size;
        self.model.parts[p].joint + glam::IVec3::new(s.x / 2, s.y / 2, 0).as_vec3()
    }

    /// Where part `p` turns as a person or a tool reads it: from the
    /// model's pivot, as the file (`joint`) and the JOINT row have it.
    pub fn file_joint(&self, p: usize) -> Vec3 {
        self.joint(p) + self.low.as_vec3()
    }

    /// The parts as `voxcade edit --shot` prints them, the list's order:
    /// how each hangs, where it turns (from the pivot, as the file and the
    /// JOINT row say), what is wrong with it.
    pub fn tree_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for &(p, depth) in &self.tree {
            let part = &self.parts()[p];
            let on = part.parent.map_or("(the root)".to_string(), |q| format!("on {}", self.parts()[q].name));
            let j = self.file_joint(p);
            let by_hand = match (part.hung == Hung::ByHand, part.joint_by_hand) {
                (true, true) => " (parent and joint set by hand)",
                (true, false) => " (parent set by hand)",
                (false, true) => " (joint set by hand)",
                (false, false) => "",
            };
            let chosen = if self.selected == Some(p) { " (chosen)" } else { "" };
            lines.push(format!("{}{} {on}, turning at {}, {}, {}{by_hand}{chosen}{}", "  ".repeat(depth + 1), part.name, j.x, j.y, j.z, if self.hidden[p] { " (hidden)" } else { "" }));
            for w in self.warnings(p) {
                lines.push(format!("{}  ! {w}", "  ".repeat(depth + 1)));
            }
            for &(q, n) in &part.overlaps {
                lines.push(format!("{}  ({n} in the same place as {}'s)", "  ".repeat(depth + 1), self.parts()[q].name));
            }
        }
        lines
    }

    /// Where part `p` is drawn at rest, as a map of the grid shown (the
    /// figure's voxel 0, 0, 0 at `shift`): from where its voxels lie in the
    /// file to where they show, its rest rotation and its parents' about
    /// their joints (`session::part_at_rest`); the identity for a figure
    /// without rest rotations.
    pub fn rest_map(&self, p: usize, shift: IVec3) -> Mat4 {
        // (Posed in the editor (POSE): the pose on top of the rest.)
        let (turn, moved) = if self.pose.iter().any(|q| *q != crate::session::PartPose::REST) { crate::session::part_posed(&self.model, &self.pose, p) } else { crate::session::part_at_rest(&self.model, p) };
        let s = self.model.size;
        let o = (IVec3::new(s.x / 2, s.y / 2, 0) + shift).as_vec3();
        Mat4::from_translation(o + moved) * Mat4::from_quat(turn) * Mat4::from_translation(-o)
    }

    /// Where part `p`'s joint shows at rest in the grid shown (it turns
    /// with its parent, not with itself).
    pub fn joint_shown(&self, p: usize, shift: IVec3) -> Vec3 {
        let at = self.joint(p) + shift.as_vec3();
        match self.model.parts[p].parent {
            Some(q) => self.rest_map(q, shift).transform_point3(at),
            None => at,
        }
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
            // Its joint set by hand: where it turns is the maker's word, so
            // touching nothing is meant (a logo's letters, an upper arm in
            // a sleeve); flagged LOOSE on every row, the warnings were
            // learnt to be ignored.
            Hung::Nearest { .. } if part.joint_by_hand => {}
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

    /// The part a ray (in the grid shown, its parts moved there by
    /// `shift`) meets first, of those not hidden.
    pub fn part_at(&self, shift: IVec3, origin: glam::Vec3, dir: glam::Vec3) -> Option<usize> {
        let frame = &self.model.frames[0];
        let mut best: Option<(f32, usize)> = None;
        for (p, part) in self.model.parts.iter().enumerate().filter(|&(p, _)| !self.hidden[p]) {
            // The ray into the part's own place (its rest turn undone): the
            // voxel met is the one seen.
            let back = self.rest_map(p, shift).inverse();
            let (origin, dir) = (back.transform_point3(origin), back.transform_vector3(dir));
            for &k in &part.visible {
                let at = (frame[k as usize].0 + shift).as_vec3();
                if let Some(d) = super::view::ray_box(origin, dir, at, at + 1.0)
                    && best.is_none_or(|(b, _)| d < b)
                {
                    best = Some((d, p));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// The figure as cubes, every part not hidden at its place moved by
    /// `shift` (into the grid shown), but the chosen one, which the view
    /// draws itself as it is being shaped: the others dimmed while one is
    /// chosen. The opaque ones go into `out`; the see-through ones (dimmed
    /// parts, glass) come back, to be drawn after them far to near.
    pub fn cubes(&self, shift: IVec3, cut: &dyn Fn(IVec3) -> bool, out: &mut Vec<Instance>) -> Vec<Instance> {
        self.cubes_seen(shift, Mat4::IDENTITY, self.selected.is_some(), cut, out)
    }

    /// `cubes`, each part at rest (its rest rotation and its parents',
    /// `rest_map`), then moved by `seen` (into the frame the view draws
    /// in: the chosen part's own, `Editor::rest_view`'s inverse).
    pub fn cubes_seen(&self, shift: IVec3, seen_by: Mat4, dim: bool, cut: &dyn Fn(IVec3) -> bool, out: &mut Vec<Instance>) -> Vec<Instance> {
        let frame = &self.model.frames[0];
        let mut through = Vec::new();
        let (_, seen_turn, _) = seen_by.to_scale_rotation_translation();
        for p in (0..self.model.parts.len()).filter(|&p| !self.hidden[p] && self.selected != Some(p)) {
            let part = &self.model.parts[p];
            let map = seen_by * self.rest_map(p, shift);
            let turn = seen_turn * crate::session::part_at_rest(&self.model, p).0;
            let square = turn.dot(Quat::IDENTITY).abs() > 1.0 - 1e-6;
            let snorm = if square { [0; 4] } else { crate::render::snorm_turn(turn) };
            // Its own surface (the faces it shares with a part hidden
            // show); sliced, what the cut leaves of it, its inside where
            // the cut opens it.
            let sliced = part.voxels.iter().any(|&k| cut(frame[k as usize].0 + shift));
            let seen: Vec<u32> = if sliced {
                let filled: crate::world::CellSet = part.voxels.iter().map(|&k| frame[k as usize].0).filter(|&p| !cut(p + shift)).collect();
                part.voxels.iter().copied().filter(|&k| {
                    let p = frame[k as usize].0;
                    filled.contains(&p) && crate::mesh::DIRS.iter().any(|d| !filled.contains(&(p + *d)))
                }).collect()
            } else {
                part.visible.clone()
            };
            // A turned part's faces between its own voxels left out as the
            // game leaves them (`covered`, in the order of `visible`).
            let covered = |n: usize| if sliced { 0 } else { part.covered.get(n).copied().unwrap_or(0) };
            // Turned: placed by the turn the shader draws them with, from
            // one voxel placed exactly, so neighbours meet corner to corner.
            let drawn = crate::render::turn_as_drawn(turn);
            let anchor = seen.first().map(|&k| (frame[k as usize].0 + shift).as_vec3() + 0.5);
            let place = |cell: Vec3| match anchor.filter(|_| !square) {
                Some(a) => map.transform_point3(a) + drawn * (cell - a),
                None => map.transform_point3(cell),
            };
            for (n, &k) in seen.iter().enumerate() {
                let (at, c) = frame[k as usize];
                let opacity = if dim { DIMMED.min(c[3]) } else { c[3] };
                let pos = place((at + shift).as_vec3() + 0.5);
                // (Dimmed too: a part seen through is its surface, not a
                // grid of every cube's faces.)
                let hidden = if c[3] < 255 { 0 } else { covered(n) };
                let cube = Instance { pos_size: [pos.x, pos.y, pos.z, 1.0], color: [c[0], c[1], c[2], 255], fx: [0, opacity, hidden, 0], shade: [0; 4], turn: snorm };
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
    use glam::{IVec3, Vec3};

    /// A figure in a folder of its own: a body, a head on it, an arm
    /// apart from it with a hand on the arm, a scarf on body and head
    /// alike.
    fn figure(name: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("figure-{name}"));
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

    /// A part whose joint the file sets by hand says nothing of touching
    /// nothing (LOOSE): where it turns is the maker's word. BunnyFarm's
    /// logo, letters apart each with its joint by hand, warns of nothing,
    /// and opens from the side that reads the right way round (from +y
    /// the game's logo read "YNNUB MRAF").
    #[test]
    fn a_joint_set_by_hand_is_not_loose() {
        let dir = figure("by-hand");
        let file = dir.join("figure.vvx");
        let text = std::fs::read_to_string(&file).unwrap().replace("name = \"arm\"\n", "name = \"arm\"\njoint = [2.5, 0.5, 4]\n");
        std::fs::write(&file, text).unwrap();
        let f = Figure::of(&ModelFile::load(&file).unwrap()).unwrap().unwrap();
        let arm = f.parts().iter().position(|p| p.name == "arm").unwrap();
        assert!(f.parts()[arm].joint_by_hand && matches!(f.parts()[arm].hung, crate::vox::Hung::Nearest { .. }), "its parent by the rules, its joint by hand");
        assert!(f.warnings(arm).is_empty(), "{:?}", f.warnings(arm));
        let logo = std::path::Path::new("games/bunnyfarm");
        let f = Figure::of(&ModelFile::load(&logo.join("logo.vvx")).unwrap()).unwrap().unwrap();
        assert!(f.parts().len() > 5);
        for p in 0..f.parts().len() {
            assert!(f.warnings(p).is_empty(), "{}: {:?}", f.parts()[p].name, f.warnings(p));
        }
        let cfg = crate::session::config::<crate::Voxcade>(logo).unwrap();
        assert_eq!(crate::editor::view::face_of("logo.vvx", &cfg), crate::editor::view::Face::Logo);
        let ed = Editor::open_model(logo, "logo.vvx").unwrap();
        let view = View::new(&ed, &cfg);
        // Looking from y 0 (the eye on the low side of y), x across left
        // to right.
        let eye = crate::camera::eye(&view.block.camera(), view.block.target, 16.0 / 9.0);
        assert!(eye.y < view.block.target.y, "from the y 0 side: {eye} for {}", view.block.target);
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
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
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
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
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
            let at = on_disk(&file)["arm"].2;
            // Taken back: the file as `ed` wrote it, byte for byte (else
            // `ed` reads it anew, its steps gone).
            act(&mut back, &mut v, Action::Undo);
            at
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

    /// The joint's ring: pressed and dragged, the joint goes across the
    /// view (along the two axes most across it: from the front, X and Z)
    /// by halves of a voxel, one step for the drag; Escape puts it back
    /// with no step; its field sets it too. The root has no ring.
    #[test]
    fn a_joint_is_dragged_by_its_ring_in_one_step() {
        let dir = figure("joint-ring");
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        let joint = |ed: &mut Editor| {
            ed.follow_figure();
            ed.figure.as_ref().unwrap().joint(arm)
        };
        let start = joint(&mut ed);
        let (lo, _) = ed.figure_box().unwrap();
        let spot = ed.joint_spot().unwrap();
        assert_eq!(spot, start + lo.as_vec3());
        // From the front (looking along +y): rays straight in.
        let ray = |p: Vec3| (p - Vec3::Y * 50.0, Vec3::Y);
        let steps = ed.steps();
        assert!(ed.grab_joint(ray(spot), Vec3::Y));
        for k in 1..=6 {
            ed.drag_joint(glam::Mat4::IDENTITY, glam::Vec2::ONE, glam::Vec2::ZERO, ray(spot + Vec3::new(k as f32 * 0.5, 0.0, k as f32 * 0.25)));
        }
        ed.release();
        assert_eq!((joint(&mut ed), ed.steps()), (start + Vec3::new(3.0, 0.0, 1.5), steps + 1), "across the view, by halves, one step");
        ed.undo();
        assert_eq!(joint(&mut ed), start);
        // Escape while dragging: as it was, no step.
        ed.grab_joint(ray(spot), Vec3::Y);
        ed.drag_joint(glam::Mat4::IDENTITY, glam::Vec2::ONE, glam::Vec2::ZERO, ray(spot + Vec3::X * 2.0));
        assert_ne!(joint(&mut ed), start);
        assert!(ed.cancel_joint());
        assert_eq!((joint(&mut ed), ed.steps()), (start, steps));
        // The field: Z to 5.5, a step.
        act(&mut ed, &mut view, Action::JointTo(2, 5.5));
        assert_eq!((joint(&mut ed).z, ed.steps()), (5.5, steps + 1));
        // The root turns about nothing: no ring.
        let body = part(&ed, "body");
        act(&mut ed, &mut view, Action::Part(body));
        assert!(ed.joint_spot().is_none());
    }

    /// A part moved (its arrows, its POSITION, `move_part`) takes those
    /// hanging on it along by the same whole voxels, and a joint set by
    /// hand on any of them; the others stay; one step takes it all back;
    /// the file reads back the same figure shifted. The root moves the
    /// whole figure from its pivot.
    #[test]
    fn a_part_moves_with_what_hangs_on_it() {
        let dir = figure("move-part");
        let file = dir.join("figure.vvx");
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        let arm = part(&ed, "arm");
        let tree: std::collections::BTreeMap<String, Option<String>> = on_disk(&file).into_iter().map(|(n, (_, parent, ..))| (n, parent)).collect();
        assert_eq!(tree["hand"].as_deref(), Some("arm"), "the hand hangs on the arm");
        act(&mut ed, &mut view, Action::Part(arm));
        // The arm's joint set by hand: it goes along.
        let j = ed.figure.as_ref().unwrap().joint(arm) + ed.figure.as_ref().unwrap().low.as_vec3();
        ed.set_joint(Some(j));
        let before = on_disk(&file);
        let text = std::fs::read_to_string(&file).unwrap();
        let steps = ed.steps();
        let by = IVec3::new(2, 0, 1);
        let x = ed.part_at().unwrap().x + 2;
        act(&mut ed, &mut view, Action::PartAt(0, x as f32));
        let z = ed.part_at().unwrap().z + 1;
        act(&mut ed, &mut view, Action::PartAt(2, z as f32));
        assert_eq!(ed.steps(), steps + 2, "a typed value a step");
        let after = on_disk(&file);
        let shifted = |v: &Vec<[i32; 3]>| v.iter().map(|p| [p[0] + by.x, p[1] + by.y, p[2] + by.z]).collect::<Vec<_>>();
        for name in ["arm", "hand"] {
            assert_eq!(after[name].0, shifted(&before[name].0), "{name} along");
        }
        for name in ["body", "head", "scarf"] {
            assert_eq!(after[name].0, before[name].0, "{name} stays");
        }
        assert_eq!(Vec3::from(after["arm"].2), Vec3::from(before["arm"].2) + by.as_vec3(), "the hand-set hinge on the arm along");
        assert_eq!(after["hand"].1, before["hand"].1, "the hand still hangs on the arm");
        ed.undo();
        ed.undo();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "steps back: the file as it was");
        // A drag of its X arrow: one step, whole voxels.
        let orbit = crate::editor::view::Orbit { yaw: 20.0, pitch: 25.0, zoom: 60.0, target: Vec3::splat(3.0), fov: crate::editor::view::FOV };
        let screen = glam::Vec2::new(1600.0, 900.0);
        let vp = orbit.view_proj(screen.x / screen.y);
        let h = ed.part_handles(vp).unwrap();
        let shapes = h.shapes(vp, screen);
        let head = shapes.iter().find(|s| s.grip == crate::editor::handles::Grip::Arrow(0) && s.what == crate::editor::handles::What::Head).unwrap().points[0] - glam::Vec2::new(3.0, 0.0);
        assert_eq!(h.grip_at(vp, screen, head), Some(crate::editor::handles::Grip::Arrow(0)));
        let ray = |p: glam::Vec2| crate::editor::view::mouse_ray(vp, p.x / screen.x, p.y / screen.y);
        let drag = crate::editor::handles::Drag::begin(&h, crate::editor::handles::Grip::Arrow(0), vp, screen, head, ray(head)).unwrap();
        let x0 = ed.part_at().unwrap().x;
        assert!(ed.grab_part(drag, h.clone()));
        let far = crate::editor::handles::project(vp, screen, h.centre + Vec3::X * (h.length + 3.2)).unwrap().0;
        let along = crate::editor::handles::project(vp, screen, h.centre + Vec3::X * h.length).unwrap().0;
        let to = head + (far - along);
        ed.drag_joint(vp, screen, to, ray(to));
        ed.release();
        assert_eq!((ed.part_at().unwrap().x - x0, ed.steps()), (3, steps + 1));
        ed.undo();
        // The root moves them all.
        let all = on_disk(&file);
        let body = part(&ed, "body");
        act(&mut ed, &mut view, Action::Part(body));
        ed.move_part(IVec3::X);
        let moved = on_disk(&file);
        for (name, (voxels, ..)) in &all {
            assert_eq!(moved[name].0, voxels.iter().map(|p| [p[0] + 1, p[1], p[2]]).collect::<Vec<_>>(), "{name}");
        }
    }

    /// The view's right click: on a voxel PICK COLOUR and ERASE (and the
    /// other part there to shape); the figure whole a part's menu; a click
    /// in the figure whole chooses the part under the mouse; PICK COLOUR
    /// and ERASE do what they say, ERASE a step.
    #[test]
    fn a_right_click_on_a_voxel_offers_what_can_be_done_there() {
        use crate::editor::view::MenuOn;
        let dir = figure("voxel-menu");
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        let words = |ed: &Editor, on| crate::editor::shaping::menu_entries(ed, on).into_iter().map(|(w, ..)| w).collect::<Vec<_>>();
        let (body, head) = (part(&ed, "body"), part(&ed, "head"));
        assert_eq!(words(&ed, MenuOn::Part(head)), ["SELECT HEAD", "HIDE", "HIDE OTHERS", "ADD PART", "DUPLICATE", "MIRROR", "RENAME", "MERGE INTO BODY", "MERGE WITH…", "REMOVE"]);
        // From high above the figure's middle: the head on top.
        let (lo, size) = ed.figure_box().unwrap();
        let top = Vec3::new(lo.x as f32 + 2.5, lo.y as f32 + 2.5, 100.0);
        assert_eq!(ed.figure.as_ref().unwrap().part_at(lo, top, Vec3::NEG_Z), Some(head));
        let _ = size;
        act(&mut ed, &mut view, Action::Part(body));
        let cell = (0..ed.grid().x).flat_map(|x| (0..ed.grid().z).map(move |z| IVec3::new(x, 1, z))).find(|&c| ed.shape().is_some_and(|b| b.get(c) != 0)).unwrap();
        assert_eq!(words(&ed, MenuOn::Voxel(cell, Some(head as u16))), ["PICK COLOR", "ERASE", "SELECT HEAD"]);
        let held = ed.colour;
        ed.colour = [1, 2, 3];
        act(&mut ed, &mut view, Action::PickAt(cell));
        assert_eq!(ed.colour, ed.shape().unwrap().colour_at(cell).unwrap());
        let _ = held;
        let steps = ed.steps();
        act(&mut ed, &mut view, Action::EraseAt(cell));
        assert_eq!((ed.shape().unwrap().get(cell), ed.steps()), (0, steps + 1));
        assert_eq!(words(&ed, MenuOn::Voxel(cell, None)), ["PICK COLOR"], "nothing there to erase");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The figure whole is the first row and the state a figure opens in:
    /// every part solid, none chosen; a part chosen (its row), the whole
    /// again by its row (or Escape, `back_to_level`); its arrows move
    /// every part together, one step, the file shifted alike.
    #[test]
    fn the_whole_figure_is_its_first_row() {
        let dir = figure("whole");
        let file = dir.join("figure.vvx");
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        let mut view = View::new(&ed, &crate::config::GameConfig::default());
        ed.follow_figure();
        assert_eq!((ed.scope, ed.figure.as_ref().unwrap().selected), (Scope::Figure, None), "opened whole");
        assert!(!crate::editor::model::Tool::of(ed.scope, ed.layer).is_empty(), "the tools there: a press on a part works at once");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        assert_eq!(ed.figure.as_ref().unwrap().selected, Some(arm));
        act(&mut ed, &mut view, Action::WholeFigure);
        assert_eq!((ed.scope, ed.figure.as_ref().unwrap().selected), (Scope::Figure, None));
        // Its arrows: every part moves.
        let orbit = crate::editor::view::Orbit { yaw: 20.0, pitch: 25.0, zoom: 60.0, target: Vec3::splat(3.0), fov: crate::editor::view::FOV };
        assert!(ed.part_handles(orbit.view_proj(16.0 / 9.0)).is_some());
        let before = on_disk(&file);
        let steps = ed.steps();
        ed.move_part(IVec3::X);
        let after = on_disk(&file);
        assert_eq!(ed.steps(), steps + 1);
        for (name, (voxels, ..)) in &before {
            assert_eq!(after[name].0, voxels.iter().map(|p| [p[0] + 1, p[1], p[2]]).collect::<Vec<_>>(), "{name}");
        }
    }
}

//! A figure's parts as objects (the owner, shaping a figure of fifteen
//! parts by hand: what anyone expects of an editor): a part rotated at
//! rest about its joint (ROTATE, its rings, three numbers), added, removed,
//! merged into another, duplicated, renamed; the others hidden. Each one
//! step, written as the editor writes a model (every other part's text
//! kept).

use glam::{IVec3, Quat, Vec3};

use super::{Editor, Scope};
use crate::blocks::Block;
use crate::editor::frames::{ModelFile, Placed, Shaped};
use crate::editor::handles::{self, Handles};

/// The words a part's name may be made of: a name the game's code uses
/// (`thing.state.parts.arm`), so a letter, then letters, digits, `_`.
fn is_word(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A model of one frame made a figure: its frame the root part `root`,
/// standing where the frame stood (from the same pivot), so the picture
/// stays as it was. A figure, or a model of several frames, unchanged.
pub(super) fn made_figure(m: &mut ModelFile, root: &str) {
    if m.parted() || m.frames.len() != 1 {
        return;
    }
    let low = -crate::vvx::pivot(m.size);
    m.parts.push(Placed { name: root.to_string(), frame: 0, low, parent: None, joint: None, rotation: [0.0; 3], offset: Vec3::ZERO, ..Placed::default() });
}

/// A part's pose in the editor (POSE): its rotation, degrees as
/// `frame:model` has them, and its offset in voxels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Posed {
    pub turn: [f32; 3],
    pub offset: [f32; 3],
}

impl Editor {
    /// The chosen part's place among the file's parts (`ModelFile::parts`).
    fn placed(&self, p: usize) -> Option<usize> {
        let name = &self.figure.as_ref()?.parts().get(p)?.name;
        self.model.as_ref()?.parts.iter().position(|o| &o.name == name)
    }

    fn chosen_placed(&self) -> Option<usize> {
        self.placed(self.figure.as_ref()?.selected?)
    }

    /// The chosen part's rest rotation (`rotation`, `pitch`, `roll`, the
    /// file's degrees).
    pub fn part_rotation(&self) -> Option<[f32; 3]> {
        Some(self.model.as_ref()?.parts[self.chosen_placed()?].rotation)
    }

    /// The chosen part's own turn as drawn (not its parents'): its rest,
    /// and its pose on it while POSE is lit.
    pub fn own_rest(&self) -> Quat {
        let rest = self.part_rotation().map_or(Quat::IDENTITY, |[r, p, l]| crate::session::turning(r, p, l));
        match self.chosen_pose().filter(|_| self.rigging.posing) {
            Some([r, p, l]) => crate::session::turning(r, p, l) * rest,
            None => rest,
        }
    }

    /// The chosen part's pose's rotation (POSE), 0 where it has none.
    pub fn chosen_pose(&self) -> Option<[f32; 3]> {
        self.chosen_posed().map(|p| p.turn)
    }

    /// The chosen part's pose (POSE): its rotation and offset.
    pub fn chosen_posed(&self) -> Option<Posed> {
        let f = self.figure.as_ref()?;
        let name = &f.parts().get(f.selected?)?.name;
        Some(self.rigging.pose.get(name).copied().unwrap_or_default())
    }

    /// The chosen part's pose's offset along one axis set (POSE's POSITION
    /// fields): drawn, nothing written, no step.
    pub fn set_pose_offset(&mut self, axis: usize, voxels: f32) {
        let Some(name) = self.figure.as_ref().and_then(|f| Some(f.parts().get(f.selected?)?.name.clone())) else { return };
        let entry = self.rigging.pose.entry(name.clone()).or_default();
        entry.offset[axis.min(2)] = (voxels * 10.0).round() / 10.0;
        if *entry == Posed::default() {
            self.rigging.pose.remove(&name);
        }
        self.follow_figure();
    }

    /// What the rings turn: the pose while POSE is lit, else the rest.
    pub fn turn_values(&self) -> Option<[f32; 3]> {
        if self.rigging.posing { self.chosen_pose() } else { self.part_rotation() }
    }

    /// The chosen part's pose about one axis (as `set_part_rotation`'s)
    /// set to `degrees`: drawn, nothing written, no step.
    pub fn set_pose(&mut self, axis: usize, degrees: f32) {
        let Some(name) = self.figure.as_ref().and_then(|f| Some(f.parts().get(f.selected?)?.name.clone())) else { return };
        let to = degrees.rem_euclid(360.0);
        let to = if to > 180.0 { to - 360.0 } else { to };
        let entry = self.rigging.pose.entry(name.clone()).or_default();
        entry.turn[axis.min(2)] = to;
        if *entry == Posed::default() {
            self.rigging.pose.remove(&name);
        }
        self.follow_figure();
    }

    /// POSE on or off (the pose kept); a part chosen shows its rings.
    pub fn set_posing(&mut self, on: bool) {
        self.release();
        self.rigging.posing = on;
        self.follow_figure();
    }

    /// RESET: no pose.
    pub fn reset_pose(&mut self) {
        self.rigging.pose.clear();
        self.follow_figure();
    }

    /// The pose as Lua, `frame:model`'s pose table to copy into a game's
    /// code: the parts posed, in the figure's order, each its turns (0s
    /// left out), whole degrees as whole numbers.
    pub fn pose_lua(&self) -> String {
        let order: Vec<String> = self.figure.as_ref().map_or_else(Vec::new, |f| f.tree.iter().map(|&(p, _)| f.parts()[p].name.clone()).collect());
        let num = |v: f32| if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") };
        let parts: Vec<String> = order
            .iter()
            .filter_map(|name| {
                let posed = self.rigging.pose.get(name)?;
                let mut keys: Vec<String> = ["rotation", "pitch", "roll"].iter().zip(posed.turn).filter(|(_, v)| *v != 0.0).map(|(k, v)| format!("{k} = {}", num(v))).collect();
                if posed.offset != [0.0; 3] {
                    let [x, y, z] = posed.offset.map(num);
                    keys.push(format!("offset = {{ {x}, {y}, {z} }}"));
                }
                (!keys.is_empty()).then(|| format!("{name} = {{ {} }}", keys.join(", ")))
            })
            .collect();
        if parts.is_empty() { "{}".into() } else { format!("{{ {} }}", parts.join(", ")) }
    }

    /// The chosen part's rest rotation about one axis (0 `rotation`, round,
    /// clockwise seen from above; 1 `pitch`; 2 `roll`) set to `degrees`;
    /// a step (one with the last while a ring is dragged).
    pub fn set_part_rotation(&mut self, axis: usize, degrees: f32) {
        let Some(k) = self.chosen_placed() else { return };
        let to = degrees.rem_euclid(360.0);
        let to = if to > 180.0 { to - 360.0 } else { to };
        self.change_model_file(|m| m.parts[k].rotation[axis.min(2)] = to);
        self.follow_figure();
    }

    /// ROTATE: the chosen part's three rings about its joint (in the grid
    /// shown, which is drawn through the part's rest): round the upright
    /// of its parent, its pitch, its roll, as a level's piece has them.
    pub fn rest_rings(&self, view_proj: glam::Mat4) -> Option<Handles> {
        let (figure, (lo, _)) = (self.figure.as_ref()?, self.figure_box()?);
        let p = figure.selected.filter(|_| self.part_mode().is_some())?;
        let [round, pitch, _] = self.turn_values()?;
        let centre = figure.joint(p) + lo.as_vec3();
        let mut h = Handles::at(centre, handles::world_size(view_proj, centre, 0.1));
        let back = self.own_rest().inverse();
        let r = Quat::from_rotation_z(-round.to_radians());
        let tip = Quat::from_rotation_x(pitch.to_radians());
        h.rings = [Some(back * r * Vec3::X), Some(back * r * tip * Vec3::Y), Some(back * Vec3::Z)];
        if let Some(size) = self.shape().map(|b| b.size) {
            let reach = (size.as_vec3() * 0.5).length() + 1.0;
            h.ring = h.ring.max(reach.min(handles::world_size(view_proj, centre, 0.42)));
        }
        Some(h)
    }

    /// The figure's middle left to right, from the pivot: a voxel at x
    /// there mirrors to `middle - x`.
    fn middle_x(&self) -> Option<i32> {
        let f = self.figure.as_ref()?;
        Some(2 * f.low.x + f.model.size.x - 1)
    }

    /// The chosen part's partner of a mirrored pair, if it is in the
    /// figure (its place among the file's parts).
    fn partner_placed(&self, p: usize) -> Option<usize> {
        let name = partner_name(&self.figure.as_ref()?.parts().get(p)?.name)?;
        self.model.as_ref()?.parts.iter().position(|o| o.name == name)
    }

    /// Whether SYMMETRY lands on the chosen part's partner (a pair, both
    /// there) rather than on its own far side.
    pub fn symmetry_on_partner(&self) -> bool {
        self.symmetry && self.figure.as_ref().and_then(|f| f.selected).is_some_and(|p| self.partner_placed(p).is_some())
    }

    /// Why part `p` cannot be mirrored (None: it can): its name says no
    /// side.
    pub fn cannot_mirror(&self, p: usize) -> Option<String> {
        if let Some(why) = self.cannot_clone(p) {
            return Some(why);
        }
        let name = &self.figure.as_ref()?.parts().get(p)?.name;
        partner_name(name).is_none().then(|| format!("Name it {name}_left or {name}_right (or _l, _r) first: its partner is named after it"))
    }

    /// MIRROR: part `p`'s partner (`arm_left`'s `arm_right`) made or made
    /// anew as its mirror left to right across the figure's middle: its
    /// voxels, place, joint and rest rotation (rotation and roll the other
    /// way, pitch alike), its parent the parent's partner (none set while
    /// that is missing) or the same; a step.
    pub fn mirror_part(&mut self, p: usize) {
        self.follow_figure();
        if self.cannot_mirror(p).is_some() {
            return;
        }
        let (Some(k), Some(m), Some(figure), Some(mid)) = (self.placed(p), self.model.as_ref(), self.figure.as_ref(), self.middle_x()) else { return };
        let source = m.parts[k].clone();
        let other = partner_name(&source.name).expect("a pair");
        let cells: std::collections::BTreeMap<[i32; 3], [u8; 3]> = cells_of(m, k).into_iter().map(|([x, y, z], c)| ([mid - x, y, z], c)).collect();
        // Its parent's partner (a right hand on the right arm); a parent
        // of a pair without its partner yet: what the rules find.
        let parent = source.parent.clone().and_then(|q| match partner_name(&q) {
            Some(r) => m.parts.iter().any(|o| o.name == r).then_some(r),
            None => Some(q),
        });
        let joint = source.joint.map(|j| Vec3::new(mid as f32 + 1.0 - j.x, j.y, j.z));
        let [r, pitch, roll] = source.rotation;
        let flip = |v: f32| if v == 0.0 { 0.0 } else { -v };
        let rotation = [flip(r), pitch, flip(roll)];
        let _ = figure;
        self.acting_on(vec![other.clone()]);
        let name = other.clone();
        self.change_model_file(move |m| {
            let k = match m.parts.iter().position(|o| o.name == name) {
                Some(k) => k,
                None => {
                    m.frames.push(Block::boxed(IVec3::ONE));
                    m.parts.push(Placed { name: name.clone(), frame: m.frames.len() - 1, low: IVec3::ZERO, parent: None, joint: None, rotation: [0.0; 3], offset: Vec3::ZERO, ..Placed::default() });
                    m.parts.len() - 1
                }
            };
            set_cells(m, k, &cells);
            let o = &mut m.parts[k];
            (o.parent, o.joint, o.rotation) = (parent, joint, rotation);
            // A copy: the voxels it shows its own.
            (o.like, o.mirror) = (None, None);
            o.offset = Vec3::new(-source.offset.x, source.offset.y, source.offset.z);
        });
        self.follow_figure();
    }

    /// With SYMMETRY on one of a pair: what the stroke changed in the
    /// chosen part (since `before`) changed in its partner too, mirrored
    /// across the figure's middle (its box grown to hold it).
    pub(super) fn land_on_partner(&mut self, before: &Shaped) {
        if !self.symmetry_on_partner() {
            return;
        }
        let (Some(p), Some(mid)) = (self.figure.as_ref().and_then(|f| f.selected), self.middle_x()) else { return };
        let (Some(k), Some(j)) = (self.placed(p), self.partner_placed(p)) else { return };
        let Some(m) = self.model.as_ref() else { return };
        // Partners whose voxels are one (a clone of the other, or both of
        // one source): the link already shows the stroke on both.
        if source_of(m, k) == source_of(m, j) {
            return;
        }
        let mut old = m.clone();
        old.reshape(before);
        let (was, now) = (cells_of(&old, k), cells_of(m, k));
        let mut theirs = cells_of(m, j);
        let mut changed = false;
        for c in was.keys().chain(now.keys()) {
            if was.get(c) == now.get(c) {
                continue;
            }
            let there = [mid - c[0], c[1], c[2]];
            match now.get(c) {
                Some(&colour) => theirs.insert(there, colour),
                None => theirs.remove(&there),
            };
            changed = true;
        }
        if changed && let Some(m) = self.model.as_mut() {
            set_cells(m, j, &theirs);
        }
    }

    /// Part `p` hung on part `q` (its row dropped on q's), where it stands:
    /// its rest rotation, place, offset and joint made anew so that it is
    /// drawn exactly as before, only what it turns with changes; the
    /// others keep their rig (`pin_rig`); one step. Refused (why, in a few
    /// words): on the figure whole (`usize::MAX`), on itself, on a part
    /// that hangs on it, the root.
    pub fn reparent(&mut self, p: usize, q: usize) -> Result<(), String> {
        self.follow_figure();
        let figure = self.figure.as_ref().ok_or("no figure")?;
        let name = |k: usize| figure.parts().get(k).map_or(String::new(), |o| o.name.clone());
        if q == usize::MAX {
            return Err(format!("{} hangs on a part: the figure whole is none", name(p)));
        }
        if p == q {
            return Err(format!("{} cannot hang on itself", name(p)));
        }
        if figure.parts().get(p).is_none_or(|o| o.parent.is_none()) {
            return Err(format!("{} is the root: the others hang on it", name(p)));
        }
        // On a part that hangs on it, by hand or by the rules: a ring.
        let mut up = Some(q);
        while let Some(k) = up {
            if k == p {
                return Err(format!("{} hangs on {}: that would make a ring", name(q), name(p)));
            }
            up = figure.parts()[k].parent;
        }
        if figure.parts()[p].parent == Some(q) {
            return Ok(());
        }
        // Where it is drawn now (its chain), and the new parent's chain:
        // its own turn and place under the new parent such that the two
        // agree (`session::part_chain`'s sum, in the model's own frame).
        let (a_old, b_old) = crate::session::part_at_rest(&figure.model, p);
        let (turn_q, shift_q) = crate::session::part_at_rest(&figure.model, q);
        let own = (turn_q.inverse() * a_old).normalize();
        let t = turn_q.inverse() * (b_old - shift_q);
        let j = figure.model.parts[p].joint;
        let v = t - j + own * j;
        let whole = v.round();
        let rest = ((v - whole) * 10000.0).round() / 10000.0;
        let joint = figure.file_joint(p) + whole;
        let rotation = euler(own);
        let (from, to) = (name(p), name(q));
        let whole = whole.as_ivec3();
        let k = self.placed(p).ok_or("no such part")?;
        self.acting_on(vec![from.clone()]);
        self.change_model_file(|m| {
            let o = &mut m.parts[k];
            o.low += whole;
            o.offset = rest;
            o.joint = Some(joint);
            o.rotation = rotation;
            o.parent = Some(to.clone());
        });
        self.follow_figure();
        self.hint = Some(format!("{from} hangs on {to} now"));
        Ok(())
    }

    /// A free name for a part: `base`, else `base_2`, `base_3` ...
    pub(super) fn free_part_name(&self, base: &str) -> String {
        let taken = |n: &str| self.model.as_ref().is_some_and(|m| m.parts.iter().any(|o| o.name == n));
        let base = base.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches('_');
        let base = if base.is_empty() { "part" } else { base };
        if !taken(base) {
            return base.to_string();
        }
        (2..).map(|k| format!("{base}_{k}")).find(|n| !taken(n)).expect("a free name")
    }

    /// The part called `name` chosen (shaped), in MOVE.
    pub(super) fn choose_named(&mut self, name: &str) {
        self.follow_figure();
        let Some(f) = self.model.as_ref().and_then(|m| m.parts.iter().find(|o| o.name == name)).map(|o| o.frame) else { return };
        self.release();
        self.scope = Scope::Frame(f);
        self.rigging.part_rotating = false;
        self.follow_figure();
    }

    /// Why the model cannot have parts (None: it can): a figure has them, a
    /// model of one frame becomes one (its frame the root part), a model
    /// of several frames (an animation) not.
    pub fn cannot_have_parts(&self) -> Option<&'static str> {
        match &self.model {
            None => Some("Only in a model"),
            Some(m) if m.parted() => None,
            Some(m) if m.frames.len() == 1 => None,
            Some(_) => Some("Frames and parts do not mix: a model of several frames is an animation"),
        }
    }

    /// The name a model of one frame gives its root part as it becomes a
    /// figure: its file's (`car` of `car.vvx`) where that is a part's name,
    /// else `body`.
    pub(super) fn root_name(&self) -> String {
        let stem = std::path::Path::new(&self.file).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
        if is_word(&stem) { stem } else { "body".to_string() }
    }

    /// ADD PART: a new part of one voxel of the color held, set on top of
    /// the chosen part (on its top voxel nearest its middle, where it is
    /// seen), else on top of the figure (its highest part), hanging on that
    /// part; chosen, its name the next free `part`; a step. (A part with no
    /// voxel would not be in the file, nor seen.) A model of one frame
    /// becomes a figure in the same step, its frame the root part
    /// (`made_figure`).
    pub fn add_part(&mut self) {
        if self.cannot_have_parts().is_some() {
            return;
        }
        self.follow_figure();
        let chosen = self.figure.as_ref().and_then(|f| f.selected).filter(|_| !self.rigging.painting_whole);
        let root = self.root_name();
        let Some(m) = self.model.as_ref() else { return };
        // (A model of one frame: as the figure it becomes.)
        let figure;
        let m = if m.parted() {
            m
        } else {
            let mut made = m.clone();
            made_figure(&mut made, &root);
            figure = made;
            &figure
        };
        // Each part's top voxel nearest its box's middle (from the pivot).
        let top = |k: usize| -> Option<IVec3> {
            let o = &m.parts[k];
            let b = &m.frames[o.frame];
            let middle = (b.size.truncate().as_vec2() - 1.0) / 2.0;
            (0..b.voxels.len())
                .map(|i| b.at(i))
                .filter(|&c| b.get(c) != 0)
                .max_by(|a, c| a.z.cmp(&c.z).then_with(|| (c.truncate().as_vec2() - middle).length_squared().total_cmp(&(a.truncate().as_vec2() - middle).length_squared())))
                .map(|c| c + o.low)
        };
        // (On an empty part: in the cell it stands in.)
        let on = match chosen.and_then(|p| self.placed(p)) {
            Some(k) if m.parts[k].empty => Some((k, m.parts[k].low - IVec3::Z)),
            Some(k) => top(k).map(|c| (k, c)),
            None => (0..m.parts.len()).filter(|&k| !m.parts[k].empty).filter_map(|k| top(k).map(|c| (k, c))).max_by_key(|&(_, c)| c.z),
        };
        let Some((k, below)) = on else { return };
        let name = self.free_part_name("part");
        let colour = self.colour;
        let (parent, at) = (m.parts[k].name.clone(), below + IVec3::Z);
        let new = name.clone();
        self.acting_on(vec![name.clone()]);
        self.change_model_file(move |m| {
            made_figure(m, &root);
            let mut voxel = Block::boxed(IVec3::ONE);
            let _ = voxel.set(IVec3::ZERO, Some(colour));
            m.frames.push(voxel);
            m.parts.push(Placed { name: new, frame: m.frames.len() - 1, low: at, parent: Some(parent), joint: None, rotation: [0.0; 3], offset: Vec3::ZERO, ..Placed::default() });
        });
        self.choose_named(&name);
    }

    /// ADD EMPTY (+ EMPTY): a new empty part (no voxels, a node the others
    /// hang on), named `name` where that is a free word, else the next
    /// free `group`; chosen, a step. On the part chosen: hanging on it,
    /// standing where its joint is (turning there). The figure whole
    /// chosen: on the root, at the pivot, where the root is empty already;
    /// else the new root at the pivot, the old root hanging on it where it
    /// stands, turning where it turned. A model of one frame becomes a
    /// figure in the same step: the empty part its root, the frame a part
    /// hanging on it (`made_figure`).
    pub fn add_empty(&mut self, name: Option<&str>) {
        if self.cannot_have_parts().is_some() {
            return;
        }
        self.follow_figure();
        let chosen = self.figure.as_ref().and_then(|f| f.selected).filter(|_| !self.rigging.painting_whole);
        let root = self.root_name();
        let Some(m) = self.model.as_ref() else { return };
        let one_frame = !m.parted();
        let taken = |n: &str| m.parts.iter().any(|o| o.name == n) || (one_frame && n == root);
        let name = match name.map(str::trim).filter(|n| is_word(n) && !taken(n)) {
            Some(n) => n.to_string(),
            None => {
                let base = self.free_part_name(name.map(str::trim).filter(|n| is_word(n)).unwrap_or("group"));
                if one_frame && base == root { self.free_part_name(&format!("{base}_2")) } else { base }
            }
        };
        // Where it hangs and stands (from the pivot), its joint where that
        // is not its cell's middle, and the old root it takes the place of
        // (with the joint it turned about).
        type Under = Option<(String, Vec3)>;
        let (parent, at, joint, under): (Option<String>, IVec3, Option<Vec3>, Under) = match (chosen, self.figure.as_ref()) {
            (Some(p), Some(f)) => {
                let j = f.file_joint(p);
                let at = j.floor().as_ivec3();
                (Some(f.parts()[p].name.clone()), at, (j != at.as_vec3() + 0.5).then_some(j), None)
            }
            (None, Some(f)) => {
                let r = f.tree.first().map_or(0, |&(r, _)| r);
                match f.parts()[r].empty {
                    Some(_) => (Some(f.parts()[r].name.clone()), IVec3::ZERO, None, None),
                    None => (None, IVec3::ZERO, None, Some((f.parts()[r].name.clone(), f.file_joint(r)))),
                }
            }
            // A model of one frame: the frame the part under it.
            _ => (None, IVec3::ZERO, None, Some((root.clone(), Vec3::ZERO))),
        };
        let new = name.clone();
        let acted = std::iter::once(name.clone()).chain(under.iter().map(|(n, _)| n.clone())).collect();
        self.acting_on(acted);
        self.change_model_file(move |m| {
            let one_frame = !m.parted();
            made_figure(m, &root);
            if let Some((old, joint)) = &under
                && let Some(o) = m.parts.iter_mut().find(|o| &o.name == old)
            {
                o.parent = Some(new.clone());
                // (Turning where it turned as the root; a frame just made a
                // part turns where the empty part does.)
                if !one_frame && o.joint.is_none() {
                    o.joint = Some(*joint);
                }
            }
            m.frames.push(Block::boxed(IVec3::ONE));
            m.parts.push(Placed { name: new, frame: m.frames.len() - 1, low: at, parent, joint, empty: true, ..Placed::default() });
        });
        self.choose_named(&name);
    }

    /// Whether part `p` is empty (no voxels: `vox::Part::empty`).
    pub fn part_empty(&self, p: usize) -> bool {
        self.figure.as_ref().and_then(|f| f.parts().get(p)).is_some_and(|o| o.empty.is_some())
    }

    /// Whether the part shaped now is an empty one: no tool sets a voxel
    /// in it.
    pub fn shaping_empty(&self) -> bool {
        let (Some(f), Some(m)) = (self.part_mode(), self.model.as_ref()) else { return false };
        m.parts.iter().any(|o| o.frame == f && o.empty)
    }

    /// Why part `p` cannot be cloned (None: it can): an empty part has no
    /// voxels to share.
    pub fn cannot_clone(&self, p: usize) -> Option<String> {
        self.part_empty(p).then(|| "An empty part has no voxels to share".to_string())
    }

    /// Why part `p` cannot be removed (None: it can): the root part while
    /// others hang on it, the last part, the last part with voxels.
    pub fn cannot_remove(&self, p: usize) -> Option<&'static str> {
        let figure = self.figure.as_ref()?;
        if figure.parts().len() < 2 {
            return Some("A figure keeps one part at least");
        }
        if figure.parts()[p].empty.is_none() && figure.parts().iter().enumerate().all(|(q, o)| q == p || o.empty.is_some()) {
            return Some("A figure keeps one part with voxels at least");
        }
        let hangs = figure.parts().iter().any(|o| o.parent == Some(p));
        (figure.parts()[p].parent.is_none() && hangs).then_some("The root part: the others hang on it (merge into it, or hang them elsewhere first)")
    }

    /// The parts hanging on part `p` hung on `to` (by hand), each where it
    /// stands, its joint kept where it is: what `remove_part` and
    /// `merge_part` leave them with.
    fn rehang(&self, p: usize, to: Option<String>) -> Vec<(String, Option<String>, Vec3)> {
        let Some(figure) = &self.figure else { return Vec::new() };
        let low = figure.low.as_vec3();
        (0..figure.parts().len()).filter(|&c| figure.parts()[c].parent == Some(p)).map(|c| (figure.parts()[c].name.clone(), to.clone(), figure.joint(c) + low)).collect()
    }

    /// REMOVE: part `p` and its voxels; what hung on it hangs on its
    /// parent, where it stands; the figure whole chosen; a step.
    pub fn remove_part(&mut self, p: usize) {
        self.follow_figure();
        if self.cannot_remove(p).is_some() {
            return;
        }
        let (Some(k), Some(figure)) = (self.placed(p), &self.figure) else { return };
        let parent = figure.parts()[p].parent.map(|q| figure.parts()[q].name.clone());
        let gone = figure.parts()[p].name.clone();
        let kids = self.rehang(p, parent);
        self.release();
        self.scope = Scope::Figure;
        self.acting_on(std::iter::once(gone).chain(kids.iter().map(|(n, ..)| n.clone())).collect());
        self.change_model_file(move |m| {
            // Its clones keep showing what they showed: the first one the
            // source now.
            let name = m.parts[k].name.clone();
            m.hand_down(&name);
            let gone = m.parts.remove(k);
            m.frames.remove(gone.frame);
            for o in &mut m.parts {
                if o.frame > gone.frame {
                    o.frame -= 1;
                }
            }
            for (name, to, joint) in &kids {
                if let Some(o) = m.parts.iter_mut().find(|o| &o.name == name) {
                    (o.parent, o.joint) = (to.clone(), Some(*joint));
                }
            }
        });
        self.follow_figure();
    }

    /// Where part `src`'s voxels lie in part `into`'s own grid as they are
    /// seen at rest (each turned with its rest rotation and its parents'):
    /// a map of cells from the pivot, when that lands voxel on voxel (the
    /// same rotation about the same joint, or quarter turns of each
    /// other); None when it does not.
    fn merge_map(&self, src: usize, into: usize) -> Option<impl Fn(IVec3) -> IVec3 + use<>> {
        let figure = self.figure.as_ref()?;
        let m = figure.rest_map(into, IVec3::ZERO).inverse() * figure.rest_map(src, IVec3::ZERO);
        let near = |v: f32| (v - v.round()).abs() < 0.01;
        // Its linear part a signed permutation, a voxel's middle on one.
        let cols = [m.x_axis.truncate(), m.y_axis.truncate(), m.z_axis.truncate()];
        let whole = cols.iter().all(|c| (0..3).all(|a| near(c[a])) && (c.abs().element_sum() - 1.0).abs() < 0.01);
        let middle = m.transform_point3(Vec3::splat(0.5)) - 0.5;
        if !whole || !(0..3).all(|a| near(middle[a])) {
            return None;
        }
        let low = figure.low;
        Some(move |c: IVec3| (m.transform_point3((c - low).as_vec3() + 0.5) - 0.5).round().as_ivec3() + low)
    }

    /// Why part `src` cannot be merged into `into` (None: it can): the same
    /// part; turned at rest off `into`'s grid (voxels cannot be laid
    /// between another part's); turned otherwise than it with parts hanging
    /// on it (they would turn otherwise).
    pub fn cannot_merge(&self, src: usize, into: usize) -> Option<&'static str> {
        let (a, b) = (self.placed(src)?, self.placed(into)?);
        let m = self.model.as_ref()?;
        if a == b {
            return Some("A part cannot be merged into itself");
        }
        if m.parts[a].empty || m.parts[b].empty {
            return Some("An empty part has no voxels to merge: REMOVE it, or hang the parts elsewhere");
        }
        let alike = m.parts[a].rotation == m.parts[b].rotation;
        if !alike && self.merge_map(src, into).is_none() {
            return Some("Its rest rotation lies off the other part's grid: rotate it by quarter turns of it first");
        }
        let hangs = self.figure.as_ref().is_some_and(|f| f.parts().iter().any(|o| o.parent == Some(src)));
        (!alike && hangs).then_some("Rotated otherwise than the other part, with parts hanging on it: those would turn otherwise")
    }

    /// MERGE: part `src`'s voxels into part `into` (where both have one,
    /// `into`'s stays) as they are seen (a part turned at rest by quarter
    /// turns of `into` laid into its grid so; else where the file has
    /// them); `into` keeps its name, parent and joint (a joint the rules
    /// found stated, where `into` is rotated: its voxels change); what
    /// hung on `src` hangs on `into`; `into` chosen; a step.
    pub fn merge_part(&mut self, src: usize, into: usize) {
        self.follow_figure();
        if self.cannot_merge(src, into).is_some() {
            return;
        }
        let (Some(a), Some(b), Some(figure)) = (self.placed(src), self.placed(into), &self.figure) else { return };
        let target = figure.parts()[into].name.clone();
        let gone = figure.parts()[src].name.clone();
        let joint = figure.file_joint(into);
        let rooted = figure.parts()[into].parent.is_none();
        let kids: Vec<_> = self.rehang(src, Some(target.clone())).into_iter().filter(|(n, ..)| *n != target).collect();
        let seen = self.merge_map(src, into);
        self.release();
        let acted = [gone, target.clone()].into_iter().chain(kids.iter().map(|(n, ..)| n.clone())).collect();
        self.acting_on(acted);
        self.change_model_file(move |m| {
            // The part merged away hands its voxels down to its clones (the
            // first the source); the part merged into keeps the voxels it
            // shows as its own: a clone is no clone any more.
            let name = m.parts[a].name.clone();
            m.hand_down(&name);
            (m.parts[b].like, m.parts[b].mirror) = (None, None);
            let (from, to) = (m.parts[a].clone(), m.parts[b].clone());
            let (fb, tb) = (&m.frames[from.frame], &m.frames[to.frame]);
            let cells = |block: &Block, low: IVec3| (0..block.voxels.len()).filter_map(|k| block.colour_at(block.at(k)).map(|c| (block.at(k) + low, c))).collect::<Vec<_>>();
            let mine = cells(tb, to.low);
            let theirs: Vec<_> = cells(fb, from.low).into_iter().map(|(c, colour)| (seen.as_ref().map_or(c, |f| f(c)), colour)).collect();
            // The box holding both, from the pivot.
            let (mut lo, mut hi) = (to.low, to.low + tb.size - 1);
            for (c, _) in mine.iter().chain(&theirs) {
                (lo, hi) = (lo.min(*c), hi.max(*c));
            }
            let mut merged = Block::boxed(hi - lo + 1);
            for (c, colour) in &mine {
                let _ = merged.set(*c - lo, Some(*colour));
            }
            for (c, colour) in &theirs {
                if merged.get(*c - lo) == 0 {
                    let _ = merged.set(*c - lo, Some(*colour));
                }
            }
            m.frames[to.frame] = merged;
            m.parts[b].low = lo;
            if to.rotation != [0.0; 3] && !rooted && m.parts[b].joint.is_none() {
                m.parts[b].joint = Some(joint);
            }
            let gone = m.parts.remove(a);
            m.frames.remove(gone.frame);
            for o in &mut m.parts {
                if o.frame > gone.frame {
                    o.frame -= 1;
                }
                if o.parent.as_deref() == Some(gone.name.as_str()) {
                    o.parent = Some(to.name.clone());
                }
            }
            for (name, to, joint) in &kids {
                if let Some(o) = m.parts.iter_mut().find(|o| &o.name == name) {
                    (o.parent, o.joint) = (to.clone(), Some(*joint));
                }
            }
        });
        self.choose_named(&target);
    }

    /// DUPLICATE: a copy of part `p` where it stands (its voxels, parent,
    /// joint, rest rotation), named after it (`arm_2`), chosen in MOVE to
    /// be dragged away; a step.
    pub fn duplicate_part(&mut self, p: usize) {
        self.follow_figure();
        let (Some(k), Some(figure)) = (self.placed(p), &self.figure) else { return };
        let parent = figure.parts()[p].parent.map(|q| figure.parts()[q].name.clone());
        let name = self.free_part_name(&figure.parts()[p].name);
        let new = name.clone();
        self.acting_on(vec![name.clone()]);
        self.change_model_file(move |m| {
            let mut copy = m.parts[k].clone();
            m.frames.push(m.frames[copy.frame].clone());
            (copy.name, copy.frame, copy.parent) = (new, m.frames.len() - 1, parent);
            // A copy: the voxels it shows its own.
            (copy.like, copy.mirror) = (None, None);
            m.parts.push(copy);
        });
        self.choose_named(&name);
    }

    /// The source and mirror a clone of part `p` takes to show what `p`
    /// shows (`flip`: mirrored in x on top): `p`'s own name and the flip,
    /// or for a clone its source's and the mirrors put together. None
    /// where that takes two mirrors (a clone mirrored in y, mirrored in x).
    fn link_to_show(&self, p: usize, flip: bool) -> Option<(String, Option<usize>)> {
        let o = &self.model.as_ref()?.parts[self.placed(p)?];
        let source = o.like.clone().unwrap_or_else(|| o.name.clone());
        match (o.mirror, flip) {
            (m, false) => Some((source, m)),
            (None, true) => Some((source, Some(0))),
            (Some(0), true) => Some((source, None)),
            (Some(_), true) => None,
        }
    }

    /// Why part `p` cannot be cloned mirrored (None: it can): a clone
    /// mirrored along y or z (two mirrors), a pair's partner already there.
    pub fn cannot_clone_mirrored(&self, p: usize) -> Option<String> {
        if let Some(why) = self.cannot_clone(p) {
            return Some(why);
        }
        let name = &self.figure.as_ref()?.parts().get(p)?.name;
        if self.link_to_show(p, true).is_none() {
            return Some(format!("{name} is mirrored along another axis: a clone of it mirrored left to right would need two mirrors (MAKE UNIQUE first)"));
        }
        let other = partner_name(name)?;
        self.model.as_ref()?.parts.iter().any(|o| o.name == other).then(|| format!("{other} is there already: its partner's name is taken"))
    }

    /// CLONE: a part like part `p` (its voxels shared: a stroke on either
    /// shows on both) where it stands, named after it as DUPLICATE names
    /// its copy, chosen in MOVE; with `mirrored` (CLONE MIRRORED) mirrored
    /// left to right and set across the figure's middle as MIRROR sets a
    /// partner (place, joint, rest rotation, offset; a pair's partner's
    /// name, `arm_left`'s `arm_right`); a clone of a clone is like its
    /// source. A step.
    pub fn clone_part(&mut self, p: usize, mirrored: bool) {
        self.follow_figure();
        if self.cannot_clone(p).is_some() || mirrored && self.cannot_clone_mirrored(p).is_some() {
            return;
        }
        let (Some(k), Some(figure), Some(mid), Some((like, mirror))) = (self.placed(p), &self.figure, self.middle_x(), self.link_to_show(p, mirrored)) else { return };
        let own = figure.parts()[p].name.clone();
        let parent = figure.parts()[p].parent.map(|q| figure.parts()[q].name.clone());
        let name = match partner_name(&own).filter(|_| mirrored) {
            Some(other) => other,
            None => self.free_part_name(&own),
        };
        // Across the middle: a parent of a pair hangs it on its partner.
        let parent = match parent {
            Some(q) if mirrored => match partner_name(&q) {
                Some(r) if self.model.as_ref().is_some_and(|m| m.parts.iter().any(|o| o.name == r)) => Some(r),
                Some(_) => None,
                None => Some(q),
            },
            other => other,
        };
        let new = name.clone();
        self.acting_on(vec![name.clone()]);
        self.change_model_file(move |m| {
            let source = m.parts[k].clone();
            let shows = &m.frames[source.frame];
            let mut copy = source.clone();
            copy.name = new;
            copy.parent = parent;
            (copy.like, copy.mirror) = (Some(like), mirror);
            let frame = if mirrored {
                // Each voxel at x across the middle at `mid - x`.
                copy.low.x = mid - source.low.x - shows.size.x + 1;
                copy.joint = source.joint.map(|j| Vec3::new(mid as f32 + 1.0 - j.x, j.y, j.z));
                let [r, pitch, roll] = source.rotation;
                let flip = |v: f32| if v == 0.0 { 0.0 } else { -v };
                copy.rotation = [flip(r), pitch, flip(roll)];
                copy.offset.x = -source.offset.x;
                crate::editor::frames::mirrored(shows, Some(0))
            } else {
                shows.clone()
            };
            m.frames.push(frame);
            copy.frame = m.frames.len() - 1;
            m.parts.push(copy);
        });
        self.choose_named(&name);
    }

    /// Part `p`'s link: its source's name and the axis it is mirrored
    /// along (None: a part with voxels of its own).
    pub fn part_link(&self, p: usize) -> Option<(String, Option<usize>)> {
        let o = &self.model.as_ref()?.parts[self.placed(p)?];
        Some((o.like.clone()?, o.mirror))
    }

    /// The parts part `p` can be like (LIKE's list): those with voxels of
    /// their own but itself; none for a part that has clones (they would
    /// be clones of a clone).
    pub fn possible_sources(&self, p: usize) -> Vec<String> {
        let (Some(m), Some(k)) = (self.model.as_ref(), self.placed(p)) else { return Vec::new() };
        if !m.clones_of(&m.parts[k].name).is_empty() {
            return Vec::new();
        }
        if m.parts[k].empty {
            return Vec::new();
        }
        m.parts.iter().enumerate().filter(|(i, o)| *i != k && o.like.is_none() && !o.empty).map(|(_, o)| o.name.clone()).collect()
    }

    /// Why part `p` cannot be linked to another (None: it can).
    pub fn cannot_link(&self, p: usize) -> Option<String> {
        let (m, k) = (self.model.as_ref()?, self.placed(p)?);
        let name = &m.parts[k].name;
        if m.parts[k].empty {
            return Some("An empty part has no voxels: it is like no other part".to_string());
        }
        if !m.clones_of(name).is_empty() {
            return Some(format!("Parts are like {name}: a source cannot be a clone (MAKE UNIQUE them first)"));
        }
        self.possible_sources(p).is_empty().then(|| "No other part has voxels of its own".to_string())
    }

    /// LIKE: part `p` made like part `source` (its own voxels thrown away;
    /// its box's low corner stays where it was, its mirror kept), or with
    /// None its voxels as it shows them its own (MAKE UNIQUE). A step.
    pub fn link_part(&mut self, p: usize, source: Option<&str>) {
        self.follow_figure();
        let Some(k) = self.placed(p) else { return };
        if let Some(s) = source
            && !self.possible_sources(p).iter().any(|o| o == s)
        {
            return;
        }
        let source = source.map(str::to_string);
        let name = self.model.as_ref().map(|m| m.parts[k].name.clone()).unwrap_or_default();
        self.acting_on(vec![name]);
        self.change_model_file(move |m| {
            let o = m.parts[k].clone();
            match source {
                None => (m.parts[k].like, m.parts[k].mirror) = (None, None),
                Some(s) => {
                    let Some(from) = m.part_named(&s) else { return };
                    m.frames[o.frame] = crate::editor::frames::mirrored(&m.frames[m.parts[from].frame], o.mirror.filter(|_| o.like.is_some()));
                    m.parts[k].mirror = o.mirror.filter(|_| o.like.is_some());
                    m.parts[k].like = Some(s);
                }
            }
        });
        self.follow_figure();
    }

    /// MIRROR (a clone's row): its source's voxels mirrored along `axis`
    /// (0 x, 1 y, 2 z) or not at all; its box stays where it is. A step.
    pub fn set_link_mirror(&mut self, p: usize, axis: Option<usize>) {
        self.follow_figure();
        let Some((source, now)) = self.part_link(p) else { return };
        let Some(k) = self.placed(p) else { return };
        if now == axis {
            return;
        }
        let name = self.model.as_ref().map(|m| m.parts[k].name.clone()).unwrap_or_default();
        self.acting_on(vec![name]);
        self.change_model_file(move |m| {
            let Some(from) = m.part_named(&source) else { return };
            let f = m.parts[k].frame;
            m.frames[f] = crate::editor::frames::mirrored(&m.frames[m.parts[from].frame], axis);
            m.parts[k].mirror = axis;
        });
        self.follow_figure();
    }

    /// RENAME: part `p` called `name` (a word the game's code can use,
    /// unique); what hangs on it by hand follows the name; a step. Why not,
    /// in a few words.
    pub fn rename_part(&mut self, p: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        let Some(k) = self.placed(p) else { return Err("no such part".into()) };
        let old = self.model.as_ref().map(|m| m.parts[k].name.clone()).unwrap_or_default();
        if name == old {
            return Ok(());
        }
        if !is_word(name) {
            return Err("A part's name is a word: letters, digits and _, a letter first".into());
        }
        if self.model.as_ref().is_some_and(|m| m.parts.iter().any(|o| o.name == name)) {
            return Err(format!("Another part is called {name}"));
        }
        let new = name.to_string();
        // (Names change: nothing to compare, nothing else moves.)
        let all = self.model.as_ref().map_or(Vec::new(), |m| m.parts.iter().map(|o| o.name.clone()).chain([name.to_string()]).collect());
        self.acting_on(all);
        self.change_model_file(move |m| {
            m.parts[k].name = new.clone();
            for o in &mut m.parts {
                if o.parent.as_deref() == Some(old.as_str()) {
                    o.parent = Some(new.clone());
                }
                if o.like.as_deref() == Some(old.as_str()) {
                    o.like = Some(new.clone());
                }
            }
        });
        self.follow_figure();
        Ok(())
    }

    /// A press from the figure whole on part `p`: the stroke works on that
    /// part (its grid, as it is drawn), while the figure stays chosen and
    /// nothing is dimmed (`end_whole_stroke` at the let-go).
    pub fn begin_whole_stroke(&mut self, p: usize) {
        let Some(f) = self.figure.as_ref().and_then(|fig| fig.frames.get(p).copied()) else { return };
        self.release();
        self.scope = Scope::Frame(f);
        self.rigging.painting_whole = true;
        self.follow_figure();
    }

    /// The stroke from the figure whole let go of: the figure whole again
    /// (a box set stays as set; its depth is no more to change).
    pub fn end_whole_stroke(&mut self) {
        if !self.rigging.painting_whole {
            return;
        }
        self.rigging.painting_whole = false;
        self.forms.fresh = None;
        self.scope = Scope::Figure;
        self.follow_figure();
    }

    /// While a part's ring is dragged: by single degrees (Ctrl held) or by
    /// fives.
    pub fn set_fine(&mut self, fine: bool) {
        if let Some(d) = &mut self.rigging.handle_drag {
            d.fine = fine;
        }
    }

    /// HIDE OTHERS: every part but `p` left out of the picture.
    pub fn hide_other_parts(&mut self, p: usize) {
        if let Some(f) = &mut self.figure {
            for (q, h) in f.hidden.iter_mut().enumerate() {
                *h = q != p;
            }
        }
    }
}

/// A turn as a part's rest rotation: `rotation`, `pitch`, `roll` in
/// degrees as `session::turning` takes them (Rz(-rotation) Rx(pitch)
/// Ry(roll)), to a thousandth.
pub fn euler(q: Quat) -> [f32; 3] {
    let m = glam::Mat3::from_quat(q);
    let at = |row: usize, col: usize| m.col(col)[row];
    let pitch = at(2, 1).clamp(-1.0, 1.0).asin();
    let (roll, round) = if pitch.cos().abs() > 1e-5 {
        ((-at(2, 0)).atan2(at(2, 2)), (-at(0, 1)).atan2(at(1, 1)))
    } else {
        // Straight up or down: the roll folded into the round.
        (0.0, at(1, 0).atan2(at(0, 0)))
    };
    let deg = |r: f32| {
        let d = (r.to_degrees() * 1000.0).round() / 1000.0;
        if d == 0.0 { 0.0 } else { d }
    };
    [deg(-round), deg(pitch), deg(roll)]
}

/// The other of a mirrored pair by name: `NAME_left` and `NAME_right`,
/// `NAME_l` and `NAME_r` (None: not one of a pair).
pub fn partner_name(name: &str) -> Option<String> {
    for (a, b) in [("_left", "_right"), ("_l", "_r")] {
        if let Some(stem) = name.strip_suffix(a).filter(|s| !s.is_empty()) {
            return Some(format!("{stem}{b}"));
        }
        if let Some(stem) = name.strip_suffix(b).filter(|s| !s.is_empty()) {
            return Some(format!("{stem}{a}"));
        }
    }
    None
}

/// The name of the part whose voxels part `k` shows: its source, or its
/// own.
fn source_of(m: &ModelFile, k: usize) -> &str {
    m.parts[k].like.as_deref().unwrap_or(&m.parts[k].name)
}

/// A part's voxels by where they lie from the pivot.
fn cells_of(m: &ModelFile, k: usize) -> std::collections::BTreeMap<[i32; 3], [u8; 3]> {
    let (o, b) = (&m.parts[k], &m.frames[m.parts[k].frame]);
    (0..b.voxels.len()).filter_map(|i| b.colour_at(b.at(i)).map(|c| ((b.at(i) + o.low).to_array(), c))).collect()
}

/// Part `k`'s voxels set to `cells` (from the pivot), its box the least
/// that holds them (one voxel at least where none is left).
fn set_cells(m: &mut ModelFile, k: usize, cells: &std::collections::BTreeMap<[i32; 3], [u8; 3]>) {
    let (lo, hi) = cells.keys().map(|&c| IVec3::from(c)).fold((IVec3::MAX, IVec3::MIN), |(lo, hi), c| (lo.min(c), hi.max(c)));
    let (lo, hi) = if cells.is_empty() { (m.parts[k].low, m.parts[k].low) } else { (lo, hi) };
    let mut block = Block::boxed(hi - lo + 1);
    for (&c, &colour) in cells {
        let _ = block.set(IVec3::from(c) - lo, Some(colour));
    }
    let f = m.parts[k].frame;
    m.frames[f] = block;
    m.parts[k].low = lo;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use crate::editor::view::{Action, View, act};

    /// A figure: a body (the root), a head and an arm on it, a hand on the
    /// arm (by hand), each a box of one color.
    pub(crate) fn figure(name: &str) -> (Editor, View, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("parts-{name}"));
        let part = |name: &str, at: [i32; 3], size: [i32; 3], sign: char, parent: Option<&str>| {
            let rows: Vec<String> = (0..size[2]).map(|_| (0..size[1]).map(|_| sign.to_string().repeat(size[0] as usize)).collect::<Vec<_>>().join("\n")).collect();
            let parent = parent.map_or(String::new(), |p| format!("parent = \"{p}\"\n"));
            format!("\n[[part]]\nname = \"{name}\"\nat = [{}, {}, {}]\n{parent}layers = \"\"\"\n{}\n\"\"\"\n", at[0], at[1], at[2], rows.join("\n\n"))
        };
        let text = format!(
            "format = \"{}\"\nsize = [9, 4, 11]\n\n[palette]\na = \"#5a7aa8\"\nb = \"#d0c8b8\"\nc = \"#c89a4a\"\n{}{}{}{}",
            crate::vvx::FORMAT,
            part("body", [-3, -2, 0], [6, 4, 8], 'a', None),
            part("head", [-2, -2, 8], [4, 4, 3], 'b', None),
            part("arm", [3, -1, 3], [2, 2, 5], 'c', None),
            part("hand", [3, -1, 1], [2, 2, 2], 'c', Some("arm")),
        );
        std::fs::write(dir.join("fig.vvx"), text).unwrap();
        let mut ed = Editor::open_model(&dir, "fig.vvx").unwrap();
        ed.follow_figure();
        let view = View::new(&ed, &crate::config::GameConfig::default());
        (ed, view, dir)
    }

    pub(crate) fn part(ed: &Editor, name: &str) -> usize {
        ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == name).unwrap_or_else(|| panic!("no part {name}"))
    }

    fn names(ed: &Editor) -> Vec<String> {
        ed.model.as_ref().unwrap().parts.iter().map(|o| o.name.clone()).collect()
    }

    fn text(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("fig.vvx")).unwrap()
    }

    /// A part rotated by its field: the file says its `pitch` (and only
    /// that part's lines changed), it reads back so, 0 takes the line out,
    /// UNDO gives the file back byte for byte; the part is drawn turned.
    #[test]
    fn a_part_is_rotated_at_rest() {
        let (mut ed, mut view, dir) = figure("rotate");
        let before = text(&dir);
        let p = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(p));
        act(&mut ed, &mut view, Action::PartRotation(1, 45.0));
        let now = text(&dir);
        assert!(now.contains("name = \"arm\"\nat = [3, -1, 3]\npitch = 45\n") || now.contains("pitch = 45"), "{now}");
        let changed: Vec<&str> = now.lines().filter(|l| !before.lines().any(|b| b == *l)).collect();
        assert_eq!(changed, ["pitch = 45"], "only the arm's line");
        assert_eq!(crate::editor::frames::ModelFile::load(&dir.join("fig.vvx")).unwrap().parts.iter().find(|o| o.name == "arm").unwrap().rotation, [0.0, 45.0, 0.0]);
        assert_ne!(ed.rest_view(), glam::Mat4::IDENTITY, "drawn turned");
        act(&mut ed, &mut view, Action::PartRotation(1, 0.0));
        assert_eq!(text(&dir), before, "0: the line taken out");
        ed.undo();
        ed.undo();
        assert_eq!(text(&dir), before);
    }

    /// A click on a part turned at rest meets the voxel one sees: the arm
    /// pitched 90° lies along y, and a ray where it shows meets it.
    #[test]
    fn a_click_meets_a_turned_part_where_it_shows() {
        let (mut ed, mut view, _dir) = figure("click");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        act(&mut ed, &mut view, Action::PartRotation(1, 90.0));
        act(&mut ed, &mut view, Action::WholeFigure);
        let (figure, (lo, _)) = (ed.figure.as_ref().unwrap(), ed.figure_box().unwrap());
        // Where the arm's top voxel shows now (its rest map), from above.
        let frame = &figure.model.frames[0];
        let top = figure.model.parts[arm].voxels.iter().map(|&k| frame[k as usize].0).max_by_key(|p| p.z).unwrap();
        let shows = figure.rest_map(arm, lo).transform_point3((top + lo).as_vec3() + 0.5);
        assert!((shows - ((top + lo).as_vec3() + 0.5)).length() > 1.5, "it moved");
        let hit = figure.part_at(lo, Vec3::new(shows.x, shows.y, 60.0), Vec3::NEG_Z);
        assert_eq!(hit, Some(arm));
    }

    /// From the figure whole, a stroke works on the part it meets and the
    /// figure stays chosen: two parts painted in turn, nothing dimmed, the
    /// choice never moving; a part's grid is where the tool acts.
    #[test]
    fn the_whole_figure_paints_the_part_met() {
        let (mut ed, _view, dir) = figure("paint");
        ed.choose(crate::editor::model::Tool::Paint);
        ed.colour = [255, 0, 0];
        for name in ["head", "body"] {
            let p = part(&ed, name);
            ed.begin_whole_stroke(p);
            assert!(ed.rigging.painting_whole && ed.part_mode().is_some());
            // A voxel of it, straight down onto its top.
            let size = ed.shape().unwrap().size;
            let hit = ed.aim(Vec3::new(0.5, 0.5, 100.0), Vec3::NEG_Z).unwrap_or(crate::editor::model::Hit { cell: IVec3::new(0, 0, size.z - 1), normal: IVec3::Z });
            ed.press(hit);
            ed.release();
            ed.end_whole_stroke();
            assert_eq!((ed.scope, ed.figure.as_ref().unwrap().selected), (Scope::Figure, None), "the figure still chosen");
        }
        assert_eq!(text(&dir).matches("#ff0000").count(), 1, "the red in the palette once");
        let m = crate::editor::frames::ModelFile::load(&dir.join("fig.vvx")).unwrap();
        let red = |n: &str| {
            let o = m.parts.iter().find(|o| o.name == n).unwrap();
            m.frames[o.frame].colours().iter().any(|c| m.palette.iter().any(|p| p.key == *c && p.mat.rgba[..3] == [255, 0, 0]))
        };
        assert!(red("head") && red("body"), "both painted");
    }

    /// ADD PART: one voxel of the color held hanging on the part chosen,
    /// named `part`, chosen; REMOVE: what hung on it hangs on its parent
    /// where it stands; the root cannot go while parts hang on it.
    #[test]
    fn parts_are_added_and_removed() {
        let (mut ed, mut view, dir) = figure("add-remove");
        let p = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(p));
        act(&mut ed, &mut view, Action::AddPart(None));
        assert_eq!(names(&ed), ["body", "head", "arm", "hand", "part"]);
        assert_eq!(ed.figure.as_ref().unwrap().selected, Some(part(&ed, "part")), "chosen, its name to type");
        assert_eq!(view.renaming_part, Some(part(&ed, "part")));
        assert!(text(&dir).contains("name = \"part\"") && text(&dir).contains("parent = \"arm\""));
        let m = ed.model.as_ref().unwrap();
        let new = m.parts.iter().find(|o| o.name == "part").unwrap().low;
        assert_eq!(new.z, 8, "on top of the arm (3 to 7 up), touching it: {new}");
        assert!((3..=4).contains(&new.x) && (-1..=0).contains(&new.y), "over the arm: {new}");
        // The figure whole: on top of its highest part, the head.
        act(&mut ed, &mut view, Action::WholeFigure);
        act(&mut ed, &mut view, Action::AddPart(None));
        let m = ed.model.as_ref().unwrap();
        let o = m.parts.iter().find(|o| o.name == "part_2").expect("a second new part");
        assert_eq!((o.low.z, o.parent.as_deref()), (11, Some("head")), "on the head's top (8 to 10 up)");
        ed.undo();
        // The arm goes: the hand and the new part hang on the body.
        let hand_at = ed.model.as_ref().unwrap().parts[3].low;
        let p = part(&ed, "arm");
        act(&mut ed, &mut view, Action::RemovePart(p));
        assert_eq!(names(&ed), ["body", "head", "hand", "part"]);
        let m = ed.model.as_ref().unwrap();
        assert_eq!((m.parts[2].parent.as_deref(), m.parts[2].low), (Some("body"), hand_at), "the hand where it stood, on the body");
        assert!(m.parts[2].joint.is_some(), "its joint kept");
        assert!(ed.cannot_remove(part(&ed, "body")).is_some(), "the root, with parts on it");
        ed.undo();
        assert_eq!(names(&ed), ["body", "head", "arm", "hand", "part"], "one step back");
    }

    /// MERGE: the hand into the arm: one part, the arm's name, parent and
    /// joint; where both had a voxel the arm's stays; refused while their
    /// rest rotations differ. DUPLICATE: a copy beside it, `arm_2`, chosen.
    #[test]
    fn parts_are_merged_and_duplicated() {
        let (mut ed, mut view, dir) = figure("merge");
        let (arm, hand) = (part(&ed, "arm"), part(&ed, "hand"));
        act(&mut ed, &mut view, Action::Part(hand));
        act(&mut ed, &mut view, Action::PartRotation(0, 30.0));
        assert!(ed.cannot_merge(hand, arm).is_some(), "turned off the arm's grid: refused");
        act(&mut ed, &mut view, Action::PartRotation(0, 0.0));
        assert!(ed.cannot_merge(hand, arm).is_none());
        act(&mut ed, &mut view, Action::MergeInto(hand, arm));
        assert_eq!(names(&ed), ["body", "head", "arm"]);
        let m = crate::editor::frames::ModelFile::load(&dir.join("fig.vvx")).unwrap();
        let o = m.parts.iter().find(|o| o.name == "arm").unwrap();
        assert_eq!((o.low, m.frames[o.frame].size), (IVec3::new(3, -1, 1), IVec3::new(2, 2, 7)), "the hand's voxels in the arm");
        let p = part(&ed, "arm");
        act(&mut ed, &mut view, Action::DuplicatePart(p));
        assert_eq!(names(&ed), ["body", "head", "arm", "arm_2"]);
        assert_eq!(ed.figure.as_ref().unwrap().selected, Some(part(&ed, "arm_2")));
        assert!(!ed.rigging.part_rotating, "in MOVE, to be dragged away");
    }

    /// RENAME: a word, unique; the file says it and what hung on it by
    /// hand follows; it reads back so. HIDE OTHERS and SHOW ALL.
    #[test]
    fn parts_are_renamed_and_the_others_hidden() {
        let (mut ed, _view, dir) = figure("rename");
        let arm = part(&ed, "arm");
        assert!(ed.rename_part(arm, "left arm").is_err() && ed.rename_part(arm, "head").is_err());
        ed.rename_part(arm, "left_arm").unwrap();
        let t = text(&dir);
        assert!(t.contains("name = \"left_arm\"") && t.contains("parent = \"left_arm\"") && !t.contains("\"arm\""), "{t}");
        let back = Editor::open_model(&dir, "fig.vvx").unwrap();
        assert!(back.model.as_ref().unwrap().parts.iter().any(|o| o.name == "left_arm"));
        ed.hide_other_parts(part(&ed, "head"));
        let f = ed.figure.as_ref().unwrap();
        assert_eq!(f.hidden.iter().filter(|&&h| !h).count(), 1);
        ed.show_all_parts();
        assert!(ed.figure.as_ref().unwrap().hidden.iter().all(|&h| !h));
    }

    /// A tool on a part turned at rest works in that part's own grid: the
    /// view's ray (through the part's rest, `View::rest`) meets the voxel
    /// one sees, and ADD sets the next one on its face, in its grid.
    #[test]
    fn a_tool_works_in_the_turned_parts_grid() {
        let (mut ed, mut view, _dir) = figure("tool");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        act(&mut ed, &mut view, Action::PartRotation(1, 90.0));
        ed.follow_figure();
        view.follow_origin(&ed);
        assert_ne!(view.rest, glam::Mat4::IDENTITY);
        // The arm's top voxel (in its own grid), where it shows, from above.
        let size = ed.shape().unwrap().size;
        let top = IVec3::new(0, 0, size.z - 1);
        let shows = view.rest.transform_point3(top.as_vec3() + 0.5);
        let back = view.rest.inverse();
        let (origin, dir) = (Vec3::new(shows.x, shows.y, shows.z + 40.0), Vec3::NEG_Z);
        let hit = ed.aim(back.transform_point3(origin), back.transform_vector3(dir)).expect("the arm met");
        assert!(ed.shape().unwrap().get(hit.cell) != 0, "a voxel of the arm");
        let steps = ed.steps();
        ed.choose(crate::editor::model::Tool::Add);
        ed.press(hit);
        ed.release();
        assert_eq!(ed.steps(), steps + 1, "a voxel set in the arm's grid");
    }

    /// The rig of every part by name (parent, joint from the pivot).
    fn rig(ed: &Editor) -> std::collections::BTreeMap<String, (Option<String>, Vec3)> {
        crate::editor::model::rig_of(ed.model.as_ref().unwrap()).expect("a figure")
    }

    /// No step re-hangs or re-hinges a part nobody acted on: a copy laid
    /// over the arm, then taken away, leaves every part hanging and hinged
    /// as before; so do ADD PART and MOVE; the JOINT field says the file's
    /// numbers (from the pivot).
    #[test]
    fn no_step_rehangs_a_part_it_did_not_act_on() {
        let (mut ed, mut view, dir) = figure("pins");
        let before = rig(&ed);
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::DuplicatePart(arm));
        let copy = part(&ed, "arm_2");
        let now = rig(&ed);
        for (name, r) in &before {
            assert_eq!(now.get(name), Some(r), "{name} after DUPLICATE");
        }
        act(&mut ed, &mut view, Action::RemovePart(copy));
        assert_eq!(rig(&ed), before, "DUPLICATE, then REMOVE the copy: as before");
        // ADD PART on the arm, MOVE the head well away: the others stay.
        let p = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(p));
        act(&mut ed, &mut view, Action::AddPart(None));
        let now = rig(&ed);
        for (name, r) in &before {
            assert_eq!(now.get(name), Some(r), "{name} after ADD PART");
        }
        let p = part(&ed, "head");
        act(&mut ed, &mut view, Action::Part(p));
        ed.move_part(IVec3::new(0, 0, 4));
        let now = rig(&ed);
        for (name, r) in before.iter().filter(|(n, _)| *n != "head") {
            assert_eq!(now.get(name), Some(r), "{name} after the head moved");
        }
        // The JOINT field: what the file says, from the pivot.
        let hand = part(&ed, "hand");
        act(&mut ed, &mut view, Action::Part(hand));
        act(&mut ed, &mut view, Action::JointTo(0, 3.5));
        let file = crate::editor::frames::ModelFile::load(&dir.join("fig.vvx")).unwrap();
        let stated = file.parts.iter().find(|o| o.name == "hand").unwrap().joint.expect("stated");
        assert_eq!(stated.x, 3.5, "typed 3.5 in X: the file's 3.5");
        let f = ed.figure.as_ref().unwrap();
        assert_eq!(f.joint(hand) + f.low.as_vec3(), stated, "the field shows the file's numbers");
    }

    /// The parts' tree `edit --shot` prints says where each part turns in
    /// the numbers the file and the JOINT row use (from the pivot), not
    /// the view's (from the figure's low corner).
    #[test]
    fn the_printed_tree_says_the_files_joints() {
        let (mut ed, mut view, dir) = figure("tree-joints");
        assert_ne!(ed.figure.as_ref().unwrap().low, IVec3::ZERO, "a figure whose low corner is not its pivot");
        // The hand's joint typed in, as the file then states it.
        let hand = part(&ed, "hand");
        act(&mut ed, &mut view, Action::Part(hand));
        act(&mut ed, &mut view, Action::JointTo(0, 3.5));
        let file = crate::editor::frames::ModelFile::load(&dir.join("fig.vvx")).unwrap();
        let stated = file.parts.iter().find(|o| o.name == "hand").unwrap().joint.expect("stated");
        let lines = ed.figure.as_ref().unwrap().tree_lines();
        let line = lines.iter().find(|l| l.trim_start().starts_with("hand ")).unwrap_or_else(|| panic!("hand in {lines:?}"));
        assert!(line.contains(&format!("turning at {}, {}, {}", stated.x, stated.y, stated.z)), "{line}: the file's {stated}");
    }

    /// REDO after each of the parts' steps gives the same file again.
    #[test]
    fn every_parts_step_is_done_again_by_redo() {
        let (mut ed, mut view, dir) = figure("redo");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        type StepFn = Box<dyn Fn(&mut Editor, &mut View)>;
        let steps: Vec<StepFn> = vec![
            Box::new(|ed, view| act(ed, view, Action::AddPart(None))),
            Box::new(|ed, view| {
                let p = part(ed, "arm");
                act(ed, view, Action::DuplicatePart(p));
            }),
            Box::new(|ed, view| {
                let p = part(ed, "arm_2");
                act(ed, view, Action::RemovePart(p));
            }),
            Box::new(|ed, view| {
                let (h, a) = (part(ed, "hand"), part(ed, "arm"));
                act(ed, view, Action::MergeInto(h, a));
            }),
            Box::new(|ed, view| {
                // (The arm is chosen: the merge left it so.)
                assert_eq!(ed.figure.as_ref().unwrap().selected, Some(part(ed, "arm")));
                act(ed, view, Action::PartRotation(0, 30.0));
            }),
            Box::new(|ed, _| {
                ed.move_part(IVec3::new(1, 0, 0));
            }),
            Box::new(|ed, _| {
                let p = part(ed, "head");
                ed.rename_part(p, "skull").unwrap();
            }),
        ];
        for (k, step) in steps.iter().enumerate() {
            let was = text(&dir);
            step(&mut ed, &mut view);
            let done = text(&dir);
            assert_ne!(was, done, "step {k} changed nothing");
            ed.undo();
            assert_eq!(text(&dir), was, "step {k}: UNDO");
            ed.redo();
            assert_eq!(text(&dir), done, "step {k}: REDO");
        }
    }

    /// Escape during a ring's drag (ROTATE): the part as it was, no step.
    #[test]
    fn escape_during_a_ring_drag_leaves_the_part() {
        use crate::editor::handles::{Drag, Grip, project};
        use crate::editor::view::{FOV, Orbit, mouse_ray};
        let (mut ed, mut view, dir) = figure("ring-escape");
        let p = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(p));
        act(&mut ed, &mut view, Action::PartRotating(true));
        let screen = glam::Vec2::new(1600.0, 900.0);
        let was = (text(&dir), ed.steps(), ed.part_rotation());
        let (lo, size) = ed.figure_box().unwrap();
        let vp = Orbit { yaw: 0.0, pitch: 89.0, zoom: 80.0, target: (lo + size / 2).as_vec3(), fov: FOV }.view_proj(screen.x / screen.y);
        let h = ed.part_handles(vp).expect("the rings");
        let ray = |p: glam::Vec2| mouse_ray(vp, p.x / screen.x, p.y / screen.y);
        let middle = project(vp, screen, h.centre).unwrap().0;
        let r = project(vp, screen, h.centre + Vec3::X * h.length).map_or(60.0, |(p, _)| p.distance(middle));
        let press = middle + glam::Vec2::new(r, 0.0);
        let drag = Drag::begin(&h, Grip::Ring(2), vp, screen, press, ray(press)).expect("the round ring met");
        assert!(ed.grab_part(drag, h));
        for k in 1..=6 {
            let a = (k as f32 * 10.0).to_radians();
            let p = middle + glam::Vec2::new(a.cos(), -a.sin()) * r;
            ed.drag_handle(vp, screen, p, ray(p));
        }
        assert_ne!(ed.part_rotation(), was.2, "turned while dragged");
        assert!(ed.cancel_joint());
        assert_eq!((text(&dir), ed.steps(), ed.part_rotation()), was, "Escape: as it was, no step");
    }

    /// Where each voxel of the figure shows at rest (its middle, rounded),
    /// with its color.
    fn seen_cells(ed: &Editor) -> std::collections::BTreeSet<([i32; 3], [u8; 3])> {
        let (f, m) = (ed.figure.as_ref().unwrap(), ed.model.as_ref().unwrap());
        let mut out = std::collections::BTreeSet::new();
        for (p, part) in f.parts().iter().enumerate() {
            let o = m.parts.iter().find(|o| o.name == part.name).unwrap();
            let b = &m.frames[o.frame];
            let map = f.rest_map(p, IVec3::ZERO);
            for k in 0..b.voxels.len() {
                if let Some(colour) = b.colour_at(b.at(k)) {
                    let at = map.transform_point3((b.at(k) + o.low - f.low).as_vec3() + 0.5).floor().as_ivec3();
                    out.insert((at.to_array(), colour));
                }
            }
        }
        out
    }

    /// A part turned at rest by a quarter turn (of its own, or its
    /// parent's otherwise) merges as it stands: every voxel shows where it
    /// showed; one off the grid is refused, quiet in the menu with why.
    #[test]
    fn a_quarter_turned_part_merges_as_it_stands() {
        let (mut ed, mut view, _dir) = figure("merge-quarter");
        let (arm, hand) = (part(&ed, "arm"), part(&ed, "hand"));
        act(&mut ed, &mut view, Action::Part(hand));
        act(&mut ed, &mut view, Action::PartRotation(1, 90.0));
        ed.follow_figure();
        assert!(ed.cannot_merge(hand, arm).is_none(), "a quarter turn: it merges");
        let before = seen_cells(&ed);
        act(&mut ed, &mut view, Action::MergeInto(hand, arm));
        assert_eq!(names(&ed), ["body", "head", "arm"]);
        ed.follow_figure();
        let after = seen_cells(&ed);
        // (Where the hand lay in the arm, the arm's voxel stays.)
        let lost: Vec<_> = before.difference(&after).filter(|(at, _)| !after.iter().any(|(b, _)| b == at)).collect();
        assert!(lost.is_empty(), "voxels went missing: {lost:?}");
        assert!(after.is_subset(&before), "voxels came where none showed");
        // Turned by 30: refused, said in the menu.
        let (mut ed, mut view, _dir) = figure("merge-off");
        let (arm, hand) = (part(&ed, "arm"), part(&ed, "hand"));
        act(&mut ed, &mut view, Action::Part(hand));
        act(&mut ed, &mut view, Action::PartRotation(1, 30.0));
        ed.follow_figure();
        let why = ed.cannot_merge(hand, arm).expect("refused");
        let entries = crate::editor::shaping::menu_entries(&ed, crate::editor::view::MenuOn::Part(hand));
        let merge = entries.iter().find(|e| e.0 == "MERGE INTO ARM").expect("MERGE INTO ARM still offered");
        assert_eq!(merge.3.as_deref(), Some(why), "quiet, why on hover");
        assert!(entries.iter().any(|e| e.0 == "MERGE WITH…"), "MERGE WITH… still offered");
    }

    /// POSE: the rings and fields turn the chosen part on top of its rest,
    /// drawn so, nothing written and no step; the pose as Lua; off, the
    /// rest is drawn again and the pose kept; RESET drops it.
    #[test]
    fn a_pose_is_drawn_not_written() {
        let (mut ed, mut view, dir) = figure("pose");
        let file = text(&dir);
        let (steps, arm) = (ed.steps(), part(&ed, "arm"));
        act(&mut ed, &mut view, Action::Pose);
        assert!(ed.rigging.posing);
        act(&mut ed, &mut view, Action::Part(arm));
        let at_rest = ed.figure.as_ref().unwrap().rest_map(arm, IVec3::ZERO);
        act(&mut ed, &mut view, Action::PoseValue(2, 20.0));
        act(&mut ed, &mut view, Action::PoseValue(0, -15.5));
        let posed = ed.figure.as_ref().unwrap().rest_map(arm, IVec3::ZERO);
        assert_ne!(posed, at_rest, "drawn posed");
        assert_eq!((text(&dir), ed.steps()), (file.clone(), steps), "nothing written, no step");
        assert_eq!(ed.pose_lua(), "{ arm = { rotation = -15.5, roll = 20 } }");
        // The hand hangs on the arm: it goes along.
        let hand = part(&ed, "hand");
        assert_ne!(ed.figure.as_ref().unwrap().rest_map(hand, IVec3::ZERO), glam::Mat4::IDENTITY);
        // The rings are the pose's.
        let vp = crate::editor::view::Orbit { yaw: 20.0, pitch: 30.0, zoom: 60.0, target: Vec3::ZERO, fov: crate::editor::view::FOV }.view_proj(16.0 / 9.0);
        assert!(ed.part_handles(vp).is_some_and(|h| h.rings.iter().all(Option::is_some) && h.arrows.iter().all(Option::is_none)));
        act(&mut ed, &mut view, Action::Pose);
        assert_eq!(ed.figure.as_ref().unwrap().rest_map(arm, IVec3::ZERO), at_rest, "POSE off: the rest drawn");
        assert!(!ed.rigging.pose.is_empty(), "the pose kept");
        act(&mut ed, &mut view, Action::Pose);
        assert_eq!(ed.figure.as_ref().unwrap().rest_map(arm, IVec3::ZERO), posed, "on again: posed again");
        act(&mut ed, &mut view, Action::PoseReset);
        assert_eq!((ed.pose_lua().as_str(), ed.figure.as_ref().unwrap().rest_map(arm, IVec3::ZERO)), ("{}", at_rest));
        assert_eq!(text(&dir), file);
    }

    /// MIRROR: `arm_left`'s partner `arm_right` made across the figure's
    /// middle (voxels, place, joint, rest rotation the other way), made
    /// anew after a change; refused, with why, for a name of no side;
    /// with SYMMETRY a voxel set in one lands mirrored in the other.
    #[test]
    fn a_pair_is_mirrored() {
        let (mut ed, mut view, dir) = figure("mirror");
        let arm = part(&ed, "arm");
        assert!(ed.cannot_mirror(arm).is_some(), "no side in the name");
        ed.rename_part(arm, "arm_left").unwrap();
        let arm = part(&ed, "arm_left");
        act(&mut ed, &mut view, Action::Part(arm));
        act(&mut ed, &mut view, Action::PartRotation(0, 30.0));
        act(&mut ed, &mut view, Action::JointTo(0, 3.5));
        act(&mut ed, &mut view, Action::MirrorPart(arm));
        let m = ed.model.as_ref().unwrap();
        let (l, r) = (m.parts.iter().position(|o| o.name == "arm_left").unwrap(), m.parts.iter().position(|o| o.name == "arm_right").expect("arm_right made"));
        let mid = 2 * ed.figure.as_ref().unwrap().low.x + ed.figure.as_ref().unwrap().model.size.x - 1;
        let mirrored: std::collections::BTreeMap<_, _> = super::cells_of(m, l).into_iter().map(|([x, y, z], c)| ([mid - x, y, z], c)).collect();
        assert_eq!(super::cells_of(m, r), mirrored, "its voxels mirrored");
        assert_eq!(m.parts[r].rotation, [-30.0, 0.0, 0.0]);
        assert_eq!(m.parts[r].joint.unwrap().x, mid as f32 + 1.0 - 3.5);
        assert!(text(&dir).contains("name = \"arm_right\""));
        // A change, MIRROR again: made anew.
        act(&mut ed, &mut view, Action::PartRotation(2, 10.0));
        let arm = part(&ed, "arm_left");
        act(&mut ed, &mut view, Action::MirrorPart(arm));
        let m = ed.model.as_ref().unwrap();
        assert_eq!(m.parts.iter().filter(|o| o.name == "arm_right").count(), 1);
        assert_eq!(m.parts.iter().find(|o| o.name == "arm_right").unwrap().rotation, [-30.0, 0.0, -10.0]);
        // SYMMETRY: a voxel set on the left arm's top lands on the right's.
        act(&mut ed, &mut view, Action::PartRotation(0, 0.0));
        act(&mut ed, &mut view, Action::PartRotation(2, 0.0));
        let arm = part(&ed, "arm_left");
        act(&mut ed, &mut view, Action::MirrorPart(arm));
        ed.symmetry = true;
        assert!(ed.symmetry_on_partner());
        ed.follow_figure();
        view.follow_origin(&ed);
        let size = ed.shape().unwrap().size;
        let top = IVec3::new(0, 0, size.z - 1);
        let hit = crate::editor::model::Hit { cell: top, normal: IVec3::Z };
        ed.choose(crate::editor::model::Tool::Add);
        let steps = ed.steps();
        ed.press(hit);
        ed.release();
        assert_eq!(ed.steps(), steps + 1, "one step for both");
        let m = ed.model.as_ref().unwrap();
        let (l, r) = (m.parts.iter().position(|o| o.name == "arm_left").unwrap(), m.parts.iter().position(|o| o.name == "arm_right").unwrap());
        let mirrored: std::collections::BTreeMap<_, _> = super::cells_of(m, l).into_iter().map(|([x, y, z], c)| ([mid - x, y, z], c)).collect();
        assert_eq!(super::cells_of(m, r), mirrored, "the stroke landed on both");
        assert_eq!(super::cells_of(m, l).len(), 2 * 2 * 5 + 1, "the left arm a voxel more, nothing on its far side");
        ed.undo();
        let m = ed.model.as_ref().unwrap();
        assert_eq!(super::cells_of(m, m.parts.iter().position(|o| o.name == "arm_right").unwrap()).len(), 20, "UNDO takes both back");
    }

    /// GAME VIEW on a figure: the figure whole from the game's camera,
    /// drawn from a model bank as the game draws it (its outline), posed
    /// while POSE is lit; nothing to click; off, the view as it was.
    #[test]
    fn a_figure_in_game_view_is_drawn_as_the_game_draws_it() {
        let (mut ed, mut view, dir) = figure("game-view");
        let figures = crate::editor::view::Figures::load(&dir, &crate::config::GameConfig::default());
        let light = crate::config::LightConfig::default();
        let before = view.block;
        view.outline = [0.0, 0.0, 0.0, 1.0];
        act(&mut ed, &mut view, Action::GameView);
        assert!(view.before_game.is_some() && ed.scope == Scope::Figure);
        let shown = crate::editor::view::Shown::new(&ed).unwrap();
        assert_eq!((view.block.yaw, view.block.pitch, view.block.fov), (view.game.yaw, view.game.pitch, view.game.fov), "the game's angle");
        // Framed as the editor frames it, not at the game's distance.
        // (Its voxels' box, as a figure opens.)
        let at = ed.figure_box().unwrap().0;
        let cells: Vec<glam::IVec3> = ed.figure.as_ref().unwrap().model.frames[0].iter().map(|v| v.0).collect();
        let lo = cells.iter().fold(cells[0], |a, &c| a.min(c));
        let size = cells.iter().fold(cells[0], |a, &c| a.max(c)) - lo + 1;
        assert_eq!(view.block.zoom, (size.max_element() as f32 * 2.8).max(16.0), "the figure fills the view");
        assert!(view.block.zoom < view.game.view_width, "nearer than the game's view");
        assert_eq!(view.block.target, (at + lo).as_vec3() + size.as_vec3() / 2.0, "its middle");
        let framed = view.block;
        let f = crate::editor::view::frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert_eq!(f.models.len(), 4, "a draw from the bank for each part");
        assert_eq!(f.outline[3], 1.0, "the game's outline");
        // Posed: the arm's draw turned.
        act(&mut ed, &mut view, Action::Pose);
        ed.rigging.pose.insert("arm".into(), super::Posed { turn: [0.0, 45.0, 0.0], offset: [0.0; 3] });
        ed.follow_figure();
        let posed = crate::editor::view::frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert_ne!(posed.models, f.models, "posed");
        // Zoomed and turned by the corner buttons: still GAME VIEW, the
        // user's angle and size.
        act(&mut ed, &mut view, Action::ZoomView(1));
        act(&mut ed, &mut view, Action::TurnView(1));
        assert!(view.before_game.is_some(), "GAME VIEW stays on");
        assert!(view.block.zoom < framed.zoom && view.block.yaw != framed.yaw);
        act(&mut ed, &mut view, Action::GameView);
        assert!(view.before_game.is_none());
        assert_eq!(view.block, before, "off: the editor's look as it was");
        // On again: the game's angle afresh.
        act(&mut ed, &mut view, Action::GameView);
        assert_eq!(view.block, framed, "on again: the game's angle, framed");
        act(&mut ed, &mut view, Action::GameView);
        // A level's GAME VIEW is the game's camera: the corner buttons end it.
        ed.scope = Scope::Level;
        view.before_game = Some(view.level);
        act(&mut ed, &mut view, Action::ZoomView(1));
        assert!(view.before_game.is_none(), "a level's ends");
    }

    /// GAME VIEW draws a figure the way the game has it look: a thing's
    /// model turned to the camera, as `frame:model` draws it on a thing of
    /// a kind facing the camera (its +y side seen, its +x on the left); the
    /// title's logo unturned and straight from the front, as the title
    /// lays it (BunnyFarm's logo read mirrored before).
    #[test]
    fn game_view_faces_a_figure_as_the_game_does() {
        let (mut ed, mut view, dir) = figure("game-view-face");
        let figures = crate::editor::view::Figures::load(&dir, &crate::config::GameConfig::default());
        let light = crate::config::LightConfig::default();
        let turns = |ed: &Editor, view: &View| {
            let shown = crate::editor::view::Shown::new(ed).unwrap();
            let f = crate::editor::view::frame(ed, view, &shown, &figures, &light, 16.0 / 9.0);
            assert!(!f.models.is_empty());
            f.models.iter().map(|m| m.turn).collect::<std::collections::BTreeSet<u32>>()
        };
        assert_eq!(view.face, crate::editor::view::Face::Front);
        act(&mut ed, &mut view, Action::GameView);
        assert_eq!(turns(&ed, &view), [2].into(), "a thing's figure turned to the camera");
        act(&mut ed, &mut view, Action::GameView);
        view.face = crate::editor::view::Face::Logo;
        act(&mut ed, &mut view, Action::GameView);
        assert_eq!(turns(&ed, &view), [0].into(), "the logo as the title lays it");
        assert_eq!((view.block.yaw, view.block.pitch), (0.0, 0.0), "from the front, as the title shows it");
    }

    /// A part's arrow dragged, its let-go never clean: the focus lost or
    /// another button (cancelled: the file as at the start, no step); the
    /// button found up (one step, one UNDO); then a motion with the stale
    /// drag's origin moves nothing; a second press, a tool's, without a
    /// let-go (the drag one step first); the disk looked at mid-drag
    /// (nothing read anew); a click on the arrow (nothing); a ray along
    /// the arrow (never further than `FAR`, never NaN).
    #[test]
    fn a_part_drag_never_let_go_cleanly_is_ended_or_undone() {
        use crate::editor::handles::{Drag, Grip, project};
        use crate::editor::view::{FOV, Orbit, mouse_ray};
        let (mut ed, mut view, dir) = figure("lost-part-drag");
        let start = text(&dir);
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        let screen = glam::Vec2::new(1600.0, 900.0);
        let (lo, size) = ed.figure_box().unwrap();
        let vp = Orbit { yaw: 30.0, pitch: 30.0, zoom: 60.0, target: (lo + size / 2).as_vec3(), fov: FOV }.view_proj(screen.x / screen.y);
        let ray = |p: glam::Vec2| mouse_ray(vp, p.x / screen.x, p.y / screen.y);
        let at = |ed: &Editor| ed.model.as_ref().unwrap().parts.iter().find(|o| o.name == "arm").unwrap().low;
        let was = at(&ed);
        let grab = |ed: &mut Editor, by: f32| {
            let h = ed.part_handles(vp).expect("its arrows");
            let head = project(vp, screen, h.centre + Vec3::X * h.length * 0.95).unwrap().0;
            let drag = Drag::begin(&h, Grip::Arrow(0), vp, screen, head, ray(head)).expect("the X arrow");
            assert!(ed.grab_part(drag, h.clone()));
            let to = project(vp, screen, h.centre + Vec3::X * (h.length * 0.95 + by)).unwrap().0;
            ed.drag_handle(vp, screen, to, ray(to));
        };
        // Focus lost: cancelled.
        grab(&mut ed, 5.0);
        assert_ne!(at(&ed), was, "moved while dragged");
        ed.end_drag(true);
        assert_eq!((text(&dir), ed.steps(), at(&ed)), (start.clone(), 0, was), "canceled: as at the start");
        // The button found up: one step; a motion after moves nothing.
        grab(&mut ed, 5.0);
        ed.end_drag(false);
        assert_eq!(ed.steps(), 1);
        let moved = at(&ed);
        ed.drag_handle(vp, screen, glam::Vec2::new(10.0, 10.0), ray(glam::Vec2::new(10.0, 10.0)));
        assert_eq!(at(&ed), moved, "no drag: nothing moves");
        ed.undo();
        assert_eq!(text(&dir), start, "one UNDO: as at the start");
        // A tool's press without a let-go between.
        grab(&mut ed, 4.0);
        ed.choose(crate::editor::model::Tool::Pick);
        ed.press(crate::editor::model::Hit { cell: IVec3::ZERO, normal: IVec3::Z });
        assert!(ed.handle_drag().is_none(), "the press ended the drag");
        ed.drag_handle(vp, screen, glam::Vec2::new(10.0, 10.0), ray(glam::Vec2::new(10.0, 10.0)));
        ed.release();
        assert_eq!(ed.steps(), 1, "the drag its step, nothing else");
        ed.undo();
        assert_eq!(text(&dir), start);
        // Two presses on the arrow without a let-go.
        grab(&mut ed, 4.0);
        grab(&mut ed, 0.0);
        ed.release();
        ed.undo();
        assert_eq!(text(&dir), start, "one UNDO after the second press");
        // The disk looked at mid-drag: the editor's own writes.
        grab(&mut ed, 3.0);
        assert!(!ed.look_at_disk(), "the editor's own file is not changed outside");
        ed.release();
        assert_eq!(ed.steps(), 1, "the step kept");
        ed.undo();
        assert_eq!(text(&dir), start);
        // A click on the arrow, the pointer 2 pixels on: nothing.
        let h = ed.part_handles(vp).unwrap();
        let head = project(vp, screen, h.centre + Vec3::X * h.length * 0.95).unwrap().0;
        assert!(ed.grab_part(Drag::begin(&h, Grip::Arrow(0), vp, screen, head, ray(head)).unwrap(), h.clone()));
        ed.drag_handle(vp, screen, head + glam::Vec2::new(2.0, 1.0), ray(head + glam::Vec2::new(2.0, 1.0)));
        ed.release();
        assert_eq!((ed.steps(), text(&dir)), (0, start.clone()), "a click moves nothing");
        // Thrown: never further than FAR, never NaN.
        grab(&mut ed, 100_000.0);
        ed.release();
        assert!(at(&ed).abs().max_element() <= crate::editor::model::FAR, "{}", at(&ed));
        ed.undo();
        assert_eq!(text(&dir), start);
    }

    /// POINTER is the tool a model opens with and changes nothing: a press
    /// and a drag over voxels, in a part or from the figure whole, make no
    /// step and leave the file as it was.
    #[test]
    fn the_pointer_changes_nothing() {
        let (mut ed, mut view, dir) = figure("pointer");
        assert_eq!(ed.tool, crate::editor::model::Tool::Pointer, "the tool a model opens with");
        let start = text(&dir);
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        for c in [IVec3::ZERO, IVec3::new(1, 1, 4)] {
            ed.press(crate::editor::model::Hit { cell: c, normal: IVec3::Z });
            ed.drag(crate::editor::model::Hit { cell: c + IVec3::X, normal: IVec3::Z });
            ed.release();
        }
        assert!(!ed.forms(), "no box drawn with it");
        assert_eq!((ed.steps(), text(&dir)), (0, start), "nothing changed");
    }

    /// A figure's box is never larger than its parts need: a part moved
    /// 500 voxels away grows it, moved back it is as it was, the file as
    /// at the start. A file opened with a box far too large is not changed
    /// by opening; TRIM (one step) makes it tight, UNDO gives it back.
    #[test]
    fn a_figures_box_shrinks_back_and_trim_makes_it_tight() {
        let (mut ed, mut view, dir) = figure("trim");
        let start = text(&dir);
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        ed.move_part(IVec3::new(0, 500, 0));
        assert!(text(&dir).contains("size = [9, 100"), "grown: {}", text(&dir).lines().nth(1).unwrap());
        ed.move_part(IVec3::new(0, -500, 0));
        assert_eq!(text(&dir), start, "moved back: the box as it was");
        // A file left with a huge box.
        let loose = start.replace("size = [9, 4, 11]", "size = [13, 2001, 41]");
        std::fs::write(dir.join("fig.vvx"), &loose).unwrap();
        let mut ed = Editor::open_model(&dir, "fig.vvx").unwrap();
        ed.follow_figure();
        assert_eq!(text(&dir), loose, "opening changes nothing");
        assert!(ed.can_trim(), "TRIM has something to cut");
        let steps = ed.steps();
        ed.trim();
        assert_eq!((ed.steps(), text(&dir)), (steps + 1, start.clone()), "TRIM: tight, one step");
        assert!(!ed.can_trim(), "ALREADY TIGHT");
        ed.undo();
        assert_eq!(text(&dir), loose);
    }

    /// A figure like the owner's: a head turned 45 round and tipped, a nose
    /// turned -45 on it.
    fn nosed(name: &str) -> (Editor, View, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("parts-{name}"));
        let part = |name: &str, at: [i32; 3], size: [i32; 3], sign: char, extra: &str| {
            let rows: Vec<String> = (0..size[2]).map(|_| (0..size[1]).map(|_| sign.to_string().repeat(size[0] as usize)).collect::<Vec<_>>().join("\n")).collect();
            format!("\n[[part]]\nname = \"{name}\"\nat = [{}, {}, {}]\n{extra}layers = \"\"\"\n{}\n\"\"\"\n", at[0], at[1], at[2], rows.join("\n\n"))
        };
        let text = format!(
            "format = \"{}\"\nsize = [8, 8, 12]\n\n[palette]\na = \"#5a7aa8\"\nb = \"#d0c8b8\"\nc = \"#c85a4a\"\n{}{}{}",
            crate::vvx::FORMAT,
            part("body", [-3, -2, 0], [6, 4, 8], 'a', ""),
            part("head", [-2, -2, 8], [4, 4, 4], 'b', "parent = \"body\"\njoint = [0, 0, 8]\nrotation = 45\npitch = 20\n"),
            part("nose", [0, -4, 9], [1, 2, 1], 'c', "parent = \"head\"\nrotation = -45\n"),
        );
        std::fs::write(dir.join("fig.vvx"), text).unwrap();
        let mut ed = Editor::open_model(&dir, "fig.vvx").unwrap();
        ed.follow_figure();
        let view = View::new(&ed, &crate::config::GameConfig::default());
        (ed, view, dir)
    }

    /// Where the cubes of a color show on the screen (shares), as drawn.
    fn seen_at(f: &crate::render::FrameData, colour: [u8; 3]) -> Vec<glam::Vec2> {
        f.instances
            .iter()
            .filter(|c| c.color[..3] == colour)
            .map(|c| {
                let p = f.view_proj * Vec3::from_slice(&c.pos_size[..3]).extend(1.0);
                glam::Vec2::new((p.x / p.w + 1.0) / 2.0, (1.0 - p.y / p.w) / 2.0)
            })
            .collect()
    }

    /// Choosing a part turned at rest moves neither the camera nor the
    /// other parts on the screen (the head stays where it shows); the
    /// chosen nose is drawn where it stands; a tool's ray at a voxel of the
    /// nose on the screen meets that voxel in the nose's own grid.
    #[test]
    fn choosing_a_turned_part_keeps_the_view() {
        let (mut ed, mut view, dir) = nosed("nosed");
        let figures = crate::editor::view::Figures::load(&dir, &crate::config::GameConfig::default());
        let light = crate::config::LightConfig::default();
        let frame = |ed: &Editor, view: &View| {
            let shown = crate::editor::view::Shown::new(ed).unwrap();
            crate::editor::view::frame(ed, view, &shown, &figures, &light, 16.0 / 9.0)
        };
        let whole = frame(&ed, &view);
        let head = [0xd0, 0xc8, 0xb8];
        let nose = [0xc8, 0x5a, 0x4a];
        let (head_before, nose_before) = (seen_at(&whole, head), seen_at(&whole, nose));
        assert!(!head_before.is_empty() && !nose_before.is_empty());
        let p = part(&ed, "nose");
        act(&mut ed, &mut view, Action::Part(p));
        view.follow_origin(&ed);
        assert_ne!(view.rest, glam::Mat4::IDENTITY, "a turned part");
        let chosen = frame(&ed, &view);
        let close = |a: &[glam::Vec2], b: &[glam::Vec2]| a.len() == b.len() && a.iter().all(|p| b.iter().any(|q| p.distance(*q) < 1e-3));
        assert!(close(&seen_at(&chosen, head), &head_before), "the head shows where it showed");
        assert!(close(&seen_at(&chosen, nose), &nose_before), "the nose drawn where it stands");
        // A ray through a nose voxel on the screen meets the nose.
        let at = seen_at(&chosen, nose)[0];
        let (origin, dir) = view.ray(&ed, 16.0 / 9.0, at.x, at.y);
        let hit = ed.aim(origin, dir).expect("the nose met");
        assert!(ed.shape().unwrap().get(hit.cell) != 0, "a voxel of the nose: {hit:?}");
    }

    /// Moved by tenths (Shift on an arrow, POSITION typed 4.3): the whole
    /// voxels go into `at`, the rest into `offset`, kept within half a
    /// voxel; the file says it one line after `at`; UNDO gives it back.
    #[test]
    fn a_part_moves_by_tenths_into_its_offset() {
        let (mut ed, mut view, dir) = figure("tenths");
        let start = text(&dir);
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        act(&mut ed, &mut view, Action::PartAt(1, -0.7));
        let o = ed.model.as_ref().unwrap().parts.iter().find(|o| o.name == "arm").unwrap().clone();
        assert_eq!((o.low.y, (o.offset.y * 10.0).round()), (-1, 3.0), "-0.7: at -1, offset 0.3");
        assert!(text(&dir).contains("name = \"arm\"\nat = [3, -1, 3]\noffset = [0, 0.3, 0]\n"), "{}", text(&dir));
        assert_eq!(ed.part_place().map(|p| (p.y * 10.0).round()), Some(-7.0));
        // More tenths: it folds over into `at`, the offset within half.
        ed.move_part_by(Vec3::new(0.0, 0.4, 0.0));
        let o = ed.model.as_ref().unwrap().parts.iter().find(|o| o.name == "arm").unwrap().clone();
        assert_eq!((o.low.y, (o.offset.y * 10.0).round()), (0, -3.0), "-0.3: at 0, offset -0.3");
        ed.undo();
        ed.undo();
        assert_eq!(text(&dir), start);
    }

    /// LOCAL (the default) moves a part along its own axes as it stands, a
    /// whole voxel along a turned axis split into `at` and `offset`; GLOBAL
    /// along the figure's. Shift held never makes PICK while a handle is
    /// dragged.
    #[test]
    fn local_moves_along_the_parts_own_axes() {
        use crate::editor::handles::{Drag, Grip, project};
        use crate::editor::view::{FOV, Orbit, mouse_ray};
        let (mut ed, mut view, _dir) = nosed("local");
        let nose = part(&ed, "nose");
        act(&mut ed, &mut view, Action::Part(nose));
        view.follow_origin(&ed);
        assert!(ed.part_turned() && !ed.rigging.move_global, "LOCAL at a turned part");
        let screen = glam::Vec2::new(1600.0, 900.0);
        let at = |ed: &Editor| ed.local_to_pivot().transform_point3(Vec3::splat(0.5));
        let drag_x = |ed: &mut Editor, view: &View, by: f32| {
            let vp = Orbit { yaw: 20.0, pitch: 25.0, zoom: 40.0, target: view.block.target, fov: FOV }.view_proj(screen.x / screen.y) * ed.rest_view();
            let ray = |p: glam::Vec2| mouse_ray(vp, p.x / screen.x, p.y / screen.y);
            let h = ed.part_handles(vp).unwrap();
            let x = h.arrows[0].unwrap();
            let head = project(vp, screen, h.centre + x * h.length * 0.95).unwrap().0;
            assert!(ed.grab_part(Drag::begin(&h, Grip::Arrow(0), vp, screen, head, ray(head)).unwrap(), h.clone()));
            ed.shift_pick(true);
            assert_ne!(ed.tool, crate::editor::model::Tool::Pick, "Shift on a handle: no PICK");
            let to = project(vp, screen, h.centre + x * (h.length * 0.95 + by)).unwrap().0;
            ed.drag_handle(vp, screen, to, ray(to));
            ed.release();
            ed.shift_pick(false);
        };
        let (before, own) = (at(&ed), ed.rest_view().to_scale_rotation_translation().1);
        drag_x(&mut ed, &view, 3.0);
        let went = at(&ed) - before;
        let along = own * Vec3::X;
        assert!((went - along * 3.0).length() < 0.15, "three voxels along its own X {along}: went {went}");
        ed.undo();
        // GLOBAL: along the figure's X.
        act(&mut ed, &mut view, Action::MoveGlobal(true));
        let before = at(&ed);
        drag_x(&mut ed, &view, 2.0);
        let went = at(&ed) - before;
        assert!((went - Vec3::X * 2.0).length() < 0.15, "two voxels along the figure's X: went {went}");
    }

    /// A model of one frame (or `frames` alike) in a folder of its own,
    /// `file`: a car, a body with a cabin of glass on it and its roof; the
    /// box wider than its voxels.
    fn one_frame(name: &str, file: &str, frames: usize) -> (Editor, View, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("parts-{name}"));
        let layers = "......\n.aaaa.\n.aaaa.\n.aaaa.\n......\n\n......\n.aaaa.\n.aaaa.\n.aaaa.\n......\n\n......\n.g..g.\n......\n.g..g.\n......\n\n......\n.aaaa.\n.aaaa.\n.aaaa.\n......";
        let frame = format!("\n[[frame]]\nlayers = \"\"\"\n{layers}\n\"\"\"\n");
        let text = format!("format = \"{}\"\nsize = [6, 5, 4]\n\n[palette]\na = \"#c83c30\"\ng = {{ color = \"#9fd8ff\", glass = 0.6 }}\n{}", crate::vvx::FORMAT, frame.repeat(frames));
        std::fs::write(dir.join(file), text).unwrap();
        let ed = Editor::open_model(&dir, file).unwrap();
        let view = View::new(&ed, &crate::config::GameConfig::default());
        (ed, view, dir)
    }

    /// Every voxel of a model's text from its pivot, with its color, of
    /// every frame or part: what the game draws.
    fn drawn(text: &str) -> BTreeSet<([i32; 3], [u8; 4])> {
        let (shape, _) = crate::vvx::read_shape(text).unwrap();
        shape.pieces.iter().flat_map(|p| p.voxels.iter().map(|(v, m)| (v.to_array(), m.rgba))).collect()
    }

    /// NEW PART in a model of one frame (the owner's car, shaped before it
    /// had parts): one step makes it a figure, its frame the root part
    /// named as the file, the selection a part hanging on it, every voxel
    /// where it stood; the file reads back; UNDO gives the one frame back
    /// byte for byte, REDO the figure.
    #[test]
    fn a_model_of_one_frame_becomes_a_figure_by_new_part() {
        let (mut ed, mut view, dir) = one_frame("one-frame", "car.vvx", 1);
        let file = dir.join("car.vvx");
        let before = std::fs::read_to_string(&file).unwrap();
        assert!(ed.figure.is_none() && ed.cannot_part().is_none());
        // The cabin's glass and the roof over it.
        ed.select_from_pivot(IVec3::new(-3, -3, 2), IVec3::new(3, 3, 3));
        assert_eq!(ed.selection().map(|s| s.count()), Some(16));
        act(&mut ed, &mut view, Action::SelNewPart);
        let after = std::fs::read_to_string(&file).unwrap();
        crate::vvx::read(&after).expect("the figure reads back");
        assert_eq!(drawn(&after), drawn(&before), "every voxel where it stood");
        let m = ed.model.as_ref().unwrap();
        assert_eq!(names(&ed), ["car", "part"]);
        assert_eq!((m.parts[0].parent.clone(), m.parts[1].parent.as_deref()), (None, Some("car")));
        assert!(ed.figure.as_ref().is_some_and(|f| f.selected == Some(part(&ed, "part"))), "the new part chosen");
        ed.undo();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before, "UNDO: the one frame back byte for byte");
        ed.follow_figure();
        assert!(ed.figure.is_none() && !ed.parted() && ed.scope == Scope::Frame(0));
        crate::editor::view::Shown::new(&ed).expect("shown as a frame again");
        ed.redo();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), after);
    }

    /// ADD PART in a model of one frame: the frame the root part (`body`
    /// where the file's name is no part's name), a voxel on top of it the
    /// new part; a model of several frames refuses, saying why.
    #[test]
    fn add_part_makes_a_model_of_one_frame_a_figure() {
        let (mut ed, mut view, dir) = one_frame("add-one", "my car.vvx", 1);
        let before = std::fs::read_to_string(dir.join("my car.vvx")).unwrap();
        act(&mut ed, &mut view, Action::AddPart(None));
        let after = std::fs::read_to_string(dir.join("my car.vvx")).unwrap();
        assert_eq!(names(&ed), ["body", "part"]);
        let (old, new) = (drawn(&before), drawn(&after));
        assert!(old.is_subset(&new) && new.len() == old.len() + 1, "the frame as it stood, one voxel more");
        let (one, _) = new.difference(&old).next().copied().unwrap();
        assert_eq!(one[2], 4, "on top of it");
        assert_eq!(view.renaming_part, ed.figure.as_ref().and_then(|f| f.selected), "named to type");
        // An animation of two frames: refused, said.
        let (mut ed, mut view, dir) = one_frame("add-two", "walk.vvx", 2);
        let text = std::fs::read_to_string(dir.join("walk.vvx")).unwrap();
        assert!(ed.cannot_have_parts().is_some_and(|w| w.contains("several frames")));
        assert_eq!(ed.cannot_part(), ed.cannot_have_parts());
        act(&mut ed, &mut view, Action::AddPart(None));
        ed.select_from_pivot(IVec3::new(-3, -3, 0), IVec3::new(3, 3, 3));
        act(&mut ed, &mut view, Action::SelNewPart);
        assert!(!ed.parted() && std::fs::read_to_string(dir.join("walk.vvx")).unwrap() == text, "nothing changed");
    }

    /// Glass is seen as glass in the editor, as the game draws it (the
    /// owner: opaque, one could not see how it would look): a frame's glass
    /// voxels are see-through cubes, not the world's, and still met by the
    /// tools; the same in the frame made a figure, the part chosen and the
    /// figure whole; a material set to glass comes in at once.
    #[test]
    fn glass_is_drawn_see_through_and_met_by_the_tools() {
        use crate::editor::view::{Figures, Shown, frame};
        let (mut ed, mut view, dir) = one_frame("glass", "car.vvx", 1);
        let figures = Figures::load(&dir, &crate::config::GameConfig::default());
        let light = crate::config::LightConfig::default();
        // The cabin's glass at x 1, y 1, z 2 of the box (the far row first).
        let glass = IVec3::new(1, 1, 2);
        let clear = |cubes: &[crate::render::Instance]| cubes.iter().filter(|c| c.fx[1] < 255 && c.fx[1] > 0).count();
        let mut shown = Shown::new(&ed).unwrap();
        assert_eq!(shown.world.get(glass), 0, "not in the opaque world");
        assert_eq!(shown.glass.len(), 4, "the four panes");
        assert!(shown.glass.iter().all(|c| c.color[3] < 255 && c.fx[1] == c.color[3]));
        let pic = frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert_eq!(clear(&pic.instances), 4);
        assert!(pic.see_through >= 4, "drawn far to near after the opaque ones");
        // Met by the tools: straight down onto a pane (the roof cut away).
        ed.set_slice(2, Vec3::new(1.5, 1.5, 50.0));
        ed.set_slice_at(2, 2);
        assert_eq!(ed.aim(Vec3::new(1.5, 1.5, 50.0), Vec3::NEG_Z).map(|h| h.cell), Some(glass));
        ed.set_slice(2, Vec3::ZERO);
        // Its material plain again: in the world at once.
        ed.colour = [0x9f, 0xd8, 0xff];
        let plain = crate::editor::frames::with_word(ed.material().unwrap(), 0, 0.0);
        ed.set_material(plain);
        shown.update(&ed).unwrap();
        assert!(shown.world.get(glass) != 0 && shown.glass.is_empty(), "plain: opaque");
        ed.undo();
        shown.update(&ed).unwrap();
        assert_eq!((shown.world.get(glass), shown.glass.len()), (0, 4), "glass again");
        // Made a figure: the part chosen (the roof) round the root, and the
        // figure whole, draw the panes see-through too.
        ed.select_from_pivot(IVec3::new(-3, -3, 3), IVec3::new(3, 3, 3));
        act(&mut ed, &mut view, Action::SelNewPart);
        let shown = Shown::new(&ed).unwrap();
        let pic = frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert!(pic.instances.iter().filter(|c| c.fx[1] < 255).count() >= 4);
        let root = part(&ed, "car");
        act(&mut ed, &mut view, Action::Part(root));
        let shown = Shown::new(&ed).unwrap();
        assert_eq!(shown.glass.len(), 4, "the root's panes, chosen");
        act(&mut ed, &mut view, Action::WholeFigure);
        let shown = Shown::new(&ed).unwrap();
        let pic = frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert_eq!(pic.instances.iter().filter(|c| c.fx[1] < 255 && c.fx[1] != crate::editor::figure::DIMMED).count(), 4, "the figure whole: its panes");
    }

    /// FLOOR off: the plate under a model is neither drawn nor met, so a
    /// ray from below reaches the model's underside; on again, as always.
    /// A level keeps its floor.
    #[test]
    fn floor_off_lets_the_tools_work_from_below() {
        use crate::editor::view::{Figures, Shown, frame};
        let (mut ed, mut view, dir) = one_frame("floor", "car.vvx", 1);
        let figures = Figures::load(&dir, &crate::config::GameConfig::default());
        let light = crate::config::LightConfig::default();
        let up = |ed: &Editor| ed.aim(Vec3::new(2.5, 2.5, -50.0), Vec3::Z);
        assert_eq!(up(&ed).map(|h| h.cell.z), Some(-1), "the floor met first");
        let shown = Shown::new(&ed).unwrap();
        let with = frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0).instances.len();
        act(&mut ed, &mut view, Action::Floor);
        assert!(!ed.floor && view.floor_changed);
        assert_eq!(up(&ed), Some(crate::editor::model::Hit { cell: IVec3::new(2, 2, 0), normal: IVec3::NEG_Z }), "the model's underside");
        // Nothing under the model: nothing met.
        assert_eq!(ed.aim(Vec3::new(0.5, 0.5, 50.0), Vec3::NEG_Z), None);
        let without = frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0).instances.len();
        assert_eq!(with - without, 6 * 5, "the plate's cells gone");
        act(&mut ed, &mut view, Action::Floor);
        assert_eq!(up(&ed).map(|h| h.cell.z), Some(-1));
    }

    /// ADD EMPTY with the figure whole chosen: an empty part `group` the new
    /// root at the pivot, the old root hanging on it where it stands and
    /// turning where it turned, every other part's rig as it was; one
    /// step, UNDO gives the file back byte for byte. Chosen, named to type;
    /// warned of nothing.
    #[test]
    fn an_empty_part_becomes_the_root_and_nothing_rehangs() {
        let (mut ed, mut view, dir) = figure("empty-root");
        let (before, file) = (rig(&ed), text(&dir));
        let steps = ed.steps();
        act(&mut ed, &mut view, Action::AddEmpty(None));
        assert_eq!(ed.steps(), steps + 1, "one step");
        assert_eq!(names(&ed), ["body", "head", "arm", "hand", "group"]);
        let f = ed.figure.as_ref().unwrap();
        let group = part(&ed, "group");
        assert_eq!((f.selected, view.renaming_part), (Some(group), Some(group)), "chosen, named to type");
        assert_eq!((f.parts()[group].parent, f.parts()[group].empty), (None, Some(IVec3::ZERO)), "the root, at the pivot");
        assert_eq!(f.tree[0].0, group, "first in the list");
        let now = rig(&ed);
        assert_eq!(now["body"], (Some("group".to_string()), before["body"].1), "the old root on it, turning where it turned");
        for name in ["head", "arm", "hand"] {
            assert_eq!(now[name], before[name], "{name} re-hung");
        }
        assert!((0..f.parts().len()).all(|p| f.warnings(p).is_empty()), "nothing to warn of");
        assert!(text(&dir).contains("\n[[part]]\nname = \"group\"\n") && !text(&dir).contains("name = \"group\"\nat"), "{}", text(&dir));
        ed.undo();
        assert_eq!(text(&dir), file, "UNDO: the file as it was");
        ed.redo();
        assert_eq!(rig(&ed), now);
        // Again with the figure whole: on the empty root, at the pivot.
        act(&mut ed, &mut view, Action::WholeFigure);
        act(&mut ed, &mut view, Action::AddEmpty(None));
        assert_eq!(rig(&ed)["group_2"].0.as_deref(), Some("group"));
    }

    /// ADD EMPTY on a part: an empty part hanging on it where its joint is;
    /// the voxel tools set nothing in it (no step), TRIM has nothing to
    /// cut, CLONE, MIRROR, LIKE and MERGE are refused with why; ADD PART on
    /// it sets a voxel in its cell, hanging on it; MOVE takes what hangs on
    /// it along; REMOVE hangs that on its parent where it stands.
    #[test]
    fn an_empty_part_on_a_part_holds_no_voxels() {
        let (mut ed, mut view, dir) = figure("empty-on");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        let joint = ed.figure.as_ref().unwrap().file_joint(arm);
        act(&mut ed, &mut view, Action::AddEmpty(None));
        let e = part(&ed, "group");
        let f = ed.figure.as_ref().unwrap();
        assert_eq!((f.parts()[e].parent, f.parts()[e].empty), (Some(arm), Some(joint.floor().as_ivec3())));
        assert_eq!(f.file_joint(e), joint, "turning where the arm does");
        assert!(ed.shaping_empty());
        // The tools: nothing set, no step.
        let (steps, file) = (ed.steps(), text(&dir));
        for tool in [crate::editor::model::Tool::Add, crate::editor::model::Tool::Box, crate::editor::model::Tool::Fill] {
            ed.choose(tool);
            let hit = crate::editor::model::Hit { cell: IVec3::ZERO, normal: IVec3::Z };
            if tool == crate::editor::model::Tool::Box {
                ed.begin_form(hit, false);
            } else {
                ed.press(hit);
            }
            ed.release();
        }
        ed.choose(crate::editor::model::Tool::Pointer);
        assert_eq!((ed.steps(), text(&dir)), (steps, file), "an empty part takes no voxel");
        assert!(!ed.can_trim(), "TRIM quiet");
        assert!(ed.cannot_clone(e).is_some() && ed.cannot_clone_mirrored(e).is_some() && ed.cannot_link(e).is_some());
        assert!(ed.cannot_merge(e, arm).is_some() && ed.cannot_merge(arm, e).is_some());
        assert!(!ed.possible_sources(arm).contains(&"group".to_string()), "no clone of an empty part");
        act(&mut ed, &mut view, Action::ClonePart(e, false));
        assert_eq!(ed.steps(), steps, "CLONE refused");
        // ADD PART on it: a voxel in its cell, hanging on it.
        act(&mut ed, &mut view, Action::AddPart(None));
        let new = part(&ed, "part");
        let m = ed.model.as_ref().unwrap();
        let o = m.parts.iter().find(|o| o.name == "part").unwrap();
        assert_eq!((o.low, o.parent.as_deref()), (joint.floor().as_ivec3(), Some("group")));
        assert_eq!(ed.figure.as_ref().unwrap().parts()[new].parent, Some(part(&ed, "group")));
        // MOVE the empty part: the new part along.
        let e = part(&ed, "group");
        act(&mut ed, &mut view, Action::Part(e));
        let low = |ed: &Editor, n: &str| ed.model.as_ref().unwrap().parts.iter().find(|o| o.name == n).unwrap().low;
        let (was_e, was_p) = (low(&ed, "group"), low(&ed, "part"));
        ed.move_part(IVec3::X * 2);
        assert_eq!((low(&ed, "group"), low(&ed, "part")), (was_e + IVec3::X * 2, was_p + IVec3::X * 2));
        // REMOVE it: the part hangs on the arm where it stands.
        let e = part(&ed, "group");
        act(&mut ed, &mut view, Action::RemovePart(e));
        assert_eq!(rig(&ed)["part"].0.as_deref(), Some("arm"));
        assert_eq!(low(&ed, "part"), was_p + IVec3::X * 2);
    }

    /// The last part with voxels cannot go while empty parts remain; a
    /// part's row dropped on an empty part's hangs it there.
    #[test]
    fn an_empty_part_takes_parts_dropped_on_it() {
        let (mut ed, mut view, _dir) = figure("empty-drop");
        act(&mut ed, &mut view, Action::AddEmpty(None));
        let (group, head) = (part(&ed, "group"), part(&ed, "head"));
        ed.reparent(head, group).unwrap();
        assert_eq!(rig(&ed)["head"].0.as_deref(), Some("group"));
        assert!(ed.reparent(part(&ed, "group"), part(&ed, "body")).is_err(), "the root hangs on nothing");
        for name in ["head", "arm", "hand"] {
            let p = part(&ed, name);
            if ed.cannot_remove(p).is_none() {
                act(&mut ed, &mut view, Action::RemovePart(p));
            }
        }
        assert!(ed.cannot_remove(part(&ed, "body")).is_some_and(|w| w.contains("voxels")), "{:?}", ed.cannot_remove(part(&ed, "body")));
    }

    /// ADD EMPTY in a model of one frame: one step makes it a figure, the
    /// empty part `group` its root at the pivot, the frame a part named as
    /// the file hanging on it, every voxel where it stood; UNDO gives the
    /// frame back byte for byte.
    #[test]
    fn a_model_of_one_frame_becomes_a_figure_by_add_empty() {
        let (mut ed, mut view, dir) = one_frame("one-frame-empty", "car.vvx", 1);
        let before = std::fs::read_to_string(dir.join("car.vvx")).unwrap();
        act(&mut ed, &mut view, Action::AddEmpty(None));
        let after = std::fs::read_to_string(dir.join("car.vvx")).unwrap();
        assert_eq!(names(&ed), ["car", "group"]);
        assert_eq!(drawn(&after), drawn(&before), "every voxel where it stood");
        assert_eq!(rig(&ed)["car"].0.as_deref(), Some("group"));
        let f = ed.figure.as_ref().unwrap();
        assert_eq!((f.tree[0].0, f.selected), (part(&ed, "group"), Some(part(&ed, "group"))));
        ed.undo();
        assert_eq!(std::fs::read_to_string(dir.join("car.vvx")).unwrap(), before);
    }
}

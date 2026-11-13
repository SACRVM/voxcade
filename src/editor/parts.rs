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
        match self.chosen_pose().filter(|_| self.posing) {
            Some([r, p, l]) => crate::session::turning(r, p, l) * rest,
            None => rest,
        }
    }

    /// The chosen part's pose (POSE), 0 where it has none.
    pub fn chosen_pose(&self) -> Option<[f32; 3]> {
        let f = self.figure.as_ref()?;
        let name = &f.parts().get(f.selected?)?.name;
        Some(self.pose.get(name).copied().unwrap_or_default())
    }

    /// What the rings turn: the pose while POSE is lit, else the rest.
    pub fn turn_values(&self) -> Option<[f32; 3]> {
        if self.posing { self.chosen_pose() } else { self.part_rotation() }
    }

    /// The chosen part's pose about one axis (as `set_part_rotation`'s)
    /// set to `degrees`: drawn, nothing written, no step.
    pub fn set_pose(&mut self, axis: usize, degrees: f32) {
        let Some(name) = self.figure.as_ref().and_then(|f| Some(f.parts().get(f.selected?)?.name.clone())) else { return };
        let to = degrees.rem_euclid(360.0);
        let to = if to > 180.0 { to - 360.0 } else { to };
        let entry = self.pose.entry(name.clone()).or_default();
        entry[axis.min(2)] = to;
        if *entry == [0.0; 3] {
            self.pose.remove(&name);
        }
        self.follow_figure();
    }

    /// POSE on or off (the pose kept); a part chosen shows its rings.
    pub fn set_posing(&mut self, on: bool) {
        self.release();
        self.posing = on;
        self.follow_figure();
    }

    /// RESET: no pose.
    pub fn reset_pose(&mut self) {
        self.pose.clear();
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
                let turns = self.pose.get(name)?;
                let keys: Vec<String> = ["rotation", "pitch", "roll"].iter().zip(turns).filter(|(_, v)| **v != 0.0).map(|(k, v)| format!("{k} = {}", num(*v))).collect();
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
                    m.parts.push(Placed { name: name.clone(), frame: m.frames.len() - 1, low: IVec3::ZERO, parent: None, joint: None, rotation: [0.0; 3] });
                    m.parts.len() - 1
                }
            };
            set_cells(m, k, &cells);
            let o = &mut m.parts[k];
            (o.parent, o.joint, o.rotation) = (parent, joint, rotation);
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
        self.part_rotating = false;
        self.follow_figure();
    }

    /// ADD PART: a new part of one voxel of the color held, set on top of
    /// the chosen part (on its top voxel nearest its middle, where it is
    /// seen), else on top of the figure (its highest part), hanging on that
    /// part; chosen, its name the next free `part`; a step. (A part with no
    /// voxel would not be in the file, nor seen.)
    pub fn add_part(&mut self) {
        self.follow_figure();
        let chosen = self.figure.as_ref().and_then(|f| f.selected).filter(|_| !self.painting_whole);
        let Some(m) = self.model.as_ref() else { return };
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
        let on = match chosen.and_then(|p| self.placed(p)) {
            Some(k) => top(k).map(|c| (k, c)),
            None => (0..m.parts.len()).filter_map(|k| top(k).map(|c| (k, c))).max_by_key(|&(_, c)| c.z),
        };
        let Some((k, below)) = on else { return };
        let name = self.free_part_name("part");
        let colour = self.colour;
        let (parent, at) = (m.parts[k].name.clone(), below + IVec3::Z);
        let new = name.clone();
        self.acting_on(vec![name.clone()]);
        self.change_model_file(move |m| {
            let mut voxel = Block::boxed(IVec3::ONE);
            let _ = voxel.set(IVec3::ZERO, Some(colour));
            m.frames.push(voxel);
            m.parts.push(Placed { name: new, frame: m.frames.len() - 1, low: at, parent: Some(parent), joint: None, rotation: [0.0; 3] });
        });
        self.choose_named(&name);
    }

    /// Why part `p` cannot be removed (None: it can): the root part while
    /// others hang on it, the last part.
    pub fn cannot_remove(&self, p: usize) -> Option<&'static str> {
        let figure = self.figure.as_ref()?;
        if figure.parts().len() < 2 {
            return Some("A figure keeps one part at least");
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
        let joint = figure.joint(into) + figure.low.as_vec3();
        let rooted = figure.parts()[into].parent.is_none();
        let kids: Vec<_> = self.rehang(src, Some(target.clone())).into_iter().filter(|(n, ..)| *n != target).collect();
        let seen = self.merge_map(src, into);
        self.release();
        let acted = [gone, target.clone()].into_iter().chain(kids.iter().map(|(n, ..)| n.clone())).collect();
        self.acting_on(acted);
        self.change_model_file(move |m| {
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
            m.parts.push(copy);
        });
        self.choose_named(&name);
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
        self.painting_whole = true;
        self.follow_figure();
    }

    /// The stroke from the figure whole let go of: the figure whole again
    /// (a box set stays as set; its depth is no more to change).
    pub fn end_whole_stroke(&mut self) {
        if !self.painting_whole {
            return;
        }
        self.painting_whole = false;
        self.fresh = None;
        self.scope = Scope::Figure;
        self.follow_figure();
    }

    /// While a part's ring is dragged: by single degrees (Ctrl held) or by
    /// fives.
    pub fn set_fine(&mut self, fine: bool) {
        if let Some(d) = &mut self.joint_drag {
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
            "format = \"{}\"\nsize = [12, 6, 12]\n\n[palette]\na = \"#5a7aa8\"\nb = \"#d0c8b8\"\nc = \"#c89a4a\"\n{}{}{}{}",
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
            assert!(ed.painting_whole && ed.part_mode().is_some());
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
        assert!(!ed.part_rotating, "in MOVE, to be dragged away");
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
        super::super::rig_of(ed.model.as_ref().unwrap()).expect("a figure")
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
            ed.drag_joint(vp, screen, p, ray(p));
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
        assert!(ed.posing);
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
        assert!(!ed.pose.is_empty(), "the pose kept");
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
        assert_eq!((view.block.pitch, view.block.fov), (view.game.pitch, view.game.fov), "the game's camera");
        let f = crate::editor::view::frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert_eq!(f.models.len(), 4, "a draw from the bank for each part");
        assert_eq!(f.outline[3], 1.0, "the game's outline");
        // Posed: the arm's draw turned.
        act(&mut ed, &mut view, Action::Pose);
        ed.pose.insert("arm".into(), [0.0, 45.0, 0.0]);
        ed.follow_figure();
        let posed = crate::editor::view::frame(&ed, &view, &shown, &figures, &light, 16.0 / 9.0);
        assert_ne!(posed.models, f.models, "posed");
        act(&mut ed, &mut view, Action::GameView);
        assert!(view.before_game.is_none());
        assert_eq!(view.block, before);
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
            ed.drag_joint(vp, screen, to, ray(to));
        };
        // Focus lost: cancelled.
        grab(&mut ed, 5.0);
        assert_ne!(at(&ed), was, "moved while dragged");
        ed.end_drag(true);
        assert_eq!((text(&dir), ed.steps(), at(&ed)), (start.clone(), 0, was), "cancelled: as at the start");
        // The button found up: one step; a motion after moves nothing.
        grab(&mut ed, 5.0);
        ed.end_drag(false);
        assert_eq!(ed.steps(), 1);
        let moved = at(&ed);
        ed.drag_joint(vp, screen, glam::Vec2::new(10.0, 10.0), ray(glam::Vec2::new(10.0, 10.0)));
        assert_eq!(at(&ed), moved, "no drag: nothing moves");
        ed.undo();
        assert_eq!(text(&dir), start, "one UNDO: as at the start");
        // A tool's press without a let-go between.
        grab(&mut ed, 4.0);
        ed.choose(crate::editor::model::Tool::Pick);
        ed.press(crate::editor::model::Hit { cell: IVec3::ZERO, normal: IVec3::Z });
        assert!(ed.joint_drag().is_none(), "the press ended the drag");
        ed.drag_joint(vp, screen, glam::Vec2::new(10.0, 10.0), ray(glam::Vec2::new(10.0, 10.0)));
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
        ed.drag_joint(vp, screen, head + glam::Vec2::new(2.0, 1.0), ray(head + glam::Vec2::new(2.0, 1.0)));
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
}

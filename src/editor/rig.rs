//! A figure of parts (`Editor`): the figure made again as its file
//! changes, a part chosen, hidden, hung on another, its joint and place
//! moved by handles, the rig kept for the parts a step did not act on.

use super::*;

impl Editor {
    /// The figure whole made again if its frames or parts changed (a
    /// stroke, a step back, a parent or a joint set); its trouble said.
    pub fn follow_figure(&mut self) {
        let Some(model) = &self.model else { return };
        // A stroke going on changes the chosen part's voxels alone, which
        // the world shown draws: the figure (the other parts as they stand,
        // the rig) is made again once it is let go, not at every voxel (a
        // big figure took a frame's time and more each).
        let stroking = self.history.stroke.is_some() && self.figure.as_ref().is_some_and(|f| f.only_chosen_voxels_changed(model));
        if !stroking {
            let (figure, trouble) = Figure::follow(self.figure.take(), model, self.rigging.made_figure.take());
            self.figure = figure;
            if trouble.is_some() {
                self.message = trouble;
            }
        }
        if self.scope == Scope::Figure && self.figure.is_none() {
            self.scope = Scope::Frame(0);
        }
        self.sync_choice();
    }

    /// The part chosen in the figure's list as the scope says: the frame
    /// shaped is that part's.
    pub(super) fn sync_choice(&mut self) {
        let scope = self.scope;
        let (posing, pose) = (self.rigging.posing, &self.rigging.pose);
        if let Some(f) = &mut self.figure {
            f.selected = match scope {
                Scope::Frame(k) => f.frames.iter().position(|&m| m == k),
                _ => None,
            };
            // The pose drawn while POSE is lit.
            f.pose = if posing && !pose.is_empty() {
                f.model.parts.iter().map(|o| pose.get(&o.name).map_or(crate::session::PartPose::REST, |q| crate::session::PartPose { turn: crate::session::turning(q.turn[0], q.turn[1], q.turn[2]), offset: Vec3::from(q.offset) })).collect()
            } else {
                Vec::new()
            };
        }
    }

    /// The model is a figure of parts (by its file, even while its rig
    /// cannot be made).
    pub fn parted(&self) -> bool {
        self.model.as_ref().is_some_and(ModelFile::parted)
    }

    /// The frame of the figure's part chosen, shaped in the figure as it
    /// stands (None: no figure, or no part chosen).
    pub fn part_mode(&self) -> Option<usize> {
        match (self.scope, &self.figure) {
            (Scope::Frame(f), Some(_)) if self.parted() => Some(f),
            _ => None,
        }
    }

    /// Where voxel 0, 0, 0 of what is shown stands from the model's pivot:
    /// the figure's low corner, or the chosen part's frame's.
    pub fn origin(&self) -> Option<IVec3> {
        match self.scope {
            Scope::Figure => self.figure.as_ref().map(|f| f.low),
            Scope::Frame(f) if self.figure.is_some() => self.model.as_ref().and_then(|m| m.origin(f)),
            _ => None,
        }
    }

    /// The whole figure's box in the grid shown (its low corner and size):
    /// the room the parts take round the one chosen.
    pub fn figure_box(&self) -> Option<(IVec3, IVec3)> {
        let figure = self.figure.as_ref()?;
        Some((figure.low - self.origin()?, figure.model.size))
    }

    /// A part chosen in the figure's list, to shape in the figure and to
    /// rig; chosen again, none. While PARENT is being chosen, the part
    /// clicked becomes the parent instead.
    pub fn choose_part(&mut self, p: usize) {
        if self.rigging.picking_parent {
            self.rigging.picking_parent = false;
            return self.set_parent(Some(p));
        }
        // (Chosen again: it stays chosen; the figure whole is its own row,
        // Escape, or a click on empty space.)
        let Some(frame) = self.figure.as_ref().and_then(|f| f.frames.get(p).copied()) else { return };
        self.release();
        self.drop_form();
        self.scope = if frame >= self.frames().len() { Scope::Figure } else { Scope::Frame(frame) };
        self.hint = None;
        self.sync_choice();
    }

    /// A part left out of FIGURE's picture, or shown again.
    pub fn hide_part(&mut self, p: usize) {
        if let Some(f) = &mut self.figure
            && let Some(h) = f.hidden.get_mut(p)
        {
            *h = !*h;
        }
    }

    /// Every part shown again.
    pub fn show_all_parts(&mut self) {
        if let Some(f) = &mut self.figure {
            f.hidden.iter_mut().for_each(|h| *h = false);
        }
    }

    /// The chosen part's place among the figure's parts.
    pub(super) fn chosen(&self) -> Option<usize> {
        self.figure.as_ref()?.selected
    }

    /// The chosen part hung on part `parent` by hand, or (None) on what
    /// the rules find; a step. A parent that hangs on the chosen part by
    /// hand, by way of others or not, would make a ring: refused, said.
    pub fn set_parent(&mut self, parent: Option<usize>) {
        self.follow_figure();
        let (Some(p), Some(figure)) = (self.chosen(), &self.figure) else { return };
        let names: Vec<String> = figure.parts().iter().map(|q| q.name.clone()).collect();
        if let Some(q) = parent {
            if let Some(why) = self.ring_with(p, q) {
                self.hint = Some(why);
                return;
            }
            self.hint = Some(format!("{} hangs on {} now (set by hand)", names[p], names[q]));
        } else {
            self.hint = Some(format!("{} hangs where the rules find again", names[p]));
        }
        let name = names[p].clone();
        let to = parent.map(|q| names[q].clone());
        self.change_model_file(|m| m.parts.iter_mut().filter(|o| o.name == name).for_each(|o| o.parent = to.clone()));
    }

    /// Why part `p` cannot hang on `q` (a ring through the parents set by
    /// hand), if it cannot.
    pub fn ring_with(&self, p: usize, q: usize) -> Option<String> {
        let model = self.model.as_ref()?;
        let figure = self.figure.as_ref()?;
        let name = |k: usize| figure.parts()[k].name.as_str();
        if p == q {
            return Some(format!("{} cannot hang on itself", name(p)));
        }
        let index = |n: &str| figure.parts().iter().position(|o| o.name == n);
        let mut parents: Vec<Option<usize>> = figure.parts().iter().map(|o| model.parts.iter().find(|m| m.name == o.name).and_then(|m| m.parent.as_deref()).and_then(index)).collect();
        parents[p] = Some(q);
        let chain = crate::vox::ring(&parents, p)?;
        let names: Vec<&str> = chain.iter().map(|&k| name(k)).collect();
        Some(format!("{} hangs on {} by hand: that would make a ring ({})", name(q), name(p), names.join(" on ")))
    }

    /// The chosen part's joint where the figure has it now (to the nearest
    /// half voxel), moved by `by` (halves of a voxel), set by hand; a step.
    pub fn move_joint(&mut self, by: Vec3) {
        self.follow_figure();
        let (Some(p), Some(figure)) = (self.chosen(), &self.figure) else { return };
        // On halves of a voxel (a joint the rules found may lie between).
        let at = ((figure.file_joint(p)) * 2.0).round() / 2.0 + by;
        self.set_joint(Some(at));
    }

    /// The chosen part's joint set by hand at `at` (from the pivot), or
    /// (None) where the rules find it; a step.
    pub fn set_joint(&mut self, at: Option<Vec3>) {
        let (Some(p), Some(figure)) = (self.chosen(), &self.figure) else { return };
        let name = figure.parts()[p].name.clone();
        self.hint = None;
        self.change_model_file(|m| m.parts.iter_mut().filter(|o| o.name == name).for_each(|o| o.joint = at));
    }

    /// Where the chosen part's joint lies in the grid shown (its ring).
    pub fn joint_spot(&self) -> Option<Vec3> {
        let figure = self.figure.as_ref()?;
        let p = figure.selected.filter(|_| self.part_mode().is_some() && !self.root_chosen())?;
        let (lo, _) = self.figure_box()?;
        Some(figure.joint(p) + lo.as_vec3())
    }

    /// The press on the chosen part's joint's ring, the mouse's ray (in the
    /// grid shown) and the way the view looks: the joint dragged across
    /// the view, along the two axes that lie most across it (the third by
    /// the JOINT field), by halves of a voxel, until let go (one step) or
    /// Escape (back as it was, no step).
    pub fn grab_joint(&mut self, (origin, dir): (Vec3, Vec3), look: Vec3) -> bool {
        let (Some(p), Some(spot)) = (self.chosen(), self.joint_spot()) else { return false };
        let n = (0..3).max_by(|&a, &b| look[a].abs().total_cmp(&look[b].abs())).unwrap_or(1);
        let Some(hit) = plane_hit(origin, dir, n, spot[n]) else { return false };
        let start = self.figure.as_ref().map_or(Vec3::ZERO, |f| f.file_joint(p));
        self.end_drag(false);
        self.rigging.joint_drag = Some(JointDrag { drag: None, start, steps: self.history.past.len(), moved: false, part: false, applied: IVec3::ZERO, ring: Some((n, hit)), rest: [0.0; 3], fine: false, press: None, applied_f: Vec3::ZERO });
        true
    }

    /// The press on an arrow of the chosen part itself (MOVE, `h` as
    /// `part_handles` gave them): the part dragged along its axis by whole
    /// voxels, those hanging on it along, until let go (one step) or
    /// Escape (back as it was, no step).
    pub fn grab_part(&mut self, drag: super::super::handles::Drag, h: super::super::handles::Handles) -> bool {
        // A selection's arrow: its voxels dragged.
        if self.selection().is_some() && !self.rigging.posing {
            self.end_drag(false);
            self.grab_selection();
            self.rigging.joint_drag = Some(JointDrag { drag: Some((drag, h)), start: Vec3::ZERO, steps: self.history.past.len(), moved: false, part: true, applied: IVec3::ZERO, ring: None, rest: [0.0; 3], fine: false, press: None, applied_f: Vec3::ZERO });
            return true;
        }
        // (The figure whole: its arrows move every part.)
        if self.chosen().is_none() && self.scope != Scope::Figure {
            return false;
        }
        self.end_drag(false);
        let rest = self.turn_values().unwrap_or_default();
        self.rigging.joint_drag = Some(JointDrag { drag: Some((drag, h)), start: Vec3::ZERO, steps: self.history.past.len(), moved: false, part: true, applied: IVec3::ZERO, ring: None, rest, fine: false, press: self.part_mode().map(|_| self.local_to_pivot()), applied_f: Vec3::ZERO });
        true
    }

    /// The chosen part's arrows (MOVE): along X, Y, Z from its box's
    /// middle in the grid shown, their heads clear of its box, the heads
    /// only grabbing (the tools work on the shafts' line).
    /// Where the grid shown is drawn: a figure's part is shaped at rest
    /// (its rest rotation and its parents'), the view looking through that
    /// (`View::rest`); else as it is.
    pub fn rest_view(&self) -> glam::Mat4 {
        match (self.part_mode(), &self.figure, self.figure_box()) {
            (Some(_), Some(f), Some((lo, _))) => f.selected.map_or(glam::Mat4::IDENTITY, |p| f.rest_map(p, lo)),
            _ => glam::Mat4::IDENTITY,
        }
    }

    pub fn part_handles(&self, view_proj: glam::Mat4) -> Option<super::super::handles::Handles> {
        // A selection: its arrows (MOVE), whatever is chosen.
        if let Some(h) = self.selection_handles(view_proj) {
            return Some(h);
        }
        // POSE: only its rings.
        if self.rigging.posing {
            return self.rest_rings(view_proj);
        }
        self.part_handles_as(view_proj, self.rigging.part_rotating)
    }

    /// The chosen part's handles in ROTATE (`rotating`) or MOVE, whichever
    /// is lit (the bar at the part stands clear of both).
    pub fn part_handles_as(&self, view_proj: glam::Mat4, rotating: bool) -> Option<super::super::handles::Handles> {
        if self.rigging.painting_whole {
            return None;
        }
        // ROTATE: the chosen part's rings, about its joint.
        if rotating
            && let Some(h) = self.rest_rings(view_proj)
        {
            return Some(h);
        }
        // The figure whole: its arrows move every part together (as the
        // root's do).
        let size = match (self.scope, &self.figure) {
            (Scope::Figure, Some(f)) => f.model.size.as_vec3(),
            _ => {
                self.figure.as_ref()?.selected.filter(|_| self.part_mode().is_some())?;
                self.shape()?.size.as_vec3()
            }
        };
        let centre = size / 2.0;
        let mut h = super::super::handles::Handles::at(centre, super::super::handles::world_size(view_proj, centre, 0.1));
        // MOVE's arrows: LOCAL along the part's own axes as it stands (its
        // parents' rests and its own; in the grid shown, which is its own),
        // GLOBAL along the figure's.
        let back = if self.rigging.move_global { self.rest_view().to_scale_rotation_translation().1.inverse() } else { glam::Quat::IDENTITY };
        h.arrows = [Some(back * Vec3::X), Some(back * Vec3::Y), Some(back * Vec3::Z)];
        h.body = (0..8).map(|k| Vec3::new(if k & 1 == 0 { 0.0 } else { size.x }, if k & 2 == 0 { 0.0 } else { size.y }, if k & 4 == 0 { 0.0 } else { size.z })).collect();
        h.heads_only = true;
        Some(h)
    }

    /// Where the chosen part stands from the pivot (its `at` in the file,
    /// before any raise).
    pub fn part_at(&self) -> Option<IVec3> {
        let (figure, model) = (self.figure.as_ref()?, self.model.as_ref()?);
        let name = &figure.parts()[figure.selected?].name;
        model.parts.iter().find(|o| &o.name == name).map(|o| o.low)
    }

    /// Where the chosen part stands from the pivot: its `at` and `offset`,
    /// and in POSE its pose's offset too.
    pub fn part_place(&self) -> Option<Vec3> {
        let k = self.chosen_placed_index()?;
        let o = &self.model.as_ref()?.parts[k];
        let posed = if self.rigging.posing { self.chosen_posed().map_or(Vec3::ZERO, |p| Vec3::from(p.offset)) } else { Vec3::ZERO };
        Some(o.low.as_vec3() + o.offset + posed)
    }

    /// Whether the chosen part is the figure's root (it hangs on nothing:
    /// no joint of its own).
    pub fn root_chosen(&self) -> bool {
        self.figure.as_ref().and_then(|f| Some(f.parts()[f.selected?].parent.is_none())).unwrap_or(false)
    }

    /// The parts that go along when part `p` moves: it and those hanging
    /// on it, theirs too (as the figure hangs them, by hand or by the
    /// rules), by their names.
    pub(super) fn moving_with(&self, p: usize) -> Vec<String> {
        let Some(figure) = &self.figure else { return Vec::new() };
        let parts = figure.parts();
        let mut out = vec![p];
        let mut k = 0;
        while k < out.len() {
            let q = out[k];
            let more: Vec<usize> = (0..parts.len()).filter(|&c| parts[c].parent == Some(q) && !out.contains(&c)).collect();
            out.extend(more);
            k += 1;
        }
        out.into_iter().map(|q| parts[q].name.clone()).collect()
    }

    /// The chosen part moved by `by` voxels, those hanging on it along: its
    /// place (`at`) in the file, and a joint set by hand with it (a hinge is
    /// on the leaf; a joint the rules find is found anew); a step. The
    /// root moves the whole figure from its pivot (where it stands in the
    /// game).
    pub fn move_part(&mut self, by: IVec3) {
        self.follow_figure();
        if by == IVec3::ZERO {
            return;
        }
        // No part chosen (the figure whole): every part.
        let names = match self.chosen() {
            Some(p) => self.moving_with(p),
            None if self.scope == Scope::Figure => self.model.as_ref().map_or(Vec::new(), |m| m.parts.iter().map(|o| o.name.clone()).collect()),
            None => return,
        };
        // Nowhere it cannot be seen: a part stays within `FAR` voxels of
        // the pivot (a model holds 256 each way).
        let far = self.model.as_ref().is_some_and(|m| m.parts.iter().filter(|o| names.contains(&o.name)).any(|o| (o.low + by).abs().max_element() > FAR));
        if far {
            self.hint = Some(format!("A part goes no further than {FAR} voxels from the pivot"));
            return;
        }
        let keep = self.chosen().zip(self.chosen_placed_index()).and_then(|(p, k)| self.joint_kept(p, k).map(|j| (k, j)));
        self.rigging.acting = Some(names.clone());
        self.change_model_file(|m| {
            if let Some((k, j)) = keep {
                m.parts[k].joint = Some(j);
            }
            for o in m.parts.iter_mut().filter(|o| names.contains(&o.name)) {
                o.low += by;
                if let Some(j) = &mut o.joint {
                    *j += by.as_vec3();
                }
            }
        });
        self.follow_figure();
    }

    /// The chosen part moved by `by` voxels in the file's directions (its
    /// parent's), fractions too: what makes whole voxels goes into its
    /// `at`, with what hangs on it and its joint set by hand (as
    /// `move_part`), the rest into its `offset`, kept within half a voxel
    /// (the part's cells move with it, so nothing shifts against a grid);
    /// one step.
    pub fn move_part_by(&mut self, by: Vec3) {
        self.follow_figure();
        let (Some(p), Some(k)) = (self.chosen(), self.chosen_placed_index()) else { return };
        let Some(m) = self.model.as_ref() else { return };
        let off = m.parts[k].offset + by;
        let whole = off.round();
        let rest = ((off - whole) * 1000.0).round() / 1000.0;
        let whole = whole.as_ivec3();
        let names = self.moving_with(p);
        if m.parts.iter().filter(|o| names.contains(&o.name)).any(|o| (o.low + whole).abs().max_element() > FAR) {
            self.hint = Some(format!("A part goes no further than {FAR} voxels from the pivot"));
            return;
        }
        let keep = self.joint_kept(p, k);
        self.rigging.acting = Some(names.clone());
        self.change_model_file(|m| {
            if let Some(j) = keep {
                m.parts[k].joint = Some(j);
            }
            for o in m.parts.iter_mut().filter(|o| names.contains(&o.name)) {
                o.low += whole;
                if let Some(j) = &mut o.joint {
                    *j += whole.as_vec3();
                }
            }
            m.parts[k].offset = rest;
        });
        self.follow_figure();
    }

    /// A part rotated at rest whose joint the rules find (where it meets
    /// its parent): that joint as it is now, to state before the part
    /// moves, so that it turns about the same point where it goes (else the
    /// joint found anew would swing it elsewhere).
    pub(super) fn joint_kept(&self, p: usize, k: usize) -> Option<Vec3> {
        let (m, f) = (self.model.as_ref()?, self.figure.as_ref()?);
        let o = &m.parts[k];
        (o.joint.is_none() && o.rotation != [0.0; 3] && f.parts()[p].parent.is_some()).then(|| f.file_joint(p))
    }

    /// The chosen part's place among the file's parts.
    pub(super) fn chosen_placed_index(&self) -> Option<usize> {
        let f = self.figure.as_ref()?;
        let name = &f.parts().get(f.selected?)?.name;
        self.model.as_ref()?.parts.iter().position(|o| &o.name == name)
    }

    /// Whether the chosen part stands turned (its parents' rests and its
    /// own): LOCAL and GLOBAL differ only then.
    pub fn part_turned(&self) -> bool {
        self.part_mode().is_some() && self.rest_view().to_scale_rotation_translation().1.angle_between(glam::Quat::IDENTITY) > 1e-4
    }

    /// The grid shown to the model's pivot: a part's own (turned at rest,
    /// moved by its offset) where it stands; else as it is.
    pub fn local_to_pivot(&self) -> glam::Mat4 {
        glam::Mat4::from_translation(self.origin().unwrap_or(IVec3::ZERO).as_vec3()) * self.rest_view()
    }

    /// The joint being dragged, if one is.
    pub fn joint_drag(&self) -> Option<&JointDrag> {
        self.rigging.joint_drag.as_ref()
    }

    /// The mouse dragged on with a joint's arrow (as `Drag::amount` takes
    /// it): the joint where the drag puts it, the drag's one step.
    pub fn drag_joint(&mut self, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) {
        self.rigging.in_drag = true;
        self.drag_joint_now(view_proj, screen, mouse, ray);
        self.rigging.in_drag = false;
    }

    pub(super) fn drag_joint_now(&mut self, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) {
        let Some(mut d) = self.rigging.joint_drag.take() else { return };
        // A click on a handle moves nothing: only a drag that has gone a
        // few pixels.
        if let Some((drag, _)) = &mut d.drag
            && !drag.travels(mouse)
        {
            self.rigging.joint_drag = Some(d);
            return;
        }
        // A part's arrow (MOVE, LOCAL or GLOBAL): measured in its grid as
        // it was at the press; whole voxels along the arrow (Shift: tenths),
        // into its `at` and `offset` (`move_part_by`).
        if let (Some((drag, h)), Some(p0)) = (&mut d.drag, d.press)
            && matches!(drag.grip, super::super::handles::Grip::Arrow(_))
            && self.selecting.sel_drag.is_none()
        {
            let fix = self.local_to_pivot().inverse() * p0;
            let back = fix.inverse();
            let amount = drag.amount(h, view_proj * fix, screen, mouse, (back.transform_point3(ray.0), back.transform_vector3(ray.1).normalize_or_zero()));
            let along = if d.fine { (amount * 10.0).round() / 10.0 } else { amount.round() };
            let way = self.own_rest() * h.direction(drag.grip).unwrap_or(Vec3::ZERO);
            let to = way * along;
            if (to - d.applied_f).abs().max_element() > 1e-4 {
                self.rigging.join_next = d.moved && self.history.past.len() > d.steps;
                self.move_part_by(to - d.applied_f);
                self.rigging.join_next = false;
                (d.applied_f, d.moved) = (to, true);
            }
            self.rigging.joint_drag = Some(d);
            return;
        }
        if let Some((drag, h)) = &mut d.drag {
            // The part: the grid shown goes with it, so the ray and the
            // view as they were at the press.
            // (A selection's: the grid moves only where a box grows.)
            let back = self.selection_drift().unwrap_or(d.applied.as_vec3());
            let vp = view_proj * glam::Mat4::from_translation(-back);
            let amount = drag.amount(h, vp, screen, mouse, (ray.0 + back, ray.1));
            // A ring (ROTATE): the part's rest rotation about its joint,
            // by fives (Ctrl: by single degrees), one step for the drag.
            if let super::super::handles::Grip::Ring(a) = drag.grip {
                let axis = [1, 2, 0][a.min(2)];
                let way = if axis == 0 { -amount } else { amount };
                // (Shift: by single degrees.)
                let snap = if d.fine { 1.0 } else { 5.0 };
                let to = ((d.rest[axis] + way) / snap).round() * snap;
                if self.rigging.posing {
                    // A pose: nothing written, no step.
                    self.set_pose(axis, to);
                } else if self.part_rotation().is_some_and(|r| r[axis] != to) {
                    self.rigging.join_next = d.moved && self.history.past.len() > d.steps;
                    self.set_part_rotation(axis, to);
                    self.rigging.join_next = false;
                    d.moved = true;
                }
                self.rigging.joint_drag = Some(d);
                return;
            }
            // Its arrows lie along the axes the part moves by in the file
            // (its parent's, the part's own rest turned back).
            let to = match drag.grip {
                super::super::handles::Grip::Arrow(a) => {
                    let mut v = IVec3::ZERO;
                    v[a] = amount.round() as i32;
                    v
                }
                _ => (h.direction(drag.grip).unwrap_or(Vec3::ZERO) * amount).round().as_ivec3(),
            };
            if self.selecting.sel_drag.is_some() {
                if to != d.applied {
                    let joined = d.moved && self.history.past.len() > d.steps;
                    self.move_selection_to(to, self.selecting.sel_keep, joined);
                    (d.applied, d.moved) = (to, true);
                }
                self.rigging.joint_drag = Some(d);
                return;
            }
            if to != d.applied {
                self.rigging.join_next = d.moved && self.history.past.len() > d.steps;
                self.move_part(to - d.applied);
                self.rigging.join_next = false;
                (d.applied, d.moved) = (to, true);
            }
        } else if let Some((n, from)) = d.ring
            && let Some(hit) = plane_hit(ray.0, ray.1, n, from[n])
        {
            let mut by = hit - from;
            by[n] = 0.0;
            // By halves of a voxel (Shift: tenths).
            let step = if d.fine { 10.0 } else { 2.0 };
            let at = d.start + (by * step).round() / step;
            if let (Some(p), Some(figure)) = (self.chosen(), &self.figure) {
                let now = figure.file_joint(p);
                if now != at {
                    // One step for the whole drag: the first change makes
                    // it, the next ones take its place.
                    self.rigging.join_next = d.moved && self.history.past.len() > d.steps;
                    self.set_joint(Some(at));
                    self.rigging.join_next = false;
                    self.follow_figure();
                    d.moved = true;
                }
            }
        }
        self.rigging.joint_drag = Some(d);
    }

    /// Escape while a joint is dragged: as it was, no step.
    pub fn cancel_joint(&mut self) -> bool {
        let Some(d) = self.rigging.joint_drag.take() else { return false };
        // A selection dragged: back where it was, no step.
        if let Some(s) = self.selecting.sel_drag.take() {
            if d.moved && self.history.past.len() > d.steps {
                self.undo();
                self.history.future.pop();
            }
            self.selecting.selection = Some(s.selection().clone());
            return true;
        }
        // A pose dragged: as it was.
        if self.rigging.posing && d.part && d.drag.as_ref().is_some_and(|(g, _)| matches!(g.grip, super::super::handles::Grip::Ring(_))) {
            for (axis, rest) in d.rest.into_iter().enumerate() {
                self.set_pose(axis, rest);
            }
            return true;
        }
        if d.moved && self.history.past.len() > d.steps {
            self.undo();
            self.history.future.pop();
        }
        true
    }

    /// After a change to a figure (from `before`): every part it did not
    /// act on (`acting`, else the chosen part) whose parent or joint the
    /// rules would now find otherwise keeps them, stated in the file (its
    /// parent by name, its joint from the pivot), so that no step re-hangs
    /// or re-hinges a part nobody touched (a copy laid over its original,
    /// a voxel set where two parts meet).
    pub(super) fn pin_rig(&mut self, before: &Shaped) {
        let acting = self.rigging.acting.take();
        let Some(model) = &self.model else { return };
        if !model.parted() {
            return;
        }
        let acting = acting.unwrap_or_else(|| self.chosen().and_then(|p| self.figure.as_ref().map(|f| vec![f.parts()[p].name.clone()])).unwrap_or_default());
        // The rig before: the figure shown, where it is still the one made
        // of `before` (a stroke's), else made again.
        let was = match self.figure.as_ref().filter(|f| f.made_of(before)) {
            Some(f) => Some(f.rig()),
            None => {
                let mut old = model.clone();
                old.reshape(before);
                rig_of(&old)
            }
        };
        let made = Figure::of(model).and_then(|f| f.ok());
        let (Some(was), Some(now)) = (was, made.as_ref().map(Figure::rig)) else { return };
        let pins: Vec<(String, Option<String>, Vec3)> = now
            .iter()
            .filter(|(name, _)| !acting.contains(name))
            .filter_map(|(name, rig)| {
                let old = was.get(name)?;
                // (A parent that went keeps nothing: what hung on it was
                // acted on.)
                let parent_stays = old.0.as_ref().is_none_or(|q| now.contains_key(q));
                // (Worked out through the figure's box, a joint comes back
                // a hair off in the last digits: no change.)
                let moved = old.0 != rig.0 || (old.1 - rig.1).abs().max_element() > 1e-3;
                (moved && parent_stays).then(|| (name.clone(), old.0.clone(), old.1))
            })
            .collect();
        // Nothing to pin: the figure just made is the model's as it is.
        if pins.is_empty() {
            self.rigging.made_figure = made;
            return;
        }
        let Some(model) = &mut self.model else { return };
        for (name, parent, joint) in pins {
            if let Some(o) = model.parts.iter_mut().find(|o| o.name == name) {
                // The root stays the root (nothing to state); else both. A
                // joint the file states is the file's: never rewritten
                // (its number kept to the last digit).
                if parent.is_some() {
                    o.parent = parent;
                    if o.joint.is_none() {
                        o.joint = Some(joint);
                    }
                }
            }
        }
    }

    /// The parts that the next step acts on, by name (`pin_rig`).
    pub(crate) fn acting_on(&mut self, names: Vec<String>) {
        self.rigging.acting = Some(names);
    }
}

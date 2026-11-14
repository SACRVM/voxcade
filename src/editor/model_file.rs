//! A model's frames and file (`Editor`): frames stepped through, copied
//! and removed, TRIM, a change to the file as one step, a colour's
//! material.

use super::*;

impl Editor {
    /// The next frame of the model shown (`by` on, back when negative,
    /// round at the ends).
    pub fn step_frame(&mut self, by: i32) {
        let (Scope::Frame(f), count) = (self.scope, self.frames().len()) else { return };
        if self.part_mode().is_some() {
            return;
        }
        self.release();
        self.scope = Scope::Frame((f as i32 + by).rem_euclid(count.max(1) as i32) as usize);
    }

    /// A copy of the frame shown after it, shown; a step.
    pub fn copy_frame(&mut self) {
        let Scope::Frame(f) = self.scope else { return };
        // A figure's frames are its parts.
        if self.parted() {
            return;
        }
        self.change_model(|frames| frames.insert(f + 1, frames[f].clone()));
        self.scope = Scope::Frame(f + 1);
    }

    /// The frame shown taken out (not the last one); a step.
    pub fn delete_frame(&mut self) {
        let Scope::Frame(f) = self.scope else { return };
        if self.frames().len() < 2 || self.parted() {
            return;
        }
        self.change_model(|frames| {
            frames.remove(f);
        });
        self.scope = Scope::Frame(f.min(self.frames().len() - 1));
    }

    /// The model's box cut down to its voxels, every frame's alike (the
    /// frames keep their places to each other, the model its pivot: it
    /// stands on its lowest voxel); a figure's part's box alone, the part
    /// staying where it stands. A step.
    pub fn trim(&mut self) {
        // The figure whole: its box as tight as its parts allow.
        if self.scope == Scope::Figure {
            return self.change_model_file(|m| m.size = m.tight_size());
        }
        let part = self.part_mode();
        let Some((lo, hi)) = self.voxel_bounds(part) else { return };
        // Where the box shown moves to: the part's low corner, or the
        // smallest box about the pivot.
        let (shift, size) = match part {
            Some(_) => (-lo, hi - lo + 1),
            None => {
                let p = crate::vvx::pivot(self.grid());
                let size = super::super::frames::least(lo - p, hi - p);
                (crate::vvx::pivot(size) - p - IVec3::Z * lo.z, size)
            }
        };
        if part.is_none() {
            self.history.moved += shift;
        }
        self.change_model_file(|m| {
            m.parts.iter_mut().filter(|p| part == Some(p.frame)).for_each(|p| p.low -= shift);
            for (_, frame) in m.frames.iter_mut().enumerate().filter(|&(k, _)| part.is_none_or(|f| f == k)) {
                let mut cut = Block::boxed(size);
                cut.palette = frame.palette.clone();
                for k in 0..cut.voxels.len() {
                    cut.voxels[k] = frame.get(cut.at(k) - shift);
                }
                *frame = cut;
            }
        });
    }

    /// The lowest and highest voxel of the frames (`part`: of that frame
    /// alone), in their boxes; None where they hold none.
    fn voxel_bounds(&self, part: Option<usize>) -> Option<(IVec3, IVec3)> {
        let frames = self.frames().iter().enumerate().filter(|&(k, _)| part.is_none_or(|f| f == k)).map(|(_, f)| f);
        let cells = frames.flat_map(|f| (0..f.voxels.len()).filter(|&k| f.voxels[k] != 0).map(|k| f.at(k)));
        cells.fold(None, |b, p| Some(b.map_or((p, p), |(lo, hi): (IVec3, IVec3)| (lo.min(p), hi.max(p)))))
    }

    /// Whether TRIM has something to cut: the figure whole's box, a
    /// part's own box, a model of frames' box (about its pivot) larger
    /// than its voxels need. (A level's block keeps its N×N×N.)
    pub fn can_trim(&self) -> bool {
        match self.scope {
            Scope::Figure => self.model.as_ref().is_some_and(|m| !m.is_tight()),
            Scope::Frame(_) => {
                let part = self.part_mode();
                let Some((lo, hi)) = self.voxel_bounds(part) else { return false };
                match part {
                    Some(_) => lo != IVec3::ZERO || hi + 1 != self.grid(),
                    None => {
                        let p = crate::vvx::pivot(self.grid());
                        super::super::frames::least(lo - p, hi - p) != self.grid()
                    }
                }
            }
            _ => false,
        }
    }

    /// The model's frames and a figure's parts as they stand.
    /// The model as a step keeps it: the newest a step keeps, shared,
    /// while the model still stands so; else a copy.
    pub(super) fn shaped(&self) -> Arc<Shaped> {
        match (&self.model, &self.history.model_now) {
            (Some(m), Some(now)) if m.is(now) => Arc::clone(now),
            (m, _) => Arc::new(m.as_ref().map_or_else(Shaped::default, ModelFile::shaped)),
        }
    }

    /// A change to the model's frames as a step, saved (nothing if it
    /// changes nothing).
    pub(super) fn change_model(&mut self, change: impl FnOnce(&mut Vec<Block>)) {
        self.change_model_file(|m| change(&mut m.frames));
    }

    /// A change to the model's frames or a figure's parts as a step, saved
    /// (nothing if it changes nothing).
    pub(super) fn change_model_file(&mut self, change: impl FnOnce(&mut ModelFile)) {
        // (What a drag let go of here commits is its own step.)
        let acting = self.rigging.acting.take();
        self.release();
        self.rigging.acting = acting;
        // A drag's next change joins the step its first made.
        let joined = match (self.rigging.join_next, self.history.past.pop()) {
            (true, Some(Step::Model(before, _))) => Some(before),
            (_, last) => {
                self.history.past.extend(last);
                None
            }
        };
        let before = self.shaped();
        let Some(model) = &mut self.model else { return };
        change(model);
        model.list_new_colours();
        // A figure's box as tight as its parts allow at every step's end
        // (a drag's at its let-go: `release`), as it grows: a part that
        // flew far and came back leaves no huge box behind.
        if model.parted() && !self.rigging.in_drag {
            model.size = model.tight_size();
        }
        self.pin_rig(&before);
        if self.model.as_ref().is_none_or(|m| m.is(&before)) && joined.is_none() {
            return;
        }
        let before = joined.unwrap_or(before);
        let step = Step::Model(before, self.shaped());
        self.commit(step);
        self.history.tuning = None;
    }

    /// What the colour held is made of in the model (None without one).
    pub fn material(&self) -> Option<crate::vvx::Mat> {
        Some(self.model.as_ref()?.material(self.colour))
    }

    /// The colour held made of `mat` in the model; a step, joined to the
    /// last if that set the same colour's material.
    pub fn set_material(&mut self, mat: crate::vvx::Mat) {
        let key = self.colour;
        if self.material().is_none_or(|m| m == mat) {
            return;
        }
        self.release();
        let before = self.shaped();
        let Some(model) = &mut self.model else { return };
        model.set_material(key, mat);
        let after = self.shaped();
        let first = match self.history.past.last() {
            Some(Step::Model(first, _)) if self.history.tuning == Some(key) => Some(first.clone()),
            _ => None,
        };
        if first.is_some() {
            self.history.past.pop();
        }
        let step = Step::Model(first.unwrap_or(before), after);
        self.commit(step);
        self.history.tuning = Some(key);
    }

    /// A colour no voxel has any more taken out of the model's palette
    /// (its sign goes with it); a step.
    pub fn forget_colour(&mut self, key: [u8; 3]) {
        self.change_model_file(|m| {
            m.forget(key);
        });
    }
}

//! BOX and SPHERE (`Editor`): a footprint dragged out on a face's plane,
//! set where it was drawn, fresh until the next action (+ and - its depth,
//! BEVEL).

use super::*;

impl Editor {
    /// Whether the tool is BOX or SPHERE (in a block or a model; BOX among
    /// a level's blocks too, its cells the level's).
    pub fn forms(&self) -> bool {
        match self.scope {
            Scope::Level => self.tool == Tool::Box && self.layer == Layer::Blocks,
            _ => matches!(self.tool, Tool::Box | Tool::Sphere),
        }
    }

    /// BOX or SPHERE pressed at a hit: a footprint begins on the face's
    /// plane, before it (or, to `cut`, in the voxel itself); the shape set
    /// before is fresh no more.
    pub fn begin_form(&mut self, hit: Hit, cut: bool) {
        if self.drag_open() {
            self.end_drag(false);
        }
        self.forms.fresh = None;
        let normal = way_out(hit);
        let start = if cut { hit.cell } else { hit.cell + normal };
        self.forms.drawing = Some(Drawing { start, normal, end: start, cut });
    }

    /// The footprint dragged on to where a ray (in the grid's cells) meets
    /// its plane.
    pub fn draw_form(&mut self, origin: Vec3, dir: Vec3) {
        let Some(d) = &mut self.forms.drawing else { return };
        let a = axis_of(d.normal);
        if dir[a].abs() < 1e-6 {
            return;
        }
        let t = (d.start[a] as f32 + 0.5 - origin[a]) / dir[a];
        if t <= 0.0 {
            return;
        }
        let mut end = (origin + dir * t).floor().as_ivec3();
        end[a] = d.start[a];
        // A model's box holds 256 voxels each way at most.
        d.end = end.clamp(d.start - 255, d.start + 255);
    }

    /// The box BOX or SPHERE shows now, its low and high corner: the
    /// footprint being dragged out (as deep as `depth`), or the shape set
    /// last while it is fresh, where it stands.
    pub fn form_box(&self) -> Option<(IVec3, IVec3)> {
        if let Some(d) = &self.forms.drawing {
            let (a, way) = (axis_of(d.normal), deep_way(d.normal, d.cut));
            let (mut lo, mut hi) = (d.start.min(d.end), d.start.max(d.end));
            if way[a] > 0 {
                hi[a] = d.start[a] + self.forms.depth.max(1) - 1;
            } else {
                lo[a] = d.start[a] - self.forms.depth.max(1) + 1;
            }
            return Some((lo, hi));
        }
        let f = self.forms.fresh?;
        Some((f.lo + f.shift, f.lo + f.shift + f.size - 1))
    }

    /// The cells of the shape in `lo..=hi` (with SYMMETRY their mirrors
    /// too, about the box as it stands).
    pub(super) fn shape_cells(&self, lo: IVec3, hi: IVec3) -> Vec<IVec3> {
        let mut cells = form_cells(self.tool, lo, hi, self.forms.bevel);
        if self.symmetry {
            let mirrored: Vec<IVec3> = cells.iter().filter_map(|&c| self.mirror(c)).collect();
            let seen: std::collections::BTreeSet<[i32; 3]> = cells.iter().map(|c| c.to_array()).collect();
            cells.extend(mirrored.into_iter().filter(|m| !seen.contains(&m.to_array())));
        }
        cells
    }

    /// The cells the footprint being dragged out would fill: what the view
    /// shows of it.
    pub fn form_cells_now(&self) -> Vec<IVec3> {
        match (self.forms.drawing.is_some(), self.form_box()) {
            (true, Some((lo, hi))) => self.shape_cells(lo, hi),
            _ => Vec::new(),
        }
    }

    /// The footprint let go: the shape set right where it was drawn
    /// (filling in the colour held, only where empty with `only_empty`, or
    /// cutting away as it was drawn), one step; fresh until the next action.
    pub fn set_drawn(&mut self, only_empty: bool) {
        let Some((lo, hi)) = self.form_box().filter(|_| self.forms.drawing.is_some()) else { return };
        let Some(d) = self.forms.drawing.take() else { return };
        let fresh = Fresh { lo, size: hi - lo + 1, way: deep_way(d.normal, d.cut), cut: d.cut, only_empty, shift: IVec3::ZERO, steps: 0 };
        self.forms.depth = fresh.size[axis_of(fresh.way)];
        self.set_fresh(fresh);
    }

    /// Sets `fresh` as one step and keeps it fresh (nothing set: none).
    pub(super) fn set_fresh(&mut self, mut fresh: Fresh) {
        let before = self.steps();
        let shift = self.place_cells(fresh.lo, fresh.lo + fresh.size - 1, fresh.cut, fresh.only_empty);
        if self.steps() == before {
            self.forms.fresh = None;
            return;
        }
        (fresh.shift, fresh.steps) = (shift, self.steps());
        self.forms.fresh = Some(fresh);
    }

    /// The fresh shape, if it is still the last step: its step taken back
    /// quietly, to be set again changed (+, -, BEVEL).
    pub(super) fn take_fresh_back(&mut self) -> Option<Fresh> {
        let fresh = self.forms.fresh.take().filter(|f| f.steps == self.steps() && self.forms.drawing.is_none() && self.history.stroke.is_none())?;
        let step = self.history.past.pop()?;
        self.go(&step, false);
        self.history.moved -= fresh.shift;
        Some(fresh)
    }

    /// A voxel deeper (`by` 1) or shallower (-1): the shape set last while
    /// it is fresh, in place (its step still one); else the depth the next
    /// footprint gets (the one being dragged out too).
    pub fn deepen(&mut self, by: i32) {
        let Some(mut fresh) = self.take_fresh_back() else {
            self.forms.depth = (self.forms.depth + by).clamp(1, 256);
            return;
        };
        let a = axis_of(fresh.way);
        let deeper = (fresh.size[a] + by).clamp(1, 256);
        // Drawn into the voxels or down a face: it grows from its far end.
        if fresh.way[a] < 0 {
            fresh.lo[a] -= deeper - fresh.size[a];
        }
        fresh.size[a] = deeper;
        self.forms.depth = deeper;
        self.set_fresh(fresh);
    }

    /// BEVEL on or off; the shape set last, while fresh, set again so.
    pub fn toggle_bevel(&mut self) {
        let fresh = self.take_fresh_back();
        self.forms.bevel = !self.forms.bevel;
        if let Some(fresh) = fresh {
            self.set_fresh(fresh);
        }
    }

    /// Escape: the footprint being dragged out let go, the shape set last
    /// fresh no more (it stays); whether there was either.
    pub fn drop_form(&mut self) -> bool {
        let had = self.forms.fresh.is_some() || self.forms.drawing.is_some();
        (self.forms.fresh, self.forms.drawing) = (None, None);
        had
    }

    /// Sets the shape in `lo..=hi`: filling in the colour held (only where
    /// empty with `only_empty`), or cutting away; mirrored too with
    /// SYMMETRY. A model's box grows to hold it (how far its voxels moved
    /// comes back); a block's stays, what lies outside left out. One step,
    /// saved.
    pub(super) fn place_cells(&mut self, lo: IVec3, hi: IVec3, cut: bool, only_empty: bool) -> IVec3 {
        let mut cells = self.shape_cells(lo, hi);
        if self.scope == Scope::Level {
            self.place_level_cells(cells, cut, only_empty);
            return IVec3::ZERO;
        }
        if cells.is_empty() || self.shape().is_none() {
            return IVec3::ZERO;
        }
        self.history.stroke = Some(match self.scope {
            Scope::Level | Scope::Figure => return IVec3::ZERO,
            Scope::Block(i) => Begun::Block(i, self.blocks[i].clone()),
            Scope::Frame(_) => Begun::Model(self.shaped()),
        });
        self.history.grown = IVec3::ZERO;
        let mut moved = IVec3::ZERO;
        if matches!(self.scope, Scope::Frame(_)) && !cut {
            let (lo, hi) = cells.iter().fold((cells[0], cells[0]), |(lo, hi), &c| (lo.min(c), hi.max(c)));
            match self.grow_model_to(lo, hi) {
                Some(shift) => {
                    cells.iter_mut().for_each(|c| *c += shift);
                    moved = shift;
                }
                None => self.message = Some("a model holds 256 voxels each way at most".into()),
            }
        }
        let colour = self.colour;
        let Some(block) = self.shape_mut() else { return moved };
        let mut trouble = None;
        for c in cells {
            if block.index(c).is_none() {
                continue;
            }
            let filled = block.get(c) != 0;
            let to = match (cut, filled) {
                (true, true) => None,
                (true, false) => continue,
                (false, true) if only_empty => continue,
                (false, _) => Some(colour),
            };
            if let Err(e) = block.set(c, to) {
                trouble = Some(format!("{e:#}"));
                break;
            }
        }
        if trouble.is_some() {
            self.message = trouble;
        }
        self.release();
        moved
    }

    /// BOX among a level's blocks: its cells take the block held, as it is
    /// turned (only empty ones with `only_empty`), or are emptied to
    /// `cut`; the level grows on its right, back and top to hold it (cells
    /// left of, before or under it left out: SIZE grows it there). One
    /// step, saved.
    fn place_level_cells(&mut self, cells: Vec<IVec3>, cut: bool, only_empty: bool) {
        let held = self.held;
        if cells.is_empty() || (!cut && held.block == 0) {
            return;
        }
        let inside = |c: &IVec3| c.cmpge(IVec3::ZERO).all() && c.cmplt(IVec3::splat(super::cells::MOST_CELLS)).all();
        let cells: Vec<IVec3> = cells.into_iter().filter(inside).collect();
        self.change_level(|l| {
            if !cut
                && let Some(hi) = cells.iter().copied().reduce(IVec3::max)
                && hi.cmpge(l.size).any()
            {
                super::cells::reframe(l, IVec3::ZERO, l.size.max(hi + 1));
            }
            for c in &cells {
                let Some(i) = l.index(*c) else { continue };
                l.cells[i] = match (cut, l.cells[i].block != 0) {
                    (true, _) => Cell::default(),
                    (false, true) if only_empty => continue,
                    (false, _) => held,
                };
            }
        });
    }

    /// A shape `size` big set at a face (`hit`, centred on it across, its
    /// depth out of it; `cut`: into the voxels), fresh: what `--shot
    /// --held` shows.
    pub fn set_form_at(&mut self, hit: Hit, size: IVec3, cut: bool) {
        let normal = way_out(hit);
        let c = if cut { hit.cell } else { hit.cell + normal };
        let (a, way) = (axis_of(normal), deep_way(normal, cut));
        let mut lo = c - (size - 1) / 2;
        lo[a] = if way[a] > 0 { c[a] } else { c[a] - size[a] + 1 };
        self.set_fresh(Fresh { lo, size, way, cut, only_empty: false, shift: IVec3::ZERO, steps: 0 });
    }
}

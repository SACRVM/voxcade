//! The tools on the grid shown (`Editor`): the one held, PICK while
//! Shift is held, where a ray meets the grid, and a stroke pressed,
//! dragged and let go (one step), a drag of any kind ended.

use super::*;

impl Editor {
    pub fn choose(&mut self, tool: Tool) {
        if tool != self.tool {
            self.forms.fresh = None;
        }
        if tool == Tool::Pick && self.tool != Tool::Pick {
            self.before_pick = self.tool;
        }
        // Another tool chosen by hand while Shift is held is the one that
        // stays when it is let go.
        if tool != Tool::Pick {
            self.shifted = false;
        }
        self.tool = tool;
    }

    /// Shift down (`down`) or up: while it is held the tool is PICK, and
    /// the tool before comes back when it is let go (a form held for BOX
    /// or SPHERE stays held meanwhile). Not begun in the middle of a
    /// stroke or of a footprint being dragged out, whose tool must not
    /// change under the mouse, nor where there is no PICK.
    pub fn shift_pick(&mut self, down: bool) {
        if down {
            // (Not while a handle is dragged: Shift is its fine step then.)
            let free = self.history.stroke.is_none() && self.forms.drawing.is_none() && self.rigging.joint_drag.is_none() && self.pieces.gripped.is_none() && Tool::of(self.scope, self.layer).contains(&Tool::Pick);
            if !self.shifted && self.tool != Tool::Pick && free {
                self.choose(Tool::Pick);
                self.shifted = true;
            }
        } else if self.shifted {
            self.shifted = false;
            if self.tool == Tool::Pick {
                self.tool = self.before_pick;
            }
        }
    }

    /// The tool the panels show the options of: the one Shift put aside
    /// while it is PICK, so they do not jump.
    pub fn resting_tool(&self) -> Tool {
        if self.shifted { self.before_pick } else { self.tool }
    }

    /// A pick made: the tool goes back to the one before PICK, unless Shift
    /// is still holding PICK.
    pub(super) fn picked(&mut self) {
        if !self.shifted {
            self.tool = self.before_pick;
        }
    }

    /// The colour of the voxel at `cell` held (the menu on a voxel's PICK
    /// COLOUR; the tool stays as it is).
    pub fn pick_at(&mut self, cell: IVec3) {
        if let Some(colour) = self.shape().and_then(|b| b.colour_at(cell)) {
            self.colour = colour;
        }
    }

    /// The voxel at `cell` taken away (the menu on a voxel's ERASE), as
    /// ERASE would; a step. The tool stays as it is.
    pub fn erase_at(&mut self, cell: IVec3) {
        if !matches!(self.scope, Scope::Block(_) | Scope::Frame(_)) {
            return;
        }
        let tool = self.tool;
        self.tool = Tool::Remove;
        self.press(Hit { cell, normal: IVec3::Z });
        self.release();
        self.tool = tool;
    }

    /// The block shown (in a block), or the one held (in the level).
    pub fn block_index(&self) -> Option<usize> {
        match self.scope {
            Scope::Block(i) => Some(i),
            Scope::Frame(_) | Scope::Figure => None,
            Scope::Level => (self.held.block as usize).checked_sub(1),
        }
    }

    /// The kind of the block shown or held.
    pub fn kind(&self) -> Option<Kind> {
        self.block_index().map(|i| self.level.blocks[i].kind)
    }

    /// The colours of the block shown, and those of all the level's blocks
    /// (each once, in the order the level lists its blocks).
    pub fn colours(&self) -> (Vec<[u8; 3]>, Vec<[u8; 3]>) {
        if let Some(model) = &self.model {
            return (self.shape().map(Block::colours).unwrap_or_default(), model.colours());
        }
        let own = self.block_index().map(|i| self.blocks[i].colours()).unwrap_or_default();
        let mut all: Vec<[u8; 3]> = Vec::new();
        for b in &self.blocks[..self.level.blocks.len()] {
            for c in b.colours() {
                if !all.contains(&c) {
                    all.push(c);
                }
            }
        }
        (own, all)
    }

    /// Whether a cell of the grid shown is filled, now or (`then`) as the
    /// stroke going on found it.
    pub(super) fn filled(&self, c: IVec3, then: bool) -> bool {
        match (self.scope, then.then_some(self.history.stroke.as_ref()).flatten()) {
            (Scope::Level, Some(Begun::Level(before))) => {
                let s = before.size;
                c.cmplt(s).all() && before.cells[(c.x + c.y * s.x + c.z * s.x * s.y) as usize].block != 0
            }
            (Scope::Level, _) => self.level.cell(c).block != 0,
            (Scope::Block(_), Some(Begun::Block(_, before))) => before.get(c) != 0,
            // (The model as the stroke found it, moved as its box grew.)
            (Scope::Frame(f), Some(Begun::Model(before))) => before.frames.get(f).is_some_and(|b| b.get(c - self.history.grown) != 0),
            (Scope::Block(_) | Scope::Frame(_), _) => self.shape().is_some_and(|b| b.get(c) != 0),
            (Scope::Figure, _) => false,
        }
    }

    /// Where a ray (in the shown grid's cells) meets it: against the
    /// surface the stroke going on began on.
    pub fn aim(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        let size = match self.scope {
            // The level grows right, back and up: room for one more cell.
            Scope::Level => self.level.size + 1,
            Scope::Block(_) | Scope::Frame(_) => self.grid(),
            // The figure whole with no part chosen: nothing to work on.
            Scope::Figure => return None,
        };
        // A figure's part is aimed at in the figure's room, its floor under
        // the figure: the other parts are not hit, the tools go through.
        // (What the slice cuts away is not met: the tools reach the cut
        // face and what lies behind it.)
        if let Some((lo, figure)) = self.part_mode().and_then(|_| self.figure_box()) {
            let lo = lo.min(IVec3::ZERO);
            let hi = (lo + figure).max(size);
            let hit = pick(origin - lo.as_vec3(), dir, hi - lo, |c| self.filled(c + lo, true) && !self.cut(c + lo))?;
            return Some(Hit { cell: hit.cell + lo, normal: hit.normal });
        }
        pick(origin, dir, size, |c| c.cmpge(IVec3::ZERO).all() && self.filled(c, true) && !self.cut(c))
    }

    /// The cell the tool works on at a hit (None: nothing there for it).
    pub fn target(&self, hit: Hit) -> Option<IVec3> {
        let c = match self.tool {
            Tool::Add => hit.cell + hit.normal,
            _ if hit.cell.z < 0 => return None,
            _ => hit.cell,
        };
        let (low, limit) = match self.scope {
            Scope::Level => (IVec3::ZERO, self.level.size + 1),
            // A model's box grows too, by ADD, a voxel on every side.
            Scope::Frame(_) if self.tool == Tool::Add => (IVec3::NEG_ONE, self.grid() + 1),
            Scope::Block(_) | Scope::Frame(_) => (IVec3::ZERO, self.grid()),
            Scope::Figure => return None,
        };
        (c.cmpge(low).all() && c.cmplt(limit).all() && !self.cut(c)).then_some(c)
    }

    /// The mouse pressed at a hit: PICK takes up, the others begin a stroke.
    pub fn press(&mut self, hit: Hit) {
        // A drag still open (its let-go never came): let go of it first,
        // its step made, before this one begins from what it left.
        if self.drag_open() {
            self.end_drag(false);
        }
        // POINTER changes nothing: in a level of blocks the block clicked
        // is held (as a click in the list holds one).
        if self.tool == Tool::Pointer {
            if self.scope == Scope::Level
                && let Some(c) = self.target(hit)
                && self.level.cell(c).block != 0
            {
                self.held = self.level.cell(c);
            }
            return;
        }
        if self.tool == Tool::Pick {
            if let Some(c) = self.target(hit) {
                match self.scope {
                    Scope::Level if self.level.cell(c).block != 0 => self.held = self.level.cell(c),
                    Scope::Block(_) | Scope::Frame(_) => {
                        if let Some(colour) = self.shape().and_then(|b| b.colour_at(c)) {
                            self.colour = colour;
                        }
                    }
                    _ => return,
                }
                self.picked();
            }
            return;
        }
        self.history.stroke = Some(match self.scope {
            Scope::Level => Begun::Level(self.level_state()),
            Scope::Block(i) => Begun::Block(i, self.blocks[i].clone()),
            Scope::Frame(_) => Begun::Model(self.shaped()),
            Scope::Figure => return,
        });
        self.history.grown = IVec3::ZERO;
        self.drag(hit);
    }

    /// The mouse dragged on to a hit, in a stroke.
    pub fn drag(&mut self, hit: Hit) {
        if self.history.stroke.is_none() {
            return;
        }
        let Some(mut c) = self.target(hit) else { return };
        // SYMMETRY: the voxel mirrored too (about the box as it stands).
        let mut mirrored = self.mirror(c);
        // ADD beyond a model's box grows it (left, forwards or down its
        // voxels move up by one), on both sides with SYMMETRY.
        if matches!(self.scope, Scope::Frame(_)) {
            let inside = |ed: &Self, p: IVec3| p.cmpge(IVec3::ZERO).all() && p.cmplt(ed.grid()).all();
            if !inside(self, c) {
                let Some(shift) = self.grow_model(c) else { return };
                c += shift;
                mirrored = mirrored.map(|m| m + shift);
            }
            if let Some(m) = mirrored
                && !inside(self, m)
            {
                mirrored = self.grow_model(m).map(|shift| {
                    c += shift;
                    m + shift
                });
            }
        }
        match self.scope {
            Scope::Level => {
                let cell = match self.tool {
                    Tool::Add if self.level.cell(c).block == 0 => self.held,
                    Tool::Paint if self.level.cell(c).block != 0 => self.held,
                    Tool::Remove => Cell::default(),
                    _ => return,
                };
                if cell.block == 0 && self.tool != Tool::Remove {
                    return;
                }
                if self.level.index(c).is_none() && !self.grow(c) {
                    return;
                }
                let i = self.level.index(c).unwrap();
                self.level.cells[i] = cell;
            }
            Scope::Figure => {}
            Scope::Block(_) | Scope::Frame(_) => {
                let flip = IVec3::new(-hit.normal.x, hit.normal.y, hit.normal.z);
                for (p, normal) in std::iter::once((c, hit.normal)).chain(mirrored.map(|m| (m, flip))) {
                    self.touch(p, normal);
                }
            }
        }
    }

    /// A block's or a model's voxel worked on by the tool (DEEP PAINT on
    /// into it against the face `normal`).
    pub(super) fn touch(&mut self, c: IVec3, normal: IVec3) {
        let (tool, held) = (self.tool, self.colour);
        let Some(block) = self.shape_mut() else { return };
        let done = if tool == Tool::Deep {
            // From the voxel hit on into the block, against the face.
            let mut p = c;
            let mut done = Ok(());
            while block.index(p).is_some() && done.is_ok() {
                if block.get(p) != 0 {
                    done = block.set(p, Some(held));
                }
                if normal == IVec3::ZERO {
                    break;
                }
                p -= normal;
            }
            done
        } else {
            let colour = match tool {
                Tool::Add if block.get(c) == 0 => Some(held),
                Tool::Paint if block.get(c) != 0 => Some(held),
                Tool::Remove => None,
                _ => return,
            };
            block.set(c, colour)
        };
        if let Err(e) = done {
            self.message = Some(format!("{e:#}"));
        }
    }

    /// With SYMMETRY, a block's or a model's voxel `c` mirrored left to
    /// right about the box's middle (None: off, in the level, or `c` on
    /// the middle).
    pub(super) fn mirror(&self, c: IVec3) -> Option<IVec3> {
        if !self.symmetry || self.scope == Scope::Level {
            return None;
        }
        // One of a pair with its partner there: the partner takes the
        // mirror (`land_on_partner`), not this part's far side.
        if self.symmetry_on_partner() {
            return None;
        }
        // A figure's part about the figure's middle: its two sides alike.
        let (lo, size) = self.part_mode().and_then(|_| self.figure_box()).unwrap_or((IVec3::ZERO, self.grid()));
        let m = IVec3::new(2 * lo.x + size.x - 1 - c.x, c.y, c.z);
        (m != c).then_some(m)
    }

    /// Makes the model's box reach voxel `c` (beside it, by one), every
    /// frame's: how far its voxels moved (on the low sides), None if it
    /// cannot (a frame holds at most 256 voxels each way).
    pub(super) fn grow_model(&mut self, c: IVec3) -> Option<IVec3> {
        if c.cmplt(IVec3::NEG_ONE).any() || c.cmpgt(self.grid()).any() {
            return None;
        }
        self.grow_model_to(c, c)
    }

    /// Makes the model's box hold `lo` to `hi`: how far its voxels moved
    /// in the grid shown, None if it cannot (256 voxels each way at most).
    /// A figure's part alone grows on the side reached, staying where it
    /// stands (its place goes down as its voxels go up in its box). A
    /// model's frames all alike, about the model's pivot, which stays
    /// under the model: across and deep the box grows on both sides as the
    /// pivot needs, the model raised by what is added under its floor.
    pub(super) fn grow_model_to(&mut self, low: IVec3, high: IVec3) -> Option<IVec3> {
        let old = self.grid();
        let (lo, hi) = (low.min(IVec3::ZERO), (high + 1).max(old));
        if (hi - lo).cmpgt(IVec3::splat(256)).any() {
            return None;
        }
        if lo == IVec3::ZERO && hi == old {
            return Some(IVec3::ZERO);
        }
        let part = self.part_mode();
        let (shift, size) = match part {
            Some(_) => (-lo, None),
            None => {
                let p = crate::vvx::pivot(old);
                let raise = IVec3::Z * (-lo.z).max(0);
                let size = super::super::frames::fit(old + raise, lo - p + raise, hi - 1 - p + raise);
                if size.cmpgt(IVec3::splat(256)).any() {
                    return None;
                }
                (crate::vvx::pivot(size) - p + raise, Some(size))
            }
        };
        let model = self.model.as_mut()?;
        if let Some(f) = part {
            model.parts.iter_mut().filter(|p| p.frame == f).for_each(|p| p.low -= shift);
        }
        for (k, frame) in model.frames.iter_mut().enumerate() {
            if part.is_some_and(|f| f != k) {
                continue;
            }
            let mut grown = Block::boxed(size.unwrap_or((frame.size + shift).max(hi - lo)));
            grown.palette = frame.palette.clone();
            for k in 0..frame.voxels.len() {
                let at = grown.index(frame.at(k) + shift).expect("inside the bigger box");
                grown.voxels[at] = frame.voxels[k];
            }
            *frame = grown;
        }
        self.history.grown += shift;
        // (A figure's part: the view follows the part's place instead,
        // `View::follow_origin`.)
        if part.is_none() {
            self.history.moved += shift;
        }
        Some(shift)
    }

    /// The mouse let go: the stroke, if it changed anything, is a step and
    /// is saved.
    pub fn release(&mut self) {
        // A drag of a part or a selection let go: the figure's box made
        // tight, in the drag's own step.
        if let Some(d) = self.rigging.joint_drag.take()
            && d.moved
            && self.model.as_ref().is_some_and(|m| m.parted() && !m.is_tight())
        {
            self.rigging.join_next = self.history.count() > d.steps;
            self.change_model_file(|m| m.size = m.tight_size());
            self.rigging.join_next = false;
        }
        self.let_go_of_slice();
        self.let_go_of_land();
        self.moving = None;
        self.pieces.dragging = None;
        self.pieces.gripped = None;
        self.rigging.joint_drag = None;
        let Some(stroke) = self.history.stroke.take() else { return };
        let step = match stroke {
            Begun::Level(before) => {
                let after = self.level_state();
                if Arc::ptr_eq(&after, &before) || after == before {
                    return;
                }
                Step::Level(before, after)
            }
            Begun::Block(i, before) => {
                if self.blocks[i] == before {
                    return;
                }
                Step::Block(i, before, self.blocks[i].clone())
            }
            Begun::Model(before) => {
                if let Some(m) = &mut self.model {
                    m.list_new_colours();
                }
                self.land_on_partner(&before);
                if let Some(m) = self.model.as_mut().filter(|m| m.parted()) {
                    m.size = m.tight_size();
                }
                self.pin_rig(&before);
                if self.model.as_ref().is_none_or(|m| m.is(&before)) {
                    return;
                }
                Step::Model(before, self.shaped())
            }
        };
        self.commit(step);
        self.history.tuning = None;
        self.pieces.tuned = None;
    }

    /// Whether a tool's stroke is open (pressed, not let go).
    pub fn stroke_open(&self) -> bool {
        self.history.stroke.is_some()
    }

    /// Whether a drag is open: a handle's (a part's, a selection's, a
    /// joint's, a piece's), the slice's, a piece over the floor, a tool's
    /// stroke, a footprint.
    pub fn drag_open(&self) -> bool {
        self.rigging.joint_drag.is_some() || self.pieces.gripped.is_some() || self.pieces.dragging.is_some() || self.moving.is_some() || self.history.stroke.is_some() || self.forms.drawing.is_some() || self.slicing()
    }

    /// A drag ended without its own let-go (the button found up, a press
    /// while it was open, the release outside the window): `cancel` (the
    /// window lost the focus, another button, Escape): all as before it,
    /// no step; else as let go where it is: one step, which UNDO takes
    /// back whole. Never a change without its step.
    pub fn end_drag(&mut self, cancel: bool) {
        if cancel {
            self.cancel_joint();
            self.cancel_drag();
            if let Some(stroke) = self.history.stroke.take() {
                self.undo_stroke(stroke);
            }
            self.moving = None;
        }
        // (A footprint never let go of sets nothing.)
        self.forms.drawing = None;
        self.release();
        self.let_go_of_selection();
        self.rigging.join_next = false;
        self.follow_figure();
    }
}

//! A level of blocks (`Editor`): its blocks (new, copied, removed,
//! turned, one opened to shape), its things on the grid.

use super::*;

/// A level of blocks holds this many cells each way at most in the editor.
pub const MOST_CELLS: i32 = 256;

/// The level's grid made `size` cells from its cell `lo` on (`lo` below
/// zero: grown on the low side; above: cut there): the cells kept where
/// they stay, the rest empty; what is set down (things, places, areas, the
/// terrain) moved with them, by as many voxels as the grid moved.
pub(super) fn reframe(level: &mut GridLevel, lo: IVec3, size: IVec3) {
    let s = size.max(IVec3::ZERO);
    let cells: Vec<Cell> = (0..(s.x * s.y * s.z) as usize).map(|k| level.cell(IVec3::new(k as i32 % s.x, k as i32 / s.x % s.y, k as i32 / (s.x * s.y)) + lo)).collect();
    let by = -lo * level.n;
    let moved = |at: &mut [i32; 3]| *at = (IVec3::from_array(*at) + by).to_array();
    level.things.iter_mut().for_each(|t| moved(&mut t.at));
    level.places.iter_mut().for_each(|p| moved(&mut p.at));
    level.areas.iter_mut().for_each(|a| moved(&mut a.at));
    if let Some(t) = &mut level.terrain {
        moved(&mut t.at);
    }
    (level.size, level.cells) = (s, cells);
}

impl Editor {
    /// A copy of the block shown or held, under a new number, held (and
    /// opened, when a block was open).
    pub fn clone_block(&mut self) -> Result<()> {
        let Some(i) = self.block_index() else { return Ok(()) };
        self.release();
        let (block, kind) = (self.blocks[i].clone(), self.level.blocks[i].kind);
        self.add_block(self.free_name(), block, kind)?;
        if let Scope::Block(_) = self.scope {
            self.scope = Scope::Block(self.level.blocks.len() - 1);
        }
        Ok(())
    }

    /// Takes the block shown or held out of the level: its cells empty, its
    /// file gone; a step (UNDO brings both back). Back to the level.
    pub fn delete_block(&mut self) {
        let Some(i) = self.block_index() else { return };
        self.release();
        let before = self.level_state();
        let mut after = LevelState::clone(&before);
        let entry = after.table.remove(i);
        let gone = i as u16 + 1;
        for c in &mut after.cells {
            if c.block == gone {
                *c = Cell::default();
            } else if c.block > gone {
                c.block -= 1;
            }
        }
        let step = Step::Delete(Box::new(Deletion { index: i, entry, block: self.blocks[i].clone(), before, after: Arc::new(after) }));
        self.go(&step, true);
        self.history.remember(&step, true);
        self.history.past.push(step);
        self.history.future.clear();
        self.history.bound();
    }

    /// The open block itself turned, its voxels with it (a side shaped as
    /// the front brought round); a step, saved.
    pub fn turn_block(&mut self, turn: Turn) {
        if let Some(f) = self.part_mode() {
            // A figure's part alone, about its box's middle.
            return self.change_model_file(|m| {
                let (was, now) = (m.frames[f].size, m.frames[f].turned(turn));
                let shift = (was - now.size) / 2;
                m.frames[f] = now;
                m.parts.iter_mut().filter(|p| p.frame == f).for_each(|p| p.low += shift);
            });
        }
        if let Scope::Frame(_) = self.scope {
            return self.change_model(|frames| frames.iter_mut().for_each(|f| *f = f.turned(turn)));
        }
        let Scope::Block(i) = self.scope else { return };
        self.release();
        let before = self.blocks[i].clone();
        self.blocks[i] = before.turned(turn);
        let step = Step::Block(i, before, self.blocks[i].clone());
        self.commit(step);
    }

    /// The level's tools to its blocks or its things; a tool the other
    /// layer lacks becomes ADD.
    pub fn switch(&mut self, layer: Layer) {
        self.release();
        self.layer = layer;
        if !Tool::of(self.scope, layer).contains(&self.tool) {
            self.tool = Tool::Add;
        }
    }

    /// Holds the game's kind `i` (its place in `kinds`) for ADD.
    pub fn hold_kind(&mut self, i: usize) {
        if let Some(k) = self.kinds.get(i) {
            self.held_kind = Some(k.clone());
        }
    }

    /// Where a thing stands for a hit on the level's voxels (`voxel`, the
    /// face it was entered through as `normal`; z -1 the floor): before the
    /// face, on a top or before a wall, in the middle of its row's depth so
    /// it stands in the lane. None: outside the level.
    pub fn thing_spot(&self, voxel: IVec3, normal: IVec3) -> Option<IVec3> {
        if normal == IVec3::ZERO {
            return None;
        }
        let n = self.level.n;
        let mut at = voxel + normal;
        let size = self.level.size * n;
        if at.x < 0 || at.x >= size.x || at.z < 0 {
            return None;
        }
        let row = at.y.div_euclid(n).clamp(0, self.level.size.y - 1);
        at.y = row * n + n / 2;
        Some(at)
    }

    /// The mouse pressed among the things: at a spot (where a new one would
    /// stand) and over a thing (its place in the list), either may be none.
    pub fn press_thing(&mut self, spot: Option<IVec3>, under: Option<usize>) {
        self.release();
        match self.tool {
            // POINTER changes nothing.
            Tool::Pointer => return,
            Tool::Pick => {
                // (A piece is no kind: pieces are set down among the
                // PIECES.)
                if let Some(t) = under.and_then(|i| self.level.things.get(i)).filter(|t| !t.is_piece()) {
                    self.held_kind = Some(t.kind.clone());
                    self.picked();
                }
                return;
            }
            Tool::Add if spot.is_none() || self.held_kind.is_none() => return,
            Tool::Remove | Tool::Move if under.is_none() => return,
            _ => {}
        }
        self.history.stroke = Some(Begun::Level(self.level_state()));
        match self.tool {
            Tool::Add => {
                let (kind, at) = (self.held_kind.clone().unwrap(), spot.unwrap());
                self.level.things.push(Thing::new(kind, at.to_array()));
            }
            Tool::Remove => {
                self.level.things.remove(under.unwrap());
            }
            Tool::Move => self.moving = under,
            _ => {}
        }
    }

    /// ALIGN ALL: every thing turned off the quarter turns faces its
    /// nearest way (`grid::Way::nearest`), one step back, written as every
    /// step is; the question is answered and the level opens.
    pub fn align_all(&mut self) {
        self.release();
        self.history.stroke = Some(Begun::Level(self.level_state()));
        for t in &mut self.level.things {
            if let Some(f) = t.facing.filter(|f| f.off_grid()) {
                t.facing = Some(crate::grid::Facing::Way(crate::grid::Way::nearest(f.bearing())));
            }
            // Tipped: the nearest quarter (a tie goes up), where it stands.
            for d in [&mut t.pitch, &mut t.roll] {
                if d.rem_euclid(90.0) != 0.0 {
                    *d = crate::grid::Way::nearest(*d).bearing();
                }
            }
        }
        self.release();
        self.question = None;
    }

    /// The mouse dragged on among the things: MOVE takes its thing along.
    pub fn drag_thing(&mut self, spot: Option<IVec3>) {
        if let (Some(i), Some(at)) = (self.moving, spot)
            && let Some(t) = self.level.things.get_mut(i)
        {
            t.at = at.to_array();
        }
    }

    /// The thing MOVE is dragging, if any.
    pub fn moving(&self) -> Option<usize> {
        self.moving
    }

    /// The held block turned (ROTATE, ROLL, MIRROR).
    pub fn turn(&mut self, turn: Turn) {
        self.held.turn = self.held.turn.then(turn);
    }

    /// Holds the level's block `i` (its place in the list).
    pub fn hold(&mut self, i: usize) {
        if i < self.level.blocks.len() {
            self.held = Cell { block: i as u16 + 1, turn: self.held.turn };
        }
    }

    /// Opens a block to shape it (a cell's block, or the held one).
    pub fn open_block(&mut self, i: usize) {
        if i < self.level.blocks.len() {
            self.release();
            self.scope = Scope::Block(i);
            self.layer = Layer::Blocks;
            self.shifted = false;
            if self.tool == Tool::Pick {
                self.tool = self.before_pick;
            }
            if let Some(&c) = self.blocks[i].colours().first()
                && !self.blocks[i].colours().contains(&self.colour)
            {
                self.colour = c;
            }
        }
    }

    /// Back to the level from a block, from a part to its figure whole.
    pub fn back_to_level(&mut self) {
        self.release();
        if let Scope::Frame(_) | Scope::Figure = self.scope {
            // From a figure's part: the figure with none chosen.
            if self.figure.is_some() {
                self.scope = Scope::Figure;
                self.sync_choice();
            }
            return;
        }
        if self.tool == Tool::Deep {
            self.tool = Tool::Paint;
        }
        if let Scope::Block(i) = self.scope {
            self.hold(i);
        }
        self.scope = Scope::Level;
    }

    /// Sets what the block shown or held is to the game.
    pub fn set_kind(&mut self, kind: Kind) {
        let Some(i) = self.block_index() else { return };
        if self.level.blocks[i].kind == kind {
            return;
        }
        let before = self.level_state();
        self.level.blocks[i].kind = kind;
        let step = Step::Level(before, self.level_state());
        self.commit(step);
    }

    /// A new block, held: drawn out of the tileset's tile `tile`, or empty;
    /// the tile's block if the level has it already.
    pub fn new_block(&mut self, tile: Option<i32>) -> Result<()> {
        self.release();
        let n = self.level.n;
        let (name, block) = match tile {
            Some(id) => {
                let name = id.to_string();
                if let Some(i) = self.level.blocks.iter().position(|b| b.name == name) {
                    self.hold(i);
                    return Ok(());
                }
                let file = blocks::path(&self.dir, &name);
                let block = if file.exists() {
                    Block::load(&file, n)?
                } else {
                    let picture = self.sheet.as_ref().and_then(|s| s.tile(id)).with_context(|| format!("the tileset has no tile {id}"))?;
                    Block::drawn_out(&picture, n)?
                };
                (name, block)
            }
            None => (self.free_name(), Block::empty(n)),
        };
        self.add_block(name, block, Kind::Solid)
    }

    /// A name for a new block: numbered on from the tiles and the blocks
    /// there are, and no file's.
    pub(super) fn free_name(&self) -> String {
        let taken = self.level.blocks.iter().filter_map(|b| b.name.parse::<i32>().ok()).max().unwrap_or(0);
        let mut k = taken.max(self.sheet.as_ref().map_or(0, |s| s.count)) + 1;
        while blocks::path(&self.dir, &k.to_string()).exists() {
            k += 1;
        }
        k.to_string()
    }

    /// A block saved and added to the level's list, held; a step.
    pub(super) fn add_block(&mut self, name: String, block: Block, kind: Kind) -> Result<()> {
        block.save(&blocks::path(&self.dir, &name))?;
        self.remember_path(&blocks::path(&self.dir, &name));
        let before = self.level_state();
        self.blocks.truncate(self.level.blocks.len());
        self.blocks.push(block);
        self.level.blocks.push(BlockEntry { name, kind });
        let step = Step::Level(before, self.level_state());
        self.commit(step);
        self.hold(self.level.blocks.len() - 1);
        Ok(())
    }

    /// Makes the grid reach cell `c` (right, back or up of it, by one):
    /// whether it does.
    pub(super) fn grow(&mut self, c: IVec3) -> bool {
        let old = self.level.size;
        if c.cmplt(IVec3::ZERO).any() || c.cmpgt(old).any() || c.cmpge(IVec3::splat(MOST_CELLS)).any() {
            return false;
        }
        reframe(&mut self.level, IVec3::ZERO, old.max(c + 1));
        true
    }

    /// FILL among the level's blocks at a hit: the cells of the block
    /// clicked joined to it face to face (`all`: every cell of it) take
    /// the block held, as it is turned; one step.
    pub fn fill_cells(&mut self, hit: Hit, all: bool) {
        let (was, held) = (self.level.cell(hit.cell), self.held);
        if was.block == 0 || held.block == 0 || was == held || self.level.index(hit.cell).is_none() {
            return;
        }
        let size = self.level.size;
        let at = |i: usize| IVec3::new(i as i32 % size.x, i as i32 / size.x % size.y, i as i32 / (size.x * size.y));
        let found: Vec<usize> = if all {
            self.level.cells.iter().enumerate().filter(|(_, c)| c.block == was.block).map(|(i, _)| i).collect()
        } else {
            let mut seen = std::collections::BTreeSet::from([self.level.index(hit.cell).expect("inside")]);
            let mut todo: Vec<usize> = seen.iter().copied().collect();
            while let Some(i) = todo.pop() {
                for d in crate::mesh::DIRS {
                    if let Some(j) = self.level.index(at(i) + d).filter(|&j| self.level.cells[j].block == was.block)
                        && seen.insert(j)
                    {
                        todo.push(j);
                    }
                }
            }
            seen.into_iter().collect()
        };
        self.change_level(|l| found.iter().for_each(|&i| l.cells[i] = held));
    }

    /// The level as a step keeps it: the newest a step keeps, shared,
    /// while the level still stands so; else a copy.
    pub(super) fn level_state(&self) -> Arc<LevelState> {
        match &self.history.level_now {
            Some(now) if now.is(&self.level) => Arc::clone(now),
            _ => Arc::new(LevelState::of(&self.level)),
        }
    }
}

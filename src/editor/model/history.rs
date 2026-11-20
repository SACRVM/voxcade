//! The steps (`Editor`): UNDO and REDO, a step committed and the level
//! saved at once.

use super::*;

impl Editor {
    /// Takes the last step back.
    pub fn undo(&mut self) {
        self.seal(self.selecting.selection.clone());
        self.before_going();
        if let Some(step) = self.history.past.pop()
            && self.go(&step, false)
        {
            let selected = matches!(step, Step::Selected(..));
            self.history.future.push(step);
            let key = self.history.count();
            if let Some((_, before, _)) = self.history.selections.iter().find(|s| s.0 == key && !selected) {
                self.selecting.selection = before.clone();
            }
        }
    }

    /// Takes the last step taken back again.
    pub fn redo(&mut self) {
        self.seal(self.selecting.selection.clone());
        self.before_going();
        if let Some(step) = self.history.future.pop()
            && self.go(&step, true)
        {
            let key = self.history.count();
            if let Some((_, _, Some(after))) = self.history.selections.iter().find(|s| s.0 == key && !matches!(step, Step::Selected(..))) {
                self.selecting.selection = after.clone();
            }
            self.history.past.push(step);
        }
    }

    /// What UNDO and REDO end first: the stroke, the fresh shape, the
    /// selection, the steps a setting would join; and another program's
    /// change is read first (then there is nothing to take back,
    /// `look_at_disk`).
    fn before_going(&mut self) {
        self.release();
        self.forms.fresh = None;
        self.selecting.selection = None;
        self.history.tuning = None;
        self.pieces.tuned = None;
        self.look_at_disk();
    }

    /// A step taken back or again, written; whether it still stands (the
    /// file changed meanwhile and it was laid on it or dropped: no).
    pub(super) fn go(&mut self, step: &Step, forward: bool) -> bool {
        let mut problem = None;
        match step {
            Step::Level(before, after) => {
                // What was locked stays locked where the step leaves as many
                // things (a move, a rename, a group: `Editor::change`'s rule).
                let (locked, count) = (self.locked_things(), self.level.things.len());
                self.set_level(if forward { after } else { before });
                if !locked.is_empty() && self.level.things.len() == count {
                    self.lock_again(&locked);
                }
            }
            Step::Block(i, before, after) => self.blocks[*i] = if forward { after } else { before }.clone(),
            Step::Selected(before, after) => self.selecting.selection = if forward { after } else { before }.clone(),
            Step::Model(before, after) => {
                if let Some(model) = &mut self.model {
                    model.reshape(if forward { after } else { before });
                    if let Scope::Frame(f) = self.scope {
                        self.scope = Scope::Frame(f.min(model.frames.len() - 1));
                    }
                    // (A figure made of one frame taken back: that frame.)
                    if self.scope == Scope::Figure && !model.parted() {
                        self.scope = Scope::Frame(0);
                    }
                }
            }
            Step::Delete(d) => {
                let file = blocks::path(&self.dir, &d.entry.name);
                let number = d.index as u16 + 1;
                if forward {
                    self.set_level(&d.after);
                    self.blocks.truncate(d.before.table.len());
                    self.blocks.remove(d.index);
                    if let Err(e) = std::fs::remove_file(&file)
                        && e.kind() != std::io::ErrorKind::NotFound
                    {
                        problem = Some(format!("removing {}: {e}", file.display()));
                    }
                    self.remember_path(&file);
                    if self.held.block == number {
                        self.held = Cell { block: (!self.level.blocks.is_empty()) as u16, turn: Turn::NONE };
                    } else if self.held.block > number {
                        self.held.block -= 1;
                    }
                    self.scope = Scope::Level;
                } else {
                    self.set_level(&d.before);
                    self.blocks.truncate(d.after.table.len());
                    self.blocks.insert(d.index, d.block.clone());
                    if let Err(e) = d.block.save(&file) {
                        problem = Some(format!("{e:#}"));
                    }
                    self.remember_path(&file);
                    self.held = Cell { block: number, turn: self.held.turn };
                }
            }
        }
        if self.held.block as usize > self.level.blocks.len() {
            self.held = Cell { block: (!self.level.blocks.is_empty()) as u16, turn: Turn::NONE };
        }
        if let Scope::Block(i) = self.scope
            && i >= self.level.blocks.len()
        {
            self.scope = Scope::Level;
        }
        self.keep_choice();
        self.history.remember(step, forward);
        let saved = self.save(step, forward);
        if problem.is_some() {
            self.message = problem;
        }
        matches!(saved, disk::Saved::Kept)
    }

    /// A stroke's state brought back: as it was when it began (Escape, a
    /// drag cancelled, the file changed under it).
    pub(super) fn undo_stroke(&mut self, begun: Begun) {
        self.history.linked = None;
        match begun {
            Begun::Level(before) => self.set_level(&before),
            Begun::Block(i, before) => {
                if let Some(b) = self.blocks.get_mut(i) {
                    *b = before;
                }
            }
            Begun::Model(before) => {
                if let Some(m) = &mut self.model {
                    m.reshape(&before);
                }
            }
        }
    }

    pub(super) fn set_level(&mut self, s: &LevelState) {
        self.level.size = s.size;
        self.level.cells = s.cells.clone();
        self.level.blocks = s.table.clone();
        self.level.things = s.things.clone();
        self.level.places = s.places.clone();
        self.level.areas = s.areas.clone();
        self.level.daylight = s.daylight.as_deref().cloned();
        self.level.camera = s.camera;
        self.level.terrain = s.terrain.as_deref().cloned();
        (self.level.location, self.level.persistent) = (s.location.clone(), s.persistent);
        // (Points no longer there are no longer chosen.)
        let t = self.level.terrain.as_ref();
        let has = |p: &glam::IVec2| t.is_some_and(|t| t.index(*p).is_some());
        self.land.selected.retain(has);
        self.land.chosen = self.land.chosen.filter(has);
    }

    /// How many steps were taken and not taken back (those gone too: the
    /// oldest go past `KEPT_STEPS` or `KEPT_BYTES`).
    pub fn steps(&self) -> usize {
        self.history.count()
    }

    /// Takes the last step back for good (a click that was the first of a
    /// double click).
    pub fn take_back(&mut self) {
        self.undo();
        self.history.future.pop();
    }

    /// A step taken: written (`save`, which looks at the disk first) and
    /// kept to take back; as laid on the file, or dropped, where another
    /// program changed the file meanwhile.
    pub(super) fn commit(&mut self, mut step: Step) {
        // The selection as this step finds it, for UNDO to bring back (as
        // it leaves it: kept by `seal`, later); the newest 16 steps'.
        let found = match &step {
            Step::Selected(before, _) => before.clone(),
            _ => self.selecting.selection.clone(),
        };
        self.seal(found.clone());
        let key = self.history.count();
        self.history.selections.retain(|s| s.0 < key);
        if !matches!(step, Step::Selected(..)) {
            self.history.selections.push((key, found, None));
            if self.history.selections.len() > 16 {
                self.history.selections.remove(0);
            }
        }
        self.follow_land(&mut step);
        match self.save(&step, true) {
            disk::Saved::Kept => {
                self.history.remember(&step, true);
                self.history.past.push(step);
                self.history.future.clear();
            }
            disk::Saved::Rebased(kept) => {
                if let Some(step) = &kept {
                    self.history.remember(step, true);
                }
                self.history.past.extend(kept.map(|s| *s));
            }
            disk::Saved::Dropped => {}
        }
        self.history.bound();
    }

    /// The newest step's selection after it, if not yet kept: `now`.
    fn seal(&mut self, now: Option<selection::Selection>) {
        if let Some(key) = self.history.count().checked_sub(1)
            && let Some(kept) = self.history.selections.iter_mut().find(|s| s.0 == key && s.2.is_none())
        {
            kept.2 = Some(now);
        }
    }

    /// A change of the selection alone (`change`: SELECT's rectangle let
    /// go, a double click, CLEAR, Escape) is a step of its own to take
    /// back, written nowhere.
    pub fn selecting(&mut self, change: impl FnOnce(&mut Self)) {
        let before = self.selection().cloned();
        change(self);
        let after = self.selection().cloned();
        if before != after {
            self.commit(Step::Selected(before, after));
        }
    }

    /// Writes the level over the text it was opened from
    /// (`grid::text::rewrite`): what did not change stays as it was.
    pub(super) fn save_level(&self) -> Result<String> {
        let mut names: Vec<&str> = self.level.blocks.iter().map(|b| b.name.as_str()).collect();
        names.sort_unstable();
        if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
            bail!("the level names block '{}' twice", w[0]);
        }
        let path = self.dir.join(&self.file);
        let text = crate::grid::text::rewrite(&self.source, &self.source_level, &self.level, &crate::grid::Legend::of_file(&path));
        blocks::save_at_once(&path, text.as_bytes())?;
        Ok(text)
    }
}

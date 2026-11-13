//! The editor's files on disk. The level editor once wrote its level from
//! what it held since it opened, over everything the game's agent had
//! changed in the file meanwhile (the owner's work, 2026-11-12). Now the
//! editor remembers each file it holds (the level, its blocks, the model)
//! as it last read or wrote it, a BLAKE3 of the bytes, and looks at the
//! disk once a second, when its window gets the focus, and right before
//! every write (`Editor::look_at_disk`, `Editor::save`):
//!
//! - a file changed by another program is read anew at once, the view and
//!   what was chosen (by name) kept; the steps from before are dropped,
//!   for a step back would bring the older file back over the newer;
//! - a step about to be written over a file that changed is laid on the
//!   file as it is now (`rebase_level`, `rebase_model`, `rebase_block`: the
//!   step's own changes, things and parts by name), else not written;
//! - a file gone is written anew by the next step;
//! - a file whose last write failed (the editor holds what it lacks) that
//!   changes asks whose to keep (`Disk::clash`).
//!
//! Never an older state over a newer file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::blocks::{self, Block};
use crate::grid::{Area, GridLevel, Place, Thing};

use crate::editor::frames::{Colour, ModelFile, Placed, Shaped};
use super::room::Item;
use super::{Editor, LevelState, Scope, Step};

/// A file's bytes as a hash; None when it is not there (or cannot be
/// read).
pub fn hash_of(path: &Path) -> Option<blake3::Hash> {
    std::fs::read(path).ok().map(|b| blake3::hash(&b))
}

/// What the editor knows of its files on disk.
#[derive(Default)]
pub struct Disk {
    /// Each file as last read or written (None: it was not there).
    seen: BTreeMap<PathBuf, Option<blake3::Hash>>,
    /// Files whose last write failed: the editor holds what they lack.
    unwritten: BTreeSet<PathBuf>,
    /// A file that changed on disk while the editor held changes it could
    /// not write: KEEP MINE or TAKE THE FILE'S answers.
    pub clash: Option<PathBuf>,
    /// What came from the disk, in a few words, the rest after a colon
    /// (the line over the view, the whole on hover).
    pub notice: Option<String>,
}

impl Disk {
    fn remember(&mut self, path: &Path, bytes: &[u8]) {
        self.seen.insert(path.to_path_buf(), Some(blake3::hash(bytes)));
        self.unwritten.remove(path);
    }
}

/// Which of the editor's files a path is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Held {
    Level,
    Block(usize),
    Model,
    /// The level round a model shaped in place: read, never written.
    Context,
}

/// What became of a step at its write.
pub(super) enum Saved {
    /// Written as it is (or nothing to write).
    Kept,
    /// The file had changed: the step laid on the file as it is now and
    /// written, the steps before dropped; the step as it now stands.
    Rebased(Option<Box<Step>>),
    /// The file had changed and the step no longer fits it: nothing
    /// written, the file read anew, the steps dropped.
    Dropped,
}

/// The upper-case word for a file in the line over the view.
fn word(held: Held) -> &'static str {
    match held {
        Held::Level => "LEVEL",
        Held::Block(_) => "BLOCK",
        Held::Model => "MODEL",
        Held::Context => "CONTEXT",
    }
}

impl Editor {
    /// The files the editor holds and writes.
    fn held(&self) -> Vec<(PathBuf, Held)> {
        if let Some(m) = &self.model {
            let room = self.context.iter().map(|c| (self.dir.join(&c.file), Held::Context));
            return if m.foreign { Vec::new() } else { std::iter::once((m.path.clone(), Held::Model)).chain(room).collect() };
        }
        let mut out = vec![(self.dir.join(&self.file), Held::Level)];
        out.extend(self.level.blocks.iter().enumerate().map(|(i, b)| (blocks::path(&self.dir, &b.name), Held::Block(i))));
        out
    }

    /// Remembers every file held as it is on disk now (opening).
    pub(crate) fn remember_held(&mut self) {
        for (path, _) in self.held() {
            let now = hash_of(&path);
            self.disk.seen.insert(path, now);
        }
    }

    /// Remembers `path` as it is on disk now (the editor wrote or took it
    /// away itself).
    pub(super) fn remember_path(&mut self, path: &Path) {
        let now = hash_of(path);
        self.disk.seen.insert(path.to_path_buf(), now);
    }

    /// Looks whether a file the editor holds changed on disk (once a
    /// second, when the window gets the focus): one that did is read anew,
    /// a drag going on let go of; one gone is said. Whether anything came.
    pub fn look_at_disk(&mut self) -> bool {
        if self.dry {
            return false;
        }
        let mut came = false;
        for (path, held) in self.held() {
            let now = hash_of(&path);
            match self.disk.seen.get(&path) {
                None => {
                    self.disk.seen.insert(path, now);
                    continue;
                }
                Some(was) if *was == now => continue,
                Some(_) => {}
            }
            came = true;
            let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
            if held == Held::Context {
                // The room round the model: drawn anew, nothing of the
                // model's touched.
                self.disk.seen.insert(path.clone(), now);
                match GridLevel::read(&path) {
                    Ok(level) => {
                        if let Some(c) = &mut self.context
                            && !c.reread(level)
                        {
                            self.context = None;
                            self.disk.notice = Some(format!("CONTEXT GONE: the piece is no longer in {name}"));
                        }
                    }
                    Err(e) => self.disk.notice = Some(format!("CONTEXT DOES NOT READ: {e:#}")),
                }
                continue;
            }
            if now.is_none() {
                self.disk.seen.insert(path, None);
                self.disk.notice = Some(format!("{} GONE FROM DISK: {name} was taken away; the next step writes it anew", word(held)));
                continue;
            }
            if self.disk.unwritten.contains(&path) {
                self.disk.clash = Some(path);
                continue;
            }
            self.drop_drags();
            match self.read_anew(&path, held) {
                Ok(()) => self.disk.notice = Some(format!("{} READ ANEW FROM DISK: another program changed {name}; the steps before cannot be taken back", word(held))),
                Err(e) => self.disk.notice = Some(format!("{} ON DISK DOES NOT READ: {e:#}; nothing is written to it until it does", word(held))),
            }
        }
        came
    }

    /// KEEP MINE: the editor's state written over the file that changed.
    pub fn keep_mine(&mut self) {
        let Some(path) = self.disk.clash.take() else { return };
        self.remember_path(&path);
        let held = self.held().into_iter().find(|(p, _)| *p == path).map(|(_, h)| h);
        let step = match held {
            Some(Held::Level) => Step::Level(self.level_state(), self.level_state()),
            Some(Held::Block(i)) => Step::Block(i, self.blocks[i].clone(), self.blocks[i].clone()),
            Some(Held::Model) => Step::Model(self.shaped(), self.shaped()),
            Some(Held::Context) | None => return,
        };
        self.save(&step);
    }

    /// TAKE THE FILE'S: the file read anew, what the editor held of it gone.
    pub fn take_file(&mut self) {
        let Some(path) = self.disk.clash.take() else { return };
        self.disk.unwritten.remove(&path);
        let Some(held) = self.held().into_iter().find(|(p, _)| *p == path).map(|(_, h)| h) else { return };
        self.drop_drags();
        match self.read_anew(&path, held) {
            Ok(()) => self.disk.notice = None,
            Err(e) => self.message = Some(format!("{e:#}")),
        }
    }

    /// Every drag and stroke let go of as before it began, nothing kept:
    /// the file under it changed.
    fn drop_drags(&mut self) {
        if let Some(stroke) = self.stroke.take() {
            match stroke {
                Step::Level(before, _) => self.set_level(&before),
                Step::Block(i, before, _) => {
                    if let Some(b) = self.blocks.get_mut(i) {
                        *b = before;
                    }
                }
                Step::Model(before, _) => {
                    if let Some(m) = &mut self.model {
                        m.reshape(&before);
                    }
                }
                Step::Delete(_) => {}
            }
        }
        self.moving = None;
        self.dragging = None;
        self.gripped = None;
        self.joint_drag = None;
        self.join_next = false;
        self.drawing = None;
        self.fresh = None;
        self.tuned = None;
        self.tuning = None;
    }

    /// What is chosen among the pieces, to find again in a level read
    /// anew: its name, else the thing as it was.
    fn chosen_mark(&self) -> Option<(Item, Option<String>, Option<Thing>)> {
        let item = self.chosen?;
        let thing = match item {
            Item::Thing(i) => self.level.things.get(i).cloned(),
            _ => None,
        };
        Some((item, self.name_of(item).map(str::to_string), thing))
    }

    fn choose_again(&mut self, mark: Option<(Item, Option<String>, Option<Thing>)>) {
        self.chosen = mark.and_then(|(item, name, thing)| match (name, thing) {
            (Some(n), _) => self.find(&n).filter(|f| std::mem::discriminant(f) == std::mem::discriminant(&item)),
            (None, Some(t)) => self.level.things.iter().position(|o| *o == t).map(Item::Thing),
            _ => None,
        });
        if self.chosen.is_none() {
            self.chosen_light = None;
        }
        if let Some(Item::Thing(i)) = self.chosen
            && self.chosen_light.is_some_and(|k| k >= self.level.things[i].lights.len())
        {
            self.chosen_light = None;
        }
    }

    /// The part the voxel editor works on, by name.
    fn chosen_part(&self) -> Option<String> {
        let (Scope::Frame(f), Some(m)) = (self.scope, &self.model) else { return None };
        m.parts.iter().find(|p| p.frame == f).map(|p| p.name.clone())
    }

    fn choose_part_again(&mut self, name: Option<String>) {
        let Some(m) = &self.model else { return };
        if let Scope::Frame(f) = self.scope {
            let frame = match name {
                Some(n) => m.parts.iter().find(|p| p.name == n).map(|p| p.frame),
                None if m.parts.is_empty() => Some(f.min(m.frames.len().saturating_sub(1))),
                None => None,
            };
            self.scope = frame.map_or(if m.parts.is_empty() { Scope::Frame(0) } else { Scope::Figure }, Scope::Frame);
        }
        self.follow_figure();
    }

    /// The file `path` read anew into the editor, its steps dropped.
    fn read_anew(&mut self, path: &Path, held: Held) -> anyhow::Result<()> {
        let bytes = std::fs::read(path)?;
        match held {
            Held::Level => {
                let text = String::from_utf8(bytes.clone())?;
                let level = GridLevel::from_text(&text)?;
                self.adopt_level(text, level);
            }
            Held::Block(i) => {
                let block = Block::load(path, self.level.n)?;
                if let Some(b) = self.blocks.get_mut(i) {
                    *b = block;
                }
            }
            Held::Model => {
                let fresh = ModelFile::load(path)?;
                let part = self.chosen_part();
                self.model = Some(fresh);
                self.choose_part_again(part);
            }
            // (Read in `look_at_disk`, never a step's.)
            Held::Context => return Ok(()),
        }
        self.disk.remember(path, &bytes);
        self.past.clear();
        self.future.clear();
        Ok(())
    }

    /// The level as `text` (read from disk) says it: what is chosen kept by
    /// name, its blocks read if the file lists others.
    fn adopt_level(&mut self, text: String, level: GridLevel) {
        let mark = self.chosen_mark();
        let names = |l: &GridLevel| l.blocks.iter().map(|b| b.name.clone()).collect::<Vec<_>>();
        let other_blocks = names(&level) != names(&self.level);
        self.source = text;
        self.source_level = level.clone();
        self.level = level;
        if other_blocks {
            match self.level.load_blocks(&self.dir) {
                Ok(b) => self.blocks = b,
                Err(e) => self.message = Some(format!("{e:#}")),
            }
            self.remember_held();
        }
        if self.held.block as usize > self.level.blocks.len() {
            self.held.block = (!self.level.blocks.is_empty()) as u16;
        }
        if let Scope::Block(i) = self.scope
            && i >= self.level.blocks.len()
        {
            self.scope = Scope::Level;
        }
        self.choose_again(mark);
    }

    /// Writes what `step` changed, after looking at the file: as it is
    /// when the file is as last seen (or gone: written anew); laid on the
    /// file as it is now when another program changed it; held when the
    /// file changed while a write of it had failed (`Disk::clash`).
    pub(super) fn save(&mut self, step: &Step) -> Saved {
        if self.dry {
            return Saved::Kept;
        }
        let (path, held) = match step {
            Step::Level(..) | Step::Delete(..) => (self.dir.join(&self.file), Held::Level),
            Step::Block(i, ..) => (blocks::path(&self.dir, &self.level.blocks[*i].name), Held::Block(*i)),
            Step::Model(..) => match &self.model {
                Some(m) => (m.path.clone(), Held::Model),
                None => return Saved::Kept,
            },
        };
        if self.disk.clash.as_ref() == Some(&path) {
            self.disk.unwritten.insert(path);
            return Saved::Kept;
        }
        let now = hash_of(&path);
        let seen = self.disk.seen.get(&path).copied();
        let changed = seen.is_some_and(|s| s != now);
        if changed && now.is_some() {
            if self.disk.unwritten.contains(&path) {
                self.disk.clash = Some(path);
                return Saved::Kept;
            }
            return self.save_onto_fresh(step, &path, held);
        }
        let gone = seen.is_some() && now.is_none();
        let written = match step {
            Step::Level(..) | Step::Delete(..) => self.save_level(),
            Step::Block(i, ..) => self.blocks[*i].save_saying(&path),
            Step::Model(..) => self.model.as_mut().map_or(Ok(String::new()), ModelFile::save),
        };
        match written {
            Ok(text) => {
                self.message = None;
                self.disk.remember(&path, text.as_bytes());
                self.disk.notice = gone.then(|| format!("{} WAS GONE: WRITTEN ANEW", word(held)));
            }
            Err(e) => {
                self.message = Some(format!("{e:#}"));
                self.disk.unwritten.insert(path);
            }
        }
        Saved::Kept
    }

    /// `step` laid on the file `path` as another program left it, written;
    /// or, where it no longer fits, nothing written and the file read anew.
    fn save_onto_fresh(&mut self, step: &Step, path: &Path, held: Held) -> Saved {
        self.drop_drags();
        let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let laid = match step {
            Step::Level(b, a) => self.lay_level(b, a, path, true),
            Step::Delete(d) => self.lay_level(&d.before, &d.after, path, false),
            Step::Block(i, b, a) => self.lay_block(*i, b, a, path),
            Step::Model(b, a) => self.lay_model(b, a, path),
        };
        self.past.clear();
        self.future.clear();
        match laid {
            Ok(kept) => {
                let kept = kept.map(Box::new);
                self.disk.notice = Some(format!("{} CHANGED ON DISK: {name} was changed by another program; this step was laid on it, the steps before cannot be taken back", word(held)));
                Saved::Rebased(kept)
            }
            Err(why) => {
                if let Err(e) = self.read_anew(path, held) {
                    self.message = Some(format!("{e:#}"));
                }
                self.disk.notice = Some(format!("STEP NOT WRITTEN: {why} ({name} was changed by another program and read anew)"));
                Saved::Dropped
            }
        }
    }

    /// A level step laid on the level file as it is now: written, the
    /// editor holding the result (a step back to the file's state if
    /// `keep`).
    fn lay_level(&mut self, b: &LevelState, a: &LevelState, path: &Path, keep: bool) -> Result<Option<Step>, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("reading {name}: {e}", name = path.display()))?;
        let fresh = GridLevel::from_text(&text).map_err(|e| format!("the file does not read: {e:#}"))?;
        let d = state_of(&fresh);
        let now = rebase_level(b, a, &d)?;
        let mark = self.chosen_mark();
        let names = |s: &LevelState| s.table.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        let other_blocks = names(&now) != names(a);
        self.source = text;
        self.source_level = fresh;
        self.level = self.source_level.clone();
        self.set_level(&now);
        if other_blocks {
            self.blocks = self.level.load_blocks(&self.dir).map_err(|e| format!("{e:#}"))?;
            self.remember_held();
        }
        self.choose_again(mark);
        let written = self.save_level().map_err(|e| format!("{e:#}"))?;
        self.disk.remember(path, written.as_bytes());
        Ok(keep.then_some(Step::Level(d, now)))
    }

    fn lay_block(&mut self, i: usize, b: &Block, a: &Block, path: &Path) -> Result<Option<Step>, String> {
        let d = Block::load(path, self.level.n).map_err(|e| format!("the file does not read: {e:#}"))?;
        let now = rebase_block(b, a, &d)?;
        self.blocks[i] = now.clone();
        let written = now.save_saying(path).map_err(|e| format!("{e:#}"))?;
        self.disk.remember(path, written.as_bytes());
        Ok(Some(Step::Block(i, d, now)))
    }

    fn lay_model(&mut self, b: &Shaped, a: &Shaped, path: &Path) -> Result<Option<Step>, String> {
        let mut fresh = ModelFile::load(path).map_err(|e| format!("the file does not read: {e:#}"))?;
        let d = fresh.shaped();
        let now = rebase_model(b, a, &d)?;
        let part = self.chosen_part();
        fresh.reshape(&now);
        fresh.list_new_colours();
        let written = fresh.save().map_err(|e| format!("{e:#}"))?;
        self.model = Some(fresh);
        self.choose_part_again(part);
        self.disk.remember(path, written.as_bytes());
        Ok(Some(Step::Model(d, self.shaped())))
    }
}

/// A level's state as a step keeps it.
fn state_of(l: &GridLevel) -> LevelState {
    LevelState { size: l.size, cells: l.cells.clone(), table: l.blocks.clone(), things: l.things.clone(), places: l.places.clone(), areas: l.areas.clone(), daylight: l.daylight.clone().map(Box::new) }
}

// ------------------------------------------------------------- laying on

/// What a step did to a list, item by item: the same, changed (its place
/// before and after), taken out, or new.
#[derive(Debug, PartialEq)]
enum Pair {
    Changed(usize, usize),
    Gone(usize),
    New(usize),
}

/// The changes between `before` and `after`: the longest run of items
/// alike in both stays; between two of those, the items before and after
/// are paired in order, the rest taken out or new.
fn pairs<T: PartialEq>(before: &[T], after: &[T]) -> Vec<Pair> {
    // Alike at both ends: nothing to pair (a step changes one or a few).
    let head = before.iter().zip(after).take_while(|(x, y)| x == y).count();
    let tail = before[head..].iter().rev().zip(after[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (b, a) = (&before[head..before.len() - tail], &after[head..after.len() - tail]);
    // The longest common run of what is left.
    let (n, m) = (b.len(), a.len());
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * (m + 1) + j] = if b[i] == a[j] { dp[(i + 1) * (m + 1) + j + 1] + 1 } else { dp[(i + 1) * (m + 1) + j].max(dp[i * (m + 1) + j + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut gb, mut ga): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let flush = |gb: &mut Vec<usize>, ga: &mut Vec<usize>, out: &mut Vec<Pair>| {
        let k = gb.len().min(ga.len());
        out.extend((0..k).map(|t| Pair::Changed(head + gb[t], head + ga[t])));
        out.extend(gb[k..].iter().map(|&x| Pair::Gone(head + x)));
        out.extend(ga[k..].iter().map(|&y| Pair::New(head + y)));
        gb.clear();
        ga.clear();
    };
    while i < n || j < m {
        if i < n && j < m && b[i] == a[j] {
            flush(&mut gb, &mut ga, &mut out);
            (i, j) = (i + 1, j + 1);
        } else if j >= m || (i < n && dp[(i + 1) * (m + 1) + j] >= dp[i * (m + 1) + j + 1]) {
            gb.push(i);
            i += 1;
        } else {
            ga.push(j);
            j += 1;
        }
    }
    flush(&mut gb, &mut ga, &mut out);
    out
}

/// What a list of things, places or areas needs to be laid on.
trait Listed: Clone + PartialEq {
    const WHAT: &'static str;
    fn name(&self) -> Option<&str>;
    fn label(&self) -> String;
    /// `d` with the fields that `b` and `a` differ in taken from `a`.
    fn merged(b: &Self, a: &Self, d: &Self) -> Self;
    /// New in the file: no line of the file's yet.
    fn unlined(self) -> Self;
}

macro_rules! take {
    ($b:ident, $a:ident, $d:ident; $($f:ident),*) => {{
        let mut o = $d.clone();
        $( if $b.$f != $a.$f { o.$f = $a.$f.clone(); } )*
        o
    }};
}

impl Listed for Thing {
    const WHAT: &'static str = "a thing";
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.model.clone().unwrap_or_else(|| self.kind.clone()))
    }
    fn merged(b: &Self, a: &Self, d: &Self) -> Self {
        take!(b, a, d; kind, at, name, model, facing, pitch, roll, lights, solid)
    }
    fn unlined(mut self) -> Self {
        self.line = 0;
        self
    }
}

impl Listed for Place {
    const WHAT: &'static str = "a place";
    fn name(&self) -> Option<&str> {
        Some(&self.name)
    }
    fn label(&self) -> String {
        self.name.clone()
    }
    fn merged(b: &Self, a: &Self, d: &Self) -> Self {
        take!(b, a, d; name, at, facing, named)
    }
    fn unlined(mut self) -> Self {
        self.line = 0;
        self
    }
}

impl Listed for Area {
    const WHAT: &'static str = "an area";
    fn name(&self) -> Option<&str> {
        Some(&self.name)
    }
    fn label(&self) -> String {
        self.name.clone()
    }
    fn merged(b: &Self, a: &Self, d: &Self) -> Self {
        take!(b, a, d; name, at, size)
    }
    fn unlined(mut self) -> Self {
        self.line = 0;
        self
    }
}

/// The step `b` to `a` of a list laid on `d` (the file's list now): each
/// item it changed or took out found in `d` by its name, else as it was;
/// new ones after the rest. A changed or taken one no longer there, or a
/// new one whose name `d` has already, does not fit.
fn rebase_list<T: Listed>(b: &[T], a: &[T], d: &[T]) -> Result<Vec<T>, String> {
    let mut out: Vec<Option<T>> = d.iter().cloned().map(Some).collect();
    let mut used = vec![false; d.len()];
    let find = |x: &T, used: &[bool]| -> Option<usize> {
        match x.name() {
            Some(n) => d.iter().position(|y| y.name() == Some(n)),
            None => (0..d.len()).find(|&k| !used[k] && d[k] == *x),
        }
    };
    let mut added = Vec::new();
    for p in pairs(b, a) {
        match p {
            Pair::Changed(i, j) => {
                let k = find(&b[i], &used).ok_or_else(|| format!("{} '{}' is no longer in the file as it was", T::WHAT, b[i].label()))?;
                used[k] = true;
                out[k] = Some(T::merged(&b[i], &a[j], &d[k]));
            }
            Pair::Gone(i) => {
                let k = find(&b[i], &used).ok_or_else(|| format!("{} '{}' is no longer in the file as it was", T::WHAT, b[i].label()))?;
                used[k] = true;
                out[k] = None;
            }
            Pair::New(j) => {
                if let Some(n) = a[j].name()
                    && d.iter().any(|y| y.name() == Some(n))
                {
                    return Err(format!("the file has {} called '{n}' now", T::WHAT));
                }
                added.push(a[j].clone().unlined());
            }
        }
    }
    Ok(out.into_iter().flatten().chain(added).collect())
}

/// The step `b` to `a` of a level laid on `d`, the level's file as it is
/// now: the cells it changed, its blocks' list, its daylight, its things,
/// places and areas by name; what else the file says stays as it says.
fn rebase_level(b: &LevelState, a: &LevelState, d: &LevelState) -> Result<LevelState, String> {
    let mut o = d.clone();
    if b.table != a.table {
        if d.table != b.table && d.table != a.table {
            return Err("the level's blocks changed in the file too".into());
        }
        o.table = a.table.clone();
    }
    if b.size != a.size {
        if d.size != b.size || d.cells != b.cells {
            return Err("the level's grid changed in the file too".into());
        }
        (o.size, o.cells) = (a.size, a.cells.clone());
    } else if b.cells != a.cells {
        if d.size != b.size || d.cells.len() != b.cells.len() {
            return Err("the level's size changed in the file".into());
        }
        for k in 0..b.cells.len() {
            if b.cells[k] != a.cells[k] {
                o.cells[k] = a.cells[k];
            }
        }
    }
    if b.daylight != a.daylight {
        o.daylight = a.daylight.clone();
    }
    o.things = rebase_list(&b.things, &a.things, &d.things)?;
    o.places = rebase_list(&b.places, &a.places, &d.places)?;
    o.areas = rebase_list(&b.areas, &a.areas, &d.areas)?;
    Ok(o)
}

/// A block's step laid on its file as it is now: the voxels it changed.
fn rebase_block(b: &Block, a: &Block, d: &Block) -> Result<Block, String> {
    if b.size != a.size || d.size != b.size {
        return if *d == *b { Ok(a.clone()) } else { Err("the block changed in its file too".into()) };
    }
    let (bc, ac) = (b.cells(), a.cells());
    let mut o = d.clone();
    for k in 0..bc.len() {
        if bc[k] != ac[k] {
            o.set(b.at(k), ac[k]).map_err(|e| format!("{e:#}"))?;
        }
    }
    Ok(o)
}

/// A frame's voxels as materials, the same whichever keys the palette
/// gives them.
fn looks(s: &Shaped, f: &Block) -> (glam::IVec3, Vec<Option<crate::vvx::Mat>>) {
    (f.size, f.cells().into_iter().map(|c| c.map(|k| material(s, k))).collect())
}

fn material(s: &Shaped, key: [u8; 3]) -> crate::vvx::Mat {
    s.palette.iter().find(|c| c.key == key).map_or_else(|| crate::editor::frames::plain(key), |c| c.mat)
}

/// The key of `mat` in `palette`, a new entry if it has none.
fn key_for(palette: &mut Vec<Colour>, mat: crate::vvx::Mat) -> [u8; 3] {
    if let Some(c) = palette.iter().find(|c| c.mat == mat) {
        return c.key;
    }
    let key = crate::editor::frames::free_key(palette, [mat.rgba[0], mat.rgba[1], mat.rgba[2]]);
    palette.push(Colour { key, sign: None, mat });
    key
}

/// `f` of `s` with the keys of `palette` (entries added for its new
/// materials).
fn carried(s: &Shaped, f: &Block, palette: &mut Vec<Colour>) -> Block {
    let mut o = Block::boxed(f.size);
    for (k, c) in f.cells().into_iter().enumerate() {
        if let Some(key) = c {
            let _ = o.set(f.at(k), Some(key_for(palette, material(s, key))));
        }
    }
    o
}

/// Frame `fd` of `out` with the voxels that frame `fb` of `b` and `fa`
/// of `a` differ in taken from `fa`; or `fa` whole if `fd` is still
/// `fb` where their boxes differ.
fn rebase_frame(b: &Shaped, fb: &Block, a: &Shaped, fa: &Block, out: &mut Shaped, fd: usize, what: &str) -> Result<(), String> {
    let (lb, la) = (looks(b, fb), looks(a, fa));
    if lb == la {
        return Ok(());
    }
    let ld = looks(out, &out.frames[fd]);
    if lb.0 != la.0 || ld.0 != lb.0 {
        if ld != lb {
            return Err(format!("{what} changed in the file too"));
        }
        out.frames[fd] = carried(a, fa, &mut out.palette);
        return Ok(());
    }
    for k in 0..lb.1.len() {
        if lb.1[k] != la.1[k] {
            let key = la.1[k].map(|m| key_for(&mut out.palette, m));
            let at = fb.at(k);
            out.frames[fd].set(at, key).map_err(|e| format!("{e:#}"))?;
        }
    }
    Ok(())
}

/// A model's step laid on its file as it is now (`d`, its own keys): the
/// colours whose material it set or that it took out, its frames by their
/// place (the voxels it changed), a figure's parts by name (their voxels,
/// place, parent and joint, new ones, ones taken out), its box.
fn rebase_model(b: &Shaped, a: &Shaped, d: &Shaped) -> Result<Shaped, String> {
    let mut out = d.clone();
    // Materials set, colours taken out of the palette.
    for c in &b.palette {
        match a.palette.iter().find(|x| x.key == c.key) {
            Some(x) if x.mat != c.mat => {
                if let Some(e) = out.palette.iter_mut().find(|e| e.mat == c.mat) {
                    e.mat = x.mat;
                }
            }
            None => {
                let used: BTreeSet<[u8; 3]> = out.frames.iter().flat_map(Block::colours).collect();
                out.palette.retain(|e| e.mat != c.mat || used.contains(&e.key));
            }
            _ => {}
        }
    }
    if b.size != a.size {
        out.size = if d.size == b.size { a.size } else { out.size.max(a.size) };
    }
    if b.parts.is_empty() && a.parts.is_empty() {
        if !d.parts.is_empty() {
            return Err("the model is a figure of parts in the file now".into());
        }
        if b.frames.len() != a.frames.len() {
            // Frames copied or taken out: only over the frames as they were.
            let same = d.frames.len() == b.frames.len() && b.frames.iter().zip(&d.frames).all(|(x, y)| looks(b, x) == looks(d, y));
            if !same {
                return Err("the model's frames changed in the file too".into());
            }
            out.frames = a.frames.iter().map(|f| carried(a, f, &mut out.palette)).collect();
            return Ok(out);
        }
        if d.frames.len() != b.frames.len() {
            return Err("frames came or went in the file".into());
        }
        for k in 0..b.frames.len() {
            rebase_frame(b, &b.frames[k], a, &a.frames[k], &mut out, k, &format!("frame {}", k + 1))?;
        }
        return Ok(out);
    }
    if d.parts.is_empty() {
        return Err("the model is no figure of parts in the file now".into());
    }
    let named = |s: &[Placed], n: &str| s.iter().position(|p| p.name == n);
    // Parts changed and taken out.
    for pb in &b.parts {
        let Some(k) = named(&out.parts, &pb.name) else { return Err(format!("part '{}' is no longer in the file", pb.name)) };
        match named(&a.parts, &pb.name) {
            Some(j) => {
                let pa = &a.parts[j];
                let fd = out.parts[k].frame;
                rebase_frame(b, &b.frames[pb.frame], a, &a.frames[pa.frame], &mut out, fd, &format!("part '{}'", pb.name))?;
                let od = &mut out.parts[k];
                if pb.low != pa.low {
                    od.low = pa.low;
                }
                if pb.parent != pa.parent {
                    od.parent = pa.parent.clone();
                }
                if pb.joint != pa.joint {
                    od.joint = pa.joint;
                }
            }
            None => {
                let gone = out.parts.remove(k);
                out.frames.remove(gone.frame);
                for p in &mut out.parts {
                    if p.frame > gone.frame {
                        p.frame -= 1;
                    }
                }
            }
        }
    }
    // New parts.
    for pa in a.parts.iter().filter(|p| named(&b.parts, &p.name).is_none()) {
        if named(&out.parts, &pa.name).is_some() {
            return Err(format!("the file has a part called '{}' now", pa.name));
        }
        let frame = carried(a, &a.frames[pa.frame], &mut out.palette);
        out.frames.push(frame);
        out.parts.push(Placed { frame: out.frames.len() - 1, ..pa.clone() });
    }
    // A parent taken out: what hung on it hangs as the rules say.
    let names: Vec<String> = out.parts.iter().map(|p| p.name.clone()).collect();
    for p in &mut out.parts {
        if p.parent.as_ref().is_some_and(|q| !names.contains(q)) {
            p.parent = None;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::tests::PIECES;
    use glam::IVec3;

    #[test]
    fn a_steps_changes_are_paired_with_what_they_were() {
        let b = ["a", "b", "c", "d"];
        assert_eq!(pairs(&b, &["a", "x", "c", "d"]), vec![Pair::Changed(1, 1)]);
        assert_eq!(pairs(&b, &["a", "c", "d"]), vec![Pair::Gone(1)]);
        assert_eq!(pairs(&b, &["a", "b", "c", "d", "e"]), vec![Pair::New(4)]);
        assert_eq!(pairs(&b, &b), vec![]);
        assert_eq!(pairs(&b, &["b", "c", "y"]), vec![Pair::Gone(0), Pair::Changed(3, 2)]);
    }

    /// The sketch's room open in the level editor; its folder.
    fn room(name: &str) -> (Editor, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("disk-{name}"));
        std::fs::write(dir.join("room.toml"), PIECES).unwrap();
        (Editor::open(&dir, "room.toml").unwrap(), dir)
    }

    fn saved(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("room.toml")).unwrap()
    }

    /// Another program writes the room.
    fn outside(dir: &Path, text: &str) {
        std::fs::write(dir.join("room.toml"), text).unwrap();
    }

    fn thing<'a>(l: &'a GridLevel, name: &str) -> &'a Thing {
        l.things.iter().find(|t| t.name.as_deref() == Some(name)).unwrap_or_else(|| panic!("no {name}"))
    }

    /// A thing's lines as a file has them.
    fn thing_text(name: &str, x: i32) -> String {
        format!("\n[[thing]]\nname = \"{name}\"\nmodel = \"room/rug.vvx\"\nat = [{x}, 60, 0]\n")
    }

    /// The level file changed under the open editor comes in at its next
    /// look: what is chosen kept by name, the steps before dropped, a line
    /// saying so.
    #[test]
    fn a_level_changed_on_disk_comes_into_the_open_editor() {
        let (mut ed, dir) = room("comes-in");
        ed.choose_item(ed.find("lamp"));
        ed.turn_item(1);
        assert_eq!(ed.steps(), 1);
        assert!(!ed.look_at_disk(), "its own write is no change");
        // A rug put in before the stove, the lamp moved.
        let text = saved(&dir).replacen("[[thing]]\nname = \"stove\"", &format!("{}\n[[thing]]\nname = \"stove\"", thing_text("rug", 5).trim_start()), 1).replace("at = [212, 40, 0]", "at = [220, 44, 0]");
        outside(&dir, &text);
        assert!(ed.look_at_disk());
        assert_eq!(ed.level.things[0].name.as_deref(), Some("rug"));
        assert_eq!(thing(&ed.level, "lamp").at, [220, 44, 0]);
        assert_eq!(ed.chosen, ed.find("lamp"), "still the lamp, a place further down");
        assert_eq!(ed.steps(), 0, "a step back would bring the older file back");
        assert!(ed.disk.notice.as_deref().is_some_and(|n| n.starts_with("LEVEL READ ANEW")), "{:?}", ed.disk.notice);
        assert!(!ed.look_at_disk(), "once");
        // The next step writes over the new text.
        ed.turn_item(1);
        assert!(saved(&dir).contains("name = \"rug\"") && saved(&dir).contains("at = [220, 44, 0]"));
    }

    /// The owner's case: things added outside, one taken out, three moved;
    /// then TURN on a piece in the editor, which had not looked yet: both
    /// in the file, the outside text kept as it was written.
    #[test]
    fn a_step_after_an_outside_change_keeps_the_outside_change() {
        let (mut ed, dir) = room("keeps");
        ed.choose_item(ed.find("stove"));
        let before = thing(&ed.level, "stove").facing;
        let mut text = saved(&dir).replace("[[thing]]\nkind = \"walker\"\nat = [100, 20, 0]\n\n", "").replace("at = [212, 40, 0]", "at = [1, 2, 0]").replace("at = [300, 190, 90]", "at = [3, 4, 5]").replace("at = [60, 150, 0]", "at = [6, 7, 0]");
        text += "\n# put here by the agent\n";
        for k in 0..7 {
            text += &thing_text(&format!("rug{k}"), 10 * k);
        }
        outside(&dir, &text);
        ed.turn_item(1);
        let now = saved(&dir);
        let level = GridLevel::from_text(&now).unwrap();
        assert_eq!(level.things.len(), 4 + 7, "{now}");
        assert!(level.things.iter().all(|t| t.kind != "walker"));
        assert_eq!([thing(&level, "lamp").at, thing(&level, "moon").at, thing(&level, "chair").at], [[1, 2, 0], [3, 4, 5], [6, 7, 0]]);
        assert_ne!(thing(&level, "stove").facing, before, "the turn");
        assert_eq!(thing(&level, "stove").facing, thing(&ed.level, "stove").facing);
        assert!(now.contains("# put here by the agent"), "the agent's own text stays");
        assert_eq!(now.lines().filter(|l| !l.starts_with("facing")).collect::<Vec<_>>(), text.lines().filter(|l| !l.starts_with("facing")).collect::<Vec<_>>(), "only the turn's line differs");
        assert_eq!(ed.steps(), 1, "the turn, as laid on the file");
        ed.undo();
        assert_eq!(saved(&dir), text, "taken back: the agent's file");
    }

    /// A step whose thing is gone from the file writes nothing: the file
    /// stays as the other program left it, read anew, the line says so.
    #[test]
    fn a_step_whose_thing_is_gone_writes_nothing() {
        let (mut ed, dir) = room("gone-thing");
        ed.choose_item(ed.find("chair"));
        let text = saved(&dir).replace("[[thing]]\nname = \"chair\"\nmodel = \"room/chair.vvx\"\nat = [60, 150, 0]\nrotation = 45\n\n", "");
        assert_ne!(text, saved(&dir));
        outside(&dir, &text);
        ed.turn_item(1);
        assert_eq!(saved(&dir), text);
        assert!(ed.find("chair").is_none() && ed.chosen.is_none());
        assert!(ed.disk.notice.as_deref().is_some_and(|n| n.starts_with("STEP NOT WRITTEN")), "{:?}", ed.disk.notice);
        assert_eq!(ed.steps(), 0);
    }

    /// A file taken away under the editor is said; the next step writes
    /// it anew, and says that.
    #[test]
    fn a_file_gone_is_written_anew_by_the_next_step() {
        let (mut ed, dir) = room("gone-file");
        std::fs::remove_file(dir.join("room.toml")).unwrap();
        assert!(ed.look_at_disk());
        assert!(ed.disk.notice.as_deref().is_some_and(|n| n.starts_with("LEVEL GONE")));
        assert!(!dir.join("room.toml").exists(), "nothing written by looking");
        ed.choose_item(ed.find("stove"));
        ed.turn_item(1);
        assert_eq!(GridLevel::from_text(&saved(&dir)).unwrap(), ed.level);
        assert!(ed.disk.notice.as_deref().is_some_and(|n| n.starts_with("LEVEL WAS GONE")));
    }

    /// A drag going on when the file changes is let go of: nothing of it
    /// stays, nothing is written by it.
    #[test]
    fn a_drag_is_let_go_of_when_the_file_changes() {
        let (mut ed, dir) = room("drag");
        let lamp = ed.find("lamp");
        ed.press_item(lamp, glam::Vec3::new(212.5, 40.5, 500.0), glam::Vec3::NEG_Z);
        ed.drag_item(glam::Vec3::new(250.5, 80.5, 500.0), glam::Vec3::NEG_Z);
        assert!(ed.dragging().is_some() && thing(&ed.level, "lamp").at != [212, 40, 0]);
        let text = saved(&dir).replace("at = [300, 190, 90]", "at = [3, 4, 5]");
        outside(&dir, &text);
        assert!(ed.look_at_disk());
        assert!(ed.dragging().is_none());
        assert_eq!(thing(&ed.level, "lamp").at, [212, 40, 0]);
        ed.release();
        assert_eq!(saved(&dir), text);
    }

    /// A file that changes while the editor holds a change it could not
    /// write asks whose to keep: KEEP MINE writes the editor's, TAKE THE
    /// FILE'S reads the file.
    #[test]
    fn a_change_not_written_asks_whose_to_keep() {
        let (mut ed, dir) = room("clash");
        let path = dir.join("room.toml");
        ed.choose_item(ed.find("stove"));
        ed.turn_item(1);
        let mine = ed.level.clone();
        ed.disk.unwritten.insert(path.clone());
        outside(&dir, &PIECES.replace("at = [300, 190, 90]", "at = [3, 4, 5]"));
        assert!(ed.look_at_disk());
        assert_eq!((ed.disk.clash.as_ref(), &ed.level), (Some(&path), &mine), "nothing taken while asking");
        ed.keep_mine();
        assert_eq!((ed.disk.clash.as_ref(), GridLevel::from_text(&saved(&dir)).unwrap()), (None, mine.clone()));
        ed.disk.unwritten.insert(path.clone());
        let theirs = PIECES.replace("at = [300, 190, 90]", "at = [3, 4, 5]");
        outside(&dir, &theirs);
        ed.look_at_disk();
        ed.take_file();
        assert_eq!((ed.disk.clash.as_ref(), saved(&dir)), (None, theirs.clone()));
        assert_eq!(ed.level, GridLevel::from_text(&theirs).unwrap());
    }

    /// A figure with a hand-written repeat (`*2`, which the writer writes
    /// out) and a comment.
    fn figure_text() -> String {
        format!(
            "format = \"{}\"\nsize = [6, 4, 6]\n\n[palette]\na = \"#c83232\"\nb = \"#3264c8\"\n\n# The body, two layers alike.\n[[part]]\nname = \"body\"\nat = [-1, -1, 0]\nlayers = \"\"\"\naa\naa\n*2\n\naa\naa\n\"\"\"\n\n[[part]]\nname = \"head\"\nat = [-1, -1, 3]\nlayers = \"\"\"\nbb\nbb\n\"\"\"\n\n[[part]]\nname = \"hand\"\nat = [1, -1, 1]\nlayers = \"\"\"\nb\nb\n\"\"\"\n",
            crate::vvx::FORMAT
        )
    }

    fn figure(name: &str) -> (Editor, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("disk-{name}"));
        std::fs::write(dir.join("figure.vvx"), figure_text()).unwrap();
        let mut ed = Editor::open_model(&dir, "figure.vvx").unwrap();
        ed.follow_figure();
        let hand = ed.figure.as_ref().unwrap().parts().iter().position(|p| p.name == "hand").unwrap();
        ed.choose_part(hand);
        (ed, dir)
    }

    fn model_text(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("figure.vvx")).unwrap()
    }

    /// A part moved: every other part keeps its text byte for byte, the
    /// hand-written repeat and the comment too; only the moved part's
    /// `at` changes.
    #[test]
    fn an_unchanged_part_keeps_its_text() {
        let (mut ed, dir) = figure("keeps-text");
        ed.move_part(IVec3::X);
        let was = figure_text();
        let now = model_text(&dir);
        assert_eq!(now, was.replace("at = [1, -1, 1]", "at = [2, -1, 1]"), "{now}");
        ed.undo();
        assert_eq!(model_text(&dir), was);
    }

    /// The model changed under the open editor: read anew at a look (the
    /// part chosen kept by name); a step after an outside change keeps
    /// it; a step on a part gone from the file writes nothing.
    #[test]
    fn a_model_changed_on_disk_comes_in_and_a_step_keeps_it() {
        let (mut ed, dir) = figure("model");
        // The head's back row blue to red, outside.
        let text = figure_text().replacen("bb\nbb\n", "aa\nbb\n", 1);
        std::fs::write(dir.join("figure.vvx"), &text).unwrap();
        assert!(ed.look_at_disk());
        assert_eq!(ed.part_at(), Some(IVec3::new(1, -1, 1)), "the hand still chosen");
        assert!(ed.disk.notice.as_deref().is_some_and(|n| n.starts_with("MODEL READ ANEW")));
        // Changed again outside, then a step before the editor looked.
        let text = text.replace("name = \"head\"\nat = [-1, -1, 3]", "name = \"head\"\nat = [-1, -1, 3]\njoint = [0, 0, 3]");
        std::fs::write(dir.join("figure.vvx"), &text).unwrap();
        ed.move_part(IVec3::X);
        assert_eq!(model_text(&dir), text.replace("at = [1, -1, 1]", "at = [2, -1, 1]"), "both");
        assert_eq!(ed.steps(), 1);
        // The hand taken out of the file, then moved in the editor.
        let now = model_text(&dir);
        let gone = now[..now.find("\n[[part]]\nname = \"hand\"").unwrap()].to_string();
        assert!(!gone.contains("hand"), "{gone}");
        std::fs::write(dir.join("figure.vvx"), &gone).unwrap();
        ed.move_part(IVec3::X);
        assert_eq!(model_text(&dir), gone, "nothing written");
        assert!(ed.disk.notice.as_deref().is_some_and(|n| n.starts_with("STEP NOT WRITTEN")), "{:?}", ed.disk.notice);
        assert!(ed.model.as_ref().unwrap().parts.iter().all(|p| p.name != "hand"));
    }
}

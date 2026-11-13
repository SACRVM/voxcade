//! A selection of voxels in the voxel editor (SELECT) and what is done
//! at it: MOVE by its arrows, DUPLICATE, REMOVE, PAINT, FLIP, ROTATE, in
//! a figure NEW PART and MOVE TO PART, COPY and PASTE; and FILL, the
//! bucket. A selection lies in a block, a model's frame, a figure's part,
//! or across the parts when the figure whole is chosen; each change one
//! step, written as every step is.

use std::collections::{BTreeMap, BTreeSet};

use glam::{IVec3, Vec3};

use super::{Editor, Hit, Scope, Step};
use crate::blocks::Block;
use crate::editor::frames::{ModelFile, Placed, Shaped};
use crate::editor::handles::{self, Handles};

/// What selected voxels lie in: a level's block, or a model's frame (a
/// figure's part has one of its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    Block(usize),
    Frame(usize),
}

/// Voxels by where they lie, with their color.
pub type Cells = BTreeMap<[i32; 3], [u8; 3]>;

/// The voxels selected, each in its owner's own grid, in the scope they
/// were selected in (another scope shows none); `copy`: DUPLICATE was
/// pressed, the next move leaves them where they were too.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    pub scope: Scope,
    pub cells: BTreeMap<Owner, BTreeSet<[i32; 3]>>,
    pub copy: bool,
}

impl Selection {
    pub fn count(&self) -> usize {
        self.cells.values().map(BTreeSet::len).sum()
    }
}

/// The box SELECT drew last, while + and - may still make it deeper (the
/// selection before it, which Ctrl added to).
#[derive(Clone, Debug, PartialEq)]
pub struct SelectBox {
    pub lo: IVec3,
    pub size: IVec3,
    pub way: IVec3,
    pub base: Option<Selection>,
}

/// The selection's arrows grabbed: the file (or block) and the selection
/// as at the press, and where the grid stood then.
#[derive(Clone, Debug)]
pub struct SelDrag {
    model: Option<Shaped>,
    block: Option<Block>,
    selection: Selection,
    origin: IVec3,
    moved: IVec3,
}

impl SelDrag {
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
}

/// COPY: the voxels by where they lay (from the pivot in a figure, else
/// in their frame or block), and the owner they came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub cells: Cells,
    pub from: Owner,
}

/// What a selection's step does to the voxels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelOp {
    Remove,
    Paint([u8; 3]),
    /// Mirrored across its own box along an axis.
    Flip(usize),
    /// A quarter turn about an axis, about its box's middle.
    Rotate(usize),
    /// Moved by whole voxels (from where the drag began); `keep`: what
    /// they land on stays (Alt); `copy`: what they leave stays too.
    Move(IVec3, bool, bool),
}

/// A frame's voxels where they lie in its model: a figure's part from the
/// pivot (its `at` added), a frame of frames in its own grid; and that
/// offset.
fn frame_cells(m: &ModelFile, f: usize) -> (Cells, IVec3) {
    let off = if m.parted() { m.origin(f).unwrap_or(IVec3::ZERO) } else { IVec3::ZERO };
    (block_cells(&m.frames[f], off), off)
}

/// A block's voxels, `off` added to where they lie.
fn block_cells(b: &Block, off: IVec3) -> Cells {
    (0..b.voxels.len()).filter_map(|i| b.colour_at(b.at(i)).map(|c| ((b.at(i) + off).to_array(), c))).collect()
}

/// A block holding `cells` (each less `off`), `size` big; those outside
/// left out.
fn block_of(size: IVec3, palette: &[[u8; 3]], cells: &Cells, off: IVec3) -> Block {
    let mut b = Block::boxed(size);
    b.palette = palette.to_vec();
    for (&c, &colour) in cells {
        let _ = b.set(IVec3::from(c) - off, Some(colour));
    }
    b
}

/// Frame `f` of the model holding `cells` (where `frame_cells` has them):
/// a figure's part's box grown to hold them (never shrunk), a frame of
/// frames' box, all the frames', grown about the pivot as a stroke grows
/// it. How far the voxels moved in the frame's grid (a frame of frames
/// grown on its low side), or None when it would pass 256 voxels.
fn put_frame(m: &mut ModelFile, f: usize, cells: &Cells) -> Option<IVec3> {
    let old = m.frames[f].size;
    let (mut lo, mut hi) = (IVec3::MAX, IVec3::MIN);
    for &c in cells.keys() {
        (lo, hi) = (lo.min(IVec3::from(c)), hi.max(IVec3::from(c)));
    }
    if m.parted() {
        let k = m.parts.iter().position(|p| p.frame == f)?;
        let low = m.parts[k].low;
        let (lo, hi) = if cells.is_empty() { (low, low + old - 1) } else { (lo.min(low), hi.max(low + old - 1)) };
        if (hi - lo + 1).cmpgt(IVec3::splat(256)).any() {
            return None;
        }
        let palette = m.frames[f].palette.clone();
        m.frames[f] = block_of(hi - lo + 1, &palette, cells, lo);
        m.parts[k].low = lo;
        return Some(IVec3::ZERO);
    }
    // A frame of frames: every frame as big; grown about the pivot.
    let (lo, hi) = if cells.is_empty() { (IVec3::ZERO, old - 1) } else { (lo.min(IVec3::ZERO), hi.max(old - 1)) };
    let (shift, size) = if lo == IVec3::ZERO && hi == old - 1 {
        (IVec3::ZERO, old)
    } else {
        let p = crate::vvx::pivot(old);
        let raise = IVec3::Z * (-lo.z).max(0);
        let size = super::super::frames::fit(old + raise, lo - p + raise, hi - p + raise);
        if size.cmpgt(IVec3::splat(256)).any() {
            return None;
        }
        (crate::vvx::pivot(size) - p + raise, size)
    };
    for k in 0..m.frames.len() {
        let mine = if k == f { cells.clone() } else { block_cells(&m.frames[k], IVec3::ZERO) };
        let palette = m.frames[k].palette.clone();
        m.frames[k] = block_of(size, &palette, &mine, -shift);
    }
    Some(shift)
}

/// The cells of `sel` (where they lie) changed by `op` in `voxels`, and
/// where they lie after.
fn apply(op: SelOp, voxels: &mut [Cells], sel: &mut [BTreeSet<[i32; 3]>]) {
    // The box of the whole selection, across its owners.
    let (mut lo, mut hi) = (IVec3::MAX, IVec3::MIN);
    for &c in sel.iter().flatten() {
        (lo, hi) = (lo.min(IVec3::from(c)), hi.max(IVec3::from(c)));
    }
    for (v, s) in voxels.iter_mut().zip(sel.iter_mut()) {
        let taken: Vec<([i32; 3], [u8; 3])> = s.iter().filter_map(|c| v.get(c).map(|&k| (*c, k))).collect();
        let map = |c: [i32; 3]| -> [i32; 3] {
            let c = IVec3::from(c);
            match op {
                SelOp::Flip(a) => {
                    let mut d = c;
                    d[a] = lo[a] + hi[a] - c[a];
                    d.to_array()
                }
                SelOp::Rotate(a) => {
                    // A quarter turn about axis `a`, the right hand's way,
                    // about the box's middle (doubled: whole numbers).
                    let (u, w) = ((a + 1) % 3, (a + 2) % 3);
                    let (su, sw) = (lo[u] + hi[u], lo[w] + hi[w]);
                    let (du, dw) = (2 * c[u] - su, 2 * c[w] - sw);
                    let mut d = c;
                    d[u] = (-dw + su).div_euclid(2);
                    d[w] = (du + sw).div_euclid(2);
                    d.to_array()
                }
                SelOp::Move(by, ..) => (c + by).to_array(),
                SelOp::Remove | SelOp::Paint(_) => c.to_array(),
            }
        };
        match op {
            SelOp::Remove => {
                for (c, _) in &taken {
                    v.remove(c);
                }
                s.clear();
            }
            SelOp::Paint(colour) => {
                for (c, _) in &taken {
                    v.insert(*c, colour);
                }
            }
            SelOp::Flip(_) | SelOp::Rotate(_) | SelOp::Move(..) => {
                let (keep, copy) = match op {
                    SelOp::Move(_, keep, copy) => (keep, copy),
                    _ => (false, false),
                };
                if !copy {
                    for (c, _) in &taken {
                        v.remove(c);
                    }
                }
                let mut now = BTreeSet::new();
                for (c, colour) in taken {
                    let to = map(c);
                    if keep && v.contains_key(&to) {
                        continue;
                    }
                    v.insert(to, colour);
                    now.insert(to);
                }
                *s = now;
            }
        }
    }
}

/// The voxels connected to `start` (faces only) of its color, in `cells`.
fn connected(cells: &Cells, start: [i32; 3]) -> BTreeSet<[i32; 3]> {
    let Some(&colour) = cells.get(&start) else { return BTreeSet::new() };
    let mut seen = BTreeSet::from([start]);
    let mut todo = vec![start];
    while let Some(c) = todo.pop() {
        for d in crate::mesh::DIRS {
            let n = (IVec3::from(c) + d).to_array();
            if cells.get(&n) == Some(&colour) && seen.insert(n) {
                todo.push(n);
            }
        }
    }
    seen
}

impl Editor {
    /// The selection, if it was made in the scope shown.
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref().filter(|s| s.scope == self.scope && s.count() > 0)
    }

    /// The owners of the voxels in the grid shown: a block, a frame (a
    /// part's), or every part of the figure whole, each with its offset in
    /// the grid shown (the part's place from the figure's low corner).
    fn owners_shown(&self) -> Vec<(Owner, IVec3)> {
        match self.scope {
            Scope::Block(i) => vec![(Owner::Block(i), IVec3::ZERO)],
            Scope::Frame(f) => vec![(Owner::Frame(f), IVec3::ZERO)],
            Scope::Figure => match (&self.model, &self.figure) {
                (Some(m), Some(fig)) => m.parts.iter().map(|p| (Owner::Frame(p.frame), p.low - fig.low)).collect(),
                _ => Vec::new(),
            },
            Scope::Level => Vec::new(),
        }
    }

    fn owner_block(&self, o: Owner) -> Option<&Block> {
        match o {
            Owner::Block(i) => self.blocks.get(i),
            Owner::Frame(f) => self.model.as_ref()?.frames.get(f),
        }
    }

    /// Where a selected voxel shows in the grid shown (its middle): in a
    /// figure whole, turned with its part's rest.
    pub fn shown_at(&self, o: Owner, c: [i32; 3]) -> Vec3 {
        let at = IVec3::from(c);
        if let (Scope::Figure, Owner::Frame(f), Some(fig), Some((lo, _))) = (self.scope, o, &self.figure, self.figure_box()) {
            let low = self.model.as_ref().and_then(|m| m.origin(f)).unwrap_or(IVec3::ZERO);
            if let Some(p) = fig.frames.iter().position(|&k| k == f) {
                return fig.rest_map(p, lo).transform_point3((at + low - fig.low + lo).as_vec3() + 0.5);
            }
        }
        at.as_vec3() + 0.5
    }

    /// Every selected voxel's middle in the grid shown.
    pub fn selection_shown(&self) -> Vec<Vec3> {
        let Some(s) = self.selection() else { return Vec::new() };
        s.cells.iter().flat_map(|(&o, cells)| cells.iter().map(move |&c| (o, c))).map(|(o, c)| self.shown_at(o, c)).collect()
    }

    /// The selection's outline in the grid shown: the edges of its outer
    /// faces where its surface turns (not across a flat side), as
    /// segments.
    pub fn selection_edges(&self) -> Vec<(Vec3, Vec3)> {
        let Some(s) = self.selection() else { return Vec::new() };
        let mut out = Vec::new();
        for (&o, cells) in &s.cells {
            // From the owner's own grid to the grid shown.
            let map: Box<dyn Fn(Vec3) -> Vec3> = match (self.scope, o, &self.figure, self.figure_box()) {
                (Scope::Figure, Owner::Frame(f), Some(fig), Some((lo, _))) => {
                    let low = self.model.as_ref().and_then(|m| m.origin(f)).unwrap_or(IVec3::ZERO);
                    let p = fig.frames.iter().position(|&k| k == f);
                    let shift = (low - fig.low + lo).as_vec3();
                    let rest = p.map(|p| fig.rest_map(p, lo));
                    Box::new(move |v: Vec3| rest.map_or(v + shift, |r| r.transform_point3(v + shift)))
                }
                _ => Box::new(|v| v),
            };
            let has = |c: IVec3| cells.contains(&c.to_array());
            for &c in cells {
                let c = IVec3::from(c);
                for n in crate::mesh::DIRS {
                    if has(c + n) {
                        continue;
                    }
                    // The face's four edges, each where the side does not go
                    // on flat past it.
                    let a = (0..3).find(|&a| n[a] != 0).unwrap_or(2);
                    let (u, w) = ((a + 1) % 3, (a + 2) % 3);
                    let mut face = c.as_vec3();
                    if n[a] > 0 {
                        face[a] += 1.0;
                    }
                    for (axis, along, side) in [(u, w, 0), (u, w, 1), (w, u, 0), (w, u, 1)] {
                        let mut t = IVec3::ZERO;
                        t[axis] = if side == 0 { -1 } else { 1 };
                        if has(c + t) && !has(c + t + n) {
                            continue;
                        }
                        let mut from = face;
                        from[axis] += side as f32;
                        let mut to = from;
                        to[along] += 1.0;
                        out.push((map(from), map(to)));
                    }
                }
            }
        }
        out
    }

    /// Whether the voxel at `cell` of the grid shown (of `part` in a figure
    /// whole) is selected.
    pub fn is_selected(&self, cell: IVec3, part: Option<usize>) -> bool {
        let Some(s) = self.selection() else { return false };
        self.owners_shown().iter().any(|&(o, off)| {
            let in_part = match (self.scope, o, part, &self.figure) {
                (Scope::Figure, Owner::Frame(f), Some(p), Some(fig)) => fig.frames.get(p) == Some(&f),
                _ => true,
            };
            in_part && s.cells.get(&o).is_some_and(|cells| cells.contains(&(cell - off).to_array()))
        })
    }

    /// The figure whole: the voxel a ray (in the grid shown) meets, as its
    /// part stands, and that part; in the part's grid as the file has it.
    pub fn aim_whole(&self, origin: Vec3, dir: Vec3) -> Option<(Hit, usize)> {
        let (fig, (lo, _)) = (self.figure.as_ref()?, self.figure_box()?);
        let p = fig.part_at(lo, origin, dir)?;
        let back = fig.rest_map(p, lo).inverse();
        let (o, d) = (back.transform_point3(origin) - lo.as_vec3(), back.transform_vector3(dir));
        let cells: crate::world::CellSet = fig.model.parts[p].voxels.iter().map(|&k| fig.model.frames[0][k as usize].0).collect();
        let hit = super::pick(o, d, fig.model.size, |c| cells.contains(&c))?;
        (hit.cell.z >= 0).then_some((Hit { cell: hit.cell + lo, normal: hit.normal }, p))
    }

    /// No selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.select_box = None;
    }

    /// The filled voxels in `lo..=hi` of the grid shown selected (`add`:
    /// besides those selected; else instead).
    pub fn select_box_cells(&mut self, lo: IVec3, hi: IVec3, add: bool) {
        let mut sel = self.selection().filter(|_| add).cloned().unwrap_or(Selection { scope: self.scope, cells: BTreeMap::new(), copy: false });
        for (o, off) in self.owners_shown() {
            let Some(b) = self.owner_block(o) else { continue };
            for z in lo.z..=hi.z {
                for y in lo.y..=hi.y {
                    for x in lo.x..=hi.x {
                        let c = IVec3::new(x, y, z) - off;
                        if b.get(c) != 0 {
                            sel.cells.entry(o).or_default().insert(c.to_array());
                        }
                    }
                }
            }
        }
        sel.cells.retain(|_, c| !c.is_empty());
        sel.copy = false;
        self.selection = Some(sel);
    }

    /// A double click with SELECT: the voxels of the clicked one's color
    /// connected to it (in its part), selected (`add`: besides).
    pub fn select_connected(&mut self, cell: IVec3, part: Option<usize>, add: bool) {
        let mut sel = self.selection().filter(|_| add).cloned().unwrap_or(Selection { scope: self.scope, cells: BTreeMap::new(), copy: false });
        for (o, off) in self.owners_shown() {
            if let (Scope::Figure, Owner::Frame(f), Some(fig)) = (self.scope, o, &self.figure)
                && part.and_then(|p| fig.frames.get(p)) != Some(&f)
            {
                continue;
            }
            let Some(b) = self.owner_block(o) else { continue };
            let found = connected(&block_cells(b, IVec3::ZERO), (cell - off).to_array());
            if !found.is_empty() {
                sel.cells.entry(o).or_default().extend(found);
                break;
            }
        }
        sel.copy = false;
        self.selection = Some(sel);
    }

    /// SELECT's footprint let go: what lies in its box selected (`add`:
    /// besides), the box kept for + and -.
    pub fn select_drawn(&mut self, add: bool) {
        let Some((lo, hi)) = self.form_box().filter(|_| self.drawing.is_some()) else { return };
        let Some(d) = self.drawing.take() else { return };
        let base = self.selection().filter(|_| add).cloned();
        let way = super::deep_way(d.normal, d.cut);
        self.depth = (hi - lo + 1)[super::axis_of(way)];
        self.select_box_cells(lo, hi, add);
        self.select_box = Some(SelectBox { lo, size: hi - lo + 1, way, base });
    }

    /// The + or - key with SELECT: the box drawn last a voxel deeper or
    /// shallower, its voxels selected again; whether there was one.
    pub fn deepen_select(&mut self, by: i32) -> bool {
        let Some(mut b) = self.select_box.take() else { return false };
        let a = super::axis_of(b.way);
        let deeper = (b.size[a] + by).clamp(1, 256);
        if b.way[a] < 0 {
            b.lo[a] -= deeper - b.size[a];
        }
        b.size[a] = deeper;
        self.depth = deeper;
        self.selection = b.base.clone();
        self.select_box_cells(b.lo, b.lo + b.size - 1, b.base.is_some());
        self.select_box = Some(b);
        true
    }

    /// The selection's arrows: at its middle, X, Y and Z, the heads only;
    /// its box the bar keeps clear of.
    pub fn selection_handles(&self, view_proj: glam::Mat4) -> Option<Handles> {
        let at = self.selection_shown();
        if at.is_empty() || self.posing {
            return None;
        }
        let (lo, hi) = at.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &c| (lo.min(c - 0.5), hi.max(c + 0.5)));
        let centre = (lo + hi) / 2.0;
        let mut h = Handles::at(centre, handles::world_size(view_proj, centre, 0.1));
        h.arrows = [Some(Vec3::X), Some(Vec3::Y), Some(Vec3::Z)];
        h.body = (0..8).map(|k| Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z })).collect();
        h.heads_only = true;
        Some(h)
    }

    /// Where the grid shown stands (its voxel 0, 0, 0 from the pivot, or
    /// for a frame of frames how far its voxels moved).
    fn grid_mark(&self) -> (IVec3, IVec3) {
        (self.origin().unwrap_or(IVec3::ZERO), self.moved)
    }

    /// The selection's arrow grabbed (`grab_part` passes it on): the file
    /// and the selection as they are, for the drag to move them from.
    pub fn grab_selection(&mut self) {
        let Some(selection) = self.selection().cloned() else { return };
        let (origin, moved) = self.grid_mark();
        let (model, block) = match self.scope {
            Scope::Block(i) => (None, self.blocks.get(i).cloned()),
            _ => (self.model.as_ref().map(ModelFile::shaped), None),
        };
        self.sel_drag = Some(SelDrag { model, block, selection, origin, moved });
    }

    /// While the selection's arrow is dragged: how far the grid shown went
    /// since the press (a box grown on its low side), to see the drag in
    /// the grid as it was.
    pub(super) fn selection_drift(&self) -> Option<Vec3> {
        let d = self.sel_drag.as_ref()?;
        let (origin, moved) = self.grid_mark();
        Some(((origin - d.origin) - (moved - d.moved)).as_vec3())
    }

    /// The selection moved `by` from where the drag began (`keep`: what it
    /// lands on stays), one step for the drag.
    pub fn move_selection_to(&mut self, by: IVec3, keep: bool, joined: bool) {
        let Some(d) = self.sel_drag.clone() else { return };
        // As far as a part may go, no further (`FAR`).
        let by = by.clamp(IVec3::splat(-super::FAR), IVec3::splat(super::FAR));
        // From the start each time: what it passes over stays.
        match (&d.model, &d.block, self.scope) {
            (Some(start), _, _) => {
                if let Some(m) = &mut self.model {
                    m.reshape(start);
                }
                self.moved = d.moved;
                self.follow_figure();
            }
            (_, Some(start), Scope::Block(i)) => self.blocks[i] = start.clone(),
            _ => return,
        }
        self.selection = Some(d.selection.clone());
        self.join_next = joined;
        self.selection_step(SelOp::Move(by, keep, d.selection.copy));
        self.join_next = false;
    }

    /// The selection's arrows let go.
    pub fn let_go_of_selection(&mut self) {
        if self.sel_drag.take().is_some()
            && let Some(s) = &mut self.selection
        {
            s.copy = false;
        }
    }

    /// `op` at the selection: one step (one with the last while its arrows
    /// are dragged), written; the selection where its voxels went.
    pub fn selection_step(&mut self, op: SelOp) {
        let Some(sel) = self.selection().cloned() else { return };
        let owners: Vec<Owner> = sel.cells.keys().copied().collect();
        if let Scope::Block(i) = self.scope {
            let joined = match (self.join_next, self.past.last()) {
                (true, Some(Step::Block(k, before, _))) if *k == i => Some(before.clone()),
                _ => None,
            };
            if joined.is_some() {
                self.past.pop();
            }
            let before = self.blocks[i].clone();
            let mut voxels = vec![block_cells(&before, IVec3::ZERO)];
            let mut cells = vec![sel.cells.get(&Owner::Block(i)).cloned().unwrap_or_default()];
            apply(op, &mut voxels, &mut cells);
            // A block keeps its size: what lies outside is left out.
            let size = before.size;
            let inside = |c: &[i32; 3]| IVec3::from(*c).cmpge(IVec3::ZERO).all() && IVec3::from(*c).cmplt(size).all();
            voxels[0].retain(|c, _| inside(c));
            cells[0].retain(inside);
            self.blocks[i] = block_of(size, &before.palette, &voxels[0], IVec3::ZERO);
            let first = joined.unwrap_or(before);
            if self.blocks[i] != first {
                self.commit(Step::Block(i, first, self.blocks[i].clone()));
            }
            self.selection = Some(Selection { scope: self.scope, cells: BTreeMap::from([(Owner::Block(i), cells.remove(0))]), copy: sel.copy });
            return;
        }
        type Went = (Vec<BTreeSet<[i32; 3]>>, Vec<IVec3>, IVec3);
        let mut out: Option<Went> = None;
        let parts = self.model.as_ref().is_some_and(ModelFile::parted);
        // (A part's voxels moved out of it may change how the others hang:
        // they keep their rig, `pin_rig`.)
        let acting: Vec<String> = self.model.as_ref().map_or_else(Vec::new, |m| m.parts.iter().filter(|p| owners.contains(&Owner::Frame(p.frame))).map(|p| p.name.clone()).collect());
        if parts {
            self.acting_on(acting);
        }
        self.change_model_file(|m| {
            let mut voxels = Vec::new();
            let mut cells = Vec::new();
            let mut offs = Vec::new();
            for &o in &owners {
                let Owner::Frame(f) = o else { continue };
                let (v, off) = frame_cells(m, f);
                voxels.push(v);
                cells.push(sel.cells[&o].iter().map(|c| (IVec3::from(*c) + off).to_array()).collect::<BTreeSet<_>>());
                offs.push(f);
            }
            apply(op, &mut voxels, &mut cells);
            let mut shift = IVec3::ZERO;
            let mut lows = Vec::new();
            for (k, &f) in offs.iter().enumerate() {
                let Some(s) = put_frame(m, f, &voxels[k]) else { continue };
                shift += s;
                lows.push(if m.parted() { m.origin(f).unwrap_or(IVec3::ZERO) } else { -s });
            }
            out = Some((cells, lows, shift));
        });
        self.follow_figure();
        let Some((cells, lows, shift)) = out else { return };
        self.moved += shift;
        let mut now = Selection { scope: self.scope, cells: BTreeMap::new(), copy: sel.copy };
        for ((o, c), low) in owners.iter().zip(cells).zip(lows) {
            let local: BTreeSet<[i32; 3]> = c.iter().map(|c| (IVec3::from(*c) - low).to_array()).collect();
            if !local.is_empty() {
                now.cells.insert(*o, local);
            }
        }
        self.selection = Some(now);
    }

    /// DUPLICATE: a copy in place, selected, to be moved away (the next
    /// move leaves the voxels where they were too).
    pub fn duplicate_selection(&mut self) {
        if let Some(s) = self.selection.as_mut().filter(|s| s.scope == self.scope) {
            s.copy = true;
        }
    }

    /// COPY: the selected voxels (from the pivot in a figure).
    pub fn copy_selection(&mut self) {
        let Some(sel) = self.selection().cloned() else { return };
        let mut cells = Cells::new();
        let mut from = None;
        for (&o, set) in &sel.cells {
            let (all, off) = match (o, &self.model) {
                (Owner::Frame(f), Some(m)) => frame_cells(m, f),
                (Owner::Block(i), _) => (self.blocks.get(i).map_or_else(Cells::new, |b| block_cells(b, IVec3::ZERO)), IVec3::ZERO),
                _ => continue,
            };
            for c in set {
                let at = (IVec3::from(*c) + off).to_array();
                if let Some(&k) = all.get(&at) {
                    cells.insert(at, k);
                }
            }
            from.get_or_insert(o);
        }
        if let Some(from) = from {
            self.clipboard = Some(Clip { cells, from });
        }
    }

    /// PASTE: what COPY took, where it lay, into the block or frame shown
    /// (the part chosen; in a figure whole, the part it came from),
    /// selected, to be moved; one step.
    pub fn paste(&mut self) {
        let Some(clip) = self.clipboard.clone() else { return };
        let owner = match self.scope {
            Scope::Block(i) => Owner::Block(i),
            Scope::Frame(f) => Owner::Frame(f),
            Scope::Figure => clip.from,
            Scope::Level => return,
        };
        // Where it came from, as it was: a copy in place to move away.
        let off = match (owner, &self.model) {
            (Owner::Frame(f), Some(m)) => frame_cells(m, f).1,
            _ => IVec3::ZERO,
        };
        let local: BTreeSet<[i32; 3]> = clip.cells.keys().map(|c| (IVec3::from(*c) - off).to_array()).collect();
        let same = self.owner_block(owner).is_some_and(|b| clip.cells.iter().all(|(c, k)| b.colour_at(IVec3::from(*c) - off) == Some(*k)));
        if same {
            self.selection = Some(Selection { scope: self.scope, cells: BTreeMap::from([(owner, local)]), copy: true });
            return;
        }
        // Else laid in: the copy is a step of its own, selected.
        self.selection = Some(Selection { scope: self.scope, cells: BTreeMap::from([(owner, BTreeSet::new())]), copy: false });
        match (owner, self.scope) {
            (Owner::Block(i), Scope::Block(_)) => {
                let before = self.blocks[i].clone();
                let mut all = block_cells(&before, IVec3::ZERO);
                all.extend(clip.cells.iter().map(|(c, k)| (*c, *k)));
                let size = before.size;
                all.retain(|c, _| IVec3::from(*c).cmpge(IVec3::ZERO).all() && IVec3::from(*c).cmplt(size).all());
                self.blocks[i] = block_of(size, &before.palette, &all, IVec3::ZERO);
                self.commit(Step::Block(i, before, self.blocks[i].clone()));
                let local = local.into_iter().filter(|c| IVec3::from(*c).cmpge(IVec3::ZERO).all() && IVec3::from(*c).cmplt(size).all()).collect();
                self.selection = Some(Selection { scope: self.scope, cells: BTreeMap::from([(owner, local)]), copy: false });
            }
            (Owner::Frame(f), _) => {
                let mut low = IVec3::ZERO;
                if let Some(m) = &self.model {
                    let name: Vec<String> = m.parts.iter().filter(|p| p.frame == f).map(|p| p.name.clone()).collect();
                    self.acting_on(name);
                }
                self.change_model_file(|m| {
                    let (mut all, _) = frame_cells(m, f);
                    all.extend(clip.cells.iter().map(|(c, k)| (*c, *k)));
                    let shift = put_frame(m, f, &all).unwrap_or(IVec3::ZERO);
                    low = if m.parted() { m.origin(f).unwrap_or(IVec3::ZERO) } else { -shift };
                });
                self.follow_figure();
                let local = clip.cells.keys().map(|c| (IVec3::from(*c) - low).to_array()).collect();
                self.selection = Some(Selection { scope: self.scope, cells: BTreeMap::from([(owner, local)]), copy: false });
            }
            _ => {}
        }
    }

    /// Why the selection cannot become a part or go to another (None: it
    /// can): only in a figure.
    pub fn cannot_part(&self) -> Option<&'static str> {
        (!self.parted() || self.figure.is_none()).then_some("Only in a figure of parts")
    }

    /// NEW PART: the selected voxels out of their part(s) into a new part
    /// (its name the next free `part`, to type), hanging on the part most
    /// of them came from; the others keep their rig; one step.
    pub fn selection_to_new_part(&mut self) -> Option<String> {
        if self.cannot_part().is_some() {
            return None;
        }
        let sel = self.selection().cloned()?;
        let name = self.free_part_name("part");
        let m = self.model.as_ref()?;
        // The part most of it came from.
        let parent = sel.cells.iter().max_by_key(|(_, c)| c.len()).and_then(|(o, _)| match o {
            Owner::Frame(f) => m.parts.iter().find(|p| p.frame == *f).map(|p| p.name.clone()),
            Owner::Block(_) => None,
        });
        let new = name.clone();
        self.acting_on(vec![name.clone()]);
        self.change_model_file(move |m| {
            let mut taken = Cells::new();
            for (o, set) in &sel.cells {
                let Owner::Frame(f) = *o else { continue };
                let (mut all, off) = frame_cells(m, f);
                for c in set {
                    let at = (IVec3::from(*c) + off).to_array();
                    if let Some(k) = all.remove(&at) {
                        taken.insert(at, k);
                    }
                }
                put_frame(m, f, &all);
            }
            if taken.is_empty() {
                return;
            }
            m.frames.push(Block::boxed(IVec3::ONE));
            m.parts.push(Placed { name: new.clone(), frame: m.frames.len() - 1, low: IVec3::ZERO, parent, joint: None, rotation: [0.0; 3] });
            let f = m.frames.len() - 1;
            m.parts.last_mut().expect("just put").low = IVec3::from(*taken.keys().next().expect("one at least"));
            put_frame(m, f, &taken);
        });
        self.clear_selection();
        self.follow_figure();
        self.choose_named(&name);
        Some(name)
    }

    /// MOVE TO PART: the selected voxels out of their part(s) into part
    /// `into` (its box grown), where they lie; one step.
    pub fn selection_to_part(&mut self, into: usize) {
        if self.cannot_part().is_some() {
            return;
        }
        let Some(sel) = self.selection().cloned() else { return };
        let Some(target) = self.figure.as_ref().and_then(|f| f.parts().get(into)).map(|p| p.name.clone()) else { return };
        self.acting_on(vec![target.clone()]);
        self.change_model_file(move |m| {
            let Some(to) = m.parts.iter().find(|p| p.name == target).map(|p| p.frame) else { return };
            let mut taken = Cells::new();
            for (o, set) in &sel.cells {
                let Owner::Frame(f) = *o else { continue };
                if f == to {
                    continue;
                }
                let (mut all, off) = frame_cells(m, f);
                for c in set {
                    let at = (IVec3::from(*c) + off).to_array();
                    if let Some(k) = all.remove(&at) {
                        taken.insert(at, k);
                    }
                }
                put_frame(m, f, &all);
            }
            let (mut all, _) = frame_cells(m, to);
            all.extend(taken);
            put_frame(m, to, &all);
        });
        self.clear_selection();
        self.follow_figure();
    }

    /// FILL at a hit: the voxels connected to the one hit of its color (in
    /// its block, frame or part) take the held color; `all`: every voxel
    /// of that color there. One step.
    pub fn fill_at(&mut self, hit: Hit, all: bool) {
        let colour = self.colour;
        let Some(b) = self.shape() else { return };
        let Some(was) = b.colour_at(hit.cell) else { return };
        if was == colour {
            return;
        }
        let cells = block_cells(b, IVec3::ZERO);
        let found: BTreeSet<[i32; 3]> = if all { cells.iter().filter(|(_, k)| **k == was).map(|(c, _)| *c).collect() } else { connected(&cells, hit.cell.to_array()) };
        self.stroke = Some(match self.scope {
            Scope::Block(i) => Step::Block(i, self.blocks[i].clone(), self.blocks[i].clone()),
            Scope::Frame(_) => Step::Model(self.shaped(), Shaped::default()),
            _ => return,
        });
        if let Some(b) = self.shape_mut() {
            for c in found {
                let _ = b.set(IVec3::from(c), Some(colour));
            }
        }
        self.release();
    }

    /// REPLACE WITH HELD: every voxel of color `was` (in the block shown,
    /// or every frame of the model) takes the held one; one step.
    pub fn replace_colour(&mut self, was: [u8; 3]) {
        let colour = self.colour;
        if was == colour {
            return;
        }
        let recolor = |b: &mut Block| {
            let cells: Vec<IVec3> = (0..b.voxels.len()).map(|i| b.at(i)).filter(|&c| b.colour_at(c) == Some(was)).collect();
            for c in cells {
                let _ = b.set(c, Some(colour));
            }
        };
        match self.scope {
            Scope::Block(i) => {
                let before = self.blocks[i].clone();
                recolor(&mut self.blocks[i]);
                if self.blocks[i] != before {
                    self.commit(Step::Block(i, before, self.blocks[i].clone()));
                }
            }
            Scope::Frame(_) | Scope::Figure => {
                self.acting_on(Vec::new());
                self.change_model_file(|m| m.frames.iter_mut().for_each(recolor));
                self.follow_figure();
            }
            Scope::Level => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::model::parts::tests::{figure, part};
    use crate::editor::view::{Action, act};

    fn text(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("fig.vvx")).unwrap()
    }

    /// The figure whole: a box over the arm's top and the body's beside it
    /// selects voxels of both parts.
    fn spanning(ed: &mut Editor) {
        let f = ed.figure.as_ref().unwrap();
        // From the pivot: x 2..=3 (the body's last column, the arm's
        // first), y -1, z 3..=4.
        let lo = IVec3::new(2, -1, 3) - f.low;
        ed.select_box_cells(lo, lo + IVec3::new(1, 0, 1), false);
    }

    /// SELECT by a box in the figure whole: voxels of two parts; NEW PART
    /// takes them from both into a part hanging on the one most came from;
    /// the other parts' text is kept; UNDO and REDO.
    #[test]
    fn a_selection_across_two_parts_becomes_a_new_part() {
        let (mut ed, mut view, dir) = figure("sel-new-part");
        act(&mut ed, &mut view, Action::WholeFigure);
        spanning(&mut ed);
        let s = ed.selection().expect("selected").clone();
        assert_eq!((s.cells.len(), s.count()), (2, 4), "two of the body, two of the arm: {s:?}");
        let before = text(&dir);
        let head_text = |t: &str| t.split("[[part]]").find(|p| p.contains("name = \"head\"")).unwrap().to_string();
        let name = ed.selection_to_new_part().expect("a new part");
        let after = text(&dir);
        assert_eq!(head_text(&after), head_text(&before), "the head's text kept");
        let m = ed.model.as_ref().unwrap();
        let o = m.parts.iter().find(|p| p.name == name).unwrap();
        assert_eq!(block_cells(&m.frames[o.frame], o.low).len(), 4);
        assert!(o.parent.is_some(), "hangs on a part it came from");
        let body = m.parts.iter().find(|p| p.name == "body").unwrap();
        assert_eq!(block_cells(&m.frames[body.frame], IVec3::ZERO).len(), 6 * 4 * 8 - 2, "two voxels left the body");
        ed.undo();
        assert_eq!(text(&dir), before, "UNDO: as it was");
        ed.redo();
        assert_eq!(text(&dir), after, "REDO: again");
    }

    /// MOVE by the arrows' amount, one step for the drag, from where it
    /// began (what it passes stays); FLIP and ROTATE in place; REMOVE;
    /// PAINT; each a step that UNDO takes back.
    #[test]
    fn a_selection_moves_flips_rotates_and_goes() {
        let (mut ed, mut view, dir) = figure("sel-ops");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        // The arm is 2 by 2 by 5: its top two layers.
        ed.select_box_cells(IVec3::new(0, 0, 3), IVec3::new(1, 1, 4), false);
        assert_eq!(ed.selection().unwrap().count(), 8);
        let start = text(&dir);
        let steps = ed.steps();
        ed.grab_selection();
        for k in 1..=3 {
            ed.move_selection_to(IVec3::new(0, 0, k), false, k > 1);
        }
        ed.let_go_of_selection();
        assert_eq!(ed.steps(), steps + 1, "one step for the drag");
        let m = ed.model.as_ref().unwrap();
        let o = m.parts.iter().find(|p| p.name == "arm").unwrap();
        let cells = block_cells(&m.frames[o.frame], o.low);
        assert_eq!(cells.len(), 20, "nothing lost");
        assert_eq!(cells.keys().map(|c| c[2]).max(), Some(3 + 4 + 3), "three up, the box grown");
        ed.undo();
        assert_eq!(text(&dir), start);
        // One voxel of another color; then FLIP and ROTATE the whole arm:
        // the same voxels, one step each.
        ed.select_box_cells(IVec3::ZERO, IVec3::ZERO, false);
        ed.selection_step(SelOp::Paint([1, 2, 3]));
        assert!(text(&dir).contains("#010203"), "painted");
        ed.select_box_cells(IVec3::new(0, 0, 0), IVec3::new(1, 1, 4), false);
        for op in [SelOp::Flip(0), SelOp::Rotate(2), SelOp::Flip(2)] {
            let n = ed.steps();
            ed.selection_step(op);
            assert_eq!(ed.steps(), n + 1, "{op:?}: a step");
            assert_eq!(ed.selection().unwrap().count(), 20, "{op:?}: still selected");
        }
        let m = ed.model.as_ref().unwrap();
        let o = m.parts.iter().find(|p| p.name == "arm").unwrap();
        let odd: Vec<[i32; 3]> = block_cells(&m.frames[o.frame], IVec3::ZERO).into_iter().filter(|(_, k)| *k == [1, 2, 3]).map(|(c, _)| c).collect();
        // (0,0,0) flipped in x: (1,0,0); a quarter round z about the 2 by 2
        // middle: (1,1,0); flipped in z: (1,1,4).
        assert_eq!(odd, [[1, 1, 4]]);
        ed.selection_step(SelOp::Remove);
        let m = ed.model.as_ref().unwrap();
        let o = m.parts.iter().find(|p| p.name == "arm").unwrap();
        assert!(block_cells(&m.frames[o.frame], o.low).is_empty(), "removed");
        for _ in 0..5 {
            ed.undo();
        }
        assert_eq!(text(&dir), start, "five steps back");
    }

    /// DUPLICATE, then a move: the voxels stay where they were and a copy
    /// goes; COPY in the arm, PASTE in the head: where it lay, selected.
    #[test]
    fn a_selection_is_duplicated_copied_and_pasted() {
        let (mut ed, mut view, _dir) = figure("sel-copy");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        ed.select_box_cells(IVec3::new(0, 0, 4), IVec3::new(1, 1, 4), false);
        ed.duplicate_selection();
        ed.grab_selection();
        ed.move_selection_to(IVec3::new(0, 0, 1), false, false);
        ed.let_go_of_selection();
        let count = |ed: &Editor, name: &str| {
            let m = ed.model.as_ref().unwrap();
            let o = m.parts.iter().find(|p| p.name == name).unwrap();
            block_cells(&m.frames[o.frame], o.low).len()
        };
        assert_eq!(count(&ed, "arm"), 24, "four more: the copy");
        ed.copy_selection();
        let head = part(&ed, "head");
        act(&mut ed, &mut view, Action::Part(head));
        let before = count(&ed, "head");
        ed.paste();
        assert_eq!(count(&ed, "head"), before + 4, "pasted where it lay, into the head");
        assert_eq!(ed.selection().unwrap().count(), 4, "selected");
    }

    /// FILL: the connected voxels of a color; Ctrl, all of that color in
    /// the part; one step each. REPLACE WITH HELD: every voxel of a color.
    #[test]
    fn fill_and_replace() {
        let (mut ed, mut view, dir) = figure("fill");
        let body = part(&ed, "body");
        act(&mut ed, &mut view, Action::Part(body));
        ed.colour = [9, 9, 9];
        let steps = ed.steps();
        ed.fill_at(Hit { cell: IVec3::new(0, 0, 0), normal: IVec3::Z }, false);
        assert_eq!(ed.steps(), steps + 1);
        let m = ed.model.as_ref().unwrap();
        let o = m.parts.iter().find(|p| p.name == "body").unwrap();
        assert!(block_cells(&m.frames[o.frame], IVec3::ZERO).values().all(|&k| k == [9, 9, 9]), "the body filled");
        assert!(text(&dir).contains("#090909"));
        ed.colour = [7, 7, 7];
        ed.replace_colour([9, 9, 9]);
        assert!(text(&dir).contains("#070707") && !text(&dir).contains("#090909 "), "replaced");
        ed.undo();
        ed.undo();
        assert!(!text(&dir).contains("#090909"));
    }

    /// A double click's selection: the voxels of the color connected.
    #[test]
    fn a_double_click_selects_what_is_connected() {
        let (mut ed, mut view, _dir) = figure("sel-connected");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        ed.select_connected(IVec3::new(0, 0, 0), None, false);
        assert_eq!(ed.selection().unwrap().count(), 20, "the whole arm, one color");
        ed.clear_selection();
        assert!(ed.selection().is_none());
    }

    /// In a level's block: selected, moved (what leaves the block is left
    /// out, the block keeps its size), removed; UNDO.
    #[test]
    fn a_selection_in_a_block() {
        use crate::config::GameConfig;
        use crate::editor::view::View;
        let dir = crate::editor::model::tests::folder("sel-block");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        let mut view = View::new(&ed, &GameConfig::default());
        act(&mut ed, &mut view, Action::Hold(0));
        act(&mut ed, &mut view, Action::Hold(0));
        assert_eq!(ed.scope, Scope::Block(0));
        let n = crate::editor::model::tests::N;
        let all = |ed: &Editor| block_cells(&ed.blocks[0], IVec3::ZERO).len();
        let full = all(&ed);
        ed.select_box_cells(IVec3::new(0, 0, n - 1), IVec3::new(n - 1, n - 1, n - 1), false);
        assert_eq!(ed.selection().unwrap().count() as i32, n * n);
        ed.grab_selection();
        ed.move_selection_to(IVec3::Z, false, false);
        ed.let_go_of_selection();
        assert_eq!(all(&ed) as i32, full as i32 - n * n, "moved out of the block: left out");
        ed.undo();
        assert_eq!(all(&ed), full);
        ed.select_box_cells(IVec3::ZERO, IVec3::new(n - 1, n - 1, 0), false);
        ed.selection_step(SelOp::Remove);
        assert_eq!(all(&ed) as i32, full as i32 - n * n);
        ed.undo();
        assert_eq!(all(&ed), full);
    }

    /// A model of frames: a selection moved past its box grows the box
    /// (every frame's, about the pivot), the voxels kept; Alt keeps what
    /// they land on.
    #[test]
    fn a_frame_grows_for_a_selection_moved_out() {
        let dir = crate::scratch::Scratch::new("sel-frame");
        let text = format!("format = \"{}\"\nsize = [2, 1, 1]\n\n[palette]\na = \"#808080\"\nb = \"#ff0000\"\n\n[[frame]]\nlayers = \"\"\"\nab\n\"\"\"\n", crate::vvx::FORMAT);
        std::fs::write(dir.join("m.vvx"), text).unwrap();
        let mut ed = Editor::open_model(&dir, "m.vvx").unwrap();
        assert_eq!(ed.scope, Scope::Frame(0));
        ed.select_box_cells(IVec3::new(1, 0, 0), IVec3::new(1, 0, 0), false);
        ed.grab_selection();
        ed.move_selection_to(IVec3::new(2, 0, 0), false, false);
        ed.let_go_of_selection();
        let b = &ed.model.as_ref().unwrap().frames[0];
        assert!(b.size.x >= 4, "the box grew: {}", b.size);
        assert_eq!(block_cells(b, IVec3::ZERO).len(), 2, "both voxels kept");
        ed.undo();
        // Alt: the red one onto the grey one, the grey one stays.
        ed.select_box_cells(IVec3::new(1, 0, 0), IVec3::new(1, 0, 0), false);
        ed.grab_selection();
        ed.move_selection_to(IVec3::new(-1, 0, 0), true, false);
        ed.let_go_of_selection();
        let b = &ed.model.as_ref().unwrap().frames[0];
        assert_eq!(block_cells(b, IVec3::ZERO).values().copied().collect::<Vec<_>>(), [[0x80, 0x80, 0x80]], "the grey one kept");
    }

    /// MOVE TO PART: selected voxels of the arm go into the head where they
    /// lie; the body's text is kept.
    #[test]
    fn a_selection_goes_to_another_part() {
        let (mut ed, mut view, dir) = figure("sel-to-part");
        let arm = part(&ed, "arm");
        act(&mut ed, &mut view, Action::Part(arm));
        ed.select_box_cells(IVec3::new(0, 0, 4), IVec3::new(1, 1, 4), false);
        let body_text = |t: &str| t.split("[[part]]").find(|p| p.contains("name = \"body\"")).unwrap().to_string();
        let before = text(&dir);
        let head = part(&ed, "head");
        act(&mut ed, &mut view, Action::SelToPart(head));
        let m = ed.model.as_ref().unwrap();
        let count = |name: &str| {
            let o = m.parts.iter().find(|p| p.name == name).unwrap();
            block_cells(&m.frames[o.frame], o.low).len()
        };
        assert_eq!((count("arm"), count("head")), (16, 4 * 4 * 3 + 4));
        assert_eq!(body_text(&text(&dir)), body_text(&before), "the body's text kept");
        ed.undo();
        assert_eq!(text(&dir), before);
    }
}

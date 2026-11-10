//! What the editor holds and does, without a window (so it is tried in
//! tests): a level of blocks and the block being shaped, the tools, the
//! history, saving every change at once.
//!
//! Both levels of the editor are a grid with the same four tools: in the
//! level the cells hold blocks (PLACE, REMOVE, REPLACE, PICK, and the held
//! block turned, rolled, mirrored), in a block the voxels hold colours
//! (ADD, ERASE, PAINT, PICK). A ray from the view finds the cell it meets
//! first and the face it enters through; a floor under the grid makes room
//! to build on nothing. A stroke (pressed, dragged, let go) works on the
//! surface it began on: dragging ADD lays one layer, ERASE takes one off,
//! instead of growing towards the eye or digging in. Every stroke is one
//! step back (UNDO) and is saved at once: the level file or the block's.
//!
//! The level has a second layer, its things (figures, enemies, pickups,
//! props): PLACE puts one of the held kind where the mouse points (on a
//! surface, or before a wall, in the middle of the row's depth), REMOVE
//! takes one away, MOVE drags one to another place, PICK takes up its kind.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use glam::{IVec3, Vec3};

use super::frames::ModelFile;
use crate::blocks::{self, Block, Turn};
use crate::grid::{BlockEntry, Cell, GridLevel, Kind, Thing};

/// What the editor shows: the level, one block of it (its place in the
/// level's blocks), or a frame of a model (an editor opened on a model has
/// only those).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Level,
    Block(usize),
    Frame(usize),
}

/// What the level's tools work on: its blocks, or its things.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Blocks,
    Things,
}

/// The tools, the same four on both levels (named by `Tool::label`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// A block into the cell before the face (a voxel before it).
    Add,
    Remove,
    /// The cell's block replaced by the held one (the voxel recoloured).
    Paint,
    /// In a block: the voxel recoloured and every one behind it, through
    /// the block (empty places stay empty).
    Deep,
    /// Among the things: one dragged to another place.
    Move,
    /// The cell's block (the voxel's colour) taken up; then back to the
    /// tool before.
    Pick,
    /// In a block or a model: a box drawn on a face (its footprint
    /// dragged, its depth by + and -), then set with a click (`Held`).
    Box,
    /// The same, an ellipsoid in that box.
    Sphere,
}

impl Tool {
    /// The tools of the level's blocks, its things, and of a block.
    pub fn of(scope: Scope, layer: Layer) -> &'static [Tool] {
        match (scope, layer) {
            (Scope::Level, Layer::Blocks) => &[Tool::Add, Tool::Remove, Tool::Paint, Tool::Pick],
            (Scope::Level, Layer::Things) => &[Tool::Add, Tool::Remove, Tool::Move, Tool::Pick],
            (Scope::Block(_) | Scope::Frame(_), _) => &[Tool::Add, Tool::Remove, Tool::Paint, Tool::Deep, Tool::Box, Tool::Sphere, Tool::Pick],
        }
    }

    pub fn label(self, scope: Scope) -> &'static str {
        match (scope, self) {
            (Scope::Level, Tool::Add) => "PLACE",
            (Scope::Level, Tool::Remove) => "REMOVE",
            (Scope::Level, Tool::Paint) => "REPLACE",
            (Scope::Block(_) | Scope::Frame(_), Tool::Add) => "ADD",
            (Scope::Block(_) | Scope::Frame(_), Tool::Remove) => "ERASE",
            (Scope::Block(_) | Scope::Frame(_), Tool::Paint) => "PAINT",
            (_, Tool::Deep) => "DEEP PAINT",
            (_, Tool::Move) => "MOVE",
            (_, Tool::Pick) => "PICK",
            (_, Tool::Box) => "BOX",
            (_, Tool::Sphere) => "SPHERE",
        }
    }
}

/// The shape BOX or SPHERE holds on the mouse: its size along the grid's
/// directions, and, while it stands where it was drawn, its low corner
/// and the way its depth goes (None once the mouse moved on: then it
/// stands at the face pointed at, its depth out of it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
    pub size: IVec3,
    pub at: Option<(IVec3, IVec3)>,
}

/// A shape's footprint being dragged out on a face's plane: the cell it
/// began at (before the face, or the voxel itself to cut), the face's way
/// out, where the mouse is on that plane now, and whether it cuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawing {
    pub start: IVec3,
    pub normal: IVec3,
    pub end: IVec3,
    pub cut: bool,
}

/// The direction a face's way out lies along (z for none: the floor).
fn axis_of(normal: IVec3) -> usize {
    (0..3).find(|&a| normal[a] != 0).unwrap_or(2)
}

/// A face's way out, the floor's (none) up.
fn way_out(hit: Hit) -> IVec3 {
    if hit.normal == IVec3::ZERO { IVec3::Z } else { hit.normal }
}

/// The way a shape's depth goes from a face: out of it, or into the
/// voxels to cut.
fn deep_way(normal: IVec3, cut: bool) -> IVec3 {
    if cut { -normal } else { normal }
}

/// The cells of a BOX from `lo` to `hi` (its edges cut back a voxel with
/// `bevel`, where it is three voxels across or more), or of a SPHERE (the
/// ellipsoid that box holds).
pub fn form_cells(tool: Tool, lo: IVec3, hi: IVec3, bevel: bool) -> Vec<IVec3> {
    let size = hi - lo + 1;
    let (middle, radius) = ((lo + hi + 1).as_vec3() * 0.5, size.as_vec3() * 0.5);
    let mut cells = Vec::new();
    for z in lo.z..=hi.z {
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let c = IVec3::new(x, y, z);
                let keep = match tool {
                    Tool::Sphere => ((c.as_vec3() + 0.5 - middle) / radius).length_squared() <= 1.0 + 1e-4,
                    _ if bevel => (0..3).filter(|&a| size[a] >= 3 && (c[a] == lo[a] || c[a] == hi[a])).count() < 2,
                    _ => true,
                };
                if keep {
                    cells.push(c);
                }
            }
        }
    }
    cells
}

/// Where a ray meets the grid: the first filled cell (z -1: the floor
/// under the grid) and the face it enters through, as the way out of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub cell: IVec3,
    pub normal: IVec3,
}

/// The first cell of a grid `size` big that a ray meets (in cells: from
/// `origin` along `dir`), counting the floor one cell under the grid as
/// filled; Amanatides and Woo's walk.
pub fn pick(origin: Vec3, dir: Vec3, size: IVec3, filled: impl Fn(IVec3) -> bool) -> Option<Hit> {
    let lo = Vec3::new(0.0, 0.0, -1.0);
    let hi = size.as_vec3();
    // Into the box (the grid and its floor).
    let (mut enter, mut leave, mut normal) = (0.0f32, f32::INFINITY, IVec3::ZERO);
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            if origin[a] < lo[a] || origin[a] >= hi[a] {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo[a] - origin[a]) / dir[a], (hi[a] - origin[a]) / dir[a]);
        let (near, far) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
        if near > enter {
            enter = near;
            normal = IVec3::ZERO;
            normal[a] = if dir[a] > 0.0 { -1 } else { 1 };
        }
        leave = leave.min(far);
    }
    if enter > leave {
        return None;
    }
    let start = origin + dir * (enter + 1e-4);
    let mut cell = start.floor().as_ivec3().clamp(lo.as_ivec3(), size - 1);
    let step = dir.signum().as_ivec3();
    let mut next = Vec3::ZERO;
    let mut delta = Vec3::ZERO;
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            next[a] = f32::INFINITY;
            delta[a] = f32::INFINITY;
        } else {
            let edge = if dir[a] > 0.0 { cell[a] + 1 } else { cell[a] } as f32;
            next[a] = (edge - origin[a]) / dir[a];
            delta[a] = 1.0 / dir[a].abs();
        }
    }
    let inside = |c: IVec3| c.x >= 0 && c.y >= 0 && c.z >= -1 && c.x < size.x && c.y < size.y && c.z < size.z;
    while inside(cell) {
        if cell.z == -1 || filled(cell) {
            return Some(Hit { cell, normal });
        }
        let a = if next.x < next.y && next.x < next.z { 0 } else if next.y < next.z { 1 } else { 2 };
        cell[a] += step[a];
        next[a] += delta[a];
        normal = IVec3::ZERO;
        normal[a] = -step[a];
    }
    None
}

/// A game's tileset as whole tiles (`blocks/tiles.png`, tile `id` at
/// place `id - 1`): what new blocks are drawn out of.
pub struct Sheet {
    pub n: i32,
    pub columns: i32,
    pub count: i32,
    width: i32,
    pixels: Vec<[u8; 4]>,
}

impl Sheet {
    pub fn load(path: &Path, n: i32) -> Result<Self> {
        let (w, h, pixels) = crate::map::read_png_rgba(path)?;
        let (columns, rows) = (w as i32 / n, h as i32 / n);
        Ok(Self { n, columns, count: columns * rows, width: w as i32, pixels })
    }

    /// Tile `id`'s picture, rows from the top (None: no such tile, or one
    /// with nothing showing).
    pub fn tile(&self, id: i32) -> Option<Vec<[u8; 4]>> {
        if id < 1 || id > self.count {
            return None;
        }
        let n = self.n;
        let (x0, y0) = ((id - 1) % self.columns * n, (id - 1) / self.columns * n);
        let picture: Vec<[u8; 4]> = (0..n * n).map(|i| self.pixels[(x0 + i % n + (y0 + i / n) * self.width) as usize]).collect();
        picture.iter().any(|p| p[3] >= 128).then_some(picture)
    }
}

/// The level's cells, blocks and things as a step back brings them back.
#[derive(Clone, PartialEq)]
struct LevelState {
    size: IVec3,
    cells: Vec<Cell>,
    table: Vec<BlockEntry>,
    things: Vec<Thing>,
}

/// One step of the history, before and after.
enum Step {
    Level(LevelState, LevelState),
    Block(usize, Block, Block),
    Delete(Box<Deletion>),
    /// A model's frames, all of them (a stroke on one, or frames copied,
    /// taken out, turned, the box grown or trimmed).
    Model(Vec<Block>, Vec<Block>),
}

/// A block taken out of the level: its place in the list, what it was,
/// and the level before and after (its cells empty, the later blocks'
/// numbers one less).
struct Deletion {
    index: usize,
    entry: BlockEntry,
    block: Block,
    before: LevelState,
    after: LevelState,
}

/// The editor: a game's level of blocks and what is being done to it.
pub struct Editor {
    pub dir: PathBuf,
    /// The level's file, in the game folder.
    pub file: String,
    pub level: GridLevel,
    /// The level's blocks as made, as it lists them (and some it no longer
    /// lists, after a step back over a new one).
    pub blocks: Vec<Block>,
    pub sheet: Option<Sheet>,
    pub scope: Scope,
    /// What the level's tools work on.
    pub layer: Layer,
    pub tool: Tool,
    /// The tool to go back to after PICK.
    before_pick: Tool,
    /// The block the level's tools place (its number, 1 on), as it lies.
    pub held: Cell,
    /// The colour a block's tools lay.
    pub colour: [u8; 3],
    /// The kinds of things the game has (`c.things`), and the one PLACE
    /// puts down.
    pub kinds: Vec<String>,
    pub held_kind: Option<String>,
    /// The thing MOVE drags (its place in the level's list).
    moving: Option<usize>,
    past: Vec<Step>,
    future: Vec<Step>,
    /// The stroke going on: how the level or block stood when it began.
    stroke: Option<Step>,
    /// What went wrong last (saving), to show.
    pub message: Option<String>,
    /// The model shaped (an editor opened on a model).
    pub model: Option<ModelFile>,
    /// How far the model's voxels moved in the stroke going on (its box
    /// grown on the low sides), and since the view last followed them.
    grown: IVec3,
    pub moved: IVec3,
    /// BOX and SPHERE: the footprint being dragged out, the shape held on
    /// the mouse, the depth a new footprint gets, edges cut back (BEVEL).
    pub drawing: Option<Drawing>,
    pub held_form: Option<Held>,
    pub depth: i32,
    pub bevel: bool,
    /// Every tool's voxel mirrored left to right about the box's middle
    /// too (SYMMETRY): a figure's two sides alike.
    pub symmetry: bool,
}

impl Editor {
    /// Opens the level `file` of the game in `dir`, with its blocks and
    /// tileset.
    pub fn open(dir: &Path, file: &str) -> Result<Self> {
        let level = GridLevel::read(&dir.join(file))?;
        let blocks = level.load_blocks(dir)?;
        let sheet_path = dir.join(blocks::FOLDER).join(crate::tiled::SHEET);
        let sheet = if sheet_path.exists() { Some(Sheet::load(&sheet_path, level.n)?) } else { None };
        let held = Cell { block: (!level.blocks.is_empty()) as u16, turn: Turn::NONE };
        let colour = blocks.first().and_then(|b| b.colours().first().copied()).unwrap_or([200, 200, 200]);
        Ok(Self {
            dir: dir.to_path_buf(),
            file: file.to_string(),
            level,
            blocks,
            sheet,
            scope: Scope::Level,
            layer: Layer::Blocks,
            tool: Tool::Add,
            before_pick: Tool::Add,
            held,
            colour,
            kinds: Vec::new(),
            held_kind: None,
            moving: None,
            past: Vec::new(),
            future: Vec::new(),
            stroke: None,
            message: None,
            model: None,
            grown: IVec3::ZERO,
            moved: IVec3::ZERO,
            drawing: None,
            held_form: None,
            depth: 1,
            bevel: false,
            symmetry: false,
        })
    }

    /// Opens the model `file` of the game in `dir`, at its first frame.
    pub fn open_model(dir: &Path, file: &str) -> Result<Self> {
        let model = ModelFile::load(&dir.join(file))?;
        let level = GridLevel::new(1, IVec3::ZERO);
        let colour = model.colours().first().copied().unwrap_or([200, 200, 200]);
        Ok(Self {
            dir: dir.to_path_buf(),
            file: file.to_string(),
            level,
            blocks: Vec::new(),
            sheet: None,
            scope: Scope::Frame(0),
            layer: Layer::Blocks,
            tool: Tool::Add,
            before_pick: Tool::Add,
            held: Cell::default(),
            colour,
            kinds: Vec::new(),
            held_kind: None,
            moving: None,
            past: Vec::new(),
            future: Vec::new(),
            stroke: None,
            message: None,
            model: Some(model),
            grown: IVec3::ZERO,
            moved: IVec3::ZERO,
            drawing: None,
            held_form: None,
            depth: 1,
            bevel: false,
            symmetry: false,
        })
    }

    /// The box of voxels shaped: the open block, or the model's frame.
    pub fn shape(&self) -> Option<&Block> {
        match self.scope {
            Scope::Level => None,
            Scope::Block(i) => self.blocks.get(i),
            Scope::Frame(f) => self.model.as_ref().and_then(|m| m.frames.get(f)),
        }
    }

    fn shape_mut(&mut self) -> Option<&mut Block> {
        match self.scope {
            Scope::Level => None,
            Scope::Block(i) => self.blocks.get_mut(i),
            Scope::Frame(f) => self.model.as_mut().and_then(|m| m.frames.get_mut(f)),
        }
    }

    /// The model's frames (none without a model).
    pub fn frames(&self) -> &[Block] {
        self.model.as_ref().map_or(&[], |m| &m.frames)
    }

    /// The size of the grid the tools work on: cells, or a block's voxels.
    pub fn grid(&self) -> IVec3 {
        match self.scope {
            Scope::Level => self.level.size,
            Scope::Block(_) => IVec3::splat(self.level.n),
            Scope::Frame(_) => self.shape().map_or(IVec3::ONE, |f| f.size),
        }
    }

    pub fn choose(&mut self, tool: Tool) {
        if tool == Tool::Pick && self.tool != Tool::Pick {
            self.before_pick = self.tool;
        }
        self.tool = tool;
    }

    /// The block shown (in a block), or the one held (in the level).
    pub fn block_index(&self) -> Option<usize> {
        match self.scope {
            Scope::Block(i) => Some(i),
            Scope::Frame(_) => None,
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
    fn filled(&self, c: IVec3, then: bool) -> bool {
        match (self.scope, then.then_some(self.stroke.as_ref()).flatten()) {
            (Scope::Level, Some(Step::Level(before, _))) => {
                let s = before.size;
                c.cmplt(s).all() && before.cells[(c.x + c.y * s.x + c.z * s.x * s.y) as usize].block != 0
            }
            (Scope::Level, _) => self.level.cell(c).block != 0,
            (Scope::Block(_), Some(Step::Block(_, before, _))) => before.get(c) != 0,
            // (The model as the stroke found it, moved as its box grew.)
            (Scope::Frame(f), Some(Step::Model(before, _))) => before.get(f).is_some_and(|b| b.get(c - self.grown) != 0),
            (Scope::Block(_) | Scope::Frame(_), _) => self.shape().is_some_and(|b| b.get(c) != 0),
        }
    }

    /// Where a ray (in the shown grid's cells) meets it: against the
    /// surface the stroke going on began on.
    pub fn aim(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        let size = match self.scope {
            // The level grows right, back and up: room for one more cell.
            Scope::Level => self.level.size + 1,
            Scope::Block(_) | Scope::Frame(_) => self.grid(),
        };
        pick(origin, dir, size, |c| c.cmpge(IVec3::ZERO).all() && self.filled(c, true))
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
        };
        (c.cmpge(low).all() && c.cmplt(limit).all()).then_some(c)
    }

    /// The mouse pressed at a hit: PICK takes up, the others begin a stroke.
    pub fn press(&mut self, hit: Hit) {
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
                self.tool = self.before_pick;
            }
            return;
        }
        self.stroke = Some(match self.scope {
            Scope::Level => Step::Level(self.level_state(), self.level_state()),
            Scope::Block(i) => Step::Block(i, self.blocks[i].clone(), self.blocks[i].clone()),
            Scope::Frame(_) => Step::Model(self.frames().to_vec(), Vec::new()),
        });
        self.grown = IVec3::ZERO;
        self.drag(hit);
    }

    /// The mouse dragged on to a hit, in a stroke.
    pub fn drag(&mut self, hit: Hit) {
        if self.stroke.is_none() {
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
    fn touch(&mut self, c: IVec3, normal: IVec3) {
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
    fn mirror(&self, c: IVec3) -> Option<IVec3> {
        if !self.symmetry || self.scope == Scope::Level {
            return None;
        }
        let m = IVec3::new(self.grid().x - 1 - c.x, c.y, c.z);
        (m != c).then_some(m)
    }

    /// Makes the model's box reach voxel `c` (beside it, by one), every
    /// frame's: how far its voxels moved (on the low sides), None if it
    /// cannot (a frame holds at most 256 voxels each way).
    fn grow_model(&mut self, c: IVec3) -> Option<IVec3> {
        if c.cmplt(IVec3::NEG_ONE).any() || c.cmpgt(self.grid()).any() {
            return None;
        }
        self.grow_model_to(c, c)
    }

    /// Makes the model's box hold `lo` to `hi`, every frame's: how far its
    /// voxels moved (on the low sides), None if it cannot (256 voxels each
    /// way at most).
    fn grow_model_to(&mut self, low: IVec3, high: IVec3) -> Option<IVec3> {
        let old = self.grid();
        let (lo, hi) = (low.min(IVec3::ZERO), (high + 1).max(old));
        if (hi - lo).cmpgt(IVec3::splat(256)).any() {
            return None;
        }
        if lo == IVec3::ZERO && hi == old {
            return Some(IVec3::ZERO);
        }
        let shift = -lo;
        let model = self.model.as_mut()?;
        for frame in &mut model.frames {
            let mut grown = Block::boxed((frame.size + shift).max(hi - lo));
            grown.palette = frame.palette.clone();
            for k in 0..frame.voxels.len() {
                let at = grown.index(frame.at(k) + shift).expect("inside the bigger box");
                grown.voxels[at] = frame.voxels[k];
            }
            *frame = grown;
        }
        self.grown += shift;
        self.moved += shift;
        Some(shift)
    }

    /// The mouse let go: the stroke, if it changed anything, is a step and
    /// is saved.
    pub fn release(&mut self) {
        self.moving = None;
        let Some(stroke) = self.stroke.take() else { return };
        let step = match stroke {
            // Strokes keep the level or a block; nothing is taken out.
            Step::Delete(_) => return,
            Step::Level(before, _) => {
                let after = self.level_state();
                if after == before {
                    return;
                }
                Step::Level(before, after)
            }
            Step::Block(i, before, _) => {
                if self.blocks[i] == before {
                    return;
                }
                Step::Block(i, before, self.blocks[i].clone())
            }
            Step::Model(before, _) => {
                if self.frames() == before.as_slice() {
                    return;
                }
                Step::Model(before, self.frames().to_vec())
            }
        };
        self.save(&step);
        self.past.push(step);
        self.future.clear();
    }

    /// Whether the tool is BOX or SPHERE (in a block or a model).
    pub fn forms(&self) -> bool {
        matches!(self.tool, Tool::Box | Tool::Sphere) && self.scope != Scope::Level
    }

    /// BOX or SPHERE pressed at a hit: a footprint begins on the face's
    /// plane, before it (or, to `cut`, in the voxel itself).
    pub fn begin_form(&mut self, hit: Hit, cut: bool) {
        let normal = way_out(hit);
        let start = if cut { hit.cell } else { hit.cell + normal };
        self.drawing = Some(Drawing { start, normal, end: start, cut });
    }

    /// The footprint dragged on to where a ray (in the grid's cells) meets
    /// its plane.
    pub fn draw_form(&mut self, origin: Vec3, dir: Vec3) {
        let Some(d) = &mut self.drawing else { return };
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

    /// Where the shape of BOX or SPHERE would go now, its low and high
    /// corner, the mouse at `hover` (`cut`: into the voxels): the footprint
    /// being dragged out (as deep as `depth`), the shape held where it was
    /// drawn, or standing at the face pointed at (its depth out of it,
    /// centred there across it).
    pub fn form_box(&self, hover: Option<Hit>, cut: bool) -> Option<(IVec3, IVec3)> {
        if let Some(d) = &self.drawing {
            let (a, way) = (axis_of(d.normal), deep_way(d.normal, d.cut));
            let (mut lo, mut hi) = (d.start.min(d.end), d.start.max(d.end));
            if way[a] > 0 {
                hi[a] = d.start[a] + self.depth.max(1) - 1;
            } else {
                lo[a] = d.start[a] - self.depth.max(1) + 1;
            }
            return Some((lo, hi));
        }
        let held = self.held_form?;
        if let Some((lo, _)) = held.at {
            return Some((lo, lo + held.size - 1));
        }
        let hit = hover?;
        let normal = way_out(hit);
        let c = if cut { hit.cell } else { hit.cell + normal };
        let (a, way) = (axis_of(normal), deep_way(normal, cut));
        let mut lo = c - (held.size - 1) / 2;
        lo[a] = if way[a] > 0 { c[a] } else { c[a] - held.size[a] + 1 };
        Some((lo, lo + held.size - 1))
    }

    /// The cells the shape would fill now (with SYMMETRY their mirrors
    /// too, about the box as it stands).
    pub fn form_cells_now(&self, hover: Option<Hit>, cut: bool) -> Vec<IVec3> {
        let Some((lo, hi)) = self.form_box(hover, cut) else { return Vec::new() };
        let mut cells = form_cells(self.tool, lo, hi, self.bevel);
        if self.symmetry {
            let mirrored: Vec<IVec3> = cells.iter().filter_map(|&c| self.mirror(c)).collect();
            let seen: std::collections::BTreeSet<[i32; 3]> = cells.iter().map(|c| c.to_array()).collect();
            cells.extend(mirrored.into_iter().filter(|m| !seen.contains(&m.to_array())));
        }
        cells
    }

    /// The footprint dragged out becomes the shape held, standing where it
    /// was drawn.
    pub fn hold_drawn(&mut self) {
        let Some((lo, hi)) = self.form_box(None, false) else { return };
        let Some(d) = self.drawing.take() else { return };
        self.held_form = Some(Held { size: hi - lo + 1, at: Some((lo, deep_way(d.normal, d.cut))) });
    }

    /// The shape held follows the mouse from now on.
    pub fn let_form_follow(&mut self) {
        if let Some(held) = &mut self.held_form {
            held.at = None;
        }
    }

    /// The shape a voxel deeper (`by` 1) or shallower (-1): the footprint
    /// being dragged out (and the next ones), the shape held along the way
    /// it was drawn while it stands there, else along the face pointed at.
    pub fn deepen(&mut self, by: i32, hover: Option<Hit>) {
        let Some(held) = self.held_form.as_mut().filter(|_| self.drawing.is_none()) else {
            self.depth = (self.depth + by).clamp(1, 256);
            return;
        };
        let (a, at) = match held.at {
            Some((lo, way)) => (axis_of(way), Some((lo, way))),
            None => match hover {
                Some(hit) => (axis_of(way_out(hit)), None),
                None => return,
            },
        };
        let deeper = (held.size[a] + by).clamp(1, 256);
        if let Some((mut lo, way)) = at {
            // Drawn into the voxels or down a face: it grows from its far end.
            if way[a] < 0 {
                lo[a] -= deeper - held.size[a];
            }
            held.at = Some((lo, way));
        }
        held.size[a] = deeper;
        self.depth = deeper;
    }

    /// The shape held let go (Escape): whether there was one.
    pub fn drop_form(&mut self) -> bool {
        let had = self.held_form.is_some() || self.drawing.is_some();
        (self.held_form, self.drawing) = (None, None);
        had
    }

    /// The shape set where `form_box` says: filling in the colour held
    /// (only where empty with `only_empty`), or cutting away; mirrored too
    /// with SYMMETRY. A model's box grows to hold it; a block's stays, what
    /// lies outside left out. One step, saved; the shape stays held.
    pub fn place_form(&mut self, hover: Option<Hit>, cut: bool, only_empty: bool) {
        let mut cells = self.form_cells_now(hover, cut);
        if cells.is_empty() || self.shape().is_none() {
            return;
        }
        self.stroke = Some(match self.scope {
            Scope::Level => return,
            Scope::Block(i) => Step::Block(i, self.blocks[i].clone(), self.blocks[i].clone()),
            Scope::Frame(_) => Step::Model(self.frames().to_vec(), Vec::new()),
        });
        self.grown = IVec3::ZERO;
        if matches!(self.scope, Scope::Frame(_)) && !cut {
            let (lo, hi) = cells.iter().fold((cells[0], cells[0]), |(lo, hi), &c| (lo.min(c), hi.max(c)));
            match self.grow_model_to(lo, hi) {
                Some(shift) => {
                    cells.iter_mut().for_each(|c| *c += shift);
                    if let Some(Held { at: Some((lo, _)), .. }) = &mut self.held_form {
                        *lo += shift;
                    }
                }
                None => self.message = Some("a model holds 256 voxels each way at most".into()),
            }
        }
        let colour = self.colour;
        let Some(block) = self.shape_mut() else { return };
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
    }

    /// Takes the last step back.
    pub fn undo(&mut self) {
        self.release();
        if let Some(step) = self.past.pop() {
            self.go(&step, false);
            self.future.push(step);
        }
    }

    /// Takes the last step taken back again.
    pub fn redo(&mut self) {
        self.release();
        if let Some(step) = self.future.pop() {
            self.go(&step, true);
            self.past.push(step);
        }
    }

    fn go(&mut self, step: &Step, forward: bool) {
        let mut problem = None;
        match step {
            Step::Level(before, after) => self.set_level(if forward { after } else { before }),
            Step::Block(i, before, after) => self.blocks[*i] = if forward { after } else { before }.clone(),
            Step::Model(before, after) => {
                if let Some(model) = &mut self.model {
                    model.frames = if forward { after } else { before }.clone();
                    if let Scope::Frame(f) = self.scope {
                        self.scope = Scope::Frame(f.min(model.frames.len() - 1));
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
        self.save(step);
        if problem.is_some() {
            self.message = problem;
        }
    }

    fn set_level(&mut self, s: &LevelState) {
        self.level.size = s.size;
        self.level.cells = s.cells.clone();
        self.level.blocks = s.table.clone();
        self.level.things = s.things.clone();
    }

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
        let mut after = before.clone();
        let entry = after.table.remove(i);
        let gone = i as u16 + 1;
        for c in &mut after.cells {
            if c.block == gone {
                *c = Cell::default();
            } else if c.block > gone {
                c.block -= 1;
            }
        }
        let step = Step::Delete(Box::new(Deletion { index: i, entry, block: self.blocks[i].clone(), before, after }));
        self.go(&step, true);
        self.past.push(step);
        self.future.clear();
    }

    /// How many steps there are to take back.
    pub fn steps(&self) -> usize {
        self.past.len()
    }

    /// Takes the last step back for good (a click that was the first of a
    /// double click).
    pub fn take_back(&mut self) {
        self.undo();
        self.future.pop();
    }

    /// The open block itself turned, its voxels with it (a side shaped as
    /// the front brought round); a step, saved.
    pub fn turn_block(&mut self, turn: Turn) {
        if let Scope::Frame(_) = self.scope {
            return self.change_model(|frames| frames.iter_mut().for_each(|f| *f = f.turned(turn)));
        }
        let Scope::Block(i) = self.scope else { return };
        self.release();
        let before = self.blocks[i].clone();
        self.blocks[i] = before.turned(turn);
        let step = Step::Block(i, before, self.blocks[i].clone());
        self.save(&step);
        self.past.push(step);
        self.future.clear();
    }

    /// The level's tools to its blocks or its things; a tool the other
    /// layer lacks becomes PLACE.
    pub fn switch(&mut self, layer: Layer) {
        self.release();
        self.layer = layer;
        if !Tool::of(self.scope, layer).contains(&self.tool) {
            self.tool = Tool::Add;
        }
    }

    /// Holds the game's kind `i` (its place in `kinds`) for PLACE.
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
            Tool::Pick => {
                if let Some(t) = under.and_then(|i| self.level.things.get(i)) {
                    self.held_kind = Some(t.kind.clone());
                    self.tool = self.before_pick;
                }
                return;
            }
            Tool::Add if spot.is_none() || self.held_kind.is_none() => return,
            Tool::Remove | Tool::Move if under.is_none() => return,
            _ => {}
        }
        self.stroke = Some(Step::Level(self.level_state(), self.level_state()));
        match self.tool {
            Tool::Add => {
                let (kind, at) = (self.held_kind.clone().unwrap(), spot.unwrap());
                self.level.things.push(Thing { kind, at: at.to_array() });
            }
            Tool::Remove => {
                self.level.things.remove(under.unwrap());
            }
            Tool::Move => self.moving = under,
            _ => {}
        }
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

    /// The held block turned (TURN, ROLL, MIRROR).
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

    pub fn back_to_level(&mut self) {
        self.release();
        if let Scope::Frame(_) = self.scope {
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
        self.save(&step);
        self.past.push(step);
        self.future.clear();
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
    fn free_name(&self) -> String {
        let taken = self.level.blocks.iter().filter_map(|b| b.name.parse::<i32>().ok()).max().unwrap_or(0);
        let mut k = taken.max(self.sheet.as_ref().map_or(0, |s| s.count)) + 1;
        while blocks::path(&self.dir, &k.to_string()).exists() {
            k += 1;
        }
        k.to_string()
    }

    /// A block saved and added to the level's list, held; a step.
    fn add_block(&mut self, name: String, block: Block, kind: Kind) -> Result<()> {
        block.save(&blocks::path(&self.dir, &name))?;
        let before = self.level_state();
        self.blocks.truncate(self.level.blocks.len());
        self.blocks.push(block);
        self.level.blocks.push(BlockEntry { name, kind });
        let step = Step::Level(before, self.level_state());
        self.save(&step);
        self.past.push(step);
        self.future.clear();
        self.hold(self.level.blocks.len() - 1);
        Ok(())
    }

    /// Makes the grid reach cell `c` (right, back or up of it, by one):
    /// whether it does.
    fn grow(&mut self, c: IVec3) -> bool {
        let old = self.level.size;
        if c.cmplt(IVec3::ZERO).any() || c.cmpgt(old).any() {
            return false;
        }
        let size = old.max(c + 1);
        let mut cells = vec![Cell::default(); (size.x * size.y * size.z) as usize];
        for z in 0..old.z {
            for y in 0..old.y {
                for x in 0..old.x {
                    cells[(x + y * size.x + z * size.x * size.y) as usize] = self.level.cells[(x + y * old.x + z * old.x * old.y) as usize];
                }
            }
        }
        self.level.size = size;
        self.level.cells = cells;
        true
    }

    fn level_state(&self) -> LevelState {
        LevelState { size: self.level.size, cells: self.level.cells.clone(), table: self.level.blocks.clone(), things: self.level.things.clone() }
    }

    /// The next frame of the model shown (`by` on, back when negative,
    /// round at the ends).
    pub fn step_frame(&mut self, by: i32) {
        let (Scope::Frame(f), count) = (self.scope, self.frames().len()) else { return };
        self.release();
        self.scope = Scope::Frame((f as i32 + by).rem_euclid(count.max(1) as i32) as usize);
    }

    /// A copy of the frame shown after it, shown; a step.
    pub fn copy_frame(&mut self) {
        let Scope::Frame(f) = self.scope else { return };
        self.change_model(|frames| frames.insert(f + 1, frames[f].clone()));
        self.scope = Scope::Frame(f + 1);
    }

    /// The frame shown taken out (not the last one); a step.
    pub fn delete_frame(&mut self) {
        let Scope::Frame(f) = self.scope else { return };
        if self.frames().len() < 2 {
            return;
        }
        self.change_model(|frames| {
            frames.remove(f);
        });
        self.scope = Scope::Frame(f.min(self.frames().len() - 1));
    }

    /// The model's box cut down to its voxels, every frame's alike (the
    /// frames keep their places to each other); a step.
    pub fn trim(&mut self) {
        let mut bounds: Option<(IVec3, IVec3)> = None;
        for frame in self.frames() {
            for k in (0..frame.voxels.len()).filter(|&k| frame.voxels[k] != 0) {
                let p = frame.at(k);
                bounds = Some(bounds.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
            }
        }
        let Some((lo, hi)) = bounds else { return };
        self.moved -= lo;
        self.change_model(|frames| {
            for frame in frames.iter_mut() {
                let mut cut = Block::boxed(hi - lo + 1);
                cut.palette = frame.palette.clone();
                for k in 0..cut.voxels.len() {
                    cut.voxels[k] = frame.get(cut.at(k) + lo);
                }
                *frame = cut;
            }
        });
    }

    /// A change to the model's frames as a step, saved (nothing if it
    /// changes nothing).
    fn change_model(&mut self, change: impl FnOnce(&mut Vec<Block>)) {
        self.release();
        let Some(model) = &mut self.model else { return };
        let before = model.frames.clone();
        change(&mut model.frames);
        if model.frames == before {
            return;
        }
        let step = Step::Model(before, model.frames.clone());
        self.save(&step);
        self.past.push(step);
        self.future.clear();
    }

    /// Saves what a step changed: the level, the block, or the model.
    fn save(&mut self, step: &Step) {
        let done = match step {
            Step::Level(..) | Step::Delete(..) => self.level.save(&self.dir.join(&self.file)),
            Step::Block(i, ..) => self.blocks[*i].save(&blocks::path(&self.dir, &self.level.blocks[*i].name)),
            Step::Model(..) => self.model.as_ref().map_or(Ok(()), ModelFile::save),
        };
        self.message = done.err().map(|e| format!("{e:#}"));
    }
}

/// The level file a game's config names (`c.world.blocks.file`).
pub fn level_file(cfg: &crate::config::GameConfig) -> Result<String> {
    match &cfg.world.blocks {
        Some(b) => Ok(b.file.clone()),
        None => bail!("this game's level is not made of blocks (c.world.blocks; `voxcade import` brings a Tiled map in)"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const N: i32 = 4;

    /// A game folder with a 3 by 2 by 2 level of one block (a solid cube,
    /// grey) in cell (0, 0, 0), and a tileset of two tiles (1 red, 2
    /// nothing).
    pub(crate) fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("voxcade-editor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut level = GridLevel::new(N, IVec3::new(3, 2, 2));
        level.blocks.push(BlockEntry { name: "1".into(), kind: Kind::Solid });
        level.cells[0] = Cell { block: 1, turn: Turn::NONE };
        level.save(&dir.join("level.json")).unwrap();
        let mut cube = Block::empty(N);
        for i in 0..N * N * N {
            cube.set(IVec3::new(i % N, i / N % N, i / (N * N)), Some([128, 128, 128])).unwrap();
        }
        cube.save(&blocks::path(&dir, "1")).unwrap();
        // Two tiles side by side: rows of 2N pixels.
        let mut rgba = Vec::new();
        for _ in 0..N {
            for x in 0..2 * N {
                rgba.extend(if x < N { [200, 0, 0, 255] } else { [0; 4] });
            }
        }
        crate::headless::write_png(&dir.join("blocks").join(crate::tiled::SHEET), (2 * N) as u32, N as u32, &rgba).unwrap();
        dir
    }

    /// A ray straight down onto cell (x, y) of the grid, from high above.
    fn down(x: i32, y: i32) -> (Vec3, Vec3) {
        (Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 50.0), Vec3::NEG_Z)
    }

    #[test]
    fn a_ray_finds_the_first_cell_and_the_face_it_enters() {
        let size = IVec3::new(4, 4, 4);
        let filled = |c: IVec3| c == IVec3::new(2, 1, 1) || c == IVec3::new(3, 1, 1);
        // From the front (-y), level with the cell: its front face.
        let hit = pick(Vec3::new(2.5, -5.0, 1.5), Vec3::Y, size, filled).unwrap();
        assert_eq!(hit, Hit { cell: IVec3::new(2, 1, 1), normal: IVec3::NEG_Y });
        // From the left, slanting down: the first of the two, its left face.
        let hit = pick(Vec3::new(-3.0, 1.5, 2.2), Vec3::new(1.0, 0.0, -0.2).normalize(), size, filled).unwrap();
        assert_eq!(hit, Hit { cell: IVec3::new(2, 1, 1), normal: IVec3::NEG_X });
        // From above: its top.
        let (o, d) = down(3, 1);
        assert_eq!(pick(o, d, size, filled).unwrap(), Hit { cell: IVec3::new(3, 1, 1), normal: IVec3::Z });
        // Past the grid: nothing.
        assert_eq!(pick(Vec3::new(-3.0, 9.0, 2.0), Vec3::X, size, filled), None);
    }

    #[test]
    fn a_ray_over_nothing_lands_on_the_floor() {
        let (o, d) = down(1, 2);
        let hit = pick(o, d, IVec3::new(4, 4, 4), |_| false).unwrap();
        assert_eq!(hit, Hit { cell: IVec3::new(1, 2, -1), normal: IVec3::Z });
    }

    #[test]
    fn placing_turning_and_removing_blocks() {
        let dir = folder("place");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        // Onto the block's top: a block in the cell above.
        let (o, d) = down(0, 0);
        let hit = ed.aim(o, d).unwrap();
        assert_eq!(hit, Hit { cell: IVec3::ZERO, normal: IVec3::Z });
        ed.turn(Turn::TURN);
        ed.press(hit);
        ed.release();
        assert_eq!(ed.level.cell(IVec3::new(0, 0, 1)), Cell { block: 1, turn: Turn::TURN });
        // On the floor beside it.
        let (o, d) = down(2, 1);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        assert_eq!(ed.level.cell(IVec3::new(2, 1, 0)).block, 1);
        // Further right than the grid reaches: it grows by a column.
        let hit = ed.aim(Vec3::new(9.0, 1.5, 0.5), Vec3::NEG_X).unwrap();
        assert_eq!(hit, Hit { cell: IVec3::new(2, 1, 0), normal: IVec3::X });
        ed.press(hit);
        ed.release();
        assert_eq!(ed.level.size, IVec3::new(4, 2, 2));
        assert_eq!(ed.level.cell(IVec3::new(3, 1, 0)).block, 1);
        assert_eq!(ed.level.cell(IVec3::ZERO).block, 1, "the cells kept their places");
        // Removed.
        ed.choose(Tool::Remove);
        let (o, d) = down(0, 0);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        assert_eq!(ed.level.cell(IVec3::new(0, 0, 1)).block, 0);
        assert_eq!(GridLevel::read(&dir.join("level.json")).unwrap(), ed.level);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn painting_recolours_and_adding_grows_from_the_face() {
        let dir = folder("paint");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.open_block(0);
        assert_eq!(ed.colour, [128, 128, 128]);
        ed.colour = [10, 200, 10];
        ed.choose(Tool::Paint);
        let (o, d) = down(1, 2);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        assert_eq!(ed.blocks[0].colour_at(IVec3::new(1, 2, N - 1)), Some([10, 200, 10]));
        ed.choose(Tool::Add);
        let hit = ed.aim(Vec3::new(1.5, -9.0, 0.5), Vec3::Y).unwrap();
        ed.press(hit);
        ed.release();
        assert_eq!(hit.normal, IVec3::NEG_Y);
        assert_eq!(ed.blocks[0].get(IVec3::new(1, -1, 0)), 0, "nothing before the block: it stays N each way");
        ed.choose(Tool::Remove);
        ed.press(hit);
        ed.release();
        assert_eq!(ed.blocks[0].get(IVec3::new(1, 0, 0)), 0);
        ed.choose(Tool::Add);
        ed.press(ed.aim(Vec3::new(1.5, -9.0, 0.5), Vec3::Y).unwrap());
        ed.release();
        assert_eq!(ed.blocks[0].colour_at(IVec3::new(1, 0, 0)), Some([10, 200, 10]), "added back before the face behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deep_paint_colours_every_voxel_behind() {
        let dir = folder("deep");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.open_block(0);
        // A hole in the column, which stays a hole.
        ed.blocks[0].set(IVec3::new(1, 2, 2), None).unwrap();
        ed.colour = [250, 20, 20];
        ed.choose(Tool::Deep);
        ed.press(ed.aim(Vec3::new(1.5, -9.0, 2.5), Vec3::Y).unwrap());
        ed.release();
        let red = Some([250, 20, 20]);
        for y in 0..N {
            let want = if y == 2 { None } else { red };
            assert_eq!(ed.blocks[0].colour_at(IVec3::new(1, y, 2)), want, "through the depth, at y {y}");
        }
        assert_eq!(ed.blocks[0].colour_at(IVec3::new(2, 1, 2)), Some([128, 128, 128]), "the next column stays");
        // From above: down through the block.
        let (o, d) = down(3, 3);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        assert!((0..N).all(|z| ed.blocks[0].colour_at(IVec3::new(3, 3, z)) == red));
        assert_eq!(Block::load(&blocks::path(&dir, "1"), N).unwrap(), ed.blocks[0], "saved");
        ed.back_to_level();
        assert_eq!(ed.tool, Tool::Paint, "the level has no deep paint");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn turning_a_block_turns_its_voxels() {
        let dir = folder("turn-block");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.open_block(0);
        ed.blocks[0].set(IVec3::new(0, N - 1, 0), Some([1, 2, 3])).unwrap();
        let made = ed.blocks[0].clone();
        ed.turn_block(Turn::TURN);
        ed.turn_block(Turn::TURN);
        assert_eq!(ed.blocks[0].colour_at(IVec3::new(N - 1, 0, 0)), Some([1, 2, 3]), "the back left corner comes round to the front right");
        assert_eq!(Block::load(&blocks::path(&dir, "1"), N).unwrap(), ed.blocks[0], "saved");
        ed.undo();
        ed.undo();
        assert_eq!(ed.blocks[0], made);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cloning_a_block_makes_a_copy_to_shape() {
        let dir = folder("clone");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.set_kind(Kind::Oneway);
        ed.clone_block().unwrap();
        assert_eq!(ed.level.blocks[1], BlockEntry { name: "3".into(), kind: Kind::Oneway });
        assert_eq!((ed.held.block, ed.scope), (2, Scope::Level), "the copy is held");
        assert_eq!(Block::load(&blocks::path(&dir, "3"), N).unwrap(), ed.blocks[0]);
        // In a block: the copy opens, to shape instead of the first.
        ed.open_block(0);
        ed.clone_block().unwrap();
        assert_eq!(ed.scope, Scope::Block(2));
        assert_eq!(ed.level.blocks[2].name, "4");
        assert_eq!(GridLevel::read(&dir.join("level.json")).unwrap().blocks.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_a_block_empties_its_cells_and_undo_brings_it_back() {
        let dir = folder("delete");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.clone_block().unwrap();
        // The copy (block 2) in a cell beside the first.
        let (o, d) = down(1, 0);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        assert_eq!(ed.level.cell(IVec3::new(1, 0, 0)).block, 2);
        let start = ed.level.clone();
        ed.hold(0);
        ed.delete_block();
        assert_eq!(ed.level.blocks.len(), 1);
        assert_eq!(ed.level.cell(IVec3::ZERO).block, 0, "its cell is empty");
        assert_eq!(ed.level.cell(IVec3::new(1, 0, 0)).block, 1, "the copy is block 1 now");
        assert!(!blocks::path(&dir, "1").exists(), "its file is gone");
        assert_eq!(GridLevel::read(&dir.join("level.json")).unwrap(), ed.level);
        ed.undo();
        assert_eq!(ed.level, start);
        assert_eq!(Block::load(&blocks::path(&dir, "1"), N).unwrap(), ed.blocks[0], "its file is back");
        ed.redo();
        assert_eq!(ed.level.blocks[0].name, "3");
        // Deleting the open block goes back to the level.
        ed.open_block(0);
        ed.delete_block();
        assert_eq!((ed.scope, ed.level.blocks.len(), ed.held.block), (Scope::Level, 0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stroke_works_on_the_surface_it_began_on() {
        let dir = folder("stroke");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.open_block(0);
        ed.choose(Tool::Remove);
        // Dragged twice over the same voxel from above: one layer off.
        let (o, d) = down(2, 2);
        ed.press(ed.aim(o, d).unwrap());
        ed.drag(ed.aim(o, d).unwrap());
        ed.drag(ed.aim(o, d).unwrap());
        ed.release();
        assert_eq!(ed.blocks[0].get(IVec3::new(2, 2, N - 1)), 0);
        assert_ne!(ed.blocks[0].get(IVec3::new(2, 2, N - 2)), 0, "the stroke did not dig in");
        // Placing dragged over the same cell: one block, not a tower.
        ed.back_to_level();
        ed.choose(Tool::Add);
        let (o, d) = down(0, 0);
        ed.press(ed.aim(o, d).unwrap());
        ed.drag(ed.aim(o, d).unwrap());
        ed.drag(ed.aim(o, d).unwrap());
        ed.release();
        assert_eq!(ed.level.cell(IVec3::new(0, 0, 1)).block, 1);
        assert_eq!(ed.level.size.z, 2, "the stroke did not grow upwards");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_and_redo_step_through_both_levels() {
        let dir = folder("undo");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        let start = (ed.level.clone(), ed.blocks[0].clone());
        let (o, d) = down(1, 0);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        ed.open_block(0);
        ed.choose(Tool::Remove);
        let (o, d) = down(0, 0);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        let end = (ed.level.clone(), ed.blocks[0].clone());
        ed.undo();
        assert_eq!(ed.blocks[0], start.1);
        assert_ne!(ed.level, start.0);
        ed.undo();
        assert_eq!((ed.level.clone(), ed.blocks[0].clone()), start);
        ed.undo();
        assert_eq!((ed.level.clone(), ed.blocks[0].clone()), start, "nothing further back");
        ed.redo();
        ed.redo();
        assert_eq!((ed.level.clone(), ed.blocks[0].clone()), end);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_change_is_saved_at_once() {
        let dir = folder("saved");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.open_block(0);
        ed.choose(Tool::Remove);
        let (o, d) = down(3, 3);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
        let on_disk = || Block::load(&blocks::path(&dir, "1"), N).unwrap();
        assert_eq!(on_disk(), ed.blocks[0]);
        ed.undo();
        assert_eq!(on_disk(), ed.blocks[0]);
        assert_ne!(on_disk().get(IVec3::new(3, 3, N - 1)), 0);
        let names: Vec<_> = std::fs::read_dir(dir.join("blocks")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        assert!(names.iter().all(|n| !n.starts_with('.')), "no file half written: {names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_block_is_drawn_out_from_its_tile() {
        let dir = folder("new");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        assert!(ed.sheet.as_ref().unwrap().tile(2).is_none(), "an empty tile");
        ed.new_block(Some(1)).unwrap();
        assert_eq!(ed.held.block, 1, "the level has tile 1's block: it is held");
        let e = ed.new_block(Some(2)).unwrap_err().to_string();
        assert!(e.contains("no tile 2"), "{e}");
        ed.new_block(None).unwrap();
        assert_eq!(ed.level.blocks.len(), 2);
        assert_eq!(ed.level.blocks[1].name, "3", "numbered on after the tiles");
        assert_eq!(ed.held.block, 2);
        assert!(blocks::path(&dir, "3").exists());
        assert_eq!(GridLevel::read(&dir.join("level.json")).unwrap().blocks.len(), 2);
        // Without its own block, a tile's is drawn out of the tileset.
        std::fs::remove_file(blocks::path(&dir, "1")).unwrap();
        ed.level.blocks[0].name = "old".into();
        ed.new_block(Some(1)).unwrap();
        assert_eq!(ed.blocks[2].colour_at(IVec3::new(0, N - 1, 0)), Some([200, 0, 0]));
        ed.undo();
        assert_eq!(ed.level.blocks.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn changing_a_kind_is_saved_with_the_level() {
        let dir = folder("kind");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        assert_eq!(ed.kind(), Some(Kind::Solid));
        ed.set_kind(Kind::Oneway);
        assert_eq!(GridLevel::read(&dir.join("level.json")).unwrap().blocks[0].kind, Kind::Oneway);
        ed.undo();
        assert_eq!(GridLevel::read(&dir.join("level.json")).unwrap().blocks[0].kind, Kind::Solid);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picking_takes_up_and_goes_back_to_the_tool() {
        let dir = folder("pick");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.held = Cell::default();
        ed.choose(Tool::Remove);
        ed.choose(Tool::Pick);
        let (o, d) = down(0, 0);
        ed.press(ed.aim(o, d).unwrap());
        assert_eq!((ed.held.block, ed.tool), (1, Tool::Remove));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Things are put down, moved and taken away, each one step back, and
    /// the level saved with them at once; PICK takes up a thing's kind.
    #[test]
    fn placing_moving_and_removing_things() {
        let dir = folder("things");
        let mut ed = Editor::open(&dir, "level.json").unwrap();
        ed.kinds = vec!["fox".into(), "gem".into()];
        ed.hold_kind(1);
        ed.switch(Layer::Things);
        let saved = || GridLevel::read(&dir.join("level.json")).unwrap().things;
        let gem = |at: [i32; 3]| vec![Thing { kind: "gem".into(), at }];
        ed.press_thing(Some(IVec3::new(2, 2, 8)), None);
        ed.release();
        assert_eq!((&ed.level.things, saved()), (&gem([2, 2, 8]), gem([2, 2, 8])));
        // Pressed over nothing, REMOVE and MOVE do nothing.
        for tool in [Tool::Remove, Tool::Move] {
            ed.choose(tool);
            ed.press_thing(Some(IVec3::new(5, 2, 8)), None);
            ed.release();
        }
        assert_eq!(ed.steps(), 1);
        ed.choose(Tool::Move);
        ed.press_thing(Some(IVec3::new(2, 2, 8)), Some(0));
        ed.drag_thing(Some(IVec3::new(6, 2, 8)));
        ed.drag_thing(Some(IVec3::new(7, 6, 4)));
        ed.release();
        assert_eq!((ed.steps(), saved()), (2, gem([7, 6, 4])), "a drag is one step");
        ed.hold_kind(0);
        ed.choose(Tool::Pick);
        ed.press_thing(None, Some(0));
        assert_eq!((ed.held_kind.as_deref(), ed.tool), (Some("gem"), Tool::Move));
        ed.choose(Tool::Remove);
        ed.press_thing(None, Some(0));
        ed.release();
        assert!(ed.level.things.is_empty() && saved().is_empty());
        ed.undo();
        assert_eq!(saved(), gem([7, 6, 4]));
        ed.undo();
        assert_eq!(saved(), gem([2, 2, 8]));
        ed.redo();
        assert_eq!(ed.level.things, gem([7, 6, 4]));
        // Back to the blocks, a tool they lack becomes PLACE.
        ed.choose(Tool::Move);
        ed.switch(Layer::Blocks);
        assert_eq!(ed.tool, Tool::Add);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A thing stands before the face the mouse points at: on a top, or
    /// before a wall, in the middle of its row's depth (the lane); on the
    /// floor in front of nothing; never off the level's sides.
    #[test]
    fn a_thing_stands_before_the_face_it_is_put_on() {
        let dir = folder("spot");
        let ed = Editor::open(&dir, "level.json").unwrap();
        let spot = |voxel: [i32; 3], normal: [i32; 3]| ed.thing_spot(IVec3::from(voxel), IVec3::from(normal)).map(|s| s.to_array());
        assert_eq!(spot([1, 1, 3], [0, 0, 1]), Some([1, 2, 4]), "on a top, in the front row's middle");
        assert_eq!(spot([5, 4, 2], [0, -1, 0]), Some([5, 2, 2]), "before a wall of the back row: in the front row");
        assert_eq!(spot([3, 0, 2], [0, -1, 0]), Some([3, 2, 2]), "before the front: still in the front row");
        assert_eq!(spot([5, 6, -1], [0, 0, 1]), Some([5, 6, 0]), "on the floor of the back row");
        assert_eq!(spot([11, 1, 3], [1, 0, 0]), None, "off the side");
        assert_eq!(spot([1, 1, 3], [0, 0, 0]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A model of two frames 3 by 2 by 5, a glass colour among its
    /// colours.
    fn model_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("voxcade-editor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let frame = |top: u8| dot_vox::Model {
            size: dot_vox::Size { x: 3, y: 2, z: 5 },
            voxels: (0..5).map(|z| dot_vox::Voxel { x: 1, y: 0, z, i: if z == 4 { top } else { 0 } }).collect(),
        };
        let mut palette = vec![dot_vox::Color { r: 0, g: 0, b: 0, a: 255 }; 256];
        palette[0] = dot_vox::Color { r: 200, g: 40, b: 40, a: 255 };
        palette[1] = dot_vox::Color { r: 120, g: 200, b: 255, a: 255 };
        let glass = dot_vox::Material { id: 2, properties: [("_type".to_string(), "_glass".to_string()), ("_trans".to_string(), "0.5".to_string())].into() };
        let data = dot_vox::DotVoxData { version: 150, index_map: Vec::new(), models: vec![frame(1), frame(0)], palette, materials: vec![glass], scenes: Vec::new(), layers: Vec::new() };
        let mut bytes = Vec::new();
        data.write_vox(&mut bytes).unwrap();
        std::fs::write(dir.join("thing.vox"), bytes).unwrap();
        dir
    }

    /// BOX's and SPHERE's shapes: a box of 3 with its edges cut back is a
    /// cross, a sphere of 3 a cube without its corners; flat ones keep
    /// their middle.
    #[test]
    fn boxes_and_spheres_are_clean_shapes() {
        let (lo, hi) = (IVec3::ZERO, IVec3::splat(2));
        assert_eq!(form_cells(Tool::Box, lo, hi, false).len(), 27);
        assert_eq!(form_cells(Tool::Box, lo, hi, true).len(), 7, "the edges and corners cut");
        assert_eq!(form_cells(Tool::Sphere, lo, hi, false).len(), 19, "the corners off");
        assert_eq!(form_cells(Tool::Sphere, lo, IVec3::new(4, 4, 0), false).len(), 21, "a disc");
        assert_eq!(form_cells(Tool::Box, lo, IVec3::new(4, 0, 0), true).len(), 5, "a bar keeps its ends");
    }

    /// BOX on a model: a footprint dragged out on the top face, a voxel
    /// deeper, held and set with a click; the box grows to hold it; with
    /// SYMMETRY the other side too; Shift fills only where empty, Ctrl
    /// cuts; each a step.
    #[test]
    fn a_box_is_drawn_held_and_set() {
        let dir = model_folder("box");
        let mut ed = Editor::open_model(&dir, "thing.vox").unwrap();
        ed.choose(Tool::Box);
        ed.colour = [10, 200, 10];
        // On top of the voxel at (1, 0, 4), dragged to x 3, two deep.
        ed.begin_form(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z }, false);
        ed.draw_form(Vec3::new(3.5, 0.5, 30.0), Vec3::NEG_Z);
        ed.deepen(1, None);
        assert_eq!(ed.form_box(None, false), Some((IVec3::new(1, 0, 5), IVec3::new(3, 0, 6))));
        ed.hold_drawn();
        assert_eq!(ed.held_form.map(|h| h.size), Some(IVec3::new(3, 1, 2)));
        ed.place_form(None, false, false);
        assert_eq!(ed.grid(), IVec3::new(4, 2, 7), "grown right and up to hold it");
        let frame = ed.shape().unwrap();
        assert!((1..=3).all(|x| (5..=6).all(|z| frame.colour_at(IVec3::new(x, 0, z)) == Some([10, 200, 10]))));
        assert_eq!(ed.steps(), 1);
        // Following the mouse: a 3 by 1 by 2 box centred on the face pointed
        // at, cut away with Ctrl (into the voxels).
        ed.let_form_follow();
        let hit = Hit { cell: IVec3::new(2, 0, 6), normal: IVec3::Z };
        assert_eq!(ed.form_box(Some(hit), true), Some((IVec3::new(1, 0, 5), IVec3::new(3, 0, 6))));
        ed.place_form(Some(hit), true, false);
        assert!((1..=3).all(|x| ed.shape().unwrap().get(IVec3::new(x, 0, 6)) == 0), "cut away");
        ed.undo();
        assert_eq!(ed.shape().unwrap().colour_at(IVec3::new(2, 0, 6)), Some([10, 200, 10]), "the cut taken back");
        // Shift: only where empty; the box's own voxels keep their colour.
        ed.colour = [255, 255, 0];
        ed.place_form(Some(Hit { cell: IVec3::new(2, 0, 4), normal: IVec3::Z }), false, true);
        assert_eq!(ed.shape().unwrap().colour_at(IVec3::new(2, 0, 5)), Some([10, 200, 10]));
        // SYMMETRY: a voxel added at the left, its mirror at the right.
        ed.symmetry = true;
        ed.choose(Tool::Add);
        let w = ed.grid().x;
        ed.press(Hit { cell: IVec3::new(0, 1, 0), normal: IVec3::Z });
        ed.release();
        let frame = ed.shape().unwrap();
        assert!(frame.get(IVec3::new(0, 1, 1)) != 0 && frame.get(IVec3::new(w - 1, 1, 1)) != 0, "both sides");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A model opens at its first frame, any size; ADD on its top grows
    /// its box (every frame's), frames are copied, taken out and turned,
    /// each a step saved at once; the file keeps its glass.
    #[test]
    fn a_model_of_any_size_is_edited_and_saved() {
        let dir = model_folder("model");
        let mut ed = Editor::open_model(&dir, "thing.vox").unwrap();
        assert_eq!((ed.scope, ed.grid(), ed.frames().len()), (Scope::Frame(0), IVec3::new(3, 2, 5), 2));
        ed.colour = [200, 40, 40];
        ed.press(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z });
        ed.release();
        assert_eq!(ed.grid(), IVec3::new(3, 2, 6), "grown up by one");
        assert!(ed.frames().iter().all(|f| f.size == IVec3::new(3, 2, 6)), "every frame's box");
        assert_eq!(ed.shape().unwrap().colour_at(IVec3::new(1, 0, 5)), Some([200, 40, 40]));
        ed.step_frame(1);
        assert_eq!(ed.scope, Scope::Frame(1));
        ed.copy_frame();
        assert_eq!((ed.scope, ed.frames().len()), (Scope::Frame(2), 3));
        ed.delete_frame();
        assert_eq!((ed.scope, ed.frames().len()), (Scope::Frame(1), 2));
        ed.turn_block(Turn::TURN);
        assert_eq!(ed.grid(), IVec3::new(2, 3, 6), "a box's sides change places");
        ed.undo();
        assert_eq!(ed.grid(), IVec3::new(3, 2, 6));
        ed.trim();
        assert_eq!(ed.grid(), IVec3::new(1, 1, 6), "cut to the voxels of both frames");
        // Saved: the model as the game reads it.
        let back = crate::vox::load_model(&dir.join("thing.vox")).unwrap();
        assert_eq!((back.frames.len(), back.size), (2, IVec3::new(1, 1, 6)));
        let top = back.frames[0].iter().find(|(p, _)| *p == IVec3::new(0, 0, 4)).unwrap().1;
        assert_eq!(top, [120, 200, 255, 128], "glass stays glass");
        assert!(back.frames[0].iter().any(|(p, c)| *p == IVec3::new(0, 0, 5) && c[3] == 255));
        // ADD to the left of the box grows it there: the voxels move right
        // by one, and the view with them.
        ed.moved = IVec3::ZERO;
        ed.press(Hit { cell: IVec3::new(0, 0, 2), normal: IVec3::NEG_X });
        ed.release();
        assert_eq!((ed.grid(), ed.moved), (IVec3::new(2, 1, 6), IVec3::X), "grown to the left");
        let frame = ed.shape().unwrap();
        assert_eq!((frame.colour_at(IVec3::new(0, 0, 2)), frame.get(IVec3::new(1, 0, 2)) != 0), (Some([200, 40, 40]), true));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

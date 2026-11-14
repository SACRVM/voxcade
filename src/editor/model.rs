//! What the editor holds and does, without a window (so it is tried in
//! tests): a level of blocks and the block being shaped, the tools, the
//! history, saving every change at once.
//!
//! Both levels of the editor are a grid with the same tools at heart
//! (`Tool::of`): in the level the cells hold blocks (POINTER, ADD, ERASE,
//! PAINT, PICK, and the held block turned, rolled, mirrored), in a block,
//! a model or a figure the voxels hold colours (those and FILL, DEEP
//! PAINT, BOX, SPHERE, SELECT). A ray from the view finds the cell it meets
//! first and the face it enters through; a floor under the grid makes room
//! to build on nothing. A stroke (pressed, dragged, let go) works on the
//! surface it began on: dragging ADD lays one layer, ERASE takes one off,
//! instead of growing towards the eye or digging in. Every stroke is one
//! step back (UNDO) and is saved at once: the level file or the block's.
//!
//! The level has a second layer, its things (figures, enemies, pickups,
//! props): ADD puts one of the held kind where the mouse points (on a
//! surface, or before a wall, in the middle of the row's depth), REMOVE
//! takes one away, MOVE drags one to another place, PICK takes up its kind.
//! And a third, its pieces (`room.rs`): models and lights set down freely
//! in voxels, places and areas, all a level without blocks has.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use glam::{IVec3, Vec3};

use super::figure::Figure;
use super::frames::{ModelFile, Shaped};
use crate::blocks::{self, Block, Turn};
use crate::grid::{Area, BlockEntry, Cell, GridLevel, Kind, Place, Thing};

#[path = "room.rs"]
pub mod room;
#[path = "disk.rs"]
pub mod disk;
#[path = "parts.rs"]
pub mod parts;
#[path = "selection.rs"]
pub mod selection;
#[path = "land.rs"]
pub mod land;
#[path = "tools.rs"]
mod tools;
#[path = "forms.rs"]
mod forms;
#[path = "history.rs"]
mod history;
#[path = "cells.rs"]
mod cells;
#[path = "model_file.rs"]
mod model_file;
#[path = "rig.rs"]
mod rig;
pub use room::{Field, Hanging, Item};
pub use cells::MOST_CELLS;

/// A joint's arrow being dragged: the drag, the handles as they were at
/// the press, where the joint was (from the pivot), how many steps there
/// were then and whether it moved since.
#[derive(Clone, Debug)]
pub struct JointDrag {
    pub drag: Option<(super::handles::Drag, super::handles::Handles)>,
    start: Vec3,
    steps: usize,
    moved: bool,
    /// The part itself dragged by an arrow; how far it went.
    pub part: bool,
    applied: IVec3,
    /// The joint's ring dragged: the axis left out (the one most along
    /// the view) and where the ray met the plane across it at the press.
    ring: Option<(usize, Vec3)>,
    /// A part's ring dragged (ROTATE): its rest rotation at the press,
    /// and whether the angles go by single degrees (Ctrl) or by fives.
    rest: [f32; 3],
    pub fine: bool,
    /// A part's arrow: the part's grid to the pivot's at the press (the
    /// drag measured in the grid as it was, `local_to_pivot`), and how far
    /// it has gone in the file's directions so far (fractions too).
    press: Option<glam::Mat4>,
    applied_f: Vec3,
}

/// How far from the pivot a figure's part may be moved (voxels).
pub const FAR: i32 = 512;

/// Where a ray (from `origin` along `dir`) meets the plane where axis `n`
/// is `at`; None if it runs along it or away.
fn plane_hit(origin: Vec3, dir: Vec3, n: usize, at: f32) -> Option<Vec3> {
    if dir[n].abs() < 1e-5 {
        return None;
    }
    let t = (at - origin[n]) / dir[n];
    (t > 0.0).then(|| origin + dir * t)
}

/// What the editor shows: the level, one block of it (its place in the
/// level's blocks), or a frame of a model (an editor opened on a model has
/// only those), or a figure of parts whole (FIGURE, `figure.rs`) with no
/// part chosen. A figure's part chosen is the frame it is made of, shaped
/// in the figure as it stands (`Editor::part_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Level,
    Block(usize),
    Frame(usize),
    Figure,
}

/// What the level's tools work on: its blocks, its things, or its pieces
/// (models and lights set down freely, places and areas: `room.rs`; a
/// level without blocks has only those).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Blocks,
    Things,
    Pieces,
    /// The level's terrain (`land.rs`).
    Terrain,
}

/// The tools, the same four on both levels (named by `Tool::label`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Changes nothing: a click chooses (a figure's part, a level's block
    /// to hold), the handles work, a drag on voxels does nothing. The
    /// tool an editor opens with and Escape comes back to (the owner kept
    /// setting voxels by a click meant to choose another part).
    Pointer,
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
    /// dragged), set where it was drawn as the mouse lets go; fresh until
    /// the next action, its depth by + and - (`Fresh`).
    Box,
    /// The same, an ellipsoid in that box.
    Sphere,
    /// In a block or a model: voxels selected by a box drawn as BOX's
    /// (into the voxels), or a double click (the color connected); worked
    /// on at the selection (`selection.rs`).
    Select,
    /// The bucket: the connected voxels of the color clicked take the held
    /// one (Ctrl: all of that color there).
    Fill,
}

impl Tool {
    /// The tools of the level's blocks, its things, and of a block.
    pub fn of(scope: Scope, layer: Layer) -> &'static [Tool] {
        match (scope, layer) {
            (Scope::Level, Layer::Blocks) => &[Tool::Pointer, Tool::Add, Tool::Remove, Tool::Paint, Tool::Fill, Tool::Box, Tool::Pick],
            (Scope::Level, Layer::Things) => &[Tool::Pointer, Tool::Add, Tool::Remove, Tool::Move, Tool::Pick],
            // (Pieces are chosen and dragged, their words on the panels.)
            (Scope::Level, Layer::Pieces) => &[],
            // (The terrain's tools are its own: `land::LandTool`.)
            (Scope::Level, Layer::Terrain) => &[],
            // (The figure whole: a press on a part chooses it, and the tool
            // works there at once.)
            (Scope::Block(_) | Scope::Frame(_) | Scope::Figure, _) => &[Tool::Pointer, Tool::Add, Tool::Remove, Tool::Paint, Tool::Fill, Tool::Deep, Tool::Box, Tool::Sphere, Tool::Select, Tool::Pick],
        }
    }

    /// Its word: one set in both editors (ADD, ERASE, PAINT: cells of
    /// blocks or voxels alike), but among a level's things, which are
    /// objects set down and taken away: ADD and REMOVE.
    pub fn label(self, scope: Scope, layer: Layer) -> &'static str {
        match (scope, layer, self) {
            (Scope::Level, Layer::Things, Tool::Remove) => "REMOVE",
            (_, _, Tool::Pointer) => "POINTER",
            (_, _, Tool::Add) => "ADD",
            (_, _, Tool::Remove) => "ERASE",
            (_, _, Tool::Paint) => "PAINT",
            (_, _, Tool::Deep) => "DEEP PAINT",
            (_, _, Tool::Move) => "MOVE",
            (_, _, Tool::Pick) => "PICK",
            (_, _, Tool::Box) => "BOX",
            (_, _, Tool::Sphere) => "SPHERE",
            (_, _, Tool::Select) => "SELECT",
            (_, _, Tool::Fill) => "FILL",
        }
    }
}

/// The shape BOX or SPHERE set last, fresh until the next action: + and -
/// make it deeper or shallower in place, BEVEL cuts its edges or not, each
/// still the one step that set it (UNDO takes it away whole). Its low
/// corner and size as before that step (a model's box may have grown with
/// it: `shift`, how far its voxels moved), the way its depth goes, and how
/// it was set: cutting, only where empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fresh {
    pub lo: IVec3,
    pub size: IVec3,
    pub way: IVec3,
    pub cut: bool,
    pub only_empty: bool,
    pub shift: IVec3,
    /// The steps there were once it was set: another step since ends it.
    steps: usize,
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

/// The level's cells, blocks, things, places, areas and daylight as a step back
/// brings them back.
#[derive(Clone, PartialEq)]
struct LevelState {
    size: IVec3,
    cells: Vec<Cell>,
    table: Vec<BlockEntry>,
    things: Vec<Thing>,
    places: Vec<Place>,
    areas: Vec<Area>,
    /// (Boxed: the steps stay small.)
    daylight: Option<Box<crate::grid::Daylight>>,
    terrain: Option<Box<crate::terrain::Terrain>>,
}

impl LevelState {
    /// The level's state as a step keeps it.
    fn of(l: &GridLevel) -> Self {
        Self {
            size: l.size,
            cells: l.cells.clone(),
            table: l.blocks.clone(),
            things: l.things.clone(),
            places: l.places.clone(),
            areas: l.areas.clone(),
            daylight: l.daylight.clone().map(Box::new),
            terrain: l.terrain.clone().map(Box::new),
        }
    }

    /// Whether the level stands as it keeps it (nothing copied).
    fn is(&self, l: &GridLevel) -> bool {
        self.size == l.size
            && self.cells == l.cells
            && self.table == l.blocks
            && self.things == l.things
            && self.places == l.places
            && self.areas == l.areas
            && self.daylight.as_deref() == l.daylight.as_ref()
            && self.terrain.as_deref() == l.terrain.as_ref()
    }

    /// About how many bytes it holds.
    fn bytes(&self) -> usize {
        use std::mem::size_of;
        let terrain = self.terrain.as_ref().map_or(0, |t| size_of::<crate::terrain::Terrain>() + t.heights.len() * 4 + t.shapes.len() * size_of::<crate::terrain::Shape>() + t.roughness.as_ref().map_or(0, Vec::len));
        size_of::<Self>() + self.cells.len() * size_of::<Cell>() + self.table.len() * size_of::<BlockEntry>() + self.things.len() * size_of::<Thing>() + self.places.len() * size_of::<Place>() + self.areas.len() * size_of::<Area>() + terrain
    }
}

/// About how many bytes a block holds.
fn block_bytes(b: &Block) -> usize {
    std::mem::size_of::<Block>() + b.voxels.len() + b.palette.len() * 3
}

/// About how many bytes a model's step keeps of it.
fn shaped_bytes(s: &Shaped) -> usize {
    s.frames.iter().map(block_bytes).sum::<usize>() + s.parts.len() * std::mem::size_of::<super::frames::Placed>() + s.palette.len() * std::mem::size_of::<super::frames::Colour>()
}

/// One step of the history, before and after. A level's or a model's
/// states are shared: a step's after is the next step's before, kept once.
enum Step {
    Level(Arc<LevelState>, Arc<LevelState>),
    Block(usize, Block, Block),
    Delete(Box<Deletion>),
    /// A model's frames, all of them, and a figure's parts as placed and
    /// rigged (a stroke on one, or frames copied, taken out, turned, the
    /// box grown or trimmed, a part's parent or joint set).
    Model(Arc<Shaped>, Arc<Shaped>),
}

impl Step {
    /// What it keeps, each with its bytes: the level, the block or the
    /// model before and after it; a shared state with its address, to be
    /// counted once (`kept_bytes`), a block of its own with none.
    fn kept(&self) -> Vec<(Option<usize>, usize)> {
        let level = |s: &Arc<LevelState>| (Some(Arc::as_ptr(s) as usize), s.bytes());
        let model = |s: &Arc<Shaped>| (Some(Arc::as_ptr(s) as usize), shaped_bytes(s));
        match self {
            Step::Level(b, a) => vec![level(b), level(a)],
            Step::Block(_, b, a) => vec![(None, block_bytes(b)), (None, block_bytes(a))],
            Step::Delete(d) => vec![(None, block_bytes(&d.block)), level(&d.before), level(&d.after)],
            Step::Model(b, a) => vec![model(b), model(a)],
        }
    }

    /// The level or the model as the step leaves it (`forward`), or as it
    /// found it.
    fn state(&self, forward: bool) -> Option<Now> {
        match self {
            Step::Level(b, a) => Some(Now::Level(Arc::clone(if forward { a } else { b }))),
            Step::Delete(d) => Some(Now::Level(Arc::clone(if forward { &d.after } else { &d.before }))),
            Step::Model(b, a) => Some(Now::Model(Arc::clone(if forward { a } else { b }))),
            Step::Block(..) => None,
        }
    }
}

/// About how many bytes steps keep together, a state two of them share
/// counted once.
fn kept_bytes<'a>(steps: impl Iterator<Item = &'a Step>) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    steps.flat_map(Step::kept).filter(|(at, _)| at.is_none_or(|at| seen.insert(at))).map(|(_, bytes)| bytes).sum()
}

/// How the level, a block or the model stood when the stroke going on
/// began (what letting go makes a step of, and Escape brings back).
enum Begun {
    Level(Arc<LevelState>),
    Block(usize, Block),
    Model(Arc<Shaped>),
}

/// The newest state a step keeps of the level or the model: the next
/// step's before shares it while they still stand so.
enum Now {
    Level(Arc<LevelState>),
    Model(Arc<Shaped>),
}

/// A block taken out of the level: its place in the list, what it was,
/// and the level before and after (its cells empty, the later blocks'
/// numbers one less).
struct Deletion {
    index: usize,
    entry: BlockEntry,
    block: Block,
    before: Arc<LevelState>,
    after: Arc<LevelState>,
}

/// How many steps the editor keeps to take back, and how many bytes they
/// may hold together (each keeps the level or the model before and after
/// it, shared with its neighbours): past either, the oldest go.
pub const KEPT_STEPS: usize = 300;
pub const KEPT_BYTES: usize = 256 << 20;

/// The editor's steps: those to take back and to do again, the stroke
/// going on, the colour whose material the last step set.
pub struct History {
    past: Vec<Step>,
    future: Vec<Step>,
    /// How many of the oldest steps went (`bound`): the count of steps
    /// ever taken is theirs and the past's (`count`).
    dropped: usize,
    /// At most so many steps and bytes kept (`KEPT_STEPS`, `KEPT_BYTES`).
    keep: (usize, usize),
    /// The stroke going on: how the level or block stood when it began.
    stroke: Option<Begun>,
    /// The newest level and model a step keeps (`Editor::level_state`,
    /// `shaped`: shared by the next step while they still stand so).
    level_now: Option<Arc<LevelState>>,
    model_now: Option<Arc<Shaped>>,
    /// The colour whose material the last step set: the next setting of it
    /// joins that step (a slider dragged is one step back).
    tuning: Option<[u8; 3]>,
    /// How far the model's voxels moved in the stroke going on (its box
    /// grown on the low sides), and since the view last followed them.
    grown: IVec3,
    pub moved: IVec3,
}

impl Default for History {
    fn default() -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            dropped: 0,
            keep: (KEPT_STEPS, KEPT_BYTES),
            stroke: None,
            level_now: None,
            model_now: None,
            tuning: None,
            grown: IVec3::ZERO,
            moved: IVec3::ZERO,
        }
    }
}

impl History {
    /// How many steps were ever taken and not taken back: those kept and
    /// those gone (a drag compares it before and after, whatever went).
    pub(super) fn count(&self) -> usize {
        self.dropped + self.past.len()
    }

    /// The oldest steps gone while more are kept than `keep` allows (by
    /// count, or by bytes with the steps to do again); the newest stays.
    pub(super) fn bound(&mut self) {
        let mut gone = 0;
        while self.past.len() - gone > 1 && (self.past.len() - gone > self.keep.0 || kept_bytes(self.past[gone..].iter().chain(&self.future)) > self.keep.1) {
            gone += 1;
        }
        if gone > 0 {
            self.past.drain(..gone);
            self.dropped += gone;
        }
    }

    /// `step` taken (`forward`) or taken back: the state it leaves is the
    /// newest, for the next step to share.
    fn remember(&mut self, step: &Step, forward: bool) {
        match step.state(forward) {
            Some(Now::Level(s)) => self.level_now = Some(s),
            Some(Now::Model(s)) => self.model_now = Some(s),
            None => {}
        }
    }
}

/// BOX and SPHERE: the footprint being dragged out, the shape set last
/// while it is fresh, the depth a new footprint gets, edges cut back
/// (BEVEL).
pub struct Forms {
    pub drawing: Option<Drawing>,
    pub fresh: Option<Fresh>,
    pub depth: i32,
    pub bevel: bool,
}

impl Default for Forms {
    fn default() -> Self {
        Self { drawing: None, fresh: None, depth: 1, bevel: false }
    }
}

/// Among the pieces (`room.rs`): the game turns things freely
/// (`c.camera.rotates = "free"`, any angle) or on the grid; what is
/// chosen; what hangs on the mouse to be set down, and the spot under
/// it; what is dragged and how far from the spot it was taken; the
/// field whose next setting joins the last step (a number dragged is
/// one step back); the model EDIT MODEL asked to open; the game's
/// worlds and prefabs, which are no models to set down.
#[derive(Default)]
pub struct Pieces {
    pub free_turns: bool,
    pub chosen: Option<Item>,
    pub hanging: Option<Hanging>,
    pub hang_spot: Option<IVec3>,
    dragging: Option<(Item, IVec3)>,
    tuned: Option<(Option<Item>, Field)>,
    pub wants_model: Option<String>,
    pub scenes: Vec<String>,
    /// The pieces' models' sizes by their files (`Figures`: what turning
    /// one about its middle and keeping it on the floor need); the light
    /// of the chosen piece chosen (its place among its lights: the arrows
    /// move it in the model); the handle being dragged; lights a piece was
    /// switched off from (by its place in the list), to switch on again.
    pub sizes: std::collections::BTreeMap<String, IVec3>,
    pub chosen_light: Option<usize>,
    gripped: Option<room::Gripped>,
    put_away: Vec<(usize, Vec<crate::grid::ThingLight>)>,
    /// The game's kinds (`c.things`) as a level's things of them turn:
    /// their models' files; those without a behavior (only they face as
    /// the file turns them); those that turn freely (`rotates = "free"`).
    pub kind_models: std::collections::BTreeMap<String, String>,
    pub still_kinds: std::collections::BTreeSet<String>,
    pub free_kinds: std::collections::BTreeSet<String>,
}

/// A figure's parts being shaped (`parts.rs`): what the next click in the
/// list, a drag, a pose and the next step do to them.
#[derive(Default)]
pub struct Rigging {
    /// A figure's joint being dragged by its arrow, and whether the next
    /// change to the model joins the last step (the drag's).
    joint_drag: Option<JointDrag>,
    join_next: bool,
    /// A figure's part: its rings (ROTATE) at it rather than its arrows
    /// (MOVE); a stroke begun from the figure whole on the part it met
    /// (the figure stays chosen, nothing is dimmed).
    pub part_rotating: bool,
    pub painting_whole: bool,
    /// The parts the next step acts on, by name (None: the chosen part):
    /// every other part keeps its parent and joint (`pin_rig`).
    acting: Option<Vec<String>>,
    /// POSE: the rings turn the chosen part as a pose on top of its rest,
    /// nothing written; the pose by part name (rotation, pitch, roll, as
    /// `frame:model`'s), kept when POSE goes off (drawn again when it
    /// comes back) until RESET or the editor closes.
    pub posing: bool,
    /// MOVE at a part turned as it stands: along the figure's axes
    /// (GLOBAL), else along its own (LOCAL, the default).
    pub move_global: bool,
    /// A handle's drag is changing the model now (no tightening of the
    /// figure's box until it is let go).
    in_drag: bool,
    pub pose: std::collections::BTreeMap<String, parts::Posed>,
    /// The figure of the model as it is now, made by `pin_rig` at a step's
    /// end: `follow_figure` takes it rather than making it again.
    made_figure: Option<Figure>,
}

/// SELECT: the voxels selected, the box drawn last (for + and -), the
/// arrows' drag, COPY's voxels, and whether a move keeps what it lands
/// on (Alt); its mode (NEW, ADD, REMOVE) and whether it takes every voxel
/// behind its rectangle (THROUGH) or only those seen.
pub struct Selecting {
    pub selection: Option<selection::Selection>,
    pub select_mode: selection::SelectMode,
    pub select_through: bool,
    sel_drag: Option<selection::SelDrag>,
    pub clipboard: Option<selection::Clip>,
    pub sel_keep: bool,
}

impl Default for Selecting {
    fn default() -> Self {
        Self { selection: None, select_mode: selection::SelectMode::New, select_through: true, sel_drag: None, clipboard: None, sel_keep: false }
    }
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
    /// Shift is held and has made the tool PICK for as long as it is
    /// (`shift_pick`): a pick then does not give the tool back, letting go
    /// does.
    shifted: bool,
    /// The block the level's tools place (its number, 1 on), as it lies.
    pub held: Cell,
    /// The colour a block's tools lay.
    pub colour: [u8; 3],
    /// The kinds of things the game has (`c.things`), and the one ADD
    /// puts down.
    pub kinds: Vec<String>,
    pub held_kind: Option<String>,
    /// LEVEL's ANCHOR for a level of blocks: the side of each axis that
    /// stays when SIZE changes (true: the low one, left, front, bottom).
    pub anchor: [bool; 3],
    /// The thing MOVE drags (its place in the level's list).
    moving: Option<usize>,
    /// The steps back and forth, the stroke going on.
    pub history: History,
    /// What went wrong last (saving), to show.
    pub message: Option<String>,
    /// The model shaped (an editor opened on a model).
    pub model: Option<ModelFile>,
    /// BOX and SPHERE.
    pub forms: Forms,
    /// Every tool's voxel mirrored left to right about the box's middle
    /// too (SYMMETRY): a figure's two sides alike.
    pub symmetry: bool,
    /// The model is a figure of parts: it shown whole (FIGURE).
    pub figure: Option<Figure>,
    /// A figure's parts being shaped: the rig, the pose, a joint's drag.
    pub rigging: Rigging,
    /// A quiet word on what the editor did or will not do (over the view).
    pub hint: Option<String>,
    /// Changes are kept in memory only, nothing written (`--shot`).
    pub dry: bool,
    /// Asked before the level opens: its things turned off the quarter
    /// turns in a game that sets things on the grid (as mistakes name
    /// them). ALIGN ALL or DON'T OPEN answers it (`align_all`).
    pub question: Option<Vec<String>>,
    /// The level's text as it was opened and the level it says: a save
    /// writes the level over it (`grid::text::rewrite`), so that what did
    /// not change stays as it was, comments too.
    source: String,
    source_level: GridLevel,
    /// The game's daylight (`c.light`): what a level's own DAYLIGHT starts
    /// from and leaves to it.
    pub game_light: crate::config::LightConfig,
    /// Among the pieces (`room.rs`).
    pub pieces: Pieces,
    /// Among the terrain's points (`land.rs`).
    pub land: land::Land,
    /// Its files as last read or written, to see another program's
    /// changes (`disk.rs`).
    pub disk: disk::Disk,
    /// What is hidden in the editor alone (`room::Hidden`).
    hidden: room::Hidden,
    /// SLICE: the plane cutting the model in the view (`slice.rs`).
    pub slice: Option<super::slice::Slice>,
    /// The level round the model, shaped in place (`context.rs`); the one
    /// EDIT asks the model's window to open it in.
    pub context: Option<super::context::Context>,
    pub wants_context: Option<super::context::Seed>,
    /// SELECT.
    pub selecting: Selecting,
}

impl Editor {
    /// An editor on `file` of the game in `dir` with nothing open yet: no
    /// level, no model, every tool and choice as it starts.
    fn blank(dir: &Path, file: &str) -> Self {
        Self {
            dir: dir.to_path_buf(),
            file: file.to_string(),
            level: GridLevel::new(1, IVec3::ZERO),
            blocks: Vec::new(),
            sheet: None,
            scope: Scope::Level,
            layer: Layer::Blocks,
            tool: Tool::Pointer,
            before_pick: Tool::Add,
            shifted: false,
            held: Cell::default(),
            colour: [200, 200, 200],
            kinds: Vec::new(),
            held_kind: None,
            anchor: [true; 3],
            moving: None,
            history: History::default(),
            message: None,
            model: None,
            forms: Forms::default(),
            symmetry: false,
            figure: None,
            rigging: Rigging::default(),
            hint: None,
            dry: false,
            question: None,
            source: String::new(),
            source_level: GridLevel::new(1, IVec3::ZERO),
            game_light: Default::default(),
            pieces: Pieces::default(),
            land: land::Land::default(),
            disk: Default::default(),
            hidden: Default::default(),
            slice: None,
            context: None,
            wants_context: None,
            selecting: Selecting::default(),
        }
    }

    /// Opens the level `file` of the game in `dir`, with its blocks and
    /// tileset.
    pub fn open(dir: &Path, file: &str) -> Result<Self> {
        let path = dir.join(file);
        let source = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let level = GridLevel::from_text(&source).with_context(|| path.display().to_string())?;
        let blocks = level.load_blocks(dir)?;
        let sheet_path = dir.join(blocks::FOLDER).join(crate::tiled::SHEET);
        let sheet = if sheet_path.exists() && level.has_grid() { Some(Sheet::load(&sheet_path, level.n)?) } else { None };
        let mut ed = Self::blank(dir, file);
        ed.held = Cell { block: (!level.blocks.is_empty()) as u16, turn: Turn::NONE };
        ed.colour = blocks.first().and_then(|b| b.colours().first().copied()).unwrap_or(ed.colour);
        // A level without blocks is its pieces.
        ed.layer = if level.has_grid() { Layer::Blocks } else { Layer::Pieces };
        (ed.source_level, ed.source, ed.level, ed.blocks, ed.sheet) = (level.clone(), source, level, blocks, sheet);
        ed.remember_held();
        Ok(ed)
    }

    /// Opens the model `file` of the game in `dir`, at its first frame; a
    /// figure of parts whole (FIGURE).
    pub fn open_model(dir: &Path, file: &str) -> Result<Self> {
        Ok(Self::with_model(dir, file, ModelFile::load(&dir.join(file))?))
    }

    /// The model `file` of the game in `dir` to look at, a MagicaVoxel
    /// `.vox` too (the IDE's picture of it): nothing written.
    pub fn look_at_model(dir: &Path, file: &str) -> Result<Self> {
        let mut ed = Self::with_model(dir, file, ModelFile::look(&dir.join(file))?);
        ed.dry = true;
        Ok(ed)
    }

    /// The world or map's prefab `file` of the game in `dir` to look at (the
    /// IDE's picture of it): nothing written.
    pub fn look_at_scene(dir: &Path, file: &str) -> Result<Self> {
        let mut ed = Self::with_model(dir, file, ModelFile::look_at_scene(&dir.join(file))?);
        ed.dry = true;
        Ok(ed)
    }

    fn with_model(dir: &Path, file: &str, model: ModelFile) -> Self {
        let mut ed = Self::blank(dir, file);
        ed.colour = model.colours().first().copied().unwrap_or(ed.colour);
        (ed.figure, ed.message) = Figure::follow(None, &model, None);
        ed.scope = if ed.figure.is_some() { Scope::Figure } else { Scope::Frame(0) };
        ed.model = Some(model);
        ed.remember_held();
        ed
    }

    /// The box of voxels shaped: the open block, or the model's frame.
    pub fn shape(&self) -> Option<&Block> {
        match self.scope {
            Scope::Level | Scope::Figure => None,
            Scope::Block(i) => self.blocks.get(i),
            Scope::Frame(f) => self.model.as_ref().and_then(|m| m.frames.get(f)),
        }
    }

    fn shape_mut(&mut self) -> Option<&mut Block> {
        match self.scope {
            Scope::Level | Scope::Figure => None,
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
            Scope::Figure => self.figure.as_ref().map_or(IVec3::ONE, |f| f.model.size),
        }
    }
}

/// The level file a game's config names (`c.world.blocks.file`), else its
/// first level in `levels/` (a level of pieces in a game of worlds).
pub fn level_file(dir: &Path, cfg: &crate::config::GameConfig) -> Result<String> {
    if let Some(b) = &cfg.world.blocks {
        return Ok(b.file.clone());
    }
    let files = crate::files::Files::disk();
    let mut found: Vec<String> = std::fs::read_dir(dir.join("levels"))
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| crate::levels::says_level(&files, p)).filter_map(|p| p.file_name().map(|n| format!("levels/{}", n.to_string_lossy()))).collect())
        .unwrap_or_default();
    found.sort_by(|a, b| crate::levels::natural(a, b));
    found.into_iter().next().context("this game has no level to edit: no c.world.blocks, and no level file in levels/ (format = \"voxcade level 1\")")
}

/// A figure's rig by part name: what each hangs on (by name) and its
/// joint from the pivot, as the rules or the file have them (None: no
/// figure, or one that cannot be made).
fn rig_of(m: &ModelFile) -> Option<std::collections::BTreeMap<String, (Option<String>, Vec3)>> {
    Some(Figure::of(m)?.ok()?.rig())
}

#[cfg(test)]
#[path = "model_tests.rs"]
pub(crate) mod tests;

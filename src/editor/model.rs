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
pub use room::{Field, Hanging, Item};

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
            (Scope::Level, Layer::Blocks) => &[Tool::Pointer, Tool::Add, Tool::Remove, Tool::Paint, Tool::Pick],
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

/// One step of the history, before and after.
enum Step {
    Level(LevelState, LevelState),
    Block(usize, Block, Block),
    Delete(Box<Deletion>),
    /// A model's frames, all of them, and a figure's parts as placed and
    /// rigged (a stroke on one, or frames copied, taken out, turned, the
    /// box grown or trimmed, a part's parent or joint set).
    Model(Shaped, Shaped),
}

impl Step {
    /// The step the other way (taking it back).
    fn reversed(&self) -> Step {
        match self {
            Step::Level(b, a) => Step::Level(a.clone(), b.clone()),
            Step::Block(i, b, a) => Step::Block(*i, a.clone(), b.clone()),
            Step::Model(b, a) => Step::Model(a.clone(), b.clone()),
            Step::Delete(d) => Step::Delete(Box::new(Deletion { index: d.index, entry: d.entry.clone(), block: d.block.clone(), before: d.after.clone(), after: d.before.clone() })),
        }
    }
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
    /// Shift is held and has made the tool PICK for as long as it is
    /// (`shift_pick`): a pick then does not give the tool back, letting go
    /// does.
    shifted: bool,
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
    /// BOX and SPHERE: the footprint being dragged out, the shape set last
    /// while it is fresh, the depth a new footprint gets, edges cut back
    /// (BEVEL).
    pub drawing: Option<Drawing>,
    pub fresh: Option<Fresh>,
    pub depth: i32,
    pub bevel: bool,
    /// Every tool's voxel mirrored left to right about the box's middle
    /// too (SYMMETRY): a figure's two sides alike.
    pub symmetry: bool,
    /// The colour whose material the last step set: the next setting of it
    /// joins that step (a slider dragged is one step back).
    tuning: Option<[u8; 3]>,
    /// The model is a figure of parts: it shown whole (FIGURE).
    pub figure: Option<Figure>,
    /// FIGURE: the next part clicked in the list becomes the chosen part's
    /// parent (PARENT ... CHOOSE).
    pub picking_parent: bool,
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
    /// Among the pieces (`room.rs`): the game turns things freely
    /// (`c.camera.rotates = "free"`, any angle) or on the grid; what is
    /// chosen; what hangs on the mouse to be set down, and the spot under
    /// it; what is dragged and how far from the spot it was taken; the
    /// field whose next setting joins the last step (a number dragged is
    /// one step back); the model EDIT MODEL asked to open; the game's
    /// worlds and prefabs, which are no models to set down.
    pub free_turns: bool,
    /// The game's daylight (`c.light`): what a level's own DAYLIGHT starts
    /// from and leaves to it.
    pub game_light: crate::config::LightConfig,
    pub chosen: Option<Item>,
    pub hanging: Option<Hanging>,
    pub hang_spot: Option<IVec3>,
    dragging: Option<(Item, IVec3)>,
    tuned: Option<(Option<Item>, Field)>,
    /// Among the terrain's points (`land.rs`).
    pub land: land::Land,
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
    /// A figure's joint being dragged by its arrow, and whether the next
    /// change to the model joins the last step (the drag's).
    joint_drag: Option<JointDrag>,
    join_next: bool,
    put_away: Vec<(usize, Vec<crate::grid::ThingLight>)>,
    /// Its files as last read or written, to see another program's
    /// changes (`disk.rs`).
    pub disk: disk::Disk,
    /// What is hidden in the editor alone (`room::Hidden`).
    hidden: room::Hidden,
    /// The game's kinds (`c.things`) as a level's things of them turn:
    /// their models' files; those without a behavior (only they face as
    /// the file turns them); those that turn freely (`rotates = "free"`).
    pub kind_models: std::collections::BTreeMap<String, String>,
    pub still_kinds: std::collections::BTreeSet<String>,
    pub free_kinds: std::collections::BTreeSet<String>,
    /// SLICE: the plane cutting the model in the view (`slice.rs`).
    pub slice: Option<super::slice::Slice>,
    /// The level round the model, shaped in place (`context.rs`); the one
    /// EDIT asks the model's window to open it in.
    pub context: Option<super::context::Context>,
    pub wants_context: Option<super::context::Seed>,
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
    /// SELECT: the voxels selected, the box drawn last (for + and -), the
    /// arrows' drag, COPY's voxels, and whether a move keeps what it lands
    /// on (Alt).
    pub selection: Option<selection::Selection>,
    /// SELECT's mode (NEW, ADD, REMOVE) and whether it takes every voxel
    /// behind its rectangle (THROUGH) or only those seen.
    pub select_mode: selection::SelectMode,
    pub select_through: bool,
    sel_drag: Option<selection::SelDrag>,
    pub clipboard: Option<selection::Clip>,
    pub sel_keep: bool,
    /// A handle's drag is changing the model now (no tightening of the
    /// figure's box until it is let go).
    in_drag: bool,
    pub pose: std::collections::BTreeMap<String, parts::Posed>,
}

impl Editor {
    /// Opens the level `file` of the game in `dir`, with its blocks and
    /// tileset.
    pub fn open(dir: &Path, file: &str) -> Result<Self> {
        let path = dir.join(file);
        let source = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let level = GridLevel::from_text(&source).with_context(|| path.display().to_string())?;
        let blocks = level.load_blocks(dir)?;
        let sheet_path = dir.join(blocks::FOLDER).join(crate::tiled::SHEET);
        let sheet = if sheet_path.exists() && level.has_grid() { Some(Sheet::load(&sheet_path, level.n)?) } else { None };
        let held = Cell { block: (!level.blocks.is_empty()) as u16, turn: Turn::NONE };
        let colour = blocks.first().and_then(|b| b.colours().first().copied()).unwrap_or([200, 200, 200]);
        // A level without blocks is its pieces.
        let layer = if level.has_grid() { Layer::Blocks } else { Layer::Pieces };
        Ok(Self {
            dir: dir.to_path_buf(),
            file: file.to_string(),
            source_level: level.clone(),
            source,
            level,
            blocks,
            sheet,
            scope: Scope::Level,
            layer,
            tool: Tool::Pointer,
            before_pick: Tool::Add,
            shifted: false,
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
            fresh: None,
            depth: 1,
            bevel: false,
            symmetry: false,
            tuning: None,
            figure: None,
            picking_parent: false,
            hint: None,
            dry: false,
            question: None,
            free_turns: false,
            game_light: Default::default(),
            chosen: None,
            hanging: None,
            hang_spot: None,
            dragging: None,
            tuned: None,
            land: land::Land::default(),
            wants_model: None,
            scenes: Vec::new(),
            sizes: Default::default(),
            chosen_light: None,
            gripped: None,
            joint_drag: None,
            join_next: false,
            put_away: Vec::new(),
            disk: Default::default(),
            hidden: Default::default(),
            kind_models: Default::default(),
            still_kinds: Default::default(),
            free_kinds: Default::default(),
            slice: None,
            context: None,
            wants_context: None,
            part_rotating: false,
            painting_whole: false,
            acting: None,
            posing: false,
            move_global: false,
            selection: None,
            select_mode: selection::SelectMode::New,
            select_through: true,
            sel_drag: None,
            clipboard: None,
            sel_keep: false,
            in_drag: false,
            pose: std::collections::BTreeMap::new(),
        })
        .map(|mut ed: Self| {
            ed.remember_held();
            ed
        })
    }

    /// Opens the model `file` of the game in `dir`, at its first frame; a
    /// figure of parts whole (FIGURE).
    pub fn open_model(dir: &Path, file: &str) -> Result<Self> {
        Self::with_model(dir, file, ModelFile::load(&dir.join(file))?)
    }

    /// The model `file` of the game in `dir` to look at, a MagicaVoxel
    /// `.vox` too (the IDE's picture of it): nothing written.
    pub fn look_at_model(dir: &Path, file: &str) -> Result<Self> {
        let mut ed = Self::with_model(dir, file, ModelFile::look(&dir.join(file))?)?;
        ed.dry = true;
        Ok(ed)
    }

    /// The world or map's prefab `file` of the game in `dir` to look at (the
    /// IDE's picture of it): nothing written.
    pub fn look_at_scene(dir: &Path, file: &str) -> Result<Self> {
        let mut ed = Self::with_model(dir, file, ModelFile::look_at_scene(&dir.join(file))?)?;
        ed.dry = true;
        Ok(ed)
    }

    fn with_model(dir: &Path, file: &str, model: ModelFile) -> Result<Self> {
        let level = GridLevel::new(1, IVec3::ZERO);
        let colour = model.colours().first().copied().unwrap_or([200, 200, 200]);
        let (figure, message) = Figure::follow(None, &model);
        Ok(Self {
            dir: dir.to_path_buf(),
            file: file.to_string(),
            level,
            blocks: Vec::new(),
            sheet: None,
            scope: if figure.is_some() { Scope::Figure } else { Scope::Frame(0) },
            layer: Layer::Blocks,
            tool: Tool::Pointer,
            before_pick: Tool::Add,
            shifted: false,
            held: Cell::default(),
            colour,
            kinds: Vec::new(),
            held_kind: None,
            moving: None,
            past: Vec::new(),
            future: Vec::new(),
            stroke: None,
            message,
            model: Some(model),
            grown: IVec3::ZERO,
            moved: IVec3::ZERO,
            drawing: None,
            fresh: None,
            depth: 1,
            bevel: false,
            symmetry: false,
            tuning: None,
            figure,
            picking_parent: false,
            hint: None,
            dry: false,
            question: None,
            source: String::new(),
            source_level: GridLevel::new(1, IVec3::ZERO),
            free_turns: false,
            game_light: Default::default(),
            chosen: None,
            hanging: None,
            hang_spot: None,
            dragging: None,
            tuned: None,
            land: land::Land::default(),
            wants_model: None,
            scenes: Vec::new(),
            sizes: Default::default(),
            chosen_light: None,
            gripped: None,
            joint_drag: None,
            join_next: false,
            put_away: Vec::new(),
            disk: Default::default(),
            hidden: Default::default(),
            kind_models: Default::default(),
            still_kinds: Default::default(),
            free_kinds: Default::default(),
            slice: None,
            context: None,
            wants_context: None,
            part_rotating: false,
            painting_whole: false,
            acting: None,
            posing: false,
            move_global: false,
            selection: None,
            select_mode: selection::SelectMode::New,
            select_through: true,
            sel_drag: None,
            clipboard: None,
            sel_keep: false,
            in_drag: false,
            pose: std::collections::BTreeMap::new(),
        })
        .map(|mut ed: Self| {
            ed.remember_held();
            ed
        })
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

    pub fn choose(&mut self, tool: Tool) {
        if tool != self.tool {
            self.fresh = None;
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
            let free = self.stroke.is_none() && self.drawing.is_none() && self.joint_drag.is_none() && self.gripped.is_none() && Tool::of(self.scope, self.layer).contains(&Tool::Pick);
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
    fn picked(&mut self) {
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
    fn filled(&self, c: IVec3, then: bool) -> bool {
        match (self.scope, then.then_some(self.stroke.as_ref()).flatten()) {
            (Scope::Level, Some(Step::Level(before, _))) => {
                let s = before.size;
                c.cmplt(s).all() && before.cells[(c.x + c.y * s.x + c.z * s.x * s.y) as usize].block != 0
            }
            (Scope::Level, _) => self.level.cell(c).block != 0,
            (Scope::Block(_), Some(Step::Block(_, before, _))) => before.get(c) != 0,
            // (The model as the stroke found it, moved as its box grew.)
            (Scope::Frame(f), Some(Step::Model(before, _))) => before.frames.get(f).is_some_and(|b| b.get(c - self.grown) != 0),
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
        self.stroke = Some(match self.scope {
            Scope::Level => Step::Level(self.level_state(), self.level_state()),
            Scope::Block(i) => Step::Block(i, self.blocks[i].clone(), self.blocks[i].clone()),
            Scope::Frame(_) => Step::Model(self.shaped(), Shaped::default()),
            Scope::Figure => return,
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
    fn grow_model(&mut self, c: IVec3) -> Option<IVec3> {
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
    fn grow_model_to(&mut self, low: IVec3, high: IVec3) -> Option<IVec3> {
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
                let size = super::frames::fit(old + raise, lo - p + raise, hi - 1 - p + raise);
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
        self.grown += shift;
        // (A figure's part: the view follows the part's place instead,
        // `View::follow_origin`.)
        if part.is_none() {
            self.moved += shift;
        }
        Some(shift)
    }

    /// The mouse let go: the stroke, if it changed anything, is a step and
    /// is saved.
    pub fn release(&mut self) {
        // A drag of a part or a selection let go: the figure's box made
        // tight, in the drag's own step.
        if let Some(d) = self.joint_drag.take()
            && d.moved
            && self.model.as_ref().is_some_and(|m| m.parted() && !m.is_tight())
        {
            self.join_next = self.past.len() > d.steps;
            self.change_model_file(|m| m.size = m.tight_size());
            self.join_next = false;
        }
        self.let_go_of_slice();
        self.let_go_of_land();
        self.moving = None;
        self.dragging = None;
        self.gripped = None;
        self.joint_drag = None;
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
        self.tuning = None;
        self.tuned = None;
    }

    /// Whether the tool is BOX or SPHERE (in a block or a model).
    pub fn forms(&self) -> bool {
        matches!(self.tool, Tool::Box | Tool::Sphere) && self.scope != Scope::Level
    }

    /// BOX or SPHERE pressed at a hit: a footprint begins on the face's
    /// plane, before it (or, to `cut`, in the voxel itself); the shape set
    /// before is fresh no more.
    pub fn begin_form(&mut self, hit: Hit, cut: bool) {
        if self.drag_open() {
            self.end_drag(false);
        }
        self.fresh = None;
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

    /// The box BOX or SPHERE shows now, its low and high corner: the
    /// footprint being dragged out (as deep as `depth`), or the shape set
    /// last while it is fresh, where it stands.
    pub fn form_box(&self) -> Option<(IVec3, IVec3)> {
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
        let f = self.fresh?;
        Some((f.lo + f.shift, f.lo + f.shift + f.size - 1))
    }

    /// The cells of the shape in `lo..=hi` (with SYMMETRY their mirrors
    /// too, about the box as it stands).
    fn shape_cells(&self, lo: IVec3, hi: IVec3) -> Vec<IVec3> {
        let mut cells = form_cells(self.tool, lo, hi, self.bevel);
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
        match (self.drawing.is_some(), self.form_box()) {
            (true, Some((lo, hi))) => self.shape_cells(lo, hi),
            _ => Vec::new(),
        }
    }

    /// The footprint let go: the shape set right where it was drawn
    /// (filling in the colour held, only where empty with `only_empty`, or
    /// cutting away as it was drawn), one step; fresh until the next action.
    pub fn set_drawn(&mut self, only_empty: bool) {
        let Some((lo, hi)) = self.form_box().filter(|_| self.drawing.is_some()) else { return };
        let Some(d) = self.drawing.take() else { return };
        let fresh = Fresh { lo, size: hi - lo + 1, way: deep_way(d.normal, d.cut), cut: d.cut, only_empty, shift: IVec3::ZERO, steps: 0 };
        self.depth = fresh.size[axis_of(fresh.way)];
        self.set_fresh(fresh);
    }

    /// Sets `fresh` as one step and keeps it fresh (nothing set: none).
    fn set_fresh(&mut self, mut fresh: Fresh) {
        let before = self.steps();
        let shift = self.place_cells(fresh.lo, fresh.lo + fresh.size - 1, fresh.cut, fresh.only_empty);
        if self.steps() == before {
            self.fresh = None;
            return;
        }
        (fresh.shift, fresh.steps) = (shift, self.steps());
        self.fresh = Some(fresh);
    }

    /// The fresh shape, if it is still the last step: its step taken back
    /// quietly, to be set again changed (+, -, BEVEL).
    fn take_fresh_back(&mut self) -> Option<Fresh> {
        let fresh = self.fresh.take().filter(|f| f.steps == self.steps() && self.drawing.is_none() && self.stroke.is_none())?;
        let step = self.past.pop()?;
        self.go(&step, false);
        self.moved -= fresh.shift;
        Some(fresh)
    }

    /// A voxel deeper (`by` 1) or shallower (-1): the shape set last while
    /// it is fresh, in place (its step still one); else the depth the next
    /// footprint gets (the one being dragged out too).
    pub fn deepen(&mut self, by: i32) {
        let Some(mut fresh) = self.take_fresh_back() else {
            self.depth = (self.depth + by).clamp(1, 256);
            return;
        };
        let a = axis_of(fresh.way);
        let deeper = (fresh.size[a] + by).clamp(1, 256);
        // Drawn into the voxels or down a face: it grows from its far end.
        if fresh.way[a] < 0 {
            fresh.lo[a] -= deeper - fresh.size[a];
        }
        fresh.size[a] = deeper;
        self.depth = deeper;
        self.set_fresh(fresh);
    }

    /// BEVEL on or off; the shape set last, while fresh, set again so.
    pub fn toggle_bevel(&mut self) {
        let fresh = self.take_fresh_back();
        self.bevel = !self.bevel;
        if let Some(fresh) = fresh {
            self.set_fresh(fresh);
        }
    }

    /// Escape: the footprint being dragged out let go, the shape set last
    /// fresh no more (it stays); whether there was either.
    pub fn drop_form(&mut self) -> bool {
        let had = self.fresh.is_some() || self.drawing.is_some();
        (self.fresh, self.drawing) = (None, None);
        had
    }

    /// Sets the shape in `lo..=hi`: filling in the colour held (only where
    /// empty with `only_empty`), or cutting away; mirrored too with
    /// SYMMETRY. A model's box grows to hold it (how far its voxels moved
    /// comes back); a block's stays, what lies outside left out. One step,
    /// saved.
    fn place_cells(&mut self, lo: IVec3, hi: IVec3, cut: bool, only_empty: bool) -> IVec3 {
        let mut cells = self.shape_cells(lo, hi);
        if cells.is_empty() || self.shape().is_none() {
            return IVec3::ZERO;
        }
        self.stroke = Some(match self.scope {
            Scope::Level | Scope::Figure => return IVec3::ZERO,
            Scope::Block(i) => Step::Block(i, self.blocks[i].clone(), self.blocks[i].clone()),
            Scope::Frame(_) => Step::Model(self.shaped(), Shaped::default()),
        });
        self.grown = IVec3::ZERO;
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

    /// Takes the last step back.
    pub fn undo(&mut self) {
        self.release();
        self.fresh = None;
        self.selection = None;
        self.tuning = None;
        self.tuned = None;
        // Another program's change first: then there is nothing to take
        // back (`look_at_disk`).
        self.look_at_disk();
        if let Some(step) = self.past.pop()
            && self.go(&step, false)
        {
            self.future.push(step);
        }
    }

    /// Takes the last step taken back again.
    pub fn redo(&mut self) {
        self.release();
        self.fresh = None;
        self.selection = None;
        self.tuning = None;
        self.tuned = None;
        self.look_at_disk();
        if let Some(step) = self.future.pop()
            && self.go(&step, true)
        {
            self.past.push(step);
        }
    }

    /// A step taken back or again, written; whether it still stands (the
    /// file changed meanwhile and it was laid on it or dropped: no).
    fn go(&mut self, step: &Step, forward: bool) -> bool {
        let mut problem = None;
        match step {
            Step::Level(before, after) => self.set_level(if forward { after } else { before }),
            Step::Block(i, before, after) => self.blocks[*i] = if forward { after } else { before }.clone(),
            Step::Model(before, after) => {
                if let Some(model) = &mut self.model {
                    model.reshape(if forward { after } else { before });
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
        // Taken back, the step goes the other way.
        let back;
        let saved = self.save(if forward {
            step
        } else {
            back = step.reversed();
            &back
        });
        if problem.is_some() {
            self.message = problem;
        }
        matches!(saved, disk::Saved::Kept)
    }

    fn set_level(&mut self, s: &LevelState) {
        self.level.size = s.size;
        self.level.cells = s.cells.clone();
        self.level.blocks = s.table.clone();
        self.level.things = s.things.clone();
        self.level.places = s.places.clone();
        self.level.areas = s.areas.clone();
        self.level.daylight = s.daylight.as_deref().cloned();
        self.level.terrain = s.terrain.as_deref().cloned();
        // (Points no longer there are no longer chosen.)
        let t = self.level.terrain.as_ref();
        let has = |p: &glam::IVec2| t.is_some_and(|t| t.index(*p).is_some());
        self.land.selected.retain(has);
        self.land.chosen = self.land.chosen.filter(has);
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
            // POINTER changes nothing.
            Tool::Pointer => return,
            Tool::Pick => {
                // (A piece is no kind to put down: placing pieces is the
                // next stage's.)
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
        self.stroke = Some(Step::Level(self.level_state(), self.level_state()));
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
        self.stroke = Some(Step::Level(self.level_state(), self.level_state()));
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
            // Choosing a parent: no more; from a figure's part: the figure
            // with none chosen.
            if self.picking_parent {
                self.picking_parent = false;
            } else if self.figure.is_some() {
                self.scope = Scope::Figure;
                self.picking_parent = false;
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
        let l = &self.level;
        LevelState { size: l.size, cells: l.cells.clone(), table: l.blocks.clone(), things: l.things.clone(), places: l.places.clone(), areas: l.areas.clone(), daylight: l.daylight.clone().map(Box::new), terrain: l.terrain.clone().map(Box::new) }
    }

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
        let mut bounds: Option<(IVec3, IVec3)> = None;
        for (_, frame) in self.frames().iter().enumerate().filter(|&(k, _)| part.is_none_or(|f| f == k)) {
            for k in (0..frame.voxels.len()).filter(|&k| frame.voxels[k] != 0) {
                let p = frame.at(k);
                bounds = Some(bounds.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
            }
        }
        let Some((lo, hi)) = bounds else { return };
        // Where the box shown moves to: the part's low corner, or the
        // smallest box about the pivot.
        let (shift, size) = match part {
            Some(_) => (-lo, hi - lo + 1),
            None => {
                let p = crate::vvx::pivot(self.grid());
                let size = super::frames::least(lo - p, hi - p);
                (crate::vvx::pivot(size) - p - IVec3::Z * lo.z, size)
            }
        };
        if part.is_none() {
            self.moved += shift;
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

    /// Whether TRIM has something to cut: the figure whole's box, a
    /// part's own box, a model of frames' box (about its pivot) larger
    /// than its voxels need. (A level's block keeps its N×N×N.)
    pub fn can_trim(&self) -> bool {
        match self.scope {
            Scope::Figure => self.model.as_ref().is_some_and(|m| !m.is_tight()),
            Scope::Frame(_) => {
                let part = self.part_mode();
                let mut bounds: Option<(IVec3, IVec3)> = None;
                for (_, frame) in self.frames().iter().enumerate().filter(|&(k, _)| part.is_none_or(|f| f == k)) {
                    for k in (0..frame.voxels.len()).filter(|&k| frame.voxels[k] != 0) {
                        let p = frame.at(k);
                        bounds = Some(bounds.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
                    }
                }
                let Some((lo, hi)) = bounds else { return false };
                match part {
                    Some(_) => lo != IVec3::ZERO || hi + 1 != self.grid(),
                    None => {
                        let p = crate::vvx::pivot(self.grid());
                        super::frames::least(lo - p, hi - p) != self.grid()
                    }
                }
            }
            _ => false,
        }
    }

    /// The figure whole made again if its frames or parts changed (a
    /// stroke, a step back, a parent or a joint set); its trouble said.
    pub fn follow_figure(&mut self) {
        let Some(model) = &self.model else { return };
        let (figure, trouble) = Figure::follow(self.figure.take(), model);
        self.figure = figure;
        if trouble.is_some() {
            self.message = trouble;
        }
        if self.scope == Scope::Figure && self.figure.is_none() {
            self.scope = Scope::Frame(0);
        }
        self.sync_choice();
    }

    /// The part chosen in the figure's list as the scope says: the frame
    /// shaped is that part's.
    fn sync_choice(&mut self) {
        let scope = self.scope;
        let (posing, pose) = (self.posing, &self.pose);
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
        if self.picking_parent {
            self.picking_parent = false;
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
    fn chosen(&self) -> Option<usize> {
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
        let at = ((figure.joint(p) + figure.low.as_vec3()) * 2.0).round() / 2.0 + by;
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
        let start = self.figure.as_ref().map_or(Vec3::ZERO, |f| f.joint(p) + f.low.as_vec3());
        self.end_drag(false);
        self.joint_drag = Some(JointDrag { drag: None, start, steps: self.past.len(), moved: false, part: false, applied: IVec3::ZERO, ring: Some((n, hit)), rest: [0.0; 3], fine: false, press: None, applied_f: Vec3::ZERO });
        true
    }

    /// The press on an arrow of the chosen part itself (MOVE, `h` as
    /// `part_handles` gave them): the part dragged along its axis by whole
    /// voxels, those hanging on it along, until let go (one step) or
    /// Escape (back as it was, no step).
    pub fn grab_part(&mut self, drag: super::handles::Drag, h: super::handles::Handles) -> bool {
        // A selection's arrow: its voxels dragged.
        if self.selection().is_some() && !self.posing {
            self.end_drag(false);
            self.grab_selection();
            self.joint_drag = Some(JointDrag { drag: Some((drag, h)), start: Vec3::ZERO, steps: self.past.len(), moved: false, part: true, applied: IVec3::ZERO, ring: None, rest: [0.0; 3], fine: false, press: None, applied_f: Vec3::ZERO });
            return true;
        }
        // (The figure whole: its arrows move every part.)
        if self.chosen().is_none() && self.scope != Scope::Figure {
            return false;
        }
        self.end_drag(false);
        let rest = self.turn_values().unwrap_or_default();
        self.joint_drag = Some(JointDrag { drag: Some((drag, h)), start: Vec3::ZERO, steps: self.past.len(), moved: false, part: true, applied: IVec3::ZERO, ring: None, rest, fine: false, press: self.part_mode().map(|_| self.local_to_pivot()), applied_f: Vec3::ZERO });
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

    pub fn part_handles(&self, view_proj: glam::Mat4) -> Option<super::handles::Handles> {
        // A selection: its arrows (MOVE), whatever is chosen.
        if let Some(h) = self.selection_handles(view_proj) {
            return Some(h);
        }
        // POSE: only its rings.
        if self.posing {
            return self.rest_rings(view_proj);
        }
        self.part_handles_as(view_proj, self.part_rotating)
    }

    /// The chosen part's handles in ROTATE (`rotating`) or MOVE, whichever
    /// is lit (the bar at the part stands clear of both).
    pub fn part_handles_as(&self, view_proj: glam::Mat4, rotating: bool) -> Option<super::handles::Handles> {
        if self.painting_whole {
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
        let mut h = super::handles::Handles::at(centre, super::handles::world_size(view_proj, centre, 0.1));
        // MOVE's arrows: LOCAL along the part's own axes as it stands (its
        // parents' rests and its own; in the grid shown, which is its own),
        // GLOBAL along the figure's.
        let back = if self.move_global { self.rest_view().to_scale_rotation_translation().1.inverse() } else { glam::Quat::IDENTITY };
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
        let posed = if self.posing { self.chosen_posed().map_or(Vec3::ZERO, |p| Vec3::from(p.offset)) } else { Vec3::ZERO };
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
    fn moving_with(&self, p: usize) -> Vec<String> {
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
        self.acting = Some(names.clone());
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
        self.acting = Some(names.clone());
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
    fn joint_kept(&self, p: usize, k: usize) -> Option<Vec3> {
        let (m, f) = (self.model.as_ref()?, self.figure.as_ref()?);
        let o = &m.parts[k];
        (o.joint.is_none() && o.rotation != [0.0; 3] && f.parts()[p].parent.is_some()).then(|| f.joint(p) + f.low.as_vec3())
    }

    /// The chosen part's place among the file's parts.
    fn chosen_placed_index(&self) -> Option<usize> {
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

    /// Whether a tool's stroke is open (pressed, not let go).
    pub fn stroke_open(&self) -> bool {
        self.stroke.is_some()
    }

    /// Whether a drag is open: a handle's (a part's, a selection's, a
    /// joint's, a piece's), the slice's, a piece over the floor, a tool's
    /// stroke, a footprint.
    pub fn drag_open(&self) -> bool {
        self.joint_drag.is_some() || self.gripped.is_some() || self.dragging.is_some() || self.moving.is_some() || self.stroke.is_some() || self.drawing.is_some() || self.slicing()
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
        }
        // (A footprint never let go of sets nothing.)
        self.drawing = None;
        self.release();
        self.let_go_of_selection();
        self.join_next = false;
        self.follow_figure();
    }

    /// The joint being dragged, if one is.
    pub fn joint_drag(&self) -> Option<&JointDrag> {
        self.joint_drag.as_ref()
    }

    /// The mouse dragged on with a joint's arrow (as `Drag::amount` takes
    /// it): the joint where the drag puts it, the drag's one step.
    pub fn drag_joint(&mut self, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) {
        self.in_drag = true;
        self.drag_joint_now(view_proj, screen, mouse, ray);
        self.in_drag = false;
    }

    fn drag_joint_now(&mut self, view_proj: glam::Mat4, screen: glam::Vec2, mouse: glam::Vec2, ray: (Vec3, Vec3)) {
        let Some(mut d) = self.joint_drag.take() else { return };
        // A click on a handle moves nothing: only a drag that has gone a
        // few pixels.
        if let Some((drag, _)) = &mut d.drag
            && !drag.travels(mouse)
        {
            self.joint_drag = Some(d);
            return;
        }
        // A part's arrow (MOVE, LOCAL or GLOBAL): measured in its grid as
        // it was at the press; whole voxels along the arrow (Shift: tenths),
        // into its `at` and `offset` (`move_part_by`).
        if let (Some((drag, h)), Some(p0)) = (&mut d.drag, d.press)
            && matches!(drag.grip, super::handles::Grip::Arrow(_))
            && self.sel_drag.is_none()
        {
            let fix = self.local_to_pivot().inverse() * p0;
            let back = fix.inverse();
            let amount = drag.amount(h, view_proj * fix, screen, mouse, (back.transform_point3(ray.0), back.transform_vector3(ray.1).normalize_or_zero()));
            let along = if d.fine { (amount * 10.0).round() / 10.0 } else { amount.round() };
            let way = self.own_rest() * h.direction(drag.grip).unwrap_or(Vec3::ZERO);
            let to = way * along;
            if (to - d.applied_f).abs().max_element() > 1e-4 {
                self.join_next = d.moved && self.past.len() > d.steps;
                self.move_part_by(to - d.applied_f);
                self.join_next = false;
                (d.applied_f, d.moved) = (to, true);
            }
            self.joint_drag = Some(d);
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
            if let super::handles::Grip::Ring(a) = drag.grip {
                let axis = [1, 2, 0][a.min(2)];
                let way = if axis == 0 { -amount } else { amount };
                // (Shift: by single degrees.)
                let snap = if d.fine { 1.0 } else { 5.0 };
                let to = ((d.rest[axis] + way) / snap).round() * snap;
                if self.posing {
                    // A pose: nothing written, no step.
                    self.set_pose(axis, to);
                } else if self.part_rotation().is_some_and(|r| r[axis] != to) {
                    self.join_next = d.moved && self.past.len() > d.steps;
                    self.set_part_rotation(axis, to);
                    self.join_next = false;
                    d.moved = true;
                }
                self.joint_drag = Some(d);
                return;
            }
            // Its arrows lie along the axes the part moves by in the file
            // (its parent's, the part's own rest turned back).
            let to = match drag.grip {
                super::handles::Grip::Arrow(a) => {
                    let mut v = IVec3::ZERO;
                    v[a] = amount.round() as i32;
                    v
                }
                _ => (h.direction(drag.grip).unwrap_or(Vec3::ZERO) * amount).round().as_ivec3(),
            };
            if self.sel_drag.is_some() {
                if to != d.applied {
                    let joined = d.moved && self.past.len() > d.steps;
                    self.move_selection_to(to, self.sel_keep, joined);
                    (d.applied, d.moved) = (to, true);
                }
                self.joint_drag = Some(d);
                return;
            }
            if to != d.applied {
                self.join_next = d.moved && self.past.len() > d.steps;
                self.move_part(to - d.applied);
                self.join_next = false;
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
                let now = figure.joint(p) + figure.low.as_vec3();
                if now != at {
                    // One step for the whole drag: the first change makes
                    // it, the next ones take its place.
                    self.join_next = d.moved && self.past.len() > d.steps;
                    self.set_joint(Some(at));
                    self.join_next = false;
                    self.follow_figure();
                    d.moved = true;
                }
            }
        }
        self.joint_drag = Some(d);
    }

    /// Escape while a joint is dragged: as it was, no step.
    pub fn cancel_joint(&mut self) -> bool {
        let Some(d) = self.joint_drag.take() else { return false };
        // A selection dragged: back where it was, no step.
        if let Some(s) = self.sel_drag.take() {
            if d.moved && self.past.len() > d.steps {
                self.undo();
                self.future.pop();
            }
            self.selection = Some(s.selection().clone());
            return true;
        }
        // A pose dragged: as it was.
        if self.posing && d.part && d.drag.as_ref().is_some_and(|(g, _)| matches!(g.grip, super::handles::Grip::Ring(_))) {
            for axis in 0..3 {
                self.set_pose(axis, d.rest[axis]);
            }
            return true;
        }
        if d.moved && self.past.len() > d.steps {
            self.undo();
            self.future.pop();
        }
        true
    }

    /// The model's frames and a figure's parts as they stand.
    fn shaped(&self) -> Shaped {
        self.model.as_ref().map_or_else(Shaped::default, ModelFile::shaped)
    }

    /// A change to the model's frames as a step, saved (nothing if it
    /// changes nothing).
    fn change_model(&mut self, change: impl FnOnce(&mut Vec<Block>)) {
        self.change_model_file(|m| change(&mut m.frames));
    }

    /// A change to the model's frames or a figure's parts as a step, saved
    /// (nothing if it changes nothing).
    fn change_model_file(&mut self, change: impl FnOnce(&mut ModelFile)) {
        // (What a drag let go of here commits is its own step.)
        let acting = self.acting.take();
        self.release();
        self.acting = acting;
        // A drag's next change joins the step its first made.
        let joined = match (self.join_next, self.past.pop()) {
            (true, Some(Step::Model(before, _))) => Some(before),
            (_, last) => {
                self.past.extend(last);
                None
            }
        };
        let Some(model) = &mut self.model else { return };
        let before = model.shaped();
        change(model);
        model.list_new_colours();
        // A figure's box as tight as its parts allow at every step's end
        // (a drag's at its let-go: `release`), as it grows: a part that
        // flew far and came back leaves no huge box behind.
        if model.parted() && !self.in_drag {
            model.size = model.tight_size();
        }
        self.pin_rig(&before);
        let Some(model) = &self.model else { return };
        if model.is(&before) && joined.is_none() {
            return;
        }
        let before = joined.unwrap_or(before);
        let step = Step::Model(before, model.shaped());
        self.commit(step);
        self.tuning = None;
    }

    /// After a change to a figure (from `before`): every part it did not
    /// act on (`acting`, else the chosen part) whose parent or joint the
    /// rules would now find otherwise keeps them, stated in the file (its
    /// parent by name, its joint from the pivot), so that no step re-hangs
    /// or re-hinges a part nobody touched (a copy laid over its original,
    /// a voxel set where two parts meet).
    fn pin_rig(&mut self, before: &Shaped) {
        let acting = self.acting.take();
        let Some(model) = &self.model else { return };
        if !model.parted() {
            return;
        }
        let acting = acting.unwrap_or_else(|| self.chosen().and_then(|p| self.figure.as_ref().map(|f| vec![f.parts()[p].name.clone()])).unwrap_or_default());
        let mut old = model.clone();
        old.reshape(before);
        let (Some(was), Some(now)) = (rig_of(&old), rig_of(model)) else { return };
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
        self.acting = Some(names);
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
        let Some(model) = &mut self.model else { return };
        let before = model.shaped();
        model.set_material(key, mat);
        let after = model.shaped();
        let first = match self.past.last() {
            Some(Step::Model(first, _)) if self.tuning == Some(key) => Some(first.clone()),
            _ => None,
        };
        if first.is_some() {
            self.past.pop();
        }
        let step = Step::Model(first.unwrap_or(before), after);
        self.commit(step);
        self.tuning = Some(key);
    }

    /// A colour no voxel has any more taken out of the model's palette
    /// (its sign goes with it); a step.
    pub fn forget_colour(&mut self, key: [u8; 3]) {
        self.change_model_file(|m| {
            m.forget(key);
        });
    }

    /// A step taken: written (`save`, which looks at the disk first) and
    /// kept to take back; as laid on the file, or dropped, where another
    /// program changed the file meanwhile.
    fn commit(&mut self, mut step: Step) {
        self.follow_land(&mut step);
        match self.save(&step) {
            disk::Saved::Kept => {
                self.past.push(step);
                self.future.clear();
            }
            disk::Saved::Rebased(kept) => self.past.extend(kept.map(|s| *s)),
            disk::Saved::Dropped => {}
        }
    }

    /// Writes the level over the text it was opened from
    /// (`grid::text::rewrite`): what did not change stays as it was.
    fn save_level(&self) -> Result<String> {
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
    let f = Figure::of(m)?.ok()?;
    let low = f.low.as_vec3();
    Some(f.parts().iter().enumerate().map(|(p, o)| (o.name.clone(), (o.parent.map(|q| f.parts()[q].name.clone()), f.joint(p) + low))).collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const N: i32 = 4;

    /// A game folder with a 3 by 2 by 2 level of one block (a solid cube,
    /// grey) in cell (0, 0, 0), and a tileset of two tiles (1 red, 2
    /// nothing).
    pub(crate) fn folder(name: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("editor-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        let mut level = GridLevel::new(N, IVec3::new(3, 2, 2));
        level.blocks.push(BlockEntry { name: "1".into(), kind: Kind::Solid });
        level.cells[0] = Cell { block: 1, turn: Turn::NONE };
        level.save(&dir.join("level.toml")).unwrap();
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
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
        assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap(), ed.level);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn painting_recolours_and_adding_grows_from_the_face() {
        let dir = folder("paint");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_a_block_empties_its_cells_and_undo_brings_it_back() {
        let dir = folder("delete");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
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
        assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap(), ed.level);
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks.len(), 2);
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        assert_eq!(ed.kind(), Some(Kind::Solid));
        ed.set_kind(Kind::Oneway);
        assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks[0].kind, Kind::Oneway);
        ed.undo();
        assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks[0].kind, Kind::Solid);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picking_takes_up_and_goes_back_to_the_tool() {
        let dir = folder("pick");
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
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
        let mut ed = Editor::open(&dir, "level.toml").unwrap();
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
        ed.kinds = vec!["fox".into(), "gem".into()];
        ed.hold_kind(1);
        ed.switch(Layer::Things);
        let saved = || GridLevel::read(&dir.join("level.toml")).unwrap().things;
        let gem = |at: [i32; 3]| vec![Thing::new("gem", at)];
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
        let ed = Editor::open(&dir, "level.toml").unwrap();
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
    /// colours: a column in the near row, its top glass in the first frame.
    fn model_folder(name: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("editor-{name}"));
        let frame = |top: char| format!("[[frame]]\nlayers = \"\"\"\n{}...\n.{top}.\n\"\"\"\n", "...\n.a.\n\n".repeat(4));
        let text = format!("format = \"{}\"\nsize = [3, 2, 5]\n\n[palette]\na = \"#c82828\"\nb = {{ color = \"#78c8ff\", glass = 0.5 }}\n\n{}\n{}", crate::vvx::FORMAT, frame('b'), frame('a'));
        std::fs::write(dir.join("thing.vvx"), text).unwrap();
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

    /// BOX on a model: a footprint dragged out on the top face and let go
    /// is set right there (its depth the last one used, 1 at first), the
    /// box grown to hold it; fresh, + and - change its depth in place and
    /// BEVEL its edges, still one step, UNDO takes it away whole; the next
    /// drag begins a new one and leaves the first; Alt fills only where
    /// empty, Ctrl cuts; with SYMMETRY the other side too.
    #[test]
    fn a_box_is_set_where_it_is_drawn() {
        let dir = model_folder("box");
        let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
        ed.choose(Tool::Box);
        ed.colour = [10, 200, 10];
        let first = ed.grid();
        let green = |ed: &Editor, x: i32, z: i32| ed.shape().unwrap().colour_at(IVec3::new(x, 0, z)) == Some([10, 200, 10]);
        // On top of the voxel at (1, 0, 4), dragged to x 3, let go.
        ed.begin_form(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z }, false);
        ed.draw_form(Vec3::new(3.5, 0.5, 30.0), Vec3::NEG_Z);
        assert_eq!(ed.form_box(), Some((IVec3::new(1, 0, 5), IVec3::new(3, 0, 5))), "one deep at first");
        ed.set_drawn(false);
        assert!(ed.drawing.is_none() && ed.fresh.is_some());
        // Up, and across on both sides of the pivot (the model stands where
        // it stood): its voxels a column to the right.
        assert_eq!((ed.grid(), ed.moved), (IVec3::new(5, 2, 6), IVec3::X), "grown right and up to hold it");
        assert!((2..=4).all(|x| green(&ed, x, 5)) && ed.steps() == 1);
        assert_eq!(ed.form_box(), Some((IVec3::new(2, 0, 5), IVec3::new(4, 0, 5))), "fresh where it stands now");
        // Fresh: a voxel deeper, in place, still one step.
        ed.deepen(1);
        assert_eq!((ed.grid(), ed.moved, ed.steps(), ed.depth), (IVec3::new(5, 2, 7), IVec3::X, 1, 2));
        assert!((2..=4).all(|x| green(&ed, x, 5) && green(&ed, x, 6)));
        ed.deepen(1);
        ed.deepen(-1);
        assert!(!green(&ed, 3, 7) && green(&ed, 3, 6) && ed.steps() == 1, "back to two deep");
        // UNDO: the whole box away.
        ed.undo();
        assert_eq!((ed.grid(), ed.steps(), ed.fresh), (first, 0, None));
        // A new one, two deep (the depth used last), then another drag: the
        // first stays as it is, + acts on the second.
        ed.begin_form(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z }, false);
        ed.draw_form(Vec3::new(3.5, 0.5, 30.0), Vec3::NEG_Z);
        ed.set_drawn(false);
        assert_eq!(ed.form_box(), Some((IVec3::new(2, 0, 5), IVec3::new(4, 0, 6))));
        ed.begin_form(Hit { cell: IVec3::new(0, 1, 4), normal: IVec3::Z }, false);
        ed.set_drawn(false);
        ed.deepen(1);
        assert!(green(&ed, 3, 6) && !green(&ed, 3, 7), "the first as it was");
        assert_eq!(ed.steps(), 2);
        // Escape: fresh no more; + is the next footprint's depth.
        assert!(ed.drop_form());
        let shape = ed.shape().unwrap().clone();
        ed.deepen(1);
        assert!(*ed.shape().unwrap() == shape && ed.depth == 4);
        ed.depth = 2;
        // Ctrl: cut into the voxels as drawn.
        ed.begin_form(Hit { cell: IVec3::new(3, 0, 6), normal: IVec3::Z }, true);
        ed.draw_form(Vec3::new(4.5, 0.5, 30.0), Vec3::NEG_Z);
        ed.set_drawn(false);
        assert!((3..=4).all(|x| ed.shape().unwrap().get(IVec3::new(x, 0, 6)) == 0 && ed.shape().unwrap().get(IVec3::new(x, 0, 5)) == 0), "cut two deep");
        ed.undo();
        assert!(green(&ed, 3, 6), "the cut taken back");
        // Alt: only where empty; the box's own voxels keep their colour.
        ed.colour = [255, 255, 0];
        ed.depth = 1;
        ed.begin_form(Hit { cell: IVec3::new(3, 0, 4), normal: IVec3::Z }, false);
        ed.set_drawn(true);
        assert!(green(&ed, 3, 5), "filled only where empty");
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

    /// BEVEL on a fresh box cuts its edges in place, one step still; a
    /// sphere set the same way, in a block.
    #[test]
    fn bevel_and_sphere_act_on_the_fresh_form() {
        let dir = model_folder("bevel");
        let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
        ed.choose(Tool::Box);
        ed.depth = 3;
        ed.begin_form(Hit { cell: IVec3::new(0, 0, 4), normal: IVec3::Z }, false);
        ed.draw_form(Vec3::new(2.5, 2.5, 30.0), Vec3::NEG_Z);
        ed.set_drawn(false);
        let (lo, hi) = ed.form_box().unwrap();
        let count = |ed: &Editor| (lo.z..=hi.z).flat_map(|z| (lo.y..=hi.y).flat_map(move |y| (lo.x..=hi.x).map(move |x| IVec3::new(x, y, z)))).filter(|c| ed.shape().unwrap().get(*c) != 0).count();
        assert_eq!(count(&ed), 27);
        ed.toggle_bevel();
        assert_eq!((count(&ed), ed.steps()), (7, 1), "its edges and corners cut, one step");
        ed.choose(Tool::Sphere);
        assert!(ed.fresh.is_none(), "another tool: fresh no more");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Shift held makes the tool PICK, a click picks the colour under the
    /// mouse and the tool stays PICK; letting go brings the tool before
    /// back, for BOX with its shape still held; a stroke in the middle is
    /// not interrupted; a tool chosen meanwhile stays.
    #[test]
    fn shift_held_picks_and_letting_go_gives_the_tool_back() {
        let dir = model_folder("shift_pick");
        let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
        let top = Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z };
        ed.choose(Tool::Paint);
        ed.colour = [10, 200, 10];
        ed.press(top);
        ed.release();
        ed.choose(Tool::Remove);
        ed.colour = [0, 0, 0];
        ed.shift_pick(true);
        assert_eq!(ed.tool, Tool::Pick, "Shift held: PICK");
        ed.press(top);
        assert_eq!((ed.colour, ed.tool), ([10, 200, 10], Tool::Pick), "picked, still PICK while Shift is held");
        ed.press(top);
        assert_eq!(ed.tool, Tool::Pick, "another pick, the same");
        ed.shift_pick(false);
        assert_eq!((ed.tool, ed.colour), (Tool::Remove, [10, 200, 10]), "let go: the tool before, the color picked");
        // Shift again and again changes nothing in between; PICK chosen by
        // hand is not given back by it.
        ed.shift_pick(false);
        assert_eq!(ed.tool, Tool::Remove);
        ed.choose(Tool::Pick);
        ed.shift_pick(true);
        ed.shift_pick(false);
        assert_eq!(ed.tool, Tool::Pick, "PICK chosen is PICK until a pick");
        ed.choose(Tool::Paint);
        // A stroke under way keeps its tool.
        ed.press(top);
        ed.shift_pick(true);
        assert_eq!(ed.tool, Tool::Paint, "not in the middle of a stroke");
        ed.release();
        ed.shift_pick(false);
        // A tool chosen with Shift held stays when it is let go.
        ed.shift_pick(true);
        ed.choose(Tool::Add);
        ed.shift_pick(false);
        assert_eq!(ed.tool, Tool::Add);
        // BOX: picking and back.
        ed.choose(Tool::Box);
        ed.colour = [0, 0, 0];
        ed.shift_pick(true);
        assert!(!ed.forms(), "no shape drawn while PICK");
        ed.press(top);
        ed.shift_pick(false);
        assert_eq!((ed.tool, ed.colour), (Tool::Box, [10, 200, 10]));
        assert!(ed.forms());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A colour's material set by its words, a slider dragged one step
    /// back; a colour no voxel has stays in the palette until taken out.
    #[test]
    fn a_colours_material_is_set_and_a_colour_taken_out() {
        let dir = model_folder("material");
        let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
        ed.colour = [200, 40, 40];
        let m = ed.material().unwrap();
        ed.set_material(super::super::frames::with_word(m, 1, 0.5));
        ed.set_material(super::super::frames::with_word(ed.material().unwrap(), 1, 0.8));
        assert_eq!(ed.steps(), 1, "one step for the slider");
        let text = std::fs::read_to_string(dir.join("thing.vvx")).unwrap();
        assert!(text.contains("a = { color = \"#c82828\", metal = 0.8, rough = 0.3 }"), "{text}");
        ed.undo();
        assert_eq!(ed.material(), Some(m));
        assert!(std::fs::read_to_string(dir.join("thing.vvx")).unwrap().contains("a = \"#c82828\""));
        // A new colour painted, then erased: it keeps its place, then goes.
        ed.colour = [1, 2, 3];
        ed.press(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z });
        ed.release();
        ed.choose(Tool::Remove);
        ed.press(Hit { cell: IVec3::new(1, 0, 5), normal: IVec3::Z });
        ed.release();
        assert!(ed.colours().1.contains(&[1, 2, 3]) && std::fs::read_to_string(dir.join("thing.vvx")).unwrap().contains("#010203"));
        ed.forget_colour([1, 2, 3]);
        assert!(!ed.colours().1.contains(&[1, 2, 3]) && !std::fs::read_to_string(dir.join("thing.vvx")).unwrap().contains("#010203"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A model opens at its first frame, any size; ADD on its top grows
    /// its box (every frame's), frames are copied, taken out and turned,
    /// each a step saved at once; the file keeps its glass.
    #[test]
    fn a_model_of_any_size_is_edited_and_saved() {
        let dir = model_folder("model");
        let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
        // (Tools that change voxels: POINTER is the default.)
        ed.choose(crate::editor::model::Tool::Add);
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
        assert_eq!(ed.grid(), IVec3::new(1, 2, 6), "cut to the voxels of both frames, about the pivot (the near row of two)");
        // Saved: the model as the game reads it.
        let back = crate::vox::load_model(&dir.join("thing.vvx")).unwrap();
        assert_eq!((back.frames.len(), back.size), (2, IVec3::new(1, 2, 6)));
        let top = back.frames[0].iter().find(|(p, _)| *p == IVec3::new(0, 0, 4)).unwrap().1;
        assert_eq!(top, [120, 200, 255, 128], "glass stays glass");
        assert!(back.frames[0].iter().any(|(p, c)| *p == IVec3::new(0, 0, 5) && c[3] == 255));
        // ADD to the left of the box grows it there: the voxels move right
        // by one, and the view with them.
        ed.moved = IVec3::ZERO;
        ed.press(Hit { cell: IVec3::new(0, 0, 2), normal: IVec3::NEG_X });
        ed.release();
        assert_eq!((ed.grid(), ed.moved), (IVec3::new(2, 2, 6), IVec3::X), "grown to the left");
        let frame = ed.shape().unwrap();
        assert_eq!((frame.colour_at(IVec3::new(0, 0, 2)), frame.get(IVec3::new(1, 0, 2)) != 0), (Some([200, 40, 40]), true));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

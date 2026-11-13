//! Levels built of voxel blocks in a 3D grid (`docs/tiles.md`). A level
//! file (`levels/NAME.toml`, text: `text`) names its blocks
//! (`blocks/NAME.vvx`) and what each is to the game, holds a block and its
//! orientation for every cell, and the things placed freely, in voxels. Loading stamps the blocks into the world's
//! voxels, so destruction, physics and meshing go on as ever; the cells'
//! kinds stay for the game's rules (`CellKinds`: ladders, hazards).
//!
//! The grid lies x across, y in depth (row 0 at the front, where the
//! camera looks from), z up. Pictures far behind the last row
//! (`backdrops`) give parallax through the perspective; `margin` voxels
//! left and right of the grid hold them where the view reaches past the
//! level's edges, `below` voxels of open space under it make pits deep
//! enough to fall out of.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use glam::{IVec3, Vec3};
use serde::{Deserialize, Serialize};

use crate::blocks::{self, Block, Turn};
use crate::config::{Backdrop, BlocksConfig};
use crate::files::Files;
use crate::world::{CHUNK, Marker, World};

/// What a cell is to the game's rules (bits, `CellKinds`).
pub const SOLID: u8 = 1;
pub const ONEWAY: u8 = 2;
pub const LADDER: u8 = 4;
pub const HAZARD: u8 = 8;

/// What a block is to the game, set once for the block: solid; held from
/// above only (a plank: the top voxel of each column, as it lies); climbed
/// (a ladder: passed through); hurting (spikes: solid too); or decoration
/// a body passes through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Solid,
    Oneway,
    Ladder,
    Hazard,
    Decor,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Solid, Kind::Oneway, Kind::Ladder, Kind::Hazard, Kind::Decor];

    /// Its bits in `CellKinds`.
    pub fn flags(self) -> u8 {
        match self {
            Kind::Solid => SOLID,
            Kind::Oneway => ONEWAY,
            Kind::Ladder => LADDER,
            Kind::Hazard => HAZARD,
            Kind::Decor => 0,
        }
    }

    /// As the level file writes it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Solid => "solid",
            Kind::Oneway => "oneway",
            Kind::Ladder => "ladder",
            Kind::Hazard => "hazard",
            Kind::Decor => "decor",
        }
    }
}

/// The level's cells by what they are (`SOLID`, `ONEWAY`, `LADDER`,
/// `HAZARD`), for the game's rules: climbing, getting hurt.
#[derive(Clone, Debug, PartialEq)]
pub struct CellKinds {
    /// The world position of cell (0, 0, 0)'s low corner.
    pub origin: IVec3,
    /// A cell's size in voxels, each way.
    pub cell: IVec3,
    /// Cells across, in depth and up.
    pub size: IVec3,
    /// x + y w + z w d.
    flags: Vec<u8>,
}

impl CellKinds {
    pub fn new(origin: IVec3, cell: IVec3, size: IVec3) -> Self {
        Self { origin, cell, size, flags: vec![0; (size.x * size.y * size.z).max(0) as usize] }
    }

    fn index(&self, c: IVec3) -> Option<usize> {
        let s = self.size;
        (c.cmpge(IVec3::ZERO).all() && c.cmplt(s).all()).then(|| (c.x + c.y * s.x + c.z * s.x * s.y) as usize)
    }

    pub fn set(&mut self, c: IVec3, flags: u8) {
        if let Some(i) = self.index(c) {
            self.flags[i] = flags;
        }
    }

    /// The bits of the cell at a world point (none outside the grid).
    pub fn at(&self, p: Vec3) -> u8 {
        let c = ((p - self.origin.as_vec3()) / self.cell.as_vec3()).floor().as_ivec3();
        self.index(c).map_or(0, |i| self.flags[i])
    }

    /// The middle of the cell at a world point.
    pub fn middle(&self, p: Vec3) -> Vec3 {
        let cell = self.cell.as_vec3();
        self.origin.as_vec3() + (((p - self.origin.as_vec3()) / cell).floor() + 0.5) * cell
    }
}

/// A block as the level names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockEntry {
    /// Its file: `blocks/NAME.vvx`.
    pub name: String,
    pub kind: Kind,
}

/// What a cell holds: no block (0), or the level's block number `block`
/// (its place in `GridLevel::blocks` plus one), lying turned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cell {
    pub block: u16,
    pub turn: Turn,
}

/// The kind every piece is (`thing.kind`): a model set down, or a light,
/// that stays where the level puts it. The engine's own, so no game names
/// a kind of its own so; `level:things("piece")` lists them all.
pub const PIECE: &str = "piece";

/// One of the four ways on the ground, by its name in a level's file: north
/// away from the camera (+y, a model's own forward), east to the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    North,
    East,
    South,
    West,
}

impl Way {
    /// In the order of their bearings, a quarter turn clockwise apart.
    pub const ALL: [Way; 4] = [Way::North, Way::East, Way::South, Way::West];

    pub fn name(self) -> &'static str {
        match self {
            Way::North => "north",
            Way::East => "east",
            Way::South => "south",
            Way::West => "west",
        }
    }

    pub fn of_name(name: &str) -> Option<Way> {
        Self::ALL.into_iter().find(|w| w.name() == name)
    }

    /// As a step on the ground: x across, y in depth.
    pub fn pair(self) -> [i32; 2] {
        match self {
            Way::North => [0, 1],
            Way::East => [1, 0],
            Way::South => [0, -1],
            Way::West => [-1, 0],
        }
    }

    pub fn of_pair(pair: [i32; 2]) -> Option<Way> {
        Self::ALL.into_iter().find(|w| w.pair() == pair)
    }

    /// Its bearing: degrees clockwise seen from above, north 0.
    pub fn bearing(self) -> f64 {
        Self::ALL.iter().position(|w| *w == self).expect("one of the four") as f64 * 90.0
    }

    /// The way nearest a bearing; a bearing right between two (45, 135 ...)
    /// goes to the one clockwise of it (45 is east), so aligning gives the
    /// same every time.
    pub fn nearest(bearing: f64) -> Way {
        let quarters = (bearing.rem_euclid(360.0) / 90.0 + 0.5).floor() as usize;
        Self::ALL[quarters % 4]
    }
}

/// Which way a piece faces: one of the four ways, or a bearing in degrees
/// (`rotation`, clockwise seen from above, north 0) where the game
/// rotates things freely (`c.camera.rotates = "free"`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Facing {
    Way(Way),
    Turn(f64),
}

impl Facing {
    pub fn bearing(self) -> f64 {
        match self {
            Facing::Way(w) => w.bearing(),
            Facing::Turn(t) => t,
        }
    }

    /// Whether it lies off the quarter turns (a game on the grid cannot
    /// show it so).
    pub fn off_grid(self) -> bool {
        matches!(self, Facing::Turn(t) if t.rem_euclid(90.0) != 0.0)
    }
}

/// The light a piece brings: where it shines from, in the model's own
/// voxels from its pivot (turning with the piece; the pivot without), how
/// far it reaches and its colour (`#rrggbb`; white without), each as the
/// file wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct ThingLight {
    pub at: Option<[i32; 3]>,
    pub radius: f64,
    pub color: Option<String>,
}

impl ThingLight {
    /// Its colour, 0 to 1 each.
    pub fn rgb(&self) -> [f32; 3] {
        self.color.as_deref().and_then(hex).map_or([1.0; 3], |c| c.map(|v| v as f32 / 255.0))
    }
}

/// A level's own daylight (`[light]` in its file), each word as `c.light`
/// and `frame:daylight` name it, as the file wrote it: the way the
/// sunlight travels (z below 0), its colour, the light every face gets,
/// how dark shadows are, the sky. What it leaves out is the game's
/// `c.light`; `frame:daylight` in `on_draw` still has the last word for a
/// picture. Only the picture: nothing in the game's rules reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Daylight {
    pub sun: Option<[f64; 3]>,
    pub sun_color: Option<String>,
    pub ambient: Option<f64>,
    pub shadow: Option<f64>,
    pub sky: Option<String>,
}

impl Daylight {
    /// The words a level's `[light]` takes, in the order the file writes them.
    pub const WORDS: [&str; 5] = ["sun", "sun_color", "ambient", "shadow", "sky"];

    /// As a picture takes it (`session::Daylight`): colours 0 to 1.
    pub fn picture(&self) -> crate::session::Daylight {
        let rgb = |c: &Option<String>| c.as_deref().and_then(hex).map(|c| c.map(|v| v as f32 / 255.0));
        crate::session::Daylight {
            sun: self.sun.map(|[x, y, z]| glam::Vec3::new(x as f32, y as f32, z as f32)),
            sun_color: rgb(&self.sun_color),
            ambient: self.ambient.map(|v| v as f32),
            shadow: self.shadow.map(|v| v as f32),
            sky: rgb(&self.sky),
        }
    }
}

/// `#rrggbb` as three bytes.
pub(crate) fn hex(text: &str) -> Option<[u8; 3]> {
    let h = text.strip_prefix('#').filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))?;
    Some([0, 2, 4].map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex digits")))
}

/// A thing placed freely: its kind's name and where it stands (the middle
/// of its feet), in voxels from the grid's low corner. A piece (`PIECE`) is
/// a model set down there by its pivot, facing a way, perhaps with lights
/// (a lamp's flame and its wider glow: `light = {...}` for one, `lights =
/// [...]` for more) and perhaps solid (`solid = true`: its voxels hold
/// bodies up and stop them), or a light alone. `name` is its name in the
/// level (else its kind's and a number, `walker_1`); `line` where the file
/// says it, for mistakes (0: from no file).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thing {
    pub kind: String,
    pub at: [i32; 3],
    #[serde(skip)]
    pub name: Option<String>,
    #[serde(skip)]
    pub model: Option<String>,
    #[serde(skip)]
    pub facing: Option<Facing>,
    /// A piece tipped further in its own directions, degrees (0 unless the
    /// file says): `pitch` its nose up, `roll` its right side down, taken
    /// as `frame:model` takes them (`session::turning`: roll, then pitch,
    /// then the turn round of `facing`), about the voxel at half its height
    /// over its pivot cell (`render::turn_centre`).
    #[serde(skip)]
    pub pitch: f64,
    #[serde(skip)]
    pub roll: f64,
    #[serde(skip)]
    pub lights: Vec<ThingLight>,
    #[serde(skip)]
    pub solid: bool,
    /// Set down by the terrain's scatter rule of this number (from 1;
    /// `scatter = 2` in the file): the editor's CLEAR of that rule takes it
    /// away again, wherever it stands now. Nothing to the game.
    #[serde(skip)]
    pub scatter: Option<u32>,
    #[serde(skip)]
    pub line: usize,
}

impl PartialEq for Thing {
    /// The same thing wherever its file says it.
    fn eq(&self, o: &Self) -> bool {
        (&self.kind, self.at, &self.name, &self.model, self.facing, self.pitch, self.roll, &self.lights, self.solid, self.scatter) == (&o.kind, o.at, &o.name, &o.model, o.facing, o.pitch, o.roll, &o.lights, o.solid, o.scatter)
    }
}

impl Thing {
    /// A thing of a kind at `at`, nothing more.
    pub fn new(kind: impl Into<String>, at: [i32; 3]) -> Self {
        Self { kind: kind.into(), at, ..Default::default() }
    }

    /// Whether it is a piece: a model set down, or a light.
    pub fn is_piece(&self) -> bool {
        self.kind == PIECE
    }

    /// Whether it is turned off the quarter turns, round (`rotation = 30`) or
    /// tipped (`pitch`, `roll`): what a game on the grid cannot show so.
    pub fn off_grid(&self) -> bool {
        self.facing.is_some_and(Facing::off_grid) || self.pitch.rem_euclid(90.0) != 0.0 || self.roll.rem_euclid(90.0) != 0.0
    }

    /// Its turn as whole quarter turns, `[turn, pitch, roll]` in degrees
    /// (`session::piece_cells`): None off the quarter turns.
    pub fn quarters(&self) -> Option<[i32; 3]> {
        if self.off_grid() {
            return None;
        }
        let whole = |d: f64| (d.rem_euclid(360.0).round() as i32) % 360;
        Some([whole(self.facing.map_or(180.0, Facing::bearing)), whole(self.pitch), whole(self.roll)])
    }

    /// How a mistake names it: its name, else its model or kind, and its
    /// line.
    pub fn called(&self) -> String {
        let what = self.name.clone().or_else(|| self.model.clone()).unwrap_or_else(|| self.kind.clone());
        if self.line > 0 { format!("'{what}' (line {})", self.line) } else { format!("'{what}'") }
    }
}

/// A named box in a level (`level.areas.NAME`): its low corner, in voxels
/// from the grid's low corner, and its size.
#[derive(Clone, Debug, Default)]
pub struct Area {
    pub name: String,
    pub at: [i32; 3],
    pub size: [i32; 3],
    pub line: usize,
}

impl PartialEq for Area {
    fn eq(&self, o: &Self) -> bool {
        (&self.name, self.at, self.size) == (&o.name, o.at, o.size)
    }
}

/// A named place a player arrives at (`world:go_to(level, place)`): where
/// its feet stand, in voxels from the grid's low corner, and the way it
/// faces (south when not given); `named`: the file says the way by its
/// name (`"east"`), else as a step (`[1, 0]`); `line` where the file says
/// it (0: from no file), as a thing's.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub name: String,
    pub at: [i32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<[i32; 2]>,
    #[serde(skip)]
    pub named: bool,
    #[serde(skip)]
    pub line: usize,
}

impl PartialEq for Place {
    /// The same place wherever its file says it.
    fn eq(&self, o: &Self) -> bool {
        (&self.name, self.at, self.facing, self.named) == (&o.name, o.at, o.facing, o.named)
    }
}

/// A level as its file holds it: blocks in a grid, things and pieces set
/// down, places and areas. A level without a grid (`n` 0) is only its
/// pieces, things, places and areas in a space `size` voxels big.
#[derive(Clone, Debug, PartialEq)]
pub struct GridLevel {
    /// A block's size: voxels each way; 0 for a level without a grid.
    pub n: i32,
    /// Cells across, in depth and up (voxels without a grid).
    pub size: IVec3,
    pub blocks: Vec<BlockEntry>,
    /// Per cell, x + y w + z w d (none without a grid).
    pub cells: Vec<Cell>,
    pub things: Vec<Thing>,
    pub places: Vec<Place>,
    pub areas: Vec<Area>,
    /// Its own daylight (`[light]`), None where the game's `c.light` is all.
    pub daylight: Option<Daylight>,
    /// Its landscape (`[terrain]`, `crate::terrain`).
    pub terrain: Option<crate::terrain::Terrain>,
}

impl GridLevel {
    pub fn new(n: i32, size: IVec3) -> Self {
        let cells = if n > 0 { (size.x * size.y * size.z).max(0) as usize } else { 0 };
        Self { n, size, blocks: Vec::new(), cells: vec![Cell::default(); cells], things: Vec::new(), places: Vec::new(), areas: Vec::new(), daylight: None, terrain: None }
    }

    /// Whether it has a grid of blocks (else only what is set down).
    pub fn has_grid(&self) -> bool {
        self.n > 0
    }

    /// Its size in voxels.
    pub fn voxels(&self) -> IVec3 {
        if self.has_grid() { self.size * self.n } else { self.size }
    }

    /// The things turned off the quarter turns (`rotation = 30`): what a game
    /// on the grid cannot show so, each as a mistake names it.
    pub fn off_grid(&self) -> Vec<&Thing> {
        // (A thing of a kind turns as its kind does: freely with `rotates =
        // "free"`, else to the nearest of the four ways.)
        self.things.iter().filter(|t| t.is_piece() && t.off_grid()).collect()
    }

    /// Cell `c`'s place in `cells` (None outside the grid).
    pub fn index(&self, c: IVec3) -> Option<usize> {
        let s = self.size;
        (self.has_grid() && c.cmpge(IVec3::ZERO).all() && c.cmplt(s).all()).then(|| (c.x + c.y * s.x + c.z * s.x * s.y) as usize)
    }

    pub fn cell(&self, c: IVec3) -> Cell {
        self.index(c).map_or(Cell::default(), |i| self.cells[i])
    }

    pub fn read(path: &Path) -> Result<Self> {
        Self::read_in(&Files::disk(), path)
    }

    /// The level at `path` of the game's files (`crate::files`).
    pub fn read_in(files: &Files, path: &Path) -> Result<Self> {
        let text = files.read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_text(&text).with_context(|| path.display().to_string())
    }

    /// The level a text says (`text::read`).
    pub fn from_text(src: &str) -> Result<Self> {
        Ok(text::read(src)?.0)
    }

    /// The level as its file holds it, with new signs (`text::write`).
    pub fn to_text(&self) -> String {
        text::write(self, &Legend::default())
    }

    /// Writes the level at once (`blocks::save_at_once`), keeping the signs
    /// and the view of the file there (`Legend`).
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut names: Vec<&str> = self.blocks.iter().map(|b| b.name.as_str()).collect();
        names.sort_unstable();
        if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
            bail!("the level names block '{}' twice", w[0]);
        }
        blocks::save_at_once(path, text::write(self, &Legend::of_file(path)).as_bytes())
    }

    /// Its blocks as made, read from the game folder `dir`.
    pub fn load_blocks(&self, dir: &Path) -> Result<Vec<Block>> {
        self.load_blocks_in(&Files::folder(dir))
    }

    /// Its blocks as made, of the game's files.
    pub fn load_blocks_in(&self, files: &Files) -> Result<Vec<Block>> {
        self.blocks.iter().map(|b| Block::load_in(files, &blocks::path(files.dir(), &b.name), self.n)).collect()
    }
}

/// The ending of a level's file (`levels/NAME.toml`).
pub const EXTENSION: &str = "toml";
/// The format and the version this engine reads and writes.
pub const FORMAT: &str = "voxcade level 1";

/// The line that heads a level's `[legend]` as the engine writes it.
pub const LEGEND_HEAD: &str = "[legend]              # a sign is a block as it lies: its name, or the name and an orientation (0 to 47)";
const FORMAT_NAME: &str = "voxcade level";
const VERSION: u32 = 1;

/// Which way a level file's layers read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// Each layer the grid seen from above, the bottom layer first; in a
    /// layer the far row first (as a model's `.vvx`).
    #[default]
    Above,
    /// Each layer the grid seen from the front, the front layer first; in
    /// a layer the top row first: a side view's room as a picture.
    Front,
}

impl View {
    fn name(self) -> &'static str {
        match self {
            View::Above => "above",
            View::Front => "front",
        }
    }

    /// The way a level without a file reads best: from the front when it
    /// stands taller than it is deep (a side view's room), else from above.
    pub fn for_size(size: IVec3) -> Self {
        if size.z > size.y { View::Front } else { View::Above }
    }
}

/// What a level's file says besides the level: which way its layers read
/// and its signs, each a block's name and orientation. Kept when the editor
/// writes the level again, so a changed cell is a changed character.
#[derive(Clone, Debug, Default)]
pub struct Legend {
    pub view: Option<View>,
    pub signs: Vec<(String, String, u8)>,
}

impl Legend {
    /// The legend of a level's text; none when it does not read.
    pub fn of_text(src: &str) -> Self {
        text::read(src).map(|(_, legend)| legend).unwrap_or_default()
    }

    /// The legend of the level file at `path`, if there is one.
    pub fn of_file(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|s| Self::of_text(&s)).unwrap_or_default()
    }
}

/// A level of blocks as text (`levels/NAME.toml`, "voxcade level 1"):
///
/// ```toml
/// format = "voxcade level 1"
/// block = 10            # a block is 10 voxels each way
/// size = [24, 2, 13]    # cells: across (x), deep (y, 0 in front), up (z)
/// view = "front"        # each layer the grid seen from the front, front layer first
///
/// [blocks]              # blocks/NAME.vvx, each what it is to the game
/// wall = "decor"
/// stone = "solid"
///
/// [legend]              # a sign is a block as it lies: its name, or the name and an orientation (0 to 47)
/// w = "wall"
/// s = "stone"
/// S = { block = "stone", orientation = 17 }
///
/// [cells]
/// layers = """
/// ssssssssssssssssssssssss
/// s.....................ws
/// ...
/// """
///
/// [[thing]]
/// kind = "gem"
/// at = [45, 5, 12]      # where its feet stand, in voxels from the grid's low corner
///
/// [[thing]]             # a piece: a model set down by its pivot, fixed
/// name = "lamp"         # its name in the level (level:thing("lamp")); else its kind's and a number
/// model = "room/lamp.vvx"
/// at = [212, 40, 0]
/// facing = "west"       # north, east, south (the default) or west; or rotation = 30 where c.camera.rotates = "free"
/// light = { at = [3, 3, 41], radius = 60, color = "#ffd9a0" }   # at: from the pivot in the model's voxels
///
/// [[thing]]             # a piece with two lights that holds bodies up and stops them
/// name = "stove"
/// model = "room/stove.vvx"
/// at = [40, 150, 0]
/// solid = true          # its voxels block, exactly where the model is drawn (quarter turns only)
/// lights = [
///   { at = [0, 2, 12], radius = 20, color = "#ffb060" },
///   { at = [0, 2, 14], radius = 90, color = "#ff8040" },
/// ]
///
/// [[thing]]             # a light alone
/// name = "moon"
/// at = [300, 190, 90]
/// light = { radius = 120, color = "#a0b8ff" }
///
/// [[place]]
/// name = "door"
/// at = [25, 8, 70]
/// facing = [0, -1]      # or a way's name: "south"
///
/// [[area]]              # a named box (level.areas.by_the_stove): its low corner and size
/// name = "by_the_stove"
/// at = [30, 120, 0]
/// size = [40, 30, 40]
/// ```
///
/// A level may have its own daylight, before the things: `[light]` with
/// the words of `c.light` (each left out is the game's):
///
/// ```toml
/// [light]
/// sun = [-0.4, 0.6, -1]     # the way the sunlight travels, towards the ground (z below 0)
/// sun_color = "#ffd0a0"     # its colour and strength
/// ambient = 0.3             # the light every face gets, 0 to 1
/// shadow = 0.6              # how dark shadows are, 0 to 1
/// sky = "#20304a"
/// ```
///
/// The cells are layers of rows of signs, `.` empty, an empty line between
/// layers; a row followed by ` *N` stands N times in all, a line `*N`
/// after a layer's rows the layer N times (as in a `.vvx`). `view` says
/// which way the layers read, the same for every level of whatever
/// camera: `"above"` (the bottom layer first, each seen from above with
/// the far row first) or `"front"` (the front layer first, each seen from
/// the front with the top row first); x runs left to right in both. One
/// letter or digit a sign (62), two each when a level needs more (`..`
/// empty). Things, places and areas come in their order.
///
/// A level without `block` has no grid (a room of models, a stage):
/// `size` is then its voxels, and it holds no `[blocks]`, `[legend]` or
/// `[cells]`, only what is set down in it. A piece (a `[[thing]]` with a
/// `model` or a `light` and no kind: its kind is `PIECE`) stays where it
/// is put, anywhere, on a wall or in the air; its `at` is where the
/// model's pivot stands (the cell `size / 2` across and deep on its floor,
/// `vvx::pivot`), as every thing's, so `thing.x` is `at`'s x and a half
/// (the middle of that cell) with the grid's low corner added. `rotation`
/// is a bearing, degrees clockwise seen from above: 0 faces north (a model's
/// own forward, away from the camera), 90 east, 180 south, 270 west. Names
/// are one thing's and one area's; every mistake says its line.
pub mod text;

/// A backdrop's picture, read.
pub struct Picture {
    pub how: Backdrop,
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<[u8; 4]>,
}

/// Builds the level `cfg` names: the world, its markers (`names`: the kinds
/// of things) and the files it was made from.
pub fn load(dir: &Path, cfg: &BlocksConfig, names: &[&str]) -> Result<(World, Vec<Marker>, Vec<PathBuf>)> {
    let made = load_at(&Files::folder(dir), cfg, &dir.join(&cfg.file), names, false)?;
    Ok((made.world, made.markers, made.files))
}

/// The mistake for a level whose things are turned off the quarter turns in
/// a game that sets things on the grid: which, and the two ways out.
pub fn off_grid_mistake(off: &[&Thing]) -> String {
    let shown: Vec<String> = off.iter().take(5).map(|t| t.called()).collect();
    let more = if off.len() > 5 { format!(" and {} more", off.len() - 5) } else { String::new() };
    let (which, are) = if off.len() == 1 { ("thing", "is") } else { ("things", "are") };
    format!(
        "this game sets things on the grid, in quarter turns, but {} {which} of this level {are} turned off them: {}{more}. Allow free rotation in conf.lua (c.camera.rotates = \"free\"), or open the level in the editor to align them (each to its nearest way)",
        off.len(),
        shown.join(", ")
    )
}

/// The mistake for a solid piece turned off the quarter turns: a model
/// turned freely cannot lie on the grid, so its voxels cannot block.
pub fn off_grid_solid(t: &Thing) -> String {
    let mut how = format!("turned {} degrees", t.facing.map_or(180.0, Facing::bearing));
    for (word, d) in [("pitch", t.pitch), ("roll", t.roll)] {
        if d != 0.0 {
            how += &format!(", {word} {d}");
        }
    }
    format!(
        "the piece {} is solid but {how}: a solid piece's voxels hold bodies up on the grid, so it turns only in quarter turns (facing = \"north\", \"east\", \"south\" or \"west\"; pitch and roll 0, 90, 180 or 270), or it is not solid",
        t.called(),
    )
}

/// The level in the file at `path` (one of a game's `levels/`): its blocks
/// in the world's voxels, its things as markers, its pieces, places and
/// areas in the world's voxels. `free`: the game turns things freely
/// (`c.camera.rotates`); else a piece turned off the quarter turns is a
/// mistake. A piece's model missing is a mistake naming its line.
pub fn load_at(game: &Files, cfg: &BlocksConfig, path: &Path, names: &[&str], free: bool) -> Result<crate::levels::Made> {
    let dir = game.dir();
    if cfg.margin < 0 || cfg.below < 0 {
        bail!("c.world.blocks: margin and below are voxels, 0 or more");
    }
    let path = path.to_path_buf();
    let level = GridLevel::read_in(game, &path)?;
    let off = level.off_grid();
    if !free && !off.is_empty() {
        bail!("{}: {}", path.display(), off_grid_mistake(&off));
    }
    let mut files = vec![path.clone()];
    files.extend(level.blocks.iter().map(|b| blocks::path(dir, &b.name)));
    // The pieces' models, which the level makes part of the game.
    for t in &level.things {
        if let Some(model) = &t.model {
            let file = dir.join(model);
            if !game.is_file(&file) {
                bail!("{}: line {}: model = \"{model}\": there is no such file in the game folder", path.display(), t.line);
            }
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    let made = level.load_blocks_in(game)?;
    let mut pictures = Vec::new();
    for b in &cfg.backdrops {
        let file = dir.join(&b.image);
        let (width, height, pixels) = crate::map::read_png_rgba_in(game, &file)?;
        files.push(file);
        pictures.push(Picture { how: b.clone(), width: width as i32, height: height as i32, pixels });
    }
    // The scatter rules' models the land bakes in are the game's files too.
    if let Some(t) = &level.terrain {
        for rule in t.scatter.iter().filter(|r| r.makes() == crate::terrain::ScatterAs::Voxels) {
            for m in &rule.models {
                let file = dir.join(m);
                if !files.contains(&file) {
                    files.push(file);
                }
            }
        }
    }
    let models = |name: &str| crate::vox::load_model_in(game, &dir.join(name));
    let mut world = build(&level, &made, cfg.margin, cfg.below, &pictures, &models)?;
    let origin = IVec3::new(cfg.margin, 0, cfg.below);
    outside_mistake(&level, origin, world.size).map_or(Ok(()), |why| Err(anyhow!("{}: {why}", path.display())))?;
    // The solid pieces' cells, exactly where their models are drawn: on
    // the grid, so in quarter turns only.
    let mut solids = Vec::new();
    for (i, t) in level.things.iter().filter(|t| t.is_piece()).enumerate() {
        let (true, Some(model)) = (t.solid, &t.model) else { continue };
        let Some(turns) = t.quarters() else { bail!("{}: {}", path.display(), off_grid_solid(t)) };
        let m = crate::vox::load_model_in(game, &dir.join(model))?;
        let cell = origin + IVec3::from(t.at);
        let cells: Vec<IVec3> = crate::session::piece_cells(&m, turns).expect("quarter turns").into_iter().map(|c| cell + c).collect();
        solids.push((i, std::sync::Arc::new(cells)));
    }
    if !solids.is_empty() {
        world.pieces = Some(std::sync::Arc::new(crate::world::Solids::new(solids)));
    }
    let markers = markers(&level, names, origin)?;
    let places = level.places.iter().map(|p| crate::levels::Place::new(&p.name, (origin + IVec3::from(p.at)).as_vec3() + glam::Vec3::new(0.5, 0.5, 0.0), p.facing)).collect();
    let marker_names = level.things.iter().filter(|t| !t.is_piece()).map(|t| t.name.clone()).collect();
    let marker_facings = level.things.iter().filter(|t| !t.is_piece()).map(|t| t.facing.map(Facing::bearing)).collect();
    let pieces = level
        .things
        .iter()
        .filter(|t| t.is_piece())
        .enumerate()
        .map(|(index, t)| crate::levels::Piece {
            name: t.name.clone(),
            model: t.model.clone(),
            cell: origin + IVec3::from(t.at),
            facing: t.facing.unwrap_or(Facing::Way(Way::South)),
            pitch: t.pitch as f32,
            roll: t.roll as f32,
            lights: t.lights.iter().map(|l| crate::levels::PieceLight { at: IVec3::from(l.at.unwrap_or_default()), radius: l.radius as f32, color: l.rgb() }).collect(),
            solid: t.solid && t.model.is_some(),
            index,
        })
        .collect();
    let areas = level.areas.iter().map(|a| crate::levels::Area { name: a.name.clone(), at: origin + IVec3::from(a.at), size: IVec3::from(a.size) }).collect();
    let daylight = level.daylight.as_ref().map(Daylight::picture).unwrap_or_default();
    Ok(crate::levels::Made { world, markers, files, places, marker_names, marker_facings, pieces, areas, daylight })
}

/// The mistake for the first thing, place or area of a level that lies
/// outside its world (`size` voxels, the grid's low corner at `origin`):
/// a thing or a place by where it stands, an area whole. Above the world
/// the sky is open (a thing stands on the top layer, a lamp hangs over a
/// floor, something comes down into the level): no limit upwards. None
/// when all lie within. The editor reads such a level all the same, to
/// drag it back.
pub fn outside_mistake(level: &GridLevel, origin: IVec3, size: IVec3) -> Option<String> {
    let v = level.voxels();
    let (low, high) = (-origin, (size - origin).with_z(i32::MAX));
    let inside = |at: IVec3, extent: IVec3| at.cmpge(low).all() && (at.as_i64vec3() + extent.as_i64vec3()).cmple(high.as_i64vec3()).all();
    let line = |n: usize| if n > 0 { format!("line {n}: ") } else { String::new() };
    let room = || format!("the level is {} by {} by {} voxels: x {} to {}, y {} to {}, z from {} up", v.x, v.y, v.z, low.x, high.x - 1, low.y, high.y - 1, low.z);
    if let Some(t) = level.things.iter().find(|t| !inside(IVec3::from(t.at), IVec3::ONE)) {
        let [x, y, z] = t.at;
        let what = t.name.clone().or_else(|| t.model.clone()).unwrap_or_else(|| t.kind.clone());
        return Some(format!("{}the thing '{what}' at [{x}, {y}, {z}] stands outside the level ({})", line(t.line), room()));
    }
    if let Some(p) = level.places.iter().find(|p| !inside(IVec3::from(p.at), IVec3::ONE)) {
        let [x, y, z] = p.at;
        return Some(format!("{}the place '{}' at [{x}, {y}, {z}] lies outside the level ({})", line(p.line), p.name, room()));
    }
    if let Some(a) = level.areas.iter().find(|a| !inside(IVec3::from(a.at), IVec3::from(a.size))) {
        let ([x, y, z], [w, d, h]) = (a.at, a.size);
        return Some(format!("{}the area '{}' at [{x}, {y}, {z}], {w} by {d} by {h}, reaches outside the level ({})", line(a.line), a.name, room()));
    }
    None
}

/// The things of kinds as markers (not the pieces), the grid's low corner
/// at `origin`.
pub fn markers(level: &GridLevel, names: &[&str], origin: IVec3) -> Result<Vec<Marker>> {
    level
        .things
        .iter()
        .filter(|t| !t.is_piece())
        .map(|t| {
            let kind = names.iter().position(|n| *n == t.kind).ok_or_else(|| {
                let at = if t.line > 0 { format!("line {}: ", t.line) } else { String::new() };
                anyhow!("{at}thing at {:?}: '{}' is no kind of thing (c.things: {})", t.at, t.kind, names.join(", "))
            })?;
            Ok(Marker { kind, cell: origin + IVec3::from(t.at) })
        })
        .collect()
}

/// The world of a level: its blocks (`made`, as the level lists them)
/// stamped into the cells as they lie, `margin` voxels left and right,
/// `below` under the grid, the backdrops behind it.
pub fn build(level: &GridLevel, made: &[Block], margin: i32, below: i32, backdrops: &[Picture], models: ModelsOf) -> Result<World> {
    let n = level.n;
    let grid = level.voxels();
    let size = IVec3::new(
        grid.x + 2 * margin,
        backdrops.iter().map(|b| grid.y + b.how.behind.max(0) + 1).max().unwrap_or(0).max(grid.y),
        backdrops.iter().map(|b| b.how.z + b.height * b.how.scale.max(1)).max().unwrap_or(0).max(below + grid.z),
    );
    let origin = IVec3::new(margin, 0, below);
    let mut world = World::new(size, [[0; 4]; 256]);

    // Each block as it lies, once for every turn the cells use, in world
    // colours (in the order the cells first use them: the same every
    // time).
    let mut slots: Vec<(Cell, Vec<u8>)> = Vec::new();
    let mut cell_slot = vec![u32::MAX; level.cells.len()];
    for (i, &cell) in level.cells.iter().enumerate() {
        if cell.block == 0 {
            continue;
        }
        let slot = match slots.iter().position(|(c, _)| *c == cell) {
            Some(s) => s,
            None => {
                let entry = &level.blocks[cell.block as usize - 1];
                let block = made.get(cell.block as usize - 1).context("a block of the level is missing")?.turned(cell.turn);
                let voxels = stamp(&mut world, &block, entry.kind).ok_or_else(|| {
                    anyhow!("the level has more than 255 colors with block '{}' (a color counts once for each way it holds: solid, held from above, passed through)", entry.name)
                })?;
                slots.push((cell, voxels));
                slots.len() - 1
            }
        };
        cell_slot[i] = slot as u32;
    }

    // The backdrops' voxels: each a wall one voxel thick, its picture
    // repeated across the world.
    let mut walls = Vec::new();
    for b in backdrops {
        let s = b.how.scale.max(1);
        let mut wall = vec![0u8; (size.x * b.height * s) as usize];
        for z in 0..b.height * s {
            for x in 0..size.x {
                let (u, v) = ((x / s).rem_euclid(b.width), b.height - 1 - z / s);
                let [r, g, bl, a] = b.pixels[(u + v * b.width) as usize];
                if a >= 128 {
                    wall[(x + z * size.x) as usize] = world.colour_as([r, g, bl], true, false).ok_or_else(|| anyhow!("the level has more than 255 colors with backdrop '{}'", b.how.image))?;
                }
            }
        }
        walls.push((grid.y + b.how.behind.max(0), b.how.z, b.height * s, wall));
    }

    // The land, its colours after the blocks'; then its scatter rules'
    // voxels (boulders, stumps), baked into it.
    let (land, baked) = match &level.terrain {
        Some(t) => {
            let surface = t.surface();
            let painter = t.painter(&mut world, origin, surface.clone())?;
            let baked = bake(&mut world, t, origin, &surface, models)?;
            (Some(painter), baked)
        }
        None => (None, crate::world::CellMap::default()),
    };

    // Into the world chunk by chunk: the grid's rows, the walls' rows and
    // the land's chunks.
    let block_voxel = |p: IVec3| -> u8 {
        if p.y < grid.y {
            if !level.has_grid() {
                return 0;
            }
            let q = p - origin;
            let Some(i) = level.index(q.div_euclid(IVec3::splat(n))) else { return 0 };
            return match cell_slot[i] {
                u32::MAX => 0,
                slot => {
                    let l = q.rem_euclid(IVec3::splat(n));
                    slots[slot as usize].1[(l.x + l.y * n + l.z * n * n) as usize]
                }
            };
        }
        for (y, z0, height, wall) in &walls {
            if p.y == *y && p.z >= *z0 && p.z < z0 + height {
                return wall[(p.x + (p.z - z0) * size.x) as usize];
            }
        }
        0
    };
    // (Where the blocks leave a voxel empty, what is baked in, else the
    // land's; the columns with something baked looked up only.)
    let mut baked_columns = vec![false; (size.x * size.y).max(0) as usize];
    for p in baked.keys() {
        baked_columns[(p.x + p.y * size.x) as usize] = true;
    }
    let voxel = |p: IVec3| -> u8 {
        match block_voxel(p) {
            0 => match baked_columns[(p.x + p.y * size.x) as usize].then(|| baked.get(&p)).flatten() {
                Some(&v) => v,
                None => land.as_ref().map_or(0, |l| l.voxel(p)),
            },
            v => v,
        }
    };
    // (A level without a grid has nothing to fill but its backdrops and
    // its land.)
    let mut rows_y: Vec<i32> = if level.has_grid() { (0..(grid.y + CHUNK - 1) / CHUNK).collect() } else { Vec::new() };
    rows_y.extend(walls.iter().map(|(y, ..)| y / CHUNK));
    rows_y.sort_unstable();
    rows_y.dedup();
    let chunks = (size + CHUNK - 1) / CHUNK;
    let mut keys: Vec<IVec3> = Vec::new();
    for &cy in &rows_y {
        for cz in 0..chunks.z {
            for cx in 0..chunks.x {
                keys.push(IVec3::new(cx, cy, cz));
            }
        }
    }
    if let Some(l) = &land {
        keys.extend(l.chunks().into_iter().filter(|c| c.cmplt(chunks).all()));
        keys.extend(baked.keys().map(|&p| World::chunk_of(p)).filter(|c| c.cmpge(IVec3::ZERO).all() && c.cmplt(chunks).all()));
        keys.sort_unstable_by_key(|c| (c.y, c.z, c.x));
        keys.dedup();
    }
    for key in keys {
        world.fill_chunk(key, voxel);
    }

    world.stage = Some((origin, origin + grid));
    if !level.has_grid() {
        world.mark_all_dirty();
        return Ok(world);
    }
    let mut kinds = CellKinds::new(origin, IVec3::splat(n), level.size);
    for z in 0..level.size.z {
        for y in 0..level.size.y {
            for x in 0..level.size.x {
                let c = IVec3::new(x, y, z);
                let cell = level.cell(c);
                if cell.block != 0 {
                    kinds.set(c, level.blocks[cell.block as usize - 1].kind.flags());
                }
            }
        }
    }
    world.cells = Some(Arc::new(kinds));
    world.mark_all_dirty();
    Ok(world)
}

/// Where a level's scatter rules' models come from: a model's file in the
/// game folder, read (`no_models` for none).
pub type ModelsOf<'a> = &'a dyn Fn(&str) -> Result<crate::vox::VoxModel>;

/// For a level without models to bake.
pub fn no_models(name: &str) -> Result<crate::vox::VoxModel> {
    bail!("{name}: no models here")
}

/// The voxels the terrain's scatter rules of `as = "voxels"` bake into the
/// land (boulders, stumps): each model where the rule sets it down, its
/// pivot on the land (sunk `sink`), turned in quarters (`rotate = "any"`
/// too: voxels stay on the grid), in the world's palette after the land's
/// colors. A rule's models beyond 255 colors: a mistake naming the rule.
fn bake(world: &mut World, t: &crate::terrain::Terrain, origin: IVec3, surface: &crate::terrain::Surface, models: ModelsOf) -> Result<crate::world::CellMap<u8>> {
    let mut out = crate::world::CellMap::default();
    let corner = origin + IVec3::from(t.at);
    for (k, rule) in t.scatter.iter().enumerate() {
        if rule.makes() != crate::terrain::ScatterAs::Voxels {
            continue;
        }
        let mut shapes = Vec::new();
        for name in &rule.models {
            let m = models(name).with_context(|| format!("the terrain's scatter {}: model = \"{name}\"", k + 1))?;
            let pivot = IVec3::new(m.size.x / 2, m.size.y / 2, 0);
            let mut cells = Vec::new();
            for &(p, c) in m.frames.first().map_or(&[][..], |f| &f[..]) {
                let v = world.colour_as([c[0], c[1], c[2]], false, false).ok_or_else(|| anyhow!("the level has more than 255 colors with the terrain's scatter {} ({name}): fewer colors in its models, or as = \"pieces\"", k + 1))?;
                cells.push((p - pivot, v));
            }
            shapes.push(cells);
        }
        let sink = rule.sink.unwrap_or(0);
        for placed in t.scatter(k, surface) {
            let base = corner + IVec3::new(placed.column.x, placed.column.y, placed.ground - sink);
            let quarters = ((placed.rotation / 90.0).round() as i32).rem_euclid(4);
            for &(rel, v) in &shapes[placed.model] {
                let mut r = rel;
                for _ in 0..quarters {
                    r = IVec3::new(r.y, -r.x, r.z);
                }
                let p = base + r;
                if world.in_bounds(p) {
                    out.insert(p, v);
                }
            }
        }
    }
    Ok(out)
}

/// A block's voxels (as it lies) in the world's colours, held as its kind
/// says: of a plank only the top voxel of each column holds, from above;
/// ladders and decoration are passed through. None: the world's 255
/// colours are used up.
pub(crate) fn stamp(world: &mut World, block: &Block, kind: Kind) -> Option<Vec<u8>> {
    let s = block.size;
    let mut out = vec![0u8; block.voxels.len()];
    for y in 0..s.y {
        for x in 0..s.x {
            let mut top = true;
            for z in (0..s.z).rev() {
                let p = IVec3::new(x, y, z);
                let Some([r, g, b]) = block.colour_at(p) else { continue };
                let (soft, oneway) = match kind {
                    Kind::Solid | Kind::Hazard => (false, false),
                    Kind::Oneway => (!top, top),
                    Kind::Ladder | Kind::Decor => (true, false),
                };
                top = false;
                out[block.index(p).unwrap()] = world.colour_as([r, g, b], soft, oneway)?;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A block of size `n` with one colour in the cells `filled` names.
    fn block(n: i32, colour: [u8; 3], filled: impl Fn(IVec3) -> bool) -> Block {
        let mut b = Block::empty(n);
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let p = IVec3::new(x, y, z);
                    if filled(p) {
                        b.set(p, Some(colour)).unwrap();
                    }
                }
            }
        }
        b
    }

    fn level(n: i32, size: IVec3, blocks: &[(&str, Kind)]) -> GridLevel {
        let mut level = GridLevel::new(n, size);
        level.blocks = blocks.iter().map(|&(name, kind)| BlockEntry { name: name.into(), kind }).collect();
        level
    }

    fn put(level: &mut GridLevel, c: IVec3, block: u16, turn: Turn) {
        let i = level.index(c).unwrap();
        level.cells[i] = Cell { block, turn };
    }

    #[test]
    fn a_grid_level_reads_back_as_written() {
        let mut lv = level(4, IVec3::new(5, 2, 3), &[("79", Kind::Solid), ("plank", Kind::Oneway)]);
        put(&mut lv, IVec3::new(0, 0, 0), 1, Turn::NONE);
        put(&mut lv, IVec3::new(1, 0, 0), 1, Turn::NONE);
        put(&mut lv, IVec3::new(4, 1, 2), 2, Turn::from_tiled(true, false, true));
        lv.things.push(Thing::new("fox", [6, 2, 4]));
        lv.places.push(Place { name: "door".into(), at: [1, 2, 3], facing: Some([0, -1]), named: false, line: 0 });
        let text = lv.to_text();
        assert_eq!(GridLevel::from_text(&text).unwrap(), lv, "{text}");
        // Taller than deep: seen from the front, the top row first; the
        // block's name gives its sign, a turned one the capital.
        assert!(text.contains("view = \"front\"") && text.contains("\n79 = \"solid\"") && text.contains("plank = \"oneway\""), "{text}");
        assert!(text.contains("p = { block = \"plank\", orientation = ") && text.contains("7 = \"79\""), "{text}");
        assert!(text.contains("layers = \"\"\"\n.....\n.....\n77...\n\n....p\n.....\n.....\n\"\"\""), "{text}");
        assert!(text.contains("[[thing]]\nkind = \"fox\"\nat = [6, 2, 4]") && text.contains("facing = [0, -1]"), "{text}");
        // Mistakes say the line.
        let e = format!("{:#}", GridLevel::from_text(&text.replace("77...", "7x...")).unwrap_err());
        assert!(e.starts_with("line ") && e.contains("'x' is no sign of the legend"), "{e}");
        let e = format!("{:#}", GridLevel::from_text(&text.replace("77...", "77..")).unwrap_err());
        assert!(e.contains("a row here is 5 cells"), "{e}");
        assert!(format!("{:#}", GridLevel::from_text(&text.replace("voxcade level 1", "voxcade level 2")).unwrap_err()).contains("newer Voxcade"));
    }

    /// Written again, a level keeps its view and its signs, a changed cell
    /// a changed character.
    #[test]
    fn a_level_keeps_its_signs_and_view() {
        let mut lv = level(4, IVec3::new(3, 3, 1), &[("stone", Kind::Solid), ("sand", Kind::Solid)]);
        put(&mut lv, IVec3::new(0, 0, 0), 1, Turn::NONE);
        put(&mut lv, IVec3::new(1, 0, 0), 2, Turn::NONE);
        let text = lv.to_text();
        assert!(text.contains("view = \"above\"") && text.contains("s = \"stone\"\nS = \"sand\""), "{text}");
        let kept = Legend::of_text(&text.replace("s = \"stone\"", "q = \"stone\"").replace("sS.", "qS."));
        put(&mut lv, IVec3::new(2, 2, 0), 1, Turn::NONE);
        let again = text::write(&lv, &kept);
        assert!(again.contains("q = \"stone\"\nS = \"sand\"") && again.contains("..q\n...\nqS."), "{again}");
        let front = Legend { view: Some(View::Front), signs: Vec::new() };
        assert!(text::write(&lv, &front).contains("view = \"front\""));
        assert_eq!(GridLevel::from_text(&text::write(&lv, &front)).unwrap(), lv);
    }

    /// The sketch of a level of pieces, as written.
    pub(crate) const PIECES: &str = r##"format = "voxcade level 1"
size = [360, 200, 120]    # voxels: across (x), deep (y, 0 in front), up (z); no grid of blocks

[[thing]]
name = "stove"
model = "room/stove.vvx"
at = [40, 150, 0]
facing = "south"

[[thing]]
name = "lamp"
model = "room/lamp.vvx"
at = [212, 40, 0]
light = { at = [3, 3, 41], radius = 60, color = "#ffd9a0" }

[[thing]]
name = "moon"
at = [300, 190, 90]
light = { radius = 120.5, color = "#a0b8ff" }

[[thing]]
name = "chair"
model = "room/chair.vvx"
at = [60, 150, 0]
rotation = 45

[[thing]]
kind = "walker"
at = [100, 20, 0]

[[place]]
name = "door"
at = [30, 12, 0]
facing = "east"

[[place]]
name = "window"
at = [30, 12, 0]
facing = [1, 1]

[[area]]
name = "by_the_stove"
at = [30, 120, 0]
size = [40, 30, 40]
"##;

    /// A level of pieces reads as its file says and is written back the
    /// same, to the character: what the editor keeps of it.
    #[test]
    fn a_level_of_pieces_reads_back_as_written() {
        let lv = GridLevel::from_text(PIECES).unwrap();
        assert!(!lv.has_grid() && lv.voxels() == IVec3::new(360, 200, 120) && lv.cells.is_empty());
        assert_eq!(lv.to_text(), PIECES);
        let lamp = &lv.things[1];
        assert_eq!((lamp.kind.as_str(), lamp.name.as_deref(), lamp.line), (PIECE, Some("lamp"), 11));
        assert_eq!(lamp.lights.iter().map(|l| (l.at, l.radius, l.rgb())).collect::<Vec<_>>(), [(Some([3, 3, 41]), 60.0, [1.0, 217.0 / 255.0, 160.0 / 255.0])]);
        assert_eq!((lv.things[2].model.as_ref(), lv.things[2].is_piece()), (None, true), "a light alone is a piece");
        assert_eq!(lv.things[3].facing, Some(Facing::Turn(45.0)));
        assert_eq!(lv.things[4], Thing::new("walker", [100, 20, 0]));
        assert_eq!((lv.places[0].facing, lv.places[0].named), (Some([1, 0]), true));
        assert_eq!(lv.areas[0], Area { name: "by_the_stove".into(), at: [30, 120, 0], size: [40, 30, 40], line: 0 });
        // A level of blocks keeps them too, after its cells.
        let mut blocks = GridLevel::new(4, IVec3::new(2, 1, 1));
        blocks.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
        (blocks.things, blocks.places, blocks.areas) = (lv.things.clone(), lv.places.clone(), lv.areas.clone());
        let text = blocks.to_text();
        assert_eq!(GridLevel::from_text(&text).unwrap(), blocks);
        assert!(text.ends_with(PIECES.split_once("\n\n").unwrap().1), "{text}");
    }

    /// The lines of `b` that are not those of `a` at the same place, and
    /// how many lines each has: what a rewrite touched.
    /// A level of a terrain, commented, with a thing after it.
    const LAND: &str = "format = \"voxcade level 1\"\nsize = [96, 64, 80]\n\n# the valley\n[terrain]\nspacing = 16   # voxels between points\nheights = \"\"\"\n 4  6 10 20 30 30\n 4  8 14 24 36 40\n 4  6 10 20 30 30\n 2  2  2  2  2  2\n\"\"\"\nseed = 7\n\n# snow\n[[terrain.material]]\ncolor = \"#eef2f5\"\nabove = 35   # the tops\n\n[[terrain.material]]\ncolor = \"#5c8a3a\"\n\n[[thing]]\nname = \"hut\"\nmodel = \"hut.vvx\"\nat = [10, 10, 4]\n";

    /// The terrain written by the editor: only its own lines change, the
    /// comments kept; materials coming and going written anew from the
    /// first; a terrain put in before what is set down, or taken out.
    #[test]
    fn a_terrain_rewrite_touches_only_its_lines() {
        use crate::terrain::{Material, Shape};
        let src = LAND.to_string();
        let was = GridLevel::from_text(&src).unwrap();
        let legend = Legend::default();
        let check = |now: &GridLevel| {
            let out = text::rewrite(&src, &was, now, &legend);
            assert_eq!(&GridLevel::from_text(&out).unwrap(), now, "{out}");
            out
        };
        // A point raised: the heights' grid, lined up anew.
        let mut now = was.clone();
        let t = now.terrain.as_mut().unwrap();
        t.heights[(1 + 2 * t.cols) as usize] = 70;
        let out = check(&now);
        assert_eq!(changed_lines(&src, &out), [" 4 70 14 24 36 40"], "{out}");
        assert!(out.contains("spacing = 16   # voxels between points") && out.contains("# snow\n") && out.contains("above = 35   # the tops"));
        // A shape: its grid put in after the heights.
        let mut now = was.clone();
        now.terrain.as_mut().unwrap().shapes[0] = Shape::Step;
        let out = check(&now);
        assert!(out.contains("\"\"\"\nshapes = \"\"\"\n .  .  .  .  .  .\n") && out.contains(" #  .  .  .  .  .\n\"\"\"\nseed = 7"), "lined up with the heights: {out}");
        // A material's field: its line alone.
        let mut now = was.clone();
        now.terrain.as_mut().unwrap().materials[0].above = Some(50);
        let out = check(&now);
        assert_eq!(changed_lines(&src, &out), ["above = 50   # the tops"]);
        now.terrain.as_mut().unwrap().materials[1].steeper = None;
        now.terrain.as_mut().unwrap().materials[0].grain = Some(0.25);
        let out = check(&now);
        assert!(out.contains("color = \"#eef2f5\"\ngrain = 0.25\nabove = 50"), "{out}");
        // A material more: written anew from the first one's header.
        let mut now = was.clone();
        now.terrain.as_mut().unwrap().materials.insert(1, Material { steeper: Some(45.0), ..Material::new("#7d7468") });
        let out = check(&now);
        assert!(out.contains("# snow\n[[terrain.material]]") && out.contains("steeper = 45\n"), "{out}");
        // Taken out, and put in again before the thing.
        let mut now = was.clone();
        now.terrain = None;
        let out = check(&now);
        assert!(!out.contains("terrain") && out.contains("[[thing]]\nname = \"hut\""), "{out}");
        let back = text::rewrite(&out, &now, &was, &legend);
        assert!(back.find("[terrain]").unwrap() < back.find("[[thing]]").unwrap(), "{back}");
        assert_eq!(GridLevel::from_text(&back).unwrap().terrain.map(|t| t.heights), was.terrain.as_ref().map(|t| t.heights.clone()));
    }

    /// A terrain beside blocks: the blocks' voxels first, the land where
    /// they leave room, both standing on the floor. Past the level's size:
    /// a mistake with its line.
    #[test]
    fn a_terrain_lies_beside_blocks_and_within_its_level() {
        let mut lv = level(4, IVec3::new(8, 4, 8), &[("rock", Kind::Solid)]);
        put(&mut lv, IVec3::new(0, 0, 4), 1, Turn::NONE);
        lv.terrain = Some(crate::terrain::Terrain::flat(8, 4, 2, 10));
        let world = build(&lv, &[block(4, [200, 0, 0], |_| true)], 0, 0, &[], &no_models).unwrap();
        assert_eq!(world.palette[1][..3], [200, 0, 0], "the blocks' colors first");
        assert_eq!(world.get(IVec3::new(1, 1, 17)), 1, "a block");
        assert_eq!(world.get(IVec3::new(9, 1, 9)), 2, "the land");
        assert_eq!(world.get(IVec3::new(9, 1, 10)), 0, "above it");
        assert_eq!(world.get(IVec3::new(9, 1, 0)), 2, "down to the floor");
        let e = GridLevel::from_text(&LAND.replace("size = [96, 64, 80]", "size = [90, 64, 80]")).unwrap_err().to_string();
        assert!(e.contains("line 5: the terrain reaches to 96 by 64 by 40 voxels"), "{e}");
    }

    /// A wide land builds fast and the same every time; a blast through a
    /// cliff leaves nothing loose (every column stands on the floor).
    #[test]
    fn a_wide_terrain_builds_fast_and_stands() {
        let (w, d) = (640 / 16, 576 / 16);
        let mut t = crate::terrain::Terrain::flat(16, w, d, 0);
        for y in 0..d {
            for x in 0..w {
                // Rolling hills and a STEP cliff across the middle.
                t.heights[(x + y * w) as usize] = 8 + ((x * 7 + y * 13) % 23) + if y > d / 2 { 40 } else { 0 };
                if y == d / 2 + 1 {
                    t.shapes[(x + y * w) as usize] = crate::terrain::Shape::Step;
                }
            }
        }
        // Every word of it at work: roughness, layers, patches, blend.
        t.materials = vec![
            crate::terrain::Material { above: Some(60), blend: Some(8.0), ..crate::terrain::Material::new("#eef2f5") },
            crate::terrain::Material { steeper: Some(45.0), blend: Some(6.0), tones: vec!["#6e665c".into()], ..crate::terrain::Material::new("#7d7468") },
            crate::terrain::Material { grain: Some(0.3), tones: vec!["#4f7d33".into(), "#6a9640".into()], patch: Some(9), ..crate::terrain::Material::new("#5c8a3a") },
        ];
        t.rough = Some(0.7);
        t.layers = vec![crate::terrain::Layer { thick: 8, hard: 0.9, color: "#9c4f2e".into() }, crate::terrain::Layer { thick: 6, hard: 0.2, color: "#d9a46a".into() }];
        let mut lv = GridLevel::new(0, IVec3::new(640, 576, 140));
        lv.terrain = Some(t);
        let start = std::time::Instant::now();
        let world = build(&lv, &[], 0, 0, &[], &no_models).unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        println!("640 by 576 voxels of land built in {ms:.0} ms, {} chunks", world.chunk_keys().len());
        assert!(ms < 3000.0, "{ms} ms");
        assert_eq!(world.content_hash(), build(&lv, &[], 0, 0, &[], &no_models).unwrap().content_hash(), "the same every time");
        // A hole through the cliff's foot: nothing above it is loose.
        let mut grounded = crate::world::CellSet::default();
        let mut w2 = world.clone();
        let foot = IVec3::new(320, 300, 10);
        for dz in 0..6 {
            for dy in -3..3 {
                w2.set(foot + IVec3::new(0, dy, dz), 0);
            }
        }
        for z in 16..40 {
            let p = IVec3::new(320, 310, z);
            if w2.get(p) != 0 {
                assert!(crate::sim::loose_piece(&w2, p, &mut grounded).is_none(), "{p} stands");
            }
        }
    }

    /// A scatter rule of voxels bakes its model into the land as it is
    /// built: the model's colors, solid, standing on the land (nothing of
    /// it loose); more than the palette holds is a mistake naming the rule.
    #[test]
    fn scatter_voxels_are_baked_into_the_land() {
        let mut t = crate::terrain::Terrain::flat(16, 8, 6, 10);
        t.scatter = vec![crate::terrain::Scatter { as_: Some(crate::terrain::ScatterAs::Voxels), apart: Some(10), ..crate::terrain::Scatter::new("rock.vvx") }];
        t.scatter[0].density = 8.0;
        let mut lv = GridLevel::new(0, IVec3::new(128, 96, 40));
        lv.terrain = Some(t.clone());
        let rock = |_: &str| ({
            let mut b = Block::empty(3);
            for z in 0..2 {
                for y in 0..3 {
                    for x in 0..3 {
                        b.set(IVec3::new(x, y, z), Some([200, 10, 10])).unwrap();
                    }
                }
            }
            b
        })
        .model();
        let world = build(&lv, &[], 0, 0, &[], &|n| Ok(rock(n))).unwrap();
        let placed = t.scatter(0, &t.surface());
        assert!(placed.len() >= 4);
        let red = (1..256).find(|&i| world.palette[i][..3] == [200, 10, 10]).unwrap() as u8;
        let mut grounded = crate::world::CellSet::default();
        for p in &placed {
            let at = IVec3::new(p.column.x, p.column.y, p.ground);
            assert_eq!(world.get(at), red, "the rock on the land at {at}");
            assert!(world.solid(at + IVec3::Z));
            assert!(crate::sim::loose_piece(&world, at + IVec3::Z, &mut grounded).is_none(), "it stands");
        }
        assert_eq!(world.content_hash(), build(&lv, &[], 0, 0, &[], &|n| Ok(rock(n))).unwrap().content_hash());
        // A model of more colors than the palette has room for.
        let many = |_: &str| {
            let frame: Vec<(IVec3, [u8; 4])> = (0..256).map(|k| (IVec3::new(k % 16, k / 16, 0), [k as u8, 7, (k / 3) as u8, 255])).collect();
            crate::vox::VoxModel::new(IVec3::new(16, 16, 1), vec![frame], vec![vec![0; 256]])
        };
        let e = build(&lv, &[], 0, 0, &[], &|n| Ok(many(n))).err().unwrap().to_string();
        assert!(e.contains("more than 255 colors with the terrain's scatter 1 (rock.vvx)"), "{e}");
    }

    fn changed_lines(a: &str, b: &str) -> Vec<String> {
        let (x, y): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
        // As many lines: those that differ; else what lies between the
        // common head and tail.
        if x.len() == y.len() {
            return x.iter().zip(&y).filter(|(p, q)| p != q).map(|(_, q)| q.to_string()).collect();
        }
        let head = x.iter().zip(&y).take_while(|(p, q)| p == q).count();
        let tail = x.iter().rev().zip(y.iter().rev()).take_while(|(p, q)| p == q).count().min(x.len().min(y.len()) - head);
        y[head..y.len() - tail].iter().map(|l| l.to_string()).collect()
    }

    /// The room's text with comments, as a person may have written it.
    fn commented() -> String {
        format!("# the kitchen\n{}", PIECES.replace("[[thing]]\nname = \"lamp\"", "# the lamp by the window\n[[thing]]\nname = \"lamp\"").replace("at = [40, 150, 0]", "at = [40, 150, 0]    # by the wall"))
    }

    /// A level written over its text keeps it to the character where it
    /// did not change, comments too; a change is its own lines only, and
    /// it reads back as the level.
    #[test]
    fn a_rewrite_touches_only_what_changed() {
        let src = commented();
        let was = GridLevel::from_text(&src).unwrap();
        let legend = Legend::default();
        assert_eq!(text::rewrite(&src, &was, &was, &legend), src, "untouched: the same text");
        let check = |now: &GridLevel, lines: &[&str]| {
            let out = text::rewrite(&src, &was, now, &legend);
            assert_eq!(&GridLevel::from_text(&out).unwrap(), now, "{out}");
            assert_eq!(changed_lines(&src, &out), lines, "{out}");
            out
        };
        // A thing moved: its at, its comment kept.
        let mut now = was.clone();
        now.things[0].at = [50, 150, 0];
        check(&now, &["at = [50, 150, 0]    # by the wall"]);
        // Turned off the grid and back on: the key and the value.
        let mut now = was.clone();
        now.things[0].facing = Some(Facing::Turn(30.0));
        check(&now, &["rotation = 30"]);
        now.things[3].facing = Some(Facing::Way(Way::West));
        let out = check(&now, &["rotation = 30", "facing = \"west\""]);
        assert!(out.contains("# the lamp by the window\n[[thing]]"));
        // A name and a light given, a light's reach.
        let mut now = was.clone();
        now.things[4].name = Some("guard".into());
        now.things[1].lights[0].radius = 75.0;
        now.things[0].lights = vec![ThingLight { at: Some([0, 0, 20]), radius: 40.0, color: None }];
        let out = text::rewrite(&src, &was, &now, &legend);
        assert_eq!(GridLevel::from_text(&out).unwrap(), now);
        assert!(out.contains("facing = \"south\"\nlight = { at = [0, 0, 20], radius = 40 }\n") && out.contains("[[thing]]\nname = \"guard\"\nkind = \"walker\""), "{out}");
        assert!(out.contains("radius = 75, color"), "{out}");
        // Taken away: its lines and the comment above it; new ones after
        // the last of their sort, a new area at the end.
        let mut now = was.clone();
        now.things.remove(1);
        now.things.push(Thing { name: Some("rug".into()), model: Some("room/rug.vvx".into()), ..Thing::new(PIECE, [5, 5, 0]) });
        now.places.push(Place { name: "back".into(), at: [1, 2, 0], facing: Some([0, 1]), named: true, line: 0 });
        now.areas.push(Area { name: "hall".into(), at: [0, 0, 0], size: [9, 9, 9], line: 0 });
        let out = text::rewrite(&src, &was, &now, &legend);
        assert_eq!(GridLevel::from_text(&out).unwrap(), now, "{out}");
        assert!(!out.contains("lamp by the window") && !out.contains("name = \"lamp\""), "{out}");
        assert!(out.contains("kind = \"walker\"\nat = [100, 20, 0]\n\n[[thing]]\nname = \"rug\""), "{out}");
        assert!(out.contains("facing = [1, 1]\n\n[[place]]\nname = \"back\"\nat = [1, 2, 0]\nfacing = \"north\"\n\n[[area]]"), "{out}");
        assert!(out.ends_with("size = [40, 30, 40]\n\n[[area]]\nname = \"hall\"\nat = [0, 0, 0]\nsize = [9, 9, 9]\n"), "{out}");
        // The last one taken away leaves no blank line behind.
        let mut now = was.clone();
        now.areas.clear();
        assert!(text::rewrite(&src, &was, &now, &legend).ends_with("facing = [1, 1]\n"));
        // The room's size: its value.
        let mut now = was.clone();
        now.size = IVec3::new(200, 200, 120);
        check(&now, &["size = [200, 200, 120]    # voxels: across (x), deep (y, 0 in front), up (z); no grid of blocks"]);
        // Things in another order than the file's: written whole.
        let mut now = was.clone();
        now.things.swap(0, 1);
        assert_eq!(text::rewrite(&src, &was, &now, &legend), now.to_text());
    }

    /// A piece tipped (`pitch`, `roll`): read, written after the way it
    /// faces only when not 0, touching only those lines (a solid line
    /// after them); off the quarter turns a game on the grid names it,
    /// and ALIGN ALL takes the nearest quarter (0: the line goes); a light
    /// alone or a thing of a kind has nothing to tip.
    #[test]
    fn a_piece_is_tipped_by_pitch_and_roll() {
        let src = commented();
        let was = GridLevel::from_text(&src).unwrap();
        let legend = Legend::default();
        let mut now = was.clone();
        (now.things[0].pitch, now.things[0].roll) = (90.0, 270.0);
        let out = text::rewrite(&src, &was, &now, &legend);
        assert_eq!(GridLevel::from_text(&out).unwrap(), now, "{out}");
        assert_eq!(changed_lines(&src, &out), ["pitch = 90", "roll = 270"], "{out}");
        assert!(out.contains("facing = \"south\"\npitch = 90\nroll = 270\n"), "{out}");
        assert!(!now.things[0].off_grid() && now.things[0].quarters() == Some([180, 90, 270]));
        // Solid after them; back to flat: the file as it was.
        let mut solid = now.clone();
        solid.things[0].solid = true;
        let out2 = text::rewrite(&out, &now, &solid, &legend);
        assert!(out2.contains("roll = 270\nsolid = true\n"), "{out2}");
        assert_eq!(text::rewrite(&out, &now, &was, &legend), src);
        // Written whole too.
        assert!(now.to_text().contains("facing = \"south\"\npitch = 90\nroll = 270\n"));
        // Off the quarters: named; aligned to the nearest, a 0 taken out.
        let mut tilted = was.clone();
        (tilted.things[0].pitch, tilted.things[0].roll) = (30.0, 100.0);
        let text = text::rewrite(&src, &was, &tilted, &legend);
        let back = GridLevel::from_text(&text).unwrap();
        assert_eq!(back.off_grid().iter().map(|t| t.called()).collect::<Vec<_>>(), ["'stove' (line 6)", "'chair' (line 26)"]);
        let aligned = text::align(&text).unwrap();
        let lv = GridLevel::from_text(&aligned).unwrap();
        assert!(lv.off_grid().is_empty(), "{aligned}");
        assert_eq!((lv.things[0].pitch, lv.things[0].roll), (0.0, 90.0));
        assert!(!aligned.contains("pitch") && aligned.contains("roll = 90\n"), "{aligned}");
        let wrong = |from: &str, to: &str| format!("{:#}", GridLevel::from_text(&PIECES.replacen(from, to, 1)).unwrap_err());
        assert!(wrong("name = \"moon\"", "name = \"moon\"\npitch = 90").contains("pitch tips a model"));
        assert!(wrong("kind = \"walker\"", "kind = \"walker\"\nroll = 90").contains("roll tips a model"));
        assert!(wrong("facing = \"south\"", "facing = \"south\"\npitch = \"up\"").contains("pitch must be degrees"));
    }

    /// A piece brings one light (`light = {...}`) or several (`lights =
    /// [...]`, a flame and its glow), and may be solid; the writer keeps
    /// `light =` for one and writes `lights = [` for more, touching only
    /// those lines; the mistakes say their line.
    #[test]
    fn a_piece_brings_several_lights_and_may_be_solid() {
        let src = commented();
        let was = GridLevel::from_text(&src).unwrap();
        let legend = Legend::default();
        // A second light on the lamp: its one line becomes the list.
        let mut now = was.clone();
        let glow = ThingLight { at: Some([3, 3, 41]), radius: 150.0, color: Some("#ffb060".into()) };
        now.things[1].lights.push(glow.clone());
        let out = text::rewrite(&src, &was, &now, &legend);
        assert_eq!(GridLevel::from_text(&out).unwrap(), now, "{out}");
        assert_eq!(changed_lines(&src, &out), ["lights = [", "  { at = [3, 3, 41], radius = 60, color = \"#ffd9a0\" },", "  { at = [3, 3, 41], radius = 150, color = \"#ffb060\" },", "]"], "{out}");
        // Back to one: `light =` again, the file as it was.
        let back = GridLevel::from_text(&out).unwrap();
        let mut one = back.clone();
        one.things[1].lights.pop();
        assert_eq!(text::rewrite(&out, &back, &one, &legend), src);
        // A list changed: only its lines.
        let mut wider = back.clone();
        wider.things[1].lights[1].radius = 200.0;
        let again = text::rewrite(&out, &back, &wider, &legend);
        assert_eq!(changed_lines(&out, &again), ["  { at = [3, 3, 41], radius = 200, color = \"#ffb060\" },"]);
        // Solid: its line after the way it faces, and gone again.
        let mut solid = was.clone();
        solid.things[0].solid = true;
        let out = text::rewrite(&src, &was, &solid, &legend);
        assert!(out.contains("facing = \"south\"\nsolid = true\n"), "{out}");
        assert_eq!(GridLevel::from_text(&out).unwrap(), solid);
        assert_eq!(text::rewrite(&out, &solid, &was, &legend), src);
        // Written whole: the same.
        let mut both = solid.clone();
        both.things[1].lights.push(glow);
        assert_eq!(GridLevel::from_text(&both.to_text()).unwrap(), both);
        // Mistakes.
        let wrong = |from: &str, to: &str| format!("{:#}", GridLevel::from_text(&PIECES.replacen(from, to, 1)).unwrap_err());
        for (from, to, says) in [
            ("light = { at = [3, 3, 41]", "lights = [{ radius = 9 }]\nlight = { at = [3, 3, 41]", "light = { ... } (one) or lights = [ ... ] (several), not both"),
            ("light = { at = [3, 3, 41], radius = 60, color = \"#ffd9a0\" }", "lights = []", "line 14: lights = [] holds no light"),
            ("light = { at = [3, 3, 41], radius = 60, color = \"#ffd9a0\" }", "lights = [{ at = [1, 2, 3] }]", "line 14: a light needs radius = N"),
            ("name = \"moon\"", "name = \"moon\"\nsolid = true", "line 17: solid = true needs a model"),
            ("kind = \"walker\"", "kind = \"walker\"\nsolid = true", "solid = true is a piece's"),
            ("facing = \"south\"", "facing = \"south\"\nsolid = 1", "line 9: solid must be true or false"),
        ] {
            let e = wrong(from, to);
            assert!(e.contains(says), "{from} -> {to}: {e}");
        }
    }

    /// A level's own daylight (`[light]`, the words of `c.light`) reads,
    /// is written before what is set down, put in, changed and taken out
    /// touching only its lines; its mistakes say their line.
    #[test]
    fn a_level_has_its_own_daylight() {
        let src = commented();
        let was = GridLevel::from_text(&src).unwrap();
        let legend = Legend::default();
        let mut now = was.clone();
        now.daylight = Some(Daylight { sun: Some([-0.4, 0.6, -1.0]), sun_color: Some("#ffd0a0".into()), ambient: Some(0.3), shadow: None, sky: None });
        let out = text::rewrite(&src, &was, &now, &legend);
        assert_eq!(GridLevel::from_text(&out).unwrap(), now, "{out}");
        assert!(out.contains("no grid of blocks\n\n[light]               # this level's daylight; what it leaves out is the game's c.light\nsun = [-0.4, 0.6, -1]\nsun_color = \"#ffd0a0\"\nambient = 0.3\n\n# the lamp") || out.contains("[light]"), "{out}");
        assert!(out.find("[light]") < out.find("[[thing]]"), "before the things: {out}");
        let day = now.daylight.as_ref().unwrap().picture();
        assert_eq!((day.sun, day.ambient, day.shadow), (Some(glam::Vec3::new(-0.4, 0.6, -1.0)), Some(0.3), None));
        // Changed: only its lines; taken out: the file as it was.
        let back = GridLevel::from_text(&out).unwrap();
        let mut darker = back.clone();
        darker.daylight.as_mut().unwrap().ambient = Some(0.1);
        let again = text::rewrite(&out, &back, &darker, &legend);
        assert_eq!(changed_lines(&out, &again), ["ambient = 0.1"]);
        let mut none = back.clone();
        none.daylight = None;
        assert_eq!(text::rewrite(&out, &back, &none, &legend), src);
        // Written whole, in a level of blocks too.
        let mut blocks = level(4, IVec3::new(2, 1, 1), &[("rock", Kind::Solid)]);
        blocks.daylight = now.daylight.clone();
        blocks.things = was.things.clone();
        assert_eq!(GridLevel::from_text(&blocks.to_text()).unwrap(), blocks);
        // Mistakes.
        let with = |light: &str| format!("{:#}", GridLevel::from_text(&PIECES.replacen("\n\n[[thing]]", &format!("\n\n[light]\n{light}\n\n[[thing]]"), 1)).unwrap_err());
        assert!(with("sun = [0, 0, 1]").contains("line 5: sun is the way its light travels"), "{}", with("sun = [0, 0, 1]"));
        assert!(with("ambient = 2").contains("ambient must be between 0 and 1"));
        assert!(with("moon = 1").contains("'moon' is no field of the level's [light]"));
        assert!(with("sky = \"blue\"").contains("sky must be a color"));
        assert!(with("sky = [1, 2, 0]").contains("sky must be a color \"#rrggbb\" or three numbers from 0 to 1"));
        // A color as conf.lua writes it, three numbers from 0 to 1.
        let numbers = GridLevel::from_text(&PIECES.replacen("\n\n[[thing]]", "\n\n[light]\nsky = [0.125, 0.1875, 0.29]\nsun_color = [1, 0.85, 0.63]\n\n[[thing]]", 1)).unwrap();
        let day = numbers.daylight.as_ref().unwrap();
        assert_eq!((day.sky.as_deref(), day.sun_color.as_deref()), (Some("#20304a"), Some("#ffd9a1")));
    }

    /// A level of blocks whose grid changed is written anew above its
    /// pieces, which stay as they were, comments too.
    #[test]
    fn a_rewritten_grid_keeps_the_pieces_text() {
        let mut lv = level(4, IVec3::new(3, 1, 1), &[("rock", Kind::Solid)]);
        put(&mut lv, IVec3::ZERO, 1, Turn::NONE);
        let src = format!("{}\n{}", lv.to_text(), commented().split_once("\n\n").unwrap().1);
        let was = GridLevel::from_text(&src).unwrap();
        let mut now = was.clone();
        put(&mut now, IVec3::new(2, 0, 0), 1, Turn::NONE);
        let out = text::rewrite(&src, &was, &now, &Legend::of_text(&src));
        assert_eq!(GridLevel::from_text(&out).unwrap(), now);
        assert_eq!(changed_lines(&src, &out), ["r.r"], "{out}");
        assert!(out.contains("at = [40, 150, 0]    # by the wall"));
    }

    /// Every mistake of a level's set-down part says its line and what is
    /// wrong.
    #[test]
    fn a_levels_mistakes_say_their_line() {
        let wrong = |from: &str, to: &str| format!("{:#}", GridLevel::from_text(&PIECES.replacen(from, to, 1)).unwrap_err());
        for (from, to, says) in [
            ("name = \"moon\"", "name = \"lamp\"", "line 17: a second thing called 'lamp' (the first is on line 11)"),
            ("name = \"stove\"\nmodel", "kind = \"stove\"\nmodel", "kind = \"stove\" and a model"),
            ("name = \"moon\"", "kind = \"piece\"\nname = \"moon\"\n[[thing]]\nkind = \"piece\"", "a piece is a model set down"),
            ("facing = \"south\"", "facing = \"down\"", "line 8: facing must be \"north\", \"east\", \"south\" or \"west\""),
            ("facing = \"south\"", "facing = \"south\"\nrotation = 10", "line 9: a thing faces a way (facing = \"south\") or has a rotation (rotation = 30), not both"),
            ("facing = \"south\"", "turn = 10", "line 8: a thing: 'turn' is called 'rotation' now (voxcade upgrade renames it)"),
            ("radius = 60, ", "", "line 14: a light needs radius = N"),
            ("color = \"#ffd9a0\"", "color = \"orange\"", "line 14: a light's color must be \"#rrggbb\""),
            ("kind = \"walker\"\n", "", "a thing needs kind = \"name\""),
            ("room/stove.vvx", "room/stove.vox", "a model is a .vvx file"),
            ("size = [40, 30, 40]", "size = [40, 30, 40]\n[[area]]\nname = \"by_the_stove\"\nat = [0, 0, 0]\nsize = [1, 1, 1]", "a second area called 'by_the_stove' (the first is on line 42)"),
            ("size = [40, 30, 40]", "size = [40, 0, 40]", "an area's size must be at least one voxel"),
            ("size = [360, 200, 120]", "size = [360, 200, 120]\n[cells]", "cells is for a level of blocks: block = N first"),
            ("[1, 1]", "\"up\"", "facing must be \"north\""),
        ] {
            let e = wrong(from, to);
            assert!(e.contains(says), "{from} -> {to}: {e}");
        }
    }

    /// The things a game on the grid cannot show (turned off the quarter
    /// turns) are listed; aligning faces each its nearest way (a turn
    /// right between two goes clockwise) and changes nothing else of the
    /// file, to the character.
    #[test]
    fn aligning_faces_each_its_nearest_way() {
        let lv = GridLevel::from_text(PIECES).unwrap();
        let off: Vec<String> = lv.off_grid().iter().map(|t| t.called()).collect();
        assert_eq!(off, ["'chair' (line 22)"]);
        assert!(off_grid_mistake(&lv.off_grid()).contains("1 thing of this level is turned off them: 'chair' (line 22)"));
        for (turn, way) in [(45.0, Way::East), (44.9, Way::North), (135.0, Way::South), (225.0, Way::West), (315.0, Way::North), (-45.0, Way::North), (-46.0, Way::West), (400.0, Way::North), (420.0, Way::East), (180.0, Way::South)] {
            assert_eq!(Way::nearest(turn), way, "{turn}");
        }
        assert!(!Facing::Turn(270.0).off_grid() && Facing::Turn(-90.0).bearing() == -90.0, "quarter turns are on the grid");
        let src = format!("# a room\n{}", PIECES.replace("rotation = 45", "rotation = 45    # a chair askew"));
        let aligned = text::align(&src).unwrap();
        assert_eq!(aligned, src.replace("rotation = 45", "facing = \"east\""));
        assert!(GridLevel::from_text(&aligned).unwrap().off_grid().is_empty());
        assert_eq!(text::align(&aligned).unwrap(), aligned, "nothing left to align");
    }

    #[test]
    fn blocks_are_stamped_in_their_turn() {
        let n = 4;
        // A post in the block's front left corner, a voxel of another
        // colour on its top.
        let mut post = block(n, [10, 20, 30], |p| p.x == 0 && p.y == 0);
        post.set(IVec3::new(1, 0, 3), Some([200, 0, 0])).unwrap();
        let mut lv = level(n, IVec3::new(2, 1, 1), &[("post", Kind::Solid)]);
        put(&mut lv, IVec3::new(0, 0, 0), 1, Turn::NONE);
        put(&mut lv, IVec3::new(1, 0, 0), 1, Turn::MIRROR);
        let world = build(&lv, &[post], 3, 2, &[], &no_models).unwrap();
        let red = |p: IVec3| world.palette[world.get(p) as usize];
        let o = IVec3::new(3, 0, 2);
        assert_ne!(world.get(o), 0);
        assert_eq!(red(o + IVec3::new(1, 0, 3)), [200, 0, 0, 255]);
        // Mirrored: the post at the right of the second cell.
        assert_ne!(world.get(o + IVec3::new(n + 3, 0, 1)), 0);
        assert_eq!(world.get(o + IVec3::new(n, 0, 1)), 0);
        assert_eq!(red(o + IVec3::new(n + 2, 0, 3)), [200, 0, 0, 255]);
        assert_eq!(world.stage, Some((o, o + IVec3::new(2 * n, n, n))));
        assert_eq!(world.size, IVec3::new(2 * n + 6, n, n + 2));
    }

    #[test]
    fn an_upside_down_plank_holds_at_its_new_top() {
        let n = 4;
        // A plank two voxels thick on the block's floor.
        let plank = block(n, [90, 60, 30], |p| p.z < 2);
        let mut lv = level(n, IVec3::new(2, 1, 1), &[("plank", Kind::Oneway)]);
        put(&mut lv, IVec3::new(0, 0, 0), 1, Turn::NONE);
        put(&mut lv, IVec3::new(1, 0, 0), 1, Turn::ROLL.then(Turn::ROLL));
        let world = build(&lv, &[plank], 0, 0, &[], &no_models).unwrap();
        let held = |p: IVec3| world.oneway[world.get(p) as usize] && !world.soft[world.get(p) as usize];
        let passed = |p: IVec3| world.get(p) != 0 && world.soft[world.get(p) as usize];
        assert!(held(IVec3::new(1, 1, 1)) && passed(IVec3::new(1, 1, 0)), "lying as made: holds at its top");
        assert!(held(IVec3::new(n + 1, 1, 3)) && passed(IVec3::new(n + 1, 1, 2)), "upside down: holds at its new top");
        assert_eq!(world.get(IVec3::new(n + 1, 1, 0)), 0);
    }

    #[test]
    fn ladders_and_hazards_are_found_by_cell() {
        let n = 4;
        let full = block(n, [1, 2, 3], |_| true);
        let mut lv = level(n, IVec3::new(3, 2, 1), &[("rock", Kind::Solid), ("ladder", Kind::Ladder), ("spikes", Kind::Hazard)]);
        put(&mut lv, IVec3::new(0, 0, 0), 1, Turn::NONE);
        put(&mut lv, IVec3::new(1, 0, 0), 2, Turn::NONE);
        put(&mut lv, IVec3::new(2, 1, 0), 3, Turn::NONE);
        let world = build(&lv, &[full.clone(), full.clone(), full], 8, 0, &[], &no_models).unwrap();
        let cells = world.cells.as_ref().unwrap();
        let at = |x: f32, y: f32, z: f32| cells.at(Vec3::new(x, y, z));
        assert_eq!(at(9.0, 1.0, 1.0), SOLID);
        assert_eq!(at(14.5, 3.9, 3.9), LADDER);
        assert_eq!(at(17.0, 5.0, 2.0), HAZARD);
        assert_eq!(at(17.0, 1.0, 2.0), 0, "an empty cell");
        assert_eq!(at(7.9, 1.0, 1.0), 0, "outside the grid");
        assert!(world.soft[world.get(IVec3::new(13, 1, 1)) as usize], "a ladder is passed through");
        assert!(!world.soft[world.get(IVec3::new(17, 5, 1)) as usize], "spikes are solid");
    }

    #[test]
    fn too_many_colours_name_the_block() {
        let n = 8;
        let many = |base: u8| {
            let mut b = Block::empty(n);
            for i in 0..130 {
                b.set(IVec3::new(i % n, i / n % n, i / (n * n)), Some([base, i as u8, 0])).unwrap();
            }
            b
        };
        let mut lv = level(n, IVec3::new(2, 1, 1), &[("first", Kind::Solid), ("second", Kind::Solid)]);
        put(&mut lv, IVec3::new(0, 0, 0), 1, Turn::NONE);
        put(&mut lv, IVec3::new(1, 0, 0), 2, Turn::NONE);
        let e = build(&lv, &[many(1), many(2)], 0, 0, &[], &no_models).err().unwrap().to_string();
        assert!(e.contains("more than 255 colors with block 'second'"), "{e}");
    }

    #[test]
    fn a_level_loads_its_blocks_and_things_from_files() {
        let dir = crate::scratch::Scratch::new("grid-files");
        let _ = std::fs::remove_dir_all(&dir);
        let mut lv = level(4, IVec3::new(1, 1, 1), &[("rock", Kind::Solid)]);
        put(&mut lv, IVec3::ZERO, 1, Turn::NONE);
        lv.things.push(Thing::new("gem", [2, 2, 4]));
        lv.save(&dir.join("level.toml")).unwrap();
        let cfg = BlocksConfig { margin: 10, below: 5, ..BlocksConfig::default() };
        let e = format!("{:#}", load(&dir, &cfg, &["gem"]).err().unwrap());
        assert!(e.contains("rock.vvx"), "a missing block names its file: {e}");
        block(4, [5, 5, 5], |_| true).save(&blocks::path(&dir, "rock")).unwrap();
        let (world, markers, files) = load(&dir, &cfg, &["miner", "gem"]).unwrap();
        assert_eq!(world.get(IVec3::new(10, 0, 5)), world.get(IVec3::new(13, 3, 8)));
        assert_ne!(world.get(IVec3::new(10, 0, 5)), 0);
        assert_eq!(markers, [Marker { kind: 1, cell: IVec3::new(12, 2, 9) }]);
        assert_eq!(files, [dir.join("level.toml"), blocks::path(&dir, "rock")]);
        let e = format!("{:#}", load(&dir, &cfg, &["miner"]).err().unwrap());
        assert!(e.contains("'gem' is no kind of thing") && e.contains("thing at [2, 2, 4]"), "{e}");
        assert!(e.matches("level.toml").count() <= 1, "the caller names the file, not this: {e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

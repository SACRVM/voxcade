//! Levels built of voxel blocks in a 3D grid (`docs/tiles.md`). A level
//! file (`levels/NAME.toml`, text: `text`) names its blocks
//! (`blocks/NAME.vvx`) and what each is to the game, holds a block and its
//! turn for every cell, and the things placed freely, in voxels. Loading stamps the blocks into the world's
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

/// A thing placed freely: its kind's name and where it stands (the middle
/// of its feet), in voxels from the grid's low corner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thing {
    pub kind: String,
    pub at: [i32; 3],
}

/// A named place a player arrives at (`world:go_to(level, place)`): where
/// its feet stand, in voxels from the grid's low corner, and the way it
/// faces (south when not given).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub name: String,
    pub at: [i32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<[i32; 2]>,
}

/// A level of blocks in a grid, as its file holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct GridLevel {
    /// A block's size: voxels each way.
    pub n: i32,
    /// Cells across, in depth and up.
    pub size: IVec3,
    pub blocks: Vec<BlockEntry>,
    /// Per cell, x + y w + z w d.
    pub cells: Vec<Cell>,
    pub things: Vec<Thing>,
    pub places: Vec<Place>,
}

impl GridLevel {
    pub fn new(n: i32, size: IVec3) -> Self {
        Self { n, size, blocks: Vec::new(), cells: vec![Cell::default(); (size.x * size.y * size.z).max(0) as usize], things: Vec::new(), places: Vec::new() }
    }

    /// Cell `c`'s place in `cells` (None outside the grid).
    pub fn index(&self, c: IVec3) -> Option<usize> {
        let s = self.size;
        (c.cmpge(IVec3::ZERO).all() && c.cmplt(s).all()).then(|| (c.x + c.y * s.x + c.z * s.x * s.y) as usize)
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
/// and its signs, each a block's name and turn. Kept when the editor
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
/// [legend]              # a sign is a block as it lies: its name, or with a turn (0 to 47)
/// w = "wall"
/// s = "stone"
/// S = { block = "stone", turn = 17 }
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
/// [[place]]
/// name = "door"
/// at = [25, 8, 70]
/// facing = [0, -1]
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
/// empty). Things and places come in their order.
pub mod text {
    use super::*;
    use crate::vvx::{self, ONE_SIGN, Reader, SIGNS};
    use toml_edit::{Document, Item};

    /// The level's text, keeping the view and the signs of `kept` (the
    /// legend the file had; `Legend::default()` for a new one).
    pub fn write(level: &GridLevel, kept: &Legend) -> String {
        let view = kept.view.unwrap_or_else(|| View::for_size(level.size));
        // The pairs of block and turn the cells hold, by first use in
        // reading order.
        let order = reading_order(level.size, view);
        let mut pairs: Vec<Cell> = Vec::new();
        for c in order.iter().flatten().flatten() {
            let cell = level.cells[*c];
            if cell.block != 0 && !pairs.contains(&cell) {
                pairs.push(cell);
            }
        }
        let name = |c: &Cell| level.blocks[c.block as usize - 1].name.clone();
        let width = if pairs.len() <= ONE_SIGN { 1 } else { 2 };
        let kept: Vec<&(String, String, u8)> = kept.signs.iter().filter(|(s, ..)| s.len() == width).collect();
        let mut taken: Vec<String> = kept.iter().map(|(s, ..)| s.clone()).collect();
        let mut sign_of: Vec<(Cell, String)> = Vec::new();
        // Kept signs first, in the file's order.
        for (s, block, turn) in &kept {
            if let Some(c) = pairs.iter().find(|c| name(c) == *block && c.turn.number() == *turn)
                && !sign_of.iter().any(|(d, _)| d == c)
            {
                sign_of.push((*c, s.clone()));
            }
        }
        let kept_count = sign_of.len();
        for c in &pairs {
            if sign_of.iter().any(|(d, _)| d == c) {
                continue;
            }
            let s = new_sign(&name(c), width, &taken);
            taken.push(s.clone());
            sign_of.push((*c, s));
        }
        // The legend's order: the kept ones as the file had them, then the
        // new ones as they came.
        let mut legend = sign_of[..kept_count].to_vec();
        legend.extend(sign_of[kept_count..].iter().cloned());
        let s = level.size;
        let mut out = format!("format = \"{FORMAT}\"\nblock = {}            # a block is this many voxels each way\nsize = [{}, {}, {}]    # cells: across (x), deep (y, 0 in front), up (z)\n", level.n, s.x, s.y, s.z);
        out += &match view {
            View::Front => "view = \"front\"        # each layer the grid seen from the front, the front layer first, its top row first\n".to_string(),
            View::Above => "view = \"above\"        # each layer the grid seen from above, the bottom layer first, its far row first\n".to_string(),
        };
        out += "\n[blocks]              # blocks/NAME.vvx, each what it is to the game\n";
        for b in &level.blocks {
            out += &format!("{} = \"{}\"\n", key(&b.name), b.kind.name());
        }
        out += "\n[legend]              # a sign is a block as it lies: its name, or the name and a turn (0 to 47)\n";
        for (c, sign) in &legend {
            match c.turn.number() {
                0 => out += &format!("{sign} = {}\n", vvx::quoted(&name(c))),
                t => out += &format!("{sign} = {{ block = {}, turn = {t} }}\n", vvx::quoted(&name(c))),
            }
        }
        let dots = ".".repeat(width);
        let layers: Vec<Vec<String>> = order
            .iter()
            .map(|layer| {
                layer
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|&i| {
                                let cell = level.cells[i];
                                if cell.block == 0 { dots.as_str() } else { sign_of.iter().find(|(d, _)| *d == cell).map_or("?", |(_, s)| s.as_str()) }
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        out += "\n[cells]\n";
        out += &vvx::layers_text(&layers);
        for t in &level.things {
            out += &format!("\n[[thing]]\nkind = {}\nat = [{}, {}, {}]\n", vvx::quoted(&t.kind), t.at[0], t.at[1], t.at[2]);
        }
        for p in &level.places {
            out += &format!("\n[[place]]\nname = {}\nat = [{}, {}, {}]\n", vvx::quoted(&p.name), p.at[0], p.at[1], p.at[2]);
            if let Some([x, y]) = p.facing {
                out += &format!("facing = [{x}, {y}]\n");
            }
        }
        out
    }

    /// A block's name as a key of TOML: bare when it can be.
    fn key(name: &str) -> String {
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { name.to_string() } else { vvx::quoted(name) }
    }

    /// A sign for a block not taken yet: the letters of its name first
    /// (`s` for stone, `S` turned), then any.
    fn new_sign(name: &str, width: usize, taken: &[String]) -> String {
        let free = |s: &String| !taken.contains(s);
        if width == 1 {
            let letters = name.chars().filter(|c| c.is_ascii_alphanumeric());
            let mut wished: Vec<String> = Vec::new();
            for c in letters {
                wished.push(c.to_ascii_lowercase().to_string());
                wished.push(c.to_ascii_uppercase().to_string());
            }
            if let Some(s) = wished.into_iter().find(free) {
                return s;
            }
            return SIGNS.iter().map(|&b| (b as char).to_string()).find(free).expect("62 signs for 62 pairs");
        }
        (0..ONE_SIGN * ONE_SIGN).map(|i| format!("{}{}", SIGNS[i / ONE_SIGN] as char, SIGNS[i % ONE_SIGN] as char)).find(free).expect("signs enough")
    }

    /// The cells' places in `cells` as the layers read: layers of rows of
    /// cells.
    fn reading_order(size: IVec3, view: View) -> Vec<Vec<Vec<usize>>> {
        let i = |x: i32, y: i32, z: i32| (x + y * size.x + z * size.x * size.y) as usize;
        match view {
            View::Above => (0..size.z).map(|z| (0..size.y).rev().map(|y| (0..size.x).map(|x| i(x, y, z)).collect()).collect()).collect(),
            View::Front => (0..size.y).map(|y| (0..size.z).rev().map(|z| (0..size.x).map(|x| i(x, y, z)).collect()).collect()).collect(),
        }
    }

    /// The level a text says and its legend, every mistake said with its
    /// line.
    pub fn read(src: &str) -> Result<(GridLevel, Legend)> {
        let doc = Document::parse(src).map_err(|e| match e.span() {
            Some(s) => anyhow!("line {}: {}", vvx::line_of(src, s.start), e.message()),
            None => anyhow!("{}", e.message()),
        })?;
        let r = Reader { src };
        let top = doc.as_table();
        vvx::check_format_of(&r, top, FORMAT_NAME, VERSION, "level")?;
        r.only(top, &["format", "block", "size", "view", "blocks", "legend", "cells", "thing", "place"], "a level")?;
        let whole = |item: &Item, what: &str| -> Result<i32> {
            let v = r.number(item, what)?;
            if v.fract() != 0.0 || v < 1.0 || v > 100_000.0 {
                return Err(r.fail(item.span(), format!("{what} must be a whole number, 1 or more")));
            }
            Ok(v as i32)
        };
        let n = whole(top.get("block").ok_or_else(|| anyhow!("no block = N (a block's voxels each way)"))?, "block")?;
        let size_item = top.get("size").ok_or_else(|| anyhow!("no size = [x, y, z] (the grid's cells)"))?;
        let size = r.ints(size_item, "size")?;
        if size.cmplt(IVec3::ONE).any() {
            return Err(r.fail(size_item.span(), "size must be at least one cell each way"));
        }
        if (size.x as i64) * (size.y as i64) * (size.z as i64) > 1 << 26 {
            return Err(r.fail(size_item.span(), "the grid is too big"));
        }
        let view = match top.get("view") {
            None => None,
            Some(item) => Some(match item.as_str() {
                Some("above") => View::Above,
                Some("front") => View::Front,
                _ => return Err(r.fail(item.span(), "view must be \"above\" or \"front\" (which way the layers read)")),
            }),
        };
        let mut level = GridLevel::new(n, size);
        if let Some(item) = top.get("blocks") {
            let table = item.as_table_like().ok_or_else(|| r.fail(item.span(), "blocks must be a table: name = \"solid\" and the like"))?;
            for (name, kind) in table.iter() {
                let word = kind.as_str().unwrap_or_default();
                let kind = Kind::ALL.into_iter().find(|k| k.name() == word);
                let names: Vec<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
                let kind = kind.ok_or_else(|| r.fail(table.get(name).and_then(Item::span), format!("block '{name}': '{word}' is no kind of block ({})", names.join(", "))))?;
                level.blocks.push(BlockEntry { name: name.to_string(), kind });
            }
        }
        // The legend: sign -> (block's number, turn).
        let mut legend = Legend { view, signs: Vec::new() };
        let mut cells_of: Vec<(String, Cell)> = Vec::new();
        let mut width = 0;
        if let Some(item) = top.get("legend") {
            let table = item.as_table_like().ok_or_else(|| r.fail(item.span(), "legend must be a table of signs"))?;
            for (sign, entry) in table.iter() {
                let len = sign.chars().count();
                if !(1..=2).contains(&len) || sign.chars().any(|c| !c.is_ascii_alphanumeric()) {
                    return Err(r.fail(entry.span(), format!("'{sign}' cannot be a sign: one or two letters or digits")));
                }
                if width != 0 && width != len {
                    return Err(r.fail(entry.span(), format!("'{sign}' has {len} characters, the signs before have {width}: all signs of a file are as long as one another")));
                }
                width = len;
                let (block, turn) = match (entry.as_str(), entry.as_table_like()) {
                    (Some(name), _) => (name.to_string(), 0),
                    (None, Some(t)) => {
                        r.only(t, &["block", "turn"], &format!("the legend's '{sign}'"))?;
                        let name = t.get("block").and_then(Item::as_str).ok_or_else(|| r.fail(entry.span(), format!("the legend's '{sign}' needs block = \"name\"")))?;
                        let turn = match t.get("turn") {
                            None => 0,
                            Some(i) => {
                                let v = r.number(i, "turn")?;
                                if v.fract() != 0.0 || !(0.0..Turn::COUNT as f32).contains(&v) {
                                    return Err(r.fail(i.span(), format!("turn must be a whole number from 0 to {}", Turn::COUNT - 1)));
                                }
                                v as u8
                            }
                        };
                        (name.to_string(), turn)
                    }
                    _ => return Err(r.fail(entry.span(), format!("the legend's '{sign}' must be a block's name, or {{ block = \"name\", turn = 1 }}"))),
                };
                let number = level.blocks.iter().position(|b| b.name == block).ok_or_else(|| r.fail(entry.span(), format!("the legend's '{sign}': no block '{block}' in [blocks]")))?;
                cells_of.push((sign.to_string(), Cell { block: number as u16 + 1, turn: Turn::from_number(turn).expect("checked") }));
                legend.signs.push((sign.to_string(), block, turn));
            }
        }
        let width = width.max(1);
        let cells = top.get("cells").ok_or_else(|| anyhow!("no [cells] with the layers of signs"))?;
        let cells = cells.as_table_like().ok_or_else(|| r.fail(cells.span(), "cells must be a table with its layers: [cells]"))?;
        r.only(cells, &["layers"], "[cells]")?;
        let item = cells.get("layers").ok_or_else(|| anyhow!("[cells] has no layers = \"\"\" ... \"\"\""))?;
        let text = item.as_str().ok_or_else(|| r.fail(item.span(), "layers must be a text between \"\"\" marks"))?;
        let first = r.first_line(item);
        let line = |i: usize| if first > 0 { format!("line {}: ", first + i) } else { String::new() };
        let layers = vvx::layer_list(text, &line)?;
        let order = reading_order(size, view.unwrap_or_default());
        let (want_layers, want_rows) = (order.len(), order.first().map_or(0, Vec::len));
        let read_layers: usize = layers.iter().map(|l| l.times).sum();
        let what = match view.unwrap_or_default() {
            View::Above => ("layers up", "rows deep"),
            View::Front => ("layers deep", "rows up"),
        };
        if read_layers != want_layers {
            return Err(r.fail(item.span(), format!("the cells have {read_layers} layers, but a grid {} by {} by {} seen {} has {want_layers} ({})", size.x, size.y, size.z, view.unwrap_or_default().name(), what.0)));
        }
        let known = || cells_of.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(", ");
        let mut k = 0;
        for layer in &layers {
            if layer.rows.len() != want_rows {
                bail!("{}this layer has {} rows, a layer here has {want_rows} ({})", line(layer.rows[0].0), layer.rows.len(), what.1);
            }
            for _ in 0..layer.times {
                for (r_, &(i, row)) in layer.rows.iter().enumerate() {
                    let bytes = row.as_bytes();
                    if bytes.len() != size.x as usize * width {
                        bail!("{}this row has {} characters, a row here is {} cells of {width}", line(i), bytes.len(), size.x);
                    }
                    for (x, sign) in bytes.chunks(width).enumerate() {
                        if sign.iter().all(|&b| b == b'.') {
                            continue;
                        }
                        let cell = cells_of.iter().find(|(s, _)| s.as_bytes() == sign).map(|(_, c)| *c);
                        let cell = cell.ok_or_else(|| anyhow!("{}'{}' is no sign of the legend (it has {})", line(i), String::from_utf8_lossy(sign), known()))?;
                        level.cells[order[k][r_][x]] = cell;
                    }
                }
                k += 1;
            }
        }
        let triple = |t: &dyn toml_edit::TableLike, what: &str| -> Result<[i32; 3]> {
            let item = t.get("at").ok_or_else(|| anyhow!("{what} has no at = [x, y, z]"))?;
            let v = r.ints(item, "at")?;
            Ok(v.to_array())
        };
        if let Some(item) = top.get("thing") {
            for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "things must be tables: [[thing]] with kind and at"))? {
                r.only(t, &["kind", "at"], "a thing")?;
                let kind = t.get("kind").and_then(Item::as_str).ok_or_else(|| anyhow!("a thing has no kind = \"name\" (at {:?})", t.get("at").and_then(Item::span).map(|s| vvx::line_of(src, s.start))))?;
                level.things.push(Thing { kind: kind.to_string(), at: triple(t, "a thing")? });
            }
        }
        if let Some(item) = top.get("place") {
            for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "places must be tables: [[place]] with name and at"))? {
                r.only(t, &["name", "at", "facing"], "a place")?;
                let name = t.get("name").and_then(Item::as_str).ok_or_else(|| anyhow!("a place has no name = \"name\""))?;
                let facing = match t.get("facing") {
                    None => None,
                    Some(f) => {
                        let wrong = || r.fail(f.span(), "facing must be two whole numbers, like [0, -1]");
                        let a = f.as_array().filter(|a| a.len() == 2).ok_or_else(wrong)?;
                        let v: Vec<i64> = a.iter().map(|v| v.as_integer().ok_or_else(wrong)).collect::<Result<_>>()?;
                        Some([v[0] as i32, v[1] as i32])
                    }
                };
                level.places.push(Place { name: name.to_string(), at: triple(t, "a place")?, facing });
            }
        }
        Ok((level, legend))
    }
}

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
    let (world, markers, files, _) = load_at(&Files::folder(dir), cfg, &dir.join(&cfg.file), names)?;
    Ok((world, markers, files))
}

/// The level of blocks in the file at `path` (one of a game's `levels/`),
/// with its named places in the world's voxels.
pub fn load_at(game: &Files, cfg: &BlocksConfig, path: &Path, names: &[&str]) -> Result<crate::levels::Made> {
    let dir = game.dir();
    if cfg.margin < 0 || cfg.below < 0 {
        bail!("c.world.blocks: margin and below are voxels, 0 or more");
    }
    let path = path.to_path_buf();
    let level = GridLevel::read_in(game, &path)?;
    let mut files = vec![path.clone()];
    files.extend(level.blocks.iter().map(|b| blocks::path(dir, &b.name)));
    let made = level.load_blocks_in(game)?;
    let mut pictures = Vec::new();
    for b in &cfg.backdrops {
        let file = dir.join(&b.image);
        let (width, height, pixels) = crate::map::read_png_rgba_in(game, &file)?;
        files.push(file);
        pictures.push(Picture { how: b.clone(), width: width as i32, height: height as i32, pixels });
    }
    let world = build(&level, &made, cfg.margin, cfg.below, &pictures).with_context(|| path.display().to_string())?;
    let origin = IVec3::new(cfg.margin, 0, cfg.below);
    let markers = markers(&level, names, origin).with_context(|| path.display().to_string())?;
    let places = level.places.iter().map(|p| crate::levels::Place::new(&p.name, (origin + IVec3::from(p.at)).as_vec3() + glam::Vec3::new(0.5, 0.5, 0.0), p.facing)).collect();
    Ok((world, markers, files, places))
}

/// The things as markers, the grid's low corner at `origin`.
pub fn markers(level: &GridLevel, names: &[&str], origin: IVec3) -> Result<Vec<Marker>> {
    level
        .things
        .iter()
        .map(|t| {
            let kind = names.iter().position(|n| *n == t.kind).ok_or_else(|| anyhow!("things: '{}' is no kind of thing (c.things: {})", t.kind, names.join(", ")))?;
            Ok(Marker { kind, cell: origin + IVec3::from(t.at) })
        })
        .collect()
}

/// The world of a level: its blocks (`made`, as the level lists them)
/// stamped into the cells as they lie, `margin` voxels left and right,
/// `below` under the grid, the backdrops behind it.
pub fn build(level: &GridLevel, made: &[Block], margin: i32, below: i32, backdrops: &[Picture]) -> Result<World> {
    let n = level.n;
    let grid = level.size * n;
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
                    anyhow!("the level has more than 255 colours with block '{}' (a colour counts once for each way it holds: solid, held from above, passed through)", entry.name)
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
                    wall[(x + z * size.x) as usize] = world.colour_as([r, g, bl], true, false).ok_or_else(|| anyhow!("the level has more than 255 colours with backdrop '{}'", b.how.image))?;
                }
            }
        }
        walls.push((grid.y + b.how.behind.max(0), b.how.z, b.height * s, wall));
    }

    // Into the world chunk by chunk: the grid's rows and the walls' rows.
    let voxel = |p: IVec3| -> u8 {
        if p.y < grid.y {
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
    let mut rows_y: Vec<i32> = (0..(grid.y + CHUNK - 1) / CHUNK).collect();
    rows_y.extend(walls.iter().map(|(y, ..)| y / CHUNK));
    rows_y.sort_unstable();
    rows_y.dedup();
    let chunks = (size + CHUNK - 1) / CHUNK;
    for &cy in &rows_y {
        for cz in 0..chunks.z {
            for cx in 0..chunks.x {
                world.fill_chunk(IVec3::new(cx, cy, cz), voxel);
            }
        }
    }

    world.stage = Some((origin, origin + grid));
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
mod tests {
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
        lv.things.push(Thing { kind: "fox".into(), at: [6, 2, 4] });
        lv.places.push(Place { name: "door".into(), at: [1, 2, 3], facing: Some([0, -1]) });
        let text = lv.to_text();
        assert_eq!(GridLevel::from_text(&text).unwrap(), lv, "{text}");
        // Taller than deep: seen from the front, the top row first; the
        // block's name gives its sign, a turned one the capital.
        assert!(text.contains("view = \"front\"") && text.contains("\n79 = \"solid\"") && text.contains("plank = \"oneway\""), "{text}");
        assert!(text.contains("p = { block = \"plank\", turn = ") && text.contains("7 = \"79\""), "{text}");
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
        let world = build(&lv, &[post], 3, 2, &[]).unwrap();
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
        let world = build(&lv, &[plank], 0, 0, &[]).unwrap();
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
        let world = build(&lv, &[full.clone(), full.clone(), full], 8, 0, &[]).unwrap();
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
        let e = build(&lv, &[many(1), many(2)], 0, 0, &[]).err().unwrap().to_string();
        assert!(e.contains("more than 255 colours with block 'second'"), "{e}");
    }

    #[test]
    fn a_level_loads_its_blocks_and_things_from_files() {
        let dir = std::env::temp_dir().join(format!("voxcade-grid-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lv = level(4, IVec3::new(1, 1, 1), &[("rock", Kind::Solid)]);
        put(&mut lv, IVec3::ZERO, 1, Turn::NONE);
        lv.things.push(Thing { kind: "gem".into(), at: [2, 2, 4] });
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
        assert!(e.contains("'gem' is no kind of thing"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

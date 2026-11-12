//! Levels built of voxel blocks in a 3D grid (`docs/tiles.md`). A level
//! file (`level.json`) names its blocks (`blocks/NAME.vvx`) and what each is
//! to the game, holds a block and its turn for every cell, and the things
//! placed freely, in voxels. Loading stamps the blocks into the world's
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

/// The level file: the cells as runs (how many in a row, block, turn), x
/// fastest, then y, then z.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LevelFile {
    block: i32,
    size: [i32; 3],
    blocks: Vec<BlockEntry>,
    cells: Vec<[u32; 3]>,
    #[serde(default)]
    things: Vec<Thing>,
    #[serde(default)]
    places: Vec<Place>,
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

    pub fn from_text(text: &str) -> Result<Self> {
        let file: LevelFile = serde_json::from_str(text).context("not a level of blocks")?;
        let [x, y, z] = file.size;
        if file.block < 1 || x < 1 || y < 1 || z < 1 {
            bail!("a block and the grid are at least 1 each way (block {}, size {x} by {y} by {z})", file.block);
        }
        let mut level = Self::new(file.block, IVec3::new(x, y, z));
        level.blocks = file.blocks;
        level.things = file.things;
        level.places = file.places;
        let mut i = 0usize;
        for [count, block, turn] in file.cells {
            if block as usize > level.blocks.len() {
                bail!("cells: block {block}, but the level names {} blocks", level.blocks.len());
            }
            let turn = Turn::from_number(turn.min(255) as u8).ok_or_else(|| anyhow!("cells: turn {turn}, there are {} (0 to {})", Turn::COUNT, Turn::COUNT - 1))?;
            let end = i + count as usize;
            if end > level.cells.len() {
                bail!("cells: more than the {} the grid holds", level.cells.len());
            }
            level.cells[i..end].fill(Cell { block: block as u16, turn });
            i = end;
        }
        if i != level.cells.len() {
            bail!("cells: {i} given, the grid holds {}", level.cells.len());
        }
        Ok(level)
    }

    /// The level as its file holds it: the blocks and things a line each,
    /// the cells' runs ten to a line.
    pub fn to_text(&self) -> String {
        let mut runs: Vec<[u32; 3]> = Vec::new();
        for c in &self.cells {
            let (b, t) = (c.block as u32, c.turn.number() as u32);
            match runs.last_mut() {
                Some(r) if r[1] == b && r[2] == t => r[0] += 1,
                _ => runs.push([1, b, t]),
            }
        }
        let json = |v: &dyn erased::Json| v.json();
        let list = |items: Vec<String>, per_line: usize| -> String {
            if items.is_empty() {
                return "[]".into();
            }
            let lines: Vec<String> = items.chunks(per_line).map(|c| format!("    {}", c.join(", "))).collect();
            format!("[\n{}\n  ]", lines.join(",\n"))
        };
        let s = self.size;
        // Places only where there are some: a file without stays as it was.
        let places = if self.places.is_empty() { String::new() } else { format!(",\n  \"places\": {}", list(self.places.iter().map(|p| json(p)).collect(), 1)) };
        format!(
            "{{\n  \"block\": {},\n  \"size\": [{}, {}, {}],\n  \"blocks\": {},\n  \"cells\": {},\n  \"things\": {}{places}\n}}\n",
            self.n,
            s.x,
            s.y,
            s.z,
            list(self.blocks.iter().map(|b| json(b)).collect(), 1),
            list(runs.iter().map(|r| json(r)).collect(), 10),
            list(self.things.iter().map(|t| json(t)).collect(), 1),
        )
    }

    /// Writes the level at once (`blocks::save_at_once`).
    pub fn save(&self, path: &Path) -> Result<()> {
        blocks::save_at_once(path, self.to_text().as_bytes())
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

/// Serialising one item to compact JSON, whatever its type.
mod erased {
    pub trait Json {
        fn json(&self) -> String;
    }

    impl<T: serde::Serialize> Json for T {
        fn json(&self) -> String {
            serde_json::to_string(self).expect("plain data")
        }
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
        let text = lv.to_text();
        assert_eq!(GridLevel::from_text(&text).unwrap(), lv);
        assert!(text.contains("{\"name\":\"plank\",\"kind\":\"oneway\"}") && text.contains("[2,1,0], [27,0,0]"), "{text}");
        let e = format!("{:#}", GridLevel::from_text(&text.replace("[2,1,0]", "[2,3,0]")).unwrap_err());
        assert!(e.contains("block 3, but the level names 2 blocks"), "{e}");
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
        lv.save(&dir.join("level.json")).unwrap();
        let cfg = BlocksConfig { margin: 10, below: 5, ..BlocksConfig::default() };
        let e = format!("{:#}", load(&dir, &cfg, &["gem"]).err().unwrap());
        assert!(e.contains("rock.vvx"), "a missing block names its file: {e}");
        block(4, [5, 5, 5], |_| true).save(&blocks::path(&dir, "rock")).unwrap();
        let (world, markers, files) = load(&dir, &cfg, &["miner", "gem"]).unwrap();
        assert_eq!(world.get(IVec3::new(10, 0, 5)), world.get(IVec3::new(13, 3, 8)));
        assert_ne!(world.get(IVec3::new(10, 0, 5)), 0);
        assert_eq!(markers, [Marker { kind: 1, cell: IVec3::new(12, 2, 9) }]);
        assert_eq!(files, [dir.join("level.json"), blocks::path(&dir, "rock")]);
        let e = format!("{:#}", load(&dir, &cfg, &["miner"]).err().unwrap());
        assert!(e.contains("'gem' is no kind of thing"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

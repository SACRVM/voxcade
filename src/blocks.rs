//! Voxel blocks (`docs/tiles.md`): the pieces a level's grid is built from,
//! N by N by N voxels each, one file a block (`blocks/NAME.vvx`, `crate::vvx`). A block
//! starts as a 2D tile drawn out through its depth and is then shaped by
//! hand; a level's cell holds one in any of 48 turns.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use glam::IVec3;

/// Where a game keeps its blocks.
pub const FOLDER: &str = "blocks";

/// The file of the block `name` in the game folder `dir`.
pub fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join(FOLDER).join(format!("{name}.{}", crate::vvx::EXTENSION))
}

/// A block: N voxels each way, x across, y in depth (0 the front), z up.
/// (The same box of voxels, of any size, is a frame of a model in the
/// editor.)
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub size: IVec3,
    /// Per voxel (x + y w + z w d): 0 empty, else its colour,
    /// `palette[v - 1]`.
    pub voxels: Vec<u8>,
    pub palette: Vec<[u8; 3]>,
}

impl Block {
    /// An empty block, `n` voxels each way.
    pub fn empty(n: i32) -> Self {
        Self::boxed(IVec3::splat(n))
    }

    /// An empty box of voxels `size` big.
    pub fn boxed(size: IVec3) -> Self {
        Self { size, voxels: vec![0; size.max(IVec3::ZERO).element_product() as usize], palette: Vec::new() }
    }

    /// Voxel `p`'s place in `voxels` (None outside the block).
    pub fn index(&self, p: IVec3) -> Option<usize> {
        let s = self.size;
        (p.cmpge(IVec3::ZERO).all() && p.cmplt(s).all()).then(|| (p.x + p.y * s.x + p.z * s.x * s.y) as usize)
    }

    /// The voxel at place `i` of `voxels`.
    pub fn at(&self, i: usize) -> IVec3 {
        let (s, i) = (self.size, i as i32);
        IVec3::new(i % s.x, i / s.x % s.y, i / (s.x * s.y))
    }

    /// The voxel at `p`: 0 empty (and outside), else its colour's number.
    pub fn get(&self, p: IVec3) -> u8 {
        self.index(p).map_or(0, |i| self.voxels[i])
    }

    pub fn colour_at(&self, p: IVec3) -> Option<[u8; 3]> {
        match self.get(p) {
            0 => None,
            v => Some(self.palette[v as usize - 1]),
        }
    }

    /// Fills the voxel at `p` with `colour`, or empties it (None); a new
    /// colour joins the palette. Outside the block nothing happens.
    pub fn set(&mut self, p: IVec3, colour: Option<[u8; 3]>) -> Result<()> {
        let Some(i) = self.index(p) else { return Ok(()) };
        self.voxels[i] = match colour {
            None => 0,
            Some(c) => self.number(c)?,
        };
        Ok(())
    }

    /// The number of `colour` in the palette, added if new.
    fn number(&mut self, colour: [u8; 3]) -> Result<u8> {
        if let Some(i) = self.palette.iter().position(|&c| c == colour) {
            return Ok(i as u8 + 1);
        }
        if self.palette.len() >= 255 {
            self.compact();
        }
        if self.palette.len() >= 255 {
            bail!("a block holds at most 255 colors");
        }
        self.palette.push(colour);
        Ok(self.palette.len() as u8)
    }

    /// Every voxel's colour, in the block's order (`index`): two blocks
    /// alike have the same, however their colours are numbered.
    pub fn cells(&self) -> Vec<Option<[u8; 3]>> {
        (0..self.voxels.len()).map(|i| self.colour_at(self.at(i))).collect()
    }

    /// The colours its voxels use, each once, in palette order.
    pub fn colours(&self) -> Vec<[u8; 3]> {
        let mut block = self.clone();
        block.compact();
        block.palette
    }

    /// Drops the colours no voxel uses and those twice in the palette.
    pub fn compact(&mut self) {
        let mut used = [false; 256];
        for &v in &self.voxels {
            used[v as usize] = true;
        }
        let (mut palette, mut renumber) = (Vec::<[u8; 3]>::new(), [0u8; 256]);
        for (i, &c) in self.palette.iter().enumerate() {
            if !used[i + 1] {
                continue;
            }
            let j = palette.iter().position(|&k| k == c).unwrap_or_else(|| {
                palette.push(c);
                palette.len() - 1
            });
            renumber[i + 1] = j as u8 + 1;
        }
        for v in &mut self.voxels {
            *v = renumber[*v as usize];
        }
        self.palette = palette;
    }

    /// A 2D tile drawn out through the block: every layer, front to back,
    /// is the picture (`n` by `n`, rows from the top; a pixel shows from
    /// half opaque on).
    pub fn drawn_out(picture: &[[u8; 4]], n: i32) -> Result<Self> {
        let mut block = Self::empty(n);
        for row in 0..n {
            for x in 0..n {
                let [r, g, b, a] = picture.get((x + row * n) as usize).copied().unwrap_or([0; 4]);
                if a < 128 {
                    continue;
                }
                let v = block.number([r, g, b])?;
                for y in 0..n {
                    let i = block.index(IVec3::new(x, y, n - 1 - row)).unwrap();
                    block.voxels[i] = v;
                }
            }
        }
        Ok(block)
    }

    /// The block as it lies turned (a box's sides change places).
    pub fn turned(&self, turn: Turn) -> Self {
        let mut out = Self { size: turn.size(self.size), voxels: vec![0; self.voxels.len()], palette: self.palette.clone() };
        for i in 0..out.voxels.len() {
            out.voxels[i] = self.get(turn.source(out.at(i), self.size));
        }
        out
    }

    /// Reads a block, which must be `n` voxels each way.
    pub fn load(path: &Path, n: i32) -> Result<Self> {
        Self::load_in(&crate::files::Files::disk(), path, n)
    }

    /// A block of the game's files (`crate::files`): its one frame, the
    /// colours alone (a material counts for models, not for blocks).
    pub fn load_in(files: &crate::files::Files, path: &Path, n: i32) -> Result<Self> {
        let model = crate::vox::load_model_in(files, path)?;
        if model.size != IVec3::splat(n) {
            let s = model.size;
            bail!("{} is {} by {} by {} voxels; this level's blocks are {n} each way", path.display(), s.x, s.y, s.z);
        }
        // A level of blocks takes plain colors (its voxels are the world's):
        // glass, metal or glow would be lost without a word.
        let frame = model.frames.first().map_or(&[][..], |f| &f[..]);
        let metal = model.metal.first().map_or(&[][..], |m| &m[..]);
        let glow = model.glow.first().map_or(&[][..], |g| &g[..]);
        let odd = (0..frame.len()).find(|&i| frame[i].1[3] != 255 || metal.get(i).is_some_and(|&m| m != 0) || glow.get(i).is_some_and(|&g| g != 0));
        if let Some(i) = odd {
            let rgba = frame[i].1;
            let mat = crate::vvx::Mat { rgba, metal: metal.get(i).copied().unwrap_or(0), glow: glow.get(i).copied().unwrap_or(0) };
            let what = if rgba[3] != 255 { "glass" } else if mat.metal != 0 { "metal" } else { "glow" };
            let src = files.read_to_string(path).unwrap_or_default();
            let sign = crate::vvx::Palette::of_text(&src).entries().iter().find(|(_, m)| *m == mat).map(|(s, _)| s.clone());
            let line = sign.as_ref().and_then(|s| src.lines().position(|l| l.trim_start().strip_prefix(s.as_str()).is_some_and(|r| r.trim_start().starts_with('=')))).map_or(String::new(), |n| format!("line {}: ", n + 1));
            let sign = sign.map_or(String::new(), |s| format!("the palette's '{s}', "));
            bail!(
                "{}: {line}{sign}#{:02x}{:02x}{:02x}, is {what}: a block's colors are plain (a level's voxels have no glass, metal or glow; a piece, a model set down, may have them)",
                path.display(),
                rgba[0],
                rgba[1],
                rgba[2]
            );
        }
        // The colours numbered in the block's own order of voxels, as a
        // block made in the editor numbers them.
        let mut cells = vec![None; (n * n * n) as usize];
        let empty = Self::empty(n);
        for &(p, [r, g, b, _]) in model.frames.first().into_iter().flatten() {
            if let Some(i) = empty.index(p) {
                cells[i].get_or_insert([r, g, b]);
            }
        }
        let mut block = empty;
        for (i, c) in cells.into_iter().enumerate() {
            if c.is_some() {
                block.set(block.at(i), c)?;
            }
        }
        Ok(block)
    }

    /// The block as a model of one frame, for `crate::vvx`.
    pub fn model(&self) -> crate::vox::VoxModel {
        let frame: Vec<_> = (0..self.voxels.len()).filter_map(|i| self.colour_at(self.at(i)).map(|[r, g, b]| (self.at(i), [r, g, b, 255]))).collect();
        let metal = vec![0; frame.len()];
        crate::vox::VoxModel::new(self.size, vec![frame], vec![metal])
    }

    /// Writes the block as a `.vvx` file (keeping the signs the file had),
    /// at once (`save_at_once`), so a game watching the folder never reads
    /// half a block.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.save_saying(path).map(drop)
    }

    /// `save`, saying the text written (the editor remembers what it
    /// wrote, to see when another program writes the file).
    pub fn save_saying(&self, path: &Path) -> Result<String> {
        let text = crate::vvx::write(&self.model(), &crate::vvx::Palette::of_file(path));
        save_at_once(path, text.as_bytes())?;
        Ok(text)
    }
}

/// Writes a file at once: into a hidden file beside it first (named so no
/// game takes it for its own), then renamed over it.
pub(crate) fn save_at_once(path: &Path, bytes: &[u8]) -> Result<()> {
    let folder = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
    let name = path.file_name().context("a file needs a name")?.to_string_lossy();
    let temp = folder.join(format!(".{name}.tmp"));
    std::fs::write(&temp, bytes).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path).with_context(|| format!("writing {}", path.display()))
}

/// One of the 48 ways a block can lie in a cell: quarter turns about the
/// three axes, mirrored or not. Voxel `p` of the turned block is voxel
/// `source(p)` of the block as made: along each axis `i` it reads the
/// made block's axis `perm[i]`, from the far end where `flip[i]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Turn(u8);

const PERMS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];

impl Turn {
    /// As made.
    pub const NONE: Turn = Turn(0);
    /// A quarter turn about the upright, anticlockwise seen from above
    /// (across becomes in depth).
    pub const TURN: Turn = Turn(2 * 8 + 1);
    /// A quarter turn about the line of sight, anticlockwise as the camera
    /// sees it (across becomes up).
    pub const ROLL: Turn = Turn(5 * 8 + 1);
    /// Mirrored across: left becomes right.
    pub const MIRROR: Turn = Turn(1);
    pub const COUNT: u8 = 48;

    fn new(perm: [usize; 3], flip: [bool; 3]) -> Turn {
        let p = PERMS.iter().position(|&q| q == perm).expect("a permutation of the three axes");
        Turn(p as u8 * 8 + flip[0] as u8 + 2 * flip[1] as u8 + 4 * flip[2] as u8)
    }

    fn perm(self) -> [usize; 3] {
        PERMS[self.0 as usize / 8]
    }

    fn flip(self) -> [bool; 3] {
        [self.0 & 1 != 0, self.0 & 2 != 0, self.0 & 4 != 0]
    }

    /// Its number, as a level stores it (0..48).
    pub fn number(self) -> u8 {
        self.0
    }

    pub fn from_number(n: u8) -> Option<Turn> {
        (n < Self::COUNT).then_some(Turn(n))
    }

    /// The voxel of a block as made (`size` big) that lies at `p` once
    /// turned.
    pub fn source(self, p: IVec3, size: IVec3) -> IVec3 {
        let (perm, flip) = (self.perm(), self.flip());
        let mut s = IVec3::ZERO;
        for i in 0..3 {
            s[perm[i]] = if flip[i] { size[perm[i]] - 1 - p[i] } else { p[i] };
        }
        s
    }

    /// The size of a box `size` big once turned.
    pub fn size(self, size: IVec3) -> IVec3 {
        let perm = self.perm();
        IVec3::new(size[perm[0]], size[perm[1]], size[perm[2]])
    }

    /// Turned by `self`, then by `after`.
    pub fn then(self, after: Turn) -> Turn {
        let (ap, af, bp, bf) = (self.perm(), self.flip(), after.perm(), after.flip());
        Turn::new([ap[bp[0]], ap[bp[1]], ap[bp[2]]], [af[bp[0]] ^ bf[0], af[bp[1]] ^ bf[1], af[bp[2]] ^ bf[2]])
    }

    /// The turn that undoes it.
    pub fn inverse(self) -> Turn {
        let (perm, flip) = (self.perm(), self.flip());
        let (mut p, mut f) = ([0; 3], [false; 3]);
        for i in 0..3 {
            p[perm[i]] = i;
            f[perm[i]] = flip[i];
        }
        Turn::new(p, f)
    }

    /// A tile as a Tiled map lays it (flipped across, upside down, about
    /// its diagonal) as a turn of its block in the picture's plane. Tiled
    /// counts the picture's rows downwards; the block's z goes up.
    pub fn from_tiled(flip_x: bool, flip_y: bool, diagonal: bool) -> Turn {
        if diagonal {
            Turn::new([2, 1, 0], [!flip_x, false, !flip_y])
        } else {
            Turn::new([0, 1, 2], [flip_x, false, flip_y])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture whose pixels differ (up to 15 by 15): red across, green
    /// down.
    fn picture(n: i32) -> Vec<[u8; 4]> {
        (0..n * n).map(|i| [(i % n % 15) as u8 * 10, (i / n % 15) as u8 * 10, 7, 255]).collect()
    }

    #[test]
    fn a_tile_is_drawn_out_through_the_block() {
        let n = 4;
        let mut pic = picture(n);
        pic[1][3] = 0; // the top row's second pixel does not show
        let block = Block::drawn_out(&pic, n).unwrap();
        for y in 0..n {
            for row in 0..n {
                for x in 0..n {
                    let p = IVec3::new(x, y, n - 1 - row);
                    let [r, g, b, a] = pic[(x + row * n) as usize];
                    assert_eq!(block.colour_at(p), (a >= 128).then_some([r, g, b]), "at {p}");
                }
            }
        }
    }

    /// Tiled's own way of drawing a flipped tile (as the Tiled loader did):
    /// its pixel at (x, y) from the top left.
    fn tiled_pixel(pic: &[[u8; 4]], n: i32, x: i32, y: i32, flip_x: bool, flip_y: bool, diagonal: bool) -> [u8; 4] {
        let (mut u, mut v) = (x, y);
        if flip_y {
            v = n - 1 - v;
        }
        if flip_x {
            u = n - 1 - u;
        }
        if diagonal {
            (u, v) = (v, u);
        }
        pic[(u + v * n) as usize]
    }

    #[test]
    fn tiled_flips_turn_blocks_as_tiled_turns_tiles() {
        let n = 5;
        let pic = picture(n);
        let block = Block::drawn_out(&pic, n).unwrap();
        for bits in 0..8 {
            let (fx, fy, d) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            let turned = block.turned(Turn::from_tiled(fx, fy, d));
            for y in 0..n {
                for x in 0..n {
                    let [r, g, b, _] = tiled_pixel(&pic, n, x, y, fx, fy, d);
                    let p = IVec3::new(x, 2, n - 1 - y);
                    assert_eq!(turned.colour_at(p), Some([r, g, b]), "flip x {fx}, y {fy}, diagonal {d}: pixel ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn turns_compose_and_undo() {
        let n = 5;
        let mut block = Block::empty(n);
        block.set(IVec3::new(4, 2, 2), Some([1, 2, 3])).unwrap();
        block.set(IVec3::new(4, 1, 0), Some([4, 5, 6])).unwrap();
        // Across becomes in depth when turned, up when rolled.
        assert_eq!(block.turned(Turn::TURN).colour_at(IVec3::new(2, 4, 2)), Some([1, 2, 3]));
        assert_eq!(block.turned(Turn::ROLL).colour_at(IVec3::new(2, 2, 4)), Some([1, 2, 3]));
        assert_eq!(block.turned(Turn::MIRROR).colour_at(IVec3::new(0, 2, 2)), Some([1, 2, 3]));
        let mut all = std::collections::BTreeSet::new();
        for a in 0..Turn::COUNT {
            let a = Turn::from_number(a).unwrap();
            all.insert(block.turned(a).voxels);
            assert_eq!(block.turned(a).turned(a.inverse()), block, "{a:?} undone");
            assert_eq!(a.then(a.inverse()), Turn::NONE);
            for b in [Turn::TURN, Turn::ROLL, Turn::MIRROR, Turn::from_tiled(true, false, true)] {
                assert_eq!(block.turned(a).turned(b), block.turned(a.then(b)), "{a:?} then {b:?}");
            }
        }
        assert_eq!(all.len(), 48, "every turn lays the block differently");
        let four = Turn::TURN.then(Turn::TURN).then(Turn::TURN).then(Turn::TURN);
        assert_eq!(four, Turn::NONE);
    }

    fn folder(name: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("blocks-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_block_reads_back_as_written() {
        let dir = folder("roundtrip");
        let mut block = Block::drawn_out(&picture(16), 16).unwrap();
        block.set(IVec3::new(3, 15, 9), None).unwrap();
        block.set(IVec3::new(0, 0, 0), Some([200, 100, 50])).unwrap();
        block.palette.push([9, 9, 9]); // unused: dropped on saving
        let file = path(&dir, "79");
        block.save(&file).unwrap();
        let back = Block::load(&file, 16).unwrap();
        // The same colour in every voxel (the colours' numbering is the
        // file's reading order now, not a palette's).
        assert_eq!(back.cells(), block.cells());
        assert_eq!(back.colours().len(), block.colours().len());
        let names: Vec<_> = std::fs::read_dir(dir.join(FOLDER)).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["79.vvx"], "no file but the block is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_block_of_another_size_is_named() {
        let dir = folder("size");
        let file = path(&dir, "big");
        Block::drawn_out(&picture(8), 8).unwrap().save(&file).unwrap();
        let e = Block::load(&file, 16).unwrap_err().to_string();
        assert!(e.contains("big.vvx") && e.contains("8 by 8 by 8") && e.contains("16 each way"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A block's colors are plain: glass, metal or glow in its file is a
    /// mistake naming the file, the line and the sign (they were dropped
    /// without a word); a palette entry no voxel uses is no matter.
    #[test]
    fn a_block_of_glass_metal_or_glow_is_named() {
        let dir = folder("materials");
        let file = path(&dir, "odd");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let block = |entry: &str, used: &str| format!("format = \"voxcade voxel 1\"\nsize = [2, 2, 2]\n\n[palette]\na = \"#808080\"\nb = {entry}\n\n[[frame]]\nlayers = \"\"\"\naa\na{used}\n\naa\naa\n\"\"\"\n");
        for (entry, what) in [("{ color = \"#80c0ff\", glass = 0.5 }", "glass"), ("{ color = \"#c0c0c0\", metal = 0.8 }", "metal"), ("{ color = \"#ffd060\", glow = 1.0 }", "glow")] {
            std::fs::write(&file, block(entry, "b")).unwrap();
            let e = Block::load(&file, 2).unwrap_err().to_string();
            assert!(e.contains("odd.vvx: line 6: the palette's 'b'") && e.contains(&format!("is {what}: a block's colors are plain")), "{e}");
            std::fs::write(&file, block(entry, "a")).unwrap();
            assert!(Block::load(&file, 2).is_ok(), "{what} no voxel uses");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_block_holds_at_most_255_colours() {
        let mut block = Block::empty(16);
        for i in 0..255 {
            block.set(IVec3::new(i % 16, i / 16, 0), Some([i as u8, 0, 0])).unwrap();
        }
        let e = block.set(IVec3::new(15, 15, 0), Some([0, 1, 0])).unwrap_err().to_string();
        assert!(e.contains("255 colors"), "{e}");
        // A colour no voxel uses any more makes room.
        block.set(IVec3::new(0, 0, 0), Some([1, 0, 0])).unwrap();
        block.set(IVec3::new(15, 15, 0), Some([0, 1, 0])).unwrap();
        assert_eq!(block.colours().len(), 255);
    }

    /// A box turned has its sides changed over, its voxels with them; the
    /// turn undone brings it back.
    #[test]
    fn a_box_turns_with_its_sides() {
        let mut b = Block::boxed(IVec3::new(3, 2, 5));
        b.set(IVec3::new(2, 1, 4), Some([1, 2, 3])).unwrap();
        b.set(IVec3::new(0, 0, 0), Some([4, 5, 6])).unwrap();
        for turn in [Turn::TURN, Turn::ROLL, Turn::MIRROR, Turn::TURN.then(Turn::ROLL)] {
            let t = b.turned(turn);
            assert_eq!(t.size, turn.size(b.size));
            assert_eq!(t.voxels.iter().filter(|&&v| v != 0).count(), 2, "{turn:?}: every voxel inside");
            assert_eq!(t.turned(turn.inverse()), b, "{turn:?} undone");
        }
    }
}

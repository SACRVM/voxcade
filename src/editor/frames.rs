//! A model's file as the editor shapes it: Voxcade's own text, `.vvx`
//! (`crate::vvx`), read and written through `vvx::read_shape` and
//! `vvx::write_shape`. What it holds: the palette (each colour with the
//! sign the file gives it and its material: glass, metal, rough, glow),
//! and the frames, each a box of voxels (`Block`) as big as the model, or
//! a figure's parts, each a box of its own placed from the model's pivot
//! (`Placed`), with the parent and joint set by hand.
//!
//! Saved, a file changes only where it must: the signs it had stay (a new
//! colour takes the next free one; a colour no voxel has any more keeps
//! its sign until it is taken out of the palette), every place is from
//! the pivot (a part that grows changes the model's size and its own
//! `at`, never another part's), and a file saved as it was read is the
//! text it was, byte for byte. MagicaVoxel's `.vox` is never written: a
//! `.vox` model is brought in first (`voxcade import`, `voxcade upgrade`).

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use glam::{IVec3, Vec3};

use crate::blocks::Block;
use crate::vvx::{self, Mat, Palette, Piece, Shape};

/// A model file being shaped.
#[derive(Clone)]
pub struct ModelFile {
    /// Where it lies.
    pub path: PathBuf,
    /// The palette: every colour the frames may hold, in the file's order.
    pub palette: Vec<Colour>,
    /// A figure's box (the frames' boxes are the model's for frames): what
    /// the file says, grown when a part reaches out of it.
    pub size: IVec3,
    pub frames: Vec<Block>,
    /// A figure's parts, each made of the frame of its place (none for a
    /// model of frames).
    pub parts: Vec<Placed>,
    /// A MagicaVoxel file, shown only (the IDE's picture): never saved.
    pub foreign: bool,
    /// The text as it was read, and the model as it came of it: saved as
    /// it came, the text it was.
    text: String,
    loaded: Shaped,
    /// The signs of the text last read or written: a colour keeps its sign
    /// from one save to the next.
    signs: Palette,
}

/// A colour of the model: its colour in the frames (`key`, the colour as
/// it is but for a second entry of the same colour in another material,
/// which is a step off so that each has its own), the sign the file gave
/// it (none for a new one: the next free when written) and what the file
/// makes of it (its true colour and material).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colour {
    pub key: [u8; 3],
    pub sign: Option<String>,
    pub mat: Mat,
}

/// A part of a figure: its name, the frame it is made of, where that
/// frame's voxel 0, 0, 0 stands from the model's pivot (the file's `at`),
/// and the parent and joint (from the pivot) set by hand.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub name: String,
    pub frame: usize,
    pub low: IVec3,
    pub parent: Option<String>,
    pub joint: Option<Vec3>,
    /// Its rest rotation (`vvx::Piece::rotation`), kept as the file has it.
    pub rotation: [f32; 3],
}

/// What of a model file an editor's step takes and gives back: its
/// frames, a figure's parts as placed and rigged, its box and palette.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shaped {
    pub frames: Vec<Block>,
    pub parts: Vec<Placed>,
    pub size: IVec3,
    pub palette: Vec<Colour>,
}

/// A material plain: the colour alone.
pub fn plain(c: [u8; 3]) -> Mat {
    Mat { rgba: [c[0], c[1], c[2], 255], metal: 0, glow: 0 }
}

/// A material's words as the palette writes them (`crate::vvx`): GLASS
/// (how much it lets through), METAL, ROUGH (how dull its mirror is: of
/// metal or glass) and GLOW, each 0 to 1. ROUGH of a colour that mirrors
/// nothing yet is the reader's 0.3.
pub const WORDS: [&str; 4] = ["GLASS", "METAL", "ROUGH", "GLOW"];

pub fn words(m: Mat) -> [f32; 4] {
    let glass = if m.rgba[3] == 255 { 0.0 } else { 1.0 - m.rgba[3] as f32 / 255.0 };
    let rough = if m.metal == 0 { 0.3 } else { 1.0 - (m.metal & 15) as f32 / 15.0 };
    [glass, (m.metal >> 4) as f32 / 15.0, rough, m.glow as f32 / 255.0]
}

/// `m` with its word `which` (of `WORDS`) set to `v`: a colour that is
/// neither metal nor glass mirrors nothing, whatever ROUGH says.
pub fn with_word(m: Mat, which: usize, v: f32) -> Mat {
    let [glass, metal, rough, _] = words(m);
    let mut out = m;
    match which {
        0 => out.rgba[3] = if v <= 0.0 { 255 } else { vvx::alpha_of(v) },
        3 => out.glow = vvx::glow_of(v),
        _ => {
            let (metal, rough) = if which == 1 { (v, rough) } else { (metal, v) };
            let mirrors = metal > 0.0 || (glass > 0.0 && (which == 2 || m.metal != 0));
            out.metal = if mirrors { crate::vox::metal::pack(metal, 1.0 - rough) } else { 0 };
        }
    }
    out
}

/// The plain words for opening a MagicaVoxel model in the editor.
fn bring_it_in(path: &Path) -> anyhow::Error {
    let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
    anyhow!(
        "{name} is a MagicaVoxel file. The editor shapes Voxcade's own models (.vvx, text): `voxcade import <game> {name}` brings it in as {} (the IDE's upgrade does a whole game). A world or a map's prefab stays .vox: shape that in MagicaVoxel.",
        path.with_extension(vvx::EXTENSION).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())
    )
}

impl ModelFile {
    /// The `.vvx` at `path`, to shape; a `.vox` is refused in plain words.
    pub fn load(path: &Path) -> Result<Self> {
        if !vvx::is_model(path) {
            return Err(bring_it_in(path));
        }
        let text = std::fs::read_to_string(path).map_err(|e| anyhow!("reading {}: {e}", path.display()))?;
        let (shape, palette) = vvx::read_shape(&text).map_err(|e| anyhow!("{}: {e}", path.display()))?;
        Ok(Self::of(path, &shape, &palette, text, false))
    }

    /// A world or a map's prefab (`crate::scene`) to look at, its colours
    /// as a model's (its markers not shown): never saved, never shaped
    /// here, for its palette's order is its colours' numbers.
    pub fn look_at_scene(path: &Path) -> Result<Self> {
        let scene = crate::scene::load(path)?;
        let pivot = vvx::pivot(scene.size);
        let mat = |[r, g, b]: [u8; 3]| Mat { rgba: [r, g, b, 255], metal: 0, glow: 0 };
        let voxels = scene.cells.iter().enumerate().filter(|(_, v)| **v != 0 && (**v as usize) <= scene.colours.len()).map(|(i, v)| (scene.position(i) - pivot, mat(scene.colours[*v as usize - 1]))).collect();
        let shape = Shape { size: scene.size, parted: false, pieces: vec![Piece { voxels, ..Piece::default() }] };
        Ok(Self::of(path, &shape, &Palette::default(), String::new(), true))
    }

    /// The model at `path` to look at, a `.vox` too (a model not brought in
    /// yet): never saved.
    pub fn look(path: &Path) -> Result<Self> {
        if vvx::is_model(path) {
            return Self::load(path);
        }
        let model = crate::vox::import_model(path)?;
        let text = vvx::convert(&model, &Palette::default()).map_err(|e| anyhow!("{}: {e}", path.display()))?.text;
        let (shape, palette) = vvx::read_shape(&text)?;
        Ok(Self::of(path, &shape, &palette, String::new(), true))
    }

    /// The model a text's shape holds, each colour its own key.
    fn of(path: &Path, shape: &Shape, palette: &Palette, text: String, foreign: bool) -> Self {
        let mut colours: Vec<Colour> = Vec::new();
        fn key_of(mat: Mat, sign: Option<&String>, colours: &mut Vec<Colour>) -> [u8; 3] {
            if let Some(c) = colours.iter().find(|c| c.mat == mat) {
                return c.key;
            }
            let key = free_key(colours, [mat.rgba[0], mat.rgba[1], mat.rgba[2]]);
            colours.push(Colour { key, sign: sign.cloned(), mat });
            key
        }
        for (sign, mat) in palette.entries() {
            key_of(*mat, Some(sign), &mut colours);
        }
        let pivot = vvx::pivot(shape.size);
        let (mut frames, mut parts) = (Vec::new(), Vec::new());
        for (i, piece) in shape.pieces.iter().enumerate() {
            let (low, size) = if shape.parted {
                let lo = piece.voxels.iter().map(|v| v.0).reduce(IVec3::min);
                let hi = piece.voxels.iter().map(|v| v.0).reduce(IVec3::max);
                match (lo, hi) {
                    (Some(lo), Some(hi)) => (lo, hi - lo + 1),
                    _ => (-pivot, IVec3::ONE),
                }
            } else {
                (-pivot, shape.size)
            };
            let mut block = Block::boxed(size);
            for &(v, mat) in &piece.voxels {
                let key = key_of(mat, None, &mut colours);
                // (A block holds 255 colours; a frame of more is shown
                // without the rest.)
                let _ = block.set(v - low, Some(key));
            }
            frames.push(block);
            if shape.parted {
                parts.push(Placed { name: piece.name.clone(), frame: i, low, parent: piece.parent.clone(), joint: piece.joint, rotation: piece.rotation });
            }
        }
        let loaded = Shaped { frames: frames.clone(), parts: parts.clone(), size: shape.size, palette: colours.clone() };
        Self { path: path.to_path_buf(), palette: colours, size: shape.size, frames, parts, foreign, signs: Palette::of_text(&text), text, loaded }
    }

    /// A new model to begin with (the IDE's NEW MODEL): one frame, 8 voxels
    /// each way, a cube of 4 standing on the floor in one colour. Not saved
    /// until `save`.
    pub fn begun(path: &Path) -> Self {
        let mut frame = Block::empty(8);
        for z in 0..4 {
            for y in 2..6 {
                for x in 2..6 {
                    let _ = frame.set(IVec3::new(x, y, z), Some([204, 112, 72]));
                }
            }
        }
        let palette = vec![Colour { key: [204, 112, 72], sign: None, mat: plain([204, 112, 72]) }];
        let loaded = Shaped::default();
        Self { path: path.to_path_buf(), palette, size: frame.size, frames: vec![frame], parts: Vec::new(), foreign: false, text: String::new(), loaded, signs: Palette::default() }
    }

    /// Its frames, parts, box and palette as a step keeps them.
    pub fn shaped(&self) -> Shaped {
        Shaped { frames: self.frames.clone(), parts: self.parts.clone(), size: self.size, palette: self.palette.clone() }
    }

    /// Its frames, parts, box and palette as a step gives them back.
    pub fn reshape(&mut self, to: &Shaped) {
        self.frames = to.frames.clone();
        self.parts = to.parts.clone();
        self.size = to.size;
        self.palette = to.palette.clone();
    }

    /// Whether it is as `shaped` holds it.
    pub fn is(&self, shaped: &Shaped) -> bool {
        self.frames == shaped.frames && self.parts == shaped.parts && self.size == shaped.size && self.palette == shaped.palette
    }

    /// It is a figure of parts: its frames are its parts, not frames of a
    /// walk.
    pub fn parted(&self) -> bool {
        !self.parts.is_empty()
    }

    /// Where voxel 0, 0, 0 of frame `f` stands from the pivot (a figure's
    /// part).
    pub fn origin(&self, f: usize) -> Option<IVec3> {
        self.parts.iter().find(|p| p.frame == f).map(|p| p.low)
    }

    /// Every colour: the palette's (used or not), then any a frame holds
    /// that the palette lacks.
    pub fn colours(&self) -> Vec<[u8; 3]> {
        let mut all: Vec<[u8; 3]> = self.palette.iter().map(|c| c.key).collect();
        for c in self.frames.iter().flat_map(Block::colours) {
            if !all.contains(&c) {
                all.push(c);
            }
        }
        all
    }

    /// Whether a voxel of some frame has the colour `key`.
    pub fn uses(&self, key: [u8; 3]) -> bool {
        self.frames.iter().any(|f| f.colours().contains(&key))
    }

    /// What the colour `key` is in the file: its entry's, else plain.
    pub fn material(&self, key: [u8; 3]) -> Mat {
        self.palette.iter().find(|c| c.key == key).map_or_else(|| plain(key), |c| c.mat)
    }

    /// The colour `key` made of `mat` (its true colour stays the key's own
    /// when the entry had none); an entry of the palette from now on.
    pub fn set_material(&mut self, key: [u8; 3], mat: Mat) {
        match self.palette.iter_mut().find(|c| c.key == key) {
            Some(c) => c.mat = mat,
            None => self.palette.push(Colour { key, sign: None, mat }),
        }
    }

    /// The colour `key` taken out of the palette, if no voxel has it.
    pub fn forget(&mut self, key: [u8; 3]) -> bool {
        if self.uses(key) {
            return false;
        }
        let before = self.palette.len();
        self.palette.retain(|c| c.key != key);
        self.palette.len() != before
    }

    /// The colours the frames hold that the palette lacks, put in it
    /// (plain), so that they keep their place when no voxel has them any
    /// more (the editor, after every step).
    pub fn list_new_colours(&mut self) {
        for c in self.colours() {
            if !self.palette.iter().any(|p| p.key == c) {
                self.palette.push(Colour { key: c, sign: None, mat: plain(c) });
            }
        }
    }

    /// What the file says now (`Shape`): a figure's box grown to hold
    /// every part about the same pivot (raised, every part alike, when one
    /// reaches under the floor), the voxels in the text's reading order.
    pub fn shape(&self) -> Shape {
        if !self.parted() {
            let size = self.frames.iter().map(|f| f.size).fold(IVec3::ONE, IVec3::max);
            let pivot = vvx::pivot(size);
            let pieces = self.frames.iter().map(|f| Piece { voxels: self.reading(f, -pivot), ..Piece::default() }).collect();
            return Shape { size, parted: false, pieces };
        }
        let (raise, size) = self.fitted();
        let pieces = self
            .parts
            .iter()
            .map(|p| {
                let frame = self.frames.get(p.frame);
                let voxels = frame.map_or_else(Vec::new, |f| self.reading(f, p.low + raise));
                Piece { name: p.name.clone(), voxels, parent: p.parent.clone(), joint: p.joint.map(|j| j + raise.as_vec3()), rotation: p.rotation }
            })
            .collect();
        Shape { size, parted: true, pieces }
    }

    /// A figure's raise (when a part reaches under the floor) and its box:
    /// the file's, grown to hold every part's voxels about its pivot.
    fn fitted(&self) -> (IVec3, IVec3) {
        let mut bounds: Option<(IVec3, IVec3)> = None;
        for p in &self.parts {
            if let Some(f) = self.frames.get(p.frame) {
                for k in (0..f.voxels.len()).filter(|&k| f.voxels[k] != 0) {
                    let v = f.at(k) + p.low;
                    bounds = Some(bounds.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
                }
            }
        }
        let Some((lo, hi)) = bounds else { return (IVec3::ZERO, self.size) };
        let raise = IVec3::Z * (-lo.z).max(0);
        (raise, fit(self.size, lo + raise, hi + raise))
    }

    /// A box's voxels from its `low` corner (from the pivot), in the
    /// text's reading order: layer by layer from the bottom, rows far to
    /// near, left to right; each with its material.
    fn reading(&self, f: &Block, low: IVec3) -> Vec<(IVec3, Mat)> {
        let s = f.size;
        let mut out = Vec::new();
        for z in 0..s.z {
            for y in (0..s.y).rev() {
                for x in 0..s.x {
                    let p = IVec3::new(x, y, z);
                    if let Some(c) = f.colour_at(p) {
                        out.push((p + low, self.material(c)));
                    }
                }
            }
        }
        out
    }

    /// The palette to write with: each colour with its sign (a new one's
    /// the sign the text last written gave it, if any), in the palette's
    /// order.
    fn kept(&self) -> Palette {
        let mut entries: Vec<(String, Mat)> = Vec::new();
        for c in &self.palette {
            let sign = c.sign.clone().or_else(|| self.signs.entries().iter().find(|(s, m)| *m == c.mat && !self.palette.iter().any(|o| o.sign.as_ref() == Some(s))).map(|(s, _)| s.clone()));
            if let Some(sign) = sign.filter(|s| !entries.iter().any(|(k, _)| k == s)) {
                entries.push((sign, c.mat));
            }
        }
        Palette::new(entries)
    }

    /// The text it would be saved as: the text it was read from while it
    /// is as it came, else written anew, but what would come out as it
    /// did keeps the text it had (`keep_text`): a part moved changes its
    /// `at`, never another part's lines.
    pub fn text(&self) -> String {
        if !self.text.is_empty() && self.is(&self.loaded) {
            return self.text.clone();
        }
        let new = self.write_text();
        if self.text.is_empty() {
            return new;
        }
        keep_text(&self.text, &new).unwrap_or(new)
    }

    /// The text written anew from what it holds (`text` without the
    /// shortcut).
    pub fn write_text(&self) -> String {
        vvx::write_shape(&self.shape(), &self.kept(), true)
    }

    /// Writes it at once (`blocks::save_at_once`): the text written.
    pub fn save(&mut self) -> Result<String> {
        if self.foreign {
            return Err(bring_it_in(&self.path));
        }
        let text = self.text();
        crate::blocks::save_at_once(&self.path, text.as_bytes())?;
        self.signs = Palette::of_text(&text);
        Ok(text)
    }

    /// A figure of parts as the game would make it of the file saved now
    /// (`vvx::model_of`): the model, each part's frame and where the
    /// figure's low corner (voxel 0, 0, 0 of the model) stands from the
    /// pivot; None for a model of frames.
    pub fn figure(&self) -> Option<Result<(crate::vox::VoxModel, Vec<usize>, IVec3)>> {
        if !self.parted() {
            return None;
        }
        let shape = self.shape();
        let names: Vec<&str> = shape.pieces.iter().map(|p| p.name.as_str()).collect();
        if let Some((p, q)) = shape.pieces.iter().find_map(|p| p.parent.as_deref().filter(|q| !names.contains(q)).map(|q| (&p.name, q))) {
            return Some(Err(anyhow!("part '{p}' hangs on '{q}', but no part is called so")));
        }
        let (raise, _) = self.fitted();
        let model = vvx::model_of(&shape);
        let frames = self.parts.iter().map(|p| p.frame).collect();
        Some(Ok((model, frames, -vvx::pivot(shape.size) - raise)))
    }
}

/// The box `size` grown (never shrunk) to hold `lo` to `hi` (from its
/// pivot, z from the floor up) about the same pivot: across and deep
/// on both sides of it as needed.
pub fn fit(size: IVec3, lo: IVec3, hi: IVec3) -> IVec3 {
    let mut size = size.max(IVec3::ONE);
    for a in 0..2 {
        while -(size[a] / 2) > lo[a] || size[a] - size[a] / 2 - 1 < hi[a] {
            size[a] += 1;
        }
    }
    size.z = size.z.max(hi.z + 1);
    size
}

/// The smallest box holding `lo` to `hi` (from its pivot) about that
/// pivot, standing on `lo`'s layer.
pub fn least(lo: IVec3, hi: IVec3) -> IVec3 {
    fit(IVec3::new(1, 1, hi.z - lo.z + 1), IVec3::new(lo.x, lo.y, 0), IVec3::new(hi.x, hi.y, 0))
}

/// A change to a model's cells made without the window (`voxcade edit
/// --model FILE --clear ... | --fill ...`): a box of cells (from the
/// pivot, as the file's `at`; both corners in) emptied, or filled with a
/// color (a sign of the palette, or `#rrggbb`).
#[derive(Clone, Debug, PartialEq)]
pub enum CellOp {
    Clear(IVec3, IVec3),
    Fill(String, IVec3, IVec3),
}

/// Which frame or part a `CellOp` changes.
#[derive(Clone, Debug, PartialEq)]
pub enum CellsOf {
    /// A model's frame (from 1, as the editor counts them).
    Frame(usize),
    Part(String),
}

/// What `change_cells` did: cells set to a color they did not have, cells
/// emptied, the box before and after if it grew.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changed {
    pub set: usize,
    pub cleared: usize,
    pub grown: Option<(IVec3, IVec3)>,
}

impl std::fmt::Display for Changed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} cells set, {} cleared", self.set, self.cleared)?;
        if let Some((was, now)) = self.grown {
            write!(f, ", the box grown from {} x {} x {} to {} x {} x {}", was.x, was.y, was.z, now.x, now.y, now.z)?;
        }
        Ok(())
    }
}

impl ModelFile {
    /// The key of a color as `--fill` names it: a sign of the palette, or
    /// `#rrggbb` (an entry of that color plain, else a new one, which
    /// takes the next free sign when written, as the editor's does).
    fn key_named(&mut self, color: &str) -> Result<[u8; 3]> {
        if let Some(hex) = color.strip_prefix('#') {
            let rgb = (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten().ok_or_else(|| anyhow!("'{color}' is no color: #rrggbb"))?;
            let rgb = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
            if let Some(c) = self.palette.iter().find(|c| c.mat == plain(rgb)) {
                return Ok(c.key);
            }
            let key = free_key(&self.palette, rgb);
            self.palette.push(Colour { key, sign: None, mat: plain(rgb) });
            return Ok(key);
        }
        self.palette.iter().find(|c| c.sign.as_deref() == Some(color)).map(|c| c.key).ok_or_else(|| {
            let signs: Vec<&str> = self.palette.iter().filter_map(|c| c.sign.as_deref()).collect();
            anyhow!("the palette has no sign '{color}' (its signs: {}); or give a color as #rrggbb", signs.join(", "))
        })
    }

    /// Changes a frame's or a part's cells as `ops` say, in order: a box
    /// that reaches past the frame's grows it (a part's about itself, a
    /// model of frames about its pivot, every frame alike; never under the
    /// floor). The text is written as the editor writes it (`save`).
    pub fn change_cells(&mut self, of: &CellsOf, ops: &[CellOp]) -> Result<Changed> {
        if self.foreign {
            return Err(bring_it_in(&self.path));
        }
        let (f, part) = match of {
            CellsOf::Frame(n) if self.parted() => bail!("--frame {n}: this model is a figure of parts: --part NAME (its parts: {})", self.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")),
            CellsOf::Frame(n) => (n.checked_sub(1).filter(|&f| f < self.frames.len()).ok_or_else(|| anyhow!("--frame {n}: the model has frames 1 to {}", self.frames.len()))?, None),
            CellsOf::Part(name) => {
                let k = self.parts.iter().position(|p| &p.name == name).ok_or_else(|| anyhow!("--part {name}: the figure has no such part (its parts: {})", self.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")))?;
                (self.parts[k].frame, Some(k))
            }
        };
        let pivot = |size: IVec3| vvx::pivot(size);
        let mut changed = Changed::default();
        let box_size = |m: &Self| if m.parted() { m.fitted().1 } else { m.frames[f].size };
        let before = box_size(self);
        for op in ops {
            let (a, b, key) = match op {
                CellOp::Clear(a, b) => (*a, *b, None),
                CellOp::Fill(color, a, b) => (*a, *b, Some(self.key_named(color)?)),
            };
            let (lo, hi) = (a.min(b), a.max(b));
            if key.is_some() && lo.z < 0 {
                bail!("z {} is under the floor: a model stands on z 0", lo.z);
            }
            // Where the frame's cell 0 lies from the pivot, grown to hold a fill.
            let mut low = match part {
                Some(k) => self.parts[k].low,
                None => -pivot(self.frames[f].size),
            };
            if key.is_some() {
                let size = self.frames[f].size;
                let (was_lo, was_hi) = (low, low + size - 1);
                if lo.cmplt(was_lo).any() || hi.cmpgt(was_hi).any() {
                    match part {
                        Some(k) => {
                            let (new_lo, new_hi) = (was_lo.min(lo), was_hi.max(hi));
                            let mut grown = Block::boxed(new_hi - new_lo + 1);
                            copy_into(&self.frames[f], &mut grown, was_lo - new_lo)?;
                            self.frames[f] = grown;
                            self.parts[k].low = new_lo;
                            low = new_lo;
                        }
                        None => {
                            let size = fit(self.frames[f].size, lo.min(was_lo), hi.max(was_hi));
                            for frame in &mut self.frames {
                                let mut grown = Block::boxed(size);
                                copy_into(frame, &mut grown, pivot(size) - pivot(frame.size))?;
                                *frame = grown;
                            }
                            self.size = size;
                            low = -pivot(size);
                        }
                    }
                }
            }
            let frame = &mut self.frames[f];
            for z in lo.z..=hi.z {
                for y in lo.y..=hi.y {
                    for x in lo.x..=hi.x {
                        let c = IVec3::new(x, y, z) - low;
                        if frame.index(c).is_none() {
                            continue;
                        }
                        let was = frame.colour_at(c);
                        if was == key {
                            continue;
                        }
                        frame.set(c, key)?;
                        match key {
                            Some(_) => changed.set += 1,
                            None => changed.cleared += 1,
                        }
                    }
                }
            }
        }
        self.list_new_colours();
        let after = box_size(self);
        changed.grown = (before != after).then_some((before, after));
        Ok(changed)
    }
}

/// Every voxel of `from` into `to`, moved by `shift`.
fn copy_into(from: &Block, to: &mut Block, shift: IVec3) -> Result<()> {
    for k in 0..from.voxels.len() {
        if let Some(c) = from.colour_at(from.at(k)) {
            to.set(from.at(k) + shift, Some(c))?;
        }
    }
    Ok(())
}

/// One `[[part]]` or `[[frame]]` of a model's text: the comments right
/// above its header, the header through its last line, and the blank
/// lines after it.
struct Unit<'a> {
    body: &'a str,
    gap: &'a str,
}

/// A model's text cut into its head (the format, size and palette, its
/// blank lines after it apart) and its frames' or parts' units; None where
/// a line past the head opens a table of another kind.
fn units(src: &str) -> Option<(&str, &str, Vec<Unit<'_>>)> {
    let mut starts: Vec<usize> = Vec::new();
    let mut at = 0;
    for line in src.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with("[[part]]") || t.starts_with("[[frame]]") {
            starts.push(at);
        } else if t.starts_with('[') && !starts.is_empty() {
            return None;
        }
        at += line.len();
    }
    // Each header with the comments right above it.
    let begin = |h: usize| {
        let mut s = h;
        while s > 0 {
            let above = src[..s - 1].rfind('\n').map_or(0, |i| i + 1);
            if !src[above..s].trim_start().starts_with('#') {
                break;
            }
            s = above;
        }
        s
    };
    let begins: Vec<usize> = starts.iter().map(|&h| begin(h)).collect();
    // A stretch's text and the blank lines that end it, apart.
    let split = |s: &'_ str| -> (usize, usize) {
        let mut end = s.len();
        while end > 0 {
            let above = s[..end - 1].rfind('\n').map_or(0, |i| i + 1);
            if !s[above..end].trim().is_empty() {
                break;
            }
            end = above;
        }
        (end, s.len())
    };
    let head_all = &src[..begins.first().copied().unwrap_or(src.len())];
    let (h, _) = split(head_all);
    let mut out = Vec::new();
    for (k, &b) in begins.iter().enumerate() {
        let e = begins.get(k + 1).copied().unwrap_or(src.len());
        let (body_end, _) = split(&src[b..e]);
        out.push(Unit { body: &src[b..b + body_end], gap: &src[b + body_end..e] });
    }
    Some((&head_all[..h], &head_all[h..], out))
}

/// The `layers` of a unit's text: where its line begins and its value
/// ends (the line's end).
fn layers_span(body: &str) -> Option<std::ops::Range<usize>> {
    let doc = toml_edit::Document::parse(body).ok()?;
    let table = doc.as_table().iter().next()?.1.as_array_of_tables()?.get(0)?;
    let (key, value) = table.get_key_value("layers")?;
    let (k, v) = (key.span()?, value.span()?);
    let begin = body[..k.start].rfind('\n').map_or(0, |i| i + 1);
    let end = body[v.end..].find('\n').map_or(body.len(), |i| v.end + i + 1);
    Some(begin..end)
}

/// The text `new` (the model written anew) with what it says as `old`
/// said it in `old`'s own words: the head, if the writer gives it as it
/// would give the old's; each part (by name) or frame (by its place)
/// whose unit comes out the same, whole, its comments and repeats as
/// written; one whose voxels did not change (it moved, its rig changed)
/// with its old `layers` lines. None where either text cannot be cut so.
fn keep_text(old: &str, new: &str) -> Option<String> {
    let (shape, palette) = vvx::read_shape(old).ok()?;
    let norm = vvx::write_shape(&shape, &palette, true);
    let (old_head, old_gap, old_units) = units(old)?;
    let (norm_head, _, norm_units) = units(&norm)?;
    let (new_head, new_gap, new_units) = units(new)?;
    if old_units.len() != norm_units.len() || old_units.len() != shape.pieces.len() {
        return None;
    }
    let new_shape = vvx::read_shape(new).ok()?.0;
    if new_shape.pieces.len() != new_units.len() {
        return None;
    }
    // Each new unit's old one: a part by its name, a frame by its place.
    let old_of = |i: usize| -> Option<usize> {
        if new_shape.parted != shape.parted {
            return None;
        }
        if shape.parted { shape.pieces.iter().position(|p| p.name == new_shape.pieces[i].name) } else { (i < shape.pieces.len()).then_some(i) }
    };
    let head_kept = new_head == norm_head;
    let mut out = String::from(if head_kept { old_head } else { new_head });
    let mut last_old: Option<usize> = None;
    for (i, unit) in new_units.iter().enumerate() {
        let j = old_of(i);
        let kept = j.filter(|&j| norm_units[j].body == unit.body);
        // The blank lines before it: the old's between two old neighbours.
        let gap = match (i, kept) {
            (0, Some(0)) if head_kept => old_gap,
            (0, _) => new_gap,
            (_, Some(j)) if last_old.is_some_and(|l| l + 1 == j) => old_units[j - 1].gap,
            _ => new_units[i - 1].gap,
        };
        out += gap;
        match (kept, j) {
            (Some(j), _) => out += old_units[j].body,
            (None, Some(j)) => {
                // Its voxels as they were: the old `layers` lines in the new unit.
                let (n, o, m) = (layers_span(unit.body), layers_span(old_units[j].body), layers_span(norm_units[j].body));
                match (n, o, m) {
                    (Some(n), Some(o), Some(m)) if unit.body[n.clone()] == norm_units[j].body[m.clone()] => {
                        out += &unit.body[..n.start];
                        out += &old_units[j].body[o];
                        out += &unit.body[n.end..];
                    }
                    _ => out += unit.body,
                }
            }
            _ => out += unit.body,
        }
        last_old = kept;
    }
    // The end: the old's last blank lines if it ends as the old did.
    let tail = match (last_old, new_units.last()) {
        (Some(j), _) if j + 1 == old_units.len() => old_units[j].gap,
        (_, Some(u)) => u.gap,
        (_, None) => new_gap,
    };
    out += tail;
    // Never a text that says something else.
    (vvx::read_shape(&out).ok()?.0 == new_shape).then_some(out)
}

/// A colour for a palette entry that no other entry has: `want` itself,
/// else a step off it (a second entry of a colour in another material).
pub(crate) fn free_key(colours: &[Colour], want: [u8; 3]) -> [u8; 3] {
    let taken = |k: [u8; 3]| colours.iter().any(|c| c.key == k);
    if !taken(want) {
        return want;
    }
    for step in 1..=255i32 {
        for i in [2, 1, 0] {
            for sign in [1, -1] {
                let mut k = want;
                let v = want[i] as i32 + sign * step;
                if (0..=255).contains(&v) {
                    k[i] = v as u8;
                    if !taken(k) {
                        return k;
                    }
                }
            }
        }
    }
    want
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every model and block of every game and template, opened in the
    /// editor and written anew without a change, is the text it was, byte
    /// for byte (not the text kept: written from what the editor holds).
    #[test]
    fn every_model_written_unchanged_is_the_same_text() {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let p = entry.path();
                // A world's and the prefabs' are no models (`crate::scene`).
                let scene = p.ends_with("world.vvx") || p.parent().is_some_and(|f| f.ends_with("prefabs") || f.ends_with("levels"));
                if p.is_dir() {
                    walk(&p, out);
                } else if vvx::is_model(&p) && !scene {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        for folder in ["games", "templates", "examples"] {
            walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join(folder), &mut files);
        }
        assert!(files.len() > 100, "the models: {}", files.len());
        for file in files {
            let model = ModelFile::load(&file).unwrap();
            assert!(model.write_text() == std::fs::read_to_string(&file).unwrap(), "{}: written anew, another text", file.display());
            assert_eq!(model.text(), std::fs::read_to_string(&file).unwrap());
        }
    }

    /// A text of two frames in two materials of one colour, and a figure.
    fn frames_text() -> String {
        format!("format = \"{}\"\nsize = [3, 1, 2]\n\n[palette]\nq = \"#ff0000\"\nb = \"#0000ff\"\ng = {{ color = \"#ff0000\", glow = 1.0 }}\n\n[[frame]]\nlayers = \"\"\"\nqbg\n\"\"\"\n\n[[frame]]\nat = [0, 0, 1]\nlayers = \"\"\"\nb\n\"\"\"\n", vvx::FORMAT)
    }

    fn write(name: &str, text: &str) -> (PathBuf, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("frames-{name}"));
        let file = dir.join("thing.vvx");
        std::fs::write(&file, text).unwrap();
        (file, dir)
    }

    /// An edit changes only the lines it must: the signs stay, a new
    /// colour takes the next free one, a colour no voxel has keeps its
    /// sign until it is taken out; the same colour in another material
    /// keeps its own sign and material.
    #[test]
    fn an_edit_changes_only_what_it_must() {
        let (file, _dir) = write("signs", &frames_text());
        let mut m = ModelFile::load(&file).unwrap();
        let glow = m.palette.iter().find(|c| c.mat.glow != 0).unwrap().key;
        assert_ne!(glow, [255, 0, 0], "the glowing red has a key of its own");
        // A voxel of the second frame painted green.
        m.frames[1].set(IVec3::new(1, 0, 1), Some([0, 255, 0])).unwrap();
        m.list_new_colours();
        m.save().unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("q = \"#ff0000\"\nb = \"#0000ff\"\ng = { color = \"#ff0000\", glow = 1.0 }\na = \"#00ff00\"\n"), "{text}");
        assert!(text.contains("qbg\n") && text.ends_with("at = [0, 0, 1]\nlayers = \"\"\"\na\n\"\"\"\n"), "{text}");
        // Blue no longer used anywhere: it keeps its sign in the palette.
        m.frames[0].set(IVec3::new(1, 0, 0), None).unwrap();
        m.save().unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("b = \"#0000ff\"") && text.contains("q.g\n"), "{text}");
        assert!(m.forget([0, 0, 255]));
        m.save().unwrap();
        assert!(!std::fs::read_to_string(&file).unwrap().contains("#0000ff"));
        // Read again: the glow is where it was.
        let back = crate::vvx::load_in(&crate::files::Files::disk(), &file).unwrap();
        assert_eq!(back.glow[0], vec![0, 255]);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    /// A material is set on a colour: written as the palette's words.
    #[test]
    fn a_colour_takes_a_material() {
        let (file, _dir) = write("material", &frames_text());
        let mut m = ModelFile::load(&file).unwrap();
        m.set_material([0, 0, 255], Mat { rgba: [0, 0, 255, 128], metal: crate::vox::metal::pack(0.0, 0.9), glow: 0 });
        m.save().unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("b = { color = \"#0000ff\", glass = 0.5, rough = 0.1 }"), "{text}");
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    /// A MagicaVoxel model is not shaped: it is said how to bring it in;
    /// to look at, it is shown.
    #[test]
    fn a_vox_model_is_brought_in_first() {
        let dir = crate::scratch::Scratch::new("frames-vox");
        std::fs::create_dir_all(&dir).unwrap();
        crate::vox::write_figure(&dir.join("robot.vox"), &[("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("arm", IVec3::new(1, 2, 5), IVec3::new(6, 1, 3))]);
        let e = ModelFile::load(&dir.join("robot.vox")).err().unwrap().to_string();
        assert!(e.contains("MagicaVoxel") && e.contains("voxcade import") && e.contains("robot.vvx"), "{e}");
        let mut seen = ModelFile::look(&dir.join("robot.vox")).unwrap();
        assert!(seen.parted() && seen.foreign);
        assert!(seen.save().is_err(), "a .vox is never written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_box_grows_about_its_pivot() {
        assert_eq!(fit(IVec3::new(5, 3, 2), IVec3::new(-3, -1, 0), IVec3::new(2, 1, 1)), IVec3::new(6, 3, 2), "one more on the left");
        assert_eq!(fit(IVec3::new(5, 3, 2), IVec3::new(-2, -1, 0), IVec3::new(3, 1, 1)), IVec3::new(7, 3, 2), "on the right: one each side");
        assert_eq!(least(IVec3::new(0, 0, 2), IVec3::new(1, 0, 4)), IVec3::new(3, 1, 3));
    }

    /// `--fill` and `--clear` without the window: cells set and emptied
    /// by their place from the pivot; a color new to the palette takes
    /// the next free sign; a box past the frame's grows it about the
    /// pivot; written as the editor writes (the other frame's text kept).
    #[test]
    fn cells_change_as_the_editor_writes_them() {
        let (file, _dir) = write("cells", &frames_text());
        let mut m = ModelFile::load(&file).unwrap();
        // The model is 3 by 1 by 2, its pivot at x 1: frame 1's row "qbg"
        // stands at x -1, 0, 1.
        let changed = m.change_cells(&CellsOf::Frame(1), &[CellOp::Clear(IVec3::new(-1, 0, 0), IVec3::new(-1, 0, 0)), CellOp::Fill("#00ff00".into(), IVec3::new(1, 0, 0), IVec3::new(1, 0, 0))]).unwrap();
        assert_eq!((changed.set, changed.cleared, changed.grown), (1, 1, None));
        m.save().unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("a = \"#00ff00\"\n") && text.contains("at = [0, 0, 0]\nlayers = \"\"\"\nba\n"), "{text}");
        assert!(text.ends_with("at = [0, 0, 1]\nlayers = \"\"\"\nb\n\"\"\"\n"), "the second frame's text as it was: {text}");
        // A sign of the palette, past the box: it grows about the pivot,
        // every frame alike.
        let changed = m.change_cells(&CellsOf::Frame(2), &[CellOp::Fill("q".into(), IVec3::new(2, 0, 0), IVec3::new(2, 0, 0))]).unwrap();
        assert_eq!(changed.grown, Some((IVec3::new(3, 1, 2), IVec3::new(5, 1, 2))));
        m.save().unwrap();
        let back = ModelFile::load(&file).unwrap();
        assert_eq!(back.frames[1].colour_at(IVec3::new(4, 0, 0)), back.palette.iter().find(|c| c.sign.as_deref() == Some("q")).map(|c| c.key));
        assert!(m.change_cells(&CellsOf::Frame(3), &[]).is_err() && m.change_cells(&CellsOf::Frame(1), &[CellOp::Fill("zz".into(), IVec3::ZERO, IVec3::ZERO)]).is_err());
        assert!(m.change_cells(&CellsOf::Frame(1), &[CellOp::Fill("q".into(), IVec3::new(0, 0, -1), IVec3::ZERO)]).is_err(), "under the floor");
    }

    /// A figure's part filled past its box grows that part alone; every
    /// other part keeps its text byte for byte.
    #[test]
    fn a_part_grows_by_its_cells_alone() {
        let text = format!("format = \"{}\"\nsize = [4, 2, 3]\n\n[palette]\na = \"#ff0000\"\nb = \"#0000ff\"\n\n# The body.\n[[part]]\nname = \"body\"\nat = [-2, -1, 0]\nlayers = \"\"\"\naaaa\naaaa\n*2\n\"\"\"\n\n[[part]]\nname = \"head\"\nat = [-1, -1, 2]\nlayers = \"\"\"\nbb\nbb\n\"\"\"\n", vvx::FORMAT);
        let (file, _dir) = write("cells-part", &text);
        let mut m = ModelFile::load(&file).unwrap();
        let changed = m.change_cells(&CellsOf::Part("head".into()), &[CellOp::Fill("b".into(), IVec3::new(-1, -1, 3), IVec3::new(0, 0, 3))]).unwrap();
        assert_eq!((changed.set, changed.cleared), (4, 0));
        assert!(changed.grown.is_some());
        m.save().unwrap();
        let now = std::fs::read_to_string(&file).unwrap();
        assert!(now.contains("# The body.\n[[part]]\nname = \"body\"\nat = [-2, -1, 0]\nlayers = \"\"\"\naaaa\naaaa\n*2\n\"\"\"\n"), "{now}");
        assert!(now.contains("name = \"head\"\nat = [-1, -1, 2]\nlayers = \"\"\"\nbb\nbb\n\nbb\nbb\n\"\"\"\n"), "{now}");
    }
}

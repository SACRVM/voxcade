//! Voxcade's own model format, `.vvx` ("voxcade voxel"): a model as text.
//! Every model, figure and block of a game is one; MagicaVoxel's `.vox` is
//! only brought in (`voxcade import`, `voxcade upgrade`), so everything a
//! game holds is code or text: git shows a changed voxel, an agent or a
//! person can write a model by hand.
//!
//! ```toml
//! format = "voxcade voxel 1"   # always the first line: the format and its version
//! size = [5, 3, 3]             # the model's box: x across, y deep, z up
//!
//! [palette]                    # a sign is a colour, or a table with a material
//! a = "#7a4a2a"
//! b = { color = "#ffd9a0", glow = 1.0 }
//! g = { color = "#88ccff", glass = 0.5, rough = 0.1 }
//! m = { color = "#c0c0c0", metal = 1.0, rough = 0.3 }
//!
//! [[frame]]                    # frames, in order; or the parts of a figure
//! layers = """
//! .aaa.
//! abbba
//! .aaa.
//!
//! ..a.. *3
//! """
//!
//! [[part]]                     # a figure: its parts in order, each its own box
//! name = "arm"
//! at = [2, -1, 4]              # the box's low corner, from the pivot
//! offset = [0, 0.3, 0]         # moved by less than a voxel (-0.5 to 0.5 each),
//!                              #   only when not 0; only the picture: drawn moved
//!                              #   with its joint and what hangs on it, after its
//!                              #   rest rotation (a frame has none)
//! parent = "body"              # only when set by hand
//! joint = [2, 0, 7.5]          # only when set by hand, from the pivot
//! rotation = 45                # at rest turned about its joint, in degrees, as a
//! pitch = 10                   #   pose entry of frame:model (rotation, pitch, roll),
//!                              #   each only when not 0; only the picture
//! layers = """ ... """
//! ```
//!
//! **Where things are: from the pivot.** The pivot is the cell the game
//! stands the model on (`docs/game-model.md`, "A model's pivot"): the cell
//! `size / 2` across and deep (halves rounded down), on the floor. A
//! frame's or a part's `at` (the low corner of its box) and a part's
//! `joint` are measured from that cell's low corner, so the model's box
//! runs from `-(size / 2)` to `size - size / 2 - 1` across and deep and
//! from 0 to `size.z - 1` up. Nothing is measured from the box's corner:
//! a part that grows changes `size` (and its own `at`), never another
//! part's `at` or joint, and the model keeps standing where it stood. A
//! joint's halves are a voxel's middle (`2.5`: the middle of the voxel
//! from 2 to 3). A frame without `at` lies at the box's low corner.
//!
//! **Layers.** Bottom layer (z 0 of the box) first, an empty line between
//! layers; in a layer a line is a row, the far row (highest y) first, so a
//! layer reads as the model seen from above; x runs left to right. `.` is
//! empty. Repeats keep big plain models short: a row followed by ` *12`
//! stands for that row 12 times in all, and a line `*8` alone after a
//! layer's rows for that layer 8 times in all (neither a sign nor `.` is
//! ever `*` or a space). The writer repeats from 3 in a row up, so small
//! figures stay plain pictures.
//!
//! **Signs.** One letter or digit a cell (a-z, A-Z, 0-9: 62 entries);
//! with more entries every sign of the file is two (`..` empty). An entry
//! is a colour with its material: the same colour as glass is another
//! sign. Signs stay what they are: writing a model again keeps the signs
//! its file had (`Palette`) and only adds new ones.
//!
//! **Order.** Voxels come out in reading order: frame by frame or part by
//! part, layer by layer from the bottom, each layer's rows far to near, x
//! left to right. A part's worked-out joint can depend on that order (the
//! first of the nearest voxels, `vox::parts_of`), so a model brought in
//! from a `.vox` gets the joint (and parent) written by hand where the
//! order would move it (`convert`).
//!
//! A cell holds one voxel of a frame or a part (parts may overlap each
//! other). A file whose first line is not the format, or names a version
//! this engine does not know, is a plain mistake naming the file.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use glam::{IVec3, Vec3};
use toml_edit::{Document, Item, TableLike};

use crate::files::Files;
use crate::vox::{Hung, VoxModel, metal};

/// The file ending of a Voxcade model.
pub const EXTENSION: &str = "vvx";
/// The format and the version this engine reads and writes.
pub const FORMAT: &str = "voxcade voxel 1";
pub(crate) const FORMAT_NAME: &str = "voxcade voxel";
pub(crate) const VERSION: u32 = 1;
/// The most voxels a model's box may hold (512 by 512 by 128): the tools
/// and the editors work over the whole box. A world or a map's prefab is
/// read as a world (`crate::scene`), up to two billion.
pub const MOST_VOXELS: i64 = 1 << 25;
/// The most voxels a model may hold in all its frames or parts together:
/// each lies in the box, but a short text can fill it again and again.
pub const MOST_IN_ALL: usize = 1 << 26;
/// The most parts a figure may have: the rules weigh every part against
/// every other (`vox::parts_of`).
pub const MOST_PARTS: usize = 1024;
/// The largest a whole number of three (`Reader::ints`: a size, a place)
/// may be either way: past any world, far inside an `i32`.
pub const MOST_WHOLE: i64 = 1_000_000;

/// What a voxel is made of besides its place: its colour (alpha under 255
/// is glass), how it mirrors (`metal::pack`) and how much it glows. Two
/// voxels of the same colour and another material are two entries.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Mat {
    pub rgba: [u8; 4],
    pub metal: u8,
    pub glow: u8,
}

/// The signs of a palette (and of a level's legend), in the order they are
/// given out.
pub(crate) const SIGNS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// How many entries a palette may have for one sign a cell.
pub(crate) const ONE_SIGN: usize = SIGNS.len();
/// The writer repeats a row or a layer only this many times in a row and
/// more: fewer stay written out, a picture.
const REPEAT_FROM: usize = 3;

/// The pivot cell of a model `size` big, from the box's low corner.
pub fn pivot(size: IVec3) -> IVec3 {
    IVec3::new(size.x / 2, size.y / 2, 0)
}

/// Whether `path` is a Voxcade model by its ending.
pub fn is_model(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXTENSION)
}

/// The palette a file had: its signs and what each is, in its order. The
/// writer keeps these signs (`write`).
#[derive(Clone, Debug, Default)]
pub struct Palette {
    entries: Vec<(String, Mat)>,
}

impl Palette {
    /// The palette of a model's text; none when it does not read.
    pub fn of_text(src: &str) -> Self {
        let Ok(doc) = Document::parse(src) else { return Self::default() };
        let r = Reader { src };
        let Some(table) = doc.as_table().get("palette").and_then(Item::as_table_like) else { return Self::default() };
        let entries = table.iter().filter_map(|(sign, item)| Some((sign.to_string(), r.material(sign, item).ok()?))).collect();
        Self { entries }
    }

    /// The palette of the model file at `path`, if there is one.
    pub fn of_file(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|s| Self::of_text(&s)).unwrap_or_default()
    }

    /// A palette of these signs and what each is, in this order.
    pub fn new(entries: Vec<(String, Mat)>) -> Self {
        Self { entries }
    }

    /// Its signs and what each is, in the file's order.
    pub fn entries(&self) -> &[(String, Mat)] {
        &self.entries
    }
}

/// A model as its text says it, before the rules hang a figure's parts:
/// its box, and its frames or parts, each with its voxels from the pivot
/// and their materials (in reading order, one a cell), a part with its
/// name and the parent and joint its text sets by hand. What the model
/// editor shapes (`editor::frames`); `read` makes the engine's model of
/// it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub size: IVec3,
    /// A figure of parts (`[[part]]`), else frames.
    pub parted: bool,
    pub pieces: Vec<Piece>,
}

/// A frame or a part of a `Shape`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Piece {
    /// A part's name (empty for a frame).
    pub name: String,
    pub voxels: Vec<(IVec3, Mat)>,
    /// A part's parent (its name) and joint (from the pivot), each only
    /// when set by hand.
    pub parent: Option<String>,
    pub joint: Option<Vec3>,
    /// A part's rest rotation about its joint, in degrees: `rotation`
    /// (round, clockwise seen from above), `pitch`, `roll`, as a pose
    /// entry of `frame:model` (`vox::Part::rest`); all 0 unless set.
    pub rotation: [f32; 3],
    /// A part moved by less than a voxel from its `at` (`offset`, each
    /// within a voxel), drawn so with what hangs on it: only the picture.
    pub offset: Vec3,
}

// ---------------------------------------------------------------- writing

/// The material of voxel `k` of frame `f`.
fn mat_of(model: &VoxModel, f: usize, k: usize) -> Mat {
    let rgba = model.frames[f][k].1;
    Mat { rgba, metal: model.metal_at(f, k), glow: model.glow_at(f, k) }
}

/// The signs a text gives out: those the file had for the same material
/// first, then new ones by first use, never one the file had for another.
struct Signs {
    width: usize,
    kept: Vec<(String, Mat)>,
    /// Each material's place in `order`.
    of: HashMap<Mat, usize>,
    order: Vec<(String, Mat)>,
    next: usize,
}

impl Signs {
    fn new(width: usize, kept: &Palette) -> Self {
        let kept = kept.entries.iter().filter(|(s, _)| s.chars().count() == width).cloned().collect();
        Self { width, kept, of: HashMap::new(), order: Vec::new(), next: 0 }
    }

    /// The sign of `mat`, given out the first time it comes.
    fn sign(&mut self, mat: Mat) -> &str {
        let at = match self.of.get(&mat) {
            Some(&at) => at,
            None => {
                let s = match self.kept.iter().find(|(_, m)| *m == mat) {
                    Some((s, _)) => s.clone(),
                    None => loop {
                        let s = nth_sign(self.next, self.width);
                        self.next += 1;
                        if !self.kept.iter().any(|(k, _)| *k == s) && !self.order.iter().any(|(k, _)| *k == s) {
                            break s;
                        }
                    },
                };
                self.of.insert(mat, self.order.len());
                self.order.push((s, mat));
                self.order.len() - 1
            }
        };
        &self.order[at].0
    }

    /// The palette's lines: the kept signs used (with `all` every kept
    /// one, used or not, but a second for a material another sign has), in
    /// the file's order, then the new ones as they came.
    fn lines(&self, all: bool) -> String {
        let given = |m: &Mat| self.of.get(m).map(|&at| &self.order[at].0);
        let mut used: Vec<&(String, Mat)> = self.kept.iter().filter(|(s, m)| given(m).map_or(all && !self.kept.iter().take_while(|(k, _)| k != s).any(|(_, o)| o == m), |o| o == s)).collect();
        used.extend(self.order.iter().filter(|(s, _)| !self.kept.iter().any(|(k, _)| k == s)));
        used.iter().map(|(s, m)| format!("{s} = {}\n", material_text(m))).collect()
    }
}

/// The box of a frame's or a part's voxels (low corner and size, in the
/// model's cells) and its layers, each a list of rows far to near; the
/// first voxel in a cell stays. None without voxels.
fn layers_of(voxels: &[(IVec3, Mat)], signs: &mut Signs) -> Option<(IVec3, Vec<Vec<String>>)> {
    let low = voxels.iter().map(|v| v.0).reduce(IVec3::min)?;
    let high = voxels.iter().map(|v| v.0).reduce(IVec3::max)?;
    let dim = high - low + IVec3::ONE;
    let mut cells: Vec<Option<Mat>> = vec![None; (dim.x * dim.y * dim.z) as usize];
    for &(p, m) in voxels {
        let c = p - low;
        cells[((c.z * dim.y + c.y) * dim.x + c.x) as usize].get_or_insert(m);
    }
    let dots = ".".repeat(signs.width);
    let layers = (0..dim.z)
        .map(|z| {
            (0..dim.y)
                .rev()
                .map(|y| {
                    let mut row = String::with_capacity(dim.x as usize * signs.width);
                    for cell in &cells[((z * dim.y + y) * dim.x) as usize..][..dim.x as usize] {
                        row.push_str(match cell {
                            None => &dots,
                            Some(m) => signs.sign(*m),
                        });
                    }
                    row
                })
                .collect()
        })
        .collect();
    Some((low, layers))
}

/// Runs of the same thing: each with how many times it comes in a row.
fn runs<T: PartialEq>(items: &[T]) -> Vec<(&T, usize)> {
    let mut out: Vec<(&T, usize)> = Vec::new();
    for item in items {
        match out.last_mut() {
            Some((last, n)) if *last == item => *n += 1,
            _ => out.push((item, 1)),
        }
    }
    out
}

/// The `layers = """ ... """` of a box's layers, repeats folded.
pub(crate) fn layers_text(layers: &[Vec<String>]) -> String {
    let mut s = String::from("layers = \"\"\"\n");
    for (k, (layer, times)) in layers_runs(layers).into_iter().enumerate() {
        if k > 0 {
            s.push('\n');
        }
        for (row, n) in runs(layer) {
            if n >= REPEAT_FROM {
                s += row;
                s += &format!(" *{n}\n");
            } else {
                for _ in 0..n {
                    s += row;
                    s.push('\n');
                }
            }
        }
        if times > 1 {
            s += &format!("*{times}\n");
        }
    }
    s + "\"\"\"\n"
}

/// Layers in runs: from `REPEAT_FROM` alike once with their count, fewer
/// one by one.
fn layers_runs(layers: &[Vec<String>]) -> Vec<(&Vec<String>, usize)> {
    runs(layers).into_iter().flat_map(|(l, n)| if n >= REPEAT_FROM { vec![(l, n)] } else { vec![(l, 1); n] }).collect()
}

/// A key's text as TOML writes a string.
pub(crate) fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The shortest decimal of `v` that `ok` accepts (`0.5`, `1.0`, `0.667`).
fn short(v: f32, ok: impl Fn(f32) -> bool) -> String {
    for d in 1..=6 {
        let s = format!("{v:.d$}");
        if s.parse::<f32>().is_ok_and(&ok) {
            let t = s.trim_end_matches('0');
            return if t.ends_with('.') { format!("{t}0") } else { t.to_string() };
        }
    }
    format!("{v}")
}

/// A palette entry's text: a colour, or a table when it has a material.
fn material_text(m: &Mat) -> String {
    let [r, g, b, a] = m.rgba;
    let color = format!("#{r:02x}{g:02x}{b:02x}");
    let mut fields = Vec::new();
    if a < 255 {
        fields.push(format!("glass = {}", short(1.0 - a as f32 / 255.0, |v| alpha_of(v) == a)));
    }
    if m.metal != 0 {
        let (hi, lo) = (m.metal >> 4, m.metal & 15);
        if hi > 0 {
            fields.push(format!("metal = {}", short(hi as f32 / 15.0, |v| metal::pack(v, 0.0) >> 4 == hi)));
        }
        fields.push(format!("rough = {}", short(1.0 - lo as f32 / 15.0, |v| metal::pack(0.0, 1.0 - v) & 15 == lo)));
    }
    if m.glow != 0 {
        fields.push(format!("glow = {}", short(m.glow as f32 / 255.0, |v| glow_of(v) == m.glow)));
    }
    if fields.is_empty() { format!("\"{color}\"") } else { format!("{{ color = \"{color}\", {} }}", fields.join(", ")) }
}

/// How opaque a voxel is (0 to 255) for glass of this transparency, as
/// `vox` reads MagicaVoxel's: an opacity of 0.05 at least.
pub(crate) fn alpha_of(glass: f32) -> u8 {
    ((1.0 - glass).clamp(0.05, 1.0) * 255.0).round() as u8
}

pub(crate) fn glow_of(glow: f32) -> u8 {
    (glow.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn triple_text(v: Vec3) -> String {
    format!("[{}, {}, {}]", v.x, v.y, v.z)
}

/// A part's rest rotation's keys, in `Piece::rotation`'s order.
const ROTATION: [&str; 3] = ["rotation", "pitch", "roll"];

/// What the text says of a part's rig: the parent (its place among the
/// parts) and the joint (from the pivot), each only when set by hand.
type Rig = (Option<usize>, Option<Vec3>);

/// The rig the model's parts have by hand, as its file stated it.
fn rig_by_hand(model: &VoxModel) -> Vec<Rig> {
    model.parts.iter().map(|p| ((p.hung == Hung::ByHand).then_some(p.parent).flatten(), p.joint_by_hand.then_some(p.joint))).collect()
}

/// The text of a model, keeping the signs of `kept` (the palette the file
/// had; `Palette::default()` for a new file): frames, or the parts of a
/// figure with the parents and joints set by hand.
pub fn write(model: &VoxModel, kept: &Palette) -> String {
    write_rig(model, kept, &rig_by_hand(model))
}

fn write_rig(model: &VoxModel, kept: &Palette, rig: &[Rig]) -> String {
    write_shape(&shape_of(model, rig), kept, false)
}

/// What a model's text says of it (`Shape`), its rig as `rig` has it.
fn shape_of(model: &VoxModel, rig: &[Rig]) -> Shape {
    let pivot = pivot(model.size);
    let voxels = |f: usize, ks: &mut dyn Iterator<Item = usize>| -> Vec<(IVec3, Mat)> { ks.map(|k| (model.frames[f][k].0 - pivot, mat_of(model, f, k))).collect() };
    let pieces = if model.parts.is_empty() {
        (0..model.frames.len()).map(|f| Piece { voxels: voxels(f, &mut (0..model.frames[f].len())), ..Piece::default() }).collect()
    } else {
        model
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (parent, joint) = rig.get(i).copied().unwrap_or_default();
                Piece { name: p.name.clone(), voxels: voxels(0, &mut p.voxels.iter().map(|&k| k as usize)), parent: parent.map(|q| model.parts[q].name.clone()), joint, rotation: p.rest, offset: p.offset }
            })
            .collect()
    };
    Shape { size: model.size, parted: !model.parts.is_empty(), pieces }
}

/// The text of a `Shape`, keeping the signs of `kept`; with `all` every
/// entry of `kept` stays in the palette, used or not (the model editor's:
/// a colour no voxel has any more keeps its sign until it is taken out).
pub fn write_shape(shape: &Shape, kept: &Palette, all: bool) -> String {
    let mut mats = std::collections::HashSet::new();
    for piece in &shape.pieces {
        mats.extend(piece.voxels.iter().map(|v| v.1));
    }
    if all {
        mats.extend(kept.entries.iter().map(|e| e.1));
    }
    let mut signs = Signs::new(if mats.len() <= ONE_SIGN { 1 } else { 2 }, kept);
    let box_low = -pivot(shape.size);
    let mut body = String::new();
    for piece in &shape.pieces {
        let layers = layers_of(&piece.voxels, &mut signs);
        if shape.parted {
            body += &format!("\n[[part]]\nname = {}\n", quoted(&piece.name));
            if let Some((low, _)) = &layers {
                body += &format!("at = {}\n", triple_text(low.as_vec3()));
            }
            // Moved by less than a voxel, only when it is.
            if piece.offset != Vec3::ZERO {
                body += &format!("offset = {}\n", triple_text(piece.offset));
            }
            if let Some(q) = &piece.parent {
                body += &format!("parent = {}\n", quoted(q));
            }
            if let Some(j) = piece.joint {
                body += &format!("joint = {}\n", triple_text(j));
            }
            // Its rest rotation, each only when not 0.
            for (key, degrees) in ROTATION.iter().zip(piece.rotation) {
                if degrees != 0.0 {
                    body += &format!("{key} = {degrees}\n");
                }
            }
        } else {
            body += "\n[[frame]]\n";
            if let Some((low, _)) = layers.as_ref().filter(|(low, _)| *low != box_low) {
                body += &format!("at = {}\n", triple_text(low.as_vec3()));
            }
        }
        match layers {
            Some((_, layers)) => body += &layers_text(&layers),
            None => body += "layers = \"\"\n",
        }
    }
    let s = shape.size;
    format!("format = \"{FORMAT}\"\nsize = [{}, {}, {}]\n\n[palette]\n{}{body}", s.x, s.y, s.z, signs.lines(all))
}

// ---------------------------------------------------------------- reading

/// The line of a byte of the source (1 on).
pub(crate) fn line_of(src: &str, at: usize) -> usize {
    src[..at.min(src.len())].matches('\n').count() + 1
}

/// Reads the parts of a text file of TOML, every mistake said with the
/// line it is on (models, and levels and maps, `grid` and `map`).
pub(crate) struct Reader<'a> {
    pub src: &'a str,
}

type Span = Option<std::ops::Range<usize>>;

/// A text of TOML as a document, its mistake said with its line.
pub(crate) fn parse(src: &str) -> Result<Document<&str>> {
    Document::parse(src).map_err(|e| match e.span() {
        Some(s) => anyhow!("line {}: {}", line_of(src, s.start), e.message()),
        None => anyhow!("{}", e.message()),
    })
}

/// `#rrggbb` as three bytes.
pub(crate) fn hex(text: &str) -> Option<[u8; 3]> {
    let h = text.strip_prefix('#').filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))?;
    Some([0, 2, 4].map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex digits")))
}

/// The `i`-th sign given out (`SIGNS`), one character or two.
pub(crate) fn nth_sign(i: usize, width: usize) -> String {
    if width == 1 { (SIGNS[i] as char).to_string() } else { format!("{}{}", SIGNS[i / ONE_SIGN] as char, SIGNS[i % ONE_SIGN] as char) }
}

/// The signs of a palette or a legend by their bytes, quick to look up: a
/// table of one byte, or of two.
pub(crate) struct SignTable<T> {
    width: usize,
    one: [Option<T>; 256],
    two: HashMap<[u8; 2], T>,
}

impl<T: Copy> SignTable<T> {
    /// The table of `signs`, each `width` characters (checked, `Reader::sign`).
    pub fn new<'s>(width: usize, signs: impl IntoIterator<Item = (&'s str, T)>) -> Self {
        let mut table = Self { width, one: [None; 256], two: HashMap::new() };
        for (sign, value) in signs {
            match *sign.as_bytes() {
                [b] => table.one[b as usize] = Some(value),
                [a, b] => _ = table.two.insert([a, b], value),
                _ => {}
            }
        }
        table
    }

    /// What a cell's sign (`width` bytes) stands for.
    pub fn get(&self, cell: &[u8]) -> Option<T> {
        if self.width == 1 { self.one[cell[0] as usize] } else { self.two.get(&[cell[0], cell[1]]).copied() }
    }
}

/// Signs as a mistake lists them: sorted, comma by comma.
pub(crate) fn sorted_signs<'s>(signs: impl Iterator<Item = &'s String>) -> String {
    let mut signs: Vec<&String> = signs.collect();
    signs.sort();
    signs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
}

/// One layer of a `layers` text as read: its rows (the line each is on
/// within the text), how many times it stands.
pub(crate) struct Layer<'a> {
    pub rows: Vec<(usize, &'a str)>,
    pub times: usize,
}

/// How many rows a layer and how many layers in all a `layers` text may
/// have: the box's depth and height (a grid's, a level's). A repeat is
/// counted before it is made, so no count in a file makes more than this.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Room {
    pub rows: usize,
    pub layers: usize,
}

impl Room {
    /// The room of a box `size` voxels big: as deep as its y, as high as
    /// its z.
    pub fn of(size: IVec3) -> Self {
        Self { rows: size.y.max(0) as usize, layers: size.z.max(0) as usize }
    }
}

/// The layers of a `layers` text: an empty line between layers, a row
/// followed by ` *N` that row N times in all, a line `*N` after a layer's
/// rows that layer N times in all. `line(i)` says where the text's line `i`
/// is in the file (`"line 12: "`). More rows in a layer, or more layers,
/// than `room` holds is a mistake, said before they are made.
pub(crate) fn layer_list<'a>(text: &'a str, line: &dyn Fn(usize) -> String, room: Room) -> Result<Vec<Layer<'a>>> {
    let count = |word: &str, i: usize| -> Result<usize> {
        let n = word.strip_prefix('*').and_then(|n| n.parse::<usize>().ok()).filter(|&n| n >= 2);
        n.ok_or_else(|| anyhow!("{}'{word}' is no repeat: a star and how many times in all, 2 or more (*3)", line(i)))
    };
    let too_many_layers = |i: usize, n: usize| anyhow!("{}that makes {n} layers, more than there is room for: {} at most", line(i), room.layers);
    let mut layers: Vec<Layer> = Vec::new();
    // The layers so far, each as often as it stands.
    let mut total = 0usize;
    let mut open = false;
    for (i, raw) in text.lines().enumerate() {
        let words: Vec<&str> = raw.split_whitespace().collect();
        match words[..] {
            [] => open = false,
            [word] if word.starts_with('*') => {
                let layer = layers.last_mut().filter(|l| open && l.times == 1).ok_or_else(|| anyhow!("{}a layer's repeat ({word}) comes on the line after its rows", line(i)))?;
                let times = count(word, i)?;
                total = (total - 1).saturating_add(times);
                if total > room.layers {
                    return Err(too_many_layers(i, total));
                }
                layer.times = times;
                open = false;
            }
            [row, ref rest @ ..] => {
                let times = match rest {
                    [] => 1,
                    [word] => count(word, i)?,
                    _ => bail!("{}a row is its signs, then perhaps a repeat (*3); this line has more", line(i)),
                };
                if !open {
                    total += 1;
                    if total > room.layers {
                        return Err(too_many_layers(i, total));
                    }
                    layers.push(Layer { rows: Vec::new(), times: 1 });
                    open = true;
                }
                let layer = layers.last_mut().expect("a layer is open");
                let rows = layer.rows.len().saturating_add(times);
                if rows > room.rows {
                    bail!("{}that makes {rows} rows in this layer, more than there is room for: {} at most", line(i), room.rows);
                }
                layer.rows.extend(std::iter::repeat_n((i, row), times));
            }
        }
    }
    Ok(layers)
}

/// Where a `layers` text lies and how its cells read: the signs `width`
/// characters a cell, the frame's low corner at `low` in a box `size` big
/// whose pivot is `pivot` (for the mistakes, which say places from it),
/// `what` the box in a mistake ("the model's size").
pub(crate) struct Lay<'a> {
    pub width: usize,
    pub low: IVec3,
    pub size: IVec3,
    pub pivot: IVec3,
    pub what: &'a str,
}

/// The one reading of a `layers` text into a box, a model's frame or
/// part's and a world's or a prefab's alike (`crate::scene`): every cell
/// that is not empty (`.`, or `..` with two signs a cell) in reading order
/// (layer by layer from the bottom, each layer's rows far to near, x left to
/// right), its value by `sign` (None: no sign of the palette, `known` the
/// signs there are), handed to `put` with its place in the box. Rows as long
/// as one another, layers as deep, every voxel inside the box; nothing past
/// the box's room is made (`layer_list`).
pub(crate) fn read_cells<T>(text: &str, line: &dyn Fn(usize) -> String, lay: &Lay, sign: impl Fn(&[u8]) -> Option<T>, known: impl Fn() -> String, mut put: impl FnMut(IVec3, T)) -> Result<()> {
    let (width, size) = (lay.width, lay.size);
    let layers = layer_list(text, line, Room::of(size))?;
    let (mut cols, mut depth) = (None, None);
    let mut z = lay.low.z;
    for layer in &layers {
        let rows = layer.rows.len();
        if *depth.get_or_insert(rows) != rows {
            bail!("{}this layer has {rows} rows, the first has {}: every layer must be as deep as the first", line(layer.rows[0].0), depth.unwrap_or_default());
        }
        for _ in 0..layer.times {
            for (r, &(i, row)) in layer.rows.iter().enumerate() {
                let bytes = row.as_bytes();
                if !row.is_ascii() {
                    let odd = row.chars().find(|c| !c.is_ascii()).unwrap_or('?');
                    bail!("{}'{odd}' is no sign of the palette (it has {})", line(i), known());
                }
                if !bytes.len().is_multiple_of(width) {
                    bail!("{}a row is {} characters, but every cell is {width} (the palette's signs are that long)", line(i), bytes.len());
                }
                let n = bytes.len() / width;
                if *cols.get_or_insert(n) != n {
                    bail!("{}this row has {n} cells, the first has {}: every row must be as long as the first", line(i), cols.unwrap_or_default());
                }
                let y = lay.low.y + (rows - 1 - r) as i32;
                for (x, cell) in bytes.chunks(width).enumerate() {
                    if cell.iter().all(|&b| b == b'.') {
                        continue;
                    }
                    let v = sign(cell).ok_or_else(|| anyhow!("{}'{}' is no sign of the palette (it has {})", line(i), String::from_utf8_lossy(cell), known()))?;
                    let p = IVec3::new(lay.low.x + x as i32, y, z);
                    if p.cmplt(IVec3::ZERO).any() || p.cmpge(size).any() {
                        let f = p - lay.pivot;
                        bail!("{}a voxel at [{}, {}, {}] (from the pivot) lies outside {} [{}, {}, {}]: make size bigger or move it in", line(i), f.x, f.y, f.z, lay.what, size.x, size.y, size.z);
                    }
                    put(p, v);
                }
            }
            z += 1;
        }
    }
    Ok(())
}

impl Reader<'_> {
    /// The line in the file of the first line of a multi-line text (a
    /// newline straight after its opening marks is not part of it); 0
    /// without a place in the file.
    pub fn first_line(&self, item: &Item) -> usize {
        item.span().map_or(0, |s| {
            let raw = &self.src[s.clone()];
            let after = raw.strip_prefix("\"\"\"").or_else(|| raw.strip_prefix("'''"));
            line_of(self.src, s.start) + usize::from(after.is_some_and(|a| a.starts_with('\n') || a.starts_with("\r\n")))
        })
    }

    /// A mistake on the line where `span` begins.
    pub fn fail(&self, span: Span, why: impl std::fmt::Display) -> anyhow::Error {
        match span {
            Some(s) => anyhow!("line {}: {why}", line_of(self.src, s.start)),
            None => anyhow!("{why}"),
        }
    }

    /// The file's `size`, three whole numbers: `missing` says what it is
    /// where there is none, `small` the mistake where one is under 1.
    pub fn size<'t>(&self, top: &'t toml_edit::Table, missing: &str, small: &str) -> Result<(IVec3, &'t Item)> {
        let item = top.get("size").ok_or_else(|| anyhow!("no size = [x, y, z] ({missing})"))?;
        let size = self.ints(item, "size")?;
        if size.cmplt(IVec3::ONE).any() {
            return Err(self.fail(item.span(), small));
        }
        Ok((size, item))
    }

    /// A palette's or a legend's sign checked: one or two letters or
    /// digits, as long as the signs before it (`width`, 0 before the
    /// first; then the sign's).
    pub fn sign(&self, sign: &str, item: &Item, width: &mut usize) -> Result<()> {
        let n = sign.chars().count();
        if !(1..=2).contains(&n) || sign.chars().any(|c| !c.is_ascii_alphanumeric()) {
            return Err(self.fail(item.span(), format!("'{sign}' cannot be a sign: one or two letters or digits")));
        }
        if *width != 0 && *width != n {
            return Err(self.fail(item.span(), format!("'{sign}' has {n} characters, the signs before have {width}: all signs of a file are as long as one another")));
        }
        *width = n;
        Ok(())
    }

    pub fn number(&self, item: &Item, what: &str) -> Result<f32> {
        // (Finite as an f32 too: 1e300 is no number a game can hold.)
        let v = item.as_float().or_else(|| item.as_integer().map(|i| i as f64)).filter(|v| v.is_finite()).ok_or_else(|| self.fail(item.span(), format!("{what} must be a number")))?;
        if v.abs() > f32::MAX as f64 {
            return Err(self.fail(item.span(), format!("{what} = {v} is too large a number")));
        }
        Ok(v as f32)
    }

    /// A whole number in `range`, else a mistake saying which: not whole,
    /// or past the range (named). Read exactly, never through a float.
    pub fn whole(&self, item: &Item, what: &str, range: std::ops::RangeInclusive<i64>) -> Result<i64> {
        let v = item.as_integer().ok_or_else(|| self.fail(item.span(), format!("{what} must be a whole number")))?;
        if !range.contains(&v) {
            return Err(self.fail(item.span(), format!("{what} = {v}: it must be from {} to {}", range.start(), range.end())));
        }
        Ok(v)
    }

    pub fn fraction(&self, item: &Item, what: &str) -> Result<f32> {
        let v = self.number(item, what)?;
        if !(0.0..=1.0).contains(&v) {
            return Err(self.fail(item.span(), format!("{what} must be between 0 and 1, not {v}")));
        }
        Ok(v)
    }

    pub fn triple(&self, item: &Item, what: &str) -> Result<[f32; 3]> {
        let wrong = || self.fail(item.span(), format!("{what} must be three numbers, like [1, 2, 3]"));
        let a = item.as_array().filter(|a| a.len() == 3).ok_or_else(wrong)?;
        let mut out = [0.0; 3];
        for (o, v) in out.iter_mut().zip(a.iter()) {
            *o = v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).filter(|v| v.is_finite()).ok_or_else(wrong)? as f32;
        }
        Ok(out)
    }

    /// Three whole numbers, each at most `MOST_WHOLE` either way (a size,
    /// a place): a number past that is told as past the limit, not as no
    /// whole number. Whole floats (`2.0`) count as whole, as before.
    pub fn ints(&self, item: &Item, what: &str) -> Result<IVec3> {
        let wrong = || self.fail(item.span(), format!("{what} must be three whole numbers, like [1, 2, 3]"));
        let a = item.as_array().filter(|a| a.len() == 3).ok_or_else(wrong)?;
        let mut out = [0i32; 3];
        for (o, v) in out.iter_mut().zip(a.iter()) {
            let n = match (v.as_integer(), v.as_float()) {
                (Some(i), _) => i as f64,
                (None, Some(f)) if f.is_finite() && f.fract() == 0.0 => f,
                _ => return Err(wrong()),
            };
            if n.abs() > MOST_WHOLE as f64 {
                let shown = v.as_integer().map_or_else(|| n.to_string(), |i| i.to_string());
                return Err(self.fail(item.span(), format!("{what}: {shown} is past the limit, {MOST_WHOLE} at most either way")));
            }
            *o = n as i32;
        }
        Ok(IVec3::from_array(out))
    }

    /// A field of `table` under a name it had before Voxcade took other
    /// tools' words (`renamed`: each old name and its new one) is a
    /// mistake naming the new one and the command that renames it.
    pub fn renamed(&self, table: &dyn TableLike, renamed: &[(&str, &str)], what: &str) -> Result<()> {
        for (old, new) in renamed {
            if let Some((_, v)) = table.iter().find(|(k, _)| k == old) {
                return Err(self.fail(v.span(), format!("{what}: '{old}' is called '{new}' now (voxcade upgrade renames it)")));
            }
        }
        Ok(())
    }

    /// Only the fields `allowed` may be in `table`, `what` named in the
    /// mistake.
    pub fn only(&self, table: &dyn TableLike, allowed: &[&str], what: &str) -> Result<()> {
        for (k, v) in table.iter() {
            if !allowed.contains(&k) {
                return Err(self.fail(v.span(), format!("'{k}' is no field of {what} (it has {})", allowed.join(", "))));
            }
        }
        Ok(())
    }

    pub fn material(&self, sign: &str, item: &Item) -> Result<Mat> {
        let hex = |text: &str, at: &Item| hex(text).ok_or_else(|| self.fail(at.span(), format!("'{text}' is no color: write it as #rrggbb")));
        if let Some(text) = item.as_str() {
            let [r, g, b] = hex(text, item)?;
            return Ok(Mat { rgba: [r, g, b, 255], metal: 0, glow: 0 });
        }
        let table = item.as_table_like().ok_or_else(|| self.fail(item.span(), format!("the palette's '{sign}' must be a color like \"#7a4a2a\" or a table like {{ color = \"#7a4a2a\", glow = 1.0 }}")))?;
        self.only(table, &["color", "glass", "metal", "rough", "glow"], &format!("the palette's '{sign}'"))?;
        let colour = table.get("color").ok_or_else(|| self.fail(item.span(), format!("the palette's '{sign}' has no color")))?;
        let [r, g, b] = hex(colour.as_str().ok_or_else(|| self.fail(colour.span(), "color must be text like \"#7a4a2a\""))?, colour)?;
        let alpha = match table.get("glass") {
            Some(g) => alpha_of(self.fraction(g, "glass")?),
            None => 255,
        };
        let (m, rough) = (table.get("metal"), table.get("rough"));
        let packed = if m.is_none() && rough.is_none() {
            0
        } else {
            let metalness = m.map_or(Ok(0.0), |m| self.fraction(m, "metal"))?;
            let rough = rough.map_or(Ok(0.3), |r| self.fraction(r, "rough"))?;
            metal::pack(metalness, 1.0 - rough)
        };
        let glow = table.get("glow").map_or(Ok(0), |g| self.fraction(g, "glow").map(glow_of))?;
        Ok(Mat { rgba: [r, g, b, alpha], metal: packed, glow })
    }

    /// The voxels of one `layers` text in reading order, in the model's
    /// cells (`at` its box's low corner from the pivot, `pivot` where that
    /// lies in the model's box, `size` the box). Rows must be as long as
    /// one another, layers as deep; every voxel inside the box.
    fn layers(&self, item: &Item, palette: &HashMap<String, Mat>, width: usize, at: IVec3, pivot: IVec3, size: IVec3) -> Result<Vec<(IVec3, Mat)>> {
        let text = item.as_str().ok_or_else(|| self.fail(item.span(), "layers must be a text between \"\"\" marks"))?;
        let first = self.first_line(item);
        let line = |i: usize| if first > 0 { format!("line {}: ", first + i) } else { String::new() };
        let known = || sorted_signs(palette.keys());
        let table = SignTable::new(width, palette.iter().map(|(s, m)| (s.as_str(), *m)));
        let sign = |s: &[u8]| table.get(s);
        let lay = Lay { width, low: at + pivot, size, pivot, what: "the model's size" };
        let mut out = Vec::new();
        read_cells(text, &line, &lay, sign, known, |p, mat| out.push((p, mat)))?;
        Ok(out)
    }
}

/// The tables of an array of tables (`[[x]]`, or `[{...}, {...}]`).
pub(crate) fn tables(item: &Item) -> Option<Vec<&dyn TableLike>> {
    if let Some(a) = item.as_array_of_tables() {
        return Some(a.iter().map(|t| t as &dyn TableLike).collect());
    }
    item.as_array()?.iter().map(|v| v.as_inline_table().map(|t| t as &dyn TableLike)).collect()
}

/// The model in the file at `path` of the game's files, its mistakes
/// naming the file.
pub fn load_in(files: &Files, path: &Path) -> Result<VoxModel> {
    let bytes = files.read(path).with_context(|| format!("reading {}", path.display()))?;
    let src = String::from_utf8(bytes).map_err(|_| anyhow!("{}: not a Voxcade model (.vvx is text; a MagicaVoxel file is brought in with voxcade import)", path.display()))?;
    read(&src).map_err(|e| anyhow!("{}: {e}", path.display()))
}

/// Checks the format line: the first key of the file, this format, a
/// version this engine knows.
fn check_format(r: &Reader, top: &toml_edit::Table) -> Result<()> {
    check_format_of(r, top, FORMAT_NAME, VERSION, "model")
}

/// Checks that a file of Voxcade's own (`what`: "model", "level", "map")
/// begins with its format line, `format = "NAME VERSION"`, a version this
/// engine reads.
pub(crate) fn check_format_of(r: &Reader, top: &toml_edit::Table, name: &str, version: u32, what: &str) -> Result<()> {
    let first = top.iter().next();
    let wanted = format!("its first line must be format = \"{name} {version}\"");
    let Some(("format", item)) = first else { bail!("not a Voxcade {what}: {wanted}") };
    let text = item.as_str().ok_or_else(|| r.fail(item.span(), format!("format must be text: {wanted}")))?;
    let found = text.strip_prefix(name).map(str::trim).and_then(|v| v.parse::<u32>().ok());
    match found {
        Some(v) if v == version => Ok(()),
        Some(v) if v > version => Err(r.fail(item.span(), format!("a {what} of version {v} of the format, from a newer Voxcade; this one reads version {version}: get the new version"))),
        _ => Err(r.fail(item.span(), format!("'{text}' is no format this Voxcade reads: {wanted}"))),
    }
}

/// The model the text says (`read` without a file: mistakes say only the
/// line), as the engine draws it: its parts' parents and joints worked out
/// but for those the text sets by hand.
pub fn read(src: &str) -> Result<VoxModel> {
    Ok(model_of(&read_shape(src)?.0))
}

/// The engine's model of a `Shape` (checked as `read_shape` checks it).
pub fn model_of(shape: &Shape) -> VoxModel {
    let (size, pivot) = (shape.size, pivot(shape.size));
    let mut model = if !shape.parted {
        let frames = shape.pieces.iter().map(|p| p.voxels.iter().map(|(v, m)| (*v + pivot, m.rgba)).collect()).collect();
        let metals = shape.pieces.iter().map(|p| p.voxels.iter().map(|(_, m)| m.metal).collect()).collect();
        let mut model = VoxModel::new(size, frames, metals);
        if shape.pieces.iter().flat_map(|p| &p.voxels).any(|(_, m)| m.glow != 0) {
            model.glow = shape.pieces.iter().map(|p| p.voxels.iter().map(|(_, m)| m.glow).collect()).collect();
        }
        model
    } else {
        let names: Vec<String> = shape.pieces.iter().map(|p| p.name.clone()).collect();
        let (mut owner, mut frame, mut metals, mut glows) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (i, piece) in shape.pieces.iter().enumerate() {
            for (v, m) in &piece.voxels {
                owner.push(i);
                frame.push((*v + pivot, m.rgba));
                metals.push(m.metal);
                glows.push(m.glow);
            }
        }
        // The rig set by hand, its joints from the box's low corner as the
        // rules work.
        let stated: Vec<(Option<usize>, Option<Vec3>)> = shape.pieces.iter().map(|p| (p.parent.as_ref().and_then(|q| names.iter().position(|n| n == q)), p.joint.map(|j| j + pivot.as_vec3()))).collect();
        let mut model = VoxModel::new(size, vec![frame.clone()], vec![metals]);
        if glows.iter().any(|&g| g != 0) {
            model.glow = vec![glows];
        }
        model.parts = crate::vox::parts_of(&frame, &owner, names, size, &stated);
        for (part, piece) in model.parts.iter_mut().zip(&shape.pieces) {
            part.rest = piece.rotation;
            part.offset = piece.offset;
        }
        model
    };
    model.size = size;
    model
}

/// What the text says (`Shape`) and its palette, every mistake said with
/// its line: the model editor's reading, the engine's (`read`) before the
/// rules hang the parts.
pub fn read_shape(src: &str) -> Result<(Shape, Palette)> {
    let doc = parse(src)?;
    let r = Reader { src };
    let top = doc.as_table();
    check_format(&r, top)?;
    r.only(top, &["format", "size", "palette", "frame", "part"], "a model")?;
    let (size, size_item) = r.size(top, "the model's box, in voxels", "size must be at least one voxel each way")?;
    let volume = size.as_i64vec3().element_product();
    if volume > MOST_VOXELS {
        return Err(r.fail(size_item.span(), format!("the model's box is too big: {} by {} by {} is {volume} voxels, a model holds {MOST_VOXELS} at most (512 by 512 by 128)", size.x, size.y, size.z)));
    }
    let pivot = pivot(size);
    let palette_item = top.get("palette").ok_or_else(|| anyhow!("no [palette]: the signs and their colors"))?;
    let table = palette_item.as_table_like().ok_or_else(|| r.fail(palette_item.span(), "palette must be a table of signs"))?;
    let (mut palette, mut entries) = (HashMap::new(), Vec::new());
    let mut width = 0;
    for (sign, item) in table.iter() {
        r.sign(sign, item, &mut width)?;
        if item.as_table_like().is_some_and(|t| t.contains_key("marker")) {
            return Err(r.fail(item.span(), format!("the palette's '{sign}' is a marker: markers place things in a world or a map's prefab, a model has none")));
        }
        let mat = r.material(sign, item)?;
        palette.insert(sign.to_string(), mat);
        entries.push((sign.to_string(), mat));
    }
    let width = width.max(1);
    let box_low = -pivot;
    let from_pivot = |voxels: Vec<(IVec3, Mat)>| voxels.into_iter().map(|(v, m)| (v - pivot, m)).collect::<Vec<_>>();
    // The voxels of the frames or parts so far, within `MOST_IN_ALL`.
    let mut held = 0usize;
    let in_all = |held: &mut usize, more: usize, span: Span| -> Result<()> {
        *held += more;
        if *held > MOST_IN_ALL {
            return Err(r.fail(span, format!("the model holds more than {MOST_IN_ALL} voxels in all its frames or parts")));
        }
        Ok(())
    };
    let shape = match (top.get("frame"), top.get("part")) {
        (Some(f), None) => {
            let list = tables(f).ok_or_else(|| r.fail(f.span(), "frames must be tables: [[frame]]"))?;
            let mut pieces = Vec::new();
            for t in list {
                if let Some((key, v)) = t.iter().find(|(k, _)| ROTATION.contains(k) || *k == "turn") {
                    return Err(r.fail(v.span(), format!("a frame does not rotate ('{key}'): a figure's part does, [[part]] with rotation, pitch and roll")));
                }
                if let Some(v) = t.get("offset") {
                    return Err(r.fail(v.span(), "a frame has no offset: a figure's part does, [[part]] with offset = [x, y, z]"));
                }
                r.only(t, &["at", "layers"], "a frame")?;
                let at = t.get("at").map_or(Ok(box_low), |a| r.ints(a, "at"))?;
                let voxels = match t.get("layers") {
                    Some(l) => r.layers(l, &palette, width, at, pivot, size)?,
                    None => Vec::new(),
                };
                in_all(&mut held, voxels.len(), t.get("layers").and_then(Item::span))?;
                pieces.push(Piece { voxels: from_pivot(voxels), ..Piece::default() });
            }
            if pieces.is_empty() {
                bail!("the model has no frame: [[frame]] with its layers");
            }
            Shape { size, parted: false, pieces }
        }
        (None, Some(p)) => {
            let list = tables(p).ok_or_else(|| r.fail(p.span(), "parts must be tables: [[part]] with a name"))?;
            let mut pieces: Vec<Piece> = Vec::new();
            let mut spans = Vec::new();
            if list.len() > MOST_PARTS {
                return Err(r.fail(p.span(), format!("the figure has {} parts, {MOST_PARTS} at most", list.len())));
            }
            for t in list {
                if let Some(v) = t.get("turn") {
                    return Err(r.fail(v.span(), "a part's 'turn' is called 'rotation' now (degrees round, clockwise seen from above)"));
                }
                r.only(t, &["name", "at", "offset", "parent", "joint", "rotation", "pitch", "roll", "layers"], "a part")?;
                let name_item = t.get("name").ok_or_else(|| anyhow!("a part has no name (name = \"arm\") before line {}", t.get("layers").and_then(Item::span).map_or(0, |s| line_of(src, s.start))))?;
                let name = name_item.as_str().filter(|n| !n.trim().is_empty()).ok_or_else(|| r.fail(name_item.span(), "a part's name must be text, not empty"))?.to_string();
                if pieces.iter().any(|p| p.name == name) {
                    return Err(r.fail(name_item.span(), format!("two parts are called '{name}': each part needs a name of its own")));
                }
                let at = t.get("at").map_or(Ok(box_low), |a| r.ints(a, "at"))?;
                let voxels = match t.get("layers") {
                    Some(l) => r.layers(l, &palette, width, at, pivot, size)?,
                    None => Vec::new(),
                };
                in_all(&mut held, voxels.len(), t.get("layers").and_then(Item::span))?;
                let parent = match t.get("parent") {
                    None => None,
                    Some(i) => {
                        spans.push((pieces.len(), i.span()));
                        Some(i.as_str().ok_or_else(|| r.fail(i.span(), "parent must be the name of a part"))?.to_string())
                    }
                };
                let joint = t.get("joint").map(|i| r.triple(i, "joint").map(Vec3::from)).transpose()?;
                let mut rotation = [0.0; 3];
                for (r_, key) in rotation.iter_mut().zip(ROTATION) {
                    if let Some(i) = t.get(key) {
                        *r_ = r.number(i, key)?;
                    }
                }
                let offset = match t.get("offset") {
                    None => Vec3::ZERO,
                    Some(i) => {
                        let o = Vec3::from(r.triple(i, "offset")?);
                        if o.abs().max_element() > 0.5 + 1e-4 {
                            return Err(r.fail(i.span(), "an offset is less than a voxel each way (-0.5 to 0.5): whole voxels go into 'at'"));
                        }
                        o
                    }
                };
                pieces.push(Piece { name, voxels: from_pivot(voxels), parent, joint, rotation, offset });
            }
            if pieces.is_empty() {
                bail!("the model has no part: [[part]] with a name and its layers");
            }
            let names: Vec<&str> = pieces.iter().map(|p| p.name.as_str()).collect();
            let mut parents = Vec::new();
            for (i, piece) in pieces.iter().enumerate() {
                parents.push(match &piece.parent {
                    None => None,
                    Some(q) => Some(names.iter().position(|n| n == q).ok_or_else(|| {
                        let span = spans.iter().find(|(k, _)| *k == i).and_then(|(_, s)| s.clone());
                        r.fail(span, format!("no part is called '{q}' (the parts are {})", names.join(", ")))
                    })?),
                });
            }
            if let Some(why) = crate::vox::ring_mistake(&parents, &names, "parent") {
                bail!(why);
            }
            Shape { size, parted: true, pieces }
        }
        (Some(f), Some(_)) => return Err(r.fail(f.span(), "a model is frames ([[frame]]) or the parts of a figure ([[part]]), not both")),
        (None, None) => bail!("the model has no [[frame]] and no [[part]]"),
    };
    Ok((shape, Palette { entries }))
}

// ------------------------------------------------------------- comparing

type Bag = Vec<(i32, i32, i32, [u8; 4], u8, u8)>;

/// The voxels `ks` of frame `f` as a sorted list (place from the pivot,
/// colour, metal, glow), the first in a cell only: two models hold the
/// same when these are equal.
fn bag(model: &VoxModel, f: usize, ks: impl Iterator<Item = usize>) -> Bag {
    let pivot = pivot(model.size);
    let mut seen = crate::world::CellSet::default();
    let mut v: Bag = ks
        .filter(|&k| seen.insert(model.frames[f][k].0))
        .map(|k| {
            let ((p, c), m) = (model.frames[f][k], mat_of(model, f, k));
            let p = p - pivot;
            (p.x, p.y, p.z, c, m.metal, m.glow)
        })
        .collect();
    v.sort_unstable();
    v
}

/// What two sorted lists do not share: how many only in the first, only in
/// the second, and one example of each.
fn bag_diff(a: &Bag, b: &Bag) -> Option<String> {
    if a == b {
        return None;
    }
    let (mut i, mut j) = (0, 0);
    let (mut only_a, mut only_b) = (Vec::new(), Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            (i, j) = (i + 1, j + 1);
        } else if j >= b.len() || (i < a.len() && a[i] < b[j]) {
            only_a.push(&a[i]);
            i += 1;
        } else {
            only_b.push(&b[j]);
            j += 1;
        }
    }
    Some(format!("{} only in the first{}, {} only in the second{}", only_a.len(), only_a.first().map_or(String::new(), |v| format!(" (e.g. {v:?})")), only_b.len(), only_b.first().map_or(String::new(), |v| format!(" (e.g. {v:?})"))))
}

/// Every way two models differ in what the game draws and moves (sizes,
/// each frame's or part's voxels from the pivot with their colours and
/// materials, the
/// parts' names, parents, joints and overlaps); empty: the same. Whether a
/// rig was set by hand or worked out is the file's way of saying it, not a
/// difference. A voxel twice in one cell counts once (the first).
pub fn differences(a: &VoxModel, b: &VoxModel) -> Vec<String> {
    let mut out = Vec::new();
    if a.size != b.size {
        out.push(format!("size {} vs {}", a.size, b.size));
    }
    if a.frames.len() != b.frames.len() {
        out.push(format!("{} frames vs {}", a.frames.len(), b.frames.len()));
    }
    if a.parts.len() != b.parts.len() {
        out.push(format!("{} parts vs {}", a.parts.len(), b.parts.len()));
    }
    if a.parts.is_empty() {
        for f in 0..a.frames.len().min(b.frames.len()) {
            if let Some(d) = bag_diff(&bag(a, f, 0..a.frames[f].len()), &bag(b, f, 0..b.frames[f].len())) {
                out.push(format!("frame {f}: {d}"));
            }
        }
    }
    let name = |m: &VoxModel, p: Option<usize>| p.map(|q| m.parts[q].name.clone());
    for (i, (p, q)) in a.parts.iter().zip(&b.parts).enumerate() {
        let label = format!("part {i} '{}'", p.name);
        if p.name != q.name {
            out.push(format!("{label}: called '{}' in the second", q.name));
            continue;
        }
        if let Some(d) = bag_diff(&bag(a, 0, p.voxels.iter().map(|&k| k as usize)), &bag(b, 0, q.voxels.iter().map(|&k| k as usize))) {
            out.push(format!("{label}: voxels: {d}"));
        }
        if name(a, p.parent) != name(b, q.parent) {
            out.push(format!("{label}: hangs on {:?}, then on {:?}", name(a, p.parent), name(b, q.parent)));
        }
        if p.joint != q.joint {
            out.push(format!("{label}: joint {} then {}", p.joint, q.joint));
        }
        if p.rest != q.rest {
            out.push(format!("{label}: rotated at rest {:?} then {:?}", p.rest, q.rest));
        }
        if p.overlaps != q.overlaps {
            out.push(format!("{label}: overlaps {:?} vs {:?}", p.overlaps, q.overlaps));
        }
    }
    out
}

/// How many voxels lie in a cell another voxel of the same frame (or part)
/// already fills: the text keeps the first, the rest are dropped.
fn doubled(model: &VoxModel) -> usize {
    let count = |f: usize, ks: &mut dyn Iterator<Item = usize>| {
        let mut seen = crate::world::CellSet::default();
        ks.filter(|&k| !seen.insert(model.frames[f][k].0)).count()
    };
    if model.parts.is_empty() {
        (0..model.frames.len()).map(|f| count(f, &mut (0..model.frames[f].len()))).sum()
    } else {
        model.parts.iter().map(|p| count(0, &mut p.voxels.iter().map(|&k| k as usize))).sum()
    }
}

/// A model brought in as text, and what it took.
pub struct Converted {
    pub text: String,
    /// The model the text holds: the one given, or with its box grown.
    pub model: VoxModel,
    /// The box made bigger (from, to) to hold voxels the other format kept
    /// outside it, every voxel and joint where it was from the pivot: the
    /// model stands and looks as before.
    pub grown: Option<(IVec3, IVec3)>,
    /// Parts given a joint by hand, and a parent, so that they hang as
    /// before (the text's voxel order would have moved them).
    pub joints: usize,
    pub parents: usize,
    /// Voxels dropped: a second one in a cell of the same frame or part.
    pub dropped: usize,
}

/// The text of a model read from another format (a `.vox`, as the engine
/// made it of that), keeping the signs of `kept`: every part hangs where
/// it hung, its joint and parent written by hand where the text's order
/// would work them out otherwise; checked to read back the same.
pub fn convert(model: &VoxModel, kept: &Palette) -> Result<Converted> {
    let from_size = model.size;
    let grown = fitted(model);
    if let Some(g) = &grown
        && let Some(d) = differences(model, g).into_iter().find(|d| !d.starts_with("size"))
    {
        bail!("growing the box moved the model: {d}");
    }
    let model = grown.as_ref().unwrap_or(model);
    let mut rig = rig_by_hand(model);
    let given = rig.clone();
    for _ in 0..=model.parts.len() {
        let text = write_rig(model, kept, &rig);
        let back = read(&text).map_err(|e| anyhow!("the text written does not read back: {e}"))?;
        let mut changed = false;
        for (i, (p, q)) in model.parts.iter().zip(&back.parts).enumerate() {
            if p.parent != q.parent && rig[i].0.is_none() {
                rig[i].0 = p.parent;
                changed = true;
            }
            if (p.joint != q.joint || p.parent != q.parent) && rig[i].1.is_none() {
                rig[i].1 = Some(p.joint);
                changed = true;
            }
        }
        if changed {
            continue;
        }
        let d = differences(model, &back);
        if !d.is_empty() {
            bail!("the text does not read back the same: {}", d.join("; "));
        }
        let set = |k: usize| rig.iter().zip(&given).filter(|(r, g)| if k == 0 { r.0.is_some() && g.0.is_none() } else { r.1.is_some() && g.1.is_none() }).count();
        return Ok(Converted { joints: set(1), parents: set(0), text, dropped: doubled(model), model: back, grown: grown.as_ref().map(|m| (from_size, m.size)) });
    }
    bail!("the parts would not hang as before however their rig was written")
}

/// The model with its box grown to hold every voxel (MagicaVoxel's files
/// may keep some beyond their size, which the engine drew all the same):
/// the box only as much bigger across and deep, on both sides of the
/// pivot, and up as needed, every voxel and joint where it was from the
/// pivot. None when every voxel lies inside.
fn fitted(model: &VoxModel) -> Option<VoxModel> {
    let pivot = pivot(model.size);
    let all = model.frames.iter().flatten().map(|(p, _)| *p - pivot);
    let (low, high) = all.fold((IVec3::MAX, IVec3::MIN), |(l, h), p| (l.min(p), h.max(p)));
    if low.cmpgt(high).any() || (low.cmpge(-pivot).all() && high.cmplt(model.size - pivot).all()) {
        return None;
    }
    if low.z < 0 {
        return None;
    }
    let mut size = model.size;
    for a in 0..2 {
        while -(size[a] / 2) > low[a] || size[a] - size[a] / 2 - 1 < high[a] {
            size[a] += 1;
        }
    }
    size.z = size.z.max(high.z + 1);
    let shift = self::pivot(size) - pivot;
    let frames = model.frames.iter().map(|f| f.iter().map(|&(p, c)| (p + shift, c)).collect()).collect();
    let mut grown = VoxModel::new(size, frames, model.metal.clone());
    grown.glow = model.glow.clone();
    grown.parts = model.parts.clone();
    Some(grown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &VoxModel, b: &VoxModel) {
        let d = differences(a, b);
        assert!(d.is_empty(), "{d:?}");
    }

    /// Writes, reads back and finds the same; the text stays as written.
    fn round_trip(m: &VoxModel) -> (String, VoxModel) {
        let text = write(m, &Palette::default());
        let back = read(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        same(m, &back);
        assert_eq!(write(&back, &Palette::of_text(&text)), text);
        (text, back)
    }

    fn head(size: &str) -> String {
        format!("format = \"{FORMAT}\"\nsize = {size}\n")
    }

    /// The far row first, frames placed by `at` from the pivot, the box's
    /// corner without one.
    #[test]
    fn the_far_row_comes_first_and_frames_are_placed_from_the_pivot() {
        // A 3 by 2 by 2 box: the pivot is cell (1, 1, 0), the box runs
        // from -1 to 1 across and -1 to 0 deep.
        let text = head("[3, 2, 2]") + "[palette]\na = \"#ff0000\"\nb = \"#00ff00\"\n[[frame]]\nlayers = \"\"\"\naa.\nb..\n\n...\n..b\n\"\"\"\n[[frame]]\nat = [0, -1, 0]\nlayers = \"\"\"\nb\n\"\"\"\n";
        let m = read(&text).unwrap();
        assert_eq!((m.size, m.frames.len()), (IVec3::new(3, 2, 2), 2));
        let at = |f: usize, p: IVec3| m.frames[f].iter().find(|v| v.0 == p).map(|v| v.1);
        assert_eq!(at(0, IVec3::new(0, 1, 0)), Some([255, 0, 0, 255]), "the first row is the far one");
        assert_eq!(at(0, IVec3::new(0, 0, 0)), Some([0, 255, 0, 255]));
        assert_eq!(at(0, IVec3::new(2, 0, 1)), Some([0, 255, 0, 255]));
        assert_eq!(m.frames[1], vec![(IVec3::new(1, 0, 0), [0, 255, 0, 255])], "at [0, -1, 0]: the pivot cell's column, the near row");
        // Reading order: layer by layer, rows far to near, x left to right.
        assert_eq!(m.frames[0].iter().map(|v| v.0).collect::<Vec<_>>(), [IVec3::new(0, 1, 0), IVec3::new(1, 1, 0), IVec3::new(0, 0, 0), IVec3::new(2, 0, 1)]);
        round_trip(&m);
    }

    /// Rows and layers that repeat are written once with their count from
    /// three in a row on; two stay written out.
    #[test]
    fn repeats_fold_rows_and_layers() {
        let mut frame = Vec::new();
        for z in 0..6 {
            for y in 0..5 {
                for x in 0..4 {
                    // Five layers alike, then one with a different row.
                    if z < 5 || y != 2 || x == 0 {
                        frame.push((IVec3::new(x, y, z), [9, 9, 9, 255]));
                    }
                }
            }
        }
        let m = VoxModel::new(IVec3::new(4, 5, 6), vec![frame], vec![Vec::new()]);
        let (text, _) = round_trip(&m);
        assert!(text.contains("aaaa *5\n*5\n"), "{text}");
        assert!(text.contains("\naaaa\naaaa\na...\naaaa\naaaa\n"), "two alike stay rows: {text}");
        // By hand: a repeat that is not one is said with its line.
        let bad = |layers: &str| read(&(head("[2, 2, 2]") + "[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\n" + layers + "\"\"\"\n")).err().map(|e| e.to_string()).unwrap();
        assert!(bad("aa *1\n").contains("line 7:"));
        assert!(bad("aa *x\n").contains("no repeat"));
        assert!(bad("*2\n").contains("comes on the line after its rows"));
        assert!(bad("aa *2\n*3\n").contains("line 8: that makes 3 layers, more than there is room for: 2 at most"), "three layers in a box two high");
        assert!(read(&(head("[2, 2, 2]") + "[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\naa *2\n*2\n\"\"\"\n")).is_ok_and(|m| m.frames[0].len() == 8));
    }

    /// A part's rest rotation (`rotation`, `pitch`, `roll` about its joint,
    /// degrees) is read, kept and written back as it was, each line only
    /// when not 0; a figure without one writes as before. `turn` on a part
    /// is refused with its new name, a rotation on a frame refused.
    #[test]
    fn a_part_keeps_its_rest_rotation() {
        let plain = head("[2, 1, 3]") + "\n[palette]\na = \"#888888\"\n\n[[part]]\nname = \"foot\"\nat = [-1, 0, 0]\nlayers = \"\"\"\naa\n\"\"\"\n\n[[part]]\nname = \"leg\"\nat = [-1, 0, 1]\nparent = \"foot\"\njoint = [-0.5, 0, 1.5]\nlayers = \"\"\"\na\n\"\"\"\n";
        let turned = plain.replace("joint = [-0.5, 0, 1.5]\n", "joint = [-0.5, 0, 1.5]\nrotation = 45\nroll = -12.5\n");
        let m = read(&turned).unwrap();
        assert_eq!((m.parts[0].rest, m.parts[1].rest), ([0.0; 3], [45.0, 0.0, -12.5]));
        let (shape, palette) = read_shape(&turned).unwrap();
        assert_eq!(shape.pieces[1].rotation, [45.0, 0.0, -12.5]);
        assert_eq!(write_shape(&shape, &palette, false), turned, "written back byte for byte");
        let (written, _) = round_trip(&m);
        assert_eq!(written, turned);
        let (shape, palette) = read_shape(&plain).unwrap();
        assert_eq!(write_shape(&shape, &palette, false), plain, "none: as before");
        assert!(read(&plain).unwrap().parts.iter().all(|p| p.rest == [0.0; 3]));
        // The root may rest turned too.
        let root = plain.replace("at = [-1, 0, 0]\n", "at = [-1, 0, 0]\npitch = 90\n");
        assert_eq!(read(&root).unwrap().parts[0].rest, [0.0, 90.0, 0.0]);
        let e = read(&plain.replace("joint = [-0.5, 0, 1.5]\n", "joint = [-0.5, 0, 1.5]\nturn = 45\n")).err().map(|e| e.to_string()).unwrap();
        assert!(e.contains("'turn' is called 'rotation' now") && e.contains("line "), "{e}");
        let e = read(&plain.replace("joint = [-0.5, 0, 1.5]\n", "joint = [-0.5, 0, 1.5]\npitch = \"up\"\n")).err().map(|e| e.to_string()).unwrap();
        assert!(e.contains("pitch must be a number"), "{e}");
        let frame = head("[1, 1, 1]") + "[palette]\na = \"#888888\"\n[[frame]]\nrotation = 45\nlayers = \"\"\"\na\n\"\"\"\n";
        let e = read(&frame).err().map(|e| e.to_string()).unwrap();
        assert!(e.contains("a frame does not rotate ('rotation')"), "{e}");
    }

    /// A part's offset (less than a voxel from its `at`): read, written
    /// one line after `at` only when not 0, byte for byte; drawn moved, its
    /// joint and what hangs on it along; refused on a frame and beyond half
    /// a voxel.
    #[test]
    fn a_part_keeps_its_offset() {
        let plain = head("[2, 1, 3]") + "\n[palette]\na = \"#888888\"\n\n[[part]]\nname = \"foot\"\nat = [-1, 0, 0]\nlayers = \"\"\"\naa\n\"\"\"\n\n[[part]]\nname = \"leg\"\nat = [-1, 0, 1]\nparent = \"foot\"\njoint = [-0.5, 0, 1.5]\nlayers = \"\"\"\na\n\"\"\"\n";
        let moved = plain.replace("name = \"foot\"\nat = [-1, 0, 0]\n", "name = \"foot\"\nat = [-1, 0, 0]\noffset = [0, 0.3, -0.25]\n");
        let (shape, palette) = read_shape(&moved).unwrap();
        assert_eq!(shape.pieces[0].offset, Vec3::new(0.0, 0.3, -0.25));
        assert_eq!(write_shape(&shape, &palette, false), moved, "written back byte for byte");
        let (shape, palette) = read_shape(&plain).unwrap();
        assert_eq!(write_shape(&shape, &palette, false), plain, "none: no line");
        // Drawn moved by it, the leg hanging on the foot along.
        let (m, n) = (read(&moved).unwrap(), read(&plain).unwrap());
        for p in 0..2 {
            let (a, b) = (crate::session::part_at_rest(&m, p).1, crate::session::part_at_rest(&n, p).1);
            assert!((a - b - Vec3::new(0.0, 0.3, -0.25)).length() < 1e-5, "part {p}: {a} against {b}");
        }
        let e = read(&moved.replace("offset = [0, 0.3, -0.25]", "offset = [0, 0.7, 0]")).err().map(|e| e.to_string()).unwrap();
        assert!(e.contains("less than a voxel"), "{e}");
        let frame = head("[1, 1, 1]") + "[palette]\na = \"#888888\"\n[[frame]]\noffset = [0, 0.5, 0]\nlayers = \"\"\"\na\n\"\"\"\n";
        let e = read(&frame).err().map(|e| e.to_string()).unwrap();
        assert!(e.contains("a frame has no offset"), "{e}");
    }

    /// A figure's parts keep their order, names and the rig set by hand;
    /// `at` and `joint` are from the pivot, so a part placed lower than
    /// all others leaves theirs alone.
    #[test]
    fn a_figure_keeps_its_parts_and_the_rig_set_by_hand() {
        let text = head("[2, 1, 3]") + "[palette]\na = \"#888888\"\n[[part]]\nname = \"foot\"\nat = [-1, 0, 0]\nlayers = \"\"\"\naa\n\"\"\"\n[[part]]\nname = \"leg\"\nat = [-1, 0, 1]\nparent = \"foot\"\njoint = [-0.5, 0, 1.5]\nlayers = \"\"\"\na\n\"\"\"\n[[part]]\nname = \"a hat\"\nat = [-1, 0, 2]\nlayers = \"\"\"\na\n\"\"\"\n";
        let m = read(&text).unwrap();
        assert_eq!(m.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["foot", "leg", "a hat"]);
        assert_eq!(m.parts[1].parent, Some(0));
        assert!(m.parts[1].joint_by_hand && m.parts[1].hung == Hung::ByHand);
        assert_eq!(m.parts[1].joint, Vec3::new(-0.5, 0.0, 1.5), "the joint is the model's, from the pivot cell");
        assert!(!m.parts[2].joint_by_hand);
        let (written, _) = round_trip(&m);
        assert!(written.contains("parent = \"foot\"") && written.contains("joint = [-0.5, 0, 1.5]") && written.contains("name = \"a hat\""), "{written}");
        assert_eq!(written.matches("joint =").count(), 1, "only what was set by hand");
        let two = text.replace("name = \"a hat\"", "name = \"leg\"");
        assert!(read(&two).err().is_some_and(|e| e.to_string().contains("two parts are called 'leg'")));
    }

    #[test]
    fn glass_metal_and_glow_are_carried() {
        let text = head("[4, 1, 1]") + "[palette]\na = \"#102030\"\ng = { color = \"#88ccff\", glass = 0.5, rough = 0.1 }\nm = { color = \"#c0c0c0\", metal = 1.0, rough = 0.3 }\nb = { color = \"#102030\", glow = 1.0 }\n[[frame]]\nlayers = \"\"\"\nagmb\n\"\"\"\n";
        let m = read(&text).unwrap();
        assert_eq!(m.frames[0][1].1[3], 128);
        assert_eq!(m.metal[0][1], metal::pack(0.0, 0.9));
        assert_eq!(m.metal[0][2], metal::pack(1.0, 0.7));
        assert_eq!(m.glow[0], vec![0, 0, 0, 255]);
        let (written, _) = round_trip(&m);
        assert!(written.contains("glass = 0.5") && written.contains("metal = 1.0") && written.contains("glow = 1.0"), "{written}");
        assert_eq!(written.lines().filter(|l| l.contains("#102030")).count(), 2, "the same color as another material is another sign");
    }

    /// Written again, a model keeps the signs its file had, even for a
    /// voxel that now comes first; a new colour takes a sign no entry had.
    #[test]
    fn signs_stay_what_they_were() {
        let text = head("[3, 1, 1]") + "[palette]\nq = \"#ff0000\"\nb = \"#0000ff\"\n[[frame]]\nlayers = \"\"\"\nqbq\n\"\"\"\n";
        let mut m = read(&text).unwrap();
        m.frames[0].retain(|v| v.0.x != 0);
        m.frames[0].insert(0, (IVec3::new(0, 0, 0), [0, 255, 0, 255]));
        m.metal[0] = vec![0; 3];
        let again = write(&m, &Palette::of_text(&text));
        assert!(again.contains("q = \"#ff0000\"\nb = \"#0000ff\"\na = \"#00ff00\"\n") && again.contains("abq"), "{again}");
        // A fresh text gives signs by first use.
        assert!(write(&m, &Palette::default()).contains("abc"));
    }

    /// Two voxels in one cell of a frame: the first stays and the rest are
    /// counted; parts may overlap each other.
    #[test]
    fn a_cell_holds_one_voxel_of_a_frame() {
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
        let m = VoxModel::new(IVec3::new(2, 1, 1), vec![vec![(IVec3::ZERO, red), (IVec3::ZERO, blue), (IVec3::X, red)]], vec![vec![0; 3]]);
        assert_eq!(doubled(&m), 1);
        let c = convert(&m, &Palette::default()).unwrap();
        assert_eq!(c.dropped, 1);
        let back = read(&c.text).unwrap();
        assert_eq!(back.frames[0], vec![(IVec3::ZERO, red), (IVec3::X, red)]);
        let text = head("[2, 1, 1]") + "[palette]\na = \"#ff0000\"\nb = \"#0000ff\"\n[[part]]\nname = \"one\"\nlayers = \"\"\"\naa\n\"\"\"\n[[part]]\nname = \"two\"\nat = [0, 0, 0]\nlayers = \"\"\"\nb\n\"\"\"\n";
        let m = read(&text).unwrap();
        assert_eq!((m.frames[0].len(), m.parts[0].overlaps.clone()), (3, vec![(1, 1)]));
        round_trip(&m);
    }

    #[test]
    fn more_than_62_colours_take_two_signs_a_cell() {
        let frame: Vec<_> = (0..100).map(|i| (IVec3::new(i % 10, i / 10, 0), [i as u8 * 2, 3, 4, 255])).collect();
        let m = VoxModel::new(IVec3::new(10, 10, 1), vec![frame], vec![vec![0; 100]]);
        let (text, _) = round_trip(&m);
        assert!(text.contains("\naa = \"#"), "{text}");
        let few: Vec<_> = (0..62).map(|i| (IVec3::new(i, 0, 0), [i as u8, 3, 4, 255])).collect();
        let m = VoxModel::new(IVec3::new(62, 1, 1), vec![few], vec![vec![0; 62]]);
        assert!(write(&m, &Palette::default()).contains("\n9 = \"#"));
    }

    /// The format line first, a version known; then every mistake says
    /// its line and what is wrong.
    #[test]
    fn mistakes_say_the_line_and_what_is_wrong() {
        let why = |text: String| read(&text).err().map(|e| e.to_string()).unwrap();
        assert!(why("size = [1, 1, 1]\n".into()).contains("first line must be format = \"voxcade voxel 1\""));
        assert!(why("size = [1, 1, 1]\nformat = \"voxcade voxel 1\"\n".into()).contains("first line must be"));
        let e = why("format = \"voxcade voxel 2\"\n".into());
        assert!(e.starts_with("line 1:") && e.contains("newer Voxcade"), "{e}");
        let body = |layers: &str| head("[2, 2, 1]") + "[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\n" + layers + "\"\"\"\n";
        // The layers begin on line 7.
        let e = why(body("a.\nax\n"));
        assert!(e.starts_with("line 8:") && e.contains("'x' is no sign of the palette"), "{e}");
        let e = why(body("a.\na\n"));
        assert!(e.starts_with("line 8:") && e.contains("1 cells, the first has 2"), "{e}");
        let e = why(body("aaa\n"));
        assert!(e.starts_with("line 7:") && e.contains("outside the model's size"), "{e}");
        assert!(why(head("[1, 1, 1]") + "[palette]\na = \"red\"\n").starts_with("line 4:"));
        let e = why(head("[1, 1, 1]") + "[palette]\na = \"#ff0000\"\n[[part]]\nname = \"x\"\nparent = \"y\"\nlayers = \"\"\"\na\n\"\"\"\n");
        assert!(e.starts_with("line 7:") && e.contains("no part is called 'y'"), "{e}");
        assert!(why(head("[1, 1, 1]") + "[palette]\na = { color = \"#ff0000\", shiny = 1 }\n").contains("'shiny' is no field"));
        // A box capped by its volume, not only each way: a hundred
        // thousand voxels each way once made `sheet` ask for 3.5 GB.
        let e = why(head("[100000, 100000, 100000]") + "[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n");
        assert!(e.starts_with("line 2:") && e.contains("too big") && e.contains(&MOST_VOXELS.to_string()), "{e}");
        assert!(read(&(head("[512, 512, 128]") + "[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n")).is_ok(), "the most a box holds reads");
    }

    /// Hostile texts, fed to every reader of the text formats whose sizes
    /// drive what is made (models, worlds and prefabs, levels with their
    /// cells and terrain, maps): each is a plain mistake, never a panic, an
    /// endless loop or an allocation that aborts the process. A row repeat
    /// of `*99999999999` once asked for 2.4 TB and dumped core.
    #[test]
    fn hostile_texts_are_mistakes_never_a_crash() {
        let model = |size: &str, layers: &str| format!("{}[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\n{layers}\"\"\"\n", head(size));
        let world = |size: &str, layers: &str| format!("format = \"voxcade voxel 1\"\nsize = {size}\n\n[palette]\na = \"#ff0000\"\nm = {{ marker = \"guard\" }}\n\n[[frame]]\nlayers = \"\"\"\n{layers}\"\"\"\n");
        let level = |head: &str, layers: &str| format!("format = \"voxcade level 1\"\n{head}\n[blocks]\nstone = \"solid\"\n[legend]\ns = \"stone\"\n[cells]\nlayers = \"\"\"\n{layers}\"\"\"\n");
        let land = |heights: &str| format!("format = \"voxcade level 1\"\nsize = [64, 64, 32]\n[terrain]\nspacing = 1024\nheights = \"\"\"\n{heights}\n\"\"\"\n");
        let map = |size: &str| format!("format = \"voxcade map 1\"\nsize = {size}\n[ground]\nheight = \"h.png\"\n[[ground.materials]]\nsurface = [\"#ffffff\"]\n");
        let deep = format!("{}x = {}1{}\n", head("[1, 1, 1]"), "[".repeat(20_000), "]".repeat(20_000));
        let long_row = model("[2, 1, 1]", &format!("{}\n", "a".repeat(10_000_000)));
        let many_parts = format!("{}[palette]\na = \"#ff0000\"\n{}", head("[1, 1, 1]"), (0..2000).map(|i| format!("[[part]]\nname = \"p{i}\"\n")).collect::<String>());
        let wide_land = land(&"0 ".repeat(2_100_000));
        type Reading = fn(&str) -> Result<()>;
        let as_model: Reading = |t| read(t).map(drop);
        let as_world: Reading = |t| crate::scene::read(t).map(drop);
        let as_level: Reading = |t| crate::grid::text::read(t).map(drop);
        let as_map: Reading = |t| crate::map::MapData::read(t).map(drop);
        let cases: Vec<(&str, Reading, String, &str)> = vec![
            ("a row repeated past the box", as_model, model("[4, 4, 4]", "aaaa *99999999999\n"), "line 7:"),
            ("a layer repeated past the box", as_model, model("[4, 4, 4]", "....\n*99999999999\n"), "line 8:"),
            ("a repeat past every number", as_model, model("[4, 4, 4]", "a *999999999999999999999999\n"), "line 7:"),
            ("rows past the box's depth", as_model, model("[1, 2, 1]", "a\na\na\n"), "line 9:"),
            ("layers past the box's height", as_model, model("[1, 1, 2]", "a\n\na\n\na\n"), "line 11:"),
            ("a negative size", as_model, model("[-1, 2, 3]", "a\n"), "line 2:"),
            ("a size past the numbers", as_model, model("[2147483647, 2147483647, 2147483647]", "a\n"), "line 2:"),
            ("a product past i32", as_model, model("[1000000, 1000000, 1000000]", "a\n"), "line 2:"),
            ("a very long row", as_model, long_row, "line 7:"),
            ("deep nesting", as_model, deep, ""),
            ("a figure of too many parts", as_model, many_parts, "line 5:"),
            ("a world's row repeated", as_world, world("[4, 4, 4]", "aaaa *99999999999\n"), "line 10:"),
            ("a world's layer repeated", as_world, world("[4, 4, 4]", "....\n*99999999999\n"), "line 11:"),
            ("a world too big", as_world, world("[1000000, 1000000, 1]", "a\n"), "line 2:"),
            ("a world past i32", as_world, world("[1000000, 1000000, 1000000]", "a\n"), "line 2:"),
            ("a level's row repeated", as_level, level("block = 2\nsize = [2, 1, 2]", "ss *99999999999\n"), "line 10:"),
            ("a level's layer repeated", as_level, level("block = 2\nsize = [2, 1, 2]", "ss\n*99999999999\n"), "line 11:"),
            ("a level of huge blocks", as_level, level("block = 100000\nsize = [1000, 1000, 64]", "s\n"), "line 3:"),
            ("a level without blocks too big", as_level, "format = \"voxcade level 1\"\nsize = [100000, 100000, 100000]\n".into(), "line 2:"),
            ("a terrain wider than any level", as_level, wide_land, ""),
            ("a map too big", as_map, map("[100000, 100000, 10]"), "line 2:"),
            ("a map past i32", as_map, map("[1000000, 1000000, 1000000]"), "line 2:"),
        ];
        for (what, reading, text, line) in cases {
            let got = std::panic::catch_unwind(|| reading(&text));
            let e = match got {
                Ok(Err(e)) => e.to_string(),
                Ok(Ok(())) => panic!("{what}: read without a mistake"),
                Err(_) => panic!("{what}: a panic"),
            };
            assert!(e.starts_with(line), "{what}: {e}");
            assert!(e.len() < 2000, "{what}: a mistake short enough to read: {e}");
        }
    }

    /// Every number a reader takes from a file is checked before it is
    /// cast: 2^31, 2^32 and 2^63 (past TOML's numbers), -1 where it may
    /// not be, and 1e30 are mistakes, never a wrapped number taken as a
    /// good one (2^32 as an i32 is 0: a map's place at x 0, a rotation of
    /// 0, a scatter rule 0 were read so before).
    #[test]
    fn numbers_past_their_range_are_mistakes_never_wrapped() {
        let map = |extra: &str| format!("format = \"voxcade map 1\"\nsize = [64, 48, 20]\n[ground]\nheight = \"h.png\"\n[[ground.materials]]\nsurface = [\"#ffffff\"]\n{extra}");
        let level = |extra: &str| format!("format = \"voxcade level 1\"\nblock = 2\nsize = [2, 1, 2]\n[blocks]\nstone = \"solid\"\n[legend]\ns = \"stone\"\n[cells]\nlayers = \"\"\"\nss\n\nss\n\"\"\"\n{extra}");
        let land = |extra: &str| format!("format = \"voxcade level 1\"\nsize = [64, 64, 32]\n[terrain]\nspacing = 8\nheights = \"\"\"\n1 2\n\"\"\"\n{extra}\n[[terrain.material]]\ncolor = \"#5c8a3a\"\n");
        let as_map = |t: &str| crate::map::MapData::read(t).map(drop);
        let as_level = |t: &str| crate::grid::text::read(t).map(drop);
        assert!(as_map(&map("[[put]]\nthing = \"guard\"\nat = [3, 4]\nrotation = 90\n[[place]]\nname = \"s\"\nat = [1, 1, 1]\nfacing = [0, 1]\n")).is_ok());
        assert!(as_level(&level("[[thing]]\nmodel = \"fir.vvx\"\nat = [0, 0, 0]\nscatter = 1\n[[place]]\nname = \"s\"\nat = [0, 0, 0]\nfacing = [0, 1]\n")).is_ok());
        assert!(as_level(&land("terrace = 4")).is_ok());
        for n in ["2147483648", "4294967296", "9223372036854775808", "1e30"] {
            let cases: Vec<(&str, Result<()>)> = vec![
                ("a model's size", read(&format!("{}[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n", head(&format!("[{n}, 1, 1]")))).map(drop)),
                ("a map's size", as_map(&map("").replace("[64, 48, 20]", &format!("[64, 48, {n}]")))),
                ("a put's at", as_map(&map(&format!("[[put]]\nthing = \"guard\"\nat = [{n}, 4]\n")))),
                ("a put's rotation", as_map(&map(&format!("[[put]]\nthing = \"guard\"\nat = [3, 4]\nrotation = {n}\n")))),
                ("a map's place's facing", as_map(&map(&format!("[[place]]\nname = \"s\"\nat = [1, 1, 1]\nfacing = [{n}, 1]\n")))),
                ("a level's block", as_level(&level("").replace("block = 2", &format!("block = {n}")))),
                ("a thing's scatter", as_level(&level(&format!("[[thing]]\nmodel = \"fir.vvx\"\nat = [0, 0, 0]\nscatter = {n}\n")))),
                ("a level's place's facing", as_level(&level(&format!("[[place]]\nname = \"s\"\nat = [0, 0, 0]\nfacing = [{n}, 1]\n")))),
                ("a thing's at", as_level(&level(&format!("[[thing]]\nmodel = \"fir.vvx\"\nat = [{n}, 0, 0]\n")))),
                ("a terrain's height", as_level(&land("").replace("1 2\n", &format!("1 {n}\n")))),
                ("a terrain's terrace", as_level(&land(&format!("terrace = {n}")))),
                ("a terrain's spacing", as_level(&land("").replace("spacing = 8", &format!("spacing = {n}")))),
            ];
            for (what, got) in cases {
                assert!(got.is_err(), "{what} = {n} read as a good number");
            }
        }
        // -1 where only 0 and more may be.
        for (what, got) in [
            ("a model's size", read(&format!("{}[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n", head("[-1, 1, 1]"))).map(drop)),
            ("a put's at", as_map(&map("[[put]]\nthing = \"guard\"\nat = [-1, 4]\n"))),
            ("a thing's scatter", as_level(&level("[[thing]]\nmodel = \"fir.vvx\"\nat = [0, 0, 0]\nscatter = -1\n"))),
            ("a terrain's height", as_level(&land("").replace("1 2\n", "1 -1\n"))),
            ("a terrain's terrace", as_level(&land("terrace = -1"))),
        ] {
            assert!(got.is_err(), "{what} = -1 read as a good number");
        }
        // A number too large says its limit, not that it is no whole number.
        let e = format!("{:#}", as_map(&map("").replace("[64, 48, 20]", "[640, 2368, 99999999]")).unwrap_err());
        assert!(e.starts_with("line 2:") && e.contains("99999999 is past the limit, 1000000 at most") && !e.contains("must be three whole numbers"), "{e}");
        let e = format!("{:#}", as_map(&map("[[put]]\nthing = \"guard\"\nat = [3, 4]\nrotation = 4294967296\n")).unwrap_err());
        assert!(e.contains("rotation = 4294967296: it must be from -1000000 to 1000000"), "{e}");
        let e = format!("{:#}", as_level(&level("").replace("block = 2", "block = 200000")).unwrap_err());
        assert!(e.contains("block = 200000: it must be from 1 to 100000"), "{e}");
        let e = format!("{:#}", as_level(&land("").replace("1 2\n", "1 2147483648\n")).unwrap_err());
        assert!(e.contains("heights: 2147483648 is out of range (0 to 4096)"), "{e}");
        let e = format!("{:#}", as_level(&land("terrace = 5000")).unwrap_err());
        assert!(e.contains("terrace must be a whole number of voxels, 1 to 4096"), "{e}");
        let e = format!("{:#}", read(&format!("{}[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n", head("[1.5, 1, 1]"))).map(drop).unwrap_err());
        assert!(e.contains("size must be three whole numbers"), "a fraction is still no whole number: {e}");
    }

    /// The repo's `.vvx` files (models, worlds, prefabs) as they stand are
    /// what both writers write: the engine's (`write_shape`, a world's
    /// `scene::Scene::write`) and `tools/vvx.py`'s (`write_model`,
    /// `write_world`), which the generators use. A world is told by the
    /// comment both writers put over its palette.
    #[test]
    fn the_python_writer_writes_what_the_engine_writes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                if p.is_dir() {
                    walk(&p, out);
                } else if is_model(&p) {
                    out.push(p);
                }
            }
        }
        for folder in ["games", "templates", "examples", "music"] {
            walk(&root.join(folder), &mut files);
        }
        assert!(files.len() > 100, "the repo's models: {}", files.len());
        let world = |src: &str| src.contains("# The colours are the world's numbers");
        // The engine's writers.
        for f in &files {
            let src = std::fs::read_to_string(f).unwrap();
            let again = if world(&src) {
                crate::scene::read(&src).unwrap_or_else(|e| panic!("{}: {e}", f.display())).write()
            } else {
                let (shape, palette) = read_shape(&src).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
                write_shape(&shape, &palette, false)
            };
            assert!(again == src, "{}: the engine writes it otherwise", f.display());
        }
        // Python's, all files in one run. CI has python3; a machine without
        // it says so (not silently passing).
        if std::process::Command::new("python3").arg("--version").output().is_err() {
            eprintln!("SKIPPED the_python_writer_writes_what_the_engine_writes' Python half: no python3 on this machine");
            return;
        }
        let out = crate::scratch::Scratch::new("vvx-python");
        let script = r##"
import sys
sys.path.insert(0, sys.argv[1])
import vvx
out = sys.argv[2]
for n, f in enumerate(sys.argv[3:]):
    m = vvx.read(f)
    dest = f"{out}/{n}.vvx"
    if "# The colours are the world's numbers" in open(f).read():
        vvx.write_world(dest, m, vvx.palette_signs(f))
    else:
        vvx.write_model(dest, m)
"##;
        let run = std::process::Command::new("python3").arg("-B").arg("-c").arg(script).arg(root.join("tools")).arg(out.path()).args(&files).output().unwrap();
        assert!(run.status.success(), "tools/vvx.py: {}", String::from_utf8_lossy(&run.stderr));
        for (n, f) in files.iter().enumerate() {
            let wrote = std::fs::read_to_string(out.join(format!("{n}.vvx"))).unwrap();
            assert!(wrote == std::fs::read_to_string(f).unwrap(), "{}: tools/vvx.py writes it otherwise", f.display());
        }
    }

    /// Models from MagicaVoxel come in the same: figures of parts touching
    /// and apart, their rig by hand too, frames of glass and metal.
    #[test]
    fn models_from_magicavoxel_come_in_the_same() {
        let dir = crate::scratch::Scratch::new("vvx-convert");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        let parts = [("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("arm", IVec3::new(1, 2, 5), IVec3::new(6, 1, 2)), ("hand", IVec3::new(1, 2, 1), IVec3::new(9, 1, 1)), ("hat", IVec3::new(3, 3, 1), IVec3::new(0, 0, 14))];
        for rig in [&[][..], &[("arm", "_parent", "head"), ("hand", "_joint", "6 1.5 1.5")][..]] {
            crate::vox::write_figure_with(&file, &parts, rig);
            let m = crate::vox::import_model(&file).unwrap();
            let c = convert(&m, &Palette::default()).unwrap();
            let back = read(&c.text).unwrap();
            same(&m, &back);
            // Written again from what it read, the text stays.
            assert_eq!(write(&back, &Palette::of_text(&c.text)), c.text);
        }
        for game in ["games/moreblood/huey.vox", "games/voxkeep/gem.vox"].map(|f| Path::new(env!("CARGO_MANIFEST_DIR")).join(f)) {
            if let Ok(m) = crate::vox::import_model(&game) {
                same(&m, &read(&convert(&m, &Palette::default()).unwrap().text).unwrap());
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}

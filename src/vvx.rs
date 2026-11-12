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
//! parent = "body"              # only when set by hand
//! joint = [2, 0, 7.5]          # only when set by hand, from the pivot
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
}

// ---------------------------------------------------------------- writing

/// The material of voxel `k` of frame `f`.
fn mat_of(model: &VoxModel, f: usize, k: usize) -> Mat {
    let rgba = model.frames[f][k].1;
    Mat { rgba, metal: model.metal.get(f).and_then(|m| m.get(k)).copied().unwrap_or(0), glow: model.glow.get(f).and_then(|g| g.get(k)).copied().unwrap_or(0) }
}

/// The signs a text gives out: those the file had for the same material
/// first, then new ones by first use, never one the file had for another.
struct Signs {
    width: usize,
    kept: Vec<(String, Mat)>,
    of: HashMap<Mat, String>,
    order: Vec<(String, Mat)>,
    next: usize,
}

impl Signs {
    fn new(width: usize, kept: &Palette) -> Self {
        let kept = kept.entries.iter().filter(|(s, _)| s.chars().count() == width).cloned().collect();
        Self { width, kept, of: HashMap::new(), order: Vec::new(), next: 0 }
    }

    fn nth(&self, i: usize) -> String {
        if self.width == 1 { (SIGNS[i] as char).to_string() } else { format!("{}{}", SIGNS[i / ONE_SIGN] as char, SIGNS[i % ONE_SIGN] as char) }
    }

    fn sign(&mut self, mat: Mat) -> String {
        if let Some(s) = self.of.get(&mat) {
            return s.clone();
        }
        let s = match self.kept.iter().find(|(_, m)| *m == mat) {
            Some((s, _)) => s.clone(),
            None => loop {
                let s = self.nth(self.next);
                self.next += 1;
                if !self.kept.iter().any(|(k, _)| *k == s) && !self.order.iter().any(|(k, _)| *k == s) {
                    break s;
                }
            },
        };
        self.of.insert(mat, s.clone());
        self.order.push((s.clone(), mat));
        s
    }

    /// The palette's lines: the kept signs used (with `all` every kept
    /// one, used or not, but a second for a material another sign has), in
    /// the file's order, then the new ones as they came.
    fn lines(&self, all: bool) -> String {
        let mut used: Vec<&(String, Mat)> = self.kept.iter().filter(|(s, m)| self.of.get(m).map_or(all && !self.kept.iter().take_while(|(k, _)| k != s).any(|(_, o)| o == m), |o| o == s)).collect();
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
                    (0..dim.x)
                        .map(|x| match cells[((z * dim.y + y) * dim.x + x) as usize] {
                            None => dots.clone(),
                            Some(m) => signs.sign(m),
                        })
                        .collect()
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
        for (row, n) in rows_runs(layer) {
            s += row;
            if n >= REPEAT_FROM {
                s += &format!(" *{n}\n");
            } else {
                s += &format!("\n{}", format!("{row}\n").repeat(n - 1));
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

fn rows_runs(rows: &[String]) -> Vec<(&String, usize)> {
    runs(rows)
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
                Piece { name: p.name.clone(), voxels: voxels(0, &mut p.voxels.iter().map(|&k| k as usize)), parent: parent.map(|q| model.parts[q].name.clone()), joint }
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
            if let Some(q) = &piece.parent {
                body += &format!("parent = {}\n", quoted(q));
            }
            if let Some(j) = piece.joint {
                body += &format!("joint = {}\n", triple_text(j));
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

/// One layer of a `layers` text as read: its rows (the line each is on
/// within the text), how many times it stands.
pub(crate) struct Layer<'a> {
    pub rows: Vec<(usize, &'a str)>,
    pub times: usize,
}

/// The layers of a `layers` text: an empty line between layers, a row
/// followed by ` *N` that row N times in all, a line `*N` after a layer's
/// rows that layer N times in all. `line(i)` says where the text's line `i`
/// is in the file (`"line 12: "`).
pub(crate) fn layer_list<'a>(text: &'a str, line: &dyn Fn(usize) -> String) -> Result<Vec<Layer<'a>>> {
    let count = |word: &str, i: usize| -> Result<usize> {
        let n = word.strip_prefix('*').and_then(|n| n.parse::<usize>().ok()).filter(|&n| n >= 2);
        n.ok_or_else(|| anyhow!("{}'{word}' is no repeat: a star and how many times in all, 2 or more (*3)", line(i)))
    };
    let mut layers: Vec<Layer> = Vec::new();
    let mut open = false;
    for (i, raw) in text.lines().enumerate() {
        let words: Vec<&str> = raw.split_whitespace().collect();
        match words[..] {
            [] => open = false,
            [word] if word.starts_with('*') => {
                let layer = layers.last_mut().filter(|l| open && l.times == 1).ok_or_else(|| anyhow!("{}a layer's repeat ({word}) comes on the line after its rows", line(i)))?;
                layer.times = count(word, i)?;
                open = false;
            }
            [row, ref rest @ ..] => {
                let times = match rest {
                    [] => 1,
                    [word] => count(word, i)?,
                    _ => bail!("{}a row is its signs, then perhaps a repeat (*3); this line has more", line(i)),
                };
                if !open {
                    layers.push(Layer { rows: Vec::new(), times: 1 });
                    open = true;
                }
                let layer = layers.last_mut().unwrap();
                layer.rows.extend(std::iter::repeat_n((i, row), times));
            }
        }
    }
    Ok(layers)
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

    pub fn number(&self, item: &Item, what: &str) -> Result<f32> {
        item.as_float().or_else(|| item.as_integer().map(|i| i as f64)).filter(|v| v.is_finite()).map(|v| v as f32).ok_or_else(|| self.fail(item.span(), format!("{what} must be a number")))
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

    pub fn ints(&self, item: &Item, what: &str) -> Result<IVec3> {
        let t = self.triple(item, what)?;
        if t.iter().any(|v| v.fract() != 0.0 || v.abs() > 1e6) {
            return Err(self.fail(item.span(), format!("{what} must be three whole numbers, like [1, 2, 3]")));
        }
        Ok(IVec3::new(t[0] as i32, t[1] as i32, t[2] as i32))
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
        let hex = |text: &str, at: &Item| -> Result<[u8; 3]> {
            let h = text.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()));
            let h = h.ok_or_else(|| self.fail(at.span(), format!("'{text}' is no color: write it as #rrggbb")))?;
            let v = u32::from_str_radix(h, 16).unwrap_or(0);
            Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
        };
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
        // The layers with their rows (a row repeated stands that often).
        let layers = layer_list(text, &line)?;
        let mut out = Vec::new();
        let (mut cols, mut rows) = (None, None);
        let known = || {
            let mut signs: Vec<&String> = palette.keys().collect();
            signs.sort();
            signs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        };
        // The signs by their bytes, quick to look up: one a cell, or two.
        let mut one = [None; 256];
        let mut two: HashMap<&[u8], Mat> = HashMap::new();
        for (sign, mat) in palette {
            match sign.as_bytes() {
                [b] => one[*b as usize] = Some(*mat),
                bytes => _ = two.insert(bytes, *mat),
            }
        }
        let mut z = 0;
        for layer in &layers {
            let depth = layer.rows.len();
            if *rows.get_or_insert(depth) != depth {
                bail!("{}this layer has {depth} rows, the first has {}: every layer must be as deep as the first", line(layer.rows[0].0), rows.unwrap());
            }
            let mut cells = Vec::new();
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
                    bail!("{}this row has {n} cells, the first has {}: every row must be as long as the first", line(i), cols.unwrap());
                }
                for (x, sign) in bytes.chunks(width).enumerate() {
                    if sign.iter().all(|&b| b == b'.') {
                        continue;
                    }
                    let found = if width == 1 { one[sign[0] as usize] } else { two.get(sign).copied() };
                    let mat = found.ok_or_else(|| anyhow!("{}'{}' is no sign of the palette (it has {})", line(i), String::from_utf8_lossy(sign), known()))?;
                    cells.push((i, IVec3::new(x as i32, (depth - 1 - r) as i32, 0), mat));
                }
            }
            for _ in 0..layer.times {
                for &(i, c, mat) in &cells {
                    let p = at + pivot + c + IVec3::Z * z;
                    if p.cmplt(IVec3::ZERO).any() || p.cmpge(size).any() {
                        let f = p - pivot;
                        bail!("{}a voxel at [{}, {}, {}] (from the pivot) lies outside the model's size [{}, {}, {}]: make size bigger or move it in", line(i), f.x, f.y, f.z, size.x, size.y, size.z);
                    }
                    out.push((p, mat));
                }
                z += 1;
            }
        }
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
        model
    };
    model.size = size;
    model
}

/// What the text says (`Shape`) and its palette, every mistake said with
/// its line: the model editor's reading, the engine's (`read`) before the
/// rules hang the parts.
pub fn read_shape(src: &str) -> Result<(Shape, Palette)> {
    let doc = Document::parse(src).map_err(|e| match e.span() {
        Some(s) => anyhow!("line {}: {}", line_of(src, s.start), e.message()),
        None => anyhow!("{}", e.message()),
    })?;
    let r = Reader { src };
    let top = doc.as_table();
    check_format(&r, top)?;
    r.only(top, &["format", "size", "palette", "frame", "part"], "a model")?;
    let size_item = top.get("size").ok_or_else(|| anyhow!("no size = [x, y, z] (the model's box, in voxels)"))?;
    let size = r.ints(size_item, "size")?;
    if size.cmplt(IVec3::ONE).any() {
        return Err(r.fail(size_item.span(), "size must be at least one voxel each way"));
    }
    let pivot = pivot(size);
    let palette_item = top.get("palette").ok_or_else(|| anyhow!("no [palette]: the signs and their colors"))?;
    let table = palette_item.as_table_like().ok_or_else(|| r.fail(palette_item.span(), "palette must be a table of signs"))?;
    let (mut palette, mut entries) = (HashMap::new(), Vec::new());
    let mut width = 0;
    for (sign, item) in table.iter() {
        let n = sign.chars().count();
        if !(1..=2).contains(&n) || sign.chars().any(|c| !c.is_ascii_alphanumeric()) {
            return Err(r.fail(item.span(), format!("'{sign}' cannot be a sign: one or two letters or digits")));
        }
        if width != 0 && width != n {
            return Err(r.fail(item.span(), format!("'{sign}' has {n} characters, the signs before have {width}: all signs of a file are as long as one another")));
        }
        width = n;
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
    let shape = match (top.get("frame"), top.get("part")) {
        (Some(f), None) => {
            let list = tables(f).ok_or_else(|| r.fail(f.span(), "frames must be tables: [[frame]]"))?;
            let mut pieces = Vec::new();
            for t in list {
                r.only(t, &["at", "layers"], "a frame")?;
                let at = t.get("at").map_or(Ok(box_low), |a| r.ints(a, "at"))?;
                let voxels = match t.get("layers") {
                    Some(l) => r.layers(l, &palette, width, at, pivot, size)?,
                    None => Vec::new(),
                };
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
            for t in list {
                r.only(t, &["name", "at", "parent", "joint", "layers"], "a part")?;
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
                let parent = match t.get("parent") {
                    None => None,
                    Some(i) => {
                        spans.push((pieces.len(), i.span()));
                        Some(i.as_str().ok_or_else(|| r.fail(i.span(), "parent must be the name of a part"))?.to_string())
                    }
                };
                let joint = match t.get("joint") {
                    None => None,
                    Some(i) => Some(Vec3::from(r.triple(i, "joint")?)),
                };
                pieces.push(Piece { name, voxels: from_pivot(voxels), parent, joint });
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
            if let Some(chain) = (0..names.len()).find_map(|p| crate::vox::ring(&parents, p)) {
                let named: Vec<&str> = chain.iter().map(|&p| names[p]).collect();
                if chain.len() == 2 {
                    bail!("part '{}' hangs on itself (parent): it must hang on another part, or on none", named[0]);
                }
                bail!("part '{}' hangs on itself by way of the others ({}): one of them must hang on another part, or on none", named[0], named.join(" on "));
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
        if p.overlaps != q.overlaps {
            out.push(format!("{label}: overlaps {:?} vs {:?}", p.overlaps, q.overlaps));
        }
    }
    out
}

/// How many voxels lie in a cell another voxel of the same frame (or part)
/// already fills: the text keeps the first, the rest are dropped.
pub fn doubled(model: &VoxModel) -> usize {
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
        assert!(bad("aa *2\n*3\n").contains("outside the model's size"), "three layers in a box two high");
        assert!(read(&(head("[2, 2, 2]") + "[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\naa *2\n*2\n\"\"\"\n")).is_ok_and(|m| m.frames[0].len() == 8));
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
        let body = |layers: &str| head("[2, 1, 1]") + "[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\n" + layers + "\"\"\"\n";
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

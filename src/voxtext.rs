//! A trial of Voxcade's own model format, as text (TOML): a model's voxels
//! drawn as layers of letters, a palette of signs with their materials,
//! frames or the parts of a figure. Git shows a changed voxel, an agent
//! can write a model by hand. MagicaVoxel's `.vox` would only be imported.
//!
//! Nothing loads this yet: `write` makes the text of what the engine holds
//! of a `.vox` (`vox::VoxModel`), `read` makes the same model of the text,
//! and `voxcade model-trial` proves it on every `.vox` it is given.
//!
//! ```toml
//! size = [5, 3, 3]          # the whole model's box: x, y, z
//!
//! [palette]                 # a sign is a colour, or a table with a material
//! a = "#7a4a2a"
//! b = { color = "#ffd9a0", glow = 1.0 }
//! g = { color = "#88ccff", glass = 0.5, rough = 0.1 }
//! m = { color = "#c0c0c0", metal = 1.0, rough = 0.3 }
//!
//! [[frame]]                 # frames: one table each; `at` is where its box
//! layers = """              # (as small as its voxels allow) lies
//! .aaa.
//! abbba
//! .aaa.
//!
//! .....
//! ..a..
//! .....
//! """
//!
//! [part.shade]              # or the parts of a figure, each its own box,
//! at = [0, 0, 38]           # in the order they are written
//! parent = "foot"           # only when set by hand
//! joint = [3, 3, 40]        # only when set by hand, from the model's low corner
//! layers = """ ... """
//! ```
//!
//! The layers run from the bottom (z = 0) up, an empty line between; in a
//! layer a line is a row, the far row (highest y) first, so a layer reads as
//! the model seen from above; a row's x runs left to right. `.` is empty.
//! A voxel's sign is one character (a-z, A-Z, 0-9 in order of first use) when
//! the palette has at most 62 entries, else two for the whole file (`..`
//! empty). A cell holds one voxel: a second voxel in a cell goes into an
//! `also` table (`[[frame.also]]`, `[[part.NAME.also]]`) with its own `at`
//! and layers.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use glam::{IVec3, Vec3};
use toml_edit::{Document, Item, TableLike};

use crate::vox::{Hung, VoxModel, metal};
use crate::world::CellSet;

/// What a voxel is made of besides its place: its colour (alpha under 255
/// is glass), how it mirrors (`metal::pack`) and how much it glows. Two
/// voxels of the same colour and another material are two entries.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Mat {
    rgba: [u8; 4],
    metal: u8,
    glow: u8,
}

const SIGNS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// How many entries a palette may have for one sign a cell.
const ONE_SIGN: usize = SIGNS.len();
const EMPTY: u32 = u32::MAX;

// ---------------------------------------------------------------- writing

/// The material of voxel `k` of frame `f`.
fn mat_of(model: &VoxModel, f: usize, k: usize) -> Mat {
    let rgba = model.frames[f][k].1;
    Mat { rgba, metal: model.metal.get(f).and_then(|m| m.get(k)).copied().unwrap_or(0), glow: model.glow.get(f).and_then(|g| g.get(k)).copied().unwrap_or(0) }
}

/// How many entries a palette of the model's text has (colours with their
/// materials, as used).
pub fn entries(model: &VoxModel) -> usize {
    let mut seen = BTreeSet::new();
    for (f, frame) in model.frames.iter().enumerate() {
        for k in 0..frame.len() {
            let m = mat_of(model, f, k);
            seen.insert((m.rgba, m.metal, m.glow));
        }
    }
    seen.len()
}

/// A rectangle of voxels: the layers of one `layers` string. Cells hold an
/// index into the material list, or `EMPTY`.
struct Pass {
    at: IVec3,
    dim: IVec3,
    cells: Vec<u32>,
}

/// The voxels (place, material) as passes: the first voxel of every cell in
/// the first, a second one in the same cell in the next, and so on. Each is
/// as small as its voxels allow.
fn passes(voxels: &[(IVec3, u32)]) -> Vec<Pass> {
    let mut rest = voxels.to_vec();
    let mut out = Vec::new();
    while !rest.is_empty() {
        let mut taken = CellSet::default();
        let (mut now, mut later) = (Vec::new(), Vec::new());
        for &(p, m) in &rest {
            if taken.insert(p) {
                now.push((p, m));
            } else {
                later.push((p, m));
            }
        }
        let low = now.iter().fold(IVec3::MAX, |l, v| l.min(v.0));
        let high = now.iter().fold(IVec3::MIN, |h, v| h.max(v.0));
        let dim = high - low + IVec3::ONE;
        let mut cells = vec![EMPTY; (dim.x * dim.y * dim.z) as usize];
        for (p, m) in now {
            let c = p - low;
            cells[((c.z * dim.y + c.y) * dim.x + c.x) as usize] = m;
        }
        out.push(Pass { at: low, dim, cells });
        rest = later;
    }
    out
}

/// The signs given out in the order the text first uses them.
struct Signs {
    width: usize,
    of: HashMap<u32, String>,
    order: Vec<u32>,
}

impl Signs {
    fn sign(&mut self, mat: u32) -> String {
        if let Some(s) = self.of.get(&mat) {
            return s.clone();
        }
        let i = self.order.len();
        let s = if self.width == 1 { (SIGNS[i] as char).to_string() } else { format!("{}{}", SIGNS[i / ONE_SIGN] as char, SIGNS[i % ONE_SIGN] as char) };
        self.of.insert(mat, s.clone());
        self.order.push(mat);
        s
    }
}

/// `at = [...]` (when not the origin) and the layers of a pass, as the
/// lines of a table.
fn pass_lines(pass: &Pass, signs: &mut Signs) -> String {
    let mut s = String::new();
    if pass.at != IVec3::ZERO {
        s += &format!("at = [{}, {}, {}]\n", pass.at.x, pass.at.y, pass.at.z);
    }
    s += "layers = \"\"\"\n";
    let dots = ".".repeat(signs.width);
    for z in 0..pass.dim.z {
        if z > 0 {
            s.push('\n');
        }
        for y in (0..pass.dim.y).rev() {
            for x in 0..pass.dim.x {
                match pass.cells[((z * pass.dim.y + y) * pass.dim.x + x) as usize] {
                    EMPTY => s += &dots,
                    m => s += &signs.sign(m),
                }
            }
            s.push('\n');
        }
    }
    s += "\"\"\"\n";
    s
}

/// A table's body under its `header`: its first pass in it, the others as
/// `also` tables of the table called `name`.
fn block_text(header: &str, name: &str, extra: &str, voxels: &[(IVec3, u32)], signs: &mut Signs) -> String {
    let mut out = format!("{header}\n{extra}");
    let all = passes(voxels);
    if let Some(first) = all.first() {
        out += &pass_lines(first, signs);
    }
    for pass in all.iter().skip(1) {
        out += &format!("\n[[{name}.also]]\n{}", pass_lines(pass, signs));
    }
    out
}

/// A key as TOML writes it: bare when it can be, else quoted.
fn key(name: &str) -> String {
    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    }
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
fn alpha_of(glass: f32) -> u8 {
    ((1.0 - glass).clamp(0.05, 1.0) * 255.0).round() as u8
}

fn glow_of(glow: f32) -> u8 {
    (glow.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn joint_text(v: Vec3) -> String {
    format!("[{}, {}, {}]", v.x, v.y, v.z)
}

/// The text of a model: frames, or the parts of a figure.
pub fn write(model: &VoxModel) -> String {
    let n = entries(model);
    let mut mats: Vec<Mat> = Vec::new();
    let mut index: HashMap<Mat, u32> = HashMap::new();
    let mut voxels_of = |f: usize, ks: &mut dyn Iterator<Item = usize>| -> Vec<(IVec3, u32)> {
        ks.map(|k| {
            let m = mat_of(model, f, k);
            let id = *index.entry(m).or_insert_with(|| {
                mats.push(m);
                mats.len() as u32 - 1
            });
            (model.frames[f][k].0, id)
        })
        .collect()
    };
    let mut signs = Signs { width: if n <= ONE_SIGN { 1 } else { 2 }, of: HashMap::new(), order: Vec::new() };
    let mut body = String::new();
    if model.parts.is_empty() {
        for f in 0..model.frames.len() {
            let v = voxels_of(f, &mut (0..model.frames[f].len()));
            body += "\n";
            body += &block_text("[[frame]]", "frame", "", &v, &mut signs);
        }
    } else {
        let pivot = IVec3::new(model.size.x / 2, model.size.y / 2, 0).as_vec3();
        for p in &model.parts {
            let v = voxels_of(0, &mut p.voxels.iter().map(|&k| k as usize));
            let mut extra = String::new();
            if p.hung == Hung::ByHand
                && let Some(q) = p.parent
            {
                extra += &format!("parent = \"{}\"\n", model.parts[q].name.replace('\\', "\\\\").replace('"', "\\\""));
            }
            if p.joint_by_hand {
                extra += &format!("joint = {}\n", joint_text(p.joint + pivot));
            }
            body += "\n";
            body += &block_text(&format!("[part.{}]", key(&p.name)), &format!("part.{}", key(&p.name)), &extra, &v, &mut signs);
        }
    }
    let mut out = format!("size = [{}, {}, {}]\n\n[palette]\n", model.size.x, model.size.y, model.size.z);
    for &id in &signs.order {
        out += &format!("{} = {}\n", signs.of[&id], material_text(&mats[id as usize]));
    }
    out + &body
}

// ---------------------------------------------------------------- reading

/// The line of a byte of the source (1 on).
fn line_of(src: &str, at: usize) -> usize {
    src[..at.min(src.len())].matches('\n').count() + 1
}

struct Reader<'a> {
    src: &'a str,
}

type Span = Option<std::ops::Range<usize>>;
/// What a part's table says of its rig: the parent's name (and where), the joint.
type Rig = (Option<(String, Span)>, Option<Vec3>);

impl Reader<'_> {
    /// A mistake on the line where `span` begins.
    fn fail(&self, span: Span, why: impl std::fmt::Display) -> anyhow::Error {
        match span {
            Some(s) => anyhow!("line {}: {why}", line_of(self.src, s.start)),
            None => anyhow!("{why}"),
        }
    }

    fn number(&self, item: &Item, what: &str) -> Result<f32> {
        item.as_float().or_else(|| item.as_integer().map(|i| i as f64)).filter(|v| v.is_finite()).map(|v| v as f32).ok_or_else(|| self.fail(item.span(), format!("{what} must be a number")))
    }

    fn fraction(&self, item: &Item, what: &str) -> Result<f32> {
        let v = self.number(item, what)?;
        if !(0.0..=1.0).contains(&v) {
            return Err(self.fail(item.span(), format!("{what} must be between 0 and 1, not {v}")));
        }
        Ok(v)
    }

    fn triple(&self, item: &Item, what: &str) -> Result<[f32; 3]> {
        let a = item.as_array().filter(|a| a.len() == 3).ok_or_else(|| self.fail(item.span(), format!("{what} must be three numbers, like [1, 2, 3]")))?;
        let mut out = [0.0; 3];
        for (o, v) in out.iter_mut().zip(a.iter()) {
            *o = v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).filter(|v| v.is_finite()).ok_or_else(|| self.fail(item.span(), format!("{what} must be three numbers, like [1, 2, 3]")))? as f32;
        }
        Ok(out)
    }

    fn ints(&self, item: &Item, what: &str) -> Result<IVec3> {
        let t = self.triple(item, what)?;
        if t.iter().any(|v| v.fract() != 0.0) {
            return Err(self.fail(item.span(), format!("{what} must be three whole numbers, like [1, 2, 3]")));
        }
        Ok(IVec3::new(t[0] as i32, t[1] as i32, t[2] as i32))
    }

    /// Only the fields `allowed` may be in `table`, `what` named in the
    /// mistake.
    fn only(&self, table: &dyn TableLike, allowed: &[&str], what: &str) -> Result<()> {
        for (k, v) in table.iter() {
            if !allowed.contains(&k) {
                return Err(self.fail(v.span(), format!("'{k}' is no field of {what} (it has {})", allowed.join(", "))));
            }
        }
        Ok(())
    }

    fn material(&self, sign: &str, item: &Item) -> Result<Mat> {
        let hex = |text: &str, at: &Item| -> Result<[u8; 3]> {
            let h = text.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()));
            let h = h.ok_or_else(|| self.fail(at.span(), format!("'{text}' is no colour: write it as #rrggbb")))?;
            let v = u32::from_str_radix(h, 16).unwrap_or(0);
            Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
        };
        if let Some(text) = item.as_str() {
            let [r, g, b] = hex(text, item)?;
            return Ok(Mat { rgba: [r, g, b, 255], metal: 0, glow: 0 });
        }
        let table = item.as_table_like().ok_or_else(|| self.fail(item.span(), format!("the palette's '{sign}' must be a colour like \"#7a4a2a\" or a table like {{ color = \"#7a4a2a\", glow = 1.0 }}")))?;
        self.only(table, &["color", "glass", "metal", "rough", "glow"], &format!("the palette's '{sign}'"))?;
        let colour = table.get("color").ok_or_else(|| self.fail(item.span(), format!("the palette's '{sign}' has no color")))?;
        let [r, g, b] = hex(colour.as_str().ok_or_else(|| self.fail(colour.span(), "color must be text like \"#7a4a2a\""))?, colour)?;
        let alpha = match table.get("glass") {
            Some(g) => alpha_of(self.fraction(g, "glass")?),
            None => 255,
        };
        let (m, r2) = (table.get("metal"), table.get("rough"));
        let packed = if m.is_none() && r2.is_none() {
            0
        } else {
            let metalness = m.map_or(Ok(0.0), |m| self.fraction(m, "metal"))?;
            let rough = r2.map_or(Ok(0.3), |r| self.fraction(r, "rough"))?;
            metal::pack(metalness, 1.0 - rough)
        };
        let glow = table.get("glow").map_or(Ok(0), |g| self.fraction(g, "glow").map(glow_of))?;
        Ok(Mat { rgba: [r, g, b, alpha], metal: packed, glow })
    }

    /// The voxels of one `layers` string (place from the box's low corner,
    /// then moved by `at`): the layers' rows are checked to be rectangular.
    fn layers(&self, item: &Item, palette: &HashMap<String, Mat>, width: usize, at: IVec3) -> Result<Vec<(IVec3, Mat)>> {
        let text = item.as_str().ok_or_else(|| self.fail(item.span(), "layers must be a text between \"\"\" marks"))?;
        let span = item.span();
        // The first line of the text in the file: a newline straight after
        // the opening marks is not part of it.
        let first = span.as_ref().map_or(0, |s| {
            let raw = &self.src[s.clone()];
            let after = raw.strip_prefix("\"\"\"").or_else(|| raw.strip_prefix("'''"));
            line_of(self.src, s.start) + usize::from(after.is_some_and(|a| a.starts_with('\n') || a.starts_with("\r\n")))
        });
        let line = |i: usize| (first > 0).then_some(format!("line {}: ", first + i)).unwrap_or_default();
        let mut layers: Vec<Vec<(usize, &str)>> = vec![Vec::new()];
        for (i, row) in text.lines().enumerate() {
            let row = row.trim();
            if row.is_empty() {
                if !layers.last().is_some_and(Vec::is_empty) {
                    layers.push(Vec::new());
                }
            } else {
                layers.last_mut().unwrap().push((i, row));
            }
        }
        if layers.last().is_some_and(Vec::is_empty) {
            layers.pop();
        }
        let mut out = Vec::new();
        let (mut cols, mut rows) = (None, None);
        let known = || palette.keys().cloned().collect::<Vec<_>>().join(", ");
        for (z, layer) in layers.iter().enumerate() {
            let depth = layer.len();
            if *rows.get_or_insert(depth) != depth {
                bail!("{}this layer has {depth} rows, the first has {}: every layer must be as high as the first", line(layer[0].0), rows.unwrap());
            }
            for (r, &(i, row)) in layer.iter().enumerate() {
                let chars: Vec<char> = row.chars().collect();
                if !chars.len().is_multiple_of(width) {
                    bail!("{}a row is {} characters, but every cell is {width} (the palette's signs are that long)", line(i), chars.len());
                }
                let n = chars.len() / width;
                if *cols.get_or_insert(n) != n {
                    bail!("{}this row has {n} cells, the first has {}: every row must be as long as the first", line(i), cols.unwrap());
                }
                for x in 0..n {
                    let sign: String = chars[x * width..(x + 1) * width].iter().collect();
                    if sign.chars().all(|c| c == '.') {
                        continue;
                    }
                    let mat = palette.get(&sign).ok_or_else(|| anyhow!("{}'{sign}' is no sign of the palette (it has {})", line(i), known()))?;
                    out.push((at + IVec3::new(x as i32, (depth - 1 - r) as i32, z as i32), *mat));
                }
            }
        }
        Ok(out)
    }

    /// The voxels of a frame's or a part's table: its own layers, then its
    /// `also` tables'.
    fn block(&self, table: &dyn TableLike, palette: &HashMap<String, Mat>, width: usize, with_rig: bool, what: &str) -> Result<Vec<(IVec3, Mat)>> {
        let mut out = Vec::new();
        let mut one = |t: &dyn TableLike, allowed: &[&str], what: &str| -> Result<()> {
            self.only(t, allowed, what)?;
            let at = t.get("at").map_or(Ok(IVec3::ZERO), |a| self.ints(a, "at"))?;
            if let Some(l) = t.get("layers") {
                out.extend(self.layers(l, palette, width, at)?);
            }
            Ok(())
        };
        let fields: &[&str] = if with_rig { &["at", "layers", "also", "parent", "joint"] } else { &["at", "layers", "also"] };
        one(table, fields, what)?;
        if let Some(also) = table.get("also") {
            let list = tables(also).ok_or_else(|| self.fail(also.span(), "also must be tables: [[frame.also]] or [[part.NAME.also]]"))?;
            for t in list {
                one(t, &["at", "layers"], &format!("{what}'s also"))?;
            }
        }
        Ok(out)
    }
}

/// The tables of an array of tables (`[[x]]`, or `[{...}, {...}]`).
fn tables(item: &Item) -> Option<Vec<&dyn TableLike>> {
    if let Some(a) = item.as_array_of_tables() {
        return Some(a.iter().map(|t| t as &dyn TableLike).collect());
    }
    item.as_array()?.iter().map(|v| v.as_inline_table().map(|t| t as &dyn TableLike)).collect()
}

/// The model the text says, as the engine makes it of a `.vox` (the parts'
/// parents and joints worked out as it does, but for those the text sets).
pub fn read(src: &str) -> Result<VoxModel> {
    let doc = Document::parse(src).map_err(|e| match e.span() {
        Some(s) => anyhow!("line {}: {}", line_of(src, s.start), e.message()),
        None => anyhow!("{}", e.message()),
    })?;
    let r = Reader { src };
    let top = doc.as_table();
    r.only(top, &["size", "palette", "frame", "part"], "a model")?;
    let size_item = top.get("size").ok_or_else(|| anyhow!("line 1: no size = [x, y, z] (the model's box, in voxels)"))?;
    let size = r.ints(size_item, "size")?;
    let palette_item = top.get("palette").ok_or_else(|| anyhow!("no [palette]: the signs and their colours"))?;
    let table = palette_item.as_table_like().ok_or_else(|| r.fail(palette_item.span(), "palette must be a table of signs"))?;
    let mut palette = HashMap::new();
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
        palette.insert(sign.to_string(), r.material(sign, item)?);
    }
    let width = width.max(1);
    let (frames, parts) = (top.get("frame"), top.get("part"));
    let mut model = match (frames, parts) {
        (Some(f), None) => {
            let list = tables(f).ok_or_else(|| r.fail(f.span(), "frame must be tables: [[frame]]"))?;
            let (mut frames, mut metals, mut glows) = (Vec::new(), Vec::new(), Vec::new());
            for t in list {
                let voxels = r.block(t, &palette, width, false, "a frame")?;
                frames.push(voxels.iter().map(|(p, m)| (*p, m.rgba)).collect());
                metals.push(voxels.iter().map(|(_, m)| m.metal).collect());
                glows.push(voxels.iter().map(|(_, m)| m.glow).collect::<Vec<u8>>());
            }
            let mut model = VoxModel::new(size, frames, metals);
            if glows.iter().flatten().any(|&g| g != 0) {
                model.glow = glows;
            }
            model
        }
        (None, Some(p)) => {
            let list = p.as_table_like().ok_or_else(|| r.fail(p.span(), "part must be tables: [part.NAME]"))?;
            let (mut names, mut owner, mut frame, mut metals, mut glows, mut stated) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
            let mut rigs: Vec<Rig> = Vec::new();
            for (name, item) in list.iter() {
                let t = item.as_table_like().ok_or_else(|| r.fail(item.span(), format!("part '{name}' must be a table: [part.{}]", key(name))))?;
                let voxels = r.block(t, &palette, width, true, &format!("part '{name}'"))?;
                let parent = match t.get("parent") {
                    None => None,
                    Some(i) => Some((i.as_str().ok_or_else(|| r.fail(i.span(), "parent must be the name of a part"))?.to_string(), i.span())),
                };
                let joint = match t.get("joint") {
                    None => None,
                    Some(i) => Some(Vec3::from(r.triple(i, "joint")?)),
                };
                rigs.push((parent, joint));
                for (v, m) in voxels {
                    owner.push(names.len());
                    frame.push((v, m.rgba));
                    metals.push(m.metal);
                    glows.push(m.glow);
                }
                names.push(name.to_string());
            }
            for (parent, joint) in &rigs {
                let found = match parent {
                    None => None,
                    Some((p, span)) => Some(names.iter().position(|n| n == p).ok_or_else(|| r.fail(span.clone(), format!("no part is called '{p}' (the parts are {})", names.join(", "))))?),
                };
                stated.push((found, *joint));
            }
            let parents: Vec<Option<usize>> = stated.iter().map(|s| s.0).collect();
            if let Some(chain) = (0..names.len()).find_map(|p| crate::vox::ring(&parents, p)) {
                let named: Vec<&str> = chain.iter().map(|&p| names[p].as_str()).collect();
                bail!("part '{}' hangs on itself by way of the others ({}): one of them must hang on another part, or on none", named[0], named.join(" on "));
            }
            let mut model = VoxModel::new(size, vec![frame.clone()], vec![metals]);
            if glows.iter().any(|&g| g != 0) {
                model.glow = vec![glows];
            }
            model.parts = crate::vox::parts_of(&frame, &owner, names, size, &stated);
            model
        }
        (Some(f), Some(_)) => return Err(r.fail(f.span(), "a model is frames ([[frame]]) or the parts of a figure ([part.NAME]), not both")),
        (None, None) => bail!("the model has no [[frame]] and no [part.NAME]"),
    };
    model.size = size;
    Ok(model)
}

// ------------------------------------------------------------- comparing

type Bag = Vec<(i32, i32, i32, [u8; 4], u8, u8)>;

/// The voxels `ks` of frame `f` as a sorted list (place, colour, metal,
/// glow): two models hold the same when these are equal.
fn bag(model: &VoxModel, f: usize, ks: impl Iterator<Item = usize>) -> Bag {
    let mut v: Bag = ks
        .map(|k| {
            let ((p, c), m) = (model.frames[f][k], mat_of(model, f, k));
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
    Some(format!("{} only in the .vox{}, {} only in the text{}", only_a.len(), only_a.first().map_or(String::new(), |v| format!(" (e.g. {v:?})")), only_b.len(), only_b.first().map_or(String::new(), |v| format!(" (e.g. {v:?})"))))
}

/// Every way two models differ in what the engine uses (sizes, frames'
/// voxels as sets with their colours and materials, the parts' names,
/// voxels, parents, joints and overlaps); empty: the same.
pub fn differences(a: &VoxModel, b: &VoxModel) -> Vec<String> {
    let mut out = Vec::new();
    if a.size != b.size {
        out.push(format!("size {} vs {}", a.size, b.size));
    }
    if a.frames.len() != b.frames.len() {
        out.push(format!("{} frames vs {}", a.frames.len(), b.frames.len()));
    }
    for f in 0..a.frames.len().min(b.frames.len()) {
        if let Some(d) = bag_diff(&bag(a, f, 0..a.frames[f].len()), &bag(b, f, 0..b.frames[f].len())) {
            out.push(format!("frame {f}: {d}"));
        }
    }
    if a.parts.len() != b.parts.len() {
        out.push(format!("{} parts vs {}", a.parts.len(), b.parts.len()));
    }
    let name = |m: &VoxModel, p: Option<usize>| p.map(|q| m.parts[q].name.clone());
    for (i, (p, q)) in a.parts.iter().zip(&b.parts).enumerate() {
        let label = format!("part {i} '{}'", p.name);
        if p.name != q.name {
            out.push(format!("{label}: called '{}' in the text", q.name));
            continue;
        }
        if let Some(d) = bag_diff(&bag(a, 0, p.voxels.iter().map(|&k| k as usize)), &bag(b, 0, q.voxels.iter().map(|&k| k as usize))) {
            out.push(format!("{label}: voxels: {d}"));
        }
        if name(a, p.parent) != name(b, q.parent) {
            out.push(format!("{label}: hangs on {:?} in the .vox, {:?} from the text", name(a, p.parent), name(b, q.parent)));
        }
        if p.joint != q.joint {
            out.push(format!("{label}: joint {} in the .vox, {} from the text", p.joint, q.joint));
        }
        if p.joint_by_hand != q.joint_by_hand {
            out.push(format!("{label}: joint by hand {} vs {}", p.joint_by_hand, q.joint_by_hand));
        }
        if (p.hung == Hung::ByHand) != (q.hung == Hung::ByHand) {
            out.push(format!("{label}: parent by hand {} vs {}", p.hung == Hung::ByHand, q.hung == Hung::ByHand));
        }
        if p.overlaps != q.overlaps {
            out.push(format!("{label}: overlaps {:?} vs {:?}", p.overlaps, q.overlaps));
        }
    }
    out
}

// -------------------------------------------------------------- the trial

/// The `.vox` files under the folders, sorted.
fn find_vox(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            find_vox(&p, out);
        } else if p.extension().is_some_and(|e| e == "vox") {
            out.push(p);
        }
    }
}

fn gzipped(bytes: &[u8]) -> usize {
    use std::io::Write;
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(bytes).ok();
    z.finish().map_or(0, |v| v.len())
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

/// `voxcade model-trial DIR... [--out DIR]`: every `.vox` under the folders
/// written as text, read back and compared with the `.vox` as the engine
/// loads it; the sizes and load times measured.
pub fn trial(args: &[String]) -> Result<()> {
    let (mut dirs, mut out_dir) = (Vec::new(), None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out_dir = Some(PathBuf::from(it.next().context("--out needs a folder")?)),
            _ => dirs.push(PathBuf::from(a)),
        }
    }
    if dirs.is_empty() {
        bail!("voxcade model-trial DIR... [--out DIR]");
    }
    let mut files = Vec::new();
    for d in &dirs {
        find_vox(d, &mut files);
    }
    let scratch = out_dir.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("voxcade-model-trial-{}", std::process::id())));
    std::fs::create_dir_all(&scratch)?;
    struct Row {
        text: PathBuf,
        vox_bytes: usize,
        text_bytes: usize,
        vox_gz: usize,
        text_gz: usize,
        entries: usize,
    }
    let (mut rows, mut same, mut different, mut unreadable) = (Vec::new(), 0, 0, 0);
    for (n, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path)?;
        let model = match crate::vox::load_model(path) {
            Ok(m) => m,
            Err(e) => {
                unreadable += 1;
                println!("UNREADABLE {}: {e}", path.display());
                continue;
            }
        };
        let text = write(&model);
        let mut problems = Vec::new();
        match read(&text) {
            Ok(back) => {
                problems = differences(&model, &back);
                if write(&back) != text {
                    problems.push("the text changes when written again".to_string());
                }
            }
            Err(e) => problems.push(format!("the text does not read back: {e}")),
        }
        let flat: String = path.to_string_lossy().chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
        let text_path = scratch.join(format!("{n:03}_{}.model.toml", flat.trim_start_matches('_')));
        std::fs::write(&text_path, &text)?;
        let two = if entries(&model) > ONE_SIGN { ", two signs a cell" } else { "" };
        if problems.is_empty() {
            same += 1;
            println!("same  {}  ({} B .vox, {} B text{two})", path.display(), bytes.len(), text.len());
        } else {
            different += 1;
            println!("DIFF  {}", path.display());
            for p in &problems {
                println!("        {p}");
            }
        }
        rows.push(Row { text: text_path, vox_bytes: bytes.len(), text_bytes: text.len(), vox_gz: gzipped(&bytes), text_gz: gzipped(text.as_bytes()), entries: entries(&model) });
    }
    println!("\n{} files: {same} the same, {different} different, {unreadable} unreadable", files.len());
    let sum = |f: &dyn Fn(&Row) -> usize, of: &[&Row]| of.iter().map(|r| f(r)).sum::<usize>();
    let all: Vec<&Row> = rows.iter().collect();
    let mut by_size: Vec<&Row> = rows.iter().collect();
    by_size.sort_by_key(|r| std::cmp::Reverse(r.vox_bytes));
    let five = &by_size[..by_size.len().min(5)];
    for (label, of) in [("all files", &all[..]), ("the five biggest", five)] {
        println!(
            "{label}: .vox {} B (gzip {} B), text {} B (gzip {} B)",
            sum(&|r| r.vox_bytes, of),
            sum(&|r| r.vox_gz, of),
            sum(&|r| r.text_bytes, of),
            sum(&|r| r.text_gz, of)
        );
    }
    let most = rows.iter().map(|r| r.entries).max().unwrap_or(0);
    println!("most palette entries in a file: {most}; files needing two signs a cell: {}", rows.iter().filter(|r| r.entries > ONE_SIGN).count());
    // Load times: the whole set, five runs, the median.
    let (mut vox_runs, mut text_runs) = (Vec::new(), Vec::new());
    for _ in 0..5 {
        let t = Instant::now();
        for p in &files {
            std::hint::black_box(crate::vox::load_model(p).ok());
        }
        vox_runs.push(t.elapsed());
        let t = Instant::now();
        for r in &rows {
            let text = std::fs::read_to_string(&r.text)?;
            std::hint::black_box(read(&text).ok());
        }
        text_runs.push(t.elapsed());
    }
    println!("load all (median of 5): .vox {:.1} ms, text {:.1} ms", median(vox_runs).as_secs_f64() * 1000.0, median(text_runs).as_secs_f64() * 1000.0);
    if out_dir.is_none() {
        std::fs::remove_dir_all(&scratch).ok();
    }
    Ok(())
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
        let text = write(m);
        let back = read(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        same(m, &back);
        assert_eq!(write(&back), text);
        (text, back)
    }

    const FRAMES: &str = "size = [3, 2, 2]\n[palette]\na = \"#ff0000\"\nb = \"#00ff00\"\n[[frame]]\nlayers = \"\"\"\naa.\nb..\n\n...\n..b\n\"\"\"\n[[frame]]\nat = [1, 0, 0]\nlayers = \"\"\"\nb\n\"\"\"\n";

    #[test]
    fn the_far_row_comes_first_and_frames_are_placed_by_at() {
        let m = read(FRAMES).unwrap();
        assert_eq!(m.size, IVec3::new(3, 2, 2));
        assert_eq!(m.frames.len(), 2);
        let at = |f: usize, p: IVec3| m.frames[f].iter().find(|v| v.0 == p).map(|v| v.1);
        // The first row of the bottom layer is y 1, the far one.
        assert_eq!(at(0, IVec3::new(0, 1, 0)), Some([255, 0, 0, 255]));
        assert_eq!(at(0, IVec3::new(0, 0, 0)), Some([0, 255, 0, 255]));
        assert_eq!(at(0, IVec3::new(2, 0, 1)), Some([0, 255, 0, 255]));
        assert_eq!(m.frames[0].len(), 4);
        assert_eq!(m.frames[1], vec![(IVec3::new(1, 0, 0), [0, 255, 0, 255])]);
        round_trip(&m);
    }

    #[test]
    fn a_figure_keeps_the_parent_and_joint_set_by_hand() {
        let text = "size = [2, 1, 3]\n[palette]\na = \"#888888\"\n[part.foot]\nlayers = \"\"\"\naa\n\"\"\"\n[part.leg]\nat = [0, 0, 1]\nparent = \"foot\"\njoint = [0.5, 0, 1.5]\nlayers = \"\"\"\na.\n\"\"\"\n[part.\"a hat\"]\nat = [0, 0, 2]\nlayers = \"\"\"\na.\n\"\"\"\n";
        let m = read(text).unwrap();
        assert_eq!(m.parts.len(), 3);
        assert_eq!(m.parts[1].parent, Some(0));
        assert!(m.parts[1].joint_by_hand && m.parts[1].hung == Hung::ByHand);
        assert!(!m.parts[2].joint_by_hand);
        let (written, _) = round_trip(&m);
        assert!(written.contains("parent = \"foot\"") && written.contains("joint = [0.5, 0, 1.5]") && written.contains("[part.\"a hat\"]"), "{written}");
        // Only what was set by hand is written.
        assert_eq!(written.matches("joint =").count(), 1);
    }

    #[test]
    fn glass_metal_and_glow_are_carried() {
        let text = "size = [4, 1, 1]\n[palette]\na = \"#102030\"\ng = { color = \"#88ccff\", glass = 0.5, rough = 0.1 }\nm = { color = \"#c0c0c0\", metal = 1.0, rough = 0.3 }\nb = { color = \"#102030\", glow = 1.0 }\n[[frame]]\nlayers = \"\"\"\nagmb\n\"\"\"\n";
        let m = read(text).unwrap();
        let f = &m.frames[0];
        assert_eq!(f[1].1[3], 128);
        assert_eq!(m.metal[0][1], metal::pack(0.0, 0.9));
        assert_eq!(m.metal[0][2], metal::pack(1.0, 0.7));
        assert_eq!(m.glow[0], vec![0, 0, 0, 255]);
        // The same colour as another material is another sign.
        let (written, _) = round_trip(&m);
        assert!(written.contains("glass = 0.5") && written.contains("metal = 1.0") && written.contains("glow = 1.0"), "{written}");
        assert_eq!(written.lines().filter(|l| l.contains("#102030")).count(), 2);
    }

    #[test]
    fn overlapping_parts_and_a_voxel_twice_in_a_cell_survive() {
        let text = "size = [2, 1, 1]\n[palette]\na = \"#ff0000\"\nb = \"#0000ff\"\n[part.one]\nlayers = \"\"\"\naa\n\"\"\"\n[part.two]\nat = [1, 0, 0]\nlayers = \"\"\"\nb\n\"\"\"\n";
        let m = read(text).unwrap();
        assert_eq!(m.frames[0].len(), 3);
        assert_eq!(m.parts[0].overlaps, vec![(1, 1)]);
        round_trip(&m);
        // Two voxels in one cell of a frame, and one outside the box.
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];
        let frame = vec![(IVec3::new(0, 0, 0), red), (IVec3::new(0, 0, 0), blue), (IVec3::new(5, 0, 0), red)];
        let m = VoxModel::new(IVec3::new(2, 1, 1), vec![frame], vec![vec![0; 3]]);
        let (written, back) = round_trip(&m);
        assert!(written.contains("[[frame.also]]"), "{written}");
        assert_eq!(back.frames[0].len(), 3);
    }

    #[test]
    fn more_than_62_colours_take_two_signs_a_cell() {
        let frame: Vec<_> = (0..100).map(|i| (IVec3::new(i % 10, i / 10, 0), [i as u8 * 2, 3, 4, 255])).collect();
        let m = VoxModel::new(IVec3::new(10, 10, 1), vec![frame], vec![vec![0; 100]]);
        assert_eq!(entries(&m), 100);
        let (text, _) = round_trip(&m);
        assert!(text.contains("\naa = \"#"), "{text}");
        let few: Vec<_> = (0..62).map(|i| (IVec3::new(i, 0, 0), [i as u8, 3, 4, 255])).collect();
        let m = VoxModel::new(IVec3::new(62, 1, 1), vec![few], vec![vec![0; 62]]);
        assert!(write(&m).contains("\nZ = \"#"));
    }

    #[test]
    fn mistakes_say_the_line_and_what_is_wrong() {
        let body = |layers: &str| format!("size = [2, 1, 1]\n[palette]\na = \"#ff0000\"\n[[frame]]\nlayers = \"\"\"\n{layers}\"\"\"\n");
        let why = |text: String| read(&text).err().map(|e| e.to_string()).unwrap();
        // The layers begin on line 6.
        let e = why(body("a.\nax\n"));
        assert!(e.starts_with("line 7:") && e.contains("'x' is no sign of the palette"), "{e}");
        let e = why(body("a.\na\n"));
        assert!(e.starts_with("line 7:") && e.contains("1 cells, the first has 2"), "{e}");
        assert!(why("[palette]\na = \"#ff0000\"\n".into()).contains("no size"));
        assert!(why("size = [1, 1, 1]\n[palette]\na = \"red\"\n".into()).starts_with("line 3:"));
        let e = why("size = [1, 1, 1]\n[palette]\na = \"#ff0000\"\n[part.x]\nparent = \"y\"\nlayers = \"\"\"\na\n\"\"\"\n".into());
        assert!(e.starts_with("line 5:") && e.contains("no part is called 'y'"), "{e}");
        let e = why("size = [1, 1, 1]\n[palette]\na = { color = \"#ff0000\", shiny = 1 }\n".into());
        assert!(e.contains("'shiny' is no field"), "{e}");
    }
}

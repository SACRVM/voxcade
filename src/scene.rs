//! A world or a map's prefab as text: a `.vvx` (`crate::vvx`, "voxcade
//! voxel 1") of one frame, whose palette's colours are the world's numbers
//! and whose markers place things.
//!
//! ```toml
//! format = "voxcade voxel 1"
//! size = [512, 512, 48]
//!
//! [palette]
//! a = "#1d1b24"                # the world's colour 1
//! b = "#4a3426"                # 2, and so on in the palette's order
//! r = { marker = "rifleman" }  # a thing of this kind stands here
//!
//! [[frame]]
//! layers = """ ... """
//! ```
//!
//! **Numbers.** A world's voxels are numbers into its palette: the state,
//! the replays and the save games keep them. The n-th colour of the
//! palette is number n (markers are not counted); a colour no voxel has
//! keeps its number all the same, so the palette's order is the meaning,
//! never the order of the voxels. A prefab's colours become the map's
//! numbers when the map is put together, the prefabs in the order of their
//! names, each one's colours in its palette's order (`map`).
//!
//! **Markers.** `{ marker = "rifleman" }` places a thing of that kind (a
//! name of `c.things`) on its cell instead of a voxel. The things come in
//! the order of the palette's marker entries, each entry's cells in
//! reading order; two entries may name the same kind (that order is then
//! why).
//!
//! The colours are plain (a world's voxels have no glass, metal or glow),
//! and the layers read as a model's do: bottom layer first, the far row
//! first in a layer, x left to right, `.` empty. The box is `size`, the
//! frame's `at` measured from the pivot as in a model (a prefab stands on
//! its pivot, the cell `size / 2` across and deep on the floor).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use glam::IVec3;
use toml_edit::Item;

use crate::files::Files;
use crate::vvx::{self, ONE_SIGN, Reader};
use crate::world::{Marker, World};

/// A world or a prefab as its file holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// Its box: voxels lie in 0..size.
    pub size: IVec3,
    /// The colours in the palette's order: colour `n` (from 1) is
    /// `colours[n - 1]`.
    pub colours: Vec<[u8; 3]>,
    /// The kinds the marker entries name, in the palette's order.
    pub markers: Vec<String>,
    /// Every cell of the box, x fastest, then y, then z: 0 empty, 1 to
    /// `colours.len()` a colour's number, above that a marker
    /// (`colours.len() + 1 + its place in markers`).
    pub cells: Vec<u16>,
}

/// The most voxels a world or a prefab's box may hold (1024 by 1024 by
/// 128): every cell of it is kept, two bytes each. The Arena's world is
/// 512 by 512 by 48; a bigger land is a map (`crate::map`) or a level.
pub const MOST_VOXELS: i64 = 1 << 27;

/// The mistake for a world or a prefab `size` big, None when it may be
/// read (`MOST_VOXELS`, and what any world may be, `world::too_big`).
pub fn too_big(size: IVec3) -> Option<String> {
    let volume = size.max(IVec3::ZERO).as_i64vec3().element_product();
    if volume > MOST_VOXELS {
        return Some(format!("the box is too big: {} by {} by {} is {volume} voxels, a world or a prefab holds {MOST_VOXELS} at most (1024 by 1024 by 128; a bigger land is a map or a level)", size.x, size.y, size.z));
    }
    crate::world::too_big(size.as_i64vec3())
}

impl Scene {
    /// An empty scene `size` big: a size from a file is checked with
    /// `too_big` first.
    pub fn new(size: IVec3) -> Self {
        let volume = size.max(IVec3::ZERO).as_i64vec3().element_product() as usize;
        Self { size, colours: Vec::new(), markers: Vec::new(), cells: vec![0; volume] }
    }

    /// The place of cell `p` in `cells`.
    pub fn index(&self, p: IVec3) -> usize {
        let s = self.size.as_i64vec3();
        (p.x as i64 + s.x * (p.y as i64 + s.y * p.z as i64)) as usize
    }

    /// The cell at place `i` of `cells`.
    pub fn position(&self, i: usize) -> IVec3 {
        let (sx, sy) = (self.size.x as usize, self.size.y as usize);
        IVec3::new((i % sx) as i32, (i / sx % sy) as i32, (i / (sx * sy)) as i32)
    }

    /// The markers' cells in the order their things come: the palette's
    /// marker entries in order, each entry's cells in reading order (layer
    /// by layer from the bottom, rows far to near, x left to right), with
    /// the marker's place in `markers`.
    pub fn found(&self) -> Vec<(IVec3, usize)> {
        if self.markers.is_empty() {
            return Vec::new();
        }
        let n = self.colours.len();
        let s = self.size;
        let mut out = Vec::new();
        for z in 0..s.z {
            for y in (0..s.y).rev() {
                for x in 0..s.x {
                    let p = IVec3::new(x, y, z);
                    let v = self.cells[self.index(p)] as usize;
                    if v > n {
                        out.push((p, v - n - 1));
                    }
                }
            }
        }
        out.sort_by_key(|&(_, k)| k);
        out
    }

    /// The world it is (a world file, `c.world.file` or one of `levels/`):
    /// its colours the world's palette by number, its markers the things
    /// to place (`names`: the kinds of things).
    pub fn world(&self, names: &[&str]) -> Result<(World, Vec<Marker>)> {
        if self.colours.len() > 255 {
            bail!("the palette has {} colors; a world holds 255 at most", self.colours.len());
        }
        let mut palette = [[0u8; 4]; 256];
        for (i, [r, g, b]) in self.colours.iter().enumerate() {
            palette[i + 1] = [*r, *g, *b, 255];
        }
        let mut world = World::new(self.size, palette);
        // Voxel by voxel as the world counts its writes (`World::changes`,
        // part of the state): the same count whatever their order.
        let n = self.colours.len() as u16;
        let s = self.size;
        let mut rows = self.cells.chunks_exact(s.x as usize);
        for z in 0..s.z {
            for y in 0..s.y {
                let row = rows.next().expect("a row of the box");
                if row.iter().all(|&v| v == 0) {
                    continue;
                }
                for (x, &v) in row.iter().enumerate() {
                    if v != 0 && v <= n {
                        world.set_raw(IVec3::new(x as i32, y, z), v as u8);
                    }
                }
            }
        }
        let markers = self.found().into_iter().map(|(cell, k)| Ok(Marker { kind: kind_of(&self.markers[k], names)?, cell })).collect::<Result<_>>()?;
        world.mark_all_dirty();
        Ok((world, markers))
    }

    /// Its text: the colours in their order, then the markers, one frame of
    /// the box its voxels fill.
    pub fn write(&self) -> String {
        let entries = self.colours.len() + self.markers.len();
        let width = if entries <= ONE_SIGN { 1 } else { 2 };
        let sign = |i: usize| vvx::nth_sign(i, width);
        let mut text = format!("format = \"{}\"\nsize = [{}, {}, {}]\n\n", vvx::FORMAT, self.size.x, self.size.y, self.size.z);
        text += "# The colours are the world's numbers in this order (the first is 1);\n# a marker places a thing of that kind instead of a voxel.\n[palette]\n";
        for (i, [r, g, b]) in self.colours.iter().enumerate() {
            text += &format!("{} = \"#{r:02x}{g:02x}{b:02x}\"\n", sign(i));
        }
        for (k, kind) in self.markers.iter().enumerate() {
            text += &format!("{} = {{ marker = {} }}\n", sign(self.colours.len() + k), vvx::quoted(kind));
        }
        text += "\n[[frame]]\n";
        let filled = self.cells.iter().enumerate().filter(|(_, v)| **v != 0).map(|(i, _)| self.position(i));
        let Some((low, high)) = filled.fold(None, |b: Option<(IVec3, IVec3)>, p| Some(b.map_or((p, p), |(l, h)| (l.min(p), h.max(p))))) else {
            return text + "layers = \"\"\n";
        };
        if low != IVec3::ZERO {
            let at = low - vvx::pivot(self.size);
            text += &format!("at = [{}, {}, {}]\n", at.x, at.y, at.z);
        }
        let dots = ".".repeat(width);
        let signs: Vec<String> = (0..entries).map(sign).collect();
        let layers: Vec<Vec<String>> = (low.z..=high.z)
            .map(|z| {
                (low.y..=high.y)
                    .rev()
                    .map(|y| {
                        let mut row = String::with_capacity(((high.x - low.x + 1) as usize) * width);
                        for x in low.x..=high.x {
                            match self.cells[self.index(IVec3::new(x, y, z))] {
                                0 => row += &dots,
                                v => row += &signs[v as usize - 1],
                            }
                        }
                        row
                    })
                    .collect()
            })
            .collect();
        text + &vvx::layers_text(&layers)
    }
}

/// A kind's place among the game's kinds of things.
fn kind_of(kind: &str, names: &[&str]) -> Result<usize> {
    names.iter().position(|n| *n == kind).ok_or_else(|| anyhow!("marker '{kind}' is no kind of thing (c.things: {})", names.join(", ")))
}

/// Whether the text could be a world's layers taken straight from the
/// file: signs, dots, repeats and their spaces, lines.
fn plain(body: &str) -> bool {
    body.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'*' | b' ' | b'\t' | b'\r' | b'\n'))
}

/// The scene in the file at `path` of the game's files, its mistakes naming
/// the file.
pub fn load_in(files: &Files, path: &Path) -> Result<Scene> {
    let bytes = files.read(path).with_context(|| format!("reading {}", path.display()))?;
    let src = String::from_utf8(bytes).map_err(|_| anyhow!("{}: not text: a world or a prefab is a .vvx (`voxcade upgrade` brings a MagicaVoxel .vox over)", path.display()))?;
    read(&src).map_err(|e| anyhow!("{}: {e}", path.display()))
}

pub fn load(path: &Path) -> Result<Scene> {
    load_in(&Files::disk(), path)
}

/// The scene a text says. A world is big, so its layers are read straight
/// from the text when they stand in one plain `"""` text (as `write` puts
/// them), the rest of the file as TOML; else all of it as TOML.
pub fn read(src: &str) -> Result<Scene> {
    // The one `"""` text: the file with only its line breaks left in it,
    // so that every line keeps its number for the mistakes.
    let fast = (|| {
        let open = src.find("\"\"\"")?;
        let line_start = src[..open].rfind('\n').map_or(0, |i| i + 1);
        if src[line_start..open].split('=').next()?.trim() != "layers" {
            return None;
        }
        let close = open + 3 + src[open + 3..].find("\"\"\"")?;
        let body = &src[open + 3..close];
        if !plain(body) || src[close + 3..].contains("\"\"\"") {
            return None;
        }
        let head = format!("{}\"\"\"{}\"\"\"{}", &src[..open], "\n".repeat(body.matches('\n').count()), &src[close + 3..]);
        // (The body's first line is the rest of the opening line, so its
        // line i is the opening line's number plus i.)
        let first = vvx::line_of(src, open);
        Some((head, body, first))
    })();
    match fast {
        Some((head, body, first)) => read_parts(&head, Some((body, first))),
        None => read_parts(src, None),
    }
}

/// What a scene's text says, its layers' text (and the line it begins on)
/// given apart or read from the TOML.
fn read_parts(src: &str, layers: Option<(&str, usize)>) -> Result<Scene> {
    let doc = vvx::parse(src)?;
    let r = Reader { src };
    let top = doc.as_table();
    vvx::check_format_of(&r, top, vvx::FORMAT_NAME, vvx::VERSION, "world")?;
    if let Some(p) = top.get("part") {
        return Err(r.fail(p.span(), "a world or a prefab is one box of voxels, one [[frame]]: it has no parts"));
    }
    r.only(top, &["format", "size", "palette", "frame"], "a world or a prefab")?;
    let (size, size_item) = r.size(top, "the box, in voxels", "size must be at least one voxel each way")?;
    if let Some(why) = too_big(size) {
        return Err(r.fail(size_item.span(), why));
    }
    let palette_item = top.get("palette").ok_or_else(|| anyhow!("no [palette]: the signs and their colors"))?;
    let table = palette_item.as_table_like().ok_or_else(|| r.fail(palette_item.span(), "palette must be a table of signs"))?;
    let mut scene = Scene::new(size);
    // The signs: (sign, colour or marker), in the file's order.
    let mut entries: Vec<(String, Result<[u8; 3], String>)> = Vec::new();
    let mut width = 0;
    for (sign, item) in table.iter() {
        r.sign(sign, item, &mut width)?;
        if let Some(t) = item.as_table_like().filter(|t| t.contains_key("marker")) {
            r.only(t, &["marker"], &format!("the palette's '{sign}' (a marker)"))?;
            let kind = t.get("marker").and_then(Item::as_str).filter(|k| !k.trim().is_empty());
            let kind = kind.ok_or_else(|| r.fail(item.span(), format!("the palette's '{sign}': marker = \"a kind of thing\", by its name")))?;
            entries.push((sign.to_string(), Err(kind.to_string())));
            continue;
        }
        let mat = r.material(sign, item)?;
        if mat.rgba[3] != 255 || mat.metal != 0 || mat.glow != 0 {
            return Err(r.fail(item.span(), format!("the palette's '{sign}': a world's colors are plain, its voxels have no glass, metal or glow")));
        }
        entries.push((sign.to_string(), Ok([mat.rgba[0], mat.rgba[1], mat.rgba[2]])));
    }
    // Numbers: the colours first by their order, then the markers.
    let mut numbers: HashMap<String, u16> = HashMap::new();
    for (sign, e) in &entries {
        if let Ok(c) = e {
            scene.colours.push(*c);
            numbers.insert(sign.clone(), scene.colours.len() as u16);
        }
    }
    if scene.colours.len() > 255 {
        bail!("the palette has {} colors; a world or a prefab holds 255 at most", scene.colours.len());
    }
    for (sign, e) in &entries {
        if let Err(kind) = e {
            scene.markers.push(kind.clone());
            numbers.insert(sign.clone(), (scene.colours.len() + scene.markers.len()) as u16);
        }
    }
    let width = width.max(1);
    let frame = top.get("frame").ok_or_else(|| anyhow!("no [[frame]] with its layers"))?;
    let list = vvx::tables(frame).ok_or_else(|| r.fail(frame.span(), "the frame must be a table: [[frame]]"))?;
    let [t] = list[..] else { bail!("a world or a prefab is one [[frame]], this has {}", list.len()) };
    r.only(t, &["at", "layers"], "a frame")?;
    let pivot = vvx::pivot(size);
    let low = t.get("at").map_or(Ok(-pivot), |a| r.ints(a, "at"))? + pivot;
    let (text, first) = match (layers, t.get("layers")) {
        (Some(given), _) => given,
        (None, Some(item)) => (item.as_str().ok_or_else(|| r.fail(item.span(), "layers must be a text between \"\"\" marks"))?, r.first_line(item)),
        (None, None) => ("", 0),
    };
    fill(&mut scene, text, first, &numbers, width, low)?;
    Ok(scene)
}

/// The layers' text into the scene's cells, its box's low corner at `low`
/// (read as a model's are, `vvx::read_cells`).
fn fill(scene: &mut Scene, text: &str, first: usize, numbers: &HashMap<String, u16>, width: usize, low: IVec3) -> Result<()> {
    let line = |i: usize| if first > 0 { format!("line {}: ", first + i) } else { String::new() };
    let table = vvx::SignTable::new(width, numbers.iter().map(|(s, &n)| (s.as_str(), n)));
    let known = || vvx::sorted_signs(numbers.keys());
    let sign = |s: &[u8]| table.get(s);
    let size = scene.size;
    let lay = vvx::Lay { width, low, size, pivot: vvx::pivot(size), what: "the box" };
    let cells = &mut scene.cells;
    vvx::read_cells(text, &line, &lay, sign, known, |p, v| cells[p.x as usize + size.x as usize * (p.y as usize + size.y as usize * p.z as usize)] = v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> Scene {
        let mut s = Scene::new(IVec3::new(4, 3, 2));
        s.colours = vec![[10, 20, 30], [200, 0, 0], [0, 0, 0]];
        s.markers = vec!["rifleman".into(), "guard".into(), "rifleman".into()];
        let mut put = |p: IVec3, v: u16| {
            let i = s.index(p);
            s.cells[i] = v;
        };
        put(IVec3::new(1, 0, 0), 1);
        put(IVec3::new(2, 2, 0), 2);
        put(IVec3::new(3, 2, 1), 4);
        put(IVec3::new(0, 0, 1), 4);
        put(IVec3::new(1, 1, 1), 5);
        put(IVec3::new(2, 1, 1), 6);
        s
    }

    /// A world reads back as written: its colours by number in the
    /// palette's order (a colour no voxel has keeps its number), its
    /// markers by kind.
    #[test]
    fn a_world_reads_back_as_written() {
        let s = scene();
        let text = s.write();
        assert!(text.contains("a = \"#0a141e\"\nb = \"#c80000\"\nc = \"#000000\"\nd = { marker = \"rifleman\" }\ne = { marker = \"guard\" }\nf = { marker = \"rifleman\" }\n"), "{text}");
        assert_eq!(read(&text).unwrap(), s);
        // Read as TOML throughout (a text the fast way does not take).
        let slow = text.replace("layers = \"\"\"", "layers = '''").replacen("\"\"\"\n", "'''\n", 1);
        assert_eq!(read(&slow).unwrap(), s, "{slow}");
    }

    /// Markers come in the order of their entries, each entry's cells in
    /// reading order; the world takes the colours' numbers.
    #[test]
    fn markers_come_by_their_entries() {
        let s = scene();
        let found: Vec<(IVec3, usize)> = s.found();
        assert_eq!(found, [(IVec3::new(3, 2, 1), 0), (IVec3::new(0, 0, 1), 0), (IVec3::new(1, 1, 1), 1), (IVec3::new(2, 1, 1), 2)]);
        let (world, markers) = s.world(&["guard", "rifleman"]).unwrap();
        assert_eq!(world.get(IVec3::new(2, 2, 0)), 2);
        assert_eq!(world.palette[3], [0, 0, 0, 255]);
        assert_eq!(world.palette[4], [0; 4], "no color beyond the palette's");
        assert_eq!(world.get(IVec3::new(3, 2, 1)), 0, "a marker is no voxel");
        assert_eq!(markers.iter().map(|m| m.kind).collect::<Vec<_>>(), [1, 1, 0, 1]);
        let e = s.world(&["guard"]).err().unwrap().to_string();
        assert!(e.contains("marker 'rifleman' is no kind of thing"), "{e}");
    }

    #[test]
    fn mistakes_say_the_line() {
        let text = scene().write();
        let wrong = text.replace(".a..\n", ".x..\n");
        let at = wrong.lines().position(|l| l == ".x..").unwrap() + 1;
        let e = read(&wrong).err().unwrap().to_string();
        assert!(e.contains("'x' is no sign") && e.starts_with(&format!("line {at}:")), "the line the row is on (read the quick way): {e}");
        let slow = wrong.replace("layers = \"\"\"", "layers = '''").replacen("\"\"\"\n", "'''\n", 1);
        let e = read(&slow).err().unwrap().to_string();
        assert!(e.starts_with(&format!("line {at}:")), "the same line read as TOML: {e}");
        let e = read(&text.replace("b = \"#c80000\"", "b = { color = \"#c80000\", glow = 1.0 }")).err().unwrap().to_string();
        assert!(e.contains("plain"), "{e}");
        assert!(crate::vvx::read(&text).err().unwrap().to_string().contains("is a marker"));
    }

    /// A world of more colours than signs takes two signs a cell and reads
    /// back as written, its numbers in the palette's order; what a
    /// hand-written world does wrong is said with its line.
    #[test]
    fn a_world_of_many_colours_reads_back_and_its_mistakes_say_the_line() {
        let mut s = Scene::new(IVec3::new(10, 10, 2));
        s.colours = (0..90).map(|i| [i as u8, 255 - i as u8, 7]).collect();
        s.markers = vec!["guard".into()];
        for (i, cell) in s.cells.iter_mut().enumerate().take(90) {
            *cell = (89 - i) as u16 + 1;
        }
        s.cells[150] = 91;
        let text = s.write();
        assert!(text.contains("\naa = \"#00ff07\"\n") && text.contains("{ marker = \"guard\" }"), "two signs a cell: {text}");
        let back = read(&text).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.write(), text);
        let (world, markers) = back.world(&["guard"]).unwrap();
        assert_eq!(world.get(IVec3::ZERO), 90, "the cell's number is its color's place in the palette");
        assert_eq!(markers.len(), 1);
        let line_of = |needle: &str| text.lines().position(|l| l.contains(needle)).unwrap() + 1;
        let why = |from: &str, to: &str| read(&text.replacen(from, to, 1)).err().map(|e| e.to_string()).expect("a mistake");
        let e = why("size = [10, 10, 2]", "size = [100000, 100000, 1000]");
        assert!(e.starts_with("line 2:") && e.contains("too big"), "{e}");
        let e = why("aa = \"#00ff07\"", "aa = \"#00ff0\"");
        assert!(e.starts_with(&format!("line {}:", line_of("aa = \"#00ff07\""))), "{e}");
        let e = why("[[frame]]\n", "[[frame]]\nrotation = 90\n");
        assert!(e.starts_with(&format!("line {}:", line_of("[[frame]]") + 1)), "{e}");
    }
}

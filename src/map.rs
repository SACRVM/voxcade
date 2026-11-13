//! Levels built from a map (`map.toml`, "voxcade map 1") instead of one
//! big world: a height map and a material map (PNG) for the ground, and
//! prefabs (`.vvx`, `crate::scene`) placed on it. The world is put together
//! when the game starts, so a level is a few hundred kilobytes; the ground
//! is edited in any paint program, placements by hand or by a script.
//!
//! ```toml
//! format = "voxcade map 1"
//! size = [640, 2048, 80]
//!
//! [ground]
//! height = "ground/height.png"
//! material = "ground/material.png"
//! bedrock = "#28201c"
//!
//! [[ground.materials]]
//! key = "#e0c080"
//! surface = ["#d6b880", "#dec189"]
//! depth = 3
//! pattern = 14
//! grain = 0.45
//! strata = ["#78583a", "#967048"]
//! band = 2
//!
//! [prefabs]
//! palm_0 = "prefabs/palm_0.vvx"
//! grass_0 = { file = "prefabs/grass_0.vvx", soft = true }
//!
//! [[put]]
//! prefab = "palm_0"
//! at = [120, 300]
//! rotation = 270
//!
//! [[put]]
//! thing = "rifleman"
//! at = [200, 410]
//!
//! [[place]]
//! name = "gate"
//! at = [320, 40, 6]
//! facing = [0, 1]
//! ```
//!
//! The height map's grey value is the height of each column in voxels; its
//! top row is the north edge of the world. The material map's colours name
//! the material of each column by its `key`. A material colours a column's
//! top `depth` voxels from `surface` (patches `pattern` voxels across,
//! `grain` of speckle), the rest in bands `band` high from `strata`
//! (rising and falling by up to `wobble`). A prefab stands with its pivot
//! (the cell in the middle of its bottom layer, size / 2) on `at`; `at`
//! with two numbers stands on the ground there, with three it is exact.
//! `rotation` turns it about its pivot, degrees clockwise seen from above
//! (as a level's thing's `rotation`), in quarter turns: 0, 90, 180 or 270.
//! The `put`s come in order: a later one covers an earlier one, and the
//! things come in that order (a prefab's markers, `crate::scene`, where
//! it stands). A soft prefab (grass, shrubs) is seen and can be blown
//! away, but bodies, bullets, grenades and debris pass through it. The
//! world's colours are numbered as they come: the bedrock, the materials'
//! colours, then the prefabs in the order of their names, each one's
//! palette in order. `[[place]]`s are named places a player arrives at
//! (`world:go_to(level, place)`), in the world's voxels.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use glam::{IVec2, IVec3};
use toml_edit::{Document, Item, TableLike};

use crate::files::Files;
use crate::vvx::{self, Reader};
use crate::world::Marker;
use crate::world::{CHUNK, World};

/// The ending of a map's file.
pub const EXTENSION: &str = "toml";
/// The format and the version this engine reads and writes.
pub const FORMAT: &str = "voxcade map 1";
const FORMAT_NAME: &str = "voxcade map";
const VERSION: u32 = 1;

/// A map as its file holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct MapData {
    pub size: [i32; 3],
    pub ground: Ground,
    /// The prefabs by name (the world takes their colours in this order).
    pub prefabs: BTreeMap<String, PrefabEntry>,
    pub put: Vec<Put>,
    pub places: Vec<crate::grid::Place>,
}

/// A prefab's file, and whether it is soft.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabEntry {
    pub file: String,
    pub soft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ground {
    /// Grey PNG: height of every column in voxels.
    pub height: String,
    /// Colour PNG: which material every column is made of. Without it every
    /// column is the first material.
    pub material: Option<String>,
    /// Colour of the bottom layer, under every column.
    pub bedrock: Option<String>,
    pub materials: Vec<Material>,
}

/// How a column is coloured: the top `depth` layers from `surface`, the
/// rest in horizontal bands from `strata`.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// Its colour in the material map.
    pub key: Option<String>,
    pub surface: Vec<String>,
    pub depth: i32,
    /// Size of the smooth patches the surface colours form (0: no patches).
    pub pattern: f32,
    /// Share of voxel-by-voxel speckle in the surface colours, 0..1.
    pub grain: f32,
    pub strata: Vec<String>,
    /// Height of each band of strata.
    pub band: i32,
    /// Bands rise and fall across the map by up to this many voxels.
    pub wobble: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self { key: None, surface: Vec::new(), depth: 1, pattern: 0.0, grain: 1.0, strata: Vec::new(), band: 4, wobble: 0.0 }
    }
}

/// What stands on the map: a prefab, or a thing of a kind; `at` x, y (on
/// the ground) or x, y, z.
#[derive(Clone, Debug, PartialEq)]
pub struct Put {
    pub prefab: Option<String>,
    pub thing: Option<String>,
    pub at: Vec<i32>,
    /// Degrees clockwise seen from above, a multiple of 90.
    pub rotation: i32,
}

/// The head of a map's `[[put]]`s as the engine writes it.
pub const PUT_HEAD: &str = "# What stands on the map, in order: a prefab (rotation: degrees clockwise\n# seen from above, in quarter turns) or a thing of a kind; at x, y on the\n# ground, or x, y, z.";

impl Put {
    /// Its rotation as quarter turns anticlockwise seen from above (0 to 3),
    /// as a prefab's cells are turned (`turn`).
    pub fn quarters(&self) -> i32 {
        (-self.rotation / 90).rem_euclid(4)
    }

    /// The rotation of `quarters` anticlockwise quarter turns (a map's
    /// `turn` before Voxcade took other tools' words): degrees clockwise,
    /// 0 to 270.
    pub fn rotation_of(quarters: i32) -> i32 {
        (-90 * quarters).rem_euclid(360)
    }
}

/// A number as TOML writes it: whole numbers without a point.
fn number(v: f32) -> String {
    format!("{v}")
}

fn strings(list: &[String]) -> String {
    format!("[{}]", list.iter().map(|s| vvx::quoted(s)).collect::<Vec<_>>().join(", "))
}

impl MapData {
    /// The map's text.
    pub fn write(&self) -> String {
        let [x, y, z] = self.size;
        let g = &self.ground;
        let mut out = format!("format = \"{FORMAT}\"\nsize = [{x}, {y}, {z}]    # voxels: across (x), north (y), up (z)\n\n");
        out += "# The ground: each column as high as the height map's grey (its top row\n# the north edge), of the material whose key the material map's colour is.\n[ground]\n";
        out += &format!("height = {}\n", vvx::quoted(&g.height));
        if let Some(m) = &g.material {
            out += &format!("material = {}\n", vvx::quoted(m));
        }
        if let Some(b) = &g.bedrock {
            out += &format!("bedrock = {}\n", vvx::quoted(b));
        }
        let d = Material::default();
        for m in &g.materials {
            out += "\n[[ground.materials]]\n";
            if let Some(k) = &m.key {
                out += &format!("key = {}\n", vvx::quoted(k));
            }
            if !m.surface.is_empty() {
                out += &format!("surface = {}\n", strings(&m.surface));
            }
            if m.depth != d.depth {
                out += &format!("depth = {}\n", m.depth);
            }
            if m.pattern != d.pattern {
                out += &format!("pattern = {}\n", number(m.pattern));
            }
            if m.grain != d.grain {
                out += &format!("grain = {}\n", number(m.grain));
            }
            if !m.strata.is_empty() {
                out += &format!("strata = {}\n", strings(&m.strata));
            }
            if m.band != d.band {
                out += &format!("band = {}\n", m.band);
            }
            if m.wobble != d.wobble {
                out += &format!("wobble = {}\n", number(m.wobble));
            }
        }
        out += "\n# The prefabs by name: each its file; a soft one (grass) is passed through.\n[prefabs]\n";
        for (name, p) in &self.prefabs {
            let key = if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && !name.is_empty() { name.clone() } else { vvx::quoted(name) };
            if p.soft {
                out += &format!("{key} = {{ file = {}, soft = true }}\n", vvx::quoted(&p.file));
            } else {
                out += &format!("{key} = {}\n", vvx::quoted(&p.file));
            }
        }
        out += &format!("\n{PUT_HEAD}\n");
        for p in &self.put {
            out += "[[put]]\n";
            if let Some(name) = &p.prefab {
                out += &format!("prefab = {}\n", vvx::quoted(name));
            }
            if let Some(kind) = &p.thing {
                out += &format!("thing = {}\n", vvx::quoted(kind));
            }
            out += &format!("at = [{}]\n", p.at.iter().map(i32::to_string).collect::<Vec<_>>().join(", "));
            if p.rotation != 0 {
                out += &format!("rotation = {}\n", p.rotation);
            }
            out += "\n";
        }
        for p in &self.places {
            out += &format!("[[place]]\nname = {}\nat = [{}, {}, {}]\n", vvx::quoted(&p.name), p.at[0], p.at[1], p.at[2]);
            if let Some([x, y]) = p.facing {
                out += &format!("facing = [{x}, {y}]\n");
            }
            out += "\n";
        }
        out.trim_end().to_string() + "\n"
    }

    /// The map a text says, every mistake with its line.
    pub fn read(src: &str) -> Result<Self> {
        let doc = Document::parse(src).map_err(|e| match e.span() {
            Some(s) => anyhow!("line {}: {}", vvx::line_of(src, s.start), e.message()),
            None => anyhow!("{}", e.message()),
        })?;
        let r = Reader { src };
        let top = doc.as_table();
        vvx::check_format_of(&r, top, FORMAT_NAME, VERSION, "map")?;
        r.only(top, &["format", "size", "ground", "prefabs", "put", "place"], "a map")?;
        let size_item = top.get("size").ok_or_else(|| anyhow!("no size = [x, y, z] (the world, in voxels)"))?;
        let size = r.ints(size_item, "size")?;
        if size.cmplt(IVec3::ONE).any() {
            return Err(r.fail(size_item.span(), "size must be positive"));
        }
        if let Some(why) = crate::world::too_big(size.as_i64vec3()) {
            return Err(r.fail(size_item.span(), format!("the map of {why}")));
        }
        let text =|t: &dyn TableLike, key: &str, what: &str| -> Result<Option<String>> {
            match t.get(key) {
                None => Ok(None),
                Some(i) => Ok(Some(i.as_str().ok_or_else(|| r.fail(i.span(), format!("{what} must be text")))?.to_string())),
            }
        };
        let list = |t: &dyn TableLike, key: &str| -> Result<Vec<String>> {
            match t.get(key) {
                None => Ok(Vec::new()),
                Some(i) => {
                    let wrong = || r.fail(i.span(), format!("{key} must be a list of colors, like [\"#d6b880\", \"#dec189\"]"));
                    i.as_array().ok_or_else(wrong)?.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(wrong)).collect()
                }
            }
        };
        let whole = |t: &dyn TableLike, key: &str, default: i32| -> Result<i32> {
            match t.get(key) {
                None => Ok(default),
                Some(i) => i.as_integer().map(|v| v as i32).ok_or_else(|| r.fail(i.span(), format!("{key} must be a whole number"))),
            }
        };
        let float = |t: &dyn TableLike, key: &str, default: f32| -> Result<f32> { t.get(key).map_or(Ok(default), |i| r.number(i, key)) };
        let ground_item = top.get("ground").ok_or_else(|| anyhow!("no [ground]: its height map and materials"))?;
        let gt = ground_item.as_table_like().ok_or_else(|| r.fail(ground_item.span(), "ground must be a table: [ground]"))?;
        r.only(gt, &["height", "material", "bedrock", "materials"], "[ground]")?;
        let mut materials = Vec::new();
        if let Some(item) = gt.get("materials") {
            for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "materials must be tables: [[ground.materials]]"))? {
                r.only(t, &["key", "surface", "depth", "pattern", "grain", "strata", "band", "wobble"], "a material")?;
                let d = Material::default();
                materials.push(Material {
                    key: text(t, "key", "key")?,
                    surface: list(t, "surface")?,
                    depth: whole(t, "depth", d.depth)?,
                    pattern: float(t, "pattern", d.pattern)?,
                    grain: float(t, "grain", d.grain)?,
                    strata: list(t, "strata")?,
                    band: whole(t, "band", d.band)?,
                    wobble: float(t, "wobble", d.wobble)?,
                });
            }
        }
        let ground = Ground {
            height: text(gt, "height", "height")?.ok_or_else(|| r.fail(ground_item.span(), "[ground] has no height = \"a gray picture\""))?,
            material: text(gt, "material", "material")?,
            bedrock: text(gt, "bedrock", "bedrock")?,
            materials,
        };
        let mut prefabs = BTreeMap::new();
        if let Some(item) = top.get("prefabs") {
            let t = item.as_table_like().ok_or_else(|| r.fail(item.span(), "prefabs must be a table: name = \"prefabs/name.vvx\""))?;
            for (name, entry) in t.iter() {
                let p = match (entry.as_str(), entry.as_table_like()) {
                    (Some(file), _) => PrefabEntry { file: file.to_string(), soft: false },
                    (None, Some(e)) => {
                        r.only(e, &["file", "soft"], &format!("prefab '{name}'"))?;
                        let file = text(e, "file", "file")?.ok_or_else(|| r.fail(entry.span(), format!("prefab '{name}' has no file")))?;
                        let soft = match e.get("soft") {
                            None => false,
                            Some(s) => s.as_bool().ok_or_else(|| r.fail(s.span(), "soft must be true or false"))?,
                        };
                        PrefabEntry { file, soft }
                    }
                    _ => return Err(r.fail(entry.span(), format!("prefab '{name}' must be its file, or {{ file = \"...\", soft = true }}"))),
                };
                prefabs.insert(name.to_string(), p);
            }
        }
        let mut put = Vec::new();
        if let Some(item) = top.get("put") {
            for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "what stands on the map must be tables: [[put]]"))? {
                if let Some(old) = t.get("turn") {
                    let was = old.as_integer().map_or(String::new(), |q| format!(": turn = {q} is rotation = {}", Put::rotation_of(q as i32)));
                    return Err(r.fail(old.span(), format!("a [[put]]: 'turn' (quarter turns anticlockwise) is called 'rotation' now, in degrees clockwise{was} (voxcade upgrade renames it)")));
                }
                r.only(t, &["prefab", "thing", "at", "rotation"], "a [[put]]")?;
                let at_item = t.get("at").ok_or_else(|| anyhow!("a [[put]] has no at (line {})", t.get("prefab").or(t.get("thing")).and_then(Item::span).map_or(0, |s| vvx::line_of(src, s.start))))?;
                let wrong = || r.fail(at_item.span(), "at must be x, y (on the ground) or x, y, z, whole numbers");
                let at: Vec<i32> = at_item.as_array().filter(|a| (2..=3).contains(&a.len())).ok_or_else(wrong)?.iter().map(|v| v.as_integer().map(|v| v as i32).ok_or_else(wrong)).collect::<Result<_>>()?;
                let (prefab, thing) = (text(t, "prefab", "prefab")?, text(t, "thing", "thing")?);
                if prefab.is_some() == thing.is_some() {
                    return Err(r.fail(at_item.span(), "a [[put]] is a prefab = \"name\" or a thing = \"kind\", one of them"));
                }
                if let Some(axis) = (0..at.len()).find(|&a| at[a] < 0 || (a < 2 && at[a] >= size[a])) {
                    let shown = at.iter().map(i32::to_string).collect::<Vec<_>>().join(", ");
                    let axis = ["x", "y", "z"][axis];
                    return Err(r.fail(at_item.span(), format!("at = [{shown}]: {axis} lies outside the map, which is {} by {} by {} voxels (x 0 to {}, y 0 to {}, z from 0 up: the sky over it is open)", size.x, size.y, size.z, size.x - 1, size.y - 1)));
                }
                let rotation = whole(t, "rotation", 0)?;
                if rotation % 90 != 0 {
                    return Err(r.fail(t.get("rotation").and_then(Item::span), format!("rotation = {rotation}: a prefab stands in quarter turns, 0, 90, 180 or 270 degrees")));
                }
                put.push(Put { prefab, thing, at, rotation });
            }
        }
        let mut places = Vec::new();
        if let Some(item) = top.get("place") {
            for t in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "places must be tables: [[place]] with name and at"))? {
                r.only(t, &["name", "at", "facing"], "a place")?;
                let name = text(t, "name", "name")?.ok_or_else(|| anyhow!("a place has no name"))?;
                let at_item = t.get("at").ok_or_else(|| anyhow!("place '{name}' has no at = [x, y, z]"))?;
                let at = r.ints(at_item, "at")?.to_array();
                if let Some(axis) = (0..3).find(|&a| at[a] < 0 || (a < 2 && at[a] >= size[a])) {
                    let axis = ["x", "y", "z"][axis];
                    return Err(r.fail(at_item.span(), format!("place '{name}' at [{}, {}, {}]: {axis} lies outside the map, which is {} by {} by {} voxels (x 0 to {}, y 0 to {}, z from 0 up: the sky over it is open)", at[0], at[1], at[2], size.x, size.y, size.z, size.x - 1, size.y - 1)));
                }
                let facing = match t.get("facing") {
                    None => None,
                    Some(f) => {
                        let wrong = || r.fail(f.span(), "facing must be two whole numbers, like [0, -1]");
                        let v: Vec<i32> = f.as_array().filter(|a| a.len() == 2).ok_or_else(wrong)?.iter().map(|v| v.as_integer().map(|v| v as i32).ok_or_else(wrong)).collect::<Result<_>>()?;
                        Some([v[0], v[1]])
                    }
                };
                places.push(crate::grid::Place { name, at, facing, named: false, line: 0 });
            }
        }
        Ok(Self { size: size.to_array(), ground, prefabs, put, places })
    }
}

/// A material with its colours turned into world palette indices.
struct Colours {
    surface: Vec<u8>,
    depth: i32,
    pattern: f32,
    grain: f32,
    strata: Vec<u8>,
    band: i32,
    wobble: f32,
}

/// A prefab ready to stamp: voxels as world palette indices around its
/// anchor cell, and the markers inside it.
struct Prefab {
    voxels: Vec<(IVec3, u8)>,
    markers: Vec<(IVec3, usize)>,
}

/// The world palette, filled with every colour the ground and prefabs use.
/// A colour used both soft and solid takes two entries.
struct Palette {
    colours: [[u8; 4]; 256],
    soft: [bool; 256],
    index: HashMap<([u8; 4], bool), u8>,
}

impl Palette {
    fn new() -> Self {
        Self { colours: [[0; 4]; 256], soft: [false; 256], index: HashMap::new() }
    }

    fn get(&mut self, colour: [u8; 4], soft: bool) -> Result<u8> {
        if let Some(&i) = self.index.get(&(colour, soft)) {
            return Ok(i);
        }
        let i = self.index.len() + 1;
        if i > 255 {
            bail!("the map uses more than 255 colors");
        }
        self.colours[i] = colour;
        self.soft[i] = soft;
        self.index.insert((colour, soft), i as u8);
        Ok(i as u8)
    }

    fn hex(&mut self, text: &str) -> Result<u8> {
        let colour = parse_hex(text)?;
        self.get(colour, false)
    }
}

fn parse_hex(text: &str) -> Result<[u8; 4]> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    let value = u32::from_str_radix(digits, 16).ok().filter(|_| digits.len() == 6);
    let v = value.with_context(|| format!("'{text}' is not a color like #d6b880"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
}

/// A PNG as width, height and RGB per pixel (grey repeated).
pub(crate) fn read_png_in(files: &Files, path: &Path) -> Result<(u32, u32, Vec<[u8; 3]>)> {
    let bytes = files.read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().with_context(|| format!("decoding {}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().context("image too large")?];
    let info = reader.next_frame(&mut buf).with_context(|| format!("decoding {}", path.display()))?;
    let samples = info.color_type.samples();
    let mut pixels = Vec::with_capacity((info.width * info.height) as usize);
    for row in buf.chunks(info.line_size).take(info.height as usize) {
        for px in row.chunks(samples).take(info.width as usize) {
            pixels.push(if samples >= 3 { [px[0], px[1], px[2]] } else { [px[0]; 3] });
        }
    }
    Ok((info.width, info.height, pixels))
}

/// A picture's pixels with their alpha (a picture without one is opaque).
pub(crate) fn read_png_rgba(path: &Path) -> Result<(u32, u32, Vec<[u8; 4]>)> {
    read_png_rgba_in(&Files::disk(), path)
}

/// A picture of the game's files (`crate::files`), with its alpha.
pub(crate) fn read_png_rgba_in(files: &Files, path: &Path) -> Result<(u32, u32, Vec<[u8; 4]>)> {
    let bytes = files.read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().with_context(|| format!("decoding {}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().context("image too large")?];
    let info = reader.next_frame(&mut buf).with_context(|| format!("decoding {}", path.display()))?;
    let samples = info.color_type.samples();
    let mut pixels = Vec::with_capacity((info.width * info.height) as usize);
    for row in buf.chunks(info.line_size).take(info.height as usize) {
        for px in row.chunks(samples).take(info.width as usize) {
            pixels.push(match samples {
                4 => [px[0], px[1], px[2], px[3]],
                3 => [px[0], px[1], px[2], 255],
                2 => [px[0], px[0], px[0], px[1]],
                _ => [px[0], px[0], px[0], 255],
            });
        }
    }
    Ok((info.width, info.height, pixels))
}

/// Turns a cell offset from the anchor by quarter turns, anticlockwise
/// seen from above.
fn turn(l: IVec3, quarters: i32) -> IVec3 {
    match quarters.rem_euclid(4) {
        0 => l,
        1 => IVec3::new(-l.y, l.x, l.z),
        2 => IVec3::new(-l.x, -l.y, l.z),
        _ => IVec3::new(l.y, -l.x, l.z),
    }
}

/// Builds the world a map describes (`names`: the kinds of things).
/// Returns it, the markers to place things, and every file it was made from
/// (for hot reload).
pub fn load_map(path: &Path, names: &[&str]) -> Result<(World, Vec<Marker>, Vec<PathBuf>)> {
    let made = load_map_at(&Files::disk(), path, names)?;
    Ok((made.world, made.markers, made.files))
}

/// The map at `path` of the game's files with its named places.
pub fn load_map_at(files: &Files, path: &Path, names: &[&str]) -> Result<crate::levels::Made> {
    let text = files.read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let map = MapData::read(&text).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut prefabs = Vec::new();
    for (name, entry) in &map.prefabs {
        let file = dir.join(&entry.file);
        prefabs.push((name.clone(), crate::scene::load_in(files, &file)?, file));
    }
    build(&map, path, files, prefabs, names)
}

/// The world of a map with its prefabs read (name, scene, file; in the
/// order of their names).
pub(crate) fn build(map: &MapData, path: &Path, files: &Files, prefab_scenes: Vec<(String, crate::scene::Scene, PathBuf)>, names: &[&str]) -> Result<crate::levels::Made> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut sources = vec![path.to_path_buf()];
    let size = IVec3::from(map.size);
    if size.cmplt(IVec3::ONE).any() {
        bail!("{}: size must be positive", path.display());
    }
    if let Some(why) = crate::world::too_big(size.as_i64vec3()) {
        bail!("{}: the map of {why}", path.display());
    }
    let mut palette = Palette::new();

    // --- ground ---
    let ground = &map.ground;
    if ground.materials.is_empty() {
        bail!("{}: ground needs at least one material", path.display());
    }
    let bedrock = ground.bedrock.as_deref().map(|c| palette.hex(c)).transpose()?;
    let mut materials = Vec::new();
    let mut keys = HashMap::new();
    for (i, m) in ground.materials.iter().enumerate() {
        if m.surface.is_empty() && m.strata.is_empty() {
            bail!("{}: material {i} has neither surface nor strata colors", path.display());
        }
        if let Some(key) = &m.key {
            keys.insert(parse_hex(key)?, i as u8);
        }
        materials.push(Colours {
            surface: m.surface.iter().map(|c| palette.hex(c)).collect::<Result<_>>()?,
            depth: m.depth,
            pattern: m.pattern,
            grain: m.grain.clamp(0.0, 1.0),
            strata: m.strata.iter().map(|c| palette.hex(c)).collect::<Result<_>>()?,
            band: m.band.max(1),
            wobble: m.wobble,
        });
    }

    let columns = (size.x * size.y) as usize;
    let height_path = dir.join(&ground.height);
    let (w, h, pixels) = read_png_in(files, &height_path)?;
    if (w as i32, h as i32) != (size.x, size.y) {
        bail!("{}: {w}x{h}, but the map is {}x{}", height_path.display(), size.x, size.y);
    }
    sources.push(height_path);
    // Image rows run north to south; world y runs south to north.
    let column = |x: i32, y: i32| (x + (size.y - 1 - y) * size.x) as usize;
    let mut heights = vec![0i32; columns];
    for y in 0..size.y {
        for x in 0..size.x {
            heights[(x + y * size.x) as usize] = (pixels[column(x, y)][0] as i32).min(size.z);
        }
    }
    let mut kinds = vec![0u8; columns];
    if let Some(file) = &ground.material {
        let material_path = dir.join(file);
        let (w, h, pixels) = read_png_in(files, &material_path)?;
        if (w as i32, h as i32) != (size.x, size.y) {
            bail!("{}: {w}x{h}, but the map is {}x{}", material_path.display(), size.x, size.y);
        }
        let mut unknown = None;
        for y in 0..size.y {
            for x in 0..size.x {
                let [r, g, b] = pixels[column(x, y)];
                kinds[(x + y * size.x) as usize] = *keys.get(&[r, g, b, 255]).unwrap_or_else(|| {
                    unknown.get_or_insert((r, g, b));
                    &0
                });
            }
        }
        if let Some((r, g, b)) = unknown {
            eprintln!(
                "warning: {}: color #{r:02x}{g:02x}{b:02x} names no material; using the first",
                material_path.display()
            );
        }
        sources.push(material_path);
    }

    // Per column: where the strata bands sit and which surface patch it is in.
    let mut wobble = vec![0i32; columns];
    let mut patch = vec![0.5f32; columns];
    for y in 0..size.y {
        for x in 0..size.x {
            let i = (x + y * size.x) as usize;
            let m = &materials[kinds[i] as usize];
            if m.wobble != 0.0 {
                wobble[i] = (noise(x as f32 + 1000.0, y as f32, 40.0) * m.wobble) as i32;
            }
            if m.pattern > 0.0 {
                patch[i] = noise(x as f32, y as f32, m.pattern);
            }
        }
    }
    let colour = |p: IVec3| -> u8 {
        let i = (p.x + p.y * size.x) as usize;
        let top = heights[i];
        if p.z >= top {
            return 0;
        }
        if p.z == 0
            && let Some(b) = bedrock
        {
            return b;
        }
        let m = &materials[kinds[i] as usize];
        let pick = |colours: &[u8], v: f32| colours[((v * colours.len() as f32) as usize).min(colours.len() - 1)];
        if m.strata.is_empty() || (top - 1 - p.z < m.depth && !m.surface.is_empty()) {
            let speckle = hash(p);
            let v = if m.pattern > 0.0 { patch[i] * (1.0 - m.grain) + speckle * m.grain } else { speckle };
            let colours = if m.surface.is_empty() { &m.strata } else { &m.surface };
            return pick(colours, v);
        }
        let band = (p.z + wobble[i]).div_euclid(m.band).rem_euclid(m.strata.len() as i32);
        m.strata[band as usize]
    };

    let mut world = World::new(size, [[0; 4]; 256]);
    let chunks = (size + CHUNK - 1) / CHUNK;
    for cy in 0..chunks.y {
        for cx in 0..chunks.x {
            let x0 = cx * CHUNK;
            let y0 = cy * CHUNK;
            let mut tallest = 0;
            for y in y0..(y0 + CHUNK).min(size.y) {
                for x in x0..(x0 + CHUNK).min(size.x) {
                    tallest = tallest.max(heights[(x + y * size.x) as usize]);
                }
            }
            for cz in 0..(tallest + CHUNK - 1) / CHUNK {
                world.fill_chunk(IVec3::new(cx, cy, cz), &colour);
            }
        }
    }

    // --- prefabs ---
    // Their colours take the world's numbers in the order of their names,
    // each one's in its palette's order.
    let mut prefabs = HashMap::new();
    for (name, scene, file) in prefab_scenes {
        let soft = map.prefabs.get(&name).is_some_and(|p| p.soft);
        let numbers: Vec<u8> = scene.colours.iter().map(|&[r, g, b]| palette.get([r, g, b, 255], soft)).collect::<Result<_>>()?;
        let anchor = vvx::pivot(scene.size);
        let mut prefab = Prefab { voxels: Vec::new(), markers: Vec::new() };
        for (i, &v) in scene.cells.iter().enumerate() {
            if v != 0 && (v as usize) <= numbers.len() {
                prefab.voxels.push((scene.position(i) - anchor, numbers[v as usize - 1]));
            }
        }
        for (cell, k) in scene.found() {
            let kind = &scene.markers[k];
            let kind = names.iter().position(|n| n == kind).ok_or_else(|| anyhow!("{}: marker '{kind}' is no kind of thing (c.things: {})", file.display(), names.join(", ")))?;
            prefab.markers.push((cell - anchor, kind));
        }
        prefabs.insert(name, prefab);
        sources.push(file);
    }

    // --- placements ---
    let mut found = Vec::new();
    let ground_at = |at: IVec2| {
        let x = at.x.clamp(0, size.x - 1);
        let y = at.y.clamp(0, size.y - 1);
        heights[(x + y * size.x) as usize]
    };
    for (n, place) in map.put.iter().enumerate() {
        let n = n + 1;
        let at = match place.at[..] {
            [x, y] => IVec3::new(x, y, ground_at(IVec2::new(x, y))),
            [x, y, z] => IVec3::new(x, y, z),
            _ => bail!("{}: put {n}: 'at' needs x, y or x, y, z", path.display()),
        };
        match (&place.prefab, &place.thing) {
            (Some(name), None) => {
                let prefab = prefabs
                    .get(name.as_str())
                    .ok_or_else(|| anyhow!("{}: put {n}: no prefab named '{name}'", path.display()))?;
                for &(l, i) in &prefab.voxels {
                    world.set_raw(at + turn(l, place.quarters()), i);
                }
                for &(l, kind) in &prefab.markers {
                    found.push(Marker { kind, cell: at + turn(l, place.quarters()) });
                }
            }
            (None, Some(name)) => {
                let kind = names
                    .iter()
                    .position(|m| m == name)
                    .ok_or_else(|| anyhow!("{}: put {n}: '{name}' is no kind of thing (c.things: {})", path.display(), names.join(", ")))?;
                found.push(Marker { kind, cell: at });
            }
            _ => bail!("{}: put {n}: give either 'prefab' or 'thing'", path.display()),
        }
    }

    world.palette = palette.colours;
    world.soft = palette.soft;
    world.mark_all_dirty();
    let places = map.places.iter().map(|p| crate::levels::Place::new(&p.name, IVec3::from(p.at).as_vec3() + glam::Vec3::new(0.5, 0.5, 0.0), p.facing)).collect();
    Ok(crate::levels::Made { world, markers: found, files: sources, places, marker_names: Vec::new(), marker_facings: Vec::new(), pieces: Vec::new(), areas: Vec::new(), daylight: Default::default() })
}

const SEED: i64 = 0x5EED;

fn lattice(ix: i64, iy: i64) -> f32 {
    let mut h = (ix.wrapping_mul(374_761_393).wrapping_add(iy.wrapping_mul(668_265_263)).wrapping_add(SEED)) as u64;
    h &= 0xFFFF_FFFF;
    h = ((h ^ (h >> 13)).wrapping_mul(1_274_126_177)) & 0xFFFF_FFFF;
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

/// Smooth value noise in 0..1 with features about `scale` voxels across.
fn noise(x: f32, y: f32, scale: f32) -> f32 {
    let (fx, fy) = (x / scale, y / scale);
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - ix, fy - iy);
    let (tx, ty) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (ix as i64, iy as i64);
    let a = lattice(ix, iy) + (lattice(ix + 1, iy) - lattice(ix, iy)) * tx;
    let b = lattice(ix, iy + 1) + (lattice(ix + 1, iy + 1) - lattice(ix, iy + 1)) * tx;
    a + (b - a) * ty
}

/// A fixed random number in 0..1 for every voxel.
fn hash(p: IVec3) -> f32 {
    let mut h = (p.x as u32).wrapping_mul(0x8DA6_B343)
        ^ (p.y as u32).wrapping_mul(0xD816_3841)
        ^ (p.z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_grey_png(path: &Path, width: u32, height: u32, data: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(data).unwrap();
    }

    /// Ground from the height map (north at the top of the image), a prefab
    /// standing on it turned a quarter with its marker turned along, and a
    /// marker placed by name.
    #[test]
    fn a_map_builds_ground_prefabs_and_markers() {
        let dir = crate::scratch::Scratch::new("map-test");
        std::fs::create_dir_all(&dir).unwrap();
        // 8 x 8, 3 high; the top four image rows (the north) 5 high.
        let mut grey = vec![3u8; 64];
        grey[..32].fill(5);
        write_grey_png(&dir.join("height.png"), 8, 8, &grey);
        // A post three voxels long along x, with an enemy marker on its end
        // (its pivot, the middle of its bottom layer, is its middle voxel).
        let mut post = crate::scene::Scene::new(IVec3::new(3, 1, 2));
        post.colours = vec![[200, 10, 10]];
        post.markers = vec!["rifleman".into()];
        post.cells[..3].fill(1);
        let top = post.index(IVec3::new(2, 0, 1));
        post.cells[top] = 2;
        std::fs::write(dir.join("post.vvx"), post.write()).unwrap();
        let map = MapData {
            size: [8, 8, 16],
            ground: Ground { height: "height.png".into(), material: None, bedrock: Some("#000001".into()), materials: vec![Material { surface: vec!["#00ff00".into()], strata: vec!["#0000ff".into()], ..Material::default() }] },
            prefabs: BTreeMap::from([("post".to_string(), PrefabEntry { file: "post.vvx".into(), soft: false })]),
            put: vec![Put { prefab: Some("post".into()), thing: None, at: vec![2, 1], rotation: 270 }, Put { prefab: None, thing: Some("goal".into()), at: vec![6, 6], rotation: 0 }],
            places: Vec::new(),
        };
        let text = map.write();
        assert_eq!(MapData::read(&text).unwrap(), map, "{text}");
        assert!(text.contains("at = [2, 1]\nrotation = 270\n"), "{text}");
        let e = format!("{:#}", MapData::read(&text.replace("rotation = 270", "turn = 1")).unwrap_err());
        assert!(e.contains("'turn' (quarter turns anticlockwise) is called 'rotation' now, in degrees clockwise: turn = 1 is rotation = 270"), "{e}");
        assert!(format!("{:#}", MapData::read(&text.replace("rotation = 270", "rotation = 45")).unwrap_err()).contains("quarter turns"));
        // Clockwise degrees and the anticlockwise quarters the cells turn by.
        for (quarters, rotation) in [(0, 0), (1, 270), (2, 180), (3, 90), (-1, 90), (5, 270)] {
            assert_eq!(Put::rotation_of(quarters), rotation);
            assert_eq!(Put { prefab: None, thing: None, at: vec![0, 0], rotation }.quarters(), quarters.rem_euclid(4));
        }
        std::fs::write(dir.join("map.toml"), text).unwrap();

        let (goal, enemy) = (0, 1);
        let (world, found, sources) = load_map(&dir.join("map.toml"), &["goal", "rifleman"]).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let rgb = |p: IVec3| world.palette[world.get(p) as usize];

        assert_eq!(world.column_top(0, 0), 3);
        assert_eq!(world.column_top(0, 7), 5, "the top image row is the north edge");
        assert_eq!(rgb(IVec3::new(0, 0, 0)), [0, 0, 1, 255], "bedrock");
        assert_eq!(rgb(IVec3::new(0, 0, 1)), [0, 0, 255, 255], "strata");
        assert_eq!(rgb(IVec3::new(0, 0, 2)), [0, 255, 0, 255], "surface");
        // Turned a quarter, the post runs along y through its anchor (2, 1).
        for y in 0..3 {
            assert_eq!(rgb(IVec3::new(2, y, 3)), [200, 10, 10, 255], "post at y {y}");
        }
        assert_eq!(world.get(IVec3::new(1, 1, 3)), 0, "the unturned post would be here");
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|m| m.kind == enemy && m.cell == IVec3::new(2, 2, 4)));
        assert!(found.iter().any(|m| m.kind == goal && m.cell == IVec3::new(6, 6, 5)));
        assert_eq!(sources.len(), 3, "map, height map and prefab");
    }

    /// A map of every word the engine writes reads back the same, and what
    /// a hand-written map does wrong is said with its line: a `[[put]]` or
    /// a place outside the map (at = [-99999, ...] once loaded without a
    /// word), a prefab not in quarter turns, a put of neither or both, an
    /// unknown word, a size below a voxel, a color that is none.
    #[test]
    fn a_maps_text_reads_back_and_its_mistakes_say_the_line() {
        let map = MapData {
            size: [64, 48, 20],
            ground: Ground {
                height: "ground/height.png".into(),
                material: Some("ground/material.png".into()),
                bedrock: Some("#101010".into()),
                materials: vec![
                    Material { key: Some("#00ff00".into()), surface: vec!["#2f6b2a".into(), "#3a7a32".into()], depth: 2, pattern: 6.5, grain: 0.25, strata: vec!["#6b4a2f".into()], band: 3, wobble: 1.5 },
                    Material::default(),
                ],
            },
            prefabs: BTreeMap::from([("hut".to_string(), PrefabEntry { file: "prefabs/hut.vvx".into(), soft: false }), ("bush".to_string(), PrefabEntry { file: "prefabs/bush.vvx".into(), soft: true })]),
            put: vec![
                Put { prefab: Some("hut".into()), thing: None, at: vec![10, 12], rotation: 90 },
                Put { prefab: Some("bush".into()), thing: None, at: vec![0, 47, 3], rotation: 0 },
                Put { prefab: None, thing: Some("rifleman".into()), at: vec![63, 0], rotation: 0 },
            ],
            places: vec![crate::grid::Place { name: "start".into(), at: [5, 6, 2], facing: Some([0, 1]), named: false, line: 0 }],
        };
        let text = map.write();
        assert_eq!(MapData::read(&text).unwrap(), map, "{text}");
        assert_eq!(MapData::read(&text).unwrap().write(), text, "written again, the same text");
        let line_of = |needle: &str| text.lines().position(|l| l.contains(needle)).unwrap() + 1;
        let why = |from: &str, to: &str| {
            assert!(text.contains(from), "{from}");
            format!("{:#}", MapData::read(&text.replacen(from, to, 1)).expect_err("a mistake"))
        };
        let e = why("at = [10, 12]", "at = [-99999, 12]");
        assert!(e.starts_with(&format!("line {}:", line_of("at = [10, 12]"))) && e.contains("x lies outside the map, which is 64 by 48 by 20 voxels (x 0 to 63, y 0 to 47, z from 0 up: the sky over it is open)"), "{e}");
        let e = why("at = [0, 47, 3]", "at = [0, 48, 3]");
        assert!(e.starts_with(&format!("line {}:", line_of("at = [0, 47, 3]"))) && e.contains("y lies outside the map"), "{e}");
        let e = why("at = [0, 47, 3]", "at = [0, 47, -1]");
        assert!(e.contains("z lies outside the map"), "{e}");
        assert!(MapData::read(&text.replacen("at = [0, 47, 3]", "at = [0, 47, 30]", 1)).is_ok(), "the sky over the map is open");
        let e = why("at = [5, 6, 2]", "at = [5, 6, -1]");
        assert!(e.starts_with(&format!("line {}:", line_of("at = [5, 6, 2]"))) && e.contains("place 'start' at [5, 6, -1]: z lies outside the map"), "{e}");
        let e = why("rotation = 90", "rotation = 45");
        assert!(e.starts_with(&format!("line {}:", line_of("rotation = 90"))) && e.contains("quarter turns"), "{e}");
        let e = why("thing = \"rifleman\"", "thing = \"rifleman\"\nprefab = \"hut\"");
        assert!(e.contains("a prefab = \"name\" or a thing = \"kind\", one of them"), "{e}");
        let e = why("[[put]]\nprefab = \"hut\"", "[[put]]\nheight = 3\nprefab = \"hut\"");
        assert!(e.starts_with(&format!("line {}:", line_of("prefab = \"hut\""))) && e.contains("height"), "{e}");
        let e = why("size = [64, 48, 20]", "size = [64, 0, 20]");
        assert!(e.starts_with("line 2:") && e.contains("size must be positive"), "{e}");
        let e = why("surface = [\"#2f6b2a\", \"#3a7a32\"]", "surface = \"#2f6b2a\"");
        assert!(e.contains("surface must be a list of colors"), "{e}");
    }

    /// More Blood's watchtower comes down when its four legs are shot
    /// through at muzzle height (below the braces), and its guard with it.
    #[test]
    fn the_watchtower_falls_when_its_legs_are_shot() {
        use crate::shooter::{Controls as Input, Shooter as Sim};
        use glam::Vec3;

        let dir = crate::scratch::Scratch::new("tower-test");
        std::fs::create_dir_all(&dir).unwrap();
        write_grey_png(&dir.join("height.png"), 64, 64, &[1; 64 * 64]);
        let tower = Path::new("games/moreblood/prefabs/tower.vvx").canonicalize().unwrap();
        let map = format!(
            "format = \"{FORMAT}\"\nsize = [64, 64, 48]\n[ground]\nheight = \"height.png\"\n[[ground.materials]]\nsurface = [\"#808080\"]\n[prefabs]\ntower = {:?}\n[[put]]\nprefab = \"tower\"\nat = [32, 32]\n",
            tower.to_string_lossy()
        );
        std::fs::write(dir.join("map.toml"), map).unwrap();
        let mut cfg = crate::shooter::tests::kit();
        let names = crate::sim::things::names(&cfg);
        let (world, markers, _) = load_map(&dir.join("map.toml"), &names).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        cfg.player.spawn = [4.0, 4.0];
        let mut sim = Sim::new(world, &cfg, crate::shooter::tests::shapes(), &markers);
        assert_eq!(sim.enemies().count(), 1, "the tower brings its guard");
        // Above the stumps and the rubble that settles on them.
        let above = |sim: &Sim| {
            (20..48).flat_map(|z| (0..64).flat_map(move |y| (0..64).map(move |x| IVec3::new(x, y, z))))
                .filter(|&c| sim.world.get(c) != 0)
                .count()
        };
        assert!(above(&sim) > 300, "the tower should stand");

        // The legs stand at -7..-6 and 5..6 around the anchor; a rifle round
        // hits 12.5 up (More Blood's muzzle) and chips a 2.8 crater.
        for (x, y) in [(26.0, 26.0), (38.0, 26.0), (26.0, 38.0), (38.0, 38.0)] {
            sim.explode(Vec3::new(x, y, 13.5), 2.8);
        }
        for _ in 0..240 {
            sim.step(&Input::default());
        }
        assert_eq!(above(&sim), 0, "the top of the tower is still standing");
        assert!(sim.enemies().next().is_none(), "the guard survived");
    }
}

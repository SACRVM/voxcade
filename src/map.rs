//! Levels built from a map file instead of one big `.vox`: a height map and
//! a material map (PNG) for the ground, and `.vox` prefabs placed on it. The
//! world is put together when the game starts, so a level is a few hundred
//! kilobytes; the ground is edited in any paint program, objects in any
//! voxel editor, placements by hand or by a script.
//!
//! ```json
//! {
//!   "size": [640, 2048, 80],
//!   "ground": {
//!     "height": "ground/height.png",
//!     "material": "ground/material.png",
//!     "bedrock": "#28201c",
//!     "materials": [{ "key": "#e0c080", "surface": ["#d6b880", "#dec189"], "depth": 3,
//!                     "pattern": 14, "grain": 0.45, "strata": ["#78583a", "#967048"], "band": 2 }]
//!   },
//!   "prefabs": { "palm_0": "prefabs/palm_0.vox",
//!                "grass_0": { "file": "prefabs/grass_0.vox", "soft": true } },
//!   "place": [
//!     { "prefab": "palm_0", "at": [120, 300], "turn": 1 },
//!     { "marker": "enemy", "at": [200, 410] }
//!   ]
//! }
//! ```
//!
//! The height map's grey value is the height of each column in voxels; its
//! top row is the north edge of the world. The material map's colours name
//! the material of each column by its `key`. A prefab is placed with the
//! cell in the middle of its bottom layer (size / 2) on `at`; `at` with two
//! numbers stands on the ground there, with three it is exact. `turn` is the
//! number of quarter turns, anticlockwise seen from above. Marker voxels
//! inside prefabs place things just like in a `.vox` level. A soft prefab
//! (grass, shrubs) is seen and can be blown away, but bodies, bullets,
//! grenades and debris pass through it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use glam::{IVec2, IVec3};
use serde::Deserialize;

use crate::files::Files;
use crate::world::Marker;
use crate::vox;
use crate::world::{CHUNK, World};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MapFile {
    size: [i32; 3],
    ground: Ground,
    #[serde(default)]
    prefabs: BTreeMap<String, PrefabEntry>,
    #[serde(default)]
    place: Vec<Place>,
    /// Named places a player arrives at (`world:go_to(level, place)`), in
    /// the world's voxels (not the prefabs' `place`).
    #[serde(default)]
    places: Vec<crate::grid::Place>,
}

/// A prefab file, or a file with options.
#[derive(Deserialize)]
#[serde(untagged)]
enum PrefabEntry {
    File(String),
    With {
        file: String,
        #[serde(default)]
        soft: bool,
    },
}

impl PrefabEntry {
    fn file(&self) -> &str {
        match self {
            PrefabEntry::File(file) | PrefabEntry::With { file, .. } => file,
        }
    }

    fn soft(&self) -> bool {
        matches!(self, PrefabEntry::With { soft: true, .. })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ground {
    /// Grey PNG: height of every column in voxels.
    height: String,
    /// Colour PNG: which material every column is made of. Without it every
    /// column is the first material.
    #[serde(default)]
    material: Option<String>,
    /// Colour of the bottom layer, under every column.
    #[serde(default)]
    bedrock: Option<String>,
    materials: Vec<Material>,
}

/// How a column is coloured: the top `depth` layers from `surface`, the
/// rest in horizontal bands from `strata`.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Material {
    /// Its colour in the material map.
    key: Option<String>,
    surface: Vec<String>,
    depth: i32,
    /// Size of the smooth patches the surface colours form (0: no patches).
    pattern: f32,
    /// Share of voxel-by-voxel speckle in the surface colours, 0..1.
    grain: f32,
    strata: Vec<String>,
    /// Height of each band of strata.
    band: i32,
    /// Bands rise and fall across the map by up to this many voxels.
    wobble: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            key: None,
            surface: Vec::new(),
            depth: 1,
            pattern: 0.0,
            grain: 1.0,
            strata: Vec::new(),
            band: 4,
            wobble: 0.0,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Place {
    #[serde(default)]
    prefab: Option<String>,
    #[serde(default)]
    marker: Option<String>,
    /// x, y (standing on the ground) or x, y, z.
    at: Vec<i32>,
    #[serde(default)]
    turn: i32,
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
            bail!("the map uses more than 255 colours");
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
    let v = value.with_context(|| format!("'{text}' is not a colour like #d6b880"))?;
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

/// Builds the world a map describes. Returns it, the markers to place
/// things, and every file it was made from (for hot reload).
pub fn load_map(path: &Path, markers: &HashMap<u8, usize>, names: &[&str]) -> Result<(World, Vec<Marker>, Vec<PathBuf>)> {
    let (world, found, sources, _) = load_map_at(&Files::disk(), path, markers, names)?;
    Ok((world, found, sources))
}

/// The map at `path` of the game's files with its named places.
pub fn load_map_at(files: &Files, path: &Path, markers: &HashMap<u8, usize>, names: &[&str]) -> Result<crate::levels::Made> {
    let text = files.read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let map: MapFile = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut sources = vec![path.to_path_buf()];
    let size = IVec3::from(map.size);
    if size.cmple(IVec3::ZERO).any() {
        bail!("{}: size must be positive", path.display());
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
            bail!("{}: material {i} has neither surface nor strata colours", path.display());
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
                "warning: {}: colour #{r:02x}{g:02x}{b:02x} names no material; using the first",
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
    let mut prefabs = HashMap::new();
    for (name, entry) in &map.prefabs {
        let prefab_path = dir.join(entry.file());
        let scene = vox::load_scene_in(files, &prefab_path)?;
        let anchor = IVec3::new(scene.size.x / 2, scene.size.y / 2, 0);
        let mut table = [None; 256];
        let mut prefab = Prefab { voxels: Vec::new(), markers: Vec::new() };
        for (p, index) in scene.voxels {
            let l = p - anchor;
            if let Some(&kind) = markers.get(&index) {
                prefab.markers.push((l, kind));
                continue;
            }
            let i = match table[index as usize] {
                Some(i) => i,
                None => {
                    let i = palette.get(scene.palette[index as usize], entry.soft())?;
                    table[index as usize] = Some(i);
                    i
                }
            };
            prefab.voxels.push((l, i));
        }
        prefabs.insert(name.as_str(), prefab);
        sources.push(prefab_path);
    }

    // --- placements ---
    let mut found = Vec::new();
    let ground_at = |at: IVec2| {
        let x = at.x.clamp(0, size.x - 1);
        let y = at.y.clamp(0, size.y - 1);
        heights[(x + y * size.x) as usize]
    };
    for (n, place) in map.place.iter().enumerate() {
        let at = match place.at[..] {
            [x, y] => IVec3::new(x, y, ground_at(IVec2::new(x, y))),
            [x, y, z] => IVec3::new(x, y, z),
            _ => bail!("{}: place {n}: 'at' needs x, y or x, y, z", path.display()),
        };
        match (&place.prefab, &place.marker) {
            (Some(name), None) => {
                let prefab = prefabs
                    .get(name.as_str())
                    .ok_or_else(|| anyhow!("{}: place {n}: no prefab named '{name}'", path.display()))?;
                for &(l, i) in &prefab.voxels {
                    world.set_raw(at + turn(l, place.turn), i);
                }
                for &(l, kind) in &prefab.markers {
                    found.push(Marker { kind, cell: at + turn(l, place.turn) });
                }
            }
            (None, Some(name)) => {
                let kind = names
                    .iter()
                    .position(|m| m == name)
                    .ok_or_else(|| anyhow!("{}: place {n}: no marker named '{name}' (markers: {})", path.display(), names.join(", ")))?;
                found.push(Marker { kind, cell: at });
            }
            _ => bail!("{}: place {n}: give either 'prefab' or 'marker'", path.display()),
        }
    }

    world.palette = palette.colours;
    world.soft = palette.soft;
    world.mark_all_dirty();
    let places = map.places.iter().map(|p| crate::levels::Place::new(&p.name, IVec3::from(p.at).as_vec3() + glam::Vec3::new(0.5, 0.5, 0.0), p.facing)).collect();
    Ok((world, found, sources, places))
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

    /// A one-model `.vox`: voxels as x, y, z, colour index; colours for
    /// indices 1, 2, ...
    fn vox_bytes(size: [u32; 3], voxels: &[[u8; 4]], colours: &[[u8; 3]]) -> Vec<u8> {
        fn chunk(tag: &[u8], content: &[u8], children: &[u8]) -> Vec<u8> {
            let mut out = tag.to_vec();
            out.extend((content.len() as u32).to_le_bytes());
            out.extend((children.len() as u32).to_le_bytes());
            out.extend(content);
            out.extend(children);
            out
        }
        let size: Vec<u8> = size.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut xyzi = (voxels.len() as u32).to_le_bytes().to_vec();
        xyzi.extend(voxels.iter().flatten());
        let mut rgba = vec![0u8; 1024];
        for (i, c) in colours.iter().enumerate() {
            rgba[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let children = [chunk(b"SIZE", &size, &[]), chunk(b"XYZI", &xyzi, &[]), chunk(b"RGBA", &rgba, &[])].concat();
        [b"VOX ".to_vec(), 150u32.to_le_bytes().to_vec(), chunk(b"MAIN", &[], &children)].concat()
    }

    /// Ground from the height map (north at the top of the image), a prefab
    /// standing on it turned a quarter with its marker turned along, and a
    /// marker placed by name.
    #[test]
    fn a_map_builds_ground_prefabs_and_markers() {
        let dir = std::env::temp_dir().join(format!("voxcade-map-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 8 x 8, 3 high; the top four image rows (the north) 5 high.
        let mut grey = vec![3u8; 64];
        grey[..32].fill(5);
        write_grey_png(&dir.join("height.png"), 8, 8, &grey);
        // A post three voxels long along x, with an enemy marker on its end.
        let post = vox_bytes([3, 1, 2], &[[0, 0, 0, 1], [1, 0, 0, 1], [2, 0, 0, 1], [2, 0, 1, 240]], &[[200, 10, 10]]);
        std::fs::write(dir.join("post.vox"), post).unwrap();
        std::fs::write(
            dir.join("map.json"),
            r##"{
                "size": [8, 8, 16],
                "ground": {
                    "height": "height.png",
                    "bedrock": "#000001",
                    "materials": [{ "surface": ["#00ff00"], "strata": ["#0000ff"] }]
                },
                "prefabs": { "post": "post.vox" },
                "place": [
                    { "prefab": "post", "at": [2, 1], "turn": 1 },
                    { "marker": "goal", "at": [6, 6] }
                ]
            }"##,
        )
        .unwrap();

        let (goal, enemy) = (0, 1);
        let markers = HashMap::from([(240u8, enemy)]);
        let (world, found, sources) = load_map(&dir.join("map.json"), &markers, &["goal", "rifleman"]).unwrap();
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

    /// More Blood's watchtower comes down when its four legs are shot
    /// through at muzzle height (below the braces), and its guard with it.
    #[test]
    fn the_watchtower_falls_when_its_legs_are_shot() {
        use crate::shooter::{Controls as Input, Shooter as Sim};
        use glam::Vec3;

        let dir = std::env::temp_dir().join(format!("voxcade-tower-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_grey_png(&dir.join("height.png"), 64, 64, &[1; 64 * 64]);
        let tower = Path::new("games/moreblood/prefabs/tower.vox").canonicalize().unwrap();
        let map = format!(
            r##"{{ "size": [64, 64, 48],
                 "ground": {{ "height": "height.png", "materials": [{{ "surface": ["#808080"] }}] }},
                 "prefabs": {{ "tower": {:?} }},
                 "place": [{{ "prefab": "tower", "at": [32, 32] }}] }}"##,
            tower.to_string_lossy()
        );
        std::fs::write(dir.join("map.json"), map).unwrap();
        let mut cfg = crate::shooter::tests::kit();
        let names = crate::sim::things::names(&cfg);
        let kind = |name: &str| names.iter().position(|n| *n == name).unwrap();
        let table = HashMap::from([(240u8, kind("rifleman")), (251u8, kind("guard"))]);
        let (world, markers, _) = load_map(&dir.join("map.json"), &table, &names).unwrap();
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

use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use glam::IVec3;
use serde::Deserialize;

use crate::blocks::Turn;
use crate::grid::{BlockEntry, Cell, GridLevel, Place, Thing};
use crate::map::{Ground, MapData, Material, PrefabEntry, Put};

/// A level of blocks as JSON: the cells as runs (how many in a row,
/// block, turn), x fastest, then y, then z.
#[derive(Deserialize)]
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

/// A level of blocks from its JSON, as the engine read it before.
pub fn level(text: &str) -> Result<GridLevel> {
    let file: LevelFile = serde_json::from_str(text).map_err(|e| anyhow!("not a level of blocks: {e}"))?;
    let [x, y, z] = file.size;
    if file.block < 1 || x < 1 || y < 1 || z < 1 {
        bail!("a block and the grid are at least 1 each way (block {}, size {x} by {y} by {z})", file.block);
    }
    let mut level = GridLevel::new(file.block, IVec3::new(x, y, z));
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

/// A map as JSON.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MapFile {
    size: [i32; 3],
    ground: GroundFile,
    #[serde(default)]
    prefabs: BTreeMap<String, PrefabFile>,
    #[serde(default)]
    place: Vec<PlaceFile>,
    #[serde(default)]
    places: Vec<Place>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PrefabFile {
    File(String),
    With {
        file: String,
        #[serde(default)]
        soft: bool,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroundFile {
    height: String,
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    bedrock: Option<String>,
    materials: Vec<MaterialFile>,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MaterialFile {
    key: Option<String>,
    surface: Vec<String>,
    depth: i32,
    pattern: f32,
    grain: f32,
    strata: Vec<String>,
    band: i32,
    wobble: f32,
}

impl Default for MaterialFile {
    fn default() -> Self {
        let d = Material::default();
        Self { key: d.key, surface: d.surface, depth: d.depth, pattern: d.pattern, grain: d.grain, strata: d.strata, band: d.band, wobble: d.wobble }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceFile {
    #[serde(default)]
    prefab: Option<String>,
    #[serde(default)]
    marker: Option<String>,
    at: Vec<i32>,
    #[serde(default)]
    turn: i32,
}

/// A map from its JSON, as the engine read it before.
pub fn map(text: &str) -> Result<MapData> {
    let m: MapFile = serde_json::from_str(text).map_err(|e| anyhow!("not a map: {e}"))?;
    let g = m.ground;
    let materials = g.materials.into_iter().map(|m| Material { key: m.key, surface: m.surface, depth: m.depth, pattern: m.pattern, grain: m.grain, strata: m.strata, band: m.band, wobble: m.wobble }).collect();
    let prefabs = m
        .prefabs
        .into_iter()
        .map(|(name, p)| {
            let entry = match p {
                PrefabFile::File(file) => PrefabEntry { file, soft: false },
                PrefabFile::With { file, soft } => PrefabEntry { file, soft },
            };
            (name, entry)
        })
        .collect();
    let put = m.place.into_iter().map(|p| Put { prefab: p.prefab, thing: p.marker, at: p.at, rotation: Put::rotation_of(p.turn) }).collect();
    Ok(MapData { size: m.size, ground: Ground { height: g.height, material: g.material, bedrock: g.bedrock, materials }, prefabs, put, places: m.places })
}

/// What a JSON file of a game is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    Level,
    Map,
}

/// Whether the JSON text is a level of blocks or a map (by what it
/// holds), or neither.
pub fn sort(text: &str) -> Option<Sort> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let o = v.as_object()?;
    if o.contains_key("cells") && o.contains_key("block") {
        Some(Sort::Level)
    } else if o.contains_key("ground") {
        Some(Sort::Map)
    } else {
        None
    }
}

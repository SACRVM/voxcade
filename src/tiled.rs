//! A Tiled map (mapeditor.org, saved as JSON) brought in once as a level of
//! voxel blocks (`grid`, `voxcade import`): Tiled draws in 2D, the level
//! lives on in 3D and is edited in Voxcade's own editor.
//!
//! Every tile the map uses becomes a block, its picture drawn out through
//! the block's depth (`Block::drawn_out`), unless the game has made that
//! block already (`blocks/ID.vvx`, by the id the map stores). Each tile
//! layer becomes a row of cells, the top layer at the front; a map of one
//! layer splits by kind: what holds and hurts in the front row, the
//! decoration a row behind. What a block is comes from its tile's class in
//! the tileset (Tiled: Tileset, Class): solid, oneway, ladder, hazard, or
//! none (decoration). Flipped and turned tiles become turned blocks;
//! object layers become the level's things, by their class, type or name.
//! The tileset is kept as `blocks/tiles.png`, tile `ID` at place `ID - 1`
//! in a grid of whole tiles: the editor draws new blocks out of it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use glam::IVec3;
use serde::Deserialize;

use crate::blocks::{self, Block, Turn};
use crate::grid::{BlockEntry, Cell, GridLevel, Kind, Thing};

/// Tiled's flags in the high bits of a tile id: flipped across, upside
/// down, over the diagonal (with the others: turned); the rest is the id.
const FLIP_X: u32 = 0x8000_0000;
const FLIP_Y: u32 = 0x4000_0000;
const FLIP_DIAGONAL: u32 = 0x2000_0000;
const ID: u32 = 0x0FFF_FFFF;

/// The tileset kept for the editor, in the blocks' folder.
pub const SHEET: &str = "tiles.png";

#[derive(Deserialize)]
struct Map {
    width: i32,
    height: i32,
    tilewidth: i32,
    tileheight: i32,
    #[serde(default)]
    infinite: bool,
    layers: Vec<Layer>,
    tilesets: Vec<Tileset>,
}

#[derive(Deserialize)]
struct Layer {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    encoding: Option<String>,
    #[serde(default)]
    objects: Vec<Object>,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default)]
    layers: Vec<Layer>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct Tileset {
    firstgid: u32,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    columns: i32,
    #[serde(default)]
    tilecount: u32,
    #[serde(default)]
    margin: i32,
    #[serde(default)]
    spacing: i32,
    #[serde(default)]
    tiles: Vec<TileInfo>,
}

/// A tile's own settings in its tileset: its class (`type` in Tiled's
/// files from 1.10 on, `class` in 1.9).
#[derive(Deserialize)]
struct TileInfo {
    id: u32,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    class: String,
}

#[derive(Deserialize)]
struct Object {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    class: String,
    x: f32,
    y: f32,
    #[serde(default)]
    width: f32,
    #[serde(default)]
    height: f32,
    #[serde(default)]
    gid: Option<u32>,
    #[serde(default)]
    point: bool,
}

/// A tileset's picture and how its tiles lie in it.
struct Sheet {
    first: u32,
    count: u32,
    columns: i32,
    margin: i32,
    spacing: i32,
    width: i32,
    pixels: Vec<[u8; 4]>,
}

/// A map brought in: the level, a block for every tile it uses (by the
/// tile's id) and the tileset as whole tiles (width, height, pixels).
pub struct Imported {
    pub level: GridLevel,
    pub blocks: Vec<(String, Block)>,
    pub sheet: (u32, u32, Vec<[u8; 4]>),
}

/// Reads a Tiled map as a level of blocks.
pub fn import(path: &Path) -> Result<Imported> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let map: Map = serde_json::from_str(&text).with_context(|| format!("{}: not a Tiled map saved as JSON", path.display()))?;
    let at = |what: String| anyhow!("{}: {what}", path.display());
    if map.infinite {
        return Err(at("an infinite map; save it with a fixed size".into()));
    }
    let n = map.tilewidth;
    if n != map.tileheight || n < 1 {
        return Err(at(format!("tiles of {} by {} pixels; blocks need square tiles", map.tilewidth, map.tileheight)));
    }
    let base = path.parent().unwrap_or(Path::new("."));

    // The tilesets' pictures, and what each tile is.
    let mut sheets = Vec::new();
    let mut kinds = BTreeMap::new();
    for set in &map.tilesets {
        if set.source.is_some() {
            return Err(at("an external tileset (.tsx); embed it in the map (Tiled: Embed Tileset)".into()));
        }
        let image = set.image.as_ref().ok_or_else(|| at("a tileset of single images; use one tileset picture".into()))?;
        let (width, _, pixels) = crate::map::read_png_rgba(&base.join(image))?;
        for tile in &set.tiles {
            let class = if tile.kind.is_empty() { &tile.class } else { &tile.kind };
            if class.is_empty() {
                continue;
            }
            let Some(kind) = Kind::ALL.into_iter().find(|k| k.name() == class) else {
                return Err(at(format!("tile {}: class '{class}' is no kind of block (solid, oneway, ladder, hazard, decor)", set.firstgid + tile.id)));
            };
            kinds.insert(set.firstgid + tile.id, kind);
        }
        sheets.push(Sheet {
            first: set.firstgid,
            count: set.tilecount,
            columns: set.columns.max(1),
            margin: set.margin,
            spacing: set.spacing,
            width: width as i32,
            pixels,
        });
    }
    let kind = |id: u32| kinds.get(&id).copied().unwrap_or(Kind::Decor);
    // A tile's picture, rows from the top.
    let picture = |id: u32| -> Result<Vec<[u8; 4]>> {
        let sheet = sheets.iter().rev().find(|s| id >= s.first).ok_or_else(|| at(format!("tile {id} is in no tileset")))?;
        let index = (id - sheet.first) as i32;
        if index as u32 >= sheet.count {
            return Err(at(format!("tile {id} is beyond its tileset")));
        }
        let (x0, y0) = (sheet.margin + (index % sheet.columns) * (n + sheet.spacing), sheet.margin + (index / sheet.columns) * (n + sheet.spacing));
        Ok((0..n * n).map(|i| sheet.pixels.get((x0 + i % n + (y0 + i / n) * sheet.width) as usize).copied().unwrap_or([0; 4])).collect())
    };

    // The tile layers, back to front, and the objects.
    let mut layers: Vec<&Layer> = Vec::new();
    fn flatten<'a>(list: &'a [Layer], out: &mut Vec<&'a Layer>) {
        for l in list {
            if l.kind == "group" {
                flatten(&l.layers, out);
            } else {
                out.push(l);
            }
        }
    }
    flatten(&map.layers, &mut layers);
    let (cols, rows) = (map.width, map.height);
    let mut tile_layers: Vec<Vec<u32>> = Vec::new();
    let mut things = Vec::new();
    for layer in layers.into_iter().filter(|l| l.visible) {
        match layer.kind.as_str() {
            "tilelayer" => {
                if layer.encoding.as_deref().is_some_and(|e| e != "csv") {
                    return Err(at(format!("layer '{}' is stored as {}; save tile layers as CSV", layer.name, layer.encoding.as_deref().unwrap_or(""))));
                }
                let data: Vec<u32> = match &layer.data {
                    Some(v) => serde_json::from_value(v.clone()).map_err(|_| at(format!("layer '{}': its tiles are no list of numbers", layer.name)))?,
                    None => Vec::new(),
                };
                if data.len() != (cols * rows) as usize {
                    return Err(at(format!("layer '{}' has {} tiles, not {cols} by {rows}", layer.name, data.len())));
                }
                tile_layers.push(data);
            }
            "objectgroup" => {
                for o in &layer.objects {
                    let kind = [&o.class, &o.kind, &o.name].into_iter().find(|k| !k.is_empty()).cloned().unwrap_or_default();
                    if kind.is_empty() {
                        return Err(at(format!("object layer '{}': an object without a class, type or name", layer.name)));
                    }
                    // Where it stands: a point, or the middle of a shape's
                    // (or a tile's) bottom edge.
                    let (x, y) = if o.point {
                        (o.x, o.y)
                    } else if o.gid.is_some() {
                        (o.x + o.width * 0.5, o.y)
                    } else {
                        (o.x + o.width * 0.5, o.y + o.height)
                    };
                    things.push(Thing::new(kind, [x.round() as i32, n / 2, ((rows * n) as f32 - y).round() as i32]));
                }
            }
            _ => {}
        }
    }
    if tile_layers.is_empty() {
        return Err(at("no tile layer".into()));
    }
    // Rows from the front: the top layer first. One layer splits: what
    // holds and hurts in front, the decoration behind.
    tile_layers.reverse();
    if tile_layers.len() == 1 {
        let all = tile_layers.pop().unwrap();
        let back: Vec<u32> = all.iter().map(|&raw| if raw & ID != 0 && kind(raw & ID) == Kind::Decor { raw } else { 0 }).collect();
        let front: Vec<u32> = all.iter().zip(&back).map(|(&raw, &b)| if b == 0 { raw } else { 0 }).collect();
        tile_layers.push(front);
        if back.iter().any(|&raw| raw != 0) {
            tile_layers.push(back);
        }
    }

    // The blocks the map uses, by id.
    let used: BTreeSet<u32> = tile_layers.iter().flatten().map(|&raw| raw & ID).filter(|&id| id != 0).collect();
    let mut level = GridLevel::new(n, IVec3::new(cols, tile_layers.len() as i32, rows));
    let mut made = Vec::new();
    for &id in &used {
        level.blocks.push(BlockEntry { name: id.to_string(), kind: kind(id) });
        made.push((id.to_string(), Block::drawn_out(&picture(id)?, n).with_context(|| format!("{}: tile {id}", path.display()))?));
    }
    for (y, data) in tile_layers.iter().enumerate() {
        for (i, &raw) in data.iter().enumerate() {
            let id = raw & ID;
            if id == 0 {
                continue;
            }
            let (col, row) = (i as i32 % cols, i as i32 / cols);
            let block = used.iter().position(|&u| u == id).unwrap() as u16 + 1;
            let turn = Turn::from_tiled(raw & FLIP_X != 0, raw & FLIP_Y != 0, raw & FLIP_DIAGONAL != 0);
            let c = level.index(IVec3::new(col, y as i32, rows - 1 - row)).unwrap();
            level.cells[c] = Cell { block, turn };
        }
    }
    level.things = things;

    // Every tile as a grid of whole tiles, tile `id` at place `id - 1`.
    let last = sheets.iter().map(|s| s.first + s.count - 1).max().unwrap_or(0);
    let columns = sheets.first().map_or(1, |s| s.columns) as u32;
    let size = n as u32;
    let (width, height) = (columns * size, last.div_ceil(columns) * size);
    let mut pixels = vec![[0u8; 4]; (width * height) as usize];
    for s in &sheets {
        for id in s.first..s.first + s.count {
            let (x0, y0) = ((id - 1) % columns * size, (id - 1) / columns * size);
            for (i, px) in picture(id)?.into_iter().enumerate() {
                let (x, y) = (x0 + i as u32 % size, y0 + i as u32 / size);
                pixels[(x + y * width) as usize] = px;
            }
        }
    }
    Ok(Imported { level, blocks: made, sheet: (width, height, pixels) })
}

/// Brings the Tiled map `map` into the game folder `dir`: its level
/// (`file`, not over one there unless `over`), a block for every tile it
/// uses that the game does not have yet, the tileset. Says what it did.
pub fn import_into(dir: &Path, map: &Path, file: &str, over: bool) -> Result<String> {
    let target = dir.join(file);
    if target.exists() && !over {
        bail!("{} is there already; --over replaces it (the blocks made for it stay)", target.display());
    }
    let imported = import(map)?;
    let (mut new, mut kept) = (0, 0);
    for (name, block) in &imported.blocks {
        let path = blocks::path(dir, name);
        if path.exists() {
            kept += 1;
        } else {
            block.save(&path)?;
            new += 1;
        }
    }
    let (w, h, pixels) = &imported.sheet;
    let rgba: Vec<u8> = pixels.iter().flatten().copied().collect();
    crate::headless::write_png(&dir.join(blocks::FOLDER).join(SHEET), *w, *h, &rgba)?;
    imported.level.save(&target)?;
    let s = imported.level.size;
    Ok(format!(
        "{}: {} by {} by {} cells of {} voxels, {} things; blocks: {new} drawn out, {kept} kept",
        target.display(),
        s.x,
        s.y,
        s.z,
        imported.level.n,
        imported.level.things.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture as RGBA pixels, written as a PNG.
    fn write_png(path: &Path, width: u32, height: u32, pixels: &[[u8; 4]]) {
        let data: Vec<u8> = pixels.iter().flatten().copied().collect();
        crate::headless::write_png(path, width, height, &data).unwrap();
    }

    const RED: [u8; 4] = [200, 40, 40, 255];
    const GREEN: [u8; 4] = [40, 200, 40, 255];
    const BLUE: [u8; 4] = [40, 40, 200, 255];
    const NONE: [u8; 4] = [0, 0, 0, 0];

    /// A folder with a tileset of three 2x2 tiles (1 red with a green top
    /// row, solid; 2 blue, no class; 3 red with a hole at its top right,
    /// one-way) and a 3 by 2 map with these tile layers.
    fn folder(name: &str, layers: &[&[u32]], objects: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("tiled-{name}"));
        // Tiles side by side: 6 by 2 pixels.
        let sheet = [GREEN, GREEN, BLUE, BLUE, RED, NONE, RED, RED, BLUE, BLUE, RED, RED];
        write_png(&dir.join("tiles.png"), 6, 2, &sheet);
        let layers: Vec<String> = layers.iter().enumerate().map(|(i, d)| format!(r#"{{"type":"tilelayer","name":"layer {i}","data":{d:?}}}"#)).collect();
        let map = format!(
            r#"{{"width":3,"height":2,"tilewidth":2,"tileheight":2,"infinite":false,
            "layers":[{},
                      {{"type":"objectgroup","name":"things","objects":[{objects}]}}],
            "tilesets":[{{"firstgid":1,"image":"tiles.png","columns":3,"tilecount":3,"margin":0,"spacing":0,
                          "tiles":[{{"id":0,"type":"solid"}},{{"id":2,"class":"oneway"}}]}}]}}"#,
            layers.join(",")
        );
        std::fs::write(dir.join("map.json"), map).unwrap();
        dir
    }

    /// Each tile layer is a row of cells, the top layer in front; every
    /// tile used is a block drawn out of its picture; the tileset is kept
    /// as whole tiles.
    #[test]
    fn a_tiled_map_becomes_rows_of_blocks() {
        let dir = folder("rows", &[&[0, 0, 0, 2, 2, 2], &[1, 0, 0, 0, 0, 3]], "");
        let got = import(&dir.join("map.json")).unwrap();
        let lv = &got.level;
        assert_eq!((lv.n, lv.size), (2, IVec3::new(3, 2, 2)));
        let names: Vec<_> = lv.blocks.iter().map(|b| (b.name.as_str(), b.kind)).collect();
        assert_eq!(names, [("1", Kind::Solid), ("2", Kind::Decor), ("3", Kind::Oneway)]);
        // The top layer in front: tile 1 at the top left, 3 at the bottom right.
        assert_eq!(lv.cell(IVec3::new(0, 0, 1)).block, 1);
        assert_eq!(lv.cell(IVec3::new(2, 0, 0)).block, 3);
        assert_eq!(lv.cell(IVec3::new(1, 1, 0)).block, 2);
        assert_eq!(lv.cell(IVec3::new(1, 0, 0)).block, 0);
        let (_, first) = &got.blocks[0];
        assert_eq!(first.colour_at(IVec3::new(0, 1, 1)), Some([40, 200, 40]), "its picture through its depth: the top row green");
        assert_eq!(first.colour_at(IVec3::new(1, 0, 0)), Some([200, 40, 40]));
        assert_eq!(got.sheet.0, 6);
        assert_eq!(got.sheet.2[5], NONE);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A map of one layer splits: what holds in front, decoration behind.
    #[test]
    fn a_single_layer_splits_into_lane_and_back() {
        let dir = folder("split", &[&[0, 0, 0, 1, 2, 3]], "");
        let lv = import(&dir.join("map.json")).unwrap().level;
        assert_eq!(lv.size, IVec3::new(3, 2, 2));
        assert_eq!([0, 1, 2].map(|x| lv.cell(IVec3::new(x, 0, 0)).block), [1, 0, 3]);
        assert_eq!([0, 1, 2].map(|x| lv.cell(IVec3::new(x, 1, 0)).block), [0, 2, 0]);
        std::fs::remove_dir_all(&dir).ok();
        let dir = folder("all-front", &[&[0, 0, 0, 1, 0, 3]], "");
        assert_eq!(import(&dir.join("map.json")).unwrap().level.size.y, 1, "nothing to put behind");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A tile flipped and turned in the map lies so in its cell.
    #[test]
    fn flipped_tiles_become_turned_blocks() {
        let dir = folder("flips", &[&[0, 0, 0, 1 | FLIP_X | FLIP_DIAGONAL, 1 | FLIP_Y, 0]], "");
        let lv = import(&dir.join("map.json")).unwrap().level;
        assert_eq!(lv.cell(IVec3::new(0, 0, 0)).turn, Turn::from_tiled(true, false, true));
        assert_eq!(lv.cell(IVec3::new(1, 0, 0)).turn, Turn::from_tiled(false, true, false));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Objects stand at a point, or at the middle of their bottom edge, in
    /// the front row's middle; their kind is their class, type or name.
    #[test]
    fn objects_become_things() {
        let objects = r#"{"name":"gem","x":3,"y":1,"point":true},
                         {"type":"miner","x":0,"y":0,"width":2,"height":2}"#;
        let dir = folder("objects", &[&[0, 0, 0, 1, 0, 0]], objects);
        let lv = import(&dir.join("map.json")).unwrap().level;
        assert_eq!(lv.things, [Thing::new("gem", [3, 1, 3]), Thing::new("miner", [1, 1, 2])]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A tile's class in the tileset is its block's kind; one that is none
    /// is named.
    #[test]
    fn tile_classes_become_kinds() {
        let dir = folder("classes", &[&[0, 0, 0, 1, 2, 3]], "");
        let text = std::fs::read_to_string(dir.join("map.json")).unwrap();
        std::fs::write(dir.join("map.json"), text.replace(r#""class":"oneway""#, r#""class":"ladder""#)).unwrap();
        let lv = import(&dir.join("map.json")).unwrap().level;
        assert_eq!(lv.blocks[2].kind, Kind::Ladder);
        std::fs::write(dir.join("map.json"), text.replace(r#""class":"oneway""#, r#""class":"lava""#)).unwrap();
        let e = import(&dir.join("map.json")).err().unwrap().to_string();
        assert!(e.contains("tile 3: class 'lava' is no kind of block"), "{e}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Bringing a map into a game keeps the blocks made for it, and the
    /// level there unless told.
    #[test]
    fn importing_keeps_the_blocks_made_already() {
        let dir = folder("into", &[&[0, 0, 0, 1, 2, 3]], "");
        let mine = Block::empty(2);
        mine.save(&blocks::path(&dir, "2")).unwrap();
        let said = import_into(&dir, &dir.join("map.json"), "level.toml", false).unwrap();
        assert!(said.contains("2 drawn out, 1 kept"), "{said}");
        assert_eq!(Block::load(&blocks::path(&dir, "2"), 2).unwrap().cells(), mine.cells());
        assert!(dir.join("blocks").join(SHEET).exists());
        let e = import_into(&dir, &dir.join("map.json"), "level.toml", false).unwrap_err().to_string();
        assert!(e.contains("--over"), "{e}");
        assert!(import_into(&dir, &dir.join("map.json"), "level.toml", true).is_ok());
        let level = GridLevel::read(&dir.join("level.toml")).unwrap();
        assert_eq!(level.blocks.len(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }
}

//! A model's file as the editor shapes it: its frames, each a box of
//! voxels (`Block`, of any size), and what the file holds besides. Glass
//! and metal (MagicaVoxel's materials) stay on the colours they were on;
//! MagicaVoxel's layout of the frames stays while they are as many and as
//! big as it was made for, else they are laid out anew side by side, as
//! the generators do. Voxels a file holds outside its frame's box (a
//! generator's slip; the game draws them) widen the box rather than being
//! lost; of a voxel there twice the first stays, as the game shows it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use dot_vox::{DotVoxData, SceneNode};
use glam::IVec3;

use crate::blocks::Block;

/// A model file being shaped.
#[derive(Clone)]
pub struct ModelFile {
    /// Where it lies.
    pub path: PathBuf,
    pub frames: Vec<Block>,
    /// The materials that are not plain (glass, metal, ...), by the colour
    /// they were on.
    materials: Vec<([u8; 3], dot_vox::Dict)>,
    /// MagicaVoxel's scene and layers, and the frames' sizes they were
    /// made for.
    scenes: Vec<SceneNode>,
    layers: Vec<dot_vox::Layer>,
    laid_out: Vec<IVec3>,
}

impl ModelFile {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let data = dot_vox::load_bytes(&bytes).map_err(|e| anyhow!("{}: {e}", path.display()))?;
        if data.models.is_empty() {
            bail!("{}: contains no model", path.display());
        }
        let rgb = |i: usize| data.palette.get(i).map_or([0; 3], |c| [c.r, c.g, c.b]);
        let frames: Vec<Block> = data
            .models
            .iter()
            .map(|m| {
                let at = |v: &dot_vox::Voxel| IVec3::new(v.x as i32, v.y as i32, v.z as i32);
                let size = m.voxels.iter().fold(IVec3::new(m.size.x as i32, m.size.y as i32, m.size.z as i32), |s, v| s.max(at(v) + 1));
                let mut block = Block::boxed(size);
                block.palette = (0..255).map(rgb).collect();
                for v in &m.voxels {
                    // A voxel twice in one place: the first, which the
                    // game shows (drawn first, it keeps its place).
                    if let Some(k) = block.index(at(v))
                        && block.voxels[k] == 0
                    {
                        block.voxels[k] = v.i.saturating_add(1);
                    }
                }
                block.compact();
                block
            })
            .collect();
        // A material's number is its colour's in the palette, from 1.
        let materials = data
            .materials
            .iter()
            .filter(|m| m.material_type().is_some_and(|t| t != "_diffuse") && m.id >= 1)
            .map(|m| (rgb(m.id as usize - 1), m.properties.clone()))
            .collect();
        let laid_out = frames.iter().map(|f| f.size).collect();
        Ok(Self { path: path.to_path_buf(), frames, materials, scenes: data.scenes, layers: data.layers, laid_out })
    }

    /// Every colour of its frames, each once, in the order they come.
    pub fn colours(&self) -> Vec<[u8; 3]> {
        let mut all = Vec::new();
        for c in self.frames.iter().flat_map(Block::colours) {
            if !all.contains(&c) {
                all.push(c);
            }
        }
        all
    }

    /// Writes it at once (`save_at_once`), its colours in one palette.
    pub fn save(&self) -> Result<()> {
        let colours = self.colours();
        if colours.len() > 255 {
            bail!("a model holds at most 255 colours, this one {}", colours.len());
        }
        let models = self
            .frames
            .iter()
            .map(|f| {
                let voxels = (0..f.voxels.len())
                    .filter_map(|k| {
                        let c = f.colour_at(f.at(k))?;
                        let i = colours.iter().position(|&o| o == c).expect("every colour listed") as u8;
                        let p = f.at(k);
                        Some(dot_vox::Voxel { x: p.x as u8, y: p.y as u8, z: p.z as u8, i })
                    })
                    .collect();
                dot_vox::Model { size: dot_vox::Size { x: f.size.x as u32, y: f.size.y as u32, z: f.size.z as u32 }, voxels }
            })
            .collect();
        let mut palette = vec![dot_vox::Color { r: 0, g: 0, b: 0, a: 255 }; 256];
        for (entry, &[r, g, b]) in palette.iter_mut().zip(&colours) {
            *entry = dot_vox::Color { r, g, b, a: 255 };
        }
        let materials = colours
            .iter()
            .enumerate()
            .filter_map(|(i, c)| self.materials.iter().find(|(m, _)| m == c).map(|(_, p)| dot_vox::Material { id: i as u32 + 1, properties: p.clone() }))
            .collect();
        let sizes: Vec<IVec3> = self.frames.iter().map(|f| f.size).collect();
        let (scenes, layers) = if sizes == self.laid_out { (self.scenes.clone(), self.layers.clone()) } else { layout(&sizes) };
        let data = DotVoxData { version: 150, index_map: Vec::new(), models, palette, materials, scenes, layers };
        let mut bytes = Vec::new();
        data.write_vox(&mut bytes)?;
        crate::blocks::save_at_once(&self.path, &bytes)
    }
}

/// MagicaVoxel's scene for frames `sizes` big: side by side along x, two
/// voxels apart, standing on the ground (none for a single frame).
fn layout(sizes: &[IVec3]) -> (Vec<SceneNode>, Vec<dot_vox::Layer>) {
    if sizes.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let dict = |pairs: &[(&str, String)]| pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<dot_vox::Dict>();
    let ids: Vec<u32> = (0..sizes.len() as u32).map(|k| 2 + 2 * k).collect();
    let mut scenes = vec![
        SceneNode::Transform { attributes: dict(&[]), frames: vec![dot_vox::Frame::new(dict(&[]))], child: 1, layer_id: u32::MAX },
        SceneNode::Group { attributes: dict(&[]), children: ids.clone() },
    ];
    let mut x = 0;
    for (k, size) in sizes.iter().enumerate() {
        let at = format!("{} 0 {}", x + size.x / 2, size.z / 2);
        x += size.x + 2;
        scenes.push(SceneNode::Transform { attributes: dict(&[]), frames: vec![dot_vox::Frame::new(dict(&[("_t", at)]))], child: ids[k] + 1, layer_id: 0 });
        scenes.push(SceneNode::Shape { attributes: dict(&[]), models: vec![dot_vox::ShapeModel { model_id: k as u32, attributes: dict(&[]) }] });
    }
    (scenes, vec![dot_vox::Layer { attributes: dict(&[]) }])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every model and block of every game and template, saved by the editor unchanged,
    /// is the same to the game: its frames, colours, glass and metal, its
    /// size (wider only where the file had voxels outside its box), and
    /// MagicaVoxel's layout of them.
    #[test]
    fn every_model_saved_unchanged_is_the_same_to_the_game() {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|e| e == "vox") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(Path::new("games"), &mut files);
        walk(Path::new("templates"), &mut files);
        assert!(files.len() > 100, "the games' and templates' models: {}", files.len());
        let copy = std::env::temp_dir().join(format!("voxcade-frames-{}.vox", std::process::id()));
        let sorted = |m: &crate::vox::VoxModel| -> Vec<Vec<(IVec3, [u8; 4], u8)>> {
            m.frames
                .iter()
                .zip(&m.metal)
                .map(|(f, metal)| {
                    // What the game shows: of a voxel there twice the
                    // first.
                    let mut seen = std::collections::BTreeSet::new();
                    let mut v: Vec<(IVec3, [u8; 4], u8)> = f.iter().zip(metal).filter(|((p, _), _)| seen.insert(p.to_array())).map(|(&(p, c), &m)| (p, c, m)).collect();
                    v.sort_by_key(|(p, ..)| (p.z, p.y, p.x));
                    v
                })
                .collect()
        };
        for file in files {
            let mut model = ModelFile::load(&file).unwrap();
            model.path = copy.clone();
            model.save().unwrap();
            let (a, b) = (crate::vox::load_model(&file).unwrap(), crate::vox::load_model(&copy).unwrap());
            let stray = b.size != a.size;
            assert!(!stray || a.frames.iter().flatten().any(|(p, _)| !p.cmplt(a.size).all()), "{}: its size", file.display());
            assert!(sorted(&a) == sorted(&b), "{}: its voxels", file.display());
            assert_eq!(ModelFile::load(&copy).unwrap().scenes.len(), model.scenes.len(), "{}: its layout", file.display());
        }
        let _ = std::fs::remove_file(&copy);
    }
}

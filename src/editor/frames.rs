//! A model's file as the editor shapes it: its frames, each a box of
//! voxels (`Block`, of any size), and what the file holds besides. Glass
//! and metal (MagicaVoxel's materials) stay on the colours they were on;
//! MagicaVoxel's layout of the frames stays while they are as many and as
//! big as it was made for, else they are laid out anew side by side, as
//! the generators do. Voxels a file holds outside its frame's box (a
//! generator's slip; the game draws them) widen the box rather than being
//! lost; of a voxel there twice the first stays, as the game shows it.
//!
//! A figure of parts (its objects named, `vox::Part`) keeps its scene
//! whatever happens to its parts' boxes: each part stays where it stands
//! (`Placed::low`; its translation written anew to match its box), so the
//! file stays a figure with every part in its place, and the parent and
//! joint set by hand are written onto the part's node (`_parent`,
//! `_joint`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use dot_vox::{DotVoxData, SceneNode};
use glam::{IVec3, Vec3};

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
    /// The file as it was read and its frames as they came of it: saved
    /// unchanged it is those bytes again (a figure's joints may hang on
    /// the order of its voxels, `vox::Hung::Nearest`).
    bytes: Vec<u8>,
    loaded: Vec<Block>,
    /// Each frame's voxels in the order the file had them: changed, a
    /// frame keeps that order for the voxels it still has.
    order: Vec<Vec<IVec3>>,
    /// A figure's parts as the scene places them and as their rig is set
    /// by hand (none for a model of frames), and as they were read.
    pub parts: Vec<Placed>,
    loaded_parts: Vec<Placed>,
}

/// A part of a figure as its file places it: its name, the frame (model)
/// it is made of, where that frame's voxel 0, 0, 0 stands in the scene
/// (`vox::Part`'s space, the translations above added in), the parent
/// and joint set by hand (the joint in the scene too), and the nodes it
/// is written to.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub name: String,
    pub frame: usize,
    pub low: IVec3,
    pub parent: Option<String>,
    pub joint: Option<Vec3>,
    /// The node carrying its name (and its rig), the one whose `_t` places
    /// it, and the translations above that one.
    named: usize,
    placed: usize,
    above: IVec3,
}

/// What of a model file an editor's step takes and gives back: its frames
/// and a figure's parts as placed and rigged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shaped {
    pub frames: Vec<Block>,
    pub parts: Vec<Placed>,
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
        let order = data.models.iter().map(|m| m.voxels.iter().map(|v| IVec3::new(v.x as i32, v.y as i32, v.z as i32)).collect()).collect();
        // A figure's parts where the game puts them (`vox::figure_of`); a
        // joint the file cannot read is left for the figure to say.
        let parts: Vec<Placed> = crate::vox::named_objects(&data)
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o.model < data.models.len())
            .map(|o| {
                let s = data.models[o.model].size;
                let half = IVec3::new(s.x as i32 / 2, s.y as i32 / 2, s.z as i32 / 2);
                let joint = o.joint.as_deref().and_then(crate::vox::read_joint).map(|j| j + o.above.as_vec3());
                Placed { name: o.name, frame: o.model, low: o.at - half, parent: o.parent, joint, named: o.named, placed: o.placed, above: o.above }
            })
            .collect();
        Ok(Self { path: path.to_path_buf(), loaded: frames.clone(), frames, materials, scenes: data.scenes, layers: data.layers, laid_out, bytes, order, loaded_parts: parts.clone(), parts })
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
        Self { path: path.to_path_buf(), laid_out: vec![frame.size], frames: vec![frame], materials: Vec::new(), scenes: Vec::new(), layers: Vec::new(), bytes: Vec::new(), loaded: Vec::new(), order: Vec::new(), parts: Vec::new(), loaded_parts: Vec::new() }
    }

    /// Its frames and parts as a step keeps them.
    pub fn shaped(&self) -> Shaped {
        Shaped { frames: self.frames.clone(), parts: self.parts.clone() }
    }

    /// Its frames and parts as a step gives them back.
    pub fn reshape(&mut self, to: &Shaped) {
        self.frames = to.frames.clone();
        self.parts = to.parts.clone();
    }

    /// Whether it is as `shaped` holds it.
    pub fn is(&self, shaped: &Shaped) -> bool {
        self.frames == shaped.frames && self.parts == shaped.parts
    }

    /// It is a figure of parts (its objects named): its frames are its
    /// parts, not frames of a walk.
    pub fn parted(&self) -> bool {
        !self.parts.is_empty()
    }

    /// Where voxel 0, 0, 0 of frame `f` stands in the scene (a figure's
    /// part; the first part made of it).
    pub fn origin(&self, f: usize) -> Option<IVec3> {
        self.parts.iter().find(|p| p.frame == f).map(|p| p.low)
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

    /// Writes it at once (`save_at_once`), its colours in one palette; as
    /// it was read if its frames are as they came.
    pub fn save(&self) -> Result<()> {
        if self.unchanged() {
            return crate::blocks::save_at_once(&self.path, &self.bytes);
        }
        let mut bytes = Vec::new();
        self.data()?.write_vox(&mut bytes)?;
        crate::blocks::save_at_once(&self.path, &bytes)
    }

    /// Its frames are as they came of the file.
    fn unchanged(&self) -> bool {
        !self.bytes.is_empty() && self.frames == self.loaded && self.parts == self.loaded_parts
    }

    /// A figure of parts as the game would make it of the file saved now
    /// (`vox::figure_of`): the model, each part's frame and where the
    /// figure's low corner stands in the scene; None for a model of frames.
    pub fn figure(&self) -> Option<Result<(crate::vox::VoxModel, Vec<usize>, IVec3)>> {
        let data = match self.data() {
            Ok(data) => data,
            Err(e) => return Some(Err(e)),
        };
        crate::vox::figure_of(&data, &self.path)
    }

    /// What the file holds as it would be saved now.
    fn data(&self) -> Result<DotVoxData> {
        if self.unchanged() {
            return dot_vox::load_bytes(&self.bytes).map_err(|e| anyhow!("{}: {e}", self.path.display()));
        }
        let colours = self.colours();
        if colours.len() > 255 {
            bail!("a model holds at most 255 colours, this one {}", colours.len());
        }
        // The voxels in the order the file had them, then the new ones (the
        // frames as many as they were).
        let kept = |k: usize| if self.order.len() == self.frames.len() { self.order[k].as_slice() } else { &[] };
        let models = self
            .frames
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let mut written = crate::world::CellSet::default();
                let voxels = kept(k)
                    .iter()
                    .copied()
                    .chain((0..f.voxels.len()).map(|k| f.at(k)))
                    .filter_map(|p| {
                        let c = f.colour_at(p)?;
                        if !written.insert(p) {
                            return None;
                        }
                        let i = colours.iter().position(|&o| o == c).expect("every colour listed") as u8;
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
        let (scenes, layers) = if self.parted() {
            (self.placed_scenes(), self.layers.clone())
        } else if sizes == self.laid_out {
            (self.scenes.clone(), self.layers.clone())
        } else {
            layout(&sizes)
        };
        Ok(DotVoxData { version: 150, index_map: Vec::new(), models, palette, materials, scenes, layers })
    }

    /// A figure's scene as it stands now: each part's translation such
    /// that its voxels stand where they stood (`Placed::low`, whatever its
    /// box became), its parent and joint set by hand written on it, or
    /// taken off.
    fn placed_scenes(&self) -> Vec<SceneNode> {
        let mut scenes = self.scenes.clone();
        for part in &self.parts {
            let s = self.frames.get(part.frame).map_or(IVec3::ZERO, |f| f.size);
            let t = part.low + s / 2 - part.above;
            if let Some(SceneNode::Transform { frames, .. }) = scenes.get_mut(part.placed) {
                if frames.is_empty() {
                    frames.push(dot_vox::Frame::new(Default::default()));
                }
                let was = frames[0].position().map(|p| IVec3::new(p.x, p.y, p.z));
                if was != Some(t) {
                    frames[0].attributes.insert("_t".into(), format!("{} {} {}", t.x, t.y, t.z));
                }
            }
            if let Some(SceneNode::Transform { attributes, .. }) = scenes.get_mut(part.named) {
                match &part.parent {
                    Some(name) => attributes.insert("_parent".into(), name.clone()),
                    None => attributes.remove("_parent"),
                };
                match part.joint {
                    Some(j) => attributes.insert("_joint".into(), crate::vox::write_joint(j - part.above.as_vec3())),
                    None => attributes.remove("_joint"),
                };
            }
        }
        scenes
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

    /// A figure of parts saved unchanged is the file it was, byte for byte;
    /// changed, every frame keeps the order of the voxels it still has (a
    /// part apart from the rest is joined by that order) and the figure
    /// keeps its parts and joints.
    #[test]
    fn a_figure_saved_keeps_its_bytes_and_its_order() {
        let dir = std::env::temp_dir().join(format!("voxcade-figure-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        crate::vox::write_figure(&file, &[("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("arm", IVec3::new(1, 2, 5), IVec3::new(6, 1, 2))]);
        // The voxels in an order of their own, as a hand-made file has.
        let mut data = dot_vox::load(file.to_str().unwrap()).unwrap();
        data.models.iter_mut().for_each(|m| m.voxels.reverse());
        data.write_vox(&mut std::fs::File::create(&file).unwrap()).unwrap();
        let bytes = std::fs::read(&file).unwrap();
        let order = |f: &Path| dot_vox::load(f.to_str().unwrap()).unwrap().models.iter().map(|m| m.voxels.iter().map(|v| (v.x, v.y, v.z)).collect::<Vec<_>>()).collect::<Vec<_>>();
        let rig = |f: &Path| crate::vox::load_model(f).unwrap().parts.iter().map(|p| (p.name.clone(), p.parent, p.joint, p.hung.clone())).collect::<Vec<_>>();
        let (was, hung) = (order(&file), rig(&file));

        let mut model = ModelFile::load(&file).unwrap();
        model.path = dir.join("copy.vox");
        model.save().unwrap();
        assert_eq!(std::fs::read(&model.path).unwrap(), bytes, "saved unchanged, other bytes");
        // A voxel of the head recoloured: every voxel where it was.
        let c = model.frames[1].colour_at(IVec3::ZERO).unwrap();
        model.frames[1].set(IVec3::ZERO, Some([c[0] ^ 1, c[1], c[2]])).unwrap();
        model.save().unwrap();
        assert_eq!(order(&model.path), was, "the voxels' order");
        assert_eq!(rig(&model.path), hung, "the parts and joints");
        let _ = std::fs::remove_dir_all(&dir);
    }

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

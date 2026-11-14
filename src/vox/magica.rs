//! MagicaVoxel's `.vox`, which a game only brings in (`import_model`,
//! `voxcade import`, `voxcade upgrade`): a model, a figure of the scene's
//! named objects, and an older game's world or prefab as a scene
//! (`load_scene`: every model of the scene, placed by its transform, its
//! colours by their palette index). Nothing writes `.vox` but the tests.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use dot_vox::{DotVoxData, SceneNode};
use glam::{IVec3, Vec3};

use super::*;
use crate::files::Files;

fn read(files: &Files, path: &Path) -> Result<DotVoxData> {
    let bytes = files.read(path).with_context(|| format!("reading {}", path.display()))?;
    dot_vox::load_bytes(&bytes).map_err(|e| anyhow!("{}: {e}", path.display()))
}

/// How opaque each file colour index is (255 for most): MagicaVoxel's glass
/// material by its transparency (`_trans`), its blend material by `_alpha`.
pub(super) fn see_through(data: &DotVoxData) -> [u8; 256] {
    let mut alpha = [255u8; 256];
    for m in &data.materials {
        let Some(opacity) = opacity(m) else { continue };
        if let Some(a) = alpha.get_mut(m.id as usize) {
            *a = (opacity.clamp(0.05, 1.0) * 255.0).round() as u8;
        }
    }
    alpha
}

/// How opaque a see-through material is: glass by its transparency
/// (`_trans`), or, written by vengi, which keeps only `_alpha`, by that;
/// blend by its alpha. None: an opaque material.
pub(super) fn opacity(m: &dot_vox::Material) -> Option<f32> {
    match m.material_type() {
        Some("_glass") => Some(m.transparency().map_or_else(|| m.opacity().unwrap_or(0.5), |t| 1.0 - t)),
        Some("_blend") => Some(m.opacity().unwrap_or(0.5)),
        _ => None,
    }
}

/// How each file colour index mirrors: MagicaVoxel's metal material by its
/// metalness (`_metal`) and roughness (`_rough`); its glass by its
/// roughness alone, not metal at all (glass written without one, as More
/// Blood's cockpit, mirrors nothing).
fn metals(data: &DotVoxData) -> [u8; 256] {
    let mut shine = [0u8; 256];
    for m in &data.materials {
        let packed = match m.material_type() {
            Some("_metal") => metal::pack(m.metalness().unwrap_or(1.0), 1.0 - m.roughness().unwrap_or(0.3)),
            Some("_glass") => m.roughness().map_or(0, |rough| metal::pack(0.0, 1.0 - rough)),
            _ => continue,
        };
        if let Some(s) = shine.get_mut(m.id as usize) {
            *s = packed;
        }
    }
    shine
}

/// How much each file colour index glows: MagicaVoxel's emission material
/// (`_type` `_emit`) by its emission (`_emit`, 0 to 1; 1 when the file
/// gives none), as `frame:cube`'s glow, 0 to 255. The other materials do
/// not glow.
fn glows(data: &DotVoxData) -> [u8; 256] {
    let mut glow = [0u8; 256];
    for m in &data.materials {
        if m.material_type() != Some("_emit") {
            continue;
        }
        if let Some(g) = glow.get_mut(m.id as usize) {
            *g = (m.emission().unwrap_or(1.0).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    glow
}

fn model_size(data: &DotVoxData, id: usize) -> IVec3 {
    let s = data.models[id].size;
    IVec3::new(s.x as i32, s.y as i32, s.z as i32)
}

/// A model on disk, whichever format: a `.vvx`, or a MagicaVoxel `.vox`
/// (what the editors and the import read; a game reads `load_model_in`).
pub fn load_model(path: &Path) -> Result<VoxModel> {
    if crate::vvx::is_model(path) { crate::vvx::load_in(&Files::disk(), path) } else { import_model(path) }
}

/// A model of the game's files (`crate::files`): a `.vvx`. A game holding
/// a MagicaVoxel `.vox` in its place is told how to bring it in.
pub fn load_model_in(files: &Files, path: &Path) -> Result<VoxModel> {
    if !crate::vvx::is_model(path) {
        return Err(not_a_model(path));
    }
    crate::vvx::load_in(files, path)
}

/// The mistake for a model file that is no `.vvx`.
pub fn not_a_model(path: &Path) -> anyhow::Error {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vox")) {
        let vvx = path.with_extension(crate::vvx::EXTENSION);
        anyhow!(
            "{}: a MagicaVoxel file; Voxcade's models are .vvx: `voxcade import <game> {}` brings it in as {} (and `voxcade upgrade <game>` a whole game made for an older Voxcade), then name the .vvx",
            path.display(),
            path.display(),
            vvx.file_name().unwrap_or_default().to_string_lossy()
        )
    } else {
        anyhow!("{}: a model is a .vvx file (Voxcade's own, text)", path.display())
    }
}

/// A MagicaVoxel `.vox` model on disk, as the engine made it of one before
/// it read its own format: what `import` and `upgrade` bring in.
pub fn import_model(path: &Path) -> Result<VoxModel> {
    model_of(&read(&Files::disk(), path)?, path)
}

/// The model a file's `data` makes (read from `path`).
fn model_of(data: &DotVoxData, path: &Path) -> Result<VoxModel> {
    if data.models.is_empty() {
        return Err(anyhow!("{}: contains no model", path.display()));
    }
    let alpha = see_through(data);
    let shine = metals(data);
    let glow = glows(data);
    if let Some(objects) = named_objects(data) {
        return figure_of_parts(path, data, &objects, &alpha, &shine, &glow).map(|(m, _)| m);
    }
    let metal = data.models.iter().map(|model| model.voxels.iter().map(|v| shine[v.i as usize + 1]).collect()).collect();
    let frames = data
        .models
        .iter()
        .map(|model| {
            model
                .voxels
                .iter()
                .map(|v| {
                    let c = data.palette[v.i as usize];
                    (IVec3::new(v.x as i32, v.y as i32, v.z as i32), [c.r, c.g, c.b, alpha[v.i as usize + 1]])
                })
                .collect()
        })
        .collect();
    let size = (0..data.models.len()).map(|id| model_size(data, id)).reduce(IVec3::max).unwrap();
    let mut model = VoxModel::new(size, frames, metal);
    if glow.iter().any(|&g| g != 0) {
        model.glow = data.models.iter().map(|m| m.voxels.iter().map(|v| glow[v.i as usize + 1]).collect()).collect();
    }
    Ok(model)
}

/// An object of a figure's scene: its name, its model, where its model's
/// middle stands (the translations down to it added up), the translations
/// above the transform that places it (the nearest above its shape, whose
/// `_t` it is), and what the file says of its rig (`Part`: `_parent`,
/// `_joint`, as written).
#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub name: String,
    pub model: usize,
    pub at: IVec3,
    pub above: IVec3,
    pub parent: Option<String>,
    pub joint: Option<String>,
}

/// The scene's objects, each a model placed where its transforms put it
/// (their translations added up; MagicaVoxel's turns are not read), when
/// every object has a name of its own: a figure of parts. None: the
/// models are frames.
pub(crate) fn named_objects(data: &DotVoxData) -> Option<Vec<Object>> {
    /// Down the scene: the translations so far, the node naming what is
    /// below and the transform last passed (its node, the translations
    /// above it).
    struct Down {
        at: IVec3,
        named: Option<usize>,
        placed: (usize, IVec3),
    }
    fn walk(scenes: &[SceneNode], node: usize, down: Down, out: &mut Vec<(Option<usize>, usize, Down)>) {
        match scenes.get(node) {
            Some(SceneNode::Transform { attributes, frames, child, .. }) => {
                let t = frames.first().and_then(|f| f.position()).map_or(IVec3::ZERO, |p| IVec3::new(p.x, p.y, p.z));
                let named = if attributes.contains_key("_name") { Some(node) } else { down.named };
                walk(scenes, *child as usize, Down { at: down.at + t, named, placed: (node, down.at) }, out);
            }
            Some(SceneNode::Group { children, .. }) => {
                for &c in children {
                    walk(scenes, c as usize, Down { ..down }, out);
                }
            }
            Some(SceneNode::Shape { models, .. }) => {
                for m in models {
                    out.push((down.named, m.model_id as usize, Down { ..down }));
                }
            }
            None => {}
        }
    }
    let mut found = Vec::new();
    walk(&data.scenes, 0, Down { at: IVec3::ZERO, named: None, placed: (0, IVec3::ZERO) }, &mut found);
    let attribute = |node: usize, key: &str| match data.scenes.get(node) {
        Some(SceneNode::Transform { attributes, .. }) => attributes.get(key).cloned(),
        _ => None,
    };
    let name = |named: Option<usize>| named.and_then(|n| attribute(n, "_name")).filter(|n| !n.is_empty());
    if found.len() < 2 || found.iter().any(|(named, ..)| name(*named).is_none()) {
        return None;
    }
    Some(
        found
            .into_iter()
            .map(|(named, model, down)| {
                let n = named.unwrap_or_default();
                let parent = attribute(n, "_parent").filter(|p| !p.trim().is_empty());
                Object { name: name(named).unwrap_or_default(), model, at: down.at, above: down.placed.1, parent, joint: attribute(n, "_joint") }
            })
            .collect(),
    )
}

/// A joint as the file writes it (`_joint`): three numbers apart.
pub(crate) fn read_joint(text: &str) -> Option<Vec3> {
    let n: Vec<f32> = text.split_whitespace().map(str::parse).collect::<std::result::Result<_, _>>().ok()?;
    match n[..] {
        [x, y, z] if n.iter().all(|v| v.is_finite()) => Some(Vec3::new(x, y, z)),
        _ => None,
    }
}

/// A figure of parts from the scene's named objects: one frame of all of
/// them as they stand, and the parts with their joints (`Part`); and
/// where the frame's low corner stands in the scene.
fn figure_of_parts(path: &Path, data: &DotVoxData, objects: &[Object], alpha: &[u8; 256], shine: &[u8; 256], glow: &[u8; 256]) -> Result<(VoxModel, IVec3)> {
    for (k, o) in objects.iter().enumerate() {
        if objects[..k].iter().any(|other| other.name == o.name) {
            return Err(anyhow!("{}: two parts called '{}' (the objects' names must differ)", path.display(), o.name));
        }
    }
    // Where every voxel stands in the scene (MagicaVoxel: a model's middle
    // at its translation), then from the low corner of them all.
    let mut placed = Vec::new();
    for (part, o) in objects.iter().enumerate() {
        let m = data.models.get(o.model).with_context(|| format!("{}: an object without its model", path.display()))?;
        let half = IVec3::new(m.size.x as i32 / 2, m.size.y as i32 / 2, m.size.z as i32 / 2);
        for v in &m.voxels {
            let c = data.palette[v.i as usize];
            placed.push((o.at + IVec3::new(v.x as i32, v.y as i32, v.z as i32) - half, [c.r, c.g, c.b, alpha[v.i as usize + 1]], shine[v.i as usize + 1], part, glow[v.i as usize + 1]));
        }
    }
    let low = placed.iter().fold(IVec3::MAX, |l, p| l.min(p.0));
    let high = placed.iter().fold(IVec3::MIN, |h, p| h.max(p.0));
    // What the file says of the rig, from the frame's low corner.
    let mut stated = Vec::new();
    for o in objects {
        let parent = match &o.parent {
            None => None,
            Some(name) => Some(objects.iter().position(|q| &q.name == name).ok_or_else(|| anyhow!("{}: part '{}' hangs on '{name}', but no part is called so (_parent)", path.display(), o.name))?),
        };
        let joint = match &o.joint {
            None => None,
            Some(text) => Some(read_joint(text).ok_or_else(|| anyhow!("{}: part '{}': its joint '{text}' is not three numbers (_joint)", path.display(), o.name))? + o.above.as_vec3() - low.as_vec3()),
        };
        stated.push((parent, joint));
    }
    let parents: Vec<Option<usize>> = stated.iter().map(|s| s.0).collect();
    let names: Vec<&str> = objects.iter().map(|o| o.name.as_str()).collect();
    if let Some(why) = ring_mistake(&parents, &names, "_parent") {
        return Err(anyhow!("{}: {why}", path.display()));
    }
    let frame: Vec<(IVec3, [u8; 4])> = placed.iter().map(|&(v, c, ..)| (v - low, c)).collect();
    let metal = placed.iter().map(|p| p.2).collect();
    let owner: Vec<usize> = placed.iter().map(|p| p.3).collect();
    let mut model = VoxModel::new(high - low + IVec3::ONE, vec![frame], vec![metal]);
    if placed.iter().any(|p| p.4 != 0) {
        model.glow = vec![placed.iter().map(|p| p.4).collect()];
    }
    model.parts = parts_of(&model.frames[0], &owner, objects.iter().map(|o| o.name.clone()).collect(), model.size, &stated);
    Ok((model, low))
}

/// Every model of a scene, put together: what levels and prefabs are made of.
pub struct Scene {
    /// Extent of all models together; voxels lie in 0..size.
    pub size: IVec3,
    /// RGBA per file colour index (1..=255; 0 is empty).
    pub palette: [[u8; 4]; 256],
    /// Position and file colour index of every voxel.
    pub voxels: Vec<(IVec3, u8)>,
}

pub fn load_scene(path: &Path) -> Result<Scene> {
    load_scene_in(&Files::disk(), path)
}

/// A scene of the game's files (`crate::files`).
pub fn load_scene_in(files: &Files, path: &Path) -> Result<Scene> {
    let data = read(files, path)?;

    // (model id, translation of the model centre)
    let mut placed = Vec::new();
    if data.scenes.is_empty() {
        placed.extend((0..data.models.len()).map(|id| (id, IVec3::ZERO)));
    } else {
        let mut rotated = false;
        walk(&data.scenes, 0, IVec3::ZERO, 0, &mut placed, &mut rotated);
        if rotated {
            eprintln!("warning: {}: rotated models are placed unrotated", path.display());
        }
    }
    placed.retain(|&(id, _)| id < data.models.len());
    if placed.is_empty() {
        return Err(anyhow!("{}: scene contains no model", path.display()));
    }

    // MagicaVoxel stores the model centre; we need the minimum corner.
    let origins: Vec<(usize, IVec3)> =
        placed.iter().map(|&(id, t)| (id, t - model_size(&data, id) / 2)).collect();
    let min = origins.iter().map(|&(_, o)| o).reduce(IVec3::min).unwrap();
    let max = origins.iter().map(|&(id, o)| o + model_size(&data, id)).reduce(IVec3::max).unwrap();

    // File colour indices are 1..=255; dot_vox stores them minus one.
    let mut palette = [[0u8; 4]; 256];
    for (index, entry) in palette.iter_mut().enumerate().skip(1) {
        if let Some(c) = data.palette.get(index - 1) {
            *entry = [c.r, c.g, c.b, 255];
        }
    }

    let mut voxels = Vec::new();
    for (id, origin) in origins {
        let o = origin - min;
        for v in &data.models[id].voxels {
            let p = o + IVec3::new(v.x as i32, v.y as i32, v.z as i32);
            voxels.push((p, v.i.saturating_add(1)));
        }
    }
    Ok(Scene { size: max - min, palette, voxels })
}

fn walk(
    nodes: &[SceneNode],
    index: u32,
    offset: IVec3,
    depth: u32,
    out: &mut Vec<(usize, IVec3)>,
    rotated: &mut bool,
) {
    if depth > 64 {
        return;
    }
    let Some(node) = nodes.get(index as usize) else { return };
    match node {
        SceneNode::Transform { frames, child, .. } => {
            let mut t = offset;
            if let Some(frame) = frames.first() {
                if let Some(p) = frame.position() {
                    t += IVec3::new(p.x, p.y, p.z);
                }
                if frame.attributes.get("_r").is_some_and(|r| r.as_str() != "4") {
                    *rotated = true;
                }
            }
            walk(nodes, *child, t, depth + 1, out, rotated);
        }
        SceneNode::Group { children, .. } => {
            for &c in children {
                walk(nodes, c, offset, depth + 1, out, rotated);
            }
        }
        SceneNode::Shape { models, .. } => {
            out.extend(models.iter().map(|m| (m.model_id as usize, offset)));
        }
    }
}

/// Writes a figure of parts for tests: each part a solid box of its own
/// colour (name, size, where its low corner stands), a named object; a
/// MagicaVoxel file, or brought in as a `.vvx` when the path says so.
#[cfg(test)]
pub(crate) fn write_figure(path: &Path, parts: &[(&str, IVec3, IVec3)]) {
    write_figure_with(path, parts, &[]);
}

/// The same with attributes of the parts' own (a part's name, the key, its
/// value: `_parent`, `_joint`).
#[cfg(test)]
pub(crate) fn write_figure_with(path: &Path, parts: &[(&str, IVec3, IVec3)], rig: &[(&str, &str, &str)]) {
    use dot_vox::{Color, Dict, Frame, Layer, Model, ShapeModel, Size, Voxel};
    let dict = |pairs: &[(&str, String)]| -> Dict { pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() };
    let models = parts
        .iter()
        .enumerate()
        .map(|(k, (_, size, _))| {
            let mut voxels = Vec::new();
            for z in 0..size.z {
                for y in 0..size.y {
                    for x in 0..size.x {
                        voxels.push(Voxel { x: x as u8, y: y as u8, z: z as u8, i: k as u8 });
                    }
                }
            }
            Model { size: Size { x: size.x as u32, y: size.y as u32, z: size.z as u32 }, voxels }
        })
        .collect();
    let palette = (0..256u32).map(|i| Color { r: (i * 60 % 256) as u8, g: (i * 90 % 256) as u8, b: (i * 130 % 256) as u8, a: 255 }).collect();
    let ids: Vec<u32> = (0..parts.len() as u32).map(|k| 2 + 2 * k).collect();
    let mut scenes = vec![
        SceneNode::Transform { attributes: dict(&[]), frames: vec![Frame::new(dict(&[]))], child: 1, layer_id: u32::MAX },
        SceneNode::Group { attributes: dict(&[]), children: ids.clone() },
    ];
    for (k, (name, size, low)) in parts.iter().enumerate() {
        let at = *low + *size / 2;
        let mut attributes = dict(&[("_name", name.to_string())]);
        attributes.extend(rig.iter().filter(|r| r.0 == *name).map(|r| (r.1.to_string(), r.2.to_string())));
        scenes.push(SceneNode::Transform { attributes, frames: vec![Frame::new(dict(&[("_t", format!("{} {} {}", at.x, at.y, at.z))]))], child: ids[k] + 1, layer_id: 0 });
        scenes.push(SceneNode::Shape { attributes: dict(&[]), models: vec![ShapeModel { model_id: k as u32, attributes: dict(&[]) }] });
    }
    let data = DotVoxData { version: 150, index_map: Vec::new(), models, palette, materials: Vec::new(), scenes, layers: vec![Layer { attributes: dict(&[]) }] };
    // A `.vvx` path gets the figure brought in as text, as a game holds it.
    let vox = if crate::vvx::is_model(path) { path.with_extension("vvx.vox") } else { path.to_path_buf() };
    let mut file = std::fs::File::create(&vox).unwrap();
    data.write_vox(&mut file).unwrap();
    if vox != path {
        crate::upgrade::bring_in(&vox, path, true).unwrap();
    }
}

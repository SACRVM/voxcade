//! Loading MagicaVoxel `.vox` files: levels and prefabs (every model of the
//! scene, placed by its transform) and actors (every model is one animation
//! frame; or, when the scene's objects have names, the figure's parts).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use dot_vox::{DotVoxData, SceneNode};
use glam::{IVec3, Vec3};

use crate::files::Files;
use crate::world::Marker;
use crate::world::World;

/// An actor model with its own colours. Each model in the file is one frame:
/// frame 0 stands still, frames 1.. are the walk cycle.
pub struct VoxModel {
    /// Size of the largest frame.
    pub size: IVec3,
    /// Per frame: position inside the model and RGBA colour; alpha under
    /// 255 is see-through (glass).
    pub frames: Vec<Vec<(IVec3, [u8; 4])>>,
    /// Per frame and voxel (as in `frames`): how metal it is, packed as
    /// `metal::pack` (0: not metal).
    pub metal: Vec<Vec<u8>>,
    /// Per frame and voxel: a direction it is lit by (xyz, snorm) and in w
    /// the faces that use it (bits in `mesh::DIRS` order, `ALL_FACES`: the
    /// whole voxel); the rest, and all when empty, by their own direction.
    /// See `shade`.
    pub shade: Vec<Vec<[i8; 4]>>,
    /// Per frame and voxel: the way the surface faces where a voxel of
    /// smooth glass lies (snorm xyz; zero inside and for the rest), what it
    /// mirrors and glints by: a cut stone's facets, not its voxels' faces.
    /// Empty for a model without smooth glass.
    pub facets: Vec<Vec<[i8; 3]>>,
    /// Per frame: which voxels (indices into `frames`) can be seen at all,
    /// the ones with open space or glass beside them and glass itself. The
    /// rest are never drawn (most of a big model); the simulation still
    /// bursts a body into all of them.
    pub visible: Vec<Vec<u32>>,
    /// Per frame: the visible voxels that are see-through. They are drawn
    /// as cubes every frame, sorted with the other see-through ones; the
    /// opaque ones lie on the GPU (`bank`).
    pub glass: Vec<Vec<u32>>,
    /// Per frame: where its opaque visible voxels lie in the game's model
    /// bank (`render::ModelBank::add`); empty until it is put there, and
    /// then the model is drawn as cubes.
    pub bank: Vec<std::ops::Range<u32>>,
    /// A figure made of parts (the file's objects have names): each part
    /// of its one frame, turned about its joint when posed (`Part`); none
    /// for a model of frames.
    pub parts: Vec<Part>,
}

/// A part of a figure: its name (the object's in the file), its voxels
/// (indices into frame 0, the whole figure), the part it hangs on and
/// moves with, and the joint it turns about, in voxels from the model's
/// pivot cell's minimum corner (`ModelBank`'s). The biggest part is the
/// root, turning about its own middle; every other part hangs on the one
/// it touches most (more of their faces meeting than any other's, among
/// those already hung: a forearm on its upper arm), turning about the
/// middle of where they meet; a part touching none hangs on the nearest,
/// about the middle between their nearest voxels.
#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub voxels: Vec<u32>,
    pub parent: Option<usize>,
    pub joint: Vec3,
    /// Its voxels that can be seen once it moves apart from the rest (its
    /// own surface, the joint's faces too), and of those the see-through.
    pub visible: Vec<u32>,
    pub glass: Vec<u32>,
    /// Its opaque visible voxels in the model bank.
    pub bank: std::ops::Range<u32>,
}

/// How a voxel mirrors, packed into a byte: metalness in the high four
/// bits, smoothness in the low four (what the shader unpacks). Smooth and
/// not metal at all is glass that mirrors a little.
pub mod metal {
    pub fn pack(metalness: f32, smoothness: f32) -> u8 {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 15.0).round() as u8;
        (q(metalness) << 4) | q(smoothness)
    }
}

fn read(files: &Files, path: &Path) -> Result<DotVoxData> {
    let bytes = files.read(path).with_context(|| format!("reading {}", path.display()))?;
    dot_vox::load_bytes(&bytes).map_err(|e| anyhow!("{}: {e}", path.display()))
}

/// How opaque each file colour index is (255 for most): MagicaVoxel's glass
/// material by its transparency (`_trans`), its blend material by `_alpha`.
fn see_through(data: &DotVoxData) -> [u8; 256] {
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
fn opacity(m: &dot_vox::Material) -> Option<f32> {
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

fn model_size(data: &DotVoxData, id: usize) -> IVec3 {
    let s = data.models[id].size;
    IVec3::new(s.x as i32, s.y as i32, s.z as i32)
}

pub fn load_model(path: &Path) -> Result<VoxModel> {
    load_model_in(&Files::disk(), path)
}

/// A model of the game's files (`crate::files`).
pub fn load_model_in(files: &Files, path: &Path) -> Result<VoxModel> {
    let data = read(files, path)?;
    if data.models.is_empty() {
        return Err(anyhow!("{}: contains no model", path.display()));
    }
    let alpha = see_through(&data);
    let shine = metals(&data);
    if let Some(objects) = named_objects(&data) {
        return figure_of_parts(path, &data, &objects, &alpha, &shine);
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
    let size = (0..data.models.len()).map(|id| model_size(&data, id)).reduce(IVec3::max).unwrap();
    Ok(VoxModel::new(size, frames, metal))
}

/// The scene's objects, each a model placed where its transforms put it
/// (their translations added up; MagicaVoxel's turns are not read), when
/// every object has a name of its own: a figure of parts. None: the
/// models are frames.
fn named_objects(data: &DotVoxData) -> Option<Vec<(String, usize, IVec3)>> {
    fn walk(scenes: &[SceneNode], node: usize, at: IVec3, name: Option<&str>, out: &mut Vec<(Option<String>, usize, IVec3)>) {
        match scenes.get(node) {
            Some(SceneNode::Transform { attributes, frames, child, .. }) => {
                let t = frames.first().and_then(|f| f.position()).map_or(IVec3::ZERO, |p| IVec3::new(p.x, p.y, p.z));
                let name = attributes.get("_name").map(String::as_str).or(name);
                walk(scenes, *child as usize, at + t, name, out);
            }
            Some(SceneNode::Group { children, .. }) => {
                for &c in children {
                    walk(scenes, c as usize, at, name, out);
                }
            }
            Some(SceneNode::Shape { models, .. }) => {
                for m in models {
                    out.push((name.map(str::to_string), m.model_id as usize, at));
                }
            }
            None => {}
        }
    }
    let mut found = Vec::new();
    walk(&data.scenes, 0, IVec3::ZERO, None, &mut found);
    if found.len() < 2 || found.iter().any(|(name, ..)| name.as_deref().is_none_or(str::is_empty)) {
        return None;
    }
    Some(found.into_iter().map(|(name, model, at)| (name.unwrap_or_default(), model, at)).collect())
}

/// A figure of parts from the scene's named objects: one frame of all of
/// them as they stand, and the parts with their joints (`Part`).
fn figure_of_parts(path: &Path, data: &DotVoxData, objects: &[(String, usize, IVec3)], alpha: &[u8; 256], shine: &[u8; 256]) -> Result<VoxModel> {
    for (k, (name, ..)) in objects.iter().enumerate() {
        if objects[..k].iter().any(|(other, ..)| other == name) {
            return Err(anyhow!("{}: two parts called '{name}' (the objects' names must differ)", path.display()));
        }
    }
    // Where every voxel stands in the scene (MagicaVoxel: a model's middle
    // at its translation), then from the low corner of them all.
    let mut placed = Vec::new();
    for (part, (_, model, at)) in objects.iter().enumerate() {
        let m = data.models.get(*model).with_context(|| format!("{}: an object without its model", path.display()))?;
        let half = IVec3::new(m.size.x as i32 / 2, m.size.y as i32 / 2, m.size.z as i32 / 2);
        for v in &m.voxels {
            let c = data.palette[v.i as usize];
            placed.push((*at + IVec3::new(v.x as i32, v.y as i32, v.z as i32) - half, [c.r, c.g, c.b, alpha[v.i as usize + 1]], shine[v.i as usize + 1], part));
        }
    }
    let low = placed.iter().fold(IVec3::MAX, |l, p| l.min(p.0));
    let high = placed.iter().fold(IVec3::MIN, |h, p| h.max(p.0));
    let frame: Vec<(IVec3, [u8; 4])> = placed.iter().map(|&(v, c, ..)| (v - low, c)).collect();
    let metal = placed.iter().map(|p| p.2).collect();
    let owner: Vec<usize> = placed.iter().map(|p| p.3).collect();
    let mut model = VoxModel::new(high - low + IVec3::ONE, vec![frame], vec![metal]);
    model.parts = parts_of(&model.frames[0], &owner, objects.iter().map(|o| o.0.clone()).collect(), model.size);
    Ok(model)
}

/// The parts of a figure (`Part`): `owner` says whose each voxel of `frame`
/// is, `names` the parts in order.
fn parts_of(frame: &[(IVec3, [u8; 4])], owner: &[usize], names: Vec<String>, size: IVec3) -> Vec<Part> {
    let count = names.len();
    let cells: crate::world::CellMap<usize> = frame.iter().zip(owner).map(|((v, _), &o)| (*v, o)).collect();
    let pivot = IVec3::new(size.x / 2, size.y / 2, 0).as_vec3();
    // Faces two parts share, and the middles of those faces added up.
    let mut touch = vec![vec![(0u32, Vec3::ZERO); count]; count];
    for ((v, _), &a) in frame.iter().zip(owner) {
        for d in crate::mesh::DIRS {
            if let Some(&b) = cells.get(&(*v + d)).filter(|&&b| b != a) {
                let (n, at) = &mut touch[a][b];
                *n += 1;
                *at += v.as_vec3() + 0.5 + d.as_vec3() * 0.5;
            }
        }
    }
    let members: Vec<Vec<u32>> = (0..count).map(|p| (0..owner.len() as u32).filter(|&k| owner[k as usize] == p).collect()).collect();
    let middle = |p: usize| members[p].iter().map(|&k| frame[k as usize].0.as_vec3() + 0.5).sum::<Vec3>() / members[p].len().max(1) as f32;
    let root = (0..count).max_by_key(|&p| (members[p].len(), std::cmp::Reverse(p))).unwrap_or(0);
    let mut parent: Vec<Option<usize>> = vec![None; count];
    let mut joint: Vec<Vec3> = vec![Vec3::ZERO; count];
    joint[root] = middle(root);
    let mut hung = vec![false; count];
    hung[root] = true;
    for _ in 1..count {
        // The part touching the hung ones most; touching none, the nearest.
        let touching = (0..count).filter(|&p| !hung[p]).flat_map(|p| (0..count).filter(|&q| hung[q]).map(move |q| (p, q))).max_by_key(|&(p, q)| (touch[p][q].0, std::cmp::Reverse((p, q))));
        let Some((p, q)) = touching else { break };
        let (p, q, at) = if touch[p][q].0 > 0 {
            (p, q, touch[p][q].1 / touch[p][q].0 as f32)
        } else {
            let mut best = (f32::MAX, p, q, Vec3::ZERO);
            for p in (0..count).filter(|&p| !hung[p]) {
                for q in (0..count).filter(|&q| hung[q]) {
                    for &a in &members[p] {
                        for &b in &members[q] {
                            let (va, vb) = (frame[a as usize].0.as_vec3(), frame[b as usize].0.as_vec3());
                            let d = va.distance_squared(vb);
                            if d < best.0 {
                                best = (d, p, q, (va + vb) * 0.5 + 0.5);
                            }
                        }
                    }
                }
            }
            (best.1, best.2, best.3)
        };
        (parent[p], joint[p], hung[p]) = (Some(q), at, true);
    }
    (0..count)
        .map(|p| {
            let own: Vec<(IVec3, [u8; 4])> = members[p].iter().map(|&k| frame[k as usize]).collect();
            let visible: Vec<u32> = visible_voxels(&own).into_iter().map(|i| members[p][i as usize]).collect();
            let glass = visible.iter().copied().filter(|&k| frame[k as usize].1[3] < 255).collect();
            Part { name: names[p].clone(), voxels: members[p].clone(), parent: parent[p], joint: joint[p] - pivot, visible, glass, bank: 0..0 }
        })
        .collect()
}

/// For each voxel of smooth glass on the surface (open space beside it),
/// the way the surface faces there: away from the voxels about it, within
/// `SHADE_REACH`. A stone cut in steps of voxels gets its facets' slant.
fn facets(frame: &[(IVec3, [u8; 4])], metal: &[u8]) -> Vec<[i8; 3]> {
    let filled: crate::world::CellSet = frame.iter().map(|(v, _)| *v).collect();
    frame
        .iter()
        .zip(metal)
        .map(|((v, c), m)| {
            let smooth_glass = c[3] < 255 && *m != 0 && *m >> 4 == 0;
            if !smooth_glass || crate::mesh::DIRS.iter().all(|d| filled.contains(&(*v + *d))) {
                return [0; 3];
            }
            let r = SHADE_REACH;
            let mut away = Vec3::ZERO;
            for z in -r..=r {
                for y in -r..=r {
                    for x in -r..=r {
                        let d = IVec3::new(x, y, z);
                        if d.length_squared() <= r * r + 2 && !filled.contains(&(*v + d)) {
                            away += d.as_vec3();
                        }
                    }
                }
            }
            let n = away.normalize_or_zero() * 127.0;
            [n.x.round() as i8, n.y.round() as i8, n.z.round() as i8]
        })
        .collect()
}

/// How far `VoxModel::shade` looks around a voxel for the surface's
/// direction, and how far an edge voxel's faces look for smooth surface
/// to take theirs from.
const SHADE_REACH: i32 = 2;
const EDGE_REACH: i32 = 4;
/// All six faces (bits in `mesh::DIRS` order).
pub const ALL_FACES: i8 = 63;

impl VoxModel {
    pub fn new(size: IVec3, frames: Vec<Vec<(IVec3, [u8; 4])>>, metal: Vec<Vec<u8>>) -> Self {
        let visible: Vec<Vec<u32>> = frames.iter().map(|frame| visible_voxels(frame)).collect();
        let glass = visible.iter().zip(&frames).map(|(seen, frame)| seen.iter().copied().filter(|&k| frame[k as usize].1[3] < 255).collect()).collect();
        let smooth_glass = |m: &u8| *m != 0 && *m >> 4 == 0;
        let facets = if metal.iter().flatten().any(smooth_glass) { frames.iter().zip(&metal).map(|(frame, m)| facets(frame, m)).collect() } else { Vec::new() };
        Self { size, frames, metal, shade: Vec::new(), facets, visible, glass, bank: Vec::new(), parts: Vec::new() }
    }

    /// Works out how each voxel is lit (`shade`). With `Smooth` a whole
    /// voxel by the direction of the surface around it, the way round shapes
    /// shade softly in pixel art. With `Edges` the same where its
    /// surroundings are one smooth surface; where two surfaces meet (they
    /// stray more than `tolerance` voxels from one) each face takes the
    /// direction of the smooth surface nearby that it belongs to, or keeps
    /// its own: hard edges stay hard, and the faces beside them do not
    /// turn to stripes. Thin parts with open space on both sides, and
    /// glass, keep their faces.
    pub fn shade(&mut self, mode: crate::config::ModelShading, tolerance: f32) {
        use crate::config::ModelShading;
        self.shade = match mode {
            ModelShading::Faces => Vec::new(),
            _ => self.frames.iter().map(|frame| shade_frame(frame, mode == ModelShading::Edges, tolerance)).collect(),
        };
    }
}

/// The voxels of a frame that can be seen: glass, and those with open
/// space or glass on a side.
fn visible_voxels(frame: &[(IVec3, [u8; 4])]) -> Vec<u32> {
    let opaque: crate::world::CellSet = frame.iter().filter(|(_, c)| c[3] == 255).map(|(v, _)| *v).collect();
    frame
        .iter()
        .enumerate()
        .filter(|(_, (v, c))| c[3] < 255 || crate::mesh::DIRS.iter().any(|d| !opaque.contains(&(*v + *d))))
        .map(|(k, _)| k as u32)
        .collect()
}

/// Per voxel: the direction it is lit by (snorm xyz) and in w the faces
/// that use it (bits in `mesh::DIRS` order; the others keep their own).
fn shade_frame(frame: &[(IVec3, [u8; 4])], edges: bool, tolerance: f32) -> Vec<[i8; 4]> {
    if frame.is_empty() {
        return Vec::new();
    }
    let pad = EDGE_REACH + 1;
    let lo = frame.iter().fold(IVec3::MAX, |a, (v, _)| a.min(*v)) - pad;
    let hi = frame.iter().fold(IVec3::MIN, |a, (v, _)| a.max(*v)) + pad;
    let size = hi - lo + 1;
    let inside = |v: IVec3| v.cmpge(lo).all() && v.cmple(hi).all();
    let index = |v: IVec3| {
        let d = v - lo;
        ((d.z * size.y + d.y) * size.x + d.x) as usize
    };
    let mut filled = vec![false; (size.x * size.y * size.z) as usize];
    for (v, _) in frame {
        filled[index(*v)] = true;
    }
    let at = |v: IVec3| inside(v) && filled[index(v)];
    let surface = |v: IVec3| at(v) && crate::mesh::DIRS.iter().any(|d| !at(v + *d));
    let weight = |d: IVec3, sigma: f32| (-(d.length_squared() as f32) / (2.0 * sigma * sigma)).exp();
    let cube = |r: i32| (-r..=r).flat_map(move |z| (-r..=r).flat_map(move |y| (-r..=r).map(move |x| IVec3::new(x, y, z))));

    // The direction of the surface at every surface voxel (towards the open
    // space around it), and whether it lies on one smooth surface there.
    let mut normals = vec![None::<(glam::Vec3, bool)>; filled.len()];
    for (v, color) in frame {
        let v = *v;
        if color[3] < 255 || !surface(v) {
            continue;
        }
        let mut normal = glam::Vec3::ZERO;
        for d in cube(SHADE_REACH).filter(|d| !at(v + *d)) {
            normal += d.as_vec3() * weight(d, 1.5);
        }
        if normal.length() < 0.5 {
            continue;
        }
        let n = normal.normalize();
        let smooth = !edges || {
            // How far the surface voxels around stray from one surface
            // through their middle facing `n`.
            let points: Vec<_> = cube(SHADE_REACH).filter(|d| surface(v + *d)).map(|d| (d.as_vec3(), weight(d, 1.5))).collect();
            let sum: f32 = points.iter().map(|p| p.1).sum();
            let mid = points.iter().map(|(p, w)| *p * *w).sum::<glam::Vec3>() / sum;
            let spread = (points.iter().map(|(p, w)| w * (*p - mid).dot(n).powi(2)).sum::<f32>() / sum).sqrt();
            spread <= tolerance
        };
        normals[index(v)] = Some((n, smooth));
    }

    let s = |c: f32| (c * 127.0).round() as i8;
    frame
        .iter()
        .map(|&(v, _)| {
            let Some((n, smooth)) = normals[index(v)] else { return [0; 4] };
            if smooth {
                return [s(n.x), s(n.y), s(n.z), ALL_FACES];
            }
            // An edge: each open face takes the direction of the smooth
            // surface nearby that faces its way (within about 75 degrees: a
            // gentle slope's risers still belong to it, the other side of an
            // edge, at 90, does not).
            let mut faces = [None::<glam::Vec3>; 6];
            for (f, dir) in crate::mesh::DIRS.iter().enumerate() {
                if at(v + *dir) {
                    continue;
                }
                let mut sum = glam::Vec3::ZERO;
                for d in cube(EDGE_REACH) {
                    if let Some((m, true)) = inside(v + d).then(|| normals[index(v + d)]).flatten()
                        && m.dot(dir.as_vec3()) > 0.25
                    {
                        sum += m * weight(d, 2.0);
                    }
                }
                faces[f] = (sum.length() > 1e-3).then(|| sum.normalize());
            }
            // One direction a voxel carries: the face's most turned away
            // from its own; the faces close to it use it, the rest their own.
            let turned = |f: usize| faces[f].map_or(0.0, |m| 1.0 - m.dot(crate::mesh::DIRS[f].as_vec3()));
            let Some(main) = (0..6).filter(|&f| faces[f].is_some()).max_by(|&a, &b| turned(a).total_cmp(&turned(b))) else {
                return [0; 4];
            };
            let m = faces[main].unwrap();
            let mask = (0..6).filter(|&f| faces[f].is_some_and(|o| o.dot(m) > 0.9)).fold(0, |bits, f| bits | 1 << f);
            [s(m.x), s(m.y), s(m.z), mask]
        })
        .collect()
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

/// Loads a level made as one `.vox` scene. Voxels whose palette index is in
/// `markers` are not placed; they come back as markers for the simulation
/// to place things.
pub fn load_world(files: &Files, path: &Path, markers: &HashMap<u8, usize>) -> Result<(World, Vec<Marker>)> {
    let scene = load_scene_in(files, path)?;
    let mut world = World::new(scene.size, scene.palette);
    let mut found = Vec::new();
    for (p, index) in scene.voxels {
        match markers.get(&index) {
            Some(&kind) => found.push(Marker { kind, cell: p }),
            None => {
                world.set_raw(p, index);
            }
        }
    }
    world.mark_all_dirty();
    Ok((world, found))
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
/// colour (name, size, where its low corner stands), a named object.
#[cfg(test)]
pub(crate) fn write_figure(path: &Path, parts: &[(&str, IVec3, IVec3)]) {
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
        scenes.push(SceneNode::Transform { attributes: dict(&[("_name", name.to_string())]), frames: vec![Frame::new(dict(&[("_t", format!("{} {} {}", at.x, at.y, at.z))]))], child: ids[k] + 1, layer_id: 0 });
        scenes.push(SceneNode::Shape { attributes: dict(&[]), models: vec![ShapeModel { model_id: k as u32, attributes: dict(&[]) }] });
    }
    let data = DotVoxData { version: 150, index_map: Vec::new(), models, palette, materials: Vec::new(), scenes, layers: vec![Layer { attributes: dict(&[]) }] };
    let mut file = std::fs::File::create(path).unwrap();
    data.write_vox(&mut file).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file whose objects have names is a figure of parts: one frame of
    /// them all as they stand; the biggest part the root; each other hung
    /// on the part it touches most, about the middle of where they meet;
    /// one touching none on the nearest. Without names, models are frames.
    #[test]
    fn a_figure_hangs_each_part_on_what_it_touches() {
        let dir = std::env::temp_dir().join(format!("voxcade-figure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        write_figure(&file, &[("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("arm", IVec3::new(1, 2, 6), IVec3::new(4, 1, 2)), ("hat", IVec3::new(2, 2, 1), IVec3::new(1, 1, 14))]);
        let m = load_model(&file).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!((m.frames.len(), m.frames[0].len(), m.size), (1, 64 + 128 + 12 + 4, IVec3::new(5, 4, 15)));
        let by = |name: &str| m.parts.iter().position(|p| p.name == name).unwrap();
        let (head, body, arm, hat) = (by("head"), by("body"), by("arm"), by("hat"));
        assert_eq!(m.parts[body].parent, None, "the biggest is the root");
        assert_eq!((m.parts[head].parent, m.parts[arm].parent, m.parts[hat].parent), (Some(body), Some(body), Some(head)));
        // Joints from the pivot cell's low corner (2, 2, 0).
        assert_eq!(m.parts[head].joint, Vec3::new(0.0, 0.0, 8.0), "the neck: the middle of the head's underside");
        assert_eq!(m.parts[arm].joint, Vec3::new(2.0, 0.0, 5.0), "the shoulder: the middle of where the arm meets the body's side");
        assert_eq!(m.parts[hat].joint.z, 13.0, "between the hat and the head's top");
        assert!(m.parts.iter().all(|p| !p.visible.is_empty()));
    }

    /// A colour with MagicaVoxel's glass material is see-through, by its
    /// transparency; the rest of the model stays opaque.
    /// A model draws only the voxels that can be seen: a solid cube its
    /// shell, and what lies behind glass.
    #[test]
    fn a_model_draws_only_what_can_be_seen() {
        let cube = |glass: bool| {
            let mut voxels = Vec::new();
            for z in 0..4 {
                for y in 0..4 {
                    for x in 0..4 {
                        let alpha = if glass && y == 0 { 120 } else { 255 };
                        voxels.push((IVec3::new(x, y, z), [200, 200, 200, alpha]));
                    }
                }
            }
            VoxModel::new(IVec3::splat(4), vec![voxels], vec![Vec::new()])
        };
        assert_eq!(cube(false).visible[0].len(), 4 * 4 * 4 - 2 * 2 * 2, "only the shell");
        // A glass front shows the four inner voxels behind it.
        assert_eq!(cube(true).visible[0].len(), 4 * 4 * 4 - 2 * 2 * 2 + 2 * 2);
    }

    /// Model shading: a sphere is lit voxel by voxel by its surface's
    /// direction; with edges a crate's corner keeps each face's own light,
    /// and a 45 degree ramp is one smooth slope, its sides included.
    #[test]
    fn smooth_shading_keeps_hard_edges() {
        use crate::config::ModelShading;
        let model = |filled: &dyn Fn(i32, i32, i32) -> bool| {
            let mut voxels = Vec::new();
            for z in 0..20 {
                for y in 0..20 {
                    for x in 0..20 {
                        if filled(x, y, z) {
                            voxels.push((IVec3::new(x, y, z), [200, 200, 200, 255]));
                        }
                    }
                }
            }
            VoxModel::new(IVec3::splat(20), vec![voxels], vec![Vec::new()])
        };
        let shade_of = |m: &VoxModel, v: IVec3| {
            let k = m.frames[0].iter().position(|(p, _)| *p == v).unwrap();
            let [x, y, z, faces] = m.shade[0][k];
            (glam::Vec3::new(x as f32, y as f32, z as f32) / 127.0, faces)
        };

        let mut sphere = model(&|x, y, z| IVec3::new(x, y, z).as_vec3().distance(glam::Vec3::splat(9.5)) < 9.0);
        sphere.shade(ModelShading::Edges, 0.5);
        let (n, faces) = shade_of(&sphere, IVec3::new(16, 9, 15));
        assert_eq!(faces, ALL_FACES, "a voxel on the sphere is not lit as a whole");
        assert!(n.x > 0.4 && n.z > 0.4, "the sphere's upper right faces {n}");

        let mut crate_ = model(&|x, y, z| (2..18).contains(&x) && (2..18).contains(&y) && (2..18).contains(&z));
        crate_.shade(ModelShading::Edges, 0.5);
        let (n, faces) = shade_of(&crate_, IVec3::new(10, 2, 17));
        assert!(faces == 0 || (n.z.abs() > 0.99 || n.y.abs() > 0.99) && faces != ALL_FACES, "the crate's top front edge went soft: {n} for faces {faces:06b}");
        crate_.shade(ModelShading::Smooth, 0.5);
        assert_eq!(shade_of(&crate_, IVec3::new(10, 2, 17)).1, ALL_FACES);

        let mut ramp = model(&|x, y, z| (2..14).contains(&x) && z <= y);
        ramp.shade(ModelShading::Edges, 0.5);
        for x in [2, 8] {
            let (n, faces) = shade_of(&ramp, IVec3::new(x, 10, 10));
            let slope = glam::Vec3::new(0.0, -1.0, 1.0).normalize();
            assert!(n.dot(slope) > 0.97, "the ramp at x {x} faces {n}");
            // Its top (+z) and front (-y) use the slope; at the side the side
            // face (-x) keeps its own.
            assert_eq!(faces & 0b01_1000, 0b01_1000, "x {x}: faces {faces:06b}");
            if x == 2 {
                assert_eq!(faces & 0b10, 0, "the side face went soft");
            }
        }
    }

    /// Glass is see-through by its transparency; written by vengi, which
    /// keeps only an alpha, by that; blend by its alpha; metal not at all.
    #[test]
    fn glass_from_magicavoxel_and_vengi_is_see_through() {
        let material = |props: &[(&str, &str)]| dot_vox::Material { id: 1, properties: props.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        assert!(opacity(&material(&[("_type", "_glass"), ("_trans", "0.6")])).is_some_and(|o| (o - 0.4).abs() < 1e-6));
        assert_eq!(opacity(&material(&[("_type", "_glass"), ("_alpha", "0.3")])), Some(0.3), "vengi's glass");
        assert_eq!(opacity(&material(&[("_type", "_glass")])), Some(0.5));
        assert_eq!(opacity(&material(&[("_type", "_blend"), ("_alpha", "0.25")])), Some(0.25));
        assert_eq!(opacity(&material(&[("_type", "_metal"), ("_alpha", "0.25")])), None);
    }

    #[test]
    fn glass_in_a_model_is_see_through() {
        let model = load_model(Path::new("games/moreblood/huey.vox")).unwrap();
        let alphas: Vec<u8> = model.frames[0].iter().map(|(_, c)| c[3]).collect();
        let glass = alphas.iter().filter(|&&a| a < 255).count();
        assert!(glass > 20, "only {glass} glass voxels");
        assert!(alphas.iter().filter(|&&a| a < 255).all(|&a| (90..=115).contains(&a)), "the glass is not 60% see-through");
        assert!(glass < alphas.len() / 4, "too much is glass");
    }

    /// Glass mirrors by its roughness, not as metal; glass without one
    /// (More Blood's cockpit) mirrors nothing. Voxkeep's cut gem takes its
    /// facets' slant: its table faces up, its girdle out, its point down.
    #[test]
    fn smooth_glass_mirrors_by_its_facets() {
        let huey = load_model(Path::new("games/moreblood/huey.vox")).unwrap();
        assert!(huey.frames.iter().zip(&huey.metal).all(|(f, m)| f.iter().zip(m).all(|((_, c), m)| c[3] == 255 || *m == 0)), "the cockpit mirrors");
        assert!(huey.facets.is_empty());
        let gem = load_model(Path::new("games/voxkeep/gem.vox")).unwrap();
        assert!(gem.metal[0].iter().all(|&m| m >> 4 == 0 && m & 15 > 6), "the gem is not smooth glass");
        let facet = |v: IVec3| {
            let k = gem.frames[0].iter().position(|(p, _)| *p == v).unwrap();
            let [x, y, z] = gem.facets[0][k];
            glam::Vec3::new(x as f32, y as f32, z as f32) / 127.0
        };
        assert!(facet(IVec3::new(3, 3, 5)).z > 0.9, "the table faces {}", facet(IVec3::new(3, 3, 5)));
        assert!(facet(IVec3::new(0, 3, 3)).x < -0.7, "the girdle faces {}", facet(IVec3::new(0, 3, 3)));
        assert!(facet(IVec3::new(3, 3, 0)).z < -0.7, "the point faces {}", facet(IVec3::new(3, 3, 0)));
        let inside = gem.frames[0].iter().position(|(p, _)| *p == IVec3::new(3, 3, 3)).unwrap();
        assert_eq!(gem.facets[0][inside], [0; 3], "inside, no facet");
    }

    /// Colours with MagicaVoxel's metal material come out metal, by their
    /// metalness and roughness; the blood on More Blood's dog tags does not.
    #[test]
    fn metal_in_a_model_shines() {
        let model = load_model(Path::new("games/moreblood/dogtags.vox")).unwrap();
        let steel = model.frames[0].iter().zip(&model.metal[0]).filter(|((_, c), _)| c[..3] == [168, 172, 176]);
        assert!(steel.clone().count() > 100);
        assert!(steel.clone().all(|(_, &m)| m >> 4 == 15 && m & 15 > 6), "steel is not shiny metal");
        let blood = model.frames[0].iter().zip(&model.metal[0]).filter(|((_, c), _)| c[..3] == [150, 20, 18]);
        assert!(blood.clone().count() > 0 && blood.clone().all(|(_, &m)| m == 0), "the blood shines like metal");
    }
}

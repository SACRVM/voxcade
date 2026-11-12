//! Models as the engine holds them (`VoxModel`: frames, or a figure's
//! parts with their joints), read from Voxcade's own text format (`.vvx`,
//! `crate::vvx`); and MagicaVoxel's `.vox`, which a game only brings in
//! (`import_model`, `voxcade import`, `voxcade upgrade`; a world or a
//! map's prefab of an older game as a scene, `load_scene`: every model of
//! the scene, placed by its transform, its colours by their palette index).

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use dot_vox::{DotVoxData, SceneNode};
use glam::{IVec3, Vec3};

use crate::files::Files;

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
    /// Per frame and voxel (as in `frames`): how much it glows, 0 to 255
    /// (MagicaVoxel's emission material, `glows`); empty for a model
    /// without a glowing colour, which is nearly all of them.
    pub glow: Vec<Vec<u8>>,
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
    /// Per frame and visible voxel (as in `visible`): the faces another
    /// opaque voxel of the frame lies against (`covered`), never drawn.
    pub covered: Vec<Vec<u8>>,
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
///
/// The file may say otherwise, part by part (the editor's FIGURE writes
/// it): a part's `parent` (a part's name) and `joint` (from the pivot,
/// `crate::vvx`). A parent so named wins, and the joint is then worked out
/// against it (the middle of where they meet, else between their nearest
/// voxels); a joint so placed wins over the one worked out. The root is
/// the biggest part that names no parent. Halves are a joint's steps (a
/// corner, an edge's or a face's middle, a voxel's). A MagicaVoxel figure
/// brought in may say the same as `_parent` and `_joint` on the transform
/// node that carries the object's `_name` (a joint in the space of the
/// part's own translation `_t`), which the editor wrote for a few days
/// before models were text; they are read, never written.
#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub voxels: Vec<u32>,
    pub parent: Option<usize>,
    pub joint: Vec3,
    /// The joint is the file's (`joint`), not worked out.
    pub joint_by_hand: bool,
    /// Its voxels that can be seen once it moves apart from the rest (its
    /// own surface, the joint's faces too), and of those the see-through.
    pub visible: Vec<u32>,
    pub glass: Vec<u32>,
    /// Per visible voxel: the faces another opaque voxel of the part lies
    /// against (`covered`).
    pub covered: Vec<u8>,
    /// Its opaque visible voxels in the model bank.
    pub bank: std::ops::Range<u32>,
    /// How it came to hang on its parent (what the editor's FIGURE tells),
    /// and the other parts it lies in the same places as (their place in
    /// the list, how many voxels): the game reads none of it.
    pub hung: Hung,
    pub overlaps: Vec<(usize, u32)>,
}

/// How a part came to hang where it hangs (`parts_of`), so that the editor
/// can say so: a rig built by rules nobody sees goes wrong unseen (an arm
/// hung at its hand).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hung {
    /// The biggest part naming no parent, turning about its own middle.
    Root,
    /// On the part the file names (`parent`), set by hand.
    ByHand,
    /// On the part it shares the most faces with among those hung before
    /// it (`faces`), and the next most among them, if another touches it
    /// (its place in the list and its faces).
    Touching { faces: u32, next: Option<(usize, u32)> },
    /// It shared no face with any part hung before it, so it was joined
    /// where it comes nearest: the parts it does touch (none: it touches
    /// nothing), and another hung part that comes as near.
    Nearest { touches: Vec<usize>, as_near: Option<usize> },
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

/// The parts hanging on each other in a ring by the parents the file names
/// (`parents`, by place), the first found, from part `p` round to it; none
/// if every chain ends at a part naming no parent.
pub fn ring(parents: &[Option<usize>], p: usize) -> Option<Vec<usize>> {
    let mut chain = vec![p];
    let mut at = p;
    while let Some(q) = parents[at] {
        if q == p {
            chain.push(p);
            return Some(chain);
        }
        if chain.contains(&q) || chain.len() > parents.len() {
            return None;
        }
        chain.push(q);
        at = q;
    }
    None
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
    if let Some(chain) = (0..objects.len()).find_map(|p| ring(&parents, p)) {
        let names: Vec<&str> = chain.iter().map(|&p| objects[p].name.as_str()).collect();
        if chain.len() == 2 {
            return Err(anyhow!("{}: part '{}' hangs on itself (_parent): it must hang on another part, or on none", path.display(), names[0]));
        }
        return Err(anyhow!("{}: part '{}' hangs on itself by way of the others ({}): one of them must hang on another part, or on none", path.display(), names[0], names.join(" on ")));
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

/// The parts of a figure (`Part`): `owner` says whose each voxel of `frame`
/// is, `names` the parts in order, `stated` what the file says of each
/// (its parent, its joint from the frame's low corner; no ring among the
/// parents, `ring`).
pub(crate) fn parts_of(frame: &[(IVec3, [u8; 4])], owner: &[usize], names: Vec<String>, size: IVec3, stated: &[(Option<usize>, Option<Vec3>)]) -> Vec<Part> {
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
    // The root: the biggest part naming no parent.
    let root = (0..count).filter(|&p| stated[p].0.is_none()).max_by_key(|&p| (members[p].len(), std::cmp::Reverse(p))).unwrap_or(0);
    let mut parent: Vec<Option<usize>> = vec![None; count];
    let mut joint: Vec<Vec3> = vec![Vec3::ZERO; count];
    joint[root] = stated[root].1.unwrap_or_else(|| middle(root));
    let mut hung = vec![false; count];
    hung[root] = true;
    // How each came to hang, for the editor to tell (read after the
    // choice, never part of it).
    let mut how = vec![Hung::Root; count];
    // Each part's box, and how near squared a point or another part's box
    // comes to it at the most: the searches below skip what cannot come
    // nearer than the nearest found (a figure of letters apart, a logo,
    // took most of a second to hang without).
    let boxes: Vec<(Vec3, Vec3)> = members.iter().map(|m| m.iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), &k| (l.min(frame[k as usize].0.as_vec3()), h.max(frame[k as usize].0.as_vec3())))).collect();
    let point_gap = |v: Vec3, q: usize| (boxes[q].0 - v).max(v - boxes[q].1).max(Vec3::ZERO).length_squared();
    let box_gap = |p: usize, q: usize| (boxes[q].0 - boxes[p].1).max(boxes[p].0 - boxes[q].1).max(Vec3::ZERO).length_squared();
    // Between the nearest voxels of `p` and `q` (the first pair found of
    // the nearest), and how near squared.
    let nearest_pair = |p: usize, q: usize| {
        let mut best = (f32::MAX, Vec3::ZERO);
        for &a in &members[p] {
            if point_gap(frame[a as usize].0.as_vec3(), q) >= best.0 {
                continue;
            }
            for &b in &members[q] {
                let (va, vb) = (frame[a as usize].0.as_vec3(), frame[b as usize].0.as_vec3());
                let d = va.distance_squared(vb);
                if d < best.0 {
                    best = (d, (va + vb) * 0.5 + 0.5);
                }
            }
        }
        best
    };
    for _ in 1..count {
        // A part whose parent the file names, once that one hangs: on it,
        // about the middle of where they meet, else between their nearest
        // voxels.
        if let Some(p) = (0..count).find(|&p| !hung[p] && stated[p].0.is_some_and(|q| hung[q])) {
            let q = stated[p].0.unwrap_or(root);
            let at = if touch[p][q].0 > 0 { touch[p][q].1 / touch[p][q].0 as f32 } else { nearest_pair(p, q).1 };
            how[p] = Hung::ByHand;
            (parent[p], joint[p], hung[p]) = (Some(q), stated[p].1.unwrap_or(at), true);
            continue;
        }
        // Of the rest, the part touching the hung ones most; touching none,
        // the nearest.
        let free = |p: usize| !hung[p] && stated[p].0.is_none();
        let touching = (0..count).filter(|&p| free(p)).flat_map(|p| (0..count).filter(|&q| hung[q]).map(move |q| (p, q))).max_by_key(|&(p, q)| (touch[p][q].0, std::cmp::Reverse((p, q))));
        let Some((p, q)) = touching else { break };
        let (p, q, at) = if touch[p][q].0 > 0 {
            let next = (0..count).filter(|&r| hung[r] && r != q && touch[p][r].0 > 0).max_by_key(|&r| (touch[p][r].0, std::cmp::Reverse(r))).map(|r| (r, touch[p][r].0));
            how[p] = Hung::Touching { faces: touch[p][q].0, next };
            (p, q, touch[p][q].1 / touch[p][q].0 as f32)
        } else {
            let mut best = (f32::MAX, p, q, Vec3::ZERO);
            for p in (0..count).filter(|&p| free(p)) {
                for q in (0..count).filter(|&q| hung[q]) {
                    if box_gap(p, q) >= best.0 {
                        continue;
                    }
                    for &a in &members[p] {
                        if point_gap(frame[a as usize].0.as_vec3(), q) >= best.0 {
                            continue;
                        }
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
            let (p, q) = (best.1, best.2);
            let as_near = (0..count).filter(|&r| hung[r] && r != q).find(|&r| best.0 < f32::MAX && box_gap(p, r) <= best.0 && nearest_pair(p, r).0 <= best.0);
            how[p] = Hung::Nearest { touches: (0..count).filter(|&r| touch[p][r].0 > 0).collect(), as_near };
            (p, q, best.3)
        };
        (parent[p], joint[p], hung[p]) = (Some(q), stated[p].1.unwrap_or(at), true);
    }
    // Voxels of two parts in one place (`cells` holds the later part's).
    let mut overlap = vec![vec![0u32; count]; count];
    for ((v, _), &a) in frame.iter().zip(owner) {
        if let Some(&b) = cells.get(v)
            && b != a
        {
            overlap[a][b] += 1;
            overlap[b][a] += 1;
        }
    }
    (0..count)
        .map(|p| {
            let overlaps = (0..count).filter(|&q| overlap[p][q] > 0).map(|q| (q, overlap[p][q])).collect();
            let own: Vec<(IVec3, [u8; 4])> = members[p].iter().map(|&k| frame[k as usize]).collect();
            let (seen, covered) = visible_voxels(&own);
            let visible: Vec<u32> = seen.into_iter().map(|i| members[p][i as usize]).collect();
            let glass = visible.iter().copied().filter(|&k| frame[k as usize].1[3] < 255).collect();
            Part { name: names[p].clone(), voxels: members[p].clone(), parent: parent[p], joint: joint[p] - pivot, joint_by_hand: stated[p].1.is_some(), visible, glass, covered, bank: 0..0, hung: how[p].clone(), overlaps }
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
        let (visible, covered): (Vec<Vec<u32>>, Vec<Vec<u8>>) = frames.iter().map(|frame| visible_voxels(frame)).unzip();
        let glass = visible.iter().zip(&frames).map(|(seen, frame)| seen.iter().copied().filter(|&k| frame[k as usize].1[3] < 255).collect()).collect();
        let smooth_glass = |m: &u8| *m != 0 && *m >> 4 == 0;
        let facets = if metal.iter().flatten().any(smooth_glass) { frames.iter().zip(&metal).map(|(frame, m)| facets(frame, m)).collect() } else { Vec::new() };
        Self { size, frames, metal, glow: Vec::new(), shade: Vec::new(), facets, visible, glass, covered, bank: Vec::new(), parts: Vec::new() }
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
fn visible_voxels(frame: &[(IVec3, [u8; 4])]) -> (Vec<u32>, Vec<u8>) {
    let opaque: crate::world::CellSet = frame.iter().filter(|(_, c)| c[3] == 255).map(|(v, _)| *v).collect();
    frame
        .iter()
        .enumerate()
        .filter(|(_, (v, c))| c[3] < 255 || crate::mesh::DIRS.iter().any(|d| !opaque.contains(&(*v + *d))))
        .map(|(k, (v, c))| (k as u32, if c[3] == 255 { covered(&opaque, *v) } else { 0 }))
        .unzip()
}

/// The faces of the opaque voxel at `v` that another opaque voxel (of
/// `opaque`, its model's frame or its part) lies against, as bits in
/// `mesh::DIRS` order, in the model's own directions. They are never drawn,
/// from the bank or as cubes: seen nearly edge on, such a face between two
/// voxels of a surface lies at the surface's own depth along the edge and
/// won single pixels there (dots in the neighbouring face's shade, all over
/// a turned figure, and over the level editor's pieces in shade).
pub fn covered(opaque: &crate::world::CellSet, v: IVec3) -> u8 {
    crate::mesh::DIRS.iter().enumerate().filter(|(_, d)| opaque.contains(&(v + **d))).fold(0, |bits, (bit, _)| bits | 1 << bit)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A file whose objects have names is a figure of parts: one frame of
    /// them all as they stand; the biggest part the root; each other hung
    /// on the part it touches most, about the middle of where they meet;
    /// one touching none on the nearest. Without names, models are frames.
    #[test]
    fn a_figure_hangs_each_part_on_what_it_touches() {
        let dir = crate::scratch::Scratch::new("figure");
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

    /// How each part came to hang is told without changing it: a part
    /// apart from the rest is joined where it comes nearest and says it
    /// touches nothing; one touching two hung parts alike says which came
    /// second; voxels of two parts in one place are counted.
    #[test]
    fn a_figure_tells_how_its_parts_hang() {
        let dir = crate::scratch::Scratch::new("hung");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        // A scarf as much on the body as on the head, a hand off by two,
        // a badge sunk one voxel into the body.
        write_figure(&file, &[("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("scarf", IVec3::new(1, 4, 2), IVec3::new(4, 0, 7)), ("hand", IVec3::new(1, 2, 2), IVec3::new(7, 1, 2)), ("badge", IVec3::new(2, 1, 1), IVec3::new(1, 3, 4))]);
        let m = load_model(&file).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let by = |name: &str| m.parts.iter().position(|p| p.name == name).unwrap();
        let (body, head, scarf, hand, badge) = (by("body"), by("head"), by("scarf"), by("hand"), by("badge"));
        assert_eq!(m.parts[body].hung, Hung::Root);
        assert_eq!(m.parts[head].hung, Hung::Touching { faces: 16, next: None });
        assert_eq!(m.parts[scarf].parent, Some(body), "a tie goes to the part first in the file");
        assert_eq!(m.parts[scarf].hung, Hung::Touching { faces: 4, next: Some((head, 4)) });
        assert_eq!(m.parts[hand].parent, Some(body));
        assert_eq!(m.parts[hand].hung, Hung::Nearest { touches: Vec::new(), as_near: None });
        assert_eq!(m.parts[badge].overlaps, vec![(body, 2)]);
        assert_eq!(m.parts[body].overlaps, vec![(badge, 2)]);
    }

    /// The parents and joints as they were worked out before a rig could
    /// be set by hand (kept here as it was, to hold today's against it).
    fn rig_before(frame: &[(IVec3, [u8; 4])], owner: &[usize], count: usize, size: IVec3) -> Vec<(Option<usize>, Vec3)> {
        let cells: crate::world::CellMap<usize> = frame.iter().zip(owner).map(|((v, _), &o)| (*v, o)).collect();
        let pivot = IVec3::new(size.x / 2, size.y / 2, 0).as_vec3();
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
        let (mut parent, mut joint, mut hung) = (vec![None; count], vec![Vec3::ZERO; count], vec![false; count]);
        joint[root] = middle(root);
        hung[root] = true;
        for _ in 1..count {
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
        (0..count).map(|p| (parent[p], joint[p] - pivot)).collect()
    }

    /// A figure whose file sets no rig hangs exactly as before rigs could
    /// be set: many figures of boxes, touching, apart, overlapping.
    #[test]
    fn a_figure_without_a_rig_hangs_as_before() {
        let mut seed = 0x2545_f491_u64;
        let mut next = |n: i32| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((seed >> 33) % n as u64) as i32
        };
        for _ in 0..300 {
            let count = 2 + next(6) as usize;
            let mut frame = Vec::new();
            let mut owner = Vec::new();
            for p in 0..count {
                let (size, low) = (IVec3::new(1 + next(4), 1 + next(4), 1 + next(5)), IVec3::new(next(10), next(6), next(12)));
                for z in 0..size.z {
                    for y in 0..size.y {
                        for x in 0..size.x {
                            frame.push((low + IVec3::new(x, y, z), [p as u8, 0, 0, 255]));
                            owner.push(p);
                        }
                    }
                }
            }
            let size = frame.iter().fold(IVec3::ZERO, |s, (v, _)| s.max(*v + 1));
            let names = (0..count).map(|p| format!("p{p}")).collect();
            let now: Vec<(Option<usize>, Vec3)> = parts_of(&frame, &owner, names, size, &vec![(None, None); count]).iter().map(|p| (p.parent, p.joint)).collect();
            assert_eq!(now, rig_before(&frame, &owner, count, size));
        }
    }

    /// Parents and joints set in the file win: a parent named hangs the
    /// part on it, its joint worked out against it (where they meet, else
    /// between their nearest voxels); a joint placed (in the scene, as the
    /// translations are) is where it turns. Parts the file says nothing of
    /// hang as the rules find; the root is the biggest naming no parent.
    #[test]
    fn a_rig_set_in_the_file_wins() {
        let dir = crate::scratch::Scratch::new("rig");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        // A body, a head on it, an arm held apart from the body and a hand
        // on the arm's end.
        let parts = [("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("arm", IVec3::new(1, 2, 5), IVec3::new(6, 1, 2)), ("hand", IVec3::new(1, 2, 1), IVec3::new(6, 1, 1))];
        write_figure_with(&file, &parts, &[("arm", "_parent", "head"), ("arm", "_joint", "6.5 2 7"), ("hand", "_joint", "6 1.5 1.5")]);
        let m = load_model(&file).unwrap();
        let by = |name: &str| m.parts.iter().position(|p| p.name == name).unwrap();
        let (body, head, arm, hand) = (by("body"), by("head"), by("arm"), by("hand"));
        assert_eq!((m.parts[body].parent, m.parts[head].parent), (None, Some(body)));
        assert_eq!((m.parts[arm].parent, m.parts[arm].hung.clone()), (Some(head), Hung::ByHand));
        // From the pivot cell's low corner (3, 2, 0); the scene's low
        // corner is the body's (0, 0, 0).
        assert_eq!((m.parts[arm].joint, m.parts[arm].joint_by_hand), (Vec3::new(3.5, 0.0, 7.0), true));
        assert_eq!(m.parts[hand].parent, Some(arm), "on the arm by the rules");
        assert_eq!(m.parts[hand].joint, Vec3::new(3.0, -0.5, 1.5));
        assert!(!m.parts[head].joint_by_hand && m.parts[head].hung != Hung::ByHand);
        // A parent named, no joint: where the two meet; touching nothing of
        // it, between their nearest voxels.
        write_figure_with(&file, &parts, &[("arm", "_parent", "head")]);
        let m = load_model(&file).unwrap();
        assert_eq!((m.parts[arm].parent, m.parts[arm].joint), (Some(head), Vec3::new(2.0, -0.5, 7.5)), "between (6, 1, 6) and (3, 1, 8), the first nearest pair");
        // The biggest part hung on another: the root is the biggest naming
        // none.
        write_figure_with(&file, &parts, &[("body", "_parent", "head")]);
        let m = load_model(&file).unwrap();
        assert_eq!((m.parts[head].parent, m.parts[body].parent, m.parts[arm].parent), (None, Some(head), Some(body)));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// What a file says of its rig that cannot be is said plainly, with
    /// the file: a parent no part is called, parts hanging on each other
    /// in a ring, a joint that is not three numbers.
    #[test]
    fn a_rig_that_cannot_be_is_said_plainly() {
        let dir = crate::scratch::Scratch::new("rig-wrong");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("figure.vox");
        let parts = [("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("arm", IVec3::new(1, 2, 5), IVec3::new(4, 1, 2))];
        let said = |rig: &[(&str, &str, &str)]| {
            write_figure_with(&file, &parts, rig);
            load_model(&file).err().map(|e| format!("{e:#}")).unwrap_or_default()
        };
        let e = said(&[("arm", "_parent", "leg")]);
        assert!(e.contains("figure.vox") && e.contains("part 'arm' hangs on 'leg', but no part is called so"), "{e}");
        let e = said(&[("arm", "_parent", "head"), ("head", "_parent", "arm")]);
        assert!(e.contains("figure.vox") && e.contains("hangs on itself by way of the others") && e.contains("head on arm on head"), "{e}");
        let e = said(&[("head", "_parent", "head")]);
        assert!(e.contains("part 'head' hangs on itself"), "{e}");
        let e = said(&[("arm", "_joint", "1 2")]);
        assert!(e.contains("figure.vox") && e.contains("part 'arm': its joint '1 2' is not three numbers"), "{e}");
        assert!(said(&[("arm", "_joint", "1 2.5 -3")]).is_empty());
        std::fs::remove_dir_all(&dir).ok();
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
        let model = load_model(Path::new("games/moreblood/huey.vvx")).unwrap();
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
        let huey = load_model(Path::new("games/moreblood/huey.vvx")).unwrap();
        assert!(huey.frames.iter().zip(&huey.metal).all(|(f, m)| f.iter().zip(m).all(|((_, c), m)| c[3] == 255 || *m == 0)), "the cockpit mirrors");
        assert!(huey.facets.is_empty());
        let gem = load_model(Path::new("games/voxkeep/gem.vvx")).unwrap();
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
        let model = load_model(Path::new("games/moreblood/dogtags.vvx")).unwrap();
        let steel = model.frames[0].iter().zip(&model.metal[0]).filter(|((_, c), _)| c[..3] == [168, 172, 176]);
        assert!(steel.clone().count() > 100);
        assert!(steel.clone().all(|(_, &m)| m >> 4 == 15 && m & 15 > 6), "steel is not shiny metal");
        let blood = model.frames[0].iter().zip(&model.metal[0]).filter(|((_, c), _)| c[..3] == [150, 20, 18]);
        assert!(blood.clone().count() > 0 && blood.clone().all(|(_, &m)| m == 0), "the blood shines like metal");
    }
}

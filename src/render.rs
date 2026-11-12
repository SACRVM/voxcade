//! wgpu renderer. A sun shadow pass; the scene (chunk meshes, opaque
//! instanced cubes, then see-through ones far to near) into a colour target
//! and a glow target; both shrunk to a quarter and blurred; and the final
//! picture put together on screen: glow on top, blur towards the top and
//! bottom of the screen, a white flash after big blasts.

use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Quat, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::mesh::{self, FaceMask, Vertex};
use crate::world::{CHUNK, World};

/// Flashes and explosions lit at once (the shader's MAX_LIGHTS).
pub const MAX_LIGHTS: usize = 16;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The scene and glow targets: room above 1 for glow, filterable for blur.
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The blurred copies are this many times smaller on each side.
const SHRINK: u32 = 4;

/// One moving voxel: player parts, bullets, debris, sparks, fire, smoke.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    /// Centre in xyz, edge length in w.
    pub pos_size: [f32; 4],
    pub color: [u8; 4],
    /// How much it glows and how opaque it is (0..255 each), and the faces a
    /// neighbouring cube covers (bits in `mesh::DIRS` order; see
    /// `hide_inner_faces`).
    pub fx: [u8; 4],
    /// A direction (snorm xyz) the faces in w (bits in `mesh::DIRS` order)
    /// are lit by instead of their own (`vox::VoxModel::shade`).
    pub shade: [i8; 4],
    /// How it is turned about its centre, freely (a quaternion, snorm xyzw,
    /// `snorm_turn`); all zeros: square to the grid.
    pub turn: [i8; 4],
}

/// A free turn as an instance carries it (`Instance::turn`): none for no
/// turn at all, so cubes square to the grid stay exactly as they were.
pub fn snorm_turn(rot: Quat) -> [i8; 4] {
    if rot == Quat::IDENTITY {
        return [0; 4];
    }
    rot.normalize().to_array().map(|c| (c.clamp(-1.0, 1.0) * 127.0).round() as i8)
}

/// The opaque voxels of every model frame a game draws, put on the GPU once
/// (the renderer uploads a bank the first time a frame brings it). A model
/// is then drawn with one entry, `ModelDraw`, instead of a cube per voxel:
/// at 16-bit scale building those cubes was most of the CPU's frame.
#[derive(Default)]
pub struct ModelBank {
    /// Relative to the model's pivot (`pos_size` xyz, before the turn).
    pub voxels: Vec<Instance>,
}

/// One model frame drawn from the bank.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelDraw {
    /// Its voxels in the bank.
    pub voxels: Range<u32>,
    /// Where the pivot's cell has its minimum corner.
    pub origin: Vec3,
    /// Quarter turns: 0 facing the model's forward (+y), 1 facing +x, 2 -y,
    /// 3 -x (`quarter`).
    pub turn: u32,
    /// Turned freely after the quarter turns (identity: not at all) and
    /// shifted (`turned_centre`): a whole model about the pivot cell's
    /// middle at half its height, a figure's part about its joint too.
    pub rot: Quat,
    pub shift: Vec3,
    /// Every voxel this colour (a flash).
    pub tint: Option<[u8; 3]>,
    /// How many steps nearer the eye its depth is taken (`PART_NEARER` of
    /// a voxel each, the picture's place unchanged): a posed figure's part
    /// by how far down the figure it hangs, so where a bent limb's faces
    /// lie in one plane with its parent's the part further out wins,
    /// instead of both flickering through each other. 0: as it lies.
    pub nearer: f32,
}

/// Where a voxel of a freely turned model has its centre: `local` its
/// minimum corner from the pivot cell's, after the quarter turns; its
/// centre turned by `rot` and then shifted by `shift` (`turn_about`,
/// `ModelDraw`), as the shader's `model_centre` does it.
pub fn turned_centre(origin: Vec3, local: Vec3, shift: Vec3, rot: Quat) -> Vec3 {
    origin + shift + rot * (local + 0.5)
}

/// The shift that turns a model by `rot` about the point `about` (from the
/// pivot cell's minimum corner): a whole model's, the pivot cell's middle
/// at half its height.
pub fn turn_about(rot: Quat, about: Vec3) -> Vec3 {
    about - rot * about
}

/// Whether a draw leaves the grid: turned, or shifted (a figure's part).
pub fn off_grid(rot: Quat, shift: Vec3) -> bool {
    rot != Quat::IDENTITY || shift != Vec3::ZERO
}

/// A position or direction turned by `turn` quarter turns (`ModelDraw`), as
/// the shader's `quarter` turns it.
pub fn quarter(x: i32, y: i32, turn: u32) -> (i32, i32) {
    match turn % 4 {
        0 => (x, y),
        1 => (y, -x),
        2 => (-x, -y),
        _ => (-y, x),
    }
}

impl ModelBank {
    /// Puts a fixed set of cubes in the bank (the title logo's letters),
    /// relative to the origin a draw gives them; drawn unturned (the faces
    /// a neighbour covers, `hide_inner_faces`, stay as they are).
    pub fn add_cubes(&mut self, cubes: &[Instance]) -> Range<u32> {
        let start = self.voxels.len() as u32;
        self.voxels.extend(cubes.iter().map(|c| {
            let [x, y, z, size] = c.pos_size;
            Instance { pos_size: [x - 0.5, y - 0.5, z - 0.5, size], ..*c }
        }));
        start..self.voxels.len() as u32
    }

    /// Puts the opaque visible voxels of each of `model`'s frames in the
    /// bank and notes where (`VoxModel::bank`).
    pub fn add(&mut self, model: &mut crate::vox::VoxModel) {
        let pivot = IVec3::new(model.size.x / 2, model.size.y / 2, 0);
        model.bank = (0..model.frames.len())
            .map(|f| {
                let start = self.voxels.len() as u32;
                let opaque: crate::world::CellSet = model.frames[f].iter().filter(|(_, c)| c[3] == 255).map(|(v, _)| *v).collect();
                for &k in &model.visible[f] {
                    let k = k as usize;
                    let (v, color) = model.frames[f][k];
                    if color[3] < 255 {
                        continue;
                    }
                    let l = (v - pivot).as_vec3();
                    let metal = model.metal.get(f).and_then(|m| m.get(k)).copied().unwrap_or(0);
                    let shade = model.shade.get(f).and_then(|s| s.get(k)).copied().unwrap_or([0; 4]);
                    self.voxels.push(Instance { pos_size: [l.x, l.y, l.z, 1.0], color, fx: [0, 255, covered(&opaque, v), metal], shade, turn: [0; 4] });
                }
                start..self.voxels.len() as u32
            })
            .collect();
        // A figure's parts: each its own run, its joints' faces too.
        for part in &mut model.parts {
            let start = self.voxels.len() as u32;
            let opaque: crate::world::CellSet = part.voxels.iter().map(|&k| model.frames[0][k as usize]).filter(|(_, c)| c[3] == 255).map(|(v, _)| v).collect();
            for &k in &part.visible {
                let k = k as usize;
                let (v, color) = model.frames[0][k];
                if color[3] < 255 {
                    continue;
                }
                let l = (v - pivot).as_vec3();
                let metal = model.metal.first().and_then(|m| m.get(k)).copied().unwrap_or(0);
                let shade = model.shade.first().and_then(|s| s.get(k)).copied().unwrap_or([0; 4]);
                self.voxels.push(Instance { pos_size: [l.x, l.y, l.z, 1.0], color, fx: [0, 255, covered(&opaque, v), metal], shade, turn: [0; 4] });
            }
            part.bank = start..self.voxels.len() as u32;
        }
    }

    /// The cubes a draw puts in the world, worked out as the GPU does
    /// (`vs_model`): for tests.
    #[cfg(test)]
    pub fn cubes<'a>(&'a self, draw: &'a ModelDraw) -> impl Iterator<Item = Instance> + 'a {
        let turn = draw.turn;
        let free = off_grid(draw.rot, draw.shift);
        // The faces lit by the shade direction, counted in the world's
        // directions after the turn.
        let dirs = mesh::DIRS;
        let to: [usize; 6] = std::array::from_fn(|f| {
            let (x, y) = quarter(dirs[f].x, dirs[f].y, turn);
            dirs.iter().position(|e| *e == IVec3::new(x, y, dirs[f].z)).unwrap_or(f)
        });
        self.voxels[draw.voxels.start as usize..draw.voxels.end as usize].iter().map(move |v| {
            let [lx, ly, lz, size] = v.pos_size;
            let (x, y) = quarter(lx as i32, ly as i32, turn);
            let local = Vec3::new(x as f32, y as f32, lz);
            let centre = if free { turned_centre(draw.origin, local, draw.shift, draw.rot) } else { draw.origin + local + 0.5 };
            let [nx, ny, nz, faces] = v.shade;
            let (nx, ny) = quarter(nx as i32, ny as i32, turn);
            let lit = (0..6).filter(|f| faces >> f & 1 == 1).fold(0i8, |bits, f| bits | 1 << to[f]);
            let color = draw.tint.map_or(v.color, |[r, g, b]| [r, g, b, v.color[3]]);
            let shade = [nx as i8, ny as i8, nz, lit];
            Instance { pos_size: centre.extend(size).to_array(), color, fx: v.fx, shade, turn: snorm_turn(draw.rot) }
        })
    }
}

/// A model draw's place on the GPU: a uniform at its own offset.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ActorData {
    /// xyz: the origin, w: quarter turns.
    origin: [f32; 4],
    /// rgb: the flash colour, w: 1 while it flashes.
    tint: [f32; 4],
    /// x: how many steps nearer the eye its depth is taken (`ModelDraw::nearer`).
    extra: [f32; 4],
    /// The free turn (xyzw; 0 0 0 1 none), and the shift after it (xyz;
    /// w 1 when it is off the grid at all).
    rot: [f32; 4],
    pivot: [f32; 4],
}

/// The faces of the voxel at `v` that another opaque voxel of its model (or
/// of its part) lies against, as bits in `mesh::DIRS` order, in the model's
/// own directions. They are not drawn: seen nearly edge on, such a face
/// between two voxels of a surface lies at the surface's own depth along
/// the edge, and won single pixels there (dots in the neighbouring face's
/// shade all over a turned figure).
fn covered(opaque: &crate::world::CellSet, v: IVec3) -> u8 {
    mesh::DIRS.iter().enumerate().filter(|(_, d)| opaque.contains(&(v + **d))).fold(0, |bits, (bit, _)| bits | 1 << bit)
}

/// Model draws' uniforms lie this far apart (the most any GPU asks).
const ACTOR_STRIDE: usize = 256;

/// Up to this many cells around the cubes `hide_inner_faces` keeps a bit per
/// cell (2 MB); more, and it looks them up in a hash set.
const DENSE_CELLS: i64 = 1 << 24;

/// Marks the faces of opaque unit cubes that touch another opaque unit
/// cube, so they are not drawn: they are never seen, and where such a face
/// meets the neighbour's surface at the same depth it shows as a seam.
/// Runs every frame over the UI's tens of thousands of voxels, so which
/// cells are taken is a bit per cell of the box around them.
pub fn hide_inner_faces(instances: &mut [Instance]) {
    let solid = |i: &Instance| i.pos_size[3] == 1.0 && i.fx[1] == 255;
    let cell = |i: &Instance| glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
    let (mut lo, mut hi) = (glam::IVec3::MAX, glam::IVec3::MIN);
    for i in instances.iter().filter(|i| solid(i)) {
        lo = lo.min(cell(i));
        hi = hi.max(cell(i));
    }
    if lo.x > hi.x {
        return;
    }
    let size = hi.as_i64vec3() - lo.as_i64vec3() + 1;
    let volume = size.x.saturating_mul(size.y).saturating_mul(size.z);
    fn mark(instances: &mut [Instance], solid: impl Fn(&Instance) -> bool, cell: impl Fn(&Instance) -> glam::IVec3, taken: impl Fn(glam::IVec3) -> bool) {
        for i in instances.iter_mut().filter(|i| solid(i)) {
            let c = cell(i);
            let mut hidden = 0u8;
            for (bit, d) in crate::mesh::DIRS.iter().enumerate() {
                if taken(c + *d) {
                    hidden |= 1 << bit;
                }
            }
            i.fx[2] = hidden;
        }
    }
    if volume > DENSE_CELLS {
        let cells: crate::world::CellSet = instances.iter().filter(|i| solid(i)).map(cell).collect();
        mark(instances, solid, cell, |c| cells.contains(&c));
        return;
    }
    let (sx, sy) = (size.x as usize, size.y as usize);
    let index = |c: glam::IVec3| {
        let d = c - lo;
        (d.z as usize * sy + d.y as usize) * sx + d.x as usize
    };
    let mut bits = vec![0u64; (volume as usize).div_ceil(64)];
    for i in instances.iter().filter(|i| solid(i)) {
        let n = index(cell(i));
        bits[n / 64] |= 1 << (n % 64);
    }
    mark(instances, solid, cell, |c| {
        c.cmpge(lo).all() && c.cmple(hi).all() && {
            let n = index(c);
            bits[n / 64] >> (n % 64) & 1 == 1
        }
    });
}

/// Marks the faces two see-through unit cubes share, so water, glass and
/// the like are one joined body and not a grid of darker stripes where the
/// faces between cubes blend on top of each other. Only cubes in
/// neighbouring cells that lie alike within their cells (a column raised by
/// a wave does not, and keeps its faces) and are not turned freely join;
/// colours may differ. Opaque cubes are left alone, and so are bits already
/// set. Needs every see-through cube of the frame at once.
///
/// One sort of shared face stays (the user's idea): a skin for every layer
/// of the body, each a whole plane without a grid, so it shows how thick it
/// is by itself: three voxels of water are as dense as three sheets were.
/// The layers lie across the body where it is thin at that cube (water
/// lies flat, a pane of glass may stand, the user: the shortest run of
/// see-through cubes through it, along x, y or z, where that is half the
/// next or less); where it is about as thick every way, a block or a gem,
/// across the way turned most towards the eye (`toward`, from the cubes to
/// the eye; the upright one in a near tie, as a view from above at 45
/// degrees is). Of the two faces shared along that
/// way the one facing the eye stays.
pub fn join_see_through(instances: &mut [Instance], toward: Vec3) {
    let joins = |i: &Instance| i.pos_size[3] == 1.0 && i.fx[1] < 255 && i.turn == [0; 4];
    let cell = |i: &Instance| glam::Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
    let mut at: crate::world::CellMap<glam::Vec3> = crate::world::CellMap::default();
    for i in instances.iter().filter(|i| joins(i)) {
        at.insert(cell(i), glam::Vec3::from_slice(&i.pos_size[..3]) - cell(i).as_vec3());
    }
    if at.len() < 2 {
        return;
    }
    // The ways by how much they face the eye, the most first.
    let a = toward.abs();
    let first = if a.z >= 0.8 * a.max_element() { 2 } else if a.y >= a.x { 1 } else { 0 };
    let mut ways = [first, (first + 1) % 3, (first + 2) % 3];
    if a[ways[2]] > a[ways[1]] {
        ways.swap(1, 2);
    }
    // No run is counted further than this each way: thick is thick.
    const FAR: i32 = 16;
    for i in instances.iter_mut().filter(|i| joins(i)) {
        let c = cell(i);
        let within = glam::Vec3::from_slice(&i.pos_size[..3]) - c.as_vec3();
        let joined = |to: glam::IVec3| at.get(&to).is_some_and(|w| (*w - within).abs().max_element() <= 0.01);
        // How many cubes the body is through here, along each way.
        let run = |axis: usize| {
            let step = crate::mesh::DIRS[axis * 2];
            let count = |d: glam::IVec3| (1..=FAR).take_while(|n| joined(c + d * *n)).count();
            1 + count(step) + count(-step)
        };
        let runs = [run(0), run(1), run(2)];
        // Thin here: half as far through as any other way, or less. A
        // body about as thick every way (a gem's middle) goes by the view,
        // or its cubes would each choose their own and the skins lie in
        // patches.
        let mut sorted = runs;
        sorted.sort_unstable();
        let across = if sorted[0] * 2 <= sorted[1] { *ways.iter().find(|w| runs[**w] == sorted[0]).expect("one is the shortest") } else { ways[0] };
        let kept = across * 2 + (toward[across] < 0.0) as usize;
        for (bit, d) in crate::mesh::DIRS.iter().enumerate() {
            if bit != kept && joined(c + *d) {
                i.fx[2] |= 1 << bit;
            }
        }
    }
}

/// A light from a flash or an explosion.
#[derive(Clone, Copy)]
pub struct Light {
    pub pos: Vec3,
    pub radius: f32,
    /// Colour times brightness.
    pub color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightData {
    pos_radius: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_vp: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    params: [f32; 4],
    eye: [f32; 4],
    haze: [f32; 4],
    misc: [f32; 4],
    /// rgb: the sky metal mirrors, overhead and at the horizon.
    reflection: [f32; 4],
    horizon: [f32; 4],
    /// rgb: the models' outline colour, w: how many voxels thick (0: none).
    outline: [f32; 4],
    /// rgb: the sunlight's colour and strength.
    sun_color: [f32; 4],
    /// x: the picture's number, y: 1 when the faces remember their shadow.
    memory: [u32; 4],
    lights: [LightData; MAX_LIGHTS],
}

/// How many faces' shadows the memory holds (the shader's FACES; three
/// words each, `FrameData::remember`).
const SHADOW_FACES: u64 = 65536;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostParams {
    params: [f32; 4],
    step: [f32; 4],
}

/// Everything the renderer needs for one frame.
pub struct FrameData {
    pub view_proj: Mat4,
    pub light_vp: Mat4,
    /// Whether each face keeps a memory of its shadow and changes only
    /// when it is clearly the other way: a game moving its sun, whose
    /// shadow map turns every picture (the shader's `remembered`).
    pub remember: bool,
    pub sun_dir: Vec3,
    /// The sunlight's colour and strength (white: as it always was).
    pub sun_color: [f32; 3],
    pub depth_bias: f32,
    pub ambient: f32,
    pub shadow: f32,
    /// 0 smooth, 1 voxel, 2 voxel with half shadow (`ShadowStyle`).
    pub shadow_style: f32,
    pub sky: [f32; 3],
    /// The sky metal mirrors, overhead and at the horizon, and how bright
    /// the streak sweeping over metal is.
    pub reflection: [f32; 3],
    pub reflection_horizon: [f32; 3],
    pub metal_sweep: f32,
    /// Seconds, for what moves by itself (the sweep).
    pub time: f32,
    pub eye: Vec3,
    /// Haze strength and colour, where it starts (distance from the eye)
    /// and over how far it builds up.
    pub haze: f32,
    pub haze_color: [f32; 3],
    pub haze_start: f32,
    pub haze_depth: f32,
    pub lights: Vec<Light>,
    pub glow: f32,
    /// The shadow map's width and height (`light_vp` is snapped to it).
    pub shadow_map: u32,
    pub tilt_shift: f32,
    pub screen_flash: f32,
    /// The line round the models: its colour (0..1) and how many voxels
    /// thick (0: none; `c.effects.outline`).
    pub outline: [f32; 4],
    /// A fade over the whole picture: its colour (0..1) and how far, 0..1
    /// (a level change, `Session::fade`).
    pub fade: [f32; 4],
    /// Models drawn from the bank, before the cubes; and in the UI's scene
    /// (the title's logo).
    pub models: Vec<ModelDraw>,
    pub ui_models: Vec<ModelDraw>,
    pub bank: Arc<ModelBank>,
    /// Opaque cubes first, then the last `see_through` ones, sorted far to near.
    pub instances: Vec<Instance>,
    pub see_through: usize,
    /// The UI's voxels, in their own scene over the world, and its camera.
    pub ui: Vec<Instance>,
    pub ui_view_proj: Mat4,
}

impl FrameData {
    /// Every cube of the world's moving things, the models' worked out as
    /// the GPU does: what tests look at.
    #[cfg(test)]
    pub fn cubes(&self) -> Vec<Instance> {
        self.models.iter().flat_map(|m| self.bank.cubes(m)).chain(self.instances.iter().copied()).collect()
    }

    /// The same for the UI's scene.
    #[cfg(test)]
    pub fn ui_cubes(&self) -> Vec<Instance> {
        self.ui_models.iter().flat_map(|m| self.bank.cubes(m)).chain(self.ui.iter().copied()).collect()
    }
}

/// The UI scene's light: from the upper left and in front.
const UI_SUN: Vec3 = Vec3::new(0.45, -0.6, -1.0);
const UI_AMBIENT: f32 = 0.55;

#[derive(Default, Clone, Copy)]
pub struct Stats {
    /// Cubes drawn: the world's (moving things) and the UI's.
    pub instances: usize,
    /// Models drawn from the bank, and their cubes.
    pub models: usize,
    pub model_cubes: usize,
    pub ui_instances: usize,
    pub chunks: usize,
    pub drawn: usize,
    /// Chunks drawn into the shadow map.
    pub shadowed: usize,
    /// Vertices of every chunk mesh, drawn or not (what sits in GPU memory).
    pub vertices_total: usize,
    pub vertices: usize,
}

/// A chunk's mesh on the GPU; its buffers may be bigger than the mesh.
struct ChunkMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    vertex_count: usize,
    min: Vec3,
    max: Vec3,
}

impl ChunkMesh {
    fn vertex_bytes(&self) -> u64 {
        (self.vertex_count * size_of::<Vertex>()) as u64
    }

    fn index_bytes(&self) -> u64 {
        u64::from(self.index_count) * 4
    }
}

/// Everything sized to the window: depth, the scene and glow targets, their
/// quarter-size copies, and what the post passes read.
struct Targets {
    depth: wgpu::TextureView,
    scene: wgpu::TextureView,
    glow: wgpu::TextureView,
    /// The UI, over the world after the blur.
    ui: wgpu::TextureView,
    /// Quarter size: scene blurred (0) and its scratch (1), glow blurred (2)
    /// and its scratch (3).
    small: [wgpu::TextureView; 4],
    /// The post passes in order: what they do, what they read, which small
    /// target they write.
    passes: Vec<(PostPass, wgpu::BindGroup, usize)>,
    /// The composite pass: the sharp scene plus the two blurred copies.
    composite: wgpu::BindGroup,
    blurred: wgpu::BindGroup,
    composite_params: wgpu::Buffer,
}

#[derive(Clone, Copy)]
enum PostPass {
    Down,
    Blur,
}

pub struct Renderer {
    globals: wgpu::Buffer,
    shadow_bind: wgpu::BindGroup,
    main_bind: wgpu::BindGroup,
    ui_globals: wgpu::Buffer,
    ui_bind: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    /// The shadow map's width and height (`settings::Shadows`).
    shadow_size: u32,
    shadow_sampler: wgpu::Sampler,
    /// The faces' memory of their shadow (`FrameData::remember`), and the
    /// pictures drawn, to tell the faces seen lately.
    faces: wgpu::Buffer,
    pictures: u32,
    main_layout: wgpu::BindGroupLayout,
    chunk_main: wgpu::RenderPipeline,
    chunk_shadow: wgpu::RenderPipeline,
    inst_main: wgpu::RenderPipeline,
    inst_see_through: wgpu::RenderPipeline,
    inst_shadow: wgpu::RenderPipeline,
    model_main: wgpu::RenderPipeline,
    /// The models' outline: their voxels grown, the far faces drawn
    /// (`c.effects.outline`).
    model_outline: wgpu::RenderPipeline,
    model_shadow: wgpu::RenderPipeline,
    post_down: wgpu::RenderPipeline,
    post_blur: wgpu::RenderPipeline,
    post_composite: wgpu::RenderPipeline,
    post_layout: wgpu::BindGroupLayout,
    blurred_layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    targets: Targets,
    cube_vertices: wgpu::Buffer,
    cube_indices: wgpu::Buffer,
    cube_index_count: u32,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    /// The model bank on the GPU, and which bank it holds.
    bank: Option<(Arc<ModelBank>, wgpu::Buffer)>,
    /// The model draws' uniforms (`ActorData`), room for so many.
    actors: wgpu::Buffer,
    actor_capacity: usize,
    actor_layout: wgpu::BindGroupLayout,
    actor_bind: wgpu::BindGroup,
    /// By chunk; a `CellMap`, so chunks are drawn in the same order every
    /// run (where two faces tie in depth, the same one wins).
    meshes: crate::world::CellMap<ChunkMesh>,
    scratch_vertices: Vec<Vertex>,
    scratch_indices: Vec<u32>,
    pub stats: Stats,
    /// GPU time per pass, when asked for (`time_passes`).
    pass_times: Option<PassTimes>,
}

/// Fewer chunks than this a thread are meshed on the calling thread: a
/// thread costs more to start than they take.
const MIN_CHUNKS_A_THREAD: usize = 2;

/// At most this many passes a frame are timed.
const MAX_TIMED_PASSES: u32 = 32;

/// How long the GPU takes for each render pass, from timestamp queries:
/// each pass writes one when it starts and one when it ends.
struct PassTimes {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// Nanoseconds a timestamp tick.
    period: f32,
    /// The passes of the frame being drawn, in order: pass i writes the
    /// queries 2i and 2i + 1.
    labels: RefCell<Vec<&'static str>>,
}

impl PassTimes {
    /// Where pass `label` writes its timestamps (None once the set is full).
    fn writes(&self, label: &'static str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let mut labels = self.labels.borrow_mut();
        let i = labels.len() as u32;
        if i >= MAX_TIMED_PASSES {
            return None;
        }
        labels.push(label);
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(2 * i),
            end_of_pass_write_index: Some(2 * i + 1),
        })
    }
}
const CHUNK_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4, 2 => Unorm8x4];
const CUBE_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4];
const INSTANCE_ATTRS: [wgpu::VertexAttribute; 5] =
    wgpu::vertex_attr_array![2 => Float32x4, 3 => Unorm8x4, 4 => Unorm8x4, 5 => Snorm8x4, 6 => Snorm8x4];

fn texture(device: &wgpu::Device, label: &str, width: u32, height: u32, format: wgpu::TextureFormat) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxcade shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxcade post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post.wgsl").into()),
        });

        let globals_buffer = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let globals = globals_buffer("globals");
        let ui_globals = globals_buffer("ui globals");

        let shadow_size = crate::settings::Shadows::High.map_size();
        let shadow_view = Self::shadow_map(device, shadow_size);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let globals_entry = uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT);
        // The shadow pass writes the shadow map, so it cannot also bind it.
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow pass"),
            entries: &[globals_entry],
        });
        let main_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("main pass"),
            entries: &[
                globals_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post source"),
            entries: &[
                texture_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let blurred_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post blurred"),
            entries: &[texture_entry(0), texture_entry(1), texture_entry(2)],
        });

        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow pass"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let faces = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("faces' shadows"),
            size: SHADOW_FACES * 3 * 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let (main_bind, ui_bind) = Self::scene_binds(device, &main_layout, &globals, &ui_globals, &shadow_view, &shadow_sampler, &faces);

        let layout = |label: &str, groups: &[Option<&wgpu::BindGroupLayout>]| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: groups,
                immediate_size: 0,
            })
        };
        // A model draw's place, its uniform at an offset given per draw.
        let actor_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model draw"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<ActorData>() as u64),
                },
                count: None,
            }],
        });
        let shadow_pipeline_layout = layout("shadow pass", &[Some(&shadow_layout)]);
        let main_pipeline_layout = layout("main pass", &[Some(&main_layout)]);
        let model_shadow_layout = layout("model shadows", &[Some(&shadow_layout), Some(&actor_layout)]);
        let model_main_layout = layout("models", &[Some(&main_layout), Some(&actor_layout)]);
        let post_pipeline_layout = layout("post", &[Some(&post_layout)]);
        let composite_pipeline_layout = layout("composite", &[Some(&post_layout), Some(&blurred_layout)]);

        let chunk_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &CHUNK_ATTRS,
        })];
        let instance_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &CUBE_ATTRS,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &INSTANCE_ATTRS,
            }),
        ];

        let scene_target = |blend: Option<wgpu::BlendState>| {
            Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL })
        };
        let opaque_targets = [scene_target(None), scene_target(None)];
        let see_through_targets =
            [scene_target(Some(wgpu::BlendState::ALPHA_BLENDING)), scene_target(Some(wgpu::BlendState::ALPHA_BLENDING))];

        // `targets` empty: a shadow pass. `depth_write` false: see-through.
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        targets: &[Option<wgpu::ColorTargetState>],
                        depth_write: bool| {
            let shadow = targets.is_empty();
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    // Meshes only contain camera- and sun-facing faces; the
                    // shadow pass must see all of them.
                    cull_mode: if shadow { None } else { Some(wgpu::Face::Back) },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: if shadow {
                        wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 }
                    } else {
                        Default::default()
                    },
                }),
                multisample: Default::default(),
                fragment: if shadow {
                    None
                } else {
                    Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets,
                    })
                },
                multiview_mask: None,
                cache: None,
            })
        };

        let chunk_main = pipeline("chunks", &main_pipeline_layout, "vs_chunk", &chunk_buffers, &opaque_targets, true);
        let chunk_shadow = pipeline("chunk shadows", &shadow_pipeline_layout, "vs_chunk_shadow", &chunk_buffers, &[], true);
        let inst_main = pipeline("instances", &main_pipeline_layout, "vs_inst", &instance_buffers, &opaque_targets, true);
        let inst_see_through =
            pipeline("see-through", &main_pipeline_layout, "vs_inst", &instance_buffers, &see_through_targets, false);
        let inst_shadow = pipeline("instance shadows", &shadow_pipeline_layout, "vs_inst_shadow", &instance_buffers, &[], true);
        let model_main = pipeline("models", &model_main_layout, "vs_model", &instance_buffers, &opaque_targets, true);
        let model_shadow = pipeline("model shadows", &model_shadow_layout, "vs_model_shadow", &instance_buffers, &[], true);
        // The outline: the far faces of the grown voxels, in a flat colour.
        let model_outline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model outlines"),
            layout: Some(&model_main_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_model_outline"), compilation_options: Default::default(), buffers: &instance_buffers },
            primitive: wgpu::PrimitiveState { front_face: wgpu::FrontFace::Ccw, cull_mode: Some(wgpu::Face::Front), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_outline"), compilation_options: Default::default(), targets: &opaque_targets }),
            multiview_mask: None,
            cache: None,
        });

        let post_pipeline = |label: &str, layout: &wgpu::PipelineLayout, fs: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &post_shader,
                    entry_point: Some("vs_post"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &post_shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let post_down = post_pipeline("shrink", &post_pipeline_layout, "fs_down", HDR_FORMAT);
        let post_blur = post_pipeline("blur", &post_pipeline_layout, "fs_blur", HDR_FORMAT);
        let post_composite = post_pipeline("composite", &composite_pipeline_layout, "fs_composite", format);

        let (cube_v, cube_i) = mesh::unit_cube();
        let cube_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck::cast_slice(&cube_v),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let cube_indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: bytemuck::cast_slice(&cube_i),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_capacity = 4096;
        let instances = Self::instance_buffer(device, instance_capacity);
        let actor_capacity = 64;
        let (actors, actor_bind) = Self::actor_buffer(device, &actor_layout, actor_capacity);
        let targets = Self::targets(device, &post_layout, &blurred_layout, &linear, width, height);

        Self {
            globals,
            shadow_bind,
            main_bind,
            ui_globals,
            ui_bind,
            shadow_view,
            shadow_size,
            shadow_sampler,
            faces,
            pictures: 0,
            main_layout,
            chunk_main,
            chunk_shadow,
            inst_main,
            inst_see_through,
            inst_shadow,
            model_main,
            model_outline,
            model_shadow,
            post_down,
            post_blur,
            post_composite,
            post_layout,
            blurred_layout,
            linear,
            targets,
            cube_vertices,
            cube_indices,
            cube_index_count: cube_i.len() as u32,
            instances,
            instance_capacity,
            bank: None,
            actors,
            actor_capacity,
            actor_layout,
            actor_bind,
            meshes: crate::world::CellMap::default(),
            scratch_vertices: Vec::new(),
            scratch_indices: Vec::new(),
            stats: Stats::default(),
            pass_times: None,
        }
    }

    fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (capacity * size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn shadow_map(device: &wgpu::Device, size: u32) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("shadow map"),
                size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// What the world's and the UI's scene read: their globals and the
    /// shadow map.
    fn scene_binds(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        ui_globals: &wgpu::Buffer,
        shadow_view: &wgpu::TextureView,
        shadow_sampler: &wgpu::Sampler,
        faces: &wgpu::Buffer,
    ) -> (wgpu::BindGroup, wgpu::BindGroup) {
        let bind = |label, buffer: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(shadow_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(shadow_sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: faces.as_entire_binding() },
                ],
            })
        };
        (bind("main pass", globals), bind("ui pass", ui_globals))
    }

    fn actor_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model draws"),
            size: (capacity * ACTOR_STRIDE) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model draws"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<ActorData>() as u64),
                }),
            }],
        });
        (buffer, bind)
    }

    fn targets(
        device: &wgpu::Device,
        post_layout: &wgpu::BindGroupLayout,
        blurred_layout: &wgpu::BindGroupLayout,
        linear: &wgpu::Sampler,
        width: u32,
        height: u32,
    ) -> Targets {
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let scene = texture(device, "scene", width, height, HDR_FORMAT);
        let glow = texture(device, "glow", width, height, HDR_FORMAT);
        let ui = texture(device, "ui", width, height, HDR_FORMAT);
        let (sw, sh) = ((width / SHRINK).max(1), (height / SHRINK).max(1));
        let small = std::array::from_fn(|i| texture(device, &format!("small {i}"), sw, sh, HDR_FORMAT));

        let bind = |view: &wgpu::TextureView, params: PostParams| {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("post params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post source"),
                layout: post_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(linear) },
                    wgpu::BindGroupEntry { binding: 2, resource: buffer.as_entire_binding() },
                ],
            });
            (group, buffer)
        };
        let full = [1.0 / width.max(1) as f32, 1.0 / height.max(1) as f32];
        let quarter = [1.0 / sw as f32, 1.0 / sh as f32];
        let down = PostParams { params: [0.0; 4], step: [full[0], full[1], 0.0, 0.0] };
        let across = PostParams { params: [0.0; 4], step: [quarter[0], quarter[1], 1.0, 0.0] };
        let upwards = PostParams { params: [0.0; 4], step: [quarter[0], quarter[1], 0.0, 1.0] };

        // Scene: shrink, blur across, blur up. Glow: shrink, then blur twice
        // for a wider halo.
        let passes = vec![
            (PostPass::Down, bind(&scene, down).0, 0),
            (PostPass::Blur, bind(&small[0], across).0, 1),
            (PostPass::Blur, bind(&small[1], upwards).0, 0),
            (PostPass::Down, bind(&glow, down).0, 2),
            (PostPass::Blur, bind(&small[2], across).0, 3),
            (PostPass::Blur, bind(&small[3], upwards).0, 2),
            (PostPass::Blur, bind(&small[2], across).0, 3),
            (PostPass::Blur, bind(&small[3], upwards).0, 2),
        ];
        let (composite, composite_params) = bind(&scene, down);
        let blurred = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post blurred"),
            layout: blurred_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&small[0]) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&small[2]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&ui) },
            ],
        });
        Targets { depth, scene, glow, ui, small, passes, composite, blurred, composite_params }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.targets = Self::targets(device, &self.post_layout, &self.blurred_layout, &self.linear, width, height);
    }

    /// Rebuilds the meshes of the chunks that changed; how many.
    pub fn update_chunks(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &mut World, mask: &FaceMask) -> usize {
        let dirty = world.take_dirty();
        self.remesh_all(device, queue, world, &dirty, mask);
        dirty.len()
    }

    /// Throws away all meshes and builds every chunk again.
    pub fn rebuild_all(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &mut World, mask: &FaceMask) {
        self.meshes.clear();
        world.take_dirty();
        let chunks = world.chunk_keys();
        self.remesh_all(device, queue, world, &chunks, mask);
    }

    /// Meshes `chunks`. A few on this thread; more spread over the CPU's
    /// cores (each chunk's mesh depends on the world alone, so they can be
    /// built at once: after a blast at 16-bit scale some fifty change in a
    /// tick), each core taking the next one not yet taken. They go to the
    /// GPU in the order given, so every run builds the same.
    fn remesh_all(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &World, chunks: &[IVec3], mask: &FaceMask) {
        let threads = crate::par::cores().min(chunks.len() / MIN_CHUNKS_A_THREAD);
        if threads <= 1 {
            for &cc in chunks {
                let (mut vertices, mut indices) = (std::mem::take(&mut self.scratch_vertices), std::mem::take(&mut self.scratch_indices));
                mesh::mesh_chunk(world, cc, mask, &mut vertices, &mut indices);
                self.upload(device, queue, cc, &vertices, &indices);
                (self.scratch_vertices, self.scratch_indices) = (vertices, indices);
            }
            return;
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let mut built: Vec<(usize, Vec<Vertex>, Vec<u32>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut out = Vec::new();
                        loop {
                            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(&cc) = chunks.get(k) else { break };
                            let (mut vertices, mut indices) = (Vec::new(), Vec::new());
                            mesh::mesh_chunk(world, cc, mask, &mut vertices, &mut indices);
                            out.push((k, vertices, indices));
                        }
                        out
                    })
                })
                .collect();
            workers.into_iter().flat_map(|w| w.join().expect("meshing a chunk")).collect()
        });
        built.sort_unstable_by_key(|b| b.0);
        for (k, vertices, indices) in built {
            self.upload(device, queue, chunks[k], &vertices, &indices);
        }
    }

    /// Puts a chunk's mesh on the GPU (or drops it when it has none). A
    /// chunk that had a mesh keeps its buffers and the new mesh is written
    /// over the old one; when it outgrows them, the new ones get room to grow
    /// (a chunk that changed once, by a blast or debris settling, tends to
    /// change again). A quarter faster after a blast than new buffers each
    /// time, and the GPU memory does not churn.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, cc: IVec3, vertices: &[Vertex], indices: &[u32]) {
        if indices.is_empty() {
            self.meshes.remove(&cc);
            return;
        }
        let (vertex_bytes, index_bytes): (&[u8], &[u8]) = (bytemuck::cast_slice(vertices), bytemuck::cast_slice(indices));
        let old = self.meshes.remove(&cc);
        let rebuilt = old.is_some();
        let buffer = |old: Option<wgpu::Buffer>, bytes: &[u8], usage, label| {
            let fits = old.filter(|b| b.size() >= bytes.len() as u64);
            let buffer = fits.unwrap_or_else(|| {
                // Exact for a chunk built the first time, half again for one rebuilt.
                let room = if rebuilt { bytes.len() / 2 } else { 0 };
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: ((bytes.len() + room) as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
                    usage: usage | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            });
            queue.write_buffer(&buffer, 0, bytes);
            buffer
        };
        let (old_vertices, old_indices) = old.map(|m| (m.vertices, m.indices)).unzip();
        let min = (cc * CHUNK).as_vec3();
        let mesh = ChunkMesh {
            vertices: buffer(old_vertices, vertex_bytes, wgpu::BufferUsages::VERTEX, "chunk vertices"),
            indices: buffer(old_indices, index_bytes, wgpu::BufferUsages::INDEX, "chunk indices"),
            index_count: indices.len() as u32,
            vertex_count: vertices.len(),
            min,
            max: min + Vec3::splat(CHUNK as f32),
        };
        self.meshes.insert(cc, mesh);
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &FrameData,
    ) {
        // A shadow map of the size the frame was made for (the player's
        // setting); a new one when it changes.
        if frame.shadow_map != self.shadow_size {
            self.shadow_size = frame.shadow_map;
            self.shadow_view = Self::shadow_map(device, self.shadow_size);
            (self.main_bind, self.ui_bind) =
                Self::scene_binds(device, &self.main_layout, &self.globals, &self.ui_globals, &self.shadow_view, &self.shadow_sampler, &self.faces);
        }
        self.pictures = self.pictures.wrapping_add(1);
        let mut lights = [LightData::zeroed(); MAX_LIGHTS];
        for (slot, l) in lights.iter_mut().zip(&frame.lights) {
            *slot = LightData { pos_radius: l.pos.extend(l.radius).to_array(), color: [l.color[0], l.color[1], l.color[2], 0.0] };
        }
        let light_count = frame.lights.len().min(MAX_LIGHTS);
        let [hr, hg, hb] = frame.haze_color;
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            light_vp: frame.light_vp.to_cols_array_2d(),
            sun_dir: frame.sun_dir.extend(frame.depth_bias).to_array(),
            params: [frame.ambient, frame.shadow, 1.0 / self.shadow_size as f32, light_count as f32],
            eye: frame.eye.extend(frame.haze).to_array(),
            haze: [hr, hg, hb, frame.haze_start],
            misc: [1.0 / frame.haze_depth.max(1.0), frame.shadow_style, frame.time, frame.metal_sweep],
            reflection: [frame.reflection[0], frame.reflection[1], frame.reflection[2], 0.0],
            horizon: [frame.reflection_horizon[0], frame.reflection_horizon[1], frame.reflection_horizon[2], 0.0],
            outline: frame.outline,
            sun_color: [frame.sun_color[0], frame.sun_color[1], frame.sun_color[2], 0.0],
            memory: [self.pictures, u32::from(frame.remember), 0, 0],
            lights,
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        // The UI scene: its own camera and sun, no shadows (the light
        // matrix puts everything outside the shadow map), no haze, no flares.
        let ui_globals = Globals {
            view_proj: frame.ui_view_proj.to_cols_array_2d(),
            light_vp: Mat4::from_translation(Vec3::new(10.0, 10.0, 0.0)).to_cols_array_2d(),
            sun_dir: UI_SUN.normalize().extend(0.0).to_array(),
            params: [UI_AMBIENT, 0.0, 1.0 / self.shadow_size as f32, 0.0],
            eye: [0.0, 0.0, 1000.0, 0.0],
            haze: [0.0; 4],
            misc: [1.0, 0.0, 0.0, 0.0],
            reflection: [0.4, 0.53, 0.76, 0.0],
            horizon: [1.0, 0.93, 0.8, 0.0],
            outline: [0.0; 4],
            sun_color: [1.0, 1.0, 1.0, 0.0],
            memory: [0; 4],
            lights: [LightData::zeroed(); MAX_LIGHTS],
        };
        queue.write_buffer(&self.ui_globals, 0, bytemuck::bytes_of(&ui_globals));
        let composite = PostParams { params: [frame.glow, frame.tilt_shift, frame.screen_flash, 0.0], step: frame.fade };
        queue.write_buffer(&self.targets.composite_params, 0, bytemuck::bytes_of(&composite));

        // The world's cubes, then the UI's, in one buffer.
        let instance_count = frame.instances.len();
        let opaque = (instance_count - frame.see_through.min(instance_count)) as u32;
        let total = instance_count + frame.ui.len();
        if total > self.instance_capacity {
            self.instance_capacity = total.next_power_of_two();
            self.instances = Self::instance_buffer(device, self.instance_capacity);
        }
        if instance_count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&frame.instances));
        }
        if !frame.ui.is_empty() {
            let offset = (instance_count * size_of::<Instance>()) as u64;
            queue.write_buffer(&self.instances, offset, bytemuck::cast_slice(&frame.ui));
        }

        // The model bank the first time it comes (and again after models
        // reload), the draws' places every frame.
        // The world's model draws, then the UI's: their places in that order.
        let all_models = frame.models.len() + frame.ui_models.len();
        if all_models > 0 && !self.bank.as_ref().is_some_and(|(b, _)| Arc::ptr_eq(b, &frame.bank)) {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("model bank"),
                contents: bytemuck::cast_slice(&frame.bank.voxels),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.bank = Some((frame.bank.clone(), buffer));
        }
        if all_models > self.actor_capacity {
            self.actor_capacity = all_models.next_power_of_two();
            (self.actors, self.actor_bind) = Self::actor_buffer(device, &self.actor_layout, self.actor_capacity);
        }
        if all_models > 0 {
            let mut places = vec![0u8; all_models * ACTOR_STRIDE];
            for (m, place) in frame.models.iter().chain(&frame.ui_models).zip(places.as_chunks_mut::<ACTOR_STRIDE>().0) {
                let tint = m.tint.map_or([0.0; 4], |[r, g, b]| [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0]);
                let data = ActorData { origin: m.origin.extend(m.turn as f32).to_array(), tint, extra: [m.nearer, 0.0, 0.0, 0.0], rot: m.rot.to_array(), pivot: m.shift.extend(if off_grid(m.rot, m.shift) { 1.0 } else { 0.0 }).to_array() };
                place[..size_of::<ActorData>()].copy_from_slice(bytemuck::bytes_of(&data));
            }
            queue.write_buffer(&self.actors, 0, &places);
        }
        let bank = self.bank.as_ref().map(|(_, buffer)| buffer).filter(|_| all_models > 0);
        // Each model draw: its place (the `first` of all places), and its
        // voxels in the bank.
        let draw_models = |pass: &mut wgpu::RenderPass, pipeline: &wgpu::RenderPipeline, models: &[ModelDraw], first: usize| {
            let Some(bank) = bank.filter(|_| !models.is_empty()) else { return };
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, bank.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            for (i, m) in models.iter().enumerate() {
                if m.voxels.is_empty() {
                    continue;
                }
                pass.set_bind_group(1, &self.actor_bind, &[((first + i) * ACTOR_STRIDE) as u32]);
                pass.draw_indexed(0..self.cube_index_count, 0, m.voxels.clone());
            }
        };

        if let Some(t) = &self.pass_times {
            t.labels.borrow_mut().clear();
        }
        let mut stats = Stats {
            instances: instance_count,
            models: all_models,
            model_cubes: frame.models.iter().chain(&frame.ui_models).map(|m| m.voxels.len()).sum(),
            ui_instances: frame.ui.len(),
            chunks: self.meshes.len(),
            vertices_total: self.meshes.values().map(|m| m.vertex_count).sum(),
            ..Stats::default()
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadows"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("shadows")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.shadow_bind, &[]);
            pass.set_pipeline(&self.chunk_shadow);
            for mesh in self.meshes.values() {
                if !on_screen(&frame.light_vp, mesh.min, mesh.max) {
                    continue;
                }
                stats.shadowed += 1;
                pass.set_vertex_buffer(0, mesh.vertices.slice(..mesh.vertex_bytes()));
                pass.set_index_buffer(mesh.indices.slice(..mesh.index_bytes()), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
            draw_models(&mut pass, &self.model_shadow, &frame.models, 0);
            // Fire, smoke and dust cast no shadow.
            if opaque > 0 {
                pass.set_pipeline(&self.inst_shadow);
                pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
                pass.set_vertex_buffer(1, self.instances.slice(..));
                pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.cube_index_count, 0, 0..opaque);
            }
        }

        {
            let [r, g, b] = frame.sky.map(f64::from);
            let t = &self.targets;
            let attachment = |view, clear| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[
                    attachment(&t.scene, wgpu::Color { r, g, b, a: 1.0 }),
                    attachment(&t.glow, wgpu::Color::TRANSPARENT),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("scene")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.main_bind, &[]);
            pass.set_pipeline(&self.chunk_main);
            for mesh in self.meshes.values() {
                if !on_screen(&frame.view_proj, mesh.min, mesh.max) {
                    continue;
                }
                stats.drawn += 1;
                stats.vertices += mesh.vertex_count;
                pass.set_vertex_buffer(0, mesh.vertices.slice(..mesh.vertex_bytes()));
                pass.set_index_buffer(mesh.indices.slice(..mesh.index_bytes()), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
            // The models before the cubes: where a cube shares a cell with a
            // model's voxel (or a model with the world), the one drawn first
            // keeps it, at the same depth.
            draw_models(&mut pass, &self.model_main, &frame.models, 0);
            if frame.outline[3] > 0.0 {
                draw_models(&mut pass, &self.model_outline, &frame.models, 0);
            }
            if instance_count > 0 {
                pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
                pass.set_vertex_buffer(1, self.instances.slice(..));
                pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
                if opaque > 0 {
                    pass.set_pipeline(&self.inst_main);
                    pass.draw_indexed(0..self.cube_index_count, 0, 0..opaque);
                }
                if (opaque as usize) < instance_count {
                    pass.set_pipeline(&self.inst_see_through);
                    pass.draw_indexed(0..self.cube_index_count, 0, opaque..instance_count as u32);
                }
            }
        }

        {
            // The UI: its own layer (cleared every frame) and the glow.
            let t = &self.targets;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.ui,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.glow,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("ui")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.ui_bind, &[]);
            draw_models(&mut pass, &self.model_main, &frame.ui_models, frame.models.len());
            if !frame.ui.is_empty() {
                pass.set_pipeline(&self.inst_main);
                pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
                pass.set_vertex_buffer(1, self.instances.slice(..));
                pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.cube_index_count, 0, instance_count as u32..total as u32);
            }
        }

        for (kind, source, dest) in &self.targets.passes {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.small[*dest],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("blur")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(match kind {
                PostPass::Down => &self.post_down,
                PostPass::Blur => &self.post_blur,
            });
            pass.set_bind_group(0, source, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("composite")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.post_composite);
            pass.set_bind_group(0, &self.targets.composite, &[]);
            pass.set_bind_group(1, &self.targets.blurred, &[]);
            pass.draw(0..3, 0..1);
        }
        if let Some(t) = &self.pass_times {
            let count = 2 * t.labels.borrow().len() as u32;
            if count > 0 {
                encoder.resolve_query_set(&t.queries, 0..count, &t.resolve, 0);
                encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, count as u64 * 8);
            }
        }
        self.stats = stats;
    }

    /// Times every pass from now on, if the device can (`Features::TIMESTAMP_QUERY`):
    /// `pass_times` then reads what the last frame took. False if it cannot.
    pub fn time_passes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return false;
        }
        let size = 2 * MAX_TIMED_PASSES as u64 * 8;
        self.pass_times = Some(PassTimes {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("pass times"),
                ty: wgpu::QueryType::Timestamp,
                count: 2 * MAX_TIMED_PASSES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pass times"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pass times readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            period: queue.get_timestamp_period(),
            labels: RefCell::new(Vec::new()),
        });
        true
    }

    /// What each kind of pass of the last frame took on the GPU, in
    /// milliseconds, in drawing order (passes of a kind summed). Call after
    /// the frame was submitted; waits for the GPU.
    pub fn pass_times(&self, device: &wgpu::Device) -> anyhow::Result<Vec<(&'static str, f64)>> {
        let Some(t) = &self.pass_times else { return Ok(Vec::new()) };
        let labels = t.labels.borrow();
        let slice = t.readback.slice(..2 * labels.len() as u64 * 8);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("mapping pass times"));
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let mut out: Vec<(&'static str, f64)> = Vec::new();
        {
            let data = slice.get_mapped_range()?;
            let ticks: &[u64] = bytemuck::cast_slice(&data);
            for (i, label) in labels.iter().enumerate() {
                let ms = ticks[2 * i + 1].saturating_sub(ticks[2 * i]) as f64 * t.period as f64 / 1e6;
                match out.iter_mut().find(|(l, _)| l == label) {
                    Some((_, sum)) => *sum += ms,
                    None => out.push((label, ms)),
                }
            }
        }
        t.readback.unmap();
        Ok(out)
    }
}

/// Whether a box may overlap the view: false only when all eight corners lie
/// outside the same clip plane. Works for perspective, where w varies.
pub(crate) fn on_screen(view_proj: &Mat4, min: Vec3, max: Vec3) -> bool {
    let clip: [Vec4; 8] = std::array::from_fn(|i| {
        let corner = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        *view_proj * corner.extend(1.0)
    });
    let outside = |f: fn(&Vec4) -> bool| clip.iter().all(f);
    !(outside(|c| c.x < -c.w)
        || outside(|c| c.x > c.w)
        || outside(|c| c.y < -c.w)
        || outside(|c| c.y > c.w)
        || outside(|c| c.z < 0.0)
        || outside(|c| c.z > c.w))
}

#[cfg(test)]
mod tests {
    /// Touching opaque unit cubes hide the faces between them, whether they
    /// sit close together (a bit per cell) or far apart (a hash set); a
    /// see-through cube neither hides nor gets hidden.
    #[test]
    fn touching_cubes_hide_the_faces_between_them() {
        use super::{Instance, hide_inner_faces};
        let cube = |x: f32, y: f32, z: f32| Instance { pos_size: [x + 0.5, y + 0.5, z + 0.5, 1.0], color: [255; 4], fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] };
        for far in [5.0, 1.0e6] {
            let mut v = vec![cube(0.0, 0.0, 0.0), cube(1.0, 0.0, 0.0), cube(0.0, 0.0, 1.0), cube(far, far, 0.0)];
            v.push(Instance { fx: [0, 128, 0, 0], ..cube(0.0, 1.0, 0.0) });
            hide_inner_faces(&mut v);
            // Bits in mesh::DIRS order: +x, -x, +y, -y, +z, -z.
            assert_eq!(v[0].fx[2], 0b01_0001, "the first cube's +x and +z faces (far cube at {far})");
            assert_eq!(v[1].fx[2], 0b00_0010, "-x");
            assert_eq!(v[2].fx[2], 0b10_0000, "-z");
            assert_eq!(v[3].fx[2], 0, "a cube on its own");
            assert_eq!(v[4].fx[2], 0, "a see-through cube");
        }
    }

    /// See-through cubes join where they lie alike in neighbouring cells:
    /// the faces they share go, a raised, turned or opaque neighbour keeps
    /// them, and a patch keeps only its outside.
    #[test]
    fn see_through_cubes_join_where_they_touch() {
        use super::{Instance, join_see_through};
        let cube = |x: f32, y: f32, z: f32| Instance { pos_size: [x + 0.5, y + 0.5, z + 0.5, 1.0], color: [50, 130, 200, 140], fx: [0, 140, 0, 0], shade: [0; 4], turn: [0; 4] };
        // Bits in mesh::DIRS order: +x, -x, +y, -y, +z, -z.
        let mut v = vec![cube(0.0, 0.0, 0.0), Instance { color: [255, 255, 255, 140], ..cube(1.0, 0.0, 0.0) }];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0b01, 0b10), "the shared faces, different colours");
        let mut v = vec![cube(0.0, 0.0, 0.0), Instance { fx: [0, 255, 0, 0], ..cube(1.0, 0.0, 0.0) }];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0, 0), "beside an opaque cube");
        let mut v = vec![cube(0.0, 0.0, 0.0), cube(1.0, 0.0, 0.3)];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0, 0), "a neighbour raised by 0.3");
        let mut v = vec![cube(0.0, 0.0, 0.0), Instance { turn: [10, 0, 0, 100], ..cube(1.0, 0.0, 0.0) }];
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!((v[0].fx[2], v[1].fx[2]), (0, 0), "a freely turned neighbour");
        let mut v = vec![cube(0.0, 0.0, 0.0), cube(1.0, 0.0, 0.0)];
        v[0].fx[2] = 0b10_0000;
        join_see_through(&mut v, glam::Vec3::Z);
        assert_eq!(v[0].fx[2], 0b10_0001, "bits set before stay");
        // A skin a layer, across the body where it is thinnest. Water two
        // deep and three wide each way, from above: the top between its
        // layers stays, every side between its cubes goes.
        let up = glam::Vec3::new(0.0, -0.7, 0.7);
        let slab = |v: &mut Vec<Instance>| {
            for z in 0..2 {
                for y in 0..3 {
                    for x in 0..3 {
                        v.push(cube(x as f32, y as f32, z as f32));
                    }
                }
            }
        };
        let mut v = Vec::new();
        slab(&mut v);
        join_see_through(&mut v, up);
        assert_eq!(v[4].fx[2], 0b00_1111, "the lower layer's middle: its sides go, its top stays (nothing below it)");
        assert_eq!(v[9 + 4].fx[2], 0b10_1111, "the upper layer's middle: its sides and its bottom go");
        // A pane of glass standing, one cube thick: one skin, no layers,
        // though the view is from above.
        let mut v: Vec<Instance> = (0..3).flat_map(|z| (0..3).map(move |x| cube(x as f32, 0.0, z as f32))).collect();
        join_see_through(&mut v, up);
        assert_eq!(v[4].fx[2], 0b11_0011, "a standing pane's middle: every face shared goes");
        // A pane two thick, six wide and high: layers across it, the far
        // one's near side stays.
        let mut v: Vec<Instance> = (0..2).flat_map(|y| (0..6).flat_map(move |z| (0..6).map(move |x| cube(x as f32, y as f32, z as f32)))).collect();
        join_see_through(&mut v, up);
        assert_eq!((v[7].fx[2], v[36 + 7].fx[2]), (0b11_0111, 0b11_0011), "two thick: the near pane loses its far side, the far one keeps its near side");
        // A block as wide as it is deep and high: across the view. From
        // above the tops between its layers stay, from the south the near sides.
        let block = || -> Vec<Instance> { (0..8).map(|n| cube((n & 1) as f32, (n >> 1 & 1) as f32, (n >> 2) as f32)).collect() };
        let mut v = block();
        join_see_through(&mut v, up);
        assert_eq!((v[0].fx[2], v[4].fx[2]), (0b00_0101, 0b10_0101), "a block from above");
        let mut v = block();
        join_see_through(&mut v, glam::Vec3::new(0.0, -1.0, 0.2));
        assert_eq!((v[0].fx[2], v[2].fx[2]), (0b01_0101, 0b01_0001), "a block from the south");
        let mut v: Vec<Instance> = (0..3).flat_map(|y| (0..3).map(move |x| cube(x as f32, y as f32, 0.0))).collect();
        join_see_through(&mut v, glam::Vec3::Z);
        for (n, i) in v.iter().enumerate() {
            let (x, y) = (n % 3, n / 3);
            let side = (x < 2) as u8 | ((x > 0) as u8) << 1 | ((y < 2) as u8) << 2 | ((y > 0) as u8) << 3;
            assert_eq!(i.fx[2], side, "cube {x},{y} of a patch: its inner sides only, tops and bottoms stay");
        }
    }

    /// The shaders parse and validate: wgpu only finds out when it runs them.
    #[test]
    fn the_shaders_are_valid() {
        use wgpu::naga;
        for (name, src) in [("shader.wgsl", include_str!("shader.wgsl")), ("post.wgsl", include_str!("post.wgsl"))] {
            let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }
}

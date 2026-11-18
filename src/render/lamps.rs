//! Lamps: lights walls stop, spots, a falloff of their own, and more than
//! `FORWARD_LIGHTS` lights a picture. A picture with a light walls stop
//! (`Light::shadows`, its level's picture given: `FrameData::lamp_world`)
//! or more lights than the scene's shader takes draws its lights in a pass
//! of their own (`lamps.wgsl`): the opaque scene first without them
//! (`fs_main_lamps`, each pixel's surface written beside its colour), then
//! the screen in tiles, each drawn with the lights whose box reaches it,
//! their light added onto the pixels there (each pixel's surface read once
//! a tile), then the see-through cubes (lit as before by the nearest
//! `FORWARD_LIGHTS`, through walls). Every other picture is drawn as before,
//! by the scene's shader alone: a game without lamps pays nothing.
//!
//! Walls: the level's solid voxels and its pieces' (`volume`) on the GPU,
//! made when a picture first asks and brought up to date where the world
//! changes; each voxel face asks once whether a lamp reaches it, and
//! remembers (`seen`, room for twice the picture's pixels and its fog's
//! beams' points, twice as large when found full, said once when it cannot
//! grow). A GPU without storage written from a fragment shader draws its
//! lamps through walls, said once; lamps reaching too far apart for the
//! volume too.
//!
//! A picture with fog (`render::fog`) is drawn by this pass too: its beams
//! are drawn as its lights are, by its tiles and walls.
//!
//! Only the picture: nothing here is the game's (no fingerprint, state
//! hash, replay or save sees it).

use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Vec4};

use super::limits::FullWatch;
use super::pipelines::{Draws, ScenePipelines, chunk_buffers, face_buffers, instance_buffers, scene_pipeline, texture, uniform_entry};
use super::shader::{LAMPS, LightData, SHADER, plain_shader, without_walls};
use super::thin::Thin;
use super::volume::{self, LEVELS, Reach, Volume};
use super::{DEPTH_FORMAT, FORWARD_LIGHTS, FrameData, HDR_FORMAT, Light, MOST_LIGHTS, Renderer};

/// How many answers the lamps' memory holds to begin with for a picture
/// `w` by `h` pixels (two words each, the count of those that found no room
/// after them), with fog its beams' target `beams` (`render::fog`, half or
/// a quarter the picture each way) or none: twice its pixels (a face lit by
/// several lamps takes an entry for each) and `BEAM_POINTS` a pixel of the
/// beams' target, a power of two, from 2^16 to 2^24 (128 MB). The beams'
/// points and the faces share it: a point and the face in front of its
/// voxel ask the same (the floor under a beam). Made twice as large when
/// found full (`Renderer::too_small`).
pub(super) fn faces_for(w: u32, h: u32, beams: Option<(u32, u32)>) -> u64 {
    let points = beams.map_or(0, |(bw, bh)| BEAM_POINTS * u64::from(bw) * u64::from(bh));
    (2 * u64::from(w) * u64::from(h) + points).next_power_of_two().clamp(1 << 16, 1 << 24)
}

/// The beams' points' room a pixel of their target: a ray takes a point
/// every 4 voxels along the camera's way within a lamp's reach (8 or 16
/// for a wide one, up to 52 a lamp), the lamps along a ray adding up, and
/// where the rays lie closer than the voxels (FOG HIGH) neighbours share
/// them. What they take comes by the voxels along the beams more than by
/// the pixels: a hall of 14 lamps (radius 40 to 120) in fog seen whole,
/// 400 voxels across, took some 4 million at 960 by 540 and at 1920 by
/// 1080, at FOG HIGH and LOW alike, where twice the pixels gave the faces
/// and the points 4 million together at 1920 by 1080 and 2 at 1280 by 720
/// (2026-11-18): the points took it all, the faces found no room and
/// walked every picture.
const BEAM_POINTS: u64 = 16;

/// The largest the lamps' memory may be on this GPU, answers (one buffer
/// bound whole, two words each and the count).
fn most_faces(device: &wgpu::Device) -> u64 {
    let limits = device.limits();
    let bytes = limits.max_storage_buffer_binding_size.min(limits.max_buffer_size);
    1 << ((bytes - 4) / 8).ilog2()
}

/// Where the count lies in a memory for `faces` faces.
fn full_at(faces: u64) -> u64 {
    faces * 2 * 4
}

/// What each pixel's surface is (`fs_main_lamps`).
const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Uint;

/// The lamps' asking's target (nothing is written into it).
const ASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// The lamps' pass's numbers (`lamps.wgsl`'s `Lamps`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LampGlobals {
    pub(super) inv_view_proj: [[f32; 4]; 4],
    pub(super) dims: [i32; 4],
    pub(super) origin: [i32; 4],
    pub(super) info: [u32; 4],
    /// The moving things' volume (`movers`): its texels each way and copies
    /// (0: none), its low corner.
    pub(super) movers_dims: [i32; 4],
    pub(super) movers_origin: [i32; 4],
}

/// The screen's tiles each way, pixels: each drawn with the lights that
/// reach it.
const TILE: u32 = 32;

/// The lamps' asking (`lamps.wgsl`'s `fs_lamp_ask`): before the lamps'
/// pass, for lamps new to the faces' memory (moved since the last picture),
/// a pixel every `ASK` each way asks for its face. All the pixels of a
/// face asking at once in the pass would each walk (an entry being written
/// is no answer yet): two moving headlights cost the Radeon 610M 7 ms so.
/// The answers are the walks' either way: the picture is the same.
pub(super) const ASK: u32 = 4;

/// The most lights a tile is drawn with at once (two a word in
/// `TileInstance::lights`); a tile with more is drawn again for the rest.
const LIGHTS_A_TILE: usize = 32;

/// A tile of the lamps' pass: its rectangle (low and high corner, -1 to
/// 1), how many lights it is drawn with, and their rows in the lights'
/// texture, two a word.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct TileInstance {
    pub(super) rect: [f32; 4],
    pub(super) count: [u32; 4],
    pub(super) lights: [u32; LIGHTS_A_TILE / 2],
}

const TILE_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Uint32x4, 2 => Uint32x4, 3 => Uint32x4, 4 => Uint32x4, 5 => Uint32x4];

/// The lamps' pass's vertex buffer: a tile an instance.
pub(super) fn lamp_buffers() -> [Option<wgpu::VertexBufferLayout<'static>>; 1] {
    [Some(wgpu::VertexBufferLayout { array_stride: size_of::<TileInstance>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &TILE_ATTRS })]
}

/// The tiles of a picture `size` pixels big that the lights at the
/// screen's rectangles `rects` (by their rows) reach, each with its
/// lights, in rows of tiles from the top.
pub(super) fn tiles(size: (u32, u32), rects: &[(u16, [f32; 4])]) -> Vec<TileInstance> {
    let (w, h) = size;
    let (across, down) = (w.div_ceil(TILE), h.div_ceil(TILE));
    let mut lists: Vec<Vec<u16>> = vec![Vec::new(); (across * down) as usize];
    for &(row, [x0, y0, x1, y1]) in rects {
        // From -1..1 (y up) to tiles (y down).
        let tx = |x: f32| ((x + 1.0) * 0.5 * w as f32 / TILE as f32).clamp(0.0, across as f32);
        let ty = |y: f32| ((1.0 - y) * 0.5 * h as f32 / TILE as f32).clamp(0.0, down as f32);
        let (a, b) = (tx(x0).floor() as u32, tx(x1).ceil() as u32);
        let (c, d) = (ty(y1).floor() as u32, ty(y0).ceil() as u32);
        for y in c..d {
            for x in a..b {
                lists[(x + y * across) as usize].push(row);
            }
        }
    }
    let mut out = Vec::new();
    for (i, list) in lists.iter().enumerate() {
        let (x, y) = (i as u32 % across, i as u32 / across);
        let ndc = |px: u32, of: u32| px.min(of) as f32 / of as f32 * 2.0 - 1.0;
        let rect = [ndc(x * TILE, w), -ndc((y + 1) * TILE, h), ndc((x + 1) * TILE, w), -ndc(y * TILE, h)];
        for part in list.chunks(LIGHTS_A_TILE) {
            let mut lights = [0u32; LIGHTS_A_TILE / 2];
            for (k, row) in part.iter().enumerate() {
                lights[k / 2] |= u32::from(*row) << (16 * (k % 2));
            }
            out.push(TileInstance { rect, count: [part.len() as u32, 0, 0, 0], lights });
        }
    }
    out
}

/// The pass and what it reads, made when first asked.
pub(super) struct Lamps {
    /// With walls: made the first time a picture has a lamp walls stop.
    walls: Walls,
    /// Without (more lights than the scene's shader takes, none stopped).
    plain: Option<LampPass>,
    /// The scene's pipelines writing the surfaces: of the plain shader,
    /// and of the one with the faces' memory of their shadow.
    scenes: [Option<LampScenes>; 2],
    /// The depth and the surfaces, the picture's size.
    targets: Option<LampTargets>,
    /// The level's solid voxels, here and on the GPU.
    volume: Option<Held>,
    /// The volume's state: another number at every change of it (the
    /// faces' answers from before are walked again).
    state: u32,
    globals: wgpu::Buffer,
    /// The tiles of the screen with their lights, and the lights (a row
    /// each, room for `rows`).
    instances: wgpu::Buffer,
    capacity: usize,
    lights: (wgpu::Texture, wgpu::TextureView),
    rows: u32,
    /// The faces' memory of what stops lamps (`faces_for` the picture and
    /// its fog, or `grown`) and how many it holds, made with the targets,
    /// anew when a picture asks for more.
    seen: Option<(u64, wgpu::Buffer)>,
    /// How many answers pictures found the memory needs (a memory found
    /// full doubled, `Renderer::too_small`): the least it is made, kept
    /// while the window changes its size.
    grown: u64,
    /// What the pass reads: made anew with new targets or a new volume.
    bind: Option<wgpu::BindGroup>,
    /// Whether `bind` is the one with walls.
    bound_walls: bool,
    /// How long the volume took to make last, ms (`shot` says it).
    pub(super) made_ms: f64,
    /// The moving things' volume on the GPU, for lamps whose shadows are
    /// "all" (`movers`), and a texture standing for it when there is none.
    movers: Option<HeldMovers>,
    no_movers: wgpu::TextureView,
    /// The boxes round the moving things (`Movers::boxes`, room for
    /// `MOST_BOXES`).
    movers_boxes: wgpu::Buffer,
    /// The lamps' lists of boxes (`Movers::lists`, room for `MOST_LISTED`)
    /// and the picture's lights' words for them (by their place in the
    /// frame's lights; the fog's beams take them too).
    movers_lists: wgpu::Buffer,
    pub(super) movers_words: Vec<Option<f32>>,
    told_movers: bool,
    /// The lamps walls stopped last picture, by their place's key: one not
    /// among them is new to the faces' memory, asked for first (`ASK`).
    known: std::collections::BTreeSet<[u32; 2]>,
    /// The asking's tiles (at a pixel every `ASK`), how many this picture,
    /// and its target (nothing is drawn into it).
    ask_instances: wgpu::Buffer,
    ask_capacity: usize,
    ask_count: u32,
    /// How many lamps were asked for first this picture (`Stats::asked`).
    pub(super) asked: usize,
    ask_target: Option<((u32, u32), wgpu::TextureView)>,
    told_big: bool,
    told_full: bool,
}

enum Walls {
    Unasked,
    Ready { pass: LampPass, full: FullWatch },
    Lacking,
}

struct LampPass {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    /// With walls: the lamps' asking (`ASK`).
    ask: Option<wgpu::RenderPipeline>,
}

/// The scene's pipelines in a picture of the lamps' pass: the opaque ones
/// writing the surfaces (and the see-through cubes drawn faint behind
/// everything first), the models' outline.
pub(super) struct LampScenes {
    pub(super) scene: ScenePipelines,
    pub(super) model_outline: wgpu::RenderPipeline,
    pub(super) face_outline: wgpu::RenderPipeline,
}

pub(super) struct LampTargets {
    size: (u32, u32),
    pub(super) depth: wgpu::TextureView,
    pub(super) surface: wgpu::TextureView,
}

/// The moving things' volume on the GPU: what it holds (`Movers::key`),
/// where, and its texture.
struct HeldMovers {
    reach: Reach,
    key: u64,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Whether it holds anything this picture (else the shader is told none).
    shown: bool,
}

/// A 3D texture of `texels` (R8Uint) with `levels` copies, for the moving
/// things' volume.
fn movers_texture(device: &wgpu::Device, texels: glam::IVec3, levels: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("the moving things' voxels"),
        size: wgpu::Extent3d { width: texels.x as u32, height: texels.y as u32, depth_or_array_layers: texels.z as u32 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::R8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

struct Held {
    volume: Volume,
    view: wgpu::TextureView,
    texture: wgpu::Texture,
    /// Its thin things (`render::thin`), found the first time a picture of
    /// fog asks, brought up to date with it.
    thin: Option<Thin>,
}

/// How a picture draws its lights: by the lamps' pass, walls stopping
/// lamps or not (`Renderer::lamps_of`).
#[derive(Clone, Copy)]
pub(super) struct LampFrame {
    pub(super) walls: bool,
    /// Which scene pipelines: 0 the plain shader's, 1 with the memory.
    pub(super) shader: usize,
}

impl Lamps {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let globals = device.create_buffer(&wgpu::BufferDescriptor { label: Some("lamps"), size: size_of::<LampGlobals>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        Self {
            walls: Walls::Unasked,
            plain: None,
            scenes: [None, None],
            targets: None,
            volume: None,
            state: 1,
            globals,
            instances: instance_buffer(device, 64),
            capacity: 64,
            lights: lights_texture(device, 64),
            rows: 64,
            seen: None,
            grown: 0,
            bind: None,
            bound_walls: false,
            made_ms: 0.0,
            movers: None,
            no_movers: movers_texture(device, glam::IVec3::ONE, 1).1,
            movers_boxes: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("the moving things' boxes"),
                size: (super::movers::MOST_BOXES * 32) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            movers_lists: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("the lamps' lists of boxes"),
                size: (super::movers::MOST_LISTED * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            movers_words: Vec::new(),
            told_movers: false,
            known: std::collections::BTreeSet::new(),
            ask_instances: instance_buffer(device, 16),
            ask_capacity: 16,
            ask_count: 0,
            asked: 0,
            ask_target: None,
            told_big: false,
            told_full: false,
        }
    }

    /// The thin things of the level's volume (`render::thin`), found the
    /// first time asked; none without a volume.
    pub(super) fn thin(&mut self) -> Option<&Thin> {
        let held = self.volume.as_mut()?;
        Some(held.thin.get_or_insert_with(|| Thin::of(&held.volume)))
    }

    /// What the level's volume takes on the GPU, bytes (0: none).
    pub(super) fn volume_bytes(&self) -> u64 {
        self.volume.as_ref().map_or(0, |h| h.volume.reach.bytes())
    }

    /// The picture's size changed: the targets are made anew.
    pub(super) fn resized(&mut self) {
        self.targets = None;
        self.seen = None;
        self.bind = None;
    }

    pub(super) fn targets(&self) -> &LampTargets {
        self.targets.as_ref().expect("made by lamps_of")
    }

    pub(super) fn scenes(&self, shader: usize) -> &LampScenes {
        self.scenes[shader].as_ref().expect("made by lamps_of")
    }

    /// What the pass reads, laid out with walls or without (the fog's
    /// beams read it too, `render::fog`): made by `lamps_of`.
    pub(super) fn layout(&self, walls: bool) -> &wgpu::BindGroupLayout {
        match (&self.walls, walls) {
            (Walls::Ready { pass, .. }, true) => &pass.layout,
            _ => &self.plain.as_ref().expect("made by lamps_of").layout,
        }
    }

    /// What the pass reads this picture: made by `write_lamps`.
    pub(super) fn bind(&self) -> &wgpu::BindGroup {
        self.bind.as_ref().expect("made by write_lamps")
    }
}

pub(super) fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("lamps' tiles"), size: (capacity * size_of::<TileInstance>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

/// The lights' texture, room for `rows` lights (a row each, `LightData`'s
/// four parts across).
pub(super) fn lights_texture(device: &wgpu::Device, rows: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lamps' lights"),
        size: wgpu::Extent3d { width: 4, height: rows, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// The lamps' pass's bind group layout: its numbers, the depth, the
/// surfaces, the lights, and with `walls` the volume and the faces'
/// memory.
fn pass_layout(device: &wgpu::Device, walls: bool) -> wgpu::BindGroupLayout {
    let tex = |binding, sample_type, view_dimension| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type, view_dimension, multisampled: false }, count: None };
    let mut entries = vec![
        uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
        tex(1, wgpu::TextureSampleType::Depth, wgpu::TextureViewDimension::D2),
        tex(2, wgpu::TextureSampleType::Uint, wgpu::TextureViewDimension::D2),
        tex(3, wgpu::TextureSampleType::Float { filterable: false }, wgpu::TextureViewDimension::D2),
    ];
    if walls {
        entries.push(tex(4, wgpu::TextureSampleType::Uint, wgpu::TextureViewDimension::D3));
        entries.push(wgpu::BindGroupLayoutEntry { binding: 5, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None });
        entries.push(tex(6, wgpu::TextureSampleType::Uint, wgpu::TextureViewDimension::D3));
        for binding in [7, 8] {
            entries.push(wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None });
        }
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(if walls { "lamps, walls" } else { "lamps" }), entries: &entries })
}

/// The lamps' pass of the shader `src`: each light's rectangle, its light
/// added onto the scene.
fn lamp_pass(device: &wgpu::Device, src: &str, walls: bool) -> LampPass {
    let layout = pass_layout(device, walls);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("lamps"), source: wgpu::ShaderSource::Wgsl(src.into()) });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("lamps"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let add = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("lamps"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_lamp"), compilation_options: Default::default(), buffers: &lamp_buffers() },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs_lamp"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(add), write_mask: wgpu::ColorWrites::ALL })] }),
        multiview_mask: None,
        cache: None,
    });
    // The asking draws nothing: its target is only there to draw into.
    let ask = walls.then(|| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lamps' asking"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_lamp"), compilation_options: Default::default(), buffers: &lamp_buffers() },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs_lamp_ask"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: ASK_FORMAT, blend: None, write_mask: wgpu::ColorWrites::empty() })] }),
            multiview_mask: None,
            cache: None,
        })
    });
    LampPass { layout, pipeline, ask }
}

/// The scene's pipelines of the shader `src` (its main pass laid out as
/// `main`) for a picture of the lamps' pass.
fn lamp_scenes(device: &wgpu::Device, src: &str, main: &wgpu::BindGroupLayout, actor: &wgpu::BindGroupLayout) -> LampScenes {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("voxcade shader, lamps"), source: wgpu::ShaderSource::Wgsl(src.into()) });
    let layout = |label: &str, groups: &[Option<&wgpu::BindGroupLayout>]| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: groups, immediate_size: 0 });
    let (main_layout, models) = (layout("main pass, lamps", &[Some(main)]), layout("models, lamps", &[Some(main), Some(actor)]));
    let hdr = |blend| Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL });
    let surface = |mask| Some(wgpu::ColorTargetState { format: SURFACE_FORMAT, blend: None, write_mask: mask });
    let opaque = [hdr(None), hdr(None), surface(wgpu::ColorWrites::ALL)];
    let faint = [hdr(Some(wgpu::BlendState::ALPHA_BLENDING)), hdr(Some(wgpu::BlendState::ALPHA_BLENDING)), surface(wgpu::ColorWrites::empty())];
    let lit = |targets| Draws::Scene { fs: "fs_main_lamps", cull: wgpu::Face::Back, targets };
    let outline = || Draws::Scene { fs: "fs_outline_lamps", cull: wgpu::Face::Front, targets: &opaque };
    LampScenes {
        scene: ScenePipelines {
            chunk: scene_pipeline(device, &module, "chunks, lamps", &main_layout, "vs_chunk", &chunk_buffers(), lit(&opaque), true),
            inst: scene_pipeline(device, &module, "instances, lamps", &main_layout, "vs_inst", &instance_buffers(), lit(&opaque), true),
            see_through: scene_pipeline(device, &module, "behind, lamps", &main_layout, "vs_inst", &instance_buffers(), Draws::Scene { fs: "fs_main", cull: wgpu::Face::Back, targets: &faint }, false),
            model: scene_pipeline(device, &module, "models, lamps", &models, "vs_model", &instance_buffers(), lit(&opaque), true),
            faces: scene_pipeline(device, &module, "model faces, lamps", &models, "vs_face", &face_buffers(), lit(&opaque), true),
        },
        model_outline: scene_pipeline(device, &module, "model outlines, lamps", &models, "vs_model_outline", &instance_buffers(), outline(), true),
        face_outline: scene_pipeline(device, &module, "model face outlines, lamps", &models, "vs_face_outline", &face_buffers(), outline(), true),
    }
}

/// The rectangle of the screen a light can reach, seen through
/// `view_proj` (its low and high corner, -1 to 1): its box's corners'; the
/// whole screen when one lies behind the eye; None when off the screen.
pub(super) fn rect_of(view_proj: &Mat4, l: &Light) -> Option<[f32; 4]> {
    let r = glam::Vec3::splat(l.radius);
    let (mut lo, mut hi) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
    for corner in crate::camera::box_corners(l.pos - r, l.pos + r) {
        let c: Vec4 = *view_proj * corner.extend(1.0);
        if c.w <= 1e-3 {
            return Some([-1.0, -1.0, 1.0, 1.0]);
        }
        let p = c.truncate().truncate() / c.w;
        (lo, hi) = (lo.min(p), hi.max(p));
    }
    let (lo, hi) = (lo.max(glam::Vec2::NEG_ONE), hi.min(glam::Vec2::ONE));
    (lo.x < hi.x && lo.y < hi.y).then_some([lo.x, lo.y, hi.x, hi.y])
}

impl Renderer {
    /// How this picture draws its lights: None by the scene's own shader
    /// (as every picture did before 0.7: at most `FORWARD_LIGHTS`, none
    /// walls stop), else by the lamps' pass, everything it needs made (the
    /// first time a picture asks) and the level's volume brought up to
    /// date. Walls stop its lamps where the GPU can and the level fits;
    /// else said once, they shine through.
    pub(super) fn lamps_of(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData, remember: bool) -> Option<LampFrame> {
        let stopped = frame.lamp_world.is_some() && frame.lights.iter().any(|l| l.stopped());
        // (A picture with fog too: its beams are drawn as this pass draws,
        // after the scene it keeps the depth of.)
        if !stopped && frame.lights.len() <= FORWARD_LIGHTS && frame.fog.is_none() {
            return None;
        }
        let walls = stopped && self.walling(device) && self.volume_for(device, queue, frame);
        let shader = usize::from(remember);
        if self.lamps.scenes[shader].is_none() {
            let src = if remember { SHADER.to_string() } else { plain_shader(&SHADER) };
            let main = match &self.memory {
                super::pipelines::Memory::Ready { layout, .. } if remember => layout,
                _ => &self.main_layout,
            };
            self.lamps.scenes[shader] = Some(lamp_scenes(device, &src, main, &self.actor_layout));
        }
        if !walls && self.lamps.plain.is_none() {
            self.lamps.plain = Some(lamp_pass(device, &without_walls(&LAMPS), false));
        }
        let (_, _, w, h) = self.picture();
        if self.lamps.targets.as_ref().is_none_or(|t| t.size != (w, h)) {
            self.lamps.targets = Some(LampTargets { size: (w, h), depth: texture(device, "lamps' depth", w, h, DEPTH_FORMAT, true), surface: texture(device, "surfaces", w, h, SURFACE_FORMAT, true) });
            self.lamps.seen = None;
            self.lamps.bind = None;
        }
        // As large as the picture and its fog's beams ask, or as pictures
        // found it needs; never smaller while the picture's size stays (a
        // game's fog coming and going would make it anew each time).
        let beams = frame.fog.as_ref().map(|f| (w.div_ceil(f.scale.max(1)), h.div_ceil(f.scale.max(1))));
        let faces = faces_for(w, h, beams).max(self.lamps.grown).min(most_faces(device));
        if walls && self.lamps.seen.as_ref().is_none_or(|(held, _)| *held < faces) {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("lamps' memory"), size: full_at(faces) + 4, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            self.lamps.seen = Some((faces, buffer));
            self.lamps.bind = None;
        }
        Some(LampFrame { walls, shader })
    }

    /// Whether walls can stop lamps on this GPU: the pass with walls, made
    /// the first time (said once where it cannot be).
    fn walling(&mut self, device: &wgpu::Device) -> bool {
        if matches!(self.lamps.walls, Walls::Unasked) {
            self.lamps.walls = make_walls(device).unwrap_or_else(|why| {
                self.said.push(format!("lamps: this GPU cannot stop lamps at walls ({why}): they shine through"));
                Walls::Lacking
            });
            self.lamps.bind = None;
        }
        matches!(self.lamps.walls, Walls::Ready { .. })
    }

    /// The level's volume for the picture's world and pieces, round what
    /// its lamps reach: made the first time (and for another level, when
    /// the pieces changed or a lamp reaches past it), else the chunks
    /// changed since brought up to date, on the GPU too. False (said once)
    /// when the lamps reach too far apart for it.
    fn volume_for(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData) -> bool {
        let Some(world) = frame.lamp_world.as_ref() else { return false };
        let lit: Vec<(glam::Vec3, f32)> = frame.lights.iter().filter(|l| l.stopped()).map(|l| (l.pos, l.radius)).collect();
        let key = volume::pieces_key(&frame.bank, &frame.lamp_pieces);
        let same = self.lamps.volume.as_ref().is_some_and(|h| h.volume.reach.holds(&lit, world.size) && h.volume.pieces_key == key && h.volume.same_level(world));
        let reach = if same { self.lamps.volume.as_ref().expect("held").volume.reach } else { Reach::of(&lit, world.size) };
        if let Err(why) = reach.fits(device.limits().max_texture_dimension_3d) {
            if !self.lamps.told_big {
                self.lamps.told_big = true;
                self.said.push(format!("lamps: {why}: they shine through walls"));
            }
            self.lamps.volume = None;
            return false;
        }
        let texels = reach.texels;
        let lamps = &mut self.lamps;
        if same {
            let held = lamps.volume.as_mut().expect("held");
            let changed = held.volume.update(world);
            if !changed.is_empty() {
                if let Some(thin) = &mut held.thin {
                    thin.update(&held.volume, &changed);
                }
                for (lo, size) in changed {
                    for m in 0..LEVELS {
                        write(queue, &held.texture, m, lo >> m as i32, size >> m as i32, &held.volume.bytes_of(m, lo, size));
                    }
                }
                lamps.state = next_state(lamps.state);
            }
            return true;
        }
        let started = Instant::now();
        lamps.volume = None;
        let volume = Volume::new(world, reach, &frame.bank, &frame.lamp_pieces, key);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("the level's solid voxels"),
            size: wgpu::Extent3d { width: texels.x as u32, height: texels.y as u32, depth_or_array_layers: texels.z as u32 },
            mip_level_count: LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for m in 0..LEVELS {
            write(queue, &texture, m, IVec3::ZERO, texels >> m as i32, &volume.copies[m as usize]);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        lamps.volume = Some(Held { volume, view, texture, thin: None });
        lamps.state = next_state(lamps.state);
        lamps.bind = None;
        lamps.made_ms = started.elapsed().as_secs_f64() * 1000.0;
        true
    }

    /// The lights of the lamps' pass, its tiles and its numbers for this
    /// picture; how many tiles it draws.
    pub(super) fn write_lamps(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData, lf: LampFrame) -> u32 {
        let shown: Vec<(usize, &Light, [f32; 4])> = frame.lights.iter().take(MOST_LIGHTS).enumerate().filter_map(|(i, l)| rect_of(&frame.view_proj, l).map(|rect| (i, l, rect))).collect();
        // The moving things, for the lamps whose shadows are "all": made
        // anew every picture, sent when they changed; each such lamp's
        // boxes listed (`Movers::lists`), its word in its row.
        let moving = lf.walls && shown.iter().any(|(_, l, _)| l.stopped_by_movers());
        let built = if moving { super::movers::Movers::of(&frame.bank, &frame.lamp_movers, opaque_cubes(frame), device.limits().max_texture_dimension_3d) } else { None };
        let lamps = &mut self.lamps;
        if let Some(held) = &mut lamps.movers {
            held.shown = false;
        }
        lamps.movers_words.clear();
        match built {
            Some(Ok(m)) => {
                if lamps.movers.as_ref().is_none_or(|h| h.reach != m.reach) {
                    let (texture, view) = movers_texture(device, m.reach.texels, LEVELS);
                    lamps.movers = Some(HeldMovers { reach: m.reach, key: !m.key, texture, view, shown: false });
                    lamps.bind = None;
                }
                let held = lamps.movers.as_mut().expect("made above");
                if held.key != m.key {
                    for k in 0..LEVELS {
                        write(queue, &held.texture, k, IVec3::ZERO, m.reach.texels >> k as i32, &m.copies[k as usize]);
                    }
                    if !m.boxes.is_empty() {
                        queue.write_buffer(&lamps.movers_boxes, 0, bytemuck::cast_slice(&m.boxes));
                    }
                    held.key = m.key;
                }
                held.shown = true;
                let (list, words) = m.lists(&frame.lights[..frame.lights.len().min(MOST_LIGHTS)]);
                if !list.is_empty() {
                    queue.write_buffer(&lamps.movers_lists, 0, bytemuck::cast_slice(&list));
                }
                lamps.movers_words = words;
            }
            Some(Err(why)) if !lamps.told_movers => {
                lamps.told_movers = true;
                self.said.push(format!("lamps: {why}"));
            }
            _ => {}
        }
        let rows: Vec<LightData> = shown.iter().map(|(i, l, _)| lamps.data_of(*i, l, lf.walls)).collect();
        let (_, _, w, h) = self.picture();
        let tiles = tiles((w, h), &shown.iter().enumerate().map(|(row, (_, _, rect))| (row as u16, *rect)).collect::<Vec<_>>());
        let lamps = &mut self.lamps;
        if tiles.len() > lamps.capacity {
            lamps.capacity = tiles.len().next_power_of_two();
            lamps.instances = instance_buffer(device, lamps.capacity);
        }
        if rows.len() as u32 > lamps.rows {
            lamps.rows = (rows.len() as u32).next_power_of_two();
            lamps.lights = lights_texture(device, lamps.rows);
            lamps.bind = None;
        }
        if !tiles.is_empty() {
            queue.write_buffer(&lamps.instances, 0, bytemuck::cast_slice(&tiles));
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &lamps.lights.0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(&rows),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size_of::<LightData>() as u32), rows_per_image: Some(rows.len() as u32) },
                wgpu::Extent3d { width: 4, height: rows.len() as u32, depth_or_array_layers: 1 },
            );
        }
        let (movers_dims, movers_origin) = match lamps.movers.as_ref().filter(|h| h.shown) {
            Some(h) => ([h.reach.texels.x, h.reach.texels.y, h.reach.texels.z, LEVELS as i32], [h.reach.origin.x, h.reach.origin.y, h.reach.origin.z, 0]),
            None => ([0; 4], [0; 4]),
        };
        let (dims, origin) = lamps.volume.as_ref().map_or(([0; 4], [0; 4]), |h| {
            let r = h.volume.reach;
            ([r.texels.x, r.texels.y, r.texels.z, LEVELS as i32], [r.origin.x, r.origin.y, r.origin.z, 0])
        });
        // The lamps walls stop that are new to the faces' memory: asked for
        // first, at a pixel every ASK.
        let mut known = std::collections::BTreeSet::new();
        let mut fresh = Vec::new();
        for (row, ((_, _, rect), data)) in shown.iter().zip(&rows).enumerate() {
            if lf.walls && data.edge[1] > 0.5 {
                let key = [data.edge[2] as u32, data.edge[3] as u32];
                if !lamps.known.contains(&key) {
                    fresh.push((row as u16, *rect));
                }
                known.insert(key);
            }
        }
        lamps.known = known;
        lamps.asked = fresh.len();
        let asked = self::tiles((w.div_ceil(ASK), h.div_ceil(ASK)), &fresh);
        lamps.ask_count = asked.len() as u32;
        if !asked.is_empty() {
            if asked.len() > lamps.ask_capacity {
                lamps.ask_capacity = asked.len().next_power_of_two();
                lamps.ask_instances = instance_buffer(device, lamps.ask_capacity);
            }
            queue.write_buffer(&lamps.ask_instances, 0, bytemuck::cast_slice(&asked));
            let size = (w.div_ceil(ASK), h.div_ceil(ASK));
            if lamps.ask_target.as_ref().is_none_or(|(s, _)| *s != size) {
                lamps.ask_target = Some((size, texture(device, "lamps' asking", size.0, size.1, ASK_FORMAT, false)));
            }
        }
        let globals = LampGlobals { inv_view_proj: frame.view_proj.inverse().to_cols_array_2d(), dims, origin, info: [lamps.state, self.pictures, 0, 0], movers_dims, movers_origin };
        queue.write_buffer(&lamps.globals, 0, bytemuck::bytes_of(&globals));
        if lamps.bind.is_none() || lf.walls != lamps.bound_walls {
            let bind = lamps.make_bind(device, lf.walls);
            lamps.bind = Some(bind);
        }
        tiles.len() as u32
    }

    /// The lamps' pass: each tile of the screen with the lights that reach
    /// it, their light added onto the scene where it has a surface.
    pub(super) fn lamp_pass(&self, encoder: &mut wgpu::CommandEncoder, lf: LampFrame, count: u32) {
        // Lamps new to the faces' memory asked for first (`ASK`).
        if let (Walls::Ready { pass: walled, .. }, true, Some((_, target))) = (&self.lamps.walls, lf.walls, &self.lamps.ask_target)
            && self.lamps.ask_count > 0
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lamps' asking"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Discard } })],
                depth_stencil_attachment: None,
                timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("lamps' asking")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(walled.ask.as_ref().expect("made with walls"));
            pass.set_bind_group(0, self.lamps.bind.as_ref().expect("made by write_lamps"), &[]);
            pass.set_vertex_buffer(0, self.lamps.ask_instances.slice(..));
            pass.draw(0..6, 0..self.lamps.ask_count);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lamps"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &self.targets.scene, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("lamps")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let pass_of = match (&self.lamps.walls, lf.walls) {
            (Walls::Ready { pass, .. }, true) => pass,
            _ => self.lamps.plain.as_ref().expect("made by lamps_of"),
        };
        if count == 0 {
            return;
        }
        pass.set_pipeline(&pass_of.pipeline);
        pass.set_bind_group(0, self.lamps.bind.as_ref().expect("made by write_lamps"), &[]);
        pass.set_vertex_buffer(0, self.lamps.instances.slice(..));
        pass.draw(0..6, 0..count);
    }

    /// A step of reading the count of faces the lamps' memory found no room
    /// for, without waiting (`render`): found full, made larger
    /// (`too_small`).
    pub(super) fn watch_lamps(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.lamps.told_full {
            return;
        }
        let (Walls::Ready { full, .. }, Some((faces, seen))) = (&mut self.lamps.walls, &self.lamps.seen) else { return };
        let faces = *faces;
        if let Some(count) = full.step(device, encoder, seen, full_at(faces), self.pictures) {
            self.too_small(device, faces, count);
        }
    }

    /// The same at once, waiting for the GPU (`shot`): whether it was
    /// found full and made larger (by the next picture).
    pub fn lamps_now(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<bool> {
        let Some((faces, seen)) = &self.lamps.seen else { return Ok(false) };
        if self.lamps.told_full {
            return Ok(false);
        }
        let faces = *faces;
        let count = super::limits::read_now(device, queue, seen, full_at(faces))?;
        Ok(self.too_small(device, faces, count))
    }

    /// The lamps' memory of `faces` answers found `count` that had no room:
    /// made twice as large from the next picture on (every answer walked
    /// once more), up to the GPU's largest buffer; past that said once.
    /// Whether it grew.
    fn too_small(&mut self, device: &wgpu::Device, faces: u64, count: u32) -> bool {
        if count == 0 {
            return false;
        }
        if faces * 2 <= most_faces(device) {
            self.lamps.grown = self.lamps.grown.max(faces * 2);
            return true;
        }
        self.lamps.told_full = true;
        self.said.push(lamps_full(faces, count));
        false
    }
}

/// What is said of the lamps' memory full, `count` faces having found no
/// room.
pub fn lamps_full(faces: u64, count: u32) -> String {
    format!("lamps: their memory is full ({faces} faces and points of beams, the most this GPU holds; {count} more found no room): those ask again every picture, the picture the same but slower")
}

impl Lamps {
    /// What the pass reads, with walls (the volume and the faces' memory)
    /// or without.
    /// The `i`th of the frame's lights as the shaders take it, walls
    /// stopping it where `walls` and it says so; a lamp whose shadows are
    /// "all" with its word for the moving things' boxes (`movers_words`).
    pub(super) fn data_of(&self, i: usize, l: &Light, walls: bool) -> LightData {
        let mut data = LightData::of(l, walls);
        if data.edge[1] > 1.5 {
            data.edge[1] = self.movers_words.get(i).copied().flatten().unwrap_or(2.0);
        }
        data
    }

    fn make_bind(&mut self, device: &wgpu::Device, walls: bool) -> wgpu::BindGroup {
        self.bound_walls = walls;
        let t = self.targets.as_ref().expect("made by lamps_of");
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&t.depth) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.surface) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&self.lights.1) },
        ];
        let layout = match (&self.walls, walls, &self.volume, &self.seen) {
            (Walls::Ready { pass, .. }, true, Some(held), Some((_, seen))) => {
                entries.push(wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&held.view) });
                entries.push(wgpu::BindGroupEntry { binding: 5, resource: seen.as_entire_binding() });
                let movers = self.movers.as_ref().map_or(&self.no_movers, |m| &m.view);
                entries.push(wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(movers) });
                entries.push(wgpu::BindGroupEntry { binding: 7, resource: self.movers_boxes.as_entire_binding() });
                entries.push(wgpu::BindGroupEntry { binding: 8, resource: self.movers_lists.as_entire_binding() });
                &pass.layout
            }
            _ => &self.plain.as_ref().expect("made by lamps_of").layout,
        };
        device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("lamps"), layout, entries: &entries })
    }
}

/// The picture's opaque cubes drawn in the scene (`FrameData::cube_runs`;
/// not those only casting): moving things, for lamps whose shadows are
/// "all".
fn opaque_cubes(frame: &FrameData) -> &[super::Instance] {
    let (behind, opaque) = frame.cube_runs();
    let drawn = opaque - (frame.cast_only as u32).min(opaque - behind);
    &frame.instances[behind as usize..drawn as usize]
}

/// The volume's next state number (never 0: an entry of the faces' memory
/// never written has 0).
fn next_state(state: u32) -> u32 {
    (state % 0x1ff) + 1
}

/// Writes `data` (`size` texels) at `origin` of copy `m`.
fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, m: u32, origin: IVec3, size: IVec3, data: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture, mip_level: m, origin: wgpu::Origin3d { x: origin.x as u32, y: origin.y as u32, z: origin.z as u32 }, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size.x as u32), rows_per_image: Some(size.y as u32) },
        wgpu::Extent3d { width: size.x as u32, height: size.y as u32, depth_or_array_layers: size.z as u32 },
    );
}

/// The pass with walls and the faces' memory. A mistake where the GPU
/// cannot have them.
fn make_walls(device: &wgpu::Device) -> Result<Walls, String> {
    if device.limits().max_storage_buffers_per_shader_stage == 0 {
        return Err("no storage buffers".into());
    }
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pass = lamp_pass(device, &LAMPS, true);
    let failed = [pollster::block_on(validation.pop()), pollster::block_on(internal.pop())];
    match failed.into_iter().flatten().next() {
        Some(e) => Err(e.to_string().lines().next().unwrap_or_default().to_string()),
        None => Ok(Walls::Ready { pass, full: FullWatch::new(device) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen's tiles are drawn with the lights whose rectangle
    /// reaches them, each tile once for every `LIGHTS_A_TILE`; a tile no
    /// light reaches is not drawn. A light's box behind the eye covers the
    /// screen; one off the screen is no rectangle.
    #[test]
    fn a_tile_is_drawn_with_the_lights_that_reach_it() {
        // 64 by 64 pixels: four tiles. A light over the left half, one
        // over the top right tile.
        let four = tiles((64, 64), &[(0, [-1.0, -1.0, 0.0, 1.0]), (1, [0.1, 0.1, 0.9, 0.9])]);
        let lights_of = |t: &TileInstance| (0..t.count[0] as usize).map(|k| (t.lights[k / 2] >> (16 * (k % 2))) & 0xffff).collect::<Vec<_>>();
        assert_eq!(four.iter().map(|t| (t.rect, lights_of(t))).collect::<Vec<_>>(), [([-1.0, 0.0, 0.0, 1.0], vec![0]), ([0.0, 0.0, 1.0, 1.0], vec![1]), ([-1.0, -1.0, 0.0, 0.0], vec![0])], "top left, top right, bottom left; bottom right none");
        let many: Vec<(u16, [f32; 4])> = (0..40).map(|k| (k, [-1.0, -1.0, 1.0, 1.0])).collect();
        let tiles = tiles((32, 32), &many);
        assert_eq!(tiles.iter().map(|t| t.count[0]).collect::<Vec<_>>(), [32, 8], "forty lights over one tile: drawn twice");
        assert_eq!(lights_of(&tiles[1]), (32..40).collect::<Vec<_>>());
        let view = crate::camera::view_proj(&crate::config::CameraConfig::default(), glam::Vec3::new(50.0, 50.0, 0.0), 16.0 / 9.0);
        let eye = crate::camera::eye(&crate::config::CameraConfig::default(), glam::Vec3::new(50.0, 50.0, 0.0), 16.0 / 9.0);
        assert_eq!(rect_of(&view, &Light::burst(eye, 5.0, [1.0; 3])), Some([-1.0, -1.0, 1.0, 1.0]), "round the eye: the whole screen");
        assert_eq!(rect_of(&view, &Light::burst(glam::Vec3::new(5000.0, 50.0, 0.0), 5.0, [1.0; 3])), None, "far off the screen");
    }
}

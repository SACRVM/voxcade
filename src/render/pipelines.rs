//! The renderer made: its pipelines, layouts, targets and buffers, and the
//! faces' memory of their shadow made when a frame first asks.

use wgpu::util::DeviceExt;

use super::bank::ActorData;
use super::shader::{Globals, PostParams, SHADER, SHADOW_FACES, plain_shader};
use super::bank::ACTOR_STRIDE;
use super::Stats;
use super::{DEPTH_FORMAT, HDR_FORMAT, Instance, Renderer, SHRINK, composite_entry};
use crate::mesh::{self, Vertex};

/// Everything sized to the window: depth, the scene and glow targets, their
/// quarter-size copies, and what the post passes read.
pub(super) struct Targets {
    pub(super) depth: wgpu::TextureView,
    pub(super) scene: wgpu::TextureView,
    pub(super) glow: wgpu::TextureView,
    /// The UI, over the world after the blur.
    pub(super) ui: wgpu::TextureView,
    /// Quarter size: scene blurred (0) and its scratch (1), glow blurred (2)
    /// and its scratch (3).
    pub(super) small: [wgpu::TextureView; 4],
    /// The post passes in order: what they do, what they read, which small
    /// target they write.
    pub(super) passes: Vec<(PostPass, wgpu::BindGroup, usize)>,
    /// The composite pass: the sharp scene plus the two blurred copies.
    pub(super) composite: wgpu::BindGroup,
    pub(super) blurred: wgpu::BindGroup,
    pub(super) composite_params: wgpu::Buffer,
}

#[derive(Clone, Copy)]
pub(super) enum PostPass {
    Down,
    Blur,
}

/// The pipelines that draw the scene with `fs_main`: the chunks, the
/// opaque cubes, the see-through ones, the models from the bank.
pub(super) struct ScenePipelines {
    pub(super) chunk: wgpu::RenderPipeline,
    pub(super) inst: wgpu::RenderPipeline,
    pub(super) see_through: wgpu::RenderPipeline,
    pub(super) model: wgpu::RenderPipeline,
}

/// The faces' memory of their shadow (`FrameData::remember`, the shader's
/// `remembered`): only a game that moves its sun needs it, and only its
/// pipelines carry it (`plain_shader`).
pub(super) enum Memory {
    /// No frame has asked for it yet.
    Unasked,
    /// Its pipelines, the layout and bind group of the main pass with the
    /// faces' buffer, the buffer.
    Ready { scene: ScenePipelines, layout: wgpu::BindGroupLayout, bind: wgpu::BindGroup, faces: wgpu::Buffer },
    /// The GPU cannot write storage from a fragment shader (said once):
    /// the plain pipelines draw, a moving shadow's edge may flicker.
    Lacking,
}

/// The main pass's bind group layout: the globals, the shadow map and its
/// sampler, and with `memory` the faces' memory of their shadow.
fn main_layout(device: &wgpu::Device, memory: bool) -> wgpu::BindGroupLayout {
    let mut entries = vec![
        uniform_entry(0, wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT),
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        },
        wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
    ];
    if memory {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        });
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(if memory { "main pass, remembering" } else { "main pass" }), entries: &entries })
}

/// The vertex buffers of a chunk mesh.
pub(super) fn chunk_buffers() -> [Option<wgpu::VertexBufferLayout<'static>>; 1] {
    [Some(wgpu::VertexBufferLayout { array_stride: size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &CHUNK_ATTRS })]
}

/// The vertex buffers of instanced cubes: the unit cube, the instances.
pub(super) fn instance_buffers() -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
    [
        Some(wgpu::VertexBufferLayout { array_stride: size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &CUBE_ATTRS }),
        Some(wgpu::VertexBufferLayout { array_stride: size_of::<Instance>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &INSTANCE_ATTRS }),
    ]
}

/// The scene's colour and glow targets, blended or not.
fn scene_targets(blend: Option<wgpu::BlendState>) -> [Option<wgpu::ColorTargetState>; 2] {
    let target = Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL });
    [target.clone(), target]
}

/// A pipeline of the shader `module`: `targets` empty, a shadow pass (no
/// fragment stage, both sides, a depth bias); `depth_write` false: the
/// see-through cubes.
#[allow(clippy::too_many_arguments)]
fn scene_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    label: &str,
    layout: &wgpu::PipelineLayout,
    vs: &str,
    buffers: &[Option<wgpu::VertexBufferLayout>],
    targets: &[Option<wgpu::ColorTargetState>],
    depth_write: bool,
) -> wgpu::RenderPipeline {
    let shadow = targets.is_empty();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: Some(vs), compilation_options: Default::default(), buffers },
        primitive: wgpu::PrimitiveState {
            front_face: wgpu::FrontFace::Ccw,
            // Meshes only contain camera- and sun-facing faces; the
            // shadow pass must see all of them. A model from the bank is
            // a closed surface (its covered faces left out): what of it
            // faces away from the sun lies behind what faces it.
            cull_mode: if !shadow { Some(wgpu::Face::Back) } else { None },
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: if shadow { wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 } } else { Default::default() },
        }),
        multisample: Default::default(),
        fragment: if shadow { None } else { Some(wgpu::FragmentState { module, entry_point: Some("fs_main"), compilation_options: Default::default(), targets }) },
        multiview_mask: None,
        cache: None,
    })
}

/// The scene's pipelines of the shader `module`, its main pass laid out as
/// `main`, a model draw's place as `actor`.
fn scene_pipelines(device: &wgpu::Device, module: &wgpu::ShaderModule, main: &wgpu::BindGroupLayout, actor: &wgpu::BindGroupLayout) -> ScenePipelines {
    let layout = |label: &str, groups: &[Option<&wgpu::BindGroupLayout>]| device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: groups, immediate_size: 0 });
    let main_pipeline_layout = layout("main pass", &[Some(main)]);
    let model_main_layout = layout("models", &[Some(main), Some(actor)]);
    let (opaque, see_through) = (scene_targets(None), scene_targets(Some(wgpu::BlendState::ALPHA_BLENDING)));
    ScenePipelines {
        chunk: scene_pipeline(device, module, "chunks", &main_pipeline_layout, "vs_chunk", &chunk_buffers(), &opaque, true),
        inst: scene_pipeline(device, module, "instances", &main_pipeline_layout, "vs_inst", &instance_buffers(), &opaque, true),
        see_through: scene_pipeline(device, module, "see-through", &main_pipeline_layout, "vs_inst", &instance_buffers(), &see_through, false),
        model: scene_pipeline(device, module, "models", &model_main_layout, "vs_model", &instance_buffers(), &opaque, true),
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
            source: wgpu::ShaderSource::Wgsl(plain_shader(&SHADER).into()),
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxcade post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../post.wgsl").into()),
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
        let main_layout = main_layout(device, false);
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
        let (main_bind, ui_bind) = Self::scene_binds(device, &main_layout, &globals, &ui_globals, &shadow_view, &shadow_sampler);

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
        let model_shadow_layout = layout("model shadows", &[Some(&shadow_layout), Some(&actor_layout)]);
        let model_main_layout = layout("models", &[Some(&main_layout), Some(&actor_layout)]);
        let post_pipeline_layout = layout("post", &[Some(&post_layout)]);
        let composite_pipeline_layout = layout("composite", &[Some(&post_layout), Some(&blurred_layout)]);

        let opaque_targets = scene_targets(None);
        let plain = scene_pipelines(device, &shader, &main_layout, &actor_layout);
        let chunk_shadow = scene_pipeline(device, &shader, "chunk shadows", &shadow_pipeline_layout, "vs_chunk_shadow", &chunk_buffers(), &[], true);
        let inst_shadow = scene_pipeline(device, &shader, "instance shadows", &shadow_pipeline_layout, "vs_inst_shadow", &instance_buffers(), &[], true);
        let model_shadow = scene_pipeline(device, &shader, "model shadows", &model_shadow_layout, "vs_model_shadow", &instance_buffers(), &[], true);
        // The outline: the far faces of the grown voxels, in a flat colour.
        let model_outline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model outlines"),
            layout: Some(&model_main_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_model_outline"), compilation_options: Default::default(), buffers: &instance_buffers() },
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
        let post_composite = post_pipeline("composite", &composite_pipeline_layout, composite_entry(format), format);

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
            memory: Memory::Unasked,
            pictures: 0,
            main_layout,
            plain,
            chunk_shadow,
            inst_shadow,
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
            window: (width, height),
            shape: None,
        }
    }

    pub(super) fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (capacity * size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub(super) fn shadow_map(device: &wgpu::Device, size: u32) -> wgpu::TextureView {
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
    pub(super) fn scene_binds(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        ui_globals: &wgpu::Buffer,
        shadow_view: &wgpu::TextureView,
        shadow_sampler: &wgpu::Sampler,
    ) -> (wgpu::BindGroup, wgpu::BindGroup) {
        let bind = |label, buffer: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(shadow_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(shadow_sampler) },
                ],
            })
        };
        (bind("main pass", globals), bind("ui pass", ui_globals))
    }

    pub(super) fn actor_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
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

    pub(super) fn targets(
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

    /// Whether the faces can remember their shadow: the pipelines with the
    /// memory, made the first time a frame asks; where the GPU cannot write
    /// storage from a fragment shader (or anything else in making them
    /// fails), said once, and the plain pipelines draw from then on.
    pub(super) fn remembering(&mut self, device: &wgpu::Device) -> bool {
        if matches!(self.memory, Memory::Unasked) {
            self.memory = self.make_memory(device).unwrap_or_else(|why| {
                eprintln!("this GPU cannot keep the faces' memory of their shadow ({why}): where the game moves its sun, a shadow's edge may flicker");
                Memory::Lacking
            });
        }
        matches!(self.memory, Memory::Ready { .. })
    }

    /// The faces' memory of their shadow: its pipelines (the whole
    /// shader), its layout, buffer and bind group. A mistake where the GPU
    /// cannot have them.
    fn make_memory(&self, device: &wgpu::Device) -> Result<Memory, String> {
        if device.limits().max_storage_buffers_per_shader_stage == 0 {
            return Err("no storage buffers".into());
        }
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("voxcade shader, remembering"), source: wgpu::ShaderSource::Wgsl(SHADER.as_str().into()) });
        let layout = main_layout(device, true);
        let scene = scene_pipelines(device, &module, &layout, &self.actor_layout);
        let faces = device.create_buffer(&wgpu::BufferDescriptor { label: Some("faces' shadows"), size: SHADOW_FACES * 3 * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let bind = Self::memory_bind(device, &layout, &self.globals, &self.shadow_view, &self.shadow_sampler, &faces);
        let failed = [pollster::block_on(validation.pop()), pollster::block_on(internal.pop())];
        match failed.into_iter().flatten().next() {
            Some(e) => Err(e.to_string().lines().next().unwrap_or_default().to_string()),
            None => Ok(Memory::Ready { scene, layout, bind, faces }),
        }
    }

    /// The main pass's bind group with the faces' memory.
    pub(super) fn memory_bind(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, globals: &wgpu::Buffer, shadow_view: &wgpu::TextureView, shadow_sampler: &wgpu::Sampler, faces: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("main pass, remembering"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(shadow_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(shadow_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: faces.as_entire_binding() },
            ],
        })
    }

    /// Whether the faces remember their shadow on this GPU (asked once a
    /// frame wanted it): the tests'.
    #[cfg(test)]
    pub(crate) fn remembers(&self) -> bool {
        matches!(self.memory, Memory::Ready { .. })
    }
}

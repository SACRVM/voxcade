//! Fog: air within the level's box that holds the light of the picture's
//! lamps, so a spot's cone shows as a beam, a lamp's ball as a glow, and
//! what lies behind fades towards the fog's colour (`Fog`,
//! `c.light.fog_density`, `frame:fog`).
//!
//! A picture with fog is drawn by the lamps' pass (`render::lamps`), which
//! keeps the scene's depth. After its see-through cubes the beams
//! (`fog_beams.wgsl`) go into a target a fraction of the picture's size
//! (`Fog::scale`, the player's FOG setting), drawn as the lamps are, by
//! the screen's tiles with the lights whose box reaches each, walls
//! stopping a lamp's beam as they stop its light (the same volume and
//! faces' memory). The composite then lays the fog over the scene before
//! the UI (`fog_composite.wgsl`, a pipeline of its own: a picture without
//! fog is put together as it always was): what the fog took, and the
//! beams, each pixel of them weighed by how alike its depth is. The UI is
//! never in fog.
//!
//! Only the picture: nothing here is the game's (no fingerprint, state
//! hash, replay or save sees it).

use bytemuck::{Pod, Zeroable};

use super::lamps::{LampFrame, TileInstance, instance_buffer, lamp_buffers, lights_texture, rect_of, tiles};
use super::pipelines::{texture, uniform_entry};
use super::shader::{FOG_BEAMS, FOG_COMPOSITE, LightData, without_walls};
use super::{Fog, FrameData, HDR_FORMAT, MOST_LIGHTS, Renderer};

/// The fog's numbers (`fog.wgsl`'s `Fog`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct FogGlobals {
    pub(super) inv_view_proj: [[f32; 4]; 4],
    /// xyz: the eye, w: the density.
    pub(super) eye: [f32; 4],
    /// rgb: the fog's colour, w: its anisotropy.
    pub(super) color: [f32; 4],
    /// xyz: the air's low corner, w: the density the beams are lit by.
    pub(super) lo: [f32; 4],
    /// xyz: its high corner.
    pub(super) hi: [f32; 4],
    /// xyz: the way the camera looks.
    pub(super) forward: [f32; 4],
    /// xy: the beams' target's size, z: its scale (`Fog::scale`).
    pub(super) size: [u32; 4],
}

/// The fog's passes and what they read, made the first time a picture has
/// fog.
pub(super) struct FogPass {
    globals: wgpu::Buffer,
    /// The beams' own group (after the lamps' pass's): the numbers and the
    /// lights.
    layout: wgpu::BindGroupLayout,
    /// The beams' pipelines, without walls and with (made when first asked).
    beams: [Option<wgpu::RenderPipeline>; 2],
    /// The composite with fog: its third group (the numbers, the depth, the
    /// beams) and its pipeline.
    composite_layout: wgpu::BindGroupLayout,
    pub(super) composite: wgpu::RenderPipeline,
    /// The beams' target: its size and scale.
    target: Option<((u32, u32), u32, wgpu::TextureView)>,
    lights: (wgpu::Texture, wgpu::TextureView),
    rows: u32,
    instances: wgpu::Buffer,
    capacity: usize,
    bind: Option<wgpu::BindGroup>,
    pub(super) composite_bind: Option<wgpu::BindGroup>,
}

impl FogPass {
    /// The pipelines for a picture drawn onto `format`, its composite's
    /// first groups laid out as `post` and `blurred` (the composite's own).
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, post: &wgpu::BindGroupLayout, blurred: &wgpu::BindGroupLayout) -> Self {
        let globals = device.create_buffer(&wgpu::BufferDescriptor { label: Some("fog"), size: size_of::<FogGlobals>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let tex = |binding, sample_type| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fog's beams"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT), tex(1, wgpu::TextureSampleType::Float { filterable: false })],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite, fog"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT), tex(1, wgpu::TextureSampleType::Depth), tex(2, wgpu::TextureSampleType::Float { filterable: false })],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("composite, fog"), source: wgpu::ShaderSource::Wgsl(FOG_COMPOSITE.as_str().into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("composite, fog"), bind_group_layouts: &[Some(post), Some(blurred), Some(&composite_layout)], immediate_size: 0 });
        let entry = if format.is_srgb() { "fs_composite_fog_srgb" } else { "fs_composite_fog" };
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite, fog"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_post"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(entry), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            globals,
            layout,
            beams: [None, None],
            composite_layout,
            composite,
            target: None,
            lights: lights_texture(device, 64),
            rows: 64,
            instances: instance_buffer(device, 64),
            capacity: 64,
            bind: None,
            composite_bind: None,
        }
    }

    /// The picture's size changed: the beams' target is made anew.
    pub(super) fn resized(&mut self) {
        self.target = None;
        self.composite_bind = None;
    }
}

/// The beams' pipeline, with walls or without, its first group the lamps'
/// pass's (`lamps`), its second the fog's own.
fn beams_pipeline(device: &wgpu::Device, lamps: &wgpu::BindGroupLayout, own: &wgpu::BindGroupLayout, walls: bool) -> wgpu::RenderPipeline {
    let src = if walls { FOG_BEAMS.to_string() } else { without_walls(&FOG_BEAMS) };
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fog's beams"), source: wgpu::ShaderSource::Wgsl(src.into()) });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("fog's beams"), bind_group_layouts: &[Some(lamps), Some(own)], immediate_size: 0 });
    // Each tile's light added; the alpha how far the eye sees there, the
    // same from every draw of a tile.
    let add = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max },
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fog's beams"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_lamp"), compilation_options: Default::default(), buffers: &lamp_buffers() },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs_beam"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(add), write_mask: wgpu::ColorWrites::ALL })] }),
        multiview_mask: None,
        cache: None,
    })
}

impl Renderer {
    /// The fog's lights, tiles and numbers for this picture (its fog `fog`,
    /// drawn by the lamps' pass as `lf`), everything it needs made the
    /// first time; how many tiles its beams draw.
    pub(super) fn write_fog(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData, fog: &Fog, lf: LampFrame) -> u32 {
        let scale = fog.scale.max(1);
        let (_, _, w, h) = self.picture();
        let size = (w.div_ceil(scale), h.div_ceil(scale));
        let pass = self.fog.get_or_insert_with(|| FogPass::new(device, self.format, &self.post_layout, &self.blurred_layout));
        let walls = usize::from(lf.walls);
        if pass.beams[walls].is_none() {
            pass.beams[walls] = Some(beams_pipeline(device, self.lamps.layout(lf.walls), &pass.layout, lf.walls));
        }
        if pass.target.as_ref().is_none_or(|(s, k, _)| *s != size || *k != scale) {
            pass.target = Some((size, scale, texture(device, "fog's beams", size.0, size.1, HDR_FORMAT, true)));
            pass.composite_bind = None;
        }
        // The lights that show in fog, each as bright as it shows there.
        let shown: Vec<(LightData, [f32; 4])> = frame
            .lights
            .iter()
            .take(MOST_LIGHTS)
            .filter(|l| l.fog > 0.0 && l.color.iter().any(|c| *c > 0.0))
            .filter_map(|l| {
                let mut data = LightData::of(l, lf.walls);
                for c in &mut data.color[..3] {
                    *c *= l.fog;
                }
                rect_of(&frame.view_proj, l).map(|rect| (data, rect))
            })
            .collect();
        let tiles: Vec<TileInstance> = tiles(size, &shown.iter().enumerate().map(|(row, (_, rect))| (row as u16, *rect)).collect::<Vec<_>>());
        if tiles.len() > pass.capacity {
            pass.capacity = tiles.len().next_power_of_two();
            pass.instances = instance_buffer(device, pass.capacity);
        }
        if shown.len() as u32 > pass.rows {
            pass.rows = (shown.len() as u32).next_power_of_two();
            pass.lights = lights_texture(device, pass.rows);
            pass.bind = None;
        }
        if !tiles.is_empty() {
            let rows: Vec<LightData> = shown.iter().map(|(d, _)| *d).collect();
            queue.write_buffer(&pass.instances, 0, bytemuck::cast_slice(&tiles));
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &pass.lights.0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(&rows),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size_of::<LightData>() as u32), rows_per_image: Some(rows.len() as u32) },
                wgpu::Extent3d { width: 4, height: rows.len() as u32, depth_or_array_layers: 1 },
            );
        }
        let inverse = frame.view_proj.inverse();
        let ahead = inverse.project_point3(glam::Vec3::new(0.0, 0.0, 1.0)) - inverse.project_point3(glam::Vec3::ZERO);
        let [r, g, b] = fog.color;
        let globals = FogGlobals {
            inv_view_proj: inverse.to_cols_array_2d(),
            eye: frame.eye.extend(fog.density).to_array(),
            color: [r, g, b, fog.anisotropy],
            lo: fog.lo.extend(fog.density * fog.glow).to_array(),
            hi: fog.hi.extend(0.0).to_array(),
            forward: ahead.normalize_or(glam::Vec3::NEG_Z).extend(0.0).to_array(),
            size: [size.0, size.1, scale, 0],
        };
        queue.write_buffer(&pass.globals, 0, bytemuck::bytes_of(&globals));
        if pass.bind.is_none() {
            pass.bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fog's beams"),
                layout: &pass.layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: pass.globals.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&pass.lights.1) }],
            }));
        }
        if pass.composite_bind.is_none() {
            let (_, _, beams) = pass.target.as_ref().expect("made above");
            pass.composite_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composite, fog"),
                layout: &pass.composite_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: pass.globals.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.lamps.targets().depth) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(beams) },
                ],
            }));
        }
        tiles.len() as u32
    }

    /// The fog's beams into their target (`count` tiles); the composite
    /// lays them over the picture.
    pub(super) fn beams_pass(&self, encoder: &mut wgpu::CommandEncoder, lf: LampFrame, count: u32) {
        let pass_of = self.fog.as_ref().expect("made by write_fog");
        let (_, _, target) = pass_of.target.as_ref().expect("made by write_fog");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fog's beams"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("fog beams")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if count > 0 {
            pass.set_pipeline(pass_of.beams[usize::from(lf.walls)].as_ref().expect("made by write_fog"));
            pass.set_bind_group(0, self.lamps.bind(), &[]);
            pass.set_bind_group(1, pass_of.bind.as_ref().expect("made by write_fog"), &[]);
            pass.set_vertex_buffer(0, pass_of.instances.slice(..));
            pass.draw(0..6, 0..count);
        }
    }
}

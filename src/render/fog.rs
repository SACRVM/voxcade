//! Fog: air within the level's box that holds the light of the picture's
//! lamps, so a spot's cone shows as a beam, a lamp's ball as a glow, and
//! what lies behind fades towards the fog's colour (`Fog`,
//! `c.light.fog_density`, `frame:fog`).
//!
//! A picture with fog is drawn by the lamps' pass (`render::lamps`), which
//! keeps the scene's depth. After it the beams (`fog_beams.wgsl`) go into a
//! target a fraction of the picture's size (`Fog::scale`, the player's FOG
//! setting), drawn as the lamps are, by the screen's tiles with the lights
//! whose box reaches each, walls stopping a lamp's beam as they stop its
//! light (the same volume and faces' memory). The composite then lays the
//! fog over the scene before the UI (`fog_composite.wgsl`, a pipeline of its
//! own: a picture without fog is put together as it always was): what the
//! fog took, and the beams, each pixel of them weighed by how alike its
//! depth is. The UI is never in fog.
//!
//! Glass and water (the see-through cubes) in a picture with fog: their
//! depth is drawn first (`glass_pass`), the beams are worked out before
//! them and behind them apart (`fs_beam_glass`), the fog behind them laid
//! over the scene before they are drawn onto it (`fog_far.wgsl`), so they
//! tint it, and the composite lays the fog before them. A picture of fog
//! without see-through cubes runs none of it.
//!
//! Only the picture: nothing here is the game's (no fingerprint, state
//! hash, replay or save sees it).

use bytemuck::{Pod, Zeroable};

use super::lamps::{LampFrame, LampRow, TileInstance, instance_buffer, lamp_buffers, rect_of, tiles};
use super::pipelines::{instance_buffers, texture, uniform_entry};
use super::shader::{FOG_BEAMS, FOG_COMPOSITE, FOG_FAR, SHADER, plain_shader, without_thin, without_walls};
use super::movers::lit;
use super::thin::Thin;
use super::{DEPTH_FORMAT, Fog, FrameData, HDR_FORMAT, Light, MOST_LIGHTS, Renderer};
use glam::Vec3;

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
    /// The beams' own group (after the lamps' pass's): the numbers, the
    /// lights and the see-through cubes' depth.
    layout: wgpu::BindGroupLayout,
    /// The beams' pipelines, without walls and with, without glass and with,
    /// without thin things' work and with (made when first asked).
    beams: [[[Option<wgpu::RenderPipeline>; 2]; 2]; 2],
    /// Whether this picture's beams look for thin things (`render::thin`).
    pub(super) thin: bool,
    /// The composite with fog: its third group (the numbers, the depth, the
    /// beams, the see-through cubes' depth) and its pipelines without glass
    /// and with.
    composite_layout: wgpu::BindGroupLayout,
    pub(super) composite: [wgpu::RenderPipeline; 2],
    /// The beams' target: its size and scale.
    target: Option<((u32, u32), u32, wgpu::TextureView)>,
    lights: (wgpu::Texture, wgpu::TextureView),
    rows: u32,
    instances: wgpu::Buffer,
    capacity: usize,
    /// What the beams and the composite read, without glass and with.
    bind: [Option<wgpu::BindGroup>; 2],
    pub(super) composite_bind: [Option<wgpu::BindGroup>; 2],
    /// Glass and water: made the first time a picture of fog has them.
    glass: Option<Glass>,
}

/// What the fog needs of a picture with see-through cubes.
struct Glass {
    /// Their depth, drawn as the scene's are (`vs_inst`, nothing else).
    depth_pipeline: wgpu::RenderPipeline,
    /// The fog behind them laid over the scene (`fog_far.wgsl`).
    far_layout: wgpu::BindGroupLayout,
    far_pipeline: wgpu::RenderPipeline,
    targets: Option<GlassTargets>,
    /// What the far pass reads.
    far_bind: Option<wgpu::BindGroup>,
}

/// The see-through cubes' depth (the picture's size, `full`) and the beams
/// behind them (the beams' target's size, `small`).
struct GlassTargets {
    full: (u32, u32),
    small: (u32, u32),
    depth: wgpu::TextureView,
    far: wgpu::TextureView,
}

impl FogPass {
    /// The pipelines for a picture drawn onto `format`, its composite's
    /// first groups laid out as `post` and `blurred` (the composite's own).
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, post: &wgpu::BindGroupLayout, blurred: &wgpu::BindGroupLayout) -> Self {
        let globals = device.create_buffer(&wgpu::BufferDescriptor { label: Some("fog"), size: size_of::<FogGlobals>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fog's beams"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT), tex(1, wgpu::TextureSampleType::Float { filterable: false }), tex(2, wgpu::TextureSampleType::Depth)],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite, fog"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT), tex(1, wgpu::TextureSampleType::Depth), tex(2, wgpu::TextureSampleType::Float { filterable: false }), tex(3, wgpu::TextureSampleType::Depth)],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("composite, fog"), source: wgpu::ShaderSource::Wgsl(FOG_COMPOSITE.as_str().into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("composite, fog"), bind_group_layouts: &[Some(post), Some(blurred), Some(&composite_layout)], immediate_size: 0 });
        let srgb = if format.is_srgb() { "_srgb" } else { "" };
        let composite = ["fs_composite_fog", "fs_composite_fog_glass"].map(|entry| {
            let entry = format!("{entry}{srgb}");
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("composite, fog"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_post"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(&entry), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
                multiview_mask: None,
                cache: None,
            })
        });
        Self {
            globals,
            layout,
            beams: Default::default(),
            thin: false,
            composite_layout,
            composite,
            target: None,
            lights: lights_texture(device, 64),
            rows: 64,
            instances: instance_buffer(device, 64),
            capacity: 64,
            bind: [None, None],
            composite_bind: [None, None],
            glass: None,
        }
    }

    /// The picture's size changed, or the lamps were let go
    /// (`Lamps::used`): the beams' targets made anew when next asked.
    pub(super) fn resized(&mut self) {
        self.target = None;
        self.bind = [None, None];
        self.composite_bind = [None, None];
        if let Some(glass) = &mut self.glass {
            glass.targets = None;
            glass.far_bind = None;
        }
    }
}

/// A texture a fragment stage reads.
fn tex(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None }
}

/// What the fog needs of glass and water: the depth pipeline of the scene's
/// shader laid out as `main`, the far pass.
fn glass(device: &wgpu::Device, main: &wgpu::BindGroupLayout) -> Glass {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("see-through depth"), source: wgpu::ShaderSource::Wgsl(plain_shader(&SHADER).into()) });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("see-through depth"), bind_group_layouts: &[Some(main)], immediate_size: 0 });
    let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("see-through depth"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_inst"), compilation_options: Default::default(), buffers: &instance_buffers() },
        primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH_FORMAT, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Less), stencil: Default::default(), bias: Default::default() }),
        multisample: Default::default(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    });
    let far_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fog behind glass"),
        entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT), tex(1, wgpu::TextureSampleType::Depth), tex(2, wgpu::TextureSampleType::Depth), tex(3, wgpu::TextureSampleType::Float { filterable: false })],
    });
    let far_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fog behind glass"), source: wgpu::ShaderSource::Wgsl(FOG_FAR.as_str().into()) });
    let far_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("fog behind glass"), bind_group_layouts: &[Some(&far_layout)], immediate_size: 0 });
    // The scene times what the fog left (the alpha), the fog's light added.
    let over = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::SrcAlpha, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    };
    let far_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fog behind glass"),
        layout: Some(&far_pipeline_layout),
        vertex: wgpu::VertexState { module: &far_module, entry_point: Some("vs_far"), compilation_options: Default::default(), buffers: &[] },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState { module: &far_module, entry_point: Some("fs_fog_far"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(over), write_mask: wgpu::ColorWrites::ALL })] }),
        multiview_mask: None,
        cache: None,
    });
    Glass { depth_pipeline, far_layout, far_pipeline, targets: None, far_bind: None }
}

/// The beams' pipeline, with walls or without, with glass (two targets:
/// before the see-through cubes and behind them) or without, its first
/// group the lamps' pass's (`lamps`), its second the fog's own.
fn beams_pipeline(device: &wgpu::Device, lamps: &wgpu::BindGroupLayout, own: &wgpu::BindGroupLayout, walls: bool, glass: bool, thin: bool) -> wgpu::RenderPipeline {
    let src = if walls { FOG_BEAMS.to_string() } else { without_walls(&FOG_BEAMS) };
    let src = if thin { src } else { without_thin(&src) };
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fog's beams"), source: wgpu::ShaderSource::Wgsl(src.into()) });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("fog's beams"), bind_group_layouts: &[Some(lamps), Some(own)], immediate_size: 0 });
    // Each tile's light added; the alpha how far the eye sees there, the
    // same from every draw of a tile.
    let add = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max },
    };
    let target = Some(wgpu::ColorTargetState { format: HDR_FORMAT, blend: Some(add), write_mask: wgpu::ColorWrites::ALL });
    let targets = [target.clone(), target];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fog's beams"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_lamp"), compilation_options: Default::default(), buffers: &lamp_buffers() },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(if glass { "fs_beam_glass" } else { "fs_beam" }), compilation_options: Default::default(), targets: &targets[..1 + usize::from(glass)] }),
        multiview_mask: None,
        cache: None,
    })
}

/// The most balls round thin things a light's row lists (`thin_balls`):
/// past them the blocks are taken together in bigger ones.
const MOST_THIN: usize = 16;

/// The columns of a light's row in the fog's lights (`FogRow`).
const FOG_COLUMNS: u32 = (size_of::<FogRow>() / 16) as u32;

/// A light's row in the fog's lights: the light as the lamps' pass takes it
/// (`LampRow`), how many balls round thin things (at the column
/// `THIN_AT`), the balls.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FogRow {
    light: LampRow,
    count: [f32; 4],
    balls: [[f32; 4]; MOST_THIN],
}

/// The column of a light's row in the fog's lights where its balls round
/// thin things begin, their count (the shaders' FOG_THIN_AT).
pub(super) const THIN_AT: usize = std::mem::offset_of!(FogRow, count) / 16;

impl FogRow {
    fn of(light: &LampRow, balls: Vec<[f32; 4]>) -> Self {
        let mut row = Self { light: *light, count: [balls.len() as f32, 0.0, 0.0, 0.0], balls: [[0.0; 4]; MOST_THIN] };
        row.balls[..balls.len()].copy_from_slice(&balls);
        row
    }
}

/// The fog's lights' texture, room for `rows`.
fn lights_texture(device: &wgpu::Device, rows: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fog's lights"),
        size: wgpu::Extent3d { width: FOG_COLUMNS, height: rows, depth_or_array_layers: 1 },
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

/// The planes a beam's points lie on are this many voxels apart along the
/// camera's way (the shaders' FOG_STEP) for a light whose ball is at most
/// `MOST_PLANES` of them across (FOG_MOST_PLANES); twice (four times ...)
/// as far for a wider one.
pub(super) const STEP: f32 = 4.0;
pub(super) const MOST_PLANES: f32 = 48.0;

/// The spacing of a beam's points for the light `l` (`fog_beams.wgsl`'s
/// `step`).
fn beam_step(l: &Light) -> i32 {
    let planes = (2.0 * l.radius / (STEP * MOST_PLANES)).log2().ceil().max(0.0);
    (STEP as i32) << (planes as i32).min(8)
}

/// A ball's spacing is kept in its radius's place, this many times the
/// spacing's power of two added (`fog_beams.wgsl`'s `thin_span`, the
/// shaders' FOG_SPACING_AT).
pub(super) const SPACING_AT: f32 = 65536.0;

/// The balls round the thin things (`render::thin`) the light `l` can
/// light, thin by its beam's spacing: centre and radius, a block's and a
/// voxel more (a point stands for its voxel's middle), with the spacing
/// its points need there (`SPACING_AT`). Past `MOST_THIN` the blocks are
/// taken together by twice their size, and again, until that many hold
/// them, each the finest spacing of its blocks.
fn thin_balls(thin: &Thin, l: &Light) -> Vec<[f32; 4]> {
    let mut boxes: Vec<(Vec3, Vec3, i32)> = thin.near(l.pos - l.radius, l.pos + l.radius, beam_step(l)).into_iter().filter(|(lo, hi, _)| lit(l, *lo, *hi)).collect();
    let mut size = super::thin::BLOCK as f32;
    while boxes.len() > MOST_THIN {
        size *= 2.0;
        let mut joined: std::collections::BTreeMap<(i32, i32, i32), (Vec3, Vec3, i32)> = std::collections::BTreeMap::new();
        for (lo, hi, s) in boxes {
            let k = (lo / size).floor().as_ivec3();
            let e = joined.entry((k.z, k.y, k.x)).or_insert((lo, hi, s));
            *e = (e.0.min(lo), e.1.max(hi), e.2.min(s));
        }
        boxes = joined.into_values().collect();
    }
    boxes.iter().map(|(lo, hi, s)| ((*lo + *hi) * 0.5).extend((*hi - *lo).length() * 0.5 + 1.0 + SPACING_AT * s.ilog2() as f32).to_array()).collect()
}

impl Renderer {
    /// Whether the lamps' pass or the fog hold anything a picture made (let
    /// go after `lamps::LET_GO` pictures without them): the tests'.
    #[cfg(test)]
    pub fn lamps_held(&self) -> bool {
        self.lamps.holds() || self.fog.as_ref().is_some_and(|f| f.target.is_some() || f.glass.as_ref().is_some_and(|g| g.targets.is_some()))
    }

    /// The fog's lights, tiles and numbers for this picture (its fog `fog`,
    /// drawn by the lamps' pass as `lf`; `glass`: with see-through cubes),
    /// everything it needs made the first time; how many tiles its beams
    /// draw.
    pub(super) fn write_fog(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData, fog: &Fog, lf: LampFrame, glass: bool) -> u32 {
        let scale = fog.scale.max(1);
        let (_, _, w, h) = self.picture();
        let size = (w.div_ceil(scale), h.div_ceil(scale));
        let pass = self.fog.get_or_insert_with(|| FogPass::new(device, self.format, &self.post_layout, &self.blurred_layout));
        let g = usize::from(glass);
        if pass.target.as_ref().is_none_or(|(s, k, _)| *s != size || *k != scale) {
            pass.target = Some((size, scale, texture(device, "fog's beams", size.0, size.1, HDR_FORMAT, true)));
            pass.composite_bind = [None, None];
        }
        if glass {
            let made = pass.glass.get_or_insert_with(|| self::glass(device, &self.main_layout));
            if made.targets.as_ref().is_none_or(|t| t.full != (w, h) || t.small != size) {
                let depth = texture(device, "see-through depth", w, h, DEPTH_FORMAT, true);
                let far = texture(device, "fog's beams behind glass", size.0, size.1, HDR_FORMAT, true);
                made.targets = Some(GlassTargets { full: (w, h), small: size, depth, far });
                made.far_bind = None;
                pass.bind[1] = None;
                pass.composite_bind[1] = None;
            }
        }
        // The lights that show in fog, each as bright as it shows there,
        // with the balls round the thin things it lights that walls stop it
        // (`render::thin`).
        let shown: Vec<(LampRow, [f32; 4], &Light)> = frame
            .lights
            .iter()
            .take(MOST_LIGHTS)
            .enumerate()
            .filter(|(_, l)| l.fog > 0.0 && l.color.iter().any(|c| *c > 0.0))
            .filter_map(|(i, l)| {
                let mut data = self.lamps.data_of(i, l, lf.walls);
                for c in &mut data.light.color[..3] {
                    *c *= l.fog;
                }
                rect_of(&frame.view_proj, l).map(|rect| (data, rect, l))
            })
            .collect();
        let thin = if fog.thin && lf.walls && shown.iter().any(|(d, _, _)| d.light.edge[1] > 0.5) { self.lamps.thin() } else { None };
        let rows: Vec<FogRow> = shown.iter().map(|(d, _, l)| FogRow::of(d, thin.filter(|_| d.light.edge[1] > 0.5).map_or(Vec::new(), |t| thin_balls(t, l)))).collect();
        let pass = self.fog.as_mut().expect("made above");
        let mut tiles: Vec<TileInstance> = tiles(size, &shown.iter().enumerate().map(|(row, (_, rect, _))| (row as u16, *rect)).collect::<Vec<_>>());
        // Each tile's lights with thin things (a bit each, `count.y`), and
        // the beams' pipeline that looks for them only where some are: the
        // work for them costs a ray that never meets one too (the Radeon
        // 610M's beams of 16 lamps 8.8 against 10.4 ms).
        let mut thin_here = false;
        for t in &mut tiles {
            for k in 0..t.count[0] as usize {
                let row = (t.lights[k / 2] >> (16 * (k % 2))) & 0xffff;
                if rows[row as usize].count[0] > 0.0 {
                    t.count[1] |= 1 << k;
                    thin_here = true;
                }
            }
        }
        let (walls, th) = (usize::from(lf.walls), usize::from(thin_here));
        if pass.beams[walls][g][th].is_none() {
            pass.beams[walls][g][th] = Some(beams_pipeline(device, self.lamps.layout(lf.walls), &pass.layout, lf.walls, glass, thin_here));
        }
        pass.thin = thin_here;
        if tiles.len() > pass.capacity {
            pass.capacity = tiles.len().next_power_of_two();
            pass.instances = instance_buffer(device, pass.capacity);
        }
        if shown.len() as u32 > pass.rows {
            pass.rows = (shown.len() as u32).next_power_of_two();
            pass.lights = lights_texture(device, pass.rows);
            pass.bind = [None, None];
        }
        if !tiles.is_empty() {
            queue.write_buffer(&pass.instances, 0, bytemuck::cast_slice(&tiles));
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &pass.lights.0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(&rows),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size_of::<FogRow>() as u32), rows_per_image: Some(rows.len() as u32) },
                wgpu::Extent3d { width: FOG_COLUMNS, height: rows.len() as u32, depth_or_array_layers: 1 },
            );
        }
        let inverse = frame.view_proj.inverse();
        let ahead = inverse.project_point3(glam::Vec3::new(0.0, 0.0, 1.0)) - inverse.project_point3(glam::Vec3::ZERO);
        let [r, g_, b] = fog.color;
        let globals = FogGlobals {
            inv_view_proj: inverse.to_cols_array_2d(),
            eye: frame.eye.extend(fog.density).to_array(),
            color: [r, g_, b, fog.anisotropy],
            lo: fog.lo.extend(0.0).to_array(),
            hi: fog.hi.extend(fog.glow).to_array(),
            forward: ahead.normalize_or(glam::Vec3::NEG_Z).extend(0.0).to_array(),
            size: [size.0, size.1, scale, 0],
        };
        queue.write_buffer(&pass.globals, 0, bytemuck::bytes_of(&globals));
        // Without glass the see-through depth's place holds the scene's (it
        // is not read then).
        let opaque = &self.lamps.targets().depth;
        let (_, _, beams) = pass.target.as_ref().expect("made above");
        let see_through = pass.glass.as_ref().and_then(|m| m.targets.as_ref()).map(|t| &t.depth).filter(|_| glass);
        if pass.bind[g].is_none() {
            pass.bind[g] = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fog's beams"),
                layout: &pass.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: pass.globals.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&pass.lights.1) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(see_through.unwrap_or(opaque)) },
                ],
            }));
        }
        if pass.composite_bind[g].is_none() {
            pass.composite_bind[g] = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composite, fog"),
                layout: &pass.composite_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: pass.globals.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(opaque) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(beams) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(see_through.unwrap_or(opaque)) },
                ],
            }));
        }
        if let Some(made) = pass.glass.as_mut().filter(|_| glass)
            && made.far_bind.is_none()
        {
            let GlassTargets { depth, far, .. } = made.targets.as_ref().expect("made above");
            made.far_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fog behind glass"),
                layout: &made.far_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: pass.globals.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(opaque) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(depth) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(far) },
                ],
            }));
        }
        tiles.len() as u32
    }

    /// The see-through cubes' depth (`frame`'s, after the opaque ones), for
    /// a picture of fog with glass or water.
    pub(super) fn glass_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData) {
        let made = self.fog.as_ref().and_then(|f| f.glass.as_ref()).expect("made by write_fog");
        let depth = &made.targets.as_ref().expect("made by write_fog").depth;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("see-through depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("fog: glass")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (_, opaque) = frame.cube_runs();
        pass.set_pipeline(&made.depth_pipeline);
        pass.set_bind_group(0, &self.main_bind, &[]);
        pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.cube_index_count, 0, opaque..frame.instances.len() as u32);
    }

    /// The fog's beams into their target (`count` tiles; with `glass`, those
    /// behind the see-through cubes into a second one); the composite lays
    /// them over the picture.
    pub(super) fn beams_pass(&self, encoder: &mut wgpu::CommandEncoder, lf: LampFrame, count: u32, glass: bool) {
        let pass_of = self.fog.as_ref().expect("made by write_fog");
        let (_, _, target) = pass_of.target.as_ref().expect("made by write_fog");
        let clear = |view| Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } });
        let far = pass_of.glass.as_ref().and_then(|m| m.targets.as_ref()).map(|t| &t.far).filter(|_| glass);
        let targets = [clear(target), far.and_then(clear)];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fog's beams"),
            color_attachments: &targets[..1 + usize::from(far.is_some())],
            depth_stencil_attachment: None,
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("fog beams")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if count > 0 {
            let g = usize::from(glass);
            pass.set_pipeline(pass_of.beams[usize::from(lf.walls)][g][usize::from(pass_of.thin)].as_ref().expect("made by write_fog"));
            pass.set_bind_group(0, self.lamps.bind(), &[]);
            pass.set_bind_group(1, pass_of.bind[g].as_ref().expect("made by write_fog"), &[]);
            pass.set_vertex_buffer(0, pass_of.instances.slice(..));
            pass.draw(0..6, 0..count);
        }
    }

    /// The fog behind the see-through cubes laid over the scene, before they
    /// are drawn onto it: only where they can show (the screen's rectangle
    /// of their box; elsewhere it would change nothing).
    pub(super) fn far_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData) {
        let made = self.fog.as_ref().and_then(|f| f.glass.as_ref()).expect("made by write_fog");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fog behind glass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &self.targets.scene, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("fog: behind glass")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (_, opaque) = frame.cube_runs();
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for c in &frame.instances[opaque as usize..] {
            let [x, y, z, size] = c.pos_size;
            // (Half its edge, and as much again for a cube turned freely.)
            (lo, hi) = (lo.min(glam::Vec3::new(x, y, z) - size), hi.max(glam::Vec3::new(x, y, z) + size));
        }
        let (_, _, w, h) = self.picture();
        let Some([x0, y0, x1, y1]) = rect_of(&frame.view_proj, &super::Light::burst((lo + hi) * 0.5, ((hi - lo) * 0.5).max_element(), [0.0; 3])) else { return };
        let px = |v: f32, of: u32| (((v + 1.0) * 0.5 * of as f32).clamp(0.0, of as f32)) as u32;
        let (left, right, top, bottom) = (px(x0, w), px(x1, w) + 1, px(-y1, h), px(-y0, h) + 1);
        let (right, bottom) = (right.min(w), bottom.min(h));
        if right <= left || bottom <= top {
            return;
        }
        pass.set_scissor_rect(left, top, right - left, bottom - top);
        pass.set_pipeline(&made.far_pipeline);
        pass.set_bind_group(0, made.far_bind.as_ref().expect("made by write_fog"), &[]);
        pass.draw(0..3, 0..1);
    }
}

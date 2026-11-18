//! A frame drawn: the globals and buffers written, the shadow, scene, UI,
//! post and composite passes, and the GPU's time for each.

use std::cell::RefCell;
use std::sync::Arc;

use bytemuck::Zeroable;
use glam::{Mat4, Vec3};

use super::baked::{self, Lies};
use super::bank::{ACTOR_STRIDE, ActorData};
use super::frame::{UI_AMBIENT, UI_SUN};
use super::pipelines::{Memory, PostKind, PostPass};
use super::shader::{Globals, LightData, PostParams};
use super::lamps::LampFrame;
use super::pipelines::ScenePipelines;
use super::{FORWARD_LIGHTS, FrameData, Instance, ModelDraw, Renderer, Stats, off_grid, on_screen};

/// At most this many passes a frame are timed.
const MAX_TIMED_PASSES: u32 = 32;

/// How long the GPU takes for each render pass, from timestamp queries:
/// each pass writes one when it starts and one when it ends.
pub(super) struct PassTimes {
    pub(super) queries: wgpu::QuerySet,
    pub(super) resolve: wgpu::Buffer,
    pub(super) readback: wgpu::Buffer,
    /// Nanoseconds a timestamp tick.
    pub(super) period: f32,
    /// The passes of the frame being drawn, in order: pass i writes the
    /// queries 2i and 2i + 1.
    pub(super) labels: RefCell<Vec<&'static str>>,
}

impl PassTimes {
    /// Where pass `label` writes its timestamps (None once the set is full).
    pub(super) fn writes(&self, label: &'static str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
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

/// How a pass draws the models: its pipelines for their cubes and for
/// their baked faces; `whole`: a face draws its voxel's whole cube (the
/// outline's, from the voxel's first face); `cast`: into the sun's map
/// (`ModelDraw::in_pass`); `eye`: the world's eye, a run's faces turned
/// away from it left out.
struct ModelPass<'a> {
    cubes: &'a wgpu::RenderPipeline,
    faces: &'a wgpu::RenderPipeline,
    whole: bool,
    cast: bool,
    eye: Option<Vec3>,
}

/// A colour target of a pass, loaded as `load` says and stored.
fn color(view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store } })
}

impl Renderer {
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
            (self.main_bind, self.ui_bind) = Self::scene_binds(device, &self.main_layout, &self.globals, &self.ui_globals, &self.shadow_view, &self.shadow_sampler);
            if let Memory::Ready { layout, bind, faces, .. } = &mut self.memory {
                *bind = Self::memory_bind(device, layout, &self.globals, &self.shadow_view, &self.shadow_sampler, faces);
            }
        }
        // A game moving its sun: the pipelines with the faces' memory
        // (made the first time, where the GPU has them).
        let remember = frame.remember && self.remembering(device);
        self.pictures = self.pictures.wrapping_add(1);
        self.watch_memory(device, encoder);
        self.watch_lamps(device, encoder);
        // Lamps walls stop, or more lights than the scene's shader takes:
        // the lamps' pass (made the first time, the level's volume up to
        // date).
        let lamps = self.lamps_of(device, queue, frame, remember);
        let tiles = lamps.map_or(0, |lf| self.write_lamps(device, queue, frame, lf));
        // Fog: its beams drawn as the lamps' pass draws, after it; the
        // composite lays it over the picture. With see-through cubes in it
        // (glass, water) the fog behind them apart, before they are drawn.
        let glass = frame.fog.is_some() && (frame.cube_runs().1 as usize) < frame.instances.len();
        let fog = match (lamps, &frame.fog) {
            (Some(lf), Some(fog)) => Some(self.write_fog(device, queue, frame, fog, lf, glass)),
            _ => None,
        };
        self.write_globals(queue, frame, remember);
        self.upload_cubes(device, queue, frame);
        let all_models = frame.models.len() + frame.ui_models.len();
        self.upload_models(device, queue, frame, all_models);
        let instance_count = frame.instances.len();

        if let Some(t) = &self.pass_times {
            t.labels.borrow_mut().clear();
        }
        let mut stats = Stats {
            instances: instance_count,
            models: all_models,
            model_cubes: frame.models.iter().chain(&frame.ui_models).map(|m| m.voxels.len()).sum(),
            model_bytes: self.bank.as_ref().map_or(0, |b| b.bytes),
            ui_instances: frame.ui.len(),
            chunks: self.meshes.len(),
            vertices_total: self.meshes.values().map(|m| m.vertex_count).sum(),
            lamps: if lamps.is_some() { frame.lights.len().min(super::MOST_LIGHTS) } else { 0 },
            walls: lamps.is_some_and(|lf| lf.walls),
            volume_bytes: self.lamps.volume_bytes(),
            volume_ms: self.lamps.made_ms,
            fog: fog.is_some(),
            asked: if lamps.is_some_and(|lf| lf.walls) { self.lamps.asked } else { 0 },
            ..Stats::default()
        };
        self.shadow_pass(encoder, frame, all_models, &mut stats);
        match lamps {
            None => self.scene_pass(encoder, frame, all_models, remember, &mut stats),
            Some(lf) => {
                self.lit_scene_pass(encoder, frame, all_models, remember, lf, &mut stats);
                self.lamp_pass(encoder, lf, tiles);
                if let Some(count) = fog {
                    if glass {
                        self.glass_pass(encoder, frame);
                    }
                    self.beams_pass(encoder, lf, count, glass);
                    if glass {
                        self.far_pass(encoder, frame);
                    }
                }
                self.see_through_pass(encoder, frame, remember, fog.is_some());
            }
        }
        self.ui_pass(encoder, frame, all_models);
        self.post_passes(encoder, frame);
        self.composite_pass(encoder, target, fog.map(|_| glass));
        if let Some(t) = &self.pass_times {
            let count = 2 * t.labels.borrow().len() as u32;
            if count > 0 {
                encoder.resolve_query_set(&t.queries, 0..count, &t.resolve, 0);
                encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, count as u64 * 8);
            }
        }
        self.stats = stats;
    }

    /// The globals of the world's scene and the UI's, and the composite's
    /// numbers, for this frame.
    fn write_globals(&self, queue: &wgpu::Queue, frame: &FrameData, remember: bool) {
        // The nearest lights, as the scene's shader lights by them (all of
        // the picture's, unless the lamps' pass draws them; then the
        // see-through cubes'), through walls.
        let mut lights = [LightData::zeroed(); FORWARD_LIGHTS];
        for (slot, l) in lights.iter_mut().zip(&frame.lights) {
            *slot = LightData::of(l, false);
        }
        let light_count = frame.lights.len().min(FORWARD_LIGHTS);
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
            memory: [self.pictures, u32::from(remember), 0, 0],
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
            lights: [LightData::zeroed(); FORWARD_LIGHTS],
        };
        queue.write_buffer(&self.ui_globals, 0, bytemuck::bytes_of(&ui_globals));
        let composite = PostParams { params: [frame.glow, frame.tilt_shift, frame.screen_flash, 0.0], step: frame.fade };
        queue.write_buffer(&self.targets.composite_params, 0, bytemuck::bytes_of(&composite));
    }

    /// The frame's cubes into the instance buffer (grown if need be).
    fn upload_cubes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData) {
        // The world's cubes, then the UI's, in one buffer.
        let instance_count = frame.instances.len();
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
    }

    /// The model bank the first time it comes (and again after models
    /// reload), and every frame the places of its `all_models` draws: the
    /// world's, then the UI's.
    fn upload_models(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData, all_models: usize) {
        if all_models > 0 && !self.bank.as_ref().is_some_and(|b| Arc::ptr_eq(&b.bank, &frame.bank)) {
            let plan = baked::plan(&frame.bank, device.limits().max_buffer_size);
            if !self.told_bank && !plan.said.is_empty() {
                self.told_bank = true;
                self.said.extend(plan.said.iter().cloned());
            }
            // (The bank before let go first: both would lie on the GPU.)
            self.bank = None;
            self.bank = Some(baked::OnGpu::new(device, frame.bank.clone(), plan));
        }
        if all_models > self.actor_capacity {
            self.actor_capacity = all_models.next_power_of_two();
            (self.actors, self.actor_bind) = Self::actor_buffer(device, &self.actor_layout, self.actor_capacity);
        }
        if all_models > 0 {
            let places = &mut self.places;
            places.clear();
            places.resize(all_models * ACTOR_STRIDE, 0);
            for (m, place) in frame.models.iter().chain(&frame.ui_models).zip(places.as_chunks_mut::<ACTOR_STRIDE>().0) {
                let tint = m.tint.map_or([0.0; 4], |[r, g, b]| [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0]);
                let data = ActorData { origin: m.origin.extend(m.turn as f32).to_array(), tint, extra: [m.nearer, 0.0, 0.0, 0.0], rot: m.rot.to_array(), pivot: m.shift.extend(if off_grid(m.rot, m.shift) { 1.0 } else { 0.0 }).to_array() };
                place[..size_of::<ActorData>()].copy_from_slice(bytemuck::bytes_of(&data));
            }
            queue.write_buffer(&self.actors, 0, places);
        }
    }

    /// Each model draw as `how` says: its place (the `first` of all
    /// `all_models` places), and its run of the bank, from its cubes or its
    /// baked faces as it lies there (`baked::Lies`).
    fn draw_models(&self, pass: &mut wgpu::RenderPass, how: ModelPass, models: &[ModelDraw], first: usize, all_models: usize) {
        let Some(bank) = self.bank.as_ref().filter(|_| all_models > 0 && !models.is_empty()) else { return };
        // The buffers bound: none yet, the cubes', the faces'.
        let mut bound = None;
        for (i, m) in models.iter().enumerate() {
            if !m.in_pass(how.cast) {
                continue;
            }
            match (bank.lies(&m.voxels), &bank.cubes, &bank.faces) {
                (Lies::Cubes(at), Some(cubes), _) => {
                    if bound != Some(false) {
                        bound = Some(false);
                        pass.set_pipeline(how.cubes);
                        pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
                        pass.set_vertex_buffer(1, cubes.slice(..));
                        pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
                    }
                    pass.set_bind_group(1, &self.actor_bind, &[((first + i) * ACTOR_STRIDE) as u32]);
                    pass.draw_indexed(0..self.cube_index_count, 0, *at..at + m.voxels.len() as u32);
                }
                (Lies::Faces { dirs, lo, hi }, _, Some(faces)) => {
                    // A face's corners by the triangles of the first of
                    // the unit cube's faces (its four corners reused), or
                    // the whole cube's.
                    let (indices, count) = if how.whole { (&self.cube_indices, self.cube_index_count) } else { (&self.face_indices, 6) };
                    if bound != Some(true) {
                        bound = Some(true);
                        pass.set_pipeline(how.faces);
                        pass.set_vertex_buffer(0, faces.slice(..));
                        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    }
                    pass.set_bind_group(1, &self.actor_bind, &[((first + i) * ACTOR_STRIDE) as u32]);
                    let own = how.eye.map_or(63, |eye| baked::facing(m, *lo, *hi, eye));
                    for faces in baked::ranges(dirs, own) {
                        pass.draw_indexed(0..count, 0, faces);
                    }
                }
                _ => {}
            }
        }
    }

    /// The sun's shadow map: the chunks, the models, the opaque cubes.
    fn shadow_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData, all_models: usize, stats: &mut Stats) {
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
        let shadows = ModelPass { cubes: &self.model_shadow, faces: &self.face_shadow, whole: false, cast: true, eye: None };
        self.draw_models(&mut pass, shadows, &frame.models, 0, all_models);
        // Only the opaque cubes: fire, smoke, dust, glass and what lies
        // faint behind everything cast no shadow.
        let casting = frame.shadow_cubes();
        if !casting.is_empty() {
            pass.set_pipeline(&self.inst_shadow);
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.cube_index_count, 0, casting);
        }
    }

    /// The world's scene: what lies faint behind, the chunks, the models
    /// (their outline), the cubes, opaque then see-through; with the faces'
    /// memory where `remember`.
    fn scene_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData, all_models: usize, remember: bool, stats: &mut Stats) {
        let [r, g, b] = frame.sky.map(f64::from);
        let t = &self.targets;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[color(&t.scene, wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 })), color(&t.glow, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT))],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("scene")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (scene, bind) = self.scene_of(remember);
        pass.set_bind_group(0, bind, &[]);
        self.draw_opaque(&mut pass, frame, scene, (&self.model_outline, &self.face_outline), all_models, stats);
        self.draw_see_through(&mut pass, frame, scene);
    }

    /// The scene's own pipelines and what its main pass reads: with the
    /// faces' memory where `remember`.
    fn scene_of(&self, remember: bool) -> (&ScenePipelines, &wgpu::BindGroup) {
        match &self.memory {
            Memory::Ready { scene, bind, .. } if remember => (scene, bind),
            _ => (&self.plain, &self.main_bind),
        }
    }

    /// The opaque scene of a picture whose lights the lamps' pass draws
    /// (`lamps`): as `scene_pass` without the lights and the see-through
    /// cubes, each pixel's surface written beside its colour, the depth
    /// kept for the lamps and the see-through cubes after them.
    fn lit_scene_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData, all_models: usize, remember: bool, lf: LampFrame, stats: &mut Stats) {
        let [r, g, b] = frame.sky.map(f64::from);
        let (t, lt) = (&self.targets, self.lamps.targets());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[
                color(&t.scene, wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 })),
                color(&t.glow, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)),
                color(&lt.surface, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &lt.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("scene")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let ls = self.lamps.scenes(lf.shader);
        pass.set_bind_group(0, self.scene_of(remember).1, &[]);
        self.draw_opaque(&mut pass, frame, &ls.scene, (&ls.model_outline, &ls.face_outline), all_models, stats);
    }

    /// The see-through cubes after the lamps' pass, onto the scene it lit,
    /// by the scene's own pipelines; the depth kept for the fog after them
    /// where `keep`.
    fn see_through_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData, remember: bool, keep: bool) {
        let (_, opaque) = frame.cube_runs();
        if opaque as usize >= frame.instances.len() {
            return;
        }
        let (t, lt) = (&self.targets, self.lamps.targets());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("see-through"),
            color_attachments: &[color(&t.scene, wgpu::LoadOp::Load), color(&t.glow, wgpu::LoadOp::Load)],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &lt.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: if keep { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard } }),
                stencil_ops: None,
            }),
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("see-through")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (scene, bind) = self.scene_of(remember);
        pass.set_bind_group(0, bind, &[]);
        self.draw_see_through(&mut pass, frame, scene);
    }

    /// The scene's opaque part as `scene` draws it, with the models'
    /// outline as `outline` (cubes, faces): what lies faint behind, the
    /// chunks, the models, the opaque cubes.
    fn draw_opaque(&self, pass: &mut wgpu::RenderPass, frame: &FrameData, scene: &ScenePipelines, outline: (&wgpu::RenderPipeline, &wgpu::RenderPipeline), all_models: usize, stats: &mut Stats) {
        let instance_count = frame.instances.len();
        let (behind, opaque) = frame.cube_runs();
        // What lies behind everything else, faint: first.
        if behind > 0 {
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&scene.see_through);
            pass.draw_indexed(0..self.cube_index_count, 0, 0..behind);
        }
        pass.set_pipeline(&scene.chunk);
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
        let models = ModelPass { cubes: &scene.model, faces: &scene.faces, whole: false, cast: false, eye: Some(frame.eye) };
        self.draw_models(pass, models, &frame.models, 0, all_models);
        if frame.outline[3] > 0.0 {
            let outline = ModelPass { cubes: outline.0, faces: outline.1, whole: true, cast: false, eye: None };
            self.draw_models(pass, outline, &frame.models, 0, all_models);
        }
        // (The last `cast_only` opaque cubes are the sun's map's alone.)
        let drawn = opaque - (frame.cast_only as u32).min(opaque - behind);
        if instance_count > 0 && drawn > behind {
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&scene.inst);
            pass.draw_indexed(0..self.cube_index_count, 0, behind..drawn);
        }
    }

    /// The see-through cubes, far to near, as `scene` draws them.
    fn draw_see_through(&self, pass: &mut wgpu::RenderPass, frame: &FrameData, scene: &ScenePipelines) {
        let instance_count = frame.instances.len();
        let (_, opaque) = frame.cube_runs();
        if (opaque as usize) < instance_count {
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&scene.see_through);
            pass.draw_indexed(0..self.cube_index_count, 0, opaque..instance_count as u32);
        }
    }

    /// The UI's scene into its own layer (and the glow).
    fn ui_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData, all_models: usize) {
        let instance_count = frame.instances.len();
        let total = instance_count + frame.ui.len();
        // The UI: its own layer (cleared every frame) and the glow.
        let t = &self.targets;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[color(&t.ui, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)), color(&t.glow, wgpu::LoadOp::Load)],
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
        let models = ModelPass { cubes: &self.plain.model, faces: &self.plain.faces, whole: false, cast: false, eye: None };
        self.draw_models(&mut pass, models, &frame.ui_models, frame.models.len(), all_models);
        if !frame.ui.is_empty() {
            pass.set_pipeline(&self.plain.inst);
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.cube_index_count, 0, instance_count as u32..total as u32);
        }
    }

    /// The scene and the glow shrunk and blurred: each only where the
    /// composite weighs it (tilt-shift, glow), else what its small target
    /// holds from before is multiplied by 0 there.
    fn post_passes(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData) {
        let t = &self.targets;
        let scene = if frame.tilt_shift != 0.0 { &t.scene_blur[..] } else { &[] };
        let glow = if frame.glow != 0.0 { &t.glow_blur[..] } else { &[] };
        for PostPass { kind, source, dest } in scene.iter().chain(glow) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[color(&t.small[*dest], wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT))],
                depth_stencil_attachment: None,
                timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("blur")),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(match kind {
                PostKind::Down => &self.post_down,
                PostKind::Blur => &self.post_blur,
            });
            pass.set_bind_group(0, source, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// The picture put together on `target`, with the fog laid over its
    /// scene where `fogged` (`fog`; true: before see-through cubes).
    fn composite_pass(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, fogged: Option<bool>) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite"),
            color_attachments: &[color(target, wgpu::LoadOp::Clear(wgpu::Color::BLACK))],
            depth_stencil_attachment: None,
            timestamp_writes: self.pass_times.as_ref().and_then(|t| t.writes("composite")),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // A picture of its own shape in its rectangle; the rest stays
        // the black it was cleared to.
        if self.shape.is_some() {
            let (x, y, w, h) = self.picture();
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
        }
        match self.fog.as_ref().zip(fogged) {
            Some((fog, glass)) => {
                let g = usize::from(glass);
                pass.set_pipeline(&fog.composite[g]);
                pass.set_bind_group(2, fog.composite_bind[g].as_ref().expect("made by write_fog"), &[]);
            }
            None => pass.set_pipeline(&self.post_composite),
        }
        pass.set_bind_group(0, &self.targets.composite, &[]);
        pass.set_bind_group(1, &self.targets.blurred, &[]);
        pass.draw(0..3, 0..1);
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

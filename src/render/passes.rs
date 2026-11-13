//! A frame drawn: the globals and buffers written, the shadow, scene, UI,
//! post and composite passes, and the GPU's time for each.

use std::cell::RefCell;
use std::sync::Arc;

use bytemuck::Zeroable;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use super::bank::{ACTOR_STRIDE, ActorData};
use super::frame::{UI_AMBIENT, UI_SUN};
use super::pipelines::{Memory, PostPass};
use super::shader::{Globals, LightData, PostParams};
use super::{FrameData, Instance, MAX_LIGHTS, ModelDraw, Renderer, Stats, off_grid, on_screen};

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
        self.write_globals(queue, frame, remember);
        self.upload_cubes(device, queue, frame);
        self.upload_models(device, queue, frame);
        let instance_count = frame.instances.len();
        let all_models = frame.models.len() + frame.ui_models.len();

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
        self.shadow_pass(encoder, frame, &mut stats);
        self.scene_pass(encoder, frame, remember, &mut stats);
        self.ui_pass(encoder, frame);
        self.post_passes(encoder);
        self.composite_pass(encoder, target);
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
            lights: [LightData::zeroed(); MAX_LIGHTS],
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

    /// The model bank, and the model draws' places.
    fn upload_models(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData) {
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
    }

    /// Each model draw: its place (the `first` of all `all_models`
    /// places), and its voxels in the bank; into the sun's map (`cast`)
    /// those only casting too (`ModelDraw::shadow_only`), else not.
    fn draw_models(&self, pass: &mut wgpu::RenderPass, pipeline: &wgpu::RenderPipeline, models: &[ModelDraw], first: usize, all_models: usize, cast: bool) {
        let Some((_, bank)) = self.bank.as_ref().filter(|_| all_models > 0 && !models.is_empty()) else { return };
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
        pass.set_vertex_buffer(1, bank.slice(..));
        pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
        for (i, m) in models.iter().enumerate() {
            if !m.in_pass(cast) {
                continue;
            }
            pass.set_bind_group(1, &self.actor_bind, &[((first + i) * ACTOR_STRIDE) as u32]);
            pass.draw_indexed(0..self.cube_index_count, 0, m.voxels.clone());
        }
    }

    /// The sun's shadow map: the chunks, the models, the opaque cubes.
    fn shadow_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData, stats: &mut Stats) {
        let all_models = frame.models.len() + frame.ui_models.len();
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
        self.draw_models(&mut pass, &self.model_shadow, &frame.models, 0, all_models, true);
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
    fn scene_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData, remember: bool, stats: &mut Stats) {
        let all_models = frame.models.len() + frame.ui_models.len();
        let instance_count = frame.instances.len();
        let (behind, opaque) = frame.cube_runs();
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
        let (scene, bind) = match &self.memory {
            Memory::Ready { scene, bind, .. } if remember => (scene, bind),
            _ => (&self.plain, &self.main_bind),
        };
        pass.set_bind_group(0, bind, &[]);
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
        self.draw_models(&mut pass, &scene.model, &frame.models, 0, all_models, false);
        if frame.outline[3] > 0.0 {
            self.draw_models(&mut pass, &self.model_outline, &frame.models, 0, all_models, false);
        }
        if instance_count > 0 {
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            // (The last `cast_only` opaque cubes are the sun's map's alone.)
            let drawn = opaque - (frame.cast_only as u32).min(opaque - behind);
            if drawn > behind {
                pass.set_pipeline(&scene.inst);
                pass.draw_indexed(0..self.cube_index_count, 0, behind..drawn);
            }
            if (opaque as usize) < instance_count {
                pass.set_pipeline(&scene.see_through);
                pass.draw_indexed(0..self.cube_index_count, 0, opaque..instance_count as u32);
            }
        }
    }

    /// The UI's scene into its own layer (and the glow).
    fn ui_pass(&self, encoder: &mut wgpu::CommandEncoder, frame: &FrameData) {
        let all_models = frame.models.len() + frame.ui_models.len();
        let instance_count = frame.instances.len();
        let total = instance_count + frame.ui.len();
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
        self.draw_models(&mut pass, &self.plain.model, &frame.ui_models, frame.models.len(), all_models, false);
        if !frame.ui.is_empty() {
            pass.set_pipeline(&self.plain.inst);
            pass.set_vertex_buffer(0, self.cube_vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
            pass.set_index_buffer(self.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.cube_index_count, 0, instance_count as u32..total as u32);
        }
    }

    /// The scene and the glow shrunk and blurred.
    fn post_passes(&self, encoder: &mut wgpu::CommandEncoder) {
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
    }

    /// The picture put together on `target`.
    fn composite_pass(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
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
        // A picture of its own shape in its rectangle; the rest stays
        // the black it was cleared to.
        if self.shape.is_some() {
            let (x, y, w, h) = self.picture();
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
        }
        pass.set_pipeline(&self.post_composite);
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

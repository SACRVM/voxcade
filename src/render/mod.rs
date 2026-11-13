//! wgpu renderer. A sun shadow pass; the scene (chunk meshes, opaque
//! instanced cubes, then see-through ones far to near) into a colour target
//! and a glow target; both shrunk to a quarter and blurred; and the final
//! picture put together on screen: glow on top, blur towards the top and
//! bottom of the screen, a white flash after big blasts.

mod bank;
mod chunks;
mod cubes;
mod frame;
mod passes;
mod pipelines;
mod shader;

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3, Vec4};

use crate::mesh::Vertex;
use chunks::ChunkMesh;
use passes::PassTimes;
use pipelines::{Memory, ScenePipelines, Targets};

pub use bank::{ModelBank, ModelDraw, off_grid, quarter, turn_about, turn_centre, turned_centre};
pub use cubes::{SeeThroughBody, hide_faces_against, hide_inner_faces, join_by_body, join_see_through, leave_out_unseen, see_through_body, see_through_side};
pub use frame::{FrameData, Stats};

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
/// The turn the shader turns a cube by when it is given `snorm_turn(rot)`
/// (read back, normalized): cubes placed by it meet their neighbours
/// corner to corner (placed by `rot` itself, the packed turn's error left
/// single pixels open between them).
pub fn turn_as_drawn(rot: Quat) -> Quat {
    let s = snorm_turn(rot);
    if s == [0; 4] {
        return Quat::IDENTITY;
    }
    Quat::from_array(s.map(|c| c as f32 / 127.0)).normalize()
}

pub fn snorm_turn(rot: Quat) -> [i8; 4] {
    if rot == Quat::IDENTITY {
        return [0; 4];
    }
    rot.normalize().to_array().map(|c| (c.clamp(-1.0, 1.0) * 127.0).round() as i8)
}

/// A light from a flash or an explosion.
#[derive(Clone, Copy)]
pub struct Light {
    pub pos: Vec3,
    pub radius: f32,
    /// Colour times brightness.
    pub color: [f32; 3],
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
    /// The faces' memory of their shadow (`FrameData::remember`), made the
    /// first time a frame asks, and the pictures drawn, to tell the faces
    /// seen lately.
    memory: Memory,
    pictures: u32,
    main_layout: wgpu::BindGroupLayout,
    /// The scene's pipelines without the memory (every game's, and the
    /// UI's).
    plain: ScenePipelines,
    chunk_shadow: wgpu::RenderPipeline,
    inst_shadow: wgpu::RenderPipeline,
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
    /// The window's (or the texture's) size in pixels, and the picture's
    /// own shape in it, if it has one (`set_shape`): the targets are the
    /// picture's size then.
    window: (u32, u32),
    shape: Option<f32>,
}

/// Where a picture of the shape `shape` (width over height) lies in a
/// window `width` by `height` pixels: the largest rectangle of that shape
/// it holds, in its middle, as x, y, width and height in whole pixels
/// (letterboxed or pillarboxed; the bars are the rest).
pub fn letterbox(width: u32, height: u32, shape: f32) -> (u32, u32, u32, u32) {
    let (w, h) = (width.max(1), height.max(1));
    if w as f32 / h as f32 > shape {
        let across = ((h as f32 * shape).round() as u32).clamp(1, w);
        ((w - across) / 2, 0, across, h)
    } else {
        let high = ((w as f32 / shape).round() as u32).clamp(1, h);
        (0, (h - high) / 2, w, high)
    }
}

/// The format to draw a window's pictures in, of those its surface offers:
/// one that stores what it is given (the lighting happens in palette
/// space, like the old machines), else the first, which is sRGB, and the
/// composite decodes once for it (`composite_entry`). None when it offers
/// none.
pub fn surface_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    formats.iter().copied().find(|f| !f.is_srgb()).or_else(|| formats.first().copied())
}

/// The composite's entry point for a target of `format`: an sRGB target
/// encodes what it is given, so its picture is decoded first; else the
/// picture as it is.
pub fn composite_entry(format: wgpu::TextureFormat) -> &'static str {
    if format.is_srgb() { "fs_composite_srgb" } else { "fs_composite" }
}

impl Renderer {
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.window = (width, height);
        let (_, _, w, h) = self.picture();
        self.targets = Self::targets(device, &self.post_layout, &self.blurred_layout, &self.linear, w, h);
    }

    /// The picture's own shape, width over height (`c.camera.aspect`), or
    /// None for the window's: the frames drawn from now on fill the
    /// largest rectangle of that shape in the window, black bars on the
    /// other two sides. Their camera and HUD must be made for that shape.
    pub fn set_shape(&mut self, device: &wgpu::Device, shape: Option<f32>) {
        if self.shape != shape {
            self.shape = shape;
            let (width, height) = self.window;
            self.resize(device, width, height);
        }
    }

    /// Where the picture lies in the window: x, y, width, height.
    fn picture(&self) -> (u32, u32, u32, u32) {
        let (width, height) = self.window;
        match self.shape {
            Some(shape) => letterbox(width, height, shape),
            None => (0, 0, width, height),
        }
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
    use super::{composite_entry, surface_format};

    /// A window draws in a format that stores what it is given where its
    /// surface offers one, whatever comes first; a surface of sRGB only is
    /// drawn in sRGB with the composite that decodes once, never the
    /// palette space's colours encoded a second time (too bright).
    #[test]
    fn the_surface_format_keeps_the_palettes_colours() {
        use wgpu::TextureFormat::*;
        assert_eq!(surface_format(&[Bgra8UnormSrgb, Bgra8Unorm, Rgba16Float]), Some(Bgra8Unorm));
        assert_eq!(surface_format(&[Rgba8Unorm]), Some(Rgba8Unorm));
        assert_eq!(surface_format(&[Bgra8UnormSrgb, Rgba8UnormSrgb]), Some(Bgra8UnormSrgb));
        assert_eq!(surface_format(&[]), None);
        assert_eq!(composite_entry(Bgra8Unorm), "fs_composite");
        assert_eq!(composite_entry(Rgba8Unorm), "fs_composite");
        assert_eq!(composite_entry(Bgra8UnormSrgb), "fs_composite_srgb");
        assert_eq!(composite_entry(Rgba8UnormSrgb), "fs_composite_srgb");
    }
}

//! What the renderer is given for one frame, and what it counts of it.

use std::ops::Range;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::{Instance, Light, ModelBank, ModelDraw};

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
    /// The first `behind` of `instances`: see-through cubes drawn before
    /// everything else, so that whatever is drawn after them lies over them
    /// (the room round a model shaped in place never tints the model).
    pub behind: usize,
    /// The UI's voxels, in their own scene over the world, and its camera.
    pub ui: Vec<Instance>,
    pub ui_view_proj: Mat4,
}

impl FrameData {
    /// The cubes in their runs (`instances`): those drawn faint behind
    /// everything end at the first number, the opaque ones at the second,
    /// the see-through ones after.
    pub fn cube_runs(&self) -> (u32, u32) {
        let count = self.instances.len();
        let behind = self.behind.min(count);
        (behind as u32, (count - self.see_through.min(count)).max(behind) as u32)
    }

    /// The cubes the sun's shadow map takes: the opaque ones. Those behind
    /// everything are see-through (the room round a model shaped in place,
    /// faint, its glass too) and cast no shadow, as no see-through cube
    /// does.
    pub fn shadow_cubes(&self) -> Range<u32> {
        let (behind, opaque) = self.cube_runs();
        behind..opaque
    }
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
pub(super) const UI_SUN: Vec3 = Vec3::new(0.45, -0.6, -1.0);
pub(super) const UI_AMBIENT: f32 = 0.55;

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

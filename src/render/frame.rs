//! What the renderer is given for one frame, and what it counts of it.

use std::ops::Range;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::{Instance, Light, ModelBank, ModelDraw};

/// A moving thing as the lamps whose shadows are "all" take it
/// (`FrameData::lamp_movers`): its key (`Drawing::of_thing`) and its
/// draws.
pub type Mover = (Option<u32>, Vec<ModelDraw>);

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
    /// The picture's lights, the nearest the middle of the view first (at
    /// most `MOST_LIGHTS`).
    pub lights: Vec<Light>,
    /// What stops its lamps (`Light::shadows`), given when one has to be
    /// stopped: the level's world as the picture has it (a copy sharing its
    /// chunks) and its pieces' draws, every one in view or not
    /// (`ModelDraw`s of the bank). None: no lamp is stopped (the editors'
    /// views, a game without lamps).
    pub lamp_world: Option<crate::world::World>,
    pub lamp_pieces: Vec<ModelDraw>,
    /// What moves and stops the lamps whose shadows are "all"
    /// (`render::movers`): the models drawn that are not the level's pieces,
    /// in view or not, within such a lamp's reach, each thing's draws
    /// together with the thing's key (`Drawing::of_thing`; a light of it,
    /// `Light::of`, is not stopped by them; the opaque cubes are taken from
    /// `instances`). Empty unless such a lamp is in the picture.
    pub lamp_movers: Vec<Mover>,
    /// The air's fog, if the picture has any (`Fog`): drawn by the lamps'
    /// pass's picture (`render::fog`). None: no fog.
    pub fog: Option<super::Fog>,
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
    /// Models drawn whole on the HUD (`frame:draw` turned freely or posed),
    /// in a pass of their own over the UI's layer, lit by the UI's light
    /// and seen from its eye (`ui::eye`; the UI's own scene takes one far
    /// off).
    pub hud_models: Vec<ModelDraw>,
    /// The HUD's voxels drawn before the first of `hud_models`, in a pass
    /// of their own over them as the UI's are drawn (empty without
    /// `hud_models`: then they are all in `ui`).
    pub hud_front: Vec<Instance>,
    pub hud_eye: Vec3,
    pub bank: Arc<ModelBank>,
    /// Opaque cubes first, then the last `see_through` ones, sorted far to near.
    pub instances: Vec<Instance>,
    pub see_through: usize,
    /// The first `behind` of `instances`: see-through cubes drawn before
    /// everything else, so that whatever is drawn after them lies over them
    /// (the room round a model shaped in place never tints the model).
    pub behind: usize,
    /// The last `cast_only` of the opaque cubes are drawn into the sun's
    /// shadow map only, never into the scene (the level editor's
    /// `shadow_only` pieces: casting while shown as ghosts or not at all).
    pub cast_only: usize,
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

    /// The cubes the sun's shadow map takes: the opaque ones (the
    /// `cast_only` ones among them). Those behind
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
    /// Models drawn from the bank, and their cubes; what the bank takes on
    /// the GPU, in bytes (its cubes and its baked faces).
    pub models: usize,
    pub model_cubes: usize,
    pub model_bytes: u64,
    pub ui_instances: usize,
    pub chunks: usize,
    pub drawn: usize,
    /// Chunks drawn into the shadow map.
    pub shadowed: usize,
    /// Vertices of every chunk mesh, drawn or not (what sits in GPU memory).
    pub vertices_total: usize,
    pub vertices: usize,
    /// Lights drawn by the lamps' pass (0: by the scene's shader alone),
    /// whether walls stopped its lamps, the level's volume's bytes
    /// on the GPU, how long it took to make, ms, and how many times it was
    /// made, and the bytes of the faces' memory (`render::lamps`).
    pub lamps: usize,
    pub walls: bool,
    pub volume_bytes: u64,
    pub volume_ms: f64,
    pub volumes_made: u32,
    pub memory_bytes: u64,
    /// Whether the picture had fog (`render::fog`), and whether its beams
    /// took the thin things' finer points (`Fog::thin`, a lamp lighting
    /// one).
    pub fog: bool,
    pub thin: bool,
    /// Lamps walls stop that were new to the faces' memory, asked for
    /// first (`render::lamps::ASK`).
    pub asked: usize,
}

//! wgpu renderer. A sun shadow pass; the scene (chunk meshes, opaque
//! instanced cubes, then see-through ones far to near) into a colour target
//! and a glow target; both shrunk to a quarter and blurred; and the final
//! picture put together on screen: glow on top, blur towards the top and
//! bottom of the screen, a white flash after big blasts.

mod baked;
mod bank;
mod chunks;
mod cubes;
mod fog;
mod frame;
mod kept;
mod lamps;
mod limits;
mod movers;
mod passes;
mod pipelines;
mod shader;
mod thin;
mod volume;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3, Vec4};

use crate::mesh::Vertex;
use chunks::ChunkMesh;
use passes::PassTimes;
use pipelines::{Memory, ScenePipelines, Targets};

pub use baked::Baked;
pub use bank::{ModelBank, ModelDraw, glow_byte, off_grid, quarter, turn_about, turn_centre, turned_centre};
pub use cubes::{SeeThroughBody, hide_faces_against, hide_inner_faces, join_by_body, join_see_through, leave_out_unseen, see_through_body, see_through_side};
pub use frame::{FrameData, Mover, Stats};
#[cfg(test)]
pub use lamps::{LET_GO, REFUSED};
pub use shader::{METAL_SHIFT, METAL_STEPS};

/// Lights a picture lights by the scene's own shader (the shader's
/// MAX_LIGHTS, the nearest the middle of the view): every light of a
/// picture with no more and none stopped by walls, and the see-through
/// cubes' in any (glass, water). Past them, or with a lamp walls stop, the
/// lights are drawn in a pass of their own (`lamps`), up to `MOST_LIGHTS`.
pub const FORWARD_LIGHTS: usize = 16;
/// The most lights a picture lights, the nearest the middle of the view
/// (the rest said once, `Session::warnings`): a bound of the light pass's
/// buffer, far past any picture's need.
pub const MOST_LIGHTS: usize = 4096;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The scene and glow targets: room above 1 for glow, filterable for blur.
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The blurred copies are this many times smaller on each side.
const SHRINK: u32 = 4;

/// The lanes of `Instance::fx`; the shader has them by these names too
/// (`shader::prelude` writes them ahead of its text).
pub const FX_GLOW: usize = 0;
pub const FX_OPACITY: usize = 1;
pub const FX_COVERED: usize = 2;
pub const FX_METAL: usize = 3;

/// One moving voxel: player parts, bullets, debris, sparks, fire, smoke.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    /// Centre in xyz, edge length in w.
    pub pos_size: [f32; 4],
    pub color: [u8; 4],
    /// Four lanes, by `FX_GLOW`, `FX_OPACITY`, `FX_COVERED`, `FX_METAL`:
    /// how much it glows and how opaque it is (0..255 each), the faces a
    /// neighbouring cube covers (bits in `mesh::DIRS` order; see
    /// `hide_inner_faces`), and its metal (`vox::metal::pack`).
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

/// A light lighting what is round it in one picture: a flash or an
/// explosion's (`burst`: round, the falloff 2, through walls), or a
/// lamp a level or the game sets (`lamp`, shaped by `LightShape`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub pos: Vec3,
    pub radius: f32,
    /// Colour times brightness.
    pub color: [f32; 3],
    /// How it fades out to its radius: (1 - distance / radius) to this
    /// power (2 as lights always did; lower reaches further bright).
    pub falloff: f32,
    /// A spot's way and cone; None: it shines every way.
    pub spot: Option<Spot>,
    /// What stops it (`lamps`): None, the light leaves it to the game's
    /// word (`c.light.lamp_shadows`); the picture's lights are all
    /// stated once the game's word is laid over them (`Light::stated`).
    pub shadows: Option<LampShadows>,
    /// How strongly its light shows in the picture's fog (`Fog`): 1 as
    /// bright as it lights, 0 not at all.
    pub fog: f32,
    /// The thing it shines from (`frame:light`'s `at` a thing, by
    /// `Drawing::of_thing`'s key): none of that thing's own voxels stop it,
    /// as a piece's own voxels stop none of its lights (`render::movers`).
    pub of: Option<u32>,
}

/// A spot's way and cone, as the shader takes them: the way it shines
/// (unit), the cosine of half its cone and the cosine within which its
/// edge is no longer soft (`LightShape::blend`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub way: Vec3,
    pub outer: f32,
    pub inner: f32,
}

impl Light {
    /// A flash's or an explosion's light (a kit's): round, falling off as
    /// lights always did, through walls.
    pub fn burst(pos: Vec3, radius: f32, color: [f32; 3]) -> Self {
        Self { pos, radius, color, falloff: 2.0, spot: None, shadows: Some(LampShadows::None), fog: 1.0, of: None }
    }

    /// A lamp (a level's light, `frame:light`) shaped as `shape` says, its
    /// way turned by `turn` (a piece's lights turn with the piece; the
    /// identity for the level's own ways).
    pub fn lamp(pos: Vec3, radius: f32, color: [f32; 3], shape: &LightShape, turn: impl Fn(Vec3) -> Vec3) -> Self {
        let spot = shape.cone.filter(|c| *c < 360.0).map(|cone| {
            let half = (cone * 0.5).to_radians();
            Spot { way: turn(shape.way()).normalize_or(Vec3::Y), outer: crate::sim::math::cos(half), inner: crate::sim::math::cos(half * (1.0 - shape.blend)) }
        });
        Self { pos, radius, color, falloff: shape.falloff, spot, shadows: shape.shadows, fog: shape.fog, of: None }
    }

    /// Whether the level's voxels and pieces stop it, `game` the game's
    /// word for a light that states none.
    pub fn stopped_by(&self, game: LampShadows) -> bool {
        self.shadows.unwrap_or(game).stops_walls()
    }

    /// The light with its word stated: its own, else the game's.
    pub fn stated(mut self, game: LampShadows) -> Self {
        self.shadows = Some(self.shadows.unwrap_or(game));
        self
    }

    /// Whether walls stop it, in a picture whose lights are stated.
    pub fn stopped(&self) -> bool {
        self.shadows.is_some_and(LampShadows::stops_walls)
    }

    /// Whether moving things stop it too (`"all"`), in a picture whose
    /// lights are stated.
    pub fn stopped_by_movers(&self) -> bool {
        self.shadows == Some(LampShadows::All)
    }

    /// The box of what it can light, low and high corner: its ball, a
    /// spot's cone within it (the apex and the cap of the ball the cone
    /// cuts: along each way of the axes as far as the cap reaches it), a
    /// voxel more each way.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let r = self.radius;
        let Some(s) = self.spot else { return (self.pos - r - 1.0, self.pos + r + 1.0) };
        let (cos, sin) = (s.outer.clamp(-1.0, 1.0), (1.0 - s.outer.clamp(-1.0, 1.0).powi(2)).sqrt());
        // How far the cap reaches along a unit way whose cosine with the
        // axis is `c`: all of the radius where the way lies in the cone,
        // else as far as the cone's nearest edge goes that way.
        let reach = |c: f32| if c >= cos { r } else { r * (c * cos + (1.0 - c * c).max(0.0).sqrt() * sin) };
        let way = s.way.to_array();
        let hi = Vec3::from_array(way.map(|a| reach(a).max(0.0)));
        let lo = Vec3::from_array(way.map(|a| reach(-a).max(0.0)));
        (self.pos - lo - 1.0, self.pos + hi + 1.0)
    }
}

/// How a lamp shines beyond where, how far and its colour: the words a
/// light is written with (a level's `light = { ... }`, `frame:light{ ...
/// }`), each with its default (`Default`: round, the falloff 2, its
/// shadows the game's word).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightShape {
    /// (1 - distance / radius) to this power: 2 unless set.
    pub falloff: f32,
    /// A spot's whole opening angle, degrees; None: every way.
    pub cone: Option<f32>,
    /// How much of the cone's edge is soft, 0 to 1 (0.15 unless set).
    pub blend: f32,
    /// The spot's way: degrees clockwise seen from above (north, +y, is
    /// 0), and up (above 0) or down.
    pub rotation: f32,
    pub pitch: f32,
    /// What stops it; None: the game's word (`c.light.lamp_shadows`,
    /// `"static"` unless it says), which a stated word beats.
    pub shadows: Option<LampShadows>,
    /// How strongly it shows in fog, 0 to 10 (1 unless set).
    pub fog: f32,
}

/// What stops a lamp's light, the words of a light's `shadows` and of
/// `c.light.lamp_shadows`: `"none"` (it shines through everything),
/// `"static"` (the level's solid voxels and its pieces) or `"all"` (moving
/// things too: figures, things, models drawn, cubes; `render::movers`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LampShadows {
    None,
    #[default]
    Static,
    All,
}

impl LampShadows {
    /// The words there are, as written.
    pub const WORDS: [&str; 3] = ["none", "static", "all"];

    /// The word as written in files and scripts.
    pub fn word(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Static => "static",
            Self::All => "all",
        }
    }

    /// What each word means, for a mistake in one.
    pub const MEANS: &str = "\"none\": it shines through walls, \"static\": the level's voxels and pieces stop it, \"all\": moving things too";

    /// A word read; the mistake names the words there are and what each
    /// means.
    pub fn read(word: &str) -> Result<Self, String> {
        match word {
            "none" => Ok(Self::None),
            "static" => Ok(Self::Static),
            "all" => Ok(Self::All),
            other => Err(format!("\"{other}\" is none of {} ({})", Self::list(), Self::MEANS)),
        }
    }

    /// The mistake for `true` or `false` (the words before 0.7 came out):
    /// what to write instead and the words there are.
    pub fn boolean(on: bool) -> String {
        format!("shadows takes words, not true or false: write \"{}\" for {on} (the words are {})", if on { "static" } else { "none" }, Self::list())
    }

    /// The words, quoted, for a mistake.
    pub fn list() -> String {
        Self::WORDS.iter().map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join(", ")
    }

    /// Whether the level's voxels and pieces stop the light.
    pub fn stops_walls(self) -> bool {
        self != Self::None
    }
}

impl<'de> serde::Deserialize<'de> for LampShadows {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Words;
        impl serde::de::Visitor<'_> for Words {
            type Value = LampShadows;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "one of {} ({})", LampShadows::list(), LampShadows::MEANS)
            }
            fn visit_bool<E: serde::de::Error>(self, on: bool) -> Result<LampShadows, E> {
                Err(E::custom(LampShadows::boolean(on)))
            }
            fn visit_str<E: serde::de::Error>(self, word: &str) -> Result<LampShadows, E> {
                LampShadows::read(word).map_err(E::custom)
            }
        }
        d.deserialize_any(Words)
    }
}

impl serde::Serialize for LampShadows {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.word())
    }
}

impl Default for LightShape {
    fn default() -> Self {
        Self { falloff: 2.0, cone: None, blend: 0.15, rotation: 0.0, pitch: 0.0, shadows: None, fog: 1.0 }
    }
}

/// Air that holds fog in a picture (`c.light.fog_density`, `frame:fog`):
/// within the level's box (`lo`, `hi`, voxels) the light of the lamps
/// shows in it as beams (stopped where walls stop the lamp), and what lies
/// behind fades towards its colour. Only the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    /// How much a voxel of air takes from the light passing through and
    /// sends on, above 0 (1 / the distance at which a third is left).
    pub density: f32,
    /// The fog's own colour, where no lamp lights it (0..1 each).
    pub color: [f32; 3],
    /// How much it sends light on ahead rather than back, -0.9 to 0.9
    /// (Henyey-Greenstein's g; 0 every way alike).
    pub anisotropy: f32,
    /// How brightly it shows the lamps' light (1: as the air would): what
    /// the fog sends on to the eye, times this; above 1 no brighter than
    /// the lamp's light along the ray (`fog_beams.wgsl`'s `beam`). The
    /// dimming, of the beams and what lies behind, is `density`'s alone.
    pub glow: f32,
    pub lo: Vec3,
    pub hi: Vec3,
    /// How many of the picture's pixels a pixel of its beams is each way
    /// (the player's FOG setting, `settings::Fog`).
    pub scale: u32,
    /// Whether the beams take the thin things' finer points (`render::thin`;
    /// FOG HIGH): without, their points lie on the planes alone, at no cost
    /// of thin things.
    pub thin: bool,
}

/// The words of a light beyond where, how far and its colour, and the
/// range of each number (`LightShape`): what the level's reader, the
/// editor and `frame:light` check alike.
pub const LIGHT_WORDS: [(&str, f64, f64); 6] = [("falloff", 0.0, 16.0), ("cone", 1.0, 360.0), ("blend", 0.0, 1.0), ("rotation", -360.0, 360.0), ("pitch", -90.0, 90.0), ("fog", 0.0, 10.0)];

impl LightShape {
    /// The way a spot shines, in the directions it is written in: north
    /// (+y) turned clockwise by `rotation`, raised by `pitch`.
    pub fn way(&self) -> Vec3 {
        let (sr, cr) = crate::sim::math::sin_cos(self.rotation.to_radians());
        let (sp, cp) = crate::sim::math::sin_cos(self.pitch.to_radians());
        Vec3::new(sr * cp, cr * cp, sp)
    }
}

pub struct Renderer {
    globals: wgpu::Buffer,
    shadow_bind: wgpu::BindGroup,
    main_bind: wgpu::BindGroup,
    ui_globals: wgpu::Buffer,
    ui_bind: wgpu::BindGroup,
    /// The UI's globals seen from its real eye, for the models drawn whole
    /// on the HUD (`FrameData::hud_eye`): their outline and their parts'
    /// depth go by it.
    hud_globals: wgpu::Buffer,
    hud_bind: wgpu::BindGroup,
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
    /// The same from the models' baked faces (`baked`).
    face_outline: wgpu::RenderPipeline,
    face_shadow: wgpu::RenderPipeline,
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
    /// The unit cube's first face's triangles, every baked face drawn by
    /// them (`vs_face`).
    face_indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    /// The model bank on the GPU, which bank it holds and where its runs
    /// lie.
    bank: Option<baked::OnGpu>,
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
    /// The model draws' places of a frame, kept from frame to frame.
    places: Vec<u8>,
    pub stats: Stats,
    /// GPU time per pass, when asked for (`time_passes`).
    pass_times: Option<PassTimes>,
    /// The window's (or the texture's) size in pixels, and the picture's
    /// own shape in it, if it has one (`set_shape`): the targets are the
    /// picture's size then.
    window: (u32, u32),
    shape: Option<f32>,
    /// Whether the faces' memory was said to be full, the model bank past
    /// what the GPU holds (`baked::plan`), and what is to be said
    /// (`limits`, `warnings`).
    told_full: bool,
    told_bank: bool,
    said: Vec<String>,
    /// The lamps' pass and what it reads (`lamps`), made when a picture
    /// first asks.
    lamps: lamps::Lamps,
    /// The fog's passes (`fog`), made when a picture first has fog.
    fog: Option<fog::FogPass>,
    /// The format the pictures are put together in (the window's).
    format: wgpu::TextureFormat,
    /// The levels of the location played kept built (`kept`).
    kept: kept::Kept,
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

/// The limits to ask of a device on `adapter`: wgpu's defaults, but a
/// storage buffer bound and a buffer as large as the lamps' memory may
/// grow (`lamps::MOST_BYTES`), as far as the adapter allows (asking past
/// its limits fails). Every device the renderer draws with is asked so.
pub fn device_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    let (most, d) = (adapter.limits(), wgpu::Limits::default());
    wgpu::Limits {
        max_storage_buffer_binding_size: d.max_storage_buffer_binding_size.max(lamps::MOST_BYTES.min(most.max_storage_buffer_binding_size)),
        max_buffer_size: d.max_buffer_size.max(lamps::MOST_BYTES.min(most.max_buffer_size)),
        ..d
    }
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
        self.lamps.resized();
        if let Some(fog) = &mut self.fog {
            fog.resized();
        }
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
    let clip: [Vec4; 8] = crate::camera::box_corners(min, max).map(|corner| *view_proj * corner.extend(1.0));
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

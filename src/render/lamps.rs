//! Lamps: lights walls stop, spots, a falloff of their own, and more than
//! `FORWARD_LIGHTS` lights a picture. A picture with a light walls stop
//! (`Light::shadows`, its level's picture given: `FrameData::lamp_world`)
//! or more lights than the scene's shader takes draws its lights in a pass
//! of their own (`lamps.wgsl`): the opaque scene first without them
//! (`fs_main_lamps`, each pixel's surface written beside its colour), then
//! the screen in tiles, each drawn with the lights whose box reaches it,
//! their light added onto the pixels there (each pixel's surface read once
//! a tile), then the see-through cubes (lit by the nearest
//! `FORWARD_LIGHTS` in the scene's shader, through walls). Every other
//! picture is drawn by the scene's shader alone: a game without lamps pays
//! nothing.
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
/// after them): a quarter of its pixels (a face of 2 by 2 pixels and one
/// lamp each), a power of two from 2^16 (512 KB): 2^19, 4 MB, at 1920 by
/// 1080. Every subject of `games/lights` fits 2^16 at 1920 by 1080, fog
/// HIGH or LOW. Made twice as large when found full
/// (`Renderer::too_small`): the fog's beams' points share it (a point and
/// the face in front of its voxel ask the same), and take what comes by the
/// voxels along the beams more than by the pixels (a level of 1600 by 900
/// voxels in fog with 16 lamps of radius 300 grows to 2^25 at 1920 by
/// 1080, FOG HIGH). Room for them up front, 16 a pixel of the beams'
/// target, would take 134 MB for a small hall.
pub(super) fn faces_for(w: u32, h: u32) -> u64 {
    (u64::from(w) * u64::from(h) / 4).next_power_of_two().max(1 << 16)
}

/// The answers a pixel up to which the lamps' memory grows when crowded
/// (`lamps.wgsl`'s GROW_AT; past it only when an answer found no room):
/// 2^24 at 1920 by 1080. A hall in fog with lamps of radius 40 to 150
/// grows to 2^23 or 2^24 and draws its beams as fast as with room to
/// spare; a level of 1600 by 900 voxels with 16 lamps of radius 300 in fog
/// finds it crowded at any size and stops at 2^25, where none lacks room.
const CROWDED_UP_TO: u64 = 8;

/// How often a window reads whether the lamps' memory was found full: every
/// this many pictures, so a scene that needs it larger has it within a
/// second or so (from 2^19 to 2^25, six times).
const GROWN_EVERY: u32 = 8;

/// The pictures in a row without the lamps' pass (no lamp walls stop, no
/// fog, at most `FORWARD_LIGHTS`) after which everything of the lamps is
/// let go (`Lamps::let_go`): ten seconds at 60 a second, so a lamp
/// flickering on and off keeps it.
pub const LET_GO: u32 = 600;

/// The most answers the lamps' memory holds on any GPU: 2^26, 512 MB. It
/// is taken only as pictures find a smaller one full, so a picture that
/// fits keeps its smaller memory; a level of 1600 by 900 voxels in fog
/// with nine lamps of radius 170 to 210 took 2^25 (256 MB) at 1920 by
/// 1080, FOG HIGH, and this leaves room for twice its pixels (wgpu's
/// default limits held 2^23, 64 MB).
const MOST_FACES: u64 = 1 << 26;

/// The bytes of the largest lamps' memory (`MOST_FACES`), what a device is
/// asked to bind and hold (`render::device_limits`).
pub(super) const MOST_BYTES: u64 = MOST_FACES * ANSWER_BYTES + COUNTS;

/// The words of an answer in the lamps' memory (`lamps.wgsl`'s `stopped`:
/// its key; whether stopped, when and its second key): the shaders'
/// LAMP_ANSWER_WORDS.
pub(super) const ANSWER_WORDS: u64 = 2;

/// An answer's bytes.
const ANSWER_BYTES: u64 = ANSWER_WORDS * 4;

/// The words after the lamps' memory's answers (the shaders'
/// LAMP_COUNT_WORDS): how many found no room within `lamps.wgsl`'s GROW_AT
/// slots (it grows), and how many within PROBES, none (said, where it
/// cannot grow).
pub(super) const COUNT_WORDS: u64 = 2;

/// Their bytes.
const COUNTS: u64 = COUNT_WORDS * 4;

/// The largest the lamps' memory may be on this GPU, answers (one buffer
/// bound whole, two words each and the counts), `MOST_FACES` at most.
fn most_faces(device: &wgpu::Device) -> u64 {
    let limits = device.limits();
    let bytes = limits.max_storage_buffer_binding_size.min(limits.max_buffer_size);
    (1 << ((bytes - COUNTS) / ANSWER_BYTES).ilog2()).min(MOST_FACES)
}

/// Where the counts lie in a memory for `faces` faces: first the crowded
/// one, then the one of those that found no room.
fn full_at(faces: u64) -> u64 {
    faces * ANSWER_BYTES
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
    /// The moving things (`movers`): x how many boxes (0: none).
    pub(super) movers: [i32; 4],
    /// What changed in the volume since the state before (`info.z`, 0:
    /// none kept), grown by `MOVED_MARGIN`: low and high corner in the
    /// level's voxels (`Lamps::walls_moved`).
    pub(super) moved_lo: [f32; 4],
    pub(super) moved_hi: [f32; 4],
}

/// How far round what changed in the volume an answer of the state before
/// is walked again (`lamps.wgsl`'s `clear_of_moved`), voxels each way: a
/// walk reads the cells of the copies holding its way's points and the
/// next cell ahead of one, each at most as big as the top copy's (2^5
/// voxels), and its face's RIM. Past this the cells it reads are as they
/// were, and so is its answer.
const MOVED_MARGIN: i32 = 2 * (1 << (LEVELS - 1)) + 2;

/// A light's row in the lamps' lights (`lamps.wgsl`'s `lights`): the light
/// as the shaders take it and, walls stopping it, where its walks begin
/// (`Volume::start`; w 1: buried, it lights nothing).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LampRow {
    pub(super) light: LightData,
    pub(super) from: [f32; 4],
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

// A tile's lights: after its rectangle and count, eight a vertex input
// (four words of two), as `lamps.wgsl`'s `vs_lamp` reads them (test
// `the_vertex_inputs_are_the_rust_structs`); a bit each in `count.y` for
// the fog's beams (`render::fog`).
const _: () = assert!(TILE_ATTRS.len() == 2 + LIGHTS_A_TILE / 8 && LIGHTS_A_TILE <= 32);

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
    /// faces' answers from before are walked again), and where it changed
    /// only in part (`walls_moved`) the state before and the box round
    /// what changed (`LampGlobals::moved_lo`, `moved_hi`).
    state: u32,
    moved: Option<(u32, [f32; 4], [f32; 4])>,
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
    /// How long the volume took to make last, ms (`shot` says it), and
    /// how many times it was made.
    pub(super) made_ms: f64,
    pub(super) volumes_made: u32,
    /// The moving things on the GPU, for lamps whose shadows are "all"
    /// (`movers`): how many boxes this picture (0: none); their solids and
    /// the solids' bits (`Movers::solids`, `bits`), each with the room it
    /// has, made larger when a picture needs more; what the bits hold
    /// (`Movers::key`).
    movers_shown: i32,
    movers_solids: (wgpu::Buffer, usize),
    movers_bits: (wgpu::Buffer, usize),
    movers_key: u64,
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
    /// Pictures in a row without the lamps' pass (`LET_GO`).
    idle: u32,
    /// What the GPU could not give (`made`), the most tried again: the
    /// faces' memory's answers, the volume's bytes, the moving things'
    /// bytes. Said once as it failed.
    memory_room: u64,
    volume_room: u64,
    movers_room: usize,
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

/// A storage buffer the shaders read of `bytes`, written from the CPU.
fn read_buffer(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: bytes as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

/// The moving things' solids' buffer, room for `count`.
fn solids_buffer(device: &wgpu::Device, count: usize) -> (wgpu::Buffer, usize) {
    (read_buffer(device, "the moving things' solids", count * size_of::<super::movers::Solid>()), count)
}

/// Their bits' buffer, room for `words`.
fn bits_buffer(device: &wgpu::Device, words: usize) -> (wgpu::Buffer, usize) {
    (read_buffer(device, "the moving things' voxels", words * 4), words)
}

/// What a level put aside built keeps of the lamps' pass (`Lamps::put_aside`).
pub(super) struct LampsKept {
    volume: Option<Held>,
    seen: Option<(u64, wgpu::Buffer)>,
    state: u32,
    known: std::collections::BTreeSet<[u32; 2]>,
}

impl LampsKept {
    /// The bytes it holds on the GPU.
    pub(super) fn bytes(&self) -> u64 {
        self.volume.as_ref().map_or(0, |h| h.texture.size().width as u64 * h.texture.size().height as u64 * h.texture.size().depth_or_array_layers as u64 * 8 / 7) + self.seen.as_ref().map_or(0, |(_, b)| b.size())
    }
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
            moved: None,
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
            volumes_made: 0,
            movers_shown: 0,
            movers_solids: solids_buffer(device, 64),
            movers_bits: bits_buffer(device, 1024),
            movers_key: 0,
            movers_boxes: read_buffer(device, "the moving things' boxes", super::movers::MOST_BOXES * 32),
            movers_lists: read_buffer(device, "the lamps' lists of boxes", super::movers::MOST_LISTED * 4),
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
            idle: 0,
            memory_room: u64::MAX,
            volume_room: u64::MAX,
            movers_room: usize::MAX,
        }
    }

    /// The largest the faces' memory may be: the GPU's largest buffer
    /// (`most_faces`), and less than it could not give.
    fn most(&self, device: &wgpu::Device) -> u64 {
        most_faces(device).min(self.memory_room)
    }

    /// A picture drawn with the lamps' pass (`used`) or without: after
    /// `LET_GO` in a row without, everything it holds let go (the level's
    /// volume and its thin things, the faces' memory and what it learnt it
    /// needs, the targets, the moving things' room); the pipelines stay.
    /// Whether it was let go now.
    pub(super) fn used(&mut self, used: bool, device: &wgpu::Device) -> bool {
        self.idle = if used { 0 } else { self.idle.saturating_add(1) };
        if self.idle != LET_GO {
            return false;
        }
        (self.targets, self.seen, self.grown, self.bind, self.volume, self.ask_target) = (None, None, 0, None, None, None);
        self.walls_changed();
        self.movers_solids = solids_buffer(device, 64);
        self.movers_bits = bits_buffer(device, 1024);
        self.movers_key = 0;
        true
    }

    /// The volume's state moved on (`state`; the faces' answers from
    /// before walked again): no lamp is known to the memory now, every one
    /// asked for first (`ASK`).
    fn walls_changed(&mut self) {
        self.state = next_state(self.state);
        self.moved = None;
        self.known.clear();
    }

    /// The volume's voxels changed within the box `lo` to `hi` only (the
    /// level's voxels, `Volume::moved`): the state moves on, and an answer
    /// of the state before whose way passes no nearer it than
    /// `MOVED_MARGIN` is kept (a figure posed anew every picture walks
    /// only the ways near it again).
    fn walls_moved(&mut self, (lo, hi): (IVec3, IVec3)) {
        let before = self.state;
        self.walls_changed();
        let corner = |c: IVec3| c.as_vec3().extend(0.0).to_array();
        self.moved = Some((before, corner(lo - MOVED_MARGIN), corner(hi + MOVED_MARGIN)));
    }

    /// What the level being played holds of the pass, taken away as it is
    /// put aside built (`render::kept`): its volume (and thin things) and
    /// the faces' memory with the state its answers were found in. The
    /// next level's are made as a fresh renderer makes them.
    pub(super) fn put_aside(&mut self) -> LampsKept {
        let kept = LampsKept { volume: self.volume.take(), seen: self.seen.take(), state: self.state, known: std::mem::take(&mut self.known) };
        // (Another state, so no answer of the level put aside is taken for
        // the next one's.)
        self.state = next_state(self.state);
        self.moved = None;
        self.bind = None;
        kept
    }

    /// A level put aside taken up again (`render::kept`): what it held of
    /// the pass, its memory's answers still its own; None: as a fresh
    /// renderer has none.
    pub(super) fn take_up(&mut self, kept: Option<LampsKept>) {
        let Some(k) = kept else {
            (self.volume, self.seen) = (None, None);
            self.known.clear();
            self.state = next_state(self.state);
            self.moved = None;
            self.bind = None;
            return;
        };
        // (A count of answers without room read from the memory before
        // tells this one's too: the same picture asks as much.)
        (self.volume, self.seen, self.state, self.known, self.moved) = (k.volume, k.seen, k.state, k.known, None);
        self.bind = None;
    }

    /// Whether it holds anything a picture made (`used`): the tests'.
    #[cfg(test)]
    pub(super) fn holds(&self) -> bool {
        self.targets.is_some() || self.seen.is_some() || self.volume.is_some() || self.ask_target.is_some()
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

    /// The bytes of the faces' memory (none before walls first stopped a
    /// lamp).
    pub(super) fn memory_bytes(&self) -> u64 {
        self.seen.as_ref().map_or(0, |(faces, _)| full_at(*faces) + COUNTS)
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

/// The texels of a `LampRow`.
const ROW_TEXELS: u32 = (size_of::<LampRow>() / 16) as u32;

pub(super) fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("lamps' tiles"), size: (capacity * size_of::<TileInstance>()) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

/// The lights' texture, room for `rows` lights (a row each, `LampRow`'s
/// parts across).
pub(super) fn lights_texture(device: &wgpu::Device, rows: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lamps' lights"),
        size: wgpu::Extent3d { width: ROW_TEXELS, height: rows, depth_or_array_layers: 1 },
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
        for binding in [6, 7, 8, 9] {
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
/// `view_proj` (its low and high corner, -1 to 1): its box's corners'
/// (`Light::bounds`: a spot's its cone's, so its tiles and its beams' are
/// those the cone can light); the whole screen when one lies behind the
/// eye; None when off the screen.
pub(super) fn rect_of(view_proj: &Mat4, l: &Light) -> Option<[f32; 4]> {
    let (bl, bh) = l.bounds();
    let (mut lo, mut hi) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
    for corner in crate::camera::box_corners(bl, bh) {
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
    /// (at most `FORWARD_LIGHTS`, none walls stop, no fog), else by the
    /// lamps' pass, everything it needs made (the
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
        // As large as the picture asks, or as pictures found it needs; never
        // smaller while the picture's size stays.
        let faces = faces_for(w, h).max(self.lamps.grown).min(self.lamps.most(device));
        let mut walls = walls && faces > 0;
        if walls && self.lamps.seen.as_ref().is_none_or(|(held, _)| *held < faces) {
            let bytes = full_at(faces) + COUNTS;
            match made(device, || device.create_buffer(&wgpu::BufferDescriptor { label: Some("lamps' memory"), size: bytes, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false })) {
                Ok(buffer) => {
                    // (A count read from the memory before is not this one's.)
                    if let Walls::Ready { full, .. } = &mut self.lamps.walls {
                        *full = FullWatch::new(device, GROWN_EVERY);
                    }
                    self.lamps.seen = Some((faces, buffer));
                    self.lamps.bind = None;
                    self.lamps.known.clear();
                }
                // Kept as it is, grown no further; without one, through
                // walls.
                Err(why) => {
                    let kept = self.lamps.seen.as_ref().map(|(held, _)| *held);
                    self.lamps.memory_room = kept.unwrap_or(0);
                    self.lamps.told_full = true;
                    self.said.push(match kept {
                        Some(held) => format!("lamps: the GPU could not give their memory {} MB ({why}): it stays at {} MB, slower where it is full", bytes >> 20, (full_at(held) + COUNTS) >> 20),
                        None => format!("lamps: the GPU could not give their memory {} MB ({why}): they shine through walls", bytes >> 20),
                    });
                    walls = kept.is_some();
                }
            }
        }
        Some(LampFrame { walls, shader })
    }

    /// Whether walls can stop lamps on this GPU: the pass with walls, made
    /// the first time (said once where it cannot be). A test whose pass
    /// would not build fails: a mistake in its shader drew no walls and no
    /// shadows of what moves, the picture otherwise as it should be.
    fn walling(&mut self, device: &wgpu::Device) -> bool {
        if matches!(self.lamps.walls, Walls::Unasked) {
            self.lamps.walls = make_walls(device).unwrap_or_else(|why| {
                #[cfg(test)]
                assert!(REFUSED.with(std::cell::Cell::get), "lamps: the pass with walls would not build: {why}");
                self.said.push(format!("lamps: the pass with walls would not build on this GPU ({why}): they shine through walls"));
                Walls::Lacking
            });
            self.lamps.bind = None;
        }
        matches!(self.lamps.walls, Walls::Ready { .. })
    }

    /// The level's volume for the picture's world and pieces, round what
    /// its lamps reach: made the first time (and for another level, another
    /// bank or other pieces, or a lamp reaching past it), else the chunks
    /// changed since brought up to date (the world's, and those a piece
    /// moved from or to: a figure posed anew every picture), on the GPU
    /// too. False (said once) when the lamps reach too far apart for it.
    fn volume_for(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &FrameData) -> bool {
        let Some(world) = frame.lamp_world.as_ref() else { return false };
        let lit: Vec<(glam::Vec3, f32)> = frame.lights.iter().filter(|l| l.stopped()).map(|l| (l.pos, l.radius)).collect();
        let key = volume::pieces_key(&frame.bank, &frame.lamp_pieces);
        let held = self.lamps.volume.as_ref().is_some_and(|h| h.volume.reach.holds(&lit, world.size) && h.volume.same_level(world));
        let reach = if held { self.lamps.volume.as_ref().expect("held").volume.reach } else { Reach::of(&lit, world.size) };
        if let Err(why) = reach.fits(device.limits().max_texture_dimension_3d) {
            if !self.lamps.told_big {
                self.lamps.told_big = true;
                self.said.push(format!("lamps: {why}: they shine through walls"));
            }
            self.lamps.volume = None;
            return false;
        }
        let texels = reach.texels;
        if reach.bytes() >= self.lamps.volume_room {
            self.lamps.volume = None;
            return false;
        }
        let lamps = &mut self.lamps;
        let kept = lamps.volume.as_mut().filter(|_| held).and_then(|h| {
            let changed = if h.volume.pieces_key == key { Some(h.volume.update(world)) } else { h.volume.repose(world, &frame.bank, &frame.lamp_pieces, key) };
            changed.map(|c| (h, c))
        });
        if let Some((held, changed)) = kept {
            if !changed.is_empty() {
                if let Some(thin) = &mut held.thin {
                    thin.update(&held.volume, &changed);
                }
                for &(lo, size) in &changed {
                    for m in 0..LEVELS {
                        write(queue, &held.texture, m, lo >> m as i32, size >> m as i32, &held.volume.bytes_of(m, lo, size));
                    }
                }
                match held.volume.moved {
                    Some(within) => lamps.walls_moved(within),
                    None => lamps.walls_changed(),
                }
            }
            return true;
        }
        let started = Instant::now();
        lamps.volume = None;
        let texture = made(device, || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("the level's solid voxels"),
                size: wgpu::Extent3d { width: texels.x as u32, height: texels.y as u32, depth_or_array_layers: texels.z as u32 },
                mip_level_count: LEVELS,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: wgpu::TextureFormat::R8Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        let texture = match texture {
            Ok(texture) => texture,
            Err(why) => {
                lamps.volume_room = reach.bytes();
                self.said.push(format!("lamps: the GPU could not give their walls {} MB ({why}): they shine through walls", reach.bytes() >> 20));
                return false;
            }
        };
        let volume = Volume::new(world, reach, &frame.bank, &frame.lamp_pieces, key);
        for m in 0..LEVELS {
            write(queue, &texture, m, IVec3::ZERO, texels >> m as i32, &volume.copies[m as usize]);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        lamps.volume = Some(Held { volume, view, texture, thin: None });
        lamps.walls_changed();
        lamps.bind = None;
        lamps.made_ms = started.elapsed().as_secs_f64() * 1000.0;
        lamps.volumes_made += 1;
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
        let built = if moving { super::movers::Movers::of(&frame.bank, &frame.lamp_movers, opaque_cubes(frame)) } else { None };
        let lamps = &mut self.lamps;
        lamps.movers_shown = 0;
        lamps.movers_words.clear();
        // Room for their solids and bits, made larger as needed: what the
        // GPU could not give stops no lamp (said once).
        let built = match built {
            Some(Ok(m)) => match lamps.movers_fit(device, &m) {
                Ok(()) => Some(Ok(m)),
                Err(None) => None,
                Err(Some(why)) => {
                    self.said.push(why);
                    None
                }
            },
            other => other,
        };
        match built {
            Some(Ok(m)) => {
                if lamps.movers_key != m.key {
                    queue.write_buffer(&lamps.movers_bits.0, 0, bytemuck::cast_slice(&m.bits));
                    lamps.movers_key = m.key;
                }
                queue.write_buffer(&lamps.movers_solids.0, 0, bytemuck::cast_slice(&m.solids));
                queue.write_buffer(&lamps.movers_boxes, 0, bytemuck::cast_slice(&m.boxes));
                lamps.movers_shown = m.boxes.len() as i32;
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
        let rows: Vec<LampRow> = shown.iter().map(|(i, l, _)| lamps.data_of(*i, l, lf.walls)).collect();
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
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size_of::<LampRow>() as u32), rows_per_image: Some(rows.len() as u32) },
                wgpu::Extent3d { width: ROW_TEXELS, height: rows.len() as u32, depth_or_array_layers: 1 },
            );
        }
        let (dims, origin) = lamps.volume.as_ref().map_or(([0; 4], [0; 4]), |h| {
            let r = h.volume.reach;
            ([r.texels.x, r.texels.y, r.texels.z, LEVELS as i32], [r.origin.x, r.origin.y, r.origin.z, 0])
        });
        // The lamps walls stop that are new to the faces' memory: asked for
        // first, at a pixel every ASK.
        let mut known = std::collections::BTreeSet::new();
        let mut fresh = Vec::new();
        for (row, ((_, _, rect), data)) in shown.iter().zip(&rows).enumerate() {
            if lf.walls && data.light.edge[1] > 0.5 {
                let key = [data.light.edge[2] as u32, data.light.edge[3] as u32];
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
        let globals = LampGlobals { inv_view_proj: frame.view_proj.inverse().to_cols_array_2d(), dims, origin, info: [lamps.state, self.pictures, lamps.moved.map_or(0, |m| m.0), 0], movers: [lamps.movers_shown, 0, 0, 0], moved_lo: lamps.moved.map_or([0.0; 4], |m| m.1), moved_hi: lamps.moved.map_or([0.0; 4], |m| m.2) };
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

    /// Which of the lamps' memory's counts tells whether it grows
    /// (`full_at`'s offset): the crowded answers' while it holds fewer than
    /// `CROWDED_UP_TO` a pixel, else those that found no room (at its most
    /// they are said).
    fn grows_by(&self, device: &wgpu::Device, faces: u64) -> u64 {
        let (_, _, w, h) = self.picture();
        if faces * 2 > self.lamps.most(device) || faces >= CROWDED_UP_TO * u64::from(w) * u64::from(h) { 4 } else { 0 }
    }

    /// A step of reading the count that tells whether the lamps' memory
    /// grows (`grows_by`), without waiting (`render`): made larger
    /// (`too_small`), or said.
    pub(super) fn watch_lamps(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.lamps.told_full {
            return;
        }
        let Some(by) = self.lamps.seen.as_ref().map(|(faces, _)| self.grows_by(device, *faces)) else { return };
        let (Walls::Ready { full, .. }, Some((faces, seen))) = (&mut self.lamps.walls, &self.lamps.seen) else { return };
        let faces = *faces;
        if let Some(count) = full.step(device, encoder, seen, full_at(faces) + by, self.pictures) {
            self.too_small(device, faces, count);
        }
    }

    /// The same at once, waiting for the GPU (`shot`): whether it was made
    /// larger (by the next picture).
    pub fn lamps_now(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<bool> {
        let Some((faces, seen)) = &self.lamps.seen else { return Ok(false) };
        if self.lamps.told_full {
            return Ok(false);
        }
        let faces = *faces;
        let count = super::limits::read_now(device, queue, seen, full_at(faces) + self.grows_by(device, faces))?;
        Ok(self.too_small(device, faces, count))
    }

    /// The lamps' memory of `faces` answers found `count` crowded or
    /// without room (`grows_by`): made twice as large from the next picture
    /// on (every answer walked once more), up to the GPU's largest buffer;
    /// there, `count` that found no room, said once. Whether it grew.
    fn too_small(&mut self, device: &wgpu::Device, faces: u64, count: u32) -> bool {
        if count == 0 {
            return false;
        }
        if faces * 2 <= self.lamps.most(device) {
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
fn lamps_full(faces: u64, count: u32) -> String {
    format!("lamps: their memory is full ({faces} faces and points of beams, the most this GPU holds; {count} more found no room): those ask again every picture, the picture the same but slower")
}

impl Lamps {
    /// The `i`th of the frame's lights as the shaders take it, walls
    /// stopping it where `walls` and it says so, from where it walks (the
    /// level's volume's `start`); a lamp whose shadows are "all" with its
    /// word for the moving things' boxes (`movers_words`).
    pub(super) fn data_of(&self, i: usize, l: &Light, walls: bool) -> LampRow {
        let mut light = LightData::of(l, walls);
        if light.edge[1] > 1.5 {
            light.edge[1] = self.movers_words.get(i).copied().flatten().unwrap_or(2.0);
        }
        let start = if light.edge[1] > 0.5 { self.volume.as_ref().map_or(Some(l.pos), |h| h.volume.start(l.pos)) } else { Some(l.pos) };
        let from = start.map_or([0.0, 0.0, 0.0, 1.0], |p| p.extend(0.0).to_array());
        LampRow { light, from }
    }

    /// Room for the moving things `m`'s solids and bits, made larger where
    /// they need more; a mistake where the GPU could not give it, what to
    /// say the first time (None after: they stop no lamp).
    fn movers_fit(&mut self, device: &wgpu::Device, m: &super::movers::Movers) -> Result<(), Option<String>> {
        let (count, words) = (m.solids.len().next_power_of_two(), m.bits.len().next_power_of_two());
        let need = count * size_of::<super::movers::Solid>() + words * 4;
        if need >= self.movers_room {
            return Err(None);
        }
        let mut failed = |why: String| {
            self.movers_room = need;
            Some(format!("lamps: the GPU could not give the moving things {} MB ({why}): they stop no lamp", need >> 20))
        };
        if m.solids.len() > self.movers_solids.1 {
            match made(device, || solids_buffer(device, count)) {
                Ok(solids) => self.movers_solids = solids,
                Err(why) => return Err(failed(why)),
            }
            self.bind = None;
        }
        if m.bits.len() > self.movers_bits.1 {
            match made(device, || bits_buffer(device, words)) {
                Ok(bits) => self.movers_bits = bits,
                Err(why) => return Err(failed(why)),
            }
            self.movers_key = !m.key;
            self.bind = None;
        }
        Ok(())
    }

    /// What the pass reads, with walls (the volume, the faces' memory and
    /// the moving things) or without.
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
                entries.push(wgpu::BindGroupEntry { binding: 6, resource: self.movers_solids.0.as_entire_binding() });
                entries.push(wgpu::BindGroupEntry { binding: 7, resource: self.movers_boxes.as_entire_binding() });
                entries.push(wgpu::BindGroupEntry { binding: 8, resource: self.movers_lists.as_entire_binding() });
                entries.push(wgpu::BindGroupEntry { binding: 9, resource: self.movers_bits.0.as_entire_binding() });
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

/// The volume's state numbers, 1 to this (the shaders' LAMP_STATES: the
/// bits an answer keeps of it).
pub(super) const STATES: u32 = 0x1ff;

/// The volume's next state number (never 0: an entry of the faces' memory
/// never written has 0).
fn next_state(state: u32) -> u32 {
    (state % STATES) + 1
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
    let pass = made(device, || lamp_pass(device, &LAMPS, true))?;
    Ok(Walls::Ready { pass, full: FullWatch::new(device, GROWN_EVERY) })
}

#[cfg(test)]
thread_local! {
    /// Whether `made` fails in a test, as a GPU without the memory would
    /// (no test can have a real one run out).
    pub static REFUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// What `make` makes on the GPU (a buffer, a texture, a pipeline), or why the
/// GPU could not give it, in words: out of memory, past what the device
/// takes, a shader it would not take. wgpu's default for such a mistake
/// ends the program; the callers keep what they had, or draw without it,
/// and say it once.
pub(super) fn made<T>(device: &wgpu::Device, make: impl FnOnce() -> T) -> Result<T, String> {
    #[cfg(test)]
    if REFUSED.with(std::cell::Cell::get) {
        return Err("refused by the test".into());
    }
    let scopes = [wgpu::ErrorFilter::OutOfMemory, wgpu::ErrorFilter::Validation, wgpu::ErrorFilter::Internal].map(|f| device.push_error_scope(f));
    let made = make();
    let failed: Vec<Option<wgpu::Error>> = scopes.into_iter().rev().map(|s| pollster::block_on(s.pop())).collect();
    match failed.into_iter().flatten().next() {
        // wgpu's words over several lines (a heading, what it was making,
        // the cause): on one.
        Some(e) => Err(e.to_string().lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")),
        None => Ok(made),
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

    /// A spot's box is its cone's within its ball (its tiles and its beams'
    /// those): every point it lights lies in it, and a narrow spot's is a
    /// fraction of its ball's; a round light's is its ball's.
    #[test]
    fn a_spots_box_holds_its_cone_and_no_more() {
        use crate::render::LightShape;
        for (cone, rotation, pitch) in [(30.0, 0.0, -90.0), (50.0, 25.0, -50.0), (120.0, 90.0, 10.0), (300.0, 200.0, 0.0)] {
            let l = Light::lamp(glam::Vec3::new(10.0, 20.0, 30.0), 60.0, [1.0; 3], &LightShape { cone: Some(cone), rotation, pitch, ..Default::default() }, |w| w);
            let s = l.spot.expect("a spot");
            let (lo, hi) = l.bounds();
            for i in 0..4000 {
                // Points of the ball spread evenly (a fixed sequence).
                let f = |k: u32| ((i * k) % 1000) as f32 / 1000.0;
                let way = glam::Vec3::new(f(37) * 2.0 - 1.0, f(91) * 2.0 - 1.0, f(53) * 2.0 - 1.0).normalize_or_zero();
                let p = l.pos + way * 60.0 * f(13);
                if way.dot(s.way) > s.outer {
                    assert!(p.cmpge(lo).all() && p.cmple(hi).all(), "cone {cone}: {p} in {lo}..{hi}");
                }
            }
        }
        let down = Light::lamp(glam::Vec3::ZERO, 60.0, [1.0; 3], &LightShape { cone: Some(30.0), pitch: -90.0, ..Default::default() }, |w| w);
        let (lo, hi) = down.bounds();
        assert!(hi.x - lo.x < 36.0 && hi.z - lo.z <= 62.0 && hi.z <= 1.0, "straight down: {lo}..{hi}");
        let round = Light::lamp(glam::Vec3::ZERO, 60.0, [1.0; 3], &LightShape::default(), |w| w);
        assert_eq!(round.bounds(), (glam::Vec3::splat(-61.0), glam::Vec3::splat(61.0)));
    }

    /// On the GPU: a device is asked for room for the lamps' memory up to
    /// `MOST_FACES`, never past what the adapter gives (the asking would
    /// fail) and never less than wgpu's defaults; the memory may grow as
    /// far as the device holds.
    #[test]
    fn the_device_holds_the_lamps_memory_as_far_as_the_gpu_allows() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).expect("a GPU");
        let (asked, most, d) = (crate::render::device_limits(&adapter), adapter.limits(), wgpu::Limits::default());
        for (a, m, d) in [(asked.max_storage_buffer_binding_size, most.max_storage_buffer_binding_size, d.max_storage_buffer_binding_size), (asked.max_buffer_size, most.max_buffer_size, d.max_buffer_size)] {
            assert!(a >= d && a <= MOST_BYTES.max(d) && (a <= m || a == d), "asked {a}, the adapter {m}, the default {d}");
        }
        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: asked.clone(), ..Default::default() })).expect("the device asked so");
        let faces = most_faces(&device);
        assert!(full_at(faces) + COUNTS <= asked.max_storage_buffer_binding_size.min(asked.max_buffer_size) && faces <= MOST_FACES);
        if most.max_storage_buffer_binding_size.min(most.max_buffer_size) >= MOST_BYTES {
            assert_eq!(faces, MOST_FACES, "this GPU holds the largest memory");
        }
        // A buffer past what the device takes is a mistake in words, not
        // the end of the program; a shader that would not build says why.
        let past = made(&device, || device.create_buffer(&wgpu::BufferDescriptor { label: None, size: device.limits().max_buffer_size + 4096, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false }));
        assert!(past.as_ref().is_err_and(|why| !why.is_empty()), "{:?}", past.err());
        let broken = made(&device, || device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("broken"), source: wgpu::ShaderSource::Wgsl("struct S { of: u32 };".into()) }));
        assert!(broken.as_ref().is_err_and(|why| why.contains("reserved") && !why.contains('\n')), "{:?}", broken.err());
    }
}

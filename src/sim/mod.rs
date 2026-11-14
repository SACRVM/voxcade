//! The engine's simulation core, what every voxel game here shares: the
//! world, bodies that walk and fall, explosions that blow holes, pieces cut
//! off from the ground falling, debris, dust, fire, smoke, flashes, the
//! camera following its target, screen shake and hit stop, and sounds
//! reported by name. A game holds a `Sim` and adds its own things and rules
//! (`shooter::Shooter`, More Blood's). Runs at a fixed 60 ticks per second
//! and is fully deterministic: the same input sequence always gives the
//! same game. Nothing in here may read the clock or use global randomness.

pub mod math;
pub mod things;

use std::sync::Arc;

use crate::world::CellSet;

use glam::{IVec2, IVec3, Vec2, Vec3};

use crate::config::{GameConfig, PhysicsConfig};
use crate::rng::Rng;
use crate::world::{Hold, World};

pub const TICK_RATE: u32 = 60;
pub const DT: f32 = 1.0 / TICK_RATE as f32;

/// Largest piece a blast can cut loose. Anything bigger counts as grounded,
/// which also bounds the search after each explosion.
const MAX_LOOSE: usize = 50_000;
/// A blast's work goes to several cores in parts of at least this many
/// layers of its box, and of this many removed voxels (`par::in_order`).
const BLAST_LAYERS_A_THREAD: usize = 8;
const REMOVED_A_THREAD: usize = 2000;
/// Body width the effect speeds are tuned for; they scale with the game's
/// `physics.body_width`.
const EFFECT_WIDTH: f32 = 3.0;
/// The 26 neighbours of a voxel. Voxels touching by a face, an edge or a
/// corner hold together, so diagonal lines (palm fronds, a leaning trunk)
/// stand the way they look. Popped last-in-first-out, the ground search
/// tries straight down first and finds the ground quickly.
const NEIGHBOURS: [IVec3; 26] = neighbours();

const fn neighbours() -> [IVec3; 26] {
    let mut out = [IVec3::ZERO; 26];
    let mut n = 0;
    let mut z = 1;
    while z >= -1 {
        let mut y = -1;
        while y <= 1 {
            let mut x = -1;
            while x <= 1 {
                // The centre is no neighbour; straight down goes last.
                if (x != 0 || y != 0) || z == 1 {
                    out[n] = IVec3::new(x, y, z);
                    n += 1;
                }
                x += 1;
            }
            y += 1;
        }
        z -= 1;
    }
    out[n] = IVec3::new(0, 0, -1);
    out
}
/// Shades of red a body bursts into (`debris.gore`).
pub const BLOOD_COLORS: [[u8; 4]; 4] =
    [[150, 16, 20, 255], [118, 10, 14, 255], [178, 30, 28, 255], [92, 6, 10, 255]];
/// Smoke from light to dark.
const SMOKE_COLORS: [[u8; 4]; 3] = [[132, 124, 112, 255], [108, 102, 94, 255], [84, 80, 74, 255]];
const SPARK_COLORS: [[u8; 4]; 4] =
    [[255, 250, 210, 255], [255, 208, 90, 255], [255, 146, 48, 255], [240, 90, 40, 255]];

/// A sound the last tick made, where, and how loud (0..1).
#[derive(Clone, Copy, Debug)]
pub struct SoundEvent {
    /// The event's name (the game's sounds are looked up by it).
    pub name: &'static str,
    pub at: Vec3,
    pub volume: f32,
}

pub use crate::world::Marker;

/// A model as far as the simulation cares: its size for collision and its
/// voxels to burst into.
#[derive(Clone, Default)]
pub struct Shape {
    pub size: IVec3,
    pub voxels: Vec<(IVec3, [u8; 4])>,
}

impl Shape {
    pub fn block(size: IVec3) -> Self {
        Self { size, voxels: Vec::new() }
    }

    pub fn height(&self) -> f32 {
        self.size.z.max(1) as f32
    }
}

/// Anything that walks: the player, enemies, hostages.
#[derive(Clone)]
pub struct Body {
    /// Centre of the feet.
    pub pos: Vec3,
    pub prev: Vec3,
    pub vel: Vec3,
    /// Unit direction on the ground it moves and aims in (8 ways).
    pub facing: Vec3,
    /// The quarter turn its model shows: models live on the voxel grid and
    /// only turn in 90° steps.
    pub look: IVec2,
    pub height: f32,
    pub on_ground: bool,
    /// Ground distance covered since it started walking; drives the walk
    /// animation so the steps match the speed.
    pub walked: f32,
    /// Height of the ground it last stood on.
    ground_z: f32,
    /// How far it dropped when it landed this tick (0 otherwise).
    pub fell: f32,
    /// What it did as it last moved and for how many ticks (`motion`):
    /// only for the picture.
    last_motion: Motion,
    motion_ticks: u32,
}

/// What a body does, as its frames show it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    Still,
    Walking,
    Air,
}

impl Body {
    pub fn new(pos: Vec3, height: f32) -> Self {
        Self {
            pos,
            prev: pos,
            vel: Vec3::ZERO,
            facing: Vec3::NEG_Y,
            look: IVec2::NEG_Y,
            height,
            on_ground: false,
            walked: 0.0,
            ground_z: pos.z,
            fell: 0.0,
            // (Not on the ground until it moves.)
            last_motion: Motion::Air,
            motion_ticks: 0,
        }
    }

    pub fn centre(&self) -> Vec3 {
        self.pos + Vec3::Z * self.height * 0.5
    }

    /// Distance from `p` to the nearest point of the body's axis, feet to
    /// head: how far a blast at `p` has to reach to touch it.
    pub fn distance_to(&self, p: Vec3) -> f32 {
        let z = p.z.clamp(self.pos.z, self.pos.z + self.height);
        p.distance(Vec3::new(self.pos.x, self.pos.y, z))
    }

    /// Whether `p` lies inside the body (`half` its half width), grown by `margin`.
    pub fn contains(&self, p: Vec3, half: f32, margin: f32) -> bool {
        let r = half + margin;
        (p.x - self.pos.x).abs() <= r
            && (p.y - self.pos.y).abs() <= r
            && p.z >= self.pos.z - margin
            && p.z <= self.pos.z + self.height + margin
    }

    /// Turns towards a direction on the ground (8 ways for aim, 4 for the model).
    pub fn face(&mut self, dir: Vec3) {
        let dir = Vec3::new(dir.x, dir.y, 0.0).normalize_or_zero();
        if dir != Vec3::ZERO {
            self.facing = dir;
            self.look = quarter_turn(dir, self.look);
        }
    }

    /// Accelerates towards `wish * speed` on the ground plane.
    pub fn steer(&mut self, wish: Vec3, speed: f32, accel: f32) {
        let accel = accel * DT;
        let horizontal = Vec3::new(self.vel.x, self.vel.y, 0.0);
        let delta = wish * speed - horizontal;
        let change = if delta.length() > accel { delta.normalize() * accel } else { delta };
        self.vel.x += change.x;
        self.vel.y += change.y;
    }

    /// Gravity, movement with collision, and the walk distance.
    pub fn advance(&mut self, world: &World, physics: &PhysicsConfig) {
        self.advance_in(world, physics, physics.body_width * 0.5, &[]);
    }

    /// `advance` for a body `half` its width across, with ledges to stand
    /// on besides the world (`move_body_in`).
    pub fn advance_in(&mut self, world: &World, physics: &PhysicsConfig, half: f32, ledges: &[Ledge]) {
        let was_on_ground = self.on_ground;
        self.vel.z = (self.vel.z - physics.gravity * DT).max(-physics.max_fall);
        move_body_in(world, self, physics, half, ledges);
        self.fell = 0.0;
        if self.on_ground {
            if !was_on_ground {
                self.fell = (self.ground_z - self.pos.z).max(0.0);
            }
            self.ground_z = self.pos.z;
            let moved = (self.pos - self.prev).truncate().length();
            self.walked = if moved > 0.05 { self.walked + moved } else { 0.0 };
        }
        let now = self.motion().0;
        self.motion_ticks = if now == self.last_motion { self.motion_ticks.saturating_add(1) } else { 0 };
        self.last_motion = now;
    }

    /// What it does, for its frames (`things::figure_frame`): standing,
    /// walking or in the air, and for how many ticks it has.
    pub fn motion(&self) -> (Motion, u32) {
        let now = if !self.on_ground { Motion::Air } else if self.walked > 0.0 { Motion::Walking } else { Motion::Still };
        (now, if now == self.last_motion { self.motion_ticks } else { 0 })
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ParticleKind {
    /// Short-lived glow from an explosion.
    Spark,
    /// A muzzle flash: stays where it is for a few ticks.
    Flash,
    /// A fireball: glows, rises a little and burns from white to red.
    Fire,
    /// Smoke after a blast: rises, drifts and thins out.
    Smoke,
    /// Dust kicked up by steps and by shots into the ground.
    Dust,
    /// A voxel of the world; may settle back into it (palette index).
    Debris(u8),
    /// A voxel of a body; bounces, lies still, fades.
    Gib,
}

#[derive(Clone)]
pub struct Particle {
    pub pos: Vec3,
    pub prev: Vec3,
    pub color: [u8; 4],
    vel: Vec3,
    kind: ParticleKind,
    life: f32,
    /// The life it started with.
    span: f32,
}

impl Particle {
    pub fn new(pos: Vec3, vel: Vec3, color: [u8; 4], kind: ParticleKind, life: f32) -> Self {
        Self { pos, prev: pos, color, vel, kind, life, span: life }
    }

    /// How it looks right now: colour, how much it glows (0..1) and how
    /// opaque it is (0..1).
    pub fn look(&self) -> ([u8; 4], f32, f32) {
        let left = (self.life / self.span).clamp(0.0, 1.0);
        match self.kind {
            ParticleKind::Spark => (self.color, 1.0, 1.0),
            ParticleKind::Flash => (self.color, 1.0, 0.85),
            ParticleKind::Fire => (fire_color(left), 0.5 + 0.5 * left, 0.3 + 0.65 * left),
            ParticleKind::Smoke => (self.color, 0.0, 0.55 * left.sqrt()),
            ParticleKind::Dust => (self.color, 0.0, 0.7 * left),
            ParticleKind::Debris(_) | ParticleKind::Gib => (self.color, 0.0, 1.0),
        }
    }

    /// Fire and smoke are drawn chunky, a few voxels across.
    pub fn chunky(&self) -> bool {
        matches!(self.kind, ParticleKind::Fire | ParticleKind::Smoke)
    }
}

/// Fire burns from white through yellow and orange to a dark red.
fn fire_color(left: f32) -> [u8; 4] {
    match left {
        l if l > 0.8 => [255, 248, 210, 255],
        l if l > 0.55 => [255, 214, 96, 255],
        l if l > 0.3 => [250, 132, 44, 255],
        _ => [176, 62, 30, 255],
    }
}

/// A short burst of light from a muzzle flash or an explosion; the renderer
/// lights what is around it.
#[derive(Clone)]
pub struct Flare {
    pub pos: Vec3,
    pub radius: f32,
    /// Colour times brightness at its start.
    pub color: [f32; 3],
    life: f32,
    span: f32,
}

impl Flare {
    /// A light at `pos`, `radius` its reach, fading out over `span` seconds.
    pub fn new(pos: Vec3, radius: f32, color: [f32; 3], span: f32) -> Self {
        Self { pos, radius, color, life: span, span }
    }

    /// 1 when it starts, fading to 0.
    pub fn strength(&self) -> f32 {
        (self.life / self.span).clamp(0.0, 1.0)
    }
}

#[derive(Clone)]
pub struct Sim {
    pub world: World,
    pub particles: Vec<Particle>,
    pub flares: Vec<Flare>,
    /// Sounds made during the last tick.
    pub sounds: Vec<SoundEvent>,
    /// What the ready-made rules did to things in the last tick, in order:
    /// what a Lua game tells its script (`on_hit`, `on_death`,
    /// `on_pickup`) and counts into inventories; like the sounds, only
    /// reported.
    pub events: Vec<Event>,
    pub tick: u64,
    /// Screen shake, 0..1.
    pub shake: f32,
    /// White over the whole screen after a big blast, 0..1.
    pub screen_flash: f32,
    /// Ticks left that the game holds still (hit stop).
    pub freeze: u32,
    /// Palette indices the last explosion removed, in order.
    chip_colors: Vec<u8>,
    /// Where the camera looks, and looked a tick ago (it follows a target
    /// across the ground, `follow`).
    pub cam_target: Vec3,
    pub cam_prev: Vec3,
    /// How far the view leads ahead of its target (a side view looking
    /// where the hero runs), and a tick ago; only the picture uses it.
    pub lead: Vec3,
    pub lead_prev: Vec3,
    /// The engine's part of the config, shared by every copy of the game
    /// (`Session::snapshot`).
    pub cfg: Arc<GameConfig>,
    /// The only source of randomness a game may use.
    pub rng: Rng,
}

impl Sim {
    /// A world to play in, the camera looking at `camera_at`.
    pub fn new(world: World, cfg: &GameConfig, camera_at: Vec3) -> Self {
        Self {
            world,
            particles: Vec::new(),
            flares: Vec::new(),
            sounds: Vec::new(),
            events: Vec::new(),
            tick: 0,
            shake: 0.0,
            screen_flash: 0.0,
            freeze: 0,
            chip_colors: Vec::new(),
            cam_target: camera_at,
            cam_prev: camera_at,
            lead: Vec3::ZERO,
            lead_prev: Vec3::ZERO,
            cfg: Arc::new(cfg.clone()),
            rng: Rng::new(0x5EED),
        }
    }

    pub fn apply_config(&mut self, cfg: &GameConfig) {
        self.cfg = Arc::new(cfg.clone());
    }

    /// Swaps in a reloaded level; the debris goes.
    pub fn replace_world(&mut self, world: World) {
        self.world = world;
        self.particles.clear();
    }

    /// The start of a tick: last tick's sounds go, the screen flash fades.
    /// Whether the tick goes on; during hit stop (`freeze`) it does not, and
    /// everything stays where it is, drawn still (the game holds its own
    /// things still too).
    pub fn begin_tick(&mut self) -> bool {
        self.sounds.clear();
        self.events.clear();
        self.screen_flash = (self.screen_flash - 4.0 * DT).max(0.0);
        if self.freeze > 0 {
            self.freeze -= 1;
            self.cam_prev = self.cam_target;
            self.lead_prev = self.lead;
            for q in &mut self.particles {
                q.prev = q.pos;
            }
            return false;
        }
        self.tick += 1;
        self.cam_prev = self.cam_target;
        true
    }

    /// Flashes and explosions' light fade.
    pub fn update_flares(&mut self) {
        self.flares.retain_mut(|f| {
            f.life -= DT;
            f.life > 0.0
        });
    }

    /// The end of a tick: the camera follows `target` (the feet) across the
    /// ground (`c.camera.follow`), the shake dies down. Seen from above the
    /// camera only moves across the ground; its height stays where it
    /// started, so climbing, jumping and rubble never move the view. A side
    /// view's camera (`c.camera.follow_height`) keeps the feet within a
    /// band around its height, moving only when they leave it.
    pub fn follow(&mut self, target: Vec3) {
        let follow = self.cfg.camera.follow.clamp(0.01, 1.0);
        self.cam_target.x += (target.x - self.cam_target.x) * follow;
        self.cam_target.y += (target.y - self.cam_target.y) * follow;
        if let Some(band) = self.cfg.camera.follow_height {
            self.cam_target.z = self.cam_target.z.max(target.z - band.above).min(target.z + band.below);
        }
        self.shake = (self.shake - 2.5 * DT).max(0.0);
    }

    /// The view's lead a step towards `want`, at most `speed` voxels a
    /// second: it eases round when the hero turns instead of whipping.
    pub fn lead_toward(&mut self, want: Vec3, speed: f32) {
        self.lead_prev = self.lead;
        let step = speed.max(0.0) * DT;
        let d = want - self.lead;
        self.lead += if d.length() > step { d.normalize() * step } else { d };
    }

    /// Reports a sound by its name (the game's sound bank plays it).
    pub fn sound(&mut self, name: &'static str, at: Vec3, volume: f32) {
        self.sounds.push(SoundEvent { name, at, volume: volume.clamp(0.0, 1.0) });
    }

    /// Whether the particles are as many as `[debris] max_particles`
    /// allows: an effect makes no more (nor draws the numbers for them).
    pub fn particles_full(&self) -> bool {
        self.particles.len() >= self.cfg.debris.max_particles
    }

    /// Half the width of every walking body's collision box.
    pub fn half(&self) -> f32 {
        self.cfg.physics.body_width * 0.5
    }

    /// Size of effects (spark, debris and gib speeds) relative to the ones
    /// tuned for a body EFFECT_WIDTH wide.
    pub fn effect_scale(&self) -> f32 {
        self.cfg.physics.body_width / EFFECT_WIDTH
    }

    /// A puff of dust in the colour of what it came from.
    pub fn dust(&mut self, at: Vec3, count: usize, color: [u8; 4]) {
        let k = self.effect_scale();
        for _ in 0..count {
            if self.particles_full() {
                break;
            }
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let vel = Vec3::new(math::cos(a), math::sin(a), 0.0) * self.rng.range(4.0, 12.0) * k + Vec3::Z * self.rng.range(3.0, 9.0) * k;
            let life = self.rng.range(0.3, 0.6);
            self.particles.push(Particle::new(at, vel, color, ParticleKind::Dust, life));
        }
    }

    /// The fire and smoke of an explosion.
    pub fn fireball(&mut self, centre: Vec3, radius: f32) {
        let k = self.effect_scale();
        let core = radius * 0.8;
        let fire = (radius * radius * 1.6) as usize;
        for _ in 0..fire {
            if self.particles_full() {
                return;
            }
            let dir = self.random_dir();
            let d = math::cbrt(self.rng.f32()) * core;
            let pos = centre + dir * d + Vec3::Z * radius * 0.15;
            let vel = (dir * self.rng.range(6.0, 18.0) + Vec3::Z * self.rng.range(6.0, 16.0)) * k;
            // The middle burns longest, so it stays white the longest.
            let life = self.rng.range(0.4, 0.8) * (1.0 - 0.5 * d / core.max(0.1));
            self.particles.push(Particle::new(pos, vel, [255; 4], ParticleKind::Fire, life));
        }
        let smoke = (radius * radius * 1.2) as usize;
        for _ in 0..smoke {
            if self.particles_full() {
                return;
            }
            let dir = self.random_dir();
            let pos = centre + dir * math::cbrt(self.rng.f32()) * radius * 0.7 + Vec3::Z * radius * 0.3;
            let drift = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0), 0.0);
            let vel = (Vec3::Z * self.rng.range(5.0, 12.0) + drift * 5.0) * k;
            let color = SMOKE_COLORS[(self.rng.next_u32() % 3) as usize];
            let life = self.rng.range(1.5, 3.2);
            self.particles.push(Particle::new(pos, vel, color, ParticleKind::Smoke, life));
        }
    }

    pub fn random_dir(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0));
            let len = v.length();
            if len > 0.05 && len <= 1.0 {
                return v / len;
            }
        }
    }

    /// Removes a sphere of voxels and throws them around as debris. Whatever
    /// the blast cut off from the ground falls down.
    pub fn explode(&mut self, centre: Vec3, radius: f32) {
        self.chip_colors.clear();
        let k = self.effect_scale();
        let bedrock = self.cfg.world.bedrock;
        let mut removed = Vec::new();
        // Only cells inside the world can hold anything.
        let lo = (centre - radius).floor().as_ivec3().max(IVec3::ZERO);
        let hi = (centre + radius).ceil().as_ivec3().min(self.world.size - 1);
        let reach = radius * radius;
        // First what the blast takes (only reading: big blasts on several
        // cores, a layer each, a chunk looked up once for its neighbours),
        // then taking it, in the same order.
        let layers: Vec<i32> = (lo.z..=hi.z).collect();
        let world = &self.world;
        let hit = crate::par::in_order(&layers, BLAST_LAYERS_A_THREAD, |part| {
            blast_cells(world, part, lo, hi, centre, reach, bedrock)
        });
        for (cell, v) in hit {
            let mid = cell.as_vec3() + 0.5;
            self.chip_colors.push(v);
            self.world.set(cell, 0);
            removed.push(cell);
            if self.particles_full() || self.rng.f32() > 0.8 {
                continue;
            }
            let out = (mid - centre).normalize_or_zero();
            let jitter = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0), 0.0);
            let vel = (out * self.rng.range(15.0, 45.0)
                + jitter * 12.0
                + Vec3::Z * self.rng.range(25.0, 60.0))
                * k;
            let life = self.rng.range(3.0, 6.0);
            self.particles.push(Particle::new(mid, vel, self.world.palette[v as usize], ParticleKind::Debris(v), life));
        }
        // Finer voxels need more of them for the same fireball on screen:
        // the radius already grows with the grid, the area once more.
        self.sparks(centre, (radius * 7.0 * k) as usize);
        // A chip off a wall throws up dust; anything bigger burns, smokes,
        // lights up its surroundings and, if big, flashes the screen.
        let width = self.cfg.physics.body_width;
        if radius < width * 0.75 {
            self.sound("impact", centre, 1.0);
            if let Some(&v) = self.chip_colors.first() {
                let color = self.world.palette[v as usize];
                self.dust(centre, 6, color);
            }
        } else {
            self.sound("explosion", centre, (radius / (width * 2.0)).clamp(0.4, 1.0));
            self.fireball(centre, radius);
            self.flares.push(Flare::new(centre, radius * 4.0, [2.0, 1.3, 0.6], 0.3));
            if radius >= width * 1.5 {
                let flash = (radius / (width * 8.0)).min(0.4) * self.cfg.effects.screen_flash;
                self.screen_flash = (self.screen_flash + flash).min(0.5);
            }
        }
        // A blast about three bodies across shakes the most.
        let size = radius / (self.cfg.physics.body_width * 3.3);
        self.shake = (self.shake + size.min(0.35)).min(1.0);
        self.drop_loose(&removed);
    }

    pub fn sparks(&mut self, centre: Vec3, count: usize) {
        let k = self.effect_scale();
        for _ in 0..count {
            if self.particles_full() {
                break;
            }
            let dir = Vec3::new(
                self.rng.range(-1.0, 1.0),
                self.rng.range(-1.0, 1.0),
                self.rng.range(-0.2, 1.0),
            )
            .normalize_or_zero();
            let color = SPARK_COLORS[(self.rng.next_u32() % 4) as usize];
            let vel = dir * self.rng.range(60.0, 110.0) * k;
            let life = self.rng.range(0.2, 0.45);
            self.particles.push(Particle::new(centre, vel, color, ParticleKind::Spark, life));
        }
    }

    /// Bursts a body into its voxels; `push` sends the pieces away from a
    /// hit (in the speeds of a body EFFECT_WIDTH wide, like the rest).
    pub fn gib(&mut self, shape_voxels: &[(IVec3, [u8; 4])], size: IVec3, at: Vec3, push: Vec3) {
        let k = self.effect_scale();
        let gore = self.cfg.debris.gore;
        let pivot = IVec3::new(size.x / 2, size.y / 2, 0);
        for &(v, color) in shape_voxels {
            if self.particles_full() {
                break;
            }
            let pos = at + (v - pivot).as_vec3() + 0.5;
            // Out from the body's axis, turned a random way so a body that
            // is wide but shallow bursts all round instead of in a line,
            // and more out than up: a splash, not a fountain.
            let flat = Vec3::new(pos.x - at.x, pos.y - at.y, 0.0).normalize_or_zero();
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let out = (flat + Vec3::new(math::cos(a), math::sin(a), 0.0)).normalize_or_zero();
            let rise = (pos.z - at.z) / size.z.max(1) as f32;
            let vel = (out * self.rng.range(12.0, 38.0) + push + Vec3::Z * (self.rng.range(8.0, 28.0) + rise * 12.0)) * k;
            let color = if gore > 0.0 && self.rng.f32() < gore {
                BLOOD_COLORS[(self.rng.next_u32() % 4) as usize]
            } else {
                color
            };
            let life = self.rng.range(1.5, 3.0);
            self.particles.push(Particle::new(pos, vel, color, ParticleKind::Gib, life));
        }
    }

    /// Finds the pieces next to the removed voxels that no longer connect to
    /// the ground (the bottom layer) and lets them fall apart.
    pub fn drop_loose(&mut self, removed: &[IVec3]) {
        // What still stands next to what went, each cell once, in the order
        // the removed cells and their neighbours come (only reading: big
        // blasts on several cores, a chunk looked up once for its
        // neighbours).
        let world = &self.world;
        let candidates = crate::par::in_order(removed, REMOVED_A_THREAD, move |part| {
            let mut reader = world.reader();
            let mut seen = CellSet::default();
            let mut out = Vec::new();
            for &cell in part {
                for d in NEIGHBOURS {
                    let seed = cell + d;
                    if reader.get(seed) != 0 && seen.insert(seed) {
                        out.push(seed);
                    }
                }
            }
            out
        });
        let mut queued = CellSet::default();
        let seeds: Vec<IVec3> = candidates.into_iter().filter(|&seed| queued.insert(seed)).collect();
        let mut grounded = CellSet::default();
        let (mut fallen, mut middle) = (0usize, Vec3::ZERO);
        for seed in seeds {
            // A piece that came down meanwhile took it along.
            if self.world.get(seed) == 0 || grounded.contains(&seed) {
                continue;
            }
            if let Some(piece) = loose_piece(&self.world, seed, &mut grounded) {
                fallen += piece.len();
                middle += piece.iter().map(|c| c.as_vec3()).sum::<Vec3>();
                self.crumble(&piece);
            }
        }
        // Anything bigger than a few handfuls rumbles as it comes down.
        let width = self.cfg.physics.body_width;
        let handful = width * width * width;
        if fallen as f32 > handful {
            let loud = (fallen as f32 / (handful * 20.0)).clamp(0.3, 1.0);
            self.sound("crumble", middle / fallen as f32, loud);
        }
    }

    /// Turns every voxel of a piece into falling debris.
    pub fn crumble(&mut self, piece: &[IVec3]) {
        let k = self.effect_scale();
        for &cell in piece {
            let v = self.world.get(cell);
            self.world.set(cell, 0);
            if self.particles_full() {
                continue;
            }
            let mid = cell.as_vec3() + 0.5;
            let vel = Vec3::new(self.rng.range(-3.0, 3.0), self.rng.range(-3.0, 3.0), self.rng.range(-2.0, 4.0)) * k;
            let life = self.rng.range(4.0, 7.0);
            self.particles.push(Particle::new(mid, vel, self.world.palette[v as usize], ParticleKind::Debris(v), life));
        }
    }

    /// Debris, gibs, sparks, fire, smoke and dust move, bounce and fade;
    /// settled debris may become part of the world again, but never inside
    /// `keep_clear` (a box, lowest corner and highest: the player).
    pub fn update_particles(&mut self, keep_clear: (Vec3, Vec3)) {
        // Below this speed a landed piece stops bouncing.
        let resting = 14.0 * self.effect_scale();
        let gravity = self.cfg.physics.gravity;
        let world = &mut self.world;
        let rng = &mut self.rng;
        let settle = self.cfg.debris.settle;
        let (player_min, player_max) = keep_clear;

        self.particles.retain_mut(|q| {
            q.prev = q.pos;
            q.life -= DT;
            if q.life <= 0.0 {
                return false;
            }
            match q.kind {
                ParticleKind::Flash => return true,
                // Fire, smoke and dust float through everything and fade.
                ParticleKind::Fire => {
                    q.vel *= 0.9;
                    q.vel.z += gravity * 0.06 * DT;
                    q.pos += q.vel * DT;
                    return true;
                }
                ParticleKind::Smoke => {
                    q.vel *= 0.97;
                    q.vel.z += gravity * 0.02 * DT;
                    q.pos += q.vel * DT;
                    return true;
                }
                ParticleKind::Dust => {
                    q.vel *= 0.9;
                    q.vel.z -= gravity * 0.05 * DT;
                    q.pos += q.vel * DT;
                    return true;
                }
                _ => {}
            }
            if q.kind == ParticleKind::Spark {
                q.vel *= 0.94;
                q.vel.z -= gravity * 0.35 * DT;
            } else {
                q.vel.z -= gravity * DT;
            }
            let next = q.pos + q.vel * DT;
            if !world.blocks(next.floor().as_ivec3()) {
                q.pos = next;
                return true;
            }
            if q.kind == ParticleKind::Spark {
                return false;
            }

            // Resolve axis by axis so debris slides and bounces naturally.
            let mut pos = q.pos;
            let mut landed = false;
            for axis in [2, 0, 1] {
                let mut trial = pos;
                trial[axis] = next[axis];
                if world.blocks(trial.floor().as_ivec3()) {
                    if axis == 2 && q.vel.z < 0.0 {
                        landed = true;
                    }
                    q.vel[axis] *= -0.3;
                } else {
                    pos = trial;
                }
            }
            q.pos = pos;
            if !landed {
                return true;
            }
            q.vel.x *= 0.5;
            q.vel.y *= 0.5;
            if q.vel.length() > resting {
                return true;
            }
            let ParticleKind::Debris(voxel) = q.kind else {
                // Body parts lie still until they fade.
                q.vel = Vec3::ZERO;
                return true;
            };

            // Settled: maybe become part of the world again (never inside the player).
            let cell = q.pos.floor().as_ivec3();
            let mid = cell.as_vec3() + 0.5;
            let in_player = mid.cmpge(player_min - 0.5).all() && mid.cmple(player_max + 0.5).all();
            if !in_player && world.in_bounds(cell) && world.get(cell) == 0 && rng.f32() < settle {
                world.set(cell, voxel);
            }
            false
        });
    }
}

/// The filled cells of a blast's ball in the given layers, in order.
fn blast_cells(world: &World, layers: &[i32], lo: IVec3, hi: IVec3, centre: Vec3, reach: f32, bedrock: bool) -> Vec<(IVec3, u8)> {
    let mut reader = world.reader();
    let mut out = Vec::new();
    for &z in layers {
        if bedrock && z == 0 {
            continue;
        }
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let cell = IVec3::new(x, y, z);
                if (cell.as_vec3() + 0.5).distance_squared(centre) > reach {
                    continue;
                }
                let v = reader.get(cell);
                if v != 0 {
                    out.push((cell, v));
                }
            }
        }
    }
    out
}

/// Collects the solid piece connected to `seed`. Returns it if it does not
/// reach the bottom layer; otherwise (or when it is larger than MAX_LOOSE)
/// marks everything visited as grounded and returns None.
pub(crate) fn loose_piece(world: &World, seed: IVec3, grounded: &mut CellSet) -> Option<Vec<IVec3>> {
    let mut reader = world.reader();
    // Most of what a blast leaves stands on an unbroken column down to the
    // ground, or to something already known to: held, no search needed.
    let mut below = seed;
    while reader.get(below) != 0 {
        if below.z == 0 || grounded.contains(&below) {
            grounded.extend((below.z..=seed.z).map(|z| IVec3::new(seed.x, seed.y, z)));
            return None;
        }
        below.z -= 1;
    }
    let mut seen = CellSet::default();
    seen.insert(seed);
    let mut order = vec![seed];
    let mut stack = vec![seed];
    while let Some(cell) = stack.pop() {
        if cell.z == 0 || grounded.contains(&cell) || order.len() > MAX_LOOSE {
            grounded.extend(order);
            return None;
        }
        for d in NEIGHBOURS {
            let next = cell + d;
            if reader.get(next) != 0 && seen.insert(next) {
                order.push(next);
                stack.push(next);
            }
        }
    }
    Some(order)
}

/// The quarter turn a model shows for a movement direction: the dominant
/// axis, and on an exact diagonal whichever of the two it already shows.
pub fn quarter_turn(dir: Vec3, current: IVec2) -> IVec2 {
    let x = IVec2::new(dir.x.signum() as i32, 0);
    let y = IVec2::new(0, dir.y.signum() as i32);
    let (ax, ay) = (dir.x.abs(), dir.y.abs());
    if ax > ay + 0.01 {
        x
    } else if ay > ax + 0.01 {
        y
    } else if current == x || current == y {
        current
    } else {
        x
    }
}

pub fn spawn_point(world: &World, spawn: [f32; 2], height: f32, half: f32) -> Vec3 {
    let size = world.size.as_vec3();
    let x = if spawn[0] < 0.0 { size.x * 0.5 } else { spawn[0] };
    let y = if spawn[1] < 0.0 { size.y * 0.5 } else { spawn[1] };
    stand_on_ground(world, Vec3::new(x, y, 0.0), height, half)
}

/// The lowest free spot for a body at `pos` (x, y), standing on what is there.
pub fn stand_on_ground(world: &World, pos: Vec3, height: f32, half: f32) -> Vec3 {
    let mut pos = pos;
    let c = pos.floor().as_ivec3();
    let r = half.ceil() as i32 + 1;
    pos.z = 0.0;
    for dy in -r..=r {
        for dx in -r..=r {
            pos.z = pos.z.max(world.column_top(c.x + dx, c.y + dy) as f32);
        }
    }
    while body_hits(world, pos, half, height) && pos.z < world.size.z as f32 + 8.0 {
        pos.z += 1.0;
    }
    pos
}

/// What a kit's camera follows: its player, as its rules lead the view (at
/// every level's start), a thing the game names (`level.camera:follow`),
/// or nothing, resting where the game put it (`level.camera:look_at`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Watch {
    #[default]
    Player,
    Thing(things::Id),
    Still,
}

/// Whom an event is about: the player (whatever figure the rules give it)
/// or one of the things.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Who {
    Player,
    Thing(things::Id),
}

/// Something the ready-made rules did to a thing (`Sim::events`).
#[derive(Clone)]
pub enum Event {
    /// It was hurt (its hit points already lowered), by whom and with what
    /// weapon if known (`c.weapons`' name, `knife`).
    Hit { who: Who, by: Option<Who>, damage: i32, weapon: Option<String> },
    /// Its hit points ran out: its kind and body as they were (the rules
    /// may have taken it away already).
    Death { who: Who, by: Option<Who>, kind: usize, body: Body, weapon: Option<String> },
    /// It fell out of the level (well under it): the rules take a thing
    /// away after; the player's figure waits for what the game decides.
    FellOut { who: Who },
    /// It took an item of `c.items` into its inventory (`thing:take`).
    Pickup { who: Who, item: String },
}

/// Whether a body fell out of the level: `lost_below` voxels under it
/// (`c.physics.lost_below`), or down on the world's floor under it (the
/// ground below the world holds).
pub fn fell_out(world: &World, body: &Body, lost_below: f32) -> bool {
    let bottom = world.stage_box().0.z;
    body.pos.z < bottom && (body.pos.z < bottom - lost_below || (body.on_ground && body.pos.z < 1.0))
}

/// A body with its feet at `pos` (a level's place): up out of anything it
/// would stand in, then down onto what is below.
pub fn stand_at(world: &World, pos: Vec3, height: f32, half: f32) -> Vec3 {
    let mut pos = pos;
    while body_hits(world, pos, half, height) && pos.z < world.size.z as f32 + 8.0 {
        pos.z += 1.0;
    }
    while pos.z > 0.0 && !body_hits(world, pos - Vec3::Z, half, height) {
        pos.z -= 1.0;
    }
    pos
}

/// Whether a body at `pos` would overlap anything that blocks it; one-way
/// voxels let it through.
pub fn body_hits(world: &World, pos: Vec3, half_width: f32, height: f32) -> bool {
    body_hits_in(world, pos, half_width, height, None)
}

/// `body_hits`, and with `floor` (the feet's height before a move down)
/// one-way voxels whose top is at or below it hold too.
fn body_hits_in(world: &World, pos: Vec3, half_width: f32, height: f32, floor: Option<f32>) -> bool {
    let min = pos - Vec3::new(half_width, half_width, 0.0);
    let max = pos + Vec3::new(half_width, half_width, height);
    let lo = min.floor().as_ivec3();
    let hi = (max - 1e-3).floor().as_ivec3();
    for z in lo.z..=hi.z {
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                match world.hold(IVec3::new(x, y, z)) {
                    Hold::Solid => return true,
                    Hold::Above if floor.is_some_and(|f| (z + 1) as f32 <= f + 1e-3) => return true,
                    _ => {}
                }
            }
        }
    }
    false
}

/// The top of something a body can stand on that is not part of the world
/// (a moving platform): it holds only from above, like a one-way voxel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ledge {
    /// Its extent on the ground plane, from corner to corner.
    pub min: Vec2,
    pub max: Vec2,
    /// The height of its top.
    pub z: f32,
}

impl Ledge {
    /// Whether a body's footprint at `pos`, `half` its width, lies over it.
    pub fn under(&self, pos: Vec3, half: f32) -> bool {
        pos.x + half > self.min.x && pos.x - half < self.max.x && pos.y + half > self.min.y && pos.y - half < self.max.y
    }
}

/// Moves a body `half` its width across by its velocity for one tick: axis
/// by axis, in small sub-steps, stepping up low ledges while on the ground,
/// standing on one-way voxels and `ledges` only when it comes down onto
/// them from above.
pub fn move_body_in(world: &World, b: &mut Body, physics: &PhysicsConfig, half: f32, ledges: &[Ledge]) {
    let d = b.vel * DT;
    let steps = (d.abs().max_element() / 0.45).ceil().max(1.0) as i32;
    let sd = d / steps as f32;
    let was_on_ground = b.on_ground;

    for _ in 0..steps {
        for axis in 0..2 {
            if sd[axis] == 0.0 {
                continue;
            }
            let mut next = b.pos;
            next[axis] += sd[axis];
            if !body_hits(world, next, half, b.height) {
                b.pos = next;
                continue;
            }
            let climbed = was_on_ground
                && (1..=physics.step).any(|s| {
                    let up = next + Vec3::Z * s as f32;
                    let free = !body_hits(world, up, half, b.height);
                    if free {
                        b.pos = up;
                    }
                    free
                });
            if !climbed {
                b.vel[axis] = 0.0;
            }
        }

        if sd.z != 0.0 {
            let mut next = b.pos;
            next.z += sd.z;
            let down = sd.z < 0.0;
            // A ledge it comes down onto from above it stands on.
            let ledge = if down {
                ledges.iter().filter(|l| l.under(b.pos, half) && l.z <= b.pos.z + 1e-3 && l.z > next.z).map(|l| l.z).reduce(f32::max)
            } else {
                None
            };
            if body_hits_in(world, next, half, b.height, down.then_some(b.pos.z)) {
                if down {
                    // Landed: stand exactly on top of the voxel below.
                    b.pos.z = next.z.ceil();
                }
                b.vel.z = 0.0;
            } else if let Some(z) = ledge {
                b.pos.z = z;
                b.vel.z = 0.0;
            } else {
                b.pos = next;
            }
        }
    }

    b.on_ground = body_hits_in(world, b.pos - Vec3::Z * 0.05, half, b.height, Some(b.pos.z))
        || ledges.iter().any(|l| l.under(b.pos, half) && l.z <= b.pos.z + 1e-3 && l.z > b.pos.z - 0.05);
    if b.on_ground && b.vel.z < 0.0 {
        b.vel.z = 0.0;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Flat ground of the given size with a 1-voxel floor.
    pub(crate) fn flat_world(size: IVec3) -> World {
        let mut palette = [[0u8; 4]; 256];
        palette[1] = [200, 200, 200, 255];
        let mut world = World::new(size, palette);
        for y in 0..size.y {
            for x in 0..size.x {
                world.set_raw(IVec3::new(x, y, 0), 1);
            }
        }
        world
    }

    use crate::config::PhysicsConfig;

    /// Ticks a body with gravity until it rests or `ticks` run out.
    fn fall(world: &World, b: &mut Body, half: f32, ledges: &[Ledge], ticks: usize) {
        for _ in 0..ticks {
            b.prev = b.pos;
            b.advance_in(world, &PhysicsConfig::default(), half, ledges);
        }
    }

    /// A world with a floor and a plank across it at height 20: its top
    /// voxel one-way (palette 2), the two under it soft (palette 3).
    fn plank_world() -> World {
        let mut world = flat_world(IVec3::new(40, 8, 40));
        world.palette[2] = [150, 100, 60, 255];
        world.palette[3] = [140, 90, 50, 255];
        world.oneway[2] = true;
        world.soft[3] = true;
        for y in 0..8 {
            for x in 0..40 {
                world.set_raw(IVec3::new(x, y, 20), 2);
                world.set_raw(IVec3::new(x, y, 19), 3);
                world.set_raw(IVec3::new(x, y, 18), 3);
            }
        }
        world
    }

    /// A figure's frames follow what its body does, each set from when it
    /// began: unset, the model's own layout (0 standing, 1 in the air, the
    /// rest walking, a frame every half its height walked); its kind's
    /// sets by name, by time, by the way walked (`stride`) or once.
    #[test]
    fn a_figure_shows_frames_for_what_it_does() {
        use things::{FPS, Frames, figure_frame};
        let world = plank_world();
        let mut b = Body::new(Vec3::new(10.0, 4.0, 21.0), 6.0);
        let step = |b: &mut Body, across: f32| {
            b.vel.x = across;
            b.prev = b.pos;
            b.advance_in(&world, &PhysicsConfig::default(), 1.5, &[]);
        };
        let set = |first, last, fps, stride, looped| Frames { first, last, fps, stride, looped };
        let none = std::collections::BTreeMap::new();
        let sets = std::collections::BTreeMap::from([
            ("idle".to_string(), set(7, 8, 60.0, None, true)),
            ("walk".to_string(), set(5, 6, FPS, Some(2.0), true)),
            ("jump".to_string(), set(9, 10, 60.0, None, false)),
        ]);
        step(&mut b, 0.0);
        assert!(b.on_ground);
        assert_eq!((figure_frame(&none, &b, 5, 8.0), figure_frame(&sets, &b, 11, 8.0)), (0, 7), "standing");
        step(&mut b, 0.0);
        assert_eq!(figure_frame(&sets, &b, 11, 8.0), 8, "idle goes on, a frame a tick");
        for _ in 0..7 {
            step(&mut b, 60.0);
            assert!(b.walked > 0.0);
            assert_eq!(figure_frame(&none, &b, 5, 8.0), 1 + (b.walked / 4.0) as usize % 4, "a frame every 4 voxels: {}", b.walked);
            assert_eq!(figure_frame(&sets, &b, 11, 8.0), 5 + (b.walked / 2.0) as usize % 2, "a frame every 2 voxels: {}", b.walked);
        }
        assert_eq!(figure_frame(&sets, &b, 1, 8.0), 0, "one frame alone shows always");
        b.on_ground = false;
        b.vel.z = 100.0;
        let mut shown = Vec::new();
        for _ in 0..4 {
            step(&mut b, 0.0);
            shown.push((figure_frame(&none, &b, 5, 8.0), figure_frame(&sets, &b, 11, 8.0)));
        }
        assert_eq!(shown, [(1, 9), (1, 10), (1, 10), (1, 10)], "in the air: the jump once, its last frame held");
    }

    /// A one-way voxel holds a body that comes down onto it and lets one
    /// through that jumps up into it or walks into it from the side.
    #[test]
    fn one_way_voxels_hold_from_above_only() {
        let world = plank_world();
        let mut b = Body::new(Vec3::new(10.0, 4.0, 1.0), 6.0);
        b.on_ground = true;
        b.vel.z = 160.0;
        let mut highest = 0.0f32;
        for _ in 0..150 {
            b.prev = b.pos;
            b.advance_in(&world, &PhysicsConfig::default(), 1.5, &[]);
            highest = highest.max(b.pos.z);
        }
        assert!(highest > 21.0, "the jump stopped under the plank at {highest}");
        assert_eq!((b.pos.z, b.on_ground), (21.0, true), "it did not land on the plank");
        // A body tall enough to reach into the plank walks through it.
        let mut walker = Body::new(Vec3::new(10.0, 4.0, 1.0), 25.0);
        walker.on_ground = true;
        for _ in 0..20 {
            walker.vel.x = 60.0;
            walker.prev = walker.pos;
            walker.advance_in(&world, &PhysicsConfig::default(), 1.5, &[]);
        }
        assert!(walker.pos.x > 20.0 && walker.pos.z == 1.0, "the plank stopped a body walking into it: {}", walker.pos);
    }

    /// Only the top of a thick one-way tile holds: a body whose feet are
    /// inside it falls through, one coming from above stands on its top.
    #[test]
    fn a_thick_one_way_tile_holds_only_on_top() {
        let world = plank_world();
        let mut inside = Body::new(Vec3::new(10.0, 4.0, 19.0), 6.0);
        fall(&world, &mut inside, 1.5, &[], 60);
        assert_eq!(inside.pos.z, 1.0, "a body inside the plank stood in it");
        let mut above = Body::new(Vec3::new(10.0, 4.0, 30.0), 6.0);
        fall(&world, &mut above, 1.5, &[], 60);
        assert_eq!((above.pos.z, above.on_ground), (21.0, true));
    }

    /// A wider body stops further from a wall: each body has its own width.
    #[test]
    fn a_wider_body_meets_walls_sooner() {
        let mut world = flat_world(IVec3::new(40, 16, 20));
        for z in 1..10 {
            for y in 0..16 {
                world.set_raw(IVec3::new(30, y, z), 1);
            }
        }
        let stop = |half: f32| {
            let mut b = Body::new(Vec3::new(10.0, 8.0, 1.0), 6.0);
            b.on_ground = true;
            for _ in 0..60 {
                b.vel.x = 60.0;
                b.prev = b.pos;
                b.advance_in(&world, &PhysicsConfig::default(), half, &[]);
            }
            b.pos.x
        };
        let (thin, wide) = (stop(1.5), stop(6.0));
        assert!((30.0 - thin - 1.5).abs() < 0.5 && (30.0 - wide - 6.0).abs() < 0.5, "thin {thin}, wide {wide}");
    }

    /// A body comes down onto a ledge (the top of a moving platform) and
    /// stands on it; one jumping up from under it passes through.
    #[test]
    fn a_body_lands_on_a_ledge() {
        let world = flat_world(IVec3::new(40, 8, 60));
        let ledge = [Ledge { min: Vec2::new(0.0, 0.0), max: Vec2::new(20.0, 8.0), z: 30.0 }];
        let mut b = Body::new(Vec3::new(10.0, 4.0, 45.0), 6.0);
        fall(&world, &mut b, 1.5, &ledge, 60);
        assert_eq!((b.pos.z, b.on_ground), (30.0, true));
        let mut beside = Body::new(Vec3::new(30.0, 4.0, 45.0), 6.0);
        fall(&world, &mut beside, 1.5, &ledge, 60);
        assert_eq!(beside.pos.z, 1.0, "a ledge held a body beside it");
        let mut jumper = Body::new(Vec3::new(10.0, 4.0, 1.0), 6.0);
        jumper.on_ground = true;
        jumper.vel.z = 160.0;
        let mut highest = 0.0f32;
        for _ in 0..80 {
            jumper.prev = jumper.pos;
            jumper.advance_in(&world, &PhysicsConfig::default(), 1.5, &ledge);
            highest = highest.max(jumper.pos.z);
        }
        assert!(highest > 30.0 && jumper.pos.z == 30.0, "the jump through the ledge: highest {highest}, at {}", jumper.pos.z);
    }

    /// A floor 40 by 40 with a pillar at (20, 20) from the floor to z 20
    /// and a beam across its top along x, 5 to 35: what a blast in the
    /// pillar cuts loose.
    fn pillar_world() -> World {
        let mut world = flat_world(IVec3::new(40, 40, 32));
        for z in 1..=20 {
            world.set_raw(IVec3::new(20, 20, z), 1);
        }
        for x in 5..=35 {
            world.set_raw(IVec3::new(x, 20, 21), 1);
        }
        world
    }

    /// A blast takes a ball of voxels (the bedrock layer kept), throws
    /// some of them as debris, and what it cut off the ground falls; the
    /// same blast on the same world comes out the same, voxel and particle.
    #[test]
    fn a_blast_takes_its_ball_and_what_it_cuts_off_falls() {
        let blast = || {
            let mut sim = Sim::new(pillar_world(), &GameConfig::default(), Vec3::ZERO);
            sim.explode(Vec3::new(20.5, 20.5, 2.5), 3.0);
            sim
        };
        let sim = blast();
        let w = &sim.world;
        assert_eq!(w.get(IVec3::new(20, 20, 0)), 1, "the bedrock layer stays");
        assert_eq!(w.get(IVec3::new(20, 20, 2)), 0, "the ball goes");
        assert_eq!(w.get(IVec3::new(25, 20, 0)), 1, "the floor beside it stays (its layer is bedrock)");
        assert!((5..=35).all(|x| w.get(IVec3::new(x, 20, 21)) == 0), "the beam came down with the pillar's top");
        assert!((6..=20).all(|z| w.get(IVec3::new(20, 20, z)) == 0), "the pillar's top fell");
        assert!(sim.particles.iter().any(|p| matches!(p.kind, ParticleKind::Debris(1))));
        assert!(sim.sounds.iter().any(|s| s.name == "crumble"), "a big piece rumbles");
        let again = blast();
        assert_eq!(again.world.content_hash(), sim.world.content_hash());
        assert_eq!(again.particles.len(), sim.particles.len());
        assert!(again.particles.iter().zip(&sim.particles).all(|(a, b)| a.pos == b.pos && a.vel == b.vel));
        // No more particles than the game allows.
        let mut cfg = GameConfig::default();
        cfg.debris.max_particles = 10;
        let mut few = Sim::new(pillar_world(), &cfg, Vec3::ZERO);
        few.explode(Vec3::new(20.5, 20.5, 2.5), 3.0);
        assert!(few.particles.len() <= 10, "{}", few.particles.len());
        assert_eq!(few.world.content_hash(), sim.world.content_hash(), "the world comes out the same whatever the particles");
    }

    /// A piece hangs on by a corner (26 neighbours); one cut loose is
    /// found and falls; one bigger than MAX_LOOSE is taken as held (the
    /// search stops), and so is all it reached.
    #[test]
    fn a_loose_piece_is_found_up_to_its_limit() {
        let world = pillar_world();
        let mut grounded = CellSet::default();
        assert!(loose_piece(&world, IVec3::new(5, 20, 21), &mut grounded).is_none(), "held by the pillar");
        assert!(grounded.contains(&IVec3::new(5, 20, 21)));
        let mut cut = world.clone();
        cut.set_raw(IVec3::new(20, 20, 10), 0);
        let piece = loose_piece(&cut, IVec3::new(5, 20, 21), &mut CellSet::default()).expect("cut loose");
        assert_eq!(piece.len(), 31 + 10, "the beam and the pillar above the cut");
        // A corner holds: a voxel diagonally on the pillar's top hangs on.
        let mut corner = world.clone();
        corner.set_raw(IVec3::new(21, 21, 22), 1);
        assert!(loose_piece(&corner, IVec3::new(21, 21, 22), &mut CellSet::default()).is_none());
        // A floating slab bigger than the limit: no search goes so far.
        let side = (MAX_LOOSE as f32).sqrt() as i32 + 2;
        let mut big = World::new(IVec3::new(side, side, 8), flat_world(IVec3::ONE).palette);
        for y in 0..side {
            for x in 0..side {
                big.set_raw(IVec3::new(x, y, 5), 1);
            }
        }
        let mut grounded = CellSet::default();
        assert!(loose_piece(&big, IVec3::new(0, 0, 5), &mut grounded).is_none(), "too big to fall");
        assert!(grounded.len() > MAX_LOOSE, "what it reached counts as held: {}", grounded.len());
        let mut small = World::new(IVec3::new(8, 8, 8), big.palette);
        small.set_raw(IVec3::new(3, 3, 5), 1);
        assert_eq!(loose_piece(&small, IVec3::new(3, 3, 5), &mut CellSet::default()), Some(vec![IVec3::new(3, 3, 5)]));
    }

    /// A model turns to the way it goes; on an exact diagonal it keeps
    /// the side it shows; a body standing at a place is lifted out of the
    /// ground and set down on it; it fell out only below the level.
    #[test]
    fn turning_standing_and_falling_out() {
        assert_eq!(quarter_turn(Vec3::new(1.0, 0.2, 0.0), IVec2::Y), IVec2::X);
        assert_eq!(quarter_turn(Vec3::new(-0.1, -1.0, 0.0), IVec2::X), IVec2::NEG_Y);
        assert_eq!(quarter_turn(Vec3::new(1.0, 1.0, 0.0), IVec2::Y), IVec2::Y, "a diagonal keeps the side shown");
        assert_eq!(quarter_turn(Vec3::new(1.0, -1.0, 0.0), IVec2::Y), IVec2::X);
        let world = pillar_world();
        assert_eq!(stand_at(&world, Vec3::new(10.5, 10.5, 0.0), 6.0, 1.0).z, 1.0, "lifted out of the floor");
        assert_eq!(stand_at(&world, Vec3::new(10.5, 10.5, 15.0), 6.0, 1.0).z, 1.0, "down onto it");
        assert_eq!(stand_at(&world, Vec3::new(20.5, 20.5, 5.0), 6.0, 1.0).z, 22.0, "up out of the pillar onto the beam");
        let mut b = Body::new(Vec3::new(10.0, 10.0, -5.0), 6.0);
        assert!(!fell_out(&world, &b, 20.0));
        b.pos.z = -25.0;
        assert!(fell_out(&world, &b, 20.0));
    }
}

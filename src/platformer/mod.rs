//! The platformer: a kit for jump & runs seen from the side (`c.kit =
//! "platformer"`), building blocks for many (Voxkeep and the jump & run
//! template are built on it). Its behaviours run as systems in a fixed
//! order every tick (platforms, the hero, walkers, hoppers, flyers, then
//! what the hero touches), each over its own things in the order they
//! came, and its state hash takes the things in that order: keep both, or
//! a run plays differently.
//!
//! The level is seen from the side (a Tiled map, `tiled`): things move
//! along x and z in the lane, y stays where the marker put them. The hero
//! runs, jumps (higher the longer the jump is held, a moment after leaving
//! a ledge still, a moment before landing already), climbs the map's
//! ladders (a ladder's top holds from above, as a plank: up a ladder
//! through a floor it stands there, down takes it down again) and rides
//! platforms; hurt (`thing:hurt`) it loses a heart, is thrown back and
//! safe for a moment, and with the last it lies still. With `air_jumps`
//! it jumps again in the air (a double jump); with `grab` it catches a
//! ledge's edge too high to jump onto and pulls itself up (Prince of
//! Persia's way: everything stays in the one lane).
//! What a touch means (a stomp, a pickup, a door, spikes), a fall out of
//! the level and a life lost are the game's code (`docs/game-model.md`):
//! the kit only moves things and reports what fell out.

pub mod kinds;
mod script;

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{IVec2, IVec3, Vec2, Vec3};

use kinds::{KindRules, Rules};
use crate::audio::synth::{Synth, Wave};
use crate::config::GameConfig;
use crate::controls::Bindings;
use crate::game::{Game, ModelSpec, Outcome, Setup};
use crate::input::Input;
use crate::session::{Drawing, Models};
use crate::sim::things::{Id, Things, Walk};
use crate::sim::{Body, DT, Event, Ledge, Sim, Who, body_hits};
use crate::world::{Hold, Marker, World};

/// The behaviours a kind may have (`c.things.NAME.behaviour`).
pub const BEHAVIOURS: [&str; 8] = ["hero", "walker", "hopper", "flyer", "pickup", "platform", "prop", "crate"];

/// Its actions besides the four directions.
pub const ACTIONS: [&str; 1] = ["jump"];
const JUMP: usize = 0;

/// The sound events it makes.
const SOUNDS: [&str; 7] = ["jump", "stomp", "hurt", "pickup", "door", "checkpoint", "lose"];

/// How long the hero shows hurt after a hit, in ticks.
const HURT_SHOWN: u32 = 20;

/// What a thing is to the platformer's systems.
#[derive(Clone, Debug)]
pub enum Brain {
    /// A Lua game's script moves it.
    Script(Walk),
    Hero(Hero),
    Walker { way: f32 },
    Hopper { wait: u32, way: f32 },
    Flyer { home: Vec3, age: u32 },
    Pickup,
    Platform(Mover),
    /// What it shows (`thing.state.pose`: a set of its kind's frames), and
    /// since which tick (only for the picture: a door opening once).
    Prop { pose: String, since: u64 },
    /// Pushed by the hero, falling, stood on.
    Crate,
}

/// The hero's state between ticks.
#[derive(Clone, Debug, Default)]
pub struct Hero {
    /// Ticks since it last stood on something (coyote time).
    air: u32,
    /// Ticks since jump went down (the jump buffer).
    asked: u32,
    /// Rising from a jump: letting go cuts it short.
    jumping: bool,
    /// Jump held the tick before.
    held: bool,
    crouch: bool,
    climbing: bool,
    /// Ticks left that nothing hurts it.
    hurt: u32,
    /// The platform it stands on.
    riding: Option<Id>,
    /// Jumps left before it lands (`air_jumps`).
    air_left: u32,
    /// Hanging from a ledge's edge: where it will stand once pulled up.
    edge: Option<Vec3>,
    /// Ticks into pulling itself up (0: not pulling), from where it hung.
    pulling: u32,
    from: Vec3,
    /// What it shows, and for how many ticks now.
    anim: Anim,
    age: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Anim {
    #[default]
    Idle,
    Run,
    Jump,
    Fall,
    Crouch,
    Hurt,
    Climb,
    Hang,
    Pull,
}

impl Anim {
    fn name(self) -> &'static str {
        match self {
            Anim::Idle => "idle",
            Anim::Run => "run",
            Anim::Jump => "jump",
            Anim::Fall => "fall",
            Anim::Crouch => "crouch",
            Anim::Hurt => "hurt",
            Anim::Climb => "climb",
            Anim::Hang => "hang",
            Anim::Pull => "pull",
        }
    }
}

/// The hero's animations by name (`Anim::name`); a walker plays walk, a
/// hopper idle and jump, a flyer fly, a pickup spin, a platform idle, and
/// every figure the poses the game sets (`Platformer::frame`).
#[cfg(test)]
const HERO_PLAYS: [&str; 9] = ["idle", "run", "jump", "fall", "crouch", "hurt", "climb", "hang", "pull"];

/// A platform on its way: where it started, how far along (0..1) and which
/// way, ticks left to wait, and where it stands (whole voxels).
#[derive(Clone, Debug)]
pub struct Mover {
    home: Vec3,
    along: f32,
    back: bool,
    wait: u32,
    at: Vec3,
    /// The end it was told to go to and stay at (`thing.state.goal`: true
    /// the far one), None: to and fro.
    goal: Option<bool>,
}

/// How the run goes: what the kit keeps of it (lives and the like are the
/// game's).
#[derive(Clone, Debug)]
pub struct Status {
    pub score: u32,
    /// The hero's hit points (`player.state.hp`, from `c.state.player.hp`).
    pub hp: u32,
    pub outcome: Option<Outcome>,
}

/// The platforms' tops this tick, and how far each moved.
type Platforms = (Vec<(Id, Ledge)>, Vec<(Id, Vec3)>);

#[derive(Clone)]
pub struct Platformer {
    pub sim: Sim,
    pub rules: Arc<Rules>,
    pub things: Things<Brain>,
    /// The thing the player steers.
    pub hero: Option<Id>,
    pub status: Status,
    /// The heroes of the seats after the first (`c.players`; seat 2
    /// first, none while it has none), and what each holds this tick.
    pub seat_heroes: Vec<Option<Id>>,
    pub seat_inputs: Vec<Option<Input>>,
    /// A seat's hero is being placed (it is no seat 1's).
    seating: bool,
    /// What the camera follows (`level.camera`).
    pub watch: crate::sim::Watch,
    /// How many frames each model has.
    frames: Arc<BTreeMap<String, usize>>,
    /// The level's markers, to start over from.
    markers: Arc<Vec<Marker>>,
}

/// The sound each event makes unless `c.sounds` says otherwise.
fn default_sound(name: &str) -> Synth {
    use Wave::*;
    let s = Synth::default();
    match name {
        "jump" => Synth { wave: Square, freq: 330.0, freq_end: Some(660.0), duty: 0.5, sustain: 0.05, decay: 0.08, volume: 0.22, ..s },
        "stomp" => Synth { wave: Square, freq: 220.0, freq_end: Some(110.0), duty: 0.5, sustain: 0.03, decay: 0.12, punch: 0.5, volume: 0.3, ..s },
        "hurt" => Synth { wave: Saw, freq: 300.0, freq_end: Some(120.0), sustain: 0.08, decay: 0.15, volume: 0.3, ..s },
        "pickup" => Synth { wave: Triangle, freq: 880.0, sustain: 0.04, decay: 0.15, arp: 1.5, arp_at: 0.04, volume: 0.3, ..s },
        "door" => Synth { wave: Triangle, freq: 523.0, sustain: 0.2, decay: 0.5, arp: 1.26, arp_at: 0.1, volume: 0.32, ..s },
        "checkpoint" => Synth { wave: Triangle, freq: 660.0, sustain: 0.06, decay: 0.2, arp: 1.33, arp_at: 0.06, volume: 0.25, ..s },
        "lose" => Synth { wave: Square, freq: 440.0, freq_end: Some(110.0), duty: 0.5, sustain: 0.3, decay: 0.4, volume: 0.25, ..s },
        _ => s,
    }
}

impl Platformer {
    pub fn new(world: World, cfg: &GameConfig, rules: Rules, frames: BTreeMap<String, usize>, markers: &[Marker]) -> Self {
        let mut game = Self {
            sim: Sim::new(world, cfg, Vec3::ZERO),
            rules: Arc::new(rules),
            things: Things::default(),
            hero: None,
            status: Status { score: 0, hp: 0, outcome: None },
            seat_heroes: vec![None; cfg.players.max(1) as usize - 1],
            seat_inputs: Vec::new(),
            seating: false,
            watch: Default::default(),
            frames: Arc::new(frames),
            markers: Arc::new(markers.to_vec()),
        };
        game.start();
        game
    }

    /// Everything placed anew from the level's markers.
    fn start(&mut self) {
        self.things.clear();
        self.hero = None;
        self.seat_heroes.fill(None);
        for m in self.markers.clone().iter() {
            self.place(m.kind, m.cell.as_vec3() + Vec3::new(0.5, 0.5, 0.0));
        }
        self.status.hp = self.sim.cfg.state.player_number("hp").unwrap_or(0).max(0) as u32;
        self.look_at_hero();
    }

    /// Every hero: seat 1's, then the other seats' in order.
    fn heroes(&self) -> Vec<Id> {
        self.hero.into_iter().chain(self.seat_heroes.iter().flatten().copied()).collect()
    }

    /// The kind of the heroes of the seats after the first: seat 1's
    /// hero's, else the first kind with the hero's rules.
    pub(crate) fn hero_kind(&self) -> Option<usize> {
        self.hero.and_then(|id| self.things.get(id)).map(|t| t.kind).or_else(|| self.rules.kinds.iter().position(|k| matches!(k.rules, KindRules::Hero(_))))
    }

    /// The hero's mind, if there is a hero.
    pub(crate) fn hero_brain(&self) -> Option<&Hero> {
        match &self.things.get(self.hero?)?.brain {
            Brain::Hero(h) => Some(h),
            _ => None,
        }
    }

    fn hero_rules(&self) -> Option<kinds::HeroRules> {
        let t = self.things.get(self.hero?)?;
        match &self.rules.kinds[t.kind].rules {
            KindRules::Hero(r) => Some(r.clone()),
            _ => None,
        }
    }

    /// The camera on the hero at once.
    fn look_at_hero(&mut self) {
        if self.watch != crate::sim::Watch::Player {
            return;
        }
        if let Some(t) = self.hero.and_then(|id| self.things.get(id)) {
            let target = t.body.pos + Vec3::Z * t.body.height * 0.5;
            (self.sim.cam_target, self.sim.cam_prev) = (target, target);
        }
    }

    /// A thing of `kind` at `at`: lifted out of anything solid, and what
    /// walks or stands (a prop that `stands`: a door, spikes) dropped onto
    /// the floor below; figures face the middle of the level, the rest the
    /// camera.
    pub fn place(&mut self, kind: usize, at: Vec3) -> Id {
        let k = &self.rules.kinds[kind];
        let world = &self.sim.world;
        let half = k.width * 0.5;
        let mut at = at;
        // (A platform may lie in a wall or a floor: it comes out of it.)
        while !matches!(k.rules, KindRules::Platform(_)) && body_hits(world, at, half, k.height) && at.z < world.size.z as f32 + 8.0 {
            at.z += 1.0;
        }
        let stands = match &k.rules {
            KindRules::Hero(_) | KindRules::Walker(_) | KindRules::Hopper(_) | KindRules::Script | KindRules::Crate(_) => true,
            KindRules::Prop(p) => p.stands,
            _ => false,
        };
        if stands {
            while at.z > 0.0 && !body_hits(world, at - Vec3::Z, half, k.height) {
                at.z -= 1.0;
            }
        }
        let (lo, hi) = world.stage_box();
        let toward = if at.x > (lo.x + hi.x) * 0.5 { -1.0 } else { 1.0 };
        let brain = match &k.rules {
            KindRules::Script => Brain::Script(Walk::default()),
            KindRules::Hero(_) => Brain::Hero(Hero { asked: u32::MAX / 2, air: u32::MAX / 2, ..Hero::default() }),
            KindRules::Walker(_) => Brain::Walker { way: toward },
            KindRules::Hopper(h) => Brain::Hopper { wait: h.every, way: toward },
            KindRules::Flyer(_) => Brain::Flyer { home: at, age: 0 },
            KindRules::Pickup => Brain::Pickup,
            KindRules::Platform(_) => Brain::Platform(Mover { home: at, along: 0.0, back: false, wait: 0, at, goal: None }),
            KindRules::Prop(_) => Brain::Prop { pose: String::new(), since: 0 },
            KindRules::Crate(_) => Brain::Crate,
        };
        let figure = matches!(k.rules, KindRules::Hero(_) | KindRules::Walker(_) | KindRules::Hopper(_) | KindRules::Flyer(_));
        let mut body = Body::new(at, k.height);
        match k.facing {
            // The way its kind says (a bat facing the camera, wings spread).
            Some([x, y]) => body.face(Vec3::new(x, y, 0.0)),
            None if figure => body.face(Vec3::new(toward, 0.0, 0.0)),
            None => {}
        }
        let hero = matches!(k.rules, KindRules::Hero(_));
        let id = self.things.spawn(kind, body, k.hp, brain);
        if hero && self.hero.is_none() && !self.seating {
            self.hero = Some(id);
        }
        id
    }

    /// One tick of its rules.
    pub fn run(&mut self, input: &Input) {
        if !self.sim.begin_tick() {
            return;
        }
        for t in self.things.iter_mut() {
            t.body.prev = t.body.pos;
        }
        if self.status.outcome.is_none() {
            let (mut ledges, moves) = self.platforms();
            self.crates(&mut ledges);
            self.hero_steps(input, &ledges, &moves);
            self.push_crates();
            self.walkers(&ledges);
            self.hoppers(&ledges);
            self.flyers();
            self.fell_out();
        }
        let keep_clear = match self.hero.and_then(|id| self.things.get(id)) {
            Some(t) => {
                let half = self.rules.kinds[t.kind].width * 0.5;
                (t.body.pos - Vec3::new(half, half, 0.0), t.body.pos + Vec3::new(half, half, t.body.height))
            }
            None => (Vec3::splat(-1.0), Vec3::splat(-1.0)),
        };
        self.sim.update_particles(keep_clear);
        self.sim.update_flares();
        match self.watch {
            crate::sim::Watch::Player => {
                if let Some(t) = self.hero.or_else(|| self.seat_heroes.iter().flatten().next().copied()).and_then(|id| self.things.get(id)) {
                    let (mut feet, look) = (t.body.pos, t.body.look.x as f32);
                    // Several heroes standing: the middle of them, across.
                    let standing: Vec<f32> = self.seat_heroes.iter().flatten().filter_map(|&id| self.things.get(id)).filter(|t| t.hp > 0).map(|t| t.body.pos.x).collect();
                    if !standing.is_empty() && self.status.hp > 0 {
                        let (lo, hi) = standing.iter().fold((feet.x, feet.x), |(lo, hi), &x| (lo.min(x), hi.max(x)));
                        feet.x = (lo + hi) * 0.5;
                    }
                    self.sim.follow(feet);
                    if let Some(r) = self.hero_rules() {
                        self.sim.lead_toward(Vec3::X * look * r.look_ahead, r.look_ahead_speed);
                    }
                }
            }
            // What the game named, without the hero's look ahead; a thing
            // gone: the hero again.
            crate::sim::Watch::Thing(id) => match self.things.get(id).map(|t| t.body.pos) {
                Some(at) => self.follow_still(at),
                None => self.watch = crate::sim::Watch::Player,
            },
            crate::sim::Watch::Still => self.follow_still(self.sim.cam_target),
        }
    }

    /// The camera towards `at`, its lead easing back to none.
    fn follow_still(&mut self, at: Vec3) {
        self.sim.follow(at);
        let speed = self.hero_rules().map_or(0.0, |r| r.look_ahead_speed);
        self.sim.lead_toward(Vec3::ZERO, speed);
    }

    /// The platforms move a step (in whole voxels); their tops, and how
    /// far each moved. One coming down onto a body under it (the hero, a
    /// crate, a walker, a hopper) waits above it, unless it crushes: then
    /// it hurts the hero and goes on.
    fn platforms(&mut self) -> Platforms {
        let (mut ledges, mut moves) = (Vec::new(), Vec::new());
        let under: Vec<(Id, Vec3, f32, f32)> = self
            .things
            .iter()
            .filter(|t| matches!(t.brain, Brain::Hero(_) | Brain::Walker { .. } | Brain::Hopper { .. } | Brain::Crate))
            .map(|t| (t.id, t.body.pos, self.rules.kinds[t.kind].width * 0.5, t.body.height))
            .collect();
        let mut crushed = Vec::new();
        for t in self.things.iter_mut().filter(|t| t.off.is_none()) {
            let k = &self.rules.kinds[t.kind];
            let (KindRules::Platform(r), Brain::Platform(m)) = (&k.rules, &mut t.brain) else { continue };
            let travel = Vec2::from(r.travel);
            let length = travel.length().max(1.0);
            let was = m.clone();
            if let Some(to_end) = m.goal {
                // Told where to go: there, and it stays; let go, it goes on
                // back the way it came.
                let (step, target) = (r.speed * DT / length, if to_end { 1.0 } else { 0.0 });
                m.along = if m.along < target { (m.along + step).min(target) } else { (m.along - step).max(target) };
                (m.wait, m.back) = (0, to_end);
            } else if m.wait > 0 {
                m.wait -= 1;
            } else {
                let step = r.speed * DT / length;
                m.along += if m.back { -step } else { step };
                if m.along >= 1.0 || m.along <= 0.0 {
                    m.along = m.along.clamp(0.0, 1.0);
                    m.back = !m.back;
                    m.wait = r.pause;
                }
            }
            let (half_x, half_y) = (k.size.x as f32 * 0.5, k.size.y as f32 * 0.5);
            let offset = (travel * m.along).round();
            let mut at = m.home + Vec3::new(offset.x, 0.0, offset.y);
            if at.z < m.at.z {
                // Down onto a body under it (not one standing on it).
                let top = at.z + k.size.z as f32;
                let onto = under.iter().find(|&&(_, p, half, height)| (p.x - at.x).abs() < half + half_x && p.z < top - 0.5 && p.z + height > at.z);
                match onto {
                    Some(&(id, ..)) if r.crushes => {
                        if Some(id) == self.hero {
                            crushed.push((t.id, at.x));
                        }
                    }
                    Some(_) => {
                        *m = was;
                        at = m.at;
                    }
                    None => {}
                }
            }
            let moved = at - m.at;
            m.at = at;
            t.body.pos = at;
            // Its model across the level and through the lane (turned to the camera).
            ledges.push((t.id, Ledge { min: Vec2::new(at.x - half_x, at.y - half_y), max: Vec2::new(at.x + half_x, at.y + half_y), z: at.z + k.size.z as f32 }));
            if moved != Vec3::ZERO {
                moves.push((t.id, moved));
            }
        }
        // Crushed: a heart less, thrown out from under it.
        for (platform, x) in crushed {
            if let Some(r) = self.hero_rules() {
                self.hurt(x, &r, 1, Some(Who::Thing(platform)), None);
            }
        }
        (ledges, moves)
    }

    /// The hero: steered by the player, carried by its platform, climbing,
    /// jumping, falling.
    fn hero_steps(&mut self, input: &Input, ledges: &[(Id, Ledge)], moves: &[(Id, Vec3)]) {
        if let Some(id) = self.hero {
            self.hero_step(id, input, self.status.hp == 0, ledges, moves);
        }
        // The other seats' heroes, each by its own keys and hearts.
        for i in 0..self.seat_heroes.len() {
            let Some(id) = self.seat_heroes[i] else { continue };
            let input = self.seat_inputs.get(i).copied().flatten().unwrap_or_default();
            let down = self.things.get(id).is_none_or(|t| t.hp <= 0);
            self.hero_step(id, &input, down, ledges, moves);
        }
    }

    /// One hero's step, by `input`; `down` (no heart left) it lies still:
    /// nothing the player presses moves it, until the game gives it hearts
    /// again.
    fn hero_step(&mut self, id: Id, input: &Input, down: bool, ledges: &[(Id, Ledge)], moves: &[(Id, Vec3)]) {
        let still = Input::default();
        let input = if down { &still } else { input };
        let cfg = self.sim.cfg.clone();
        let rules = self.rules.clone();
        let tops: Vec<Ledge> = ledges.iter().map(|(_, l)| *l).collect();
        let mut sounds = Vec::new();
        let world = &self.sim.world;
        let Some(t) = self.things.get_mut(id).filter(|t| t.off.is_none()) else { return };
        let k = &rules.kinds[t.kind];
        let (KindRules::Hero(r), Brain::Hero(h)) = (&k.rules, &mut t.brain) else { return };
        let b = &mut t.body;
        if let Some(d) = h.riding.and_then(|p| moves.iter().find(|(i, _)| *i == p)).map(|(_, d)| *d) {
            b.pos += d;
        }
        let dir = input.right as i32 - input.left as i32;
        let jump = input.held(JUMP);
        let pressed = jump && !h.held;
        h.held = jump;
        h.asked = if pressed { 0 } else { h.asked.saturating_add(1) };
        h.air = if b.on_ground { 0 } else { h.air.saturating_add(1) };
        h.hurt = h.hurt.saturating_sub(1);
        let half = k.width * 0.5;
        let g = cfg.physics.gravity;
        // Hanging from a ledge's edge: up or jump pulls it up (rising first,
        // then over the edge), down lets go; the body neither falls nor
        // steers meanwhile.
        let hung = h.edge.is_some();
        if let Some(stand) = h.edge {
            b.vel = Vec3::ZERO;
            if h.pulling == 0 && (pressed || input.up) {
                (h.pulling, h.from) = (1, b.pos);
            } else if h.pulling == 0 && input.down {
                h.edge = None;
            }
            if h.pulling > 0 {
                let done = h.pulling as f32 / r.pull.max(1) as f32;
                let (up, over) = ((done / 0.6).min(1.0), ((done - 0.6) / 0.4).clamp(0.0, 1.0));
                b.pos = Vec3::new((h.from.x + (stand.x - h.from.x) * over).round(), h.from.y, (h.from.z + (stand.z - h.from.z) * up).round());
                h.pulling += 1;
                if h.pulling > r.pull.max(1) {
                    (b.pos, b.on_ground) = (stand, true);
                    (h.edge, h.pulling, h.air) = (None, 0, 0);
                }
            }
        }
        if b.on_ground || h.climbing || hung {
            h.air_left = r.air_jumps;
        }
        let on_ladder = |p: Vec3| world.cells.as_ref().is_some_and(|g| g.at(p) & crate::grid::LADDER != 0);
        let ladder = !hung && on_ladder(b.centre());
        // A ladder just under the feet: climbed on up to its top, or down
        // from there.
        let below = !hung && on_ladder(b.pos - Vec3::Z * 0.5);
        if h.climbing && (!(ladder || below) || pressed) {
            h.climbing = false;
        }
        if !h.climbing && ((ladder && (input.up || (input.down && !b.on_ground))) || (below && input.down)) {
            h.climbing = true;
            h.jumping = false;
        }
        if hung {
            // Held still (or pulled) above.
        } else if h.climbing {
            let up = (input.up as i32 - input.down as i32) as f32;
            // Climbing without left or right, it moves to the ladder's
            // middle (through a hole one cell wide its head would catch
            // the edge); with them it steps aside. Gravity is taken off
            // again as the body moves.
            let middle = world.cells.as_ref().map_or(b.pos.x, |g| g.middle(b.centre()).x);
            let across = if dir != 0 { dir as f32 * r.climb * 0.5 } else { ((middle - b.pos.x) / DT).clamp(-r.climb, r.climb) };
            b.vel = Vec3::new(across, 0.0, up * r.climb + g * DT);
        } else {
            h.crouch = input.down && b.on_ground;
            let target = if h.crouch { 0.0 } else { dir as f32 * r.speed };
            let accel = if b.on_ground { r.accel } else { r.air_accel } * DT;
            b.vel.x += (target - b.vel.x).clamp(-accel, accel);
            b.vel.y = 0.0;
            if h.asked <= r.buffer && (b.on_ground || h.air <= r.coyote) && !h.jumping {
                b.vel.z = (2.0 * g * r.jump).sqrt();
                h.jumping = true;
                h.asked = u32::MAX / 2;
                h.air = u32::MAX / 2;
                h.riding = None;
                sounds.push(("jump", b.pos));
            } else if pressed && !b.on_ground && h.air > r.coyote && h.air_left > 0 {
                // Again in the air (a double jump): from where it is now.
                b.vel.z = (2.0 * g * r.air_jump).sqrt();
                h.air_left -= 1;
                h.jumping = true;
                h.asked = u32::MAX / 2;
                sounds.push(("jump", b.pos));
            }
            if h.jumping && !jump && b.vel.z > 0.0 {
                b.vel.z *= r.jump_cut;
                h.jumping = false;
            }
            if b.vel.z <= 0.0 {
                h.jumping = false;
            }
        }
        if dir != 0 && !hung {
            b.face(Vec3::new(dir as f32, 0.0, 0.0));
        }
        if !hung && !h.climbing && !b.on_ground {
            // Lighter at the top while jump is held (time to steer), heavier
            // on the way down (snappy): on top of the gravity advance_in adds.
            let top = jump && b.vel.z.abs() < r.hang_speed;
            let extra = if top { r.hang - 1.0 } else if b.vel.z < 0.0 { r.fall_gravity - 1.0 } else { 0.0 };
            b.vel.z -= g * extra * DT;
        }
        if !hung && !h.climbing && b.vel.z > 0.0 && r.corner > 0 {
            // A head about to clip a ceiling's edge slips past it sideways,
            // by the fewest voxels that free it.
            let rise = (b.vel.z * DT).max(0.5);
            if body_hits(world, b.pos + Vec3::Z * rise, half, b.height) {
                'nudge: for k in 1..=r.corner {
                    for side in [1.0f32, -1.0] {
                        let dx = side * k as f32;
                        if !body_hits(world, b.pos + Vec3::new(dx, 0.0, rise), half, b.height) && !body_hits(world, b.pos + Vec3::X * dx, half, b.height) {
                            b.pos.x += dx;
                            // Running on under the ceiling would undo it.
                            if b.vel.x * dx < 0.0 {
                                b.vel.x = 0.0;
                            }
                            break 'nudge;
                        }
                    }
                }
            }
        }
        if !hung {
            // A ladder's top holds from above, as a plank would, unless down
            // is held (the way down it).
            let mut tops = tops.clone();
            if let Some(g) = world.cells.as_deref().filter(|_| !h.climbing && !input.down) {
                let cell = g.cell.as_vec3();
                for z in [b.pos.z - 0.5, b.pos.z + (b.vel.z * DT).min(0.0) - 1.0] {
                    let mid = g.middle(Vec3::new(b.pos.x, b.pos.y, z));
                    if g.at(mid) & crate::grid::LADDER != 0 && g.at(mid + Vec3::Z * cell.z) & crate::grid::LADDER == 0 {
                        tops.push(Ledge { min: (mid - cell * 0.5).truncate(), max: (mid + cell * 0.5).truncate(), z: mid.z + cell.z * 0.5 });
                    }
                }
            }
            b.advance_in(world, &cfg.physics, half, &tops);
        }
        h.riding = if b.on_ground { ledges.iter().find(|(_, l)| l.under(b.pos, half) && (l.z - b.pos.z).abs() < 0.05).map(|(i, _)| *i) } else { None };
        // An edge too high to land on caught near the top of a jump or
        // falling past it, steering towards it (or holding up or jump):
        // above the head it hangs from it, lower it pulls itself up.
        let f = b.look.x as f32;
        if r.grab > 0.0 && f != 0.0 && !hung && !h.climbing && !b.on_ground && !input.down && b.vel.z <= r.hang_speed && (dir as f32 == f || input.up || jump)
            && let Some((at, stand, hangs)) = edge(world, b.pos, half, b.height, f, r)
        {
            (b.pos, b.vel) = (at, Vec3::ZERO);
            (h.edge, h.jumping, h.riding) = (Some(stand), false, None);
            if !hangs {
                (h.pulling, h.from) = (1, at);
            }
        }
        // A jump pressed just before landing goes the tick it lands.
        if !hung && !h.climbing && b.on_ground && h.asked <= r.buffer && !h.jumping {
            b.vel.z = (2.0 * g * r.jump).sqrt();
            h.jumping = true;
            h.asked = u32::MAX / 2;
            h.air = u32::MAX / 2;
            h.riding = None;
            sounds.push(("jump", b.pos));
        }
        let anim = if h.hurt + HURT_SHOWN > r.invulnerable && h.hurt > 0 {
            Anim::Hurt
        } else if h.pulling > 0 {
            Anim::Pull
        } else if h.edge.is_some() {
            Anim::Hang
        } else if h.climbing {
            Anim::Climb
        } else if !b.on_ground {
            if b.vel.z > 0.0 { Anim::Jump } else { Anim::Fall }
        } else if h.crouch {
            Anim::Crouch
        } else if b.vel.x.abs() > 2.0 {
            Anim::Run
        } else {
            Anim::Idle
        };
        if anim == h.anim {
            h.age += 1;
        } else {
            (h.anim, h.age) = (anim, 0);
        }
        for (name, at) in sounds {
            self.sim.sound(name, at, 0.8);
        }
    }

    /// The crates fall (onto the floor, platforms, each other), only the
    /// hero moves them across; their tops are ledges for the rest.
    fn crates(&mut self, ledges: &mut Vec<(Id, Ledge)>) {
        let cfg = self.sim.cfg.clone();
        let ids: Vec<Id> = self.things.iter().filter(|t| t.off.is_none() && matches!(t.brain, Brain::Crate)).map(|t| t.id).collect();
        for id in ids {
            let tops: Vec<Ledge> = ledges.iter().filter(|(i, _)| *i != id).map(|(_, l)| *l).collect();
            let Some(t) = self.things.get_mut(id) else { continue };
            let k = &self.rules.kinds[t.kind];
            let half = k.width * 0.5;
            t.body.vel.x = 0.0;
            t.body.advance_in(&self.sim.world, &cfg.physics, half, &tops);
            let at = t.body.pos;
            ledges.push((id, Ledge { min: Vec2::new(at.x - half, at.y - half), max: Vec2::new(at.x + half, at.y + half), z: at.z + t.body.height }));
        }
    }

    /// Where the crates stand: their middle at the floor, half their width,
    /// their height, their ids.
    fn crate_boxes(&self) -> Vec<(Id, Vec3, f32, f32)> {
        self.things.iter().filter(|t| matches!(t.brain, Brain::Crate)).map(|t| (t.id, t.body.pos, self.rules.kinds[t.kind].width * 0.5, t.body.height)).collect()
    }

    /// The hero walking into a crate's side pushes it along (as fast as the
    /// crate's `push` at most), unless a wall or another crate stops it:
    /// then the crate stops the hero. (Standing on one is standing on a
    /// ledge.)
    fn push_crates(&mut self) {
        for id in self.heroes() {
            self.push_crates_by(id);
        }
    }

    /// One hero pushing the crates it walks into.
    fn push_crates_by(&mut self, id: Id) {
        let Some((kind, mut body)) = self.things.get(id).map(|t| (t.kind, t.body.clone())) else { return };
        let half = self.rules.kinds[kind].width * 0.5;
        let boxes = self.crate_boxes();
        for &(c, at, c_half, c_height) in &boxes {
            let beside = body.pos.z < at.z + c_height - 0.5 && body.pos.z + body.height > at.z + 0.5;
            let (dx, reach) = (body.pos.x - at.x, half + c_half);
            if !beside || dx.abs() >= reach {
                continue;
            }
            let way = if dx < 0.0 { 1.0 } else { -1.0 };
            let to = at + Vec3::X * way * (reach - dx.abs());
            let others = boxes.iter().filter(|(o, ..)| *o != c).any(|&(_, p, h, z)| (p.x - to.x).abs() < h + c_half && to.z < p.z + z && to.z + c_height > p.z);
            let push = match &self.rules.kinds[self.things.get(c).map_or(0, |t| t.kind)].rules {
                KindRules::Crate(r) => r.push,
                _ => 0.0,
            };
            if way * body.vel.x > 0.0 && !others && !body_hits(&self.sim.world, to, c_half, c_height) {
                if let Some(t) = self.things.get_mut(c) {
                    t.body.pos = to;
                }
                body.vel.x = body.vel.x.clamp(-push, push);
            } else {
                body.pos.x = at.x - way * reach;
                if way * body.vel.x > 0.0 {
                    body.vel.x = 0.0;
                }
            }
        }
        if let Some(t) = self.things.get_mut(id) {
            (t.body.pos, t.body.vel) = (body.pos, body.vel);
        }
    }

    /// Walkers: to and fro, turning at walls, crates and (if they mind
    /// them) ledges.
    fn walkers(&mut self, ledges: &[(Id, Ledge)]) {
        let cfg = self.sim.cfg.clone();
        let tops: Vec<Ledge> = ledges.iter().map(|(_, l)| *l).collect();
        let crates = self.crate_boxes();
        let world = &self.sim.world;
        for t in self.things.iter_mut().filter(|t| t.off.is_none()) {
            let k = &self.rules.kinds[t.kind];
            let (KindRules::Walker(r), Brain::Walker { way }) = (&k.rules, &mut t.brain) else { continue };
            let half = k.width * 0.5;
            let b = &mut t.body;
            if b.on_ground {
                b.vel.x = *way * r.speed;
                let ahead = Vec3::new(b.pos.x + *way * (half + 1.0), b.pos.y, b.pos.z - 1.0).floor().as_ivec3();
                if r.ledges && world.hold(ahead) == Hold::Free {
                    *way = -*way;
                    b.vel.x = *way * r.speed;
                }
            }
            b.face(Vec3::new(*way, 0.0, 0.0));
            b.advance_in(world, &cfg.physics, half, &tops);
            let into_crate = crates.iter().any(|&(_, at, h, z)| (b.pos.x - at.x).abs() < half + h && b.pos.z < at.z + z - 0.5 && b.pos.z + b.height > at.z + 0.5);
            if into_crate {
                b.pos.x = b.prev.x;
            }
            if b.on_ground && (b.vel.x == 0.0 || into_crate) {
                *way = -*way;
            }
        }
    }

    /// Hoppers: waiting on the ground, then a hop, left and right in turn.
    fn hoppers(&mut self, ledges: &[(Id, Ledge)]) {
        let cfg = self.sim.cfg.clone();
        let tops: Vec<Ledge> = ledges.iter().map(|(_, l)| *l).collect();
        let world = &self.sim.world;
        for t in self.things.iter_mut().filter(|t| t.off.is_none()) {
            let k = &self.rules.kinds[t.kind];
            let (KindRules::Hopper(r), Brain::Hopper { wait, way }) = (&k.rules, &mut t.brain) else { continue };
            let b = &mut t.body;
            if b.on_ground {
                b.vel.x = 0.0;
                *wait = wait.saturating_sub(1);
                if *wait == 0 {
                    b.vel.x = *way * r.hop[0];
                    b.vel.z = r.hop[1];
                    b.face(Vec3::new(*way, 0.0, 0.0));
                    *way = -*way;
                    *wait = r.every;
                }
            }
            b.advance_in(world, &cfg.physics, k.width * 0.5, &tops);
        }
    }

    /// Flyers: up and down over where they were placed, facing the hero
    /// (unless their kind says not to).
    fn flyers(&mut self) {
        let heroes: Vec<f32> = self.heroes().iter().filter_map(|&id| self.things.get(id)).map(|t| t.body.pos.x).collect();
        for t in self.things.iter_mut().filter(|t| t.off.is_none()) {
            let k = &self.rules.kinds[t.kind];
            let (KindRules::Flyer(r), Brain::Flyer { home, age }) = (&k.rules, &mut t.brain) else { continue };
            *age += 1;
            let phase = std::f32::consts::TAU * *age as f32 / r.period.max(1) as f32;
            t.body.pos = *home + Vec3::Z * (r.range * crate::sim::math::sin(phase)).round();
            // The nearest hero (seat 1's first among equals).
            let me = t.body.pos.x;
            let hero_x = heroes.iter().copied().reduce(|a, b| if (b - me).abs() < (a - me).abs() { b } else { a });
            if let Some(x) = hero_x
                && r.face_hero
            {
                t.body.face(Vec3::new((x - t.body.pos.x).signum(), 0.0, 0.0));
            }
        }
    }

    /// What fell out of the level (well under it): told to the game every
    /// tick it is out (the game hears it once, `on_fall_out`); the game
    /// takes the rest away and decides about the hero. The script's own
    /// things the game finds itself.
    fn fell_out(&mut self) {
        for t in self.things.iter().filter(|t| !matches!(t.brain, Brain::Script(_))) {
            if crate::sim::fell_out(&self.sim.world, &t.body, self.sim.cfg.physics.lost_below) {
                let who = if Some(t.id) == self.hero { Who::Player } else { Who::Thing(t.id) };
                self.sim.events.push(Event::FellOut { who });
            }
        }
    }

    /// The hero hit from `from` (x) by `by`: `damage` hearts less, thrown
    /// back, safe for a moment; with the last it lies still (`on_death`:
    /// the game decides).
    fn hurt(&mut self, from: f32, r: &kinds::HeroRules, damage: i32, by: Option<Who>, weapon: Option<String>) {
        if let Some(id) = self.hero {
            self.hurt_hero(id, from, r, damage, by, weapon);
        }
    }

    /// A hero hit (seat 1's hearts are the run's, `status.hp`; another
    /// seat's its own hit points).
    pub(crate) fn hurt_hero(&mut self, id: Id, from: f32, r: &kinds::HeroRules, damage: i32, by: Option<Who>, weapon: Option<String>) {
        let first = self.hero == Some(id);
        let who = if first { Who::Player } else { Who::Thing(id) };
        let Some(t) = self.things.get_mut(id) else { return };
        let Brain::Hero(h) = &mut t.brain else { return };
        if h.hurt > 0 {
            return;
        }
        h.hurt = r.invulnerable;
        h.climbing = false;
        h.riding = None;
        (h.edge, h.pulling) = (None, 0);
        let away = if from > t.body.pos.x { -1.0 } else { 1.0 };
        t.body.vel = Vec3::new(away * r.knockback[0], 0.0, r.knockback[1]);
        let at = t.body.pos;
        let hp = if first { self.status.hp as i32 } else { t.hp };
        self.sim.sound("hurt", at, 0.9);
        if hp <= 0 {
            return;
        }
        let left = (hp - damage.max(0)).max(0);
        match first {
            true => self.status.hp = left as u32,
            false => {
                if let Some(t) = self.things.get_mut(id) {
                    t.hp = left;
                }
            }
        }
        self.sim.events.push(Event::Hit { who, by, damage, weapon: weapon.clone() });
        if left == 0
            && let Some(t) = self.things.get(id)
        {
            self.sim.events.push(Event::Death { who, by, kind: t.kind, body: t.body.clone(), weapon });
        }
    }

    /// The frame a thing shows: its kind's frames for what it does (or
    /// the pose the game set, from when it was set), each set at its own
    /// speed, round and round or once (the names: `HERO_PLAYS`).
    fn frame(&self, t: &crate::sim::things::Thing<Brain>) -> usize {
        let k = &self.rules.kinds[t.kind];
        if let Some((pose, from)) = &t.pose
            && let Some(f) = k.frames.get(pose)
        {
            return f.at(self.sim.tick - from, t.body.walked);
        }
        let (name, age) = match &t.brain {
            Brain::Hero(h) => (h.anim.name(), h.age),
            Brain::Walker { .. } => ("walk", self.sim.tick as u32),
            Brain::Hopper { .. } => (if t.body.on_ground { "idle" } else { "jump" }, self.sim.tick as u32),
            Brain::Flyer { age, .. } => ("fly", *age),
            Brain::Pickup => ("spin", self.sim.tick as u32 + t.id.raw() * 7),
            Brain::Prop { pose, since } if !pose.is_empty() => (pose.as_str(), (self.sim.tick - since) as u32),
            _ => ("idle", self.sim.tick as u32),
        };
        k.frames.get(name).map_or(0, |f| f.at(age as u64, t.body.walked))
    }
}

/// A ledge's edge a body at `pos` (`half` its half width, `height` tall)
/// facing `f` (x, 1 or -1) can catch: the top of a column up to
/// `grab_ahead` voxels beyond its side, between its middle and `grab` above
/// its head, with room to rise beside it and to stand on it. Where the body
/// holds on, where it stands once pulled up, and whether it hangs first (an
/// edge above its head: its head `hang_below` under the top, waiting for
/// up) or pulls itself up at once (an edge it nearly made).
fn edge(world: &World, pos: Vec3, half: f32, height: f32, f: f32, r: &kinds::HeroRules) -> Option<(Vec3, Vec3, bool)> {
    let (top, reach) = (pos.z + height, r.grab);
    let y = pos.y.floor() as i32;
    for d in 1..=(r.grab_ahead.round() as i32).max(1) {
        let x = (pos.x + f * (half + d as f32 - 0.5)).floor() as i32;
        let filled = |z: i32| world.hold(IVec3::new(x, y, z)) != Hold::Free;
        let (lo, hi) = ((pos.z + height * 0.4).ceil() as i32, (top + reach).floor() as i32);
        let Some(e) = (lo..=hi).find(|&z| filled(z - 1) && !filled(z)) else {
            if (lo..=hi).any(filled) {
                return None; // a wall, no edge within reach
            }
            continue;
        };
        let (face, e) = (if f > 0.0 { x as f32 } else { x as f32 + 1.0 }, e as f32);
        let hangs = e >= top - r.hang_below;
        let at = Vec3::new(face - f * half, pos.y, if hangs { e - height - r.hang_below } else { pos.z });
        let risen = Vec3::new(at.x, pos.y, e);
        let stand = Vec3::new(face + f * half, pos.y, e);
        let room = |p: Vec3| !body_hits(world, p, half, height);
        return (room(at) && room(risen) && room(stand)).then_some((at, stand, hangs));
    }
    None
}

impl Game for Platformer {
    type Snapshot = Platformer;

    fn behaviours(_cfg: &GameConfig) -> Vec<&'static str> {
        BEHAVIOURS.to_vec()
    }

    fn actions(_cfg: &GameConfig) -> Vec<&'static str> {
        ACTIONS.to_vec()
    }

    fn default_keys(_cfg: &GameConfig) -> Bindings {
        Bindings::new(&[("jump", &["Space", "Z"])])
    }

    fn default_pad(_cfg: &GameConfig) -> Bindings {
        Bindings::new(&[("jump", &["south"])])
    }

    fn sounds(_cfg: &GameConfig) -> Vec<(&'static str, Synth)> {
        SOUNDS.iter().map(|&n| (n, default_sound(n))).collect()
    }

    /// The kinds' models (their heights count for the simulation).
    fn models(cfg: &GameConfig) -> anyhow::Result<Vec<ModelSpec>> {
        let mut specs: Vec<ModelSpec> = Vec::new();
        let defs = crate::sim::things::defs(cfg, &BEHAVIOURS)?;
        for file in defs.iter().filter_map(|d| d.model.clone()) {
            if !specs.iter().any(|s| s.file == file) {
                specs.push(ModelSpec { name: file.clone(), file, shaped: true });
            }
        }
        Ok(specs)
    }

    fn load(setup: Setup) -> anyhow::Result<Self> {
        let rules = Rules::from_game(setup.cfg, |file| setup.models.get(file).map(|m| m.size))?;
        let frames = frame_counts(setup.models, &rules);
        Ok(Self::new(setup.world, setup.cfg, rules, frames, &setup.markers))
    }

    fn snapshot(&self) -> anyhow::Result<Platformer> {
        Ok(self.clone())
    }

    fn restore(&mut self, kept: &Platformer) {
        *self = kept.clone();
    }

    fn sim(&self) -> &Sim {
        &self.sim
    }

    fn sim_mut(&mut self) -> &mut Sim {
        &mut self.sim
    }

    fn step(&mut self, input: &Input) {
        self.run(input);
    }

    /// The core's, the things' in the order the systems move them, the run's.
    fn state_hash(&self) -> u64 {
        let mut h = crate::hash::Stable::default();
        h.u64(self.sim.tick).u64(self.sim.rng.state()).u64(self.sim.world.changes()).u64(self.sim.freeze as u64);
        h.u64(self.things.len() as u64);
        for t in self.things.iter() {
            let b = &t.body;
            h.u64(t.id.raw() as u64).u64(t.kind as u64).vec3(b.pos).vec3(b.vel).i32(b.look.x).i32(b.look.y).u64(b.on_ground as u64).i32(t.hp);
            match &t.brain {
                Brain::Hero(x) => {
                    h.u64(x.air as u64).u64(x.asked as u64).u64(x.jumping as u64).u64(x.held as u64).u64(x.climbing as u64).u64(x.hurt as u64);
                    h.u64(x.riding.map_or(u64::MAX, |r| r.raw() as u64));
                    // (Only with air jumps: games without keep their hashes.)
                    if x.air_left > 0 {
                        h.u64(x.air_left as u64);
                    }
                    if let Some(e) = x.edge {
                        h.vec3(e).u64(x.pulling as u64).vec3(x.from);
                    }
                }
                Brain::Walker { way } => {
                    h.f32(*way);
                }
                Brain::Hopper { wait, way } => {
                    h.u64(*wait as u64).f32(*way);
                }
                Brain::Flyer { age, .. } => {
                    h.u64(*age as u64);
                }
                Brain::Platform(m) => {
                    h.f32(m.along).u64(m.back as u64).u64(m.wait as u64);
                    // (Only when told: games without goals keep their hashes.)
                    if let Some(to_end) = m.goal {
                        h.u64(2 + to_end as u64);
                    }
                }
                Brain::Prop { pose, .. } => {
                    h.bytes(pose.as_bytes());
                }
                _ => {}
            }
            // What the game set on it, only when it did.
            if let Some((pose, from)) = &t.pose {
                h.bytes(pose.as_bytes()).u64(*from);
            }
            if t.off.is_some() {
                h.u64(0x0ff);
            }
        }
        let s = &self.status;
        h.u64(s.score as u64).u64(s.hp as u64);
        h.u64(match s.outcome {
            None => 0,
            Some(Outcome::Won) => 1,
            Some(Outcome::Lost) => 2,
        });
        h.finish()
    }

    fn draw(&self, out: &mut Drawing) {
        let ticks = self.sim.tick as f32 + out.alpha();
        for t in self.things.iter() {
            let k = &self.rules.kinds[t.kind];
            if matches!(k.rules, KindRules::Script) {
                continue; // the script draws its own
            }
            let Some(model) = &k.model else { continue };
            if let (Brain::Hero(h), KindRules::Hero(r)) = (&t.brain, &k.rules)
                && h.hurt > 0
                && r.blink > 0
                && (h.hurt / r.blink) % 2 == 1
            {
                continue; // blinking while nothing hurts it
            }
            let origin = out.body_origin(&t.body);
            // Climbing, the hero turns to the ladder.
            let look = match (&t.brain, &k.rules) {
                // why: the ladder lies behind the lane, up the screen
                (Brain::Hero(h), KindRules::Hero(r)) if h.climbing && r.face_ladder => IVec2::Y,
                _ => t.body.look,
            };
            // A kind that spins turns round (a gem), a figure of parts
            // swings its parts with its steps.
            let swung = crate::session::swinging(&k.swing, &t.body, k.height);
            out.model_posed(model, self.frame(t), origin, look, crate::session::spinning(k.spin, ticks), &swung, None);
        }
    }

    fn focus(&self) -> Option<&Body> {
        match self.watch {
            // Seat 1's hero, or with it out the first other seat's.
            crate::sim::Watch::Player => self.hero.or_else(|| self.seat_heroes.iter().flatten().next().copied()).and_then(|id| self.things.get(id)).map(|t| &t.body),
            crate::sim::Watch::Thing(id) => self.things.get(id).map(|t| &t.body),
            crate::sim::Watch::Still => None,
        }
    }

    fn outcome(&self) -> Option<Outcome> {
        self.status.outcome
    }

    fn scored(&self) -> bool {
        true
    }

    fn score(&self) -> u32 {
        self.status.score
    }

    fn summary(&self) -> Option<String> {
        Some(format!("score {} | hp {}", self.status.score, self.status.hp))
    }

    fn numbers(&self) -> Vec<String> {
        self.rules.numbers()
    }

    fn report(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(t) = self.hero.and_then(|id| self.things.get(id)) {
            let b = &t.body;
            let anim = match &t.brain {
                Brain::Hero(h) => h.anim.name(),
                _ => "",
            };
            lines.push(format!("hero at ({:.1}, {:.1}, {:.1}) {} {anim}, hp {}", b.pos.x, b.pos.y, b.pos.z, if b.on_ground { "standing" } else { "in the air" }, self.status.hp));
        }
        lines
    }

    /// The hero put at x and height y (the level is seen from the side),
    /// dropped onto the floor below.
    fn place_player(&mut self, x: f32, y: f32) {
        let Some(id) = self.hero else { return };
        let Some(t) = self.things.get(id) else { return };
        let (kind, lane) = (t.kind, t.body.pos.y);
        self.things.remove(id);
        self.hero = None;
        self.place(kind, Vec3::new(x, lane, y));
        self.look_at_hero();
    }

    /// New settings: the kinds' numbers anew; things keep their kind by its
    /// name, a kind gone takes its things with it.
    fn apply_config(&mut self, cfg: &GameConfig) -> anyhow::Result<()> {
        let old = self.rules.clone();
        let sizes: BTreeMap<String, glam::IVec3> = old.kinds.iter().filter_map(|k| Some((k.model.clone()?, k.size))).collect();
        let rules = Rules::from_game(cfg, |file| sizes.get(file).copied())?;
        let now = |kind: usize| rules.find(&old.kinds[kind].name);
        self.things.retain(|t| now(t.kind).is_some());
        for t in self.things.iter_mut() {
            t.kind = now(t.kind).expect("kept by its kind");
        }
        if self.hero.is_some_and(|id| self.things.get(id).is_none()) {
            self.hero = None;
        }
        // A kind that keeps the way it faces faces it at once (a bat to
        // the front), not only once placed anew.
        for t in self.things.iter_mut() {
            let k = &rules.kinds[t.kind];
            if let (Some([x, y]), KindRules::Flyer(f)) = (k.facing, &k.rules)
                && !f.face_hero
            {
                t.body.face(Vec3::new(x, y, 0.0));
            }
        }
        self.rules = Arc::new(rules);
        self.sim.apply_config(cfg);
        Ok(())
    }

    /// A new level: everything placed anew from its markers.
    fn replace_world(&mut self, world: World, markers: &[Marker]) {
        self.sim.replace_world(world);
        self.watch = Default::default();
        self.markers = Arc::new(markers.to_vec());
        let (score, hp) = (self.status.score, self.status.hp);
        self.start();
        (self.status.score, self.status.hp) = (score, hp);
    }

    /// New models: the kinds' sizes and heights follow them.
    fn reshape(&mut self, models: &Models) {
        let mut rules = (*self.rules).clone();
        for k in &mut rules.kinds {
            if let Some(m) = k.model.as_deref().and_then(|f| models.get(f)) {
                let explicit = (k.height - k.size.z as f32).abs() > 0.5;
                k.size = m.size;
                if !explicit {
                    k.height = m.size.z as f32;
                }
            }
        }
        self.frames = Arc::new(frame_counts(models, &rules));
        self.rules = Arc::new(rules);
    }
}

/// How many frames each of the kinds' models (and effects) has.
fn frame_counts(models: &Models, rules: &Rules) -> BTreeMap<String, usize> {
    rules
        .kinds
        .iter()
        .flat_map(|k| k.model.iter())
        .filter_map(|f| models.get(f).map(|m| (f.clone(), m.frames.len())))
        .collect()
}

#[cfg(test)]
mod tests;

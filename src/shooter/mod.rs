//! The shooter kit (`c.kit = "shooter"`): building blocks for shooters seen
//! from above (soldiers, huts, hostages, a gunship, pickups; weapons,
//! attacks, teams and blasts as the game's data). More Blood, its first
//! game, was a prototype; what a kill, a pickup, a life or the HUD mean is
//! each game's code. `Shooter` holds the engine's simulation core (`sim::Sim`:
//! the world, bodies, explosions, debris) and adds its own things; what it
//! calls things lives here too: its actions, sounds and markers. `combat`
//! holds what makes a level a fight, `helicopter` the gunship.

mod combat;
pub mod config;
mod draw;
mod helicopter;
pub mod kinds;
#[cfg(test)]
mod pins;
mod script;

use std::sync::Arc;

use anyhow::Context;
use glam::{IVec3, Vec3};

use crate::audio::SoundDef;
use crate::config::GameConfig;
use crate::game::{Game, ModelSpec, Outcome, Setup};
use crate::session::{Drawing, Models};
use crate::controls::Bindings;
use crate::input::Input;
use crate::sim::math;
use crate::sim::{Body, DT, Marker, Shape, Sim, SoundEvent, body_hits, spawn_point};
use crate::world::World;

pub use combat::{Captive, Grenade, Phase, Soldier, Spawner, Status};
pub use config::ShooterConfig;
pub use helicopter::{Gunship, State as HelicopterState};
pub use kinds::{Kind, KindRules, Rules, WeaponRules};

/// What a thing of the shooter's is, with its state as that: the brain of a
/// thing in the engine's list (`sim::things`).
#[derive(Clone)]
pub enum Brain {
    /// A Lua game's script moves it (`lua::host::Kit`).
    Script(crate::sim::things::Walk),
    Soldier(Soldier),
    Captive(Captive),
    Gunship(Gunship),
    Pickup,
    Spawner(Spawner),
    /// A player's figure in a game of several (`c.players`), every seat's:
    /// a game of one plays the kit's own (`Shooter::player`).
    Player(Box<Seated>),
}

pub type Thing = crate::sim::things::Thing<Brain>;

fn is_soldier(t: &Thing) -> bool {
    matches!(t.brain, Brain::Soldier(_))
}

fn is_captive(t: &Thing) -> bool {
    matches!(t.brain, Brain::Captive(_))
}

fn is_gunship(t: &Thing) -> bool {
    matches!(t.brain, Brain::Gunship(_))
}

/// The behaviours of the shooter's kinds of things (`[things.NAME]
/// behaviour`, `kinds`).
pub const BEHAVIOURS: [&str; 5] = ["soldier", "spawner", "captive", "gunship", "pickup"];

/// The keys for its actions to start with.
pub fn keys() -> Bindings {
    Bindings::new(&[("fire", &["X", "J", "Left Ctrl"]), ("jump", &["Space", "Z"]), ("grenade", &["C", "K"]), ("strafe", &["Left Shift", "Right Shift"])])
}

/// The gamepad buttons to start with: positional names, so every pad works
/// alike (south is A on an Xbox pad and the cross on a PlayStation one).
pub fn pad() -> Bindings {
    Bindings::new(&[("fire", &["west", "righttrigger"]), ("jump", &["south"]), ("grenade", &["east", "rightshoulder"]), ("strafe", &["lefttrigger", "leftshoulder"])])
}

/// The actions of More Blood and the kit's tests, as their attacks and
/// moves name them (`Binds::actions_of`): the input's bits in tests.
#[cfg(test)]
pub const ACTIONS: [&str; 4] = ["fire", "grenade", "jump", "strafe"];

#[cfg(test)]
impl From<Controls> for Input {
    fn from(keys: Controls) -> Self {
        keys.input(&ACTIONS)
    }
}

impl Controls {
    /// The engine's input these keys make, the game's actions by their
    /// names (a name the game has not: nothing).
    pub fn input(&self, actions: &[&str]) -> Input {
        let mut input = Input { left: self.left, right: self.right, up: self.up, down: self.down, actions: 0 };
        for (name, on) in [("fire", self.fire), ("jump", self.jump), ("grenade", self.grenade), ("strafe", self.strafe)] {
            if let Some(n) = actions.iter().position(|a| *a == name) {
                input.hold(n, on);
            }
        }
        input
    }
}

/// The kit's own sounds' files, built in (`sounds/`); its `impact`,
/// `explosion` and `crumble` are the engine's (`audio::CORE_FILES`).
pub const SOUND_FILES: &[(&str, &str)] = &[
    ("shot", include_str!("sounds/shot.toml")),
    ("enemy_shot", include_str!("sounds/enemy_shot.toml")),
    ("rocket", include_str!("sounds/rocket.toml")),
    ("knife", include_str!("sounds/knife.toml")),
    ("throw", include_str!("sounds/throw.toml")),
    ("splat", include_str!("sounds/splat.toml")),
    ("hurt", include_str!("sounds/hurt.toml")),
    ("down", include_str!("sounds/down.toml")),
    ("pickup", include_str!("sounds/pickup.toml")),
    ("rescue", include_str!("sounds/rescue.toml")),
    ("jump", include_str!("sounds/jump.toml")),
    ("land", include_str!("sounds/land.toml")),
    ("checkpoint", include_str!("sounds/checkpoint.toml")),
    ("win", include_str!("sounds/win.toml")),
    ("game_over", include_str!("sounds/game_over.toml")),
    ("rotor", include_str!("sounds/rotor.toml")),
];

/// Its sound events by name, with the sounds they make unless the game has
/// a file of that name (`sounds/NAME.toml`).
pub fn sounds() -> Vec<(&'static str, SoundDef)> {
    let mut all = crate::audio::sound::builtin(SOUND_FILES);
    all.extend(crate::audio::core_sounds());
    Sfx::ALL.iter().map(|sfx| all.iter().find(|(n, _)| *n == sfx.name()).cloned().unwrap_or_else(|| panic!("the shooter's {} has no file", sfx.name()))).collect()
}

#[derive(Clone, Copy, Default, Debug)]
/// The shooter's input: the directions and its actions as switches, made
/// from the engine's `Input` (`controls`).
pub struct Controls {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub jump: bool,
    pub fire: bool,
    pub grenade: bool,
    /// Keep facing and aim while moving: walk sideways or backwards.
    pub strafe: bool,
}

/// Something that makes a sound. The simulation only reports these; what
/// they sound like is the game's business (`audio`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sfx {
    Shot,
    EnemyShot,
    Rocket,
    Knife,
    /// A shot chipping the world.
    Impact,
    Explosion,
    Throw,
    /// A body bursting.
    Splat,
    Hurt,
    /// The player going down.
    Down,
    Pickup,
    Rescue,
    Jump,
    Land,
    Checkpoint,
    Win,
    GameOver,
    /// A piece cut off from the ground coming down.
    Crumble,
    /// One chop of a gunship's rotor (it repeats while it flies).
    Rotor,
}

impl Sfx {
    pub const ALL: [Sfx; 19] = [
        Sfx::Shot,
        Sfx::EnemyShot,
        Sfx::Rocket,
        Sfx::Knife,
        Sfx::Impact,
        Sfx::Explosion,
        Sfx::Throw,
        Sfx::Splat,
        Sfx::Hurt,
        Sfx::Down,
        Sfx::Pickup,
        Sfx::Rescue,
        Sfx::Jump,
        Sfx::Land,
        Sfx::Checkpoint,
        Sfx::Win,
        Sfx::GameOver,
        Sfx::Crumble,
        Sfx::Rotor,
    ];

    /// Its name in `c.sounds`.
    pub fn name(self) -> &'static str {
        match self {
            Sfx::Shot => "shot",
            Sfx::EnemyShot => "enemy_shot",
            Sfx::Rocket => "rocket",
            Sfx::Knife => "knife",
            Sfx::Impact => "impact",
            Sfx::Explosion => "explosion",
            Sfx::Throw => "throw",
            Sfx::Splat => "splat",
            Sfx::Hurt => "hurt",
            Sfx::Down => "down",
            Sfx::Pickup => "pickup",
            Sfx::Rescue => "rescue",
            Sfx::Jump => "jump",
            Sfx::Land => "land",
            Sfx::Checkpoint => "checkpoint",
            Sfx::Win => "win",
            Sfx::GameOver => "game_over",
            Sfx::Crumble => "crumble",
            Sfx::Rotor => "rotor",
        }
    }

    pub fn from_name(name: &str) -> Option<Sfx> {
        Sfx::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// What the simulation knows of the models: the player's, the others' by
/// file, and a block of the player's size for a kind without one.
#[derive(Clone, Default)]
pub struct Shapes {
    pub player: Shape,
    models: std::collections::BTreeMap<String, Shape>,
    fallback: Shape,
}

impl Shapes {
    pub fn new(player: Shape, models: std::collections::BTreeMap<String, Shape>) -> Self {
        let fallback = Shape::block(player.size);
        Self { player, models, fallback }
    }

    /// The shape of the model in `file`.
    pub fn of(&self, file: Option<&str>) -> &Shape {
        file.and_then(|f| self.models.get(f)).unwrap_or(&self.fallback)
    }

    /// A kind's shape.
    pub fn kind(&self, kind: &Kind) -> &Shape {
        self.of(kind.model.as_deref())
    }

    /// How tall the model in `file` is, if it is one of these.
    pub fn height_of(&self, file: &str) -> Option<f32> {
        self.models.get(file).map(Shape::height)
    }
}

/// A seat's player in a game of several: its figure is the thing (its
/// body, its hit points), these its rules' numbers, as `Player` and
/// `Status` are the one player's.
#[derive(Clone)]
pub struct Seated {
    pub seat: usize,
    pub invulnerable: u32,
    pub(super) cooldowns: Vec<u32>,
    pub(super) held: u32,
    pub(super) busy: u32,
    pub(super) jump_held: bool,
    /// Hit too often: lying in pieces, out of the fight, until the game's
    /// code gives it hit points again (the one player's `Phase::Down`).
    pub down: bool,
    pub weapon: Option<usize>,
    pub stock: std::collections::BTreeMap<String, u32>,
}

/// One on the players' side: the one player of a game of one, or a seat's
/// figure; as it stands this tick.
#[derive(Clone)]
pub(crate) struct Fighter {
    pub who: crate::sim::Who,
    pub body: Body,
    /// Its seat's team (`p.state.team`): rounds and blasts of another hurt it.
    pub team: &'static str,
    /// In the fight: not down, the run going on.
    pub playing: bool,
}

#[derive(Clone)]
pub struct Player {
    pub body: Body,
    /// Ticks left during which hits do nothing (after a hit or a respawn).
    pub invulnerable: u32,
    /// Each attack's ticks until it may attack again (`Binds::attacks`).
    cooldowns: Vec<u32>,
    /// The actions held last tick (a press is one held now and not then).
    held: u32,
    /// Ticks until the game's code may attack with it again (`thing:attack`).
    busy: u32,
    jump_held: bool,
}

#[derive(Clone)]
pub struct Bullet {
    pub pos: Vec3,
    pub prev: Vec3,
    /// Where it was fired from; its trail never reaches back past it.
    pub origin: Vec3,
    vel: Vec3,
    life: f32,
    damage: i32,
    /// Crater radius where it hits the world; 0 leaves the world alone.
    blast: f32,
    /// Explodes where it hits, hurting everyone near.
    pub rocket: bool,
    /// Fired by an enemy: hurts the player, not enemies.
    pub hostile: bool,
    /// The weapon it came from (its place in `Rules::weapons`).
    pub weapon: usize,
    /// The team of whoever fired it (the player's: `PLAYER_TEAM`): it hurts
    /// other teams and none.
    pub team: Option<&'static str>,
    /// The seat's player who fired it, in a game of several (whose hit it
    /// is); the one player's and the others' none.
    pub from: Option<crate::sim::things::Id>,
}

pub use crate::sim::things::PLAYER_TEAM;

impl Bullet {
    /// Colours of a rocket's head and of the fire behind it.
    pub fn colors(&self) -> ([u8; 4], [u8; 4]) {
        ([250, 250, 240, 255], [255, 150, 60, 255])
    }

    /// How far it flies in one tick: the length of its tracer.
    pub fn trail(&self) -> Vec3 {
        self.vel * DT
    }
}

/// Where a body's shots leave its gun: `muzzle` is (ahead of the feet's
/// centre, above them) in voxels; without one, just in front of the body at
/// half its height.
/// A player's body a tick on its keys (the one player's and each seat's
/// alike): running the way held, turning to it (strafing keeps the way it
/// looks), jumping; how far it had walked before.
fn walk(b: &mut Body, jump_held: &mut bool, input: &Input, sim: &mut Sim, binds: &kinds::Binds) -> f32 {
    let (moves, physics) = (&sim.cfg.player, &sim.cfg.physics);
    let (sin, cos) = math::sin_cos(sim.cfg.camera.yaw.to_radians());
    let forward = Vec3::new(sin, cos, 0.0);
    let right = Vec3::new(cos, -sin, 0.0);
    let mut wish = Vec3::ZERO;
    if input.up {
        wish += forward;
    }
    if input.down {
        wish -= forward;
    }
    if input.right {
        wish += right;
    }
    if input.left {
        wish -= right;
    }
    let wish = wish.normalize_or_zero();
    let (jump, strafe) = (binds.jump.is_some_and(|a| input.held(a)), binds.strafe.is_some_and(|a| input.held(a)));
    if strafe {
        // Strafing aims the way the model looks: straight along it, never
        // diagonally, whichever way the player was running before.
        b.facing = Vec3::new(b.look.x as f32, b.look.y as f32, 0.0);
    } else {
        b.face(wish);
    }

    // Snappy arcade movement: reach full speed within a few ticks.
    let accel = if b.on_ground { moves.accel } else { moves.air_accel };
    b.steer(wish, moves.speed, accel);
    if jump && !*jump_held && b.on_ground {
        // The take-off speed that reaches `jump` voxels high.
        b.vel.z = (2.0 * physics.gravity * moves.jump.max(0.0)).sqrt();
        b.on_ground = false;
        sim.sounds.push(SoundEvent { name: Sfx::Jump.name(), at: b.pos, volume: 1.0 });
    }
    // Letting go of jump early cuts the jump short.
    if !jump && b.vel.z > 0.0 {
        b.vel.z *= 0.6;
    }
    *jump_held = jump;
    let walked = b.walked;
    b.advance(&sim.world, physics);
    walked
}

fn muzzle_at(body: &Body, muzzle: Option<[f32; 2]>, half: f32) -> Vec3 {
    let [ahead, up] = muzzle.unwrap_or([half + 1.0, body.height * 0.55]);
    body.pos + body.facing * ahead + Vec3::Z * up
}

/// More Blood's game on the engine's core: the player with his weapons,
/// enemies, hostages, huts, gunships, pickups, grenades and bullets, lives
/// and score. It reads through to the core (`Deref`): `shooter.world`,
/// `shooter.particles`.
#[derive(Clone)]
pub struct Shooter {
    pub sim: Sim,
    /// Its own sections of the config (`ShooterConfig`); the engine's are
    /// the core's (`sim.cfg`).
    pub cfg: Arc<ShooterConfig>,
    /// The kinds of things and the weapons (`kinds`).
    pub rules: Arc<Rules>,
    pub player: Player,
    /// Enemies, hostages, gunships, pickups, huts and the script's things,
    /// in the order they came into the world.
    pub things: crate::sim::things::Things<Brain>,
    pub grenades: Vec<Grenade>,
    pub bullets: Vec<Bullet>,
    pub status: Status,
    /// What the camera follows (`level.camera`).
    pub watch: crate::sim::Watch,
    /// Attacks the game's code asked for this tick (`thing:attack`), and
    /// the ticks until a thing may attack so again.
    queued: Vec<Queued>,
    busy: std::collections::BTreeMap<crate::sim::things::Id, u32>,
    shapes: Arc<Shapes>,
    /// The level's places (a hostage may run to one, `flee`).
    places: Arc<[crate::levels::Place]>,
    /// What the seats after the first hold this tick (seat 2 first), in a
    /// game of several.
    seat_inputs: Vec<Option<Input>>,
    /// Each seat's team this tick (seat 1 first; none: `PLAYER_TEAM`), the
    /// game's `p.state.team`.
    seat_teams: Vec<Option<&'static str>>,
    /// The seats' players taken out of the world as another level is
    /// entered: theirs again as their figures come back (hit points, gun,
    /// rounds), by seat.
    departed: Vec<(usize, Seated, i32)>,
}

/// The game's config with the items the weapons are (a weapon with `ammo`
/// is an item of its name, that many to a stack: the inventory holds its
/// rounds), unless `c.items` names one itself.
fn with_items(cfg: &GameConfig, rules: &Rules) -> GameConfig {
    let mut cfg = cfg.clone();
    for w in 0..rules.weapons.len() {
        if let Some(item) = rules.rounds_of(w) {
            cfg.items.entry(item.to_string()).or_insert(crate::config::ItemConfig { stack: rules.weapons[w].ammo, ..Default::default() });
        }
    }
    cfg
}

/// An attack the game's code asked for: who, with which weapon, at what
/// (a point, and the thing there).
type Queued = (crate::sim::Who, usize, Option<(Vec3, Option<crate::sim::things::Id>)>);

impl std::ops::Deref for Shooter {
    type Target = Sim;

    fn deref(&self) -> &Sim {
        &self.sim
    }
}

impl std::ops::DerefMut for Shooter {
    fn deref_mut(&mut self) -> &mut Sim {
        &mut self.sim
    }
}

impl Shooter {
    /// With its sections read from `cfg`. Panics if they do not read (tools
    /// and tests; a game folder loads through the session, which says why).
    pub fn new(world: World, cfg: &GameConfig, shapes: Shapes, markers: &[Marker]) -> Self {
        let own = ShooterConfig::from_game(cfg).expect("the shooter's sections of the config");
        let rules = Rules::from_game(cfg, &own, &|file| shapes.height_of(file)).expect("the shooter's kinds and weapons in the config");
        Self::with_rules(world, cfg, own, rules, shapes, markers)
    }

    /// With its sections and the rules read from them.
    pub fn with_rules(world: World, cfg: &GameConfig, own: ShooterConfig, rules: Rules, shapes: Shapes, markers: &[Marker]) -> Self {
        let cfg = &with_items(cfg, &rules);
        let height = shapes.player.height();
        let spawn = spawn_point(&world, cfg.player.spawn, height, cfg.physics.body_width * 0.5);
        let mut player = Player {
            body: Body::new(spawn, height),
            invulnerable: 0,
            cooldowns: vec![0; rules.binds.attacks.len()],
            held: 0,
            busy: 0,
            jump_held: false,
        };
        if let Some([x, y]) = own.player.facing {
            player.body.face(Vec3::new(x, y, 0.0));
        }
        let cam = spawn + Vec3::Z * height * 0.5;
        let mut shooter = Self {
            sim: Sim::new(world, cfg, cam),
            rules: Arc::new(rules),
            cfg: Arc::new(own),
            player,
            things: Default::default(),
            grenades: Vec::new(),
            bullets: Vec::new(),
            status: Status::new(),
            watch: Default::default(),
            queued: Vec::new(),
            busy: Default::default(),
            shapes: Arc::new(shapes),
            places: Arc::new([]),
            seat_inputs: Vec::new(),
            seat_teams: Vec::new(),
            departed: Vec::new(),
        };
        shooter.populate(markers);
        shooter
    }

    /// A changed config; its own sections read anew (an error leaves
    /// them as they were). Things keep their kind by its name; a kind gone,
    /// or now with another behaviour, takes its things with it.
    pub fn apply_config(&mut self, cfg: &GameConfig) -> anyhow::Result<()> {
        let own = ShooterConfig::from_game(cfg)?;
        let shapes = Arc::clone(&self.shapes);
        let rules = Rules::from_game(cfg, &own, &|file| shapes.height_of(file))?;
        self.sim.apply_config(&with_items(cfg, &rules));
        let old = Arc::clone(&self.rules);
        let now = |kind: usize| {
            let was = &old.kinds[kind];
            rules.find(&was.name).filter(|&k| std::mem::discriminant(&rules.kinds[k].rules) == std::mem::discriminant(&was.rules))
        };
        self.things.retain(|t| now(t.kind).is_some());
        for t in self.things.iter_mut() {
            t.kind = now(t.kind).expect("kept by its kind");
        }
        // The gun in hand by its name; the actions' cooldowns as many as
        // the attacks.
        self.status.weapon = self.status.weapon.and_then(|w| rules.weapon_names.iter().position(|n| *n == old.weapon_names[w]));
        self.player.cooldowns.resize(rules.binds.attacks.len(), 0);
        for t in self.things.iter_mut() {
            if let Brain::Player(s) = &mut t.brain {
                s.weapon = s.weapon.and_then(|w| rules.weapon_names.iter().position(|n| *n == old.weapon_names[w]));
                s.cooldowns.resize(rules.binds.attacks.len(), 0);
            }
        }
        self.rules = Arc::new(rules);
        self.cfg = Arc::new(own);
        self.unstick_player();
        Ok(())
    }

    /// Swaps in a reloaded level. The player stays where it is, lifted out
    /// of any geometry that appeared around it; everything the markers place
    /// starts over.
    pub fn replace_world(&mut self, world: World, markers: &[Marker]) {
        self.sim.replace_world(world);
        self.watch = Default::default();
        self.bullets.clear();
        self.populate(markers);
        self.unstick_player();
    }

    pub fn set_shapes(&mut self, shapes: Shapes) {
        self.player.body.height = shapes.player.height();
        self.shapes = Arc::new(shapes);
        self.unstick_player();
    }

    fn unstick_player(&mut self) {
        let half = self.sim.half();
        let p = &mut self.player.body;
        for _ in 0..self.sim.world.size.z + 8 {
            if !body_hits(&self.sim.world, p.pos, half, p.height) {
                break;
            }
            p.pos.z += 1.0;
        }
        p.prev = p.pos;
    }

    /// One tick.
    /// A tick with these keys held (tests and tools; the game's input is
    /// `run`'s).
    pub fn step(&mut self, keys: &Controls) {
        let input = keys.input(&self.rules.binds.actions);
        self.run(&input);
    }

    /// One tick of its rules: the player, enemies, gunships, hostages,
    /// grenades and bullets, then the core's debris and light, the level's
    /// rules, and the camera following the player. A question it asks the
    /// game stays open (`asked`) until `settle_rules`.
    pub(crate) fn run(&mut self, input: &Input) {
        if !self.sim.begin_tick() {
            self.hold();
            return;
        }
        let input = if self.status.phase == Phase::Playing { *input } else { Input::default() };
        self.busy.retain(|_, n| {
            *n -= 1;
            *n > 0
        });
        if self.several() {
            self.update_seats(&input);
        } else {
            self.update_player(&input);
        }
        self.update_enemies();
        self.update_helicopters();
        self.update_hostages();
        self.update_grenades();
        self.update_bullets();
        let half = self.sim.half();
        // (The first player's place kept clear of debris.)
        let p = self.fighters().into_iter().next().map_or_else(|| Body::new(Vec3::splat(-1e6), 0.0), |f| f.body);
        let keep_clear = (p.pos - Vec3::new(half, half, 0.0), p.pos + Vec3::new(half, half, p.height));
        self.sim.update_particles(keep_clear);
        self.sim.update_flares();
        self.update_level();
        let watched = match self.watch {
            crate::sim::Watch::Player => Some(self.players_at()),
            crate::sim::Watch::Thing(id) => self.things.get(id).map(|t| t.body.pos),
            crate::sim::Watch::Still => Some(self.sim.cam_target),
        };
        // A thing followed that is gone: the player again.
        let target = watched.unwrap_or_else(|| {
            self.watch = crate::sim::Watch::Player;
            self.players_at()
        });
        self.sim.follow(target);
    }

    /// The first seat's figure that stands, in seat order.
    fn first_seated(&self) -> Option<&Thing> {
        self.things.iter().filter_map(|t| match &t.brain {
            Brain::Player(s) => Some((s.seat, t)),
            _ => None,
        }).min_by_key(|(seat, _)| *seat).map(|(_, t)| t)
    }

    /// Whether it plays several (`c.players`): every seat's player a
    /// figure (`Brain::Player`), the kit's own player out of it.
    pub(crate) fn several(&self) -> bool {
        self.sim.cfg.players > 1
    }

    /// Those on the players' side, in seat order: the kit's own player in
    /// a game of one, each seat's figure in a game of several.
    pub(crate) fn fighters(&self) -> Vec<Fighter> {
        let on = self.status.phase == Phase::Playing;
        if !self.several() {
            return vec![Fighter { who: crate::sim::Who::Player, body: self.player.body.clone(), team: self.seat_team(1), playing: on }];
        }
        let mut seated: Vec<(usize, Fighter)> = self
            .things
            .iter()
            .filter_map(|t| match &t.brain {
                Brain::Player(s) => Some((s.seat, Fighter { who: crate::sim::Who::Thing(t.id), body: t.body.clone(), team: self.seat_team(s.seat), playing: on && !s.down })),
                _ => None,
            })
            .collect();
        seated.sort_by_key(|(seat, _)| *seat);
        seated.into_iter().map(|(_, f)| f).collect()
    }

    /// Seat `seat`'s team (`p.state.team`, `PLAYER_TEAM` unless set).
    pub(crate) fn seat_team(&self, seat: usize) -> &'static str {
        self.seat_teams.get(seat.wrapping_sub(1)).copied().flatten().unwrap_or(PLAYER_TEAM)
    }

    /// The team of a player (the one, or a seat's figure).
    pub(crate) fn team_of(&self, who: crate::sim::Who) -> &'static str {
        match who {
            crate::sim::Who::Thing(id) => match self.things.get(id).map(|t| &t.brain) {
                Some(Brain::Player(s)) => self.seat_team(s.seat),
                _ => PLAYER_TEAM,
            },
            crate::sim::Who::Player => self.seat_team(1),
        }
    }

    /// Each seat's team, seat 1 first (`p.state.team`; none: `PLAYER_TEAM`).
    pub(crate) fn set_seat_teams(&mut self, teams: &[Option<&'static str>]) {
        self.seat_teams.clear();
        self.seat_teams.extend_from_slice(teams);
    }

    /// Seat `seat`'s figure in a game of several.
    pub(crate) fn seated(&self, seat: usize) -> Option<crate::sim::things::Id> {
        self.things.iter().find(|t| matches!(&t.brain, Brain::Player(s) if s.seat == seat)).map(|t| t.id)
    }

    /// The camera's mark: the one player; the middle of those in the
    /// fight (else of all of them), at the first one's height.
    fn players_at(&self) -> Vec3 {
        if !self.several() {
            return self.player.body.pos;
        }
        let all = self.fighters();
        let fighting: Vec<&Fighter> = all.iter().filter(|f| f.playing).collect();
        let group: Vec<&Fighter> = if fighting.is_empty() { all.iter().collect() } else { fighting };
        let Some(first) = group.first() else { return self.sim.cam_target };
        let (lo, hi) = group.iter().fold((first.body.pos, first.body.pos), |(lo, hi), f| (lo.min(f.body.pos), hi.max(f.body.pos)));
        ((lo + hi) * 0.5).truncate().extend(first.body.pos.z)
    }

    /// The seats' players, in seat order, each with what it holds.
    fn update_seats(&mut self, input: &Input) {
        let mut seats: Vec<(usize, crate::sim::things::Id)> = self.things.iter().filter_map(|t| match &t.brain {
            Brain::Player(s) => Some((s.seat, t.id)),
            _ => None,
        }).collect();
        seats.sort_by_key(|(seat, _)| *seat);
        for (seat, id) in seats {
            let held = match seat {
                1 => *input,
                n => self.seat_inputs.get(n - 2).copied().flatten().unwrap_or_default(),
            };
            let held = if self.status.phase == Phase::Playing { held } else { Input::default() };
            self.update_seated(id, &held);
        }
    }

    /// One seat's player a tick: as the one player moves and attacks;
    /// down, it lies there until the game's code gives it hit points.
    fn update_seated(&mut self, id: crate::sim::things::Id, input: &Input) {
        let Some(t) = self.things.get_mut(id) else { return };
        let Brain::Player(s) = &mut t.brain else { return };
        if s.down && t.hp > 0 {
            s.down = false;
        }
        let input = if s.down { Input::default() } else { *input };
        t.body.prev = t.body.pos;
        let walked = walk(&mut t.body, &mut s.jump_held, &input, &mut self.sim, &self.rules.binds);
        self.footsteps(Some(id), walked);
        let Some(t) = self.things.get_mut(id) else { return };
        let Brain::Player(s) = &mut t.brain else { return };
        s.invulnerable = s.invulnerable.saturating_sub(1);
        s.busy = s.busy.saturating_sub(1);
        let before = std::mem::replace(&mut s.held, input.actions);
        let who = crate::sim::Who::Thing(id);
        let rules = Arc::clone(&self.rules);
        for (slot, (action, choices)) in rules.binds.attacks.iter().enumerate() {
            let Some(Brain::Player(s)) = self.things.get_mut(id).map(|t| &mut t.brain) else { return };
            let cooldown = &mut s.cooldowns[slot];
            *cooldown = cooldown.saturating_sub(1);
            if !input.held(*action) || *cooldown > 0 {
                continue;
            }
            let pressed = before >> action & 1 == 0;
            let Some((weapon, target)) = self.first_that_can(who, choices) else { continue };
            if !rules.weapons[weapon].auto && !pressed {
                continue;
            }
            let wait = self.attack(who, weapon, target, None);
            if let Some(Brain::Player(s)) = self.things.get_mut(id).map(|t| &mut t.brain) {
                s.cooldowns[slot] = wait;
            }
        }
    }

    /// Puts the player on the ground at (x, y), the camera with him, as if
    /// he had started there (tools look at any place in a level this way).
    pub fn move_player(&mut self, x: f32, y: f32) {
        let height = self.player.body.height;
        let at = spawn_point(&self.sim.world, [x, y], height, self.sim.half());
        self.put_player(at);
    }

    /// Stands the player with its feet at `at` (a level's place): up out of
    /// anything it would stand in, down onto what is below.
    pub fn stand_player(&mut self, at: Vec3) {
        let at = crate::sim::stand_at(&self.sim.world, at, self.player.body.height, self.sim.half());
        self.put_player(at);
    }

    fn put_player(&mut self, at: Vec3) {
        let height = self.player.body.height;
        let b = &mut self.player.body;
        (b.pos, b.prev, b.vel) = (at, at, Vec3::ZERO);
        self.sim.cam_target = at + Vec3::Z * height * 0.5;
        self.sim.cam_prev = self.sim.cam_target;
    }

    /// The simulation's state as one number, the same on every machine for
    /// the same run: what replays check every second. It takes in what
    /// carries the game forward (the tick, the random generator, every
    /// voxel written, the player, the status, enemies, hostages, the
    /// gunships, pickups, grenades, bullets and debris); any difference
    /// shows in it within a tick or two.
    pub fn state_hash(&self) -> u64 {
        let mut h = crate::hash::Stable::default();
        h.u64(self.sim.tick).u64(self.sim.rng.state()).u64(self.sim.world.changes()).u64(self.sim.freeze as u64);
        let body = |h: &mut crate::hash::Stable, b: &Body| {
            h.vec3(b.pos).vec3(b.vel).i32(b.look.x).i32(b.look.y).u64(b.on_ground as u64);
        };
        body(&mut h, &self.player.body);
        h.u64(self.player.invulnerable as u64);
        let s = &self.status;
        let phase = match s.phase {
            Phase::Playing => 0,
            Phase::Down => 1,
            Phase::Won => 2,
            Phase::GameOver => 3,
        };
        h.u64(phase).i32(s.hp).u64(s.score as u64).u64(s.weapon.map_or(u64::MAX, |w| w as u64));
        h.u64(s.stock.len() as u64);
        for (item, n) in &s.stock {
            h.bytes(item.as_bytes()).u64(*n as u64);
        }
        // What the code's attacks wait for, only when it attacked.
        if !self.busy.is_empty() || self.player.busy > 0 {
            h.u64(self.player.busy as u64).u64(self.busy.len() as u64);
            for (id, n) in &self.busy {
                h.u64(id.raw() as u64).u64(*n as u64);
            }
        }
        // The things by what they are, each kind in the order they came.
        h.u64(self.things.iter().filter(|t| is_soldier(t)).count() as u64);
        for t in self.things.iter() {
            if let Brain::Soldier(_) = &t.brain {
                body(&mut h, &t.body);
                // Riflemen 0, grenadiers 1.
                h.i32(t.hp).u64(self.rules.weapons[self.rules.soldier(t.kind).weapon].thrown() as u64);
            }
        }
        h.u64(self.things.iter().filter(|t| is_captive(t)).count() as u64);
        for t in self.things.iter() {
            if let Brain::Captive(g) = &t.brain {
                body(&mut h, &t.body);
                h.u64(g.freed as u64);
            }
        }
        h.u64(self.gunships().count() as u64);
        for (t, c) in self.gunships() {
            h.vec3(t.body.pos).i32(t.hp).u64(c.state as u64);
        }
        h.u64(self.things.iter().filter(|t| matches!(t.brain, Brain::Pickup)).count() as u64);
        for t in self.things.iter() {
            if let Brain::Pickup = t.brain {
                let cell = t.body.pos.as_ivec3();
                h.i32(cell.x).i32(cell.y).i32(cell.z).u64(self.rules.pickup(t.kind).frame as u64);
            }
        }
        h.u64(self.grenades.len() as u64);
        for g in &self.grenades {
            h.vec3(g.pos);
        }
        h.u64(self.bullets.len() as u64);
        for b in &self.bullets {
            h.vec3(b.pos).u64(b.hostile as u64);
        }
        h.u64(self.sim.particles.len() as u64);
        for q in &self.sim.particles {
            h.vec3(q.pos);
        }
        // A Lua game's own things, when it has any (so the hash of a game
        // without stays as it was).
        let scripted = self.things.iter().filter(|t| matches!(t.brain, Brain::Script(_))).count();
        if scripted > 0 {
            h.u64(scripted as u64);
            for t in self.things.iter().filter(|t| matches!(t.brain, Brain::Script(_))) {
                body(&mut h, &t.body);
                h.u64(t.id.raw() as u64).u64(t.kind as u64).i32(t.hp);
            }
        }
        // The seats' players in a game of several, by seat.
        let seats: Vec<Fighter> = if self.several() { self.fighters() } else { Vec::new() };
        if !seats.is_empty() {
            h.u64(seats.len() as u64);
            for f in &seats {
                let crate::sim::Who::Thing(id) = f.who else { continue };
                let t = self.things.get(id).expect("a seat's figure");
                let Brain::Player(s) = &t.brain else { continue };
                body(&mut h, &t.body);
                h.u64(s.seat as u64).i32(t.hp).u64(s.invulnerable as u64).u64(s.busy as u64).u64(s.down as u64).u64(s.weapon.map_or(u64::MAX, |w| w as u64));
                h.u64(s.stock.len() as u64);
                for (item, n) in &s.stock {
                    h.bytes(item.as_bytes()).u64(*n as u64);
                }
            }
        }
        // Things whose rules the code switched off, when there are any.
        let off: Vec<&Thing> = self.things.iter().filter(|t| t.off.is_some() && !matches!(t.brain, Brain::Script(_))).collect();
        if !off.is_empty() {
            h.u64(off.len() as u64);
            for t in off {
                body(&mut h, &t.body);
                h.u64(t.id.raw() as u64);
            }
        }
        h.finish()
    }

    fn sound(&mut self, sfx: Sfx, at: Vec3, volume: f32) {
        self.sim.sound(sfx.name(), at, volume);
    }

    /// Hit stop: the shooter's things stay where they are this tick, drawn
    /// still (the core holds its own, `Sim::begin_tick`).
    fn hold(&mut self) {
        let p = &mut self.player.body;
        p.prev = p.pos;
        // Not the gunships: they slide on through the hit stop.
        for t in self.things.iter_mut().filter(|t| is_soldier(t) || is_captive(t) || matches!(t.brain, Brain::Script(_) | Brain::Player(_)) || t.off.is_some()) {
            t.body.prev = t.body.pos;
        }
        for b in &mut self.bullets {
            b.prev = b.pos;
        }
        for g in &mut self.grenades {
            g.prev = g.pos;
        }
    }

    /// Dust from a body's steps (one puff a stride) and from landing, as
    /// its kind (or the player's config) says.
    fn footsteps(&mut self, thing: Option<crate::sim::things::Id>, walked_before: f32) {
        let (body, steps) = match thing {
            None => (&self.player.body, self.cfg.player.steps()),
            Some(id) => {
                let t = self.things.get(id).expect("a walker in the list");
                let steps = match (&t.brain, &self.rules.kinds[t.kind].rules) {
                    (Brain::Player(_), _) => self.cfg.player.steps(),
                    (_, kinds::KindRules::Soldier(r)) => r.steps,
                    _ => kinds::Steps::default(),
                };
                (&t.body, steps)
            }
        };
        let player = thing.is_none_or(|id| self.things.get(id).is_some_and(|t| matches!(t.brain, Brain::Player(_))));
        let stride = steps.stride.unwrap_or(body.height * 0.5).max(1.0);
        let stepped = steps.step_dust > 0 && body.on_ground && (walked_before / stride).floor() != (body.walked / stride).floor();
        let landed = body.fell > steps.landing;
        if !(stepped || landed) {
            return;
        }
        let feet = body.pos;
        if landed && player {
            let loud = (body.fell / body.height).clamp(0.3, 1.0);
            self.sound(Sfx::Land, feet, loud);
        }
        let below = feet.floor().as_ivec3() - IVec3::Z;
        let v = self.sim.world.get(below);
        if v == 0 {
            return;
        }
        let color = self.sim.world.palette[v as usize];
        self.sim.dust(feet + Vec3::Z * 0.5, if landed { steps.landing_dust } else { steps.step_dust } as usize, color);
    }

    fn update_player(&mut self, input: &Input) {
        let p = &mut self.player;
        p.body.prev = p.body.pos;
        let walked = walk(&mut p.body, &mut p.jump_held, input, &mut self.sim, &self.rules.binds);
        self.footsteps(None, walked);
        let p = &mut self.player;

        p.invulnerable = p.invulnerable.saturating_sub(1);
        p.busy = p.busy.saturating_sub(1);
        let before = std::mem::replace(&mut p.held, input.actions);
        // Each action attacks with the first of its weapons that can, while
        // held (a weapon that is not `auto`: once a press).
        let rules = Arc::clone(&self.rules);
        for (slot, (action, choices)) in rules.binds.attacks.iter().enumerate() {
            let cooldown = &mut self.player.cooldowns[slot];
            *cooldown = cooldown.saturating_sub(1);
            if !input.held(*action) || *cooldown > 0 {
                continue;
            }
            let pressed = before >> action & 1 == 0;
            let Some((weapon, target)) = self.first_that_can(crate::sim::Who::Player, choices) else { continue };
            if !rules.weapons[weapon].auto && !pressed {
                continue;
            }
            self.player.cooldowns[slot] = self.attack(crate::sim::Who::Player, weapon, target, None);
        }
    }

    fn update_bullets(&mut self) {
        let world = &self.sim.world;
        let size = world.size;
        let half = self.sim.half();
        // Gunships, enemies and the game's own things with hit points as they
        // stand, each in the order they came, with their teams: a round hits
        // the first it reaches that it can hurt, by its place among them.
        let kinds = &self.rules.kinds;
        let gunships: Vec<(&Body, [f32; 3], usize)> =
            self.things.iter().filter(|t| is_gunship(t)).map(|t| (&t.body, self.rules.gunship(t.kind).size, t.kind)).collect();
        let enemies: Vec<(&Body, usize)> = self.things.iter().filter(|t| is_soldier(t)).map(|t| (&t.body, t.kind)).collect();
        let others: Vec<(crate::sim::things::Id, &Body, usize)> = self
            .things
            .iter()
            .filter(|t| {
                t.hp > 0
                    && match &t.brain {
                        Brain::Script(_) => true,
                        Brain::Captive(c) => !c.freed,
                        _ => false,
                    }
            })
            .map(|t| (t.id, &t.body, t.kind))
            .collect();
        let can = |team: Option<&str>, kind: usize| !kinds[kind].immune.round && kinds::hurts(team, kinds[kind].team);
        let mut heli_hits = Vec::new();
        // The players in the fight, in seat order: an enemy's round hits
        // the first it reaches.
        let fighters = self.fighters();
        let mut impacts = Vec::new();
        let mut enemy_hits = Vec::new();
        let mut other_hits = Vec::new();
        let mut player_hits: Vec<(i32, Option<usize>, Option<crate::sim::things::Id>)> = vec![(0, None, None); fighters.len()];
        self.bullets.retain_mut(|b| {
            b.prev = b.pos;
            b.life -= DT;
            if b.life <= 0.0 {
                return false;
            }
            let d = b.vel * DT;
            let steps = (d.length() / 0.5).ceil().max(1.0) as i32;
            for _ in 0..steps {
                b.pos += d / steps as f32;
                let cell = b.pos.floor().as_ivec3();
                if cell.x < 0 || cell.y < 0 || cell.z < 0 || cell.x >= size.x || cell.y >= size.y {
                    return false;
                }
                // A player of another team than the round's (an enemy's,
                // or another player's in a game of teams).
                if let Some(i) = fighters.iter().position(|f| f.playing && kinds::hurts(b.team, Some(f.team)) && f.body.contains(b.pos, half, 0.3)) {
                    player_hits[i].0 += b.damage;
                    if player_hits[i].1.is_none() {
                        (player_hits[i].1, player_hits[i].2) = (Some(b.weapon), b.from);
                    }
                    return false;
                }
                if let Some(i) = gunships.iter().position(|(g, size, kind)| can(b.team, *kind) && helicopter::contains(g, b.pos, *size, 0.5)) {
                    heli_hits.push((i, b.damage, b.pos, b.blast, b.rocket, b.weapon, b.team, b.from));
                    return false;
                } else if let Some(i) = enemies.iter().position(|(e, kind)| can(b.team, *kind) && e.contains(b.pos, half, 0.5)) {
                    enemy_hits.push((i, b.damage, b.pos, b.vel, b.blast, b.rocket, b.weapon, b.team, b.from));
                    return false;
                } else if let Some(&(id, ..)) = others.iter().find(|(_, o, kind)| can(b.team, *kind) && o.contains(b.pos, half, 0.5)) {
                    other_hits.push((id, b.damage, b.pos, b.blast, b.rocket, b.weapon, b.team, b.from));
                    return false;
                }
                if world.solid(cell) {
                    impacts.push((b.pos, b.blast, b.rocket, b.hostile, b.weapon, b.team, b.from));
                    return false;
                }
            }
            true
        });
        let rules = Arc::clone(&self.rules);
        let name = |w: usize| Some(rules.weapon_names[w].clone());
        let one = self.seat_team(1);
        let by = |team: Option<&str>, from: Option<crate::sim::things::Id>| match from {
            Some(id) => Some(crate::sim::Who::Thing(id)),
            None => (team == Some(one)).then_some(crate::sim::Who::Player),
        };
        for (pos, blast, rocket, hostile, weapon, team, from) in impacts {
            // Rounds hitting around them give the player away.
            if !hostile {
                self.alert(pos, combat::Noise::Impact);
            }
            if rocket {
                self.blast(pos, blast, Some(weapon), team, by(team, from));
            } else if blast > 0.0 {
                self.sim.explode(pos, blast);
            } else {
                self.sim.sparks(pos, 4);
                self.sound(Sfx::Impact, pos, 1.0);
            }
        }
        // A hit goes to whoever stands at that place among them now (an
        // earlier blast or kill this tick moves the later ones up, and the
        // round hits the one who moved up: how hits have always counted).
        // Gunships stay in the list while they fall.
        for (i, damage, pos, blast, rocket, weapon, team, from) in heli_hits {
            if rocket {
                self.blast(pos, blast, Some(weapon), team, by(team, from));
            } else if let Some(id) = self.things.nth(i, is_gunship) {
                self.damage_helicopter(id, damage, by(team, from), name(weapon));
                self.sim.sparks(pos, 3);
            }
        }
        // The last place first, so a fall keeps the places before it.
        enemy_hits.sort_by_key(|&(i, ..)| std::cmp::Reverse(i));
        for (i, damage, pos, vel, blast, rocket, weapon, team, from) in enemy_hits {
            if rocket {
                self.blast(pos, blast, Some(weapon), team, by(team, from));
            } else if let Some(id) = self.things.nth(i, is_soldier) {
                let survives = self.things.get(id).is_some_and(|t| t.hp > damage);
                self.damage_enemy_by(id, damage, false, by(team, from), name(weapon));
                if survives {
                    self.sim.blood_spray(pos, vel);
                }
            }
        }
        for (id, damage, pos, blast, rocket, weapon, team, from) in other_hits {
            if rocket {
                self.blast(pos, blast, Some(weapon), team, by(team, from));
            } else {
                self.damage_thing(id, damage, false, by(team, from), name(weapon));
                self.sim.sparks(pos, 3);
            }
        }
        for (f, (damage, weapon, from)) in fighters.iter().zip(player_hits) {
            if damage > 0 {
                self.hurt_fighter(f.who, damage, from.map(crate::sim::Who::Thing), weapon.and_then(name));
            }
        }
    }

    /// The gunships and their minds, in the order they came.
    pub fn gunships(&self) -> impl Iterator<Item = (&Thing, &Gunship)> {
        self.things.iter().filter_map(|t| match &t.brain {
            Brain::Gunship(g) => Some((t, g)),
            _ => None,
        })
    }

    /// The enemies and their minds, in the order they came.
    pub fn enemies(&self) -> impl Iterator<Item = (&Thing, &Soldier)> {
        self.things.iter().filter_map(|t| match &t.brain {
            Brain::Soldier(s) => Some((t, s)),
            _ => None,
        })
    }
}

#[cfg(test)]
impl Shooter {
    /// The `n`th enemy and its mind (tests).
    pub(crate) fn enemy(&self, n: usize) -> (&Thing, &Soldier) {
        self.enemies().nth(n).expect("no such enemy")
    }

    pub(crate) fn enemy_mut(&mut self, n: usize) -> &mut Soldier {
        self.things.iter_mut().filter_map(|t| if let Brain::Soldier(s) = &mut t.brain { Some(s) } else { None }).nth(n).expect("no such enemy")
    }

    /// The `n`th gunship and its mind (tests).
    pub(crate) fn gunship(&self, n: usize) -> (&Thing, &Gunship) {
        self.gunships().nth(n).expect("no such gunship")
    }
}

/// A game's answer to a question of the shooter's (`Shooter::asked`), by
/// its name; None: the kit's own rule holds.
/// What the simulation needs to know of the models: sizes and the voxels to
/// burst into. The player's is required; missing ones stand in with its
/// size, a missing grenadier with the enemy.
fn shapes_of(models: &Models, rules: &Rules) -> anyhow::Result<Shapes> {
    let player = models.shape("player").context("the player has no model (c.player.model)")?;
    if let Some(m) = models.get("player")
        && let Some(missing) = rules.player_swing.keys().find(|k| !m.parts.iter().any(|p| &p.name == *k))
    {
        let names: Vec<&str> = m.parts.iter().map(|p| p.name.as_str()).collect();
        anyhow::bail!("c.player.swing.{missing}: no such part of the player's model (its parts: {})", if names.is_empty() { "none, it is no figure of parts".to_string() } else { names.join(", ") });
    }
    let files = rules.kinds.iter().filter_map(|k| k.model.clone());
    Ok(Shapes::new(player, files.filter_map(|f| Some((f.clone(), models.shape(&f)?))).collect()))
}

/// The files of the kinds' models, each once, and whether its shape counts
/// for the simulation (walkers and gunships collide and burst by theirs).
fn model_files(rules: &Rules) -> Vec<(String, bool)> {
    let mut files: Vec<(String, bool)> = Vec::new();
    for kind in &rules.kinds {
        let Some(file) = &kind.model else { continue };
        let shaped = matches!(kind.rules, KindRules::Soldier(_) | KindRules::Captive(_) | KindRules::Gunship(_) | KindRules::Script);
        match files.iter_mut().find(|(f, _)| f == file) {
            Some((_, s)) => *s |= shaped,
            None => files.push((file.clone(), shaped)),
        }
    }
    files
}

impl Game for Shooter {
    type Snapshot = Shooter;

    fn sections(_cfg: &GameConfig) -> Vec<&'static str> {
        ShooterConfig::SECTIONS.to_vec()
    }

    fn behaviours(_cfg: &GameConfig) -> Vec<&'static str> {
        BEHAVIOURS.to_vec()
    }

    /// The actions its player's attacks and moves name (`c.player.attacks`,
    /// `moves`); none if they cannot be read (loading says why).
    fn actions(cfg: &GameConfig) -> Vec<&'static str> {
        ShooterConfig::from_game(cfg).map(|own| kinds::Binds::actions_of(&own.player)).unwrap_or_default()
    }

    fn default_keys(_cfg: &GameConfig) -> Bindings {
        keys()
    }

    fn default_pad(_cfg: &GameConfig) -> Bindings {
        pad()
    }

    fn sounds(_cfg: &GameConfig) -> Vec<(&'static str, SoundDef)> {
        sounds()
    }

    fn snapshot(&self) -> anyhow::Result<Shooter> {
        Ok(self.clone())
    }

    fn restore(&mut self, kept: &Shooter) {
        *self = kept.clone();
    }

    /// The player's model (`[player] model`) and the kinds' (`[things.NAME]
    /// model`); a pickup's shape does not count for the simulation.
    fn models(cfg: &GameConfig) -> anyhow::Result<Vec<ModelSpec>> {
        // The models' files only: their heights are not known yet.
        let rules = Rules::from_game(cfg, &ShooterConfig::from_game(cfg)?, &|_| None)?;
        let player = ModelSpec { name: "player".into(), file: cfg.player.model.clone(), shaped: true };
        let kinds = model_files(&rules).into_iter().map(|(file, shaped)| ModelSpec { name: file.clone(), file, shaped });
        Ok(std::iter::once(player).chain(kinds).collect())
    }

    fn load(setup: Setup) -> anyhow::Result<Self> {
        let own = ShooterConfig::from_game(setup.cfg)?;
        let rules = Rules::from_game(setup.cfg, &own, &|file| setup.models.shape(file).map(|s| s.height()))?;
        // A hostage that runs to a place: some level has it.
        for k in &rules.kinds {
            if let KindRules::Captive(kinds::CaptiveRules { flee: kinds::Flee::Place(place), .. }) = &k.rules
                && !setup.levels.list.iter().any(|l| l.places.iter().any(|p| &p.name == place))
            {
                anyhow::bail!("c.things.{}.flee = \"{place}\": no level has a place called {place} (\"away\", or a way such as {{ 0, -1 }})", k.name);
            }
        }
        // Several players: each a figure of a kind of the game's.
        if setup.cfg.players > 1 && setup.cfg.player.kind.is_none() {
            anyhow::bail!("c.players = {}: each player's figure is a kind of thing without behaviour (c.things.hero = {{ model = \"player.vvx\" }}), named by c.player.kind = {{ \"hero\", \"hero2\" }} (or one for all)", setup.cfg.players);
        }
        let shapes = shapes_of(setup.models, &rules)?;
        let places = setup.levels.list.first().map(|l| l.places.clone()).unwrap_or_default();
        let mut shooter = Self::with_rules(setup.world, setup.cfg, own, rules, shapes, &setup.markers);
        shooter.places = places.into();
        Ok(shooter)
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

    fn state_hash(&self) -> u64 {
        Shooter::state_hash(self)
    }

    fn draw(&self, out: &mut Drawing) {
        self.draw_into(out);
    }

    fn focus(&self) -> Option<&Body> {
        match self.watch {
            crate::sim::Watch::Player if self.several() => self.first_seated().map(|t| &t.body),
            crate::sim::Watch::Player => Some(&self.player.body),
            crate::sim::Watch::Thing(id) => self.things.get(id).map(|t| &t.body),
            crate::sim::Watch::Still => None,
        }
    }

    fn outcome(&self) -> Option<Outcome> {
        match self.status.phase {
            Phase::Won => Some(Outcome::Won),
            Phase::GameOver => Some(Outcome::Lost),
            _ => None,
        }
    }

    fn scored(&self) -> bool {
        self.status_line().is_some()
    }

    fn score(&self) -> u32 {
        self.status.score
    }

    /// Hard when the player is hit or goes down, by how close an explosion
    /// was, a little for every shot.
    fn rumble(&self) -> Option<(f32, u32)> {
        let mut strength = 0.0f32;
        let mut ms = 0;
        let player = self.fighters().first().map_or(self.sim.cam_target, |f| f.body.centre());
        let reach = self.sim.cfg.camera.view_width * 0.5;
        for event in &self.sim.sounds {
            let (s, t) = match event.name {
                "down" => (1.0, 450),
                "hurt" => (0.7, 200),
                "explosion" => ((1.0 - event.at.distance(player) / reach).max(0.0) * event.volume, 220),
                "shot" => (0.12, 40),
                _ => continue,
            };
            if s > strength {
                strength = s;
                ms = t;
            }
        }
        (strength > 0.05).then_some((strength, ms))
    }

    fn summary(&self) -> Option<String> {
        self.status_line()
    }

    fn numbers(&self) -> Vec<String> {
        self.rules.numbers()
    }

    fn report(&self) -> Vec<String> {
        let first = self.fighters().into_iter().next().map_or_else(|| self.player.body.clone(), |f| f.body);
        let p = &first;
        let size = self.sim.world.size;
        let mut lines = vec![format!(
            "player pos ({:.1}, {:.1}, {:.1}) on_ground {} | world {}x{}x{} | {} enemies, {} alerted | {} bullets",
            p.pos.x,
            p.pos.y,
            p.pos.z,
            p.on_ground,
            size.x,
            size.y,
            size.z,
            self.enemies().count(),
            self.alerted_enemies(),
            self.bullets.len()
        )];
        lines.extend(self.status_line());
        for (t, g) in self.gunships() {
            let (p, look) = (t.body.pos, t.body.look);
            lines.push(format!("gunship {:?} at ({:.1}, {:.1}, {:.1}) facing ({}, {}) hp {}", g.state, p.x, p.y, p.z, look.x, look.y, t.hp));
        }
        lines
    }

    fn place_player(&mut self, x: f32, y: f32) {
        self.move_player(x, y);
    }

    fn apply_config(&mut self, cfg: &GameConfig) -> anyhow::Result<()> {
        Shooter::apply_config(self, cfg)
    }

    fn replace_world(&mut self, world: World, markers: &[Marker]) {
        Shooter::replace_world(self, world, markers);
    }

    fn reshape(&mut self, models: &Models) {
        match shapes_of(models, &self.rules) {
            Ok(shapes) => self.set_shapes(shapes),
            Err(e) => eprintln!("{e:#}"),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sim::tests::flat_world;
    use crate::sim::loose_piece;
    use crate::world::CellSet;
    use glam::IVec2;

    /// A config with the kit's kinds of things and weapons (`kit.toml`).
    pub(crate) fn kit() -> GameConfig {
        let kit: toml::Table = toml::from_str(kinds::KIT).unwrap();
        let things = kit["things"].as_table().unwrap().iter().map(|(k, v)| (k.clone(), v.as_table().unwrap().clone())).collect();
        let mut cfg = GameConfig { things, ..GameConfig::default() };
        cfg.game.insert("weapons".into(), kit["weapons"].clone());
        // A run as More Blood starts one, its attacks and moves.
        cfg.state.player = toml::from_str("hp = 5\nweapon = \"rifle\"\ninventory = { grenade = 5 }").unwrap();
        let player: toml::Table = toml::from_str("invulnerable = 75\nmoves = [\"jump\", \"strafe\"]\n[attacks]\nfire = [\"knife\", \"held\", \"rifle\"]\ngrenade = [\"grenade\"]\n").unwrap();
        cfg.player.game.extend(player);
        cfg
    }

    /// Sets `key` of the kind `kind` (`[things.kind]`).
    pub(crate) fn set(cfg: &mut GameConfig, kind: &str, key: &str, value: impl Into<toml::Value>) {
        cfg.things.get_mut(kind).expect("a kind in the config").insert(key.into(), value.into());
    }

    /// Sets `key` of `[weapons]` (`aim_assist`, or a weapon's table).
    pub(crate) fn set_weapons(cfg: &mut GameConfig, key: &str, value: impl Into<toml::Value>) {
        let weapons = cfg.game.entry("weapons").or_insert_with(|| toml::Value::Table(toml::Table::new()));
        weapons.as_table_mut().unwrap().insert(key.into(), value.into());
    }

    /// A marker for a thing of the kind called `kind` in `cfg`, at `cell`.
    pub(crate) fn marker(cfg: &GameConfig, kind: &str, cell: IVec3) -> Marker {
        let kind = crate::sim::things::names(cfg).iter().position(|n| *n == kind).expect("a kind in the config");
        Marker { kind, cell }
    }

    pub(crate) fn shapes() -> Shapes {
        Shapes::new(Shape::block(IVec3::new(5, 4, 10)), Default::default())
    }

    /// A changed config keeps every thing by the name of its kind,
    /// wherever the kind now stands in the list; a kind gone takes its
    /// things with it.
    #[test]
    fn a_reload_keeps_things_by_their_kind() {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        let markers = ["rifleman", "guard", "hostage"].map(|k| marker(&cfg, k, IVec3::new(64, 200, 1)));
        let mut sim = Shooter::new(flat_world(IVec3::new(128, 256, 16)), &cfg, shapes(), &markers);
        cfg.things.remove("guard");
        cfg.things.insert("a_crate".into(), toml::from_str("reach = 3").unwrap());
        sim.apply_config(&cfg).unwrap();
        let kinds: Vec<&str> = sim.things.iter().map(|t| sim.rules.kinds[t.kind].name.as_str()).collect();
        assert_eq!(kinds, ["rifleman", "hostage"]);
        for _ in 0..30 {
            sim.step(&Controls::default());
        }
    }

    /// The dust a figure's steps and landings raise is the game's
    /// (`c.player.stride`, `step_dust`, `landing`, `landing_dust`; a
    /// soldier's in its kind), and how the player blinks (`c.player.blink`).
    #[test]
    fn the_dust_of_steps_is_the_games() {
        let walk = |player: &str| {
            let mut cfg = kit();
            cfg.player.game.extend(toml::from_str::<toml::Table>(player).unwrap());
            let mut sim = Shooter::new(flat_world(IVec3::new(64, 160, 16)), &cfg, shapes(), &[]);
            let mut specks = 0;
            for _ in 0..90 {
                sim.sim.particles.clear();
                sim.step(&Controls { up: true, ..Controls::default() });
                specks += sim.sim.particles.len();
            }
            (specks, sim.cfg.player.blink)
        };
        let (usual, long, none) = (walk(""), walk("stride = 20\nblink = 0"), walk("step_dust = 0\nlanding_dust = 0"));
        assert!(usual.0 > long.0 && long.0 > 0 && none.0 == 0, "specks: {usual:?} {long:?} {none:?}");
        assert_eq!((usual.1, long.1), (4, 0));
    }

    /// Holding strafe keeps facing and aim while the player walks sideways.
    #[test]
    fn strafing_keeps_facing() {
        let mut sim = Shooter::new(flat_world(IVec3::new(64, 64, 16)), &kit(), shapes(), &[]);
        for _ in 0..5 {
            sim.step(&Controls { up: true, ..Controls::default() });
        }
        let x_before = sim.player.body.pos.x;
        for _ in 0..30 {
            sim.step(&Controls { left: true, strafe: true, fire: true, ..Controls::default() });
        }
        assert_eq!(sim.player.body.facing, Vec3::Y);
        assert_eq!(sim.player.body.look, IVec2::Y);
        assert!(sim.player.body.pos.x < x_before - 10.0, "the player should have moved left");
        assert!(sim.bullets.iter().all(|b| b.pos.y > b.prev.y && b.pos.x == b.prev.x), "shots fly straight ahead");
    }

    /// A run starts with the player facing south, or as `c.player.facing`
    /// says.
    #[test]
    fn the_player_starts_facing_the_games_way() {
        let sim = Shooter::new(flat_world(IVec3::new(96, 96, 16)), &kit(), shapes(), &[]);
        assert_eq!(sim.player.body.look, IVec2::NEG_Y);
        let mut cfg = kit();
        cfg.player.game.insert("facing".into(), vec![1, 0].into());
        let sim = Shooter::new(flat_world(IVec3::new(96, 96, 16)), &cfg, shapes(), &[]);
        assert_eq!((sim.player.body.facing, sim.player.body.look), (Vec3::X, IVec2::X));
    }

    /// Strafing after running diagonally fires the way the model looks,
    /// along it, not diagonally.
    #[test]
    fn strafing_fires_the_way_the_model_looks() {
        let mut sim = Shooter::new(flat_world(IVec3::new(96, 96, 16)), &kit(), shapes(), &[]);
        for _ in 0..5 {
            sim.step(&Controls { up: true, right: true, ..Controls::default() });
        }
        let facing = sim.player.body.facing;
        assert!(facing.x != 0.0 && facing.y != 0.0, "running diagonally should face diagonally");
        let look = sim.player.body.look;
        for _ in 0..30 {
            sim.step(&Controls { down: true, strafe: true, fire: true, ..Controls::default() });
        }
        assert_eq!(sim.player.body.look, look, "strafing turned the model");
        assert!(!sim.bullets.is_empty(), "no shots");
        for b in &sim.bullets {
            let flew = (b.pos - b.prev).truncate().normalize();
            assert!(flew.abs_diff_eq(look.as_vec2(), 1e-5), "a shot flew {flew}, the model looks {look}");
        }
    }

    /// A diagonal line (a palm frond, a leaning trunk) stands as long as its
    /// foot does, and comes down as a whole once the foot is blown away.
    #[test]
    fn diagonal_lines_hold_together() {
        let mut world = flat_world(IVec3::new(48, 48, 32));
        let line: Vec<IVec3> = (1..20).map(|i| IVec3::new(10 + i, 24, i)).collect();
        for &c in &line {
            world.set_raw(c, 1);
        }
        let mut cfg = GameConfig::default();
        cfg.player.spawn = [40.0, 4.0];
        let mut sim = Shooter::new(world, &cfg, shapes(), &[]);
        let standing = |sim: &Shooter| line.iter().filter(|&&c| sim.world.get(c) != 0).count();

        // A blast next to the line, not on it, leaves it standing.
        sim.explode(Vec3::new(24.5, 30.5, 10.5), 2.5);
        for _ in 0..300 {
            sim.step(&Controls::default());
        }
        assert_eq!(standing(&sim), line.len(), "a diagonal line fell apart without being hit");

        // Without its foot the rest falls.
        sim.explode(Vec3::new(11.5, 24.5, 1.5), 1.6);
        for _ in 0..600 {
            sim.step(&Controls::default());
        }
        assert_eq!((8..32).flat_map(|z| (0..48).flat_map(move |y| (0..48).map(move |x| IVec3::new(x, y, z))))
            .filter(|&c| sim.world.get(c) != 0).count(), 0, "the line is still up in the air");
    }

    /// Grass and shrubs (soft voxels) let the player and bullets through,
    /// but a blast still clears them.
    #[test]
    fn soft_voxels_let_bodies_and_bullets_through() {
        let mut world = flat_world(IVec3::new(64, 64, 16));
        world.palette[2] = [90, 140, 60, 255];
        world.soft[2] = true;
        for x in 0..64 {
            for z in 1..6 {
                world.set_raw(IVec3::new(x, 30, z), 2);
            }
        }
        let mut cfg = GameConfig::default();
        cfg.player.spawn = [32.0, 20.0];
        let mut sim = Shooter::new(world, &cfg, shapes(), &[]);
        sim.player.body.face(Vec3::Y);
        sim.step(&Controls { fire: true, ..Controls::default() });
        for _ in 0..30 {
            sim.step(&Controls::default());
        }
        assert!(sim.particles.is_empty(), "the shot hit the grass");
        assert_eq!(sim.world.get(IVec3::new(32, 30, 3)), 2, "the shot cut the grass");
        for _ in 0..40 {
            sim.step(&Controls { up: true, ..Controls::default() });
        }
        assert!(sim.player.body.pos.y > 34.0, "the grass stopped the player at y {}", sim.player.body.pos.y);
        assert!(sim.player.body.pos.z < 1.5, "the player climbed onto the grass");
        sim.explode(Vec3::new(10.5, 30.5, 2.5), 3.0);
        assert_eq!(sim.world.get(IVec3::new(10, 30, 2)), 0, "the blast left the grass");
    }

    /// However often the same spot is bombed, the bottom layer stays: there
    /// is no blowing a hole through the floor.
    #[test]
    fn blasts_never_open_the_bottom() {
        let mut world = flat_world(IVec3::new(48, 48, 24));
        for z in 1..6 {
            for y in 0..48 {
                for x in 0..48 {
                    world.set_raw(IVec3::new(x, y, z), 1);
                }
            }
        }
        let mut cfg = GameConfig::default();
        cfg.player.spawn = [4.0, 4.0];
        let mut sim = Shooter::new(world, &cfg, shapes(), &[]);
        for _ in 0..6 {
            sim.explode(Vec3::new(24.5, 24.5, 1.5), 9.0);
            for _ in 0..30 {
                sim.step(&Controls::default());
            }
        }
        for y in 0..48 {
            for x in 0..48 {
                assert_ne!(sim.world.get(IVec3::new(x, y, 0)), 0, "a hole in the bottom at {x},{y}");
            }
        }
        assert_eq!(sim.world.get(IVec3::new(24, 24, 1)), 0, "the blast should reach the bottom");
    }

    /// A pillar with a slab on top: blowing the pillar apart in the middle
    /// must bring the upper part down instead of leaving it in the sky.
    #[test]
    fn pieces_cut_off_from_the_ground_fall() {
        let mut world = flat_world(IVec3::new(24, 24, 24));
        for z in 1..14 {
            world.set_raw(IVec3::new(12, 12, z), 1);
        }
        for y in 10..15 {
            for x in 10..15 {
                world.set_raw(IVec3::new(x, y, 14), 1);
            }
        }
        let mut cfg = GameConfig::default();
        cfg.player.spawn = [3.0, 3.0];
        let mut sim = Shooter::new(world, &cfg, shapes(), &[]);

        let above = |sim: &Shooter| {
            (7..24).flat_map(|z| (0..24).flat_map(move |y| (0..24).map(move |x| IVec3::new(x, y, z))))
                .filter(|&c| sim.world.get(c) != 0)
                .count()
        };
        // Voxels that do not connect to the bottom layer.
        let floating = |sim: &Shooter| {
            let mut grounded = CellSet::default();
            let cells: Vec<_> = (1..24)
                .flat_map(|z| (0..24).flat_map(move |y| (0..24).map(move |x| IVec3::new(x, y, z))))
                .filter(|&c| sim.world.get(c) != 0)
                .collect();
            let mut loose = 0;
            for c in cells {
                if !grounded.contains(&c)
                    && let Some(piece) = loose_piece(&sim.world, c, &mut grounded)
                {
                    loose += piece.len();
                }
            }
            loose
        };
        assert_eq!(above(&sim), 7 + 25);
        assert_eq!(floating(&sim), 0);

        sim.explode(Vec3::new(12.5, 12.5, 5.5), 1.6);
        for _ in 0..600 {
            sim.step(&Controls::default());
        }
        assert_eq!(floating(&sim), 0, "voxels left hanging after the blast");
        for z in 13..24 {
            for y in 0..24 {
                for x in 0..24 {
                    assert_eq!(sim.world.get(IVec3::new(x, y, z)), 0, "the slab is still up at z {z}");
                }
            }
        }
        let pile = (1..7)
            .flat_map(|z| (0..24).flat_map(move |y| (0..24).map(move |x| IVec3::new(x, y, z))))
            .filter(|&c| sim.world.get(c) != 0)
            .count();
        println!("voxels in the pile after the fall: {pile}");
        assert!(pile > 15, "the fallen piece should mostly settle on the ground");
    }
}

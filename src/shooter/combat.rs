//! What makes a level a fight: enemies that notice you, shoot and lob
//! grenades; your knife, rifle and grenades; pickups lying about, hostages
//! to free, the player going down. What a kill, a pickup, a checkpoint or
//! the goal means is the game's code. Everything here is placed by marker
//! voxels in the level; a level without markers has no fight at all.
//!
//! Distances and speeds come from the game's config (`[player]`,
//! `[weapons.NAME]`, `[things.NAME]`, read into `kinds::Rules`), in voxels;
//! what stays here are counts and times, which do not depend on how fine a
//! game's grid is.

use std::sync::Arc;

use glam::{IVec3, Vec3};

use super::kinds::{self, Choice, Flee, KindRules};
use super::{Brain, Bullet, Sfx, Shooter, is_captive, is_gunship, is_soldier, muzzle_at};
use crate::sim::things::Id;
use crate::sim::{Body, DT, Event, Marker, Who, body_hits, math};

/// With `[weapons] aim_assist`, how far off a shooter's line a shot finds
/// its target: voxels across per voxel ahead (about 8.5 degrees), on top of
/// the target's width.
const AIM_CONE: f32 = 0.15;
/// Flight time of a grenade thrown the full distance, in seconds.
const THROW_TIME: f32 = 0.93;

/// What a soldier may hear: a shot fired, a round hitting near him, a
/// blast, a comrade falling (seen as far as he sees).
#[derive(Clone, Copy, Debug)]
pub(super) enum Noise {
    Shot,
    Impact,
    Blast,
    Fall,
    /// Heard this far (a weapon's `noise`), by those whose hearing reaches.
    Within(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    /// Hit too often: lying in pieces, out of the fight (no input, no
    /// target, nobody sent for him), until the game's code gives him hit
    /// points again.
    Down,
    Won,
    GameOver,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub phase: Phase,
    pub hp: i32,
    pub score: u32,
    /// The gun in the player's hand (`player.state.weapon`), by its place
    /// in `Rules::weapons`.
    pub weapon: Option<usize>,
    /// The rounds and grenades the player holds, by their item (a weapon
    /// with `ammo` is an item of its name): his inventory, which the
    /// game's code sees as `player.state.inventory`.
    pub stock: std::collections::BTreeMap<String, u32>,
}

impl Default for Status {
    fn default() -> Self {
        Self::new()
    }
}

impl Status {
    pub fn new() -> Self {
        Self { phase: Phase::Playing, hp: 0, score: 0, weapon: None, stock: Default::default() }
    }

    /// A run's start, as the game says (`c.state.player`: `hp`, `weapon`,
    /// `inventory`); what it leaves out it has none of.
    pub fn start(cfg: &crate::config::GameConfig, rules: &super::kinds::Rules) -> Self {
        let p = &cfg.state.player;
        let weapon = p.get("weapon").and_then(toml::Value::as_str).and_then(|name| rules.weapon_names.iter().position(|w| w == name));
        let mut stock = std::collections::BTreeMap::new();
        if let Some(toml::Value::Table(inventory)) = p.get("inventory") {
            for (item, n) in inventory {
                if let Some(n) = n.as_integer() {
                    stock.insert(item.clone(), n.max(0) as u32);
                }
            }
        }
        Self { hp: cfg.state.player_number("hp").unwrap_or(0) as i32, weapon, stock, ..Self::new() }
    }
}

/// A soldier's mind: whether he knows the player is here, when he may fire
/// again, when he next makes up his mind and where he heads meanwhile.
#[derive(Clone)]
pub struct Soldier {
    pub alert: bool,
    cooldown: u32,
    think: u32,
    wander: Vec3,
}

impl Soldier {
    fn new(alert: bool) -> Self {
        Self { alert, cooldown: 45, think: 1, wander: Vec3::ZERO }
    }
}

/// A hostage: freed and running off, and for how long.
#[derive(Clone)]
pub struct Captive {
    pub freed: bool,
    gone: u32,
}

#[derive(Clone)]
pub struct Grenade {
    pub pos: Vec3,
    pub prev: Vec3,
    vel: Vec3,
    fuse: u32,
    blast: f32,
    /// The weapon thrown (its place in `Rules::weapons`).
    weapon: usize,
    /// The thrower's team.
    team: Option<&'static str>,
    /// The player who threw it (whose kills its blast's are).
    by: Option<Who>,
}

impl Grenade {
    /// Thrown in an arc that comes down at `target`; `throw` is the distance
    /// of a full throw, which takes THROW_TIME.
    #[allow(clippy::too_many_arguments)]
    fn lobbed(from: Vec3, target: Vec3, throw: f32, blast: f32, gravity: f32, weapon: usize, team: Option<&'static str>, by: Option<Who>) -> Self {
        let flat = (target - from).truncate();
        let t = (THROW_TIME * flat.length() / throw.max(1.0)).clamp(0.5, 1.1);
        let vel = Vec3::new(flat.x / t, flat.y / t, (target.z - from.z + 0.5 * gravity * t * t) / t);
        // Goes off just after it lands, like in the old arcade games.
        Self { pos: from, prev: from, vel, fuse: (t * 60.0) as u32 + 4, blast, weapon, team, by }
    }
}

/// A hut: ticks until the next man comes out, and how many have.
#[derive(Clone)]
pub struct Spawner {
    timer: u32,
    spawned: u32,
}

/// The nearest of the 8 ways on the ground to `dir` (zero for none).
pub(super) fn eight_way(dir: Vec3) -> Vec3 {
    if dir.truncate() == glam::Vec2::ZERO {
        return Vec3::ZERO;
    }
    let step = std::f32::consts::FRAC_PI_4;
    let a = (super::math::atan2(dir.y, dir.x) / step).round() * step;
    let (s, c) = super::math::sin_cos(a);
    // Exact on the axes, so a shot along one does not drift.
    let snap = |v: f32| if v.abs() < 1e-6 { 0.0 } else { v };
    Vec3::new(snap(c), snap(s), 0.0)
}

/// Where a shot from `from` goes that leaves along `line` (one of the 8 ways
/// on the ground), the same for the player and the enemies: at the nearest
/// target (centre, width) standing in that line, rising or dipping to its
/// height but keeping to the line; with `assist` (`[weapons] aim_assist`)
/// the nearest in a narrow cone around the line, straight at it. None when
/// no target is there.
pub(super) fn aim(from: Vec3, line: Vec3, reach: f32, assist: bool, targets: impl Iterator<Item = (Vec3, f32)>) -> Option<Vec3> {
    let flat = line.truncate().normalize_or_zero();
    targets
        .filter_map(|(centre, width)| {
            let to = centre - from;
            let ahead = to.truncate().dot(flat);
            let across = (to.truncate() - flat * ahead).length();
            let cone = width + 0.5 + if assist { ahead * AIM_CONE } else { 0.0 };
            (ahead > 1.0 && ahead < reach && across < cone).then_some((ahead, to))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(ahead, to)| if assist { to.normalize() } else { (flat * ahead).extend(to.z).normalize() })
}

/// A shot from `from` along `line` (one of the 8 ways) towards `target`,
/// which need not stand in it: rising or dipping to the target's height
/// where it comes level with it.
pub(super) fn along(from: Vec3, line: Vec3, target: Vec3) -> Vec3 {
    let to = target - from;
    let flat = line.truncate();
    (flat * to.truncate().dot(flat)).extend(to.z).normalize_or_zero()
}

fn rotate_z(v: Vec3, angle: f32) -> Vec3 {
    let (s, c) = super::math::sin_cos(angle);
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}

/// The nearest of `targets` (where each stands, and what to aim at) to
/// `at` on the ground; the first of them on a tie.
pub(super) fn nearest(targets: &[(Vec3, Vec3)], at: Vec3) -> Option<(Vec3, Vec3)> {
    let mut best: Option<(f32, (Vec3, Vec3))> = None;
    for &t in targets {
        let d = (t.0 - at).truncate().length();
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, t));
        }
    }
    best.map(|(_, t)| t)
}

/// Where a body placed by a marker stands: on the marker's cell, lifted out
/// of anything solid.
fn place(world: &crate::world::World, cell: IVec3, height: f32, half: f32) -> Vec3 {
    let mut pos = Vec3::new(cell.x as f32 + 0.5, cell.y as f32 + 0.5, cell.z as f32);
    while body_hits(world, pos, half, height) && pos.z < world.size.z as f32 + 8.0 {
        pos.z += 1.0;
    }
    pos
}

impl Shooter {
    /// Places what the level's markers stand for, and starts the fight over.
    pub(super) fn populate(&mut self, markers: &[Marker]) {
        // The seats' players go along, theirs again as their figures come
        // back (`Host::put_seat`).
        for t in self.things.iter() {
            if let Brain::Player(s) = &t.brain {
                self.departed.retain(|(seat, ..)| *seat != s.seat);
                self.departed.push((s.seat, (**s).clone(), t.hp));
            }
        }
        self.things.clear();
        self.grenades.clear();
        for m in markers {
            self.place_kind(m.kind, m.cell);
        }
        self.status = Status::start(&self.sim.cfg, &self.rules);
    }

    /// Places a thing of kind `kind` where a marker in `cell` would: walkers
    /// on the ground there, the others on the cell or in its middle, a
    /// gunship on top of what stands there.
    pub(super) fn place_kind(&mut self, kind: usize, cell: IVec3) -> crate::sim::things::Id {
        let half = self.sim.half();
        // Things that only stand where they are: on the marker's cell, or in
        // its middle.
        let at_cell = Body::new(cell.as_vec3(), 0.0);
        let rules = Arc::clone(&self.rules);
        let k = &rules.kinds[kind];
        match &k.rules {
            KindRules::Script => {
                let height = self.shapes.kind(k).height();
                let pos = place(&self.sim.world, cell, height, half);
                self.things.spawn(kind, Body::new(pos, height), k.hp, Brain::Script(Default::default()))
            }
            KindRules::Soldier(_) => {
                let height = self.shapes.kind(k).height();
                let pos = place(&self.sim.world, cell, height, half);
                self.things.spawn(kind, Body::new(pos, height), k.hp.max(1), Brain::Soldier(Soldier::new(false)))
            }
            KindRules::Spawner(r) => self.things.spawn(kind, at_cell, 0, Brain::Spawner(Spawner { timer: r.first, spawned: 0 })),
            KindRules::Captive(_) => {
                let height = self.shapes.kind(k).height();
                let pos = place(&self.sim.world, cell, height, half);
                self.things.spawn(kind, Body::new(pos, height), k.hp, Brain::Captive(Captive { freed: false, gone: 0 }))
            }
            KindRules::Pickup(_) => self.things.spawn(kind, at_cell, 0, Brain::Pickup),
            KindRules::Gunship(r) => {
                let top = self.sim.world.column_top(cell.x, cell.y) as f32;
                let at = Vec3::new(cell.x as f32 + 0.5, cell.y as f32 + 0.5, top);
                let rockets = &rules.weapons[r.rockets];
                self.things.spawn(kind, Body::new(at, 0.0), k.hp, Brain::Gunship(super::Gunship::new(top, rockets)))
            }
        }
    }

    /// One line of game state for the window title or the console: what
    /// the kit keeps (the rest is the game's).
    pub fn status_line(&self) -> Option<String> {
        let s = &self.status;
        let weapon = match s.weapon {
            Some(w) => match self.rules.rounds_of(w) {
                Some(item) => format!("{} {}", self.rules.weapon_names[w], s.stock.get(item).copied().unwrap_or(0)),
                None => self.rules.weapon_names[w].clone(),
            },
            None => "no gun".into(),
        };
        let stock: Vec<String> = s.stock.iter().map(|(item, n)| format!("{item} {n}")).collect();
        let line = format!("HP {}  {weapon}  [{}]  Score {}", s.hp, stock.join(", "), s.score);
        Some(match s.phase {
            Phase::Playing => line,
            Phase::Down => format!("DOWN  {line}"),
            Phase::Won => format!("MISSION COMPLETE  {line}"),
            Phase::GameOver => format!("GAME OVER (F5 restarts)  {line}"),
        })
    }

    /// How many enemies know the player is here.
    pub fn alerted_enemies(&self) -> usize {
        self.things.iter().filter(|t| matches!(&t.brain, Brain::Soldier(s) if s.alert)).count()
    }

    /// Wakes every enemy who hears a noise at `at` (each as far as his
    /// kind hears it).
    pub(super) fn alert(&mut self, at: Vec3, noise: Noise) {
        for t in self.things.iter_mut() {
            let Brain::Soldier(s) = &mut t.brain else { continue };
            let r = self.rules.soldier(t.kind);
            let radius = match noise {
                Noise::Shot => r.hearing,
                Noise::Impact => r.hearing * 0.5,
                Noise::Blast => r.blast_hearing,
                Noise::Fall => r.sight,
                Noise::Within(n) => r.hearing.min(n),
            };
            if t.body.pos.distance(at) < radius {
                s.alert = true;
            }
        }
    }

    /// The first of an action's weapons that can attack now: a blow when
    /// one stands close in front (and the one it hits), a gun or a throw
    /// when the player (`who`: the one, or a seat's) holds a round of it or
    /// it needs none.
    pub(super) fn first_that_can(&self, who: Who, choices: &[Choice]) -> Option<(usize, Option<Id>)> {
        let (held, stock) = self.arms(who)?;
        choices.iter().find_map(|choice| {
            let w = match *choice {
                Choice::Weapon(w) => w,
                Choice::Held => held?,
            };
            if self.rules.weapons[w].melee() {
                return self.in_reach(who, w).map(|id| (w, Some(id)));
            }
            let has = self.rules.rounds_of(w).is_none_or(|item| stock.get(item).is_some_and(|&n| n > 0));
            has.then_some((w, None))
        })
    }

    /// A player's gun in hand and its rounds and grenades: the one
    /// player's, or a seat's.
    pub(super) fn arms(&self, who: Who) -> Option<(Option<usize>, &std::collections::BTreeMap<String, u32>)> {
        match who {
            Who::Player => Some((self.status.weapon, &self.status.stock)),
            Who::Thing(id) => match &self.things.get(id)?.brain {
                Brain::Player(s) => Some((s.weapon, &s.stock)),
                _ => None,
            },
        }
    }

    /// A player's body: the one player's, or a seat's figure's.
    pub(super) fn body_of(&self, who: Who) -> Option<&Body> {
        match who {
            Who::Player => Some(&self.player.body),
            Who::Thing(id) => self.things.get(id).map(|t| &t.body),
        }
    }

    /// The first a blow of weapon `w` reaches and can hurt: close in front,
    /// within its cone, at the player's height.
    fn in_reach(&self, who: Who, w: usize) -> Option<Id> {
        let rules = &self.rules.weapons[w];
        let b = self.body_of(who)?;
        let (pos, facing, height) = (b.pos, b.facing, b.height);
        let ahead = math::cos((rules.cone * 0.5).to_radians());
        let team = Some(self.team_of(who));
        self.things
            .iter()
            .find(|t| {
                let to = t.body.pos - pos;
                let flat = to.truncate();
                self.hurt_by(t, team, true) && flat.length() < rules.reach && to.z.abs() < height * 0.6 && flat.normalize_or_zero().dot(facing.truncate()) > ahead
            })
            .map(|t| t.id)
    }

    /// How far a weapon's attack is heard.
    fn noise_of(&self, w: usize) -> Option<Noise> {
        match self.rules.weapons[w].noise {
            None => Some(Noise::Shot),
            Some(n) if n <= 0.0 => None,
            Some(n) => Some(Noise::Within(n)),
        }
    }

    /// The player (`who`: the one, or a seat's) attacks with weapon `w`: a
    /// blow at `target`, a shot (at `at`, else along the line he faces,
    /// found by the aim assist), a throw (to `at`, else its full throw
    /// ahead). Its rounds are spent; the ticks until it may attack again.
    pub(super) fn attack(&mut self, who: Who, w: usize, target: Option<Id>, at: Option<Vec3>) -> u32 {
        let rules = Arc::clone(&self.rules);
        let weapon = &rules.weapons[w];
        let name = rules.weapon_names[w].clone();
        if weapon.melee() {
            let Some(id) = target.filter(|&id| self.things.get(id).is_some()) else { return 0 };
            let hit = self.things.get(id).expect("a target there").body.centre();
            self.sim.sound(weapon.sound.0, hit, 1.0);
            self.sim.sparks(hit, 5);
            let noise = self.noise_of(w);
            self.damage_thing(id, weapon.damage, noise.is_none(), Some(who), Some(name));
            if let Some(noise) = noise {
                self.alert(hit, noise);
            }
            return weapon.cooldown;
        }
        if let Some(item) = rules.rounds_of(w) {
            let stock = match who {
                Who::Player => Some(&mut self.status.stock),
                Who::Thing(id) => match self.things.get_mut(id).map(|t| &mut t.brain) {
                    Some(Brain::Player(s)) => Some(&mut s.stock),
                    _ => None,
                },
            };
            if let Some(stock) = stock {
                let left = stock.entry(item.to_string()).or_default();
                *left = left.saturating_sub(1);
            }
        }
        let Some(b) = self.body_of(who).cloned() else { return 0 };
        let b = &b;
        // A seat's rounds are its own (whose kill it is).
        let from = match who {
            Who::Thing(id) => Some(id),
            Who::Player => None,
        };
        if weapon.thrown() {
            let (throw, blast, sound) = (weapon.throw, weapon.blast, weapon.sound.0);
            let from = b.pos + Vec3::Z * b.height * 0.8 + b.facing * (self.sim.half() + 0.5);
            let target = at.unwrap_or(b.pos + b.facing * throw);
            let team = Some(self.team_of(who));
            self.grenades.push(Grenade::lobbed(from, target, throw, blast, self.sim.cfg.physics.gravity, w, team, Some(who)));
            self.sim.sound(sound, from, 1.0);
            return weapon.cooldown;
        }
        let facing = b.facing;
        let (speed, damage, blast, spread, life, rocket) = (weapon.speed, weapon.damage, weapon.blast, weapon.spread, weapon.life, weapon.rocket);
        let half = self.sim.half();
        let muzzle = muzzle_at(b, self.cfg.player.muzzle, half);
        // At what the game named, or the nearest enemy or gunship in the
        // line of fire (with aim assist, in a narrow cone around it); else
        // the way the player faces. Without aim assist nothing strays off
        // the 8 ways, not even the machine gun's spray.
        let reach = speed * life;
        let assist = rules.aim_assist;
        let aimed = match at {
            Some(at) => (at - muzzle).normalize_or(facing),
            None => {
                // What the round would hurt: the soldiers, the gunships in
                // the air or parked, the game's own with hit points, the
                // players of another team.
                let player = Some(self.team_of(who));
                let rivals: Vec<super::Fighter> = self.fighters().into_iter().filter(|f| f.playing && kinds::hurts(player, Some(f.team))).collect();
                let targets = self
                    .things
                    .iter()
                    .filter(|t| is_soldier(t) && self.hurt_by(t, player, false))
                    .map(|t| (t.body.centre(), half))
                    .chain(self.gunships().filter(|(t, g)| g.state != super::HelicopterState::Falling && self.hurt_by(t, player, false)).map(|(t, _)| {
                        let heli = rules.gunship(t.kind).size;
                        (super::helicopter::centre(&t.body, heli), heli[0].max(heli[1]) * 0.5)
                    }))
                    .chain(self.things.iter().filter(|t| matches!(t.brain, Brain::Script(_) | Brain::Captive(_)) && self.hurt_by(t, player, false)).map(|t| (t.body.centre(), half)))
                    .chain(rivals.iter().map(|f| (f.body.centre(), half)));
                aim(muzzle, facing, reach, assist, targets).unwrap_or(facing)
            }
        };
        let dir = if assist { rotate_z(aimed, self.sim.rng.range(-spread, spread)) } else { aimed };
        self.sim.flash(muzzle, dir);
        self.sim.sound(weapon.sound.0, muzzle, 1.0);
        let team = Some(self.team_of(who));
        self.bullets.push(Bullet { pos: muzzle, prev: muzzle, origin: muzzle, vel: dir * speed, life, damage, blast, rocket, hostile: false, weapon: w, team, from });
        if let Some(noise) = self.noise_of(w) {
            self.alert(muzzle, noise);
        }
        weapon.cooldown
    }

    /// The attacks the game's code asked for this tick (`thing:attack`),
    /// each as the thing may: not while it waits after its last, the
    /// player only playing and holding rounds for it.
    pub(super) fn start_attacks(&mut self) {
        for (who, w, at) in std::mem::take(&mut self.queued) {
            // Seat 1 in a game of several: its figure.
            let who = match who {
                Who::Player if self.several() => match self.seated(1) {
                    Some(id) => Who::Thing(id),
                    None => continue,
                },
                who => who,
            };
            let seat = match who {
                Who::Thing(id) => match self.things.get(id).map(|t| &t.brain) {
                    Some(Brain::Player(s)) => Some(s.busy > 0 || s.down),
                    _ => None,
                },
                Who::Player => Some(self.player.busy > 0),
            };
            match (who, seat) {
                (_, Some(waits)) => {
                    if waits || self.status.phase != Phase::Playing {
                        continue;
                    }
                    let Some(body) = self.body_of(who).cloned() else { continue };
                    let target = if self.rules.weapons[w].melee() {
                        match at.and_then(|(_, id)| id).filter(|&id| self.reaches(w, who, &body, id)) {
                            Some(id) => Some(id),
                            None => match self.in_reach(who, w) {
                                Some(id) => Some(id),
                                None => continue,
                            },
                        }
                    } else {
                        let stock = self.arms(who).map(|(_, stock)| stock);
                        if self.rules.rounds_of(w).is_some_and(|item| stock.and_then(|s| s.get(item)).is_none_or(|&n| n == 0)) {
                            continue;
                        }
                        None
                    };
                    let busy = self.attack(who, w, target, at.map(|(p, _)| p));
                    match who {
                        Who::Player => self.player.busy = busy,
                        Who::Thing(id) => {
                            if let Some(Brain::Player(s)) = self.things.get_mut(id).map(|t| &mut t.brain) {
                                s.busy = busy;
                            }
                        }
                    }
                }
                (Who::Player, None) => {}
                (Who::Thing(id), None) => {
                    if self.busy.contains_key(&id) || self.things.get(id).is_none() {
                        continue;
                    }
                    let busy = self.attack_from(id, w, at.map(|(p, _)| p));
                    if busy > 0 {
                        self.busy.insert(id, busy);
                    }
                }
            }
        }
    }

    /// Whether a blow of weapon `w` by `who` from `from` reaches thing
    /// `id`: close in front, within its cone, at the attacker's height.
    fn reaches(&self, w: usize, who: Who, from: &Body, id: Id) -> bool {
        let rules = &self.rules.weapons[w];
        let team = Some(self.team_of(who));
        let Some(t) = self.things.get(id).filter(|t| self.hurt_by(t, team, true)) else { return false };
        let to = t.body.pos - from.pos;
        let flat = to.truncate();
        flat.length() < rules.reach && to.z.abs() < from.height * 0.6 && flat.normalize_or_zero().dot(from.facing.truncate()) > math::cos((rules.cone * 0.5).to_radians())
    }

    /// One of the things attacks with weapon `w` (the game's code): a blow
    /// at the player if he stands close in front, a shot at `at` or the
    /// way it faces, a throw to `at` or its full throw ahead. It carries no
    /// inventory. The ticks until it may attack again.
    fn attack_from(&mut self, id: Id, w: usize, at: Option<Vec3>) -> u32 {
        let rules = Arc::clone(&self.rules);
        let weapon = &rules.weapons[w];
        let Some((body, team)) = self.things.get(id).map(|t| (t.body.clone(), rules.kinds[t.kind].team)) else { return 0 };
        let facing = body.facing;
        if weapon.melee() {
            // The first player in reach, in seat order.
            let ahead = math::cos((weapon.cone * 0.5).to_radians());
            let struck = self.fighters().into_iter().find(|f| {
                let to = f.body.pos - body.pos;
                if !kinds::hurts(team, Some(f.team)) {
                    return false;
                }
                let flat = to.truncate();
                flat.length() < weapon.reach && to.z.abs() < body.height * 0.6 && flat.normalize_or_zero().dot(facing.truncate()) > ahead
            });
            if let Some(f) = struck {
                let hit = f.body.centre();
                self.sim.sound(weapon.sound.0, hit, 1.0);
                self.sim.sparks(hit, 5);
                self.hurt_fighter(f.who, weapon.damage, None, Some(rules.weapon_names[w].clone()));
            }
            return weapon.cooldown;
        }
        if weapon.thrown() {
            let from = body.pos + Vec3::Z * body.height * 0.8 + facing * (self.sim.half() + 0.5);
            let target = at.unwrap_or(body.pos + facing * weapon.throw);
            self.grenades.push(Grenade::lobbed(from, target, weapon.throw, weapon.blast, self.sim.cfg.physics.gravity, w, team, None));
            self.sim.sound(weapon.sound.0, from, 1.0);
            return weapon.cooldown;
        }
        let muzzle = muzzle_at(&body, None, self.sim.half());
        let dir = at.map_or(facing, |p| (p - muzzle).normalize_or(facing));
        self.sim.flash(muzzle, dir);
        self.sim.sound(weapon.sound.0, muzzle, 1.0);
        self.bullets.push(Bullet { pos: muzzle, prev: muzzle, origin: muzzle, vel: dir * weapon.speed, life: weapon.life, damage: weapon.damage, blast: weapon.blast, rocket: weapon.rocket, hostile: true, weapon: w, team, from: None });
        weapon.cooldown
    }

    /// An explosion of weapon `w` (a grenade, a rocket) set off by `team`
    /// (by a player: `by`, whose kills they are): the crater, the weapon's
    /// damage to those near, a noise every enemy around hears.
    pub(super) fn blast(&mut self, centre: Vec3, radius: f32, weapon: Option<usize>, team: Option<&'static str>, by: Option<Who>) {
        let damage = weapon.map_or(1, |w| self.rules.weapons[w].damage);
        self.blast_of(centre, radius, weapon, team, damage, by);
    }

    /// An explosion doing `damage` to everyone with hit points within its
    /// reach (the player too), unless they are immune to blasts or of the
    /// team that set it off while the weapon spares its own
    /// (`friendly_fire = false`); with `falloff` less towards the edge.
    pub(super) fn blast_of(&mut self, centre: Vec3, radius: f32, weapon: Option<usize>, team: Option<&'static str>, damage: i32, by: Option<Who>) {
        let rules = Arc::clone(&self.rules);
        let named = weapon.map(|w| rules.weapon_names[w].clone());
        let (spares, falloff) = weapon.map_or((false, false), |w| (!rules.weapons[w].friendly_fire, rules.weapons[w].falloff));
        self.sim.explode(centre, radius);
        let reach = radius + self.sim.half() + 0.5;
        let hit = |kind: usize| {
            let k = &rules.kinds[kind];
            !k.immune.blast && !(spares && !kinds::hurts(team, k.team))
        };
        let dealt = |dist: f32| if falloff { ((damage as f32 * (1.0 - dist / reach)).ceil() as i32).max(1) } else { damage };
        // The last first, as they came into the world.
        for id in self.things.ids(is_soldier).into_iter().rev() {
            let Some(dist) = self.things.get(id).filter(|t| hit(t.kind)).map(|t| t.body.distance_to(centre)) else { continue };
            if dist < reach {
                self.damage_enemy_by(id, dealt(dist), false, by, named.clone());
            }
        }
        for id in self.things.ids(is_captive).into_iter().rev() {
            let caged = |t: &super::Thing| matches!(&t.brain, Brain::Captive(c) if !c.freed);
            let Some(dist) = self.things.get(id).filter(|t| caged(t) && hit(t.kind)).map(|t| t.body.distance_to(centre)) else { continue };
            if dist < reach {
                self.hurt_captive(id, dealt(dist), by, named.clone());
            }
        }
        for id in self.things.ids(is_gunship) {
            if self.things.get(id).is_some_and(|t| hit(t.kind) && super::helicopter::contains(&t.body, centre, rules.gunship(t.kind).size, radius)) {
                self.damage_helicopter(id, damage, by, named.clone());
            }
        }
        // The game's own things with hit points: their hits told, their
        // deaths its code's.
        for id in self.things.ids(|t| matches!(t.brain, Brain::Script(_)) && t.hp > 0) {
            let Some(dist) = self.things.get(id).filter(|t| hit(t.kind)).map(|t| t.body.distance_to(centre)) else { continue };
            if dist < reach {
                self.hurt_script(id, dealt(dist), by, named.clone());
            }
        }
        for f in self.fighters() {
            let dist = f.body.distance_to(centre);
            let spared = spares && !kinds::hurts(team, Some(f.team));
            if dist < reach && !spared {
                // Another player's blast is theirs; one's own is nobody's.
                let by = by.filter(|&b| b != f.who && matches!(b, Who::Thing(_)));
                self.hurt_fighter(f.who, dealt(dist), by, named.clone());
            }
        }
        self.alert(centre, Noise::Blast);
    }

    /// A hostage hit: at no hit points left it dies, its cage too.
    pub(super) fn hurt_captive(&mut self, id: Id, damage: i32, by: Option<Who>, weapon: Option<String>) {
        let Some(t) = self.things.get_mut(id) else { return };
        t.hp -= damage;
        t.flash = 6;
        self.sim.events.push(Event::Hit { who: Who::Thing(id), by, damage, weapon: weapon.clone() });
        if self.things.get(id).is_some_and(|t| t.hp <= 0) {
            let h = self.things.remove(id).expect("a hostage in the list");
            self.sim.events.push(Event::Death { who: Who::Thing(id), by, kind: h.kind, body: h.body.clone(), weapon });
            self.sound(Sfx::Splat, h.body.centre(), 1.0);
            let shape = self.shapes.kind(&self.rules.kinds[h.kind]).clone();
            self.sim.gib(&shape.voxels, shape.size, h.body.pos, Vec3::ZERO);
        }
    }

    /// One of the game's own things hit: its hit points less and the hit
    /// told (what it means, a death too, is the game's code).
    pub(super) fn hurt_script(&mut self, id: Id, damage: i32, by: Option<Who>, weapon: Option<String>) {
        let Some(t) = self.things.get_mut(id) else { return };
        t.hp -= damage;
        t.flash = 6;
        self.sim.events.push(Event::Hit { who: Who::Thing(id), by, damage, weapon });
    }

    /// Any thing with hit points hit, by its rules: a soldier, a gunship, a
    /// caged hostage, one of the game's own.
    pub(super) fn damage_thing(&mut self, id: Id, damage: i32, quiet: bool, by: Option<Who>, weapon: Option<String>) {
        match self.things.get(id).map(|t| &t.brain) {
            Some(Brain::Soldier(_)) => self.damage_enemy_by(id, damage, quiet, by, weapon),
            Some(Brain::Gunship(_)) => self.damage_helicopter(id, damage, by, weapon),
            Some(Brain::Captive(c)) if !c.freed => self.hurt_captive(id, damage, by, weapon),
            Some(Brain::Script(_)) => self.hurt_script(id, damage, by, weapon),
            Some(Brain::Player(_)) => self.hurt_seated(id, damage, by, weapon),
            _ => {}
        }
    }

    /// Whether what `team` does with harm `round`/`melee` can hurt thing
    /// `t`: it has hit points, is of another team or none, is not immune,
    /// and is not a hostage already freed; a seat's player by its seat's
    /// team, while in the fight.
    pub(super) fn hurt_by(&self, t: &super::Thing, team: Option<&str>, melee: bool) -> bool {
        let k = &self.rules.kinds[t.kind];
        let immune = if melee { k.immune.melee } else { k.immune.round };
        let (alive, theirs) = match &t.brain {
            Brain::Soldier(_) | Brain::Gunship(_) | Brain::Script(_) => (t.hp > 0, k.team),
            Brain::Captive(c) => (!c.freed && t.hp > 0, k.team),
            Brain::Player(s) => (!s.down && t.hp > 0 && self.status.phase == super::Phase::Playing, Some(self.seat_team(s.seat))),
            _ => (false, k.team),
        };
        alive && !immune && kinds::hurts(team, theirs)
    }

    /// A hit on an enemy by the player's rounds (the tests' shorthand).
    #[cfg(test)]
    pub(super) fn damage_enemy(&mut self, id: Id, damage: i32, quiet: bool, weapon: Option<String>) {
        self.damage_enemy_by(id, damage, quiet, Some(Who::Player), weapon);
    }

    /// `quiet`: nobody hears the blow (a silent weapon), nor sees him fall.
    pub(super) fn damage_enemy_by(&mut self, id: Id, damage: i32, quiet: bool, by: Option<Who>, weapon: Option<String>) {
        let Some(t) = self.things.get_mut(id) else { return };
        t.hp -= damage;
        t.flash = 6;
        if let Brain::Soldier(s) = &mut t.brain {
            s.alert = true;
        }
        let dead = t.hp <= 0;
        self.sim.events.push(Event::Hit { who: Who::Thing(id), by, damage, weapon: weapon.clone() });
        if dead {
            self.kill_enemy(id, quiet, by, weapon);
        }
    }

    /// An enemy falls (`quiet`: a silent weapon's, his comrades see
    /// nothing).
    fn kill_enemy(&mut self, id: Id, quiet: bool, by: Option<Who>, weapon: Option<String>) {
        self.sim.freeze = self.sim.freeze.max(self.sim.cfg.effects.hitstop);
        let e = self.things.remove(id).expect("an enemy in the list");
        self.sim.events.push(Event::Death { who: Who::Thing(id), by, kind: e.kind, body: e.body.clone(), weapon });
        self.sound(Sfx::Splat, e.body.centre(), 1.0);
        // Comrades who could see him fall are on their guard; a silent
        // weapon is quiet.
        if !quiet {
            self.alert(e.body.pos, Noise::Fall);
        }
        let shape = self.shapes.kind(&self.rules.kinds[e.kind]).clone();
        // Away from who killed him (a seat's player), else the first.
        let killer = by.and_then(|who| self.body_of(who)).filter(|_| self.several()).map(|b| b.pos);
        let from = killer.or_else(|| self.fighters().first().map(|f| f.body.pos)).unwrap_or(e.body.pos);
        let push = (e.body.pos - from).truncate().normalize_or_zero().extend(0.0) * 15.0;
        self.sim.gib(&shape.voxels, shape.size, e.body.pos, push);
    }

    /// A player hit: the one player, or a seat's.
    pub(super) fn hurt_fighter(&mut self, who: Who, damage: i32, by: Option<Who>, weapon: Option<String>) {
        match who {
            Who::Player => self.hurt_player(damage, by, weapon),
            Who::Thing(id) => self.hurt_seated(id, damage, by, weapon),
        }
    }

    /// A seat's player hit, as the one player is (`hurt_player`): safe a
    /// while after, down at no hit points left.
    pub(super) fn hurt_seated(&mut self, id: Id, damage: i32, by: Option<Who>, weapon: Option<String>) {
        let (safe, on) = (self.cfg.player.invulnerable, self.status.phase == Phase::Playing);
        let Some(t) = self.things.get_mut(id) else { return };
        let Brain::Player(s) = &mut t.brain else { return };
        if !on || s.down || s.invulnerable > 0 {
            return;
        }
        t.hp -= damage;
        s.invulnerable = safe;
        let down = t.hp <= 0;
        if down {
            t.hp = 0;
            s.down = true;
        }
        let (at, kind, body) = (t.body.centre(), t.kind, t.body.clone());
        self.sim.events.push(Event::Hit { who: Who::Thing(id), by, damage, weapon: weapon.clone() });
        self.sim.shake = (self.sim.shake + 0.5).min(1.0);
        self.sound(if down { Sfx::Down } else { Sfx::Hurt }, at, 1.0);
        if down {
            self.sim.events.push(Event::Death { who: Who::Thing(id), by, kind, body: body.clone(), weapon });
            let shape = self.shapes.kind(&self.rules.kinds[kind]).clone();
            self.sim.gib(&shape.voxels, shape.size, body.pos, Vec3::ZERO);
        }
    }

    pub(super) fn hurt_player(&mut self, damage: i32, by: Option<Who>, weapon: Option<String>) {
        if self.status.phase != Phase::Playing || self.player.invulnerable > 0 {
            return;
        }
        self.status.hp -= damage;
        self.sim.events.push(Event::Hit { who: Who::Player, by, damage, weapon: weapon.clone() });
        self.player.invulnerable = self.cfg.player.invulnerable;
        self.sim.shake = (self.sim.shake + 0.5).min(1.0);
        let at = self.player.body.centre();
        self.sound(if self.status.hp <= 0 { Sfx::Down } else { Sfx::Hurt }, at, 1.0);
        if self.status.hp <= 0 {
            self.status.hp = 0;
            self.status.phase = Phase::Down;
            self.sim.events.push(Event::Death { who: Who::Player, by, kind: usize::MAX, body: self.player.body.clone(), weapon });
            let shape = self.shapes.player.clone();
            let at = self.player.body.pos;
            self.sim.gib(&shape.voxels, shape.size, at, Vec3::ZERO);
        }
    }

    /// The player stood up afresh at `at` (a respawn: where the game says),
    /// still, facing the way he did (the game's code turns him).
    pub(super) fn stand_afresh(&mut self, at: Vec3) {
        let height = self.player.body.height;
        let at = place(&self.sim.world, at.floor().as_ivec3(), height, self.sim.half());
        let (facing, look) = (self.player.body.facing, self.player.body.look);
        self.player.body = Body::new(at, height);
        (self.player.body.facing, self.player.body.look) = (facing, look);
    }

    /// The player down comes back to life (`player.state.hp` above 0): on
    /// his feet again.
    pub(super) fn back_to_life(&mut self) {
        if self.status.phase == Phase::Down {
            self.status.phase = Phase::Playing;
        }
    }

    /// The rounds and grenades in flight taken away (a team's only, if
    /// given): `level:clear_rounds`.
    pub(super) fn clear_rounds(&mut self, team: Option<&str>) {
        let gone = |of: Option<&'static str>| team.is_none_or(|t| of == Some(t));
        self.bullets.retain(|b| !gone(b.team));
        self.grenades.retain(|g| !gone(g.team));
    }

    pub(super) fn update_enemies(&mut self) {
        // Each after the nearest player in the fight; with none, the first
        // as he lies there (nobody awake to him).
        let fighters = self.fighters();
        let targets: Vec<(Vec3, Vec3)> = fighters.iter().filter(|f| f.playing).map(|f| (f.body.pos, f.body.centre())).collect();
        let resting = fighters.first().map_or((self.sim.cam_target, self.sim.cam_target), |f| (f.body.pos, f.body.centre()));
        let half = self.sim.half();
        let rules = Arc::clone(&self.rules);
        let physics = &self.sim.cfg.physics;
        let assist = rules.aim_assist;
        let mut shots = Vec::new();
        let mut throws = Vec::new();
        let mut fallen = Vec::new();
        let mut steps = Vec::new();
        for t in self.things.iter_mut().filter(|t| t.off.is_none()) {
            let Brain::Soldier(e) = &mut t.brain else { continue };
            let foes = rules.soldier(t.kind);
            let weapon = &rules.weapons[foes.weapon];
            let b = &mut t.body;
            b.prev = b.pos;
            let ((player_pos, target), awake) = match nearest(&targets, b.pos) {
                Some(n) => (n, true),
                None => (resting, false),
            };
            t.flash = t.flash.saturating_sub(1);
            let flat = (player_pos - b.pos).truncate().extend(0.0);
            let dist = flat.length();
            if dist > foes.active {
                continue;
            }
            if awake && dist < foes.sight {
                e.alert = true;
            }

            e.think = e.think.saturating_sub(1);
            if e.think == 0 {
                e.think = 30 + self.sim.rng.next_u32() % 60;
                e.wander = if e.alert && awake {
                    let dir = flat.normalize_or_zero();
                    let keep = foes.distance;
                    if dist > keep * 1.25 {
                        dir
                    } else if dist < keep * 0.7 {
                        -dir
                    } else if self.sim.rng.f32() < 0.35 {
                        Vec3::ZERO
                    } else {
                        let side = Vec3::new(-dir.y, dir.x, 0.0);
                        if self.sim.rng.f32() < 0.5 { side } else { -side }
                    }
                } else if self.sim.rng.f32() < 0.5 {
                    Vec3::ZERO
                } else {
                    let a = self.sim.rng.range(0.0, std::f32::consts::TAU);
                    Vec3::new(super::math::cos(a), super::math::sin(a), 0.0)
                };
            }
            // Like the player they face one of the 8 ways and fire along it;
            // a rifleman after the player steps aside onto a line with him
            // (with aim assist: near one), firing all the while.
            let line = eight_way(flat);
            let off_line = flat - line * flat.dot(line);
            let lined_up = off_line.length() < (half + 0.5 + if assist { dist * AIM_CONE } else { 0.0 }) * 0.8;
            let wish = if e.alert && awake && !weapon.thrown() && dist < foes.range && !lined_up {
                (e.wander + off_line.normalize_or_zero() * 1.5).normalize_or_zero()
            } else {
                e.wander
            };
            b.steer(wish, if e.alert { foes.speed } else { foes.idle_speed }, foes.accel);
            b.face(if e.alert && awake { line } else { e.wander });
            let walked = b.walked;
            b.advance(&self.sim.world, physics);
            steps.push((t.id, walked));
            // A fall from higher than they are tall (a tower coming down,
            // the ground blown away) kills them.
            if b.fell > b.height {
                fallen.push(t.id);
                continue;
            }

            if !(e.alert && awake && dist < foes.range) {
                continue;
            }
            e.cooldown = e.cooldown.saturating_sub(1);
            if e.cooldown > 0 {
                continue;
            }
            let jitter = if weapon.jitter > 0 { self.sim.rng.next_u32() % weapon.jitter } else { 0 };
            e.cooldown = weapon.cooldown + jitter;
            if weapon.thrown() {
                let from = b.pos + Vec3::Z * b.height * 0.8;
                let off = physics.body_width * 2.0;
                let miss = Vec3::new(self.sim.rng.range(-off, off), self.sim.rng.range(-off, off), 0.0);
                throws.push((from, player_pos + miss, foes.weapon, rules.kinds[t.kind].team));
            } else {
                // A nervous finger: he fires as soon as he can, along his
                // line towards the player even when the player is not
                // quite in it (the rounds are slow, he may run into them),
                // straight at him when he is.
                let from = muzzle_at(b, foes.muzzle, half);
                let dir = aim(from, line, foes.range * 2.0, assist, std::iter::once((target, half)))
                    .unwrap_or_else(|| along(from, line, target));
                let spread = if assist { weapon.spread } else { 0.0 };
                shots.push((from, rotate_z(dir, self.sim.rng.range(-spread, spread)), foes.weapon, rules.kinds[t.kind].team));
            }
        }
        for (id, walked) in steps {
            self.footsteps(Some(id), walked);
        }
        for id in fallen.into_iter().rev() {
            self.kill_enemy(id, false, None, None);
        }
        for (from, dir, weapon, team) in shots {
            let w = &rules.weapons[weapon];
            self.sim.flash(from, dir);
            self.sim.sound(w.sound.0, from, 1.0);
            self.bullets.push(Bullet {
                pos: from,
                prev: from,
                origin: from,
                vel: dir * w.speed,
                life: w.life,
                damage: w.damage,
                blast: w.blast,
                rocket: w.rocket,
                hostile: true,
                weapon,
                team,
                from: None,
            });
        }
        let gravity = self.sim.cfg.physics.gravity;
        for (from, to, weapon, team) in throws {
            let w = &rules.weapons[weapon];
            self.grenades.push(Grenade::lobbed(from, to, w.throw, w.blast, gravity, weapon, team, None));
            self.sim.sound(w.sound.0, from, 0.8);
        }
    }

    pub(super) fn update_hostages(&mut self) {
        let physics = &self.sim.cfg.physics;
        let players: Vec<(Vec3, Vec3)> = self.fighters().iter().map(|f| (f.body.pos, f.body.pos)).collect();
        for t in self.things.iter_mut().filter(|t| t.off.is_none()) {
            let Brain::Captive(h) = &mut t.brain else { continue };
            let r = self.rules.captive(t.kind);
            let body = &mut t.body;
            body.prev = body.pos;
            // Away from the nearest player.
            let player = nearest(&players, body.pos).map_or(body.pos, |(p, _)| p);
            if h.freed {
                // Run for it, the way its kind says.
                h.gone += 1;
                let away = || (body.pos - player).truncate().normalize_or(body.facing.truncate()).extend(0.0);
                let way = match &r.flee {
                    Flee::Way(way) => *way,
                    Flee::Away => away(),
                    Flee::Place(name) => match self.places.iter().find(|p| &p.name == name) {
                        Some(p) => {
                            let to = (p.at - body.pos).truncate();
                            if to.length() < 1.0 { Vec3::ZERO } else { to.normalize().extend(0.0) }
                        }
                        None => away(),
                    },
                };
                body.steer(way, r.speed, r.accel);
                body.face(way);
            }
            body.advance(&self.sim.world, physics);
        }
        self.things.retain(|t| !matches!(&t.brain, Brain::Captive(h) if h.gone >= 150));
    }

    /// A hostage freed (the game's code decides when): the cage bursts
    /// open around it and it runs off; the HUD counts it. False if it was
    /// free already or is no hostage.
    pub(super) fn free_captive(&mut self, id: crate::sim::things::Id) -> bool {
        let Some(t) = self.things.get_mut(id) else { return false };
        let Brain::Captive(h) = &mut t.brain else { return false };
        if h.freed {
            return false;
        }
        h.freed = true;
        let (at, height, reach) = (t.body.pos, t.body.height, self.rules.captive(t.kind).reach);
        let player = self.fighters().first().map_or(at, |f| f.body.pos);
        self.sound(Sfx::Rescue, player, 1.0);
        self.sim.break_out(at, reach * 0.6, height);
        true
    }

    pub(super) fn update_grenades(&mut self) {
        let world = &self.sim.world;
        let gravity = self.sim.cfg.physics.gravity;
        let mut blasts = Vec::new();
        self.grenades.retain_mut(|g| {
            g.prev = g.pos;
            g.vel.z -= gravity * DT;
            let next = g.pos + g.vel * DT;
            let mut pos = g.pos;
            for axis in [2, 0, 1] {
                let mut trial = pos;
                trial[axis] = next[axis];
                if world.blocks(trial.floor().as_ivec3()) {
                    g.vel[axis] *= -0.35;
                    if axis == 2 {
                        g.vel.x *= 0.6;
                        g.vel.y *= 0.6;
                    }
                } else {
                    pos = trial;
                }
            }
            g.pos = pos;
            g.fuse = g.fuse.saturating_sub(1);
            if g.fuse == 0 {
                blasts.push((g.pos, g.blast, g.weapon, g.team, g.by));
                return false;
            }
            true
        });
        for (at, radius, weapon, team, by) in blasts {
            self.blast(at, radius, Some(weapon), team, by);
        }
    }

    /// Reinforcements; the player down lies there (the game decides when it
    /// comes back, `on_death`).
    pub(super) fn update_level(&mut self) {
        // The players in the fight (none: nobody is sent for).
        let fighting: Vec<(Vec3, f32)> = self.fighters().iter().filter(|f| f.playing).map(|f| (f.body.pos, f.body.height)).collect();
        if fighting.is_empty() {
            return;
        }
        let rules = Arc::clone(&self.rules);

        // A hut sends its men while the player is near, not too near, and
        // (with `ahead`) while it still lies that way of him by more than
        // `ahead_by`: More Blood's huts north of him, so nobody comes out
        // behind his back. Of several players: near the nearest, too near
        // none and ahead of all.
        let half = self.sim.half();
        let mut arrivals = Vec::new();
        for t in self.things.iter_mut().filter(|t| t.off.is_none()) {
            let Brain::Spawner(s) = &mut t.brain else { continue };
            let r = rules.spawner(t.kind);
            let at = t.body.pos;
            let passed = r.ahead.is_some_and(|way| fighting.iter().any(|(pos, height)| at.truncate().dot(way) < pos.truncate().dot(way) + r.ahead_by.unwrap_or(*height)));
            let near = fighting.iter().map(|(pos, _)| pos.distance(at)).fold(f32::INFINITY, f32::min);
            let close = fighting.iter().any(|(pos, _)| pos.distance(at) < r.not_near);
            if s.spawned >= r.count || passed || near > r.range || close {
                continue;
            }
            s.timer = s.timer.saturating_sub(1);
            if s.timer == 0 {
                s.timer = r.every;
                s.spawned += 1;
                let height = self.shapes.kind(&rules.kinds[r.spawns]).height();
                arrivals.push((place(&self.sim.world, at.as_ivec3(), height, half), height, r.spawns));
            }
        }
        for (at, height, kind) in arrivals {
            let hp = rules.kinds[kind].hp.max(1);
            self.things.spawn(kind, Body::new(at, height), hp, Brain::Soldier(Soldier::new(true)));
        }

    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shooter::Controls;
    use crate::shooter::tests::{kit, set, set_weapons, shapes};
    use crate::sim::tests::flat_world;

    fn arena(markers: &[Marker]) -> Shooter {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        Shooter::new(flat_world(IVec3::new(128, 256, 24)), &cfg, shapes(), markers)
    }

    /// A marker for a thing of the kit's kind `kind` at (x, y), on the floor.
    fn marker(kind: &str, x: i32, y: i32) -> Marker {
        crate::shooter::tests::marker(&kit(), kind, IVec3::new(x, y, 1))
    }

    /// The knife kills an enemy right in front without a sound; the rifle
    /// wakes the next one up.
    #[test]
    fn the_knife_is_quiet_the_rifle_is_not() {
        // The far enemy is out of sight (68 away) but within earshot (66 from
        // the muzzle), and off the line of fire.
        let mut sim = arena(&[marker("rifleman", 64, 36), marker("rifleman", 100, 90)]);
        sim.player.body.face(Vec3::Y);
        sim.step(&Controls { fire: true, ..Controls::default() });
        assert_eq!(sim.enemies().count(), 1, "the close enemy should be knifed");
        assert!(sim.sim.events.iter().any(|e| matches!(e, Event::Death { weapon: Some(w), .. } if w == "knife")), "his death says the knife");
        assert!(!sim.enemy(0).1.alert, "the knife must not wake the far enemy");
        for _ in 0..20 {
            sim.step(&Controls { fire: true, ..Controls::default() });
        }
        assert!(sim.enemies().all(|(_, e)| e.alert), "gunfire should wake the far enemy");
        assert_eq!(sim.status.score, 0, "what a kill is worth is the game's code");
    }

    /// Shooting an enemy from afar, out of sight and earshot, still alarms
    /// the comrade next to it: it sees him fall. Missing alarms it too: the
    /// rounds hit the ground around it.
    #[test]
    fn a_comrade_falling_or_a_near_miss_alarms_them() {
        let mut sim = arena(&[marker("rifleman", 64, 140), marker("rifleman", 80, 146)]);
        sim.player.body.face(Vec3::Y);
        for _ in 0..40 {
            sim.step(&Controls { fire: true, ..Controls::default() });
            if sim.enemies().count() < 2 {
                break;
            }
        }
        assert_eq!(sim.enemies().count(), 1, "the enemy in the line of fire should fall");
        assert!(sim.enemy(0).1.alert, "his comrade did not notice");

        // A round into the ground next to an enemy far away.
        let mut sim = arena(&[marker("rifleman", 64, 150)]);
        let near = Vec3::new(70.5, 150.5, 0.5);
        sim.bullets.push(Bullet {
            pos: near + Vec3::Z,
            prev: near + Vec3::Z,
            origin: near + Vec3::Z * 4.0,
            vel: Vec3::NEG_Z * 50.0,
            life: 1.0,
            damage: 1,
            blast: 0.0,
            rocket: false,
            hostile: false,
            weapon: 0,
            team: Some(crate::shooter::PLAYER_TEAM),
            from: None,
        });
        sim.step(&Controls::default());
        assert!(sim.enemy(0).1.alert, "a round hitting the ground next to it went unnoticed");
    }

    /// A hut sends two men, one after another, while it lies ahead of the
    /// player, and then stays empty. Nobody comes out once he is level with
    /// it or past it, so no one shoots him in the back.
    #[test]
    fn a_hut_sends_a_few_men_and_none_behind_the_player() {
        // Counts who comes out over `ticks`; arrivals are taken away at once
        // so nobody fights.
        fn arrivals(sim: &mut Shooter, ticks: u32) -> usize {
            let mut count = 0;
            for _ in 0..ticks {
                sim.step(&Controls::default());
                count += sim.enemies().count();
                sim.things.retain(|t| !is_soldier(t));
            }
            count
        }

        const SPAWN_EVERY: u32 = 420;
        let mut sim = arena(&[marker("hut", 64, 100)]);
        assert_eq!(arrivals(&mut sim, 60), 1, "the first man comes out a second after the player is in range");
        assert_eq!(arrivals(&mut sim, SPAWN_EVERY - 1), 0, "the next one takes his time");
        assert_eq!(arrivals(&mut sim, 1), 1);
        assert_eq!(arrivals(&mut sim, SPAWN_EVERY * 5), 0, "the hut is empty");

        // A hut that sends only while north of the player (More Blood's):
        // level with it or past it, nobody.
        let north = |markers: &[Marker], change: fn(&mut crate::config::GameConfig)| {
            let mut cfg = kit();
            cfg.player.spawn = [64.0, 32.0];
            set(&mut cfg, "hut", "ahead", vec![0, 1]);
            change(&mut cfg);
            Shooter::new(flat_world(IVec3::new(128, 256, 24)), &cfg, shapes(), markers)
        };
        for y in [36, 20] {
            let mut sim = north(&[marker("hut", 64, y)], |_| {});
            assert_eq!(arrivals(&mut sim, SPAWN_EVERY * 3), 0, "a hut at y {y} sent a man next to or behind the player");
        }
        // Without `ahead` it sends on any side.
        let mut sim = arena(&[marker("hut", 64, 20)]);
        assert_eq!(arrivals(&mut sim, 60), 1, "a hut behind sends too");

        // Walking up to it after the first man stops the second.
        let mut sim = north(&[marker("hut", 64, 100)], |_| {});
        assert_eq!(arrivals(&mut sim, 60), 1);
        let near = Vec3::new(64.5, 95.5, 1.0);
        (sim.player.body.pos, sim.player.body.prev) = (near, near);
        assert_eq!(arrivals(&mut sim, SPAWN_EVERY * 2), 0);

        // Nobody comes out while the player is nearer than `not_near`.
        let mut sim = north(&[marker("hut", 64, 100)], |c| set(c, "hut", "not_near", 80));
        assert_eq!(arrivals(&mut sim, SPAWN_EVERY), 0);
    }

    /// A hostage freed (by the game's code) bursts its cage open from
    /// within and runs away from the player (or the way its kind says).
    #[test]
    fn a_freed_hostage_bursts_its_cage() {
        let mut world = flat_world(IVec3::new(128, 256, 24));
        let (cx, cy) = (64i32, 80i32);
        for y in cy - 4..=cy + 4 {
            for x in cx - 4..=cx + 4 {
                let edge = (x - cx).abs() == 4 || (y - cy).abs() == 4;
                if edge && (x + y) % 2 == 0 {
                    for z in 1..14 {
                        world.set_raw(IVec3::new(x, y, z), 1);
                    }
                }
            }
        }
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 50.0];
        let mut sim = Shooter::new(world, &cfg, shapes(), &[marker("hostage", cx, cy)]);
        let hostage = sim.things.ids(is_captive)[0];
        assert!(sim.free_captive(hostage));
        assert!(!sim.free_captive(hostage), "freed twice");
        // Only the corners may stand; the hostage gets out.
        let bars_left = (cy - 4..=cy + 4)
            .flat_map(|y| (cx - 4..=cx + 4).map(move |x| IVec3::new(x, y, 5)))
            .filter(|&c| sim.world.get(c) != 0)
            .count();
        assert!(bars_left <= 4, "{bars_left} bars still stand around the hostage");
        for _ in 0..30 {
            sim.step(&Controls::default());
        }
        assert!(sim.things.iter().filter(|t| is_captive(t)).all(|h| h.body.pos.y > (cy + 5) as f32), "the hostage did not get away");

        set(&mut cfg, "hostage", "flee", vec![1, 0]);
        let mut sim = Shooter::new(flat_world(IVec3::new(128, 256, 24)), &cfg, shapes(), &[marker("hostage", cx, cy)]);
        let hostage = sim.things.ids(is_captive)[0];
        assert!(sim.free_captive(hostage));
        for _ in 0..30 {
            sim.step(&Controls::default());
        }
        let h = sim.things.get(hostage).expect("still running");
        assert!(h.body.pos.x > (cx + 5) as f32 && (h.body.pos.y - cy as f32).abs() < 1.0, "east: {}", h.body.pos);
    }

    /// A hostage freed whose kind's `flee` names a place runs there and
    /// stays.
    #[test]
    fn a_freed_hostage_runs_to_its_place() {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 50.0];
        set(&mut cfg, "hostage", "flee", "exit");
        let mut sim = Shooter::new(flat_world(IVec3::new(128, 256, 24)), &cfg, shapes(), &[marker("hostage", 64, 80)]);
        sim.places = Arc::new([crate::levels::Place::new("exit", Vec3::new(20.5, 80.5, 1.0), None)]);
        let hostage = sim.things.ids(is_captive)[0];
        assert!(sim.free_captive(hostage));
        for _ in 0..120 {
            sim.step(&Controls::default());
        }
        let h = sim.things.get(hostage).expect("still there");
        assert!((h.body.pos.truncate() - glam::Vec2::new(20.5, 80.5)).length() < 2.0, "at {}", h.body.pos);
    }

    /// Walking up to a hostage frees nothing by itself: the game's code
    /// does (`thing.state.freed`).
    #[test]
    fn walking_up_to_a_hostage_frees_nothing_by_itself() {
        let mut sim = arena(&[marker("hostage", 64, 48)]);
        for _ in 0..40 {
            sim.step(&Controls { up: true, ..Controls::default() });
        }
        assert!(sim.things.iter().all(|t| !matches!(&t.brain, Brain::Captive(c) if c.freed)));
        assert_eq!(sim.status.score, 0);
    }

    /// Enemy fire hurts, but only once per grace period.
    #[test]
    fn hits_hurt_with_a_grace_period() {
        let mut sim = arena(&[marker("rifleman", 64, 70)]);
        let mut hp = Vec::new();
        for _ in 0..400 {
            sim.step(&Controls::default());
            hp.push(sim.status.hp);
        }
        let hits = hp.windows(2).filter(|w| w[1] < w[0]).count();
        assert!(hits >= 1, "the enemy should hit at some point");
        let first = hp.iter().position(|&h| h < 5).unwrap();
        let grace = sim.cfg.player.invulnerable as usize;
        assert!(grace > 0);
        assert!(hp[first..(first + grace - 1).min(hp.len())].iter().all(|&h| h == hp[first]), "no damage during the grace period");
    }

    /// Going down, the player lies there and its death is told; the kit
    /// takes no life and brings it back only when the game gives it hit
    /// points again (`player.state.hp`), where it was placed, facing the
    /// way it did. The rounds in flight stay unless the code clears them.
    #[test]
    fn going_down_waits_for_the_game() {
        use crate::lua::host::Host;
        let mut sim = arena(&[marker("rifleman", 64, 250)]);
        sim.player.body.face(Vec3::X);
        sim.hurt_player(5, None, None);
        assert_eq!(sim.status.phase, Phase::Down);
        assert!(sim.sim.events.iter().any(|e| matches!(e, Event::Death { who: Who::Player, .. })));
        for _ in 0..300 {
            sim.step(&Controls::default());
        }
        assert_eq!(sim.status.phase, Phase::Down, "lying there");
        sim.bullets.push(Bullet { pos: Vec3::splat(9.0), prev: Vec3::splat(9.0), origin: Vec3::splat(9.0), vel: Vec3::Y, life: 1.0, damage: 1, blast: 0.0, rocket: false, hostile: true, weapon: 0, team: Some("enemies"), from: None });
        Host::put_player(&mut sim, Vec3::new(64.5, 40.5, 1.0)).unwrap();
        Host::set_player_number(&mut sim, "hp", 5);
        assert_eq!((sim.status.phase, sim.status.hp), (Phase::Playing, 5));
        assert_eq!(sim.player.body.look, glam::IVec2::X, "no kit turns the player by itself");
        assert_eq!(sim.bullets.len(), 1);
        Host::clear_rounds(&mut sim, Some("player")).unwrap();
        assert_eq!(sim.bullets.len(), 1, "another team's stay");
        Host::clear_rounds(&mut sim, Some("enemies")).unwrap();
        assert!(sim.bullets.is_empty());
    }

    /// A grenade going off on the ground next to an enemy kills it: the blast
    /// reaches the nearest part of the body, not just its middle.
    #[test]
    fn a_grenade_at_their_feet_kills() {
        let mut sim = arena(&[marker("rifleman", 64, 120)]);
        let feet = sim.enemy(0).0.body.pos;
        let grenade = sim.rules.weapon_names.iter().position(|w| w == "grenade").unwrap();
        sim.blast(feet + Vec3::new(5.0, 0.0, 0.0), sim.rules.weapons[grenade].blast, Some(grenade), Some(crate::shooter::PLAYER_TEAM), Some(Who::Player));
        assert!(sim.enemies().next().is_none(), "the enemy survived a grenade at its feet");
    }

    /// A platform `z` up on a post off to the side, with an enemy on it who
    /// neither sees nor hears much.
    fn tower(post: IVec3) -> Shooter {
        let mut world = flat_world(IVec3::new(128, 256, 48));
        for z in 1..20 {
            for y in post.y - 1..=post.y + 1 {
                for x in post.x - 1..=post.x + 1 {
                    world.set_raw(IVec3::new(x, y, z), 1);
                }
            }
        }
        for y in 78..103 {
            for x in 50..79 {
                world.set_raw(IVec3::new(x, y, 20), 1);
            }
        }
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        set(&mut cfg, "rifleman", "sight", 1.0);
        set(&mut cfg, "rifleman", "hearing", 1.0);
        let guard = crate::shooter::tests::marker(&cfg, "rifleman", IVec3::new(64, 90, 21));
        Shooter::new(world, &cfg, shapes(), &[guard])
    }

    /// Shots aim up at an enemy on a tower instead of flying under it.
    #[test]
    fn shots_reach_an_enemy_up_on_a_tower() {
        let mut sim = tower(IVec3::new(74, 90, 0));
        sim.player.body.face(Vec3::Y);
        for _ in 0..30 {
            sim.step(&Controls { fire: true, ..Controls::default() });
        }
        assert!(sim.enemies().next().is_none(), "the enemy on the platform survived");
    }

    /// On the player's level too a shot finds an enemy a little off the
    /// line and heads straight for it; one well off the line it leaves
    /// alone and flies the way the player faces.
    #[test]
    fn shots_find_an_enemy_near_the_line() {
        let mut sim = arena(&[marker("rifleman", 72, 100)]);
        sim.player.body.face(Vec3::Y);
        sim.step(&Controls { fire: true, ..Controls::default() });
        let shot = sim.bullets.iter().find(|b| !b.hostile).expect("no shot");
        assert!(shot.vel.x > 0.0, "the shot did not turn towards the enemy");
        for _ in 0..30 {
            sim.step(&Controls { fire: true, ..Controls::default() });
        }
        assert!(sim.enemies().next().is_none(), "the enemy just off the line survived");

        let mut sim = arena(&[marker("rifleman", 110, 100)]);
        sim.player.body.face(Vec3::Y);
        sim.step(&Controls { fire: true, ..Controls::default() });
        let shot = sim.bullets.iter().find(|b| !b.hostile).expect("no shot");
        assert_eq!(shot.vel.truncate().normalize(), glam::Vec2::Y, "the shot bent towards an enemy well off the line");
    }

    /// Without aim assist every shot keeps to the 8 ways: the player's
    /// leaves an enemy just off his line alone and still reaches a guard up
    /// on a tower right in it; a rifleman off the lines fires along the
    /// nearest one at once, before he has stepped onto it.
    #[test]
    fn without_aim_assist_shots_keep_to_the_eight_ways() {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        set_weapons(&mut cfg, "aim_assist", false);
        let world = || flat_world(IVec3::new(128, 256, 24));
        let mut sim = Shooter::new(world(), &cfg, shapes(), &[marker("rifleman", 72, 100)]);
        sim.player.body.face(Vec3::Y);
        sim.step(&Controls { fire: true, ..Controls::default() });
        let shot = sim.bullets.iter().find(|b| !b.hostile).expect("no shot");
        assert_eq!(shot.vel.truncate().normalize(), glam::Vec2::Y, "the shot bent towards the enemy");

        let mut sim = tower(IVec3::new(74, 90, 0));
        std::sync::Arc::make_mut(&mut sim.rules).aim_assist = false;
        sim.player.body.face(Vec3::Y);
        for _ in 0..30 {
            sim.step(&Controls { fire: true, ..Controls::default() });
        }
        assert!(sim.enemies().next().is_none(), "the guard up in the line survived");

        // 18 degrees off the line south of him: he has to step aside first.
        let mut sim = Shooter::new(world(), &cfg, shapes(), &[marker("rifleman", 84, 92)]);
        sim.enemy_mut(0).alert = true;
        let mut directions = Vec::new();
        let mut first = None;
        for t in 0..400 {
            sim.player.invulnerable = 10;
            sim.step(&Controls::default());
            if sim.sounds.iter().any(|s| s.name == Sfx::EnemyShot.name()) {
                let shot = sim.bullets.iter().rev().find(|b| b.hostile).expect("a shot sound without a shot");
                directions.push(shot.vel.truncate().normalize());
                first.get_or_insert(t);
            }
        }
        assert!(first.is_some_and(|t| t < 60), "the rifleman held his fire (first shot at tick {first:?})");
        for d in directions {
            let eight = d.x.abs() < 1e-4 || d.y.abs() < 1e-4 || (d.x.abs() - d.y.abs()).abs() < 1e-4;
            assert!(eight, "an enemy shot flew {d}, off the 8 ways");
        }
    }

    /// A guard is like a rifleman (`like = "rifleman"`), and with `hp` of
    /// his own takes more hits; More Blood has one on every watchtower,
    /// taking four.
    #[test]
    fn guards_take_more_hits() {
        let mut cfg = kit();
        set(&mut cfg, "rifleman", "hp", 2);
        let world = || flat_world(IVec3::new(64, 64, 16));
        let sim = Shooter::new(world(), &cfg, shapes(), &[marker("guard", 20, 20)]);
        assert_eq!(sim.enemy(0).0.hp, 2, "a guard is like a rifleman");
        set(&mut cfg, "guard", "hp", 4);
        let sim = Shooter::new(world(), &cfg, shapes(), &[marker("guard", 20, 20)]);
        let (guard, _) = sim.enemy(0);
        assert_eq!((sim.rules.kinds[guard.kind].name.as_str(), guard.hp), ("guard", 4));

        let game = crate::session::Session::<crate::shooter::Shooter>::load(std::path::Path::new("games/moreblood")).unwrap();
        let map = crate::map::MapData::read(&std::fs::read_to_string("games/moreblood/map.toml").unwrap()).unwrap();
        let towers = map.put.iter().filter(|p| p.prefab.as_deref() == Some("tower")).count();
        let guards = game.game.enemies().filter(|(e, _)| e.hp == 4).count();
        assert!(towers > 0);
        assert_eq!(guards, towers, "{towers} watchtowers but {guards} guards taking four hits");
    }

    /// A hit that does not kill sprays blood (in a game with gore).
    #[test]
    fn a_hit_that_does_not_kill_sprays_blood() {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        cfg.debris.gore = 0.8;
        set(&mut cfg, "rifleman", "hp", 3);
        let mut sim = Shooter::new(flat_world(IVec3::new(128, 256, 24)), &cfg, shapes(), &[marker("rifleman", 64, 70)]);
        sim.player.body.face(Vec3::Y);
        let blood = |sim: &Shooter| sim.particles.iter().filter(|p| crate::sim::BLOOD_COLORS.contains(&p.color)).count();
        for _ in 0..40 {
            sim.step(&Controls { fire: true, ..Controls::default() });
            if sim.enemies().next().is_some_and(|(e, _)| e.hp < 3) {
                break;
            }
        }
        assert_eq!(sim.enemies().count(), 1, "the enemy should still stand");
        assert!(sim.enemy(0).0.hp < 3, "the enemy was never hit");
        assert!(blood(&sim) > 0, "the hit sprayed no blood");
    }

    /// A kind's `hp`: a tougher rifleman flashes at the first hit and goes
    /// down at the second.
    #[test]
    fn tougher_enemies_take_more_hits() {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        set(&mut cfg, "rifleman", "hp", 2);
        let mut sim = Shooter::new(flat_world(IVec3::new(128, 256, 24)), &cfg, shapes(), &[marker("rifleman", 64, 120)]);
        let id = sim.enemy(0).0.id;
        sim.damage_enemy(id, 1, false, None);
        assert_eq!(sim.enemies().count(), 1, "one hit was enough");
        assert!(sim.enemy(0).0.flash > 0, "the hit did not show");
        let id = sim.enemy(0).0.id;
        sim.damage_enemy(id, 1, false, None);
        assert!(sim.enemies().next().is_none(), "two hits were not enough");
    }

    /// Blowing away the post brings the platform down, and the fall kills
    /// the guard on it.
    #[test]
    fn a_guard_falls_with_his_tower() {
        let mut sim = tower(IVec3::new(64, 90, 0));
        sim.explode(Vec3::new(64.5, 90.5, 8.0), 3.0);
        for _ in 0..120 {
            sim.step(&Controls::default());
        }
        assert!(sim.enemies().next().is_none(), "the guard survived the fall");
    }

    /// Firing, a grenade and a kill each make their sound.
    #[test]
    fn the_fight_is_heard() {
        let mut sim = arena(&[marker("rifleman", 64, 75)]);
        sim.player.body.face(Vec3::Y);
        let mut heard = Vec::new();
        for t in 0..120 {
            sim.step(&Controls { fire: t < 3, grenade: t == 10, ..Controls::default() });
            heard.extend(sim.sounds.iter().filter_map(|e| Sfx::from_name(e.name)));
        }
        for sfx in [Sfx::Shot, Sfx::Throw, Sfx::Explosion, Sfx::Splat] {
            assert!(heard.contains(&sfx), "no {} in {heard:?}", sfx.name());
        }
    }
}

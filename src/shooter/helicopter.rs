//! The gunship: parked on its pad until the player comes near, then it
//! lifts off, holds off in front of the player, slides from side to side,
//! fires bursts and rockets. Shot down it spins in quarter turns, trails
//! smoke and crashes in a big blast (More Blood's code wins at the helipad
//! only once one is down).
//!
//! Its numbers come from its kind (`[things.NAME]`, behavior "gunship")
//! and its weapons (`[weapons.NAME]`).

use glam::{IVec2, IVec3, Vec3};

use std::sync::Arc;

use super::kinds::WeaponRules;
use super::{Brain, Bullet, Sfx, Shooter, is_gunship};
use crate::sim::things::Id;
use crate::sim::{Body, DT, Event, Particle, ParticleKind, Who, quarter_turn};

/// Ticks between rotor chops (the sound), and before its first burst.
const CHOP_EVERY: u64 = 5;
const FIRST_BURST: u32 = 60;
/// How long it takes to climb to its height, and to turn while it falls.
const RISE_TIME: f32 = 2.0;
const SPIN_EVERY: u64 = 7;
/// Ticks between quarter turns in flight: it turns round through its side.
const TURN_EVERY: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// On the ground, rotor still.
    Parked,
    Rising,
    Fighting,
    /// Shot down.
    Falling,
}

/// A gunship's mind. Its body is where the bottom of the model (its skids)
/// is, on the grid when drawn, and the quarter turn it shows (`look`,
/// forward is the model's +y); its health and flash are the thing's.
#[derive(Clone)]
pub struct Gunship {
    /// The height it took off from: it flies at `altitude` over this.
    home: f32,
    pub state: State,
    cooldown: u32,
    rounds: u32,
    rocket: u32,
    /// Which side it holds to, -1 .. 1, and ticks until it picks again.
    side: f32,
    think: u32,
    /// Ticks until it may turn another quarter.
    turn: u32,
    /// The side whose gun and whose rocket pod fire next, -1 or 1.
    gun: f32,
    pod: f32,
    /// Ticks since it lifted off, for its frames (only the picture).
    pub flown: u32,
}

impl Gunship {
    /// Parked at `home`'s height; its first rocket comes after half the
    /// pods' time.
    pub fn new(home: f32, rockets: &WeaponRules) -> Self {
        Self {
            home,
            state: State::Parked,
            cooldown: FIRST_BURST,
            rounds: 0,
            rocket: rockets.cooldown / 2,
            side: 1.0,
            think: 1,
            turn: 0,
            gun: 1.0,
            pod: 1.0,
            flown: 0,
        }
    }

    /// Whether the rotor turns (and is heard).
    pub fn flying(&self) -> bool {
        self.state != State::Parked
    }

    /// Turns its body a quarter towards `target` when it may: a half turn
    /// goes through a side, so it is seen to turn.
    fn turn_toward(&mut self, body: &mut Body, target: IVec2) {
        self.turn = self.turn.saturating_sub(1);
        if target == body.look || self.turn > 0 {
            return;
        }
        body.look = if target == -body.look { IVec2::new(-body.look.y, body.look.x) } else { target };
        self.turn = TURN_EVERY;
    }
}

/// Half a gunship's size across and along (turned with it) and its height.
fn extent(body: &Body, size: [f32; 3]) -> (f32, f32, f32) {
    let (across, along) = (size[0] * 0.5, size[1] * 0.5);
    if body.look.x != 0 { (along, across, size[2]) } else { (across, along, size[2]) }
}

/// Whether `p` lies inside a gunship's hit box, grown by `margin`.
pub fn contains(body: &Body, p: Vec3, size: [f32; 3], margin: f32) -> bool {
    let (hx, hy, h) = extent(body, size);
    (p.x - body.pos.x).abs() <= hx + margin
        && (p.y - body.pos.y).abs() <= hy + margin
        && p.z >= body.pos.z - margin
        && p.z <= body.pos.z + h + margin
}

pub fn centre(body: &Body, size: [f32; 3]) -> Vec3 {
    body.pos + Vec3::Z * size[2] * 0.5
}

/// Which way a gunship wants to face, `to` being the way to the player: at
/// him, or (side-on) across him, nose the way it flies or already points.
fn wanted(body: &Body, to: Vec3, side_on: bool) -> IVec2 {
    if !side_on {
        return quarter_turn(to.normalize_or_zero(), body.look);
    }
    let across = Vec3::new(-to.y, to.x, 0.0).normalize_or_zero();
    let along = across.dot(body.vel);
    let sign = if along.abs() > 2.0 { along.signum() } else { across.dot(Vec3::new(body.look.x as f32, body.look.y as f32, 0.0)).signum() };
    quarter_turn(across * sign, body.look)
}

/// A point given in a gunship's model (across to the right, along to the
/// front, up from the skids), turned with it; `side` -1 mirrors it.
fn point(body: &Body, at: [f32; 3], side: f32) -> Vec3 {
    let forward = Vec3::new(body.look.x as f32, body.look.y as f32, 0.0);
    let right = Vec3::new(forward.y, -forward.x, 0.0);
    body.pos + right * at[0] * side + forward * at[1] + Vec3::Z * at[2]
}

impl Shooter {
    /// The top of whatever stands under a gunship's hull (not the rotor's
    /// reach).
    fn top_under(&self, body: &Body, size: [f32; 3]) -> f32 {
        let (hx, hy, _) = extent(body, [size[0] * 0.5, size[1], size[2]]);
        let c = body.pos.floor().as_ivec3();
        let (rx, ry) = (hx as i32, hy as i32);
        let mut top = 0;
        for dy in (-ry..=ry).step_by(3) {
            for dx in (-rx..=rx).step_by(3) {
                top = top.max(self.sim.world.column_top(c.x + dx, c.y + dy));
            }
        }
        top as f32
    }

    pub(super) fn update_helicopters(&mut self) {
        let rules = Arc::clone(&self.rules);
        // Each after the nearest player in the fight; with none, the first
        // as he lies there (nobody awake to him).
        let fighters = self.fighters();
        let targets: Vec<(Vec3, Vec3)> = fighters.iter().filter(|f| f.playing).map(|f| (f.body.pos, f.body.centre())).collect();
        let resting = fighters.first().map_or(self.sim.cam_target, |f| f.body.centre());
        // The gunship holds at its offset from the player, inside the
        // world, sliding across the screen as the view sees it.
        let yaw = self.sim.cfg.camera.yaw.to_radians();
        let (sin, cos) = super::math::sin_cos(yaw);
        let right = Vec3::new(cos, -sin, 0.0);
        let mut shots = Vec::new();
        let mut rockets = Vec::new();
        let mut crashes = Vec::new();
        for id in self.things.ids(|t| is_gunship(t) && t.off.is_none()) {
            let kind = self.things.get(id).expect("a gunship in the list").kind;
            let cfg = rules.gunship(kind);
            let (gun, pods) = (&rules.weapons[cfg.weapon], &rules.weapons[cfg.rockets]);
            let margin = cfg.size[1] * 0.5;
            let far = self.sim.world.size.as_vec3() - margin;
            let top = self.top_under(&self.things.get(id).expect("a gunship in the list").body, cfg.size);
            let tick = self.sim.tick;
            let t = self.things.get_mut(id).expect("a gunship in the list");
            let Brain::Gunship(h) = &mut t.brain else { unreachable!() };
            if h.flying() {
                h.flown = h.flown.saturating_add(1);
            }
            let b = &mut t.body;
            b.prev = b.pos;
            t.flash = t.flash.saturating_sub(1);
            let (player, awake) = match super::combat::nearest(&targets, b.pos) {
                Some((_, centre)) => (centre, true),
                None => (resting, false),
            };
            let to_player = player - b.pos;
            let flat = Vec3::new(to_player.x, to_player.y, 0.0);
            let height = (h.home + cfg.altitude).max(top + cfg.clearance);
            match h.state {
                State::Parked => {
                    if awake && flat.length() < cfg.wake {
                        h.state = State::Rising;
                    }
                }
                State::Rising => {
                    b.vel = Vec3::Z * (cfg.altitude / RISE_TIME);
                    if b.pos.z >= height {
                        h.state = State::Fighting;
                    }
                    let want = wanted(b, flat, cfg.side_on);
                    h.turn_toward(b, want);
                }
                State::Fighting => {
                    // Hold off in front of the player, slide to a side now and then.
                    h.think = h.think.saturating_sub(1);
                    if h.think == 0 {
                        h.think = 120 + (self.sim.rng.next_u32() % 120);
                        h.side = self.sim.rng.range(-1.0, 1.0);
                    }
                    let offset = glam::Vec2::from(cfg.offset);
                    let slide = offset.length().max(cfg.size[1]) * 0.6;
                    let mut goal = player + offset.extend(0.0) + right * h.side * slide;
                    goal.x = goal.x.clamp(margin, far.x.max(margin));
                    goal.y = goal.y.clamp(margin, far.y.max(margin));
                    goal.z = height;
                    let wish = goal - b.pos;
                    let speed = (wish.length() * 2.0).min(cfg.speed);
                    let want = wish.normalize_or_zero() * speed;
                    let change = want - b.vel;
                    let accel = cfg.speed * 2.0 * DT;
                    b.vel += if change.length() > accel { change.normalize() * accel } else { change };
                    let want = wanted(b, flat, cfg.side_on);
                    h.turn_toward(b, want);

                    // Bursts from the chin gun, rockets now and then.
                    if awake {
                        h.cooldown = h.cooldown.saturating_sub(1);
                        h.rocket = h.rocket.saturating_sub(1);
                        if h.cooldown == 0 {
                            if h.rounds == 0 {
                                h.rounds = gun.burst.max(1);
                            }
                            // From the gun's muzzle: side-on the gun on the player's side,
                            // else both sides taking turns.
                            if cfg.side_on {
                                let forward = Vec3::new(b.look.x as f32, b.look.y as f32, 0.0);
                                h.gun = Vec3::new(forward.y, -forward.x, 0.0).dot(flat).signum();
                            }
                            shots.push((point(b, cfg.gun, h.gun), cfg.weapon, rules.kinds[kind].team, player));
                            if !cfg.side_on {
                                h.gun = -h.gun;
                            }
                            h.rounds -= 1;
                            h.cooldown = if h.rounds == 0 { gun.pause } else { gun.cooldown };
                        }
                        if h.rocket == 0 {
                            h.rocket = pods.cooldown;
                            // From the pods, one side after the other.
                            rockets.push((point(b, cfg.pods, h.pod), cfg.rockets, rules.kinds[kind].team, player));
                            h.pod = -h.pod;
                        }
                    }
                }
                State::Falling => {
                    b.vel.z -= self.sim.cfg.physics.gravity * 0.12 * DT;
                    b.vel.x *= 0.98;
                    b.vel.y *= 0.98;
                    if tick.is_multiple_of(SPIN_EVERY) {
                        b.look = IVec2::new(-b.look.y, b.look.x);
                    }
                    if b.pos.z <= top + 1.0 {
                        crashes.push(id);
                    }
                }
            }
            b.pos += b.vel * DT;
            // Never into what stands under it.
            if h.state == State::Fighting {
                b.pos.z = b.pos.z.max(top + 1.0);
            }
            if h.flying() && tick.is_multiple_of(CHOP_EVERY) {
                let at = b.pos;
                self.sim.sounds.push(super::SoundEvent { name: Sfx::Rotor.name(), at, volume: 0.8 });
            }
        }
        // Smoke and fire from the ones going down.
        let falling: Vec<Vec3> =
            self.gunships().filter(|(_, g)| g.state == State::Falling).map(|(t, _)| centre(&t.body, rules.gunship(t.kind).size)).collect();
        for at in falling {
            self.trail(at);
        }

        // Along one of its 8 ways, like everyone: straight at the player when
        // he is in that line (with aim assist: near it), else along the line
        // towards him.
        let assist = rules.aim_assist;
        let half = self.sim.half();
        let towards = |from: Vec3, player: Vec3| {
            let line = super::combat::eight_way(player - from);
            super::combat::aim(from, line, f32::INFINITY, assist, std::iter::once((player, half)))
                .unwrap_or_else(|| super::combat::along(from, line, player))
        };
        for (from, weapon, team, player) in shots {
            let w = &rules.weapons[weapon];
            let aim = towards(from, player);
            let scatter = if assist { w.spread } else { 0.0 };
            let spread = Vec3::new(self.sim.rng.range(-scatter, scatter), self.sim.rng.range(-scatter, scatter), 0.0);
            // The flash along the line, the round scattered off it.
            self.sim.flash(from, aim);
            self.sim.sound(w.sound.0, from, 1.0);
            self.bullets.push(Bullet::fired(w, weapon, from, (aim + spread).normalize_or_zero(), true, team, None));
        }
        for (from, weapon, team, player) in rockets {
            let w = &rules.weapons[weapon];
            let aim = towards(from, player);
            self.sim.sound(w.sound.0, from, 1.0);
            self.bullets.push(Bullet::fired(w, weapon, from, aim, true, team, None));
        }
        for id in crashes.into_iter().rev() {
            self.crash(id);
        }
    }

    /// A hit on a gunship: the flash, and down it goes when it is done.
    pub(super) fn damage_helicopter(&mut self, id: Id, damage: i32, by: Option<Who>, weapon: Option<String>) {
        let Some(t) = self.things.get_mut(id) else { return };
        let Brain::Gunship(h) = &mut t.brain else { return };
        if h.state == State::Falling {
            return;
        }
        t.hp -= damage;
        t.flash = 2;
        self.sim.events.push(Event::Hit { who: Who::Thing(id), by, damage, weapon: weapon.clone() });
        if h.state == State::Parked {
            h.state = State::Rising;
        }
        if t.hp <= 0 {
            self.sim.events.push(Event::Death { who: Who::Thing(id), by, kind: t.kind, body: t.body.clone(), weapon });
            h.state = State::Falling;
            t.body.vel.z = t.body.vel.z.min(0.0);
            let r = self.rules.gunship(t.kind);
            let at = centre(&t.body, r.size);
            self.sound(Sfx::Explosion, at, 0.7);
            self.sim.fireball(at, self.sim.cfg.physics.body_width * 1.5);
        }
    }

    /// Smoke and a few flames behind a falling gunship.
    fn trail(&mut self, at: Vec3) {
        let k = self.sim.effect_scale();
        for kind in [ParticleKind::Smoke, ParticleKind::Smoke, ParticleKind::Fire] {
            if self.sim.particles.len() >= self.sim.cfg.debris.max_particles {
                return;
            }
            let jitter = Vec3::new(self.sim.rng.range(-1.0, 1.0), self.sim.rng.range(-1.0, 1.0), self.sim.rng.range(0.0, 1.0));
            let life = if kind == ParticleKind::Fire { self.sim.rng.range(0.2, 0.4) } else { self.sim.rng.range(1.0, 2.0) };
            let color = [96, 92, 86, 255];
            self.sim.particles.push(Particle::new(at + jitter * 2.0, jitter * 6.0 * k, color, kind, life));
        }
    }

    /// The crash: a big blast where it hits the ground, and the wreck in pieces.
    fn crash(&mut self, id: Id) {
        let h = self.things.remove(id).expect("a gunship in the list");
        let r = self.rules.gunship(h.kind);
        let (at, crash_blast) = (centre(&h.body, r.size), r.crash_blast);
        self.blast_of(at, crash_blast, None, self.rules.kinds[h.kind].team, r.crash_damage, None);
        self.sim.shake = 1.0;
        let shapes = Arc::clone(&self.shapes);
        let shape = shapes.kind(&self.rules.kinds[h.kind]);
        self.wreck(&shape.voxels, shape.size, h.body.pos, h.body.look);
    }

    /// Throws a model's voxels about as pieces that lie and fade (no blood).
    fn wreck(&mut self, voxels: &[(IVec3, [u8; 4])], size: IVec3, at: Vec3, look: IVec2) {
        let k = self.sim.effect_scale();
        let pivot = IVec3::new(size.x / 2, size.y / 2, 0);
        for &(v, color) in voxels.iter().step_by(2) {
            if self.sim.particles.len() >= self.sim.cfg.debris.max_particles {
                break;
            }
            let l = v - pivot;
            let turned = match (look.x, look.y) {
                (0, 1) => l,
                (1, 0) => IVec3::new(l.y, -l.x, l.z),
                (0, -1) => IVec3::new(-l.x, -l.y, l.z),
                _ => IVec3::new(-l.y, l.x, l.z),
            };
            let pos = at + turned.as_vec3() + 0.5;
            let out = Vec3::new(turned.x as f32, turned.y as f32, 0.0).normalize_or_zero();
            let vel = (out * self.sim.rng.range(10.0, 40.0) + Vec3::Z * self.sim.rng.range(15.0, 50.0)) * k;
            let life = self.sim.rng.range(2.0, 4.0);
            self.sim.particles.push(Particle::new(pos, vel, color, ParticleKind::Gib, life));
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::{IVec3, Vec3};

    use super::State;
    use crate::shooter::tests::shapes;
    use crate::shooter::tests::{kit, set};
    use crate::shooter::{Controls, Phase, Sfx, Shooter};
    use crate::sim::Marker;
    use crate::sim::tests::flat_world;

    /// Open ground with room overhead; the player can't be hurt, so the
    /// gunship's fire doesn't end the test.
    fn sky(markers: &[Marker]) -> Shooter {
        sky_with(markers, |_| {})
    }

    fn sky_with(markers: &[Marker], change: impl FnOnce(&mut crate::config::GameConfig)) -> Shooter {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        change(&mut cfg);
        let mut sim = Shooter::new(flat_world(IVec3::new(128, 256, 48)), &cfg, shapes(), markers);
        sim.player.invulnerable = u32::MAX / 2;
        sim
    }

    fn at(kind: &str, x: i32, y: i32) -> Marker {
        crate::shooter::tests::marker(&kit(), kind, IVec3::new(x, y, 1))
    }

    /// Parked out of reach it waits; once the player comes near it lifts off
    /// with its rotor chopping and holds at its height up the screen.
    #[test]
    fn the_gunship_waits_then_rises_and_holds_in_front() {
        let mut sim = sky(&[at("gunship", 64, 200)]);
        for _ in 0..30 {
            sim.step(&Controls::default());
        }
        assert_eq!(sim.gunship(0).1.state, State::Parked);
        assert!(!sim.gunships().any(|(_, g)| g.flying()));

        let mut chops = 0;
        for _ in 0..60 {
            sim.step(&Controls { up: true, ..Controls::default() });
            chops += sim.sounds.iter().filter(|e| e.name == Sfx::Rotor.name()).count();
        }
        assert!(sim.gunships().any(|(_, g)| g.flying()), "walking up did not wake it");

        for _ in 0..420 {
            sim.step(&Controls::default());
            chops += sim.sounds.iter().filter(|e| e.name == Sfx::Rotor.name()).count();
        }
        assert!(chops > 60, "the rotor is not heard ({chops} chops)");
        let (t, h) = sim.gunship(0);
        let cfg = sim.rules.gunship(t.kind);
        assert_eq!(h.state, State::Fighting);
        assert!((t.body.pos.z - (1.0 + cfg.altitude)).abs() < 1.0, "flies at {} instead of its height", t.body.pos.z);
        let ahead = t.body.pos.y - sim.player.body.pos.y;
        assert!((ahead - cfg.offset[1]).abs() < 10.0, "holds {ahead} north of him, not {}", cfg.offset[1]);
    }

    /// Rifle fire reaches it in the air; shot down it falls, crashes in a
    /// blast and is worth its score.
    #[test]
    fn shooting_the_gunship_down_crashes_it() {
        let mut sim = sky(&[at("gunship", 64, 120)]);
        sim.player.body.face(Vec3::Y);
        let (mut fell, mut blast) = (false, false);
        for _ in 0..1500 {
            sim.step(&Controls { fire: true, strafe: true, ..Controls::default() });
            fell |= sim.gunships().next().is_some_and(|(_, h)| h.state == State::Falling);
            blast |= fell && sim.sounds.iter().any(|e| e.name == Sfx::Explosion.name());
            if sim.gunships().next().is_none() {
                break;
            }
        }
        assert!(fell, "it never went down");
        assert!(sim.gunships().next().is_none(), "it never crashed");
        assert!(blast, "it went down without a bang");
        assert_eq!(sim.status.score, 0, "what it is worth is the game's code");
    }

    /// A side-on gunship (door gunners) shows the player its side, and its
    /// rounds leave from the gun on his side.
    #[test]
    fn a_side_on_gunship_turns_its_door_to_the_player() {
        let mut cfg = kit();
        cfg.player.spawn = [64.0, 32.0];
        set(&mut cfg, "gunship", "side_on", true);
        set(&mut cfg, "gunship", "gun", toml::Value::Array(vec![6.0.into(), 1.0.into(), 4.0.into()]));
        let mut sim = Shooter::new(flat_world(IVec3::new(128, 256, 48)), &cfg, shapes(), &[at("gunship", 64, 120)]);
        sim.player.invulnerable = u32::MAX / 2;
        let mut rounds = 0;
        for _ in 0..600 {
            sim.step(&Controls::default());
            let (t, h) = sim.gunship(0);
            if h.state != State::Fighting || t.body.look.y != 0 {
                continue;
            }
            // The player is south of it: a side-on gunship looks east or west.
            // Rounds fired this tick (2.5 s to live, one tick of it gone).
            for b in sim.bullets.iter().filter(|b| b.hostile && !b.rocket && b.life > 2.47) {
                assert!(b.origin.y < t.body.pos.y - 3.0, "a round from the far door: {} vs {}", b.origin.y, t.body.pos.y);
                rounds += 1;
            }
        }
        let (t, _) = sim.gunship(0);
        assert_eq!(t.body.look.y, 0, "it faces the player instead of showing its side");
        assert!(rounds > 3, "only {rounds} rounds side-on");
    }

    /// The kit has no goal: a gunship down or a goal reached ends nothing
    /// by itself (the game decides, More Blood in its `main.lua`).
    #[test]
    fn nothing_wins_by_itself() {
        let mut sim = sky(&[at("gunship", 64, 200), at("goal", 64, 36)]);
        let id = sim.gunship(0).0.id;
        sim.damage_helicopter(id, 1000, None, None);
        for _ in 0..60 {
            sim.step(&Controls::default());
        }
        assert_eq!(sim.status.phase, Phase::Playing, "the game decides");
    }
}

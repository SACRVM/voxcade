//! Digger: the smallest game on Voxcade, written in Rust against the
//! engine's API alone. A miner made of cubes runs over hills made in code,
//! jumps, and blasts a hole in front of him: the engine's explosions,
//! debris, falling pieces and sounds do the rest.
//!
//! `cargo run --release --example rust_digger` opens it in a window; with
//! arguments it is the whole `voxcade` command line for this kind of game
//! (`-- shot examples/rust_digger ...`, `-- check examples/rust_digger`).
//! Its settings are code too (`Game::conf`); the Lua door's Digger is
//! `games/digger`.

use std::path::Path;

use anyhow::Result;
use glam::{IVec2, IVec3, Vec3};
use voxcade::audio::SoundDef;
use voxcade::config::{GameConfig, ShadowStyle};
use voxcade::controls::Bindings;
use voxcade::files::Files;
use voxcade::game::{Game, Setup};
use voxcade::hash::{Stable, mix};
use voxcade::input::Input;
use voxcade::render::Light;
use voxcade::session::Drawing;
use voxcade::sim::{Body, Sim, math, spawn_point};
use voxcade::world::World;

#[cfg_attr(test, allow(dead_code))]
fn main() -> Result<()> {
    voxcade::cli::main::<Digger>(Some(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/rust_digger"))))
}

/// The radius of the hole a blast leaves (voxels), and the ticks between two.
const BLAST: f32 = 6.0;
const RELOAD: u32 = 20;

#[derive(Clone)]
struct Digger {
    sim: Sim,
    body: Body,
    jump_held: bool,
    cooldown: u32,
    holes: u32,
}

/// The palette: grass, earth, stone, bedrock.
const GRASS: u8 = 1;
const EARTH: u8 = 3;
const STONE: u8 = 4;
const BEDROCK: u8 = 5;

/// The miner, 3 voxels wide and 8 high, facing +y: boots, trousers, shirt,
/// a pack on his back, face, helmet; his lamp glows apart.
const FIGURE: [(i32, i32, i32, [u8; 4]); 18] = {
    const BOOT: [u8; 4] = [60, 40, 30, 255];
    const LEGS: [u8; 4] = [50, 70, 130, 255];
    const SHIRT: [u8; 4] = [210, 100, 40, 255];
    const PACK: [u8; 4] = [96, 72, 44, 255];
    const SKIN: [u8; 4] = [230, 180, 140, 255];
    const HELMET: [u8; 4] = [240, 200, 40, 255];
    [
        (-1, 0, 0, BOOT), (1, 0, 0, BOOT), (-1, 1, 0, BOOT), (1, 1, 0, BOOT),
        (-1, 0, 1, LEGS), (1, 0, 1, LEGS), (0, 0, 2, LEGS),
        (-1, 0, 3, SHIRT), (0, 0, 3, SHIRT), (1, 0, 3, SHIRT), (0, 0, 4, SHIRT), (0, -1, 3, PACK), (0, -1, 4, PACK),
        (0, 0, 5, SKIN), (-1, 0, 6, HELMET), (0, 0, 6, HELMET), (1, 0, 6, HELMET), (0, 0, 7, HELMET),
    ]
};
const LAMP: IVec3 = IVec3::new(0, 1, 6);

impl Game for Digger {
    type Snapshot = Digger;

    /// How it plays and looks, over the engine's defaults.
    fn conf(_files: &Files, c: &mut GameConfig) -> Result<()> {
        c.title = "Digger".into();
        c.player.spawn = [96.0, 96.0];
        c.player.speed = 40.0;
        c.player.jump = 8.0;
        c.camera.pitch = 40.0;
        c.camera.view_width = 110.0;
        c.light.sun = [0.45, 0.6, -1.0];
        c.light.shadow_style = ShadowStyle::Voxel;
        c.light.sky = [0.42, 0.6, 0.8];
        Ok(())
    }

    fn actions(_cfg: &GameConfig) -> Vec<&'static str> {
        vec!["jump", "fire"]
    }

    fn default_keys(_cfg: &GameConfig) -> Bindings {
        Bindings::new(&[("jump", &["Space", "Z"]), ("fire", &["X", "J", "Left Ctrl"])])
    }

    fn default_pad(_cfg: &GameConfig) -> Bindings {
        Bindings::new(&[("jump", &["south"]), ("fire", &["west", "righttrigger"])])
    }

    /// The engine's explosion and crumble; its jump is a file of its own,
    /// `sounds/jump.toml`, as any game's.
    fn sounds(_cfg: &GameConfig) -> Vec<(&'static str, SoundDef)> {
        voxcade::audio::core_sounds().into_iter().filter(|(name, _)| *name != "impact").collect()
    }

    /// Rolling hills of earth under grass, stone deeper down, on bedrock:
    /// a few waves laid over each other, the same on every machine
    /// (`sim::math`).
    fn level(_cfg: &GameConfig) -> Result<Option<World>> {
        let size = IVec3::new(192, 192, 40);
        let mut palette = [[0; 4]; 256];
        palette[GRASS as usize] = [90, 150, 60, 255];
        palette[GRASS as usize + 1] = [76, 136, 52, 255];
        palette[EARTH as usize] = [130, 90, 56, 255];
        palette[STONE as usize] = [120, 118, 112, 255];
        palette[BEDROCK as usize] = [52, 48, 46, 255];
        let mut world = World::new(size, palette);
        for y in 0..size.y {
            for x in 0..size.x {
                let (fx, fy) = (x as f32, y as f32);
                let wave = math::sin(fx * 0.05) * 5.0 + math::cos(fy * 0.07) * 4.0 + math::sin((fx + fy) * 0.023) * 6.0;
                let top = (20.0 + wave) as i32;
                for z in 0..top {
                    let v = match top - z {
                        _ if z == 0 => BEDROCK,
                        1 => GRASS + (mix((x * 7919 + y) as u64) & 1) as u8,
                        2..=5 => EARTH,
                        _ => STONE,
                    };
                    world.set_raw(IVec3::new(x, y, z), v);
                }
            }
        }
        Ok(Some(world))
    }

    fn load(setup: Setup) -> Result<Self> {
        let cfg = setup.cfg;
        let at = spawn_point(&setup.world, cfg.player.spawn, 8.0, cfg.physics.body_width * 0.5);
        let sim = Sim::new(setup.world, cfg, at);
        Ok(Self { sim, body: Body::new(at, 8.0), jump_held: false, cooldown: 0, holes: 0 })
    }

    fn snapshot(&self) -> Result<Digger> {
        Ok(self.clone())
    }

    fn restore(&mut self, kept: &Digger) {
        *self = kept.clone();
    }

    fn sim(&self) -> &Sim {
        &self.sim
    }

    fn sim_mut(&mut self) -> &mut Sim {
        &mut self.sim
    }

    fn step(&mut self, input: &Input) {
        if !self.sim.begin_tick() {
            self.body.prev = self.body.pos;
            return;
        }
        let (moves, physics) = (&self.sim.cfg.player, &self.sim.cfg.physics);
        let b = &mut self.body;
        b.prev = b.pos;
        let x = input.right as i32 - input.left as i32;
        let y = input.up as i32 - input.down as i32;
        let wish = Vec3::new(x as f32, y as f32, 0.0).normalize_or_zero();
        b.face(wish);
        b.steer(wish, moves.speed, if b.on_ground { moves.accel } else { moves.air_accel });
        // A jump reaches `jump` voxels high; letting go early cuts it short.
        let jump = input.held(0);
        let takes_off = jump && !self.jump_held && b.on_ground;
        if takes_off {
            b.vel.z = (2.0 * physics.gravity * moves.jump).sqrt();
        }
        if !jump && b.vel.z > 0.0 {
            b.vel.z *= 0.6;
        }
        self.jump_held = jump;
        b.advance(&self.sim.world, physics);
        if takes_off {
            self.sim.sound("jump", self.body.pos, 1.0);
        }

        self.cooldown = self.cooldown.saturating_sub(1);
        if input.held(1) && self.cooldown == 0 {
            let reach = self.sim.half() + BLAST;
            let centre = self.body.pos + self.body.facing * reach + Vec3::Z;
            self.sim.explode(centre, BLAST);
            self.cooldown = RELOAD;
            self.holes += 1;
        }
        let half = Vec3::new(self.sim.half(), self.sim.half(), 0.0);
        let p = self.body.pos;
        self.sim.update_particles((p - half, p + half + Vec3::Z * self.body.height));
        self.sim.update_flares();
        self.sim.follow(self.body.pos);
    }

    fn state_hash(&self) -> u64 {
        let (s, b) = (&self.sim, &self.body);
        let mut h = Stable::default();
        h.u64(s.tick).u64(s.rng.state()).u64(s.world.changes()).u64(s.freeze as u64);
        h.vec3(b.pos).vec3(b.vel).i32(b.look.x).i32(b.look.y).u64(b.on_ground as u64);
        h.u64(self.jump_held as u64).u64(self.cooldown as u64).u64(self.holes as u64);
        h.u64(s.particles.len() as u64);
        h.finish()
    }

    /// The miner, turned in quarter turns the way he looks, his lamp
    /// glowing and lighting the ground in front of him.
    fn draw(&self, out: &mut Drawing) {
        if out.title() {
            return;
        }
        let feet = out.body_origin(&self.body);
        let turn = |v: IVec3| match self.body.look {
            IVec2 { x: 0, y: 1 } => v,
            IVec2 { x: 1, y: 0 } => IVec3::new(v.y, -v.x, v.z),
            IVec2 { x: 0, y: -1 } => IVec3::new(-v.x, -v.y, v.z),
            _ => IVec3::new(-v.y, v.x, v.z),
        };
        let lamp = feet + turn(LAMP).as_vec3();
        out.cube(lamp, [255, 250, 200, 255], 1.0, 1.0);
        for (x, y, z, color) in FIGURE {
            out.cube(feet + turn(IVec3::new(x, y, z)).as_vec3(), color, 0.0, 1.0);
        }
        let ahead = Vec3::new(self.body.look.x as f32, self.body.look.y as f32, 0.0);
        out.light(Light { pos: lamp + ahead * 4.0, radius: 14.0, color: [1.0, 0.9, 0.6] });
    }

    fn focus(&self) -> Option<&Body> {
        Some(&self.body)
    }

    fn summary(&self) -> Option<String> {
        Some(format!("holes {}", self.holes))
    }

    fn report(&self) -> Vec<String> {
        let p = self.body.pos;
        vec![format!("digger: at {:.1},{:.1},{:.1} holes {}", p.x, p.y, p.z, self.holes)]
    }

    fn place_player(&mut self, x: f32, y: f32) {
        let at = spawn_point(&self.sim.world, [x, y], self.body.height, self.sim.half());
        self.body = Body::new(at, self.body.height);
        self.sim.cam_target = at;
        self.sim.cam_prev = at;
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// Its replays run as recorded (`replays/tests/`, made with `shot
    /// --record`): the engine under it plays the same.
    #[test]
    fn the_replays_play_as_recorded() {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/rust_digger"));
        assert!(voxcade::replay::check::<Digger>(dir, false).unwrap());
    }
}

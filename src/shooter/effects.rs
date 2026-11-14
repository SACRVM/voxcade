//! The shooter's own effects in the core's particles: blood from a hit,
//! a muzzle flash, a cage burst open.

use glam::{IVec3, Vec3};

use crate::sim::{BLOOD_COLORS, DT, Flare, Particle, ParticleKind, Sim};

/// A muzzle flash from its core outwards.
const FLASH_COLORS: [[u8; 4]; 3] = [[255, 255, 230, 255], [255, 220, 110, 255], [255, 150, 50, 255]];

/// Blood from a hit that does not kill: out of the far side along the
/// shot and fanning wide and up, so it shows round the body, falling to
/// lie on the ground. Only in a game with gore (`[debris] gore`).
pub(super) fn blood_spray(sim: &mut Sim, at: Vec3, along: Vec3) {
    if sim.cfg.debris.gore <= 0.0 {
        return;
    }
    let k = sim.effect_scale();
    let along = along.truncate().normalize_or_zero().extend(0.0);
    for _ in 0..22 {
        if sim.particles_full() {
            break;
        }
        let fan = Vec3::new(sim.rng.range(-1.0, 1.0), sim.rng.range(-1.0, 1.0), sim.rng.range(0.4, 1.4));
        let dir = (along * 0.6 + fan).normalize_or_zero();
        let vel = dir * sim.rng.range(16.0, 44.0) * k;
        let color = BLOOD_COLORS[(sim.rng.next_u32() % 4) as usize];
        let life = sim.rng.range(1.2, 2.4);
        sim.particles.push(Particle::new(at, vel, color, ParticleKind::Gib, life));
    }
}

/// A muzzle flash: a bright core at the gun and a tongue of fire along
/// the shot, for a few ticks.
pub(super) fn muzzle_flash(sim: &mut Sim, at: Vec3, dir: Vec3) {
    let size = sim.effect_scale().round().max(1.0) as i32;
    let side = Vec3::new(-dir.y, dir.x, 0.0).normalize_or_zero();
    let up = side.cross(dir).normalize_or_zero();
    let mut spots = Vec::new();
    let core = size + 1;
    let middle = (core - 1) as f32 * 0.5;
    for z in 0..core {
        for y in 0..core {
            for x in 0..core {
                spots.push((Vec3::new(x as f32, y as f32, z as f32) - middle, 0));
            }
        }
    }
    let reach = 3 * size + 1;
    for i in 1..=reach {
        spots.push((dir * (middle + i as f32), (3 * i / (reach + 1)) as usize));
    }
    // Spikes fanning out forwards, like a star.
    for j in 1..=size + 1 {
        for spike in [side, -side, up, -up, (side + up) * 0.7, (side - up) * 0.7, (up - side) * 0.7, -(side + up) * 0.7] {
            spots.push((spike * (middle + j as f32) + dir * j as f32, (1 + j as usize / 2).min(2)));
        }
    }
    for (offset, shade) in spots {
        if sim.particles_full() {
            break;
        }
        sim.particles.push(Particle::new(at + offset, Vec3::ZERO, FLASH_COLORS[shade.min(2)], ParticleKind::Flash, 3.5 * DT));
    }
    let radius = sim.cfg.physics.body_width * 4.0;
    sim.flares.push(Flare::new(at, radius, [1.6, 1.2, 0.6], 3.5 * DT));
}

/// Bursts whatever stands around a body (a cage) outwards as debris:
/// everything within `radius` of `feet`, from the ground up to a little
/// over the body's head. Nobody is hurt; what is left hanging falls.
pub(super) fn break_out(sim: &mut Sim, feet: Vec3, radius: f32, height: f32) {
    let k = sim.effect_scale();
    let bedrock = sim.cfg.world.bedrock;
    let lo = (feet - Vec3::new(radius, radius, 0.0)).floor().as_ivec3();
    let hi = (feet + Vec3::new(radius, radius, height + 4.0)).ceil().as_ivec3();
    let mut removed = Vec::new();
    for z in lo.z..=hi.z {
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let cell = IVec3::new(x, y, z);
                let mid = cell.as_vec3() + 0.5;
                let out = Vec3::new(mid.x - feet.x, mid.y - feet.y, 0.0);
                if out.length() > radius || (bedrock && z == 0) || !sim.world.voxel_solid(cell) {
                    continue;
                }
                let v = sim.world.get(cell);
                sim.world.set(cell, 0);
                removed.push(cell);
                if sim.particles_full() {
                    continue;
                }
                let vel = (out.normalize_or_zero() * sim.rng.range(20.0, 45.0) + Vec3::Z * sim.rng.range(10.0, 35.0)) * k;
                let life = sim.rng.range(3.0, 5.0);
                sim.particles.push(Particle::new(mid, vel, sim.world.palette[v as usize], ParticleKind::Debris(v), life));
            }
        }
    }
    if !removed.is_empty() {
        sim.sound("crumble", feet, 0.6);
    }
    sim.drop_loose(&removed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GameConfig;
    use crate::sim::tests::flat_world;

    /// Breaking out of a cage takes what stands round the feet up to over
    /// the head, never the bedrock layer; what it leaves hanging falls.
    #[test]
    fn breaking_out_takes_the_cage_and_its_roof_falls() {
        let mut world = flat_world(IVec3::new(40, 40, 32));
        for z in 1..=12 {
            for (x, y) in [(18, 20), (22, 20), (20, 18), (20, 22)] {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
        for y in 14..=26 {
            for x in 14..=26 {
                world.set_raw(IVec3::new(x, y, 13), 1);
            }
        }
        let mut sim = Sim::new(world, &GameConfig::default(), Vec3::ZERO);
        break_out(&mut sim, Vec3::new(20.5, 20.5, 1.0), 3.0, 8.0);
        assert!([(18, 20), (22, 20), (20, 18), (20, 22)].iter().all(|&(x, y)| sim.world.get(IVec3::new(x, y, 5)) == 0), "the bars go");
        assert_eq!(sim.world.get(IVec3::new(20, 20, 0)), 1, "the floor stays");
        assert_eq!(sim.world.get(IVec3::new(14, 14, 13)), 0, "the roof, left hanging, fell");
        assert!(sim.sounds.iter().any(|s| s.name == "crumble"));
    }
}

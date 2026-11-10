//! How the shooter's things look: the player (blinking while hits do
//! nothing), enemies flashing when hit, hostages, gunships turning their
//! rotor, pickups bobbing, grenades, bullets with their tracers, rockets
//! and their light. Drawn into the engine's frame (`session::Drawing`),
//! in this order: where two things share a cell, the first keeps it.

use glam::{IVec2, Vec3};

use super::{Brain, Phase, Shooter};
use crate::render::Light;
use crate::session::Drawing;
use crate::sim::things::Frames;

/// White over a model that was just hit.
const FLASH: [u8; 4] = [255, 255, 255, 255];

impl Shooter {
    pub(super) fn draw_into(&self, out: &mut Drawing) {
        let tick = self.sim.tick;
        // The player blinks while hits do nothing, and is gone while down
        // (and on the title).
        let p = &self.player;
        let blink = self.cfg.player.blink as u64;
        let blinking = p.invulnerable > 0 && blink > 0 && (tick / blink) % 2 == 1;
        if !out.title() && !self.several() && self.status.phase == Phase::Playing && !blinking {
            let origin = out.body_origin(&p.body);
            out.figure("player", &self.rules.player_frames, &self.rules.player_swing, &p.body, origin, p.body.look, glam::Quat::IDENTITY, None);
        }
        let model = |kind: usize| self.rules.kinds[kind].model.as_deref();
        let frames = |kind: usize| &self.rules.kinds[kind].frames;
        let swing = |kind: usize| &self.rules.kinds[kind].swing;
        // A kind that turns freely faces exactly its way (a diagonal too);
        // one that spins turns round besides.
        let ticks = tick as f32 + out.alpha();
        let turn = |kind: usize, body: &crate::sim::Body| {
            let k = &self.rules.kinds[kind];
            crate::session::facing(k.free_turn, body) * crate::session::spinning(k.spin, ticks)
        };
        // The seats' players in a game of several, as the one player: by
        // their kind's model (its frames, else the player's), blinking
        // while hits do nothing, gone while down.
        for t in self.things.iter() {
            let Brain::Player(s) = &t.brain else { continue };
            let blinking = s.invulnerable > 0 && blink > 0 && (tick / blink) % 2 == 1;
            let Some(file) = model(t.kind).filter(|_| !out.title() && !s.down && !blinking) else { continue };
            let own = frames(t.kind).is_empty();
            let (frames, swing) = if own { (&self.rules.player_frames, &self.rules.player_swing) } else { (frames(t.kind), swing(t.kind)) };
            let origin = out.body_origin(&t.body);
            out.figure(file, frames, swing, &t.body, origin, t.body.look, turn(t.kind, &t.body), None);
        }
        for (t, _) in self.enemies() {
            let Some(file) = model(t.kind) else { continue };
            let origin = out.body_origin(&t.body);
            let tint = (t.flash > 0).then_some(FLASH);
            out.figure(file, frames(t.kind), swing(t.kind), &t.body, origin, t.body.look, turn(t.kind, &t.body), tint);
        }
        for t in self.things.iter().filter(|t| matches!(t.brain, Brain::Captive(_))) {
            let Some(file) = model(t.kind) else { continue };
            let origin = out.body_origin(&t.body);
            out.figure(file, frames(t.kind), swing(t.kind), &t.body, origin, t.body.look, turn(t.kind, &t.body), None);
        }
        // The gunship: its kind's frames parked (idle) and flying (fly;
        // unset, every frame of its model turns the rotor, 30 a second),
        // and it flashes white when hit.
        for (t, h) in self.gunships() {
            let Some(file) = model(t.kind) else { continue };
            let rotor = out.model_of(file).map_or(1, |m| m.frames.len());
            let (name, own, since) = match h.flying() {
                true => ("fly", Frames { first: 0, last: rotor - 1, fps: 30.0, stride: None, looped: true }, h.flown as u64),
                false => ("idle", Frames::held(0), tick),
            };
            let frame = frames(t.kind).get(name).unwrap_or(&own).at(since, 0.0).min(rotor - 1);
            let tint = (t.flash > 0).then_some(FLASH);
            let origin = out.snapped(out.between(t.body.prev, t.body.pos));
            out.model_turned(file, frame, origin, t.body.look, turn(t.kind, &t.body), tint);
        }
        // Pickups bob up and down (`bob`, `bob_ticks`).
        for t in self.things.iter().filter(|t| matches!(t.brain, Brain::Pickup)) {
            let Some(m) = model(t.kind).and_then(|file| out.model_of(file).map(|m| (file, m))) else { continue };
            let (file, m) = m;
            let r = self.rules.pickup(t.kind);
            let up = r.bob_ticks > 0 && (tick / r.bob_ticks as u64) % 2 == 1;
            let bob = Vec3::Z * if up { r.bob as f32 } else { 0.0 };
            let frame = r.frame.min(m.frames.len() - 1);
            let spin = crate::session::spinning(self.rules.kinds[t.kind].spin, ticks);
            out.model_turned(file, frame, t.body.pos + bob, IVec2::NEG_Y, spin, None);
        }
        for g in &self.grenades {
            out.blob(out.between(g.prev, g.pos), [70, 92, 48, 255], 0.0, 1.0);
        }
        // Bullets, the player's and the enemies' alike: a black voxel in front
        // and a long glowing streak behind it, orange to pale yellow, fading
        // out, never reaching back past the muzzle. Rockets are a glowing chunky
        // streak the length of one tick's flight.
        let width = self.sim.cfg.physics.body_width;
        let streak = (width * 2.0).round().max(4.0);
        let chunky = out.chunky() as f32;
        for b in &self.bullets {
            let head = out.between(b.prev, b.pos);
            let back = -b.trail().normalize_or_zero();
            if b.rocket {
                let (lead, behind) = b.colors();
                let trail = b.trail().length().max(3.0 * chunky).min(head.distance(b.origin));
                let mut d = 0.0;
                while d <= trail {
                    out.blob(head + back * d, if d < chunky { lead } else { behind }, 1.0, 1.0);
                    d += 0.5;
                }
                continue;
            }
            out.cube(out.corner(head), [16, 16, 18, 255], 0.0, 1.0);
            let reach = streak.min(head.distance(b.origin));
            let mut d = 1.0;
            while d <= reach {
                // Light orange right behind the bullet, through pale yellow to
                // almost white.
                let t = d / (streak + 1.0);
                let color = [255, (183.0 + 67.0 * t) as u8, (133.0 + 90.0 * t) as u8, 255];
                out.cube(out.corner(head + back * d), color, (1.0 - t).powf(0.6), 0.9 * (1.0 - t));
                d += 1.0;
            }
        }
        // Rockets in flight light what they pass.
        let lights = self.sim.cfg.effects.lights;
        for b in self.bullets.iter().filter(|b| b.rocket) {
            out.light(Light { pos: out.between(b.prev, b.pos), radius: width * 5.0, color: [1.2 * lights, 0.7 * lights, 0.3 * lights] });
        }
    }
}

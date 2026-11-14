//! The state hash, in parts.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// The state hash (`Game::state_hash`), each part's hash kept in
    /// `parts` as it is taken in: the ready-made rules with their things
    /// (or the core's things), `world.state`, `player.state`,
    /// `level.state`, the levels and visits, the kept states, the things'
    /// states, the seats, the music. Parts a game does not use leave the
    /// hash as it was, so every machine names the same parts.
    pub(super) fn hash_by_parts(&self, mut parts: Option<&mut Vec<(&'static str, u64)>>) -> u64 {
        let mut mark = |name: &'static str, hash: u64| {
            if let Some(parts) = parts.as_deref_mut() {
                parts.push((name, hash));
            }
        };
        // A stopped game's states are not taken again (they made it stop).
        let script = if self.halted.borrow().is_some() {
            Captured::default()
        } else {
            self.capture().unwrap_or_else(|e| {
                self.halt(e);
                Captured::default()
            })
        };
        let mut s = Stable::default();
        match (self.kit(), self.plain()) {
            (Some(kit), _) => {
                let moved_on = self.stage.level != 0 || self.stage.visits.iter().enumerate().any(|(i, &v)| v != (i == 0) as u32);
                let solid = self.stage.host.sim().world.pieces.is_some();
                if script.state.is_none() && script.player.is_none() && script.level.is_none() && script.kept.is_empty() && script.data.is_empty() && !moved_on && !self.code.uses_music && self.stage.here.lights.is_empty() && !solid {
                    let only = kit.state_hash();
                    for name in PARTS {
                        mark(name, only);
                    }
                    return only;
                }
                s.u64(kit.state_hash());
            }
            (None, Some(h)) => Self::plain_hash(h, &mut s),
            (None, None) => unreachable!("a host is plain or the kit"),
        }
        mark(PARTS[0], s.finish());
        if let Some(state) = &script.state {
            value::hash(&mut s, state);
        }
        mark(PARTS[1], s.finish());
        if let Some(player) = &script.player {
            s.u64(0x706c_6179);
            value::hash(&mut s, player);
        }
        mark(PARTS[2], s.finish());
        if let Some(level) = &script.level {
            s.u64(0x6c65_7665);
            value::hash(&mut s, level);
        }
        mark(PARTS[3], s.finish());
        // Which level and how often each was entered: only once a game has
        // left its first level, so games of one level keep their replays.
        let stage = &self.stage;
        if stage.level != 0 || stage.visits.iter().enumerate().any(|(i, &v)| v != (i == 0) as u32) {
            s.u64(0x7669_7369).u64(stage.level as u64);
            for &v in &stage.visits {
                s.u64(v as u64);
            }
        }
        mark(PARTS[4], s.finish());
        for (name, v) in &script.kept {
            s.bytes(name.as_bytes());
            value::hash(&mut s, v);
        }
        mark(PARTS[5], s.finish());
        for (id, v) in &script.data {
            s.u64(*id as u64);
            value::hash(&mut s, v);
        }
        // The pieces' lights, in a level that has some (others hash as
        // before).
        for (id, b) in &stage.here.lights {
            s.u64(0x6c69_6768).u64(*id as u64).f32(*b);
        }
        // The solid pieces taken away, in a level that has some (others
        // hash as before).
        if let Some(solids) = &stage.host.sim().world.pieces {
            s.u64(0x736f_6c69);
            for i in solids.lifted() {
                s.u64(i as u64);
            }
        }
        mark(PARTS[6], s.finish());
        // The seats: once a second was taken, or a player's own keys set
        // (a game of one hashes as it did).
        for (i, seat) in stage.seats.iter().enumerate() {
            let plain = !seat.out && seat.team.is_none() && seat.kind.is_none();
            if (i == 0 || !seat.been) && plain {
                continue;
            }
            s.u64(0x7365_6174).u64(i as u64).u64(seat.taken as u64).u64(seat.out as u64).u64(seat.score as u64).i32(seat.hp.unwrap_or(i32::MIN));
            s.bytes(seat.team.as_deref().unwrap_or("").as_bytes()).bytes(seat.kind.as_deref().unwrap_or("").as_bytes());
            let figure = stage.figure(i + 1).map_or(u64::MAX, |f| f.raw() as u64);
            s.u64(figure);
        }
        for (n, v) in &script.seats {
            s.u64(*n as u64);
            value::hash(&mut s, v);
        }
        mark(PARTS[7], s.finish());
        // The music's clock, for a game that reads or picks its music.
        if self.code.uses_music {
            let m = &stage.music;
            let next = m.next.map_or(u64::MAX, |(song, when, _)| song.map_or(u64::MAX - 1, |s| s as u64) * 4 + when as u64);
            s.u64(0x6d75_7369).u64(m.song.map_or(u64::MAX, |s| s as u64)).u64(m.steps).u64(next).u64(m.told.unwrap_or(u64::MAX));
            // The tension only once the game sets it (so earlier checks hold).
            if m.intensity != 1.0 {
                s.f32(m.intensity);
            }
        }
        mark(PARTS[8], s.finish());
        s.finish()
    }

    /// The plain host's state as one number.
    pub(super) fn plain_hash(h: &Plain, s: &mut Stable) {
        s.u64(h.sim.tick).u64(h.sim.rng.state()).u64(h.sim.world.changes()).u64(h.sim.freeze as u64);
        s.u64(h.things.len() as u64);
        for t in h.things.iter() {
            let b = &t.body;
            s.u64(t.id.raw() as u64).u64(t.kind as u64).vec3(b.pos).vec3(b.vel).i32(b.look.x).i32(b.look.y).u64(b.on_ground as u64).i32(t.hp);
        }
        s.u64(h.sim.particles.len() as u64);
        for q in &h.sim.particles {
            s.vec3(q.pos);
        }
        s.u64(h.score as u64).u64(match h.outcome {
            None => 0,
            Some(Outcome::Won) => 1,
            Some(Outcome::Lost) => 2,
        });
    }
}

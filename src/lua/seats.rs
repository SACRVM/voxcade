//! Several players: seats taken and left, their figures, where they arrive,
//! falling out, things the script moves, the camera's group and the hold.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// The seats after the first as the machines say for this tick: a seat
    /// taken (its state a copy of `c.state.player`, or kept from before),
    /// a seat left (out of play at once), and what each holds
    /// (`docs/game-model.md`, "Several players").
    pub(super) fn take_seats(&mut self) -> Result<()> {
        for n in 2..=self.stage.seats.len() {
            let s = &mut self.stage.seats[n - 1];
            let next = s.next;
            if next.is_some() && !s.taken {
                (s.taken, s.joined, s.coming) = (true, Some(if s.been { "back" } else { "new" }), None);
                if !std::mem::replace(&mut s.been, true) {
                    self.start_seat(n)?;
                }
            } else if next.is_none() && s.taken {
                (s.taken, s.left) = (false, true);
            }
            let s = &mut self.stage.seats[n - 1];
            (s.before, s.input) = (s.input, next.unwrap_or_default());
            let dirs = s.input.directions();
            for (k, held) in s.held_for.iter_mut().enumerate() {
                let on = if k < 4 { dirs[k] } else { s.input.held(k - 4) };
                *held = if on { held.saturating_add(1) } else { 0 };
            }
        }
        Ok(())
    }

    /// Seat `seat`'s state as a new player's: a copy of `c.state.player`
    /// (the score and hit points the engine's), an empty inventory where
    /// the game has items.
    pub(super) fn start_seat(&mut self, seat: usize) -> Result<()> {
        let start = self.stage.host.sim().cfg.state.player.clone();
        let has_items = !self.stage.host.sim().cfg.items.is_empty();
        let lua = &self.sandbox.lua;
        let table = lua.create_table()?;
        if has_items {
            table.raw_set("inventory", lua.create_table()?)?;
        }
        // (A key the rules keep as a name, the shooter's gun, theirs.)
        let kept: Vec<&str> = self.stage.host.player_keys().iter().copied().filter(|k| !matches!(*k, "score" | "hp" | "invulnerable")).collect();
        for (key, v) in &start {
            let s = &mut self.stage.seats[seat - 1];
            match (key.as_str(), v) {
                ("score", toml::Value::Integer(n)) => s.score = (*n).max(0) as u32,
                ("hp", toml::Value::Integer(n)) => s.hp = Some(*n as i32),
                ("score" | "hp", _) => anyhow::bail!("c.state.player.{key}: the engine keeps it, as a whole number"),
                (k, toml::Value::String(name)) if kept.contains(&k) => self.stage.host.set_seat_kept(seat, k, host::Kept::Text(name.clone())).map_err(|e| anyhow::anyhow!("c.state.player.{key}: {e}"))?,
                _ => table.raw_set(key.as_str(), world::from_toml(lua, v)?)?,
            }
        }
        self.registry("voxcade.seats")?.raw_set(seat, table)?;
        // A view of the seat's earlier state shows this one now.
        self.registry("voxcade.views")?.raw_set(format!("seat{seat}"), mlua::Value::Nil)?;
        Ok(())
    }

    /// Seats taken and left this tick, told first among the events
    /// (`on_player_join`, `on_player_leave`), seat by seat.
    pub(super) fn tell_seats(&mut self) -> Result<()> {
        for n in 2..=self.stage.seats.len() {
            let s = &mut self.stage.seats[n - 1];
            let (joined, left) = (s.joined.take(), std::mem::take(&mut s.left));
            if left {
                self.tell("on_player_leave", vec![Told::Thing(host::seat_handle(n))])?;
            }
            if let Some(how) = joined {
                self.tell("on_player_join", vec![Told::Thing(host::seat_handle(n)), Told::Str(how.to_string())])?;
            }
        }
        Ok(())
    }

    /// The players' figures as their seats say, at the tick's end (or as a
    /// level is entered, `entering`): one out of play or whose seat was
    /// left goes; one in play without a figure comes in where `p:place`
    /// put it, else (entering) at its arrival place, else where the first
    /// player in play stands; one whose `state.kind` changed becomes that
    /// kind where it stands. Only without ready-made rules: theirs are
    /// one player's yet.
    pub(super) fn seat_figures(&mut self, entering: bool) {
        if self.kit().is_some() {
            return self.kit_seat_figures(entering);
        }
        let seats = self.stage.seats.len();
        for n in 1..=seats {
            let want = self.stage.in_play(n);
            let kind = self.stage.seat_kind(n);
            let figure = self.stage.figure(n);
            let h = self.plain_mut().expect("a plain host without a kit");
            let standing = figure.and_then(|id| h.things.get(id)).map(|t| (t.kind, t.body.clone(), t.hp));
            match (standing, want, kind) {
                // Out of play, or nobody sits there: it goes, its hit points kept.
                (Some((_, _, hp)), false, _) => {
                    let id = figure.expect("a figure that stands");
                    h.things.remove(id);
                    Self::set_figure(h, n, None);
                    // The camera on the other players, if any play.
                    if h.focus == Some(id) {
                        h.focus = h.player.into_iter().chain(h.seats.iter().flatten().copied()).next();
                    }
                    self.stage.seats[n - 1].hp = Some(hp);
                    self.stage.seats[n - 1].coming = None;
                }
                // Another kind now: that kind where it stands.
                (Some((was, body, hp)), true, Some(k)) if was != k => {
                    let id = figure.expect("a figure that stands");
                    let mut body = body;
                    body.height = h.kinds[k].height;
                    let new = h.things.spawn(k, body, if h.kinds[k].has_hp { hp } else { 0 }, crate::sim::things::Walk::default());
                    h.things.remove(id);
                    if h.focus == Some(id) {
                        h.focus = Some(new);
                    }
                    Self::set_figure(h, n, Some(new));
                }
                // In play without a figure: it comes in.
                (None, true, Some(k)) => {
                    let coming = self.stage.seats[n - 1].coming.take();
                    let first = (1..=seats).filter(|&m| m != n && self.stage.in_play(m)).find_map(|m| self.stage.seen(host::seat_handle(m)).map(|t| t.body.pos));
                    let arrival = self.arrival(n);
                    let (at, facing) = match (coming, entering, first) {
                        (Some(c), _, _) => c,
                        (None, false, Some(at)) => (at, None),
                        _ => (arrival.at, arrival.facing),
                    };
                    let hp = self.stage.seats[n - 1].hp.take();
                    let h = self.plain_mut().expect("a plain host without a kit");
                    let (height, half) = (h.kinds[k].height, h.sim.half());
                    let at = crate::sim::stand_at(&h.sim.world, at, height, half);
                    let mut body = Body::new(at, height);
                    if let Some(way) = facing {
                        body.face(way.as_vec2().extend(0.0));
                    }
                    let id = h.things.spawn(k, body, hp.unwrap_or(h.kinds[k].hp), crate::sim::things::Walk::default());
                    if n == 1 && !h.looking {
                        h.focus = Some(id);
                    }
                    Self::set_figure(h, n, Some(id));
                    // What it stands in it touches without on_touch.
                    self.stage.placed.push(host::seat_handle(n));
                }
                _ => {}
            }
        }
    }

    /// The ready-made rules' figures for the seats after the first, as
    /// `seat_figures` does a game's own (seat 1's are the rules' own).
    pub(super) fn kit_seat_figures(&mut self, entering: bool) {
        // Seat 1's too in a game of several (the rules' own player stays
        // in play in a game of one).
        let from = if self.stage.seats.len() > 1 { 1 } else { 2 };
        for n in from..=self.stage.seats.len() {
            let (want, kind, figure) = (self.stage.in_play(n), self.stage.seat_kind(n), self.stage.figure(n));
            let standing = figure.and_then(|id| self.stage.host.get(id)).map(|t| (t.kind, t.body.pos, t.body.look, t.hp));
            let placed = match (standing, want, kind) {
                (Some((_, _, _, hp)), false, _) => {
                    self.stage.seats[n - 1].hp = Some(hp);
                    self.stage.host.put_seat(n, None)
                }
                (Some((was, at, look, hp)), true, Some(k)) if was != k => self.stage.host.put_seat(n, Some((k, at, Some(look), hp))),
                (None, true, Some(k)) => {
                    let coming = self.stage.seats[n - 1].coming.take();
                    let first = (1..=self.stage.seats.len()).filter(|&m| m != n && self.stage.in_play(m)).find_map(|m| self.stage.seen(host::seat_handle(m)).map(|t| t.body.pos));
                    let arrival = self.arrival(n);
                    let (at, facing) = match (coming, entering, first) {
                        (Some(c), _, _) => c,
                        (None, false, Some(at)) => (at, None),
                        _ => (arrival.at, arrival.facing),
                    };
                    let hp = self.stage.seats[n - 1].hp.take().unwrap_or(self.stage.kinds[k].hp);
                    let placed = self.stage.host.put_seat(n, Some((k, at, facing, hp)));
                    self.stage.placed.push(host::seat_handle(n));
                    placed
                }
                _ => Ok(None),
            };
            if let Err(e) = placed {
                self.halt(anyhow::anyhow!("player {n}: {e}"));
            }
        }
    }

    /// Seat `seat`'s figure is `id` (seat 1's the player's).
    pub(super) fn set_figure(h: &mut Plain, seat: usize, id: Option<Id>) {
        match seat {
            1 => h.player = id,
            _ => h.seats[seat - 2] = id,
        }
    }

    /// Where seat `seat`'s player arrives in the level being played: the
    /// place the level was entered at, `NAME_n` for seat n where the level
    /// has one.
    pub(super) fn arrival(&self, seat: usize) -> crate::levels::Place {
        let arrived = &self.stage.arrived;
        if seat > 1 {
            let own = format!("{}_{seat}", arrived.name);
            if let Some(p) = self.stage.levels.list[self.stage.level].places.iter().find(|p| p.name == own) {
                return p.clone();
            }
        }
        arrived.clone()
    }

    /// Arriving somewhere (a level entered, the game's start): what touches
    /// now touches without `on_touch`, until it parts.
    pub(super) fn arrive(&mut self) {
        self.stage.touching = if self.handles("on_touch") || self.handles("on_touch_end") || self.actions.contains(&"interact") { self.stage.contacts() } else { Default::default() };
        self.stage.placed.clear();
        self.stage.note_positions();
    }

    /// The things the script moves that fell out of the level: told in
    /// the next tick (`on_fall_out`, once: `tell_event` notes it in
    /// `fallen`); the kits tell of their own.
    pub(super) fn fell_out(&mut self) {
        let stage = &mut self.stage;
        // Back in the level (placed again): it may fall out again.
        let fallen: Vec<u32> = stage.fallen.iter().copied().filter(|&raw| stage.seen(raw).is_some_and(|t| crate::sim::fell_out(&stage.host.sim().world, t.body, stage.host.sim().cfg.physics.lost_below))).collect();
        stage.fallen = fallen.into_iter().collect();
        let mut fell = Vec::new();
        for id in stage.host.ids() {
            let Some(t) = stage.host.get(id) else { continue };
            let raw = stage.figure_handle(id).unwrap_or(id.raw());
            // (A piece stays where its level put it, under the floor too: a
            // piece laid on its side has its pivot there.)
            let scripted = stage.kinds.get(t.kind).is_some_and(|k| k.scripted) && !stage.pieces.contains_key(&id.raw());
            if scripted && !stage.fallen.contains(&raw) && crate::sim::fell_out(&stage.host.sim().world, t.body, stage.host.sim().cfg.physics.lost_below) {
                fell.push(raw);
            }
        }
        for raw in fell {
            let who = match host::handle_seat(raw) {
                Some(1) => crate::sim::Who::Player,
                Some(seat) => crate::sim::Who::Thing(stage.figure(seat).expect("a figure that fell")),
                None => crate::sim::Who::Thing(Id::from_raw(raw)),
            };
            stage.later.push(crate::sim::Event::FellOut { who });
        }
    }

    /// The things the script moves go the way they were told; without
    /// ready-made rules, the debris and the camera too.
    pub(super) fn move_scripted(&mut self) {
        let kinds = self.stage.kinds.clone();
        let fixed = move |kind: usize| kinds.get(kind).is_some_and(|k| k.fixed);
        if let Some(kit) = self.kit_mut() {
            kit.move_scripted(&fixed);
            self.fell_out();
            if let Err(e) = self.prune() {
                self.halt(e);
            }
            return;
        }
        let h = self.plain_mut().expect("a plain host without a kit");
        host::move_things(&mut h.things, &h.sim, &fixed, |w| Some(w));
        self.hold_players();
        self.fell_out();
        if let Err(e) = self.prune() {
            self.halt(e);
        }
        // Several players: the camera goes for the middle of their span.
        let group = if self.follows_players() { self.group_figures() } else { Vec::new() };
        let h = self.plain_mut().expect("a plain host without a kit");
        let half = h.sim.half();
        let mut focus = h.focus.and_then(|id| h.things.get(id)).map(|t| (t.body.pos, t.body.height));
        if group.len() > 1 {
            let at: Vec<Vec3> = group.iter().filter_map(|&id| h.things.get(id)).map(|t| t.body.pos).collect();
            let (lo, hi) = at.iter().fold((at[0], at[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            focus = Some(((lo + hi) * 0.5, focus.map_or(0.0, |f| f.1)));
        }
        let keep_clear = focus.map_or((Vec3::splat(-1.0), Vec3::splat(-1.0)), |(p, height)| (p - Vec3::new(half, half, 0.0), p + Vec3::new(half, half, height)));
        h.sim.update_particles(keep_clear);
        h.sim.update_flares();
        let target = focus.map_or(h.sim.cam_target, |(p, _)| p);
        h.sim.follow(target);
    }

    pub(super) fn focus_body(&self) -> Option<&Body> {
        let h = self.plain()?;
        h.focus.and_then(|id| h.things.get(id)).map(|t| &t.body)
    }

    /// The players' figures playing and alive, in seat order: those the
    /// camera keeps together and the hold holds (`c.camera.hold`).
    pub(super) fn group_figures(&self) -> Vec<Id> {
        let host = &self.stage.host;
        let alive = |n: usize, id: Id| match (n, self.kit().is_some()) {
            // Seat 1's hearts are the ready-made rules' run.
            (1, true) => host.player_number("hp").is_none_or(|hp| hp > 0),
            (_, true) => host.get(id).is_some_and(|t| t.hp > 0),
            _ => host.get(id).is_some_and(|t| !self.stage.kinds[t.kind].has_hp || t.hp > 0),
        };
        (1..=self.stage.seats.len()).filter(|&n| self.stage.in_play(n)).filter_map(|n| self.stage.figure(n).map(|id| (n, id))).filter(|&(n, id)| alive(n, id)).map(|(_, id)| id).collect()
    }

    /// Whether the camera follows the players (not a thing of the level
    /// it was told to follow, nor a point).
    pub(super) fn follows_players(&self) -> bool {
        match self.plain() {
            Some(h) => !h.looking && h.focus.is_none_or(|f| self.stage.figure_handle(f).is_some()),
            None => self.stage.host.follows_players(),
        }
    }

    /// Keeps the players playing and alive within the hold of each other
    /// (`c.camera.hold`), as an arcade machine's screen did: each player's
    /// move, in seat order, cut short where it took the span of the
    /// players further than the hold (and further than it was); a move
    /// that narrows it is never stopped.
    pub(super) fn hold_players(&mut self) {
        let Some(limit) = self.stage.host.sim().cfg.camera.hold_span() else { return };
        let figures = self.group_figures();
        if figures.len() < 2 {
            return;
        }
        let half = self.stage.host.sim().half();
        for &id in &figures {
            let host = &self.stage.host;
            let others: Vec<Vec3> = figures.iter().filter(|&&o| o != id).filter_map(|&o| host.get(o)).map(|t| t.body.pos).collect();
            let Some(t) = host.get(id) else { continue };
            let (prev, now, height) = (t.body.prev, t.body.pos, t.body.height);
            let mut at = now;
            for axis in 0..2 {
                let (lo, hi) = others.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p[axis]), hi.max(p[axis])));
                let before = hi.max(prev[axis]) - lo.min(prev[axis]);
                let room = limit[axis].max(before);
                at[axis] = at[axis].clamp(hi - room, lo + room);
            }
            if at == now {
                continue;
            }
            // Cut short on the way it came; where that is no room, it stays.
            let at = if crate::sim::body_hits(&host.sim().world, at, half, height) { Vec3::new(prev.x, prev.y, at.z) } else { at };
            match self.plain_mut() {
                Some(h) => {
                    if let Some(t) = h.things.get_mut(id) {
                        for axis in 0..2 {
                            if at[axis] != t.body.pos[axis] {
                                t.body.vel[axis] = 0.0;
                            }
                        }
                        t.body.pos = at;
                    }
                }
                None => self.stage.host.hold_back(id, at),
            }
        }
    }
}

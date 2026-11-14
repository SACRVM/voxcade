//! Events told to the script: the kits' hits, deaths and pickups, the
//! touches the engine finds, interact.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// The handle id an event names: the player's for its figure.
    pub(super) fn fold(&self, who: crate::sim::Who) -> u32 {
        match who {
            crate::sim::Who::Player => PLAYER,
            crate::sim::Who::Thing(id) => self.stage.figure_handle(id).unwrap_or(id.raw()),
        }
    }

    /// Whether the level is held (`level.state.held` set, not false).
    pub(super) fn held(&self) -> bool {
        self.registry("voxcade.level").ok().and_then(|t| t.raw_get::<mlua::Value>("held").ok()).is_some_and(|v| !matches!(v, mlua::Value::Nil | mlua::Value::Boolean(false)))
    }

    /// Whether the script has a handler called `name`.
    pub(super) fn handles(&self, name: &str) -> bool {
        self.globals.as_ref().and_then(|g| Sandbox::function(g, name).ok().flatten()).is_some()
    }

    /// Calls the script's handler `name` (if it has one) with `args`.
    pub(super) fn tell(&mut self, name: &'static str, args: Vec<Told>) -> Result<()> {
        let Some(f) = self.globals.as_ref().map(|g| Sandbox::function(g, name)).transpose()?.flatten() else { return Ok(()) };
        world::set_phase(&self.sandbox.lua, None);
        self.with_world(|sb, _| {
            let lua = &sb.lua;
            let mut values = Vec::new();
            for arg in args {
                values.push(match arg {
                    Told::Thing(id) => mlua::Value::UserData(world::handle(lua, id)?),
                    Told::Nil => mlua::Value::Nil,
                    Told::Int(n) => mlua::Value::Integer(n),
                    Told::Str(s) => mlua::Value::String(lua.create_string(s)?),
                });
            }
            sb.call::<()>(&f, mlua::MultiValue::from_vec(values), name)
        })
    }

    /// What happened to things since the last tick's events, told to the
    /// script in order (`docs/game-model.md`, "A tick, in order", step 3):
    /// deaths the script caused by setting hit points (and things back to
    /// life), what the ready-made rules and `thing:hurt`/`thing:take` did
    /// (`on_hit`, `on_death`, `on_fall_out`, `on_pickup`), touches that began and
    /// ended, and the action button on what the player touches
    /// (`on_interact`). Returns the action that press used up.
    pub(super) fn tell_events(&mut self, input: &Input) -> Result<Option<usize>> {
        // Things back to life.
        let stage = &mut self.stage;
        let alive = |stage: &Stage, id: u32| stage.seen(id).is_some_and(|t| t.hp > 0);
        let back: Vec<u32> = stage.dead.iter().copied().filter(|&id| alive(stage, id)).collect();
        for id in back {
            stage.dead.remove(&id);
        }
        // What the script did to things last tick (thing:hurt, thing:take),
        // then the script's things whose hit points ran out, then what the
        // ready-made rules did this tick.
        let later = std::mem::take(&mut self.stage.later);
        let ruled = std::mem::take(&mut self.stage.host.sim_mut().events);
        for event in later {
            self.tell_event(event)?;
        }
        let stage = &mut self.stage;
        let mut died = Vec::new();
        for id in stage.host.ids() {
            let Some(t) = stage.host.get(id) else { continue };
            let raw = stage.figure_handle(id).unwrap_or(id.raw());
            let kind = &stage.kinds[t.kind];
            if kind.scripted && kind.has_hp && t.hp <= 0 && !stage.dead.contains(&raw) {
                died.push((raw, t.kind, t.body.pos, t.body.look));
            }
        }
        // A hurt it lived through names nobody's later death.
        self.stage.killers.retain(|id, _| died.iter().any(|d| d.0 == *id));
        for (id, kind, at, look) in died {
            // Its kind's effect and sound, as for the ready-made rules' dead
            // (a player's figure stays).
            if host::handle_seat(id).is_none() {
                self.stage.goes(kind, at, look);
            }
            self.stage.dead.insert(id);
            // Who did it, where thing:hurt did (not hit points set by hand).
            let (by, weapon) = self.stage.killers.remove(&id).unwrap_or_default();
            let by = by.filter(|&b| self.stage.seen(b).is_some() || host::handle_seat(b).is_some()).map_or(Told::Nil, Told::Thing);
            self.tell("on_death", vec![Told::Thing(id), by, weapon.map_or(Told::Nil, Told::Str)])?;
        }
        for event in ruled {
            self.tell_event(event)?;
        }
        // Touches: begun and ended (not with a thing gone or dead).
        let interact = self.actions.iter().position(|a| *a == "interact");
        let pressed = interact.filter(|&a| input.held(a) && !self.before.held(a));
        if self.held() {
            return Ok(None);
        }
        let any_pressed = pressed.is_some() || interact.is_some_and(|a| self.stage.seats.iter().skip(1).any(|s| s.input.held(a) && !s.before.held(a)));
        let found = self.handles("on_touch") || self.handles("on_touch_end") || any_pressed;
        if found {
            let now = self.stage.contacts();
            let ended: Vec<(u32, u32)> = self.stage.touching.difference(&now).copied().collect();
            let placed = std::mem::take(&mut self.stage.placed);
            // How each began, judged from where things stood when touches
            // were last looked for, before anything is told.
            let begun: Vec<(u32, u32, &'static str)> = now
                .difference(&self.stage.touching)
                .filter(|(a, b)| !placed.contains(a) && !placed.contains(b))
                .map(|&(a, b)| match self.stage.met(a, b) {
                    // Of two players, the one that came down onto the other.
                    host::Met::Bottom if host::handle_seat(b).is_some() => (b, a, host::Met::Top.word()),
                    met => (a, b, met.word()),
                })
                .collect();
            self.stage.touching = now;
            self.stage.note_positions();
            for (a, b, met) in begun {
                self.tell("on_touch", vec![Told::Thing(a), Told::Thing(b), Told::Str(met.to_string())])?;
            }
            for (a, b) in ended {
                let there = |id: u32| self.stage.seen(id).is_some() && !self.stage.dead.contains(&id) && !self.stage.gone.contains(&id) && !self.stage.graveyard.iter().any(|(g, ..)| *g == id);
                if there(a) && there(b) {
                    self.tell("on_touch_end", vec![Told::Thing(a), Told::Thing(b)])?;
                }
            }
        }
        // The action button on the nearest thing each player touches, seat
        // by seat; the press used up for its seat.
        let interact = self.actions.iter().position(|a| *a == "interact");
        let mut used = None;
        for n in 1..=self.stage.seats.len() {
            let pressed = match n {
                1 => pressed,
                _ => interact.filter(|&a| self.stage.seats[n - 1].input.held(a) && !self.stage.seats[n - 1].before.held(a)),
            };
            let Some(action) = pressed.filter(|_| self.stage.playing(n)) else { continue };
            if self.interact(host::seat_handle(n))? {
                match n {
                    1 => used = Some(action),
                    _ => self.stage.seats[n - 1].before.hold(action, true),
                }
            }
        }
        Ok(used)
    }

    /// `on_interact` for the nearest thing player `who` touches, if any.
    pub(super) fn interact(&mut self, who: u32) -> Result<bool> {
        let Some(me) = self.stage.seen(who).map(|t| t.body.pos) else { return Ok(false) };
        let nearest = self
            .stage
            .touching
            .iter()
            .filter_map(|&(a, b)| (a == who).then_some(b).or((b == who).then_some(a)))
            .filter_map(|b| Some((self.stage.seen(b)?.body.pos.truncate().distance_squared(me.truncate()), b)))
            .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        let Some((_, thing)) = nearest else { return Ok(false) };
        self.tell("on_interact", vec![Told::Thing(thing), Told::Thing(who)])?;
        Ok(true)
    }

    /// One event of the ready-made rules' (or of the script's own,
    /// `thing:hurt`, `thing:take`) told to the script.
    pub(super) fn tell_event(&mut self, event: crate::sim::Event) -> Result<()> {
        match event {
            crate::sim::Event::Hit { who, by, damage, weapon } => {
                let args = vec![Told::Thing(self.fold(who)), by.map_or(Told::Nil, |b| Told::Thing(self.fold(b))), Told::Int(damage as i64), weapon.map_or(Told::Nil, Told::Str)];
                self.tell("on_hit", args)
            }
            crate::sim::Event::FellOut { who } => {
                // Once until it is back in, whoever saw it fall (a kit tells
                // every tick it is out).
                let id = self.fold(who);
                if self.stage.seen(id).is_none() || !self.stage.fallen.insert(id) {
                    return Ok(());
                }
                self.tell("on_fall_out", vec![Told::Thing(id)])?;
                // What nobody can reach any more is taken away; the player
                // waits for what the game decides.
                if host::handle_seat(id).is_none() && !self.stage.gone.contains(&id) {
                    self.stage.gone.push(id);
                }
                Ok(())
            }
            crate::sim::Event::Death { who, by, kind, body, weapon } => {
                let id = self.fold(who);
                if host::handle_seat(id).is_none() {
                    self.stage.goes(kind, body.pos, body.look);
                }
                if self.stage.seen(id).is_none() {
                    let half = self.stage.host.sim().half();
                    self.stage.graveyard.push((id, kind, body, half));
                }
                if !self.stage.dead.insert(id) {
                    // Told already (its hit points ran out by the script's hand).
                    return Ok(());
                }
                let by = by.map_or(Told::Nil, |b| Told::Thing(self.fold(b)));
                self.tell("on_death", vec![Told::Thing(id), by, weapon.map_or(Told::Nil, Told::Str)])
            }
            crate::sim::Event::Pickup { who, item } => {
                self.tell("on_pickup", vec![Told::Thing(self.fold(who)), Told::Str(item)])
            }
        }
    }
}

//! The run's course: levels left and entered at the tick's end, the run's
//! end.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// Does what the script asked at the tick's end (`docs/game-model.md`,
    /// "Leaving a level"): `on_level_end(why)`, then another level entered
    /// (or this one afresh) and `on_level_start(how)`, or the run's end. An
    /// ask in that `on_level_start` counts at once; a mistake stops the
    /// game.
    pub(super) fn change_levels(&mut self) {
        loop {
            if self.halted.borrow().is_some() || self.stage.ended {
                return;
            }
            let Some(ask) = self.stage.ask.take() else { return };
            self.changes += 1;
            if self.changes > 100 {
                self.halt(anyhow::anyhow!("levels keep being left before a tick was played (a hundred times): does on_level_start ask again and again?"));
                return;
            }
            if let Err(e) = self.call_handler("on_level_end", ask.why(), Some(world::Phase::LevelEnd)) {
                // Nothing has changed yet: the ask waits for the file to be mended.
                self.stage.ask = Some(ask);
                self.halt(e);
                return;
            }
            let entered = match ask {
                Ask::Won => return self.end_run(Outcome::Won),
                Ask::Lost => return self.end_run(Outcome::Lost),
                Ask::GoTo { level, place } => match self.stage.levels.find(&level) {
                    Some(index) => {
                        // A checkpoint belongs to the level it was reached in
                        // (every player's).
                        if let Ok(player) = self.registry("voxcade.player") {
                            let _ = player.raw_set("checkpoint", mlua::Value::Nil);
                        }
                        if let Ok(seats) = self.registry("voxcade.seats") {
                            for (_, state) in seats.pairs::<i64, Table>().flatten() {
                                let _ = state.raw_set("checkpoint", mlua::Value::Nil);
                            }
                        }
                        let place = self.place_in(index, place);
                        self.enter(index, place, "go_to", None)
                    }
                    None => Err(anyhow::anyhow!("world:go_to: the level '{level}' is gone (its file was taken away while playing)")),
                },
                Ask::Restart { place } => {
                    let checkpoint = self.registry("voxcade.player").ok().and_then(|p| p.raw_get::<Option<String>>("checkpoint").ok().flatten());
                    let level = &self.stage.levels.list[self.stage.level];
                    // A checkpoint is a place, or a thing of the level by its
                    // name (stood at once it is there again).
                    let (at_place, at_thing) = match checkpoint {
                        Some(c) if level.places.iter().any(|p| p.name == c) => (Some(c), None),
                        Some(c) => (None, Some(c)),
                        None => (None, None),
                    };
                    let thing = at_thing.filter(|_| place.is_none());
                    let place = self.place_in(self.stage.level, place.or(at_place));
                    self.arrive_at = thing;
                    self.enter(self.stage.level, place, "restart", None)
                }
                Ask::Load { slot: None } => match self.start_text.take() {
                    Some(text) => self.load_save(&text).context("loading a save"),
                    None => Err(anyhow::anyhow!("no save to go on from")),
                },
                // A replay's load reads what its run read then.
                Ask::Load { slot: Some(n) } => match self.stage.replayed.pop_front().or_else(|| self.stage.slot_texts.get(&n).cloned()) {
                    Some(text) => {
                        self.stage.loaded.push(text.clone());
                        self.load_save(&text).with_context(|| format!("loading the save in slot {n}"))
                    }
                    None => Err(anyhow::anyhow!("world:load: slot {n} is empty")),
                },
            };
            if let Err(e) = entered {
                self.halt(e);
                return;
            }
        }
    }

    /// The place called `name` in level `index` (without a name, its place
    /// called start); None: it has none, the player starts where the
    /// level's rules stand it.
    pub(super) fn place_in(&self, index: usize, name: Option<String>) -> Option<crate::levels::Place> {
        let name = name.unwrap_or_else(|| "start".into());
        self.stage.levels.list[index].places.iter().find(|p| p.name == name).cloned()
    }

    /// Enters level `index` (`how` it came to be entered), the player at
    /// `place` (or where the level starts): its things from its file,
    /// `level.state` fresh (or a save's, with the voxels its file would not
    /// bring back), then `on_level_start(how)`.
    pub(super) fn enter(&mut self, index: usize, place: Option<crate::levels::Place>, how: &'static str, loaded: Option<Loaded>) -> Result<()> {
        let levels = Arc::clone(&self.stage.levels);
        let level = &levels.list[index];
        let arrived = place.clone().unwrap_or_else(|| self.stage.start(index));
        self.stage.level = index;
        let mut world = level.world.clone();
        if let Some(l) = &loaded {
            for &(i, colour) in &l.palette {
                world.palette[i] = colour;
            }
            for &(p, v) in &l.voxels {
                world.set(p, v);
            }
        }
        match self.kit_mut() {
            Some(kit) => kit.enter_level(world, &level.markers, &level.places, place.map(|p| (p.at, p.facing))),
            None => {
                // The player's figure goes along; the level's things are the
                // new level's.
                let h = self.plain_mut().expect("a plain host without a kit");
                let player = h.player;
                let figures: Vec<Id> = player.into_iter().chain(h.seats.iter().flatten().copied()).collect();
                h.things.retain(|t| figures.contains(&t.id));
                (h.focus, h.looking) = (player, false);
                h.sim.replace_world(world);
                Self::place(h, &level.markers);
                Self::stand_player(h, &arrived);
                (h.sim.cam_target, h.sim.cam_prev) = (arrived.at, arrived.at);
            }
        }
        // A player arriving with no hit points left gets the start's back
        // (`c.state.player.hp`).
        if self.kit().is_some()
            && let Some(hp) = self.stage.host.sim().cfg.state.player_number("hp")
            && self.stage.host.player_number("hp").is_some_and(|n| n <= 0)
        {
            self.stage.host.set_player_number("hp", hp);
        }
        let sim = self.stage.host.sim_mut();
        (sim.freeze, sim.shake, sim.screen_flash) = (0, 0.0, 0.0);
        sim.flares.clear();
        self.stage.arrived = arrived;
        // The seats after the first at their arrival places (`NAME_n`), or
        // where a save stood them.
        let saved = std::mem::take(&mut self.seat_places);
        for n in 2..=self.stage.seats.len() {
            let place = saved.iter().find(|(m, _)| *m == n).map_or_else(|| self.arrival(n), |(_, p)| p.clone());
            if let Some(id) = self.stage.figure(n)
                && let Some(h) = self.plain_mut()
            {
                Self::stand_figure(h, id, &place);
            }
        }
        if how != "reload" && how != "load" {
            self.stage.visits[index] += 1;
        }
        self.stage.gone.clear();
        // The level's things are new: nothing touches, nothing died yet.
        (self.stage.touching, self.stage.dead, self.stage.graveyard, self.stage.placed, self.stage.later, self.stage.killers) = Default::default();
        self.spawn_pieces();
        self.stage.name_things();
        // A save's lights as they were, by the pieces' names.
        if let Some(l) = &loaded {
            for (name, brightness) in &l.lights {
                if let Some(id) = self.stage.named(name).filter(|id| self.stage.lights.contains_key(id)) {
                    self.stage.lights.insert(id, brightness.clamp(0.0, 1.0));
                }
            }
        }
        // Arriving at a thing (a checkpoint by its name): stood where it is.
        if let Some(name) = self.arrive_at.take()
            && let Some(at) = self.stage.named(&name).and_then(|id| self.stage.seen(id)).map(|t| t.body.pos)
        {
            let place = crate::levels::Place::new(&name, at, None);
            match self.plain_mut() {
                Some(h) => Self::stand_player(h, &place),
                None => {
                    let _ = self.stage.host.put_player(at);
                }
            }
            self.stage.arrived = place;
        }
        // Started over, the other players at their own checkpoints (a place,
        // or a thing by its name), if they passed one.
        if how == "restart" {
            for n in 2..=self.stage.seats.len() {
                let checkpoint = self.registry("voxcade.seats").ok().and_then(|t| t.raw_get::<Option<Table>>(n).ok().flatten()).and_then(|t| t.raw_get::<Option<String>>("checkpoint").ok().flatten());
                let Some(name) = checkpoint else { continue };
                let place = level.places.iter().find(|p| p.name == name).cloned().or_else(|| {
                    let at = self.stage.named(&name).and_then(|id| self.stage.seen(id)).map(|t| t.body.pos)?;
                    Some(crate::levels::Place::new(&name, at, None))
                });
                if let (Some(place), Some(id)) = (place, self.stage.figure(n))
                    && let Some(h) = self.plain_mut()
                {
                    Self::stand_figure(h, id, &place);
                }
            }
        }
        let lua = &self.sandbox.lua;
        for name in ["voxcade.level", "voxcade.data", "voxcade.views", "voxcade.props"] {
            lua.set_named_registry_value(name, lua.create_table()?)?;
        }
        if let Some(l) = loaded {
            lua.set_named_registry_value("voxcade.level", value::restore(lua, &l.level_state)?)?;
        }
        self.call_handler("on_level_start", how, None)?;
        self.stage.settle();
        self.seat_figures(true);
        self.built();
        self.arrive();
        // Without them, the camera over the thing it follows, as at the start.
        if let Some(b) = self.focus_body() {
            let target = b.pos + Vec3::Z * b.height * 0.5;
            let sim = self.stage.host.sim_mut();
            (sim.cam_target, sim.cam_prev) = (target, target);
        }
        self.check_states().map_err(|e| anyhow::anyhow!("{e:#} (after on_level_start)"))?;
        // A world:save of on_level_start keeps this level, before an ask
        // of it goes on.
        self.write_saves();
        Ok(())
    }

    /// The run ends (`won` or `lost`, `on_level_end` told already):
    /// `on_world_end(result)`, then the outcome is the host's.
    pub(super) fn end_run(&mut self, outcome: Outcome) {
        if self.stage.ended {
            return;
        }
        self.stage.ended = true;
        let result = match outcome {
            Outcome::Won => "won",
            Outcome::Lost => "lost",
        };
        if let Err(e) = self.call_handler("on_world_end", result, Some(world::Phase::WorldEnd)) {
            self.halt(e);
        }
        self.stage.host.end(outcome);
    }
}

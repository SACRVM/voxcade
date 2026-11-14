//! Save games: the game as a save, written and loaded.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// The game as a save (`save::SaveData`): the states, every level's
    /// kept and visits, the level being played and its voxels changed,
    /// where the player stands, the engine's numbers and the ready-made
    /// rules' run, the time and the random generator.
    pub(super) fn save_data(&self) -> Result<save::SaveData> {
        let captured = self.capture()?;
        let write = |v: &Option<Value>| v.as_ref().map_or_else(|| "{}".to_string(), data::write);
        let stage = &self.stage;
        let level = &stage.levels.list[stage.level];
        let host = &stage.host;
        let sim = host.sim();
        let changed = sim.world.changed_from(&level.world);
        let palette = (1..256)
            .filter(|&i| sim.world.palette[i] != level.world.palette[i])
            .map(|i| {
                let [r, g, b, a] = sim.world.palette[i];
                (i.to_string(), format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
            })
            .collect();
        let figure = host.player_seen().map(|t| save::Figure { at: t.body.pos.to_array(), facing: t.body.look.to_array() });
        // The seats someone sat on (seat 1's own keys where set).
        let seats = (1..=stage.seats.len())
            .filter(|&n| {
                let s = &stage.seats[n - 1];
                (n > 1 && s.been) || s.out || s.team.is_some() || s.kind.is_some()
            })
            .map(|n| {
                let s = &stage.seats[n - 1];
                let standing = (n > 1).then(|| stage.seen(host::seat_handle(n))).flatten();
                save::SavedSeat {
                    seat: n,
                    taken: s.taken,
                    state: captured.seats.iter().find(|(m, _)| *m == n).map(|(_, v)| data::write(v)).unwrap_or_default(),
                    score: s.score,
                    hp: standing.as_ref().map(|t| t.hp).or(s.hp),
                    out: s.out,
                    team: s.team.clone(),
                    kind: s.kind.clone(),
                    figure: standing.map(|t| save::Figure { at: t.body.pos.to_array(), facing: t.body.look.to_array() }),
                }
            })
            .collect();
        Ok(save::SaveData {
            seats,
            format: save::FORMAT,
            game: sim.cfg.title.clone(),
            version: sim.cfg.saves.version,
            level: level.name.clone(),
            ticks: sim.tick,
            rng: format!("{:016x}", sim.rng.state()),
            states: save::States { world: write(&captured.state), player: write(&captured.player), level: write(&captured.level) },
            kept: captured.kept.iter().map(|(n, v)| (n.clone(), data::write(v))).collect(),
            visits: stage.levels.list.iter().zip(&stage.visits).filter(|(_, v)| **v > 0).map(|(l, v)| (l.name.clone(), *v)).collect(),
            numbers: host.player_keys().iter().filter_map(|k| host.player_number(k).map(|n| (k.to_string(), n))).collect(),
            figure,
            run: self.kit().map(|k| k.run()).unwrap_or_default(),
            voxels: save::write_voxels(&changed, sim.world.size),
            size: sim.world.size.to_array(),
            palette,
            lights: stage.lights.iter().filter(|(id, _)| host.get(Id::from_raw(**id)).is_some()).filter_map(|(id, b)| Some((stage.names.get(id)?.clone(), *b))).collect(),
        })
    }

    /// Makes the saves `world:save` asked for, as the game stands now (the
    /// tick's end, the end of an `on_level_start`): the script reads them
    /// at once (`world.saves`, `world:load`), the session writes them into
    /// the slots. A game stopped by a mistake is in no state to keep.
    pub(super) fn write_saves(&mut self) {
        let slots = std::mem::take(&mut self.stage.saving);
        if slots.is_empty() || self.halted.borrow().is_some() {
            return;
        }
        let saved = self.save_data().and_then(|data| Ok((data.write()?, data)));
        let (text, data) = match saved {
            Ok(saved) => saved,
            Err(e) => return self.halt(e.context("world:save")),
        };
        for n in slots {
            let info = crate::saves::SlotInfo { level: data.level.clone(), ticks: data.ticks, when: 0 };
            self.stage.slots[n as usize - 1] = Some(info);
            self.stage.slot_texts.insert(n, Arc::from(text.as_str()));
            self.written.push((n, text.clone()));
        }
    }

    /// Goes on from a save (`docs/game-model.md`, "Save games"): its states,
    /// kept and visits, time and random generator, the player's numbers and
    /// the ready-made rules' run; `on_save_loaded(version)`; then its level
    /// entered (`"load"`) with its
    /// `level.state`, the player where it stood. Names the game no longer
    /// has (a level, an item) are dropped with a note.
    pub(super) fn load_save(&mut self, text: &str) -> Result<()> {
        let save = save::SaveData::read(text)?;
        let lua = self.sandbox.lua.clone();
        let read = |text: &str, what: &str| -> Result<Value> { data::read(text).with_context(|| format!("the save's {what}")) };
        // Its level, as it stood; one gone since: the first, as it starts.
        let found = self.stage.levels.find(&save.level);
        if found.is_none() {
            self.note(&format!("the save's level '{}' is gone: the first level instead, from its start", save.level));
        }
        let index = found.unwrap_or(0);
        let (world_state, player_state) = (read(&save.states.world, "world.state")?, read(&save.states.player, "player.state")?);
        let level_state = if found.is_some() { read(&save.states.level, "level.state")? } else { Value::Table(Vec::new()) };
        let mut kept = Vec::new();
        for (name, text) in &save.kept {
            match self.stage.levels.find(name) {
                Some(_) => kept.push((name.clone(), read(text, &format!("level.kept of {name}"))?)),
                None => self.note(&format!("the save's level.kept of '{name}' is dropped: the level is gone")),
            }
        }
        // The voxels its file would not bring back, where the level is the
        // one they were counted in (the same size).
        let size = self.stage.levels.list[index].world.size;
        let fits = found.is_some() && size.to_array() == save.size;
        if found.is_some() && !fits && !save.voxels.is_empty() {
            self.note(&format!("the level '{}' changed its size since the save: what was blown away there is back", save.level));
        }
        let voxels = if fits { save::read_voxels(&save.voxels, size)? } else { Vec::new() };
        let mut palette = Vec::new();
        for (i, colour) in save.palette.iter().filter(|_| fits) {
            let hex = colour.strip_prefix('#').filter(|h| h.len() == 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
            match (i.parse::<usize>().ok().filter(|i| (1..256).contains(i)), hex) {
                (Some(i), Some(hex)) => palette.push((i, [0, 2, 4, 6].map(|k| u8::from_str_radix(&hex[k..k + 2], 16).expect("hex digits")))),
                _ => anyhow::bail!("the save's palette: {i} = \"{colour}\" does not read"),
            }
        }
        let rng = u64::from_str_radix(&save.rng, 16).context("the save's random generator does not read")?;
        // The states; things in them came back as nil (they live in their
        // level, which comes from its file).
        lua.set_named_registry_value("voxcade.state", value::restore(&lua, &world_state)?)?;
        lua.set_named_registry_value("voxcade.player", value::restore(&lua, &player_state)?)?;
        let all = lua.create_table()?;
        for (name, v) in &kept {
            all.raw_set(name.as_str(), value::restore(&lua, v)?)?;
        }
        lua.set_named_registry_value("voxcade.kept", all)?;
        lua.set_named_registry_value("voxcade.views", lua.create_table()?)?;
        let items = self.stage.host.sim().cfg.items.clone();
        if let Ok(inventory) = self.registry("voxcade.player")?.raw_get::<Table>("inventory") {
            let names: Vec<String> = inventory.pairs::<String, mlua::Value>().filter_map(|p| p.ok()).map(|(n, _)| n).collect();
            for name in names.into_iter().filter(|n| !items.contains_key(n)) {
                inventory.raw_set(name.as_str(), mlua::Value::Nil)?;
                self.note(&format!("the save's item '{name}' is dropped: c.items has it no more"));
            }
        }
        self.stage.visits = self.stage.levels.list.iter().map(|l| save.visits.get(&l.name).copied().unwrap_or(0)).collect();
        // The level played was entered at least once.
        self.stage.visits[index] = self.stage.visits[index].max(1);
        let sim = self.stage.host.sim_mut();
        sim.tick = save.ticks;
        sim.rng = crate::rng::Rng::from_state(rng);
        if let Some(kit) = self.kit_mut() {
            kit.set_run(&save.run);
        }
        for (key, n) in &save.numbers {
            self.stage.host.set_player_number(key, *n);
        }
        // Every seat's own back; a seat saved but away now waits for
        // whoever takes it (it comes back with it).
        let seats = self.registry("voxcade.seats")?;
        for saved in &save.seats {
            let Some(s) = self.stage.seats.get_mut(saved.seat.wrapping_sub(1)) else {
                self.note(&format!("the save's player {} is dropped: the game has {} seats (c.players)", saved.seat, self.stage.seats.len()));
                continue;
            };
            (s.out, s.team, s.kind) = (saved.out, saved.team.clone(), saved.kind.clone());
            if saved.seat > 1 {
                (s.been, s.score, s.hp) = (true, saved.score, saved.hp);
                let state = if saved.state.is_empty() { Value::Table(Vec::new()) } else { read(&saved.state, &format!("player {}'s state", saved.seat))? };
                seats.raw_set(saved.seat, value::restore(&lua, &state)?)?;
                if let Some(f) = saved.figure.as_ref().filter(|_| found.is_some()) {
                    self.seat_places.push((saved.seat, crate::levels::Place { name: "saved".into(), at: Vec3::from(f.at), facing: Some(glam::IVec2::from(f.facing)) }));
                }
            }
        }
        // The script hears every load, with the version of the game that
        // made the save (an older one it brings up to date), before its
        // level is built.
        if let Some(f) = self.globals.as_ref().map(|g| Sandbox::function(g, "on_save_loaded")).transpose()?.flatten() {
            world::set_phase(&lua, Some(world::Phase::WorldStart));
            let called = self.with_world(|sb, _| sb.call::<()>(&f, save.version, "on_save_loaded"));
            world::set_phase(&self.sandbox.lua, None);
            called?;
        }
        let place = save.figure.as_ref().filter(|_| found.is_some()).map(|f| crate::levels::Place { name: "saved".into(), at: Vec3::from(f.at), facing: Some(glam::IVec2::from(f.facing)) });
        let lights = if found.is_some() { save.lights.iter().map(|(n, b)| (n.clone(), *b)).collect() } else { Vec::new() };
        self.enter(index, place, "load", Some(Loaded { level_state, voxels, palette, lights }))
    }
}

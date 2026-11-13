//! Calling the script: the engine's numbers handed in and out around a
//! handler, a handler called, the game reopened for a scenario or a console
//! line.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// The rounds the ready-made rules spend, from the player's inventory
    /// before their tick (the game's code may have changed them).
    pub(super) fn stock_in(&mut self) -> Result<Vec<(String, u32)>> {
        let stock = self.stage.host.stock();
        if stock.is_empty() {
            return Ok(stock);
        }
        let inventory = self.registry("voxcade.player")?.raw_get::<Option<Table>>("inventory")?;
        let mut read = Vec::with_capacity(stock.len());
        for (item, _) in stock {
            let n = inventory.as_ref().map(|t| t.raw_get::<Option<i64>>(item.as_str())).transpose()?.flatten().unwrap_or(0).max(0) as u32;
            self.stage.host.set_stock(&item, n);
            read.push((item, n));
        }
        Ok(read)
    }

    /// The same for the seats after the first (`p.state.inventory`): what
    /// each seat's rules spend, read in before their tick.
    pub(super) fn seats_stock_in(&mut self) -> Result<SeatsStock> {
        let mut all = Vec::new();
        for seat in 2..=self.stage.seats.len() {
            let stock = self.stage.host.seat_stock(seat);
            if stock.is_empty() {
                continue;
            }
            let state = self.registry("voxcade.seats")?.raw_get::<Option<Table>>(seat)?;
            let inventory = state.map(|t| t.raw_get::<Option<Table>>("inventory")).transpose()?.flatten();
            let mut read = Vec::with_capacity(stock.len());
            for (item, _) in stock {
                let n = inventory.as_ref().map(|t| t.raw_get::<Option<i64>>(item.as_str())).transpose()?.flatten().unwrap_or(0).max(0) as u32;
                self.stage.host.set_seat_stock(seat, &item, n);
                read.push((item, n));
            }
            all.push((seat, read));
        }
        Ok(all)
    }

    /// What each seat's rules spent, back into its inventory.
    pub(super) fn seats_stock_out(&mut self, read: &SeatsStock) -> Result<()> {
        for (seat, read) in read {
            let now = self.stage.host.seat_stock(*seat);
            let changed: Vec<&(String, u32)> = now.iter().filter(|(item, n)| read.iter().find(|(i, _)| i == item).is_none_or(|(_, was)| was != n)).collect();
            if changed.is_empty() {
                continue;
            }
            let Some(state) = self.registry("voxcade.seats")?.raw_get::<Option<Table>>(*seat)? else { continue };
            let inventory = match state.raw_get::<Option<Table>>("inventory")? {
                Some(t) => t,
                None => {
                    let t = self.sandbox.lua.create_table()?;
                    state.raw_set("inventory", &t)?;
                    t
                }
            };
            for (item, n) in changed {
                inventory.raw_set(item.as_str(), *n)?;
            }
        }
        Ok(())
    }

    /// What the rules spent, back into the inventory after their tick
    /// (only what changed: an item the player never had stays out).
    pub(super) fn stock_out(&mut self, read: &[(String, u32)]) -> Result<()> {
        let now = self.stage.host.stock();
        let changed: Vec<&(String, u32)> = now.iter().filter(|(item, n)| read.iter().find(|(i, _)| i == item).is_none_or(|(_, was)| was != n)).collect();
        if changed.is_empty() {
            return Ok(());
        }
        let player = self.registry("voxcade.player")?;
        let inventory = match player.raw_get::<Option<Table>>("inventory")? {
            Some(t) => t,
            None => {
                let t = self.sandbox.lua.create_table()?;
                player.raw_set("inventory", &t)?;
                t
            }
        };
        for (item, n) in changed {
            inventory.raw_set(item.as_str(), *n)?;
        }
        Ok(())
    }

    /// Calls the script's handler `name` with `arg`, if it has one, the
    /// registry's phase saying where the call is (`level_end`,
    /// `world_end`: no more asks there).
    pub(super) fn call_handler(&mut self, name: &'static str, arg: &'static str, phase: Option<&str>) -> Result<()> {
        let Some(f) = self.globals.as_ref().map(|g| Sandbox::function(g, name)).transpose()?.flatten() else { return Ok(()) };
        let lua = &self.sandbox.lua;
        match phase {
            Some(phase) => lua.set_named_registry_value("voxcade.phase", phase)?,
            None => lua.unset_named_registry_value("voxcade.phase")?,
        }
        let called = if name == "on_level_start" { self.with_world(|sb, _| sb.call_start::<()>(&f, arg)) } else { self.with_world(|sb, _| sb.call::<()>(&f, arg, name)) };
        self.sandbox.lua.unset_named_registry_value("voxcade.phase")?;
        called
    }

    /// Does what the script asked at the tick's end (`docs/game-model.md`,
    /// "Leaving a level"): `on_level_end(why)`, then another level entered
    /// (or this one afresh) and `on_level_start(how)`, or the run's end. An
    /// ask in that `on_level_start` counts at once; a mistake stops the
    /// game.
    /// Runs `f` with the game's files open to `require` again, in the
    /// game's environment (its globals frozen), `require` giving the
    /// modules it has. What a scenario requires anew counts as the game's
    /// code (`code`: its replays run from it); what a console line does
    /// not (the line itself is in the replay, and the fingerprint is
    /// checked before the first tick).
    pub(super) fn reopened<R>(&mut self, counts: bool, f: impl FnOnce(&mut Self, &Table) -> Result<R>) -> Result<R> {
        let env = self.env.clone().context("the game has not started (mend main.lua first)")?;
        *self.sandbox.reading.borrow_mut() = sandbox::Reading::reopened(&self.files, env.clone(), self.required.clone());
        let done = f(self, &env);
        let mut reading = self.sandbox.reading.borrow_mut();
        self.required = reading.given();
        let (modules, read) = (std::mem::take(&mut reading.modules), std::mem::take(&mut reading.read));
        reading.close();
        drop(reading);
        if counts {
            for file in modules.into_iter().chain(read) {
                if !self.modules.iter().chain(&self.data).any(|(name, _)| *name == file.0) {
                    self.modules.push(file);
                }
            }
        }
        done
    }

    /// What a tick's end does, after a scenario's setup or a console line:
    /// what the script removed goes, the states are checked (one that is
    /// no plain data stops the game), the attacks asked for start, saves
    /// are written and an ask is carried out.
    pub(super) fn settle_between(&mut self, after: &str) -> Result<()> {
        self.stage.settle();
        if let Err(e) = self.check_states() {
            let text = format!("{e:#} (after {after})");
            self.halt(anyhow::anyhow!(text.clone()));
            self.state_failed.set(true);
            anyhow::bail!(text);
        }
        self.stage.host.start_attacks();
        self.write_saves();
        self.change_levels();
        match self.halted() {
            Some(e) => anyhow::bail!(e),
            None => Ok(()),
        }
    }
}

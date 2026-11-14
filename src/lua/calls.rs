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
        let read = counted(Some(&self.registry("voxcade.player")?), stock)?;
        for (item, n) in &read {
            self.stage.host.set_stock(item, *n);
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
            let read = counted(self.registry("voxcade.seats")?.raw_get::<Option<Table>>(seat)?.as_ref(), stock)?;
            for (item, n) in &read {
                self.stage.host.set_seat_stock(seat, item, *n);
            }
            all.push((seat, read));
        }
        Ok(all)
    }

    /// What each seat's rules spent, back into its inventory.
    pub(super) fn seats_stock_out(&mut self, read: &SeatsStock) -> Result<()> {
        for (seat, read) in read {
            let now = self.stage.host.seat_stock(*seat);
            let changed = changed(read, &now);
            if changed.is_empty() {
                continue;
            }
            if let Some(state) = self.registry("voxcade.seats")?.raw_get::<Option<Table>>(*seat)? {
                self.write_back(&state, &changed)?;
            }
        }
        Ok(())
    }

    /// What the rules spent, back into the inventory after their tick
    /// (only what changed: an item the player never had stays out).
    pub(super) fn stock_out(&mut self, read: &[(String, u32)]) -> Result<()> {
        let now = self.stage.host.stock();
        let changed = changed(read, &now);
        if changed.is_empty() {
            return Ok(());
        }
        self.write_back(&self.registry("voxcade.player")?, &changed)
    }

    /// The counts the rules changed, into `state`'s inventory (made if it
    /// has none).
    fn write_back(&self, state: &Table, changed: &[&(String, u32)]) -> Result<()> {
        let inventory = world::table_at(&self.sandbox.lua, state, "inventory")?;
        for (item, n) in changed {
            inventory.raw_set(item.as_str(), *n)?;
        }
        Ok(())
    }

    /// Calls the script's handler `name` with `arg`, if it has one, the
    /// phase saying where the call is (the level being left: no more asks
    /// there).
    pub(super) fn call_handler(&mut self, name: &'static str, arg: &'static str, phase: Option<world::Phase>) -> Result<()> {
        let Some(f) = self.globals.as_ref().map(|g| Sandbox::function(g, name)).transpose()?.flatten() else { return Ok(()) };
        world::set_phase(&self.sandbox.lua, phase);
        let called = if name == "on_level_start" { self.with_world(|sb, _| sb.call_start::<()>(&f, arg)) } else { self.with_world(|sb, _| sb.call::<()>(&f, arg, name)) };
        world::set_phase(&self.sandbox.lua, None);
        called
    }

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

/// How many of each of `stock`'s items `state`'s inventory holds (none: 0).
fn counted(state: Option<&Table>, stock: Vec<(String, u32)>) -> Result<Vec<(String, u32)>> {
    let inventory = state.map(|t| t.raw_get::<Option<Table>>("inventory")).transpose()?.flatten();
    let mut read = Vec::with_capacity(stock.len());
    for (item, _) in stock {
        let n = inventory.as_ref().map(|t| t.raw_get::<Option<i64>>(item.as_str())).transpose()?.flatten().unwrap_or(0).max(0) as u32;
        read.push((item, n));
    }
    Ok(read)
}

/// The counts that are not as they were `read`.
fn changed<'a>(read: &[(String, u32)], now: &'a [(String, u32)]) -> Vec<&'a (String, u32)> {
    now.iter().filter(|(item, n)| read.iter().find(|(i, _)| i == item).is_none_or(|(_, was)| was != n)).collect()
}

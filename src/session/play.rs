//! Ticking the game: its run, input and seats, snapshots and saves,
//! scenarios and the console, a level change's fade.

use super::*;

impl<G: Game> Session<G> {
    /// Whether a level change is being shown (no ticks run; the pause menu
    /// waits for it).
    pub fn fading(&self) -> bool {
        self.fade.is_some()
    }

    /// Whether a change the game asked for waits for its fade out: the
    /// game still stands as it was before it (a recorder's check waits).
    pub fn changing(&self) -> bool {
        self.fade.as_ref().is_some_and(|f| f.out)
    }

    /// Does the change the game asked for; every chunk the level before
    /// showed is drawn anew (the next level's chunks are others). A load
    /// reads its slot now (a replay's: the save it loaded then).
    pub(super) fn change(&mut self) {
        let shown = self.game.sim().world.clone();
        self.game.change();
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.enter_level();
        // A save asked for in on_level_start.
        self.saves_due();
    }

    /// Writes the saves the game made (`world:save`) into their slots.
    pub(super) fn saves_due(&mut self) {
        let due = self.game.saving();
        if due.is_empty() {
            return;
        }
        for (slot, text) in due {
            if let Err(e) = self.slots.write(slot, &text, self.clock) {
                eprintln!("{e:#}");
                self.problem = Some(format!("{e:#}"));
            }
        }
        self.game.set_slots(self.slots.listing(), self.slots.texts());
    }

    /// Saves the game into `slot` now, between two ticks (SAVE in the
    /// pause menu); not a game stopped by a mistake, half way through.
    pub fn save_to(&mut self, slot: u32) -> Result<()> {
        if let Some(e) = self.game.halted() {
            anyhow::bail!("the game stopped with a mistake, nothing to save: {e}");
        }
        let text = self.game.save()?;
        self.slots.write(slot, &text, self.clock)?;
        self.game.set_slots(self.slots.listing(), self.slots.texts());
        self.pending.push(Event::Save { slot, when: self.clock });
        Ok(())
    }

    /// The run goes on from a save at once, between two ticks: LOAD in the
    /// pause menu (on a game loaded afresh, as its replay plays it), a
    /// replay that began from a save.
    pub fn start_from(&mut self, text: &str) -> Result<()> {
        let shown = self.game.sim().world.clone();
        self.game.start_from(text)?;
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.enter_level();
        self.started_from = Some(text.to_string());
        self.saves_due();
        Ok(())
    }

    /// The run goes on from the scenario `name` at once, before its first
    /// tick (`docs/game-model.md`, "Scenarios and the console").
    pub fn start_scenario(&mut self, name: &str) -> Result<()> {
        let shown = self.game.sim().world.clone();
        let started = self.game.start_scenario(name);
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.enter_level();
        started?;
        self.scenario = Some(name.to_string());
        self.saves_due();
        Ok(())
    }

    /// A console line run between two ticks: what it gave back and
    /// printed. It is part of the run (`evals`: a replay runs it again
    /// after the same step), a mistake in it too.
    pub fn eval(&mut self, code: &str) -> Result<String> {
        let shown = self.game.sim().world.clone();
        let said = self.game.eval(code);
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.enter_level();
        self.evals.push(code.to_string());
        self.pending.push(Event::Eval(code.to_string()));
        self.saves_due();
        said
    }

    /// Whether the run was helped: begun from a scenario, a console line
    /// run in it, files changed in it (APPLY). Its high score goes in as
    /// `HELPED` (no initials asked).
    pub fn helped(&self) -> bool {
        self.scenario.is_some() || !self.evals.is_empty() || self.applied
    }

    /// What the game's script printed since last asked.
    pub fn printed(&mut self) -> Vec<String> {
        self.game.printed()
    }

    /// What stops the game: a script's error (its file and line in it),
    /// or a file that would not load on a reload.
    pub fn trouble(&self) -> Option<String> {
        self.game.halted().or_else(|| self.problem.clone())
    }

    pub fn actions(&self) -> &[&'static str] {
        &self.actions
    }

    /// What the player plays with as the game sees it: nobody as it loads,
    /// then the window's player (`player_controls`) or a replay's.
    pub fn controls(&self) -> &Controls {
        &self.controls
    }

    /// What the player plays with from now on: the game sees it in the
    /// next tick (`controls` in Lua) and a recording notes the change. A
    /// window sets its player's before every tick it plays; a replay sets
    /// what it recorded.
    pub fn set_controls(&mut self, controls: Controls) {
        if controls != self.controls {
            self.game.set_controls(&controls);
            self.controls = controls;
        }
    }

    /// The seats after the first (seat 2 first) as the next tick finds them.
    pub fn seats(&self) -> &[crate::game::Seat] {
        &self.seats
    }

    /// Who sits on seat `seat` (2 and on) for the next tick and what they
    /// hold (None: nobody, the seat free); a seat the game does not have
    /// is no mistake, nobody sits there.
    pub fn set_seat(&mut self, seat: usize, input: Option<crate::input::Input>) {
        if let Some(s) = seat.checked_sub(2).and_then(|i| self.seats.get_mut(i)) {
            s.input = input;
        }
    }

    /// What the player on seat `seat` (2 and on) plays with.
    pub fn set_seat_controls(&mut self, seat: usize, controls: Controls) {
        if let Some(s) = seat.checked_sub(2).and_then(|i| self.seats.get_mut(i)) {
            s.controls = controls;
        }
    }

    /// The language the game speaks to the player: theirs of the game's
    /// (`c.languages`), else its first, else English.
    pub fn language(&self) -> String {
        let speaks = &self.cfg.languages;
        self.controls.language.as_ref().filter(|l| speaks.contains(l)).or(speaks.first()).cloned().unwrap_or_else(|| "en".into())
    }

    /// The player's controls: `device`, a pad printing its buttons as
    /// `pad_style` does, the keys as `settings` bind and label them.
    pub fn player_controls(&self, device: crate::controls::Device, pad_style: crate::controls::PadStyle, settings: &Settings) -> Controls {
        Controls::of(&self.actions, device, pad_style, settings)
    }

    /// The keys and gamepad buttons for its actions to start with.
    pub fn default_keys(&self) -> crate::controls::Bindings {
        G::default_keys(&self.cfg)
    }

    pub fn default_pad(&self) -> crate::controls::Bindings {
        G::default_pad(&self.cfg)
    }

    /// Keeps the game's whole state as it stands, to bring it back later
    /// (`restore`): going back in a replay, rollback when playing over a
    /// network, saving. Cheap: the copy shares the world's chunks until one
    /// side writes to them.
    pub fn snapshot(&self) -> Result<Snapshot<G>> {
        Ok(Snapshot {
            kept: self.game.snapshot()?,
            slots: self.slots.clone(),
            controls: self.controls.clone(),
            seats: self.seats.clone(),
            pending: self.pending.clone(),
            played: self.played.clone(),
            inputs: self.inputs,
            clock: self.clock,
            files: self.files.clone(),
        })
    }

    /// Brings a kept state back (as often as wanted); what differs from the
    /// world drawn now is meshed anew. The sounds of the tick it was kept at
    /// are not played again.
    pub fn restore(&mut self, snapshot: &Snapshot<G>) {
        // Files applied since: those of then taken in again first (the
        // code, the levels), then the state on them.
        let mut differing = self.files.differing(&snapshot.files);
        if !differing.is_empty() {
            self.files = snapshot.files.clone();
            differing.sort_by_key(|name| self.apply_order(name));
            let touched = self.touched;
            for name in differing {
                let path = self.files.path(&name);
                if let Err(e) = self.take_in(&path) {
                    eprintln!("restoring {name}: {e:#}");
                }
            }
            self.touched = touched;
        }
        let shown = self.game.sim().world.clone();
        self.game.restore(&snapshot.kept);
        self.game.sim_mut().world.mark_changed_since(&shown);
        self.slots = snapshot.slots.clone();
        self.controls = snapshot.controls.clone();
        self.game.set_controls(&self.controls);
        self.seats = snapshot.seats.clone();
        self.pending = snapshot.pending.clone();
        self.played = snapshot.played.clone();
        (self.inputs, self.clock) = (snapshot.inputs, snapshot.clock);
        self.game.set_local(&self.local);
        self.enter_level();
    }

    /// The world, to build its meshes from.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.game.sim_mut().world
    }

    /// An error when the game stopped (a script's error): tools without a
    /// window end with it.
    pub fn stopped(&self) -> Result<()> {
        match self.game.halted() {
            Some(e) => anyhow::bail!("the game stopped at tick {}: {e}", self.game.sim().tick),
            None => Ok(()),
        }
    }

    /// One tick of the game, and its own song when it picks another. A
    /// change the game asked for at the tick's end (another level, the
    /// run's end) is done right after it, or, where changes fade, between
    /// the fade out and the fade in, none of whose frames is a tick.
    pub fn step(&mut self, input: &crate::input::Input) {
        if let Some(fade) = &mut self.fade {
            self.ticked = false;
            // The song plays on through the fade: its clock goes on.
            self.game.music_step();
            fade.t += crate::sim::DT;
            if fade.t >= fade.length {
                if fade.out {
                    (fade.out, fade.t) = (false, 0.0);
                    self.change();
                } else {
                    (self.fade, self.fade_cut) = (None, false);
                }
            }
            self.notice_song();
            return;
        }
        self.ticked = true;
        if !self.seats.is_empty() {
            self.game.set_seats(&self.seats);
        }
        self.played = Tick { events: std::mem::take(&mut self.pending), when: self.clock, controls: self.controls.clone(), input: *input, seats: self.seats.clone() };
        self.played.hash_into(&mut self.inputs);
        self.game.step(input);
        self.saves_due();
        if let Some(why) = self.game.leaving() {
            let length = self.cfg.transition.length(why);
            if self.fades && length > 0.0 {
                self.fade = Some(Fade { out: true, t: 0.0, length, color: self.cfg.transition.rgb() });
                self.fade_cut = false;
            } else if self.fade_clock && length > 0.0 {
                // A replay of a run that faded: the fade's steps on the
                // music's clock, out and in, as the window took them.
                let steps = fade_steps(length);
                (0..steps).for_each(|_| self.game.music_step());
                self.change();
                (0..steps).for_each(|_| self.game.music_step());
            } else {
                self.change();
            }
        }
        // A level entered within the tick (an ask its code made carried
        // out at once) is seen to as one changed after it.
        self.enter_level();
        self.notice_song();
    }

    /// The game's own song changed (`music:play`): it begins anew, over the
    /// crossfade asked (or the game's), silent to the end of a fade being
    /// shown without one.
    pub(super) fn notice_song(&mut self) {
        let cue = self.game.music().map(|c| c.song);
        if cue != self.cue {
            self.cue = cue;
            self.music_changed = true;
            let seconds = self.game.music_switch().began.unwrap_or(self.cfg.audio.crossfade);
            self.crossfade = Some(seconds);
            self.fade_cut |= self.fade.is_some() && seconds <= 0.0;
        }
    }

    /// Whether the music is silent for a level's fade: a song is to change
    /// (or changed) in it without a crossfade, the short pause between
    /// the two (a song going on plays through).
    pub(super) fn fade_hold(&self) -> bool {
        let cut = self.game.music_switch().pending.is_some_and(|c| c.unwrap_or(self.cfg.audio.crossfade) <= 0.0);
        self.fade.is_some() && (self.fade_cut || cut)
    }

    /// Plays a tick brought whole (`Tick`: over the network, a test): its
    /// events, then the tick with its seats, controls and time. A level
    /// change's fade takes no tick, so a session that fades lets it pass
    /// (`step` while `fading`) before it plays the next.
    pub fn play(&mut self, tick: &Tick) {
        for event in &tick.events {
            match event {
                Event::Save { slot, when } => {
                    self.clock = *when;
                    if let Err(e) = self.save_to(*slot) {
                        eprintln!("saving into slot {slot}: {e:#}");
                    }
                }
                Event::Eval(code) => {
                    let _ = self.eval(code);
                }
                Event::Apply(changes) => {
                    if let Err(e) = self.apply(changes.clone()) {
                        eprintln!("applying files: {e:#}");
                    }
                }
            }
        }
        self.clock = tick.when;
        self.set_controls(tick.controls.clone());
        for (n, seat) in tick.seats.iter().enumerate() {
            self.set_seat(n + 2, seat.input);
            self.set_seat_controls(n + 2, seat.controls.clone());
        }
        self.step(&tick.input);
    }

    /// The last tick played, with what happened before it.
    pub fn played(&self) -> &Tick {
        &self.played
    }

    /// The saves and console lines since the last tick: the next one's
    /// events.
    pub fn pending(&self) -> &[Event] {
        &self.pending
    }

    /// The time saves are stamped with now.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// The slots hold these saves from now on, for this run only (a guest
    /// of a network game keeps the host's, `docs/multiplayer.md`).
    pub fn adopt_slots(&mut self, texts: BTreeMap<u32, String>) {
        let count = self.slots.count();
        self.slots = Slots::holding(texts);
        self.slots.set_count(count);
        self.game.set_slots(self.slots.listing(), self.slots.texts());
    }

    /// The time saves are stamped with from now on (seconds since 1970):
    /// the window's clock where someone plays, the host's over the network,
    /// so every machine writes the same save.
    pub fn set_clock(&mut self, when: u64) {
        self.clock = when;
    }

    /// The state hash by its parts (`Game::state_sections`) and, last, the
    /// ticks played (`inputs`): where two machines ran apart, and whether
    /// they played the same.
    pub fn sections(&self) -> Vec<(&'static str, u64)> {
        let mut parts = self.game.state_sections();
        parts.push(("inputs", self.inputs.finish()));
        parts
    }

    /// How a run of the game begins, as one number: the world's voxels and
    /// the state's parts (`sections` but the ticks played), taken before
    /// any tick (read-only; the IDE's play compares it after a reload).
    pub fn start_mark(&self) -> u64 {
        let mut h = crate::hash::Stable::default();
        h.u64(self.game.sim().world.content_hash());
        for (name, part) in self.game.state_sections() {
            h.bytes(name.as_bytes()).u64(part);
        }
        h.finish()
    }

    /// What this machine has of the game: its seats and its player's
    /// language (`game::Local`; at one machine all of them, the default).
    pub fn set_local(&mut self, local: crate::game::Local) {
        if local != self.local {
            self.game.set_local(&local);
            self.local = local;
        }
    }

    pub fn local(&self) -> &crate::game::Local {
        &self.local
    }

    /// The lines this machine's players give its high score table: its own
    /// seats', or the team's line (`c.team_score`) at every machine.
    pub fn local_scores(&self) -> Vec<(usize, u32)> {
        let team = self.cfg.team_score;
        self.game.scores().into_iter().filter(|&(seat, _)| team || self.local.has(seat)).collect()
    }
}

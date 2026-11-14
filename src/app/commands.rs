//! What the menus (and the keys around them) asked for, done: starting,
//! hosting and joining, saving and loading, restarting and going back to
//! the title (the high score table on the way), the settings applied.

use std::time::Instant;

use anyhow::{Context, Result};

use super::{App, Flow, apply_settings, begun, next_initials, pause_menu, planned, play_the_games_song, title_menu, trouble};
use crate::game::Game;
use crate::menu::{Command, Menu, Page};
use crate::replay::{Playback, replay_session};
use crate::saves::Slots;
use crate::session::Session;

impl<G: Game> App<G> {
    /// Each of `commands` in turn; QUIT ends the window at once.
    pub(super) fn commands(&mut self, commands: Vec<Command>) -> Result<Flow> {
        for command in commands {
            if self.command(command)? == Flow::Quit {
                return Ok(Flow::Quit);
            }
        }
        Ok(Flow::Go)
    }

    fn command(&mut self, command: Command) -> Result<Flow> {
        match command {
            Command::Quit => return Ok(Flow::Quit),
            Command::Start => {
                if !self.begin.is_empty() {
                    begun(&self.begin, &mut self.game);
                    self.renderer.rebuild_all(&self.screen.device, &self.screen.queue, self.game.world_mut(), &self.mask);
                }
                self.runs.start(&self.game);
                self.seat_the_players();
                self.front.glide_in();
                play_the_games_song(&mut self.game, &self.settings);
            }
            command @ (Command::Host | Command::HostNet) => {
                // As START, the game open to the network at once: the
                // LAN, or the internet (its ticket on the clipboard).
                if !self.begin.is_empty() {
                    begun(&self.begin, &mut self.game);
                    self.renderer.rebuild_all(&self.screen.device, &self.screen.queue, self.game.world_mut(), &self.mask);
                }
                self.game.set_clock(crate::saves::now());
                let hosted = if command == Command::HostNet { crate::network::Network::host_net(&self.game) } else { crate::network::Network::host(&self.game, self.host_port) };
                match hosted {
                    Ok(n) => {
                        // Across the internet the ticket goes to the
                        // clipboard once the link is there (`net_send`).
                        println!("hosting {} at {}", self.game.cfg.title, n.found().join(", "));
                        self.net = Some(n);
                    }
                    Err(e) => self.parting = Some((format!("{e:#}").to_uppercase(), Instant::now())),
                }
                self.runs.start(&self.game);
                self.seat_the_players();
                self.front.glide_in();
                play_the_games_song(&mut self.game, &self.settings);
            }
            Command::Join => {
                // An address typed is kept for next time; a ticket is for
                // one game.
                let address = self.front.menu.address.trim().to_string();
                if !crate::net::link::Link::is_net(&address) {
                    self.settings.address = address.clone();
                    self.save_settings();
                }
                self.join(crate::network::Network::join(&self.dir, &address, self.game.cfg.players, crate::network::app_of(&self.game).ok()));
            }
            Command::JoinCode => self.join(crate::network::Network::join(&self.dir, &self.front.menu.code, self.game.cfg.players, crate::network::app_of(&self.game).ok())),
            Command::JoinAny => self.join(crate::network::app_of(&self.game).and_then(|(app, version)| crate::network::Network::join_any(&self.dir, self.game.cfg.players, app, version))),
            Command::Disconnect => {
                if let Some(mut n) = self.net.take() {
                    n.leave();
                }
                self.front.menu = title_menu(&self.game);
            }
            Command::PasteTicket => match self.video.clipboard().clipboard_text() {
                Ok(text) if crate::net::link::Link::is_ticket(&text) => {
                    self.front.menu.address = text.trim().to_string();
                    self.front.menu.stack.truncate(1);
                    self.front.menu.stack.push((Page::Joining, 0));
                    self.queued.push(Command::Join);
                }
                Ok(text) if crate::net::link::Link::is_code(&text) => {
                    self.front.menu.code = matchmaker_protocol::tidy(&text).unwrap_or_default();
                    self.front.menu.stack.truncate(1);
                    self.front.menu.stack.push((Page::Joining, 0));
                    self.queued.push(Command::JoinCode);
                }
                _ => self.parting = Some((crate::lang::say(&self.settings.language(), "NO TICKET OR CODE ON THE CLIPBOARD").to_string(), Instant::now())),
            },
            Command::CopyCode => {
                if let Some(n) = &mut self.net
                    && let Some(code) = n.game_code().map(str::to_string)
                    && self.video.clipboard().set_clipboard_text(&code).is_ok()
                {
                    n.say(crate::lang::say(&self.settings.language(), "CODE COPIED: SEND IT").to_string());
                }
            }
            Command::CopyTicket => {
                if let Some(n) = &mut self.net
                    && let Some(ticket) = n.ticket().map(str::to_string)
                    && self.video.clipboard().set_clipboard_text(&ticket).is_ok()
                {
                    n.say(crate::lang::say(&self.settings.language(), "TICKET COPIED: SEND IT").to_string());
                }
            }
            Command::LetIn(peer) => {
                if let Some(n) = &mut self.net {
                    n.let_in(peer);
                }
                if let Some((_, cursor)) = self.front.menu.stack.last_mut() {
                    *cursor = 0;
                }
            }
            Command::Drop(machine) => {
                if let Some(crate::network::Side::Host(h)) = self.net.as_mut().map(|n| &mut n.side) {
                    h.host.drop_machine(machine);
                }
                if let Some((_, cursor)) = self.front.menu.stack.last_mut() {
                    *cursor = 0;
                }
            }
            Command::Resume => self.front.menu = Menu::default(),
            // The players page (CONTROLS in a game of several): the run's
            // seats while playing, else the plan the next run begins with;
            // the settings keep it.
            Command::SeatKeys(..) | Command::SeatPad(..) | Command::Swap => {
                let named = self.pads.named();
                let mut seats = if self.front.playing { self.seating.clone() } else { planned(&self.settings, &named, self.game.cfg.players) };
                let mut chosen = None;
                match command {
                    Command::SeatKeys(seat, step) => seats.set_keyboard(seat, seats.keys(seat).step(step)),
                    Command::SeatPad(seat, step) => {
                        let choices: Vec<Option<u32>> = std::iter::once(None).chain(named.iter().map(|(id, _)| Some(*id))).collect();
                        seats.set_pad(seat, crate::settings::cycle(&choices, seats.pad(seat), step));
                        chosen = Some(seat);
                    }
                    _ => seats.swap(1, 2),
                }
                self.settings.players = crate::seating::plan_of(&seats, &named, &self.settings.players, chosen);
                self.save_settings();
                if self.front.playing {
                    self.seating = seats;
                }
            }
            Command::Leave(seat) => {
                // A guest's first player gives up its seat and watches.
                match &mut self.net {
                    Some(n) if seat == 1 => n.watching = true,
                    _ => self.seating.leave(seat),
                }
                println!("player {seat} leaves");
                self.front.menu = Menu::default();
            }
            Command::Save(slot) => {
                self.game.set_clock(crate::saves::now());
                if let Err(e) = self.game.save_to(slot) {
                    eprintln!("saving into slot {slot}: {e:#}");
                    self.parting = Some(trouble(&self.settings.language(), "COULD NOT SAVE: {}", &e));
                }
                // Back on the pause page, the slot's new line in it.
                self.front.menu.slots = pause_menu(&self.game, true).slots;
                self.front.menu.stack.pop();
            }
            // The host's LOAD: the save goes to every guest first, then
            // the run starts from it for everyone at one tick.
            Command::Load(slot) if self.net.as_ref().is_some_and(|n| n.hosting()) => {
                if let Some(crate::network::Side::Host(h)) = self.net.as_mut().map(|n| &mut n.side)
                    && let Err(e) = h.load(&self.game, slot)
                {
                    eprintln!("loading slot {slot}: {e:#}");
                    self.parting = Some(trouble(&self.settings.language(), "COULD NOT LOAD: {}", &e));
                }
                self.front.menu = Menu::default();
            }
            Command::Load(slot) => self.load(slot),
            Command::Entered => {
                let rank = self.settings.enter(&self.front.menu.entry.initials(), self.front.menu.entry.score);
                self.save_settings();
                // The next player's initials, if another got in.
                self.front.menu = next_initials(&mut self.front.entering, &self.settings).unwrap_or_else(|| Menu::high_scores(rank));
            }
            Command::Title => {
                self.front.menu = title_menu(&self.game);
                (self.front.table_in_round, self.front.table_shown) = (false, false);
            }
            // Over the network F5 starts the run anew for everyone, from
            // the host; a guest's F5 does nothing.
            Command::Restart if self.net.is_some() => {
                if let Some(n) = &mut self.net
                    && let crate::network::Side::Host(h) = &mut n.side
                    && let Err(e) = h.restart(&self.game)
                {
                    n.say(format!("{e:#}").to_uppercase());
                }
                self.front.menu = Menu::default();
            }
            Command::Restart | Command::ToTitle => self.leave_the_run(command)?,
            Command::Changed | Command::Fullscreen | Command::Song => {
                crate::controls::label_keys(&mut self.settings);
                if command == Command::Fullscreen {
                    self.screen.window.set_fullscreen(self.settings.fullscreen).context("toggling fullscreen")?;
                }
                if command == Command::Song && self.front.playing {
                    self.game.select_song(self.settings.song);
                }
                apply_settings(&mut self.game, &mut self.settings);
                let crate::window::Screen { surface, device, config, .. } = &mut self.screen;
                crate::window::set_frame_rate(surface, device, config, &self.adapter, self.settings.frame_rate);
                self.save_settings();
            }
        }
        Ok(Flow::Go)
    }

    /// A network game joined, or why not (back on the title).
    fn join(&mut self, joined: Result<crate::network::Network<G>>) {
        match joined {
            Ok(n) => self.net = Some(n),
            Err(e) => {
                self.parting = Some((format!("{e:#}").to_uppercase(), Instant::now()));
                self.front.menu = title_menu(&self.game);
            }
        }
    }

    /// Going on from a save is a run of its own: the game as loaded
    /// afresh, then the save (as its replay does it). One that does not
    /// load leaves the run as it was, paused.
    fn load(&mut self, slot: u32) {
        let text = self.game.slots.read(slot).map(str::to_string).filter(|_| self.watching.is_none());
        let loaded = text.map(|text| Session::<G>::load_with(&self.dir, Slots::on_disk(&self.dir)).and_then(|mut fresh| fresh.start_from(&text).map(|()| fresh)));
        match loaded {
            Some(Ok(fresh)) => {
                // From the title (CONTINUE) no run was going.
                if self.front.playing {
                    self.runs.end(&self.game);
                }
                self.front.title_music = false;
                self.renew(fresh);
                play_the_games_song(&mut self.game, &self.settings);
                self.runs.start(&self.game);
                self.front.menu = Menu::default();
                self.front.playing = true;
            }
            Some(Err(e)) => {
                eprintln!("loading slot {slot}: {e:#}");
                self.parting = Some(trouble(&self.settings.language(), "COULD NOT LOAD: {}", &e));
                self.front.menu = if self.front.playing { pause_menu(&self.game, true) } else { title_menu(&self.game) };
            }
            None => self.front.menu = if self.front.playing { pause_menu(&self.game, self.watching.is_none()) } else { title_menu(&self.game) },
        }
    }

    /// RESTART (F5) or back to the title: a network game left, its run
    /// kept; the run ended; the game loaded afresh. Begun again, or on the
    /// title, a run left (its end, or quit from the pause) going into the
    /// high score table if it gets in: its initials first.
    fn leave_the_run(&mut self, command: Command) -> Result<()> {
        if let Some(mut n) = self.net.take() {
            n.keep_run(&self.game, &self.runs);
            n.leave();
        }
        let watching = self.watching.is_some();
        let left = (command == Command::ToTitle && !watching && self.game.game.scored()).then(|| self.game.local_scores());
        let helped = self.game.helped();
        if command == Command::ToTitle && !watching && self.game.game.outcome().is_none() {
            self.game.game.quit();
        }
        if !watching {
            self.runs.end(&self.game);
        }
        let fresh = match (&self.watching, command) {
            (Some(replay), Command::Restart) => replay_session::<G>(&self.dir, replay),
            _ => Session::<G>::load_with(&self.dir, Slots::on_disk(&self.dir)),
        };
        match fresh {
            Ok(fresh) => self.renew(fresh),
            Err(e) => eprintln!("reloading the game failed: {e:#}"),
        }
        if command == Command::Restart {
            match &self.watching {
                Some(replay) => self.playback = Some(Playback::new(replay, &mut self.game)?),
                None => {
                    if !self.begin.is_empty() {
                        begun(&self.begin, &mut self.game);
                        self.renderer.rebuild_all(&self.screen.device, &self.screen.queue, self.game.world_mut(), &self.mask);
                    }
                    self.runs.start(&self.game);
                }
            }
            self.front.straight_in();
            play_the_games_song(&mut self.game, &self.settings);
        } else {
            let menu = match left {
                Some(lines) if helped => {
                    // The console or a scenario helped: its lines go in
                    // as GOD, no initials asked.
                    let mut rank = None;
                    for &(_, score) in &lines {
                        rank = self.settings.enter(crate::session::HELPED, score).or(rank);
                    }
                    self.save_settings();
                    Menu::high_scores(rank)
                }
                Some(lines) => {
                    // Each player whose score gets in, in turn.
                    let several = lines.len() > 1;
                    self.front.entering = lines.into_iter().map(|(seat, score)| (several.then_some(seat), score)).collect();
                    next_initials(&mut self.front.entering, &self.settings).unwrap_or_else(|| Menu::high_scores(None))
                }
                None => title_menu(&self.game),
            };
            self.front.back_to_title(menu);
            self.game.music_changed = false;
            if let Some(mut mixer) = self.speaker.as_mut().and_then(|s| s.lock()) {
                mixer.set_music(self.game.title_music());
            }
        }
        Ok(())
    }
}

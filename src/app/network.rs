//! A network game in the window (`docs/multiplayer.md`): what came in, a
//! guest catching up, running apart or left; this machine's players' keys
//! sent; a guest playing the host's bundles; the ticket and the code on the
//! clipboard.

use super::{App, CATCH_UP_BUDGET, MAX_CATCH_UP, failure, pads_of, play_the_games_song, renew, say_now, seat_controls, seated, tick_sounds, title_menu};
use crate::game::Game;
use crate::menu::{Command, Page};
use crate::saves::Slots;
use crate::session::Session;

impl<G: Game> App<G> {
    /// What the network brought: a host's link that would not start ends
    /// its network game; a guest plays the run so far through, then its
    /// game is the one shown; a guest's game run apart writes its report
    /// (and joins again once, where only what it was sent differed); a
    /// guest's game ended says why.
    pub(super) fn network(&mut self, commands: &mut Vec<Command>) {
        let App { net, game, settings, display, seating, last_pad, pads, front, runs, parting, queued, rejoined, dir, .. } = self;
        // The host's link across the internet would not start: its network
        // game is over, the game plays on alone.
        if let Some(why) = net.as_mut().and_then(|n| n.broken()) {
            *parting = say_now(why.to_uppercase());
            *net = None;
        }
        let Some(n) = net.as_mut() else { return };
        if let Err(e) = n.receive(game, settings.language()) {
            eprintln!("the network game: {e:#}");
        }
        if let crate::network::Side::Guest(j) = &mut n.side {
            match j.catch_up(CATCH_UP_BUDGET) {
                Ok(Some(caught)) => {
                    println!("caught up with the host at tick {}", caught.game.sim().tick);
                    renew(game, caught, settings, display);
                    game.fades = true;
                    *seating = seated(game.cfg.players, *last_pad, front, settings, pads);
                    front.straight_in();
                    play_the_games_song(game, settings);
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = j.end(&failure(&e));
                }
            }
        }
        // A guest's game run apart from the host's: a report written,
        // and where only what it was sent differed (the network's
        // fault), it joins again by itself, once.
        let mut rejoin = false;
        if let crate::network::Side::Guest(j) = &mut n.side
            && let crate::net::play::Joined::Apart(apart) = j.state()
        {
            match j.report(game, &apart) {
                Ok(at) => at.iter().for_each(|at| println!("the desync's report: {}", at.display())),
                Err(e) => eprintln!("the desync's report: {e:#}"),
            }
            let line = crate::network::desync_line(&apart, "", &j.host_commit);
            let _ = j.end(&line);
            rejoin = apart.inputs && !*rejoined;
            *rejoined |= rejoin;
        }
        if let crate::network::Side::Guest(j) = &n.side
            && let crate::net::play::Joined::Gone(why) = j.state()
        {
            println!("the network game ended: {why}");
            n.keep_run(game, runs);
            *parting = say_now(crate::lang::say_line(settings.language(), &why));
            *net = None;
            if rejoin {
                // Back to the title's game, joining at once.
                if let Ok(fresh) = Session::<G>::load_with(dir, Slots::on_disk(dir)) {
                    renew(game, fresh, settings, display);
                }
                (front.playing, front.intro, front.title_time) = (false, None, 0.0);
                front.menu = title_menu(game);
                front.menu.address = settings.address.clone();
                front.menu.stack.push((Page::Joining, 0));
                queued.push(Command::Join);
            } else if front.playing {
                commands.push(Command::ToTitle);
            } else {
                front.menu = title_menu(game);
            }
        }
    }

    /// A network game: this machine's players' keys and controls go to the
    /// host (the host's own held back there), the seats they sit on asked
    /// for and given up; what the pictures show of them is theirs.
    pub(super) fn net_input(&mut self, keyboard: &sdl3::keyboard::KeyboardState) {
        let App { net, front, settings, seating, pads, game, .. } = self;
        if let Some(n) = net
            && front.playing
        {
            let lang = settings.language();
            for local in n.settle(|l| seating.taken(l)) {
                if local > 1 {
                    seating.leave(local);
                } else {
                    n.watching = true;
                }
                n.say(crate::lang::say(lang, "NO SEAT FREE: YOU WATCH").to_string());
            }
            // A guest's own menu open, its players hold nothing.
            let away = !n.hosting() && front.menu.is_open();
            for local in 1..=seating.seats() {
                let Some(seat) = n.locals.seat(local) else { continue };
                let pad = if local == 1 { &settings.pad } else { settings.pad_of(local) };
                let its_pads = pads_of(pads, seating, local);
                let input = if away { crate::input::Input::default() } else { crate::controls::read_seat(Some(keyboard), seating.keys(local), &its_pads, pad, settings, game.actions()) };
                let controls = seat_controls(game, settings, front, seating.keys(local), local, its_pads.first().copied());
                n.read(seat, input, controls);
            }
            game.set_local(crate::game::Local { seats: Some(n.locals.seats()), language: Some(lang.to_string()) });
        }
    }

    /// A guest plays the host's bundles as they come, whatever menu is
    /// open (its own menu stops nothing); a fade between them, the rest
    /// wait for it. The host's loop standing still pauses every machine.
    pub(super) fn guest_ticks(&mut self) {
        let App { net, front, game, display, speaker, settings, pads, prompt, .. } = self;
        if let Some(n) = net
            && front.playing
        {
            let now = n.now();
            if let crate::network::Side::Guest(j) = &mut n.side
                && !game.fading()
            {
                n.held.extend(j.ticks(now, MAX_CATCH_UP as usize));
                while !game.fading()
                    && let Some(b) = n.held.pop_front()
                {
                    match j.play(game, &b) {
                        Ok(true) => display.remesh(game),
                        Ok(false) => {}
                        Err(e) => {
                            let _ = j.end(&failure(&e));
                            break;
                        }
                    }
                    tick_sounds(game, speaker, pads, settings, 1.0, true);
                }
            }
            if let crate::network::Side::Host(h) = &mut n.side {
                h.host.set_paused((front.menu.is_open() || prompt.open).then_some(0));
            }
        }
    }

    /// What this machine has for the others sent; the ticket, once the
    /// link across the internet is there, and the game's code, once our
    /// server gave it (the shorter to send), on the clipboard.
    pub(super) fn net_send(&mut self) {
        let App { net, video, settings, copied_code, .. } = self;
        if let Some(n) = net {
            n.send();
            if let Some(ticket) = n.new_ticket()
                && video.clipboard().set_clipboard_text(&ticket).is_ok()
            {
                n.say(crate::lang::say(settings.language(), "TICKET COPIED: SEND IT").to_string());
            }
            if let Some(code) = n.game_code().map(str::to_string)
                && copied_code.as_ref() != Some(&code)
            {
                if video.clipboard().set_clipboard_text(&code).is_ok() {
                    n.say(crate::lang::say_with(settings.language(), "ON THE INTERNET: CODE {}", &[&code]));
                    n.say(crate::lang::say(settings.language(), "CODE COPIED: SEND IT").to_string());
                }
                *copied_code = Some(code);
            }
        }
    }
}


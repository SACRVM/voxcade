//! A network game in the window (`docs/multiplayer.md`): what came in, a
//! guest catching up, running apart or left; this machine's players' keys
//! sent; a guest playing the host's bundles; the ticket and the code on the
//! clipboard.

use std::time::Instant;

use super::{App, CATCH_UP_BUDGET, MAX_CATCH_UP, apply_settings, keys, play_the_games_song, rumble, title_menu};
use crate::controls::Device;
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
        let App { net, game, settings, mask, renderer, screen, seating, last_pad, pads, front, runs, parting, queued, rejoined, dir, .. } = self;
        // The host's link across the internet would not start: its network
        // game is over, the game plays on alone.
        if let Some(why) = net.as_mut().and_then(|n| n.broken()) {
            *parting = Some((why.to_uppercase(), Instant::now()));
            *net = None;
        }
        let Some(n) = net.as_mut() else { return };
        if let Err(e) = n.receive(game, &settings.language()) {
            eprintln!("the network game: {e:#}");
        }
        if let crate::network::Side::Guest(j) = &mut n.side {
            match j.catch_up(CATCH_UP_BUDGET) {
                Ok(Some(caught)) => {
                    println!("caught up with the host at tick {}", caught.game.sim().tick);
                    *game = caught;
                    apply_settings(game, settings);
                    game.fades = true;
                    *mask = game.face_mask();
                    renderer.rebuild_all(&screen.device, &screen.queue, game.world_mut(), mask);
                    *seating = crate::seating::Seating::new(game.cfg.players);
                    seating.begin(last_pad.filter(|_| front.played_with == Device::Gamepad), &crate::seating::resolve(&settings.players, &pads.named()));
                    front.straight_in();
                    play_the_games_song(game, settings);
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = j.end(&format!("{e:#}").to_uppercase());
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
            *parting = Some((crate::lang::say_line(&settings.language(), &why), Instant::now()));
            *net = None;
            if rejoin {
                // Back to the title's game, joining at once.
                if let Ok(fresh) = Session::<G>::load_with(dir, Slots::on_disk(dir)) {
                    *game = fresh;
                    apply_settings(game, settings);
                    *mask = game.face_mask();
                    renderer.rebuild_all(&screen.device, &screen.queue, game.world_mut(), mask);
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
                n.say(crate::lang::say(&lang, "NO SEAT FREE: YOU WATCH").to_string());
            }
            let of_seat = |seat: usize| pads.open.iter().filter(|p| p.id().ok().and_then(|id| seating.pad_seat(id.raw())) == Some(seat)).collect::<Vec<_>>();
            // A guest's own menu open, its players hold nothing.
            let away = !n.hosting() && front.menu.is_open();
            for local in 1..=seating.seats() {
                let Some(seat) = n.locals.seat(local) else { continue };
                let pad = if local == 1 { &settings.pad } else { settings.pad_of(local) };
                let input = if away { crate::input::Input::default() } else { crate::controls::read_seat(Some(keyboard), seating.keys(local), &of_seat(local), pad, settings, game.actions()) };
                let controls = match (local, seating.keys(local)) {
                    (1, half @ (crate::seating::Keys::Left | crate::seating::Keys::Right)) if front.played_with == Device::Keyboard => crate::controls::Controls::of_half(game.actions(), half, settings),
                    (1, crate::seating::Keys::Nothing) => game.player_controls(Device::Gamepad, front.pad_kind, settings),
                    (1, _) => game.player_controls(front.played_with, front.pad_kind, settings),
                    (_, crate::seating::Keys::Whole) => crate::controls::Controls::of(game.actions(), Device::Keyboard, front.pad_kind, settings),
                    (_, crate::seating::Keys::Nothing) => crate::controls::Controls::of_pad(game.actions(), of_seat(local).first().map_or(front.pad_kind, |p| keys::pad_style(p.r#type())), pad, settings),
                    (_, half) => crate::controls::Controls::of_half(game.actions(), half, settings),
                };
                n.read(seat, input, controls);
            }
            game.set_local(crate::game::Local { seats: Some(n.locals.seats()), language: Some(lang) });
        }
    }

    /// A guest plays the host's bundles as they come, whatever menu is
    /// open (its own menu stops nothing); a fade between them, the rest
    /// wait for it. The host's loop standing still pauses every machine.
    pub(super) fn guest_ticks(&mut self) {
        let App { net, front, game, mask, renderer, screen, speaker, settings, pads, prompt, .. } = self;
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
                        Ok(renewed) => {
                            if renewed {
                                *mask = game.face_mask();
                                renderer.rebuild_all(&screen.device, &screen.queue, game.world_mut(), mask);
                            }
                        }
                        Err(e) => {
                            let _ = j.end(&format!("{e:#}").to_uppercase());
                            break;
                        }
                    }
                    if !game.game.sim().sounds.is_empty() {
                        if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
                            game.play_sounds(&mut mixer, settings.sound_gain());
                        }
                        if settings.rumble {
                            rumble(&mut pads.open, game);
                        }
                    }
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
                n.say(crate::lang::say(&settings.language(), "TICKET COPIED: SEND IT").to_string());
            }
            if let Some(code) = n.game_code().map(str::to_string)
                && copied_code.as_ref() != Some(&code)
            {
                if video.clipboard().set_clipboard_text(&code).is_ok() {
                    n.say(crate::lang::say_with(&settings.language(), "ON THE INTERNET: CODE {}", &[&code]));
                    n.say(crate::lang::say(&settings.language(), "CODE COPIED: SEND IT").to_string());
                }
                *copied_code = Some(code);
            }
        }
    }
}


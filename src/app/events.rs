//! The window's events: its size and focus, pads plugged in and out, text
//! typed, keys, gamepad buttons and sticks. What they ask of the menus
//! goes to the frame's commands.

use sdl3::event::{Event, WindowEvent};
use sdl3::gamepad::{Axis, Button};
use sdl3::joystick::JoystickId;
use sdl3::keyboard::{Keycode, Mod, Scancode};

use super::keys::{self, PlayKey};
use super::{App, Flow, pause_for, planned_keyboard, planned_pad};
use crate::controls::{Device, Nav, PAUSE};
use crate::game::Game;
use crate::menu::{Command, Page};
use crate::seating::{Keys, Start};

impl<G: Game> App<G> {
    /// One event of the window; `phase_over`: the run has ended.
    pub(super) fn event(&mut self, event: Event, phase_over: bool, commands: &mut Vec<Command>) -> Flow {
        match event {
            Event::Quit { .. } => return Flow::Quit,
            Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => {
                let super::Display { screen, renderer, .. } = &mut self.display;
                screen.resize(renderer, w, h);
            }
            // Looking away pauses the game (a network game's host fetching
            // a drink would freeze every screen: ESCAPE does it).
            Event::Window { win_event: WindowEvent::FocusLost, .. } if self.front.playing && !self.front.menu.is_open() && !phase_over && self.net.is_none() => {
                if self.game.fading() {
                    self.front.pause_after_fade = true;
                } else {
                    pause_for(&mut self.front, &self.game, self.watching.is_none(), None);
                }
            }
            Event::GamepadAdded { which, .. } => self.pads.added(which),
            Event::GamepadRemoved { which, .. } => self.pads.removed(which),
            Event::TextInput { text, .. } if self.prompt.open => self.prompt.type_text(&text),
            Event::TextInput { text, .. } if self.front.typing_address => text.chars().for_each(|c| self.front.menu.type_address(c)),
            Event::KeyDown { scancode: Some(code), keycode, keymod, repeat, .. } => return self.key_down(code, keycode, keymod, repeat, phase_over, commands),
            Event::GamepadButtonDown { which, button, .. } => self.button_down(which, button, phase_over, commands),
            Event::GamepadAxisMotion { axis, value, .. } => self.axis_motion(axis, value, commands),
            _ => {}
        }
        Flow::Go
    }

    /// A key: the console's, the address typed, a menu's, the maker's, a
    /// seat's pause or start, the pause; the game itself reads the keys
    /// held (`read_input`).
    fn key_down(&mut self, code: Scancode, keycode: Option<Keycode>, keymod: Mod, repeat: bool, phase_over: bool, commands: &mut Vec<Command>) -> Flow {
        if self.watching.is_some() && code == Scancode::Escape {
            return Flow::Quit;
        }
        self.attract.input();
        if self.front.leaves_attract() {
            return Flow::Go;
        }
        // The console open: its keys, none for the game.
        if self.prompt.open {
            self.prompt_key(code, keymod, repeat);
            return Flow::Go;
        }
        // The host's address typed: its keys.
        if self.front.typing_address {
            self.address_key(code, repeat, commands);
            return Flow::Go;
        }
        let App { prompt, text_input, display, net, game, front, settings, seating, runs, dir, watching, maker, .. } = self;
        // A guest's console would change its game alone.
        if code == Scancode::Grave && !repeat && front.playing && watching.is_none() && !front.menu.is_open() && !game.fading() && net.as_ref().is_none_or(|n| n.hosting()) {
            prompt.open();
            text_input.start(&display.screen.window);
            return Flow::Go;
        }
        let name = code.name();
        if front.menu.rebinding.is_some() {
            if !repeat {
                commands.extend(front.menu.bind(Device::Keyboard, name, code == Scancode::Escape, settings));
            }
            return Flow::Go;
        }
        // Initials are typed, the letter keys taken as the letters they say
        // on this keyboard.
        let typed = keycode.map(|k| k.name()).unwrap_or_default();
        if front.menu.page() == Some(Page::Initials)
            && let Some(c) = typed.chars().next().filter(|c| typed.len() == 1 && crate::menu::LETTERS.contains(c.to_ascii_uppercase()))
        {
            front.played_with = Device::Keyboard;
            if !repeat {
                commands.extend(front.menu.type_letter(c));
            }
            return Flow::Go;
        }
        let maker_code = keys::maker_key(code, *maker, watching.is_some());
        // Playing, not watching a replay: a key pressed, not one held.
        let in_play = !repeat && front.playing && watching.is_none();
        if code == Scancode::F11 && !repeat {
            settings.fullscreen = !settings.fullscreen;
            commands.push(Command::Fullscreen);
        } else if maker_code == Some(Scancode::F5) && !repeat {
            commands.push(Command::Restart);
        } else if maker_code == Some(Scancode::F9) && !repeat && watching.is_none() && net.as_ref().is_some_and(|n| !n.hosting()) {
            // A guest keeps the network game's recording.
            if let Some(crate::network::Side::Guest(j)) = net.as_ref().map(|n| &n.side) {
                let path = dir.join("replays").join(format!("network-{}.toml", crate::saves::now()));
                match j.recording(game).and_then(|r| r.save(&path)) {
                    Ok(()) => println!("kept the run as {}", path.display()),
                    Err(e) => println!("F9: {e:#}"),
                }
            }
        } else if maker_code == Some(Scancode::F9) && !repeat && watching.is_none() {
            match runs.keep(game) {
                Ok(path) => println!("kept the run as {}", path.display()),
                Err(e) => println!("F9: {e:#}"),
            }
        } else if let Some(index) = maker_code.and_then(keys::song_key).filter(|_| !repeat && front.menu.page() != Some(Page::Initials)) {
            if index < game.cfg.audio.music.files().len() {
                settings.song = index;
                commands.push(Command::Song);
            } else {
                println!("no song {} (c.audio.music lists {})", index + 1, game.cfg.audio.music.files().len());
            }
        } else if front.menu.is_open() {
            front.played_with = Device::Keyboard;
            if let Some(nav) = keys::key_nav(code, settings).filter(|n| !repeat || keys::is_direction(*n)) {
                commands.extend(front.menu.nav(nav, settings, game.cfg.audio.music.files().len()));
            }
        } else if in_play
            && !phase_over
            && let Some(seat) = (1..=seating.seats()).filter(|&n| matches!(seating.keys(n), Keys::Left | Keys::Right)).find(|&n| settings.keys_of(seating.keys(n)).and_then(|b| b.action_of(name)) == Some(PAUSE))
        {
            // A half's pause: its seat's (LEAVE offered after the first).
            if !game.fading() {
                pause_for(front, game, true, (seat > 1).then_some(seat));
            }
        } else if in_play && !phase_over && !seating.alone() && name == crate::seating::KEYBOARD_START && settings.keys.action_of(name).is_none() {
            // The keyboard (whole, or its right half): a seat taken, then
            // its pause.
            match seating.keyboard_start(planned_keyboard(settings)) {
                Start::Pause(seat) if !game.fading() => pause_for(front, game, true, (seat > 1).then_some(seat)),
                Start::Join(seat) if seating.keys(seat) == Keys::Whole => println!("player {seat} takes a seat at the keyboard"),
                Start::Join(seat) => println!("player {seat} takes a seat at the keyboard's right half"),
                _ => {}
            }
        } else if in_play {
            front.played_with = Device::Keyboard;
            match keys::play_key(code, settings, phase_over, game.fading()) {
                PlayKey::ToTitle => commands.push(Command::ToTitle),
                // A level change's fade is seen out first; the whole
                // keyboard another seat's, LEAVE offered.
                PlayKey::Pause => pause_for(front, game, true, seating.keyboard_seat().filter(|&s| seating.keys(s) == Keys::Whole)),
                PlayKey::Game => {}
            }
        }
        Flow::Go
    }

    /// A key while the console is open.
    fn prompt_key(&mut self, code: Scancode, keymod: Mod, repeat: bool) {
        let App { prompt, text_input, display, net, game, video, .. } = self;
        let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
        match code {
            Scancode::Escape | Scancode::Grave if !repeat => {
                prompt.close();
                text_input.stop(&display.screen.window);
            }
            Scancode::Return | Scancode::KpEnter if !repeat => {
                if let Some(line) = prompt.take() {
                    prompt.answer(net.as_ref().map_or(Ok(()), |n| n.takes_line(game, &line)).and_then(|()| game.eval(&line)));
                }
            }
            Scancode::Backspace => prompt.backspace(),
            Scancode::Up => prompt.recall(-1),
            Scancode::Down => prompt.recall(1),
            Scancode::L if ctrl => prompt.clear(),
            Scancode::V if ctrl => {
                if let Ok(text) = video.clipboard().clipboard_text() {
                    prompt.type_text(&text);
                }
            }
            _ => {}
        }
    }

    /// A key while the host's address is typed: Backspace takes a sign,
    /// Enter joins, Escape goes back; the letters are the text's.
    fn address_key(&mut self, code: Scancode, repeat: bool, commands: &mut Vec<Command>) {
        let (menu, settings) = (&mut self.front.menu, &mut self.settings);
        match code {
            Scancode::Backspace => menu.backspace_address(),
            Scancode::Return | Scancode::KpEnter if !repeat => commands.extend(menu.nav(Nav::Confirm, settings, 0)),
            Scancode::Escape if !repeat => commands.extend(menu.nav(Nav::Back, settings, 0)),
            _ => {}
        }
    }

    /// A gamepad's button: the pad's kind noted, a menu's, a seat taken or
    /// its pause.
    fn button_down(&mut self, which: JoystickId, button: Button, phase_over: bool, commands: &mut Vec<Command>) {
        self.attract.input();
        if self.front.leaves_attract() {
            return;
        }
        let Some(name) = crate::controls::button_name(button) else { return };
        let App { pads, front, last_pad, settings, game, seating, watching, .. } = self;
        // The letters on the pad in hand, for the menus' hints.
        if let Some(pad) = pads.open.iter().find(|p| p.id().ok() == Some(which)) {
            front.pad_kind = keys::pad_style(pad.r#type());
        }
        if !front.playing {
            *last_pad = Some(which.raw());
        }
        if front.menu.rebinding.is_some() {
            commands.extend(front.menu.bind(Device::Gamepad, name, button == Button::Back, settings));
            return;
        }
        if front.menu.is_open() {
            front.played_with = Device::Gamepad;
            if let Some(nav) = keys::pad_nav(button) {
                commands.extend(front.menu.nav(nav, settings, game.cfg.audio.music.files().len()));
            }
        } else if front.playing && watching.is_none() {
            front.played_with = Device::Gamepad;
            match keys::play_button(button, name, settings, phase_over, game.fading()) {
                PlayKey::ToTitle => commands.push(Command::ToTitle),
                // A pad no seat has takes one; a seat's START pauses (a
                // fade seen out first), offering LEAVE to a player after
                // the first.
                PlayKey::Pause => {
                    let plugged = pads.ids();
                    match seating.pad_start(which.raw(), &plugged, planned_pad(settings, &pads.named(), which.raw())) {
                        Start::Pause(seat) => pause_for(front, game, true, (seat > 1).then_some(seat)),
                        Start::Join(seat) => println!("player {seat} takes a seat"),
                        Start::Full => {}
                    }
                }
                PlayKey::Game => {}
            }
        }
    }

    /// A gamepad's stick or trigger: a trigger bound, the left stick
    /// steering a menu (once past the middle, back to it before it counts
    /// again).
    fn axis_motion(&mut self, axis: Axis, value: i16, commands: &mut Vec<Command>) {
        let v = value as f32 / 32767.0;
        if v.abs() > 0.5 {
            self.attract.input();
            if self.front.leaves_attract() {
                return;
            }
        }
        let App { front, settings, game, watching, .. } = self;
        if front.menu.rebinding.is_some() {
            let trigger = match axis {
                Axis::TriggerLeft => Some("lefttrigger"),
                Axis::TriggerRight => Some("righttrigger"),
                _ => None,
            };
            if let Some(name) = trigger.filter(|_| v > 0.6) {
                commands.extend(front.menu.bind(Device::Gamepad, name, false, settings));
            }
            return;
        }
        if !front.menu.is_open() {
            if v.abs() > 0.5 && front.playing && watching.is_none() {
                front.played_with = Device::Gamepad;
            }
            return;
        }
        if !matches!(axis, Axis::LeftX | Axis::LeftY) {
            return;
        }
        let way = match (axis, v) {
            (Axis::LeftX, v) if v < -0.6 => Some(Nav::Left),
            (Axis::LeftX, v) if v > 0.6 => Some(Nav::Right),
            (Axis::LeftY, v) if v < -0.6 => Some(Nav::Up),
            (Axis::LeftY, v) if v > 0.6 => Some(Nav::Down),
            _ => None,
        };
        let along = |n: Nav| matches!((axis, n), (Axis::LeftX, Nav::Left | Nav::Right) | (Axis::LeftY, Nav::Up | Nav::Down));
        match way {
            Some(nav) if front.stick != Some(nav) => {
                front.stick = Some(nav);
                front.played_with = Device::Gamepad;
                commands.extend(front.menu.nav(nav, settings, game.cfg.audio.music.files().len()));
            }
            None if v.abs() < 0.3 && front.stick.is_some_and(along) => front.stick = None,
            _ => {}
        }
    }
}

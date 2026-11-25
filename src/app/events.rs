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
            // Looking away holds a game of one, no menu opened, and looking
            // back lets it go on at once (a network game goes on: the host
            // fetching a drink would freeze every screen; ESCAPE does it).
            // What was held lets go (the keys, the mouse's buttons), and the
            // time away is owed to nobody.
            Event::Window { win_event: WindowEvent::FocusLost, .. } => {
                self.front.look_away(self.net.is_some());
                self.mouse.let_go();
                self.sticks.iter_mut().for_each(super::mouse::StickPointer::let_go);
            }
            Event::Window { win_event: WindowEvent::FocusGained, .. } => {
                self.front.look_back();
                self.clock.last = std::time::Instant::now();
                self.clock.ticks.settle();
            }
            Event::GamepadAdded { which, .. } => self.pads.added(which),
            Event::GamepadRemoved { which, .. } => self.pads.removed(which),
            Event::TextInput { text, .. } if self.prompt.open => self.prompt.type_text(&text),
            Event::TextInput { text, .. } if self.front.typing_address => text.chars().for_each(|c| self.front.menu.type_address(c)),
            Event::TextInput { text, .. } if self.mouse.typing => self.mouse.type_text(crate::input::Typed { text: text.chars().filter(|&c| crate::ui::writes(c)).collect(), ..Default::default() }),
            Event::Window { win_event: WindowEvent::MouseLeave, .. } => self.mouse.at = None,
            Event::MouseMotion { x, y, .. } if self.game.has_pointer() => self.mouse_moved([x, y]),
            Event::MouseButtonDown { mouse_btn, x, y, .. } if self.game.has_pointer() => self.mouse_down(mouse_btn, [x, y], commands),
            Event::MouseButtonUp { mouse_btn, .. } if self.game.has_pointer() => {
                if let Some(i) = super::mouse::Mouse::index(mouse_btn) {
                    self.mouse.up(i);
                }
            }
            Event::MouseWheel { y, .. } if self.game.has_pointer() && self.front.menu.rebinding.is_some() && self.front.menu.page() == Some(Page::Controls) && y != 0.0 => {
                let name = if y > 0.0 { "wheel_up" } else { "wheel_down" };
                commands.extend(self.front.menu.bind(Device::Keyboard, name, false, &mut self.settings));
            }
            Event::MouseWheel { y, direction, .. } if self.game.has_pointer() && self.front.playing && !self.front.menu.is_open() => {
                let flip = if matches!(direction, sdl3::mouse::MouseWheelDirection::Flipped) { -1.0 } else { 1.0 };
                self.front.played_with = Device::Keyboard;
                self.mouse.turned(y * flip);
            }
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
        // The keyboard's player types: Backspace, Enter and Escape are the
        // text's (the letters come as text), nothing pauses.
        if self.mouse.typing && self.front.playing && !self.front.menu.is_open() {
            let typed = match code {
                Scancode::Backspace => Some(crate::input::Typed { erase: 1, ..Default::default() }),
                Scancode::Return | Scancode::KpEnter if !repeat => Some(crate::input::Typed { done: true, ..Default::default() }),
                Scancode::Escape if !repeat => Some(crate::input::Typed { cancel: true, ..Default::default() }),
                _ => None,
            };
            let swallowed = typed.is_some() || (crate::controls::types(code) && code != Scancode::Grave);
            if let Some(t) = typed {
                self.mouse.type_text(t);
            }
            if swallowed {
                return Flow::Go;
            }
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
        // Playing, not watching a replay: a key pressed, not one held. On
        // the game's own title the pause keys are the game's alone.
        let in_play = !repeat && front.playing && watching.is_none();
        let pauses = !game.on_title();
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
        } else if front.menu.is_open() {
            front.played_with = Device::Keyboard;
            if let Some(nav) = keys::key_nav(code, settings).filter(|n| !repeat || keys::is_direction(*n)) {
                commands.extend(front.menu.nav(nav, settings));
            }
        } else if in_play
            && pauses
            && !phase_over
            && let Some(seat) = (1..=seating.seats()).filter(|&n| matches!(seating.keys(n), Keys::Left | Keys::Right)).find(|&n| settings.keys_of(seating.keys(n)).and_then(|b| b.action_of(name)) == Some(PAUSE))
        {
            // A half's pause: its seat's (LEAVE offered after the first).
            if !game.fading() {
                pause_for(front, game, true, (seat > 1).then_some(seat));
            }
        } else if in_play && pauses && !phase_over && !seating.alone() && name == crate::seating::KEYBOARD_START && settings.keys.action_of(name).is_none() {
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
                PlayKey::Pause if pauses => pause_for(front, game, true, seating.keyboard_seat().filter(|&s| seating.keys(s) == Keys::Whole)),
                PlayKey::Pause | PlayKey::Game => {}
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
            Scancode::Return | Scancode::KpEnter if !repeat => commands.extend(menu.nav(Nav::Confirm, settings)),
            Scancode::Escape if !repeat => commands.extend(menu.nav(Nav::Back, settings)),
            _ => {}
        }
    }

    /// The mouse moved to `at` (window points): the keyboard's player uses
    /// it; over an engine's menu the row under it is chosen.
    fn mouse_moved(&mut self, at: [f32; 2]) {
        let moved = self.mouse.at.is_some_and(|was| (was[0] - at[0]).abs() + (was[1] - at[1]).abs() > 2.0);
        self.mouse.at = Some(at);
        if moved {
            self.front.played_with = Device::Keyboard;
            self.attract.input();
        }
        if let Some(row) = self.menu_row_at(at) {
            self.front.menu.choose(row);
        }
    }

    /// A mouse button went down at `at`: over an engine's menu the left
    /// one confirms the row under it, the right goes back; playing, it is
    /// the game's, with what the pointer is over now.
    fn mouse_down(&mut self, button: sdl3::mouse::MouseButton, at: [f32; 2], commands: &mut Vec<Command>) {
        self.mouse.at = Some(at);
        self.front.played_with = Device::Keyboard;
        self.attract.input();
        if self.front.leaves_attract() {
            return;
        }
        // CONTROLS waits for a key: the mouse's button is one (a game of
        // one's, the keyboard's bindings).
        if self.front.menu.rebinding.is_some() {
            if self.front.menu.page() == Some(Page::Controls)
                && let Some(i) = super::mouse::Mouse::index(button)
            {
                commands.extend(self.front.menu.bind(Device::Keyboard, crate::input::MOUSE_KEYS[i], false, &mut self.settings));
            }
            return;
        }
        if self.front.menu.is_open() {
            let App { front, settings, .. } = self;
            match button {
                sdl3::mouse::MouseButton::Left => {
                    if let Some(row) = self.menu_row_at(at) {
                        self.front.menu.choose(row);
                        commands.extend(self.front.menu.nav(Nav::Confirm, &mut self.settings));
                    }
                }
                sdl3::mouse::MouseButton::Right => commands.extend(front.menu.nav(Nav::Back, settings)),
                _ => {}
            }
            return;
        }
        if let Some(i) = super::mouse::Mouse::index(button).filter(|_| self.front.playing && self.watching.is_none()) {
            let answer = self.pointer_now();
            self.mouse.down(i, answer);
        }
    }

    /// The engine's menu row under window point `at`, if one is.
    fn menu_row_at(&self, at: [f32; 2]) -> Option<usize> {
        if !self.front.menu.is_open() || self.front.menu.rebinding.is_some() {
            return None;
        }
        let [x, y] = self.hud_at(at)?;
        let rows = self.game.menu_rows(&self.front.menu, &self.settings, self.display.screen.aspect());
        rows.into_iter().find(|&(_, rx, ry, w, h)| x >= rx && y >= ry && x < rx + w && y < ry + h).map(|(i, ..)| i)
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
                commands.extend(front.menu.nav(nav, settings));
            }
        } else if front.playing && watching.is_none() {
            front.played_with = Device::Gamepad;
            match keys::play_button(button, name, settings, phase_over, game.fading()) {
                PlayKey::Game => self.stick_press(which, name),
                PlayKey::ToTitle => commands.push(Command::ToTitle),
                // A pad no seat has takes one; a seat's START pauses (a
                // fade seen out first), offering LEAVE to a player after
                // the first.
                PlayKey::Pause if !game.on_title() => {
                    let plugged = pads.ids();
                    match seating.pad_start(which.raw(), &plugged, planned_pad(settings, &pads.named(), which.raw())) {
                        Start::Pause(seat) => pause_for(front, game, true, (seat > 1).then_some(seat)),
                        Start::Join(seat) => println!("player {seat} takes a seat"),
                        Start::Full => {}
                    }
                }
                PlayKey::Pause => {}
            }
        }
    }

    /// A pad's button that is one of the game's actions for its seat, the
    /// seat's stick moving the pointer (`c.pointer_stick`): it carries the
    /// pointer's answers of this moment, as a mouse's click does.
    fn stick_press(&mut self, which: JoystickId, name: &str) {
        let Some(seat) = self.seating.pad_seat(which.raw()).filter(|&s| s == 1 && s == self.mouse_seat()) else { return };
        let bound = self.settings.pad_of(seat).action_of(name).is_some_and(|a| self.game.actions().contains(&a));
        if bound && !self.sticks.is_empty() {
            let answer = self.pointer_now();
            self.sticks[seat - 1].down(answer);
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
        let App { front, settings, watching, .. } = self;
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
                commands.extend(front.menu.nav(nav, settings));
            }
            None if v.abs() < 0.3 && front.stick.is_some_and(along) => front.stick = None,
            _ => {}
        }
    }
}

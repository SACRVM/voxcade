//! Play in the IDE's main area: the game from its folder begun as a
//! window begins it (on its own title where it has one, `c.title_screen.
//! level`, else the run at once: PLAY shows no engine's title), stepped at
//! the simulation's fixed tick and drawn into a texture of its own on the
//! IDE's GPU, as big as the area. The engine's pages the game opens over
//! itself (SETTINGS, the pause page, its pause keys' too) show over it as
//! in a window, the window's front end (`crate::front`): the game holds
//! while one shows, the settings it changes are the player's of that game,
//! saved and taking effect at once. Its sounds and music come out of the
//! player's speaker, its keys and pads are bound as the player set them
//! (`controls::read_input`, as `voxcade run` reads them). It holds still
//! while the main area shows a file or the window is elsewhere; a file
//! saved comes in at once (hot reload), and a script's error halts it
//! until mended (`trouble`). A local of the game's that changes while it
//! plays, state no snapshot keeps, is named in OUTPUT within a second
//! (`lua::hidden::Watch`). Its runs are recorded as `voxcade run` records
//! them (`replay::Runs`: kept in the config folder from 10 s on, F9 puts
//! one in the game's `replays/`; a run with a file saved during it is not
//! kept; a title counts for nothing). In a game with a pointer the mouse
//! over its picture is the pointer, as a window's mouse is, its buttons
//! and wheel the keys the game binds (over a page, its rows), a pad's
//! stick moving it where `c.pointer_stick` says (`crate::mouse`, the
//! window's own); the text a player types goes to the game while it has
//! the keys.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use sdl3::gamepad::{Axis, Button};
use sdl3::keyboard::Scancode;

use crate::controls::{Device, PAUSE, PadStyle};
use crate::front::keys::{PlayKey, play_button, play_key};
use crate::front::{Front, apply_settings, pause_for, pause_menu, planned_keyboard, planned_pad, reseat};
use crate::game::Game;
use crate::menu::{Command, Menu};
use crate::render::Renderer;
use crate::replay::Runs;
use crate::saves::Slots;
use crate::seating::{Keys, Start};
use crate::session::{Reloaded, Scene, Session};
use crate::settings::Settings;
use crate::watch::FolderWatch;

/// The format it is drawn in: the games' own, as the headless canvas's.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;


/// What the game is drawn into, and its name to egui.
struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
    id: egui::TextureId,
}

/// The frames counted for SHOW FPS, as a window counts them: since when,
/// how many, the rate last counted.
struct Rate {
    since: Instant,
    frames: u32,
    fps: f32,
}

impl Rate {
    fn frame(&mut self) {
        self.frames += 1;
        let seconds = self.since.elapsed().as_secs_f32();
        if seconds >= 0.5 {
            (self.fps, self.frames, self.since) = (self.frames as f32 / seconds, 0, Instant::now());
        }
    }
}

/// A game being played.
pub struct Play<G: Game> {
    dir: PathBuf,
    game: Session<G>,
    renderer: Renderer,
    mask: crate::mesh::FaceMask,
    target: Option<Target>,
    settings: Settings,
    /// Where the player's settings are written, as a window writes them
    /// (`settings::path_for`), and whether the saves are the player's on
    /// disk (`Slots::on_disk`; a test keeps them in memory).
    settings_path: PathBuf,
    on_disk: bool,
    watch: FolderWatch,
    speaker: Option<crate::audio::Speaker>,
    runs: Runs,
    /// Files that would not load since last asked (for OUTPUT), and what
    /// the pages asked that failed (a save, a load).
    failed: Vec<String>,
    /// The files that would not load when they changed, by name, with what
    /// was said; a file leaves when it loads again or is gone.
    unloaded: std::collections::BTreeMap<String, String>,
    /// Time not yet stepped, and when the last frame was (None: held, so
    /// it goes on without catching up).
    clock: crate::app::TickClock,
    last: Option<Instant>,
    /// The scenario every run of it begins from (PLAY SCENARIO; F5 too).
    scenario: Option<String>,
    /// Who plays on which seat (`c.players`): pads and the keyboard's
    /// halves, as in a window of the game.
    seating: crate::seating::Seating,
    /// How the run being played began (`Session::start_mark`).
    mark: u64,
    /// What the play says of itself, for OUTPUT (`said`).
    notes: Vec<String>,
    /// The hidden-state guard: the locals the game's functions keep,
    /// looked at every second (`lua::hidden::Watch`), and what it said of
    /// those that changed, for OUTPUT.
    guard: crate::lua::hidden::Watch,
    hidden: Vec<String>,
    /// Seat 1's keys as the line under the game shows them (`keys_of`),
    /// made anew only when the seating or the game changes.
    keys: Vec<(String, String)>,
    /// The game asked to end (`world:quit()`, QUIT): PLAY stops it, never
    /// the IDE.
    quit: bool,
    /// The front end over the game, as a window's (`crate::front`): the
    /// engine's page open over it, what seat 1 played with last
    /// (`controls.device`), the letters on the pad in hand, the stick
    /// steering a page. Always playing: a title is the game's own level.
    front: Front,
    /// The mouse over the game's picture (its place in the picture's pixels
    /// from the top left, None off it), its buttons and wheel, and the text
    /// typed while the keyboard's player types; each seat's pointer its
    /// pad's stick moves (`c.pointer_stick`), seat 1's first: as a window's.
    mouse: crate::mouse::Mouse,
    sticks: Vec<crate::mouse::StickPointer>,
    /// Which seats have a pad, and the pads plugged in by name (the
    /// players page), as read last.
    seat_pads: Vec<bool>,
    named: Vec<(u32, String)>,
    /// The world is built anew on the GPU at the next picture (another
    /// game in place of the one played).
    remesh: bool,
    rate: Rate,
}

impl<G: Game> Play<G> {
    /// The game in `dir`, its sound through `speaker` (none in tests),
    /// begun from `scenario` if one is given, else as a window begins it
    /// (`begin`).
    pub fn start(dir: &Path, device: &wgpu::Device, queue: &wgpu::Queue, speaker: Option<crate::audio::Speaker>, scenario: Option<&str>) -> Result<Self> {
        let mut game = Session::<G>::load_with(dir, Slots::on_disk(dir))?;
        begin(&mut game, scenario, true)?;
        let settings_path = crate::settings::path_for(dir);
        let mut settings = Settings::load(&settings_path);
        settings.adopt(game.actions(), &game.default_keys(), &game.default_pad(), game.has_pointer());
        settings.system_languages = crate::settings::system_languages();
        settings.embedded = true;
        crate::controls::label_keys(&mut settings);
        apply_settings(&mut game, &mut settings);
        let mut renderer = Renderer::new(device, FORMAT, 64, 64);
        let mask = game.face_mask();
        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
        let mut runs = Runs::new(dir);
        runs.start(&game);
        let seating = crate::seating::Seating::new(game.cfg.players);
        let mark = game.start_mark();
        let mut front = Front::new(Menu::default());
        front.straight_in();
        let rate = Rate { since: Instant::now(), frames: 0, fps: 0.0 };
        let mut play = Self { dir: dir.to_path_buf(), game, renderer, mask, target: None, settings, settings_path, on_disk: true, watch: FolderWatch::new(dir), speaker, runs, failed: Vec::new(), unloaded: Default::default(), clock: Default::default(), last: None, scenario: scenario.map(str::to_string), seating, mark, notes: Vec::new(), guard: Default::default(), hidden: Vec::new(), keys: Vec::new(), quit: false, front, mouse: Default::default(), sticks: Vec::new(), seat_pads: Vec::new(), named: Vec::new(), remesh: false, rate };
        play.guard.begin(play.game.game.hidden());
        play.name_keys();
        Ok(play)
    }

    /// The game from its files again (F5, AGAIN), as PLAY began it: from
    /// its scenario, else on its own title where it has one, else the run.
    pub fn restart(&mut self) -> Result<()> {
        let mut fresh = self.load()?;
        begin(&mut fresh, self.scenario.as_deref(), true)?;
        self.adopt(fresh);
        // Played again from its start, its song too.
        self.game.restart_music();
        Ok(())
    }

    /// RESTART on the pause page, as a window's: the run begun anew from
    /// the files (from PLAY's scenario if it was played from one), its
    /// song from the start.
    fn restart_run(&mut self) {
        let scenario = self.scenario.clone();
        match self.load().and_then(|mut fresh| begin(&mut fresh, scenario.as_deref(), false).map(|()| fresh)) {
            Ok(fresh) => {
                self.adopt(fresh);
                self.game.restart_music();
            }
            Err(e) => self.failed.push(format!("RESTART: the game would not begin again: {e:#}")),
        }
    }

    /// Back to the game's own title (`world:to_title()`, QUIT TO TITLE, a
    /// run's end), as a window goes (`leave_the_run`): the run ended (quit
    /// where it was not over), the game loaded afresh, its title begun, the
    /// same song playing on.
    fn back_to_title(&mut self) {
        if self.game.game.outcome().is_none() {
            self.game.game.quit();
        }
        match self.load().and_then(|mut fresh| begin(&mut fresh, None, true).map(|()| fresh)) {
            Ok(fresh) => self.adopt(fresh),
            Err(e) => self.failed.push(format!("Back to the title: the game would not begin again: {e:#}")),
        }
    }

    /// The run over and the player going on (Enter, the pause key, the
    /// pad's START or bottom button): back to the game's own title, as in
    /// a window; a game without one, which PLAY shows no title of, begins
    /// again.
    fn ended(&mut self) {
        if self.game.cfg.title_screen.level.is_some() {
            self.back_to_title();
        } else if let Err(e) = self.restart() {
            self.failed.push(format!("The game would not begin again: {e:#}"));
        }
    }

    /// The game loaded afresh from its folder, with the player's saves (as
    /// a window loads it).
    fn load(&self) -> Result<Session<G>> {
        let slots = if self.on_disk { Slots::on_disk(&self.dir) } else { self.game.slots.clone() };
        Session::<G>::load_with(&self.dir, slots)
    }

    /// `fresh`, a game just begun, plays in place of the one played (no
    /// page over it), its world built at the next picture.
    fn adopt(&mut self, fresh: Session<G>) {
        self.runs.end(&self.game);
        self.game = fresh;
        self.mark = self.game.start_mark();
        self.guard.begin(self.game.game.hidden());
        self.runs.start(&self.game);
        apply_settings(&mut self.game, &mut self.settings);
        self.front.straight_in();
        self.remesh = true;
        self.name_keys();
    }

    /// What a page or the game asked (`Command`), done as a window does it
    /// where PLAY has it: QUIT stops PLAY, QUIT TO TITLE goes back to the
    /// game's own title or, where it has none, stops PLAY too (PLAY shows
    /// no engine's title); the engine's title, its high score table and
    /// network play are a window's alone.
    fn command(&mut self, command: Command) {
        match command {
            Command::Quit => self.quit = true,
            Command::Resume => self.front.menu = Menu::default(),
            // The game opened the pause as its key does: not over a fade,
            // after the run's end, on a title or over another page.
            Command::Pause => {
                if !self.front.menu.is_open() && !self.game.on_title() && !self.game.fading() && self.game.game.outcome().is_none() {
                    pause_for(&mut self.front, &self.game, true, None);
                }
            }
            Command::Restart => self.restart_run(),
            Command::ToTitle if self.game.cfg.title_screen.level.is_some() => self.back_to_title(),
            Command::ToTitle => self.quit = true,
            Command::Save(slot) => {
                self.game.set_clock(crate::saves::now());
                if let Err(e) = self.game.save_to(slot) {
                    self.failed.push(format!("Saving into slot {slot} failed: {e:#}"));
                }
                // Back on the pause page, the slot's new line in it.
                self.front.menu.slots = pause_menu(&self.game, true).slots;
                self.front.menu.stack.pop();
            }
            Command::Load(slot) => self.load_slot(slot),
            Command::Changed | Command::Fullscreen => {
                crate::controls::label_keys(&mut self.settings);
                apply_settings(&mut self.game, &mut self.settings);
                self.save_settings();
                self.name_keys();
            }
            // The players page (CONTROLS in a game of several): the run's
            // seats; the settings keep the plan.
            Command::SeatKeys(..) | Command::SeatPad(..) | Command::Swap => {
                self.seating = reseat(command, self.seating.clone(), &mut self.settings, &self.named);
                self.save_settings();
                self.name_keys();
            }
            Command::Leave(seat) => {
                self.seating.leave(seat);
                self.front.menu = Menu::default();
                self.name_keys();
            }
            _ => {}
        }
    }

    /// Going on from the save in `slot` (LOAD), a run of its own: the game
    /// loaded afresh, then the save; one that does not load leaves the run
    /// as it was, paused.
    fn load_slot(&mut self, slot: u32) {
        let loaded = self.game.slots.read(slot).map(str::to_string).map(|text| self.load().and_then(|mut fresh| fresh.start_from(&text).map(|()| fresh)));
        match loaded {
            Some(Ok(fresh)) => self.adopt(fresh),
            Some(Err(e)) => {
                self.failed.push(format!("Loading slot {slot} failed: {e:#}"));
                self.front.menu = pause_menu(&self.game, true);
            }
            None => self.front.menu = pause_menu(&self.game, true),
        }
    }

    /// The player's settings written, as a window writes them; a failure
    /// said in OUTPUT.
    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.settings_path) {
            self.failed.push(format!("The settings were not saved: {e:#}"));
        }
    }

    /// `commands`, the pages' asks, done in turn.
    fn commands(&mut self, commands: Vec<Command>) {
        for command in commands {
            self.command(command);
        }
    }

    /// A console line run in the game between two ticks (the line under
    /// OUTPUT, the agent's `eval`): what it answered. It is part of the run.
    pub fn eval(&mut self, line: &str) -> Result<String> {
        self.game.eval(line)
    }

    /// What seat 1 holds now, the other seats' set on the game for the
    /// ticks to come (`keys`: the keyboard, while the game has it; the
    /// keyboard's player types then if its state says so, and the keys that
    /// type hold nothing), each seat's stick that moves the pointer read
    /// for it, as a window reads them (`App::read_input`).
    pub fn read(&mut self, keys: Option<&dyn crate::controls::KeyState>, pads: &[sdl3::gamepad::Gamepad]) -> crate::input::Input {
        use crate::seating::Keys;
        let mouse_seat = self.seating.mouse_seat();
        let typing = keys.is_some() && !self.front.menu.is_open() && self.game.typing(mouse_seat);
        if typing != self.mouse.typing {
            self.mouse.typing = typing;
            if !typing {
                self.mouse.typed.clear();
            }
        }
        let typed = keys.map(|keys| crate::controls::WithMouse { keys, mouse: &[], typing });
        let keys_of = |seat: usize| if seat == mouse_seat { typed.as_ref().map(|k| k as &dyn crate::controls::KeyState) } else { keys };
        let stick = self.game.cfg.pointer_stick;
        self.size_sticks();
        let aspect = self.aspect();
        // The players page shows the run's seats, the pads by name.
        self.named = crate::pads::named(pads);
        if self.game.cfg.players > 1 && self.front.menu.is_open() {
            self.front.menu.seats = crate::menu::SeatLine::all(&self.seating, &self.named);
        }
        let seating = &self.seating;
        let of_seat = |seat: usize| pads.iter().filter(|p| p.id().ok().and_then(|id| seating.pad_seat(id.raw())) == Some(seat)).collect::<Vec<_>>();
        let (input, lean) = if seating.alone() {
            crate::controls::read_input(keys_of(1), pads, &self.settings, self.game.actions(), stick)
        } else {
            self.seat_pads = (1..=seating.seats()).map(|seat| !of_seat(seat).is_empty()).collect();
            for seat in 2..=seating.seats() {
                let (mut input, lean) = crate::controls::read_seat(keys_of(seat), seating.keys(seat), &of_seat(seat), self.settings.pad_of(seat), &self.settings, self.game.actions(), stick);
                // The mouse's, where this seat has the keyboard; else the
                // pointer its pad's stick moves, aimed at.
                if let Some(s) = self.sticks.get_mut(seat - 1) {
                    s.lean = lean;
                }
                if seat == mouse_seat {
                    self.mouse.add_to(&mut input, &self.settings.keys, self.game.actions(), None);
                } else if let Some(now) = self.seat_pointer(seat) {
                    input.pointer = Some(Box::new(now));
                }
                if let Some(s) = self.sticks.get_mut(seat - 1) {
                    s.add_to(&mut input);
                }
                if seat != mouse_seat {
                    self.game.mouse_aim(&mut input, seat, aspect);
                }
                let controls = match seating.keys(seat) {
                    Keys::Whole => crate::controls::Controls::of(self.game.actions(), Device::Keyboard, PadStyle::default(), &self.settings),
                    Keys::Nothing => crate::controls::Controls::of_pad(self.game.actions(), PadStyle::default(), self.settings.pad_of(seat), &self.settings),
                    half => crate::controls::Controls::of_half(self.game.actions(), half, &self.settings),
                };
                self.game.set_seat(seat, seating.taken(seat).then_some(input));
                self.game.set_seat_controls(seat, controls);
            }
            crate::controls::read_seat(keys_of(1), seating.keys(1), &of_seat(1), &self.settings.pad, &self.settings, self.game.actions(), stick)
        };
        if let Some(s) = self.sticks.first_mut() {
            s.lean = lean;
        }
        input
    }

    /// Whether the game has a pointer (`c.pointer`).
    pub fn has_pointer(&self) -> bool {
        self.game.has_pointer()
    }

    /// Whether the system's arrow hides over the game's picture
    /// (`c.pointer = "own"` with no page over the game, as a window hides
    /// it), and the picture's own shape in its room (`c.camera.aspect`:
    /// bars round it).
    pub fn arrow_hidden(&self) -> bool {
        self.game.cfg.pointer == Some(crate::config::PointerStyle::Own) && !self.front.menu.is_open()
    }

    pub fn shape(&self) -> Option<f32> {
        self.game.cfg.camera.aspect
    }

    /// The mouse at `at` of the game's picture (its pixels from the top
    /// left; None: off it, the bars round a picture of its own shape too):
    /// moved there, the keyboard's player uses it; over a page the row
    /// under it is chosen.
    pub fn mouse_at(&mut self, at: Option<[f32; 2]>) {
        if !self.game.has_pointer() {
            return;
        }
        match at {
            Some(at) if self.mouse.move_to(at) => self.front.played_with = Device::Keyboard,
            Some(_) => {}
            None => self.mouse.at = None,
        }
        if let Some(row) = self.menu_row() {
            self.front.menu.choose(row);
        }
    }

    /// A mouse button went down at `at` of the picture: a page's (a binding
    /// CONTROLS waits for, the row under it confirmed, back), else the
    /// game's, with what the pointer is over now (`Mouse::down`).
    pub fn mouse_down(&mut self, button: sdl3::mouse::MouseButton, at: [f32; 2]) {
        if !self.game.has_pointer() {
            return;
        }
        self.mouse.at = Some(at);
        self.front.played_with = Device::Keyboard;
        let (row, mut commands) = (self.menu_row(), Vec::new());
        if self.front.menu_click(button, row, &mut self.settings, &mut commands) {
            return self.commands(commands);
        }
        if let Some(i) = crate::mouse::Mouse::index(button) {
            let answer = self.seat_pointer(self.seating.mouse_seat());
            self.mouse.down(i, answer);
        }
    }

    /// The row of the page open under the mouse, as last drawn.
    fn menu_row(&self) -> Option<usize> {
        self.front.row_at(self.mouse_place(), || self.game.menu_rows(&self.front.menu, &self.settings, self.aspect()))
    }

    pub fn mouse_up(&mut self, button: sdl3::mouse::MouseButton) {
        if let Some(i) = crate::mouse::Mouse::index(button) {
            self.mouse.up(i);
        }
    }

    /// The wheel turned `notches` over the picture (above 0: up): bound
    /// where CONTROLS waits for a key, nothing over another page, else the
    /// game's.
    pub fn wheel(&mut self, notches: f32) {
        let mut commands = Vec::new();
        if !self.game.has_pointer() {
            return;
        }
        if self.front.wheel_bind(notches, &mut self.settings, &mut commands) {
            return self.commands(commands);
        }
        if !self.front.menu.is_open() {
            self.front.played_with = Device::Keyboard;
            self.mouse.turned(notches);
        }
    }

    /// Whether the keyboard's player types (`p.state.typing`) while the
    /// game has the keys: the text typed, Backspace, Enter and Escape are
    /// the game's (`type_in`, `type_text`).
    pub fn typing(&self) -> bool {
        self.mouse.typing
    }

    pub fn type_in(&mut self, text: &str) {
        self.mouse.type_in(text);
    }

    pub fn type_text(&mut self, typed: crate::input::Typed) {
        self.mouse.type_text(typed);
    }

    /// A key went down for the game: seat 1 plays on the keyboard.
    pub fn keyboard_used(&mut self) {
        self.front.played_with = Device::Keyboard;
    }

    /// A key went down while the game has the keys, taken as a window takes
    /// it before the game hears it: a binding CONTROLS waits for; a page
    /// open steered (Escape a page back, from the first closing it; F5 and
    /// F9 stay the maker's); else, pressed and not held, a half's pause, the
    /// keyboard's start taking a seat or pausing (a game of several), the
    /// pause keys opening the pause page where the game does not take them
    /// (`pause_gives_way`; not on the game's own title), and with the run
    /// over the pause key or Enter going on (`ended`). Escape is the
    /// maker's otherwise (back to the file). Whether it was taken: the IDE
    /// does nothing more with it.
    pub fn key_down(&mut self, code: Scancode, repeat: bool) -> bool {
        let mut commands = Vec::new();
        let taken = self.key_taken(code, repeat, &mut commands);
        self.commands(commands);
        taken
    }

    fn key_taken(&mut self, code: Scancode, repeat: bool, commands: &mut Vec<Command>) -> bool {
        if self.front.rebind_key(code, repeat, &mut self.settings, commands) {
            return true;
        }
        if self.front.menu.is_open() {
            if matches!(code, Scancode::F5 | Scancode::F9) {
                return false;
            }
            self.front.menu_key(code, repeat, &mut self.settings, commands);
            return true;
        }
        if repeat || code == Scancode::Escape {
            return false;
        }
        let (name, over, fading, pauses) = (code.name(), self.game.game.outcome().is_some(), self.game.fading(), !self.game.on_title());
        let (front, game, settings, seating) = (&mut self.front, &self.game, &self.settings, &mut self.seating);
        let half = (1..=seating.seats()).filter(|&n| matches!(seating.keys(n), Keys::Left | Keys::Right)).find(|&n| settings.keys_of(seating.keys(n)).and_then(|b| b.action_of(name)) == Some(PAUSE));
        if pauses && !over && let Some(seat) = half {
            // A half's pause: its seat's (LEAVE offered after the first).
            if !fading {
                pause_for(front, game, true, (seat > 1).then_some(seat));
            }
            return true;
        }
        if pauses && !over && !seating.alone() && name == crate::seating::KEYBOARD_START && settings.keys.action_of(name).is_none() {
            // The keyboard (whole, or its right half): a seat taken, then
            // its pause.
            match seating.keyboard_start(planned_keyboard(settings)) {
                Start::Pause(seat) if !fading => pause_for(front, game, true, (seat > 1).then_some(seat)),
                Start::Join(_) => self.name_keys(),
                _ => {}
            }
            return true;
        }
        match play_key(code, settings, over, fading) {
            PlayKey::ToTitle => self.ended(),
            // The whole keyboard another seat's: LEAVE offered.
            PlayKey::Pause if pauses => pause_for(front, game, true, seating.keyboard_seat().filter(|&s| seating.keys(s) == Keys::Whole)),
            PlayKey::Pause | PlayKey::Game => return false,
        }
        true
    }

    /// A pad's button went down (`pad` its id, printing its buttons as
    /// `style` says; `plugged` the pads plugged in), taken as a window takes
    /// it: a binding CONTROLS waits for, a page open steered; else the run
    /// over going on (`ended`), START taking a seat or pausing (not on the
    /// game's own title), and a button bound to one of its seat's actions
    /// carrying the pointer's answers of this moment where the seat's stick
    /// moves it, as a click does (`App::stick_press`).
    pub fn pad_button(&mut self, pad: u32, style: PadStyle, button: Button, plugged: &[u32]) {
        self.front.pad_kind = style;
        let Some(name) = crate::controls::button_name(button) else { return };
        let mut commands = Vec::new();
        if self.front.menu_button(button, name, &mut self.settings, &mut commands) {
            return self.commands(commands);
        }
        self.front.played_with = Device::Gamepad;
        match play_button(button, name, &self.settings, self.game.game.outcome().is_some(), self.game.fading()) {
            PlayKey::Game => self.stick_press(pad, name),
            PlayKey::ToTitle => self.ended(),
            PlayKey::Pause if !self.game.on_title() => match self.seating.pad_start(pad, plugged, planned_pad(&self.settings, &self.named, pad)) {
                Start::Pause(seat) => pause_for(&mut self.front, &self.game, true, (seat > 1).then_some(seat)),
                Start::Join(_) => self.name_keys(),
                Start::Full => {}
            },
            PlayKey::Pause => {}
        }
    }

    /// A pad's button bound to one of its seat's actions, the seat's stick
    /// moving the pointer: it carries the pointer's answers of this moment.
    fn stick_press(&mut self, pad: u32, name: &str) {
        let Some(seat) = self.seating.pad_seat(pad) else { return };
        let bound = self.settings.pad_of(seat).action_of(name).is_some_and(|a| self.game.actions().contains(&a));
        if bound && seat <= self.sticks.len() {
            let answer = self.seat_pointer(seat);
            self.sticks[seat - 1].down(answer);
        }
    }

    /// A pad's stick or trigger moved to `value`: a page open steered by
    /// the left stick, a trigger bound where CONTROLS waits for one; else,
    /// leant far, seat 1 plays on its pad.
    pub fn axis(&mut self, axis: Axis, value: i16) {
        let mut commands = Vec::new();
        if !self.front.menu_axis(axis, value, &mut self.settings, &mut commands) && (value as f32 / 32767.0).abs() > 0.5 {
            self.front.played_with = Device::Gamepad;
        }
        self.commands(commands);
    }

    /// Nothing held or waiting any more (the window lost the focus, the
    /// game held).
    pub fn let_go(&mut self) {
        self.mouse.let_go();
        self.sticks.iter_mut().for_each(crate::mouse::StickPointer::let_go);
    }

    /// The pictures to come show the mouse at `at` of the picture (its
    /// pixels; `--shot --pointer`), as a frame shows it.
    pub fn show_pointer(&mut self, at: Option<[f32; 2]>) {
        self.mouse_at(at);
        self.pointer_shown();
    }

    /// The picture's shape as last drawn (1 before).
    fn aspect(&self) -> f32 {
        self.target.as_ref().map_or(1.0, |t| t.size[0] as f32 / t.size[1].max(1) as f32)
    }

    /// The HUD's measure, its width and height (`frame.width`, `height`).
    fn hud_size(&self) -> [f32; 2] {
        let layout = crate::ui::Layout::of(&self.game.cfg.ui, self.game.picture_aspect(self.aspect()));
        [layout.width, layout.height]
    }

    /// Where seat `seat`'s pointer is in the HUD's measure, as a window's
    /// (`mouse::seat_place`): the mouse's over the picture as last drawn,
    /// or its stick's.
    fn seat_place(&self, seat: usize) -> Option<[f32; 2]> {
        if !self.game.has_pointer() {
            return None;
        }
        let stick = self.sticks.get(seat - 1).and_then(|s| s.at);
        crate::mouse::seat_place(seat, self.seating.mouse_seat(), self.front.played_with, self.mouse_place(), stick, self.seat_pads.get(seat - 1).copied().unwrap_or(false))
    }

    /// Where the mouse is in the HUD's measure, over the picture as last
    /// drawn (`mouse::hud_at`); None off it.
    fn mouse_place(&self) -> Option<[f32; 2]> {
        let t = self.target.as_ref()?;
        crate::mouse::hud_at(self.mouse.at?, (t.size[0], t.size[1]), self.game.cfg.camera.aspect, self.hud_size())
    }

    /// Seat `seat`'s pointer answered where it is now.
    fn seat_pointer(&self, seat: usize) -> Option<crate::input::Pointer> {
        let [x, y] = self.seat_place(seat)?;
        Some(self.game.point(x, y, self.aspect()))
    }

    /// The pictures to come show the mouse seat's pointer where it is now
    /// (`frame.pointer`), in a game with one.
    fn pointer_shown(&mut self) {
        if self.game.has_pointer() {
            let (at, aspect) = (self.seat_place(self.seating.mouse_seat()), self.aspect());
            self.game.set_pointer(at, aspect);
        }
    }

    /// One a seat, where a pad's stick moves the pointer; none else.
    fn size_sticks(&mut self) {
        match self.game.cfg.pointer_stick {
            Some(_) => self.sticks.resize_with(self.seating.seats(), Default::default),
            None => self.sticks.clear(),
        }
    }

    /// Each seat's stick's pointer glided over `seconds`, while no page is
    /// over the game (`mouse::glide_sticks`).
    fn glide(&mut self, seconds: f32) {
        self.size_sticks();
        if self.sticks.is_empty() {
            return;
        }
        let mouse = self.mouse_place().filter(|_| self.front.played_with == Device::Keyboard);
        let speed = self.game.cfg.pointer_speed.unwrap_or(self.game.cfg.ui.height);
        let (mouse_seat, hud, playing) = (self.seating.mouse_seat(), self.hud_size(), !self.front.menu.is_open());
        if crate::mouse::glide_sticks(&mut self.sticks, mouse, mouse_seat, hud, speed, seconds, playing) {
            self.front.played_with = Device::Gamepad;
        }
    }

    /// A tick's input from seat 1's keys `held`: the mouse's buttons and
    /// wheel, its pointer answered now (a press's answers before them),
    /// the text typed, a pad's press, the aim at the pointer, taken once a
    /// tick as a window takes them (`App::ticks`).
    fn tick_input(&mut self, held: &crate::input::Input) -> crate::input::Input {
        let mut input = held.clone();
        let now = self.seat_pointer(1);
        if self.seating.mouse_seat() == 1 {
            self.mouse.add_to(&mut input, &self.settings.keys, self.game.actions(), now);
        } else {
            input.pointer = now.map(Box::new);
        }
        if let Some(s) = self.sticks.first_mut() {
            s.add_to(&mut input);
        }
        self.game.mouse_aim(&mut input, 1, self.aspect());
        input
    }

    /// The keys of seat 1 as the window's controls menu would show them
    /// (`keys_of`): the keyboard's or the pad's, as that seat plays here.
    pub fn keys(&self) -> &[(String, String)] {
        &self.keys
    }

    /// Names seat 1's keys anew (the game begun, a seat taken).
    fn name_keys(&mut self) {
        self.keys = keys_of(self.game.actions(), self.seating.keys(1), &self.settings);
    }

    /// What stops the game: a script's error (its file and line in it) or
    /// a file that would not load.
    pub fn trouble(&self) -> Option<String> {
        self.game.trouble()
    }

    /// Holds it still (the main area shows a file, the window is
    /// elsewhere): no ticks, no sound.
    pub fn hold(&mut self) {
        self.let_go();
        self.mouse.at = None;
        if std::mem::take(&mut self.mouse.typing) {
            self.mouse.typed.clear();
        }
        if self.last.take().is_some()
            && let Some(s) = &self.speaker
        {
            s.hold(true);
        }
    }

    /// What was saved since comes in (hot reload; held too, so a file
    /// mended takes away the error that stopped it): whether anything did.
    pub fn reload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let changed = self.watch.poll();
        let mut rebuild = false;
        let mut took = false;
        let mut failed = false;
        for path in &changed {
            let name = path.strip_prefix(&self.dir).unwrap_or(path).display().to_string();
            let mut this_failed = false;
            match self.game.reload(path) {
                Ok(Reloaded::World) => (rebuild, took) = (true, true),
                Ok(Reloaded::Nothing) => {}
                Ok(_) => took = true,
                Err(e) => {
                    (failed, this_failed) = (true, true);
                    let said = format!("{name} would not load: {e:#}");
                    self.failed.push(said.clone());
                    self.unloaded.insert(name.clone(), said);
                }
            }
            // Loaded again (or gone): no longer in the way.
            if !this_failed || !path.exists() {
                self.unloaded.remove(&name);
            }
        }
        // What a level is built from is built when it begins: if the game
        // would begin otherwise now, it begins anew (a number of the rules
        // alone goes on).
        if took && !failed && self.begins_otherwise() {
            return true;
        }
        // Code taken in: its functions keep locals of their own.
        if took {
            self.guard.reloaded(self.game.game.hidden());
        }
        if rebuild {
            self.game.options = self.settings.options();
            self.mask = self.game.face_mask();
            self.renderer.rebuild_all(device, queue, self.game.world_mut(), &self.mask);
        }
        !changed.is_empty()
    }

    /// Whether the game as the files have it now begins otherwise than the
    /// run being played did (the world's voxels, the first states, the
    /// settings that wait for a restart), begun as it began (`as_begun`):
    /// then it plays anew from the fresh game, said in OUTPUT. A game that
    /// would not load or begin leaves things as they are.
    fn begins_otherwise(&mut self) -> bool {
        let Ok(fresh) = self.as_begun() else { return false };
        if fresh.trouble().is_some() || (fresh.start_mark() == self.mark && self.game.cfg.restart_only(&fresh.cfg).is_empty()) {
            return false;
        }
        self.adopt(fresh);
        self.notes.push("The game's beginning changed: begun anew.".to_string());
        true
    }

    /// The game from its files anew, begun as the run being played began:
    /// on its own title, from the save it went on from, from its scenario,
    /// else as loaded (a run its title began with `world:new_game()` is the
    /// very run a fresh load begins).
    fn as_begun(&self) -> Result<Session<G>> {
        let mut fresh = self.load()?;
        let begun = if self.game.on_title() {
            fresh.begin_title()
        } else if let Some(text) = &self.game.started_from {
            fresh.start_from(text)
        } else if let Some(name) = &self.game.scenario {
            fresh.start_scenario(name)
        } else {
            Ok(())
        };
        begun.map(|()| fresh)
    }

    /// What the game said since last asked, for OUTPUT: what it did of
    /// itself, files that would not load, and what its script printed.
    pub fn said(&mut self) -> Vec<(super::output::Kind, String)> {
        // (A limit the pictures went past, lights left out and the like.)
        let limits = self.game.warnings().into_iter().chain(self.renderer.warnings());
        let notes = self.notes.drain(..).chain(limits).map(|n| (super::output::Kind::Note, n));
        let failed = self.failed.drain(..).chain(self.hidden.drain(..)).map(|f| (super::output::Kind::Error, f));
        let printed = self.game.printed().into_iter().map(|p| (super::output::Kind::Print, p));
        notes.chain(failed).chain(printed).collect()
    }

    /// A run the game's own title began in its place (`world:new_game()`,
    /// `world:load` there), as a window takes it: recorded from here, its
    /// locals looked at afresh. Whether one began.
    fn began(&mut self) -> bool {
        if !std::mem::take(&mut self.game.begun) {
            return false;
        }
        self.runs.start(&self.game);
        self.mark = self.game.start_mark();
        self.guard.begin(self.game.game.hidden());
        true
    }

    /// After a tick: what the game asked of the machine done as a window
    /// does it (`Front::asked`: SETTINGS or the pause page opened over it,
    /// QUIT, back to the title); once a second the hidden-state guard looks
    /// at the locals the game's functions keep and says which changed.
    fn after_tick(&mut self) {
        let asked = self.game.requests();
        if let Some(then) = self.front.asked(&asked) {
            self.command(then);
        }
        let game = &self.game.game;
        self.hidden.extend(self.guard.tick(|| game.hidden()));
    }

    /// A file of the game that would not load when it last changed (the
    /// first, if several), what was said of it; None once they all load.
    pub fn unloaded(&self) -> Option<String> {
        self.unloaded.values().next().cloned()
    }

    /// Whether the game asked to end (`world:quit()`, QUIT): STOP for it.
    pub fn quits(&self) -> bool {
        self.quit
    }

    /// Puts the run being played, or else the last one, in the game's
    /// `replays/` (F9); where it went.
    pub fn keep(&self) -> Result<PathBuf> {
        self.runs.keep(&self.game)
    }

    /// What the player plays with, for the ticks to come: the device seat
    /// 1 used last, the keyboard (and mouse) or a pad.
    fn controls(&mut self) {
        let controls = self.game.player_controls(self.front.played_with, self.front.pad_kind, &self.settings);
        self.game.set_controls(controls);
    }

    /// Steps `ticks` ticks with `input` at once (a shot, a test; not
    /// recorded), the game held while a page shows over it.
    pub fn step(&mut self, ticks: u32, input: &crate::input::Input) {
        self.controls();
        for _ in 0..ticks {
            if self.front.menu.is_open() {
                self.front.menu.update();
                continue;
            }
            self.game.step(input);
            self.began();
            self.after_tick();
        }
    }

    /// A frame: what changed on disk comes in, the ticks due are stepped
    /// with `input` (and the mouse's, the text typed, a pad's pointer), and the game is drawn into its texture, `size`
    /// pixels big (egui told of it). Its name to egui.
    pub fn frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer, size: [u32; 2], input: &crate::input::Input) -> egui::TextureId {
        let now = Instant::now();
        if self.last.is_none()
            && let Some(s) = &self.speaker
        {
            s.hold(false);
        }
        let elapsed = self.last.map_or(0.0, |l| (now - l).as_secs_f32());
        self.last = Some(now);
        self.reload(device, queue);
        self.rate.frame();
        // The frame's time owed to the game (all of it while its own song
        // keeps time with its clock and no page holds it: `app::TickClock`),
        // and the time a save in its ticks is stamped with.
        let held = self.front.menu.is_open();
        let keep = self.game.steers_music() && !held;
        self.clock.frame(elapsed, keep);
        self.game.set_clock(crate::saves::now());

        // The music (started over when it changed), the player's volume.
        let volume = self.music_volume();
        if let Some(speaker) = self.speaker.as_mut() {
            // A song is made before the mixer is locked: the audio device
            // waits on the lock. (A level's fade holds no song:
            // `Session::hand_music` says when.)
            self.game.music_ahead(self.clock.behind(0.0));
            let new = self.game.new_music();
            if let Some(mut mixer) = speaker.lock() {
                // A song begun now begins at the clock: nothing owed before
                // it. (Held under a page, as in a window.)
                if self.game.hand_music(&mut mixer, new, held) {
                    self.clock.settle();
                }
                mixer.set_music_volume(volume);
                mixer.set_drive(self.game.cfg.audio.drive);
                mixer.set_ceiling(self.game.cfg.audio.ceiling);
            }
        }

        // The pointer where it is now, for the pictures and the ticks.
        self.glide(elapsed);
        self.pointer_shown();
        // The ticks due (a page over the game: its animations alone).
        self.controls();
        for _ in 0..self.clock.due(keep) {
            if self.front.menu.is_open() {
                self.front.menu.update();
            } else {
                self.play_tick(input);
            }
        }
        self.draw(device, queue, egui, size)
    }

    /// The music's volume PLAY's speaker plays at: the game's times the
    /// player's (AUDIO's MUSIC).
    fn music_volume(&self) -> f32 {
        self.game.cfg.audio.music_volume * self.settings.music_gain()
    }

    /// A tick played as a frame plays it: seat 1's keys `held` with the
    /// mouse's, the text typed and a pad's pointer (`tick_input`), recorded
    /// (from its start, a run the title began), its sounds, what it asked.
    fn play_tick(&mut self, held: &crate::input::Input) {
        self.game.fades = true;
        let input = self.tick_input(held);
        self.game.step(&input);
        if !self.began() {
            self.runs.step(&self.game);
        }
        if !self.game.game.sim().sounds.is_empty()
            && let Some(mut mixer) = self.speaker.as_mut().and_then(|s| s.lock())
        {
            self.game.play_sounds(&mut mixer, self.settings.sound_gain());
        }
        self.after_tick();
    }

    /// Draws the game as it stands into its texture, `size` pixels big;
    /// its name to egui.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer, size: [u32; 2]) -> egui::TextureId {
        if std::mem::take(&mut self.remesh) {
            self.mask = self.game.face_mask();
            self.renderer.rebuild_all(device, queue, self.game.world_mut(), &self.mask);
        }
        self.mask = self.game.face_mask();
        let location = self.game.location();
        self.renderer.update_level(device, queue, self.game.world_mut(), &self.mask, location);
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("play"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.renderer.resize(device, size[0], size[1]);
            let id = match &self.target {
                Some(t) => {
                    egui.update_egui_texture_from_wgpu_texture(device, &view, wgpu::FilterMode::Linear, t.id);
                    t.id
                }
                None => egui.register_native_texture(device, &view, wgpu::FilterMode::Linear),
            };
            self.target = Some(Target { texture, view, size, id });
        }
        let target = self.target.as_ref().expect("made above");
        // The picture's shape: the game's own (bars round it), else the
        // play area's.
        let aspect = self.game.picture_aspect(size[0] as f32 / size[1].max(1) as f32);
        self.renderer.set_shape(device, self.game.cfg.camera.aspect);
        // What stops it is said in the IDE's red line over the picture
        // (the traceback in OUTPUT), not in the game's big letters.
        self.game.problem_shown = false;
        // A page over the game, its hints for what seat 1 played with last.
        let menu = &mut self.front.menu;
        (menu.device, menu.pad_style) = (self.front.played_with, self.front.pad_kind);
        let mut frame = self.game.menu_frame(&self.front.menu, &self.settings, self.clock.fraction(), aspect, Scene::Play);
        if self.settings.show_fps {
            crate::ui::fps(&mut frame.ui, self.rate.fps.round() as u32, &self.game.cfg.ui, aspect);
        }
        // A picture at another pitch or field of view (`frame:camera`) may
        // need faces the meshes left out: built anew with them, once.
        let faces = self.game.face_mask();
        if faces != self.mask {
            self.mask = faces;
            self.renderer.rebuild_all(device, queue, self.game.world_mut(), &self.mask);
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("play") });
        self.renderer.render(device, queue, &mut encoder, &target.view, &frame);
        queue.submit([encoder.finish()]);
        target.id
    }

    /// The game as last drawn, as a PNG (for the agent: what the player
    /// sees).
    pub fn picture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Vec<u8>> {
        let t = self.target.as_ref().ok_or_else(|| anyhow::anyhow!("the game has not been drawn yet"))?;
        let rgba = crate::headless::read_texture(device, queue, &t.texture)?;
        crate::headless::png_bytes(t.size[0], t.size[1], &rgba)
    }

    /// What its sound did, in a line for OUTPUT as it stops
    /// (`Speaker::report`: songs begun and played on, jumps of the music,
    /// the device running dry); none without a speaker.
    pub fn sound_said(&mut self) -> Option<String> {
        self.speaker.as_mut()?.report(true)
    }

    /// Ends it: its run over (kept if long enough), its texture freed.
    pub fn end(mut self, egui: &mut egui_wgpu::Renderer) {
        self.runs.end(&self.game);
        if let Some(t) = &self.target {
            egui.free_texture(&t.id);
        }
    }

    /// The game's state as one number (for tests).
    #[cfg(test)]
    pub fn state_hash(&self) -> u64 {
        self.game.game.state_hash()
    }
}

/// Whether the game in `dir` as its files are now begins (what PLAY finds
/// out first), without a window: the error if not.
pub fn begins<G: Game>(dir: &Path) -> Result<()> {
    let mut game = Session::<G>::load_with(dir, Slots::on_disk(dir))?;
    begin(&mut game, None, true)
}

/// What seat 1 does with which key, for the line under the game: moving
/// first (the arrows and WASD; a half of the keyboard its own four keys;
/// the pad's d-pad), then the game's actions in its order by their own
/// names in capitals, each with its key (or button) as the keyboard in use
/// labels it (`Controls::of`, the same the game's own menu shows); an
/// action nothing is bound to is left out.
pub fn keys_of(actions: &[&str], seat: crate::seating::Keys, settings: &Settings) -> Vec<(String, String)> {
    use crate::controls::Controls;
    use crate::seating::Keys;
    let (moving, controls) = match seat {
        Keys::Whole => ("ARROWS".to_string(), Controls::of(actions, Device::Keyboard, PadStyle::default(), settings)),
        Keys::Nothing => ("D-PAD".to_string(), Controls::of_pad(actions, PadStyle::default(), &settings.pad, settings)),
        half => {
            let bound = settings.keys_of(half);
            let ways: Vec<String> = ["up", "left", "down", "right"].iter().filter_map(|w| Some(settings.key_label(bound?.get(w).first()?))).collect();
            (if ways.len() == 4 { ways.join(" ") } else { "ARROWS".to_string() }, Controls::of_half(actions, half, settings))
        }
    };
    let mut line = vec![("MOVE".to_string(), moving)];
    line.extend(actions.iter().filter_map(|a| Some((a.to_uppercase(), controls.keys.get(*a).filter(|k| !k.is_empty())?.clone()))));
    line
}

/// The file and line a game's trouble names (`main.lua:12: ...`), if it
/// names one.
pub fn line_of(trouble: &str) -> Option<(String, usize)> {
    let at = trouble.find(".lua:")?;
    let start = trouble[..at].rfind(|c: char| !(c.is_ascii_alphanumeric() || "_./-".contains(c))).map_or(0, |i| i + 1);
    let file = &trouble[start..at + 4];
    let digits: String = trouble[at + 5..].chars().take_while(char::is_ascii_digit).collect();
    Some((file.to_string(), digits.parse().ok()?))
}

/// Lua's most common mistakes in plain words, as one short line under the
/// message (by its first line; None: no plain words for it).
pub fn plain_words(trouble: &str) -> Option<String> {
    let first = trouble.lines().next().unwrap_or("");
    // The type Lua names in "attempt to ... a number value".
    let what = |after: &str| first.split(after).nth(1).and_then(|r| r.split_whitespace().next()).map(str::to_string);
    let said = if first.contains("attempt to index a nil value") {
        "Something here is nil: a name misspelled, or not set yet.".to_string()
    } else if first.contains("attempt to call a nil value") {
        "A function here does not exist: its name misspelled, or not defined yet.".to_string()
    } else if first.contains("attempt to perform arithmetic on a nil value") {
        "A number here is nil: a name misspelled, or not set yet.".to_string()
    } else if first.contains("attempt to concatenate a nil value") {
        "Text joined with .. here is nil: a name misspelled, or not set yet.".to_string()
    } else if first.contains("attempt to compare") {
        "Two values here cannot be compared with < or >: one is nil, or a number meets a text.".to_string()
    } else if let Some(t) = what("attempt to index a ") {
        format!("Something here is a {t}, not a table: it has no fields to read.")
    } else if let Some(t) = what("attempt to call a ") {
        format!("Something called here is a {t}, not a function.")
    } else if let Some(t) = what("attempt to perform arithmetic on a ") {
        format!("Something counted with here is a {t}, not a number.")
    } else if first.contains("' expected") || first.contains("unexpected symbol") || first.contains("unfinished string") {
        "Lua cannot read this line: often an end, a bracket or a quote missing, here or above.".to_string()
    } else if first.contains("stack overflow") {
        "A function calls itself again and again without end.".to_string()
    } else {
        return None;
    };
    Some(said)
}

/// A game just loaded begun as PLAY begins it: from `scenario` if one is
/// given, else on the game's own title where `title` asks and it has one
/// (`c.title_screen.level`, as a window opens it), else the run as loaded.
/// A mistake in the game's code stops it, shown over it until mended; a
/// scenario that is not there is said at once.
fn begin<G: Game>(game: &mut Session<G>, scenario: Option<&str>, title: bool) -> Result<()> {
    let begun = match scenario {
        Some(name) => game.start_scenario(name),
        None if title && game.cfg.title_screen.level.is_some() => game.begin_title(),
        None => Ok(()),
    };
    match begun {
        Err(e) if game.game.halted().is_none() => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{Item, Page};

    /// Played, the game steps as `voxcade shot` steps it (the same state
    /// after the same ticks and keys) and is drawn into a texture of the
    /// area's size.
    #[test]
    fn play_steps_and_draws_into_its_panel() {
        let canvas = crate::headless::Canvas::new(320, 180, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = Path::new("games/voxkeep");
        let mut play = Play::<crate::Voxcade>::start(dir, device, queue, None, None).unwrap();
        let mut shot = Session::<crate::Voxcade>::load(dir).unwrap();
        let right = crate::input::Input { right: true, ..Default::default() };
        play.step(90, &right);
        for _ in 0..90 {
            shot.step(&right);
        }
        assert_eq!(play.state_hash(), shot.game.state_hash(), "stepped as a shot steps");
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        let id = play.draw(device, queue, &mut egui, [320, 180]);
        assert_eq!(play.draw(device, queue, &mut egui, [320, 180]), id, "the same texture while the size stays");
        assert_eq!(play.target.as_ref().map(|t| t.size), Some([320, 180]));
        assert!(play.trouble().is_none());
        play.end(&mut egui);
    }

    /// A game stopped by a script's error and held takes in the file
    /// mended: the error goes, without a tick played.
    #[test]
    fn a_held_game_takes_in_the_mended_script() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = crate::scratch::Scratch::new("play-mend");
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 32, 32, 8 }\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  local t = nil\n  local x = t.x\nend\n").unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        play.step(1, &crate::input::Input::default());
        let trouble = play.trouble().expect("stopped");
        assert_eq!(line_of(&trouble), Some(("main.lua".into(), 3)), "{trouble}");
        // The error in big letters over the picture is a window's: the
        // IDE's red line says it instead.
        let letters = |play: &Play<crate::Voxcade>| play.game.compose(0.0, 1.0, crate::session::Scene::Play, None).ui.len();
        assert!(letters(&play) > 0, "a window shows it over the game");
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        play.draw(device, queue, &mut egui, [64, 64]);
        assert_eq!(letters(&play), 0, "PLAY in the IDE does not");
        play.hold();
        // (A file counts as saved once it stood still for a look.)
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("main.lua"), "function on_tick()\nend\n").unwrap();
        let mut came = false;
        for _ in 0..12 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            came |= play.reload(device, queue);
        }
        assert!(came && play.trouble().is_none(), "mended: {:?}", play.trouble());
    }

    /// A game in a folder of its own with `main` as its `main.lua`: played
    /// a few ticks.
    fn played(name: &str, conf: &str, main: &str, device: &wgpu::Device, queue: &wgpu::Queue) -> (crate::scratch::Scratch, Play<crate::Voxcade>) {
        let dir = crate::scratch::Scratch::new(&format!("play-{name}"));
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        std::fs::write(dir.join("main.lua"), main).unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        play.step(5, &crate::input::Input::default());
        (dir, play)
    }

    /// Writes `main` over `main.lua` and waits for the play to take it in.
    fn saved(dir: &Path, file: &str, text: &str, play: &mut Play<crate::Voxcade>, device: &wgpu::Device, queue: &wgpu::Queue) {
        // (A file counts as saved once it stood still for a look.)
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join(file), text).unwrap();
        let mut came = false;
        for _ in 0..12 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            came |= play.reload(device, queue);
        }
        assert!(came, "{file} came in");
    }

    const CONF: &str = "function conf(c)\n  c.world.size = { 32, 32, 8 }\nend\n";

    fn floor(play: &Play<crate::Voxcade>) -> [u8; 3] {
        let w = &play.game.game.sim().world;
        let c = w.palette[w.get(glam::IVec3::new(3, 3, 0)) as usize];
        [c[0], c[1], c[2]]
    }

    fn noted(play: &mut Play<crate::Voxcade>) -> bool {
        play.said().iter().any(|(k, t)| matches!(k, super::super::output::Kind::Note) && t.contains("begun anew"))
    }

    const GREEN: &str = "function on_level_start(how)\n  level:fill(0, 0, 0, 31, 31, 0, \"#00ff00\")\nend\nfunction on_tick()\nend\n";

    /// How the level is built changed: the game being played begins anew
    /// and shows the new floor.
    #[test]
    fn a_changed_level_begins_the_game_anew() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let (dir, mut play) = played("level", CONF, GREEN, device, queue);
        assert_eq!(floor(&play), [0, 255, 0]);
        assert_eq!(play.game.game.sim().tick, 5);
        saved(&dir, "main.lua", &GREEN.replace("#00ff00", "#ff0000"), &mut play, device, queue);
        assert_eq!(floor(&play), [255, 0, 0], "the new floor");
        assert_eq!(play.game.game.sim().tick, 0, "begun anew");
        assert!(noted(&mut play));
    }

    /// A number of the rules alone: the run goes on.
    #[test]
    fn a_changed_rule_keeps_the_run_going() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let main = "function on_level_start(how)\n  level:fill(0, 0, 0, 31, 31, 0, \"#00ff00\")\nend\nfunction on_tick()\n  player.state.score = player.state.score + 1\nend\n";
        let (dir, mut play) = played("rule", CONF, main, device, queue);
        saved(&dir, "main.lua", &main.replace("+ 1", "+ 2"), &mut play, device, queue);
        assert_eq!(play.game.game.sim().tick, 5, "played on");
        assert_eq!(floor(&play), [0, 255, 0]);
        assert!(!noted(&mut play));
    }

    /// A setting that waits for a restart changed in `conf.lua`.
    #[test]
    fn a_restart_only_setting_begins_the_game_anew() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let (dir, mut play) = played("conf", CONF, GREEN, device, queue);
        saved(&dir, "conf.lua", "function conf(c)\n  c.world.size = { 32, 32, 8 }\n  c.player.spawn = { 10, 10 }\nend\n", &mut play, device, queue);
        assert_eq!(play.game.game.sim().tick, 0, "begun anew");
        assert!(noted(&mut play));
    }

    /// The run played is recorded: F9 puts it in the game's replays,
    /// and it plays through as it was played.
    #[test]
    fn a_run_played_is_kept_with_f9() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = crate::scratch::Scratch::new("play-keep");
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 32, 32, 8 }\nend\n").unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  if input.right then player.state.score = player.state.score + 1 end\nend\n").unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        let right = crate::input::Input { right: true, ..Default::default() };
        for _ in 0..4 {
            play.frame(device, queue, &mut egui, [64, 64], &right);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let path = play.keep().unwrap();
        assert!(path.starts_with(dir.join("replays")), "{}", path.display());
        let replay = crate::replay::Replay::load(&path).unwrap();
        assert!(replay.ticks > 0 && replay.score as u64 == replay.ticks, "{} ticks, {} points", replay.ticks, replay.score);
        assert!(matches!(crate::replay::play_through::<crate::Voxcade>(&dir, &replay).unwrap(), crate::replay::Verdict::Same));
        play.end(&mut egui);
    }

    /// What OUTPUT shows as a mistake.
    fn errors(play: &mut Play<crate::Voxcade>) -> Vec<String> {
        play.said().into_iter().filter(|(k, _)| matches!(k, super::super::output::Kind::Error)).map(|(_, t)| t).collect()
    }

    /// A local of the game's that its `on_tick` changes is named in OUTPUT
    /// within a second of play, once (`voxcade check` alone found it, and
    /// only in a game with replays); a game that keeps its state in states
    /// hears nothing, nor does one whose code is taken in while it plays.
    #[test]
    fn a_local_that_changes_while_playing_is_named() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let hidden = "local count = 0\nlocal function more(n) return n + 1 end\nfunction on_tick()\n  count = more(count)\nend\n";
        let (dir, mut play) = played("hidden", CONF, hidden, device, queue);
        play.step(55, &crate::input::Input::default());
        let said = errors(&mut play);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("'count' changed while the game ran") && said[0].contains("world.state"), "{}", said[0]);
        play.step(240, &crate::input::Input::default());
        assert!(errors(&mut play).is_empty(), "said once");
        let _ = std::fs::remove_dir_all(&dir);

        let clean = "local function more(n) return n + 1 end\nfunction on_tick()\n  world.state.count = more(world.state.count or 0)\nend\n";
        let (dir, mut play) = played("clean", CONF, clean, device, queue);
        play.step(55, &crate::input::Input::default());
        saved(&dir, "main.lua", &clean.replace("n + 1", "n + 2"), &mut play, device, queue);
        play.step(180, &crate::input::Input::default());
        assert!(errors(&mut play).is_empty(), "{:?}", errors(&mut play));
        assert!(play.game.game.sim().tick > 200, "played on");
    }

    /// The common mistakes have plain words, the rest none.
    #[test]
    fn common_mistakes_are_said_plainly() {
        let said = |t: &str| plain_words(t).unwrap_or_default();
        assert!(said("main.lua:148: attempt to index a nil value (global 'nothing_here')\nstack traceback:").starts_with("Something here is nil"));
        assert!(said("main.lua:3: attempt to call a nil value (method 'jmp')").starts_with("A function here does not exist"));
        assert!(said("main.lua:3: attempt to perform arithmetic on a nil value (field 'x')").starts_with("A number here is nil"));
        assert!(said("main.lua:3: attempt to compare number with nil").starts_with("Two values"));
        assert_eq!(said("main.lua:3: attempt to index a number value (local 'n')"), "Something here is a number, not a table: it has no fields to read.");
        assert!(said("main.lua:9: 'end' expected (to close 'function' at line 3) near <eof>").starts_with("Lua cannot read"));
        assert_eq!(plain_words("main.lua:1: the level 'x' is not there"), None);
    }

    /// A script's error names its file and line.
    #[test]
    fn a_games_trouble_names_its_line() {
        assert_eq!(line_of("main.lua:12: attempt to index a nil value (tick 30)"), Some(("main.lua".into(), 12)));
        assert_eq!(line_of("in levels/a.lua:3: boom"), Some(("levels/a.lua".into(), 3)));
        assert_eq!(line_of("conf.lua could not be read"), None);
    }

    /// The line under the game names moving and then each action of the game
    /// by its own name with its key as the keyboard in use labels it (a German
    /// keyboard's Y for the bindings' Z); an action with no key is left out.
    #[test]
    fn the_keys_line_names_the_actions_and_their_keys() {
        use crate::controls::Bindings;
        use crate::seating::Keys;
        let mut settings = Settings::default();
        settings.adopt(&["jump", "fire", "bomb"], &Bindings::new(&[("jump", &["Z"]), ("fire", &["X"])]), &Bindings::new(&[("jump", &["south"])]), false);
        settings.key_names.insert("Z".into(), "Y".into());
        let line = |seat| keys_of(&["jump", "fire", "bomb"], seat, &settings).into_iter().map(|(a, k)| format!("{a} {k}")).collect::<Vec<_>>();
        assert_eq!(line(Keys::Whole), ["MOVE ARROWS", "JUMP Y", "FIRE X"]);
        assert_eq!(line(Keys::Nothing), ["MOVE D-PAD", "JUMP A"], "a pad's buttons");
        assert_eq!(line(Keys::Left)[0].split(' ').count(), 5, "a half's own four keys: {:?}", line(Keys::Left));
    }

    /// A game with a pointer (`c.pointer = "own"`, a pad's left stick
    /// moving it too): a crate drawn by `frame:model`; what each tick's
    /// pointer is over, `click` and `next` held and the text typed noted.
    /// Its player types from tick 3 on.
    fn pointing(name: &str) -> crate::scratch::Scratch {
        let conf = "  c.pointer = \"own\"\n  c.pointer_stick = \"left\"\n  c.actions = { \"click\", \"next\" }\n  c.keys.click = { \"mouse_left\" }\n  c.keys.next = { \"wheel_down\" }\n  c.pad = { click = { \"south\" } }\n  c.things.crate = {}\n";
        let level = "format = \"voxcade level 1\"\nsize = [64, 64, 40]\n\n[[place]]\nname = \"start\"\nat = [24, 24, 4]\n";
        let main = r#"
function on_world_start()
  world.state.seen = {}
end
function on_level_start()
  level:spawn("crate", 32, 30, { z = 4 })
end
function on_tick()
  local p, t = input.pointer, input.typed
  table.insert(world.state.seen, string.format("%s|%s%s|%s", p and (p.thing and p.thing.kind or "-") or "nil",
    input:held("click") and "click" or "", input:held("next") and "next" or "", t and t.text or ""))
  if world.tick == 3 then player.state.typing = true end
end
function on_draw(frame)
  for _, t in ipairs(level:things()) do
    if t.kind == "crate" then frame:model("box.vvx", t) end
  end
end
"#;
        crate::lua::tests::pieces_folder(name, conf, level, main)
    }

    /// What the game noted each tick (`pointing`).
    fn seen(play: &mut Play<crate::Voxcade>) -> Vec<String> {
        let said = play.eval("return table.concat(world.state.seen, '#')").unwrap();
        said.trim_matches('"').split('#').map(str::to_string).collect()
    }

    /// The keyboard with no key down.
    struct NoKeys;

    impl crate::controls::KeyState for NoKeys {
        fn pressed(&self, _: sdl3::keyboard::Scancode) -> bool {
            false
        }
    }

    /// `pointing` played and drawn 320 by 180; where the crate's top is in
    /// the HUD's measure and in the picture's pixels.
    fn crate_in_view(name: &str, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer) -> (crate::scratch::Scratch, Play<crate::Voxcade>, [f32; 2], [f32; 2]) {
        let dir = pointing(name);
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        assert!(play.trouble().is_none(), "{:?}", play.trouble());
        play.draw(device, queue, egui, [320, 180]);
        let [w, h] = play.hud_size();
        let [x, y] = play.game.to_hud(glam::Vec3::new(32.5, 30.5, 7.9), play.aspect()).expect("in view");
        (dir, play, [x, y], [x / w * 320.0, y / h * 180.0])
    }

    /// The mouse over PLAY's picture is the game's pointer as a window's
    /// is: a click over the crate answers it as the window's path answers
    /// that place of the same picture (`Session::point`), with the answers
    /// of its moment though the mouse went on before the tick; a wheel's
    /// notch is a press and a release a tick apart; off the picture there
    /// is no pointer. Typed text reaches the game only while its player
    /// types and the game has the keys. The run so recorded replays as
    /// recorded.
    #[test]
    fn the_mouse_and_typing_in_play_are_the_windows() {
        let canvas = crate::headless::Canvas::new(320, 180, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        let (dir, mut play, hud, px) = crate_in_view("play-mouse", device, queue, &mut egui);
        let held = crate::input::Input::default();
        play.mouse_at(Some(px));
        play.mouse_down(sdl3::mouse::MouseButton::Left, px);
        play.mouse_up(sdl3::mouse::MouseButton::Left);
        play.mouse_at(Some([2.0, 2.0]));
        play.play_tick(&held);
        let met = |p: &Option<Box<crate::input::Pointer>>| p.as_ref().and_then(|p| p.level.as_ref()).and_then(|m| m.thing);
        let windows = play.game.point(hud[0], hud[1], play.aspect());
        assert!(windows.level.as_ref().is_some_and(|m| m.thing.is_some()), "the window's path meets the crate");
        assert_eq!(met(&play.game.played().input.pointer), windows.level.and_then(|m| m.thing), "answered as the window answers");
        play.wheel(-1.0);
        play.play_tick(&held);
        play.play_tick(&held);
        play.mouse_at(None);
        play.play_tick(&held);
        assert!(!play.typing(), "not typing yet");
        assert!(play.read(None, &[]) == held && !play.typing(), "the game without the keys: no typing");
        play.read(Some(&NoKeys), &[]);
        assert!(play.typing(), "its player types, the game has the keys");
        play.type_in("Hi\u{1}");
        play.play_tick(&held);
        play.read(None, &[]);
        assert!(!play.typing(), "the keys elsewhere: the text is the IDE's");
        assert_eq!(seen(&mut play), ["crate|click|", "-|next|", "-||", "nil||", "nil||Hi"]);
        let path = play.keep().unwrap();
        let replay = crate::replay::Replay::load(&path).unwrap();
        assert!(matches!(crate::replay::play_through::<crate::Voxcade>(&dir, &replay).unwrap(), crate::replay::Verdict::Same));
        play.end(&mut egui);
    }

    /// The pad's stick given to the pointer (`c.pointer_stick`) glides it
    /// in PLAY as in a window, from the picture's middle at the game's
    /// speed, seat 1 then playing on the pad; a pad's button bound to an
    /// action carries the answers of its moment; the mouse moved takes the
    /// pointer back. (The stick holds no directions: PLAY reads the pads
    /// with the game's `c.pointer_stick`, `controls::sticks`.)
    #[test]
    fn a_pads_pointer_in_play_glides_and_its_button_carries_the_answer() {
        let canvas = crate::headless::Canvas::new(320, 180, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        let (dir, mut play, hud, px) = crate_in_view("play-pad", device, queue, &mut egui);
        let [w, h] = play.hud_size();
        play.size_sticks();
        play.sticks[0].lean = [1.0, 0.0];
        play.glide(0.1);
        assert_eq!(play.front.played_with, Device::Gamepad, "leaning, seat 1 plays on its pad");
        let speed = play.game.cfg.ui.height;
        let at = play.seat_place(1).unwrap();
        assert!((at[0] - (w / 2.0 + speed * 0.1)).abs() < 1e-3 && (at[1] - h / 2.0).abs() < 1e-3, "{at:?}");
        play.sticks[0] = Default::default();
        play.sticks[0].at = Some(hud);
        play.pad_button(0, PadStyle::default(), Button::South, &[0]);
        play.sticks[0].at = Some([1.0, 1.0]);
        play.play_tick(&crate::input::Input::default());
        play.mouse_at(Some(px));
        play.mouse_at(Some([px[0] + 10.0, px[1]]));
        assert_eq!(play.front.played_with, Device::Keyboard, "the mouse moved: its pointer again");
        assert_eq!(seen(&mut play)[0], "crate||", "the button's press carried the crate");
        let _ = dir;
        play.end(&mut egui);
    }

    /// AGAIN (`restart`) begins the game from the start again, from its
    /// scenario if it was played from one.
    #[test]
    fn again_begins_from_the_scenario_again() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let dir = crate::scratch::Scratch::new("play-again");
        std::fs::create_dir_all(dir.join("scenarios")).unwrap();
        std::fs::write(dir.join("conf.lua"), CONF).unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  player.state.score = player.state.score + 1\nend\n").unwrap();
        std::fs::write(dir.join("scenarios/late.lua"), "return {\n  about = \"late\",\n  setup = function()\n    player.state.score = 1000\n  end,\n}\n").unwrap();
        let mut play = Play::<crate::Voxcade>::start(&dir, device, queue, None, Some("late")).unwrap();
        play.step(5, &crate::input::Input::default());
        assert_eq!(play.game.game.sim().tick, 5);
        play.restart().unwrap();
        assert_eq!(play.game.game.sim().tick, 0, "from the start");
        play.step(3, &crate::input::Input::default());
        assert_eq!(play.scenario.as_deref(), Some("late"), "the scenario again");
        let score = play.eval("player.state.score").unwrap();
        assert!(score.contains("1003"), "begun from the scenario's 1000, three ticks on: {score}");
    }

    /// A game with a title of its own (`lua::tests::title::title_folder`):
    /// each tick counted, a point a tick while right is held, and what
    /// `world.state.ask` says asked of the machine (a console line sets it).
    const TITLED: &str = r#"
function on_level_start(how)
  world.state.how = how
end
function on_tick()
  world.state.ticks = (world.state.ticks or 0) + 1
  if input.right then player.state.score = player.state.score + 1 end
  local ask = world.state.ask
  world.state.ask = nil
  if ask == "new" then world:new_game()
  elseif ask == "load" then world:load(1)
  elseif ask == "title" then world:to_title()
  elseif ask == "quit" then world:quit()
  elseif ask == "win" then world:win()
  elseif ask then world:open(ask) end
end
"#;

    /// `TITLED` with these lines more in `conf.lua`, played in PLAY: its
    /// settings and saves kept in `kept`, a scratch config, not the
    /// player's.
    fn titled(name: &str, conf: &str, kept: &Path, device: &wgpu::Device, queue: &wgpu::Queue) -> (crate::scratch::Scratch, Play<crate::Voxcade>) {
        let dir = crate::lua::tests::title::title_folder(name, TITLED);
        let text = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        std::fs::write(dir.join("conf.lua"), text.replace("\nend", &format!("\n{conf}end"))).unwrap();
        let play = Play::<crate::Voxcade>::start(&dir, device, queue, None, None).unwrap();
        (dir, in_scratch(play, kept))
    }

    /// `play` keeping its settings and saves in the scratch folder `kept`.
    fn in_scratch(mut play: Play<crate::Voxcade>, kept: &Path) -> Play<crate::Voxcade> {
        play.settings_path = kept.join("settings.toml");
        play.on_disk = false;
        play.game.adopt_slots(Default::default());
        play
    }

    /// `line` run in the game, then a tick played as a frame plays it.
    fn asked(play: &mut Play<crate::Voxcade>, line: &str) {
        play.eval(line).unwrap();
        play.play_tick(&crate::input::Input::default());
    }

    /// Ticks played as a frame plays them until `done` (a level change's
    /// fade seen out, as a window shows it), a few seconds at most.
    fn until(play: &mut Play<crate::Voxcade>, done: impl Fn(&Play<crate::Voxcade>) -> bool) {
        for _ in 0..300 {
            if done(play) {
                return;
            }
            play.play_tick(&crate::input::Input::default());
        }
        panic!("not done in 300 ticks: {:?}", play.trouble());
    }

    /// The run the title began with `world:new_game()`, its fade seen out.
    fn new_game(play: &mut Play<crate::Voxcade>) {
        asked(play, "world.state.ask = 'new'");
        until(play, |p| !p.game.on_title() && !p.game.fading());
    }

    fn state(play: &mut Play<crate::Voxcade>, key: &str) -> String {
        play.eval(&format!("world.state.{key}")).unwrap()
    }

    /// PLAY begins a game with a title of its own on it, as a window opens
    /// it, and keeps no run there; `world:new_game()` begins the run,
    /// recorded from there (F9 keeps it and it replays as played), the
    /// title's console line no part of it; AGAIN (F5) goes back to the
    /// title, PLAY's start. A game without one begins its run at once.
    #[test]
    fn play_begins_on_the_games_own_title_and_new_game_begins_the_run() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let kept = crate::scratch::Scratch::new("play-title-kept");
        let (dir, mut play) = titled("play-title", "", &kept, device, queue);
        assert!(play.game.on_title() && !play.front.menu.is_open(), "the game's own title, no page over it");
        let right = crate::input::Input { right: true, ..Default::default() };
        for _ in 0..5 {
            play.play_tick(&right);
        }
        assert!(play.keep().is_err(), "the title counts for nothing");
        new_game(&mut play);
        assert_eq!(play.game.game.sim().tick, 0, "the run begun, its first tick to come");
        for _ in 0..30 {
            play.play_tick(&right);
        }
        let replay = crate::replay::Replay::load(&play.keep().unwrap()).unwrap();
        assert!(replay.ticks == 30 && replay.score == 30, "{} ticks, {} points", replay.ticks, replay.score);
        assert!(matches!(crate::replay::play_through::<crate::Voxcade>(&dir, &replay).unwrap(), crate::replay::Verdict::Same));
        play.restart().unwrap();
        assert!(play.game.on_title(), "AGAIN: the title again");

        let plain = crate::lua::tests::levels_folder("play-plain", TITLED, &[("a", &[])]);
        let play = Play::<crate::Voxcade>::start(&plain, device, queue, None, None).unwrap();
        assert!(!play.game.on_title() && play.game.game.sim().tick == 0, "no title of its own: the run at once");
    }

    /// `world:to_title()` in a run, and a run's end with Enter, go back to
    /// the game's own title as in a window, the run ended; `world:quit()`
    /// stops PLAY. In a game without a title of its own `world:to_title()`
    /// stops PLAY too (it shows no engine's title).
    #[test]
    fn to_title_and_a_runs_end_go_back_to_the_title_and_quit_stops() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let kept = crate::scratch::Scratch::new("play-back-kept");
        let (_dir, mut play) = titled("play-back", "", &kept, device, queue);
        new_game(&mut play);
        play.step(3, &crate::input::Input::default());
        asked(&mut play, "world.state.ask = 'title'");
        assert!(play.game.on_title() && !play.quits(), "back on the title");
        assert_eq!(state(&mut play, "how"), "\"start\"", "begun afresh");
        new_game(&mut play);
        asked(&mut play, "world.state.ask = 'win'");
        until(&mut play, |p| p.game.game.outcome().is_some());
        assert!(play.key_down(Scancode::Return, false) && play.game.on_title(), "the run over, Enter: the title");
        asked(&mut play, "world.state.ask = 'quit'");
        assert!(play.quits(), "QUIT stops PLAY");

        let plain = crate::lua::tests::levels_folder("play-back-plain", TITLED, &[("a", &[])]);
        let mut play = Play::<crate::Voxcade>::start(&plain, device, queue, None, None).unwrap();
        asked(&mut play, "world.state.ask = 'title'");
        assert!(play.quits(), "no title to go back to: PLAY stops");
    }

    /// `world:open("settings")` shows SETTINGS over the game in PLAY, the
    /// game held while it shows; on CONTROLS a key bound and RESET put the
    /// game's bindings back, each saved as a window saves them; Escape
    /// goes back a page and from the first closes it, the game going on;
    /// with no page Escape is the IDE's (back to the file). FULLSCREEN and
    /// FRAME RATE, the IDE window's, are not on VIDEO.
    #[test]
    fn settings_open_over_play_and_reset_puts_the_bindings_back() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let kept = crate::scratch::Scratch::new("play-settings-kept");
        let (_dir, mut play) = titled("play-settings", "  c.actions = { \"jump\" }\n  c.keys.jump = { \"Z\" }\n", &kept, device, queue);
        asked(&mut play, "world.state.ask = 'settings'");
        assert_eq!(play.front.menu.page(), Some(Page::Settings), "SETTINGS over the title");
        let ticks = state(&mut play, "ticks");
        play.step(10, &crate::input::Input::default());
        assert_eq!(state(&mut play, "ticks"), ticks, "held while it shows");
        let video = play.front.menu.items(Page::Video, &play.settings);
        assert!(!video.contains(&Item::Fullscreen) && !video.contains(&Item::FrameRate) && video.contains(&Item::Shadows), "{video:?}");
        let key = |play: &mut Play<crate::Voxcade>, code| assert!(play.key_down(code, false), "{code:?} taken by the page");
        for code in [Scancode::Down, Scancode::Down, Scancode::Return] {
            key(&mut play, code);
        }
        assert_eq!(play.front.menu.page(), Some(Page::Controls));
        key(&mut play, Scancode::Return);
        assert_eq!(play.front.menu.rebinding, Some("jump"));
        key(&mut play, Scancode::K);
        assert_eq!(play.settings.keys.action_of("K"), Some("jump"));
        let saved = || std::fs::read_to_string(kept.join("settings.toml")).unwrap();
        assert!(saved().contains("\"K\""), "saved: {}", saved());
        // RESET: the second row from the end.
        for code in [Scancode::Up, Scancode::Up, Scancode::Return] {
            key(&mut play, code);
        }
        assert_eq!((play.settings.keys.action_of("K"), play.settings.keys.action_of("Z")), (None, Some("jump")), "the game's own again");
        assert!(!saved().contains("\"K\""), "saved: {}", saved());
        key(&mut play, Scancode::Escape);
        assert_eq!(play.front.menu.page(), Some(Page::Settings), "a page back");
        key(&mut play, Scancode::Escape);
        assert!(!play.front.menu.is_open(), "closed from the first");
        play.step(2, &crate::input::Input::default());
        assert_ne!(state(&mut play, "ticks"), ticks, "the game goes on");
        assert!(!play.key_down(Scancode::Escape, false), "no page: Escape is the IDE's");
    }

    /// AUDIO's MUSIC turned down in PLAY reaches its speaker at once: the
    /// song plays at the game's volume times the player's.
    #[test]
    fn a_volume_changed_in_play_reaches_its_speaker() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let kept = crate::scratch::Scratch::new("play-volume-kept");
        let dir = crate::lua::tests::title::title_folder("play-volume", "function on_level_start(how)\n  music:play('song', { from = 'now' })\nend\nfunction on_tick()\n  if world.tick == 2 then world:open('settings') end\nend\n");
        std::fs::create_dir_all(dir.join("music")).unwrap();
        std::fs::write(dir.join("music/song.mml"), "#tempo 120\n@s sine\nA: @s o4 c1\n").unwrap();
        let text = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        std::fs::write(dir.join("conf.lua"), text.replace("\nend", "\n  c.audio.music = { \"music/song.mml\" }\nend")).unwrap();
        let play = Play::<crate::Voxcade>::start(&dir, device, queue, Some(crate::audio::Speaker::without_device()), None).unwrap();
        let mut play = in_scratch(play, &kept);
        let mut egui = egui_wgpu::Renderer::new(device, FORMAT, egui_wgpu::RendererOptions::default());
        let heard = |play: &mut Play<crate::Voxcade>| play.speaker.as_mut().unwrap().lock().unwrap().music_volume();
        // (A song asked in on_level_start begins with the first tick.)
        play.step(1, &crate::input::Input::default());
        play.frame(device, queue, &mut egui, [64, 64], &crate::input::Input::default());
        let before = heard(&mut play).expect("the song plays");
        play.step(2, &crate::input::Input::default());
        assert_eq!(play.front.menu.page(), Some(Page::Settings));
        for code in [Scancode::Return, Scancode::Left, Scancode::Left] {
            assert!(play.key_down(code, false));
        }
        play.frame(device, queue, &mut egui, [64, 64], &crate::input::Input::default());
        let after = heard(&mut play).unwrap();
        assert!(after < before && (after - play.music_volume()).abs() < 1e-6, "{before} then {after}");
        play.end(&mut egui);
    }

    /// `world:load(slot)` on the title goes on from the save, which SAVE on
    /// the pause page (`world:open("pause")` in a run) put there; the pause
    /// key opens it too, in a game that does not take it.
    #[test]
    fn the_pause_page_saves_and_load_on_the_title_goes_on_from_it() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let kept = crate::scratch::Scratch::new("play-save-kept");
        let (_dir, mut play) = titled("play-save", "", &kept, device, queue);
        new_game(&mut play);
        play.eval("world.state.kept = 7").unwrap();
        asked(&mut play, "world.state.ask = 'pause'");
        assert_eq!(play.front.menu.page(), Some(Page::Pause));
        // RESUME, SAVE: its first slot.
        for code in [Scancode::Down, Scancode::Return, Scancode::Return] {
            assert!(play.key_down(code, false));
        }
        assert!(play.game.slots.read(1).is_some(), "saved into slot 1");
        assert_eq!(play.front.menu.page(), Some(Page::Pause), "back on the pause page");
        assert!(play.key_down(Scancode::Escape, false) && !play.front.menu.is_open());
        assert!(play.key_down(Scancode::P, false) && play.front.menu.page() == Some(Page::Pause), "P pauses");
        play.key_down(Scancode::Escape, false);
        asked(&mut play, "world.state.ask = 'title'");
        assert!(play.game.on_title());
        asked(&mut play, "world.state.ask = 'load'");
        until(&mut play, |p| !p.game.on_title());
        assert!(play.game.started_from.is_some());
        assert!(state(&mut play, "kept").contains('7'), "gone on from the save");
    }

    /// A rule changed during a run the title began keeps the run, as in any
    /// run; a changed beginning begins that run anew, never the title.
    /// PLAY SCENARIO begins in the scenario, not on the title.
    #[test]
    fn a_file_saved_in_a_run_the_title_began_keeps_it_a_run() {
        let canvas = crate::headless::Canvas::new(64, 64, false).unwrap();
        let (device, queue, _) = canvas.gpu();
        let kept = crate::scratch::Scratch::new("play-reload-kept");
        let (dir, mut play) = titled("play-reload", "", &kept, device, queue);
        new_game(&mut play);
        play.step(5, &crate::input::Input::default());
        saved(&dir, "main.lua", &TITLED.replace("score + 1", "score + 2"), &mut play, device, queue);
        assert!(!play.game.on_title() && play.game.game.sim().tick == 5, "played on");
        assert!(!noted(&mut play));
        saved(&dir, "main.lua", &TITLED.replace("world.state.how = how", "world.state.how = how .. '!'"), &mut play, device, queue);
        assert!(noted(&mut play) && !play.game.on_title() && play.game.game.sim().tick == 0, "the run begun anew");

        std::fs::create_dir_all(dir.join("scenarios")).unwrap();
        std::fs::write(dir.join("scenarios/late.lua"), "return {\n  about = \"late\",\n  setup = function()\n    world.state.late = true\n  end,\n}\n").unwrap();
        let play = Play::<crate::Voxcade>::start(&dir, device, queue, None, Some("late")).unwrap();
        assert!(!play.game.on_title() && play.game.scenario.is_some(), "the scenario, not the title");
    }
}

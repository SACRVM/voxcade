//! Play in the IDE's main area: the game straight from its folder, no
//! title and no menus, stepped at the simulation's fixed tick and drawn
//! into a texture of its own on the IDE's GPU, as big as the area. Its
//! sounds and music come out of the player's speaker, its keys and pads
//! are bound as the player set them (`controls::read_input`, as `voxcade
//! run` reads them). It holds still while the main area shows a file or
//! the window is elsewhere; a file saved comes in at once (hot reload),
//! and a script's error halts it until mended (`trouble`). A local of the
//! game's that changes while it plays, state no snapshot keeps, is named
//! in OUTPUT within a second (`lua::hidden::Watch`). Its runs are
//! recorded as `voxcade run` records them (`replay::Runs`: kept in the
//! config folder from 10 s on, F9 puts one in the game's `replays/`; a
//! run with a file saved during it is not kept). In a game with a pointer
//! the mouse over its picture is the pointer, as a window's mouse is, its
//! buttons and wheel the keys the game binds, a pad's stick moving it where
//! `c.pointer_stick` says (`crate::mouse`, the window's own); the text a
//! player types goes to the game while it has the keys.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::controls::{Device, PadStyle};
use crate::game::Game;
use crate::saves::Slots;
use crate::menu::Menu;
use crate::render::Renderer;
use crate::replay::Runs;
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

/// A game being played.
pub struct Play<G: Game> {
    dir: PathBuf,
    game: Session<G>,
    renderer: Renderer,
    mask: crate::mesh::FaceMask,
    target: Option<Target>,
    settings: Settings,
    watch: FolderWatch,
    speaker: Option<crate::audio::Speaker>,
    runs: Runs,
    /// Files that would not load since last asked (for OUTPUT).
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
    /// The game asked to end (`world:quit()`): PLAY stops it, never the IDE.
    quit: bool,
    /// The mouse over the game's picture (its place in the picture's pixels
    /// from the top left, None off it), its buttons and wheel, and the text
    /// typed while the keyboard's player types; each seat's pointer its
    /// pad's stick moves (`c.pointer_stick`), seat 1's first: as a window's.
    mouse: crate::mouse::Mouse,
    sticks: Vec<crate::mouse::StickPointer>,
    /// What seat 1 played with last (`controls.device`), and which seats
    /// have a pad, as read last.
    played_with: Device,
    seat_pads: Vec<bool>,
}

impl<G: Game> Play<G> {
    /// The game in `dir`, its sound through `speaker` (none in tests),
    /// begun from `scenario` if one is given.
    pub fn start(dir: &Path, device: &wgpu::Device, queue: &wgpu::Queue, speaker: Option<crate::audio::Speaker>, scenario: Option<&str>) -> Result<Self> {
        let mut game = Session::<G>::load_with(dir, Slots::on_disk(dir))?;
        begin(&mut game, scenario)?;
        let mut settings = Settings::load(&crate::settings::path_for(dir));
        settings.adopt(game.actions(), &game.default_keys(), &game.default_pad());
        settings.offer_languages(&game.cfg.languages);
        settings.system_languages = crate::settings::system_languages();
        crate::controls::label_keys(&mut settings);
        game.options = settings.options();
        let mut renderer = Renderer::new(device, FORMAT, 64, 64);
        let mask = game.face_mask();
        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
        let mut runs = Runs::new(dir);
        runs.start(&game);
        let seating = crate::seating::Seating::new(game.cfg.players);
        let mark = game.start_mark();
        let mut play = Self { dir: dir.to_path_buf(), game, renderer, mask, target: None, settings, watch: FolderWatch::new(dir), speaker, runs, failed: Vec::new(), unloaded: Default::default(), clock: Default::default(), last: None, scenario: scenario.map(str::to_string), seating, mark, notes: Vec::new(), guard: Default::default(), hidden: Vec::new(), keys: Vec::new(), quit: false, mouse: Default::default(), sticks: Vec::new(), played_with: Device::Keyboard, seat_pads: Vec::new() };
        play.guard.begin(play.game.game.hidden());
        play.name_keys();
        Ok(play)
    }

    /// The game from its files again (F5), as it starts.
    pub fn restart(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
        let mut fresh = Session::<G>::load_with(&self.dir, Slots::on_disk(&self.dir))?;
        begin(&mut fresh, self.scenario.as_deref())?;
        self.adopt(fresh, device, queue);
        // Played again from its start, its song too.
        self.game.restart_music();
        Ok(())
    }

    /// `fresh`, a game just begun, plays in place of the one played.
    fn adopt(&mut self, fresh: Session<G>, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.runs.end(&self.game);
        self.game = fresh;
        self.mark = self.game.start_mark();
        self.guard.begin(self.game.game.hidden());
        self.runs.start(&self.game);
        self.game.options = self.settings.options();
        self.mask = self.game.face_mask();
        self.renderer.rebuild_all(device, queue, self.game.world_mut(), &self.mask);
        self.name_keys();
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
        let typing = keys.is_some() && self.game.typing(mouse_seat);
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
    /// (`c.pointer = "own"`, as a window hides it), and the picture's own
    /// shape in its room (`c.camera.aspect`: bars round it).
    pub fn arrow_hidden(&self) -> bool {
        self.game.cfg.pointer == Some(crate::config::PointerStyle::Own)
    }

    pub fn shape(&self) -> Option<f32> {
        self.game.cfg.camera.aspect
    }

    /// The mouse at `at` of the game's picture (its pixels from the top
    /// left; None: off it, the bars round a picture of its own shape too):
    /// moved there, the keyboard's player uses it.
    pub fn mouse_at(&mut self, at: Option<[f32; 2]>) {
        if !self.game.has_pointer() {
            return;
        }
        match at {
            Some(at) if self.mouse.move_to(at) => self.played_with = Device::Keyboard,
            Some(_) => {}
            None => self.mouse.at = None,
        }
    }

    /// A mouse button went down at `at` of the picture: the game's, with
    /// what the pointer is over now (`Mouse::down`).
    pub fn mouse_down(&mut self, button: sdl3::mouse::MouseButton, at: [f32; 2]) {
        if !self.game.has_pointer() {
            return;
        }
        self.mouse.at = Some(at);
        self.played_with = Device::Keyboard;
        if let Some(i) = crate::mouse::Mouse::index(button) {
            let answer = self.seat_pointer(self.seating.mouse_seat());
            self.mouse.down(i, answer);
        }
    }

    pub fn mouse_up(&mut self, button: sdl3::mouse::MouseButton) {
        if let Some(i) = crate::mouse::Mouse::index(button) {
            self.mouse.up(i);
        }
    }

    /// The wheel turned `notches` over the picture (above 0: up).
    pub fn wheel(&mut self, notches: f32) {
        if self.game.has_pointer() {
            self.played_with = Device::Keyboard;
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
        self.played_with = Device::Keyboard;
    }

    /// A pad leaned or pressed: seat 1 plays on it (where it is seat 1's).
    pub fn pad_used(&mut self) {
        self.played_with = Device::Gamepad;
    }

    /// A pad's button went down (`pad`, its id): one bound to one of its
    /// seat's actions carries the pointer's answers of this moment where
    /// the seat's stick moves it, as a click does (`App::stick_press`).
    pub fn pad_button(&mut self, pad: u32, button: sdl3::gamepad::Button) {
        self.pad_used();
        let Some(name) = crate::controls::button_name(button) else { return };
        let Some(seat) = self.seating.pad_seat(pad) else { return };
        let bound = self.settings.pad_of(seat).action_of(name).is_some_and(|a| self.game.actions().contains(&a));
        if bound && seat <= self.sticks.len() {
            let answer = self.seat_pointer(seat);
            self.sticks[seat - 1].down(answer);
        }
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
        crate::mouse::seat_place(seat, self.seating.mouse_seat(), self.played_with, self.mouse_place(), stick, self.seat_pads.get(seat - 1).copied().unwrap_or(false))
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

    /// Each seat's stick's pointer glided over `seconds` (`mouse::glide_sticks`).
    fn glide(&mut self, seconds: f32) {
        self.size_sticks();
        if self.sticks.is_empty() {
            return;
        }
        let mouse = self.mouse_place().filter(|_| self.played_with == Device::Keyboard);
        let speed = self.game.cfg.pointer_speed.unwrap_or(self.game.cfg.ui.height);
        let (mouse_seat, hud) = (self.seating.mouse_seat(), self.hud_size());
        if crate::mouse::glide_sticks(&mut self.sticks, mouse, mouse_seat, hud, speed, seconds, true) {
            self.played_with = Device::Gamepad;
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

    /// A pad's START: a seat taken (a pad no seat has), else nothing.
    pub fn pad_start(&mut self, pad: u32, plugged: &[u32]) {
        if !self.seating.alone()
            && let crate::seating::Start::Join(seat) = self.seating.pad_start(pad, plugged, None)
        {
            self.game.set_seat(seat, Some(crate::input::Input::default()));
            self.name_keys();
        }
    }

    /// The keyboard's start key (unless the game binds it): a seat taken,
    /// the keyboard shared (seat 1 plays on it here).
    pub fn keyboard_start(&mut self) {
        let name = crate::seating::KEYBOARD_START;
        if !self.seating.alone()
            && self.settings.keys.action_of(name).is_none()
            && let crate::seating::Start::Join(seat) = self.seating.keyboard_start(None)
        {
            self.game.set_seat(seat, Some(crate::input::Input::default()));
            self.name_keys();
        }
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
        if took && !failed && self.begins_otherwise(device, queue) {
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
    /// settings that wait for a restart): then it plays anew from the
    /// fresh game, said in OUTPUT. A game that would not load or begin
    /// leaves things as they are.
    fn begins_otherwise(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let Ok(mut fresh) = Session::<G>::load_with(&self.dir, Slots::on_disk(&self.dir)) else { return false };
        if begin(&mut fresh, self.scenario.as_deref()).is_err() || fresh.trouble().is_some() {
            return false;
        }
        if fresh.start_mark() == self.mark && self.game.cfg.restart_only(&fresh.cfg).is_empty() {
            return false;
        }
        self.adopt(fresh, device, queue);
        self.notes.push("The game's beginning changed: begun anew.".to_string());
        true
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

    /// A tick played: once a second the hidden-state guard looks at the
    /// locals the game's functions keep and says which changed.
    /// After a tick: a run the game's own title began (`world:new_game()`)
    /// recorded from here, its locals looked at afresh, its song;
    /// `world:quit()` heard (the shell stops the game, `quits`), a page it
    /// asks for said in OUTPUT; the
    /// hidden-state guard.
    fn guard_tick(&mut self) {
        if std::mem::take(&mut self.game.begun) {
            self.runs.start(&self.game);
            self.mark = self.game.start_mark();
            self.guard.begin(self.game.game.hidden());
        }
        for asked in self.game.requests() {
            match asked {
                // Back to the title: PLAY has none, it stops as at QUIT.
                crate::game::Request::Quit | crate::game::Request::ToTitle => self.quit = true,
                // PLAY has no menus: said, so a row that seems to do nothing is explained.
                crate::game::Request::Open(page) => self.notes.push(format!("The game opens the engine's {} page here: shown when it plays in its own window, not in PLAY.", page.to_uppercase())),
            }
        }
        let game = &self.game.game;
        self.hidden.extend(self.guard.tick(|| game.hidden()));
    }

    /// A file of the game that would not load when it last changed (the
    /// first, if several), what was said of it; None once they all load.
    pub fn unloaded(&self) -> Option<String> {
        self.unloaded.values().next().cloned()
    }

    /// Whether the game asked to end (`world:quit()`): STOP for it.
    pub fn quits(&self) -> bool {
        self.quit
    }

    /// Whether the run is won or lost (PRESS START: the IDE has no title
    /// to go back to, so it starts over).
    pub fn over(&self) -> bool {
        self.game.game.outcome().is_some()
    }

    /// Puts the run being played, or else the last one, in the game's
    /// `replays/` (F9); where it went.
    pub fn keep(&self) -> Result<PathBuf> {
        self.runs.keep(&self.game)
    }

    /// What the player plays with, for the ticks to come: the device seat
    /// 1 used last, the keyboard (and mouse) or a pad.
    fn controls(&mut self) {
        let controls = self.game.player_controls(self.played_with, PadStyle::default(), &self.settings);
        self.game.set_controls(controls);
    }

    /// Steps `ticks` ticks with `input` at once (a shot, a test; not
    /// recorded).
    pub fn step(&mut self, ticks: u32, input: &crate::input::Input) {
        self.controls();
        for _ in 0..ticks {
            self.game.step(input);
            self.guard_tick();
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
        // The frame's time owed to the game (all of it while its own song
        // keeps time with its clock: `app::TickClock`).
        let keep = self.game.steers_music();
        self.clock.frame(elapsed, keep);

        // The music (started over when it changed), the player's volume.
        let music_gain = self.settings.music_gain();
        if let Some(speaker) = self.speaker.as_mut() {
            // A song is made before the mixer is locked: the audio device
            // waits on the lock. (A level's fade holds no song:
            // `Session::hand_music` says when.)
            self.game.music_ahead(self.clock.behind(0.0));
            let new = self.game.new_music();
            if let Some(mut mixer) = speaker.lock() {
                // A song begun now begins at the clock: nothing owed before it.
                if self.game.hand_music(&mut mixer, new, false) {
                    self.clock.settle();
                }
                mixer.set_music_volume(self.game.cfg.audio.music_volume * music_gain);
                mixer.set_drive(self.game.cfg.audio.drive);
                mixer.set_ceiling(self.game.cfg.audio.ceiling);
            }
        }

        // The pointer where it is now, for the pictures and the ticks.
        self.glide(elapsed);
        self.pointer_shown();
        // The ticks due.
        self.controls();
        for _ in 0..self.clock.due(keep) {
            self.play_tick(input);
        }
        self.draw(device, queue, egui, size)
    }

    /// A tick played as a frame plays it: seat 1's keys `held` with the
    /// mouse's, the text typed and a pad's pointer (`tick_input`), the
    /// hidden-state guard, recorded, its sounds.
    fn play_tick(&mut self, held: &crate::input::Input) {
        self.game.fades = true;
        let input = self.tick_input(held);
        self.game.step(&input);
        self.guard_tick();
        self.runs.step(&self.game);
        if !self.game.game.sim().sounds.is_empty()
            && let Some(mut mixer) = self.speaker.as_mut().and_then(|s| s.lock())
        {
            self.game.play_sounds(&mut mixer, self.settings.sound_gain());
        }
    }

    /// Draws the game as it stands into its texture, `size` pixels big;
    /// its name to egui.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, egui: &mut egui_wgpu::Renderer, size: [u32; 2]) -> egui::TextureId {
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
        let frame = self.game.menu_frame(&Menu::default(), &self.settings, self.clock.fraction(), aspect, Scene::Play);
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
    begin(&mut game, None)
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

/// A fresh run begun from `scenario`: a mistake in the scenario's code
/// stops the game, shown over it until mended (as one in `main.lua`); a
/// scenario that is not there is said at once.
fn begin<G: Game>(game: &mut Session<G>, scenario: Option<&str>) -> Result<()> {
    if let Some(name) = scenario
        && let Err(e) = game.start_scenario(name)
        && game.game.halted().is_none()
    {
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        settings.adopt(&["jump", "fire", "bomb"], &Bindings::new(&[("jump", &["Z"]), ("fire", &["X"])]), &Bindings::new(&[("jump", &["south"])]));
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
        assert_eq!(play.played_with, Device::Gamepad, "leaning, seat 1 plays on its pad");
        let speed = play.game.cfg.ui.height;
        let at = play.seat_place(1).unwrap();
        assert!((at[0] - (w / 2.0 + speed * 0.1)).abs() < 1e-3 && (at[1] - h / 2.0).abs() < 1e-3, "{at:?}");
        play.sticks[0] = Default::default();
        play.sticks[0].at = Some(hud);
        play.pad_button(0, sdl3::gamepad::Button::South);
        play.sticks[0].at = Some([1.0, 1.0]);
        play.play_tick(&crate::input::Input::default());
        play.mouse_at(Some(px));
        play.mouse_at(Some([px[0] + 10.0, px[1]]));
        assert_eq!(play.played_with, Device::Keyboard, "the mouse moved: its pointer again");
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
        play.restart(device, queue).unwrap();
        assert_eq!(play.game.game.sim().tick, 0, "from the start");
        play.step(3, &crate::input::Input::default());
        assert_eq!(play.scenario.as_deref(), Some("late"), "the scenario again");
        let score = play.eval("player.state.score").unwrap();
        assert!(score.contains("1003"), "begun from the scenario's 1000, three ticks on: {score}");
    }
}

//! A game in the IDE's window (`run`): its shell, the editors' windows,
//! the game played, the speakers, the agent's bridge and another
//! session's hands, each frame's phases one after another.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;

use crate::game::Game;
use super::console::{Console, Start};
use super::play::Play;
use super::shell::{Ask, Shell, Stage};
use super::window::{Frame, Picked, Window};
use super::{Next, Route, bridge, choose, memory, no_game, output, prefs, projects, remote, route};

/// An editor's window the IDE opened, and what on.
#[derive(PartialEq)]
enum Editing {
    /// A level (its file) and its blocks.
    Level(String),
    /// A model (its path in the game folder).
    Model(String),
}

/// The game in the IDE's window, between frames.
struct Project<'w, G: Game> {
    window: &'w mut Window,
    dir: PathBuf,
    /// Another session tests the IDE (`--remote`).
    remote: bool,
    shell: Shell,
    /// The bridge for `voxcade mcp` (the agent).
    bridge: Option<bridge::Bridge>,
    /// The editors' windows, on the IDE's GPU, and the models EDIT asked
    /// for from a level, to open in place (the level round them).
    editors: Vec<(Editing, crate::editor::EditorWindow)>,
    in_place: Vec<(String, crate::editor::context::Seed)>,
    /// The game being played in the main area, and whether the window has
    /// the focus (elsewhere the game holds still, unless the IDE answers
    /// another session, `remote`).
    play: Option<Play<G>>,
    focused: bool,
    /// Another session's hands (`--remote`): its pointer, keys and pictures
    /// of the windows.
    hands: remote::Remote,
    /// A song played from its file (PLAY over a .mml), through a speaker of
    /// its own while it plays; a sound played from its text (PLAY SOUND),
    /// its speaker kept open once opened, so the next sound comes at once.
    song: Option<crate::audio::Speaker>,
    blip: Option<crate::audio::Speaker>,
    /// When egui wants to draw again (None: only after an event), and when
    /// the open text is due to be saved or looked at on disk.
    repaint_at: Option<Instant>,
    tick_at: Instant,
    /// While the game would not start: its files watched, so the red line
    /// goes once they are mended.
    start_watch: Option<crate::watch::FolderWatch>,
}

/// The game in `dir` in the IDE's window until it is closed or another is
/// opened (the menu on its name); what the window shows next.
pub(super) fn project<G: Game>(window: &mut Window, dir: &Path, remote: bool) -> Result<Next> {
    let cfg = crate::session::config::<G>(dir)?;
    // Who the game is in the config folder, before its memory, journals
    // and lock are read or made there (`settings::settle`).
    crate::settings::settle(dir);
    // egui afresh: what it keeps of widgets (a text's cursor by its path)
    // belongs to the game before.
    window.fresh(&format!("{} - Voxcade", cfg.title));
    let bridge = bridge::Bridge::open(&bridge::lock_path(dir), dir, window.wake.clone()).map_err(|e| eprintln!("no MCP bridge: {e:#}")).ok();
    let console = Console::new(dir.to_path_buf(), Start::Shell, window.wake.clone());
    let brought = G::sounds(&cfg);
    let mut shell = Shell::new(dir, cfg, memory::Memory::path_for(dir), console).with_journals(memory::Memory::journals_for(dir)).with_sounds(brought).with_prefs(prefs::Prefs::path()).with_output(output::Output::path_for(dir));
    if let Some(said) = window.note.take() {
        shell.note(said);
    }
    let hands = remote::Remote::new(window.sdl.event().context("SDL events")?.event_sender());
    let mut project = Project::<G> {
        window,
        dir: dir.to_path_buf(),
        remote,
        shell,
        bridge,
        editors: Vec::new(),
        in_place: Vec::new(),
        play: None,
        focused: true,
        hands,
        song: None,
        blip: None,
        repaint_at: Some(Instant::now()),
        tick_at: Instant::now(),
        start_watch: None,
    };
    let next = project.run()?;
    project.leave();
    Ok(next)
}

impl<G: Game> Project<'_, G> {
    /// Frame after frame until the window closes or shows something else.
    fn run(&mut self) -> Result<Next> {
        loop {
            let first = self.wait();
            let clipboard = self.window.video.clipboard();
            let (mut happened, next) = self.events(first, &clipboard);
            if let Some(next) = next {
                return Ok(next);
            }
            self.editors_frame(false);
            if let Some(next) = self.picked(&mut happened) {
                return Ok(next);
            }
            let capture = self.calls(&mut happened);
            let changed = self.tick(&mut happened);
            // Nothing new for the IDE's window: no frame.
            if !(happened || changed || self.repaint_at.is_some_and(|r| Instant::now() >= r)) {
                continue;
            }
            if let Some(next) = self.frame(&clipboard, capture)? {
                return Ok(next);
            }
        }
    }

    /// Waits for the first event until something is due: egui's next
    /// frame, the editors' panels, a pad settling, another session's next
    /// step, the open text's save or look.
    fn wait(&mut self) -> Option<Event> {
        self.window.pads.settle();
        let until = self.editors.iter().filter_map(|(_, e)| e.wakes_at()).chain(self.repaint_at).chain(self.window.pads.wakes_at()).chain(self.hands.wakes_at(Instant::now())).fold(self.tick_at, Instant::min);
        self.window.wait(Some(until))
    }

    /// The events waiting, `first` first: to the editors' windows, the
    /// pads, the game's keys, else egui. Whether something happened that
    /// the IDE's window shows, and what it shows next if it is closed.
    fn events(&mut self, first: Option<Event>, clipboard: &sdl3::clipboard::ClipboardUtil) -> (bool, Option<Next>) {
        let mut happened = false;
        for event in first.into_iter().chain(self.window.events.poll_iter().collect::<Vec<_>>()) {
            // A real mouse takes the synthetic pointer's mark away.
            self.hands.saw(&event);
            // The left button let go anywhere: every other editor window's
            // drag still open ends there (a drag let go over the IDE's
            // window or another, whose up event that window got).
            if let Event::MouseButtonUp { mouse_btn: sdl3::mouse::MouseButton::Left, window_id, .. } = event {
                for (_, e) in self.editors.iter_mut().filter(|(_, e)| e.id() != window_id) {
                    e.mouse_up_elsewhere();
                }
            }
            // An editor's window takes its own events; closed, it goes.
            if let Route::Editor(id) = route(&event, self.window.screen.window.id()) {
                let shift = crate::editor::shift_held(&self.window.events) || self.hands.shift();
                if let Some(i) = self.editors.iter().position(|(_, e)| e.id() == id)
                    && !self.editors[i].1.handle(&event, shift)
                {
                    self.editors.remove(i);
                }
                continue;
            }
            if event.is_gamepad() || event.is_joy() {
                self.pad_event(&event);
                continue;
            }
            happened = true;
            match &event {
                // Closing the IDE's window ends the IDE, its editors too
                // (the text being changed saved first, the run being
                // played ended: kept if long enough).
                Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return (happened, Some(Next::Quit)),
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => self.window.screen.resize_surface(*w, *h),
                // Back in the IDE: the open file afresh (an editor may have
                // changed it).
                Event::Window { win_event: WindowEvent::FocusGained, .. } => {
                    self.focused = true;
                    self.shell.refresh();
                }
                Event::Window { win_event: WindowEvent::FocusLost, .. } => self.focused = false,
                // The game's keys (`game_key`); egui does not see them.
                Event::KeyDown { keycode: Some(key), repeat: false, .. } if self.shell.keys_to_game(&self.window.ctx) && self.game_key(*key) => continue,
                _ => {}
            }
            // (No text on the clipboard: Ctrl+V goes on to the console,
            // where Claude Code pastes a picture.)
            let shell = &mut self.shell;
            self.window.input.handle(&event, || match clipboard.clipboard_text() {
                Ok(text) => Some(text).filter(|t| !t.is_empty()),
                Err(e) => {
                    shell.note(format!("PASTING FAILED: {e}"));
                    None
                }
            });
        }
        (happened, None)
    }

    /// Pads plugged in and out; their buttons and sticks are read while
    /// the game runs (drawn every frame then), so they draw nothing
    /// themselves (a drifting stick would redraw the IDE all the time).
    /// The run over (PRESS START): from the start, as F5; else a pad's
    /// START takes a seat (a game of several).
    fn pad_event(&mut self, event: &Event) {
        use sdl3::gamepad::Button;
        match event {
            Event::GamepadAdded { which, .. } => self.window.pads.added(*which),
            Event::GamepadRemoved { which, .. } => self.window.pads.removed(*which),
            Event::GamepadButtonDown { button: button @ (Button::Start | Button::South), which, .. } if self.shell.stage.as_ref().is_some_and(|s| s.shown) && self.focused => match self.play.as_mut() {
                Some(p) if p.over() => self.restart(),
                Some(p) if *button == Button::Start => p.pad_start(which.raw(), &self.window.pads.ids()),
                _ => {}
            },
            _ => {}
        }
    }

    /// The game's keys while it has them: Escape back to the file, Return
    /// a seat for the keyboard's right half (a game of several), or the
    /// run over, Enter starting it over as F5 does; F9 keeps the run in
    /// the game's replays. Whether the key was the game's alone (egui
    /// does not see it).
    fn game_key(&mut self, key: Keycode) -> bool {
        let over = self.play.as_ref().map(Play::over);
        match key {
            Keycode::Escape => self.shell.back_to_file(),
            Keycode::Return if over == Some(false) => {
                if let Some(p) = &mut self.play {
                    p.keyboard_start();
                }
                return false;
            }
            Keycode::Return | Keycode::KpEnter if over == Some(true) => self.restart(),
            Keycode::F5 => self.restart(),
            Keycode::F9 => {
                if let Some(p) = &self.play {
                    self.shell.note(match p.keep() {
                        Ok(path) => format!("KEPT THE RUN AS {}", path.strip_prefix(&self.dir).unwrap_or(&path).display()),
                        Err(e) => format!("NO RUN KEPT: {e:#}"),
                    });
                }
            }
            _ => return false,
        }
        true
    }

    /// The game begun anew from its files (F5, AGAIN, the run over).
    fn restart(&mut self) {
        let screen = &self.window.screen;
        if let Some(p) = &mut self.play
            && let Err(e) = p.restart(&screen.device, &screen.queue)
        {
            self.shell.tell(format!("{e:#}"));
        }
    }

    /// The editors' windows drawn; EDIT MODEL in a level's editor opens the
    /// model in a window of its own, as one opened from the list (a
    /// piece's: shaped in place, the level round it); with `pictures`
    /// the pictures another session asked of them taken; an editor that
    /// answered DON'T OPEN goes.
    fn editors_frame(&mut self, pictures: bool) {
        for (_, editor) in &mut self.editors {
            if let Err(e) = editor.draw() {
                self.shell.tell(format!("{e:#}"));
            }
            if let Some(model) = editor.take_model_edit() {
                if let Some(seed) = editor.take_model_context() {
                    self.in_place.push((model.clone(), seed));
                }
                self.shell.ask_edit(model);
                self.repaint_at = Some(Instant::now());
            }
            if pictures && let Some(picture) = editor.take_picture() {
                self.hands.captured(editor.id(), picture);
            }
        }
        self.editors.retain(|(_, e)| !e.closing());
    }

    /// A folder the system's dialog picked: a game to open (what the
    /// window shows next), or where NEW puts games.
    fn picked(&mut self, happened: &mut bool) -> Option<Next> {
        while let Ok(folder) = self.window.picked.1.try_recv() {
            *happened = true;
            match folder {
                Picked::Game(to) => match choose(&mut self.window.note, &to) {
                    Next::Game(game) => self.shell.leave_for(game),
                    Next::Found(folder, games) => {
                        self.shell.flush();
                        return Some(Next::Found(folder, games));
                    }
                    _ => self.shell.tell(no_game(&to)),
                },
                Picked::Projects(to) => self.shell.set_projects(to),
            }
        }
        None
    }

    /// What the agent asks through the bridge, answered; the pointer, keys
    /// and pictures another session asked for: the next step of its call,
    /// and the marks and pictures asked of the windows. Which window's
    /// picture is to be taken, and whether late.
    fn calls(&mut self, happened: &mut bool) -> Option<(u32, bool)> {
        let calls = self.bridge.as_ref().map(bridge::Bridge::calls).unwrap_or_default();
        let windows = if self.remote && (!calls.is_empty() || self.hands.busy()) { window_infos(&self.window.screen, &self.editors) } else { Vec::new() };
        for call in calls {
            *happened = true;
            answer(call, &mut self.shell, &self.window.ctx, self.play.as_mut(), &self.window.screen, self.remote, (&mut self.hands, &windows));
        }
        let capture = if self.hands.busy() {
            let now = Instant::now();
            self.hands.run(now, &windows);
            *happened = true;
            self.hands.capturing(now)
        } else {
            None
        };
        for (_, editor) in &mut self.editors {
            editor.set_marker(self.hands.marker(editor.id()));
            if let Some((_, late)) = capture.filter(|c| c.0 == editor.id()) {
                editor.want_picture(late);
            }
        }
        capture
    }

    /// What is due between frames: the text being changed saved, the
    /// disk looked at; a song that ended let go of; a game stopped by an
    /// error and held takes in what is saved meanwhile; a game that would
    /// not start begun in the background once its files change. Whether
    /// the shell has something new to show.
    fn tick(&mut self, happened: &mut bool) -> bool {
        let (changed, next) = self.shell.tick(Instant::now());
        self.tick_at = Instant::now() + next;
        // A song that plays once ends by itself: looked for twice a second.
        if self.song.is_some() {
            if self.song.as_mut().and_then(|s| s.lock()).is_none_or(|m| m.music_over()) {
                drop(self.song.take());
                self.shell.sound.listening = None;
                *happened = true;
            } else {
                self.tick_at = self.tick_at.min(Instant::now() + Duration::from_millis(500));
            }
        }
        // (A few times a second: the line marked goes once the file is
        // mended.)
        let screen = &self.window.screen;
        if let (Some(p), Some(stage)) = (&mut self.play, &mut self.shell.stage)
            && (stage.trouble.is_some() || stage.unloaded.is_some())
            && !(stage.shown && (self.focused || self.remote))
        {
            if p.reload(&screen.device, &screen.queue) {
                stage.trouble = p.trouble();
                stage.unloaded = p.unloaded();
                *happened = true;
            }
            self.tick_at = self.tick_at.min(Instant::now() + Duration::from_millis(250));
        }
        // (The red line goes if the game would start now; no window.)
        if !self.shell.start_failed() {
            self.start_watch = None;
        }
        if let Some(watch) = &mut self.start_watch {
            if !watch.poll().is_empty() && super::play::begins::<G>(&self.dir).is_ok() {
                self.shell.start_mended();
                self.start_watch = None;
                *happened = true;
            }
            self.tick_at = self.tick_at.min(Instant::now() + Duration::from_millis(250));
        }
        changed
    }

    /// A frame of the IDE's window: egui's, the open file's picture, what
    /// the window asked of the game and the speakers, the game's own
    /// frame, the system's part, the editors asked for, and the frame
    /// shown (its picture taken if `capture` names this window). What the
    /// window shows next, if it asked for another game.
    fn frame(&mut self, clipboard: &sdl3::clipboard::ClipboardUtil, capture: Option<(u32, bool)>) -> Result<Option<Next>> {
        let marker = self.hands.marker(self.window.screen.window.id()).map(|(x, y)| self.window.input.point(x, y));
        let shell = &mut self.shell;
        let frame = self.window.run(|ui| {
            shell.ui(ui);
            if let Some(tip) = marker {
                crate::gui::marker::paint(ui.ctx(), tip);
            }
        });
        // The open file's picture, drawn anew if it changed (shown from the
        // next frame on when it is new).
        let pictured = self.shell.render(&self.window.screen.device, &self.window.screen.queue, &mut self.window.painter.renderer);
        if let Some(next) = self.asks() {
            return Ok(Some(next));
        }
        let running = self.game_frame();
        // What the game said: into OUTPUT.
        if let Some(p) = &mut self.play {
            let said = p.said();
            self.shell.game_said(p.trouble(), said);
        }
        if let Some(trouble) = self.window.platform(&frame) {
            self.shell.note(trouble);
        }
        self.selection(clipboard);
        let delay = Window::delay(&frame);
        self.repaint_at = if pictured || running { Some(Instant::now()) } else { Instant::now().checked_add(delay) };
        self.edits();
        self.editors_frame(true);
        // A picture of this window asked for: taken once the frame is
        // settled (egui wants no frame at once), or as it is when it has
        // waited long.
        let take = capture.is_some_and(|(id, late)| id == self.window.screen.window.id() && (late || (!delay.is_zero() && !pictured)));
        self.present(frame, take)?;
        Ok(None)
    }

    /// What the window asked of the game and the speakers: the game
    /// started (saved first, by the shell) or ended, begun anew, a line
    /// run in it; a song, a sound; another game (what it shows next).
    fn asks(&mut self) -> Option<Next> {
        for ask in self.shell.take_asks() {
            match ask {
                Ask::Play(scenario) => match Play::<G>::start(&self.dir, &self.window.screen.device, &self.window.screen.queue, crate::audio::speaker(&self.window.sdl), scenario.as_deref()) {
                    Ok(p) => {
                        self.shell.stage = Some(Stage { shown: true, keys: p.keys().to_vec(), ..Stage::default() });
                        self.play = Some(p);
                        self.shell.start_mended();
                        self.start_watch = None;
                    }
                    Err(e) => {
                        self.shell.wont_start(format!("{e:#}"));
                        self.start_watch = Some(crate::watch::FolderWatch::new(&self.dir));
                    }
                },
                Ask::Stop => {
                    if let Some(mut p) = self.play.take() {
                        let said = p.said();
                        self.shell.game_said(None, said);
                        p.end(&mut self.window.painter.renderer);
                    }
                    self.shell.stage = None;
                }
                Ask::Again => self.restart(),
                Ask::Eval(line) => match self.play.as_mut() {
                    Some(p) => {
                        let said = p.eval(&line);
                        self.shell.evaluated(&line, &said);
                    }
                    None => self.shell.tell("No game plays: PLAY first, then its lines run in it.".into()),
                },
                Ask::Listen(path) => self.listen(&path),
                // The speaker closed with it.
                Ask::Hush => drop(self.song.take()),
                Ask::Sound => self.sound(),
                Ask::Project(projects::Wish::Open(to)) => return Some(Next::Game(to)),
                Ask::Project(projects::Wish::Upgrade(to, play)) => return Some(Next::Upgrade(to, play)),
                Ask::Project(projects::Wish::Browse) => self.window.browse(self.shell.projects_dir(), Picked::Game),
                Ask::Project(projects::Wish::BrowseProjects) => self.window.browse(self.shell.projects_dir(), Picked::Projects),
            }
        }
        None
    }

    /// PLAY SONG: the song in `path` through the song's speaker (opened
    /// for it).
    fn listen(&mut self, path: &str) {
        let player = match self.shell.song(path) {
            Ok(player) => player,
            Err(e) => {
                self.shell.sound.listening = None;
                return self.shell.tell(format!("{e:#}"));
            }
        };
        let drive = self.shell.drive();
        match opened(&mut self.song, &self.window.sdl) {
            Some(mut mixer) => {
                mixer.set_drive(drive);
                mixer.set_music(Some(player));
            }
            None => {
                self.shell.sound.listening = None;
                self.shell.tell(NO_SPEAKER.into());
            }
        }
    }

    /// PLAY SOUND: the sound made ready through the sounds' speaker.
    fn sound(&mut self) {
        let Some((samples, volume)) = self.shell.heard() else { return };
        let drive = self.shell.drive();
        match opened(&mut self.blip, &self.window.sdl) {
            Some(mut mixer) => {
                mixer.set_drive(drive);
                mixer.play("heard", &samples, volume, 0.0);
            }
            None => self.shell.tell(NO_SPEAKER.into()),
        }
    }

    /// While the game shows and the window has the focus it runs, drawn
    /// every frame, its keys read as the player bound them; else it holds
    /// still. Whether it ran.
    fn game_frame(&mut self) -> bool {
        let keys = self.shell.keys_to_game(&self.window.ctx);
        let (Some(p), Some(stage)) = (&mut self.play, &mut self.shell.stage) else { return false };
        // (For another session testing it, `--remote`, it runs on with the
        // window elsewhere: what it fetches is the game as it goes.)
        let Some(size) = stage.wanted.filter(|_| stage.shown && (self.focused || self.remote)) else {
            p.hold();
            return false;
        };
        let window = &mut *self.window;
        // The pads always, the keyboard while the game has it (and the
        // keys another session holds down for it).
        let state = window.events.keyboard_state();
        let extra = self.hands.held(Instant::now());
        let with = crate::controls::WithHeld(&state, &extra);
        let held = p.read(keys.then_some(&with), &window.pads.open);
        stage.picture = Some(p.frame(&window.screen.device, &window.screen.queue, &mut window.painter.renderer, size, &held));
        stage.trouble = p.trouble();
        stage.unloaded = p.unloaded();
        if stage.keys != p.keys() {
            stage.keys = p.keys().to_vec();
        }
        true
    }

    /// The console and the system's selection (Linux's primary): what the
    /// mouse selects goes there, a middle click pastes it.
    fn selection(&mut self, clipboard: &sdl3::clipboard::ClipboardUtil) {
        if let Some(text) = self.shell.console_mut().take_selected()
            && let Err(e) = clipboard.set_primary_selection_text(&text)
        {
            self.shell.note(format!("SELECTING FAILED: {e}"));
        }
        if self.shell.console_mut().wants_selection()
            && let Ok(text) = clipboard.primary_selection_text()
        {
            self.shell.console_mut().paste(&text);
        }
    }

    /// Files asked to be edited: into the window that has them (brought to
    /// the front), else a new one; a block in a level's editor, at the
    /// block (one open already, else the level the config names).
    fn edits(&mut self) {
        for path in self.shell.take_edits() {
            let block = path.strip_prefix("blocks/").and_then(|p| p.strip_suffix(".vvx").or_else(|| p.strip_suffix(".vox")));
            let open_level = self.editors.iter().find_map(|(w, _)| if let Editing::Level(file) = w { Some(file.clone()) } else { None });
            let what = match () {
                _ if block.is_some() => match open_level.or_else(|| self.shell.first_level().map(str::to_string)) {
                    Some(file) => Editing::Level(file),
                    None => continue,
                },
                _ if !super::shell::is_model_file(&path) => Editing::Level(path.clone()),
                _ => Editing::Model(path.clone()),
            };
            let Some(i) = self.editor_for(what) else { continue };
            let editor = &mut self.editors[i].1;
            if let Some(name) = block {
                editor.open_block(name);
            }
            // EDIT on a piece: the level round the model.
            if let Some(k) = self.in_place.iter().position(|(m, _)| *m == path) {
                editor.set_context(self.in_place.remove(k).1);
            }
            editor.raise();
            self.repaint_at = Some(Instant::now());
        }
    }

    /// The editor's window on `what` (its place in the list): the one
    /// open, else a new one (what went wrong opening it said).
    fn editor_for(&mut self, what: Editing) -> Option<usize> {
        if let Some(i) = self.editors.iter().position(|(w, _)| *w == what) {
            return Some(i);
        }
        let (video, screen) = (&self.window.video, &self.window.screen);
        let opened = match &what {
            Editing::Level(file) => crate::editor::EditorWindow::open::<G>(video, &self.dir, Some(file), None, false, Some(screen.gpu())),
            Editing::Model(file) => crate::editor::EditorWindow::open_model::<G>(video, &self.dir, file, Some(screen.gpu())),
        };
        match opened {
            Ok(e) => {
                self.editors.push((what, e));
                Some(self.editors.len() - 1)
            }
            Err(e) => {
                self.shell.tell(format!("{e:#}"));
                None
            }
        }
    }

    /// The frame shown, its picture taken if asked (another session's).
    fn present(&mut self, frame: Frame, take: bool) -> Result<()> {
        let (shown, picture) = self.window.present(frame, take)?;
        if !shown {
            self.repaint_at = Some(Instant::now());
        }
        if let Some(picture) = picture {
            self.hands.captured(self.window.screen.window.id(), picture);
        }
        Ok(())
    }

    /// Leaving the game: the text being changed saved, the run being
    /// played ended (kept if long enough), the pictures freed, what the
    /// IDE remembers written down (failing, it is said, but the window
    /// still closes or goes on); the editors' windows, the console and the
    /// bridge go with the project.
    fn leave(mut self) {
        self.shell.flush();
        if let Some(p) = self.play.take() {
            p.end(&mut self.window.painter.renderer);
        }
        self.shell.release(&mut self.window.painter.renderer);
        if let Err(e) = self.shell.save() {
            eprintln!("remembering what was open: {e:#}");
        }
    }
}

/// What is said when a speaker would not open.
const NO_SPEAKER: &str = "No sound: the speaker would not open.";

/// The speaker in `slot`, opened if it is not yet, its mixer locked.
fn opened<'a>(slot: &'a mut Option<crate::audio::Speaker>, sdl: &sdl3::Sdl) -> Option<std::sync::MutexGuard<'a, crate::audio::Mixer>> {
    if slot.is_none() {
        *slot = crate::audio::speaker(sdl);
    }
    slot.as_mut().and_then(|s| s.lock())
}

/// The IDE's windows as the remote tools list them: the main one first,
/// then the editors'.
fn window_infos(screen: &crate::window::Screen, editors: &[(Editing, crate::editor::EditorWindow)]) -> Vec<remote::WindowInfo> {
    let main = remote::WindowInfo {
        id: screen.window.id(),
        title: screen.window.title().to_string(),
        width: screen.config.width,
        height: screen.config.height,
        density: screen.window.pixel_density(),
        keyboard: screen.window.has_input_focus(),
        main: true,
    };
    let others = editors.iter().map(|(_, e)| {
        let (title, width, height, density, keyboard) = e.info();
        remote::WindowInfo { id: e.id(), title, width, height, density, keyboard, main: false }
    });
    std::iter::once(main).chain(others).collect()
}

/// Why a call may not be answered, if it may not: typing into the console
/// is running commands in the user's shell, past the agent's permission
/// prompts, and the pointer and keys are a person's hands, so these tools
/// (the console's, and `remote::TOOLS`) are only for an IDE started with
/// `--remote` (another session's, testing), never the game's own agent.
pub(super) fn refused(tool: &str, remote: bool) -> Option<String> {
    ((matches!(tool, "console_send" | "console_read") || remote::TOOLS.contains(&tool)) && !remote).then(|| format!("{tool} is not allowed: the IDE was not started with --remote (voxcade ide <game> --remote)"))
}

/// The most lines `console_read` gives, and what it gives without being
/// asked.
const READ_MOST: usize = 2000;
const READ_LINES: usize = 200;

/// Answers a call of the agent's (`voxcade mcp`, through the bridge): what
/// the user sees, a file shown, the game played, stopped or looked at; with
/// `remote` the console too.
fn answer<G: Game>(call: bridge::Call, shell: &mut Shell, ctx: &egui::Context, play: Option<&mut Play<G>>, screen: &crate::window::Screen, remote: bool, hands: (&mut remote::Remote, &[remote::WindowInfo])) {
    use serde_json::json;
    if let Some(why) = refused(&call.tool, remote) {
        return call.fail(&why);
    }
    // The windows' tools: the pointer, the keys, pictures of the windows.
    if remote::TOOLS.contains(&call.tool.as_str()) {
        return hands.0.submit(call, hands.1);
    }
    let text = |t: String| json!({ "type": "text", "text": t });
    let args = call.args.clone();
    match call.tool.as_str() {
        "view_state" => call.answer(vec![text(serde_json::to_string_pretty(&shell.state(ctx)).unwrap_or_default())]),
        "errors" => call.answer(vec![text(shell.errors())]),
        "open" => match args["path"].as_str() {
            Some(path) if shell.has(path) => {
                let line = args["line"].as_u64().map(|l| l as usize);
                shell.go_to(path, line);
                call.answer(vec![text(format!("{path} shown{}", line.map_or(String::new(), |l| format!(" at line {l}"))))]);
            }
            Some(path) => call.fail(&format!("the game folder has no {path} (logic, models, levels, pictures, music)")),
            None => call.fail("open needs path: the file, its path in the game folder"),
        },
        "play" => {
            shell.play();
            call.answer(vec![text("The game plays in the IDE's main area (the text being changed saved first).".into())]);
        }
        "stop" => {
            shell.stop();
            call.answer(vec![text("The game is ended.".into())]);
        }
        "eval" => match (play, args["line"].as_str()) {
            (Some(p), Some(line)) => {
                let said = p.eval(line);
                shell.evaluated(line, &said);
                match said {
                    Ok(said) => call.answer(vec![text(if said.is_empty() { "done".into() } else { said })]),
                    Err(e) => call.fail(&format!("{e:#}")),
                }
            }
            (None, _) => call.fail("No game plays in the IDE: play starts one (inspect and shot take --do without the IDE)."),
            (_, None) => call.fail("eval needs line: the Lua to run in the game"),
        },
        "console_send" => match args["text"].as_str() {
            Some(line) => match shell.console_mut().send(line, args["enter"].as_bool().unwrap_or(true)) {
                Ok(said) => call.answer(vec![text(said)]),
                Err(why) => call.fail(&why),
            },
            None => call.fail("console_send needs text: what to type into the console"),
        },
        "console_read" => {
            let lines = args["lines"].as_u64().map_or(READ_LINES, |n| (n as usize).clamp(1, READ_MOST));
            match shell.console().read(lines) {
                Ok(said) => call.answer(vec![text(if said.is_empty() { "(the console shows nothing)".into() } else { said })]),
                Err(why) => call.fail(&why),
            }
        }
        "game_frame" => match play.map(|p| p.picture(&screen.device, &screen.queue)) {
            Some(Ok(png)) => call.answer(vec![json!({ "type": "image", "data": crate::mcp::base64(&png), "mimeType": "image/png" })]),
            Some(Err(e)) => call.fail(&format!("{e:#}")),
            None => call.fail("No game plays in the IDE: play starts one (shot plays without the IDE)."),
        },
        other => {
            let why = format!("the IDE has no tool {other}");
            call.fail(&why);
        }
    }
}

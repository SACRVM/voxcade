//! The editor in a window of its own (`EditorWindow`): `voxcade edit`'s
//! loop, the window's events by their kind, a frame drawn.

use super::*;

/// The buttons pressed in the view and not yet let go (`View::held`).
pub const HELD_LEFT: u8 = 1;
pub const HELD_RIGHT: u8 = 2;
pub const HELD_MIDDLE: u8 = 4;

/// How long the left button is found up, a drag still open, before the
/// drag ends as let go (its up event lost).
const LOST_RELEASE: Duration = Duration::from_millis(200);

/// Whether the left mouse button is down, as the system has it (not as
/// the events said: one may be lost).
fn left_button_down() -> bool {
    // SAFETY: SDL is running (a window is open); null positions are allowed.
    let buttons = unsafe { sdl3::sys::mouse::SDL_GetGlobalMouseState(std::ptr::null_mut(), std::ptr::null_mut()) };
    buttons.0 & sdl3::sys::mouse::SDL_BUTTON_LMASK.0 != 0
}

/// Opens the editor in a window on the game in `dir` (its `level`, else
/// the one its config names; at `block` if given, or among the level's
/// `things`; or on the `model` file) until it is closed.
pub fn run<G: Game>(dir: &Path, level: Option<&str>, block: Option<&str>, things: bool, model: Option<&str>) -> Result<()> {
    let sdl = sdl3::init().context("initializing SDL")?;
    let video = sdl.video().context("initializing SDL video")?;
    let first = match model {
        Some(file) => EditorWindow::open_model::<G>(&video, dir, file, None)?,
        None => EditorWindow::open::<G>(&video, dir, level, block, things, None)?,
    };
    // The level's window, and a model's for each EDIT MODEL (on its GPU).
    let mut windows = vec![first];
    let mut events = sdl.event_pump().context("SDL events")?;
    loop {
        for w in &mut windows {
            w.draw()?;
        }
        let asked: Vec<(String, Option<context::Seed>)> = windows.iter_mut().filter_map(|w| w.take_model_edit().map(|f| (f, w.take_model_context()))).collect();
        for (file, seed) in asked {
            if let Some(w) = windows.iter_mut().find(|w| w.model_file() == Some(file.as_str())) {
                if let Some(seed) = seed {
                    w.set_context(seed);
                }
                w.raise();
                continue;
            }
            match EditorWindow::open_model::<G>(&video, dir, &file, Some(windows[0].gpu())) {
                Ok(mut w) => {
                    if let Some(seed) = seed {
                        w.set_context(seed);
                    }
                    windows.push(w)
                }
                Err(e) => windows[0].say(format!("{e:#}")),
            }
        }
        // DON'T OPEN: nothing was touched.
        windows.retain(|w| !w.closing());
        if windows.is_empty() {
            return Ok(());
        }
        // Drawn only after something happened (or when its panels ask):
        // the editor sits still otherwise.
        let at = windows.iter().map(EditorWindow::wakes_at).min().unwrap_or_else(Instant::now);
        let first = events.wait_event_timeout(at.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)));
        for event in first.into_iter().chain(events.poll_iter().collect::<Vec<_>>()) {
            let shift = shift_held(&events);
            // The left button let go: every other window's drag still open
            // ends too (it may have been begun there and let go here).
            if let sdl3::event::Event::MouseButtonUp { mouse_btn: sdl3::mouse::MouseButton::Left, window_id, .. } = event {
                for w in windows.iter_mut().filter(|w| w.id() != window_id) {
                    w.mouse_up_elsewhere();
                }
            }
            match event.get_window_id() {
                // Each window its own events; one closed goes.
                Some(id) => {
                    if let Some(k) = windows.iter().position(|w| w.id() == id)
                        && !windows[k].handle(&event, shift)
                    {
                        windows.remove(k);
                    }
                }
                None if matches!(event, sdl3::event::Event::Quit { .. }) => return Ok(()),
                None => {}
            }
        }
        if windows.is_empty() {
            return Ok(());
        }
    }
}

/// A key among the pieces (not typed into a field): Escape lets go of what
/// hangs on the mouse, closes ADD MODEL's list or a name being typed, else
/// chooses nothing; the arrows nudge the chosen one across and in depth,
/// as the view looks (Shift: by 10), Page Up and Page Down (and + and -)
/// raise and lower it; T turns it (Shift: the other way), Delete and
/// Backspace remove it, F2 renames it. Whether the key was one of these.
fn pieces_key(ed: &mut Editor, view: &mut View, key: sdl3::keyboard::Keycode, keymod: sdl3::keyboard::Mod) -> bool {
    use sdl3::keyboard::{Keycode, Mod};
    if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD | Mod::LALTMOD | Mod::RALTMOD) {
        return false;
    }
    let by = if keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) { 10 } else { 1 };
    // Away from the eye and to the right, as the view is turned.
    let yaw = view.level.yaw.to_radians();
    let ahead = if yaw.sin().abs() > yaw.cos().abs() { glam::IVec3::new(yaw.sin().signum() as i32, 0, 0) } else { glam::IVec3::new(0, yaw.cos().signum() as i32, 0) };
    let right = glam::IVec3::new(ahead.y, -ahead.x, 0);
    match key {
        Keycode::Escape => {
            // A menu, a drag (all back as it was), the name typed, what
            // hangs on the mouse, ADD MODEL's pictures, the choice: the
            // nearest goes first.
            if view.menu.is_some() {
                view.menu = None;
            } else if ed.cancel_drag() {
            } else if view.renaming.is_some() {
                view.renaming = None;
            } else if !ed.drop_hanging() {
                if view.picker {
                    view.picker = false;
                } else {
                    ed.choose_item(None);
                }
            }
        }
        Keycode::Up => ed.nudge(ahead * by),
        Keycode::Down => ed.nudge(-ahead * by),
        Keycode::Right => ed.nudge(right * by),
        Keycode::Left => ed.nudge(-right * by),
        Keycode::PageUp | Keycode::Plus | Keycode::Equals | Keycode::KpPlus => ed.nudge(glam::IVec3::Z * by),
        Keycode::PageDown | Keycode::Minus | Keycode::KpMinus => ed.nudge(-glam::IVec3::Z * by),
        Keycode::T => ed.turn_item(if by > 1 { -1 } else { 1 }),
        Keycode::Delete | Keycode::Backspace if ed.pieces.hanging.is_none() => ed.remove_item(),
        Keycode::F2 => view.renaming = ed.pieces.chosen,
        _ => return false,
    }
    true
}

/// Whether a shift key is down.
pub fn shift_held(events: &sdl3::EventPump) -> bool {
    use sdl3::keyboard::Scancode;
    let keys = events.keyboard_state();
    keys.is_scancode_pressed(Scancode::LShift) || keys.is_scancode_pressed(Scancode::RShift)
}

/// How often an open editor looks whether another program changed its
/// files.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// The editor in a window of its own: `voxcade edit`'s, or one of those the
/// IDE opens (several, on the IDE's GPU). It takes the events of its
/// window and draws when something happened.
pub struct EditorWindow {
    /// Its window, its 3D view's renderer and the panels' painter (none:
    /// a window without a window, for tests that feed its events).
    win: Option<Win>,
    /// The size a window without a window has (its points and pixels).
    pub(super) headless: [u32; 2],
    pub(super) ed: Editor,
    pub(super) view: View,
    pub(super) shown: Shown,
    pub(super) figures: Figures,
    pub(super) cfg: crate::config::GameConfig,
    /// The panels (egui, as the IDE): their input, painter, pictures, the
    /// keys they named and where they lay, last frame.
    pub(super) gui: egui::Context,
    pub(super) input: crate::gui::input::Input,
    pub(super) thumbs: panels::Thumbs,
    pub(super) keys: Vec<(char, view::Action)>,
    pub(super) rects: Vec<egui::Rect>,
    /// Where the parts' rows lay, last frame (for tests that click them).
    #[cfg(test)]
    pub(super) rows: Vec<(String, egui::Rect)>,
    /// What a window without a window would have drawn: how often the
    /// world shown was built anew whole (every chunk meshed again), the
    /// last frame and whether it built the world anew (for tests that
    /// count and time what a window does).
    #[cfg(test)]
    pub(super) rebuilds: usize,
    #[cfg(test)]
    pub(super) last: Option<(crate::render::FrameData, bool)>,
    /// Steps there were when the last left click came: a double click
    /// takes back what its first click did.
    pub(super) steps_at_click: usize,
    /// Something happened since the last frame, and when the panels want
    /// to be drawn again (None: only after an event).
    pub(super) dirty: bool,
    pub(super) repaint_at: Option<Instant>,
    /// Another session driving the IDE (`ide::remote`): where its pointer
    /// is (the window's points; an arrow is drawn there), a picture of the
    /// window asked for (`Some(late)`: also if the panels are not settled)
    /// and the one drawn.
    pub(super) marker: Option<(f32, f32)>,
    pub(super) capture: Option<bool>,
    pub(super) picture: Option<Result<Vec<u8>, String>>,
    /// DON'T OPEN was answered: the window closes at its next event.
    pub(super) closing: bool,
    /// When it last looked whether another program changed its files
    /// (`Editor::look_at_disk`: once a second, as the IDE's list).
    pub(super) looked: Instant,
}

/// An editor window's own: the window, the 3D view's renderer, the
/// panels' painter.
struct Win {
    screen: crate::window::Screen,
    renderer: crate::render::Renderer,
    painter: crate::gui::Painter,
}

impl EditorWindow {
    /// A window on the level of the game in `dir` (at `block` if given, or
    /// among its `things`), drawn with `gpu` if given.
    pub fn open<G: Game>(video: &sdl3::VideoSubsystem, dir: &Path, level: Option<&str>, block: Option<&str>, things: bool, gpu: Option<&crate::window::Gpu>) -> Result<Self> {
        let (ed, view, cfg) = open::<G>(dir, level, block, things)?;
        // A game of several levels says which.
        let title = match level.and_then(|l| Path::new(l).file_stem()) {
            Some(name) => format!("Level {} - {}", name.to_string_lossy(), cfg.title),
            None => format!("Level - {}", cfg.title),
        };
        Self::with(video, &title, ed, view, cfg, dir, gpu)
    }

    /// A window on the model `file` of the game in `dir`.
    pub fn open_model<G: Game>(video: &sdl3::VideoSubsystem, dir: &Path, file: &str, gpu: Option<&crate::window::Gpu>) -> Result<Self> {
        let (ed, view, cfg) = open_model::<G>(dir, file)?;
        let title = format!("{file} - {}", cfg.title);
        Self::with(video, &title, ed, view, cfg, dir, gpu)
    }

    fn with(video: &sdl3::VideoSubsystem, title: &str, ed: Editor, mut view: View, cfg: crate::config::GameConfig, dir: &Path, gpu: Option<&crate::window::Gpu>) -> Result<Self> {
        // The person's BACKGROUND for each editor (not the game's).
        view.backgrounds = background::Backgrounds::load(&background::Backgrounds::path()).colours();
        let screen = crate::window::Screen::open_on(video, title, 1600, 900, false, gpu)?;
        // SDL sends typed text only to a window that asked for it: without
        // this no field of the editor (a name, a number) takes a key from a
        // real keyboard (the owner: "alle textfelder erlauben keinen input").
        video.text_input().start(&screen.window);
        let mut renderer = crate::render::Renderer::new(&screen.device, screen.config.format, screen.config.width, screen.config.height);
        let mut shown = Shown::new(&ed)?;
        renderer.rebuild_all(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES);
        let figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
        let gui = egui::Context::default();
        crate::gui::style(&gui);
        let painter = crate::gui::Painter::new(&screen.device, screen.config.format, true);
        Ok(Self::of(Some(Win { screen, renderer, painter }), ed, view, cfg, shown, figures, gui))
    }

    /// An editor window without a window (`win` none): its events fed and
    /// its frames run as a window's, nothing drawn; for tests.
    #[cfg(test)]
    pub fn headless(ed: Editor, view: View, cfg: crate::config::GameConfig, dir: &Path) -> Result<Self> {
        let shown = Shown::new(&ed)?;
        let figures = Figures::load(dir, &cfg).with_pieces(dir, &ed.level);
        let gui = egui::Context::default();
        crate::gui::style(&gui);
        Ok(Self::of(None, ed, view, cfg, shown, figures, gui))
    }

    #[allow(clippy::too_many_arguments)]
    fn of(win: Option<Win>, ed: Editor, view: View, cfg: crate::config::GameConfig, shown: Shown, figures: Figures, gui: egui::Context) -> Self {
        Self {
            win,
            headless: [1600, 900],
            ed,
            view,
            shown,
            figures,
            cfg,
            gui,
            input: crate::gui::input::Input::default(),
            thumbs: panels::Thumbs::default(),
            keys: Vec::new(),
            rects: Vec::new(),
            #[cfg(test)]
            rows: Vec::new(),
            #[cfg(test)]
            rebuilds: 0,
            #[cfg(test)]
            last: None,
            steps_at_click: 0,
            dirty: true,
            repaint_at: None,
            marker: None,
            capture: None,
            picture: None,
            closing: false,
            looked: Instant::now(),
        }
    }

    /// Its size in points (as the mouse gives them) and in pixels.
    fn sizes(&self) -> ((f32, f32), [u32; 2]) {
        match &self.win {
            Some(w) => {
                let (pw, ph) = w.screen.window.size();
                ((pw as f32, ph as f32), [w.screen.config.width, w.screen.config.height])
            }
            None => ((self.headless[0] as f32, self.headless[1] as f32), self.headless),
        }
    }

    /// Whether it is to close (DON'T OPEN was answered).
    pub fn closing(&self) -> bool {
        self.closing
    }

    /// The left button let go where this window did not hear it (outside
    /// it, over another window): a drag still open ends there as let go,
    /// one step.
    pub fn mouse_up_elsewhere(&mut self) {
        self.view.up_since = None;
        self.view.press_at = None;
        self.view.held = 0;
        self.view.marquee = None;
        // The panels let go too (egui would hold the button down for good).
        self.input.release_buttons();
        if self.ed.drag_open() {
            self.ed.end_drag(false);
            self.ed.end_whole_stroke();
            self.view.follow_origin(&self.ed);
            self.dirty = true;
        }
    }

    /// When it wants to draw next: at once when dirty, else when its
    /// panels ask or the disk is to be looked at again.
    pub fn wakes_at(&self) -> Instant {
        // A drag open with the button found up: soon, to end it.
        if let Some(since) = self.view.up_since {
            return since + LOST_RELEASE;
        }
        let look = self.looked + LOOK_EVERY;
        if self.dirty { Instant::now() } else { self.repaint_at.map_or(look, |r| r.min(look)) }
    }

    /// Its window's SDL id: the events that are its.
    pub fn id(&self) -> u32 {
        self.win.as_ref().map_or(0, |w| w.screen.window.id())
    }

    /// Its title, size in pixels, pixels to one of SDL's points (the
    /// mouse's) and whether it has the keyboard.
    pub fn info(&self) -> (String, u32, u32, f32, bool) {
        match &self.win {
            Some(w) => (w.screen.window.title().to_string(), w.screen.config.width, w.screen.config.height, w.screen.window.pixel_density(), w.screen.window.has_input_focus()),
            None => (String::new(), self.headless[0], self.headless[1], 1.0, true),
        }
    }

    /// Where a synthetic pointer is (SDL's points), if it is.
    pub fn set_marker(&mut self, at: Option<(f32, f32)>) {
        if self.marker != at {
            self.marker = at;
            self.dirty = true;
        }
    }

    /// A picture of the window is wanted of the next frame drawn (`late`:
    /// even if the panels are still moving).
    pub fn want_picture(&mut self, late: bool) {
        if self.capture != Some(late) {
            self.capture = Some(late);
            self.dirty = true;
        }
    }

    /// The picture asked for, once drawn (PNG bytes), or why not.
    pub fn take_picture(&mut self) -> Option<Result<Vec<u8>, String>> {
        self.picture.take()
    }

    /// The model EDIT MODEL asked to open (its path in the game folder),
    /// once.
    pub fn take_model_edit(&mut self) -> Option<String> {
        self.ed.pieces.wants_model.take()
    }

    /// The level round the model EDIT asked to open, for a piece: to shape
    /// it in place (`context.rs`), once.
    pub fn take_model_context(&mut self) -> Option<context::Seed> {
        self.ed.wants_context.take()
    }

    /// Opens the level round its model (EDIT on a piece of it): said over
    /// the panels where it cannot.
    pub fn set_context(&mut self, seed: context::Seed) {
        if let Err(e) = self.ed.set_context(seed) {
            self.ed.message = Some(format!("{e:#}"));
        }
        self.ed.remember_held();
        self.dirty = true;
    }

    /// The GPU it draws with, to open another window on.
    pub fn gpu(&self) -> &crate::window::Gpu {
        self.win.as_ref().expect("a window").screen.gpu()
    }

    /// The model file it shapes, if it shapes one.
    pub fn model_file(&self) -> Option<&str> {
        self.ed.model.as_ref().map(|_| self.ed.file.as_str())
    }

    /// Says a mistake over its panels (one of another window, say).
    pub fn say(&mut self, text: String) {
        self.ed.message = Some(text);
        self.dirty = true;
    }

    /// Brings the window to the front.
    pub fn raise(&mut self) {
        if let Some(w) = &mut self.win {
            w.screen.window.raise();
        }
    }

    /// Opens the level's block `name` to shape it (nothing if the level
    /// has no such block).
    pub fn open_block(&mut self, name: &str) {
        if let Some(i) = self.ed.level.blocks.iter().position(|b| b.name == name) {
            self.ed.hold(i);
            self.ed.open_block(i);
            self.dirty = true;
        }
    }

    /// Takes in an event of its window (`shift`: a shift key is down);
    /// false when the window is to close.
    pub fn handle(&mut self, event: &sdl3::event::Event, shift: bool) -> bool {
        use sdl3::event::{Event, WindowEvent};
        use sdl3::keyboard::Keycode;
        use sdl3::mouse::MouseButton;

        if self.closing {
            return false;
        }
        self.input.handle(event, || None);
        // While the question is asked only its buttons answer (the panels
        // take the clicks); the level behind is not touched.
        if self.ed.question.is_some() {
            use sdl3::event::{Event, WindowEvent};
            self.dirty = true;
            match *event {
                Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return false,
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => {
                    if let Some(win) = &mut self.win {
                        win.screen.resize(&mut win.renderer, w, h);
                    }
                }
                _ => {}
            }
            return true;
        }
        let ((pw, ph), [wide, high]) = self.sizes();
        let Self { win, ed, view, shown, figures, cfg, keys, rects, gui, input, steps_at_click, .. } = self;
        self.dirty = true;
        let aspect = wide as f32 / high.max(1) as f32;
        let share = |x: f32, y: f32| (x / pw.max(1.0), y / ph.max(1.0));
        // The mouse over the panels (or a list or picker of theirs open
        // there) is theirs, not the 3D view's.
        // (By where the mouse is now, not where egui last saw it: a click
        // whose motion, press and release come at once is the view's.)
        let over = |x: f32, y: f32| {
            let p = input.point(x, y);
            rects.iter().any(|r| r.contains(p)) || gui.egui_is_using_pointer() || gui.layer_id_at(p).is_some_and(|l| l.order != egui::Order::Background)
        };
        match *event {
            Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return false,
            Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => {
                if let Some(win) = win {
                    win.screen.resize(&mut win.renderer, w, h);
                }
            }
            // Ctrl down or up: BOX and SPHERE cut away while it is held.
            Event::KeyDown { keycode: Some(Keycode::LCtrl | Keycode::RCtrl), .. } => view.cut = true,
            Event::KeyUp { keycode: Some(Keycode::LCtrl | Keycode::RCtrl), .. } => view.cut = false,
            // Alt: they fill only where it is empty.
            Event::KeyDown { keycode: Some(Keycode::LAlt | Keycode::RAlt), .. } => view.only_empty = true,
            Event::KeyUp { keycode: Some(Keycode::LAlt | Keycode::RAlt), .. } => view.only_empty = false,
            // Shift down: the tool is PICK until it is up (`shift` still
            // says so when the other shift key is down). Losing the focus
            // with it held would never see it let go.
            Event::KeyDown { keycode: Some(Keycode::LShift | Keycode::RShift), .. } => ed.shift_pick(true),
            Event::KeyUp { keycode: Some(Keycode::LShift | Keycode::RShift), .. } => ed.shift_pick(shift),
            // The focus lost (another window, the desktop), the window
            // hidden: a drag still open is cancelled, all as before it.
            Event::Window { win_event: WindowEvent::FocusLost | WindowEvent::Hidden | WindowEvent::Minimized, .. } => {
                view.cut = false;
                view.only_empty = false;
                ed.shift_pick(false);
                if ed.drag_open() {
                    ed.end_drag(true);
                    ed.end_whole_stroke();
                }
                view.press_at = None;
                view.held = 0;
                view.marquee = None;
            }
            // Another button while the left one drags: cancelled.
            Event::MouseButtonDown { mouse_btn: MouseButton::Right | MouseButton::Middle, .. } if ed.drag_open() => {
                ed.end_drag(true);
                ed.end_whole_stroke();
                view.press_at = None;
            }
            // Back to the window: its files as they are now (another
            // program's change comes in); at the level, the pieces' models
            // too (one changed in the model editor shows).
            Event::Window { win_event: WindowEvent::FocusGained, .. } => {
                ed.look_at_disk();
                if ed.scope == Scope::Level {
                    *figures = Figures::load(&ed.dir, cfg).with_pieces(&ed.dir, &ed.level);
                }
            }
            // F1: the page of the keys; Esc closes it first.
            Event::KeyDown { keycode: Some(Keycode::F1), .. } if !gui.egui_wants_keyboard_input() => view::act(ed, view, view::Action::Keys),
            Event::KeyDown { keycode: Some(Keycode::Escape), .. } if view.keys_shown => view.keys_shown = false,
            Event::KeyDown { keycode: Some(key), keymod, .. } if ed.among_pieces() && !gui.egui_wants_keyboard_input() && pieces_key(ed, view, key, keymod) => {}
            Event::KeyDown { keycode: Some(key), keymod, .. } if !gui.egui_wants_keyboard_input() => {
                key_down(ed, view, keys, key, keymod);
            }
            Event::MouseMotion { mousestate, x, y, xrel, yrel, .. } => {
                Here::of(ed, view, shown, figures, gui, input, rects, steps_at_click, (pw, ph), aspect).motion(mousestate, x, y, xrel, yrel, shift);
            }
            // A figure in GAME VIEW is only looked at.
            Event::MouseButtonDown { mouse_btn: MouseButton::Left, .. } if view.before_game.is_some() && ed.scope != Scope::Level => {}
            Event::MouseButtonDown { mouse_btn: MouseButton::Left, clicks, x, y, which, .. } if !over(x, y) => {
                Here::of(ed, view, shown, figures, gui, input, rects, steps_at_click, (pw, ph), aspect).left_down(clicks, x, y, which);
            }
            // BOX and SPHERE: the footprint let go is set right where it
            // was drawn (Alt: only where empty; Ctrl, held as it was
            // pressed: cut away), fresh for + and - until the next action.
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } if ed.slicing() => {
                view.held &= !HELD_LEFT;
                ed.let_go_of_slice();
            }
            // SELECT's rectangle let go: what it holds selected.
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, x, y, .. } if view.marquee.is_some() => {
                view.held &= !HELD_LEFT;
                if let Some(mut m) = view.marquee.take() {
                    let (sx, sy) = share(x, y);
                    m.to = [sx, sy];
                    let size = glam::Vec2::new(pw, ph);
                    let at = |s: [f32; 2]| glam::Vec2::new(s[0] * pw, s[1] * ph);
                    if ed.among_land() {
                        ed.select_points(view.view_proj(&view.level, aspect), size, at(m.from), at(m.to), m.mode);
                    } else {
                        let vp = view.view_proj(&view.block, aspect);
                        ed.select_rect(vp, size, at(m.from), at(m.to), m.mode, ed.selecting.select_through);
                    }
                }
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } if ed.forms() && ed.joint_drag().is_none() => {
                view.held &= !HELD_LEFT;
                if ed.forms.drawing.is_some() {
                    ed.set_drawn(view.only_empty);
                }
                ed.end_whole_stroke();
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } => {
                view.held &= !HELD_LEFT;
                ed.release();
                ed.let_go_of_selection();
                ed.end_whole_stroke();
                view.press_at = None;
            }
            // A right click (not a drag) among the pieces: the menu of what
            // is under the mouse (chosen), or of the floor there.
            Event::MouseButtonDown { mouse_btn: MouseButton::Right, x, y, .. } => {
                view.right_press = (!over(x, y)).then_some((x, y));
                if !over(x, y) {
                    view.held |= HELD_RIGHT;
                }
            }
            Event::MouseButtonDown { mouse_btn: MouseButton::Middle, x, y, .. } if !over(x, y) => view.held |= HELD_MIDDLE,
            Event::MouseButtonUp { mouse_btn: MouseButton::Middle, .. } => view.held &= !HELD_MIDDLE,
            Event::MouseButtonUp { mouse_btn: MouseButton::Right, .. } if ed.among_land() => {
                view.held &= !HELD_RIGHT;
                view.right_press = None;
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Right, x, y, .. } if !ed.among_pieces() => {
                view.held &= !HELD_RIGHT;
                // The voxel editor and the level of blocks: the menu of the
                // voxel (the part) or the block under the mouse.
                if view.right_press.take().is_some() {
                    let (sx, sy) = share(x, y);
                    let at = input.point(x, y);
                    if let Some(on) = voxel_menu(ed, view, aspect, sx, sy) {
                        if let view::MenuOn::Block(i) = on {
                            ed.hold(i);
                        }
                        view.menu = Some(view::Menu { on, at: [at.x, at.y] });
                    }
                }
            }
            Event::MouseButtonUp { mouse_btn: MouseButton::Right, x, y, .. } => {
                Here::of(ed, view, shown, figures, gui, input, rects, steps_at_click, (pw, ph), aspect).right_up(x, y);
            }
            // Over the panels the wheel scrolls them (egui), else it zooms.
            Event::MouseWheel { y, mouse_x, mouse_y, .. } if !over(mouse_x, mouse_y) => {
                view.orbit(ed.scope).zoom_by(y);
                view.leave_game_view(ed.scope);
            }
            _ => {}
        }
        // The view goes with a model's voxels when its box grew on the
        // low sides or was trimmed.
        let moved = std::mem::take(&mut ed.history.moved);
        view.block.target += moved.as_vec3();
        true
    }

    /// Draws a frame if something happened since the last, or the panels
    /// asked for one: the 3D view, the panels over it.
    pub fn draw(&mut self) -> Result<()> {
        // A drag whose let-go never came (released outside the windows,
        // over another, events lost): the real button is asked; up for a
        // while (its up event may just be on its way), the drag ends as let
        // go where it is, one step. (Not a program's pointer: no real
        // button to ask.)
        if self.ed.drag_open() && !self.view.synthetic_press && self.win.is_some() && !left_button_down() {
            let since = *self.view.up_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= LOST_RELEASE {
                self.mouse_up_elsewhere();
            }
        } else {
            self.view.up_since = None;
        }
        // A BACKGROUND chosen: kept for the next window, either editor's.
        if self.view.backgrounds_changed {
            self.view.backgrounds_changed = false;
            let path = background::Backgrounds::path();
            // (The other editor's as the file has it now: another window
            // may have chosen it.)
            let mut keep = background::Backgrounds::load(&path).colours();
            let k = (self.ed.scope != Scope::Level) as usize;
            keep[k] = self.view.backgrounds[k];
            if let Err(e) = background::Backgrounds::of(keep).save(&path) {
                eprintln!("background: {e:#}");
            }
            self.dirty = true;
        }
        if self.looked.elapsed() >= LOOK_EVERY {
            self.looked = Instant::now();
            if self.ed.look_at_disk() {
                self.dirty = true;
            }
        }
        if !self.dirty && self.repaint_at.is_none_or(|r| Instant::now() < r) {
            return Ok(());
        }
        self.dirty = false;
        let (_, size) = self.sizes();
        let (scale, density) = self.win.as_ref().map_or((1.0, 1.0), |w| (w.screen.window.display_scale(), w.screen.window.pixel_density()));
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let Self { win, ed, view, shown, figures, cfg, gui, input, thumbs, keys, rects, .. } = self;
        // The panels first: what was clicked there acts before the 3D view
        // is drawn.
        let raw = input.take(size, scale, density);
        let mut drawn = panels::PanelsDrawn::default();
        let marker = self.marker.map(|(x, y)| input.point(x, y));
        // A figure's parts as its frames stand now (a stroke, a step back),
        // the view kept on it; the pieces' models, a new one's too.
        ed.follow_figure();
        view.follow_origin(ed);
        ed.slice_toward(view.block_eye(aspect));
        figures.follow(&ed.dir, ed);
        ed.pieces.sizes = figures.sizes();
        view.aspect = aspect;
        let out = gui.run_ui(raw, |ui| {
            drawn = panels::ui(ui, ed, view, figures, thumbs);
            let free = room_of(&drawn, ui.ctx().content_rect()).0;
            panels::joints(ui, ed, view, free);
            panels::marks(ui, ed, view, figures, free);
            if let Some(tip) = marker {
                crate::gui::marker::paint(ui.ctx(), tip);
            }
        });
        let acted = !drawn.clicked.is_empty() || drawn.renamed.is_some() || drawn.renamed_part.is_some();
        if let Some((p, typed)) = drawn.renamed_part.take() {
            match ed.rename_part(p, &typed) {
                Ok(()) => (view.renaming_part, view.rename_problem) = (None, None),
                Err(problem) => view.rename_problem = Some(problem),
            }
        }
        if let Some((item, typed)) = drawn.renamed.take() {
            match ed.rename(item, &typed) {
                Ok(()) => (view.renaming, view.rename_problem) = (None, None),
                Err(problem) => view.rename_problem = Some(problem),
            }
        }
        for action in std::mem::take(&mut drawn.clicked) {
            // DON'T OPEN: the window goes, the file untouched.
            if action == view::Action::DontOpen {
                self.closing = true;
                continue;
            }
            view::act(ed, view, action);
        }
        view.set_room(room_of(&drawn, gui.content_rect()).1);
        (*keys, *rects) = (drawn.keys, drawn.rects);
        #[cfg(test)]
        {
            self.rows = drawn.rows;
        }
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // A picture asked for is taken once the panels have settled.
        let capture = self.capture.is_some_and(|late| late || (!acted && !delay.is_zero()));
        // A click changes what the panels show: once more at once.
        self.repaint_at = if acted { Some(Instant::now()) } else { Instant::now().checked_add(delay) };

        // The world brought up to the editor, the frame drawn.
        let rebuilt = shown.update(ed);
        let frame = view::frame(ed, view, shown, figures, &cfg.light, aspect);
        // (A window without a window draws nothing.)
        let Some(Win { screen, renderer, painter }) = win else {
            #[cfg(test)]
            {
                let whole = matches!(rebuilt, Ok(true));
                self.rebuilds += whole as usize;
                self.last = Some((frame, whole));
            }
            if let Err(e) = rebuilt {
                ed.message = Some(format!("{e:#}"));
            }
            return Ok(());
        };
        match rebuilt {
            Ok(true) => renderer.rebuild_all(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES),
            Ok(false) => {
                renderer.update_chunks(&screen.device, &screen.queue, &mut shown.world, &ALL_FACES);
            }
            Err(e) => ed.message = Some(format!("{e:#}")),
        }
        // A surface set up anew draws next time (`Screen::texture`).
        let texture = match screen.texture()? {
            crate::window::Got::Picture(texture) => texture,
            crate::window::Got::Wait => return Ok(()),
            crate::window::Got::Again => {
                self.dirty = true;
                return Ok(());
            }
        };
        let target = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = screen.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        renderer.render(&screen.device, &screen.queue, &mut encoder, &target, &frame);
        screen.queue.submit([encoder.finish()]);
        painter.paint(&screen.device, &screen.queue, &target, gui, out, size);
        if capture {
            self.capture = None;
            self.picture = Some(screen.picture(&texture.texture).map_err(|e| format!("{e:#}")));
        }
        screen.queue.present(texture);
        Ok(())
    }
}


/// A key the panels and the view take (not typed into a field, not one of
/// the pieces' own): BOX and SPHERE deeper, Escape's many ways back, the
/// selection's keys, undo and redo, a part renamed or removed, SLICE's
/// layer, a model's frames, a key the panels name.
fn key_down(ed: &mut Editor, view: &mut View, keys: &[(char, view::Action)], key: sdl3::keyboard::Keycode, keymod: sdl3::keyboard::Mod) {
    use sdl3::keyboard::{Keycode, Mod};
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD);
    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
    match key {
        // BOX and SPHERE: the shape a voxel deeper or shallower.
        Keycode::Plus | Keycode::Equals | Keycode::KpPlus if ed.forms() => ed.deepen(1),
        Keycode::Minus | Keycode::KpMinus if ed.forms() => ed.deepen(-1),
        Keycode::Escape if ed.forms() && ed.drop_form() => {}
        // SELECT's rectangle being dragged: let go of, nothing.
        Keycode::Escape if view.marquee.is_some() => view.marquee = None,
        // Among the terrain's points: none chosen (a drag going
        // on is cancelled first, below).
        Keycode::Escape if ed.among_land() && !ed.stroke_open() && (ed.land.chosen.is_some() || !ed.land.selected.is_empty()) => {
            ed.land.chosen = None;
            ed.land.selected.clear();
        }
        // A drag going on: cancelled, all as before it.
        Keycode::Escape if ed.joint_drag().is_some() || ed.gripped().is_some() || ed.dragging().is_some() || ed.stroke_open() => {
            ed.end_drag(true);
            ed.end_whole_stroke();
        }
        // The selection: none selected; its voxels gone; copied,
        // pasted.
        Keycode::Escape if ed.joint_drag().is_none() && view.menu.is_none() && ed.selection().is_some() => ed.clear_selection(),
        Keycode::Delete | Keycode::Backspace if ed.selection().is_some() => view::act(ed, view, view::Action::SelRemove),
        Keycode::C if ctrl && ed.selection().is_some() => view::act(ed, view, view::Action::SelCopy),
        Keycode::V if ctrl && ed.selecting.clipboard.is_some() => view::act(ed, view, view::Action::SelPaste),
        Keycode::Z if ctrl && shift => ed.redo(),
        // The chosen part: its name to type, removed.
        Keycode::F2 if ed.part_mode().is_some() => view::act(ed, view, view::Action::RenamePart(ed.figure.as_ref().and_then(|f| f.selected).unwrap_or(usize::MAX))),
        Keycode::Delete if ed.part_mode().is_some() => {
            if let Some(p) = ed.figure.as_ref().and_then(|f| f.selected).filter(|&p| ed.cannot_remove(p).is_none()) {
                view::act(ed, view, view::Action::RemovePart(p));
            }
        }
        // A model's frame, a level's block (the one held or open, its file
        // too): taken out by Delete, never by a letter beside the typing
        // keys; one step UNDO brings back whole.
        Keycode::Delete if matches!(ed.scope, Scope::Frame(_)) && ed.figure.is_none() => view::act(ed, view, view::Action::DeleteFrame),
        Keycode::Delete if matches!(ed.scope, Scope::Block(_)) || (ed.scope == Scope::Level && ed.layer == Layer::Blocks) => view::act(ed, view, view::Action::Delete),
        Keycode::Z if ctrl => ed.undo(),
        Keycode::Y if ctrl => ed.redo(),
        // SLICE's layer, a voxel at a time.
        Keycode::PageUp if ed.slice.is_some() => {
            let at = ed.slice.as_ref().map_or(0, |s| s.at);
            ed.set_slice_at(at + 1);
        }
        Keycode::PageDown if ed.slice.is_some() => {
            let at = ed.slice.as_ref().map_or(0, |s| s.at);
            ed.set_slice_at(at - 1);
        }
        // A model's frames, one by one.
        Keycode::Left => ed.step_frame(-1),
        Keycode::Right => ed.step_frame(1),
        Keycode::Escape => {
            if view.menu.is_some() {
                view.menu = None;
            } else if ed.cancel_joint() {
            } else if view.choosing {
                view.choosing = false;
            } else if ed.tool != Tool::Pointer && Tool::of(ed.scope, ed.layer).contains(&Tool::Pointer) {
                // Back to POINTER, which changes nothing.
                ed.choose(Tool::Pointer);
            } else {
                view::act(ed, view, view::Action::Back);
            }
        }
        // A key the panels name (a word's, a colour's digit;
        // by what the key says).
        _ if !ctrl => {
            let name = key.name();
            let mut chars = name.chars();
            if let (Some(c), None) = (chars.next(), chars.next())
                && c.is_ascii_alphanumeric()
                && let Some(action) = panels::key_action(keys, c)
            {
                view::act(ed, view, action);
            }
        }
        _ => {}
    }
}

/// What an event of the window works on (`EditorWindow::handle`): the
/// editor, its view and what they show, the panels as last drawn, the
/// steps there were at the last click, the window's size in points and
/// the view's shape.
struct Here<'a> {
    ed: &'a mut Editor,
    view: &'a mut View,
    shown: &'a mut Shown,
    figures: &'a mut Figures,
    gui: &'a egui::Context,
    input: &'a crate::gui::input::Input,
    rects: &'a [egui::Rect],
    steps_at_click: &'a mut usize,
    size: (f32, f32),
    aspect: f32,
}

impl<'a> Here<'a> {
    #[allow(clippy::too_many_arguments)]
    fn of(ed: &'a mut Editor, view: &'a mut View, shown: &'a mut Shown, figures: &'a mut Figures, gui: &'a egui::Context, input: &'a crate::gui::input::Input, rects: &'a [egui::Rect], steps_at_click: &'a mut usize, size: (f32, f32), aspect: f32) -> Self {
        Self { ed, view, shown, figures, gui, input, rects, steps_at_click, size, aspect }
    }

    /// The mouse moved: the camera turned or moved, what is dragged
    /// follows, what lies under it lit.
    fn motion(self, mousestate: sdl3::mouse::MouseState, x: f32, y: f32, xrel: f32, yrel: f32, shift: bool) {
        let Here { ed, view, shown, figures, gui, input, rects, size: (pw, ph), aspect, .. } = self;
        let share = |x: f32, y: f32| (x / pw.max(1.0), y / ph.max(1.0));
        // The mouse over the panels (or a list or picker of theirs open
        // there) is theirs, not the 3D view's.
        let over = |x: f32, y: f32| {
            let p = input.point(x, y);
            rects.iter().any(|r| r.contains(p)) || gui.egui_is_using_pointer() || gui.layer_id_at(p).is_some_and(|l| l.order != egui::Order::Background)
        };
        // A button counts as held only if it went down in this
        // window's view and was not let go since (as the window
        // heard it): a motion carrying a button held from elsewhere
        // (pressed on a panel, in another window, a press whose
        // release was lost) drags nothing and turns nothing.
        let held = |b: u8| view.held & b != 0;
        let left = mousestate.left() && held(HELD_LEFT);
        let right = mousestate.right() && held(HELD_RIGHT);
        let middle = mousestate.middle() && held(HELD_MIDDLE);
        let (sx, sy) = share(x, y);
        let (dx, dy) = share(xrel, yrel);
        if middle || (right && shift) {
            view.orbit(ed.scope).pan(dx, dy, aspect);
            view.leave_game_view(ed.scope);
        } else if right {
            view.orbit(ed.scope).turn(dx, dy);
            view.leave_game_view(ed.scope);
        }
        // The right button dragged: no click, no menu.
        if let Some((px, py)) = view.right_press
            && (px - x).abs() + (py - y).abs() > 5.0
        {
            view.right_press = None;
        }
        let over = over(x, y) && !left;
        // SELECT's rectangle follows the pointer.
        if let Some(m) = &mut view.marquee
            && left
        {
            let (sx, sy) = share(x, y);
            m.to = [sx, sy];
        }
        // Gone a few pixels from the press: a drag now.
        if let Some((px, py)) = view.press_at
            && (px - x).hypot(py - y) >= handles::TRAVEL
        {
            view.press_at = None;
        }
        if !ed.among_pieces() {
            let free = !(over || left);
            view.slice_hover = free && slice_under(ed, view, gui.content_rect(), input.point(x, y)).is_some();
            view.ring_hover = free && !view.slice_hover && on_ring(ed, view, gui.content_rect(), input.point(x, y));
            view.grip_hover = if !free || view.ring_hover || view.slice_hover { None } else { joint_under(ed, view, gui.content_rect(), input.point(x, y)).map(|(g, ..)| g) };
        }
        if ed.among_pieces() {
            let (origin, dir) = view.voxel_ray(aspect, sx, sy);
            let room = gui.content_rect();
            let mouse = input.point(x, y);
            if left && ed.gripped().is_some() {
                let vp = view.view_proj(&view.level, room.width() / room.height().max(1.0));
                ed.drag_grip(vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), (origin, dir));
            } else if left && ed.dragging().is_some() && view.press_at.is_none() {
                ed.drag_item(origin, dir);
            } else if ed.pieces.hanging.is_some() {
                ed.hang_at(if over { None } else { view::set_down_spot(ed, figures, &shown.world, origin, dir) });
            }
            let busy = over || ed.pieces.hanging.is_some() || left;
            view.grip_hover = if busy { None } else { stage::grip_under(ed, view, room, mouse) };
            view.over = if busy || view.grip_hover.is_some() { None } else { view::item_along(ed, figures, origin, dir) };
        } else if ed.among_land() {
            // The point under the mouse lit; a press held drags it
            // (POINTER) or brushes over the points (the others).
            let (vp, size, m) = land_screen(view, gui.content_rect(), input.point(x, y));
            let p = if over { None } else { ed.point_at(vp, size, m) };
            ed.land.hover = p;
            ed.land.near = if over { None } else { ed.nearest_point(vp, size, m) };
            if left && ed.land_dragging() && view.press_at.is_none() {
                ed.land_drag(p, view.voxel_ray(aspect, sx, sy), shift);
            }
        } else if among_things(ed) {
            (view.spot, view.under) = if over { (None, None) } else { view.aim_things(ed, shown, figures, aspect, sx, sy) };
            if left {
                ed.drag_thing(view.spot);
            }
        } else if ed.slicing() && left {
            // SLICE's arrow dragged.
            let room = gui.content_rect();
            let mouse = input.point(x, y);
            let vp = view.view_proj(&view.block, room.width() / room.height().max(1.0));
            let ray = view.ray(ed, aspect, sx, sy);
            ed.drag_slice(vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), ray);
        } else if ed.joint_drag().is_some() && left {
            // A joint's arrow dragged.
            let room = gui.content_rect();
            let mouse = input.point(x, y);
            let vp = view.view_proj(&view.block, room.width() / room.height().max(1.0));
            let ray = view.ray(ed, aspect, sx, sy);
            // (Shift: finer, a tenth of a voxel, a degree.)
            ed.set_fine(shift);
            ed.selecting.sel_keep = view.only_empty;
            ed.drag_joint(vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), ray);
            // (A part moved moves the grid shown: the view keeps the
            // figure still at once, for the next motion's ray.)
            view.follow_origin(ed);
        } else if ed.forms() {
            // BOX and SPHERE: the footprint dragged out on its face's
            // plane.
            view.hover = if over { None } else { view.aim(ed, aspect, sx, sy) };
            if left && ed.forms.drawing.is_some() {
                let (origin, dir) = view.ray(ed, aspect, sx, sy);
                ed.draw_form(origin, dir);
            }
        } else {
            view.hover = if over { None } else { view.aim(ed, aspect, sx, sy) };
            if left
                && let Some(hit) = view.hover
            {
                ed.drag(hit);
            }
        }
    }

    /// The left button down in the view: what the tool, the scope and
    /// what lies under it make of the press.
    fn left_down(self, clicks: u8, x: f32, y: f32, which: u32) {
        let Here { ed, view, shown, figures, gui, input, steps_at_click, size: (pw, ph), aspect, .. } = self;
        let share = |x: f32, y: f32| (x / pw.max(1.0), y / ph.max(1.0));
        view.held |= HELD_LEFT;
        // Where the press was (a drag moves things only once the
        // pointer has gone a few pixels), and whether a program's
        // pointer made it (`ide::remote`: no real button to ask).
        view.press_at = Some((x, y));
        view.synthetic_press = which == crate::ide::remote::SYNTHETIC;
        let (sx, sy) = share(x, y);
        let sliced = if ed.among_pieces() { None } else { slice_under(ed, view, gui.content_rect(), input.point(x, y)) };
        let ring = sliced.is_none() && !ed.among_pieces() && on_ring(ed, view, gui.content_rect(), input.point(x, y));
        let joint = if ring || sliced.is_some() { None } else { joint_under(ed, view, gui.content_rect(), input.point(x, y)) };
        if !ed.among_pieces() {
            view.menu = None;
        }
        if let Some((grip, h, vp, mouse)) = sliced {
            // SLICE's arrow: dragged until let go.
            let room = gui.content_rect();
            if let Some(drag) = handles::Drag::begin(&h, grip, vp, glam::Vec2::new(room.width(), room.height()), mouse, view.ray(ed, aspect, sx, sy)) {
                ed.grab_slice(drag, h);
            }
        } else if ring {
            // The joint's ring: dragged across the view.
            let look = crate::camera::view_dir(&view.block.camera());
            ed.grab_joint(view.ray(ed, aspect, sx, sy), look);
        } else if let Some((grip, h, vp, mouse)) = joint {
            // The part's arrow: dragged until let go.
            let room = gui.content_rect();
            if let Some(drag) = handles::Drag::begin(&h, grip, vp, glam::Vec2::new(room.width(), room.height()), mouse, view.ray(ed, aspect, sx, sy)) {
                ed.grab_part(drag, h);
            }
        } else if ed.tool == Tool::Select && ed.scope != Scope::Level {
            // SELECT: a rectangle on the screen from here (on empty
            // space too), its mode the lit one or Ctrl (ADD), Alt
            // (REMOVE); a double click the color connected.
            if clicks >= 2 {
                view.marquee = None;
                let (origin, dir) = view.ray(ed, aspect, sx, sy);
                let add = ed.selecting.select_mode == model::selection::SelectMode::Add || view.cut;
                match ed.scope {
                    Scope::Figure => {
                        if let Some((hit, p)) = ed.aim_whole(origin, dir) {
                            ed.select_connected(hit.cell, Some(p), add);
                        }
                    }
                    _ => {
                        if let Some(hit) = view.aim(ed, aspect, sx, sy).filter(|h| h.cell.z >= 0) {
                            ed.select_connected(hit.cell, None, add);
                        }
                    }
                }
            } else {
                let mode = if view.cut { model::selection::SelectMode::Add } else if view.only_empty { model::selection::SelectMode::Remove } else { ed.selecting.select_mode };
                view.marquee = Some(view::Marquee { from: [sx, sy], to: [sx, sy], mode });
            }
        } else if ed.scope == Scope::Figure {
            // The figure whole: the tool works on the part it meets
            // (the figure stays chosen, nothing dimmed); a double
            // click chooses that part, the first click's work taken
            // back. A click never both chooses and paints.
            let (origin, dir) = view.ray(ed, aspect, sx, sy);
            if ed.tool == Tool::Pointer {
                // POINTER: the part clicked chosen, nothing changed.
                if let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box())
                    && let Some(p) = figure.part_at(lo, origin, dir)
                {
                    view::act(ed, view, view::Action::Part(p));
                }
            } else if let (Some(figure), Some((lo, _))) = (&ed.figure, ed.figure_box())
                && let Some(p) = figure.part_at(lo, origin, dir)
            {
                if clicks >= 2 {
                    if ed.steps() > *steps_at_click {
                        ed.take_back();
                    }
                    view::act(ed, view, view::Action::Part(p));
                } else {
                    *steps_at_click = ed.steps();
                    ed.begin_whole_stroke(p);
                    // (The grid shown is the part's now: the view
                    // kept still, the ray taken anew.)
                    view.follow_origin(ed);
                    if let Some(hit) = view.aim(ed, aspect, sx, sy) {
                        if ed.tool == Tool::Fill {
                            ed.fill_at(hit, view.cut);
                        } else if ed.forms() {
                            ed.begin_form(hit, view.cut);
                        } else {
                            ed.press(hit);
                        }
                    }
                }
            }
        } else if ed.among_land() {
            // SELECT: a rectangle on the screen; else the point under
            // the mouse chosen and grabbed, or brushed.
            if ed.land.tool == model::land::LandTool::Select {
                let mode = if view.cut { model::selection::SelectMode::Add } else if view.only_empty { model::selection::SelectMode::Remove } else { ed.land.mode };
                view.marquee = Some(view::Marquee { from: [sx, sy], to: [sx, sy], mode });
            } else {
                let (vp, size, m) = land_screen(view, gui.content_rect(), input.point(x, y));
                let p = ed.point_at(vp, size, m);
                ed.land_press(p, view.voxel_ray(aspect, sx, sy));
            }
        } else if ed.among_pieces() {
            // What hangs on the mouse set down; a handle of the
            // chosen one grabbed; else the item under it chosen, to
            // be dragged (nothing: none chosen, the level shown); a
            // double click opens a piece's model.
            let (origin, dir) = view.voxel_ray(aspect, sx, sy);
            let room = gui.content_rect();
            let mouse = input.point(x, y);
            view.menu = None;
            let picked = view::pick_along(ed, figures, origin, dir);
            if ed.pieces.hanging.is_some() {
                // A model set down stays on the mouse for the next
                // click (a stamp: trees by the dozen) until Escape;
                // a double click's second click sets none.
                if clicks < 2 {
                    ed.hang_at(view::set_down_spot(ed, figures, &shown.world, origin, dir));
                    ed.stamp();
                }
            } else if clicks >= 2 {
                if let Some((item @ Item::Thing(i), _)) = picked
                    && ed.level.things[i].model.is_some()
                {
                    ed.choose_item(Some(item));
                    ed.edit_model();
                }
            } else if let (Some(grip), Some((h, vp, _))) = (stage::grip_under(ed, view, room, mouse), stage::handles_on_screen(ed, view, room)) {
                ed.grab(grip, h, vp, glam::Vec2::new(room.width(), room.height()), glam::Vec2::new(mouse.x - room.left(), mouse.y - room.top()), (origin, dir));
            } else {
                ed.press_item(picked.map(|(item, _)| item), origin, dir);
                ed.choose_light(picked.and_then(|(_, light)| light));
                (view.renaming, view.level_settings) = (None, false);
            }
        } else if among_things(ed) {
            // A second click of a double click would put down or
            // take away a second thing at once.
            if clicks < 2 {
                (view.spot, view.under) = view.aim_things(ed, shown, figures, aspect, sx, sy);
                ed.press_thing(view.spot, view.under);
            }
        } else if clicks >= 2 && ed.scope == Scope::Level {
            // A double click opens the block clicked on.
            if ed.steps() > *steps_at_click {
                ed.take_back();
            }
            let block = view.aim(ed, aspect, sx, sy).map(|hit| ed.level.cell(hit.cell).block).unwrap_or(0);
            if block != 0 {
                ed.hold(block as usize - 1);
                ed.open_block(block as usize - 1);
            }
        } else if ed.tool == Tool::Pointer && ed.scope != Scope::Level {
            // POINTER: in a figure's part, a click on another part
            // (seen through) chooses it, on empty space the figure
            // whole; nothing is changed, nothing dragged.
            if ed.part_mode().is_some() {
                let (origin, dir) = view.ray(ed, aspect, sx, sy);
                let (origin, dir) = (view.rest.transform_point3(origin), view.rest.transform_vector3(dir));
                let met = ed.figure.as_ref().zip(ed.figure_box()).and_then(|(f, (lo, _))| f.part_at(lo, origin, dir));
                let chosen = ed.figure.as_ref().and_then(|f| f.selected);
                match met {
                    Some(p) if Some(p) != chosen => view::act(ed, view, view::Action::Part(p)),
                    Some(_) => {}
                    None => view::act(ed, view, view::Action::WholeFigure),
                }
            }
        } else if let Some(hit) = view.aim(ed, aspect, sx, sy) {
            *steps_at_click = ed.steps();
            if ed.tool == Tool::Fill {
                ed.fill_at(hit, view.cut);
            } else if ed.forms() {
                ed.begin_form(hit, view.cut);
            } else {
                ed.press(hit);
            }
        } else if ed.selection().is_some() {
            // A click on empty space: none selected.
            ed.clear_selection();
        } else if ed.part_mode().is_some() {
            // A part shaped, a click on empty space: the figure
            // whole again.
            view::act(ed, view, view::Action::WholeFigure);
        }
    }

    /// The right button let go among the pieces, without a drag: the menu
    /// of what is under the mouse, or of the floor there.
    fn right_up(self, x: f32, y: f32) {
        let Here { ed, view, shown, figures, gui, input, size: (pw, ph), aspect, .. } = self;
        let share = |x: f32, y: f32| (x / pw.max(1.0), y / ph.max(1.0));
        view.held &= !HELD_RIGHT;
        if view.right_press.take().is_some() && ed.among_pieces() && ed.pieces.hanging.is_none() {
            let (sx, sy) = share(x, y);
            let (origin, dir) = view.voxel_ray(aspect, sx, sy);
            let at = input.point(x, y);
            let room = gui.content_rect();
            // On the chosen one's handles (or within them, where
            // the ray meets nothing): its menu.
            let on_handles = stage::grip_under(ed, view, room, at).is_some() || stage::handles_on_screen(ed, view, room).is_some_and(|(_, _, b)| b.contains(at));
            match (view::pick_along(ed, figures, origin, dir), ed.pieces.chosen) {
                (_, Some(item)) if stage::grip_under(ed, view, room, at).is_some() => view::act(ed, view, view::Action::OpenMenu(item, [at.x, at.y])),
                (Some((item, light)), _) => {
                    let light = if ed.pieces.chosen == Some(item) { ed.pieces.chosen_light.or(light) } else { light };
                    view::act(ed, view, view::Action::OpenMenu(item, [at.x, at.y]));
                    ed.choose_light(light);
                }
                (None, Some(item)) if on_handles => view::act(ed, view, view::Action::OpenMenu(item, [at.x, at.y])),
                (None, _) => {
                    if let Some(spot) = view::floor_spot(&shown.world, origin, dir) {
                        view.menu = Some(view::Menu { on: view::MenuOn::Floor(spot), at: [at.x, at.y] });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests;

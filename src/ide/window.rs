//! The IDE's window and what every page shown in it shares: SDL's events
//! into egui, a frame of egui painted onto the window, what egui wants of
//! the system, the system's folder dialog; the upgrade's page and the
//! start page, each a loop of such frames.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::gui::{Painter, input as sdl_egui};
use super::{Next, choose, prefs, projects, wizard};

/// A folder the system's dialog picked: a game's, or where NEW puts games.
pub(super) enum Picked {
    Game(PathBuf),
    Projects(PathBuf),
}

/// The IDE's window and what every game shown in it shares.
pub(super) struct Window {
    pub sdl: sdl3::Sdl,
    pub video: sdl3::VideoSubsystem,
    pub screen: crate::window::Screen,
    pub ctx: egui::Context,
    pub painter: Painter,
    pub input: sdl_egui::Input,
    /// Set until the next frame once the window was woken from another
    /// thread (the console's output, the bridge, the dialog), and what
    /// wakes it.
    pub woken: Arc<AtomicBool>,
    pub wake: Arc<dyn Fn() + Send + Sync>,
    /// Gamepads: every one plugged in, now or later, drives the game.
    pub pads: crate::pads::Pads,
    pub events: sdl3::EventPump,
    cursor: (egui::CursorIcon, Option<sdl3::mouse::Cursor>),
    /// The folders the system's dialog picked.
    pub picked: (mpsc::Sender<Picked>, mpsc::Receiver<Picked>),
    /// What the game opened next is to say in OUTPUT (which one was opened
    /// of the games below a folder).
    pub note: Option<String>,
}

/// A frame of egui as the window runs it: what egui made of it, at the
/// window's size and scale.
pub(super) struct Frame {
    pub out: egui::FullOutput,
    pub size: [u32; 2],
    pixels_per_point: f32,
}

/// A window's picture read back: PNG bytes, or why not.
pub(super) type Picture = Result<Vec<u8>, String>;

impl Window {
    pub fn open() -> Result<Self> {
        use sdl3::event::Event;
        let sdl = sdl3::init().context("initializing SDL")?;
        let video = sdl.video().context("initializing SDL video")?;
        let screen = crate::window::Screen::open(&video, "Voxcade", 1600, 1000, false)?;
        video.text_input().start(&screen.window);
        let ctx = egui::Context::default();
        let painter = Painter::new(&screen.device, screen.config.format, false);
        // Another thread wakes the window with an event of our own, once
        // until the next frame.
        let woken = Arc::new(AtomicBool::new(false));
        let wake: Arc<dyn Fn() + Send + Sync> = {
            let (woken, sender) = (woken.clone(), sdl.event().context("SDL events")?.event_sender());
            // Safety: registering an event number has no conditions.
            let kind = unsafe { sdl.event().context("SDL events")?.register_event() }.context("an SDL event for the console")?;
            Arc::new(move || {
                if !woken.swap(true, Ordering::AcqRel) {
                    let _ = sender.push_event(Event::User { timestamp: 0, window_id: 0, type_: kind, code: 0, data1: std::ptr::null_mut(), data2: std::ptr::null_mut() });
                }
            })
        };
        let pads = crate::pads::Pads::new(&sdl);
        let events = sdl.event_pump().context("SDL events")?;
        Ok(Self { sdl, video, screen, ctx, painter, input: sdl_egui::Input::default(), woken, wake, pads, events, cursor: (egui::CursorIcon::Default, None), picked: mpsc::channel(), note: None })
    }

    /// egui with nothing remembered of widgets, in the IDE's look (its
    /// textures stay its own: the old game's pictures are freed with the
    /// next frame), under `title`.
    pub fn fresh(&mut self, title: &str) {
        let _ = self.screen.window.set_title(title);
        self.ctx.memory_mut(|m| *m = egui::Memory::default());
        crate::gui::style(&self.ctx);
    }

    /// Waits for SDL's events until `at` (None: until one comes) and gives
    /// them to egui; false once the window is closed (the rest of the
    /// events left).
    fn take_events(&mut self, at: Option<Instant>) -> bool {
        use sdl3::event::{Event, WindowEvent};
        let first = self.wait(at);
        let clipboard = self.video.clipboard();
        for event in first.into_iter().chain(self.events.poll_iter().collect::<Vec<_>>()) {
            match &event {
                Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return false,
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => self.screen.resize_surface(*w, *h),
                _ => {}
            }
            self.input.handle(&event, || clipboard.clipboard_text().ok().filter(|t| !t.is_empty()));
        }
        true
    }

    /// Waits for the first event until `at` (None: an hour), the window
    /// woken from now on again.
    pub fn wait(&mut self, at: Option<Instant>) -> Option<sdl3::event::Event> {
        let timeout = at.map_or(Duration::from_secs(3600), |r| r.saturating_duration_since(Instant::now()));
        let first = if timeout.is_zero() { self.events.poll_event() } else { self.events.wait_event_timeout(timeout.max(Duration::from_millis(1))) };
        self.woken.store(false, Ordering::Release);
        first
    }

    /// A frame of egui at the window's size, `ui` drawing it.
    pub fn run(&mut self, ui: impl FnMut(&mut egui::Ui)) -> Frame {
        let size = [self.screen.config.width, self.screen.config.height];
        let pixels_per_point = self.screen.window.display_scale();
        let out = self.ctx.run_ui(self.input.take(size, pixels_per_point, self.screen.window.pixel_density()), ui);
        Frame { out, size, pixels_per_point }
    }

    /// What egui wants of the system after `frame`: the mouse's look, the
    /// clipboard (what went wrong copying), where typed text goes.
    pub fn platform(&mut self, frame: &Frame) -> Option<String> {
        let (platform, video, screen) = (&frame.out.platform_output, &self.video, &self.screen);
        if platform.cursor_icon != self.cursor.0 {
            self.cursor.1 = sdl3::mouse::Cursor::from_system(sdl_egui::cursor(platform.cursor_icon)).ok();
            if let Some(c) = &self.cursor.1 {
                c.set();
            }
            self.cursor.0 = platform.cursor_icon;
        }
        let mut trouble = None;
        for command in &platform.commands {
            if let egui::OutputCommand::CopyText(text) = command
                && let Err(e) = video.clipboard().set_clipboard_text(text)
            {
                trouble = Some(format!("COPYING FAILED: {e}"));
            }
        }
        if let Some(ime) = &platform.ime {
            let r = ime.cursor_rect * (frame.pixels_per_point / screen.window.pixel_density());
            video.text_input().set_rect(&screen.window, sdl3::rect::Rect::new(r.min.x as i32, r.min.y as i32, r.width().max(1.0) as u32, r.height().max(1.0) as u32), 0);
        }
        trouble
    }

    /// When egui wants the next frame after `frame` (None: only after an
    /// event), and how long that is.
    pub fn delay(frame: &Frame) -> Duration {
        frame.out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay)
    }

    /// Paints `frame` onto the window; false when the window's surface had
    /// to be set up anew (another frame is due). With `capture` the frame
    /// is also read back, as PNG bytes, before it is shown (none if the
    /// window was not drawn). A lost GPU device ends the window
    /// (`Screen::texture`).
    pub fn present(&mut self, frame: Frame, capture: bool) -> Result<(bool, Option<Picture>)> {
        let texture = match self.screen.texture()? {
            crate::window::Got::Picture(texture) => texture,
            crate::window::Got::Wait => return Ok((true, None)),
            crate::window::Got::Again => return Ok((false, None)),
        };
        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.painter.paint(&self.screen.device, &self.screen.queue, &view, &self.ctx, frame.out, frame.size);
        let picture = capture.then(|| self.screen.picture(&texture.texture).map_err(|e| format!("{e:#}")));
        self.screen.queue.present(texture);
        Ok((true, picture))
    }

    /// A page's frame shown: what egui wants of the system done (what went
    /// wrong, to say), painted; when the next frame is due.
    fn show(&mut self, frame: Frame) -> Result<(Option<Instant>, Option<String>)> {
        let trouble = self.platform(&frame);
        let mut at = Instant::now().checked_add(Self::delay(&frame));
        if !self.present(frame, false)?.0 {
            at = Some(Instant::now());
        }
        Ok((at, trouble))
    }

    /// The system's folder dialog over the window, starting `at`; the
    /// folder picked comes back as `what` makes of it, the window woken.
    pub fn browse(&self, at: PathBuf, what: fn(PathBuf) -> Picked) {
        let (send, wake) = (self.picked.0.clone(), self.wake.clone());
        let at = at.is_dir().then_some(at);
        sdl3::dialog::show_open_folder_dialog(
            at,
            false,
            &self.screen.window,
            Box::new(move |result, _| {
                if let Some(dir) = result.ok().and_then(|dirs| dirs.into_iter().next()) {
                    let _ = send.send(what(dir));
                    wake();
                }
            }),
        );
    }
}

/// The upgrade's page (`wizard`) in the IDE's window until the game opens,
/// the start page is wanted again or the window is closed.
pub(super) fn upgrade_page(window: &mut Window, mut page: wizard::Wizard, refit: wizard::Refitter) -> Result<Next> {
    window.fresh("Voxcade: bringing a game up to date");
    let mut repaint_at = Some(Instant::now());
    loop {
        // (Closed while it works: the thread ends with the program.)
        if !window.take_events(repaint_at) {
            return Ok(Next::Quit);
        }
        let mut wish = None;
        let frame = window.run(|ui| wish = page.ui(ui, refit));
        match wish {
            Some(wizard::Wish::Back) => return Ok(Next::Start(None)),
            Some(wizard::Wish::Open) => return Ok(Next::Game(page.dir.clone())),
            Some(wizard::Wish::Play) => return Ok(Next::Play(page.dir.clone())),
            None => {}
        }
        repaint_at = window.show(frame)?.0;
    }
}

/// The start page in the IDE's window until a game is chosen or the window
/// closed; with `play`, that game played at once.
pub(super) fn start_page(window: &mut Window, message: Option<String>, play: Option<PathBuf>, found: Option<(PathBuf, Vec<PathBuf>)>) -> Result<Next> {
    window.fresh("Voxcade");
    let (prefs, trouble) = match prefs::Prefs::load(&prefs::Prefs::path()) {
        Ok(prefs) => (prefs, None),
        Err(e) => (prefs::Prefs::default(), Some(format!("The settings: {e:#}"))),
    };
    let mut page = projects::StartPage::new(prefs, message.or(trouble), true);
    if let Some(dir) = play {
        page.play(&dir, &projects::title_of(&dir));
    }
    if let Some((folder, games)) = found {
        page.show_found(folder, games);
    }
    let mut repaint_at = Some(Instant::now());
    loop {
        if !window.take_events(repaint_at) {
            return Ok(Next::Quit);
        }
        while let Ok(folder) = window.picked.1.try_recv() {
            match folder {
                Picked::Game(dir) => match choose(&mut window.note, &dir) {
                    Next::Found(folder, games) => page.show_found(folder, games),
                    Next::Start(Some(message)) => page.tell(message),
                    next => return Ok(next),
                },
                Picked::Projects(dir) => page.set_projects(dir),
            }
        }
        let mut wish = None;
        let frame = window.run(|ui| wish = page.ui(ui));
        match wish {
            Some(projects::Wish::Open(dir)) => return Ok(Next::Game(dir)),
            Some(projects::Wish::Browse) => window.browse(page.prefs.projects_dir(), Picked::Game),
            Some(projects::Wish::BrowseProjects) => window.browse(page.prefs.projects_dir(), Picked::Projects),
            Some(projects::Wish::Upgrade(dir, play)) => return Ok(Next::Upgrade(dir, play)),
            None => {}
        }
        let (at, trouble) = window.show(frame)?;
        if let Some(trouble) = trouble {
            page.tell(trouble);
        }
        repaint_at = at;
    }
}

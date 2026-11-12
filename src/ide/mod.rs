//! Voxcade's IDE (`docs/ide.md`): one window to make a Lua game in, a very
//! stripped-down VS Code. On the left only the game's logic and its
//! assets, in the main area the one thing open (no tabs), PLAY and BUILD
//! along the top. It is drawn with egui on the engine's own window and GPU,
//! and it draws only when something happened.

pub mod bridge;
mod buffer;
mod claude;
mod code;
mod console;
mod export;
mod files;
mod help;
mod maker;
mod memory;
mod output;
mod play;
mod prefs;
pub use prefs::is_notes_file;
mod preview;
mod projects;
mod remote;
mod shell;
mod sounds;
mod templates;
mod terminal;
pub mod wizard;

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::game::Game;
use console::{Console, Start};
use crate::gui::{Painter, input as sdl_egui};
use play::Play;
use shell::{Ask, Shell, Stage};

/// How `voxcade ide --shot` draws the IDE without a window.
pub struct ShotOptions {
    /// The file to open (its path in the game folder), else what was open.
    pub open: Option<String>,
    /// A command to run in the console (through the system's shell) and
    /// wait for, at most ten seconds; else the console stays empty.
    pub console: Option<String>,
    /// The game played in the main area, this many ticks in.
    pub play: Option<u32>,
    /// A game made for another Voxcade: its upgrade's page as after
    /// UPGRADE, the upgrade run dry (nothing changed).
    pub upgraded: bool,
    pub width: u32,
    pub height: u32,
    pub out: PathBuf,
}

/// An editor's window the IDE opened, and what on.
#[derive(PartialEq)]
enum Editing {
    /// A level (its file) and its blocks.
    Level(String),
    /// A model (its path in the game folder).
    Model(String),
}

/// Why a call may not be answered, if it may not: typing into the console
/// is running commands in the user's shell, past the agent's permission
/// prompts, and the pointer and keys are a person's hands, so these tools
/// (the console's, and `remote::TOOLS`) are only for an IDE started with
/// `--remote` (another session's, testing), never the game's own agent.
fn refused(tool: &str, remote: bool) -> Option<String> {
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
    let text_of = text;
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
                    Ok(text) => call.answer(vec![text_of(if text.is_empty() { "done".into() } else { text })]),
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

/// Where an event goes: to the IDE's window, or to the editor window of
/// that id.
#[derive(Debug, PartialEq)]
enum Route {
    Ide,
    Editor(u32),
}

/// Where `event` goes when the IDE's window has the id `ide`: an event of
/// another window to that window, the rest (and the program's own) to the
/// IDE.
fn route(event: &sdl3::event::Event, ide: u32) -> Route {
    match event.get_window_id() {
        Some(id) if id != ide && id != 0 => Route::Editor(id),
        _ => Route::Ide,
    }
}

/// Draws the IDE as it opens into a PNG (without a game: its start page,
/// with the games opened last as the settings have them).
pub fn shot<G: Game>(dir: Option<&Path>, opts: &ShotOptions) -> Result<()> {
    let Some(dir) = dir else { return shot_start(opts) };
    // A game made for another Voxcade: the upgrade's page, as it opens or
    // (`--upgraded`) as after UPGRADE run dry.
    if wizard::needed(dir) {
        return shot_upgrade::<G>(dir, opts);
    }
    let start = opts.console.clone().map_or(Start::Nothing, Start::Command);
    let console = Console::new(dir.to_path_buf(), start, Arc::new(|| {}));
    let cfg = crate::session::config::<G>(dir)?;
    let brought = G::sounds(&cfg);
    let mut shell = Shell::new(dir, cfg, std::env::temp_dir().join(format!("voxcade-ide-shot-{}.toml", std::process::id())), console).with_sounds(brought);
    shell.animate = false;
    // A command run in the console is meant to be seen: unfolded (else a
    // shot shows a game's console as a game opens, folded).
    if opts.console.is_some() {
        shell.unfold_console();
    }
    // `--open help` shows HELP as it opens, at every name of the API
    // (`help:N` the example N, from 1); anything else is a file.
    match opts.open.as_deref().and_then(|o| o.strip_prefix("help")) {
        Some(topic) => {
            shell.help = true;
            shell.help_topic = topic.strip_prefix(':').and_then(|n| n.parse::<usize>().ok()).map_or(0, |n| n.min(help::TOPICS.len()));
        }
        None => {
            // `--open text:hero.vvx`: a model shown as its text.
            if let Some(path) = &opts.open {
                match path.strip_prefix("text:") {
                    Some(model) => {
                        shell.open(model);
                        shell.model_as_text();
                    }
                    None => shell.open(path),
                }
            }
        }
    }
    let mut play = None;
    let mut canvas = crate::headless::Canvas::new(opts.width, opts.height, false)?;
    let ctx = egui::Context::default();
    crate::gui::style(&ctx);
    let mut painter = Painter::new(canvas.gpu().0, wgpu::TextureFormat::Rgba8Unorm, false);
    let mut input = sdl_egui::Input::default();
    // Frames a sixtieth of a second apart until egui has laid out its first
    // ones anew (fonts, sizes) and its animations (scroll bars fading in)
    // have ended: two seconds at most.
    for frame in 0..120 {
        let mut raw = input.take([opts.width, opts.height], 1.0, 1.0);
        raw.time = Some(frame as f64 / 60.0);
        let out = ctx.run_ui(raw, |ui| shell.ui(ui));
        let mut settled = frame >= 2 && out.viewport_output.get(&egui::ViewportId::ROOT).is_none_or(|v| v.repaint_delay == Duration::MAX);
        let (device, queue, view) = canvas.gpu();
        // The game played, its ticks stepped at once, drawn every frame
        // (a file opened shows instead, the game held behind it).
        if let Some(ticks) = opts.play {
            if play.is_none() {
                let mut p = Play::<G>::start(dir, device, queue, None, None)?;
                p.step(ticks, &crate::input::Input::default());
                shell.stage = Some(Stage { shown: opts.open.is_none(), trouble: p.trouble(), unloaded: p.unloaded(), ..Stage::default() });
                play = Some(p);
            }
            if let (Some(p), Some(stage)) = (&mut play, &mut shell.stage)
                && let Some(size) = stage.wanted
            {
                let first = stage.picture.is_none();
                stage.picture = Some(p.draw(device, queue, &mut painter.renderer, size));
                stage.keys = p.keys();
                stage.trouble = p.trouble();
                stage.unloaded = p.unloaded();
                settled &= !first;
            }
        }
        // A picture drawn anew shows from the next frame on.
        if shell.render(device, queue, &mut painter.renderer) {
            settled = false;
        }
        painter.paint(device, queue, view, &ctx, out, [opts.width, opts.height]);
        // The console's command started on the first frame, at its size:
        // its output, until it ends.
        if frame == 0 {
            let until = Instant::now() + Duration::from_secs(10);
            while !shell.console().ended() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        if settled {
            break;
        }
    }
    let encoder = canvas.gpu().0.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let pixels = canvas.read(encoder)?;
    crate::headless::write_png(&opts.out, opts.width, opts.height, &pixels)?;
    println!("{}: {} open", opts.out.display(), shell.opened().unwrap_or("nothing"));
    Ok(())
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

/// What the IDE's window shows next: a game, the start page (saying what
/// went wrong, if something did), or nothing: it closes.
enum Next {
    Game(PathBuf),
    Start(Option<String>),
    /// A game made for another Voxcade: the upgrade's page (`play`: from
    /// the launcher's PLAY).
    Upgrade(PathBuf, bool),
    /// The launcher, playing this game at once (the upgrade's PLAY).
    Play(PathBuf),
    Quit,
}

/// A folder the system's dialog picked: a game's, or where NEW puts games.
enum Picked {
    Game(PathBuf),
    Projects(PathBuf),
}

/// The IDE's window and what every game shown in it shares.
struct Window {
    sdl: sdl3::Sdl,
    video: sdl3::VideoSubsystem,
    screen: crate::window::Screen,
    ctx: egui::Context,
    painter: Painter,
    input: sdl_egui::Input,
    /// Set until the next frame once the window was woken from another
    /// thread (the console's output, the bridge, the dialog), and what
    /// wakes it.
    woken: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// Gamepads: every one plugged in, now or later, drives the game.
    pads: crate::pads::Pads,
    events: sdl3::EventPump,
    cursor: (egui::CursorIcon, Option<sdl3::mouse::Cursor>),
    /// The folders the system's dialog picked.
    picked: (mpsc::Sender<Picked>, mpsc::Receiver<Picked>),
}

impl Window {
    fn open() -> Result<Self> {
        use sdl3::event::Event;
        let sdl = sdl3::init().context("initialising SDL")?;
        let video = sdl.video().context("initialising SDL video")?;
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
        Ok(Self { sdl, video, screen, ctx, painter, input: sdl_egui::Input::default(), woken, wake, pads, events, cursor: (egui::CursorIcon::Default, None), picked: mpsc::channel() })
    }
}

/// Draws the upgrade's page of the game in `dir` into a PNG; nothing is
/// changed.
fn shot_upgrade<G: Game>(dir: &Path, opts: &ShotOptions) -> Result<()> {
    let mut page = wizard::Wizard::new(dir, false);
    if opts.upgraded {
        page.upgraded_dry();
    }
    let mut canvas = crate::headless::Canvas::new(opts.width, opts.height, false)?;
    let ctx = egui::Context::default();
    crate::gui::style(&ctx);
    let mut painter = Painter::new(canvas.gpu().0, wgpu::TextureFormat::Rgba8Unorm, false);
    let mut input = sdl_egui::Input::default();
    for frame in 0..120 {
        let mut raw = input.take([opts.width, opts.height], 1.0, 1.0);
        raw.time = Some(frame as f64 / 60.0);
        let out = ctx.run_ui(raw, |ui| {
            page.ui(ui, crate::replay::refit_saying::<G>);
        });
        let settled = frame >= 2 && out.viewport_output.get(&egui::ViewportId::ROOT).is_none_or(|v| v.repaint_delay == Duration::MAX);
        let (device, queue, view) = canvas.gpu();
        painter.paint(device, queue, view, &ctx, out, [opts.width, opts.height]);
        if settled {
            break;
        }
    }
    let encoder = canvas.gpu().0.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let pixels = canvas.read(encoder)?;
    crate::headless::write_png(&opts.out, opts.width, opts.height, &pixels)?;
    println!("{}: the upgrade's page of {}", opts.out.display(), dir.display());
    Ok(())
}

/// Draws the start page into a PNG.
fn shot_start(opts: &ShotOptions) -> Result<()> {
    // A picture looks for a new version only at a feed named for it.
    let mut page = projects::StartPage::new(prefs::Prefs::load(&prefs::Prefs::path()).unwrap_or_default(), None, std::env::var_os("VOXCADE_UPDATE_FEED").is_some());
    // `--open new:NAME`: NEW GAME's form with the name typed.
    if let Some(name) = opts.open.as_deref().and_then(|o| o.strip_prefix("new:")) {
        page.show_new(name);
    }
    let mut canvas = crate::headless::Canvas::new(opts.width, opts.height, false)?;
    let ctx = egui::Context::default();
    crate::gui::style(&ctx);
    let mut painter = Painter::new(canvas.gpu().0, wgpu::TextureFormat::Rgba8Unorm, false);
    let mut input = sdl_egui::Input::default();
    for frame in 0..120 {
        let mut raw = input.take([opts.width, opts.height], 1.0, 1.0);
        raw.time = Some(frame as f64 / 60.0);
        let out = ctx.run_ui(raw, |ui| {
            page.ui(ui);
        });
        let settled = frame >= 2 && out.viewport_output.get(&egui::ViewportId::ROOT).is_none_or(|v| v.repaint_delay == Duration::MAX);
        let (device, queue, view) = canvas.gpu();
        painter.paint(device, queue, view, &ctx, out, [opts.width, opts.height]);
        if settled {
            break;
        }
    }
    let encoder = canvas.gpu().0.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let pixels = canvas.read(encoder)?;
    crate::headless::write_png(&opts.out, opts.width, opts.height, &pixels)?;
    println!("{}: the start page", opts.out.display());
    Ok(())
}

/// Opens the IDE in a window on the game in `first` (or its start page)
/// until it is closed; another game opens in the same window. With
/// `remote` it answers the console's tools (`console_send`,
/// `console_read`) for another session testing it.
pub fn run<G: Game>(first: Option<&Path>, remote: bool) -> Result<()> {
    // Another session's keys arrive while the person works elsewhere.
    crate::gui::input::keep_focus(remote);
    let mut window = Window::open()?;
    let mut next = match first {
        Some(dir) => Next::Game(dir.to_path_buf()),
        None => Next::Start(None),
    };
    loop {
        next = match next {
            // A game made for another Voxcade: the upgrade's page first.
            Next::Game(dir) if wizard::needed(&dir) => Next::Upgrade(dir, false),
            Next::Game(dir) => match project::<G>(&mut window, &dir, remote) {
                Ok(next) => next,
                // A game that would not open: the start page says why.
                Err(e) => Next::Start(Some(format!("{}: {e:#}", dir.display()))),
            },
            Next::Upgrade(dir, play) => upgrade_page(&mut window, wizard::Wizard::new(&dir, play), crate::replay::refit_saying::<G>)?,
            Next::Start(message) => start_page(&mut window, message, None)?,
            Next::Play(dir) => start_page(&mut window, None, Some(dir))?,
            Next::Quit => {
                // SDL not quit: its audio may hang in a sound server gone
                // away, and the window's end must not wait for it (the
                // program's end takes everything).
                std::mem::forget(window.sdl);
                return Ok(());
            }
        };
    }
}

/// The upgrade's page (`wizard`) in the IDE's window until the game opens,
/// the start page is wanted again or the window is closed.
fn upgrade_page(window: &mut Window, mut page: wizard::Wizard, refit: wizard::Refitter) -> Result<Next> {
    use sdl3::event::{Event, WindowEvent};
    let Window { video, screen, ctx, painter, input, woken, events, cursor, .. } = window;
    let _ = screen.window.set_title("Voxcade: bringing a game up to date");
    fresh(ctx);
    let mut repaint_at = Some(Instant::now());
    loop {
        let timeout = repaint_at.map_or(Duration::from_secs(3600), |r| r.saturating_duration_since(Instant::now()));
        let first = if timeout.is_zero() { events.poll_event() } else { events.wait_event_timeout(timeout.max(Duration::from_millis(1))) };
        woken.store(false, Ordering::Release);
        let clipboard = video.clipboard();
        for event in first.into_iter().chain(events.poll_iter().collect::<Vec<_>>()) {
            match &event {
                // (Closed while it works: the thread ends with the program.)
                Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return Ok(Next::Quit),
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => screen.resize_surface(*w, *h),
                _ => {}
            }
            input.handle(&event, || clipboard.clipboard_text().ok().filter(|t| !t.is_empty()));
        }
        let size = [screen.config.width, screen.config.height];
        let pixels_per_point = screen.window.display_scale();
        let mut wish = None;
        let out = ctx.run_ui(input.take(size, pixels_per_point, screen.window.pixel_density()), |ui| wish = page.ui(ui, refit));
        match wish {
            Some(wizard::Wish::Back) => return Ok(Next::Start(None)),
            Some(wizard::Wish::Open) => return Ok(Next::Game(page.dir.clone())),
            Some(wizard::Wish::Play) => return Ok(Next::Play(page.dir.clone())),
            None => {}
        }
        let _ = platform(&out.platform_output, cursor, video, screen, pixels_per_point);
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        repaint_at = Instant::now().checked_add(delay);
        if !present(screen, painter, ctx, out, size, false).0 {
            repaint_at = Some(Instant::now());
        }
    }
}

/// The start page in the IDE's window until a game is chosen or the window
/// closed; with `play`, that game played at once.
fn start_page(window: &mut Window, message: Option<String>, play: Option<PathBuf>) -> Result<Next> {
    use sdl3::event::{Event, WindowEvent};
    let Window { video, screen, ctx, painter, input, woken, wake, events, cursor, picked, .. } = window;
    let _ = screen.window.set_title("Voxcade");
    fresh(ctx);
    let (prefs, trouble) = match prefs::Prefs::load(&prefs::Prefs::path()) {
        Ok(prefs) => (prefs, None),
        Err(e) => (prefs::Prefs::default(), Some(format!("The settings: {e:#}"))),
    };
    let mut page = projects::StartPage::new(prefs, message.or(trouble), true);
    if let Some(dir) = play {
        page.play(&dir, &projects::title_of(&dir));
    }
    let mut repaint_at = Some(Instant::now());
    loop {
        let timeout = repaint_at.map_or(Duration::from_secs(3600), |r| r.saturating_duration_since(Instant::now()));
        let first = if timeout.is_zero() { events.poll_event() } else { events.wait_event_timeout(timeout.max(Duration::from_millis(1))) };
        woken.store(false, Ordering::Release);
        let clipboard = video.clipboard();
        for event in first.into_iter().chain(events.poll_iter().collect::<Vec<_>>()) {
            match &event {
                Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => return Ok(Next::Quit),
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => screen.resize_surface(*w, *h),
                _ => {}
            }
            input.handle(&event, || clipboard.clipboard_text().ok().filter(|t| !t.is_empty()));
        }
        while let Ok(folder) = picked.1.try_recv() {
            match folder {
                Picked::Game(dir) if projects::is_game(&dir) => return Ok(Next::Game(dir)),
                Picked::Game(dir) => page.tell(no_game(&dir)),
                Picked::Projects(dir) => page.set_projects(dir),
            }
        }
        let size = [screen.config.width, screen.config.height];
        let pixels_per_point = screen.window.display_scale();
        let mut wish = None;
        let out = ctx.run_ui(input.take(size, pixels_per_point, screen.window.pixel_density()), |ui| wish = page.ui(ui));
        match wish {
            Some(projects::Wish::Open(dir)) => return Ok(Next::Game(dir)),
            Some(projects::Wish::Browse) => browse(&picked.0, wake, &screen.window, page.prefs.projects_dir(), Picked::Game),
            Some(projects::Wish::BrowseProjects) => browse(&picked.0, wake, &screen.window, page.prefs.projects_dir(), Picked::Projects),
            Some(projects::Wish::Upgrade(dir, play)) => return Ok(Next::Upgrade(dir, play)),
            None => {}
        }
        if let Some(trouble) = platform(&out.platform_output, cursor, video, screen, pixels_per_point) {
            page.tell(trouble);
        }
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        repaint_at = Instant::now().checked_add(delay);
        if !present(screen, painter, ctx, out, size, false).0 {
            repaint_at = Some(Instant::now());
        }
    }
}

/// egui with nothing remembered of widgets, in the IDE's look (its
/// textures stay its own: the old game's pictures are freed with the next
/// frame).
fn fresh(ctx: &mut egui::Context) {
    ctx.memory_mut(|m| *m = egui::Memory::default());
    crate::gui::style(ctx);
}

/// What the IDE says of a folder without a game.
fn no_game(dir: &Path) -> String {
    format!("No game in {}: a game's folder has its conf.lua.", dir.display())
}

/// The system's folder dialog over `window`, starting `at`; the folder
/// picked goes to `send` as `what` makes of it, the window woken.
fn browse(send: &mpsc::Sender<Picked>, wake: &Arc<dyn Fn() + Send + Sync>, window: &sdl3::video::Window, at: PathBuf, what: fn(PathBuf) -> Picked) {
    let (send, wake) = (send.clone(), wake.clone());
    let at = at.is_dir().then_some(at);
    sdl3::dialog::show_open_folder_dialog(
        at,
        false,
        window,
        Box::new(move |result, _| {
            if let Some(dir) = result.ok().and_then(|dirs| dirs.into_iter().next()) {
                let _ = send.send(what(dir));
                wake();
            }
        }),
    );
}

/// What egui wants of the system after a frame: the mouse's look, the
/// clipboard (what went wrong copying), where typed text goes.
fn platform(platform: &egui::PlatformOutput, cursor: &mut (egui::CursorIcon, Option<sdl3::mouse::Cursor>), video: &sdl3::VideoSubsystem, screen: &crate::window::Screen, pixels_per_point: f32) -> Option<String> {
    if platform.cursor_icon != cursor.0 {
        cursor.1 = sdl3::mouse::Cursor::from_system(sdl_egui::cursor(platform.cursor_icon)).ok();
        if let Some(c) = &cursor.1 {
            c.set();
        }
        cursor.0 = platform.cursor_icon;
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
        let r = ime.cursor_rect * (pixels_per_point / screen.window.pixel_density());
        video.text_input().set_rect(&screen.window, sdl3::rect::Rect::new(r.min.x as i32, r.min.y as i32, r.width().max(1.0) as u32, r.height().max(1.0) as u32), 0);
    }
    trouble
}

/// Paints egui's frame `out` onto the window; false when the window's
/// surface had to be set up anew (another frame is due). With `capture`
/// the frame is also read back, as PNG bytes, before it is shown (none if
/// the window was not drawn).
fn present(screen: &mut crate::window::Screen, painter: &mut Painter, ctx: &egui::Context, out: egui::FullOutput, size: [u32; 2], capture: bool) -> (bool, Option<Result<Vec<u8>, String>>) {
    match screen.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
            let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
            painter.paint(&screen.device, &screen.queue, &view, ctx, out, size);
            let picture = capture.then(|| screen.picture(&texture.texture).map_err(|e| format!("{e:#}")));
            screen.queue.present(texture);
            (true, picture)
        }
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            screen.surface.configure(&screen.device, &screen.config);
            (false, None)
        }
        _ => (true, None),
    }
}

/// The game in `dir` in the IDE's window until it is closed or another is
/// opened (the menu on its name); what the window shows next.
fn project<G: Game>(window: &mut Window, dir: &Path, remote: bool) -> Result<Next> {
    use sdl3::event::{Event, WindowEvent};

    let cfg = crate::session::config::<G>(dir)?;
    let title = cfg.title.clone();
    let Window { sdl, video, screen, ctx, painter, input, woken, wake, pads, events, cursor, picked } = window;
    let _ = screen.window.set_title(&format!("{title} - Voxcade"));
    // egui afresh: what it keeps of widgets (a text's cursor by its path)
    // belongs to the game before.
    fresh(ctx);
    // The bridge for `voxcade mcp` (the agent), woken the same way.
    let bridge = bridge::Bridge::open(&bridge::lock_path(dir), dir, wake.clone()).map_err(|e| eprintln!("no MCP bridge: {e:#}")).ok();
    let console = Console::new(dir.to_path_buf(), Start::Shell, wake.clone());
    let brought = G::sounds(&cfg);
    let mut shell = Shell::new(dir, cfg, memory::Memory::path_for(dir), console).with_sounds(brought).with_prefs(prefs::Prefs::path()).with_output(output::Output::path_for(dir));
    // The editors' windows, on the IDE's GPU.
    let mut editors: Vec<(Editing, crate::editor::EditorWindow)> = Vec::new();
    // The game being played in the main area, and whether the window has
    // the focus (elsewhere the game holds still, unless the IDE answers
    // another session, `remote`).
    let mut play: Option<Play<G>> = None;
    let mut focused = true;
    // Another session's hands (`--remote`): its pointer, keys and pictures
    // of the windows.
    let mut hands = remote::Remote::new(sdl.event().context("SDL events")?.event_sender());
    // A song played from its file (PLAY over a .mml), through a speaker of
    // its own while it plays.
    let mut song: Option<crate::audio::Speaker> = None;
    // A sound played from its text (PLAY SOUND): its speaker stays open
    // once opened, so the next sound comes at once.
    let mut blip: Option<crate::audio::Speaker> = None;
    // When egui wants to draw again (None: only after an event), and when
    // the open text is due to be saved or looked at on disk.
    let mut repaint_at = Some(Instant::now());
    let mut tick_at = Instant::now();
    // While the game would not start: its files watched, so the red line
    // goes once they are mended.
    let mut start_watch: Option<crate::watch::FolderWatch> = None;
    let next = 'project: loop {
        // (The editors' panels may want to be drawn again too.)
        // (And a pad plugged in opens once it has settled.)
        pads.settle();
        let until = editors.iter().filter_map(|(_, e)| e.wakes_at()).chain(repaint_at).chain(pads.wakes_at()).chain(hands.wakes_at(Instant::now())).fold(tick_at, Instant::min);
        let timeout = until.saturating_duration_since(Instant::now());
        let first = if timeout.is_zero() { events.poll_event() } else { events.wait_event_timeout(timeout.max(Duration::from_millis(1))) };
        woken.store(false, Ordering::Release);
        let clipboard = video.clipboard();
        // Whether something happened that the IDE's window shows.
        let mut happened = false;
        for event in first.into_iter().chain(events.poll_iter().collect::<Vec<_>>()) {
            // A real mouse takes the synthetic pointer's mark away.
            hands.saw(&event);
            // An editor's window takes its own events; closed, it goes.
            if let Route::Editor(id) = route(&event, screen.window.id()) {
                let shift = crate::editor::shift_held(events) || hands.shift();
                if let Some(i) = editors.iter().position(|(_, e)| e.id() == id)
                    && !editors[i].1.handle(&event, shift)
                {
                    editors.remove(i);
                }
                continue;
            }
            // Pads plugged in and out; their buttons and sticks are read
            // while the game runs (drawn every frame then), so they draw
            // nothing themselves (a drifting stick would redraw the IDE
            // all the time).
            if event.is_gamepad() || event.is_joy() {
                match &event {
                    Event::GamepadAdded { which, .. } => pads.added(*which),
                    Event::GamepadRemoved { which, .. } => pads.removed(*which),
                    // The run over (PRESS START): from the start, as F5;
                    // else a pad's START takes a seat (a game of several).
                    Event::GamepadButtonDown { button: sdl3::gamepad::Button::Start | sdl3::gamepad::Button::South, which, .. } if shell.stage.as_ref().is_some_and(|s| s.shown) && focused => {
                        match play.as_mut() {
                            Some(p) if p.over() => {
                                if let Err(e) = p.restart(&screen.device, &screen.queue) {
                                    shell.tell(format!("{e:#}"));
                                }
                            }
                            Some(p) if matches!(event, Event::GamepadButtonDown { button: sdl3::gamepad::Button::Start, .. }) => {
                                p.pad_start(which.raw(), &pads.ids());
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                continue;
            }
            happened = true;
            match &event {
                // Closing the IDE's window ends the IDE, its editors too
                // (the text being changed saved first, the run being
                // played ended: kept if long enough).
                Event::Quit { .. } | Event::Window { win_event: WindowEvent::CloseRequested, .. } => break 'project Next::Quit,
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => screen.resize_surface(*w, *h),
                // Back in the IDE: the open file afresh (an editor may have
                // changed it).
                Event::Window { win_event: WindowEvent::FocusGained, .. } => {
                    focused = true;
                    shell.refresh();
                }
                Event::Window { win_event: WindowEvent::FocusLost, .. } => focused = false,
                // The game's keys: Escape back to the file, F5 from the
                // start, F9 keeps the run in the game's replays.
                Event::KeyDown { keycode: Some(sdl3::keyboard::Keycode::Escape), repeat: false, .. } if shell.keys_to_game(ctx) => {
                    shell.back_to_file();
                    continue;
                }
                // The keyboard's right half takes a seat (a game of several).
                Event::KeyDown { keycode: Some(sdl3::keyboard::Keycode::Return), repeat: false, .. } if shell.keys_to_game(ctx) && play.as_ref().is_some_and(|p| !p.over()) => {
                    if let Some(p) = &mut play {
                        p.keyboard_start();
                    }
                }
                // The run over (PRESS START): Enter starts it over too.
                Event::KeyDown { keycode: Some(sdl3::keyboard::Keycode::Return | sdl3::keyboard::Keycode::KpEnter), repeat: false, .. }
                    if shell.keys_to_game(ctx) && play.as_ref().is_some_and(Play::over) =>
                {
                    if let Some(p) = &mut play
                        && let Err(e) = p.restart(&screen.device, &screen.queue)
                    {
                        shell.tell(format!("{e:#}"));
                    }
                    continue;
                }
                Event::KeyDown { keycode: Some(sdl3::keyboard::Keycode::F5), repeat: false, .. } if shell.keys_to_game(ctx) => {
                    if let Some(p) = &mut play
                        && let Err(e) = p.restart(&screen.device, &screen.queue)
                    {
                        shell.tell(format!("{e:#}"));
                    }
                    continue;
                }
                Event::KeyDown { keycode: Some(sdl3::keyboard::Keycode::F9), repeat: false, .. } if shell.keys_to_game(ctx) => {
                    if let Some(p) = &play {
                        shell.note(match p.keep() {
                            Ok(path) => format!("KEPT THE RUN AS {}", path.strip_prefix(dir).unwrap_or(&path).display()),
                            Err(e) => format!("NO RUN KEPT: {e:#}"),
                        });
                    }
                    continue;
                }
                _ => {}
            }
            // (No text on the clipboard: Ctrl+V goes on to the console,
            // where Claude Code pastes a picture.)
            input.handle(&event, || match clipboard.clipboard_text() {
                Ok(text) => Some(text).filter(|t| !t.is_empty()),
                Err(e) => {
                    shell.note(format!("PASTING FAILED: {e}"));
                    None
                }
            });
        }
        for (_, editor) in &mut editors {
            if let Err(e) = editor.draw() {
                shell.tell(format!("{e:#}"));
            }
        }
        // A folder the system's dialog picked: a game to open, or where NEW
        // puts games.
        while let Ok(folder) = picked.1.try_recv() {
            happened = true;
            match folder {
                Picked::Game(to) if projects::is_game(&to) => shell.leave_for(to),
                Picked::Game(to) => shell.tell(no_game(&to)),
                Picked::Projects(to) => shell.set_projects(to),
            }
        }
        // What the agent asks through the bridge.
        let calls = bridge.as_ref().map(bridge::Bridge::calls).unwrap_or_default();
        let windows = if remote && (!calls.is_empty() || hands.busy()) { window_infos(screen, &editors) } else { Vec::new() };
        for call in calls {
            happened = true;
            answer(call, &mut shell, ctx, play.as_mut(), screen, remote, (&mut hands, &windows));
        }
        // The pointer, keys and pictures another session asked for: the
        // next step of its call, and the marks and pictures asked of the
        // windows.
        let capture = if hands.busy() {
            let now = Instant::now();
            hands.run(now, &windows);
            happened = true;
            hands.capturing(now)
        } else {
            None
        };
        for (_, editor) in &mut editors {
            editor.set_marker(hands.marker(editor.id()));
            if let Some((_, late)) = capture.filter(|c| c.0 == editor.id()) {
                editor.want_picture(late);
            }
        }
        // The text being changed saved when due, the disk looked at.
        let (changed, next) = shell.tick(Instant::now());
        tick_at = Instant::now() + next;
        // A song that plays once ends by itself: looked for twice a second.
        if song.is_some() {
            if song.as_mut().and_then(|s| s.lock()).is_none_or(|m| m.music_over()) {
                drop(song.take());
                shell.listening = None;
                happened = true;
            } else {
                tick_at = tick_at.min(Instant::now() + Duration::from_millis(500));
            }
        }
        // A game stopped by an error and held takes in what is saved
        // meanwhile, looked for a few times a second: the line marked goes
        // once the file is mended.
        if let (Some(p), Some(stage)) = (&mut play, &mut shell.stage)
            && (stage.trouble.is_some() || stage.unloaded.is_some())
            && !(stage.shown && (focused || remote))
        {
            if p.reload(&screen.device, &screen.queue) {
                stage.trouble = p.trouble();
                stage.unloaded = p.unloaded();
                happened = true;
            }
            tick_at = tick_at.min(Instant::now() + Duration::from_millis(250));
        }
        // A game that would not start, its files changed: begun in the
        // background to see (no window); the red line goes if it would.
        if !shell.start_failed() {
            start_watch = None;
        }
        if let Some(watch) = &mut start_watch {
            if !watch.poll().is_empty() && play::begins::<G>(dir).is_ok() {
                shell.start_mended();
                start_watch = None;
                happened = true;
            }
            tick_at = tick_at.min(Instant::now() + Duration::from_millis(250));
        }
        // Nothing new for the IDE's window: no frame.
        if !(happened || changed || repaint_at.is_some_and(|r| Instant::now() >= r)) {
            continue;
        }
        let size = [screen.config.width, screen.config.height];
        let pixels_per_point = screen.window.display_scale();
        let raw = input.take(size, pixels_per_point, screen.window.pixel_density());
        let marker = hands.marker(screen.window.id()).map(|(x, y)| input.point(x, y));
        let out = ctx.run_ui(raw, |ui| {
            shell.ui(ui);
            if let Some(tip) = marker {
                crate::gui::marker::paint(ui.ctx(), tip);
            }
        });
        // The open file's picture, drawn anew if it changed (shown from the
        // next frame on when it is new).
        let pictured = shell.render(&screen.device, &screen.queue, &mut painter.renderer);

        // The game: started (saved first, by the shell), ended; while it
        // shows and the window has the focus it runs, drawn every frame,
        // its keys read as the player bound them; else it holds still.
        for ask in shell.take_asks() {
            match ask {
                Ask::Play(scenario) => match Play::<G>::start(dir, &screen.device, &screen.queue, crate::audio::speaker(sdl), scenario.as_deref()) {
                    Ok(p) => {
                        shell.stage = Some(Stage { shown: true, keys: p.keys(), ..Stage::default() });
                        play = Some(p);
                        shell.start_mended();
                        start_watch = None;
                    }
                    Err(e) => {
                        shell.wont_start(format!("{e:#}"));
                        start_watch = Some(crate::watch::FolderWatch::new(dir));
                    }
                },
                Ask::Stop => {
                    if let Some(mut p) = play.take() {
                        let said = p.said();
                        shell.game_said(None, said);
                        p.end(&mut painter.renderer);
                    }
                    shell.stage = None;
                }
                Ask::Again => {
                    if let Some(p) = &mut play
                        && let Err(e) = p.restart(&screen.device, &screen.queue)
                    {
                        shell.tell(format!("{e:#}"));
                    }
                }
                Ask::Eval(line) => match play.as_mut() {
                    Some(p) => {
                        let said = p.eval(&line);
                        shell.evaluated(&line, &said);
                    }
                    None => shell.tell("No game plays: PLAY first, then its lines run in it.".into()),
                },
                Ask::Listen(path) => match shell.song(&path) {
                    Ok(player) => {
                        if song.is_none() {
                            song = crate::audio::speaker(sdl);
                        }
                        match song.as_mut().and_then(|s| s.lock()) {
                            Some(mut mixer) => {
                                mixer.set_drive(shell.drive());
                                mixer.set_music(Some(player));
                            }
                            None => {
                                shell.listening = None;
                                shell.tell("No sound: the speaker would not open.".into());
                            }
                        }
                    }
                    Err(e) => {
                        shell.listening = None;
                        shell.tell(format!("{e:#}"));
                    }
                },
                // The speaker closed with it.
                Ask::Hush => drop(song.take()),
                Ask::Sound => {
                    if let Some((samples, volume)) = shell.heard() {
                        if blip.is_none() {
                            blip = crate::audio::speaker(sdl);
                        }
                        match blip.as_mut().and_then(|s| s.lock()) {
                            Some(mut mixer) => {
                                mixer.set_drive(shell.drive());
                                mixer.play("heard", &samples, volume, 0.0);
                            }
                            None => shell.tell("No sound: the speaker would not open.".into()),
                        }
                    }
                }
                // Another game (from the menu on the name), the dialog to
                // pick one, or where NEW puts games.
                Ask::Project(projects::Wish::Open(to)) => break 'project Next::Game(to),
                Ask::Project(projects::Wish::Upgrade(to, play)) => break 'project Next::Upgrade(to, play),
                Ask::Project(projects::Wish::Browse) => browse(&picked.0, wake, &screen.window, shell.projects_dir(), Picked::Game),
                Ask::Project(projects::Wish::BrowseProjects) => browse(&picked.0, wake, &screen.window, shell.projects_dir(), Picked::Projects),
            }
        }
        let keys = shell.keys_to_game(ctx);
        let mut running = false;
        if let (Some(p), Some(stage)) = (&mut play, &mut shell.stage) {
            // (For another session testing it, `--remote`, it runs on with
            // the window elsewhere: what it fetches is the game as it goes.)
            match stage.wanted.filter(|_| stage.shown && (focused || remote)) {
                Some(size) => {
                    // The pads always, the keyboard while the game has it.
                    let state = events.keyboard_state();
                    // (And the keys another session holds down for it.)
                    let extra = hands.held(Instant::now());
                    let with = crate::controls::WithHeld(&state, &extra);
                    let held = p.read(keys.then_some(&with), &pads.open);
                    stage.picture = Some(p.frame(&screen.device, &screen.queue, &mut painter.renderer, size, &held));
                    stage.trouble = p.trouble();
                    stage.unloaded = p.unloaded();
                    stage.keys = p.keys();
                    running = true;
                }
                None => p.hold(),
            }
        }

        // What the game said: into OUTPUT.
        if let Some(p) = &mut play {
            let said = p.said();
            shell.game_said(p.trouble(), said);
        }
        // What egui wants of the system: the cursor, the clipboard, where
        // typed text goes.
        if let Some(trouble) = platform(&out.platform_output, cursor, video, screen, pixels_per_point) {
            shell.note(trouble);
        }
        // The console and the system's selection (Linux's primary): what
        // the mouse selects goes there, a middle click pastes it.
        if let Some(text) = shell.console_mut().take_selected()
            && let Err(e) = clipboard.set_primary_selection_text(&text)
        {
            shell.note(format!("SELECTING FAILED: {e}"));
        }
        if shell.console_mut().wants_selection()
            && let Ok(text) = clipboard.primary_selection_text()
        {
            shell.console_mut().paste(&text);
        }
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        repaint_at = if pictured || running { Some(Instant::now()) } else { Instant::now().checked_add(delay) };

        // Files asked to be edited: into the window that has them (brought
        // to the front), else a new one.
        for path in shell.take_edits() {
            // A block opens in a level's editor, at the block: one open
            // already, else the level the config names.
            let block = path.strip_prefix("blocks/").and_then(|p| p.strip_suffix(".vvx").or_else(|| p.strip_suffix(".vox")));
            let open_level = editors.iter().find_map(|(w, _)| if let Editing::Level(file) = w { Some(file.clone()) } else { None });
            let what = match () {
                _ if block.is_some() => match open_level.or_else(|| shell.first_level().map(str::to_string)) {
                    Some(file) => Editing::Level(file),
                    None => continue,
                },
                _ if !shell::is_model_file(&path) => Editing::Level(path.clone()),
                _ => Editing::Model(path.clone()),
            };
            let found = editors.iter_mut().find(|(w, _)| *w == what);
            let editor = match found {
                Some((_, e)) => e,
                None => match match &what {
                    Editing::Level(file) => crate::editor::EditorWindow::open::<G>(video, dir, Some(file), None, false, Some(screen.gpu())),
                    Editing::Model(file) => crate::editor::EditorWindow::open_model::<G>(video, dir, file, Some(screen.gpu())),
                } {
                    Ok(e) => {
                        editors.push((what, e));
                        &mut editors.last_mut().expect("just pushed").1
                    }
                    Err(e) => {
                        shell.tell(format!("{e:#}"));
                        continue;
                    }
                },
            };
            if let Some(name) = block {
                editor.open_block(name);
            }
            editor.raise();
            repaint_at = Some(Instant::now());
        }
        for (_, editor) in &mut editors {
            if let Err(e) = editor.draw() {
                shell.tell(format!("{e:#}"));
            }
            if let Some(picture) = editor.take_picture() {
                hands.captured(editor.id(), picture);
            }
        }

        // A picture of this window asked for: taken once the frame is
        // settled (egui wants no frame at once), or as it is when it has
        // waited long.
        let take = capture.is_some_and(|(id, late)| id == screen.window.id() && (late || (!delay.is_zero() && !pictured)));
        let (shown, picture) = present(screen, painter, ctx, out, size, take);
        if !shown {
            repaint_at = Some(Instant::now());
        }
        if let Some(picture) = picture {
            hands.captured(screen.window.id(), picture);
        }
    };
    // Leaving the game: the text being changed saved, the run being played
    // ended (kept if long enough), the pictures freed, what the IDE
    // remembers written down (failing, it is said, but the window still
    // closes or goes on); the editors' windows, the console and the bridge
    // go with this function.
    shell.flush();
    if let Some(p) = play.take() {
        p.end(&mut painter.renderer);
    }
    shell.release(&mut painter.renderer);
    if let Err(e) = shell.save() {
        eprintln!("remembering what was open: {e:#}");
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl3::event::Event;

    /// The console's tools are answered only by an IDE started with
    /// `--remote`; the other tools never wait on it.
    #[test]
    fn the_console_tools_need_remote() {
        for tool in ["console_send", "console_read", "windows", "window_shot", "pointer", "scroll", "keys"] {
            let why = refused(tool, false).expect("refused without --remote");
            assert!(why.contains("--remote"), "{why}");
            assert_eq!(refused(tool, true), None);
        }
        for tool in ["view_state", "open", "play", "eval", "errors"] {
            assert_eq!(refused(tool, false), None);
        }
    }

    /// An editor window's events go to it, the IDE's own and the
    /// program's (quit, the console's wake) to the IDE.
    #[test]
    fn an_editor_window_takes_only_its_own_events() {
        let key = |window_id| Event::KeyDown { timestamp: 0, window_id, keycode: None, scancode: None, keymod: sdl3::keyboard::Mod::empty(), repeat: false, which: 0, raw: 0 };
        let wake = Event::User { timestamp: 0, window_id: 0, type_: 0x8000, code: 0, data1: std::ptr::null_mut(), data2: std::ptr::null_mut() };
        assert_eq!(route(&key(7), 3), Route::Editor(7));
        assert_eq!(route(&key(3), 3), Route::Ide);
        assert_eq!(route(&Event::Quit { timestamp: 0 }, 3), Route::Ide);
        assert_eq!(route(&wake, 3), Route::Ide);
    }
}

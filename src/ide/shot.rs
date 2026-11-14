//! `voxcade ide --shot`: the IDE drawn without a window into a PNG, as it
//! opens on a game (or with the game played), on a game's upgrade page or
//! on the start page.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::game::Game;
use crate::gui::{Painter, input as sdl_egui};
use super::console::{Console, Start};
use super::play::Play;
use super::shell::{Shell, Stage};
use super::{buffer, help, prefs, projects, wizard};

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
    /// A text left in the open file's journal before the IDE opens it, as
    /// an older IDE leaves one (saying nothing of the file it was changed
    /// from): unlike the file, it is asked about (CHANGED OUTSIDE TOO).
    pub journal: Option<PathBuf>,
    /// The find row over the open text for this word, and its replace
    /// field with that text (`--find`, `--replace`).
    pub find: Option<String>,
    pub replace: Option<String>,
    /// The search across the game for this word, and its n-th line opened
    /// as a click does (`--search`, `--hit`).
    pub search: Option<String>,
    pub hit: Option<usize>,
    pub width: u32,
    pub height: u32,
    pub out: PathBuf,
}

/// What a picture without a window shows, frame by frame.
trait Page {
    /// Before the first frame, with egui's context in the IDE's look.
    fn begin(&mut self, _ctx: &egui::Context) {}
    /// egui's part of a frame.
    fn ui(&mut self, ui: &mut egui::Ui);
    /// The GPU's part, after egui's (`frame` its number, from 0): whether
    /// it is still at work, so the picture is not settled yet.
    fn gpu(&mut self, _frame: u32, _device: &wgpu::Device, _queue: &wgpu::Queue, _painter: &mut Painter) -> bool {
        false
    }
    /// The frame painted.
    fn painted(&mut self, _frame: u32) {}
}

/// A page that is egui alone.
struct Plain<F>(F);

impl<F: FnMut(&mut egui::Ui)> Page for Plain<F> {
    fn ui(&mut self, ui: &mut egui::Ui) {
        (self.0)(ui);
    }
}

/// Draws `page` into the PNG at `opts.out`: frames a sixtieth of a second
/// apart until egui has laid out its first ones anew (fonts, sizes) and
/// its animations (scroll bars fading in) have ended, two seconds at most.
fn headless(opts: &ShotOptions, page: &mut dyn Page) -> Result<()> {
    let mut canvas = crate::headless::Canvas::new(opts.width, opts.height, false)?;
    let ctx = egui::Context::default();
    crate::gui::style(&ctx);
    page.begin(&ctx);
    let mut painter = Painter::new(canvas.gpu().0, wgpu::TextureFormat::Rgba8Unorm, false);
    let mut input = sdl_egui::Input::default();
    for frame in 0..120 {
        let mut raw = input.take([opts.width, opts.height], 1.0, 1.0);
        raw.time = Some(frame as f64 / 60.0);
        let out = ctx.run_ui(raw, |ui| page.ui(ui));
        let mut settled = frame >= 2 && out.viewport_output.get(&egui::ViewportId::ROOT).is_none_or(|v| v.repaint_delay == Duration::MAX);
        let (device, queue, view) = canvas.gpu();
        settled &= !page.gpu(frame, device, queue, &mut painter);
        painter.paint(device, queue, view, &ctx, out, [opts.width, opts.height]);
        page.painted(frame);
        if settled {
            break;
        }
    }
    let encoder = canvas.gpu().0.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let pixels = canvas.read(encoder)?;
    crate::headless::write_png(&opts.out, opts.width, opts.height, &pixels)
}

/// Draws the IDE as it opens into a PNG (without a game: its start page,
/// with the games opened last as the settings have them).
pub fn shot<G: Game>(dir: Option<&Path>, opts: &ShotOptions) -> Result<()> {
    let Some(dir) = dir else { return shot_start(opts, None, None) };
    // A folder that is no game: the only game below it, or the start page.
    let only;
    let dir = match projects::opening(dir) {
        projects::Opening::Here => dir,
        projects::Opening::Only(game) => {
            println!("{}: the only game in {}", game.display(), dir.display());
            only = game;
            &only
        }
        projects::Opening::Several(games) => return shot_start(opts, None, Some((dir.to_path_buf(), games))),
        projects::Opening::Nothing => return shot_start(opts, Some(super::no_game(dir)), None),
    };
    // A game made for another Voxcade: the upgrade's page, as it opens or
    // (`--upgraded`) as after UPGRADE run dry.
    if wizard::needed(dir) {
        return shot_upgrade::<G>(dir, opts);
    }
    let start = opts.console.clone().map_or(Start::Nothing, Start::Command);
    let console = Console::new(dir.to_path_buf(), start, Arc::new(|| {}));
    let cfg = crate::session::config::<G>(dir)?;
    let brought = G::sounds(&cfg);
    // What the shell remembers goes in a folder that is gone with the shot.
    let scratch = crate::scratch::Scratch::new("ide-shot");
    if let (Some(text), Some(path)) = (&opts.journal, &opts.open) {
        let left = std::fs::read_to_string(text).with_context(|| format!("--journal {}", text.display()))?;
        buffer::leave(dir, path, &scratch.join("memory.journal"), &left)?;
    }
    let mut shell = Shell::new(dir, cfg, scratch.join("memory.toml"), console).with_sounds(brought);
    shell.pictures.animate = false;
    // (Not the person's editor background: shots look the same anywhere.)
    shell.pictures.own_background = false;
    // A command run in the console is meant to be seen: unfolded (else a
    // shot shows a game's console as a game opens, folded).
    if opts.console.is_some() {
        shell.unfold_console();
    }
    open_as_asked(&mut shell, opts.open.as_deref());
    let mut page = GamePage::<G> { dir, opts, shell, play: None };
    headless(opts, &mut page)?;
    println!("{}: {} open", opts.out.display(), page.shell.showing());
    Ok(())
}

/// What `--open` asks shown: `help` HELP as it opens, at every name of
/// the API (`help:N` the example N, from 1); `text:FILE` a model as its
/// text; anything else a file.
fn open_as_asked(shell: &mut Shell, open: Option<&str>) {
    match open.and_then(|o| o.strip_prefix("help")) {
        Some(topic) => {
            shell.help = true;
            shell.help_topic = topic.strip_prefix(':').and_then(|n| n.parse::<usize>().ok()).map_or(0, |n| n.min(help::TOPICS.len()));
        }
        None => match open.map(|o| (o, o.strip_prefix("text:"))) {
            Some((_, Some(model))) => {
                shell.open(model);
                shell.model_as_text();
            }
            Some((path, None)) => shell.open(path),
            None => {}
        },
    }
}

/// The IDE on a game, its game played if asked.
struct GamePage<'a, G: Game> {
    dir: &'a Path,
    opts: &'a ShotOptions,
    shell: Shell,
    play: Option<Play<G>>,
}

impl<G: Game> Page for GamePage<'_, G> {
    fn begin(&mut self, ctx: &egui::Context) {
        if let Some(word) = &self.opts.search {
            self.shell.show_search(word, ctx, self.opts.hit);
        }
        if let Some(word) = &self.opts.find {
            self.shell.show_find(word, self.opts.replace.as_deref());
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        self.shell.ui(ui);
    }

    /// The game played, its ticks stepped at once, drawn every frame (a
    /// file opened shows instead, the game held behind it); the open
    /// file's picture drawn anew shows from the next frame on.
    fn gpu(&mut self, frame: u32, device: &wgpu::Device, queue: &wgpu::Queue, painter: &mut Painter) -> bool {
        let mut busy = false;
        if let Some(ticks) = self.opts.play {
            // A game that would not start (a script's syntax error) is said
            // in the red line, as PLAY in the window says it.
            if frame == 0 {
                match Play::<G>::start(self.dir, device, queue, None, None) {
                    Ok(mut p) => {
                        p.step(ticks, &crate::input::Input::default());
                        self.shell.stage = Some(Stage { shown: self.opts.open.is_none(), trouble: p.trouble(), unloaded: p.unloaded(), ..Stage::default() });
                        self.play = Some(p);
                    }
                    Err(e) => self.shell.wont_start(format!("{e:#}")),
                }
            }
            if let (Some(p), Some(stage)) = (&mut self.play, &mut self.shell.stage)
                && let Some(size) = stage.wanted
            {
                busy |= stage.picture.is_none();
                stage.picture = Some(p.draw(device, queue, &mut painter.renderer, size));
                stage.keys = p.keys().to_vec();
                stage.trouble = p.trouble();
                stage.unloaded = p.unloaded();
            }
        }
        busy | self.shell.render(device, queue, &mut painter.renderer)
    }

    /// The console's command started on the first frame, at its size: its
    /// output, until it ends.
    fn painted(&mut self, frame: u32) {
        if frame == 0 {
            let until = Instant::now() + Duration::from_secs(10);
            while !self.shell.console().ended() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

/// Draws the upgrade's page of the game in `dir` into a PNG; nothing is
/// changed.
fn shot_upgrade<G: Game>(dir: &Path, opts: &ShotOptions) -> Result<()> {
    let mut page = wizard::Wizard::new(dir, false);
    if opts.upgraded {
        page.upgraded_dry();
    }
    headless(
        opts,
        &mut Plain(|ui: &mut egui::Ui| {
            page.ui(ui, crate::replay::refit_saying::<G>);
        }),
    )?;
    println!("{}: the upgrade's page of {}", opts.out.display(), dir.display());
    Ok(())
}

/// Draws the start page into a PNG.
fn shot_start(opts: &ShotOptions, message: Option<String>, found: Option<(PathBuf, Vec<PathBuf>)>) -> Result<()> {
    // A picture looks for a new version only at a feed named for it.
    let mut page = projects::StartPage::new(prefs::Prefs::load(&prefs::Prefs::path()).unwrap_or_default(), message, std::env::var_os("VOXCADE_UPDATE_FEED").is_some());
    if let Some((folder, games)) = found {
        page.show_found(folder, games);
    }
    // `--open new:NAME`: NEW GAME's form with the name typed.
    if let Some(name) = opts.open.as_deref().and_then(|o| o.strip_prefix("new:")) {
        page.show_new(name);
    }
    // `--open edit:FOLDER`: EDIT pressed on a game that came with the
    // program, its card asking MAKE A COPY or EDIT HERE.
    if let Some(name) = opts.open.as_deref().and_then(|o| o.strip_prefix("edit:")) {
        anyhow::ensure!(page.show_asking(name), "--open edit:{name}: no game of that folder comes with the program");
    }
    headless(
        opts,
        &mut Plain(|ui: &mut egui::Ui| {
            page.ui(ui);
        }),
    )?;
    println!("{}: the start page", opts.out.display());
    Ok(())
}

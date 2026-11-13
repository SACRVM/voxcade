//! `voxcade shot`: run the simulation without a window and save one
//! rendered frame as PNG. This is how an agent (or a test) sees the game.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};

use crate::game::Game;
use crate::controls::{Device, PadStyle};
use crate::session::{Scene, Session};
use crate::menu::{Menu, Page};
use crate::render::{FrameData, Renderer};
use crate::settings::Settings;
use crate::replay::{Playback, Recorder, Replay, Script, SeatScripts, parse_script};

pub struct ShotOptions {
    /// Ticks to run; by default 90, or a replay's length.
    pub frames: Option<u32>,
    /// A replay to play instead of `input`, and a file to record the run to.
    pub replay: Option<PathBuf>,
    pub record: Option<PathBuf>,
    /// A menu or screen to show instead of the game (`SCREENS`).
    pub screen: Option<String>,
    pub input: String,
    /// The seats' after the first (`--input2` and on): seat and script.
    pub seats: Vec<(usize, String)>,
    /// Where the player starts instead of the level's spawn (x, y).
    pub at: Option<[f32; 2]>,
    pub shadows: crate::settings::Shadows,
    /// Hints for a gamepad of this kind instead of the keyboard.
    pub pad: Option<crate::controls::PadStyle>,
    pub width: u32,
    pub height: u32,
    pub out: PathBuf,
    /// A scenario and console lines the run begins with.
    pub begin: crate::session::Begin,
}

/// Draws of a frame `shot` makes to time its passes; the first ones only
/// set the GPU up and do not count.
const TIMED_DRAWS: usize = 8;
const WARM_UP_DRAWS: usize = 3;

/// A GPU without a window: frames drawn into a texture and read back
/// (`shot`, `film`, the editor's `--shot`).
pub(crate) struct Canvas {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    view: wgpu::TextureView,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
    width: u32,
    height: u32,
    /// Bytes a row takes in the readback buffer (rows are padded).
    row: u32,
    /// The renderer times its passes (the GPU has timestamps).
    timed: bool,
    /// The faces the meshes hold (`Session::face_mask`).
    mask: crate::mesh::FaceMask,
}

impl Canvas {
    pub(crate) fn new(width: u32, height: u32, time_passes: bool) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .context("no suitable GPU")?;
        // Timestamps, where the GPU has them, to say what each pass takes.
        let required_features = if time_passes { adapter.features() & wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("voxcade headless"),
            required_features,
            ..Default::default()
        }))
        .context("creating GPU device")?;
        // A picture larger than the GPU's textures is said, not a panic
        // deep in wgpu.
        let most = device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > most || height > most {
            bail!("a picture of {width} by {height} pixels cannot be drawn: this GPU draws pictures from 1 to {most} pixels each way");
        }
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut renderer = Renderer::new(&device, format, width, height);
        let timed = time_passes && renderer.time_passes(&device, &queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self { device, queue, renderer, view, target, readback, width, height, row, timed, mask: [false; 6] })
    }

    /// Builds the meshes of a whole world, with the faces in `mask`.
    pub(crate) fn mesh_world(&mut self, world: &mut crate::world::World, mask: &crate::mesh::FaceMask) {
        self.renderer.rebuild_all(&self.device, &self.queue, world, mask);
    }

    /// Whether the faces remember their shadow here (made the first time
    /// a frame asked): the tests'.
    #[cfg(test)]
    pub(crate) fn remembers(&self) -> bool {
        self.renderer.remembers()
    }

    /// Builds the meshes of the whole world.
    fn mesh_all<G: Game>(&mut self, game: &mut Session<G>) {
        self.mask = game.face_mask();
        self.renderer.rebuild_all(&self.device, &self.queue, game.world_mut(), &self.mask);
    }

    /// Ready to draw a frame of the game: in the picture's own shape
    /// (`c.camera.aspect`), the meshes built anew if a picture the game set
    /// (`frame:camera` at another pitch or field of view) needs more faces.
    fn fit<G: Game>(&mut self, game: &mut Session<G>) {
        self.renderer.set_shape(&self.device, game.cfg.camera.aspect);
        if game.face_mask() != self.mask {
            self.mesh_all(game);
        }
    }

    /// Rebuilds the chunks the last tick changed and sends them to the GPU,
    /// as the game does every frame; how many.
    fn remesh<G: Game>(&mut self, game: &mut Session<G>) -> usize {
        if game.face_mask() != self.mask {
            self.mesh_all(game);
        }
        let dirty = self.renderer.update_chunks(&self.device, &self.queue, game.world_mut(), &self.mask);
        // Held back, the uploads of all ticks pile up in GPU memory.
        self.queue.submit([]);
        dirty
    }

    /// Draws a frame into the texture (without reading it back: more may
    /// be drawn over it, the editor's panels).
    pub(crate) fn render(&mut self, frame: &FrameData) {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer.render(&self.device, &self.queue, &mut encoder, &self.view, frame);
        self.queue.submit([encoder.finish()]);
    }

    /// Draws a frame and reads it back: RGBA, row after row.
    pub(crate) fn draw(&mut self, frame: &FrameData) -> Result<Vec<u8>> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer.render(&self.device, &self.queue, &mut encoder, &self.view, frame);
        self.read(encoder)
    }

    /// The GPU, and the texture to draw into (RGBA, not sRGB), for drawing
    /// other than the game's (the IDE's `--shot`).
    pub(crate) fn gpu(&self) -> (&wgpu::Device, &wgpu::Queue, &wgpu::TextureView) {
        (&self.device, &self.queue, &self.view)
    }

    /// What was drawn into the texture, read back after `encoder`'s work:
    /// RGBA, row after row.
    pub(crate) fn read(&mut self, mut encoder: wgpu::CommandEncoder) -> Result<Vec<u8>> {
        let (width, height, row) = (self.width, self.height, self.row);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.expect("mapping readback buffer"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).context("waiting for GPU")?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        {
            let data = slice.get_mapped_range().context("reading back the frame")?;
            for y in 0..height as usize {
                let start = y * row as usize;
                pixels.extend_from_slice(&data[start..start + width as usize * 4]);
            }
        }
        self.readback.unmap();
        Ok(pixels)
    }
}

/// Every screen `shot --screen` shows, with what it is: the command line
/// takes these names and its help lists them (`cli`); `screen_menu`
/// begins each.
pub const SCREENS: &[(&str, &str)] = &[
    ("title", "the title, N ticks into its animations"),
    ("continue", "the title of a game with a save to go on from"),
    ("settings", "SETTINGS"),
    ("audio", "SETTINGS' AUDIO page"),
    ("video", "SETTINGS' VIDEO page"),
    ("controls", "SETTINGS' CONTROLS page"),
    ("players", "a game of several's CONTROLS: who plays with what"),
    ("player-controls", "the second player's own keys"),
    ("network", "a game of several's NETWORK: host or join"),
    ("find", "JOIN's list of the games found (samples)"),
    ("join", "the host's address typed"),
    ("code", "a game's code typed (across the internet)"),
    ("joining", "joining, catching up"),
    ("joining-net", "joining across the internet: the code to say"),
    ("intro", "the glide from the title into the game, N ticks in"),
    ("pause", "the pause page over the game after N ticks"),
    ("save", "the pause's SAVE page, the run's saves in it"),
    ("load", "the pause's LOAD page"),
    ("host-pause", "a network host's pause page: who plays, one to drop"),
    ("host-alone", "a host's pause page before anyone came: how to let them in"),
    ("host-net-pause", "a host's pause page across the internet: its code, one asking in"),
    ("guest-pause", "a guest's own pause page"),
    ("attract", "a --replay as the title plays it, PRESS START over it"),
    ("highscores", "the high score table, a sample run's line in it"),
    ("initials", "a sample run's initials being entered"),
    ("console", "the console open over the game, the --do lines in it"),
];

/// The menu a `--screen` begins with, and whether the title's scene is
/// behind it: `title` and `players` the game's (`c.title`, `c.players`),
/// `replay` whether a replay plays.
pub(crate) fn screen_menu(screen: Option<&str>, title: &str, players: u32, replay: bool) -> Result<(Menu, bool)> {
    Ok(match screen {
        None => (Menu::default(), false),
        Some("title") => (Menu::title(), true),
        // The title of a game with a save to go on from.
        Some("continue") => (Menu::title().continuing(Some(1)), true),
        Some("settings") => (Menu { stack: vec![(Page::Title, 1), (Page::Settings, 0)], ..Menu::title() }, true),
        Some("audio") => (Menu { stack: vec![(Page::Title, 1), (Page::Settings, 0), (Page::Audio, 0)], ..Menu::title() }, true),
        Some("video") => (Menu { stack: vec![(Page::Title, 1), (Page::Settings, 1), (Page::Video, 0)], ..Menu::title() }, true),
        Some("controls") => (Menu { stack: vec![(Page::Title, 1), (Page::Settings, 2), (Page::Controls, 0)], ..Menu::title() }, true),
        // A game of several's CONTROLS: who plays with what, and the
        // second player's keys at the keyboard's right half.
        Some("players") => (Menu { stack: vec![(Page::Title, 1), (Page::Settings, 2), (Page::Players, 0)], ..Menu::title() }, true),
        Some("player-controls") => (Menu { stack: vec![(Page::Title, 1), (Page::Settings, 2), (Page::Players, 5), (Page::PlayerControls(2), 0)], ..Menu::title() }, true),
        // A network game (a game of several): HOST or JOIN, the host's
        // address typed, joining (how far).
        Some("network") => (Menu { stack: vec![(Page::Title, 2), (Page::Network, 0)], ..Menu::title() }, true),
        Some("find") => {
            let found = vec![
                crate::menu::FoundLine { line: format!("{}  ANNA  1/{}  192.168.1.5:26500", title.to_uppercase(), players), address: "192.168.1.5:26500".into(), same: true },
                crate::menu::FoundLine { line: "MORE BLOOD  BEN  1/1  192.168.1.7:26500".into(), address: "192.168.1.7:26500".into(), same: false },
            ];
            let found = found.into_iter().chain([crate::menu::FoundLine { line: format!("{}  BEN  1/2  DX67A3", title.to_uppercase()), address: "DX67A3".into(), same: true }]).collect();
            (Menu { stack: vec![(Page::Title, 2), (Page::Network, 1), (Page::Find, 0)], found, internet: true, ..Menu::title() }, true)
        }
        Some("join") => (Menu { stack: vec![(Page::Title, 2), (Page::Network, 1), (Page::Find, 3), (Page::Join, 0)], address: "192.168.1.5".into(), ..Menu::title() }, true),
        // A game's code being typed (across the internet).
        Some("code") => (Menu { stack: vec![(Page::Title, 2), (Page::Network, 1), (Page::Find, 2), (Page::Code, 0)], code: "DX67".into(), ..Menu::title() }, true),
        Some("joining") => {
            let net = crate::menu::NetView { lines: vec!["CATCHING UP 40%".into()], ..Default::default() };
            (Menu { stack: vec![(Page::Title, 2), (Page::Joining, 0)], net: Some(net), ..Menu::title() }, true)
        }
        // Joining across the internet: connected, the code to say.
        Some("joining-net") => {
            let net = crate::menu::NetView { lines: vec![crate::lang::say_with("en", "THE HOST LETS YOU IN: SAY {}", &["KQ7P"])], ..Default::default() };
            (Menu { stack: vec![(Page::Title, 2), (Page::Joining, 0)], net: Some(net), ..Menu::title() }, true)
        }
        Some("pause" | "save" | "load" | "console" | "host-pause" | "host-alone" | "host-net-pause" | "guest-pause") => (Menu::default(), false),
        Some("intro") => (Menu::default(), true),
        // The high score table, a new line blinking, and the initials of
        // one being entered (`--frames` ticks into their animations).
        Some("highscores" | "initials") => (Menu::default(), true),
        // A replay as the title plays it, PRESS START over it.
        Some("attract") if replay => (Menu::default(), false),
        Some("attract") => bail!("--screen attract shows a replay: give one with --replay"),
        Some(other) => bail!("no screen '{other}' ({})", SCREENS.iter().map(|(s, _)| *s).collect::<Vec<_>>().join(", ")),
    })
}

pub fn shot<G: Game>(dir: &Path, opts: &ShotOptions) -> Result<()> {
    let started = Instant::now();
    let replay = opts.replay.as_deref().map(Replay::load).transpose()?;
    let mut game = match &replay {
        Some(r) => crate::replay::replay_session::<G>(dir, r)?,
        None => Session::<G>::load(dir)?,
    };
    let load_ms = ms(started);

    let (width, height) = (opts.width, opts.height);
    let mut canvas = Canvas::new(width, height, true)?;
    let timed = canvas.timed;
    let started = Instant::now();
    canvas.mesh_all(&mut game);
    let mesh_ms = ms(started);

    if opts.replay.is_some() && (!opts.input.is_empty() || !opts.seats.is_empty()) {
        bail!("--replay plays its own input: leave out --input");
    }
    if (opts.replay.is_some() || opts.record.is_some()) && opts.at.is_some() {
        bail!("a replay starts where the game starts: leave out --at");
    }
    if opts.replay.is_some() && !opts.begin.is_empty() {
        bail!("a replay begins as its run began: leave out --scenario and --do");
    }
    // What the lines answered, for the console's picture (--screen console).
    let mut answered = Vec::new();
    opts.begin.apply(&mut game, |line, said| {
        println!("> {line}\n{said}");
        answered.push((line.to_string(), said.to_string()));
    })?;
    let mut script = Script::new(parse_script(&opts.input, game.actions())?);
    let mut seats = SeatScripts::parse(&opts.seats, game.actions(), game.cfg.players)?;
    if let Some([x, y]) = opts.at {
        game.game.place_player(x, y);
    }
    // Every tick timed, the simulation and the meshes it changed rebuilt as
    // the game does: blasts show as the slowest ticks.
    let mut ticks: Vec<(u32, f64, f64, usize)> = Vec::new();
    let started = Instant::now();
    // What would have been heard: sounds per event, in the order they first came.
    let mut heard: Vec<(&'static str, u32)> = Vec::new();
    // A menu screen: the title's run their animations for the frames
    // instead of the game; pause first plays them.
    let (mut menu, title) = screen_menu(opts.screen.as_deref(), &game.cfg.title, game.cfg.players, opts.replay.is_some())?;
    // A game of several offers NETWORK on its title.
    menu.network = game.cfg.players > 1;
    if matches!(opts.screen.as_deref(), Some("network" | "find" | "join" | "code" | "joining" | "joining-net")) && game.cfg.players < 2 {
        bail!("--screen {}: a game of several players (c.players) has it", opts.screen.as_deref().unwrap_or(""));
    }
    // The seats of a game of several: the first at the keyboard's left
    // half, the second at its right (as the screens of the PLAYERS page
    // show them; the title offers the page).
    if game.cfg.players > 1 {
        let mut seats = crate::seating::Seating::new(game.cfg.players);
        seats.begin(None, &[]);
        if matches!(opts.screen.as_deref(), Some("players" | "player-controls")) {
            seats.set_keyboard(2, crate::seating::Keys::Right);
        }
        menu.seats = crate::menu::SeatLine::all(&seats, &[]);
    } else if matches!(opts.screen.as_deref(), Some("players" | "player-controls")) {
        bail!("--screen {}: a game of several players (c.players) has it", opts.screen.as_deref().unwrap_or(""));
    }
    let mut settings = Settings::default();
    settings.shadows = opts.shadows;
    settings.adopt(game.actions(), &game.default_keys(), &game.default_pad());
    settings.offer_languages(&game.cfg.languages);
    settings.adopt_scores(&game.cfg.high_scores);
    // A sample run: third in the game's table.
    let sample = settings.high_scores.get(1..3).map_or(1000, |h| (h[0].score + h[1].score) / 2);
    match opts.screen.as_deref() {
        Some("highscores") => menu = Menu::high_scores(settings.enter("YOU", sample)),
        Some("initials") => {
            menu = Menu::initials(sample, settings.rank(sample).unwrap_or(0));
            (menu.entry.letters, menu.entry.at) = (['Y', 'O', 'A'], 2);
            // A game of several: the second player's turn.
            menu.entry.seat = (game.cfg.players > 1).then_some(2);
        }
        _ => {}
    }
    game.options.shadow_map = opts.shadows.map_size();
    let mut playback = replay.as_ref().map(|r| Playback::new(r, &mut game)).transpose()?;
    // Told once it began as the run did (a scenario's file counts).
    if replay.as_ref().is_some_and(|r| !r.fits(game.fingerprint().unwrap_or(0))) {
        eprintln!("warning: the replay was recorded on other game data");
    }
    // Someone plays: on the keyboard, or the pad `--pad` names (a replay
    // brings its own).
    if playback.is_none() {
        let (device, style) = opts.pad.map_or((Device::Keyboard, PadStyle::default()), |style| (Device::Gamepad, style));
        let controls = game.player_controls(device, style, &settings);
        for &(seat, _) in &opts.seats {
            game.set_seat_controls(seat, controls.clone());
        }
        game.set_controls(controls);
    }
    let mut recorder = opts.record.as_ref().map(|_| Recorder::default());
    let mut apart = None;
    let frames = opts.frames.or(replay.as_ref().map(|r| r.ticks as u32)).unwrap_or(90);
    // The hidden-state guard as the IDE's PLAY has it (`inspect` too).
    let mut watch = crate::lua::hidden::Watch::default();
    watch.begin(game.game.hidden());
    let mut warnings = Vec::new();
    for tick in 0..frames {
        if title {
            menu.update();
            continue;
        }
        let step = Instant::now();
        let input = match &mut playback {
            Some(p) => p.next_input(&mut game),
            None => {
                seats.next(&mut game);
                script.next_input()
            }
        };
        game.step(&input);
        let sim_tick = ms(step);
        if let Some(Err(a)) = playback.as_mut().map(|p| p.check(&mut game)) {
            apart.get_or_insert(a);
        }
        if let Some(r) = &mut recorder {
            r.record(&game);
        }
        let remesh = Instant::now();
        let dirty = canvas.remesh(&mut game);
        ticks.push((tick, sim_tick, ms(remesh), dirty));
        for event in &game.game.sim().sounds {
            match heard.iter_mut().find(|(name, _)| *name == event.name) {
                Some((_, count)) => *count += 1,
                None => heard.push((event.name, 1)),
            }
        }
        warnings.extend(watch.tick(|| game.game.hidden()));
        // A script's error: the frame shows it, then the shot ends with it.
        if game.game.halted().is_some() {
            break;
        }
    }
    for line in &warnings {
        println!("warning: {line}");
    }
    let _ = started;
    let sim_ms = ticks.iter().fold(0.0, |sum, t| sum + t.1);


    if let Some(screen @ ("pause" | "save" | "load" | "host-pause" | "host-alone" | "host-net-pause" | "guest-pause")) = opts.screen.as_deref() {
        let now = crate::saves::now();
        let seats = std::mem::take(&mut menu.seats);
        menu = Menu::pause().with_slots((1..=game.slots.count()).map(|n| game.slots.info(n).map(|i| crate::menu::slot_line(n, Some(&i), now, &game.language()))).collect()).with_seats(seats);
        // A network game's: the host's addresses, who plays, one gone
        // quiet to drop; a guest's own menu.
        menu.net = match screen {
            "host-pause" => Some(crate::menu::NetView {
                hosting: true,
                lines: vec!["HOSTED AT 192.168.1.5:26500".into()],
                who: vec!["ANNA 4 MS".into(), "BEN 31 MS ...".into()],
                help: Vec::new(),
                drops: vec![(2, "ben".into())],
                seated: true,
                ticket: false,
                code: false,
                knocks: Vec::new(),
            }),
            // Nobody came yet: the help to let them in.
            "host-alone" => Some(crate::menu::NetView {
                hosting: true,
                lines: vec!["HOSTED AT 192.168.1.5:26500".into()],
                help: vec![crate::lang::say_with("en", "NOBODY COMES? LET UDP {} IN:", &["26500"]), crate::lang::say_with("en", "SUDO UFW ALLOW {}/UDP", &["26500"])],
                ..Default::default()
            }),
            // Across the internet: the game's code, a machine asking in
            // with the signs to compare, the code to copy again.
            "host-net-pause" => Some(crate::menu::NetView {
                hosting: true,
                lines: vec![crate::lang::say_with("en", "ON THE INTERNET: CODE {}", &["DX67A3"])],
                who: vec!["ANNA 23 MS".into()],
                seated: true,
                ticket: true,
                code: true,
                knocks: vec![(2, "BEN KQ7P".into())],
                ..Default::default()
            }),
            "guest-pause" => Some(crate::menu::NetView { hosting: false, who: vec!["HOST 0 MS".into(), "ANNA 4 MS".into()], seated: true, ..Default::default() }),
            _ => None,
        };
        match screen {
            "save" => menu.stack = vec![(Page::Pause, 1), (Page::Save, 1)],
            "load" => menu.stack = vec![(Page::Pause, 2), (Page::Load, 1)],
            _ => {}
        }
        for _ in 0..30 {
            menu.update();
        }
    }
    if let Some(p) = &playback {
        match apart {
            None => println!("replay: as recorded ({} s checked)", p.steps() / crate::replay::CHECK_EVERY),
            Some(a) => println!("replay: runs apart after {} s", a.second),
        }
    }
    if let (Some(r), Some(path), None) = (&recorder, &opts.record, game.game.halted()) {
        let replay = r.replay(&game)?;
        replay.save(path)?;
        println!("recorded {} ({} ticks, {}, score {})", path.display(), replay.ticks, replay.outcome, replay.score);
    }
    let time = frames as f32 * crate::sim::DT;
    let scene = match opts.screen.as_deref() {
        Some("intro") => Scene::Intro { time, t: time / crate::session::INTRO_TIME },
        _ if title => Scene::Title { time },
        _ => Scene::Play,
    };
    // What building the frame takes on the CPU: the median of a few builds
    // (the first also fills caches, like the display font's glyphs).
    let mut builds = Vec::new();
    let mut frame = None;
    if let Some(style) = opts.pad {
        (menu.device, menu.pad_style) = (crate::controls::Device::Gamepad, style);
    }
    for _ in 0..TIMED_DRAWS {
        let started = Instant::now();
        frame = Some(game.menu_frame(&menu, &settings, 1.0, width as f32 / height as f32, scene));
        builds.push(ms(started));
    }
    let mut frame = frame.expect("a frame was built");
    if opts.screen.as_deref() == Some("attract") {
        crate::ui::press_start(&mut frame.ui, 0.0, &game.cfg.ui, game.picture_aspect(width as f32 / height as f32), &game.language());
    }
    // The console open over the game, the --do lines in it as run.
    if opts.screen.as_deref() == Some("console") {
        let mut prompt = crate::prompt::Prompt::default();
        prompt.open();
        prompt.part = 1.0;
        for (line, said) in &answered {
            prompt.type_text(line);
            prompt.take();
            prompt.answer(Ok(said.clone()));
        }
        let (ui, aspect) = (&game.cfg.ui, game.picture_aspect(width as f32 / height as f32));
        let camera = crate::ui::camera(&crate::ui::Layout::of(ui, aspect), ui.slant, ui.depth as f32);
        frame = game.compose(1.0, aspect, Scene::Play, Some((crate::ui::prompt_panel(&prompt, true, ui, aspect), Vec::new(), camera)));
        frame.view_proj = glam::Mat4::from_translation(glam::Vec3::new(0.0, 2.0 * crate::ui::prompt_lift(&prompt), 0.0)) * frame.view_proj;
    }
    builds.sort_by(f64::total_cmp);
    let frame_ms = builds[builds.len() / 2];
    canvas.fit(&mut game);
    // What each pass takes on the GPU: the median of a few draws of this
    // frame (the first ones also set the GPU up).
    let mut gpu: Vec<(&'static str, Vec<f64>)> = Vec::new();
    if timed {
        for k in 0..TIMED_DRAWS {
            canvas.render(&frame);
            for (label, took) in canvas.renderer.pass_times(&canvas.device)? {
                if k >= WARM_UP_DRAWS {
                    match gpu.iter_mut().find(|(l, _)| *l == label) {
                        Some((_, all)) => all.push(took),
                        None => gpu.push((label, vec![took])),
                    }
                }
            }
        }
    }
    let pixels = canvas.draw(&frame)?;
    write_png(&opts.out, width, height, &pixels)?;
    if game.game.halted().is_some() {
        println!("wrote {} ({width}x{height}), the game's error over it", opts.out.display());
        return game.stopped();
    }

    let s = canvas.renderer.stats;
    println!("wrote {} ({width}x{height})", opts.out.display());
    println!(
        "ticks {} | load {load_ms:.0} ms | sim {sim_ms:.1} ms | mesh {mesh_ms:.0} ms | chunks {}/{} drawn, {} in shadow map | {}k of {}k verts | {} particles",
        frames,
        s.drawn,
        s.chunks,
        s.shadowed,
        s.vertices / 1000,
        s.vertices_total / 1000,
        game.game.sim().particles.len(),
    );
    // What one frame costs: building it on the CPU, drawing it on the GPU.
    let median = |all: &[f64]| {
        let mut sorted = all.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted[sorted.len() / 2]
    };
    let passes: Vec<String> = gpu.iter().map(|(label, all)| format!("{label} {:.2}", median(all))).collect();
    let total: f64 = gpu.iter().map(|(_, all)| median(all)).sum();
    println!(
        "frame: cpu {frame_ms:.1} ms ({} cubes + {} from {} models on the GPU, {} ui) | gpu {}",
        s.instances,
        s.model_cubes,
        s.models,
        s.ui_instances,
        if timed { format!("{total:.2} ms ({})", passes.join(", ")) } else { "not timed (no timestamp queries)".to_string() }
    );
    // The slowest ticks: what a blast costs.
    if !ticks.is_empty() {
        let mut slow = ticks.clone();
        slow.sort_by(|a, b| (b.1 + b.2).total_cmp(&(a.1 + a.2)));
        let list: Vec<String> = slow
            .iter()
            .take(3)
            .map(|(tick, sim, remesh, chunks)| format!("tick {tick}: sim {sim:.2} + remesh {remesh:.2} ms ({chunks} chunks)"))
            .collect();
        println!("ticks: sim {:.3} ms on average | slowest {}", sim_ms / ticks.len() as f64, list.join(", "));
    }
    for line in game.game.report() {
        println!("{line}");
    }
    if !heard.is_empty() {
        let list: Vec<String> = heard.iter().map(|(name, count)| format!("{name} {count}")).collect();
        println!("sounds: {}", list.join(", "));
    }
    Ok(())
}

/// What `film` makes and how.
pub struct FilmOptions {
    pub replay: PathBuf,
    /// Seconds into the replay where the film starts and ends (default: its
    /// end; a GIF runs `GIF_SECONDS`).
    pub from: f32,
    pub to: Option<f32>,
    /// Frames a second, dividing 60 (default: 60, a GIF 20).
    pub fps: Option<u32>,
    /// Default: 1920x1080, a GIF 480x270 (GIFs grow big fast).
    pub size: Option<(u32, u32)>,
    /// Keep the video under this many megabytes (a chat's upload limit).
    pub max_mb: Option<f32>,
    /// `.mp4` (H.264 and AAC), `.webm` (VP9 and Opus) or `.gif` (no sound).
    pub out: PathBuf,
}

/// Seconds a GIF runs when `--to` does not say: GIFs grow big.
const GIF_SECONDS: f32 = 10.0;

/// Plays `frames` ticks of `input` without drawing and prints, as JSON,
/// how the game stands: the tick, the outcome and score, `report`'s lines,
/// the sounds it made, the error that stopped it, and the game's things and
/// state (`Game::inspect`). For tools: an agent, `voxcade mcp`.
pub fn inspect<G: Game>(dir: &Path, frames: u32, input: &str, seats: &[(usize, String)], at: Option<[f32; 2]>, begin: &crate::session::Begin) -> Result<()> {
    let standing = inspected::<G>(dir, frames, input, seats, at, begin)?;
    println!("{}", serde_json::to_string_pretty(&standing)?);
    // A halted script fails the command, as `shot`'s does; the JSON says
    // where.
    if let Some(error) = standing["halted"].as_str() {
        anyhow::bail!("the game stopped at tick {}: {}", standing["tick"], error.lines().next().unwrap_or(""));
    }
    Ok(())
}

/// What `inspect` prints: the game played `frames` ticks and drawn after
/// each as a window draws it, how it stands then, and what the hidden-
/// state guard said (`warnings`).
fn inspected<G: Game>(dir: &Path, frames: u32, input: &str, seats: &[(usize, String)], at: Option<[f32; 2]>, begin: &crate::session::Begin) -> Result<serde_json::Value> {
    let mut game = Session::<G>::load(dir)?;
    // What the lines answer goes beside the JSON, not into it.
    begin.apply(&mut game, |line, said| eprintln!("> {line}\n{said}"))?;
    if let Some([x, y]) = at {
        game.game.place_player(x, y);
    }
    let mut script = Script::new(parse_script(input, game.actions())?);
    let mut seat_scripts = SeatScripts::parse(seats, game.actions(), game.cfg.players)?;
    // Someone plays on the keyboard, as in `shot`.
    let controls = keyboard(&game);
    for &(seat, _) in seats {
        game.set_seat_controls(seat, controls.clone());
    }
    game.set_controls(controls);
    let mut heard: std::collections::BTreeMap<&str, u32> = Default::default();
    // The hidden-state guard as the IDE's PLAY has it: a local of the
    // game's files its functions change is said, the run goes on.
    let mut watch = crate::lua::hidden::Watch::default();
    watch.begin(game.game.hidden());
    let mut warnings = Vec::new();
    for _ in 0..frames {
        seat_scripts.next(&mut game);
        game.step(&script.next_input());
        // Drawn as a window draws it (without the picture), as `check`
        // does: a mistake in on_draw halts the game here too.
        game.look();
        for event in &game.game.sim().sounds {
            *heard.entry(event.name).or_default() += 1;
        }
        warnings.extend(watch.tick(|| game.game.hidden()));
        if game.game.halted().is_some() {
            break;
        }
    }
    let mut standing = standing(&game, &heard);
    standing["warnings"] = serde_json::json!(warnings);
    Ok(standing)
}

/// How `inspect` plays over the network: hosting on a port, or joining a
/// host at an address.
pub enum Networked {
    Host(u16),
    /// Hosting across the internet (iroh, a ticket).
    HostNet,
    /// Joining an address on the LAN, or a ticket.
    Join(String),
}

/// The keys someone at the keyboard has, as in `shot` and `inspect`.
fn keyboard<G: Game>(game: &Session<G>) -> crate::controls::Controls {
    let mut settings = Settings::default();
    settings.adopt(game.actions(), &game.default_keys(), &game.default_pad());
    settings.offer_languages(&game.cfg.languages);
    game.player_controls(Device::Keyboard, PadStyle::default(), &settings)
}

/// How the game stands, as `inspect` prints it.
fn standing<G: Game>(game: &Session<G>, heard: &std::collections::BTreeMap<&str, u32>) -> serde_json::Value {
    let outcome = match game.game.outcome() {
        Some(crate::game::Outcome::Won) => "won",
        Some(crate::game::Outcome::Lost) => "lost",
        None => "playing",
    };
    serde_json::json!({
        "tick": game.game.sim().tick,
        "outcome": outcome,
        "score": game.game.score(),
        "report": game.game.report(),
        "sounds": heard,
        "halted": game.game.halted(),
        "game": game.game.inspect(),
    })
}

/// `inspect` as a network game, in real time over a LAN's UDP or across
/// the internet (`docs/multiplayer.md`, stages 4 and 6; a host across the
/// internet says its ticket and lets in every machine that asks, saying
/// its code): the host plays `frames` ticks with its player's keys, then
/// waits for its guests to have them all and says farewell; a guest joins,
/// plays the run so far through, sits and plays its keys until the host
/// leaves. Each prints how the game stands and writes its recording with
/// `record` (a guest's equals the host's).
pub fn inspect_net<G: Game>(dir: &Path, frames: u32, input: &str, begin: &crate::session::Begin, net: &Networked, record: Option<&Path>) -> Result<()> {
    use crate::net::link::{Link, Where};
    use crate::net::play::{Hosting, Joined, Joining, Locals};
    use crate::net::udp;
    let start = Instant::now();
    let micros = || start.elapsed().as_micros() as u64;
    let frame = std::time::Duration::from_micros(crate::net::TICK_US);
    let none = std::collections::BTreeMap::new();
    match net {
        Networked::Host(_) | Networked::HostNet => {
            let mut game = Session::<G>::load(dir)?;
            let (mut socket, port) = match net {
                Networked::Host(port) => {
                    let (link, port) = Link::lan_host(Some(*port))?;
                    (link, Some(port))
                }
                _ => (Link::net_host(crate::net::quic::reach(), Some(crate::network::room_of(&game)?))?, None),
            };
            begin.apply(&mut game, |line, said| eprintln!("> {line}\n{said}"))?;
            game.set_clock(crate::saves::now());
            let mut script = Script::new(parse_script(input, game.actions())?);
            let mut hosting = Hosting::<G, Where>::new(&game, udp::random(), &udp::machine_name())?;
            hosting.reports_to = Some(crate::net::play::reports());
            hosting.host.set_limit(socket.limit());
            match (port, socket.ticket()) {
                (Some(port), _) => {
                    eprintln!("{} hosted on UDP port {port}", game.cfg.title);
                    hosting.host.set_here(crate::net::wire::Here {
                        game: crate::settings::game_name(dir),
                        title: game.cfg.title.clone(),
                        fingerprint: game.fingerprint()?,
                        commit: udp::COMMIT.to_string(),
                        name: udp::machine_name(),
                        seats: 0,
                        taken: 0,
                        port,
                    });
                }
                (None, ticket) => eprintln!("{} hosted across the internet, the ticket:\n{}", game.cfg.title, ticket.unwrap_or_default()),
            }
            hosting.set_controls(1, keyboard(&game));
            let (mut owed, mut last, mut done) = (0u64, 0u64, None::<Instant>);
            // The seats the room at our matchmaker shows.
            let mut room = None;
            for n in 0u32.. {
                let now = micros();
                for (from, bytes) in socket.drain() {
                    hosting.host.receive(now, from, &bytes);
                }
                for knock in socket.let_all_in() {
                    eprintln!("let in a machine asking in, code {}", knock.code);
                }
                if socket.game_code().is_some() && room != Some(hosting.host.seats_taken()) {
                    let seats = hosting.host.seats_taken();
                    room = Some(seats);
                    socket.update_room(matchmaker_protocol::Update { taken: Some(seats.0 as u32), seats: Some(seats.1 as u32), ..Default::default() });
                    eprintln!("the room shows {}/{} seats taken", seats.0, seats.1);
                }
                for trouble in socket.trouble() {
                    eprintln!("the network: {trouble}");
                }
                for notice in hosting.notices(&game)? {
                    eprintln!("{notice:?}");
                }
                game.set_clock(crate::saves::now());
                if hosting.host.next_tick() < frames as u64 {
                    hosting.read(1, script.next_input());
                    owed += now - last;
                    let mut ticks = 0;
                    while owed >= crate::net::TICK_US && ticks < crate::net::MAX_CATCH_UP && hosting.host.next_tick() < frames as u64 {
                        hosting.tick(&mut game)?;
                        game.stopped()?;
                        (owed, ticks) = (owed - crate::net::TICK_US, ticks + 1);
                    }
                } else {
                    let since = done.get_or_insert_with(Instant::now);
                    if hosting.host.all_delivered() || since.elapsed().as_secs() >= 3 {
                        for _ in 0..10 {
                            for (to, bytes) in hosting.host.farewell() {
                                socket.send(to, &bytes);
                            }
                        }
                        break;
                    }
                }
                last = now;
                for (to, bytes) in hosting.host.outgoing(now) {
                    socket.send(to, &bytes);
                }
                std::thread::sleep((frame * (n + 1)).saturating_sub(start.elapsed()));
            }
            if let Some(path) = record {
                hosting.recording(&game)?.save(path)?;
            }
            let mut out = standing(&game, &none);
            out["network"] = serde_json::json!({ "port": port, "internet": port.is_none(), "machines": hosting.host.roster().iter().map(|m| &m.name).collect::<Vec<_>>() });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Networked::Join(address) => {
            anyhow::ensure!(begin.is_empty(), "a guest plays the host's run: --scenario and --do are the host's");
            // What our matchmaker knows this game by: a code of another is
            // refused as such, and `any` asks for any room of it.
            let known = Session::<G>::load(dir).ok().and_then(|g| crate::network::app_of(&g).ok());
            let (mut socket, host) = if address == "any" {
                let (app, version) = known.context("--join any: the game would not load")?;
                Link::net_guest_any(crate::net::quic::reach(), matchmaker_protocol::Search { app, version, ..Default::default() })?
            } else if Link::is_net(address) {
                Link::net_guest(address, crate::net::quic::reach(), known)?
            } else {
                (Link::lan_guest()?, Where::Lan(udp::address(address)?))
            };
            // A machine of its own each time: two of these on one computer
            // are two guests.
            let id = [udp::random().to_le_bytes(), udp::random().to_le_bytes()].concat().try_into().expect("16 bytes");
            let guest = crate::net::Guest::new(host, id, udp::random(), &udp::machine_name(), udp::COMMIT, micros());
            let mut joining = Joining::<G, Where>::new(dir, guest);
            joining.reports_to = Some(crate::net::play::reports());
            let mut game: Option<Session<G>> = None;
            let mut locals = Locals::new(1, false);
            let mut script = None;
            for n in 0u32.. {
                let now = micros();
                for (from, bytes) in socket.drain() {
                    joining.guest.receive(now, from, &bytes);
                }
                // Nowhere to go (no game with the code, our server silent).
                for trouble in socket.trouble() {
                    if matches!(joining.state(), Joined::Waiting(None)) {
                        bail!("{trouble}");
                    }
                    eprintln!("the network: {trouble}");
                }
                if game.is_none() {
                    match joining.catch_up(std::time::Duration::from_millis(40)) {
                        Ok(caught) => {
                            if let Some(g) = &caught {
                                eprintln!("caught up with the host at tick {} after {:.1} s", g.game.sim().tick, start.elapsed().as_secs_f32());
                            }
                            game = caught;
                        }
                        Err(e) => bail!("{e:#}"),
                    }
                }
                if let Some(game) = &mut game {
                    // Its one player sits once caught up.
                    locals.settle(&mut joining, |_| true);
                    if let Some(seat) = locals.seat(1) {
                        let keys = match &mut script {
                            Some(s) => s,
                            None => script.insert(Script::new(parse_script(input, game.actions())?)),
                        };
                        joining.read(seat, keys.next_input());
                        joining.set_controls(seat, keyboard(game));
                    }
                    for b in joining.ticks(now, usize::MAX) {
                        joining.play(game, &b)?;
                    }
                }
                for (to, bytes) in joining.guest.outgoing(now) {
                    socket.send(to, &bytes);
                }
                match joining.state() {
                    Joined::Gone(why) if why == "THE HOST LEFT" => break,
                    Joined::Gone(why) => bail!("{why}"),
                    Joined::Apart(apart) => {
                        if let Some(game) = &game
                            && let Ok(Some(at)) = joining.report(game, &apart)
                        {
                            eprintln!("the desync's report: {}", at.display());
                        }
                        bail!("{}", crate::network::desync_line(&apart, "", &joining.host_commit));
                    }
                    Joined::Waiting(_) if start.elapsed().as_secs() >= 20 => bail!("no game at {address}"),
                    _ => {}
                }
                std::thread::sleep((frame * (n + 1)).saturating_sub(start.elapsed()));
            }
            let game = game.context("the game never came")?;
            if let Some(path) = record {
                joining.recording(&game)?.save(path)?;
            }
            println!("{}", serde_json::to_string_pretty(&standing(&game, &none))?);
        }
    }
    Ok(())
}

/// Renders a replay as a film: every frame drawn without a window, the
/// game's music and sounds mixed alongside, perfectly in step (the
/// simulation plays the same twice: once for the sound, once for the
/// pictures). Videos go through ffmpeg; without it the frames are written
/// as PNGs with the sound as WAV, and the command that joins them is said.
pub fn film<G: Game>(dir: &Path, opts: &FilmOptions) -> Result<()> {
    let replay = Replay::load(&opts.replay)?;
    let kind = opts.out.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(kind.as_str(), "mp4" | "webm" | "gif") {
        bail!("--out: a .mp4, .webm or .gif file");
    }
    let gif = kind == "gif";
    let fps = opts.fps.unwrap_or(if gif { 20 } else { 60 });
    if fps == 0 || 60 % fps != 0 {
        bail!("--fps must divide 60 (60, 30, 20, 15 ...)");
    }
    let every = u64::from(60 / fps);
    let (width, height) = opts.size.unwrap_or(if gif { (480, 270) } else { (1920, 1080) });
    if !gif && (width % 2 == 1 || height % 2 == 1) {
        bail!("--size: a video needs an even width and height");
    }
    let tick = |seconds: f32| (seconds * 60.0).round().max(0.0) as u64;
    let first = tick(opts.from).min(replay.ticks);
    let last = match opts.to {
        Some(t) => tick(t),
        None if gif => first + tick(GIF_SECONDS),
        None => replay.ticks,
    }
    .min(replay.ticks);
    if first >= last {
        bail!("nothing between --from and --to (the replay runs {} s)", replay.ticks / 60);
    }
    let seconds = (last - first) as f32 / 60.0;

    let started = Instant::now();
    // The sound's WAV lives in a folder of its own that goes with the film,
    // however it ends (the temp folder may be in RAM).
    let scratch = crate::scratch::Scratch::new("film");
    let sound = if gif { None } else { Some(film_sound::<G>(dir, &replay, first, last, &scratch.join("sound.wav"))?) };

    let mut game = crate::replay::replay_session::<G>(dir, &replay)?;
    let mut playback = Playback::new(&replay, &mut game)?;
    if !replay.fits(game.fingerprint()?) {
        eprintln!("warning: the replay was recorded on other game data: the film shows something else");
    }
    let mut canvas = Canvas::new(width, height, false)?;
    canvas.mesh_all(&mut game);
    let mut sink = match (gif, &sound) {
        (true, _) => Sink::gif(&opts.out, width, height)?,
        (false, Some(wav)) => Sink::video(&opts.out, &kind, width, height, fps, wav, opts.max_mb, seconds)?,
        (false, None) => unreachable!("a video has its sound"),
    };
    let (menu, settings) = (Menu::default(), Settings::default());
    let aspect = width as f32 / height as f32;
    let mut frames = 0u64;
    for tick in 0..last {
        let input = playback.next_input(&mut game);
        game.step(&input);
        game.stopped()?;
        if let Err(apart) = playback.check(&mut game) {
            eprintln!("warning: the replay runs apart after {} s", apart.second);
        }
        if tick >= first && (tick - first).is_multiple_of(every) {
            canvas.remesh(&mut game);
            let frame = game.menu_frame(&menu, &settings, 1.0, aspect, Scene::Play);
            canvas.fit(&mut game);
            sink.push(canvas.draw(&frame)?, fps)?;
            frames += 1;
            if frames.is_multiple_of(u64::from(fps) * 10) {
                eprintln!("{} of {seconds:.0} s", frames / u64::from(fps));
            }
        }
    }
    sink.finish()?;
    let size = std::fs::metadata(&opts.out).map(|m| m.len() as f64 / 1e6).unwrap_or(0.0);
    println!("wrote {} ({seconds:.1} s, {width}x{height}, {fps} fps, {size:.1} MB) in {:.0} s", opts.out.display(), started.elapsed().as_secs_f32());
    Ok(())
}

/// The film's sound: the replay played without drawing, its music and
/// sounds mixed as the game mixes them (the default volumes), from tick
/// `first` to `last`, as the WAV file `wav`.
fn film_sound<G: Game>(dir: &Path, replay: &Replay, first: u64, last: u64, wav: &Path) -> Result<PathBuf> {
    use crate::audio::{Mixer, SAMPLE_RATE};
    let mut game = crate::replay::replay_session::<G>(dir, replay)?;
    let mut playback = Playback::new(replay, &mut game)?;
    let settings = Settings::default();
    let mut mixer = Mixer::default();
    // The game's first song, as when it starts, and the songs its code
    // picks in time with it.
    game.music_changed = true;
    let per_tick = (SAMPLE_RATE / 60) as usize * 2;
    let mut buf = vec![0.0f32; per_tick];
    let mut samples = Vec::with_capacity((last - first) as usize * per_tick);
    for tick in 0..last {
        let input = playback.next_input(&mut game);
        game.step(&input);
        game.stopped()?;
        game.feed_music(&mut mixer, false);
        mixer.set_music_volume(game.cfg.audio.music_volume * settings.music_gain());
        mixer.set_drive(game.cfg.audio.drive);
        game.play_sounds(&mut mixer, settings.sound_gain());
        mixer.mix(&mut buf);
        if tick >= first {
            samples.extend_from_slice(&buf);
        }
    }
    // The limiter holds the sound back by its look-ahead: off the front,
    // and its tail played out, so the sound lies on the picture.
    let mut tail = vec![0.0f32; mixer.latency() * 2];
    mixer.mix(&mut tail);
    samples.extend_from_slice(&tail);
    samples.drain(..tail.len());
    write_wav(wav, &samples, 2)?;
    Ok(wav.to_path_buf())
}

/// How long a GIF shows frame `n`, in the hundredths of a second GIFs
/// count: they add up to the frame rate (30 fps: 3, 4, 3, 3, 4, 3 ...).
fn gif_delay(n: u64, fps: u32) -> u16 {
    let at = |n: u64| (n * 100 + u64::from(fps) / 2) / u64::from(fps);
    (at(n + 1) - at(n)) as u16
}

/// Where a film's frames go.
enum Sink {
    Gif { encoder: gif::Encoder<BufWriter<File>>, frames: u64, width: u16, height: u16 },
    Ffmpeg { child: std::process::Child, stdin: BufWriter<std::process::ChildStdin> },
    /// No ffmpeg: PNGs in a folder, joined later by the command said.
    Pngs { dir: PathBuf, frames: u64, command: String, width: u32, height: u32 },
}

impl Sink {
    fn gif(out: &Path, width: u32, height: u32) -> Result<Self> {
        if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let file = BufWriter::new(File::create(out).with_context(|| format!("creating {}", out.display()))?);
        let mut encoder = gif::Encoder::new(file, width as u16, height as u16, &[]).context("starting the GIF")?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        Ok(Sink::Gif { encoder, frames: 0, width: width as u16, height: height as u16 })
    }

    #[allow(clippy::too_many_arguments)]
    fn video(out: &Path, kind: &str, width: u32, height: u32, fps: u32, wav: &Path, max_mb: Option<f32>, seconds: f32) -> Result<Self> {
        if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let audio_kbps = if kind == "mp4" { 192 } else { 160 };
        let mut args: Vec<String> = ["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"].map(String::from).to_vec();
        args.extend([format!("{width}x{height}"), "-r".into(), fps.to_string(), "-i".into(), "-".into(), "-i".into(), wav.display().to_string()]);
        // Quality: constant, or as much as fits the size asked for.
        let video_kbps = max_mb.map(|mb| ((mb * 8000.0 * 0.92 / seconds) as i64 - audio_kbps).max(200));
        let rate = |k: i64| ["-b:v".to_string(), format!("{k}k"), "-maxrate".into(), format!("{}k", k * 3 / 2), "-bufsize".into(), format!("{}k", k * 2)];
        if kind == "mp4" {
            args.extend(["-c:v", "libx264", "-preset", "slow", "-pix_fmt", "yuv420p"].map(String::from));
            match video_kbps {
                Some(k) => args.extend(rate(k)),
                // Hardly to be told from 18 (the sand's grain), half the size.
                None => args.extend(["-crf", "23"].map(String::from)),
            }
            args.extend(["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"].map(String::from));
        } else {
            args.extend(["-c:v", "libvpx-vp9", "-row-mt", "1", "-pix_fmt", "yuv420p"].map(String::from));
            match video_kbps {
                Some(k) => args.extend(rate(k)),
                None => args.extend(["-crf", "30", "-b:v", "0"].map(String::from)),
            }
            args.extend(["-c:a", "libopus", "-b:a", "160k"].map(String::from));
        }
        args.extend(["-shortest".to_string(), out.display().to_string()]);
        match std::process::Command::new("ffmpeg").args(&args).stdin(std::process::Stdio::piped()).spawn() {
            Ok(mut child) => {
                let stdin = BufWriter::new(child.stdin.take().context("ffmpeg's input")?);
                Ok(Sink::Ffmpeg { child, stdin })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let dir = out.with_extension("frames");
                std::fs::create_dir_all(&dir)?;
                let sound = dir.join("sound.wav");
                std::fs::copy(wav, &sound)?;
                let mut command = format!("ffmpeg -framerate {fps} -i {}/%06d.png -i {}", dir.display(), sound.display());
                for a in args.iter().skip_while(|a| *a != "-c:v") {
                    command.push(' ');
                    command.push_str(a);
                }
                Ok(Sink::Pngs { dir, frames: 0, command, width, height })
            }
            Err(e) => Err(e).context("starting ffmpeg"),
        }
    }

    fn push(&mut self, mut rgba: Vec<u8>, fps: u32) -> Result<()> {
        match self {
            Sink::Gif { encoder, frames, width, height } => {
                let mut frame = gif::Frame::from_rgba_speed(*width, *height, &mut rgba, 10);
                frame.delay = gif_delay(*frames, fps);
                encoder.write_frame(&frame).context("writing the GIF")?;
                *frames += 1;
            }
            Sink::Ffmpeg { stdin, .. } => stdin.write_all(&rgba).context("sending a frame to ffmpeg")?,
            Sink::Pngs { dir, frames, width, height, .. } => {
                *frames += 1;
                write_png(&dir.join(format!("{frames:06}.png")), *width, *height, &rgba)?;
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<()> {
        match self {
            Sink::Gif { .. } => {}
            Sink::Ffmpeg { mut child, stdin } => {
                drop(stdin.into_inner().map_err(|e| e.into_error()).context("sending the last frames to ffmpeg")?);
                let status = child.wait()?;
                if !status.success() {
                    bail!("ffmpeg failed ({status})");
                }
            }
            Sink::Pngs { command, .. } => println!("no ffmpeg found: frames and sound written; join them with\n  {command}"),
        }
        Ok(())
    }
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// A texture (RGBA, 8 bits a channel) read back from the GPU, row after
/// row (the IDE's game picture for the agent).
pub(crate) fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Result<Vec<u8>> {
    let (width, height) = (texture.width(), texture.height());
    let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("read back"), size: (row * height) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.expect("mapping the read-back buffer"));
    device.poll(wgpu::PollType::wait_indefinitely()).context("waiting for the GPU")?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    {
        let data = slice.get_mapped_range().context("reading back the texture")?;
        for y in 0..height as usize {
            let start = y * row as usize;
            pixels.extend_from_slice(&data[start..start + width as usize * 4]);
        }
    }
    buffer.unmap();
    Ok(pixels)
}

/// An RGBA picture as PNG bytes.
pub(crate) fn png_bytes(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(rgba)?;
    }
    Ok(bytes)
}

/// What `sheet` draws.
pub struct SheetOptions {
    /// One of the game's models: its name or its file.
    pub model: String,
    pub out: PathBuf,
}

/// The sides a model sheet shows, each a quarter turn (the way it faces,
/// x east, y north) turned on by degrees: front, front three-quarter,
/// side, back three-quarter, back.
const SIDES: [((i32, i32), f32); 5] = [((0, -1), 0.0), ((0, -1), -45.0), ((1, 0), 0.0), ((1, 0), -45.0), ((0, 1), 0.0)];

/// A model sheet: one of the game's models alone as the game draws it (its
/// light, camera angle and shading), in one picture, row after row: five
/// sides close up (front, front three-quarter, side, back three-quarter,
/// back), every frame from the side (a figure of parts a kind swings: its
/// walk in eight steps), and the five sides at the size the game shows
/// them (a voxel as many pixels as in a 1920 by 1080 window).
pub fn sheet<G: Game>(dir: &Path, opts: &SheetOptions) -> Result<()> {
    let game = Session::<G>::load(dir)?;
    let m = game.models.named(&opts.model).with_context(|| format!("no model '{}' in the game (a kind's model, or one in c.models)", opts.model))?;
    let (size, count) = (m.size.as_vec3(), m.frames.len());
    let colours: std::collections::BTreeSet<[u8; 4]> = m.frames.iter().flatten().map(|(_, c)| *c).collect();
    // A cell's view: the model and some floor, a voxel the same size in
    // every cell of a row.
    let close = |w: u32, h: u32| {
        let aspect = w as f32 / h as f32;
        (size.z * 1.15 * aspect).max(size.x.max(size.y) * 1.6)
    };
    let per_voxel = 1920.0 / crate::camera::shown_width(&game.cfg.camera, 16.0 / 9.0);
    let small = ((size.x.max(size.y) * 1.6).max(size.z * 1.3) * per_voxel).ceil().max(16.0) as u32;
    // What the GPU cannot hold is said before anything is drawn: the row
    // at the game's size as one picture, the floor under the model as
    // cubes (the device asks for wgpu's default limits).
    let limits = wgpu::Limits::default();
    let across = m.size.x.max(m.size.y) as u64 + 8;
    let floor_bytes = across * across * std::mem::size_of::<crate::render::Instance>() as u64;
    if small > limits.max_texture_dimension_2d || floor_bytes > limits.max_buffer_size {
        bail!(
            "model '{}' is {} by {} by {} voxels, too big for a sheet: at the game's size ({per_voxel:.1} pixels a voxel) one side takes {small} pixels, a picture holds {} at most",
            opts.model,
            m.size.x,
            m.size.y,
            m.size.z,
            limits.max_texture_dimension_2d
        );
    }
    let mut rows: Vec<(u32, u32, Vec<Vec<u8>>)> = Vec::new();
    // A figure of parts a kind swings: its walk, a stride every half its
    // height, in eight steps through two strides.
    let swing = crate::sim::things::defs(&game.cfg, &G::behaviors(&game.cfg))
        .ok()
        .and_then(|defs| defs.into_iter().find(|d| d.model.as_deref() == Some(opts.model.as_str()) && !d.swing.is_empty()).map(|d| d.swing))
        .filter(|_| !m.parts.is_empty());
    let mut row = |w: u32, h: u32, width: f32, cells: &[(usize, glam::IVec2, f32, f32)]| -> Result<()> {
        let mut canvas = Canvas::new(w, h, false).with_context(|| format!("model '{}' is {} by {} by {} voxels, too big for a sheet", opts.model, m.size.x, m.size.y, m.size.z))?;
        let mut drawn = Vec::new();
        for &(frame, look, turn, walked) in cells {
            let mut body = crate::sim::Body::new(glam::Vec3::ZERO, size.z);
            body.walked = walked;
            let none = std::collections::BTreeMap::new();
            let swung = crate::session::swinging(swing.as_ref().unwrap_or(&none), &body, size.z);
            let still = game.still(&opts.model, frame, look, crate::session::turning(turn, 0.0, 0.0), &swung, width, w as f32 / h as f32).expect("the model is there");
            drawn.push(canvas.draw(&still)?);
        }
        rows.push((w, h, drawn));
        Ok(())
    };
    let sides: Vec<_> = SIDES.iter().map(|&((x, y), turn)| (0, glam::IVec2::new(x, y), turn, 0.0)).collect();
    row(360, 480, close(360, 480), &sides)?;
    let frames: Vec<_> = match swing {
        Some(_) => (0..8).map(|k| (0, glam::IVec2::X, 0.0, k as f32 * size.z / 8.0)).collect(),
        None => (0..count).map(|f| (f, glam::IVec2::X, 0.0, 0.0)).collect(),
    };
    row(240, 320, close(240, 320), &frames)?;
    row(small, small, small as f32 / per_voxel, &sides)?;
    // The rows one under another, a gap between, on the editor's grey.
    let gap = 8;
    let width = rows.iter().map(|(w, _, cells)| (w + gap) * cells.len() as u32).max().unwrap_or(1) + gap;
    let height = rows.iter().map(|(_, h, _)| h + gap).sum::<u32>() + gap;
    let mut sheet = [30u8, 30, 36, 255].repeat((width * height) as usize);
    let mut top = gap;
    for (w, h, cells) in &rows {
        for (i, pixels) in cells.iter().enumerate() {
            let left = gap + i as u32 * (w + gap);
            for y in 0..*h {
                let from = (y * w * 4) as usize;
                let to = (((top + y) * width + left) * 4) as usize;
                sheet[to..to + (w * 4) as usize].copy_from_slice(&pixels[from..from + (w * 4) as usize]);
            }
        }
        top += h + gap;
    }
    write_png(&opts.out, width, height, &sheet)?;
    println!(
        "{}: {} by {} by {} voxels, {count} frames, {} colors | rows: front, front 3/4, side, back 3/4, back close up; {}; the five sides at the game's size ({per_voxel:.1} pixels a voxel)",
        opts.out.display(),
        m.size.x,
        m.size.y,
        m.size.z,
        colours.len(),
        if m.parts.is_empty() { "every frame from the side".to_string() } else if frames.len() == 8 { format!("{} parts, its walk from the side in eight steps", m.parts.len()) } else { format!("{} parts (no kind swings them)", m.parts.len()) }
    );
    Ok(())
}

pub(crate) fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}

/// Writes the game's sounds, every event one after another with a pause
/// between (or one), as a 16-bit WAV file, and lists them with their times.
pub fn sounds<G: Game>(dir: &Path, out: &Path, only: Option<&str>) -> Result<()> {
    use crate::audio::{SAMPLE_RATE, SoundBank};

    let cfg = crate::session::config::<G>(dir)?;
    let brought = G::sounds(&cfg);
    let bank = SoundBank::load(&crate::files::Files::folder(dir), &cfg.sounds, &brought)?;
    let names: Vec<&str> = crate::audio::sound::all(&brought, &cfg.sounds).into_iter().map(|(n, ..)| n).collect();
    let events: Vec<&str> = match only {
        Some(name) => vec![*names.iter().find(|n| **n == name).with_context(|| format!("no sound called '{name}' (the game's: {})", names.join(", ")))?],
        None => names.clone(),
    };
    let pause = SAMPLE_RATE as usize / 2;
    let mut all: Vec<f32> = Vec::new();
    for name in events {
        let start = all.len() as f32 / SAMPLE_RATE as f32;
        match bank.get(name) {
            Some(samples) => {
                println!("{start:6.2}s  {name:<11} {:.2}s", samples.len() as f32 / SAMPLE_RATE as f32);
                all.extend(samples.iter().map(|s| s * cfg.audio.volume));
            }
            None => println!("{start:6.2}s  {name:<11} (silent)"),
        }
        all.extend(std::iter::repeat_n(0.0, pause));
    }
    let (limited, said) = limit(&all, 1, cfg.audio.drive);
    write_wav(out, &limited, 1)?;
    println!("wrote {} ({:.1}s; {said})", out.display(), all.len() as f32 / SAMPLE_RATE as f32);
    Ok(())
}

/// Lists the game's sounds: each one's name, where it comes from (the
/// game's file, or built in: the ready-made rules' or the engine's), how
/// long it lasts and its peak (dBFS, before the game's volume).
pub fn sound_list<G: Game>(dir: &Path) -> Result<()> {
    use crate::audio::{SAMPLE_RATE, SoundBank};

    let cfg = crate::session::config::<G>(dir)?;
    let brought = G::sounds(&cfg);
    let bank = SoundBank::load(&crate::files::Files::folder(dir), &cfg.sounds, &brought)?;
    let core = crate::audio::CORE_FILES;
    for (name, _, file) in crate::audio::sound::all(&brought, &cfg.sounds) {
        let from = match file {
            Some(file) => file,
            None if core.iter().any(|(n, _)| *n == name) => "the engine's".to_string(),
            None => "the ready-made rules'".to_string(),
        };
        match bank.get(name) {
            Some(samples) => {
                let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                println!("{name:<13} {from:<24} {:5.2}s  peak {:6.1} dB", samples.len() as f32 / SAMPLE_RATE as f32, 20.0 * peak.max(1e-9).log10());
            }
            None => println!("{name:<13} {from:<24} silent"),
        }
    }
    Ok(())
}

/// Writes every sound of the game's `c.sounds` (in `conf.lua`, the way
/// sounds were made before they were files) as a file in `sounds/`, and
/// says which lines to take out of `conf.lua` (it changes nothing there:
/// the game is the maker's).
pub fn sounds_to_files<G: Game>(dir: &Path) -> Result<()> {
    use crate::audio::sound;

    let files = crate::files::Files::folder(dir);
    let mut cfg = crate::config::GameConfig::default();
    G::conf(&files, &mut cfg)?;
    let Some(table) = cfg.game.get("sounds") else {
        println!("conf.lua has no c.sounds: nothing to write");
        return Ok(());
    };
    let table = table.as_table().context("c.sounds: a table of sounds by name")?;
    let conf = files.read_to_string("conf.lua").unwrap_or_default();
    let lines: Vec<&str> = conf.lines().collect();
    let statements = sound_statements(&lines);
    std::fs::create_dir_all(dir.join(sound::FOLDER))?;
    for (name, value) in table {
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', ':']) {
            bail!("c.sounds[\"{name}\"]: a sound's name is its file's, sounds/NAME.toml: no / or : in it");
        }
        let (def, notes) = sound::from_conf(name, value)?;
        let comments = statements.iter().find(|s| s.name.as_deref() == Some(name.as_str())).map(|s| s.comments.clone()).unwrap_or_default();
        let file = format!("{}/{name}.toml", sound::FOLDER);
        let path = dir.join(&file);
        if path.exists() {
            println!("{file} is there already: left as it is (c.sounds.{name} may differ)");
            continue;
        }
        let text = sound::write(&def, &comments);
        std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
        println!("wrote {file}");
        for note in notes {
            println!("  {note}");
        }
        if let sound::SoundDef::Synth(s) = &def
            && s.wave == crate::audio::Wave::Square
            && s.duty != 0.5
        {
            println!("  duty {}: the engine now plays a pulse off 0.5 round zero, without the DC offset it had (the same tone, no thump)", s.duty);
        }
        if let Err(e) = sound::read(&file, &text) {
            println!("  it will not load as it is: {e:#}");
        }
    }
    if !statements.is_empty() {
        println!("Now take these lines out of conf.lua (the game will not load while it has c.sounds):");
        for s in &statements {
            for (n, line) in lines.iter().enumerate().take(s.to + 1).skip(s.from) {
                println!("  {:4}: {line}", n + 1);
            }
        }
    }
    Ok(())
}

/// A statement of `conf.lua` that sets `c.sounds` (lines `from` to `to`,
/// from 0), the comment lines just above it among them, and those
/// comments (and one at its end) to keep in the sound's file.
pub(crate) struct Statement {
    pub name: Option<String>,
    pub from: usize,
    pub to: usize,
    pub comments: Vec<String>,
}

pub(crate) fn sound_statements(lines: &[&str]) -> Vec<Statement> {
    let code = |l: &str| l.split("--").next().unwrap_or("").to_string();
    let mut found = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let c = code(lines[i]);
        let Some(at) = c.find("c.sounds") else {
            i += 1;
            continue;
        };
        let rest = &c[at + "c.sounds".len()..];
        let name = rest.strip_prefix('.').map(|r| r.chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect::<String>()).or_else(|| rest.strip_prefix("[\"").and_then(|r| r.split('"').next()).map(str::to_string)).filter(|n| !n.is_empty());
        let mut to = i;
        let mut depth = c.matches('{').count() as i64 - c.matches('}').count() as i64;
        while depth > 0 && to + 1 < lines.len() {
            to += 1;
            let c = code(lines[to]);
            depth += c.matches('{').count() as i64 - c.matches('}').count() as i64;
        }
        let mut from = i;
        while from > 0 && lines[from - 1].trim_start().starts_with("--") {
            from -= 1;
        }
        let mut comments: Vec<String> = lines[from..i].iter().map(|l| l.trim_start().trim_start_matches('-').trim().to_string()).collect();
        if let Some((_, tail)) = lines[to].split_once("--") {
            comments.push(tail.trim().to_string());
        }
        found.push(Statement { name, from, to, comments });
        i = to + 1;
    }
    found
}

/// Writes the game's music as a stereo WAV file: once through, or for
/// `seconds` (looping as the game would).
pub fn music<G: Game>(dir: &Path, out: &Path, seconds: Option<f32>) -> Result<()> {
    let game = Session::<G>::load(dir)?;
    let file = game.song_file().context("conf.lua names no music (c.audio.music)")?.to_string();
    song(&dir.join(&file), Some((game.cfg.audio.music_volume, game.cfg.audio.drive)), out, seconds)
}

/// Writes one song file as a stereo WAV, at a game's music volume and
/// drive (the engine's defaults without a game).
pub fn song(file: &Path, game: Option<(f32, f32)>, out: &Path, seconds: Option<f32>) -> Result<()> {
    use crate::audio::SAMPLE_RATE;

    let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let song = crate::audio::music::parse(&text).with_context(|| format!("in {}", file.display()))?;
    let length = song.length as f32 / SAMPLE_RATE as f32;
    let title = song.title.clone();
    let lengths = song.channel_lengths();
    if lengths.iter().any(|&(_, l)| (l - length).abs() > 0.01) {
        let list: Vec<String> = lengths.iter().map(|(c, l)| format!("{c} {l:.2}s")).collect();
        println!("warning: channels differ in length: {}", list.join(", "));
    }
    let defaults = crate::config::AudioConfig::default();
    let (volume, drive) = game.unwrap_or((defaults.music_volume, defaults.drive));
    let samples = crate::audio::music::Player::new(song, volume).render(seconds);
    let (limited, said) = limit(&samples, 2, drive);
    write_wav(out, &limited, 2)?;
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!(
        "wrote {} ({:.1}s; '{title}' is {length:.1}s long, peak {peak:.2}; {said})",
        out.display(),
        samples.len() as f32 / 2.0 / SAMPLE_RATE as f32
    );
    Ok(())
}

/// A recording through the mixer's limiter at the game's drive, on time
/// (`limiter::limit_whole`): what the speaker plays. With what the limiter
/// did and how loud it is.
fn limit(samples: &[f32], channels: usize, drive: f32) -> (Vec<f32>, String) {
    use crate::audio::{SAMPLE_RATE, limiter, loudness};

    let mut lim = limiter::Limiter::with(SAMPLE_RATE, limiter::Options { drive_db: f64::from(drive), ..limiter::Options::default() });
    let limited = limiter::limit_whole(&mut lim, samples, channels);
    let t = lim.read_telemetry();
    let limiting = if t.duty_cycle > 0.0 { format!("limited {:.1} dB at most, {:.1} % of the time", -t.gain_reduction_db, t.duty_cycle * 100.0) } else { "never limited".into() };
    let lufs = loudness::integrated_lufs(&limited, channels, SAMPLE_RATE);
    (limited, format!("{limiting}; {lufs:.1} LUFS"))
}

/// 16-bit PCM. The sound has been through the limiter (the mixer's, or
/// `limit`'s), so nothing reaches full scale; clamped all the same.
fn write_wav(out: &Path, samples: &[f32], channels: u16) -> Result<()> {
    use crate::audio::SAMPLE_RATE;

    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let data: Vec<u8> = samples.iter().flat_map(|&s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut wav = b"RIFF".to_vec();
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(channels.to_le_bytes());
    wav.extend(SAMPLE_RATE.to_le_bytes());
    wav.extend((SAMPLE_RATE * 2 * channels as u32).to_le_bytes());
    wav.extend((2 * channels).to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(out, wav).with_context(|| format!("writing {}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `inspect` draws after every tick as a window and `check` do, so a
    /// mistake in `on_draw` halts the game there too (it said "halted":
    /// null of a game `shot` stopped); and it says, as a warning, a local
    /// of the game's files its functions change, the run going on.
    #[test]
    fn inspect_draws_and_names_a_hidden_local() {
        let game = |name: &str, main: &str| {
            let dir = crate::scratch::Scratch::new(name);
            std::fs::write(dir.join("conf.lua"), "function conf(c) c.world.size = { 16, 16, 8 } end\n").unwrap();
            std::fs::write(dir.join("main.lua"), main).unwrap();
            dir
        };
        let begin = crate::session::Begin::default();
        let dir = game("inspect-draws", "function on_draw(frame)\n  world.state.x = 1\nend\n");
        let standing = inspected::<crate::Voxcade>(&dir, 5, "", &[], None, &begin).unwrap();
        assert!(standing["halted"].as_str().is_some_and(|h| h.starts_with("main.lua:2: only to look at here")), "{standing}");
        // The command fails with it, as `shot` does (its JSON printed first).
        let e = inspect::<crate::Voxcade>(&dir, 5, "", &[], None, &begin).unwrap_err().to_string();
        assert!(e.starts_with("the game stopped at tick") && e.contains("main.lua:2: only to look at here"), "{e}");
        let dir = game("inspect-hidden", "local n = 0\nfunction on_tick() n = n + 1 end\n");
        let standing = inspected::<crate::Voxcade>(&dir, 130, "", &[], None, &begin).unwrap();
        assert!(standing["halted"].is_null(), "{standing}");
        assert_eq!(standing["warnings"], serde_json::json!([crate::lua::hidden::said("n")]));
        let dir = game("inspect-quiet", "function on_tick() world.state.n = (world.state.n or 0) + 1 end\n");
        let standing = inspected::<crate::Voxcade>(&dir, 130, "", &[], None, &begin).unwrap();
        assert_eq!(standing["warnings"], serde_json::json!([]));
    }

    /// A GIF's frame times add up to the frame rate: a second of frames
    /// lasts a second.
    #[test]
    fn gif_frames_last_as_long_as_the_frame_rate_says() {
        for fps in [10, 12, 15, 20, 30, 60] {
            let total: u64 = (0..u64::from(fps)).map(|n| u64::from(gif_delay(n, fps))).sum();
            assert_eq!(total, 100, "{fps} fps");
        }
        assert_eq!((0..6).map(|n| gif_delay(n, 30)).collect::<Vec<_>>(), [3, 4, 3, 3, 4, 3]);
    }

    /// A game of its own picture shape (`c.camera.aspect`) is drawn into
    /// the largest rectangle of it in the window, black bars on the other
    /// two sides, exactly the rectangle's pixels; across a wide window the
    /// bars stand left and right. Unset, the picture fills the window as
    /// before, the same pixel for pixel.
    #[test]
    fn a_picture_of_its_own_shape_has_bars() {
        let game = |name: &str, aspect: &str| {
            let dir = crate::scratch::Scratch::new(name);
            std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.size = {{ 64, 64, 8 }}\n  c.light.sky = {{ 0.3, 0.4, 0.6 }}\n  {aspect}\nend\n")).unwrap();
            std::fs::write(dir.join("main.lua"), "function on_level_start() level:fill(0, 0, 0, 63, 63, 0, '#5a6e4a') end\nfunction on_draw(frame) frame:box(0, 0, frame.width, 4, { 255, 255, 255 }) end\n").unwrap();
            dir
        };
        let picture = |dir: &Path, width: u32, height: u32, shape: Option<f32>| {
            let mut game = Session::<crate::Voxcade>::load(dir).unwrap();
            game.step(&crate::input::Input::default());
            let mut canvas = Canvas::new(width, height, false).unwrap();
            canvas.mesh_all(&mut game);
            canvas.renderer.set_shape(&canvas.device, shape);
            let frame = game.menu_frame(&Menu::default(), &Settings::default(), 1.0, width as f32 / height as f32, Scene::Play);
            canvas.draw(&frame).unwrap()
        };
        let black_rows = |pixels: &[u8], width: u32, rows: std::ops::Range<u32>| rows.clone().all(|y| (0..width).all(|x| pixels[((y * width + x) * 4) as usize..][..3] == [0, 0, 0]));
        let lit_row = |pixels: &[u8], width: u32, y: u32| (0..width).any(|x| pixels[((y * width + x) * 4) as usize..][..3] != [0, 0, 0]);
        let film = game("bars", "c.camera.aspect = 2.39");
        // 320 by 180: 320 / 2.39 = 134 rows high, 23 above and below.
        assert_eq!(crate::render::letterbox(320, 180, 2.39), (0, 23, 320, 134));
        let pixels = picture(&film, 320, 180, Some(2.39));
        assert!(black_rows(&pixels, 320, 0..23) && black_rows(&pixels, 320, 157..180), "black bars above and below");
        assert!(lit_row(&pixels, 320, 23) && lit_row(&pixels, 320, 156), "the picture fills its rectangle");
        // The HUD lies inside it: its white band at the rectangle's top.
        assert!(pixels[((24 * 320 + 160) * 4) as usize..][..3].iter().all(|&c| c > 200), "the HUD's white band");
        // A window wider than the picture: bars left and right.
        assert_eq!(crate::render::letterbox(400, 100, 2.39), (80, 0, 239, 100));
        let wide = picture(&film, 400, 100, Some(2.39));
        let column_black = |x: u32| (0..100).all(|y| wide[((y * 400 + x) * 4) as usize..][..3] == [0, 0, 0]);
        assert!((0..80).all(column_black) && (319..400).all(column_black) && !column_black(80) && !column_black(318));
        // Unset: the window's shape, as without the setting.
        let plain = game("no-bars", "");
        assert_eq!(picture(&plain, 320, 180, None), picture(&plain, 320, 180, Session::<crate::Voxcade>::load(&plain).unwrap().cfg.camera.aspect));
        assert!(lit_row(&picture(&plain, 320, 180, None), 320, 0));
    }
}

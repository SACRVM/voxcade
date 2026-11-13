//! The windowed game: SDL3 for window and input, wgpu for drawing, a fixed
//! 60 Hz simulation with interpolated rendering, and hot reload of the
//! game folder.
//!
//! `run` opens the window and makes an `App`, then each frame goes
//! through its steps in order: the window's events (`events.rs`), the
//! network's news (`network.rs`), what the menus asked for
//! (`commands.rs`), the title's replay ended, hot reload, music, the
//! devices' keys, the title's round, the ticks, a guest's ticks, the
//! network sent, the picture (`draw.rs`). The front end's state, the
//! title, the glide in, the game, attract mode, is `front::Front`; what
//! keys mean before the game hears them is `keys.rs`.

mod commands;
mod draw;
mod events;
mod front;
mod keys;
mod network;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use sdl3::gamepad::Gamepad;
use sdl3::keyboard::Scancode;

use crate::controls::Device;
use crate::game::Game;
use crate::menu::{Command, Menu, Page};
use crate::render::Renderer;
use crate::replay::replay_session;
use crate::replay::{Attract, Playback, Replay, Runs};
use crate::saves::Slots;
use crate::session::{Reloaded, Session};
use crate::settings::{self, Settings};
use crate::sim::DT;
use crate::watch::FolderWatch;
use front::{Front, Idle};

/// Simulation ticks allowed per rendered frame before we drop time.
const MAX_CATCH_UP: u32 = 5;
/// How loud the game's sounds are under the title music while the title
/// plays a replay (attract mode).
const ATTRACT_SOUNDS: f32 = 1.0 / 3.0;
/// How long a frame plays the run so far through while a guest catches up:
/// most of it (only the progress is drawn meanwhile), so an hour of play
/// takes about 50 s here.
const CATCH_UP_BUDGET: Duration = Duration::from_millis(40);

/// Whether the window's loop goes on after a step.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Go,
    Quit,
}

/// The frame's clock: when the last frame was, the simulation's time owed,
/// and the frames counted for the frame rate shown.
struct Clock {
    last: Instant,
    accumulator: f32,
    fps_frames: u32,
    fps_since: Instant,
    fps: f32,
}

/// Everything the window's loop holds, between frames. (Its fields drop
/// in this order, the window's and the GPU's last.)
struct App<G: Game> {
    clock: Clock,
    watch: FolderWatch,
    now_playing: String,
    /// The console (the key left of 1): a line of Lua between two ticks,
    /// the game standing still while it is open.
    prompt: crate::prompt::Prompt,
    text_input: sdl3::keyboard::TextInputUtil,
    /// The search for games on the LAN while JOIN lists them, and the list
    /// of games on the internet from our matchmaker, likewise.
    finder: Option<crate::network::Finder>,
    net_finder: Option<crate::network::NetFinder>,
    /// A guest joins again by itself once after its game ran apart.
    rejoined: bool,
    /// Player 1's keys from a script (`--input`), a step every tick's time,
    /// instead of the devices: a game tried hands-free.
    scripted: Option<(crate::replay::Script, f32, crate::input::Input)>,
    /// The hosted game's code last put on the clipboard.
    copied_code: Option<String>,
    pacer: crate::window::Pacer,
    /// A line said on the screen for a few seconds (a network game left,
    /// a failure), and since when.
    parting: Option<(String, Instant)>,
    /// What to do first in the next frame (the command line's --host and
    /// --join, a ticket pasted).
    queued: Vec<Command>,
    /// A network game (`docs/multiplayer.md`, stage 4), and the port asked
    /// to host on.
    net: Option<crate::network::Network<G>>,
    host_port: Option<u16>,
    front: Front,
    /// The title's attract mode: the game's replays in turn.
    attract: Attract,
    /// A replay being watched or played by the title gives the input
    /// instead of the keys.
    playback: Option<Playback>,
    /// Every run is recorded.
    runs: Runs,
    /// Who plays on which seat (`c.players`): the pad pressed last before
    /// a run began is seat 1's, with the keyboard.
    last_pad: Option<u32>,
    seating: crate::seating::Seating,
    /// Gamepads: every one plugged in now or later drives the game.
    pads: crate::pads::Pads,
    settings: Settings,
    settings_path: PathBuf,
    /// Sound is welcome, not required: without an audio device the game
    /// runs silent.
    speaker: Option<crate::audio::Speaker>,
    mask: crate::mesh::FaceMask,
    renderer: Renderer,
    adapter: wgpu::Adapter,
    screen: crate::window::Screen,
    video: sdl3::VideoSubsystem,
    game: Session<G>,
    dir: PathBuf,
    /// A replay shown instead of a game played (`voxcade replay`).
    watching: Option<Replay>,
    /// The scenario and lines every run begins with (`--scenario`, `--do`).
    begin: crate::session::Begin,
    /// The maker's keys work (`voxcade run --maker`).
    maker: bool,
}

/// Runs the game folder `dir`, a game of kind `G`, in a window; with
/// `watching`, shows that replay instead; with `network`, hosts a network
/// game at once or joins one (`--host`, `--join`).
///
/// `maker`: the maker's keys work (`voxcade run --maker`: F5 restarts the
/// run on the files as they are, F9 keeps it, 1 to 9 pick the song); a
/// player of a packaged game, or of the launcher's PLAY, has none of them.
pub fn run<G: Game>(dir: &Path, watching: Option<Replay>, begin: crate::session::Begin, network: Option<crate::headless::Networked>, keys: Option<&str>, maker: bool) -> Result<()> {
    // A replay watched plays on slots of its own, never the player's.
    let mut game = match &watching {
        Some(replay) => replay_session::<G>(dir, replay)?,
        None => Session::<G>::load_with(dir, Slots::on_disk(dir))?,
    };

    let sdl = sdl3::init().context("initializing SDL")?;
    let video = sdl.video().context("initializing SDL video")?;
    let mut screen = crate::window::Screen::open(&video, &format!("voxcade - {}", game.cfg.title), 1600, 900, true)?;
    // Shadows start coarser on the processor's own graphics (`Settings::load_for`).
    let adapter = screen.gpu().adapter().clone();
    let integrated = adapter.get_info().device_type == wgpu::DeviceType::IntegratedGpu;
    let crate::window::Screen { surface, device, queue, config, window, .. } = &mut screen;
    let (format, width, height) = (config.format, config.width, config.height);

    let mut renderer = Renderer::new(device, format, width, height);
    let mask = game.face_mask();
    renderer.rebuild_all(device, queue, game.world_mut(), &mask);

    // Sound is welcome, not required: without an audio device the game runs silent.
    let mut speaker = crate::audio::speaker(&sdl);

    let settings_path = settings::path_for(dir);
    let (mut settings, settings_trouble) = Settings::load_for(&settings_path, integrated);
    settings.adopt(game.actions(), &game.default_keys(), &game.default_pad());
    // The window's player speaks the system's language where the game does.
    settings.system_languages = settings::system_languages();
    if settings.adopt_scores(&game.cfg.high_scores) && let Err(e) = settings.save(&settings_path) {
        eprintln!("settings: {e:#}");
    }
    crate::controls::label_keys(&mut settings);
    for name in settings.keys.names() {
        if Scancode::from_name(name).is_none() {
            eprintln!("settings: no key named '{name}'");
        }
    }
    apply_settings(&mut game, &mut settings);
    crate::window::set_frame_rate(surface, device, config, &adapter, settings.frame_rate);
    if settings.fullscreen {
        window.set_fullscreen(true).context("fullscreen")?;
    }

    let pads = crate::pads::Pads::new(&sdl);

    // The front end: the title (its menu, its scene's clock), or the game,
    // paused while the menu is open.
    let mut front = Front::new(title_menu(&game));
    let seating = crate::seating::Seating::new(game.cfg.players);
    if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
        mixer.set_music(game.title_music());
    }

    // Every run is recorded (`Runs`); a replay being watched gives the input
    // instead of the keys.
    let mut runs = Runs::new(dir);
    let mut playback = None;
    let attract = Attract::new(dir);
    if let Some(replay) = &watching {
        playback = Some(Playback::new(replay, &mut game)?);
        // Told once it began as the run did (a scenario's file counts).
        if !replay.fits(game.fingerprint()?) {
            eprintln!("the replay was recorded on other game data: it will run apart");
        }
        front.straight_in();
        play_the_games_song(&mut game, &settings);
    } else if !begin.is_empty() {
        // A scenario to try: past the title, straight into it.
        begun(&begin, &mut game);
        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
        runs.start(&game);
        front.straight_in();
        play_the_games_song(&mut game, &settings);
    }

    // A network game, what to do first (the command line's --host and
    // --join).
    let mut host_port = None;
    let mut queued = Vec::new();
    match network {
        Some(crate::headless::Networked::Host(port)) => {
            host_port = (port != 0).then_some(port);
            queued.push(Command::Host);
        }
        Some(crate::headless::Networked::HostNet) => queued.push(Command::HostNet),
        Some(crate::headless::Networked::Join(address)) => {
            front.menu.address = address;
            front.menu.stack.push((Page::Joining, 0));
            queued.push(Command::Join);
        }
        None => {}
    }
    // What went wrong before the first picture (settings or saves that would
    // not read) is said on the screen too, as a network's trouble is.
    let mut parting: Option<(String, Instant)> = None;
    {
        let lang = settings.language();
        let mut said = Vec::new();
        if settings_trouble.is_some() {
            said.push(crate::lang::say(&lang, "SETTINGS UNREADABLE, THE OLD FILE IS KEPT AS .BROKEN").to_string());
        }
        for slot in game.slots.unreadable() {
            said.push(crate::lang::say_with(&lang, "SAVE {} CANNOT BE READ", &[&slot.to_string()]));
        }
        if !said.is_empty() {
            parting = Some((said.join("  "), Instant::now()));
        }
    }
    let scripted = keys.map(|k| crate::replay::parse_script(k, game.actions()).map(crate::replay::Script::new)).transpose()?.map(|s| (s, 0.0f32, crate::input::Input::default()));
    let text_input = video.text_input();
    let watch = FolderWatch::new(dir);

    let mut app = App {
        clock: Clock { last: Instant::now(), accumulator: 0.0, fps_frames: 0, fps_since: Instant::now(), fps: 0.0 },
        watch,
        now_playing: String::new(),
        prompt: crate::prompt::Prompt::default(),
        text_input,
        finder: None,
        net_finder: None,
        rejoined: false,
        scripted,
        copied_code: None,
        pacer: crate::window::Pacer::default(),
        parting,
        queued,
        net: None,
        host_port,
        front,
        attract,
        playback,
        runs,
        last_pad: None,
        seating,
        pads,
        settings,
        settings_path,
        speaker,
        mask,
        renderer,
        adapter,
        screen,
        video: video.clone(),
        game,
        dir: dir.to_path_buf(),
        watching,
        begin,
        maker,
    };
    // (Made after the app, so it goes first at the end, as it always did.)
    let mut events = sdl.event_pump().context("SDL event pump")?;

    loop {
        let mut commands = std::mem::take(&mut app.queued);
        let phase_over = app.game.game.outcome().is_some();
        if events.poll_iter().any(|event| app.event(event, phase_over, &mut commands) == Flow::Quit) {
            break;
        }
        app.prompt.settle();
        app.pads.settle();
        app.network(&mut commands);
        if app.commands(commands)? == Flow::Quit {
            break;
        }
        app.attract_ended();
        app.hot_reload();
        app.music();
        let keyboard = events.keyboard_state();
        let input = app.read_input(&keyboard);
        app.net_input(&keyboard);
        let elapsed = app.elapsed();
        let input = app.scripted(elapsed, input);
        app.title_round(elapsed);
        app.ticks(input);
        app.guest_ticks();
        app.net_send();
        app.draw()?;
        app.pace();
    }
    app.end();
    // The sound closed apart, and SDL not quit: its audio may hang in a
    // sound server gone away, and the window's end must not wait for it
    // (the program's end takes everything).
    drop(app.speaker.take());
    std::mem::forget(sdl);
    Ok(())
}

impl<G: Game> App<G> {
    /// The game's world built anew on the GPU with the faces it shows now.
    fn remesh(&mut self) {
        self.mask = self.game.face_mask();
        self.renderer.rebuild_all(&self.screen.device, &self.screen.queue, self.game.world_mut(), &self.mask);
    }

    /// `fresh` in place of the game, the player's settings on it, its world
    /// built.
    fn renew(&mut self, fresh: Session<G>) {
        self.game = fresh;
        apply_settings(&mut self.game, &mut self.settings);
        self.remesh();
    }

    /// The game loaded afresh from its folder, in place of the one there;
    /// a game that does not load leaves it, said.
    fn reload_game(&mut self) {
        match Session::<G>::load_with(&self.dir, Slots::on_disk(&self.dir)) {
            Ok(fresh) => self.renew(fresh),
            Err(e) => eprintln!("reloading the game failed: {e:#}"),
        }
    }

    /// The settings written; a failure said on the screen too.
    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.settings_path) {
            eprintln!("settings: {e:#}");
            self.parting = Some(trouble(&self.settings.language(), "SETTINGS NOT SAVED: {}", &e));
        }
    }

    /// The seats as a run begins: the plan in the settings, the pad pressed
    /// last seat 1's.
    fn seat_the_players(&mut self) {
        self.seating = crate::seating::Seating::new(self.game.cfg.players);
        self.seating.begin(self.last_pad.filter(|_| self.front.played_with == Device::Gamepad), &crate::seating::resolve(&self.settings.players, &self.pads.named()));
    }

    /// The title's replay ends at any key, at its end, or where it runs
    /// apart: back to the title, its music playing on.
    fn attract_ended(&mut self) {
        if self.front.attracting && (std::mem::take(&mut self.front.leave_attract) || self.playback.as_ref().is_none_or(Playback::done)) {
            self.front.attracting = false;
            self.playback = None;
            self.reload_game();
            self.front.playing = false;
            self.front.menu = title_menu(&self.game);
            self.front.title_time = 0.0;
            self.attract.input();
        }
    }

    /// Hot reload: whatever changed in the game folder applied. Off in a
    /// network game: the guests have only the files it began with.
    fn hot_reload(&mut self) {
        let changed = self.watch.poll();
        if !changed.is_empty() && self.net.is_none() {
            let mut rebuild = false;
            for path in changed {
                match self.game.reload(&path) {
                    Ok(Reloaded::World) => rebuild = true,
                    Ok(Reloaded::Models) => println!("reloaded {}", path.display()),
                    Ok(Reloaded::Sounds) => println!("reloaded sounds"),
                    Ok(Reloaded::Music) => println!("reloaded music"),
                    Ok(Reloaded::Code) => println!("reloaded {}", path.display()),
                    Ok(Reloaded::Nothing) => {}
                    Err(e) => eprintln!("reload failed: {e:#}"),
                }
            }
            if rebuild {
                apply_settings(&mut self.game, &mut self.settings);
                self.remesh();
                println!("reloaded world");
            }
        }
    }

    /// Music: the title's on the title, the chosen song while the settings
    /// are open there; the game's (started over when it changed) while
    /// playing; the player's volume on top.
    fn music(&mut self) {
        let App { speaker, game, front, settings, prompt, now_playing, .. } = self;
        if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
            if front.title_music {
                game.music_changed = false;
                let last = game.cfg.audio.music.files().len().saturating_sub(1);
                let preview = front.menu.previews_song().then_some(settings.song.min(last));
                if preview != front.previewing {
                    front.previewing = preview;
                    mixer.set_music(match preview {
                        Some(song) => game.song_at(song),
                        None => game.title_music(),
                    });
                }
            } else if game.feed_music(&mut mixer, front.menu.is_open() || prompt.open) {
                // The player's own choice numbered when there are several.
                let choice = game.cfg.audio.music.files().len() > 1 && !game.steers_music();
                *now_playing = match mixer.music_title() {
                    Some(t) if choice => format!(" | \u{266a} {}: {t}", game.song + 1),
                    Some(t) => format!(" | \u{266a} {t}"),
                    None => String::new(),
                };
            }
            mixer.set_music_volume(game.cfg.audio.music_volume * settings.music_gain());
            mixer.set_drive(game.cfg.audio.drive);
        }
    }

    /// The keys held for the ticks to come: player 1's, and in a game of
    /// several each other seat's given to the game (a replay brings its
    /// own); the players page shows the seats.
    fn read_input(&mut self, keyboard: &sdl3::keyboard::KeyboardState) -> crate::input::Input {
        let App { game, pads, settings, seating, front, playback, net, .. } = self;
        // The players page shows the run's seats while playing, else the
        // plan (a game of several).
        if game.cfg.players > 1 {
            let named = pads.named();
            let shown = if front.playing { seating.clone() } else { planned(settings, &named, game.cfg.players) };
            front.menu.seats = crate::menu::SeatLine::all(&shown, &named);
        }
        if seating.alone() {
            crate::controls::read_input(Some(keyboard), &pads.open, settings, game.actions())
        } else {
            // Each seat its keys and pads; who sits there for the ticks to
            // come (a replay brings its own).
            let of_seat = |seat: usize| pads.open.iter().filter(|p| p.id().ok().and_then(|id| seating.pad_seat(id.raw())) == Some(seat)).collect::<Vec<_>>();
            for seat in 2..=seating.seats() {
                let input = crate::controls::read_seat(Some(keyboard), seating.keys(seat), &of_seat(seat), settings.pad_of(seat), settings, game.actions());
                if playback.is_none() && front.playing && net.is_none() {
                    let controls = match seating.keys(seat) {
                        crate::seating::Keys::Whole => crate::controls::Controls::of(game.actions(), Device::Keyboard, front.pad_kind, settings),
                        crate::seating::Keys::Nothing => crate::controls::Controls::of_pad(game.actions(), of_seat(seat).first().map_or(front.pad_kind, |p| keys::pad_style(p.r#type())), settings.pad_of(seat), settings),
                        half => crate::controls::Controls::of_half(game.actions(), half, settings),
                    };
                    game.set_seat(seat, seating.taken(seat).then_some(input));
                    game.set_seat_controls(seat, controls);
                }
            }
            crate::controls::read_seat(Some(keyboard), seating.keys(1), &of_seat(1), &settings.pad, settings, game.actions())
        }
    }

    /// The time since the last frame (at most a quarter of a second), owed
    /// to the simulation; the console slides, the title's clock runs.
    fn elapsed(&mut self) -> f32 {
        let now = Instant::now();
        let elapsed = (now - self.clock.last).as_secs_f32().min(0.25);
        self.clock.last = now;
        elapsed
    }

    /// The script's keys for player 1 (`--input`), a step every tick's
    /// time, in place of the devices' `input` while the game plays.
    fn scripted(&mut self, elapsed: f32, input: crate::input::Input) -> crate::input::Input {
        let App { scripted, front, net, game, settings, .. } = self;
        let input = match scripted {
            Some((script, owed, held)) if front.playing => {
                *owed += elapsed / DT;
                while *owed >= 1.0 {
                    *held = script.next_input();
                    *owed -= 1.0;
                }
                if let Some(n) = net
                    && let Some(seat) = n.locals.seat(1)
                {
                    let controls = game.player_controls(Device::Keyboard, front.pad_kind, settings);
                    n.read(seat, *held, controls);
                }
                *held
            }
            _ => input,
        };
        self.prompt.slide(elapsed);
        self.clock.accumulator += elapsed;
        input
    }

    /// The title's clock, its round (the high score table, a replay after
    /// a while without a key), the glide into the game; what the player
    /// plays with for the ticks to come.
    fn title_round(&mut self, elapsed: f32) {
        if !self.front.playing {
            self.front.title_time += elapsed;
        }
        // Nothing flies over while initials are typed or the table shows:
        // a flyby starts afresh after.
        if matches!(self.front.menu.page(), Some(Page::Initials | Page::HighScores)) || self.game.flyby_from > self.front.title_time {
            self.game.flyby_from = self.front.title_time;
        }
        if self.front.attracting {
            self.front.attract_time += elapsed;
        }
        // Long enough on the title without a key: it plays one of the
        // game's replays (on the game as its files are).
        let on_title = self.front.on_title();
        let waited = self.watching.is_none() && self.net.is_none() && on_title && self.attract.wait(elapsed, self.game.cfg.title_screen.attract_after);
        // First the high score table, then a replay.
        let replay = match self.front.idle(waited, self.game.game.scored()) {
            Idle::Replay => {
                let next = self.attract.next(self.game.fingerprint().unwrap_or(0));
                if next.is_none() {
                    self.attract.input();
                }
                next
            }
            Idle::Table | Idle::Wait => None,
        };
        // The table goes after a while: on to the replay in the title's
        // round, else to the title.
        let game = &self.game;
        if self.front.table_over(|| title_menu(game)) {
            self.attract.input();
        }
        // A replay that does not load (a save it began from no longer
        // reads) is passed over.
        let attract_game = replay.map(|replay| replay_session::<G>(&self.dir, &replay).and_then(|mut fresh| Playback::new(&replay, &mut fresh).map(|p| (fresh, p))));
        match attract_game {
            Some(Ok((fresh, p))) => {
                self.renew(fresh);
                self.playback = Some(p);
                self.front.attracting = true;
                self.front.attract_time = 0.0;
                self.front.playing = true;
                self.front.menu = Menu::default();
            }
            Some(Err(e)) => eprintln!("the title's replay: {e:#}"),
            None => {}
        }
        self.front.glide(elapsed);
        // What the player plays with, for the ticks to come (a replay
        // brings its own), and the time a save in them is stamped with.
        self.game.set_clock(crate::saves::now());
        if self.playback.is_none() && self.net.is_none() {
            let controls = match self.seating.keys(1) {
                half @ (crate::seating::Keys::Left | crate::seating::Keys::Right) if self.front.played_with == Device::Keyboard => crate::controls::Controls::of_half(self.game.actions(), half, &self.settings),
                // The keyboard another's: seat 1 plays on its pad.
                crate::seating::Keys::Nothing => self.game.player_controls(Device::Gamepad, self.front.pad_kind, &self.settings),
                _ => self.game.player_controls(self.front.played_with, self.front.pad_kind, &self.settings),
            };
            self.game.set_controls(controls);
        }
    }

    /// The simulation's ticks the time owes (at most `MAX_CATCH_UP`, the
    /// rest dropped): the game played with `input` or the replay's, the
    /// host's bundles built, the title's own ticks while it shows.
    fn ticks(&mut self, input: crate::input::Input) {
        let mut ticks = 0;
        while self.clock.accumulator >= DT && ticks < MAX_CATCH_UP {
            let App { front, playback, net, game, prompt, parting, speaker, settings, .. } = self;
            if front.menu.is_open() {
                front.menu.update();
            }
            let replay_over = playback.as_ref().is_some_and(Playback::done);
            let guest = net.as_ref().is_some_and(|n| !n.hosting());
            if guest {
                // A guest plays the host's bundles below; a level change's
                // fade passes here, frame by frame.
                if front.playing
                    && game.fading()
                    && let Some(crate::network::Side::Guest(j)) = net.as_mut().map(|n| &mut n.side)
                {
                    j.fade(game);
                }
            } else if front.playing && !front.menu.is_open() && !prompt.open && !replay_over {
                let input = match playback {
                    Some(p) => p.next_input(game),
                    None => input,
                };
                // Level changes fade where someone plays; replays cut.
                game.fades = playback.is_none();
                // The host builds the tick's bundle and plays it.
                let hosted = match net.as_mut().map(|n| &mut n.side) {
                    Some(crate::network::Side::Host(h)) => Some(h.tick(game)),
                    _ => {
                        game.step(&input);
                        None
                    }
                };
                match hosted {
                    // F5 or LOAD: a new run for everyone.
                    Some(Ok(true)) => {
                        self.remesh();
                        self.runs.start(&self.game);
                    }
                    Some(Err(e)) => {
                        eprintln!("the network game: {e:#}");
                        *parting = Some((format!("{e:#}").to_uppercase(), Instant::now()));
                        if let Some(mut n) = net.take() {
                            n.leave();
                        }
                    }
                    _ => {}
                }
                let App { front, playback, game, runs, speaker, settings, pads, watching, .. } = self;
                if front.pause_after_fade && !game.fading() {
                    front.pause_after_fade = false;
                    front.menu = pause_menu(game, watching.is_none());
                }
                match playback {
                    Some(p) => {
                        if let Err(apart) = p.check(game) {
                            if front.attracting {
                                front.leave_attract = true;
                            } else {
                                println!("the replay runs apart after {} s", apart.second);
                            }
                        }
                        if p.done() && !front.attracting {
                            println!("replay over: F5 plays it again, Escape quits");
                        }
                    }
                    None => runs.step(game),
                }
                if !game.game.sim().sounds.is_empty() {
                    if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
                        let quiet = if front.attracting { ATTRACT_SOUNDS } else { 1.0 };
                        game.play_sounds(&mut mixer, settings.sound_gain() * quiet);
                    }
                    if settings.rumble && !front.attracting {
                        rumble(&mut pads.open, game);
                    }
                }
            } else if !front.playing {
                front.title_tick += 1;
                if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
                    game.title_sounds(&mut mixer, front.title_tick, front.title_time, settings.sound_gain());
                }
            }
            self.clock.accumulator -= DT;
            ticks += 1;
        }
        if ticks == MAX_CATCH_UP {
            self.clock.accumulator = 0.0;
        }
    }

    /// The frame shown as soon as the frame rate allows, and the frame
    /// rate counted (with the game's line in the window's title).
    fn pace(&mut self) {
        self.pacer.wait(self.settings.frame_rate.limit());
        self.clock.fps_frames += 1;
        if self.clock.fps_since.elapsed() >= Duration::from_millis(500) {
            self.clock.fps = self.clock.fps_frames as f32 / self.clock.fps_since.elapsed().as_secs_f32();
            let fps = self.clock.fps;
            let s = self.renderer.stats;
            let (game, front) = (&self.game, &self.front);
            let title = match game.game.summary() {
                Some(hud) if front.playing => {
                    let replay = if self.watching.is_some() || front.attracting { " | replay" } else { "" };
                    format!("{}{replay} | {hud}{} | {fps:.0} fps", game.cfg.title, self.now_playing)
                }
                _ if !front.playing => format!("{} | {fps:.0} fps", game.cfg.title),
                _ => format!(
                    "voxcade - {} | {fps:.0} fps | chunks {}/{} | {}k verts | {} particles",
                    game.cfg.title,
                    s.drawn,
                    s.chunks,
                    s.vertices / 1000,
                    game.game.sim().particles.len()
                ),
            };
            let _ = self.screen.window.set_title(&title);
            self.clock.fps_frames = 0;
            self.clock.fps_since = Instant::now();
        }
    }

    /// The window closes: a network game left (its run kept), the run
    /// played ended.
    fn end(&mut self) {
        if let Some(n) = &mut self.net {
            n.keep_run(&self.game, &self.runs);
            n.leave();
        }
        if self.watching.is_none() && !self.front.attracting {
            self.runs.end(&self.game);
        }
    }
}

/// The game's own song (the one the settings chose) from the next tick on,
/// started over.
fn play_the_games_song<G: Game>(game: &mut Session<G>, settings: &Settings) {
    game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
    game.music_changed = true;
}

/// The title's menu: CONTINUE first when a save slot holds a save (the
/// newest).
fn title_menu<G: Game>(game: &Session<G>) -> Menu {
    let mut menu = Menu::title().continuing(game.slots.newest());
    menu.network = game.cfg.players > 1;
    menu.internet = crate::net::quic::matchmaker_known();
    menu
}

/// The initials page for the next of `lines` (whose, how many) that gets
/// into the table, the lines before it that do not dropped.
fn next_initials(lines: &mut Vec<(Option<usize>, u32)>, settings: &Settings) -> Option<Menu> {
    while !lines.is_empty() {
        let (seat, score) = lines.remove(0);
        if let Some(rank) = settings.rank(score) {
            let mut menu = Menu::initials(score, rank);
            menu.entry.seat = seat;
            return Some(menu);
        }
    }
    None
}

/// The pause page; SAVE and LOAD where the game has save slots and someone
/// plays (not while a replay is watched).
fn pause_menu<G: Game>(game: &Session<G>, playing: bool) -> Menu {
    if !playing {
        return Menu::pause();
    }
    let now = crate::saves::now();
    Menu::pause().with_slots((1..=game.slots.count()).map(|n| game.slots.info(n).map(|i| crate::menu::slot_line(n, Some(&i), now, &game.language()))).collect())
}

/// The seats as the plan in the settings has them, with the pads plugged
/// in (the players page before a run, and where its runs begin).
fn planned(settings: &Settings, named: &[(u32, String)], seats: u32) -> crate::seating::Seating {
    let mut s = crate::seating::Seating::new(seats);
    s.begin(None, &crate::seating::resolve(&settings.players, named));
    s
}

/// The seat after the first the plan gives the keyboard (its start key
/// takes it).
fn planned_keyboard(settings: &Settings) -> Option<usize> {
    settings.players.iter().enumerate().skip(1).find(|(_, p)| p.keyboard != crate::seating::Keys::Nothing).map(|(i, _)| i + 1)
}

/// The seat the plan gives a pad (its START takes it).
fn planned_pad(settings: &Settings, named: &[(u32, String)], pad: u32) -> Option<usize> {
    let name = named.iter().find(|(id, _)| *id == pad).map(|(_, n)| n)?;
    settings.players.iter().position(|p| p.pad.as_ref() == Some(name)).map(|i| i + 1)
}

/// The player's settings that the game itself follows, and the game's
/// languages they choose from.
fn apply_settings<G: Game>(game: &mut Session<G>, settings: &mut Settings) {
    settings.offer_languages(&game.cfg.languages);
    game.options = settings.options();
}

/// Gamepads rumble with what happened in the last tick, as hard and as
/// long as the game says (`Game::rumble`).
fn rumble<G: Game>(pads: &mut [Gamepad], game: &Session<G>) {
    let Some((strength, ms)) = game.game.rumble() else { return };
    let level = (strength.min(1.0) * 65535.0) as u16;
    for pad in pads.iter_mut() {
        let _ = pad.set_rumble(level, level / 2, ms);
    }
}

/// A failure said on the screen for a few seconds: `what` with the cause's
/// own words (the whole chain went to stderr already).
fn trouble(lang: &str, what: &'static str, e: &anyhow::Error) -> (String, Instant) {
    (crate::lang::say_with(lang, what, &[&e.root_cause().to_string()]).to_uppercase(), Instant::now())
}

/// A fresh run begun from `--scenario` and `--do`: what the lines answer
/// printed; a mistake printed too (a scenario's stops the game, and shows
/// over it until mended).
fn begun<G: Game>(begin: &crate::session::Begin, game: &mut Session<G>) {
    if let Err(e) = begin.apply(game, |line, said| println!("> {line}\n{said}")) {
        eprintln!("{e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failure to save says why on the screen, in the player's language,
    /// in the cause's own words.
    #[test]
    fn a_failure_is_said_on_the_screen() {
        let e = anyhow::anyhow!("No space left on device (os error 28)").context("writing /x/1.toml");
        assert_eq!(trouble("en", "COULD NOT SAVE: {}", &e).0, "COULD NOT SAVE: NO SPACE LEFT ON DEVICE (OS ERROR 28)");
        assert!(trouble("de", "COULD NOT SAVE: {}", &e).0.starts_with("SPEICHERN FEHLGESCHLAGEN"));
    }
}

//! The windowed game: SDL3 for window and input, wgpu for drawing, a fixed
//! 60 Hz simulation with interpolated rendering, and hot reload of the
//! game folder.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Scancode;

use sdl3::gamepad::{Axis, Button, Gamepad};


use crate::controls::{Device, PAUSE, Nav, PadStyle};
use crate::game::Game;
use crate::replay::replay_session;
use crate::saves::Slots;
use crate::session::{INTRO_TIME, Reloaded, Scene, Session};
use crate::menu::{Command, Menu, Page};
use crate::render::Renderer;
use crate::settings::{self, Settings};
use crate::replay::{Attract, Playback, Replay, Runs};
use crate::sim::DT;
use crate::watch::FolderWatch;

/// Simulation ticks allowed per rendered frame before we drop time.
const MAX_CATCH_UP: u32 = 5;
/// How loud the game's sounds are under the title music while the title
/// plays a replay (attract mode).
const ATTRACT_SOUNDS: f32 = 1.0 / 3.0;
/// How long a frame plays the run so far through while a guest catches up:
/// most of it (only the progress is drawn meanwhile), so an hour of play
/// takes about 50 s here.
const CATCH_UP_BUDGET: Duration = Duration::from_millis(40);
/// Ticks the high score table shows before the title (or the replay of the
/// title's round) comes.
const HIGH_SCORES_TICKS: u64 = 8 * 60;

/// Runs the game folder `dir`, a game of kind `G`, in a window; with
/// `watching`, shows that replay instead; with `network`, hosts a network
/// game at once or joins one (`--host`, `--join`).
pub fn run<G: Game>(dir: &Path, watching: Option<Replay>, begin: crate::session::Begin, network: Option<crate::headless::Networked>, keys: Option<&str>) -> Result<()> {
    // A replay watched plays on slots of its own, never the player's.
    let mut game = match &watching {
        Some(replay) => replay_session::<G>(dir, replay)?,
        None => Session::<G>::load_with(dir, Slots::on_disk(dir))?,
    };

    let sdl = sdl3::init().context("initialising SDL")?;
    let video = sdl.video().context("initialising SDL video")?;
    let mut screen = crate::window::Screen::open(&video, &format!("voxcade - {}", game.cfg.title), 1600, 900, true)?;
    let crate::window::Screen { surface, device, queue, config, window, .. } = &mut screen;
    let (format, width, height) = (config.format, config.width, config.height);

    let mut renderer = Renderer::new(device, format, width, height);
    let mut mask = game.face_mask();
    renderer.rebuild_all(device, queue, game.world_mut(), &mask);

    // Sound is welcome, not required: without an audio device the game runs silent.
    let mut speaker = crate::audio::speaker(&sdl);

    let settings_path = settings::path_for(dir);
    let mut settings = Settings::load(&settings_path);
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
    if settings.fullscreen {
        window.set_fullscreen(true).context("fullscreen")?;
    }

    // Gamepads: every one plugged in now or later drives the game.
    let mut pads = crate::pads::Pads::new(&sdl);
    // The left stick steering a menu: the way it points, once past the
    // middle, and back to the middle before it counts again.
    let mut stick: Option<Nav> = None;

    // The front end: the title (its menu, its scene's clock), or the game,
    // paused while the menu is open.
    let mut menu = title_menu(&game);
    // What the player pressed last, kept whatever menu opens: the menus'
    // hints and the game's name its keys or buttons.
    let mut played_with = Device::Keyboard;
    let mut pad_kind = PadStyle::default();
    // Who plays on which seat (`c.players`): the pad pressed last before a
    // run began is seat 1's, with the keyboard.
    let mut seating = crate::seating::Seating::new(game.cfg.players);
    let mut last_pad: Option<u32> = None;
    // The lines of a run of several still to enter, whose and how many.
    let mut entering: Vec<(Option<usize>, u32)> = Vec::new();
    let mut playing = false;
    let mut title_time = 0.0f32;
    let mut title_tick = 0u64;
    let mut title_music = true;
    // The song the title's settings play instead of the title's, if any.
    let mut previewing: Option<usize> = None;
    // Seconds into the glide from the title into the game, while it runs.
    let mut intro: Option<f32> = None;
    if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
        mixer.set_music(game.title_music());
    }

    // Every run is recorded (`Runs`); a replay being watched gives the input
    // instead of the keys.
    let mut runs = Runs::new(dir);
    let mut playback = None;
    // The title's attract mode: a replay playing under the title's music.
    let mut attract = Attract::new(dir);
    let mut attracting = false;
    let mut attract_time = 0.0f32;
    let mut leave_attract = false;
    // Looking away during a level change's fade pauses once it is over.
    let mut pause_after_fade = false;
    // The title's round as at an arcade machine: the title, the high score
    // table, a replay. Whether the table is showing in it, and whether it
    // was shown since the last replay.
    let mut table_in_round = false;
    let mut table_shown = false;
    if let Some(replay) = &watching {
        playback = Some(Playback::new(replay, &mut game)?);
        // Told once it began as the run did (a scenario's file counts).
        if !replay.fits(game.fingerprint()?) {
            eprintln!("the replay was recorded on other game data: it will run apart");
        }
        playing = true;
        menu = Menu::default();
        game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
        game.music_changed = true;
        title_music = false;
    } else if !begin.is_empty() {
        // A scenario to try: past the title, straight into it.
        begun(&begin, &mut game);
        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
        runs.start(&game);
        playing = true;
        menu = Menu::default();
        game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
        game.music_changed = true;
        title_music = false;
    }

    // A network game (`docs/multiplayer.md`, stage 4), the port asked to
    // host on, what to do first (the command line's --host and --join), a
    // line said on leaving one, and whether the host's address is typed.
    let mut net: Option<crate::network::Network<G>> = None;
    let mut host_port = None;
    let mut queued = Vec::new();
    match network {
        Some(crate::headless::Networked::Host(port)) => {
            host_port = (port != 0).then_some(port);
            queued.push(Command::Host);
        }
        Some(crate::headless::Networked::HostNet) => queued.push(Command::HostNet),
        Some(crate::headless::Networked::Join(address)) => {
            menu.address = address;
            menu.stack.push((Page::Joining, 0));
            queued.push(Command::Join);
        }
        None => {}
    }
    let mut parting: Option<(String, Instant)> = None;
    let mut typing_address = false;
    // The hosted game's code last put on the clipboard.
    let mut copied_code: Option<String> = None;
    // Player 1's keys from a script (`--input`), a step every tick's time,
    // instead of the devices: a game tried hands-free.
    let mut scripted = keys.map(|k| crate::replay::parse_script(k, game.actions()).map(crate::replay::Script::new)).transpose()?.map(|s| (s, 0.0f32, crate::input::Input::default()));
    // A guest joins again by itself once after its game ran apart.
    let mut rejoined = false;
    // The search for games on the LAN while JOIN lists them.
    let mut finder: Option<crate::network::Finder> = None;
    // The list of games on the internet from our matchmaker, likewise.
    let mut net_finder: Option<crate::network::NetFinder> = None;

    // The console (the key left of 1): a line of Lua between two ticks,
    // the game standing still while it is open.
    let mut prompt = crate::prompt::Prompt::default();
    let text_input = video.text_input();

    let mut now_playing = String::new();
    let mut watch = FolderWatch::new(dir);

    let mut events = sdl.event_pump().context("SDL event pump")?;
    let mut last = Instant::now();
    let mut accumulator = 0.0f32;
    let mut fps_frames = 0u32;
    let mut fps_since = Instant::now();
    let mut fps = 0.0f32;

    'main: loop {
        let mut commands = std::mem::take(&mut queued);
        let phase_over = game.game.outcome().is_some();
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::Window { win_event: WindowEvent::PixelSizeChanged(w, h), .. } => {
                    config.width = w.max(1) as u32;
                    config.height = h.max(1) as u32;
                    surface.configure(device, config);
                    renderer.resize(device, config.width, config.height);
                }
                // Looking away pauses the game (a network game's host fetching
                // a drink would freeze every screen: ESCAPE does it).
                Event::Window { win_event: WindowEvent::FocusLost, .. } if playing && !menu.is_open() && !phase_over && net.is_none() => {
                    if game.fading() {
                        pause_after_fade = true;
                    } else {
                        menu = pause_menu(&game, watching.is_none());
                    }
                }
                Event::GamepadAdded { which, .. } => pads.added(which),
                Event::GamepadRemoved { which, .. } => pads.removed(which),
                Event::TextInput { text, .. } if prompt.open => prompt.type_text(&text),
                Event::TextInput { text, .. } if typing_address => text.chars().for_each(|c| menu.type_address(c)),

                // Keys.
                Event::KeyDown { scancode: Some(code), keycode, keymod, repeat, .. } => {
                    if watching.is_some() && code == Scancode::Escape {
                        break 'main;
                    }
                    attract.input();
                    if attracting {
                        leave_attract = true;
                        continue;
                    }
                    // The console open: its keys, none for the game.
                    if prompt.open {
                        let ctrl = keymod.intersects(sdl3::keyboard::Mod::LCTRLMOD | sdl3::keyboard::Mod::RCTRLMOD);
                        match code {
                            Scancode::Escape | Scancode::Grave if !repeat => {
                                prompt.close();
                                text_input.stop(window);
                            }
                            Scancode::Return | Scancode::KpEnter if !repeat => {
                                if let Some(line) = prompt.take() {
                                    prompt.answer(game.eval(&line));
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
                        continue;
                    }
                    // The host's address typed: Backspace takes a sign, Enter
                    // joins, Escape goes back; the letters are the text's.
                    if typing_address {
                        match code {
                            Scancode::Backspace => menu.backspace_address(),
                            Scancode::Return | Scancode::KpEnter if !repeat => commands.extend(menu.nav(Nav::Confirm, &mut settings, 0)),
                            Scancode::Escape if !repeat => commands.extend(menu.nav(Nav::Back, &mut settings, 0)),
                            _ => {}
                        }
                        continue;
                    }
                    // A guest's console would change its game alone.
                    if code == Scancode::Grave && !repeat && playing && watching.is_none() && !menu.is_open() && !game.fading() && net.as_ref().is_none_or(|n| n.hosting()) {
                        prompt.open();
                        text_input.start(window);
                        continue;
                    }
                    let name = code.name();
                    if menu.rebinding.is_some() {
                        if !repeat {
                            commands.extend(menu.bind(Device::Keyboard, name, code == Scancode::Escape, &mut settings));
                        }
                        continue;
                    }
                    // Initials are typed, the letter keys taken as the letters
                    // they say on this keyboard.
                    let typed = keycode.map(|k| k.name()).unwrap_or_default();
                    if menu.page() == Some(Page::Initials)
                        && let Some(c) = typed.chars().next().filter(|c| typed.len() == 1 && crate::menu::LETTERS.contains(c.to_ascii_uppercase()))
                    {
                        played_with = Device::Keyboard;
                        if !repeat {
                            commands.extend(menu.type_letter(c));
                        }
                        continue;
                    }
                    if code == Scancode::F11 && !repeat {
                        settings.fullscreen = !settings.fullscreen;
                        commands.push(Command::Fullscreen);
                    } else if code == Scancode::F5 && !repeat {
                        commands.push(Command::Restart);
                    } else if code == Scancode::F9 && !repeat && watching.is_none() && net.as_ref().is_some_and(|n| !n.hosting()) {
                        // A guest keeps the network game's recording.
                        if let Some(crate::network::Side::Guest(j)) = net.as_ref().map(|n| &n.side) {
                            let path = dir.join("replays").join(format!("network-{}.toml", crate::saves::now()));
                            match j.recording(&game).and_then(|r| r.save(&path)) {
                                Ok(()) => println!("kept the run as {}", path.display()),
                                Err(e) => println!("F9: {e:#}"),
                            }
                        }
                    } else if code == Scancode::F9 && !repeat && watching.is_none() {
                        match runs.keep(&game) {
                            Ok(path) => println!("kept the run as {}", path.display()),
                            Err(e) => println!("F9: {e:#}"),
                        }
                    } else if let Some(index) = song_key(code).filter(|_| !repeat && menu.page() != Some(Page::Initials)) {
                        if index < game.cfg.audio.music.files().len() {
                            settings.song = index;
                            commands.push(Command::Song);
                        } else {
                            println!("no song {} (c.audio.music lists {})", index + 1, game.cfg.audio.music.files().len());
                        }
                    } else if menu.is_open() {
                        played_with = Device::Keyboard;
                        if let Some(nav) = key_nav(code, &settings).filter(|n| !repeat || is_direction(*n)) {
                            commands.extend(menu.nav(nav, &mut settings, game.cfg.audio.music.files().len()));
                        }
                    } else if !repeat
                        && playing
                        && watching.is_none()
                        && !phase_over
                        && let Some(seat) = (1..=seating.seats()).filter(|&n| matches!(seating.keys(n), crate::seating::Keys::Left | crate::seating::Keys::Right)).find(|&n| settings.keys_of(seating.keys(n)).and_then(|b| b.action_of(name)) == Some(PAUSE))
                    {
                        // A half's pause: its seat's (LEAVE offered after the first).
                        if !game.fading() {
                            menu = pause_menu(&game, watching.is_none());
                            menu.leaving = (seat > 1).then_some(seat);
                        }
                    } else if !repeat && playing && watching.is_none() && !phase_over && !seating.alone() && name == crate::seating::KEYBOARD_START && settings.keys.action_of(name).is_none() {
                        // The keyboard (whole, or its right half): a seat taken,
                        // then its pause.
                        match seating.keyboard_start(planned_keyboard(&settings)) {
                            crate::seating::Start::Pause(seat) if !game.fading() => {
                                menu = pause_menu(&game, watching.is_none());
                                menu.leaving = (seat > 1).then_some(seat);
                            }
                            crate::seating::Start::Join(seat) if seating.keys(seat) == crate::seating::Keys::Whole => println!("player {seat} takes a seat at the keyboard"),
                            crate::seating::Start::Join(seat) => println!("player {seat} takes a seat at the keyboard's right half"),
                            _ => {}
                        }
                    } else if !repeat && playing && watching.is_none() {
                        played_with = Device::Keyboard;
                        let pause = settings.keys.action_of(name) == Some(PAUSE);
                        if phase_over && (pause || matches!(code, Scancode::Return | Scancode::KpEnter)) {
                            commands.push(Command::ToTitle);
                        } else if pause && !game.fading() {
                            // A level change's fade is seen out first; the
                            // whole keyboard another seat's, LEAVE offered.
                            menu = pause_menu(&game, watching.is_none());
                            menu.leaving = seating.keyboard_seat().filter(|&s| seating.keys(s) == crate::seating::Keys::Whole);
                        }
                    }
                }

                // Gamepad buttons and sticks.
                Event::GamepadButtonDown { which, button, .. } => {
                    attract.input();
                    if attracting {
                        leave_attract = true;
                        continue;
                    }
                    let Some(name) = crate::controls::button_name(button) else { continue };
                    // The letters on the pad in hand, for the menus' hints.
                    if let Some(pad) = pads.open.iter().find(|p| p.id().ok() == Some(which)) {
                        pad_kind = pad_style(pad.r#type());
                    }
                    if !playing {
                        last_pad = Some(which.raw());
                    }
                    if menu.rebinding.is_some() {
                        commands.extend(menu.bind(Device::Gamepad, name, button == Button::Back, &mut settings));
                        continue;
                    }
                    if menu.is_open() {
                        played_with = Device::Gamepad;
                        if let Some(nav) = pad_nav(button) {
                            commands.extend(menu.nav(nav, &mut settings, game.cfg.audio.music.files().len()));
                        }
                    } else if playing && watching.is_none() {
                        played_with = Device::Gamepad;
                        let pause = settings.pad.action_of(name) == Some(PAUSE);
                        if phase_over && (pause || button == Button::South) {
                            commands.push(Command::ToTitle);
                        } else if pause && !game.fading() {
                            // A pad no seat has takes one; a seat's START
                            // pauses (a fade seen out first), offering
                            // LEAVE to a player after the first.
                            let plugged = pads.ids();
                            match seating.pad_start(which.raw(), &plugged, planned_pad(&settings, &pads.named(), which.raw())) {
                                crate::seating::Start::Pause(seat) => {
                                    menu = pause_menu(&game, watching.is_none());
                                    menu.leaving = (seat > 1).then_some(seat);
                                }
                                crate::seating::Start::Join(seat) => println!("player {seat} takes a seat"),
                                crate::seating::Start::Full => {}
                            }
                        }
                    }
                }
                Event::GamepadAxisMotion { axis, value, .. } => {
                    let v = value as f32 / 32767.0;
                    if v.abs() > 0.5 {
                        attract.input();
                        if attracting {
                            leave_attract = true;
                            continue;
                        }
                    }
                    if menu.rebinding.is_some() {
                        let trigger = match axis {
                            Axis::TriggerLeft => Some("lefttrigger"),
                            Axis::TriggerRight => Some("righttrigger"),
                            _ => None,
                        };
                        if let Some(name) = trigger.filter(|_| v > 0.6) {
                            commands.extend(menu.bind(Device::Gamepad, name, false, &mut settings));
                        }
                        continue;
                    }
                    if !menu.is_open() {
                        if v.abs() > 0.5 && playing && watching.is_none() {
                            played_with = Device::Gamepad;
                        }
                        continue;
                    }
                    if !matches!(axis, Axis::LeftX | Axis::LeftY) {
                        continue;
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
                        Some(nav) if stick != Some(nav) => {
                            stick = Some(nav);
                            played_with = Device::Gamepad;
                            commands.extend(menu.nav(nav, &mut settings, game.cfg.audio.music.files().len()));
                        }
                        None if v.abs() < 0.3 && stick.is_some_and(along) => stick = None,
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        prompt.settle();
        pads.settle();

        // The network: what came; a guest plays the run so far through,
        // then its game is the one shown; a guest's game ended says why.
        if let Some(n) = &mut net {
            if let Err(e) = n.receive(&game, &settings.language()) {
                eprintln!("the network game: {e:#}");
            }
            if let crate::network::Side::Guest(j) = &mut n.side {
                match j.catch_up(CATCH_UP_BUDGET) {
                    Ok(Some(caught)) => {
                        println!("caught up with the host at tick {}", caught.game.sim().tick);
                        game = caught;
                        apply_settings(&mut game, &mut settings);
                        game.fades = true;
                        mask = game.face_mask();
                        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                        seating = crate::seating::Seating::new(game.cfg.players);
                        seating.begin(last_pad.filter(|_| played_with == Device::Gamepad), &crate::seating::resolve(&settings.players, &pads.named()));
                        playing = true;
                        menu = Menu::default();
                        game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
                        game.music_changed = true;
                        title_music = false;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        let _ = j.end(&format!("{e:#}").to_uppercase());
                    }
                }
            }
            // A guest's game run apart from the host's: a report written,
            // and where only what it was sent differed (the network's
            // fault), it joins again by itself, once.
            let mut rejoin = false;
            if let crate::network::Side::Guest(j) = &mut n.side
                && let crate::net::play::Joined::Apart(apart) = j.state()
            {
                match j.report(&game, &apart) {
                    Ok(at) => at.iter().for_each(|at| println!("the desync's report: {}", at.display())),
                    Err(e) => eprintln!("the desync's report: {e:#}"),
                }
                let line = crate::network::desync_line(&apart, "", &j.host_commit);
                let _ = j.end(&line);
                rejoin = apart.inputs && !rejoined;
                rejoined |= rejoin;
            }
            if let crate::network::Side::Guest(j) = &n.side
                && let crate::net::play::Joined::Gone(why) = j.state()
            {
                println!("the network game ended: {why}");
                n.keep_run(&game, &runs);
                parting = Some((why, Instant::now()));
                net = None;
                if rejoin {
                    // Back to the title's game, joining at once.
                    if let Ok(fresh) = Session::<G>::load_with(dir, Slots::on_disk(dir)) {
                        game = fresh;
                        apply_settings(&mut game, &mut settings);
                        mask = game.face_mask();
                        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                    }
                    (playing, intro, title_time) = (false, None, 0.0);
                    menu = title_menu(&game);
                    menu.address = settings.address.clone();
                    menu.stack.push((Page::Joining, 0));
                    queued.push(Command::Join);
                } else if playing {
                    commands.push(Command::ToTitle);
                } else {
                    menu = title_menu(&game);
                }
            }
        }

        // What the menus (and the keys around them) asked for.
        for command in commands {
            match command {
                Command::Quit => break 'main,
                Command::Start => {
                    if !begin.is_empty() {
                        begun(&begin, &mut game);
                        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                    }
                    runs.start(&game);
                    seating = crate::seating::Seating::new(game.cfg.players);
                    seating.begin(last_pad.filter(|_| played_with == Device::Gamepad), &crate::seating::resolve(&settings.players, &pads.named()));
                    intro = Some(0.0);
                    menu = Menu::default();
                    game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
                    game.music_changed = true;
                    title_music = false;
                }
                command @ (Command::Host | Command::HostNet) => {
                    // As START, the game open to the network at once: the
                    // LAN, or the internet (its ticket on the clipboard).
                    if !begin.is_empty() {
                        begun(&begin, &mut game);
                        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                    }
                    game.set_clock(crate::saves::now());
                    let hosted = if command == Command::HostNet { crate::network::Network::host_net(&game) } else { crate::network::Network::host(&game, host_port) };
                    match hosted {
                        Ok(mut n) => {
                            println!("hosting {} at {}", game.cfg.title, n.found().join(", "));
                            if let Some(ticket) = n.ticket() {
                                let copied = video.clipboard().set_clipboard_text(ticket).is_ok();
                                if copied {
                                    n.say(crate::lang::say(&settings.language(), "TICKET COPIED: SEND IT").to_string());
                                }
                            }
                            net = Some(n);
                        }
                        Err(e) => parting = Some((format!("{e:#}").to_uppercase(), Instant::now())),
                    }
                    runs.start(&game);
                    seating = crate::seating::Seating::new(game.cfg.players);
                    seating.begin(last_pad.filter(|_| played_with == Device::Gamepad), &crate::seating::resolve(&settings.players, &pads.named()));
                    intro = Some(0.0);
                    menu = Menu::default();
                    game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
                    game.music_changed = true;
                    title_music = false;
                }
                Command::Join => {
                    // An address typed is kept for next time; a ticket is
                    // for one game.
                    let address = menu.address.trim().to_string();
                    if !crate::net::link::Link::is_net(&address) {
                        settings.address = address.clone();
                        if let Err(e) = settings.save(&settings_path) {
                            eprintln!("settings: {e:#}");
                        }
                    }
                    match crate::network::Network::join(dir, &address, game.cfg.players, crate::network::app_of(&game).ok()) {
                        Ok(n) => net = Some(n),
                        Err(e) => {
                            parting = Some((format!("{e:#}").to_uppercase(), Instant::now()));
                            menu = title_menu(&game);
                        }
                    }
                }
                Command::JoinCode => match crate::network::Network::join(dir, &menu.code, game.cfg.players, crate::network::app_of(&game).ok()) {
                    Ok(n) => net = Some(n),
                    Err(e) => {
                        parting = Some((format!("{e:#}").to_uppercase(), Instant::now()));
                        menu = title_menu(&game);
                    }
                },
                Command::JoinAny => match crate::network::app_of(&game).and_then(|(app, version)| crate::network::Network::join_any(dir, game.cfg.players, app, version)) {
                    Ok(n) => net = Some(n),
                    Err(e) => {
                        parting = Some((format!("{e:#}").to_uppercase(), Instant::now()));
                        menu = title_menu(&game);
                    }
                },
                Command::Disconnect => {
                    if let Some(mut n) = net.take() {
                        n.leave();
                    }
                    menu = title_menu(&game);
                }
                Command::PasteTicket => match video.clipboard().clipboard_text() {
                    Ok(text) if crate::net::link::Link::is_ticket(&text) => {
                        menu.address = text.trim().to_string();
                        menu.stack.truncate(1);
                        menu.stack.push((Page::Joining, 0));
                        queued.push(Command::Join);
                    }
                    Ok(text) if crate::net::link::Link::is_code(&text) => {
                        menu.code = matchmaker_protocol::tidy(&text).unwrap_or_default();
                        menu.stack.truncate(1);
                        menu.stack.push((Page::Joining, 0));
                        queued.push(Command::JoinCode);
                    }
                    _ => parting = Some((crate::lang::say(&settings.language(), "NO TICKET OR CODE ON THE CLIPBOARD").to_string(), Instant::now())),
                },
                Command::CopyCode => {
                    if let Some(n) = &mut net
                        && let Some(code) = n.game_code().map(str::to_string)
                        && video.clipboard().set_clipboard_text(&code).is_ok()
                    {
                        n.say(crate::lang::say(&settings.language(), "CODE COPIED: SEND IT").to_string());
                    }
                }
                Command::CopyTicket => {
                    if let Some(n) = &mut net
                        && let Some(ticket) = n.ticket().map(str::to_string)
                        && video.clipboard().set_clipboard_text(&ticket).is_ok()
                    {
                        n.say(crate::lang::say(&settings.language(), "TICKET COPIED: SEND IT").to_string());
                    }
                }
                Command::LetIn(peer) => {
                    if let Some(n) = &mut net {
                        n.let_in(peer);
                    }
                    if let Some((_, cursor)) = menu.stack.last_mut() {
                        *cursor = 0;
                    }
                }
                Command::Drop(machine) => {
                    if let Some(crate::network::Side::Host(h)) = net.as_mut().map(|n| &mut n.side) {
                        h.host.drop_machine(machine);
                    }
                    if let Some((_, cursor)) = menu.stack.last_mut() {
                        *cursor = 0;
                    }
                }
                Command::Resume => menu = Menu::default(),
                // The players page (CONTROLS in a game of several): the run's seats while playing, else the
                // plan the next run begins with; the settings keep it.
                Command::SeatKeys(..) | Command::SeatPad(..) | Command::Swap => {
                    let named = pads.named();
                    let mut seats = if playing { seating.clone() } else { planned(&settings, &named, game.cfg.players) };
                    let mut chosen = None;
                    match command {
                        Command::SeatKeys(seat, step) => seats.set_keyboard(seat, seats.keys(seat).step(step)),
                        Command::SeatPad(seat, step) => {
                            let choices: Vec<Option<u32>> = std::iter::once(None).chain(named.iter().map(|(id, _)| Some(*id))).collect();
                            let at = choices.iter().position(|&p| p == seats.pad(seat)).unwrap_or(0) as i32;
                            seats.set_pad(seat, choices[(at + step).rem_euclid(choices.len() as i32) as usize]);
                            chosen = Some(seat);
                        }
                        _ => seats.swap(1, 2),
                    }
                    settings.players = crate::seating::plan_of(&seats, &named, &settings.players, chosen);
                    if let Err(e) = settings.save(&settings_path) {
                        eprintln!("settings: {e:#}");
                    }
                    if playing {
                        seating = seats;
                    }
                }
                Command::Leave(seat) => {
                    // A guest's first player gives up its seat and watches.
                    match &mut net {
                        Some(n) if seat == 1 => n.watching = true,
                        _ => seating.leave(seat),
                    }
                    println!("player {seat} leaves");
                    menu = Menu::default();
                }
                Command::Save(slot) => {
                    game.set_clock(crate::saves::now());
                    if let Err(e) = game.save_to(slot) {
                        eprintln!("saving into slot {slot}: {e:#}");
                    }
                    // Back on the pause page, the slot's new line in it.
                    menu.slots = pause_menu(&game, true).slots;
                    menu.stack.pop();
                }
                // The host's LOAD: the save goes to every guest first, then
                    // the run starts from it for everyone at one tick.
                Command::Load(slot) if net.as_ref().is_some_and(|n| n.hosting()) => {
                    if let Some(crate::network::Side::Host(h)) = net.as_mut().map(|n| &mut n.side)
                        && let Err(e) = h.load(&game, slot)
                    {
                        eprintln!("loading slot {slot}: {e:#}");
                    }
                    menu = Menu::default();
                }
                Command::Load(slot) => {
                    // Going on from a save is a run of its own: the game as
                    // loaded afresh, then the save (as its replay does it).
                    // One that does not load leaves the run as it was, paused.
                    let text = game.slots.read(slot).map(str::to_string).filter(|_| watching.is_none());
                    let loaded = text.map(|text| Session::<G>::load_with(dir, Slots::on_disk(dir)).and_then(|mut fresh| fresh.start_from(&text).map(|()| fresh)));
                    match loaded {
                        Some(Ok(fresh)) => {
                            // From the title (CONTINUE) no run was going.
                            if playing {
                                runs.end(&game);
                            }
                            title_music = false;
                            game = fresh;
                            apply_settings(&mut game, &mut settings);
                            mask = game.face_mask();
                            renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                            game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
                            game.music_changed = true;
                            runs.start(&game);
                            menu = Menu::default();
                            playing = true;
                        }
                        Some(Err(e)) => {
                            eprintln!("loading slot {slot}: {e:#}");
                            menu = if playing { pause_menu(&game, true) } else { title_menu(&game) };
                        }
                        None => menu = if playing { pause_menu(&game, watching.is_none()) } else { title_menu(&game) },
                    }
                }
                Command::Entered => {
                    let rank = settings.enter(&menu.entry.initials(), menu.entry.score);
                    if let Err(e) = settings.save(&settings_path) {
                        eprintln!("settings: {e:#}");
                    }
                    // The next player's initials, if another got in.
                    menu = next_initials(&mut entering, &settings).unwrap_or_else(|| Menu::high_scores(rank));
                }
                Command::Title => {
                    menu = title_menu(&game);
                    (table_in_round, table_shown) = (false, false);
                }
                // Over the network F5 starts the run anew for everyone, from
                // the host; a guest's F5 does nothing.
                Command::Restart if net.is_some() => {
                    if let Some(n) = &mut net
                        && let crate::network::Side::Host(h) = &mut n.side
                        && let Err(e) = h.restart(&game)
                    {
                        n.say(format!("{e:#}").to_uppercase());
                    }
                    menu = Menu::default();
                }
                Command::Restart | Command::ToTitle => {
                    if let Some(mut n) = net.take() {
                        n.keep_run(&game, &runs);
                        n.leave();
                    }
                    // A run left (its end, or quit from the pause) goes into
                    // the high score table if it gets in: its initials first.
                    let left = (command == Command::ToTitle && watching.is_none() && game.game.scored()).then(|| game.local_scores());
                    let helped = game.helped();
                    if command == Command::ToTitle && watching.is_none() && game.game.outcome().is_none() {
                        game.game.quit();
                    }
                    if watching.is_none() {
                        runs.end(&game);
                    }
                    let fresh = match (&watching, command) {
                        (Some(replay), Command::Restart) => replay_session::<G>(dir, replay),
                        _ => Session::<G>::load_with(dir, Slots::on_disk(dir)),
                    };
                    match fresh {
                        Ok(fresh) => {
                            game = fresh;
                            apply_settings(&mut game, &mut settings);
                            mask = game.face_mask();
                            renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                        }
                        Err(e) => eprintln!("reloading the game failed: {e:#}"),
                    }
                    if command == Command::Restart {
                        match &watching {
                            Some(replay) => playback = Some(Playback::new(replay, &mut game)?),
                            None => {
                                if !begin.is_empty() {
                                    begun(&begin, &mut game);
                                    renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                                }
                                runs.start(&game);
                            }
                        }
                        playing = true;
                        menu = Menu::default();
                        game.select_song(settings.song.min(game.cfg.audio.music.files().len().saturating_sub(1)));
                        game.music_changed = true;
                        title_music = false;
                    } else {
                        playing = false;
                        menu = match left {
                            Some(lines) if helped => {
                                // The console or a scenario helped: its lines
                                // go in as GOD, no initials asked.
                                let mut rank = None;
                                for &(_, score) in &lines {
                                    rank = settings.enter(crate::session::HELPED, score).or(rank);
                                }
                                if let Err(e) = settings.save(&settings_path) {
                                    eprintln!("settings: {e:#}");
                                }
                                Menu::high_scores(rank)
                            }
                            Some(lines) => {
                                // Each player whose score gets in, in turn.
                                let several = lines.len() > 1;
                                entering = lines.into_iter().map(|(seat, score)| (several.then_some(seat), score)).collect();
                                next_initials(&mut entering, &settings).unwrap_or_else(|| Menu::high_scores(None))
                            }
                            None => title_menu(&game),
                        };
                        table_in_round = false;
                        title_time = 0.0;
                        title_music = true;
                        previewing = None;
                        game.music_changed = false;
                        if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
                            mixer.set_music(game.title_music());
                        }
                    }
                }
                Command::Changed | Command::Fullscreen | Command::Song => {
                    crate::controls::label_keys(&mut settings);
                    if command == Command::Fullscreen {
                        window.set_fullscreen(settings.fullscreen).context("toggling fullscreen")?;
                    }
                    if command == Command::Song && playing {
                        game.select_song(settings.song);
                    }
                    apply_settings(&mut game, &mut settings);
                    if let Err(e) = settings.save(&settings_path) {
                        eprintln!("settings: {e:#}");
                    }
                }
            }
        }

        // The title's replay ends at any key, at its end, or where it runs
        // apart: back to the title, its music playing on.
        if attracting && (std::mem::take(&mut leave_attract) || playback.as_ref().is_none_or(Playback::done)) {
            attracting = false;
            playback = None;
            match Session::<G>::load_with(dir, Slots::on_disk(dir)) {
                Ok(fresh) => {
                    game = fresh;
                    apply_settings(&mut game, &mut settings);
                    mask = game.face_mask();
                    renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                }
                Err(e) => eprintln!("reloading the game failed: {e:#}"),
            }
            playing = false;
            menu = title_menu(&game);
            title_time = 0.0;
            attract.input();
        }

        // Hot reload: apply whatever changed in the game folder.
        // Hot reload is off in a network game: the guests have only the
        // files it began with.
        let changed = watch.poll();
        if !changed.is_empty() && net.is_none() {
            let mut rebuild = false;
            for path in changed {
                match game.reload(&path) {
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
                apply_settings(&mut game, &mut settings);
                mask = game.face_mask();
                renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                println!("reloaded world");
            }
        }

        // Music: the title's on the title, the chosen song while the settings
        // are open there; the game's (started over when it changed) while
        // playing; the player's volume on top.
        if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
            if title_music {
                game.music_changed = false;
                let last = game.cfg.audio.music.files().len().saturating_sub(1);
                let preview = menu.previews_song().then_some(settings.song.min(last));
                if preview != previewing {
                    previewing = preview;
                    mixer.set_music(match preview {
                        Some(song) => game.song_at(song),
                        None => game.title_music(),
                    });
                }
            } else if game.feed_music(&mut mixer, menu.is_open() || prompt.open || game.fading()) {
                // The player's own choice numbered when there are several.
                let choice = game.cfg.audio.music.files().len() > 1 && !game.steers_music();
                now_playing = match mixer.music_title() {
                    Some(t) if choice => format!(" | \u{266a} {}: {t}", game.song + 1),
                    Some(t) => format!(" | \u{266a} {t}"),
                    None => String::new(),
                };
            }
            mixer.set_music_volume(game.cfg.audio.music_volume * settings.music_gain());
            mixer.set_drive(game.cfg.audio.drive);
        }

        // The players page shows the run's seats while playing, else the
        // plan (a game of several).
        if game.cfg.players > 1 {
            let named = pads.named();
            let shown = if playing { seating.clone() } else { planned(&settings, &named, game.cfg.players) };
            menu.seats = crate::menu::SeatLine::all(&shown, &named);
        }
        let keyboard = events.keyboard_state();
        let input = if seating.alone() {
            crate::controls::read_input(Some(&keyboard), &pads.open, &settings, game.actions())
        } else {
            // Each seat its keys and pads; who sits there for the ticks to
            // come (a replay brings its own).
            let of_seat = |seat: usize| pads.open.iter().filter(|p| p.id().ok().and_then(|id| seating.pad_seat(id.raw())) == Some(seat)).collect::<Vec<_>>();
            for seat in 2..=seating.seats() {
                let input = crate::controls::read_seat(Some(&keyboard), seating.keys(seat), &of_seat(seat), settings.pad_of(seat), &settings, game.actions());
                if playback.is_none() && playing && net.is_none() {
                    let controls = match seating.keys(seat) {
                        crate::seating::Keys::Whole => crate::controls::Controls::of(game.actions(), Device::Keyboard, pad_kind, &settings),
                        crate::seating::Keys::Nothing => crate::controls::Controls::of_pad(game.actions(), of_seat(seat).first().map_or(pad_kind, |p| pad_style(p.r#type())), settings.pad_of(seat), &settings),
                        half => crate::controls::Controls::of_half(game.actions(), half, &settings),
                    };
                    game.set_seat(seat, seating.taken(seat).then_some(input));
                    game.set_seat_controls(seat, controls);
                }
            }
            crate::controls::read_seat(Some(&keyboard), seating.keys(1), &of_seat(1), &settings.pad, &settings, game.actions())
        };
        // A network game: this machine's players' keys and controls go to
        // the host (the host's own held back there), the seats they sit on
        // asked for and given up; what the pictures show of them is theirs.
        if let Some(n) = &mut net
            && playing
        {
            let lang = settings.language();
            for local in n.settle(|l| seating.taken(l)) {
                if local > 1 {
                    seating.leave(local);
                } else {
                    n.watching = true;
                }
                n.say(crate::lang::say(&lang, "NO SEAT FREE: YOU WATCH").to_string());
            }
            let of_seat = |seat: usize| pads.open.iter().filter(|p| p.id().ok().and_then(|id| seating.pad_seat(id.raw())) == Some(seat)).collect::<Vec<_>>();
            // A guest's own menu open, its players hold nothing.
            let away = !n.hosting() && menu.is_open();
            for local in 1..=seating.seats() {
                let Some(seat) = n.locals.seat(local) else { continue };
                let pad = if local == 1 { &settings.pad } else { settings.pad_of(local) };
                let input = if away { crate::input::Input::default() } else { crate::controls::read_seat(Some(&keyboard), seating.keys(local), &of_seat(local), pad, &settings, game.actions()) };
                let controls = match (local, seating.keys(local)) {
                    (1, half @ (crate::seating::Keys::Left | crate::seating::Keys::Right)) if played_with == Device::Keyboard => crate::controls::Controls::of_half(game.actions(), half, &settings),
                    (1, crate::seating::Keys::Nothing) => game.player_controls(Device::Gamepad, pad_kind, &settings),
                    (1, _) => game.player_controls(played_with, pad_kind, &settings),
                    (_, crate::seating::Keys::Whole) => crate::controls::Controls::of(game.actions(), Device::Keyboard, pad_kind, &settings),
                    (_, crate::seating::Keys::Nothing) => crate::controls::Controls::of_pad(game.actions(), of_seat(local).first().map_or(pad_kind, |p| pad_style(p.r#type())), pad, &settings),
                    (_, half) => crate::controls::Controls::of_half(game.actions(), half, &settings),
                };
                n.read(seat, input, controls);
            }
            game.set_local(crate::game::Local { seats: Some(n.locals.seats()), language: Some(lang) });
        }
        let now = Instant::now();
        let elapsed = (now - last).as_secs_f32().min(0.25);
        // The script's keys for player 1, a step every tick's time.
        let input = match &mut scripted {
            Some((script, owed, held)) if playing => {
                *owed += elapsed / DT;
                while *owed >= 1.0 {
                    *held = script.next_input();
                    *owed -= 1.0;
                }
                if let Some(n) = &mut net
                    && let Some(seat) = n.locals.seat(1)
                {
                    let controls = game.player_controls(Device::Keyboard, pad_kind, &settings);
                    n.read(seat, *held, controls);
                }
                *held
            }
            _ => input,
        };
        prompt.slide(elapsed);
        accumulator += elapsed;
        last = now;
        if !playing {
            title_time += elapsed;
        }
        // Nothing flies over while initials are typed or the table shows:
        // a flyby starts afresh after.
        if matches!(menu.page(), Some(Page::Initials | Page::HighScores)) || game.flyby_from > title_time {
            game.flyby_from = title_time;
        }
        if attracting {
            attract_time += elapsed;
        }
        // Long enough on the title without a key: it plays one of the
        // game's replays (on the game as its files are).
        let on_title = !playing && intro.is_none() && menu.stack.len() == 1 && menu.page() == Some(Page::Title);
        let waited = watching.is_none() && net.is_none() && on_title && attract.wait(elapsed, game.cfg.title_screen.attract_after);
        // First the high score table, then a replay.
        let replay = if waited && game.game.scored() && !table_shown {
            menu = Menu::high_scores(None);
            (table_in_round, table_shown) = (true, true);
            None
        } else if waited {
            table_shown = false;
            let next = attract.next(game.fingerprint().unwrap_or(0));
            if next.is_none() {
                attract.input();
            }
            next
        } else {
            None
        };
        // The table goes after a while: on to the replay in the title's
        // round, else to the title.
        if menu.page() == Some(Page::HighScores) && menu.tick - menu.opened >= HIGH_SCORES_TICKS {
            menu = title_menu(&game);
            menu.tick = HIGH_SCORES_TICKS;
            if !std::mem::take(&mut table_in_round) {
                table_shown = false;
                attract.input();
            }
        }
        // A replay that does not load (a save it began from no longer
        // reads) is passed over.
        let attract_game = replay.map(|replay| replay_session::<G>(dir, &replay).and_then(|mut fresh| Playback::new(&replay, &mut fresh).map(|p| (fresh, p))));
        match attract_game {
            Some(Ok((fresh, p))) => {
                game = fresh;
                apply_settings(&mut game, &mut settings);
                mask = game.face_mask();
                renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                playback = Some(p);
                attracting = true;
                attract_time = 0.0;
                playing = true;
                menu = Menu::default();
            }
            Some(Err(e)) => eprintln!("the title's replay: {e:#}"),
            None => {}
        }
        if let Some(t) = &mut intro {
            *t += elapsed;
            if *t >= INTRO_TIME {
                intro = None;
                playing = true;
            }
        }
        // What the player plays with, for the ticks to come (a replay
        // brings its own), and the time a save in them is stamped with.
        game.set_clock(crate::saves::now());
        if playback.is_none() && net.is_none() {
            let controls = match seating.keys(1) {
                half @ (crate::seating::Keys::Left | crate::seating::Keys::Right) if played_with == Device::Keyboard => crate::controls::Controls::of_half(game.actions(), half, &settings),
                // The keyboard another's: seat 1 plays on its pad.
                crate::seating::Keys::Nothing => game.player_controls(Device::Gamepad, pad_kind, &settings),
                _ => game.player_controls(played_with, pad_kind, &settings),
            };
            game.set_controls(controls);
        }
        let mut ticks = 0;
        while accumulator >= DT && ticks < MAX_CATCH_UP {
            if menu.is_open() {
                menu.update();
            }
            let replay_over = playback.as_ref().is_some_and(Playback::done);
            let guest = net.as_ref().is_some_and(|n| !n.hosting());
            if guest {
                // A guest plays the host's bundles below; a level change's
                // fade passes here, frame by frame.
                if playing
                    && game.fading()
                    && let Some(crate::network::Side::Guest(j)) = net.as_mut().map(|n| &mut n.side)
                {
                    j.fade(&mut game);
                }
            } else if playing && !menu.is_open() && !prompt.open && !replay_over {
                let input = match &mut playback {
                    Some(p) => p.next_input(&mut game),
                    None => input,
                };
                // Level changes fade where someone plays; replays cut.
                game.fades = playback.is_none();
                // The host builds the tick's bundle and plays it.
                let hosted = match net.as_mut().map(|n| &mut n.side) {
                    Some(crate::network::Side::Host(h)) => Some(h.tick(&mut game)),
                    _ => {
                        game.step(&input);
                        None
                    }
                };
                match hosted {
                    // F5 or LOAD: a new run for everyone.
                    Some(Ok(true)) => {
                        mask = game.face_mask();
                        renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                        runs.start(&game);
                    }
                    Some(Err(e)) => {
                        eprintln!("the network game: {e:#}");
                        parting = Some((format!("{e:#}").to_uppercase(), Instant::now()));
                        if let Some(mut n) = net.take() {
                            n.leave();
                        }
                    }
                    _ => {}
                }
                if pause_after_fade && !game.fading() {
                    pause_after_fade = false;
                    menu = pause_menu(&game, watching.is_none());
                }
                match &mut playback {
                    Some(p) => {
                        if let Err(apart) = p.check(&mut game) {
                            if attracting {
                                leave_attract = true;
                            } else {
                                println!("the replay runs apart after {} s", apart.second);
                            }
                        }
                        if p.done() && !attracting {
                            println!("replay over: F5 plays it again, Escape quits");
                        }
                    }
                    None => runs.step(&game),
                }
                if !game.game.sim().sounds.is_empty() {
                    if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
                        let quiet = if attracting { ATTRACT_SOUNDS } else { 1.0 };
                        game.play_sounds(&mut mixer, settings.sound_gain() * quiet);
                    }
                    if settings.rumble && !attracting {
                        rumble(&mut pads.open, &game);
                    }
                }
            } else if !playing {
                title_tick += 1;
                if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
                    game.title_sounds(&mut mixer, title_tick, title_time, settings.sound_gain());
                }
            }
            accumulator -= DT;
            ticks += 1;
        }
        if ticks == MAX_CATCH_UP {
            accumulator = 0.0;
        }
        // A guest plays the host's bundles as they come, whatever menu is
        // open (its own menu stops nothing); a fade between them, the rest
        // wait for it.
        if let Some(n) = &mut net
            && playing
        {
            let now = n.now();
            if let crate::network::Side::Guest(j) = &mut n.side
                && !game.fading()
            {
                n.held.extend(j.ticks(now, MAX_CATCH_UP as usize));
                while !game.fading()
                    && let Some(b) = n.held.pop_front()
                {
                    match j.play(&mut game, &b) {
                        Ok(renewed) => {
                            if renewed {
                                mask = game.face_mask();
                                renderer.rebuild_all(device, queue, game.world_mut(), &mask);
                            }
                        }
                        Err(e) => {
                            let _ = j.end(&format!("{e:#}").to_uppercase());
                            break;
                        }
                    }
                    if !game.game.sim().sounds.is_empty() {
                        if let Some(mut mixer) = speaker.as_mut().and_then(|s| s.lock()) {
                            game.play_sounds(&mut mixer, settings.sound_gain());
                        }
                        if settings.rumble {
                            rumble(&mut pads.open, &game);
                        }
                    }
                }
            }
            // The host's loop standing still pauses every machine.
            if let crate::network::Side::Host(h) = &mut n.side {
                h.host.set_paused((menu.is_open() || prompt.open).then_some(0));
            }
        }
        if let Some(n) = &mut net {
            n.send();
            // The game's code, once our server gave it, on the clipboard
            // in place of the ticket: the shorter to send.
            if let Some(code) = n.game_code().map(str::to_string)
                && copied_code.as_ref() != Some(&code)
            {
                if video.clipboard().set_clipboard_text(&code).is_ok() {
                    n.say(crate::lang::say_with(&settings.language(), "ON THE INTERNET: CODE {}", &[&code]));
                    n.say(crate::lang::say(&settings.language(), "CODE COPIED: SEND IT").to_string());
                }
                copied_code = Some(code);
            }
        }
        renderer.update_chunks(device, queue, game.world_mut(), &mask);

        let aspect = config.width as f32 / config.height as f32;
        let scene = match intro {
            Some(t) => Scene::Intro { time: title_time, t: t / INTRO_TIME },
            None if playing => Scene::Play,
            None => Scene::Title { time: title_time },
        };
        (menu.device, menu.pad_style) = (played_with, pad_kind);
        // What the menus show of the network game; the games found on the
        // LAN while JOIN lists them; the host's address typed as text.
        menu.net = net.as_ref().filter(|_| menu.is_open()).map(|n| n.view(&settings.language()));
        if menu.page() == Some(Page::Find) {
            if finder.is_none() {
                finder = game.fingerprint().ok().and_then(|f| crate::network::Finder::new(f).map_err(|e| eprintln!("searching the network: {e:#}")).ok());
            }
            let mut found = finder.as_mut().map(|f| f.found(&settings.language())).unwrap_or_default();
            if net_finder.is_none() && menu.internet {
                net_finder = crate::network::app_of(&game).ok().map(|(app, version)| crate::network::NetFinder::new(app, version));
            }
            if let Some(n) = &mut net_finder {
                found.extend(n.found());
            }
            menu.found = found;
        } else if finder.take().is_some() | net_finder.take().is_some() {
            menu.found.clear();
        }
        let typing = matches!(menu.page(), Some(Page::Join | Page::Code));
        if typing != typing_address {
            typing_address = typing;
            if typing {
                text_input.start(window);
            } else {
                text_input.stop(window);
            }
        }
        let mut frame = if prompt.showing() && !menu.is_open() {
            // The console in place of the HUD, as a menu would be, the game
            // pushed up above it.
            let blink = fps_since.elapsed().as_millis() % 1000 < 600;
            let ui = &game.cfg.ui;
            let camera = crate::ui::camera(&crate::ui::Layout::of(ui, aspect), ui.slant, ui.depth as f32);
            let mut frame = game.compose(accumulator / DT, aspect, scene, Some((crate::ui::prompt_panel(&prompt, blink, ui, aspect), Vec::new(), camera)));
            frame.view_proj = glam::Mat4::from_translation(glam::Vec3::new(0.0, 2.0 * crate::ui::prompt_lift(&prompt), 0.0)) * frame.view_proj;
            frame
        } else {
            game.menu_frame(&menu, &settings, accumulator / DT, aspect, scene)
        };
        if attracting {
            crate::ui::press_start(&mut frame.ui, attract_time, &game.cfg.ui, aspect, &settings.language());
        }
        if settings.show_fps {
            crate::ui::fps(&mut frame.ui, fps.round() as u32, &game.cfg.ui, aspect);
        }
        // The network's lines at the top: who went quiet, who paused; on
        // leaving, why.
        let mut said = net.as_mut().map(|n| n.notices(&settings.language())).unwrap_or_default();
        if let Some((why, at)) = &parting {
            if at.elapsed().as_secs() < 6 {
                said.push(why.clone());
            } else {
                parting = None;
            }
        }
        if !said.is_empty() {
            // Over a menu low, above its hint and the host's help.
            let low = menu.is_open().then(|| 14.0 + menu.net.as_ref().filter(|_| menu.page() == Some(Page::Pause)).map_or(0, |n| n.help.len()) as f32 * 9.0);
            crate::ui::notice(&mut frame.ui, &said, &game.cfg.ui, aspect, low);
        }

        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                renderer.render(device, queue, &mut encoder, &view, &frame);
                queue.submit([encoder.finish()]);
                queue.present(texture);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(device, config);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
            wgpu::CurrentSurfaceTexture::Validation => bail!("surface validation error"),
        }

        fps_frames += 1;
        if fps_since.elapsed() >= Duration::from_millis(500) {
            fps = fps_frames as f32 / fps_since.elapsed().as_secs_f32();
            let s = renderer.stats;
            let title = match game.game.summary() {
                Some(hud) if playing => {
                    let replay = if watching.is_some() || attracting { " | replay" } else { "" };
                    format!("{}{replay} | {hud}{now_playing} | {fps:.0} fps", game.cfg.title)
                }
                _ if !playing => format!("{} | {fps:.0} fps", game.cfg.title),
                _ => format!(
                    "voxcade - {} | {fps:.0} fps | chunks {}/{} | {}k verts | {} particles",
                    game.cfg.title,
                    s.drawn,
                    s.chunks,
                    s.vertices / 1000,
                    game.game.sim().particles.len()
                ),
            };
            let _ = window.set_title(&title);
            fps_frames = 0;
            fps_since = Instant::now();
        }
    }
    if let Some(n) = &mut net {
        n.keep_run(&game, &runs);
        n.leave();
    }
    if watching.is_none() && !attracting {
        runs.end(&game);
    }
    // The sound closed apart, and SDL not quit: its audio may hang in a
    // sound server gone away, and the window's end must not wait for it
    // (the program's end takes everything).
    drop(speaker);
    std::mem::forget(sdl);
    Ok(())
}

/// The title's menu: CONTINUE first when a save slot holds a save (the
/// newest).
fn title_menu<G: Game>(game: &Session<G>) -> Menu {
    let mut menu = Menu::title().continuing(game.slots.newest());
    menu.network = game.cfg.players > 1;
    menu.internet = crate::net::quic::matchmaker_known();
    menu
}

/// The pause page; SAVE and LOAD where the game has save slots and someone
/// plays (not while a replay is watched).
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

/// Menu steering from a key: the arrows and WASD, Enter or Space to choose,
/// Escape or Backspace to go back, the pause keys.
fn key_nav(code: Scancode, settings: &Settings) -> Option<Nav> {
    Some(match code {
        Scancode::Up | Scancode::W => Nav::Up,
        Scancode::Down | Scancode::S => Nav::Down,
        Scancode::Left | Scancode::A => Nav::Left,
        Scancode::Right | Scancode::D => Nav::Right,
        Scancode::Return | Scancode::KpEnter | Scancode::Space => Nav::Confirm,
        Scancode::Escape | Scancode::Backspace => Nav::Back,
        _ if settings.keys.action_of(code.name()) == Some(PAUSE) => Nav::Pause,
        _ => return None,
    })
}

fn is_direction(nav: Nav) -> bool {
    matches!(nav, Nav::Up | Nav::Down | Nav::Left | Nav::Right)
}

/// Which letters a kind of gamepad prints on its buttons.
fn pad_style(kind: sdl3::gamepad::GamepadType) -> PadStyle {
    use sdl3::gamepad::GamepadType::*;
    match kind {
        NintendoSwitchPro | NintendoSwitchJoyconLeft | NintendoSwitchJoyconRight | NintendoSwitchJoyconPair => PadStyle::Nintendo,
        PS3 | PS4 | PS5 => PadStyle::PlayStation,
        _ => PadStyle::Xbox,
    }
}

/// Menu steering from a gamepad button: d-pad, the bottom button to
/// choose, the right one or Back to go back, Start.
fn pad_nav(button: Button) -> Option<Nav> {
    Some(match button {
        Button::DPadUp => Nav::Up,
        Button::DPadDown => Nav::Down,
        Button::DPadLeft => Nav::Left,
        Button::DPadRight => Nav::Right,
        Button::South => Nav::Confirm,
        Button::East | Button::Back => Nav::Back,
        Button::Start => Nav::Pause,
        _ => return None,
    })
}

/// The song a number key picks: 1 the first.
fn song_key(code: Scancode) -> Option<usize> {
    const KEYS: [Scancode; 9] = [
        Scancode::_1,
        Scancode::_2,
        Scancode::_3,
        Scancode::_4,
        Scancode::_5,
        Scancode::_6,
        Scancode::_7,
        Scancode::_8,
        Scancode::_9,
    ];
    KEYS.iter().position(|&k| k == code)
}

/// A fresh run begun from `--scenario` and `--do`: what the lines answer
/// printed; a mistake printed too (a scenario's stops the game, and shows
/// over it until mended).
fn begun<G: Game>(begin: &crate::session::Begin, game: &mut Session<G>) {
    if let Err(e) = begin.apply(game, |line, said| println!("> {line}\n{said}")) {
        eprintln!("{e:#}");
    }
}

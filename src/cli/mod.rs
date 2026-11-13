//! The command line every game gets: run, shot, check, film, sounds,
//! music ... for game folders of one kind. The `voxcade` program is this
//! for Lua games; a game written in Rust calls `main` with its own type
//! (`examples/rust_digger`).
//!
//! One table, `table::COMMANDS`, says every command and every option it takes:
//! the parsers take nothing it does not list (`known`), and the help is
//! written from it, so the two cannot drift (tests
//! `the_help_says_every_option_the_parsers_take`,
//! `the_parsers_take_every_option_the_help_says`).

pub mod crash;
mod parse;
mod table;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::game::Game;
use crate::{app, headless, pads, replay};
use parse::{Call, CheckHow, EditMode, NetTest, SoundsDo, parse};
use table::{all_help, short_help, usage_of};

/// This program's file. Rebuilt while running (the IDE, `voxcade mcp`),
/// Linux names the running one `voxcade (deleted)`: the new one at the
/// same place is meant.
pub fn program() -> std::io::Result<PathBuf> {
    Ok(rebuilt(std::env::current_exe()?))
}

/// The commands the program answers to (`voxcade --help`'s).
pub fn command_names() -> Vec<&'static str> {
    table::COMMANDS.iter().map(|c| c.name).collect()
}

/// Whether the program takes the command line `args` (the words after
/// `voxcade`), read as `main` reads it and nothing done: the docs'
/// examples are tested so (`tests/docs_names.rs`).
pub fn takes(args: &[String]) -> Result<()> {
    match parse(args)? {
        Call::Unknown(word) => bail!("{word} is no command of voxcade's"),
        _ => Ok(()),
    }
}

/// Numbers as Lua writes and reads them ("1.5"), whatever language the
/// system speaks: the C library's locale for numbers is "C" from the start.
/// Lua's `tostring` and `string.format` go through it, and two machines
/// playing one game must write the same text (docs/multiplayer.md).
fn numbers_as_written() {
    // SAFETY: the program's first step, before any other thread runs.
    unsafe {
        libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
    }
}

/// The program at `exe`, or the one built anew in its place.
fn rebuilt(exe: PathBuf) -> PathBuf {
    match exe.to_str().and_then(|s| s.strip_suffix(" (deleted)")).map(PathBuf::from) {
        Some(new) if !exe.exists() && new.exists() => new,
        _ => exe,
    }
}

/// Reads the command line and does what it says, with game folders of
/// kind `G`; without arguments it runs `default` (a game's own folder) in
/// a window, or opens the launcher.
pub fn main<G: Game>(default: Option<&Path>) -> Result<()> {
    numbers_as_written();
    crate::desktop::name_app();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty()
        && let Some(dir) = default
    {
        crash::install(true);
        return app::run::<G>(dir, None, Default::default(), None, None, false);
    }
    let call = parse(&args)?;
    // A crash in a window is reported, not silent (`crash`); the commands
    // without one say it on the terminal as ever.
    if call.opens_a_window() {
        crash::install(true);
    }
    match call {
        // Started by a double click: the launcher (the IDE's start page).
        Call::Launcher => {
            crate::desktop::keep_entry();
            crate::ide::run::<G>(None, false)
        }
        Call::Run { dir, begin, net, keys, maker } => app::run::<G>(&opened(&dir)?, None, begin, net, keys.as_deref(), maker),
        Call::Replay { dir, file } => {
            let dir = opened(&dir)?;
            app::run::<G>(&dir, Some(replay::Replay::load(&file)?), Default::default(), None, None, false)
        }
        Call::Replays { dir } => replay::list::<G>(&opened(&dir)?),
        Call::Ide { dir, opts, shot, remote } => {
            let dir = dir.as_deref().map(game_dir).transpose()?;
            if shot { crate::ide::shot::<G>(dir.as_deref(), &opts) } else { crate::ide::run::<G>(dir.as_deref(), remote) }
        }
        Call::Edit { dir, opts, mode } => {
            let dir = opened(&dir)?;
            match mode {
                EditMode::Shot => crate::editor::shot::<G>(&dir, &opts),
                EditMode::Cells => crate::editor::change_cells(&dir, &opts),
                EditMode::Scatter => crate::editor::scatter_level::<G>(&dir, &opts),
                EditMode::Window => crate::editor::run::<G>(&dir, opts.level.as_deref(), opts.block.as_deref(), opts.things, opts.model.as_deref()),
            }
        }
        Call::ImportMap { dir, source, file, over } => {
            println!("{}", crate::tiled::import_into(&game_dir(&dir)?, &source, &file, over)?);
            Ok(())
        }
        Call::ImportModels { dir, sources, into } => import_models(&game_dir(&dir)?, &sources, into.as_deref()),
        Call::Upgrade { dir, dry } => {
            for line in crate::upgrade::upgrade(&game_dir(&dir)?, dry)? {
                println!("{line}");
            }
            Ok(())
        }
        Call::Update { install } => update(install),
        Call::Version => {
            println!("voxcade {}", crate::update::VERSION);
            Ok(())
        }
        Call::Help(what) => {
            match what.as_deref() {
                None => print!("{}", short_help()),
                Some("all") => print!("{}", all_help()),
                Some(command) => match usage_of(command) {
                    Some(usage) => print!("{usage}"),
                    None => bail!("{command} is no command of voxcade's (voxcade --help lists them)"),
                },
            }
            Ok(())
        }
        Call::Usage(command) => {
            print!("{}", usage_of(&command).unwrap_or_default());
            Ok(())
        }
        Call::Shot { dir, opts } => headless::shot::<G>(&opened(&dir)?, &opts),
        Call::Check { dir, how } => check::<G>(&opened(&dir)?, how),
        Call::Inspect { dir, frames, input, seats, at, begin, net, record } => {
            let dir = opened(&dir)?;
            match net {
                Some(net) => headless::inspect_net::<G>(&dir, frames, &input, &begin, &net, record.as_deref()),
                None => headless::inspect::<G>(&dir, frames, &input, &seats, at, &begin),
            }
        }
        Call::Scenarios { dir } => scenarios::<G>(&opened(&dir)?),
        Call::Sheet { dir, opts } => headless::sheet::<G>(&opened(&dir)?, &opts),
        Call::Film { dir, opts } => headless::film::<G>(&opened(&dir)?, &opts),
        Call::Sounds { dir, out, only, what } => {
            let dir = game_dir(&dir)?;
            match what {
                SoundsDo::List => headless::sound_list::<G>(&dir),
                SoundsDo::ToFiles => headless::sounds_to_files::<G>(&dir),
                SoundsDo::Write => headless::sounds::<G>(&dir, &out, only.as_deref()),
            }
        }
        Call::Music { first, out, seconds } => {
            // A song file by itself, or a game's first song.
            if first.is_file() && first.extension().is_some_and(|e| e == "mml") {
                headless::song(&first, None, &out, seconds)
            } else {
                headless::music::<G>(&game_dir(&first)?, &out, seconds)
            }
        }
        Call::Kinds { dir } => {
            let session = crate::session::Session::<G>::load(&opened(&dir)?)?;
            for line in session.game.numbers() {
                println!("{line}");
            }
            Ok(())
        }
        Call::Api { filter, lua } => api(filter.as_deref(), lua.as_deref()),
        Call::Mcp { remote } => crate::mcp::serve(remote),
        Call::Rooms { dir } => rooms::<G>(&opened(&dir)?),
        Call::Pads { seconds } => pads::pads(seconds),
        Call::Soak { seeds, digest } => soak(seeds, digest.as_deref()),
        Call::Nettest { reach, guests, seconds, size, what } => nettest(reach, guests, seconds, size, what),
        Call::Unknown(other) => {
            eprintln!("{other} is no command of voxcade's\n");
            eprint!("{}", short_help());
            std::process::exit(2);
        }
    }
}

/// `check`: every replay as recorded, recorded anew, refitted, or played
/// going back again and again; a replay that does not end the program
/// with 1.
fn check<G: Game>(dir: &Path, how: CheckHow) -> Result<()> {
    let fine = match how {
        CheckHow::AsRecorded => replay::check::<G>(dir, false)?,
        CheckHow::Update => replay::check::<G>(dir, true)?,
        CheckHow::Refit => replay::refit::<G>(dir)?,
        CheckHow::Rollback(every) => replay::check_going_back::<G>(dir, every)?,
    };
    if !fine {
        std::process::exit(1);
    }
    Ok(())
}

/// `scenarios`: each of the game's with what it is about.
fn scenarios<G: Game>(dir: &Path) -> Result<()> {
    let mut session = crate::session::Session::<G>::load(dir)?;
    let scenarios = session.scenarios()?;
    if scenarios.is_empty() {
        println!("no scenarios yet: scenarios/NAME.lua in the game folder (docs/game-model.md, \"Scenarios and the console\")");
    }
    for (name, about) in scenarios {
        println!("{name:<24} {about}");
    }
    Ok(())
}

/// `rooms`: the game's open rooms at the matchmaker.
fn rooms<G: Game>(dir: &Path) -> Result<()> {
    let session = crate::session::Session::<G>::load(dir)?;
    let (app, version) = crate::network::app_of(&session)?;
    anyhow::ensure!(crate::net::quic::matchmaker_known(), "no matchmaker known (VOXCADE_MATCHMAKER and VOXCADE_RELAY, or ours in src/net/quic.rs)");
    let quic = crate::net::quic::Quic::start(crate::net::quic::reach())?;
    quic.list(matchmaker_protocol::Search { app: app.clone(), version: version.clone(), ..Default::default() });
    let start = std::time::Instant::now();
    loop {
        for event in quic.drain() {
            match event {
                crate::net::quic::Event::Rooms(rooms) => {
                    println!("{} open room{} of {app} {version}", rooms.rooms.len(), if rooms.rooms.len() == 1 { "" } else { "s" });
                    for room in rooms.rooms {
                        let name = room.attributes.get("name").cloned().unwrap_or_default();
                        println!("{}  {:<20} {:<16} {}/{}  {} s", room.code, room.title, name, room.taken, room.seats, room.age);
                    }
                    return Ok(());
                }
                crate::net::quic::Event::Trouble(t) => bail!("{t}"),
                _ => {}
            }
        }
        anyhow::ensure!(start.elapsed() < std::time::Duration::from_secs(20), "the matchmaker did not answer");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// `import` of MagicaVoxel models: each a .vvx in the game.
fn import_models(dir: &Path, sources: &[PathBuf], into: Option<&str>) -> Result<()> {
    for vox in sources {
        let name = match into {
            Some(name) => PathBuf::from(name),
            None => PathBuf::from(vox.file_name().context("a model's file")?).with_extension(crate::vvx::EXTENSION),
        };
        anyhow::ensure!(crate::vvx::is_model(&name), "{}: a model of the game is a .vvx", name.display());
        println!("{}", crate::upgrade::bring_in(vox, &dir.join(name), false)?);
    }
    Ok(())
}

/// `update`: whether a newer version is there, and with `install` put in
/// place.
fn update(install: bool) -> Result<()> {
    match crate::update::look()? {
        None => println!("voxcade {} is the newest version", crate::update::VERSION),
        Some(release) if !install => println!("voxcade {} is there (this is {}): {}\nvoxcade update --install puts it in place", release.version, crate::update::VERSION, release.notes),
        Some(release) => {
            let dir = crate::update::install_dir().context("this program does not lie in a Voxcade folder (its games beside it): fetch the new version from voxca.de")?;
            crate::update::install(&release, &dir, &|_| {})?;
            let (generations, bytes) = crate::update::kept(&dir);
            println!("voxcade {} is in place in {}; what was there before lies in before-update/{}", release.version, dir.display(), crate::update::VERSION);
            println!("before-update keeps {generations} generation{} of what updates replaced, {:.1} MB: delete the ones you no longer need", if generations == 1 { "" } else { "s" }, bytes as f64 / 1e6);
        }
    }
    Ok(())
}

/// `api`: the names containing `filter`, or the definitions for editors
/// written to `lua`.
fn api(filter: Option<&str>, lua: Option<&Path>) -> Result<()> {
    if let Some(file) = lua {
        if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, crate::lua::api::definitions())?;
        println!("wrote {}", file.display());
        return Ok(());
    }
    print!("{}", crate::lua::api::text(filter));
    Ok(())
}

/// The network's soak (docs/multiplayer.md, stage 4): machines on
/// simulated networks play the same ticks; the digest is compared between
/// systems.
fn soak(seeds: u64, digest: Option<&Path>) -> Result<()> {
    let started = std::time::Instant::now();
    let sum = format!("{:016x}", crate::net::soak::soak(seeds));
    println!("{seeds} seeds played alike in {:.1} s: digest {sum}", started.elapsed().as_secs_f32());
    if let Some(path) = digest {
        std::fs::write(path, format!("{sum}\n")).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// `nettest`: the iroh trial here, or one machine's side of it.
fn nettest(reach: crate::net::quic::Reach, guests: usize, seconds: u32, size: usize, what: NetTest) -> Result<()> {
    let fine = match what {
        NetTest::Here => crate::net::trial::run(reach, guests, seconds, size)?,
        NetTest::Host => return crate::net::trial::host(reach),
        NetTest::Join(ticket) => crate::net::trial::join(&ticket, reach, seconds, size)?,
    };
    if !fine {
        std::process::exit(1);
    }
    Ok(())
}

/// What went wrong in plain words, in the system's language (English,
/// German or French), with what the program itself said below.
pub fn plainly(error: &anyhow::Error, languages: &[String]) -> String {
    let said = format!("{error:#}");
    let lang = languages.iter().map(String::as_str).find(|l| matches!(*l, "en" | "de" | "fr")).unwrap_or("en");
    let graphics = ["no suitable GPU", "creating surface", "creating window", "request_device"].iter().any(|s| said.contains(s));
    if said.contains(crate::window::DEVICE_LOST) {
        let plain = match lang {
            "de" => "Die Grafikkarte hat aufgehört zu zeichnen (ihr Treiber wurde wohl neu gestartet), und Voxcade muss aufhören. Starte es einfach neu; geschieht es öfter, hilft oft ein neuer Grafiktreiber.",
            "fr" => "La carte graphique a cessé de dessiner (son pilote a sans doute redémarré) et Voxcade doit s'arrêter. Relancez-le ; si cela revient souvent, un pilote graphique plus récent aide souvent.",
            _ => "The graphics card stopped drawing (its driver was likely restarted), and Voxcade has to stop. Just start it again; if it happens often, a newer graphics driver often helps.",
        };
        let it_said = match lang {
            "de" => "Das Programm meldet",
            "fr" => "Le programme dit",
            _ => "The program says",
        };
        return format!("{plain}\n\n{it_said}: {said}");
    }
    let plain = match (graphics, lang) {
        (true, "de") => "Voxcade findet keine Grafik, mit der es zeichnen kann.\n\nEs braucht eine Grafikkarte mit Vulkan oder, unter Windows, DirectX 12. Oft hilft ein neuer Grafiktreiber. Mehr dazu: voxca.de/de/install.html",
        (true, "fr") => "Voxcade ne trouve pas de carte graphique avec laquelle dessiner.\n\nIl lui faut une carte graphique avec Vulkan ou, sous Windows, DirectX 12. Un pilote graphique plus récent aide souvent. Plus d'aide : voxca.de/install.html",
        (true, _) => "Voxcade found no graphics it can draw with.\n\nIt needs a graphics card with Vulkan or, on Windows, DirectX 12. A newer graphics driver often helps. More help: voxca.de/install.html",
        (false, "de") => "Voxcade ist auf einen Fehler gestoßen und muss aufhören.",
        (false, "fr") => "Voxcade a rencontré une erreur et doit s'arrêter.",
        (false, _) => "Voxcade ran into a mistake and has to stop.",
    };
    let it_said = match lang {
        "de" => "Das Programm meldet",
        "fr" => "Le programme dit",
        _ => "The program says",
    };
    format!("{plain}\n\n{it_said}: {said}")
}

/// Whether the command line `args` (the program's name left out) was a
/// window's (the launcher, a game, a replay, the IDE, an editor), where a
/// mistake is said in a window too. A mistake in the command line itself
/// was made at a terminal, and a command without a window (`edit --fill`,
/// `--clear`, `--scatter`, any `--shot`) never shows one: a script or an
/// agent would hang on it.
fn opens_a_window(args: &[String]) -> bool {
    parse(args).is_ok_and(|call| call.opens_a_window())
}

/// Says `error` in a window of its own, if the program was started for a
/// window (`opens_a_window`) and not to draw a picture or work without one.
pub fn tell_in_a_window(error: &anyhow::Error) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if opens_a_window(&args) {
        let message = plainly(error, &crate::settings::system_languages());
        let _ = sdl3::messagebox::show_simple_message_box(sdl3::messagebox::MessageBoxFlag::ERROR, "Voxcade", &message, None);
    }
}

/// The game folder `dir`, checked to be for this Voxcade
/// (`upgrade::opens`): one made for another is a plain mistake.
fn opened(dir: &Path) -> Result<PathBuf> {
    let dir = game_dir(dir)?;
    crate::upgrade::opens(&dir)?;
    Ok(dir)
}

/// The game folder `dir`, checked to be a folder.
fn game_dir(dir: &Path) -> Result<PathBuf> {
    if !dir.is_dir() {
        bail!("{} is no game folder", dir.display());
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::table::*;
    use super::*;

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|w| w.to_string()).collect()
    }

    /// What follows each command's name before its options in the tests:
    /// a sample of its forms' words.
    fn sample(command: &str) -> &'static [&'static str] {
        match command {
            "replay" | "film" => &["g", "r.toml"],
            "sheet" => &["g", "hero"],
            "import" => &["g", "map.json"],
            "update" | "version" | "help" | "api" | "mcp" | "pads" | "soak" | "nettest" => &[],
            _ => &["g"],
        }
    }

    /// Every option given a word where it wants a number (or numbers, or a
    /// number after a `--by`) says which option it is, what it wants and
    /// what it got, never Rust's bare "invalid digit found in string".
    #[test]
    fn every_number_option_says_what_it_wants() {
        let bare = ["invalid digit", "invalid float", "cannot parse", "number too large", "number too small"];
        let mut said_number = 0;
        for c in COMMANDS {
            for o in c.opts.iter().filter(|o| !o.value.is_empty()) {
                // The value's words, each placeholder a word in turn, the others 1.
                let tokens: Vec<&str> = o.value.split_whitespace().filter(|t| !t.starts_with('[') && !t.ends_with(']') && *t != "|").collect();
                for wrong in 0..tokens.len() {
                    if tokens[wrong].starts_with("--") {
                        continue;
                    }
                    let mut line: Vec<&str> = vec![c.name];
                    line.extend(sample(c.name));
                    line.push(o.name);
                    // A fitting number for the others: as many as the placeholder's commas say.
                    let fit = |t: &str| match t.matches(',').count() {
                        0 if t == "WxH" => "1x1",
                        0 => "1",
                        1 => "1,1",
                        2 => "1,1,1",
                        _ => "1,1,1,1",
                    };
                    line.extend(tokens.iter().enumerate().map(|(i, t)| if t.starts_with("--") { t } else if i == wrong { "abc" } else { fit(t) }));
                    let Err(e) = parse(&words(&line)) else { continue };
                    let said = format!("{e:#}");
                    assert!(!bare.iter().any(|b| said.contains(b)), "{line:?}: {said}");
                    if said.contains(" wants ") {
                        said_number += 1;
                        assert!(said.contains("'abc'") && (said.contains(o.name) || tokens.iter().any(|t| t.starts_with("--") && said.contains(t))), "{line:?}: {said}");
                    }
                }
            }
        }
        assert!(said_number > 20, "only {said_number} options said what they want");
        let said = format!("{:#}", parse(&words(&["inspect", "g", "--frames", "abc"])).err().unwrap());
        assert_eq!(said, "--frames N wants a whole number (0 or more), not 'abc'");
        let said = format!("{:#}", parse(&words(&["soak", "many"])).err().unwrap());
        assert_eq!(said, "soak SEEDS wants a whole number (0 or more), not 'many'");
        let said = format!("{:#}", parse(&words(&["edit", "g", "--held", "1,x,3"])).err().unwrap());
        assert_eq!(said, "--held X,Y,Z wants whole numbers apart by commas, not '1,x,3'");
    }

    /// `inspect --record` records a network game's run; without the
    /// network it would write nothing, so it is refused, saying what does.
    #[test]
    fn inspect_records_only_over_the_network() {
        let said = format!("{:#}", parse(&words(&["inspect", "g", "--record", "r.toml"])).err().expect("refused"));
        assert!(said.contains("--host or --join") && said.contains("shot --record"), "{said}");
        assert!(parse(&words(&["inspect", "g", "--host", "26500", "--record", "r.toml"])).is_ok());
        assert!(parse(&words(&["inspect", "g", "--join", "127.0.0.1", "--record", "r.toml"])).is_ok());
    }

    /// The short help lists every command the program answers to, each
    /// with a line, a person's first; each has its usage (`voxcade COMMAND
    /// --help`), and every word of the table is a command the parser takes.
    #[test]
    fn the_help_lists_every_command() {
        let help = short_help();
        assert!(help.contains("Without anything after it, voxcade opens the launcher (a window)"), "{help}");
        assert!(help.find("TO PLAY AND MAKE GAMES").unwrap() < help.find("\n  run ").unwrap());
        assert!(help.find("\n  run ").unwrap() < help.find("TO LOOK, CHECK").unwrap() && help.find("TO LOOK, CHECK").unwrap() < help.find("\n  shot ").unwrap());
        for c in COMMANDS {
            assert!(help.contains(&format!("\n  {} ", c.name)), "{} is missing from voxcade --help", c.name);
            let usage = usage_of(c.name).unwrap_or_else(|| panic!("{} has no usage", c.name));
            assert!(usage.starts_with(&format!("voxcade {}\n", c.name)) || usage.starts_with(&format!("voxcade {} ", c.name)), "{}: {usage}", c.name);
            let mut line = vec![c.name];
            line.extend(sample(c.name));
            assert!(!matches!(parse(&words(&line)), Ok(Call::Unknown(_))), "{} is in the table and not taken", c.name);
        }
        assert!(matches!(parse(&words(&["nothing"])), Ok(Call::Unknown(_))));
        assert!(matches!(parse(&words(&["shot", "--help"])), Ok(Call::Usage(c)) if c == "shot"));
        assert!(matches!(parse(&[]), Ok(Call::Launcher)));
        let shot = usage_of("shot").unwrap();
        assert!(shot.contains("simulates N ticks") && shot.contains("INPUT SCRIPT") && !shot.contains("voxcade check"), "{shot}");
        assert!(usage_of("replay").unwrap().lines().all(|l| !l.contains("voxcade replays")));
        assert!(usage_of("nothing").is_none());
        let all = all_help();
        assert!(COMMANDS.iter().all(|c| all.contains(&usage_body(c))) && all.matches("INPUT SCRIPT:").count() == 1, "{all}");
        assert!(all.lines().all(|l| l.chars().count() <= WIDTH), "a line of help all is wider than {WIDTH}");
    }

    /// Every option a parser takes is in its command's help, with what it
    /// takes, and under its heading: what a person uses first, the
    /// pictures' and tests' options after it. Every screen of `shot
    /// --screen` is listed, and the parser takes only those.
    #[test]
    fn the_help_says_every_option_the_parsers_take() {
        for c in COMMANDS {
            let usage = usage_of(c.name).unwrap();
            let lines: Vec<&str> = usage.lines().collect();
            let mut last = 0;
            for o in c.opts {
                let head = head(o);
                let at = lines.iter().position(|l| *l == format!("  {head}") || l.starts_with(&format!("  {head}  "))).unwrap_or_else(|| panic!("{} {}: not in its help:\n{usage}", c.name, o.name));
                let heading = lines[..at].iter().rev().find(|l| !l.starts_with(' ') && l.ends_with(':'));
                assert_eq!(heading, Some(&o.group.heading()), "{} {}: under its heading", c.name, o.name);
                last = last.max(Group::ALL.iter().position(|g| *g == o.group).unwrap());
                assert_eq!(Group::ALL.iter().position(|g| *g == o.group).unwrap(), last, "{} {}: the groups in order, a person's first", c.name, o.name);
            }
            // Anything else is refused, saying so.
            let mut line = vec![c.name];
            line.extend(sample(c.name));
            line.push("--no-such-option");
            if !matches!(c.name, "help") {
                let said = parse(&words(&line)).err().map(|e| format!("{e:#}")).unwrap_or_default();
                assert!(said.starts_with("unknown option --no-such-option"), "{}: {said}", c.name);
            }
        }
        let shot = usage_of("shot").unwrap();
        for (screen, _) in headless::SCREENS {
            assert!(shot.contains(&format!("\n  {screen} ")), "{screen} is not in shot's help");
            assert!(parse(&words(&["shot", "g", "--screen", screen])).is_ok(), "{screen}");
            assert!(headless::screen_menu(Some(screen), "Game", 2, true).is_ok(), "{screen}: not begun");
        }
        assert!(parse(&words(&["shot", "g", "--screen", "nowhere"])).is_err());
        // The edit's synopsis is what a person uses: no picture's options.
        let edit = usage_of("edit").unwrap();
        let synopsis = &edit[..edit.find("\n\n").unwrap()];
        assert!(synopsis.contains("[--level FILE]") && !synopsis.contains("--hover") && !synopsis.contains("--shot"), "{synopsis}");
        assert!(!usage_of("shot").unwrap()[..shot.find("\n\n").unwrap()].contains("--screen"));
    }

    /// Every option the help says is one its parser takes: a value where
    /// the help says one, none for a switch (a parser that had no arm for
    /// it would call it unknown).
    #[test]
    fn the_parsers_take_every_option_the_help_says() {
        for c in COMMANDS {
            for o in c.opts {
                let mut line = vec![c.name];
                line.extend(sample(c.name));
                line.push(o.name);
                let alone = parse(&words(&line)).err().map(|e| format!("{e:#}"));
                if o.value.is_empty() {
                    let said = alone.unwrap_or_default();
                    assert!(!said.contains("unknown option") && !said.contains("needs a value"), "{} {}: {said}", c.name, o.name);
                    continue;
                }
                if !o.value.starts_with('[') {
                    assert!(alone.is_some(), "{} {}: taken without its value", c.name, o.name);
                }
                line.push("1");
                let said = parse(&words(&line)).err().map(|e| format!("{e:#}")).unwrap_or_default();
                assert!(!said.starts_with(&format!("unknown option {}", o.name)), "{} {}: {said}", c.name, o.name);
            }
        }
        // Old names say the new one.
        let said = format!("{:#}", parse(&words(&["edit", "g", "--turn", "x", "--by", "90"])).err().unwrap());
        assert_eq!(said, "--turn is called --rotate now");
    }

    /// An option with one value given twice is refused, naming it; those
    /// the table lets come again are taken each time; a switch twice is
    /// the switch. Every repeatable option is one some command takes.
    #[test]
    fn an_option_given_twice_is_refused_unless_it_may_come_again() {
        for line in [&["shot", "g", "--size", "1x1", "--size", "2x2"][..], &["run", "g", "--host", "--host", "1"], &["inspect", "g", "--input2", "a", "--input2", "b"], &["film", "g", "r.toml", "--out", "a.mp4", "--out", "b.mp4"]] {
            let said = parse(&words(line)).err().map(|e| format!("{e:#}")).unwrap_or_default();
            assert!(said.contains("is given twice"), "{line:?}: {said}");
        }
        assert!(parse(&words(&["shot", "g", "--do", "a = 1", "--do", "b = 2", "--input2", "a", "--input3", "b"])).is_ok());
        assert!(parse(&words(&["edit", "g", "--level", "l.toml", "--add", "a", "--at", "1,2,3", "--add", "b", "--at", "4,5,6", "--shot", "x.png"])).is_ok());
        assert!(parse(&words(&["run", "g", "--maker", "--maker"])).is_ok());
        for name in REPEATABLE {
            assert!(COMMANDS.iter().any(|c| c.opts.iter().any(|o| o.name == *name && !o.value.is_empty())), "{name}: no command takes it with a value");
        }
    }

    /// A mistake is said in a window only where the command opens one: a
    /// command that writes or draws without a window says it on the
    /// terminal (a modal box would hang a script or an agent), and so does
    /// a mistake in the command line itself.
    #[test]
    fn only_a_window_says_a_mistake_in_a_window() {
        for line in [
            &["edit", "g", "--model", "m.vvx", "--fill", "a", "--from", "0,0,0", "--to", "1,1,1"][..],
            &["edit", "g", "--model", "m.vvx", "--clear", "0,0,0", "--to", "1,1,1", "--dry"],
            &["edit", "g", "--level", "levels/a.toml", "--scatter", "1"],
            &["edit", "g", "--level", "levels/a.toml", "--scatter", "1", "--dry"],
            &["edit", "g", "--dry"],
            &["edit", "g", "--shot", "x.png"],
            &["edit", "g", "--no-such-option"],
            &["edit"],
            &["run", "g", "--no-such-option"],
            &["ide", "g", "--shot", "x.png"],
            &["shot", "g"],
            &["check", "g"],
            &["mcp"],
        ] {
            assert!(!opens_a_window(&words(line)), "{line:?}");
        }
        for line in [&[][..], &["run", "g"], &["replay", "g", "r.toml"], &["edit", "g"], &["edit", "g", "--model", "m.vvx"], &["ide"], &["ide", "g"]] {
            assert!(opens_a_window(&words(line)), "{line:?}");
        }
    }

    #[test]
    fn a_mistake_is_said_plainly_in_the_systems_language() {
        let none = anyhow::anyhow!("no suitable GPU");
        assert!(plainly(&none, &["de".into()]).starts_with("Voxcade findet keine Grafik"));
        assert!(plainly(&none, &["ja".into(), "fr".into()]).contains("carte graphique"));
        let lost = anyhow::anyhow!("{} (DeviceLost: reset)", crate::window::DEVICE_LOST);
        assert!(plainly(&lost, &["fr".into()]).starts_with("La carte graphique a cessé"));
        assert!(plainly(&lost, &[]).starts_with("The graphics card stopped drawing"));
        let other = anyhow::anyhow!("games/x is no game folder");
        let said = plainly(&other, &[]);
        assert!(said.starts_with("Voxcade ran into a mistake") && said.ends_with("The program says: games/x is no game folder"), "{said}");
    }

    /// A program rebuilt while it runs is found in its place.
    #[test]
    fn a_rebuilt_program_is_found_in_its_place() {
        let dir = crate::scratch::Scratch::new("rebuilt");
        std::fs::create_dir_all(&dir).unwrap();
        let new = dir.join("voxcade");
        let gone = dir.join("voxcade (deleted)");
        assert_eq!(rebuilt(gone.clone()), gone, "nothing in its place yet");
        std::fs::write(&new, b"").unwrap();
        assert_eq!(rebuilt(gone), new);
        assert_eq!(rebuilt(new.clone()), new);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

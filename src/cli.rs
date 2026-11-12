//! The command line every game gets: run, shot, check, film, sounds,
//! music ... for game folders of one kind. The `voxcade` program is this
//! for the shooter; a game written in Rust calls `main` with its own type
//! (`examples/rust_digger`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::controls::PadStyle;
use crate::game::Game;
use crate::{app, headless, pads, replay};

const USAGE: &str = "\
voxcade - a voxel arcade engine

Without anything after it: the launcher, to play a game or make one.

USAGE:
  voxcade --version
  voxcade update [--install]
  voxcade run  <game-dir> [--scenario NAME] [--do LINE] [--host [PORT | internet] | --join ADDRESS[:PORT] | --join TICKET] [--input SCRIPT]
  voxcade replay <game-dir> <replay-file>
  voxcade replays <game-dir>
  voxcade shot <game-dir> [--frames N] [--input SCRIPT | --replay FILE] [--record FILE]
               [--screen NAME] [--at X,Y] [--shadows high|medium|low]
               [--pad xbox|nintendo|playstation] [--size WxH] [--out FILE]
               [--scenario NAME] [--do LINE]
  voxcade check  <game-dir> [--update | --refit | --rollback TICKS]
  voxcade inspect <game-dir> [--frames N] [--input SCRIPT] [--at X,Y] [--scenario NAME] [--do LINE]
                  [--host PORT | --host internet | --join ADDRESS[:PORT] | --join CODE|TICKET|any] [--record FILE]
  voxcade scenarios <game-dir>
  voxcade rooms  <game-dir>
  voxcade api    [NAME] [--lua FILE]
  voxcade kinds  <game-dir>
  voxcade soak   [SEEDS] [--digest FILE]
  voxcade nettest [internet | relayed | direct] [--guests N] [--seconds N] [--size BYTES]
  voxcade nettest host [internet | relayed | direct]
  voxcade nettest join TICKET [internet | relayed | direct] [--seconds N] [--size BYTES]
  voxcade sheet  <game-dir> <model> [--out FILE]
  voxcade import <game-dir> <tiled-map.json> [--file NAME] [--over]
  voxcade import <game-dir> <model.vox>... [--as PATH.vvx]
  voxcade upgrade <game-dir> [--dry]
  voxcade edit   <game-dir> [--level FILE] [--block NAME | --things | --model FILE [--part NAME [--parent NAME] [--joint X,Y,Z]]] [--stamp] [--shot FILE [--align]] [--view YAW,PITCH] [--size WxH]
                 [--choose NAME] [--game-view] [--picker] [--lights-off] [--add|--hang MODEL|light|place|area --at X,Y,Z]
                 [--move NAME --at X,Y,Z] [--turn NAME --by DEG] [--copy NAME --at X,Y,Z] [--remove NAME]
                 [--rename NAME --to NEW] [--light NAME [--radius N] [--colour #rrggbb] [--at X,Y,Z]] [--room-size X,Y,Z]
  voxcade ide    [<game-dir>] [--shot FILE] [--open PATH] [--console CMD] [--play TICKS] [--size WxH] [--remote]
  voxcade mcp    [--remote]
  voxcade film   <game-dir> <replay-file> [--from S] [--to S] [--fps N] [--size WxH]
                 [--max-mb N] [--out FILE.mp4|.webm|.gif]
  voxcade sounds <game-dir> [--out FILE] [--only NAME] | --list | --to-files
  voxcade music  <game-dir>|<song.mml> [--out FILE] [--seconds N]
  voxcade pads   [--seconds N]

run   opens the game in a window. Saving any file in the game folder
      reloads it while the game keeps running. Every run is recorded:
      runs of 10 s or more are kept in the config folder (the newest 20,
      see replays), F9 puts the run being played or the last one in the
      game's replays/ folder.
      --scenario begins every run there (scenarios/NAME.lua: a late
      situation set up in code), past the title; --do runs a line of Lua
      in the game before its first tick (again and again for more); the
      key left of 1 opens the console, a line of Lua between two ticks.
      Both are recorded with the run.
replay shows a replay in a window; F5 plays it again, Escape quits.
scenarios lists the game's scenarios (scenarios/NAME.lua) and what each
      is about; run, shot and inspect begin from one with --scenario.
replays lists the runs kept for a game.
update looks whether a newer version is there (the feed at voxca.de; the
      launcher does it too, unless switched off there), and with
      --install fetches it, checks it and puts it in place in the folder
      the program lies in; what was there before goes to before-update.
      VOXCADE_UPDATE_FEED names another feed (a file's path too).
rooms lists the game's open rooms across the internet at our matchmaker
      (../matchmaker): each one's code, title, host, seats taken, age.
      VOXCADE_RELAY, VOXCADE_RELAY_TOKEN and VOXCADE_MATCHMAKER name
      another.
shot  simulates N ticks without a window and writes one rendered frame
      as PNG. Default: 90 ticks, 1920x1080, shots/shot.png. --screen
      shows a menu instead: title, continue (the title with a save to go
      on from), settings, controls (N ticks into their animations), intro (the glide into the game, N ticks in) or pause
      (over the game after N ticks; save and load: its slot pages, the
      saves of the run in them), attract (a --replay as the title
      plays it), highscores (the table, a sample run in it), initials
      (a sample run's initials being entered), console (the console
      open over the game, the --do lines in it). --at starts the player at X,Y;
      --shadows sets the shadow quality (as in the settings); --pad
      shows the hints for a gamepad of that kind. --scenario and --do
      begin the run as for run. --replay
      plays a replay (to its end unless --frames) and says whether it
      runs as recorded; --record writes the run as a replay.
check plays every replay in the game's replays/ folder without a window
      and says whether each runs as recorded (the regression check after
      changing the engine). --update records their checks anew from their
      input, after a change to the game or engine that was meant. --refit
      plays them whatever data they were recorded on and, only if every
      one runs exactly as recorded, writes the data's new fingerprint into
      them: after a change to the files meant to leave the game as it was.
      --rollback TICKS goes back that many ticks again and again (every
      TICKS ticks) and plays on, as rollback over a network would: each
      replay must still come out as recorded; it says what keeping and
      bringing back the game cost.
soak plays network games of a stub game on simulated networks, up to
      eight machines on bad and hard ones (default 100 seeds): every
      machine must play the same ticks; the digest of every hash, the same
      on every system, goes to --digest FILE.
nettest tries network play across the internet (iroh, docs/multiplayer.md
      stage 6, step 2): a host and its guests (default 3) in this process,
      reaching each other over the internet (default), only through a
      relay, or only directly (a LAN without the internet), 60 datagrams a
      second each way (default 20 s, 1000 bytes); says what got through
      and how late, the biggest datagram, how soon a machine that vanished
      is noticed and what it does to the main loop's frames, each pass or
      fail. `nettest host` and `nettest join TICKET` try two machines.
inspect plays N ticks (default 60) without a window and prints how the
      game stands as JSON: tick, outcome, score, the report, the sounds,
      the error that stopped it, the things and the script's state.
      With --host PORT it hosts a network game in real time (UDP), playing
      N ticks, then says farewell once its guests have them all; with
      --join ADDRESS a guest joins it, plays the run so far through, takes
      a seat and plays its --input until the host leaves. --record writes
      the run (a guest's equals the host's). --host internet hosts across
      the internet (iroh): it prints its ticket, then its code from our
      server, and lets in every machine that asks; --join CODE or --join
      TICKET joins such a game. VOXCADE_REACH=relayed (only through a
      relay) or direct (no relay: a LAN without the internet) tries the
      other ways, VOXCADE_RELAY=URL another relay, VOXCADE_MATCHMAKER=KEY
      another matchmaker.
sheet draws one of the game's models (a kind's, or one in c.models: its
      file or name) alone as the game draws it, its light, camera angle
      and shading, into one PNG: five sides close up, every frame from
      the side, the five sides at the game's size. Default:
      shots/<model>_sheet.png.
import brings a Tiled map into a game once, as a level of voxel blocks
      (c.world.blocks): every tile it uses a block, its picture drawn out
      through the depth, unless the game has made that block already
      (blocks/ID.vvx); each tile layer a row of cells, the top layer in
      front (a map of one layer: what holds in front, decoration behind);
      what a block is from its tile's class in the tileset (solid, oneway,
      ladder, hazard; none: decoration); the objects as things. Writes
      level.toml (--file NAME), over one there only with --over.
      Given MagicaVoxel models (.vox), it brings each into the game as a
      model of Voxcade's own (.vvx, text), beside the others in the game
      folder (--as: another path there, one model), its parts hanging as
      they did; the .vox is left alone. Name the .vvx in the game's Lua.
upgrade brings a game made for an older Voxcade up to date: every step
      it needs (0.6.0: its models, figures and blocks become .vvx; its
      levels of blocks and maps .toml; its worlds and map prefabs .vvx too,
      their markers named in them instead of c.markers; the Lua naming the
      files follows), then
      c.voxcade in conf.lua says this version. It says what each step
      did; --dry only says what it would do. It commits nothing, and says
      first when the folder has changes git has not committed. A game for
      an older Voxcade does not open until then; one for a newer neither.
edit  opens the editor on a game's level: of voxel blocks, the level (its
      things with --things; another than the config's with --level FILE,
      levels/02_crypt.toml), or a block of it (--block); or on a model of
      the game, any size, its frames (--model FILE, its path in the game
      folder; a figure of parts, its objects named, opens whole: FIGURE,
      its parts by what they hang on, where each turns, what is wrong
      with the rig; a part chosen is shaped in the figure, its parent
      and joint set by hand). --shot draws it as it opens into a PNG
      instead of a window, writing nothing (--part NAME chooses a
      figure's part, --parent NAME and --joint X,Y,Z set its rig, the
      joint as the printed tree has it; the mouse in the middle of the
      view; --view turns the camera; --tool WORD chooses a tool, BOX and
      SPHERE holding a shape --held X,Y,Z big, --stamp setting it there,
      --shift as with Shift held (the tool is PICK);
      default 1920x1080). A level whose pieces are turned off the quarter
      turns, in a game that sets things on the grid, asks before it
      opens: ALIGN ALL or DON'T OPEN; --align answers ALIGN ALL for the
      shot (nothing written). A level without blocks (a room of pieces:
      models and lights set down, places, areas) opens among its pieces,
      as a level of blocks with PIECES; --shot tries changes there in
      turn, writing nothing: --add a model (its path) or a light, place
      or area --at a spot (--hang: left on the mouse there), --move, --turn (--by degrees), --copy,
      --remove, --rename, --light (a piece's: --radius, --colour, --at
      where it shines from), --room-size; then --choose one by its name,
      --game-view looks as the game's camera, --picker shows ADD MODEL's
      list, --lights-off the view without the lights.
ide   opens the IDE on a game (docs/ide.md): its logic and assets on the
      left, the one thing open in the main area, the console with your
      shell below; without one, its start page (games opened last, OPEN,
      NEW). --shot draws it as it opens into a PNG instead of a
      window (--open: that file, or help for HELP's list of every name,
      help:N its topic N; --console: CMD run in the console, waited
      for; --play: the game played in the main area, TICKS in, or with
      --open held behind the file; default 1920x1080). --remote lets
      another session type into the console and read it, and use the
      IDE's windows as a person does (`voxcade mcp --remote`:
      console_send, console_read, windows, window_shot, pointer,
      scroll, keys); without it the IDE refuses them all.
api   lists the Lua API (the names containing NAME); --lua writes the
      definitions for editors (LuaLS: completion and checks) to FILE,
      lua/voxcade.lua beside the program.
kinds prints the game's kinds of things and weapons with every number in
      effect, as conf.lua would write them: the defaults it leaves unset
      too (scaled to a kind's height), nothing hidden.
mcp   serves the engine's tools to an agent over MCP on stdin/stdout:
      shot (a frame as an image), check, run, things, api. Claude Code:
        claude mcp add voxcade -- /path/to/voxcade mcp
      --remote adds console_send and console_read (type into the IDE's
      console and read it) and windows, window_shot, pointer, scroll,
      keys (pictures of the IDE's windows, a synthetic mouse and
      keyboard), for a session testing the IDE (run the IDE with
      --remote too; never in the game's own .mcp.json).
film  renders a replay as a video with its music and sounds (.mp4: H.264
      and AAC, plays everywhere; .webm: VP9 and Opus; through ffmpeg) or
      as a GIF without sound (built in). --from/--to cut it (seconds),
      --max-mb keeps a video under that size (a chat's upload limit).
      Default: the whole replay, 1920x1080, 60 fps, shots/film.mp4; a GIF
      480x270, 20 fps, 10 s.
sounds writes every sound of the game one after another as a WAV file (or
      just one with --only), to listen to them without playing.
      Default: shots/sounds.wav. --list lists them: where each comes from
      (a file in sounds/, or built in), its length and peak. --to-files
      writes the c.sounds of an older conf.lua as files in sounds/ and
      says which lines to take out (sounds are files now).
pads  lists the controllers SDL sees (and whether it knows them as
      gamepads), then shows what they send for N seconds.
music writes the game's music (c.audio.music), or one song file, as a
      WAV file, once through or for N seconds. Default: shots/music.wav.

INPUT SCRIPT (shot): segments separated by ';', each 'keys:ticks', keys
joined by '+' from: left right up down jump fire grenade strafe none.
  example: --input \"right+fire:40;none:20;up+jump:15\"
";

/// Every command, with a line on what it does (`voxcade --help`); the
/// usage of each is in `USAGE` (`voxcade COMMAND --help`).
const COMMANDS: &[(&str, &str)] = &[
    ("run", "play a game in a window"),
    ("replay", "show a replay in a window"),
    ("replays", "list the runs kept for a game"),
    ("shot", "play a game without a window and write a picture of it (PNG)"),
    ("check", "play every replay of a game and say whether each runs as recorded"),
    ("inspect", "play a game without a window and print how it stands (JSON)"),
    ("scenarios", "list a game's scenarios (a run begun late)"),
    ("rooms", "list a game's open rooms across the internet"),
    ("api", "list the Lua API; --lua writes the definitions for editors"),
    ("kinds", "print a game's kinds of things and weapons with every number in effect"),
    ("sheet", "draw one of a game's models alone, as the game draws it"),
    ("import", "bring a Tiled map or MagicaVoxel models into a game"),
    ("upgrade", "bring a game made for an older Voxcade up to date"),
    ("edit", "open the level or model editor (a window)"),
    ("ide", "open the IDE (a window)"),
    ("mcp", "serve the engine's tools to an agent over MCP"),
    ("film", "render a replay as a video or a GIF"),
    ("sounds", "write a game's sounds as a WAV file"),
    ("music", "write a game's music as a WAV file"),
    ("pads", "list the controllers and show what they send"),
    ("soak", "play network games on simulated networks"),
    ("nettest", "try network play across the internet"),
    ("update", "look for a newer version; --install puts it in place"),
    ("version", "say which version this is (also --version)"),
    ("help", "this list (also --help); help all: everything at once"),
];

/// The short help: what `voxcade` alone does, and every command in a line.
fn short_help() -> String {
    let mut out = String::from(
        "voxcade - a voxel arcade engine\n\nWithout anything after it, voxcade opens the launcher (a window): play a game or make one.\nvoxcade COMMAND --help says how to use a command; voxcade help all says everything.\n\nCOMMANDS:\n",
    );
    let width = COMMANDS.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
    for (command, what) in COMMANDS {
        out += &format!("  {command:<width$}  {what}\n");
    }
    out
}

/// A command's usage, as `USAGE` has it: its lines in the list, what it
/// does, and how an input script is written where it takes one. None for
/// a word that is no command.
fn usage_of(command: &str) -> Option<String> {
    COMMANDS.iter().find(|(c, _)| *c == command)?;
    let word = if command == "version" { "--version" } else { command };
    let lines: Vec<&str> = USAGE.lines().collect();
    let mut out = String::new();
    // Its lines among USAGE's, each with the lines that go on under it.
    let mut taking = false;
    for line in &lines {
        if let Some(rest) = line.strip_prefix("  voxcade ") {
            taking = rest.split_whitespace().next() == Some(word);
        } else if !line.starts_with("    ") {
            taking = false;
        }
        if taking {
            out += &format!("{line}\n");
        }
    }
    // What it does: its paragraph, the word first.
    let mut taking = false;
    let mut said = String::new();
    for line in &lines {
        if line.starts_with(' ') {
            if taking {
                said += &format!("{line}\n");
            }
            continue;
        }
        taking = line.split_whitespace().next() == Some(command);
        if taking {
            said += &format!("{line}\n");
        }
    }
    if !said.is_empty() {
        out += &format!("\n{said}");
    }
    if out.contains("--input SCRIPT") && let Some(at) = USAGE.find("INPUT SCRIPT") {
        out += &format!("\n{}", &USAGE[at..]);
    }
    (!out.is_empty()).then_some(out)
}

/// Reads the command line and does what it says, with game folders of
/// kind `G`; without arguments it runs `default` (a game's own folder) in
/// a window, or shows how to use it.
/// This program's file. Rebuilt while running (the IDE, `voxcade mcp`),
/// Linux names the running one `voxcade (deleted)`: the new one at the
/// same place is meant.
pub fn program() -> std::io::Result<PathBuf> {
    Ok(rebuilt(std::env::current_exe()?))
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

pub fn main<G: Game>(default: Option<&Path>) -> Result<()> {
    numbers_as_written();
    crate::desktop::name_app();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty()
        && let Some(dir) = default
    {
        return app::run::<G>(dir, None, Default::default(), None, None);
    }
    // `voxcade COMMAND --help`: that command's usage.
    if let (Some(command), Some("--help" | "-h")) = (args.first(), args.get(1).map(String::as_str))
        && let Some(usage) = usage_of(command)
    {
        print!("{usage}");
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("run") => {
            let dir = opened(&args)?;
            let (mut begin, mut net, mut keys) = (crate::session::Begin::default(), None, None);
            let mut it = args[2..].iter().peekable();
            while let Some(flag) = it.next() {
                // --host with a port or without (26500 or the next free), or
                // across the internet.
                if flag == "--host" {
                    if it.next_if(|p| *p == "internet").is_some() {
                        net = Some(headless::Networked::HostNet);
                        continue;
                    }
                    let port = it.next_if(|p| p.parse::<u16>().is_ok()).map(|p| p.parse::<u16>().unwrap_or(crate::net::udp::PORT));
                    net = Some(headless::Networked::Host(port.unwrap_or(0)));
                    continue;
                }
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--join" => net = Some(headless::Networked::Join(value.clone())),
                    "--input" => keys = Some(value.clone()),
                    flag if begin.flag(flag, value) => {}
                    _ => bail!("unknown option {flag}"),
                }
            }
            app::run::<G>(&dir, None, begin, net, keys.as_deref())
        }
        Some("scenarios") => {
            let mut session = crate::session::Session::<G>::load(&opened(&args)?)?;
            let scenarios = session.scenarios()?;
            if scenarios.is_empty() {
                println!("no scenarios yet: scenarios/NAME.lua in the game folder (docs/game-model.md, \"Scenarios and the console\")");
            }
            for (name, about) in scenarios {
                println!("{name:<24} {about}");
            }
            Ok(())
        }
        Some("rooms") => {
            let session = crate::session::Session::<G>::load(&opened(&args)?)?;
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
        Some("replay") => {
            let dir = opened(&args)?;
            let file = args.get(2).context("missing the replay file")?;
            app::run::<G>(&dir, Some(replay::Replay::load(std::path::Path::new(file))?), Default::default(), None, None)
        }
        Some("replays") => replay::list::<G>(&opened(&args)?),
        Some("shot") => headless::shot::<G>(&opened(&args)?, &shot_options(&args[2..])?),
        Some("film") => {
            let dir = opened(&args)?;
            let replay = PathBuf::from(args.get(2).context("missing the replay file")?);
            let mut opts = headless::FilmOptions { replay, from: 0.0, to: None, fps: None, size: None, max_mb: None, out: PathBuf::from("shots/film.mp4") };
            let mut it = args[3..].iter();
            while let Some(flag) = it.next() {
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--from" => opts.from = value.parse().context("--from")?,
                    "--to" => opts.to = Some(value.parse().context("--to")?),
                    "--fps" => opts.fps = Some(value.parse().context("--fps")?),
                    "--max-mb" => opts.max_mb = Some(value.parse().context("--max-mb")?),
                    "--size" => {
                        let (w, h) = value.split_once('x').context("--size expects WxH")?;
                        opts.size = Some((w.parse().context("--size width")?, h.parse().context("--size height")?));
                    }
                    "--out" => opts.out = PathBuf::from(value),
                    other => bail!("unknown option {other}"),
                }
            }
            headless::film::<G>(&dir, &opts)
        }
        Some("check") => {
            let dir = opened(&args)?;
            let fine = match args.get(2).map(String::as_str) {
                None => replay::check::<G>(&dir, false)?,
                Some("--update") => replay::check::<G>(&dir, true)?,
                Some("--refit") => replay::refit::<G>(&dir)?,
                Some("--rollback") => {
                    let every = args.get(3).context("--rollback needs how many ticks to go back")?.parse().context("--rollback: a number of ticks")?;
                    replay::check_going_back::<G>(&dir, every)?
                }
                Some(other) => bail!("unknown option {other}"),
            };
            if !fine {
                std::process::exit(1);
            }
            Ok(())
        }
        Some("sheet") => {
            let dir = opened(&args)?;
            let model = args.get(2).filter(|a| !a.starts_with("--")).context("voxcade sheet <game-dir> <model>")?.clone();
            let stem = Path::new(&model).file_stem().map_or("model".into(), |s| s.to_string_lossy().to_string());
            let mut out = PathBuf::from(format!("shots/{stem}_sheet.png"));
            let mut it = args[3..].iter();
            while let Some(flag) = it.next() {
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--out" => out = PathBuf::from(value),
                    other => bail!("unknown option {other}"),
                }
            }
            headless::sheet::<G>(&dir, &headless::SheetOptions { model, out })
        }
        Some("inspect") => {
            let dir = opened(&args)?;
            let (mut frames, mut input, mut at, mut begin) = (60, String::new(), None, crate::session::Begin::default());
            let (mut seats, mut net, mut record) = (Vec::new(), None, None);
            let mut it = args[2..].iter();
            while let Some(flag) = it.next() {
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    flag if begin.flag(flag, value) => {}
                    "--frames" => frames = value.parse().context("--frames")?,
                    "--input" => input = value.clone(),
                    "--host" if value == "internet" => net = Some(headless::Networked::HostNet),
                    "--host" => net = Some(headless::Networked::Host(value.parse().context("--host PORT or internet")?)),
                    "--join" => net = Some(headless::Networked::Join(value.clone())),
                    "--record" => record = Some(PathBuf::from(value)),
                    flag if seat_input(flag).is_some() => seats.push((seat_input(flag).unwrap_or(0), value.clone())),
                    "--at" => {
                        let (x, y) = value.split_once(',').context("--at expects x,y")?;
                        at = Some([x.trim().parse().context("--at x")?, y.trim().parse().context("--at y")?]);
                    }
                    other => bail!("unknown option {other}"),
                }
            }
            match net {
                Some(net) => {
                    anyhow::ensure!(seats.is_empty() && at.is_none(), "over the network each machine plays its own keys (--input): no --input2 or --at");
                    headless::inspect_net::<G>(&dir, frames, &input, &begin, &net, record.as_deref())
                }
                None => headless::inspect::<G>(&dir, frames, &input, &seats, at, &begin),
            }
        }
        // The iroh trial (docs/multiplayer.md, stage 6, step 2).
        Some("nettest") => {
            let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
            let (mut reach, mut guests, mut seconds, mut size) = (crate::net::quic::Reach::Internet, 3, 20, 1000);
            let mut words = Vec::new();
            let mut it = rest.iter();
            while let Some(arg) = it.next() {
                match *arg {
                    "--guests" => guests = it.next().context("--guests needs a number")?.parse().context("--guests")?,
                    "--seconds" => seconds = it.next().context("--seconds needs a number")?.parse().context("--seconds")?,
                    "--size" => size = it.next().context("--size needs a number")?.parse().context("--size")?,
                    "internet" | "relayed" | "direct" => reach = crate::net::trial::reach_named(arg)?,
                    word => words.push(word),
                }
            }
            match words.as_slice() {
                [] => {
                    if !crate::net::trial::run(reach, guests, seconds, size)? {
                        std::process::exit(1);
                    }
                    Ok(())
                }
                ["host"] => crate::net::trial::host(reach),
                ["join", ticket] => {
                    if !crate::net::trial::join(ticket, reach, seconds, size)? {
                        std::process::exit(1);
                    }
                    Ok(())
                }
                _ => bail!("nettest [internet | relayed | direct] [--guests N] [--seconds N] [--size BYTES], nettest host, nettest join TICKET"),
            }
        }
        // The network's soak (docs/multiplayer.md, stage 4): machines on
        // simulated networks play the same ticks; the digest is compared
        // between systems.
        Some("soak") => {
            let mut seeds = 100;
            let mut digest = None;
            let mut it = args[1..].iter();
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--digest" => digest = Some(PathBuf::from(it.next().context("--digest needs a file")?)),
                    n => seeds = n.parse().with_context(|| format!("soak: '{n}' is no number of seeds"))?,
                }
            }
            let started = std::time::Instant::now();
            let sum = format!("{:016x}", crate::net::soak::soak(seeds));
            println!("{seeds} seeds played alike in {:.1} s: digest {sum}", started.elapsed().as_secs_f32());
            if let Some(path) = digest {
                std::fs::write(&path, format!("{sum}\n")).with_context(|| format!("writing {}", path.display()))?;
            }
            Ok(())
        }
        Some("kinds") => {
            let session = crate::session::Session::<G>::load(&opened(&args)?)?;
            for line in session.game.numbers() {
                println!("{line}");
            }
            Ok(())
        }
        Some("api") => {
            let mut filter = None;
            let mut it = args[1..].iter();
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--lua" => {
                        let file = PathBuf::from(it.next().context("--lua needs a file")?);
                        if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&file, crate::lua::api::definitions())?;
                        println!("wrote {}", file.display());
                        return Ok(());
                    }
                    name => filter = Some(name.to_string()),
                }
            }
            print!("{}", crate::lua::api::text(filter.as_deref()));
            Ok(())
        }
        Some("import") => {
            let dir = game_dir(&args)?;
            let source = PathBuf::from(args.get(2).context("import needs a Tiled map (.json) or MagicaVoxel models (.vox)")?);
            if source.extension().is_some_and(|e| e.eq_ignore_ascii_case("vox")) {
                // Models from MagicaVoxel: each a .vvx in the game, the
                // source left alone.
                let mut into = None;
                let mut sources = Vec::new();
                let mut it = args[2..].iter();
                while let Some(arg) = it.next() {
                    match arg.as_str() {
                        "--as" => into = Some(it.next().context("--as needs the model's path in the game (hero.vvx)")?.clone()),
                        other if other.starts_with("--") => bail!("unknown option {other}"),
                        file => sources.push(PathBuf::from(file)),
                    }
                }
                anyhow::ensure!(into.is_none() || sources.len() == 1, "--as names the .vvx of one model");
                for vox in &sources {
                    let name = match &into {
                        Some(name) => PathBuf::from(name),
                        None => PathBuf::from(vox.file_name().context("a model's file")?).with_extension(crate::vvx::EXTENSION),
                    };
                    anyhow::ensure!(crate::vvx::is_model(&name), "{}: a model of the game is a .vvx", name.display());
                    println!("{}", crate::upgrade::bring_in(vox, &dir.join(name), false)?);
                }
                return Ok(());
            }
            let (mut file, mut over) = ("level.toml".to_string(), false);
            let mut it = args[3..].iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--file" => file = it.next().context("--file needs a name")?.clone(),
                    "--over" => over = true,
                    other => bail!("unknown option {other}"),
                }
            }
            anyhow::ensure!(file.ends_with(".toml"), "--file {file}: a level of the game is a .toml (levels/02_crypt.toml)");
            println!("{}", crate::tiled::import_into(&dir, &source, &file, over)?);
            Ok(())
        }
        Some("upgrade") => {
            let dir = game_dir(&args)?;
            let dry = match args.get(2).map(String::as_str) {
                None => false,
                Some("--dry") => true,
                Some(other) => bail!("unknown option {other} (upgrade GAME [--dry])"),
            };
            for line in crate::upgrade::upgrade(&dir, dry)? {
                println!("{line}");
            }
            Ok(())
        }
        Some("edit") => {
            let dir = opened(&args)?;
            let mut opts = crate::editor::ShotOptions { level: None, block: None, model: None, part: None, parent: None, joint: None, stamp: false, things: false, shift: false, align: false, tool: None, held: None, view: None, width: 1920, height: 1080, out: PathBuf::new(), ops: Vec::new(), choose: None, game_view: false, picker: false, lights_off: false };
            let mut shot = false;
            let three = |flag: &str, value: &str| -> Result<glam::IVec3> {
                let n: Vec<i32> = value.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>().with_context(|| format!("{flag} X,Y,Z"))?;
                let [x, y, z] = n[..] else { bail!("{flag} X,Y,Z: three numbers") };
                Ok(glam::IVec3::new(x, y, z))
            };
            let mut it = args[2..].iter();
            while let Some(flag) = it.next() {
                if flag == "--things" || flag == "--stamp" {
                    (opts.things, opts.stamp) = (opts.things || flag == "--things", opts.stamp || flag == "--stamp");
                    continue;
                }
                if flag == "--shift" || flag == "--align" {
                    (opts.shift, opts.align) = (opts.shift || flag == "--shift", opts.align || flag == "--align");
                    continue;
                }
                match flag.as_str() {
                    "--game-view" => opts.game_view = true,
                    "--picker" => opts.picker = true,
                    "--lights-off" => opts.lights_off = true,
                    _ => {}
                }
                if matches!(flag.as_str(), "--game-view" | "--picker" | "--lights-off") {
                    continue;
                }
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--level" => opts.level = Some(value.clone()),
                    "--tool" => opts.tool = Some(value.clone()),
                    "--held" => {
                        let n: Vec<i32> = value.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>().context("--held X,Y,Z")?;
                        let [x, y, z] = n[..] else { bail!("--held X,Y,Z: three numbers") };
                        opts.held = Some(glam::IVec3::new(x, y, z));
                    }
                    "--block" => opts.block = Some(value.clone()),
                    // Among the pieces: changes tried in turn, nothing written.
                    "--choose" => opts.choose = Some(value.clone()),
                    "--add" => opts.ops.push(crate::editor::ShotOp::Add(value.clone(), None)),
                    "--hang" => opts.ops.push(crate::editor::ShotOp::Hang(value.clone(), None)),
                    "--move" => opts.ops.push(crate::editor::ShotOp::Move(value.clone(), None)),
                    "--copy" => opts.ops.push(crate::editor::ShotOp::Copy(value.clone(), None)),
                    "--remove" => opts.ops.push(crate::editor::ShotOp::Remove(value.clone())),
                    "--light" => opts.ops.push(crate::editor::ShotOp::Light(value.clone(), None, None, None)),
                    "--room-size" => opts.ops.push(crate::editor::ShotOp::Size(three(flag, value)?)),
                    "--at" => {
                        let spot = three(flag, value)?;
                        anyhow::ensure!(opts.ops.last_mut().is_some_and(|op| op.at(spot)), "--at follows --add, --move, --copy or --light (where its light shines from)");
                    }
                    "--turn" => {
                        let by = it.next().filter(|b| *b == "--by").and(it.next()).context("--turn NAME --by DEGREES")?;
                        opts.ops.push(crate::editor::ShotOp::Turn(value.clone(), by.parse().context("--by DEGREES")?));
                    }
                    "--rename" => {
                        let to = it.next().filter(|b| *b == "--to").and(it.next()).context("--rename NAME --to NEW")?;
                        opts.ops.push(crate::editor::ShotOp::Rename(value.clone(), to.clone()));
                    }
                    "--radius" | "--colour" | "--color" => match opts.ops.last_mut() {
                        Some(crate::editor::ShotOp::Light(_, radius, colour, _)) => {
                            if flag == "--radius" {
                                *radius = Some(value.parse().context("--radius N")?);
                            } else {
                                *colour = Some(value.clone());
                            }
                        }
                        _ => bail!("{flag} follows --light NAME"),
                    },
                    "--model" => opts.model = Some(value.clone()),
                    "--part" => opts.part = Some(value.clone()),
                    "--parent" => opts.parent = Some(value.clone()),
                    "--joint" => {
                        let n: Vec<f32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--joint X,Y,Z")?;
                        let [x, y, z] = n[..] else { bail!("--joint X,Y,Z: three numbers") };
                        opts.joint = Some(glam::Vec3::new(x, y, z));
                    }
                    "--shot" => {
                        opts.out = PathBuf::from(value);
                        shot = true;
                    }
                    "--view" => {
                        let (yaw, pitch) = value.split_once(',').context("--view YAW,PITCH")?;
                        opts.view = Some((yaw.trim().parse().context("--view yaw")?, pitch.trim().parse().context("--view pitch")?));
                    }
                    "--size" => {
                        let (w, h) = value.split_once('x').context("--size WxH")?;
                        (opts.width, opts.height) = (w.parse().context("--size width")?, h.parse().context("--size height")?);
                    }
                    other => bail!("unknown option {other}"),
                }
            }
            if shot { crate::editor::shot::<G>(&dir, &opts) } else { crate::editor::run::<G>(&dir, opts.level.as_deref(), opts.block.as_deref(), opts.things, opts.model.as_deref()) }
        }
        Some("ide") => {
            // Without a game: the start page.
            // (A game made for another Voxcade opens on its upgrade's page.)
            let dir = match args.get(1).filter(|a| !a.starts_with("--")) {
                Some(_) => Some(game_dir(&args)?),
                None => None,
            };
            let mut opts = crate::ide::ShotOptions { open: None, console: None, play: None, upgraded: false, width: 1920, height: 1080, out: PathBuf::new() };
            let mut shot = false;
            let mut remote = false;
            let mut it = args[if dir.is_some() { 2 } else { 1 }..].iter();
            while let Some(flag) = it.next() {
                if flag == "--remote" {
                    remote = true;
                    continue;
                }
                if flag == "--upgraded" {
                    opts.upgraded = true;
                    continue;
                }
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--open" => opts.open = Some(value.clone()),
                    "--console" => opts.console = Some(value.clone()),
                    "--play" => opts.play = Some(value.parse().context("--play TICKS")?),
                    "--shot" => {
                        opts.out = PathBuf::from(value);
                        shot = true;
                    }
                    "--size" => {
                        let (w, h) = value.split_once('x').context("--size WxH")?;
                        (opts.width, opts.height) = (w.parse().context("--size width")?, h.parse().context("--size height")?);
                    }
                    other => bail!("unknown option {other}"),
                }
            }
            if shot { crate::ide::shot::<G>(dir.as_deref(), &opts) } else { crate::ide::run::<G>(dir.as_deref(), remote) }
        }
        Some("mcp") => crate::mcp::serve(args[1..].iter().any(|a| a == "--remote")),
        Some("sounds") => {
            let dir = game_dir(&args)?;
            let mut out = PathBuf::from("shots/sounds.wav");
            let mut only = None;
            let mut it = args[2..].iter();
            while let Some(flag) = it.next() {
                match flag.as_str() {
                    "--list" => return headless::sound_list::<G>(&dir),
                    "--to-files" => return headless::sounds_to_files::<G>(&dir),
                    _ => {}
                }
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--out" => out = PathBuf::from(value),
                    "--only" => only = Some(value.clone()),
                    other => bail!("unknown option {other}"),
                }
            }
            headless::sounds::<G>(&dir, &out, only.as_deref())
        }
        Some("pads") => {
            let seconds = match args.get(1).map(String::as_str) {
                Some("--seconds") => args.get(2).context("--seconds needs a value")?.parse().context("--seconds")?,
                _ => 0.0,
            };
            pads::pads(seconds)
        }
        Some("music") => {
            // A song file by itself, or a game's first song.
            let song = args.get(1).map(PathBuf::from).filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "mml"));
            let dir = match &song {
                Some(_) => PathBuf::new(),
                None => game_dir(&args)?,
            };
            let mut out = PathBuf::from("shots/music.wav");
            let mut seconds = None;
            let mut it = args[2..].iter();
            while let Some(flag) = it.next() {
                let value = it.next().with_context(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--out" => out = PathBuf::from(value),
                    "--seconds" => seconds = Some(value.parse().context("--seconds")?),
                    other => bail!("unknown option {other}"),
                }
            }
            match song {
                Some(file) => headless::song(&file, None, &out, seconds),
                None => headless::music::<G>(&dir, &out, seconds),
            }
        }
        // Started by a double click: the launcher (the IDE's start page).
        None => {
            crate::desktop::keep_entry();
            crate::ide::run::<G>(None, false)
        }
        Some("--version" | "-V" | "version") => {
            println!("voxcade {}", crate::update::VERSION);
            Ok(())
        }
        Some("--help" | "-h" | "help") => {
            match args.get(1).map(String::as_str) {
                None => print!("{}", short_help()),
                Some("all") => print!("{USAGE}"),
                Some(command) => match usage_of(command) {
                    Some(usage) => print!("{usage}"),
                    None => bail!("{command} is no command of voxcade's (voxcade --help lists them)"),
                },
            }
            Ok(())
        }
        Some("update") => {
            let install = match args.get(1).map(String::as_str) {
                None => false,
                Some("--install") => true,
                Some(other) => bail!("unknown option {other}"),
            };
            match crate::update::look()? {
                None => println!("voxcade {} is the newest version", crate::update::VERSION),
                Some(release) if !install => println!("voxcade {} is there (this is {}): {}\nvoxcade update --install puts it in place", release.version, crate::update::VERSION, release.notes),
                Some(release) => {
                    let dir = crate::update::install_dir().context("this program does not lie in a Voxcade folder (its games beside it): fetch the new version from voxca.de")?;
                    crate::update::install(&release, &dir, &|_| {})?;
                    println!("voxcade {} is in place in {}; what was there before lies in before-update", release.version, dir.display());
                }
            }
            Ok(())
        }
        _ => {
            eprint!("{}", short_help());
            std::process::exit(2);
        }
    }
}

/// What went wrong in plain words, in the system's language (English,
/// German or French), with what the program itself said below.
pub fn plainly(error: &anyhow::Error, languages: &[String]) -> String {
    let said = format!("{error:#}");
    let lang = languages.iter().map(String::as_str).find(|l| matches!(*l, "en" | "de" | "fr")).unwrap_or("en");
    let graphics = ["no suitable GPU", "creating surface", "creating window", "request_device"].iter().any(|s| said.contains(s));
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

/// Says `error` in a window of its own, if the program was started for a
/// window (the launcher, a game, the IDE, an editor) and not to draw a
/// picture or play without one.
pub fn tell_in_a_window(error: &anyhow::Error) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let window = match args.first().map(String::as_str) {
        None => true,
        Some("run" | "ide" | "edit" | "replay") => !args.iter().any(|a| a == "--shot"),
        _ => false,
    };
    if window {
        let message = plainly(error, &crate::settings::system_languages());
        let _ = sdl3::messagebox::show_simple_message_box(sdl3::messagebox::MessageBoxFlag::ERROR, "Voxcade", &message, None);
    }
}

/// The game folder the command names, checked to be for this Voxcade
/// (`upgrade::opens`): one made for another is a plain mistake.
fn opened(args: &[String]) -> Result<PathBuf> {
    let dir = game_dir(args)?;
    crate::upgrade::opens(&dir)?;
    Ok(dir)
}

fn game_dir(args: &[String]) -> Result<PathBuf> {
    let dir = args.get(1).context("missing <game-dir>")?;
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        bail!("{} is no game folder", dir.display());
    }
    Ok(dir)
}

/// The seat an `--inputN` names (`--input2`: seat 2), if it is one.
fn seat_input(flag: &str) -> Option<usize> {
    flag.strip_prefix("--input")?.parse().ok().filter(|&n| n >= 2)
}

fn shot_options(args: &[String]) -> Result<headless::ShotOptions> {
    let mut opts = headless::ShotOptions {
        frames: None,
        replay: None,
        record: None,
        screen: None,
        input: String::new(),
        seats: Vec::new(),
        at: None,
        shadows: Default::default(),
        pad: None,
        width: 1920,
        height: 1080,
        out: PathBuf::from("shots/shot.png"),
        begin: Default::default(),
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            flag if opts.begin.flag(flag, value) => {}
            "--frames" => opts.frames = Some(value.parse().context("--frames")?),
            "--replay" => opts.replay = Some(PathBuf::from(value)),
            "--record" => opts.record = Some(PathBuf::from(value)),
            "--input" => opts.input = value.clone(),
            flag if seat_input(flag).is_some() => opts.seats.push((seat_input(flag).unwrap_or(0), value.clone())),
            "--screen" => opts.screen = Some(value.clone()),
            "--at" => {
                let (x, y) = value.split_once(',').context("--at expects x,y")?;
                opts.at = Some([x.trim().parse().context("--at x")?, y.trim().parse().context("--at y")?]);
            }
            "--out" => opts.out = PathBuf::from(value),
            "--shadows" => {
                opts.shadows = toml::Value::String(value.clone()).try_into().context("--shadows: high, medium or low")?;
            }
            "--pad" => opts.pad = Some(PadStyle::named(value).context("--pad: xbox, nintendo or playstation")?),
            "--size" => {
                let (w, h) = value.split_once('x').context("--size expects WxH")?;
                opts.width = w.parse().context("--size width")?;
                opts.height = h.parse().context("--size height")?;
            }
            other => bail!("unknown option {other}"),
        }
    }
    Ok(opts)
}

#[cfg(test)]
mod tests {
    /// The short help lists every command the program answers to, each
    /// with a line, and each has its usage (`voxcade COMMAND --help`).
    #[test]
    fn the_help_lists_every_command() {
        let src = include_str!("cli.rs");
        let main = &src[src.find("pub fn main<G: Game>").unwrap()..src.find("pub fn plainly").unwrap()];
        let mut matched = Vec::new();
        for line in main.lines().filter(|l| l.starts_with("        Some(\"")) {
            let words = line.trim_start().strip_prefix("Some(").unwrap().split(')').next().unwrap();
            matched.extend(words.split('|').map(|w| w.trim().trim_matches('"').to_string()));
        }
        assert!(matched.len() > 20, "{matched:?}");
        let help = super::short_help();
        assert!(help.contains("Without anything after it, voxcade opens the launcher (a window)"), "{help}");
        for word in matched.iter().filter(|w| !w.starts_with('-')) {
            assert!(super::COMMANDS.iter().any(|(c, _)| c == word), "{word} is missing from voxcade --help");
            assert!(help.contains(&format!("\n  {word} ")), "{word}");
        }
        for (command, _) in super::COMMANDS.iter().filter(|(c, _)| *c != "help") {
            let usage = super::usage_of(command).unwrap_or_else(|| panic!("{command} has no usage"));
            assert!(usage.contains(&format!("voxcade {}", if *command == "version" { "--version" } else { command })), "{command}: {usage}");
        }
        let shot = super::usage_of("shot").unwrap();
        assert!(shot.contains("simulates N ticks") && shot.contains("INPUT SCRIPT") && !shot.contains("voxcade check"), "{shot}");
        assert!(super::usage_of("replay").unwrap().lines().all(|l| !l.contains("voxcade replays")));
        assert!(super::usage_of("nothing").is_none());
    }

    #[test]
    fn a_mistake_is_said_plainly_in_the_systems_language() {
        let none = anyhow::anyhow!("no suitable GPU");
        assert!(super::plainly(&none, &["de".into()]).starts_with("Voxcade findet keine Grafik"));
        assert!(super::plainly(&none, &["ja".into(), "fr".into()]).contains("carte graphique"));
        let other = anyhow::anyhow!("games/x is no game folder");
        let said = super::plainly(&other, &[]);
        assert!(said.starts_with("Voxcade ran into a mistake") && said.ends_with("The program says: games/x is no game folder"), "{said}");
    }

    use super::*;

    /// A program rebuilt while it runs is found in its place.
    #[test]
    fn a_rebuilt_program_is_found_in_its_place() {
        let dir = std::env::temp_dir().join(format!("voxcade-rebuilt-{}", std::process::id()));
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

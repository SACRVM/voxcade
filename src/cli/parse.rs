//! The command line read without doing anything or looking at the disk:
//! `parse` makes a `Call` of it, each command's flags checked against the
//! table first (`Flags`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::table::{Command, Opt, command, known};
use crate::controls::PadStyle;
use crate::headless::{self, Networked};

/// A command's flags, each one known to its table before it is read.
pub(super) struct Flags<'a> {
    pub(super) command: &'static Command,
    pub(super) it: std::iter::Peekable<std::slice::Iter<'a, String>>,
}

impl<'a> Flags<'a> {
    pub(super) fn new(name: &str, args: &'a [String]) -> Self {
        Flags { command: command(name).expect("a command of the table"), it: args.iter().peekable() }
    }

    /// The next flag and its option.
    pub(super) fn next(&mut self) -> Result<Option<(&'a str, &'static Opt)>> {
        let Some(flag) = self.it.next() else { return Ok(None) };
        Ok(Some((flag.as_str(), known(self.command, flag)?)))
    }

    /// The value after `flag`.
    pub(super) fn value(&mut self, flag: &str) -> Result<&'a String> {
        self.it.next().with_context(|| format!("{flag} needs a value"))
    }

    /// `WORD VALUE` after a flag's value (`--by 90`), else the usage.
    pub(super) fn then(&mut self, word: &str, usage: impl Fn() -> String) -> Result<&'a String> {
        let found = self.it.next().filter(|b| *b == word);
        let value = self.it.next();
        found.and(value).with_context(usage)
    }
}

/// The arguments from `at` on.
pub(super) fn tail(args: &[String], at: usize) -> &[String] {
    args.get(at..).unwrap_or(&[])
}

/// The game folder a command names (its first argument), not yet looked at.
pub(super) fn named_dir(args: &[String]) -> Result<PathBuf> {
    args.get(1).map(PathBuf::from).context("missing <game-dir>")
}

/// How `check` plays the replays.
#[derive(Debug, PartialEq)]
pub(super) enum CheckHow {
    AsRecorded,
    Update,
    Refit,
    Rollback(u64),
}

/// What `edit` does: draws a picture, writes a model's cells or a level's
/// scattered pieces without a window, or opens its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditMode {
    Shot,
    Cells,
    Scatter,
    Window,
}

/// What `nettest` tries.
pub(super) enum NetTest {
    Here,
    Host,
    Join(String),
}

/// What `sounds` does.
#[derive(Clone, Copy)]
pub(super) enum SoundsDo {
    Write,
    List,
    ToFiles,
}

/// What the command line asks for, read and not yet done.
pub(super) enum Call {
    /// The launcher (nothing after the program's name), or a Rust game's
    /// own folder in a window.
    Launcher,
    Run { dir: PathBuf, begin: crate::session::Begin, net: Option<Networked>, keys: Option<String>, maker: bool },
    Replay { dir: PathBuf, file: PathBuf },
    Replays { dir: PathBuf },
    Ide { dir: Option<PathBuf>, opts: crate::ide::ShotOptions, shot: bool, remote: bool },
    Edit { dir: PathBuf, opts: Box<crate::editor::ShotOptions>, mode: EditMode },
    ImportMap { dir: PathBuf, source: PathBuf, file: String, over: bool },
    ImportModels { dir: PathBuf, sources: Vec<PathBuf>, into: Option<String> },
    Upgrade { dir: PathBuf, dry: bool },
    Update { install: bool },
    Version,
    Help(Option<String>),
    /// `voxcade COMMAND --help`.
    Usage(String),
    Shot { dir: PathBuf, opts: headless::ShotOptions },
    Check { dir: PathBuf, how: CheckHow },
    Inspect { dir: PathBuf, frames: u32, input: String, seats: Vec<(usize, String)>, at: Option<[f32; 2]>, begin: crate::session::Begin, net: Option<Networked>, record: Option<PathBuf> },
    Scenarios { dir: PathBuf },
    Sheet { dir: PathBuf, opts: headless::SheetOptions },
    Film { dir: PathBuf, opts: headless::FilmOptions },
    Sounds { dir: PathBuf, out: PathBuf, only: Option<String>, what: SoundsDo },
    /// A game's music, or a song file (`first` is either).
    Music { first: PathBuf, out: PathBuf, seconds: Option<f32> },
    Kinds { dir: PathBuf },
    Api { filter: Option<String>, lua: Option<PathBuf> },
    Mcp { remote: bool },
    Rooms { dir: PathBuf },
    Pads { seconds: f32 },
    Soak { seeds: u64, digest: Option<PathBuf> },
    Nettest { reach: crate::net::quic::Reach, guests: usize, seconds: u32, size: usize, what: NetTest },
    /// A word that is no command.
    Unknown(String),
}

impl Call {
    /// Whether doing it opens a window (where a mistake is said in a
    /// window too: nobody may be reading a terminal).
    pub(super) fn opens_a_window(&self) -> bool {
        match self {
            Call::Launcher | Call::Run { .. } | Call::Replay { .. } => true,
            Call::Ide { shot, .. } => !shot,
            Call::Edit { mode, .. } => *mode == EditMode::Window,
            _ => false,
        }
    }
}

/// Reads the command line (the program's name left out) without doing
/// anything or looking at the disk.
pub(super) fn parse(args: &[String]) -> Result<Call> {
    let Some(word) = args.first() else { return Ok(Call::Launcher) };
    // `voxcade COMMAND --help`: that command's usage.
    if matches!(args.get(1).map(String::as_str), Some("--help" | "-h")) && command(word).is_some() {
        return Ok(Call::Usage(word.clone()));
    }
    Ok(match word.as_str() {
        "run" => parse_run(args)?,
        "replay" => {
            let dir = named_dir(args)?;
            let file = PathBuf::from(args.get(2).context("missing the replay file")?);
            no_flags("replay", tail(args, 3))?;
            Call::Replay { dir, file }
        }
        "replays" => {
            no_flags("replays", tail(args, 2))?;
            Call::Replays { dir: named_dir(args)? }
        }
        "scenarios" => {
            no_flags("scenarios", tail(args, 2))?;
            Call::Scenarios { dir: named_dir(args)? }
        }
        "rooms" => {
            no_flags("rooms", tail(args, 2))?;
            Call::Rooms { dir: named_dir(args)? }
        }
        "kinds" => {
            no_flags("kinds", tail(args, 2))?;
            Call::Kinds { dir: named_dir(args)? }
        }
        "ide" => parse_ide(args)?,
        "edit" => {
            let dir = named_dir(args)?;
            let (opts, mode) = parse_edit(tail(args, 2))?;
            Call::Edit { dir, opts: Box::new(opts), mode }
        }
        "import" => parse_import(args)?,
        "upgrade" => {
            let dir = named_dir(args)?;
            let mut f = Flags::new("upgrade", tail(args, 2));
            let mut dry = false;
            while let Some((flag, _)) = f.next()? {
                match flag {
                    "--dry" => dry = true,
                    other => bail!("unknown option {other} (upgrade GAME [--dry])"),
                }
            }
            Call::Upgrade { dir, dry }
        }
        "update" => {
            let mut f = Flags::new("update", tail(args, 1));
            let mut install = false;
            while let Some((flag, _)) = f.next()? {
                match flag {
                    "--install" => install = true,
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Update { install }
        }
        "--version" | "-V" | "version" => {
            no_flags("version", tail(args, 1))?;
            Call::Version
        }
        "--help" | "-h" | "help" => Call::Help(args.get(1).cloned()),
        "shot" => {
            let dir = named_dir(args)?;
            Call::Shot { dir, opts: shot_options(tail(args, 2))? }
        }
        "check" => {
            let dir = named_dir(args)?;
            Call::Check { dir, how: parse_check(tail(args, 2))? }
        }
        "inspect" => parse_inspect(args)?,
        "sheet" => {
            let dir = named_dir(args)?;
            let model = args.get(2).filter(|a| !a.starts_with("--")).context("voxcade sheet <game-dir> <model>")?.clone();
            let stem = Path::new(&model).file_stem().map_or("model".into(), |s| s.to_string_lossy().to_string());
            let mut out = PathBuf::from(format!("shots/{stem}_sheet.png"));
            let mut f = Flags::new("sheet", tail(args, 3));
            while let Some((flag, _)) = f.next()? {
                let value = f.value(flag)?;
                match flag {
                    "--out" => out = PathBuf::from(value),
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Sheet { dir, opts: headless::SheetOptions { model, out } }
        }
        "film" => parse_film(args)?,
        "sounds" => {
            let dir = named_dir(args)?;
            let (mut out, mut only, mut what) = (PathBuf::from("shots/sounds.wav"), None, None);
            let mut f = Flags::new("sounds", tail(args, 2));
            while let Some((flag, _)) = f.next()? {
                match flag {
                    // The first of these says what is done.
                    "--list" => {
                        what.get_or_insert(SoundsDo::List);
                    }
                    "--to-files" => {
                        what.get_or_insert(SoundsDo::ToFiles);
                    }
                    flag => {
                        let value = f.value(flag)?;
                        match flag {
                            "--out" => out = PathBuf::from(value),
                            "--only" => only = Some(value.clone()),
                            other => bail!("unknown option {other}"),
                        }
                    }
                }
            }
            Call::Sounds { dir, out, only, what: what.unwrap_or(SoundsDo::Write) }
        }
        "music" => {
            let first = named_dir(args)?;
            let (mut out, mut seconds) = (PathBuf::from("shots/music.wav"), None);
            let mut f = Flags::new("music", tail(args, 2));
            while let Some((flag, _)) = f.next()? {
                let value = f.value(flag)?;
                match flag {
                    "--out" => out = PathBuf::from(value),
                    "--seconds" => seconds = Some(value.parse().context("--seconds")?),
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Music { first, out, seconds }
        }
        "api" => {
            let (mut filter, mut lua) = (None, None);
            let mut it = tail(args, 1).iter();
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--lua" => lua = Some(PathBuf::from(it.next().context("--lua needs a file")?)),
                    flag if flag.starts_with('-') => {
                        known(command("api").expect("api"), flag)?;
                        bail!("{flag}: not taken here");
                    }
                    name => filter = Some(name.to_string()),
                }
            }
            Call::Api { filter, lua }
        }
        "mcp" => {
            let mut f = Flags::new("mcp", tail(args, 1));
            let mut remote = false;
            while let Some((flag, _)) = f.next()? {
                match flag {
                    "--remote" => remote = true,
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Mcp { remote }
        }
        "pads" => {
            let mut f = Flags::new("pads", tail(args, 1));
            let mut seconds = 0.0;
            while let Some((flag, _)) = f.next()? {
                let value = f.value(flag)?;
                match flag {
                    "--seconds" => seconds = value.parse().context("--seconds")?,
                    other => bail!("unknown option {other}"),
                }
            }
            Call::Pads { seconds }
        }
        "soak" => {
            let (mut seeds, mut digest) = (100, None);
            let mut it = tail(args, 1).iter();
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--digest" => digest = Some(PathBuf::from(it.next().context("--digest needs a file")?)),
                    flag if flag.starts_with("--") => {
                        known(command("soak").expect("soak"), flag)?;
                        bail!("{flag}: not taken here");
                    }
                    n => seeds = n.parse().with_context(|| format!("soak: '{n}' is no number of seeds"))?,
                }
            }
            Call::Soak { seeds, digest }
        }
        "nettest" => parse_nettest(args)?,
        _ => Call::Unknown(word.clone()),
    })
}

/// A command that takes no options: anything after its words is a mistake.
pub(super) fn no_flags(name: &str, rest: &[String]) -> Result<()> {
    match Flags::new(name, rest).next()? {
        Some((flag, _)) => bail!("{flag}: in the table of {name}, not taken"),
        None => Ok(()),
    }
}

pub(super) fn parse_run(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let (mut begin, mut net, mut keys, mut maker) = (crate::session::Begin::default(), None, None, false);
    let mut f = Flags::new("run", tail(args, 2));
    while let Some((flag, _)) = f.next()? {
        // --host with a port or without (26500 or the next free), or
        // across the internet.
        if flag == "--host" {
            if f.it.next_if(|p| *p == "internet").is_some() {
                net = Some(Networked::HostNet);
                continue;
            }
            let port = f.it.next_if(|p| p.parse::<u16>().is_ok()).map(|p| p.parse::<u16>().unwrap_or(crate::net::udp::PORT));
            net = Some(Networked::Host(port.unwrap_or(0)));
            continue;
        }
        // The maker's keys (F5, F9, 1 to 9): off for players.
        if flag == "--maker" {
            maker = true;
            continue;
        }
        let value = f.value(flag)?;
        match flag {
            "--join" => net = Some(Networked::Join(value.clone())),
            "--input" => keys = Some(value.clone()),
            flag if begin.flag(flag, value) => {}
            _ => bail!("unknown option {flag}"),
        }
    }
    Ok(Call::Run { dir, begin, net, keys, maker })
}

pub(super) fn parse_ide(args: &[String]) -> Result<Call> {
    // Without a game: the start page.
    // (A game made for another Voxcade opens on its upgrade's page.)
    let dir = args.get(1).filter(|a| !a.starts_with("--")).map(PathBuf::from);
    let mut opts = crate::ide::ShotOptions { open: None, console: None, play: None, upgraded: false, journal: None, find: None, replace: None, search: None, hit: None, width: 1920, height: 1080, out: PathBuf::new() };
    let (mut shot, mut remote) = (false, false);
    let mut f = Flags::new("ide", tail(args, if dir.is_some() { 2 } else { 1 }));
    while let Some((flag, _)) = f.next()? {
        if flag == "--remote" {
            remote = true;
            continue;
        }
        if flag == "--upgraded" {
            opts.upgraded = true;
            continue;
        }
        let value = f.value(flag)?;
        match flag {
            "--open" => opts.open = Some(value.clone()),
            "--console" => opts.console = Some(value.clone()),
            "--journal" => opts.journal = Some(PathBuf::from(value)),
            "--find" => opts.find = Some(value.clone()),
            "--replace" => opts.replace = Some(value.clone()),
            "--search" => opts.search = Some(value.clone()),
            "--hit" => opts.hit = Some(value.parse().context("--hit N")?),
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
    Ok(Call::Ide { dir, opts, shot, remote })
}

/// `X,Y,Z` after `flag`: three whole numbers.
pub(super) fn three(flag: &str, value: &str) -> Result<glam::IVec3> {
    let n: Vec<i32> = value.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>().with_context(|| format!("{flag} X,Y,Z"))?;
    let [x, y, z] = n[..] else { bail!("{flag} X,Y,Z: three numbers") };
    Ok(glam::IVec3::new(x, y, z))
}

pub(super) fn parse_edit(rest: &[String]) -> Result<(crate::editor::ShotOptions, EditMode)> {
    use crate::editor::ShotOp;
    let mut opts = crate::editor::ShotOptions { level: None, block: None, model: None, part: None, parent: None, joint: None, stamp: false, things: false, shift: false, align: false, tool: None, held: None, view: None, width: 1920, height: 1080, out: PathBuf::new(), ops: Vec::new(), choose: None, game_view: false, picker: false, lights_off: false, choose_light: None, menu: None, add_menu: false, level_settings: false, hover: None, turn_handles: false, part_moves: Vec::new(), hide: Vec::new(), click: None, slice: None, cells: Vec::new(), frame: None, dry: false, context: None, context_off: false, part_turns: Vec::new(), part_poses: Vec::new(), last_pose: false, add_part: false, merge_into: None, mirror: false, background: None, marquee: None, select: None, selection_ops: Vec::new(), terrain: false, land_ops: Vec::new(), material: None, scatter: Vec::new() };
    let mut shot = false;
    let mut f = Flags::new("edit", rest);
    while let Some((flag, opt)) = f.next()? {
        if opt.value.is_empty() {
            match flag {
                "--things" => opts.things = true,
                "--stamp" => opts.stamp = true,
                "--mirror" => opts.mirror = true,
                "--dry" => opts.dry = true,
                "--context-off" => opts.context_off = true,
                "--add-part" => opts.add_part = true,
                "--shift" => opts.shift = true,
                "--align" => opts.align = true,
                "--terrain" => opts.terrain = true,
                "--game-view" => opts.game_view = true,
                "--picker" => opts.picker = true,
                "--lights-off" => opts.lights_off = true,
                "--add-menu" => opts.add_menu = true,
                "--level-settings" => opts.level_settings = true,
                other => bail!("unknown option {other}"),
            }
            continue;
        }
        let value = f.value(flag)?;
        match flag {
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
            "--add" => opts.ops.push(ShotOp::Add(value.clone(), None)),
            "--hang" => opts.ops.push(ShotOp::Hang(value.clone(), None)),
            "--move" => opts.ops.push(ShotOp::Move(value.clone(), None)),
            "--copy" => opts.ops.push(ShotOp::Copy(value.clone(), None)),
            "--remove" => opts.ops.push(ShotOp::Remove(value.clone())),
            "--light" => opts.ops.push(ShotOp::Light(value.clone(), None, None, None, false)),
            "--add-light" => opts.ops.push(ShotOp::Light(value.clone(), None, None, None, true)),
            "--solid" => opts.ops.push(ShotOp::Solid(value.clone())),
            "--room-size" => opts.ops.push(ShotOp::Size(three(flag, value)?)),
            "--at" => {
                let spot = three(flag, value)?;
                anyhow::ensure!(opts.ops.last_mut().is_some_and(|op| op.at(spot)), "--at follows --add, --move, --copy, --light or --add-light (where its light shines from)");
            }
            "--rotate" => {
                let by = f.then("--by", || "--rotate NAME --by DEGREES".into())?;
                opts.ops.push(ShotOp::Turn(value.clone(), by.parse().context("--by DEGREES")?, 2));
            }
            // (After --rotate-part or --pose-part: rotation, pitch or roll.)
            "--about" if ["rotation", "pitch", "roll"].contains(&value.as_str()) => {
                let axis = ["rotation", "pitch", "roll"].iter().position(|a| a == value).expect("one of them");
                match (opts.last_pose, opts.part_poses.last_mut(), opts.part_turns.last_mut()) {
                    (true, Some(t), _) | (false, _, Some(t)) => t.2 = axis,
                    _ => bail!("--about rotation|pitch|roll follows --rotate-part NAME --by DEG (or --pose-part)"),
                }
            }
            "--about" => match opts.ops.last_mut() {
                Some(ShotOp::Turn(_, _, axis)) => *axis = ["x", "y", "z"].iter().position(|a| a == value).context("--about x, y or z")?,
                _ => bail!("--about follows --rotate NAME --by DEGREES"),
            },
            "--lay" => opts.ops.push(ShotOp::Turn(value.clone(), 90.0, 0)),
            "--menu" => opts.menu = Some(value.clone()),
            "--move-part" | "--move-joint" => {
                let by = f.then("--by", || format!("{flag} NAME --by X,Y,Z"))?;
                let n: Vec<f32> = by.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().with_context(|| format!("{flag} NAME --by X,Y,Z"))?;
                let [x, y, z] = n[..] else { bail!("{flag} NAME --by X,Y,Z: three numbers") };
                opts.part_moves.push((value.clone(), glam::Vec3::new(x, y, z), flag == "--move-joint"));
            }
            "--merge-into" => opts.merge_into = Some(value.clone()),
            "--material" => opts.material = Some(value.parse().context("--material N: a terrain's material, from 1")?),
            "--scatter" => opts.scatter.push(value.parse().context("--scatter N: a terrain's scatter rule, from 1")?),
            "--raise" | "--shape" => {
                let n: Vec<i32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().with_context(|| format!("{flag} X,Y: a point's column and row"))?;
                let [x, y] = n[..] else { bail!("{flag} X,Y: two numbers") };
                let p = glam::IVec2::new(x, y);
                if flag == "--shape" {
                    let word = f.it.next().context("--shape X,Y smooth|linear|step")?;
                    let shape = crate::terrain::Shape::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(word)).context("--shape X,Y smooth|linear|step")?;
                    opts.land_ops.push(crate::editor::LandOp::Shape(p, shape));
                } else {
                    let by = f.then("--by", || "--raise X,Y --by N [--radius R]".into())?;
                    let by: i32 = by.parse().context("--raise X,Y --by N: whole voxels")?;
                    let radius = match f.it.peek() {
                        Some(r) if r.as_str() == "--radius" => {
                            f.it.next();
                            f.it.next().context("--radius R")?.parse().context("--radius R: points")?
                        }
                        _ => 3,
                    };
                    opts.land_ops.push(crate::editor::LandOp::Raise(p, by, radius));
                }
            }
            "--select" => {
                let to = f.then("--to", || "--select X0,Y0,Z0 --to X1,Y1,Z1".into())?;
                let corner = |v: &str| -> Result<glam::IVec3> {
                    let n: Vec<i32> = v.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--select X0,Y0,Z0 --to X1,Y1,Z1: whole numbers")?;
                    let [x, y, z] = n[..] else { bail!("--select X0,Y0,Z0 --to X1,Y1,Z1: three numbers each") };
                    Ok(glam::IVec3::new(x, y, z))
                };
                opts.select = Some((corner(value)?, corner(to)?));
            }
            "--selection" => opts.selection_ops.push(value.clone()),
            "--marquee" => {
                let n: Vec<f32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--marquee X0,Y0,X1,Y1")?;
                let [a, b, c, d] = n[..] else { bail!("--marquee X0,Y0,X1,Y1: four numbers") };
                opts.marquee = Some([a, b, c, d]);
            }
            "--background" => opts.background = Some(crate::editor::background::hex(value).context("--background #rrggbb")?),
            "--rotate-part" => {
                let by = f.then("--by", || "--rotate-part NAME --by DEG [--about rotation|pitch|roll]".into())?;
                let by: f32 = by.parse().context("--rotate-part NAME --by DEG: degrees")?;
                opts.part_turns.push((value.clone(), by, 0));
                opts.last_pose = false;
            }
            "--pose-part" => {
                let by = f.then("--by", || "--pose-part NAME --by DEG [--about rotation|pitch|roll]".into())?;
                let by: f32 = by.parse().context("--pose-part NAME --by DEG: degrees")?;
                opts.part_poses.push((value.clone(), by, 0));
                opts.last_pose = true;
            }
            "--hover" => opts.hover = Some(value.clone()),
            "--hide" => opts.hide.push(value.clone()),
            "--context" => {
                let (level, name) = value.rsplit_once(':').context("--context LEVEL:NAME (a level's file and a piece in it)")?;
                opts.context = Some((level.to_string(), name.to_string()));
            }
            "--frame" => opts.frame = Some(value.parse().context("--frame N (from 1)")?),
            "--clear" => {
                let to = f.then("--to", || "--clear X0,Y0,Z0 --to X1,Y1,Z1".into())?;
                opts.cells.push(crate::editor::frames::CellOp::Clear(three("--clear", value)?, three("--to", to)?));
            }
            "--fill" => {
                let usage = || "--fill SIGN|#rrggbb --from X0,Y0,Z0 --to X1,Y1,Z1".to_string();
                let from = f.then("--from", usage)?;
                let to = f.then("--to", usage)?;
                opts.cells.push(crate::editor::frames::CellOp::Fill(value.clone(), three("--from", from)?, three("--to", to)?));
            }
            "--slice" => {
                let (axis, at) = value.split_once(',').context("--slice x|y|z,LAYER")?;
                let axis = ["x", "y", "z"].iter().position(|a| *a == axis).context("--slice x|y|z,LAYER")?;
                opts.slice = Some((axis, at.trim().parse().context("--slice x|y|z,LAYER: a whole number")?));
            }
            "--click" => {
                let n: Vec<f32> = value.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>().context("--click X,Y: a pixel of the shot")?;
                let [x, y] = n[..] else { bail!("--click X,Y: two numbers") };
                opts.click = Some((x, y));
            }
            "--handles" => {
                opts.turn_handles = match value.as_str() {
                    "move" => false,
                    "rotate" => true,
                    _ => bail!("--handles move or rotate"),
                }
            }
            "--choose-light" => opts.choose_light = Some(value.parse().context("--choose-light K (0 the first)")?),
            "--rename" => {
                let to = f.then("--to", || "--rename NAME --to NEW".into())?;
                opts.ops.push(ShotOp::Rename(value.clone(), to.clone()));
            }
            "--radius" | "--color" => match opts.ops.last_mut() {
                Some(ShotOp::Light(_, radius, colour, _, _)) => {
                    if flag == "--radius" {
                        *radius = Some(value.parse().context("--radius N")?);
                    } else {
                        *colour = Some(value.clone());
                    }
                }
                _ => bail!("{flag} follows --light NAME or --add-light NAME"),
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
    let mode = if shot {
        EditMode::Shot
    } else if !opts.cells.is_empty() {
        // Without a window: written as the editor writes.
        EditMode::Cells
    } else if !opts.scatter.is_empty() {
        EditMode::Scatter
    } else if opts.dry {
        bail!("--dry goes with --clear, --fill or --scatter: without them there is nothing to write")
    } else {
        EditMode::Window
    };
    Ok((opts, mode))
}

pub(super) fn parse_import(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let source = PathBuf::from(args.get(2).context("import needs a Tiled map (.json) or MagicaVoxel models (.vox)")?);
    let c = command("import").expect("import");
    if source.extension().is_some_and(|e| e.eq_ignore_ascii_case("vox")) {
        // Models from MagicaVoxel: each a .vvx in the game, the source
        // left alone.
        let mut into = None;
        let mut sources = Vec::new();
        let mut it = tail(args, 2).iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--as" => into = Some(it.next().context("--as needs the model's path in the game (hero.vvx)")?.clone()),
                flag if flag.starts_with("--") => {
                    known(c, flag)?;
                    bail!("{flag} is for a Tiled map, not models");
                }
                file => sources.push(PathBuf::from(file)),
            }
        }
        anyhow::ensure!(into.is_none() || sources.len() == 1, "--as names the .vvx of one model");
        return Ok(Call::ImportModels { dir, sources, into });
    }
    let (mut file, mut over) = ("level.toml".to_string(), false);
    let mut f = Flags::new("import", tail(args, 3));
    while let Some((flag, _)) = f.next()? {
        match flag {
            "--file" => file = f.it.next().context("--file needs a name")?.clone(),
            "--over" => over = true,
            "--as" => bail!("--as is for models (.vox), not a Tiled map"),
            other => bail!("unknown option {other}"),
        }
    }
    anyhow::ensure!(file.ends_with(".toml"), "--file {file}: a level of the game is a .toml (levels/02_crypt.toml)");
    Ok(Call::ImportMap { dir, source, file, over })
}

pub(super) fn parse_check(rest: &[String]) -> Result<CheckHow> {
    let mut f = Flags::new("check", rest);
    let how = match f.next()? {
        None => CheckHow::AsRecorded,
        Some(("--update", _)) => CheckHow::Update,
        Some(("--refit", _)) => CheckHow::Refit,
        Some(("--rollback", _)) => {
            let every = f.it.next().context("--rollback needs how many ticks to go back")?;
            CheckHow::Rollback(every.parse().context("--rollback: a number of ticks")?)
        }
        Some((other, _)) => bail!("unknown option {other}"),
    };
    if let Some((flag, _)) = f.next()? {
        bail!("check takes one of --update, --refit and --rollback TICKS ({flag} besides)");
    }
    Ok(how)
}

pub(super) fn parse_inspect(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let (mut frames, mut input, mut at, mut begin) = (60, String::new(), None, crate::session::Begin::default());
    let (mut seats, mut net, mut record) = (Vec::new(), None, None);
    let mut f = Flags::new("inspect", tail(args, 2));
    while let Some((flag, _)) = f.next()? {
        let value = f.value(flag)?;
        match flag {
            flag if begin.flag(flag, value) => {}
            "--frames" => frames = value.parse().context("--frames")?,
            "--input" => input = value.clone(),
            "--host" if value == "internet" => net = Some(Networked::HostNet),
            "--host" => net = Some(Networked::Host(value.parse().context("--host PORT or internet")?)),
            "--join" => net = Some(Networked::Join(value.clone())),
            "--record" => record = Some(PathBuf::from(value)),
            flag if seat_input(flag).is_some() => seats.push((seat_input(flag).unwrap_or(0), value.clone())),
            "--at" => {
                let (x, y) = value.split_once(',').context("--at expects x,y")?;
                at = Some([x.trim().parse().context("--at x")?, y.trim().parse().context("--at y")?]);
            }
            other => bail!("unknown option {other}"),
        }
    }
    if net.is_some() {
        anyhow::ensure!(seats.is_empty() && at.is_none(), "over the network each machine plays its own keys (--input): no --input2 or --at");
    }
    Ok(Call::Inspect { dir, frames, input, seats, at, begin, net, record })
}

pub(super) fn parse_film(args: &[String]) -> Result<Call> {
    let dir = named_dir(args)?;
    let replay = PathBuf::from(args.get(2).context("missing the replay file")?);
    let mut opts = headless::FilmOptions { replay, from: 0.0, to: None, fps: None, size: None, max_mb: None, out: PathBuf::from("shots/film.mp4") };
    let mut f = Flags::new("film", tail(args, 3));
    while let Some((flag, _)) = f.next()? {
        let value = f.value(flag)?;
        match flag {
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
    Ok(Call::Film { dir, opts })
}

// The iroh trial (docs/multiplayer.md, stage 6, step 2).
pub(super) fn parse_nettest(args: &[String]) -> Result<Call> {
    let (mut reach, mut guests, mut seconds, mut size) = (crate::net::quic::Reach::Internet, 3, 20, 1000);
    let mut words = Vec::new();
    let mut it = tail(args, 1).iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--guests" => guests = it.next().context("--guests needs a number")?.parse().context("--guests")?,
            "--seconds" => seconds = it.next().context("--seconds needs a number")?.parse().context("--seconds")?,
            "--size" => size = it.next().context("--size needs a number")?.parse().context("--size")?,
            word @ ("internet" | "relayed" | "direct") => reach = crate::net::trial::reach_named(word)?,
            flag if flag.starts_with("--") => {
                known(command("nettest").expect("nettest"), flag)?;
                bail!("{flag}: not taken here");
            }
            word => words.push(word),
        }
    }
    let what = match words.as_slice() {
        [] => NetTest::Here,
        ["host"] => NetTest::Host,
        ["join", ticket] => NetTest::Join(ticket.to_string()),
        _ => bail!("nettest [internet | relayed | direct] [--guests N] [--seconds N] [--size BYTES], nettest host, nettest join TICKET"),
    };
    Ok(Call::Nettest { reach, guests, seconds, size, what })
}

/// The seat an `--inputN` names (`--input2`: seat 2), if it is one.
pub(super) fn seat_input(flag: &str) -> Option<usize> {
    flag.strip_prefix("--input")?.parse().ok().filter(|&n| n >= 2)
}

pub(super) fn shot_options(args: &[String]) -> Result<headless::ShotOptions> {
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
    let mut f = Flags::new("shot", args);
    while let Some((flag, _)) = f.next()? {
        let value = f.value(flag)?;
        match flag {
            flag if opts.begin.flag(flag, value) => {}
            "--frames" => opts.frames = Some(value.parse().context("--frames")?),
            "--replay" => opts.replay = Some(PathBuf::from(value)),
            "--record" => opts.record = Some(PathBuf::from(value)),
            "--input" => opts.input = value.clone(),
            flag if seat_input(flag).is_some() => opts.seats.push((seat_input(flag).unwrap_or(0), value.clone())),
            "--screen" => {
                let names = || headless::SCREENS.iter().map(|(s, _)| *s).collect::<Vec<_>>().join(", ");
                anyhow::ensure!(headless::SCREENS.iter().any(|(s, _)| s == value), "no screen '{value}' ({})", names());
                opts.screen = Some(value.clone());
            }
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

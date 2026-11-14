//! A game's levels (`docs/game-model.md`, "Levels and places"): the files in
//! its `levels/` folder that `c.world` says are levels (levels of blocks and
//! maps are `.toml`, voxel worlds `.vvx`), all read when the game loads, in
//! the order of their names, numbers counted as numbers (`2` before `10`).
//! Without such files the game has one level, made as `c.world` says. A
//! level holds its world as built, the markers that place its things and
//! its named places; entering one clones its world (the chunks are shared
//! until written), so a level change reads no file and plays the same every
//! time.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use glam::{IVec2, IVec3, Vec3};

use crate::config::GameConfig;
use crate::files::Files;
use crate::game::Game;
use crate::sim::things::Named;
use crate::world::{Marker, World};

/// Where a player arrives in a level: its name, where its feet stand (in
/// voxels) and the way it faces there, if the level says (else it faces
/// as its rules turn it).
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub name: String,
    pub at: Vec3,
    pub facing: Option<IVec2>,
}

impl Place {
    /// A place facing `facing` (one of the 8 ways), if given.
    pub fn new(name: &str, at: Vec3, facing: Option<[i32; 2]>) -> Self {
        let facing = facing.map(|[x, y]| IVec2::new(x.signum(), y.signum())).filter(|f| *f != IVec2::ZERO);
        Self { name: name.to_string(), at, facing }
    }
}

/// A piece as a level sets it down (`grid::PIECE`): its name in the file,
/// its model (none: a light alone), the cell its pivot stands on in the
/// world's voxels, the way it faces, the lights it brings (all dimmed
/// together by `thing.state.light`), whether its voxels are solid (the
/// world's `Solids` hold them, by `index`: its place among the level's
/// pieces).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub name: Option<String>,
    pub model: Option<String>,
    pub cell: IVec3,
    pub facing: crate::grid::Facing,
    /// Tipped further in its own directions, degrees (`grid::Thing`).
    pub pitch: f32,
    pub roll: f32,
    pub lights: Vec<PieceLight>,
    pub solid: bool,
    /// Only in the sun's shadow map, never drawn (`grid::Thing`).
    pub shadow_only: bool,
    pub index: usize,
}

/// A piece's light: where it shines from in the model's own voxels from
/// its pivot (turning with the piece), how far it reaches, its colour (0
/// to 1 each).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieceLight {
    pub at: IVec3,
    pub radius: f32,
    pub color: [f32; 3],
}

/// A named box of a level (`level.areas.NAME`): its low corner in the
/// world's voxels, and its size.
#[derive(Clone, Debug, PartialEq)]
pub struct Area {
    pub name: String,
    pub at: IVec3,
    pub size: IVec3,
}

impl Area {
    /// Whether a point lies inside it (a thing's feet at its pivot);
    /// `z` none: at any height.
    pub fn holds(&self, x: f32, y: f32, z: Option<f32>) -> bool {
        let (lo, hi) = (self.at.as_vec3(), (self.at + self.size).as_vec3());
        let inside = |v: f32, a: f32, b: f32| v >= a && v < b;
        inside(x, lo.x, hi.x) && inside(y, lo.y, hi.y) && z.is_none_or(|z| inside(z, lo.z, hi.z))
    }
}

/// What a level's file gives one of its things over its kind's props
/// (`props = {...}` on a `[[thing]]`): its kind's name, the values
/// `thing.props` reads, and the line each key is on, for a mistake.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnProps {
    pub kind: String,
    pub values: Arc<toml::Table>,
    pub lines: Vec<(String, usize)>,
}

impl OwnProps {
    /// A thing's props as its file has them; none without any.
    pub fn of(thing: &crate::grid::Thing) -> Option<Self> {
        let props = &thing.props;
        (!props.is_empty()).then(|| Self {
            kind: thing.kind.clone(),
            values: Arc::new(props.iter().map(|p| (p.key.clone(), p.value.clone())).collect()),
            lines: props.iter().map(|p| (p.key.clone(), p.line)).collect(),
        })
    }
}

/// A level as made from its file: its world, the markers that place its
/// things, the files it came from, its places, its pieces and areas, its
/// own daylight.
pub struct Made {
    pub world: World,
    pub markers: Vec<Marker>,
    pub files: Vec<PathBuf>,
    pub places: Vec<Place>,
    /// The names the file gives its markers' things (if it does), in the
    /// markers' order.
    pub marker_names: Vec<Option<String>>,
    /// The ways the file turns its markers' things (degrees clockwise
    /// seen from above, north 0), in the markers' order.
    pub marker_facings: Vec<Option<f64>>,
    /// The props the file gives its markers' things, in the markers' order.
    pub marker_props: Vec<Option<OwnProps>>,
    pub pieces: Vec<Piece>,
    pub areas: Vec<Area>,
    /// Its own daylight (`[light]` in its file): what it sets of the
    /// picture's daylight over the game's `c.light`.
    pub daylight: crate::session::Daylight,
}

impl Made {
    /// A world and the markers in it, made from `files`: nothing else set
    /// down, no places, the game's daylight.
    pub fn world(world: World, markers: Vec<Marker>, files: Vec<PathBuf>) -> Self {
        let daylight = Default::default();
        Self { world, markers, files, places: Vec::new(), marker_names: Vec::new(), marker_facings: Vec::new(), marker_props: Vec::new(), pieces: Vec::new(), areas: Vec::new(), daylight }
    }

    /// Each of its things' own props is one its kind declares, of its
    /// default's type (`sim::things::check_prop`); `declared` gives a
    /// kind's props by its name (none for a kind the game lacks: a
    /// mistake of its own).
    pub fn check_props<'a>(&self, declared: impl Fn(&str) -> Option<&'a toml::Table>) -> Result<()> {
        for own in self.marker_props.iter().flatten() {
            let Some(props) = declared(&own.kind) else { continue };
            for (key, line) in &own.lines {
                crate::sim::things::check_prop(&own.kind, props, key, &own.values[key]).map_err(|why| anyhow::anyhow!("line {line}: {why}"))?;
            }
        }
        Ok(())
    }
}

/// One level as it was read: its name, and what its file made (read
/// through the level: `level.world`, `level.places`).
pub struct Level {
    /// Its name: the file's stem (`hall` for `levels/hall.toml`); `main`
    /// for a game's one level made in code.
    pub name: String,
    /// Its file as a mistake names it (`levels/hall.toml`; "" for a level
    /// made in code).
    pub file: String,
    pub made: Made,
}

impl std::ops::Deref for Level {
    type Target = Made;

    fn deref(&self) -> &Made {
        &self.made
    }
}

impl Level {
    /// A level of a world, markers and places, nothing set down.
    pub fn plain(name: &str, world: World, markers: Vec<Marker>, places: Vec<Place>) -> Self {
        Self { name: name.to_string(), file: String::new(), made: Made { places, ..Made::world(world, markers, Vec::new()) } }
    }

    /// The names its things have as it is entered (`level:thing(name)`),
    /// the markers' and the pieces', in that order (`thing_names`): `kind`
    /// gives a marker's kind's name and whether the ready-made rules stand
    /// the player as it (`PLAYER_BEHAVIOR`).
    pub fn thing_names<'a>(&'a self, kind: impl Fn(usize) -> (&'a str, bool)) -> Vec<Option<String>> {
        let markers = self.markers.iter().enumerate().map(|(m, marker)| {
            let (name, player) = kind(marker.kind);
            (name, self.marker_names.get(m).and_then(Option::as_deref), player)
        });
        thing_names(markers.chain(self.pieces.iter().map(|p| (crate::grid::PIECE, p.name.as_deref(), false))))
    }
}

/// The name of the `n`-th thing of `kind` in its level (from 1) that its
/// file gives none: `lift_1` (`level:thing`).
pub fn called(kind: &str, n: u32) -> String {
    format!("{kind}_{n}")
}

/// The names of a level's things, given by kind, the name the file gives
/// each (if it does) and whether the ready-made rules stand the player as
/// a thing of its kind, in the order they lie in: the file's, else
/// `called`'s, counted by kind (named ones too, as the game counts them).
/// The first thing of such a kind becomes the player: no name, no number
/// (`Stage::name_things` skips the players' figures so); None. A number
/// whose name another thing has (its own, or given before) is passed over
/// (`unused`).
pub fn thing_names<'a>(things: impl Iterator<Item = (&'a str, Option<&'a str>, bool)>) -> Vec<Option<String>> {
    let things: Vec<_> = things.collect();
    let owns: std::collections::BTreeSet<&str> = things.iter().filter_map(|t| t.1).collect();
    let mut given = std::collections::BTreeSet::new();
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    let mut player = false;
    things
        .into_iter()
        .map(|(kind, own, stands_player)| {
            if stands_player && !player {
                player = true;
                return None;
            }
            let n = counts.entry(kind).or_insert(0);
            *n += 1;
            Some(own.map_or_else(|| unused(kind, *n, &owns, &mut given), str::to_string))
        })
        .collect()
}

/// `called(kind, n)`, or past `n` the first number whose name is neither
/// another thing's own (`owns`) nor `given` already, then given: after
/// `lever_1` was taken out, the lever a prop named keeps `lever_2` as its
/// own and the next is `lever_3` still, not a second `lever_2`. Where no
/// own name is a kind's name and a number, as in every level written
/// before, it is `called`.
pub fn unused(kind: &str, n: u32, owns: &std::collections::BTreeSet<&str>, given: &mut std::collections::BTreeSet<String>) -> String {
    let name = (n..).map(|k| called(kind, k)).find(|name| !owns.contains(name.as_str()) && !given.contains(name)).expect("a free number");
    given.insert(name.clone());
    name
}

/// Every level of a game, in order.
#[derive(Clone)]
pub struct Levels {
    pub list: Vec<Arc<Level>>,
    /// The files they were made from (the replays' fingerprint, hot reload).
    pub files: Vec<PathBuf>,
    /// The level a run begins in (`c.start`; the first one unless set): the
    /// one rule for every start of a run, `first`.
    pub start: usize,
}

impl Levels {
    /// A game of one level.
    pub fn one(name: &str, world: World, markers: Vec<Marker>, places: Vec<Place>, files: Vec<PathBuf>) -> Self {
        Self { list: vec![Arc::new(Level::plain(name, world, markers, places))], files, start: 0 }
    }

    /// The level a run begins in, as `c.start` names it, else the first.
    pub fn first(&self) -> &Arc<Level> {
        &self.list[self.start]
    }

    /// The models the levels' pieces show, each once, in order: the game
    /// draws them though `conf.lua` names none of them.
    pub fn piece_models(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in self.list.iter().flat_map(|l| l.pieces.iter().filter_map(|p| p.model.clone())) {
            if !out.contains(&m) {
                out.push(m);
            }
        }
        out
    }

    /// The suns the levels set (`[light]`'s `sun`), each once: the meshes
    /// keep the faces each of them lights.
    pub fn suns(&self) -> Vec<Vec3> {
        let mut out: Vec<Vec3> = Vec::new();
        for s in self.list.iter().filter_map(|l| l.daylight.sun) {
            if !out.contains(&s) {
                out.push(s);
            }
        }
        out
    }

    /// The level called `name`, by its place in the order.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.list.iter().position(|l| l.name == name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.list.iter().map(|l| l.name.as_str()).collect()
    }

    /// Whether the props of a thing in level `at` that its kind says name
    /// something (`names`) name what there is: a level of the game; a
    /// place `world:go_to` arrives at (`START`, or one of the level's
    /// places) in the level its `Level` prop names, else in `at`; one of
    /// `things`, the names of the things in `at`. `value` gives a prop's
    /// value in effect (its own, else its kind's); "" names nothing. The
    /// mistake: the prop's key and why.
    pub fn check_named<'v>(&self, at: usize, things: &[&str], names: &BTreeMap<String, Named>, value: impl Fn(&str) -> Option<&'v toml::Value>) -> Result<(), (String, String)> {
        let text = |key: &str| value(key).and_then(toml::Value::as_str).unwrap_or("");
        let fail = |key: &str, why: String| (key.to_string(), format!("props.{key}: {why}"));
        // The level first: a place lies in the one it names.
        let mut level = at;
        for (key, _) in names.iter().filter(|(_, n)| **n == Named::Level) {
            let name = text(key);
            if !name.is_empty() {
                level = self.find(name).ok_or_else(|| fail(key, no_such("level", name, "", "levels", &self.names())))?;
            }
        }
        let places: Vec<&str> = std::iter::once(START).chain(self.list[level].places.iter().map(|p| p.name.as_str())).collect();
        for (key, named) in names {
            let among = |name: &str, place: bool| {
                let (what, list, level, there) = if place { ("place", "its places", level, places.as_slice()) } else { ("thing", "its named things", at, things) };
                match name.is_empty() || there.contains(&name) {
                    true => Ok(()),
                    false => Err(fail(key, no_such(what, name, &self.list[level].name, list, there))),
                }
            };
            match named {
                Named::Level => {}
                Named::Place => among(text(key), true)?,
                Named::Thing => among(text(key), false)?,
                Named::Things => {
                    for v in value(key).and_then(toml::Value::as_array).into_iter().flatten() {
                        let Some(name) = v.as_str() else { return Err(fail(key, format!("a list of things' names, in quotes, not {v}"))) };
                        among(name, false)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// The place every level has (`world:go_to(level, "start")`): where it
/// starts, its place of that name or else where its rules stand the player.
pub const START: &str = "start";

/// "no level called 02_cript (did you mean 02_crypt?)": the nearest name
/// there is, else those there are (`list` says what they are).
fn no_such(what: &str, name: &str, level: &str, list: &str, there: &[&str]) -> String {
    let within = if level.is_empty() { String::new() } else { format!(" in level {level}") };
    let hint = match crate::audio::words::near(name, there.iter().copied()) {
        Some(near) => format!("did you mean {near}?"),
        None if there.is_empty() => format!("{list}: none"),
        None => format!("{list}: {}", there.join(", ")),
    };
    format!("no {what} called {name}{within} ({hint})")
}

/// Names compared as people read them: runs of digits as numbers
/// (`level2` before `level10`), the rest by its characters; names that
/// read the same (`level2`, `level02`) by their characters.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    loop {
        match (x.first(), y.first()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let (dx, dy) = (x.iter().take_while(|c| c.is_ascii_digit()).count(), y.iter().take_while(|c| c.is_ascii_digit()).count());
                let (nx, ny) = (trim_zeros(&x[..dx]), trim_zeros(&y[..dy]));
                let by_number = nx.len().cmp(&ny.len()).then_with(|| nx.cmp(ny));
                if by_number != Ordering::Equal {
                    return by_number;
                }
                (x, y) = (&x[dx..], &y[dy..]);
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(d);
                }
                (x, y) = (&x[1..], &y[1..]);
            }
        }
    }
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let zeros = digits.iter().take_while(|&&c| c == b'0').count();
    &digits[zeros.min(digits.len().saturating_sub(1))..]
}

/// What kind of file a level is, as `c.world` says.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Blocks,
    Map,
    World,
}

impl Kind {
    /// The kind `c.world` says: blocks, a map (`.toml`) or a world (`.vvx`).
    fn of(world: &crate::config::WorldConfig) -> Self {
        if world.blocks.is_some() {
            Kind::Blocks
        } else if world.file.ends_with(".toml") {
            Kind::Map
        } else {
            Kind::World
        }
    }

    /// The ending of its files in `levels/`.
    fn ending(self) -> &'static str {
        if self == Kind::World { crate::vvx::EXTENSION } else { "toml" }
    }
}

/// The ending of the level files in `levels/` of a game of this config:
/// `toml` for levels of blocks and maps, `vvx` for worlds.
pub fn ending(cfg: &GameConfig) -> &'static str {
    Kind::of(&cfg.world).ending()
}

/// The game's worlds and map prefabs (`crate::scene`), as paths from the
/// game folder `dir` (`/` between): the world file or the worlds in
/// `levels/`, or the prefabs its maps name, read as the game reads its
/// folder (`Files`). What the IDE shows but does not shape as a model.
pub fn scene_files(dir: &Path, cfg: &GameConfig) -> Vec<String> {
    let kind = Kind::of(&cfg.world);
    if cfg.world.size.is_some() || kind == Kind::Blocks {
        return Vec::new();
    }
    let game = Files::folder(dir);
    let ending = format!(".{}", kind.ending());
    let names = game.list("levels").into_iter().filter_map(|p| p.file_name()?.to_str().map(str::to_string));
    let in_levels: Vec<String> = names.filter(|n| n.ends_with(&ending)).map(|n| format!("levels/{n}")).collect();
    let files = if in_levels.is_empty() { vec![cfg.world.file.clone()] } else { in_levels };
    if kind == Kind::World {
        return files;
    }
    let mut out = Vec::new();
    for map in files {
        let folder = Path::new(&map).parent().unwrap_or(Path::new("")).to_path_buf();
        let Ok(data) = game.read_to_string(&map).map_err(anyhow::Error::from).and_then(|t| crate::map::MapData::read(&t)) else { continue };
        for p in data.prefabs.values() {
            out.push(crate::files::slashed(&folder.join(&p.file)));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A level file the config names in a format before Voxcade 0.6.0: a plain
/// mistake with the command that brings it over.
fn older_format(file: &str) -> Result<()> {
    let new = match Path::new(file).extension().and_then(|e| e.to_str()) {
        Some("json") => "a level or a map is a .toml",
        Some("vox") => "a world is a .vvx",
        _ => return Ok(()),
    };
    anyhow::bail!("c.world names {file}: since Voxcade 0.6.0 {new} (`voxcade upgrade` brings the game's files over)")
}

/// The game's levels: the files in `levels/` of the kind `c.world` says, or
/// else its one level.
pub fn load<G: Game>(game: &Files, cfg: &GameConfig) -> Result<Levels> {
    let dir = game.dir();
    if let Some(world) = G::level(cfg)? {
        start_in(cfg, &[])?;
        return Ok(Levels::one("main", world, Vec::new(), Vec::new(), Vec::new()));
    }
    // A world its script builds: made once, never anew from the config.
    if let Some([x, y, z]) = cfg.world.size {
        start_in(cfg, &[])?;
        if x < 1 || y < 1 || z < 1 {
            anyhow::bail!("c.world.size = {{ {x}, {y}, {z} }}: a world is at least one voxel each way");
        }
        return Ok(Levels::one("main", World::new(IVec3::new(x, y, z), [[0; 4]; 256]), Vec::new(), Vec::new(), Vec::new()));
    }
    let kind = Kind::of(&cfg.world);
    older_format(cfg.world.blocks.as_ref().map_or(&cfg.world.file, |b| &b.file))?;
    if !cfg.markers.is_empty() {
        anyhow::bail!("c.markers: since Voxcade 0.6.0 a marker is a palette entry of the world or the prefab it lies in ({{ marker = \"rifleman\" }}); `voxcade upgrade` moves them there");
    }
    let start = start_in(cfg, &folder_files(game, &cfg.world).iter().map(|p| stem(p)).collect::<Vec<_>>())?;
    let mut levels = Levels { list: Vec::new(), files: Vec::new(), start };
    for (name, file) in level_files(game, &cfg.world) {
        let path = dir.join(&file);
        let made = one(game, cfg, kind, &path).with_context(|| file.clone())?;
        levels.files.extend(made.files.iter().cloned());
        levels.list.push(Arc::new(Level { name, file, made }));
    }
    levels.files.sort();
    levels.files.dedup();
    Ok(levels)
}

/// The game's level files by their levels' names, in order, each as the
/// game folder names it (`levels/01_hall.toml`): the files in `levels/` of
/// the kind `c.world` says and levels of pieces whatever it says (their
/// first line tells: format = "voxcade level 1"), else its one file.
/// Nothing read but those first lines (the level editor lists them so).
pub fn level_files(game: &Files, world: &crate::config::WorldConfig) -> Vec<(String, String)> {
    let files = folder_files(game, world);
    if files.is_empty() {
        let file = world.blocks.as_ref().map_or(&world.file, |b| &b.file);
        return vec![(stem(Path::new(file)), file.clone())];
    }
    files.iter().map(|p| (stem(p), format!("levels/{}", p.file_name().unwrap_or_default().to_string_lossy()))).collect()
}

/// The level files in `levels/`, in the order of their names.
fn folder_files(game: &Files, world: &crate::config::WorldConfig) -> Vec<PathBuf> {
    let kind = Kind::of(world);
    let extension = kind.ending();
    let mut files: Vec<PathBuf> = game.list(game.dir().join("levels")).into_iter().filter(|p| p.extension().is_some_and(|e| e == extension) || (kind == Kind::World && says_level(game, p))).collect();
    files.sort_by(|a, b| natural(&stem(a), &stem(b)));
    files
}

/// Where a run begins (`c.start`): the place of the level it names among
/// `names` (the files of `levels/`), else 0. A name that is no level, or a
/// game without levels in `levels/`, is a mistake saying so.
fn start_in(cfg: &GameConfig, names: &[String]) -> Result<usize> {
    let Some(name) = &cfg.start else { return Ok(0) };
    if names.is_empty() {
        anyhow::bail!("c.start = \"{name}\": the game has no levels in levels/ to begin in (a map, a world made in code and a game of one level begin where they are)");
    }
    let there: Vec<&str> = names.iter().map(String::as_str).collect();
    match names.iter().position(|n| n == name) {
        Some(at) => Ok(at),
        None => anyhow::bail!("c.start = \"{name}\": {}", no_such("level", name, "", "levels", &there)),
    }
}

/// `c.start` read anew (`conf.lua` changed) checked as the game loads: the
/// same mistakes, the levels not built.
pub fn check_start<G: Game>(game: &Files, cfg: &GameConfig) -> Result<()> {
    if cfg.start.is_none() {
        return Ok(());
    }
    let none = G::level(cfg)?.is_some() || cfg.world.size.is_some();
    let names: Vec<String> = if none { Vec::new() } else { folder_files(game, &cfg.world).iter().map(|p| stem(p)).collect() };
    start_in(cfg, &names).map(|_| ())
}

/// Whether the file at `path` is a level of blocks or pieces by its first
/// line (`format = "voxcade level 1"`).
pub(crate) fn says_level(game: &Files, path: &Path) -> bool {
    path.extension().is_some_and(|e| e == crate::grid::EXTENSION)
        && game.read_to_string(path).is_ok_and(|t| t.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).is_some_and(|l| l.replace(' ', "").starts_with("format=\"voxcadelevel")))
}

/// One level from the file at `path`, of the kind `c.world` says (a level
/// of pieces whatever it says), its things' props checked by their kinds
/// as every link by name is, when the game loads.
fn one(game: &Files, cfg: &GameConfig, kind: Kind, path: &Path) -> Result<Made> {
    let names = crate::sim::things::names(cfg);
    let free = cfg.camera.rotates == crate::config::Rotates::Free;
    let made = match (kind, &cfg.world.blocks) {
        (Kind::Blocks, Some(blocks)) => crate::grid::load_at(game, blocks, path, &names, free)?,
        (Kind::World, _) if says_level(game, path) => crate::grid::load_at(game, &crate::config::BlocksConfig::default(), path, &names, free)?,
        (Kind::Map, _) => crate::map::load_map_at(game, path, &names)?,
        _ => {
            let (world, found) = crate::scene::load_in(game, path)?.world(&names).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
            Made::world(world, found, vec![path.to_path_buf()])
        }
    };
    if made.marker_props.iter().any(Option::is_some) {
        let kinds = crate::config::resolve_like("things", &cfg.things)?;
        let none = toml::Table::new();
        made.check_props(|kind| Some(kinds.get(kind)?.get("props").and_then(toml::Value::as_table).unwrap_or(&none)))?;
    }
    Ok(made)
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Level names sort as people read them.
    #[test]
    fn level_names_sort_as_they_read() {
        let mut names = vec!["level10", "level2", "01_hall", "1_cave", "level02", "tower", "Tower", "level1"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["1_cave", "01_hall", "Tower", "level1", "level02", "level2", "level10", "tower"]);
    }

    type Game = crate::shooter::Shooter;

    /// A level of pieces, `size` voxels, its body after the head.
    fn pieces(size: [i32; 3], body: &str) -> String {
        format!("format = \"voxcade level 1\"\nsize = [{}, {}, {}]\n\n{body}", size[0], size[1], size[2])
    }

    /// The levels in `levels/` load in the order people read their names,
    /// worlds and levels of pieces alike, each with its places and areas
    /// in the world's voxels; the files they came from are listed.
    #[test]
    fn levels_load_in_order_with_their_places() {
        let dir = crate::scratch::Scratch::new("levels-order");
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        let light = "[[thing]]\nname = \"lamp\"\nat = [2, 2, 3]\nlight = { radius = 20 }\n\n[[place]]\nname = \"door\"\nat = [1, 1, 0]\nfacing = \"north\"\n\n[[area]]\nname = \"pit\"\nat = [0, 0, 0]\nsize = [4, 4, 1]\n";
        for name in ["10_last", "2_middle", "1_first"] {
            std::fs::write(dir.join("levels").join(format!("{name}.toml")), pieces([8, 8, 8], light)).unwrap();
        }
        let levels = load::<Game>(&Files::folder(&dir), &GameConfig::default()).unwrap();
        assert_eq!(levels.names(), ["1_first", "2_middle", "10_last"]);
        assert_eq!(levels.files.len(), 3);
        let first = &levels.list[0];
        assert_eq!(first.places, [Place::new("door", Vec3::new(1.5, 1.5, 0.0), Some([0, 1]))]);
        assert_eq!(first.areas, [Area { name: "pit".into(), at: IVec3::ZERO, size: IVec3::new(4, 4, 1) }]);
        assert_eq!((first.pieces.len(), first.pieces[0].lights.len()), (1, 1));
        assert!(first.areas[0].holds(3.5, 0.5, Some(0.5)) && !first.areas[0].holds(4.0, 0.5, None));
    }

    /// What a hand-written level does wrong is said with the file and the
    /// line: a thing, a place or an area outside the level (a hero at
    /// x = 15000 in a level 8 wide once loaded without a word), a file of
    /// an older Voxcade, `c.markers`.
    #[test]
    fn a_levels_mistakes_name_the_file_and_the_line() {
        let dir = crate::scratch::Scratch::new("levels-mistakes");
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        let file = dir.join("levels").join("hall.toml");
        let why = |body: &str| {
            std::fs::write(&file, pieces([8, 8, 8], body)).unwrap();
            format!("{:#}", load::<Game>(&Files::folder(&dir), &GameConfig::default()).err().expect("a mistake"))
        };
        // The body begins on line 4.
        let e = why("[[thing]]\nname = \"lamp\"\nat = [15000, 2, 3]\nlight = { radius = 20 }\n");
        assert!(e.contains("levels/hall.toml") && e.contains("line 5: the thing 'lamp' at [15000, 2, 3] stands outside the level") && e.contains("8 by 8 by 8 voxels: x 0 to 7"), "{e}");
        let e = why("[[place]]\nname = \"door\"\nat = [1, -1, 0]\n");
        assert!(e.contains("line 5: the place 'door' at [1, -1, 0] lies outside the level"), "{e}");
        let e = why("[[area]]\nname = \"pit\"\nat = [4, 4, 0]\nsize = [5, 1, 1]\n");
        assert!(e.contains("line 5: the area 'pit' at [4, 4, 0], 5 by 1 by 1, reaches outside the level"), "{e}");
        // Right at the edge is inside.
        assert!(std::fs::write(&file, pieces([8, 8, 8], "[[area]]\nname = \"pit\"\nat = [4, 4, 0]\nsize = [4, 4, 8]\n")).is_ok());
        assert!(load::<Game>(&Files::folder(&dir), &GameConfig::default()).is_ok());
        // Above the level the sky is open: a thing on its top layer, a
        // lamp hanging high over it; under its floor is outside.
        std::fs::write(&file, pieces([8, 8, 8], "[[thing]]\nname = \"lamp\"\nat = [2, 2, 40]\nlight = { radius = 20 }\n")).unwrap();
        assert!(load::<Game>(&Files::folder(&dir), &GameConfig::default()).is_ok());
        let e = why("[[thing]]\nname = \"lamp\"\nat = [2, 2, -1]\nlight = { radius = 20 }\n");
        assert!(e.contains("stands outside the level") && e.contains("y 0 to 7, z from 0 up"), "{e}");
        let e = why("[[thing]]\nkind = \"slimee\"\nat = [1, 1, 1]\n");
        assert!(e.contains("line 5: thing at [1, 1, 1]: 'slimee' is no kind of thing"), "{e}");
        std::fs::remove_file(&file).unwrap();
        let mut cfg = GameConfig::default();
        cfg.world.file = "world.vox".into();
        let e = load::<Game>(&Files::folder(&dir), &cfg).err().unwrap().to_string();
        assert!(e.contains("world.vox") && e.contains("voxcade upgrade"), "{e}");
    }
}

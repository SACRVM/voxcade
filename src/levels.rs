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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use glam::{IVec2, IVec3, Vec3};

use crate::config::GameConfig;
use crate::files::Files;
use crate::game::Game;
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

/// A level as made from its file: its world, the markers that place its
/// things (and the name the file gives each, if it does), the files it
/// came from, its places, its pieces and areas, its own daylight.
pub struct Made {
    pub world: World,
    pub markers: Vec<Marker>,
    pub files: Vec<PathBuf>,
    pub places: Vec<Place>,
    pub marker_names: Vec<Option<String>>,
    /// The ways the file turns its markers' things (degrees clockwise
    /// seen from above, north 0), in the markers' order.
    pub marker_facings: Vec<Option<f64>>,
    pub pieces: Vec<Piece>,
    pub areas: Vec<Area>,
    pub daylight: crate::session::Daylight,
}

/// One level as it was read.
pub struct Level {
    /// Its name: the file's stem (`hall` for `levels/hall.toml`); `main`
    /// for a game's one level made in code.
    pub name: String,
    pub world: World,
    pub markers: Vec<Marker>,
    pub places: Vec<Place>,
    /// The names the file gives its markers' things (`marker_names`), its
    /// pieces and its areas.
    pub marker_names: Vec<Option<String>>,
    /// The ways the file turns its markers' things (`marker_facings`).
    pub marker_facings: Vec<Option<f64>>,
    pub pieces: Vec<Piece>,
    pub areas: Vec<Area>,
    /// Its own daylight (`[light]` in its file): what it sets of the
    /// picture's daylight over the game's `c.light`.
    pub daylight: crate::session::Daylight,
}

impl Level {
    /// A level of a world, markers and places, nothing set down.
    pub fn plain(name: &str, world: World, markers: Vec<Marker>, places: Vec<Place>) -> Self {
        Self { name: name.to_string(), world, markers, places, marker_names: Vec::new(), marker_facings: Vec::new(), pieces: Vec::new(), areas: Vec::new(), daylight: Default::default() }
    }

    fn of(name: String, made: Made) -> Self {
        Self { name, world: made.world, markers: made.markers, places: made.places, marker_names: made.marker_names, marker_facings: made.marker_facings, pieces: made.pieces, areas: made.areas, daylight: made.daylight }
    }
}

/// Every level of a game, in order.
#[derive(Clone)]
pub struct Levels {
    pub list: Vec<Arc<Level>>,
    /// The files they were made from (the replays' fingerprint, hot reload).
    pub files: Vec<PathBuf>,
}

impl Levels {
    /// A game of one level.
    pub fn one(name: &str, world: World, markers: Vec<Marker>, places: Vec<Place>, files: Vec<PathBuf>) -> Self {
        Self { list: vec![Arc::new(Level::plain(name, world, markers, places))], files }
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
    fn of(cfg: &GameConfig) -> Self {
        if cfg.world.blocks.is_some() {
            Kind::Blocks
        } else if cfg.world.file.ends_with(".toml") {
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
    Kind::of(cfg).ending()
}

/// The game's worlds and map prefabs (`crate::scene`), as paths from the
/// game folder `dir` (`/` between): the world file or the worlds in
/// `levels/`, or the prefabs its maps name. What the IDE shows but does not
/// shape as a model.
pub fn scene_files(dir: &Path, cfg: &GameConfig) -> Vec<String> {
    let kind = Kind::of(cfg);
    if cfg.world.size.is_some() || kind == Kind::Blocks {
        return Vec::new();
    }
    let in_levels: Vec<String> = std::fs::read_dir(dir.join("levels"))
        .map(|d| d.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).filter(|n| n.ends_with(&format!(".{}", kind.ending()))).map(|n| format!("levels/{n}")).collect())
        .unwrap_or_default();
    let files = if in_levels.is_empty() { vec![cfg.world.file.clone()] } else { in_levels };
    if kind == Kind::World {
        return files;
    }
    let mut out = Vec::new();
    for map in files {
        let folder = Path::new(&map).parent().unwrap_or(Path::new("")).to_path_buf();
        let Ok(data) = std::fs::read_to_string(dir.join(&map)).map_err(anyhow::Error::from).and_then(|t| crate::map::MapData::read(&t)) else { continue };
        for p in data.prefabs.values() {
            let path = folder.join(&p.file);
            out.push(path.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
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
        return Ok(Levels::one("main", world, Vec::new(), Vec::new(), Vec::new()));
    }
    // A world its script builds: made once, never anew from the config.
    if let Some([x, y, z]) = cfg.world.size {
        if x < 1 || y < 1 || z < 1 {
            anyhow::bail!("c.world.size = {{ {x}, {y}, {z} }}: a world is at least one voxel each way");
        }
        return Ok(Levels::one("main", World::new(IVec3::new(x, y, z), [[0; 4]; 256]), Vec::new(), Vec::new(), Vec::new()));
    }
    let kind = Kind::of(cfg);
    older_format(cfg.world.blocks.as_ref().map_or(&cfg.world.file, |b| &b.file))?;
    if !cfg.markers.is_empty() {
        anyhow::bail!("c.markers: since Voxcade 0.6.0 a marker is a palette entry of the world or the prefab it lies in ({{ marker = \"rifleman\" }}); `voxcade upgrade` moves them there");
    }
    let extension = kind.ending();
    // The files of the kind c.world says, and levels of pieces whatever
    // it says (their first line tells: format = "voxcade level 1").
    let mut files: Vec<PathBuf> =
        game.list(dir.join("levels")).into_iter().filter(|p| p.extension().is_some_and(|e| e == extension) || (kind == Kind::World && says_level(game, p))).collect();
    files.sort_by(|a, b| natural(&stem(a), &stem(b)));
    if files.is_empty() {
        let file = match &cfg.world.blocks {
            Some(blocks) => &blocks.file,
            None => &cfg.world.file,
        };
        let path = dir.join(file);
        let made = one(game, cfg, kind, &path).with_context(|| path.display().to_string())?;
        let sources = made.files.clone();
        return Ok(Levels { list: vec![Arc::new(Level::of(stem(&path), made))], files: sources });
    }
    let mut levels = Levels { list: Vec::new(), files: Vec::new() };
    for path in files {
        let made = one(game, cfg, kind, &path).with_context(|| format!("levels/{}", path.file_name().unwrap_or_default().to_string_lossy()))?;
        levels.files.extend(made.files.iter().cloned());
        levels.list.push(Arc::new(Level::of(stem(&path), made)));
    }
    levels.files.sort();
    levels.files.dedup();
    Ok(levels)
}

/// Whether the file at `path` is a level of blocks or pieces by its first
/// line (`format = "voxcade level 1"`).
pub(crate) fn says_level(game: &Files, path: &Path) -> bool {
    path.extension().is_some_and(|e| e == crate::grid::EXTENSION)
        && game.read_to_string(path).is_ok_and(|t| t.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).is_some_and(|l| l.replace(' ', "").starts_with("format=\"voxcadelevel")))
}

/// One level from the file at `path`, of the kind `c.world` says (a level
/// of pieces whatever it says).
fn one(game: &Files, cfg: &GameConfig, kind: Kind, path: &Path) -> Result<Made> {
    let names = crate::sim::things::names(cfg);
    let free = cfg.camera.rotates == crate::config::Rotates::Free;
    match (kind, &cfg.world.blocks) {
        (Kind::Blocks, Some(blocks)) => crate::grid::load_at(game, blocks, path, &names, free),
        (Kind::World, _) if says_level(game, path) => crate::grid::load_at(game, &crate::config::BlocksConfig::default(), path, &names, free),
        (Kind::Map, _) => crate::map::load_map_at(game, path, &names),
        _ => {
            let (world, found) = crate::scene::load_in(game, path)?.world(&names).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
            Ok(Made { world, markers: found, files: vec![path.to_path_buf()], places: Vec::new(), marker_names: Vec::new(), marker_facings: Vec::new(), pieces: Vec::new(), areas: Vec::new(), daylight: Default::default() })
        }
    }
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
        let e = why("[[thing]]\nkind = \"slimee\"\nat = [1, 1, 1]\n");
        assert!(e.contains("line 5: thing at [1, 1, 1]: 'slimee' is no kind of thing"), "{e}");
        std::fs::remove_file(&file).unwrap();
        let mut cfg = GameConfig::default();
        cfg.world.file = "world.vox".into();
        let e = load::<Game>(&Files::folder(&dir), &cfg).err().unwrap().to_string();
        assert!(e.contains("world.vox") && e.contains("voxcade upgrade"), "{e}");
    }
}

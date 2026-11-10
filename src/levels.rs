//! A game's levels (`docs/game-model.md`, "Levels and places"): the files in
//! its `levels/` folder that `c.world` says are levels (levels of blocks and
//! maps are `.json`, voxel worlds `.vox`), all read when the game loads, in
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

/// A level as made from its file: its world, the markers that place its
/// things, the files it came from, its places.
pub type Made = (World, Vec<Marker>, Vec<PathBuf>, Vec<Place>);

/// One level as it was read.
pub struct Level {
    /// Its name: the file's stem (`hall` for `levels/hall.json`); `main`
    /// for a game's one level made in code.
    pub name: String,
    pub world: World,
    pub markers: Vec<Marker>,
    pub places: Vec<Place>,
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
        Self { list: vec![Arc::new(Level { name: name.to_string(), world, markers, places })], files }
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
    Vox,
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
    let kind = if cfg.world.blocks.is_some() {
        Kind::Blocks
    } else if cfg.world.file.ends_with(".json") {
        Kind::Map
    } else {
        Kind::Vox
    };
    let extension = if kind == Kind::Vox { "vox" } else { "json" };
    let mut files: Vec<PathBuf> = game.list(dir.join("levels")).into_iter().filter(|p| p.extension().is_some_and(|e| e == extension)).collect();
    files.sort_by(|a, b| natural(&stem(a), &stem(b)));
    if files.is_empty() {
        let file = match &cfg.world.blocks {
            Some(blocks) => &blocks.file,
            None => &cfg.world.file,
        };
        let path = dir.join(file);
        let (world, markers, sources, places) = one(game, cfg, kind, &path)?;
        return Ok(Levels::one(&stem(&path), world, markers, places, sources));
    }
    let mut levels = Levels { list: Vec::new(), files: Vec::new() };
    for path in files {
        let (world, markers, sources, places) = one(game, cfg, kind, &path).with_context(|| format!("levels/{}", path.file_name().unwrap_or_default().to_string_lossy()))?;
        levels.files.extend(sources);
        levels.list.push(Arc::new(Level { name: stem(&path), world, markers, places }));
    }
    levels.files.sort();
    levels.files.dedup();
    Ok(levels)
}

/// One level from the file at `path`, of the kind `c.world` says.
fn one(game: &Files, cfg: &GameConfig, kind: Kind, path: &Path) -> Result<Made> {
    let names = crate::sim::things::names(cfg);
    match (kind, &cfg.world.blocks) {
        (Kind::Blocks, Some(blocks)) => crate::grid::load_at(game, blocks, path, &names),
        (Kind::Map, _) => {
            let markers = cfg.markers.table(&names)?;
            crate::map::load_map_at(game, path, &markers, &names)
        }
        _ => {
            let markers = cfg.markers.table(&names)?;
            let (world, found) = crate::vox::load_world(game, path, &markers)?;
            Ok((world, found, vec![path.to_path_buf()], Vec::new()))
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
}

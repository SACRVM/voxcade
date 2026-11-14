//! What is kept of a game outside its folder, in the player's config
//! folder (`config_dir`): who a game folder is there (`Identity`), where
//! each thing kept of it lies (`Kept`: settings and high scores, saves,
//! kept runs, the IDE's memory), and adopting what an older Voxcade kept
//! under a folder's bare name (`settle`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The player's config folder, `voxcade` in the system's place for it:
/// `$XDG_CONFIG_HOME`, else `~/.config` (Linux, and macOS too for now: no
/// macOS release yet, Application Support would be its place), on Windows
/// `%APPDATA%` (a `voxcade` folder an older Voxcade made under `HOME`'s
/// `.config`, while there is none in `%APPDATA%`, stays in use).
pub(crate) fn config_dir() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    config_from(var("XDG_CONFIG_HOME"), var("HOME"), var("APPDATA"), cfg!(windows), &|p: &Path| p.is_dir())
}

/// `config_dir` from the variables it reads (`is_dir` asks the disk).
fn config_from(xdg: Option<PathBuf>, home: Option<PathBuf>, appdata: Option<PathBuf>, windows: bool, is_dir: &dyn Fn(&Path) -> bool) -> PathBuf {
    let under_home = home.map(|h| h.join(".config").join("voxcade"));
    let appdata = appdata.map(|a| a.join("voxcade"));
    let chosen = match (xdg, windows) {
        (Some(x), _) => Some(x.join("voxcade")),
        (None, true) => match (&appdata, &under_home) {
            (Some(a), Some(h)) if !is_dir(a) && is_dir(h) => under_home,
            _ => appdata.or(under_home),
        },
        (None, false) => under_home.or(appdata),
    };
    chosen.unwrap_or_else(|| PathBuf::from("voxcade"))
}

/// The games that come with Voxcade (the source's `games/`; the package
/// has them but the Arena and the Bench): their folders' bare names are
/// theirs alone in the config folder (`settle`; test
/// `the_games_of_voxcade_are_the_sources_games`).
pub(crate) const GAMES_OF_VOXCADE: [&str; 7] = ["arena", "bench", "bunnyfarm", "moreblood", "pulsar", "sokovox", "voxkeep"];

/// Who a game folder is for what is kept of it outside the folder: its
/// settings and high scores, saves, kept runs, and the IDE's memory,
/// OUTPUT, deleted files, journals and lock.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Identity {
    /// The folder's name.
    pub name: String,
    /// The name and the first 8 signs of the BLAKE3 of the folder's
    /// canonical path (`voxkeep-1a2b3c4d`): this folder and no other.
    pub folder: String,
    /// What its kept things are named by: the bare name for a game that
    /// comes with Voxcade (`comes_with_voxcade`: its high scores survive an
    /// update and a package unpacked elsewhere), else `folder`.
    pub key: String,
}

impl Identity {
    pub fn of(game_dir: &Path) -> Self {
        let path = game_dir.canonicalize().or_else(|_| std::path::absolute(game_dir)).unwrap_or_else(|_| game_dir.to_path_buf());
        let name = path.file_name().map_or_else(|| "game".to_string(), |n| n.to_string_lossy().into_owned());
        let hash = blake3::hash(path.to_string_lossy().as_bytes()).to_hex();
        let folder = format!("{name}-{}", &hash[..8]);
        let key = if comes_with_voxcade(&path) { name.clone() } else { folder.clone() };
        Self { name, folder, key }
    }
}

/// Whether the game folder `dir` (canonical) is one that comes with
/// Voxcade: it lies in a `games` folder beside a Voxcade program (a
/// package) or in a Voxcade source tree's. Told by where it lies, not by
/// which program asks, so every program names it alike.
fn comes_with_voxcade(dir: &Path) -> bool {
    let Some(games) = dir.parent().filter(|g| g.file_name().is_some_and(|n| n == "games")) else { return false };
    let Some(root) = games.parent() else { return false };
    root.join("voxcade").is_file() || root.join("voxcade.exe").is_file() || std::fs::read_to_string(root.join("Cargo.toml")).is_ok_and(|t| t.lines().any(|l| l.trim() == "name = \"voxcade\""))
}

/// What is kept of a game in the config folder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kept {
    /// The player's settings and high scores, `<key>.toml`.
    Settings,
    /// A settings file that would not read, kept aside (`Settings::load_for`).
    Broken,
    /// The save games, `saves/<key>/`.
    Saves,
    /// The runs kept from the window, `replays/<key>/`.
    Runs,
    /// What the IDE remembers, `ide/<key>.toml`.
    IdeMemory,
    /// OUTPUT's lines, `ide/output/<key>.toml`.
    IdeOutput,
    /// Files deleted where there was no trash, `ide/deleted/<key>/`.
    IdeDeleted,
    /// The open texts' journals, `ide/<folder>.journal/`: this folder's
    /// files only, never adopted.
    Journals,
    /// The IDE's MCP lock, `ide/<folder>.lock`: this folder's IDE only.
    Lock,
}

impl Kept {
    /// What a game adopts of what an older Voxcade kept under its bare name.
    const ADOPTED: [Kept; 7] = [Kept::Settings, Kept::Broken, Kept::Saves, Kept::Runs, Kept::IdeMemory, Kept::IdeOutput, Kept::IdeDeleted];

    /// Where it lies in the config folder `config` for the name `key`.
    fn at(self, config: &Path, key: &str) -> PathBuf {
        let ide = config.join("ide");
        match self {
            Kept::Settings => config.join(format!("{key}.toml")),
            Kept::Broken => config.join(format!("{key}.toml.broken")),
            Kept::Saves => config.join("saves").join(key),
            Kept::Runs => config.join("replays").join(key),
            Kept::IdeMemory => ide.join(format!("{key}.toml")),
            Kept::IdeOutput => ide.join("output").join(format!("{key}.toml")),
            Kept::IdeDeleted => ide.join("deleted").join(key),
            Kept::Journals => ide.join(format!("{key}.journal")),
            Kept::Lock => ide.join(format!("{key}.lock")),
        }
    }

    /// The name of `id` this is kept under.
    fn name_of(self, id: &Identity) -> &str {
        match self {
            Kept::Journals | Kept::Lock => &id.folder,
            _ => &id.key,
        }
    }
}

/// Where `what` of the game in `game_dir` is kept.
pub(crate) fn kept_for(game_dir: &Path, what: Kept) -> PathBuf {
    what.at(&config_dir(), what.name_of(&Identity::of(game_dir)))
}

/// Where the runs played in the window are kept: beside the settings,
/// `voxcade/replays/<key>/`.
pub fn replays_for(game_dir: &Path) -> PathBuf {
    kept_for(game_dir, Kept::Runs)
}

/// The list of the game folders the config folder's names stand for
/// (`folders/index.toml`): written by `settle`, read by people.
const FOLDERS: &str = "voxcade folders 1";

#[derive(Default, Deserialize, Serialize)]
struct Folders {
    format: String,
    /// Each key settled (`Identity::key`), its folder's path.
    #[serde(default)]
    folders: BTreeMap<String, String>,
}

/// Settles who the game in `game_dir` is in the config folder, before a
/// window plays it or the IDE opens it (`settle_in`).
pub fn settle(game_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let beside = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("games")));
    let claimed = |name: &str| GAMES_OF_VOXCADE.contains(&name) || beside.as_ref().is_some_and(|b| b.join(name).join("conf.lua").is_file());
    let moved = settle_in(&config_dir(), game_dir, &claimed);
    for (from, to) in &moved {
        eprintln!("kept for {}: {} is now {}", game_dir.display(), from.display(), to.display());
    }
    moved
}

/// The adoption rule: a game that comes with Voxcade keeps its bare name
/// and nothing moves. Another game, the first time it is settled (its key not yet in `folders/index.toml`),
/// adopts what an older Voxcade kept under its folder's bare name, unless
/// that name is `claimed` by a game that comes with Voxcade (a copy of
/// Voxkeep never takes Voxkeep's saves): each such thing is moved to its
/// key where the key has none yet, so a second folder of the same name
/// starts clean. Then the key goes into the index with the folder's path,
/// and nothing is adopted for it again. A folder moved or renamed is
/// another folder: it starts clean, what was kept under its old key stays
/// there. What moved, from and to.
fn settle_in(config: &Path, game_dir: &Path, claimed: &dyn Fn(&str) -> bool) -> Vec<(PathBuf, PathBuf)> {
    let id = Identity::of(game_dir);
    let mut moved = Vec::new();
    if id.key == id.name {
        return moved;
    }
    let index = config.join("folders").join("index.toml");
    let mut folders: Folders = std::fs::read_to_string(&index).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default();
    if folders.folders.contains_key(&id.key) {
        return moved;
    }
    if !claimed(&id.name) {
        for what in Kept::ADOPTED {
            let (from, to) = (what.at(config, &id.name), what.at(config, &id.key));
            // `ide/claude.toml` was the IDE's own once (`Prefs::load`).
            if what == Kept::IdeMemory && id.name == "claude" {
                continue;
            }
            if from.exists() && !to.exists() {
                if let Some(parent) = to.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::rename(&from, &to) {
                    Ok(()) => moved.push((from, to)),
                    Err(e) => eprintln!("kept for {}: {} stays ({e})", game_dir.display(), from.display()),
                }
            }
        }
    }
    folders.format = FOLDERS.into();
    let path = game_dir.canonicalize().unwrap_or_else(|_| game_dir.to_path_buf());
    folders.folders.insert(id.key, path.to_string_lossy().into_owned());
    match toml::to_string(&folders) {
        Ok(text) => {
            if let Err(e) = crate::blocks::save_at_once(&index, text.as_bytes()) {
                eprintln!("{}: {e:#}", index.display());
            }
        }
        Err(e) => eprintln!("{}: {e}", index.display()),
    }
    moved
}

/// The game folder's name (what a network game is known by,
/// `network::app_of`, never `Identity`'s key).
pub(crate) fn game_name(game_dir: &Path) -> String {
    game_dir
        .canonicalize()
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "game".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game folder with a `conf.lua` at `dir`.
    fn game_at(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c) end\n").unwrap();
        dir.canonicalize().unwrap()
    }

    /// Everything an older Voxcade kept for the name `key` in `config`,
    /// each saying `said`.
    fn keep_all(config: &Path, key: &str, said: &str) {
        for what in Kept::ADOPTED {
            let at = what.at(config, key);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            if matches!(what, Kept::Saves | Kept::Runs | Kept::IdeDeleted) {
                std::fs::create_dir_all(&at).unwrap();
                std::fs::write(at.join("1.toml"), said).unwrap();
            } else {
                std::fs::write(&at, said).unwrap();
            }
        }
    }

    /// What `config` holds for the name `key`, thing by thing (None: not there).
    fn held(config: &Path, key: &str) -> Vec<Option<String>> {
        Kept::ADOPTED
            .iter()
            .map(|what| {
                let at = what.at(config, key);
                let at = if at.is_dir() { at.join("1.toml") } else { at };
                std::fs::read_to_string(at).ok()
            })
            .collect()
    }

    fn all(said: &str) -> Vec<Option<String>> {
        vec![Some(said.to_string()); Kept::ADOPTED.len()]
    }

    fn none() -> Vec<Option<String>> {
        vec![None; Kept::ADOPTED.len()]
    }

    /// A game folder is known by its name and a short hash of its whole
    /// path, two folders of one name apart; a game that comes with Voxcade
    /// (in the `games` folder of a package or of Voxcade's source) keeps
    /// its bare name, whichever program asks. The journals and the lock
    /// are always the folder's own.
    #[test]
    fn a_game_is_known_by_its_folders_path() {
        let root = crate::scratch::Scratch::new("identity");
        let package = root.join("package");
        game_at(&package.join("games").join("voxkeep"));
        std::fs::write(package.join("voxcade"), "the program").unwrap();
        let source = root.join("source");
        game_at(&source.join("games").join("voxkeep"));
        std::fs::write(source.join("Cargo.toml"), "[package]\nname = \"voxcade\"\n").unwrap();
        let other_source = root.join("other");
        game_at(&other_source.join("games").join("voxkeep"));
        std::fs::write(other_source.join("Cargo.toml"), "[package]\nname = \"another\"\n").unwrap();
        let own = game_at(&root.join("projects").join("voxkeep"));
        let own_too = game_at(&root.join("more").join("voxkeep"));
        for dir in [package.join("games/voxkeep"), source.join("games/voxkeep")] {
            let id = Identity::of(&dir);
            assert_eq!((id.name.as_str(), id.key.as_str()), ("voxkeep", "voxkeep"), "{}", dir.display());
            assert_ne!(id.folder, id.key, "its journals are its folder's own");
        }
        let (a, b, c) = (Identity::of(&own), Identity::of(&own_too), Identity::of(&other_source.join("games/voxkeep")));
        for id in [&a, &b, &c] {
            assert_eq!(id.key, id.folder);
            assert!(id.key.starts_with("voxkeep-") && id.key.len() == "voxkeep-".len() + 8 && id.key[8..].chars().all(|c| c.is_ascii_hexdigit()), "{}", id.key);
        }
        assert!(a.key != b.key && b.key != c.key && a.key != c.key, "two folders of one name");
        assert_eq!(Identity::of(&root.join("projects").join(".").join("voxkeep")), a, "the same folder however written");
        let config = root.join("config");
        assert_eq!(Kept::Journals.at(&config, Kept::Journals.name_of(&a)), config.join("ide").join(format!("{}.journal", a.folder)));
        assert_eq!(Kept::Settings.at(&config, Kept::Settings.name_of(&Identity::of(&source.join("games/voxkeep")))), config.join("voxkeep.toml"));
    }

    /// The adoption rule, every branch: a game of its own adopts what an
    /// older Voxcade kept under its bare name, moved, once; a second folder
    /// of that name starts clean; nothing is adopted again once settled; a
    /// thing its key already has is not written over; a game that comes
    /// with Voxcade moves nothing and keeps its name; a copy of one never
    /// takes its things; a folder moved starts clean and what was kept
    /// stays; the IDE's own `claude.toml` is no game's.
    #[test]
    fn a_game_adopts_what_its_bare_name_kept_once() {
        let root = crate::scratch::Scratch::new("adoption");
        let config = root.join("config");
        let claimed = |name: &str| name == "voxkeep";

        // A game of its own adopts, moved.
        keep_all(&config, "test", "old");
        let first = game_at(&root.join("a").join("test"));
        let moved = settle_in(&config, &first, &claimed);
        assert_eq!(moved.len(), Kept::ADOPTED.len());
        let key = Identity::of(&first).key;
        assert_eq!(held(&config, &key), all("old"));
        assert_eq!(held(&config, "test"), none(), "moved, not copied");
        let index = std::fs::read_to_string(config.join("folders/index.toml")).unwrap();
        assert!(index.starts_with("format = \"voxcade folders 1\"") && index.contains(&key) && index.contains(&*first.to_string_lossy()), "{index}");

        // A second folder of the name starts clean.
        let second = game_at(&root.join("b").join("test"));
        assert!(settle_in(&config, &second, &claimed).is_empty());
        assert_eq!(held(&config, &Identity::of(&second).key), none());

        // Settled once: what an older Voxcade keeps under the name later is not taken.
        keep_all(&config, "test", "later");
        assert!(settle_in(&config, &first, &claimed).is_empty() && settle_in(&config, &second, &claimed).is_empty());
        assert_eq!((held(&config, &key), held(&config, "test")), (all("old"), all("later")));

        // What the key has already is not written over.
        let third = game_at(&root.join("c").join("test"));
        let third_key = Identity::of(&third).key;
        std::fs::write(Kept::Settings.at(&config, &third_key), "mine").unwrap();
        let moved = settle_in(&config, &third, &claimed);
        assert_eq!(moved.len(), Kept::ADOPTED.len() - 1);
        assert_eq!(std::fs::read_to_string(Kept::Settings.at(&config, &third_key)).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(Kept::Settings.at(&config, "test")).unwrap(), "later", "left where it was");

        // A game that comes with Voxcade keeps its bare name; a copy of one takes nothing.
        keep_all(&config, "voxkeep", "voxkeep's");
        let source = root.join("source");
        std::fs::write({ std::fs::create_dir_all(&source).unwrap(); source.join("Cargo.toml") }, "[package]\nname = \"voxcade\"\n").unwrap();
        let bundled = game_at(&source.join("games").join("voxkeep"));
        assert!(settle_in(&config, &bundled, &claimed).is_empty());
        let copy = game_at(&root.join("projects").join("voxkeep"));
        assert!(settle_in(&config, &copy, &claimed).is_empty(), "a copy took the bundled game's things");
        assert_eq!(held(&config, "voxkeep"), all("voxkeep's"));
        assert_eq!(held(&config, &Identity::of(&copy).key), none());
        assert!(!std::fs::read_to_string(config.join("folders/index.toml")).unwrap().contains("\"voxkeep\""), "a bundled game is in no index");

        // A folder moved is another: it starts clean, its old things stay.
        std::fs::remove_file(Kept::Settings.at(&config, "test")).unwrap();
        let away = root.join("moved").join("test");
        std::fs::create_dir_all(away.parent().unwrap()).unwrap();
        std::fs::rename(&first, &away).unwrap();
        assert!(settle_in(&config, &away, &claimed).is_empty());
        assert_eq!(held(&config, &Identity::of(&away).key), none());
        assert_eq!(held(&config, &key), all("old"), "kept under the old key");

        // `ide/claude.toml` is the IDE's own.
        std::fs::create_dir_all(config.join("ide")).unwrap();
        std::fs::write(config.join("ide/claude.toml"), "resume = true\n").unwrap();
        let claude = game_at(&root.join("d").join("claude"));
        settle_in(&config, &claude, &claimed);
        assert!(config.join("ide/claude.toml").is_file());
    }

    /// The config folder: `XDG_CONFIG_HOME`'s, else `~/.config`'s; on
    /// Windows `%APPDATA%`'s, unless only an older Voxcade's under `HOME`
    /// is there, which stays in use.
    #[test]
    fn the_config_folder_is_the_systems_place_for_it() {
        let p = |s: &str| Some(PathBuf::from(s));
        let none = |_: &Path| false;
        let home_has = |d: &Path| d == Path::new("/h/.config/voxcade");
        assert_eq!(config_from(p("/x"), p("/h"), p("/a"), true, &none), PathBuf::from("/x/voxcade"));
        assert_eq!(config_from(None, p("/h"), p("/a"), false, &none), PathBuf::from("/h/.config/voxcade"));
        assert_eq!(config_from(None, p("/h"), p("/a"), true, &none), PathBuf::from("/a/voxcade"), "Windows: APPDATA before HOME");
        assert_eq!(config_from(None, p("/h"), p("/a"), true, &home_has), PathBuf::from("/h/.config/voxcade"), "an older Voxcade's stays in use");
        let both = |d: &Path| d == Path::new("/h/.config/voxcade") || d == Path::new("/a/voxcade");
        assert_eq!(config_from(None, p("/h"), p("/a"), true, &both), PathBuf::from("/a/voxcade"));
        assert_eq!(config_from(None, None, p("/a"), false, &none), PathBuf::from("/a/voxcade"));
        assert_eq!(config_from(None, p("/h"), None, true, &none), PathBuf::from("/h/.config/voxcade"));
    }

    /// `GAMES_OF_VOXCADE` names the source's games, no more and no fewer.
    #[test]
    fn the_games_of_voxcade_are_the_sources_games() {
        let mut games: Vec<String> = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("games")).unwrap().flatten().filter(|e| e.path().join("conf.lua").is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        games.sort();
        assert_eq!(games, GAMES_OF_VOXCADE);
    }
}

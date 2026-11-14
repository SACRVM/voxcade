//! The files a game is made of, as the engine reads them: from its folder,
//! or from bytes laid over it (`docs/multiplayer.md`, stage 6, "The engine
//! loads files from bytes"). A session reads every file of its game
//! through its `Files`, so a game can play files sealed at a moment (a
//! JAM's, a replay's) while the folder on disk goes on changing, and no
//! replay ever writes a game folder. The bytes are kept by their content
//! in a `Store`; a `Set` names a game's files by path and hash.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};

/// A file's content: its bytes and their hash (`hash`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub hash: String,
    pub bytes: Arc<[u8]>,
}

impl File {
    pub fn of(bytes: Vec<u8>) -> Self {
        Self { hash: hash(&bytes), bytes: bytes.into() }
    }
}

/// A game's files by their names (relative to the game folder, `/`
/// between folders) and the hashes of their bytes.
pub type Set = BTreeMap<String, String>;

/// Files changed at once (an APPLY), by name: the new content, or None
/// where a file went.
pub type Changes = BTreeMap<String, Option<File>>;

/// The hash a file is known by: BLAKE3 of its bytes, in hex.
pub fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// The folders of a game folder that hold no part of the game: what BUILD
/// leaves out, and the recorded runs. (An agent's notes at the top of the
/// folder, `CLAUDE.md`, are none of the game either: `ide::is_notes_file`.)
const NOT_THE_GAME: [&str; 4] = ["builds", "shots", "target", "replays"];

/// A game folder's files as the engine reads them.
#[derive(Clone, Debug)]
pub struct Files {
    dir: PathBuf,
    /// Files laid over the folder, by name (None: gone).
    over: Arc<BTreeMap<String, Option<File>>>,
    /// Only the files laid over it count: the folder itself is not read.
    sealed: bool,
}

impl Files {
    /// The disk itself: paths read as they are (the editors' and tools'
    /// files, which are no game's).
    pub fn disk() -> Self {
        Self::folder(Path::new(""))
    }

    /// The folder as it is on disk.
    pub fn folder(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf(), over: Default::default(), sealed: false }
    }

    /// Exactly these files, named as in `dir`: nothing else of the folder
    /// is read.
    pub fn sealed(dir: &Path, files: BTreeMap<String, File>) -> Self {
        Self { dir: dir.to_path_buf(), over: Arc::new(files.into_iter().map(|(name, file)| (name, Some(file))).collect()), sealed: true }
    }

    /// Seals the game folder `dir` as it is now: every file of the game
    /// (not hidden ones, nor `builds/`, `shots/`, `target/`, `replays/`,
    /// nor links) kept in `store`, and the files as they are.
    pub fn seal(dir: &Path, store: &Store) -> Result<(Set, Self)> {
        let mut files = BTreeMap::new();
        gather(dir, dir, &mut files)?;
        let mut set = Set::new();
        for (name, file) in &files {
            store.put_file(file)?;
            set.insert(name.clone(), file.hash.clone());
        }
        Ok((set, Self::sealed(dir, files)))
    }

    /// The files of `set`, their bytes from `store`.
    pub fn of_set(dir: &Path, set: &Set, store: &Store) -> Result<Self> {
        let files = set.iter().map(|(name, hash)| Ok((name.clone(), store.get(hash).with_context(|| format!("{name} ({hash})"))?))).collect::<Result<_>>()?;
        Ok(Self::sealed(dir, files))
    }

    /// What changed in the folder since these files were sealed: each file
    /// new or other (kept in `store`), and those gone. Nothing for files
    /// not sealed.
    pub fn changes_since(&self, store: &Store) -> Result<Changes> {
        let Some(set) = self.set() else { return Ok(Changes::new()) };
        let mut now = BTreeMap::new();
        gather(&self.dir, &self.dir, &mut now)?;
        let mut changes: Changes = set.keys().filter(|n| !now.contains_key(*n)).map(|n| (n.clone(), None)).collect();
        for (name, file) in now {
            if set.get(&name) != Some(&file.hash) {
                store.put_file(&file)?;
                changes.insert(name, Some(file));
            }
        }
        Ok(changes)
    }

    /// These files with `changes` laid over them.
    pub fn changed(&self, changes: &Changes) -> Self {
        let mut over = (*self.over).clone();
        for (name, file) in changes {
            over.insert(name.clone(), file.clone());
        }
        Self { dir: self.dir.clone(), over: Arc::new(over), sealed: self.sealed }
    }

    /// The names whose content differs between these files and `other`
    /// (the files of a game before and after an APPLY).
    pub fn differing(&self, other: &Files) -> Vec<String> {
        if Arc::ptr_eq(&self.over, &other.over) && self.sealed == other.sealed {
            return Vec::new();
        }
        let hash = |files: &Files, name: &str| files.over.get(name).map(|f| f.as_ref().map(|f| f.hash.clone()));
        let names: std::collections::BTreeSet<&String> = self.over.keys().chain(other.over.keys()).collect();
        names.into_iter().filter(|n| hash(self, n) != hash(other, n)).cloned().collect()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether only the files laid over the folder count (a sealed set):
    /// a change on disk is none of the game's.
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    /// The set of a sealed game's files (None: the folder counts).
    pub fn set(&self) -> Option<Set> {
        self.sealed.then(|| self.over.iter().filter_map(|(name, file)| Some((name.clone(), file.as_ref()?.hash.clone()))).collect())
    }

    /// A file's name in the game (relative, `/` between folders), for a
    /// path in its folder or relative to it; None for one outside.
    pub fn name(&self, path: &Path) -> Option<String> {
        let rel = if path.is_absolute() || path.starts_with(&self.dir) { path.strip_prefix(&self.dir).ok()? } else { path };
        let mut parts = Vec::new();
        for part in rel.components() {
            match part {
                Component::Normal(p) => parts.push(p.to_str()?.to_string()),
                Component::CurDir => {}
                _ => return None,
            }
        }
        (!parts.is_empty()).then(|| parts.join("/"))
    }

    /// A file's path in the folder, for its name or a path relative to it.
    pub fn path(&self, file: impl AsRef<Path>) -> PathBuf {
        let file = file.as_ref();
        if file.is_absolute() || file.starts_with(&self.dir) { file.to_path_buf() } else { self.dir.join(file) }
    }

    /// What lies over the folder for this path: Some(None) where the file
    /// went (or is none of a sealed game's), None where the folder counts.
    fn laid(&self, path: &Path) -> Option<Option<&File>> {
        let name = self.name(path);
        match name.as_deref().and_then(|n| self.over.get(n)) {
            Some(file) => Some(file.as_ref()),
            None if self.sealed => Some(None),
            None => None,
        }
    }

    /// A file's bytes (a path in the folder, or relative to it).
    pub fn read(&self, file: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let path = self.path(file);
        match self.laid(&path) {
            Some(Some(f)) => Ok(f.bytes.to_vec()),
            Some(None) => Err(io::Error::new(io::ErrorKind::NotFound, format!("{}: no such file in the game's files", path.display()))),
            None => std::fs::read(&path),
        }
    }

    /// A text file.
    pub fn read_to_string(&self, file: impl AsRef<Path>) -> io::Result<String> {
        String::from_utf8(self.read(file)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn is_file(&self, file: impl AsRef<Path>) -> bool {
        let path = self.path(file);
        match self.laid(&path) {
            Some(found) => found.is_some(),
            None => path.is_file(),
        }
    }

    /// The files directly in a folder of the game, as paths, in order of
    /// their names.
    pub fn list(&self, folder: impl AsRef<Path>) -> Vec<PathBuf> {
        let folder = self.path(folder);
        let mut names: std::collections::BTreeSet<String> = Default::default();
        if !self.sealed {
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                if entry.file_type().is_ok_and(|t| t.is_file())
                    && let Some(name) = entry.file_name().to_str()
                {
                    names.insert(name.to_string());
                }
            }
        }
        let prefix = self.name(&folder).map(|n| format!("{n}/")).unwrap_or_default();
        for (name, file) in self.over.iter() {
            let Some(rest) = name.strip_prefix(&prefix).filter(|r| !r.contains('/')) else { continue };
            match file {
                Some(_) => names.insert(rest.to_string()),
                None => names.remove(rest),
            };
        }
        names.into_iter().map(|n| folder.join(n)).collect()
    }
}

/// A path's parts with `/` between, as the game names a file on every
/// system (`main.lua`, `levels/hall.toml`).
pub fn slashed(path: &Path) -> String {
    path.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Every file of the game under `folder`, by its name in `dir`.
fn gather(dir: &Path, folder: &Path, files: &mut BTreeMap<String, File>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))?.collect::<io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if name.starts_with('.') || (folder == dir && (NOT_THE_GAME.contains(&name.as_str()) || crate::ide::is_notes_file(&name))) {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            gather(dir, &entry.path(), files)?;
        } else if kind.is_file() {
            let path = entry.path();
            let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            files.insert(slashed(path.strip_prefix(dir)?), File::of(bytes));
        }
    }
    Ok(())
}

/// Files kept by their content (`hash`): each under its hash, written once.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn at(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf() }
    }

    /// This machine's, in the config folder (`jam/objects/`).
    pub fn local() -> Self {
        Self::at(&crate::settings::config_dir().join("jam").join("objects"))
    }

    fn path_of(&self, hash: &str) -> Result<PathBuf> {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
            bail!("\"{hash}\" is no file's hash");
        }
        Ok(self.dir.join(&hash[..2]).join(hash))
    }

    /// Keeps `bytes`; their hash.
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let file = File::of(bytes.to_vec());
        self.put_file(&file)?;
        Ok(file.hash)
    }

    pub fn put_file(&self, file: &File) -> Result<()> {
        let path = self.path_of(&file.hash)?;
        if path.is_file() {
            return Ok(());
        }
        let folder = path.parent().context("the store's folder")?;
        std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
        // Whole or not at all: written beside it, then renamed.
        let part = folder.join(format!("{}.part{}", file.hash, std::process::id()));
        std::fs::write(&part, &file.bytes).with_context(|| format!("writing {}", part.display()))?;
        std::fs::rename(&part, &path).with_context(|| format!("writing {}", path.display()))
    }

    /// The file kept under `hash`, its bytes checked against it.
    pub fn get(&self, hash: &str) -> Result<File> {
        let path = self.path_of(hash)?;
        let bytes = std::fs::read(&path).with_context(|| format!("no file {hash} in the store ({})", self.dir.display()))?;
        let file = File::of(bytes);
        if file.hash != hash {
            bail!("the store's file {hash} is damaged");
        }
        Ok(file)
    }

    pub fn has(&self, hash: &str) -> bool {
        self.path_of(hash).is_ok_and(|p| p.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> crate::scratch::Scratch {
        crate::scratch::Scratch::new(&format!("files-{name}"))
    }

    /// A sealed game reads its files as they were sealed, whatever the
    /// folder holds since; changes laid over it come and go.
    #[test]
    fn a_sealed_game_reads_its_files_as_sealed() {
        let dir = scratch("sealed");
        let store = Store::at(&dir.join("store"));
        let game = dir.join("game");
        std::fs::create_dir_all(game.join("levels")).unwrap();
        std::fs::create_dir_all(game.join(".git")).unwrap();
        std::fs::create_dir_all(game.join("shots")).unwrap();
        std::fs::write(game.join("CLAUDE.md"), "notes").unwrap();
        std::fs::write(game.join("main.lua"), "-- one").unwrap();
        std::fs::write(game.join("levels/01.json"), "{}").unwrap();
        std::fs::write(game.join(".git/HEAD"), "x").unwrap();
        std::fs::write(game.join("shots/a.png"), "x").unwrap();
        let (set, files) = Files::seal(&game, &store).unwrap();
        assert_eq!(set.keys().collect::<Vec<_>>(), ["levels/01.json", "main.lua"], "hidden files, shots/ and the agent's notes at the top are none of the game");
        assert_eq!(files.set().unwrap(), set);
        std::fs::write(game.join("main.lua"), "-- two").unwrap();
        std::fs::write(game.join("levels/02.json"), "{}").unwrap();
        assert_eq!(files.read_to_string("main.lua").unwrap(), "-- one");
        assert_eq!(files.read_to_string(game.join("main.lua")).unwrap(), "-- one", "a path in the folder reads the same");
        assert_eq!(files.list("levels"), [game.join("levels/01.json")], "a file new on disk is none of a sealed game's");
        assert!(!files.is_file("levels/02.json"));
        // The set again, from the store alone.
        let again = Files::of_set(&game, &set, &store).unwrap();
        assert_eq!(again.read("main.lua").unwrap(), b"-- one");
        // An APPLY: one file changed, one gone, one new.
        let changes: Changes = [("main.lua".to_string(), Some(File::of(b"-- three".to_vec()))), ("levels/01.json".to_string(), None), ("levels/03.json".to_string(), Some(File::of(b"[]".to_vec())))].into();
        let changed = files.changed(&changes);
        assert_eq!(changed.read_to_string("main.lua").unwrap(), "-- three");
        assert_eq!(changed.list("levels"), [game.join("levels/03.json")]);
        assert!(changed.read("levels/01.json").is_err());
        assert_eq!(files.read_to_string("main.lua").unwrap(), "-- one", "the files before stay as they were");
        // What the folder holds since, as changes to apply.
        let since = files.changes_since(&store).unwrap();
        assert_eq!(since.keys().collect::<Vec<_>>(), ["levels/02.json", "main.lua"]);
        assert_eq!(&*since["main.lua"].as_ref().unwrap().bytes, b"-- two");
        assert!(store.has(&since["levels/02.json"].as_ref().unwrap().hash), "a changed file is kept to be sent");
        // The folder itself, with a change laid over it.
        let folder = Files::folder(&game).changed(&changes);
        assert_eq!(folder.list("levels"), [game.join("levels/02.json"), game.join("levels/03.json")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The store keeps a file once under its hash and refuses names that
    /// are no hash (a path out of it) and bytes that do not fit theirs.
    #[test]
    fn the_store_keeps_files_by_their_content() {
        let dir = scratch("store");
        let store = Store::at(&dir);
        let hash = store.put(b"hello").unwrap();
        assert_eq!(hash, super::hash(b"hello"));
        assert_eq!(store.put(b"hello").unwrap(), hash);
        assert_eq!(&*store.get(&hash).unwrap().bytes, b"hello");
        assert!(store.get("../../etc/passwd").is_err());
        assert!(store.get(&hash.to_uppercase()).is_err());
        std::fs::write(store.path_of(&hash).unwrap(), b"tampered").unwrap();
        assert!(store.get(&hash).is_err(), "a damaged file is no file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A set naming a file the store does not have, or has damaged, does
    /// not make files: the mistake names the file and its hash.
    #[test]
    fn a_sealed_file_missing_from_the_store_is_named() {
        let dir = scratch("missing");
        let store = Store::at(&dir.join("store"));
        let kept = store.put(b"-- main").unwrap();
        let absent = super::hash(b"never kept");
        let set: Set = [("main.lua".to_string(), kept.clone()), ("levels/01.toml".to_string(), absent.clone())].into();
        let e = format!("{:#}", Files::of_set(&dir, &set, &store).unwrap_err());
        assert!(e.contains("levels/01.toml") && e.contains(&absent) && e.contains("no file"), "{e}");
        std::fs::write(store.path_of(&kept).unwrap(), b"-- changed behind its back").unwrap();
        let set: Set = [("main.lua".to_string(), kept.clone())].into();
        let e = format!("{:#}", Files::of_set(&dir, &set, &store).unwrap_err());
        assert!(e.contains("main.lua") && e.contains("damaged"), "{e}");
        let set: Set = [("main.lua".to_string(), "../../x".to_string())].into();
        assert!(Files::of_set(&dir, &set, &store).is_err(), "a hash that is no hash");
    }

    /// A path leaving the game folder has no name in the game, and a
    /// sealed game reads nothing there, however it is written.
    #[test]
    fn a_path_leaving_the_game_folder_is_none_of_the_games() {
        let dir = scratch("leaving");
        let game = dir.join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("main.lua"), "-- main").unwrap();
        std::fs::write(dir.join("secret.txt"), "not the game's").unwrap();
        let (_, files) = Files::seal(&game, &Store::at(&dir.join("store"))).unwrap();
        let beside = dir.join("secret.txt");
        for outside in [Path::new("../secret.txt"), Path::new("levels/../../secret.txt"), beside.as_path()] {
            assert_eq!(files.name(outside), None, "{}", outside.display());
            assert!(files.read(outside).is_err(), "{} read from a sealed game", outside.display());
            assert!(!files.is_file(outside), "{}", outside.display());
        }
        assert_eq!(files.name(Path::new("./main.lua")).as_deref(), Some("main.lua"));
        assert_eq!(files.read_to_string("./main.lua").unwrap(), "-- main");
        assert!(Files::folder(&game).changes_since(&Store::at(&dir.join("store"))).unwrap().is_empty(), "a folder not sealed has no changes to send");
    }
}

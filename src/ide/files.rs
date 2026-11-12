//! A game folder as the IDE lists it: its logic (the Lua scripts, and the
//! text files they `read`), its scenarios (`scenarios/NAME.lua`: runs
//! begun late, to try things) and its assets in a few groups (the level,
//! models, blocks, images, sounds, music, the rest; the models, sounds and
//! music listed even when empty). Hidden files, the recorded runs
//! (`replays/`), what BUILD made (`builds/`), the test renders `shots/` and
//! whatever the game's `.gitignore` files ignore (`crate::gitignore`) are
//! left out: the list shows what belongs to the game. (The engine still
//! reads an ignored file the game names; only the lists leave it out.)

use std::path::Path;

use crate::gitignore::Ignore;

/// The folder's files, by their paths in it (with `/` between folders).
#[derive(Debug, Default, PartialEq)]
pub struct Files {
    pub logic: Vec<String>,
    pub scenarios: Vec<String>,
    pub assets: Vec<Group>,
}

/// Assets of one sort.
#[derive(Debug, PartialEq)]
pub struct Group {
    pub name: &'static str,
    pub files: Vec<String>,
}

/// The groups, in the order the IDE shows them.
const GROUPS: [&str; 7] = ["Levels", "Models", "Blocks", "Images", "Sounds", "Music", "Other"];

/// The sorts listed even with no file.
const ALWAYS: [&str; 4] = ["Levels", "Models", "Sounds", "Music"];

/// How deep the IDE looks into the folder's folders.
const DEPTH: usize = 5;

impl Files {
    /// The game folder `dir` as the IDE lists it.
    #[cfg(test)]
    pub fn scan(dir: &Path) -> Self {
        Self::from_names(dir, Self::names(dir))
    }

    /// The names of the folder's files as the list would show them, sorted:
    /// no reading of any file, so cheap enough to look at every second to
    /// see whether `scan` has anything new to say.
    pub fn names(dir: &Path) -> Vec<String> {
        let mut all = Vec::new();
        walk(dir, "", 0, &Ignore::for_game(dir), &mut all);
        all.sort();
        all
    }

    /// The folder's files as the IDE lists them, from their `names`.
    pub fn from_names(dir: &Path, all: Vec<String>) -> Self {
        let scripts: Vec<String> = all.iter().filter(|f| f.ends_with(".lua")).filter_map(|f| std::fs::read_to_string(dir.join(f)).ok()).collect();
        let read_by_a_script = |f: &str| scripts.iter().any(|s| s.contains(&format!("read(\"{f}\")")) || s.contains(&format!("read('{f}')")));
        let mut files = Files::default();
        let mut groups: Vec<Vec<String>> = vec![Vec::new(); GROUPS.len()];
        for f in all {
            let name = f.rsplit('/').next().unwrap_or(&f);
            let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
            let group = match ext {
                "lua" if f.starts_with("scenarios/") && !f["scenarios/".len()..].contains('/') => {
                    files.scenarios.push(f);
                    continue;
                }
                "lua" => {
                    files.logic.push(f);
                    continue;
                }
                "txt" if read_by_a_script(&f) => {
                    files.logic.push(f);
                    continue;
                }
                // The levels the game loads lie at the top (a Tiled map
                // beside one is only where it came from).
                // The worlds and a map's prefabs with them, by where games
                // keep them (shown, not shaped as models).
                _ if f == "level.toml" || f == "map.toml" || (f.starts_with("levels/") && (ext == "toml" || ext == "vvx")) => 0,
                "vvx" if f == "world.vvx" || f.starts_with("prefabs/") => 0,
                "vvx" | "vox" if f.starts_with("blocks/") => 2,
                "vvx" | "vox" => 1,
                "png" => 3,
                // The sounds the game plays by name: the files of `sounds/`
                // itself (a recording there is none, an older game's).
                "toml" if f.starts_with("sounds/") && !f["sounds/".len()..].contains('/') => 4,
                "mml" => 5,
                _ => 6,
            };
            groups[group].push(f);
        }
        // The game's own two first, the other scripts after them.
        files.logic.sort_by_key(|f| (["conf.lua", "main.lua"].iter().position(|n| n == f).unwrap_or(2), f.clone()));
        // The levels, models, sounds and music always have their place (the
        // IDE's plus there makes the first one); the other sorts show when
        // there are some.
        files.assets = GROUPS.iter().zip(groups).filter(|(name, g)| !g.is_empty() || ALWAYS.contains(name)).map(|(&name, files)| Group { name, files }).collect();
        files
    }

    /// Whether the folder has this file.
    pub fn has(&self, path: &str) -> bool {
        self.logic.iter().chain(&self.scenarios).chain(self.assets.iter().flat_map(|g| &g.files)).any(|f| f == path)
    }

    /// The heading the list shows the file under: "Logic", "Scenarios" or
    /// an asset group's name (None: the folder has no such file).
    pub fn sort_of(&self, path: &str) -> Option<&'static str> {
        if self.logic.iter().any(|f| f == path) {
            return Some("Logic");
        }
        if self.scenarios.iter().any(|f| f == path) {
            return Some("Scenarios");
        }
        self.assets.iter().find(|g| g.files.iter().any(|f| f == path)).map(|g| g.name)
    }
}

/// The folders at the top of a game folder that are never listed, however
/// the `.gitignore` reads: the recorded runs, what BUILD made, test renders.
pub const NEVER_LISTED: [&str; 3] = ["replays", "builds", "shots"];

/// Every file under `dir/prefix` that belongs to the game (`ignore`: what
/// the `.gitignore` files say), its path from `dir`.
fn walk(dir: &Path, prefix: &str, depth: usize, ignore: &Ignore, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir.join(prefix)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                if depth + 1 < DEPTH && !(prefix.is_empty() && NEVER_LISTED.contains(&name.as_str())) && !ignore.ignores(&path, true) {
                    walk(dir, &path, depth + 1, &ignore.entered(&path), out);
                }
            }
            Ok(_) if !ignore.ignores(&path, false) => out.push(path),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scripts and what they read are logic, the scenarios apart, the
    /// rest are assets by their sort; hidden files and replays are not
    /// listed.
    #[test]
    fn a_game_folder_splits_into_logic_and_assets() {
        let dir = std::env::temp_dir().join(format!("voxcade-ide-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in [
            ("main.lua", "-- see levels/README.txt\nlocal levels = read(\"levels/one.txt\")"),
            ("conf.lua", "function conf(c) end"),
            ("tools.lua", ""),
            ("scenarios/late.lua", "return {}"),
            ("levels/one.txt", "#"),
            ("levels/README.txt", "credits"),
            ("tiled/level.json", "{}"),
            ("hero.vvx", ""),
            ("blocks/stone.vvx", ""),
            ("level.toml", "format = \"voxcade level 1\""),
            ("prefabs/hut.vvx", ""),
            ("back.png", ""),
            ("sounds/coin.toml", "wave = \"sine\""),
            ("sounds/boom.wav", ""),
            ("sounds/raw/deep.wav", ""),
            ("music/song.mml", ""),
            ("replays/tests/run.toml", ""),
            (".luarc.json", "{}"),
            ("blocks/.stone.vvx.tmp", ""),
        ] {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let files = Files::scan(&dir);
        assert_eq!(files.logic, ["conf.lua", "main.lua", "levels/one.txt", "tools.lua"]);
        assert_eq!(files.scenarios, ["scenarios/late.lua"]);
        let groups: Vec<(&str, Vec<&str>)> = files.assets.iter().map(|g| (g.name, g.files.iter().map(String::as_str).collect())).collect();
        assert_eq!(
            groups,
            [
                ("Levels", vec!["level.toml", "prefabs/hut.vvx"]),
                ("Models", vec!["hero.vvx"]),
                ("Blocks", vec!["blocks/stone.vvx"]),
                ("Images", vec!["back.png"]),
                ("Sounds", vec!["sounds/coin.toml"]),
                ("Music", vec!["music/song.mml"]),
                ("Other", vec!["levels/README.txt", "sounds/boom.wav", "sounds/raw/deep.wav", "tiled/level.json"]),
            ]
        );
        assert!(files.has("blocks/stone.vvx") && !files.has("replays/tests/run.toml"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Test renders in `shots/` are never listed, with a `.gitignore` or
    /// without one; what a `.gitignore` of the game (or of a subfolder)
    /// ignores is not listed either, and a `!` brings a file back.
    #[test]
    fn the_list_leaves_out_what_is_ignored() {
        let dir = std::env::temp_dir().join(format!("voxcade-ide-ignored-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in [
            ("main.lua", ""),
            ("shots/a.png", ""),
            ("shots/film.mp4", ""),
            ("hero.vvx", ""),
            ("notes/ideas.txt", ""),
            ("scratch.vvx", ""),
            ("kept.vvx", ""),
            ("sounds/old.toml", ""),
            ("sounds/new.toml", ""),
            ("sounds/.gitignore", "old.toml\n"),
        ] {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        // No `.gitignore`: only `shots/` is out.
        let names = Files::names(&dir);
        assert!(names.iter().all(|n| !n.starts_with("shots/")), "{names:?}");
        assert!(names.contains(&"notes/ideas.txt".to_string()) && names.contains(&"scratch.vvx".to_string()));
        std::fs::write(dir.join(".gitignore"), "notes/\n*.vvx\n!hero.vvx\n!kept.vvx\nscratch.vvx\n").unwrap();
        let names = Files::names(&dir);
        assert_eq!(names, ["hero.vvx", "kept.vvx", "main.lua", "sounds/new.toml"].map(String::from), "ignored files, folders and the subfolder's own file are out");
        // A file ignored but open stays open: the list only does not name it.
        assert!(!Files::scan(&dir).has("notes/ideas.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

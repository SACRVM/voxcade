//! A game folder as the IDE lists it: its logic (the Lua scripts, and the
//! text files they `read`), its scenarios (`scenarios/NAME.lua`: runs
//! begun late, to try things) and its assets in a few groups (the level,
//! models, blocks, images, music, the rest; the models and the music
//! listed even when empty). Hidden files and the recorded
//! runs (`replays/`) are left out.

use std::path::Path;

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
const GROUPS: [&str; 6] = ["Levels", "Models", "Blocks", "Images", "Music", "Other"];

/// The sorts listed even with no file.
const ALWAYS: [&str; 2] = ["Models", "Music"];

/// How deep the IDE looks into the folder's folders.
const DEPTH: usize = 5;

impl Files {
    /// The game folder `dir` as the IDE lists it.
    pub fn scan(dir: &Path) -> Self {
        let mut all = Vec::new();
        walk(dir, "", 0, &mut all);
        all.sort();
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
                _ if f == "level.json" || f == "map.json" || (f.starts_with("levels/") && ext == "json") => 0,
                "vox" if f.starts_with("blocks/") => 2,
                "vox" => 1,
                "png" => 3,
                "mml" => 4,
                _ => 5,
            };
            groups[group].push(f);
        }
        // The game's own two first, the other scripts after them.
        files.logic.sort_by_key(|f| (["conf.lua", "main.lua"].iter().position(|n| n == f).unwrap_or(2), f.clone()));
        // The models and the music always have their place (the IDE's plus
        // there makes the first one); the other sorts show when there are some.
        files.assets = GROUPS.iter().zip(groups).filter(|(name, g)| !g.is_empty() || ALWAYS.contains(name)).map(|(&name, files)| Group { name, files }).collect();
        files
    }

    /// Whether the folder has this file.
    pub fn has(&self, path: &str) -> bool {
        self.logic.iter().chain(&self.scenarios).chain(self.assets.iter().flat_map(|g| &g.files)).any(|f| f == path)
    }
}

/// Every file under `dir/prefix`, its path from `dir`.
fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir.join(prefix)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                if depth + 1 < DEPTH && !(prefix.is_empty() && name == "replays") {
                    walk(dir, &path, depth + 1, out);
                }
            }
            Ok(_) => out.push(path),
            Err(_) => {}
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
            ("hero.vox", ""),
            ("blocks/stone.vox", ""),
            ("level.json", "{}"),
            ("back.png", ""),
            ("music/song.mml", ""),
            ("replays/tests/run.toml", ""),
            (".luarc.json", "{}"),
            ("blocks/.stone.vox.tmp", ""),
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
                ("Levels", vec!["level.json"]),
                ("Models", vec!["hero.vox"]),
                ("Blocks", vec!["blocks/stone.vox"]),
                ("Images", vec!["back.png"]),
                ("Music", vec!["music/song.mml"]),
                ("Other", vec!["levels/README.txt", "tiled/level.json"]),
            ]
        );
        assert!(files.has("blocks/stone.vox") && !files.has("replays/tests/run.toml"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

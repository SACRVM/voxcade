//! BUILD: the game as one zip file for this system, to pass on: the
//! program (this one), the game folder beside it under `games/`, a
//! launcher to double-click and a short README, all in a folder named for
//! the game. Left out: hidden files (the editors' and agents' settings),
//! the scripted regression runs (`replays/tests/`), earlier builds
//! (`builds/`), the test renders (`shots/`) and what the game's
//! `.gitignore` files ignore (`crate::gitignore`); the replays the title
//! plays stay. A file left out that the game names is said (`Packed::missing`). Unsigned (macOS asks once, as the README says).

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::gitignore::Ignore;

/// The system the program is built for, as the zip's name says it.
fn system() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Where the build of the game in `dir` goes: `builds/<game>-<system>.zip`
/// in the game's own folder (a new game's `.gitignore` leaves it out; the
/// list and the next build leave it out too).
pub fn out_path(dir: &Path) -> PathBuf {
    dir.join("builds").join(format!("{}-{}.zip", crate::settings::game_name(dir), system()))
}

/// Opens the folder `zip` lies in in the system's file manager.
pub fn show_in_folder(zip: &Path) {
    if let Some(folder) = zip.parent() {
        crate::update::show(&folder.to_string_lossy());
    }
}

/// What a build made.
#[derive(Debug)]
pub struct Packed {
    pub bytes: u64,
    /// Files left out because the `.gitignore` ignores them although the
    /// game names them (their paths): it will not find them.
    pub missing: Vec<String>,
}

/// Packs the game in `dir` (called `title`) with the program at `program`
/// into the zip at `out`; how many bytes it holds, and the ignored files
/// the game needs.
pub fn export(dir: &Path, title: &str, program: &Path, out: &Path) -> Result<Packed> {
    let name = crate::settings::game_name(dir);
    let top = title.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect::<String>();
    if let Some(folder) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
    }
    // Written beside the zip first and renamed over it: no half a zip.
    let temp = out.with_extension("zip.part");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&temp).with_context(|| format!("writing {}", temp.display()))?);
    let plain = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o644);
    let runs = plain.unix_permissions(0o755);
    let mut add = |path: String, bytes: &[u8], options| -> Result<()> {
        zip.start_file(format!("{top}/{path}"), options)?;
        zip.write_all(bytes)?;
        Ok(())
    };
    // The program.
    let exe = if cfg!(windows) { "voxcade.exe" } else { "voxcade" };
    add(exe.into(), &std::fs::read(program).with_context(|| format!("reading {}", program.display()))?, runs)?;
    // The game.
    let (mut files, mut ignored) = (Vec::new(), Vec::new());
    walk(dir, "", &Ignore::for_game(dir), &mut files, &mut ignored);
    files.sort();
    for file in &files {
        add(format!("games/{name}/{file}"), &std::fs::read(dir.join(file))?, plain)?;
    }
    ignored.sort();
    let missing = needed(dir, &files, &ignored);
    // The launcher and the README.
    let (launcher, script, options) = if cfg!(windows) {
        (format!("play-{name}.bat"), format!("@echo off\r\ncd /d \"%~dp0\"\r\nvoxcade.exe run games\\{name}\r\nif errorlevel 1 pause\r\n"), plain)
    } else {
        let ending = if cfg!(target_os = "macos") { "command" } else { "sh" };
        (format!("play-{name}.{ending}"), format!("#!/bin/sh\ncd \"$(dirname \"$0\")\" || exit 1\n./voxcade run games/{name}\n"), runs)
    };
    add(launcher.clone(), script.as_bytes(), options)?;
    add("README.txt".into(), readme(title, &name, &launcher).as_bytes(), plain)?;
    // The licences of what is built into the program travel with it (a
    // package has them beside the program; a build from source has not).
    // Voxcade's own licence too, under a name that leaves LICENSE to the game.
    for (beside, packed) in [("THIRD-PARTY-LICENSES.txt", "THIRD-PARTY-LICENSES.txt"), ("LICENSE.txt", "VOXCADE-LICENSE.txt")] {
        if let Some(file) = program.parent().map(|d| d.join(beside)).filter(|f| f.is_file()) {
            add(packed.into(), &std::fs::read(file)?, plain)?;
        }
    }
    zip.finish()?;
    std::fs::rename(&temp, out).with_context(|| format!("writing {}", out.display()))?;
    Ok(Packed { bytes: std::fs::metadata(out)?.len(), missing })
}

/// Which of the `ignored` files the game's text names (in quotes): its
/// path, or for a sound or a song its name. Cheap, not sure: a file named
/// by a path built in code is not found.
fn needed(dir: &Path, packed: &[String], ignored: &[String]) -> Vec<String> {
    if ignored.is_empty() {
        return Vec::new();
    }
    let text: String = packed.iter().filter(|f| [".lua", ".toml", ".mml", ".vvx"].iter().any(|e| f.ends_with(e))).filter_map(|f| std::fs::read_to_string(dir.join(f)).ok()).collect::<Vec<_>>().join("\n");
    let named = |word: &str| text.contains(&format!("\"{word}\"")) || text.contains(&format!("'{word}'"));
    ignored
        .iter()
        .filter(|f| {
            let stem = f.rsplit('/').next().and_then(|n| n.rsplit_once('.')).map(|(s, _)| s);
            let by_name = (f.starts_with("sounds/") || f.starts_with("music/")) && stem.is_some_and(named);
            by_name || named(f)
        })
        .cloned()
        .collect()
}

/// The README beside the program.
fn readme(title: &str, name: &str, launcher: &str) -> String {
    format!(
        "{title}\n{}\n\nPlay: double-click {launcher}, or in a terminal in this folder:\n  voxcade run games/{name}\n\n\
         In the game Escape pauses; SETTINGS has the keys, the sound and the shadows\n(lower for weaker graphics). \
         Settings and high scores are kept in the user's\nconfig folder.\n\n\
         macOS: the build is not signed. If macOS refuses to open it, run this once\nin Terminal, in this folder:  xattr -dr com.apple.quarantine .\n\
         Linux: needs a desktop with Vulkan drivers.\n\nMade with Voxcade.\n",
        "=".repeat(title.chars().count())
    )
}

/// The files under `dir/prefix` a build takes, their paths from `dir`:
/// not hidden, not the regression runs, not earlier builds or test renders,
/// not an agent's notes, not what the `.gitignore` files ignore (those go
/// to `ignored`, but for the folders that are never part of a game).
fn walk(dir: &Path, prefix: &str, ignore: &Ignore, out: &mut Vec<String>, ignored: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir.join(prefix)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if name.starts_with('.') || path == "replays/tests" || (prefix.is_empty() && (matches!(name.as_str(), "builds" | "shots") || super::is_notes_file(&name))) {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        if ignore.ignores(&path, kind.is_dir()) {
            // What the game names but the build leaves out is said.
            if kind.is_dir() {
                list_all(dir, &path, ignored);
            } else {
                ignored.push(path);
            }
            continue;
        }
        if kind.is_dir() {
            walk(dir, &path, &ignore.entered(&path), out, ignored);
        } else {
            out.push(path);
        }
    }
}

/// Every file under `dir/prefix`, hidden ones left out (an ignored folder's).
fn list_all(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir.join(prefix)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}/{name}");
        match entry.file_type() {
            _ if name.starts_with('.') => {}
            Ok(t) if t.is_dir() => list_all(dir, &path, out),
            Ok(_) => out.push(path),
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The build goes into the game's own folder, and a build does not pack
    /// the builds before it.
    #[test]
    fn a_build_lands_in_the_games_folder_and_is_not_packed_again() {
        let dir = crate::scratch::Scratch::new("export-where");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("builds")).unwrap();
        std::fs::write(dir.join("main.lua"), "x").unwrap();
        std::fs::write(dir.join("builds/old.zip"), "x").unwrap();
        assert_eq!(out_path(&dir).parent(), Some(dir.join("builds").as_path()));
        let mut files = Vec::new();
        walk(&dir, "", &Ignore::for_game(&dir), &mut files, &mut Vec::new());
        assert_eq!(files, ["main.lua"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A build leaves out `shots/` (a `.gitignore` or none) and what the
    /// game's `.gitignore` ignores, as the list does, and says which of the
    /// ignored files the game names in quotes.
    #[test]
    fn a_build_leaves_out_what_is_ignored_and_says_what_the_game_needs() {
        let dir = crate::scratch::Scratch::new("export-ignored");
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in [
            ("main.lua", "local m = \"models/hero.vvx\"\nsound(\"coin\")"),
            ("shots/a.png", "x"),
            ("notes/ideas.txt", "x"),
            ("models/hero.vvx", "x"),
            ("models/spare.vvx", "x"),
            ("sounds/coin.toml", "x"),
            ("kept.txt", "x"),
        ] {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let (mut files, mut ignored) = (Vec::new(), Vec::new());
        walk(&dir, "", &Ignore::for_game(&dir), &mut files, &mut ignored);
        files.sort();
        assert_eq!(files, ["kept.txt", "main.lua", "models/hero.vvx", "models/spare.vvx", "notes/ideas.txt", "sounds/coin.toml"], "shots/ out without a .gitignore");
        std::fs::write(dir.join(".gitignore"), "notes/\nmodels/\nsounds/*.toml\n").unwrap();
        (files, ignored) = (Vec::new(), Vec::new());
        walk(&dir, "", &Ignore::for_game(&dir), &mut files, &mut ignored);
        files.sort();
        assert_eq!(files, ["kept.txt", "main.lua"]);
        ignored.sort();
        assert_eq!(needed(&dir, &files, &ignored), ["models/hero.vvx", "sounds/coin.toml"], "named by the game; the spare model and the notes are not");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The agent's notes at the top of a game folder are not packed (a file
    /// by that name deeper down is the game's).
    #[test]
    fn a_build_leaves_the_agents_notes_out() {
        let dir = crate::scratch::Scratch::new("export-notes");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        for file in ["main.lua", "CLAUDE.md", "levels/CLAUDE.md", ".gitignore"] {
            std::fs::write(dir.join(file), "x").unwrap();
        }
        let mut files = Vec::new();
        walk(&dir, "", &Ignore::for_game(&dir), &mut files, &mut Vec::new());
        files.sort();
        assert_eq!(files, ["levels/CLAUDE.md", "main.lua"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A build holds the program, the game (no hidden files, no regression
    /// runs), its launcher and README; unpacked, the game loads as the
    /// original does: the same data to the last voxel and token (the
    /// replays' fingerprint), and it plays.
    #[test]
    fn an_export_holds_the_game_and_runs() {
        let dir = Path::new("games/voxkeep");
        let tmp = crate::scratch::Scratch::new("export");
        let _ = std::fs::remove_dir_all(&tmp);
        // A stand-in for the program: the test's own.
        let program = std::env::current_exe().unwrap();
        let zip = tmp.join("voxkeep.zip");
        let size = export(dir, "Voxkeep", &program, &zip).unwrap().bytes;
        assert!(size > 1000 && !zip.with_extension("zip.part").exists());
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
        let names: Vec<String> = archive.file_names().map(|n| n.unwrap().into_owned()).collect();
        for want in ["Voxkeep/README.txt", "Voxkeep/games/voxkeep/main.lua", "Voxkeep/games/voxkeep/blocks/stone.vvx"] {
            assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
        }
        assert!(names.iter().any(|n| n.starts_with("Voxkeep/play-voxkeep.")), "a launcher");
        assert!(!names.iter().any(|n| n.contains("/.") || n.contains("replays/tests")), "no hidden files, no regression runs");
        archive.extract(&tmp).unwrap();
        let copy = tmp.join("Voxkeep/games/voxkeep");
        let (mut a, b) = (crate::session::Session::<crate::Voxcade>::load(&copy).unwrap(), crate::session::Session::<crate::Voxcade>::load(dir).unwrap());
        assert_eq!(a.fingerprint().unwrap(), b.fingerprint().unwrap(), "the same game data");
        for _ in 0..60 {
            a.step(&crate::input::Input::default());
        }
        assert!(a.trouble().is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

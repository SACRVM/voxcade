//! BUILD: the game as one zip file for this system, to pass on: the
//! program (this one), the game folder beside it under `games/`, a
//! launcher to double-click and a short README, all in a folder named for
//! the game. Left out: hidden files (the editors' and agents' settings)
//! and the scripted regression runs (`replays/tests/`); the replays the
//! title plays stay. Unsigned (macOS asks once, as the README says).

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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
/// in the folder the IDE runs in.
pub fn out_path(dir: &Path) -> PathBuf {
    PathBuf::from("builds").join(format!("{}-{}.zip", crate::settings::game_name(dir), system()))
}

/// Packs the game in `dir` (called `title`) with the program at `program`
/// into the zip at `out`; how many bytes it holds.
pub fn export(dir: &Path, title: &str, program: &Path, out: &Path) -> Result<u64> {
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
    let mut files = Vec::new();
    walk(dir, "", &mut files);
    files.sort();
    for file in &files {
        add(format!("games/{name}/{file}"), &std::fs::read(dir.join(file))?, plain)?;
    }
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
    Ok(std::fs::metadata(out)?.len())
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
/// not hidden, not the regression runs.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir.join(prefix)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if name.starts_with('.') || path == "replays/tests" {
            continue;
        }
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk(dir, &path, out),
            Ok(_) => out.push(path),
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A build holds the program, the game (no hidden files, no regression
    /// runs), its launcher and README; unpacked, the game loads as the
    /// original does: the same data to the last voxel and token (the
    /// replays' fingerprint), and it plays.
    #[test]
    fn an_export_holds_the_game_and_runs() {
        let dir = Path::new("games/voxkeep");
        let tmp = std::env::temp_dir().join(format!("voxcade-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        // A stand-in for the program: the test's own.
        let program = std::env::current_exe().unwrap();
        let zip = tmp.join("voxkeep.zip");
        let size = export(dir, "Voxkeep", &program, &zip).unwrap();
        assert!(size > 1000 && !zip.with_extension("zip.part").exists());
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
        let names: Vec<String> = archive.file_names().map(|n| n.unwrap().into_owned()).collect();
        for want in ["Voxkeep/README.txt", "Voxkeep/games/voxkeep/main.lua", "Voxkeep/games/voxkeep/blocks/stone.vox"] {
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

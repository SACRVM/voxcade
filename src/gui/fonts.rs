//! The IDE's letters: one monospace typeface throughout (egui's Hack),
//! and for the signs it lacks (Claude Code's ⏺ and ⎿, ✓, CJK) the fonts
//! the system has, where it has them, then egui's own emoji. Nothing is
//! bundled for them: a sign no font has shows as a box.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// The fallback fonts by file name, the first found of each, in the order
/// they are asked: a monospace one first, then symbols, mathematics, CJK.
const LINUX: [&str; 4] = ["DejaVuSansMono.ttf", "NotoSansSymbols2-Regular.ttf", "NotoSansMath-Regular.ttf", "NotoSansCJK-Regular.ttc"];
const WINDOWS: [&str; 3] = ["consola.ttf", "seguisym.ttf", "msgothic.ttc"];
const MACOS: [&str; 3] = ["/System/Library/Fonts/Menlo.ttc", "/System/Library/Fonts/Apple Symbols.ttf", "/System/Library/Fonts/Hiragino Sans GB.ttc"];

/// Hack for every text, the system's fonts behind it.
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut mono = fonts.families[&FontFamily::Monospace].clone();
    for (i, path) in fallbacks().into_iter().enumerate() {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let name = format!("system {i}");
        fonts.font_data.insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
        // Behind Hack, before egui's proportional and emoji fonts.
        mono.insert(1 + i.min(mono.len() - 1), name);
    }
    fonts.families.insert(FontFamily::Monospace, mono.clone());
    fonts.families.insert(FontFamily::Proportional, mono);
    fonts
}

/// The fallback fonts this system has.
fn fallbacks() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        MACOS.iter().map(PathBuf::from).filter(|p| p.is_file()).collect()
    } else if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join("Fonts");
        WINDOWS.iter().map(|f| dir.join(f)).filter(|p| p.is_file()).collect()
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let dirs = [PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts"), home.join(".local/share/fonts"), home.join(".fonts")];
        LINUX.iter().filter_map(|name| dirs.iter().find_map(|d| find(d, name, 3))).collect()
    }
}

/// The file called `name` under `dir`, at most `depth` folders down.
fn find(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    if let Some(e) = entries.iter().find(|e| e.file_name() == name) {
        return Some(e.path());
    }
    if depth == 0 {
        return None;
    }
    entries.iter().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).find_map(|e| find(&e.path(), name, depth - 1))
}

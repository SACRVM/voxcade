use super::*;

/// The game's worlds and the prefabs its maps name (as JSON or as
/// text), as paths from the game folder, whatever their ending.
pub fn of(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut prefabs = Vec::new();
    for json in json_files(dir, legacy::Sort::Map) {
        let folder = json.parent().unwrap_or(Path::new("")).to_path_buf();
        if let Ok(m) = std::fs::read_to_string(dir.join(&json)).map_err(anyhow::Error::from).and_then(|t| legacy::map(&t)) {
            prefabs.extend(m.prefabs.values().map(|p| clean(&folder.join(&p.file))));
        }
    }
    for toml in toml_maps(dir) {
        let folder = toml.parent().unwrap_or(Path::new("")).to_path_buf();
        if let Ok(m) = read_map(dir, &toml) {
            prefabs.extend(m.prefabs.values().map(|p| clean(&folder.join(&p.file))));
        }
    }
    prefabs.sort();
    prefabs.dedup();
    let mut worlds: Vec<PathBuf> = ["vox", "vvx"].iter().flat_map(|e| files_ending(&dir.join("levels"), e)).filter(|p| p.parent().is_none_or(|f| f.as_os_str().is_empty())).map(|p| Path::new("levels").join(p)).collect();
    worlds.extend(world_lines(dir).into_iter().filter(|q| q.ends_with(".vox") || q.ends_with(".vvx")).map(PathBuf::from));
    // The default world, when conf.lua names none.
    if !world_lines(dir).iter().any(|q| q.contains('.')) {
        worlds.extend(["world.vox", "world.vvx"].map(PathBuf::from).into_iter().filter(|p| dir.join(p).is_file()));
    }
    worlds.sort();
    worlds.dedup();
    worlds.retain(|w| !prefabs.contains(w));
    (worlds, prefabs)
}

/// A path from the game folder without `.` and `x/..` in it.
fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

use super::*;

/// The game's `.vox` files: the models to bring over, and the worlds
/// and prefabs that stay.
fn sorted(dir: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let all = files_ending(dir, "vox");
    let scenes = scenes(dir, &all)?;
    Ok(all.into_iter().partition(|p| !scenes.contains(p)))
}

/// The `.vox` files that are scenes: a world or a prefab a map names
/// (`scenes::of`).
fn scenes(dir: &Path, all: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let (worlds, prefabs) = super::scenes::of(dir);
    Ok(all.iter().filter(|p| worlds.contains(p) || prefabs.contains(p)).cloned().collect())
}

/// Each Lua line naming a model brought over: the file, its number, the
/// line as it is and as it becomes. A quoted path of a model, and the
/// ending alone (`kind .. ".vox"`, a model's name made in code).
fn lua_lines(dir: &Path, models: &[PathBuf]) -> Vec<(PathBuf, usize, String, String)> {
    // The models brought over now, and those an upgrade cut short
    // brought over before (a .vvx whose .vox is gone).
    let mut names: Vec<String> = models.iter().map(|p| named(p)).collect();
    for vvx in files_ending(dir, "vvx") {
        let vox = vvx.with_extension("vox");
        if !dir.join(&vox).exists() && !names.contains(&named(&vox)) {
            names.push(named(&vox));
        }
    }
    let mut out = Vec::new();
    for lua in files_ending(dir, "lua") {
        let text = std::fs::read_to_string(dir.join(&lua)).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            let mut new = line.to_string();
            for q in ['"', '\''] {
                for name in &names {
                    new = new.replace(&format!("{q}{name}{q}"), &format!("{q}{}{q}", name.strip_suffix(".vox").map_or(name.clone(), |s| format!("{s}.vvx"))));
                }
                if !names.is_empty() {
                    new = new.replace(&format!("{q}.vox{q}"), &format!("{q}.vvx{q}"));
                }
            }
            if new != line {
                out.push((lua.clone(), i + 1, line.to_string(), new));
            }
        }
    }
    out
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let (models, scenes) = sorted(dir)?;
    let mut out = Vec::new();
    for p in &models {
        let model = crate::vox::import_model(&dir.join(p)).with_context(|| format!("{}", dir.join(p).display()))?;
        let what = if model.parts.is_empty() { format!("{} frame{}", model.frames.len(), if model.frames.len() == 1 { "" } else { "s" }) } else { format!("a figure of {} parts", model.parts.len()) };
        out.push(format!("{} becomes {} ({what})", named(p), named(&p.with_extension("vvx"))));
    }
    for (file, n, _, new) in lua_lines(dir, &models) {
        out.push(format!("{}:{n} becomes: {}", named(&file), new.trim()));
    }
    for p in &scenes {
        out.push(format!("{} is a world or a map's prefab: \"worlds and prefabs as text\" brings it over", named(p)));
    }
    Ok(out)
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let (models, scenes) = sorted(dir)?;
    let mut out = Vec::new();
    for p in &models {
        out.push(bring_in(&dir.join(p), &dir.join(p).with_extension("vvx"), true)?.replace(&format!("{}/", dir.display()), ""));
    }
    let mut changed: Vec<(PathBuf, String)> = Vec::new();
    for (file, n, _, new) in lua_lines(dir, &models) {
        let path = dir.join(&file);
        let text = match changed.iter().position(|(f, _)| *f == path) {
            Some(k) => changed.remove(k).1,
            None => std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
        };
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines[n - 1] = new.clone();
        let mut joined = lines.join("\n");
        if text.ends_with('\n') {
            joined.push('\n');
        }
        changed.push((path, joined));
        out.push(format!("{}:{n} now: {}", named(&file), new.trim()));
    }
    for (path, text) in changed {
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    for p in &scenes {
        out.push(format!("{} is a world or a map's prefab: \"worlds and prefabs as text\" brings it over", named(p)));
    }
    Ok(out)
}

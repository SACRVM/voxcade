use super::*;
use crate::grid::{GridLevel, Legend, View};

pub fn left(dir: &Path) -> bool {
    !json_files(dir, legacy::Sort::Level).is_empty()
}

fn renames(files: &[PathBuf]) -> Vec<(String, String)> {
    files.iter().map(|p| (named(p), named(&with_ending(p, "toml")))).collect()
}

fn read(dir: &Path, p: &Path) -> Result<GridLevel> {
    let text = std::fs::read_to_string(dir.join(p)).with_context(|| format!("reading {}", dir.join(p).display()))?;
    legacy::level(&text).with_context(|| named(p))
}

fn what(level: &GridLevel) -> String {
    let s = level.size;
    let view = match View::for_size(s) {
        View::Front => "its layers seen from the front",
        View::Above => "its layers seen from above",
    };
    format!("{} by {} by {} cells, {} thing{}, {view}", s.x, s.y, s.z, level.things.len(), if level.things.len() == 1 { "" } else { "s" })
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let files = json_files(dir, legacy::Sort::Level);
    let mut out = Vec::new();
    for p in &files {
        let level = read(dir, p)?;
        out.push(format!("{} becomes {} ({})", named(p), named(&with_ending(p, "toml")), what(&level)));
    }
    for (file, n, _, new) in lua_renames(dir, &renames(&files)) {
        out.push(format!("{}:{n} becomes: {}", named(&file), new.trim()));
    }
    Ok(out)
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let files = json_files(dir, legacy::Sort::Level);
    let mut out = Vec::new();
    for p in &files {
        let level = read(dir, p)?;
        let to = dir.join(with_ending(p, "toml"));
        if to.exists() {
            bail!("{} is there already: the level is {} too; take one of them away first", to.display(), named(p));
        }
        crate::blocks::save_at_once(&to, crate::grid::text::write(&level, &Legend::default()).as_bytes())?;
        let back = GridLevel::read(&to)?;
        if back != level {
            std::fs::remove_file(&to).ok();
            bail!("{} did not read back as {} was; the JSON is kept", to.display(), named(p));
        }
        std::fs::remove_file(dir.join(p)).with_context(|| format!("removing {}", dir.join(p).display()))?;
        out.push(format!("{} became {} ({}), read back the same", named(p), named(&with_ending(p, "toml")), what(&level)));
    }
    let edits: Vec<_> = lua_renames(dir, &renames(&files)).into_iter().map(|(f, n, old, new)| (f, n, old, Some(new))).collect();
    out.extend(write_lua(dir, &edits)?);
    Ok(out)
}

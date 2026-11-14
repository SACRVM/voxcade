use super::*;
use crate::map::MapData;

pub fn left(dir: &Path) -> bool {
    !json_files(dir, legacy::Sort::Map).is_empty()
}

fn read(dir: &Path, p: &Path) -> Result<MapData> {
    let text = std::fs::read_to_string(dir.join(p)).with_context(|| format!("reading {}", dir.join(p).display()))?;
    legacy::map(&text).with_context(|| named(p))
}

fn renames(files: &[PathBuf]) -> Vec<(String, String)> {
    files.iter().map(|p| (named(p), named(&with_ending(p, "toml")))).collect()
}

fn what(m: &MapData) -> String {
    let [x, y, z] = m.size;
    format!("{x} by {y} by {z} voxels, {} prefabs, {} put on it", m.prefabs.len(), m.put.len())
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let files = json_files(dir, legacy::Sort::Map);
    let mut out = Vec::new();
    for p in &files {
        let m = read(dir, p)?;
        out.push(format!("{} becomes {} ({}; its height and material pictures stay PNG)", named(p), named(&with_ending(p, "toml")), what(&m)));
    }
    for (file, n, _, new) in lua_renames(dir, &renames(&files)) {
        out.push(format!("{}:{n} becomes: {}", named(&file), new.trim()));
    }
    Ok(out)
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let files = json_files(dir, legacy::Sort::Map);
    let mut out = Vec::new();
    for p in &files {
        let m = read(dir, p)?;
        let to = dir.join(with_ending(p, "toml"));
        if to.exists() {
            bail!("{} is there already: the map is {} too; take one of them away first", to.display(), named(p));
        }
        crate::blocks::save_at_once(&to, m.write().as_bytes())?;
        let back = MapData::read(&std::fs::read_to_string(&to)?).with_context(|| to.display().to_string())?;
        if back != m {
            std::fs::remove_file(&to).ok();
            bail!("{} did not read back as {} was; the JSON is kept", to.display(), named(p));
        }
        std::fs::remove_file(dir.join(p)).with_context(|| format!("removing {}", dir.join(p).display()))?;
        out.push(format!("{} became {} ({}), read back the same", named(p), named(&with_ending(p, "toml")), what(&m)));
    }
    let edits: Vec<_> = lua_renames(dir, &renames(&files)).into_iter().map(|(f, n, old, new)| (f, n, old, Some(new))).collect();
    out.extend(write_lua(dir, &edits)?);
    Ok(out)
}

use std::collections::BTreeMap;

use glam::IVec3;

use super::*;
use crate::scene::Scene;

/// The `.vox` worlds and prefabs of the game.
fn files(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let (worlds, prefabs) = scenes::of(dir);
    let vox = |p: &PathBuf| p.extension().is_some_and(|e| e == "vox") && dir.join(p).is_file();
    (worlds.into_iter().filter(vox).collect(), prefabs.into_iter().filter(vox).collect())
}

/// The `c.markers` lines of `conf.lua`: its number and text.
fn marker_lines(dir: &Path) -> Vec<(usize, String)> {
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    conf.lines().enumerate().filter(|(_, l)| l.trim_start().starts_with("c.markers")).map(|(i, l)| (i + 1, l.to_string())).collect()
}

/// The lines of `conf.lua` that go with `c.markers`: its own, the
/// comment that heads a block of them, and a blank line left double.
fn marker_removals(dir: &Path) -> Vec<(usize, String)> {
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    let lines: Vec<&str> = conf.lines().collect();
    let marked: Vec<usize> = marker_lines(dir).into_iter().map(|(n, _)| n - 1).collect();
    let mut gone = vec![false; lines.len()];
    for &i in &marked {
        gone[i] = true;
    }
    let blank = |i: usize| lines.get(i).is_some_and(|l| l.trim().is_empty());
    let mut i = 0;
    while i < lines.len() {
        if !marked.contains(&i) || (i > 0 && marked.contains(&(i - 1))) {
            i += 1;
            continue;
        }
        // A run of them: the comment lines straight above it head it.
        let mut top = i;
        while top > 0 && lines[top - 1].trim_start().starts_with("--") {
            top -= 1;
            gone[top] = true;
        }
        let mut end = i;
        while marked.contains(&(end + 1)) {
            end += 1;
        }
        if top > 0 && blank(top - 1) && blank(end + 1) {
            gone[end + 1] = true;
        }
        i = end + 1;
    }
    lines.iter().enumerate().filter(|(i, _)| gone[*i]).map(|(i, l)| (i + 1, l.to_string())).collect()
}

pub fn left(dir: &Path) -> bool {
    let (w, p) = files(dir);
    !w.is_empty() || !p.is_empty() || !marker_lines(dir).is_empty()
}

/// The markers `conf.lua` sets: each palette index's kind. From the
/// settings as the game reads them; where `conf.lua` does not run,
/// from its `c.markers.NAME = N` lines.
fn markers(dir: &Path) -> BTreeMap<u8, String> {
    let mut cfg = crate::config::GameConfig::default();
    if crate::lua::conf::run(&crate::files::Files::folder(dir), &mut cfg).is_ok() {
        return cfg.markers.by_index();
    }
    let mut out = BTreeMap::new();
    for (_, line) in marker_lines(dir) {
        let code = line.split("--").next().unwrap_or_default();
        if let Some((name, number)) = code.trim().strip_prefix("c.markers.").and_then(|r| r.split_once('='))
            && let Ok(n) = number.trim().parse::<u8>()
        {
            out.insert(n, name.trim().to_string());
        }
    }
    out
}

/// The voxels of a `.vox` scene as the engine read them: the cells'
/// last word (a later voxel covers an earlier one), the markers in the
/// order of the file's voxels, the colour indices in the order of their
/// first use.
struct Read {
    cells: Vec<u8>,
    markers: Vec<(IVec3, String)>,
    first_use: Vec<u8>,
}

fn read_vox(old: &crate::vox::Scene, markers: &BTreeMap<u8, String>) -> Result<Read> {
    let s = old.size;
    let index = |p: IVec3| (p.x + s.x * (p.y + s.y * p.z)) as usize;
    let mut cells = vec![0u8; (s.x * s.y * s.z) as usize];
    let mut marked = vec![false; cells.len()];
    let (mut found, mut first_use) = (Vec::new(), Vec::new());
    for &(p, i) in &old.voxels {
        let at = index(p);
        match markers.get(&i) {
            Some(kind) => {
                if marked[at] || cells[at] != 0 {
                    bail!("a marker ({kind}) shares cell {} {} {} with another voxel: a text holds one a cell", p.x, p.y, p.z);
                }
                marked[at] = true;
                found.push((p, kind.clone()));
            }
            None => {
                if marked[at] {
                    bail!("a voxel shares cell {} {} {} with a marker: a text holds one a cell", p.x, p.y, p.z);
                }
                cells[at] = i;
                if !first_use.contains(&i) {
                    first_use.push(i);
                }
            }
        }
    }
    Ok(Read { cells, markers: found, first_use })
}

/// The markers as palette entries: the kinds in the order the things
/// came, a new entry where the reading order of one entry's cells would
/// put them otherwise; each marker's entry.
fn marker_entries(read: &Read) -> (Vec<String>, Vec<usize>) {
    let key = |p: IVec3| (p.z, -p.y, p.x);
    let (mut kinds, mut of) = (Vec::new(), Vec::new());
    let mut last: Option<IVec3> = None;
    for (p, kind) in &read.markers {
        let same = kinds.last() == Some(kind) && last.is_some_and(|l| key(*p) > key(l));
        if !same {
            kinds.push(kind.clone());
        }
        of.push(kinds.len() - 1);
        last = Some(*p);
    }
    (kinds, of)
}

/// The scene a `.vox` is as text: a world with its file's colours in
/// their order (every voxel keeps its number), a prefab with its
/// colours in the order of their first use (the map numbers them so).
/// What it says of the palette, too.
pub fn convert(old: &crate::vox::Scene, markers: &BTreeMap<u8, String>, world: bool) -> Result<(Scene, String)> {
    if let Some(why) = crate::scene::too_big(old.size) {
        bail!("{why}");
    }
    let read = read_vox(old, markers)?;
    let mut scene = Scene::new(old.size);
    let numbers: Vec<u8> = if world {
        let highest = read.cells.iter().copied().max().unwrap_or(0);
        (1..=highest).collect()
    } else {
        read.first_use.clone()
    };
    scene.colours = numbers.iter().map(|&i| [old.palette[i as usize][0], old.palette[i as usize][1], old.palette[i as usize][2]]).collect();
    let mut number_of = [0u16; 256];
    for (n, &i) in numbers.iter().enumerate() {
        number_of[i as usize] = n as u16 + 1;
    }
    for (k, &i) in read.cells.iter().enumerate() {
        scene.cells[k] = number_of[i as usize];
    }
    let (kinds, of) = marker_entries(&read);
    let first_marker = scene.colours.len() as u16 + 1;
    for ((p, _), k) in read.markers.iter().zip(&of) {
        let at = scene.index(*p);
        scene.cells[at] = first_marker + *k as u16;
    }
    scene.markers = kinds;
    // As the engine reads the text: the same voxels, the same numbers,
    // the markers in the same order.
    let found: Vec<(IVec3, String)> = scene.found().into_iter().map(|(p, k)| (p, scene.markers[k].clone())).collect();
    if found != read.markers {
        bail!("the markers would not come in their order");
    }
    let used = numbers.len();
    let said = if world {
        let rest = (used + 1..256).filter(|&i| old.palette[i] != [0, 0, 0, 0]).count();
        if rest > 0 { format!("colors 1 to {used} kept in their order, the {rest} after them no voxel has left out") } else { format!("colors 1 to {used} kept in their order") }
    } else {
        format!("{used} colors in the order the map takes them")
    };
    Ok((scene, said))
}

fn kind_of(dir: &Path, p: &Path) -> &'static str {
    let (worlds, _) = files(dir);
    if worlds.iter().any(|w| w == p) { "world" } else { "prefab" }
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let (worlds, prefabs) = files(dir);
    let markers = markers(dir);
    let mut out = Vec::new();
    for p in worlds.iter().chain(&prefabs) {
        let world = kind_of(dir, p) == "world";
        let old = crate::vox::load_scene(&dir.join(p))?;
        match convert(&old, &markers, world) {
            Ok((scene, said)) => out.push(format!("{} becomes {} (a {}, {} by {} by {} voxels; {said}{})", named(p), named(&with_ending(p, "vvx")), kind_of(dir, p), old.size.x, old.size.y, old.size.z, marker_words(&scene))),
            Err(e) => out.push(format!("{} stays a .vox: {e:#}", named(p))),
        }
    }
    out.extend(map_names(dir, &prefabs)?.into_iter().map(|(m, from, to)| format!("{} names {to} for {from}", named(&m))));
    for m in json_files(dir, legacy::Sort::Map) {
        out.push(format!("{} (as text by then) names the prefabs' .vvx", named(&with_ending(&m, "toml"))));
    }
    let renamed: Vec<(String, String)> = worlds.iter().map(|p| (named(p), named(&with_ending(p, "vvx")))).collect();
    for (file, n, _, new) in lua_renames(dir, &renamed) {
        out.push(format!("{}:{n} becomes: {}", named(&file), new.trim()));
    }
    for (n, line) in marker_removals(dir) {
        let line = if line.trim().is_empty() { "a blank line left double" } else { line.trim() };
        out.push(format!("conf.lua:{n} goes (the markers are in the files now): {line}"));
    }
    Ok(out)
}

fn marker_words(scene: &Scene) -> String {
    match scene.markers.len() {
        0 => String::new(),
        n => format!("; {n} marker{}: {}", if n == 1 { "" } else { "s" }, scene.markers.join(", ")),
    }
}

/// The map files' prefab entries that name a `.vox` of `prefabs`: the
/// map, the name as it is and as it becomes.
fn map_names(dir: &Path, prefabs: &[PathBuf]) -> Result<Vec<(PathBuf, String, String)>> {
    let mut out = Vec::new();
    for m in toml_maps(dir) {
        let data = read_map(dir, &m)?;
        let folder = m.parent().unwrap_or(Path::new(""));
        for p in data.prefabs.values() {
            let file = folder.join(&p.file);
            if prefabs.iter().any(|q| named(q) == named(&file)) {
                out.push((m.clone(), p.file.clone(), named(&with_ending(Path::new(&p.file), "vvx"))));
            }
        }
    }
    Ok(out)
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let (worlds, prefabs) = files(dir);
    let markers = markers(dir);
    let mut out = Vec::new();
    let mut kept = Vec::new();
    for p in worlds.iter().chain(&prefabs) {
        let world = worlds.contains(p);
        let old = crate::vox::load_scene(&dir.join(p))?;
        let (scene, said) = match convert(&old, &markers, world) {
            Ok(c) => c,
            Err(e) => {
                out.push(format!("{} stays a .vox: {e:#}", named(p)));
                kept.push(p.clone());
                continue;
            }
        };
        let to = dir.join(with_ending(p, "vvx"));
        if to.exists() {
            bail!("{} is there already: the {} is {} too; take one of them away first", to.display(), if world { "world" } else { "prefab" }, named(p));
        }
        crate::blocks::save_at_once(&to, scene.write().as_bytes())?;
        let back = crate::scene::load(&to)?;
        if back != scene {
            std::fs::remove_file(&to).ok();
            bail!("{} did not read back as {} was; the .vox is kept", to.display(), named(p));
        }
        std::fs::remove_file(dir.join(p)).with_context(|| format!("removing {}", dir.join(p).display()))?;
        out.push(format!("{} became {} ({said}{}), read back the same", named(p), named(&with_ending(p, "vvx")), marker_words(&scene)));
    }
    // The maps name the prefabs brought over.
    let done: Vec<PathBuf> = prefabs.iter().filter(|p| !kept.contains(p)).cloned().collect();
    let names = map_names(dir, &done)?;
    let mut maps: Vec<&PathBuf> = names.iter().map(|n| &n.0).collect();
    maps.dedup();
    for m in maps {
        let path = dir.join(m);
        let mut data = crate::map::MapData::read(&std::fs::read_to_string(&path)?).with_context(|| named(m))?;
        for p in data.prefabs.values_mut() {
            if let Some((_, _, to)) = names.iter().find(|(n, from, _)| n == m && *from == p.file) {
                p.file = to.clone();
            }
        }
        crate::blocks::save_at_once(&path, data.write().as_bytes())?;
        out.push(format!("{} names the prefabs' .vvx", named(m)));
    }
    let renamed: Vec<(String, String)> = worlds.iter().filter(|p| !kept.contains(p)).map(|p| (named(p), named(&with_ending(p, "vvx")))).collect();
    let mut edits: Vec<_> = lua_renames(dir, &renamed).into_iter().map(|(f, n, old, new)| (f, n, old, Some(new))).collect();
    if kept.is_empty() {
        edits.extend(marker_removals(dir).into_iter().map(|(n, line)| (PathBuf::from("conf.lua"), n, line, None)));
    }
    edits.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    out.extend(write_lua(dir, &edits)?);
    if !kept.is_empty() {
        out.push("c.markers stays in conf.lua while a world or prefab is still a .vox".into());
    } else if !self::markers(dir).is_empty() {
        out.push("conf.lua still sets c.markers in a way this step cannot take out: take those lines out by hand (the markers are in the files now)".into());
    }
    Ok(out)
}

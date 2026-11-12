//! Bringing a game made for an older Voxcade up to date (`voxcade upgrade
//! GAME`). Before 1.0 the formats still change; the engine reads only the
//! current ones, and every change of a format comes with a step here that
//! brings a game's files over.
//!
//! A game states the Voxcade its files are for in `conf.lua`, one line:
//! `c.voxcade = "0.6.0"` (`VERSION_LINE`; a game without it was made
//! before 0.6.0). It opens when no step is newer than that and it is not
//! newer than the engine (`opens`); else `run`, `shot`, `check` and the
//! rest refuse it with the command that helps. `standing` says what a game
//! needs, each step with what it would do (the IDE's and the launcher's
//! question); `upgrade` does it, or with `dry` only says, and then writes
//! the engine's version into that line. Steps look at the game's files to
//! say exactly what they change and change nothing else; the run never
//! commits, but says first when the folder has changes git has not seen.
//!
//! A game is a folder with `conf.lua` or `main.lua` (a game in Rust is
//! built with the engine and needs no line).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// One step: the version it arrives with, what it is, what it would do to
/// a game (nothing: the game needs no change from it) and the doing.
struct Step {
    since: &'static str,
    name: &'static str,
    plan: fn(&Path) -> Result<Vec<String>>,
    apply: fn(&Path) -> Result<Vec<String>>,
}

/// Every step, oldest first. Next to come: level files from JSON to TOML.
const STEPS: &[Step] = &[
    Step { since: "0.6.0", name: "models as text", plan: models::plan, apply: models::apply },
    Step { since: "0.6.0", name: "no recorded sounds", plan: recordings::plan, apply: recordings::plan },
];

/// The setting naming the Voxcade a game's files are for, as `conf.lua`
/// writes it (`c.voxcade = "0.6.0"`).
pub const VERSION_LINE: &str = "c.voxcade";

/// This engine's version.
pub fn engine() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A version's numbers, for comparing ("0.6.0" before "0.10.0").
fn numbers(version: &str) -> Option<Vec<u64>> {
    let v: Option<Vec<u64>> = version.trim().split('.').map(|n| n.parse().ok()).collect();
    v.filter(|v| !v.is_empty() && v.len() <= 4)
}

fn older(a: &str, b: &str) -> bool {
    numbers(a) < numbers(b)
}

/// Whether the folder is a Lua game (it has `conf.lua` or `main.lua`).
pub fn is_lua_game(dir: &Path) -> bool {
    dir.join("conf.lua").is_file() || dir.join("main.lua").is_file()
}

/// The version `conf.lua` states, as the line writes it; None without the
/// line (or without `conf.lua`).
pub fn stated(dir: &Path) -> Result<Option<String>> {
    let path = dir.join("conf.lua");
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    for (i, line) in text.lines().enumerate() {
        if let Some(value) = version_in(line) {
            if numbers(&value).is_none() {
                bail!("{}:{}: {VERSION_LINE} = \"{value}\" is no version like \"{}\"", path.display(), i + 1, engine());
            }
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// The version a line of `conf.lua` sets (`c.voxcade = "0.6.0"`), if it is
/// that line.
fn version_in(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(VERSION_LINE)?.trim_start().strip_prefix('=')?.trim_start();
    let quote = rest.chars().next().filter(|q| *q == '"' || *q == '\'')?;
    let value = &rest[1..];
    Some(value[..value.find(quote)?].to_string())
}

/// A step a game needs and what it would do.
#[derive(Clone, Debug)]
pub struct Plan {
    pub name: &'static str,
    /// The version the step came with.
    pub since: &'static str,
    /// What it would do, a line each, in plain words.
    pub would: Vec<String>,
}

/// Where a game stands against this engine.
#[derive(Clone, Debug)]
pub enum Standing {
    /// It opens: the same version, or older with no step in between, or
    /// no Lua game at all.
    Current,
    /// Made for an older Voxcade (`made`: the version it states, None
    /// without the line): the steps it needs.
    Older { made: Option<String>, steps: Vec<Plan> },
    /// Made with a newer Voxcade than this one.
    Newer { made: String },
}

/// Where the game in `dir` stands, and what an upgrade would do (a dry
/// run): what the IDE and the launcher ask before opening a game.
pub fn standing(dir: &Path) -> Result<Standing> {
    if !is_lua_game(dir) {
        return Ok(Standing::Current);
    }
    let made = stated(dir)?;
    if let Some(v) = &made
        && older(engine(), v)
    {
        return Ok(Standing::Newer { made: v.clone() });
    }
    let due: Vec<&Step> = STEPS.iter().filter(|s| made.as_deref().is_none_or(|v| older(v, s.since))).collect();
    if due.is_empty() {
        return Ok(Standing::Current);
    }
    let steps = due.iter().map(|s| Ok(Plan { name: s.name, since: s.since, would: (s.plan)(dir)? })).collect::<Result<_>>()?;
    Ok(Standing::Older { made, steps })
}

/// Whether the game in `dir` may be opened by this engine: a plain mistake
/// naming what is old and the command, or the newer version, if not.
pub fn opens(dir: &Path) -> Result<()> {
    match standing(dir)? {
        Standing::Current => Ok(()),
        Standing::Older { made, .. } => {
            let made = made.map_or("an older Voxcade".to_string(), |v| format!("Voxcade {v}"));
            bail!("{}: made for {made}; this is {}: `voxcade upgrade {}` brings it up to date", dir.display(), engine(), dir.display())
        }
        Standing::Newer { made } => bail!("{}: made with Voxcade {made}, this is {}: get the new version (voxca.de)", dir.display(), engine()),
    }
}

/// Upgrades the game in `dir`: every step it needs, then its version line;
/// with `dry` only says what would be done. The lines of what was (or
/// would be) done.
pub fn upgrade(dir: &Path, dry: bool) -> Result<Vec<String>> {
    if !is_lua_game(dir) {
        bail!("{}: no game made in Lua here (no conf.lua or main.lua)", dir.display());
    }
    let mut out = Vec::new();
    if let Some(changes) = uncommitted(dir) {
        out.push(format!("note: the folder has {changes} change{} git has not committed; the upgrade goes on (it commits nothing), so its changes mix with yours", if changes == 1 { "" } else { "s" }));
    }
    let steps = match standing(dir)? {
        Standing::Newer { made } => bail!("{}: made with Voxcade {made}, this is {}: get the new version to open it", dir.display(), engine()),
        Standing::Current => {
            out.push(format!("{}: nothing to do, its files are for Voxcade {}", dir.display(), stated(dir)?.unwrap_or_else(|| engine().to_string())));
            return Ok(out);
        }
        Standing::Older { steps, .. } => steps,
    };
    for plan in &steps {
        let step = STEPS.iter().find(|s| s.name == plan.name).expect("a step of the list");
        out.push(format!("{} (Voxcade {}):", step.name, step.since));
        let said = if dry { plan.would.clone() } else { (step.apply)(dir)? };
        if said.is_empty() {
            out.push("  nothing to change".into());
        }
        out.extend(said.into_iter().map(|l| format!("  {l}")));
    }
    let line = set_version_in_file(dir, engine(), dry)?;
    out.push(line);
    // Replays name the files they were recorded on: refitted, they take
    // the new ones if they still play exactly as recorded.
    if !files_ending(&dir.join("replays"), "toml").is_empty() {
        out.push(format!("the replays were recorded on the old files: `voxcade check {} --refit` fits them to the new ones if every one still plays as recorded", dir.display()));
    }
    Ok(out)
}

/// How many files git counts as changed or new in the folder, when it is
/// in a git repository and has any.
pub fn uncommitted(dir: &Path) -> Option<usize> {
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(["status", "--porcelain", "--", "."]).stderr(std::process::Stdio::null()).output().ok()?;
    let n = out.status.success().then(|| String::from_utf8_lossy(&out.stdout).lines().count())?;
    (n > 0).then_some(n)
}

/// `conf.lua`'s text with the version line saying `version`: the line
/// changed where there is one, else put first in `function conf(c)`. None
/// when the text has neither.
pub fn with_version(text: &str, version: &str) -> Option<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if let Some(i) = lines.iter().position(|l| version_in(l).is_some()) {
        let line = &lines[i];
        let old = version_in(line)?;
        let at = line.find(&old)?;
        lines[i] = format!("{}{version}{}", &line[..at], &line[at + old.len()..]);
    } else {
        let i = lines.iter().position(|l| {
            let l = l.trim_start();
            l.starts_with("function conf(") || l.starts_with("function conf (")
        })?;
        let indent: String = lines.get(i + 1).map(|l| l.chars().take_while(|c| c.is_whitespace()).collect()).filter(|s: &String| !s.is_empty()).unwrap_or_else(|| "  ".into());
        lines.insert(i + 1, format!("{indent}{VERSION_LINE} = \"{version}\"   -- the Voxcade its files are for (voxcade upgrade brings an older game up to date)"));
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// Writes the version line into the game's `conf.lua` (a new `conf.lua`
/// for a game without one); says what it did, or where to put it by hand.
fn set_version_in_file(dir: &Path, version: &str, dry: bool) -> Result<String> {
    let path = dir.join("conf.lua");
    let line = format!("{VERSION_LINE} = \"{version}\"");
    let verb = if dry { "would say" } else { "says" };
    if !path.is_file() {
        if !dry {
            std::fs::write(&path, format!("-- The game's settings (main.lua has its code).\nfunction conf(c)\n  {line}\nend\n")).with_context(|| format!("writing {}", path.display()))?;
        }
        return Ok(format!("conf.lua (new) {verb} {line}"));
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    match with_version(&text, version) {
        Some(new) => {
            if !dry && new != text {
                std::fs::write(&path, new).with_context(|| format!("writing {}", path.display()))?;
            }
            Ok(format!("conf.lua {verb} {line}"))
        }
        None => Ok(format!("conf.lua has no `function conf(c)`: put {line} in it by hand, as the first line of conf(c)")),
    }
}

/// The game's files of an ending, under `dir` (not hidden ones, not BUILD's
/// `builds/` nor `shots/`), sorted, as paths from `dir`.
pub(crate) fn files_ending(dir: &Path, ending: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, from: &Path, ending: &str, out: &mut Vec<PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                if dir == from && (name == "builds" || name == "shots") {
                    continue;
                }
                walk(&p, from, ending, out);
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ending)) {
                out.push(p.strip_prefix(from).unwrap_or(&p).to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, ending, &mut out);
    out
}

/// A path from the game folder as the game's Lua names it (`/` between).
fn named(path: &Path) -> String {
    path.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Step 0.6.0: every model, figure and block of a game a `.vvx`
/// (`crate::vvx`), the `.vox` gone once the text reads back the same, the
/// Lua that names one in quotes naming the `.vvx`. Worlds and map prefabs
/// stay `.vox` scenes: their colours are numbered by the file's palette
/// and markers are colours, which the text does not carry (yet).
mod models {
    use super::*;

    /// The game's `.vox` files: the models to bring over, and the worlds
    /// and prefabs that stay.
    fn sorted(dir: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
        let all = files_ending(dir, "vox");
        let scenes = scenes(dir, &all)?;
        Ok(all.into_iter().partition(|p| !scenes.contains(p)))
    }

    /// The `.vox` files that are scenes: a world (`world.vox`, `levels/`,
    /// the file `c.world.file` names) or a prefab a map names.
    fn scenes(dir: &Path, all: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let mut named_by = Vec::new();
        for json in files_ending(dir, "json") {
            let text = std::fs::read_to_string(dir.join(&json)).unwrap_or_default();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
                && let Some(prefabs) = v.get("prefabs").and_then(|p| p.as_object())
            {
                let folder = json.parent().unwrap_or(Path::new(""));
                for entry in prefabs.values() {
                    if let Some(file) = entry.as_str().or_else(|| entry.get("file").and_then(|f| f.as_str())) {
                        named_by.push(folder.join(file));
                    }
                }
            }
        }
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
        for line in conf.lines().filter(|l| l.contains("world")) {
            named_by.extend(quoted(line).into_iter().filter(|q| q.ends_with(".vox")).map(PathBuf::from));
        }
        Ok(all.iter().filter(|p| p.starts_with("levels") || named(p) == "world.vox" || named_by.iter().any(|n| named(n) == named(p))).cloned().collect())
    }

    /// The texts between quotes on a line of Lua.
    fn quoted(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = line;
        while let Some(start) = rest.find(['"', '\'']) {
            let q = rest[start..].chars().next().unwrap_or('"');
            let after = &rest[start + 1..];
            let Some(end) = after.find(q) else { break };
            out.push(after[..end].to_string());
            rest = &after[end + 1..];
        }
        out
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
            out.push(format!("{} stays a .vox (a world or a map's prefab)", named(p)));
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
            out.push(format!("{} stays a .vox (a world or a map's prefab)", named(p)));
        }
        Ok(out)
    }
}

/// Step 0.6.0: Voxcade plays no recordings (everything in a game is code
/// or text). A recording cannot be made into numbers, so the step only
/// says which sounds are recordings and what to do; it changes nothing
/// and deletes nothing (the game says the same at load until it is done).
mod recordings {
    use super::*;

    pub fn plan(dir: &Path) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let folder = dir.join(crate::audio::sound::FOLDER);
        for ending in crate::audio::sound::RECORDINGS {
            for p in files_ending(&folder, ending).into_iter().filter(|p| p.parent().is_none_or(|f| f.as_os_str().is_empty())) {
                let name = p.file_stem().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                out.push(format!("sounds/{} is a recording, which Voxcade no longer plays: make the sound \"{name}\" a sounds/{name}.toml of numbers (the IDE's sound maker rolls one to start from), then take the recording out; it is left as it is", named(&p)));
            }
        }
        for p in files_ending(&folder, "toml") {
            let text = std::fs::read_to_string(folder.join(&p)).unwrap_or_default();
            if let Some(n) = text.lines().position(|l| l.trim_start().starts_with("file")) {
                out.push(format!("sounds/{}:{} names a recording (file = ...), which Voxcade no longer plays: write the sound in numbers instead (wave, freq, decay and the rest), and take the recording out; it is left as it is", named(&p), n + 1));
            }
        }
        Ok(out)
    }
}

/// Brings the MagicaVoxel model `vox` in as the `.vvx` at `to` (keeping the
/// signs of one there): its parts hanging as before, checked to read back
/// the same from the file written; with `remove` the `.vox` goes then.
/// Says what it did.
pub fn bring_in(vox: &Path, to: &Path, remove: bool) -> Result<String> {
    let model = crate::vox::import_model(vox).with_context(|| format!("bringing in {}", vox.display()))?;
    let c = crate::vvx::convert(&model, &crate::vvx::Palette::of_file(to)).with_context(|| format!("bringing in {}", vox.display()))?;
    if let Some(folder) = to.parent() {
        std::fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
    }
    crate::blocks::save_at_once(to, c.text.as_bytes())?;
    let back = crate::vvx::load_in(&crate::files::Files::disk(), to)?;
    let d = crate::vvx::differences(&c.model, &back);
    if !d.is_empty() {
        bail!("{} does not read back as {} was ({}); the .vox is kept", to.display(), vox.display(), d.join("; "));
    }
    let mut said = format!("{} became {}", vox.display(), to.display());
    let mut notes = Vec::new();
    if c.joints + c.parents > 0 {
        notes.push(format!("{} joint{} and {} parent{} written by hand so its parts hang as before", c.joints, if c.joints == 1 { "" } else { "s" }, c.parents, if c.parents == 1 { "" } else { "s" }));
    }
    if let Some((from, to)) = c.grown {
        notes.push(format!("its box grown from {} by {} by {} to {} by {} by {} to hold voxels the .vox kept outside it; it stands and looks as before", from.x, from.y, from.z, to.x, to.y, to.z));
    }
    if c.dropped > 0 {
        notes.push(format!("{} voxel{} dropped: a second in a cell of one frame or part, the first kept as the game showed it", c.dropped, if c.dropped == 1 { "" } else { "s" }));
    }
    if !notes.is_empty() {
        said += &format!(" ({})", notes.join("; "));
    }
    if remove {
        std::fs::remove_file(vox).with_context(|| format!("removing {}", vox.display()))?;
    }
    Ok(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(older("0.5.0", "0.6.0") && older("0.9.0", "0.10.0") && !older("0.6.0", "0.6.0") && !older("1.0.0", "0.9.9"));
        assert_eq!(version_in("  c.voxcade = \"0.6.0\"   -- for"), Some("0.6.0".into()));
        assert_eq!(version_in("c.voxcade='0.5.1'"), Some("0.5.1".into()));
        assert_eq!(version_in("-- c.voxcade = \"0.6.0\""), None);
    }

    /// The line goes first into conf(c) when there is none, and changes
    /// only its version when there is.
    #[test]
    fn the_version_line_is_written_into_conf() {
        let text = "-- a game\nfunction conf(c)\n    c.title = \"X\"\nend\n";
        let new = with_version(text, "0.6.0").unwrap();
        assert_eq!(new.lines().nth(2).unwrap().split("--").next().unwrap().trim_end(), "    c.voxcade = \"0.6.0\"");
        assert!(new.ends_with("end\n"));
        let again = with_version(&new, "0.7.1").unwrap();
        assert_eq!(again.replace("0.7.1", "0.6.0"), new);
        assert_eq!(with_version("x = 1\n", "0.6.0"), None);
    }

    /// A game without the line was made before; one stating a newer
    /// version does not open either; the same version opens.
    #[test]
    fn a_game_opens_only_for_its_version() {
        let dir = std::env::temp_dir().join(format!("voxcade-upgrade-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.title = \"X\"\nend\n").unwrap();
        let e = opens(&dir).unwrap_err().to_string();
        assert!(e.contains("made for an older Voxcade") && e.contains("voxcade upgrade"), "{e}");
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.5.0\"\nend\n").unwrap();
        assert!(opens(&dir).unwrap_err().to_string().contains("made for Voxcade 0.5.0; this is"));
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"99.0.0\"\nend\n").unwrap();
        assert!(opens(&dir).unwrap_err().to_string().contains("made with Voxcade 99.0.0"));
        std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.voxcade = \"{}\"\nend\n", engine())).unwrap();
        opens(&dir).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Every game and template here is for this Voxcade.
    #[test]
    fn the_games_here_open() {
        for folder in ["games", "templates", "examples"] {
            for game in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(folder)).unwrap().flatten() {
                opens(&game.path()).unwrap();
            }
        }
    }

    /// An older game's models become text, its Lua names them, the line
    /// is written; a dry run changes nothing; a world stays.
    #[test]
    fn an_older_game_is_brought_up_to_date() {
        let dir = std::env::temp_dir().join(format!("voxcade-upgrade-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("blocks")).unwrap();
        crate::vox::write_figure(&dir.join("robot.vox"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO), ("arm", glam::IVec3::new(1, 2, 5), glam::IVec3::new(6, 1, 3))]);
        crate::vox::write_figure(&dir.join("world.vox"), &[("ground", glam::IVec3::new(8, 8, 2), glam::IVec3::ZERO), ("rock", glam::IVec3::new(2, 2, 2), glam::IVec3::new(0, 0, 2))]);
        std::fs::copy(dir.join("robot.vox"), dir.join("blocks/stone.vox")).unwrap();
        let conf = "function conf(c)\n  c.things.robot = { model = \"robot.vox\" }\n  c.world.file = \"world.vox\"\nend\n";
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        std::fs::write(dir.join("main.lua"), "function on_draw(frame)\n  frame:model('robot' .. \".vox\", player)\nend\n").unwrap();
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        std::fs::write(dir.join("sounds/scream.wav"), b"RIFF").unwrap();
        let Standing::Older { made: None, steps } = standing(&dir).unwrap() else { panic!("not older") };
        assert_eq!(steps.len(), 2);
        assert!(steps[1].would.iter().any(|l| l.starts_with("sounds/scream.wav is a recording") && l.contains("sounds/scream.toml")), "{:?}", steps[1].would);
        let dry = upgrade(&dir, true).unwrap().join("\n");
        assert!(dry.contains("robot.vox becomes robot.vvx (a figure of 2 parts)") && dry.contains("world.vox stays a .vox") && dry.contains("would say c.voxcade"), "{dry}");
        assert!(dir.join("robot.vox").is_file() && std::fs::read_to_string(dir.join("conf.lua")).unwrap() == conf, "a dry run changes nothing");
        let done = upgrade(&dir, false).unwrap().join("\n");
        assert!(done.contains("robot.vvx") && done.contains("conf.lua:2 now"), "{done}");
        assert!(!dir.join("robot.vox").exists() && dir.join("robot.vvx").is_file() && dir.join("blocks/stone.vvx").is_file() && dir.join("world.vox").is_file());
        assert!(done.contains("sounds/scream.wav is a recording") && dir.join("sounds/scream.wav").is_file(), "the recording is named, not deleted");
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        assert!(conf.contains("model = \"robot.vvx\"") && conf.contains("c.world.file = \"world.vox\"") && conf.contains(&format!("c.voxcade = \"{}\"", engine())), "{conf}");
        assert!(std::fs::read_to_string(dir.join("main.lua")).unwrap().contains("'robot' .. \".vvx\""), "a model's name made in code");
        opens(&dir).unwrap();
        assert!(upgrade(&dir, false).unwrap().join("\n").contains("nothing to do"));
        std::fs::remove_dir_all(&dir).ok();
    }
}

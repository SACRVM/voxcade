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
/// `left` says whether a game already stating the step's version still has
/// files it changes: a step that came while that version was being made
/// (0.6.0 is not out yet) also comes to the games made with it before.
struct Step {
    since: &'static str,
    name: &'static str,
    plan: fn(&Path) -> Result<Vec<String>>,
    apply: fn(&Path) -> Result<Vec<String>>,
    left: Option<fn(&Path) -> bool>,
}

/// Every step, oldest first. "rotation and behavior" goes first of those
/// of 0.6.0, though it came last: the other steps read the game's text
/// files and `conf.lua` with the words of now.
const STEPS: &[Step] = &[
    Step { since: "0.6.0", name: "rotation and behavior", plan: words::plan, apply: words::apply, left: Some(words::left) },
    Step { since: "0.6.0", name: "models as text", plan: models::plan, apply: models::apply, left: None },
    Step { since: "0.6.0", name: "no recorded sounds", plan: recordings::plan, apply: recordings::plan, left: None },
    Step { since: "0.6.0", name: "levels as text", plan: levels::plan, apply: levels::apply, left: Some(levels::left) },
    Step { since: "0.6.0", name: "maps as text", plan: maps::plan, apply: maps::apply, left: Some(maps::left) },
    Step { since: "0.6.0", name: "worlds and prefabs as text", plan: worlds::plan, apply: worlds::apply, left: Some(worlds::left) },
];

impl Step {
    /// Whether a game stating version `made` (None: none) needs the step.
    fn due(&self, made: Option<&str>, dir: &Path) -> bool {
        match made {
            None => true,
            Some(v) if older(v, self.since) => true,
            Some(v) => numbers(v) == numbers(self.since) && self.left.is_some_and(|left| left(dir)),
        }
    }
}

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
    let due: Vec<&Step> = STEPS.iter().filter(|s| s.due(made.as_deref(), dir)).collect();
    if due.is_empty() {
        return Ok(Standing::Current);
    }
    let steps: Vec<Plan> = due.iter().map(|s| Ok(Plan { name: s.name, since: s.since, would: (s.plan)(dir)? })).collect::<Result<_>>()?;
    // A game stating no version is taken as made before the line came
    // (0.6.0); where no step finds anything of it to change, its files are
    // this Voxcade's already: it opens, only the line missing (`unstated`).
    if made.is_none() && steps.iter().all(|p| p.would.is_empty()) {
        return Ok(Standing::Current);
    }
    Ok(Standing::Older { made, steps })
}

/// The plain line for a game that opens though its `conf.lua` states no
/// Voxcade (`standing` found nothing to change): None when it states one,
/// or is no Lua game.
pub fn unstated(dir: &Path) -> Option<String> {
    if !is_lua_game(dir) || !matches!(stated(dir), Ok(None)) {
        return None;
    }
    Some(format!("conf.lua states no Voxcade: {VERSION_LINE} = \"{}\" as the first line of conf(c) says its files are for this one (`voxcade upgrade {}` writes it)", engine(), dir.display()))
}

/// Whether the game in `dir` may be opened by this engine: a plain mistake
/// naming what is old and the command, or the newer version, if not.
pub fn opens(dir: &Path) -> Result<()> {
    match standing(dir)? {
        Standing::Current => {
            if let Some(line) = unstated(dir) {
                eprintln!("{}: {line}", dir.display());
            }
            Ok(())
        }
        Standing::Older { made, steps } => {
            // A game of this very version holding something it reads no
            // more (`c.markers` typed anew): what, not "made for an earlier".
            if made.as_deref() == Some(engine())
                && let Some(first) = steps.iter().find_map(|s| s.would.first())
            {
                let names = steps.iter().map(|s| s.name).collect::<Vec<_>>().join(", ");
                bail!("{}: Voxcade {} reads this no more ({names}): {first}; `voxcade upgrade {}` brings it over", dir.display(), engine(), dir.display());
            }
            let made = match made {
                Some(v) if v == engine() => format!("an earlier Voxcade {v} (before: {})", steps.iter().map(|s| s.name).collect::<Vec<_>>().join(", ")),
                Some(v) => format!("Voxcade {v}"),
                // No line: made before it came, and a step has something to
                // change (else it opens): say the first.
                None => {
                    let (step, first) = steps.iter().find_map(|s| s.would.first().map(|w| (s.name, w.as_str()))).unwrap_or(("", ""));
                    format!("an older Voxcade (conf.lua states none, {VERSION_LINE}; {step}: {first})")
                }
            };
            bail!("{}: made for {made}; this is {}: `voxcade upgrade {}` brings it up to date", dir.display(), engine(), dir.display())
        }
        Standing::Newer { made } => bail!("{}: made with Voxcade {made}, this is {}: get the new version (voxca.de)", dir.display(), engine()),
    }
}

/// Where an upgrade keeps a game's files as they were, in the config folder
/// (as `update` keeps the program's in `before-update/`).
pub const KEPT: &str = "before-upgrade";

/// Upgrades the game in `dir`: every step it needs, then its version line;
/// with `dry` only says what would be done. The lines of what was (or
/// would be) done. A game that is not in a git repository with everything
/// committed has its files kept first, as they were, in the config
/// folder's `before-upgrade/` (`keep_files`): the steps change files in
/// place, and git cannot give them back.
pub fn upgrade(dir: &Path, dry: bool) -> Result<Vec<String>> {
    // The tests keep theirs in a scratch folder, never the user's config.
    #[cfg(test)]
    let scratch = crate::scratch::Scratch::new("upgrade-kept");
    #[cfg(test)]
    let keep = scratch.to_path_buf();
    #[cfg(not(test))]
    let keep = crate::settings::config_dir().join(KEPT);
    upgrade_keeping(dir, dry, &keep)
}

/// `upgrade`, keeping the files of a game outside git in a folder under
/// `keep`.
pub fn upgrade_keeping(dir: &Path, dry: bool, keep: &Path) -> Result<Vec<String>> {
    if !is_lua_game(dir) {
        bail!("{}: no game made in Lua here (no conf.lua or main.lua)", dir.display());
    }
    let mut out = Vec::new();
    if let Some(changes) = uncommitted(dir) {
        out.push(format!("note: the folder has {changes} change{} git has not committed; the upgrade goes on (it commits nothing), so its changes mix with yours", if changes == 1 { "" } else { "s" }));
    }
    let steps = match standing(dir)? {
        Standing::Newer { made } => bail!("{}: made with Voxcade {made}, this is {}: get the new version to open it", dir.display(), engine()),
        // Only the version line missing: it is written, nothing else.
        Standing::Current if stated(dir)?.is_none() => {
            out.push(format!("{}: its files are for Voxcade {} already", dir.display(), engine()));
            out.push(set_version_in_file(dir, engine(), dry)?);
            return Ok(out);
        }
        Standing::Current => {
            out.push(format!("{}: nothing to do, its files are for Voxcade {}", dir.display(), stated(dir)?.unwrap_or_else(|| engine().to_string())));
            return Ok(out);
        }
        Standing::Older { steps, made } => (steps, made),
    };
    let (steps, made) = steps;
    // The files as they were, where git does not hold them all.
    let kept = if dry || committed(dir) { None } else { Some(keep_files(dir, keep, made.as_deref())?) };
    if let Some(k) = &kept {
        out.push(format!("the game's files as they were are kept in {} (the folder is no git repository with everything committed)", k.display()));
    }
    let back = || match &kept {
        Some(k) => format!("the game's files as they were are in {}", k.display()),
        None => "git has the game's files as they were".to_string(),
    };
    for plan in &steps {
        let Some(step) = STEPS.iter().find(|s| s.name == plan.name) else { bail!("no step '{}' in this Voxcade", plan.name) };
        out.push(format!("{} (Voxcade {}):", step.name, step.since));
        let said = if dry { plan.would.clone() } else { (step.apply)(dir).with_context(|| format!("{}: the step '{}' stopped halfway; {}", dir.display(), step.name, back()))? };
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

/// Whether the folder lies in a git repository and git has every one of
/// its files committed (nothing changed, nothing new): git can give back
/// what an upgrade changes.
pub fn committed(dir: &Path) -> bool {
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(["status", "--porcelain", "--", "."]).stderr(std::process::Stdio::null()).output();
    out.is_ok_and(|o| o.status.success() && o.stdout.is_empty())
}

/// Copies the game's files (not hidden ones, not BUILD's `builds/` nor
/// `shots/`) into a new folder under `keep`, named for the game and the
/// Voxcade it was made for (`made`; a number after it when that is taken),
/// and says where. Every file is copied before any step writes.
fn keep_files(dir: &Path, keep: &Path, made: Option<&str>) -> Result<PathBuf> {
    let name = format!("{}-for-{}", crate::settings::game_name(dir), made.unwrap_or("an-older-voxcade"));
    let mut to = keep.join(&name);
    let mut n = 2;
    while to.exists() {
        to = keep.join(format!("{name}-{n}"));
        n += 1;
    }
    fn walk(dir: &Path, from: &Path, out: &mut Vec<PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with('.') || (dir == from && (name == "builds" || name == "shots")) {
                continue;
            }
            if p.is_dir() {
                walk(&p, from, out);
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    for file in &files {
        let target = to.join(file.strip_prefix(dir).unwrap_or(file));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("keeping the game's files before the upgrade: creating {}", parent.display()))?;
        }
        std::fs::copy(file, &target).with_context(|| format!("keeping the game's files before the upgrade: copying {} to {} (nothing of the game was changed)", file.display(), target.display()))?;
    }
    Ok(to)
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
/// are left to the step "worlds and prefabs as text" (`worlds`): their
/// colours are numbered and their markers named there.
mod models {
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

/// The texts between quotes on a line of Lua.
fn quoted_texts(line: &str) -> Vec<String> {
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

/// The files of a game's levels and maps as they were before 0.6.0
/// (JSON), and what a step finds of them.
mod legacy {
    use std::collections::BTreeMap;

    use anyhow::{Result, anyhow, bail};
    use glam::IVec3;
    use serde::Deserialize;

    use crate::blocks::Turn;
    use crate::grid::{BlockEntry, Cell, GridLevel, Place, Thing};
    use crate::map::{Ground, MapData, Material, PrefabEntry, Put};

    /// A level of blocks as JSON: the cells as runs (how many in a row,
    /// block, turn), x fastest, then y, then z.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LevelFile {
        block: i32,
        size: [i32; 3],
        blocks: Vec<BlockEntry>,
        cells: Vec<[u32; 3]>,
        #[serde(default)]
        things: Vec<Thing>,
        #[serde(default)]
        places: Vec<Place>,
    }

    /// A level of blocks from its JSON, as the engine read it before.
    pub fn level(text: &str) -> Result<GridLevel> {
        let file: LevelFile = serde_json::from_str(text).map_err(|e| anyhow!("not a level of blocks: {e}"))?;
        let [x, y, z] = file.size;
        if file.block < 1 || x < 1 || y < 1 || z < 1 {
            bail!("a block and the grid are at least 1 each way (block {}, size {x} by {y} by {z})", file.block);
        }
        let mut level = GridLevel::new(file.block, IVec3::new(x, y, z));
        level.blocks = file.blocks;
        level.things = file.things;
        level.places = file.places;
        let mut i = 0usize;
        for [count, block, turn] in file.cells {
            if block as usize > level.blocks.len() {
                bail!("cells: block {block}, but the level names {} blocks", level.blocks.len());
            }
            let turn = Turn::from_number(turn.min(255) as u8).ok_or_else(|| anyhow!("cells: turn {turn}, there are {} (0 to {})", Turn::COUNT, Turn::COUNT - 1))?;
            let end = i + count as usize;
            if end > level.cells.len() {
                bail!("cells: more than the {} the grid holds", level.cells.len());
            }
            level.cells[i..end].fill(Cell { block: block as u16, turn });
            i = end;
        }
        if i != level.cells.len() {
            bail!("cells: {i} given, the grid holds {}", level.cells.len());
        }
        Ok(level)
    }

    /// A map as JSON.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct MapFile {
        size: [i32; 3],
        ground: GroundFile,
        #[serde(default)]
        prefabs: BTreeMap<String, PrefabFile>,
        #[serde(default)]
        place: Vec<PlaceFile>,
        #[serde(default)]
        places: Vec<Place>,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PrefabFile {
        File(String),
        With {
            file: String,
            #[serde(default)]
            soft: bool,
        },
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GroundFile {
        height: String,
        #[serde(default)]
        material: Option<String>,
        #[serde(default)]
        bedrock: Option<String>,
        materials: Vec<MaterialFile>,
    }

    #[derive(Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct MaterialFile {
        key: Option<String>,
        surface: Vec<String>,
        depth: i32,
        pattern: f32,
        grain: f32,
        strata: Vec<String>,
        band: i32,
        wobble: f32,
    }

    impl Default for MaterialFile {
        fn default() -> Self {
            let d = Material::default();
            Self { key: d.key, surface: d.surface, depth: d.depth, pattern: d.pattern, grain: d.grain, strata: d.strata, band: d.band, wobble: d.wobble }
        }
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PlaceFile {
        #[serde(default)]
        prefab: Option<String>,
        #[serde(default)]
        marker: Option<String>,
        at: Vec<i32>,
        #[serde(default)]
        turn: i32,
    }

    /// A map from its JSON, as the engine read it before.
    pub fn map(text: &str) -> Result<MapData> {
        let m: MapFile = serde_json::from_str(text).map_err(|e| anyhow!("not a map: {e}"))?;
        let g = m.ground;
        let materials = g.materials.into_iter().map(|m| Material { key: m.key, surface: m.surface, depth: m.depth, pattern: m.pattern, grain: m.grain, strata: m.strata, band: m.band, wobble: m.wobble }).collect();
        let prefabs = m
            .prefabs
            .into_iter()
            .map(|(name, p)| {
                let entry = match p {
                    PrefabFile::File(file) => PrefabEntry { file, soft: false },
                    PrefabFile::With { file, soft } => PrefabEntry { file, soft },
                };
                (name, entry)
            })
            .collect();
        let put = m.place.into_iter().map(|p| Put { prefab: p.prefab, thing: p.marker, at: p.at, rotation: Put::rotation_of(p.turn) }).collect();
        Ok(MapData { size: m.size, ground: Ground { height: g.height, material: g.material, bedrock: g.bedrock, materials }, prefabs, put, places: m.places })
    }

    /// What a JSON file of a game is.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Sort {
        Level,
        Map,
    }

    /// Whether the JSON text is a level of blocks or a map (by what it
    /// holds), or neither.
    pub fn sort(text: &str) -> Option<Sort> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let o = v.as_object()?;
        if o.contains_key("cells") && o.contains_key("block") {
            Some(Sort::Level)
        } else if o.contains_key("ground") {
            Some(Sort::Map)
        } else {
            None
        }
    }
}

/// The game's level and map files of a sort as JSON: in `levels/`, the
/// defaults `level.json` and `map.json`, and those `conf.lua` names on a
/// line about the world; each as a path from the game folder.
fn json_files(dir: &Path, sort: legacy::Sort) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = files_ending(&dir.join("levels"), "json").into_iter().filter(|p| p.parent().is_none_or(|f| f.as_os_str().is_empty())).map(|p| Path::new("levels").join(p)).collect();
    found.extend([PathBuf::from("level.json"), PathBuf::from("map.json")]);
    found.extend(world_lines(dir).into_iter().filter(|q| q.ends_with(".json")).map(PathBuf::from));
    found.sort();
    found.dedup();
    found.into_iter().filter(|p| std::fs::read_to_string(dir.join(p)).ok().and_then(|t| legacy::sort(&t)) == Some(sort)).collect()
}

/// The texts in quotes on `conf.lua`'s lines about the world (`c.world.file`,
/// `c.world.blocks`).
fn world_lines(dir: &Path) -> Vec<String> {
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    conf.lines().filter(|l| l.contains("world") && !l.trim_start().starts_with("--")).flat_map(quoted_texts).collect()
}

/// The game's maps as text (`.toml` files the world lines or `levels/`
/// name that read as maps).
fn toml_maps(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = files_ending(&dir.join("levels"), "toml").into_iter().filter(|p| p.parent().is_none_or(|f| f.as_os_str().is_empty())).map(|p| Path::new("levels").join(p)).collect();
    found.push(PathBuf::from("map.toml"));
    found.extend(world_lines(dir).into_iter().filter(|q| q.ends_with(".toml")).map(PathBuf::from));
    found.sort();
    found.dedup();
    found.into_iter().filter(|p| std::fs::read_to_string(dir.join(p)).is_ok_and(|t| t.trim_start().starts_with(&format!("format = \"{}\"", crate::map::FORMAT)))).collect()
}

/// A map of the game as text, read with the words of now though its file
/// may still have those of before (a dry run's later steps read it before
/// "rotation and behavior" has changed it).
fn read_map(dir: &Path, p: &Path) -> Result<crate::map::MapData> {
    let text = std::fs::read_to_string(dir.join(p)).with_context(|| format!("reading {}", dir.join(p).display()))?;
    let text = words::now(p, &text)?;
    crate::map::MapData::read(&text).with_context(|| named(p))
}

/// Where the game's worlds and prefabs are.
mod scenes {
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
}

/// Each Lua line of the game naming one of the files `renamed` (from, to:
/// paths as the Lua names them) in quotes: the file, the line's number, the
/// line as it is and as it becomes.
fn lua_renames(dir: &Path, renamed: &[(String, String)]) -> Vec<(PathBuf, usize, String, String)> {
    let mut out = Vec::new();
    for lua in files_ending(dir, "lua") {
        let text = std::fs::read_to_string(dir.join(&lua)).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            let mut new = line.to_string();
            for q in ['"', '\''] {
                for (from, to) in renamed {
                    new = new.replace(&format!("{q}{from}{q}"), &format!("{q}{to}{q}"));
                }
            }
            if new != line {
                out.push((lua.clone(), i + 1, line.to_string(), new));
            }
        }
    }
    out
}

/// Writes the lines `edits` (as `lua_renames` gives them; None: the line
/// goes) into the game's Lua; says each.
fn write_lua(dir: &Path, edits: &[(PathBuf, usize, String, Option<String>)]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut files: Vec<&PathBuf> = edits.iter().map(|e| &e.0).collect();
    files.dedup();
    for file in files {
        let path = dir.join(file);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut lines: Vec<Option<String>> = text.lines().map(|l| Some(l.to_string())).collect();
        for (_, n, old, new) in edits.iter().filter(|e| e.0 == *file) {
            if lines.get(n - 1).and_then(|l| l.as_deref()) != Some(old.as_str()) {
                bail!("{}:{n} is no longer what it was; nothing written to it", named(file));
            }
            lines[n - 1] = new.clone();
            match new {
                Some(new) => out.push(format!("{}:{n} now: {}", named(file), new.trim())),
                None if old.trim().is_empty() => out.push(format!("{}:{n} taken out: a blank line left double", named(file))),
                None => out.push(format!("{}:{n} taken out: {}", named(file), old.trim())),
            }
        }
        let mut joined = lines.into_iter().flatten().collect::<Vec<_>>().join("\n");
        if text.ends_with('\n') {
            joined.push('\n');
        }
        crate::blocks::save_at_once(&path, joined.as_bytes())?;
    }
    Ok(out)
}

/// The path with another ending, as the Lua names it.
fn with_ending(path: &Path, ending: &str) -> PathBuf {
    path.with_extension(ending)
}

/// Step 0.6.0: a level of blocks is text (`levels/NAME.toml`, "voxcade
/// level 1", `grid::text`), no longer JSON: each read as the engine read it
/// before, written, read back and found the same before the JSON goes.
mod levels {
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
}

/// Step 0.6.0: a map is text (`map.toml`, "voxcade map 1", `map::MapData`),
/// no longer JSON: read as the engine read it before, written, read back
/// and found the same before the JSON goes. Its prefabs keep their names
/// here (`worlds` brings them over).
mod maps {
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
}

/// Step 0.6.0: worlds and map prefabs are `.vvx` too (`crate::scene`). A
/// world's voxels are numbers into its palette and markers were numbers
/// (`c.markers`): the text numbers the colours by the palette's order and
/// names each marker's kind in its entry, so the conversion writes the
/// palette so that every voxel keeps the number it had (a world: its file's
/// numbers in order; a prefab: its colours in the order the map took
/// them, their first use) and the markers so that the things come in the
/// order they came. Each is checked to read back the same before the `.vox`
/// goes; then the maps name the `.vvx`, and `c.markers` leaves `conf.lua`.
pub(crate) mod worlds {
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
}

/// Step 0.6.0: Voxcade says what other tools say (the owner, 2026-11-12:
/// "turn" meant five things, and British spellings go). A level's thing
/// `turn` is `rotation`, its legend's `turn` (a block's way of lying, one
/// of 48) `orientation`; a map's `[[put]]` `turn` (quarter turns
/// anticlockwise) `rotation` in degrees clockwise, as a level's thing's
/// (`map::Put::rotation_of`: turn 1 is 270, 2 is 180, 3 is 90); in
/// `conf.lua` a kind's `behaviour` `behavior` and its `turns` `rotates`,
/// `c.camera.turns` `c.camera.rotates`, a title prop's `turn` `rotation`
/// in degrees as a map's; `frame:model`'s `turn` (a rotation, a pose's
/// part) `rotation`.
///
/// The text files are changed key by key (comments, order and the rest as
/// they were) and read back before they are written. Lua is changed token
/// by token, never by guessing: in `conf.lua` the keys where the settings
/// have them (the word `behaviour` in its comments too, as it names the
/// key); elsewhere the `turn` keys of tables written into a `:model(...)`
/// call. A `turn` key in any other table of the game's Lua is the game's
/// own, or a rotation built apart from the call: the step names its line
/// and leaves it (the game says so when it reaches `frame:model`).
mod words {
    use std::ops::Range;

    use toml_edit::{Document, Item};

    use super::*;
    use crate::lua::lex::{self, Kind};

    /// The changes to one file: its text before and after, and what was
    /// changed in words.
    struct Change {
        file: PathBuf,
        before: String,
        after: String,
        said: Vec<String>,
    }

    /// The line a byte of a text is on, from 1.
    fn line_at(text: &str, at: usize) -> usize {
        text[..at.min(text.len())].matches('\n').count() + 1
    }

    /// The text with each range replaced (the ranges apart from one another).
    fn edited(src: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
        edits.sort_by_key(|(r, _)| r.start);
        let mut out = src.to_string();
        for (range, with) in edits.into_iter().rev() {
            out.replace_range(range, &with);
        }
        out
    }

    /// The legend's head and a map's head of its `[[put]]`s as the engine
    /// wrote them before.
    pub(super) const OLD_LEGEND_HEAD: &str = "[legend]              # a sign is a block as it lies: its name, or the name and a turn (0 to 47)";
    pub(super) const OLD_PUT_HEAD: &str = "# What stands on the map, in order: a prefab (turn: quarter turns\n# anticlockwise) or a thing of a kind; at x, y on the ground, or x, y, z.";

    /// What a `.toml` of the game is by its format line: a level, a map or
    /// neither.
    fn format_of(text: &str) -> Option<&'static str> {
        let first = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'))?;
        if first.starts_with(&format!("format = \"{}\"", crate::grid::FORMAT)) {
            Some("level")
        } else if first.starts_with(&format!("format = \"{}\"", crate::map::FORMAT)) {
            Some("map")
        } else {
            None
        }
    }

    /// A level's or a map's text with the new words, and what changed.
    fn text_file(file: &Path, src: &str) -> Result<Option<Change>> {
        let Some(sort) = format_of(src) else { return Ok(None) };
        let doc = Document::parse(src).map_err(|e| anyhow::anyhow!("{}: {}", named(file), e.message()))?;
        let top = doc.as_table();
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        let mut said = Vec::new();
        let key_span = |t: &dyn toml_edit::TableLike, key: &str| -> Option<(Range<usize>, Range<usize>)> {
            let (k, v) = t.get_key_value(key)?;
            Some((k.span()?, v.span()?))
        };
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        if sort == "level" {
            let mut legend = 0;
            if let Some(entries) = top.get("legend").and_then(Item::as_table_like) {
                for (_, entry) in entries.iter() {
                    if let Some(t) = entry.as_table_like()
                        && let Some((k, _)) = key_span(t, "turn")
                    {
                        edits.push((k, "orientation".into()));
                        legend += 1;
                    }
                }
            }
            let mut things = 0;
            if let Some(tables) = top.get("thing").and_then(crate::vvx::tables) {
                for t in tables {
                    if let Some((k, _)) = key_span(t, "turn") {
                        edits.push((k, "rotation".into()));
                        things += 1;
                    }
                }
            }
            if legend > 0 {
                said.push(format!("{}: the legend's turn becomes orientation ({legend} sign{})", named(file), plural(legend)));
            }
            if things > 0 {
                said.push(format!("{}: a thing's turn becomes rotation, the same degrees ({things} thing{})", named(file), plural(things)));
            }
            if let Some(at) = src.find(OLD_LEGEND_HEAD) {
                edits.push((at..at + OLD_LEGEND_HEAD.len(), crate::grid::LEGEND_HEAD.into()));
            }
        } else {
            let mut puts = 0;
            if let Some(tables) = top.get("put").and_then(crate::vvx::tables) {
                for t in tables {
                    let Some((k, v)) = key_span(t, "turn") else { continue };
                    let Some(quarters) = t.get("turn").and_then(Item::as_integer) else {
                        bail!("{}:{}: turn must be a whole number of quarter turns", named(file), line_at(src, k.start));
                    };
                    let rotation = crate::map::Put::rotation_of(quarters as i32);
                    // A turn that comes round to none: its line goes.
                    if rotation == 0 {
                        let begin = src[..k.start].rfind('\n').map_or(0, |i| i + 1);
                        let end = src[v.end..].find('\n').map_or(src.len(), |i| v.end + i + 1);
                        edits.push((begin..end, String::new()));
                    } else {
                        edits.push((k.start..v.end, format!("rotation = {rotation}")));
                    }
                    puts += 1;
                }
            }
            if puts > 0 {
                said.push(format!("{}: a [[put]]'s turn (quarter turns anticlockwise) becomes rotation in degrees clockwise: turn = 1 is rotation = 270, 2 is 180, 3 is 90 ({puts} put{})", named(file), plural(puts)));
            }
            if let Some(at) = src.find(OLD_PUT_HEAD) {
                edits.push((at..at + OLD_PUT_HEAD.len(), crate::map::PUT_HEAD.into()));
            }
        }
        if edits.is_empty() {
            return Ok(None);
        }
        let after = edited(src, edits);
        // Read back: the new text is a level or a map again.
        let back = if sort == "level" { crate::grid::GridLevel::from_text(&after).map(|_| ()) } else { crate::map::MapData::read(&after).map(|_| ()) };
        back.with_context(|| format!("{} with the new words does not read", named(file)))?;
        if said.is_empty() {
            said.push(format!("{}: its comment names the new words", named(file)));
        }
        Ok(Some(Change { file: file.to_path_buf(), before: src.to_string(), after, said }))
    }

    /// A level's or a map's text with the words of now.
    pub fn now(file: &Path, src: &str) -> Result<String> {
        Ok(text_file(file, src)?.map_or_else(|| src.to_string(), |c| c.after))
    }

    /// A token of Lua that counts (no comment): its text and place, and
    /// whether the bracket it stands in is a table's (`{`).
    struct Tok<'a> {
        text: &'a str,
        kind: Kind,
        range: Range<usize>,
        in_table: bool,
    }

    fn tokens(src: &str) -> Vec<Tok<'_>> {
        let mut open: Vec<&str> = Vec::new();
        let mut out = Vec::new();
        // Brackets, and the blocks of code (a function in a table is code).
        for s in lex::spans(src).into_iter().filter(|s| s.kind != Kind::Comment) {
            let text = &src[s.range.clone()];
            let closes = match s.kind {
                Kind::Symbol => matches!(text, "}" | ")" | "]"),
                Kind::Keyword => matches!(text, "end" | "until"),
                _ => false,
            };
            if closes {
                open.pop();
            }
            out.push(Tok { text, kind: s.kind, range: s.range, in_table: open.last() == Some(&"{") });
            let opens = match s.kind {
                Kind::Symbol => matches!(text, "{" | "(" | "["),
                Kind::Keyword => matches!(text, "function" | "do" | "if" | "repeat"),
                _ => false,
            };
            if opens {
                open.push(text);
            }
        }
        out
    }

    /// Whether token `i` is a key in a table constructor (`{ key = ...`),
    /// not a name of a list of names (`local a, b = ...`).
    fn is_key(toks: &[Tok], i: usize) -> bool {
        toks[i].in_table && toks.get(i + 1).is_some_and(|t| t.text == "=") && i > 0 && matches!(toks[i - 1].text, "{" | "," | ";")
    }

    /// The names of a chain ending at token `i` (`c.things.slime`), first
    /// to last.
    fn chain<'a>(toks: &[Tok<'a>], i: usize) -> Vec<&'a str> {
        let mut out = vec![toks[i].text];
        let mut j = i;
        while j >= 2 && toks[j - 1].text == "." && toks[j - 2].kind == Kind::Name {
            j -= 2;
            out.insert(0, toks[j].text);
        }
        out
    }

    /// Where each name of `conf.lua` that is a key or a field lies among
    /// the settings: its path from `c` (`things`, `slime`, `behaviour`;
    /// `[]` an element of a list, `?` a table the path is not known of).
    fn paths<'a>(toks: &[Tok<'a>]) -> Vec<(usize, Vec<&'a str>)> {
        let mut stack: Vec<Vec<&'a str>> = Vec::new();
        let mut out = Vec::new();
        // The settings' path a chain names (its first name `c`, or a local).
        let from_c = |names: Vec<&'a str>| -> Vec<&'a str> { if names.len() > 1 { names[1..].to_vec() } else { vec!["?"] } };
        for i in 0..toks.len() {
            match toks[i].text {
                "{" => {
                    let path = if i >= 2 && toks[i - 1].text == "=" && toks[i - 2].kind == Kind::Name {
                        if is_key(toks, i - 2) {
                            let mut p = stack.last().cloned().unwrap_or_else(|| vec!["?"]);
                            p.push(toks[i - 2].text);
                            p
                        } else {
                            from_c(chain(toks, i - 2))
                        }
                    } else if i >= 1 && matches!(toks[i - 1].text, "{" | "," | ";") && !stack.is_empty() {
                        let mut p = stack.last().cloned().unwrap_or_default();
                        p.push("[]");
                        p
                    } else {
                        vec!["?"]
                    };
                    stack.push(path);
                }
                "}" => {
                    stack.pop();
                }
                _ if toks[i].kind == Kind::Name => {
                    if is_key(toks, i) && !stack.is_empty() {
                        let mut p = stack.last().cloned().unwrap_or_default();
                        p.push(toks[i].text);
                        out.push((i, p));
                    } else if i >= 1 && toks[i - 1].text == "." {
                        out.push((i, from_c(chain(toks, i))));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Whether a path is `pattern`'s (`*` any one name).
    fn matches(path: &[&str], pattern: &[&str]) -> bool {
        path.len() == pattern.len() && path.iter().zip(pattern).all(|(p, q)| *q == "*" || p == q)
    }

    /// The edits to a Lua file, and the names left that the step cannot
    /// place (their byte, what to write).
    type Edits = (Vec<(Range<usize>, String)>, Vec<(usize, String)>);

    const PROP_TURN: &str = "a title prop's turn (quarter turns anticlockwise) is rotation, degrees clockwise: turn = 1 is rotation = 270";

    /// `conf.lua` with the new words.
    pub(super) fn conf_edits(src: &str) -> Edits {
        let toks = tokens(src);
        let (mut edits, mut left) = (Vec::new(), Vec::new());
        for (i, path) in paths(&toks) {
            match toks[i].text {
                // The word is the setting's wherever it stands.
                "behaviour" => edits.push((toks[i].range.clone(), "behavior".to_string())),
                "turns" if matches(&path, &["camera", "turns"]) || matches(&path, &["things", "*", "turns"]) => edits.push((toks[i].range.clone(), "rotates".to_string())),
                "turns" => left.push((toks[i].range.start, "turns: if it is a kind's or the camera's, write rotates".to_string())),
                "turn" if matches(&path, &["title_screen", "props", "[]", "turn"]) => {
                    // turn = N (a whole number, minus perhaps): rotation in degrees.
                    let (eq, mut v) = (i + 1, i + 2);
                    let minus = toks.get(v).is_some_and(|t| t.text == "-");
                    if minus {
                        v += 1;
                    }
                    let number = toks.get(v).filter(|t| t.kind == Kind::Number).and_then(|t| t.text.parse::<i32>().ok());
                    match (toks.get(eq).map(|t| t.text), number) {
                        (Some("="), Some(q)) => {
                            let rotation = crate::map::Put::rotation_of(if minus { -q } else { q });
                            edits.push((toks[i].range.start..toks[v].range.end, format!("rotation = {rotation}")));
                        }
                        _ => left.push((toks[i].range.start, PROP_TURN.to_string())),
                    }
                }
                // A turn the step cannot place (props[2].turn): perhaps a prop's.
                "turn" if path.contains(&"?") => left.push((toks[i].range.start, format!("if it is a title prop's turn: {PROP_TURN}"))),
                _ => {}
            }
        }
        // Its comments speak of the key as it is called now.
        for s in lex::spans(src).into_iter().filter(|s| s.kind == Kind::Comment) {
            let text = &src[s.range.clone()];
            let new = text.replace("behaviour", "behavior").replace("Behaviour", "Behavior");
            if new != text {
                edits.push((s.range, new));
            }
        }
        (edits, left)
    }

    /// Another Lua file of the game with the new words: the `turn` keys of
    /// the tables written into a `:model(...)` call; the `turn` keys of
    /// other tables are left.
    fn lua_edits(src: &str) -> Edits {
        let toks = tokens(src);
        let mut inside = vec![false; toks.len()];
        for i in 0..toks.len() {
            if toks[i].text == "model" && i >= 1 && toks[i - 1].text == ":" && toks.get(i + 1).is_some_and(|t| t.text == "(") {
                let mut depth = 0;
                for (j, t) in toks.iter().enumerate().skip(i + 1) {
                    match t.text {
                        "(" => depth += 1,
                        ")" => depth -= 1,
                        _ => {}
                    }
                    inside[j] = true;
                    if depth == 0 {
                        break;
                    }
                }
            }
        }
        let (mut edits, mut left) = (Vec::new(), Vec::new());
        for i in 0..toks.len() {
            if toks[i].kind == Kind::Name && toks[i].text == "turn" && is_key(&toks, i) {
                if inside[i] {
                    edits.push((toks[i].range.clone(), "rotation".to_string()));
                } else {
                    left.push((toks[i].range.start, "if this table goes to frame:model (a rotation, or a part's in a pose), its turn is rotation now".to_string()));
                }
            }
        }
        (edits, left)
    }

    /// Every change the step would make to the game in `dir`, and the lines
    /// it leaves to the author.
    fn changes(dir: &Path) -> Result<(Vec<Change>, Vec<String>)> {
        let mut out = Vec::new();
        let mut left = Vec::new();
        for p in files_ending(dir, "toml") {
            let src = std::fs::read_to_string(dir.join(&p)).unwrap_or_default();
            out.extend(text_file(&p, &src)?);
        }
        for p in files_ending(dir, "lua") {
            let Ok(src) = std::fs::read_to_string(dir.join(&p)) else { continue };
            let (edits, rest) = if p == Path::new("conf.lua") { conf_edits(&src) } else { lua_edits(&src) };
            for (at, what) in rest {
                left.push(format!("{}:{} is left as it is: {what}", named(&p), line_at(&src, at)));
            }
            if edits.is_empty() {
                continue;
            }
            let after = edited(&src, edits);
            let (old, new): (Vec<&str>, Vec<&str>) = (src.lines().collect(), after.lines().collect());
            let said = old.iter().zip(&new).enumerate().filter(|(_, (a, b))| a != b).map(|(n, (_, b))| format!("{}:{} becomes: {}", named(&p), n + 1, b.trim())).collect();
            out.push(Change { file: p, before: src, after, said });
        }
        Ok((out, left))
    }

    pub fn left(dir: &Path) -> bool {
        changes(dir).is_ok_and(|(c, _)| !c.is_empty())
    }

    pub fn plan(dir: &Path) -> Result<Vec<String>> {
        let (changes, left) = changes(dir)?;
        Ok(changes.into_iter().flat_map(|c| c.said).chain(left).collect())
    }

    pub fn apply(dir: &Path) -> Result<Vec<String>> {
        let (changes, left) = changes(dir)?;
        let mut out = Vec::new();
        for c in changes {
            let path = dir.join(&c.file);
            if std::fs::read_to_string(&path)? != c.before {
                bail!("{} changed while the upgrade read it; nothing written to it", named(&c.file));
            }
            crate::blocks::save_at_once(&path, c.after.as_bytes())?;
            out.extend(c.said.into_iter().map(|l| l.replacen(" becomes: ", " now: ", 1).replacen(" becomes ", " became ", 1)));
        }
        out.extend(left);
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
        let dir = crate::scratch::Scratch::new("upgrade-open");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.title = \"X\"\nend\n").unwrap();
        // Without the line and nothing any step would change: it opens,
        // the line missing said plainly; `upgrade` writes only the line.
        opens(&dir).unwrap();
        assert!(matches!(standing(&dir).unwrap(), Standing::Current));
        assert!(unstated(&dir).is_some_and(|l| l.contains("c.voxcade = \"") && l.contains("voxcade upgrade")), "{:?}", unstated(&dir));
        let said = upgrade(&dir, true).unwrap();
        assert!(said.iter().any(|l| l.contains("would say c.voxcade")), "{said:?}");
        // Without the line and something to change: made for an older one,
        // the first thing to change named.
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        std::fs::write(dir.join("sounds/boom.wav"), b"RIFF").unwrap();
        let e = opens(&dir).unwrap_err().to_string();
        assert!(e.contains("made for an older Voxcade (conf.lua states none") && e.contains("sounds/boom.wav") && e.contains("voxcade upgrade"), "{e}");
        std::fs::remove_dir_all(dir.join("sounds")).unwrap();
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
    /// is written; a dry run changes nothing; its world becomes text too.
    #[test]
    fn an_older_game_is_brought_up_to_date() {
        let dir = crate::scratch::Scratch::new("upgrade-run");
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
        assert_eq!(steps.len(), STEPS.len());
        let sounds = steps.iter().find(|s| s.name == "no recorded sounds").unwrap();
        assert!(sounds.would.iter().any(|l| l.starts_with("sounds/scream.wav is a recording") && l.contains("sounds/scream.toml")), "{:?}", sounds.would);
        let dry = upgrade(&dir, true).unwrap().join("\n");
        assert!(dry.contains("robot.vox becomes robot.vvx (a figure of 2 parts)") && dry.contains("world.vox is a world or a map's prefab") && dry.contains("world.vox becomes world.vvx (a world, 8 by 8 by 4 voxels") && dry.contains("would say c.voxcade"), "{dry}");
        assert!(dir.join("robot.vox").is_file() && std::fs::read_to_string(dir.join("conf.lua")).unwrap() == conf, "a dry run changes nothing");
        let done = upgrade(&dir, false).unwrap().join("\n");
        assert!(done.contains("robot.vvx") && done.contains("conf.lua:2 now"), "{done}");
        assert!(!dir.join("robot.vox").exists() && dir.join("robot.vvx").is_file() && dir.join("blocks/stone.vvx").is_file() && !dir.join("world.vox").exists() && dir.join("world.vvx").is_file());
        assert!(done.contains("sounds/scream.wav is a recording") && dir.join("sounds/scream.wav").is_file(), "the recording is named, not deleted");
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        assert!(conf.contains("model = \"robot.vvx\"") && conf.contains("c.world.file = \"world.vvx\"") && conf.contains(&format!("c.voxcade = \"{}\"", engine())), "{conf}");
        assert!(std::fs::read_to_string(dir.join("main.lua")).unwrap().contains("'robot' .. \".vvx\""), "a model's name made in code");
        opens(&dir).unwrap();
        assert!(upgrade(&dir, false).unwrap().join("\n").contains("nothing to do"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A game with every word Voxcade said before it took other tools'
    /// (`turn`, `turns`, `behaviour`) in its levels, its map, `conf.lua` and
    /// its Lua.
    fn game_of_old_words(dir: &Path, version_line: &str) {
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::create_dir_all(dir.join("blocks")).unwrap();
        std::fs::create_dir_all(dir.join("ground")).unwrap();
        let conf = format!(
            "-- A kind's behaviour moves it.\nfunction conf(c)\n{version_line}  c.kit = \"platformer\"\n  c.camera.turns = \"free\"\n  c.things.gem = {{ behaviour = \"pickup\", model = \"gem.vvx\", turns = \"free\" }}\n  c.things.bat = {{ model = \"gem.vvx\" }}\n  c.things.bat.behaviour = \"flyer\"\n  c.things.bat.frames = {{ turn = {{ 0, 0 }} }}\n  c.title_screen.props = {{\n    {{ model = \"gem.vvx\", at = {{ 10, 10 }}, turn = 1 }},\n    {{ model = \"gem.vvx\", at = {{ 20, 10 }} }},\n  }}\n  c.world.blocks = {{ file = \"levels/01.toml\" }}\nend\n"
        );
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        let main = "local SPIN = { turn = 3 }   -- the game's own\n\nfunction on_draw(frame)\n  local a, turn = 1, 2\n  frame:model(\"gem.vvx\", player, 0, { turn = 45, pitch = 10 }, { head = { turn = 30 } })\nend\n";
        std::fs::write(dir.join("main.lua"), main).unwrap();
        std::fs::write(dir.join("gem.vvx"), "format = \"voxcade voxel 1\"\nsize = [1, 1, 1]\n\n[palette]\na = \"#ff0000\"\n\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n").unwrap();
        let mut lv = crate::grid::GridLevel::new(2, glam::IVec3::new(3, 1, 2));
        lv.blocks.push(crate::grid::BlockEntry { name: "rock".into(), kind: crate::grid::Kind::Solid });
        lv.cells[1] = crate::grid::Cell { block: 1, turn: crate::blocks::Turn::MIRROR };
        let mut chair = crate::grid::Thing::new("gem", [1, 1, 2]);
        chair.facing = Some(crate::grid::Facing::Turn(30.0));
        lv.things.push(chair);
        // Written as the engine wrote it before.
        let text = crate::grid::text::write(&lv, &crate::grid::Legend::default())
            .replace(crate::grid::LEGEND_HEAD, words::OLD_LEGEND_HEAD)
            .replace("orientation = ", "turn = ")
            .replace("rotation = 30", "turn = 30    # askew");
        assert!(text.contains("turn = 30    # askew") && text.contains(", turn = "), "{text}");
        std::fs::write(dir.join("levels/01.toml"), text).unwrap();
        crate::headless::write_png(&dir.join("ground/height.png"), 16, 16, &[3, 3, 3, 255].repeat(256)).unwrap();
        let mut post = crate::scene::Scene::new(glam::IVec3::new(3, 2, 1));
        post.colours = vec![[200, 10, 10], [10, 200, 10]];
        post.cells = vec![1, 1, 2, 0, 0, 1];
        std::fs::write(dir.join("post.vvx"), post.write()).unwrap();
        let mut map = format!("format = \"{}\"\nsize = [16, 16, 8]\n\n[ground]\nheight = \"ground/height.png\"\n\n[[ground.materials]]\nsurface = [\"#00ff00\"]\n\n[prefabs]\npost = \"post.vvx\"\n\n{}\n", crate::map::FORMAT, words::OLD_PUT_HEAD);
        for (q, at) in [(0, [3, 3]), (1, [8, 3]), (2, [3, 8]), (3, [8, 8]), (5, [12, 12])] {
            map += &format!("[[put]]\nprefab = \"post\"\nat = [{}, {}]\n{}\n", at[0], at[1], if q == 0 { String::new() } else { format!("turn = {q}\n") });
        }
        std::fs::write(dir.join("map.toml"), map).unwrap();
    }

    /// What an old map's prefab put on the ground: each voxel turned by
    /// `turn` quarter turns anticlockwise about its pivot, as the engine did
    /// before (`map::turn`, written out again here).
    fn old_post_cells(at: [i32; 2], quarters: i32) -> Vec<glam::IVec3> {
        let pivot = glam::IVec3::new(1, 1, 0);
        let cells = [(0, 0), (1, 0), (2, 0), (2, 1)].map(|(x, y)| glam::IVec3::new(x, y, 0) - pivot);
        cells
            .iter()
            .map(|&l| {
                let t = match quarters.rem_euclid(4) {
                    0 => l,
                    1 => glam::IVec3::new(-l.y, l.x, l.z),
                    2 => glam::IVec3::new(-l.x, -l.y, l.z),
                    _ => glam::IVec3::new(l.y, -l.x, l.z),
                };
                glam::IVec3::new(at[0], at[1], 3) + t
            })
            .collect()
    }

    /// An older game's words are Voxcade's of now: a level's thing turn its
    /// rotation, the legend's turn an orientation, a map's quarter turns
    /// degrees clockwise building the same world, conf.lua's behaviour,
    /// turns and a title prop's turn, frame:model's turn in the call;
    /// comments kept; a dry run changes nothing; a second run nothing more.
    /// A game made before 0.6.0 and one stating 0.6.0 alike.
    #[test]
    fn the_old_words_become_other_tools_words() {
        for (name, line) in [("words-0.5", "  -- made before 0.6.0\n"), ("words-0.6", "  c.voxcade = \"0.6.0\"\n")] {
            let dir = crate::scratch::Scratch::new(name);
            game_of_old_words(&dir, line);
            let Standing::Older { steps, .. } = standing(&dir).unwrap() else { panic!("{name}: not older") };
            let words = steps.iter().find(|s| s.name == "rotation and behavior").expect("the step");
            let would = words.would.join("\n");
            for says in [
                "levels/01.toml: the legend's turn becomes orientation (1 sign)",
                "levels/01.toml: a thing's turn becomes rotation, the same degrees (1 thing)",
                "map.toml: a [[put]]'s turn (quarter turns anticlockwise) becomes rotation in degrees clockwise: turn = 1 is rotation = 270, 2 is 180, 3 is 90 (4 puts)",
                "conf.lua:1 becomes: -- A kind's behavior moves it.",
                "conf.lua:5 becomes: c.camera.rotates = \"free\"",
                "conf.lua:6 becomes: c.things.gem = { behavior = \"pickup\", model = \"gem.vvx\", rotates = \"free\" }",
                "conf.lua:8 becomes: c.things.bat.behavior = \"flyer\"",
                "conf.lua:11 becomes: { model = \"gem.vvx\", at = { 10, 10 }, rotation = 270 },",
                "main.lua:5 becomes: frame:model(\"gem.vvx\", player, 0, { rotation = 45, pitch = 10 }, { head = { rotation = 30 } })",
                "main.lua:1 is left as it is: if this table goes to frame:model",
            ] {
                assert!(would.contains(says), "{name}: {says}\n{would}");
            }
            assert!(!would.contains("conf.lua:9") && !would.contains("main.lua:4"), "a frame set called turn and a local are the game's: {would}");
            let before: Vec<_> = ["conf.lua", "main.lua", "map.toml", "levels/01.toml"].iter().map(|f| std::fs::read_to_string(dir.join(f)).unwrap()).collect();
            let dry = upgrade(&dir, true).unwrap().join("\n");
            assert!(dry.contains("rotation and behavior (Voxcade 0.6.0):") && dry.contains("conf.lua:5 becomes"), "{dry}");
            let after: Vec<_> = ["conf.lua", "main.lua", "map.toml", "levels/01.toml"].iter().map(|f| std::fs::read_to_string(dir.join(f)).unwrap()).collect();
            assert_eq!(before, after, "a dry run changes nothing");

            let done = upgrade(&dir, false).unwrap().join("\n");
            assert!(done.contains("conf.lua:5 now: c.camera.rotates = \"free\"") && done.contains("levels/01.toml: the legend's turn became orientation"), "{done}");
            let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
            assert!(conf.contains("-- A kind's behavior moves it.") && conf.contains("c.things.bat.frames = { turn = { 0, 0 } }"), "{conf}");
            let level = std::fs::read_to_string(dir.join("levels/01.toml")).unwrap();
            assert!(level.contains(crate::grid::LEGEND_HEAD) && level.contains("rotation = 30    # askew") && level.contains(", orientation = ") && !level.contains("turn"), "{level}");
            let map = std::fs::read_to_string(dir.join("map.toml")).unwrap();
            assert!(map.contains(crate::map::PUT_HEAD) && map.contains("rotation = 270\n") && map.contains("rotation = 180\n") && map.contains("rotation = 90\n") && !map.contains("turn ="), "{map}");
            // The map builds the world the old one did, voxel for voxel.
            let (world, _, _) = crate::map::load_map(&dir.join("map.toml"), &[]).unwrap();
            for (q, at) in [(0, [3, 3]), (1, [8, 3]), (2, [3, 8]), (3, [8, 8]), (5, [12, 12])] {
                let cells = old_post_cells(at, q);
                assert!(cells.iter().all(|&c| world.get(c) != 0), "turn = {q}: the post where it stood");
                assert_eq!(world.palette[world.get(cells[2]) as usize], [10, 200, 10, 255], "turn = {q}: its green end where it was");
            }
            let mut cfg = crate::config::GameConfig::default();
            crate::lua::conf::run(&crate::files::Files::folder(&dir), &mut cfg).unwrap();
            assert_eq!((cfg.camera.rotates, cfg.title_screen.props[0].rotation), (crate::config::Rotates::Free, 270));
            assert_eq!(cfg.things["gem"]["behavior"].as_str(), Some("pickup"));
            assert!(crate::grid::GridLevel::read(&dir.join("levels/01.toml")).unwrap().things[0].facing == Some(crate::grid::Facing::Turn(30.0)));
            let again = upgrade(&dir, false).unwrap().join("\n");
            assert!(again.contains("nothing to do"), "{name}: {again}");
            opens(&dir).unwrap();
        }
        // What the step cannot place is said with its line, and left.
        let (edits, left) = words::conf_edits("function conf(c)\n  c.title_screen.props[2].turn = 2\n  local kind = { turns = \"free\" }\nend\n");
        assert!(edits.is_empty() && left.len() == 2, "{left:?}");
    }

    /// Every file under `dir` (but hidden ones) with its bytes, by its path
    /// from `dir`.
    fn contents(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, from: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.file_name().unwrap().to_string_lossy().starts_with('.') {
                    continue;
                }
                if p.is_dir() {
                    walk(&p, from, out);
                } else {
                    out.insert(p.strip_prefix(from).unwrap().to_path_buf(), std::fs::read(&p).unwrap());
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(dir, dir, &mut out);
        out
    }

    /// A game outside git has its files kept, as they were, before the
    /// first step writes (the steps change files in place: without git a
    /// step failing halfway left no way back); a dry run and a game with
    /// nothing to do keep nothing; a step that fails says where they are.
    #[test]
    fn an_upgrade_outside_git_keeps_the_files_first() {
        let dir = crate::scratch::Scratch::new("upgrade-keep");
        let keep = crate::scratch::Scratch::new("upgrade-keep-to");
        game_of_old_words(&dir, "");
        std::fs::create_dir_all(dir.join("shots")).unwrap();
        std::fs::write(dir.join("shots/a.png"), b"not the game's").unwrap();
        let before = contents(&dir);
        upgrade_keeping(&dir, true, &keep).unwrap();
        assert_eq!(std::fs::read_dir(&keep).unwrap().count(), 0, "a dry run keeps nothing");
        let done = upgrade_keeping(&dir, false, &keep).unwrap().join("\n");
        let kept = keep.join(format!("{}-for-an-older-voxcade", crate::settings::game_name(&dir)));
        assert!(done.contains(&format!("the game's files as they were are kept in {}", kept.display())), "{done}");
        let mut game = before.clone();
        game.retain(|p, _| !p.starts_with("shots"));
        assert_eq!(contents(&kept), game, "every file of the game as it was, not shots/");
        assert_ne!(contents(&dir), before, "the game itself was upgraded");
        assert!(upgrade_keeping(&dir, false, &keep).unwrap().join("\n").contains("nothing to do"));
        assert_eq!(std::fs::read_dir(&keep).unwrap().count(), 1, "nothing to do, nothing kept");

        // A step that cannot write says it stopped and where the files are;
        // a second keeping takes a name of its own.
        let stuck = crate::scratch::Scratch::new("upgrade-stuck");
        game_of_old_words(&stuck, "");
        // (levels/ read-only: the step writes a level beside it and renames.)
        let levels = stuck.join("levels");
        let was = std::fs::metadata(&levels).unwrap().permissions();
        let mut ro = was.clone();
        ro.set_readonly(true);
        std::fs::set_permissions(&levels, ro).unwrap();
        if std::fs::write(levels.join("probe"), "x").is_ok() {
            std::fs::set_permissions(&levels, was).unwrap();
            eprintln!("SKIPPED the failing half: a read-only folder is written here (root?)");
            return;
        }
        let failed = upgrade_keeping(&stuck, false, &keep);
        std::fs::set_permissions(&levels, was).unwrap();
        let e = format!("{:#}", failed.unwrap_err());
        let kept = keep.join(format!("{}-for-an-older-voxcade", crate::settings::game_name(&stuck)));
        assert!(e.contains("stopped halfway") && e.contains(&format!("the game's files as they were are in {}", kept.display())), "{e}");
        assert_eq!(std::fs::read(kept.join("conf.lua")).unwrap(), std::fs::read(stuck.join("conf.lua")).unwrap());
    }

    /// A game in a git repository with everything committed keeps nothing:
    /// git has it all.
    #[test]
    fn an_upgrade_in_git_keeps_nothing() {
        let dir = crate::scratch::Scratch::new("upgrade-git");
        let keep = crate::scratch::Scratch::new("upgrade-git-to");
        game_of_old_words(&dir, "");
        let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(dir.path()).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output();
        if git(&["init", "-q"]).is_err() {
            eprintln!("SKIPPED an_upgrade_in_git_keeps_nothing: no git here");
            return;
        }
        git(&["add", "-A"]).unwrap();
        assert!(git(&["commit", "-q", "-m", "the game"]).unwrap().status.success());
        assert!(committed(&dir));
        let done = upgrade_keeping(&dir, false, &keep).unwrap().join("\n");
        assert!(!done.contains("kept in") && std::fs::read_dir(&keep).unwrap().count() == 0, "{done}");
        assert!(!committed(&dir), "the upgrade changed what git holds");
    }

    /// A one-model `.vox`: voxels as x, y, z, colour index; colours for
    /// indices 1, 2, ...
    fn vox_bytes(size: [u32; 3], voxels: &[[u8; 4]], colours: &[[u8; 3]]) -> Vec<u8> {
        fn chunk(tag: &[u8], content: &[u8], children: &[u8]) -> Vec<u8> {
            let mut out = tag.to_vec();
            out.extend((content.len() as u32).to_le_bytes());
            out.extend((children.len() as u32).to_le_bytes());
            out.extend(content);
            out.extend(children);
            out
        }
        let size: Vec<u8> = size.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut xyzi = (voxels.len() as u32).to_le_bytes().to_vec();
        xyzi.extend(voxels.iter().flatten());
        let mut rgba = vec![0u8; 1024];
        for (i, c) in colours.iter().enumerate() {
            rgba[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let children = [chunk(b"SIZE", &size, &[]), chunk(b"XYZI", &xyzi, &[]), chunk(b"RGBA", &rgba, &[])].concat();
        [b"VOX ".to_vec(), 150u32.to_le_bytes().to_vec(), chunk(b"MAIN", &[], &children)].concat()
    }

    /// An older game's map and its prefabs become text: the prefab's
    /// colours in the order the map took them (a later colour first used
    /// before an earlier one), its markers named in it, `c.markers` out of
    /// `conf.lua`; the world built from the text is the one the old files
    /// made. A level of blocks as JSON becomes text and reads the same.
    #[test]
    fn levels_maps_and_worlds_become_text() {
        use glam::IVec3;
        let dir = crate::scratch::Scratch::new("upgrade-maps");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ground")).unwrap();
        crate::headless::write_png(&dir.join("ground/height.png"), 8, 8, &[3, 3, 3, 255].repeat(64)).unwrap();
        // A post along x of colours 5 then 2 (5 first used), two markers of
        // one kind the reading order would swap, one of another.
        let post = vox_bytes([3, 1, 3], &[[0, 0, 0, 5], [1, 0, 0, 2], [2, 0, 0, 2], [2, 0, 2, 240], [0, 0, 1, 240], [1, 0, 1, 241]], &[[1, 1, 1], [9, 9, 9], [0, 0, 0], [0, 0, 0], [200, 10, 10]]);
        std::fs::write(dir.join("post.vox"), post).unwrap();
        std::fs::write(dir.join("map.json"), r##"{ "size": [8, 8, 16], "ground": { "height": "ground/height.png", "materials": [{ "surface": ["#00ff00"] }] }, "prefabs": { "post": "post.vox" }, "place": [{ "prefab": "post", "at": [2, 1], "turn": 1 }, { "marker": "goal", "at": [6, 6] }] }"##).unwrap();
        let conf = "function conf(c)\n  c.voxcade = \"0.6.0\"\n  c.world.file = \"map.json\"\n  c.things.goal = {}\n  c.things.rifleman = {}\n  c.things.guard = {}\n\n  -- palette slots that place things\n  c.markers.rifleman = 240\n  c.markers.guard = 241\n\n  c.player.spawn = { 1, 1 }\nend\n";
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        std::fs::write(dir.join("main.lua"), "function on_world_start() end\n").unwrap();
        let Standing::Older { made: Some(_), steps } = standing(&dir).unwrap() else { panic!("not older") };
        assert_eq!(steps.iter().map(|s| s.name).collect::<Vec<_>>(), ["maps as text", "worlds and prefabs as text"]);
        let e = opens(&dir).unwrap_err().to_string();
        assert!(e.contains("Voxcade 0.6.0 reads this no more (maps as text, worlds and prefabs as text): ") && !e.contains("earlier"), "{e}");
        let done = upgrade(&dir, false).unwrap().join("\n");
        assert!(done.contains("map.json became map.toml") && done.contains("post.vox became post.vvx") && done.contains("conf.lua:9 taken out: c.markers.rifleman = 240"), "{done}");
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        assert!(conf.contains("c.world.file = \"map.toml\"") && !conf.contains("markers") && !conf.contains("\n\n\n"), "{conf}");
        let text = std::fs::read_to_string(dir.join("post.vvx")).unwrap();
        assert!(text.contains("a = \"#c80a0a\"\nb = \"#090909\"\nc = { marker = \"rifleman\" }\nd = { marker = \"rifleman\" }\ne = { marker = \"guard\" }"), "{text}");
        assert!(std::fs::read_to_string(dir.join("map.toml")).unwrap().contains("post = \"post.vvx\""));
        opens(&dir).unwrap();
        let mut cfg = crate::config::GameConfig::default();
        crate::lua::conf::run(&crate::files::Files::folder(&dir), &mut cfg).unwrap();
        let levels = crate::levels::load::<crate::Voxcade>(&crate::files::Files::folder(&dir), &cfg).unwrap();
        let level = &levels.list[0];
        let names = crate::sim::things::names(&cfg);
        let kinds: Vec<(&str, IVec3)> = level.markers.iter().map(|m| (names[m.kind], m.cell)).collect();
        // Turned a quarter about its pivot (1, 0, 0) standing on (2, 1, 3):
        // x runs along y.
        assert_eq!(kinds, [("rifleman", IVec3::new(2, 2, 5)), ("rifleman", IVec3::new(2, 0, 4)), ("guard", IVec3::new(2, 1, 4)), ("goal", IVec3::new(6, 6, 3))]);
        let w = &level.world;
        assert_eq!((w.palette[w.get(IVec3::new(2, 0, 3)) as usize], w.get(IVec3::new(2, 0, 3))), ([200, 10, 10, 255], 2), "the map's colors: the ground's first, then the post's in the order of first use");
        assert_eq!(w.palette[w.get(IVec3::new(2, 2, 3)) as usize], [9, 9, 9, 255]);

        // A level of blocks.
        let blocks = dir.join("blocks_game");
        std::fs::create_dir_all(blocks.join("levels")).unwrap();
        let mut lv = crate::grid::GridLevel::new(2, IVec3::new(3, 1, 2));
        lv.blocks.push(crate::grid::BlockEntry { name: "rock".into(), kind: crate::grid::Kind::Solid });
        lv.cells[1] = crate::grid::Cell { block: 1, turn: crate::blocks::Turn::MIRROR };
        lv.things.push(crate::grid::Thing::new("gem", [1, 1, 2]));
        let json = format!("{{ \"block\": 2, \"size\": [3, 1, 2], \"blocks\": [{{ \"name\": \"rock\", \"kind\": \"solid\" }}], \"cells\": [[1, 0, 0], [1, 1, {}], [4, 0, 0]], \"things\": [{{ \"kind\": \"gem\", \"at\": [1, 1, 2] }}] }}", crate::blocks::Turn::MIRROR.number());
        std::fs::write(blocks.join("levels/01_cave.json"), json).unwrap();
        std::fs::write(blocks.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\n  c.world.blocks = { file = \"levels/01_cave.json\" }\nend\n").unwrap();
        let done = upgrade(&blocks, false).unwrap().join("\n");
        assert!(done.contains("levels/01_cave.json became levels/01_cave.toml") && done.contains("conf.lua:3 now"), "{done}");
        assert_eq!(crate::grid::GridLevel::read(&blocks.join("levels/01_cave.toml")).unwrap(), lv);
        assert!(!blocks.join("levels/01_cave.json").exists());
        opens(&blocks).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod step_tests;

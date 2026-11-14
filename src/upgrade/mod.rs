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
    Step { since: "0.6.0", name: "no list of models", plan: listed::plan, apply: listed::apply, left: Some(listed::left) },
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
/// what an upgrade changes. A folder git ignores is not (its status says
/// nothing of it, and git holds none of it): its game's own files must be
/// tracked.
pub fn committed(dir: &Path) -> bool {
    let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(dir).args(args).stderr(std::process::Stdio::null()).output();
    let clean = git_status(dir).is_some_and(|changed| changed.is_empty());
    let own: Vec<&str> = ["conf.lua", "main.lua"].into_iter().filter(|f| dir.join(f).is_file()).collect();
    let tracked = || !own.is_empty() && git(&[&["ls-files", "--error-unmatch", "--"][..], &own].concat()).is_ok_and(|o| o.status.success());
    clean && tracked()
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
    for file in &game_files(dir) {
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
    let n = git_status(dir)?.lines().count();
    (n > 0).then_some(n)
}

/// What `git status` says changed or is new in the folder, a line each;
/// None outside a git repository (or without git).
fn git_status(dir: &Path) -> Option<String> {
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(["status", "--porcelain", "--", "."]).stderr(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
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

/// The game's files of an ending, under `dir` (`game_files`), as paths
/// from `dir`.
pub(crate) fn files_ending(dir: &Path, ending: &str) -> Vec<PathBuf> {
    let ends = |p: &PathBuf| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ending));
    game_files(dir).into_iter().filter(ends).map(|p| p.strip_prefix(dir).map(Path::to_path_buf).unwrap_or(p)).collect()
}

/// The game's files under `dir` as the upgrade sees them (not hidden ones,
/// not BUILD's `builds/` nor `shots/`), sorted folder by folder.
fn game_files(dir: &Path) -> Vec<PathBuf> {
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
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

/// A path from the game folder as the game's Lua names it (`/` between).
use crate::files::slashed as named;

/// Step 0.6.0: every model, figure and block of a game a `.vvx`
/// (`crate::vvx`), the `.vox` gone once the text reads back the same, the
/// Lua that names one in quotes naming the `.vvx`. Worlds and map prefabs
/// are left to the step "worlds and prefabs as text" (`worlds`): their
/// colours are numbered and their markers named there.
mod models;

/// Step 0.6.0: Voxcade plays no recordings (everything in a game is code
/// or text). A recording cannot be made into numbers, so the step only
/// says which sounds are recordings and what to do; it changes nothing
/// and deletes nothing (the game says the same at load until it is done).
mod recordings;

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
mod legacy;

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
mod scenes;

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
mod levels;

/// Step 0.6.0: a map is text (`map.toml`, "voxcade map 1", `map::MapData`),
/// no longer JSON: read as the engine read it before, written, read back
/// and found the same before the JSON goes. Its prefabs keep their names
/// here (`worlds` brings them over).
mod maps;

/// Step 0.6.0: worlds and map prefabs are `.vvx` too (`crate::scene`). A
/// world's voxels are numbers into its palette and markers were numbers
/// (`c.markers`): the text numbers the colours by the palette's order and
/// names each marker's kind in its entry, so the conversion writes the
/// palette so that every voxel keeps the number it had (a world: its file's
/// numbers in order; a prefab: its colours in the order the map took
/// them, their first use) and the markers so that the things come in the
/// order they came. Each is checked to read back the same before the `.vox`
/// goes; then the maps name the `.vvx`, and `c.markers` leaves `conf.lua`.
pub(crate) mod worlds;

/// Step 0.6.0: a game lists no models (the owner, 2026-11-14: "ich gebe
/// den ort an wo das model geladen wird - wieso muss es irgendwo
/// angemeldet werden?"). `frame:model` draws any model file of the game by
/// its path, loaded when first drawn, so `c.models` is no more: its lines
/// go out of `conf.lua`. A line that sets it (`c.models = { ... }` to its
/// closing brace, `c.models[...] = ...`, `table.insert(c.models, ...)`),
/// a loop that only filled it (its `for ... do` and its `end` at the same
/// indent), the comment that heads a block of them alone, and a blank
/// line left double; any other line naming `c.models` is said and left.
mod listed;

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
mod words;

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
mod tests;

#[cfg(test)]
mod step_tests;

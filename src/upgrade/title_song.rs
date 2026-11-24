//! "the title's song named" (0.7.0): the engine's title plays the song
//! `c.title_screen.music` names and is silent without one (the owner,
//! 2026-11-24: no song is a default). Before, it played the first song of
//! `c.audio.music` a player hears. For a game on the engine's title with
//! songs and no `c.title_screen.music`, the step writes that song into
//! `conf.lua`, right after the statement setting `c.audio.music` in
//! `conf(c)`, so the title sounds as before; where `conf.lua` has no such
//! statement of its own (the songs set in a loop, a table of `c.audio`
//! whole), it says the line to add and changes nothing.

use super::*;

/// The comment after the line the step writes.
const SAID: &str = "   -- the engine's title's song (unset, the title is silent)";

/// What the step finds: the song the title played, and where its line goes
/// (the line index after which it is put and its indent), if it can.
struct Found {
    song: String,
    after: Option<(usize, String)>,
}

/// The braces and parentheses a line opens less those it closes, outside
/// strings and its comment.
fn depth(line: &str) -> i32 {
    let (mut depth, mut quote, mut chars) = (0, None, line.chars().peekable());
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '-') if chars.peek() == Some(&'-') => break,
            (None, '{' | '(') => depth += 1,
            (None, '}' | ')') => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn indent(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Whether a line's code begins by setting `name` itself (`c.audio.music =`,
/// not `c.audio.music_volume`).
fn sets(line: &str, name: &str) -> bool {
    line.trim_start().strip_prefix(name).is_some_and(|rest| rest.trim_start().starts_with('=') && !rest.trim_start().starts_with("=="))
}

/// Where the line goes in `conf`: after the last line of the statement
/// setting `c.audio.music` at the level of `conf(c)`'s body, unless the
/// body sets `c.title_screen` whole after it (which would put the line out).
fn place(conf: &str) -> Option<(usize, String)> {
    let lines: Vec<&str> = conf.lines().collect();
    let head = lines.iter().position(|l| l.starts_with("function conf(") || l.starts_with("function conf ("))?;
    let end = (head + 1..lines.len()).find(|&i| lines[i].trim_end() == "end")?;
    let body = indent(lines[head + 1..end].iter().find(|l| !l.trim().is_empty())?).to_string();
    if body.is_empty() {
        return None;
    }
    let at = (head + 1..end).find(|&i| indent(lines[i]) == body && sets(lines[i], "c.audio.music"))?;
    let mut last = at;
    let mut open = depth(lines[at]);
    while open > 0 && last + 1 < end {
        last += 1;
        open += depth(lines[last]);
    }
    if open != 0 || (last + 1..end).any(|i| sets(lines[i], "c.title_screen")) {
        return None;
    }
    Some((last, body))
}

fn look(dir: &Path) -> Option<Found> {
    let files = crate::files::Files::folder(dir);
    let mut cfg = crate::config::GameConfig::default();
    crate::lua::conf::run(&files, &mut cfg).ok()?;
    if cfg.title_screen.level.is_some() || cfg.title_screen.music.is_some() {
        return None;
    }
    // The first song a player hears, as the session chose it: a file of
    // instruments (a studio) is none, a song that does not read still was.
    let heard = |f: &&str| match std::fs::read_to_string(dir.join(f)) {
        Ok(text) => match crate::audio::music::read(&text) {
            Ok(song) => !song.studio(),
            Err(_) => crate::audio::music::has_channels(&text),
        },
        Err(_) => true,
    };
    let song = cfg.audio.music.files().into_iter().find(heard)?.to_string();
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    Some(Found { song, after: place(&conf) })
}

fn line(found: &Found) -> String {
    format!("c.title_screen.music = \"{}\"", found.song)
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let Some(found) = look(dir) else { return Ok(Vec::new()) };
    let line = line(&found);
    Ok(vec![match &found.after {
        Some((at, _)) => format!("conf.lua:{}: the engine's title is silent now unless c.title_screen.music names its song; adds {line} after c.audio.music, so the title plays the song it played before", at + 2),
        None => format!("conf.lua: the engine's title is silent now unless c.title_screen.music names its song: add {line} to conf(c), after c.audio.music, for the song it played before (nothing is changed here)"),
    }])
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let said = plan(dir)?;
    let Some(found) = look(dir) else { return Ok(said) };
    let Some((at, indent)) = &found.after else { return Ok(said) };
    let path = dir.join("conf.lua");
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines: Vec<&str> = text.split_inclusive('\n').collect();
    let new = format!("{indent}{}{SAID}\n", line(&found));
    lines.insert(*at + 1, &new);
    std::fs::write(&path, lines.concat()).with_context(|| format!("writing {}", path.display()))?;
    Ok(said)
}

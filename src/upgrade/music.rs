//! "music from the code" (0.7.0): a game's music comes from its Lua code
//! alone (`music:play`). Before, a game that asked for no song heard the
//! player's (the first of `c.audio.music`, or the one SETTINGS chose), and
//! a title level of the game's own played `c.title_screen.music`. The step
//! adds the call where it safely can (a game whose code defines no
//! `on_level_start` and asks no song), so the game sounds as before, and
//! says in words what the author should add everywhere else; it takes
//! nothing of the game's code away.

use super::*;

/// The comment over the handler the step adds, by which it knows it.
const ADDED: &str = "-- Voxcade 0.7: music comes from the game's code alone (voxcade upgrade added this).";

/// What the step finds in a game.
struct Found {
    /// The song the player heard (the first of `c.audio.music` that is no
    /// file of instruments), by its name as `music:play` takes it.
    first: Option<String>,
    /// A title level of the game's own and the song and bar
    /// `c.title_screen.music` and `music_at` gave it.
    title: Option<(String, Option<String>, u32)>,
    /// The lines of `conf.lua` setting `c.title_screen.music` and `music_at`.
    title_lines: Vec<usize>,
    /// Whether the game's Lua asks a song anywhere, and where it calls
    /// `music:play()` without one (file, line).
    asks: bool,
    bare: Vec<(String, usize)>,
    /// Where the game defines `on_level_start` (file, line), if it does.
    handler: Option<(String, usize)>,
    /// Whether `on_world_start` or `on_level_start` asks a song (a song
    /// from the start; else the game was silent there until it asked one).
    asks_at_start: bool,
}

/// A song file's name as `music:play` takes it.
fn song_name(file: &str) -> String {
    Path::new(file).file_stem().map_or(file.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The code of a line, its comment cut off (well enough for these words:
/// a `--` in a string is rare in a call).
fn code(line: &str) -> &str {
    line.split("--").next().unwrap_or("")
}

fn look(dir: &Path) -> Option<Found> {
    let files = crate::files::Files::folder(dir);
    let mut cfg = crate::config::GameConfig::default();
    crate::lua::conf::run(&files, &mut cfg).ok()?;
    let songs = cfg.audio.music.files();
    let first = songs.iter().find(|f| std::fs::read_to_string(dir.join(f)).ok().and_then(|t| crate::audio::music::read(&t).ok()).is_some_and(|s| !s.studio())).map(|f| song_name(f));
    let title = cfg.title_screen.level.clone().map(|level| (level, cfg.title_screen.music.as_deref().map(|m| song_name(cfg.audio.music.file_of(m).unwrap_or(m))), cfg.title_screen.music_at));
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    let title_lines = conf.lines().enumerate().filter(|(_, l)| {
        let c = code(l).trim_start();
        c.starts_with("c.title_screen.music") && c["c.title_screen.music".len()..].trim_start().starts_with(['=', '_'])
    });
    let title_lines = title_lines.map(|(i, _)| i + 1).collect();
    let (mut asks, mut bare, mut handler, mut asks_at_start) = (false, Vec::new(), None, false);
    for lua in files_ending(dir, "lua") {
        let name = lua.display().to_string();
        if name == "conf.lua" {
            continue;
        }
        let text = std::fs::read_to_string(dir.join(&lua)).unwrap_or_default();
        // Inside a start handler: from its line to the next `end` that
        // begins a line.
        let mut in_start = false;
        for (i, line) in text.lines().enumerate() {
            let c = code(line);
            let head = c.trim_start();
            if head.starts_with("function on_level_start") || head.starts_with("function on_world_start") {
                in_start = true;
            } else if c.starts_with("end") {
                in_start = false;
            }
            asks_at_start |= in_start && c.contains("music:play(") && !c.contains("music:play()");
            for (at, _) in c.match_indices("music:play(") {
                let args = c[at + "music:play(".len()..].trim_start();
                if args.starts_with(')') {
                    bare.push((name.clone(), i + 1));
                } else {
                    asks = true;
                }
            }
            asks |= c.contains("music:play \"") || c.contains("music:play '");
            let defines = c.trim_start().starts_with("function on_level_start") || c.split_whitespace().collect::<String>().starts_with("on_level_start=");
            if defines && handler.is_none() {
                handler = Some((name.clone(), i + 1));
            }
        }
    }
    Some(Found { first, title, title_lines, asks, bare, handler, asks_at_start })
}

/// The call that plays what the game played before, in `on_level_start`:
/// on a title level of its own its title song, elsewhere the first song.
fn call(found: &Found) -> Option<String> {
    let play = |song: &str, at: u32| if at > 1 { format!("music:play(\"{song}\", {{ at = {at} }})") } else { format!("music:play(\"{song}\")") };
    let first = found.first.as_deref();
    match (&found.title, first) {
        (Some((level, Some(song), at)), first) => Some(match first {
            Some(f) => format!("if level.name == \"{level}\" then {} else {} end", play(song, *at), play(f, 1)),
            None => format!("if level.name == \"{level}\" then {} end", play(song, *at)),
        }),
        (_, Some(f)) => Some(play(f, 1)),
        (_, None) => None,
    }
}

/// Whether the step can add the call itself: songs to play, no song asked
/// in the code, no `on_level_start` of its own.
fn adds(found: &Found) -> bool {
    !found.asks && found.handler.is_none() && call(found).is_some()
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let Some(found) = look(dir) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    let Some(call) = call(&found) else { return Ok(out) };
    let title = found.title.as_ref().filter(|(_, song, _)| song.is_some());
    match (&found.handler, found.asks, found.asks_at_start) {
        (None, false, _) => out.push(format!("main.lua: music comes from the game's code alone now; adds on_level_start(how) with {call}, so the game sounds as before")),
        (Some((file, line)), false, _) => out.push(format!("{file}:{line}: music comes from the game's code alone now: add {call} to on_level_start, or the game plays no music (nothing is changed here)")),
        (handler, true, false) => {
            let at = handler.as_ref().map_or("main.lua: on_level_start".to_string(), |(file, line)| format!("{file}:{line}: on_level_start"));
            out.push(format!("{at}: music comes from the game's code alone now, and the game asks its songs only later: until then it is silent where the player's song played; {call} there keeps that (nothing is changed here)"));
        }
        (_, true, true) => {}
    }
    for (file, line) in &found.bare {
        let back = found.first.as_deref().map_or("a song of c.audio.music".to_string(), |f| format!("music:play(\"{f}\")"));
        out.push(format!("{file}:{line}: music:play() without a song gave the music back to the player's song, which is no more: name the song, {back} (nothing is changed here)"));
    }
    if let Some((level, Some(song), _)) = title {
        let lines: Vec<String> = found.title_lines.iter().map(|l| l.to_string()).collect();
        if adds(&found) {
            out.push(format!("conf.lua:{}: c.title_screen.music is the engine's title's; the title level {level} plays {song} from its code now (the handler above), so the line goes", lines.join(",")));
        } else {
            out.push(format!("conf.lua:{}: c.title_screen.music is the engine's title's: play {song} from the title level {level}'s code (music:play in its on_level_start) and take the line out; the game does not load until then (nothing is changed here)", lines.join(",")));
        }
    }
    Ok(out)
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let said = plan(dir)?;
    let Some(found) = look(dir) else { return Ok(said) };
    if !adds(&found) {
        return Ok(said);
    }
    let call = call(&found).unwrap_or_default();
    let path = dir.join("main.lua");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.contains(ADDED) {
        return Ok(Vec::new());
    }
    let sep = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
    std::fs::write(&path, format!("{text}{sep}{ADDED}\nfunction on_level_start(how)\n  {call}\nend\n")).with_context(|| format!("writing {}", path.display()))?;
    if found.title.as_ref().is_some_and(|(_, song, _)| song.is_some()) && !found.title_lines.is_empty() {
        let conf = dir.join("conf.lua");
        let text = std::fs::read_to_string(&conf)?;
        let kept: Vec<&str> = text.split_inclusive('\n').enumerate().filter(|(i, _)| !found.title_lines.contains(&(i + 1))).map(|(_, l)| l).collect();
        std::fs::write(&conf, kept.concat()).with_context(|| format!("writing {}", conf.display()))?;
    }
    Ok(said)
}

/// A game stating 0.7.0 (made while it was being made) still needs it
/// where the step would add the call itself; what it only says shows
/// there as the plain mistake of the load (`c.title_screen.music` with a
/// title of the game's own) or of the line (`music:play()`).
pub fn left(dir: &Path) -> bool {
    look(dir).is_some_and(|f| adds(&f))
}

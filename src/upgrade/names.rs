//! "names of 0.7" (0.7.0): names that read as something else, called as
//! they are meant (the owner, 2026-11-21). A hero's `hang`, `hang_speed`
//! and `air_jump` are `apex_gravity`, `apex_speed` and `air_jump_height`;
//! how near the player comes before a soldier or a spawner acts is
//! `wake`, as a gunship's; a gunship's `gun` is `muzzle`, as a soldier's;
//! a soldier's and `c.player`'s `landing` is `landing_height`;
//! `c.title_screen.drip` is `logo_drip`; `level.away` is
//! `level.ticks_away`. A kind's key is renamed by its behavior, found
//! through `like`: a spawner's `range` is `wake`, a soldier's and a
//! flyer's stay. What the step cannot place is named, not changed.

use std::collections::BTreeMap;
use std::ops::Range;

use super::words::{Change, Edits, Tok, edited, line_at, matches, paths, tokens};
use super::*;
use crate::lua::lex::{self, Kind};

/// What the step says once, before its lines.
const NOW: &str = "names that read as something else are called as they are meant since Voxcade 0.7.0: a hero's hang, hang_speed and air_jump are apex_gravity, apex_speed and air_jump_height; a soldier's active and a spawner's range are wake (as a gunship's); a gunship's gun is muzzle (as a soldier's); landing is landing_height; c.title_screen.drip is logo_drip; level.away is level.ticks_away";

/// A kind's keys by its behavior: the behavior, the old name, the new.
const KINDS: [(&str, &str, &str); 7] = [
    ("hero", "hang", "apex_gravity"),
    ("hero", "hang_speed", "apex_speed"),
    ("hero", "air_jump", "air_jump_height"),
    ("soldier", "active", "wake"),
    ("soldier", "landing", "landing_height"),
    ("spawner", "range", "wake"),
    ("gunship", "gun", "muzzle"),
];

/// Old names of `KINDS` other behaviors still have: renamed only where
/// the kind's behavior is known.
const SHARED: [&str; 1] = ["range"];

/// Settings by their path from `c`, and the new name.
const SETTINGS: [(&[&str], &str); 2] = [(&["player", "landing"], "landing_height"), (&["title_screen", "drip"], "logo_drip")];

/// A string token's text without its quotes.
fn unquoted<'a>(t: &Tok<'a>) -> Option<&'a str> {
    (t.kind == Kind::String).then(|| t.text.trim_matches(['"', '\'']))
}

/// Each kind's behavior as `conf.lua` writes it, through `like`.
fn behaviors(toks: &[Tok], keyed: &[(usize, Vec<&str>)]) -> BTreeMap<String, String> {
    let (mut own, mut like) = (BTreeMap::new(), BTreeMap::new());
    for (i, path) in keyed {
        let value = toks.get(i + 2).filter(|_| toks.get(i + 1).is_some_and(|t| t.text == "=")).and_then(unquoted);
        let (Some(value), true) = (value, matches(path, &["things", "*", "*"])) else { continue };
        match path[2] {
            "behavior" => own.insert(path[1].to_string(), value.to_string()),
            "like" => like.insert(path[1].to_string(), value.to_string()),
            _ => None,
        };
    }
    let mut out = BTreeMap::new();
    for name in own.keys().chain(like.keys()) {
        let mut at = name;
        // A chain of `like`s, at most as long as there are kinds.
        for _ in 0..=like.len() {
            if let Some(b) = own.get(at) {
                out.insert(name.clone(), b.clone());
                break;
            }
            let Some(next) = like.get(at) else { break };
            at = next;
        }
    }
    out
}

/// The token after the `}` that closes the `{` at `open`.
fn closing(toks: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (j, t) in toks.iter().enumerate().skip(open) {
        match t.text {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

/// A comment with `old = ` written `new = ` (the word alone, not inside
/// another).
fn comment_with(text: &str, old: &str, new: &str) -> String {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(old) {
        let before = rest[..at].chars().next_back().is_some_and(word);
        let after = &rest[at + old.len()..];
        if !before && after.trim_start().starts_with('=') && !after.trim_start().starts_with("==") {
            out.push_str(&rest[..at]);
            out.push_str(new);
        } else {
            out.push_str(&rest[..at + old.len()]);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// `conf.lua`'s edits, and what it leaves to the author.
fn conf_edits(src: &str) -> Edits {
    let toks = tokens(src);
    let keyed = paths(&toks);
    let behavior = behaviors(&toks, &keyed);
    let (mut edits, mut left) = (Vec::new(), Vec::new());
    let mut tables: Vec<(Range<usize>, &str)> = Vec::new();
    for (i, path) in &keyed {
        let key = toks[*i].text;
        if let Some((_, new)) = SETTINGS.iter().find(|(p, _)| matches(path, p)) {
            edits.push((toks[*i].range.clone(), new.to_string()));
            continue;
        }
        // A kind's table: its comments name its keys too.
        if matches(path, &["things", "*"])
            && let Some(b) = behavior.get(path[1])
            && toks.get(i + 1).is_some_and(|t| t.text == "=")
            && toks.get(i + 2).is_some_and(|t| t.text == "{")
            && let Some(close) = closing(&toks, i + 2)
        {
            tables.push((toks[i + 2].range.start..toks[close].range.end, b.as_str()));
        }
        let olds: Vec<&(&str, &str, &str)> = KINDS.iter().filter(|(_, old, _)| *old == key).collect();
        if olds.is_empty() {
            continue;
        }
        if matches(path, &["things", "*", "*"]) {
            match behavior.get(path[1]) {
                Some(b) => {
                    if let Some((_, _, new)) = olds.iter().find(|(of, _, _)| of == b) {
                        edits.push((toks[*i].range.clone(), new.to_string()));
                    }
                }
                None if !SHARED.contains(&key) => edits.push((toks[*i].range.clone(), olds[0].2.to_string())),
                None => left.push((toks[*i].range.start, format!("c.things.{}'s behavior is not written here: if it is a {}, its {key} is {} now", path[1], olds[0].0, olds[0].2))),
            }
        } else if path.contains(&"?") {
            let what = olds.iter().map(|(of, _, new)| format!("a {of}'s {key} is {new} now")).collect::<Vec<_>>().join(", ");
            left.push((toks[*i].range.start, format!("if this table is a kind's: {what}")));
        }
    }
    for s in lex::spans(src).into_iter().filter(|s| s.kind == Kind::Comment) {
        let Some((_, b)) = tables.iter().find(|(r, _)| r.contains(&s.range.start)) else { continue };
        let text = &src[s.range.clone()];
        let new = KINDS.iter().filter(|(of, _, _)| of == b).fold(text.to_string(), |t, (_, old, new)| comment_with(&t, old, new));
        if new != text {
            edits.push((s.range, new));
        }
    }
    (edits, left)
}

/// Any Lua file's edits: `level.away` is `level.ticks_away`; the level
/// read otherwise (another name for it, `level["away"]`) is named.
fn lua_edits(src: &str) -> Edits {
    let toks = tokens(src);
    let (mut edits, mut left) = (Vec::new(), Vec::new());
    let field = |i: usize| i == 0 || !matches!(toks[i - 1].text, "." | ":");
    // Names the level is given (`local here = level`).
    let mut aliases: Vec<&str> = Vec::new();
    for i in 2..toks.len() {
        if toks[i].kind == Kind::Name && toks[i].text == "level" && field(i) && toks[i - 1].text == "=" && toks[i - 2].kind == Kind::Name && !toks.get(i + 1).is_some_and(|t| matches!(t.text, "." | ":" | "[" | "(")) {
            aliases.push(toks[i - 2].text);
        }
    }
    for i in 0..toks.len() {
        if toks[i].kind != Kind::Name || !field(i) {
            continue;
        }
        let (dot, name) = (toks.get(i + 1).map(|t| t.text), toks.get(i + 2));
        let away = name.is_some_and(|t| t.kind == Kind::Name && t.text == "away");
        if toks[i].text == "level" && dot == Some(".") && away {
            edits.push((toks[i + 2].range.clone(), "ticks_away".to_string()));
        } else if toks[i].text == "level" && dot == Some("[") && name.and_then(unquoted) == Some("away") {
            left.push((toks[i].range.start, "level[\"away\"] is level.ticks_away now".to_string()));
        } else if aliases.contains(&toks[i].text) && dot == Some(".") && away {
            left.push((toks[i].range.start, format!("if {} is the level: level.away is level.ticks_away now", toks[i].text)));
        }
    }
    (edits, left)
}

/// A change with a line said for each line it changes.
fn said(file: &Path, before: &str, after: String) -> Change {
    let said = before.lines().zip(after.lines()).enumerate().filter(|(_, (a, b))| a != b).map(|(n, (_, b))| format!("{}:{} becomes: {}", named(file), n + 1, b.trim())).collect();
    Change { file: file.to_path_buf(), before: before.to_string(), after, said }
}

/// Every change the step would make to the game in `dir`, and the lines
/// it leaves to the author.
fn changes(dir: &Path) -> (Vec<Change>, Vec<String>) {
    let (mut out, mut left) = (Vec::new(), Vec::new());
    for p in files_ending(dir, "lua") {
        let Ok(src) = std::fs::read_to_string(dir.join(&p)) else { continue };
        let (mut edits, mut rest) = lua_edits(&src);
        if p == Path::new("conf.lua") {
            let (more, more_left) = conf_edits(&src);
            edits.extend(more);
            rest.extend(more_left);
        }
        rest.sort_by_key(|(at, _)| *at);
        left.extend(rest.into_iter().map(|(at, what)| format!("{}:{} is left as it is: {what}", named(&p), line_at(&src, at))));
        if !edits.is_empty() {
            out.push(said(&p, &src, edited(&src, edits)));
        }
    }
    (out, left)
}

pub fn left(dir: &Path) -> bool {
    !changes(dir).0.is_empty()
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let (changes, left) = changes(dir);
    if changes.is_empty() && left.is_empty() {
        return Ok(Vec::new());
    }
    Ok(std::iter::once(NOW.to_string()).chain(changes.into_iter().flat_map(|c| c.said)).chain(left).collect())
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let (changes, left) = changes(dir);
    if changes.is_empty() && left.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = vec![NOW.to_string()];
    for c in changes {
        let path = dir.join(&c.file);
        if std::fs::read_to_string(&path)? != c.before {
            bail!("{} changed while the upgrade read it; nothing written to it", named(&c.file));
        }
        crate::blocks::save_at_once(&path, c.after.as_bytes())?;
        out.extend(c.said.into_iter().map(|l| l.replacen(" becomes: ", " now: ", 1)));
    }
    out.extend(left);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "function conf(c)
  c.voxcade = \"0.7.0\"
  c.kit = \"shooter\"
  c.player.landing = 3          -- dust from this high
  c.title_screen.drip = 1       -- BLOOD bleeds
  c.things.rifleman = {
    behavior = \"soldier\",
    active = 440,           -- further away he waits
    range = 260,            -- shooting range
    landing = 2,
  }
  c.things.guard = { like = \"rifleman\", range = 300 }
  c.things.hut = {
    behavior = \"spawner\",
    range = 240,
  }
  c.things.shed = { like = \"hut\", range = 200 }
  c.things.gunship = {
    behavior = \"gunship\",
    gun = { 16, 4, 15 },    -- the door gunners' muzzles
    -- The Hind instead: gun = { 0, 38, 5 } (its chin gun)
  }
  c.things.hero = { behavior = \"hero\", hang = 0.4, hang_speed = 30, air_jumps = 1, air_jump = 18, hang_below = 2,
    frames = { hang = { 8, 8 } } }
  c.things.odd = { behavior = KIND, range = 9, active = 3 }
end
";

    const MAIN: &str = "function on_level_start(how)
  local lamp = level:thing(\"lamp\")
  lamp.state.oil = (lamp.state.oil or 10) - level.away   -- level.away in a comment stays
  local here = level
  if here.away > 0 or level[\"away\"] then end
  local s = { away = 1 }
  s.away = s.level.away
end
";

    /// Each name is renamed where it is certain, by the kind's behavior
    /// through `like` (a soldier's `range` stays, a spawner's and its
    /// copy's are `wake`), comments in a kind's table too; what it cannot
    /// place is named; the rest of each file is as it was, and a second
    /// run has nothing to do.
    #[test]
    fn the_names_of_0_7_are_renamed_where_certain() {
        let dir = crate::scratch::Scratch::new("names-of-0-7");
        std::fs::write(dir.join("conf.lua"), CONF).unwrap();
        std::fs::write(dir.join("main.lua"), MAIN).unwrap();
        let e = opens(&dir).unwrap_err().to_string();
        assert!(e.contains("names of 0.7") && e.contains("voxcade upgrade"), "{e}");
        let dry = upgrade(&dir, true).unwrap().join("\n");
        assert!(dry.contains("conf.lua:8 becomes: wake = 440,           -- further away he waits"), "{dry}");
        assert!(dry.contains("conf.lua:25 is left as it is: c.things.odd's behavior is not written here: if it is a spawner, its range is wake now"), "{dry}");
        assert!(dry.contains("main.lua:5 is left as it is: if here is the level"), "{dry}");
        assert!(dry.contains("main.lua:5 is left as it is: level[\"away\"] is level.ticks_away now"), "{dry}");
        upgrade(&dir, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("conf.lua")).unwrap(),
            "function conf(c)
  c.voxcade = \"0.7.0\"
  c.kit = \"shooter\"
  c.player.landing_height = 3          -- dust from this high
  c.title_screen.logo_drip = 1       -- BLOOD bleeds
  c.things.rifleman = {
    behavior = \"soldier\",
    wake = 440,           -- further away he waits
    range = 260,            -- shooting range
    landing_height = 2,
  }
  c.things.guard = { like = \"rifleman\", range = 300 }
  c.things.hut = {
    behavior = \"spawner\",
    wake = 240,
  }
  c.things.shed = { like = \"hut\", wake = 200 }
  c.things.gunship = {
    behavior = \"gunship\",
    muzzle = { 16, 4, 15 },    -- the door gunners' muzzles
    -- The Hind instead: muzzle = { 0, 38, 5 } (its chin gun)
  }
  c.things.hero = { behavior = \"hero\", apex_gravity = 0.4, apex_speed = 30, air_jumps = 1, air_jump_height = 18, hang_below = 2,
    frames = { hang = { 8, 8 } } }
  c.things.odd = { behavior = KIND, range = 9, wake = 3 }
end
"
        );
        assert_eq!(std::fs::read_to_string(dir.join("main.lua")).unwrap(), MAIN.replacen("- level.away", "- level.ticks_away", 1));
        // Only named lines are left: the game opens, a second run changes nothing.
        opens(&dir).unwrap();
        let again = upgrade(&dir, false).unwrap().join("\n");
        assert!(again.contains("nothing to do"), "{again}");
        assert_eq!(std::fs::read_to_string(dir.join("main.lua")).unwrap(), MAIN.replacen("- level.away", "- level.ticks_away", 1));
    }

    #[test]
    fn a_comment_names_a_key_only_as_a_word_set() {
        assert_eq!(comment_with("-- gun = { 0 }, its gun, shotgun = 1, gun == x", "gun", "muzzle"), "-- muzzle = { 0 }, its gun, shotgun = 1, gun == x");
    }
}

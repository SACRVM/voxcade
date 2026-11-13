//! Where the game's Lua names one of its files, read by its tokens
//! (`lua::lex`): a sound by its name where a sound is taken
//! (`level:sound`, a kind's `sound = ...`), a song by its path
//! (`c.audio.music`, `music = ...`) or its name (`music:play`), a model by
//! its path (`frame:model`, `level:effect`, `model = ...`), a Lua file by
//! `require`. The same name quoted where the API takes something else
//! (`frame:icon("coin")`, `icon = "coin"`) is no use of the file; quoted
//! where nothing tells (a game's own function, a local) it may be one:
//! RENAME lists those apart, unticked, and USED under MAYBE. Comments are
//! never uses.

use std::ops::Range;
use std::path::Path;

use crate::lua::lex::{self, Kind};

/// The sorts of file the game's Lua may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Lua,
    Scenario,
    Model,
    Sound,
    Song,
    /// Images and the rest: named by their path in quotes, if at all.
    Other,
}

impl Sort {
    /// The sort of file listed under the heading `group`.
    pub fn of(group: &str) -> Self {
        match group {
            "Logic" => Sort::Lua,
            "Scenarios" => Sort::Scenario,
            "Models" => Sort::Model,
            "Sounds" => Sort::Sound,
            "Music" => Sort::Song,
            _ => Sort::Other,
        }
    }
}

/// One place in a script that names the file (or may): the line as
/// written, and the quoted name's bytes in the file with what they become.
#[derive(Clone, Debug, PartialEq)]
pub struct Ref {
    pub file: String,
    pub line: usize,
    /// The line as written, trimmed.
    pub text: String,
    /// It names the file for certain (else only maybe: ticked off at first).
    pub sure: bool,
    /// The string token, its quotes included, and what it is written as
    /// once renamed.
    pub range: Range<usize>,
    pub old: String,
    pub new: String,
}

/// A text the file is named by, and where it counts.
struct Wanted {
    /// The name inside the quotes, and what it becomes.
    name: String,
    to: String,
    /// The calls (a method's or function's name, the argument from 0) and
    /// the keys (`sound = "..."`, `music = { ... }`) that take it.
    calls: &'static [(&'static str, usize)],
    keys: &'static [&'static str],
    /// A path with its ending: quoted anywhere else it is still the file
    /// (no other name looks like it); a bare name only maybe.
    whole: bool,
    /// The calls that take this sort of file in some spelling (a song's
    /// name in `music:play`): the other spelling there is no certain use,
    /// but not ruled out either.
    near: &'static [(&'static str, usize)],
}

/// The calls that take a file, by what they take (`(name, argument)`).
const SOUND_CALLS: &[(&str, usize)] = &[("sound", 0)];
const SONG_CALLS: &[(&str, usize)] = &[("play", 0)];
const MODEL_CALLS: &[(&str, usize)] = &[("model", 0), ("effect", 0)];
const MODULE_CALLS: &[(&str, usize)] = &[("require", 0)];
const PATH_CALLS: &[(&str, usize)] = &[("read", 0)];

/// The settings and fields that take a file.
const SOUND_KEYS: &[&str] = &["sound"];
const SONG_KEYS: &[&str] = &["music"];
const MODEL_KEYS: &[&str] = &["model", "models", "logo_model"];
const PATH_KEYS: &[&str] = &["file"];

/// Keys that name something else than a file (a quoted name there is not
/// the file's): icons, kinds, behaviors, actions, weapons, families.
const OTHER_KEYS: &[&str] = &["icon", "icons", "kind", "behavior", "family", "weapon", "action", "actions", "effect", "name", "text", "title", "pay"];

/// What the texts naming `from` (a path in the game folder, of `sort`)
/// look like, and what they become once it is `to`.
fn wanted(from: &str, to: &str, sort: Sort) -> Vec<Wanted> {
    let path = |calls, keys, near| Wanted { name: from.to_string(), to: to.to_string(), calls, keys, whole: true, near };
    match sort {
        Sort::Scenario => Vec::new(),
        Sort::Sound => match (super::sounds::name_of(from), super::sounds::name_of(to)) {
            (Some(a), Some(b)) => vec![Wanted { name: a.into(), to: b.into(), calls: SOUND_CALLS, keys: SOUND_KEYS, whole: false, near: SOUND_CALLS }],
            _ => Vec::new(),
        },
        // `require` names the file without its ending, dots for folders.
        Sort::Lua => {
            let module = |p: &str| p.strip_suffix(".lua").unwrap_or(p).replace('/', ".");
            vec![Wanted { name: module(from), to: module(to), calls: MODULE_CALLS, keys: &[], whole: false, near: MODULE_CALLS }]
        }
        // A song by its path in the settings, by its name in `music:play`.
        Sort::Song => {
            let mut out = vec![path(&[], SONG_KEYS, SONG_CALLS)];
            let stem = |p: &str| p.strip_prefix("music/").and_then(|n| n.strip_suffix(".mml")).filter(|n| !n.contains('/')).map(str::to_string);
            if let (Some(a), Some(b)) = (stem(from), stem(to)) {
                out.push(Wanted { name: a, to: b, calls: SONG_CALLS, keys: &[], whole: false, near: SONG_CALLS });
            }
            out
        }
        Sort::Model => vec![path(MODEL_CALLS, MODEL_KEYS, MODEL_CALLS)],
        Sort::Other => vec![path(PATH_CALLS, PATH_KEYS, PATH_CALLS)],
    }
}

/// Where a string token stands, as far as its name says what it is.
#[derive(Clone, Debug, PartialEq)]
enum Place {
    /// An argument of a call (the called name, the argument from 0).
    Call(String, usize),
    /// The value of a key or a variable (`sound = "coin"`).
    Key(String),
    /// Anywhere else (a comparison, a return, a table's list).
    Loose,
}

/// What a quoted name in `place` is to `w`: Some(true) the file, Some(false)
/// maybe, None not the file.
fn judge(w: &Wanted, place: &Place) -> Option<bool> {
    let fallback = Some(w.whole);
    match place {
        Place::Call(name, arg) => {
            if w.calls.iter().any(|(n, a)| n == name && a == arg) {
                Some(true)
            } else if is_api_call(name) && !w.near.iter().any(|(n, a)| n == name && a == arg) {
                // The API's own call, taking something else there (an icon,
                // a kind, another sort of file).
                None
            } else {
                fallback
            }
        }
        Place::Key(key) => {
            if w.keys.contains(&key.as_str()) {
                Some(true)
            } else if OTHER_KEYS.contains(&key.as_str()) || [SOUND_KEYS, SONG_KEYS, MODEL_KEYS, PATH_KEYS].iter().any(|k| k.contains(&key.as_str())) {
                None
            } else {
                fallback
            }
        }
        Place::Loose => fallback,
    }
}

/// Whether `name` is a method or function of the API (`frame:icon`,
/// `require`), whose arguments the register names.
fn is_api_call(name: &str) -> bool {
    use crate::lua::api::{API, What};
    API.iter().any(|i| i.name == name && matches!(i.what, What::Method { .. } | What::Function { .. }))
}

/// An open bracket: `(` of a call (or not), `{` and `[`, with the place of
/// what it opens and how many commas it holds so far.
struct Open {
    bracket: u8,
    place: Place,
    commas: usize,
}

/// Every quoted name in `src` that names (or may name) the file, as
/// `wanted` has it.
fn scan(file: &str, src: &str, wanted: &[Wanted]) -> Vec<Ref> {
    let spans: Vec<lex::Span> = lex::spans(src).into_iter().filter(|s| s.kind != Kind::Comment).collect();
    let text = |i: usize| &src[spans[i].range.clone()];
    let mut stack: Vec<Open> = Vec::new();
    let mut out = Vec::new();
    for i in 0..spans.len() {
        let s = &spans[i];
        match (s.kind, text(i)) {
            (Kind::Symbol, "(") => {
                let place = match i.checked_sub(1) {
                    Some(p) if spans[p].kind == Kind::Name => Place::Call(text(p).to_string(), 0),
                    _ => stack.last().map_or(Place::Loose, |o| o.place.clone()),
                };
                stack.push(Open { bracket: b'(', place, commas: 0 });
            }
            (Kind::Symbol, "{") => {
                let place = place_before(&spans, src, i, &stack);
                stack.push(Open { bracket: b'{', place, commas: 0 });
            }
            (Kind::Symbol, "[") => stack.push(Open { bracket: b'[', place: Place::Loose, commas: 0 }),
            (Kind::Symbol, ")" | "}" | "]") => {
                stack.pop();
            }
            (Kind::Symbol, ",") => {
                if let Some(o) = stack.last_mut() {
                    o.commas += 1;
                    if let Place::Call(_, arg) = &mut o.place
                        && o.bracket == b'('
                    {
                        *arg = o.commas;
                    }
                }
            }
            (Kind::String, token) if !s.open && token.len() >= 2 && (token.starts_with('"') || token.starts_with('\'')) => {
                let inside = &token[1..token.len() - 1];
                let Some(w) = wanted.iter().find(|w| w.name == inside) else { continue };
                let place = match i.checked_sub(1) {
                    // `require "x"`: a call without brackets.
                    Some(p) if spans[p].kind == Kind::Name => Place::Call(text(p).to_string(), 0),
                    _ => place_before(&spans, src, i, &stack),
                };
                let Some(sure) = judge(w, &place) else { continue };
                let quote = &token[..1];
                let line = src[..s.range.start].matches('\n').count() + 1;
                let whole = src.lines().nth(line - 1).unwrap_or("").trim().to_string();
                out.push(Ref { file: file.to_string(), line, text: whole, sure, range: s.range.clone(), old: token.to_string(), new: format!("{quote}{}{quote}", w.to) });
            }
            _ => {}
        }
    }
    out
}

/// The place of the token `i` by what stands before it: `key =` its key,
/// else the place of the bracket it is in (an argument of a call, an
/// element of a list given to a key), else loose.
fn place_before(spans: &[lex::Span], src: &str, i: usize, stack: &[Open]) -> Place {
    let text = |k: usize| &src[spans[k].range.clone()];
    if i >= 2 && text(i - 1) == "=" && spans[i - 2].kind == Kind::Name {
        return Place::Key(text(i - 2).to_string());
    }
    let after_open = i == 0 || matches!(text(i - 1), "(" | "{" | "," | ";");
    match stack.last() {
        // A list's element takes the place of the list (`music = { "a.mml" }`).
        Some(o) if o.bracket == b'{' && after_open => o.place.clone(),
        Some(o) if o.bracket == b'(' => o.place.clone(),
        _ => Place::Loose,
    }
}

/// Every place in `scripts` (paths in `dir`) that names `from`, of `sort`,
/// and what it becomes once the file is `to`: the certain ones first in
/// each file's order, then the maybes.
pub fn find(dir: &Path, scripts: &[String], from: &str, to: &str, sort: Sort) -> Vec<Ref> {
    let wanted = wanted(from, to, sort);
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for file in scripts.iter().filter(|f| f.ends_with(".lua")) {
        let Ok(src) = std::fs::read_to_string(dir.join(file)) else { continue };
        out.extend(scan(file, &src, &wanted));
    }
    out.sort_by_key(|r| !r.sure);
    out
}

/// The places `refs` changed in their files (each saved whole at once),
/// those whose file no longer holds the old name there left out; the files
/// that changed.
pub fn rewrite(dir: &Path, refs: &[&Ref]) -> anyhow::Result<Vec<String>> {
    let mut files: Vec<&str> = refs.iter().map(|r| r.file.as_str()).collect();
    files.sort_unstable();
    files.dedup();
    let mut changed = Vec::new();
    for file in files {
        let Ok(mut text) = std::fs::read_to_string(dir.join(file)) else { continue };
        let mut here: Vec<&&Ref> = refs.iter().filter(|r| r.file == file).collect();
        // From the end, so the ranges before stay where they are.
        here.sort_by_key(|r| std::cmp::Reverse(r.range.start));
        let before = text.clone();
        for r in here {
            if text.get(r.range.clone()) == Some(r.old.as_str()) {
                text.replace_range(r.range.clone(), &r.new);
            }
        }
        if text != before {
            crate::blocks::save_at_once(&dir.join(file), text.as_bytes())?;
            changed.push(file.to_string());
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(src: &str, from: &str, sort: Sort) -> Vec<(usize, bool)> {
        scan("main.lua", src, &wanted(from, from, sort)).iter().map(|r| (r.line, r.sure)).collect()
    }

    /// BunnyFarm's coin: a sound and an icon of one name. Renaming the
    /// sound finds `level:sound` (inside `and ... or` too) for certain,
    /// the game's own `fx.fly` as a maybe, and leaves the icon alone
    /// (`frame:icon`, `icon = "coin"`); a comment is no use.
    #[test]
    fn a_sound_is_told_from_an_icon_of_the_same_name() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/bunnyfarm");
        let scripts: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".lua")).collect();
        let refs = find(&dir, &scripts, "sounds/coin.toml", "sounds/ring.toml", Sort::Sound);
        let at = |file: &str, line: usize| refs.iter().find(|r| r.file == file && r.line == line).map(|r| r.sure);
        assert_eq!(at("main.lua", 220), Some(true), "level:sound(\"coin\", player)");
        assert_eq!(at("main.lua", 292), Some(true), "level:sound(... and \"coin\" or \"error\", player)");
        assert_eq!(at("mail.lua", 174), Some(true));
        assert_eq!(at("main.lua", 225), Some(false), "fx.fly(\"coin\", ...): the game's own, maybe");
        assert_eq!(at("hud.lua", 138), None, "frame:icon(\"coin\", ...)");
        assert_eq!(at("hud.lua", 508), None, "frame:icon(r.pay or \"coin\", ...)");
        assert_eq!(at("data.lua", 174), None, "icon = \"coin\"");
        assert!(refs.iter().all(|r| r.new == r.old.replace("coin", "ring")));
        // The certain ones first.
        assert!(refs.iter().skip_while(|r| r.sure).all(|r| !r.sure));
    }

    #[test]
    fn each_sort_is_named_where_it_is_taken() {
        let src = "local t = require(\"tools\")\nlocal u = require 'tools'\nx = \"tools\" -- \"tools\"\nc.things.gem = { sound = 'coin', icon = 'coin' }\nlevel:sound('coin', p)\nframe:model(\"hero.vvx\", p)\nskins = { \"hero.vvx\" }\nc.audio.music = { \"music/storm.mml\" }\nmusic:play(\"storm\")\nif s == \"storm\" then end\n";
        assert_eq!(found(src, "tools.lua", Sort::Lua), [(1, true), (2, true), (3, false)]);
        assert_eq!(found(src, "sounds/coin.toml", Sort::Sound), [(4, true), (5, true)]);
        assert_eq!(found(src, "hero.vvx", Sort::Model), [(6, true), (7, true)]);
        assert_eq!(found(src, "music/storm.mml", Sort::Song), [(8, true), (9, true), (10, false)]);
        assert!(found(src, "scenarios/late.lua", Sort::Scenario).is_empty());
    }

    /// Only the places chosen change, and only where the file still holds
    /// the old name.
    #[test]
    fn only_the_places_chosen_are_rewritten() {
        let dir = crate::scratch::Scratch::new("refs-rewrite");
        std::fs::write(dir.join("main.lua"), "level:sound(\"coin\", p)\nfx.fly(\"coin\")\nframe:icon(\"coin\", 1, 2)\n").unwrap();
        let scripts = ["main.lua".to_string()];
        let refs = find(&dir, &scripts, "sounds/coin.toml", "sounds/ring.toml", Sort::Sound);
        assert_eq!(refs.iter().map(|r| (r.line, r.sure)).collect::<Vec<_>>(), [(1, true), (2, false)]);
        let chosen: Vec<&Ref> = refs.iter().filter(|r| r.sure).collect();
        assert_eq!(rewrite(&dir, &chosen).unwrap(), ["main.lua"]);
        assert_eq!(std::fs::read_to_string(dir.join("main.lua")).unwrap(), "level:sound(\"ring\", p)\nfx.fly(\"coin\")\nframe:icon(\"coin\", 1, 2)\n");
        // Asked again on a file changed meanwhile: nothing written.
        assert!(rewrite(&dir, &chosen).unwrap().is_empty());
    }
}

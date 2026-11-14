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

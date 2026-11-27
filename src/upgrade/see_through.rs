//! "see-through colors on the HUD" (0.7.0): a fourth value of a color given
//! to `frame:text`, `frame:box`, `frame:bar` and `frame:drop_in` was dropped
//! in 0.6 and the color drawn solid; since 0.7 it is see-through, as the API
//! always said (so is a letter of `c.icons`' colors). The step names each
//! literal color with a fourth value below 255 (a table of four numbers, or
//! "#rrggbbaa") by file and line; colors worked out in code it cannot
//! follow. It changes nothing.

use super::words::{Tok, line_at, paths, tokens};
use super::*;
use crate::lua::lex::Kind;

/// What a color argument is.
enum Color {
    /// Nothing to say: solid, no literal, or not a color.
    Solid,
    /// A literal with a fourth value below 255.
    Through,
    /// Worked out in code (a name, a call, a sum).
    Worked,
}

/// The color the tokens `from..to` (one argument or table value) write.
fn classify(toks: &[Tok], from: usize, to: usize) -> Color {
    let part = &toks[from..to];
    match part {
        [] => Color::Solid,
        [one] if one.kind == Kind::String => match one.text.get(1..one.text.len().saturating_sub(1)).and_then(crate::color::hex) {
            Some([.., a]) if a < 255 => Color::Through,
            _ => Color::Solid,
        },
        [one] if one.text == "nil" => Color::Solid,
        [open, inner @ .., close] if open.text == "{" && close.text == "}" => {
            let mut numbers = Vec::new();
            for (n, t) in inner.iter().enumerate() {
                match (n % 2, t.kind, t.text) {
                    (0, Kind::Number, text) => numbers.push(text.parse::<f64>().ok()),
                    (1, _, "," | ";") => {}
                    _ => return Color::Worked,
                }
            }
            match numbers.as_slice() {
                [_, _, _, Some(a)] if *a < 255.0 => Color::Through,
                [_, _, _, None] => Color::Worked,
                _ => Color::Solid,
            }
        }
        _ => Color::Worked,
    }
}

/// Where a value starting at token `from` ends: at a comma, a semicolon or
/// the bracket closing what holds it, beyond the brackets it opens itself.
fn value_end(toks: &[Tok], from: usize) -> usize {
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate().skip(from) {
        match (t.kind, t.text) {
            (Kind::Symbol, "(" | "{" | "[") => depth += 1,
            (Kind::Symbol, ")" | "}" | "]") if depth == 0 => return i,
            (Kind::Symbol, ")" | "}" | "]") => depth -= 1,
            (Kind::Symbol, "," | ";") if depth == 0 => return i,
            _ => {}
        }
    }
    toks.len()
}

/// The source text of tokens `from..to`.
fn written(src: &str, toks: &[Tok], from: usize, to: usize) -> String {
    src[toks[from].range.start..toks[to - 1].range.end].split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What the step says of a game's colors: each literal with a fourth value
/// below 255 given to the HUD's calls and to `c.icons`, and once that
/// colors given through variables are not looked at. Nothing for a game
/// without such a literal.
pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let (mut out, mut worked) = (Vec::new(), false);
    // The argument that is the color of each call (bar: its back as well).
    let colors: [(&str, &[usize]); 4] = [("text", &[3]), ("box", &[4]), ("bar", &[5, 6]), ("drop_in", &[4])];
    for lua in files_ending(dir, "lua") {
        let src = std::fs::read_to_string(dir.join(&lua)).unwrap_or_default();
        let toks = tokens(&src);
        let file = named(&lua);
        for i in 1..toks.len().saturating_sub(2) {
            let Some((call, at)) = colors.iter().find(|(c, _)| toks[i].text == *c && toks[i - 1].text == ":" && toks[i + 1].text == "(").map(|(c, at)| (*c, *at)) else { continue };
            // The call's arguments, split at the commas outside brackets.
            let mut args = Vec::new();
            let mut from = i + 2;
            while from < toks.len() && toks[from].text != ")" {
                let to = value_end(&toks, from);
                args.push((from, to));
                from = to + 1;
            }
            for &n in at {
                let Some(&(from, to)) = args.get(n) else { continue };
                match classify(&toks, from, to) {
                    Color::Through => out.push(format!("{file}:{}: frame:{call}: {} is drawn see-through now, was solid: drop its fourth value to keep the old look", line_at(&src, toks[from].range.start), written(&src, &toks, from, to))),
                    Color::Worked => worked = true,
                    Color::Solid => {}
                }
            }
        }
    }
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    let toks = tokens(&conf);
    for (i, path) in paths(&toks) {
        if !(path.len() == 3 && path[0] == "icons" && path[2] == "colors" && toks[i].text == "colors" && toks.get(i + 1).map(|t| t.text) == Some("=") && toks.get(i + 2).map(|t| t.text) == Some("{")) {
            continue;
        }
        // Each letter: `a = { ... }` or `["a"] = { ... }`.
        let mut j = i + 3;
        while j < toks.len() && toks[j].text != "}" {
            let (key, eq) = if toks[j].text == "[" { (toks.get(j + 1), j + 3) } else { (toks.get(j), j + 1) };
            if toks.get(eq).map(|t| t.text) != Some("=") {
                j = value_end(&toks, j) + 1;
                continue;
            }
            let to = value_end(&toks, eq + 1);
            let letter = key.map_or("?", |k| k.text.trim_matches(['"', '\'']));
            match classify(&toks, eq + 1, to) {
                Color::Through => out.push(format!("conf.lua:{}: c.icons.{}.colors {letter} = {} is drawn see-through now, was solid: drop its fourth value to keep the old look", line_at(&conf, toks[eq + 1].range.start), path[1], written(&conf, &toks, eq + 1, to))),
                Color::Worked => worked = true,
                Color::Solid => {}
            }
            j = to + 1;
        }
    }
    if !out.is_empty() {
        if worked {
            out.push("colors given through variables are not looked at: a fourth value in one is see-through now too".into());
        }
        out.push("a fourth value of a color is see-through since Voxcade 0.7, as the API always said (in 0.6 the HUD's calls and c.icons dropped it and drew the color solid); nothing here is changed".into());
    }
    Ok(out)
}

//! "colors from 0 to 255" (0.7.0): a color is "#rrggbb" or { r, g, b }
//! with numbers from 0 to 255 everywhere (`color::read`). `conf.lua`'s
//! colors of `c.light` and `c.effects` and a level's `[light]` were three
//! numbers from 0 to 1: each such table is written on the new scale, so
//! that the picture stays the same to the bit (`color::on_255`), as
//! "#rrggbb" where its numbers come out whole. A fade's or a logo's color
//! written without its `#` gets it (they were taken so, and are refused
//! now). A color worked out in code is named, not changed: the step cannot
//! know its numbers.

use toml_edit::{Document, Item};

use super::words::{Change, Edits, edited, format_of, line_at, matches, paths, tokens};
use super::*;
use crate::lua::lex::Kind;

/// What the step says once, before its lines.
const NOW: &str = "a color's numbers are from 0 to 255 since Voxcade 0.7.0 (c.light's and c.effects' colors and a level's [light] were from 0 to 1): written on the new scale so the picture stays the same, as \"#rrggbb\" where the numbers are whole";

/// The new text of a color table on the scale from 0 to 1 (`numbers` as
/// written), or None when it is no such table.
fn rewritten(numbers: &[f64]) -> Option<String> {
    if numbers.len() != 3 || !crate::color::old_scale(numbers) {
        return None;
    }
    // As the settings held them: each number as an f32, read on the new
    // scale back to that f32 (`color::on_255`).
    let n: Vec<f64> = numbers.iter().map(|v| crate::color::on_255(*v as f32)).collect();
    let color = crate::color::Color { rgba: [n[0], n[1], n[2], 255.0] };
    Some(if color.whole() { format!("\"{}\"", color.hex()) } else { format!("{{ {}, {}, {} }}", n[0], n[1], n[2]) })
}

/// A six-digit hex color without its `#` (`"000000"`), quotes included:
/// the text with it.
fn with_hash(token: &str) -> Option<String> {
    let quote = token.chars().next().filter(|q| *q == '"' || *q == '\'')?;
    let inner = token.strip_prefix(quote)?.strip_suffix(quote)?;
    (inner.len() == 6 && inner.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("{quote}#{inner}{quote}"))
}

/// `conf.lua`'s edits, and what it leaves to the author (its byte, what to
/// write).
fn conf_edits(src: &str) -> Edits {
    let toks = tokens(src);
    let (mut edits, mut left) = (Vec::new(), Vec::new());
    for (i, path) in paths(&toks) {
        if toks.get(i + 1).map(|t| t.text) != Some("=") {
            continue;
        }
        let Some(value) = toks.get(i + 2) else { continue };
        if let Some((section, key)) = crate::config::COLORS.iter().find(|(s, k)| matches(&path, &[*s, *k])) {
            match value.text {
                "{" => {
                    // { number, number, number }, a comma after the last too.
                    let mut numbers = Vec::new();
                    let mut j = i + 3;
                    let mut plain = true;
                    while let Some(t) = toks.get(j) {
                        match (t.kind, t.text) {
                            (_, "}") => break,
                            (Kind::Number, text) => match text.parse::<f64>() {
                                Ok(v) => numbers.push(v),
                                Err(_) => plain = false,
                            },
                            (_, "," | ";") => {}
                            _ => plain = false,
                        }
                        j += 1;
                    }
                    match (plain, toks.get(j)) {
                        (true, Some(close)) => {
                            if let Some(new) = rewritten(&numbers) {
                                edits.push((value.range.start..close.range.end, new));
                            }
                        }
                        _ => left.push((toks[i].range.start, format!("c.{section}.{key} is worked out in code: a color's numbers are from 0 to 255 now (255 times what they were), or \"#rrggbb\""))),
                    }
                }
                _ if value.kind == Kind::String => {}
                _ => left.push((toks[i].range.start, format!("c.{section}.{key} is set from code: a color's numbers are from 0 to 255 now (255 times what they were), or \"#rrggbb\""))),
            }
        }
        // A fade's or a logo's color without its #.
        let strings: Vec<usize> = if matches(&path, &["transition", "color"]) {
            vec![i + 2]
        } else if matches(&path, &["title_screen", "logo_colors"]) && value.text == "{" {
            (i + 3..toks.len()).take_while(|&j| toks[j].text != "}").filter(|&j| toks[j].kind == Kind::String).collect()
        } else {
            Vec::new()
        };
        for j in strings {
            let Some(t) = toks.get(j).filter(|t| t.kind == Kind::String) else { continue };
            match with_hash(t.text) {
                Some(new) => edits.push((t.range.clone(), new)),
                None if crate::color::hex(t.text.trim_matches(['"', '\''])).is_none() => {
                    left.push((t.range.start, format!("{} is no color (it was taken as black or white without a word): write the color meant, \"#rrggbb\"", t.text)))
                }
                None => {}
            }
        }
    }
    (edits, left)
}

/// A level's file with its `[light]` colors on the new scale.
fn level_file(file: &Path, src: &str) -> Result<Option<Change>> {
    if format_of(src) != Some("level") {
        return Ok(None);
    }
    let doc = Document::parse(src).map_err(|e| anyhow::anyhow!("{}: {}", named(file), e.message()))?;
    let Some(light) = doc.as_table().get("light").and_then(Item::as_table_like) else { return Ok(None) };
    let mut edits = Vec::new();
    for key in ["sun_color", "sky"] {
        let Some(item) = light.get(key) else { continue };
        let Some(numbers) = item.as_array().and_then(|a| a.iter().map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))).collect::<Option<Vec<f64>>>()) else { continue };
        if numbers.len() != 3 || !crate::color::old_scale(&numbers) {
            continue;
        }
        // As the level's reader held it: rounded into "#rrggbb".
        let color = crate::color::Color { rgba: [numbers[0] * 255.0, numbers[1] * 255.0, numbers[2] * 255.0, 255.0] };
        let Some(span) = item.span() else { continue };
        edits.push((span, format!("\"{}\"", color.hex())));
    }
    if edits.is_empty() {
        return Ok(None);
    }
    let after = edited(src, edits);
    crate::grid::GridLevel::from_text(&after).with_context(|| format!("{} with its colors on the new scale does not read", named(file)))?;
    Ok(Some(said(file, src, after)))
}

/// A change with a line said for each line it changes.
fn said(file: &Path, before: &str, after: String) -> Change {
    let said = before.lines().zip(after.lines()).enumerate().filter(|(_, (a, b))| a != b).map(|(n, (_, b))| format!("{}:{} becomes: {}", named(file), n + 1, b.trim())).collect();
    Change { file: file.to_path_buf(), before: before.to_string(), after, said }
}

/// Every change the step would make to the game in `dir`, and the lines
/// it leaves to the author.
fn changes(dir: &Path) -> Result<(Vec<Change>, Vec<String>)> {
    let mut out = Vec::new();
    let mut left = Vec::new();
    if let Ok(src) = std::fs::read_to_string(dir.join("conf.lua")) {
        let (edits, rest) = conf_edits(&src);
        for (at, what) in rest {
            left.push(format!("conf.lua:{} is left as it is: {what}", line_at(&src, at)));
        }
        if !edits.is_empty() {
            out.push(said(Path::new("conf.lua"), &src, edited(&src, edits)));
        }
    }
    for p in files_ending(dir, "toml") {
        let src = std::fs::read_to_string(dir.join(&p)).unwrap_or_default();
        out.extend(level_file(&p, &src)?);
    }
    Ok((out, left))
}

pub fn left(dir: &Path) -> bool {
    changes(dir).is_ok_and(|(c, l)| !c.is_empty() || !l.is_empty())
}

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let (changes, left) = changes(dir)?;
    if changes.is_empty() && left.is_empty() {
        return Ok(Vec::new());
    }
    Ok(std::iter::once(NOW.to_string()).chain(changes.into_iter().flat_map(|c| c.said)).chain(left).collect())
}

pub fn apply(dir: &Path) -> Result<Vec<String>> {
    let (changes, left) = changes(dir)?;
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

    /// A table on the scale from 0 to 1 is written so it reads back to
    /// the same f32 as it was: hex where whole, the fewest decimals else.
    #[test]
    fn a_table_from_0_to_1_is_written_to_read_back_the_same() {
        assert_eq!(rewritten(&[0.06, 0.07, 0.1]).unwrap(), "{ 15.3, 17.85, 25.5 }");
        assert_eq!(rewritten(&[1.0, 1.0, 1.0]).unwrap(), "\"#ffffff\"");
        assert_eq!(rewritten(&[0.2, 0.4, 1.0]).unwrap(), "\"#3366ff\"");
        assert_eq!(rewritten(&[0.0, 0.0, 0.0]), None, "black is black either way");
        assert_eq!(rewritten(&[15.3, 17.85, 25.5]), None, "on the new scale already");
        for numbers in [[0.06, 0.07, 0.1], [0.45, 0.62, 0.85], [0.86, 0.78, 0.62], [0.3, 0.32, 0.42], [0.01, 0.01, 0.03]] {
            let text = rewritten(&numbers).unwrap();
            let read: Vec<f64> = text.trim_matches(['{', '}', ' ']).split(',').map(|n| n.trim().parse().unwrap()).collect();
            let unit: Vec<f32> = read.iter().map(|v| crate::color::unit(*v)).collect();
            assert_eq!(unit, numbers.map(|v| v as f32), "{text}");
        }
        assert_eq!(with_hash("\"000000\"").unwrap(), "\"#000000\"");
        assert_eq!(with_hash("'#000000'"), None);
        assert_eq!(with_hash("\"orange\""), None);
    }

    /// A game already stating 0.7.0 with colors from 0 to 1 (the step came
    /// while 0.7.0 was being made) does not open, says why, and the upgrade
    /// writes them on the new scale, the rest of each file as it was; a
    /// color worked out in code is named and left; a second run has
    /// nothing to do.
    #[test]
    fn a_game_of_this_version_gets_its_colors_on_the_new_scale() {
        let dir = crate::scratch::Scratch::new("colors-now");
        let conf = "function conf(c)\n  c.voxcade = \"0.7.0\"\n  c.light.sky = { 0.06, 0.07, 0.10 }   -- night\n  c.light = { sun_color = { 1, 1, 1 }, fog_color = { 0, 0, 0 } }\n  c.effects.outline_color = DARK\n  c.title_screen.logo_colors = { \"#f0e8d0\", \"c8181a\" }\n  c.icons.heart = { rows = { \"R\" }, colors = { R = { 214, 36, 44 } } }\nend\n";
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::write(dir.join("levels/hall.toml"), "format = \"voxcade level 1\"\nsize = [32, 32, 16]\n\n[light]\nsky = [0.125, 0.1875, 0.29]   # dusk\nsun_color = [0, 0, 0]\n").unwrap();
        let e = opens(&dir).unwrap_err().to_string();
        assert!(e.contains("colors from 0 to 255") && e.contains("voxcade upgrade"), "{e}");
        let dry = upgrade(&dir, true).unwrap().join("\n");
        assert!(dry.contains("conf.lua:3 becomes: c.light.sky = { 15.3, 17.85, 25.5 }   -- night"), "{dry}");
        assert!(dry.contains("conf.lua:5 is left as it is: c.effects.outline_color is set from code"), "{dry}");
        upgrade(&dir, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("conf.lua")).unwrap(),
            "function conf(c)\n  c.voxcade = \"0.7.0\"\n  c.light.sky = { 15.3, 17.85, 25.5 }   -- night\n  c.light = { sun_color = \"#ffffff\", fog_color = { 0, 0, 0 } }\n  c.effects.outline_color = DARK\n  c.title_screen.logo_colors = { \"#f0e8d0\", \"#c8181a\" }\n  c.icons.heart = { rows = { \"R\" }, colors = { R = { 214, 36, 44 } } }\nend\n"
        );
        assert_eq!(std::fs::read_to_string(dir.join("levels/hall.toml")).unwrap(), "format = \"voxcade level 1\"\nsize = [32, 32, 16]\n\n[light]\nsky = \"#20304a\"   # dusk\nsun_color = [0, 0, 0]\n");
        // What is left is only said: the game opens once nothing else is.
        std::fs::write(dir.join("conf.lua"), std::fs::read_to_string(dir.join("conf.lua")).unwrap().replace("  c.effects.outline_color = DARK\n", "")).unwrap();
        opens(&dir).unwrap();
        assert!(upgrade(&dir, false).unwrap().join("\n").contains("nothing to do"));
    }
}

//! A Lua script as tokens: what of it counts for replays (comments, blank
//! lines and indentation do not; names, keywords, numbers and strings as
//! written do), and the one operator the sandbox refuses: `^` calls the
//! system's `pow`, which differs in the last bit between Linux, Windows and
//! macOS (Lua folds `x ^ 2` into `x * x`, so squares stay).

use anyhow::{Result, bail};

/// What a piece of a script is (for colouring it in the IDE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Comment,
    String,
    Number,
    Keyword,
    Name,
    /// Operators and punctuation (a letter that is not ASCII, too).
    Symbol,
}

/// A piece of a script: where it lies (bytes, on a letter's bounds), what
/// it is, and whether it is a comment or string that never ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: std::ops::Range<usize>,
    pub kind: Kind,
    pub open: bool,
}

/// Lua's keywords.
pub const KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// The script in pieces, one after another (the space between them is in
/// none).
/// Whether the code names one of `words` (as a name, not in a string or
/// a comment).
pub fn names(src: &str, words: &[&str]) -> bool {
    spans(src).iter().any(|s| s.kind == Kind::Name && words.contains(&&src[s.range.clone()]))
}

pub fn spans(src: &str) -> Vec<Span> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let (kind, open) = if c == b'-' && b.get(i + 1) == Some(&b'-') {
            i += 2;
            if let Some(level) = long_bracket(b, i) {
                let end = close_long(b, i + level + 2, level);
                i = end.unwrap_or(b.len());
                (Kind::Comment, end.is_none())
            } else {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                (Kind::Comment, false)
            }
        } else if let Some(level) = (c == b'[').then(|| long_bracket(b, i)).flatten() {
            let end = close_long(b, i + level + 2, level);
            i = end.unwrap_or(b.len());
            (Kind::String, end.is_none())
        } else if c == b'"' || c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != c {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            let open = i >= b.len();
            i = (i + 1).min(b.len());
            (Kind::String, open)
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            (if KEYWORDS.contains(&&src[start..i]) { Kind::Keyword } else { Kind::Name }, false)
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let hex = c == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X'));
            if hex {
                i += 2;
            }
            while let Some(&d) = b.get(i) {
                let exponent = if hex { matches!(d, b'p' | b'P') } else { matches!(d, b'e' | b'E') };
                if exponent && matches!(b.get(i + 1), Some(b'+' | b'-')) {
                    i += 2;
                } else if d.is_ascii_alphanumeric() || d == b'.' {
                    i += 1;
                } else {
                    break;
                }
            }
            (Kind::Number, false)
        } else {
            let three = [b"...".as_slice()];
            let two = [b"..".as_slice(), b"==", b"~=", b"<=", b">=", b"<<", b">>", b"//", b"::"];
            i += if three.iter().any(|t| b[i..].starts_with(t)) {
                3
            } else if two.iter().any(|t| b[i..].starts_with(t)) {
                2
            } else {
                // A letter that is not ASCII, whole.
                src[i..].chars().next().map_or(1, char::len_utf8)
            };
            (Kind::Symbol, false)
        };
        // (An escape at the very end can step past it.)
        i = i.min(b.len());
        out.push(Span { range: start..i, kind, open });
    }
    out
}

/// The script's tokens, one after another, for the fingerprint; an error
/// names the file and line of a `^` other than `^ 2` or of text Lua could
/// not read.
pub fn tokens(file: &str, src: &str) -> Result<Vec<u8>> {
    let b = src.as_bytes();
    let line_at = |at: usize| 1 + b[..at].iter().filter(|&&x| x == b'\n').count();
    let mut out = Vec::new();
    let mut last_power: Option<usize> = None;
    for span in spans(src) {
        let start = span.range.start;
        // A power must have a plain 2 after it.
        if let Some(at) = last_power.take() {
            let two = b[start] == b'2' && !b.get(start + 1).is_some_and(|n| n.is_ascii_alphanumeric() || *n == b'.' || *n == b'_');
            if !two {
                bail!("{file}:{}: `^` differs between Linux, Windows and macOS: use math.pow(x, y) (x ^ 2 is fine)", line_at(at));
            }
        }
        let text = &b[span.range.clone()];
        match span.kind {
            Kind::Comment if span.open => bail!("{file}:{}: a comment that never ends", line_at(start)),
            Kind::Comment => continue,
            // A quoted string that never ends is named where the file does.
            Kind::String if span.open && text[0] != b'[' => bail!("{file}:{}: a string that never ends", line_at(span.range.end)),
            Kind::String if span.open => bail!("{file}:{}: a string that never ends", line_at(start)),
            Kind::Symbol if text == b"^" => last_power = Some(start),
            _ => {}
        }
        // A letter that is not ASCII counts byte by byte.
        if span.kind == Kind::Symbol && text[0] >= 0x80 {
            for &x in text {
                out.extend_from_slice(&[x, 0]);
            }
            continue;
        }
        out.extend_from_slice(text);
        out.push(0);
    }
    if let Some(at) = last_power {
        bail!("{file}:{}: `^` at the end of the script", line_at(at));
    }
    Ok(out)
}

/// The level of a long bracket opening at `i` (`[[` 0, `[=[` 1, ...).
fn long_bracket(b: &[u8], i: usize) -> Option<usize> {
    if b.get(i) != Some(&b'[') {
        return None;
    }
    let mut level = 0;
    while b.get(i + 1 + level) == Some(&b'=') {
        level += 1;
    }
    (b.get(i + 1 + level) == Some(&b'[')).then_some(level)
}

/// Just past the long bracket of `level` that closes after `from`.
fn close_long(b: &[u8], from: usize, level: usize) -> Option<usize> {
    let mut i = from;
    while i < b.len() {
        if b[i] == b']' && b[i + 1..].iter().take(level).all(|&x| x == b'=') && b.get(i + 1 + level) == Some(&b']') {
            return Some(i + level + 2);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fingerprint's tokens made from the spans are, byte for byte,
    /// those the lexer made before them: of every game's scripts and of
    /// pieces that go wrong.
    #[test]
    fn highlighting_keeps_the_fingerprint() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "lua") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(std::path::Path::new("games"), &mut files);
        walk(std::path::Path::new("lua"), &mut files);
        let mut sources: Vec<String> = files.iter().map(|f| std::fs::read_to_string(f).unwrap()).collect();
        assert!(sources.len() > 8);
        for odd in [
            "x = 2 ^ 2.5", "x = a^2_", "x = a ^ --[[c]] 2", "s = 'never", "s = \"a\\\nb", "--[[ open", "s = [==[ open ]=]", "x = 0x1p+4 .. 1e-3", "x = 'é' .. ü", "a ^", "a = b..c...d == e ~= f // g :: h",
        ] {
            sources.push(odd.to_string());
        }
        for src in &sources {
            let (new, old) = (tokens("t.lua", src), reference("t.lua", src));
            match (new, old) {
                (Ok(a), Ok(b)) => assert!(a == b, "{src:?}"),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "{src:?}"),
                (a, b) => panic!("{src:?}: {a:?} against {b:?}"),
            }
        }
    }

    /// The spans cover what there is to colour: keywords, names, numbers,
    /// strings and comments where they are, a string that never ends to
    /// the end.
    #[test]
    fn a_script_comes_in_coloured_spans() {
        let src = "local x = 3 -- three\nprint('a' .. [[b]]) s = 'open";
        let got: Vec<(&str, Kind, bool)> = spans(src).iter().map(|s| (&src[s.range.clone()], s.kind, s.open)).collect();
        use Kind::*;
        assert_eq!(
            got,
            [
                ("local", Keyword, false),
                ("x", Name, false),
                ("=", Symbol, false),
                ("3", Number, false),
                ("-- three", Comment, false),
                ("print", Name, false),
                ("(", Symbol, false),
                ("'a'", String, false),
                ("..", Symbol, false),
                ("[[b]]", String, false),
                (")", Symbol, false),
                ("s", Name, false),
                ("=", Symbol, false),
                ("'open", String, true),
            ]
        );
    }

    /// The tokens as they were made before `spans` (the fingerprint must
    /// stay the same, byte for byte).
    fn reference(file: &str, src: &str) -> Result<Vec<u8>> {
        let b = src.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        let mut line = 1;
        let mut last_power: Option<usize> = None;
        let push = |out: &mut Vec<u8>, tok: &[u8]| {
            out.extend_from_slice(tok);
            out.push(0);
        };
        while i < b.len() {
            let c = b[i];
            if c == b'\n' {
                line += 1;
                i += 1;
                continue;
            }
            if c.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            // A power must have a plain 2 after it.
            if let Some(at) = last_power.take() {
                let two = c == b'2' && !b.get(i + 1).is_some_and(|n| n.is_ascii_alphanumeric() || *n == b'.' || *n == b'_');
                if !two {
                    bail!("{file}:{at}: `^` differs between Linux, Windows and macOS: use math.pow(x, y) (x ^ 2 is fine)");
                }
            }
            let start = i;
            if c == b'-' && b.get(i + 1) == Some(&b'-') {
                i += 2;
                if let Some(level) = long_bracket(b, i) {
                    let end = close_long(b, i + level + 2, level).ok_or_else(|| anyhow::anyhow!("{file}:{line}: a comment that never ends"))?;
                    line += b[i..end].iter().filter(|&&x| x == b'\n').count();
                    i = end;
                } else {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
                continue;
            }
            if c == b'['
                && let Some(level) = long_bracket(b, i)
            {
                let end = close_long(b, i + level + 2, level).ok_or_else(|| anyhow::anyhow!("{file}:{line}: a string that never ends"))?;
                line += b[i..end].iter().filter(|&&x| x == b'\n').count();
                push(&mut out, &b[start..end]);
                i = end;
                continue;
            }
            if c == b'"' || c == b'\'' {
                i += 1;
                while i < b.len() && b[i] != c {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    if i < b.len() && b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                if i >= b.len() {
                    bail!("{file}:{line}: a string that never ends");
                }
                i += 1;
                push(&mut out, &b[start..i]);
                continue;
            }
            if c.is_ascii_alphabetic() || c == b'_' {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                push(&mut out, &b[start..i]);
                continue;
            }
            if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
                let hex = c == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X'));
                if hex {
                    i += 2;
                }
                while let Some(&d) = b.get(i) {
                    let exponent = if hex { matches!(d, b'p' | b'P') } else { matches!(d, b'e' | b'E') };
                    if exponent && matches!(b.get(i + 1), Some(b'+' | b'-')) {
                        i += 2;
                    } else if d.is_ascii_alphanumeric() || d == b'.' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                push(&mut out, &b[start..i]);
                continue;
            }
            let three = [b"...".as_slice()];
            let two = [b"..".as_slice(), b"==", b"~=", b"<=", b">=", b"<<", b">>", b"//", b"::"];
            let len = if three.iter().any(|t| b[i..].starts_with(t)) {
                3
            } else if two.iter().any(|t| b[i..].starts_with(t)) {
                2
            } else {
                1
            };
            if c == b'^' {
                last_power = Some(line);
            }
            push(&mut out, &b[i..i + len]);
            i += len;
        }
        if let Some(at) = last_power {
            bail!("{file}:{at}: `^` at the end of the script");
        }
        Ok(out)
    }

    /// Comments, blank lines and indentation leave the tokens as they were;
    /// a changed number does not.
    #[test]
    fn comments_and_layout_leave_the_tokens() {
        let a = tokens("main.lua", "function on_tick()\n  local x = 3 -- three\nend\n").unwrap();
        let b = tokens("main.lua", "--[[ a game ]]\n\nfunction on_tick( )\n\tlocal x = 3\nend").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, tokens("main.lua", "function on_tick() local x = 4 end").unwrap());
        assert_ne!(a, tokens("main.lua", "function on_tick() local x = '3' end").unwrap());
    }

    /// A power other than a square is refused, naming the line.
    #[test]
    fn power_is_refused_but_squares_are_not() {
        assert!(tokens("main.lua", "local a = x ^ 2 + y^2").is_ok());
        let e = tokens("main.lua", "local a = 1\nlocal b = x ^ 0.5").unwrap_err().to_string();
        assert!(e.starts_with("main.lua:2:"), "{e}");
        assert!(tokens("main.lua", "local b = x ^ 20").is_err());
        assert!(tokens("main.lua", "local s = '2^3' -- 2^3 in a string and a comment").is_ok());
    }
}

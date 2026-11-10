//! A state as text a person can read and the engine reads back exactly:
//! a Lua table as it would be written in `main.lua` (`{ quests = { [1] =
//! "lamp" }, gold = 12 }`), for save games (`docs/game-model.md`, "Save
//! games"). Floats keep every bit (`0.1` comes back as the same `0.1`);
//! strings keep every byte. Things are left out: a thing lives in its level
//! and comes back from it, never from a save; a player is its seat
//! (`world.players[2]`). Read by a small parser of its own, not by Lua: a
//! save from elsewhere runs no code.

use anyhow::{Result, anyhow, bail};

use super::value::{Key, Value};

/// How deep tables may lie in one another: Lua's own limit for a
/// constructor, far beyond what a game keeps (and a hostile file cannot
/// run the reader out of stack).
const DEEPEST: usize = 200;

/// Lua's words, which a key cannot be written bare as.
const KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// `value` as a Lua constructor, tables broken over lines below `indent`.
pub fn write(value: &Value) -> String {
    let mut out = String::new();
    put(&mut out, value, 0);
    out
}

fn put(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(bits) => out.push_str(&float(f64::from_bits(*bits))),
        Value::Str(s) => out.push_str(&string(s)),
        Value::Thing(id) => match super::host::handle_seat(*id) {
            Some(seat) => out.push_str(&format!("world.players[{seat}]")),
            None => out.push_str("nil"),
        },
        Value::Table(entries) => {
            let entries: Vec<&(Key, Value)> = entries.iter().filter(|(_, v)| !matches!(v, Value::Thing(id) if super::host::handle_seat(*id).is_none())).collect();
            if entries.is_empty() {
                return out.push_str("{}");
            }
            let flat = entries.iter().all(|(_, v)| !matches!(v, Value::Table(t) if !t.is_empty()));
            let mut one_line = String::from("{ ");
            for (n, (k, v)) in entries.iter().enumerate() {
                if n > 0 {
                    one_line.push_str(", ");
                }
                one_line.push_str(&key(k));
                put(&mut one_line, v, indent + 1);
            }
            one_line.push_str(" }");
            if flat && one_line.len() <= 72 {
                return out.push_str(&one_line);
            }
            out.push_str("{\n");
            for (k, v) in entries {
                out.push_str(&"  ".repeat(indent + 1));
                out.push_str(&key(k));
                put(out, v, indent + 1);
                out.push_str(",\n");
            }
            out.push_str(&"  ".repeat(indent));
            out.push('}');
        }
    }
}

fn key(k: &Key) -> String {
    match k {
        Key::Str(s) if is_name(s) => format!("{} = ", String::from_utf8_lossy(s)),
        Key::Str(s) => format!("[{}] = ", string(s)),
        Key::Int(i) => format!("[{i}] = "),
        Key::Float(bits) => format!("[{}] = ", float(f64::from_bits(*bits))),
        Key::Bool(b) => format!("[{b}] = "),
    }
}

fn is_name(s: &[u8]) -> bool {
    let first = s.first().is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_');
    first && s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_') && !KEYWORDS.contains(&std::str::from_utf8(s).unwrap_or(""))
}

/// A float as Lua reads it back to the same bits: always with a point or an
/// exponent (`1.0`, not `1`, which is an integer).
fn float(f: f64) -> String {
    if f.is_nan() {
        "0/0".into()
    } else if f.is_infinite() {
        if f > 0.0 { "math.huge".into() } else { "-math.huge".into() }
    } else {
        let text = format!("{f:?}");
        if text.contains(['.', 'e', 'E']) { text } else { format!("{text}.0") }
    }
}

/// A string in double quotes: text as it is, quotes, backslashes and the
/// bytes that are no printable text as escapes.
fn string(s: &[u8]) -> String {
    let mut out = String::from("\"");
    let text = std::str::from_utf8(s).ok();
    let escape = |out: &mut String, b: u8| match b {
        b'"' => out.push_str("\\\""),
        b'\'' => out.push_str("\\'"),
        b'\\' => out.push_str("\\\\"),
        b'\n' => out.push_str("\\n"),
        b'\t' => out.push_str("\\t"),
        b'\r' => out.push_str("\\r"),
        b if b < 0x20 || b == 0x7f || b >= 0x80 => out.push_str(&format!("\\{b:03}")),
        b => out.push(b as char),
    };
    match text {
        Some(text) => {
            for c in text.chars() {
                if c.is_ascii() { escape(&mut out, c as u8) } else { out.push(c) }
            }
        }
        None => {
            for &b in s {
                escape(&mut out, b);
            }
        }
    }
    out.push('"');
    out
}

/// The value `text` writes (as `write` writes it, or a Lua constructor of
/// plain data by hand).
pub fn read(text: &str) -> Result<Value> {
    let mut p = Parser { s: text.as_bytes(), at: 0, depth: 0 };
    let v = p.value()?;
    p.space();
    if p.at < p.s.len() {
        bail!("{}: more after the value", p.place());
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
    /// Tables open around the one being read.
    depth: usize,
}

impl Parser<'_> {
    /// Where the parser is, as a line and column for a mistake.
    fn place(&self) -> String {
        let before = &self.s[..self.at.min(self.s.len())];
        let line = before.iter().filter(|&&c| c == b'\n').count() + 1;
        let column = before.iter().rev().take_while(|&&c| c != b'\n').count() + 1;
        format!("line {line}, column {column}")
    }

    fn space(&mut self) {
        loop {
            while self.at < self.s.len() && self.s[self.at].is_ascii_whitespace() {
                self.at += 1;
            }
            if self.s[self.at..].starts_with(b"--") {
                while self.at < self.s.len() && self.s[self.at] != b'\n' {
                    self.at += 1;
                }
            } else {
                return;
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.space();
        self.s.get(self.at).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'{') => self.table(),
            Some(b'"' | b'\'') => Ok(Value::Str(self.string()?)),
            Some(c) if c.is_ascii_digit() || c == b'-' || c == b'.' => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => match self.name().as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "math" if self.s[self.at..].starts_with(b".huge") => {
                    self.at += 5;
                    Ok(Value::Float(f64::INFINITY.to_bits()))
                }
                // A player, by its seat.
                "world" if self.s[self.at..].starts_with(b".players[") => {
                    self.at += 9;
                    let from = self.at;
                    while self.s.get(self.at).is_some_and(u8::is_ascii_digit) {
                        self.at += 1;
                    }
                    let seat: usize = std::str::from_utf8(&self.s[from..self.at]).ok().and_then(|t| t.parse().ok()).filter(|&n| (1..=crate::config::MOST_PLAYERS as usize).contains(&n)).ok_or_else(|| anyhow!("{}: a seat's number", self.place()))?;
                    if !self.eat(b']') {
                        bail!("{}: ']' after the seat", self.place());
                    }
                    Ok(Value::Thing(super::host::seat_handle(seat)))
                }
                other => bail!("{}: '{other}' is no plain data", self.place()),
            },
            _ => bail!("{}: a value (a number, a string, true, false or a table)", self.place()),
        }
    }

    fn name(&mut self) -> String {
        let from = self.at;
        while self.at < self.s.len() && (self.s[self.at].is_ascii_alphanumeric() || self.s[self.at] == b'_') {
            self.at += 1;
        }
        String::from_utf8_lossy(&self.s[from..self.at]).into_owned()
    }

    fn number(&mut self) -> Result<Value> {
        let minus = self.eat(b'-');
        if self.s[self.at..].starts_with(b"math.huge") {
            self.at += 9;
            return Ok(Value::Float(if minus { f64::NEG_INFINITY } else { f64::INFINITY }.to_bits()));
        }
        if self.s[self.at..].starts_with(b"0/0") {
            self.at += 3;
            return Ok(Value::Float(f64::NAN.to_bits()));
        }
        let from = self.at;
        while self.at < self.s.len() && (self.s[self.at].is_ascii_alphanumeric() || matches!(self.s[self.at], b'.' | b'+' | b'-')) {
            // A sign only right after an exponent.
            if matches!(self.s[self.at], b'+' | b'-') && !matches!(self.s[self.at - 1], b'e' | b'E') {
                break;
            }
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.s[from..self.at]).unwrap_or("");
        let signed = if minus { format!("-{text}") } else { text.to_string() };
        if text.contains(['.', 'e', 'E', 'n', 'i']) {
            let f: f64 = signed.parse().map_err(|_| anyhow!("{}: '{signed}' is no number", self.place()))?;
            Ok(Value::Float(f.to_bits()))
        } else {
            Ok(Value::Int(signed.parse().map_err(|_| anyhow!("{}: '{signed}' is no whole number", self.place()))?))
        }
    }

    fn string(&mut self) -> Result<Vec<u8>> {
        let quote = self.s[self.at];
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let Some(&c) = self.s.get(self.at) else { bail!("{}: a string without its end", self.place()) };
            self.at += 1;
            match c {
                b'\\' => {
                    let Some(&e) = self.s.get(self.at) else { bail!("{}: a string without its end", self.place()) };
                    self.at += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'\\' | b'"' | b'\'' => out.push(e),
                        b'0'..=b'9' => {
                            let mut n = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.s.get(self.at) {
                                    Some(d) if d.is_ascii_digit() => {
                                        n = n * 10 + (d - b'0') as u32;
                                        self.at += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(u8::try_from(n).map_err(|_| anyhow!("{}: \\{n} is no byte", self.place()))?);
                        }
                        other => bail!("{}: '\\{}' is no escape here", self.place(), other as char),
                    }
                }
                c if c == quote => return Ok(out),
                b'\n' => bail!("{}: a string without its end", self.place()),
                c => out.push(c),
            }
        }
    }

    /// A key written as a name (`gold = `): the name, its `=` read.
    fn named_key(&mut self) -> Option<String> {
        if !self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
            return None;
        }
        let at = self.at;
        let name = self.name();
        if !KEYWORDS.contains(&name.as_str()) && self.peek() == Some(b'=') && !self.s[self.at..].starts_with(b"==") {
            self.at += 1;
            return Some(name);
        }
        self.at = at;
        None
    }

    fn table(&mut self) -> Result<Value> {
        self.depth += 1;
        if self.depth > DEEPEST {
            bail!("{}: tables more than {DEEPEST} deep", self.place());
        }
        self.at += 1;
        let mut entries: Vec<(Key, Value)> = Vec::new();
        let mut next = 1;
        while !self.eat(b'}') {
            let key = if self.eat(b'[') {
                let k = match self.value()? {
                    Value::Int(i) => Key::Int(i),
                    Value::Float(f) => Key::Float(f),
                    Value::Str(s) => Key::Str(s),
                    Value::Bool(b) => Key::Bool(b),
                    _ => bail!("{}: a key is a number, a string or a boolean", self.place()),
                };
                if !self.eat(b']') || !self.eat(b'=') {
                    bail!("{}: '] =' after a key", self.place());
                }
                k
            } else if let Some(name) = self.named_key() {
                Key::Str(name.into_bytes())
            } else {
                next += 1;
                Key::Int(next - 1)
            };
            let value = self.value()?;
            // A key given twice: the later counts, as in Lua.
            entries.retain(|(k, _)| *k != key);
            entries.push((key, value));
            if !self.eat(b',') && !self.eat(b';') {
                if self.eat(b'}') {
                    break;
                }
                bail!("{}: ',' or '}}' after a value", self.place());
            }
        }
        entries.sort_by(|a, b| a.0.before(&b.0));
        self.depth -= 1;
        Ok(Value::Table(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Value {
        Value::Str(text.as_bytes().to_vec())
    }

    /// What is written is read back the same, bit for bit: floats, odd
    /// strings, keys of every kind, tables in tables; things are left out.
    #[test]
    fn a_state_reads_back_as_it_was_written() {
        let quests = Value::Table(vec![(Key::Int(1), s("lamp")), (Key::Int(2), s("say \"hi\"\n\\ it's \u{e9}t\u{e9}"))]);
        let state = Value::Table(vec![
            (Key::Float(1.5f64.to_bits()), Value::Bool(true)),
            (Key::Int(3), Value::Float(0.1f64.to_bits())),
            (Key::Str(b"end".to_vec()), Value::Int(-7)),
            (Key::Str(b"gold".to_vec()), Value::Int(i64::MAX)),
            (Key::Str(b"huge".to_vec()), Value::Float(f64::INFINITY.to_bits())),
            (Key::Str(b"one".to_vec()), Value::Float(1.0f64.to_bits())),
            (Key::Str(b"quests".to_vec()), quests),
            (Key::Str(b"raw".to_vec()), Value::Str(vec![0, 200, b'a', 255])),
            (Key::Str(b"tiny".to_vec()), Value::Float(1e-300f64.to_bits())),
            (Key::Bool(false), Value::Table(Vec::new())),
        ]);
        let text = write(&state);
        assert_eq!(read(&text).unwrap(), state, "{text}");
        let mut with_thing = state.clone();
        if let Value::Table(t) = &mut with_thing {
            t.push((Key::Str(b"hero".to_vec()), Value::Thing(4)));
        }
        assert_eq!(read(&write(&with_thing)).unwrap(), state, "no things");
    }

    /// A table written by hand reads too: positions, names, comments; a
    /// mistake says where.
    #[test]
    fn a_hand_written_table_reads() {
        let v = read("{ 'a', \"b\"; x = 1, -- a comment\n [10] = -2.5e3, }").unwrap();
        assert_eq!(v, Value::Table(vec![(Key::Int(1), s("a")), (Key::Int(2), s("b")), (Key::Int(10), Value::Float((-2500.0f64).to_bits())), (Key::Str(b"x".to_vec()), Value::Int(1))]));
        let e = read("{ x = print }").unwrap_err().to_string();
        assert!(e.contains("line 1, column 12: 'print' is no plain data"), "{e}");
        assert!(read("{ x = 1\n y = 2 }").unwrap_err().to_string().contains("line 2"));
        assert_eq!(read("{ [1] = 'a', 'b' }").unwrap(), Value::Table(vec![(Key::Int(1), s("b"))]), "the later counts, as in Lua");
    }

    /// A file of tables nested beyond reason is a mistake, not a crash.
    #[test]
    fn tables_nested_too_deep_are_refused() {
        let deep = "{".repeat(40_000) + &"}".repeat(40_000);
        assert!(read(&deep).unwrap_err().to_string().contains("more than 200 deep"));
        let fine = "{".repeat(200) + &"}".repeat(200);
        assert!(read(&fine).is_ok());
    }
}

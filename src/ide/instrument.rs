//! The instrument maker over a song's text, the sound maker's way for an
//! MML instrument (`@name wave words`, its `+` layers and its `=` sum):
//! the instrument the text cursor is in (`at_cursor`), a sort's button
//! (KICK, SNARE, HAT, CLAP, TOM, BASS, LEAD, PAD) rolling a fresh one from
//! that sort's small table of ranges, five faders moving words of it, a
//! note of it rendered alone (`note`) by the music's own player. A roll is
//! rendered and measured before anyone hears it (`good`; a KICK inside
//! the owner's kick zone, `kick_meters`). The text stays the one truth:
//! a roll writes the instrument's lines anew (its name, its comments and
//! every other line kept byte for byte), a fader changes its words in
//! place.

use crate::audio::SAMPLE_RATE;
use crate::audio::words;
use super::maker::Dice;

/// What a line of an instrument is: its own (`@name ...`), a further
/// layer (`+ ...`) or the sum of its layers (`= ...`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Own,
    Layer,
    Sum,
}

/// An instrument in a song's text: its name and its lines (their places
/// among the text's lines, from 0), its own line first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub lines: Vec<(usize, Part)>,
}

/// What a line of a song's text is, read as `parse` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Empty, or a comment alone.
    Blank,
    Instrument,
    Layer,
    Sum,
    /// `A: notes`, its letter.
    Channel(char),
    Macro,
    /// `#setting`.
    Setting,
    Other,
}

/// A line without its comment.
fn code(line: &str) -> &str {
    line.split(';').next().unwrap_or("")
}

fn kind(line: &str) -> Kind {
    let c = code(line).trim();
    match c.chars().next() {
        None => Kind::Blank,
        Some('@') => Kind::Instrument,
        Some('+') => Kind::Layer,
        Some('=') => Kind::Sum,
        Some('$') => Kind::Macro,
        Some('#') => Kind::Setting,
        _ => match c.split_once(':') {
            Some((l, _)) if l.trim().chars().count() == 1 => Kind::Channel(l.trim().chars().next().unwrap_or('A')),
            _ => Kind::Other,
        },
    }
}

/// The name an instrument's line (`@name ...`) gives.
fn name_of(line: &str) -> &str {
    let c = code(line).trim_start();
    let rest = c.strip_prefix('@').unwrap_or("");
    rest.split(char::is_whitespace).next().unwrap_or("")
}

/// The instrument whose own line is line `at` of `lines`: that line and
/// the `+` and `=` lines under it (blank lines and comments may stand
/// between, as the reader takes them).
fn block_from(lines: &[&str], at: usize) -> Block {
    let mut block = Block { name: name_of(lines[at]).to_string(), lines: vec![(at, Part::Own)] };
    for (i, line) in lines.iter().enumerate().skip(at + 1) {
        match kind(line) {
            Kind::Blank => {}
            Kind::Layer => block.lines.push((i, Part::Layer)),
            Kind::Sum => block.lines.push((i, Part::Sum)),
            _ => break,
        }
    }
    block
}

/// The instrument named `name` in `text` (its first line of that name).
pub fn named(text: &str, name: &str) -> Option<Block> {
    let lines: Vec<&str> = text.split('\n').collect();
    let at = lines.iter().position(|l| kind(l) == Kind::Instrument && name_of(l) == name)?;
    Some(block_from(&lines, at))
}

/// The instrument the cursor (`at`, in characters) is in: on its own line
/// or a layer's or the sum's, that one; on a channel's notes (or a
/// macro's), the one the channel plays there (its last `@name` before
/// the cursor, from the channel's first line on); else the last
/// instrument's line above. None before the first.
pub fn at_cursor(text: &str, at: usize) -> Option<Block> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (row, col) = row_col(text, at);
    let row = row.min(lines.len().saturating_sub(1));
    let above = |row: usize| (0..=row).rev().find(|&i| kind(lines[i]) == Kind::Instrument).map(|i| block_from(&lines, i));
    match kind(lines[row]) {
        Kind::Instrument => Some(block_from(&lines, row)),
        Kind::Layer | Kind::Sum => {
            let own = (0..row).rev().find(|&i| !matches!(kind(lines[i]), Kind::Blank | Kind::Layer | Kind::Sum))?;
            (kind(lines[own]) == Kind::Instrument).then(|| block_from(&lines, own))
        }
        k @ (Kind::Channel(_) | Kind::Macro) => {
            let mut used = None;
            let rows: Vec<usize> = match k {
                Kind::Channel(letter) => (0..=row).filter(|&i| kind(lines[i]) == Kind::Channel(letter)).collect(),
                _ => vec![row],
            };
            for i in rows {
                let line = if i == row { lines[i].chars().take(col).collect::<String>() } else { lines[i].to_string() };
                // (A channel's letter is no note; a macro's name neither.)
                let notes = code(&line).split_once([':', '=']).map_or("", |(_, n)| n).to_string();
                for (name, _) in instruments_in(&notes) {
                    used = Some(name);
                }
            }
            used.and_then(|n| named(text, &n)).or_else(|| above(row))
        }
        _ => above(row),
    }
}

/// The `@name`s in a channel's notes and where each stands (chars), quoted
/// texts left out.
fn instruments_in(notes: &str) -> Vec<(String, usize)> {
    let chars: Vec<char> = notes.chars().collect();
    let (mut out, mut i, mut quoted) = (Vec::new(), 0, false);
    while i < chars.len() {
        match chars[i] {
            '"' => quoted = !quoted,
            '@' if !quoted => {
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }
                out.push((chars[start..end].iter().collect(), i));
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// The line and column (characters) of the character `at` in `text`.
pub fn row_col(text: &str, at: usize) -> (usize, usize) {
    let (mut row, mut col) = (0, 0);
    for c in text.chars().take(at) {
        if c == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (row, col)
}

/// The character at line `row`, column `col` (both from 0, the column
/// held within its line).
pub fn char_at(text: &str, row: usize, col: usize) -> usize {
    let mut at = 0;
    for (i, line) in text.split('\n').enumerate() {
        let n = line.chars().count();
        if i == row {
            return at + col.min(n);
        }
        at += n + 1;
    }
    text.chars().count()
}

/// The octave the instrument `name` plays its first note in, the note
/// that stands first in the file (each channel's `o`, `<`, `>` followed;
/// repeats and macros not): what the maker's keys begin at. None where no
/// channel plays it.
pub fn octave_used(text: &str, name: &str) -> Option<i32> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut letters: Vec<char> = Vec::new();
    for l in &lines {
        if let Kind::Channel(c) = kind(l)
            && !letters.contains(&c)
        {
            letters.push(c);
        }
    }
    // Each channel's first note of it: its row, its octave.
    let mut first: Option<(usize, i32)> = None;
    for letter in letters {
        let chars: Vec<(char, usize)> = lines.iter().enumerate().filter(|(_, l)| kind(l) == Kind::Channel(letter)).flat_map(|(row, l)| code(l).split_once(':').map_or("", |(_, n)| n).chars().chain([' ']).map(move |c| (c, row)).collect::<Vec<_>>()).collect();
        let (mut octave, mut playing, mut quoted) = (4i32, false, false);
        let mut i = 0;
        while i < chars.len() {
            let (c, row) = chars[i];
            i += 1;
            if c == '"' {
                quoted = !quoted;
                continue;
            }
            if quoted {
                continue;
            }
            match c {
                '@' | '$' => {
                    let start = i;
                    while i < chars.len() && (chars[i].0.is_alphanumeric() || chars[i].0 == '_') {
                        i += 1;
                    }
                    if c == '@' {
                        playing = chars[start..i].iter().map(|(c, _)| c).collect::<String>() == name;
                    }
                }
                'o' => {
                    let start = i;
                    while i < chars.len() && chars[i].0.is_ascii_digit() {
                        i += 1;
                    }
                    if let Ok(o) = chars[start..i].iter().map(|(c, _)| c).collect::<String>().parse::<i32>() {
                        octave = o;
                    }
                }
                '<' => octave -= 1,
                '>' => octave += 1,
                'a'..='g' if playing => {
                    if first.is_none_or(|(r, _)| row < r) {
                        first = Some((row, octave.clamp(0, 9)));
                    }
                    break;
                }
                _ => {}
            }
        }
    }
    first.map(|(_, o)| o)
}

/// A word of a line: where it stands (bytes of the line), its key (a
/// bare wave is its own key) and its value.
#[derive(Clone, Debug)]
struct Token {
    start: usize,
    end: usize,
    key: String,
    value: Option<String>,
}

/// The words of a line of an instrument (after `@name`, `+` or `=`, up to
/// the comment).
fn tokens(line: &str) -> Vec<Token> {
    let code = code(line);
    let trimmed = code.trim_start();
    let mut from = code.len() - trimmed.len();
    match trimmed.chars().next() {
        Some('@') => from += trimmed.find(char::is_whitespace).unwrap_or(trimmed.len()),
        Some('+' | '=') => from += 1,
        _ => {}
    }
    let mut out = Vec::new();
    let bytes = code.as_bytes();
    let mut i = from;
    while i < code.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < code.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let word = &code[start..i];
        let (key, value) = match word.split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (word.to_string(), None),
        };
        out.push(Token { start, end: i, key, value });
    }
    out
}

/// The waves a line may name, bare.
const WAVES: [&str; 9] = ["square", "pulse", "saw", "triangle", "sine", "noise", "sample", "voice", "singer"];

/// A line's wave (square unless it names one).
fn wave(line: &str) -> String {
    tokens(line).into_iter().rfind(|t| t.value.is_none() && WAVES.contains(&t.key.as_str())).map_or("square".into(), |t| t.key)
}

/// A word's number on a line, if written and a number.
fn get(line: &str, key: &str) -> Option<f64> {
    tokens(line).into_iter().rfind(|t| t.key == key).and_then(|t| t.value?.parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn has(line: &str, key: &str) -> bool {
    tokens(line).iter().any(|t| t.key == key)
}

/// A word's number as the instrument has it: written, or its default
/// (`song::Instrument`'s; a sum's volume 1).
fn value(line: &str, key: &str, part: Part) -> f64 {
    get(line, key).unwrap_or_else(|| default(key, part))
}

/// What an instrument word is unless written (`Instrument::default`;
/// the sum's volume 1).
fn default(key: &str, part: Part) -> f64 {
    match key {
        "volume" if part == Part::Sum => 1.0,
        "attack" => 0.004,
        "decay" => 0.1,
        "sustain" => 0.7,
        "release" => 0.08,
        "volume" => 0.5,
        "duty" => 0.5,
        "cutoff" => 1000.0,
        "pitch_decay" => 0.02,
        "vibrato_speed" => 6.0,
        "arp_speed" => 50.0,
        "pwm_speed" => 1.0,
        "sidechain_release" => 0.2,
        "duck" => 1.0,
        _ => 0.0,
    }
}

/// Where a key stands in the register's order: a bare wave first.
fn rank(key: &str) -> usize {
    if WAVES.contains(&key) {
        return 0;
    }
    words::WORDS.iter().filter(|w| w.mml.is_some()).position(|w| w.name == key).map_or(usize::MAX, |p| p + 1)
}

/// `line` with the words `set` (a value: written in place where the word
/// stands, after its kin in the register's order where it is new; None:
/// taken out); everything else on the line as it was, its comment too.
fn set_words(line: &str, set: &[(&str, Option<String>)]) -> String {
    let mut out = line.to_string();
    for (key, value) in set {
        let toks = tokens(&out);
        match (toks.iter().rfind(|t| t.key == *key), value) {
            (Some(t), Some(v)) => out.replace_range(t.start..t.end, &format!("{key}={v}")),
            (Some(t), None) => {
                // The word and the room before it.
                let from = out[..t.start].trim_end().len();
                out.replace_range(from..t.end, "");
            }
            (None, Some(v)) => {
                let after = toks.iter().filter(|t| rank(&t.key) <= rank(key)).map(|t| t.end).max();
                let at = after.or_else(|| toks.first().map(|t| t.start)).unwrap_or_else(|| code(&out).trim_end().len());
                let word = format!("{key}={v}");
                if after.is_some() || toks.is_empty() {
                    out.insert_str(at, &format!(" {word}"));
                } else {
                    out.insert_str(at, &format!("{word} "));
                }
            }
            (None, None) => {}
        }
    }
    out
}

/// A number as an instrument's text writes it: `places` after the point
/// at most, no zeros at the end.
fn written(v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

/// A word's number tidied to what the ear tells and inside the register's
/// range for MML.
fn tidy(key: &str, v: f64) -> f64 {
    let places = places(key, v);
    let k = 10f64.powi(places as i32);
    let v = (v * k).round() / k;
    match words::word(key).and_then(|w| w.mml.as_ref()).and_then(|u| u.range) {
        Some((lo, hi)) => v.clamp(lo, hi),
        None => v,
    }
}

fn places(key: &str, v: f64) -> usize {
    match key {
        "freq" | "pitch_start" | "cutoff" | "cutoff_end" | "lowpass" if v >= 100.0 => 0,
        "freq" | "pitch_start" | "cutoff" | "cutoff_end" | "lowpass" | "drive" | "sweep" | "vibrato_speed" => 1,
        "volume" | "sustain" | "resonance" | "duty" => 2,
        // Times: milliseconds, a tenth of one under 10.
        _ if v >= 0.01 => 3,
        _ => 4,
    }
}

fn num(key: &str, v: f64) -> String {
    let v = tidy(key, v);
    written(v, places(key, v))
}

/// A line of a fresh instrument: its part, its wave (none for a sum) and
/// its words in the register's order, numbers as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub part: Part,
    pub wave: Option<&'static str>,
    pub words: Vec<(&'static str, String)>,
}

impl Spec {
    fn new(part: Part, wave: Option<&'static str>, mut words: Vec<(&'static str, f64)>, choices: Vec<(&'static str, &'static str)>) -> Spec {
        // Only what differs from its default (a filter's cutoff and a
        // fixed pitch always: they say something by being there).
        words.retain(|(k, v)| matches!(*k, "freq" | "cutoff") || tidy(k, *v) != tidy(k, default(k, part)));
        let mut all: Vec<(&'static str, String)> = words.into_iter().map(|(k, v)| (k, num(k, v))).collect();
        all.extend(choices.into_iter().map(|(k, v)| (k, v.to_string())));
        all.sort_by_key(|(k, _)| rank(k));
        Spec { part, wave, words: all }
    }

    /// The words as written after `@name`, `+` or `=`.
    fn text(&self) -> String {
        let mut out: Vec<String> = self.wave.iter().map(|w| w.to_string()).collect();
        out.extend(self.words.iter().map(|(k, v)| format!("{k}={v}")));
        out.join(" ")
    }
}

/// `line` with its words (after `@name`, `+` or `=`) replaced by `words`:
/// what stands before them and its comment kept.
fn with_words(line: &str, words: &str) -> String {
    let toks = tokens(line);
    let code = code(line);
    let lead = match toks.first() {
        Some(t) => line[..t.start].to_string(),
        None => {
            let c = code.trim_end();
            format!("{c} ")
        }
    };
    let tail = match toks.last() {
        Some(t) => line[t.end..].to_string(),
        None => line[code.trim_end().len()..].to_string(),
    };
    format!("{lead}{words}{tail}")
}

/// `text` with the instrument `block` written anew as `specs` (its own
/// line first, then its layers, then its sum, if any): its name and the
/// room before its words kept, each line's comment kept, a layer more
/// than it had written under its last, one less taken out; every other
/// line as it was.
pub fn rolled(text: &str, block: &Block, specs: &[Spec]) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let old_layers: Vec<usize> = block.lines.iter().filter(|(_, p)| *p == Part::Layer).map(|(i, _)| *i).collect();
    let old_sum = block.lines.iter().find(|(_, p)| *p == Part::Sum).map(|(i, _)| *i);
    let own = block.lines[0].0;
    let new_own = specs.iter().find(|s| s.part == Part::Own);
    let new_layers: Vec<&Spec> = specs.iter().filter(|s| s.part == Part::Layer).collect();
    let new_sum = specs.iter().find(|s| s.part == Part::Sum);
    // How a new layer's and a new sum's line begins.
    let indent = block.lines.iter().skip(1).map(|(i, _)| lines[*i]).next().map_or("  ".to_string(), |l| l[..l.len() - l.trim_start().len()].to_string());
    let anchor = old_layers.last().copied().unwrap_or(own);
    let mut out: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == own {
            out.push(new_own.map_or(line.to_string(), |s| with_words(line, &s.text())));
        } else if let Some(k) = old_layers.iter().position(|&l| l == i) {
            if let Some(s) = new_layers.get(k) {
                out.push(with_words(line, &s.text()));
            }
        } else if Some(i) == old_sum {
            if let Some(s) = new_sum {
                out.push(with_words(line, &s.text()));
            }
        } else {
            out.push(line.to_string());
        }
        if i == anchor {
            for s in new_layers.iter().skip(old_layers.len()) {
                out.push(format!("{indent}+ {}", s.text()));
            }
            if let (None, Some(s)) = (old_sum, new_sum) {
                out.push(format!("{indent}= {}", s.text()));
            }
        }
    }
    out.join("\n")
}

/// Where the cursor at `at` (characters of `old`) stands in `new`, the
/// instrument `block` of `old` rewritten: before it, where it was; inside
/// it, at the instrument's own line; after it, as far on as the lines the
/// instrument gained or lost.
pub fn cursor_after(old: &str, new: &str, block: &Block, at: usize) -> usize {
    let (row, col) = row_col(old, at);
    let (first, last) = (block.lines[0].0, block.lines.last().map_or(0, |(i, _)| *i));
    let grown = new.split('\n').count() as isize - old.split('\n').count() as isize;
    match row {
        r if r < first => char_at(new, r, col),
        r if r > last => char_at(new, (r as isize + grown).max(0) as usize, col),
        r if grown == 0 => char_at(new, r, col),
        _ => char_at(new, first, 0),
    }
}

/// Whether the instrument plays a recording (the wave `sample` on a line).
pub fn uses_samples(text: &str, block: &Block) -> bool {
    lines_of(text, block).iter().any(|(l, _)| wave(l) == "sample")
}

/// The lines of the instrument in `text`, in its order.
fn lines_of<'a>(text: &'a str, block: &Block) -> Vec<(&'a str, Part)> {
    let lines: Vec<&str> = text.split('\n').collect();
    block.lines.iter().filter_map(|(i, p)| lines.get(*i).map(|l| (*l, *p))).collect()
}

/// `text` with line `row` replaced.
fn replace_line(text: &str, row: usize, line: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if row < lines.len() {
        lines[row] = line;
    }
    lines.join("\n")
}

// ---------------------------------------------------------------- sorts

/// A sort of instrument a button rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Kick,
    Snare,
    Hat,
    Clap,
    Tom,
    Bass,
    Lead,
    Pad,
}

impl Sort {
    pub const ALL: [Sort; 8] = [Sort::Kick, Sort::Snare, Sort::Hat, Sort::Clap, Sort::Tom, Sort::Bass, Sort::Lead, Sort::Pad];

    /// The sort its word names, in any case.
    pub fn named(word: &str) -> Option<Sort> {
        Sort::ALL.into_iter().find(|s| s.word().eq_ignore_ascii_case(word))
    }

    pub fn word(self) -> &'static str {
        match self {
            Sort::Kick => "KICK",
            Sort::Snare => "SNARE",
            Sort::Hat => "HAT",
            Sort::Clap => "CLAP",
            Sort::Tom => "TOM",
            Sort::Bass => "BASS",
            Sort::Lead => "LEAD",
            Sort::Pad => "PAD",
        }
    }

    pub fn hover(self) -> &'static str {
        match self {
            Sort::Kick => "A fresh bass drum: a sine falling to its low pitch, often a body and a click over it, driven together; warm to hard. It replaces this instrument and plays.",
            Sort::Snare => "A fresh snare drum: a short tone and a burst of filtered noise. It replaces this instrument and plays.",
            Sort::Hat => "A fresh closed hi-hat: bright noise, high-passed, dying fast. It replaces this instrument and plays.",
            Sort::Clap => "A fresh hand clap: band-passed noise in two or three layers, each a little later and longer. It replaces this instrument and plays.",
            Sort::Tom => "A fresh tom: a sine falling a little in pitch, ringing out. It replaces this instrument and plays.",
            Sort::Bass => "A fresh bass: a triangle, a pulse or a filtered saw, short and firm. It replaces this instrument and plays.",
            Sort::Lead => "A fresh lead for a melody: a pulse, a triangle or a saw, soft-filtered, with a little vibrato. It replaces this instrument and plays.",
            Sort::Pad => "A fresh pad: a saw or a triangle fading in and out slowly, low-passed. It replaces this instrument and plays.",
        }
    }

    /// The octave its tests measure a roll at (the maker's keys say
    /// another).
    #[cfg(test)]
    pub fn octave(self) -> i32 {
        match self {
            Sort::Bass => 2,
            Sort::Pad => 3,
            _ => 4,
        }
    }

    /// The longest a note of the sort may ring, in seconds (to -60 dB).
    fn longest(self) -> f64 {
        match self {
            Sort::Kick => 1.6,
            Sort::Snare | Sort::Clap => 0.8,
            Sort::Hat => 0.5,
            Sort::Tom => 1.4,
            Sort::Bass => 1.2,
            Sort::Lead => 1.6,
            Sort::Pad => 3.5,
        }
    }

    /// Where most of its energy must lie, in Hz.
    fn heard(self) -> (f64, f64) {
        match self {
            Sort::Kick => (25.0, 8000.0),
            Sort::Hat => (2000.0, 22_050.0),
            Sort::Tom | Sort::Bass => (40.0, 8000.0),
            _ => (80.0, 12_000.0),
        }
    }
}

/// Between two ends, on a scale of octaves (pitches, times).
fn ratio(dice: &mut Dice, (lo, hi): (f64, f64)) -> f64 {
    dice.pitch((lo, hi))
}

fn lerp((a, b): (f64, f64), t: f64) -> f64 {
    a + (b - a) * t
}

/// A fresh instrument of `sort` drawn from its ranges, each line its
/// words.
fn draw(sort: Sort, dice: &mut Dice) -> Vec<Spec> {
    let o = Part::Own;
    match sort {
        // The owner's kick recipe (from JUST EDIT, the owner's own audio
        // tool, measured on 154 one-shot kicks): a sub falling in Hz
        // to its pitch, often a body in the slam band (100 to 250 Hz) and
        // a click of band-passed noise (its centre 2.0 to 2.6 kHz), driven
        // together into the soft clipper. `hard` moves them together from
        // warm and long to hard and short, as its BITE and WUMMS do.
        Sort::Kick => {
            let hard = dice.unit();
            let tune = dice.between((44.0, 56.0));
            let start = tune * ratio(dice, (lerp((1.7, 4.0), hard) * 0.85, lerp((1.7, 4.0), hard) * 1.15));
            let decay = lerp((0.95, 0.5), hard) * dice.between((0.85, 1.15));
            let mut specs = vec![Spec::new(o, Some("sine"), vec![("freq", tune), ("pitch_start", start), ("pitch_decay", lerp((0.035, 0.018), hard) * dice.between((0.8, 1.2))), ("attack", 0.001), ("decay", decay), ("sustain", 0.0), ("release", decay), ("volume", 0.9)], vec![("curve", "exp")])];
            if dice.unit() < lerp((0.45, 0.95), hard) {
                let body = dice.between((110.0, 170.0));
                let d = dice.between((0.07, 0.14));
                specs.push(Spec::new(Part::Layer, Some("sine"), vec![("freq", body), ("pitch_start", body * dice.between((2.0, 2.5))), ("pitch_decay", dice.between((0.008, 0.014))), ("attack", 0.001), ("decay", d), ("sustain", 0.0), ("release", d), ("drive", dice.between((4.0, 9.0))), ("volume", lerp((0.25, 0.8), hard) * dice.between((0.85, 1.15)))], vec![("curve", "exp")]));
            }
            if dice.unit() < lerp((0.55, 1.0), hard) {
                let d = dice.between((0.05, 0.12));
                specs.push(Spec::new(Part::Layer, Some("noise"), vec![("freq", 20_000.0), ("cutoff", dice.between((1300.0, 1700.0))), ("resonance", 0.2), ("attack", 0.0005), ("decay", d), ("sustain", 0.0), ("release", d), ("volume", lerp((0.06, 1.0), hard) * dice.between((0.8, 1.0)))], vec![("filter", "bandpass"), ("curve", "exp")]));
            }
            specs.push(Spec::new(Part::Sum, None, vec![("drive", lerp((1.5, 10.0), hard) * dice.between((0.85, 1.15)))], vec![]));
            specs
        }
        // A tone layer and a noise layer (the snares of the drum kits the
        // library had until 2026-11-24: a tone at 160 to 260 Hz, noise
        // high- or band-passed over 1.2 kHz).
        Sort::Snare => {
            let tone = dice.between((160.0, 260.0));
            let d = dice.between((0.06, 0.14));
            let mut own = vec![("freq", tone), ("sweep", -dice.between((1.0, 5.0))), ("sweep_time", dice.between((0.02, 0.05))), ("attack", 0.0008), ("decay", d), ("sustain", 0.0), ("release", dice.between((0.01, 0.04))), ("volume", dice.between((0.45, 0.65)))];
            let wave = dice.pick(&["triangle", "sine", "saw"]);
            let mut choices = vec![];
            if wave == "saw" {
                own.push(("cutoff", tone * dice.between((1.8, 2.8))));
                choices.push(("filter", "lowpass"));
            }
            let n = dice.between((0.08, 0.2));
            let band = dice.unit() < 0.35;
            let noise = Spec::new(Part::Layer, Some("noise"), vec![("freq", 20_000.0), ("cutoff", if band { dice.between((1500.0, 3000.0)) } else { dice.between((1200.0, 2500.0)) }), ("resonance", if band { dice.between((0.1, 0.3)) } else { 0.0 }), ("attack", 0.0008), ("decay", n), ("sustain", 0.0), ("release", dice.between((0.01, 0.06))), ("volume", dice.between((0.3, 0.5)))], vec![("filter", if band { "bandpass" } else { "highpass" })]);
            let mut specs = vec![Spec::new(o, Some(wave), own, choices), noise];
            if dice.unit() < 0.5 {
                specs.push(Spec::new(Part::Sum, None, vec![("drive", dice.between((1.0, 6.0)))], vec![]));
            }
            specs
        }
        // Noise high-passed over 5 kHz (or band-passed round 7 to 10), its
        // fall to -20 dB about the measured references' 58 ms: an
        // exponential decay of 0.08 to 0.25 s to -60 dB.
        Sort::Hat => {
            let d = dice.between((0.08, 0.25));
            let band = dice.unit() < 0.3;
            vec![Spec::new(o, Some("noise"), vec![("freq", 20_000.0), ("cutoff", if band { dice.between((7000.0, 10_000.0)) } else { dice.between((5000.0, 8000.0)) }), ("resonance", dice.between((0.0, 0.3))), ("attack", 0.0005), ("decay", d), ("sustain", 0.0), ("release", d), ("volume", dice.between((0.3, 0.5)))], vec![("filter", if band { "bandpass" } else { "highpass" }), ("curve", "exp")])]
        }
        // Band-passed noise (the measured references' centre round 2.4
        // kHz: a band pass here at 1100 to 1600 Hz), two or three layers
        // each later and longer than the one before (no envelope here
        // fires again, so the hands are layers), the last the tail.
        Sort::Clap => {
            let cut = dice.between((1100.0, 1600.0));
            let res = dice.between((0.3, 0.65));
            let count = if dice.unit() < 0.5 { 2 } else { 3 };
            let mut specs = Vec::new();
            for k in 0..count {
                let tail = k == count - 1;
                let attack = if k == 0 { 0.0005 } else { 0.01 * k as f64 * dice.between((0.85, 1.2)) };
                let d = if tail { dice.between((0.12, 0.25)) } else { dice.between((0.025, 0.045)) };
                let part = if k == 0 { o } else { Part::Layer };
                specs.push(Spec::new(part, Some("noise"), vec![("freq", 20_000.0), ("cutoff", cut * dice.between((0.93, 1.07))), ("resonance", res), ("attack", attack), ("decay", d), ("sustain", 0.0), ("release", d), ("volume", if tail { dice.between((0.5, 0.8)) } else { dice.between((0.6, 0.9)) })], vec![("filter", "bandpass"), ("curve", "exp")]));
            }
            if dice.unit() < 0.5 {
                specs.push(Spec::new(Part::Sum, None, vec![("drive", dice.between((1.0, 6.0)))], vec![]));
            }
            specs
        }
        // A sine at 80 to 260 Hz (those kits' toms 107 to 254) falling a
        // little from over it, ringing out.
        Sort::Tom => {
            let f = ratio(dice, (80.0, 260.0));
            let d = dice.between((0.3, 0.8));
            vec![Spec::new(o, Some("sine"), vec![("freq", f), ("pitch_start", f * dice.between((1.3, 2.0))), ("pitch_decay", dice.between((0.02, 0.06))), ("attack", 0.001), ("decay", d), ("sustain", 0.0), ("release", d), ("drive", dice.between((0.0, 4.0))), ("volume", dice.between((0.6, 0.85)))], vec![("curve", "exp")])]
        }
        // compose.py's basses: a triangle, a pulse low-passed, or a saw
        // through the filter closing on every note.
        Sort::Bass => {
            let wave = dice.pick(&["triangle", "square", "saw"]);
            let mut w = vec![("attack", dice.between((0.002, 0.01))), ("decay", dice.between((0.12, 0.35))), ("sustain", dice.between((0.45, 0.7))), ("release", dice.between((0.05, 0.2))), ("volume", dice.between((0.4, 0.6)))];
            match wave {
                "square" => w.push(("lowpass", dice.between((900.0, 1600.0)))),
                "saw" => {
                    let cut = dice.between((500.0, 1000.0));
                    w.extend([("cutoff", cut), ("cutoff_end", dice.between((200.0, 320.0)).min(cut)), ("cutoff_time", dice.between((0.08, 0.18))), ("resonance", dice.between((0.2, 0.45)))]);
                }
                _ => {}
            }
            vec![Spec::new(o, Some(wave), w, vec![])]
        }
        // compose.py's leads: a pulse or a triangle, low-passed, a little
        // late vibrato.
        Sort::Lead => {
            let wave = dice.pick(&["square", "square", "triangle", "saw"]);
            let mut w = vec![("attack", dice.between((0.005, 0.03))), ("decay", dice.between((0.2, 0.4))), ("sustain", dice.between((0.5, 0.6))), ("release", dice.between((0.15, 0.4))), ("vibrato", dice.between((0.006, 0.012))), ("vibrato_speed", dice.between((4.5, 6.0))), ("vibrato_delay", dice.between((0.15, 0.35))), ("volume", dice.between((0.27, 0.45)))];
            w.push(("lowpass", if wave == "saw" { dice.between((1000.0, 1800.0)) } else { dice.between((1300.0, 2600.0)) }));
            vec![Spec::new(o, Some(wave), w, vec![])]
        }
        // compose.py's pads: a saw or a triangle fading in and out
        // slowly, low-passed.
        Sort::Pad => {
            let wave = dice.pick(&["saw", "triangle"]);
            let w = vec![("attack", dice.between((0.2, 0.8))), ("decay", dice.between((0.6, 1.0))), ("sustain", dice.between((0.6, 0.75))), ("release", dice.between((0.5, 1.0))), ("lowpass", if wave == "saw" { dice.between((600.0, 1200.0)) } else { dice.between((1000.0, 1800.0)) }), ("volume", dice.between((0.14, 0.22)))];
            vec![Spec::new(o, Some(wave), w, vec![])]
        }
    }
}

/// How many instruments a roll draws before it gives the last one drawn.
pub const TRIES: usize = 48;

/// A fresh instrument of `sort` from `seed`, its lines and the text with
/// it in place of `block`: drawn until one passes `good` played at the
/// octave `octave` in a song of `tempo` (`TRIES` at most), else the last
/// drawn.
pub fn roll(text: &str, block: &Block, sort: Sort, seed: u64, octave: i32) -> (Vec<Spec>, String) {
    let mut dice = Dice(seed);
    let mut last = None;
    for _ in 0..TRIES {
        let specs = draw(sort, &mut dice);
        let new = rolled(text, block, &specs);
        let fresh = named(&new, &block.name).expect("the instrument keeps its name");
        let passes = note(&new, &fresh, octave, 0, &crate::audio::music::Samples::default()).is_ok_and(|s| good(&s, sort).is_ok());
        if passes {
            return (specs, new);
        }
        last = Some((specs, new));
    }
    last.expect("one drawn")
}

// ------------------------------------------------------------ a note

/// The notes' names in MML, from C.
pub const NOTES: [&str; 12] = ["c", "c+", "d", "d+", "e", "f", "f+", "g", "g+", "a", "a+", "b"];
/// The same as the keys say them.
pub const KEYS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// A song of one note of the instrument `block` in `text`: the song's
/// tempo and meter, the instrument's lines, a channel playing a quarter
/// at the octave and semitone (from C) asked, once.
pub fn note_song(text: &str, block: &Block, octave: i32, semitone: usize) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut song = String::new();
    for word in ["#tempo", "#meter"] {
        if let Some(l) = lines.iter().find(|l| code(l).split_whitespace().next() == Some(word)) {
            song.push_str(code(l).trim());
            song.push('\n');
        }
    }
    song.push_str("#loop no\n");
    for (l, _) in lines_of(text, block) {
        song.push_str(l);
        song.push('\n');
    }
    song.push_str(&format!("A: @{} o{} l4 {}\n", block.name, octave.clamp(0, 9), NOTES[semitone % 12]));
    song
}

/// The tempo `text` states, 120 unless.
pub fn tempo(text: &str) -> f64 {
    text.split('\n').find_map(|l| code(l).trim().strip_prefix("#tempo").and_then(|t| t.trim().parse::<f64>().ok())).filter(|t| *t > 0.0).unwrap_or(120.0)
}

/// One note of the instrument `block` in `text` as heard (one channel, the
/// player's own samples, its silence at the end cut off), at the player's
/// full volume: what a key plays and a roll is measured on.
pub fn note(text: &str, block: &Block, octave: i32, semitone: usize, samples: &crate::audio::music::Samples) -> anyhow::Result<Vec<f32>> {
    let mut song = crate::audio::music::parse(&note_song(text, block, octave, semitone))?;
    song.bind(samples)?;
    // A quarter, then as long as the longest line's fall and release.
    let quarter = 60.0 / tempo(text);
    let lines = lines_of(text, block);
    let rings = lines.iter().map(|(l, p)| value(l, "attack", *p) + value(l, "decay", *p)).fold(quarter, f64::max);
    let release = lines.iter().map(|(l, p)| value(l, "release", *p)).fold(0.0, f64::max);
    let seconds = (rings + release + 0.1).min(12.0);
    let stereo = crate::audio::music::Player::new(song, 1.0).render(Some(seconds as f32));
    let mut mono: Vec<f32> = stereo.chunks(2).map(|f| (f[0] + f.get(1).copied().unwrap_or(f[0])) * 0.5).collect();
    let peak = mono.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let end = mono.iter().rposition(|x| x.abs() > peak * 1e-4).map_or(0, |i| i + 1);
    mono.truncate((end + SAMPLE_RATE as usize / 200).min(mono.len()));
    Ok(mono)
}

// ------------------------------------------------------- the measures

/// The quietest a rolled note may peak, and its loudness (RMS) while it
/// sounds, full scale 1; the loudest it may peak (more clips).
pub const PEAK_FLOOR: f32 = 0.05;
pub const RMS_FLOOR: f32 = 0.005;
pub const PEAK_CEILING: f32 = 0.98;
/// The share of its energy that must lie in its sort's band (`Sort::heard`).
pub const HEARD_SHARE: f64 = 0.5;

/// Whether a rolled note (`note`) is good to give back: loud enough and
/// not clipping, round zero, rung out within its sort's longest, its
/// energy mostly where its sort is heard, and a KICK inside the owner's
/// kick zone (`kick_meters`). The why when not.
pub fn good(samples: &[f32], sort: Sort) -> Result<(), String> {
    if samples.is_empty() {
        return Err("no samples".into());
    }
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > PEAK_CEILING {
        return Err(format!("clips: peak {peak:.3}"));
    }
    // Rung out: where it last lies over -60 dB of its peak.
    let rung = samples.iter().rposition(|x| x.abs() > peak * 1e-3).map_or(0, |i| i + 1);
    let seconds = rung as f64 / SAMPLE_RATE as f64;
    if seconds > sort.longest() {
        return Err(format!("{seconds:.2} s, longer than {} s", sort.longest()));
    }
    let sounding = &samples[..rung.max(1)];
    let rms = (sounding.iter().map(|x| x * x).sum::<f32>() / sounding.len() as f32).sqrt();
    if peak < PEAK_FLOOR || rms < RMS_FLOOR {
        return Err(format!("too quiet: peak {peak:.3}, rms {rms:.4}"));
    }
    let mean = sounding.iter().sum::<f32>() / sounding.len() as f32;
    if mean.abs() > 0.1 * rms {
        return Err(format!("off zero by {mean:.4}"));
    }
    let heard = super::maker::share_between(sounding, sort.heard());
    if heard < HEARD_SHARE {
        return Err(format!("only {:.0} % of it between {} and {} Hz", heard * 100.0, sort.heard().0, sort.heard().1));
    }
    if sort == Sort::Kick {
        let m = kick_meters(samples);
        m.inside()?;
    }
    Ok(())
}

/// The owner's three kick meters (JUST EDIT's, the owner's own audio
/// tool: its KNALL, BALLERN and BRUSTKORB), ported as its own text
/// defines them: the one-shot from
/// its onset, over its own peak; bands split by cascaded one-poles (two
/// a side, 12 dB an octave).
#[derive(Clone, Copy, Debug)]
pub struct KickMeters {
    /// The crack (KNALL): the RMS over 1 kHz in the first 60 ms, dB.
    pub crack: f64,
    /// The slam (BALLERN): the RMS in 100 to 250 Hz in the first 50 ms, dB.
    pub slam: f64,
    /// The chest (BRUSTKORB): the energy in 40 to 80 Hz over the whole
    /// one-shot, as the milliseconds of a full-scale sine carrying as much.
    pub chest: f64,
}

/// The owner's good zone for a kick: the envelope of the kicks the
/// owner's ear approved (its gates for the crack and the slam).
pub const CRACK_ZONE: (f64, f64) = (-46.0, -13.0);
pub const SLAM_ZONE: (f64, f64) = (-14.0, -8.5);
pub const CHEST_ZONE: (f64, f64) = (11.3, 68.5);

impl KickMeters {
    /// Whether the kick lies in the zone; the why when not.
    pub fn inside(&self) -> Result<(), String> {
        let within = |v: f64, (lo, hi): (f64, f64)| (lo..=hi).contains(&v);
        if !within(self.crack, CRACK_ZONE) {
            return Err(format!("crack {:.1} dB outside {:?}", self.crack, CRACK_ZONE));
        }
        if !within(self.slam, SLAM_ZONE) {
            return Err(format!("slam {:.1} dB outside {:?}", self.slam, SLAM_ZONE));
        }
        if !within(self.chest, CHEST_ZONE) {
            return Err(format!("chest {:.1} ms outside {:?}", self.chest, CHEST_ZONE));
        }
        Ok(())
    }
}

/// A one-pole low pass at `hz` over `x`, in place.
fn low_pass(x: &mut [f64], hz: f64) {
    let a = 1.0 - (-std::f64::consts::TAU * hz / SAMPLE_RATE as f64).exp();
    let mut y = 0.0;
    for v in x.iter_mut() {
        y += a * (*v - y);
        *v = y;
    }
}

/// A one-pole high pass: what the low pass takes away.
fn high_pass(x: &mut [f64], hz: f64) {
    let mut low = x.to_vec();
    low_pass(&mut low, hz);
    for (v, l) in x.iter_mut().zip(low) {
        *v -= l;
    }
}

/// The three kick meters of a one-shot.
pub fn kick_meters(samples: &[f32]) -> KickMeters {
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-9) as f64;
    let onset = samples.iter().position(|x| x.abs() as f64 >= peak * 1e-3).unwrap_or(0);
    let x: Vec<f64> = samples[onset..].iter().map(|v| *v as f64 / peak).collect();
    let rate = SAMPLE_RATE as f64;
    let band = |highs: &[f64], lows: &[f64]| {
        let mut y = x.clone();
        for &h in highs {
            high_pass(&mut y, h);
        }
        for &l in lows {
            low_pass(&mut y, l);
        }
        y
    };
    let rms_db = |y: &[f64], seconds: f64| {
        let n = (seconds * rate) as usize;
        let sum: f64 = y.iter().take(n).map(|v| v * v).sum();
        10.0 * (sum / n as f64).max(1e-30).log10()
    };
    let crack = rms_db(&band(&[1000.0, 1000.0], &[]), 0.060);
    let slam = rms_db(&band(&[100.0, 100.0], &[250.0, 250.0]), 0.050);
    let chest = band(&[40.0, 40.0], &[80.0, 80.0]).iter().map(|v| v * v).sum::<f64>() / rate / 0.5 * 1000.0;
    KickMeters { crack, slam, chest }
}

// ------------------------------------------------------------ faders

/// A fader over the instrument, each moving words of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fader {
    Length,
    Punch,
    Bright,
    Drive,
    Level,
}

/// PUNCH at its ends: the attack (seconds) and the pitch's fall
/// (semitones).
const SLOW: f64 = 1.0;
const FAST: f64 = 0.0005;
const FALL: f64 = 36.0;

impl Fader {
    pub const ALL: [Fader; 5] = [Fader::Length, Fader::Punch, Fader::Bright, Fader::Drive, Fader::Level];

    pub fn word(self) -> &'static str {
        match self {
            Fader::Length => "LENGTH",
            Fader::Punch => "PUNCH",
            Fader::Bright => "BRIGHT",
            Fader::Drive => "DRIVE",
            Fader::Level => "LEVEL",
        }
    }

    pub fn hover(self) -> &'static str {
        match self {
            Fader::Length => "How long a note rings: its fall and its fade after the key is let go, together. Moves decay and release of the instrument's own line, keeping their ratio.",
            Fader::Punch => "How hard a note starts. Where the pitch falls at the start (pitch_start, or sweep down), how far it falls, every layer that falls alike, its attack shorter with it; else the attack alone.",
            Fader::Bright => "How bright it sounds. Moves the filter's cutoff (the sum's where = states one, else the first line with a filter, or lowpass), cutoff_end along; a line without a filter gets a low pass. On sines alone: the own line's drive.",
            Fader::Drive => "How hard it is pushed into the soft clipper, in dB: 0 clean, 12 rough. The sum's drive where the instrument has an = line, else its own line's.",
            Fader::Level => "How loud the instrument plays. The sum's volume where it has an = line, else every line's volume together, keeping their balance.",
        }
    }

    pub fn range(self) -> (f64, f64) {
        match self {
            Fader::Length => (0.01, 6.0),
            Fader::Punch => (0.0, 1.0),
            Fader::Bright => (50.0, 20_000.0),
            Fader::Drive => (0.0, 36.0),
            Fader::Level => (0.0, 1.0),
        }
    }

    pub fn logarithmic(self) -> bool {
        matches!(self, Fader::Length | Fader::Bright)
    }

    /// What it stands at for the instrument `block` of `text` (`hz` the
    /// pitch of the key last played, for a fall without a fixed pitch),
    /// and its number as shown; or why it means nothing for it.
    pub fn place(self, text: &str, block: &Block, hz: f64) -> Result<(f64, String), &'static str> {
        let lines = lines_of(text, block);
        let (own, _) = *lines.first().ok_or("no instrument")?;
        let sum = lines.iter().find(|(_, p)| *p == Part::Sum).map(|(l, _)| *l);
        let (lo, hi) = self.range();
        let (v, shown) = match self {
            Fader::Length => {
                let v = value(own, "decay", Part::Own) + value(own, "release", Part::Own);
                (v, format!("{v:.2} S"))
            }
            Fader::Punch => match falls(&lines, hz).first() {
                Some(&(_, st)) => (st / FALL, format!("{st:.0} ST")),
                None => {
                    let a = value(own, "attack", Part::Own).max(FAST);
                    ((a / SLOW).ln() / (FAST / SLOW).ln(), format!("{:.1} MS", a * 1000.0))
                }
            },
            Fader::Bright => match bright(&lines)? {
                Bright::Cutoff(i, key) => {
                    let v = value(lines[i].0, key, lines[i].1);
                    (v, format!("{v:.0} HZ"))
                }
                Bright::Open => (hi, "OPEN".into()),
                Bright::Drive => {
                    let v = value(own, "drive", Part::Own);
                    (lo * (hi / lo).powf(v / 36.0), format!("{v:.1} DB"))
                }
            },
            Fader::Drive => {
                let v = match sum {
                    Some(s) => value(s, "drive", Part::Sum),
                    None => value(own, "drive", Part::Own),
                };
                (v, format!("{v:.1} DB"))
            }
            Fader::Level => {
                let v = match sum {
                    Some(s) => value(s, "volume", Part::Sum),
                    None => value(own, "volume", Part::Own),
                };
                (v, format!("{v:.2}"))
            }
        };
        Ok((v.clamp(lo, hi), shown))
    }

    /// `text` with the fader moved to `to` over the instrument `block`
    /// (`hz` as for `place`): only the words it says, in place, on the
    /// lines it says; the text as it was where it means nothing.
    pub fn moved(self, text: &str, block: &Block, hz: f64, to: f64) -> String {
        let Ok((from, _)) = self.place(text, block, hz) else { return text.to_string() };
        let (lo, hi) = self.range();
        let to = to.clamp(lo, hi);
        let lines = lines_of(text, block);
        let rows: Vec<usize> = block.lines.iter().map(|(i, _)| *i).collect();
        let own = lines[0].0;
        let sum = lines.iter().position(|(_, p)| *p == Part::Sum);
        let mut out = text.to_string();
        let mut set = |row: usize, line: &str, words: &[(&str, Option<String>)]| out = replace_line(&out, row, &set_words(line, words));
        match self {
            Fader::Length => {
                let (d, r) = (value(own, "decay", Part::Own), value(own, "release", Part::Own));
                let k = if d + r > 0.0 { to / (d + r) } else { 1.0 };
                let (d, r) = if d + r > 0.0 { (d * k, r * k) } else { (to / 2.0, to / 2.0) };
                set(rows[0], own, &[("decay", Some(num("decay", d.max(0.001)))), ("release", Some(num("release", r.max(0.001))))]);
            }
            Fader::Punch => {
                let fallen = falls(&lines, hz);
                let first = fallen.first().map(|&(_, st)| st);
                let goal = to * FALL;
                // The attack shorter or longer by as much as PUNCH moved.
                let attack = |line: &str, part: Part| {
                    let a = value(line, "attack", part).max(FAST) * (FAST / SLOW).powf(to - from);
                    Some(num("attack", a.clamp(0.0001, SLOW)))
                };
                if fallen.is_empty() {
                    set(rows[0], own, &[("attack", Some(num("attack", SLOW * (FAST / SLOW).powf(to))))]);
                }
                for (i, st) in fallen {
                    let (line, part) = lines[i];
                    let fall = match first {
                        Some(f) if f > 0.05 => st * goal / f,
                        _ => goal,
                    };
                    let mut words = vec![("attack", attack(line, part))];
                    if has(line, "pitch_start") {
                        let base = get(line, "freq").unwrap_or(hz);
                        words.push(("pitch_start", Some(num("pitch_start", base * 2f64.powf(fall / 12.0)))));
                    } else {
                        words.push(("sweep", Some(num("sweep", -fall))));
                    }
                    set(rows[i], line, &words);
                }
            }
            Fader::Bright => match bright(&lines) {
                Ok(Bright::Cutoff(i, key)) => {
                    let (line, part) = lines[i];
                    let old = value(line, key, part);
                    let cut = tidy(key, to);
                    let mut words = vec![(key, Some(num(key, cut)))];
                    if key == "cutoff"
                        && let Some(end) = get(line, "cutoff_end")
                    {
                        words.push(("cutoff_end", Some(num("cutoff_end", end * cut / old.max(1.0)))));
                    }
                    set(rows[i], line, &words);
                }
                Ok(Bright::Open) if to < hi * 0.999 => set(rows[0], own, &[("cutoff", Some(num("cutoff", to)))]),
                Ok(Bright::Drive) => set(rows[0], own, &[("drive", Some(num("drive", 36.0 * (to / lo).ln() / (hi / lo).ln())))]),
                _ => {}
            },
            Fader::Drive => match sum {
                Some(s) => set(rows[s], lines[s].0, &[("drive", Some(num("drive", to)))]),
                None => set(rows[0], own, &[("drive", Some(num("drive", to)))]),
            },
            Fader::Level => match sum {
                Some(s) => set(rows[s], lines[s].0, &[("volume", Some(num("volume", to)))]),
                None => {
                    let k = if from > 0.0 { to / from } else { 1.0 };
                    for (i, (line, part)) in lines.iter().enumerate() {
                        let v = if from > 0.0 { value(line, "volume", *part) * k } else { to };
                        set(rows[i], line, &[("volume", Some(num("volume", v)))]);
                    }
                }
            },
        }
        out
    }
}

/// The lines whose pitch falls at the start (`pitch_start`, or a `sweep`
/// not rising) and how far, in semitones (`hz` the note's pitch where the
/// line has no fixed one).
fn falls(lines: &[(&str, Part)], hz: f64) -> Vec<(usize, f64)> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, (l, _))| match (get(l, "pitch_start"), has(l, "sweep").then(|| value(l, "sweep", Part::Own))) {
            (Some(start), _) => Some((i, (12.0 * (start / get(l, "freq").unwrap_or(hz).max(1.0)).log2()).max(0.0))),
            (None, Some(s)) if s <= 0.0 => Some((i, -s)),
            _ => None,
        })
        .collect()
}

/// What BRIGHT moves.
enum Bright {
    /// A line's (its place among the instrument's) cutoff or lowpass.
    Cutoff(usize, &'static str),
    /// No filter yet: one is set on the own line.
    Open,
    /// Sines alone: the own line's drive.
    Drive,
}

fn bright(lines: &[(&str, Part)]) -> Result<Bright, &'static str> {
    let filtered = |l: &str| ["filter", "cutoff", "cutoff_end", "resonance"].iter().any(|k| has(l, k));
    if let Some(i) = lines.iter().position(|(l, p)| *p == Part::Sum && has(l, "cutoff")) {
        return Ok(Bright::Cutoff(i, "cutoff"));
    }
    if let Some(i) = lines.iter().position(|(l, p)| *p != Part::Sum && filtered(l)) {
        return Ok(Bright::Cutoff(i, "cutoff"));
    }
    if let Some(i) = lines.iter().position(|(l, p)| *p != Part::Sum && has(l, "lowpass")) {
        return Ok(Bright::Cutoff(i, "lowpass"));
    }
    let sines = lines.iter().filter(|(_, p)| *p != Part::Sum).all(|(l, _)| wave(l) == "sine");
    match (sines, lines.iter().any(|(_, p)| *p == Part::Sum)) {
        (true, true) => Ok(Bright::Drive),
        (true, false) => Err("A sine alone has nothing to brighten: DRIVE gives it overtones."),
        _ => Ok(Bright::Open),
    }
}

#[cfg(test)]
#[path = "instrument_tests.rs"]
mod tests;

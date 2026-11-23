//! A channel's notes moved by octaves and semitones in a song's text, as a
//! synthesizer transposes (the IDE's CHANNEL controls), and a channel
//! played alone. The text stays the truth: a move rewrites only the
//! notes, the octave commands and as few `<` and `>` as it needs, every
//! other byte kept; read back, the song plays exactly the same notes
//! moved, the same lengths and starts (`shift` checks it by reading both).
//!
//! The rules: an octave changes the octave commands (`o4` to `o5`), a
//! line with none before its first note gets one at its start; a
//! semitone writes each note anew in the line's own way, flats where the
//! line writes more flats than sharps, else sharps as the line writes
//! them (`#` or `+`, `+` where it writes none), naturals natural, and
//! where a note crosses between b and c the channel's octave follows by a
//! `>` or `<` before it (or one taken out where the line had the opposite
//! there), and back at the end where notes after it would hear it.

use super::parse::{note_freq, parse};
use super::song::Song;

/// What a move does: whole octaves (the notes' letters kept, only the
/// octaves change) or semitones (each note written anew).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Octaves(i32),
    Semitones(i32),
}

impl Move {
    fn semitones(self) -> i32 {
        match self {
            Move::Octaves(n) => 12 * n,
            Move::Semitones(n) => n,
        }
    }
}

/// The notes a move or PLAY CHANNEL works on: the channel lines picked,
/// whole, or the notes of a stretch selected within one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
    /// The channels' letters, in the order of their lines picked.
    pub letters: Vec<char>,
    /// The bytes of the text the notes picked start in.
    spans: Vec<(usize, usize)>,
    /// A selection of one channel's notes (PLAY CHANNEL plays only them),
    /// else whole channels.
    pub stretch: bool,
    /// A selection of only some of a line's notes (an octave then puts
    /// no `o` before them, but a `>` or `<`).
    within: bool,
}

/// A line of a song's text as the reader takes it.
#[derive(Clone, Debug)]
enum Line {
    /// A channel's: its letter and the bytes of its notes (after the
    /// colon, up to the comment).
    Channel(char, usize, usize),
    /// A macro (`$name = notes`).
    Macro(String, String),
    /// `#layer` or `#loop`.
    Layer,
    Loop,
    Other,
}

/// Each line of `text`: where it starts and ends (bytes, without its
/// line break) and what it is.
fn lines(text: &str) -> Vec<(usize, usize, Line)> {
    let mut out = Vec::new();
    let mut start = 0;
    for raw in text.split('\n') {
        let end = start + raw.len();
        let code_end = raw.find(';').unwrap_or(raw.len());
        let code = raw[..code_end].trim();
        let kind = if code.is_empty() || code.starts_with(['+', '=', '@']) {
            Line::Other
        } else if let Some(rest) = code.strip_prefix('#') {
            match rest.split_whitespace().next() {
                Some("layer") => Line::Layer,
                Some("loop") => Line::Loop,
                _ => Line::Other,
            }
        } else if let Some(rest) = code.strip_prefix('$') {
            match rest.split_once('=') {
                Some((name, body)) => Line::Macro(name.trim().to_string(), body.trim().to_string()),
                None => Line::Other,
            }
        } else {
            match code.split_once(':') {
                Some((letter, _)) if letter.trim().chars().count() == 1 => {
                    let colon = raw.find(':').unwrap_or(0);
                    Line::Channel(letter.trim().chars().next().unwrap_or('A'), start + colon + 1, start + code_end)
                }
                _ => Line::Other,
            }
        };
        out.push((start, end, kind));
        start = end + 1;
    }
    out
}

/// The notes picked by a cursor or a selection (bytes of `text`, `from`
/// to `to`): the cursor in a channel line, that line; a selection within
/// one channel line, the notes that start in it; a selection over several
/// lines, every channel line it touches. None where it is in no channel
/// line (or a selection holds no channel's notes).
pub fn pick(text: &str, from: usize, to: usize) -> Option<Picked> {
    let (from, to) = (from.min(to), from.max(to));
    let all = lines(text);
    let row_of = |at: usize| all.iter().position(|(s, e, _)| at >= *s && at <= *e).unwrap_or(all.len().saturating_sub(1));
    let first = row_of(from);
    // A selection ending at a line's start takes none of that line.
    let last = if to > from && all.get(row_of(to)).is_some_and(|(s, _, _)| *s == to) { row_of(to).saturating_sub(1).max(first) } else { row_of(to) };
    let mut picked = Picked { letters: Vec::new(), spans: Vec::new(), stretch: false, within: false };
    if first == last && from < to {
        let Line::Channel(letter, a, b) = all[first].2 else { return None };
        let notes = text[a..b].trim_end().len();
        picked.within = from > a + (text[a..b].len() - text[a..b].trim_start().len()) || to < a + notes;
        let (a, b) = (a.max(from), b.min(to));
        if a >= b {
            return None;
        }
        picked.letters.push(letter);
        picked.spans.push((a, b));
        picked.stretch = true;
        return Some(picked);
    }
    for (_, _, kind) in &all[first..=last] {
        if let Line::Channel(letter, a, b) = *kind {
            if !picked.letters.contains(&letter) {
                picked.letters.push(letter);
            }
            picked.spans.push((a, b));
        }
    }
    // A selection over lines of one channel plays as a stretch of it.
    picked.stretch = from < to && picked.letters.len() == 1;
    (!picked.spans.is_empty()).then_some(picked)
}

/// A character of a channel as read: where it stands in the text (bytes;
/// None for the space between two lines) and, for one a macro brought,
/// the `$` that used it.
#[derive(Clone, Copy, Debug)]
struct Ch {
    c: char,
    at: Option<usize>,
    from_macro: Option<usize>,
}

impl Ch {
    /// Where it stands for picking: its own place, or its macro's use.
    fn place(&self) -> Option<usize> {
        self.from_macro.or(self.at)
    }
}

/// What a channel's token is.
#[derive(Clone, Debug)]
enum K {
    /// A note: its semitone from C of the octave it is written in (its
    /// sharps and flats counted), whether its letter is a capital, and the
    /// letter with its signs (from, to in the channel's characters), then a
    /// tie's (`c4&c8`).
    Note { semi: i32, upper: bool, letters: Vec<(usize, usize)> },
    /// `o4`: the octave, its number's characters.
    Octave(i32, usize, usize),
    Up,
    Down,
    /// `[`, and the place of its `]` among the tokens.
    Open(usize),
    /// `]`, the place of its `[`, the times its body plays.
    Close(usize, u32),
    /// `l`, `v`, `q`, `p` or `t` and its number as written.
    Set(char, String),
    /// `@name`.
    At(String),
    Other,
}

#[derive(Clone, Debug)]
struct Tok {
    k: K,
    from: usize,
    to: usize,
}

/// A channel of the song: its characters (lines joined, macros expanded
/// as the reader expands them) and its tokens.
struct Chan {
    chars: Vec<Ch>,
    toks: Vec<Tok>,
}

impl Chan {
    /// Where character `i` stands in the text (bytes), None for one a
    /// macro brought.
    fn byte(&self, i: usize) -> Option<usize> {
        self.chars.get(i).and_then(|c| c.at)
    }

    /// Where character `i` ends in the text.
    fn end(&self, i: usize) -> Option<usize> {
        self.chars.get(i).and_then(|c| c.at.map(|b| b + c.c.len_utf8()))
    }

    /// Where token `t`'s text ends: its last character's end, or for one
    /// a macro brought, the end of the `$name` that used it.
    fn tok_end(&self, text: &str, t: usize) -> Option<usize> {
        let last = self.toks[t].to.checked_sub(1)?;
        self.end(last).or_else(|| {
            let use_at = self.chars.get(last)?.from_macro?;
            let name = text[use_at + 1..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').map(char::len_utf8).sum::<usize>();
            Some(use_at + 1 + name)
        })
    }
}

/// A macro's body with the macros in it expanded, as the reader does.
fn expand(body: &str, macros: &[(String, String)], depth: u32) -> Result<String, String> {
    if depth > 8 {
        return Err("macros nest too deep".into());
    }
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        while let Some(&n) = chars.peek().filter(|n| n.is_alphanumeric() || **n == '_') {
            name.push(n);
            chars.next();
        }
        let inner = macros.iter().rev().find(|(n, _)| *n == name).ok_or_else(|| format!("no macro ${name}"))?;
        out.push_str(&expand(&inner.1, macros, depth + 1)?);
    }
    Ok(out)
}

/// Channel `letter` of `text` read into characters and tokens.
fn channel(text: &str, all: &[(usize, usize, Line)], letter: char) -> Result<Chan, String> {
    let macros: Vec<(String, String)> = all.iter().filter_map(|(_, _, l)| if let Line::Macro(n, b) = l { Some((n.clone(), b.clone())) } else { None }).collect();
    let mut raw: Vec<(char, Option<usize>)> = Vec::new();
    for (_, _, line) in all {
        if let Line::Channel(l, a, b) = *line
            && l == letter
        {
            raw.push((' ', None));
            raw.extend(text[a..b].char_indices().map(|(i, c)| (c, Some(a + i))));
        }
    }
    let mut chars = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let (c, at) = raw[i];
        i += 1;
        if c != '$' {
            chars.push(Ch { c, at, from_macro: None });
            continue;
        }
        let mut name = String::new();
        while i < raw.len() && (raw[i].0.is_alphanumeric() || raw[i].0 == '_') {
            name.push(raw[i].0);
            i += 1;
        }
        let body = macros.iter().rev().find(|(n, _)| *n == name).ok_or_else(|| format!("no macro ${name}"))?;
        chars.extend(expand(&body.1, &macros, 1)?.chars().map(|c| Ch { c, at: None, from_macro: at }));
    }
    let toks = tokens(&chars)?;
    Ok(Chan { chars, toks })
}

/// The tokens of a channel's characters, read as the song's reader reads
/// them; a mistake where it would stop.
fn tokens(s: &[Ch]) -> Result<Vec<Tok>, String> {
    let at = |i: usize| s.get(i).map(|ch| ch.c);
    let number = |i: &mut usize| -> String {
        let start = *i;
        if at(*i) == Some('-') {
            *i += 1;
        }
        while at(*i).is_some_and(|c| c.is_ascii_digit()) {
            *i += 1;
        }
        if *i == start + 1 && at(start) == Some('-') {
            *i = start;
        }
        s[start..*i].iter().map(|ch| ch.c).collect()
    };
    let duration = |i: &mut usize| {
        number(i);
        while at(*i) == Some('.') {
            *i += 1;
        }
    };
    let signs = |i: &mut usize| -> i32 {
        let mut by = 0;
        while let Some(sign @ ('+' | '#' | '-')) = at(*i) {
            by += if sign == '-' { -1 } else { 1 };
            *i += 1;
        }
        by
    };
    let base = |c: char| match c.to_ascii_lowercase() {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        _ => 11,
    };
    let mut toks: Vec<Tok> = Vec::new();
    let mut opens: Vec<usize> = Vec::new();
    let mut i = 0;
    while let Some(c) = at(i) {
        let from = i;
        i += 1;
        let k = match c.to_ascii_lowercase() {
            ' ' | '\t' | '\n' | '\r' | '|' => continue,
            'a'..='g' => {
                let semi = base(c) + signs(&mut i);
                let mut letters = vec![(from, i)];
                duration(&mut i);
                while at(i) == Some('&') {
                    i += 1;
                    while at(i).is_some_and(|c| c.is_whitespace() || c == '|') {
                        i += 1;
                    }
                    match at(i) {
                        Some(n) if ('a'..='g').contains(&n.to_ascii_lowercase()) => {
                            let tie = i;
                            i += 1;
                            signs(&mut i);
                            letters.push((tie, i));
                        }
                        Some(n) if n.is_ascii_digit() || n == '.' => {}
                        _ => return Err("& ties a note to one of the same pitch or to a length".into()),
                    }
                    duration(&mut i);
                }
                K::Note { semi, upper: c.is_ascii_uppercase(), letters }
            }
            '"' => {
                let len = s[i..].iter().position(|ch| ch.c == '"').ok_or("a text without its closing quote")?;
                i += len + 1;
                K::Other
            }
            'r' => {
                duration(&mut i);
                K::Other
            }
            'o' => {
                let start = i;
                let n = number(&mut i);
                K::Octave(n.parse().map_err(|_| "o needs an octave, like o4")?, start, i)
            }
            '<' => K::Down,
            '>' => K::Up,
            set @ ('l' | 'v' | 'q' | 'p' | 't') => K::Set(set, number(&mut i)),
            '@' => {
                let start = i;
                while at(i).is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    i += 1;
                }
                K::At(s[start..i].iter().map(|ch| ch.c).collect())
            }
            '[' => {
                opens.push(toks.len());
                K::Open(0)
            }
            ']' => {
                let open = opens.pop().ok_or("] without [")?;
                let count = number(&mut i);
                let close = toks.len();
                toks[open] = Tok { k: K::Open(close), ..toks[open].clone() };
                K::Close(open, if count.is_empty() { 2 } else { count.parse().map_err(|_| "a repeat's count is a whole number")? })
            }
            other => return Err(format!("unknown command '{other}'")),
        };
        toks.push(Tok { k, from, to: i });
    }
    if !opens.is_empty() {
        return Err("[ without ]".into());
    }
    Ok(toks)
}

/// What a channel has set at a point: its octave, the `l`, `v`, `q`, `p`
/// and `t` as written, its instrument.
#[derive(Clone, Debug)]
struct State {
    octave: i32,
    sets: Vec<(char, String)>,
    at: Option<String>,
}

/// The most notes a walk through a channel's repeats takes.
const MOST_VISITS: usize = 4_000_000;

/// The channel's tokens `from` to `to` played as the reader plays them
/// (repeats as often as they say): each note's token and the octave it is
/// written in, every time it plays.
fn run(toks: &[Tok], from: usize, to: usize, state: &mut State, out: &mut Vec<(usize, i32)>) -> Result<(), String> {
    let mut i = from;
    while i < to {
        match &toks[i].k {
            K::Octave(o, ..) => state.octave = *o,
            K::Up => state.octave += 1,
            K::Down => state.octave -= 1,
            K::Note { .. } => {
                if out.len() >= MOST_VISITS {
                    return Err("too many notes to follow".into());
                }
                out.push((i, state.octave));
            }
            K::Set(c, v) => {
                state.sets.retain(|(s, _)| s != c);
                state.sets.push((*c, v.clone()));
            }
            K::At(name) => state.at = Some(name.clone()),
            K::Open(close) => {
                let K::Close(_, count) = toks[*close].k else { unreachable!("an open's partner closes") };
                for _ in 0..count.max(1) {
                    run(toks, i + 1, *close, state, out)?;
                }
                i = *close;
            }
            K::Close(..) | K::Other => {}
        }
        i += 1;
    }
    Ok(())
}

fn fresh() -> State {
    State { octave: 4, sets: Vec::new(), at: None }
}

/// The tokens of `chan` picked (their places, first to last), if any.
fn picked_toks(chan: &Chan, picked: &Picked) -> Option<(usize, usize)> {
    let inside = |t: &Tok| chan.chars[t.from].place().is_some_and(|p| picked.spans.iter().any(|(a, b)| p >= *a && p < *b));
    let first = chan.toks.iter().position(inside)?;
    let last = chan.toks.iter().rposition(inside)?;
    Some((first, last))
}

/// How deep in repeats token `t` stands.
fn depth(toks: &[Tok], t: usize) -> usize {
    toks[..t].iter().filter(|x| matches!(x.k, K::Open(close) if close >= t)).count()
}

/// A change to the text: the bytes `at` to `end` become `text`.
#[derive(Clone, Debug)]
struct Edit {
    at: usize,
    end: usize,
    text: String,
}

/// A song's text with a channel moved, and how its places moved.
#[derive(Clone, Debug)]
pub struct Shifted {
    pub text: String,
    /// Each change: where, the bytes it took, the bytes it put.
    changes: Vec<(usize, usize, usize)>,
}

impl Shifted {
    /// Where byte `at` of the old text is in the new; `before` for the
    /// start of something (text put right at it then follows it).
    pub fn map(&self, at: usize, before: bool) -> usize {
        let mut delta = 0i64;
        for &(c, took, put) in &self.changes {
            let ends = c + took;
            let moves = if took == 0 { if before { c < at } else { c <= at } } else { ends <= at };
            if moves {
                delta += put as i64 - took as i64;
            } else if c < at && at < ends {
                // Within what changed: as far in, at most its new end.
                return (c as i64 + delta + (at - c).min(put) as i64).max(0) as usize;
            }
        }
        (at as i64 + delta).max(0) as usize
    }
}

/// The notes' names written with sharps and with flats, from C.
const SHARPS: [(char, i32); 12] = [('c', 0), ('c', 1), ('d', 0), ('d', 1), ('e', 0), ('f', 0), ('f', 1), ('g', 0), ('g', 1), ('a', 0), ('a', 1), ('b', 0)];
const FLATS: [(char, i32); 12] = [('c', 0), ('d', -1), ('d', 0), ('e', -1), ('e', 0), ('f', 0), ('g', -1), ('g', 0), ('a', -1), ('a', 0), ('b', -1), ('b', 0)];

/// How a line writes its sharps and flats: flats where it writes more
/// flats than sharps; else sharps, `#` where it writes more `#` than `+`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    Plus,
    Hash,
    Flat,
}

/// The pitch class `pc` (0 C to 11 B) written in `style`, its letter
/// a capital where `upper`.
fn spelled(pc: i32, style: Style, upper: bool) -> String {
    let (letter, sign) = if style == Style::Flat { FLATS[pc as usize] } else { SHARPS[pc as usize] };
    let letter = if upper { letter.to_ascii_uppercase() } else { letter };
    let sign = match (sign, style) {
        (0, _) => "",
        (-1, _) => "-",
        (_, Style::Hash) => "#",
        _ => "+",
    };
    format!("{letter}{sign}")
}

/// A note as a person reads it: `c#4`.
fn named(midi: i32) -> String {
    format!("{}{}", spelled(midi.rem_euclid(12), Style::Hash, false), midi.div_euclid(12) - 1)
}

/// Each channel picked: whether each note it plays (every time it plays)
/// is one of those moved.
type Marks = Vec<(char, Vec<bool>)>;

/// The changes `mv` makes to the notes `picked` of `text`, and which notes
/// they move, or why it cannot (a channel that does not read, a macro's
/// notes among them, a note past octave 0 or 9).
fn plan(text: &str, picked: &Picked, mv: Move) -> Result<(Vec<Edit>, Marks), String> {
    let all = lines(text);
    let k = mv.semitones();
    let mut edits = Vec::new();
    let mut flags = Vec::new();
    let (mut low, mut high): (Option<i32>, Option<i32>) = (None, None);
    for &letter in &picked.letters {
        let chan = channel(text, &all, letter).map_err(|e| format!("channel {letter}: {e}"))?;
        let toks = &chan.toks;
        let Some((first, last)) = picked_toks(&chan, picked) else { continue };
        let mut visits = Vec::new();
        run(toks, 0, toks.len(), &mut fresh(), &mut visits)?;
        flags.push((letter, visits.iter().map(|(t, _)| (first..=last).contains(t)).collect::<Vec<bool>>()));
        // Where the notes picked would sound, and the octave written.
        for &(t, octave) in visits.iter().filter(|(t, _)| (first..=last).contains(t)) {
            let K::Note { semi, .. } = toks[t].k else { continue };
            let written = octave + need(semi, k, mv);
            if !(0..=9).contains(&written) {
                let now = 12 * (octave + 1) + semi;
                return Err(if written > 9 { format!("{} has no octave above: MML's octaves go from 0 to 9.", named(now)) } else { format!("{} has no octave below: MML's octaves go from 0 to 9.", named(now)) });
            }
            let now = 12 * (octave + 1) + semi;
            low = Some(low.map_or(now, |l| l.min(now)));
            high = Some(high.map_or(now, |h| h.max(now)));
        }
        edits.extend(channel_edits(text, &chan, first, last, mv, picked.within)?);
    }
    if low.is_none() {
        return Err("No notes here to move.".into());
    }
    Ok((edits, flags))
}

/// How many octaves the channel's octave must stand higher at a note of
/// semitone `semi` (from C of the octave it is written in) moved by `k`.
fn need(semi: i32, k: i32, mv: Move) -> i32 {
    match mv {
        Move::Octaves(n) => n,
        Move::Semitones(_) => (semi + k).div_euclid(12),
    }
}

/// The changes to channel `chan`'s tokens `first` to `last` (see the
/// module's rules).
fn channel_edits(text: &str, chan: &Chan, first: usize, last: usize, mv: Move, within: bool) -> Result<Vec<Edit>, String> {
    let toks = &chan.toks;
    let k = mv.semitones();
    let mut walk = Walk {
        text,
        chan,
        octaves: matches!(mv, Move::Octaves(_)) && depth(toks, first) == 0 && !within,
        start_octave: {
            let mut state = fresh();
            run(toks, 0, first, &mut state, &mut Vec::new())?;
            state.octave
        },
        start: chan.chars[toks[first].from].place().unwrap_or(0),
        off: 0,
        pending_o: None,
        pending_rel: Vec::new(),
        edits: Vec::new(),
    };
    let mut stack: Vec<i32> = Vec::new();
    let (mut seen_note, mut seen_o) = (false, false);
    for t in first..=last {
        let tok = &toks[t];
        let editable = chan.chars[tok.from].from_macro.is_none();
        match &tok.k {
            K::Octave(..) => {
                seen_o = true;
                walk.pending_rel.clear();
                walk.pending_o = editable.then_some(t);
                walk.off = 0;
            }
            K::Up | K::Down if editable => walk.pending_rel.push(t),
            K::Note { semi, upper, letters } => {
                if !editable {
                    return Err("A macro ($name) plays notes here: move the notes on its own line.".into());
                }
                let at = chan.byte(letters[0].0).ok_or("a note without its place")?;
                walk.settle(need(*semi, k, mv), at, false, !seen_note && !seen_o);
                if let Move::Semitones(_) = mv {
                    let style = style_of(text, chan, at);
                    for (i, &(a, b)) in letters.iter().enumerate() {
                        let upper = if i == 0 { *upper } else { chan.chars[a].c.is_ascii_uppercase() };
                        let (Some(a), Some(b)) = (chan.byte(a), chan.end(b - 1)) else { return Err("a tie across a macro".into()) };
                        walk.edits.push(Edit { at: a, end: b, text: spelled((semi + k).rem_euclid(12), style, upper) });
                    }
                }
                walk.pending_o = None;
                walk.pending_rel.clear();
                seen_note = true;
            }
            K::Open(close) => {
                let at = chan.byte(tok.from).ok_or("a repeat in a macro")?;
                // A repeat whose `]` is picked too begins and ends at the
                // same difference; else it must be none.
                if *close <= last {
                    // An octave: the `o` before it changed (or one put at
                    // the start) as for the first note in it.
                    let note = toks[t + 1..*close].iter().find_map(|x| if let K::Note { semi, .. } = x.k { Some(semi) } else { None });
                    if let (Move::Octaves(_), Some(semi)) = (mv, note)
                        && (walk.pending_o.is_some() || (!seen_note && !seen_o && walk.octaves))
                    {
                        walk.settle(need(semi, k, mv), at, false, !seen_note && !seen_o);
                    }
                    stack.push(walk.off);
                } else {
                    walk.settle(0, at, false, false);
                    stack.push(0);
                }
                walk.pending_o = None;
                walk.pending_rel.clear();
            }
            K::Close(open, _) => {
                let at = chan.byte(tok.from).ok_or("a repeat in a macro")?;
                let want = if *open >= first { stack.pop().unwrap_or(0) } else { 0 };
                walk.settle(want, at, false, false);
                walk.pending_o = None;
                walk.pending_rel.clear();
            }
            _ => {}
        }
    }
    // Back where notes after the ones picked would hear the difference.
    let next = toks[last + 1..].iter().find(|t| matches!(t.k, K::Note { .. } | K::Octave(..) | K::Open(_) | K::Close(..)));
    if next.is_some_and(|t| !matches!(t.k, K::Octave(..))) && walk.off != 0 {
        let at = chan.end(toks[last].to - 1).ok_or("the notes picked end in a macro")?;
        walk.settle(0, at, true, false);
    }
    Ok(walk.edits)
}

/// The line's style of sharps and flats, by the line `at` (a byte) is on.
fn style_of(text: &str, chan: &Chan, at: usize) -> Style {
    let (start, end) = (text[..at].rfind('\n').map_or(0, |n| n + 1), text[at..].find('\n').map_or(text.len(), |n| at + n));
    let (mut plus, mut hash, mut flat) = (0, 0, 0);
    for t in chan.toks.iter().filter(|t| chan.byte(t.from).is_some_and(|b| b >= start && b < end)) {
        if let K::Note { letters, .. } = &t.k {
            for ch in &chan.chars[letters[0].0 + 1..letters[0].1] {
                match ch.c {
                    '+' => plus += 1,
                    '#' => hash += 1,
                    _ => flat += 1,
                }
            }
        }
    }
    if flat > plus + hash {
        Style::Flat
    } else if hash > plus {
        Style::Hash
    } else {
        Style::Plus
    }
}

/// A walk over the tokens picked, writing the changes: the channel's
/// octave in the new text less the old's (`off`), and what was put off
/// since the last note to bring it where the next one wants it (an `o` to
/// change, `<` and `>` to take out).
struct Walk<'a> {
    text: &'a str,
    chan: &'a Chan,
    /// Whole octaves moved, the notes picked not in a repeat: an `o` may
    /// be put at their start.
    octaves: bool,
    /// The old octave where the notes picked start, and that place.
    start_octave: i32,
    start: usize,
    off: i32,
    pending_o: Option<usize>,
    pending_rel: Vec<usize>,
    edits: Vec<Edit>,
}

impl Walk<'_> {
    /// The difference brought to `want` before byte `at` (or, `after`,
    /// right after it) by the cleanest means there is: an `o` since the
    /// last note changed; an `o` put at the start (`may_put_o`: before the
    /// first note, with no `o` before it); a `<` or `>` since the last
    /// note that went the other way taken out; else `>` or `<` put.
    fn settle(&mut self, want: i32, at: usize, after: bool, may_put_o: bool) {
        let mut change = want - self.off;
        if change == 0 {
            return;
        }
        let toks = &self.chan.toks;
        if let Some(t) = self.pending_o
            && let K::Octave(n, a, b) = toks[t].k
            && (0..=9).contains(&(n + want))
            && let (Some(a), Some(b)) = (self.chan.byte(a), self.chan.end(b.saturating_sub(1)))
        {
            self.edits.push(Edit { at: a, end: b, text: (n + want).to_string() });
            self.off = want;
            return;
        }
        if may_put_o && self.octaves && (0..=9).contains(&(self.start_octave + want)) {
            self.edits.push(Edit { at: self.start, end: self.start, text: format!("o{} ", self.start_octave + want) });
            self.off = want;
            return;
        }
        while change != 0 {
            let other: fn(&K) -> bool = if change > 0 { |k| matches!(k, K::Down) } else { |k| matches!(k, K::Up) };
            let Some(p) = self.pending_rel.iter().rposition(|&t| other(&toks[t].k)) else { break };
            let t = self.pending_rel.remove(p);
            let Some(b) = self.chan.byte(toks[t].from) else { break };
            // With the space after it, where one stands before it too.
            let before = self.text[..b].chars().next_back();
            let space_after = self.text[b + 1..].starts_with(' ') && before.is_none_or(|c| c == ' ' || c == ':');
            self.edits.push(Edit { at: b, end: b + 1 + usize::from(space_after), text: String::new() });
            change -= change.signum();
        }
        if change != 0 {
            let signs = if change > 0 { ">" } else { "<" }.repeat(change.unsigned_abs() as usize);
            self.edits.push(Edit { at, end: at, text: if after { format!(" {signs}") } else { signs } });
        }
        self.off = want;
    }
}

/// The text with `edits` made (none overlapping), and how its places moved.
fn apply(text: &str, mut edits: Vec<Edit>) -> Result<Shifted, String> {
    // Text put at a place comes before a change that starts there.
    edits.sort_by_key(|e| (e.at, e.end > e.at));
    let mut out = String::with_capacity(text.len() + 16);
    let mut from = 0;
    let mut changes = Vec::new();
    for e in &edits {
        if e.at < from {
            return Err("The move would change a place twice: nothing was changed.".into());
        }
        out.push_str(&text[from..e.at]);
        out.push_str(&e.text);
        from = e.end;
        changes.push((e.at, e.end - e.at, e.text.len()));
    }
    out.push_str(&text[from..]);
    Ok(Shifted { text: out, changes })
}

/// Whether `mv` can move the notes `picked`, and if not, why.
pub fn can(text: &str, picked: &Picked, mv: Move) -> Result<(), String> {
    plan(text, picked, mv).map(|_| ())
}

/// The notes `picked` of the song `text` moved by `mv` (see the module's
/// rules): the new text, checked by reading both songs: every note picked
/// sounds moved, every other note as it was, at the same start and
/// length. Or why not.
pub fn shift(text: &str, picked: &Picked, mv: Move) -> Result<Shifted, String> {
    let (edits, flags) = plan(text, picked, mv)?;
    let shifted = apply(text, edits)?;
    let old = parse(text).map_err(|e| format!("{e:#}"))?;
    let new = parse(&shifted.text).map_err(|e| format!("Moved, the song would not read: {e:#}"))?;
    same_but_moved(&old, &new, &flags, mv.semitones())?;
    Ok(shifted)
}

/// The pitch `freq` moved by `k` semitones, as the reader makes it.
fn moved(freq: f32, k: i32) -> f32 {
    let midi = (69.0 + 12.0 * libm::log2f(freq / 440.0)).round() as i32 + k;
    note_freq(midi.div_euclid(12) - 1, midi.rem_euclid(12))
}

/// Whether `new` plays what `old` does with the notes `flags` marks (by
/// channel, every time each note plays) moved by `k` semitones.
fn same_but_moved(old: &Song, new: &Song, flags: &[(char, Vec<bool>)], k: i32) -> Result<(), String> {
    let wrong = |what: &str| Err(format!("The move would change more than the notes ({what}): nothing was changed."));
    if old.names != new.names {
        return wrong("the channels");
    }
    for (c, letter) in old.names.iter().enumerate() {
        let (a, b) = (&old.channels[c], &new.channels[c]);
        if a.len() != b.len() {
            return wrong("how many notes");
        }
        let marks = flags.iter().find(|(l, _)| l == letter).map(|(_, f)| f.as_slice());
        if marks.is_some_and(|f| f.len() + 1 != a.len()) {
            return wrong("the notes followed");
        }
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            let picked = marks.and_then(|f| f.get(i)).copied().unwrap_or(false);
            let freq = if picked { x.freq.map(|f| moved(f, k)) } else { x.freq };
            if (x.start, x.held, x.instrument, x.volume, x.pan, x.sung) != (y.start, y.held, y.instrument, y.volume, y.pan, y.sung) {
                return wrong("a note's time");
            }
            if freq != y.freq {
                return wrong("a note's pitch");
            }
        }
    }
    Ok(())
}

/// A song of the channels `picked` alone (or the stretch selected of one,
/// as it stood set there: its octave, instrument, lengths, volume), the
/// song's instruments, settings and macros, every other channel and the
/// `#layer`s left out: what PLAY CHANNEL plays, once or (`round`) round
/// and round.
pub fn alone(text: &str, picked: &Picked, round: bool) -> Result<String, String> {
    let all = lines(text);
    let mut out = String::new();
    if !round {
        out.push_str("#loop no\n");
    }
    for (a, b, kind) in &all {
        let keep = match kind {
            Line::Channel(l, ..) => !picked.stretch && picked.letters.contains(l),
            Line::Layer => false,
            Line::Loop => round && !picked.stretch,
            _ => true,
        };
        if keep {
            out.push_str(&text[*a..*b]);
            out.push('\n');
        }
    }
    if picked.stretch {
        let letter = picked.letters[0];
        let chan = channel(text, &all, letter).map_err(|e| format!("channel {letter}: {e}"))?;
        let (first, last) = picked_toks(&chan, picked).ok_or("No notes selected.")?;
        let cuts = (first..=last).any(|t| match chan.toks[t].k {
            K::Open(close) => close > last,
            K::Close(open, _) => open < first,
            _ => false,
        });
        if cuts || depth(&chan.toks, first) > 0 {
            return Err("The selection cuts a repeat ([ ]): select it whole, or none of it.".into());
        }
        let mut state = fresh();
        run(&chan.toks, 0, first, &mut state, &mut Vec::new())?;
        let mut line = format!("{letter}:");
        for (c, v) in state.sets.iter().filter(|(c, _)| *c == 't') {
            line.push_str(&format!(" {c}{v}"));
        }
        if let Some(name) = &state.at {
            line.push_str(&format!(" @{name}"));
        }
        line.push_str(&format!(" o{}", state.octave.clamp(0, 9)));
        for (c, v) in state.sets.iter().filter(|(c, _)| *c != 't') {
            line.push_str(&format!(" {c}{v}"));
        }
        // The stretch, line by line.
        let from = chan.chars[chan.toks[first].from].place().unwrap_or(0);
        let to = chan.tok_end(text, last).unwrap_or(from);
        for (_, _, kind) in &all {
            if let Line::Channel(l, a, b) = *kind
                && l == letter
            {
                let (a, b) = (a.max(from), b.min(to));
                if a < b {
                    line.push(' ');
                    line.push_str(&text[a..b]);
                }
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    // As it reads, or with the song's `#loop` left out where the channel
    // alone does not reach it.
    match parse(&out) {
        Ok(_) => Ok(out),
        Err(_) if round && all.iter().any(|(_, _, k)| matches!(k, Line::Loop)) => {
            let without: String = out.split_inclusive('\n').filter(|l| !l.trim_start().starts_with("#loop")).collect();
            parse(&without).map(|_| without).map_err(|e| format!("{e:#}"))
        }
        Err(e) => Err(format!("{e:#}")),
    }
}

#[cfg(test)]
#[path = "shift_tests.rs"]
mod tests;

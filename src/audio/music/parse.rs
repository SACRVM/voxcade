//! MML text to a `Song`: instruments, macros, repeats and the channels' notes.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use super::song::{FilterMode, Instrument, Note, Song};
use super::{SAMPLE_RATE, Wave, words};

/// The most a song may ask of the reader: notes in a channel, steps of
/// reading in one (repeats multiply), characters of a channel after its
/// macros are expanded, repeats of one `[ ]`, beats a minute, note lengths
/// and octaves.
const MOST_NOTES: usize = 200_000;
const MOST_STEPS: usize = 20_000_000;
const MOST_TEXT: usize = 1 << 20;
const MOST_REPEATS: f32 = 999.0;
const TEMPOS: (f32, f32) = (1.0, 999.0);
const LENGTHS: (f32, f32) = (1.0, 192.0);
const OCTAVES: (i32, i32) = (0, 9);

/// Frequency of a note: octave 4's A is 440 Hz.
pub(super) fn note_freq(octave: i32, semitone: i32) -> f32 {
    let midi = (octave + 1) * 12 + semitone;
    440.0 * 2f32.powf((midi - 69) as f32 / 12.0)
}

fn parse_instrument(spec: &str) -> Result<Instrument> {
    let mut ins = Instrument::default();
    for word in spec.split_whitespace() {
        let (key, value) = match word.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (word, None),
        };
        let num = || -> Result<f32> {
            let v = value.with_context(|| format!("{key} needs a value"))?.parse::<f32>().with_context(|| format!("{key}={}", value.unwrap_or("")))?;
            if !v.is_finite() {
                bail!("{key}={} is no number", value.unwrap_or(""));
            }
            if let Some(u) = words::word(key).and_then(|w| w.mml.as_ref())
                && let Some((lo, hi)) = u.range
                && (f64::from(v) < lo || f64::from(v) > hi)
            {
                bail!("{key}={} is out of range, {}", value.unwrap_or(""), words::range_words(u));
            }
            Ok(v)
        };
        match key {
            "square" | "pulse" => ins.wave = Wave::Square,
            "saw" => ins.wave = Wave::Saw,
            "triangle" => ins.wave = Wave::Triangle,
            "sine" => ins.wave = Wave::Sine,
            "noise" => ins.wave = Wave::Noise,
            "duty" => ins.duty = num()?,
            "pwm" => ins.pwm = num()?,
            "pwm_speed" => ins.pwm_speed = num()?,
            "attack" => ins.attack = num()?,
            "decay" => ins.decay = num()?,
            "sustain" => ins.sustain = num()?,
            "release" => ins.release = num()?,
            "vibrato" => ins.vibrato = num()?,
            "vibrato_speed" => ins.vibrato_speed = num()?,
            "vibrato_delay" => ins.vibrato_delay = num()?,
            "arp" => {
                ins.arp = value
                    .context("arp needs semitones like 0,4,7")?
                    .split(',')
                    .map(|n| n.trim().parse::<f32>().ok().filter(|v| v.is_finite() && v.abs() <= 120.0).with_context(|| format!("arp step '{n}' is not semitones from -120 to 120")))
                    .collect::<Result<_>>()?
            }
            "arp_speed" => ins.arp_speed = num()?,
            "sweep" => ins.sweep = num()?,
            "sweep_time" => ins.sweep_time = Some(num()?),
            "freq" => ins.freq = Some(num()?),
            "lowpass" => ins.lowpass = Some(num()?),
            "filter" => {
                ins.filter = Some(match value.context("filter needs lowpass, bandpass or highpass")? {
                    "lowpass" | "low" => FilterMode::Low,
                    "bandpass" | "band" => FilterMode::Band,
                    "highpass" | "high" => FilterMode::High,
                    other => bail!("filter={other}: lowpass, bandpass or highpass"),
                })
            }
            "cutoff" => {
                ins.cutoff = num()?.max(20.0);
                ins.filter.get_or_insert(FilterMode::Low);
            }
            "cutoff_end" => {
                ins.cutoff_end = Some(num()?.max(20.0));
                ins.filter.get_or_insert(FilterMode::Low);
            }
            "cutoff_time" => ins.cutoff_time = Some(num()?),
            "resonance" => {
                ins.resonance = num()?.clamp(0.0, 1.0);
                ins.filter.get_or_insert(FilterMode::Low);
            }
            "sidechain" => ins.sidechain = num()?.clamp(0.0, 1.0),
            "sidechain_release" => ins.sidechain_release = num()?,
            "duck" => ins.duck = num()?.clamp(0.0, 1.0),
            "volume" => ins.volume = num()?,
            "wave" => bail!("the wave is a bare word, not wave=...: square, pulse, saw, triangle, sine or noise (like @lead square duty=0.25)"),
            other => {
                let known = words::WORDS.iter().filter(|w| w.mml.is_some() && w.name != "wave" && w.name != other).map(|w| w.name).chain(["pulse"]).chain(Wave::ALL.iter().map(|(n, _)| *n)).filter(|w| *w != other);
                match words::near(other, known) {
                    Some(near) => bail!("unknown instrument option '{other}': did you mean '{near}'?"),
                    None => bail!("unknown instrument option '{other}'"),
                }
            }
        }
    }
    Ok(ins)
}

/// Expands `$name` macros (up to a depth, so a macro cannot loop forever).
fn expand(text: &str, macros: &HashMap<String, String>, depth: u32) -> Result<String> {
    let out = expand_in(text, macros, depth)?;
    if out.len() > MOST_TEXT {
        bail!("macros grow past {MOST_TEXT} characters");
    }
    Ok(out)
}

fn expand_in(text: &str, macros: &HashMap<String, String>, depth: u32) -> Result<String> {
    if depth > 8 {
        bail!("macros nest too deep");
    }
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        while let Some(&n) = chars.peek() {
            if n.is_alphanumeric() || n == '_' {
                name.push(n);
                chars.next();
            } else {
                break;
            }
        }
        let body = macros.get(&name).with_context(|| format!("no macro ${name}"))?;
        out.push_str(&expand(body, macros, depth + 1)?);
        if out.len() > MOST_TEXT {
            bail!("macros grow past {MOST_TEXT} characters");
        }
    }
    Ok(out)
}

/// A channel's text with its macros expanded, each character with the line
/// of the file it came from (a macro's: the line that uses it).
fn expand_lines(text: &[(char, usize)], macros: &HashMap<String, String>) -> Result<(Vec<char>, Vec<usize>)> {
    let (mut chars, mut lines) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < text.len() {
        let (c, line) = text[i];
        i += 1;
        let piece = if c == '$' {
            let mut name = String::new();
            while i < text.len() && (text[i].0.is_alphanumeric() || text[i].0 == '_') {
                name.push(text[i].0);
                i += 1;
            }
            let body = macros.get(&name).with_context(|| format!("line {line}: no macro ${name}"))?;
            expand(body, macros, 1).with_context(|| format!("line {line}: macro ${name}"))?
        } else {
            c.to_string()
        };
        for ch in piece.chars() {
            chars.push(ch);
            lines.push(line);
        }
        if chars.len() > MOST_TEXT {
            bail!("line {line}: the channel grows past {MOST_TEXT} characters");
        }
    }
    Ok((chars, lines))
}

/// What a channel line has set so far.
struct State {
    octave: i32,
    length: f32,
    volume: f32,
    gate: f32,
    pan: f32,
    tempo: f32,
    instrument: usize,
    time: f64,
}

struct Parser<'a> {
    chars: Vec<char>,
    /// The line of the song's file each of `chars` came from (a macro's
    /// characters: the line that uses it).
    lines: Vec<usize>,
    at: usize,
    /// Steps of reading so far, repeats counted every pass.
    steps: usize,
    instruments: &'a HashMap<String, usize>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn number(&mut self) -> Option<f32> {
        let start = self.at;
        if self.peek() == Some('-') {
            self.at += 1;
        }
        // Whole numbers only: a dot after a length makes a dotted note.
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        let text: String = self.chars[start..self.at].iter().collect();
        if text.is_empty() || text == "-" {
            self.at = start;
            return None;
        }
        text.parse().ok()
    }

    /// A note length (`4`, `8.`), in beats; without a number the default.
    fn duration(&mut self, state: &State) -> Result<f32> {
        let length = match self.number() {
            Some(n) if (LENGTHS.0..=LENGTHS.1).contains(&n) => n,
            Some(n) => bail!("a note length of {n} is out of range, {} to {}", LENGTHS.0, LENGTHS.1),
            None => state.length,
        };
        let mut beats = 4.0 / length;
        let mut dot = beats / 2.0;
        while self.peek() == Some('.') {
            self.at += 1;
            beats += dot;
            dot /= 2.0;
        }
        Ok(beats)
    }

    /// A number that must lie within `range`, or a mistake saying so.
    fn within(&mut self, what: &str, range: (f32, f32)) -> Result<f32> {
        let v = self.number().with_context(|| format!("{what} needs a number from {} to {}", range.0, range.1))?;
        if v < range.0 || v > range.1 {
            bail!("{what} is {v}: from {} to {}", range.0, range.1);
        }
        Ok(v)
    }

    /// The semitone of the note letter `c` (just read) with the sharps
    /// (`+`, `#`) and flats (`-`) after it: `c++` a double sharp, `b--` a
    /// double flat, two at most.
    fn pitch(&mut self, c: char) -> Result<i32> {
        let mut semitone = match c.to_ascii_lowercase() {
            'c' => 0,
            'd' => 2,
            'e' => 4,
            'f' => 5,
            'g' => 7,
            'a' => 9,
            _ => 11,
        };
        let mut signs = 0;
        while let Some(sign @ ('+' | '#' | '-')) = self.peek() {
            signs += 1;
            if signs > 2 {
                bail!("a note takes two sharps or two flats at most (c++, b--)");
            }
            semitone += if sign == '-' { -1 } else { 1 };
            self.at += 1;
        }
        Ok(semitone)
    }

    fn octave(&self, octave: i32) -> Result<i32> {
        if octave < OCTAVES.0 || octave > OCTAVES.1 {
            bail!("octave {octave} is out of range, {} to {}", OCTAVES.0, OCTAVES.1);
        }
        Ok(octave)
    }

    fn sequence(&mut self, state: &mut State, notes: &mut Vec<Note>, until: Option<char>) -> Result<()> {
        let rate = SAMPLE_RATE as f64;
        while let Some(c) = self.peek() {
            if Some(c) == until {
                return Ok(());
            }
            self.at += 1;
            self.steps += 1;
            if self.steps > MOST_STEPS {
                bail!("this channel asks for too much reading (repeats inside repeats)");
            }
            match c.to_ascii_lowercase() {
                ' ' | '\t' | '\n' | '\r' | '|' => {}
                // A note, in small letters or capitals (`C D E` as `c d e`).
                'a'..='g' => {
                    if notes.len() >= MOST_NOTES {
                        bail!("more than {MOST_NOTES} notes in one channel");
                    }
                    let note_octave = self.octave(state.octave)?;
                    let semitone = self.pitch(c)?;
                    let mut beats = self.duration(state)?;
                    // `&` ties the following note's length on: the same
                    // pitch held, or a length alone (`c4&8`).
                    while self.peek() == Some('&') {
                        self.at += 1;
                        // A tie may run over a bar line.
                        while self.peek().is_some_and(|c| c.is_whitespace() || c == '|') {
                            self.at += 1;
                        }
                        match self.peek() {
                            Some(n) if ('a'..='g').contains(&n.to_ascii_lowercase()) => {
                                self.at += 1;
                                if self.pitch(n)? != semitone {
                                    bail!("& ties a note to one of the same pitch (c4&c8, or c4&8); to go on to another pitch, write it without & (c4 d4)");
                                }
                            }
                            Some(n) if n.is_ascii_digit() || n == '.' => {}
                            _ => bail!("& ties a note to one of the same pitch (c4&c8) or to a length (c4&8); nothing else may stand between"),
                        }
                        beats += self.duration(state)?;
                    }
                    let seconds = beats as f64 * 60.0 / state.tempo as f64;
                    notes.push(Note {
                        start: (state.time * rate) as u64,
                        held: (seconds * state.gate as f64 * rate) as u64,
                        freq: Some(note_freq(note_octave, semitone)),
                        instrument: state.instrument,
                        volume: state.volume,
                        pan: state.pan,
                    });
                    state.time += seconds;
                }
                'r' => {
                    let beats = self.duration(state)?;
                    state.time += beats as f64 * 60.0 / state.tempo as f64;
                }
                'o' => {
                    let o = self.number().context("o needs an octave, like o4")?;
                    state.octave = self.octave(o as i32)?;
                }
                '<' => state.octave = self.octave(state.octave - 1)?,
                '>' => state.octave = self.octave(state.octave + 1)?,
                'l' => state.length = self.within("l (the default note length)", LENGTHS)?,
                'v' => state.volume = self.within("v (volume)", (0.0, 15.0))? / 15.0,
                'q' => state.gate = self.within("q (how much of a note sounds)", (1.0, 8.0))? / 8.0,
                'p' => state.pan = self.within("p (pan)", (-100.0, 100.0))? / 100.0,
                't' => state.tempo = self.within("t (tempo)", TEMPOS)?,
                '@' => {
                    let start = self.at;
                    while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        self.at += 1;
                    }
                    let name: String = self.chars[start..self.at].iter().collect();
                    state.instrument = *self.instruments.get(&name).with_context(|| format!("no instrument @{name}"))?;
                }
                '[' => {
                    let start = self.at;
                    let mut times = 1;
                    let mut first = true;
                    // Parse the body once to find where it ends, then as often as asked.
                    loop {
                        self.at = start;
                        self.sequence(state, notes, Some(']'))?;
                        if self.peek() != Some(']') {
                            bail!("[ without ]");
                        }
                        self.at += 1;
                        if first {
                            let count = self.number().unwrap_or(2.0);
                            if !(1.0..=MOST_REPEATS).contains(&count) {
                                bail!("a repeat of {count} is out of range, 1 to {MOST_REPEATS}");
                            }
                            times = count as u32;
                            first = false;
                        }
                        times -= 1;
                        if times == 0 {
                            break;
                        }
                    }
                    // Skip the count after the last pass.
                    self.number();
                }
                ']' => bail!("] without ["),
                _ => bail!("unknown command '{c}'"),
            }
        }
        if until.is_some() {
            bail!("[ without ]");
        }
        Ok(())
    }
}

pub fn parse(text: &str) -> Result<Song> {
    parse_song(text, false)
}

pub(super) fn parse_song(text: &str, may_be_empty: bool) -> Result<Song> {
    let mut title = String::new();
    let mut tempo = 120.0;
    let mut meter = 4;
    let mut looping = true;
    let mut layered: Vec<(char, f32)> = Vec::new();
    let mut instruments = Vec::new();
    let mut names = HashMap::new();
    let mut macros = HashMap::new();
    let mut lines: Vec<(char, String, usize)> = Vec::new();

    for (n, raw) in text.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = || format!("line {}", n + 1);
        if let Some(rest) = line.strip_prefix('#') {
            let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            match key {
                "title" => title = value.trim().to_string(),
                "tempo" => {
                    tempo = value.trim().parse::<f32>().ok().filter(|t| (TEMPOS.0..=TEMPOS.1).contains(t)).with_context(|| format!("{}: #tempo is the beats a minute, {} to {} (not '{}')", at(), TEMPOS.0, TEMPOS.1, value.trim()))?
                }
                "layer" => {
                    // Channel letters, then the tension they sound from.
                    let words: Vec<&str> = value.split_whitespace().collect();
                    let (last, letters) = words.split_last().with_context(|| format!("{}: #layer is channels and a tension, like D C 0.35", at()))?;
                    let from: f32 = last.parse().ok().filter(|v: &f32| (0.0..=1.0).contains(v)).with_context(|| format!("{}: #layer ends with the tension it sounds from, 0 to 1 (not {last})", at()))?;
                    if letters.is_empty() {
                        bail!("{}: #layer names its channels first, like D C 0.35", at());
                    }
                    for l in letters {
                        let mut chars = l.chars();
                        match (chars.next(), chars.next()) {
                            (Some(c), None) => layered.push((c, from)),
                            _ => bail!("{}: #layer: '{l}' is no channel (a letter)", at()),
                        }
                    }
                }
                "meter" => {
                    // 3, or 3/4: the beats in a bar.
                    let beats = value.trim().split('/').next().unwrap_or("");
                    meter = beats.trim().parse::<u32>().ok().filter(|&n| n > 0).with_context(|| format!("{}: #meter is the beats in a bar, like 4 or 3/4", at()))?;
                }
                "loop" => looping = !matches!(value.trim(), "no" | "off" | "false"),
                "inst" | "instrument" => bail!("{}: an instrument is written @name options, like @lead square duty=0.25 (not #{key})", at()),
                other => bail!("{}: unknown #{other}", at()),
            }
        } else if let Some(rest) = line.strip_prefix('@') {
            let (name, spec) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let ins = parse_instrument(spec).with_context(|| format!("{}: @{name}", at()))?;
            names.insert(name.to_string(), instruments.len());
            instruments.push(ins);
        } else if let Some(rest) = line.strip_prefix('$') {
            let (name, body) = rest.split_once('=').with_context(|| format!("{}: a macro is $name = notes", at()))?;
            macros.insert(name.trim().to_string(), body.trim().to_string());
        } else if let Some((channel, notes)) = line.split_once(':')
            && channel.trim().chars().count() == 1
        {
            let letter = channel.trim().chars().next().unwrap();
            if !letter.is_ascii_alphabetic() {
                bail!("line {}: a channel is a letter, A to Z (not '{letter}')", n + 1);
            }
            if notes.trim().is_empty() {
                bail!("line {}: channel {letter} has no notes", n + 1);
            }
            lines.push((letter, notes.to_string(), n + 1));
        } else {
            bail!("line {}: expected #setting, @instrument, $macro or a channel like 'A: c d e'", n + 1);
        }
    }
    if lines.is_empty() && !may_be_empty {
        bail!("the song has no channel: write one like 'A: c d e f' (a letter, a colon, notes)");
    }
    if instruments.is_empty() {
        instruments.push(Instrument::default());
        names.insert("default".into(), 0);
    }

    // Lines of the same channel continue each other.
    let mut order: Vec<char> = Vec::new();
    let mut texts: HashMap<char, Vec<(char, usize)>> = HashMap::new();
    for (channel, notes, n) in lines {
        if !order.contains(&channel) {
            order.push(channel);
        }
        let text = texts.entry(channel).or_default();
        text.push((' ', n));
        text.extend(notes.chars().map(|c| (c, n)));
    }
    let mut channels = Vec::new();
    let letters = order.clone();
    for channel in order {
        let (text, lines) = expand_lines(&texts[&channel], &macros).with_context(|| format!("channel {channel}"))?;
        let mut state = State { octave: 4, length: 4.0, volume: 0.8, gate: 0.875, pan: 0.0, tempo, instrument: 0, time: 0.0 };
        let mut notes = Vec::new();
        let mut parser = Parser { chars: text, lines, at: 0, steps: 0, instruments: &names };
        parser.sequence(&mut state, &mut notes, None).with_context(|| {
            let around: String = parser.chars[parser.at.saturating_sub(12)..(parser.at + 4).min(parser.chars.len())].iter().collect();
            let line = parser.lines.get(parser.at.saturating_sub(1)).or(parser.lines.last()).copied().unwrap_or(0);
            format!("line {line}: channel {channel}, near '{}'", around.trim())
        })?;
        // The channel's end is where its last note or rest ends.
        let end = (state.time * SAMPLE_RATE as f64) as u64;
        notes.push(Note { start: end, held: 0, freq: None, instrument: 0, volume: 0.0, pan: 0.0 });
        channels.push(notes);
    }
    let length = channels.iter().filter_map(|c| c.last().map(|n| n.start)).max().unwrap_or(0).max(1);
    if let Some((c, _)) = layered.iter().find(|(c, _)| !letters.contains(c)) {
        bail!("#layer names channel {c}, which plays nothing");
    }
    let layers = letters.iter().map(|c| layered.iter().rev().find(|(l, _)| l == c).map_or(0.0, |(_, f)| *f)).collect();
    Ok(Song { title, tempo, meter, layers, names: letters, instruments, channels, length, looping })
}

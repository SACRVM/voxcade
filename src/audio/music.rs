//! Music written as MML (Music Macro Language): plain text, one line of
//! notes per channel, played by SID-like voices (pulse with width
//! modulation, saw, triangle, sine, noise; ADSR, vibrato, sweeps, chords as
//! fast arpeggios, a low-pass filter). Text, so a person or an agent can
//! write it, and the game plays it again on every save.
//!
//! ```text
//! ; a comment
//! #title  Desert March
//! #tempo  140                      ; beats per minute
//! @lead   square duty=0.25 attack=0.005 decay=0.15 sustain=0.6 release=0.1 vibrato=0.012
//! @chord  square duty=0.5 arp=0,3,7 volume=0.5
//! @kick   triangle freq=110 sweep=-24 decay=0.12 sustain=0
//! $beat = @kick c8 @hat c8 @snare c8 @hat c8
//! A: @lead v12 o4 l8 d4. a a4 g f | [c d e f]2 >c2
//! D: [$beat]8
//! ```
//!
//! Channel commands: notes `c d e f g a b` (`+` or `#` sharp, `-` flat), a
//! length (1 whole, 4 quarter, 8 eighth ...) and dots; `r` rest; `&` ties
//! to the next note; `o4` octave, `<` `>` octave down and up; `l8` default
//! length; `v0`..`v15` volume; `q1`..`q8` how much of a note sounds (8
//! legato); `p-100`..`p100` pan; `t140` tempo; `@name` instrument;
//! `[ ... ]3` repeat; `$name` a macro; `|` bar lines, ignored. `#meter 3`
//! (beats in a bar, 4 unless set) is what a game that reads the song's
//! bars counts by (`music.bar`); its beats are `#tempo`'s, a `t` in a
//! channel changing nothing for them. `#layer D C 0.35` puts channels in a
//! layer: they sound while the game's tension (`music.intensity`, 1
//! unless a game sets it) is at least 0.35, fading in and out over a bar;
//! a channel in no layer always sounds.
//!
//! Instrument options: a wave (`square saw triangle sine noise`), `duty`,
//! `pwm` and `pwm_speed` (pulse width wobble), `attack` `decay` `sustain`
//! (a level 0..1) `release`, `vibrato` `vibrato_speed` `vibrato_delay`,
//! `arp=0,4,7` and `arp_speed` (Hz), `sweep` (semitones over
//! `sweep_time`), `freq` (a fixed pitch, for drums), `lowpass` (a gentle
//! fixed one), `volume`.
//!
//! The SID's filter: `filter=lowpass|bandpass|highpass` (12 dB per octave),
//! `cutoff` (Hz), `resonance` (0..1; it overdrives softly instead of
//! spiking), and `cutoff_end` with `cutoff_time` to sweep the cutoff on
//! every note, the way C64 players run their filter tables (acid: open
//! wide, close fast, lots of resonance). Any of these turns the filter on,
//! as a low-pass unless `filter` says otherwise.
//!
//! Sidechain: an instrument with `sidechain=0.6` ducks the other channels
//! by that much whenever one of its notes starts (a kick pumping the bass
//! and pads), and they come back over `sidechain_release` seconds. `duck`
//! (0..1, default 1) is how far an instrument follows; 0 leaves it alone.
//!
//! A game's own say over its song as heard (`music:volume`,
//! `music:muffle`): a part of its loudness and a low-pass over the whole
//! song, as through a wall (`Mix`), each gliding to what the game asked.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use super::synth::{SAMPLE_RATE, Wave};
use super::words;

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

#[derive(Clone, Debug)]
pub struct Instrument {
    pub wave: Wave,
    pub duty: f32,
    pub pwm: f32,
    pub pwm_speed: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub vibrato: f32,
    pub vibrato_speed: f32,
    pub vibrato_delay: f32,
    pub arp: Vec<f32>,
    pub arp_speed: f32,
    pub sweep: f32,
    pub sweep_time: Option<f32>,
    pub freq: Option<f32>,
    pub lowpass: Option<f32>,
    /// The SID-style filter, off when None.
    pub filter: Option<FilterMode>,
    pub cutoff: f32,
    pub cutoff_end: Option<f32>,
    pub cutoff_time: Option<f32>,
    pub resonance: f32,
    /// How far this instrument's notes duck the other channels (0: not at all).
    pub sidechain: f32,
    pub sidechain_release: f32,
    /// How far this instrument follows the ducking.
    pub duck: f32,
    pub volume: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Low,
    Band,
    High,
}

impl Default for Instrument {
    fn default() -> Self {
        Self {
            wave: Wave::Square,
            duty: 0.5,
            pwm: 0.0,
            pwm_speed: 1.0,
            attack: 0.004,
            decay: 0.1,
            sustain: 0.7,
            release: 0.08,
            vibrato: 0.0,
            vibrato_speed: 6.0,
            vibrato_delay: 0.0,
            arp: Vec::new(),
            arp_speed: 50.0,
            sweep: 0.0,
            sweep_time: None,
            freq: None,
            lowpass: None,
            filter: None,
            cutoff: 1000.0,
            cutoff_end: None,
            cutoff_time: None,
            resonance: 0.0,
            sidechain: 0.0,
            sidechain_release: 0.2,
            duck: 1.0,
            volume: 0.5,
        }
    }
}

/// One note on a channel, in samples from the start of the song.
#[derive(Clone, Debug)]
struct Note {
    start: u64,
    /// How long the key is held; the release follows.
    held: u64,
    /// None: a rest.
    freq: Option<f32>,
    instrument: usize,
    volume: f32,
    pan: f32,
}

#[derive(Debug)]
pub struct Song {
    pub title: String,
    /// Beats a minute (`#tempo`) and beats in a bar (`#meter`).
    pub tempo: f32,
    pub meter: u32,
    /// The tension each channel sounds from (`#layer`; 0: always).
    layers: Vec<f32>,
    /// Channel letters in the order they first appear.
    pub names: Vec<char>,
    instruments: Vec<Instrument>,
    channels: Vec<Vec<Note>>,
    /// Samples until it starts over.
    pub length: u64,
    pub looping: bool,
}

/// Frequency of a note: octave 4's A is 440 Hz.
fn note_freq(octave: i32, semitone: i32) -> f32 {
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
            other => {
                let known = words::WORDS.iter().filter(|w| w.mml.is_some()).map(|w| w.name).chain(["pulse"]);
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
                'a'..='g' if c.is_ascii_lowercase() => {
                    if notes.len() >= MOST_NOTES {
                        bail!("more than {MOST_NOTES} notes in one channel");
                    }
                    let note_octave = self.octave(state.octave)?;
                    let mut semitone = match c {
                        'c' => 0,
                        'd' => 2,
                        'e' => 4,
                        'f' => 5,
                        'g' => 7,
                        'a' => 9,
                        _ => 11,
                    };
                    match self.peek() {
                        Some('+' | '#') => {
                            semitone += 1;
                            self.at += 1;
                        }
                        Some('-') => {
                            semitone -= 1;
                            self.at += 1;
                        }
                        _ => {}
                    }
                    let mut beats = self.duration(state)?;
                    // `&` ties the following note's length on.
                    while self.peek() == Some('&') {
                        self.at += 1;
                        // A tie may run over a bar line.
                        while self.peek().is_some_and(|c| c.is_whitespace() || c == '|') {
                            self.at += 1;
                        }
                        if self.peek().is_some_and(|c| "abcdefg".contains(c)) {
                            self.at += 1;
                            if matches!(self.peek(), Some('+' | '#' | '-')) {
                                self.at += 1;
                            }
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
                other => bail!("unknown command '{other}'"),
            }
        }
        if until.is_some() {
            bail!("[ without ]");
        }
        Ok(())
    }
}

pub fn parse(text: &str) -> Result<Song> {
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
            && channel.trim().len() == 1
        {
            lines.push((channel.trim().chars().next().unwrap(), notes.to_string(), n + 1));
        } else {
            bail!("line {}: expected #setting, @instrument, $macro or a channel like 'A: c d e'", n + 1);
        }
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

/// When a song's beats fall, for a game that reads them (`music.beat`):
/// its name (the file's, without the folder and `.mml`), beats a minute,
/// beats in a bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    pub name: String,
    pub tempo: f32,
    pub meter: u32,
}

impl Song {
    /// A song of nothing, 120 beats a minute and 4 to a bar (in place of
    /// one that does not read).
    pub fn empty() -> Song {
        parse("").expect("an empty song reads")
    }

    /// Its beats, for a game: by its file's name.
    pub fn timing(&self, file: &str) -> Timing {
        let name = std::path::Path::new(file).file_stem().map_or_else(|| file.to_string(), |s| s.to_string_lossy().into_owned());
        Timing { name, tempo: self.tempo, meter: self.meter }
    }

    /// How long each channel is, in seconds (they should match, or the
    /// short ones fall silent before the song starts over).
    pub fn channel_lengths(&self) -> Vec<(char, f32)> {
        self.names
            .iter()
            .zip(&self.channels)
            .map(|(&name, notes)| (name, notes.last().map_or(0, |n| n.start) as f32 / SAMPLE_RATE as f32))
            .collect()
    }
}

/// One channel's voice: the note it plays and where the oscillator is.
#[derive(Default)]
struct Voice {
    next: usize,
    note: Option<usize>,
    started: u64,
    phase: f32,
    noise: f32,
    seed: u32,
    low: f32,
    filter: Svf,
    /// Envelope level while the key is held, where the release starts from.
    released_at: f32,
}

/// A state-variable filter (the SID's kind: low, band and high pass from
/// one 12 dB/octave stage), in the zero-delay form that stays stable
/// however fast the cutoff moves.
#[derive(Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    fn process(&mut self, x: f32, cutoff: f32, resonance: f32, mode: FilterMode) -> f32 {
        let sr = SAMPLE_RATE as f32;
        let g = (std::f32::consts::PI * cutoff.clamp(20.0, sr * 0.45) / sr).tan();
        // Damping: Butterworth (no peak) at 0, a ringing peak near 1.
        let k = (std::f32::consts::SQRT_2 * (1.0 - resonance)).max(0.1);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let y = match mode {
            FilterMode::Low => v2,
            FilterMode::Band => v1,
            FilterMode::High => x - k * v1 - v2,
        };
        // Overdrive softly past full scale, so resonance growls instead of spiking.
        if y.abs() <= 1.0 { y } else { y.signum() * (1.0 + (y.abs() - 1.0).tanh()) }
    }
}

/// The ducking the last sidechain note started: since when, how deep, how long.
#[derive(Clone, Copy)]
struct Duck {
    from: u64,
    depth: f32,
    release: f32,
}

impl Duck {
    /// How far the other channels are down `now`, 0..depth: down within a
    /// few milliseconds, back up along a smooth curve.
    fn amount(&self, now: u64) -> f32 {
        const ATTACK: f32 = 0.004;
        let t = now.saturating_sub(self.from) as f32 / SAMPLE_RATE as f32;
        if t < ATTACK {
            return self.depth * t / ATTACK;
        }
        let r = ((t - ATTACK) / self.release.max(1e-3)).min(1.0);
        self.depth * 0.5 * (1.0 + (std::f32::consts::PI * r).cos())
    }
}

/// A jump this far off (samples, 50 ms) when keeping time with a game;
/// less is caught up by going a little faster or slower.
const JUMP: i64 = SAMPLE_RATE as i64 / 20;

/// Samples a jump fades in over (5 ms), so it does not click.
const FADE_IN: u32 = SAMPLE_RATE / 200;

/// A value of the game's song as heard that the game moves over a time
/// (`music:volume`, `music:muffle`): from where it was to where it goes,
/// over so many of the game's ticks, so many of them gone; how many times
/// it was asked (a new ask starts the audio's own glide). The game keeps
/// it (in its snapshot, never in its hash: only heard).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    pub from: f32,
    pub to: f32,
    pub ticks: u32,
    pub done: u32,
    pub asked: u32,
}

impl Glide {
    /// Standing at `at`, never asked.
    pub const fn still(at: f32) -> Self {
        Self { from: at, to: at, ticks: 0, done: 0, asked: 0 }
    }

    /// Where it is now.
    pub fn now(&self) -> f32 {
        if self.done >= self.ticks { self.to } else { self.from + (self.to - self.from) * self.done as f32 / self.ticks as f32 }
    }

    /// The seconds of its glide still to go.
    pub fn left(&self) -> f32 {
        self.ticks.saturating_sub(self.done) as f32 * crate::sim::DT
    }

    /// A tick of the game on.
    pub fn step(&mut self) {
        self.done = (self.done + 1).min(self.ticks);
    }

    /// Asked to go to `to` over `seconds`, from where it is now.
    pub fn ask(&mut self, to: f32, seconds: f32) {
        let ticks = (seconds / crate::sim::DT).round().min(u32::MAX as f32) as u32;
        *self = Self { from: self.now(), to, ticks, done: 0, asked: self.asked.wrapping_add(1) };
    }
}

/// How the game's song sounds as the game asked (`music:volume`, a part of
/// its loudness, 1 as written; `music:muffle`, 0 clear to 1 very dull).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub volume: Glide,
    pub muffle: Glide,
}

impl Default for Mix {
    fn default() -> Self {
        Self { volume: Glide::still(1.0), muffle: Glide::still(0.0) }
    }
}

/// The shortest glides the audio makes, the loudness's (5 ms) and the
/// muffle's (20 ms: the filter's sound blends in over its first quarter):
/// a change asked "at once" does not click.
const SHORTEST: (f32, f32) = (0.005, 0.02);

/// The low-pass's cutoff for a muffle (0..1), on a musical curve: 300 Hz
/// times 60 to the power (1 - amount)^1.5: 18 kHz (open) at 0, about
/// 4.3 kHz at 0.25, 1.3 kHz at 0.5, 500 Hz at 0.75, 300 Hz at 1.
pub fn muffle_cutoff(amount: f32) -> f32 {
    300.0 * 60f32.powf((1.0 - amount.clamp(0.0, 1.0)).powf(1.5))
}

/// A value going straight to where it is asked, a step a sample.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    now: f32,
    to: f32,
    step: f32,
}

impl Ramp {
    fn at(v: f32) -> Self {
        Self { now: v, to: v, step: 0.0 }
    }

    /// Towards `to` over `seconds` (`shortest` at least).
    fn toward(&mut self, to: f32, seconds: f32, shortest: f32) {
        let samples = (seconds.max(shortest) * SAMPLE_RATE as f32).max(1.0);
        (self.to, self.step) = (to, (to - self.now) / samples);
    }

    /// The value for this sample, and a step on.
    fn next(&mut self) -> f32 {
        let now = self.now;
        if self.now != self.to {
            let n = self.now + self.step;
            self.now = if self.step == 0.0 || (self.step > 0.0) == (n >= self.to) { self.to } else { n };
        }
        now
    }
}

/// The game's say over its song (`Mix`) as the player applies it: the
/// loudness and the muffle each gliding sample by sample, and a low-pass
/// of 12 dB an octave (the zero-delay state-variable filter, Butterworth:
/// stable and without zipper noise however it glides) on each side.
#[derive(Clone, Debug)]
struct Bus {
    level: Ramp,
    muffle: Ramp,
    /// The asks last followed (`Glide::asked`), the volume's and the muffle's.
    asked: (u32, u32),
    /// Each side's filter state, and its coefficient for the muffle it was
    /// last worked out for.
    low: [(f32, f32); 2],
    g: (f32, f32),
}

impl Default for Bus {
    fn default() -> Self {
        Self { level: Ramp::at(1.0), muffle: Ramp::at(0.0), asked: (0, 0), low: [(0.0, 0.0); 2], g: (-1.0, 0.0) }
    }
}

impl Bus {
    /// One frame of the song through it.
    fn apply(&mut self, l: f32, r: f32) -> (f32, f32) {
        let level = self.level.next();
        let amount = self.muffle.next();
        let (l, r) = (l * level, r * level);
        if self.g.0 != amount {
            let cutoff = muffle_cutoff(amount).min(SAMPLE_RATE as f32 * 0.45);
            self.g = (amount, (std::f32::consts::PI * cutoff / SAMPLE_RATE as f32).tan());
        }
        // Always run, so its state is the song's when a muffle begins.
        let g = self.g.1;
        let k = std::f32::consts::SQRT_2;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let (a2, a3) = (g * a1, g * g * a1);
        let low = |x: f32, s: &mut (f32, f32)| {
            let v3 = x - s.1;
            let v1 = a1 * s.0 + a2 * v3;
            let v2 = s.1 + a2 * s.0 + a3 * v3;
            *s = (2.0 * v1 - s.0, 2.0 * v2 - s.1);
            v2
        };
        let [sl, sr] = &mut self.low;
        let (wl, wr) = (low(l, sl), low(r, sr));
        if amount <= 0.0 {
            return (l, r);
        }
        // Blended in over the first quarter, so a muffle begins without a step.
        let wet = (amount * 4.0).min(1.0);
        (l + (wl - l) * wet, r + (wr - r) * wet)
    }
}

/// Plays a song, sample by sample; runs in the audio thread.
pub struct Player {
    song: Song,
    voices: Vec<Voice>,
    time: u64,
    duck: Option<Duck>,
    pub volume: f32,
    /// How fast its time goes against the samples (1: as written), nudged
    /// to keep with a game that reads its beats (`follow`); what is left
    /// of a sample.
    rate: f64,
    carry: f64,
    /// Samples since a jump, fading in up to `FADE_IN`.
    since_jump: u32,
    /// The game's tension (`music.intensity`), and how loud each channel's
    /// layer is now (0..1, moving a bar's worth a bar).
    pub intensity: f32,
    loud: Vec<f32>,
    /// The game's say over how it sounds (`Mix`): neutral unless followed.
    bus: Bus,
}

impl Player {
    pub fn title(&self) -> &str {
        &self.song.title
    }

    pub fn new(song: Song, volume: f32) -> Self {
        let voices = (0..song.channels.len()).map(|i| Voice { seed: 0x1234_5678 ^ (i as u32 * 7919), ..Voice::default() }).collect();
        let loud = song.layers.iter().map(|_| 1.0).collect();
        Self { song, voices, time: 0, duck: None, volume, rate: 1.0, carry: 0.0, since_jump: FADE_IN, intensity: 1.0, loud, bus: Bus::default() }
    }

    /// Sounds as the game's `mix` is now, gliding on where it still glides
    /// (a song begun, or the game brought back).
    pub fn start_mix(&mut self, mix: &Mix) {
        self.bus.level = Ramp::at(mix.volume.now());
        self.bus.muffle = Ramp::at(mix.muffle.now());
        self.bus.asked = (mix.volume.asked, mix.muffle.asked);
        self.bus.level.toward(mix.volume.to, mix.volume.left(), SHORTEST.0);
        self.bus.muffle.toward(mix.muffle.to, mix.muffle.left(), SHORTEST.1);
    }

    /// Follows the game's `mix`: a new ask glides from where the song
    /// sounds now to where it was asked, over what is left of its time.
    pub fn follow_mix(&mut self, mix: &Mix) {
        if mix.volume.asked != self.bus.asked.0 {
            self.bus.level.toward(mix.volume.to, mix.volume.left(), SHORTEST.0);
        }
        if mix.muffle.asked != self.bus.asked.1 {
            self.bus.muffle.toward(mix.muffle.to, mix.muffle.left(), SHORTEST.1);
        }
        self.bus.asked = (mix.volume.asked, mix.muffle.asked);
    }

    /// Sets how loud each layer is at once, for the tension it starts at
    /// (a song begun while the game is calm starts calm).
    pub fn start_at(&mut self, intensity: f32) {
        self.intensity = intensity;
        for (loud, &from) in self.loud.iter_mut().zip(&self.song.layers) {
            *loud = if intensity >= from { 1.0 } else { 0.0 };
        }
    }

    /// Samples into the song a beat count is (from its start, round its
    /// loop as it plays).
    pub fn at_beat(&self, beats: f64) -> u64 {
        (beats * 60.0 / self.song.tempo as f64 * SAMPLE_RATE as f64) as u64
    }

    /// Jumps to `at` samples into the song (round its loop), each voice
    /// taking up the note that sounds there, faded in so it does not click.
    pub fn seek(&mut self, at: u64) {
        let at = if self.song.looping { at % self.song.length } else { at.min(self.song.length) };
        (self.time, self.carry, self.duck, self.since_jump) = (at, 0.0, None, 0);
        for (ch, v) in self.voices.iter_mut().enumerate() {
            let notes = &self.song.channels[ch];
            v.next = notes.partition_point(|n| n.start <= at);
            v.note = v.next.checked_sub(1).filter(|&i| notes[i].freq.is_some());
            if let Some(i) = v.note {
                (v.started, v.phase) = (notes[i].start, 0.0);
            }
        }
    }

    /// Keeps time with a game's clock, `target` samples into the song: a
    /// jump when it is far off (after a pause), else a little faster or
    /// slower until it is with it again, unheard.
    pub fn follow(&mut self, target: u64) {
        let length = self.song.length.max(1) as i64;
        let target = if self.song.looping { target % self.song.length } else { target.min(self.song.length) };
        let mut off = target as i64 - self.time as i64;
        if self.song.looping {
            // The shorter way round the loop.
            if off > length / 2 {
                off -= length;
            } else if off < -length / 2 {
                off += length;
            }
        }
        if off.abs() > JUMP {
            self.seek(target);
            self.rate = 1.0;
        } else {
            self.rate = 1.0 + (off as f64 / (4.0 * SAMPLE_RATE as f64)).clamp(-0.003, 0.003);
        }
    }

    /// Whether a song that plays once has ended.
    pub fn over(&self) -> bool {
        !self.song.looping && self.time >= self.song.length
    }

    /// Adds the next frames to interleaved stereo `out`.
    pub fn mix(&mut self, out: &mut [f32]) {
        let sr = SAMPLE_RATE as f32;
        for frame in out.as_chunks_mut::<2>().0 {
            if self.time >= self.song.length {
                if !self.song.looping {
                    return;
                }
                self.time = 0;
                self.duck = None;
                for v in &mut self.voices {
                    v.next = 0;
                    v.note = None;
                }
            }
            let ducked = self.duck.map_or(0.0, |d| d.amount(self.time));
            // The layers move towards the tension, a bar's worth a bar.
            let step = self.song.tempo / (60.0 * SAMPLE_RATE as f32 * self.song.meter as f32);
            for (loud, &from) in self.loud.iter_mut().zip(&self.song.layers) {
                let want = if self.intensity >= from { 1.0 } else { 0.0 };
                *loud = if *loud < want { (*loud + step).min(want) } else { (*loud - step).max(want) };
            }
            let mut sum = [0.0f32; 2];
            for (ch, v) in self.voices.iter_mut().enumerate() {
                let notes = &self.song.channels[ch];
                while v.next < notes.len() && notes[v.next].start <= self.time {
                    // Rests and the end marker have no pitch: the note before rings out.
                    if notes[v.next].freq.is_some() {
                        v.note = Some(v.next);
                        v.started = notes[v.next].start;
                        v.phase = 0.0;
                        let ins = &self.song.instruments[notes[v.next].instrument];
                        if ins.sidechain > 0.0 {
                            self.duck = Some(Duck { from: self.time, depth: ins.sidechain, release: ins.sidechain_release });
                        }
                    }
                    v.next += 1;
                }
                let Some(index) = v.note else { continue };
                let n = &notes[index];
                let ins = &self.song.instruments[n.instrument];
                let t = (self.time - v.started) as f32 / sr;
                let held = n.held as f32 / sr;

                // ADSR.
                let level = if t < held {
                    let l = if t < ins.attack {
                        t / ins.attack.max(1e-4)
                    } else {
                        let d = (t - ins.attack) / ins.decay.max(1e-4);
                        if d < 1.0 { 1.0 + (ins.sustain - 1.0) * d } else { ins.sustain }
                    };
                    v.released_at = l;
                    l
                } else {
                    let r = (t - held) / ins.release.max(1e-4);
                    if r >= 1.0 {
                        v.note = None;
                        continue;
                    }
                    v.released_at * (1.0 - r)
                };

                // Pitch: the note (or a fixed one), a sweep, the arpeggio, vibrato.
                let mut freq = ins.freq.unwrap_or(n.freq.unwrap_or(0.0));
                if ins.sweep != 0.0 {
                    let span = ins.sweep_time.unwrap_or(ins.attack + ins.decay).max(1e-3);
                    freq *= 2f32.powf(ins.sweep * (t / span).min(1.0) / 12.0);
                }
                if !ins.arp.is_empty() {
                    let step = (t * ins.arp_speed) as usize % ins.arp.len();
                    freq *= 2f32.powf(ins.arp[step] / 12.0);
                }
                if ins.vibrato > 0.0 && t > ins.vibrato_delay {
                    freq *= 1.0 + ins.vibrato * (std::f32::consts::TAU * ins.vibrato_speed * (t - ins.vibrato_delay)).sin();
                }
                v.phase += freq / sr;
                let wrapped = v.phase >= 1.0;
                v.phase = v.phase.fract();
                let duty = (ins.duty + ins.pwm * (std::f32::consts::TAU * ins.pwm_speed * t).sin()).clamp(0.05, 0.95);
                let mut s = match ins.wave {
                    // A pulse off duty 0.5 is high longer than low: taken off
                    // its middle so it swings round zero (as in `synth`).
                    Wave::Square => (if v.phase < duty { 1.0 } else { -1.0 }) - (2.0 * duty - 1.0),
                    Wave::Saw => 2.0 * v.phase - 1.0,
                    Wave::Triangle => 1.0 - 4.0 * (v.phase - 0.5).abs(),
                    Wave::Sine => (std::f32::consts::TAU * v.phase).sin(),
                    Wave::Noise => {
                        if wrapped {
                            v.seed ^= v.seed << 13;
                            v.seed ^= v.seed >> 17;
                            v.seed ^= v.seed << 5;
                            v.noise = v.seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
                        }
                        v.noise
                    }
                };
                if let Some(cut) = ins.lowpass {
                    let alpha = 1.0 - (-std::f32::consts::TAU * cut / sr).exp();
                    v.low += alpha * (s - v.low);
                    s = v.low;
                }
                if let Some(mode) = ins.filter {
                    // The cutoff swept per note, evenly in octaves.
                    let mut cutoff = ins.cutoff;
                    if let Some(end) = ins.cutoff_end {
                        let span = ins.cutoff_time.unwrap_or(ins.attack + ins.decay).max(1e-3);
                        cutoff *= (end / ins.cutoff).powf((t / span).min(1.0));
                    }
                    s = v.filter.process(s, cutoff, ins.resonance, mode);
                }
                // Ducked under a sidechain note, unless this is one.
                let duck = if ins.sidechain > 0.0 { 1.0 } else { 1.0 - ducked * ins.duck };
                let fade = if self.since_jump < FADE_IN { self.since_jump as f32 / FADE_IN as f32 } else { 1.0 };
                let gain = s * level * ins.volume * n.volume * self.volume * duck * fade * self.loud[ch];
                let angle = (n.pan + 1.0) * std::f32::consts::FRAC_PI_4;
                sum[0] += gain * angle.cos() * std::f32::consts::SQRT_2;
                sum[1] += gain * angle.sin() * std::f32::consts::SQRT_2;
            }
            // As loud and as clear as the game asked (`Mix`).
            let (l, r) = self.bus.apply(sum[0], sum[1]);
            frame[0] += l;
            frame[1] += r;
            self.since_jump = self.since_jump.saturating_add(1);
            // A sample on, or a hair more or less when keeping time.
            self.carry += self.rate;
            let on = self.carry as u64;
            self.carry -= on as f64;
            self.time += on;
        }
    }

    /// The whole song (once) as interleaved stereo samples.
    pub fn render(mut self, seconds: Option<f32>) -> Vec<f32> {
        let frames = match seconds {
            Some(s) => (s * SAMPLE_RATE as f32) as u64,
            None => self.song.length,
        };
        let mut out = vec![0.0; frames as usize * 2];
        self.mix(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A channel in a layer sounds while the tension is at its threshold or
    /// more, and fades in or out over a bar when it crosses it.
    #[test]
    fn layers_follow_the_tension() {
        let text = "#tempo 120\n#layer B 0.5\n@a square duty=0.5 attack=0 decay=0 sustain=1 release=0.01\nA: c1 c1 c1 c1\nB: g1 g1 g1 g1\n";
        let loudness = |intensity: f32| {
            let mut p = Player::new(parse(text).unwrap(), 1.0);
            p.start_at(intensity);
            let mut out = vec![0.0; SAMPLE_RATE as usize * 2];
            p.mix(&mut out);
            out.iter().map(|s| s * s).sum::<f32>()
        };
        let (calm, tense) = (loudness(0.2), loudness(0.8));
        assert!(tense > calm * 1.5, "the layer adds to it: {calm} {tense}");
        // Crossing the threshold, it comes in over a bar (2 seconds).
        let mut p = Player::new(parse(text).unwrap(), 1.0);
        p.start_at(0.0);
        p.intensity = 1.0;
        let mut out = vec![0.0; SAMPLE_RATE as usize * 2];
        p.mix(&mut out);
        assert!((p.loud[1] - 0.5).abs() < 0.01, "half way after a second: {}", p.loud[1]);
        assert_eq!(p.loud[0], 1.0, "a channel in no layer always sounds");
        assert!(parse("#layer Z 0.5\nA: c\n").unwrap_err().to_string().contains("channel Z"));
        assert!(parse("#layer A 2\nA: c\n").is_err() && parse("#layer 0.5\nA: c\n").is_err());
    }

    /// A jump into a held note takes it up there (no silence until the
    /// next), faded in; keeping time catches a small lag by going a hair
    /// faster and jumps over a big one.
    #[test]
    fn a_song_jumps_and_keeps_time() {
        let song = parse("#tempo 120\n@a square duty=0.5 attack=0 decay=0 sustain=1 release=0.01\nA: c1 d1\n").unwrap();
        assert_eq!((song.tempo, song.meter), (120.0, 4));
        let mut p = Player::new(song, 1.0);
        let second = SAMPLE_RATE as u64;
        assert_eq!(p.at_beat(2.0), second, "two beats at 120 a minute");
        p.seek(second);
        let mut out = vec![0.0; 960 * 2];
        p.mix(&mut out);
        assert!(out[200..].iter().any(|s| s.abs() > 0.1), "the held note sounds after the jump");
        assert!(out[0].abs() < out[400].abs() + 1e-6, "faded in");
        // A little behind: a hair faster, until it is with the target.
        let target = p.time + 600;
        p.follow(target);
        assert!(p.rate > 1.0 && p.rate <= 1.003);
        // Far off: a jump.
        p.follow(3 * second);
        assert_eq!((p.time, p.rate), (3 * second, 1.0));
        assert!(parse("#meter 3/4\nA: c\n").unwrap().meter == 3 && parse("#meter x\nA: c\n").is_err());
    }

    const SR: f32 = SAMPLE_RATE as f32;

    #[test]
    fn notes_lengths_and_octaves() {
        let song = parse("#tempo 120\nA: o4 a4 > c8. < a+ r2 e-16").unwrap();
        let notes = &song.channels[0];
        assert!((notes[0].freq.unwrap() - 440.0).abs() < 0.01);
        assert!((notes[1].freq.unwrap() - note_freq(5, 0)).abs() < 0.01);
        assert!((notes[2].freq.unwrap() - note_freq(4, 10)).abs() < 0.01);
        // A quarter at 120 is half a second; a dotted eighth three eighths of one.
        assert_eq!(notes[1].start, (0.5 * SR) as u64);
        assert_eq!(notes[2].start, ((0.5 + 0.375) * SR) as u64);
        // After the default-length a+ (a quarter) and a half rest.
        assert_eq!(notes[3].start, ((0.875 + 0.5 + 1.0) * SR) as u64);
        assert!((notes[3].freq.unwrap() - note_freq(4, 3)).abs() < 0.01);
    }

    #[test]
    fn repeats_ties_and_macros() {
        let song = parse("#tempo 60\n$two = c d\nA: [$two]3 e4& |\nA: e4 f").unwrap();
        let notes = &song.channels[0];
        let pitches: Vec<i32> = notes.iter().filter_map(|n| n.freq).map(|f| (12.0 * (f / 261.6256).log2()).round() as i32).collect();
        assert_eq!(pitches, vec![0, 2, 0, 2, 0, 2, 4, 5]);
        // The tied e lasts two beats: f starts at 6 + 2 seconds.
        assert_eq!(notes[7].start, (8.0 * SR) as u64);
    }

    /// A song that plays once is over at its end; one that loops never.
    #[test]
    fn a_song_that_plays_once_ends() {
        for (text, ends) in [("#tempo 240\n#loop no\nA: c4 d4", true), ("#tempo 240\nA: c4 d4", false)] {
            let mut player = Player::new(parse(text).unwrap(), 1.0);
            let mut out = vec![0.0; 2 * SAMPLE_RATE as usize];
            assert!(!player.over(), "not before it played");
            player.mix(&mut out);
            assert_eq!(player.over(), ends, "{text}");
        }
    }

    #[test]
    fn mistakes_say_where() {
        let err = format!("{:#}", parse("A: c d x").unwrap_err());
        assert!(err.contains("unknown command 'x'"), "{err}");
        let err = format!("{:#}", parse("A: @nope c").unwrap_err());
        assert!(err.contains("no instrument @nope"), "{err}");
        assert!(parse("A: [c d").is_err());
    }

    /// Every song parses, the games' and the library's (`music/`), and its
    /// channels are equally long (or the short ones fall silent before it
    /// starts over).
    #[test]
    fn the_songs_line_up() {
        fn songs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    songs(&path, out);
                } else if path.extension().is_some_and(|e| e == "mml") {
                    out.push(path);
                }
            }
        }
        let mut all = Vec::new();
        songs("games".as_ref(), &mut all);
        songs("music".as_ref(), &mut all);
        assert!(all.len() >= 7, "only {} songs found", all.len());
        for path in all {
            let song = parse(&std::fs::read_to_string(&path).unwrap()).unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
            let total = song.length as f32 / SR;
            for (channel, length) in song.channel_lengths() {
                assert!((length - total).abs() < 0.01, "{}: channel {channel} is {length:.2}s of {total:.2}s", path.display());
            }
        }
    }

    /// Mono samples of a song (the left channel).
    fn play(text: &str, seconds: f32) -> Vec<f32> {
        let out = Player::new(parse(text).unwrap(), 1.0).render(Some(seconds));
        out.iter().step_by(2).copied().collect()
    }

    /// Root mean square of `samples` between two times.
    fn rms(samples: &[f32], from: f32, to: f32) -> f32 {
        let part = &samples[(from * SR) as usize..(to * SR) as usize];
        (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt()
    }

    /// How much of the sound is high, against its loudness: the bends from
    /// sample to sample (second differences weigh highs by their frequency
    /// squared) over the level.
    fn edge(samples: &[f32], from: f32, to: f32) -> f32 {
        let part = &samples[(from * SR) as usize..(to * SR) as usize];
        let bends = part.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).sum::<f32>() / part.len() as f32;
        bends / rms(samples, from, to)
    }

    /// The SID filter: a low-pass darkens a bright saw, resonance lifts a
    /// tone at the cutoff, a high-pass takes out a low one, and a swept
    /// cutoff closes on every note.
    #[test]
    fn the_filter_cuts_resonates_and_sweeps() {
        let plain = play("@x saw sustain=1\nA: @x o6 c1", 0.5);
        let dark = play("@x saw sustain=1 cutoff=300\nA: @x o6 c1", 0.5);
        assert!(rms(&dark, 0.1, 0.4) < rms(&plain, 0.1, 0.4) * 0.3, "the low-pass does not cut");

        // A sine right at the cutoff: about -3 dB plain, far over it ringing.
        let flat = play("@x sine sustain=1 cutoff=880\nA: @x o5 a1", 0.5);
        let ringing = play("@x sine sustain=1 cutoff=880 resonance=0.85\nA: @x o5 a1", 0.5);
        assert!(rms(&ringing, 0.1, 0.4) > rms(&flat, 0.1, 0.4) * 2.5, "resonance does not lift the cutoff");
        assert!(ringing.iter().all(|s| s.abs() <= 2.0), "resonance spikes instead of overdriving");

        let low = play("@x sine sustain=1\nA: @x o2 c1", 0.5);
        let cut = play("@x sine sustain=1 filter=highpass cutoff=2000\nA: @x o2 c1", 0.5);
        assert!(rms(&cut, 0.1, 0.4) < rms(&low, 0.1, 0.4) * 0.05, "the high-pass lets the bass through");

        let swept = play("@x saw sustain=1 cutoff=6000 cutoff_end=200 cutoff_time=0.1\nA: @x o4 c1", 0.5);
        assert!(edge(&swept, 0.0, 0.03) > edge(&swept, 0.2, 0.4) * 3.0, "the cutoff does not close");
    }

    /// A sidechain note ducks the other channels at once and lets them come
    /// back over its release; a channel with duck=0 is left alone.
    #[test]
    fn the_kick_ducks_the_rest() {
        // A silent kick on the second beat (0.5 s at 120) under a held tone.
        let song = "@pad sine sustain=1 {duck}\n@kick triangle freq=60 decay=0.05 sustain=0 sidechain=0.8 sidechain_release=0.3 volume=0\n#tempo 120\nA: @pad o4 a1\nD: @kick r4 c4 r2";
        let pumped = play(&song.replace("{duck}", ""), 1.0);
        let before = rms(&pumped, 0.3, 0.45);
        assert!(rms(&pumped, 0.51, 0.53) < before * 0.35, "the kick does not duck the pad");
        assert!(rms(&pumped, 0.85, 0.95) > before * 0.95, "the pad does not come back");
        let steady = play(&song.replace("{duck}", "duck=0"), 1.0);
        assert!(rms(&steady, 0.51, 0.53) > before * 0.95, "duck=0 still ducks");
    }

    /// Every voice of More Blood's theme sounds, and the song loops.
    #[test]
    fn the_theme_plays() {
        let text = std::fs::read_to_string("games/moreblood/music/theme.mml").unwrap();
        let song = parse(&text).unwrap();
        assert!(song.channels.len() >= 3);
        let length = song.length;
        let mut player = Player::new(song, 1.0);
        let mut out = vec![0.0; (length as usize + 2000) * 2];
        player.mix(&mut out);
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms > 0.03, "the theme is nearly silent ({rms})");
        assert!(player.time < 4000, "the song did not start over");
    }
}

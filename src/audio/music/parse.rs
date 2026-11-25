//! MML text to a `Song`: instruments, macros, repeats and the channels' notes.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use super::clear;
use super::song::{self, Curve, FilterMode, Instrument, LAYERS, Note, SUM_WORDS, Sampled, Song, Sum, Voicing};
use super::voice::{self, Sing, Syllable};
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
/// Samples a loop's ends lie before its bar lines (`#loop`): a note on the
/// bar line, its start a sum of lengths, may lie a sample before it.
pub(super) const LOOP_SLACK: f64 = 2.0;

/// Frequency of a note: octave 4's A is 440 Hz.
pub(super) fn note_freq(octave: i32, semitone: i32) -> f32 {
    let midi = (octave + 1) * 12 + semitone;
    440.0 * libm::powf(2.0, (midi - 69) as f32 / 12.0)
}

/// The pitch of a note written as MML writes one: a letter, up to two
/// sharps (`+`, `#`) or flats (`-`), an octave of `OCTAVES` (`c4`, `f#3`,
/// `b-2`); None for anything else.
pub fn note_hz(text: &str) -> Option<f32> {
    let mut chars = text.chars();
    let semitone = match chars.next()?.to_ascii_lowercase() {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let octave_at = rest.find(|c: char| c.is_ascii_digit())?;
    let (signs, octave) = rest.split_at(octave_at);
    if signs.len() > 2 || (!signs.chars().all(|c| matches!(c, '+' | '#')) && !signs.chars().all(|c| c == '-')) {
        return None;
    }
    let shift = signs.chars().map(|c| if c == '-' { -1 } else { 1 }).sum::<i32>();
    let octave: i32 = octave.parse().ok().filter(|o| (OCTAVES.0..=OCTAVES.1).contains(o))?;
    Some(note_freq(octave, semitone + shift))
}

/// The note nearest a pitch, as MML writes it (`c4`, `f#3`): what
/// `note_hz` reads back.
pub fn note_name(hz: f32) -> String {
    const LETTERS: [&str; 12] = ["c", "c#", "d", "d#", "e", "f", "f#", "g", "g#", "a", "a#", "b"];
    let midi = (69.0 + 12.0 * libm::log2f(hz.max(1.0) / 440.0)).round() as i32;
    format!("{}{}", LETTERS[midi.rem_euclid(12) as usize], midi.div_euclid(12) - 1)
}

fn parse_instrument(spec: &str) -> Result<Instrument> {
    let mut ins = Instrument::default();
    // A word of the pulse wave's, which a voice has none of, and a word of
    // a voice's, which a wave has none of.
    let (mut pulse, mut sung) = (None, None);
    let mut swept = false;
    // A word of unison's, which only a wave of a pitch has.
    let mut spread = None;
    // The LFO's speeds written (Hz, beats), and whether its cutoff.
    let (mut timed, mut lfo_cut) = ((false, false), false);
    // The wave `sample`, and the words of a sample read (the first one
    // written, for a mistake on another wave).
    let mut sampled = false;
    let mut sample = Sampled { name: String::new(), root: note_freq(4, 0), start: 0.0, end: None, looping: false, reverse: false, line: 0, recording: None };
    let mut sample_word = None;
    let yes = |key: &str, value: Option<&str>| -> Result<bool> {
        match value {
            Some("yes" | "true") => Ok(true),
            Some("no" | "false") => Ok(false),
            _ => bail!("{key}={}: yes or no", value.unwrap_or("")),
        }
    };
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
            "square" | "pulse" => (ins.wave, ins.voice, sampled) = (Wave::Square, None, false),
            "saw" => (ins.wave, ins.voice, sampled) = (Wave::Saw, None, false),
            "triangle" => (ins.wave, ins.voice, sampled) = (Wave::Triangle, None, false),
            "sine" => (ins.wave, ins.voice, sampled) = (Wave::Sine, None, false),
            "noise" => (ins.wave, ins.voice, sampled) = (Wave::Noise, None, false),
            "sample" => (ins.voice, sampled) = (None, true),
            "voice" => (ins.voice, sampled) = (Some(Voicing::Formants), false),
            clear if clear == clear::WAVE => (ins.voice, sampled) = (Some(Voicing::Clear), false),
            "name" => {
                let name = value.filter(|v| !v.is_empty()).context("name needs the sample's name: its file in samples/ without the ending, like name=thump")?;
                if let Some(stem) = name.strip_suffix(".wav").or_else(|| name.strip_suffix(".WAV")) {
                    bail!("name={name}: the name without its ending, name={stem}");
                }
                if name.contains(['/', '\\']) {
                    bail!("name={name}: a sample's name is its file's in samples/, without a folder");
                }
                (sample.name, sample_word) = (name.to_string(), sample_word.or(Some(key)));
            }
            "root" => {
                let v = value.unwrap_or("");
                sample.root = note_hz(v).with_context(|| format!("root={v}: a note, a letter and an octave, like c4, f#3 or b-2"))?;
                sample_word = sample_word.or(Some(key));
            }
            "start" => (sample.start, sample_word) = (num()?, sample_word.or(Some(key))),
            "end" => (sample.end, sample_word) = (Some(num()?), sample_word.or(Some(key))),
            "loop" => (sample.looping, sample_word) = (yes(key, value)?, sample_word.or(Some(key))),
            "reverse" => (sample.reverse, sample_word) = (yes(key, value)?, sample_word.or(Some(key))),
            "duty" => (ins.duty, pulse) = (num()?, Some(key)),
            "pwm" => (ins.pwm, pulse) = (num()?, Some(key)),
            "pwm_speed" => (ins.pwm_speed, pulse) = (num()?, Some(key)),
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
            "sweep" => (ins.sweep, swept) = (num()?, true),
            "sweep_time" => ins.sweep_time = Some(num()?),
            "pitch_start" => ins.pitch_start = Some(num()?),
            "pitch_decay" => ins.pitch_decay = num()?,
            "curve" => {
                ins.curve = match value.context("curve needs linear or exp")? {
                    "linear" => Curve::Linear,
                    "exp" => Curve::Exp,
                    other => bail!("curve={other}: linear or exp"),
                }
            }
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
            "drive" => ins.drive = num()?,
            "drive_end" => ins.drive_end = Some(num()?),
            "drive_time" => ins.drive_time = Some(num()?),
            "consonants" => (ins.consonants, sung) = (num()?, Some(key)),
            "crush_rate" => ins.crush_rate = Some(num()?),
            "crush_bits" => {
                let bits = num()?;
                if bits.fract() != 0.0 {
                    bail!("crush_bits={} is no whole number of bits", value.unwrap_or(""));
                }
                ins.crush_bits = Some(bits as u32);
            }
            "unison" => {
                let n = num()?;
                if n.fract() != 0.0 {
                    bail!("unison={} is no whole number of oscillators", value.unwrap_or(""));
                }
                (ins.unison, spread) = (n as u32, Some(key));
            }
            "detune" => (ins.detune, spread) = (num()?, Some(key)),
            "lfo_speed" => (ins.lfo.speed, timed.0) = (num()?, true),
            "lfo_beats" => (ins.lfo.beats, timed.1) = (Some(num()?), true),
            "lfo_wave" => {
                let v = value.context("lfo_wave needs sine, triangle, square, saw or noise")?;
                ins.lfo.wave = Wave::ALL.iter().find(|(n, _)| *n == v).map(|(_, w)| *w).with_context(|| format!("lfo_wave={v}: sine, triangle, square, saw or noise"))?;
            }
            "lfo_cutoff" => (ins.lfo.cutoff, lfo_cut) = (num()?, true),
            "lfo_volume" => ins.lfo.volume = num()?,
            "wave" => bail!("the wave is a bare word, not wave=...: square, pulse, saw, triangle, sine, noise, sample, voice or {} (like @lead square duty=0.25)", clear::WAVE),
            other => {
                let known = words::WORDS.iter().filter(|w| w.mml.is_some() && w.name != "wave" && w.name != other).map(|w| w.name).chain(["pulse", "sample", "voice", clear::WAVE]).chain(Wave::ALL.iter().map(|(n, _)| *n)).filter(|w| *w != other);
                match words::near(other, known) {
                    Some(near) => bail!("unknown instrument option '{other}': did you mean '{near}'?"),
                    None => bail!("unknown instrument option '{other}'"),
                }
            }
        }
    }
    if let Some(word) = pulse.filter(|_| ins.voice.is_some()) {
        bail!("{word} means nothing on a voice: it shapes the pulse of a square wave, and a voice sings");
    }
    let wave = if sampled { "sample" } else { ins.wave.name() };
    if let Some(word) = sung.filter(|_| ins.voice.is_none()) {
        bail!("{word} means nothing on a {wave}: only a voice (voice or {}) sings consonants", clear::WAVE);
    }
    if let Some(word) = spread.filter(|_| ins.voice.is_some() || sampled || ins.wave == Wave::Noise) {
        let what = match ins.voice {
            Some(Voicing::Clear) => clear::WAVE,
            Some(Voicing::Formants) => "voice",
            None => wave,
        };
        bail!("{word} means nothing on a {what}: unison spreads the oscillators of a square, saw, triangle or sine");
    }
    if timed == (true, true) {
        bail!("lfo_speed and lfo_beats both say how fast the LFO turns: take one (lfo_speed in Hz from each note's start, lfo_beats in the song's beats)");
    }
    if lfo_cut && ins.filter.is_none() {
        bail!("lfo_cutoff moves the filter's cutoff, and this line has no filter: write filter=lowpass (or bandpass, highpass) or a cutoff with it");
    }
    if swept && ins.pitch_start.is_some() {
        bail!("pitch_start and sweep both bend the pitch: take one (pitch_start falls in Hz to the note's pitch, sweep slides in semitones)");
    }
    if sampled {
        if let Some(word) = pulse {
            bail!("{word} means nothing on a sample: it shapes the pulse of a square wave, and a sample plays a recording");
        }
        if sample.name.is_empty() {
            bail!("the wave sample needs the recording's name: name=thump plays samples/thump.wav");
        }
        if let Some(end) = sample.end.filter(|&end| end <= sample.start) {
            bail!("start={} and end={end}: a sample's stretch ends after it begins", sample.start);
        }
        ins.sample = Some(sample);
    } else if let Some(word) = sample_word {
        let what = if ins.voice.is_some() { "voice" } else { wave };
        bail!("{word} means nothing on a {what}: only the wave sample plays a recording (@break sample name=...)");
    }
    Ok(ins)
}

/// The words a line of an instrument's options writes, in its order.
fn keys(spec: &str) -> impl Iterator<Item = &str> {
    spec.split_whitespace().map(|w| w.split_once('=').map_or(w, |(k, _)| k))
}

/// A further layer of an instrument (`+`): a wave and an instrument's
/// words, but those of the note as a whole, which the instrument's own
/// line says (how it ducks and is ducked).
fn parse_layer(spec: &str) -> Result<Instrument> {
    if let Some(key) = keys(spec).find(|k| ["sidechain", "sidechain_release", "duck"].contains(k)) {
        bail!("{key} is the whole note's, not a layer's: write it on the instrument's own line");
    }
    let ins = parse_instrument(spec)?;
    if ins.voice.is_some() {
        bail!("a layer is a wave (square, saw, triangle, sine, noise or sample), not a voice");
    }
    Ok(ins)
}

/// What a layered instrument's layers go through once added (`=`): only
/// the words of `SUM_WORDS`, a volume of 1 and no drive unless stated.
fn parse_sum(spec: &str) -> Result<Sum> {
    if let Some(key) = keys(spec).find(|k| !SUM_WORDS.contains(k)) {
        let near = words::near(key, SUM_WORDS.iter().copied()).map_or(String::new(), |w| format!(" (did you mean {w}?)"));
        bail!("the sum takes {}, not {key}{near}", SUM_WORDS.join(", "));
    }
    let ins = parse_instrument(spec)?;
    let stated = |word: &str| keys(spec).any(|k| k == word);
    Ok(Sum {
        drive: ins.drive,
        drive_end: ins.drive_end,
        drive_time: ins.drive_time,
        volume: if stated("volume") { ins.volume } else { 1.0 },
        filter: ins.filter,
        cutoff: ins.cutoff,
        resonance: ins.resonance,
        crush_rate: ins.crush_rate,
        crush_bits: ins.crush_bits,
        lfo: ins.lfo,
        drive_stated: stated("drive") || stated("drive_end"),
        cutoff_stated: stated("cutoff"),
        resonance_stated: stated("resonance"),
        lfo_stated: keys(spec).any(|k| k.starts_with("lfo_")),
    })
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
    /// A laid channel's length so far, exactly (None in a file's song;
    /// none either once past what it can hold).
    beats: Option<Beats>,
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
    /// Which instruments sing (`voice`).
    voices: &'a [Option<Voicing>],
    /// A quoted text read, for the next note to sing.
    text: Option<Syllable>,
    /// The voice notes so far, and whether one sang a text yet.
    sings: Vec<Sing>,
    sung: bool,
    /// The song names no instrument: its channels play the plain square.
    plain: bool,
    /// A channel a game laid (`lay`): no `t`, no singing.
    laid: bool,
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
    /// A laid channel's length grows by it exactly (`State::beats`).
    fn duration(&mut self, state: &mut State) -> Result<f32> {
        let length = match self.number() {
            Some(n) if (LENGTHS.0..=LENGTHS.1).contains(&n) => n,
            Some(n) => bail!("a note length of {n} is out of range, {} to {}", LENGTHS.0, LENGTHS.1),
            None => state.length,
        };
        let mut beats = 4.0 / length;
        let mut dot = beats / 2.0;
        let mut dots = 0;
        while self.peek() == Some('.') {
            self.at += 1;
            beats += dot;
            dot /= 2.0;
            dots += 1;
        }
        state.beats = state.beats.and_then(|b| b.plus(Beats::of(length as u32, dots)?));
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

    /// Instrument `index` as a mistake names it.
    fn instrument_name(&self, index: usize) -> String {
        match self.instruments.iter().find(|(_, i)| **i == index) {
            Some((name, _)) if !self.plain => format!("@{name}"),
            _ => "the plain square".to_string(),
        }
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
                    let text = self.text.take();
                    if let Some(voicing) = self.voices[state.instrument] {
                        if self.laid {
                            bail!("{} sings ({}), which a channel a game lays does not yet", self.instrument_name(state.instrument), if voicing == Voicing::Clear { clear::WAVE } else { "voice" });
                        }
                        if text.is_none() && !self.sung {
                            bail!("a voice's first note needs a text, the syllable it sings, like \"l a\" c4 (a note without one holds the vowel before)");
                        }
                        self.sung = true;
                        let end = state.time + seconds;
                        self.sings.push(Sing { note: notes.len(), text, at: (state.time * rate) as u64, end: (end * rate) as u64, voicing });
                    } else if text.is_some() {
                        bail!("a text is sung only by a voice, and {} plays no voice (an instrument like @vox voice)", self.instrument_name(state.instrument));
                    }
                    notes.push(Note {
                        start: (state.time * rate) as u64,
                        held: (seconds * state.gate as f64 * rate) as u64,
                        freq: Some(note_freq(note_octave, semitone)),
                        instrument: state.instrument,
                        volume: state.volume,
                        pan: state.pan,
                        sung: None,
                    });
                    state.time += seconds;
                }
                '"' => {
                    let start = self.at;
                    let Some(len) = self.chars[start..].iter().position(|&c| c == '"') else { bail!("a text without its closing quote") };
                    self.at = start + len + 1;
                    if self.text.is_some() {
                        bail!("two texts before one note: each note sings one syllable");
                    }
                    let text: String = self.chars[start..start + len].iter().collect();
                    self.text = Some(voice::syllable(&text)?);
                }
                'r' => {
                    if self.text.is_some() {
                        bail!("a text before a rest: the note after a text sings it");
                    }
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
                't' if self.laid => bail!("t sets a channel's own tempo, which a channel a game lays takes from its file of instruments (#tempo)"),
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
        if self.text.is_some() {
            bail!("a text with no note after it to sing it");
        }
        Ok(())
    }
}

pub fn parse(text: &str) -> Result<Song> {
    parse_song(text, false)
}

/// A song file or a studio (`Song::studio`): a file of instruments and
/// settings without a channel, which a game lays its own channels over
/// (`lay`). A studio goes round whatever is laid whole, so it takes no
/// `#loop`.
pub fn read(text: &str) -> Result<Song> {
    parse_song(text, true)
}

/// Whether a song file's text has a channel line (`A: c d e`), as the
/// reader tells one: a file without is a studio even where it does not
/// read.
pub fn has_channels(text: &str) -> bool {
    text.lines().map(|l| l.split(';').next().unwrap_or("").trim()).any(|l| !l.starts_with(['#', '@', '$']) && l.split_once(':').is_some_and(|(c, _)| c.trim().chars().count() == 1))
}

/// The song a game lays over a studio (`music:play`'s `channels`): the
/// studio's instruments, settings and macros, and the laid channels in
/// their letters' order. Every laid channel fills the same number of whole
/// bars (counted exactly, in beats), changes no tempo (`t`) and sings no
/// voice; a mistake names the channel, no line (the text is in no file).
pub fn lay(studio: &str, laid: &Laid) -> Result<Song> {
    let head = read_head(studio, true)?;
    if !head.lines.is_empty() {
        bail!("the song has channels of its own: a game lays channels over a file of instruments only");
    }
    let channels = laid.channels.iter().map(|(c, text)| (*c, text.chars().map(|ch| (ch, 0)).collect())).collect();
    let mut song = build(head, channels, true)?;
    song.sources = laid.channels.iter().map(|(_, text)| text_hash(text)).collect();
    song.laid = Some(laid.digest);
    Ok(song)
}

/// One channel's text as a number: whether a laid channel stayed the same.
fn text_hash(text: &str) -> u64 {
    u64::from_le_bytes(blake3::hash(text.as_bytes()).as_bytes()[..8].try_into().expect("eight bytes"))
}

/// Channels a game lays over a studio (`music:play`'s `channels`): each
/// letter's MML, in the letters' order, and their digest (BLAKE3 of each
/// letter and its text, length first): the same channels, the same digest,
/// on every machine (only the text counts, never what is read of it).
#[derive(Clone, Debug)]
pub struct Laid {
    pub channels: Vec<(char, String)>,
    pub digest: [u8; 32],
}

impl Laid {
    pub fn new(mut channels: Vec<(char, String)>) -> Laid {
        channels.sort_by_key(|(c, _)| *c);
        let mut h = blake3::Hasher::new();
        for (c, text) in &channels {
            h.update(&u32::from(*c).to_le_bytes()).update(&(text.len() as u64).to_le_bytes()).update(text.as_bytes());
        }
        Laid { digest: *h.finalize().as_bytes(), channels }
    }
}

impl PartialEq for Laid {
    fn eq(&self, other: &Laid) -> bool {
        self.digest == other.digest
    }
}

/// What a song file says before its channels: its settings, instruments,
/// macros and channel lines.
struct Head {
    title: String,
    tempo: f32,
    meter: u32,
    looping: bool,
    /// `#loop`: its bars, and its line once given.
    round: Option<(u32, Option<u32>)>,
    loop_line: Option<usize>,
    layered: Vec<(char, f32)>,
    instruments: Vec<Instrument>,
    names: HashMap<String, usize>,
    macros: HashMap<String, String>,
    lines: Vec<(char, String, usize)>,
    /// It names no instrument: its channels play the plain square.
    plain: bool,
}

pub(super) fn parse_song(text: &str, may_be_empty: bool) -> Result<Song> {
    let head = read_head(text, may_be_empty)?;
    // Lines of the same channel continue each other.
    let mut channels: Vec<(char, Vec<(char, usize)>)> = Vec::new();
    for (channel, notes, n) in &head.lines {
        let at = match channels.iter().position(|(c, _)| c == channel) {
            Some(at) => at,
            None => {
                channels.push((*channel, Vec::new()));
                channels.len() - 1
            }
        };
        let text = &mut channels[at].1;
        text.push((' ', *n));
        text.extend(notes.chars().map(|c| (c, *n)));
    }
    build(head, channels, false)
}

fn read_head(text: &str, may_be_empty: bool) -> Result<Head> {
    let mut head = Head {
        title: String::new(),
        tempo: 120.0,
        meter: 4,
        looping: true,
        round: None,
        loop_line: None,
        layered: Vec::new(),
        instruments: Vec::new(),
        names: HashMap::new(),
        macros: HashMap::new(),
        lines: Vec::new(),
        plain: false,
    };
    // The instrument whose layers (`+`) and sum (`=`) may come next: its
    // place, its name, and its sum's line once given.
    let mut open: Option<(usize, String, Option<usize>)> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = || format!("line {}", n + 1);
        if let Some(rest) = line.strip_prefix(['+', '=']) {
            let sum = line.starts_with('=');
            let what = if sum { "a sum (=)" } else { "a layer (+)" };
            let Some((index, name, sum_line)) = &mut open else {
                bail!("{}: {what} goes right under an instrument's line (@name ...){}", at(), if sum { " and its layers" } else { " or its other layers" });
            };
            let ins = &mut head.instruments[*index];
            if ins.voice.is_some() {
                bail!("{}: @{name} sings, and a voice takes no layers and no sum: one instrument a voice", at());
            }
            if let Some(line) = sum_line {
                bail!("{}: @{name} has its sum (=) on line {line} already{}", at(), if sum { ": an instrument takes one" } else { ", and its layers come before it" });
            }
            if sum {
                ins.sum = Some(parse_sum(rest).with_context(|| format!("{}: @{name}'s sum (=)", at()))?);
                *sum_line = Some(n + 1);
            } else {
                if ins.layers.len() + 1 >= LAYERS {
                    bail!("{}: @{name} has {LAYERS} layers already (its own line and {} more), the most an instrument takes", at(), LAYERS - 1);
                }
                let mut layer = parse_layer(rest).with_context(|| format!("{}: @{name}'s layer {}", at(), ins.layers.len() + 2))?;
                layer.sample.iter_mut().for_each(|s| s.line = n + 1);
                ins.layers.push(layer);
            }
            continue;
        }
        open = None;
        if let Some(rest) = line.strip_prefix('#') {
            let (key, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            match key {
                "title" => head.title = value.trim().to_string(),
                "tempo" => {
                    head.tempo = value.trim().parse::<f32>().ok().filter(|t| (TEMPOS.0..=TEMPOS.1).contains(t)).with_context(|| format!("{}: #tempo is the beats a minute, {} to {} (not '{}')", at(), TEMPOS.0, TEMPOS.1, value.trim()))?
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
                            (Some(c), None) => head.layered.push((c, from)),
                            _ => bail!("{}: #layer: '{l}' is no channel (a letter)", at()),
                        }
                    }
                }
                "meter" => {
                    // 3, or 3/4: the beats in a bar.
                    let beats = value.trim().split('/').next().unwrap_or("");
                    head.meter = beats.trim().parse::<u32>().ok().filter(|&n| n > 0).with_context(|| format!("{}: #meter is the beats in a bar, like 4 or 3/4", at()))?;
                }
                "loop" => {
                    if head.loop_line.is_some() {
                        bail!("{}: #loop is given twice (a song takes one)", at());
                    }
                    head.loop_line = Some(n + 1);
                    let wrong = || format!("{}: #loop is the bars that go round, whole numbers from 1: like 33 80, or 33 (from bar 33 to the end), or no (it plays once); not '{}'", at(), value.trim());
                    let bar = |w: &str| -> Result<u32> {
                        match w.parse::<u32>() {
                            Ok(0) => bail!("{}: #loop: bars count from 1 (the song's first), not 0", at()),
                            Ok(b) => Ok(b),
                            Err(_) => bail!(wrong()),
                        }
                    };
                    match value.split_whitespace().collect::<Vec<_>>().as_slice() {
                        [] | ["yes" | "on" | "true"] => {}
                        ["no" | "off" | "false"] => head.looping = false,
                        [from] => head.round = Some((bar(from)?, None)),
                        [from, to] => head.round = Some((bar(from)?, Some(bar(to)?))),
                        _ => bail!(wrong()),
                    }
                }
                "inst" | "instrument" => bail!("{}: an instrument is written @name options, like @lead square duty=0.25 (not #{key})", at()),
                other => bail!("{}: unknown #{other}", at()),
            }
        } else if let Some(rest) = line.strip_prefix('@') {
            let (name, spec) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let mut ins = parse_instrument(spec).with_context(|| format!("{}: @{name}", at()))?;
            ins.sample.iter_mut().for_each(|s| s.line = n + 1);
            head.names.insert(name.to_string(), head.instruments.len());
            open = Some((head.instruments.len(), name.to_string(), None));
            head.instruments.push(ins);
        } else if let Some(rest) = line.strip_prefix('$') {
            let (name, body) = rest.split_once('=').with_context(|| format!("{}: a macro is $name = notes", at()))?;
            head.macros.insert(name.trim().to_string(), body.trim().to_string());
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
            head.lines.push((letter, notes.to_string(), n + 1));
        } else {
            bail!("line {}: expected #setting, @instrument, $macro or a channel like 'A: c d e'", n + 1);
        }
    }
    if head.lines.is_empty() && !may_be_empty {
        bail!("the song has no channel: write one like 'A: c d e f' (a letter, a colon, notes)");
    }
    head.plain = head.instruments.is_empty();
    if head.plain {
        head.instruments.push(Instrument::default());
        head.names.insert("default".into(), 0);
    }
    Ok(head)
}

/// The song of `head` with `channels` (each letter's characters with the
/// line of the file they are on): laid ones (`lay`) or the file's own.
fn build(head: Head, channels: Vec<(char, Vec<(char, usize)>)>, laid: bool) -> Result<Song> {
    let Head { title, tempo, meter, looping, round, loop_line, layered, instruments, names, macros, lines, plain } = head;
    if let (true, Some(line)) = (laid || lines.is_empty(), loop_line) {
        bail!("line {line}: #loop: a file of instruments goes round whatever channels a game lays over it whole, so it takes no #loop");
    }
    let mut notes_of = Vec::new();
    let (mut segments, mut clear) = (Vec::new(), Vec::new());
    // Where the song ends exactly, a fraction of a sample too.
    let mut ends = 1.0f64;
    let voices: Vec<Option<Voicing>> = instruments.iter().map(|i: &Instrument| i.voice).collect();
    let letters: Vec<char> = channels.iter().map(|(c, _)| *c).collect();
    // The first laid channel's length, exactly, and its letter.
    let mut filled: Option<(char, Beats)> = None;
    for (channel, text) in channels {
        let (text, lines) = expand_lines(&text, &macros).with_context(|| format!("channel {channel}"))?;
        let mut state = State { octave: 4, length: 4.0, volume: 0.8, gate: 0.875, pan: 0.0, tempo, instrument: 0, time: 0.0, beats: laid.then_some(Beats::ZERO) };
        let mut notes = Vec::new();
        let mut parser = Parser { chars: text, lines, at: 0, steps: 0, instruments: &names, voices: &voices, text: None, sings: Vec::new(), sung: false, plain, laid };
        parser.sequence(&mut state, &mut notes, None).with_context(|| {
            let around: String = parser.chars[parser.at.saturating_sub(12)..(parser.at + 4).min(parser.chars.len())].iter().collect();
            let line = parser.lines.get(parser.at.saturating_sub(1)).or(parser.lines.last()).copied().unwrap_or(0);
            if laid { format!("channel {channel}, near '{}'", around.trim()) } else { format!("line {line}: channel {channel}, near '{}'", around.trim()) }
        })?;
        if laid {
            let beats = state.beats.with_context(|| format!("channel {channel}: its lengths are too many odd ones to add up exactly"))?;
            let bars = beats.bars(meter).with_context(|| format!("channel {channel} lasts {}: a laid channel fills whole bars of {meter} beats", beats.said()))?;
            match filled {
                None => filled = Some((channel, beats)),
                Some((first, b)) if b != beats => bail!("channel {channel} fills {}, channel {first} {}: every laid channel fills as many bars", said_bars(bars), said_bars(b.bars(meter).unwrap_or(0))),
                _ => {}
            }
        }
        voice::lay_out(&mut notes, &parser.sings, &mut segments, &mut clear);
        // The channel's end is where its last note or rest ends.
        let end = (state.time * SAMPLE_RATE as f64) as u64;
        ends = ends.max(state.time * SAMPLE_RATE as f64);
        notes.push(Note { start: end, held: 0, freq: None, instrument: 0, volume: 0.0, pan: 0.0, sung: None });
        notes_of.push(notes);
    }
    let channels = notes_of;
    let length = channels.iter().filter_map(|c| c.last().map(|n| n.start)).max().unwrap_or(0).max(1);
    // A studio's layers name the letters a game lays, laid now or not.
    if !laid
        && !lines.is_empty()
        && let Some((c, _)) = layered.iter().find(|(c, _)| !letters.contains(c))
    {
        bail!("#layer names channel {c}, which plays nothing");
    }
    let layers = letters.iter().map(|c| layered.iter().rev().find(|(l, _)| l == c).map_or(0.0, |(_, f)| *f)).collect();
    // Bars begun (a sum of note lengths a sample short of a bar line
    // begins none more).
    let bar = f64::from(meter) * 60.0 / f64::from(tempo) * SAMPLE_RATE as f64;
    let bars = ((length as f64 - LOOP_SLACK).max(0.0) / bar).ceil().clamp(1.0, f64::from(u32::MAX)) as u32;
    let mut span = (0.0, ends);
    if let Some((from, to)) = round {
        let line = loop_line.unwrap_or(0);
        for b in std::iter::once(from).chain(to) {
            if b > bars {
                bail!("line {line}: #loop: bar {b} is past the song's end (it has {bars} bar{})", if bars == 1 { "" } else { "s" });
            }
        }
        if let Some(to) = to.filter(|&to| to < from) {
            bail!("line {line}: #loop from bar {from} to bar {to}: the first bar comes first (like #loop {to} {from})");
        }
        // Exactly as long as its bars (the game's clock goes round with
        // them), its ends a little before their bar lines but where they
        // are the song's own start and end.
        let line = |bar: u32| song::bar_line(tempo, meter, bar);
        let last = to.map_or(ends, |to| line(to.saturating_add(1)));
        let end = if to.is_some() || from > 1 { last - LOOP_SLACK } else { last };
        span = (end - (last - line(from)), end);
    }
    Ok(Song { title, tempo, meter, layers, names: letters, instruments, channels, segments, clear, length, looping, round, span, bars, sources: Vec::new(), laid: None })
}

/// `n` bars in words.
fn said_bars(n: u64) -> String {
    if n == 1 { "1 bar".into() } else { format!("{n} bars") }
}

/// A length in beats, exactly: a fraction (note lengths are 4 over 1 to
/// 192, each dot adding half the last; ties and repeats add), so whether
/// a laid channel fills whole bars never hangs on rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Beats {
    num: u128,
    den: u128,
}

impl Beats {
    const ZERO: Beats = Beats { num: 0, den: 1 };

    /// Four over `length` with `dots`; None past what it can hold.
    fn of(length: u32, dots: u32) -> Option<Beats> {
        // 4/n (2 - 1/2^d) = 4 (2^(d+1) - 1) / (n 2^d)
        let two = 1u128.checked_shl(dots).filter(|t| t.leading_zeros() > 8)?;
        Beats { num: 4 * (two * 2 - 1), den: u128::from(length) * two }.reduced()
    }

    fn plus(self, other: Beats) -> Option<Beats> {
        let g = gcd(self.den, other.den);
        let den = (self.den / g).checked_mul(other.den)?;
        let num = self.num.checked_mul(other.den / g)?.checked_add(other.num.checked_mul(self.den / g)?)?;
        Beats { num, den }.reduced()
    }

    fn reduced(self) -> Option<Beats> {
        let g = gcd(self.num, self.den).max(1);
        Some(Beats { num: self.num / g, den: self.den / g })
    }

    /// The whole bars of `meter` beats it is, if it is whole bars (and at
    /// least one).
    fn bars(self, meter: u32) -> Option<u64> {
        let meter = u128::from(meter);
        (self.den == 1 && self.num > 0 && self.num.is_multiple_of(meter)).then(|| u64::try_from(self.num / meter).unwrap_or(u64::MAX))
    }

    /// As a person says it: "3 beats", "1/2 beat", "7 and 1/4 beats".
    fn said(self) -> String {
        let (whole, rest) = (self.num / self.den, self.num % self.den);
        match (whole, rest) {
            (1, 0) => "1 beat".into(),
            (w, 0) => format!("{w} beats"),
            (0, r) => format!("{r}/{} beat", self.den),
            (w, r) => format!("{w} and {r}/{} beats", self.den),
        }
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

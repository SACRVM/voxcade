//! A sound is a file of the game: `sounds/NAME.toml`, a few numbers a line
//! (the words of `audio::words`), or `sounds/NAME.wav`, a recording as it
//! is. The file's name without its ending is the sound's name, the one
//! `level:sound("coin", at)` and a kind's `sound = "coin"` call it by. A
//! file names only what differs from the defaults:
//!
//! ```toml
//! # a coin: one note, then a fourth up
//! wave = "triangle"
//! freq = 880
//! jump = 5
//! jump_at = 0.05
//! hold = 0.04
//! decay = 0.16
//! volume = 0.25
//! ```
//!
//! A recording made lo-fi names it: `file = "recordings/scream.wav"`, `rate
//! = 7000`, `bits = 8`; `silent = true` silences a sound the engine or the
//! ready-made rules bring. The kits' own sounds are files of the same kind,
//! built into the program (`builtin`); a game's file of the same name
//! replaces one.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, anyhow, bail};

use super::synth::{self, Synth, Wave};
use super::words::{self, Form, Unit};

/// The folder of a game's sounds.
pub const FOLDER: &str = "sounds";

/// What a sound is.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundDef {
    Synth(Synth),
    Recording(Recording),
    /// No sound at all (`silent = true`).
    Silent,
}

/// A `.wav` file, made lo-fi if asked: down to `rate` Hz and `bits` bits
/// (7000 and 8 sound like the speech in Wolfenstein 3D).
#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    /// Its name in the game's files (`recordings/scream.wav`).
    pub file: String,
    pub rate: Option<u32>,
    pub bits: Option<u32>,
    pub volume: f32,
}

/// One of the game's own sounds: the file it is read from (its name in
/// the game, `sounds/coin.toml`) and what it says.
#[derive(Clone, Debug, PartialEq)]
pub struct GameSound {
    pub file: String,
    pub def: SoundDef,
}

/// Endings of recordings the engine does not read, to say so instead of
/// passing them over.
const OTHER_AUDIO: [&str; 7] = ["ogg", "mp3", "flac", "aif", "aiff", "m4a", "opus"];

/// A sound file's text as the engine's sound. `file` names it in messages
/// (`sounds/coin.toml`); a mistake says the file, the line and the word.
pub fn read(file: &str, text: &str) -> Result<SoundDef> {
    let table: BTreeMap<String, toml::Spanned<toml::Value>> = toml::from_str(text).map_err(|e| {
        let line = e.span().map(|s| line_of(text, s.start)).map(|l| format!(" line {l}")).unwrap_or_default();
        anyhow!("{file}{line}: {}", e.message().trim())
    })?;
    let at = |name: &str| table.get(name).map_or(0, |v| line_of(text, v.span().start));
    let said = |name: &str, what: String| anyhow!("{file} line {}: {name}{what}", at(name));
    // The form: a recording if it names a file, nothing if silent, else
    // numbers for the synth.
    let silent = match table.get("silent").map(|v| v.get_ref()) {
        None => false,
        Some(toml::Value::Boolean(b)) => *b,
        Some(_) => return Err(said("silent", ": true or false".into())),
    };
    let form = if silent {
        Form::Alone
    } else if table.contains_key("file") {
        Form::Recording
    } else {
        Form::Synth
    };
    for (name, value) in &table {
        let Some((_, u)) = words::sound_words().find(|(n, _)| n == name) else {
            let line = line_of(text, value.span().start);
            if let Some((_, now)) = words::RENAMED.iter().find(|(old, _)| old == name) {
                bail!("{file} line {line}: {name}: a sound says {now}");
            }
            if words::word(name).is_some_and(|w| w.mml.is_some()) {
                bail!("{file} line {line}: {name}: a word of MML instruments, not of sounds (a sound's words: {})", sound_words());
            }
            let renamed = words::RENAMED.iter().map(|(old, _)| *old);
            let near = match (words::near(name, words::sound_words().map(|(n, _)| n)), words::near(name, renamed)) {
                (Some(near), _) => format!(" (did you mean {near}?)"),
                (None, Some(old)) => format!(" (did you mean {old}? a sound says {})", words::RENAMED.iter().find(|(o, _)| *o == old).map_or("", |(_, now)| now.split(' ').next().unwrap_or(now))),
                (None, None) => format!(" (a sound's words: {})", sound_words()),
            };
            bail!("{file} line {line}: {name}: no such field{near}");
        };
        let fits = match (form, u.form) {
            (Form::Alone, _) => *name == "silent",
            (_, Form::Both) => true,
            (form, own) => form == own || own == Form::Alone,
        };
        if !fits {
            let why = match form {
                Form::Alone => "a silent sound says nothing else".to_string(),
                Form::Recording => "a recording (file = ...) takes only file, rate, bits and volume".to_string(),
                _ => format!("{name} belongs to a recording, with file = \"...\""),
            };
            return Err(said(name, format!(": {why}")));
        }
        // Numbers in their range.
        if let Some((lo, hi)) = u.range {
            let n = number(value.get_ref()).ok_or_else(|| said(name, ": not a number".into()))?;
            if !(lo..=hi).contains(&n) {
                return Err(said(name, format!(" = {}: {} ({})", value.get_ref(), words::range_words(u), u.meaning)));
            }
            if (matches!(u.unit, Unit::Bits) || *name == "rate") && n.fract() != 0.0 {
                return Err(said(name, format!(" = {n}: a whole number {}", words::range_words(u))));
            }
        }
    }
    let num = |name: &str| table.get(name).and_then(|v| number(v.get_ref()));
    let text_of = |name: &str| -> Result<Option<String>> {
        match table.get(name).map(|v| v.get_ref()) {
            None => Ok(None),
            Some(toml::Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(said(name, ": a word in quotes".into())),
        }
    };
    match form {
        Form::Alone => Ok(SoundDef::Silent),
        Form::Recording => {
            let path = text_of("file")?.unwrap_or_default();
            if !path.to_ascii_lowercase().ends_with(".wav") {
                return Err(said("file", format!(" = \"{path}\": a .wav file, its path from the game folder")));
            }
            Ok(SoundDef::Recording(Recording { file: path, rate: num("rate").map(|n| n as u32), bits: num("bits").map(|n| n as u32), volume: num("volume").map_or(1.0, |n| n as f32) }))
        }
        _ => {
            let d = Synth::default();
            let wave = match text_of("wave").map_err(|_| said("wave", format!(": a word in quotes, one of {}", waves())))? {
                None => d.wave,
                Some(w) if w == "pulse" => Wave::Square,
                Some(w) => Wave::ALL.iter().find(|(n, _)| *n == w).map(|(_, wave)| *wave).ok_or_else(|| said("wave", format!(": \"{w}\" is no wave ({})", waves())))?,
            };
            let f = |name: &str, default: f32| num(name).map_or(default, |n| n as f32);
            let s = Synth {
                wave,
                freq: f("freq", d.freq),
                sweep: num("sweep").unwrap_or(d.sweep),
                duty: f("duty", d.duty),
                attack: f("attack", d.attack),
                hold: f("hold", d.hold),
                decay: f("decay", d.decay),
                punch: f("punch", d.punch),
                vibrato: f("vibrato", d.vibrato),
                vibrato_speed: f("vibrato_speed", d.vibrato_speed),
                jump: num("jump").unwrap_or(d.jump),
                jump_at: f("jump_at", d.jump_at),
                lowpass: num("lowpass").map(|n| n as f32),
                lowpass_end: num("lowpass_end").map(|n| n as f32),
                volume: f("volume", d.volume),
            };
            // What the numbers say together.
            let length = s.length();
            if length > synth::LONGEST {
                let last = ["attack", "hold", "decay"].into_iter().max_by_key(|n| at(n)).unwrap_or("decay");
                return Err(said(last, format!(": attack, hold and decay last {length:.2} seconds together; a sound lasts {} seconds at most (longer is music: c.audio.music)", synth::LONGEST)));
            }
            let end = s.freq_end() as f64;
            if s.sweep != 0.0 && !(words::PITCHES.0..=words::PITCHES.1).contains(&end) {
                return Err(said("sweep", format!(" = {}: slides the pitch from {} Hz to {end:.0} Hz; it ends from {} to {} Hz", s.sweep, s.freq, words::PITCHES.0, words::PITCHES.1)));
            }
            if s.jump != 0.0 && s.jump_at as f64 >= length as f64 {
                return Err(said("jump_at", format!(" = {}: after the sound's end ({length:.2} seconds), so it never jumps", s.jump_at)));
            }
            if s.lowpass_end.is_some() && s.lowpass.is_none() {
                return Err(said("lowpass_end", ": needs lowpass, where the filter starts".into()));
            }
            Ok(SoundDef::Synth(s))
        }
    }
}

/// A TOML number as f64 (integers too: `freq = 880`); None for anything
/// else, or a number that is none (nan, inf).
fn number(v: &toml::Value) -> Option<f64> {
    match v {
        toml::Value::Integer(i) => Some(*i as f64),
        toml::Value::Float(f) if f.is_finite() => Some(*f),
        _ => None,
    }
}

/// The line (from 1) a byte of `text` is on.
fn line_of(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].bytes().filter(|b| *b == b'\n').count() + 1
}

fn waves() -> String {
    Wave::ALL.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
}

fn sound_words() -> String {
    words::sound_words().map(|(n, _)| n).collect::<Vec<_>>().join(", ")
}

/// A sound as its file's text: what differs from the defaults, in the
/// register's order, after `comments` as `#` lines.
pub fn write(def: &SoundDef, comments: &[String]) -> String {
    let mut out: String = comments.iter().map(|c| if c.trim().is_empty() { "#\n".to_string() } else { format!("# {}\n", c.trim_end()) }).collect();
    let mut line = |name: &str, value: String| out.push_str(&format!("{name} = {value}\n"));
    match def {
        SoundDef::Silent => line("silent", "true".into()),
        SoundDef::Recording(r) => {
            line("file", format!("{:?}", r.file));
            if let Some(rate) = r.rate {
                line("rate", rate.to_string());
            }
            if let Some(bits) = r.bits {
                line("bits", bits.to_string());
            }
            if r.volume != 1.0 {
                line("volume", r.volume.to_string());
            }
        }
        SoundDef::Synth(s) => {
            let d = Synth::default();
            let differs = |v: f32, d: f32| (v != d).then(|| v.to_string());
            let written = [
                // Always the wave, so a file says what it is.
                ("wave", Some(format!("\"{}\"", s.wave.name()))),
                ("freq", differs(s.freq, d.freq)),
                ("sweep", (s.sweep != d.sweep).then(|| s.sweep.to_string())),
                ("duty", differs(s.duty, d.duty)),
                ("attack", differs(s.attack, d.attack)),
                ("hold", differs(s.hold, d.hold)),
                ("decay", differs(s.decay, d.decay)),
                ("punch", differs(s.punch, d.punch)),
                ("vibrato", differs(s.vibrato, d.vibrato)),
                ("vibrato_speed", differs(s.vibrato_speed, d.vibrato_speed)),
                ("jump", (s.jump != d.jump).then(|| s.jump.to_string())),
                ("jump_at", differs(s.jump_at, d.jump_at)),
                ("lowpass", s.lowpass.map(|v| v.to_string())),
                ("lowpass_end", s.lowpass_end.map(|v| v.to_string())),
                ("volume", differs(s.volume, d.volume)),
            ];
            for (name, value) in written {
                if let Some(value) = value {
                    line(name, value);
                }
            }
        }
    }
    out
}

/// Sounds built into the program (a kit's, the engine's), each by its
/// name and its file's text. A mistake in one is the program's: the tests
/// read every one.
pub fn builtin(list: &[(&'static str, &'static str)]) -> Vec<(&'static str, SoundDef)> {
    list.iter().map(|(name, text)| (*name, read(&format!("{name}.toml (built in)"), text).unwrap_or_else(|e| panic!("{e:#}")))).collect()
}

/// The sound files of a game, by name (the file's name without its
/// ending): `sounds/NAME.toml` or `sounds/NAME.wav`. Two files of one name
/// are a mistake, as is a recording the engine cannot read or a sound
/// naming a recording that is not there.
pub fn read_folder(files: &crate::files::Files) -> Result<BTreeMap<String, GameSound>> {
    let mut sounds: BTreeMap<String, GameSound> = BTreeMap::new();
    for path in files.list(FOLDER) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let Some((stem, ending)) = name.rsplit_once('.') else { continue };
        if name.starts_with('.') {
            continue;
        }
        let file = format!("{FOLDER}/{name}");
        let ending = ending.to_ascii_lowercase();
        let def = match ending.as_str() {
            "toml" => {
                let text = files.read_to_string(&path).map_err(|e| anyhow!("{file}: {e}"))?;
                let def = read(&file, &text)?;
                if let SoundDef::Recording(r) = &def
                    && !files.is_file(&r.file)
                {
                    let line = text.lines().position(|l| l.trim_start().starts_with("file")).map_or(1, |i| i + 1);
                    bail!("{file} line {line}: file = \"{}\": no such file in the game", r.file);
                }
                def
            }
            "wav" => SoundDef::Recording(Recording { file: file.clone(), rate: None, bits: None, volume: 1.0 }),
            e if OTHER_AUDIO.contains(&e) => bail!("{file}: the engine plays .wav recordings (and sounds made of numbers, .toml); make it a .wav"),
            _ => continue,
        };
        if let Some(other) = sounds.get(stem) {
            bail!("{} and {file}: two sounds called \"{stem}\"; keep one", other.file);
        }
        sounds.insert(stem.to_string(), GameSound { file, def });
    }
    Ok(sounds)
}

/// Whether `path` is a file of the game's sounds folder (`sounds/x.toml`
/// in `dir`), new, changed or gone.
pub fn in_folder(dir: &Path, path: &Path) -> bool {
    let folder = dir.join(FOLDER);
    path.parent().is_some_and(|p| p == folder || (p.canonicalize().ok().is_some() && p.canonicalize().ok() == folder.canonicalize().ok()))
}

/// The sounds of a game: those its code brings (a kit's, the engine's:
/// `Game::sounds`), each replaced by the game's file of that name, and the
/// game's others after them.
pub fn all(brought: &[(&'static str, SoundDef)], own: &BTreeMap<String, GameSound>) -> Vec<(&'static str, SoundDef, Option<String>)> {
    let mut list: Vec<(&'static str, SoundDef, Option<String>)> = brought
        .iter()
        .map(|(name, def)| match own.get(*name) {
            Some(g) => (*name, g.def.clone(), Some(g.file.clone())),
            None => (*name, def.clone(), None),
        })
        .collect();
    for (name, g) in own {
        if !brought.iter().any(|(n, _)| n == name) {
            list.push((crate::names::intern(name), g.def.clone(), Some(g.file.clone())));
        }
    }
    list
}

/// A sound as `c.sounds` wrote it in `conf.lua` (a `.wav` file, "none", a
/// recording table, or synth numbers with the old words), in the file's
/// words: end frequencies and jump factors in semitones, as exact as the
/// numbers it had, written as short as they stay exact.
pub fn from_conf(name: &str, value: &toml::Value) -> Result<(SoundDef, Vec<String>)> {
    let mut notes = Vec::new();
    let table = match value {
        toml::Value::String(s) if s == "none" => return Ok((SoundDef::Silent, notes)),
        toml::Value::String(s) => return Ok((SoundDef::Recording(Recording { file: s.clone(), rate: None, bits: None, volume: 1.0 }), notes)),
        toml::Value::Table(t) => t,
        other => bail!("c.sounds.{name} = {other}: a .wav file, \"none\" or a table"),
    };
    let n = |key: &str| -> Result<Option<f32>> {
        match table.get(key) {
            None => Ok(None),
            Some(v) => number(v).map(|n| Some(n as f32)).ok_or_else(|| anyhow!("c.sounds.{name}.{key}: not a number")),
        }
    };
    if let Some(file) = table.get("file") {
        if let Some(key) = table.keys().find(|k| !["file", "rate", "bits", "volume"].contains(&k.as_str())) {
            bail!("c.sounds.{name}.{key}: a recording (file = ...) takes only file, rate, bits and volume");
        }
        let file = file.as_str().ok_or_else(|| anyhow!("c.sounds.{name}.file: a file's name in quotes"))?.to_string();
        // As the engine read them: a rate above 0 and under 44100, bits 1 to 15.
        let rate = n("rate")?.map(|r| r as u32).filter(|r| *r > 0 && *r < synth::SAMPLE_RATE);
        let bits = n("bits")?.map(|b| b as u32).filter(|b| (1..16).contains(b));
        return Ok((SoundDef::Recording(Recording { file, rate, bits, volume: n("volume")?.unwrap_or(1.0) }), notes));
    }
    const OLD: [&str; 15] = ["wave", "freq", "freq_end", "duty", "attack", "sustain", "decay", "punch", "vibrato", "vibrato_speed", "arp", "arp_at", "lowpass", "lowpass_end", "volume"];
    if let Some(key) = table.keys().find(|k| !OLD.contains(&k.as_str())) {
        bail!("c.sounds.{name}.{key}: no such field");
    }
    let d = Synth::default();
    let wave = match table.get("wave") {
        None => d.wave,
        Some(toml::Value::String(w)) => Wave::ALL.iter().find(|(n, _)| n == w).map(|(_, wave)| *wave).ok_or_else(|| anyhow!("c.sounds.{name}.wave: \"{w}\" is no wave ({})", waves()))?,
        Some(_) => bail!("c.sounds.{name}.wave: a word in quotes"),
    };
    let freq = n("freq")?.unwrap_or(d.freq);
    let mut sweep = 0.0;
    if let Some(end) = n("freq_end")? {
        if end > 0.0 && freq > 0.0 {
            sweep = exactly(synth::semitones(freq as f64, end as f64), |s| synth::shifted(freq, s), end);
        } else {
            notes.push(format!("freq_end = {end} left out: the engine passed it over (it glides only between pitches above 0)"));
        }
    }
    let mut jump = 0.0;
    if let Some(arp) = n("arp")?.filter(|a| *a != 1.0) {
        if arp <= 0.0 {
            bail!("c.sounds.{name}.arp = {arp}: a jump to no pitch; write the sound's file by hand");
        }
        jump = exactly(synth::semitones(1.0, arp as f64), synth::ratio, arp);
    }
    let mut lowpass_end = n("lowpass_end")?;
    let lowpass = n("lowpass")?;
    if lowpass.is_none() && lowpass_end.take().is_some() {
        notes.push("lowpass_end left out: without lowpass the engine passed it over".into());
    }
    let s = Synth {
        wave,
        freq,
        sweep,
        duty: n("duty")?.unwrap_or(d.duty),
        attack: n("attack")?.unwrap_or(d.attack),
        hold: n("sustain")?.unwrap_or(d.hold),
        decay: n("decay")?.unwrap_or(d.decay),
        punch: n("punch")?.unwrap_or(d.punch),
        vibrato: n("vibrato")?.unwrap_or(d.vibrato),
        vibrato_speed: n("vibrato_speed")?.unwrap_or(d.vibrato_speed),
        jump,
        jump_at: n("arp_at")?.unwrap_or(d.jump_at),
        lowpass,
        lowpass_end,
        volume: n("volume")?.unwrap_or(d.volume),
    };
    Ok((SoundDef::Synth(s), notes))
}

/// `exact` written with as few decimals as give back `want` through
/// `back` exactly (the f32 the engine had), so a converted sound plays
/// sample for sample as before.
fn exactly(exact: f64, back: impl Fn(f64) -> f32, want: f32) -> f64 {
    for places in 0..=15 {
        let v: f64 = format!("{exact:.places$}").parse().expect("a number written is read back");
        if back(v) == want {
            return v;
        }
    }
    exact
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> String {
        format!("{:#}", read("sounds/coin.toml", text).unwrap_err())
    }

    /// A file names only what differs; the rest is the defaults.
    #[test]
    fn a_sound_file_says_what_differs() {
        let SoundDef::Synth(s) = read("sounds/coin.toml", "# a coin\nwave = \"triangle\"\nfreq = 880\njump = 7\njump_at = 0.05\n").unwrap() else { panic!("a synth") };
        assert_eq!(s, Synth { wave: Wave::Triangle, freq: 880.0, jump: 7.0, jump_at: 0.05, ..Synth::default() });
        assert_eq!(read("sounds/beep.toml", "").unwrap(), SoundDef::Synth(Synth::default()), "an empty file is the default beep");
        assert_eq!(read("sounds/beep.toml", "wave = \"pulse\"").unwrap(), SoundDef::Synth(Synth::default()), "pulse is square, as in MML");
    }

    /// Each kind of mistake says the file, the line, the word and what is wrong.
    #[test]
    fn mistakes_name_the_file_the_line_and_the_word() {
        assert_eq!(err("wave = \"square\"\n\njump = \"up\"\n"), "sounds/coin.toml line 3: jump: not a number");
        assert_eq!(err("wave = \"sqare\"\n"), "sounds/coin.toml line 1: wave: \"sqare\" is no wave (square, saw, triangle, sine, noise)");
        assert_eq!(err("freq = 440\nfreq_en = 200\n"), "sounds/coin.toml line 2: freq_en: no such field (did you mean freq_end? a sound says sweep)");
        assert_eq!(err("frq = 440\n"), "sounds/coin.toml line 1: frq: no such field (did you mean freq?)");
        assert!(err("loudness = 1\n").starts_with("sounds/coin.toml line 1: loudness: no such field (a sound's words: wave, freq, sweep,"), "{}", err("loudness = 1"));
        assert_eq!(err("freq_end = 200\n"), "sounds/coin.toml line 1: freq_end: a sound says sweep (semitones the pitch slides, -12 an octave down; freq_end was the end pitch in Hz)");
        assert_eq!(err("sustain = 0.2\n"), "sounds/coin.toml line 1: sustain: a sound says hold (seconds at full volume; sustain is a level in MML instruments)");
        assert!(err("release = 0.2\n").starts_with("sounds/coin.toml line 1: release: a word of MML instruments, not of sounds"));
        assert_eq!(err("wave = \"saw\"\ndecay = 40\n"), "sounds/coin.toml line 2: decay = 40: from 0 to 10 seconds (fading from full volume to silence at the end, 0.005 at least)");
        assert_eq!(err("volume = 3\n"), "sounds/coin.toml line 1: volume = 3: from 0 to 1 (how loud, 1 full scale)");
        assert_eq!(err("freq = 0\n"), "sounds/coin.toml line 1: freq = 0: from 20 to 20000 Hz (the pitch at the start; for noise how fast it changes, higher is brighter)");
        assert_eq!(err("hold = 6\ndecay = 6\n"), "sounds/coin.toml line 2: decay: attack, hold and decay last 12.00 seconds together; a sound lasts 10 seconds at most (longer is music: c.audio.music)");
        assert_eq!(err("freq = 440\nsweep = -100\n"), "sounds/coin.toml line 2: sweep = -100: slides the pitch from 440 Hz to 1 Hz; it ends from 20 to 20000 Hz");
        assert_eq!(err("jump = 5\njump_at = 2\n"), "sounds/coin.toml line 2: jump_at = 2: after the sound's end (0.30 seconds), so it never jumps");
        assert_eq!(err("lowpass_end = 300\n"), "sounds/coin.toml line 1: lowpass_end: needs lowpass, where the filter starts");
        assert_eq!(err("freq = 440\nfreq = 220\n").lines().next().unwrap(), "sounds/coin.toml line 2: duplicate key");
        assert!(err("wave = square\n").starts_with("sounds/coin.toml line 1: "), "{}", err("wave = square\n"));
        assert_eq!(err("file = \"a.wav\"\nwave = \"saw\"\n"), "sounds/coin.toml line 2: wave: a recording (file = ...) takes only file, rate, bits and volume");
        assert_eq!(err("silent = true\nvolume = 0.5\n"), "sounds/coin.toml line 2: volume: a silent sound says nothing else");
        assert_eq!(err("bits = 8\n"), "sounds/coin.toml line 1: bits: bits belongs to a recording, with file = \"...\"");
        assert_eq!(err("file = \"a.wav\"\nbits = 7.5\n"), "sounds/coin.toml line 2: bits = 7.5: a whole number from 1 to 16 bits");
        assert_eq!(err("file = \"a.mp3\"\n"), "sounds/coin.toml line 1: file = \"a.mp3\": a .wav file, its path from the game folder");
    }

    /// A recording names its file and how lo-fi; silent is nothing at all.
    #[test]
    fn a_recording_and_silence() {
        let def = read("sounds/scream.toml", "file = \"recordings/scream.wav\"\nrate = 7000\nbits = 8\n").unwrap();
        assert_eq!(def, SoundDef::Recording(Recording { file: "recordings/scream.wav".into(), rate: Some(7000), bits: Some(8), volume: 1.0 }));
        assert_eq!(read("sounds/rotor.toml", "silent = true\n").unwrap(), SoundDef::Silent);
        assert_eq!(read("sounds/rotor.toml", "silent = false\nfreq = 300\n").unwrap(), SoundDef::Synth(Synth { freq: 300.0, ..Synth::default() }));
        for def in [def, SoundDef::Silent] {
            assert_eq!(read("x.toml", &write(&def, &[])).unwrap(), def, "written as read");
        }
    }

    /// Semitones and the synth's Hz and ratios: an end frequency and a
    /// jump factor `c.sounds` had come back exactly from the semitones
    /// written, and a file reads back as written.
    #[test]
    fn semitones_give_back_the_pitches_exactly() {
        for (freq, end) in [(1400.0f32, 500.0f32), (900.0, 120.0), (9000.0, 1500.0), (220.0, 520.0), (6000.0, 12000.0), (330.0, 55.0), (2400.0, 900.0)] {
            let table: toml::Value = toml::from_str(&format!("wave = \"square\"\nfreq = {freq}\nfreq_end = {end}")).unwrap();
            let (SoundDef::Synth(s), _) = from_conf("x", &table).unwrap() else { panic!() };
            assert_eq!(s.freq_end(), end, "{freq} to {end}: sweep = {}", s.sweep);
            assert!((s.sweep - synth::semitones(freq as f64, end as f64)).abs() < 1e-3, "near the exact semitones");
            let back = read("x.toml", &write(&SoundDef::Synth(s.clone()), &[])).unwrap();
            assert_eq!(back, SoundDef::Synth(s), "read as written");
        }
        for arp in [1.26f32, 1.5, 1.335, 1.498, 2.0, 1.33] {
            let table: toml::Value = toml::from_str(&format!("arp = {arp}\narp_at = 0.05")).unwrap();
            let (SoundDef::Synth(s), _) = from_conf("x", &table).unwrap() else { panic!() };
            assert_eq!(synth::ratio(s.jump), arp, "jump = {}", s.jump);
        }
        let table: toml::Value = toml::from_str("arp = 2.0").unwrap();
        let (SoundDef::Synth(s), _) = from_conf("x", &table).unwrap() else { panic!() };
        assert_eq!(s.jump, 12.0, "an octave is 12");
    }

    /// Two files of one name are a mistake naming both; a game's sound
    /// files are read through its files (sealed ones too).
    #[test]
    fn the_folder_holds_the_games_sounds() {
        let dir = std::env::temp_dir().join(format!("voxcade-sounds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        std::fs::write(dir.join("sounds/coin.toml"), "wave = \"triangle\"\n").unwrap();
        std::fs::write(dir.join("sounds/README.txt"), "notes").unwrap();
        let sounds = read_folder(&crate::files::Files::folder(&dir)).unwrap();
        assert_eq!(sounds.keys().collect::<Vec<_>>(), ["coin"]);
        assert_eq!(sounds["coin"].file, "sounds/coin.toml");
        std::fs::write(dir.join("sounds/coin.wav"), b"RIFF").unwrap();
        assert_eq!(format!("{:#}", read_folder(&crate::files::Files::folder(&dir)).unwrap_err()), "sounds/coin.toml and sounds/coin.wav: two sounds called \"coin\"; keep one");
        std::fs::remove_file(dir.join("sounds/coin.wav")).unwrap();
        std::fs::write(dir.join("sounds/scream.toml"), "file = \"recordings/scream.wav\"\n").unwrap();
        assert_eq!(format!("{:#}", read_folder(&crate::files::Files::folder(&dir)).unwrap_err()), "sounds/scream.toml line 1: file = \"recordings/scream.wav\": no such file in the game");
        std::fs::remove_file(dir.join("sounds/scream.toml")).unwrap();
        std::fs::write(dir.join("sounds/boom.mp3"), b"ID3").unwrap();
        assert!(format!("{:#}", read_folder(&crate::files::Files::folder(&dir)).unwrap_err()).starts_with("sounds/boom.mp3: the engine plays .wav recordings"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

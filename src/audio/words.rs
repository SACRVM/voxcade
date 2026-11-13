//! The words of sound, in one register: what each word a sound file
//! (`sounds/NAME.toml`, `audio::sound`) or an MML instrument (`@lead square
//! duty=0.5 ...`, `audio::music`) knows means, in which unit, the values it
//! takes and what it is unless written. One vocabulary for both: a word
//! both know means the same in both, in the same unit (test
//! `a_word_means_the_same_in_a_sound_and_an_instrument`), so a maker who
//! learnt one has learnt the other. The sound reader's messages come from
//! here, and the editors' help will.

/// What a word's value is counted in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Seconds,
    Hz,
    /// Steps of the scale: 12 an octave, -12 an octave down.
    Semitones,
    /// A part of a whole, 0 to 1 (a level, a depth, a volume).
    Share,
    /// One of a few words (`wave = "square"`).
    Choice,
    /// true or false.
    YesNo,
    /// Semitone steps as a list, `0,4,7`.
    SemitoneList,
}

impl Unit {
    /// The unit as a word after a number ("seconds", "Hz"), or nothing.
    pub fn after_number(self) -> &'static str {
        match self {
            Unit::Seconds => " seconds",
            Unit::Hz => " Hz",
            Unit::Semitones => " semitones",
            _ => "",
        }
    }
}

/// Which form of sound a word belongs to: synthesized from numbers, or a
/// file that says only this (`silent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Synth,
    Alone,
}

/// What a word means in one of the two readers.
#[derive(Clone, Copy, Debug)]
pub struct Use {
    pub unit: Unit,
    /// Plain English, for people who do not know the engine.
    pub meaning: &'static str,
    /// The values it takes (numbers), lowest and highest; a value outside
    /// is refused, in a sound file and in an MML instrument alike.
    pub range: Option<(f64, f64)>,
    /// What it is when not written, as it would be written.
    pub default: &'static str,
    pub form: Form,
}

pub struct Word {
    pub name: &'static str,
    /// In a sound file, if a word there.
    pub sound: Option<Use>,
    /// In an MML instrument, if a word there.
    pub mml: Option<Use>,
}

const fn sound(unit: Unit, range: Option<(f64, f64)>, default: &'static str, form: Form, meaning: &'static str) -> Option<Use> {
    Some(Use { unit, meaning, range, default, form })
}

const fn mml(unit: Unit, range: Option<(f64, f64)>, default: &'static str, meaning: &'static str) -> Option<Use> {
    Some(Use { unit, meaning, range, default, form: Form::Synth })
}

/// Pitches a sound starts, ends and filters at: the range of hearing.
pub const PITCHES: (f64, f64) = (20.0, 20_000.0);
/// Times within a sound: up to the longest sound (`synth::LONGEST`).
const TIMES: Option<(f64, f64)> = Some((0.0, super::synth::LONGEST as f64));

/// Times within a song's instrument, the same ceiling.
const SONG_TIMES: Option<(f64, f64)> = TIMES;

use Form::*;
use Unit::*;

/// Every word, a sound's in the order a sound file is written.
pub const WORDS: &[Word] = &[
    Word {
        name: "wave",
        sound: sound(Choice, None, "square", Synth, "the oscillator's wave: square, saw, triangle, sine or noise; pulse is square"),
        mml: mml(Choice, None, "square", "the same waves, written as a bare word: @lead square ..."),
    },
    Word {
        name: "freq",
        sound: sound(Hz, Some(PITCHES), "440", Synth, "the pitch at the start; for noise how fast it changes, higher is brighter"),
        mml: mml(Hz, Some(PITCHES), "the note's", "a pitch the instrument always plays, whatever the note: drums"),
    },
    Word {
        name: "sweep",
        sound: sound(Semitones, Some((-120.0, 120.0)), "0", Synth, "how far the pitch slides over the whole sound, evenly: -12 an octave down"),
        mml: mml(Semitones, Some((-120.0, 120.0)), "0", "how far each note's pitch slides, over sweep_time"),
    },
    Word {
        name: "duty",
        sound: sound(Share, Some((0.01, 0.99)), "0.5", Synth, "how much of a square wave's turn is high: 0.5 hollow, less thin and nasal"),
        mml: mml(Share, Some((0.05, 0.95)), "0.5", "the same, from 0.05 to 0.95"),
    },
    Word {
        name: "attack",
        sound: sound(Seconds, TIMES, "0", Synth, "fading in from silence to full volume"),
        mml: mml(Seconds, SONG_TIMES, "0.004", "from silence to the peak at a note's start"),
    },
    Word {
        name: "hold",
        sound: sound(Seconds, TIMES, "0.1", Synth, "time at full volume after the attack"),
        mml: None,
    },
    Word {
        name: "decay",
        sound: sound(Seconds, TIMES, "0.2", Synth, "fading from full volume to silence at the end, 0.005 at least"),
        mml: mml(Seconds, SONG_TIMES, "0.1", "from the peak down to the sustain level, where the note stays until released (in a sound file decay is the fade to silence)"),
    },
    Word {
        name: "punch",
        sound: sound(Share, Some((0.0, 1.0)), "0", Synth, "extra loudness at the start of the hold, falling away"),
        mml: None,
    },
    Word {
        name: "vibrato",
        sound: sound(Share, Some((0.0, 1.0)), "0", Synth, "how far the pitch wobbles, a share of it; 0.01 is gentle"),
        mml: mml(Share, Some((0.0, 1.0)), "0", "the same wobble on every note"),
    },
    Word {
        name: "vibrato_speed",
        sound: sound(Hz, Some((0.0, 50.0)), "6", Synth, "how often the pitch wobbles in a second"),
        mml: mml(Hz, Some((0.0, 50.0)), "6", "the same"),
    },
    Word {
        name: "jump",
        sound: sound(Semitones, Some((-48.0, 48.0)), "0", Synth, "the pitch jumps by this many semitones once, at jump_at; a coin: 5, a fourth"),
        mml: None,
    },
    Word {
        name: "jump_at",
        sound: sound(Seconds, TIMES, "0", Synth, "when the pitch jumps, from the sound's start"),
        mml: None,
    },
    Word {
        name: "lowpass",
        sound: sound(Hz, Some(PITCHES), "none", Synth, "a gentle filter: above this pitch it grows quieter, for rumble and muffling"),
        mml: mml(Hz, Some(PITCHES), "none", "the same gentle filter, fixed"),
    },
    Word {
        name: "lowpass_end",
        sound: sound(Hz, Some(PITCHES), "lowpass", Synth, "where the filter has slid to at the end, from lowpass"),
        mml: None,
    },
    Word {
        name: "volume",
        sound: sound(Share, Some((0.0, 1.0)), "0.5", Synth, "how loud, 1 full scale"),
        mml: mml(Share, Some((0.0, 1.0)), "0.5", "how loud the instrument plays"),
    },
    Word {
        name: "silent",
        sound: sound(YesNo, None, "false", Alone, "true: no sound at all, silencing one the engine or the ready-made rules bring"),
        mml: None,
    },
    // MML's own.
    Word { name: "sustain", sound: None, mml: mml(Share, Some((0.0, 1.0)), "0.7", "the level a held note stays at after the decay") },
    Word { name: "release", sound: None, mml: mml(Seconds, SONG_TIMES, "0.08", "fading to silence after the note is let go") },
    Word { name: "vibrato_delay", sound: None, mml: mml(Seconds, SONG_TIMES, "0", "how long a note plays before it wobbles") },
    Word { name: "arp", sound: None, mml: mml(SemitoneList, None, "none", "semitone steps a note cycles through, fast; 0,4,7 a major chord") },
    Word { name: "arp_speed", sound: None, mml: mml(Hz, Some((0.0, 1000.0)), "50", "steps of arp in a second") },
    Word { name: "sweep_time", sound: None, mml: mml(Seconds, SONG_TIMES, "attack + decay", "how long the sweep takes") },
    Word { name: "pwm", sound: None, mml: mml(Share, Some((0.0, 0.5)), "0", "how far the duty wobbles") },
    Word { name: "pwm_speed", sound: None, mml: mml(Hz, Some((0.0, 50.0)), "1", "how often the duty wobbles") },
    Word { name: "filter", sound: None, mml: mml(Choice, None, "none", "the SID's filter: lowpass, bandpass or highpass") },
    Word { name: "cutoff", sound: None, mml: mml(Hz, Some((20.0, 20_000.0)), "1000", "where the SID filter cuts") },
    Word { name: "cutoff_end", sound: None, mml: mml(Hz, Some((20.0, 20_000.0)), "none", "where the cutoff slides to on every note") },
    Word { name: "cutoff_time", sound: None, mml: mml(Seconds, SONG_TIMES, "attack + decay", "how long the cutoff slides") },
    Word { name: "resonance", sound: None, mml: mml(Share, Some((0.0, 1.0)), "0", "how much the filter rings at its cutoff") },
    Word { name: "sidechain", sound: None, mml: mml(Share, Some((0.0, 1.0)), "0", "how far this instrument's notes duck the other channels") },
    Word { name: "sidechain_release", sound: None, mml: mml(Seconds, SONG_TIMES, "0.2", "how long the others take to come back") },
    Word { name: "duck", sound: None, mml: mml(Share, Some((0.0, 1.0)), "1", "how far this instrument follows the ducking") },
];

/// Words both readers know whose unit differs, each with why it may.
/// None since `sustain` (a sound's time at full volume, MML's level) became
/// the sound's `hold`, `arp`/`arp_at` (a factor, MML's cycling list) the
/// sound's `jump`/`jump_at` in semitones, and `freq_end` (Hz) the sound's
/// `sweep` in semitones.
pub const DIFFERENT: &[(&str, &str)] = &[];

/// Names a sound once had (in `c.sounds`), and what says it now.
pub const RENAMED: &[(&str, &str)] = &[
    ("sustain", "hold (seconds at full volume; sustain is a level in MML instruments)"),
    ("freq_end", "sweep (semitones the pitch slides, -12 an octave down; freq_end was the end pitch in Hz)"),
    ("arp", "jump (semitones the pitch jumps, 5 a fourth; arp was a factor)"),
    ("arp_at", "jump_at"),
];

pub fn word(name: &str) -> Option<&'static Word> {
    WORDS.iter().find(|w| w.name == name)
}

/// The words of a sound file.
pub fn sound_words() -> impl Iterator<Item = (&'static str, &'static Use)> {
    WORDS.iter().filter_map(|w| Some((w.name, w.sound.as_ref()?)))
}

/// The word meant, when `name` is one or two letters off a word of
/// `among`.
pub fn near<'a>(name: &str, among: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    among.map(|w| (distance(name, w), w)).filter(|(d, _)| *d <= 2).min_by_key(|(d, _)| *d).map(|(_, w)| w)
}

/// Letters to change, add or take away to make one word the other.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (last + (ca != *cb) as usize).min(row[j] + 1).min(here + 1);
            last = here;
        }
    }
    row[b.len()]
}

/// "from 0 to 10 seconds": a word's range in words.
pub fn range_words(u: &Use) -> String {
    match u.range {
        Some((lo, hi)) => format!("from {lo} to {hi}{}", u.unit.after_number()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One vocabulary: a word a sound file and an MML instrument both know
    /// is counted in the same unit in both, or stands in `DIFFERENT` with
    /// its reason; and the register holds exactly the words each reader
    /// takes.
    #[test]
    fn a_word_means_the_same_in_a_sound_and_an_instrument() {
        for w in WORDS {
            if let (Some(s), Some(m)) = (&w.sound, &w.mml) {
                assert!(s.unit == m.unit || DIFFERENT.iter().any(|(n, _)| *n == w.name), "{}: {:?} in a sound, {:?} in MML", w.name, s.unit, m.unit);
            }
            assert!(w.sound.is_some() || w.mml.is_some(), "{} is a word of neither", w.name);
        }
        let mut names: Vec<&str> = WORDS.iter().map(|w| w.name).collect();
        names.sort();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "a word twice in the register");
        // The words MML's reader takes: its options' arms, the waves as
        // the word `wave`.
        let source = include_str!("music.rs");
        let body = &source[source.find("fn parse_instrument").unwrap()..source.find("fn expand").unwrap()];
        let mut taken: Vec<&str> = Vec::new();
        for line in body.lines().filter(|l| l.starts_with("            \"")) {
            for word in line.split("=>").next().unwrap().split('|') {
                let word = word.trim().trim_matches('"');
                taken.push(if crate::audio::Wave::ALL.iter().any(|(n, _)| *n == word) || word == "pulse" { "wave" } else { word });
            }
        }
        taken.sort();
        taken.dedup();
        let in_register: Vec<&str> = {
            let mut v: Vec<&str> = WORDS.iter().filter(|w| w.mml.is_some()).map(|w| w.name).collect();
            v.sort();
            v
        };
        assert_eq!(taken, in_register, "MML's options and the register's MML words");
        // Every word the register gives MML, MML takes.
        for w in WORDS.iter().filter(|w| w.mml.is_some() && w.name != "wave") {
            let value = match w.name {
                "arp" => "0,4,7".to_string(),
                "filter" => "lowpass".to_string(),
                // A value inside the word's range (MML refuses the rest).
                _ => match w.mml.as_ref().and_then(|u| u.range) {
                    Some((lo, hi)) if lo > 1.0 || hi < 1.0 => ((lo + hi) / 2.0).to_string(),
                    _ => "1".to_string(),
                },
            };
            crate::audio::music::parse(&format!("@x {}={value}\nA: c", w.name)).unwrap_or_else(|e| panic!("{}: {e:#}", w.name));
        }
        // Renamed words are no words of a sound any more.
        for (old, _) in RENAMED {
            assert!(word(old).is_none_or(|w| w.sound.is_none()), "{old} is still a sound's word");
        }
    }

    /// A slip of a letter or two finds the word meant.
    #[test]
    fn a_near_word_is_found() {
        let words = || sound_words().map(|(n, _)| n);
        assert_eq!(near("frq", words()), Some("freq"));
        assert_eq!(near("hlod", words()), Some("hold"));
        assert_eq!(near("wav", words()), Some("wave"));
        assert_eq!(near("vibratospeed", words()), Some("vibrato_speed"));
        assert_eq!(near("loudness", words()), None);
    }
}

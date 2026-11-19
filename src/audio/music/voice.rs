//! The singing voice (`@name voice`): a formant synthesizer after Klatt
//! (1980), without samples. An impulse train (the vocal cords) through a
//! glottal low-pass and five resonators in a row (three set per vowel and
//! gliding, two fixed), noise through a band of its own (twice) for the
//! hissing consonants, closure, burst and breath for the plosives, nasals
//! and liquids as weak voiced formants, the lips' radiation last. Its
//! noise (breath and hiss) comes out apart from its voicing, so an
//! instrument's `drive` clips the voice alone and the consonants keep
//! their distance from the vowels.
//!
//! A channel sings syllables: a quoted text before a note is what that note
//! sings, its phonemes split by spaces (`"s ae ng" a4`): onset consonants, one
//! vowel or diphthong, coda consonants. The vowel starts on the note's beat,
//! the onset sounds before it, the coda ends where the note is let go (`q`).
//! A voice note without a text holds the vowel sung last (a melisma): the
//! notes tied so share one vowel, the coda and a diphthong's glide on the
//! last of them. The parser lays every note out as segments (`lay_out`); the
//! player's `Singer` reads them sample by sample, taking no memory.

use anyhow::{Result, bail};

use super::SAMPLE_RATE;

/// Formants F1, F2, F3 in Hz.
type Formants = [f32; 3];

/// What a phoneme is to the synthesizer.
#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    /// A steady vowel.
    Vowel(Formants),
    /// From one vowel to another: the first quarter the first, the middle
    /// half gliding, the last quarter the second.
    Diphthong(Formants, Formants),
    /// A nasal, liquid or glide: weak voicing through its own formants.
    Sonorant { formants: Formants, voicing: f32, length: f32 },
    /// Noise through a band (`noise` its middle, `width` its width), its
    /// level, the formants meanwhile, and voicing under it (v, z).
    Fricative { length: f32, noise: f32, width: f32, level: f32, formants: Formants, voicing: f32 },
    /// A closure (voiced or silent), a burst of noise, breath after it, and
    /// the formants of where the tongue or lips close (the locus).
    Plosive { closure: f32, voiced: bool, burst: f32, noise: f32, width: f32, level: f32, breath: f32, locus: Formants },
    /// Breath through the formants of the syllable's vowel (h).
    Breath { length: f32, level: f32 },
}

#[derive(Debug)]
pub(super) struct Phoneme {
    pub(super) name: &'static str,
    pub(super) kind: Kind,
}

const fn vowel(name: &'static str, f: Formants) -> Phoneme {
    Phoneme { name, kind: Kind::Vowel(f) }
}

const fn diphthong(name: &'static str, from: Formants, to: Formants) -> Phoneme {
    Phoneme { name, kind: Kind::Diphthong(from, to) }
}

const fn sonorant(name: &'static str, formants: Formants, voicing: f32, length: f32) -> Phoneme {
    Phoneme { name, kind: Kind::Sonorant { formants, voicing, length } }
}

/// A fricative: (length s, noise Hz, its width Hz, level), formants, voicing.
const fn fricative(name: &'static str, (length, noise, width, level): (f32, f32, f32, f32), formants: Formants, voicing: f32) -> Phoneme {
    Phoneme { name, kind: Kind::Fricative { length, noise, width, level, formants, voicing } }
}

/// A plosive: (closure s, voiced, burst s), (burst noise Hz, its width Hz,
/// level), breath s, locus.
const fn plosive(name: &'static str, (closure, voiced, burst): (f32, bool, f32), (noise, width, level): (f32, f32, f32), breath: f32, locus: Formants) -> Phoneme {
    Phoneme { name, kind: Kind::Plosive { closure, voiced, burst, noise, width, level, breath, locus } }
}

// The vowels: men's formants after Peterson & Barney (1952) and
// Hillenbrand et al. (1995), German ö and ü after typical German values.
const AE: Formants = [660.0, 1720.0, 2410.0];
const EH: Formants = [530.0, 1840.0, 2480.0];
const A: Formants = [730.0, 1090.0, 2440.0];
const O: Formants = [500.0, 900.0, 2400.0];
const U: Formants = [350.0, 900.0, 2300.0];
const E: Formants = [400.0, 2000.0, 2600.0];
const AX: Formants = [600.0, 1300.0, 2400.0];
const I: Formants = [300.0, 2300.0, 3000.0];
const IH: Formants = [430.0, 2030.0, 2680.0];
const OE: Formants = [380.0, 1500.0, 2200.0];
const UE: Formants = [280.0, 1700.0, 2100.0];

/// Every phoneme a text may name, vowels first. The consonants lie under
/// the vowel after them as in speech (relative phonetic powers after
/// Sacia & Beck, tabled by Fletcher, 1953): sh about 13 dB, s, t, k and g
/// about 15 to 16, ch and d 17, f, h and p 19 to 20; their loudest 10 ms
/// against the vowel's (test `consonants_lie_under_the_vowels`). The noise
/// bands: s 5 to 8 kHz, sh 2 to 4, ch 2 to 5.7.
pub(super) const PHONEMES: &[Phoneme] = &[
    vowel("a", A), // father, German Vater
    vowel("ae", AE), // cat
    vowel("e", E), // German Weg, a closed e
    vowel("eh", EH), // bed
    vowel("i", I), // see, German Lied
    vowel("ih", IH), // bit
    vowel("o", O), // German so
    vowel("u", U), // blue, German gut
    vowel("ax", AX), // but, the a in sofa
    vowel("oe", OE), // German schön
    vowel("ue", UE), // German über
    diphthong("aw", A, U), // now, German Haus
    diphthong("ow", O, U), // go
    diphthong("ai", A, IH), // my, German mein
    diphthong("oi", O, IH), // boy, German neu
    diphthong("ei", EH, I), // day
    sonorant("m", [250.0, 1100.0, 2300.0], 0.45, 0.07), // me
    sonorant("n", [250.0, 1400.0, 2500.0], 0.45, 0.07), // no
    sonorant("ng", [250.0, 1900.0, 2300.0], 0.45, 0.07), // sing
    sonorant("l", [350.0, 1100.0, 2700.0], 0.6, 0.06), // low
    sonorant("r", [350.0, 1100.0, 1500.0], 0.7, 0.06), // red
    sonorant("w", [300.0, 700.0, 2200.0], 0.7, 0.06), // we
    sonorant("j", [280.0, 2250.0, 3000.0], 0.7, 0.06), // yes, German ja
    fricative("f", (0.08, 4500.0, 4000.0, 0.085), [300.0, 1300.0, 2400.0], 0.0), // fun
    fricative("v", (0.07, 4500.0, 4000.0, 0.04), [300.0, 1300.0, 2400.0], 0.35), // very
    fricative("s", (0.09, 6500.0, 3000.0, 0.031), [300.0, 1700.0, 2700.0], 0.0), // sun
    fricative("z", (0.08, 6500.0, 3000.0, 0.015), [300.0, 1700.0, 2700.0], 0.35), // zoo, German See
    fricative("sh", (0.09, 3000.0, 1800.0, 0.19), [300.0, 1800.0, 2400.0], 0.0), // she, German schon
    fricative("ch", (0.09, 4000.0, 2500.0, 0.082), [280.0, 2200.0, 2900.0], 0.0), // German ich
    Phoneme { name: "h", kind: Kind::Breath { length: 0.07, level: 0.03 } }, // hat
    plosive("t", (0.045, false, 0.022), (4500.0, 2500.0, 0.088), 0.03, [250.0, 1800.0, 2700.0]), // top
    plosive("k", (0.045, false, 0.028), (1900.0, 900.0, 0.25), 0.025, [250.0, 2000.0, 2200.0]), // cat
    plosive("p", (0.05, false, 0.015), (900.0, 1200.0, 0.7), 0.03, [250.0, 900.0, 2300.0]), // pen
    plosive("d", (0.035, true, 0.012), (4000.0, 2500.0, 0.13), 0.0, [250.0, 1700.0, 2600.0]), // do
    plosive("g", (0.035, true, 0.015), (1800.0, 900.0, 0.28), 0.0, [250.0, 2000.0, 2200.0]), // go
    plosive("b", (0.04, true, 0.01), (900.0, 1200.0, 0.45), 0.0, [250.0, 900.0, 2300.0]), // be
];

fn phoneme(name: &str) -> Option<&'static Phoneme> {
    PHONEMES.iter().find(|p| p.name == name)
}

/// The phonemes by sort, as HELP and a mistake list them: "vowels a ae
/// ..., diphthongs aw ..., consonants m ...".
pub fn phonemes() -> String {
    let of = |vowel: bool, glide: bool| -> String {
        let names = PHONEMES.iter().filter(|p| matches!(p.kind, Kind::Vowel(_)) == vowel && matches!(p.kind, Kind::Diphthong(..)) == glide);
        names.map(|p| p.name).collect::<Vec<_>>().join(" ")
    };
    format!("vowels {}, diphthongs {}, consonants {}", of(true, false), of(false, true), of(false, false))
}

/// What a quoted text sings: onset consonants, a vowel, coda consonants.
#[derive(Clone, Debug)]
pub(super) struct Syllable {
    onset: Vec<&'static Phoneme>,
    vowel: &'static Phoneme,
    coda: Vec<&'static Phoneme>,
}

impl Syllable {
    /// The vowel sung alone (a melisma after a rest).
    fn bare(&self) -> Syllable {
        Syllable { onset: Vec::new(), vowel: self.vowel, coda: Vec::new() }
    }
}

/// A quoted text as a syllable, or the mistake in it.
pub(super) fn syllable(text: &str) -> Result<Syllable> {
    let mut onset = Vec::new();
    let mut vowel = None;
    let mut coda = Vec::new();
    for word in text.split_whitespace() {
        let Some(p) = phoneme(word) else {
            let lower = word.to_lowercase();
            let near = match phoneme(&lower) {
                Some(p) => Some(p.name),
                None => crate::audio::words::near(word, PHONEMES.iter().map(|p| p.name)),
            };
            let hint = near.map_or(String::new(), |n| format!(": did you mean '{n}'?"));
            bail!("unknown phoneme '{word}'{hint} (in small letters: {})", phonemes());
        };
        let is_vowel = matches!(p.kind, Kind::Vowel(_) | Kind::Diphthong(..));
        match (is_vowel, vowel) {
            (true, None) => vowel = Some(p),
            (true, Some(first)) => bail!("\"{text}\" has two vowels, '{}' and '{}': a syllable has one, sing the other on a note of its own", first.name, p.name),
            (false, None) => onset.push(p),
            (false, Some(_)) => coda.push(p),
        }
    }
    let Some(vowel) = vowel else {
        if text.trim().is_empty() {
            bail!("an empty text: write the syllable's phonemes, like \"l a\"");
        }
        bail!("\"{text}\" has no vowel: a syllable needs one ({})", phonemes());
    };
    Ok(Syllable { onset, vowel, coda })
}

/// A stretch of a sung note, in samples from where the note is taken up.
#[derive(Clone, Copy, Debug)]
pub(super) struct Segment {
    pub(super) from: u64,
    pub(super) to: u64,
    formants: Formants,
    /// Gliding to these over the segment (a diphthong).
    glide: Option<Formants>,
    pub(super) voicing: f32,
    pub(super) frication: f32,
    aspiration: f32,
    /// The frication's band: its middle and width in Hz.
    noise: f32,
    width: f32,
}

impl Segment {
    fn new(from: i64, to: i64, formants: Formants) -> Self {
        Self { from: from.max(0) as u64, to: to.max(0) as u64, formants, glide: None, voicing: 0.0, frication: 0.0, aspiration: 0.0, noise: 3000.0, width: 2000.0 }
    }
}

fn samples(seconds: f32) -> i64 {
    (f64::from(seconds) * f64::from(SAMPLE_RATE)).round() as i64
}

/// How long a consonant lasts, in samples.
fn length(p: &Phoneme) -> i64 {
    match p.kind {
        Kind::Vowel(_) | Kind::Diphthong(..) => 0,
        Kind::Sonorant { length, .. } | Kind::Fricative { length, .. } | Kind::Breath { length, .. } => samples(length),
        Kind::Plosive { closure, burst, breath, .. } => samples(closure) + samples(burst) + samples(breath),
    }
}

/// A plan of segments as they are added: a later one wins where it overlaps
/// an earlier (the next consonant cutting into a vowel).
struct Plan<'a> {
    out: &'a mut Vec<Segment>,
    first: usize,
}

impl Plan<'_> {
    fn add(&mut self, s: Segment) {
        if s.to <= s.from {
            return;
        }
        while self.out.len() > self.first && self.out.last().is_some_and(|l| l.from >= s.from) {
            self.out.pop();
        }
        if let Some(l) = self.out[self.first..].last_mut() {
            l.to = l.to.min(s.from);
        }
        self.out.push(s);
    }

    /// The consonant `p` from `at` (samples), the vowel's formants for a
    /// breath; where it ends.
    fn consonant(&mut self, p: &Phoneme, at: i64, vowel: Formants) -> i64 {
        match p.kind {
            Kind::Vowel(_) | Kind::Diphthong(..) => at,
            Kind::Sonorant { formants, voicing, length } => {
                let end = at + samples(length);
                self.add(Segment { voicing, ..Segment::new(at, end, formants) });
                end
            }
            Kind::Fricative { length, noise, width, level, formants, voicing } => {
                let end = at + samples(length);
                self.add(Segment { voicing, frication: level, noise, width, ..Segment::new(at, end, formants) });
                end
            }
            Kind::Breath { length, level } => {
                let end = at + samples(length);
                self.add(Segment { aspiration: level, ..Segment::new(at, end, vowel) });
                end
            }
            Kind::Plosive { closure, voiced, burst, noise, width, level, breath, locus } => {
                let mut t = at;
                let end = t + samples(closure);
                self.add(Segment { voicing: if voiced { 0.12 } else { 0.0 }, ..Segment::new(t, end, locus) });
                t = end;
                let end = t + samples(burst);
                self.add(Segment { frication: level, noise, width, ..Segment::new(t, end, locus) });
                t = end;
                if breath > 0.0 {
                    let end = t + samples(breath);
                    self.add(Segment { aspiration: 0.07, ..Segment::new(t, end, locus) });
                    t = end;
                }
                t
            }
        }
    }
}

/// How a note sings its syllable's vowel.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    /// All of it, a diphthong's glide too.
    Whole,
    /// Its first vowel, held (a melisma's notes before the last).
    Held,
}

/// The segments of one note: `syllable` (its onset, vowel and coda as the
/// note has them), its beat at `beat` and let go at `off` (samples from
/// its take-up).
fn plan(out: &mut Vec<Segment>, s: &Syllable, part: Part, beat: i64, off: i64) {
    let (first, last) = match s.vowel.kind {
        Kind::Vowel(f) => (f, f),
        Kind::Diphthong(a, b) => (a, b),
        _ => unreachable!("a syllable's vowel is a vowel"),
    };
    let mut plan = Plan { first: out.len(), out };
    let mut t = beat - s.onset.iter().map(|p| length(p)).sum::<i64>();
    for p in &s.onset {
        t = plan.consonant(p, t, first);
    }
    let vowel_end = off - s.coda.iter().map(|p| length(p)).sum::<i64>();
    let glide = (part == Part::Whole && first != last).then_some(last);
    plan.add(Segment { voicing: 1.0, glide, ..Segment::new(beat, vowel_end, first) });
    // A coda comes only on a syllable's last note, which sings it whole.
    let mut t = vowel_end;
    for p in &s.coda {
        t = plan.consonant(p, t, last);
    }
}

/// A voice note as the parser found it: its index among the channel's
/// notes, its text (none: it holds the vowel before), and where it lies.
#[derive(Debug)]
pub(super) struct Sing {
    pub(super) note: usize,
    pub(super) text: Option<Syllable>,
    /// Samples from the song's start: its beat and its end (the next
    /// note's start when no rest follows).
    pub(super) at: u64,
    pub(super) end: u64,
}

/// Lays a channel's voice notes out: each a range of segments in
/// `segments`, its onset moved before its beat (never before the note
/// before it), a melisma's notes tied, their coda on the last.
pub(super) fn lay_out(notes: &mut [super::song::Note], sings: &[Sing], segments: &mut Vec<Segment>) {
    // Whether the voice note after `a` holds its vowel on, with no rest between.
    let holds_on = |a: &Sing, b: &Sing| b.text.is_none() && b.note == a.note + 1 && b.at == a.end;
    let mut sung: Option<&Syllable> = None;
    for (i, sing) in sings.iter().enumerate() {
        let tied = sings.get(i + 1).is_some_and(|next| holds_on(sing, next));
        let held_on = i > 0 && holds_on(&sings[i - 1], sing);
        if let Some(text) = &sing.text {
            sung = Some(text);
        }
        let Some(text) = sung else { unreachable!("the parser refuses a first voice note without a text") };
        // The onset on a syllable's first note only, the coda on the last
        // note of its melisma; a note after a rest sings the vowel alone.
        let mut syllable = text.bare();
        if sing.text.is_some() {
            syllable.onset.clone_from(&text.onset);
        }
        if !tied && (sing.text.is_some() || held_on) {
            syllable.coda.clone_from(&text.coda);
        }
        let part = if tied { Part::Held } else { Part::Whole };
        let onset: i64 = syllable.onset.iter().map(|p| length(p)).sum();
        let earliest = if sing.note == 0 { 0 } else { notes[sing.note - 1].start };
        let note = &mut notes[sing.note];
        let beat = note.start;
        let begin = beat.saturating_sub(onset as u64).max(earliest).min(beat);
        // Tied, the note is held up to the next.
        let held = if tied { sing.end - sing.at } else { note.held };
        let from = segments.len();
        plan(segments, &syllable, part, (beat - begin) as i64, (beat - begin + held) as i64);
        note.start = begin;
        note.held = beat - begin + held;
        note.sung = Some((from as u32, segments.len() as u32));
    }
}

/// The voice as it sings, sample by sample: the noise's generator, the
/// coefficients that never change, and what does (`Tract`). Takes no
/// memory.
#[derive(Clone, Debug)]
pub(super) struct Singer {
    seed: u32,
    /// The glottal low-pass's and the two high resonators' coefficients;
    /// each formant's pole for its bandwidth; how much of their last value
    /// the tracks keep a sample.
    glottal: (f64, f64, f64),
    high: [(f64, f64, f64); 2],
    poles: [(f64, f64); 3],
    keep: [f64; 4],
    tract: Tract,
}

/// What changes as the voice sings: the glottal pulse's phase, the
/// resonators' states, the tracks gliding to the segment's targets.
#[derive(Clone, Debug)]
struct Tract {
    phase: f64,
    glottal: (f64, f64),
    /// The resonators' states for the voicing and, apart, for the breath
    /// (the same filters, run twice: a voice's noise does not go through
    /// its clipper, `next`).
    resonators: [(f64, f64); 5],
    breaths: [(f64, f64); 5],
    hiss: [(f64, f64); 2],
    /// The voiced and the noisy part's last sample, for the radiation.
    last: (f64, f64),
    /// The tracks as they glide: formants, voicing, frication, aspiration.
    formants: [f64; 3],
    voicing: f64,
    frication: f64,
    aspiration: f64,
    /// The formants aimed at, held after the last segment.
    aim: Formants,
    /// The segment being sung, and the frication band's coefficients.
    cursor: usize,
    band: (f32, f32, (f64, f64, f64)),
}

impl Default for Tract {
    fn default() -> Self {
        Self {
            phase: 0.0,
            glottal: (0.0, 0.0),
            resonators: [(0.0, 0.0); 5],
            breaths: [(0.0, 0.0); 5],
            hiss: [(0.0, 0.0); 2],
            last: (0.0, 0.0),
            formants: [500.0, 1500.0, 2500.0],
            voicing: 0.0,
            frication: 0.0,
            aspiration: 0.0,
            aim: [500.0, 1500.0, 2500.0],
            cursor: 0,
            band: (0.0, 0.0, (0.0, 0.0, 0.0)),
        }
    }
}

impl Default for Singer {
    fn default() -> Self {
        Self::new(0x2545_f491)
    }
}

const SR: f64 = SAMPLE_RATE as f64;

/// A resonator's pole for a peak `bw` Hz wide: its radius doubled (`b`
/// over the cosine of its frequency) and `c`.
fn pole(bw: f64) -> (f64, f64) {
    (2.0 * (-std::f64::consts::PI * bw / SR).exp(), -(-2.0 * std::f64::consts::PI * bw / SR).exp())
}

/// A resonator's coefficients (Klatt's: unity gain at 0 Hz) for a peak at
/// `f` Hz on the pole `(r2, c)`.
fn coefficients(f: f64, (r2, c): (f64, f64)) -> (f64, f64, f64) {
    let b = r2 * (2.0 * std::f64::consts::PI * f / SR).cos();
    (1.0 - b - c, b, c)
}

/// The formants' bandwidths, then the two fixed resonators above them.
const BANDWIDTHS: [f64; 3] = [70.0, 100.0, 150.0];
const HIGH: [(f64, f64); 2] = [(3500.0, 250.0), (4500.0, 300.0)];

/// The tracks' glide times: formants, voicing, frication, aspiration (ms).
const GLIDES: [f64; 4] = [12.0, 6.0, 2.0, 4.0];

/// What brings a sung line's peaks to about full scale (the resonators'
/// unity gain at 0 Hz leaves a line peaking near 0.012).
const LOUDNESS: f64 = 1.0 / 0.012;

impl Singer {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            seed: seed.max(1),
            glottal: coefficients(0.0, pole(110.0)),
            high: HIGH.map(|(f, bw)| coefficients(f, pole(bw))),
            poles: BANDWIDTHS.map(pole),
            keep: GLIDES.map(|ms| (-1000.0 / (SR * ms)).exp()),
            tract: Tract::default(),
        }
    }

    /// A note taken up: its segments from the start. From silence the
    /// resonators are still and the formants start where the note does.
    pub(super) fn start(&mut self, plan: &[Segment], from_silence: bool) {
        self.tract.cursor = 0;
        if from_silence {
            self.tract = Tract { phase: self.tract.phase, ..Tract::default() };
            if let Some(s) = plan.first() {
                self.tract.aim = s.formants;
                self.tract.formants = s.formants.map(f64::from);
            }
        }
    }

    /// A noise sample, near a normal distribution (four uniform ones).
    fn noise(&mut self) -> f64 {
        let mut sum = 0.0;
        for _ in 0..4 {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 17;
            self.seed ^= self.seed << 5;
            sum += f64::from(self.seed) / f64::from(u32::MAX) * 2.0 - 1.0;
        }
        // Four uniform values of -1..1 sum to a variance of 4/3.
        sum * 0.75f64.sqrt()
    }

    /// The next sample, `t` samples into the note whose segments are
    /// `plan`, sung at `f0` Hz: its voiced part and its noise (breath and
    /// hiss), apart, so a clipper can drive the voice without lifting the
    /// noise with it.
    pub(super) fn next(&mut self, plan: &[Segment], t: u64, f0: f32) -> (f32, f32) {
        let noise = self.noise();
        let v = &mut self.tract;
        while plan.get(v.cursor).is_some_and(|s| t >= s.to) {
            v.cursor += 1;
        }
        let (mut voicing, mut frication, mut aspiration) = (0.0, 0.0, 0.0);
        if let Some(s) = plan.get(v.cursor).filter(|s| t >= s.from) {
            v.aim = match s.glide {
                Some(to) => {
                    let along = (t - s.from) as f32 / (s.to - s.from) as f32;
                    let k = ((along - 0.25) / 0.5).clamp(0.0, 1.0);
                    std::array::from_fn(|r| s.formants[r] + (to[r] - s.formants[r]) * k)
                }
                None => s.formants,
            };
            (voicing, frication, aspiration) = (s.voicing, s.frication, s.aspiration);
            if (s.noise, s.width) != (v.band.0, v.band.1) {
                v.band = (s.noise, s.width, coefficients(f64::from(s.noise), pole(f64::from(s.width))));
            }
        }
        let glide = |now: &mut f64, to: f32, keep: f64| *now = f64::from(to) + (*now - f64::from(to)) * keep;
        for (now, aim) in v.formants.iter_mut().zip(v.aim) {
            glide(now, aim, self.keep[0]);
        }
        glide(&mut v.voicing, voicing, self.keep[1]);
        glide(&mut v.frication, frication, self.keep[2]);
        glide(&mut v.aspiration, aspiration, self.keep[3]);

        // The vocal cords: a pulse a period, through the glottal low-pass.
        v.phase += f64::from(f0.max(0.0)) / SR;
        let pulse = if v.phase >= 1.0 {
            v.phase -= v.phase.floor();
            1.0
        } else {
            0.0
        };
        let (ga, gb, gc) = self.glottal;
        let g = ga * pulse * 60.0 + gb * v.glottal.0 + gc * v.glottal.1;
        v.glottal = (g, v.glottal.0);
        // Voice and breath through the resonators in a row, the formants
        // first, each through its own.
        let mut x = g * v.voicing;
        let mut h = noise * v.aspiration * 0.05;
        for (k, (voiced, breath)) in v.resonators.iter_mut().zip(v.breaths.iter_mut()).enumerate() {
            let (a, b, c) = if k < 3 { coefficients(v.formants[k], self.poles[k]) } else { self.high[k - 3] };
            for (x, state) in [(&mut x, voiced), (&mut h, breath)] {
                let y = a * *x + b * state.0 + c * state.1;
                *state = (y, state.0);
                *x = y;
            }
        }
        // A consonant's hiss beside them, through its own band twice, so
        // it falls off steeply above it, not a hiss over every height.
        let (fa, fb, fc) = v.band.2;
        let mut hiss = noise * v.frication * 0.012;
        for state in &mut v.hiss {
            hiss = fa * hiss + fb * state.0 + fc * state.1;
            *state = (hiss, state.0);
        }
        // The lips' radiation: the difference from the sample before.
        let noisy = h + hiss;
        let out = (x - v.last.0, noisy - v.last.1);
        v.last = (x, noisy);
        ((out.0 * LOUDNESS) as f32, (out.1 * LOUDNESS) as f32)
    }
}

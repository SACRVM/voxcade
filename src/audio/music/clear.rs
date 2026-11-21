//! The clear voice (`@name singer`): the same syllables and phonemes as
//! `voice` (`super::voice`), sung by a cascade/parallel formant synthesizer
//! after Klatt (1980) made for words to be understood.
//!
//! - The source is a glottal flow pulse (KLGLOTT88's polynomial, Klatt &
//!   Klatt 1990), falling about 12 dB an octave above the pitch before the
//!   lips' radiation, not an impulse through a steep low-pass, its flow
//!   smaller as the pitch rises so a vowel keeps its loudness.
//! - Five formants in cascade, the upper two and every bandwidth set per
//!   vowel, so the formants' levels come out as a vocal tract makes them
//!   (Peterson & Barney's 1952 levels are the measure).
//! - Every join glides: the formants move from a consonant's locus into the
//!   vowel and out of it again (F2 onset = k F2vowel + c, the locus
//!   equation's form, Klatt 1979b via Klatt 1987, p. 755; our k and c are
//!   from memory of Sussman et al. 1991, not checked against the paper:
//!   labials low, alveolars towards 1700 Hz, velars with F2 and F3 drawn
//!   together), over a time each class of sound takes (ours); voiceless
//!   plosives breathe before the voicing starts (voice onset time after
//!   Lisker & Abramson 1964), voiced ones a short release growing with a
//!   place further back, a velar's longest, and a voice bar only after a
//!   voiced sound; nasals with a nasal pole and a zero by their place
//!   (Fujimura 1962); r the German one, uvular, a light rasp before a vowel
//!   (its F3 high, so it is not heard as l with an English r's low F3); a
//!   trill (`rr`) taps.
//! - A parallel branch gives fricatives and bursts spectra of their own: s
//!   high, sh lower, the German ich between, ach low, f flat and weak; a
//!   burst its place's noise (a velar's compact at the vowel's F2), voiced
//!   fricatives and closures a low hum.
//! - Clusters: notes sung without a rest share the time between their
//!   vowels (`joins`): a coda ends where the next onset begins, both
//!   compressed down to each consonant's least where they do not fit; a
//!   plosive after s or sh unaspirated.
//! - No step that clicks (Klatt 1980, p. 974, 977, 987): the voicing set
//!   only as a glottal pulse begins, a hiss coming and going over 5 ms but
//!   a burst's onset, the nasal zero gliding with the formants; a voiced
//!   fricative two voices at once (through the formants and as a murmur)
//!   under a hiss 10 dB weaker than its voiceless twin's, beating with the
//!   glottis, a labiodental's hiss through the flat bypass.
//! - What sells a singer (Sundberg 1987): the pitch gliding into a note
//!   from a little below, a vibrato that grows in (the player's), a little
//!   jitter, shimmer and breath.
//!
//! Its noise comes out apart from its voicing, as `voice`'s, so a drive
//! clips the voice alone. Sample by sample in f64, its noise from a
//! generator of its own seeded per channel, taking no memory.

use super::SAMPLE_RATE;
use super::voice::{Kind, Part, Phoneme, Place, Syllable};

/// The wave's word: the clear voice is `@name singer ...`.
pub const WAVE: &str = "singer";

const SR: f64 = SAMPLE_RATE as f64;

/// Formants F1 to F5, or their bandwidths.
type Five = [f32; 5];

/// A stretch of a note the clear voice sings, in samples from where the
/// note is taken up, with what it aims at.
#[derive(Clone, Copy, Debug)]
pub(super) struct Seg {
    /// The phoneme it sings (read by the tests, and in a plan printed).
    #[allow(dead_code)]
    pub(super) name: &'static str,
    pub(super) from: u64,
    pub(super) to: u64,
    /// The formants and bandwidths aimed at, reached over `glide` samples
    /// from where they were (a raised cosine).
    formants: Five,
    widths: Five,
    glide: u32,
    /// A diphthong's second vowel (F1 to F3), reached over the middle half.
    turn: Option<[f32; 3]>,
    pub(super) voicing: f32,
    aspiration: f32,
    /// The parallel branch: two bands (Hz, width, level) and a flat part.
    bands: [(f32, f32, f32); 2],
    flat: f32,
    /// The nasal zero (Hz), 0 for none.
    nasal: f32,
    /// Taps a second breaking the voicing (a trill), 0 for none.
    taps: f32,
    /// How much of the voicing is a murmur, the voice bar of a voiced
    /// fricative or closure: the pulse low-passed, the lips or tongue
    /// narrowing the tract so its formants above F1 barely ring (Klatt's
    /// quasi-sinusoidal voicing). 0 for none.
    murmur: f32,
    /// A plosive's release: its hiss jumps on and falls to half by its end.
    burst: bool,
}

impl Seg {
    fn new(from: i64, to: i64, formants: Five, widths: Five, glide: f32) -> Self {
        Self {
            name: "",
            from: from.max(0) as u64,
            to: to.max(0) as u64,
            formants,
            widths,
            glide: ms(glide) as u32,
            turn: None,
            voicing: 0.0,
            aspiration: 0.0,
            bands: [(1000.0, 500.0, 0.0); 2],
            flat: 0.0,
            nasal: 0.0,
            taps: 0.0,
            murmur: 0.0,
            burst: false,
        }
    }

    /// Whether it hisses or bursts.
    fn hisses(&self) -> bool {
        self.flat > 0.0 || self.bands.iter().any(|b| b.2 > 0.0)
    }

    /// Its noise: the parallel branch's bands and flat part.
    fn noise(self, bands: [(f32, f32, f32); 2], flat: f32) -> Self {
        Self { bands, flat, ..self }
    }
}

/// Samples in `ms` milliseconds.
fn ms(ms: f32) -> i64 {
    (f64::from(ms) * SR / 1000.0).round() as i64
}

// Times in milliseconds: the closure, burst and breath of plosives (the
// breath of p, t, k their voice onset time, shorter in singing than Lisker
// & Abramson's English 58, 70, 80 ms), nasals, liquids, glides, the trill
// and h; the glides of the formants into a vowel by what comes before.
const CLOSURE: [f32; 2] = [55.0, 45.0];
const BURST: f32 = 12.0;
const NASAL: f32 = 70.0;
const LIQUID: f32 = 60.0;
const GLIDE: f32 = 55.0;
const BREATH: f32 = 60.0;

/// The voice onset time of a voiceless plosive by its place, before a vowel.
fn onset_time(place: Place) -> f32 {
    match place {
        Place::Labial => 45.0,
        Place::Alveolar => 55.0,
        _ => 60.0,
    }
}

/// How long a voiced plosive's release hisses before its voicing, by its
/// place: German b, d, g short-lag, about 0 to 25 ms, without prevoicing
/// (Kehoe et al. 2004; Lisker & Abramson 1964's short lag), the lag
/// growing as the place lies further back (Lisker & Abramson 1964). The
/// numbers are ours, within that range: a velar's release long enough
/// that its compact noise is heard, the cue that tells g from b.
fn release(place: Place) -> f32 {
    match place {
        Place::Labial => 10.0,
        Place::Alveolar => 15.0,
        _ => 25.0,
    }
}

/// How long the formants glide into a vowel after a consonant (ms).
fn transition(p: &Phoneme) -> f32 {
    match (p.kind, p.place) {
        (Kind::Plosive { .. }, Place::Labial) => 35.0,
        (Kind::Plosive { .. }, Place::Velar) => 55.0,
        (Kind::Plosive { .. }, _) => 45.0,
        (Kind::Sonorant { .. }, Place::Labial | Place::Palatal) if p.name != "m" => 70.0,
        (Kind::Sonorant { .. }, _) if matches!(p.name, "m" | "n" | "ng") => 35.0,
        (Kind::Sonorant { .. } | Kind::Trill { .. }, _) => 50.0,
        (Kind::Breath { .. }, _) => 5.0,
        _ => 40.0,
    }
}

/// A vowel's five formants and their bandwidths: the table's F1 to F3, F4
/// and F5 a little higher behind front vowels, bandwidths widening with
/// the formants' height (near Klatt's 1980 values for vowels).
fn vowel_five(f: [f32; 3]) -> (Five, Five) {
    let front = ((f[1] - 1000.0) / 1300.0).clamp(0.0, 1.0);
    let formants = [f[0], f[1], f[2], 3300.0 + 250.0 * front, 3800.0 + 300.0 * front];
    let widths = [50.0 + 0.08 * (f[0] - 250.0).max(0.0), 70.0 + 0.02 * (f[1] - 800.0).max(0.0), 110.0 + 0.08 * (f[2] - 2000.0).max(0.0), 250.0, 300.0];
    (formants, widths)
}

/// Where a consonant's place sets the formants a vowel `v` glides from
/// (or to): F2 by a locus equation (its form Klatt 1979b's, via Klatt 1987,
/// p. 755; k and c ours, from memory of Sussman et al. 1991), F1 low, F3
/// by the place (ours).
fn locus(place: Place, v: Five) -> Five {
    let (k, c) = match place {
        Place::Labial => (0.85, 100.0),
        Place::Alveolar => (0.45, 1200.0),
        Place::Postalveolar => (0.4, 1300.0),
        Place::Palatal => (0.3, 1650.0),
        Place::Velar => (0.7, 800.0),
        Place::Glottal | Place::Vowel => (1.0, 0.0),
    };
    let f2 = k * v[1] + c;
    let f3 = match place {
        Place::Labial => v[2] * 0.92,
        Place::Alveolar => v[2].max(2700.0),
        Place::Velar => (f2 + 350.0).min(v[2]),
        Place::Postalveolar | Place::Palatal => v[2].max(2800.0),
        Place::Glottal | Place::Vowel => v[2],
    };
    let f1 = if matches!(place, Place::Glottal | Place::Vowel) { v[0] } else { v[0].min(280.0) };
    [f1, f2, f3, v[3], v[4]]
}

/// The noise of a fricative or a burst by its place: two bands (Hz, width,
/// level) and a flat part; a velar's band at the vowel's F2 and F3 (Klatt
/// 1980, p. 987: a velar's burst at F2 before a vowel not front). A
/// labial burst broad and weak, a labial fricative's noise through the
/// flat bypass alone.
fn spectrum(place: Place, v: Five, burst: bool) -> ([(f32, f32, f32); 2], f32) {
    match place {
        Place::Labial if burst => ([(1200.0, 1500.0, 0.3), (5500.0, 5000.0, 0.4)], 0.25),
        // Labiodental noise has no peaks: through the bypass alone (Klatt
        // 1980, p. 981-982, AB 57 dB for f and v).
        Place::Labial => ([(1000.0, 500.0, 0.0); 2], 1.0),
        Place::Alveolar => ([(6200.0, 2000.0, 1.0), (7800.0, 2000.0, 0.4)], 0.0),
        Place::Postalveolar => ([(2800.0, 700.0, 1.0), (3500.0, 800.0, 0.4)], 0.0),
        Place::Palatal => ([(3700.0, 900.0, 1.0), (4600.0, 1000.0, 0.35)], 0.0),
        Place::Velar => ([(v[1].max(1200.0), 500.0, 1.0), (v[2], 700.0, 0.4)], 0.0),
        Place::Glottal | Place::Vowel => ([(1000.0, 500.0, 0.0); 2], 0.0),
    }
}

/// How far a consonant's noise lies under a sung vowel, in dB: by its
/// class, as speech has them (relative phonetic powers after Sacia &
/// Beck, tabled by Fletcher 1953: sh about 13 dB under a, s and t about
/// 15, f and p about 20; voiced fricatives' hiss weaker under their voice).
fn under(p: &Phoneme) -> f32 {
    match (p.kind, p.place) {
        // A voiced fricative's hiss 10 dB under its voiceless twin's
        // (Klatt 1980, p. 986: AF 50 against 60 dB).
        (Kind::Fricative { voicing, .. }, Place::Labial) if voicing > 0.0 => -30.0,
        (Kind::Fricative { voicing, .. }, _) if voicing > 0.0 => -25.0,
        (Kind::Fricative { .. }, Place::Postalveolar) => -13.5,
        (Kind::Fricative { .. }, Place::Alveolar) => -15.0,
        (Kind::Fricative { .. }, Place::Palatal) => -17.0,
        (Kind::Fricative { .. }, Place::Velar) => -18.0,
        (Kind::Fricative { .. }, _) => -20.0,
        (Kind::Plosive { voiced: false, .. }, Place::Labial) => -18.0,
        (Kind::Plosive { voiced: false, .. }, _) => -12.0,
        (Kind::Plosive { .. }, Place::Labial) => -22.0,
        (Kind::Plosive { .. }, _) => -16.0,
        _ => -60.0,
    }
}

/// A sung vowel's RMS as the clear voice gives it (its voiced part).
const VOWEL: f32 = 0.30;

/// The levels of a parallel branch's bands and flat part for `p`: its
/// spectrum's shape, scaled so the whole lies `under(p)` dB under a vowel
/// (a band's noise power is its width's share of the sample rate, times
/// pi over 2 for its skirts).
fn noise_of(p: &Phoneme, v: Five, burst: bool) -> ([(f32, f32, f32); 2], f32) {
    let (bands, flat) = spectrum(p.place, v, burst);
    scaled(bands, flat, under(p))
}

/// Bands and a flat part scaled so their noise lies `under` dB under a vowel.
fn scaled(bands: [(f32, f32, f32); 2], flat: f32, under: f32) -> ([(f32, f32, f32); 2], f32) {
    let power: f32 = bands.iter().map(|(_, w, l)| l * l * std::f32::consts::PI * w / SAMPLE_RATE as f32).sum::<f32>() + flat * flat;
    let scale = VOWEL * 10f32.powf(under / 20.0) / power.sqrt().max(1e-6);
    (bands.map(|(f, w, l)| (f, w, l * scale)), flat * scale)
}

/// The breath of a voiceless plosive and of h, into the cascade.
const ASPIRATE: f32 = 0.04;

/// The German r before a vowel, uvular [ʁ] (the most common German r:
/// Wikipedia's Standard German phonology), a voiced fricative made the
/// Klatt way (voicing and a hiss together, Klatt 1980, p. 986). Its
/// numbers are ours, as no German values were at hand: F3 high as in l,
/// not an English r's low one; F1 raised and F2 low by a constriction far
/// back; its hiss low and weak, a rasp 36 dB under a vowel.
const UVULAR: [f32; 3] = [550.0, 1250.0, 2400.0];
const RASP: [(f32, f32, f32); 2] = [(1400.0, 1200.0, 1.0), (2600.0, 1500.0, 0.3)];
const RASP_UNDER: f32 = -36.0;

/// What a syllable's neighbours ask of the clear voice: how much of their
/// length its onset's and its coda's consonants keep (the two compressed
/// together where a coda and the next onset do not fit before the next
/// vowel), the sound before its onset (the coda of a syllable sung right
/// before it), whether a syllable was sung right before it at all, and the
/// next syllable's onset length when that follows without a rest (its coda
/// ends where that begins).
#[derive(Clone, Copy, Debug)]
pub(super) struct Join {
    onset: f32,
    coda: f32,
    before: Option<&'static Phoneme>,
    sung_before: bool,
    pub(super) next_onset: Option<i64>,
}

impl Default for Join {
    fn default() -> Self {
        Self { onset: 1.0, coda: 1.0, before: None, sung_before: false, next_onset: None }
    }
}

/// The least a vowel keeps between its beat and the next onset: 45 ms, or
/// 30 % of the time to the next beat.
fn vowel_least(span: i64) -> i64 {
    ms(45.0).max(span * 3 / 10)
}

/// The joins of a channel's clear notes: `laid` is what each note sings
/// (its syllable as its melisma gives it) and whether the next follows it
/// without a rest; `spans` the samples from each note's beat to the next's.
pub(super) fn joins(laid: &[(&Syllable, bool)], spans: &[i64]) -> Vec<Join> {
    let mut joins = vec![Join::default(); laid.len()];
    for i in 0..laid.len().saturating_sub(1) {
        let ((this, follows), (next, _)) = (laid[i], laid[i + 1]);
        if !follows {
            continue;
        }
        joins[i + 1].before = this.coda.last().copied();
        joins[i + 1].sung_before = true;
        let coda = coda_length(this, 1.0);
        let onset = onset_length(next, &joins[i + 1]);
        let room = spans[i] - vowel_least(spans[i]);
        let scale = if coda + onset > room { (room.max(0) as f32 / (coda + onset) as f32).clamp(0.0, 1.0) } else { 1.0 };
        joins[i].coda = scale;
        joins[i + 1].onset = scale;
        joins[i].next_onset = Some(onset_length(next, &joins[i + 1]));
    }
    joins
}

/// What follows a consonant: the vowel, or another consonant.
#[derive(Clone, Copy)]
enum Then {
    Vowel,
    Consonant,
    /// Nothing: a coda's last, released.
    End,
}

fn voiceless_fricative(p: &Phoneme) -> bool {
    matches!(p.kind, Kind::Fricative { voicing, .. } if voicing == 0.0)
}

fn nasal(p: &Phoneme) -> bool {
    matches!(p.name, "m" | "n" | "ng")
}

/// A plosive's closure, release and breath in ms, by what stands round it,
/// at `scale` of their length (never under each part's least): a voiced
/// one's release by its place (`release`), a voiceless one's breath long
/// only right before a vowel, unaspirated after a voiceless fricative (st,
/// sp, sk, schp: Klatt 1975b, via Klatt 1987, p. 758).
fn plosive_parts(p: &Phoneme, voiced: bool, before: Option<&Phoneme>, then: Then, scale: f32) -> [f32; 3] {
    let after_hiss = before.is_some_and(voiceless_fricative);
    let closure = if voiced { (CLOSURE[1], 25.0) } else { (CLOSURE[0], 30.0) };
    let burst = if voiced { (release(p.place), 8.0) } else { (BURST, 8.0) };
    let breath = match (voiced, then) {
        (true, _) => (0.0, 0.0),
        (false, _) if after_hiss => (12.0, 8.0),
        (false, Then::Vowel) => (onset_time(p.place), 35.0),
        (false, _) => (15.0, 8.0),
    };
    [closure, burst, breath].map(|(full, least): (f32, f32)| (full * scale).max(least.min(full)))
}

/// How long a consonant lasts in samples, at `scale` of its length (never
/// under its least: what still sounds as itself).
fn length(p: &Phoneme, before: Option<&Phoneme>, then: Then, scale: f32) -> i64 {
    let at = |full: f32, least: f32| ms((full * scale).max(least.min(full)));
    match p.kind {
        Kind::Vowel(_) | Kind::Diphthong(..) => 0,
        Kind::Plosive { voiced, .. } => plosive_parts(p, voiced, before, then, scale).iter().map(|&t| ms(t)).sum(),
        Kind::Fricative { length, voicing, .. } => at(length * 1000.0, if voicing > 0.0 { 35.0 } else { 45.0 }),
        Kind::Breath { .. } => at(BREATH, 35.0),
        Kind::Trill { length, .. } => at(length * 1000.0, 50.0),
        Kind::Sonorant { .. } if nasal(p) => at(NASAL, 40.0),
        Kind::Sonorant { .. } if matches!(p.name, "w" | "j") => at(GLIDE, 35.0),
        Kind::Sonorant { .. } => at(LIQUID, 35.0),
    }
}

/// Each consonant of a row (an onset or a coda) with the sound before it
/// and what follows it.
fn row<'a>(row: &'a [&'static Phoneme], before: Option<&'static Phoneme>, ends: Then) -> impl Iterator<Item = (&'static Phoneme, Option<&'static Phoneme>, Then)> + 'a {
    row.iter().enumerate().map(move |(i, &p)| (p, if i == 0 { before } else { Some(row[i - 1]) }, row.get(i + 1).map_or(ends, |_| Then::Consonant)))
}

/// How long the onset's consonants take before the vowel's beat, in samples.
pub(super) fn onset_length(s: &Syllable, join: &Join) -> i64 {
    row(&s.onset, join.before, Then::Vowel).map(|(p, b, t)| length(p, b, t, join.onset)).sum()
}

/// How long the coda's consonants take after the vowel, in samples.
fn coda_length(s: &Syllable, scale: f32) -> i64 {
    row(&s.coda, None, Then::End).map(|(p, b, t)| length(p, b, t, scale)).sum()
}

/// A labiodental fricative's formants and bandwidths, Klatt's (1980, Table
/// III, p. 987), its F4 and F5 the vowel's; none for another sound.
fn labiodental(p: &Phoneme, v: Five) -> Option<(Five, Five)> {
    match p.kind {
        Kind::Fricative { voicing, .. } if p.place == Place::Labial => Some(if voicing > 0.0 {
            ([220.0, 1100.0, 2080.0, v[3], v[4]], [60.0, 90.0, 120.0, 250.0, 300.0])
        } else {
            ([340.0, 1100.0, 2080.0, v[3], v[4]], [200.0, 120.0, 150.0, 250.0, 300.0])
        }),
        _ => None,
    }
}

/// The segments of one note, as `voice::plan` lays them for `voice`: the
/// onset before `beat`, the vowel from it, the coda ending at `off`; a
/// later segment wins where they overlap. Each consonant takes its locus
/// from the syllable's vowel.
pub(super) fn plan(out: &mut Vec<Seg>, s: &Syllable, part: Part, beat: i64, off: i64, join: &Join) {
    let (first, last) = match s.vowel.kind {
        Kind::Vowel(f) => (f, f),
        Kind::Diphthong(a, b) => (a, b),
        _ => unreachable!("a syllable's vowel is a vowel"),
    };
    let (v, vw) = vowel_five(first);
    let (end_v, _) = vowel_five(if part == Part::Whole { last } else { first });
    let first_at = out.len();
    let mut add = |seg: Seg| {
        if seg.to <= seg.from {
            return;
        }
        while out.len() > first_at && out.last().is_some_and(|l| l.from >= seg.from) {
            out.pop();
        }
        if let Some(l) = out[first_at..].last_mut() {
            l.to = l.to.min(seg.from);
        }
        out.push(seg);
    };
    // The onset; its first consonant follows a voiced sound when a syllable
    // was sung right before it and ended on a vowel or a voiced consonant.
    let voiced_before = join.sung_before && join.before.is_none_or(voiced);
    let mut t = beat - onset_length(s, join);
    for (i, (p, before, then)) in row(&s.onset, join.before, Then::Vowel).enumerate() {
        let after_voice = if i == 0 { voiced_before } else { before.is_some_and(voiced) };
        t = consonant(&mut add, Sound { p, before, then, vowel: v, scale: join.onset, onset: true, after_voice }, t);
    }
    // The vowel, gliding in from what came before; out to a coda's locus. The
    // reduced vowel er (an unstressed -er) sings softer.
    let coda_len = coda_length(s, join.coda);
    let vowel_end = off - coda_len;
    let into = s.onset.last().map_or(25.0, |p| transition(p));
    let mut vowel = Seg { name: s.vowel.name, ..Seg::new(beat, vowel_end, v, vw, into) };
    vowel.voicing = if s.vowel.name == "er" { REDUCED } else { 1.0 };
    vowel.turn = (part == Part::Whole && first != last).then_some(last);
    match s.coda.first() {
        Some(p) if !matches!(p.kind, Kind::Breath { .. }) => {
            // The last part of the vowel turns towards the coda's locus.
            let out_ms = transition(p).min(40.0);
            let turn_at = (vowel_end - ms(out_ms)).max(beat + (vowel_end - beat) / 2);
            add(Seg { to: turn_at as u64, ..vowel });
            add(Seg { name: s.vowel.name, voicing: vowel.voicing, ..Seg::new(turn_at, vowel_end, locus(p.place, end_v), vw, out_ms) });
        }
        _ => add(vowel),
    }
    // The coda.
    let mut t = vowel_end;
    for (i, (p, before, then)) in row(&s.coda, None, Then::End).enumerate() {
        let after_voice = i == 0 || before.is_some_and(voiced);
        t = consonant(&mut add, Sound { p, before, then, vowel: end_v, scale: join.coda, onset: false, after_voice }, t);
    }
}

/// Whether a sound is voiced: a vowel, a sonorant, a trill, a voiced
/// plosive or fricative.
fn voiced(p: &Phoneme) -> bool {
    match p.kind {
        Kind::Plosive { voiced, .. } => voiced,
        Kind::Fricative { voicing, .. } => voicing > 0.0,
        Kind::Breath { .. } => false,
        _ => true,
    }
}

/// How loud the reduced vowel er sings against a full one.
const REDUCED: f32 = 0.75;

/// A consonant as it is sung: what stands round it, the syllable's vowel
/// (its locus is taken from it), its share of its length, and whether a
/// voiced sound comes right before it.
struct Sound {
    p: &'static Phoneme,
    before: Option<&'static Phoneme>,
    then: Then,
    vowel: Five,
    scale: f32,
    onset: bool,
    after_voice: bool,
}

/// A consonant from `at`; where it ends.
fn consonant(add: &mut impl FnMut(Seg), s: Sound, at: i64) -> i64 {
    let Sound { p, before, then, vowel: v, scale, onset, after_voice } = s;
    let mut add = |seg: Seg| add(Seg { name: p.name, ..seg });
    let (_, vw) = vowel_five([v[0], v[1], v[2]]);
    let at_locus = locus(p.place, v);
    let wide = [vw[0] * 1.5, vw[1] * 1.3, vw[2], vw[3], vw[4]];
    match p.kind {
        Kind::Vowel(_) | Kind::Diphthong(..) => at,
        Kind::Plosive { voiced, .. } => {
            let [closure, burst, breath] = plosive_parts(p, voiced, before, then, scale);
            let mut t = at;
            let end = t + ms(closure);
            // A voiced plosive's voice bar only after a voiced sound: German
            // b, d, g begin without prevoicing (Kehoe et al. 2004), a voice
            // bar is there only in certain places (Klatt 1987, p. 752).
            let voicing = if voiced && after_voice { 0.12 } else { 0.0 };
            add(Seg { voicing, murmur: 1.0, ..Seg::new(t, end, at_locus, wide, 20.0) });
            t = end;
            let (bands, flat) = noise_of(p, at_locus, true);
            let end = t + ms(burst);
            add(Seg { burst: true, ..Seg::new(t, end, at_locus, wide, 0.0).noise(bands, flat) });
            t = end;
            if breath > 0.0 {
                // The breath before the voicing starts, the formants already
                // gliding towards the vowel right after it (else as they were).
                let end = t + ms(breath);
                let (to, glide) = if onset && matches!(then, Then::Vowel) { (v, transition(p).min(breath)) } else { (at_locus, 0.0) };
                add(Seg { aspiration: ASPIRATE, ..Seg::new(t, end, to, vw, glide) });
                t = end;
            }
            t
        }
        Kind::Fricative { voicing, .. } => {
            let end = at + length(p, before, then, scale);
            let (bands, flat) = noise_of(p, at_locus, false);
            // A voiced fricative is two voices at once, through the cascade
            // and as a murmur, AV and AVS equal (Klatt 1980, p. 986: AV 47,
            // AVS 47 dB); a labiodental takes Klatt's own formants and
            // bandwidths (Table III, p. 987: f 340/1100/2080 Hz, B
            // 200/120/150; v 220/1100/2080, B 60/90/120).
            // Each 13 dB under a vowel's AV 60: 0.22 each, 0.45 together.
            let (f, w) = labiodental(p, v).unwrap_or((at_locus, wide));
            let voicing = if voicing > 0.0 { 0.45 } else { 0.0 };
            add(Seg { voicing, murmur: if voicing > 0.0 { 0.5 } else { 0.0 }, ..Seg::new(at, end, f, w, 30.0).noise(bands, flat) });
            end
        }
        Kind::Breath { .. } => {
            let end = at + length(p, before, then, scale);
            add(Seg { aspiration: ASPIRATE * 0.6, ..Seg::new(at, end, v, vw, 0.0) });
            end
        }
        Kind::Sonorant { voicing, .. } if p.name == "r" => {
            // The German r: before a vowel a light rasp, beating with the
            // glottis as a voiced fricative's hiss does; in a coda, where
            // German vocalizes it (Wikipedia's Standard German phonology),
            // its voice alone.
            let end = at + length(p, before, then, scale);
            let f = [UVULAR[0], UVULAR[1], UVULAR[2], v[3], v[4]];
            let seg = Seg { voicing, ..Seg::new(at, end, f, wide, if onset { 20.0 } else { 35.0 }) };
            if onset {
                let (bands, flat) = scaled(RASP, 0.0, RASP_UNDER);
                add(seg.noise(bands, flat));
            } else {
                add(seg);
            }
            end
        }
        Kind::Trill { formants, .. } => {
            let end = at + length(p, before, then, scale);
            let f = [formants[0], formants[1], formants[2], v[3], v[4]];
            add(Seg { voicing: 0.8, taps: 26.0, ..Seg::new(at, end, f, wide, 25.0) });
            end
        }
        Kind::Sonorant { formants, voicing, .. } => {
            let end = at + length(p, before, then, scale);
            let zero = match p.name {
                "m" => 1000.0,
                "n" => 1500.0,
                "ng" => 2800.0,
                _ => 0.0,
            };
            let f = if zero > 0.0 { [250.0, at_locus[1], at_locus[2], v[3], v[4]] } else { [formants[0], formants[1], formants[2], v[3], v[4]] };
            add(Seg { voicing: if zero > 0.0 { voicing * 1.4 } else { voicing }, nasal: zero, ..Seg::new(at, end, f, wide, if onset { 20.0 } else { 35.0 }) });
            end
        }
    }
}

/// A biquad's coefficients and two samples of its state (direct form I).
#[derive(Clone, Copy, Debug, Default)]
struct Band {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Band {
    /// A band-pass of peak gain 1 at `f` Hz, `bw` wide.
    fn set(&mut self, f: f32, bw: f32) {
        let w = std::f64::consts::TAU * f64::from(f).min(SR * 0.45) / SR;
        // Q is the band's middle over its width.
        let alpha = w.sin() * f64::from(bw) / (2.0 * f64::from(f).max(1.0));
        let a0 = 1.0 + alpha;
        self.b = [alpha / a0, 0.0, -alpha / a0];
        self.a = [-2.0 * w.cos() / a0, (1.0 - alpha) / a0];
    }

    fn next(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// A formant resonator's coefficients (Klatt's: unity gain at 0 Hz).
fn resonator(f: f64, bw: f64) -> (f64, f64, f64) {
    let c = -(-2.0 * std::f64::consts::PI * bw / SR).exp();
    let b = 2.0 * (-std::f64::consts::PI * bw / SR).exp() * (std::f64::consts::TAU * f / SR).cos();
    (1.0 - b - c, b, c)
}

/// One sample through a resonator of coefficients `(a, b, c)`.
fn resonate((a, b, c): (f64, f64, f64), x: f64, s: &mut (f64, f64)) -> f64 {
    let y = a * x + b * s.0 + c * s.1;
    *s = (y, s.0);
    y
}

/// One sample through the antiresonator (a zero) of a resonator's
/// coefficients: its inverse.
fn antiresonate((a, b, c): (f64, f64, f64), x: f64, s: &mut (f64, f64)) -> f64 {
    let y = (x - b * s.0 - c * s.1) / a;
    *s = (x, s.0);
    y
}

/// A resonator's coefficients, kept with the formant and width they are for.
type Made = ((f64, f64), (f64, f64, f64));

/// The nasal pole: where it lies, and how wide; a zero at the same place
/// cancels it outside nasals (Klatt's way).
const NASAL_POLE: (f64, f64) = (270.0, 100.0);

/// The clear voice's vowels' typical level (their RMS before the volumes):
/// what a drive raises a voice by at that level raises its noise past the
/// clipper (`voice`'s is `player::SUNG`).
pub(super) const SUNG: f32 = 0.45;

/// The pitch the voice's loudness is set at (Hz).
const PITCHED: f64 = 175.0;

/// Where the murmur's low-pass lies (Hz).
const MURMUR: f64 = 350.0;

/// The flat noise's low-pass (its bend at 5 kHz, ours) and what keeps its
/// level as it was: white noise keeps a/(2 - a) of its power under it.
const FLAT_PASS: f64 = 0.5096;
const FLAT_GAIN: f64 = 1.71;

/// The glottis open this share of each period.
const OPEN: f64 = 0.6;

/// A breath on every vowel, a part of the pulse.
const BREATHY: f64 = 0.03;

/// What brings a sung vowel to `voice`'s loudness.
const LOUDNESS: f64 = 6.5;

/// The clear voice as it sings, sample by sample.
#[derive(Clone, Debug)]
pub(super) struct Singer {
    seed: u32,
    /// The pitch as heard (gliding into the note's), the glottis' phase,
    /// this period's jitter and shimmer, the tilt's state.
    pitch: f64,
    phase: f64,
    jitter: f64,
    shimmer: f64,
    tilt: f64,
    /// The cascade's states for the voicing and, apart, for the breath:
    /// the nasal pole and zero, then F1 to F5.
    voiced: [(f64, f64); 7],
    breath: [(f64, f64); 7],
    bands: [Band; 2],
    last: (f64, f64),
    /// The formants and bandwidths now, and where their glide began.
    formants: [f64; 5],
    widths: [f64; 5],
    from: ([f64; 5], [f64; 5]),
    /// The resonators' coefficients and the formant and width they are for.
    coefs: [Made; 5],
    /// The levels as they glide: voicing, aspiration, murmur; the voicing a
    /// pulse takes, set only as a pulse begins (Klatt 1980, p. 977: no pops
    /// from a level changed within a period); the murmur's low-pass.
    levels: [f64; 3],
    av: f64,
    low: f64,
    /// The nasal zero where it is and where its glide began; the flat
    /// noise's low-pass (Klatt's frication source falls 6 dB an octave,
    /// 1980, p. 976; where it bends, 5 kHz, is ours).
    zero: (f64, f64),
    flat_low: f64,
    /// How far the murmur's low-pass moves a sample; the nasal pole's
    /// coefficients, and the zero's with where they are for.
    pass: f64,
    pole: (f64, f64, f64),
    zero_made: (f64, (f64, f64, f64)),
    cursor: usize,
    entered: Option<usize>,
}

impl Default for Singer {
    fn default() -> Self {
        Self::new(0x5bd1_e995)
    }
}

impl Singer {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            seed: seed.max(1),
            pitch: 0.0,
            phase: 0.0,
            jitter: 1.0,
            shimmer: 1.0,
            tilt: 0.0,
            voiced: [(0.0, 0.0); 7],
            breath: [(0.0, 0.0); 7],
            bands: [Band::default(); 2],
            last: (0.0, 0.0),
            formants: [500.0, 1500.0, 2500.0, 3300.0, 3800.0],
            widths: [80.0, 90.0, 150.0, 250.0, 300.0],
            from: ([500.0, 1500.0, 2500.0, 3300.0, 3800.0], [80.0, 90.0, 150.0, 250.0, 300.0]),
            coefs: [((0.0, 0.0), (0.0, 0.0, 0.0)); 5],
            levels: [0.0; 3],
            av: 0.0,
            low: 0.0,
            zero: (NASAL_POLE.0, NASAL_POLE.0),
            flat_low: 0.0,
            pass: 1.0 - (-std::f64::consts::TAU * MURMUR / SR).exp(),
            pole: resonator(NASAL_POLE.0, NASAL_POLE.1),
            zero_made: (0.0, (0.0, 0.0, 0.0)),
            cursor: 0,
            entered: None,
        }
    }

    /// A note taken up: its segments from the start. From silence all is
    /// still, the formants where the note starts and the pitch a little
    /// below its own, to glide up into it.
    pub(super) fn start(&mut self, plan: &[Seg], from_silence: bool) {
        (self.cursor, self.entered) = (0, None);
        if from_silence {
            *self = Self { seed: self.seed, ..Self::new(self.seed) };
            if let Some(s) = plan.first() {
                self.formants = s.formants.map(f64::from);
                self.widths = s.widths.map(f64::from);
            }
        }
    }

    /// A uniform random number in -1..1.
    fn uniform(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        f64::from(self.seed) / f64::from(u32::MAX) * 2.0 - 1.0
    }

    /// A noise sample near a normal distribution.
    fn noise(&mut self) -> f64 {
        (self.uniform() + self.uniform() + self.uniform() + self.uniform()) * 0.75f64.sqrt()
    }

    /// The next sample, `t` samples into the note whose segments are
    /// `plan`, at `f0` Hz: its voiced part and its noise, apart.
    pub(super) fn next(&mut self, plan: &[Seg], t: u64, f0: f32) -> (f32, f32) {
        let noise = self.noise();
        while plan.get(self.cursor).is_some_and(|s| t >= s.to) {
            self.cursor += 1;
        }
        let seg = plan.get(self.cursor).filter(|s| t >= s.from);
        let (mut voicing, mut aspiration, mut taps, mut murmur) = (0.0, 0.0, 0.0, 0.0);
        if let Some(s) = seg {
            if self.entered != Some(self.cursor) {
                // A new segment: its glide starts from where the voice is.
                self.entered = Some(self.cursor);
                self.from = (self.formants, self.widths);
                self.zero.1 = self.zero.0;
                for (band, (f, w, _)) in self.bands.iter_mut().zip(s.bands) {
                    band.set(f, w);
                }
            }
            let into = (t - s.from) as f64;
            let k = if s.glide == 0 || into >= f64::from(s.glide) { 1.0 } else { 0.5 - 0.5 * (std::f64::consts::PI * into / f64::from(s.glide)).cos() };
            let mut aim = s.formants;
            if let Some(turn) = s.turn {
                let along = (t - s.from) as f32 / (s.to - s.from) as f32;
                let q = ((along - 0.25) / 0.5).clamp(0.0, 1.0);
                for (a, to) in aim.iter_mut().zip(turn) {
                    *a += (to - *a) * q;
                }
            }
            let between = |from: f64, to: f32| from + (f64::from(to) - from) * k;
            self.formants = std::array::from_fn(|r| between(self.from.0[r], aim[r]));
            self.widths = std::array::from_fn(|r| between(self.from.1[r], s.widths[r]));
            // The nasal zero glides as the formants do: a zero jumping clicks
            // (Klatt 1980, p. 987).
            let zero_to = if s.nasal > 0.0 { f64::from(s.nasal) } else { NASAL_POLE.0 };
            self.zero.0 = self.zero.1 + (zero_to - self.zero.1) * k;
            (voicing, aspiration, taps, murmur) = (f64::from(s.voicing), f64::from(s.aspiration), f64::from(s.taps), f64::from(s.murmur));
        }
        // The levels glide (5 ms voicing, murmur and breath).
        let keep = |ms: f64| (-1000.0 / (SR * ms)).exp();
        self.levels[0] = voicing + (self.levels[0] - voicing) * keep(5.0);
        self.levels[1] = aspiration + (self.levels[1] - aspiration) * keep(4.0);
        self.levels[2] = murmur + (self.levels[2] - murmur) * keep(5.0);

        // The pitch glides into the note's (30 ms).
        let f0 = f64::from(f0.max(1.0));
        self.pitch = if self.pitch <= 0.0 { f0 * 0.96 } else { f0 + (self.pitch - f0) * keep(30.0) };
        // The glottal pulse: a flow rising and falling over the open part
        // of each period (KLGLOTT88), then a gentle tilt.
        self.phase += self.pitch * self.jitter / SR;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.jitter = 1.0 + 0.004 * self.uniform();
            self.shimmer = 1.0 + 0.03 * self.uniform();
            self.av = voicing;
        }
        let x = self.phase / OPEN;
        let flow = if x < 1.0 { 6.75 * x * x * (1.0 - x) } else { 0.0 };
        // A flow as much smaller as the period is shorter: the lips'
        // radiation raises it with the pitch, so a vowel keeps its
        // loudness (and the consonants their distance) at any pitch.
        let mut voice = flow * self.av * self.shimmer * PITCHED / self.pitch;
        // A voice bar hums: the pulse low-passed, as much as it murmurs.
        self.low += (voice - self.low) * self.pass;
        voice += (self.low - voice) * self.levels[2];
        if taps > 0.0 {
            // The tongue tapping: the voicing broken `taps` times a second.
            let tap = (t as f64 * taps / SR).fract();
            voice *= if tap < 0.35 { 0.15 } else { 1.0 };
        }
        self.tilt += (voice - self.tilt) * 0.55;
        let breath = noise * (self.levels[1] + BREATHY * flow * self.av);
        // The cascade: the nasal pole and zero, then F1 to F5.
        let pole = self.pole;
        if self.zero_made.0 != self.zero.0 {
            self.zero_made = (self.zero.0, resonator(self.zero.0, NASAL_POLE.1));
        }
        let zero = self.zero_made.1;
        for (r, (made, co)) in self.coefs.iter_mut().enumerate() {
            if *made != (self.formants[r], self.widths[r]) {
                (*made, *co) = ((self.formants[r], self.widths[r]), resonator(self.formants[r], self.widths[r]));
            }
        }
        let mut out = [self.tilt, breath];
        for (o, states) in out.iter_mut().zip([&mut self.voiced, &mut self.breath]) {
            let mut y = resonate(pole, *o, &mut states[0]);
            y = antiresonate(zero, y, &mut states[1]);
            for (r, (_, co)) in self.coefs.iter().enumerate() {
                y = resonate(*co, y, &mut states[r + 2]);
            }
            *o = y;
        }
        // The lips' radiation on the cascade.
        let voiced = out[0] - self.last.0;
        let breathed = out[1] - self.last.1;
        self.last = (out[0], out[1]);
        // The parallel branch: the hiss through its own bands.
        let mut hiss = 0.0;
        if let Some(s) = seg.filter(|s| s.hisses()) {
            for (band, (_, _, level)) in self.bands.iter_mut().zip(s.bands) {
                hiss += band.next(noise) * f64::from(level);
            }
            self.flat_low += (noise - self.flat_low) * FLAT_PASS;
            hiss += self.flat_low * FLAT_GAIN * f64::from(s.flat);
            // A hiss comes and goes over 5 ms (Klatt 1980, p. 977), coming
            // only where no hiss came before, going only where none follows;
            // a burst jumps on (the same page) and falls to half by its end
            // (the fall ours).
            let (into, left) = ((t - s.from) as f64, (s.to - t) as f64);
            let ramp = 5.0 * SR / 1000.0;
            let comes = !s.burst && !self.cursor.checked_sub(1).and_then(|c| plan.get(c)).is_some_and(|b| b.to == s.from && b.hisses());
            let goes = !plan.get(self.cursor + 1).is_some_and(|n| n.from == s.to && n.hisses());
            hiss *= if comes { (into / ramp).min(1.0) } else { 1.0 } * if goes { (left / ramp).min(1.0) } else { 1.0 };
            if s.burst {
                hiss *= 1.0 - 0.5 * into / (s.to - s.from) as f64;
            }
            if s.voicing > 0.0 {
                // A voiced fricative's hiss beats with the glottis, half as
                // strong as the glottis closes (Klatt 1980, p. 976-977: a
                // square wave of 50 %; here a cosine, ours, so it does not
                // step).
                hiss *= 0.75 + 0.25 * (std::f64::consts::TAU * self.phase).cos();
            }
        } else {
            for band in &mut self.bands {
                band.next(0.0);
            }
        }
        ((voiced * LOUDNESS) as f32, (breathed * LOUDNESS + hiss) as f32)
    }
}

#[cfg(test)]
impl Singer {
    /// F1 to F3 as they are now.
    pub(super) fn formants_now(&self) -> [f64; 3] {
        [self.formants[0], self.formants[1], self.formants[2]]
    }
}


#[cfg(test)]
impl Seg {
    /// The middle of its noise's main band, if it hisses or bursts through
    /// a band (not the flat bypass alone).
    pub(super) fn band(&self) -> Option<f32> {
        (self.bands[0].2 > 0.0).then_some(self.bands[0].0)
    }

    /// Whether it hisses or bursts at all.
    pub(super) fn noisy(&self) -> bool {
        self.hisses()
    }

    /// Whether it breathes (an aspiration).
    pub(super) fn breathes(&self) -> bool {
        self.aspiration > 0.0
    }
}

//! Instruments that move as they sound: a drive that slides over each note
//! (`drive_end`, `drive_time`), oscillators in unison (`unison`, `detune`).

use super::player::*;
use super::*;
use crate::audio::SAMPLE_RATE;

const SR: f32 = SAMPLE_RATE as f32;

/// The left side of `text` played for `seconds`.
fn left(text: &str, seconds: f32) -> Vec<f32> {
    Player::new(parse(text).unwrap(), 1.0).render(Some(seconds)).into_iter().step_by(2).collect()
}

/// The left side of `text` played for `seconds`, its strips as `mix` from
/// its first sample.
fn mixed(text: &str, seconds: f32, mix: &Mix) -> Vec<f32> {
    let mut p = Player::new(parse(text).unwrap(), 1.0);
    p.start_mix(mix);
    p.render(Some(seconds)).into_iter().step_by(2).collect()
}

/// A mix whose strip `letter` turns `word` to `value` at once.
fn turned(letter: char, word: &str, value: f32) -> Mix {
    let mut mix = Mix::default();
    assert!(std::sync::Arc::make_mut(&mut mix.strips).entry(letter).or_default().turn(word, Some(value), 0.0));
    mix
}

/// The samples within `from..to` seconds.
fn within(s: &[f32], from: f32, to: f32) -> &[f32] {
    &s[(from * SR) as usize..(to * SR) as usize]
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
}

/// How square a stretch is: its RMS against its peak, a clean sine's 0.71,
/// nearer 1 the harder it is clipped.
fn squareness(s: &[f32]) -> f32 {
    rms(s) / peak(s)
}

/// The largest difference between two renders.
fn apart(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()))
}

/// A sine held at full level, its words after.
fn held(words: &str) -> String {
    format!("#tempo 60\n@k sine freq=110 attack=0 decay=0 sustain=1 release=0.05 volume=1 {words}\nA: @k c1\n")
}

/// `drive_end`: the clipper slides evenly in dB from `drive` to it over
/// `drive_time`, early clipped hard, late gently, then stays there: past
/// its time as a note driven at the end all along (to a float's rounding).
/// With an end the clipper works the whole note, 0 dB a rounding at a gain
/// of 1 (tanh), not clean; `drive=0` without one is clean as ever.
#[test]
fn a_drive_slides_from_hard_to_gentle() {
    let slid = left(&held("drive=24 drive_end=0 drive_time=0.4"), 0.9);
    let (early, late) = (squareness(within(&slid, 0.02, 0.1)), squareness(within(&slid, 0.5, 0.85)));
    assert!(early > 0.8 && late < 0.76 && early > late + 0.08, "early {early}, late {late}");
    // Halfway in dB: as a fixed drive of 12 there.
    let twelve = squareness(within(&left(&held("drive=12"), 0.9), 0.5, 0.85));
    let middle = squareness(within(&slid, 0.19, 0.21));
    assert!((middle - twelve).abs() < 0.03, "halfway {middle}, a drive of 12 {twelve}");
    let gentle = left(&held("drive=0 drive_end=0"), 0.9);
    assert!(apart(within(&slid, 0.5, 0.85), within(&gentle, 0.5, 0.85)) < 1e-3, "past its time not where it ended");
    let clean = left(&held(""), 0.9);
    let rounded = peak(within(&gentle, 0.5, 0.85)) / peak(within(&clean, 0.5, 0.85));
    assert!((rounded - libm::tanhf(1.0)).abs() < 0.01, "0 dB with an end is tanh at a gain of 1: {rounded}");
    assert!(left(&held("drive=0"), 0.9) == clean, "drive=0 without an end is clean");
    // Up the other way, over attack and decay unless it says how long.
    let up = left(&held("drive=0 drive_end=24 decay=0.3"), 0.9);
    assert!(squareness(within(&up, 0.5, 0.85)) > squareness(within(&up, 0.01, 0.05)) + 0.08);
}

/// A sum's drive slides as an instrument's; a strip's drive_end goes to
/// the sum where its `=` line states a drive or its end, the same as the
/// file saying so there, and to the first layer's line otherwise.
#[test]
fn a_sums_drive_slides_and_a_strip_turns_it() {
    let song = |own: &str, sum: &str| format!("#tempo 60\n@k saw freq=110 attack=0 decay=0 sustain=1 volume=0.6 {own}\n+ saw freq=165 attack=0 decay=0 sustain=1 volume=0.6\n= {sum}\nA: @k c1\n");
    let slid = left(&song("", "drive=24 drive_end=0 drive_time=0.3"), 0.9);
    let (early, late) = (squareness(within(&slid, 0.02, 0.1)), squareness(within(&slid, 0.5, 0.85)));
    assert!(early > late + 0.1, "the sum's drive did not slide: early {early}, late {late}");
    let stated = song("", "drive=24 drive_end=0 drive_time=0.3");
    assert!(mixed(&stated, 0.9, &turned('A', "drive_end", 12.0)) == left(&song("", "drive=24 drive_end=12 drive_time=0.3"), 0.9), "drive_end to the sum");
    assert!(mixed(&stated, 0.9, &turned('A', "drive_time", 0.1)) == left(&song("", "drive=24 drive_end=0 drive_time=0.1"), 0.9), "drive_time to the sum");
    let own = song("drive=24 drive_end=0", "volume=1");
    assert!(mixed(&own, 0.9, &turned('A', "drive_end", 12.0)) == left(&song("drive=24 drive_end=12", "volume=1"), 0.9), "drive_end to the first layer");
}

/// A voice whose drive slides raises its breath and hiss as it goes, as a
/// fixed drive raises them: late in the note as one driven at the end.
#[test]
fn a_voices_noise_follows_its_sliding_drive() {
    let song = |words: &str| format!("#tempo 60\n@v singer attack=0.005 decay=0 sustain=1 release=0.05 volume=0.8 {words}\nA: @v o3 \"s a s\" c1\n");
    let slid = left(&song("drive=20 drive_end=6 drive_time=0.2"), 1.0);
    let fixed = left(&song("drive=6 drive_end=6"), 1.0);
    let late = |s: &[f32]| rms(within(s, 0.4, 0.7));
    assert!((late(&slid) / late(&fixed) - 1.0).abs() < 0.01, "late {} against {}", late(&slid), late(&fixed));
}

/// The drive's mistakes say their line and the word: a value out of
/// range, on an instrument's line and on its sum's.
#[test]
fn the_drives_mistakes_say_their_line() {
    let wrong = |text: &str| format!("{:#}", parse(text).unwrap_err());
    for (text, says) in [
        ("@k sine drive_end=40\nA: @k c\n", "line 1: @k: drive_end=40 is out of range, from 0 to 36 dB"),
        ("@k sine\n+ sine drive_time=-1\nA: @k c\n", "line 2: @k's layer 2: drive_time=-1 is out of range, from 0 to 10 seconds"),
        ("@k sine\n+ sine\n= drive_end=37\nA: @k c\n", "line 3: @k's sum (=): drive_end=37 is out of range"),
        ("@k sine drive_ned=3\nA: @k c\n", "did you mean 'drive_end'?"),
    ] {
        let e = wrong(text);
        assert!(e.contains(says), "{text:?}: {e}");
    }
}

/// How strong `freq` Hz is in `s` (the magnitude of its one bin, a
/// window of whole seconds).
fn strength(s: &[f32], freq: f32) -> f32 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, v) in s.iter().enumerate() {
        let a = std::f64::consts::TAU * f64::from(freq) * i as f64 / f64::from(SR);
        (re, im) = (re + f64::from(*v) * libm::cos(a), im + f64::from(*v) * libm::sin(a));
    }
    (libm::sqrt(re * re + im * im) / s.len() as f64) as f32
}

/// `unison=1`, or unset, is the one oscillator of before to the sample,
/// whatever its detune; three in unison half a semitone apart sound at
/// the note and half a semitone either side of it, about as strong each,
/// and nothing between; seven are about as loud as one, and spread over
/// their turns they strike no edge together at the note's start.
#[test]
fn unison_spreads_oscillators_about_the_note() {
    let song = |words: &str| format!("#tempo 60\n@u sine attack=0 decay=0 sustain=1 volume=1 {words}\nA: @u o4 a1\n");
    let one = left(&song(""), 2.0);
    assert!(left(&song("unison=1 detune=0.7"), 2.0) == one, "unison=1 is not the oscillator of before");
    let three = left(&song("unison=3 detune=0.5"), 2.0);
    let window = within(&three, 0.5, 1.5);
    let at = |semitones: f32| strength(window, 440.0 * libm::powf(2.0, semitones / 12.0));
    let (low, mid, high, between) = (at(-0.5), at(0.0), at(0.5), at(0.25));
    for (s, name) in [(low, "below"), (mid, "at the note"), (high, "above")] {
        assert!((s / mid - 1.0).abs() < 0.05 && s > 10.0 * between, "{name} {s}, at the note {mid}, between {between}");
    }
    // Saws, seven against one, as loud; the start no louder than it goes on.
    let saw = |words: &str| left(&format!("#tempo 60\n@u saw attack=0 decay=0 sustain=1 volume=1 {words}\nA: @u o2 a1\n"), 2.0);
    let (alone, seven) = (saw(""), saw("unison=7 detune=0.3"));
    let louder = rms(within(&seven, 0.2, 1.8)) / rms(within(&alone, 0.2, 1.8));
    assert!((0.7..1.4).contains(&louder), "seven in unison {louder} times as loud as one");
    assert!(peak(within(&seven, 0.0, 0.01)) < 1.2 * peak(within(&seven, 0.5, 1.5)), "a strike at the start: {} against {}", peak(within(&seven, 0.0, 0.01)), peak(within(&seven, 0.5, 1.5)));
    assert!(saw("unison=7 detune=0.3") == seven, "two renders differ");
}

/// The cost of unison at its worst: four layers of seven sines each (the
/// dearest wave), a note on every one of eight channels, against the same
/// with one oscillator a layer. Printed (here 4 s took 90 ms with one,
/// 193 with seven: 0.6 % of a core a voice); and far quicker than it
/// plays.
#[test]
fn unison_at_its_worst_is_quick() {
    let song = |unison: u32| {
        let line = format!("sine unison={unison} detune=0.2 filter=lowpass cutoff=2000 attack=0.01 decay=0 sustain=1 volume=0.1");
        let channels: String = "ABCDEFGH".chars().map(|c| format!("{c}: @w o3 c1 e1\n")).collect();
        format!("#tempo 120\n@w {line}\n+ {line}\n+ {line}\n+ {line}\n{channels}")
    };
    let seconds = 4.0;
    let time = |unison: u32| {
        let p = Player::new(parse(&song(unison)).unwrap(), 1.0);
        let at = std::time::Instant::now();
        std::hint::black_box(p.render(Some(seconds)));
        at.elapsed().as_secs_f32()
    };
    let (one, seven) = (time(1), time(7));
    println!("eight voices of four layers for {seconds} s: {:.1} ms with one oscillator a layer, {:.1} ms with seven ({:.2} % of a core for one such voice)", one * 1e3, seven * 1e3, seven / seconds / 8.0 * 100.0);
    assert!(seven < seconds / 4.0, "seven in unison on four layers of eight voices took {seven} s for {seconds} s");
}

/// Unison's mistakes say their line and the word: on a wave without a
/// pitch to spread (noise), a sample, a voice and on the sum's line; past
/// seven, or not whole.
#[test]
fn unisons_mistakes_say_their_line() {
    let wrong = |text: &str| format!("{:#}", parse(text).unwrap_err());
    for (text, says) in [
        ("@n noise unison=3\nA: @n c\n", "line 1: @n: unison means nothing on a noise"),
        ("@n saw\n+ noise detune=0.3\nA: @n c\n", "line 2: @n's layer 2: detune means nothing on a noise"),
        ("@s sample name=thump unison=2\nA: @s c\n", "line 1: @s: unison means nothing on a sample"),
        ("@v voice unison=2\nA: @v \"l a\" c\n", "line 1: @v: unison means nothing on a voice"),
        ("@v singer detune=0.2\nA: @v \"l a\" c\n", "line 1: @v: detune means nothing on a singer"),
        ("@k saw\n+ saw\n= unison=3\nA: @k c\n", "line 3: @k's sum (=): the sum takes"),
        ("@k saw unison=8\nA: @k c\n", "line 1: @k: unison=8 is out of range, from 1 to 7"),
        ("@k saw unison=2.5\nA: @k c\n", "line 1: @k: unison=2.5 is no whole number of oscillators"),
        ("@k saw detune=2\nA: @k c\n", "detune=2 is out of range, from 0 to 1 semitones"),
    ] {
        let e = wrong(text);
        assert!(e.contains(says), "{text:?}: {e}");
    }
}

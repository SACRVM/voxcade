//! Instruments that move as they sound: a drive that slides over each note
//! (`drive_end`, `drive_time`).

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

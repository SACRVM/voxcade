//! A note that gives way to the next on its channel fades out under it
//! (`player::give_way`) instead of stopping dead: no step in the wave.

use super::player::*;
use super::sample::*;
use super::*;
use crate::audio::SAMPLE_RATE;

const SR: f32 = SAMPLE_RATE as f32;

/// The samples a note giving way fades over (`GIVE_WAY`, 5 ms): it is
/// heard for one fewer, then silent.
const FADE: usize = SAMPLE_RATE as usize / 200;

/// The left side of `song` played for `seconds`, at `ratio` times its tempo.
fn left_of(song: Song, seconds: f32, ratio: f64) -> Vec<f32> {
    let mut p = Player::new(song, 1.0);
    p.set_ratio(ratio);
    p.render(Some(seconds)).into_iter().step_by(2).collect()
}

fn left(text: &str, seconds: f32) -> Vec<f32> {
    left_of(parse(text).unwrap(), seconds, 1.0)
}

/// The largest step from one sample to the next within `from..to` samples.
fn steepest(s: &[f32], from: usize, to: usize) -> f32 {
    s[from..to.min(s.len())].windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()))
}

/// The largest step within 2 ms of each sample of `at`, and the largest
/// anywhere else in `s`.
fn steps_at(s: &[f32], at: &[usize]) -> (f32, f32) {
    let near = (0.002 * SR) as usize;
    let mut others = vec![true; s.len()];
    let mut there = 0.0f32;
    for &a in at {
        there = there.max(steepest(s, a - near, a + near));
        others[a - near..a + near].fill(false);
    }
    let elsewhere = s.windows(2).enumerate().filter(|(i, _)| others[*i] && others[i + 1]).fold(0.0f32, |m, (_, w)| m.max((w[1] - w[0]).abs()));
    (there, elsewhere)
}

/// The bass drum the owner heard crackle: a sine of a second's decay and
/// most of a second's release, hit every quarter at 125, so each hit takes
/// the voice from a note still well up.
const KICK: &str = "#tempo 125\n@kick sine freq=52.44 sweep=-1 sweep_time=0.4 attack=0.001 decay=1.0 sustain=0 release=0.9 volume=0.9\n";

/// A quarter at 125, in samples.
const QUARTER: usize = 21_168;

/// The kick hit every quarter steps no further at a hit than its wave does
/// anywhere else (it stepped 30 times as far, cut dead: 0.26 against
/// 0.0086), at its own tempo and at another.
#[test]
fn a_long_note_taken_over_steps_no_further_than_its_wave() {
    let text = format!("{KICK}K: @kick v10 o4 l16 c r r r c r r r c r r r c r r r r1\n");
    let s = left(&text, 4.0);
    let (there, elsewhere) = steps_at(&s, &[QUARTER, 2 * QUARTER, 3 * QUARTER]);
    assert!(there <= elsewhere, "a step of {there} at the hits, {elsewhere} elsewhere");
    let fast = left_of(parse(&text).unwrap(), 3.0, 1.5);
    let q = (QUARTER as f64 / 1.5).round() as usize;
    let (there, elsewhere) = steps_at(&fast, &[q, 2 * q, 3 * q]);
    assert!(there <= elsewhere, "at 1.5 times the tempo, a step of {there} at the hits, {elsewhere} elsewhere");
}

/// What a note taken over leaves: the new note's samples exactly as if it
/// were alone, on time, with the old one under it (let go, as played
/// alone) at a weight falling from 1 to 0 over `FADE` samples, then the new
/// note's alone, to the bit. `song` makes the song of a channel's notes,
/// `first` and `second` a quarter each.
fn hands_over(song: impl Fn(&str) -> Song, first: &str, second: &str) {
    let at = QUARTER;
    let play = |line: String| left_of(song(&line), 1.0, 1.0);
    let (both, old, new) = (play(format!("{first} {second} r1")), play(format!("{first} r r1")), play(format!("r {second} r1")));
    assert_eq!(both[..at], old[..at], "before the second note, the first alone");
    let sounding = old[at - 441..at].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(sounding > 0.05, "the first note still sounds as the second comes ({sounding})");
    for k in 0..FADE - 1 {
        let w = (FADE - 1 - k) as f32 / FADE as f32;
        let want = new[at + k] + old[at + k] * w;
        assert!((both[at + k] - want).abs() < 1e-5, "{k} samples in: {} where the new note and the old faded make {want}", both[at + k]);
    }
    assert_eq!(both[at + FADE - 1..], new[at + FADE - 1..], "5 ms after it, the new note alone");
}

/// A plain note, a note of layers and a sample's hand over so.
#[test]
fn a_note_taken_over_fades_out_under_the_next_on_time() {
    hands_over(|line| parse(&format!("{KICK}K: @kick v10 o4 l4 {line}\n")).unwrap(), "c", "c");
    let layered = "#tempo 125\n@k sine freq=50 pitch_start=100 pitch_decay=0.03 attack=0.001 decay=1.4 sustain=0 release=1.4 curve=exp volume=0.9\n+ triangle freq=130 attack=0.001 decay=0.6 sustain=0 release=0.5 volume=0.4\n= volume=0.9\n";
    hands_over(|line| parse(&format!("{layered}K: @k v10 o4 l4 {line}\n")).unwrap(), "c", "e");
    let tone: Vec<f32> = (0..SAMPLE_RATE as usize).map(|i| 0.8 * libm::sinf(std::f32::consts::TAU * 220.0 * i as f32 / SR)).collect();
    let samples = Samples::of(vec![Recording { name: "tone".into(), file: "samples/tone.wav".into(), hash: "tone".into(), frames: tone }]);
    let sampled = |line: &str| {
        let mut song = parse(&format!("#tempo 125\n@s sample name=tone root=a4 attack=0.001 decay=0 sustain=1 release=0.3\nA: @s o4 l4 {line}\n")).unwrap();
        song.bind(&samples).unwrap();
        song
    };
    hands_over(sampled, "a", "e");
}

/// A note that had already rung out when the next came leaves nothing:
/// the next note's samples are as if it were alone.
#[test]
fn a_note_that_had_ended_is_untouched() {
    let song = "#tempo 125\n@k sine freq=60 attack=0.001 decay=0.05 sustain=0 release=0.01\n";
    let both = left(&format!("{song}A: @k l4 c c r1\n"), 1.0);
    let alone = left(&format!("{song}A: @k l4 r c r1\n"), 1.0);
    assert_eq!(both[QUARTER..], alone[QUARTER..]);
}

/// Notes closer than the fade (each a few milliseconds, every one taking
/// over the last while it still fades): what had nowhere left to fade goes
/// on from where it was to silence, and the wave never steps far.
#[test]
fn notes_closer_than_the_fade_step_no_further() {
    // A 192nd at 400 is 3.1 ms.
    let s = left("#tempo 400\n@s sine freq=50 attack=0.001 decay=0 sustain=1 release=1 volume=0.8\nA: @s l192 [c]40 r1\n", 0.5);
    let own = 0.8 * std::f32::consts::TAU * 50.0 / SR;
    let steps = steepest(&s, 0, s.len());
    assert!(steps < 3.0 * own, "a step of {steps}, the sine's own {own}");
}

/// A note ringing on in its channel's tail since the song last went round
/// gives way, fading, to the note let go at the next seam (it was cut).
#[test]
fn a_tail_rings_out_through_the_next_seam() {
    // A bar a second, its one note's release three seconds long.
    let text = "#tempo 240\n@s sine freq=55 attack=0.01 decay=0 sustain=1 release=3 volume=0.8\nA: @s c4 r4 r4 r4\n";
    let s = left(text, 3.5);
    let bar = SAMPLE_RATE as usize;
    let (there, elsewhere) = steps_at(&s, &[bar, 2 * bar, 3 * bar]);
    assert!(there <= 1.5 * elsewhere, "a step of {there} at the seams, {elsewhere} elsewhere");
    let mut p = Player::new(parse(text).unwrap(), 1.0);
    let mut out = vec![0.0; (2 * bar + 10) * 2];
    p.mix(&mut out);
    assert!(p.tails[0].note.is_some() && p.giving[0].note.is_some(), "the tail let go of at the second seam fades out");
}

/// A channel a game laid anew under a note still sounding: the new
/// channel's note takes the voice over as a song's own does, fading the
/// old one out.
#[test]
fn a_note_taken_over_by_a_laid_channel_fades_out() {
    let studio = "#tempo 120\n@k sine freq=55 attack=0.001 decay=0 sustain=1 release=0.5 volume=0.8\n";
    let laid = |line: &str| Laid::new(vec![('K', line.to_string())]);
    let mut p = Player::new(lay(studio, &laid("@k c1")).unwrap(), 1.0);
    drop(p.pend(Some(Swap::new(lay(studio, &laid("@k c4 e4 e4 e4")).unwrap(), 1.0))));
    let mut out = vec![0.0; SAMPLE_RATE as usize * 2];
    p.mix(&mut out);
    let s: Vec<f32> = out.iter().step_by(2).copied().collect();
    let beat = SAMPLE_RATE as usize / 2;
    let (there, elsewhere) = steps_at(&s, &[beat]);
    assert!(there <= 1.5 * elsewhere, "a step of {there} at the swap, {elsewhere} elsewhere");
}

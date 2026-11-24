//! A song played faster or slower for a game's tempo (`Player::set_ratio`):
//! its time scaled, each sound's shape kept, the audio with the clock.

use super::player::*;
use super::*;
use crate::audio::SAMPLE_RATE;

/// Where the player is in the song, exactly (a fraction of a sample too).
fn place(p: &Player) -> f64 {
    p.time as f64 + p.carry
}

/// `d` samples the shorter way round a loop `length` long.
fn round_off(d: f64, length: f64) -> f64 {
    let d = d.rem_euclid(length);
    if d > length / 2.0 { d - length } else { d }
}

/// A sound keeps its shape at another tempo: a note's attack and decay
/// heard at twice the tempo are the same samples as at the song's own,
/// until its key is let go (a length of the song: sooner).
#[test]
fn a_sound_keeps_its_shape_at_another_tempo() {
    let text = "#tempo 60\n@a square attack=0.01 decay=0.3 sustain=0.2 release=0.1 vibrato=0.02 sweep=-3\n@k triangle sidechain=0.6 sidechain_release=0.4 decay=0.2 sustain=0\nA: @a c1\nB: @k c1\n";
    let render = |ratio: f64| {
        let mut p = Player::new(parse(text).unwrap(), 1.0);
        p.set_ratio(ratio);
        p.render(Some(0.5))
    };
    assert!(render(1.0) == render(2.0), "the first half second, before the key is let go at either tempo");
}

/// `the_audio_keeps_with_a_tempo_for_an_hour`'s each ratio: an hour of a
/// one-bar song at 120 a minute played `ratio` times as fast, fed a tick
/// at a time as the game's clock counts it in units; how far off the
/// clock the audio was at worst, in samples heard.
fn worst_off(milli: u32) -> f64 {
    let mut p = Player::new(parse("#tempo 120\nA: c16 r8. r2.\n").unwrap(), 1.0);
    let ratio = f64::from(milli) / 120_000.0;
    p.set_ratio(ratio);
    let tick = SAMPLE_RATE as usize / 60;
    let mut out = vec![0.0; tick * 2];
    let length = p.at_bar(2);
    let mut units = 0u64;
    let mut worst = 0.0f64;
    for _ in 0..216_000 {
        p.mix(&mut out);
        units += u64::from(milli);
        let beats = units as f64 / 3_600_000.0;
        p.follow(beats, 0.0);
        assert!(p.since_jump as usize > tick, "a jump at {ratio}x");
        worst = worst.max(round_off(p.within(p.at_beat(beats)) - place(&p), length).abs() / ratio);
    }
    worst
}

/// The audio keeps with the game's clock at any tempo: an hour at 0.25,
/// 0.8, 1.25 and 4 times the song's own, never more than a sample or two
/// off and never jumping.
#[test]
fn the_audio_keeps_with_a_tempo_for_an_hour() {
    std::thread::scope(|s| {
        let runs: Vec<_> = [30_000, 96_000, 150_000, 480_000].into_iter().map(|m| (m, s.spawn(move || worst_off(m)))).collect();
        for (milli, run) in runs {
            let worst = run.join().unwrap();
            assert!(worst <= 2.0, "{}x: the audio {worst:.2} samples off the clock", f64::from(milli) / 120_000.0);
        }
    });
}

/// Fed with a frame's jitter (each feed a little early or late, as a
/// window's frames come), a song at 4 times and at a quarter of its tempo
/// never jumps for a minute: how far off the clock it is counts in
/// samples heard, not the song's (at 4x a frame's jitter is 4 frames of
/// the song).
#[test]
fn a_song_faster_or_slower_never_jumps_on_a_frames_jitter() {
    for milli in [480_000u32, 30_000] {
        let mut p = Player::new(parse("#tempo 120\nA: c4 e4 g4 > c4\n").unwrap(), 1.0);
        let ratio = f64::from(milli) / 120_000.0;
        p.set_ratio(ratio);
        let tick = SAMPLE_RATE as i64 / 60;
        let (mut units, mut made, mut seed) = (0u64, 0i64, 0x2545_f491u32);
        for n in 1..=3600i64 {
            // Where the audio is at this feed: on time give or take a frame.
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let due = n * tick + (seed % (2 * tick as u32 + 1)) as i64 - tick;
            let frames = (due - made).max(0);
            let mut out = vec![0.0; frames as usize * 2];
            p.mix(&mut out);
            made += frames;
            units += u64::from(milli);
            p.follow(units as f64 / 3_600_000.0, 0.0);
            assert!(p.since_jump > 0 || frames == 0, "{ratio}x: a jump at feed {n}");
        }
    }
}

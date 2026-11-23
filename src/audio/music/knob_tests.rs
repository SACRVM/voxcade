//! A strip's knobs (`music:mix`'s instrument words) as the player applies
//! them: the gliding ones step without a click, the next note's leave the
//! sounding note alone, a cutoff keeps a note's sweep as deep.

use super::player::*;
use super::*;
use crate::audio::SAMPLE_RATE;

const SR: usize = SAMPLE_RATE as usize;

/// A mix whose strip `letter` turns `word` to `value` over `seconds`.
fn turned(mut mix: Mix, letter: char, word: &str, value: f32, seconds: f32) -> Mix {
    assert!(std::sync::Arc::make_mut(&mut mix.strips).entry(letter).or_default().turn(word, Some(value), seconds));
    mix
}

/// `song` played for `seconds` (the left side), its strips as `start`
/// from its first sample and, `at` seconds in, as `then`.
fn played(song: &str, seconds: f32, start: &Mix, then: Option<(f32, &Mix)>) -> Vec<f32> {
    let mut p = Player::new(parse(song).unwrap(), 1.0);
    p.start_mix(start);
    let total = (seconds * SR as f32) as usize;
    let cut = then.map_or(total, |(at, _)| (at * SR as f32) as usize);
    let mut out = vec![0.0f32; total * 2];
    p.mix(&mut out[..cut * 2]);
    if let Some((_, mix)) = then {
        p.follow_mix(mix);
        p.mix(&mut out[cut * 2..]);
    }
    out.into_iter().step_by(2).collect()
}

/// The largest step from one sample to the next within `from..to`.
fn steepest(s: &[f32], from: usize, to: usize) -> f32 {
    s[from..to].windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()))
}

/// Each gliding word turned at once while a note sounds steps little more
/// than the same note does with either value held from its start: the
/// shortest glide (5 ms, the cutoff's 20) keeps it from clicking. The
/// tones are slow (20 Hz, turned at a crest), so a step in a level shows:
/// turned without a glide, the sustain stepped some 200 times as far.
#[test]
fn a_gliding_knob_turned_at_once_does_not_click() {
    let sine = "#tempo 120\n@s sine freq=20 attack=0.005 decay=0.1 sustain=0.6 release=0.05 volume=0.8\nA: @s o4 a1\n";
    let square = "#tempo 120\n@s square attack=0.005 decay=0.1 sustain=0.6 release=0.05 volume=0.8 pwm_speed=3\nA: @s o3 a1\n";
    let filtered = "#tempo 120\n@s saw filter=lowpass cutoff=500 resonance=0.5 attack=0.005 decay=0.1 sustain=0.6 release=0.05 volume=0.5\nA: @s o2 a1\n";
    // B ducked under A's silent kick on every beat.
    let ducked = "#tempo 120\n@k sine freq=60 sidechain=0.8 sidechain_release=0.3 volume=0 decay=0.1 sustain=0\n@s sine freq=20 attack=0.005 decay=0.1 sustain=0.6 release=0.05 volume=0.8\nA: @k c4 c4 c4 c4\nB: @s o4 a1\n";
    for (song, letter, word, from, to) in [
        (square, 'A', "duty", 0.2, 0.7),
        (square, 'A', "pwm", 0.0, 0.4),
        (sine, 'A', "vibrato", 0.0, 0.05),
        (sine, 'A', "sustain", 0.2, 0.9),
        (filtered, 'A', "cutoff", 300.0, 3000.0),
        (filtered, 'A', "resonance", 0.0, 0.9),
        (ducked, 'B', "duck", 0.0, 1.0),
    ] {
        let (a, b) = (turned(Mix::default(), letter, word, from, 0.0), turned(Mix::default(), letter, word, to, 0.0));
        let at = 0.6125;
        let stepped = played(song, 1.2, &a, Some((at, &b)));
        let window = ((at - 0.1) * SR as f32) as usize..((at + 0.1) * SR as f32) as usize;
        let own = steepest(&played(song, 1.2, &a, None), window.start, window.end).max(steepest(&played(song, 1.2, &b, None), window.start, window.end));
        let step = steepest(&stepped, window.start, window.end);
        assert!(step <= own * 4.0, "{word} turned from {from} to {to} at once stepped {step}, held {own}");
        // It sounds the new value: the turned note no longer the old one.
        let late = ((at + 0.1) * SR as f32) as usize;
        assert!(stepped[late..] != played(song, 1.2, &a, None)[late..], "{word} turned changed nothing");
    }
}

/// A word from the next note turned while a note sounds leaves that note
/// alone to the sample, and the next note takes it, for every such word
/// on an instrument that has every part (a pitch that falls from a start
/// in place of a sweep for the start's words), exponential falls too.
#[test]
fn a_word_from_the_next_note_leaves_the_sounding_note_alone() {
    let swept = "#tempo 120\n@k square pwm=0.1 freq=110 sweep=-12 sweep_time=0.2 filter=lowpass cutoff=800 cutoff_end=200 cutoff_time=0.2 vibrato=0.01 vibrato_speed=5 vibrato_delay=0.05 pwm_speed=2 arp=0,12 arp_speed=20 lowpass=5000 drive=6 crush_rate=20000 crush_bits=12 sidechain=0.3 sidechain_release=0.1 attack=0.01 decay=0.2 sustain=0.5 release=0.05\n@s sine volume=0.5\nA: @k c4 c4 c4 c4\nB: @s o4 a1\n";
    let falling = swept.replace("sweep=-12 sweep_time=0.2", "pitch_start=900 pitch_decay=0.05 curve=exp");
    let next = SR / 2;
    for word in NEXT_NOTE {
        let song = if word.starts_with("pitch_") { falling.as_str() } else { swept };
        let plain = played(song, 1.0, &Mix::default(), None);
        let usage = crate::audio::words::word(word).and_then(|w| w.mml.as_ref()).unwrap();
        let (lo, hi) = usage.range.unwrap();
        let mut value = ((lo + hi) / 2.0).round() as f32;
        if word == "attack" || word == "release" || word.ends_with("time") || word.ends_with("decay") || word == "vibrato_delay" {
            value = 0.03;
        }
        let then = turned(Mix::default(), 'A', word, value, 0.0);
        let s = played(song, 1.0, &Mix::default(), Some((0.2, &then)));
        assert!(s[..next] == plain[..next], "{word} changed the sounding note at sample {:?}", s.iter().zip(&plain).position(|(a, b)| a != b));
        assert!(s[next..] != plain[next..], "{word} did not change the next note");
    }
}

/// The cutoff turned is the note's sweep's base: the note sweeps as deep
/// from it (twice the knob halfway along a sweep of two octaves up), a
/// cutoff glides in octaves, and a turned cutoff_end sweeps from where the
/// cutoff was as the note began.
#[test]
fn a_cutoff_keeps_a_notes_sweep_as_deep() {
    let song = "#tempo 120\n@b saw filter=lowpass cutoff=400 cutoff_end=1600 cutoff_time=0.2 attack=0.001 decay=0.3 sustain=0.5\nA: @b o2 c2 c2\n";
    let cut_at = |mix: &Mix, then: Option<(f32, &Mix)>, at: f32| {
        let mut p = Player::new(parse(song).unwrap(), 1.0);
        p.start_mix(mix);
        let (mut done, mut out) = (0usize, [0.0f32; 2]);
        let target = (at * SR as f32) as usize;
        while done < target {
            if let Some((t, m)) = then
                && done == (t * SR as f32) as usize
            {
                p.follow_mix(m);
            }
            p.mix(&mut out);
            done += 1;
        }
        p.voices[0].tone.filter.cut
    };
    let near = |a: f32, b: f32| (a / b - 1.0).abs() < 0.002;
    let own = cut_at(&Mix::default(), None, 0.1);
    assert!(near(own, 800.0), "the instrument's own halfway: {own}");
    let knob = turned(Mix::default(), 'A', "cutoff", 1000.0, 0.0);
    let k = cut_at(&knob, None, 0.1);
    assert!(near(k, 2000.0), "the knob's halfway: {k}");
    // Gliding from 1000 to 4000 over 0.4 s past the sweep: an octave in 0.2 s.
    let glide = turned(knob.clone(), 'A', "cutoff", 4000.0, 0.4);
    let g = cut_at(&knob, Some((0.5, &glide)), 0.7);
    assert!(near(g, 4.0 * 2000.0), "an octave into the glide, the sweep's end over it: {g}");
    // cutoff_end turned: the next note sweeps from the cutoff to it.
    let end = turned(knob, 'A', "cutoff_end", 9000.0, 0.0);
    let e = cut_at(&end, None, 1.1);
    assert!(near(e, 3000.0), "the next note halfway from 1000 to 9000: {e}");
}

/// A word the note's instrument lacks does nothing on it: a fixed pitch on
/// a pitched instrument, a cutoff without a filter, a low-pass and a
/// crusher it does not have.
#[test]
fn a_word_the_instrument_lacks_does_nothing() {
    let song = "#tempo 120\n@s square attack=0.01 decay=0.1 sustain=0.6\nA: @s c4 e4 g4 c4\n";
    let plain = played(song, 1.5, &Mix::default(), None);
    for (word, value) in [("freq", 300.0), ("cutoff", 300.0), ("resonance", 0.9), ("cutoff_end", 3000.0), ("lowpass", 400.0), ("crush_rate", 2000.0), ("crush_bits", 3.0), ("pitch_start", 300.0), ("pitch_decay", 0.1)] {
        let mix = turned(Mix::default(), 'A', word, value, 0.0);
        assert!(played(song, 1.5, &mix, None) == plain, "{word} changed an instrument without its part");
    }
}

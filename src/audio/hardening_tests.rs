//! MML refuses what it cannot play, says the line a mistake is on, and
//! plays pulses round zero; a stolen voice fades out (audit package P6).

use std::sync::Arc;

use super::music::{Player, parse};
use super::{Mixer, SAMPLE_RATE};

fn error(text: &str) -> String {
    match parse(text) {
        Ok(_) => panic!("read, but should be refused: {text:?}"),
        Err(e) => format!("{e:#}"),
    }
}

/// `#tempo 0` panicked in `voxcade music` (capacity overflow), a negative
/// one made a silent song of no length; `t0` and the like are refused the
/// same.
#[test]
fn a_tempo_that_is_no_tempo_is_refused() {
    for text in ["#tempo 0\nA: c", "#tempo -60\nA: c", "#tempo nan\nA: c", "#tempo inf\nA: c", "#tempo fast\nA: c", "#tempo 5000\nA: c", "#tempo\nA: c"] {
        assert!(error(text).contains("#tempo"), "{text}");
    }
    for text in ["A: t0 c", "A: t-5 c", "A: t5000 c"] {
        assert!(error(text).contains("t (tempo)"), "{text}");
    }
    assert!(parse("#tempo 1\nA: c").is_ok() && parse("#tempo 999\nA: c").is_ok());
}

/// Numbers outside the words' ranges were clamped or went through:
/// `volume=40` played 24 dB too loud, `duty=3` silently became 0.95.
#[test]
fn numbers_out_of_range_are_refused() {
    for (text, word) in [
        ("@x volume=40\nA: @x c", "volume=40 is out of range"),
        ("@x duty=3\nA: @x c", "duty=3 is out of range"),
        ("@x sustain=2\nA: @x c", "sustain=2"),
        ("@x attack=-1\nA: @x c", "attack=-1"),
        ("@x attack=1000\nA: @x c", "attack=1000"),
        ("@x freq=0\nA: @x c", "freq=0"),
        ("@x vibrato_speed=900\nA: @x c", "vibrato_speed=900"),
        ("@x sweep=nan\nA: @x c", "no number"),
        ("@x arp=0,x,7\nA: @x c", "arp step 'x'"),
        ("A: l0 c", "l (the default note length)"),
        ("A: c0", "note length of 0"),
        ("A: c500", "note length of 500"),
        ("A: o12 c", "octave 12"),
        ("A: > > > > > > c", "octave 10"),
        ("A: < < < < < c", "octave -1"),
        ("A: v99 c", "v (volume)"),
        ("A: q0 c", "q (how much"),
        ("A: p500 c", "p (pan)"),
        ("A: [c]100000", "repeat of 100000"),
        ("A: [c]0", "repeat of 0"),
    ] {
        let message = error(text);
        assert!(message.contains(word), "{text}: {message}");
    }
    // The edges stay good.
    assert!(parse("@x volume=1 duty=0.95 sustain=0 attack=0\nA: @x o0 c192 o9 v15 q8 p-100 [c]999").is_ok());
}

/// Repeats inside repeats and macros that grow multiply; they stop with a
/// mistake instead of eating the memory or the time.
#[test]
fn a_song_cannot_grow_without_bound() {
    assert!(error("A: [[[[r]999]999]999]999").contains("too much"));
    assert!(error("A: [[[c]999]999]999").contains("200000 notes"));
    let mut text = String::from("$a = c c c c c c c c\n");
    for (i, prev) in ["a", "b", "c", "d", "e", "f", "g"].iter().enumerate() {
        let name = (b'b' + i as u8) as char;
        text += &format!("${name} = ${prev} ${prev} ${prev} ${prev} ${prev} ${prev} ${prev} ${prev}\n");
    }
    text += "A: $h\n";
    assert!(error(&text).contains("macros grow"));
}

/// A slip of a letter finds the word, as in a sound file.
#[test]
fn an_option_near_a_word_is_suggested() {
    assert!(error("@x atack=0.1\nA: c").contains("did you mean 'attack'"));
    assert!(error("@x sustian=0.5\nA: c").contains("did you mean 'sustain'"));
    assert!(!error("@x zzzzzz=1\nA: c").contains("did you mean"));
}

/// The line of a mistake is its own, not the line the channel began on:
/// channels continue over lines and macros come from elsewhere.
#[test]
fn a_mistake_names_its_own_line() {
    let text = "#tempo 120\nA: c d e\nB: c\nA: f g\nA: a x b\n";
    assert!(error(text).contains("line 5: channel A"), "{}", error(text));
    // A mistake on the second line of a channel that began on line 2.
    let text = "A: c d\n; a comment\nA: e q\n";
    assert!(error(text).contains("line 3"), "{}", error(text));
    // In a macro: the line that uses it.
    let text = "$bad = c x\n\nA: c\nA: d $bad\n";
    assert!(error(text).contains("line 4"), "{}", error(text));
    assert!(error("A: c\nA: $nope\n").contains("line 2: no macro $nope"));
    // An unclosed repeat is on the last line of the channel.
    assert!(error("A: c [d\nA: e\n").contains("line 2"), "{}", error("A: c [d\nA: e\n"));
}

/// A pulse off duty 0.5 swings round zero, as a sound's does (`synth`): no
/// DC offset to thump and eat headroom. Without it a duty of 0.1 sat at
/// -0.8 of its swing.
#[test]
fn a_pulse_off_duty_half_has_no_dc() {
    for duty in ["0.1", "0.25", "0.5", "0.75", "0.9"] {
        let text = format!("#tempo 120\n@x square duty={duty} attack=0 decay=0 sustain=1 release=0.01 volume=1\nA: @x v15 o4 c1");
        let out = Player::new(parse(&text).unwrap(), 1.0).render(Some(1.0));
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let part = &left[2000..left.len() - 2000];
        let mean = part.iter().sum::<f32>() / part.len() as f32;
        let peak = part.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(mean.abs() < 0.02 * peak, "duty {duty}: mean {mean} of a peak {peak}");
    }
}

/// A voice stolen for a new sound (a machine gun's fifth shot) fades out
/// over 5 ms instead of stopping on a sample.
#[test]
fn a_stolen_voice_fades_out_instead_of_cutting() {
    let mut mixer = Mixer::default();
    let long: Arc<[f32]> = Arc::from(vec![0.5f32; SAMPLE_RATE as usize]);
    for _ in 0..super::MAX_SAME {
        mixer.play("gun", &long, 1.0, 0.0);
    }
    mixer.play("gun", &long, 1.0, 0.0);
    // The stolen one is still there, going out; it never counts again.
    assert_eq!(mixer.voices.len(), super::MAX_SAME + 1);
    assert_eq!(mixer.voices.iter().filter(|v| v.fade.is_some()).count(), 1);
    let mut block = vec![0.0; 2 * 1024];
    mixer.mix(&mut block);
    assert_eq!(mixer.voices.len(), super::MAX_SAME, "gone after its fade");
    // A further steal while one fades takes a live voice, not the fading one.
    mixer.play("gun", &long, 1.0, 0.0);
    mixer.play("gun", &long, 1.0, 0.0);
    assert_eq!(mixer.voices.iter().filter(|v| v.fade.is_none()).count(), super::MAX_SAME);
}

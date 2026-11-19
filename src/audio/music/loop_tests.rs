//! `#loop`: the bars that go round, and the audio keeping time round them.

use super::player::*;
use super::*;
use crate::audio::SAMPLE_RATE;

/// The note channel 0 plays after each of `frames` steps of `chunk`
/// samples: its pitch in semitones from middle c, None silent.
fn notes_played(p: &mut Player, frames: usize, chunk: usize) -> Vec<Option<i32>> {
    let mut out = vec![0.0; chunk * 2];
    (0..frames)
        .map(|_| {
            p.mix(&mut out);
            p.voices[0].note.and_then(|i| p.song.channels[0][i].freq).map(|f| (12.0 * (f / 261.6256).log2()).round() as i32)
        })
        .collect()
}

/// `#loop FROM TO` plays the song from its start once, then goes round
/// bars FROM to TO (both counted from 1) for good, what lies after TO
/// never reached; `#loop FROM` goes round to the song's end; `#loop 1`
/// is the whole song, as without it, sample for sample.
#[test]
fn a_loop_goes_round_its_bars() {
    // A bar a second at 240 beats a minute: c, d, e, f, g.
    let five = "@a square attack=0 decay=0 sustain=1 release=0.001\nA: @a o4 c1 d1 e1 f1 g1\n";
    let bars = |text: &str, seconds: usize| -> Vec<Option<i32>> {
        let mut p = Player::new(parse(text).unwrap(), 1.0);
        // The middle of every bar.
        let half = SAMPLE_RATE as usize / 2;
        notes_played(&mut p, seconds * 2, half).into_iter().step_by(2).collect()
    };
    let (c, d, e, f, g) = (Some(0), Some(2), Some(4), Some(5), Some(7));
    assert_eq!(bars(&format!("#tempo 240\n#loop 2 3\n{five}"), 8), [c, d, e, d, e, d, e, d]);
    assert_eq!(bars(&format!("#tempo 240\n#loop 3\n{five}"), 9), [c, d, e, f, g, e, f, g, e]);
    assert_eq!(bars(&format!("#tempo 240\n#loop 5 5\n{five}"), 7), [c, d, e, f, g, g, g]);
    assert_eq!(bars(&format!("#tempo 240\n{five}"), 7), [c, d, e, f, g, c, d]);
    let whole = Player::new(parse(&format!("#tempo 240\n{five}")).unwrap(), 1.0).render(Some(12.0));
    assert!(whole == Player::new(parse(&format!("#tempo 240\n#loop 1\n{five}")).unwrap(), 1.0).render(Some(12.0)), "#loop 1 is the whole song");
    assert_eq!(parse(&format!("#tempo 240\n#loop 1 1\n{five}")).unwrap().round, Some((1, Some(1))));
    // `#loop no` still plays once, `#loop yes` (or bare) goes round whole.
    for (word, looping) in [("no", false), ("off", false), ("yes", true), ("", true)] {
        let song = parse(&format!("#loop {word}\n{five}")).unwrap();
        assert_eq!((song.looping, song.round), (looping, None), "#loop {word}");
    }
}

/// Rendered without a length (`voxcade music`), a song with `#loop` is
/// written to its loop's end and round into its first bars again, so the
/// seam lies in the file; one without is written once, as ever.
#[test]
fn a_loop_is_rendered_round_its_seam() {
    let five = "#tempo 240\n@a square attack=0 decay=0 sustain=1 release=0.001\nA: @a o4 c1 d1 e1 f1 g1\n";
    let frames = |text: &str| Player::new(parse(text).unwrap(), 1.0).render(None).len() / 2;
    let bar = SAMPLE_RATE as usize;
    assert_eq!(frames(five), 5 * bar);
    // To the end of bar 3, then bars 2 and 3 again (the loop is shorter than four).
    assert_eq!(frames(&five.replace("#tempo 240\n", "#tempo 240\n#loop 2 3\n")), 3 * bar - 2 + 2 * bar);
    assert_eq!(frames(&five.replace("#tempo 240\n", "#tempo 240\n#loop 1\n")), 5 * bar + 4 * bar);
}

/// A loop's ends lie on its bar lines however the notes' lengths add up
/// (a sum of eighths at 137 beats a minute is a sample off the line now
/// and then): at the jump the loop's first note is taken up at once, and
/// the note after its last bar never sounds.
#[test]
fn a_loop_ends_on_its_bar_lines() {
    for tempo in [137, 140, 99, 173] {
        let song = parse(&format!("#tempo {tempo}\n#meter 3\n#loop 3 7\nA: [c8 d8 e8 f8 g8 a8]9\n")).unwrap();
        let notes = &song.channels[0];
        // Six eighths a bar of three: bar 3 begins with note 12, bar 8 with note 42.
        assert!(notes[11].start < song.span.0 && song.span.0 <= notes[12].start, "{tempo}: from {} in {}..{}", song.span.0, notes[11].start, notes[12].start);
        assert!(notes[41].start < song.span.1 && song.span.1 <= notes[42].start, "{tempo}: to {} in {}..{}", song.span.1, notes[41].start, notes[42].start);
        assert_eq!(song.bars, 9);
    }
}

/// Mistakes in `#loop` name its line: bars past the song, the first after
/// the last, not whole numbers, 0, a word, given twice.
#[test]
fn a_loops_mistakes_say_their_line() {
    let song = |head: &str| parse(&format!("#tempo 240\n{head}\nA: c1 d1 e1 f1\n")).map(|_| ()).map_err(|e| format!("{e:#}"));
    for (head, says) in [
        ("#loop 5", "line 2: #loop: bar 5 is past the song's end (it has 4 bars)"),
        ("#loop 2 9", "line 2: #loop: bar 9 is past the song's end (it has 4 bars)"),
        ("#loop 3 2", "line 2: #loop from bar 3 to bar 2: the first bar comes first (like #loop 2 3)"),
        ("#loop 2.5 3", "line 2: #loop is the bars that go round, whole numbers from 1"),
        ("#loop -1", "line 2: #loop is the bars that go round, whole numbers from 1"),
        ("#loop 0 3", "line 2: #loop: bars count from 1 (the song's first), not 0"),
        ("#loop forever", "line 2: #loop is the bars that go round"),
        ("#loop 1 2 3", "line 2: #loop is the bars that go round"),
        ("#loop 1 2\n#loop 2", "line 3: #loop is given twice"),
        ("#loop no\n#loop 2", "line 3: #loop is given twice"),
    ] {
        let e = song(head).expect_err(head);
        assert!(e.contains(says), "{head}: {e}");
    }
    assert!(song("#loop 4 4").is_ok() && song("#loop 1 4").is_ok());
}

/// The audio keeps time with a game's clock across the loop's seam, as
/// the window feeds it a tick at a time (`Session::feed_music`: the
/// beats since the song began, on and on, which the player folds into
/// the song): it goes round with the clock without a jump, and a clock
/// set back (a snapshot brought back) or far on is met where it is.
#[test]
fn the_audio_keeps_with_the_clock_round_a_loop() {
    let text = "#tempo 150\n#loop 2 3\n@a square attack=0 decay=0 sustain=1 release=0.001\nA: @a [c4 d4 e4 f4]5\n";
    let mut p = Player::new(parse(text).unwrap(), 1.0);
    let tick = SAMPLE_RATE as usize / 60;
    let mut out = vec![0.0; tick * 2];
    let target = |p: &Player, steps: u64| p.at_beat(steps as f64 * 150.0 / 3600.0);
    // How far the audio is from the clock, the shorter way round the loop.
    let off = |p: &Player, at: u64| {
        let (from, to) = p.song.span;
        let d = p.within(at) as i64 - p.time as i64;
        let l = (to - from) as i64;
        if d > l / 2 {
            d - l
        } else if d < -l / 2 {
            d + l
        } else {
            d
        }
    };
    // 12 seconds: the loop (bars 2 and 3, 3.2 s) goes round three times.
    for steps in 1..=720 {
        p.mix(&mut out);
        p.follow(target(&p, steps));
        assert!(p.since_jump as usize > tick, "a jump at step {steps}");
        assert!(off(&p, target(&p, steps)).abs() <= 2, "step {steps}: {} off", off(&p, target(&p, steps)));
    }
    assert!(p.time >= p.song.span.0 && p.time < p.song.span.1, "round the loop");
    // Set back into the first bar (a snapshot from before the loop): met there.
    p.follow(target(&p, 30));
    assert_eq!(p.time, target(&p, 30));
    // Far on: where the song goes round to.
    let far = target(&p, 7000);
    p.follow(far);
    assert_eq!(p.time, p.within(far));
    assert!(p.time >= p.song.span.0 && p.time < p.song.span.1);
}

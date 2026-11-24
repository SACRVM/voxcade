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
            p.voices[0].note.and_then(|n| n.freq).map(|f| (12.0 * (f / 261.6256).log2()).round() as i32)
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
        let at = |i: usize| notes[i].start as f64;
        assert!(at(11) < song.span.0 && song.span.0 <= at(12), "{tempo}: from {} in {}..{}", song.span.0, at(11), at(12));
        assert!(at(41) < song.span.1 && song.span.1 <= at(42), "{tempo}: to {} in {}..{}", song.span.1, at(41), at(42));
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

/// Where the player is in the song, exactly (a fraction of a sample too).
fn place(p: &Player) -> f64 {
    p.time as f64 + p.carry
}

/// `d` samples the shorter way round a loop `length` long.
fn round_off(d: f64, length: f64) -> f64 {
    let d = d.rem_euclid(length);
    if d > length / 2.0 { d - length } else { d }
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
    let beats = |steps: u64| steps as f64 * 150.0 / 3600.0;
    // How far the audio is from the clock, the shorter way round the loop.
    let off = |p: &Player, beats: f64| round_off(p.within(p.at_beat(beats)) - place(p), p.song.span.1 - p.song.span.0);
    // 12 seconds: the loop (bars 2 and 3, 3.2 s) goes round three times.
    for steps in 1..=720 {
        p.mix(&mut out);
        p.follow(beats(steps), 0.0);
        assert!(p.since_jump as usize > tick, "a jump at step {steps}");
        assert!(off(&p, beats(steps)).abs() <= 2.0, "step {steps}: {} off", off(&p, beats(steps)));
    }
    assert!(place(&p) >= p.song.span.0 && place(&p) < p.song.span.1, "round the loop");
    // Set back into the first bar (a snapshot from before the loop): met there.
    p.follow(beats(30), 0.0);
    assert_eq!(place(&p), p.at_beat(beats(30)));
    // Far on: where the song goes round to.
    p.follow(beats(7000), 0.0);
    assert!((place(&p) - p.within(p.at_beat(beats(7000)))).abs() < 1e-6);
    assert!(place(&p) >= p.song.span.0 && place(&p) < p.song.span.1);
}

/// A loop goes round exactly as long as its bars, though a bar is seldom
/// a whole number of samples: fed a tick at a time, the audio stays
/// within a sample of where the game's clock is in the bar, a one-bar
/// song going round 5,000 times at 124 and 128 beats a minute (2.7 and
/// 2.6 hours; folded in whole samples it ran ahead 0.84 and 0.5 samples
/// a round, 35 and 22 ms an hour), and `#loop`'s bars too: later ones,
/// the song's first ones (the loop beginning a little before its start;
/// it ran ahead 2.84 samples a round) and from a bar to the song's end.
#[test]
fn a_short_loop_never_drifts_from_the_clock() {
    let bar = "c16 r8. r2.";
    // Hours of sound: each on a thread of its own.
    let cases = [
        (124, "", bar.to_string(), 5000),
        (128, "", bar.to_string(), 5000),
        (124, "#loop 2 2\n", format!("{bar} {bar} {bar}"), 500),
        (124, "#loop 1 1\n", format!("{bar} {bar}"), 500),
        (124, "#loop 2\n", format!("{bar} {bar}"), 500),
    ];
    std::thread::scope(|s| {
        for case in &cases {
            s.spawn(|| keeps_with_the_clock(case));
        }
    });
}

/// `a_short_loop_never_drifts_from_the_clock`'s each song: at `tempo`,
/// `head` its `#loop`, going round `rounds` times.
fn keeps_with_the_clock(&(tempo, head, ref notes, rounds): &(u32, &str, String, u32)) {
    let mut p = Player::new(parse(&format!("#tempo {tempo}\n{head}A: {notes}\n")).unwrap(), 1.0);
    // The loop as the game's clock counts it: a bar, from the start or bar 2.
    let length = 4.0 * 60.0 / f64::from(tempo) * SAMPLE_RATE as f64;
    let from = if head.starts_with("#loop 2") { length } else { 0.0 };
    let tick = SAMPLE_RATE as u64 / 60;
    let mut out = vec![0.0; tick as usize * 2];
    let steps = ((from + rounds as f64 * length) / tick as f64) as u64;
    let mut worst = 0.0f64;
    for step in 1..=steps {
        p.mix(&mut out);
        p.follow(step as f64 * f64::from(tempo) / 3600.0, 0.0);
        let clock = (step * tick) as f64;
        let off = if clock < from + length { place(&p) - clock } else { round_off(place(&p) - clock, length) };
        worst = worst.max(off.abs());
    }
    assert!(worst <= 1.0, "{tempo} {head:?}: the audio {worst:.2} samples off the clock");
}

/// A note that sounds where the song goes round is let go there, its
/// release ringing on beside the channel's next round, which begins on
/// time: no click at the seam, whose largest step from one sample to the
/// next is no larger than where the same note ends by its envelope within
/// a song (it was cut dead there). So at the song's end, at a `#loop`'s
/// (a note held over it), and at the end of a song that plays once, which
/// is over once its release has rung out.
#[test]
fn a_note_rings_over_the_seam() {
    let head = "#tempo 240\n@a sine attack=0.005 decay=0 sustain=1 release=0.05\n";
    let bar = SAMPLE_RATE as usize;
    let left = |text: &str, seconds: f32| -> Vec<f32> { Player::new(parse(&format!("{head}{text}")).unwrap(), 1.0).render(Some(seconds)).into_iter().step_by(2).collect() };
    let jump = |s: &[f32], at: usize| s[at - 500..at + 500].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let rms = |s: &[f32], from: usize, to: usize| (s[from..to].iter().map(|x| x * x).sum::<f32>() / (to - from) as f32).sqrt();
    // Within a song: held to its end (q8) and let go there, a rest after.
    let (c, e) = (left("A: @a q8 o4 c1 r1\n", 2.0), left("A: @a q8 o4 e1 r1\n", 2.0));
    for (text, seam, within) in [
        ("A: @a q8 o4 c1\n", bar, jump(&c, bar)),
        ("#loop 2 2\nA: @a q8 o4 c1 e1&e1 g1\n", 2 * bar - 2, jump(&e, bar)),
        ("#loop no\nA: @a q8 o4 c1\n", bar, jump(&c, bar)),
    ] {
        let s = left(text, 2.5);
        assert!(jump(&s, seam) <= 2.5 * within, "{text:?}: a step of {} at the seam, {within} within a song", jump(&s, seam));
        // The release rings on past the seam.
        assert!(rms(&s, seam + 1000, seam + 1500) > 0.1 * rms(&s, seam - 1500, seam - 1000), "{text:?}: no release after the seam");
    }
    // Going round, the next round begins on time beside the note ringing on.
    let mut p = Player::new(parse(&format!("{head}A: @a q8 o4 c1\n")).unwrap(), 1.0);
    let mut out = vec![0.0; (bar + 10) * 2];
    p.mix(&mut out);
    assert_eq!((p.voices[0].note.map(|n| n.start), p.tails[0].note.map(|n| n.start)), (Some(0), Some(0)));
    // A song played once is over when its last note has rung out.
    let mut once = Player::new(parse(&format!("{head}#loop no\nA: @a q8 o4 c1\n")).unwrap(), 1.0);
    let mut out = vec![0.0; (bar + 1000) * 2];
    once.mix(&mut out);
    assert!(!once.over(), "over while its release rings");
    once.mix(&mut out);
    assert!(once.over());
}

use super::parse::note_freq;
use super::player::*;
use super::*;
use crate::audio::SAMPLE_RATE;

/// A channel in a layer sounds while the tension is at its threshold or
/// more, and fades in or out over a bar when it crosses it.
#[test]
fn layers_follow_the_tension() {
    let text = "#tempo 120\n#layer B 0.5\n@a square duty=0.5 attack=0 decay=0 sustain=1 release=0.01\nA: c1 c1 c1 c1\nB: g1 g1 g1 g1\n";
    let loudness = |intensity: f32| {
        let mut p = Player::new(parse(text).unwrap(), 1.0);
        p.start_at(intensity);
        let mut out = vec![0.0; SAMPLE_RATE as usize * 2];
        p.mix(&mut out);
        out.iter().map(|s| s * s).sum::<f32>()
    };
    let (calm, tense) = (loudness(0.2), loudness(0.8));
    assert!(tense > calm * 1.5, "the layer adds to it: {calm} {tense}");
    // Crossing the threshold, it comes in over a bar (2 seconds).
    let mut p = Player::new(parse(text).unwrap(), 1.0);
    p.start_at(0.0);
    p.intensity = 1.0;
    let mut out = vec![0.0; SAMPLE_RATE as usize * 2];
    p.mix(&mut out);
    assert!((p.loud[1] - 0.5).abs() < 0.01, "half way after a second: {}", p.loud[1]);
    assert_eq!(p.loud[0], 1.0, "a channel in no layer always sounds");
    assert!(parse("#layer Z 0.5\nA: c\n").unwrap_err().to_string().contains("channel Z"));
    assert!(parse("#layer A 2\nA: c\n").is_err() && parse("#layer 0.5\nA: c\n").is_err());
}

/// A jump into a held note takes it up there (no silence until the
/// next), faded in; keeping time catches a small lag by going a hair
/// faster and jumps over a big one.
#[test]
fn a_song_jumps_and_keeps_time() {
    let song = parse("#tempo 120\n@a square duty=0.5 attack=0 decay=0 sustain=1 release=0.01\nA: c1 d1\n").unwrap();
    assert_eq!((song.tempo, song.meter), (120.0, 4));
    let mut p = Player::new(song, 1.0);
    let second = SAMPLE_RATE as u64;
    assert_eq!(p.at_beat(2.0), second, "two beats at 120 a minute");
    p.seek(second);
    let mut out = vec![0.0; 960 * 2];
    p.mix(&mut out);
    assert!(out[200..].iter().any(|s| s.abs() > 0.1), "the held note sounds after the jump");
    assert!(out[0].abs() < out[400].abs() + 1e-6, "faded in");
    // A little behind: a hair faster, until it is with the target.
    let target = p.time + 600;
    p.follow(target);
    assert!(p.rate > 1.0 && p.rate <= 1.003);
    // Far off: a jump.
    p.follow(3 * second);
    assert_eq!((p.time, p.rate), (3 * second, 1.0));
    assert!(parse("#meter 3/4\nA: c\n").unwrap().meter == 3 && parse("#meter x\nA: c\n").is_err());
}

const SR: f32 = SAMPLE_RATE as f32;

#[test]
fn notes_lengths_and_octaves() {
    let song = parse("#tempo 120\nA: o4 a4 > c8. < a+ r2 e-16").unwrap();
    let notes = &song.channels[0];
    assert!((notes[0].freq.unwrap() - 440.0).abs() < 0.01);
    assert!((notes[1].freq.unwrap() - note_freq(5, 0)).abs() < 0.01);
    assert!((notes[2].freq.unwrap() - note_freq(4, 10)).abs() < 0.01);
    // A quarter at 120 is half a second; a dotted eighth three eighths of one.
    assert_eq!(notes[1].start, (0.5 * SR) as u64);
    assert_eq!(notes[2].start, ((0.5 + 0.375) * SR) as u64);
    // After the default-length a+ (a quarter) and a half rest.
    assert_eq!(notes[3].start, ((0.875 + 0.5 + 1.0) * SR) as u64);
    assert!((notes[3].freq.unwrap() - note_freq(4, 3)).abs() < 0.01);
}

#[test]
fn repeats_ties_and_macros() {
    let song = parse("#tempo 60\n$two = c d\nA: [$two]3 e4& |\nA: e4 f").unwrap();
    let notes = &song.channels[0];
    let pitches: Vec<i32> = notes.iter().filter_map(|n| n.freq).map(|f| (12.0 * (f / 261.6256).log2()).round() as i32).collect();
    assert_eq!(pitches, vec![0, 2, 0, 2, 0, 2, 4, 5]);
    // The tied e lasts two beats: f starts at 6 + 2 seconds.
    assert_eq!(notes[7].start, (8.0 * SR) as u64);
}

/// Notes in capitals are the notes in small letters; two sharps or flats
/// make a double sharp or flat (`c++` is d, `e--` is d); a tie joins a
/// note to one of the same pitch or a length, and refuses another pitch
/// (`c4&d4` dropped the d and played a long c, without a word).
#[test]
fn capitals_double_signs_and_ties_to_another_pitch() {
    let pitches = |text: &str| -> Vec<u32> { parse(text).unwrap().channels[0].iter().filter_map(|n| n.freq).map(|f| f.round() as u32).collect() };
    assert_eq!(pitches("A: C D E F+ B- c"), pitches("A: c d e f+ b- c"));
    assert_eq!(pitches("A: c++ e-- d b+ > c"), pitches("A: d d d > c c"));
    assert!(format!("{:#}", parse("A: c+++").unwrap_err()).contains("two sharps or two flats at most"));
    let song = parse("#tempo 60\nA: c4&C8 e").unwrap();
    assert_eq!(song.channels[0][1].start, (1.5 * SR) as u64, "a tie in capitals holds the same note");
    for text in ["A: c4&d4", "A: c4&c+4", "A: c4& >c4"] {
        let e = format!("{:#}", parse(text).unwrap_err());
        assert!(e.contains("& ties a note to one of the same pitch"), "{text}: {e}");
    }
    let e = format!("{:#}", parse("A: c d X").unwrap_err());
    assert!(e.contains("unknown command 'X'"), "{e}");
}

/// A song that plays once is over at its end; one that loops never.
#[test]
fn a_song_that_plays_once_ends() {
    for (text, ends) in [("#tempo 240\n#loop no\nA: c4 d4", true), ("#tempo 240\nA: c4 d4", false)] {
        let mut player = Player::new(parse(text).unwrap(), 1.0);
        let mut out = vec![0.0; 2 * SAMPLE_RATE as usize];
        assert!(!player.over(), "not before it played");
        player.mix(&mut out);
        assert_eq!(player.over(), ends, "{text}");
    }
}

#[test]
fn mistakes_say_where() {
    let err = format!("{:#}", parse("A: c d x").unwrap_err());
    assert!(err.contains("unknown command 'x'"), "{err}");
    let err = format!("{:#}", parse("A: @nope c").unwrap_err());
    assert!(err.contains("no instrument @nope"), "{err}");
    assert!(parse("A: [c d").is_err());
}

/// Every song parses, the games' and the library's (`music/`), and its
/// channels are equally long (or the short ones fall silent before it
/// starts over).
#[test]
fn the_songs_line_up() {
    fn songs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                songs(&path, out);
            } else if path.extension().is_some_and(|e| e == "mml") {
                out.push(path);
            }
        }
    }
    let mut all = Vec::new();
    songs("games".as_ref(), &mut all);
    songs("music".as_ref(), &mut all);
    assert!(all.len() >= 7, "only {} songs found", all.len());
    for path in all {
        let song = parse(&std::fs::read_to_string(&path).unwrap()).unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
        let total = song.length as f32 / SR;
        for (channel, length) in song.channel_lengths() {
            assert!((length - total).abs() < 0.01, "{}: channel {channel} is {length:.2}s of {total:.2}s", path.display());
        }
    }
}

/// Mono samples of a song (the left channel).
fn play(text: &str, seconds: f32) -> Vec<f32> {
    let out = Player::new(parse(text).unwrap(), 1.0).render(Some(seconds));
    out.iter().step_by(2).copied().collect()
}

/// Root mean square of `samples` between two times.
fn rms(samples: &[f32], from: f32, to: f32) -> f32 {
    let part = &samples[(from * SR) as usize..(to * SR) as usize];
    (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt()
}

/// How much of the sound is high, against its loudness: the bends from
/// sample to sample (second differences weigh highs by their frequency
/// squared) over the level.
fn edge(samples: &[f32], from: f32, to: f32) -> f32 {
    let part = &samples[(from * SR) as usize..(to * SR) as usize];
    let bends = part.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).sum::<f32>() / part.len() as f32;
    bends / rms(samples, from, to)
}

/// The SID filter: a low-pass darkens a bright saw, resonance lifts a
/// tone at the cutoff, a high-pass takes out a low one, and a swept
/// cutoff closes on every note.
#[test]
fn the_filter_cuts_resonates_and_sweeps() {
    let plain = play("@x saw sustain=1\nA: @x o6 c1", 0.5);
    let dark = play("@x saw sustain=1 cutoff=300\nA: @x o6 c1", 0.5);
    assert!(rms(&dark, 0.1, 0.4) < rms(&plain, 0.1, 0.4) * 0.3, "the low-pass does not cut");

    // A sine right at the cutoff: about -3 dB plain, far over it ringing.
    let flat = play("@x sine sustain=1 cutoff=880\nA: @x o5 a1", 0.5);
    let ringing = play("@x sine sustain=1 cutoff=880 resonance=0.85\nA: @x o5 a1", 0.5);
    assert!(rms(&ringing, 0.1, 0.4) > rms(&flat, 0.1, 0.4) * 2.5, "resonance does not lift the cutoff");
    assert!(ringing.iter().all(|s| s.abs() <= 2.0), "resonance spikes instead of overdriving");

    let low = play("@x sine sustain=1\nA: @x o2 c1", 0.5);
    let cut = play("@x sine sustain=1 filter=highpass cutoff=2000\nA: @x o2 c1", 0.5);
    assert!(rms(&cut, 0.1, 0.4) < rms(&low, 0.1, 0.4) * 0.05, "the high-pass lets the bass through");

    let swept = play("@x saw sustain=1 cutoff=6000 cutoff_end=200 cutoff_time=0.1\nA: @x o4 c1", 0.5);
    assert!(edge(&swept, 0.0, 0.03) > edge(&swept, 0.2, 0.4) * 3.0, "the cutoff does not close");
}

/// A sidechain note ducks the other channels at once and lets them come
/// back over its release; a channel with duck=0 is left alone.
#[test]
fn the_kick_ducks_the_rest() {
    // A silent kick on the second beat (0.5 s at 120) under a held tone.
    let song = "@pad sine sustain=1 {duck}\n@kick triangle freq=60 decay=0.05 sustain=0 sidechain=0.8 sidechain_release=0.3 volume=0\n#tempo 120\nA: @pad o4 a1\nD: @kick r4 c4 r2";
    let pumped = play(&song.replace("{duck}", ""), 1.0);
    let before = rms(&pumped, 0.3, 0.45);
    assert!(rms(&pumped, 0.51, 0.53) < before * 0.35, "the kick does not duck the pad");
    assert!(rms(&pumped, 0.85, 0.95) > before * 0.95, "the pad does not come back");
    let steady = play(&song.replace("{duck}", "duck=0"), 1.0);
    assert!(rms(&steady, 0.51, 0.53) > before * 0.95, "duck=0 still ducks");
}

/// Every voice of More Blood's theme sounds, and the song loops.
#[test]
fn the_theme_plays() {
    let text = std::fs::read_to_string("games/moreblood/music/theme.mml").unwrap();
    let song = parse(&text).unwrap();
    assert!(song.channels.len() >= 3);
    let length = song.length;
    let mut player = Player::new(song, 1.0);
    let mut out = vec![0.0; (length as usize + 2000) * 2];
    player.mix(&mut out);
    let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
    assert!(rms > 0.03, "the theme is nearly silent ({rms})");
    assert!(player.time < 4000, "the song did not start over");
}

/// A channel that names no instrument plays the plain one, as the module
/// says: the same samples as a square of every default named.
#[test]
fn a_channel_without_an_instrument_plays_the_plain_square() {
    let render = |text: &str| Player::new(parse(text).unwrap(), 1.0).render(None);
    let plain = render("#tempo 120\nA: o4 c4 d4 e4 f4\n");
    let named = render("#tempo 120\n@plain square\nA: @plain o4 c4 d4 e4 f4\n");
    assert!(plain.iter().any(|s| s.abs() > 0.01), "it sounds");
    assert_eq!(plain, named);
}

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
    assert_eq!(p.at_beat(2.0), second as f64, "two beats at 120 a minute");
    p.seek(p.at_beat(2.0));
    let mut out = vec![0.0; 960 * 2];
    p.mix(&mut out);
    assert!(out[200..].iter().any(|s| s.abs() > 0.1), "the held note sounds after the jump");
    assert!(out[0].abs() < out[400].abs() + 1e-6, "faded in");
    // A little behind (600 samples; the clock in beats, two a second): a
    // hair faster, until it is with the target.
    p.follow((p.time + 600) as f64 / SAMPLE_RATE as f64 * 2.0);
    assert!(p.rate > 1.0 && p.rate <= 1.003);
    // Far off: a jump.
    p.follow(6.0);
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

/// A hash of samples rounded to 12 bits (as `audio::digest_tests`).
fn digest(samples: &[f32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for s in samples {
        for b in ((s * 2048.0).round() as i16).to_le_bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// The samples and their hash a song of every instrument word but the
/// voice's, drive and the crusher made before those came.
/// Renewed once when the music went through `libm`: all but two samples within
/// 1e-5 of before, 1248 of 320,726 one 12-bit step off, a pulse edge a sample
/// earlier in two.
const PINNED: (usize, u64) = (320_726, 0x10cb_940d_f3d7_a5d3);

/// Every instrument word but the voice's, drive and the crusher plays as
/// before they came: the song pinned by the samples it made then.
#[test]
fn the_instruments_play_as_pinned() {
    let song = "#tempo 132\n#layer C 0.4\n\
        @lead square duty=0.25 pwm=0.2 pwm_speed=3 attack=0.01 decay=0.1 sustain=0.6 release=0.1 vibrato=0.01 vibrato_delay=0.1\n\
        @chord saw arp=0,4,7 arp_speed=40 lowpass=3000 volume=0.3\n\
        @acid square cutoff=3000 cutoff_end=400 cutoff_time=0.1 resonance=0.8 volume=0.3\n\
        @kick triangle freq=150 sweep=-24 sweep_time=0.06 decay=0.15 sustain=0 sidechain=0.6 sidechain_release=0.2\n\
        @hat noise freq=12000 decay=0.03 sustain=0 filter=highpass cutoff=5000 duck=0\n\
        @pad sine filter=bandpass cutoff=900 resonance=0.3 attack=0.2 release=0.4\n\
        A: @lead o5 l8 c d e g a g e d | c4 e4 g4 >c4<\nB: @chord o4 l2 c a- f g\nC: @acid o2 l16 [c c >c< c d+ c >c< c]4\n\
        D: @kick l4 [c c c c]2\nE: @hat l8 [r c]8\nF: @pad o3 p-50 l1 c e\n";
    let samples = Player::new(parse(song).unwrap(), 1.0).render(None);
    assert_eq!((samples.len(), digest(&samples)), PINNED, "the instruments no longer play as they did");
    // Written out at its default, drive changes nothing.
    let quiet = song.replace("volume=0.3\n", "volume=0.3 drive=0\n");
    assert_eq!(digest(&Player::new(parse(&quiet).unwrap(), 1.0).render(None)), PINNED.1);
}

/// The voice `voice` as it sang when `singer` came: a song of every
/// phoneme it had then, plain, driven and crushed, pinned by its samples.
/// Renewed once when the music went through `libm`: largest difference 1.8e-7,
/// rms 1.6e-8, 8 of 756,000 samples one 12-bit step off.
const VOICE_PINNED: (usize, u64) = (756_000, 0xc2b4_af99_c0f0_e469);

#[test]
fn the_voice_sings_as_pinned() {
    let all = "a ae e eh i ih o u ax oe ue aw ow ai oi ei m n ng l r w j f v s z sh ch h t k p d g b";
    let line: String = all.split(' ').enumerate().map(|(k, p)| if k < 16 { format!("\"{p}\" c8 ") } else { format!("\"{p} a {p}\" e8 ") }).collect();
    let song = format!(
        "#tempo 140\n@v voice attack=0.005 decay=0 sustain=1 release=0.05 vibrato=0.006 vibrato_speed=5.5 volume=0.8\n\
         @d voice drive=19 filter=bandpass cutoff=1150 volume=0.5\n@c voice drive=12 crush_rate=6300 crush_bits=4 volume=0.4\n\
         A: @v o3 {line} d4 r4\nB: @d o3 r4 {line} e4\nC: @c o4 r2 {line}\n"
    );
    let samples = Player::new(parse(&song).unwrap(), 1.0).render(None);
    assert_eq!((samples.len(), digest(&samples)), VOICE_PINNED, "the voice no longer sings as it did");
}

/// A voice's song: one instrument that sings, notes at 120 (a quarter is
/// 22,050 samples).
fn sung(notes: &str) -> String {
    format!("#tempo 120\n@v voice attack=0.005 decay=0 sustain=1 release=0.05 volume=1\nA: @v o3 {notes}\n")
}

/// A text is the syllable its note sings: the onset before the beat, the
/// vowel from it, the coda ending where the note is let go; a note
/// without a text holds the vowel (the coda moving to the last of the
/// melisma), and after a rest sings the vowel alone.
#[test]
fn a_text_is_the_syllable_its_note_sings() {
    let quarter = SAMPLE_RATE as u64 / 2;
    let song = parse(&sung("r4 \"s ae n\" a4 \"l a\" a4 b4 c4 r4 d4")).unwrap();
    let notes = &song.channels[0];
    let segs = |i: usize| -> &[super::voice::Segment] {
        let (a, b) = notes[i].sung.unwrap();
        &song.segments[a as usize..b as usize]
    };
    // "s ae n": the s 90 ms before the beat, the n at the end of the held note.
    let onset = (0.09 * SR).round() as u64;
    assert_eq!(notes[0].start, quarter - onset);
    let s = segs(0);
    assert!(s[0].frication > 0.0 && s[0].from == 0 && s[0].to == onset, "{s:?}");
    assert!(s[1].voicing == 1.0 && s[1].from == onset, "the vowel on the beat: {s:?}");
    let held = (0.5 * 0.875 * SR) as u64;
    assert_eq!(s.last().unwrap().to, onset + held, "the n ends where the note is let go");
    // "l a" and its melisma: tied to the end, the vowel alone after a rest.
    assert_eq!(notes[1].held, (0.06 * SR).round() as u64 + quarter, "tied to the next");
    assert_eq!((notes[2].start, notes[2].held), (3 * quarter, quarter), "held on to the next");
    assert!(segs(2).iter().all(|s| s.voicing == 1.0), "no onset in a melisma");
    let after_rest = segs(4);
    assert!(after_rest.len() == 1 && after_rest[0].voicing == 1.0 && notes[4].start == 6 * quarter, "{after_rest:?}");
    // The coda goes to a melisma's last note.
    let song = parse(&sung("\"m ae n\" a4 b4")).unwrap();
    let last = song.channels[0][1].sung.unwrap();
    assert!(song.segments[last.1 as usize - 1].voicing < 1.0, "the n on the second note");
    let first = song.channels[0][0].sung.unwrap();
    assert_eq!(song.segments[first.1 as usize - 1].voicing, 1.0, "the first ends on the vowel");
    // A voice's channel is as long as its notes, the onsets moved or not.
    assert_eq!(song.length, 2 * quarter);
}

/// A text's mistakes say their line and what was meant.
#[test]
fn a_texts_mistakes_say_their_line() {
    let said = |text: &str| format!("{:#}", parse(text).unwrap_err());
    for (song, says) in [
        (sung("\"sch a\" c4"), "unknown phoneme 'sch': did you mean 'sh'?"),
        (sung("\"L a\" c4"), "unknown phoneme 'L': did you mean 'l'?"),
        (sung("\"l a o\" c4"), "two vowels, 'a' and 'o'"),
        (sung("\"s t\" c4"), "has no vowel"),
        (sung("\"\" c4"), "an empty text"),
        (sung("\"l a\" r4 c4"), "a text before a rest"),
        (sung("\"l a\" c4 \"l a\""), "a text with no note after it"),
        (sung("\"l a\" \"l a\" c4"), "two texts before one note"),
        (sung("\"l a c4"), "without its closing quote"),
        (sung("c4"), "a voice's first note needs a text"),
        ("@sq square\nA: @sq \"l a\" c4\n".to_string(), "@sq plays no voice"),
        ("A: \"l a\" c4\n".to_string(), "the plain square plays no voice"),
    ] {
        let e = said(&song);
        assert!(e.contains(says) && e.contains("line "), "{song}: {e}");
    }
    let e = said("#tempo 120\n@v voice duty=0.3\nA: @v \"l a\" c4\n");
    assert!(e.contains("line 2") && e.contains("duty means nothing on a voice"), "{e}");
    // The list a mistake gives is the table's.
    assert!(said(&sung("\"q a\" c4")).contains(&super::voice::phonemes()));
}

/// The new words' ranges: drive in decibels, the crusher's rate and its
/// whole bits.
#[test]
fn drive_and_crush_take_their_ranges() {
    for (words, says) in [("drive=40", "from 0 to 36 dB"), ("drive=-1", "out of range"), ("crush_rate=50", "from 100 to 44100 Hz"), ("crush_bits=0", "from 1 to 16 bits"), ("crush_bits=17", "out of range"), ("crush_bits=4.5", "no whole number")] {
        let e = format!("{:#}", parse(&format!("@x saw {words}\nA: @x c")).unwrap_err());
        assert!(e.contains(says) && e.contains("line 1"), "{words}: {e}");
    }
    parse("@x saw drive=36 crush_rate=44100 crush_bits=16\nA: @x c").unwrap();
}

/// Drive pushes a sine towards a square (its harmonics, louder) and
/// leaves no middle off zero; the crusher holds the sound at its rate and
/// rounds it to its bits (one bit: -1, 0 or 1).
#[test]
fn drive_clips_softly_and_the_crusher_steps() {
    let clean = play("@x sine sustain=1\nA: @x o4 a1", 0.5);
    let driven = play("@x sine sustain=1 drive=24\nA: @x o4 a1", 0.5);
    assert!(edge(&driven, 0.1, 0.4) > edge(&clean, 0.1, 0.4) * 3.0, "no harmonics");
    assert!(rms(&driven, 0.1, 0.4) > rms(&clean, 0.1, 0.4) * 1.2, "not louder");
    assert!(driven.iter().all(|s| s.abs() <= 1.0));
    let held = play("@x saw sustain=1 crush_rate=1000\nA: @x o5 a1", 0.3);
    let part = &held[(0.1 * SR) as usize..(0.2 * SR) as usize];
    let changes = part.windows(2).filter(|w| w[0] != w[1]).count();
    assert!((95..=105).contains(&changes), "held at 1000 Hz: {changes} changes in 0.1 s");
    let bits = play("@x sine sustain=1 volume=1 crush_bits=1\nA: @x v15 o4 a1", 0.3);
    assert!(bits.iter().all(|s| [-1.0, 0.0, 1.0].iter().any(|l| (s - l).abs() < 1e-5)), "one bit is three levels");
    // A voice through drive keeps to zero.
    let shout = play(&sung("\"l a\" c2 \"l ae\" e2").replace("volume=1", "volume=1 drive=20"), 2.0);
    let mean = shout.iter().sum::<f32>() / shout.len() as f32;
    assert!(mean.abs() < 0.002, "a driven voice's middle {mean}");
}

/// Every phoneme sings: in a syllable with a, finite, heard, below full
/// scale; a consonant heard before the beat as an onset too.
#[test]
fn every_phoneme_sings() {
    for p in super::voice::PHONEMES {
        let vowel = matches!(p.kind, super::voice::Kind::Vowel(_) | super::voice::Kind::Diphthong(..));
        let text = if vowel { format!("\"{}\"", p.name) } else { format!("\"{0} a {0}\"", p.name) };
        let samples = play(&sung(&format!("r4 {text} a4 r2")), 2.0);
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() < 1.0), "{}", p.name);
        assert!(rms(&samples, 0.5, 0.95) > 0.02, "{} is nearly silent", p.name);
        if !vowel {
            assert!(rms(&samples, 0.46, 0.5) > 0.001, "{} as an onset", p.name);
        }
    }
}

/// The consonants lie under the vowels as in speech (relative phonetic
/// powers after Sacia & Beck, tabled by Fletcher, 1953: sh about 9 dB
/// under a, s and t about 16, f and p about 20), by class: each one's
/// loudest 10 ms against the loudest of the a after it. Driven, the
/// noise joins past the clipper, so an unvoiced one keeps its distance
/// (it was lifted to the vowel's level and hurt).
#[test]
fn consonants_lie_under_the_vowels() {
    use super::voice::Kind;
    // The loudest 10 ms between two times, in dB.
    let loudest = |x: &[f32], from: f32, to: f32| {
        let (a, b, n) = ((from * SR) as usize, (to * SR) as usize, (0.01 * SR) as usize);
        let top = (a..b.saturating_sub(n).max(a + 1)).step_by(n / 4).map(|i| (x[i..i + n].iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt()).fold(0.0, f32::max);
        20.0 * top.log10()
    };
    // The clear voice's plosives breathe longer; `the_singer_tells_minimal_pairs_apart` has them.
    for (p, wave) in super::voice::PHONEMES.iter().flat_map(|p| [(p, "voice"), (p, super::clear::WAVE)]) {
        if wave != "voice" && matches!(p.kind, Kind::Plosive { .. }) {
            continue;
        }
        let (heard, voiced) = match p.kind {
            Kind::Fricative { length, voicing, .. } => (length, voicing > 0.0),
            Kind::Breath { length, .. } => (length, false),
            Kind::Plosive { burst, breath, voiced, .. } => (burst + breath, voiced),
            _ => continue,
        };
        let (lo, hi) = match p.name {
            "s" | "sh" => (-17.0, -11.0),
            "f" | "ch" | "x" | "h" => (-22.0, -15.0),
            // A voiced fricative about 13 dB under its vowel, mostly its
            // voice (Klatt 1980, p. 986: AV and AVS 47 against a vowel's 60).
            "v" | "z" => (-19.0, -10.0),
            "b" => (-30.0, -23.0),
            _ => (-22.0, -13.0),
        };
        let level = |drive: &str| {
            let x = play(&sung(&format!("r2 \"{} a\" a2", p.name)).replace("@v voice", &format!("@v {wave}")).replace("volume=1", &format!("volume=1 {drive}")), 2.0);
            loudest(&x, 1.0 - heard, 1.0) - loudest(&x, 1.1, 1.5)
        };
        let plain = level("");
        assert!((lo..=hi).contains(&plain), "{} sung by {wave}: {plain:.1} dB under the vowel, not {lo} to {hi}", p.name);
        if !voiced {
            for drive in ["drive=10", "drive=19"] {
                let driven = level(drive);
                assert!((driven - plain).abs() < 2.0, "{} with {drive}: {driven:.1} dB against {plain:.1} plain", p.name);
            }
        }
    }
}

/// A sung line joins its syllables without a click: no step from sample
/// to sample where a syllable is taken up greater than within a vowel.
#[test]
fn a_voice_joins_its_syllables_without_clicks() {
    let line = sung("\"l a\" a8 \"l ae\" a8 \"m a\" g8 \"n o\" e8 \"j u\" a4 b4 > c2");
    let samples = play(&line, 3.2);
    let song = parse(&line).unwrap();
    let step = |from: usize, to: usize| samples[from..to].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let within = step((0.05 * SR) as usize, (0.2 * SR) as usize);
    for n in song.channels[0].iter().filter(|n| n.freq.is_some()).skip(1) {
        let at = n.start as usize;
        assert!(step(at - 20, at + 20) < within * 1.5, "a click at {at}: {} against {within}", step(at - 20, at + 20));
    }
}

/// `samples` through a biquad (RBJ's): a low-pass, high-pass (Q 0.71) or
/// band-pass (Q 2) at `f` Hz.
fn filtered(samples: &[f32], kind: &str, f: f32) -> Vec<f32> {
    let w = std::f32::consts::TAU * f / SR;
    let q = if kind == "band" { 2.0 } else { std::f32::consts::FRAC_1_SQRT_2 };
    let alpha = w.sin() / (2.0 * q);
    let cos = w.cos();
    let b = match kind {
        "low" => [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0],
        "high" => [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
        _ => [alpha, 0.0, -alpha],
    };
    let a0 = 1.0 + alpha;
    let a = [-2.0 * cos / a0, (1.0 - alpha) / a0];
    let (mut x, mut y) = ([0.0f32; 2], [0.0f32; 2]);
    samples
        .iter()
        .map(|&s| {
            let out = (b[0] * s + b[1] * x[0] + b[2] * x[1]) / a0 - a[0] * y[0] - a[1] * y[1];
            (x, y) = ([s, x[0]], [out, y[0]]);
            out
        })
        .collect()
}

/// A song sung by the clear voice, as `sung` makes one for `voice`.
fn clearly(notes: &str) -> String {
    sung(notes).replace("@v voice", &format!("@v {}", super::clear::WAVE))
}

/// The clear voice sings each vowel by its formants: of the harmonics of
/// a held vowel at 110 Hz, the strongest under 1 kHz lies at its F1 and
/// the strongest peak (louder than both its neighbours) between F1 and
/// 2.6 kHz at its F2 (the table's, within a harmonic or 12 %).
#[test]
fn the_singer_sings_each_vowel_by_its_formants() {
    for p in super::voice::PHONEMES {
        let super::voice::Kind::Vowel(f) = p.kind else { continue };
        let x = play(&clearly(&format!("o2 \"{}\" a1", p.name)), 1.0);
        let part = &x[(0.4 * SR) as usize..(0.9 * SR) as usize];
        let level = |hz: f32| {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, s) in part.iter().enumerate() {
                let phase = std::f32::consts::TAU * hz * i as f32 / SR;
                (re, im) = (re + s * phase.cos(), im + s * phase.sin());
            }
            re.hypot(im)
        };
        let levels: Vec<f32> = (0..40).map(|k| level(k as f32 * 110.0)).collect();
        let strongest = |lo: f32, hi: f32, peak: bool| {
            let k = (1..39).filter(|&k| (lo..=hi).contains(&(k as f32 * 110.0)) && (!peak || (levels[k] > levels[k - 1] && levels[k] > levels[k + 1])));
            k.max_by(|a, b| levels[*a].total_cmp(&levels[*b])).unwrap() as f32 * 110.0
        };
        let f1 = strongest(150.0, 1000.0, false);
        let f2 = strongest(f[0] + 250.0, 2600.0, true);
        for (got, want, which) in [(f1, f[0], "F1"), (f2, f[1], "F2")] {
            assert!((got - want).abs() <= (want * 0.12).max(110.0), "{}: {which} at {got} Hz, the table's {want}", p.name);
        }
    }
}

/// Both voices put their energy where sung speech does: of a sung line,
/// a small part above 1 kHz (about 9 % in speech's long-term spectrum),
/// most of it below.
#[test]
fn the_voices_put_their_energy_where_speech_does() {
    let line = "\"ih m\" d8 \"l ih ch t\" e8 \"d e r\" f8 \"z o\" g8 \"n ax\" e8 \"sh l eh f t\" f8 \"d i\" g8 \"k l ai\" a8 \"n ax\" g8 \"k a\" f8 \"t s ax\" e8 \"sh t ih l\" d4 r4";
    for song in [sung(line), clearly(line)] {
        let x = play(&song, 3.5);
        let total: f32 = x.iter().map(|s| s * s).sum();
        let high: f32 = filtered(&filtered(&x, "high", 1000.0), "high", 1000.0).iter().map(|s| s * s).sum();
        let share = high / total;
        assert!((0.03..0.25).contains(&share), "{share:.3} above 1 kHz: {}", &song[..40]);
    }
}

/// The clear voice tells minimal pairs apart as speech does: F2 rises
/// into a after b and falls after d, after g F3 starts near F2 (a
/// velar's); p breathes before its voicing (long voice onset time), b
/// does not; s hisses above 5 kHz, sh between 2 and 4.
#[test]
fn the_singer_tells_minimal_pairs_apart() {
    // F1 to F3 5 ms into the vowel, from the voice's own formants.
    let onset = |text: &str| {
        let mut p = Player::new(parse(&clearly(&format!("r4 \"{text}\" c4"))).unwrap(), 1.0);
        let mut out = vec![0.0; ((0.5 + 0.005) * SR) as usize * 2];
        p.mix(&mut out);
        p.voices[0].clear.formants_now()
    };
    let (b, d, g) = (onset("b a"), onset("d a"), onset("g a"));
    assert!(b[1] < 1090.0 && d[1] > 1300.0, "F2 into a: after b {:.0}, after d {:.0}", b[1], d[1]);
    assert!(g[2] - g[1] < 700.0 && d[2] - d[1] > 1000.0, "F3 over F2 after g {:.0}, after d {:.0}", g[2] - g[1], d[2] - d[1]);
    // The voice onset time: from the burst (where the noise above 2 kHz
    // first reaches 30 % of its most before the beat) to the voicing
    // (under 600 Hz at half the vowel's).
    let vot = |text: &str| {
        let x = play(&clearly(&format!("r4 \"{text}\" c4")), 1.0);
        let env = |y: &[f32], at: usize| (y[at..at + 220].iter().map(|s| s * s).sum::<f32>() / 220.0).sqrt();
        let (high, low) = (filtered(&x, "high", 2000.0), filtered(&x, "low", 600.0));
        let beat = (0.5 * SR) as usize;
        let most = (beat - (0.15 * SR) as usize..beat).map(|i| env(&high, i)).fold(0.0, f32::max);
        let burst = (beat - (0.15 * SR) as usize..beat).find(|&i| env(&high, i) > most * 0.3).unwrap();
        let vowel = env(&low, beat + (0.15 * SR) as usize);
        let voiced = (burst..beat + 4410).find(|&i| env(&low, i) > vowel * 0.5).unwrap();
        (voiced - burst) as f32 / SR * 1000.0
    };
    let (p, b) = (vot("p a"), vot("b a"));
    // A long lag after p, a short one after b (up to about 25 ms: Lisker &
    // Abramson 1964).
    assert!(p > 40.0 && b <= 25.0, "voice onset after p {p:.0} ms, after b {b:.0} ms");
    // Where s and sh hiss: the 70 ms before the beat, 6.3 kHz against 2.9.
    let hiss = |text: &str| {
        let x = play(&clearly(&format!("r4 \"{text}\" c4")), 1.0);
        let at = ((0.5 - 0.075) * SR) as usize..(0.495 * SR) as usize;
        let band = |f: f32| filtered(&x, "band", f)[at.clone()].iter().map(|s| s * s).sum::<f32>();
        band(6300.0) / band(2900.0)
    };
    let (s, sh) = (hiss("s a"), hiss("sh a"));
    assert!(s > 3.0 && sh < 0.3, "6.3 kHz against 2.9 kHz: s {s:.2}, sh {sh:.2}");
}

/// `consonants` moves a voice's hiss against its vowels by its decibels,
/// in both voices, and means nothing on a wave.
#[test]
fn consonants_move_the_consonants() {
    for wave in ["voice", super::clear::WAVE] {
        let level = |words: &str| {
            let x = play(&sung("r2 \"s a\" a2").replace("@v voice", &format!("@v {wave} {words}")), 2.0);
            20.0 * (rms(&x, 0.93, 0.99) / rms(&x, 1.1, 1.5)).log10()
        };
        let (plain, up, down) = (level(""), level("consonants=6"), level("consonants=-12"));
        assert!((up - plain - 6.0).abs() < 1.0 && (plain - down - 12.0).abs() < 1.0, "{wave}: {plain:.1} dB, +6 {up:.1}, -12 {down:.1}");
    }
    let e = format!("{:#}", parse("@x saw consonants=3\nA: @x c").unwrap_err());
    assert!(e.contains("consonants means nothing on a saw") && e.contains("line 1"), "{e}");
    let e = format!("{:#}", parse("@x voice consonants=20\nA: @x \"l a\" c").unwrap_err());
    assert!(e.contains("from -24 to 12 dB"), "{e}");
}

/// The clear voice sings every consonant of a cluster, as eighths at 124
/// with a quarter last: each one its segments, at least 25 ms in all;
/// each heard, its loudest 10 ms (in its noise's band where it hisses or
/// bursts) 12 dB over the quietest 10 ms in the word or more (a voiced
/// plosive's voice bar may be that) and within 30 dB of its loudest vowel, a hiss or
/// a burst 4 dB under the word's loudest vowel at least (consonants that
/// stood out hurt; `consonants_lie_under_the_vowels` pins them against an
/// a); a plosive
/// after s or sh unaspirated, a coda kept before the next onset. A
/// shape a word: s and a plosive (and r, l) in an onset, a coda s before
/// a t, a nasal before a plosive of its place, a plosive and r, a hiss and
/// v, a coda cluster, pf and l, sh, p and r.
#[test]
fn the_singer_sings_every_consonant_of_a_cluster() {
    let words: [&[&str]; 10] = [
        &["sh t r u m p f"],
        &["s p l ae sh"],
        &["s t r eh ng k f"],
        &["sh v eh s", "t er"],
        &["t a ng", "g o"],
        &["g r ue n"],
        &["t s v eh r k"],
        &["a ng s t"],
        &["p f l a n", "t s ax"],
        &["sh p r eh", "ch ax n"],
    ];
    for syllables in words {
        let notes: Vec<String> = syllables.iter().enumerate().map(|(i, s)| if i + 1 == syllables.len() { format!("q8 \"{s}\" e4 q7") } else { format!("\"{s}\" e8") }).collect();
        let text = format!("#tempo 124\n@v {} attack=0.005 decay=0 sustain=1 release=0.06 volume=1\nA: @v o3 r2 {} r4\n", super::clear::WAVE, notes.join(" "));
        let song = parse(&text).unwrap();
        let x = play(&text, song.length as f32 / SR);
        // Every segment where it lies in the song, with its note's start.
        let segs: Vec<(u64, &super::clear::Seg)> = song.channels[0].iter().filter_map(|n| n.sung.map(|(a, b)| (n.start, a, b))).flat_map(|(at, a, b)| song.clear[a as usize..b as usize].iter().map(move |s| (at, s))).collect();
        let n = (0.01 * SR) as usize;
        // The loudest 10 ms between two samples, of `y`, in dB.
        let loudest = |y: &[f32], a: u64, b: u64| {
            let (a, b) = (a as usize, (b as usize).max(a as usize + n).min(y.len()));
            let top = (a..b.saturating_sub(n).max(a + 1)).step_by(n / 4).map(|i| (y[i..i + n].iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt()).fold(0.0, f32::max);
            20.0 * top.max(1e-9).log10()
        };
        let word = (segs[0].0 + segs[0].1.from, segs.last().map_or(0, |(at, s)| at + s.to));
        let vowel = segs.iter().filter(|(_, s)| s.voicing >= 1.0).map(|(at, s)| loudest(&x, at + s.from, at + s.to)).fold(-200.0, f32::max);
        let quiet = (word.0 as usize..word.1 as usize - n).step_by(n).map(|i| loudest(&x, i as u64, (i + n) as u64)).fold(0.0, f32::min).max(vowel - 60.0);
        let said = syllables.join(" | ");
        let mut before: Option<&str> = None;
        for p in syllables.iter().flat_map(|s| s.split(' ')).filter(|p| super::voice::PHONEMES.iter().any(|q| q.name == *p && !matches!(q.kind, super::voice::Kind::Vowel(_) | super::voice::Kind::Diphthong(..)))) {
            let own: Vec<&(u64, &super::clear::Seg)> = segs.iter().filter(|(_, s)| s.name == p).collect();
            let length: u64 = own.iter().map(|(_, s)| s.to - s.from).sum();
            assert!(length as f32 >= 0.025 * SR, "{said}: {p} lasts {:.0} ms", length as f32 / SR * 1000.0);
            // Heard: in its band where it hisses through one, else whole.
            let level = own
                .iter()
                .map(|(at, s)| match s.band() {
                    Some(f) => loudest(&filtered(&x, "band", f), at + s.from, at + s.to).max(loudest(&x, at + s.from, at + s.to) - 6.0),
                    None => loudest(&x, at + s.from, at + s.to),
                })
                .fold(-200.0, f32::max);
            assert!(level >= quiet + 12.0 && level >= vowel - 30.0, "{said}: {p} at {level:.1} dB, the quiet {quiet:.1}, the vowel {vowel:.1}");
            if own.iter().any(|(_, s)| s.noisy() && s.voicing == 0.0) {
                assert!(level <= vowel - 4.0, "{said}: {p} at {level:.1} dB, the vowel {vowel:.1}");
            }
            if before.is_some_and(|b| b == "s" || b == "sh") && matches!(p, "p" | "t" | "k") {
                let breath: u64 = own.iter().filter(|(_, s)| s.breathes()).map(|(_, s)| s.to - s.from).sum();
                assert!(breath as f32 <= 0.015 * SR, "{said}: {p} after {before:?} breathes {:.0} ms", breath as f32 / SR * 1000.0);
            }
            before = Some(p);
        }
    }
}

/// The clear voice tells g from b before r, and r from l (grau was heard
/// as blau): a voiced plosive's release hisses longer the further back its
/// place, a g's at least 20 ms and a b's at most 15, a g's compact near the
/// vowel's F2 (its strongest between 1.2 and 2.5 kHz), a b's not; the r a
/// German one, its F3 high as in l (not an English r's low one) but its F1
/// higher than l's, and a rasp where l is clean; before ü its F1 no
/// higher than the vowel's, a vowel before it turning towards it; a g
/// after ng closed briefly and voiced.
#[test]
fn the_singer_tells_g_from_b_and_r_from_l() {
    use super::clear::Seg;
    let plan = |text: &str| {
        let song = parse(&clearly(&format!("r4 \"{text}\" c4"))).unwrap();
        let (a, b) = song.channels[0].iter().find_map(|n| n.sung).unwrap();
        song.clear[a as usize..b as usize].to_vec()
    };
    let release = |segs: &[Seg], p: &str| segs.iter().filter(|s| s.name == p && s.noisy()).map(|s| (s.to - s.from) as f32 / SR * 1000.0).sum::<f32>();
    let (g, b) = (plan("g r aw"), plan("b r aw"));
    let (g_ms, b_ms) = (release(&g, "g"), release(&b, "b"));
    assert!(g_ms >= 20.0 && b_ms <= 15.0, "the release of g {g_ms:.0} ms, of b {b_ms:.0} ms");
    let g_band = g.iter().find(|s| s.name == "g" && s.noisy()).and_then(|s| s.band()).unwrap();
    assert!((1200.0..2500.0).contains(&g_band), "g's release at {g_band:.0} Hz");
    // No voice bar before a g that begins a phrase.
    assert!(g.iter().filter(|s| s.name == "g").all(|s| s.voicing == 0.0), "a voice bar before g after silence");
    // The r against the l: F1 to F3 in the middle of each, from the voice.
    let middle = |text: &str| {
        let segs = plan(text);
        let s = segs.iter().find(|s| s.name == text.split(' ').nth(1).unwrap()).unwrap();
        let start = parse(&clearly(&format!("r4 \"{text}\" c4"))).unwrap().channels[0].iter().find(|n| n.sung.is_some()).unwrap().start;
        let mut p = Player::new(parse(&clearly(&format!("r4 \"{text}\" c4"))).unwrap(), 1.0);
        let mut out = vec![0.0; (start + (s.from + s.to) / 2) as usize * 2];
        p.mix(&mut out);
        (p.voices[0].clear.formants_now(), s.noisy())
    };
    let ((r, rasp), (l, clean)) = (middle("g r aw"), middle("g l aw"));
    assert!(r[2] > 2200.0 && r[0] > l[0] + 100.0 && rasp && !clean, "r {r:.0?} rasping {rasp}, l {l:.0?} hissing {clean}");
    // Before a closer vowel the r is no more open than it: its F1 not over ü's.
    let (r, _) = middle("g r ue");
    assert!(r[0] < 330.0, "r before ue at F1 {:.0}", r[0]);
    // A vowel before a coda r turns towards the r, not away and back: at
    // its end F1 risen and F2 fallen towards the r's.
    let text = clearly("r4 \"d e r\" c4");
    let song = parse(&text).unwrap();
    let (start, end) = song.channels[0].iter().find_map(|n| n.sung.map(|(a, b)| (n.start, song.clear[a as usize..b as usize].iter().filter(|s| s.name == "e").map(|s| s.to).max().unwrap()))).unwrap();
    let mut p = Player::new(song, 1.0);
    let mut out = vec![0.0; (start + end) as usize * 2];
    p.mix(&mut out);
    let f = p.voices[0].clear.formants_now();
    assert!(f[0] > 450.0 && f[1] < 1500.0, "the e of d e r ends at {f:.0?}");
    // A g after ng closes briefly, the voicing going on through it.
    let song = parse(&clearly("r4 \"t o ng\" c8 \"g r aw\" c4")).unwrap();
    let closure: u64 = song.clear.iter().filter(|s| s.name == "g" && !s.noisy()).map(|s| s.to - s.from).sum();
    let voiced = song.clear.iter().filter(|s| s.name == "g" && !s.noisy()).all(|s| s.voicing > 0.0);
    assert!(closure as f32 <= 0.021 * SR && voiced, "g after ng closed {:.0} ms, voiced {voiced}", closure as f32 / SR * 1000.0);
}

/// The music is the same samples on every system: no call of the system's
/// maths library, whose last bit differs between glibc versions and systems
/// and which the drive and the crusher blow up (as `sim::math` for the
/// simulation); `libm` instead. `sqrt` is exactly rounded and stays.
/// The limiter and the loudness meter, which the WAVs go through, too. The
/// tests themselves (`tests.rs`, `loop_tests.rs`, what follows a
/// `#[cfg(test)]`) only measure, and measure with std.
#[test]
fn the_music_does_its_own_math() {
    let banned = [
        ".sin(", ".cos(", ".sin_cos(", ".tan(", ".asin(", ".acos(", ".atan(", ".atan2(", ".sinh(", ".cosh(", ".tanh(",
        ".exp(", ".exp2(", ".ln(", ".log(", ".log2(", ".log10(", ".powf(", ".powi(", ".cbrt(", ".hypot(",
    ];
    let mut found = Vec::new();
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir("src/audio/music").expect("reading the music").map(|e| e.unwrap().path()).collect();
    files.extend(["src/audio/limiter.rs", "src/audio/loudness.rs"].map(Into::into));
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if ["tests.rs", "loop_tests.rs", "laid_tests.rs"].contains(&name.as_str()) || path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let text = text.split("#[cfg(test)]").next().unwrap_or("");
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for b in banned.iter().filter(|b| code.contains(*b)) {
                found.push(format!("{}:{}: {b}", path.display(), n + 1));
            }
        }
    }
    assert!(found.is_empty(), "the system's math in the music (use libm):\n{}", found.join("\n"));
}

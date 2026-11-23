//! Samples: the WAV reader and its mistakes, the converter to the
//! engine's rate, and the wave `sample` played (its pitch, its stretch,
//! backwards, round, as a layer, its mistakes, the strip's words).

use super::player::*;
use super::sample::*;
use super::*;
use crate::audio::SAMPLE_RATE;

const SR: f32 = SAMPLE_RATE as f32;

/// A sine of `hz` for `seconds` at `rate`, at `level`.
fn sine(hz: f32, seconds: f32, rate: u32, level: f32) -> Vec<f32> {
    (0..(seconds * rate as f32) as usize).map(|i| level * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin()).collect()
}

/// A recording named `name` of these frames at the engine's rate.
fn recording(name: &str, frames: Vec<f32>) -> Recording {
    Recording { name: name.into(), file: format!("samples/{name}.wav"), hash: name.into(), frames }
}

/// `text` read with `samples`, played for `seconds`: its left side.
fn left(text: &str, samples: &Samples, seconds: f32) -> Vec<f32> {
    let mut song = parse(text).unwrap();
    song.bind(samples).unwrap();
    Player::new(song, 1.0).render(Some(seconds)).into_iter().step_by(2).collect()
}

/// How fast `s` goes round within `from..to` seconds, by its rising
/// crossings of zero (between samples, in a straight line).
fn hz(s: &[f32], from: f32, to: f32) -> f32 {
    let s = &s[(from * SR) as usize..(to * SR) as usize];
    let ups: Vec<f32> = (1..s.len()).filter(|&i| s[i - 1] < 0.0 && s[i] >= 0.0).map(|i| i as f32 - 1.0 + s[i - 1] / (s[i - 1] - s[i])).collect();
    (ups.len() - 1) as f32 * SR / (ups[ups.len() - 1] - ups[0])
}

/// The largest step from one sample to the next within `from..to` seconds.
fn steepest(s: &[f32], from: f32, to: f32) -> f32 {
    s[(from * SR) as usize..(to * SR) as usize].windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()))
}

/// The loudest sample within `from..to` seconds.
fn peak(s: &[f32], from: f32, to: f32) -> f32 {
    s[(from * SR) as usize..(to * SR) as usize].iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// The reader reads 16 and 24 bit, mono and stereo, plain and extensible,
/// at 22050, 44100 and 48000 Hz: the values written (a stereo file's two
/// sides added at half each), to the bits' step.
#[test]
fn the_reader_reads_16_and_24_bit_mono_and_stereo() {
    let values = [0.0f32, 0.5, -0.5, 0.999, -1.0, 0.25, 0.125, -0.0625];
    for rate in [22_050, 44_100, 48_000] {
        for bits in [16u16, 24] {
            for extensible in [false, true] {
                let step = 2.0 / (1u32 << (bits - 1)) as f32;
                let mono = decode("m.wav", &wav_file(rate, 1, bits, extensible, &values)).unwrap();
                assert_eq!(mono.rate, rate);
                assert_eq!(mono.frames.len(), values.len());
                assert!(mono.frames.iter().zip(&values).all(|(a, b)| (a - b).abs() <= step), "{bits} bit mono: {:?}", mono.frames);
                let stereo: Vec<f32> = values.iter().flat_map(|&v| [v, -v * 0.5]).collect();
                let two = decode("s.wav", &wav_file(rate, 2, bits, extensible, &stereo)).unwrap();
                assert_eq!(two.frames.len(), values.len());
                assert!(two.frames.iter().zip(&values).all(|(a, &v)| (a - 0.25 * v).abs() <= step), "{bits} bit stereo: {:?}", two.frames);
            }
        }
    }
    // Chunks it does not know are passed over, an odd one with its pad byte.
    let mut file = wav_file(44_100, 1, 16, false, &values);
    let at = 12;
    file.splice(at..at, *b"LIST\x03\x00\x00\x00abc\x00");
    assert_eq!(decode("l.wav", &file).unwrap().frames.len(), values.len());
}

/// What the reader does not read it refuses naming the file and what it
/// is: float, 8 and 32 bit, compressed, more channels, another rate, a
/// big-endian or 64-bit RIFF, no WAV at all, no sound, too long.
#[test]
fn the_reader_refuses_the_rest_by_name() {
    let good = wav_file(44_100, 1, 16, false, &[0.1, 0.2, 0.3, 0.4]);
    let with = |at: usize, bytes: &[u8]| {
        let mut f = good.clone();
        f[at..at + bytes.len()].copy_from_slice(bytes);
        f
    };
    let wrong = |bytes: &[u8]| format!("{:#}", decode("samples/x.wav", bytes).unwrap_err());
    // The fmt chunk's body begins at 20: tag, channels, rate, bytes a
    // second, a frame's bytes, bits.
    for (bytes, says) in [
        (with(20, &3u16.to_le_bytes()), "32-bit float"),
        (with(20, &2u16.to_le_bytes()), "compressed (format 2)"),
        (with(20, &7u16.to_le_bytes()), "A-law or µ-law"),
        (with(34, &8u16.to_le_bytes()), "8 bit"),
        (with(34, &32u16.to_le_bytes()), "32-bit integers"),
        (with(22, &3u16.to_le_bytes()), "3 channels; a sample is mono or stereo"),
        (with(24, &8_000u32.to_le_bytes()), "recorded at 8000 Hz; a sample is from 22050 to 48000 Hz"),
        (with(24, &96_000u32.to_le_bytes()), "recorded at 96000 Hz"),
        (with(0, b"RIFX"), "big-endian"),
        (with(0, b"RF64"), "RF64"),
        (b"ID3\x03 an mp3".to_vec(), "not a WAV"),
        (with(32, &4u16.to_le_bytes()), "its frames are 4 bytes"),
    ] {
        let e = wrong(&bytes);
        assert!(e.starts_with("samples/x.wav: ") && e.contains(says), "{says}: {e}");
    }
    // Float in an extensible file, by its subformat.
    let mut float = wav_file(44_100, 1, 16, true, &[0.1, 0.2]);
    float[44] = 3;
    assert!(wrong(&float).contains("32-bit float"), "{}", wrong(&float));
    float[50] ^= 0xFF;
    assert!(wrong(&float).contains("a format of its own"), "{}", wrong(&float));
    // No sound, and a sample past ten seconds.
    assert!(wrong(&wav_file(44_100, 1, 16, false, &[])).contains("holds no sound"));
    let long = wav_file(22_050, 1, 16, false, &vec![0.0; (10.5 * 22_050.0) as usize]);
    assert!(wrong(&long).contains("lasts 10.50 seconds; a sample is 10 seconds at most"), "{}", wrong(&long));
    assert!(decode("ok.wav", &wav_file(22_050, 1, 16, false, &vec![0.0; 10 * 22_050])).is_ok(), "ten seconds are taken");
}

/// A hostile WAV is a mistake, never a crash or a huge allocation: every
/// file cut short at every length, sizes past the file (up to 4 GB), a
/// format cut short, the sound before its format, and bytes changed at
/// random all over a good file.
#[test]
fn hostile_wavs_are_mistakes_never_a_crash() {
    let good = wav_file(48_000, 2, 24, true, &sine(440.0, 0.01, 48_000, 0.5).repeat(2));
    for cut in 0..good.len() {
        let _ = decode("cut.wav", &good[..cut]);
    }
    let with = |at: usize, bytes: &[u8]| {
        let mut f = good.clone();
        f[at..at + bytes.len()].copy_from_slice(bytes);
        f
    };
    let data_size = good.windows(4).position(|w| w == b"data").unwrap() + 4;
    for (bytes, says) in [
        (with(16, &u32::MAX.to_le_bytes()), "cut short: the chunk \"fmt\" says 4294967295 bytes"),
        (with(16, &8u32.to_le_bytes()), "its format (the fmt chunk) is cut short: 8 bytes"),
        (with(16, &20u32.to_le_bytes()), "its extensible format is cut short: 20 bytes"),
        (with(data_size, &u32::MAX.to_le_bytes()), "cut short: its sound says 4294967295 bytes"),
        (with(12, b"data"), "its sound (data) comes before its format"),
    ] {
        let e = format!("{:#}", decode("h.wav", &bytes).unwrap_err());
        assert!(e.contains(says), "{says}: {e}");
    }
    // Bytes changed at random, a few at a time, many times.
    let mut seed = 0x2545_f491u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for _ in 0..5_000 {
        let mut f = good.clone();
        for _ in 0..1 + next() % 4 {
            let at = next() as usize % 80.min(f.len());
            f[at] = next() as u8;
        }
        if let Ok(wav) = decode("r.wav", &f) {
            assert!(wav.frames.len() <= good.len() && (RATES.0..=RATES.1).contains(&wav.rate));
        }
    }
}

/// A sine recorded at 22050, 44100 or 48000 Hz plays at the engine's
/// rate at its own frequency at its root (c4 unless said), an octave up
/// at twice it and half as long, at another root by that root.
#[test]
fn a_sample_plays_at_its_frequency_at_root_and_an_octave_up() {
    for rate in [22_050, 44_100, 48_000] {
        let dir = crate::scratch::Scratch::new("sample-rates");
        std::fs::create_dir_all(dir.join("samples")).unwrap();
        std::fs::write(dir.join("samples/tone.wav"), wav_file(rate, 1, 16, false, &sine(220.0, 1.0, rate, 0.5))).unwrap();
        let samples = Samples::read_folder(&crate::files::Files::folder(&dir)).unwrap();
        let tone = samples.get("tone").unwrap();
        assert!((tone.seconds() - 1.0).abs() < 1e-3, "{rate} Hz: {} seconds at the engine's rate", tone.seconds());
        let play = |root: &str, note: &str| left(&format!("#tempo 60\n@s sample name=tone {root} attack=0 decay=0 sustain=1 release=0.01 volume=1\nA: @s v15 {note}1 r1\n"), &samples, 2.0);
        let at_root = play("", "o4 c");
        assert!((hz(&at_root, 0.1, 0.9) - 220.0).abs() < 0.2, "{rate} Hz at its root: {}", hz(&at_root, 0.1, 0.9));
        assert!((peak(&at_root, 0.1, 0.9) - 0.5).abs() < 0.01, "as loud as recorded: {}", peak(&at_root, 0.1, 0.9));
        let up = play("", "o5 c");
        assert!((hz(&up, 0.05, 0.45) - 440.0).abs() < 0.4, "{rate} Hz an octave up: {}", hz(&up, 0.05, 0.45));
        assert!(peak(&up, 0.5, 1.0) == 0.0, "an octave up it is over in half the time");
        let other = play("root=a3", "o4 a");
        assert!((hz(&other, 0.05, 0.45) - 440.0).abs() < 0.4, "a4 over a root of a3: an octave up, {}", hz(&other, 0.05, 0.45));
    }
}

/// `start` and `end` play that stretch of the recording; `reverse` plays
/// it backwards; an unlooped note held past its stretch ends silent,
/// faded over its last milliseconds without a step, the note over then
/// though its envelope is not.
#[test]
fn a_stretch_plays_forwards_or_backwards_and_ends_silent() {
    // A ramp from 0 to 1 over a second: each sample tells where it was read.
    let ramp: Vec<f32> = (0..SAMPLE_RATE).map(|i| i as f32 / SR).collect();
    let samples = Samples::of(vec![recording("ramp", ramp)]);
    let play = |words: &str| left(&format!("#tempo 60\n#loop no\n@s sample name=ramp {words} attack=0 decay=0 sustain=1 release=0.01 volume=1\nA: @s v15 o4 c1\n"), &samples, 1.5);
    let cut = play("start=0.25 end=0.5");
    assert!((cut[0] - 0.25).abs() < 1e-3 && (cut[(0.2 * SR) as usize] - 0.45).abs() < 1e-3, "from 0.25: {} {}", cut[0], cut[(0.2 * SR) as usize]);
    assert!(cut[(0.25 * SR) as usize..].iter().all(|v| *v == 0.0), "silent after its stretch");
    assert!(steepest(&cut, 0.2, 0.3) < 0.01, "a step at its end: {}", steepest(&cut, 0.2, 0.3));
    let back = play("start=0.25 end=0.5 reverse=yes");
    assert!((back[0] - 0.5).abs() < 1e-3 && (back[(0.2 * SR) as usize] - 0.3).abs() < 1e-3, "backwards from 0.5: {} {}", back[0], back[(0.2 * SR) as usize]);
    assert!(back[(0.25 * SR) as usize..].iter().all(|v| *v == 0.0));
    let mut p = Player::new(parse("#tempo 60\n@s sample name=ramp end=0.1 attack=0 decay=0 sustain=1 release=2\nA: @s q8 o4 c1\n").unwrap(), 1.0);
    p.song.bind(&samples).unwrap();
    let mut out = vec![0.0; (0.05 * SR) as usize * 2];
    p.mix(&mut out);
    assert!(p.voices[0].note.is_some(), "it sounds");
    let mut out = vec![0.0; (0.1 * SR) as usize * 2];
    p.mix(&mut out);
    assert!(p.voices[0].note.is_none(), "over once its stretch played out, its key still held");
}

/// `loop=yes`: the stretch goes round while a long note sounds, its seam
/// faded across, so a sine not of whole turns in it steps no more at the
/// seam than within (unfaded the seam steps some 20 times as far).
#[test]
fn a_loop_holds_a_long_note_without_a_click() {
    // 0.1 s of 230 Hz: 23 turns, the end not where the start is.
    let tone = recording("tone", sine(230.0, 0.1037, SAMPLE_RATE, 0.8));
    let samples = Samples::of(vec![tone]);
    let held = left("#tempo 60\n@s sample name=tone loop=yes attack=0.002 decay=0 sustain=1 release=0.05 volume=1\nA: @s v15 o4 c1 r1\n", &samples, 3.0);
    let own = 0.8 * std::f32::consts::TAU * 230.0 / SR;
    assert!(peak(&held, 2.8, 2.9) > 0.5, "it still sounds after many rounds");
    assert!(steepest(&held, 0.01, 2.0) < 1.5 * own, "a click at a seam: {} against the sine's own {own}", steepest(&held, 0.01, 2.0));
    // At another pitch too.
    let high = left("#tempo 60\n@s sample name=tone loop=yes attack=0.002 decay=0 sustain=1 release=0.05 volume=1\nA: @s v15 o4 g1\n", &samples, 2.0);
    assert!(steepest(&high, 0.01, 1.9) < 1.5 * own * 1.5, "a click at a seam a fifth up: {}", steepest(&high, 0.01, 1.9));
    assert!(peak(&high, 1.5, 1.9) > 0.5);
}

/// A sample as a layer (`+ sample name=...`) adds up as the same sample on
/// a channel of its own does, and a sample instrument's layers alike.
#[test]
fn a_sample_as_a_layer_adds_up() {
    let samples = Samples::of(vec![recording("hit", sine(150.0, 0.3, SAMPLE_RATE, 0.7).into_iter().enumerate().map(|(i, v)| v * (1.0 - i as f32 / (0.3 * SR))).collect())]);
    let (a, b) = ("sine freq=60 attack=0.001 decay=0.3 sustain=0 curve=exp volume=0.6", "sample name=hit root=c4 attack=0.001 decay=0 sustain=1 release=0.02 lowpass=3000 volume=0.5");
    let layered = left(&format!("#tempo 120\n@k {a}\n  + {b}\nA: @k o4 l4 c d c r\n"), &samples, 2.0);
    let apart = left(&format!("#tempo 120\n@a {a}\n@b {b}\nA: @a o4 l4 c d c r\nB: @b o4 l4 c d c r\n"), &samples, 2.0);
    assert!(peak(&layered, 0.0, 2.0) > 0.3, "it sounds");
    let off = layered.iter().zip(&apart).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
    assert!(off < 1e-5, "the layers are not the channels' sum: {off}");
    let other = left(&format!("#tempo 120\n@k {b}\n  + {a}\nA: @k o4 l4 c d c r\n"), &samples, 2.0);
    assert!(other.iter().zip(&apart).all(|(x, y)| (x - y).abs() < 1e-5), "a sample's own layers");
}

/// A sample's mistakes say their line: the wave without a name, a name not
/// among the samples (the nearest named), a stretch past the recording or
/// ending before it begins, a word of the pulse on it, a sample's word on
/// another wave, a root that is no note, a loop neither yes nor no, a name
/// with its ending; and a song naming no sample reads without any.
#[test]
fn a_samples_mistakes_say_their_line() {
    let wrong = |text: &str| format!("{:#}", parse(text).unwrap_err());
    for (text, says) in [
        ("@s sample\nA: @s c\n", "line 1: @s: the wave sample needs the recording's name"),
        ("@s sample name=a start=0.5 end=0.2\nA: @s c\n", "start=0.5 and end=0.2: a sample's stretch ends after it begins"),
        ("@s sample name=a duty=0.3\nA: @s c\n", "duty means nothing on a sample"),
        ("@s square name=a\nA: @s c\n", "name means nothing on a square: only the wave sample plays a recording"),
        ("@s voice root=c4\nA: @s \"l a\" c\n", "root means nothing on a voice"),
        ("@s sample name=a root=h4\nA: @s c\n", "root=h4: a note, a letter and an octave"),
        ("@s sample name=a loop=maybe\nA: @s c\n", "loop=maybe: yes or no"),
        ("@s sample name=a.wav\nA: @s c\n", "name=a.wav: the name without its ending, name=a"),
        ("@s sample name=a consonants=3\nA: @s c\n", "consonants means nothing on a sample"),
        ("@s sine\n+ sample\nA: @s c\n", "line 2: @s's layer 2: the wave sample needs"),
        ("@s sample name=a start=11\nA: @s c\n", "start=11 is out of range, from 0 to 10 seconds"),
    ] {
        let e = wrong(text);
        assert!(e.contains(says), "{text:?}: {e}");
    }
    let samples = Samples::of(vec![recording("thump", vec![0.1; 4410]), recording("snap", vec![0.1; 4410])]);
    let bound = |text: &str, samples: &Samples| parse(text).unwrap().bind(samples).map_err(|e| format!("{e:#}"));
    let e = bound("#tempo 90\n\n@k sample name=thmp\nA: @k c\n", &samples).unwrap_err();
    assert!(e.contains("line 3: sample name=thmp: there is no samples/thmp.wav (the nearest: \"thump\")"), "{e}");
    let e = bound("@k sine\n+ sample name=snp\nA: @k c\n", &samples).unwrap_err();
    assert!(e.contains("line 2: sample name=snp") && e.contains("\"snap\""), "{e}");
    let e = bound("@k sample name=x\nA: @k c\n", &Samples::default()).unwrap_err();
    assert!(e.contains("the game has no samples"), "{e}");
    let e = bound("@k sample name=thump end=0.2\nA: @k c\n", &samples).unwrap_err();
    assert!(e.contains("line 1: sample end=0.2 lies past the end of samples/thump.wav, which lasts 0.100 seconds"), "{e}");
    let e = bound("@k sample name=thump start=0.1\nA: @k c\n", &samples).unwrap_err();
    assert!(e.contains("sample start=0.1 lies past the end"), "{e}");
    assert!(bound("@k sample name=thump end=0.1\nA: @k c\n", &samples).is_ok());
    assert!(bound("@k sine\nA: @k c\n", &Samples::default()).is_ok());
    // Not found, an instrument plays nothing (a song read without its samples).
    assert!(Player::new(parse("@k sample name=thump attack=0 decay=0 sustain=1\nA: @k c\n").unwrap(), 1.0).render(Some(0.3)).iter().all(|v| *v == 0.0));
}

/// A strip's `root`, `start` and `end` count from the channel's next note:
/// the note that sounds keeps its own to the sample, the next plays as if
/// the instrument said so.
#[test]
fn a_samples_words_on_a_strip_count_from_the_next_note() {
    let ramp: Vec<f32> = (0..SAMPLE_RATE).map(|i| i as f32 / SR).collect();
    let samples = Samples::of(vec![recording("ramp", ramp), recording("tone", sine(220.0, 1.0, SAMPLE_RATE, 0.5))]);
    let song = |words: &str, name: &str| format!("#tempo 120\n@s sample name={name} {words} attack=0 decay=0 sustain=1 release=0.01 volume=1\nA: @s v15 o4 c2 c2\n");
    let played = |text: &str, turned: Option<(&str, f32)>| {
        let mut s = parse(text).unwrap();
        s.bind(&samples).unwrap();
        let mut p = Player::new(s, 1.0);
        let mut out = vec![0.0f32; SAMPLE_RATE as usize * 2 * 2];
        let half = SAMPLE_RATE as usize / 2;
        p.mix(&mut out[..half * 2]);
        if let Some((word, value)) = turned {
            let mut mix = Mix::default();
            assert!(std::sync::Arc::make_mut(&mut mix.strips).entry('A').or_default().turn(word, Some(value), 0.0));
            p.follow_mix(&mix);
        }
        p.mix(&mut out[half * 2..]);
        out.into_iter().step_by(2).collect::<Vec<f32>>()
    };
    let next = SAMPLE_RATE as usize;
    for (word, value, said, name) in [("start", 0.25f32, "start=0.25", "ramp"), ("end", 0.3, "end=0.3", "ramp"), ("root", note_hz("g3").unwrap(), "root=g3", "tone")] {
        let plain = played(&song("", name), None);
        let turned = played(&song("", name), Some((word, value)));
        let written = played(&song(said, name), None);
        assert!(turned[..next] == plain[..next], "{word} changed the sounding note");
        assert!(turned[next..] == written[next..], "{word}: the next note not as the instrument saying {said}");
        assert!(turned[next..] != plain[next..], "{word} changed nothing");
    }
    for word in ["name", "loop", "reverse"] {
        assert!(NO_KNOB.iter().any(|(w, _)| *w == word), "{word} is said to be no knob");
        assert!(!Strip::default().turn(word, Some(1.0), 0.0), "{word} is turned");
    }
}

/// A note's name as MML writes it reads as its pitch and back.
#[test]
fn a_note_reads_as_its_pitch_and_back() {
    assert!((note_hz("a4").unwrap() - 440.0).abs() < 1e-3);
    assert!((note_hz("c4").unwrap() - 261.626).abs() < 1e-2);
    assert_eq!(note_hz("f#3"), note_hz("g-3"));
    assert_eq!(note_hz("c+4"), note_hz("C#4"));
    for bad in ["", "h4", "c", "c10", "c#-4", "c+++4", "c4x"] {
        assert_eq!(note_hz(bad), None, "{bad}");
    }
    for name in ["c4", "f#3", "a0", "b9", "d#5"] {
        assert_eq!(note_name(note_hz(name).unwrap()), name);
    }
}

/// The converter keeps a tone at its pitch and level from any rate (a
/// sine of 1 kHz from 22050, 32000 and 48000 Hz within 0.3 dB, at 1 kHz),
/// and takes off what lies above the engine's Nyquist (22.5 kHz recorded
/// at 48000 Hz 40 dB down at least).
#[test]
fn the_converter_keeps_the_tone_and_takes_off_what_cannot_be_held() {
    let level = |s: &[f32]| (s[2000..s.len() - 2000].iter().map(|v| v * v).sum::<f32>() / (s.len() - 4000) as f32).sqrt() * std::f32::consts::SQRT_2;
    for rate in [22_050, 32_000, 48_000] {
        let out = resample(&sine(1000.0, 0.5, rate, 0.5), rate);
        assert!(out.len().abs_diff((0.5 * SR) as usize) <= 1, "{rate}: as long, {}", out.len());
        let db = 20.0 * (level(&out) / 0.5).log10();
        assert!(db.abs() < 0.3, "{rate}: {db:.2} dB");
        assert!((hz(&out, 0.05, 0.45) - 1000.0).abs() < 0.5, "{rate}: {} Hz", hz(&out, 0.05, 0.45));
    }
    let high = resample(&sine(22_500.0, 0.5, 48_000, 0.5), 48_000);
    assert!(level(&high) < 0.5 * 0.01, "22.5 kHz kept at {}", level(&high));
}

/// A game's samples folder: each WAV a sample by its file's name, other
/// files passed over, a recording of another kind a mistake saying so,
/// two files of one name a mistake, and all of them together past the
/// most a game holds a mistake naming the file that passes it (here a
/// limit of 1 MB, in place of `MOST_BYTES`).
#[test]
fn the_samples_folder_holds_wavs_within_its_limit() {
    let dir = crate::scratch::Scratch::new("sample-folder");
    std::fs::create_dir_all(dir.join("samples")).unwrap();
    // 48000 Hz, stereo, 24 bit: 288,000 bytes a second, 2 s each.
    let big = wav_file(48_000, 2, 24, false, &sine(100.0, 2.0, 48_000, 0.3).repeat(2));
    std::fs::write(dir.join("samples/a.wav"), &big).unwrap();
    std::fs::write(dir.join("samples/README.txt"), "made by the engine").unwrap();
    std::fs::write(dir.join("samples/.hidden.wav"), b"no").unwrap();
    let files = crate::files::Files::folder(&dir);
    let one = Samples::read_within(&files, 1 << 20).unwrap();
    assert_eq!(one.names().collect::<Vec<_>>(), ["a"]);
    std::fs::write(dir.join("samples/b.wav"), &big).unwrap();
    let e = format!("{:#}", Samples::read_within(&files, 1 << 20).unwrap_err());
    assert!(e.contains("samples/b.wav: the samples come to more than 1 MB together"), "{e}");
    assert!(Samples::read_folder(&files).is_ok(), "within 64 MB");
    std::fs::write(dir.join("samples/b.WAV"), &big).unwrap();
    let e = format!("{:#}", Samples::read_folder(&files).unwrap_err());
    assert!(e.contains("two samples called \"b\""), "{e}");
    std::fs::remove_file(dir.join("samples/b.WAV")).unwrap();
    std::fs::write(dir.join("samples/loop.ogg"), b"OggS").unwrap();
    let e = format!("{:#}", Samples::read_folder(&files).unwrap_err());
    assert!(e.contains("samples/loop.ogg: only WAV is read"), "{e}");
    std::fs::remove_file(dir.join("samples/loop.ogg")).unwrap();
    // The same files are the same samples; a byte changed is another.
    let (x, y) = (Samples::read_folder(&files).unwrap(), Samples::read_folder(&files).unwrap());
    assert!(x.same(&y));
    let mut other = big.clone();
    *other.last_mut().unwrap() ^= 1;
    std::fs::write(dir.join("samples/b.wav"), &other).unwrap();
    assert!(!x.same(&Samples::read_folder(&files).unwrap()));
}

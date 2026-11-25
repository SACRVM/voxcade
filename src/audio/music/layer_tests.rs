//! Instruments of layers (`+` lines under an instrument's own, a sum `=`),
//! a pitch that falls from a start in Hz (`pitch_start`, `pitch_decay`)
//! and envelopes that fall exponentially (`curve=exp`).

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

/// The loudest sample within `from..to` seconds.
fn peak(s: &[f32], from: f32, to: f32) -> f32 {
    s[(from * SR) as usize..(to * SR) as usize].iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// The largest step from one sample to the next within `from..to` seconds.
fn steepest(s: &[f32], from: f32, to: f32) -> f32 {
    s[(from * SR) as usize..(to * SR) as usize].windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()))
}

/// The largest difference between two renders.
fn apart(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()))
}

/// The layers of an instrument without a sum add up as the same layers
/// played on channels of their own do (to a float's rounding), each with
/// its own envelope; two noise layers are two noises, not one twice.
#[test]
fn layers_add_up_as_channels_do() {
    let words = ["sine freq=55 attack=0.001 decay=0.4 sustain=0 release=0.2 curve=exp volume=0.6", "saw freq=140 pitch_start=320 pitch_decay=0.012 attack=0.001 decay=0.09 sustain=0 release=0.05 drive=8 cutoff=900 volume=0.3", "triangle attack=0.002 decay=0.02 sustain=0.3 release=0.1 vibrato=0.02 volume=0.2"];
    let layered = format!("#tempo 120\n@k {}\n  + {}\n  + {}\nA: @k o4 l8 c e g r c4 r4\n", words[0], words[1], words[2]);
    let apart_ = format!("#tempo 120\n@a {}\n@b {}\n@c {}\nA: @a o4 l8 c e g r c4 r4\nB: @b o4 l8 c e g r c4 r4\nC: @c o4 l8 c e g r c4 r4\n", words[0], words[1], words[2]);
    let (one, three) = (left(&layered, 2.0), left(&apart_, 2.0));
    assert!(peak(&one, 0.0, 2.0) > 0.2, "it sounds");
    assert!(apart(&one, &three) < 1e-5, "the layers are not the channels' sum: {}", apart(&one, &three));
    let twice = left("#tempo 120\n@n noise freq=8000 decay=0 sustain=1 volume=0.5\n+ noise freq=8000 decay=0 sustain=1 volume=0.5\nA: @n c1\n", 0.5);
    let once = left("#tempo 120\n@n noise freq=8000 decay=0 sustain=1 volume=1\nA: @n c1\n", 0.5);
    assert!(apart(&twice, &once) > 0.1, "two noise layers play one noise twice");
}

/// A layered note sounds until its last layer has rung out, whichever
/// layer that is: a song played once is over only then.
#[test]
fn a_layered_note_sounds_until_its_last_layer_rang_out() {
    let text = "#tempo 240\n#loop no\n@k sine freq=200 attack=0.001 decay=0.02 sustain=0 release=0.01\n+ sine freq=300 attack=0.001 decay=0 sustain=1 release=0.4\nA: @k q8 c4\n";
    let mut p = Player::new(parse(text).unwrap(), 1.0);
    let mut out = vec![0.0; (0.5 * SR) as usize * 2];
    p.mix(&mut out);
    assert!(!p.over(), "over while its second layer still rings");
    let s: Vec<f32> = out.iter().step_by(2).copied().collect();
    assert!(peak(&s, 0.3, 0.4) > 0.05, "the long layer rings on after the first is silent");
    let mut more = vec![0.0; (0.5 * SR) as usize * 2];
    p.mix(&mut more);
    assert!(p.over(), "over once every layer has rung out");
}

/// A pitch that begins at `pitch_start` falls exponentially in Hz to the
/// note's: the first turn of the wave as quick as the fall worked out
/// sample by sample makes it, the turns late in the note the note's own.
#[test]
fn a_pitch_falls_from_its_start_to_the_notes() {
    let s = left("#tempo 60\n@k sine pitch_start=400 pitch_decay=0.02 attack=0 decay=0 sustain=1\nA: @k o1 a1\n", 1.0);
    let ups: Vec<usize> = (1..s.len()).filter(|&i| s[i - 1] < 0.0 && s[i] >= 0.0).collect();
    // The phase as the oscillator adds it up: a turn ends where it passes 1.
    let note = 55.0f64;
    let (mut phase, mut first) = (0.0f64, 0usize);
    for i in 0.. {
        let t = i as f64 / SAMPLE_RATE as f64;
        phase += (note + (400.0 - note) * (-t / 0.02).exp()) / SAMPLE_RATE as f64;
        if phase >= 1.0 {
            first = i;
            break;
        }
    }
    assert!(ups[0].abs_diff(first) <= 2, "the first turn ends at sample {}, the fall makes it {first}", ups[0]);
    assert!(first < (SR / 300.0) as usize, "the first turn is quick (from 400 Hz): {first}");
    // Late in the note: 55 Hz.
    let late: Vec<&usize> = ups.iter().filter(|&&i| i as f32 > 0.5 * SR && (i as f32) < 0.9 * SR).collect();
    let hz = (late.len() - 1) as f32 * SR / (*late[late.len() - 1] - *late[0]) as f32;
    assert!((hz - 55.0).abs() < 0.2, "late in the note {hz} Hz, not the note's 55");
}

/// `curve=exp`: a decay to silence is at -60 dB (within a decibel) just
/// before its time and silent after it, without a step at its end; a
/// release alike from where the key was let go, the note over after it;
/// halfway, -30 dB. `curve=linear` written is as unwritten.
#[test]
fn an_exponential_fall_is_at_minus_60_db_at_its_time_without_a_click() {
    let db = |a: f32, b: f32| 20.0 * (a / b).log10();
    let decay = left("#tempo 60\n@e sine freq=1000 attack=0.001 decay=0.4 sustain=0 release=0.01 curve=exp volume=1\nA: @e c1\n", 1.0);
    let top = peak(&decay, 0.0, 0.01);
    let end = 0.401;
    let near = db(peak(&decay, end - 0.003, end - 0.002), top);
    assert!((near + 60.0).abs() < 1.0, "the decay {near:.1} dB just before its time");
    let half = db(peak(&decay, end - 0.201, end - 0.199), top);
    assert!((half + 30.0).abs() < 1.0, "halfway {half:.1} dB");
    assert!(decay[(end * SR) as usize + 2..].iter().all(|v| *v == 0.0), "silent after its time");
    assert!(steepest(&decay, end - 0.01, end + 0.01) < 3e-4 * top, "a step at its end: {}", steepest(&decay, end - 0.01, end + 0.01) / top);

    let release = left("#tempo 60\n@e sine freq=1000 attack=0.001 decay=0 sustain=1 release=0.3 curve=exp volume=1\nA: @e q8 c4 r4\n", 2.0);
    let (top, end) = (peak(&release, 0.9, 1.0), 1.3);
    let near = db(peak(&release, end - 0.003, end - 0.002), top);
    assert!((near + 60.0).abs() < 1.0, "the release {near:.1} dB just before its time");
    assert!(release[(end * SR) as usize + 2..].iter().all(|v| *v == 0.0), "silent after its time");
    assert!(steepest(&release, end - 0.01, end + 0.01) < 3e-4 * top, "a step at its end");

    let plain = "#tempo 120\n@e saw attack=0.01 decay=0.2 sustain=0.4 release=0.1\nA: @e c4 e4\n";
    assert!(left(plain, 1.0) == left(&plain.replace("release=0.1", "release=0.1 curve=linear"), 1.0));
}

/// The sum (`=`): its volume turns the added layers down, its drive holds
/// them about full scale however loud they add up (its DC blocker
/// overshooting a little on a clipped edge, as a layer's own), its filter
/// cuts them all.
#[test]
fn the_sum_drives_filters_and_turns_down() {
    let layers = "#tempo 120\n@k saw freq=220 attack=0.001 decay=0 sustain=1 volume=1\n+ saw freq=220 attack=0.001 decay=0 sustain=1 volume=1\n";
    let play = |sum: &str| left(&format!("{layers}{sum}A: @k v15 c1\n"), 0.5);
    let plain = play("");
    let half = play("= volume=0.5\n");
    assert!(plain.iter().zip(&half).all(|(a, b)| (a * 0.5 - b).abs() < 1e-6), "volume=0.5 is not half");
    assert!(peak(&plain, 0.1, 0.4) > 1.3, "the layers add up past full scale");
    let driven = play("= drive=18\n");
    assert!(peak(&driven, 0.1, 0.4) < 0.6 * peak(&plain, 0.1, 0.4), "the sum's drive does not hold them: {}", peak(&driven, 0.1, 0.4));
    // The bends from sample to sample weigh the highs: the saw's edges.
    let dark = play("= cutoff=300\n");
    let edge = |s: &[f32]| s.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).sum::<f32>();
    assert!(edge(&dark) < 0.2 * edge(&plain), "the sum's low-pass does not cut");
}

/// The mistakes of layers and sums say their line: a layer or a sum with
/// no instrument above, a fifth layer, a second sum, a layer after the
/// sum, a word the sum does not take, layers on a voice, a voice as a
/// layer, a word of the whole note on a layer, a pitch's start beside a
/// sweep, a curve of neither kind. Comments and blank lines may stand
/// between an instrument and its layers.
#[test]
fn the_mistakes_of_layers_say_their_line() {
    let wrong = |text: &str| format!("{:#}", parse(text).unwrap_err());
    for (text, says) in [
        ("+ sine\n@k sine\nA: @k c\n", "line 1: a layer (+) goes right under an instrument's line"),
        ("@k sine\nA: @k c\n+ sine\n", "line 3: a layer (+) goes right under an instrument's line"),
        ("@k sine\n#tempo 90\n+ sine\nA: @k c\n", "line 3: a layer (+)"),
        ("= drive=2\nA: c\n", "line 1: a sum (=) goes right under an instrument's line"),
        ("@k sine\n+ sine\n+ sine\n+ sine\n+ sine\nA: @k c\n", "line 5: @k has 4 layers already"),
        ("@k sine\n+ sine\n= drive=3\n= volume=0.5\nA: @k c\n", "line 4: @k has its sum (=) on line 3 already: an instrument takes one"),
        ("@k sine\n= drive=3\n+ sine\nA: @k c\n", "line 3: @k has its sum (=) on line 2 already, and its layers come before it"),
        ("@k sine\n+ sine\n= decay=0.3\nA: @k c\n", "line 3: @k's sum (=): the sum takes drive, drive_end, drive_time, volume, filter, cutoff, resonance, crush_rate, crush_bits, lfo_speed, lfo_beats, lfo_wave, lfo_cutoff, lfo_volume, not decay"),
        ("@k sine\n= drve=3\nA: @k c\n", "not drve (did you mean drive?)"),
        ("@v singer\n+ sine\nA: @v \"l a\" c\n", "line 2: @v sings, and a voice takes no layers"),
        ("@k sine\n+ voice\nA: @k c\n", "line 2: @k's layer 2: a layer is a wave"),
        ("@k sine\n+ sine duck=0\nA: @k c\n", "line 2: @k's layer 2: duck is the whole note's"),
        ("@k sine sweep=-12 pitch_start=200\nA: @k c\n", "line 1: @k: pitch_start and sweep both bend the pitch"),
        ("@k sine\n+ sine pitch_start=300 sweep=0\nA: @k c\n", "line 2: @k's layer 2: pitch_start and sweep"),
        ("@k sine curve=log\nA: @k c\n", "line 1: @k: curve=log: linear or exp"),
        ("@k sine pitch_decay=3\nA: @k c\n", "pitch_decay=3 is out of range, from 0.001 to 2 seconds"),
    ] {
        let e = wrong(text);
        assert!(e.contains(says), "{text:?}: {e}");
    }
    let song = parse("@k sine\n; the body\n\n   + triangle freq=140\n\t= drive=2\nA: @k c\n").unwrap();
    assert_eq!(song.instruments[0].layers.len(), 1);
    assert!(song.instruments[0].sum.as_ref().is_some_and(|s| s.drive_stated && s.volume == 1.0));
}

/// A layered note held over the song's end is let go there and rings on
/// in its channel's tail, every layer, while the next round begins on time:
/// no click at the seam.
#[test]
fn a_layered_note_rings_over_the_seam() {
    let head = "#tempo 240\n@a sine attack=0.005 decay=0 sustain=1 release=0.05\n+ triangle attack=0.005 decay=0 sustain=1 release=0.08 curve=exp volume=0.3\n";
    let bar = SAMPLE_RATE as usize;
    let jump = |s: &[f32], at: usize| s[at - 500..at + 500].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let rms = |s: &[f32], from: usize, to: usize| (s[from..to].iter().map(|x| x * x).sum::<f32>() / (to - from) as f32).sqrt();
    let within = left(&format!("{head}A: @a q8 o4 c1 r1\n"), 2.0);
    let round = left(&format!("{head}A: @a q8 o4 c1\n"), 2.5);
    assert!(jump(&round, bar) <= 2.5 * jump(&within, bar), "a step of {} at the seam, {} within a song", jump(&round, bar), jump(&within, bar));
    assert!(rms(&round, bar + 1000, bar + 1500) > 0.1 * rms(&round, bar - 1500, bar - 1000), "no release after the seam");
    let mut p = Player::new(parse(&format!("{head}A: @a q8 o4 c1\n")).unwrap(), 1.0);
    let mut out = vec![0.0; (bar + 10) * 2];
    p.mix(&mut out);
    assert_eq!((p.voices[0].note.map(|n| n.start), p.tails[0].note.map(|n| n.start)), (Some(0), Some(0)));
}

/// A layered note keeps its shape at another tempo (each layer's fall and
/// pitch by the samples heard), and the next note on its channel takes the
/// voice over as from a plain one: once the kick has faded out under it
/// (`GIVE_WAY`, 5 ms), the same samples.
#[test]
fn a_layered_note_keeps_its_shape_at_another_tempo_and_gives_way() {
    let kick = "@k sine freq=50 pitch_start=180 pitch_decay=0.022 attack=0.001 decay=0.6 sustain=0 release=0.4 curve=exp\n+ sine freq=140 pitch_start=320 pitch_decay=0.012 attack=0.001 decay=0.09 sustain=0 curve=exp drive=8 volume=0.5\n+ noise freq=20000 filter=bandpass cutoff=2100 attack=0.0005 decay=0.02 sustain=0 curve=exp volume=0.3\n= drive=4\n";
    let text = format!("#tempo 60\n{kick}A: @k c1\n");
    let render = |ratio: f64| {
        let mut p = Player::new(parse(&text).unwrap(), 1.0);
        p.set_ratio(ratio);
        p.render(Some(0.5))
    };
    assert!(render(1.0) == render(2.0), "the first half second, before the key is let go at either tempo");
    // A plain note after the kick plays as after a plain one.
    let after = |first: &str| left(&format!("#tempo 120\n{kick}@p sine attack=0.002 decay=0 sustain=1 volume=0.5\n@q sine freq=50 volume=0.4\nA: {first} @p o4 c4\n"), 1.0);
    let (a, b) = (after("@k c4"), after("@q c4"));
    let faded = (0.505 * SR) as usize;
    assert!(a[faded..] == b[faded..], "the kick's layers sound on under the next note past their fade");
}

/// A layered note of a channel a game laid rings out in a tail of its own
/// when a swap lays the channel no more, every layer, without a click.
#[test]
fn a_laid_layered_note_rings_out_through_a_swap() {
    let studio = "#tempo 120\n@k sine freq=60 attack=0.001 decay=0 sustain=1 release=0.3 curve=exp\n+ triangle freq=180 attack=0.001 decay=0 sustain=1 release=0.3 volume=0.4\n@p triangle attack=0.002 decay=0 sustain=1 release=0.01\n";
    let laid = |channels: &[(char, &str)]| Laid::new(channels.iter().map(|(c, t)| (*c, t.to_string())).collect());
    let bar = 4 * 22_050;
    let mut p = Player::new(lay(studio, &laid(&[('K', "@k o4 c1"), ('H', "r1")])).unwrap(), 1.0);
    drop(p.pend(Some(Swap::new(lay(studio, &laid(&[('H', "r1")])).unwrap(), 2.0))));
    let mut out = vec![0.0; 2 * bar * 2];
    p.mix(&mut out);
    let s: Vec<f32> = out.iter().step_by(2).copied().collect();
    let half = bar / 2;
    let rms = |from: usize, to: usize| (s[from..to].iter().map(|x| x * x).sum::<f32>() / (to - from) as f32).sqrt();
    assert!(rms(half + 500, half + 2500) > 0.2 * rms(half - 2500, half - 500), "the kick does not ring out after the swap");
    let step = s[half - 200..half + 200].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let own = s[half - 2200..half - 1800].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(step <= 1.5 * own, "a step of {step} at the swap, {own} before it");
}

/// `music:mix`'s words act on a layered instrument's first layer, but
/// drive, cutoff and resonance on the sum's where its `=` line states
/// them: the same as the file saying so there.
#[test]
fn a_strip_turns_the_first_layer_or_the_sum() {
    let song = |head: &str, sum: &str| format!("#tempo 120\n@k sine freq=60 attack=0.001 decay=0.3 sustain=0.2 release=0.1 filter=lowpass cutoff=2000 {head}\n+ saw freq=180 attack=0.001 decay=0.1 sustain=0 cutoff=1500\n= volume=0.8 {sum}\nA: @k c4 c4 c4 c4\n");
    let close = |a: &[f32], b: &[f32], what: &str| assert!(apart(a, b) < 1e-4, "{what}: {}", apart(a, b));
    let same = |a: &[f32], b: &[f32], what: &str| assert!(a == b, "{what}: {}", apart(a, b));
    // The sum states them: the strip turns the sum's.
    let stated = song("", "drive=4 cutoff=3000 resonance=0.2");
    same(&mixed(&stated, 2.0, &turned('A', "drive", 12.0)), &left(&song("", "drive=12 cutoff=3000 resonance=0.2"), 2.0), "drive to the sum");
    close(&mixed(&stated, 2.0, &turned('A', "cutoff", 800.0)), &left(&song("", "drive=4 cutoff=800 resonance=0.2"), 2.0), "cutoff to the sum");
    same(&mixed(&stated, 2.0, &turned('A', "resonance", 0.7)), &left(&song("", "drive=4 cutoff=3000 resonance=0.7"), 2.0), "resonance to the sum");
    same(&mixed(&stated, 2.0, &turned('A', "sustain", 0.6)), &left(&song("sustain=0.6", "drive=4 cutoff=3000 resonance=0.2"), 2.0), "sustain to the first layer");
    same(&mixed(&stated, 2.0, &turned('A', "decay", 0.5)), &left(&song("decay=0.5", "drive=4 cutoff=3000 resonance=0.2"), 2.0), "decay to the first layer");
    // The sum states none of them: the first layer's.
    let unstated = song("", "");
    same(&mixed(&unstated, 2.0, &turned('A', "drive", 12.0)), &left(&song("drive=12", ""), 2.0), "drive to the first layer");
    close(&mixed(&unstated, 2.0, &turned('A', "cutoff", 800.0)), &left(&song("cutoff=800", ""), 2.0), "cutoff to the first layer");
    same(&mixed(&unstated, 2.0, &turned('A', "resonance", 0.7)), &left(&song("resonance=0.7", ""), 2.0), "resonance to the first layer");
    // The second layer keeps its own.
    assert!(mixed(&unstated, 2.0, &turned('A', "cutoff", 800.0)) != left(&song("cutoff=800", "").replace("cutoff=1500", "cutoff=800"), 2.0));
}

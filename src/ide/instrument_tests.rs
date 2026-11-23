//! The instrument maker's tests: the instrument at the cursor, rolls,
//! faders, the measures and a note's pitch.

use super::*;
use crate::audio::music::Samples;

const KIT: &str = "; a kit\n#title Kit\n#tempo 125\n\n; the kick\n@kick sine freq=49.5 pitch_start=100 pitch_decay=0.035 attack=0.001 decay=0.9 sustain=0 release=0.9 curve=exp volume=0.9 ; the sub\n  + sine freq=130 pitch_start=260 pitch_decay=0.01 attack=0.001 decay=0.08 sustain=0 release=0.08 curve=exp drive=6 volume=0.4\n  ; the click\n  + noise freq=20000 filter=bandpass cutoff=1500 resonance=0.2 attack=0.0005 decay=0.08 sustain=0 release=0.08 curve=exp volume=0.08\n  = drive=3\n\n@lead   square duty=0.5 attack=0.01 decay=0.3 sustain=0.55 release=0.25 lowpass=1600 volume=0.3\n@hat noise freq=20000 filter=highpass cutoff=7000 decay=0.05 sustain=0 release=0.005 volume=0.4\n";

const SONG: &str = "#tempo 120\n@bass triangle attack=0.01 decay=0.3 sustain=0.7 release=0.2 volume=0.6\n@lead square lowpass=2000 volume=0.3\n@tone sine attack=0.001 decay=0.5 sustain=1 release=0.1 volume=0.5\n$riff = @tone c d\nA: @lead o5 l8 c d e f\nB: @bass o2 l4 c c\nA: g a @bass b\n";

/// The rows (from 0) that differ between two texts of as many lines.
fn changed_rows(a: &str, b: &str) -> Vec<usize> {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.split('\n').collect(), b.split('\n').collect());
    assert_eq!(a.len(), b.len());
    (0..a.len()).filter(|&i| a[i] != b[i]).collect()
}

fn samples() -> Samples {
    Samples::default()
}

/// The meters, ported, read the kit's kick as its comment says they read
/// it (KNALL -42 dB, BALLERN -14.3 dB, BRUSTKORB 13 ms).
#[test]
fn the_kits_kick_reads_as_its_comment_measured_it() {
    let block = named(KIT, "kick").unwrap();
    let m = kick_meters(&note(KIT, &block, 4, 0, &samples()).unwrap());
    assert!((m.crack + 42.0).abs() < 0.5 && (m.slam + 14.3).abs() < 0.3 && (m.chest - 13.0).abs() < 1.0, "{m:?}");
    // The zone turns away a sine with no fall, no body and no click.
    let thin = "@k sine freq=50 attack=0.001 decay=0.5 sustain=0 release=0.5 curve=exp volume=0.9\n";
    let m = kick_meters(&note(thin, &named(thin, "k").unwrap(), 4, 0, &samples()).unwrap());
    assert!(m.inside().is_err(), "{m:?}");
}

/// The cursor on an instrument's line, a layer's, the sum's or a comment
/// between is in that instrument; on a channel's notes in the one the
/// channel plays there; elsewhere in the last above, before the first in
/// none.
#[test]
fn the_cursor_finds_its_instrument() {
    let at = |text: &str, row: usize, col: usize| at_cursor(text, char_at(text, row, col)).map(|b| b.name);
    assert_eq!(at(KIT, 0, 3), None, "the first line, before any");
    assert_eq!(at(KIT, 5, 0), Some("kick".into()));
    let kick = named(KIT, "kick").unwrap();
    assert_eq!(kick.lines, vec![(5, Part::Own), (6, Part::Layer), (8, Part::Layer), (9, Part::Sum)]);
    for row in [6, 7, 8, 9, 10] {
        assert_eq!(at(KIT, row, 2), Some("kick".into()), "row {row}");
    }
    assert_eq!(at(KIT, 11, 5), Some("lead".into()));
    assert_eq!(named(KIT, "lead").unwrap().lines, vec![(11, Part::Own)]);
    // On a channel: the instrument it plays at the cursor.
    assert_eq!(at(SONG, 5, 3), Some("tone".into()), "a channel before its first @: the last above");
    assert_eq!(at(SONG, 5, 12), Some("lead".into()));
    assert_eq!(at(SONG, 6, 20), Some("bass".into()));
    assert_eq!(at(SONG, 7, 4), Some("lead".into()), "A goes on from its line before");
    assert_eq!(at(SONG, 7, 12), Some("bass".into()));
    assert_eq!(at(SONG, 4, 12), Some("tone".into()), "a macro");
    assert_eq!(octave_used(SONG, "lead"), Some(5));
    assert_eq!(octave_used(SONG, "bass"), Some(2), "its first note in the file: B's before A's second line");
    assert_eq!(octave_used(SONG, "tone"), None);
    assert_eq!(octave_used("A: o3 @x c < @y d\n", "y"), Some(2));
}

/// Every sort rolled 200 times: each roll reads as a song, passes its
/// measure (a KICK in the owner's kick zone), changes only the
/// instrument's lines and keeps its name; the rolls differ.
#[test]
fn every_sort_rolls_only_good_instruments() {
    let text = "; mine\n#tempo 125\n@drum square volume=0.3 ; keep this\nA: @drum o4 c4 c4\n";
    let block = named(text, "drum").unwrap();
    for sort in Sort::ALL {
        let mut texts = std::collections::BTreeSet::new();
        for seed in 0..200u64 {
            let (specs, new) = roll(text, &block, sort, seed * 7919 + 1, sort.octave());
            crate::audio::music::read(&new).unwrap_or_else(|e| panic!("{sort:?} seed {seed}: {e:#}\n{new}"));
            let fresh = named(&new, "drum").expect("the name kept");
            assert_eq!(fresh.lines.len(), specs.len());
            let played = note(&new, &fresh, sort.octave(), 0, &samples()).unwrap();
            good(&played, sort).unwrap_or_else(|e| panic!("{sort:?} seed {seed}: {e}\n{new}"));
            let (old_lines, new_lines): (Vec<&str>, Vec<&str>) = (text.split('\n').collect(), new.split('\n').collect());
            assert_eq!(new_lines[..2], old_lines[..2]);
            assert_eq!(new_lines[new_lines.len() - 2..], old_lines[old_lines.len() - 2..], "{new}");
            assert!(new_lines[2].starts_with("@drum ") && new_lines[2].ends_with(" ; keep this"), "{new}");
            texts.insert(new);
        }
        assert!(texts.len() >= 190, "{sort:?}: only {} different in 200", texts.len());
        assert_eq!(roll(text, &block, sort, 42, 4).1, roll(text, &block, sort, 42, 4).1, "a seed rolls the same");
    }
}

/// A roll writes the instrument anew: its name and the room after it,
/// each line's comment and the comments between kept, a layer more added
/// under the last, one less and the sum taken out; the rest byte for byte.
#[test]
fn a_roll_rewrites_only_the_instruments_lines() {
    let kick = named(KIT, "kick").unwrap();
    let hat = vec![Spec::new(Part::Own, Some("noise"), vec![("freq", 20_000.0), ("cutoff", 6000.0), ("decay", 0.2), ("sustain", 0.0)], vec![("filter", "highpass")])];
    let out = rolled(KIT, &kick, &hat);
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(lines[5], "@kick noise freq=20000 decay=0.2 sustain=0 filter=highpass cutoff=6000 ; the sub");
    assert_eq!(lines[6], "  ; the click", "the layers and the sum gone, the comment between kept");
    let mut expected: Vec<&str> = KIT.split('\n').collect();
    expected[5] = lines[5];
    expected.retain(|l| !l.starts_with("  +") && !l.starts_with("  ="));
    assert_eq!(out, expected.join("\n"));
    // The lead becomes a kick: layers and a sum under it, as indented as a
    // layer is elsewhere... here none: two spaces.
    let lead = named(KIT, "lead").unwrap();
    let (_, kicked) = roll(KIT, &lead, Sort::Kick, 3, 4);
    let block = named(&kicked, "lead").unwrap();
    assert!(block.lines.len() >= 2, "{kicked}");
    assert!(kicked.split('\n').nth(11).unwrap().starts_with("@lead   sine "), "the room after the name kept");
    assert!(kicked.split('\n').nth(block.lines.last().unwrap().0).unwrap().starts_with("  = drive="));
    assert!(kicked.ends_with("@hat noise freq=20000 filter=highpass cutoff=7000 decay=0.05 sustain=0 release=0.005 volume=0.4\n"));
    // The cursor after the instrument moves on with the lines it gained.
    let at = char_at(KIT, 12, 4);
    assert_eq!(row_col(&kicked, cursor_after(KIT, &kicked, &lead, at)), (block.lines.last().unwrap().0 + 1, 4));
    assert_eq!(row_col(&kicked, cursor_after(KIT, &kicked, &lead, char_at(KIT, 11, 9))), (11, 0));
    assert_eq!(cursor_after(KIT, &kicked, &lead, 5), 5);
}

/// Each fader moves the words it says, on the lines it says, in place;
/// every other word and line stays byte for byte.
#[test]
fn faders_move_only_their_words() {
    let kick = named(KIT, "kick").unwrap();
    let words = |text: &str, row: usize| -> Vec<(String, Option<String>)> { tokens(text.split('\n').nth(row).unwrap()).into_iter().map(|t| (t.key, t.value)).collect() };
    let differ = |a: &str, b: &str, row: usize| -> Vec<String> {
        let (x, y) = (words(a, row), words(b, row));
        let mut keys: Vec<String> = x.iter().chain(&y).filter(|w| !x.contains(w) || !y.contains(w)).map(|w| w.0.clone()).collect();
        keys.sort();
        keys.dedup();
        keys
    };
    let hz = 261.6;
    // LENGTH: decay and release of the own line, their ratio kept.
    let out = Fader::Length.moved(KIT, &kick, hz, 0.9);
    assert_eq!(changed_rows(KIT, &out), [5]);
    assert_eq!(differ(KIT, &out, 5), ["decay", "release"]);
    assert!(out.contains("decay=0.45 sustain=0 release=0.45 curve=exp volume=0.9 ; the sub"), "{out}");
    // PUNCH: every falling line's pitch_start and attack.
    let out = Fader::Punch.moved(KIT, &kick, hz, 0.6);
    assert_eq!(changed_rows(KIT, &out), [5, 6]);
    assert_eq!(differ(KIT, &out, 5), ["attack", "pitch_start"]);
    let (st, _) = Fader::Punch.place(&out, &named(&out, "kick").unwrap(), hz).unwrap();
    assert!((st - 0.6).abs() < 0.01, "{st}");
    // BRIGHT: the first line with a filter, the click's cutoff.
    let out = Fader::Bright.moved(KIT, &kick, hz, 2400.0);
    assert_eq!(changed_rows(KIT, &out), [8]);
    assert_eq!(differ(KIT, &out, 8), ["cutoff"]);
    assert!(out.contains("cutoff=2400 resonance"));
    // DRIVE and LEVEL: the sum's.
    let out = Fader::Drive.moved(KIT, &kick, hz, 7.5);
    assert_eq!(changed_rows(KIT, &out), [9]);
    assert!(out.contains("\n  = drive=7.5\n"), "{out}");
    let out = Fader::Level.moved(KIT, &kick, hz, 0.8);
    assert!(out.contains("\n  = volume=0.8 drive=3\n"), "a new word in the register's order: {out}");
    // Without a sum: DRIVE the own line's, LEVEL every line's.
    let lead = named(KIT, "lead").unwrap();
    let out = Fader::Drive.moved(KIT, &lead, hz, 6.0);
    assert_eq!(changed_rows(KIT, &out), [11]);
    assert_eq!(differ(KIT, &out, 11), ["drive"]);
    let out = Fader::Level.moved(KIT, &lead, hz, 0.6);
    assert!(out.contains("lowpass=1600 volume=0.6\n"));
    // BRIGHT on the lead's lowpass; PUNCH on its attack alone.
    assert!(Fader::Bright.moved(KIT, &lead, hz, 900.0).contains("lowpass=900 volume=0.3"));
    let out = Fader::Punch.moved(KIT, &lead, hz, 1.0);
    assert_eq!(differ(KIT, &out, 11), ["attack"]);
    assert!(out.contains("attack=0.0005 decay"), "{out}");
    // A sine alone has no BRIGHT; a line without a filter gets one.
    let sine = "@s sine decay=0.2\n";
    assert!(Fader::Bright.place(sine, &named(sine, "s").unwrap(), hz).is_err());
    let saw = "@s saw decay=0.2 ; mine\n";
    assert_eq!(Fader::Bright.moved(saw, &named(saw, "s").unwrap(), hz, 3000.0), "@s saw decay=0.2 cutoff=3000 ; mine\n");
    // Every fader at either end and between writes what reads.
    for name in ["kick", "lead", "hat"] {
        let block = named(KIT, name).unwrap();
        for fader in Fader::ALL {
            let (lo, hi) = fader.range();
            for k in 0..=6 {
                let to = if fader.logarithmic() { lo * (hi / lo).powf(k as f64 / 6.0) } else { lo + (hi - lo) * k as f64 / 6.0 };
                let out = fader.moved(KIT, &block, hz, to);
                crate::audio::music::read(&out).unwrap_or_else(|e| panic!("{name} {fader:?} {to}: {e:#}\n{out}"));
            }
        }
    }
}

/// A key plays the instrument at its note: a sine's period at C3 and C5,
/// an octave apart twice.
#[test]
fn a_key_plays_its_note() {
    let block = named(SONG, "tone").unwrap();
    let pitch = |octave: i32, semitone: usize| {
        let s = note(SONG, &block, octave, semitone, &samples()).unwrap();
        assert!(s.iter().fold(0.0f32, |m, x| m.max(x.abs())) > 0.1, "not silent");
        // Upward zero crossings over 0.1 s to 0.3 s.
        let (a, b) = (SAMPLE_RATE as usize / 10, SAMPLE_RATE as usize * 3 / 10);
        let ups: Vec<usize> = (a..b).filter(|&i| s[i - 1] < 0.0 && s[i] >= 0.0).collect();
        (ups.len() - 1) as f64 * SAMPLE_RATE as f64 / (ups[ups.len() - 1] - ups[0]) as f64
    };
    let (c3, c5) = (pitch(3, 0), pitch(5, 0));
    assert!((c3 - 130.81).abs() < 0.5, "{c3}");
    assert!((c5 - 523.25).abs() < 1.0, "{c5}");
    assert!((pitch(4, 9) - 440.0).abs() < 1.0, "A4");
    // A quarter at the song's tempo, then the release: about 0.6 s.
    let s = note(SONG, &block, 4, 0, &samples()).unwrap();
    let seconds = s.len() as f64 / SAMPLE_RATE as f64;
    assert!((0.45..0.75).contains(&seconds), "{seconds}");
    let song = note_song(SONG, &block, 4, 1);
    assert_eq!(song, "#tempo 120\n#loop no\n@tone sine attack=0.001 decay=0.5 sustain=1 release=0.1 volume=0.5\nA: @tone o4 l4 c+\n");
}
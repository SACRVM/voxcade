//! Channels a game lays over a studio (`parse::lay`) and the swap of them
//! at a sample (`Player::pend`).

use super::*;

/// A studio at 120 beats a minute, so a beat is 22,050 samples and an
/// eighth 11,025, exactly: a short square, a held triangle, a ringing one.
const STUDIO: &str = "#tempo 120\n@k square attack=0.002 decay=0.05 sustain=0.5 release=0.01\n@p triangle attack=0.002 decay=0 sustain=1 release=0.01\n@r triangle attack=0.002 decay=0 sustain=1 release=0.3\n";
const BAR: usize = 4 * 22_050;

fn laid(channels: &[(char, &str)]) -> Laid {
    Laid::new(channels.iter().map(|(c, t)| (*c, t.to_string())).collect())
}

fn player(channels: &[(char, &str)]) -> Player {
    Player::new(lay(STUDIO, &laid(channels)).unwrap(), 1.0)
}

fn rendered(mut p: Player, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    p.mix(&mut out);
    out
}

/// A studio reads as a song without a channel, which no player picks
/// alone; one with `#loop` is a mistake (it goes round what is laid
/// whole); a channel line makes a file a song.
#[test]
fn a_studio_is_a_file_of_instruments() {
    let studio = read(STUDIO).unwrap();
    assert!(studio.studio() && studio.tempo == 120.0);
    assert!(!has_channels(STUDIO) && has_channels("@k square\nK: c1 ; a channel\n") && !has_channels("; K: c1\n#tempo 90\n"));
    let e = format!("{:#}", read(&format!("{STUDIO}#loop 2\n")).unwrap_err());
    assert!(e.contains("line 5: #loop: a file of instruments"), "{e}");
    assert!(parse(STUDIO).is_err(), "parse still wants a song");
    // A studio's layers name letters only laid, laid now or not.
    let layered = format!("{STUDIO}#layer H 0.5\n");
    assert!(read(&layered).is_ok() && lay(&layered, &laid(&[('K', "c1")])).is_ok());
    let e = format!("{:#}", lay("@k square\nA: c1\n", &laid(&[('K', "c1")])).unwrap_err());
    assert!(e.contains("channels of its own"), "{e}");
}

/// The channels' digest is of their texts in their letters' order, each
/// with its length: the same set the same digest whichever order a table
/// gave them in; a letter or a byte moved, another.
#[test]
fn the_laid_channels_digest_is_of_their_texts_in_order() {
    let a = laid(&[('K', "c1"), ('H', "r1")]);
    let b = laid(&[('H', "r1"), ('K', "c1")]);
    assert_eq!(a.digest, b.digest);
    assert_eq!(a.channels[0].0, 'H', "sorted by letter");
    assert_ne!(a.digest, laid(&[('K', "r1"), ('H', "c1")]).digest);
    assert_ne!(laid(&[('H', "c1"), ('K', "c")]).digest, laid(&[('H', "c"), ('K', "1c")]).digest, "lengths keep the texts apart");
}

/// Whether laid channels fill whole bars is counted exactly, in beats (a
/// third twelve times is four beats; dots and ties add exactly), and the
/// mistakes name the channel, not a line: a part of a bar, channels of
/// other bars, a channel's own tempo, a singing voice.
#[test]
fn laid_channels_fill_whole_bars_exactly() {
    for fills in ["l12 c c c c c c c c c c c c", "l3 c c c", "c4. c8 c2", "c4&8 c8 c2", "[l192 c]192", "[c16]16 [r16]16", "l24 [c c c]8"] {
        assert!(lay(STUDIO, &laid(&[('K', fills)])).is_ok(), "{fills}");
    }
    let wrong = |channels: &[(char, &str)]| format!("{:#}", lay(STUDIO, &laid(channels)).unwrap_err());
    assert_eq!(wrong(&[('K', "c4 c4 c4")]), "channel K lasts 3 beats: a laid channel fills whole bars of 4 beats");
    assert_eq!(wrong(&[('K', "c4 c4 c4 c4.")]), "channel K lasts 4 and 1/2 beats: a laid channel fills whole bars of 4 beats");
    assert_eq!(wrong(&[('K', "l12 c c c c c c c c c c c")]), "channel K lasts 3 and 2/3 beats: a laid channel fills whole bars of 4 beats");
    assert_eq!(wrong(&[('K', "c1"), ('H', "c1 c1")]), "channel K fills 1 bar, channel H 2 bars: every laid channel fills as many bars");
    assert!(wrong(&[('K', "t140 c1")]).starts_with("channel K, near 't140': t sets a channel's own tempo"), "{}", wrong(&[('K', "t140 c1")]));
    assert!(wrong(&[('K', "@kik c1")]).contains("no instrument @kik"));
    let singing = format!("{STUDIO}@v singer\n");
    let e = format!("{:#}", lay(&singing, &laid(&[('V', "@v \"l a\" c1")])).unwrap_err());
    assert!(e.contains("@v sings (singer), which a channel a game lays does not yet"), "{e}");
    // Lines are only a file's: none named for a laid channel.
    assert!(!wrong(&[('K', "c1 x")]).contains("line"));
}

/// A note of a laid one-bar pattern still ringing as the bar goes round
/// rings on over the seam (its release), never cut.
#[test]
fn a_laid_pattern_rings_over_its_seam() {
    let s = rendered(player(&[('K', "@r o4 l4 r r r c")]), 2 * BAR);
    let after: f32 = s[BAR * 2..(BAR + 2_205) * 2].iter().fold(0.0, |m, v| m.max(v.abs()));
    assert!(after > 0.05, "the last beat's note rings over the line ({after})");
    let step = (s[BAR * 2] - s[BAR * 2 - 2]).abs();
    assert!(step < 0.05, "no click at the line ({step})");
}

/// A change swapped in at a bar line lands on its sample: the pattern
/// played and then the next sounds exactly as one song of both written
/// out, from its start; a channel laid the same plays on untouched,
/// sample for sample, while another changes under it, mid-note; a
/// pattern of other bars is folded by the beats since the song began.
#[test]
fn a_change_lands_on_its_sample() {
    let (first, next) = ("@k o4 l8 q4 c r e r g r e r", "@k o4 l8 q4 g r g r > c r < b r");
    let mut swapped = player(&[('K', first)]);
    drop(swapped.pend(Some(Swap::new(lay(STUDIO, &laid(&[('K', next)])).unwrap(), 4.0))));
    let whole = Player::new(parse(&format!("{STUDIO}K: {first} {next} {next}\n")).unwrap(), 1.0);
    let (a, b) = (rendered(swapped, 3 * BAR), rendered(whole, 3 * BAR));
    assert!(a[BAR * 2..].iter().any(|v| v.abs() > 0.1), "the next pattern plays");
    assert!(a == b, "as one song of both: first differs at {:?}", a.iter().zip(&b).position(|(x, y)| x != y));

    // H holds one note a whole bar; K changes from one silence to another
    // halfway through it, at beat 2.
    let pad = "@p o3 c1";
    let mut changed = player(&[('H', pad), ('K', "r1")]);
    let to = laid(&[('H', pad), ('K', "r2 r2")]);
    drop(changed.pend(Some(Swap::new(lay(STUDIO, &to).unwrap(), 2.0))));
    let mut out = vec![0.0; 3 * BAR];
    changed.mix(&mut out);
    assert_eq!(changed.laid(), (Some(to.digest), None), "swapped");
    let mut rest = vec![0.0; 3 * BAR];
    changed.mix(&mut rest);
    out.extend(rest);
    let alone = rendered(player(&[('H', pad), ('K', "r1")]), 3 * BAR);
    assert!(out == alone, "the same channel untouched: first differs at {:?}", out.iter().zip(&alone).position(|(x, y)| x != y));

    // Two bars swapped in at bar 3 (beat 8) of a one-bar loop: it plays
    // its own bar 1 there (8 beats are two of its rounds), then its bar 2.
    let mut longer = player(&[('K', "@p o4 c1")]);
    drop(longer.pend(Some(Swap::new(lay(STUDIO, &laid(&[('K', "@p o4 e1 g1")])).unwrap(), 8.0))));
    let s = rendered(longer, 4 * BAR);
    let hz = |from: usize| {
        let x: Vec<f32> = s[from * 2..(from + 11_025) * 2].iter().step_by(2).copied().collect();
        let crossings = x.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        crossings as f32 / 2.0 / 0.25
    };
    let (c4, e4, g4) = (hz(BAR / 2), hz(2 * BAR + BAR / 2), hz(3 * BAR + BAR / 2));
    assert!((c4 - 261.6).abs() < 8.0 && (e4 - 329.6).abs() < 8.0 && (g4 - 392.0).abs() < 8.0, "{c4} {e4} {g4}");
}

/// A swap called off before it was due never sounds; one asked for a
/// moment already past swaps at once (a state brought back), and what it
/// let go of waits for the main thread.
#[test]
fn a_swap_called_off_never_sounds() {
    let (first, next) = (laid(&[('K', "@k o4 c1")]), laid(&[('K', "@k o5 c1")]));
    let mut p = Player::new(lay(STUDIO, &first).unwrap(), 1.0);
    drop(p.pend(Some(Swap::new(lay(STUDIO, &next).unwrap(), 4.0))));
    assert_eq!(p.laid(), (Some(first.digest), Some(next.digest)));
    let mut out = vec![0.0; BAR];
    p.mix(&mut out);
    let [called_off, spent] = p.pend(None);
    assert!(called_off.is_some() && spent.is_none());
    let mut out = vec![0.0; 4 * BAR];
    p.mix(&mut out);
    assert_eq!(p.laid(), (Some(first.digest), None));
    drop(p.pend(Some(Swap::new(lay(STUDIO, &next).unwrap(), f64::NEG_INFINITY))));
    p.mix(&mut out[..2]);
    assert_eq!(p.laid(), (Some(next.digest), None), "at once");
    assert!(p.spent().is_some(), "the old song kept for the main thread");
}

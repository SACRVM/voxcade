//! A channel moved by octaves and semitones: what the text becomes, and
//! that every channel line of the repo's own songs moved up and back plays
//! its notes as before.

use super::*;

/// The notes picked by a cursor at the start of line `row` (from 0).
fn line(text: &str, row: usize) -> Picked {
    let at: usize = text.split('\n').take(row).map(|l| l.len() + 1).sum();
    pick(text, at, at).unwrap_or_else(|| panic!("line {row} is no channel's: {text}"))
}

fn moved_text(text: &str, row: usize, mv: Move) -> String {
    shift(text, &line(text, row), mv).unwrap_or_else(|e| panic!("{e}: {text}")).text
}

/// Whether two songs play the same notes, each at the same start, as long.
fn same_notes(a: &Song, b: &Song) -> bool {
    a.names == b.names
        && a.channels.iter().zip(&b.channels).all(|(x, y)| x.len() == y.len() && x.iter().zip(y).all(|(m, n)| (m.start, m.held, m.freq, m.instrument, m.volume, m.pan, m.sung) == (n.start, n.held, n.freq, n.instrument, n.volume, n.pan, n.sung)))
}

#[test]
fn a_semitone_crossing_into_the_next_octave_moves_the_channels_octave_there() {
    assert_eq!(moved_text("A: o4 a b > c d", 0, Move::Semitones(1)), "A: o4 a+ >c c+ d+");
    assert_eq!(moved_text("A: o4 a+ >c c+ d+", 0, Move::Semitones(-1)), "A: o4 a b > c d".replace("> ", ">"));
    // An `o` before the note that crosses: its number.
    assert_eq!(moved_text("A: o4 b c", 0, Move::Semitones(1)), "A: o5 c <c+");
    assert_eq!(moved_text("A: o4 c d", 0, Move::Semitones(-1)), "A: o3 b >c+");
    assert_eq!(moved_text("A: o3 b >c+", 0, Move::Semitones(1)), "A: o4 c d");
}

#[test]
fn a_line_keeps_its_own_way_of_writing_sharps_and_flats() {
    assert_eq!(moved_text("A: o4 c e- g", 0, Move::Semitones(1)), "A: o4 d- e a-");
    assert_eq!(moved_text("A: o4 c d# g", 0, Move::Semitones(1)), "A: o4 c# e g#");
    assert_eq!(moved_text("A: o4 C D E", 0, Move::Semitones(2)), "A: o4 D E F+", "capitals stay capitals");
    // A tie's letter goes with its note; lengths, rests, volumes and the
    // rest stay as they are.
    assert_eq!(moved_text("@vox singer\nA: @vox v10 l8 \"l a\" c4.&c8 r4 \"d e\" e16 | g ; mine", 1, Move::Semitones(1)), "@vox singer\nA: @vox v10 l8 \"l a\" c+4.&c+8 r4 \"d e\" f16 | g+ ; mine");
}

#[test]
fn an_octave_changes_the_octave_commands_or_puts_one_at_the_start() {
    assert_eq!(moved_text("A: o4 c d o3 e", 0, Move::Octaves(1)), "A: o5 c d o4 e");
    assert_eq!(moved_text("@lead square\nA: @lead c > d", 1, Move::Octaves(-1)), "@lead square\nA: o3 @lead c > d");
    // The line after hears the channel's octave as before.
    assert_eq!(moved_text("A: c d e\nA: f g", 0, Move::Octaves(1)), "A: o5 c d e <\nA: f g");
    assert_eq!(moved_text("A: c d e\nA: o4 f g", 0, Move::Octaves(1)), "A: o5 c d e\nA: o4 f g");
    // A line further on: the channel's octave there, moved.
    assert_eq!(moved_text("A: o3 c >\nA: d e", 1, Move::Octaves(1)), "A: o3 c >\nA: o5 d e");
}

#[test]
fn a_repeat_begins_and_ends_each_time_alike() {
    assert_eq!(moved_text("A: [b c]2 d", 0, Move::Semitones(1)), "A: [>c <c+]2 d+");
    assert_eq!(moved_text("A: o4 [c > c]3 <<< d", 0, Move::Octaves(1)), "A: o5 [c > c]3 <<< d");
}

#[test]
fn a_selection_moves_only_the_notes_it_holds() {
    let text = "A: o4 c d e f";
    let at = text.find('d').unwrap();
    let picked = pick(text, at, at + 3).unwrap();
    assert!(picked.stretch);
    let shifted = shift(text, &picked, Move::Semitones(1)).unwrap();
    assert_eq!(shifted.text, "A: o4 c d+ f f");
    // The selection held over the notes as they are now.
    assert_eq!((shifted.map(at, true), shifted.map(at + 3, false)), (at, at + 4));
    let octave = shift(text, &picked, Move::Octaves(1)).unwrap();
    assert_eq!(octave.text, "A: o4 c >d e < f");
}

#[test]
fn what_cannot_be_moved_says_why() {
    let why = |text: &str, mv| can(text, &line(text, 0), mv).unwrap_err();
    assert!(why("A: o9 c", Move::Octaves(1)).contains("c9 has no octave above"), "{}", why("A: o9 c", Move::Octaves(1)));
    assert!(why("A: o9 b", Move::Semitones(1)).contains("b9"));
    assert!(why("A: o0 c", Move::Semitones(-1)).contains("c0 has no octave below"));
    assert!(can("A: o9 b", &line("A: o9 b", 0), Move::Semitones(-1)).is_ok());
    let macro_song = "$m = c d\nA: $m e";
    assert!(can(macro_song, &line(macro_song, 1), Move::Semitones(1)).unwrap_err().contains("macro"));
    // A macro without notes is no trouble.
    assert_eq!(moved_text("$v = v10\nA: $v c", 1, Move::Semitones(1)), "$v = v10\nA: $v c+");
    assert!(can("A: r4 r4", &line("A: r4 r4", 0), Move::Semitones(1)).is_err(), "no notes");
    assert_eq!(pick("#tempo 120\nA: c", 3, 3), None, "a setting's line is no channel's");
}

#[test]
fn a_channel_plays_alone_or_a_stretch_of_it() {
    let text = "#tempo 100\n#layer B 0.5\n#loop 1\n@lead square\nA: @lead o5 l8 c d e f\nB: o3 c1\n";
    let whole = alone(text, &line(text, 4), false).unwrap();
    assert!(whole.starts_with("#loop no\n") && whole.contains("A: @lead o5 l8 c d e f") && !whole.contains("B:") && !whole.contains("#layer") && !whole.contains("#loop 1"), "{whole}");
    assert!(alone(text, &line(text, 4), true).unwrap().contains("#loop 1"));
    let at = text.find("e f").unwrap();
    let stretch = alone(text, &pick(text, at, at + 3).unwrap(), true).unwrap();
    assert!(stretch.ends_with("A: @lead o5 l8 e f\n"), "{stretch}");
    let cut = "A: c [d e]2 f";
    let at = cut.find('e').unwrap();
    assert!(alone(cut, &pick(cut, at, cut.len()).unwrap(), false).unwrap_err().contains("repeat"));
}

/// Every channel line of every song of the repo (its music, its games'
/// and templates'), moved a semitone up and back down, plays its notes as
/// before; up twelve semitones it plays what an octave up plays. Where a
/// line cannot be moved, it says why (its notes at octave 9, a macro's).
#[test]
fn every_channel_line_of_the_repos_songs_moves_and_comes_back() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    let mut folders = vec![root.join("music")];
    for group in ["games", "templates"] {
        for entry in std::fs::read_dir(root.join(group)).unwrap().flatten() {
            folders.push(entry.path().join("music"));
        }
    }
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "mml") {
                files.push(path);
            }
        }
    }
    files.sort();
    let (mut moved, mut octaves, mut stretches, mut rests, mut refused) = (0, 0, 0, 0, Vec::new());
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        let Ok(song) = parse(&text) else { continue };
        for (row, raw) in text.split('\n').enumerate() {
            let at: usize = text.split('\n').take(row).map(|l| l.len() + 1).sum();
            let Some(picked) = pick(&text, at, at) else { continue };
            let said = format!("{}:{}: {raw}", file.display(), row + 1);
            // Up and back, down and back.
            for k in [1, -1] {
                match shift(&text, &picked, Move::Semitones(k)) {
                    Ok(there) => {
                        let back = shift(&there.text, &line(&there.text, row), Move::Semitones(-k)).unwrap_or_else(|e| panic!("{said}: {k} and back: {e}"));
                        assert!(same_notes(&song, &parse(&back.text).unwrap()), "{said}: {k} and back");
                        moved += 1;
                    }
                    Err(why) if why.starts_with("No notes") => rests += usize::from(k == 1),
                    Err(why) => {
                        assert!(why.contains("octave") || why.contains("macro"), "{said}: {why}");
                        refused.push((said.clone(), why));
                    }
                }
            }
            if let (Ok(octave), Ok(twelve)) = (shift(&text, &picked, Move::Octaves(1)), shift(&text, &picked, Move::Semitones(12))) {
                assert!(same_notes(&parse(&octave.text).unwrap(), &parse(&twelve.text).unwrap()), "{said}: an octave and twelve semitones");
                octaves += 1;
            }
            // The middle third of the line selected: up and back.
            let (from, to) = (at + raw.len() / 3, at + 2 * raw.len() / 3);
            if text.is_char_boundary(from)
                && text.is_char_boundary(to)
                && let Some(stretch) = pick(&text, from, to)
                && let Ok(up) = shift(&text, &stretch, Move::Semitones(1))
            {
                let back = pick(&up.text, up.map(from, true), up.map(to, false)).unwrap_or_else(|| panic!("{said}: the stretch moved"));
                let down = shift(&up.text, &back, Move::Semitones(-1)).unwrap_or_else(|e| panic!("{said}: the stretch back: {e}"));
                assert!(same_notes(&song, &parse(&down.text).unwrap()), "{said}: the stretch up and back");
                stretches += 1;
            }
        }
    }
    eprintln!("{} songs: {moved} moves of a channel line by a semitone and back, {octaves} an octave as twelve semitones, {stretches} stretches up and back, {rests} lines of rests alone, {} refused", files.len(), refused.len());
    for (said, why) in &refused {
        eprintln!("refused: {said}: {why}");
    }
    assert!(moved > 2000, "{moved} lines");
}

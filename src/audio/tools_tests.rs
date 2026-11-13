//! The music tools in Python (`tools/compose.py`, `tools/midi2mml.py`)
//! run from the tests: the recipes still make the songs the repository
//! holds, and a small MIDI file comes out as MML the engine reads. Without
//! python3 (or numpy, which midi2mml.py needs) a test says so and passes:
//! the engine itself does not need them.

use std::path::Path;
use std::process::Command;

/// `python3 ARGS` run in `dir`: its exit, stdout and stderr; none (said on
/// the terminal) when python3 or `module` is missing.
fn python(dir: &Path, args: &[&str], module: Option<&str>) -> Option<(bool, String, String)> {
    let out = match Command::new("python3").args(args).current_dir(dir).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("skipped: python3 not found ({e}); the music tools are not tried");
            return None;
        }
    };
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned());
    if let Some(module) = module
        && stderr.contains(&format!("No module named '{module}'"))
    {
        eprintln!("skipped: python3 has no {module}; {} is not tried", args[0]);
        return None;
    }
    Some((out.status.success(), stdout, stderr))
}

/// Every recipe in `music/` makes, byte for byte, the songs its `out`
/// names (the library's and the games' copies): `compose.py --check`
/// writes nothing.
#[test]
fn the_recipes_make_the_songs_the_repository_holds() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut recipes: Vec<String> = std::fs::read_dir(root.join("music"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".compose.toml"))
        .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
        .collect();
    recipes.sort();
    assert!(recipes.len() >= 8, "{recipes:?}");
    let mut args = vec!["tools/compose.py", "--check"];
    args.extend(recipes.iter().map(String::as_str));
    let Some((fine, said, err)) = python(root, &args, None) else { return };
    assert!(fine, "a recipe no longer makes its song:\n{said}{err}");
    assert!(!said.contains("differs") && !said.contains("missing"), "{said}");
}

/// A variable-length number as MIDI writes it.
fn varlen(mut n: u32, out: &mut Vec<u8>) {
    let mut bytes = vec![(n & 0x7f) as u8];
    n >>= 7;
    while n > 0 {
        bytes.push((n & 0x7f) as u8 | 0x80);
        n >>= 7;
    }
    out.extend(bytes.iter().rev());
}

/// A MIDI file of one track at 120 beats a minute: `notes` (MIDI numbers)
/// a quarter note each, one after another.
fn midi(notes: &[u8]) -> Vec<u8> {
    const QUARTER: u32 = 480;
    let mut track = Vec::new();
    // The tempo: 500000 microseconds a quarter.
    track.extend([0x00, 0xff, 0x51, 0x03, 0x07, 0xa1, 0x20]);
    for &note in notes {
        varlen(0, &mut track);
        track.extend([0x90, note, 100]);
        varlen(QUARTER, &mut track);
        track.extend([0x80, note, 0]);
    }
    track.extend([0x00, 0xff, 0x2f, 0x00]);
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend([0, 0, 0, 1]);
    out.extend((QUARTER as u16).to_be_bytes());
    out.extend(b"MTrk");
    out.extend((track.len() as u32).to_be_bytes());
    out.extend(track);
    out
}

/// A tune made here as MIDI comes out of midi2mml.py as MML the engine
/// reads: its notes in their order, as long as they were.
#[test]
fn midi2mml_turns_a_small_tune_into_a_song() {
    let dir = crate::scratch::Scratch::new("midi2mml");
    // Two bars of quarters, C major up and down (C4 is 60).
    let tune = [60, 62, 64, 65, 67, 65, 64, 62];
    std::fs::write(dir.join("tune.mid"), midi(&tune)).unwrap();
    let import = "title = \"Tune\"\nout = \"tune.mml\"\nbpm = 120\ntempo = 120\n\n[instruments]\nlead = \"square duty=0.25\"\n\n[[part]]\nchannel = \"A\"\nfile = \"tune.mid\"\npick = \"high\"\ninstrument = \"lead\"\nvolume = 12\n";
    std::fs::write(dir.join("tune.import.toml"), import).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/midi2mml.py");
    let Some((fine, said, err)) = python(&dir, &[script.to_str().unwrap(), "tune.import.toml"], Some("numpy")) else { return };
    assert!(fine, "midi2mml.py failed:\n{said}{err}");
    let text = std::fs::read_to_string(dir.join("tune.mml")).unwrap();
    let song = crate::audio::music::parse(&text).unwrap_or_else(|e| panic!("{e:#}\n{text}"));
    assert_eq!(song.title, "Tune");
    let seconds = song.length as f32 / crate::audio::SAMPLE_RATE as f32;
    assert!((seconds - 4.0).abs() < 0.05, "two bars at 120 are 4 s, not {seconds}:\n{text}");
    // The notes in their order, whatever octave marks and lengths stand
    // between them.
    let line = text.lines().find(|l| l.starts_with("A:")).unwrap_or_else(|| panic!("no channel A:\n{text}"));
    let names: String = line[2..].split(['@', ' ']).filter(|w| !w.is_empty() && !w.starts_with(['o', 'v', 'q', 'l', 'p', 'r', '<', '>', '|'])).flat_map(|w| w.chars().next()).collect();
    assert!(names.contains("cdefgfed"), "{names}: {line}");
}

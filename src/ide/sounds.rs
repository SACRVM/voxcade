//! What the IDE says of a sound: a line in words of what it is, where the
//! game's code plays it, and the buffer PLAY SOUND plays (the sound as its
//! text stands now, rendered as the game's mixer gets it).

use std::path::Path;
use std::sync::Arc;

use crate::audio::sound::{Recording, SoundDef};
use crate::audio::synth::{SAMPLE_RATE, Synth, Wave};
use crate::audio::{SoundBank, wav};

/// The name of the sound a file of `sounds/` is (`sounds/coin.toml`:
/// `coin`), if it is one.
pub fn name_of(path: &str) -> Option<&str> {
    let name = path.strip_prefix("sounds/")?;
    let (stem, ending) = name.rsplit_once('.')?;
    (!stem.contains('/') && matches!(ending, "toml" | "wav")).then_some(stem)
}

/// The definition a `.wav` file in the game's sounds is: itself, as it is.
pub fn recording_of(path: &str) -> SoundDef {
    SoundDef::Recording(Recording { file: path.to_string(), rate: None, bits: None, volume: 1.0 })
}

/// A sound in a line of words: how long, and what kind. `dir` is the game
/// folder (a recording's length is read from its file).
pub fn describe(def: &SoundDef, dir: &Path) -> String {
    match def {
        SoundDef::Silent => "A sound that is silent.".into(),
        SoundDef::Synth(s) => format!("A sound: {:.2} s, {}.", s.length(), synth_words(s)),
        SoundDef::Recording(r) => {
            let length = wav::load(&crate::files::Files::folder(dir), &dir.join(&r.file)).map(|s| s.len() as f32 / SAMPLE_RATE as f32);
            let crushed = match (r.rate, r.bits) {
                (Some(rate), Some(bits)) => format!(", crushed to {rate} Hz and {bits} bits"),
                (Some(rate), None) => format!(", crushed to {rate} Hz"),
                (None, Some(bits)) => format!(", crushed to {bits} bits"),
                (None, None) => String::new(),
            };
            match length {
                Ok(seconds) => format!("A sound: a recording, {seconds:.1} s{crushed}."),
                Err(_) => format!("A sound: a recording ({} cannot be read).", r.file),
            }
        }
    }
}

/// What a synthesized sound is: its wave, and how its pitch moves.
fn synth_words(s: &Synth) -> String {
    let mut out = match s.wave {
        Wave::Noise => "a burst of noise".to_string(),
        wave => format!("a {} wave", wave.name()),
    };
    if s.wave != Wave::Noise {
        if s.sweep.round() != 0.0 {
            out.push_str(&format!(" {} {}", if s.sweep < 0.0 { "falling" } else { "rising" }, interval(s.sweep.abs())));
        }
        if s.jump.round() != 0.0 {
            out.push_str(&format!(", jumping {} {}", if s.jump < 0.0 { "down" } else { "up" }, interval(s.jump.abs())));
        }
    }
    out
}

/// A pitch step in words: an octave, an octave and a half, 5 semitones.
fn interval(semitones: f64) -> String {
    let n = semitones.round() as u32;
    let (octaves, rest) = (n / 12, n % 12);
    let whole = |k: u32| match k {
        1 => "an octave".to_string(),
        2 => "two octaves".to_string(),
        3 => "three octaves".to_string(),
        k => format!("{k} octaves"),
    };
    match (octaves, rest) {
        (0, 1) => "a semitone".into(),
        (0, r) => format!("{r} semitones"),
        (k, 0) => whole(k),
        (k, 6) => format!("{} and a half", whole(k)),
        _ => format!("{n} semitones"),
    }
}

/// One place in the game that names a sound.
#[derive(Clone, Debug, PartialEq)]
pub struct Use {
    pub file: String,
    pub line: usize,
    /// The line as written, trimmed.
    pub text: String,
}

/// Where the game names the sound `name`: every line of its scripts (their
/// paths in `dir`) with `"name"` or `'name'` as a whole string. More
/// sources of uses (a song naming it as an instrument) go in here.
pub fn used(dir: &Path, scripts: &[String], name: &str) -> Vec<Use> {
    lua_uses(dir, scripts, name)
}

fn lua_uses(dir: &Path, scripts: &[String], name: &str) -> Vec<Use> {
    let (double, single) = (format!("\"{name}\""), format!("'{name}'"));
    let mut out = Vec::new();
    for file in scripts.iter().filter(|f| f.ends_with(".lua")) {
        let Ok(text) = std::fs::read_to_string(dir.join(file)) else { continue };
        for (i, line) in text.lines().enumerate() {
            if line.contains(&double) || line.contains(&single) {
                out.push(Use { file: file.clone(), line: i + 1, text: line.trim().to_string() });
            }
        }
    }
    out
}

/// The sound as the game's mixer gets it (mono, at the engine's rate), for
/// PLAY SOUND; empty for a silent one. A recording that will not play is
/// the error.
pub fn render(def: &SoundDef, dir: &Path) -> anyhow::Result<Arc<[f32]>> {
    let mut bank = SoundBank::default();
    bank.put(&crate::files::Files::folder(dir), "heard", def)?;
    Ok(bank.get("heard").cloned().unwrap_or_else(|| Arc::from(Vec::new())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sound;

    fn def(text: &str) -> SoundDef {
        sound::read("x.toml", text).unwrap()
    }

    /// A synth, a recording and a silent sound each say what they are.
    #[test]
    fn a_sound_is_described_in_words() {
        let dir = std::env::temp_dir().join(format!("voxcade-ide-sounds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        let ping = def("wave = \"square\"\nhold = 0.1\ndecay = 0.1\nsweep = -18\n");
        assert_eq!(describe(&ping, &dir), "A sound: 0.20 s, a square wave falling an octave and a half.");
        assert_eq!(describe(&def("wave = \"noise\"\nhold = 0\ndecay = 0.3\n"), &dir), "A sound: 0.30 s, a burst of noise.");
        assert_eq!(describe(&def("wave = \"triangle\"\njump = 5\njump_at = 0.05\n"), &dir), "A sound: 0.30 s, a triangle wave, jumping up 5 semitones.");
        assert_eq!(describe(&def("silent = true\n"), &dir), "A sound that is silent.");
        // One second of silence as a 8-bit mono recording at 8000 Hz.
        let mut wav = b"RIFF".to_vec();
        wav.extend((36u32 + 8000).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        for v in [1u16, 1] {
            wav.extend(v.to_le_bytes());
        }
        wav.extend(8000u32.to_le_bytes());
        wav.extend(8000u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(8u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(8000u32.to_le_bytes());
        wav.extend(vec![128u8; 8000]);
        std::fs::write(dir.join("sounds/boom.wav"), &wav).unwrap();
        assert_eq!(describe(&recording_of("sounds/boom.wav"), &dir), "A sound: a recording, 1.0 s.");
        assert!(describe(&recording_of("sounds/none.wav"), &dir).contains("cannot be read"));
        // PLAY SOUND's buffer: the synth as the mixer has it, a silent one empty.
        assert_eq!(render(&ping, &dir).unwrap().len(), (0.2 * SAMPLE_RATE as f32).ceil() as usize);
        assert!(render(&def("silent = true\n"), &dir).unwrap().is_empty());
        assert!(!render(&recording_of("sounds/boom.wav"), &dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The lines naming a sound as a whole string are found, `"coin"` or
    /// `'coin'`, not `"coins"`.
    #[test]
    fn a_sound_is_found_where_the_scripts_name_it() {
        let dir = std::env::temp_dir().join(format!("voxcade-ide-uses-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  level:sound(\"coin\", player)\n  x = \"coins\"\nend\n").unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n\n  c.things.gem = { sound = 'coin' }\nend\n").unwrap();
        let found = used(&dir, &["conf.lua".into(), "main.lua".into()], "coin");
        assert_eq!(found, [Use { file: "conf.lua".into(), line: 3, text: "c.things.gem = { sound = 'coin' }".into() }, Use { file: "main.lua".into(), line: 2, text: "level:sound(\"coin\", player)".into() }]);
        assert!(used(&dir, &["main.lua".into()], "bell").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

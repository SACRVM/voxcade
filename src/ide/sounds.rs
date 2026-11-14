//! What the IDE says of a sound: a line in words of what it is, where the
//! game's code plays it, and the buffer PLAY SOUND plays (the sound as its
//! text stands now, rendered as the game's mixer gets it).

use std::path::Path;
use std::sync::Arc;

use super::refs::Ref;
use crate::audio::SoundBank;
use crate::audio::sound::SoundDef;
use crate::audio::synth::{Synth, Wave};

/// The name of the sound a file of `sounds/` is (`sounds/coin.toml`:
/// `coin`), if it is one.
pub fn name_of(path: &str) -> Option<&str> {
    let name = path.strip_prefix("sounds/")?;
    let (stem, ending) = name.rsplit_once('.')?;
    (!stem.contains('/') && ending == "toml").then_some(stem)
}

/// A sound as a line of facts: how long, its wave, how its pitch moves.
pub fn describe(def: &SoundDef) -> String {
    match def {
        SoundDef::Silent => "silent".into(),
        SoundDef::Synth(s) => format!("{:.2} s · {}", s.length(), synth_words(s)),
    }
}

/// What a synthesized sound is: its wave, and how its pitch moves.
fn synth_words(s: &Synth) -> String {
    let mut out = match s.wave {
        Wave::Noise => "noise".to_string(),
        wave => format!("{} wave", wave.name()),
    };
    if s.wave != Wave::Noise {
        if s.sweep.round() != 0.0 {
            out.push_str(&format!(" · {} {}", if s.sweep < 0.0 { "falling" } else { "rising" }, interval(s.sweep.abs())));
        }
        if s.jump.round() != 0.0 {
            out.push_str(&format!(" · jumping {} {}", if s.jump < 0.0 { "down" } else { "up" }, interval(s.jump.abs())));
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

/// Where the game names the sound `name`: every place of its scripts
/// (their paths in `dir`) where a sound is taken by that name for certain,
/// then those where the name stands in quotes and may mean it (`refs`).
/// More sources of uses (a song naming it as an instrument) go in here.
pub fn used(dir: &Path, scripts: &[String], name: &str) -> Vec<Ref> {
    let path = format!("sounds/{name}.toml");
    super::refs::find(dir, scripts, &path, &path, super::refs::Sort::Sound)
}

/// The sound as the game's mixer gets it (mono, at the engine's rate), for
/// PLAY SOUND; empty for a silent one.
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

    /// A synth and a silent sound each say what they are.
    #[test]
    fn a_sound_is_described_in_words() {
        let dir = crate::scratch::Scratch::new("ide-sounds");
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        let ping = def("wave = \"square\"\nhold = 0.1\ndecay = 0.1\nsweep = -18\n");
        assert_eq!(describe(&ping), "0.20 s · square wave · falling an octave and a half");
        assert_eq!(describe(&def("wave = \"noise\"\nhold = 0\ndecay = 0.3\n")), "0.30 s · noise");
        assert_eq!(describe(&def("wave = \"triangle\"\njump = 5\njump_at = 0.05\n")), "0.30 s · triangle wave · jumping up 5 semitones");
        assert_eq!(describe(&def("silent = true\n")), "silent");
        // PLAY SOUND's buffer: the synth as the mixer has it, a silent one empty.
        assert_eq!(render(&ping, &dir).unwrap().len(), (0.2 * crate::audio::SAMPLE_RATE as f32).ceil() as usize);
        assert!(render(&def("silent = true\n"), &dir).unwrap().is_empty());
    }

    /// The places naming a sound are found, `"coin"` or `'coin'`, not
    /// `"coins"`, the game's own call apart as a maybe.
    #[test]
    fn a_sound_is_found_where_the_scripts_name_it() {
        let dir = crate::scratch::Scratch::new("ide-uses");
        std::fs::write(dir.join("main.lua"), "function on_tick()\n  level:sound(\"coin\", player)\n  x = \"coins\"\n  fx.fly(\"coin\")\nend\n").unwrap();
        std::fs::write(dir.join("conf.lua"), "function conf(c)\n\n  c.things.gem = { sound = 'coin' }\nend\n").unwrap();
        let found = used(&dir, &["conf.lua".into(), "main.lua".into()], "coin");
        let rows: Vec<(&str, usize, &str, bool)> = found.iter().map(|u| (u.file.as_str(), u.line, u.text.as_str(), u.sure)).collect();
        assert_eq!(rows, [("conf.lua", 3, "c.things.gem = { sound = 'coin' }", true), ("main.lua", 2, "level:sound(\"coin\", player)", true), ("main.lua", 4, "fx.fly(\"coin\")", false)]);
        assert!(used(&dir, &["main.lua".into()], "bell").is_empty());
    }
}

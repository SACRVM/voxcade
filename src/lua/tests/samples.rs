//! Samples in a game: `samples/` read through the game's files (sealed ones
//! too), a studio's mistakes at load, a sample saved anew heard without a
//! restart, the run untouched, and `music:mix`'s words of a sample.

use super::*;
use crate::audio::music::sample::wav_file;
use crate::session::Reloaded;

/// A sine of `hz` for a second at 44100 Hz, as a 16-bit mono WAV.
fn tone(hz: f32) -> Vec<u8> {
    let s: Vec<f32> = (0..44_100).map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / 44_100.0).sin()).collect();
    wav_file(44_100, 1, 16, false, &s)
}

/// A game whose music is `music/s.mml` (this text), with this `main.lua`
/// and a sample `tone` of 220 Hz.
fn sampled(name: &str, main: &str, song: &str) -> crate::scratch::Scratch {
    let dir = folder(name, main);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::create_dir_all(dir.join("samples")).unwrap();
    std::fs::write(dir.join("music/s.mml"), song).unwrap();
    std::fs::write(dir.join("samples/tone.wav"), tone(220.0)).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/s.mml\" }\nend\n").unwrap();
    dir
}

/// How fast the music `g` plays (else its song `music/s.mml`) goes round over
/// its first half second, by its rising crossings of zero; None without
/// music.
fn pitch(g: &Session<LuaGame>) -> Option<f32> {
    let s: Vec<f32> = g.music().or_else(|| g.song("music/s.mml"))?.render(Some(0.5)).into_iter().step_by(2).collect();
    let ups: Vec<usize> = (1..s.len()).filter(|&i| s[i - 1] < 0.0 && s[i] >= 0.0).collect();
    (ups.len() > 2).then(|| (ups.len() - 1) as f32 * 44_100.0 / (ups[ups.len() - 1] - ups[0]) as f32)
}

const HELD: &str = "#tempo 120\n@s sample name=tone loop=yes attack=0.001 decay=0 sustain=1 volume=1\nA: @s o4 c1\n";

/// A sample saved anew is heard at once, the song that plays made anew
/// from it, without a restart; one taken away leaves the song silent (its
/// mistake said); the run's fingerprint never moves, the game plays on.
#[test]
fn a_sample_saved_anew_is_heard_without_a_restart() {
    let dir = sampled("sample-reload", "function on_tick() world.state.n = (world.state.n or 0) + 1 end\n", HELD);
    let mut g = game_in(&dir);
    let print = g.fingerprint().unwrap();
    assert!(pitch(&g).is_some_and(|hz| (hz - 220.0).abs() < 1.0), "{:?}", pitch(&g));
    steps(&mut g, 5);
    g.music_changed = false;
    std::fs::write(dir.join("samples/tone.wav"), tone(330.0)).unwrap();
    assert_eq!(g.reload(&dir.join("samples/tone.wav")).unwrap(), Reloaded::Music);
    assert!(g.music_changed, "the song is made anew");
    assert!(pitch(&g).is_some_and(|hz| (hz - 330.0).abs() < 1.0), "{:?}", pitch(&g));
    assert_eq!(g.reload(&dir.join("samples/tone.wav")).unwrap(), Reloaded::Nothing, "the same bytes again change nothing");
    steps(&mut g, 5);
    assert_eq!(state(&mut g, "n"), format!("{:?}", crate::lua::Value::Int(10)), "the run went on");
    assert_eq!(g.fingerprint().unwrap(), print, "a sample is no part of the run");
    std::fs::remove_file(dir.join("samples/tone.wav")).unwrap();
    assert_eq!(g.reload(&dir.join("samples/tone.wav")).unwrap(), Reloaded::Music);
    assert!(g.music().is_none(), "a song naming a sample gone does not play");
    assert_eq!(halted(&g), "");
}

/// A studio naming a sample the game lacks is a mistake as the game
/// loads, naming its file, its line and the nearest sample; a sample
/// taken away under a studio is a mistake of the reload. The channels a
/// game lays over a studio play its samples.
#[test]
fn a_studio_naming_a_sample_it_lacks_is_a_mistake_at_load() {
    let main = "function on_level_start(how) music:play('s', { channels = { A = '@t o4 c4 c4 c4 c4' }, from = 'now' }) end\n";
    let dir = sampled("sample-studio", main, "#tempo 120\n\n@t sample name=tonne attack=0.001 decay=0 sustain=1\n");
    let e = start_error(&dir);
    assert!(e.contains("music/s.mml (a file of instruments, no channel)") && e.contains("line 3: sample name=tonne") && e.contains("(the nearest: \"tone\")"), "{e}");
    std::fs::write(dir.join("music/s.mml"), "#tempo 120\n@t sample name=tone attack=0.001 decay=0 sustain=1\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert!(pitch(&g).is_some_and(|hz| (hz - 220.0).abs() < 1.0), "the laid channel plays the sample: {:?}", pitch(&g));
    std::fs::remove_file(dir.join("samples/tone.wav")).unwrap();
    let e = format!("{:#}", g.reload(&dir.join("samples/tone.wav")).unwrap_err());
    assert!(e.contains("sample name=tone: there is no samples/tone.wav"), "{e}");
}

/// A game of sealed files plays the samples sealed with it, whatever the
/// folder holds since.
#[test]
fn a_sealed_game_plays_its_sealed_samples() {
    let dir = sampled("sample-sealed", "function on_tick() end\n", HELD);
    let mut sealed = std::collections::BTreeMap::new();
    for name in ["conf.lua", "main.lua", "music/s.mml", "samples/tone.wav"] {
        sealed.insert(name.to_string(), crate::files::File::of(std::fs::read(dir.join(name)).unwrap()));
    }
    std::fs::remove_dir_all(dir.join("samples")).unwrap();
    let g = Session::<LuaGame>::load_from(crate::files::Files::sealed(&dir, sealed), crate::saves::Slots::default()).unwrap();
    assert!(pitch(&g).is_some_and(|hz| (hz - 220.0).abs() < 1.0), "{:?}", pitch(&g));
}

/// `music:mix` turns a sample's root (a note, read back as one), start and
/// end from the next note; its name, loop and reverse do not turn, saying
/// why, and a root that is no note is a mistake.
#[test]
fn a_strip_turns_a_samples_root_start_and_end() {
    let main = r#"
function on_level_start(how) music:play('s', 'now') end
function on_tick()
  if world.tick == 3 then music:mix('A', { root = 'g3', start = 0.25, ['end'] = 0.5 }) end
  if world.tick == 4 then
    local m = music:mix('A')
    world.state.read = m.root .. ' ' .. m.start .. ' ' .. m['end']
  end
end
"#;
    let mut g = game_in(&sampled("sample-mix", main, HELD));
    steps(&mut g, 6);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "read"), format!("{:?}", crate::lua::Value::Str(b"g3 0.25 0.5".to_vec())));
    for (asked, says) in [("{ loop = true }", "music:mix: loop does not turn: whether a sample goes round"), ("{ name = 'x' }", "name does not turn"), ("{ reverse = true }", "reverse does not turn"), ("{ root = 'h4' }", "root is a note, a letter and an octave"), ("{ root = 440 }", "root is a note")] {
        let main = format!("function on_tick()\n  if world.tick == 2 then music:mix('A', {asked}) end\nend\n");
        let mut g = game_in(&sampled("sample-mix-not", &main, HELD));
        steps(&mut g, 3);
        assert!(halted(&g).contains(says), "{asked}: {}", halted(&g));
    }
}

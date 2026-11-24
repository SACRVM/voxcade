//! A song begun at a bar (`music:play`'s `at`, `c.title_screen.music_at`)
//! and one going round its `#loop`, with the game's clock.

use super::*;

/// A bar a second (240 beats a minute), each its own pure tone: bar 1
/// c4, 2 e4, 3 g4, 4 c5, 5 e5; bars 3 and 4 go round.
const BARS: &str = "#tempo 240\n#loop 3 4\n@s sine attack=0.002 decay=0 sustain=1 release=0.002 volume=0.6\nA: @s o4 c1 e1 g1 > c1 e1\n";
/// The bars' pitches in Hz.
const PITCH: [f64; 5] = [261.63, 329.63, 392.0, 523.25, 659.26];

/// A game of one level and the song `BARS` as `music/song.mml`.
fn bars_folder(name: &str, main: &str, conf: &str) -> crate::scratch::Scratch {
    let dir = folder(name, main);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/song.mml"), BARS).unwrap();
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.size = {{ 16, 16, 8 }}\n  c.audio.music = {{ \"music/song.mml\" }}\n{conf}\nend\n")).unwrap();
    dir
}

/// The bar (1 to 5) whose tone is loudest in mono `s` from sample `from`,
/// a tenth of a second of it.
fn bar_heard(s: &[f32], from: usize) -> usize {
    let x = &s[from..from + crate::audio::SAMPLE_RATE as usize / 10];
    let power = |hz: f64| {
        let w = std::f64::consts::TAU * hz / crate::audio::SAMPLE_RATE as f64;
        let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, &v)| (re + f64::from(v) * (w * n as f64).cos(), im - f64::from(v) * (w * n as f64).sin()));
        re * re + im * im
    };
    (0..5).max_by(|&a, &b| power(PITCH[a]).total_cmp(&power(PITCH[b]))).unwrap() + 1
}

/// The music `g` plays over `ticks` steps, fed and mixed as a window does,
/// its left side.
fn heard(g: &mut Session<LuaGame>, mixer: &mut crate::audio::Mixer, ticks: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; crate::audio::SAMPLE_RATE as usize / 60 * 2];
    let mut out = Vec::new();
    for _ in 0..ticks {
        g.step(&Input::default());
        g.feed_music(mixer, false);
        mixer.mix(&mut buf);
        out.extend(buf.iter().step_by(2));
    }
    out
}

/// `music:play(song, { at = 2 })` begins the song at bar 2, its beat 0
/// there (`music.beat`, `music.bar`, `on_beat` count from it), and with
/// `#loop 3 4` it goes round bars 3 and 4 as written, bar 5 never
/// heard; the beats count on round the loop. Brought back from a snapshot
/// inside the loop, the game plays on the same and the music is heard
/// where it was.
#[test]
fn a_song_begun_at_a_bar_goes_round_its_loop() {
    let main = "function on_level_start(how)\n  music:play('song', { from = 'now', at = 2 })\nend\nfunction on_beat(beat)\n  world.state.beats = (world.state.beats or 0) + 1\nend\nfunction on_tick()\n  if world.tick == 1 then world.state.first = music.beat .. ' ' .. music.bar end\n  world.state.now = music.beat .. ' ' .. music.bar\nend\n";
    let dir = bars_folder("music-at", main, "");
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let second = crate::audio::SAMPLE_RATE as usize;
    let s = heard(&mut g, &mut mixer, 300);
    assert_eq!(halted(&g), "");
    // The limiter's 1.5 ms aside, a second a bar: 2, 3, 4, then 3, 4 again.
    let bars: Vec<usize> = (0..5).map(|n| bar_heard(&s, n * second + second / 2)).collect();
    assert_eq!(bars, [2, 3, 4, 3, 4]);
    assert_eq!(state(&mut g, "first"), format!("{:?}", crate::lua::Value::Str(b"0 0".to_vec())));
    // 300 steps at 4 beats a second: beat 19 (on and on round the loop), bar 4.
    assert_eq!(state(&mut g, "now"), format!("{:?}", crate::lua::Value::Str(b"19 4".to_vec())));
    assert_eq!(state(&mut g, "beats"), format!("{:?}", crate::lua::Value::Int(20)));

    // Kept inside the loop and brought back: the same hashes, the same bars heard.
    let kept = g.snapshot().unwrap();
    let hashes: Vec<u64> = (0..120).map(|_| { steps(&mut g, 1); g.game.state_hash() }).collect();
    g.restore(&kept);
    let mut fresh = crate::audio::Mixer::default();
    g.music_changed = true;
    let mut again = Vec::new();
    let mut buf = vec![0.0f32; second / 60 * 2];
    for (n, hash) in hashes.iter().enumerate() {
        g.step(&Input::default());
        assert_eq!(g.game.state_hash(), *hash, "tick {} after the snapshot", n + 1);
        g.feed_music(&mut fresh, false);
        fresh.mix(&mut buf);
        again.extend(buf.iter().step_by(2));
    }
    // Brought back at the loop's seam: bar 3 again, then 4.
    assert_eq!([bar_heard(&again, second / 2), bar_heard(&again, second + second / 2)], [3, 4]);
}

/// `at` is a whole bar of the song, not after its `#loop` (which it never
/// leaves): mistakes name what is wrong; a song
/// asked for while it plays goes on, whatever bar is asked.
#[test]
fn a_bar_to_begin_at_is_one_of_the_song() {
    let dir = bars_folder("music-at-wrong", "function on_tick() end\n", "");
    for (line, says) in [
        ("music:play('song', { at = 0 })", "music:play: at is the bar the song begins at, a whole number from 1 (its first bar), not 0"),
        ("music:play('song', { at = 2.5 })", "music:play: at is the bar the song begins at, a whole number from 1 (its first bar), not 2.5"),
        ("music:play('song', { at = 'two' })", "not \"two\""),
        ("music:play('song', { at = 6 })", "music:play: at = 6 is past the end of \"song\", which has 5 bars"),
        ("music:play('song', { at = 5 })", "music:play: at = 5 lies after the loop of \"song\" (#loop 3 4), which it never leaves"),
        ("music:play(nil, { at = 2 })", "music:play: which song?"),
        ("music:play('song', { start = 2 })", "music:play"),
    ] {
        std::fs::write(dir.join("main.lua"), format!("function on_tick()\n  {line}\nend\n")).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{line}: {}", halted(&g));
    }
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  music:play('song', { from = 'now', at = 4 })\n  if world.tick == 3 then music:play('song', { at = 2 }) end\n  world.state.now = music.beat\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 120);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "now"), format!("{:?}", crate::lua::Value::Int(7)), "it went on");
}

/// A run with a song begun at a bar and going round its `#loop` replays
/// the same, going back every 20 ticks too; `#loop` counts for the game's
/// data (another loop: another fingerprint), and a game that names
/// `music` with a song without one keeps its fingerprint's line as it was.
#[test]
fn a_run_round_a_loop_replays_the_same() {
    use crate::replay::{Recorder, check, check_going_back};
    let main = "function on_level_start(how)\n  music:play('song', { from = 'now', at = 3 })\nend\nfunction on_beat(beat)\n  world.state.beats = (world.state.beats or 0) + 1\n  if beat % 4 == 0 then level:spawn('crate', 4 + beat % 8, 4) end\nend\nfunction on_tick() end\n";
    let dir = bars_folder("music-at-replay", main, "  c.things.crate = {}");
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    while g.game.sim().tick < 600 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/round.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "the replay plays the same");
    assert!(check_going_back::<LuaGame>(&dir, 20).unwrap(), "going back every 20 ticks too");
    let before = g.fingerprint().unwrap();
    std::fs::write(dir.join("music/song.mml"), BARS.replace("#loop 3 4", "#loop 2 4")).unwrap();
    assert_ne!(game_in(&dir).fingerprint().unwrap(), before, "another loop, other game data");
    // The songs' line of the fingerprint: `#loop` only where there is one.
    let line = |text: &str| {
        std::fs::write(dir.join("music/song.mml"), text).unwrap();
        game_in(&dir).fingerprint_files().unwrap().into_iter().find(|(n, _)| n == "music").unwrap().1
    };
    let hashed = |line: &str| crate::hash::Stable::default().bytes(line.as_bytes()).finish();
    assert_eq!(line(&BARS.replace("#loop 3 4\n", "")), hashed("song 240 4\n"));
    assert_eq!(line(BARS), hashed("song 240 4 loop 3 4\n"));
    assert_eq!(line(&BARS.replace("#loop 3 4", "#loop 3")), hashed("song 240 4 loop 3\n"));
}

/// `c.title_screen.music_at` begins the title's song at that bar (and it
/// goes round its `#loop`); a bar past the song's end, or out of its
/// range, is a mistake naming the setting.
#[test]
fn the_title_song_begins_at_its_bar() {
    let named = "  c.title_screen.music = \"song\"\n";
    let dir = bars_folder("title-music-at", "function on_tick() end\n", &format!("{named}  c.title_screen.music_at = 4"));
    let g = game_in(&dir);
    let mut title = g.title_music().unwrap();
    let second = crate::audio::SAMPLE_RATE as usize;
    let mut out = vec![0.0f32; 3 * second * 2];
    title.mix(&mut out);
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    let bars: Vec<usize> = (0..3).map(|n| bar_heard(&left, n * second + second / 2)).collect();
    assert_eq!(bars, [4, 3, 4], "from bar 4, round bars 3 and 4");
    let plain = bars_folder("title-music-plain", "function on_tick() end\n", named);
    let mut out = vec![0.0f32; second * 2];
    game_in(&plain).title_music().unwrap().mix(&mut out);
    assert_eq!(bar_heard(&out.iter().step_by(2).copied().collect::<Vec<_>>(), second / 2), 1, "from its start unless said");
    for (line, says) in [
        ("  c.title_screen.music_at = 6", "c.title_screen.music_at = 6 is past the end of \"song\", which has 5 bars (music/song.mml)"),
        ("  c.title_screen.music_at = 5", "c.title_screen.music_at = 5 lies after the loop of \"song\" (#loop 3 4)"),
        ("  c.title_screen.music_at = 0", "c.title_screen.music_at"),
    ] {
        let dir = bars_folder("title-music-wrong", "function on_tick() end\n", &format!("{named}{line}"));
        assert!(start_error(&dir).contains(says), "{line}: {}", start_error(&dir));
    }
    // Without a song for the title there is nothing to begin: no mistake.
    let silent = bars_folder("title-music-none", "function on_tick() end\n", "  c.audio.music = nil\n  c.title_screen.music_at = 2");
    assert_eq!(start_error(&silent), "");
    // Songs but none named for the title: a silent title, no mistake.
    let unnamed = bars_folder("title-music-unnamed", "function on_tick() end\n", "  c.title_screen.music_at = 6");
    assert_eq!(start_error(&unnamed), "");
    assert!(game_in(&unnamed).title_music().is_none());
}

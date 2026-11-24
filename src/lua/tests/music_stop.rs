//! `music:stop`: silence in place of the game's song.

use super::*;

/// Two songs of long sine notes: "a" at 440 Hz, "b" at 660 Hz, 120 beats
/// a minute (a beat every 30 ticks, a bar every 120).
fn songs_folder(name: &str, main: &str) -> crate::scratch::Scratch {
    let dir = folder(name, main);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    for (song, pitch) in [("a", "a"), ("b", "> e")] {
        let text = format!("#tempo 120\n@s sine attack=0.002 decay=0 sustain=1 release=0.5 volume=0.6\nA: @s o4 {pitch}1 {pitch}1 {pitch}1 {pitch}1\n");
        std::fs::write(dir.join(format!("music/{song}.mml")), text).unwrap();
    }
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/a.mml\", \"music/b.mml\" }\nend\n").unwrap();
    dir
}

/// Steps `g` a tick at a time, feeding and mixing as a window does: each
/// tick's left side, and whether the mixer had a song after it.
fn played(g: &mut Session<LuaGame>, mixer: &mut crate::audio::Mixer, ticks: usize) -> Vec<(Vec<f32>, bool)> {
    let mut buf = vec![0.0f32; crate::audio::SAMPLE_RATE as usize / 60 * 2];
    (0..ticks)
        .map(|_| {
            g.step(&Input::default());
            g.feed_music(mixer, false);
            mixer.mix(&mut buf);
            (buf.iter().step_by(2).copied().collect(), mixer.music_title().is_some())
        })
        .collect()
}

fn loudest(s: &[f32]) -> f32 {
    s.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// Stopped, the game's song is silence: `music.song`, `beat`, `bar` and
/// `position` are nil, no beat is told and no other song comes in its
/// place. A play after it begins its song from beat 0; a stop from "bar"
/// waits for the bar line and fades as long as it says; a song asked after
/// it plays. Silence counts for replays only where a game stops.
#[test]
fn a_stopped_song_is_silence_and_no_other_comes_in() {
    let main = r#"
function on_level_start(how) music:play('a', 'now') end
function on_beat(b) world.state.beats = (world.state.beats or '') .. b .. '@' .. world.tick .. ';' end
function on_tick()
  if world.tick == 60 then music:stop() end
  if world.tick == 61 then world.state.after = tostring(music.song) .. ' ' .. tostring(music.beat) .. ' ' .. tostring(music.bar) .. ' ' .. tostring(music.position) end
  if world.tick == 120 then music:play('a', 'now') end
  if world.tick == 180 then music:stop({ from = 'bar', fade = 0.5 }) end
  if world.tick == 300 then music:play('b', 'now') end
end
"#;
    let dir = songs_folder("music-stop", main);
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let heard = played(&mut g, &mut mixer, 360);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "after"), format!("{:?}", crate::lua::Value::Str(b"nil nil nil nil".to_vec())));
    // Beats of the first song until the stop, of the second from its own
    // beat 0 until its bar line; none while stopped.
    assert_eq!(state(&mut g, "beats"), format!("{:?}", crate::lua::Value::Str(b"0@1;1@31;0@121;1@151;2@181;3@211;0@301;1@331;".to_vec())));
    // (Tick n of the run is heard[n - 1].)
    let at = |t: usize| &heard[t - 1];
    assert!(loudest(&at(50).0) > 0.1, "the song plays");
    assert!(loudest(&at(62).0) < 1e-6 && !at(62).1, "silence a tick after the stop, no song in the mixer: {} {} {}", loudest(&at(61).0), loudest(&at(62).0), at(62).1);
    for t in 62..121 {
        assert!(!at(t).1 && loudest(&at(t).0) < 1e-6, "a song came back at tick {t}");
    }
    assert!(loudest(&at(130).0) > 0.1, "a play after the stop begins its song");
    // The stop from "bar" at the bar line (tick 241), faded over half a
    // second: still heard a quarter second on, silent after it.
    assert!(loudest(&at(220).0) > 0.1);
    let quarter = loudest(&at(256).0);
    assert!(quarter > 0.05 && quarter < loudest(&at(220).0), "fading: {quarter} {}", loudest(&at(220).0));
    assert!(loudest(&at(275).0) < 1e-6, "faded out");
    assert!(loudest(&at(320).0) > 0.1 && at(320).1, "a song asked after it plays");
    assert!(!g.game.music_silent());

    // Stopped and brought back, the state is the same tick for tick.
    let mut g = game_in(&dir);
    steps(&mut g, 70);
    let snapshot = g.snapshot().unwrap();
    let hashes: Vec<u64> = (0..100).map(|_| {
        g.step(&Input::default());
        g.game.state_hash()
    }).collect();
    g.restore(&snapshot);
    for (n, hash) in hashes.iter().enumerate() {
        g.step(&Input::default());
        assert_eq!(g.game.state_hash(), *hash, "tick {} played out differently", 71 + n);
    }
}

/// One rule for a play over a stop to come: it replaces the stop. The
/// song that plays asked for plays on, never silent, its clock going on;
/// another song comes at the play's own `from`. A stop right after the
/// game's first song is silence too, and a stop while stopped forgets a
/// song asked since.
#[test]
fn a_play_over_a_stop_to_come_replaces_it() {
    let main = r#"
function on_level_start(how) music:play('a', 'now') end
function on_beat(b) world.state.beats = (world.state.beats or '') .. b .. ';' end
function on_tick()
  if world.tick == 20 then music:stop('bar'); music:play('a') end
  if music.song == nil then world.state.silent = (world.state.silent or 0) + 1 end
  if world.tick == 200 then music:stop('bar') end
  if world.tick == 210 then music:play('b') end
  if world.tick == 241 then world.state.at_bar = tostring(music.song) .. ' ' .. tostring(music.beat) end
end
"#;
    let dir = songs_folder("music-stop-over", main);
    let mut g = game_in(&dir);
    steps(&mut g, 260);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "silent"), "", "never silent");
    assert_eq!(state(&mut g, "at_bar"), format!("{:?}", crate::lua::Value::Str(b"b 0".to_vec())), "b at the bar line, as its play asked");
    assert_eq!(state(&mut g, "beats"), format!("{:?}", crate::lua::Value::Str(b"0;1;2;3;4;5;6;7;0;".to_vec())), "a's clock went on until b");

    // A stop while a song plays that the game asked before: silence, at once.
    let main = "function on_level_start() music:play('a', 'now') end\nfunction on_tick()\n  if world.tick == 5 then music:stop() end\n  if world.tick == 30 then music:stop('bar'); music:play('b', 'now'); music:stop() end\nend\n";
    let dir = songs_folder("music-stop-own", main);
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let heard = played(&mut g, &mut mixer, 60);
    assert!(heard[3].1 && !heard[10].1, "the song, then silence");
    assert!(g.game.music_silent() && g.game.music().is_none(), "a stop while stopped forgot the song asked");
    assert!(heard[30..].iter().all(|(s, song)| !song && loudest(s) < 1e-6));
}

/// A song stopped and begun again between two feeds of the audio (a
/// window playing several ticks in one picture) is a song begun anew, not
/// one the audio must find its way back into: the session sees both.
#[test]
fn a_stop_and_a_play_between_two_feeds_begin_the_song_anew() {
    let main = "function on_level_start(how) music:play('a', 'now') end\nfunction on_tick()\n  if world.tick == 100 then music:stop() end\n  if world.tick == 101 then music:play('a') end\nend\n";
    let dir = songs_folder("music-stop-play", main);
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    played(&mut g, &mut mixer, 99);
    steps(&mut g, 4);
    assert_eq!(g.game.music().map(|c| c.beats), Some(1.0 / 30.0), "begun anew at tick 102, a step in");
    assert!(g.feed_music(&mut mixer, false), "a song handed afresh");
}

/// A stop "now" does not click: the music fades over 5 ms at least, so no
/// sample steps further from the one before than the song's own do.
#[test]
fn a_stop_does_not_click() {
    let main = "function on_level_start(how) music:play('a', 'now') end\nfunction on_tick()\n  if world.tick == 3 then music:stop() end\nend\n";
    let dir = songs_folder("music-stop-click", main);
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let s: Vec<f32> = played(&mut g, &mut mixer, 10).into_iter().flat_map(|(s, _)| s).collect();
    let step = |s: &[f32]| s.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    let song = step(&s[200..2 * 735]);
    assert!(loudest(&s[200..2 * 735]) > 0.1, "the song is heard before the stop: {}", loudest(&s[200..2 * 735]));
    assert!(step(&s) <= song * 1.2, "the stop stepped {} against the song's {song}", step(&s));
    assert!(loudest(&s[5 * 735..]) < 1e-6, "silent after it");
}

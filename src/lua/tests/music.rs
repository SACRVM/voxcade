//! The game's music (its clock, its beats, the mix) and its sounds.

use super::*;

/// A game plays a song of its own (`music:play`) in time with its ticks:
/// `on_beat` tells each beat before `on_tick` (at 120 a minute one every
/// 30 ticks, a hit stop's ticks counted too), `music.beat` and `music.bar`
/// count them; a song asked for from "bar" begins at the next bar of the
/// one playing, at its beat 0. The session plays it where the game is in
/// it, and the songs' beats count for replays.
#[test]
fn a_game_goes_with_its_music() {
    let dir = crate::scratch::Scratch::new("lua-music");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/drive.mml"), "#tempo 120\n@a square duty=0.5\nA: c1 c1 c1 c1\n").unwrap();
    std::fs::write(dir.join("music/calm.mml"), "#tempo 60\n#meter 3\n@a triangle\nA: c2. c2.\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/drive.mml\", \"music/calm.mml\" }\nend\n").unwrap();
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 15, 15, 0, "#444444")
  world.state.told = ""
  world.state.quiet = music.song == nil and music.beat == nil and music.bar == nil
  music:play("drive", "now")
end

function on_beat(beat)
  world.state.told = world.state.told .. beat .. "@" .. world.tick .. ";"
end

function on_tick()
  if world.tick == 100 then music:play("calm") end
  if world.tick == 130 then
    world.state.now = music.song .. " " .. music.beat .. " " .. music.bar
    world:hitstop(40)
  end
end
"##;
    std::fs::write(dir.join("main.lua"), main).unwrap();
    let mut g = Session::<crate::Voxcade>::load(&dir).unwrap();
    assert!(g.music_changed && !g.steers_music(), "nothing of its own before the first tick");
    let mut mixer = crate::audio::Mixer::default();
    let mut buf = vec![0.0f32; crate::audio::SAMPLE_RATE as usize / 60 * 2];
    let mut snapshot = None;
    let mut hashes = Vec::new();
    for tick in 0..200 {
        g.step(&Input::default());
        g.feed_music(&mut mixer, false);
        mixer.mix(&mut buf);
        if tick == 90 {
            snapshot = Some(g.snapshot().unwrap());
        } else if tick > 90 {
            hashes.push(g.game.state_hash());
        }
    }
    assert_eq!(g.game.halted(), None);
    let text = |g: &Session<crate::Voxcade>, key: &str| match g.game.capture().unwrap().state {
        Some(crate::lua::Value::Table(t)) => t.iter().find(|(k, _)| *k == crate::lua::value::Key::Str(key.as_bytes().to_vec())).map(|(_, v)| format!("{v:?}")).unwrap_or_default(),
        _ => String::new(),
    };
    assert_eq!(text(&g, "quiet"), format!("{:?}", crate::lua::Value::Bool(true)));
    // drive's beats every 30 ticks; calm from drive's next bar (tick 121),
    // its beat 0 there; calm's beat 1 a second later, 40 of them a hit
    // stop's: told at the first tick after it, world.tick 141.
    assert_eq!(text(&g, "told"), format!("{:?}", crate::lua::Value::Str(b"0@1;1@31;2@61;3@91;0@121;1@141;".to_vec())));
    assert_eq!(text(&g, "now"), format!("{:?}", crate::lua::Value::Str(b"calm 0 0".to_vec())));
    assert!(g.steers_music());
    assert_eq!(mixer.music_title(), Some(""), "the game's song plays (calm has no #title)");
    // Brought back, it plays on the same: the clock and the song asked for
    // are the game's state.
    let told = text(&g, "told");
    g.restore(snapshot.as_ref().unwrap());
    for (n, hash) in hashes.iter().enumerate() {
        g.step(&Input::default());
        assert_eq!(g.game.state_hash(), *hash, "tick {} played out differently", 92 + n);
    }
    assert_eq!(text(&g, "told"), told);

    // Its songs' beats count for its replays.
    let before = g.fingerprint().unwrap();
    std::fs::write(dir.join("music/calm.mml"), "#tempo 61\n#meter 3\n@a triangle\nA: c2. c2.\n").unwrap();
    let again = Session::<crate::Voxcade>::load(&dir).unwrap();
    assert_ne!(again.fingerprint().unwrap(), before, "another tempo, other game data");

    // Mistakes name what is wrong.
    for (line, says) in [
        ("music:play('nope')", "music:play: no song called 'nope' in c.audio.music (its songs: drive, calm)"),
        ("music:play('drive', 'soon')", "music:play: from \"bar\", \"beat\" or \"now\", not \"soon\""),
        ("local v = music.loudness", "music has no `loudness`"),
    ] {
        std::fs::write(dir.join("main.lua"), format!("function on_tick()\n  {line}\nend\n")).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{line}: {}", halted(&g));
    }
    // The tension: 1 unless set, 0..1, counted once it is set.
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  if world.tick == 1 then world.state.was = music.intensity end\n  if world.tick == 2 then music.intensity = 0.25 end\n  world.state.now = music.intensity\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    let calm = g.game.state_hash();
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "was"), format!("{:?}", crate::lua::Value::Float(1.0f64.to_bits())));
    assert_eq!(state(&mut g, "now"), format!("{:?}", crate::lua::Value::Float(0.25f64.to_bits())));
    assert_ne!(calm, g.game.state_hash());
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  music.intensity = 2\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert!(halted(&g).contains("music.intensity: from 0 (calm) to 1"), "{}", halted(&g));
    std::fs::write(dir.join("main.lua"), "music = 3\n").unwrap();
    assert!(start_error(&dir).contains("'music' is the engine's"), "{}", start_error(&dir));
}

/// `music:play()` without a song gives the music back to the player's
/// choice from the next bar: no song of the game's, no beats, the session
/// plays the player's song again (More Blood's gunship song so).
#[test]
fn the_player_gets_the_music_back() {
    let dir = crate::scratch::Scratch::new("lua-music-back");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/mine.mml"), "#tempo 120\nA: c1\n").unwrap();
    std::fs::write(dir.join("music/boss.mml"), "#tempo 120\nA: c1\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/mine.mml\", \"music/boss.mml\" }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_tick()\n  if world.tick == 10 then music:play('boss') end\n  if world.tick == 20 then music:play() end\n  world.state.song = music.song or '-'\nend\n").unwrap();
    let mut g = Session::<crate::Voxcade>::load(&dir).unwrap();
    let mut songs = Vec::new();
    for _ in 0..200 {
        g.step(&Input::default());
        songs.push((g.steers_music(), g.song_file().map(str::to_string)));
    }
    assert_eq!(halted(&g), "");
    assert!(!songs[5].0 && songs[15].0, "the game's from tick 11");
    assert!(songs[100].0, "until the boss song's next bar (tick 131)");
    assert!(!songs[150].0, "the player's again after it");
    assert_eq!(songs[150].1.as_deref(), Some("music/mine.mml"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.audio.boss_music = \"music/boss.mml\"\nend\n").unwrap();
    assert!(start_error(&dir).contains("c.audio.boss_music is no more"), "{}", start_error(&dir));
}

/// A game with two levels, `a` and `b`, and one song: `main.lua` is
/// `main`.
fn song_folder(name: &str, main: &str) -> crate::scratch::Scratch {
    // Saw chords and noise on the eighths: low and high alike.
    song_folder_with(name, main, "#tempo 120\n@pad saw attack=0.01 decay=0.1 sustain=0.8 release=0.05 volume=0.5\n@hat noise attack=0.001 decay=0.04 sustain=0 volume=0.25\nA: @pad o3 l1 c e g a\nB: @pad o4 l1 e g b >c\nD: @hat l8 [c]32\n")
}

/// The same with a song of its own.
fn song_folder_with(name: &str, main: &str, song: &str) -> crate::scratch::Scratch {
    let dir = levels_folder(name, main, &[("a", &[]), ("b", &[])]);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/song.mml"), song).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.audio.music = { \"music/song.mml\" }\nend\n").unwrap();
    dir
}

/// The music of `dir`'s game over `ticks` steps, as the film's sound pass
/// mixes it (a window's way with `fades`: a level change fades, its frames
/// mixed as ticks are), interleaved stereo.
fn music_of(dir: &Path, ticks: usize, fades: bool) -> Vec<f32> {
    let mut g = game_in(dir);
    g.fades = fades;
    let mut mixer = crate::audio::Mixer::default();
    let mut buf = vec![0.0f32; crate::audio::SAMPLE_RATE as usize / 60 * 2];
    let mut out = Vec::new();
    for _ in 0..ticks {
        g.step(&Input::default());
        g.feed_music(&mut mixer, false);
        mixer.set_music_volume(0.5);
        mixer.mix(&mut buf);
        out.extend_from_slice(&buf);
    }
    assert_eq!(halted(&g), "");
    out
}

/// The root mean square of interleaved stereo from tick `from` to `to`.
fn rms(s: &[f32], from: usize, to: usize) -> f64 {
    let part = &s[from * 1470..to * 1470];
    (part.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / part.len() as f64).sqrt()
}

/// The largest step from one sample to the next, either side.
fn largest_step(s: &[f32]) -> f32 {
    s.chunks(2).zip(s.chunks(2).skip(1)).map(|(a, b)| (b[0] - a[0]).abs().max((b[1] - a[1]).abs())).fold(0.0, f32::max)
}

/// The energy above 2 kHz against the energy below, in decibels: the
/// spectrum of 4096-sample windows (Hann) from tick `from` on, summed.
fn above_2k_db(s: &[f32], from: usize, windows: usize) -> f64 {
    const N: usize = 4096;
    let (cos, sin): (Vec<f64>, Vec<f64>) = (0..N).map(|i| { let a = std::f64::consts::TAU * i as f64 / N as f64; (a.cos(), a.sin()) }).unzip();
    let (mut high, mut low) = (0.0, 0.0);
    for w in 0..windows {
        let at = from * 735 + w * N;
        let x: Vec<f64> = (0..N).map(|n| f64::from(s[(at + n) * 2] + s[(at + n) * 2 + 1]) * 0.5 * (0.5 - 0.5 * cos[n])).collect();
        for k in 1..N / 2 {
            let (mut re, mut im) = (0.0, 0.0);
            for (n, v) in x.iter().enumerate() {
                let i = k * n % N;
                re += v * cos[i];
                im -= v * sin[i];
            }
            let hz = k as f64 * crate::audio::SAMPLE_RATE as f64 / N as f64;
            if hz >= 2000.0 { high += re * re + im * im } else { low += re * re + im * im }
        }
    }
    10.0 * (high / low).log10()
}

/// `music:volume` and `music:muffle`: a part of the song's loudness and a
/// low-pass over it, gliding there; read back as asked; mistakes name the
/// argument; kept across a level change and in a snapshot, never in the
/// hash (a game that asks plays exactly as one that does not).
#[test]
fn the_game_turns_its_music_down_and_muffles_it() {
    let main = "function on_level_start(how)\n  music:play('song', 'now')\nend\nfunction on_tick()\n  if world.tick == 1 then world.state.was = music:volume() .. ' ' .. music:muffle() end\n  if world.tick == 2 then music:volume(0.25, 1) music:muffle(0.5) end\n  if world.tick == 3 then world.state.now = music:volume() .. ' ' .. music:muffle() end\n  if world.tick == 10 then world:go_to('b') end\n  world.state.at = level.name\nend\n";
    let dir = song_folder("music-mix", main);
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "was"), format!("{:?}", crate::lua::Value::Str(b"1.0 0.0".to_vec())));
    assert_eq!(state(&mut g, "now"), format!("{:?}", crate::lua::Value::Str(b"0.25 0.5".to_vec())));
    let mix = g.game.music_mix();
    assert_eq!((mix.volume.to, mix.volume.ticks, mix.volume.done, mix.muffle.ticks), (0.25, 60, 1, 18), "a second; 0.3 s unless said");
    let kept = g.snapshot().unwrap();
    steps(&mut g, 30);
    assert!((g.game.music_mix().volume.now() - (1.0 - 0.75 * 31.0 / 60.0)).abs() < 1e-6, "{}", g.game.music_mix().volume.now());
    steps(&mut g, 40);
    assert_eq!(g.game.music_mix().volume.now(), 0.25);
    assert_eq!(state(&mut g, "at"), format!("{:?}", crate::lua::Value::Str(b"b".to_vec())), "went on to b");
    assert_eq!((g.game.music_mix().volume.to, g.game.music_mix().muffle.to), (0.25, 0.5), "kept across the level change");
    g.restore(&kept);
    assert_eq!(g.game.music_mix(), mix, "brought back as it was");

    // Only heard: the hash is the same as a game's that never asks.
    let asks = |name: &str, line: &str| song_folder(name, &format!("function on_level_start(how)\n  music:play('song', 'now')\nend\nfunction on_tick()\n  if world.tick == 2 then {line} end\n  if world.tick == 40 then world:go_to('b') end\nend\n"));
    let (asking, not) = (asks("music-mix-asks", "music:volume(0.25, 1) music:muffle(0.5)"), asks("music-mix-none", "local _ = 1"));
    let (mut a, mut b) = (game_in(&asking), game_in(&not));
    for _ in 0..80 {
        a.step(&Input::default());
        b.step(&Input::default());
        assert_eq!(a.game.state_hash(), b.game.state_hash(), "tick {}", a.game.sim().tick);
    }

    // A crossfade is seconds, 0 to 60.
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.audio.music = { \"music/song.mml\" }\n  c.audio.crossfade = -1\nend\n").unwrap();
    assert!(start_error(&dir).contains("c.audio.crossfade = -1: from 0 to 60"), "{}", start_error(&dir));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.audio.music = { \"music/song.mml\" }\nend\n").unwrap();
    // Mistakes name the argument.
    for (line, says) in [
        ("music:volume(2)", "music:volume: level from 0 (silent) to 1 (as loud as the song is), not 2"),
        ("music:volume('loud')", "music:volume: level is a number from 0 (silent) to 1 (as loud as the song is), not \"loud\""),
        ("music:volume(0.5, -1)", "music:volume: seconds is 0 (at once) or more, not -1"),
        ("music:muffle(0.5, 'slowly')", "music:muffle: seconds is a number, 0 (at once) or more, not \"slowly\""),
        ("music:muffle(-0.1)", "music:muffle: amount from 0 (clear) to 1 (very dull, as through a wall), not -0.1"),
        ("music:muffle(nil, 1)", "music:muffle: the amount first"),
        ("music:play('song', { crossfade = -1 })", "music:play: crossfade is seconds from 0 (none) to 60, not -1"),
        ("music:play('song', { from = 'soon' })", "music:play: from \"bar\", \"beat\" or \"now\", not \"soon\""),
        ("music:play('song', { fade = 2 })", "music:play: no 'fade' (it takes from, crossfade)"),
        ("music:play('song', 3)", "music:play: after the song, \"bar\", \"beat\" or \"now\", or { from = ..., crossfade = seconds }, not 3"),
    ] {
        std::fs::write(dir.join("main.lua"), format!("function on_tick()\n  {line}\nend\n")).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{line}: {}", halted(&g));
    }
}

/// What the ears would hear, measured: a quarter of the loudness is
/// -12 dB, reached over the time asked; a muffle takes the highs away by
/// its amount; a change at once does not click; a level change leaves the
/// song going on, sample for sample, at the volume and muffle asked.
#[test]
fn the_music_sounds_as_the_game_asks() {
    let with = |name: &str, ticks: &str| song_folder(name, &format!("function on_level_start(how)\n  music:play('song')\nend\nfunction on_tick()\n{ticks}\nend\n"));
    let base = music_of(&with("music-base", ""), 300, false);
    let peak = base.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.1 && peak < 0.85, "the song plays under the limiter: peak {peak}");

    // A quarter, over half a second from tick 60.
    let quiet = music_of(&with("music-quarter", "  if world.tick == 60 then music:volume(0.25, 0.5) end"), 300, false);
    let db = 20.0 * (rms(&quiet, 120, 300) / rms(&base, 120, 300)).log10();
    assert!((db + 12.04).abs() < 0.05, "a quarter is {db:.2} dB");
    // The fade: from the first millisecond under 99 % to the first at 25.25 %.
    let ms = crate::audio::SAMPLE_RATE as usize / 1000;
    let ratio = |i: usize| {
        let (q, b) = (&quiet[i * ms * 2..(i + 1) * ms * 2], &base[i * ms * 2..(i + 1) * ms * 2]);
        q.iter().map(|q| q.abs()).sum::<f32>() / b.iter().map(|b| b.abs()).sum::<f32>().max(1e-9)
    };
    let begins = (990..5000).find(|&i| ratio(i) < 0.99).unwrap();
    let ends = (begins..5000).find(|&i| ratio(i) <= 0.2525).unwrap();
    // Those two cover (0.99 - 0.2525) / 0.75 of the whole way down.
    let fade = ((ends - begins) * ms) as f32 / crate::audio::SAMPLE_RATE as f32 / ((0.99 - 0.2525) / 0.75);
    assert!((0.49..0.52).contains(&fade), "the fade took {fade} s");
    eprintln!("volume 0.25: {db:.2} dB; the fade {fade:.3} s (asked: 0.5 s)");

    // The highs taken away, by the amount.
    let shares: Vec<f64> = [0.0, 0.5, 1.0].iter().map(|m| above_2k_db(&music_of(&with(&format!("music-muffle-{m}"), &format!("  if world.tick == 1 then music:muffle({m}, 0) end")), 240, false), 60, 8)).collect();
    eprintln!("energy above 2 kHz against below: muffle 0 {:.1} dB, 0.5 {:.1} dB, 1 {:.1} dB", shares[0], shares[1], shares[2]);
    assert!(shares[1] < shares[0] - 10.0 && shares[2] < shares[1] - 10.0, "{shares:?}");

    // At once, both ways, in the middle of held notes (sines, which step
    // little from sample to sample, so a click would show): no click.
    let smooth = |name: &str, ticks: &str| song_folder_with(name, &format!("function on_level_start(how)\n  music:play('song')\nend\nfunction on_tick()\n{ticks}\nend\n"), "#tempo 120\n@pad sine attack=0.05 sustain=1 release=0.1 volume=0.6\nA: @pad o4 c1&c1&c1&c1\nB: @pad o5 e1&e1&e1&e1\n");
    let sines = music_of(&smooth("music-sines", ""), 300, false);
    let jumps = music_of(&smooth("music-jumps", "  if world.tick == 61 then music:volume(0.1, 0) end\n  if world.tick == 101 then music:volume(1, 0) end\n  if world.tick == 141 then music:muffle(1, 0) end\n  if world.tick == 181 then music:muffle(0, 0) end"), 300, false);
    let (calm, changed) = (largest_step(&sines), largest_step(&jumps));
    eprintln!("largest step from sample to sample: the song alone {calm:.4}, with the changes at once {changed:.4}");
    assert!(changed <= calm * 1.05, "a click: {changed} against {calm}");
    // And where each change lands (its tick and the next two), against the
    // song alone there.
    for tick in [61, 101, 141, 181] {
        let at = |s: &[f32]| largest_step(&s[(tick - 1) * 1470..(tick + 2) * 1470]);
        eprintln!("  at tick {tick}: {:.4} (the song alone {:.4})", at(&jumps), at(&sines));
        assert!(at(&jumps) <= at(&sines) * 1.05, "a click at tick {tick}");
    }

    // A level change mid-song: the song goes on, as loud and as muffled.
    let stays = "  if world.tick == 50 then music:volume(0.5, 0.2) music:muffle(0.4, 0.2) end";
    let on = music_of(&with("music-on", stays), 300, false);
    let gone = music_of(&with("music-go", &format!("{stays}\n  if world.tick == 150 then world:go_to('b') end")), 300, false);
    let off = on.iter().zip(&gone).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert_eq!(off, 0.0, "the level change changed the music");
    eprintln!("a level change: the samples after it differ from a run without it by {off}");

    // In a window the song plays on through the level's fade, the game's
    // own (music:play) and the player's alike: the fade's frames take the
    // music's clock on, so the sound is the run's without a level change.
    let change = format!("{stays}\n  if world.tick == 150 then world:go_to('b') end");
    let own = music_of(&with("music-fade", &change), 300, true);
    let theirs = music_of(&song_folder("music-fade-free", &format!("function on_tick()\n{change}\nend\n")), 300, true);
    let theirs_on = music_of(&song_folder("music-fade-free-on", &format!("function on_tick()\n{stays}\nend\n")), 300, false);
    let silent = |s: &[f32]| s[100 * 1470..220 * 1470].chunks(1470).filter(|t| t.iter().all(|&s| s == 0.0)).count();
    let apart = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    eprintln!("through the window's fade (0.3 s out, 0.3 s in): silent ticks {} and {}, the samples off a run without a level change by {} and {}", silent(&own), silent(&theirs), apart(&own, &on), apart(&theirs, &theirs_on));
    assert_eq!((silent(&own), silent(&theirs)), (0, 0));
    assert_eq!((apart(&own, &on), apart(&theirs, &theirs_on)), (0.0, 0.0), "the song went on as if no level had changed");
}

/// The power of a pure tone of `hz` in the music from tick `from` to `to`
/// (both sides as one), from the song's start.
fn tone(s: &[f32], hz: f64, from: usize, to: usize) -> f64 {
    let x: Vec<f64> = s[from * 1470..to * 1470].chunks(2).map(|f| f64::from(f[0] + f[1]) * 0.5).collect();
    let w = std::f64::consts::TAU * hz / crate::audio::SAMPLE_RATE as f64;
    let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, v)| (re + v * (w * n as f64).cos(), im - v * (w * n as f64).sin()));
    (re * re + im * im) / (x.len() as f64 * x.len() as f64)
}

/// Two songs, `a` a low sine and `b` a high one, in a game of two levels.
fn two_songs(name: &str, main: &str, conf: &str) -> crate::scratch::Scratch {
    let dir = song_folder_with(name, main, "#tempo 120\n@pad sine attack=0.05 sustain=1 release=0.1 volume=0.6\nA: @pad o4 c1&c1&c1&c1&c1&c1\n");
    std::fs::rename(dir.join("music/song.mml"), dir.join("music/a.mml")).unwrap();
    std::fs::write(dir.join("music/b.mml"), "#tempo 90\n@pad sine attack=0.05 sustain=1 release=0.1 volume=0.6\nA: @pad o5 g1&g1&g1&g1&g1&g1\n").unwrap();
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.blocks = {{ file = \"level.toml\" }}\n  c.player.spawn = {{ 2, 2 }}\n  c.audio.music = {{ \"music/a.mml\", \"music/b.mml\" }}\n{conf}\nend\n")).unwrap();
    dir
}

/// A song changed crossfades when asked (`music:play`'s `crossfade`, or
/// `c.audio.crossfade`): the old fades out as the new fades in, equal in
/// power, both there in the middle, the old gone after the seconds, no
/// click, as loud and muffled as the game asks; at a level change both run
/// through the window's fade. Without a crossfade a song changed at a
/// level change leaves the rest of the fade silent (the pause between).
#[test]
fn one_song_crossfades_into_the_next() {
    const LOW: f64 = 261.63;
    const HIGH: f64 = 783.99;
    let game = |name: &str, ticks: &str, conf: &str| two_songs(name, &format!("function on_level_start(how)\n  if how == 'start' then music:play('a', 'now') end\nend\nfunction on_tick()\n{ticks}\n  world.state.beat = music.beat\nend\n"), conf);
    let low = music_of(&game("cross-a", "", ""), 300, false);
    let high = music_of(&game("cross-b", "  if world.tick == 1 then music:play('b', 'now') end", ""), 300, false);
    let crossed = music_of(&game("cross", "  if world.tick == 60 then music:play('b', { from = 'now', crossfade = 2 }) end", ""), 300, false);
    // It begins at tick 61, its middle a second later.
    let (mid, end) = (121, 182);
    let together = 10.0 * ((rms(&crossed, mid - 5, mid + 5).powi(2)) / ((rms(&low, mid - 5, mid + 5).powi(2) + rms(&high, mid - 5, mid + 5).powi(2)) / 2.0)).log10();
    let (l, h) = (10.0 * (tone(&crossed, LOW, mid - 5, mid + 5) / tone(&low, LOW, mid - 5, mid + 5)).log10(), 10.0 * (tone(&crossed, HIGH, mid - 5, mid + 5) / tone(&high, HIGH, mid - 5, mid + 5)).log10());
    let gone = 10.0 * (tone(&crossed, LOW, end + 2, end + 30) / tone(&low, LOW, end + 2, end + 30)).log10();
    let steps = (largest_step(&crossed), largest_step(&low).max(largest_step(&high)));
    eprintln!("a crossfade of 2 s: in its middle {together:+.2} dB against the two songs' mean power, the old song {l:+.1} dB, the new {h:+.1} dB; the old after it {gone:.0} dB; largest step {:.4} (the songs alone {:.4})", steps.0, steps.1);
    assert!(together.abs() < 1.0, "a dip of {together} dB");
    assert!((l + 3.0).abs() < 1.0 && (h + 3.0).abs() < 1.0, "both at half power in the middle: {l} {h}");
    assert!(gone < -50.0, "the old song still sounds: {gone} dB");
    assert!(steps.0 <= steps.1 * 1.05, "a click");
    // Over the game's c.audio.crossfade the same; quieter as asked.
    let by_conf = music_of(&game("cross-conf", "  if world.tick == 60 then music:play('b', 'now') end", "  c.audio.crossfade = 2"), 300, false);
    assert_eq!(by_conf, crossed, "c.audio.crossfade = 2 is {{ crossfade = 2 }}");
    let quiet = music_of(&game("cross-quiet", "  if world.tick == 2 then music:volume(0.25, 0) end\n  if world.tick == 60 then music:play('b', { from = 'now', crossfade = 2 }) end", ""), 300, false);
    let db = 20.0 * (rms(&quiet, mid - 5, mid + 5) / rms(&crossed, mid - 5, mid + 5)).log10();
    assert!((db + 12.04).abs() < 0.1, "music:volume over the crossfade: {db} dB");

    // At a level change in a window: the new song asked in on_level_start.
    let changing = |name: &str, how: &str| two_songs(name, &format!("function on_level_start(how)\n  if how == 'start' then music:play('a', 'now') else music:play('b', {how}) end\nend\nfunction on_tick()\n  if world.tick == 60 then world:go_to('b') end\nend\n"), "");
    let faded = music_of(&changing("cross-fade", "{ from = 'now', crossfade = 2 }"), 300, true);
    let cut = music_of(&changing("cut-fade", "'now'"), 300, true);
    let silent = |s: &[f32]| s[40 * 1470..140 * 1470].chunks(1470).filter(|t| t.iter().all(|&s| s == 0.0)).count();
    // The fade: from tick 60 about 19 steps out, the change (the new song
    // asked), 19 in; 85 to 95 late in it.
    let both = (10.0 * (tone(&faded, LOW, 85, 95) / tone(&low, LOW, 85, 95)).log10(), 10.0 * (tone(&faded, HIGH, 85, 95) / tone(&high, HIGH, 85, 95)).log10());
    eprintln!("at a level change with the window's fade: crossfading, {} silent ticks, the old {:+.1} dB and the new {:+.1} dB late in the fade; without, {} silent ticks", silent(&faded), both.0, both.1, silent(&cut));
    assert_eq!(silent(&faded), 0);
    assert!(both.0 > -20.0 && both.1 > -20.0, "both songs sound across the fade: {both:?}");
    assert!((15..=20).contains(&silent(&cut)), "the pause between the songs: {}", silent(&cut));
}

/// A run played where level changes fade takes the fades' steps on the
/// music's clock; its replay (`fades = true`) takes them without a
/// picture and plays the same; without them it would run apart.
#[test]
fn a_replay_takes_the_fades_on_the_music_clock() {
    use crate::replay::{Recorder, Replay, Verdict, play_through};
    let main = "function on_level_start(how)\n  music:play('song')\nend\nfunction on_beat(beat)\n  world.state.beats = (world.state.beats or 0) + 1\nend\nfunction on_tick()\n  if world.tick % 100 == 50 then world:go_to(level.name == 'a' and 'b' or 'a') end\nend\n";
    let dir = song_folder("fade-replay", main);
    let mut g = game_in(&dir);
    g.fades = true;
    let mut recorder = Recorder::default();
    let mut ticks = 0;
    while ticks < 600 {
        g.step(&Input::default());
        recorder.record(&g);
        ticks += usize::from(g.ticked);
    }
    assert_eq!(halted(&g), "");
    let replay = recorder.replay(&g).unwrap();
    assert!(replay.fades);
    let path = dir.join("faded.toml");
    replay.save(&path).unwrap();
    let back = Replay::load(&path).unwrap();
    assert_eq!(back, replay);
    assert_eq!(play_through::<LuaGame>(&dir, &back).unwrap(), Verdict::Same);
    assert!(matches!(play_through::<LuaGame>(&dir, &Replay { fades: false, ..back }).unwrap(), Verdict::RunsApart(_)), "the fades' steps count");
    // Every beat told once, as many as the song has in the steps it went.
    let steps = 600 + 6 * 2 * 18;
    assert_eq!(state(&mut g, "beats"), format!("{:?}", crate::lua::Value::Int((steps as i64 - 1) * 120 / 3600 + 1)));
}

/// `c.audio.drive` pushes the game's sound into the limiter, 0 to 12
/// decibels; outside that it is a mistake that names it.
#[test]
fn the_drive_is_from_0_to_12() {
    let dir = crate::scratch::Scratch::new("lua-drive");
    std::fs::write(dir.join("main.lua"), "function on_tick() end\n").unwrap();
    for bad in ["13", "-1", "0/0"] {
        std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.size = {{ 16, 16, 8 }}\n  c.audio.drive = {bad}\nend\n")).unwrap();
        assert!(start_error(&dir).contains("c.audio.drive"), "{bad}: {}", start_error(&dir));
    }
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.drive = 6\nend\n").unwrap();
    assert_eq!(Session::<crate::Voxcade>::load(&dir).unwrap().cfg.audio.drive, 6.0);
}

/// A sound is a file: `sounds/x.toml` plays as "x"; `c.sounds` in
/// conf.lua is refused with the way out; a name with no file halts the
/// script saying where sounds come from.
#[test]
fn a_sound_is_a_file_of_the_game() {
    let dir = folder("sound-file", "function on_tick()\n  level:sound(\"zap\", { x = 8, y = 8, z = 0 })\nend\n");
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/zap.toml"), "wave = \"saw\"\nfreq = 600\nsweep = 7\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert!(g.game.stage.host.sim().sounds.iter().any(|s| s.name == "zap"), "it plays");
    assert!(g.sounds.get("zap").is_some() && g.sounds.get("explosion").is_some(), "the game's and the engine's");
    std::fs::write(dir.join("conf.lua"), CONF.replace("c.things.miner = {}", "c.things.miner = {}\n  c.sounds.zap = { wave = \"saw\" }")).unwrap();
    let e = start_error(&dir);
    assert!(e.starts_with("c.sounds is no more: a sound is a file now, sounds/NAME.toml") && e.contains("--to-files"), "{e}");
    std::fs::write(dir.join("conf.lua"), CONF).unwrap();
    std::fs::remove_file(dir.join("sounds/zap.toml")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert!(halted(&g).contains("no sound called 'zap': a sound is a file, sounds/zap.toml (the game's: impact, explosion, crumble)"), "{}", halted(&g));
}

/// A sound file saved while the game runs is made anew, and only it; a
/// new one is there to play at once, one taken away falls back to the
/// kit's of its name or is gone. A sealed game plays its sounds from its
/// files, and an applied one is made anew as well.
#[test]
fn a_sound_file_comes_in_while_playing() {
    let dir = kit_folder("sound-reload", "", "function on_tick()\n  if world.state.boom then level:sound(\"boom\", { x = 8, y = 8, z = 0 }) end\nend\n");
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 600\n").unwrap();
    std::fs::write(dir.join("sounds/shot.toml"), "wave = \"sine\"\nhold = 0\ndecay = 0.05\n").unwrap();
    let mut g = game_in(&dir);
    let kits_shot = g.sounds.get("splat").unwrap().clone();
    let zap = g.sounds.get("zap").unwrap().clone();
    let reload = |g: &mut Session<LuaGame>, file: &str| g.reload(&dir.join(file)).unwrap();
    // One changed: only it is made anew.
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 300\ndecay = 0.4\n").unwrap();
    assert_eq!(reload(&mut g, "sounds/zap.toml"), crate::session::Reloaded::Sounds);
    assert!(!std::sync::Arc::ptr_eq(g.sounds.get("zap").unwrap(), &zap), "zap made anew");
    assert!(std::sync::Arc::ptr_eq(g.sounds.get("splat").unwrap(), &kits_shot), "the others kept");
    // One new: the script may play it now.
    std::fs::write(dir.join("sounds/boom.toml"), "wave = \"noise\"\n").unwrap();
    assert_eq!(reload(&mut g, "sounds/boom.toml"), crate::session::Reloaded::Sounds);
    g.eval("world.state.boom = true").unwrap();
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert!(g.game.stage.host.sim().sounds.iter().any(|s| s.name == "boom"), "the new sound plays");
    // A mistake keeps the sounds as they were and says where.
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 300\ndecay = 40\n").unwrap();
    let e = format!("{:#}", g.reload(&dir.join("sounds/zap.toml")).unwrap_err());
    assert_eq!(e, "sounds/zap.toml line 2: decay = 40: from 0 to 10 seconds (fading from full volume to silence at the end, 0.005 at least)");
    assert_eq!(g.trouble(), Some(e));
    // The game's shot taken away: the kit's again.
    let games_shot = g.sounds.get("shot").unwrap().len();
    std::fs::remove_file(dir.join("sounds/shot.toml")).unwrap();
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 300\n").unwrap();
    assert_eq!(reload(&mut g, "sounds/shot.toml"), crate::session::Reloaded::Sounds);
    assert_ne!(g.sounds.get("shot").unwrap().len(), games_shot, "the kit's shot");
    assert_eq!(g.trouble(), None);
    // A file gone elsewhere changes nothing.
    assert_eq!(g.reload(&dir.join("gone.vvx")).unwrap(), crate::session::Reloaded::Nothing);

    // Sealed: the sounds are its files', whatever the folder holds since.
    let store_dir = crate::scratch::Scratch::new("sound-store");
    let store = crate::files::Store::at(&store_dir);
    let (_, files) = crate::files::Files::seal(&dir, &store).unwrap();
    let mut sealed = Session::<LuaGame>::load_from(files, crate::saves::Slots::default()).unwrap();
    assert!(sealed.sounds.get("boom").is_some() && sealed.sounds.get("zap").is_some(), "its files' sounds");
    std::fs::remove_file(dir.join("sounds/boom.toml")).unwrap();
    assert_eq!(sealed.reload(&dir.join("sounds/boom.toml")).unwrap(), crate::session::Reloaded::Nothing, "the folder is none of a sealed game's");
    std::fs::write(dir.join("sounds/zap.toml"), "freq = 1200\ndecay = 0.6\n").unwrap();
    let before = sealed.sounds.get("zap").unwrap().len();
    let changes = sealed.files.changes_since(&store).unwrap();
    assert_eq!(sealed.apply(std::sync::Arc::new(changes)).unwrap(), crate::session::Reloaded::Sounds);
    assert_ne!(sealed.sounds.get("zap").unwrap().len(), before, "applied, made anew");
    assert!(sealed.sounds.get("boom").is_none(), "and the one taken away is gone");
}

/// `voxcade sounds GAME --to-files` writes each of `c.sounds` as a file,
/// with the comments that told about it, the numbers in the file's words,
/// exactly as they sounded.
#[test]
fn the_converter_writes_c_sounds_as_files() {
    let dir = folder("sound-convert", "function on_tick() end\n");
    let conf = CONF.replace("c.things.miner = {}", "c.things.miner = {}\n  -- a laser\n  c.sounds.shot = { wave = \"square\", freq = 1400, freq_end = 500, sustain = 0.01, decay = 0.07, volume = 0.18 }\n  c.sounds.coin = {\n    wave = \"triangle\", freq = 880, arp = 1.5, arp_at = 0.05,\n  }   -- a coin\n  c.sounds.rotor = \"none\"");
    std::fs::write(dir.join("conf.lua"), &conf).unwrap();
    crate::headless::sounds_to_files::<LuaGame>(&dir).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("sounds/shot.toml")).unwrap(), "# a laser\nwave = \"square\"\nfreq = 1400\nsweep = -17.825122\nhold = 0.01\ndecay = 0.07\nvolume = 0.18\n");
    assert_eq!(std::fs::read_to_string(dir.join("sounds/coin.toml")).unwrap(), "# a coin\nwave = \"triangle\"\nfreq = 880\njump = 7.01955\njump_at = 0.05\n");
    assert_eq!(std::fs::read_to_string(dir.join("sounds/rotor.toml")).unwrap(), "silent = true\n");
    let lines: Vec<&str> = conf.lines().collect();
    let found: Vec<(Option<String>, usize, usize)> = crate::headless::sound_statements(&lines).into_iter().map(|s| (s.name, s.from, s.to)).collect();
    let first = lines.iter().position(|l| l.contains("-- a laser")).unwrap();
    assert_eq!(found, [(Some("shot".into()), first, first + 1), (Some("coin".into()), first + 2, first + 4), (Some("rotor".into()), first + 5, first + 5)]);
    // Taken out of conf.lua, the game plays them.
    std::fs::write(dir.join("conf.lua"), CONF).unwrap();
    let g = game_in(&dir);
    assert!(g.sounds.get("shot").is_some() && g.sounds.get("coin").is_some());
}

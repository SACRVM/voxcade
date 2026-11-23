//! `music:mix`: each channel's strip on the mixing desk (fader, mute, pan).

use super::*;

/// Two songs of two channels of long sine notes, 120 beats a minute: "a"
/// with A at 420 Hz and B at 660, "b" with A at 300 and B at 540 (each
/// note held, so a channel sounds steadily; whole periods in a tick, so
/// one tone does not leak into another's measure).
fn desk_folder(name: &str, main: &str) -> crate::scratch::Scratch {
    let dir = folder(name, main);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    let ins = |name: &str, hz: u32| format!("@{name} sine freq={hz} attack=0.002 decay=0 sustain=1 release=0.01 volume=0.5\n");
    std::fs::write(dir.join("music/a.mml"), format!("#tempo 120\n{}{}A: @x c1 c1\nB: @y c1 c1\n", ins("x", 420), ins("y", 660))).unwrap();
    std::fs::write(dir.join("music/b.mml"), format!("#tempo 120\n{}{}A: @x c1 c1\nB: @y c1 c1\n", ins("x", 300), ins("y", 540))).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/a.mml\", \"music/b.mml\" }\nend\n").unwrap();
    dir
}

/// The music `g` plays over `ticks` steps, fed and mixed as a window
/// does, interleaved stereo.
fn played(g: &mut Session<LuaGame>, mixer: &mut crate::audio::Mixer, ticks: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; crate::audio::SAMPLE_RATE as usize / 60 * 2];
    let mut out = Vec::new();
    for _ in 0..ticks {
        g.step(&Input::default());
        g.feed_music(mixer, false);
        mixer.mix(&mut buf);
        out.extend_from_slice(&buf);
    }
    out
}

/// How strongly `hz` sounds on side `side` (0 left, 1 right) of
/// interleaved `s` in tick `tick` (from 1), as an amplitude.
fn tone(s: &[f32], tick: usize, side: usize, hz: f64) -> f64 {
    let n = crate::audio::SAMPLE_RATE as usize / 60;
    let x: Vec<f32> = s[(tick - 1) * n * 2..tick * n * 2].iter().skip(side).step_by(2).copied().collect();
    let w = std::f64::consts::TAU * hz / crate::audio::SAMPLE_RATE as f64;
    let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (k, &v)| (re + f64::from(v) * (w * k as f64).cos(), im - f64::from(v) * (w * k as f64).sin()));
    2.0 * (re * re + im * im).sqrt() / x.len() as f64
}

/// A fader glides to its value over the seconds asked and reads back as
/// asked; a mute at once silences its channel within a few milliseconds
/// without a click, the other channel untouched; a pan turns the far
/// side down, the near as it was; false gives a key and the whole strip
/// back.
#[test]
fn a_strip_glides_to_what_was_asked() {
    let main = r#"
function on_level_start(how) music:play('a', 'now') end
function on_tick()
  if world.tick == 30 then music:mix('B', { volume = 0.5 }, 0.5) end
  if world.tick == 31 then
    local m = music:mix('B')
    world.state.read = m.volume .. ' ' .. tostring(m.mute) .. ' ' .. m.pan
  end
  if world.tick == 90 then music:mix('A', { mute = true }, 0) end
  if world.tick == 120 then music:mix('B', { pan = 1 }, 0) end
  if world.tick == 150 then music:mix('A', { mute = false, volume = false }, 0); music:mix('B', { pan = false }) end
  if world.tick == 180 then music:mix('B', false, 0) end
  if world.tick == 181 then
    local m = music:mix('B')
    world.state.back = m.volume .. ' ' .. tostring(m.mute) .. ' ' .. m.pan
  end
end
"#;
    let dir = desk_folder("music-mix", main);
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let s = played(&mut g, &mut mixer, 200);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "read"), format!("{:?}", crate::lua::Value::Str(b"0.5 false 0.0".to_vec())));
    assert_eq!(state(&mut g, "back"), format!("{:?}", crate::lua::Value::Str(b"1.0 false 0.0".to_vec())));
    let (a, b) = (420.0, 660.0);
    let full = tone(&s, 20, 0, b);
    assert!(full > 0.05 && (tone(&s, 20, 0, a) - full).abs() < 0.02 * full, "both channels as written");
    let half = tone(&s, 60, 0, b) / full;
    assert!((half - 0.5).abs() < 0.01, "the fader at a half after half a second: {half}");
    let midway = tone(&s, 46, 0, b) / full;
    assert!(midway > 0.6 && midway < 0.9, "gliding: {midway}");
    // Muted at once: gone in the next tick, B as it was.
    assert!(tone(&s, 92, 0, a) < 0.001 * full, "{}", tone(&s, 92, 0, a));
    assert!((tone(&s, 92, 0, b) / full - 0.5).abs() < 0.01);
    let n = crate::audio::SAMPLE_RATE as usize / 60 * 2;
    let left: Vec<f32> = s[85 * n..95 * n].iter().step_by(2).copied().collect();
    let step = left.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    let own: Vec<f32> = s[60 * n..70 * n].iter().step_by(2).copied().collect();
    let own = own.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    assert!(step <= own * 1.05, "the mute stepped {step}, the song itself {own}");
    // Panned right: its left side silent, its right as before.
    assert!(tone(&s, 122, 0, b) < 0.001 * full && (tone(&s, 122, 1, b) / full - 0.5).abs() < 0.01);
    // Given back.
    assert!((tone(&s, 175, 0, a) / full - 1.0).abs() < 0.01 && (tone(&s, 175, 0, b) / full - 0.5).abs() < 0.01);
    assert!((tone(&s, 185, 0, b) / full - 1.0).abs() < 0.01);
}

/// A channel muted before its song begins is silent from the song's first
/// sample; a strip holds by letter when another song begins, and through
/// a stop.
#[test]
fn a_muted_channel_is_silent_from_its_songs_first_sample() {
    let main = r#"
function on_level_start(how)
  music:mix('B', { mute = true }, 0)
  music:play('a', 'now')
end
function on_tick()
  if world.tick == 60 then music:play('b', 'now') end
  if world.tick == 90 then music:stop() end
  if world.tick == 100 then music:play('a', 'now') end
end
"#;
    let dir = desk_folder("music-mix-start", main);
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let s = played(&mut g, &mut mixer, 120);
    assert_eq!(halted(&g), "");
    let full = tone(&s, 20, 0, 420.0);
    assert!(full > 0.05);
    for tick in [20, 62, 80, 102, 110] {
        let b = if (61..100).contains(&tick) { 540.0 } else { 660.0 };
        assert!(tone(&s, tick, 0, b) < 0.001 * full, "B heard at tick {tick}: {}", tone(&s, tick, 0, b));
    }
    assert!(tone(&s, 70, 0, 300.0) > 0.9 * full, "b's A plays");
    // The same songs without B, sample for sample: B never sounded, not
    // even for the first milliseconds of a song.
    let dir = desk_folder("music-mix-start-alone", &main.replace("music:mix('B', { mute = true }, 0)", ""));
    for song in ["a", "b"] {
        let text = std::fs::read_to_string(dir.join(format!("music/{song}.mml"))).unwrap();
        std::fs::write(dir.join(format!("music/{song}.mml")), text.replace("B: @y c1 c1\n", "")).unwrap();
    }
    let mut alone = game_in(&dir);
    let alone = played(&mut alone, &mut crate::audio::Mixer::default(), 120);
    assert!(s == alone, "B heard at sample {:?}", s.iter().zip(&alone).position(|(a, b)| a != b));
}

/// Laid channels swapped in keep their strips by letter; a letter the
/// studio lays no more and lays again finds its strip waiting.
#[test]
fn a_strip_holds_through_a_change_within_the_song() {
    let main = r#"
local function lay(h) music:play('kit', { from = 'now', channels = h and { K = '@x c1', H = '@y c1' } or { K = '@x c1' } }) end
function on_level_start(how)
  music:mix('H', { mute = true }, 0)
  lay(true)
end
function on_tick()
  if world.tick == 30 then lay(false) end
  if world.tick == 60 then lay(true) end
  if world.tick == 150 then music:mix('H', { mute = false }, 0) end
end
"#;
    let dir = desk_folder("music-mix-laid", main);
    std::fs::write(dir.join("music/kit.mml"), "#tempo 120\n@x sine freq=420 attack=0.002 decay=0 sustain=1 release=0.01 volume=0.5\n@y sine freq=660 attack=0.002 decay=0 sustain=1 release=0.01 volume=0.5\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/kit.mml\" }\nend\n").unwrap();
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let s = played(&mut g, &mut mixer, 170);
    assert_eq!(halted(&g), "");
    let full = tone(&s, 20, 0, 420.0);
    assert!(full > 0.05);
    for tick in [2, 20, 125, 140] {
        assert!(tone(&s, tick, 0, 660.0) < 0.001 * full, "H heard at tick {tick}");
    }
    assert!(tone(&s, 160, 0, 660.0) > 0.9 * full, "H unmuted");
}

/// The strips are only heard: kept in a snapshot (a game brought back
/// sounds as it did) and in no hash; a letter the song file lacks is a
/// mistake, any letter before a song of the game's plays waits.
#[test]
fn strips_are_heard_only_and_their_letters_are_checked() {
    let quiet = "function on_level_start(how) music:play('a', 'now') end\nfunction on_tick() world.state.t = world.tick end\n";
    let busy = "function on_level_start(how) music:play('a', 'now') end\nfunction on_tick()\n  world.state.t = world.tick\n  music:mix('A', { volume = (world.tick % 7) / 7, pan = 0.5, cutoff = 200 + world.tick * 10, decay = 0.1 })\nend\n";
    let (q, b) = (desk_folder("music-mix-quiet", quiet), desk_folder("music-mix-busy", busy));
    let (mut q, mut b) = (game_in(&q), game_in(&b));
    for _ in 0..40 {
        q.step(&Input::default());
        b.step(&Input::default());
        assert_eq!(q.game.state_hash(), b.game.state_hash(), "a strip changed the hash");
    }
    let kept = b.snapshot().unwrap();
    let mix = b.game.music_mix();
    steps(&mut b, 5);
    assert_ne!(b.game.music_mix(), mix);
    b.restore(&kept);
    assert_eq!(b.game.music_mix(), mix, "brought back as it was");

    for (line, says) in [
        ("music:mix('C', { volume = 0.5 })", "music:mix: no channel C in your song (a has A, B)"),
        ("music:mix('A', { wave = 'saw' })", "music:mix: wave does not turn: the wave is another sound"),
        ("music:mix('A', { cutof = 900 })", "music:mix: no word cutof (did you mean cutoff?)"),
        ("music:mix('A', { hold = 1 })", "music:mix: hold is a word of sound files"),
        ("music:mix('A', { cutoff = 10 })", "music:mix: cutoff is a number from 20 to 20000 Hz, or false"),
        ("music:mix('A', { crush_bits = 4.5 })", "music:mix: crush_bits is a whole number from 1 to 16 bits"),
        ("music:mix('AB', { volume = 1 })", "music:mix: a channel is a letter"),
        ("music:mix('A', { volume = 2 })", "music:mix: volume is a number from 0 (silent) to 1"),
        ("music:mix('A', { pan = -3 })", "music:mix: pan is a number from -1 (left) to 1"),
        ("music:mix('A', { mute = 1 })", "music:mix: mute is true or false"),
        ("music:mix('A', 3)", "music:mix: after the channel"),
    ] {
        let dir = desk_folder("music-mix-wrong", &format!("function on_level_start(how) music:play('a', 'now') end\nfunction on_tick()\n  {line}\nend\n"));
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{line}: {}", halted(&g));
    }
    // Before a song of the game's plays, and over a song to come, any
    // letter of it waits.
    let dir = desk_folder("music-mix-early", "function on_level_start(how)\n  music:mix('Q', { mute = true })\n  music:play('a')\n  music:mix('B', { mute = true })\nend\n");
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert!(g.game.music_mix().strip('Q').mute && g.game.music_mix().strip('B').mute);
}

/// An instrument's words on a strip read back as asked, false gives one
/// back and false the whole strip every one; a word the channel's
/// instrument lacks (a cutoff on a sine without a filter) is taken and
/// changes nothing it plays.
#[test]
fn a_strips_knobs_read_back_and_a_word_the_instrument_lacks_does_nothing() {
    let main = r#"
function on_level_start(how) music:play('a', 'now') end
function on_tick()
  if world.tick == 10 then music:mix('A', { cutoff = 900, resonance = 0.8, decay = 0.05 }, 0.1) end
  if world.tick == 11 then
    local m = music:mix('A')
    world.state.read = m.cutoff .. ' ' .. m.resonance .. ' ' .. m.decay .. ' ' .. tostring(m.duty)
  end
  if world.tick == 20 then music:mix('A', { resonance = false }) end
  if world.tick == 21 then world.state.less = tostring(music:mix('A').resonance) .. ' ' .. music:mix('A').cutoff end
  if world.tick == 30 then music:mix('A', false) end
  if world.tick == 31 then
    local n = 0
    for _ in pairs(music:mix('A')) do n = n + 1 end
    world.state.back = n
  end
end
"#;
    let dir = desk_folder("music-mix-knobs", main);
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let turned = played(&mut g, &mut mixer, 40);
    assert_eq!(halted(&g), "");
    let s = |v: &str| format!("{:?}", crate::lua::Value::Str(v.as_bytes().to_vec()));
    assert_eq!(state(&mut g, "read"), s("900.0 0.80000001192093 0.050000000745058 nil"));
    assert_eq!(state(&mut g, "less"), s("nil 900.0"));
    assert_eq!(state(&mut g, "back"), format!("{:?}", crate::lua::Value::Int(3)));
    // The sines have no filter and hold their notes: nothing they play changed.
    let dir = desk_folder("music-mix-knobs-not", "function on_level_start(how) music:play('a', 'now') end\n");
    let plain = played(&mut game_in(&dir), &mut crate::audio::Mixer::default(), 40);
    assert!(turned == plain, "a word the instrument lacks changed the sound at sample {:?}", turned.iter().zip(&plain).position(|(a, b)| a != b));
}

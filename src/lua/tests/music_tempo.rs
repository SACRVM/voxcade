//! `music.tempo`: the tempo the game sets, and the music's clock in units.

use super::*;
use crate::lua::host::{Music, SongAsk, UNITS, When};

/// A song's timing as the game is told it: a tempo, a meter.
fn timing(tempo: f32, meter: u32) -> crate::audio::music::Timing {
    crate::audio::music::Timing { name: format!("t{tempo}"), tempo, meter, round: None, bars: 1, studio: None, letters: vec!['A'] }
}

/// The clock as it was before tempos: the steps at the song's `#tempo`.
fn todays(steps: u64, tempo: f32, meter: u32) -> (u64, u64, u64) {
    let b = steps as f64 * tempo as f64 / 3600.0;
    (b.floor() as u64, (b / f64::from(meter)).floor() as u64, b.to_bits())
}

fn read(m: &Music, songs: &[crate::audio::music::Timing]) -> (u64, u64, u64) {
    let c = m.clock(songs).unwrap();
    (c.beat, c.bar, c.position.to_bits())
}

/// While no tempo is set the clock is today's formula, to the bit; and
/// after a whole tempo is set, counted in units, it still is: every whole
/// tempo from 1 to 999, an hour of ticks each, set to the song's own
/// tempo in the middle of the song and set to another from a song's
/// start (so Pulsar's replays, which go by on_beat, and stage 1's bars
/// never move).
#[test]
fn the_clock_is_todays_while_unset_and_after_a_whole_tempo_is_set() {
    const HOUR: u64 = 216_000;
    let tempos: Vec<u32> = (1..=999).collect();
    std::thread::scope(|s| {
        for chunk in tempos.chunks(42) {
            s.spawn(move || {
                for &tempo in chunk {
                    let meter = if tempo % 3 == 0 { 3 } else { 4 };
                    let songs = [timing(tempo as f32, meter), timing(120.0, meter)];
                    let (mut unset, mut same, mut other) = (Music { song: Some(0), ..Music::default() }, Music { song: Some(0), ..Music::default() }, Music::default());
                    other.tempo = Some(tempo * 1000);
                    other.next = Some(SongAsk { song: Some(1), stop: false, when: When::Now, crossfade: None, at: 1, laid: None });
                    other.step(&songs);
                    assert_eq!(other.units, Some(0), "a song begun under a tempo counts in units");
                    for step in 1..=HOUR {
                        if step == 1000 {
                            same.tempo = Some(tempo * 1000);
                        }
                        unset.step(&songs);
                        same.step(&songs);
                        other.step(&songs);
                        let want = todays(step, tempo as f32, meter);
                        assert_eq!(read(&unset, &songs), want, "unset at {tempo}, step {step}");
                        assert_eq!(read(&same, &songs), want, "set to its own {tempo} at step 1000, step {step}");
                        assert_eq!(read(&other, &songs), want, "120 set to {tempo} from its start, step {step}");
                    }
                    assert!(unset.units.is_none() && same.units.is_some());
                }
            });
        }
    });
}

/// A fractional `#tempo` (124.4, an f32 a hair over it) never tells a
/// beat twice nor skips one where the clock goes over to units, whenever
/// that is, nor with a tempo set anew every tick.
#[test]
fn a_fractional_tempo_never_tells_a_beat_twice() {
    let songs = [timing(124.4, 4)];
    let check = |m: &mut Music, set: &dyn Fn(u64) -> Option<u32>| {
        let mut told = Vec::new();
        for step in 0..7200 {
            if let Some(t) = set(step) {
                m.tempo = Some(t);
            }
            m.step(&songs);
            told.extend(m.untold(&songs));
        }
        let last = m.clock(&songs).unwrap().beat;
        assert_eq!(told, (0..=last).collect::<Vec<_>>());
    };
    for switch in 0..240 {
        check(&mut Music { song: Some(0), ..Music::default() }, &|step| (step == switch).then_some(124_400));
    }
    check(&mut Music { song: Some(0), ..Music::default() }, &|step| Some(124_400 + (step % 7) as u32 * 100));
}

/// A tempo changed in the middle of a bar: a song asked for from the next
/// bar or beat begins exactly at the step its line is crossed at the new
/// tempo, counted in units.
#[test]
fn a_tempo_changed_mid_bar_keeps_the_changes_on_their_lines() {
    let songs = [timing(120.0, 4), timing(90.0, 4)];
    for (when, line) in [(When::Bar, 4 * UNITS), (When::Beat, UNITS)] {
        let mut m = Music { song: Some(0), ..Music::default() };
        m.next = Some(SongAsk { song: Some(1), stop: false, when, crossfade: None, at: 1, laid: None });
        // Within the first beat and bar: 20 steps at 120 is 2/3 of a beat.
        for _ in 0..20 {
            m.step(&songs);
        }
        m.tempo = Some(160_000);
        let mut units = 0;
        let mut step = 20;
        while m.song == Some(0) {
            units = m.units.unwrap_or(0);
            m.step(&songs);
            step += 1;
        }
        // The step before it the line was not reached; at it, it was.
        assert!(units < line && units + 160_000 >= line, "{when:?}: {units}");
        let expected = 20 + (line - 20 * 120_000).div_ceil(160_000);
        assert_eq!(step, expected, "{when:?}");
        assert_eq!((m.song, m.units, m.steps), (Some(1), Some(0), 0), "the next song begins at the tempo held, its units from 0");
    }
}

/// `music.tempo`: the song's own `#tempo` until set, read through
/// thousandths; set, kept to a thousandth, holding across songs and a
/// stop until set nil; nil while none of the game's plays and none is
/// set. `on_beat` and the bars go with it; counted for replays only once
/// a game sets one; a song gone from the list keeps the tempo and the
/// strips the game holds.
#[test]
fn a_game_sets_its_songs_tempo() {
    let dir = folder("music-tempo", r#"
function on_level_start(how) world.state.none = tostring(music.tempo) end
function on_beat(b) world.state.beats = (world.state.beats or '') .. b .. '@' .. world.tick .. ';' end
function on_tick()
  if world.tick == 1 then music:play('a', 'now') end
  if world.tick == 2 then world.state.own = music.tempo end
  if world.tick == 40 then music.tempo = 240.4567 end
  if world.tick == 41 then world.state.set = music.tempo end
  if world.tick == 100 then music:play('b', 'now') end
  if world.tick == 101 then world.state.held = music.tempo end
  if world.tick == 120 then music:stop() end
  if world.tick == 121 then world.state.stopped = music.tempo end
  if world.tick == 130 then music.tempo = nil end
  if world.tick == 131 then world.state.gone = tostring(music.tempo) end
  if world.tick == 140 then music:play('b', 'now') end
  if world.tick == 141 then world.state.back = music.tempo end
end
"#);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/a.mml"), "#tempo 124.4\nA: c1\n").unwrap();
    std::fs::write(dir.join("music/b.mml"), "#tempo 90\nA: c1\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/a.mml\", \"music/b.mml\" }\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 150);
    assert_eq!(halted(&g), "");
    let float = |v: f64| format!("{:?}", crate::lua::Value::Float(v.to_bits()));
    assert_eq!(state(&mut g, "none"), format!("{:?}", crate::lua::Value::Str(b"nil".to_vec())));
    assert_eq!(state(&mut g, "own"), float(124.4), "the song's own, to a thousandth");
    assert_eq!(state(&mut g, "set"), float(240.457), "kept to a thousandth");
    assert_eq!(state(&mut g, "held"), float(240.457), "held for the next song");
    assert_eq!(state(&mut g, "stopped"), float(240.457), "held through a stop");
    assert_eq!(state(&mut g, "gone"), format!("{:?}", crate::lua::Value::Str(b"nil".to_vec())));
    assert_eq!(state(&mut g, "back"), float(90.0));
    // a's beats at 124.4 (one every 28.94 ticks) until tick 41, at 240.457
    // after (one every 14.97 ticks); b's from 0 at tick 101.
    let beats = state(&mut g, "beats");
    assert_eq!(beats, format!("{:?}", crate::lua::Value::Str(b"0@2;1@31;2@51;3@66;4@81;5@96;0@101;1@116;0@141;".to_vec())));

    // Counted for replays only once set.
    let quiet = folder("music-tempo-quiet", "function on_tick()\n  if world.tick == 1 then music:play('a', 'now') end\n  if world.tick == 50 then music.tempo = 124.4 end\nend\n");
    std::fs::create_dir_all(quiet.join("music")).unwrap();
    std::fs::write(quiet.join("music/a.mml"), "#tempo 124.4\nA: c1\n").unwrap();
    std::fs::write(quiet.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/a.mml\" }\nend\n").unwrap();
    let same = folder("music-tempo-same", "function on_tick()\n  if world.tick == 1 then music:play('a', 'now') end\nend\n");
    std::fs::create_dir_all(same.join("music")).unwrap();
    std::fs::copy(quiet.join("music/a.mml"), same.join("music/a.mml")).unwrap();
    std::fs::copy(quiet.join("conf.lua"), same.join("conf.lua")).unwrap();
    let (mut q, mut s) = (game_in(&quiet), game_in(&same));
    for tick in 1..=60 {
        q.step(&Input::default());
        s.step(&Input::default());
        assert_eq!(q.game.state_hash() == s.game.state_hash(), tick < 50, "tick {tick}");
    }
    // A song gone from the list leaves the tempo and the strips held.
    said(&mut q, "music:mix('A', { volume = 0.5 })");
    q.game.set_songs(&[]);
    assert_eq!(said(&mut q, "music.tempo"), "124.4");
    assert_eq!(q.game.music_mix().strip('A').volume, 0.5);

    for (line, says) in [
        ("music.tempo = 0", "music.tempo: beats a minute from 1 to 999, or nil for each song's own #tempo, not 0"),
        ("music.tempo = 1000", "music.tempo: beats a minute from 1 to 999"),
        ("music.tempo = 'fast'", "music.tempo: beats a minute from 1 to 999"),
    ] {
        let dir = folder("music-tempo-wrong", &format!("function on_tick()\n  {line}\nend\n"));
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{line}: {}", halted(&g));
    }
}

//! Channels a game lays over a studio (`music:play`'s `channels`): the
//! asks, their mistakes, the hash and the fingerprint, and the audio
//! following what the game says plays whatever moved it.

use super::*;

/// A studio at 120 beats a minute: a beat every 30 ticks, a bar every 120.
const STUDIO: &str = "#tempo 120\n@k square attack=0.002 decay=0.05 sustain=0.5 release=0.05\n@v singer\n";
/// Three patterns of one bar, an octave apart.
const PATTERNS: &str = "K = { '@k o4 c4 c4 c4 c4', '@k o5 c4 c4 c4 c4', '@k o3 c4 c4 c4 c4' }\n";

/// A game with a song and a studio (`music/drums.mml`), which lays
/// pattern 1 as it starts and then the pattern `ASKS[tick]` names, from
/// when it says (`{ from, pattern }`), `music.position` kept.
fn drums(name: &str, asks: &str) -> crate::scratch::Scratch {
    let main = format!(
        "{PATTERNS}ASKS = {{ {asks} }}\n\
         function on_level_start(how)\n  level:fill(0, 0, 0, 15, 15, 0, '#444444')\n  music:play('drums', {{ from = 'now', channels = {{ K = K[1] }} }})\nend\n\
         function on_tick()\n  local ask = ASKS[world.tick]\n  if ask then music:play('drums', {{ from = ask[1], channels = {{ K = K[ask[2]] }} }}) end\n  world.state.position = music.position\n  world.state.whole = math.floor(music.position) == music.beat\nend\n"
    );
    let dir = folder(name, &main);
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music/drums.mml"), STUDIO).unwrap();
    std::fs::write(dir.join("music/song.mml"), "#tempo 90\nA: c1 e1\n").unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 16, 16, 8 }\n  c.audio.music = { \"music/drums.mml\", \"music/song.mml\" }\nend\n").unwrap();
    dir
}

/// Pattern `n`'s digest, as the game lays it.
fn pattern(n: usize) -> [u8; 32] {
    let texts = ["@k o4 c4 c4 c4 c4", "@k o5 c4 c4 c4 c4", "@k o3 c4 c4 c4 c4"];
    crate::audio::music::Laid::new(vec![('K', texts[n - 1].to_string())]).digest
}

/// What plays and comes, by pattern number (0: none): the game's, and the
/// mixer's.
type Heard = ((usize, usize), (usize, usize));

fn number(d: Option<[u8; 32]>) -> usize {
    d.map_or(0, |d| (1..=3).find(|&n| pattern(n) == d).unwrap_or(9))
}

/// A tick played and its music fed and mixed, as a window does: what the
/// game and the mixer then play and have to come.
fn tick(g: &mut Session<LuaGame>, mixer: &mut crate::audio::Mixer) -> Heard {
    g.step(&Input::default());
    g.feed_music(mixer, false);
    let mut buf = vec![0.0f32; crate::audio::SAMPLE_RATE as usize / 60 * 2];
    mixer.mix(&mut buf);
    heard(g, mixer)
}

fn heard(g: &Session<LuaGame>, mixer: &crate::audio::Mixer) -> Heard {
    let cue = g.game.music().unwrap();
    let game = (number(cue.laid.map(|l| l.digest)), number(cue.next.map(|(l, _)| l.digest)));
    let (playing, pending) = mixer.music_laid().unwrap_or((None, None));
    (game, (number(playing), number(pending)))
}

/// A pattern asked from "bar" is handed to the audio as it is asked, with
/// its line, and swapped there; the game makes it current in the tick its
/// bar begins. Asked again before the line, the last ask wins; asked for
/// what plays, the change is called off, in the audio too. The same ask
/// again changes nothing. `music.position` counts beats with the fraction.
#[test]
fn laid_channels_change_at_the_bar_and_the_audio_follows() {
    let dir = drums("laid-bar", "[40] = { 'bar', 2 }, [41] = { 'bar', 2 }, [45] = { 'bar', 1 }, [50] = { 'bar', 2 }, [51] = { 'bar', 3 }");
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let mut log = Vec::new();
    for _ in 0..130 {
        log.push(tick(&mut g, &mut mixer));
    }
    assert_eq!(halted(&g), "");
    assert_eq!(log[0], ((1, 0), (1, 0)), "laid at once");
    assert_eq!(log[39], ((1, 2), (1, 2)), "asked, and handed to the audio ahead");
    assert_eq!(log[44], ((1, 0), (1, 0)), "called off by asking what plays");
    assert_eq!(log[49], ((1, 2), (1, 2)));
    assert_eq!(log[50], ((1, 3), (1, 3)), "the last ask wins");
    // The audio, a tick ahead of the clock, swaps on the line itself; the
    // game makes it current in the tick the bar begins (tick 121).
    let line = log.iter().position(|h| h.1.0 == 3).unwrap();
    assert_eq!(log[line], ((1, 3), (3, 0)), "the audio on the line before the game's tick");
    assert_eq!(log[line + 1], ((3, 0), (3, 0)));
    assert_eq!(line + 2, 121, "the bar begins at tick 121");
    assert!(log.iter().all(|h| h.0.0 != 2 && h.1.0 != 2), "pattern 2 never played");
    assert!(state(&mut g, "position").starts_with("Float") && state(&mut g, "whole") == "Bool(true)");
}

/// Whatever moved what the game says plays, the audio follows at the next
/// feed: a pattern called off after the audio, ahead of the clock, already
/// swapped to it is swapped back at once; a state brought back from before
/// a swap plays its own pattern again, and hashes as it did.
#[test]
fn the_audio_follows_what_the_game_says_plays() {
    let dir = drums("laid-follow", "[110] = { 'bar', 2 }, [113] = { 'bar', 1 }, [200] = { 'bar', 3 }");
    let mut g = game_in(&dir);
    let mut mixer = crate::audio::Mixer::default();
    let mut snapshot = None;
    for t in 1..=150 {
        let h = tick(&mut g, &mut mixer);
        if t == 111 {
            // The audio far ahead of the clock, past the line: it swaps.
            let mut ahead = vec![0.0f32; 2 * crate::audio::SAMPLE_RATE as usize / 2];
            mixer.mix(&mut ahead);
            assert_eq!(heard(&g, &mixer), ((1, 2), (2, 0)), "swapped ahead of the game");
        }
        if t == 113 {
            assert_eq!(h, ((1, 0), (1, 0)), "called off: swapped back at once");
        }
        if t == 190 - 60 {
            snapshot = Some((g.snapshot().unwrap(), g.game.state_hash()));
        }
    }
    for _ in 150..250 {
        tick(&mut g, &mut mixer);
    }
    assert_eq!(heard(&g, &mixer), ((3, 0), (3, 0)));
    let (snapshot, hash) = snapshot.unwrap();
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), hash, "brought back, it hashes as it did");
    g.feed_music(&mut mixer, false);
    mixer.mix(&mut [0.0; 4]);
    assert_eq!(heard(&g, &mixer), ((1, 0), (1, 0)), "the audio plays what the state brought back says");
}

/// Mistakes in an ask halt the game at the script's line, saying what is
/// wrong: a channel's text naming the channel, a part of a bar, channels
/// of other bars, a tempo of its own, a voice; a key or a text that is
/// none; channels over a song or none over a studio; `at` on a change.
#[test]
fn a_mistake_in_laid_channels_halts_at_the_line() {
    for (line, says) in [
        ("music:play('drums', { channels = { K = '@kk c1' } })", "main.lua:2: music:play: channel K, near '@kk c1': no instrument @kk"),
        ("music:play('drums', { channels = { K = 'c4 c4 c4' } })", "music:play: channel K lasts 3 beats: a laid channel fills whole bars of 4 beats"),
        ("music:play('drums', { channels = { K = 'c1', H = 'c1 c1' } })", "music:play: channel K fills 1 bar, channel H 2 bars"),
        ("music:play('drums', { channels = { K = 't90 c1' } })", "music:play: channel K, near 't90 c': t sets a channel's own tempo"),
        ("music:play('drums', { channels = { V = '@v \"l a\" c1' } })", "@v sings (singer), which a channel a game lays does not yet"),
        ("music:play('drums', { channels = { KK = 'c1' } })", "music:play: a channel is a letter, A to Z or a to z, not \"KK\""),
        ("music:play('drums', { channels = { 'c1' } })", "music:play: a channel is a letter, A to Z or a to z, not 1"),
        ("music:play('drums', { channels = { K = 3 } })", "music:play: channel K is its notes as a text of MML, not 3"),
        ("music:play('drums', { channels = {} })", "music:play: channels lays one channel at least"),
        ("music:play('drums', { channels = 'c1' })", "music:play: channels is a table of a letter and its notes"),
        ("music:play('song', { channels = { K = 'c1' } })", "music:play: song has channels of its own"),
        ("music:play(nil, { channels = { K = 'c1' } })", "music:play: channels are laid over a file of instruments"),
        ("music:play('drums')", "music:play: drums is a file of instruments: lay channels over it"),
        ("if world.tick == 1 then music:play('drums', { from = 'now', channels = { K = 'c1' } }) else music:play('drums', { at = 2, channels = { K = 'c2 c2' } }) end", "music:play: at is for a song that begins; drums plays"),
        ("if world.tick == 1 then music:play('drums', { from = 'now', channels = { K = 'c1' } }) else music:play('drums', { crossfade = 1, channels = { K = 'c2 c2' } }) end", "music:play: crossfade is for a song that begins"),
        ("music:play('drums', { at = 3, channels = { K = 'c1 c1' } })", "music:play: at = 3 is past the end of \"drums\", which has 2 bars"),
    ] {
        let dir = drums("laid-mistakes", "");
        std::fs::write(dir.join("main.lua"), format!("function on_tick()\n  {line}\nend\n")).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 3);
        assert!(halted(&g).contains(says), "{line}: {}", halted(&g));
    }
    // Laid over a studio, without channels the laid ones play on.
    let dir = drums("laid-stay", "[5] = { 'bar', 2 }");
    std::fs::write(dir.join("main.lua"), format!("{PATTERNS}function on_tick()\n  if world.tick == 1 then music:play('drums', {{ from = 'now', channels = {{ K = K[2] }} }}) else music:play('drums') end\nend\n")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 5);
    assert_eq!(halted(&g), "");
    assert_eq!(number(g.game.music().unwrap().laid.map(|l| l.digest)), 2);
}

/// A studio is no song a player picks: not among the player's songs, not
/// the title's; naming it the title's music is a mistake, and so is a
/// studio that does not read (its tempo is the game's clock). Its whole
/// text counts for replays (an instrument's word changed, another
/// fingerprint, the file named). Saved while laid channels play, its
/// instruments are read anew and the channels with them; channels that no
/// longer read halt the game as `music:play` would.
#[test]
fn a_studio_is_no_song_a_player_picks_and_counts_whole_for_replays() {
    let dir = drums("laid-studio", "");
    let mut g = game_in(&dir);
    assert_eq!(g.player_songs(), vec!["music/song.mml"]);
    assert_eq!(g.song_titles, vec!["song".to_string()]);
    assert_eq!(g.song_file(), Some("music/song.mml"));
    let mut mixer = crate::audio::Mixer::default();
    for _ in 0..10 {
        tick(&mut g, &mut mixer);
    }
    let before = g.fingerprint().unwrap();
    assert!(g.fingerprint_files().unwrap().iter().any(|(n, _)| n == "music/drums.mml"));
    // A word of an instrument: the channels read, play on, a new song made.
    std::fs::write(dir.join("music/drums.mml"), STUDIO.replace("release=0.05", "release=0.1")).unwrap();
    g.reload(&dir.join("music/drums.mml")).unwrap();
    assert_ne!(g.fingerprint().unwrap(), before, "another studio, another fingerprint");
    assert_eq!(tick(&mut g, &mut mixer), ((1, 0), (1, 0)));
    assert_eq!(halted(&g), "");
    // Its instrument renamed: the laid channel no longer reads.
    std::fs::write(dir.join("music/drums.mml"), STUDIO.replace("@k ", "@kick ")).unwrap();
    g.reload(&dir.join("music/drums.mml")).unwrap();
    assert!(halted(&g).starts_with("music:play: the channels laid over drums no longer read against it: channel K, near '@k o4"), "{}", halted(&g));

    let wrong = |studio: &str, title: &str| {
        let dir = drums("laid-studio-wrong", "");
        std::fs::write(dir.join("music/drums.mml"), studio).unwrap();
        std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.size = {{ 16, 16, 8 }}\n  c.audio.music = {{ \"music/drums.mml\", \"music/song.mml\" }}\n{title}\nend\n")).unwrap();
        start_error(&dir)
    };
    assert!(wrong(STUDIO, "  c.title_screen.music = \"music/drums.mml\"").contains("c.title_screen.music = \"music/drums.mml\" is a file of instruments"));
    assert!(wrong("#tempo 120\n@k sqare\n", "").contains("music/drums.mml (a file of instruments, no channel): line 2: @k"));
    assert!(wrong("#tempo 120\n#loop 2\n@k square\n", "").contains("line 2: #loop: a file of instruments"));
    assert_eq!(wrong(STUDIO, ""), "");
}

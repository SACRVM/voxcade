//! The window's sound without a window: the front end and the mixer as
//! the window has them, its music handed by the same two functions
//! (`made_music`, `hand_made`), the device's pull in its place (`shot
//! --sound`, and the tests of what the window hands the mixer).

use super::*;
use crate::audio::{Mixer, SAMPLE_RATE};

/// Where a window opens (`run`'s paths): on the engine's title, in a
/// title level of the game's own, or straight into a run (a scenario, a
/// replay watched, `shot` without `--screen title`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    EngineTitle,
    OwnTitle,
    Run,
}

/// The window's front end and mixer, a frame at a time, and the sound it
/// made.
pub struct Heard {
    front: Front,
    pub settings: Settings,
    pub mixer: Mixer,
    /// The sound pulled so far, interleaved stereo.
    pub samples: Vec<f32>,
}

impl Heard {
    /// As the window opens on `game` (`run`): its front end, nothing
    /// sounding yet. `OwnTitle` begins the game's title level as the window
    /// does (`own_title`) unless `begun` (the caller began it).
    pub fn open<G: Game>(game: &mut Session<G>, opening: Opening, begun: bool) -> Result<Self> {
        let mut front = Front::new(title_menu(game));
        match opening {
            Opening::EngineTitle => {}
            Opening::OwnTitle if begun => front.straight_in(),
            Opening::OwnTitle => {
                own_title(game, &mut front)?;
            }
            Opening::Run => front.straight_in(),
        }
        let mut settings = Settings::default();
        settings.adopt(game.actions(), &game.default_keys(), &game.default_pad());
        Ok(Self { front, settings, mixer: Mixer::default(), samples: Vec::new() })
    }

    /// A frame's music, as `App::music` hands it (`held`: a menu over the
    /// game); whether a song of the game's began.
    pub fn music<G: Game>(&mut self, game: &mut Session<G>, held: bool) -> bool {
        let made = made_music(game, &mut self.front);
        let held = held || self.front.holds();
        hand_made(made, game, &mut self.mixer, held, &self.settings)
    }

    /// The window loses the focus, or has it again (`Front::look_away`,
    /// `look_back`), a network game's or not.
    pub fn focus(&mut self, has: bool, networked: bool) {
        if has { self.front.look_back() } else { self.front.look_away(networked) }
    }

    /// Whether the window's game holds now (a menu, the window elsewhere).
    pub fn holds(&self) -> bool {
        self.front.holds()
    }

    /// The sounds of the tick just played, as the window plays them.
    pub fn sounds<G: Game>(&mut self, game: &Session<G>) {
        game.play_sounds(&mut self.mixer, self.settings.sound_gain());
    }

    /// `seconds` of sound pulled from the mixer, as the device would.
    pub fn pull(&mut self, seconds: f64) {
        let frames = (seconds * f64::from(SAMPLE_RATE)).round() as usize;
        let at = self.samples.len();
        self.samples.resize(at + frames * 2, 0.0);
        self.mixer.mix(&mut self.samples[at..]);
    }

    /// Back on the title as a run's end or QUIT TO TITLE has it: the
    /// engine's (`front.back_to_title`) or, for a game with a title level
    /// of its own, that level begun anew.
    pub fn to_title<G: Game>(&mut self, game: &mut Session<G>) -> Result<()> {
        if !own_title(game, &mut self.front)? {
            self.front.back_to_title(title_menu(game));
        }
        Ok(())
    }

    /// From the engine's title into a run (START): the glide, its end.
    pub fn start(&mut self) {
        self.front.glide_in();
        self.front.glide(crate::session::INTRO_TIME);
    }

    /// The menu over the game or the title (SETTINGS, the pause menu).
    pub fn menu(&mut self, menu: Menu) {
        self.front.menu = menu;
    }

    /// The sound so far written as a WAV file.
    pub fn write(&self, path: &Path) -> Result<()> {
        crate::headless::write_wav(path, &self.samples, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Input;

    /// The songs: `one` at 220 Hz, `two` at 330, `three` at 440, a held
    /// sine each, eight bars of four beats at 120 (16 s), no `#loop`.
    const SONGS: [(&str, f64); 3] = [("one", 220.0), ("two", 330.0), ("three", 440.0)];

    /// A game with a title level of its own (`title`, then `zone_1`,
    /// `zone_2`), the three songs, `conf` lines and `main`.
    fn game(name: &str, main: &str, conf: &str) -> (crate::scratch::Scratch, Session<crate::Voxcade>) {
        let dir = crate::lua::tests::title::title_folder(name, main);
        std::fs::create_dir_all(dir.join("music")).unwrap();
        for (song, hz) in SONGS {
            let text = format!("#tempo 120\n@s sine freq={hz} attack=0.002 decay=0 sustain=1 release=1 volume=0.4\nA: @s c1 c1 c1 c1 c1 c1 c1 c1\n");
            std::fs::write(dir.join(format!("music/{song}.mml")), text).unwrap();
        }
        let text = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        let lines = format!("  c.audio.music = {{ \"music/one.mml\", \"music/two.mml\", \"music/three.mml\" }}\n{conf}");
        std::fs::write(dir.join("conf.lua"), text.replace("\nend", &format!("\n{lines}end"))).unwrap();
        let game = Session::<crate::Voxcade>::load(&dir).unwrap();
        (dir, game)
    }

    /// How strongly `hz` sounds in the left side of the last `seconds` of
    /// `s` (a sine of amplitude a gives a).
    fn tone(s: &[f32], seconds: f64, hz: f64) -> f64 {
        let n = (seconds * f64::from(SAMPLE_RATE)) as usize;
        let x: Vec<f32> = s[s.len() - n * 2..].iter().step_by(2).copied().collect();
        let w = std::f64::consts::TAU * hz / f64::from(SAMPLE_RATE);
        let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (k, &v)| (re + f64::from(v) * (w * k as f64).cos(), im - f64::from(v) * (w * k as f64).sin()));
        2.0 * (re * re + im * im).sqrt() / x.len() as f64
    }

    /// `frames` frames of a window at 60 a second: its music, a tick, the
    /// tick's sounds, a tick's time of sound pulled; the songs sounding at
    /// most at once after each.
    fn frames(heard: &mut Heard, game: &mut Session<crate::Voxcade>, frames: usize, held: bool) -> usize {
        let mut most = 0;
        for _ in 0..frames {
            heard.music(game, held);
            if !held && !heard.holds() && heard.front.playing {
                // Level changes fade, as in a window.
                game.fades = true;
                game.step(&Input::default());
                heard.sounds(game);
            }
            heard.pull(f64::from(crate::sim::DT));
            most = most.max(heard.mixer.songs_sounding());
        }
        most
    }

    /// The song heard now, by its tone, over the last 0.1 s: the one of
    /// `SONGS` at least half as loud as the loudest there.
    fn heard_now(heard: &Heard) -> Vec<&'static str> {
        let levels: Vec<(&str, f64)> = SONGS.iter().map(|&(name, hz)| (name, tone(&heard.samples, 0.1, hz))).collect();
        let loudest = levels.iter().fold(0.0f64, |m, &(_, l)| m.max(l));
        levels.into_iter().filter(|&(_, l)| loudest > 0.01 && l > loudest * 0.5).map(|(n, _)| n).collect()
    }

    /// The title's level asks `SONG` (`main` the rest), the run's first
    /// level `RUN` (nothing where empty): `on_level_start` as a game writes it.
    fn asking(title: &str, run: &str, more: &str) -> String {
        let run = if run.is_empty() { String::new() } else { format!("music:play(\"{run}\")") };
        format!("function on_level_start() if level.name == \"title\" then music:play(\"{title}\") else {run} end end\n{more}")
    }

    /// Goes to a new game from the title, as the title's code would.
    const GO: &str = "function on_tick() if world.state.go then world:new_game() end end\n";

    /// A title level of the game's own gets its song once, from its code,
    /// from the window opening to a steady title: nothing before the level
    /// begins, nothing crossfaded (a crossfade set for every change), one
    /// song sounding.
    #[test]
    fn a_title_of_the_games_own_hands_one_song() {
        let (_dir, mut g) = game("heard-own-title", &asking("three", "", ""), "  c.audio.crossfade = 2\n");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        assert_eq!(heard.mixer.tally.songs, 0, "nothing before the first frame");
        let most = frames(&mut heard, &mut g, 240, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.crossfades, t.jumps, most), (1, 0, 0, 1), "{t:?}");
        assert_eq!(heard_now(&heard), ["three"]);
    }

    /// The loudness of the left side in windows of 20 ms.
    fn levels(s: &[f32]) -> Vec<f32> {
        s.iter().step_by(2).copied().collect::<Vec<_>>().chunks(882).map(|w| (w.iter().map(|v| v * v).sum::<f32>() / w.len() as f32).sqrt()).collect()
    }

    /// How far, at most, the loudness of `heard` strays from `alone`'s in
    /// windows of 20 ms (a part of `alone`'s), from `from` seconds on.
    fn strays(heard: &[f32], alone: &[f32], from: f64) -> f32 {
        let (a, b) = (levels(heard), levels(alone));
        let first = (from / 0.02) as usize;
        a.iter().zip(&b).skip(first).fold(0.0f32, |m, (x, y)| m.max((x - y).abs() / y.max(1e-4)))
    }

    /// A change of scene faded in a window (`c.transition`, an ask's fade):
    /// the same song asked on both sides plays on unbroken through it, at
    /// its level, as if no scene had changed (a fade of the picture is no
    /// fade of the music): from a title of the game's own into a run begun
    /// with `world:new_game { fade = 1.2 }`, and from level to level with
    /// `world:go_to(..., { fade = 1 })`. A level asking nothing lets it play
    /// on too. It held the song silent for the fade's second half before:
    /// the new run's song counted as a change without a crossfade.
    #[test]
    fn the_same_song_plays_on_through_a_faded_change_of_scene() {
        let go = "function on_tick() if world.state.go and level.name == \"title\" then world:new_game { fade = 1.2 } end end\n";
        let (_dir, mut g) = game("heard-fade-new-game", &asking("one", "one", go), "");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        frames(&mut heard, &mut g, 60, false);
        g.eval("world.state.go = true").unwrap();
        frames(&mut heard, &mut g, 240, false);
        assert!(!g.on_title(), "the run");
        let (_dir, mut still) = game("heard-fade-alone", &asking("one", "one", ""), "");
        let mut alone = Heard::open(&mut still, Opening::OwnTitle, false).unwrap();
        frames(&mut alone, &mut still, 300, false);
        let t = heard.mixer.tally;
        let off = strays(&heard.samples, &alone.samples, 0.5);
        eprintln!("title to a faded new game: {t:?}, the loudness off the song alone by {:.1} % at most", off * 100.0);
        assert_eq!((t.songs, t.crossfades, t.jumps, t.silences), (1, 0, 0, 0), "{t:?}");
        assert!(off < 0.02, "the song's level strayed by {:.1} % from the song alone", off * 100.0);
        // From level to level, faded, the same song; and a level asking none.
        for (name, zone_2) in [("heard-fade-go-to", "one"), ("heard-fade-go-to-none", "")] {
            let main = format!("function on_level_start() if level.name ~= \"zone_2\" then music:play(\"one\") else {} end end\nfunction on_tick() if world.tick == 60 and level.name ~= \"zone_2\" then world:go_to(\"zone_2\", {{ fade = 1 }}) end end\n", if zone_2.is_empty() { String::new() } else { format!("music:play(\"{zone_2}\")") });
            let (_dir, mut g) = game(name, &main, "  c.title_screen.level = nil\n");
            let mut heard = Heard::open(&mut g, Opening::Run, false).unwrap();
            frames(&mut heard, &mut g, 300, false);
            assert_eq!(g.game.inspect()["at"]["name"], "zone_2", "{name}: gone to the next level");
            let (_dir, mut still) = game(&format!("{name}-alone"), "function on_level_start() music:play(\"one\") end\n", "  c.title_screen.level = nil\n");
            let mut alone = Heard::open(&mut still, Opening::Run, false).unwrap();
            frames(&mut alone, &mut still, 300, false);
            let t = heard.mixer.tally;
            let off = strays(&heard.samples, &alone.samples, 0.1);
            assert_eq!((t.songs, t.jumps, t.silences), (1, 0, 0), "{name}: {t:?}");
            assert!(off < 0.02, "{name}: the song's level strayed by {:.1} %", off * 100.0);
        }
    }

    /// The window losing the focus holds a game of one, no pause menu
    /// opened: no ticks, its song faded out over 5 ms and held; the focus
    /// back, it goes on at once, the song from the sample it held at (one
    /// song, no jump), the clock where it held. A menu open stays open.
    #[test]
    fn looking_away_holds_the_game_and_looking_back_goes_on() {
        let (_dir, mut g) = game("heard-focus", &asking("one", "", ""), "");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        frames(&mut heard, &mut g, 60, false);
        let tick = g.game.sim().tick;
        heard.focus(false, false);
        assert!(heard.holds() && !heard.front.menu.is_open(), "held, no menu");
        let at = heard.samples.len();
        frames(&mut heard, &mut g, 60, false);
        assert_eq!(g.game.sim().tick, tick, "no ticks while away");
        assert!(heard.samples[at + 2 * 735..].iter().all(|v| *v == 0.0), "the song held");
        heard.focus(true, false);
        frames(&mut heard, &mut g, 60, false);
        assert!(g.game.sim().tick > tick && !heard.holds(), "on again by itself");
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.jumps, heard_now(&heard)), (1, 0, vec!["one"]), "{t:?}");
        // A menu open before stays open; a network game goes on.
        heard.menu(Menu::pause());
        heard.focus(false, false);
        heard.focus(true, false);
        assert!(heard.front.menu.is_open() && heard.holds());
        heard.menu(Menu::default());
        heard.focus(false, true);
        assert!(!heard.holds(), "a network game goes on");
    }

    /// A different song on the far side of a faded change of scene changes
    /// as asked: crossfaded where the ask says, never a silence between.
    #[test]
    fn another_song_through_a_faded_change_crossfades_as_asked() {
        let main = "function on_level_start() if level.name == \"zone_2\" then music:play(\"two\", { from = \"now\", crossfade = 1 }) else music:play(\"one\") end end\nfunction on_tick() if world.tick == 60 and level.name ~= \"zone_2\" then world:go_to(\"zone_2\", { fade = 1 }) end end\n";
        let (_dir, mut g) = game("heard-fade-other", main, "  c.title_screen.level = nil\n");
        let mut heard = Heard::open(&mut g, Opening::Run, false).unwrap();
        frames(&mut heard, &mut g, 300, false);
        assert_eq!(g.game.inspect()["at"]["name"], "zone_2");
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.crossfades, t.silences), (2, 1, 0), "{t:?}");
        assert_eq!(heard_now(&heard), ["two"]);
        let quiet = levels(&heard.samples).iter().skip(10).filter(|l| **l < 0.01).count();
        assert_eq!(quiet, 0, "{quiet} windows of 20 ms silent through the change");
    }

    /// The engine's title without `c.title_screen.music` is silent, its
    /// attract mode too (the owner, 2026-11-24: no song is a default, not
    /// the first of `c.audio.music`): no song begun from the window opening
    /// on, into a run whose code asks one and back; named, that song.
    #[test]
    fn the_engine_title_plays_only_the_song_it_names() {
        let (_dir, mut g) = game("heard-silent-title", "function on_tick() if world.tick == 30 then music:play(\"two\") end end\n", "  c.title_screen.level = nil\n");
        // Songs begun: every hand-over that was no silence.
        let begun = |h: &Heard| h.mixer.tally.songs - h.mixer.tally.silences;
        let mut heard = Heard::open(&mut g, Opening::EngineTitle, false).unwrap();
        frames(&mut heard, &mut g, 120, false);
        assert_eq!((begun(&heard), heard_now(&heard)), (0, vec![]), "a silent title");
        assert!(heard.samples.iter().all(|v| *v == 0.0));
        heard.front.attracting = true;
        frames(&mut heard, &mut g, 60, false);
        assert_eq!(begun(&heard), 0, "its attract mode under no music");
        heard.front.attracting = false;
        heard.start();
        frames(&mut heard, &mut g, 60, false);
        assert_eq!((begun(&heard), heard_now(&heard)), (1, vec!["two"]), "the run's code asks its song");
        heard.to_title(&mut g).unwrap();
        frames(&mut heard, &mut g, 30, false);
        assert_eq!((begun(&heard), heard.mixer.songs_sounding()), (1, 0), "back on a silent title: the run's song goes, none begun");
        let (_dir, mut g) = game("heard-named-title", "", "  c.title_screen.level = nil\n  c.title_screen.music = \"three\"\n");
        let mut heard = Heard::open(&mut g, Opening::EngineTitle, false).unwrap();
        frames(&mut heard, &mut g, 60, false);
        assert_eq!((begun(&heard), heard_now(&heard)), (1, vec!["three"]), "the song it names");
    }

    /// The engine's title, a run and the way back: the title's song from
    /// the first frame; a run whose code asks nothing lets it play on
    /// (music comes from the code alone, and a scene that asks nothing
    /// changes nothing); a run that asks a song gets it, once; a pause over
    /// the run leaves a song the game asked held, nothing else; the run's
    /// end brings the title's song again.
    #[test]
    fn the_engine_title_and_a_run_hand_the_mixer_once_a_change() {
        let (_dir, mut g) = game("heard-engine-title", "function on_tick() if world.tick == 60 then music:play(\"two\", \"now\") end end\n", "  c.title_screen.level = nil\n  c.title_screen.music = \"one\"\n");
        let mut heard = Heard::open(&mut g, Opening::EngineTitle, false).unwrap();
        assert_eq!(frames(&mut heard, &mut g, 60, false), 1);
        assert_eq!((heard.mixer.tally.songs, heard_now(&heard)), (1, vec!["one"]), "the title's song from the first frame");
        heard.start();
        let most = frames(&mut heard, &mut g, 30, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, most, heard_now(&heard)), (1, 1, vec!["one"]), "into the run, nothing asked yet: it plays on: {t:?}");
        frames(&mut heard, &mut g, 60, false);
        assert_eq!((heard.mixer.tally.songs, heard_now(&heard)), (2, vec!["two"]), "the run's code asks a song");
        heard.menu(Menu::pause());
        let before = heard.samples.len();
        frames(&mut heard, &mut g, 30, true);
        let held = &heard.samples[before + 2 * 441..];
        assert!(held.iter().all(|v| *v == 0.0), "paused by the engine's menu: the game's song holds");
        heard.menu(Menu::default());
        frames(&mut heard, &mut g, 30, false);
        assert_eq!(heard_now(&heard), ["two"]);
        heard.to_title(&mut g).unwrap();
        let most = frames(&mut heard, &mut g, 30, false);
        assert_eq!((heard.mixer.tally.songs, most, heard_now(&heard)), (3, 1, vec!["one"]));
        assert_eq!(heard.mixer.tally.crossfades, 0, "none asked");
    }

    /// The song of a title level of the game's own asked again by the
    /// run's first level plays on into the run, one song: no restart, no
    /// jump, nothing crossfaded, the run's clock at its beat 0 taken as
    /// where the song is (`Player::rebase`); the same back to the title. A
    /// run asking another song gets it once. F5 begins the title's anew.
    #[test]
    fn the_same_song_plays_on_from_the_title_into_the_run_and_back() {
        let (_dir, mut g) = game("heard-own-run", &asking("one", "one", GO), "  c.audio.crossfade = 2\n");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        frames(&mut heard, &mut g, 90, false);
        g.eval("world.state.go = true").unwrap();
        let at = heard.samples.len();
        let most = frames(&mut heard, &mut g, 120, false);
        let t = heard.mixer.tally;
        assert!(!g.on_title() && g.steers_music(), "a run that plays the song as its own");
        assert_eq!(g.game.music().map(|c| c.beats < 4.5), Some(true), "the run's clock from its beat 0 (4 beats in 2 s)");
        assert_eq!((t.songs, t.crossfades, t.jumps, t.played_on, most, heard_now(&heard)), (1, 0, 0, 1, 1, vec!["one"]), "one song, played on: {t:?}");
        // Unbroken: the samples go on as one sine (no step past its own).
        let s: Vec<f32> = heard.samples[at - 2000..at + 20_000].iter().step_by(2).copied().collect();
        let steepest = s.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        let own = 2.0 * std::f32::consts::PI * 220.0 / SAMPLE_RATE as f32 * s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(steepest <= own * 1.05, "a step of {steepest} where the sine's own is {own}");
        heard.to_title(&mut g).unwrap();
        let most = frames(&mut heard, &mut g, 120, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.jumps, t.played_on, most), (1, 0, 2, 1), "back on the title, still the one song: {t:?}");
        g.restart_music();
        frames(&mut heard, &mut g, 30, false);
        assert_eq!((heard.mixer.tally.songs, heard.mixer.tally.crossfades), (2, 0), "F5 begins it anew, never crossfaded into itself");
        // A run asking another song: that one, crossfaded as asked.
        let (_dir, mut g) = game("heard-own-run-other", &asking("one", "two", GO), "  c.audio.crossfade = 2\n");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        frames(&mut heard, &mut g, 90, false);
        g.eval("world.state.go = true").unwrap();
        let most = frames(&mut heard, &mut g, 240, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.crossfades, most, heard_now(&heard)), (2, 1, 2, vec!["two"]), "{t:?}");
        assert_eq!(heard.mixer.songs_sounding(), 1);
    }

    /// A song file saved while it plays is made anew (its text is another
    /// song now): handed at once, never crossfaded into what it was.
    #[test]
    fn a_song_saved_while_it_plays_is_handed_once() {
        let (dir, mut g) = game("heard-saved", &asking("one", "", ""), "  c.audio.crossfade = 2\n");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        frames(&mut heard, &mut g, 30, false);
        let path = dir.join("music/one.mml");
        std::fs::write(&path, std::fs::read_to_string(&path).unwrap().replace("freq=220", "freq=330")).unwrap();
        g.reload(&path).unwrap();
        let most = frames(&mut heard, &mut g, 30, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.crossfades, most, heard_now(&heard)), (2, 0, 1, vec!["two"]), "{t:?}");
        assert_eq!(heard.mixer.songs_sounding(), 1);
    }

    /// The owner's car radio: a key steps through the game's songs with
    /// `music:play`, at once or crossfaded; one song sounds but while the
    /// fade asked runs, and the same song asked again plays on.
    #[test]
    fn a_car_radio_steps_through_the_songs() {
        let main = "local SONGS = { \"one\", \"two\", \"three\" }\n\
            function on_world_start() world.state.station = 1 end\n\
            function on_level_start() music:play(SONGS[world.state.station], \"now\") end\n\
            function on_tick()\n  local s = world.state\n  if s.next then\n    s.next = nil\n    s.station = s.station % #SONGS + 1\n    music:play(SONGS[s.station], { from = \"now\", crossfade = s.fade or 0 })\n  end\n  if s.again then s.again = nil; music:play(SONGS[s.station], \"now\") end\nend\n";
        let (_dir, mut g) = game("heard-radio", main, "  c.title_screen.level = nil\n");
        let mut heard = Heard::open(&mut g, Opening::Run, false).unwrap();
        frames(&mut heard, &mut g, 60, false);
        assert_eq!(heard_now(&heard), ["one"]);
        let mut most = 0;
        for (want, fade) in [("two", 0.0), ("three", 0.0), ("one", 1.0), ("two", 0.5)] {
            g.eval(&format!("world.state.next = true; world.state.fade = {fade}")).unwrap();
            let during = frames(&mut heard, &mut g, 90, false);
            most = most.max(during);
            assert!(during <= if fade > 0.0 { 2 } else { 1 }, "{want}: {during} songs at once");
            assert_eq!(heard_now(&heard), [want]);
        }
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.crossfades, most), (5, 2, 2), "{t:?}");
        g.eval("world.state.again = true").unwrap();
        frames(&mut heard, &mut g, 30, false);
        assert_eq!((heard.mixer.tally.songs, heard_now(&heard)), (5, vec!["two"]), "the same station again plays on");
    }

    /// `music:pause()` holds the song where it is, its clock too, and
    /// `music:resume()` goes on from the very sample it held at, faded out
    /// and in over 5 ms: the samples heard are the song's own, with the
    /// silence between.
    #[test]
    fn a_paused_song_resumes_where_it_held() {
        let main = "function on_level_start() music:play(\"one\", \"now\") end\n\
            function on_tick()\n  if world.tick == 61 then music:pause() end\n  if world.tick == 121 then music:resume() end\n  world.state.beat = music.position\n  world.state.paused = music.paused\nend\n";
        let (_dir, mut g) = game("heard-pause", main, "  c.title_screen.level = nil\n");
        let mut heard = Heard::open(&mut g, Opening::Run, false).unwrap();
        frames(&mut heard, &mut g, 90, false);
        assert!(g.game.music_paused());
        let held_at = g.game.music().map(|c| c.beats).unwrap();
        frames(&mut heard, &mut g, 20, false);
        assert_eq!(g.game.music().map(|c| c.beats), Some(held_at), "the clock holds");
        let quiet = heard.samples[heard.samples.len() - 20 * 1470..].iter().all(|v| *v == 0.0);
        assert!(quiet, "silent while paused");
        frames(&mut heard, &mut g, 60, false);
        assert!(!g.game.music_paused() && g.game.music().map(|c| c.beats).unwrap() > held_at, "on again");
        assert_eq!(heard_now(&heard), ["one"]);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.jumps), (1, 0), "{t:?}");
        // The song unpaused is the song never paused, but for the silence
        // and its two fades.
        let (_dir, mut plain) = game("heard-pause-plain", "function on_level_start() music:play(\"one\", \"now\") end\n", "  c.title_screen.level = nil\n");
        let mut whole = Heard::open(&mut plain, Opening::Run, false).unwrap();
        frames(&mut whole, &mut plain, 150, false);
        let fade = 2 * 220;
        let frame = 1470;
        let (paused, at_once) = (&heard.samples, &whole.samples);
        // Where it fell silent and where it sounded again; the song goes on
        // from the sample after the last it made (that one silent).
        let stop = (0..paused.len() / 2).find(|&i| i > 61 * frame / 2 && paused[2 * i] == 0.0 && paused[2 * i + 1] == 0.0 && paused[2 * i + 2..2 * i + 40].iter().all(|v| *v == 0.0)).unwrap();
        let go = (stop..paused.len() / 2).find(|&i| paused[2 * i] != 0.0).unwrap();
        assert_eq!(&paused[..2 * stop - fade], &at_once[..2 * stop - fade], "the same until the fade out");
        let after = &paused[2 * go + fade..2 * go + fade + 4000];
        let then = &at_once[2 * stop + 2 + fade..2 * stop + 2 + fade + 4000];
        let worst = after.iter().zip(then).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(worst < 1e-4, "resumed from the very sample it held at: {worst}");
    }

    /// A window held up (a model read, a shader made, the system) while
    /// the game's own song plays: the song plays straight through and the
    /// clock catches up with it after (`TickClock`), where it jumped back
    /// before (the clock dropped the time past five ticks, the song went
    /// back to it: heard as a hang). Frames of 16.7 ms with jitter, every
    /// 2 s one held up 120 ms, a long first picture of 400 ms; the device
    /// pulls blocks of 1024 frames 20 ms ahead.
    #[test]
    fn a_window_held_up_never_jumps_the_games_song() {
        let run = |keep: bool| {
            let (_dir, mut g) = game("heard-held-up", &asking("three", "", ""), "");
            let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
            let mut clock = TickClock::default();
            let (mut wall, mut device, mut since) = (0.0f64, 0.0f64, 0.0f32);
            for frame in 0..600u32 {
                // As `App::music`, then `App::elapsed` and `App::ticks`.
                g.music_ahead(if keep { clock.behind(since) } else { 0.0 });
                if heard.music(&mut g, false) && keep {
                    (clock, since) = (TickClock::default(), 0.0);
                }
                let tick_keep = keep && g.steers_music();
                clock.frame(since, tick_keep);
                for _ in 0..clock.due(tick_keep) {
                    g.step(&Input::default());
                }
                let lasted = match frame {
                    0 => 0.4,
                    f if f % 120 == 60 => 0.12,
                    f => 1.0 / 60.0 + f64::from(f % 7) * 0.0004 - 0.0012,
                };
                wall += lasted;
                since = lasted as f32;
                while device < wall + 0.02 {
                    heard.pull(1024.0 / f64::from(SAMPLE_RATE));
                    device += 1024.0 / f64::from(SAMPLE_RATE);
                }
            }
            heard.mixer.tally
        };
        let (before, now) = (run(false), run(true));
        eprintln!("the clock dropping time (before): {before:?}\nthe clock catching up (now): {now:?}");
        assert!(before.jumps >= 4, "the case is one that jumped: {before:?}");
        assert_eq!(now.jumps, 0, "{now:?}");
        assert!(now.largest_drift < 50.0, "{now:?}");
    }
}

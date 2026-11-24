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
        let made = made_music(game, &mut self.front, &self.settings);
        hand_made(made, game, &mut self.mixer, held, &self.settings)
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
            if !held {
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

    /// A title level of the game's own gets its song once, from the window
    /// opening to a steady title: one song handed, nothing before the
    /// level begins, nothing crossfaded (a crossfade set for every change),
    /// one song sounding; asked in its `on_level_start` too (`music:play`
    /// of the title's song, the build in between), still once.
    #[test]
    fn a_title_of_the_games_own_hands_one_song() {
        for main in ["", "function on_level_start() music:play(\"three\", \"now\") end\n"] {
            let (_dir, mut g) = game("heard-own-title", main, "  c.title_screen.music = \"three\"\n  c.audio.crossfade = 2\n");
            let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
            assert_eq!(heard.mixer.tally.songs, 0, "{main}: nothing before the first frame");
            let most = frames(&mut heard, &mut g, 240, false);
            let t = heard.mixer.tally;
            assert_eq!((t.songs, t.crossfades, t.jumps, most), (1, 0, 0, 1), "{main}: {t:?}");
            assert_eq!(heard_now(&heard), ["three"]);
        }
    }

    /// The engine's title, its settings' preview, a run, a pause and the
    /// way back: each change hands the mixer once, the same song plays on
    /// (the title's and the player's are one file), at most one song
    /// sounds but while a fade asked for runs, and never two copies of
    /// one; a song chosen in the settings plays at once in the run.
    #[test]
    fn the_engine_title_and_a_run_hand_the_mixer_once_a_change() {
        let (_dir, mut g) = game("heard-engine-title", "", "  c.title_screen.level = nil\n");
        let mut heard = Heard::open(&mut g, Opening::EngineTitle, false).unwrap();
        assert_eq!(frames(&mut heard, &mut g, 60, false), 1);
        assert_eq!((heard.mixer.tally.songs, heard_now(&heard)), (1, vec!["one"]), "the title's song from the first frame");
        // SETTINGS over the title: the song chosen there, then back.
        heard.settings.song = 1;
        heard.menu(Menu { stack: vec![(Page::Title, 1), (Page::Settings, 0)], ..Menu::title() });
        frames(&mut heard, &mut g, 30, false);
        assert_eq!((heard.mixer.tally.songs, heard_now(&heard)), (2, vec!["two"]), "the preview");
        heard.settings.song = 0;
        frames(&mut heard, &mut g, 30, false);
        heard.menu(Menu::title());
        frames(&mut heard, &mut g, 30, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.played_on, heard_now(&heard)), (3, 1, vec!["one"]), "the first chosen again, the title the same song: {t:?}");
        // START: the player's song is the title's: it plays on.
        heard.start();
        let most = frames(&mut heard, &mut g, 60, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.played_on, most), (3, 2, 1), "into the run, the same song: {t:?}");
        // Paused: the player's song goes on (only the game's own holds).
        heard.menu(Menu::pause());
        frames(&mut heard, &mut g, 30, true);
        heard.menu(Menu::default());
        // A song chosen in the settings over the run plays at once.
        heard.settings.song = 2;
        frames(&mut heard, &mut g, 30, false);
        assert_eq!((heard.mixer.tally.songs, heard_now(&heard)), (4, vec!["three"]));
        // The run's end: the engine's title, its song again.
        heard.to_title(&mut g).unwrap();
        let most = frames(&mut heard, &mut g, 30, false);
        assert_eq!((heard.mixer.tally.songs, most, heard_now(&heard)), (5, 1, vec!["one"]));
        assert_eq!(heard.mixer.tally.crossfades, 0, "none asked");
    }

    /// A game's own title, a run begun from it and back: a run that plays
    /// the player's song, the title's file, lets it play on; a run whose
    /// code asks the title's song begins it on its own clock (its beat 0),
    /// cleanly, never crossfaded into itself; QUIT TO TITLE begins the
    /// title's anew; F5 begins it again from its start.
    #[test]
    fn a_run_from_the_games_own_title_and_back() {
        let main = "function on_tick() if world.state.go then world:new_game() end end\n";
        let (_dir, mut g) = game("heard-own-run", main, "  c.title_screen.music = \"one\"\n  c.audio.crossfade = 2\n");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        frames(&mut heard, &mut g, 60, false);
        g.eval("world.state.go = true").unwrap();
        let most = frames(&mut heard, &mut g, 120, false);
        let t = heard.mixer.tally;
        assert!(!g.on_title(), "a run");
        assert_eq!((t.songs, t.crossfades, t.played_on, most, heard_now(&heard)), (1, 0, 1, 1, vec!["one"]), "the player's song is the title's: it plays on: {t:?}");
        heard.to_title(&mut g).unwrap();
        let most = frames(&mut heard, &mut g, 60, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.crossfades, most), (2, 0, 1), "the title's clock at its beat 0: begun anew, the one before ended within 5 ms: {t:?}");
        assert_eq!(heard.mixer.songs_sounding(), 1);
        // F5 on the title: begun again from its start, the same song.
        g.restart_music();
        frames(&mut heard, &mut g, 30, false);
        assert_eq!((heard.mixer.tally.songs, heard.mixer.tally.crossfades), (3, 0), "F5");
        // A run whose code asks for the title's song: on its own clock.
        let main = "function on_level_start() if level.name ~= \"title\" then music:play(\"one\", \"now\") end end\n\
            function on_tick() if world.state.go then world:new_game() end end\n";
        let (_dir, mut g) = game("heard-own-run-asks", main, "  c.title_screen.music = \"one\"\n  c.audio.crossfade = 2\n");
        let mut heard = Heard::open(&mut g, Opening::OwnTitle, false).unwrap();
        frames(&mut heard, &mut g, 60, false);
        g.eval("world.state.go = true").unwrap();
        let most = frames(&mut heard, &mut g, 120, false);
        let t = heard.mixer.tally;
        assert_eq!((t.songs, t.crossfades, most, heard_now(&heard)), (2, 0, 1, vec!["one"]), "begun anew on the run's clock, not crossfaded into itself: {t:?}");
        assert_eq!(heard.mixer.songs_sounding(), 1);
    }

    /// A song file saved while it plays is made anew (its text is another
    /// song now): handed at once, never crossfaded into what it was.
    #[test]
    fn a_song_saved_while_it_plays_is_handed_once() {
        let (dir, mut g) = game("heard-saved", "", "  c.title_screen.music = \"one\"\n  c.audio.crossfade = 2\n");
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
            let (_dir, mut g) = game("heard-held-up", "", "  c.title_screen.music = \"three\"\n");
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

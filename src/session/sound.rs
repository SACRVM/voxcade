//! The game's music and sounds as the session feeds them to the speaker.

use super::*;

/// The titles of the game's songs: each file's `#title` line, else its name.
pub(super) fn song_titles(files: &Files, cfg: &GameConfig) -> Vec<String> {
    cfg.audio
        .music
        .files()
        .iter()
        .map(|f| {
            files
                .read_to_string(f)
                .ok()
                .and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("#title").map(|v| v.trim().to_string())))
                .unwrap_or_else(|| Path::new(f).file_stem().map_or(f.to_string(), |n| n.to_string_lossy().into_owned()))
        })
        .collect()
}

/// How many steps a fade of `length` seconds takes, out or in, as a
/// window's `Session::step` counts them.
pub(super) fn fade_steps(length: f32) -> u32 {
    let (mut t, mut n) = (0.0f32, 0);
    while t < length {
        t += crate::sim::DT;
        n += 1;
    }
    n
}

/// The beats of the songs of `c.audio.music`, in order: a song that does
/// not read keeps its place, at 120 beats a minute (its mistake shows when
/// it is played).
pub(super) fn song_timings(files: &Files, cfg: &GameConfig) -> Vec<audio::music::Timing> {
    cfg.audio
        .music
        .files()
        .iter()
        .map(|f| {
            let song = files.read_to_string(f).ok().and_then(|t| audio::music::parse(&t).ok());
            match song {
                Some(song) => song.timing(f),
                None => audio::music::Song::empty().timing(f),
            }
        })
        .collect()
}

/// The title's song file: `c.title_screen.music`, else the first song.
fn title_song(cfg: &GameConfig) -> Option<&str> {
    cfg.title_screen.music.as_deref().or_else(|| cfg.audio.music.files().first().copied())
}

/// `c.title_screen.music_at` within the title's song: a bar past its
/// end or after its `#loop` is a mistake naming the setting (a song that
/// does not read says its own mistake when it is played; without a song
/// there is nothing to begin).
pub(super) fn check_title_bar(files: &Files, cfg: &GameConfig) -> Result<()> {
    let at = cfg.title_screen.music_at;
    if at <= 1 {
        return Ok(());
    }
    // Without a song for the title there is nothing to begin.
    if let Some(file) = title_song(cfg)
        && let Some(song) = files.read_to_string(file).ok().and_then(|t| audio::music::parse(&t).ok())
        && let Some(wrong) = song.timing(file).bar_mistake(at)
    {
        anyhow::bail!("conf.lua: c.title_screen.music_at = {at} {wrong} ({file})");
    }
    Ok(())
}

impl<G: Game> Session<G> {
    /// The file of the song that plays.
    pub fn song_file(&self) -> Option<&str> {
        let files = self.cfg.audio.music.files();
        files.get(self.song.min(files.len().saturating_sub(1))).copied()
    }

    /// Switches to the game's `index`th song (from 0); false if there is none.
    pub fn select_song(&mut self, index: usize) -> bool {
        if index >= self.cfg.audio.music.files().len() || index == self.song {
            return false;
        }
        self.song = index;
        self.music_changed = true;
        true
    }

    /// The songs' beats anew (a song or `c.audio.music` changed), told to
    /// the game.
    pub(super) fn retime_songs(&mut self) {
        let songs = song_timings(&self.files, &self.cfg);
        if songs != self.songs {
            self.game.set_songs(&songs);
            self.songs = songs;
        }
    }

    /// The game's music, ready to play: its own song where it is in it,
    /// else the player's; None without music or when its file has a
    /// mistake (reported).
    pub fn music(&self) -> Option<audio::music::Player> {
        let mut player = match self.game.music() {
            Some(cue) => {
                let mut player = self.song(self.cfg.audio.music.files().get(cue.song)?)?;
                player.seek(player.at_beat(cue.start + cue.beats));
                player.start_at(cue.intensity);
                player
            }
            None => self.song(self.song_file()?)?,
        };
        // As loud and as muffled as the game asked, whichever song it is.
        player.start_mix(&self.game.music_mix());
        Some(player)
    }

    /// Whether the game plays a song of its own choice (`music:play`):
    /// the music then keeps time with the game, and holds while it does.
    pub fn steers_music(&self) -> bool {
        self.game.music().is_some()
    }

    /// Hands the music to the mixer: the song anew when it changed, else
    /// the game's own song kept in time with its clock (held while the
    /// game is, `held`: a pause, a fade); either as loud and as muffled as
    /// the game asks (`Game::music_mix`). Whether a song began.
    pub fn feed_music(&mut self, mixer: &mut Mixer, held: bool) -> bool {
        let began = std::mem::take(&mut self.music_changed);
        if began {
            match self.crossfade.take() {
                Some(seconds) if seconds > 0.0 => mixer.crossfade_music(self.music(), seconds),
                _ => mixer.set_music(self.music()),
            }
        } else if let Some(cue) = self.game.music()
            && let Some(t) = self.songs.get(cue.song)
        {
            mixer.follow_music(((cue.start + cue.beats) * 60.0 / t.tempo as f64 * audio::SAMPLE_RATE as f64) as u64);
            mixer.music_intensity(cue.intensity);
        }
        if !began {
            mixer.music_mix(&self.game.music_mix());
        }
        mixer.hold_music((held && self.steers_music()) || self.fade_hold());
        began
    }

    /// The game's `index`th song (from 0), ready to play.
    pub fn song_at(&self, index: usize) -> Option<audio::music::Player> {
        self.song(self.cfg.audio.music.files().get(index)?)
    }

    /// The title's song, begun at `c.title_screen.music_at`'s bar.
    pub fn title_music(&self) -> Option<audio::music::Player> {
        let mut player = self.song(title_song(&self.cfg)?)?;
        let at = self.cfg.title_screen.music_at;
        if at > 1 {
            player.seek(player.at_bar(at));
        }
        Some(player)
    }

    pub(super) fn song(&self, file: &str) -> Option<audio::music::Player> {
        let path = self.files.path(file);
        let text = self.files.read_to_string(&path).map_err(|e| eprintln!("music: {}: {e}", path.display())).ok()?;
        match audio::music::parse(&text) {
            Ok(song) => Some(audio::music::Player::new(song, self.cfg.audio.music_volume)),
            Err(e) => {
                eprintln!("music: {}: {e:#}", path.display());
                None
            }
        }
    }

    /// Hands the last tick's sounds to the mixer, placed by where they
    /// happened in the view, `gain` the player's volume.
    pub fn play_sounds(&self, mixer: &mut Mixer, gain: f32) {
        let sim = self.game.sim();
        let listener = sim.cam_target;
        let volume = self.cfg.audio.volume.max(0.0) * gain;
        let right = self.screen_right();
        for event in &sim.sounds {
            if let Some(samples) = self.sounds.get(event.name) {
                // (A fixed height: as wide as a 16:9 window shows.)
                let (gain, pan) = audio::place(event.at, listener, camera::shown_width(&self.cfg.camera, 16.0 / 9.0), right);
                mixer.play(event.name, samples, gain * event.volume * volume, pan);
            }
        }
    }

    /// The title's sounds for tick `tick`, `time` seconds in: the flyby's
    /// rotor chopping as it crosses, placed where it is. (The sound's name
    /// is More Blood's gunship's, the one word of a game's in the engine
    /// until the title names its flyby's sound.)
    pub fn title_sounds(&self, mixer: &mut Mixer, tick: u64, time: f32, gain: f32) {
        let Some(at) = self.flyby_at(time) else { return };
        if !tick.is_multiple_of(5) {
            return;
        }
        if let Some(samples) = self.sounds.get("rotor") {
            let (g, pan) = audio::place(at, self.title_target(), self.title_camera().view_width, self.screen_right());
            mixer.play("rotor", samples, g * 0.8 * gain * self.cfg.audio.volume.max(0.0), pan);
        }
    }
}

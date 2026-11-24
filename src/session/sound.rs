//! The game's music and sounds as the session feeds them to the speaker.

use super::*;

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

/// The beats of the files of `c.audio.music`, in order, a studio's whole
/// text with them (`Song::studio`): a song that does not read keeps its
/// place, at 120 beats a minute (its mistake shows when it is played); a
/// studio that does not read is a mistake now, as the game's clock and its
/// laid channels go by it, and so is one naming a sample `samples` lacks.
pub(super) fn song_timings(files: &Files, cfg: &GameConfig, samples: &audio::music::Samples) -> Result<Vec<audio::music::Timing>> {
    cfg.audio
        .music
        .files()
        .iter()
        .map(|f| {
            let Ok(text) = files.read_to_string(f) else { return Ok(audio::music::Song::empty().timing(f)) };
            match audio::music::read(&text) {
                Ok(mut song) if song.studio() => {
                    song.bind(samples).map_err(|e| e.context(format!("{f} (a file of instruments, no channel)")))?;
                    Ok(audio::music::Timing { studio: Some(Arc::from(text.as_str())), ..song.timing(f) })
                }
                Ok(song) => Ok(song.timing(f)),
                Err(e) if !audio::music::has_channels(&text) => Err(e.context(format!("{f} (a file of instruments, no channel)"))),
                Err(_) => Ok(audio::music::Song::empty().timing(f)),
            }
        })
        .collect()
}

/// The files of `c.audio.music` a player hears: its songs, not its
/// studios (`Song::studio`), which only a game's laid channels play.
fn heard<'a>(cfg: &'a GameConfig, songs: &[audio::music::Timing]) -> Vec<&'a str> {
    cfg.audio.music.files().into_iter().zip(songs).filter(|(_, t)| t.studio.is_none()).map(|(f, _)| f).collect()
}

/// The title's song file: `c.title_screen.music`, else the first song a
/// player hears.
fn title_song<'a>(cfg: &'a GameConfig, songs: &[audio::music::Timing]) -> Option<&'a str> {
    // By its file or by its name; a name not among the songs stays as
    // written (the engine's title then finds no file and plays nothing).
    cfg.title_screen.music.as_deref().map(|named| cfg.audio.music.file_of(named).unwrap_or(named)).or_else(|| heard(cfg, songs).first().copied())
}

/// The title's song: not a studio, and `c.title_screen.music_at` within
/// it: a bar past its end or after its `#loop` is a mistake naming the
/// setting (a song that does not read says its own mistake when it is
/// played; without a song there is nothing to begin).
pub(super) fn check_title_bar(files: &Files, cfg: &GameConfig, songs: &[audio::music::Timing]) -> Result<()> {
    if let Some(file) = cfg.title_screen.music.as_deref().map(|named| cfg.audio.music.file_of(named).unwrap_or(named))
        && cfg.audio.music.files().iter().zip(songs).any(|(f, t)| *f == file && t.studio.is_some())
    {
        anyhow::bail!("conf.lua: c.title_screen.music = \"{file}\" is a file of instruments, no song to play (a game lays its channels over it, music:play)");
    }
    let at = cfg.title_screen.music_at;
    if at <= 1 {
        return Ok(());
    }
    // Without a song for the title there is nothing to begin.
    if let Some(file) = title_song(cfg, songs)
        && let Some(song) = files.read_to_string(file).ok().and_then(|t| audio::music::parse(&t).ok())
        && let Some(wrong) = song.timing(file).bar_mistake(at)
    {
        anyhow::bail!("conf.lua: c.title_screen.music_at = {at} {wrong} ({file})");
    }
    Ok(())
}

/// What song a file of `c.audio.music` with `text` is
/// (`Player::identified`): its name and its text, length-prefixed.
fn song_identity(file: &str, text: &str) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    for part in [file.as_bytes(), text.as_bytes()] {
        h.update(&(part.len() as u64).to_le_bytes());
        h.update(part);
    }
    *h.finalize().as_bytes()
}

/// What a session made to hand the mixer (`Session::new_music`): the song
/// when the music changed, with its crossfade; the laid channels its song
/// is to swap to (None inside: none to come), when they changed.
pub struct NewMusic {
    player: Option<(Option<audio::music::Player>, Option<f32>)>,
    swap: Option<Option<Box<audio::music::Swap>>>,
}

impl<G: Game> Session<G> {
    /// The files of `c.audio.music` that are songs, not studios
    /// (`Song::studio`): the engine's title plays the first unless
    /// `c.title_screen.music` names one, `voxcade music GAME` writes it.
    pub fn player_songs(&self) -> Vec<&str> {
        heard(&self.cfg, &self.songs)
    }

    /// The songs' beats anew (a song or `c.audio.music` changed), told to
    /// the game. A studio changed touches the run: its whole text is in the
    /// fingerprint.
    pub(super) fn retime_songs(&mut self) -> Result<()> {
        let songs = song_timings(&self.files, &self.cfg, &self.samples)?;
        if songs != self.songs {
            self.touched |= songs.iter().zip(&self.songs).any(|(a, b)| a.studio != b.studio);
            self.game.set_songs(&songs);
            self.songs = songs;
        }
        Ok(())
    }

    /// The game's song, ready to play: the one its code asked last (with
    /// the channels it laid over a studio), at its clock; None without one
    /// (stopped, or nothing asked yet) or when its file has a mistake
    /// (reported).
    pub fn music(&self) -> Option<audio::music::Player> {
        let cue = self.game.music()?;
        let mut player = match &cue.laid {
            Some(laid) => audio::music::Player::new(self.laid_song(cue.song, laid)?, self.cfg.audio.music_volume).identified(self.laid_identity(cue.song, laid)?),
            None => self.song(self.cfg.audio.music.files().get(cue.song)?)?,
        };
        // At the game's tempo from its first sample, not ramping to it.
        player.set_ratio(cue.ratio);
        player.seek(player.at_beat(cue.start + cue.beats));
        player.start_at(cue.intensity);
        // As loud and as muffled as the game asked.
        player.start_mix(&self.game.music_mix());
        Some(player)
    }

    /// Whether the game plays a song of its own choice (`music:play`):
    /// the music then keeps time with the game, and holds while it does.
    pub fn steers_music(&self) -> bool {
        self.game.music().is_some()
    }

    /// The channels `laid` over the studio `index` of `c.audio.music`, read
    /// (None, said, where they do not read: the game halted for it).
    fn laid_song(&self, index: usize, laid: &audio::music::Laid) -> Option<audio::music::Song> {
        let t = self.songs.get(index)?;
        let mut song = audio::music::lay(t.studio.as_deref()?, laid).map_err(|e| eprintln!("music: {}: {e:#}", t.name)).ok()?;
        song.bind(&self.samples).map_err(|e| eprintln!("music: {}: {e:#}", t.name)).ok()?;
        Some(song)
    }

    /// What song the studio `index` with the channels `laid` over it is:
    /// its file, its whole text and the channels' digest.
    fn laid_identity(&self, index: usize, laid: &audio::music::Laid) -> Option<[u8; 32]> {
        let t = self.songs.get(index)?;
        let mut h = blake3::Hasher::new();
        for part in [t.name.as_bytes(), t.studio.as_deref()?.as_bytes(), &laid.digest] {
            h.update(&(part.len() as u64).to_le_bytes());
            h.update(part);
        }
        Some(*h.finalize().as_bytes())
    }

    /// The song to hand the mixer when the music changed, and the laid
    /// channels its song is to swap to, read and made before the mixer is
    /// locked (the audio device waits on the lock while a song is parsed
    /// otherwise): for `hand_music`. The game's song is compared with the
    /// mixer's at every feed, so whatever moved it (a tick, a state
    /// brought back, a new run) the music follows.
    pub fn new_music(&mut self) -> NewMusic {
        self.notice_song();
        // Music comes from the game's code alone: where it asked nothing
        // (yet: a song asked in `on_level_start` begins with the first
        // tick), what plays goes on, silence stays.
        let asked = self.game.music().is_some() || self.game.music_silent();
        let fresh = asked && std::mem::take(&mut self.music_changed);
        let player = fresh.then(|| (self.music(), self.crossfade.take()));
        let swap = self.laid_swap(fresh);
        NewMusic { player, swap }
    }

    /// What the mixer's song is to swap to, where the channels the game
    /// laid and the mixer's differ (`audio_laid`, or what a fresh song
    /// plays): those that play now, at once, where the mixer neither plays
    /// them nor is to swap to them (an ask called off after the audio,
    /// ahead of the clock, swapped; a state brought back); else those to
    /// come at the beat they are due at, or none to come. None: nothing to
    /// change (the audio ahead of the clock swapped already, or swaps to
    /// what the game made current as it reaches it).
    fn laid_swap(&self, fresh: bool) -> Option<Option<Box<audio::music::Swap>>> {
        let cue = self.game.music()?;
        let digest = |l: &Option<Arc<audio::music::Laid>>| l.as_ref().map(|l| l.digest);
        let (playing, pending) = if fresh { (digest(&cue.laid), None) } else { self.audio_laid? };
        let (now, next) = (digest(&cue.laid), cue.next.as_ref().map(|(l, _)| l.digest));
        if next.is_some() && playing == next && pending.is_none() {
            return None;
        }
        if playing != now && pending != now {
            let laid = cue.laid.as_ref()?;
            return Some(Some(audio::music::Swap::new(self.laid_song(cue.song, laid)?, f64::NEG_INFINITY)));
        }
        if pending == next || pending == now {
            return None;
        }
        Some(match &cue.next {
            Some((laid, due)) => Some(audio::music::Swap::new(self.laid_song(cue.song, laid)?, *due)),
            None => None,
        })
    }

    /// The next song handed begins from its start even where it is the
    /// song that plays: F5 and AGAIN, the game played again from its files
    /// (a stop or another song of the game's own and then this one again
    /// does the same, `notice_song`).
    pub fn restart_music(&mut self) {
        (self.restart, self.music_changed) = (true, true);
    }

    /// The game's music begun anew: the next feed makes it (the song that
    /// plays going on where it is the same), a window's front end having
    /// played its own meanwhile (the engine's title).
    pub fn music_anew(&mut self) {
        self.music_changed = true;
    }

    /// How far, in seconds, the time heard lies past the game's clock (a
    /// window's ticks owed, `app::TickClock::behind`): its own song keeps
    /// with that. 0 unless a window says.
    pub fn music_ahead(&mut self, seconds: f32) {
        self.music_ahead = f64::from(seconds.max(0.0));
    }

    /// The music fed to a mixer no other thread waits on (a film's sound,
    /// tests): `new_music` and `hand_music` at once.
    pub fn feed_music(&mut self, mixer: &mut Mixer, held: bool) -> bool {
        let new = self.new_music();
        self.hand_music(mixer, new, held)
    }

    /// Hands the music to the mixer: the song made anew when it changed
    /// (`new_music`), else the game's own song kept in time with its clock
    /// (held while the game is, `held`: a pause, a fade); either as loud
    /// and as muffled as the game asks (`Game::music_mix`). The song made
    /// anew is the song that plays (`Player::identity`: its file, text and
    /// laid channels): it plays on, never begun twice nor crossfaded into
    /// itself, but where F5 asked for it from the start
    /// (`restart_music`). Whether a song began.
    pub fn hand_music(&mut self, mixer: &mut Mixer, new: NewMusic, held: bool) -> bool {
        let restart = new.player.is_some() && std::mem::take(&mut self.restart);
        let playing = mixer.playing();
        let same = |p: &Option<audio::music::Player>| p.as_ref().and_then(audio::music::Player::identity).is_some_and(|id| playing == Some(id));
        let asked = new.player.is_some();
        let again = asked && !restart && new.player.as_ref().is_some_and(|(p, _)| same(p));
        // Resumed (`music:resume`): the song goes on from the sample it held
        // at, its clock taken as where it is (the audio held a frame and a
        // fade later than the clock did).
        let paused = self.game.music_paused();
        let resumed = std::mem::replace(&mut self.music_was_paused, paused) && !paused;
        let player = new.player.filter(|_| !again);
        let began = player.is_some();
        mixer.tally.played_on += u64::from(asked && !began);
        if let Some((player, crossfade)) = player {
            // Never a song crossfaded into itself: begun anew, the one
            // playing ends within 5 ms.
            let crossfade = crossfade.filter(|_| !same(&player));
            match (player, crossfade) {
                // Silence (a stop) fades over its own time, 5 ms at least,
                // so it never clicks.
                (None, seconds) => mixer.crossfade_music(None, seconds.unwrap_or(0.0).max(audio::music::SHORTEST.0)),
                (player, Some(seconds)) if seconds > 0.0 => mixer.crossfade_music(player, seconds),
                (player, _) => mixer.set_music(player),
            }
        } else if let Some(cue) = self.game.music() {
            if again || resumed {
                // The song that plays asked again (a run begun anew with
                // the song its title played, its clock at 0): it plays on,
                // its sound unbroken, the clock taken as where it is.
                mixer.rebase_music(cue.start + cue.beats, cue.ratio, self.music_ahead);
            }
            // (Paused, the song and its clock hold: nothing to follow.)
            if !paused {
                mixer.follow_music(cue.start + cue.beats, cue.ratio, self.music_ahead);
            }
            mixer.music_intensity(cue.intensity);
        }
        if let Some(swap) = new.swap {
            mixer.pend_music(swap);
        }
        if !began {
            mixer.music_mix(&self.game.music_mix());
        }
        mixer.hold_music((held && self.steers_music()) || self.fade_hold() || self.game.music_paused());
        self.audio_laid = mixer.music_laid();
        began
    }

    /// The title's song, begun at `c.title_screen.music_at`'s bar.
    pub fn title_music(&self) -> Option<audio::music::Player> {
        let mut player = self.song(title_song(&self.cfg, &self.songs)?)?;
        let at = self.cfg.title_screen.music_at;
        if at > 1 {
            player.seek(player.at_bar(at));
        }
        Some(player)
    }

    pub(crate) fn song(&self, file: &str) -> Option<audio::music::Player> {
        let path = self.files.path(file);
        let text = self.files.read_to_string(&path).map_err(|e| eprintln!("music: {}: {e}", path.display())).ok()?;
        match audio::music::parse(&text).and_then(|mut song| song.bind(&self.samples).map(|()| song)) {
            // The same file with the same text is the same song, the
            // title's, the player's or the game's own.
            Ok(song) => Some(audio::music::Player::new(song, self.cfg.audio.music_volume).identified(song_identity(file, &text))),
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

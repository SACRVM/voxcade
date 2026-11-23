//! Sound: what the simulation's events sound like, and the mixer that plays
//! them. A sound is a file of the game (`sound`: `sounds/NAME.toml`, a few
//! numbers for the synthesizer; a sound is no recording, it is code or
//! text, and only a song's instrument plays one, `music::sample`'s
//! `samples/`), its words in one register with MML's
//! (`words`). The engine and the kits bring sounds of their own, files of
//! the same kind built in; a game's file of the same name replaces one.
//! Sounds are placed in stereo by where they happen on screen.

pub mod limiter;
pub mod loudness;
pub mod music;
pub mod sound;
pub mod synth;
pub mod words;

#[cfg(test)]
mod hardening_tests;
#[cfg(test)]
mod digest_tests;
#[cfg(test)]
mod tools_tests;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use glam::Vec3;

pub use sound::{GameSound, SoundDef};
pub use synth::{SAMPLE_RATE, Synth, Wave};

/// The engine's own sounds' files, built in (`sounds/`).
pub const CORE_FILES: &[(&str, &str)] = &[
    ("impact", include_str!("sounds/impact.toml")),
    ("explosion", include_str!("sounds/explosion.toml")),
    ("crumble", include_str!("sounds/crumble.toml")),
];

/// The engine's own sound events (the simulation's core reports them): a
/// shot chipping the world, an explosion, a piece of the world coming down.
pub fn core_sounds() -> Vec<(&'static str, SoundDef)> {
    sound::builtin(CORE_FILES)
}

/// The sound of every event, ready to play.
#[derive(Default)]
pub struct SoundBank {
    sounds: HashMap<&'static str, Arc<[f32]>>,
}

impl SoundBank {
    /// The game's sounds: those it brings (`brought`, a kit's or the
    /// engine's, `Game::sounds`), each replaced by the game's own file of
    /// that name (`own`, `sound::read_folder`), and the game's others.
    pub fn load(files: &crate::files::Files, own: &BTreeMap<String, GameSound>, brought: &[(&'static str, SoundDef)]) -> Result<Self> {
        let mut bank = SoundBank::default();
        for (name, def, file) in sound::all(brought, own) {
            bank.put(files, name, &def).with_context(|| file.unwrap_or_else(|| format!("{name} (built in)")))?;
        }
        Ok(bank)
    }

    /// Renders one sound anew (its file changed) and keeps it by `name`.
    pub fn put(&mut self, _files: &crate::files::Files, name: &'static str, def: &SoundDef) -> Result<()> {
        self.sounds.remove(name);
        let samples = match def {
            SoundDef::Silent => Vec::new(),
            SoundDef::Synth(synth) => synth.render(),
        };
        if !samples.is_empty() {
            self.sounds.insert(name, samples.into());
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<[f32]>> {
        self.sounds.get(name)
    }
}

/// Gain and stereo pan (-1 left .. 1 right) of a sound at `at` heard from
/// the middle of the view, `width` voxels across: full and centred in the
/// middle, panned towards the screen edges, fainter further away. `right`:
/// the screen's right on the ground (+x, unless the camera turns).
pub fn place(at: Vec3, listener: Vec3, width: f32, right: glam::Vec2) -> (f32, f32) {
    let dx = (at - listener).truncate().dot(right);
    let distance = (at - listener).truncate().length() / width.max(1.0);
    let pan = (dx / (width * 0.5)).clamp(-1.0, 1.0) * 0.8;
    (1.0 / (1.0 + distance * distance * 3.0), pan)
}

const MAX_VOICES: usize = 48;
/// The same event at most this often at once (a machine gun would pile up).
const MAX_SAME: usize = 4;

struct Voice {
    samples: Arc<[f32]>,
    at: usize,
    left: f32,
    right: f32,
    name: &'static str,
    /// Samples of fade-out left, once stolen for a new sound (never to
    /// count again): it ends in silence instead of a click.
    fade: Option<usize>,
}

impl Voice {
    /// Whether it has played to its end or faded out.
    fn ended(&self) -> bool {
        self.at >= self.samples.len() || self.fade == Some(0)
    }
}

/// A stolen voice fades out over 5 ms.
const STEAL_FADE: usize = SAMPLE_RATE as usize / 200;

/// Mixes the music and the sounds that are playing; runs in the audio thread.
pub struct Mixer {
    voices: Vec<Voice>,
    music: Option<music::Player>,
    /// Samples the song has played since it was set.
    music_age: u64,
    /// How `music` fades in under a crossfade: samples gone, samples in
    /// all (None: as loud as it is).
    coming: Option<(u64, u64)>,
    /// The songs fading out under it, each finishing its own fade (a song
    /// asked while one still fades lets it ring out).
    leaving: [Option<Leaving>; LEAVING],
    /// The songs a finished fade let go of, for the main thread to drop
    /// when it hands the next song (the audio thread gives no memory back
    /// either).
    spent: [Option<music::Player>; LEAVING],
    /// Each song's own samples while two crossfade.
    apart: (Vec<f32>, Vec<f32>),
    /// The music held where it is (a game that reads its beats, paused).
    music_held: bool,
    /// The last stage: holds the mix under its ceiling (`limiter`; -1
    /// dBTP, or the game's `c.audio.ceiling`).
    limiter: limiter::Limiter,
}

/// A song fading out (`Mixer::leaving`): from the loudness it had as it
/// began to (the cosine of a quarter turn from there), samples gone,
/// samples in all.
struct Leaving {
    player: music::Player,
    from: f32,
    gone: u64,
    length: u64,
}

/// The most songs fading out at once: one more asked meanwhile cuts the
/// quietest.
const LEAVING: usize = 3;

/// The loudness of a fade `gone` samples into its `length`: in by the
/// sine, out by the cosine of a quarter turn, together as loud as either.
fn faded((gone, length): (u64, u64)) -> (f32, f32) {
    let x = (gone as f32 / length as f32).min(1.0) * std::f32::consts::FRAC_PI_2;
    (x.sin(), x.cos())
}

/// The most samples (both sides, interleaved) the mixer makes in one step:
/// a longer buffer is mixed block by block, the same samples as at once
/// (every stage goes sample by sample), so the buffers it works in are
/// made once, this big, and never grow on the audio thread.
pub const MIX_BLOCK: usize = 4096;

impl Default for Mixer {
    fn default() -> Self {
        Self {
            // Room for every voice and as many fading out: a sound played
            // under the lock takes no new memory either.
            voices: Vec::with_capacity(2 * MAX_VOICES),
            music: None,
            music_age: 0,
            coming: None,
            leaving: Default::default(),
            spent: Default::default(),
            apart: (vec![0.0; MIX_BLOCK], vec![0.0; MIX_BLOCK]),
            music_held: false,
            limiter: limiter::Limiter::new(SAMPLE_RATE),
        }
    }
}

impl Mixer {
    /// How many frames later the mix comes out than it is made: the
    /// limiter's look-ahead (66 at 44.1 kHz, 1.5 ms).
    pub fn latency(&self) -> usize {
        self.limiter.latency_samples()
    }

    /// Starts a song from the top (None: silence), whatever played or
    /// faded cut.
    pub fn set_music(&mut self, player: Option<music::Player>) {
        (self.music, self.music_age, self.coming, self.leaving, self.spent) = (player, 0, None, Default::default(), Default::default());
    }

    /// Starts a song from the top at once in place of the one playing (a
    /// game's song begun without a crossfade), the songs still fading out
    /// ringing out under it.
    pub fn start_music(&mut self, player: Option<music::Player>) {
        (self.music, self.music_age, self.coming, self.spent) = (player, 0, None, Default::default());
    }

    /// Starts a song as the one playing fades out over `seconds`, equal in
    /// power (the old by the cosine, the new by the sine of a quarter
    /// turn: together as loud as either), both going on as they were; a
    /// song still fading from before finishes its fade under them, and
    /// one still coming in fades out from as loud as it had come. A song
    /// begun only a moment ago (a quarter of a second) is not faded out
    /// but replaced, the new one coming in as it came, unless silence
    /// comes (a stop fades, however young the song); after silence the
    /// song begins at once.
    pub fn crossfade_music(&mut self, player: Option<music::Player>, seconds: f32) {
        let length = (seconds * SAMPLE_RATE as f32) as u64;
        let Some(old) = self.music.take().filter(|_| length > 0) else {
            return self.start_music(player);
        };
        if player.is_some() && self.music_age < u64::from(SAMPLE_RATE / 4) {
            (self.music, self.music_age, self.spent) = (player, 0, Default::default());
            return;
        }
        let from = self.coming.map_or(1.0, |c| faded(c).0);
        // Full: the quietest cut (dropped here, on the main thread).
        let free = self.leaving.iter().position(Option::is_none).unwrap_or_else(|| (0..LEAVING).min_by(|&i, &j| self.loudness(i).total_cmp(&self.loudness(j))).unwrap_or(0));
        self.leaving[free] = Some(Leaving { player: old, from, gone: 0, length });
        self.coming = player.is_some().then_some((0, length));
        (self.music, self.music_age, self.spent) = (player, 0, Default::default());
    }

    /// How loud the song fading out in slot `i` is now.
    fn loudness(&self, i: usize) -> f32 {
        self.leaving[i].as_ref().map_or(0.0, |l| l.from * faded((l.gone, l.length)).1)
    }

    /// Whether no music plays (none set, or a song that plays once
    /// ended).
    pub fn music_over(&self) -> bool {
        self.music.as_ref().is_none_or(music::Player::over) && self.leaving.iter().all(Option::is_none)
    }

    /// Whether nothing sounds: no voice plays and no music is set.
    pub fn idle(&self) -> bool {
        self.voices.iter().all(Voice::ended) && self.music.is_none() && self.leaving.iter().all(Option::is_none)
    }

    /// Keeps the music with a game's clock, `beats` since the song began,
    /// going `ratio` times as fast as its `#tempo` (`music::Player::follow`,
    /// `set_ratio`).
    pub fn follow_music(&mut self, beats: f64, ratio: f64) {
        if let Some(m) = &mut self.music {
            m.set_ratio(ratio);
            m.follow(beats);
            // What a swap let go of, dropped here on the main thread.
            drop(m.spent());
        }
    }

    /// The digests of the channels a game laid in the song that plays,
    /// and in those it is to swap to (`music::Player::laid`); None when no
    /// song plays.
    pub fn music_laid(&self) -> Option<music::Digests> {
        self.music.as_ref().map(music::Player::laid)
    }

    /// The laid channels the song that plays swaps to when they are due,
    /// in place of any to come before (None: none to come); what the song
    /// no longer holds is dropped here, on the main thread.
    pub fn pend_music(&mut self, swap: Option<Box<music::Swap>>) {
        if let Some(m) = &mut self.music {
            drop(m.pend(swap));
        }
    }

    /// The game's tension: the song's layers follow it.
    pub fn music_intensity(&mut self, intensity: f32) {
        if let Some(m) = &mut self.music {
            m.intensity = intensity;
        }
    }

    /// How loud and how muffled the game asks its song to sound
    /// (`music:volume`, `music:muffle`).
    pub fn music_mix(&mut self, mix: &music::Mix) {
        if let Some(m) = &mut self.music {
            m.follow_mix(mix);
        }
        for l in self.leaving.iter_mut().flatten() {
            l.player.follow_mix(mix);
        }
    }

    /// Holds the music where it is, or lets it go on.
    pub fn hold_music(&mut self, held: bool) {
        self.music_held = held;
    }

    /// The title of the song that plays.
    pub fn music_title(&self) -> Option<&str> {
        self.music.as_ref().map(music::Player::title)
    }

    /// How hard the mix goes into the limiter, in dB (`c.audio.drive`).
    pub fn set_drive(&mut self, drive_db: f32) {
        self.limiter.set_drive_db(f64::from(drive_db));
    }

    /// The limiter's ceiling, in dB true peak (`c.audio.ceiling`).
    pub fn set_ceiling(&mut self, ceiling_db: f32) {
        self.limiter.set_ceiling_db(f64::from(ceiling_db));
    }

    pub fn set_music_volume(&mut self, volume: f32) {
        if let Some(m) = &mut self.music {
            m.volume = volume;
        }
        for l in self.leaving.iter_mut().flatten() {
            l.player.volume = volume;
        }
    }

    pub fn play(&mut self, name: &'static str, samples: &Arc<[f32]>, gain: f32, pan: f32) {
        // The voices that ended let go of here, on the main thread.
        self.voices.retain(|v| !v.ended());
        if gain <= 0.001 {
            return;
        }
        // Replace the oldest of the same sound, or of any once full.
        // (Fading ones, already given up, do not count.)
        let live = |v: &Voice| v.fade.is_none();
        let same = |v: &Voice| v.name == name && live(v);
        let steal = if self.voices.iter().filter(|v| same(v)).count() >= MAX_SAME {
            (0..self.voices.len()).filter(|&i| same(&self.voices[i])).max_by_key(|&i| self.voices[i].at)
        } else if self.voices.iter().filter(|v| live(v)).count() >= MAX_VOICES {
            (0..self.voices.len()).filter(|&i| live(&self.voices[i])).max_by_key(|&i| self.voices[i].at)
        } else {
            None
        };
        if let Some(i) = steal {
            self.voices[i].fade = Some(STEAL_FADE);
        }
        // Equal-power panning.
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        self.voices.push(Voice { samples: samples.clone(), at: 0, left: gain * angle.cos(), right: gain * angle.sin(), name, fade: None });
    }

    /// The music into `out` (zeros): the song alone, or with those
    /// fading out under it, each by its own fade.
    fn mix_music(&mut self, out: &mut [f32]) {
        let frames = out.len() as u64 / 2;
        self.music_age += frames;
        if self.coming.is_none() && self.leaving.iter().all(Option::is_none) {
            if let Some(m) = &mut self.music {
                m.mix(out);
            }
            return;
        }
        // `out` is a block at most (`mix`): the buffers made for it hold it.
        debug_assert!(out.len() <= MIX_BLOCK && self.apart.0.len() == MIX_BLOCK && self.apart.1.len() == MIX_BLOCK, "a crossfade's buffers grow on the audio thread");
        let (a, b) = &mut self.apart;
        let (a, b) = (&mut a[..out.len()], &mut b[..out.len()]);
        for slot in &mut self.leaving {
            let Some(l) = slot else { continue };
            a.fill(0.0);
            l.player.mix(a);
            for (i, frame) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                let fade_out = l.from * faded((l.gone + i as u64, l.length)).1;
                frame[0] += a[i * 2] * fade_out;
                frame[1] += a[i * 2 + 1] * fade_out;
            }
            l.gone += frames;
            // The song faded out kept for the main thread to drop (the
            // slots are emptied whenever a song is handed, so one is free).
            if l.gone >= l.length
                && let Some(free) = self.spent.iter_mut().find(|s| s.is_none())
            {
                *free = slot.take().map(|l| l.player);
            }
        }
        b.fill(0.0);
        if let Some(m) = &mut self.music {
            m.mix(b);
        }
        for (i, frame) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let fade_in = self.coming.map_or(1.0, |c| faded((c.0 + i as u64, c.1)).0);
            frame[0] += b[i * 2] * fade_in;
            frame[1] += b[i * 2 + 1] * fade_in;
        }
        if let Some((gone, length)) = &mut self.coming {
            *gone += frames;
            if *gone >= *length {
                self.coming = None;
            }
        }
    }

    /// Fills interleaved stereo samples, `MIX_BLOCK` at a time.
    pub fn mix(&mut self, out: &mut [f32]) {
        for block in out.chunks_mut(MIX_BLOCK) {
            self.mix_block(block);
        }
    }

    fn mix_block(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        if !self.music_held {
            self.mix_music(out);
        }
        for v in &mut self.voices {
            for frame in out.as_chunks_mut::<2>().0 {
                let Some(&s) = v.samples.get(v.at) else { break };
                let s = match &mut v.fade {
                    Some(0) => break,
                    Some(left) => {
                        *left -= 1;
                        s * *left as f32 / STEAL_FADE as f32
                    }
                    None => s,
                };
                frame[0] += s * v.left;
                frame[1] += s * v.right;
                v.at += 1;
            }
        }
        // (A voice that ended is taken out by the main thread, `play`: its
        // samples may be the last of a sound made anew meanwhile.)
        // When much plays at once, turned down just enough to stay under the
        // ceiling, both sides alike, and back up slowly; a mix under it comes
        // out as it is, 1.5 ms later.
        self.limiter.process_interleaved_stereo(out);
    }
}

/// The game's sound from the default device. Its mixer lies apart from
/// SDL's stream, behind a lock of its own that the device's callback holds
/// only while it mixes: the game never waits on SDL's audio. (A Bluetooth
/// headset dropping out and the sound server started anew left SDL's
/// stream locked; the game, locking it every frame, froze with it and could
/// not be quit.)
pub struct Speaker {
    mixer: Arc<Mutex<Mixer>>,
    held: Arc<AtomicBool>,
    stream: Option<Detached>,
}

impl Speaker {
    /// The mixer, to change what plays (none if the callback broke off
    /// mixing).
    pub fn lock(&mut self) -> Option<MutexGuard<'_, Mixer>> {
        self.mixer.lock().ok()
    }

    /// Silent and still while held (the IDE's game held behind a file),
    /// going on where it stood after: the mixer is not asked meanwhile.
    pub fn hold(&self, held: bool) {
        self.held.store(held, Ordering::Relaxed);
    }
}

impl Drop for Speaker {
    /// The stream is closed on a thread of its own: closing waits for SDL's
    /// audio, which a sound server gone away can hold for good, and nothing
    /// that ends (a run, the window) waits with it; the program's end takes
    /// that thread.
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = std::thread::Builder::new().name("audio close".into()).spawn(move || {
                let _one = ONE_AT_A_TIME.lock();
                drop(stream);
            });
        }
    }
}

/// A stream is opened or closed, never both at once: SDL takes its sound
/// server's loop down with the last stream, and a stream opened meanwhile
/// reached into it (PLAY SOUND pressed twice in the IDE crashed it).
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The right to open a stream, waited for a moment while one closes: a
/// close that hangs (a sound server gone) must not hold the window.
fn may_open() -> Option<MutexGuard<'static, ()>> {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(500);
    loop {
        match ONE_AT_A_TIME.try_lock() {
            Ok(guard) => return Some(guard),
            Err(std::sync::TryLockError::Poisoned(p)) => return Some(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) if std::time::Instant::now() < until => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(_) => return None,
        }
    }
}

/// What the device's callback pulls the sound from.
struct Feed {
    mixer: Arc<Mutex<Mixer>>,
    held: Arc<AtomicBool>,
    buf: Vec<f32>,
}

impl Feed {
    /// The `requested` samples (both sides) handed to `put` a block at a
    /// time, mixed in `buf`, made `MIX_BLOCK` big once: the audio thread
    /// takes no memory, however much the device asks for (it resized the
    /// buffer to every request).
    fn feed(&mut self, requested: i32, mut put: impl FnMut(&[f32])) {
        let mut left = requested.max(0) as usize & !1;
        let held = self.held.load(Ordering::Relaxed);
        let mut mixer = self.mixer.lock();
        while left > 0 {
            let block = &mut self.buf[..left.min(MIX_BLOCK)];
            match &mut mixer {
                Ok(mixer) if !held => mixer.mix(block),
                _ => block.fill(0.0),
            }
            put(block);
            left -= block.len();
        }
        debug_assert_eq!(self.buf.len(), MIX_BLOCK, "the feed's buffer grew on the audio thread");
    }
}

impl sdl3::audio::AudioCallback<f32> for Feed {
    fn callback(&mut self, stream: &mut sdl3::audio::AudioStream, requested: i32) {
        self.feed(requested, |block| {
            let _ = stream.put_data_f32(block);
        });
    }
}

/// SDL's stream with the callback that feeds it, to be closed on another
/// thread (`Speaker`'s drop).
struct Detached(#[allow(dead_code)] sdl3::audio::AudioStreamWithCallback<Feed>);

// SAFETY: SDL3's audio stream functions may be called from any thread (its
// SDL_audio.h: they are thread-safe), and the callback's own data is Send.
unsafe impl Send for Detached {}

/// The game's sound from the default device (`Speaker`). None (and why,
/// said) when there is no sound: the game runs silent.
pub fn speaker(sdl: &sdl3::Sdl) -> Option<Speaker> {
    let spec = sdl3::audio::AudioSpec { freq: Some(SAMPLE_RATE as i32), channels: Some(2), format: Some(sdl3::audio::AudioFormat::f32_sys()) };
    let (mixer, held) = (Arc::new(Mutex::new(Mixer::default())), Arc::new(AtomicBool::new(false)));
    let feed = Feed { mixer: mixer.clone(), held: held.clone(), buf: vec![0.0; MIX_BLOCK] };
    let Some(_one) = may_open() else {
        eprintln!("no sound: the speaker opened before is still closing");
        return None;
    };
    let audio = sdl.audio().map_err(|e| anyhow::anyhow!("initializing SDL audio: {e}"));
    match audio.and_then(|a| a.open_playback_stream(&spec, feed).map_err(|e| anyhow::anyhow!("opening audio: {e}"))) {
        Ok(stream) => match stream.resume() {
            Ok(()) => Some(Speaker { mixer, held, stream: Some(Detached(stream)) }),
            Err(e) => {
                eprintln!("no sound: {e}");
                None
            }
        },
        Err(e) => {
            eprintln!("no sound: {e:#}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every event has a sound of its own, and none is silent or too loud.
    #[test]
    fn every_event_has_a_sound() {
        for brought in [crate::shooter::sounds(), <crate::platformer::Platformer as crate::game::Game>::sounds(&Default::default()), core_sounds()] {
            let bank = SoundBank::load(&crate::files::Files::folder(Path::new(".")), &Default::default(), &brought).unwrap();
            for (name, _) in &brought {
                let samples = bank.get(name).unwrap_or_else(|| panic!("{name} has no sound"));
                let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                assert!(peak > 0.05 && peak <= 1.0, "{name} peaks at {peak}");
            }
        }
    }

    /// The kits' sounds are files built in, read by the same reader as a
    /// game's: one for every name the kit plays, none more.
    #[test]
    fn the_kits_sound_files_are_their_names() {
        let mut shooter: Vec<&str> = crate::shooter::SOUND_FILES.iter().chain(CORE_FILES).map(|(n, _)| *n).collect();
        let mut events: Vec<&str> = crate::shooter::Sfx::ALL.iter().map(|s| s.name()).collect();
        shooter.sort();
        events.sort();
        assert_eq!(shooter, events, "the shooter's files and its sounds");
        let platformer: Vec<&str> = crate::platformer::SOUND_FILES.iter().map(|(n, _)| *n).collect();
        let named: Vec<&str> = <crate::platformer::Platformer as crate::game::Game>::sounds(&Default::default()).into_iter().map(|(n, _)| n).collect();
        assert_eq!(platformer, named, "the platformer's files and its sounds");
        // Each a file of its own folder, as `include_str!` names it.
        for (dir, list) in [("src/shooter/sounds", crate::shooter::SOUND_FILES), ("src/platformer/sounds", crate::platformer::SOUND_FILES), ("src/audio/sounds", CORE_FILES)] {
            let mut on_disk: Vec<String> = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(dir)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            on_disk.sort();
            let mut listed: Vec<String> = list.iter().map(|(n, _)| format!("{n}.toml")).collect();
            listed.sort();
            assert_eq!(on_disk, listed, "{dir}");
            for (name, text) in list {
                sound::read(&format!("{dir}/{name}.toml"), text).unwrap_or_else(|e| panic!("{e:#}"));
            }
        }
    }

    /// A game's file replaces the kit's sound of its name, `silent`
    /// silences one, a new name is one more; a recording is none.
    #[test]
    fn a_games_file_replaces_the_kits_sound() {
        let dir = crate::scratch::Scratch::new("bank");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        std::fs::write(dir.join("sounds/shot.toml"), "wave = \"sine\"\nfreq = 300\nhold = 0\ndecay = 0.05\n").unwrap();
        std::fs::write(dir.join("sounds/rotor.toml"), "silent = true\n").unwrap();
        std::fs::write(dir.join("sounds/beep.toml"), "freq = 880\n").unwrap();
        let files = crate::files::Files::folder(&dir);
        let own = sound::read_folder(&files).unwrap();
        let bank = SoundBank::load(&files, &own, &crate::shooter::sounds()).unwrap();
        assert_eq!(bank.get("shot").unwrap().len(), (0.05 * SAMPLE_RATE as f32).ceil() as usize, "the game's shot");
        assert!(bank.get("rotor").is_none(), "silent");
        assert!(bank.get("beep").is_some() && bank.get("splat").is_some(), "the game's own and the kit's");
        std::fs::write(dir.join("sounds/scream.wav"), b"RIFF").unwrap();
        assert!(format!("{:#}", sound::read_folder(&files).unwrap_err()).contains("a sound plays none"));
        assert!(sound::NO_RECORDINGS.contains("samples/ as a WAV"), "it points to samples/");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Everything at once, loud, stays under the limiter's ceiling (by
    /// BS.1770's true peak), mixed in blocks as the device asks for them.
    #[test]
    fn a_loud_mix_stays_under_the_ceiling() {
        let mut mixer = Mixer::default();
        for i in 0..MAX_VOICES {
            let synth = Synth { wave: if i % 3 == 0 { Wave::Noise } else { Wave::Square }, freq: 60.0 + 70.0 * i as f32, volume: 1.0, ..Synth::default() };
            mixer.play(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"][i % 12], &Arc::from(synth.render()), 1.0, (i as f32 / 24.0) - 1.0);
        }
        let mut all = Vec::new();
        let mut block = vec![0.0f32; 1024];
        for _ in 0..20 {
            mixer.mix(&mut block);
            all.extend_from_slice(&block);
        }
        let ceiling = mixer.limiter.ceiling();
        assert!(limiter::true_peak_stereo(&all) <= ceiling * 1.001, "true peak {:.3} dBTP", limiter::db_tp(limiter::true_peak_stereo(&all)));
        assert!(all.iter().fold(0.0f32, |m, s| m.max(s.abs())) > ceiling * 0.9, "it plays");
    }

    /// A sound on the left of the view plays louder on the left; in a
    /// level seen from behind, one at the world's west is on the right.
    #[test]
    fn sounds_are_placed_in_stereo() {
        let (gain, pan) = place(Vec3::new(-50.0, 0.0, 0.0), Vec3::ZERO, 200.0, glam::Vec2::X);
        assert_eq!(place(Vec3::new(-50.0, 0.0, 0.0), Vec3::ZERO, 200.0, glam::Vec2::NEG_X), (gain, -pan), "seen from behind");
        let mut mixer = Mixer::default();
        mixer.play("shot", &Arc::from(vec![0.5f32; 100]), gain, pan);
        let mut out = vec![0.0; 200];
        mixer.mix(&mut out);
        let at = mixer.latency() * 2;
        assert!(out[at] > out[at + 1] * 1.5, "left {} right {}", out[at], out[at + 1]);
        let (far, _) = place(Vec3::new(400.0, 0.0, 0.0), Vec3::ZERO, 200.0, glam::Vec2::X);
        assert!(far < gain * 0.3, "far away is not much quieter ({far} vs {gain})");
    }

    /// Counts what the thread that watches takes from the heap and gives
    /// back to it (`taken_by`): the audio thread's rule checked, not
    /// trusted. Every other allocation goes to the system as ever.
    struct Counting;

    thread_local! {
        static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static TAKEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn count() {
        if WATCHING.try_with(std::cell::Cell::get).unwrap_or(false) {
            TAKEN.with(|t| t.set(t.get() + 1));
        }
    }

    // SAFETY: it hands every call on to the system's allocator unchanged.
    unsafe impl std::alloc::GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            count();
            unsafe { std::alloc::System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
            count();
            unsafe { std::alloc::System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, at: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
            count();
            unsafe { std::alloc::System.realloc(at, layout, size) }
        }
        unsafe fn dealloc(&self, at: *mut u8, layout: std::alloc::Layout) {
            count();
            unsafe { std::alloc::System.dealloc(at, layout) }
        }
    }

    #[global_allocator]
    static COUNTING: Counting = Counting;

    /// How many times `work` took memory or gave it back, on this thread.
    fn taken_by(work: impl FnOnce()) -> usize {
        TAKEN.with(|t| t.set(0));
        WATCHING.with(|w| w.set(true));
        work();
        WATCHING.with(|w| w.set(false));
        TAKEN.with(std::cell::Cell::get)
    }

    /// The audio thread takes no memory and gives none back: the mixer's
    /// crossfade buffers and the device's feed are made once, `MIX_BLOCK`
    /// big, and a request longer than that is mixed block by block into
    /// the same samples as in one piece (they were resized to every
    /// request, on the thread that must never wait); a song a finished
    /// crossfade let go of is kept for the main thread to drop as it hands
    /// the next (it was dropped as the crossfade ended). A singing voice,
    /// driven and crushed, plays meanwhile (its syllables laid out as the
    /// song is read), and its song goes round, its notes ringing on.
    #[test]
    fn the_audio_thread_takes_no_memory() {
        let song = |text: &str| Some(music::Player::new(music::parse(text).unwrap(), 1.0));
        let crossfading = || {
            let mut mixer = Mixer::default();
            mixer.set_music(song("#tempo 120\n@s square\n@v voice drive=12 crush_rate=6000 crush_bits=4\nA: @s c d e f g a b > c\nB: @v \"l a\" c \"s i ng\" d e \"t oi\" f"));
            let mut warm = vec![0.0f32; SAMPLE_RATE as usize];
            mixer.mix(&mut warm);
            mixer.crossfade_music(song("#tempo 90\n@n noise\nA: o2 c4 e g\nB: @n c8 c8 c8"), 2.0);
            mixer.play("blip", &Arc::from(vec![0.5f32; 30_000]), 1.0, 0.3);
            mixer
        };
        let (mut whole, mut parts) = (crossfading(), crossfading());
        let buffers = (whole.apart.0.as_ptr(), whole.apart.1.as_ptr(), whole.voices.capacity());
        let mut once = vec![0.0f32; 3 * MIX_BLOCK + 10];
        assert_eq!(taken_by(|| whole.mix(&mut once)), 0, "memory taken or given back while mixing");
        assert!(whole.leaving.iter().any(Option::is_some), "still crossfading");
        assert_eq!((whole.apart.0.as_ptr(), whole.apart.1.as_ptr(), whole.voices.capacity()), buffers, "the buffers stayed");
        assert_eq!((whole.apart.0.len(), whole.apart.1.len()), (MIX_BLOCK, MIX_BLOCK));
        let mut pieces = Vec::new();
        for size in [6, 1000, MIX_BLOCK, 1, 2 * MIX_BLOCK].into_iter().cycle() {
            let mut block = vec![0.0f32; (size & !1).max(2).min(once.len() - pieces.len())];
            parts.mix(&mut block);
            pieces.extend_from_slice(&block);
            if pieces.len() == once.len() {
                break;
            }
        }
        assert!(once == pieces, "mixed in blocks, the same samples");
        // The feed hands a long request on block by block from its own buffer.
        let mut feed = Feed { mixer: Arc::new(Mutex::new(crossfading())), held: Arc::new(AtomicBool::new(false)), buf: vec![0.0; MIX_BLOCK] };
        let at = feed.buf.as_ptr();
        let (mut handed, mut blocks) = (0, 0);
        feed.feed(5 * MIX_BLOCK as i32 + 7, |b| {
            handed += b.len();
            blocks += 1;
            assert!(b.len() <= MIX_BLOCK);
        });
        assert_eq!((handed, blocks, feed.buf.as_ptr(), feed.buf.len()), (5 * MIX_BLOCK + 6, 6, at, MIX_BLOCK));
        // The crossfade ends (2 s), the new song goes round (2 s) and the
        // sound ends, the last to hold its samples, on the audio thread:
        // nothing taken or given back, the old song and the sound kept.
        let mut long = vec![0.0f32; 5 * SAMPLE_RATE as usize];
        assert_eq!(taken_by(|| whole.mix(&mut long)), 0, "memory taken or given back as the crossfade and a sound ended");
        assert!(whole.leaving.iter().all(Option::is_none) && whole.spent[0].is_some(), "the song faded out kept for the main thread");
        assert!(whole.voices.len() == 1 && whole.voices[0].ended(), "the sound that ended kept for the main thread");
        // Let go of on the main thread, as the next song and sound are handed.
        whole.set_music(None);
        whole.play("blip", &Arc::from(vec![0.5f32; 10]), 1.0, 0.0);
        assert!(whole.spent.iter().all(Option::is_none) && whole.voices.len() == 1 && !whole.voices[0].ended());
        // Channels a game laid swapped in on the audio thread: one the same,
        // one changed under a ringing note, one laid no more (its note rings
        // out in a tail), one new, the song of other bars, a layer and the
        // ducking going on; the old song kept for the main thread.
        let studio = "#tempo 120\n#layer N 0.5\n@k triangle sidechain=0.5 release=0.4\n@n noise drive=6 crush_bits=6 release=0.3\n@p square duty=0.25 release=0.5\n";
        let laid = |c: &[(char, &str)]| music::Laid::new(c.iter().map(|(l, t)| (*l, t.to_string())).collect());
        let (from, to) = (laid(&[('A', "@k c4 c4 c4 c4"), ('B', "@p c1"), ('G', "@n c2 c2")]), laid(&[('A', "@k c4 c4 c4 c4 c4 c4 c4 c4"), ('B', "@p e1 g1"), ('N', "@n c1 c1")]));
        let mut mixer = Mixer::default();
        mixer.set_music(Some(music::Player::new(music::lay(studio, &from).unwrap(), 1.0)));
        mixer.music_intensity(0.7);
        mixer.pend_music(Some(music::Swap::new(music::lay(studio, &to).unwrap(), 3.5)));
        // Strips gliding under it (`music:mix`), one on a channel laid no
        // more, one on a channel to come.
        let mut mix = music::Mix::default();
        let mut strip = music::Strip { mute: true, ..Default::default() };
        strip.level.ask(0.0, 0.1);
        strip.pan.ask(0.7, 0.2);
        Arc::make_mut(&mut mix.strips).extend([('G', strip), ('N', strip), ('A', music::Strip::default())]);
        mixer.music_mix(&mix);
        let mut long = vec![0.0f32; 2 * 3 * SAMPLE_RATE as usize];
        assert_eq!(taken_by(|| mixer.mix(&mut long)), 0, "memory taken or given back as laid channels were swapped in");
        assert_eq!(mixer.music_laid(), Some((Some(to.digest), None)));
        assert!(mixer.music.as_ref().is_some_and(|m| m.spent.is_some()), "what was swapped out kept for the main thread");
        mixer.follow_music(6.0, 1.0);
        assert!(mixer.music.as_ref().is_some_and(|m| m.spent.is_none()), "dropped on the main thread");
        // A tempo set every tick for 10 s (`music.tempo` turned as a knob,
        // half to twice the song's own): nothing made, nothing jumping, and
        // no sample further from the one before than the song's own at
        // the slowest or the fastest of them held still.
        let song = "#tempo 120\n@s sine attack=0.005 decay=0.1 sustain=0.6 release=0.05 volume=0.8\nA: @s l8 c e g > c < g e c e\n";
        let knob = |ratio: &dyn Fn(usize) -> f64| {
            let mut mixer = Mixer::default();
            mixer.set_music(Some(music::Player::new(music::parse(song).unwrap(), 1.0)));
            let tick = SAMPLE_RATE as usize / 60;
            let (mut buf, mut out) = (vec![0.0f32; tick * 2], Vec::<f32>::new());
            let mut beats = 0.0;
            for n in 0..600 {
                let ratio = ratio(n);
                beats += 120.0 * ratio / 3600.0;
                assert_eq!(taken_by(|| {
                    mixer.mix(&mut buf);
                    mixer.follow_music(beats, ratio);
                }), 0, "memory taken with a tempo set every tick");
                assert!(mixer.music.as_ref().is_some_and(|m| m.since_jump as usize > tick), "a jump at tick {n}");
                out.extend(buf.iter().step_by(2));
            }
            out.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()))
        };
        let (slow, fast, turned) = (knob(&|_| 0.5), knob(&|_| 2.0), knob(&|n| 0.5 + (n % 90) as f64 / 60.0));
        assert!(turned <= slow.max(fast) * 1.05, "a tempo turned stepped {turned}, held at half and twice {slow} {fast}");
        // A strip's cutoff swept every tick for 10 s over sounding notes
        // (`music:mix`'s knobs), its resonance and a next note's decay
        // turned with it: nothing taken on the audio thread.
        let acid = "#tempo 130\n@b saw filter=lowpass cutoff=400 cutoff_end=1200 cutoff_time=0.1 resonance=0.6 attack=0.002 decay=0.15 sustain=0.3\nB: @b o2 l16 c c > c < c d# c c > c < c f c c > c < c g\n";
        let mut mixer = Mixer::default();
        mixer.set_music(Some(music::Player::new(music::parse(acid).unwrap(), 1.0)));
        let tick = SAMPLE_RATE as usize / 60;
        let mut buf = vec![0.0f32; tick * 2];
        let mut mix = music::Mix::default();
        for n in 0..600 {
            let strip = Arc::make_mut(&mut mix.strips).entry('B').or_default();
            strip.turn("cutoff", Some(300.0 + 2000.0 * (n % 120) as f32 / 120.0), 1.0 / 60.0);
            strip.turn("resonance", Some((n % 50) as f32 / 50.0), 0.0);
            strip.turn("decay", Some(0.05 + (n % 30) as f32 / 100.0), 0.0);
            mixer.music_mix(&mix);
            mix.step();
            assert_eq!(taken_by(|| mixer.mix(&mut buf)), 0, "memory taken with a knob turned every tick");
        }
        // Samples playing (`samples/`): one round and round, one backwards
        // as a layer, one played out, its stretch turned from the next
        // note, a jump into it; their recordings read once, behind an Arc.
        let dir = crate::scratch::Scratch::new("audio-samples");
        std::fs::create_dir_all(dir.join("samples")).unwrap();
        let tone: Vec<f32> = (0..30_000).map(|i| 0.5 * (std::f32::consts::TAU * 220.0 * i as f32 / 48_000.0).sin()).collect();
        std::fs::write(dir.join("samples/tone.wav"), music::sample::wav_file(48_000, 1, 24, false, &tone)).unwrap();
        let samples = music::Samples::read_folder(&crate::files::Files::folder(&dir)).unwrap();
        let text = "#tempo 120\n@l sample name=tone loop=yes release=0.2\n@h sine freq=80 decay=0.2 sustain=0\n+ sample name=tone reverse=yes root=g3 volume=0.5\n@o sample name=tone end=0.3\nA: @l o3 c1 e1\nB: @h l8 c e g e\nC: @o l4 c e g >c<\n";
        let mut song = music::parse(text).unwrap();
        song.bind(&samples).unwrap();
        let mut mixer = Mixer::default();
        mixer.set_music(Some(music::Player::new(song, 1.0)));
        let mut mix = music::Mix::default();
        let strip = Arc::make_mut(&mut mix.strips).entry('C').or_default();
        strip.turn("start", Some(0.1), 0.0);
        strip.turn("root", Some(330.0), 0.0);
        mixer.music_mix(&mix);
        let mut long = vec![0.0f32; 2 * 3 * SAMPLE_RATE as usize];
        assert_eq!(taken_by(|| mixer.mix(&mut long)), 0, "memory taken with samples playing");
        assert!(long.iter().any(|v| v.abs() > 0.1), "the samples sound");
        assert_eq!(taken_by(|| mixer.follow_music(7.0, 1.0)), 0, "memory taken jumping into a sample");
        assert_eq!(taken_by(|| mixer.mix(&mut long)), 0, "memory taken with samples playing after a jump");
    }

    /// A song asked while another still fades out lets it finish its fade
    /// under the new one: after a stop's fade (the new song begun at once),
    /// and in a crossfade during a crossfade (the one coming in fading out
    /// from as loud as it had come), without a step and without memory
    /// taken on the audio thread.
    #[test]
    fn a_song_fading_out_rings_out_under_the_next() {
        let sr = SAMPLE_RATE as usize;
        let song = |hz: u32| Some(music::Player::new(music::parse(&format!("#tempo 120\n@x sine freq={hz} attack=0.002 decay=0 sustain=1 release=0.01 volume=0.3\nA: @x c1 c1 c1 c1\n")).unwrap(), 1.0));
        // How strongly `hz` sounds in the left side of `s` over 1/60 s from `at` seconds.
        let tone = |s: &[f32], at: f32, hz: f64| {
            let from = (at * sr as f32) as usize;
            let x: Vec<f32> = s[from * 2..(from + sr / 60) * 2].iter().step_by(2).copied().collect();
            let w = std::f64::consts::TAU * hz / sr as f64;
            let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (k, &v)| (re + f64::from(v) * (w * k as f64).cos(), im - f64::from(v) * (w * k as f64).sin()));
            2.0 * (re * re + im * im).sqrt() / x.len() as f64
        };
        let steepest = |s: &[f32], from: f32, to: f32| s[(from * sr as f32) as usize * 2..(to * sr as f32) as usize * 2].iter().step_by(2).collect::<Vec<_>>().windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        let run = |steps: &mut dyn FnMut(&mut Mixer, usize)| {
            let mut mixer = Mixer::default();
            let (mut out, mut tick) = (Vec::new(), vec![0.0f32; sr / 60 * 2]);
            for n in 0..180 {
                steps(&mut mixer, n);
                assert_eq!(taken_by(|| mixer.mix(&mut tick)), 0, "memory taken at tick {n}");
                out.extend_from_slice(&tick);
            }
            out
        };
        // A stop fading over 2 s from 0.5 s; at 1 s a song begins at once.
        let stopped = run(&mut |m, n| match n {
            0 => m.set_music(song(420)),
            30 => m.crossfade_music(None, 2.0),
            60 => m.start_music(song(660)),
            _ => {}
        });
        let full = tone(&stopped, 0.3, 420.0);
        assert!(full > 0.1);
        let half = std::f64::consts::FRAC_PI_4.cos();
        assert!((tone(&stopped, 1.5, 420.0) / full - half).abs() < 0.02, "the stop's fade cut: {}", tone(&stopped, 1.5, 420.0) / full);
        assert!((tone(&stopped, 1.1, 660.0) / full - 1.0).abs() < 0.02, "the new song at once");
        assert!(tone(&stopped, 2.6, 420.0) < 0.001 * full, "the old song gone once faded");
        // A crossfade of 2 s from 0.5 s, and at 1 s another of 2 s.
        let twice = run(&mut |m, n| match n {
            0 => m.set_music(song(420)),
            30 => m.crossfade_music(song(660), 2.0),
            60 => m.crossfade_music(song(300), 2.0),
            _ => {}
        });
        let (a, b, c) = (tone(&twice, 1.5, 420.0) / full, tone(&twice, 1.0, 660.0) / full, tone(&twice, 1.5, 300.0) / full);
        assert!((a - half).abs() < 0.03, "the first fade finished under the second: {a}");
        assert!((b - (std::f64::consts::PI / 8.0).sin()).abs() < 0.03, "the song coming in fades out from as loud as it came: {b}");
        assert!((c - (std::f64::consts::PI / 8.0).sin()).abs() < 0.03, "the third coming in: {c}");
        for s in [&stopped, &twice] {
            let (own, around) = (steepest(s, 1.05, 1.3), steepest(s, 0.95, 1.05));
            assert!(around <= own * 1.1, "stepped {around} where a song was asked, {own} as they sound on");
        }
    }
}

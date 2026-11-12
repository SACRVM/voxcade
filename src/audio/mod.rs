//! Sound: what the simulation's events sound like, and the mixer that plays
//! them. A sound is a file of the game (`sound`: `sounds/NAME.toml`, a few
//! numbers for the synthesizer; Voxcade plays no recordings, everything in
//! a game is code or text), its words in one register with MML's
//! (`words`). The engine and the kits bring sounds of their own, files of
//! the same kind built in; a game's file of the same name replaces one.
//! Sounds are placed in stereo by where they happen on screen.

pub mod limiter;
pub mod loudness;
pub mod music;
pub mod sound;
pub mod synth;
pub mod words;

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
/// middle, panned towards the screen edges, fainter further away.
pub fn place(at: Vec3, listener: Vec3, width: f32) -> (f32, f32) {
    let dx = at.x - listener.x;
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
}

/// Mixes the music and the sounds that are playing; runs in the audio thread.
pub struct Mixer {
    voices: Vec<Voice>,
    music: Option<music::Player>,
    /// The music held where it is (a game that reads its beats, paused).
    music_held: bool,
    /// The last stage: holds the mix under -1 dBTP (`limiter`).
    limiter: limiter::Limiter,
}

impl Default for Mixer {
    fn default() -> Self {
        Self { voices: Vec::new(), music: None, music_held: false, limiter: limiter::Limiter::new(SAMPLE_RATE) }
    }
}

impl Mixer {
    /// How many frames later the mix comes out than it is made: the
    /// limiter's look-ahead (66 at 44.1 kHz, 1.5 ms).
    pub fn latency(&self) -> usize {
        self.limiter.latency_samples()
    }

    /// Starts a song from the top (None: silence).
    pub fn set_music(&mut self, player: Option<music::Player>) {
        self.music = player;
    }

    /// Whether no music plays (none set, or a song that plays once
    /// ended).
    pub fn music_over(&self) -> bool {
        self.music.as_ref().is_none_or(music::Player::over)
    }

    /// Whether nothing sounds: no voice plays and no music is set.
    pub fn idle(&self) -> bool {
        self.voices.is_empty() && self.music.is_none()
    }

    /// Keeps the music with a game's clock (`music::Player::follow`).
    pub fn follow_music(&mut self, target: u64) {
        if let Some(m) = &mut self.music {
            m.follow(target);
        }
    }

    /// The game's tension: the song's layers follow it.
    pub fn music_intensity(&mut self, intensity: f32) {
        if let Some(m) = &mut self.music {
            m.intensity = intensity;
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

    pub fn set_music_volume(&mut self, volume: f32) {
        if let Some(m) = &mut self.music {
            m.volume = volume;
        }
    }

    pub fn play(&mut self, name: &'static str, samples: &Arc<[f32]>, gain: f32, pan: f32) {
        if gain <= 0.001 {
            return;
        }
        // Replace the oldest of the same sound, or of any once full.
        let same: Vec<usize> = (0..self.voices.len()).filter(|&i| self.voices[i].name == name).collect();
        if same.len() >= MAX_SAME {
            let oldest = same.into_iter().max_by_key(|&i| self.voices[i].at).unwrap();
            self.voices.swap_remove(oldest);
        } else if self.voices.len() >= MAX_VOICES {
            let oldest = (0..self.voices.len()).max_by_key(|&i| self.voices[i].at).unwrap();
            self.voices.swap_remove(oldest);
        }
        // Equal-power panning.
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        self.voices.push(Voice { samples: samples.clone(), at: 0, left: gain * angle.cos(), right: gain * angle.sin(), name });
    }

    /// Fills interleaved stereo samples.
    pub fn mix(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        if let Some(m) = self.music.as_mut().filter(|_| !self.music_held) {
            m.mix(out);
        }
        for v in &mut self.voices {
            for frame in out.as_chunks_mut::<2>().0 {
                let Some(&s) = v.samples.get(v.at) else { break };
                frame[0] += s * v.left;
                frame[1] += s * v.right;
                v.at += 1;
            }
        }
        self.voices.retain(|v| v.at < v.samples.len());
        // When much plays at once, turned down just enough to stay under the
        // ceiling, both sides alike, and back up slowly; a mix under it comes
        // out as it is, 1.5 ms later. (It went through tanh, which bent
        // anything loud a little, 0.5 out as 0.46, and held no ceiling.)
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

impl sdl3::audio::AudioCallback<f32> for Feed {
    fn callback(&mut self, stream: &mut sdl3::audio::AudioStream, requested: i32) {
        self.buf.resize(requested.max(0) as usize & !1, 0.0);
        match self.mixer.lock() {
            Ok(mut mixer) if !self.held.load(Ordering::Relaxed) => mixer.mix(&mut self.buf),
            _ => self.buf.fill(0.0),
        }
        let _ = stream.put_data_f32(&self.buf);
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
    let feed = Feed { mixer: mixer.clone(), held: held.clone(), buf: Vec::new() };
    let Some(_one) = may_open() else {
        eprintln!("no sound: the speaker opened before is still closing");
        return None;
    };
    let audio = sdl.audio().map_err(|e| anyhow::anyhow!("initialising SDL audio: {e}"));
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
        let dir = std::env::temp_dir().join(format!("voxcade-bank-{}", std::process::id()));
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
        assert!(format!("{:#}", sound::read_folder(&files).unwrap_err()).contains("Voxcade plays none"));
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

    /// A sound on the left of the view plays louder on the left.
    #[test]
    fn sounds_are_placed_in_stereo() {
        let (gain, pan) = place(Vec3::new(-50.0, 0.0, 0.0), Vec3::ZERO, 200.0);
        let mut mixer = Mixer::default();
        mixer.play("shot", &Arc::from(vec![0.5f32; 100]), gain, pan);
        let mut out = vec![0.0; 200];
        mixer.mix(&mut out);
        let at = mixer.latency() * 2;
        assert!(out[at] > out[at + 1] * 1.5, "left {} right {}", out[at], out[at + 1]);
        let (far, _) = place(Vec3::new(400.0, 0.0, 0.0), Vec3::ZERO, 200.0);
        assert!(far < gain * 0.3, "far away is not much quieter ({far} vs {gain})");
    }
}

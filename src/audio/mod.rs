//! Sound: what the simulation's events sound like, and the mixer that plays
//! them. Every sound is either synthesized from a few numbers (`Synth`) or
//! a `.wav` recording, which can be crushed to early-90s quality. The
//! engine has a sound for every event; a game's `[sounds]` replaces any of
//! them. Sounds are placed in stereo by where they happen on screen.

pub mod limiter;
pub mod loudness;
pub mod music;
pub mod synth;
pub mod wav;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use glam::Vec3;
use serde::{Deserialize, Serialize};

pub use synth::{SAMPLE_RATE, Synth, Wave};

/// The engine's own sound events (the simulation's core reports them): a
/// shot chipping the world, an explosion, a piece of the world coming down.
pub fn core_sounds() -> Vec<(&'static str, Synth)> {
    ["impact", "explosion", "crumble"].into_iter().map(|name| (name, core_sound(name))).collect()
}

/// How an engine event sounds unless a game says otherwise.
pub fn core_sound(name: &str) -> Synth {
    use Wave::*;
    let s = Synth::default();
    match name {
        "impact" => Synth { wave: Noise, freq: 5000.0, freq_end: Some(1500.0), sustain: 0.01, decay: 0.09, lowpass: Some(4000.0), lowpass_end: Some(900.0), volume: 0.22, ..s },
        // A thump and a long rumble.
        "explosion" => Synth { wave: Noise, freq: 1400.0, freq_end: Some(180.0), sustain: 0.08, decay: 1.1, punch: 0.9, lowpass: Some(2200.0), lowpass_end: Some(140.0), volume: 0.95, ..s },
        "crumble" => Synth { wave: Noise, freq: 700.0, freq_end: Some(120.0), attack: 0.03, sustain: 0.25, decay: 0.7, lowpass: Some(1100.0), lowpass_end: Some(160.0), volume: 0.6, ..s },
        _ => s,
    }
}

/// How a game says what an event sounds like.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum SoundDef {
    /// A `.wav` file in the game folder, or "none" for silence.
    File(String),
    /// A recording, made lo-fi if asked: down to `rate` Hz and `bits` bits
    /// (7000 and 8 sound like the speech in Wolfenstein 3D).
    Sample(Sample),
    Synth(Synth),
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub file: String,
    #[serde(default)]
    pub rate: Option<u32>,
    #[serde(default)]
    pub bits: Option<u32>,
    #[serde(default = "full")]
    pub volume: f32,
}

fn full() -> f32 {
    1.0
}

/// The sound of every event, ready to play.
#[derive(Default)]
pub struct SoundBank {
    sounds: HashMap<&'static str, Arc<[f32]>>,
    /// Files the sounds came from (for hot reload).
    pub files: Vec<PathBuf>,
}

impl SoundBank {
    /// The engine's sounds with the game's on top. Problems with a game's
    /// sound are reported and that event keeps the engine's sound.
    /// The game's sounds: each of its events (`defaults`, by name, with
    /// the sound it makes unless `c.sounds` says otherwise in `defs`).
    pub fn load(files: &crate::files::Files, defs: &std::collections::BTreeMap<String, SoundDef>, defaults: &[(&'static str, Synth)]) -> Self {
        let mut bank = SoundBank::default();
        for (name, _) in defs.iter().filter(|(name, _)| !defaults.iter().any(|(n, _)| n == name)) {
            let known: Vec<_> = defaults.iter().map(|(n, _)| *n).collect();
            eprintln!("warning: c.sounds.{name}: no such event (events: {})", known.join(", "));
        }
        for (name, synth) in defaults {
            let samples = match defs.get(*name) {
                None => synth.render(),
                Some(def) => match bank.render(files, def) {
                    Ok(samples) => samples,
                    Err(e) => {
                        eprintln!("warning: c.sounds.{name}: {e:#}");
                        synth.render()
                    }
                },
            };
            if !samples.is_empty() {
                bank.sounds.insert(name, samples.into());
            }
        }
        bank
    }

    fn render(&mut self, files: &crate::files::Files, def: &SoundDef) -> anyhow::Result<Vec<f32>> {
        Ok(match def {
            SoundDef::File(file) if file == "none" => Vec::new(),
            SoundDef::File(file) => {
                let path = files.path(file);
                self.files.push(path.clone());
                wav::load(files, &path)?
            }
            SoundDef::Sample(s) => {
                let path = files.path(&s.file);
                self.files.push(path.clone());
                let samples = wav::crush(&wav::load(files, &path)?, s.rate, s.bits);
                samples.into_iter().map(|x| x * s.volume).collect()
            }
            SoundDef::Synth(synth) => synth.render(),
        })
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
            let _ = std::thread::Builder::new().name("audio close".into()).spawn(move || drop(stream));
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

    /// Every event has a sound of its own, and none is silent or too loud.
    #[test]
    fn every_event_has_a_sound() {
        let defaults = crate::shooter::sounds();
        let bank = SoundBank::load(&crate::files::Files::folder(std::path::Path::new(".")), &Default::default(), &defaults);
        for (name, _) in &defaults {
            let samples = bank.get(name).unwrap_or_else(|| panic!("{name} has no sound"));
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.05 && peak <= 1.0, "{name} peaks at {peak}");
        }
    }

    /// `c.sounds` names a sound as a file, as a lo-fi recording, or by synth numbers.
    #[test]
    fn sounds_can_be_files_samples_or_synths() {
        let cfg: crate::config::GameConfig = toml::from_str(
            r#"
            [sounds]
            explosion = "sfx/boom.wav"
            down = { file = "sfx/mein_leben.wav", rate = 7000, bits = 8 }
            shot = { wave = "noise", freq = 9000, freq_end = 1500, decay = 0.14 }
            jump = "none"
            "#,
        )
        .unwrap();
        assert!(matches!(&cfg.sounds["explosion"], SoundDef::File(f) if f == "sfx/boom.wav"));
        assert!(matches!(&cfg.sounds["down"], SoundDef::Sample(s) if s.rate == Some(7000) && s.bits == Some(8)));
        assert!(matches!(&cfg.sounds["shot"], SoundDef::Synth(s) if s.wave == Wave::Noise && s.freq_end == Some(1500.0)));
        let bank = SoundBank::load(&crate::files::Files::folder(std::path::Path::new(".")), &cfg.sounds, &crate::shooter::sounds());
        assert!(bank.get("jump").is_none(), "'none' should be silent");
        assert!(bank.get("explosion").is_some(), "a missing file falls back to the game's own sound");
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

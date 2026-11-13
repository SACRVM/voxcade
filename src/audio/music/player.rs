//! The player: renders a song's voices, sidechain, layers, filter and the game's mix.

use super::mix::{Mix, SHORTEST, muffle_cutoff};
use super::song::{FilterMode, Song};
use super::{SAMPLE_RATE, Wave};

/// One channel's voice: the note it plays and where the oscillator is.
#[derive(Default)]
pub(super) struct Voice {
    pub(super) next: usize,
    pub(super) note: Option<usize>,
    pub(super) started: u64,
    pub(super) phase: f32,
    pub(super) noise: f32,
    pub(super) seed: u32,
    pub(super) low: f32,
    pub(super) filter: Svf,
    /// Envelope level while the key is held, where the release starts from.
    pub(super) released_at: f32,
}

/// A state-variable filter (the SID's kind: low, band and high pass from
/// one 12 dB/octave stage), in the zero-delay form that stays stable
/// however fast the cutoff moves.
#[derive(Default)]
pub(super) struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    fn process(&mut self, x: f32, cutoff: f32, resonance: f32, mode: FilterMode) -> f32 {
        let sr = SAMPLE_RATE as f32;
        let g = (std::f32::consts::PI * cutoff.clamp(20.0, sr * 0.45) / sr).tan();
        // Damping: Butterworth (no peak) at 0, a ringing peak near 1.
        let k = (std::f32::consts::SQRT_2 * (1.0 - resonance)).max(0.1);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let y = match mode {
            FilterMode::Low => v2,
            FilterMode::Band => v1,
            FilterMode::High => x - k * v1 - v2,
        };
        // Overdrive softly past full scale, so resonance growls instead of spiking.
        if y.abs() <= 1.0 { y } else { y.signum() * (1.0 + (y.abs() - 1.0).tanh()) }
    }
}

/// The ducking the last sidechain note started: since when, how deep, how long.
#[derive(Clone, Copy)]
pub(super) struct Duck {
    pub(super) from: u64,
    pub(super) depth: f32,
    pub(super) release: f32,
}

impl Duck {
    /// How far the other channels are down `now`, 0..depth: down within a
    /// few milliseconds, back up along a smooth curve.
    fn amount(&self, now: u64) -> f32 {
        const ATTACK: f32 = 0.004;
        let t = now.saturating_sub(self.from) as f32 / SAMPLE_RATE as f32;
        if t < ATTACK {
            return self.depth * t / ATTACK;
        }
        let r = ((t - ATTACK) / self.release.max(1e-3)).min(1.0);
        self.depth * 0.5 * (1.0 + (std::f32::consts::PI * r).cos())
    }
}

/// A jump this far off (samples, 50 ms) when keeping time with a game;
/// less is caught up by going a little faster or slower.
const JUMP: i64 = SAMPLE_RATE as i64 / 20;

/// Samples a jump fades in over (5 ms), so it does not click.
const FADE_IN: u32 = SAMPLE_RATE / 200;

/// A value going straight to where it is asked, a step a sample.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ramp {
    pub(super) now: f32,
    pub(super) to: f32,
    pub(super) step: f32,
}

impl Ramp {
    fn at(v: f32) -> Self {
        Self { now: v, to: v, step: 0.0 }
    }

    /// Towards `to` over `seconds` (`shortest` at least).
    fn toward(&mut self, to: f32, seconds: f32, shortest: f32) {
        let samples = (seconds.max(shortest) * SAMPLE_RATE as f32).max(1.0);
        (self.to, self.step) = (to, (to - self.now) / samples);
    }

    /// The value for this sample, and a step on.
    fn next(&mut self) -> f32 {
        let now = self.now;
        if self.now != self.to {
            let n = self.now + self.step;
            self.now = if self.step == 0.0 || (self.step > 0.0) == (n >= self.to) { self.to } else { n };
        }
        now
    }
}

/// The game's say over its song (`Mix`) as the player applies it: the
/// loudness and the muffle each gliding sample by sample, and a low-pass
/// of 12 dB an octave (the zero-delay state-variable filter, Butterworth:
/// stable and without zipper noise however it glides) on each side.
#[derive(Clone, Debug)]
pub(super) struct Bus {
    pub(super) level: Ramp,
    pub(super) muffle: Ramp,
    /// The asks last followed (`Glide::asked`), the volume's and the muffle's.
    pub(super) asked: (u32, u32),
    /// Each side's filter state, and its coefficient for the muffle it was
    /// last worked out for.
    pub(super) low: [(f32, f32); 2],
    pub(super) g: (f32, f32),
}

impl Default for Bus {
    fn default() -> Self {
        Self { level: Ramp::at(1.0), muffle: Ramp::at(0.0), asked: (0, 0), low: [(0.0, 0.0); 2], g: (-1.0, 0.0) }
    }
}

impl Bus {
    /// One frame of the song through it.
    fn apply(&mut self, l: f32, r: f32) -> (f32, f32) {
        let level = self.level.next();
        let amount = self.muffle.next();
        let (l, r) = (l * level, r * level);
        if self.g.0 != amount {
            let cutoff = muffle_cutoff(amount).min(SAMPLE_RATE as f32 * 0.45);
            self.g = (amount, (std::f32::consts::PI * cutoff / SAMPLE_RATE as f32).tan());
        }
        // Always run, so its state is the song's when a muffle begins.
        let g = self.g.1;
        let k = std::f32::consts::SQRT_2;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let (a2, a3) = (g * a1, g * g * a1);
        let low = |x: f32, s: &mut (f32, f32)| {
            let v3 = x - s.1;
            let v1 = a1 * s.0 + a2 * v3;
            let v2 = s.1 + a2 * s.0 + a3 * v3;
            *s = (2.0 * v1 - s.0, 2.0 * v2 - s.1);
            v2
        };
        let [sl, sr] = &mut self.low;
        let (wl, wr) = (low(l, sl), low(r, sr));
        if amount <= 0.0 {
            return (l, r);
        }
        // Blended in over the first quarter, so a muffle begins without a step.
        let wet = (amount * 4.0).min(1.0);
        (l + (wl - l) * wet, r + (wr - r) * wet)
    }
}

/// Plays a song, sample by sample; runs in the audio thread.
pub struct Player {
    pub(super) song: Song,
    pub(super) voices: Vec<Voice>,
    pub(super) time: u64,
    pub(super) duck: Option<Duck>,
    pub volume: f32,
    /// How fast its time goes against the samples (1: as written), nudged
    /// to keep with a game that reads its beats (`follow`); what is left
    /// of a sample.
    pub(super) rate: f64,
    pub(super) carry: f64,
    /// Samples since a jump, fading in up to `FADE_IN`.
    pub(super) since_jump: u32,
    /// The game's tension (`music.intensity`), and how loud each channel's
    /// layer is now (0..1, moving a bar's worth a bar).
    pub intensity: f32,
    pub(super) loud: Vec<f32>,
    /// The game's say over how it sounds (`Mix`): neutral unless followed.
    pub(super) bus: Bus,
}

impl Player {
    pub fn title(&self) -> &str {
        &self.song.title
    }

    pub fn new(song: Song, volume: f32) -> Self {
        let voices = (0..song.channels.len()).map(|i| Voice { seed: 0x1234_5678 ^ (i as u32 * 7919), ..Voice::default() }).collect();
        let loud = song.layers.iter().map(|_| 1.0).collect();
        Self { song, voices, time: 0, duck: None, volume, rate: 1.0, carry: 0.0, since_jump: FADE_IN, intensity: 1.0, loud, bus: Bus::default() }
    }

    /// Sounds as the game's `mix` is now, gliding on where it still glides
    /// (a song begun, or the game brought back).
    pub fn start_mix(&mut self, mix: &Mix) {
        self.bus.level = Ramp::at(mix.volume.now());
        self.bus.muffle = Ramp::at(mix.muffle.now());
        self.bus.asked = (mix.volume.asked, mix.muffle.asked);
        self.bus.level.toward(mix.volume.to, mix.volume.left(), SHORTEST.0);
        self.bus.muffle.toward(mix.muffle.to, mix.muffle.left(), SHORTEST.1);
    }

    /// Follows the game's `mix`: a new ask glides from where the song
    /// sounds now to where it was asked, over what is left of its time.
    pub fn follow_mix(&mut self, mix: &Mix) {
        if mix.volume.asked != self.bus.asked.0 {
            self.bus.level.toward(mix.volume.to, mix.volume.left(), SHORTEST.0);
        }
        if mix.muffle.asked != self.bus.asked.1 {
            self.bus.muffle.toward(mix.muffle.to, mix.muffle.left(), SHORTEST.1);
        }
        self.bus.asked = (mix.volume.asked, mix.muffle.asked);
    }

    /// Sets how loud each layer is at once, for the tension it starts at
    /// (a song begun while the game is calm starts calm).
    pub fn start_at(&mut self, intensity: f32) {
        self.intensity = intensity;
        for (loud, &from) in self.loud.iter_mut().zip(&self.song.layers) {
            *loud = if intensity >= from { 1.0 } else { 0.0 };
        }
    }

    /// Samples into the song a beat count is (from its start, round its
    /// loop as it plays).
    pub fn at_beat(&self, beats: f64) -> u64 {
        (beats * 60.0 / self.song.tempo as f64 * SAMPLE_RATE as f64) as u64
    }

    /// Jumps to `at` samples into the song (round its loop), each voice
    /// taking up the note that sounds there, faded in so it does not click.
    pub fn seek(&mut self, at: u64) {
        let at = if self.song.looping { at % self.song.length } else { at.min(self.song.length) };
        (self.time, self.carry, self.duck, self.since_jump) = (at, 0.0, None, 0);
        for (ch, v) in self.voices.iter_mut().enumerate() {
            let notes = &self.song.channels[ch];
            v.next = notes.partition_point(|n| n.start <= at);
            v.note = v.next.checked_sub(1).filter(|&i| notes[i].freq.is_some());
            if let Some(i) = v.note {
                (v.started, v.phase) = (notes[i].start, 0.0);
            }
        }
    }

    /// Keeps time with a game's clock, `target` samples into the song: a
    /// jump when it is far off (after a pause), else a little faster or
    /// slower until it is with it again, unheard.
    pub fn follow(&mut self, target: u64) {
        let length = self.song.length.max(1) as i64;
        let target = if self.song.looping { target % self.song.length } else { target.min(self.song.length) };
        let mut off = target as i64 - self.time as i64;
        if self.song.looping {
            // The shorter way round the loop.
            if off > length / 2 {
                off -= length;
            } else if off < -length / 2 {
                off += length;
            }
        }
        if off.abs() > JUMP {
            self.seek(target);
            self.rate = 1.0;
        } else {
            self.rate = 1.0 + (off as f64 / (4.0 * SAMPLE_RATE as f64)).clamp(-0.003, 0.003);
        }
    }

    /// Whether a song that plays once has ended.
    pub fn over(&self) -> bool {
        !self.song.looping && self.time >= self.song.length
    }

    /// Adds the next frames to interleaved stereo `out`.
    pub fn mix(&mut self, out: &mut [f32]) {
        let sr = SAMPLE_RATE as f32;
        for frame in out.as_chunks_mut::<2>().0 {
            if self.time >= self.song.length {
                if !self.song.looping {
                    return;
                }
                self.time = 0;
                self.duck = None;
                for v in &mut self.voices {
                    v.next = 0;
                    v.note = None;
                }
            }
            let ducked = self.duck.map_or(0.0, |d| d.amount(self.time));
            // The layers move towards the tension, a bar's worth a bar.
            let step = self.song.tempo / (60.0 * SAMPLE_RATE as f32 * self.song.meter as f32);
            for (loud, &from) in self.loud.iter_mut().zip(&self.song.layers) {
                let want = if self.intensity >= from { 1.0 } else { 0.0 };
                *loud = if *loud < want { (*loud + step).min(want) } else { (*loud - step).max(want) };
            }
            let mut sum = [0.0f32; 2];
            for (ch, v) in self.voices.iter_mut().enumerate() {
                let notes = &self.song.channels[ch];
                while v.next < notes.len() && notes[v.next].start <= self.time {
                    // Rests and the end marker have no pitch: the note before rings out.
                    if notes[v.next].freq.is_some() {
                        v.note = Some(v.next);
                        v.started = notes[v.next].start;
                        v.phase = 0.0;
                        let ins = &self.song.instruments[notes[v.next].instrument];
                        if ins.sidechain > 0.0 {
                            self.duck = Some(Duck { from: self.time, depth: ins.sidechain, release: ins.sidechain_release });
                        }
                    }
                    v.next += 1;
                }
                let Some(index) = v.note else { continue };
                let n = &notes[index];
                let ins = &self.song.instruments[n.instrument];
                let t = (self.time - v.started) as f32 / sr;
                let held = n.held as f32 / sr;

                // ADSR.
                let level = if t < held {
                    let l = if t < ins.attack {
                        t / ins.attack.max(1e-4)
                    } else {
                        let d = (t - ins.attack) / ins.decay.max(1e-4);
                        if d < 1.0 { 1.0 + (ins.sustain - 1.0) * d } else { ins.sustain }
                    };
                    v.released_at = l;
                    l
                } else {
                    let r = (t - held) / ins.release.max(1e-4);
                    if r >= 1.0 {
                        v.note = None;
                        continue;
                    }
                    v.released_at * (1.0 - r)
                };

                // Pitch: the note (or a fixed one), a sweep, the arpeggio, vibrato.
                let mut freq = ins.freq.unwrap_or(n.freq.unwrap_or(0.0));
                if ins.sweep != 0.0 {
                    let span = ins.sweep_time.unwrap_or(ins.attack + ins.decay).max(1e-3);
                    freq *= 2f32.powf(ins.sweep * (t / span).min(1.0) / 12.0);
                }
                if !ins.arp.is_empty() {
                    let step = (t * ins.arp_speed) as usize % ins.arp.len();
                    freq *= 2f32.powf(ins.arp[step] / 12.0);
                }
                if ins.vibrato > 0.0 && t > ins.vibrato_delay {
                    freq *= 1.0 + ins.vibrato * (std::f32::consts::TAU * ins.vibrato_speed * (t - ins.vibrato_delay)).sin();
                }
                v.phase += freq / sr;
                let wrapped = v.phase >= 1.0;
                v.phase = v.phase.fract();
                let duty = (ins.duty + ins.pwm * (std::f32::consts::TAU * ins.pwm_speed * t).sin()).clamp(0.05, 0.95);
                let mut s = match ins.wave {
                    // A pulse off duty 0.5 is high longer than low: taken off
                    // its middle so it swings round zero (as in `synth`).
                    Wave::Square => (if v.phase < duty { 1.0 } else { -1.0 }) - (2.0 * duty - 1.0),
                    Wave::Saw => 2.0 * v.phase - 1.0,
                    Wave::Triangle => 1.0 - 4.0 * (v.phase - 0.5).abs(),
                    Wave::Sine => (std::f32::consts::TAU * v.phase).sin(),
                    Wave::Noise => {
                        if wrapped {
                            v.seed ^= v.seed << 13;
                            v.seed ^= v.seed >> 17;
                            v.seed ^= v.seed << 5;
                            v.noise = v.seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
                        }
                        v.noise
                    }
                };
                if let Some(cut) = ins.lowpass {
                    let alpha = 1.0 - (-std::f32::consts::TAU * cut / sr).exp();
                    v.low += alpha * (s - v.low);
                    s = v.low;
                }
                if let Some(mode) = ins.filter {
                    // The cutoff swept per note, evenly in octaves.
                    let mut cutoff = ins.cutoff;
                    if let Some(end) = ins.cutoff_end {
                        let span = ins.cutoff_time.unwrap_or(ins.attack + ins.decay).max(1e-3);
                        cutoff *= (end / ins.cutoff).powf((t / span).min(1.0));
                    }
                    s = v.filter.process(s, cutoff, ins.resonance, mode);
                }
                // Ducked under a sidechain note, unless this is one.
                let duck = if ins.sidechain > 0.0 { 1.0 } else { 1.0 - ducked * ins.duck };
                let fade = if self.since_jump < FADE_IN { self.since_jump as f32 / FADE_IN as f32 } else { 1.0 };
                let gain = s * level * ins.volume * n.volume * self.volume * duck * fade * self.loud[ch];
                let angle = (n.pan + 1.0) * std::f32::consts::FRAC_PI_4;
                sum[0] += gain * angle.cos() * std::f32::consts::SQRT_2;
                sum[1] += gain * angle.sin() * std::f32::consts::SQRT_2;
            }
            // As loud and as clear as the game asked (`Mix`).
            let (l, r) = self.bus.apply(sum[0], sum[1]);
            frame[0] += l;
            frame[1] += r;
            self.since_jump = self.since_jump.saturating_add(1);
            // A sample on, or a hair more or less when keeping time.
            self.carry += self.rate;
            let on = self.carry as u64;
            self.carry -= on as f64;
            self.time += on;
        }
    }

    /// The whole song (once) as interleaved stereo samples.
    pub fn render(mut self, seconds: Option<f32>) -> Vec<f32> {
        let frames = match seconds {
            Some(s) => (s * SAMPLE_RATE as f32) as u64,
            None => self.song.length,
        };
        let mut out = vec![0.0; frames as usize * 2];
        self.mix(&mut out);
        out
    }
}

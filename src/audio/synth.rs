//! A small sound synthesizer in the spirit of sfxr: one oscillator (square,
//! saw, sine, triangle or noise), a pitch that glides, an envelope with a
//! punch, vibrato, one jump in pitch, and a low-pass filter. A handful of
//! numbers make any arcade effect, and they live in `conf.lua` (`c.sounds`).

use serde::{Deserialize, Serialize};

pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    Square,
    Saw,
    Sine,
    Triangle,
    /// Random values held for one period of `freq`: higher sounds brighter.
    Noise,
}

/// One synthesized sound. Times in seconds, frequencies in Hz.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Synth {
    pub wave: Wave,
    /// Pitch at the start, and at the end if it glides there.
    pub freq: f32,
    pub freq_end: Option<f32>,
    /// Share of a square wave's period that is high (0.5 is hollow, small
    /// values thin and nasal).
    pub duty: f32,
    /// Fade in, hold, fade out.
    pub attack: f32,
    pub sustain: f32,
    pub decay: f32,
    /// Extra loudness at the start of the hold that falls away, 0..1.
    pub punch: f32,
    /// Pitch wobble: depth (share of the pitch) and speed (Hz).
    pub vibrato: f32,
    pub vibrato_speed: f32,
    /// After `arp_at` seconds the pitch jumps by this factor (coins, jingles).
    pub arp: f32,
    pub arp_at: f32,
    /// Low-pass cutoff at the start and at the end (rumble, muffling).
    pub lowpass: Option<f32>,
    pub lowpass_end: Option<f32>,
    pub volume: f32,
}

impl Default for Synth {
    fn default() -> Self {
        Self {
            wave: Wave::Square,
            freq: 440.0,
            freq_end: None,
            duty: 0.5,
            attack: 0.0,
            sustain: 0.1,
            decay: 0.2,
            punch: 0.0,
            vibrato: 0.0,
            vibrato_speed: 6.0,
            arp: 1.0,
            arp_at: 0.0,
            lowpass: None,
            lowpass_end: None,
            volume: 0.5,
        }
    }
}

impl Synth {
    /// The sound as mono samples at SAMPLE_RATE. Always the same samples for
    /// the same numbers.
    pub fn render(&self) -> Vec<f32> {
        let sr = SAMPLE_RATE as f32;
        let attack = self.attack.max(0.0);
        let sustain = self.sustain.max(0.0);
        let decay = self.decay.max(0.001);
        let total = attack + sustain + decay;
        let count = (total * sr).ceil() as usize;
        let mut out = Vec::with_capacity(count);
        let mut phase = 0.0f32;
        let mut held = 0.0f32;
        let mut seed = 0x9E37_79B9u32;
        let mut low = 0.0f32;
        for i in 0..count {
            let t = i as f32 / sr;
            let u = t / total;
            let mut freq = match self.freq_end {
                Some(end) if end > 0.0 && self.freq > 0.0 => self.freq * (end / self.freq).powf(u),
                _ => self.freq,
            };
            if self.arp != 1.0 && t >= self.arp_at {
                freq *= self.arp;
            }
            freq *= 1.0 + self.vibrato * (std::f32::consts::TAU * self.vibrato_speed * t).sin();
            phase += freq.max(0.0) / sr;
            let wrapped = phase >= 1.0;
            phase = phase.fract();
            let mut s = match self.wave {
                Wave::Square => {
                    if phase < self.duty.clamp(0.01, 0.99) {
                        1.0
                    } else {
                        -1.0
                    }
                }
                Wave::Saw => 2.0 * phase - 1.0,
                Wave::Sine => (std::f32::consts::TAU * phase).sin(),
                Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
                Wave::Noise => {
                    if wrapped || i == 0 {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        held = seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
                    }
                    held
                }
            };
            if let Some(cut) = self.lowpass {
                let cut = match self.lowpass_end {
                    Some(end) if end > 0.0 && cut > 0.0 => cut * (end / cut).powf(u),
                    _ => cut,
                };
                let alpha = 1.0 - (-std::f32::consts::TAU * cut / sr).exp();
                low += alpha * (s - low);
                s = low;
            }
            let envelope = if t < attack {
                t / attack
            } else if t < attack + sustain {
                1.0 + self.punch * (1.0 - (t - attack) / sustain.max(0.001))
            } else {
                let x = ((t - attack - sustain) / decay).min(1.0);
                (1.0 - x) * (1.0 - x)
            };
            out.push(s * envelope * self.volume);
        }
        // A punch can push the start past full scale; bring it back down.
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        if peak > 1.0 {
            out.iter_mut().for_each(|s| *s /= peak);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound lasts attack + sustain + decay, is not silent and stays in
    /// range, for every wave.
    #[test]
    fn a_synth_makes_a_sound_of_its_length() {
        for wave in [Wave::Square, Wave::Saw, Wave::Sine, Wave::Triangle, Wave::Noise] {
            let synth = Synth { wave, freq_end: Some(110.0), punch: 0.5, lowpass: Some(3000.0), ..Synth::default() };
            let samples = synth.render();
            assert_eq!(samples.len(), (0.3 * SAMPLE_RATE as f32).ceil() as usize, "{wave:?}");
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            assert!(peak <= 1.0, "{wave:?} peaks at {peak}");
            assert!(rms > 0.02, "{wave:?} is nearly silent ({rms})");
            assert_eq!(samples, synth.render(), "{wave:?} differs between renders");
        }
    }
}

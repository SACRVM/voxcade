//! A small sound synthesizer in the spirit of sfxr: one oscillator (square,
//! saw, sine, triangle or noise), a pitch that slides, an envelope with a
//! punch, vibrato, one jump in pitch, and a low-pass filter. A handful of
//! numbers make any arcade effect; a game writes them in a file of its own,
//! `sounds/NAME.toml` (`audio::sound`), in the words of `audio::words`.

use serde::{Deserialize, Serialize};

pub const SAMPLE_RATE: u32 = 44_100;

/// The longest a sound lasts, in seconds: a sound is an effect, longer is
/// music (`c.audio.music`). A file may be made by a dice or an agent
/// without anyone listening first; this keeps one from filling memory.
pub const LONGEST: f32 = 10.0;

/// The shortest fade at a sound's end, in seconds: a shorter decay would
/// stop the wave where it stands, a click.
pub const SHORTEST_DECAY: f32 = 0.005;

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

impl Wave {
    pub const ALL: [(&'static str, Wave); 5] = [("square", Wave::Square), ("saw", Wave::Saw), ("triangle", Wave::Triangle), ("sine", Wave::Sine), ("noise", Wave::Noise)];

    /// The wave's word (`wave = "square"`).
    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(_, w)| *w == self).map(|(n, _)| *n).expect("every wave has a word")
    }
}

/// The oscillator of a sound and of a song's voice alike: where it is in
/// its wave's turn (0 to 1), and the noise's generator and the value it
/// holds for a turn.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Oscillator {
    pub(crate) phase: f32,
    seed: u32,
    held: f32,
}

impl Oscillator {
    pub(crate) fn new(seed: u32) -> Self {
        Self { phase: 0.0, seed, held: 0.0 }
    }

    /// The next sample of `wave`, `step` of a turn on (its frequency over
    /// the sample rate). A pulse off duty 0.5 is high longer than low (or
    /// the other way): its middle lies off zero, a DC offset that thumps
    /// and takes headroom, taken off exactly so it swings round zero
    /// (nothing changes at 0.5). Noise draws a new value each turn, and
    /// where `draw`.
    pub(crate) fn next(&mut self, wave: Wave, step: f32, duty: f32, draw: bool) -> f32 {
        self.phase += step;
        let wrapped = self.phase >= 1.0;
        self.phase = self.phase.fract();
        match wave {
            Wave::Square => (if self.phase < duty { 1.0 } else { -1.0 }) - (2.0 * duty - 1.0),
            Wave::Saw => 2.0 * self.phase - 1.0,
            Wave::Sine => (std::f32::consts::TAU * self.phase).sin(),
            Wave::Triangle => 1.0 - 4.0 * (self.phase - 0.5).abs(),
            Wave::Noise => {
                if wrapped || draw {
                    self.seed ^= self.seed << 13;
                    self.seed ^= self.seed >> 17;
                    self.seed ^= self.seed << 5;
                    self.held = self.seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
                }
                self.held
            }
        }
    }
}

/// The gentle low-pass of a sound and of a song's instrument: one pole,
/// 6 dB an octave.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OnePole {
    low: f32,
}

impl OnePole {
    /// How far a sample moves it for a cutoff at `cut` Hz.
    pub(crate) fn alpha(cut: f32) -> f32 {
        1.0 - (-std::f32::consts::TAU * cut / SAMPLE_RATE as f32).exp()
    }

    /// `s` through it, moving `alpha` of the way.
    pub(crate) fn next(&mut self, s: f32, alpha: f32) -> f32 {
        self.low += alpha * (s - self.low);
        self.low
    }
}

/// One synthesized sound. Times in seconds, frequencies in Hz, pitch
/// changes in semitones (the words of `audio::words`).
#[derive(Clone, Debug, PartialEq)]
pub struct Synth {
    pub wave: Wave,
    /// Pitch at the start.
    pub freq: f32,
    /// Semitones the pitch slides over the whole sound (-12: an octave
    /// down), evenly in semitones. Kept as written (f64), so the end pitch
    /// comes out exactly where an older end frequency put it.
    pub sweep: f64,
    /// Share of a square wave's period that is high (0.5 is hollow, small
    /// values thin and nasal).
    pub duty: f32,
    /// Fade in, hold at full volume, fade out to silence.
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    /// Extra loudness at the start of the hold that falls away, 0..1.
    pub punch: f32,
    /// Pitch wobble: depth (share of the pitch) and speed (Hz).
    pub vibrato: f32,
    pub vibrato_speed: f32,
    /// After `jump_at` seconds the pitch jumps by this many semitones, once
    /// (coins, jingles).
    pub jump: f64,
    pub jump_at: f32,
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
            sweep: 0.0,
            duty: 0.5,
            attack: 0.0,
            hold: 0.1,
            decay: 0.2,
            punch: 0.0,
            vibrato: 0.0,
            vibrato_speed: 6.0,
            jump: 0.0,
            jump_at: 0.0,
            lowpass: None,
            lowpass_end: None,
            volume: 0.5,
        }
    }
}

/// A pitch `semitones` above `freq` (below if negative), in Hz.
pub fn shifted(freq: f32, semitones: f64) -> f32 {
    (freq as f64 * 2f64.powf(semitones / 12.0)) as f32
}

/// The frequency ratio of a step of `semitones` (12: 2, an octave).
pub fn ratio(semitones: f64) -> f32 {
    2f64.powf(semitones / 12.0) as f32
}

/// The semitones from `from` to `to` Hz (or of a ratio, `from` 1).
pub fn semitones(from: f64, to: f64) -> f64 {
    12.0 * (to / from).log2()
}

impl Synth {
    /// How long it lasts: attack, hold and decay (a decay of at least
    /// `SHORTEST_DECAY`).
    pub fn length(&self) -> f32 {
        self.attack.max(0.0) + self.hold.max(0.0) + self.decay.max(SHORTEST_DECAY)
    }

    /// The pitch at the end of the slide, in Hz.
    pub fn freq_end(&self) -> f32 {
        shifted(self.freq, self.sweep)
    }

    /// The sound as mono samples at SAMPLE_RATE. Always the same samples for
    /// the same numbers.
    pub fn render(&self) -> Vec<f32> {
        let sr = SAMPLE_RATE as f32;
        let attack = self.attack.max(0.0);
        let hold = self.hold.max(0.0);
        let decay = self.decay.max(SHORTEST_DECAY);
        let total = (attack + hold + decay).min(LONGEST);
        let count = (total * sr).ceil() as usize;
        let mut out = Vec::with_capacity(count);
        let mut osc = Oscillator::new(0x9E37_79B9);
        let mut low = OnePole::default();
        // The file speaks semitones; the oscillator goes in Hz and ratios,
        // computed once (the end pitch exactly as an end frequency in Hz
        // gave it before).
        let end = (self.sweep != 0.0).then(|| self.freq_end());
        let jump = ratio(self.jump);
        let duty = self.duty.clamp(0.01, 0.99);
        for i in 0..count {
            let t = i as f32 / sr;
            let u = t / total;
            let mut freq = match end {
                Some(end) if end > 0.0 && self.freq > 0.0 => self.freq * (end / self.freq).powf(u),
                _ => self.freq,
            };
            if self.jump != 0.0 && t >= self.jump_at {
                freq *= jump;
            }
            freq *= 1.0 + self.vibrato * (std::f32::consts::TAU * self.vibrato_speed * t).sin();
            let mut s = osc.next(self.wave, freq.max(0.0) / sr, duty, i == 0);
            if let Some(cut) = self.lowpass {
                let cut = match self.lowpass_end {
                    Some(end) if end > 0.0 && cut > 0.0 => cut * (end / cut).powf(u),
                    _ => cut,
                };
                s = low.next(s, OnePole::alpha(cut));
            }
            let envelope = if t < attack {
                t / attack
            } else if t < attack + hold {
                1.0 + self.punch * (1.0 - (t - attack) / hold.max(0.001))
            } else {
                let x = ((t - attack - hold) / decay).min(1.0);
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

    /// A sound lasts attack + hold + decay, is not silent and stays in
    /// range, for every wave.
    #[test]
    fn a_synth_makes_a_sound_of_its_length() {
        for wave in [Wave::Square, Wave::Saw, Wave::Sine, Wave::Triangle, Wave::Noise] {
            let synth = Synth { wave, sweep: -24.0, punch: 0.5, lowpass: Some(3000.0), ..Synth::default() };
            let samples = synth.render();
            assert_eq!(samples.len(), (0.3 * SAMPLE_RATE as f32).ceil() as usize, "{wave:?}");
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            assert!(peak <= 1.0, "{wave:?} peaks at {peak}");
            assert!(rms > 0.02, "{wave:?} is nearly silent ({rms})");
            assert_eq!(samples, synth.render(), "{wave:?} differs between renders");
        }
    }

    /// A pulse off 0.5 swings round zero (no DC offset to thump); at 0.5
    /// nothing is taken off. A decay too short to fade still fades over
    /// `SHORTEST_DECAY`, and no sound outlasts `LONGEST`.
    #[test]
    fn a_thin_pulse_swings_round_zero_and_every_sound_fades() {
        let mean = |s: &Synth| {
            let samples = Synth { attack: 0.0, hold: 1.0, decay: 0.005, ..s.clone() }.render();
            samples.iter().sum::<f32>() / samples.len() as f32
        };
        let thin = Synth { duty: 0.2, volume: 1.0, ..Synth::default() };
        assert!(mean(&thin).abs() < 0.002, "a thin pulse's middle {}", mean(&thin));
        let half = Synth { volume: 1.0, ..Synth::default() }.render();
        assert!(half.iter().all(|s| s.abs() <= 1.0) && half.contains(&1.0), "a pulse at 0.5 as it always was");
        let short = Synth { hold: 0.05, decay: 0.0, volume: 1.0, ..Synth::default() }.render();
        assert_eq!(short.len(), ((0.05 + SHORTEST_DECAY) * SAMPLE_RATE as f32).ceil() as usize);
        assert!(short.last().unwrap().abs() < 0.01, "it ends near silence: {}", short.last().unwrap());
        assert_eq!(Synth { hold: 30.0, ..Synth::default() }.render().len(), (LONGEST * SAMPLE_RATE as f32).ceil() as usize);
    }
}

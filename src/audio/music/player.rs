//! The player: renders a song's voices, sidechain, layers, filter and the game's mix.

use super::SAMPLE_RATE;
use super::parse::LOOP_SLACK;
use super::mix::{Glide, LETTERS, Mix, SHORTEST, letter_index, muffle_cutoff};
use super::clear::{self, Seg};
use super::song::{FilterMode, Instrument, Note, Song, Voicing};
use super::voice::{Segment, Singer};
use crate::audio::synth::{OnePole, Oscillator};

/// Channels a game laid, made on the main thread to be swapped in by the
/// audio (`Player::pend`): the song and a voice for each of its channels
/// (a channel laid before brings its own), room for the notes of channels
/// laid no more to ring out, and the beats since the song began it is due
/// at (`f64::NEG_INFINITY`: at once).
pub struct Swap {
    song: Song,
    voices: Vec<Voice>,
    tails: Vec<Voice>,
    loud: Vec<f32>,
    due: f64,
}

/// What a song plays and is to swap to (`Player::laid`), by the digests of
/// the channels a game laid (`parse::Laid`): none for a file's own.
pub type Digests = (Option<[u8; 32]>, Option<[u8; 32]>);

/// The most channels a song has: one a letter, A to Z and a to z.
const MOST_LETTERS: usize = 52;

impl Swap {
    pub fn new(song: Song, due: f64) -> Box<Swap> {
        let n = song.channels.len();
        let voices = (0..n).map(|i| Voice::new(i as u32 * 7919, song.names[i])).collect();
        let mut tails = Vec::with_capacity(n + MOST_LETTERS);
        tails.resize_with(n, Voice::default);
        Box::new(Swap { voices, tails, loud: vec![1.0; n], due, song })
    }
}

/// One channel's voice: the note it plays (its own copy, so it can ring
/// on after the song went round) and where the oscillator is.
#[derive(Clone, Default)]
pub(super) struct Voice {
    pub(super) next: usize,
    pub(super) note: Option<Note>,
    /// The sample of the song the note started at (before 0, wrapped, for
    /// a note ringing on in a round begun since).
    pub(super) started: u64,
    pub(super) osc: Oscillator,
    pub(super) low: OnePole,
    pub(super) filter: Svf,
    /// Envelope level while the key is held, where the release starts from.
    pub(super) released_at: f32,
    /// What the note it plays keeps the same from start to end: how far
    /// its instrument's low-pass moves a sample, and its pan's share of
    /// each side.
    alpha: f32,
    pan: (f32, f32),
    /// The envelope's level at the last sample, and where a voice's attack
    /// starts from (the level it was at: a sung line never jumps to
    /// silence between its syllables).
    level: f32,
    from: f32,
    /// The singing voices (`voice`, and the clear one), and the note's
    /// drive (a gain, 0 off) and bit crusher.
    pub(super) singer: Singer,
    pub(super) clear: clear::Singer,
    drive: f32,
    crush: Crusher,
    /// The drive's DC blocker: the last sample in and out.
    dc: (f32, f32),
    /// What a voice's noise is raised by, joining past the clipper.
    noise_gain: f32,
    /// Its channel's strip on the desk (`Player::lanes`, by letter): its
    /// own, so a note ringing out of a channel laid no more keeps it.
    strip: u8,
}

impl Voice {
    fn new(seed: u32, letter: char) -> Self {
        let strip = letter_index(letter).unwrap_or(0) as u8;
        Voice { osc: Oscillator::new(0x1234_5678 ^ seed), singer: Singer::new(0x2545_f491 ^ seed), clear: clear::Singer::new(0x5bd1_e995 ^ seed), strip, ..Voice::default() }
    }

    /// Takes up note `n` (played by `ins`) from its start; a voice note
    /// sings its part of `segments` (`voice`'s) or `clear`.
    fn take_up(&mut self, n: &Note, ins: &Instrument, (segments, clear): (&[Segment], &[Seg])) {
        let sounding = self.note.is_some();
        (self.note, self.started, self.osc.phase) = (Some(*n), n.start, 0.0);
        self.alpha = ins.lowpass.map_or(0.0, OnePole::alpha);
        let angle = (n.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        self.pan = (libm::cosf(angle), libm::sinf(angle));
        self.drive = if ins.drive > 0.0 { libm::powf(10.0, ins.drive / 20.0) } else { 0.0 };
        let vowels = if ins.voice == Some(Voicing::Clear) { clear::SUNG } else { SUNG };
        self.noise_gain = if self.drive > 0.0 { libm::tanhf(self.drive * vowels) / vowels } else { 1.0 };
        if ins.consonants != 0.0 {
            self.noise_gain *= libm::powf(10.0, ins.consonants / 20.0);
        }
        self.crush.set(ins);
        if ins.voice.is_some() {
            self.from = if sounding { self.level } else { 0.0 };
        }
        match ins.voice {
            Some(Voicing::Formants) => self.singer.start(sung(n, segments), !sounding),
            Some(Voicing::Clear) => self.clear.start(sung(n, clear), !sounding),
            None => {}
        }
    }

    /// The next frame of the note it plays at sample `time` of the song,
    /// ducked by `ducked` and scaled by the song's `volume`, a jump's
    /// `fade` and the channel's layer (`loud`); None once it is silent.
    fn sound(&mut self, song: &Song, time: u64, ducked: f32, (volume, fade, loud): (f32, f32, f32)) -> Option<[f32; 2]> {
        let sr = SAMPLE_RATE as f32;
        let n = self.note?;
        let ins = song.instruments.get(n.instrument)?;
        let t = time.wrapping_sub(self.started) as f32 / sr;
        let held = n.held as f32 / sr;

        // ADSR (a voice's attack from where it was).
        let level = if t < held {
            let l = if t < ins.attack {
                if ins.voice.is_some() { self.from + (1.0 - self.from) * (t / ins.attack.max(1e-4)) } else { t / ins.attack.max(1e-4) }
            } else {
                let d = (t - ins.attack) / ins.decay.max(1e-4);
                if d < 1.0 { 1.0 + (ins.sustain - 1.0) * d } else { ins.sustain }
            };
            self.released_at = l;
            l
        } else {
            let r = (t - held) / ins.release.max(1e-4);
            if r >= 1.0 {
                (self.note, self.level) = (None, 0.0);
                return None;
            }
            self.released_at * (1.0 - r)
        };
        self.level = level;

        // Pitch: the note (or a fixed one), a sweep, the arpeggio, vibrato.
        let mut freq = ins.freq.unwrap_or(n.freq.unwrap_or(0.0));
        if ins.sweep != 0.0 {
            let span = ins.sweep_time.unwrap_or(ins.attack + ins.decay).max(1e-3);
            freq *= libm::powf(2.0, ins.sweep * (t / span).min(1.0) / 12.0);
        }
        if !ins.arp.is_empty() {
            let step = (t * ins.arp_speed) as usize % ins.arp.len();
            freq *= libm::powf(2.0, ins.arp[step] / 12.0);
        }
        if ins.vibrato > 0.0 && t > ins.vibrato_delay {
            // The clear voice's vibrato grows in over a third of a second, as a singer's.
            let depth = if ins.voice == Some(Voicing::Clear) { ins.vibrato * ((t - ins.vibrato_delay) / 0.3).min(1.0) } else { ins.vibrato };
            freq *= 1.0 + depth * libm::sinf(std::f32::consts::TAU * ins.vibrato_speed * (t - ins.vibrato_delay));
        }
        let duty = (ins.duty + ins.pwm * libm::sinf(std::f32::consts::TAU * ins.pwm_speed * t)).clamp(0.05, 0.95);
        let (mut s, noise) = match ins.voice {
            Some(Voicing::Formants) => self.singer.next(sung(&n, &song.segments), time.wrapping_sub(self.started), freq),
            Some(Voicing::Clear) => self.clear.next(sung(&n, &song.clear), time.wrapping_sub(self.started), freq),
            None => (self.osc.next(ins.wave, freq / sr, duty, false), 0.0),
        };
        // Driven into a soft clipper before the filters, as an amp
        // before its speaker; a wave not even above and below zero
        // (a voice's) clips to a middle off zero, taken off again.
        if self.drive > 0.0 {
            let x = libm::tanhf(s * self.drive);
            s = x - self.dc.0 + DC_BLOCK * self.dc.1;
            self.dc = (x, s);
        }
        // A voice's breath and hiss join past the clipper, raised as
        // much as it raises a voice, so they keep their distance
        // from the vowels however hard it drives.
        if ins.voice.is_some() {
            s += noise * self.noise_gain;
        }
        if ins.lowpass.is_some() {
            s = self.low.next(s, self.alpha);
        }
        if let Some(mode) = ins.filter {
            // The cutoff swept per note, evenly in octaves.
            let mut cutoff = ins.cutoff;
            if let Some(end) = ins.cutoff_end {
                let span = ins.cutoff_time.unwrap_or(ins.attack + ins.decay).max(1e-3);
                cutoff *= libm::powf(end / ins.cutoff, (t / span).min(1.0));
            }
            s = self.filter.process(s, cutoff, ins.resonance, mode);
        }
        // Ducked under a sidechain note, unless this is one.
        let duck = if ins.sidechain > 0.0 { 1.0 } else { 1.0 - ducked * ins.duck };
        // Crushed after the envelope, before the volumes: its steps are the instrument's own.
        let mut s = s * level;
        if self.crush.on() {
            s = self.crush.next(s);
        }
        let gain = s * ins.volume * n.volume * volume * duck * fade * loud;
        Some([gain * self.pan.0 * std::f32::consts::SQRT_2, gain * self.pan.1 * std::f32::consts::SQRT_2])
    }
}

/// The segments a voice note sings (none for another note).
fn sung<'a, S>(n: &Note, segments: &'a [S]) -> &'a [S] {
    n.sung.map_or(&[], |(from, to)| &segments[from as usize..to as usize])
}

/// A bit crusher: the sound held at a rate (sample and hold) and rounded
/// to a number of bits, as an early sampler played it.
#[derive(Clone, Default)]
pub(super) struct Crusher {
    /// A rate's share of a sample (0: none), and the steps either side of
    /// zero (0: none).
    step: f32,
    steps: f32,
    phase: f32,
    held: f32,
}

impl Crusher {
    fn set(&mut self, ins: &Instrument) {
        self.step = ins.crush_rate.map_or(0.0, |r| (r / SAMPLE_RATE as f32).min(1.0));
        self.steps = ins.crush_bits.map_or(0.0, |b| libm::powf(2.0, (b as i32 - 1) as f32));
    }

    fn on(&self) -> bool {
        self.step > 0.0 || self.steps > 0.0
    }

    fn next(&mut self, x: f32) -> f32 {
        let mut x = x;
        if self.step > 0.0 {
            if self.phase <= 0.0 {
                self.held = x;
                self.phase += 1.0;
            }
            self.phase -= self.step;
            x = self.held;
        }
        if self.steps > 0.0 {
            x = ((x * self.steps).round() / self.steps).clamp(-1.0, 1.0);
        }
        x
    }
}

/// A state-variable filter (the SID's kind: low, band and high pass from
/// one 12 dB/octave stage), in the zero-delay form that stays stable
/// however fast the cutoff moves.
#[derive(Clone, Default)]
pub(super) struct Svf {
    ic1: f32,
    ic2: f32,
    /// The cutoff its coefficient was last worked out for, and it.
    cut: f32,
    g: f32,
}

impl Svf {
    fn process(&mut self, x: f32, cutoff: f32, resonance: f32, mode: FilterMode) -> f32 {
        let sr = SAMPLE_RATE as f32;
        let cutoff = cutoff.clamp(20.0, sr * 0.45);
        if cutoff != self.cut {
            (self.cut, self.g) = (cutoff, libm::tanf(std::f32::consts::PI * cutoff / sr));
        }
        let g = self.g;
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
        if y.abs() <= 1.0 { y } else { y.signum() * (1.0 + libm::tanhf(y.abs() - 1.0)) }
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
        let t = now.wrapping_sub(self.from) as f32 / SAMPLE_RATE as f32;
        if t < ATTACK {
            return self.depth * t / ATTACK;
        }
        let r = ((t - ATTACK) / self.release.max(1e-3)).min(1.0);
        self.depth * 0.5 * (1.0 + libm::cosf(std::f32::consts::PI * r))
    }
}

/// A jump this far off (samples, 50 ms) when keeping time with a game;
/// less is caught up by going a little faster or slower.
const JUMP: i64 = SAMPLE_RATE as i64 / 20;

/// How much of its last output the drive's DC blocker keeps a sample: a
/// high-pass at about 10 Hz.
const DC_BLOCK: f32 = 1.0 - std::f32::consts::TAU * 10.0 / SAMPLE_RATE as f32;

/// A sung vowel's typical level (its RMS before the volumes): what the
/// clipper raises a voice by at that level raises its noise past it.
const SUNG: f32 = 0.4;

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
    /// The asks last followed (`Glide::key`), the volume's and the muffle's.
    pub(super) asked: [(u32, u32, u32); 2],
    /// Each side's filter state, and its coefficient for the muffle it was
    /// last worked out for.
    pub(super) low: [(f32, f32); 2],
    pub(super) g: (f32, f32),
}

impl Default for Bus {
    fn default() -> Self {
        Self { level: Ramp::at(1.0), muffle: Ramp::at(0.0), asked: [Glide::still(1.0).key(), Glide::still(0.0).key()], low: [(0.0, 0.0); 2], g: (-1.0, 0.0) }
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
            self.g = (amount, libm::tanf(std::f32::consts::PI * cutoff / SAMPLE_RATE as f32));
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

/// A channel's strip as the player applies it (`Mix::strips`): its level
/// (the fader, or 0 muted) and its pan, each gliding sample by sample,
/// and the asks last followed (`Glide::key`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Lane {
    pub(super) level: Ramp,
    pub(super) pan: Ramp,
    asked: [(u32, u32, u32); 2],
}

impl Lane {
    /// As the game's strip is now, gliding on where it still glides.
    fn at(level: &Glide, pan: &Glide) -> Self {
        let mut lane = Lane { level: Ramp::at(level.now()), pan: Ramp::at(pan.now()), asked: [level.key(), pan.key()] };
        lane.level.toward(level.to, level.left(), SHORTEST.0);
        lane.pan.toward(pan.to, pan.left(), SHORTEST.0);
        lane
    }

    /// Follows a new ask, from where it sounds now.
    fn follow(&mut self, level: &Glide, pan: &Glide) {
        if level.key() != self.asked[0] {
            self.level.toward(level.to, level.left(), SHORTEST.0);
        }
        if pan.key() != self.asked[1] {
            self.pan.toward(pan.to, pan.left(), SHORTEST.0);
        }
        self.asked = [level.key(), pan.key()];
    }

    /// Whether it sounds as written and stays so (nothing to apply).
    fn neutral(&self) -> bool {
        (self.level.now, self.level.to, self.pan.now, self.pan.to) == (1.0, 1.0, 0.0, 0.0)
    }

    /// Its gain on each side for this sample, and a step on.
    fn next(&mut self) -> [f32; 2] {
        let (level, pan) = (self.level.next(), self.pan.next());
        [level * (1.0 - pan).min(1.0), level * (1.0 + pan).min(1.0)]
    }
}

/// Plays a song, sample by sample; runs in the audio thread.
pub struct Player {
    pub(super) song: Song,
    pub(super) voices: Vec<Voice>,
    /// Each channel's note let go where the song went round (or ended),
    /// its release ringing on while the channel's voice begins the next
    /// round: made with the song, so the audio thread makes none.
    pub(super) tails: Vec<Voice>,
    pub(super) time: u64,
    pub(super) duck: Option<Duck>,
    pub volume: f32,
    /// How fast its time goes against the samples (1: as written), nudged
    /// to keep with a game that reads its beats (`follow`); what is left
    /// of a sample (where the song went round between two samples, or a
    /// little before its start: below 0 while `time` is 0).
    pub(super) rate: f64,
    pub(super) carry: f64,
    /// Samples into the song as written played on and on, never folded
    /// round its loop (the game's beats, `follow`): where a swap is due.
    pub(super) position: f64,
    /// Channels a game laid, to swap in when `position` reaches their due
    /// point (`pend`), and those swapped out, for the main thread to drop
    /// (the audio thread gives no memory back).
    pub(super) pending: Option<Box<Swap>>,
    pub(in crate::audio) spent: Option<Box<Swap>>,
    /// Samples since a jump, fading in up to `FADE_IN`.
    pub(super) since_jump: u32,
    /// The game's tension (`music.intensity`), and how loud each channel's
    /// layer is now (0..1, moving a bar's worth a bar).
    pub intensity: f32,
    pub(super) loud: Vec<f32>,
    /// The game's say over how it sounds (`Mix`): neutral unless followed.
    pub(super) bus: Bus,
    /// Each channel's strip by letter (`Mix::strips`), made with the
    /// player, kept through a swap; whether any is not neutral, and each
    /// one's gains for the sample being made.
    pub(super) lanes: [Lane; LETTERS],
    lanes_live: bool,
    gains: [[f32; 2]; LETTERS],
}

impl Player {
    pub fn title(&self) -> &str {
        &self.song.title
    }

    pub fn new(song: Song, volume: f32) -> Self {
        let voices = (0..song.channels.len()).map(|i| Voice::new(i as u32 * 7919, song.names[i])).collect();
        let tails = (0..song.channels.len()).map(|_| Voice::default()).collect();
        let loud = song.layers.iter().map(|_| 1.0).collect();
        let lane = Lane::at(&Glide::still(1.0), &Glide::still(0.0));
        Self {
            song,
            voices,
            tails,
            time: 0,
            duck: None,
            volume,
            rate: 1.0,
            carry: 0.0,
            position: 0.0,
            pending: None,
            spent: None,
            since_jump: FADE_IN,
            intensity: 1.0,
            loud,
            bus: Bus::default(),
            lanes: [lane; LETTERS],
            lanes_live: false,
            gains: [[1.0; 2]; LETTERS],
        }
    }

    /// Sounds as the game's `mix` is now, gliding on where it still glides
    /// (a song begun, or the game brought back): a channel muted is silent
    /// from the song's first sample.
    pub fn start_mix(&mut self, mix: &Mix) {
        self.bus.level = Ramp::at(mix.volume.now());
        self.bus.muffle = Ramp::at(mix.muffle.now());
        self.bus.asked = [mix.volume.key(), mix.muffle.key()];
        self.bus.level.toward(mix.volume.to, mix.volume.left(), SHORTEST.0);
        self.bus.muffle.toward(mix.muffle.to, mix.muffle.left(), SHORTEST.1);
        for (i, lane) in self.lanes.iter_mut().enumerate() {
            let strip = mix.strip(letter(i));
            *lane = Lane::at(&strip.level, &strip.pan);
        }
        self.lanes_live = self.lanes.iter().any(|l| !l.neutral());
    }

    /// Follows the game's `mix`: a new ask glides from where the song
    /// sounds now to where it was asked, over what is left of its time.
    pub fn follow_mix(&mut self, mix: &Mix) {
        if mix.volume.key() != self.bus.asked[0] {
            self.bus.level.toward(mix.volume.to, mix.volume.left(), SHORTEST.0);
        }
        if mix.muffle.key() != self.bus.asked[1] {
            self.bus.muffle.toward(mix.muffle.to, mix.muffle.left(), SHORTEST.1);
        }
        self.bus.asked = [mix.volume.key(), mix.muffle.key()];
        for (i, lane) in self.lanes.iter_mut().enumerate() {
            let strip = mix.strip(letter(i));
            lane.follow(&strip.level, &strip.pan);
        }
        self.lanes_live = self.lanes.iter().any(|l| !l.neutral());
    }

    /// Sets how loud each layer is at once, for the tension it starts at
    /// (a song begun while the game is calm starts calm).
    pub fn start_at(&mut self, intensity: f32) {
        self.intensity = intensity;
        for (loud, &from) in self.loud.iter_mut().zip(&self.song.layers) {
            *loud = if intensity >= from { 1.0 } else { 0.0 };
        }
    }

    /// Samples into the song a beat count is (from its start, on and on
    /// round its loop as it plays), exactly.
    pub fn at_beat(&self, beats: f64) -> f64 {
        beats * 60.0 / self.song.tempo as f64 * SAMPLE_RATE as f64
    }

    /// Samples into the song bar `bar` (from 1) begins at, exactly.
    pub fn at_bar(&self, bar: u32) -> f64 {
        super::song::bar_line(self.song.tempo, self.song.meter, bar)
    }

    /// Jumps to `at` samples into the song (round its loop), each voice
    /// taking up the note that sounds there, faded in so it does not click;
    /// what rang on from before falls silent.
    pub fn seek(&mut self, at: f64) {
        self.position = at;
        let at = self.within(at);
        let time = at.max(0.0) as u64;
        (self.time, self.carry, self.duck, self.since_jump) = (time, at - time as f64, None, 0);
        for tail in &mut self.tails {
            tail.note = None;
        }
        for (v, notes) in self.voices.iter_mut().zip(&self.song.channels) {
            v.next = notes.partition_point(|n| n.start <= time);
            v.note = None;
            if let Some(n) = v.next.checked_sub(1).map(|i| &notes[i]).filter(|n| n.freq.is_some()) {
                v.take_up(n, &self.song.instruments[n.instrument], (&self.song.segments, &self.song.clear));
            }
        }
    }

    /// Keeps time with a game's clock, `beats` since the song began (on
    /// and on, which it folds into the song exactly, so a short loop
    /// never drifts from the game's bars): a jump when it is far off
    /// (after a pause), else a little faster or slower until it is with
    /// it again, unheard.
    pub fn follow(&mut self, beats: f64) {
        let (from, to) = self.song.span;
        let length = to - from;
        let unwrapped = self.at_beat(beats);
        let target = self.within(unwrapped);
        let now = self.time as f64 + self.carry;
        let mut off = target - now;
        if self.song.looping && now >= from && target >= from {
            // The shorter way round the loop.
            if off > length / 2.0 {
                off -= length;
            } else if off < -length / 2.0 {
                off += length;
            }
        }
        if off.abs() > JUMP as f64 {
            self.seek(unwrapped);
            self.rate = 1.0;
        } else {
            self.rate = 1.0 + (off / (4.0 * SAMPLE_RATE as f64)).clamp(-0.003, 0.003);
        }
    }

    /// Where `at` samples of the song as written, played on and on, lie:
    /// round its loop (`#loop`'s bars, else the whole song) past its end,
    /// at the end of a song that plays once.
    pub(super) fn within(&self, at: f64) -> f64 {
        let (from, to) = self.song.span;
        match () {
            _ if !self.song.looping => at.min(to),
            _ if at < to => at,
            _ => from + (at - from) % (to - from),
        }
    }

    /// Whether a song that plays once has ended, its last notes rung out.
    pub fn over(&self) -> bool {
        !self.song.looping && self.time as f64 + self.carry >= self.song.span.1 && self.voices.iter().chain(&self.tails).all(|v| v.note.is_none())
    }

    /// Lets go of every note that sounds as the song goes round or ends:
    /// it rings on in its channel's tail, the voice as it was (its filters,
    /// its noise) going on to the next round as when the note was cut
    /// there. (A tail still ringing from the time before is cut.)
    fn let_go(&mut self) {
        for (v, tail) in self.voices.iter_mut().zip(&mut self.tails) {
            if let Some(n) = &mut v.note {
                n.held = n.held.min(self.time.wrapping_sub(v.started));
                tail.clone_from(v);
                v.note = None;
            }
        }
    }

    /// Goes round the loop, exactly as long as its bars: what is left of a
    /// sample kept, the notes still ringing and the ducking going on from
    /// where they were, every voice to begin the loop's notes.
    fn go_round(&mut self) {
        let (from, to) = self.song.span;
        let at = self.time as f64 + self.carry - (to - from);
        let time = at.max(0.0) as u64;
        let back = self.time - time;
        (self.time, self.carry) = (time, at - time as f64);
        for tail in &mut self.tails {
            tail.started = tail.started.wrapping_sub(back);
        }
        if let Some(duck) = &mut self.duck {
            duck.from = duck.from.wrapping_sub(back);
        }
        for (v, notes) in self.voices.iter_mut().zip(&self.song.channels) {
            v.next = notes.partition_point(|n| (n.start as f64) < from);
        }
    }

    /// Adds the next frames to interleaved stereo `out`.
    pub fn mix(&mut self, out: &mut [f32]) {
        // The layers move towards the tension, a bar's worth a bar.
        let step = self.song.tempo / (60.0 * SAMPLE_RATE as f32 * self.song.meter as f32);
        for frame in out.as_chunks_mut::<2>().0 {
            // Laid channels swapped in where they are due: a sample or two
            // early rather than late (`LOOP_SLACK`), so a note a sum of
            // lengths puts a sample before the bar line is not missed.
            if self.pending.as_ref().is_some_and(|s| self.position + LOOP_SLACK >= s.due) {
                self.swap();
            }
            // Round the loop (`#loop`'s bars, else the whole song), or at
            // the end of a song that plays once: what sounds is let go.
            if self.time as f64 + self.carry >= self.song.span.1 {
                self.let_go();
                if self.song.looping {
                    self.go_round();
                } else if self.over() {
                    return;
                }
            }
            let ducked = self.duck.map_or(0.0, |d| d.amount(self.time));
            for (loud, &from) in self.loud.iter_mut().zip(&self.song.layers) {
                let want = if self.intensity >= from { 1.0 } else { 0.0 };
                *loud = if *loud < want { (*loud + step).min(want) } else { (*loud - step).max(want) };
            }
            let fade = if self.since_jump < FADE_IN { self.since_jump as f32 / FADE_IN as f32 } else { 1.0 };
            // Each channel's strip, where the game set one (`Mix::strips`).
            let live = self.lanes_live;
            if live {
                for (gain, lane) in self.gains.iter_mut().zip(&mut self.lanes) {
                    *gain = lane.next();
                }
            }
            let gains = &self.gains;
            let strip = |v: &Voice, [l, r]: [f32; 2]| if live { [l * gains[v.strip as usize][0], r * gains[v.strip as usize][1]] } else { [l, r] };
            let mut sum = [0.0f32; 2];
            for (ch, (v, tail)) in self.voices.iter_mut().zip(&mut self.tails).enumerate() {
                let notes = &self.song.channels[ch];
                while v.next < notes.len() && notes[v.next].start <= self.time {
                    // Rests and the end marker have no pitch: the note before rings out.
                    let n = &notes[v.next];
                    if n.freq.is_some() {
                        let ins = &self.song.instruments[n.instrument];
                        v.take_up(n, ins, (&self.song.segments, &self.song.clear));
                        if ins.sidechain > 0.0 {
                            self.duck = Some(Duck { from: self.time, depth: ins.sidechain, release: ins.sidechain_release });
                        }
                    }
                    v.next += 1;
                }
                let scale = (self.volume, fade, self.loud[ch]);
                let heard = [v.sound(&self.song, self.time, ducked, scale).map(|s| strip(v, s)), tail.sound(&self.song, self.time, ducked, scale).map(|s| strip(tail, s))];
                for [l, r] in heard.into_iter().flatten() {
                    sum[0] += l;
                    sum[1] += r;
                }
            }
            // The notes of channels laid no more, ringing out.
            for tail in &mut self.tails[self.voices.len()..] {
                if let Some([l, r]) = tail.sound(&self.song, self.time, ducked, (self.volume, fade, 1.0)).map(|s| strip(tail, s)) {
                    sum[0] += l;
                    sum[1] += r;
                }
            }
            // As loud and as clear as the game asked (`Mix`).
            let (l, r) = self.bus.apply(sum[0], sum[1]);
            frame[0] += l;
            frame[1] += r;
            self.since_jump = self.since_jump.saturating_add(1);
            // A sample on, or a hair more or less when keeping time.
            self.carry += self.rate;
            self.position += self.rate;
            let on = self.carry as u64;
            self.carry -= on as f64;
            self.time += on;
        }
    }

    /// The digests of the channels laid in the song it plays and in the one
    /// it is to swap to (`parse::Laid`), where a game laid them.
    pub fn laid(&self) -> Digests {
        (self.song.laid, self.pending.as_ref().and_then(|s| s.song.laid))
    }

    /// The channels to swap to when their due point comes (`Swap`; None:
    /// none, what was to come called off), in place of any to come before:
    /// what it no longer holds handed back, for the main thread to drop.
    pub fn pend(&mut self, swap: Option<Box<Swap>>) -> [Option<Box<Swap>>; 2] {
        let swap = swap.map(|mut s| {
            s.due = self.at_beat(s.due);
            s
        });
        [std::mem::replace(&mut self.pending, swap), self.spent.take()]
    }

    /// What a swap let go of, for the main thread to drop.
    pub fn spent(&mut self) -> Option<Box<Swap>> {
        self.spent.take()
    }

    /// Swaps the laid channels pending in, at this sample: a channel the
    /// same as before plays on untouched; a changed one's voice keeps the
    /// note it sounds (its own copy) until the channel's next note takes
    /// it, its notes from here on the new ones; a channel laid no more
    /// rings out in a tail of its own; the layers go by letter. A song of
    /// other bars is folded by the beats since it began (bar 9 of a
    /// four-bar song is its bar 1). Nothing is made or dropped here: the
    /// swap was made on the main thread, the old song is kept for it.
    fn swap(&mut self) {
        let Some(mut new) = self.pending.take() else { return };
        for (i, letter) in new.song.names.iter().enumerate() {
            match self.song.names.iter().position(|l| l == letter) {
                Some(j) => {
                    std::mem::swap(&mut new.voices[i], &mut self.voices[j]);
                    std::mem::swap(&mut new.tails[i], &mut self.tails[j]);
                    new.loud[i] = self.loud[j];
                }
                None => new.loud[i] = if self.intensity >= new.song.layers[i] { 1.0 } else { 0.0 },
            }
        }
        for (j, letter) in self.song.names.iter().enumerate() {
            if !new.song.names.contains(letter) && self.voices[j].note.is_some() && new.tails.len() < new.tails.capacity() {
                new.tails.push(std::mem::take(&mut self.voices[j]));
            }
        }
        let same_span = new.song.span == self.song.span;
        std::mem::swap(&mut self.song, &mut new.song);
        std::mem::swap(&mut self.voices, &mut new.voices);
        std::mem::swap(&mut self.tails, &mut new.tails);
        std::mem::swap(&mut self.loud, &mut new.loud);
        // (`new` holds what played until now.)
        if !same_span {
            let at = self.within(self.position);
            let time = at.max(0.0) as u64;
            let shift = time.wrapping_sub(self.time);
            (self.time, self.carry) = (time, at - time as f64);
            for v in self.voices.iter_mut().chain(&mut self.tails) {
                v.started = v.started.wrapping_add(shift);
            }
            if let Some(duck) = &mut self.duck {
                duck.from = duck.from.wrapping_add(shift);
            }
        }
        for (i, (v, notes)) in self.voices.iter_mut().zip(&self.song.channels).enumerate() {
            let letter = self.song.names[i];
            let same = same_span && new.song.names.iter().position(|l| *l == letter).is_some_and(|j| new.song.sources[j] == self.song.sources[i]);
            if !same {
                v.next = notes.partition_point(|n| n.start < self.time);
            }
        }
        self.spent = Some(new);
    }

    /// The whole song (once; with `#loop` round its loop into its first
    /// bars again, `Song::once`) as interleaved stereo samples.
    pub fn render(mut self, seconds: Option<f32>) -> Vec<f32> {
        let frames = match seconds {
            Some(s) => (s * SAMPLE_RATE as f32) as u64,
            None => self.song.once(),
        };
        let mut out = vec![0.0; frames as usize * 2];
        self.mix(&mut out);
        out
    }
}

/// The letter of a strip's place among the `LETTERS` (`letter_index`'s
/// other way).
fn letter(i: usize) -> char {
    if i < 26 { (b'A' + i as u8) as char } else { (b'a' + (i - 26) as u8) as char }
}

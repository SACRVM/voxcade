//! The player: renders a song's voices, sidechain, layers, filter and the game's mix.

use super::SAMPLE_RATE;
use super::parse::LOOP_SLACK;
use super::mix::{CUTOFF, GLIDING, Glide, Knob, LETTERS, Mix, NEXT_NOTE, SHORTEST, Strip, letter_index, muffle_cutoff, place};
use super::clear::{self, Seg};
use super::song::{Curve, FilterMode, Instrument, LAYERS, Note, Song, Sum, UNISON, Voicing};
use super::voice::{Segment, Singer};
use crate::audio::STEAL_FADE;
use crate::audio::synth::{OnePole, Oscillator, ratio};

/// Channels a game laid, made on the main thread to be swapped in by the
/// audio (`Player::pend`): the song and a voice for each of its channels
/// with its tail and its slot for a note giving way (a channel laid before
/// brings its own), room for the notes of channels laid no more to ring
/// out, and the beats since the song began it is due at
/// (`f64::NEG_INFINITY`: at once).
pub struct Swap {
    song: Song,
    voices: Vec<Voice>,
    tails: Vec<Voice>,
    giving: Vec<Voice>,
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
        // Room for each channel laid no more: its note and its tail.
        let mut tails = Vec::with_capacity(n + 2 * MOST_LETTERS);
        tails.resize_with(n, Voice::default);
        let giving = (0..n).map(|_| Voice::default()).collect();
        Box::new(Swap { voices, tails, giving, loud: vec![1.0; n], due, song })
    }
}

/// One channel's voice: the note it plays (its own copy, so it can ring
/// on after the song went round), and for each layer of its instrument
/// where its oscillator, filters and envelope are (`Tone`).
#[derive(Clone, Default)]
pub(super) struct Voice {
    pub(super) next: usize,
    pub(super) note: Option<Note>,
    /// The sample of the song the note started at (before 0, wrapped, for
    /// a note ringing on in a round begun since).
    pub(super) started: u64,
    /// The samples heard since the note began, and those when its key was
    /// let go: its sound's own time, which a tempo set by the game does not
    /// stretch (`Player::ratio`), while its start and its length are the
    /// song's.
    age: u64,
    gate: Option<u64>,
    /// The instrument's own line (an instrument without layers: the whole
    /// of it), and its further layers (`Instrument::layers`, `+`): fixed,
    /// made with the voice, so a layered note takes no memory.
    pub(super) tone: Tone,
    more: [Tone; LAYERS - 1],
    /// What a layered note's sum goes through (`Instrument::sum`, `=`).
    sum: Summed,
    /// The note's pan's share of each side, kept from start to end.
    pan: (f32, f32),
    /// Where a voice's attack starts from (the level it was at: a sung
    /// line never jumps to silence between its syllables).
    from: f32,
    /// The singing voices (`voice`, and the clear one).
    pub(super) singer: Singer,
    pub(super) clear: clear::Singer,
    /// What a voice's noise is raised by, joining past the clipper.
    noise_gain: f32,
    /// Its channel's strip on the desk (`Player::lanes`, by letter): its
    /// own, so a note ringing out of a channel laid no more keeps it.
    strip: u8,
    /// Samples left of its fade, a note that gave way to the next on its
    /// channel (`give_way`; 0: not fading), and its last frame as heard.
    fading: u32,
    last: [f32; 2],
}

/// Samples a note giving way to the next on its channel fades out over
/// (5 ms, as a sound the mixer steals), under the new one.
const GIVE_WAY: u32 = STEAL_FADE as u32;

/// What is left of notes that had to go at once (`Residue::add`): their
/// last frame, laid down to silence in a straight line over `GIVE_WAY`
/// samples, so even a note with nowhere left to fade ends without a step.
#[derive(Clone, Copy, Default)]
pub(super) struct Residue {
    now: [f32; 2],
    left: u32,
}

impl Residue {
    /// Takes over the frame a note made last (added to what is left of
    /// others), from where it is to silence.
    fn add(&mut self, last: [f32; 2]) {
        self.now = [self.now[0] + last[0], self.now[1] + last[1]];
        self.left = GIVE_WAY;
    }

    /// This sample's frame, and a step on.
    fn next(&mut self) -> [f32; 2] {
        if self.left == 0 {
            return [0.0; 2];
        }
        let now = self.now;
        let keep = (self.left - 1) as f32 / self.left as f32;
        self.now = [now[0] * keep, now[1] * keep];
        self.left -= 1;
        now
    }
}

/// Where a voice's note goes as the next note on its channel takes the
/// voice at sample `time` of the song: let go, into the channel's slot for
/// it (`giving`), fading out over `GIVE_WAY` samples under the new one,
/// which starts on time as ever (from silence: the voice holds no note
/// then). A note fading there still (notes closer than the fade) goes on
/// as a residue from its last frame. A note that has ended is left alone,
/// and a sung line goes on from syllable to syllable as ever (a voice's
/// note taken up over a voice's).
fn give_way(v: &mut Voice, next: &Instrument, song: &Song, time: u64, giving: &mut Voice, residue: &mut Residue) {
    let Some(n) = v.note else { return };
    if next.voice.is_some() && song.instruments.get(n.instrument).is_some_and(|i| i.voice.is_some()) {
        return;
    }
    if giving.note.is_some() {
        residue.add(giving.last);
    }
    giving.clone_from(v);
    if let Some(n) = &mut giving.note {
        n.held = n.held.min(time.wrapping_sub(v.started));
    }
    giving.fading = GIVE_WAY;
    v.note = None;
}

/// One layer of a note as it sounds: its oscillator, its filters, its
/// envelope's level, its drive (a gain, 0 off) and bit crusher, and its
/// shape as it was taken up.
#[derive(Clone, Default)]
pub(super) struct Tone {
    osc: Oscillator,
    low: OnePole,
    pub(super) filter: Svf,
    /// Envelope level while the key is held, where the release starts
    /// from; and the level at the last sample.
    released_at: f32,
    level: f32,
    /// How far its low-pass moves a sample (kept the note's whole length).
    alpha: f32,
    drive: f32,
    crush: Crusher,
    /// The drive's DC blocker: the last sample in and out.
    dc: (f32, f32),
    /// The note's shape as it was taken up: its layer's, with what the
    /// strip turned from the next note (`Strip::next`) laid over the
    /// first layer's.
    shape: Shape,
    /// A sample's read head: frames of its recording into the stretch it
    /// plays (`Shape::stretch`), counted the way it plays (from its end
    /// when reversed).
    head: f64,
    /// Its further unison oscillators (`osc` the first), each one's pitch
    /// against the note's, and the detune those were worked out for (NaN:
    /// none yet).
    unison: [Oscillator; UNISON - 1],
    ratios: [f32; UNISON],
    detuned: f32,
}

/// What a layered note's layers go through once added (`Sum`): the soft
/// clipper's gain (0 off) and its DC blocker, the filter, the crusher.
#[derive(Clone, Default)]
struct Summed {
    drive: f32,
    drive_sweep: Option<(f32, f32)>,
    dc: (f32, f32),
    filter: Svf,
    crush: Crusher,
}

/// A note's shape, kept from start to end: its envelope's times and how
/// they fall, its pitch's sweep or fall from a start, its cutoff's and its
/// drive's sweeps (how far, as a ratio, and over how long), its rates, and
/// how far it ducks the other channels.
#[derive(Clone, Copy, Default)]
struct Shape {
    attack: f32,
    decay: f32,
    release: f32,
    curve: Curve,
    freq: Option<f32>,
    sweep: f32,
    sweep_span: f32,
    pitch_start: Option<f32>,
    pitch_decay: f32,
    cutoff_sweep: Option<(f32, f32)>,
    drive_sweep: Option<(f32, f32)>,
    vibrato_speed: f32,
    vibrato_delay: f32,
    pwm_speed: f32,
    arp_speed: f32,
    sidechain: f32,
    sidechain_release: f32,
    /// A sample's: the pitch it plays at its own speed at (Hz), and the
    /// stretch of its recording it plays, its first frame and how many.
    root: f32,
    stretch: (usize, usize),
}

/// Where each word lies in a strip's tables (`GLIDING`, `NEXT_NOTE`),
/// worked out as the program is built.
mod at {
    use super::{GLIDING, NEXT_NOTE, place};
    pub const DUTY: usize = place(&GLIDING, "duty");
    pub const PWM: usize = place(&GLIDING, "pwm");
    pub const VIBRATO: usize = place(&GLIDING, "vibrato");
    pub const SUSTAIN: usize = place(&GLIDING, "sustain");
    pub const RESONANCE: usize = place(&GLIDING, "resonance");
    pub const DUCK: usize = place(&GLIDING, "duck");
    pub const DETUNE: usize = place(&GLIDING, "detune");
    pub const ATTACK: usize = place(&NEXT_NOTE, "attack");
    pub const DECAY: usize = place(&NEXT_NOTE, "decay");
    pub const RELEASE: usize = place(&NEXT_NOTE, "release");
    pub const FREQ: usize = place(&NEXT_NOTE, "freq");
    pub const SWEEP: usize = place(&NEXT_NOTE, "sweep");
    pub const SWEEP_TIME: usize = place(&NEXT_NOTE, "sweep_time");
    pub const PITCH_START: usize = place(&NEXT_NOTE, "pitch_start");
    pub const PITCH_DECAY: usize = place(&NEXT_NOTE, "pitch_decay");
    pub const CUTOFF_END: usize = place(&NEXT_NOTE, "cutoff_end");
    pub const CUTOFF_TIME: usize = place(&NEXT_NOTE, "cutoff_time");
    pub const VIBRATO_SPEED: usize = place(&NEXT_NOTE, "vibrato_speed");
    pub const VIBRATO_DELAY: usize = place(&NEXT_NOTE, "vibrato_delay");
    pub const PWM_SPEED: usize = place(&NEXT_NOTE, "pwm_speed");
    pub const ARP_SPEED: usize = place(&NEXT_NOTE, "arp_speed");
    pub const LOWPASS: usize = place(&NEXT_NOTE, "lowpass");
    pub const DRIVE: usize = place(&NEXT_NOTE, "drive");
    pub const DRIVE_END: usize = place(&NEXT_NOTE, "drive_end");
    pub const DRIVE_TIME: usize = place(&NEXT_NOTE, "drive_time");
    pub const CRUSH_RATE: usize = place(&NEXT_NOTE, "crush_rate");
    pub const CRUSH_BITS: usize = place(&NEXT_NOTE, "crush_bits");
    pub const SIDECHAIN: usize = place(&NEXT_NOTE, "sidechain");
    pub const SIDECHAIN_RELEASE: usize = place(&NEXT_NOTE, "sidechain_release");
    pub const ROOT: usize = place(&NEXT_NOTE, "root");
    pub const START: usize = place(&NEXT_NOTE, "start");
    pub const END: usize = place(&NEXT_NOTE, "end");
}

/// How long a looped sample's seam is faded across (`loop=yes`), and an
/// unlooped one's last stretch faded to silence as it plays out, in
/// seconds: neither clicks.
const LOOP_FADE: f64 = 0.005;
const END_FADE: f32 = 0.003;

/// The recording `x` at `p` frames, between frames by a cubic through the
/// four around (Catmull-Rom), the frames past its ends its end ones.
fn cubic(x: &[f32], p: f64) -> f32 {
    let last = x.len() as i64 - 1;
    let i = p.floor() as i64;
    let t = (p - i as f64) as f32;
    let at = |k: i64| x[(i + k).clamp(0, last) as usize];
    let (a, b, c, d) = (at(-1), at(0), at(1), at(2));
    b + 0.5 * t * (c - a + t * (2.0 * a - 5.0 * b + 4.0 * c - d + t * (3.0 * (b - c) + d - a)))
}

/// A gliding knob's value for this sample: what the strip turned (its
/// value, and how far it holds, 0..1) over the instrument's `own`; the
/// cutoff blends in octaves.
fn over(own: f32, (value, on): (f32, f32), octaves: bool) -> f32 {
    match () {
        _ if on <= 0.0 => own,
        _ if on >= 1.0 => value,
        _ if octaves => own * libm::powf(value / own, on),
        _ => own + (value - own) * on,
    }
}

/// What a strip turns for one sample: each `GLIDING` word's value as the
/// voice reads it (the cutoff in Hz) and how far it holds.
type Turned = [(f32, f32); GLIDING.len()];

/// The natural logarithm of 1000: an exponential fall (`curve=exp`) of
/// this many e-folds is one of 60 dB.
const LN_1000: f32 = 6.907_755_3;

/// The last stretch of an exponential fall, in seconds (at most all of
/// it), laid down to its target in a straight line, so it is there at its
/// time without the step from -60 dB to nothing.
const TAPER: f32 = 0.002;

/// How far an exponential fall (`curve=exp`) of `seconds` has still to go
/// at `x` of its time (from 1 at 0 to 0 at 1): 10^(-3x), -60 dB at its
/// time, but for its last `TAPER` seconds, which go straight to 0.
fn fallen(x: f32, seconds: f32) -> f32 {
    if x >= 1.0 {
        return 0.0;
    }
    let left = libm::expf(-LN_1000 * x);
    let taper = (TAPER / seconds).min(1.0);
    if x > 1.0 - taper { left * (1.0 - x) / taper } else { left }
}

/// A drive's gain `t` seconds into a note: `gain` as it began, moved by
/// its sweep (the ratio it reaches, over how long) evenly in decibels.
fn driven(gain: f32, sweep: Option<(f32, f32)>, t: f32) -> f32 {
    match sweep {
        Some((ratio, span)) => gain * libm::powf(ratio, (t / span).min(1.0)),
        None => gain,
    }
}

/// What a soft clipper at `gain` raises a voice's vowels by (`SUNG` their
/// level, or the clear voice's), and so its noise joining past it.
fn raised(gain: f32, vowels: f32) -> f32 {
    libm::tanhf(gain * vowels) / vowels
}

impl Tone {
    /// Takes up a note of layer `ins` from its start, the words a strip
    /// turned from the next note (`next`) laid over it, a word only where
    /// the layer has its part (a fixed pitch, a pitch's start, a low-pass,
    /// a cutoff's sweep, a crusher); `cutoff` is where its strip has the
    /// cutoff now, from which a turned `cutoff_end` sweeps.
    fn take_up(&mut self, ins: &Instrument, next: &[Option<f32>; NEXT_NOTE.len()], cutoff: f32) {
        self.osc.phase = 0.0;
        // Unison's oscillators begin spread over their wave's turn (by the
        // golden ratio), so no edge of theirs falls together on the beat.
        for (i, o) in self.unison.iter_mut().enumerate() {
            o.phase = ((i + 1) as f32 * 0.618_034).fract();
        }
        self.detuned = f32::NAN;
        let word = |w: usize, own: f32| next[w].unwrap_or(own);
        let part = |w: usize, own: Option<f32>| own.map(|o| word(w, o));
        let (attack, decay) = (word(at::ATTACK, ins.attack), word(at::DECAY, ins.decay));
        let cutoff_sweep = ins.cutoff_end.map(|end| {
            let depth = match next[at::CUTOFF_END] {
                Some(turned) => turned / cutoff,
                None => end / ins.cutoff,
            };
            (depth, next[at::CUTOFF_TIME].or(ins.cutoff_time).unwrap_or(attack + decay).max(1e-3))
        });
        // The drive's gain, sliding evenly in dB where it has an end: then
        // the clipper works the whole note, 0 dB its gentlest.
        let (drive, drive_end) = (word(at::DRIVE, ins.drive), part(at::DRIVE_END, ins.drive_end));
        self.drive = if drive > 0.0 || drive_end.is_some() { libm::powf(10.0, drive / 20.0) } else { 0.0 };
        let drive_sweep = drive_end.map(|end| (libm::powf(10.0, (end - drive) / 20.0), next[at::DRIVE_TIME].or(ins.drive_time).unwrap_or(attack + decay).max(1e-3)));
        self.shape = Shape {
            attack,
            decay,
            release: word(at::RELEASE, ins.release),
            curve: ins.curve,
            freq: part(at::FREQ, ins.freq),
            // A pitch that falls from its start slides no more.
            sweep: if ins.pitch_start.is_some() { 0.0 } else { word(at::SWEEP, ins.sweep) },
            sweep_span: next[at::SWEEP_TIME].or(ins.sweep_time).unwrap_or(attack + decay).max(1e-3),
            pitch_start: part(at::PITCH_START, ins.pitch_start),
            pitch_decay: word(at::PITCH_DECAY, ins.pitch_decay),
            cutoff_sweep,
            drive_sweep,
            vibrato_speed: word(at::VIBRATO_SPEED, ins.vibrato_speed),
            vibrato_delay: word(at::VIBRATO_DELAY, ins.vibrato_delay),
            pwm_speed: word(at::PWM_SPEED, ins.pwm_speed),
            arp_speed: word(at::ARP_SPEED, ins.arp_speed),
            sidechain: word(at::SIDECHAIN, ins.sidechain),
            sidechain_release: word(at::SIDECHAIN_RELEASE, ins.sidechain_release),
            root: 0.0,
            stretch: (0, 0),
        };
        // A sample's stretch, by the frames its recording has (a stretch
        // a strip turned past them cut at its end, an empty one silent).
        if let Some(s) = &ins.sample {
            let frames = s.recording.as_ref().map_or(0, |r| r.frames.len());
            let frame_of = |seconds: f32| ((f64::from(seconds) * SAMPLE_RATE as f64).round().max(0.0) as usize).min(frames);
            let first = frame_of(word(at::START, s.start));
            let end = next[at::END].or(s.end).map_or(frames, frame_of).max(first);
            (self.shape.root, self.shape.stretch, self.head) = (word(at::ROOT, s.root).max(1.0), (first, end - first), 0.0);
        }
        self.alpha = part(at::LOWPASS, ins.lowpass).map_or(0.0, OnePole::alpha);
        let bits = ins.crush_bits.map(|b| next[at::CRUSH_BITS].map_or(b, |v| v as u32));
        self.crush.set(part(at::CRUSH_RATE, ins.crush_rate), bits);
    }

    /// The envelope's level `t` seconds into the note, its key let go
    /// `gate` seconds in (None: held), the level held after the decay
    /// `sustain`, a voice's attack starting from `from`; None once it has
    /// rung out.
    fn envelope(&mut self, t: f32, gate: Option<f32>, sustain: f32, from: Option<f32>) -> Option<f32> {
        let shape = self.shape;
        let level = match gate {
            None => {
                let l = if t < shape.attack {
                    match from {
                        Some(from) => from + (1.0 - from) * (t / shape.attack.max(1e-4)),
                        None => t / shape.attack.max(1e-4),
                    }
                } else {
                    let d = (t - shape.attack) / shape.decay.max(1e-4);
                    match shape.curve {
                        Curve::Linear => {
                            if d < 1.0 { 1.0 + (sustain - 1.0) * d } else { sustain }
                        }
                        Curve::Exp => sustain + (1.0 - sustain) * fallen(d, shape.decay),
                    }
                };
                self.released_at = l;
                l
            }
            // Released when the key was let go, however far into the
            // release the song was when it was found let go.
            Some(gate) => {
                let r = (t - gate) / shape.release.max(1e-4);
                if r >= 1.0 {
                    self.level = 0.0;
                    return None;
                }
                match shape.curve {
                    Curve::Linear => self.released_at * (1.0 - r),
                    Curve::Exp => self.released_at * fallen(r, shape.release),
                }
            }
        };
        self.level = level;
        Some(level)
    }

    /// The pitch `t` seconds into note `n`: the note's (or a fixed one), a
    /// sweep or a fall from a start, the arpeggio, `vibrato` deep.
    fn pitch(&self, ins: &Instrument, n: &Note, t: f32, vibrato: f32) -> f32 {
        let shape = &self.shape;
        let mut freq = shape.freq.unwrap_or(n.freq.unwrap_or(0.0));
        if let Some(start) = shape.pitch_start {
            freq += (start - freq) * libm::expf(-t / shape.pitch_decay);
        }
        if shape.sweep != 0.0 {
            freq *= libm::powf(2.0, shape.sweep * (t / shape.sweep_span).min(1.0) / 12.0);
        }
        if !ins.arp.is_empty() {
            let step = (t * shape.arp_speed) as usize % ins.arp.len();
            freq *= libm::powf(2.0, ins.arp[step] / 12.0);
        }
        if vibrato > 0.0 && t > shape.vibrato_delay {
            // The clear voice's vibrato grows in over a third of a second, as a singer's.
            let depth = if ins.voice == Some(Voicing::Clear) { vibrato * ((t - shape.vibrato_delay) / 0.3).min(1.0) } else { vibrato };
            freq *= 1.0 + depth * libm::sinf(std::f32::consts::TAU * shape.vibrato_speed * (t - shape.vibrato_delay));
        }
        freq
    }

    /// A pulse's duty `t` seconds into the note, its width wobbling `pwm` deep.
    fn duty(&self, t: f32, duty: f32, pwm: f32) -> f32 {
        (duty + pwm * libm::sinf(std::f32::consts::TAU * self.shape.pwm_speed * t)).clamp(0.05, 0.95)
    }

    /// Sample `s` of its wave through its drive, the noise of a voice
    /// joining past it (`noise`), and its filters, the SID's at `cutoff`
    /// (swept by the note) and `resonance`.
    fn color(&mut self, ins: &Instrument, s: f32, noise: Option<f32>, t: f32, cutoff: f32, resonance: f32) -> f32 {
        let mut s = s;
        // Driven into a soft clipper before the filters, as an amp
        // before its speaker; a wave not even above and below zero
        // (a voice's) clips to a middle off zero, taken off again.
        if self.drive > 0.0 {
            let x = libm::tanhf(s * driven(self.drive, self.shape.drive_sweep, t));
            s = x - self.dc.0 + DC_BLOCK * self.dc.1;
            self.dc = (x, s);
        }
        // A voice's breath and hiss join past the clipper, raised as
        // much as it raises a voice, so they keep their distance
        // from the vowels however hard it drives.
        if let Some(noise) = noise {
            s += noise;
        }
        if ins.lowpass.is_some() {
            s = self.low.next(s, self.alpha);
        }
        if let Some(mode) = ins.filter {
            // The cutoff swept per note, evenly in octaves, from where
            // the strip has it as deep as the note's sweep goes.
            let mut cutoff = cutoff;
            if let Some((depth, span)) = self.shape.cutoff_sweep {
                cutoff *= libm::powf(depth, (t / span).min(1.0));
            }
            s = self.filter.process(s, cutoff, resonance, mode);
        }
        s
    }

    /// `s` at its envelope's `level`, crushed after the envelope, before
    /// the volumes: its steps are the layer's own.
    fn crushed(&mut self, s: f32, level: f32) -> f32 {
        let mut s = s * level;
        if self.crush.on() {
            s = self.crush.next(s);
        }
        s
    }

    /// The next sample of a further layer `ins` of note `n` (`t`, `gate`
    /// as `envelope`'s), before its volume; None once it has rung out.
    fn next(&mut self, ins: &Instrument, n: &Note, t: f32, gate: Option<f32>) -> Option<f32> {
        let level = self.envelope(t, gate, ins.sustain, None)?;
        let freq = self.pitch(ins, n, t, ins.vibrato);
        let duty = self.duty(t, ins.duty, ins.pwm);
        let s = self.source(ins, freq, duty, ins.detune)?;
        let s = self.color(ins, s, None, t, ins.cutoff, ins.resonance);
        Some(self.crushed(s, level))
    }

    /// The next sample of its source at `freq`: its wave's (its unison's,
    /// `detune` apart), or its recording's (None once a stretch that does
    /// not go round has played out, or where there is no recording to
    /// play).
    fn source(&mut self, ins: &Instrument, freq: f32, duty: f32, detune: f32) -> Option<f32> {
        match &ins.sample {
            None if ins.unison > 1 => Some(self.unison(ins, freq / SAMPLE_RATE as f32, duty, detune)),
            None => Some(self.osc.next(ins.wave, freq / SAMPLE_RATE as f32, duty, false)),
            Some(s) => self.play(s, freq),
        }
    }

    /// The next sample of `ins.unison` oscillators of its wave, `step` of
    /// a turn on, spread evenly from `detune` semitones below to as far
    /// above, their sum as loud as one (over the root of how many: they
    /// beat apart, uncorrelated).
    fn unison(&mut self, ins: &Instrument, step: f32, duty: f32, detune: f32) -> f32 {
        let n = (ins.unison as usize).min(UNISON);
        if detune != self.detuned {
            for (i, r) in self.ratios[..n].iter_mut().enumerate() {
                *r = ratio(f64::from(detune) * (2.0 * i as f64 / (n - 1) as f64 - 1.0));
            }
            self.detuned = detune;
        }
        let mut s = self.osc.next(ins.wave, step * self.ratios[0], duty, false);
        for (o, r) in self.unison[..n - 1].iter_mut().zip(&self.ratios[1..n]) {
            s += o.next(ins.wave, step * r, duty, false);
        }
        s / libm::sqrtf(n as f32)
    }

    /// The next frame of sample `s` at `freq`: its recording read at `freq`
    /// over its root's speed (higher faster, no time stretched), between
    /// frames by a cubic. A stretch that goes round fades its last
    /// `LOOP_FADE` into its first as it comes round (after the first time
    /// it begins that far in), so the seam does not click; one that does
    /// not fades out over its last `END_FADE` of playing and is silent.
    fn play(&mut self, s: &super::song::Sampled, freq: f32) -> Option<f32> {
        let frames = &s.recording.as_deref()?.frames;
        let (first, length) = self.shape.stretch;
        if length < 2 {
            return None;
        }
        let speed = f64::from((freq / self.shape.root).max(0.0));
        let len = length as f64;
        let at = |q: f64| if s.reverse { (first + length - 1) as f64 - q } else { first as f64 + q };
        let q = self.head;
        let out = if s.looping {
            let fade = (LOOP_FADE * SAMPLE_RATE as f64).min(len / 2.0);
            let mut v = cubic(frames, at(q));
            if q >= len - fade {
                let w = ((q - (len - fade)) / fade) as f32;
                v += (cubic(frames, at(q - (len - fade))) - v) * w;
            }
            self.head += speed;
            if self.head >= len {
                self.head = fade + (self.head - fade) % (len - fade);
            }
            v
        } else {
            let left = len - 1.0 - q;
            if left <= 0.0 {
                return None;
            }
            self.head += speed;
            let fading = END_FADE * SAMPLE_RATE as f32 * speed as f32;
            cubic(frames, at(q)) * (left as f32 / fading).min(1.0)
        };
        Some(out)
    }

    /// Its sample's read head `samples` heard into a note of `n` (a note
    /// taken up after its start, by a jump), as at the note's own pitch.
    fn skip(&mut self, ins: &Instrument, n: &Note, samples: u64) {
        if let Some(s) = &ins.sample {
            let len = self.shape.stretch.1 as f64;
            let speed = f64::from(self.shape.freq.unwrap_or(n.freq.unwrap_or(0.0)) / self.shape.root);
            let mut head = samples as f64 * speed;
            if s.looping && head >= len && len >= 2.0 {
                let fade = (LOOP_FADE * SAMPLE_RATE as f64).min(len / 2.0);
                head = fade + (head - fade) % (len - fade);
            }
            self.head = head;
        }
    }
}

impl Summed {
    /// Takes up a note whose layers go through `sum`, the words a strip
    /// turned for the sum from the next note (`next`, the rest None) laid
    /// over its own; its drive slides over `span` unless it says how long
    /// (the instrument's own line's attack and decay).
    fn take_up(&mut self, sum: &Sum, next: &[Option<f32>; NEXT_NOTE.len()], span: f32) {
        let drive = next[at::DRIVE].unwrap_or(sum.drive);
        let end = sum.drive_end.map(|own| next[at::DRIVE_END].unwrap_or(own));
        self.drive = if drive > 0.0 || end.is_some() { libm::powf(10.0, drive / 20.0) } else { 0.0 };
        self.drive_sweep = end.map(|end| (libm::powf(10.0, (end - drive) / 20.0), next[at::DRIVE_TIME].or(sum.drive_time).unwrap_or(span).max(1e-3)));
        self.crush.set(sum.crush_rate, sum.crush_bits);
    }

    /// The layers' sum `s` `t` seconds into the note through the soft
    /// clipper (as a layer's own drive, its DC taken off again), the
    /// filter at `cutoff` and `resonance`, the crusher and the volume.
    fn next(&mut self, s: f32, sum: &Sum, cutoff: f32, resonance: f32, t: f32) -> f32 {
        let mut s = s;
        if self.drive > 0.0 {
            let x = libm::tanhf(s * driven(self.drive, self.drive_sweep, t));
            s = x - self.dc.0 + DC_BLOCK * self.dc.1;
            self.dc = (x, s);
        }
        if let Some(mode) = sum.filter {
            s = self.filter.process(s, cutoff, resonance, mode);
        }
        if self.crush.on() {
            s = self.crush.next(s);
        }
        s * sum.volume
    }
}

impl Voice {
    fn new(seed: u32, letter: char) -> Self {
        let strip = letter_index(letter).unwrap_or(0) as u8;
        // Each layer's noise of its own.
        let tone = |layer: u32| Tone { osc: Oscillator::new(0x1234_5678 ^ seed ^ layer.wrapping_mul(0x9e37_79b9)), ..Tone::default() };
        Voice {
            tone: tone(0),
            more: std::array::from_fn(|i| tone(i as u32 + 1)),
            singer: Singer::new(0x2545_f491 ^ seed),
            clear: clear::Singer::new(0x5bd1_e995 ^ seed),
            strip,
            ..Voice::default()
        }
    }

    /// Takes up note `n` (played by `ins`) from its start, at sample
    /// `time` of the song; a voice note sings its part of `segments`
    /// (`voice`'s) or `clear`. Its strip's words from the next note
    /// (`next`) are laid over its first layer's (`Tone::take_up`), the
    /// drive over the sum's where the sum states one; `cutoff` is where
    /// its strip has the cutoff now.
    fn take_up(&mut self, n: &Note, ins: &Instrument, (segments, clear): (&[Segment], &[Seg]), time: u64, (next, cutoff): (&[Option<f32>; NEXT_NOTE.len()], f32)) {
        let sounding = self.note.is_some();
        (self.note, self.started) = (Some(*n), n.start);
        (self.age, self.gate) = (time.wrapping_sub(n.start), None);
        // A word the sum states the strip turns there, not on the first layer.
        let sum = ins.sum.as_ref();
        let (mut first, mut to_sum) = (*next, [None; NEXT_NOTE.len()]);
        if sum.is_some_and(|s| s.drive_stated) {
            for w in [at::DRIVE, at::DRIVE_END, at::DRIVE_TIME] {
                to_sum[w] = first[w].take();
            }
        }
        let cutoff = if sum.is_some_and(|s| s.cutoff_stated) { ins.cutoff } else { cutoff };
        self.tone.take_up(ins, &first, cutoff);
        for (tone, layer) in self.more.iter_mut().zip(&ins.layers) {
            tone.take_up(layer, &[None; NEXT_NOTE.len()], layer.cutoff);
        }
        // A note taken up after its start (a jump into the song): its
        // samples read from as far in.
        if (1..u64::from(u32::MAX)).contains(&self.age) {
            self.tone.skip(ins, n, self.age);
            for (tone, layer) in self.more.iter_mut().zip(&ins.layers) {
                tone.skip(layer, n, self.age);
            }
        }
        if let Some(sum) = sum {
            self.sum.take_up(sum, &to_sum, self.tone.shape.attack + self.tone.shape.decay);
        }
        let angle = (n.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        self.pan = (libm::cosf(angle), libm::sinf(angle));
        let drive = self.tone.drive;
        self.noise_gain = if drive > 0.0 { raised(drive, vowels(ins)) } else { 1.0 };
        if ins.consonants != 0.0 {
            self.noise_gain *= libm::powf(10.0, ins.consonants / 20.0);
        }
        if ins.voice.is_some() {
            self.from = if sounding { self.tone.level } else { 0.0 };
        }
        match ins.voice {
            Some(Voicing::Formants) => self.singer.start(sung(n, segments), !sounding),
            Some(Voicing::Clear) => self.clear.start(sung(n, clear), !sounding),
            None => {}
        }
    }

    /// The next frame of the note it plays at sample `time` of the song,
    /// ducked by `ducked` and scaled by the song's `volume`, a jump's
    /// `fade` and the channel's layer (`loud`), its strip's gliding words
    /// laid over its first layer's (the sum's cutoff and resonance where
    /// the sum states them) where it turns any (`turned`); its layers
    /// added and through their sum (`Summed`), if it has any; None once
    /// every layer is silent.
    fn sound(&mut self, song: &Song, time: u64, ducked: f32, (volume, fade, loud): (f32, f32, f32), turned: Option<&Turned>) -> Option<[f32; 2]> {
        let sr = SAMPLE_RATE as f32;
        let n = self.note?;
        let ins = song.instruments.get(n.instrument)?;
        let knob = |w: usize, own: f32| turned.map_or(own, |k| over(own, k[w], w == CUTOFF));
        let sum = ins.sum.as_ref();
        let (sum_cutoff, sum_resonance) = sum.map_or((false, false), |s| (s.cutoff_stated, s.resonance_stated));
        // The key is held for a length of the song; the sound's shape goes
        // by the samples heard (the same at the song's own tempo).
        let into = time.wrapping_sub(self.started);
        let (age, t) = (self.age, self.age as f32 / sr);
        self.age += 1;
        let held = n.held as f32 / sr;
        let gate = if (into as f32 / sr) < held { None } else { Some(*self.gate.get_or_insert(age - age.min(into.saturating_sub(n.held))) as f32 / sr) };

        // The instrument's own line (a voice's attack from where it was).
        let from = ins.voice.is_some().then_some(self.from);
        let first = match self.tone.envelope(t, gate, knob(at::SUSTAIN, ins.sustain), from) {
            None => None,
            Some(level) => {
                let tone = &mut self.tone;
                let freq = tone.pitch(ins, &n, t, knob(at::VIBRATO, ins.vibrato));
                let duty = tone.duty(t, knob(at::DUTY, ins.duty), knob(at::PWM, ins.pwm));
                let sourced = match ins.voice {
                    Some(Voicing::Formants) => Some(self.singer.next(sung(&n, &song.segments), time.wrapping_sub(self.started), freq)),
                    Some(Voicing::Clear) => Some(self.clear.next(sung(&n, &song.clear), time.wrapping_sub(self.started), freq)),
                    // (A sample played out is silent from here.)
                    None => tone.source(ins, freq, duty, knob(at::DETUNE, ins.detune)).map(|s| (s, 0.0)),
                };
                sourced.map(|(s, noise)| {
                    // A drive that slides raises the noise as it goes.
                    let raise = match tone.shape.drive_sweep {
                        Some(sweep) if ins.voice.is_some() => raised(driven(tone.drive, Some(sweep), t), vowels(ins)) / raised(tone.drive, vowels(ins)),
                        _ => 1.0,
                    };
                    let noise = ins.voice.is_some().then_some(noise * self.noise_gain * raise);
                    let cutoff = if sum_cutoff { ins.cutoff } else { knob(CUTOFF, ins.cutoff) };
                    let resonance = if sum_resonance { ins.resonance } else { knob(at::RESONANCE, ins.resonance) };
                    let s = tone.color(ins, s, noise, t, cutoff, resonance);
                    tone.crushed(s, level)
                })
            }
        };
        // Ducked under a sidechain note, unless this is one.
        let duck = if self.tone.shape.sidechain > 0.0 { 1.0 } else { 1.0 - ducked * knob(at::DUCK, ins.duck) };
        let gain = if sum.is_none() && ins.layers.is_empty() {
            let Some(s) = first else {
                self.note = None;
                return None;
            };
            s * ins.volume * n.volume * volume * duck * fade * loud
        } else {
            // Every layer at its volume, added, then through the sum.
            let mut sounding = first.is_some();
            let mut s = first.map_or(0.0, |s| s * ins.volume);
            for (tone, layer) in self.more.iter_mut().zip(&ins.layers) {
                if let Some(l) = tone.next(layer, &n, t, gate) {
                    s += l * layer.volume;
                    sounding = true;
                }
            }
            if !sounding {
                self.note = None;
                return None;
            }
            if let Some(sum) = sum {
                let cutoff = if sum_cutoff { knob(CUTOFF, sum.cutoff) } else { sum.cutoff };
                let resonance = if sum_resonance { knob(at::RESONANCE, sum.resonance) } else { sum.resonance };
                s = self.sum.next(s, sum, cutoff, resonance, t);
            }
            s * n.volume * volume * duck * fade * loud
        };
        // A note giving way to the next fades out under it.
        let gain = match self.fading {
            0 => gain,
            1 => {
                (self.fading, self.note) = (0, None);
                return None;
            }
            _ => {
                self.fading -= 1;
                gain * self.fading as f32 / GIVE_WAY as f32
            }
        };
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
    /// At `rate` (Hz) and to `bits`, each off when None.
    fn set(&mut self, rate: Option<f32>, bits: Option<u32>) {
        self.step = rate.map_or(0.0, |r| (r / SAMPLE_RATE as f32).min(1.0));
        self.steps = bits.map_or(0.0, |b| libm::powf(2.0, (b as i32 - 1) as f32));
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
    pub(super) cut: f32,
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

/// The ducking the last sidechain note started: since when (the samples
/// heard, `Player::heard`: a compressor's release is seconds), how deep,
/// how long.
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

/// What keeping time with a game's clock did (`Player::follow`): how far
/// off the song was, in milliseconds heard (ahead of the clock below 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Followed {
    /// Caught up by going a little faster or slower, unheard.
    Nudged(f64),
    /// Too far off: it jumped to where the clock is.
    Jumped(f64),
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

/// The vowels' typical level of the voice `ins` sings with.
fn vowels(ins: &Instrument) -> f32 {
    if ins.voice == Some(Voicing::Clear) { clear::SUNG } else { SUNG }
}

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

/// One of a strip's gliding knobs as the player applies it: its value (log2
/// Hz for the cutoff, and that in Hz as last worked out) and how far it
/// holds over the instrument's, each gliding sample by sample, and the
/// asks last followed.
#[derive(Clone, Copy, Debug)]
struct KnobLane {
    value: Ramp,
    on: Ramp,
    hz: (f32, f32),
    asked: [(u32, u32, u32); 2],
}

impl KnobLane {
    fn at(k: &Knob, shortest: f32) -> Self {
        let mut lane = KnobLane { value: Ramp::at(k.value.now()), on: Ramp::at(k.on.now()), hz: (f32::NAN, 0.0), asked: [k.value.key(), k.on.key()] };
        lane.value.toward(k.value.to, k.value.left(), shortest);
        lane.on.toward(k.on.to, k.on.left(), shortest);
        lane
    }

    /// Follows a new ask, from where it sounds now (a knob coming in over
    /// the instrument's own from its value at once).
    fn follow(&mut self, k: &Knob, shortest: f32) {
        if k.value.key() != self.asked[0] {
            if self.on.now == 0.0 {
                self.value = Ramp::at(k.value.to);
            } else {
                self.value.toward(k.value.to, k.value.left(), shortest);
            }
        }
        if k.on.key() != self.asked[1] {
            self.on.toward(k.on.to, k.on.left(), shortest);
        }
        self.asked = [k.value.key(), k.on.key()];
    }

    /// Whether it holds over the instrument's now or is to.
    fn live(&self) -> bool {
        self.on.now != 0.0 || self.on.to != 0.0
    }

    /// Its value as the voice reads it and how far it holds, for this
    /// sample, and a step on; the cutoff from log2 Hz.
    fn next(&mut self, octaves: bool) -> (f32, f32) {
        let (value, on) = (self.value.next(), self.on.next());
        if !octaves {
            return (value, on);
        }
        if self.hz.0 != value {
            self.hz = (value, libm::exp2f(value));
        }
        (self.hz.1, on)
    }
}

/// A channel's strip as the player applies it (`Mix::strips`): its level
/// (the fader, or 0 muted) and its pan, each gliding sample by sample, the
/// asks last followed (`Glide::key`), its gliding knobs, whether any
/// holds, and its words from the next note as asked.
#[derive(Clone, Copy, Debug)]
pub(super) struct Lane {
    pub(super) level: Ramp,
    pub(super) pan: Ramp,
    asked: [(u32, u32, u32); 2],
    knobs: [KnobLane; GLIDING.len()],
    turning: bool,
    next: [Option<f32>; NEXT_NOTE.len()],
}

/// The shortest glide of `GLIDING`'s word `w`: the cutoff's as the
/// muffle's, the rest as the loudness's.
fn shortest(w: usize) -> f32 {
    if w == CUTOFF { SHORTEST.1 } else { SHORTEST.0 }
}

impl Lane {
    /// As the game's strip is now, gliding on where it still glides.
    fn at(strip: &Strip) -> Self {
        let (level, pan) = (&strip.level, &strip.pan);
        let knobs = std::array::from_fn(|w| KnobLane::at(&strip.knobs[w], shortest(w)));
        let mut lane = Lane { level: Ramp::at(level.now()), pan: Ramp::at(pan.now()), asked: [level.key(), pan.key()], knobs, turning: false, next: strip.next };
        lane.level.toward(level.to, level.left(), SHORTEST.0);
        lane.pan.toward(pan.to, pan.left(), SHORTEST.0);
        lane.turning = lane.knobs.iter().any(KnobLane::live);
        lane
    }

    /// Follows a new ask, from where it sounds now.
    fn follow(&mut self, strip: &Strip) {
        let (level, pan) = (&strip.level, &strip.pan);
        if level.key() != self.asked[0] {
            self.level.toward(level.to, level.left(), SHORTEST.0);
        }
        if pan.key() != self.asked[1] {
            self.pan.toward(pan.to, pan.left(), SHORTEST.0);
        }
        self.asked = [level.key(), pan.key()];
        for (w, (lane, k)) in self.knobs.iter_mut().zip(&strip.knobs).enumerate() {
            lane.follow(k, shortest(w));
        }
        self.turning = self.knobs.iter().any(KnobLane::live);
        self.next = strip.next;
    }

    /// Whether it sounds as written and stays so (nothing to apply).
    fn neutral(&self) -> bool {
        (self.level.now, self.level.to, self.pan.now, self.pan.to) == (1.0, 1.0, 0.0, 0.0)
    }

    /// Its gliding knobs for this sample, and a step on.
    fn turn(&mut self) -> Turned {
        std::array::from_fn(|w| self.knobs[w].next(w == CUTOFF))
    }

    /// What a note taking up its strip's words is given: its words from
    /// the next note, and where its cutoff is now over the instrument's
    /// `own`.
    fn take_up(&self, own: f32) -> (&[Option<f32>; NEXT_NOTE.len()], f32) {
        let k = &self.knobs[CUTOFF];
        let cutoff = if self.turning { over(own, (libm::exp2f(k.value.now), k.on.now), true) } else { own };
        (&self.next, cutoff)
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
    /// Each channel's note giving way to the next, fading out under it
    /// (`give_way`), and what is left of notes that had nowhere to fade:
    /// made with the song too.
    pub(super) giving: Vec<Voice>,
    residue: Residue,
    pub(super) time: u64,
    pub(super) duck: Option<Duck>,
    pub volume: f32,
    /// How fast its time goes against the samples (1: as written), nudged
    /// to keep with a game that reads its beats (`follow`); what is left
    /// of a sample (where the song went round between two samples, or a
    /// little before its start: below 0 while `time` is 0).
    pub(super) rate: f64,
    pub(super) carry: f64,
    /// How fast the song goes against its `#tempo` for the game
    /// (`music.tempo`; exactly 1 unless set): its notes' starts, lengths,
    /// its loop, its swaps and its layers' fades go so, each sound's own
    /// shape (`Voice::age`) and the ducking by the samples heard.
    pub(super) ratio: f64,
    /// The samples it made: the ducking's clock.
    pub(super) heard: u64,
    /// Samples into the song as written played on and on, never folded
    /// round its loop (the game's beats, `follow`): where a swap is due.
    pub(super) position: f64,
    /// Where a game's beat 0 lies, in samples as written past the song's
    /// own (`rebase`): 0 for a song begun at its clock; a song that plays
    /// on into a run begun anew keeps sounding, its new clock's beat 0
    /// where it was then.
    base: f64,
    /// Channels a game laid, to swap in when `position` reaches their due
    /// point (`pend`), and those swapped out, for the main thread to drop
    /// (the audio thread gives no memory back).
    pub(super) pending: Option<Box<Swap>>,
    pub(in crate::audio) spent: Option<Box<Swap>>,
    /// Samples since a jump, fading in up to `FADE_IN`.
    pub(in crate::audio) since_jump: u32,
    /// The game's tension (`music.intensity`), and how loud each channel's
    /// layer is now (0..1, moving a bar's worth a bar).
    pub intensity: f32,
    pub(super) loud: Vec<f32>,
    /// The game's say over how it sounds (`Mix`): neutral unless followed.
    pub(super) bus: Bus,
    /// Each channel's strip by letter (`Mix::strips`), made with the
    /// player, kept through a swap; whether any is not neutral, and each
    /// one's gains for the sample being made (the strips boxed: a mixer
    /// holds seven players, and they are most of one).
    pub(super) lanes: Box<[Lane; LETTERS]>,
    lanes_live: bool,
    gains: [[f32; 2]; LETTERS],
    /// Whether any strip turns a gliding knob (`Lane::turning`), and each
    /// one's knobs for the sample being made.
    knobs_live: bool,
    turned: [Turned; LETTERS],
    /// What song it is, for the one who hands it to tell it from another
    /// (`identified`): None, never the same as any.
    identity: Option<[u8; 32]>,
}

impl Player {
    pub fn title(&self) -> &str {
        &self.song.title
    }

    /// Named `identity` (a digest of the song's file, its text and the
    /// channels laid over it): handed while a player of the same plays, it
    /// is that song again.
    pub fn identified(mut self, identity: [u8; 32]) -> Self {
        self.identity = Some(identity);
        self
    }

    pub fn identity(&self) -> Option<[u8; 32]> {
        self.identity
    }

    pub fn new(song: Song, volume: f32) -> Self {
        let voices = (0..song.channels.len()).map(|i| Voice::new(i as u32 * 7919, song.names[i])).collect();
        let tails = (0..song.channels.len()).map(|_| Voice::default()).collect();
        let giving = (0..song.channels.len()).map(|_| Voice::default()).collect();
        let loud = song.layers.iter().map(|_| 1.0).collect();
        let lane = Lane::at(&Strip::default());
        Self {
            song,
            voices,
            tails,
            giving,
            residue: Residue::default(),
            time: 0,
            duck: None,
            volume,
            rate: 1.0,
            carry: 0.0,
            ratio: 1.0,
            heard: 0,
            position: 0.0,
            base: 0.0,
            pending: None,
            spent: None,
            since_jump: FADE_IN,
            intensity: 1.0,
            loud,
            bus: Bus::default(),
            lanes: Box::new([lane; LETTERS]),
            lanes_live: false,
            gains: [[1.0; 2]; LETTERS],
            knobs_live: false,
            turned: [[(0.0, 0.0); GLIDING.len()]; LETTERS],
            identity: None,
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
            *lane = Lane::at(&mix.strip(letter(i)));
        }
        self.lanes_live = self.lanes.iter().any(|l| !l.neutral());
        self.knobs_live = self.lanes.iter().any(|l| l.turning);
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
            lane.follow(&mix.strip(letter(i)));
        }
        self.lanes_live = self.lanes.iter().any(|l| !l.neutral());
        self.knobs_live = self.lanes.iter().any(|l| l.turning);
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
        for tail in self.tails.iter_mut().chain(&mut self.giving) {
            tail.note = None;
        }
        self.residue = Residue::default();
        for (v, notes) in self.voices.iter_mut().zip(&self.song.channels) {
            v.next = notes.partition_point(|n| n.start <= time);
            v.note = None;
            if let Some(n) = v.next.checked_sub(1).map(|i| &notes[i]).filter(|n| n.freq.is_some()) {
                let ins = &self.song.instruments[n.instrument];
                v.take_up(n, ins, (&self.song.segments, &self.song.clear), time, self.lanes[v.strip as usize].take_up(ins.cutoff));
            }
        }
    }

    /// Goes `ratio` times as fast as its `#tempo` (the game's tempo,
    /// `music.tempo`), from now: nothing laid anew, a sound already
    /// sounding rings on as it was.
    pub fn set_ratio(&mut self, ratio: f64) {
        (self.ratio, self.rate) = (ratio, ratio);
    }

    /// Keeps time with a game's clock, `beats` since the song began (on
    /// and on, which it folds into the song exactly, so a short loop
    /// never drifts from the game's bars): a jump when it is far off
    /// (after a pause), else a little faster or slower until it is with
    /// it again, unheard. How far off is measured in samples heard (the
    /// song's divided by its `ratio`), so a song played faster or slower
    /// jumps and closes in as it does at its own tempo. `ahead` seconds
    /// heard past those beats (a window's clock behind the time heard,
    /// `app::TickClock::behind`; 0 elsewhere). What it did.
    pub fn follow(&mut self, beats: f64, ahead: f64) -> Followed {
        let (unwrapped, off) = self.off(beats, ahead);
        let ms = off * 1000.0 / SAMPLE_RATE as f64;
        if off.abs() > JUMP as f64 {
            self.seek(unwrapped);
            self.rate = self.ratio;
            Followed::Jumped(ms)
        } else {
            self.rate = self.ratio * (1.0 + (off / (4.0 * SAMPLE_RATE as f64)).clamp(-0.003, 0.003));
            Followed::Nudged(ms)
        }
    }

    /// Where `beats` (and `ahead` seconds heard past them) are, in samples
    /// as written played on and on, and how far it is from there, in
    /// samples heard (ahead of the clock below 0).
    fn off(&self, beats: f64, ahead: f64) -> (f64, f64) {
        let (from, to) = self.song.span;
        let length = to - from;
        let unwrapped = self.at_beat(beats) + ahead.max(0.0) * SAMPLE_RATE as f64 * self.ratio + self.base;
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
        (unwrapped, off / self.ratio)
    }

    /// Takes a game's clock as it is now, at `beats` (and `ahead` seconds
    /// heard past them), to be where the song is: it plays on, unbroken,
    /// and keeps time with that clock from here (`follow`), exactly where
    /// it stood (a run begun anew, its clock at 0, asking the song that
    /// plays).
    pub fn rebase(&mut self, beats: f64, ahead: f64) {
        self.base = self.position - (self.at_beat(beats) + ahead.max(0.0) * SAMPLE_RATE as f64 * self.ratio);
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
        !self.song.looping && self.time as f64 + self.carry >= self.song.span.1 && self.voices.iter().chain(&self.tails).chain(&self.giving).all(|v| v.note.is_none()) && self.residue.left == 0
    }

    /// Lets go of every note that sounds as the song goes round or ends:
    /// it rings on in its channel's tail, the voice as it was (its filters,
    /// its noise) going on to the next round as when the note was cut
    /// there. A tail still ringing from the time before gives way to it,
    /// fading out (`give_way`'s slot; a note fading there already goes on
    /// as a residue).
    fn let_go(&mut self) {
        for ((v, tail), giving) in self.voices.iter_mut().zip(&mut self.tails).zip(&mut self.giving) {
            if let Some(n) = &mut v.note {
                n.held = n.held.min(self.time.wrapping_sub(v.started));
                if tail.note.is_some() {
                    if giving.note.is_some() {
                        self.residue.add(giving.last);
                    }
                    std::mem::swap(tail, giving);
                    giving.fading = GIVE_WAY;
                }
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
        for tail in self.tails.iter_mut().chain(&mut self.giving) {
            tail.started = tail.started.wrapping_sub(back);
        }
        for (v, notes) in self.voices.iter_mut().zip(&self.song.channels) {
            v.next = notes.partition_point(|n| (n.start as f64) < from);
        }
    }

    /// Adds the next frames to interleaved stereo `out`.
    pub fn mix(&mut self, out: &mut [f32]) {
        // The layers move towards the tension, a bar's worth a bar.
        let step = self.song.tempo / (60.0 * SAMPLE_RATE as f32 * self.song.meter as f32) * self.ratio as f32;
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
            let ducked = self.duck.map_or(0.0, |d| d.amount(self.heard));
            for (loud, &from) in self.loud.iter_mut().zip(&self.song.layers) {
                let want = if self.intensity >= from { 1.0 } else { 0.0 };
                *loud = if *loud < want { (*loud + step).min(want) } else { (*loud - step).max(want) };
            }
            let fade = if self.since_jump < FADE_IN { self.since_jump as f32 / FADE_IN as f32 } else { 1.0 };
            // Each channel's strip, where the game set one (`Mix::strips`).
            let live = self.lanes_live;
            if live {
                for (gain, lane) in self.gains.iter_mut().zip(self.lanes.iter_mut()) {
                    *gain = lane.next();
                }
            }
            let gains = &self.gains;
            let strip = |v: &Voice, [l, r]: [f32; 2]| if live { [l * gains[v.strip as usize][0], r * gains[v.strip as usize][1]] } else { [l, r] };
            // Each strip's knobs, where the game turns any.
            let turning = self.knobs_live;
            if turning {
                for (t, lane) in self.turned.iter_mut().zip(self.lanes.iter_mut()) {
                    if lane.turning {
                        *t = lane.turn();
                    }
                }
            }
            let (lanes, turned) = (&self.lanes, &self.turned);
            let knobs = |v: &Voice| (turning && lanes[v.strip as usize].turning).then(|| &turned[v.strip as usize]);
            let mut sum = [0.0f32; 2];
            for (ch, ((v, tail), giving)) in self.voices.iter_mut().zip(&mut self.tails).zip(&mut self.giving).enumerate() {
                let notes = &self.song.channels[ch];
                while v.next < notes.len() && notes[v.next].start <= self.time {
                    // Rests and the end marker have no pitch: the note before rings out.
                    let n = &notes[v.next];
                    if n.freq.is_some() {
                        let ins = &self.song.instruments[n.instrument];
                        give_way(v, ins, &self.song, self.time, giving, &mut self.residue);
                        v.take_up(n, ins, (&self.song.segments, &self.song.clear), self.time, lanes[v.strip as usize].take_up(ins.cutoff));
                        if v.tone.shape.sidechain > 0.0 {
                            self.duck = Some(Duck { from: self.heard, depth: v.tone.shape.sidechain, release: v.tone.shape.sidechain_release });
                        }
                    }
                    v.next += 1;
                }
                let scale = (self.volume, fade, self.loud[ch]);
                for x in [v, tail, giving] {
                    if let Some([l, r]) = x.sound(&self.song, self.time, ducked, scale, knobs(x)).map(|s| strip(x, s)) {
                        (x.last, sum[0], sum[1]) = ([l, r], sum[0] + l, sum[1] + r);
                    }
                }
            }
            // The notes of channels laid no more, ringing out.
            for tail in &mut self.tails[self.voices.len()..] {
                if let Some([l, r]) = tail.sound(&self.song, self.time, ducked, (self.volume, fade, 1.0), knobs(tail)).map(|s| strip(tail, s)) {
                    (tail.last, sum[0], sum[1]) = ([l, r], sum[0] + l, sum[1] + r);
                }
            }
            // What had nowhere left to fade, this sample's included.
            let [l, r] = self.residue.next();
            (sum[0], sum[1]) = (sum[0] + l, sum[1] + r);
            // As loud and as clear as the game asked (`Mix`).
            let (l, r) = self.bus.apply(sum[0], sum[1]);
            frame[0] += l;
            frame[1] += r;
            self.since_jump = self.since_jump.saturating_add(1);
            self.heard += 1;
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
            s.due = self.at_beat(s.due) + self.base;
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
                    std::mem::swap(&mut new.giving[i], &mut self.giving[j]);
                    new.loud[i] = self.loud[j];
                }
                None => new.loud[i] = if self.intensity >= new.song.layers[i] { 1.0 } else { 0.0 },
            }
        }
        // A channel laid no more rings out: its note and its tail where
        // there is room, the rest from its last frame (`Residue`); so do
        // the notes still ringing out of channels laid no more before.
        let mut ring_out = |v: &mut Voice, residue: &mut Residue| match () {
            _ if v.note.is_none() => {}
            _ if new.tails.len() < new.tails.capacity() => new.tails.push(std::mem::take(v)),
            _ => residue.add(v.last),
        };
        for (j, letter) in self.song.names.iter().enumerate() {
            if !new.song.names.contains(letter) {
                ring_out(&mut self.voices[j], &mut self.residue);
                ring_out(&mut self.tails[j], &mut self.residue);
                ring_out(&mut self.giving[j], &mut self.residue);
            }
        }
        for tail in &mut self.tails[self.voices.len()..] {
            ring_out(tail, &mut self.residue);
        }
        let same_span = new.song.span == self.song.span;
        std::mem::swap(&mut self.song, &mut new.song);
        std::mem::swap(&mut self.voices, &mut new.voices);
        std::mem::swap(&mut self.tails, &mut new.tails);
        std::mem::swap(&mut self.giving, &mut new.giving);
        std::mem::swap(&mut self.loud, &mut new.loud);
        // (`new` holds what played until now.)
        if !same_span {
            let at = self.within(self.position);
            let time = at.max(0.0) as u64;
            let shift = time.wrapping_sub(self.time);
            (self.time, self.carry) = (time, at - time as f64);
            for v in self.voices.iter_mut().chain(&mut self.tails).chain(&mut self.giving) {
                v.started = v.started.wrapping_add(shift);
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

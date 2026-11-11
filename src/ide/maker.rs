//! The sound maker over an open sound's text, in the sfxr family's way: a
//! sort's button (COIN, JUMP, SHOT, BLAST, HIT, POWER UP, BLIP) rolls a
//! fresh sound from that sort's small table of ranges, the same button again rolls another,
//! five faders move a word or two of the file each. A rolled sound is
//! rendered and measured before anyone hears it (`good`): only one that
//! passes comes back, so the dice cannot give a silent, endless or
//! inaudible sound. The text stays the one truth: every change is the
//! file's text rewritten (`rewrite`, through `toml_edit`, its comments
//! kept), which goes into the buffer as a typed change does.

use toml_edit::{DocumentMut, Item, Value};

use crate::audio::synth::{SAMPLE_RATE, Synth, Wave};
use crate::audio::words;

/// A sort of sound a button rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Coin,
    Jump,
    Shot,
    Blast,
    Hit,
    PowerUp,
    Blip,
}

impl Sort {
    pub const ALL: [Sort; 7] = [Sort::Coin, Sort::Jump, Sort::Shot, Sort::Blast, Sort::Hit, Sort::PowerUp, Sort::Blip];

    /// Its button's word.
    pub fn word(self) -> &'static str {
        match self {
            Sort::Coin => "COIN",
            Sort::Jump => "JUMP",
            Sort::Shot => "SHOT",
            Sort::Blast => "BLAST",
            Sort::Hit => "HIT",
            Sort::PowerUp => "POWER UP",
            Sort::Blip => "BLIP",
        }
    }

    /// What its button makes, in plain words.
    pub fn hover(self) -> &'static str {
        match self {
            Sort::Coin => "A fresh coin or pickup sound: a bright note that jumps up. It replaces this sound and plays.",
            Sort::Jump => "A fresh jump: a short note sliding up. It replaces this sound and plays.",
            Sort::Shot => "A fresh shot or laser: a quick note sliding down. It replaces this sound and plays.",
            Sort::Blast => "A fresh explosion: a burst of noise rumbling away. It replaces this sound and plays.",
            Sort::Hit => "A fresh hit or hurt: a short, harsh knock falling in pitch. It replaces this sound and plays.",
            Sort::PowerUp => "A fresh power-up: a longer note rising, now and then wobbling. It replaces this sound and plays.",
            Sort::Blip => "A fresh blip, for menus: a very short plain note. It replaces this sound and plays.",
        }
    }

    fn ranges(self) -> &'static Ranges {
        match self {
            Sort::Coin => &COIN,
            Sort::Jump => &JUMP,
            Sort::Shot => &SHOT,
            Sort::Blast => &BLAST,
            Sort::Hit => &HIT,
            Sort::PowerUp => &POWER_UP,
            Sort::Blip => &BLIP,
        }
    }
}

/// What a sort rolls from, in the file's words and units: each number
/// drawn between its two ends (pitches on a scale of octaves), a wave and a
/// jump picked from their lists. A word a sort leaves out stays its default.
struct Ranges {
    waves: &'static [Wave],
    /// Hz.
    freq: (f64, f64),
    /// Semitones.
    sweep: (f64, f64),
    /// A square wave's only.
    duty: (f64, f64),
    /// Seconds.
    hold: (f64, f64),
    decay: (f64, f64),
    punch: (f64, f64),
    /// Semitones, one picked; 0 no jump.
    jump: &'static [f64],
    /// Seconds from the start.
    jump_at: (f64, f64),
    vibrato: (f64, f64),
    vibrato_speed: (f64, f64),
    /// Hz, the filter's start and end, if the sort has one.
    lowpass: Option<((f64, f64), (f64, f64))>,
    volume: (f64, f64),
    /// The longest a sound of the sort may last, in seconds.
    longest: f32,
}

const NONE: (f64, f64) = (0.0, 0.0);

const COIN: Ranges = Ranges { waves: &[Wave::Square, Wave::Triangle], freq: (700.0, 1600.0), sweep: NONE, duty: (0.25, 0.5), hold: (0.03, 0.09), decay: (0.1, 0.3), punch: (0.2, 0.6), jump: &[4.0, 5.0, 7.0, 12.0], jump_at: (0.03, 0.08), vibrato: NONE, vibrato_speed: (6.0, 6.0), lowpass: None, volume: (0.25, 0.32), longest: 0.6 };
const JUMP: Ranges = Ranges { waves: &[Wave::Square], freq: (180.0, 480.0), sweep: (5.0, 14.0), duty: (0.2, 0.5), hold: (0.02, 0.07), decay: (0.06, 0.18), punch: (0.0, 0.2), jump: &[0.0], jump_at: NONE, vibrato: NONE, vibrato_speed: (6.0, 6.0), lowpass: None, volume: (0.18, 0.28), longest: 0.4 };
const SHOT: Ranges = Ranges { waves: &[Wave::Square, Wave::Saw], freq: (500.0, 1800.0), sweep: (-24.0, -8.0), duty: (0.2, 0.5), hold: (0.0, 0.03), decay: (0.06, 0.2), punch: (0.0, 0.4), jump: &[0.0], jump_at: NONE, vibrato: NONE, vibrato_speed: (6.0, 6.0), lowpass: None, volume: (0.16, 0.26), longest: 0.35 };
const BLAST: Ranges = Ranges { waves: &[Wave::Noise], freq: (500.0, 1800.0), sweep: (-30.0, -16.0), duty: (0.5, 0.5), hold: (0.03, 0.15), decay: (0.4, 1.0), punch: (0.5, 0.9), jump: &[0.0], jump_at: NONE, vibrato: NONE, vibrato_speed: (6.0, 6.0), lowpass: Some(((1500.0, 3500.0), (150.0, 400.0))), volume: (0.6, 0.9), longest: 1.3 };
const HIT: Ranges = Ranges { waves: &[Wave::Square, Wave::Saw], freq: (150.0, 500.0), sweep: (-18.0, -8.0), duty: (0.3, 0.5), hold: (0.0, 0.04), decay: (0.08, 0.2), punch: (0.3, 0.7), jump: &[0.0], jump_at: NONE, vibrato: NONE, vibrato_speed: (6.0, 6.0), lowpass: None, volume: (0.25, 0.34), longest: 0.35 };
const POWER_UP: Ranges = Ranges { waves: &[Wave::Square, Wave::Triangle], freq: (250.0, 600.0), sweep: (12.0, 24.0), duty: (0.3, 0.5), hold: (0.1, 0.25), decay: (0.15, 0.35), punch: NONE, jump: &[0.0], jump_at: NONE, vibrato: (0.0, 0.015), vibrato_speed: (6.0, 12.0), lowpass: None, volume: (0.24, 0.32), longest: 0.7 };
const BLIP: Ranges = Ranges { waves: &[Wave::Square, Wave::Triangle, Wave::Sine], freq: (400.0, 1400.0), sweep: NONE, duty: (0.3, 0.5), hold: (0.01, 0.05), decay: (0.03, 0.09), punch: NONE, jump: &[0.0], jump_at: NONE, vibrato: NONE, vibrato_speed: (6.0, 6.0), lowpass: None, volume: (0.2, 0.28), longest: 0.2 };

/// A small dice of its own (splitmix64): the IDE is not the simulation,
/// and a roll from a seed comes out the same in the tests.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// From 0 to 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn between(&mut self, (lo, hi): (f64, f64)) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Between two pitches, evenly in octaves.
    fn pitch(&mut self, (lo, hi): (f64, f64)) -> f64 {
        lo * (hi / lo).powf(self.unit())
    }

    fn pick<T: Copy>(&mut self, among: &[T]) -> T {
        among[(self.next() % among.len() as u64) as usize]
    }
}

/// One sound of a sort, drawn from its ranges, every number as the file
/// will write it.
fn draw(r: &Ranges, dice: &mut Dice) -> Synth {
    let wave = dice.pick(r.waves);
    let jump = dice.pick(r.jump);
    let vibrato = tidy("vibrato", dice.between(r.vibrato));
    let lowpass = r.lowpass.map(|(start, end)| (tidy("lowpass", dice.pitch(start)), tidy("lowpass_end", dice.pitch(end))));
    Synth {
        wave,
        freq: tidy("freq", dice.pitch(r.freq)) as f32,
        sweep: tidy("sweep", dice.between(r.sweep)),
        duty: if wave == Wave::Square { tidy("duty", dice.between(r.duty)) as f32 } else { 0.5 },
        attack: 0.0,
        hold: tidy("hold", dice.between(r.hold)) as f32,
        decay: tidy("decay", dice.between(r.decay)) as f32,
        punch: tidy("punch", dice.between(r.punch)) as f32,
        vibrato: vibrato as f32,
        vibrato_speed: if vibrato > 0.0 { tidy("vibrato_speed", dice.between(r.vibrato_speed)) as f32 } else { 6.0 },
        jump,
        jump_at: if jump != 0.0 { tidy("jump_at", dice.between(r.jump_at)) as f32 } else { 0.0 },
        lowpass: lowpass.map(|(l, _)| l as f32),
        lowpass_end: lowpass.map(|(_, e)| e as f32),
        volume: tidy("volume", dice.between(r.volume)) as f32,
    }
}

/// The sort's safe middle: every range's middle, its first wave. What a
/// roll gives when nothing it drew passed.
fn middle(r: &Ranges) -> Synth {
    let mid = |(lo, hi): (f64, f64)| (lo + hi) / 2.0;
    let pitch = |(lo, hi): (f64, f64)| (lo * hi).sqrt();
    let jump = r.jump[r.jump.len() / 2];
    Synth {
        wave: r.waves[0],
        freq: tidy("freq", pitch(r.freq)) as f32,
        sweep: tidy("sweep", mid(r.sweep)),
        duty: if r.waves[0] == Wave::Square { tidy("duty", mid(r.duty)) as f32 } else { 0.5 },
        attack: 0.0,
        hold: tidy("hold", mid(r.hold)) as f32,
        decay: tidy("decay", mid(r.decay)) as f32,
        punch: tidy("punch", mid(r.punch)) as f32,
        vibrato: tidy("vibrato", mid(r.vibrato)) as f32,
        vibrato_speed: tidy("vibrato_speed", mid(r.vibrato_speed)) as f32,
        jump,
        jump_at: if jump != 0.0 { tidy("jump_at", mid(r.jump_at)) as f32 } else { 0.0 },
        lowpass: r.lowpass.map(|(start, _)| tidy("lowpass", pitch(start)) as f32),
        lowpass_end: r.lowpass.map(|(_, end)| tidy("lowpass_end", pitch(end)) as f32),
        volume: tidy("volume", mid(r.volume)) as f32,
    }
}

/// How many sounds a roll draws before it gives the sort's safe middle.
pub const TRIES: usize = 40;

/// A fresh sound of `sort` from `seed`: drawn until one passes `good`
/// (`TRIES` at most), else the sort's safe middle.
pub fn roll(sort: Sort, seed: u64) -> Synth {
    let r = sort.ranges();
    let mut dice = Dice(seed);
    for _ in 0..TRIES {
        let s = draw(r, &mut dice);
        if good(&s, r.longest).is_ok() {
            return s;
        }
    }
    middle(r)
}

/// The quietest a rolled sound may peak, and its loudness (RMS) over its
/// length, full scale 1.
pub const PEAK_FLOOR: f32 = 0.05;
pub const RMS_FLOOR: f32 = 0.01;
/// The share of its energy that must lie where ears and speakers hear
/// well, between `HEARD_FROM` and `HEARD_TO` Hz.
pub const HEARD_SHARE: f64 = 0.5;
pub const HEARD_FROM: f64 = 80.0;
pub const HEARD_TO: f64 = 8000.0;
/// Below this pitch a sound is rumble at best; it may lie there for at
/// most `LOW_SHARE` of its length.
pub const LOWEST: f64 = 30.0;
pub const LOW_SHARE: f64 = 1.0 / 3.0;
/// Its middle may lie off zero by at most this share of its loudness (DC,
/// which thumps and takes headroom).
pub const DC_SHARE: f32 = 0.1;
/// Its last `TAIL` seconds peak at most `END_SHARE` of its peak: it fades
/// out instead of stopping with a click.
pub const TAIL: f32 = 0.003;
pub const END_SHARE: f32 = 0.05;

/// Whether a sound is good to give back from a roll, measured on its
/// rendered samples: not longer than `longest` seconds, loud enough, its
/// energy mostly where it is heard, not sliding to below 30 Hz for most of
/// it, round zero, quiet at its end. The why when not.
pub fn good(s: &Synth, longest: f32) -> Result<(), String> {
    let length = s.length();
    if length > longest {
        return Err(format!("{length:.2} s, longer than {longest} s"));
    }
    let samples = s.render();
    if samples.is_empty() {
        return Err("no samples".into());
    }
    let peak = samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt();
    if peak < PEAK_FLOOR || rms < RMS_FLOOR {
        return Err(format!("too quiet: peak {peak:.3}, rms {rms:.3}"));
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    if mean.abs() > DC_SHARE * rms {
        return Err(format!("off zero by {mean:.4}"));
    }
    let tail = ((TAIL * SAMPLE_RATE as f32) as usize).clamp(1, samples.len());
    let end = samples[samples.len() - tail..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if end > END_SHARE * peak {
        return Err(format!("ends loud: {end:.3} of {peak:.3}"));
    }
    let heard = heard_share(&samples);
    if heard < HEARD_SHARE {
        return Err(format!("only {:.0} % of it heard well", heard * 100.0));
    }
    let low = low_share(s);
    if low > LOW_SHARE {
        return Err(format!("below {LOWEST} Hz for {:.0} % of it", low * 100.0));
    }
    Ok(())
}

/// The share of the time the pitch lies below `LOWEST`, as the synth
/// moves it (the slide, the jump).
fn low_share(s: &Synth) -> f64 {
    const POINTS: usize = 64;
    let length = s.length() as f64;
    let end = s.freq_end() as f64;
    let low = (0..POINTS)
        .filter(|i| {
            let t = (*i as f64 + 0.5) / POINTS as f64 * length;
            let mut f = if s.sweep != 0.0 && end > 0.0 && s.freq > 0.0 { s.freq as f64 * (end / s.freq as f64).powf(t / length) } else { s.freq as f64 };
            if s.jump != 0.0 && t >= s.jump_at as f64 {
                f *= 2f64.powf(s.jump / 12.0);
            }
            f < LOWEST
        })
        .count();
    low as f64 / POINTS as f64
}

/// The share of the samples' energy between `HEARD_FROM` and `HEARD_TO`
/// Hz: their spectrum in frames of 2048 (Hann windows, half over each
/// other), summed.
fn heard_share(samples: &[f32]) -> f64 {
    const N: usize = 2048;
    let window: Vec<f64> = (0..N).map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / N as f64).cos()).collect();
    let bin = SAMPLE_RATE as f64 / N as f64;
    let (mut heard, mut all) = (0.0, 0.0);
    let mut start = 0;
    loop {
        let mut re: Vec<f64> = (0..N).map(|i| samples.get(start + i).map_or(0.0, |x| *x as f64) * window[i]).collect();
        let mut im = vec![0.0; N];
        fft(&mut re, &mut im);
        for k in 0..=N / 2 {
            let power = re[k] * re[k] + im[k] * im[k];
            all += power;
            if (HEARD_FROM..=HEARD_TO).contains(&(k as f64 * bin)) {
                heard += power;
            }
        }
        start += N / 2;
        if start >= samples.len() {
            break;
        }
    }
    if all > 0.0 { heard / all } else { 0.0 }
}

/// A Fourier transform in place (radix 2; the length a power of two).
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -std::f64::consts::TAU / len as f64;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
}

/// A fader over the sound, each moving words of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fader {
    Pitch,
    Length,
    Slide,
    Bright,
    Punch,
}

impl Fader {
    pub const ALL: [Fader; 5] = [Fader::Pitch, Fader::Length, Fader::Slide, Fader::Bright, Fader::Punch];

    pub fn word(self) -> &'static str {
        match self {
            Fader::Pitch => "PITCH",
            Fader::Length => "LENGTH",
            Fader::Slide => "SLIDE",
            Fader::Bright => "BRIGHT",
            Fader::Punch => "PUNCH",
        }
    }

    /// What it does, in plain words, and which words of the file it moves.
    pub fn hover(self) -> &'static str {
        match self {
            Fader::Pitch => "How high the sound starts; for noise, how bright its hiss is. Moves freq.",
            Fader::Length => "How long the sound lasts: its time at full volume and its fade together. Moves hold and decay.",
            Fader::Slide => "How far the pitch slides over the sound, in semitones: below 0 it falls, above 0 it rises; 12 is an octave. Moves sweep.",
            Fader::Bright => "How bright the sound is: lower muffles it; all the way right, nothing is muffled. Moves lowpass.",
            Fader::Punch => "How hard the start hits: extra loudness at the start that falls away. Moves punch.",
        }
    }

    /// Its two ends.
    pub fn range(self) -> (f64, f64) {
        match self {
            Fader::Pitch => (30.0, 12_000.0),
            Fader::Length => (0.02, 3.0),
            Fader::Slide => (-48.0, 48.0),
            Fader::Bright => (100.0, BRIGHT_OFF),
            Fader::Punch => (0.0, 1.0),
        }
    }

    /// Whether it moves evenly in octaves (or times) rather than in steps.
    pub fn logarithmic(self) -> bool {
        matches!(self, Fader::Pitch | Fader::Length | Fader::Bright)
    }

    /// Its number shown beside it.
    pub fn shown(self, v: f64) -> String {
        match self {
            Fader::Pitch => format!("{v:.0} Hz"),
            Fader::Length => format!("{v:.2} s"),
            Fader::Slide if v.round() == 0.0 => "0".into(),
            Fader::Slide => format!("{v:+.0}"),
            Fader::Bright if v >= BRIGHT_OFF => "ALL".into(),
            Fader::Bright => format!("{v:.0} Hz"),
            Fader::Punch => format!("{v:.2}"),
        }
    }

    /// Where it stands for the sound `s`, as its text says.
    pub fn place(self, s: &Synth) -> f64 {
        let v = match self {
            Fader::Pitch => s.freq as f64,
            Fader::Length => (s.hold.max(0.0) + s.decay.max(0.0)) as f64,
            Fader::Slide => s.sweep,
            Fader::Bright => s.lowpass.map_or(BRIGHT_OFF, |l| l as f64),
            Fader::Punch => s.punch as f64,
        };
        let (lo, hi) = self.range();
        v.clamp(lo, hi)
    }

    /// The words to write for the fader moved to `to` over the sound `s`:
    /// a value each, or None to take the word out. Only its own words, and
    /// another only where the file would no longer read without it (the
    /// slide where a new pitch would slide it out of hearing).
    pub fn moved(self, s: &Synth, to: f64) -> Vec<(&'static str, Option<f64>)> {
        let (lo, hi) = self.range();
        let to = to.clamp(lo, hi);
        // How far the pitch may slide from `freq` and stay heard.
        let slide = |freq: f64, sweep: f64| {
            let (down, up) = ((12.0 * (words::PITCHES.0 / freq).log2()).ceil(), (12.0 * (words::PITCHES.1 / freq).log2()).floor());
            tidy("sweep", sweep).clamp(down, up)
        };
        match self {
            Fader::Pitch => {
                let freq = tidy("freq", to);
                let mut out = vec![("freq", Some(freq))];
                if s.sweep != 0.0 && slide(freq, s.sweep) != s.sweep {
                    out.push(("sweep", Some(slide(freq, s.sweep))));
                }
                out
            }
            Fader::Slide => vec![("sweep", Some(slide(s.freq as f64, to)))],
            Fader::Length => {
                let (hold, decay) = (s.hold.max(0.0) as f64, s.decay.max(0.0) as f64);
                let (hold, decay, k) = if hold + decay > 0.0 { (hold * to / (hold + decay), decay * to / (hold + decay), to / (hold + decay)) } else { (0.0, to, 1.0) };
                let (hold, decay) = (tidy("hold", hold), tidy("decay", decay).max(0.001));
                let mut out = vec![("hold", Some(hold)), ("decay", Some(decay))];
                if s.jump != 0.0 {
                    // The jump keeps its place in the sound.
                    let length = s.attack.max(0.0) as f64 + hold + decay.max(crate::audio::synth::SHORTEST_DECAY as f64);
                    out.push(("jump_at", Some(tidy("jump_at", (s.jump_at as f64 * k).min(length * 0.9)))));
                }
                out
            }
            Fader::Bright if to >= BRIGHT_OFF * 0.999 => vec![("lowpass", None), ("lowpass_end", None)],
            Fader::Bright => {
                let cut = tidy("lowpass", to);
                let mut out = vec![("lowpass", Some(cut))];
                // The filter's slide keeps its ratio.
                if let (Some(start), Some(end)) = (s.lowpass, s.lowpass_end) {
                    out.push(("lowpass_end", Some(tidy("lowpass_end", end as f64 / start as f64 * cut))));
                }
                out
            }
            Fader::Punch => vec![("punch", Some(tidy("punch", to)))],
        }
    }
}

/// BRIGHT at its top: no filter at all.
pub const BRIGHT_OFF: f64 = 20_000.0;

/// A word's number as the file writes it: rounded as fine as the ear
/// tells (whole Hz and semitones, milliseconds, hundredths of a share)
/// and inside the word's checked range.
pub fn tidy(name: &str, v: f64) -> f64 {
    let places = match name {
        "freq" | "lowpass" | "lowpass_end" | "vibrato_speed" | "sweep" | "jump" => 0,
        "duty" | "punch" | "volume" => 2,
        _ => 3,
    };
    let k = 10f64.powi(places);
    let v = (v * k).round() / k;
    match words::word(name).and_then(|w| w.sound.as_ref()).and_then(|u| u.range) {
        Some((lo, hi)) => v.clamp(lo, hi),
        None => v,
    }
}

/// A word's number as a TOML value: whole numbers without a point.
pub fn number(name: &str, v: f64) -> Value {
    let v = tidy(name, v);
    if v.fract() == 0.0 && v.abs() < 1e15 { Value::from(v as i64) } else { Value::from(v) }
}

/// The words of a fresh synthesized sound, in the register's order: the
/// wave always, the rest where they differ from the defaults (a duty only
/// for a square wave, a jump's time only with a jump, the filter's end
/// only with the filter).
pub fn words_of(s: &Synth) -> Vec<(&'static str, Option<Value>)> {
    let d = Synth::default();
    let f = |name: &'static str, v: f64, default: f64| (name, (tidy(name, v) != tidy(name, default)).then(|| number(name, v)));
    vec![
        ("wave", Some(Value::from(s.wave.name()))),
        f("freq", s.freq as f64, d.freq as f64),
        f("sweep", s.sweep, d.sweep),
        if s.wave == Wave::Square { f("duty", s.duty as f64, d.duty as f64) } else { ("duty", None) },
        f("attack", s.attack as f64, d.attack as f64),
        f("hold", s.hold as f64, d.hold as f64),
        f("decay", s.decay as f64, d.decay as f64),
        f("punch", s.punch as f64, d.punch as f64),
        f("vibrato", s.vibrato as f64, d.vibrato as f64),
        if s.vibrato != 0.0 { f("vibrato_speed", s.vibrato_speed as f64, d.vibrato_speed as f64) } else { ("vibrato_speed", None) },
        f("jump", s.jump, d.jump),
        if s.jump != 0.0 { f("jump_at", s.jump_at as f64, d.jump_at as f64) } else { ("jump_at", None) },
        ("lowpass", s.lowpass.map(|v| number("lowpass", v as f64))),
        ("lowpass_end", s.lowpass.and(s.lowpass_end).map(|v| number("lowpass_end", v as f64))),
        f("volume", s.volume as f64, d.volume as f64),
    ]
}

/// The sound file's `text` with the words `set` (a value: written, in
/// place where the word stands, after its kin in the register's order
/// where it is new; None: taken out). `whole`: a fresh sound, every word
/// not set taken out too. The comments above the first word stay at the
/// top, a comment after a value on its line stays with it. A text TOML
/// cannot read is written anew under its leading comments when `whole`,
/// else left as it is.
pub fn rewrite(text: &str, set: &[(&str, Option<Value>)], whole: bool) -> String {
    let Ok(mut doc) = text.parse::<DocumentMut>() else {
        if !whole {
            return text.to_string();
        }
        let mut out: String = text.lines().take_while(|l| l.trim().is_empty() || l.trim_start().starts_with('#')).map(|l| format!("{l}\n")).collect();
        for (name, value) in set {
            if let Some(v) = value {
                out.push_str(&format!("{name} = {v}\n"));
            }
        }
        return out;
    };
    // The comments at the top, held apart while words come and go.
    let first = doc.iter().next().map(|(k, _)| k.to_string());
    let leading = match &first {
        Some(k) => {
            let mut key = doc.key_mut(k).expect("the first key is there");
            let decor = key.leaf_decor_mut();
            let text = decor.prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
            decor.set_prefix("");
            text
        }
        None => {
            let text = doc.trailing().as_str().unwrap_or("").to_string();
            doc.set_trailing("");
            text
        }
    };
    if whole {
        let keep: Vec<&str> = set.iter().filter(|(_, v)| v.is_some()).map(|(n, _)| *n).collect();
        doc.retain(|k, _| keep.contains(&k));
    }
    let before: Vec<String> = doc.iter().map(|(k, _)| k.to_string()).collect();
    let order = |name: &str| words::WORDS.iter().position(|w| w.name == name);
    let mut added = false;
    for (name, value) in set {
        match value {
            Some(v) => match doc.get_mut(name).and_then(Item::as_value_mut) {
                Some(old) => {
                    let decor = old.decor().clone();
                    *old = v.clone();
                    *old.decor_mut() = decor;
                }
                None => {
                    doc.insert(name, Item::Value(v.clone()));
                    added = true;
                }
            },
            None => {
                doc.remove(name);
            }
        }
    }
    if added {
        // A new word after the last word before it in the register's
        // order (at the top if none), the others where they stood.
        let rank = |name: &str| -> f64 {
            if let Some(at) = before.iter().position(|k| k == name) {
                return at as f64;
            }
            let own = order(name).unwrap_or(usize::MAX) as f64 / 1000.0;
            let kin = before.iter().enumerate().filter(|(_, k)| order(k).is_some_and(|o| Some(o) < order(name))).map(|(i, _)| i).max();
            kin.map_or(-1.0, |i| i as f64 + 0.5) + own / 1000.0
        };
        doc.sort_values_by(|a, _, b, _| rank(a.get()).total_cmp(&rank(b.get())));
    }
    let first = doc.iter().next().map(|(k, _)| k.to_string());
    match first {
        Some(k) => {
            let mut key = doc.key_mut(&k).expect("the first key is there");
            let decor = key.leaf_decor_mut();
            let own = decor.prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
            decor.set_prefix(format!("{leading}{own}"));
        }
        None => {
            let rest = doc.trailing().as_str().unwrap_or("").to_string();
            doc.set_trailing(format!("{leading}{rest}"));
        }
    }
    doc.to_string()
}

/// The text of a fresh sound over the old text (its leading comments kept).
pub fn fresh(text: &str, s: &Synth) -> String {
    rewrite(text, &words_of(s), true)
}

/// The text with the fader moved to `to` over the sound `s` the text says.
pub fn faded(text: &str, s: &Synth, fader: Fader, to: f64) -> String {
    let set: Vec<(&str, Option<Value>)> = fader.moved(s, to).into_iter().map(|(n, v)| (n, v.map(|v| number(n, v)))).collect();
    rewrite(text, &set, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sound::{self, SoundDef};

    fn synth(text: &str) -> Synth {
        match sound::read("sounds/x.toml", text) {
            Ok(SoundDef::Synth(s)) => s,
            other => panic!("{other:?}\n{text}"),
        }
    }

    /// Every sort rolled 200 times from fixed seeds gives sounds that pass
    /// the checks, read back from their file as rolled, and differ; the
    /// sorts' safe middles pass too, and few rolls fall back to them.
    #[test]
    fn the_dice_gives_back_only_good_sounds() {
        for sort in Sort::ALL {
            let r = sort.ranges();
            good(&middle(r), r.longest).unwrap_or_else(|e| panic!("{sort:?}'s middle: {e}"));
            let mut texts = std::collections::BTreeSet::new();
            let mut middles = 0;
            for seed in 0..200u64 {
                let s = roll(sort, seed * 7919 + 1);
                good(&s, r.longest).unwrap_or_else(|e| panic!("{sort:?} seed {seed}: {e}: {s:?}"));
                middles += (s == middle(r)) as usize;
                let text = fresh("# made by the dice\n", &s);
                assert!(text.starts_with("# made by the dice\nwave = "), "{text}");
                assert_eq!(synth(&text), s, "{sort:?} seed {seed} reads back as rolled:\n{text}");
                texts.insert(text);
            }
            assert!(texts.len() >= 190, "{sort:?}: only {} different sounds in 200", texts.len());
            assert!(middles <= 10, "{sort:?}: {middles} rolls of 200 found nothing good");
            assert_eq!(roll(sort, 42), roll(sort, 42), "a seed rolls the same");
        }
    }

    /// The checks turn away what they are for: a sound too quiet, too
    /// long, sliding to below hearing, off zero, ending loud, or mostly
    /// too high to hear well.
    #[test]
    fn the_checks_turn_bad_sounds_away() {
        let base = Synth { wave: Wave::Triangle, freq: 600.0, hold: 0.05, decay: 0.15, volume: 0.3, ..Synth::default() };
        assert!(good(&base, 1.0).is_ok(), "{:?}", good(&base, 1.0));
        assert!(good(&Synth { volume: 0.01, ..base.clone() }, 1.0).unwrap_err().starts_with("too quiet"));
        assert!(good(&base, 0.1).unwrap_err().contains("longer than"));
        assert!(good(&Synth { sweep: -100.0, ..base.clone() }, 1.0).unwrap_err().contains("below 30 Hz"), "sliding to 2 Hz");
        assert!(good(&Synth { wave: Wave::Sine, freq: 12_000.0, ..base.clone() }, 1.0).unwrap_err().contains("heard well"));
        assert!(good(&Synth { wave: Wave::Square, freq: 25.0, hold: 0.3, decay: 0.01, ..base.clone() }, 1.0).is_err(), "a slow square: off zero, low");
    }

    /// A roll keeps the file's leading comments and a value's own comment,
    /// takes out the words it no longer says and writes new ones in the
    /// register's order; a fader changes its words in place.
    #[test]
    fn the_file_is_rewritten_with_its_comments() {
        let text = "# coin: picked up\n# second line\n\nwave = \"square\" # the wave\nfreq = 880\nfile_left = 1\nvibrato = 0.1\n";
        let s = Synth { wave: Wave::Triangle, freq: 1000.0, hold: 0.05, jump: 5.0, jump_at: 0.04, ..Synth::default() };
        let out = fresh(text, &s);
        assert_eq!(out, "# coin: picked up\n# second line\n\nwave = \"triangle\" # the wave\nfreq = 1000\nhold = 0.05\njump = 5\njump_at = 0.04\n");
        assert_eq!(synth(&out), s);
        // A fader: one word in place, a new one after its kin.
        let coin = "# a coin\nwave = \"triangle\"\nfreq = 880 # high\nhold = 0.04\ndecay = 0.16\nvolume = 0.3\n";
        let s = synth(coin);
        assert_eq!(faded(coin, &s, Fader::Pitch, 440.0), "# a coin\nwave = \"triangle\"\nfreq = 440 # high\nhold = 0.04\ndecay = 0.16\nvolume = 0.3\n");
        assert_eq!(faded(coin, &s, Fader::Slide, -12.4), "# a coin\nwave = \"triangle\"\nfreq = 880 # high\nsweep = -12\nhold = 0.04\ndecay = 0.16\nvolume = 0.3\n");
        assert_eq!(faded(coin, &s, Fader::Punch, 0.5), "# a coin\nwave = \"triangle\"\nfreq = 880 # high\nhold = 0.04\ndecay = 0.16\npunch = 0.5\nvolume = 0.3\n");
        assert_eq!(faded(coin, &s, Fader::Length, 0.4), "# a coin\nwave = \"triangle\"\nfreq = 880 # high\nhold = 0.08\ndecay = 0.32\nvolume = 0.3\n");
        let muffled = faded(coin, &s, Fader::Bright, 2000.0);
        assert!(muffled.ends_with("decay = 0.16\nlowpass = 2000\nvolume = 0.3\n"), "{muffled}");
        assert_eq!(faded(&muffled, &synth(&muffled), Fader::Bright, BRIGHT_OFF), coin, "all the way right: the filter gone");
        // A word new at the top keeps the comment at the top.
        assert_eq!(faded("# x\nhold = 0.1\n", &synth("hold = 0.1"), Fader::Pitch, 500.0), "# x\nfreq = 500\nhold = 0.1\n");
        // Only comments: the words below them.
        assert_eq!(fresh("# only this\n", &Synth::default()), "# only this\nwave = \"square\"\n");
        // A text TOML cannot read: written anew under its comments by a roll, left by a fader.
        assert_eq!(fresh("# mine\nwave = = 2\n", &Synth::default()), "# mine\nwave = \"square\"\n");
        assert_eq!(faded("wave = = 2\n", &Synth::default(), Fader::Pitch, 300.0), "wave = = 2\n");
    }

    /// The faders read their place from the sound and write only numbers
    /// the reader takes, at either end and between, over sounds of every
    /// sort (a pitch moved far keeps the slide inside hearing; a jump keeps
    /// its place inside a shorter sound).
    #[test]
    fn faders_write_only_what_reads() {
        let mut sounds: Vec<Synth> = Sort::ALL.iter().map(|s| roll(*s, 9)).collect();
        sounds.push(synth("wave = \"saw\"\nfreq = 30\nsweep = -6\n"));
        sounds.push(synth("wave = \"triangle\"\nfreq = 12000\nsweep = 6\nhold = 0\ndecay = 0\n"));
        for s in &sounds {
            let text = fresh("# x\n", s);
            for fader in Fader::ALL {
                let (lo, hi) = fader.range();
                assert!((lo..=hi).contains(&fader.place(s)));
                for k in 0..=10 {
                    let to = if fader.logarithmic() { lo * (hi / lo).powf(k as f64 / 10.0) } else { lo + (hi - lo) * k as f64 / 10.0 };
                    let out = faded(&text, s, fader, to);
                    let moved = synth(&out);
                    let near = match fader {
                        Fader::Pitch => tidy("freq", moved.freq as f64) == tidy("freq", to),
                        Fader::Punch => tidy("punch", moved.punch as f64) == tidy("punch", to),
                        Fader::Bright => (fader.place(&moved) - to).abs() <= 0.5,
                        Fader::Length => (fader.place(&moved) - to).abs() <= 0.002 + to * 0.01,
                        // Held inside hearing where the pitch would slide out of it.
                        Fader::Slide => moved.sweep == tidy("sweep", to) || moved.sweep.abs() < tidy("sweep", to).abs(),
                    };
                    assert!(near, "{fader:?} to {to}: {out}");
                }
            }
        }
        // The register's ranges: a number out of one is pulled into it.
        assert_eq!(tidy("volume", 3.0), 1.0);
        assert_eq!(tidy("freq", 5.0), 20.0);
        assert_eq!(tidy("sweep", -500.0), -120.0);
        assert_eq!(number("hold", 0.12345).to_string(), "0.123");
        assert_eq!(number("freq", 880.4).to_string(), "880");
    }
}

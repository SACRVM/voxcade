//! A port of the JUST suite's limiter (JustPlay.Analysis's MasteringLimiter, same author).
//!
//! The last stage of the sound: a stereo true-peak limiter that holds the
//! mix under a ceiling (-1 dBTP) instead of bending it as `tanh` did. Below
//! the ceiling it does nothing at all: the mix comes out as it went in,
//! only later by the look-ahead (1.5 ms). Above it, both channels are
//! turned down together, by just enough, and back up slowly.
//!
//! How, stage by stage (as the original):
//!
//! 1. Drive: a gain before everything (0 dB here: a safety limiter, nothing
//!    pushed up; the original's maximizer for a louder "club" sound).
//! 2. True peak: each channel through BS.1770's 4x polyphase filter (the
//!    49-tap Hann-windowed sinc libebur128 and ffmpeg generate), so a peak
//!    between two samples counts as much as one on them. The louder
//!    channel decides for both (linked: the stereo image never shifts).
//! 3. Look-ahead: the loudest true peak in a window of the next 1.5 ms
//!    (a sliding maximum, a deque of falling values), while the sound
//!    itself waits in a delay line as long: the gain is down before the
//!    peak comes out, so there is no overshoot and no attack to hear.
//! 4. Gain: ceiling / peak when over, else 1, never above 1. Down at once;
//!    back up through two releases in a row, fast (50 ms) then slow
//!    (250 ms), the slow one smoothing away the pumping.
//! 5. The delayed sound times that gain.
//!
//! It runs in the audio callback, so `process_interleaved_stereo` touches
//! only what `new` made: no allocation, no lock, no system call.

/// 4x oversampling, BS.1770's least.
const FACTOR: usize = 4;
/// Odd, so the filter is symmetric: 13 taps a phase (12 that are not 0).
const TAPS: usize = 49;
/// Input samples the filter remembers: (TAPS + FACTOR - 1) / FACTOR.
const FIR_DELAY: usize = TAPS.div_ceil(FACTOR);

/// The original's constructor arguments, with its defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// True-peak ceiling. -1 dBTP is the broadcast-safe default.
    pub ceiling_db_tp: f64,
    /// Makeup gain before limiting, in dB (0: safety only).
    pub drive_db: f64,
    /// The look-ahead window (1 to 5 ms typical); the sound comes out this
    /// much later.
    pub lookahead_ms: f64,
    /// The fast release's time constant.
    pub fast_release_ms: f64,
    /// The slow release's (the smoothing that keeps it from pumping).
    pub slow_release_ms: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self { ceiling_db_tp: -1.0, drive_db: 0.0, lookahead_ms: 1.5, fast_release_ms: 50.0, slow_release_ms: 250.0 }
    }
}

/// What the limiter did since it was last asked (`Limiter::read_telemetry`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Telemetry {
    /// The deepest reduction, in dB (0 or below; 0: it did not reduce).
    pub gain_reduction_db: f64,
    /// The share of frames it was reducing (by more than about 0.004 dB).
    pub duty_cycle: f64,
    /// Whether each channel's true peak went over the ceiling (which one
    /// drove the linked reduction).
    pub left_at_ceiling: bool,
    pub right_at_ceiling: bool,
}

/// The stereo true-peak limiter (see the module's notes).
pub struct Limiter {
    sample_rate: u32,
    options: Options,

    ceiling: f32,
    drive: f32,
    /// The delay and the sliding maximum's window, in samples.
    lookahead: usize,
    fast_release: f64,
    slow_release: f64,

    /// The 4x filter: one subfilter a phase, and each channel's history.
    phase: [[f32; FIR_DELAY]; FACTOR],
    fir_l: [f32; FIR_DELAY],
    fir_r: [f32; FIR_DELAY],
    fir_pos: usize,

    /// The look-ahead delay (driven samples), one longer than the delay so
    /// the read and write heads never meet.
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    delay_pos: usize,

    /// The sliding maximum: candidates' sample numbers and true peaks
    /// (falling from front to back), slots [head, tail) of a ring.
    dq_idx: Vec<i64>,
    dq_val: Vec<f32>,
    dq_head: usize,
    dq_tail: usize,
    /// Input samples so far.
    n: i64,

    /// The two release stages (linked, at most 1).
    gr_fast: f64,
    gr_slow: f64,

    tel_min_gain: f64,
    tel_active_frames: u64,
    tel_total_frames: u64,
    tel_hit_l: bool,
    tel_hit_r: bool,
}

impl Limiter {
    /// The original's defaults at `sample_rate`.
    pub fn new(sample_rate: u32) -> Self {
        Self::with(sample_rate, Options::default())
    }

    /// Panics, as the original throws, on a rate or a time that is not
    /// above 0: those are mistakes in the code, not in a game.
    pub fn with(sample_rate: u32, options: Options) -> Self {
        assert!(sample_rate > 0, "limiter: sample rate {sample_rate}");
        assert!(options.lookahead_ms > 0.0, "limiter: look-ahead {} ms", options.lookahead_ms);
        assert!(options.fast_release_ms > 0.0, "limiter: fast release {} ms", options.fast_release_ms);
        assert!(options.slow_release_ms > 0.0, "limiter: slow release {} ms", options.slow_release_ms);
        let rate = f64::from(sample_rate);
        let lookahead = ((options.lookahead_ms * rate / 1000.0).round() as usize).max(1);
        Self {
            sample_rate,
            options,
            ceiling: 10f64.powf(options.ceiling_db_tp / 20.0) as f32,
            drive: if options.drive_db == 0.0 { 1.0 } else { 10f64.powf(options.drive_db / 20.0) as f32 },
            lookahead,
            fast_release: (-1.0 / (options.fast_release_ms * rate / 1000.0)).exp(),
            slow_release: (-1.0 / (options.slow_release_ms * rate / 1000.0)).exp(),
            phase: true_peak_phases(),
            fir_l: [0.0; FIR_DELAY],
            fir_r: [0.0; FIR_DELAY],
            fir_pos: 0,
            delay_l: vec![0.0; lookahead + 1],
            delay_r: vec![0.0; lookahead + 1],
            delay_pos: 0,
            // At most window + 1 candidates (the original's sizing; see
            // `push_max`).
            dq_idx: vec![0; lookahead + 2],
            dq_val: vec![0.0; lookahead + 2],
            dq_head: 0,
            dq_tail: 0,
            n: 0,
            gr_fast: 1.0,
            gr_slow: 1.0,
            tel_min_gain: 1.0,
            tel_active_frames: 0,
            tel_total_frames: 0,
            tel_hit_l: false,
            tel_hit_r: false,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn options(&self) -> Options {
        self.options
    }

    /// A new drive while it plays (a game's `c.audio.drive` saved). The
    /// original made a new limiter for a new drive; here the delay, the
    /// gain and the filter's history go on, so it does not click: only the
    /// 1.5 ms already in the delay come out at the drive before.
    pub fn set_drive_db(&mut self, drive_db: f64) {
        if drive_db != self.options.drive_db {
            self.options.drive_db = drive_db;
            self.drive = if drive_db == 0.0 { 1.0 } else { 10f64.powf(drive_db / 20.0) as f32 };
        }
    }

    /// The ceiling as a linear amplitude (-1 dBTP: about 0.8913).
    pub fn ceiling(&self) -> f32 {
        self.ceiling
    }

    /// Clears everything (delay, filter history, gain, telemetry), as when
    /// it was made.
    pub fn reset(&mut self) {
        self.fir_l = [0.0; FIR_DELAY];
        self.fir_r = [0.0; FIR_DELAY];
        self.fir_pos = 0;
        self.delay_l.fill(0.0);
        self.delay_r.fill(0.0);
        self.delay_pos = 0;
        (self.dq_head, self.dq_tail) = (0, 0);
        self.n = 0;
        (self.gr_fast, self.gr_slow) = (1.0, 1.0);
        (self.tel_min_gain, self.tel_active_frames, self.tel_total_frames, self.tel_hit_l, self.tel_hit_r) = (1.0, 0, 0, false, false);
    }

    /// How much later the sound comes out, in samples.
    pub fn latency_samples(&self) -> usize {
        self.lookahead
    }

    /// The reduction now, in dB, 0 or above (0: not limiting).
    pub fn last_gain_reduction_db(&self) -> f64 {
        if self.gr_slow < 1.0 { (-20.0 * self.gr_slow.log10()).max(0.0) } else { 0.0 }
    }

    /// The gain applied to the last frame (1: none).
    pub fn gain(&self) -> f32 {
        self.gr_slow as f32
    }

    /// What it did since the last call, and starts counting anew.
    pub fn read_telemetry(&mut self) -> Telemetry {
        let t = Telemetry {
            gain_reduction_db: if self.tel_min_gain >= 1.0 { 0.0 } else { 20.0 * self.tel_min_gain.log10() },
            duty_cycle: if self.tel_total_frames > 0 { self.tel_active_frames as f64 / self.tel_total_frames as f64 } else { 0.0 },
            left_at_ceiling: self.tel_hit_l,
            right_at_ceiling: self.tel_hit_r,
        };
        (self.tel_min_gain, self.tel_active_frames, self.tel_total_frames, self.tel_hit_l, self.tel_hit_r) = (1.0, 0, 0, false, false);
        t
    }

    /// Limits interleaved stereo (L, R, L, R ...) in place, going on from
    /// the last call. An odd last sample is left as it is.
    pub fn process_interleaved_stereo(&mut self, buf: &mut [f32]) {
        for frame in buf.as_chunks_mut::<2>().0 {
            // 1. The drive first: everything after sees the driven sound.
            let l = frame[0] * self.drive;
            let r = frame[1] * self.drive;

            // 2. The frame's true peak, the louder channel's (linked).
            let tp_l = fir_true_peak(&self.phase, &mut self.fir_l, self.fir_pos, l);
            let tp_r = fir_true_peak(&self.phase, &mut self.fir_r, self.fir_pos, r);
            let tp = tp_l.max(tp_r);
            self.fir_pos = (self.fir_pos + 1) % FIR_DELAY;
            if tp_l > self.ceiling {
                self.tel_hit_l = true;
            }
            if tp_r > self.ceiling {
                self.tel_hit_r = true;
            }

            // 3. The loudest true peak in the window ahead of the sample
            // about to leave the delay.
            self.push_max(self.n, tp);
            self.expire_older_than(self.n - self.lookahead as i64);
            let win_max = self.dq_val[self.dq_head];

            // 4. The gain that lands that peak on the ceiling, never above 1
            // (the division in f32 as the original's).
            let target = if win_max > self.ceiling { f64::from(self.ceiling / win_max) } else { 1.0 };
            if target < self.gr_fast {
                self.gr_fast = target;
            } else {
                self.gr_fast = self.fast_release * self.gr_fast + (1.0 - self.fast_release) * target;
            }
            if self.gr_fast < self.gr_slow {
                self.gr_slow = self.gr_fast;
            } else {
                self.gr_slow = self.slow_release * self.gr_slow + (1.0 - self.slow_release) * self.gr_fast;
            }
            if self.gr_slow > 1.0 {
                self.gr_slow = 1.0;
            }
            let gain = self.gr_slow as f32;

            self.tel_total_frames += 1;
            if self.gr_slow < 0.9995 {
                self.tel_active_frames += 1;
            }
            if self.gr_slow < self.tel_min_gain {
                self.tel_min_gain = self.gr_slow;
            }

            // 5. Out goes the delayed sample at that gain; in goes this one.
            let len = self.delay_l.len();
            let read = (self.delay_pos + len - self.lookahead) % len;
            frame[0] = gain * self.delay_l[read];
            frame[1] = gain * self.delay_r[read];
            self.delay_l[self.delay_pos] = l;
            self.delay_r[self.delay_pos] = r;
            self.delay_pos = (self.delay_pos + 1) % len;

            self.n += 1;
        }
    }

    /// The sliding maximum: drops candidates this one outdoes (they can
    /// never be the window's loudest again) and adds it at the back.
    ///
    /// Ported as the original has it, a ring of window + 2 slots. Before
    /// the expiry it can have to hold window + 2 candidates (true peaks
    /// falling that long): the ring is then full, `head == tail` reads as
    /// empty, and the next push forgets every candidate. A bug in the
    /// original, kept here as it is until its author decides (the test
    /// `the_sliding_maximum_is_the_windows_loudest` shows it).
    fn push_max(&mut self, idx: i64, val: f32) {
        let len = self.dq_val.len();
        while self.dq_head != self.dq_tail {
            let back = (self.dq_tail + len - 1) % len;
            if self.dq_val[back] <= val {
                self.dq_tail = back;
            } else {
                break;
            }
        }
        self.dq_idx[self.dq_tail] = idx;
        self.dq_val[self.dq_tail] = val;
        self.dq_tail = (self.dq_tail + 1) % len;
    }

    fn expire_older_than(&mut self, oldest_kept: i64) {
        while self.dq_head != self.dq_tail && self.dq_idx[self.dq_head] < oldest_kept {
            self.dq_head = (self.dq_head + 1) % self.dq_val.len();
        }
    }
}

/// BS.1770's 4x interpolation filter as libebur128 generates it (it ships
/// no table): a sinc low pass at the factor, Hann-windowed, dealt out into
/// one subfilter a phase. Phase 0 is the sample itself, six samples late.
fn true_peak_phases() -> [[f32; FIR_DELAY]; FACTOR] {
    let mut phase = [[0.0f32; FIR_DELAY]; FACTOR];
    for j in 0..TAPS {
        let m = j as f64 - (TAPS - 1) as f64 / 2.0;
        let mut c = 1.0;
        if m != 0.0 {
            let x = m * std::f64::consts::PI / FACTOR as f64;
            c = x.sin() / x;
        }
        c *= 0.5 * (1.0 - (2.0 * std::f64::consts::PI * j as f64 / (TAPS - 1) as f64).cos());
        phase[j % FACTOR][j / FACTOR] = c as f32;
    }
    phase
}

/// Pushes one sample into a channel's history and returns the largest of
/// its four interpolated values (products in f32, summed in f64, as the
/// original).
fn fir_true_peak(phase: &[[f32; FIR_DELAY]; FACTOR], hist: &mut [f32; FIR_DELAY], pos: usize, sample: f32) -> f32 {
    hist[pos] = sample;
    let mut peak = 0.0f32;
    for coeff in phase {
        let mut acc = 0.0f64;
        for (t, &c) in coeff.iter().enumerate() {
            let h = (pos + FIR_DELAY - t) % FIR_DELAY;
            acc += f64::from(c * hist[h]);
        }
        let a = if acc < 0.0 { -acc as f32 } else { acc as f32 };
        if a > peak {
            peak = a;
        }
    }
    peak
}

/// The true peak of mono samples by the same 4x filter (linear; 0 for
/// none), as libebur128 and ffmpeg read it: from silence before, and not
/// on past the end (the last six samples' peaks between them unread).
pub fn true_peak(samples: &[f32]) -> f32 {
    true_peaks(samples, 0).into_iter().fold(0.0, f32::max)
}

/// Each step's true peak by the same 4x filter: the largest of the four
/// values it interpolates, which lie six samples back (its delay).
/// `after` steps of silence follow, to read the last samples too.
pub fn true_peaks(samples: &[f32], after: usize) -> Vec<f32> {
    let phase = true_peak_phases();
    let mut hist = [0.0f32; FIR_DELAY];
    let tail = std::iter::repeat_n(0.0, after);
    samples.iter().copied().chain(tail).enumerate().map(|(i, s)| fir_true_peak(&phase, &mut hist, i % FIR_DELAY, s)).collect()
}

/// The filter's delay in samples: a step's true peak is that far back.
pub const TRUE_PEAK_DELAY: usize = (FIR_DELAY - 1) / 2;

/// The true peak of interleaved stereo, the louder channel's.
pub fn true_peak_stereo(samples: &[f32]) -> f32 {
    let (l, r): (Vec<f32>, Vec<f32>) = samples.as_chunks::<2>().0.iter().map(|f| (f[0], f[1])).unzip();
    true_peak(&l).max(true_peak(&r))
}

/// A linear amplitude in dBTP (minus infinity for 0).
pub fn db_tp(linear: f32) -> f64 {
    if linear <= 0.0 { f64::NEG_INFINITY } else { 20.0 * f64::from(linear).log10() }
}

/// A whole recording limited offline, as the speaker would play it but
/// on time: the look-ahead's delay taken off the front, the tail flushed
/// with silence, so the output lines up with the input sample for sample.
/// `channels` 1 (each sample in both channels, the left one kept) or 2.
pub fn limit_whole(limiter: &mut Limiter, samples: &[f32], channels: usize) -> Vec<f32> {
    assert!(channels == 1 || channels == 2, "limit_whole: {channels} channels");
    let delay = limiter.latency_samples() * 2;
    let mut stereo: Vec<f32> = if channels == 2 { samples.to_vec() } else { samples.iter().flat_map(|&s| [s, s]).collect() };
    stereo.extend(std::iter::repeat_n(0.0, delay));
    limiter.process_interleaved_stereo(&mut stereo);
    let on_time = &stereo[delay..];
    if channels == 2 { on_time.to_vec() } else { on_time.iter().step_by(2).copied().collect() }
}

#[cfg(test)]
mod tests;

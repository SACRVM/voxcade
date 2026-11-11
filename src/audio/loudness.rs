//! A port of the JUST suite's loudness measurement (JustPlay.Analysis's Bs1770Loudness, same author).
//!
//! ITU-R BS.1770 / EBU R128 integrated loudness in LUFS: the sound
//! K-weighted (a high shelf of +4 dB above about 1.5 kHz, as the head
//! hears, and a high pass at 38 Hz), its mean square taken over blocks of
//! 400 ms every 100 ms, blocks under -70 LUFS left out, then those 10 LU
//! under the mean of the rest; what remains averaged. With it the sample
//! peak (`detect`), as ReplayGain wants. For checking what the limiter does
//! to real songs, and for tools later; nothing of it runs while a game
//! plays.
//!
//! The original measured mono at 11 025 Hz only, its filter and offset
//! worked out for that rate (a 997 Hz full-scale sine reads -3.01 LUFS).
//! At 11 025 Hz this port is that, number for number; at any other rate the
//! filter comes from the same formulas libebur128 uses (at 48 kHz they give
//! the standard's table) and the standard's offset, -0.691. More channels
//! than one add their mean squares, as BS.1770 does (left and right
//! weighing 1 each); one channel is the original's sum.

/// The original's rate, and its filter for it.
const ORIGINAL_RATE: u32 = 11_025;

/// Stage 1, high shelf (+4 dB, Q 1/sqrt 2, 1500 Hz), for 11 025 Hz:
/// b0 b1 b2 a1 a2. Recalculated by the RBJ cookbook's bilinear transform
/// for that rate; the 48 kHz table would read about 0.1 LU off there.
const ORIGINAL_SHELF: [f64; 5] = [1.38913282926416, -1.334347523892191, 0.471855655650174, -0.744739880908181, 0.271380841930324];
/// Stage 2, the RLB high pass (38.13507 Hz), for 11 025 Hz.
const ORIGINAL_HIGH_PASS: [f64; 5] = [0.984749705922840, -1.969499411845681, 0.984749705922840, -1.969266826852296, 0.969731996839065];
/// LUFS = offset + 10 log10(mean square). The standard's -0.691 holds at
/// 48 kHz; at 11 025 Hz the shelf weighs 997 Hz a little less (about 0.59
/// against 0.66 dB), and this keeps the 997 Hz sine at -3.01 LUFS.
const ORIGINAL_OFFSET: f64 = -0.5899;
const STANDARD_OFFSET: f64 = -0.691;

/// The gates: absolute, and relative to the mean of what passed it.
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_DELTA: f64 = -10.0;

/// The K-weighting for one rate: two biquads (b0 b1 b2 a1 a2, a0 = 1) and
/// the offset that goes with them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KWeighting {
    pub shelf: [f64; 5],
    pub high_pass: [f64; 5],
    pub offset: f64,
}

impl KWeighting {
    pub fn for_rate(rate: u32) -> Self {
        if rate == ORIGINAL_RATE {
            return Self { shelf: ORIGINAL_SHELF, high_pass: ORIGINAL_HIGH_PASS, offset: ORIGINAL_OFFSET };
        }
        // libebur128's ebur128_init_filter, the standard's filters made for
        // any rate.
        let pi = std::f64::consts::PI;
        let rate = f64::from(rate);
        let (f0, g, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (pi * f0 / rate).tan();
        let vh = 10f64.powf(g / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        let shelf = [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0];
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (pi * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        let high_pass = [1.0, -2.0, 1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0];
        Self { shelf, high_pass, offset: STANDARD_OFFSET }
    }

    fn lufs(&self, mean_square: f64) -> f64 {
        self.offset + 10.0 * mean_square.log10()
    }
}

/// One channel K-weighted, as f64: the two biquads in one pass, direct
/// form II transposed (the least rounding piling up).
pub fn apply_k_weighting(input: impl IntoIterator<Item = f32>, k: &KWeighting) -> Vec<f64> {
    let [b0, b1, b2, a1, a2] = k.shelf;
    let [c0, c1, c2, d1, d2] = k.high_pass;
    let (mut s1w1, mut s1w2, mut s2w1, mut s2w2) = (0.0, 0.0, 0.0, 0.0);
    input
        .into_iter()
        .map(|x| {
            let x = f64::from(x);
            let y1 = b0 * x + s1w1;
            s1w1 = b1 * x - a1 * y1 + s1w2;
            s1w2 = b2 * x - a2 * y1;
            let y2 = c0 * y1 + s2w1;
            s2w1 = c1 * y1 - d1 * y2 + s2w2;
            s2w2 = c2 * y1 - d2 * y2;
            y2
        })
        .collect()
}

/// Integrated loudness in LUFS of K-weighted channels (all as long),
/// minus infinity for silence or nothing over the absolute gate.
pub fn integrated_loudness(channels: &[Vec<f64>], rate: u32, k: &KWeighting) -> f64 {
    let total = channels.first().map_or(0, Vec::len);
    // 400 ms blocks, a quarter of that on (at 11 025 Hz: 4410 and 1102).
    let block = (0.4 * f64::from(rate)).round() as usize;
    let hop = block / 4;
    let power = |start: usize, len: usize| channels.iter().map(|c| mean_square(c, start, len)).sum::<f64>();
    if total < block {
        // Too short for a block: one, ungated.
        let ms = power(0, total);
        return if ms > 0.0 { k.lufs(ms) } else { f64::NEG_INFINITY };
    }
    let blocks: Vec<f64> = (0..=total - block).step_by(hop.max(1)).map(|start| power(start, block)).filter(|&ms| ms > 0.0 && k.lufs(ms) >= ABSOLUTE_GATE_LUFS).collect();
    if blocks.is_empty() {
        return f64::NEG_INFINITY;
    }
    let ungated = k.lufs(blocks.iter().sum::<f64>() / blocks.len() as f64);
    let threshold = ungated + RELATIVE_GATE_DELTA;
    let kept: Vec<f64> = blocks.into_iter().filter(|&ms| k.lufs(ms) >= threshold).collect();
    if kept.is_empty() {
        // Everything gated out: the ungated mean, as the original falls back.
        return ungated;
    }
    k.lufs(kept.iter().sum::<f64>() / kept.len() as f64)
}

fn mean_square(buf: &[f64], start: usize, len: usize) -> f64 {
    if len == 0 {
        return 0.0;
    }
    buf[start..start + len].iter().map(|x| x * x).sum::<f64>() / len as f64
}

/// Integrated loudness of interleaved samples (`channels` of them a
/// frame) at `rate`.
pub fn integrated_lufs(samples: &[f32], channels: usize, rate: u32) -> f64 {
    let k = KWeighting::for_rate(rate);
    let weighted: Vec<Vec<f64>> = (0..channels).map(|c| apply_k_weighting(samples.iter().skip(c).step_by(channels).copied(), &k)).collect();
    integrated_loudness(&weighted, rate, &k)
}

/// Loudness and sample peak, what ReplayGain's tags take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loudness {
    pub integrated_lufs: f64,
    /// The largest sample, any channel (linear).
    pub peak: f64,
}

/// Loudness and peak; none for nothing, no rate, or silence (under -80
/// LUFS).
pub fn detect(samples: &[f32], channels: usize, rate: u32) -> Option<Loudness> {
    if samples.is_empty() || rate == 0 || channels == 0 {
        return None;
    }
    let lufs = integrated_lufs(samples, channels, rate);
    if lufs == f64::NEG_INFINITY || lufs < -80.0 {
        return None;
    }
    let peak = samples.iter().fold(0.0f64, |m, &s| m.max(f64::from(s).abs()));
    Some(Loudness { integrated_lufs: lufs, peak })
}

#[cfg(test)]
mod tests {
    //! The original's Bs1770LoudnessDetectorTests (mono at 11 025 Hz, its
    //! signals and tolerances), then the rates Voxcade plays at.

    use super::*;

    const RATE: u32 = 11_025;

    fn sine(freq: f64, amplitude: f64, seconds: f64, rate: u32) -> Vec<f32> {
        let n = (seconds * f64::from(rate)) as usize;
        (0..n).map(|i| (amplitude * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin()) as f32).collect()
    }

    #[test]
    fn nothing_no_rate_or_silence_measures_nothing() {
        assert_eq!(detect(&[], 1, RATE), None);
        assert_eq!(detect(&sine(997.0, 1.0, 2.0, RATE), 1, 0), None);
        assert_eq!(detect(&vec![0.0; RATE as usize * 3], 1, RATE), None);
        // About -120 dBFS, far under the absolute gate.
        assert_eq!(detect(&sine(997.0, 1e-6, 5.0, RATE), 1, RATE), None);
    }

    /// BS.1770's calibration: a full-scale 997 Hz sine reads -3.01 LUFS.
    #[test]
    fn a_full_scale_997_hz_sine_reads_minus_3() {
        let lufs = detect(&sine(997.0, 1.0, 5.0, RATE), 1, RATE).unwrap().integrated_lufs;
        assert!((-3.31..=-2.71).contains(&lufs), "{lufs}");
    }

    #[test]
    fn half_the_amplitude_reads_6_lu_lower() {
        let full = detect(&sine(997.0, 1.0, 5.0, RATE), 1, RATE).unwrap().integrated_lufs;
        let half = detect(&sine(997.0, 0.5, 5.0, RATE), 1, RATE).unwrap().integrated_lufs;
        assert!((5.5..=6.5).contains(&(full - half)), "{}", full - half);
    }

    #[test]
    fn the_peak_is_the_largest_sample() {
        let full = detect(&sine(997.0, 1.0, 5.0, RATE), 1, RATE).unwrap().peak;
        assert!((0.99..=1.01).contains(&full), "{full}");
        let half = detect(&sine(997.0, 0.5, 5.0, RATE), 1, RATE).unwrap().peak;
        assert!((0.49..=0.51).contains(&half), "{half}");
    }

    #[test]
    fn a_normal_sine_gives_plausible_numbers() {
        let r = detect(&sine(880.0, 0.7, 5.0, RATE), 1, RATE).unwrap();
        assert!((-60.0..=0.0).contains(&r.integrated_lufs), "{}", r.integrated_lufs);
        assert!((0.0..=1.0).contains(&r.peak), "{}", r.peak);
    }

    /// At 48 kHz the formulas give the standard's own table (BS.1770-4,
    /// tables 1 and 2).
    #[test]
    fn at_48_khz_the_filter_is_the_standards() {
        let k = KWeighting::for_rate(48_000);
        let shelf = [1.53512485958697, -2.69169618940638, 1.19839281085285, -1.69065929318241, 0.73248077421585];
        let high_pass = [1.0, -2.0, 1.0, -1.99004745483398, 0.99007225036621];
        for (got, want) in k.shelf.iter().chain(&k.high_pass).zip(shelf.iter().chain(&high_pass)) {
            assert!((got - want).abs() < 1e-8, "{got} against {want}");
        }
        assert_eq!(k.offset, -0.691);
    }

    /// At Voxcade's rate and at 48 kHz the calibration holds: 997 Hz at
    /// full scale in one channel of two reads -3.01 LUFS, in both 0.
    #[test]
    fn the_calibration_holds_at_the_mixers_rate() {
        for rate in [44_100, 48_000] {
            let mono = sine(997.0, 1.0, 5.0, rate);
            let left: Vec<f32> = mono.iter().flat_map(|&s| [s, 0.0]).collect();
            let both: Vec<f32> = mono.iter().flat_map(|&s| [s, s]).collect();
            let (one, two) = (integrated_lufs(&left, 2, rate), integrated_lufs(&both, 2, rate));
            assert!((one + 3.01).abs() < 0.02, "{rate} Hz, one channel: {one}");
            assert!(two.abs() < 0.02, "{rate} Hz, both: {two}");
        }
    }

    /// The gates: a quiet stretch 20 LU under the rest, as long, counts for
    /// nothing (an average of both would read 3 LU lower), but for the few
    /// blocks that reach over the change.
    #[test]
    fn a_quiet_stretch_is_gated_out() {
        let loud = sine(997.0, 0.5, 5.0, 44_100);
        let mut with_quiet = loud.clone();
        with_quiet.extend(sine(997.0, 0.05, 5.0, 44_100));
        let (a, b) = (integrated_lufs(&loud, 1, 44_100), integrated_lufs(&with_quiet, 1, 44_100));
        assert!((a - b).abs() < 0.2, "{a} {b}");
    }
}

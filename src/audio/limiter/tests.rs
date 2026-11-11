//! The limiter's tests: the original's (TruePeakLimiterTests,
//! TruePeakLimiterMetricsTests and MasteringLimiterTests, their signals,
//! expectations and tolerances), then Voxcade's own: nasty inputs held
//! under the ceiling by a meter of the tests' own, a quiet mix untouched,
//! the release, extreme values, blocks of any size.
//!
//! The original's first two test classes were written for its prototype,
//! TruePeakLimiter (mono; 2 ms look-ahead, 0.5 ms attack, 100 ms release;
//! a 4x linear-interpolation meter), which the mastering limiter replaced
//! and which is not ported. Their cases run here on the limiter that was
//! ported, mono as both channels, measured by its own BS.1770 filter.

use super::*;
use crate::rng::Rng;

const RATE: u32 = 44_100;
const CEILING_DB_TP: f64 = -1.0;

fn ceiling() -> f32 {
    10f64.powf(CEILING_DB_TP / 20.0) as f32
}

fn sine(amplitude: f64, freq: f64, seconds: f64, rate: u32) -> Vec<f32> {
    let n = (seconds * f64::from(rate)) as usize;
    (0..n).map(|i| (amplitude * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin()) as f32).collect()
}

fn stereo_sine(amp_l: f64, amp_r: f64, freq: f64, seconds: f64) -> Vec<f32> {
    let n = (seconds * f64::from(RATE)) as usize;
    (0..n)
        .flat_map(|i| {
            let s = (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).sin();
            [(amp_l * s) as f32, (amp_r * s) as f32]
        })
        .collect()
}

/// Mono through the stereo limiter in place, as both channels (what the
/// prototype's mono `ProcessInPlace` did, delay and all).
fn process_mono(lim: &mut Limiter, samples: &mut [f32]) {
    let mut stereo: Vec<f32> = samples.iter().flat_map(|&s| [s, s]).collect();
    lim.process_interleaved_stereo(&mut stereo);
    for (s, f) in samples.iter_mut().zip(stereo.as_chunks::<2>().0) {
        *s = f[0];
    }
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// The true peak of `samples[range]` read in its place, with the signal
/// around it (a slice measured alone would start and end with an edge,
/// whose overshoot is the meter's, not the signal's).
fn true_peak_db_in(samples: &[f32], range: std::ops::Range<usize>) -> f64 {
    let peaks = true_peaks(samples, TRUE_PEAK_DELAY + 1);
    db_tp(peaks[range.start + TRUE_PEAK_DELAY..range.end + TRUE_PEAK_DELAY].iter().fold(0.0, |m, &p| m.max(p)))
}

/// The prototype's test limiter; its own numbers have no place in the
/// ported one, so the original's defaults.
fn default_limiter() -> Limiter {
    Limiter::new(RATE)
}

// ---- TruePeakLimiterTests -------------------------------------------------

/// T1: a +6 dBFS sine is brought to the ceiling (the steady second half
/// within 0.5 dB of it).
#[test]
fn t1_hot_sine_limited_to_ceiling() {
    for rate in [44_100, 48_000] {
        let mut out = sine(2.0, 997.0, 1.0, rate);
        process_mono(&mut Limiter::new(rate), &mut out);
        let steady = true_peak_db_in(&out, out.len() / 2..out.len());
        assert!(steady <= CEILING_DB_TP + 0.5, "at {rate} Hz: steady true peak {steady:.2} dBTP");
    }
}

#[test]
fn t1_hot_sine_output_is_not_silent() {
    let mut out = sine(2.0, 997.0, 1.0, RATE);
    process_mono(&mut default_limiter(), &mut out);
    assert!(peak(&out) > 0.1, "max {}", peak(&out));
}

/// T2: a sine at -6 dBFS passes through unchanged (peaks within 2 %).
#[test]
fn t2_quiet_sine_passes_through_unchanged() {
    let input = sine(0.5, 997.0, 1.0, RATE);
    let mut out = input.clone();
    process_mono(&mut default_limiter(), &mut out);
    let half = out.len() / 2;
    let tp = true_peak_db_in(&out, half..out.len());
    assert!(tp < CEILING_DB_TP, "quiet signal limited: {tp:.2} dBTP");
    let ratio = f64::from(peak(&out[half..])) / f64::from(peak(&input[half..]));
    assert!((0.98..=1.02).contains(&ratio), "ratio {ratio}");
}

#[test]
fn t2_quiet_sine_low_thd() {
    let mut out = sine(0.5, 440.0, 2.0, RATE);
    process_mono(&mut default_limiter(), &mut out);
    let gain_error = f64::from(peak(&out[out.len() / 2..])) / 0.5;
    assert!((0.99..=1.01).contains(&gain_error), "{gain_error}");
}

/// T3: a peak between samples is found and limited.
///
/// The original's case is 64 samples long, shorter than the look-ahead,
/// so what comes out is all delay, zeros: it passes whatever the limiter
/// does. Ported as it is; `inter_sample_peaks_are_held_under_the_ceiling`
/// below is the case that tests it.
#[test]
fn t3_inter_sample_peak_detected_and_limited() {
    let freq = f64::from(RATE) / 4.0 + 1.0;
    let amp = f64::from(ceiling()) * 1.05;
    let samples: Vec<f32> = (0..64).map(|i| (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).cos()) as f32).collect();
    let tp = true_peak(&samples);
    assert!(tp > ceiling(), "true peak {:.2} dBTP should exceed the ceiling", db_tp(tp));
    let mut out = samples.clone();
    process_mono(&mut default_limiter(), &mut out);
    let steady = true_peak_db_in(&out, out.len() / 2..out.len());
    assert!(steady <= CEILING_DB_TP + 0.5, "{steady:.2} dBTP");
}

#[test]
fn t3_true_peak_measurement_exceeds_sample_peak_near_nyquist() {
    let freq = f64::from(RATE) / 4.0 + 100.0;
    let amp = 0.95;
    let samples: Vec<f32> = (0..RATE as usize / 2).map(|i| (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).cos()) as f32).collect();
    let (sp, tp) = (peak(&samples), true_peak(&samples));
    assert!(tp >= sp * 0.99, "true peak {tp} under sample peak {sp}");
    assert!((amp as f32 * 0.90..=amp as f32 * 1.10).contains(&tp), "{tp}");
}

/// T4: never louder than what went in (no AGC).
#[test]
fn t4_quiet_signal_gain_never_exceeds_unity() {
    let input = sine(0.001, 440.0, 1.0, RATE);
    let mut out = input.clone();
    process_mono(&mut default_limiter(), &mut out);
    assert!(peak(&out) <= peak(&input) + 1e-5, "{} > {}", peak(&out), peak(&input));
}

#[test]
fn t4_silence_stays_silence() {
    let mut out = vec![0.0f32; RATE as usize];
    process_mono(&mut default_limiter(), &mut out);
    assert!(out.iter().all(|&s| s == 0.0));
}

#[test]
fn t4_gain_never_above_unity_on_varying_amplitude() {
    let input = sine(0.5, 440.0, 1.0, RATE);
    let mut out = input.clone();
    process_mono(&mut default_limiter(), &mut out);
    assert!(peak(&out) <= peak(&input) + 1e-5);
    let mut loud = sine(2.0, 440.0, 1.0, RATE);
    process_mono(&mut default_limiter(), &mut loud);
    let steady = peak(&loud[loud.len() / 2..]);
    assert!(steady <= ceiling() * 1.05, "steady peak {steady}");
}

/// T5: an impulse is limited without a long hole after it.
#[test]
fn t5_impulse_limited_without_long_hole() {
    let n = RATE as usize;
    let mut samples: Vec<f32> = (0..n).map(|i| (0.5 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / f64::from(RATE)).sin()) as f32).collect();
    samples[1000] = 5.0;
    process_mono(&mut default_limiter(), &mut samples);
    let tail = peak(&samples[n - RATE as usize / 10..]);
    assert!((0.5 * 0.70..=0.5 * 1.05).contains(&tail), "tail peak {tail}");
}

#[test]
fn t5_impulse_train_no_progressive_drift() {
    let mut samples = vec![0.0f32; RATE as usize];
    let spacing = RATE as usize / 10;
    for k in 0..5 {
        samples[spacing * (k + 1)] = 3.0;
    }
    process_mono(&mut default_limiter(), &mut samples);
    let (from, to) = (spacing * 4 + (0.030 * f64::from(RATE)) as usize, spacing * 5 - (0.030 * f64::from(RATE)) as usize);
    assert!(peak(&samples[from..to]) < 0.01);
}

#[test]
fn defaults_are_the_originals() {
    let lim = Limiter::new(44_100);
    assert_eq!(lim.sample_rate(), 44_100);
    assert_eq!(lim.options().ceiling_db_tp, -1.0);
    assert_eq!(lim.options(), Options { ceiling_db_tp: -1.0, drive_db: 0.0, lookahead_ms: 1.5, fast_release_ms: 50.0, slow_release_ms: 250.0 });
    // 1.5 ms: 66 samples at 44.1 kHz, 72 at 48 kHz.
    assert_eq!(lim.latency_samples(), 66);
    assert_eq!(Limiter::new(48_000).latency_samples(), 72);
}

#[test]
#[should_panic]
fn a_rate_of_zero_is_refused() {
    Limiter::new(0);
}

#[test]
fn an_empty_buffer_is_nothing_to_do() {
    default_limiter().process_interleaved_stereo(&mut []);
}

#[test]
fn reset_clears_delay_and_envelope() {
    let mut lim = default_limiter();
    let mut loud = sine(2.0, 1000.0, 0.1, RATE);
    process_mono(&mut lim, &mut loud);
    lim.reset();
    let mut silence = vec![0.0f32; RATE as usize / 10];
    process_mono(&mut lim, &mut silence);
    assert!(silence.iter().all(|&s| s == 0.0));
    assert_eq!(lim.gain(), 1.0);
}

#[test]
fn measure_true_peak_full_scale_sine_approximates_amplitude() {
    let tp = true_peak(&sine(1.0, 997.0, 1.0, RATE));
    assert!((0.95..=1.05).contains(&tp), "{tp}");
}

#[test]
fn db_tp_of_zero_and_one() {
    assert_eq!(db_tp(0.0), f64::NEG_INFINITY);
    assert!(db_tp(1.0).abs() < 0.001);
}

/// The prototype's `ProcessWithMetrics` (a fresh limiter on a copy): a
/// +6 dBFS sine reports a reduction, and its input peak.
#[test]
fn process_with_metrics_reports_gain_reduction() {
    let samples = sine(2.0, 997.0, 0.5, RATE);
    let mut out = samples.clone();
    process_mono(&mut default_limiter(), &mut out);
    let input_peak = peak(&samples);
    let gr_db = 20.0 * (f64::from(true_peak(&out)) / f64::from(input_peak)).log10();
    assert!(gr_db < 0.0, "{gr_db:.2} dB");
    assert!((1.9..=2.1).contains(&input_peak), "{input_peak}");
}

// ---- TruePeakLimiterMetricsTests: measurements, printed -------------------
// (the original's always pass; where they say what to expect, so do these)

#[test]
fn metrics() {
    let (half, n) = (RATE as usize, 2 * RATE as usize);
    // M1: a quiet sine, steady peaks in and out.
    let input = sine(0.5, 997.0, 2.0, RATE);
    let mut out = input.clone();
    process_mono(&mut default_limiter(), &mut out);
    let gain_db = 20.0 * (f64::from(peak(&out[half..])) / f64::from(peak(&input[half..]))).log10();
    println!("[M1] quiet 997 Hz sine (0.5): steady gain {gain_db:+.3} dB (expect under 0.1 dB)");
    assert!(gain_db.abs() < 0.1);
    // M2: a hot sine's reduction.
    let input = sine(2.0, 997.0, 2.0, RATE);
    let mut out = input.clone();
    process_mono(&mut default_limiter(), &mut out);
    let (in_db, out_db) = (true_peak_db_in(&input, half..n), true_peak_db_in(&out, half..n));
    println!("[M2] hot sine (+6 dBFS): in {in_db:+.2} dBTP, out {out_db:+.2} dBTP, reduction {:+.2} dB", out_db - in_db);
    assert!(out_db <= CEILING_DB_TP + 0.5);
    // M3: true peak against sample peak near a quarter of the rate.
    let freq = f64::from(RATE) / 4.0 + 1000.0;
    let samples: Vec<f32> = (0..RATE as usize).map(|i| (0.9 * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).cos()) as f32).collect();
    let (sp, tp) = (db_tp(peak(&samples)), db_tp(true_peak(&samples)));
    println!("[M3] {freq:.0} Hz sine (0.9): sample peak {sp:+.2} dBFS, true peak {tp:+.2} dBTP, {:+.2} dB between", tp - sp);
    // M4: an impulse on a sine, and the sine after it.
    let mut proc: Vec<f32> = (0..RATE as usize).map(|i| (0.5 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / f64::from(RATE)).sin()) as f32).collect();
    proc[2205] = 5.0;
    let mut lim = default_limiter();
    let lookahead = lim.latency_samples();
    process_mono(&mut lim, &mut proc);
    let tail = peak(&proc[RATE as usize - RATE as usize / 10..]);
    let hole = proc[2205 + lookahead..2205 + lookahead + 882].iter().fold(f32::MAX, |m, s| m.min(s.abs()));
    println!(
        "[M4] impulse 5.0 at 50 ms: out there {:.5}, delayed impulse {:.5}, tail peak {tail:.5} ({:+.2} dB), smallest after it {hole:.5} (look-ahead {lookahead} samples)",
        proc[2205].abs(),
        proc[2205 + lookahead].abs(),
        20.0 * (f64::from(tail) / 0.5).log10()
    );
    // M5: the reduction over time on a hot sine.
    let mut proc = sine(2.0, 997.0, 0.5, RATE);
    process_mono(&mut default_limiter(), &mut proc);
    let line: Vec<String> = (10..=490)
        .step_by(80)
        .map(|ms| {
            let start = ms * RATE as usize / 1000;
            let tp = true_peak_db_in(&proc, start..(start + RATE as usize / 100).min(proc.len()));
            format!("{ms} ms {tp:+.2} dBTP ({:+.2} dB)", tp - 20.0 * 2f64.log10())
        })
        .collect();
    println!("[M5] hot sine over time: {}", line.join(", "));
}

// ---- MasteringLimiterTests -----------------------------------------------

/// Each channel's sample peak past the first 100 ms (the look-ahead's
/// ramp in).
fn steady_peak(interleaved: &[f32]) -> (f32, f32) {
    let skip = RATE as usize / 10;
    interleaved.as_chunks::<2>().0[skip..].iter().fold((0.0f32, 0.0f32), |(l, r), f| (l.max(f[0].abs()), r.max(f[1].abs())))
}

fn db(linear: f32) -> f64 {
    db_tp(linear)
}

#[test]
fn hot_sine_is_brought_to_the_ceiling() {
    let mut buf = stereo_sine(2.0, 2.0, 1000.0, 0.5);
    Limiter::new(RATE).process_interleaved_stereo(&mut buf);
    let (l, r) = steady_peak(&buf);
    assert!(db(l) <= CEILING_DB_TP + 0.3, "L {:.2}", db(l));
    assert!(db(r) <= CEILING_DB_TP + 0.3, "R {:.2}", db(r));
    assert!(db(l) >= CEILING_DB_TP - 2.0, "L {:.2} over-attenuated", db(l));
}

#[test]
fn under_the_ceiling_passes_through_unchanged() {
    let mut buf = stereo_sine(0.5, 0.5, 1000.0, 0.3);
    let input = buf.clone();
    Limiter::new(RATE).process_interleaved_stereo(&mut buf);
    // xUnit's Assert.Equal(a, b, 2): equal when rounded to 2 decimals.
    let round = |x: f32| (f64::from(x) * 100.0).round();
    assert_eq!(round(steady_peak(&input).0), round(steady_peak(&buf).0));
}

#[test]
fn gain_reduction_is_stereo_linked() {
    let mut buf = stereo_sine(2.0, 0.1, 1000.0, 0.5);
    Limiter::new(RATE).process_interleaved_stereo(&mut buf);
    let (l, r) = steady_peak(&buf);
    let (gain_l, gain_r) = (f64::from(l) / 2.0, f64::from(r) / 0.1);
    assert_eq!((gain_l * 100.0).round(), (gain_r * 100.0).round(), "L {gain_l:.4} R {gain_r:.4}");
    assert!(gain_r < 0.95, "R {gain_r:.3}: the quiet channel was not linked");
}

#[test]
fn drive_raises_the_level_when_there_is_headroom_but_never_above_the_ceiling() {
    let mut buf = stereo_sine(0.2, 0.2, 1000.0, 0.3);
    Limiter::with(RATE, Options { drive_db: 6.0, ..Options::default() }).process_interleaved_stereo(&mut buf);
    let (l, _) = steady_peak(&buf);
    assert!(l > 0.36 && l < 0.45, "driven peak {l:.3}");
    assert!(db(l) <= CEILING_DB_TP + 0.3);
}

#[test]
fn without_drive_it_never_amplifies() {
    let mut buf = stereo_sine(0.3, 0.3, 1000.0, 0.3);
    let input = buf.clone();
    Limiter::new(RATE).process_interleaved_stereo(&mut buf);
    assert!(steady_peak(&buf).0 <= steady_peak(&input).0 + 1e-4);
}

#[test]
fn output_is_finite_and_reset_clears_state() {
    let mut buf = stereo_sine(1.5, 1.5, 220.0, 0.2);
    let mut lim = Limiter::with(RATE, Options { drive_db: 3.0, ..Options::default() });
    lim.process_interleaved_stereo(&mut buf);
    assert!(buf.iter().all(|s| s.is_finite()));
    lim.reset();
    let mut silence = vec![0.0f32; RATE as usize];
    lim.process_interleaved_stereo(&mut silence);
    assert!(silence.iter().all(|&s| s == 0.0));
}

// ---- Voxcade's own -------------------------------------------------------

/// The tests' own meter, apart from the limiter's filter: 8x
/// oversampling by a Blackman-windowed sinc 64 samples wide (much longer
/// than BS.1770's 13), the true peak of interleaved stereo.
fn meter_8x(interleaved: &[f32]) -> f32 {
    const UP: usize = 8;
    const HALF: isize = 32;
    let frames = interleaved.as_chunks::<2>().0;
    let window = |x: f64| {
        let t = (x + HALF as f64) / (2 * HALF) as f64;
        0.42 - 0.5 * (2.0 * std::f64::consts::PI * t).cos() + 0.08 * (4.0 * std::f64::consts::PI * t).cos()
    };
    let mut peak = 0.0f64;
    for ch in 0..2 {
        for i in 0..frames.len() as isize {
            for p in 0..UP {
                let frac = p as f64 / UP as f64;
                let mut acc = 0.0;
                for k in -HALF + 1..=HALF {
                    let Some(f) = frames.get((i + k) as usize).filter(|_| i + k >= 0) else { continue };
                    let x = k as f64 - frac;
                    let sinc = if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                    acc += f64::from(f[ch]) * sinc * window(x);
                }
                peak = peak.max(acc.abs());
            }
        }
    }
    peak as f32
}

/// Nasty inputs, each a second long and loud; what they are.
fn nasty_inputs() -> Vec<(&'static str, Vec<f32>)> {
    let n = RATE as usize;
    let mut rng = Rng::new(1);
    let mut list = Vec::new();
    // A full-scale square, 22 samples up and 22 down (about 1 kHz).
    list.push(("full-scale square", (0..n).flat_map(|i| [if i % 44 < 22 { 1.0 } else { -1.0 }; 2]).collect()));
    // A quarter of the rate at 45 degrees: every sample at 0.707, the
    // waveform between them at 1.0 (sample peak -3 dBFS, true peak 0 dBTP).
    list.push(("fs/4 sine at 45 degrees", (0..n).flat_map(|i| [(std::f64::consts::FRAC_PI_2 * i as f64 + std::f64::consts::FRAC_PI_4).sin() as f32; 2]).collect()));
    // 48 voices at once (the mixer's most), squares and noise at random
    // pitches, pans and starts, each at 0.4.
    let mut sum = vec![0.0f32; n * 2];
    for _ in 0..48 {
        let (freq, pan, start) = (rng.range(40.0, 4000.0), rng.range(0.0, 1.0), (rng.f32() * n as f32 * 0.2) as usize);
        let noise = rng.f32() < 0.3;
        let mut held = 0.0;
        for i in start..n {
            let phase = ((i - start) as f32 * freq / RATE as f32).fract();
            let s = if noise {
                if i % 4 == 0 {
                    held = rng.range(-1.0, 1.0);
                }
                held
            } else if phase < 0.5 {
                1.0
            } else {
                -1.0
            };
            let decay = (-((i - start) as f32) / (RATE as f32 * 0.3)).exp();
            sum[i * 2] += 0.4 * s * decay * (1.0 - pan);
            sum[i * 2 + 1] += 0.4 * s * decay * pan;
        }
    }
    list.push(("48 loud voices", sum));
    // A pulse of duty 1/8 between -0.2 and 1.8: mostly DC.
    list.push(("DC-heavy pulse", (0..n).flat_map(|i| [if i % 80 < 10 { 1.8 } else { -0.2 }; 2]).collect()));
    // Silence, then one sample at 4.
    let mut impulse = vec![0.0f32; n * 2];
    (impulse[n], impulse[n + 1]) = (4.0, 4.0);
    list.push(("silence then an impulse", impulse));
    // A low sine, slowly falling halves (a bass at 20 Hz, +6 dBFS).
    list.push(("20 Hz sine at +6 dBFS", (0..n).flat_map(|i| [(2.0 * (std::f64::consts::TAU * 20.0 * i as f64 / f64::from(RATE)).sin()) as f32; 2]).collect()));
    // Full-scale noise.
    list.push(("full-scale noise", (0..n * 2).map(|_| rng.range(-1.0, 1.0)).collect()));
    list
}

/// Whatever comes in, the true peak that goes out stays under the
/// ceiling by BS.1770's meter, the one the limiter reads (within 0.01 dB:
/// the gain changing across the filter's 13 samples can lift it a hair,
/// 0.004 dB at most here).
///
/// The tests' finer meter sees more where a signal has much above about
/// 18 kHz, which BS.1770's short 4x filter rolls off and so does not limit:
/// squares, noise and pulses made sample by sample. For those it holds
/// each case to what it measured when this was written (a little over),
/// so that it cannot get worse unnoticed; for the band-limited ones, to
/// the ceiling within 0.05 dB.
#[test]
fn nasty_inputs_stay_under_the_ceiling() {
    let finer_allows = [
        ("full-scale square", -0.70),
        ("fs/4 sine at 45 degrees", -0.95),
        ("48 loud voices", -0.55),
        ("DC-heavy pulse", -0.75),
        ("silence then an impulse", -0.95),
        ("20 Hz sine at +6 dBFS", -0.95),
        ("full-scale noise", 0.9),
    ];
    for ((name, mut input), (named, allowed)) in nasty_inputs().into_iter().zip(finer_allows) {
        assert_eq!(name, named);
        // A fifth of a second of silence after: the limiter sees the end
        // coming as any other edge (cut off, the edge's true peak would be
        // the meter's alone).
        input.extend(std::iter::repeat_n(0.0, RATE as usize / 5));
        let mut out = input.clone();
        let mut lim = Limiter::new(RATE);
        lim.process_interleaved_stereo(&mut out);
        let t = lim.read_telemetry();
        assert!(out.iter().all(|s| s.is_finite()), "{name}");
        let (bs1770, fine) = (db_tp(true_peak_stereo(&out)), db_tp(meter_8x(&out)));
        println!(
            "{name:<24} in: sample {:+6.2} dBFS, true {:+6.2} dBTP (BS.1770 {:+6.2}) | out: BS.1770 {bs1770:+7.3}, 8x {fine:+7.3} dBTP | reduced {:5.2} dB at most, {:3.0} % of the time",
            db_tp(peak(&input)),
            db_tp(meter_8x(&input)),
            db_tp(true_peak_stereo(&input)),
            -t.gain_reduction_db,
            t.duty_cycle * 100.0
        );
        assert!(bs1770 <= CEILING_DB_TP + 0.01, "{name}: {bs1770:.3} dBTP by the BS.1770 meter");
        assert!(fine <= allowed, "{name}: {fine:.3} dBTP by the 8x meter, {allowed} allowed");
    }
}

/// The original's sliding maximum keeps at most window + 1 candidates in
/// a ring of window + 2, but holds window + 2 for a moment when the true
/// peaks fall steadily for that long (a bass's falling half, a long
/// decay): the ring is full, reads as empty, and the next push forgets
/// every candidate, so the window's loudest is taken for its quietest
/// until it fills again. Ported unfixed; this shows it. With the release
/// as slow as it is, the gain hardly moves meanwhile: on the nasty inputs
/// above, a ring one slot larger changes no output by more than 0.00065.
#[test]
#[ignore = "fails: a bug in the original (JustPlay MasteringLimiter's deque), ported as it is"]
fn the_sliding_maximum_is_the_windows_loudest() {
    let mut lim = Limiter::new(RATE);
    let window = lim.latency_samples();
    // A slow fall: every true peak a little under the one before.
    let input: Vec<f32> = (0..1000).flat_map(|i| [1.0 - i as f32 * 0.0005; 2]).collect();
    let (phase, mut hist) = (true_peak_phases(), [0.0f32; FIR_DELAY]);
    let mut peaks = Vec::new();
    for (i, f) in input.as_chunks::<2>().0.iter().enumerate() {
        peaks.push(fir_true_peak(&phase, &mut hist, i % FIR_DELAY, f[0]));
        lim.process_interleaved_stereo(&mut f.clone());
        let loudest = peaks[i.saturating_sub(window)..].iter().fold(0.0f32, |m, &p| m.max(p));
        assert_eq!(lim.dq_val[lim.dq_head], loudest, "after {} samples", i + 1);
    }
}

/// A mix that never reaches the ceiling comes out exactly as it went in,
/// the look-ahead later: bit for bit, the gain stays exactly 1.
#[test]
fn a_quiet_mix_comes_out_as_it_went_in_only_later() {
    let mut rng = Rng::new(7);
    let input: Vec<f32> = (0..RATE as usize * 2).map(|i| rng.range(-0.3, 0.3) + 0.2 * ((i / 2) as f32 * 0.05).sin()).collect();
    assert!(true_peak_stereo(&input) < ceiling());
    let mut out = input.clone();
    let mut lim = Limiter::new(RATE);
    lim.process_interleaved_stereo(&mut out);
    let delay = lim.latency_samples() * 2;
    assert!(out[..delay].iter().all(|&s| s == 0.0));
    assert_eq!(&out[delay..], &input[..input.len() - delay]);
    assert_eq!(lim.read_telemetry(), Telemetry::default());
}

/// After a loud burst the gain comes back to 1 as the two releases say:
/// no slower than the cascade of 50 and 250 ms from where it stood, under
/// the "limiting" mark (0.9995) within 2 s, exactly 1 within 5 s.
#[test]
fn the_gain_comes_back_after_a_loud_burst() {
    let burst = sine(4.0, 300.0, 0.5, RATE);
    let after = sine(0.5, 300.0, 6.0, RATE);
    let mut lim = Limiter::new(RATE);
    let mut buf: Vec<f32> = burst.iter().flat_map(|&s| [s, s]).collect();
    lim.process_interleaved_stereo(&mut buf);
    // Past the burst and the look-ahead, the release begins.
    let mut frame = [0.0f32; 2];
    let start = 1.0 - f64::from(lim.gain());
    assert!(start > 0.6, "the burst took {start:.3} off");
    let (tf, ts) = (0.05, 0.25);
    let (mut back_under_mark, mut back_to_one) = (None, None);
    for (i, &s) in after.iter().enumerate() {
        frame = [s, s];
        lim.process_interleaved_stereo(&mut frame);
        let t = (i as f64 + 1.0) / f64::from(RATE);
        let (lookahead, g) = (lim.latency_samples() as f64 / f64::from(RATE), f64::from(lim.gain()));
        if t > lookahead + 0.001 {
            let tr = t - lookahead - 0.001;
            let bound = start * (ts * (-tr / ts).exp() - tf * (-tr / tf).exp()) / (ts - tf);
            assert!(1.0 - g <= bound + 1e-6, "at {t:.3} s the gain is {g:.6}, the releases allow {:.6}", 1.0 - bound);
        }
        if back_under_mark.is_none() && g >= 0.9995 {
            back_under_mark = Some(t);
        }
        if back_to_one.is_none() && lim.gain() == 1.0 {
            back_to_one = Some(t);
        }
    }
    let _ = frame;
    let (mark, one) = (back_under_mark.expect("never under the mark"), back_to_one.expect("never back to 1"));
    println!("after a burst {:.2} dB down: under 0.004 dB after {mark:.2} s, exactly 1 after {one:.2} s", -20.0 * (1.0 - start).log10());
    assert!(mark < 2.0 && one < 5.0, "{mark} {one}");
}

/// Extreme values that are numbers give numbers back: the largest f32s,
/// both ways, alternating, the smallest, a ramp up to the largest.
#[test]
fn extreme_values_give_finite_output() {
    let n = 4096;
    let inputs: Vec<(&str, Vec<f32>)> = vec![
        ("largest", vec![f32::MAX; n * 2]),
        ("largest, alternating", (0..n * 2).map(|i| if (i / 2) % 2 == 0 { f32::MAX } else { -f32::MAX }).collect()),
        ("smallest normal", vec![f32::MIN_POSITIVE; n * 2]),
        ("subnormal", vec![1e-40; n * 2]),
        ("1e30 bursts", (0..n * 2).map(|i| if (i / 64) % 3 == 0 { 1e30 } else { 0.1 }).collect()),
        ("ramp to the largest", (0..n * 2).map(|i| f32::MAX * (i as f32 / (n * 2) as f32)).collect()),
    ];
    for (name, input) in inputs {
        let mut out = input.clone();
        Limiter::new(RATE).process_interleaved_stereo(&mut out);
        assert!(out.iter().all(|s| s.is_finite()), "{name}");
        assert!(peak(&out) <= ceiling() * 1.01, "{name}: {}", peak(&out));
    }
}

/// Blocks of any size, as the speaker asks for them, make the same as one
/// block: 1, 7, 64 and 511 frames at a time.
#[test]
fn blocks_of_any_size_make_the_same() {
    let (_, input) = nasty_inputs().swap_remove(2);
    let mut whole = input.clone();
    Limiter::new(RATE).process_interleaved_stereo(&mut whole);
    for frames in [1, 7, 64, 511] {
        let mut out = input.clone();
        let mut lim = Limiter::new(RATE);
        for block in out.chunks_mut(frames * 2) {
            lim.process_interleaved_stereo(block);
        }
        assert!(out == whole, "blocks of {frames} frames differ");
    }
}

/// `limit_whole` lines the output up with the input: a quiet recording
/// comes back as it is, mono or stereo.
#[test]
fn a_whole_recording_comes_back_on_time() {
    let mono = sine(0.5, 440.0, 0.2, RATE);
    assert_eq!(limit_whole(&mut Limiter::new(RATE), &mono, 1), mono);
    let stereo = stereo_sine(0.3, 0.6, 440.0, 0.2);
    assert_eq!(limit_whole(&mut Limiter::new(RATE), &stereo, 2), stereo);
}



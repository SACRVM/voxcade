//! A sound as a picture, for whoever cannot hear it: a spectrogram, time
//! left to right, pitch up on a log axis, loudness in a dark-to-light
//! ramp. Our own code, no simulation and no audio thread, so plain floats.
//! The textbook parts: the Hann window and the decimation-in-time radix-2
//! FFT (Cooley and Tukey; any signal-processing text, e.g. Oppenheim and
//! Schafer, "Discrete-Time Signal Processing", chapters on the DFT and
//! the FFT); the ramp's stops are ours, in the spirit of matplotlib's
//! `inferno` (dark through purple and orange to pale yellow).

use anyhow::{Result, bail};

/// The window's length in samples, a power of two.
pub const WINDOW: usize = 2048;
/// How far a column of the picture moves on, at the most.
pub const HOP: usize = 512;
/// The pitches the picture spans, Hz.
pub const LOWEST: f64 = 30.0;
pub const HIGHEST: f64 = 16_000.0;
/// The loudness it spans: this many decibels below its brightest.
pub const RANGE_DB: f64 = 80.0;
/// The picture's size at the most, margins included.
pub const WIDEST: u32 = 1600;
pub const TALLEST: u32 = 400;
/// The margins: left for the pitch's labels, under for the time's.
const LEFT: u32 = 26;
const UNDER: u32 = 20;
/// A short sound is laid out over at least this many columns.
const AT_LEAST: usize = 400;
/// The shortest stretch a picture is made of, in samples (a window).
pub const SHORTEST: usize = WINDOW;

/// The picture and what its axes are.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// RGBA, rows from the top.
    pub rgba: Vec<u8>,
    /// The plot's columns and rows (the picture less its margins).
    pub columns: u32,
    pub rows: u32,
    /// The seconds the plot's left and right edges stand for.
    pub from: f64,
    pub to: f64,
    /// The decibels (full scale 0) of the brightest point: the ramp's top.
    pub top_db: f64,
}

impl Picture {
    /// The axes in words, for a reader who sees only the picture.
    pub fn says(&self) -> String {
        format!(
            "{}x{} px; the plot is {} columns by {} rows at x {LEFT}..{}, y 0..{}; time runs left to right from {:.2} s to {:.2} s ({:.4} s a pixel); pitch is a log axis, {LOWEST} Hz at the bottom row to {} Hz at the top row, labels at 100, 1K and 10K; \
             brightness is loudness from {:.0} dB (black) to {:.1} dB (pale yellow), dB against full scale, {RANGE_DB} dB of range below the brightest point",
            self.width,
            self.height,
            self.columns,
            self.rows,
            LEFT + self.columns - 1,
            self.rows - 1,
            self.from,
            self.to,
            (self.to - self.from) / f64::from(self.columns),
            HIGHEST,
            self.top_db - RANGE_DB,
            self.top_db
        )
    }
}

/// The discrete Fourier transform of `re` + i `im` in place, by the
/// radix-2 decimation-in-time FFT (Cooley and Tukey); the length is a
/// power of two.
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n, "an FFT of a power of two");
    // The bit-reversal permutation.
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = (i.reverse_bits() >> (usize::BITS - bits)) % n;
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    // Butterflies of 2, 4, 8 ... points, with the twiddle factors e^(-2 pi i k / size).
    let mut size = 2;
    while size <= n {
        let half = size / 2;
        let step = -2.0 * std::f64::consts::PI / size as f64;
        for start in (0..n).step_by(size) {
            for k in 0..half {
                let (s, c) = (step * k as f64).sin_cos();
                let (a, b) = (start + k, start + k + half);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                (re[a], im[a]) = (re[a] + tr, im[a] + ti);
            }
        }
        size *= 2;
    }
}

/// The Hann window of `WINDOW` samples (periodic, as for spectral work).
fn hann() -> Vec<f64> {
    (0..WINDOW).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / WINDOW as f64).cos()).collect()
}

/// The ramp: loudness 0 (the range's floor) to 1 (its top) as a colour.
fn ramp(v: f64) -> [u8; 3] {
    const STOPS: [[f64; 3]; 5] = [[0.0, 0.0, 4.0], [87.0, 16.0, 110.0], [188.0, 55.0, 84.0], [249.0, 142.0, 9.0], [252.0, 255.0, 164.0]];
    let at = v.clamp(0.0, 1.0) * 4.0;
    let i = (at as usize).min(3);
    let f = at - i as f64;
    std::array::from_fn(|c| (STOPS[i][c] + (STOPS[i + 1][c] - STOPS[i][c]) * f).round() as u8)
}

/// The spectrogram of `mono` (at `rate` Hz) between `from` and `to`
/// seconds (the ends of the sound where unsaid).
pub fn spectrogram(mono: &[f32], rate: u32, from: Option<f64>, to: Option<f64>) -> Result<Picture> {
    let total = mono.len() as f64 / f64::from(rate);
    let (from, to) = (from.unwrap_or(0.0), to.unwrap_or(total));
    if !(from.is_finite() && to.is_finite()) || from < 0.0 || to <= from {
        bail!("from {from} to {to} seconds: a stretch begins at 0 or later and ends after it begins (the sound lasts {total:.2} s)");
    }
    if from >= total {
        bail!("from {from} seconds is past the end: the sound lasts {total:.2} s");
    }
    let to = to.min(total);
    let first = (from * f64::from(rate)) as usize;
    let last = ((to * f64::from(rate)) as usize).min(mono.len());
    if last - first < SHORTEST / 4 {
        bail!("{:.3} seconds are too short to draw (at least {:.3})", (last - first) as f64 / f64::from(rate), SHORTEST as f64 / 4.0 / f64::from(rate));
    }
    // The stretch, padded with silence to one window at least; a sound a
    // file made of floats may hold what is no number: silence.
    let mut stretch: Vec<f64> = mono[first..last].iter().map(|&s| if s.is_finite() { f64::from(s) } else { 0.0 }).collect();
    stretch.resize(stretch.len().max(WINDOW), 0.0);
    let reach = stretch.len() - WINDOW;
    // Columns: one a hop where the sound is long enough for them, the hop
    // grown as the sound is longer, or the columns made more for a short one.
    let natural = reach / HOP + 1;
    let columns = (natural * AT_LEAST.div_ceil(natural)).min((WIDEST - LEFT) as usize);
    let rows = (TALLEST - UNDER) as usize;
    let window = hann();
    let amplitude = window.iter().sum::<f64>() / 2.0;
    // Each row's span of bins (a row is a band of pitch; at the low end
    // narrower than a bin, where the bins are read between).
    let ratio = (HIGHEST / LOWEST).ln();
    let hz = |row: f64| LOWEST * (ratio * (rows as f64 - row) / rows as f64).exp();
    let bin = |f: f64| f * WINDOW as f64 / f64::from(rate);
    let nyquist = (WINDOW / 2) as f64;
    let spans: Vec<(f64, f64)> = (0..rows).map(|r| (bin(hz(r as f64 + 1.0)).min(nyquist), bin(hz(r as f64)).min(nyquist))).collect();
    // Every column's decibels per row.
    let mut db = vec![0.0f64; columns * rows];
    let (mut re, mut im) = (vec![0.0; WINDOW], vec![0.0; WINDOW]);
    let mut magnitude = vec![0.0; WINDOW / 2 + 1];
    for x in 0..columns {
        let at = if columns == 1 { 0 } else { reach * x / (columns - 1) };
        for i in 0..WINDOW {
            re[i] = stretch[at + i] * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for (k, m) in magnitude.iter_mut().enumerate() {
            *m = re[k].hypot(im[k]) / amplitude;
        }
        for (r, &(lo, hi)) in spans.iter().enumerate() {
            let m = if hi - lo < 1.0 {
                // Between two bins.
                let mid = 0.5 * (lo + hi);
                let k = (mid as usize).min(WINDOW / 2 - 1);
                let f = mid - k as f64;
                magnitude[k] * (1.0 - f) + magnitude[k + 1] * f
            } else {
                // Many bins to the row: the loudest of them.
                magnitude[lo.ceil() as usize..=(hi.floor() as usize).min(WINDOW / 2)].iter().fold(0.0f64, |a, &b| a.max(b))
            };
            db[x * rows + r] = 20.0 * m.max(1e-12).log10();
        }
    }
    let top_db = db.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let (width, height) = (LEFT + columns as u32, TALLEST);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut put = |x: u32, y: u32, c: [u8; 3]| {
        let i = ((y * width + x) * 4) as usize;
        rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
    };
    // The margins first, then the plot.
    for y in 0..height {
        for x in 0..width {
            put(x, y, [18, 18, 22]);
        }
    }
    for x in 0..columns {
        for r in 0..rows {
            put(LEFT + x as u32, r as u32, ramp((db[x * rows + r] - top_db + RANGE_DB) / RANGE_DB));
        }
    }
    let mut picture = Picture { width, height, rgba, columns: columns as u32, rows: rows as u32, from, to, top_db };
    draw_axes(&mut picture);
    Ok(picture)
}

/// Ticks and labels: pitch at 50 to 10K (labelled at 100, 1K, 10K), time
/// at whole seconds in a step that leaves labels room.
fn draw_axes(p: &mut Picture) {
    let (width, rows) = (p.width, p.rows);
    let ratio = (HIGHEST / LOWEST).ln();
    let light = [200, 200, 210];
    for (f, label) in [(50.0, ""), (100.0, "100"), (200.0, ""), (500.0, ""), (1000.0, "1K"), (2000.0, ""), (5000.0, ""), (10_000.0, "10K")] {
        let y = ((ratio - (f / LOWEST).ln()) / ratio * f64::from(rows)).round() as i64;
        if !(0..i64::from(rows)).contains(&y) {
            continue;
        }
        let y = y as u32;
        let strong = !label.is_empty();
        // A faint line over the plot, a tick into the margin.
        for x in LEFT..width {
            blend(p, x, y, [255, 255, 255], if strong { 0.22 } else { 0.1 });
        }
        for x in LEFT - 3..LEFT {
            blend(p, x, y, light, 1.0);
        }
        if strong {
            let w = label.len() as u32 * 6 - 1;
            text(p, label, LEFT.saturating_sub(5 + w), y.saturating_sub(3).min(rows - 7), light);
        }
    }
    // Time: the least step among 1, 2, 5, 10 ... seconds that leaves 56 px.
    let per_px = (p.to - p.from) / f64::from(p.columns);
    let step = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0].into_iter().find(|s| s / per_px >= 56.0).unwrap_or(1200.0);
    let mut t = (p.from / step).ceil() * step;
    while t <= p.to {
        let x = LEFT + ((t - p.from) / per_px).round() as u32;
        if x < width {
            for y in 0..rows {
                blend(p, x, y, [255, 255, 255], 0.18);
            }
            for y in rows..rows + 3 {
                blend(p, x, y, light, 1.0);
            }
            let label = format!("{}S", t.round() as u64);
            let w = label.len() as u32 * 6 - 1;
            if x + w / 2 < width && x >= LEFT + w / 2 {
                text(p, &label, x - w / 2, rows + 5, light);
            }
        }
        t += step;
    }
}

/// `c` laid over the pixel at `x`, `y` by `alpha`.
fn blend(p: &mut Picture, x: u32, y: u32, c: [u8; 3], alpha: f64) {
    let i = ((y * p.width + x) * 4) as usize;
    for (k, &on) in c.iter().enumerate() {
        p.rgba[i + k] = (f64::from(p.rgba[i + k]) * (1.0 - alpha) + f64::from(on) * alpha).round() as u8;
    }
}

/// A line in the pixel font (capitals, digits and a few signs), its top
/// left at `x`, `y`.
fn text(p: &mut Picture, s: &str, x: u32, y: u32, c: [u8; 3]) {
    let mut at = x;
    for ch in s.chars() {
        if let Some((_, rows)) = crate::ui::FONT.iter().find(|(g, _)| *g == ch) {
            for (dy, row) in rows.iter().take(7).enumerate() {
                for (dx, bit) in row.bytes().enumerate() {
                    if bit == b'#' && at + (dx as u32) < p.width && y + (dy as u32) < p.height {
                        blend(p, at + dx as u32, y + dy as u32, c, 1.0);
                    }
                }
            }
        }
        at += 6;
    }
}

/// A WAV file's bytes to look at: its rate and its frames in one channel,
/// every size checked against the bytes, at most `MOST_BYTES` of them and
/// `LONGEST` seconds long.
pub fn read_wav(file: &str, bytes: &[u8]) -> Result<(u32, Vec<f32>)> {
    super::music::sample::read_to_look_at(file, bytes, LONGEST)
}

/// The longest sound a picture is made of, seconds (half an hour).
pub const LONGEST: f32 = 1800.0;
/// The biggest WAV file it reads, bytes.
pub const MOST_BYTES: u64 = 200 << 20;

#[cfg(test)]
mod tests {
    use super::*;

    /// A direct DFT, the sums as the definition has them.
    fn dft(input: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let n = input.len();
        let (mut re, mut im) = (vec![0.0; n], vec![0.0; n]);
        for k in 0..n {
            for (t, &x) in input.iter().enumerate() {
                let a = -2.0 * std::f64::consts::PI * (k * t) as f64 / n as f64;
                re[k] += x * a.cos();
                im[k] += x * a.sin();
            }
        }
        (re, im)
    }

    /// The FFT is the DFT: on a few signals of 16 and 64 samples, to the
    /// float's last digits.
    #[test]
    fn the_fft_is_the_dft() {
        for n in [2usize, 16, 64] {
            let signal: Vec<f64> = (0..n).map(|i| ((i * 7 + 3) % 11) as f64 - 5.0 + (i as f64 * 0.7).sin()).collect();
            let (want_re, want_im) = dft(&signal);
            let (mut re, mut im) = (signal.clone(), vec![0.0; n]);
            fft(&mut re, &mut im);
            for k in 0..n {
                assert!((re[k] - want_re[k]).abs() < 1e-9 && (im[k] - want_im[k]).abs() < 1e-9, "n {n} bin {k}: {} {} against {} {}", re[k], im[k], want_re[k], want_im[k]);
            }
        }
    }

    /// A sine of 440 Hz stands in its row, a full scale one at 0 dB.
    #[test]
    fn a_sine_is_bright_in_its_own_row() {
        let rate = 44_100;
        let sine: Vec<f32> = (0..rate * 2).map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / f64::from(rate)).sin() as f32).collect();
        let p = spectrogram(&sine, rate, None, None).unwrap();
        let row = |f: f64| ((((HIGHEST / LOWEST).ln() - (f / LOWEST).ln()) / (HIGHEST / LOWEST).ln()) * f64::from(p.rows)).round() as u32;
        let at = |f: f64| {
            let (x, y) = (LEFT + p.columns / 2, row(f));
            let i = ((y * p.width + x) * 4) as usize;
            u32::from(p.rgba[i]) + u32::from(p.rgba[i + 1]) + u32::from(p.rgba[i + 2])
        };
        assert!(at(440.0) > at(220.0) + 150 && at(440.0) > at(880.0) + 150, "{} {} {}", at(220.0), at(440.0), at(880.0));
        // Off a bin's middle a Hann window loses up to 1.42 dB (scalloping).
        assert!(p.top_db > -1.6 && p.top_db < 0.1, "a full scale sine is within 1.5 dB of 0 dB, not {}", p.top_db);
        assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
        assert!(p.width <= WIDEST && p.height == TALLEST);
    }

    /// A long sound still fits, a stretch is cut, nonsense is a mistake.
    #[test]
    fn long_sounds_fit_and_a_stretch_is_cut() {
        let rate = 22_050;
        let long = vec![0.1f32; rate as usize * 120];
        assert_eq!(spectrogram(&long, rate, None, None).unwrap().width, WIDEST);
        let part = spectrogram(&long, rate, Some(10.0), Some(12.0)).unwrap();
        assert!((part.from - 10.0).abs() < 1e-9 && (part.to - 12.0).abs() < 1e-9);
        for (from, to) in [(Some(5.0), Some(2.0)), (Some(-1.0), None), (Some(500.0), None), (Some(f64::NAN), None)] {
            assert!(spectrogram(&long, rate, from, to).is_err(), "{from:?} {to:?}");
        }
        assert!(spectrogram(&[0.0; 10], rate, None, None).is_err());
        // A sound of NaNs is a silent picture, not a crash.
        assert!(spectrogram(&vec![f32::NAN; 5000], rate, None, None).is_ok());
    }
}

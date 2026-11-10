//! Reading `.wav` files: PCM 8/16/24/32 bit or 32-bit float, any number of
//! channels and any rate, mixed down to mono and resampled to SAMPLE_RATE.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::synth::SAMPLE_RATE;

pub fn load(files: &crate::files::Files, path: &Path) -> Result<Vec<f32>> {
    let bytes = files.read(path).with_context(|| format!("reading {}", path.display()))?;
    decode(&bytes).with_context(|| format!("decoding {}", path.display()))
}

pub fn decode(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("not a WAVE file");
    }
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let mut format = None;
    let mut data = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32_at(bytes, at + 4) as usize;
        let body = &bytes[at + 8..(at + 8 + len).min(bytes.len())];
        match id {
            b"fmt " if body.len() >= 16 => {
                // 0xFFFE (extensible) keeps the real format in its sub-format.
                let mut tag = u16_at(body, 0);
                if tag == 0xFFFE && body.len() >= 26 {
                    tag = u16_at(body, 24);
                }
                format = Some((tag, u16_at(body, 2) as usize, u32_at(body, 4), u16_at(body, 14) as usize));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + len + (len & 1);
    }
    let (tag, channels, rate, bits) = format.context("no fmt chunk")?;
    let data = data.context("no data chunk")?;
    if channels == 0 || rate == 0 {
        bail!("{channels} channels at {rate} Hz");
    }
    let width = bits / 8;
    let sample = |i: usize| -> Option<f32> {
        let b = data.get(i * width..i * width + width)?;
        Some(match (tag, bits) {
            (1, 8) => (b[0] as f32 - 128.0) / 128.0,
            (1, 16) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (1, 32) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            _ => return None,
        })
    };
    if sample(0).is_none() && !data.is_empty() {
        bail!("unsupported format {tag} with {bits} bits");
    }
    let frames = data.len() / width / channels;
    let mono: Vec<f32> = (0..frames)
        .map(|f| (0..channels).filter_map(|c| sample(f * channels + c)).sum::<f32>() / channels as f32)
        .collect();
    Ok(resample(&mono, rate, SAMPLE_RATE))
}

/// Linear resampling from one rate to another.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let step = from as f64 / to as f64;
    let count = (samples.len() as f64 / step).floor() as usize;
    (0..count)
        .map(|i| {
            let x = i as f64 * step;
            let j = x as usize;
            let t = (x - j as f64) as f32;
            let a = samples[j];
            let b = *samples.get(j + 1).unwrap_or(&a);
            a + (b - a) * t
        })
        .collect()
}

/// Lo-fi like the speech in early 90s games: down to `rate` Hz (held, not
/// smoothed, so it keeps its grit) and to `bits` bits.
pub fn crush(samples: &[f32], rate: Option<u32>, bits: Option<u32>) -> Vec<f32> {
    let mut out = samples.to_vec();
    if let Some(rate) = rate.filter(|&r| r > 0 && r < SAMPLE_RATE) {
        let hold = SAMPLE_RATE as f32 / rate as f32;
        let mut next = 0.0f32;
        let mut value = 0.0;
        for (i, s) in out.iter_mut().enumerate() {
            if i as f32 >= next {
                value = *s;
                next += hold;
            }
            *s = value;
        }
    }
    if let Some(bits) = bits.filter(|&b| (1..16).contains(&b)) {
        let levels = (1u32 << (bits - 1)) as f32;
        for s in &mut out {
            *s = (*s * levels).round() / levels;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit stereo file at 22050 Hz comes back as mono at SAMPLE_RATE.
    #[test]
    fn a_stereo_wav_is_read_as_mono() {
        let frames: Vec<[i16; 2]> = (0..2205).map(|i| [((i % 50) * 400) as i16, ((i % 50) * 400) as i16]).collect();
        let data: Vec<u8> = frames.iter().flat_map(|f| f.iter().flat_map(|s| s.to_le_bytes())).collect();
        let mut wav = b"RIFF".to_vec();
        wav.extend((36 + data.len() as u32).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(22050u32.to_le_bytes());
        wav.extend((22050u32 * 4).to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((data.len() as u32).to_le_bytes());
        wav.extend(&data);
        let samples = decode(&wav).unwrap();
        assert_eq!(samples.len(), 4410);
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(*s));
        assert!((peak - 19600.0 / 32768.0).abs() < 0.01, "peak {peak}");
    }

    /// Crushing to 4 bits leaves at most 16 levels.
    #[test]
    fn crushing_leaves_few_levels() {
        let ramp: Vec<f32> = (0..1000).map(|i| i as f32 / 500.0 - 1.0).collect();
        let crushed = crush(&ramp, Some(7000), Some(4));
        let mut levels: Vec<i32> = crushed.iter().map(|s| (s * 1000.0) as i32).collect();
        levels.dedup();
        levels.sort();
        levels.dedup();
        assert!(levels.len() <= 17, "{} levels", levels.len());
    }
}

//! Samples: recordings a song's instrument plays (`@break sample
//! name=amen`), the files of a game's `samples/` folder. Only WAV, read by
//! our own small reader (RIFF, integer PCM of 16 or 24 bits, mono or
//! stereo, 22050 to 48000 Hz), each converted once as the game loads to
//! the engine's rate and to one channel (a song's voices are one channel
//! each, placed by their pan), and kept behind an `Arc`: the audio thread
//! only reads it. A sample changes only how a game sounds, never its run:
//! in neither the fingerprint nor the state hash, as the sounds.
//!
//! Everything a file says is checked before it is used: a size past the
//! file, a chunk that does not end, a sample longer than `LONGEST`, all of
//! a game's past `MOST_BYTES`, a format not read, each a plain mistake
//! naming the file, never a panic or a huge allocation.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, bail};

use super::SAMPLE_RATE;

/// The folder of a game the samples lie in.
pub const FOLDER: &str = "samples";

/// The longest a sample lasts, in seconds (as long as a sound): a sample
/// is a hit, a phrase, a loop of a few bars, not a song.
pub const LONGEST: f32 = 10.0;

/// The most bytes all of a game's samples together take as files: a game
/// played over a network sends its files to the others (`wire::MAX_BLOB`,
/// 66 MB, holds them with the rest).
pub const MOST_BYTES: u64 = 64 << 20;

/// The rates a sample may be recorded at, in Hz.
pub const RATES: (u32, u32) = (22_050, 48_000);

/// Endings of recordings that are not WAV, to say so instead of passing
/// them over.
const NOT_WAV: [&str; 7] = ["ogg", "mp3", "flac", "aif", "aiff", "m4a", "opus"];

/// A sample as the engine holds it: its name (the file's without its
/// ending), its file, the hash of its bytes (whether it changed), and its
/// frames at the engine's rate, one channel.
pub struct Recording {
    pub name: String,
    pub file: String,
    pub hash: String,
    pub frames: Vec<f32>,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({} frames)", self.file, self.frames.len())
    }
}

impl Recording {
    /// How long it lasts at the engine's rate, in seconds.
    pub fn seconds(&self) -> f32 {
        self.frames.len() as f32 / SAMPLE_RATE as f32
    }
}

/// A game's samples by name, shared, not copied, by every song that plays
/// one.
#[derive(Clone, Debug, Default)]
pub struct Samples {
    by_name: Arc<BTreeMap<String, Arc<Recording>>>,
}

impl Samples {
    /// The samples of a game's `samples/` folder, read through its files
    /// (sealed ones too). A recording of another kind there is a mistake
    /// saying so; other files (a README) are passed over.
    pub fn read_folder(files: &crate::files::Files) -> Result<Samples> {
        Self::read_within(files, MOST_BYTES)
    }

    /// As `read_folder`, at most `most` bytes in all.
    pub(super) fn read_within(files: &crate::files::Files, most: u64) -> Result<Samples> {
        Self::read_listed(files, Path::new(FOLDER), FOLDER, most)
    }

    /// The recordings in a folder of WAV files as it lies (a game's
    /// `samples/` folder, or any other holding WAVs; a folder holding a
    /// `samples/` folder is read there, as a game's), at most
    /// `MOST_BYTES` of them.
    pub fn read_dir(dir: &Path) -> Result<Samples> {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        let dir = if dir.join(FOLDER).is_dir() { dir.join(FOLDER) } else { dir };
        let label = dir.display().to_string();
        Self::read_listed(&crate::files::Files::folder(&dir), &dir, &label, MOST_BYTES)
    }

    /// The WAV files of `folder` (its path in `files`), named in mistakes
    /// as `label/NAME.wav`.
    fn read_listed(files: &crate::files::Files, folder: &Path, label: &str, most: u64) -> Result<Samples> {
        let mut by_name: BTreeMap<String, Arc<Recording>> = BTreeMap::new();
        let mut total = 0u64;
        for path in files.list(folder) {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            let Some((stem, ending)) = name.rsplit_once('.') else { continue };
            if name.starts_with('.') {
                continue;
            }
            let file = format!("{label}/{name}");
            let ending = ending.to_ascii_lowercase();
            if NOT_WAV.contains(&ending.as_str()) {
                bail!("{file}: only WAV is read (PCM, 16 or 24 bit): save it as a WAV");
            }
            if ending != "wav" {
                continue;
            }
            if let Some(other) = by_name.get(stem) {
                bail!("{} and {file}: two samples called \"{stem}\"; keep one", other.file);
            }
            let size = files.size(&path).map_err(|e| anyhow::anyhow!("{file}: {e}"))?;
            total = total.saturating_add(size);
            if total > most {
                bail!("{file}: the samples come to more than {} MB together, the most a game holds (a game played over a network sends them to the others)", most >> 20);
            }
            let bytes = files.read(&path).map_err(|e| anyhow::anyhow!("{file}: {e}"))?;
            let wav = decode(&file, &bytes)?;
            let frames = resample(&wav.frames, wav.rate);
            by_name.insert(stem.to_string(), Arc::new(Recording { name: stem.to_string(), file, hash: crate::files::hash(&bytes), frames }));
        }
        Ok(Samples { by_name: Arc::new(by_name) })
    }

    /// Samples of these recordings (tests).
    #[cfg(test)]
    pub(super) fn of(recordings: Vec<Recording>) -> Samples {
        Samples { by_name: Arc::new(recordings.into_iter().map(|r| (r.name.clone(), Arc::new(r))).collect()) }
    }

    pub fn get(&self, name: &str) -> Option<&Arc<Recording>> {
        self.by_name.get(name)
    }

    /// Their names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    /// Whether these are the same files as `other` (by name and bytes).
    pub fn same(&self, other: &Samples) -> bool {
        self.by_name.len() == other.by_name.len() && self.by_name.iter().zip(other.by_name.iter()).all(|((a, x), (b, y))| a == b && x.hash == y.hash)
    }
}

/// Whether `path` is a file of the game's samples folder (`samples/x.wav`
/// in `dir`), new, changed or gone.
pub fn in_folder(dir: &Path, path: &Path) -> bool {
    let folder = dir.join(FOLDER);
    path.parent().is_some_and(|p| p == folder || p.canonicalize().is_ok_and(|p| folder.canonicalize().is_ok_and(|f| f == p)))
}

/// A WAV file read: its rate and its frames, one channel (a stereo file's
/// two sides added at half each).
#[derive(Debug)]
pub(super) struct Wav {
    pub(super) rate: u32,
    pub(super) frames: Vec<f32>,
}

/// The PCM subformat's GUID after its first two bytes (the format's
/// number), as WAVE_FORMAT_EXTENSIBLE writes it.
const PCM_GUID_TAIL: [u8; 14] = [0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71];

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The format a WAV's `fmt ` chunk states.
struct Format {
    channels: u16,
    rate: u32,
    bits: u16,
}

/// The `fmt ` chunk's body read, or the plain mistake of a format not read.
fn format(file: &str, body: &[u8], loose: bool) -> Result<Format> {
    if body.len() < 16 {
        bail!("{file}: its format (the fmt chunk) is cut short: {} bytes, 16 at least", body.len());
    }
    let mut tag = u16_at(body, 0);
    let (channels, rate, block, bits) = (u16_at(body, 2), u32_at(body, 4), u16_at(body, 12), u16_at(body, 14));
    if tag == 0xFFFE {
        // WAVE_FORMAT_EXTENSIBLE: the real format is the subformat's GUID.
        if body.len() < 40 {
            bail!("{file}: its extensible format is cut short: {} bytes, 40 at least", body.len());
        }
        let guid = &body[24..40];
        if guid[2..] != PCM_GUID_TAIL {
            bail!("{file}: a format of its own (an extensible WAV whose subformat is no standard one): save it as a WAV of PCM, 16 or 24 bit");
        }
        tag = u16_at(guid, 0);
    }
    let float = loose && tag == 3 && bits == 32;
    match tag {
        1 => {}
        3 if float => {}
        3 => bail!("{file}: 32-bit float; a sample is integer PCM of 16 or 24 bit: export it so"),
        6 | 7 => bail!("{file}: compressed as telephones do (A-law or µ-law); a sample is PCM of 16 or 24 bit"),
        other => bail!("{file}: compressed (format {other}); a sample is PCM of 16 or 24 bit, not compressed"),
    }
    match bits {
        16 | 24 => {}
        32 if float => {}
        8 => bail!("{file}: 8 bit; a sample is 16 or 24 bit"),
        32 => bail!("{file}: 32-bit integers; a sample is 16 or 24 bit"),
        other => bail!("{file}: {other} bits a sample; a sample is 16 or 24 bit"),
    }
    match channels {
        1 | 2 => {}
        0 => bail!("{file}: no channels"),
        n => bail!("{file}: {n} channels; a sample is mono or stereo"),
    }
    if loose && !(8_000..=192_000).contains(&rate) {
        bail!("{file}: recorded at {rate} Hz; a picture is of a WAV from 8000 to 192000 Hz");
    }
    if !loose && !(RATES.0..=RATES.1).contains(&rate) {
        bail!("{file}: recorded at {rate} Hz; a sample is from {} to {} Hz", RATES.0, RATES.1);
    }
    if u32::from(block) != u32::from(channels) * u32::from(bits / 8) {
        bail!("{file}: its frames are {block} bytes, which {channels} channel{} of {bits} bits are not", if channels == 1 { "" } else { "s" });
    }
    Ok(Format { channels, rate, bits })
}

/// A WAV file's bytes as a recording at its own rate, one channel; every
/// size it states checked against the bytes there are before it is used.
pub(super) fn decode(file: &str, bytes: &[u8]) -> Result<Wav> {
    decode_with(file, bytes, None)
}

/// A WAV of any length up to `longest` seconds, any rate from 8000 to
/// 192000 Hz and 32-bit floats too, to be looked at (`audio::picture`),
/// not played: its rate and its frames, one channel. Every size checked
/// against the bytes as `decode` does.
pub fn read_to_look_at(file: &str, bytes: &[u8], longest: f32) -> Result<(u32, Vec<f32>)> {
    let wav = decode_with(file, bytes, Some(longest))?;
    Ok((wav.rate, wav.frames))
}

/// `decode`, with a sample's rules (`longest` None) or those of a WAV to
/// look at.
fn decode_with(file: &str, bytes: &[u8], longest: Option<f32>) -> Result<Wav> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        let kind = match bytes.get(0..4) {
            Some(b"RIFX") => "a big-endian WAV (RIFX), which is not read",
            Some(b"RF64") => "a WAV of 64-bit sizes (RF64), which is not read",
            _ => "not a WAV (no RIFF and WAVE at its start)",
        };
        bail!("{file}: {kind}");
    }
    let mut at = 12usize;
    let mut fmt: Option<Format> = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4) as usize;
        let body_at = at + 8;
        let left = bytes.len() - body_at;
        let name = String::from_utf8_lossy(id).trim().to_string();
        if size > left {
            if id == b"data" && fmt.is_some() {
                bail!("{file}: cut short: its sound says {size} bytes, the file holds {left} after its start");
            }
            bail!("{file}: cut short: the chunk \"{name}\" says {size} bytes, the file holds {left}");
        }
        let body = &bytes[body_at..body_at + size];
        match id {
            b"fmt " => fmt = Some(format(file, body, longest.is_some())?),
            b"data" => {
                let Some(f) = &fmt else { bail!("{file}: its sound (data) comes before its format (fmt), which a WAV states first") };
                let width = usize::from(f.bits / 8) * usize::from(f.channels);
                let count = size / width;
                if count == 0 {
                    bail!("{file}: holds no sound");
                }
                let seconds = count as f64 / f64::from(f.rate);
                match longest {
                    None if seconds > f64::from(LONGEST) + 1e-9 => bail!("{file}: lasts {seconds:.2} seconds; a sample is {LONGEST} seconds at most"),
                    Some(most) if seconds > f64::from(most) + 1e-9 => bail!("{file}: lasts {seconds:.0} seconds; a picture is of {most:.0} seconds at most"),
                    _ => {}
                }
                let one = |i: usize| -> f32 {
                    match f.bits {
                        16 => f32::from(i16::from_le_bytes([body[i], body[i + 1]])) / 32_768.0,
                        32 => f32::from_le_bytes([body[i], body[i + 1], body[i + 2], body[i + 3]]),
                        _ => (i32::from_le_bytes([0, body[i], body[i + 1], body[i + 2]]) >> 8) as f32 / 8_388_608.0,
                    }
                };
                let step = usize::from(f.bits / 8);
                let frames = (0..count)
                    .map(|n| {
                        let i = n * width;
                        if f.channels == 2 { 0.5 * (one(i) + one(i + step)) } else { one(i) }
                    })
                    .collect();
                return Ok(Wav { rate: f.rate, frames });
            }
            _ => {}
        }
        // A chunk of an odd size has a byte of padding after it.
        at = body_at + size + (size & 1);
    }
    match fmt {
        None => bail!("{file}: no format (fmt) and no sound (data) in it"),
        Some(_) => bail!("{file}: no sound (data) in it"),
    }
}

/// Taps of the converter on each side, at its cutoff: half the length of
/// its windowed sinc.
const TAPS: f64 = 32.0;
/// Steps the converter's kernel is laid out in between two input samples
/// (read between them in a straight line).
const PHASES: usize = 512;
/// How near the converter's cutoff lies to the lower rate's Nyquist.
const PASS: f64 = 0.9;

/// `input` at `from` Hz converted to the engine's rate: a windowed sinc
/// (Blackman's window), its cutoff a little under the lower rate's
/// Nyquist, its kernel worked out once into a table; the same at the
/// engine's rate.
pub(super) fn resample(input: &[f32], from: u32) -> Vec<f32> {
    let to = SAMPLE_RATE;
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    // The cutoff in cycles an input sample, and the kernel's half width
    // in input samples.
    let fc = 0.5 * PASS * (f64::from(to) / f64::from(from)).min(1.0);
    let half = (TAPS / (2.0 * fc)).ceil();
    let pi = std::f64::consts::PI;
    let kernel = |d: f64| -> f32 {
        if d >= half {
            return 0.0;
        }
        let x = 2.0 * fc * d;
        let sinc = if x == 0.0 { 1.0 } else { libm::sin(pi * x) / (pi * x) };
        let u = d / half;
        let window = 0.42 + 0.5 * libm::cos(pi * u) + 0.08 * libm::cos(2.0 * pi * u);
        (2.0 * fc * sinc * window) as f32
    };
    let table: Vec<f32> = (0..=half as usize * PHASES + 1).map(|i| kernel(i as f64 / PHASES as f64)).collect();
    let step = f64::from(from) / f64::from(to);
    let count = (input.len() as f64 / step).floor() as usize;
    let reach = half as i64;
    let last = input.len() as i64 - 1;
    (0..count)
        .map(|n| {
            let t = n as f64 * step;
            let i0 = t.floor() as i64;
            let mut sum = 0.0f32;
            for k in (i0 - reach + 1).max(0)..=(i0 + reach).min(last) {
                let at = (t - k as f64).abs() * PHASES as f64;
                let j = at as usize;
                let frac = (at - j as f64) as f32;
                let (a, b) = (table[j.min(table.len() - 1)], table[(j + 1).min(table.len() - 1)]);
                sum += input[k as usize] * (a + (b - a) * frac);
            }
            sum
        })
        .collect()
}

/// A WAV file of integer PCM (tests): `samples` interleaved, each -1 to 1,
/// at `rate`, of `channels` and `bits` (16 or 24); `extensible` writes
/// WAVE_FORMAT_EXTENSIBLE with the PCM subformat.
#[cfg(test)]
pub fn wav_file(rate: u32, channels: u16, bits: u16, extensible: bool, samples: &[f32]) -> Vec<u8> {
    let width = bits / 8;
    let mut data = Vec::new();
    for &s in samples {
        let full = (1i32 << (bits - 1)) - 1;
        let v = (s.clamp(-1.0, 1.0) * full as f32).round() as i32;
        data.extend_from_slice(&v.to_le_bytes()[..usize::from(width)]);
    }
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&(if extensible { 0xFFFEu16 } else { 1 }).to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&(rate * u32::from(channels * width)).to_le_bytes());
    fmt.extend_from_slice(&(channels * width).to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());
    if extensible {
        fmt.extend_from_slice(&22u16.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        fmt.extend_from_slice(&0u32.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&PCM_GUID_TAIL);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((4 + 8 + fmt.len() + 8 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    out.extend_from_slice(&fmt);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

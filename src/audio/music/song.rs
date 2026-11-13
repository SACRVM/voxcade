//! A song as read: its instruments, notes per channel and timing (`parse` makes it).

use super::parse::parse_song;
use super::{SAMPLE_RATE, Wave};

#[derive(Clone, Debug)]
pub struct Instrument {
    pub wave: Wave,
    pub duty: f32,
    pub pwm: f32,
    pub pwm_speed: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub vibrato: f32,
    pub vibrato_speed: f32,
    pub vibrato_delay: f32,
    pub arp: Vec<f32>,
    pub arp_speed: f32,
    pub sweep: f32,
    pub sweep_time: Option<f32>,
    pub freq: Option<f32>,
    pub lowpass: Option<f32>,
    /// The SID-style filter, off when None.
    pub filter: Option<FilterMode>,
    pub cutoff: f32,
    pub cutoff_end: Option<f32>,
    pub cutoff_time: Option<f32>,
    pub resonance: f32,
    /// How far this instrument's notes duck the other channels (0: not at all).
    pub sidechain: f32,
    pub sidechain_release: f32,
    /// How far this instrument follows the ducking.
    pub duck: f32,
    pub volume: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Low,
    Band,
    High,
}

impl Default for Instrument {
    fn default() -> Self {
        Self {
            wave: Wave::Square,
            duty: 0.5,
            pwm: 0.0,
            pwm_speed: 1.0,
            attack: 0.004,
            decay: 0.1,
            sustain: 0.7,
            release: 0.08,
            vibrato: 0.0,
            vibrato_speed: 6.0,
            vibrato_delay: 0.0,
            arp: Vec::new(),
            arp_speed: 50.0,
            sweep: 0.0,
            sweep_time: None,
            freq: None,
            lowpass: None,
            filter: None,
            cutoff: 1000.0,
            cutoff_end: None,
            cutoff_time: None,
            resonance: 0.0,
            sidechain: 0.0,
            sidechain_release: 0.2,
            duck: 1.0,
            volume: 0.5,
        }
    }
}

/// One note on a channel, in samples from the start of the song.
#[derive(Clone, Debug)]
pub(super) struct Note {
    pub(super) start: u64,
    /// How long the key is held; the release follows.
    pub(super) held: u64,
    /// None: a rest.
    pub(super) freq: Option<f32>,
    pub(super) instrument: usize,
    pub(super) volume: f32,
    pub(super) pan: f32,
}

#[derive(Debug)]
pub struct Song {
    pub title: String,
    /// Beats a minute (`#tempo`) and beats in a bar (`#meter`).
    pub tempo: f32,
    pub meter: u32,
    /// The tension each channel sounds from (`#layer`; 0: always).
    pub(super) layers: Vec<f32>,
    /// Channel letters in the order they first appear.
    pub names: Vec<char>,
    pub(super) instruments: Vec<Instrument>,
    pub(super) channels: Vec<Vec<Note>>,
    /// Samples until it starts over.
    pub length: u64,
    pub looping: bool,
}

/// When a song's beats fall, for a game that reads them (`music.beat`):
/// its name (the file's, without the folder and `.mml`), beats a minute,
/// beats in a bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    pub name: String,
    pub tempo: f32,
    pub meter: u32,
}

impl Song {
    /// A song of nothing, 120 beats a minute and 4 to a bar (in place of
    /// one that does not read).
    pub fn empty() -> Song {
        parse_song("", true).expect("an empty song reads")
    }

    /// Its beats, for a game: by its file's name.
    pub fn timing(&self, file: &str) -> Timing {
        let name = std::path::Path::new(file).file_stem().map_or_else(|| file.to_string(), |s| s.to_string_lossy().into_owned());
        Timing { name, tempo: self.tempo, meter: self.meter }
    }

    /// How long each channel is, in seconds (they should match, or the
    /// short ones fall silent before the song starts over).
    pub fn channel_lengths(&self) -> Vec<(char, f32)> {
        self.names
            .iter()
            .zip(&self.channels)
            .map(|(&name, notes)| (name, notes.last().map_or(0, |n| n.start) as f32 / SAMPLE_RATE as f32))
            .collect()
    }
}

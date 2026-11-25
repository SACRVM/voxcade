//! A song as read: its instruments, notes per channel and timing (`parse` makes it).

use super::parse::parse_song;
use super::clear::Seg;
use super::voice::Segment;
use super::{SAMPLE_RATE, Wave};

#[derive(Clone, Debug)]
pub struct Instrument {
    pub wave: Wave,
    /// Sings instead of playing `wave`: `voice` (`super::voice`) or the
    /// clear voice (`super::clear`).
    pub voice: Option<Voicing>,
    /// A voice's consonants (their hiss, bursts and breath) against its
    /// vowels, in decibels.
    pub consonants: f32,
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
    /// How hard the sound is pushed into a soft clipper, in decibels (0:
    /// not at all).
    pub drive: f32,
    /// Where the drive slides to on every note, evenly in decibels, and
    /// over how long (None: `attack + decay`); with an end the clipper
    /// works the whole note, a drive of 0 its gentlest.
    pub drive_end: Option<f32>,
    pub drive_time: Option<f32>,
    /// The bit crusher: the rate the sound is held at (Hz) and the bits it
    /// is rounded to, each off when None.
    pub crush_rate: Option<f32>,
    pub crush_bits: Option<u32>,
    /// The pitch a note begins at (Hz), falling exponentially in Hz to the
    /// note's own over `pitch_decay` seconds (its time constant); none
    /// when None.
    pub pitch_start: Option<f32>,
    pub pitch_decay: f32,
    /// How many oscillators play each note together (`UNISON` at most),
    /// spread evenly from `detune` semitones below the note to as far
    /// above it; one plays as ever.
    pub unison: u32,
    pub detune: f32,
    /// Its LFO, moving its filter's cutoff and its volume.
    pub lfo: Lfo,
    /// How `decay` and `release` fall: a straight line, or exponentially
    /// to -60 dB in their time.
    pub curve: Curve,
    /// Its further layers (`+` lines under its own, `LAYERS` with it at
    /// most), each an instrument of its own sounding with every note, and
    /// what their sum goes through (`=`). An instrument without either
    /// plays as one ever did.
    pub layers: Vec<Instrument>,
    pub sum: Option<Sum>,
    /// Plays a recording of the game's `samples/` instead of `wave` (the
    /// wave `sample`).
    pub sample: Option<Sampled>,
}

/// What an instrument of the wave `sample` plays: the recording by name
/// (`name`, its file in `samples/` without the ending), the note it plays
/// at its own speed at (`root`, in Hz), the stretch of it (`start`, `end`,
/// in seconds; None: to its end), whether that goes round while a note
/// sounds (`loop`) and whether backwards (`reverse`); the line of the song
/// that named it, and the recording once found (`Song::bind`; until then,
/// and where none was found, it plays nothing).
#[derive(Clone, Debug)]
pub struct Sampled {
    pub name: String,
    pub root: f32,
    pub start: f32,
    pub end: Option<f32>,
    pub looping: bool,
    pub reverse: bool,
    pub line: usize,
    pub recording: Option<std::sync::Arc<super::sample::Recording>>,
}

/// The most layers an instrument has, its own line one of them: the voice
/// holds the rest in a fixed array (`player::Voice`), so the audio thread
/// makes none.
pub const LAYERS: usize = 4;

/// The most oscillators an instrument's line plays together (`unison`):
/// each layer holds them in a fixed array (`player::Tone`).
pub const UNISON: usize = 7;

/// A line's LFO (`lfo_*`): how fast it turns (`speed` Hz from each note's
/// start, or one turn in `beats` of the song's), its wave, and how far it
/// moves the filter's cutoff (semitones up and down) and takes the volume
/// down (a share: 1 to silence at its lowest). Unless written it moves
/// nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lfo {
    pub speed: f32,
    pub beats: Option<f32>,
    pub wave: Wave,
    pub cutoff: f32,
    pub volume: f32,
}

impl Default for Lfo {
    fn default() -> Self {
        Self { speed: 5.0, beats: None, wave: Wave::Sine, cutoff: 0.0, volume: 0.0 }
    }
}

/// How an envelope's falls go (`curve`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    /// A straight line in level, to where it falls in its time.
    #[default]
    Linear,
    /// Exponential: its time is the fall to -60 dB of where it started
    /// (towards the sustain level for the decay, silence for the
    /// release), its last few milliseconds laid down to the target in a
    /// straight line, so it is there at its time without a step.
    Exp,
}

/// What a layered instrument's layers go through once added (`=`): a soft
/// clipper (`drive`, its gain 0 off, sliding to `drive_end` as an
/// instrument's), the filter, the crusher, a volume.
#[derive(Clone, Debug, PartialEq)]
pub struct Sum {
    pub drive: f32,
    pub drive_end: Option<f32>,
    pub drive_time: Option<f32>,
    pub volume: f32,
    pub filter: Option<FilterMode>,
    pub cutoff: f32,
    pub resonance: f32,
    pub crush_rate: Option<f32>,
    pub crush_bits: Option<u32>,
    /// Its LFO, on the sum's filter and its volume.
    pub lfo: Lfo,
    /// Which of the words a strip turns (`music:mix`) the line states:
    /// drive, cutoff, resonance turn the sum's then, not the first layer's
    /// (the drive's end and time with the drive, where it states either).
    pub drive_stated: bool,
    pub cutoff_stated: bool,
    pub resonance_stated: bool,
    /// The line states an LFO's word: a strip's turn the sum's LFO.
    pub lfo_stated: bool,
}

/// The words a sum line (`=`) takes.
pub const SUM_WORDS: [&str; 14] = [
    "drive", "drive_end", "drive_time", "volume", "filter", "cutoff", "resonance", "crush_rate", "crush_bits", "lfo_speed", "lfo_beats", "lfo_wave",
    "lfo_cutoff", "lfo_volume",
];

/// Which voice sings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voicing {
    /// `voice`: the first, the script's port.
    Formants,
    /// The clear voice (`super::clear::WAVE`).
    Clear,
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
            voice: None,
            consonants: 0.0,
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
            drive: 0.0,
            drive_end: None,
            drive_time: None,
            crush_rate: None,
            crush_bits: None,
            pitch_start: None,
            pitch_decay: 0.02,
            unison: 1,
            detune: 0.1,
            lfo: Lfo::default(),
            curve: Curve::Linear,
            layers: Vec::new(),
            sum: None,
            sample: None,
        }
    }
}

/// One note on a channel, in samples from the start of the song.
#[derive(Clone, Copy, Debug)]
pub(super) struct Note {
    pub(super) start: u64,
    /// How long the key is held; the release follows.
    pub(super) held: u64,
    /// None: a rest.
    pub(super) freq: Option<f32>,
    pub(super) instrument: usize,
    pub(super) volume: f32,
    pub(super) pan: f32,
    /// A voice's: the range of the song's `segments` it sings.
    pub(super) sung: Option<(u32, u32)>,
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
    /// Each instrument's name, by its index (empty where a later `@` of the
    /// same name took it over).
    pub(super) instrument_names: Vec<String>,
    pub(super) instruments: Vec<Instrument>,
    pub(super) channels: Vec<Vec<Note>>,
    /// What the voices' notes sing, each note a range of it: `voice`'s
    /// and the clear voice's.
    pub(super) segments: Vec<Segment>,
    pub(super) clear: Vec<Seg>,
    /// Samples until it starts over.
    pub length: u64,
    pub looping: bool,
    /// The bars that go round (`#loop FROM TO`, counted from 1; no TO:
    /// to the song's end), None without `#loop`.
    pub round: Option<(u32, Option<u32>)>,

    /// The samples that go round, from where to where, exactly (a bar is
    /// seldom a whole number of samples, and the audio goes round with the
    /// game's clock for hours): the whole song unless `#loop` says bars,
    /// whose ends lie `LOOP_SLACK` samples before their bar lines (so a
    /// loop of the song's first bars begins a little before 0).
    pub(super) span: (f64, f64),
    /// Bars begun in the song (`#meter` beats of `#tempo` each).
    pub bars: u32,
    /// Channels a game laid over a studio (`parse::lay`): each one's text
    /// as a number (a channel the same in the next laid song plays on
    /// untouched), and their digest (`Laid`); none for a file's song.
    pub(super) sources: Vec<u64>,
    pub(super) laid: Option<[u8; 32]>,
}

/// When a song's beats fall, for a game that reads them (`music.beat`):
/// its name (the file's, without the folder and `.mml`), beats a minute,
/// beats in a bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    pub name: String,
    pub tempo: f32,
    pub meter: u32,
    /// The bars that go round (`#loop`), and the bars the song has (a
    /// song begun at a bar, `music:play`'s `at`, begins within them).
    pub round: Option<(u32, Option<u32>)>,
    pub bars: u32,
    /// A studio's whole text (`Song::studio`): what a game's laid channels
    /// are read against; None for a song.
    pub studio: Option<std::sync::Arc<str>>,
    /// A song's channels, by letter (`music:mix` names them).
    pub letters: Vec<char>,
}

/// Samples into a song of `tempo` and `meter` bar `bar` (from 1) begins
/// at, exactly: seconds first, then samples.
pub(super) fn bar_line(tempo: f32, meter: u32, bar: u32) -> f64 {
    let beats = f64::from(bar.saturating_sub(1)) * f64::from(meter);
    beats * 60.0 / f64::from(tempo) * SAMPLE_RATE as f64
}

impl Timing {
    /// Why a song cannot begin at `bar` (from 1), if it cannot: a bar past
    /// its end, or after its `#loop`, which it never comes to.
    pub fn bar_mistake(&self, bar: u32) -> Option<String> {
        let s = |n: u32| if n == 1 { "" } else { "s" };
        match self.round {
            _ if bar > self.bars => Some(format!("is past the end of \"{}\", which has {} bar{}", self.name, self.bars, s(self.bars))),
            Some((from, Some(to))) if bar > to => Some(format!("lies after the loop of \"{}\" (#loop {from} {to}), which it never leaves", self.name)),
            _ => None,
        }
    }
}

impl Song {
    /// A song of nothing, 120 beats a minute and 4 to a bar (in place of
    /// one that does not read).
    pub fn empty() -> Song {
        parse_song("", true).expect("an empty song reads")
    }

    /// Whether it is a studio: a file of instruments without a channel,
    /// which a game lays its own channels over (`music:play`'s
    /// `channels`): no song a player hears alone.
    pub fn studio(&self) -> bool {
        self.channels.is_empty()
    }

    /// Its beats, for a game: by its file's name.
    pub fn timing(&self, file: &str) -> Timing {
        let name = std::path::Path::new(file).file_stem().map_or_else(|| file.to_string(), |s| s.to_string_lossy().into_owned());
        Timing { name, tempo: self.tempo, meter: self.meter, round: self.round, bars: self.bars, studio: None, letters: self.names.clone() }
    }

    /// The seconds the loop goes round from and to (`#loop`), if it has one.
    pub fn round_seconds(&self) -> Option<(f32, f32)> {
        self.round.map(|_| (self.span.0.max(0.0) as f32 / SAMPLE_RATE as f32, self.span.1 as f32 / SAMPLE_RATE as f32))
    }

    /// The samples a render without a length writes: the song once, or
    /// with `#loop` the beginning, the loop once and its first bars again
    /// (four, or the whole loop when shorter), so the seam lies in it.
    pub fn once(&self) -> u64 {
        match self.round {
            None => self.length,
            Some(_) => {
                let (from, to) = self.span;
                (to + (to - from).min(4.0 * bar_line(self.tempo, self.meter, 2))) as u64
            }
        }
    }

    /// Finds the recording each instrument of the wave `sample` names
    /// (its layers' too) among `samples`: a name not there is a mistake
    /// naming its line and the nearest name, and so is a stretch (`start`,
    /// `end`) past the recording's end.
    pub fn bind(&mut self, samples: &super::sample::Samples) -> anyhow::Result<()> {
        for ins in &mut self.instruments {
            for s in std::iter::once(&mut ins.sample).chain(ins.layers.iter_mut().map(|l| &mut l.sample)) {
                let Some(s) = s else { continue };
                let Some(rec) = samples.get(&s.name) else {
                    let names: Vec<&str> = samples.names().collect();
                    let hint = match super::words::nearest(&s.name, names.iter().copied()) {
                        Some(near) => format!(" (the nearest: \"{near}\")"),
                        None => " (the game has no samples: a WAV in samples/ is one, its name its file's)".to_string(),
                    };
                    anyhow::bail!("line {}: sample name={}: there is no {}/{}.wav{hint}", s.line, s.name, super::sample::FOLDER, s.name);
                };
                let length = rec.seconds();
                let past = [("start", (s.start >= length).then_some(s.start)), ("end", s.end.filter(|&end| end > length + 0.5 / SAMPLE_RATE as f32))];
                if let Some((word, at)) = past.into_iter().find_map(|(w, at)| Some((w, at?))) {
                    anyhow::bail!("line {}: sample {word}={at} lies past the end of {}, which lasts {length:.3} seconds", s.line, rec.file);
                }
                s.recording = Some(rec.clone());
            }
        }
        Ok(())
    }

    /// The instruments by name, with what each plays: its wave (or
    /// `voice`, `singer`, `sample`), how many layers it has and how many
    /// oscillators its line plays together.
    pub fn instrument_list(&self) -> Vec<(&str, &'static str, usize, u32)> {
        let wave = |i: &Instrument| match (i.voice, &i.sample) {
            (Some(Voicing::Formants), _) => "voice",
            (Some(Voicing::Clear), _) => super::clear::WAVE,
            (None, Some(_)) => "sample",
            (None, None) => i.wave.name(),
        };
        self.instrument_names.iter().zip(&self.instruments).filter(|(n, _)| !n.is_empty()).map(|(n, i)| (n.as_str(), wave(i), i.layers.len() + 1, i.unison)).collect()
    }

    /// How many notes (rests not counted) each channel plays.
    pub fn note_counts(&self) -> Vec<(char, usize)> {
        self.names.iter().zip(&self.channels).map(|(&name, notes)| (name, notes.iter().filter(|n| n.freq.is_some()).count())).collect()
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

//! The game's say over its song as heard: loudness and muffle, each gliding.

/// A value of the game's song as heard that the game moves over a time
/// (`music:volume`, `music:muffle`): from where it was to where it goes,
/// over so many of the game's ticks, so many of them gone; how many times
/// it was asked (a new ask starts the audio's own glide). The game keeps
/// it (in its snapshot, never in its hash: only heard).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    pub from: f32,
    pub to: f32,
    pub ticks: u32,
    pub done: u32,
    pub asked: u32,
}

impl Glide {
    /// Standing at `at`, never asked.
    pub const fn still(at: f32) -> Self {
        Self { from: at, to: at, ticks: 0, done: 0, asked: 0 }
    }

    /// Where it is now.
    pub fn now(&self) -> f32 {
        if self.done >= self.ticks { self.to } else { self.from + (self.to - self.from) * self.done as f32 / self.ticks as f32 }
    }

    /// The seconds of its glide still to go.
    pub fn left(&self) -> f32 {
        self.ticks.saturating_sub(self.done) as f32 * crate::sim::DT
    }

    /// A tick of the game on.
    pub fn step(&mut self) {
        self.done = (self.done + 1).min(self.ticks);
    }

    /// Asked to go to `to` over `seconds`, from where it is now.
    pub fn ask(&mut self, to: f32, seconds: f32) {
        let ticks = (seconds / crate::sim::DT).round().min(u32::MAX as f32) as u32;
        *self = Self { from: self.now(), to, ticks, done: 0, asked: self.asked.wrapping_add(1) };
    }

    /// Whether it still goes somewhere.
    pub fn gliding(&self) -> bool {
        self.done < self.ticks
    }

    /// What tells one ask from another for the audio: how many times it
    /// was asked, where to and over how long (a state brought back that
    /// asked as often but elsewhere is followed too).
    pub fn key(&self) -> (u32, u32, u32) {
        (self.asked, self.to.to_bits(), self.ticks)
    }
}

/// A channel's strip on the mixing desk as the game asked it (`music:mix`):
/// its fader (0..1) and mute, one glide between them (the fader, or 0
/// muted), and its pan (-1 left to 1 right, a balance: the far side turned
/// down, never a side louder than written).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    pub volume: f32,
    pub mute: bool,
    pub level: Glide,
    pub pan: Glide,
}

impl Default for Strip {
    fn default() -> Self {
        Self { volume: 1.0, mute: false, level: Glide::still(1.0), pan: Glide::still(0.0) }
    }
}

/// The channels a song has, one a letter: A to Z, then a to z.
pub const LETTERS: usize = 52;

/// The place of channel `letter` among the `LETTERS`.
pub fn letter_index(letter: char) -> Option<usize> {
    match letter {
        'A'..='Z' => Some(letter as usize - 'A' as usize),
        'a'..='z' => Some(26 + letter as usize - 'a' as usize),
        _ => None,
    }
}

/// How the game's song sounds as the game asked (`music:volume`, a part of
/// its loudness, 1 as written; `music:muffle`, 0 clear to 1 very dull;
/// `music:mix`, each channel's strip by its letter, shared, not copied, by
/// a snapshot).
#[derive(Clone, Debug, PartialEq)]
pub struct Mix {
    pub volume: Glide,
    pub muffle: Glide,
    pub strips: std::sync::Arc<std::collections::BTreeMap<char, Strip>>,
}

impl Default for Mix {
    fn default() -> Self {
        Self { volume: Glide::still(1.0), muffle: Glide::still(0.0), strips: Default::default() }
    }
}

impl Mix {
    /// A tick of the game on: every glide (the strips copied only while
    /// one of them goes somewhere).
    pub fn step(&mut self) {
        self.volume.step();
        self.muffle.step();
        if self.strips.values().any(|s| s.level.gliding() || s.pan.gliding()) {
            for s in std::sync::Arc::make_mut(&mut self.strips).values_mut() {
                s.level.step();
                s.pan.step();
            }
        }
    }

    /// Channel `letter`'s strip, as never set where the game set none.
    pub fn strip(&self, letter: char) -> Strip {
        self.strips.get(&letter).copied().unwrap_or_default()
    }
}

/// The shortest glides the audio makes, the loudness's (5 ms) and the
/// muffle's (20 ms: the filter's sound blends in over its first quarter):
/// a change asked "at once" does not click.
pub const SHORTEST: (f32, f32) = (0.005, 0.02);

/// The low-pass's cutoff for a muffle (0..1), on a musical curve: 300 Hz
/// times 60 to the power (1 - amount)^1.5: 18 kHz (open) at 0, about
/// 4.3 kHz at 0.25, 1.3 kHz at 0.5, 500 Hz at 0.75, 300 Hz at 1.
pub fn muffle_cutoff(amount: f32) -> f32 {
    300.0 * libm::powf(60.0, libm::powf(1.0 - amount.clamp(0.0, 1.0), 1.5))
}

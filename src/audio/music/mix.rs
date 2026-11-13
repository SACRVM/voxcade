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
}

/// How the game's song sounds as the game asked (`music:volume`, a part of
/// its loudness, 1 as written; `music:muffle`, 0 clear to 1 very dull).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub volume: Glide,
    pub muffle: Glide,
}

impl Default for Mix {
    fn default() -> Self {
        Self { volume: Glide::still(1.0), muffle: Glide::still(0.0) }
    }
}

/// The shortest glides the audio makes, the loudness's (5 ms) and the
/// muffle's (20 ms: the filter's sound blends in over its first quarter):
/// a change asked "at once" does not click.
pub(super) const SHORTEST: (f32, f32) = (0.005, 0.02);

/// The low-pass's cutoff for a muffle (0..1), on a musical curve: 300 Hz
/// times 60 to the power (1 - amount)^1.5: 18 kHz (open) at 0, about
/// 4.3 kHz at 0.25, 1.3 kHz at 0.5, 500 Hz at 0.75, 300 Hz at 1.
pub fn muffle_cutoff(amount: f32) -> f32 {
    300.0 * 60f32.powf((1.0 - amount.clamp(0.0, 1.0)).powf(1.5))
}

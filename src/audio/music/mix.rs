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

/// The words of an instrument a strip turns while a note sounds: levels and
/// depths the voice reads every sample (`player::Voice::sound`), so they
/// glide under it. In this order in `Strip::knobs`. Of a layered
/// instrument a strip turns the first layer's, but cutoff and resonance
/// the sum's where its `=` line states them.
pub const GLIDING: [&str; 7] = ["duty", "pwm", "vibrato", "sustain", "cutoff", "resonance", "duck"];
/// `GLIDING`'s cutoff, which glides in octaves (its glide holds log2 Hz).
pub const CUTOFF: usize = place(&GLIDING, "cutoff");

/// The place of `word` among `words`, worked out as the program is built
/// (a word not there does not build).
pub(super) const fn place(words: &[&str], word: &str) -> usize {
    let mut i = 0;
    while i < words.len() {
        let (a, b) = (words[i].as_bytes(), word.as_bytes());
        if a.len() == b.len() {
            let mut j = 0;
            while j < a.len() && a[j] == b[j] {
                j += 1;
            }
            if j == a.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("no such word")
}

/// The words of an instrument a strip turns from a channel's next note: a
/// time or a rate a note's shape is measured by (moved under it, the shape
/// would jump), or what the voice takes up as a note begins
/// (`player::Voice::take_up`). The note that sounds keeps its own. In this
/// order in `Strip::next`. Of a layered instrument (`+` lines) a strip
/// turns the first layer's, but drive the sum's where its `=` line states
/// a drive, as `GLIDING`'s cutoff and resonance. A sample's `root` (held
/// as Hz), `start` and `end` (`SAMPLE_KNOBS`) are among them: a break cut
/// up by the game, note by note.
pub const NEXT_NOTE: [&str; 23] = [
    "attack", "decay", "release", "freq", "sweep", "sweep_time", "pitch_start", "pitch_decay", "cutoff_end", "cutoff_time", "vibrato_speed",
    "vibrato_delay", "pwm_speed", "arp_speed", "lowpass", "drive", "crush_rate", "crush_bits", "sidechain", "sidechain_release", "root", "start",
    "end",
];

/// `NEXT_NOTE`'s words of the wave `sample` alone.
pub const SAMPLE_KNOBS: [&str; 3] = ["root", "start", "end"];

/// An instrument's words that are no knob, and why.
pub const NO_KNOB: [(&str, &str); 8] = [
    ("wave", "the wave is another sound, not a knob: set it in the instrument"),
    ("filter", "the filter's kind is a choice of the instrument, not a knob: its cutoff and resonance turn"),
    ("arp", "an arpeggio is a list of steps, not a knob: arp_speed turns"),
    ("consonants", "a voice's consonants are part of its singing, not a knob"),
    ("curve", "how the envelope falls is a choice of the instrument, not a knob: its decay and release turn"),
    ("name", "which recording a sample plays is a choice of the instrument, not a knob: its start, end and root turn"),
    ("loop", "whether a sample goes round is a choice of the instrument, not a knob: its start and end turn"),
    ("reverse", "which way a sample plays is a choice of the instrument, not a knob: its start, end and root turn"),
];

/// One of a strip's `GLIDING` words: the value asked (None: the
/// instrument's), the glide to it, and how far it holds over the
/// instrument's own (1 set, gliding to 0 when given back), so turning it
/// on or giving it back never steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knob {
    pub set: Option<f32>,
    pub value: Glide,
    pub on: Glide,
}

impl Knob {
    const OFF: Knob = Knob { set: None, value: Glide::still(0.0), on: Glide::still(0.0) };

    /// Turned to `to` (as the voice reads it: log2 Hz for the cutoff) over
    /// `seconds`, or given back to the instrument (None).
    fn turn(&mut self, asked: Option<f32>, to: f32, seconds: f32) {
        self.set = asked;
        match asked {
            None => self.on.ask(0.0, seconds),
            // From the instrument's own: the value there at once, the
            // knob coming in over it.
            Some(_) if self.on.now() == 0.0 => {
                self.value = Glide { from: to, to, ticks: 0, done: 0, asked: self.value.asked.wrapping_add(1) };
                self.on.ask(1.0, seconds);
            }
            Some(_) => {
                self.value.ask(to, seconds);
                if self.on.to != 1.0 {
                    self.on.ask(1.0, seconds);
                }
            }
        }
    }
}

/// A channel's strip on the mixing desk as the game asked it (`music:mix`):
/// its fader (0..1) and mute, one glide between them (the fader, or 0
/// muted), its pan (-1 left to 1 right, a balance: the far side turned
/// down, never a side louder than written), and its instrument's knobs
/// over what its notes' instrument says: `GLIDING`'s gliding, `NEXT_NOTE`'s
/// as asked (None: the instrument's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    pub volume: f32,
    pub mute: bool,
    pub level: Glide,
    pub pan: Glide,
    pub knobs: [Knob; GLIDING.len()],
    pub next: [Option<f32>; NEXT_NOTE.len()],
}

impl Default for Strip {
    fn default() -> Self {
        Self { volume: 1.0, mute: false, level: Glide::still(1.0), pan: Glide::still(0.0), knobs: [Knob::OFF; GLIDING.len()], next: [None; NEXT_NOTE.len()] }
    }
}

impl Strip {
    /// An instrument's word turned to `value` (None: given back to the
    /// instrument), gliding over `seconds` where it glides; false where
    /// it is no word a strip turns.
    pub fn turn(&mut self, word: &str, value: Option<f32>, seconds: f32) -> bool {
        if let Some(i) = GLIDING.iter().position(|w| *w == word) {
            let to = value.map_or(0.0, |v| if i == CUTOFF { libm::log2f(v) } else { v });
            self.knobs[i].turn(value, to, seconds);
        } else if let Some(i) = NEXT_NOTE.iter().position(|w| *w == word) {
            self.next[i] = value;
        } else {
            return false;
        }
        true
    }

    /// The instrument's words it turns, as asked, in the tables' order.
    pub fn turned(&self) -> impl Iterator<Item = (&'static str, f32)> + '_ {
        let gliding = GLIDING.iter().zip(&self.knobs).filter_map(|(w, k)| Some((*w, k.set?)));
        gliding.chain(NEXT_NOTE.iter().zip(&self.next).filter_map(|(w, v)| Some((*w, (*v)?))))
    }

    /// Whether any of its glides still goes somewhere.
    fn gliding(&self) -> bool {
        self.level.gliding() || self.pan.gliding() || self.knobs.iter().any(|k| k.value.gliding() || k.on.gliding())
    }

    /// A tick of the game on.
    fn step(&mut self) {
        self.level.step();
        self.pan.step();
        for k in &mut self.knobs {
            k.value.step();
            k.on.step();
        }
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
        if self.strips.values().any(Strip::gliding) {
            for s in std::sync::Arc::make_mut(&mut self.strips).values_mut() {
                s.step();
            }
        }
    }

    /// Channel `letter`'s strip, as never set where the game set none.
    pub fn strip(&self, letter: char) -> Strip {
        self.strips.get(&letter).copied().unwrap_or_default()
    }
}

/// The shortest glides the audio makes, the loudness's (5 ms, a knob's
/// too) and the muffle's (20 ms: the filter's sound blends in over its
/// first quarter; a cutoff's too): a change asked "at once" does not click.
pub const SHORTEST: (f32, f32) = (0.005, 0.02);

/// The low-pass's cutoff for a muffle (0..1), on a musical curve: 300 Hz
/// times 60 to the power (1 - amount)^1.5: 18 kHz (open) at 0, about
/// 4.3 kHz at 0.25, 1.3 kHz at 0.5, 500 Hz at 0.75, 300 Hz at 1.
pub fn muffle_cutoff(amount: f32) -> f32 {
    300.0 * libm::powf(60.0, libm::powf(1.0 - amount.clamp(0.0, 1.0), 1.5))
}

//! The console in the window (`docs/game-model.md`, "Scenarios and the
//! console"), as Quake had it on the key left of 1: it slides in at the
//! bottom, pushing the game up, a few lines high, and out again. What is
//! being typed, the lines run before (up and down bring them back), and
//! what each answered. Running a line is the session's (`Session::eval`);
//! this only keeps the text and how far it is in.

/// How a line of what the console shows reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// A line as it was typed, run.
    Typed,
    /// What it answered.
    Answer,
    /// A mistake.
    Mistake,
}

/// The lines it keeps, the newest last.
const KEPT: usize = 200;

/// Seconds it takes to slide in or out.
const SLIDE: f32 = 0.15;

#[derive(Debug, Default)]
pub struct Prompt {
    pub open: bool,
    /// How far it is in: 0 out of sight, 1 all in.
    pub part: f32,
    /// The line being typed.
    pub line: String,
    /// What was run and answered, the newest last.
    pub shown: Vec<(String, Tone)>,
    /// The lines run, the newest last, and which one up and down brought
    /// back (none: the one being typed).
    history: Vec<String>,
    back: Option<usize>,
    /// The key that opened it may still send its sign as text (the key
    /// left of 1 is a dead key on some keyboards): taken out once.
    swallow: bool,
}

impl Prompt {
    pub fn open(&mut self) {
        (self.open, self.swallow, self.back) = (true, true, None);
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// `dt` seconds of sliding towards in (open) or out (closed).
    pub fn slide(&mut self, dt: f32) {
        let to = if self.open { 1.0 } else { 0.0 };
        let step = dt / SLIDE;
        self.part = if self.part < to { (self.part + step).min(to) } else { (self.part - step).max(to) };
    }

    /// Whether any of it is in sight (open, or still sliding out).
    pub fn showing(&self) -> bool {
        self.open || self.part > 0.0
    }

    /// The events of one frame are over: the opening key's sign came by
    /// now, or will not.
    pub fn settle(&mut self) {
        self.swallow = false;
    }

    /// Text typed (or pasted: one line of it).
    pub fn type_text(&mut self, text: &str) {
        let mut text = text.lines().next().unwrap_or_default();
        if std::mem::take(&mut self.swallow) {
            text = text.trim_start_matches(['`', '^', '~']);
        }
        self.line.push_str(text);
        self.back = None;
    }

    pub fn backspace(&mut self) {
        self.line.pop();
    }

    /// Up (-1) or down (+1) through the lines run before.
    pub fn recall(&mut self, way: i32) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        self.back = match (self.back, way < 0) {
            (None, true) => Some(last),
            (None, false) => None,
            (Some(0), true) => Some(0),
            (Some(n), true) => Some(n - 1),
            (Some(n), false) if n < last => Some(n + 1),
            (Some(_), false) => None,
        };
        self.line = self.back.map(|n| self.history[n].clone()).unwrap_or_default();
    }

    /// The line typed, to be run (empty: nothing); it goes into the
    /// history and what is shown.
    pub fn take(&mut self) -> Option<String> {
        let line = std::mem::take(&mut self.line);
        self.back = None;
        if line.trim().is_empty() {
            return None;
        }
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }
        self.show(&format!("> {line}"), Tone::Typed);
        Some(line)
    }

    /// What a line run answered, or the mistake it made.
    pub fn answer(&mut self, said: anyhow::Result<String>) {
        match said {
            Ok(text) => {
                for line in text.lines() {
                    self.show(line, Tone::Answer);
                }
            }
            Err(e) => {
                // Lua's own frames say nothing about the game's code.
                for line in format!("{e:#}").lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("[C]:")) {
                    self.show(line, Tone::Mistake);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.shown.clear();
    }

    fn show(&mut self, text: &str, tone: Tone) {
        self.shown.push((text.replace('\t', "  "), tone));
        let over = self.shown.len().saturating_sub(KEPT);
        self.shown.drain(..over);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Typed, run, recalled: the opening key's sign is taken out once, the
    /// history walks up and back down to the line being typed.
    #[test]
    fn the_prompt_keeps_what_was_typed_and_run() {
        let mut p = Prompt::default();
        p.open();
        p.type_text("^");
        p.type_text("player.state.coins");
        assert_eq!(p.take().as_deref(), Some("player.state.coins"));
        p.answer(Ok("40".into()));
        p.type_text("nothing.here");
        p.take();
        p.answer(Err(anyhow::anyhow!("console:1: attempt to index a nil value\nstack traceback:\n\t[C]: in metamethod 'index'")));
        let tones: Vec<Tone> = p.shown.iter().map(|(_, t)| *t).collect();
        assert_eq!(tones, [Tone::Typed, Tone::Answer, Tone::Typed, Tone::Mistake, Tone::Mistake]);
        p.type_text("half");
        p.recall(-1);
        assert_eq!(p.line, "nothing.here");
        p.recall(-1);
        p.recall(-1);
        assert_eq!(p.line, "player.state.coins", "it stops at the oldest");
        p.recall(1);
        p.recall(1);
        assert_eq!(p.line, "", "past the newest: an empty line");
        assert_eq!(p.take(), None, "an empty line runs nothing");
        p.slide(0.1);
        assert!(p.part > 0.6 && p.part < 0.7, "sliding in: {}", p.part);
        p.slide(1.0);
        assert_eq!(p.part, 1.0);
        p.close();
        p.slide(0.1);
        assert!(p.showing(), "still sliding out");
        p.slide(1.0);
        assert!(!p.showing());
    }
}

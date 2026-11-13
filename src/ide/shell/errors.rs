//! What is wrong, said: the red line over the main area (the game
//! stopped, a file that would not load, a game that would not start),
//! OUTPUT's lines from the game, a mistake in red with COPY, and the
//! errors as the agent reads them.

use super::*;

impl Shell {
    /// What the game said since last asked: what stops it (once each), a
    /// file that would not load, what the script printed.
    pub fn game_said(&mut self, trouble: Option<String>, said: Vec<(Kind, String)>) {
        if trouble != self.below.reported {
            if let Some(t) = &trouble {
                let at = crate::ide::play::line_of(t);
                // Whole, its traceback too (Lua's own frames left out): the
                // red line over the game says only its first line.
                let whole: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("[C]:")).collect();
                self.report(Kind::Error, format!("THE GAME STOPPED: {}", whole.join("\n")), at);
            }
            self.below.reported = trouble;
        }
        for (kind, text) in said {
            let at = (kind == Kind::Error).then(|| crate::ide::play::line_of(&text)).flatten();
            self.report(kind, text, at);
        }
    }

    /// A line run in the game being played, and what it answered, into
    /// OUTPUT (the user's, or the agent's `eval`).
    pub fn evaluated(&mut self, line: &str, said: &anyhow::Result<String>) {
        let shown = self.below.output_shown;
        self.below.output.add(Kind::Note, format!("> {line}"), None, shown);
        match said {
            Ok(text) => {
                for answer in text.lines() {
                    self.below.output.add(Kind::Print, answer, None, shown);
                }
            }
            Err(e) => {
                let text = format!("{e:#}");
                let at = crate::ide::play::line_of(&text);
                let first = text.lines().next().unwrap_or("").to_string();
                self.below.output.add(Kind::Error, first, at, shown);
            }
        }
    }

    /// What stops the game or a file of it, as one line in red over
    /// whatever the main area shows: its first line, TO THE LINE where it
    /// names one of the game's files, SHOW (OUTPUT, the console unfolded)
    /// and COPY. It goes when the files are mended. None: all is well.
    pub(super) fn game_error(&self) -> Option<(String, String)> {
        let first = |t: &str| t.lines().next().unwrap_or("").to_string();
        let stage = self.stage.as_ref();
        if let Some(t) = stage.and_then(|s| s.trouble.as_deref()) {
            Some((format!("THE GAME STOPPED: {}", first(t)), t.to_string()))
        } else if let Some(t) = stage.and_then(|s| s.unloaded.as_deref()) {
            Some((first(t), t.to_string()))
        } else {
            self.start_failed.as_deref().map(|t| (format!("THE GAME WOULD NOT START: {}", first(t)), t.to_string()))
        }
    }

    /// The game would not start (PLAY, or the game as the files are now):
    /// said in the red line and in OUTPUT, until it starts or the files are
    /// mended.
    pub fn wont_start(&mut self, why: String) {
        let at = crate::ide::play::line_of(&why);
        self.report(Kind::Error, format!("THE GAME WOULD NOT START: {}", why.lines().next().unwrap_or("")), at);
        self.start_failed = Some(why);
    }

    /// The game starts, or loads again: the red line for it goes.
    pub fn start_mended(&mut self) {
        self.start_failed = None;
    }

    /// Whether the game would not start, as far as is known.
    pub fn start_failed(&self) -> bool {
        self.start_failed.is_some()
    }

    /// SHOW: OUTPUT in the console below, which unfolds (and is there
    /// again if the settings had it off).
    pub(super) fn show_output(&mut self) {
        self.forms.prefs.console = true;
        self.below.output_shown = true;
        self.memory.console_folded = false;
        let _ = self.save();
    }

    /// What stops the game and what Lua cannot read in the open file, as
    /// text (`errors`).
    pub fn errors(&self) -> String {
        let mut out = Vec::new();
        if let Some(trouble) = self.stage.as_ref().and_then(|s| s.trouble.as_ref()) {
            out.push(format!("The game is stopped by:\n{trouble}"));
        }
        if let (Some(Shown::Code(buffer)), Some(path)) = (&self.shown, &self.memory.open)
            && let Some((line, why)) = &buffer.code.error
        {
            out.push(if *line > 0 { format!("{path}:{line}: {why}") } else { format!("{path}: {why}") });
        }
        let recent: Vec<String> = self.below.output.errors(5).iter().map(|l| l.text.clone()).collect();
        if !recent.is_empty() {
            out.push(format!("The last errors in OUTPUT:\n{}", recent.join("\n")));
        }
        if out.is_empty() { "No errors.".into() } else { out.join("\n\n") }
    }
}

/// COPY, small and quiet: puts `text` on the clipboard.
pub(super) fn copy_button(ui: &mut egui::Ui, text: &str) {
    let button = ui.small_button("COPY").on_hover_text("Copies the message, to paste to someone who can help.");
    #[cfg(test)]
    COPY_BUTTONS.with(|b| b.borrow_mut().push(button.rect));
    if button.clicked() {
        ui.ctx().copy_text(text.to_string());
    }
}

// Where the COPY buttons were drawn since last taken (for the tests' mouse).
#[cfg(test)]
thread_local! {
    pub(super) static COPY_BUTTONS: std::cell::RefCell<Vec<egui::Rect>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A mistake in red with COPY at its right end: `copy` is the whole
/// message as it stands alone (the file's name in front).
pub(super) fn error_row(ui: &mut egui::Ui, shown: &str, copy: &str) {
    // The message takes the room left of COPY and breaks into lines
    // there: a long one cannot push the button out of the window.
    ui.horizontal_top(|ui| {
        let room = (ui.available_width() - 96.0).max(120.0);
        ui.allocate_ui_with_layout(egui::vec2(room, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(RichText::new(shown).color(ERROR)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(16.0);
            copy_button(ui, copy);
        });
    });
}

/// The red line of what is wrong with the game: the message in the room
/// left of the buttons, breaking into lines there (a long one cannot push
/// them out of the window), TO THE LINE if `line`, SHOW and COPY at the
/// right end. Whether TO THE LINE and SHOW were pressed.
pub(super) fn game_error_row(ui: &mut egui::Ui, shown: &str, copy: &str, line: bool) -> (bool, bool) {
    let (mut go, mut show) = (false, false);
    ui.horizontal_top(|ui| {
        let room = (ui.available_width() - if line { 290.0 } else { 170.0 }).max(120.0);
        ui.allocate_ui_with_layout(egui::vec2(room, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(RichText::new(shown).color(ERROR)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(16.0);
            copy_button(ui, copy);
            show = ui.small_button("SHOW").on_hover_text("Shows OUTPUT below, where the whole message is.").clicked();
            if line {
                go = ui.small_button("TO THE LINE").on_hover_text("Opens the file at the line the message names.").clicked();
            }
        });
    });
    (go, show)
}

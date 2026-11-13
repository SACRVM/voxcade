//! OUTPUT, beside the console below the main area: what the IDE and the
//! game report, newest last. BUILD's result, a file that would not load,
//! the game stopped by a script's error, what the editors could not do,
//! runs kept, and what the script printed. An error that names a line
//! goes there on a click. Each game keeps the last lines in the config
//! folder (`ide/output/<game>.toml`, a folder of its own), so the next time
//! it is opened they are there, under a line that says so.

use std::path::{Path, PathBuf};

use egui::{RichText, TextStyle};
use serde::{Deserialize, Serialize};

use crate::gui::{ERROR, QUIET, TEXT};

/// How many lines a game keeps.
const KEEP: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// What the IDE did (built, kept a run).
    Note,
    Error,
    /// What the script printed.
    Print,
    /// The game opened again: what is above came before.
    Opened,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub kind: Kind,
    pub text: String,
    /// The file and line it names, to go to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<(String, usize)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Kept {
    lines: Vec<Line>,
}

pub struct Output {
    lines: Vec<Line>,
    /// Where they are kept (None: nowhere, a test's or a shot's), and
    /// whether there is something new to write down.
    path: Option<PathBuf>,
    dirty: bool,
    /// Errors added while OUTPUT was not shown.
    pub unseen: usize,
}

impl Output {
    /// Where the game in `game_dir` keeps its lines.
    pub fn path_for(game_dir: &Path) -> PathBuf {
        crate::settings::kept_for(game_dir, crate::settings::Kept::IdeOutput)
    }

    /// The lines kept at `path`, the game opened again under them (None:
    /// none kept, kept nowhere).
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut lines = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| toml::from_str::<Kept>(&t).ok()).map(|k| k.lines).unwrap_or_default();
        if !lines.is_empty() && lines.last().is_some_and(|l| l.kind != Kind::Opened) {
            lines.push(Line { kind: Kind::Opened, text: "OPENED AGAIN".into(), at: None });
        }
        Self { lines, path, dirty: false, unseen: 0 }
    }

    /// Adds a line (`shown`: OUTPUT is in view, else an error counts as
    /// unseen).
    pub fn add(&mut self, kind: Kind, text: impl Into<String>, at: Option<(String, usize)>, shown: bool) {
        let text = text.into();
        // The same error again (a game stopped twice the same way) once.
        if kind == Kind::Error && self.lines.last().is_some_and(|l| l.kind == kind && l.text == text) {
            return;
        }
        if kind == Kind::Error && !shown {
            self.unseen += 1;
        }
        self.lines.push(Line { kind, text, at });
        let over = self.lines.len().saturating_sub(KEEP);
        self.lines.drain(..over);
        self.dirty = true;
    }

    /// Writes the lines down if there are new ones.
    pub fn save(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.path.as_ref().filter(|_| self.dirty) else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::blocks::save_at_once(path, toml::to_string(&Kept { lines: self.lines.clone() })?.as_bytes())?;
        self.dirty = false;
        Ok(())
    }

    /// Every line as plain text, in order (COPY ALL).
    pub fn text(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }

    /// Forgets every line (CLEAR).
    pub fn clear(&mut self) {
        self.lines.clear();
        self.unseen = 0;
        self.dirty = true;
    }

    /// The last errors, newest last, for the agent (`errors`).
    pub fn errors(&self, count: usize) -> Vec<&Line> {
        let errors: Vec<&Line> = self.lines.iter().filter(|l| l.kind == Kind::Error).collect();
        errors[errors.len().saturating_sub(count)..].to_vec()
    }

    /// The lines in `ui`, scrolled to the newest; the file and line of an
    /// error clicked.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<(String, usize)> {
        self.unseen = 0;
        let mut go = None;
        egui::ScrollArea::vertical().id_salt("output").auto_shrink([false, false]).stick_to_bottom(true).content_margin(egui::Margin { left: 20, right: 12, top: 6, bottom: 10 }).show(ui, |ui| {
            for line in &self.lines {
                match line.kind {
                    Kind::Opened => {
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("—— {} ——", line.text)).small().color(QUIET));
                        ui.add_space(4.0);
                    }
                    kind => {
                        let colour = match kind {
                            Kind::Error => ERROR,
                            Kind::Print => TEXT,
                            _ => QUIET,
                        };
                        let text = RichText::new(&line.text).text_style(TextStyle::Monospace).color(colour);
                        match &line.at {
                            Some(at) => {
                                if ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(format!("To {} line {}", at.0, at.1)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    go = Some(at.clone());
                                }
                            }
                            None => {
                                ui.label(text);
                            }
                        }
                    }
                }
            }
        });
        go
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines come back the next time, under OPENED AGAIN; the same error
    /// twice in a row is one; errors not seen are counted; the newest
    /// `KEEP` stay.
    #[test]
    fn the_output_is_kept_for_the_next_time() {
        let scratch = crate::scratch::Scratch::new("output");
        let path = scratch.join("game.toml");
        let mut out = Output::open(Some(path.clone()));
        out.add(Kind::Note, "BUILT x.zip", None, false);
        out.add(Kind::Error, "main.lua:3: boom", Some(("main.lua".into(), 3)), false);
        out.add(Kind::Error, "main.lua:3: boom", Some(("main.lua".into(), 3)), false);
        assert_eq!((out.lines.len(), out.unseen), (2, 1));
        out.save().unwrap();
        let again = Output::open(Some(path.clone()));
        assert_eq!(again.lines.len(), 3);
        assert_eq!(again.lines[2].kind, Kind::Opened);
        assert_eq!(again.errors(5)[0].at, Some(("main.lua".into(), 3)));
        let mut many = Output::open(None);
        for i in 0..KEEP + 10 {
            many.add(Kind::Print, i.to_string(), None, true);
        }
        assert_eq!((many.lines.len(), many.lines[0].text.as_str()), (KEEP, "10"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

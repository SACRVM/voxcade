//! Below the main area: the console (folded or as high as dragged), or
//! OUTPUT with the line that runs Lua in the game being played.

use super::*;

impl Shell {
    /// The console below the main area, or OUTPUT in its place.
    pub(super) fn below(&mut self, ui: &mut egui::Ui) {
        // The console below the main area, folded to its heading or as
        // high as dragged (and kept so); or none (CONSOLE). A game's first
        // time it is folded: the bar only, the room is the main area's.
        let folded = self.memory.console_folded;
        if self.forms.prefs.console {
            // At first almost half the window: room for Claude Code's
            // screens.
            let tall = self.memory.console_height.unwrap_or(0.45 * ui.available_height());
            let panel = if folded { egui::Panel::bottom("console folded") } else { egui::Panel::bottom("console").resizable(true).default_size(tall).min_size(120.0) };
            let shown = panel.frame(egui::Frame::new().fill(terminal::BACKGROUND)).show(ui, |ui| {
                let pressed = self.console.heading(ui, &mut self.memory.console_folded, self.forms.prefs.agent, self.forms.prefs.claude.connect, &mut self.below.output_shown, self.below.output.unseen);
                if pressed.agent {
                    self.below.output_shown = false;
                    self.start_agent(false, true);
                }
                if pressed.clear {
                    self.below.output.clear();
                }
                if pressed.copy_all {
                    ui.ctx().copy_text(self.below.output.text());
                }
                if !folded {
                    if self.below.output_shown {
                        if self.stage.is_some() {
                            self.lua_line(ui);
                        }
                        // An error clicked: its file, at its line.
                        if let Some((file, line)) = self.below.output.ui(ui).filter(|(f, _)| self.files.has(f)) {
                            self.hide_game();
                            self.help = false;
                            self.open(&file);
                            self.opening.goto = Some((line, true));
                        }
                    } else if self.agent.no_agent {
                        if agent_missing(ui, self.forms.prefs.agent) {
                            self.start_agent(false, true);
                        }
                    } else {
                        self.console.body(ui);
                        if let Some((file, line)) = self.console.take_opened() {
                            self.open_from_console(&file, line);
                        }
                    }
                }
            });
            // The height the user dragged it to, kept once the mouse is let go.
            if !folded && !self.memory.console_folded {
                let height = shown.response.rect.height();
                if self.below.console_seen.is_some_and(|h| (h - height).abs() > 0.5) && ui.input(|i| i.pointer.any_down()) {
                    self.below.console_dragged = true;
                }
                self.below.console_seen = Some(height);
                if self.below.console_dragged && !ui.input(|i| i.pointer.any_down()) {
                    self.below.console_dragged = false;
                    self.memory.console_height = Some(height);
                    let _ = self.save();
                }
            } else {
                (self.below.console_seen, self.below.console_dragged) = (None, false);
            }
            if self.memory.console_folded != folded {
                let _ = self.save();
            }
        }
    }

    /// The line under OUTPUT while the game plays: Lua run in it between
    /// two ticks (up and down bring earlier lines back), answered in OUTPUT.
    pub(super) fn lua_line(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("lua line").frame(egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 12, top: 4, bottom: 8 })).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(">").text_style(TextStyle::Monospace).color(QUIET));
                let edit = ui.add(egui::TextEdit::singleline(&mut self.below.lua.line).font(TextStyle::Monospace).desired_width(f32::INFINITY).hint_text("player.state.score = 5000"));
                if edit.has_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        self.below.lua.recall(-1);
                    } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        self.below.lua.recall(1);
                    }
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(line) = self.below.lua.take() {
                        self.asks.push(Ask::Eval(line));
                    }
                    edit.request_focus();
                }
            });
        });
    }
}

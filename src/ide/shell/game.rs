//! The game played in the main area: PLAY, AGAIN, a scenario played, STOP,
//! its picture with the keys under it, and the file shown in its place.

use super::*;

impl Shell {
    /// Plays the game: shows it again, or starts it (the text being
    /// changed saved first).
    pub fn play(&mut self) {
        self.hush();
        self.bar.notice = None;
        self.help = false;
        match &mut self.stage {
            Some(stage) => stage.shown = true,
            None => {
                self.flush();
                self.asks.push(Ask::Play(None));
            }
        }
    }

    /// AGAIN: the game shown, begun anew from the start (as F5 does).
    pub fn again(&mut self) {
        self.hush();
        (self.bar.notice, self.help) = (None, false);
        if let Some(stage) = &mut self.stage {
            stage.shown = true;
            self.asks.push(Ask::Again);
        }
    }

    /// PLAY SCENARIO over a scenario's file: the game played afresh, begun
    /// from it (the text being changed saved first).
    pub fn play_scenario(&mut self, name: &str) {
        self.hush();
        (self.bar.notice, self.help) = (None, false);
        self.flush();
        if self.stage.is_some() {
            self.asks.push(Ask::Stop);
        }
        self.asks.push(Ask::Play(Some(name.to_string())));
    }

    /// Ends the game.
    pub fn stop(&mut self) {
        if self.stage.is_some() {
            self.asks.push(Ask::Stop);
        }
    }

    /// The main area playing the game: its picture filling it (what stops
    /// it is said above, by `main`), the keys its while it has the focus.
    pub(super) fn game(&mut self, ui: &mut egui::Ui) {
        // The game's keys on a line under it (wrapping, never wider than
        // the room), the picture takes what is left.
        let keys = self.stage.as_ref().map_or(&[][..], |s| s.keys.as_slice());
        if !keys.is_empty() {
            egui::Panel::bottom("keys line").frame(egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 12, top: 6, bottom: 8 })).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, (what, key)) in keys.iter().enumerate() {
                        if i > 0 {
                            ui.label(RichText::new("·").color(QUIET));
                        }
                        ui.label(RichText::new(what).color(QUIET));
                        ui.label(RichText::new(key).color(TEXT));
                    }
                });
            });
        }
        let Some(stage) = &mut self.stage else { return };
        let room = egui::vec2(ui.available_width() - 20.0, ui.available_height() - 14.0).max(egui::vec2(1.0, 1.0));
        let (rect, response) = ui.allocate_exact_size(room, egui::Sense::click());
        let pixels = ui.ctx().pixels_per_point();
        stage.wanted = Some([(rect.width() * pixels).round() as u32, (rect.height() * pixels).round() as u32]);
        let response = ui.interact(rect, game_id(), egui::Sense::click()).union(response);
        if response.clicked() {
            response.request_focus();
        }
        if response.has_focus() {
            ui.memory_mut(|m| m.set_focus_lock_filter(game_id(), egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true }));
        }
        if let Some(id) = stage.picture {
            ui.painter().image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }

    /// Whether the game has the keys: it shows and nothing else has them
    /// (a click in the console takes them, one on the game gives them
    /// back).
    pub fn keys_to_game(&self, ctx: &egui::Context) -> bool {
        self.stage.as_ref().is_some_and(|s| s.shown) && ctx.memory(|m| m.focused()).is_none_or(|f| f == game_id())
    }

    /// Back to the file from the game (Escape): the game holds still, the
    /// text takes the keys where its cursor stood.
    pub fn back_to_file(&mut self) {
        self.hide_game();
        self.opening.focus_code = true;
    }

    /// The file shown instead of the game, which holds still.
    pub fn hide_game(&mut self) {
        if let Some(stage) = &mut self.stage {
            stage.shown = false;
        }
    }
}

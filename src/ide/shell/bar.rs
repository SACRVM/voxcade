//! The bar along the top: the game's name, OPEN and NEW, PLAY (or BACK
//! TO THE GAME), AGAIN and STOP, BUILD, SEARCH, HELP and the gear; what
//! BUILD made, a file just deleted with UNDO, the game's keys.

use super::*;

impl Shell {
    /// The bar along the top, and what its buttons asked for.
    pub(super) fn bar(&mut self, ui: &mut egui::Ui) {
        let (mut undo, mut forget_gone) = (false, false);
        let mut show_zip: Option<PathBuf> = None;
        egui::Panel::top("bar").frame(egui::Frame::new().fill(GROUND).inner_margin(egui::Margin::symmetric(16, 10))).show(ui, |ui| {
            // The name centred on the buttons' height.
            ui.spacing_mut().interact_size.y = ui.text_style_height(&TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
            ui.horizontal(|ui| {
                ui.label(RichText::new(self.title.to_uppercase()).heading().color(ACCENT));
                ui.add_space(16.0);
                // Another game: OPEN, one opened last or another folder (the
                // system's dialog at once while there are none); NEW, one
                // begun from a template.
                let open = ui.button("OPEN").on_hover_text("Another game: one opened last, or a folder of your own.");
                // (The games opened last looked for, on the disk, only while
                // OPEN is pressed or its list shown.)
                let asked = open.clicked() || egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&open));
                if asked && self.forms.prefs.recent_games(Some(&self.dir)).is_empty() {
                    if open.clicked() {
                        self.asks.push(Ask::Project(Wish::Browse));
                    }
                } else if asked {
                    egui::Popup::from_toggle_button_response(&open)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .align(egui::RectAlign::BOTTOM_START)
                        .gap(6.0)
                        .width(380.0)
                        .frame(egui::Frame::popup(ui.style()).fill(PANEL).inner_margin(egui::Margin::same(14)))
                        .show(|ui| {
                            if let Some(dir) = projects::recent(ui, &self.forms.prefs, Some(&self.dir)) {
                                self.leave_for(dir);
                                ui.close();
                            }
                            ui.add_space(8.0);
                            if ui.button("OTHER FOLDER…").on_hover_text("A game's folder (the one with its conf.lua), in the system's dialog.").clicked() {
                                self.asks.push(Ask::Project(Wish::Browse));
                                ui.close();
                            }
                        });
                }
                if ui.button("NEW").on_hover_text("Begins a new game from a template, in the projects folder (settings).").clicked() {
                    self.forms.new_game = Some(NewGame::new(&self.forms.prefs));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The settings, apart from the game's buttons.
                    let gear = ui.button(RichText::new("⚙").size(18.0)).on_hover_text("Settings: the console, the agent in it and how it starts.");
                    egui::Popup::from_toggle_button_response(&gear)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .align(egui::RectAlign::BOTTOM_END)
                        .gap(6.0)
                        .frame(egui::Frame::popup(ui.style()).fill(PANEL).inner_margin(egui::Margin::same(14)))
                        .show(|ui| self.settings(ui));
                    // HELP in the main area, or the file again.
                    if ui.add(egui::Button::new("HELP").selected(self.help)).on_hover_text("The Lua API by example, to copy into your game.").clicked() {
                        self.help = !self.help;
                        self.hide_game();
                    }
                    if ui.add(egui::Button::new("SEARCH").selected(self.search.shown)).on_hover_text("Finds a word in every text file of the game · Ctrl+Shift+F").clicked() {
                        if self.search.shown {
                            self.search.shown = false;
                        } else {
                            let selected = self.selection(ui.ctx()).0;
                            self.search.open(selected.as_deref());
                        }
                    }
                    ui.add_space(16.0);
                    if ui.button("BUILD").on_hover_text("Packs the game and the program into one zip for this system, to pass on (saved first).").clicked() {
                        self.build();
                    }
                    // PLAY starts the game (saving first) or shows it again,
                    // STOP ends it.
                    // A game held behind the file (after Escape) is gone back
                    // to with BACK TO THE GAME; AGAIN begins it anew.
                    let shown = self.stage.as_ref().is_some_and(|s| s.shown);
                    let held = self.stage.is_some() && !shown;
                    if self.stage.is_some() && ui.button("STOP").on_hover_text("Ends the game.").clicked() {
                        self.asks.push(Ask::Stop);
                    }
                    if self.stage.is_some() && ui.button("AGAIN").on_hover_text("Begins the game again from the start, as F5 does (from its scenario, if it was played from one).").clicked() {
                        self.again();
                        ui.memory_mut(|m| m.request_focus(game_id()));
                    }
                    let (word, tip) = if held { ("BACK TO THE GAME", "The game waits behind the file, as you left it: shows it again (Escape comes back to the file).") } else { ("PLAY", "Plays the game here, straight in (saved first).") };
                    if !shown && ui.button(word).on_hover_text(tip).clicked() {
                        self.play();
                        ui.memory_mut(|m| m.request_focus(game_id()));
                    }
                    if let Some(until) = self.bar.notice_until {
                        match until.checked_duration_since(std::time::Instant::now()) {
                            Some(left) => ui.ctx().request_repaint_after(left),
                            None => (self.bar.notice, self.bar.notice_until) = (None, None),
                        }
                    }
                    // A file just deleted: what was done, and UNDO for a few seconds.
                    if let Some(gone) = &self.managing.gone {
                        match gone.until.checked_duration_since(std::time::Instant::now()) {
                            Some(left) => {
                                ui.ctx().request_repaint_after(left);
                                if gone.can_undo() {
                                    undo |= ui.button("UNDO").on_hover_text("Puts the file back as it was.").clicked();
                                }
                                ui.add(egui::Label::new(RichText::new(&gone.said).small().color(QUIET)).truncate()).on_hover_text(&gone.said);
                            }
                            None => forget_gone = true,
                        }
                    } else if let Some(notice) = &self.bar.notice {
                        // (Truncated, with the whole in its hover text: a long
                        // path never pushes the buttons about.)
                        match notice {
                            Notice::Said(text) => {
                                ui.add(egui::Label::new(RichText::new(text).small().color(QUIET)).truncate()).on_hover_text(text);
                            }
                            Notice::Building => {
                                ui.label(RichText::new("BUILDING…").small().color(ACCENT));
                            }
                            Notice::Built(zip, said) => {
                                if ui.button("SHOW IN FOLDER").on_hover_text("Opens the folder the zip lies in.").clicked() {
                                    show_zip = Some(zip.clone());
                                }
                                ui.add(egui::Label::new(RichText::new(said).small().color(QUIET)).truncate()).on_hover_text(said);
                            }
                            Notice::Failed(said) => {
                                copy_button(ui, said);
                                ui.add(egui::Label::new(RichText::new(said).small().color(ERROR)).truncate()).on_hover_text(said);
                            }
                        }
                    } else if shown {
                        ui.label(RichText::new("ESCAPE FILE · F5 AGAIN · F9 KEEP RUN").small().color(QUIET));
                    }
                });
            });
        });
        if let Some(zip) = show_zip {
            crate::ide::export::show_in_folder(&zip);
        }
        if forget_gone {
            self.managing.gone = None;
        }
        if undo {
            self.undo();
        }
    }

    /// BUILD: the text being changed saved, the game and this program
    /// packed into one zip in the game's `builds/` (`export.rs`) on a thread
    /// of its own, so the window goes on; the bar says BUILDING… and then
    /// what came of it (`built`).
    pub fn build(&mut self) {
        if self.bar.building.is_some() {
            return;
        }
        self.flush();
        let out = crate::ide::export::out_path(&self.dir);
        let (dir, title, to) = (self.dir.clone(), self.title.clone(), out.clone());
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let made = crate::cli::program().map_err(anyhow::Error::from).and_then(|program| crate::ide::export::export(&dir, &title, &program, &to));
            let _ = send.send(made);
        });
        self.bar.building = Some((receive, out));
        (self.bar.notice, self.bar.notice_until) = (Some(Notice::Building), None);
    }

    /// BUILD's answer: where the zip is and how big, or why it was not made.
    pub(super) fn built(&mut self, made: anyhow::Result<crate::ide::export::Packed>, out: PathBuf) {
        self.bar.notice_until = None;
        match made {
            Ok(packed) => {
                let bytes = packed.bytes;
                let zip = out.canonicalize().unwrap_or(out);
                // (From the game's folder: the whole path would not fit the
                // bar, and its end is what tells.)
                let dir = self.dir.canonicalize().unwrap_or_else(|_| self.dir.clone());
                let mut said = format!("BUILT {} ({:.1} MB)", zip.strip_prefix(&dir).unwrap_or(&zip).display(), bytes as f64 / 1e6);
                self.report(Kind::Note, said.clone(), None);
                // Ignored by a `.gitignore` and so not packed, yet named by
                // the game: the build would not find them.
                if !packed.missing.is_empty() {
                    let list = packed.missing.join(", ");
                    let warning = format!("NOT PACKED, IGNORED BY .gitignore, BUT NAMED BY THE GAME: {list}. The build will not find them: take them out of .gitignore.");
                    self.report(Kind::Error, warning, None);
                    said = format!("{said}; {} IGNORED FILES THE GAME NAMES NOT PACKED (OUTPUT)", packed.missing.len());
                }
                self.bar.notice = Some(Notice::Built(zip, said));
            }
            Err(e) => {
                let said = format!("BUILD FAILED: {e:#}");
                self.report(Kind::Error, said.clone(), None);
                self.bar.notice = Some(Notice::Failed(said));
            }
        }
    }
}

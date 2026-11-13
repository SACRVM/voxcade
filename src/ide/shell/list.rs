//! The list on the left: each sort of file under its heading that folds,
//! a plus making a new one, a name typed in place (the plus's, RENAME's),
//! a right click's RENAME and DELETE, the built-in sounds; or the search
//! across the game in its place.

use super::*;

/// How wide the list's panel grows at most while SEARCH is open.
pub(super) const SEARCH_WIDE: f32 = 560.0;

impl Shell {
    /// The list on the left (or the search in its place), and what was
    /// chosen, made or renamed in it.
    pub(super) fn list(&mut self, ui: &mut egui::Ui) {
        let (mut clicked, mut edit, mut made, mut builtin) = (None, None, None, None);
        let mut managed: Option<(Act, String)> = None;
        let mut renamed: Option<(String, String)> = None;
        let mut stop_renaming = false;
        // The margins on the right lie inside the scroll areas, so that their
        // bars sit at the edge.
        let (mut hit, mut search_closed) = (None, false);
        // While SEARCH is open the panel is wider (a third of the window,
        // up to SEARCH_WIDE), so that a hit shows more of its line.
        let panel = if self.search.shown {
            let wide = (ui.ctx().content_rect().width() * 0.34).clamp(250.0, SEARCH_WIDE);
            egui::Panel::left("files searched").exact_size(wide)
        } else {
            egui::Panel::left("files").default_size(250.0)
        };
        panel.frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 14, right: 0, top: 12, bottom: 12 })).show(ui, |ui| {
            // The search across the game in the list's place while shown.
            if self.search.shown {
                (hit, search_closed) = self.search.ui(ui, &self.dir);
                return;
            }
            egui::ScrollArea::vertical().content_margin(egui::Margin { right: 6, ..egui::Margin::ZERO }).show(ui, |ui| {
                let open = if self.outside.is_some() || self.builtin.is_some() { None } else { self.memory.open.clone() };
                #[cfg(test)]
                self.headings.clear();
                // Each sort of file under its name, which opens and closes
                // it: the logic, then the assets, the big groups closed at
                // first.
                // The scenarios after the logic, there even while the game
                // has none: every game should have some to be tried with.
                let logic = [("Logic", &self.files.logic, true), ("Scenarios", &self.files.scenarios, true)].into_iter();
                let assets = self.files.assets.iter().map(|g| (g.name, &g.files, g.files.len() <= 12 && g.name != "Other"));
                for (i, (sort, files, open_at_first)) in logic.chain(assets).enumerate() {
                    if i > 0 {
                        ui.add_space(10.0);
                    }
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(sort), open_at_first);
                    let header = crate::gui::row(ui, &format!("{} ({})", sort.to_uppercase(), files.len()), TextStyle::Small, QUIET, false, None);
                    let at = egui::pos2(header.rect.right() - INSET - 4.0, header.rect.center().y);
                    // A plus of its own left of the arrow (over the row, so it
                    // takes its clicks): a new file there, its name typed
                    // below.
                    let mut over_plus = false;
                    if let Some(tip) = new_what(sort) {
                        let rect = egui::Rect::from_center_size(egui::pos2(at.x - 26.0, at.y), egui::vec2(24.0, 22.0));
                        let plus = ui.interact(rect, ui.make_persistent_id((sort, "plus")), egui::Sense::click()).on_hover_text(tip);
                        over_plus = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| rect.contains(p));
                        if plus.hovered() {
                            ui.painter().rect_filled(rect, 3.0, crate::gui::ROW_HOVERED);
                        }
                        #[cfg(test)]
                        self.headings.push((sort.to_string(), rect, header.rect));
                        let colour = if plus.hovered() { TEXT } else { QUIET };
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "+", TextStyle::Body.resolve(ui.style()), colour);
                        if plus.clicked() {
                            self.naming = Some(Naming { sort: sort.to_string(), text: String::new(), blocks: false, problem: None, fresh: true });
                            state.set_open(true);
                        }
                    }
                    if header.clicked() && !over_plus {
                        state.toggle(ui);
                    }
                    arrow(ui, at, state.openness(ui.ctx()));
                    // A file being renamed from the main area: its heading opens.
                    if self.renaming.as_ref().is_some_and(|r| r.fresh && files.contains(&r.path)) {
                        state.set_open(true);
                    }
                    state.show_body_unindented(ui, |ui| {
                        // The name being typed for a new file, first.
                        if let Some(naming) = self.naming.as_mut().filter(|n| n.sort == sort) {
                            let (mut enter, mut cancel) = (false, false);
                            ui.horizontal(|ui| {
                                ui.add_space(INSET);
                                let edit = ui.add(egui::TextEdit::singleline(&mut naming.text).font(TextStyle::Body).hint_text("name, then Enter").desired_width(f32::INFINITY));
                                if std::mem::take(&mut naming.fresh) {
                                    edit.request_focus();
                                }
                                if edit.lost_focus() {
                                    enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    cancel = ui.input(|i| i.key_pressed(egui::Key::Escape)) || (!enter && naming.text.is_empty());
                                    if enter {
                                        // (The file made opens in a text that would take the key too.)
                                        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                                        edit.request_focus();
                                    }
                                }
                            });
                            // A new level: a room of pieces, or (where the game
                            // has blocks) a level of blocks.
                            if sort == "Levels" && self.dir.join(crate::blocks::FOLDER).is_dir() {
                                ui.horizontal(|ui| {
                                    ui.add_space(INSET);
                                    for (blocks, word, tip) in [(false, "ROOM OF PIECES", "Models and lights set down anywhere"), (true, "LEVEL OF BLOCKS", "A grid of the game's blocks")] {
                                        if ui.selectable_label(naming.blocks == blocks, RichText::new(word).small()).on_hover_text(tip).clicked() {
                                            naming.blocks = blocks;
                                            naming.fresh = true;
                                        }
                                    }
                                });
                            }
                            if let Some(problem) = &naming.problem {
                                ui.label(RichText::new(problem).small().color(ERROR));
                            }
                            if cancel {
                                self.naming = None;
                            } else if enter {
                                made = Some((sort.to_string(), naming.text.clone(), naming.blocks));
                            }
                        }
                        // The scripts by their paths; assets say what they
                        // are by their group: the name alone, the path where
                        // two have the same name.
                        let base = |f: &str| f.rsplit('/').next().unwrap_or(f).to_string();
                        for f in files {
                            // A file being renamed: its name typed in its place.
                            if let Some(r) = self.renaming.as_mut().filter(|r| r.path == *f) {
                                let (ext, mut enter, mut cancel) = (format!(".{}", manage::split_name(f).2), false, false);
                                ui.horizontal(|ui| {
                                    ui.add_space(INSET);
                                    let id = ui.make_persistent_id(("renaming", f.as_str()));
                                    let room = if ext.len() > 1 { 12.0 + 7.0 * ext.len() as f32 } else { 0.0 };
                                    let edit = ui.add(egui::TextEdit::singleline(&mut r.text).id(id).font(TextStyle::Body).desired_width((ui.available_width() - room).max(60.0)));
                                    if ext.len() > 1 {
                                        ui.label(RichText::new(&ext).small().color(QUIET));
                                    }
                                    if std::mem::take(&mut r.fresh) {
                                        // The name is selected, to be typed over.
                                        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                                        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(r.text.chars().count()))));
                                        state.store(ui.ctx(), id);
                                        edit.request_focus();
                                        edit.scroll_to_me(None);
                                    }
                                    if edit.lost_focus() {
                                        enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        // Anything but Enter leaves it (a click elsewhere too).
                                        cancel = !enter;
                                        if enter {
                                            // (A name that cannot be keeps the field, with the keys.)
                                            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                                            edit.request_focus();
                                        }
                                    }
                                });
                                if let Some(problem) = &r.problem {
                                    ui.label(RichText::new(problem).small().color(ERROR));
                                }
                                if cancel {
                                    stop_renaming = true;
                                } else if enter {
                                    renamed = Some((f.clone(), r.text.clone()));
                                }
                                continue;
                            }
                            let short = sort != "Logic" && files.iter().filter(|g| base(g) == base(f)).count() == 1;
                            let on = open.as_deref() == Some(f.as_str());
                            let response = crate::gui::row(ui, &if short { base(f) } else { f.clone() }, TextStyle::Body, if on { ACCENT } else { TEXT }, on, None);
                            if response.clicked() {
                                clicked = Some(f.clone());
                            }
                            // A double click opens it in its editor.
                            if response.double_clicked() {
                                edit = Some(f.clone());
                            }
                            // A right click: RENAME and DELETE.
                            let why = manage::why_not(f, sort);
                            response.context_menu(|ui| {
                                for (word, act, tip) in [("RENAME", Act::Rename, "Gives the file another name, typed here in the list."), ("DELETE", Act::Delete, "Moves the file to the trash. UNDO shows for a few seconds.")] {
                                    if ui.add_enabled(why.is_none(), egui::Button::new(word)).on_hover_text(tip).on_disabled_hover_text(why.unwrap_or("")).clicked() {
                                        managed = Some((act, f.clone()));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        // The sounds the game has from the engine and its
                        // ready-made rules, which no file of its own replaces.
                        if sort == "Sounds" {
                            for (name, _) in &self.brought {
                                if files.iter().any(|f| sounds::name_of(f) == Some(name)) {
                                    continue;
                                }
                                let on = self.builtin == Some(*name) && self.outside.is_none();
                                if crate::gui::row(ui, name, TextStyle::Body, if on { ACCENT } else { QUIET }, on, Some("built in")).clicked() {
                                    builtin = Some(*name);
                                }
                            }
                        }
                    });
                }
            });
        });
        if let Some(path) = clicked {
            self.open(&path);
            self.back_to_file();
            self.help = false;
        }
        if search_closed {
            self.search.shown = false;
        }
        if let Some((file, line)) = hit {
            self.go_to_hit(&file, line);
        }
        if let Some(name) = builtin {
            // Shown like a file, which is let go of (saved) first.
            self.flush();
            self.hush();
            self.outside = None;
            self.unload();
            self.builtin = Some(name);
            self.hide_game();
            self.help = false;
        }
        if let Some(path) = edit.filter(|p| self.editable(p)) {
            self.edits.push(path);
        }
        if let Some((sort, typed, blocks)) = made {
            let made = if sort == "Levels" { self.make_level(&typed, blocks) } else { self.make_file(&sort, &typed) };
            match made {
                Ok(_) => self.naming = None,
                Err(problem) => {
                    if let Some(n) = &mut self.naming {
                        n.problem = Some(problem);
                    }
                }
            }
        }
        if let Some((act, path)) = managed {
            self.manage(act, &path);
        }
        if stop_renaming {
            self.renaming = None;
        }
        if let Some((path, typed)) = renamed {
            match self.rename(&path, &typed) {
                Ok(()) => self.renaming = None,
                Err(problem) => {
                    if let Some(r) = &mut self.renaming {
                        r.problem = Some(problem);
                    }
                }
            }
        }
    }
}

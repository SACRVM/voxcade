//! The list on the left: each sort of file under its heading that folds,
//! a plus making a new one, a name typed in place (the plus's, RENAME's),
//! a right click's RENAME and DELETE, the built-in sounds; or the search
//! across the game in its place.

use super::*;
use crate::ide::find::{self, FIELD_MARGIN, field, select_all};

/// How wide the list's panel grows at most while SEARCH is open.
pub(super) const SEARCH_WIDE: f32 = 560.0;

/// The filter over the list's names (Ctrl+P): what is typed, and the rows
/// it leaves, worked out when the text or the files change, not every frame.
#[derive(Default)]
pub(super) struct Filter {
    pub(super) text: String,
    /// The text trimmed that `rows` are for, and for each heading the places
    /// in its files of the rows left (`Files::matching`).
    rows: Option<(String, Vec<Vec<usize>>)>,
}

impl Filter {
    /// The files changed: the rows are worked out again.
    pub(super) fn stale(&mut self) {
        self.rows = None;
    }

    /// The text to look for (trimmed) and the rows it leaves; None while
    /// there is nothing to look for.
    fn shown(&mut self, files: &Files) -> Option<(&str, &[Vec<usize>])> {
        let needle = self.text.trim();
        if needle.is_empty() {
            return None;
        }
        if self.rows.as_ref().is_none_or(|(t, _)| t != needle) {
            self.rows = Some((needle.to_string(), files.matching(&needle.to_lowercase())));
        }
        self.rows.as_ref().map(|(t, r)| (t.as_str(), r.as_slice()))
    }
}

/// The filter field's id.
pub(super) fn filter_id() -> egui::Id {
    egui::Id::unique("list filter")
}

impl Shell {
    /// The list filtered by `text` (a shot's `--filter`).
    pub fn show_filter(&mut self, text: &str) {
        self.filter.text = text.to_string();
    }

    /// The list on the left (or the search in its place), and what was
    /// chosen, made or renamed in it.
    pub(super) fn list(&mut self, ui: &mut egui::Ui) {
        let (mut clicked, mut edit, mut made, mut builtin) = (None, None, None, None);
        let mut managed: Option<(Act, String)> = None;
        let mut renamed: Option<(String, String)> = None;
        let (mut stop_renaming, mut unfilter) = (false, false);
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
            // The filter over the names, above the headings: no label, its
            // hint says what it is.
            let want_focus = !self.keys_are_taken(ui) && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P));
            let typed = ui.add(field(ui, &mut self.filter.text, filter_id(), ui.available_width() - 16.0 - 2.0 * FIELD_MARGIN.x).hint_text("FILTER"));
            let typed = typed.on_hover_text("Shows only the files whose names hold this text. Ctrl+P · Escape clears it. SEARCH looks inside the files.");
            if want_focus {
                select_all(ui.ctx(), filter_id(), &self.filter.text);
                typed.request_focus();
            }
            // An ✕ at the field's right end while there is text (drawn over
            // the field, so nothing below moves).
            if !self.filter.text.is_empty() {
                let cross = egui::Rect::from_center_size(egui::pos2(typed.rect.right() - 14.0, typed.rect.center().y), egui::vec2(18.0, 18.0));
                let hit = ui.interact(cross, filter_id().with("clear"), egui::Sense::click()).on_hover_text("Clear · Escape");
                let colour = if hit.hovered() { TEXT } else { QUIET };
                let c = cross.center();
                for (a, b) in [(egui::vec2(-3.5, -3.5), egui::vec2(3.5, 3.5)), (egui::vec2(-3.5, 3.5), egui::vec2(3.5, -3.5))] {
                    ui.painter().line_segment([c + a, c + b], egui::Stroke::new(1.5, colour));
                }
                if hit.clicked() {
                    self.filter.text.clear();
                    typed.request_focus();
                }
            }
            let (enter, escape) = if typed.has_focus() || typed.lost_focus() { ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape))) } else { (false, false) };
            if escape {
                self.filter.text.clear();
            }
            let enter = enter && typed.lost_focus();
            ui.add_space(6.0);
            let shown = self.filter.shown(&self.files);
            // Enter opens the first row left.
            if let Some((_, rows)) = shown.filter(|_| enter) {
                clicked = self.files.headings().zip(rows).find_map(|((_, files), at)| at.first().map(|&i| files[i].clone()));
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).content_margin(egui::Margin { right: 6, ..egui::Margin::ZERO }).show(ui, |ui| {
                let open = if self.outside.is_some() || self.sound.builtin.is_some() { None } else { self.memory.open.clone() };
                // Each sort of file under its name, which opens and closes
                // it: the logic, then the assets, the big groups closed at
                // first. The scenarios after the logic, there even while
                // the game has none: every game should have some to be
                // tried with.
                // While a filter is set only the headings with a row left
                // show, all open, and the folds are neither read nor written.
                let needle = shown.map(|(n, _)| n);
                let mut drawn = false;
                for (i, (sort, files)) in self.files.headings().enumerate() {
                    let here = shown.map(|(_, rows)| rows[i].as_slice());
                    let builtin_left = sort == "Sounds" && needle.is_some_and(|n| self.sound.brought.iter().any(|(name, _)| !find::matches(name, n, false).is_empty()));
                    let naming_here = self.managing.naming.as_ref().is_some_and(|n| n.sort == sort);
                    if here.is_some_and(<[usize]>::is_empty) && !builtin_left && !naming_here {
                        continue;
                    }
                    if std::mem::replace(&mut drawn, true) {
                        ui.add_space(10.0);
                    }
                    let open_at_first = matches!(sort, "Logic" | "Scenarios") || (files.len() <= 12 && sort != "Other");
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(sort), open_at_first);
                    let header = crate::gui::row(ui, &format!("{} ({})", sort.to_uppercase(), here.map_or(files.len(), <[usize]>::len)), TextStyle::Small, QUIET, false, None);
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
                        let colour = if plus.hovered() { TEXT } else { QUIET };
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "+", TextStyle::Body.resolve(ui.style()), colour);
                        if plus.clicked() {
                            self.managing.naming = Some(Naming { sort: sort.to_string(), text: String::new(), blocks: false, problem: None, fresh: true });
                            // (A file made must show: the filter goes.)
                            unfilter = true;
                            state.set_open(true);
                            state.store(ui.ctx());
                        }
                    }
                    if header.clicked() && !over_plus && here.is_none() {
                        state.toggle(ui);
                    }
                    arrow(ui, at, if here.is_some() { 1.0 } else { state.openness(ui.ctx()) });
                    // A file being renamed from the main area: its heading
                    // opens, and the filter goes if it hides the file.
                    if let Some(at) = self.managing.renaming.as_ref().filter(|r| r.fresh).and_then(|r| files.iter().position(|f| *f == r.path)) {
                        unfilter |= here.is_some_and(|rows| rows.binary_search(&at).is_err());
                        state.set_open(true);
                        state.store(ui.ctx());
                    }
                    let mut body = |ui: &mut egui::Ui| {
                        // The name being typed for a new file, first.
                        if let Some(naming) = self.managing.naming.as_mut().filter(|n| n.sort == sort) {
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
                                self.managing.naming = None;
                            } else if enter {
                                made = Some((sort.to_string(), naming.text.clone(), naming.blocks));
                            }
                        }
                        for (_, f) in files.iter().enumerate().filter(|(j, _)| here.is_none_or(|rows| rows.binary_search(j).is_ok())) {
                            // A file being renamed: its name typed in its place.
                            if let Some(r) = self.managing.renaming.as_mut().filter(|r| r.path == *f) {
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
                            // The scripts by their paths; assets say what
                            // they are by their group (`Files::label`).
                            let on = open.as_deref() == Some(f.as_str());
                            let response = crate::gui::row(ui, self.files.label(sort, f), TextStyle::Body, if on { ACCENT } else { TEXT }, on, self.starts.of(f));
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
                            for (name, _) in &self.sound.brought {
                                if files.iter().any(|f| sounds::name_of(f) == Some(name)) || needle.is_some_and(|n| find::matches(name, n, false).is_empty()) {
                                    continue;
                                }
                                let on = self.sound.builtin == Some(*name) && self.outside.is_none();
                                if crate::gui::row(ui, name, TextStyle::Body, if on { ACCENT } else { QUIET }, on, Some("built in")).clicked() {
                                    builtin = Some(*name);
                                }
                            }
                        }
                    };
                    if here.is_some() {
                        body(ui);
                    } else {
                        state.show_body_unindented(ui, body);
                    }
                }
                if needle.is_some() && !drawn {
                    ui.horizontal(|ui| {
                        ui.add_space(INSET);
                        ui.label(RichText::new("NOTHING FOUND").small().color(QUIET));
                    });
                }
            });
        });
        if unfilter {
            self.filter.text.clear();
        }
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
            self.sound.builtin = Some(name);
            self.hide_game();
            self.help = false;
        }
        if let Some(path) = edit.filter(|p| self.editable(p)) {
            self.edits.push(path);
        }
        if let Some((sort, typed, blocks)) = made {
            let made = if sort == "Levels" { self.make_level(&typed, blocks) } else { self.make_file(&sort, &typed) };
            match made {
                Ok(_) => {
                    self.managing.naming = None;
                    self.filter.text.clear();
                }
                Err(problem) => {
                    if let Some(n) = &mut self.managing.naming {
                        n.problem = Some(problem);
                    }
                }
            }
        }
        if let Some((act, path)) = managed {
            self.manage(act, &path);
        }
        if stop_renaming {
            self.managing.renaming = None;
        }
        if let Some((path, typed)) = renamed {
            match self.rename(&path, &typed) {
                Ok(()) => self.managing.renaming = None,
                Err(problem) => {
                    if let Some(r) = &mut self.managing.renaming {
                        r.problem = Some(problem);
                    }
                }
            }
        }
    }
}

/// Which level a run begins in and which is the game's own title, said at
/// their rows' right ends: from `conf.lua` as it lies in the folder, read
/// anew when it or the levels' names change (looked at with the list).
#[derive(Default)]
pub(super) struct Starts {
    /// How the game's `conf.lua` is read (`Shell::with_conf`); without it
    /// the settings the IDE opened with tell.
    pub read: Option<fn(&std::path::Path) -> anyhow::Result<crate::config::GameConfig>>,
    /// What was looked at last: `conf.lua`'s text and the level files.
    seen: Option<(Option<blake3::Hash>, Vec<String>)>,
    start: Option<String>,
    title: Option<String>,
}

impl Starts {
    /// Looks at `conf.lua` and the levels; whether a mark moved.
    pub fn look(&mut self, dir: &std::path::Path, opened: &crate::config::GameConfig, files: &Files) -> bool {
        let levels: Vec<String> = files.headings().find(|(sort, _)| *sort == "Levels").map(|(_, f)| f.iter().filter(|f| f.starts_with("levels/")).cloned().collect()).unwrap_or_default();
        let conf = std::fs::read(dir.join("conf.lua")).ok().map(|b| blake3::hash(&b));
        if self.seen.as_ref().is_some_and(|(c, l)| *c == conf && *l == levels) {
            return false;
        }
        // A conf.lua that does not read keeps the marks as they were.
        let read = self.read.map(|read| read(dir));
        let cfg = match &read {
            Some(Ok(cfg)) => cfg,
            Some(Err(_)) if self.seen.is_some() => {
                self.seen = Some((conf, levels));
                return false;
            }
            _ => opened,
        };
        let names: Vec<String> = levels.iter().map(|f| std::path::Path::new(f).file_stem().unwrap_or_default().to_string_lossy().into_owned()).collect();
        let (start, title) = crate::levels::marked(cfg, &names);
        let was = (self.start.take(), self.title.take());
        self.start = start.map(|i| levels[i].clone());
        self.title = title.map(|i| levels[i].clone());
        self.seen = Some((conf, levels));
        was != (self.start.clone(), self.title.clone())
    }

    /// The word at a file's row: "start" at the level a run begins in,
    /// "title" at the game's own title.
    pub fn of(&self, file: &str) -> Option<&'static str> {
        if self.start.as_deref() == Some(file) {
            Some("start")
        } else if self.title.as_deref() == Some(file) {
            Some("title")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_a_run_begins_in_and_the_title_are_marked() {
        let dir = crate::scratch::Scratch::new("ide-starts");
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(dir.join(format!("levels/{name}.toml")), "").unwrap();
        }
        let files = Files::from_names(&dir, Files::names(&dir));
        let mut cfg = crate::config::GameConfig::default();
        let mut starts = Starts::default();
        starts.look(&dir, &cfg, &files);
        assert_eq!((starts.of("levels/a.toml"), starts.of("levels/b.toml")), (Some("start"), None), "the first, unless said");
        // Nothing changed in the folder: nothing is read again.
        assert!(!starts.look(&dir, &cfg, &files));
        cfg.start = Some("c".into());
        cfg.title_screen.level = Some("a".into());
        let mut starts = Starts::default();
        starts.look(&dir, &cfg, &files);
        assert_eq!((starts.of("levels/a.toml"), starts.of("levels/b.toml"), starts.of("levels/c.toml")), (Some("title"), None, Some("start")));
        // One level alone says nothing.
        std::fs::remove_file(dir.join("levels/b.toml")).unwrap();
        std::fs::remove_file(dir.join("levels/c.toml")).unwrap();
        let mut starts = Starts::default();
        starts.look(&dir, &crate::config::GameConfig::default(), &Files::from_names(&dir, Files::names(&dir)));
        assert_eq!(starts.of("levels/a.toml"), None);
    }
}

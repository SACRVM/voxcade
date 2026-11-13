//! The main area: the red line of what is wrong over the one thing open,
//! a file (its name and buttons above it: SAVED, FIND, RENAME, DELETE,
//! EDIT, TEXT, PLAY SONG, PLAY SCENARIO), a picture, HELP or the game.

use super::*;

impl Shell {
    /// The main area: the red line of what is wrong with the game, over the
    /// open file (or the game, HELP, nothing).
    pub(super) fn main(&mut self, ui: &mut egui::Ui) {
        if let Some((said, copy)) = self.game_error() {
            let at = crate::ide::play::line_of(&copy).filter(|(file, _)| self.files.has(file));
            let (go, show) = game_error_row(ui, &said, &copy, at.is_some());
            if let Some(plain) = crate::ide::play::plain_words(&copy) {
                ui.label(RichText::new(plain).color(QUIET));
            }
            ui.add_space(8.0);
            if show {
                self.show_output();
            }
            if let (true, Some((file, line))) = (go, at) {
                self.hide_game();
                self.help = false;
                self.outside = None;
                self.open(&file);
                self.goto = Some((line, true));
            }
        }
        self.main_area(ui);
    }

    /// The main area: the open file.
    pub(super) fn main_area(&mut self, ui: &mut egui::Ui) {
        // Asked for the text open, once: the line to go to, the keys to it
        // (gone if what shows is no text).
        let (goto, focus) = (self.goto.take(), std::mem::take(&mut self.focus_code));
        if self.stage.as_ref().is_some_and(|s| s.shown) {
            return self.game(ui);
        }
        if self.help {
            return crate::ide::help::ui(ui, &mut self.help_topic);
        }
        if let Some(file) = self.outside.clone() {
            if self.shown.is_none() {
                self.shown = Some(load(&file, &file.to_string_lossy(), ui.ctx()));
                self.facts = None;
            }
            ui.label(RichText::new(format!("{} · NOT IN THE GAME", file.display())).color(QUIET));
            ui.add_space(10.0);
            egui::ScrollArea::both().id_salt("outside").auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO }).show(ui, |ui| match self.shown.as_ref() {
                Some(Shown::Image(texture)) => picture(ui, texture),
                Some(Shown::Note(note)) => {
                    ui.label(RichText::new(note.as_str()).color(QUIET));
                }
                _ => {}
            });
            return;
        }
        if let Some(name) = self.builtin {
            return self.builtin_ui(ui, name);
        }
        let Some(path) = self.memory.open.clone() else {
            return;
        };
        // The open file gone from the folder (the agent, or anything outside
        // the IDE, removed or renamed it): said plainly, nothing made again
        // by itself. A text keeps its text (SAVE AGAIN, below); anything else
        // has nothing to show.
        if !self.files.has(&path) && !matches!(self.shown, Some(Shown::Code(_))) {
            let said = if self.opened_there { format!("{path} IS GONE FROM THE FOLDER") } else { format!("NO FILE {path} IN THE GAME") };
            ui.label(RichText::new(said).color(ACCENT));
            return;
        }
        if self.shown.is_none() {
            // Scripts and the texts they read, sounds and songs are to
            // change, the rest to look at.
            let to_change = self.files.logic.contains(&path) || self.files.scenarios.contains(&path) || path.ends_with(".mml") || (path.ends_with(".toml") && sounds::name_of(&path).is_some()) || (self.model_text && (path.ends_with(".vvx") || self.is_level(&path))) || self.is_map(&path);
            self.shown = Some(if to_change {
                match Buffer::open(&self.dir, &path, &self.memory_path.with_extension("journal")) {
                    Ok(buffer) => Shown::Code(Box::new(buffer)),
                    Err(e) => Shown::Note(format!("{e:#}")),
                }
            } else {
                load(&self.dir.join(&path), &path, ui.ctx())
            });
            let level = self.is_level(&path);
            self.facts = match () {
                _ if self.is_scene(&path) => Some(scene_facts(&self.dir.join(&path), &path)),
                _ if self.editable(&path) || is_model_file(&path) => Some(facts(&self.dir.join(&path), &path, level)),
                _ if path.ends_with(".mml") => Some(song_facts(&self.dir.join(&path))),
                _ => None,
            };
            // What an editor opens shows as a picture.
            let preview = match () {
                _ if level && !self.model_text => Some(Preview::level(&self.dir, &path, &self.cfg)),
                _ if self.is_scene(&path) && !self.model_text => Some(Preview::scene(&self.dir, &path, &self.cfg)),
                _ if is_model_file(&path) && !self.model_text => Some(Preview::model(&self.dir, &path, &self.cfg).map(|p| p.on(self.model_background()))),
                _ => None,
            };
            self.preview = preview.and_then(|p| p.map_err(|e| self.message = Some(format!("{e:#}"))).ok());
            match (&mut self.preview, self.spare.take()) {
                (Some(new), Some(old)) => new.adopt(old),
                (None, Some(old)) => self.retired.extend(old.texture()),
                _ => {}
            }
        }
        // What the text says is read afresh if it changed (its line of words and
        // its mistake stand above it).
        if let Some(Shown::Code(b)) = &mut self.shown {
            b.code.check();
        }
        // Whether the text shown is saved (a word above it, left of the buttons),
        // and RENAME and DELETE beside it (off with the reason for a file the
        // game needs).
        let saved = self.saved_word();
        let mut want_find = false;
        let why_not = self.files.sort_of(&path).map_or(Some("This is not a file of the game."), |group| manage::why_not(&path, group));
        let mut act = None;
        // What an editor opens: a line of what it is, and EDIT; a song:
        // what it is, and PLAY.
        let facts = match &self.shown {
            // A song's line follows its text, a model's too.
            Some(Shown::Code(b)) if path.ends_with(".mml") => Some(b.code.song.clone().unwrap_or_else(|| "The song cannot be read.".into())),
            Some(Shown::Code(b)) if path.ends_with(".vvx") => Some(b.code.model.clone().unwrap_or_else(|| "The model cannot be read.".into())),
            Some(Shown::Code(b)) if self.is_level(&path) => Some(b.code.model.clone().unwrap_or_else(|| "The level cannot be read.".into())),
            _ => self.facts.clone(),
        };
        let had_facts = facts.is_some();
        if let Some(facts) = facts {
            header_row(ui, &path, Some((&facts, "")), |ui| {
                {
                    if self.editable(&path) {
                        if ui.button("EDIT").on_hover_text("Opens it in its editor, a window of its own (so does a double click in the list).").clicked() {
                            self.flush();
                            self.edits.push(path.clone());
                        }
                        // A model is text too: shown as its text to change,
                        // or as its picture.
                        let level = self.is_level(&path);
                        if path.ends_with(".vvx") || level {
                            let (word, tip) = match (self.model_text, level) {
                                (true, false) => ("PICTURE", "Shows the model as the game draws it."),
                                (false, false) => ("TEXT", "Shows the model's text, to read and change (its palette, its frames or parts, layer by layer)."),
                                (true, true) => ("PICTURE", "Shows the level as its editor draws it."),
                                (false, true) => ("TEXT", "Shows the level's text, to read and change (its blocks, its cells layer by layer, its things)."),
                            };
                            if ui.button(word).on_hover_text(tip).clicked() {
                                self.flush();
                                self.model_text = !self.model_text;
                                self.unload();
                            }
                        }
                    } else if !path.ends_with(".mml") {
                    } else if self.listening.is_some() {
                        if ui.button("STOP SONG").on_hover_text("Ends the song.").clicked() {
                            self.listen();
                        }
                    } else if ui.button("PLAY SONG").on_hover_text("Plays the song as it stands, at the game's music volume.").clicked() {
                        self.listen();
                    }
                    act = act.or(manage::buttons(ui, why_not));
                    want_find |= saved_label(ui, saved);
                }
            });
            // (A text to change shows it with its own messages.)
            if !matches!(self.shown, Some(Shown::Code(_))) && let Some(message) = &self.message {
                error_row(ui, message, message);
            }
            ui.add_space(10.0);
        }
        // A scenario: what it is, and PLAY SCENARIO.
        if let Some(name) = path.strip_prefix("scenarios/").and_then(|n| n.strip_suffix(".lua")).filter(|_| self.files.scenarios.contains(&path)) {
            let mut play = false;
            header_row(ui, &path, Some(("scenario", "The game begun late, set up by this code.")), |ui| {
                play = ui.button("PLAY SCENARIO").on_hover_text("Plays the game from it (saved first); F5 begins it again.").clicked();
                act = act.or(manage::buttons(ui, why_not));
                want_find |= saved_label(ui, saved);
            });
            ui.add_space(6.0);
            if play {
                self.play_scenario(name);
            }
        } else if !had_facts && sounds::name_of(&path).is_none() {
            // Any other file: its name, its buttons, and the word if it is a
            // text to change.
            header_row(ui, &path, None, |ui| {
                act = act.or(manage::buttons(ui, why_not));
                want_find |= saved_label(ui, saved);
            });
            ui.add_space(6.0);
        }
        // A sound: what it is in words, and PLAY SOUND where the maker
        // (whose PLAY and LOOP play it) is not over its text; where it is
        // used lies under its text.
        let sound_name = sounds::name_of(&path).map(str::to_string);
        let maker = sound_name.is_some() && path.ends_with(".toml") && matches!(self.shown, Some(Shown::Code(_)));
        let mut play_sound = false;
        if sound_name.is_some() {
            let key = match &self.shown {
                Some(Shown::Code(b)) => format!("{path}\0{}", b.code.text),
                _ => path.clone(),
            };
            if self.sound_line.as_ref().is_none_or(|(k, _)| *k != key) {
                let line = match &self.shown {
                    Some(Shown::Code(b)) => b.code.sound.as_ref().map_or_else(|| "The sound cannot be read.".to_string(), sounds::describe),
                    _ => "The sound cannot be read.".to_string(),
                };
                self.sound_line = Some((key, line));
            }
            let line = self.sound_line.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
            header_row(ui, &path, Some((&line, "")), |ui| {
                if !maker {
                    play_sound = ui.button("PLAY SOUND").on_hover_text("Plays the sound as it stands, at the game's sound volume.").clicked();
                }
                act = act.or(manage::buttons(ui, why_not));
                want_find |= saved_label(ui, saved);
            });
            if !matches!(self.shown, Some(Shown::Code(_))) && let Some(message) = &self.message {
                error_row(ui, message, message);
            }
            ui.add_space(6.0);
        }
        // A sound of numbers to change: the maker between its line and its text.
        let id = egui::Id::unique(("code", path.as_str()));
        if maker {
            play_sound |= self.maker_ui(ui, id);
            ui.add_space(10.0);
        }
        if play_sound {
            self.play_sound();
        }
        if let Some(act) = act {
            self.manage(act, &path);
            return;
        }
        if let Some(preview) = &mut self.preview {
            // Down to the console, the scroll bar's room kept at the right.
            let animate = self.animate;
            ui.allocate_ui(egui::vec2(ui.available_width() - 12.0, ui.available_height() - 14.0), |ui| preview.ui(ui, animate));
            return;
        }
        // Unloaded by a button of this frame (TEXT, PICTURE): shown anew in
        // the next one.
        if self.shown.is_none() {
            ui.ctx().request_repaint();
            return;
        }
        // A line to go to (a game's error): the cursor there, in view.
        let mut focus = focus;
        if let (Some((line, keys)), Some(Shown::Code(buffer))) = (goto, &self.shown) {
            let at: usize = buffer.code.text.split('\n').take(line.saturating_sub(1)).map(|l| l.chars().count() + 1).sum();
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
            state.store(ui.ctx(), id);
            focus |= keys;
            let row = ui.text_style_height(&TextStyle::Monospace);
            self.restore = Some(line.saturating_sub(6) as f32 * row);
        }
        // The cursor where it stood when the text was last open (another
        // session: in this one egui keeps it), the keys to the text if
        // asked.
        if let Some(Shown::Code(buffer)) = &self.shown {
            if egui::TextEdit::load_state(ui.ctx(), id).is_none()
                && let Some(&at) = self.memory.cursor.get(&path)
            {
                let mut state = egui::text_edit::TextEditState::default();
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at.min(buffer.code.text.chars().count())))));
                state.store(ui.ctx(), id);
            }
            if focus {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
        // A text being changed: what it asks of the user above it.
        let mut put_back = false;
        if let Some(Shown::Code(buffer)) = &mut self.shown {
            if buffer.gone {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{path} IS GONE FROM THE FOLDER · TEXT KEPT")).color(ACCENT));
                    if ui.button("SAVE AGAIN").on_hover_text(format!("Writes the text back into the game folder as {path}.")).clicked() {
                        put_back = true;
                    }
                });
                ui.add_space(8.0);
            }
            if buffer.theirs.is_some() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CHANGED OUTSIDE TOO").color(ACCENT));
                    if ui.button("KEEP MINE").clicked() {
                        buffer.keep_mine();
                    }
                    if ui.button("TAKE THE FILE'S").clicked() {
                        buffer.take_theirs();
                    }
                });
                ui.add_space(8.0);
            }
            if buffer.restored {
                ui.label(RichText::new("RESTORED FROM THE JOURNAL").color(QUIET));
                ui.add_space(8.0);
            }
            if let Some((line, why)) = &buffer.code.error {
                // Copied with the file's name, to stand alone when pasted.
                let (shown, copy) = if *line > 0 { (format!("LINE {line}: {why}"), format!("{path} line {line}: {why}")) } else { (why.clone(), format!("{path}: {why}")) };
                error_row(ui, &shown, &copy);
                ui.add_space(8.0);
            }
            // Where the game played stopped, if in this file: its line
            // marked until the game takes the file anew (the red line over the
            // main area says why).
            let trouble = self.stage.as_ref().and_then(|s| s.trouble.as_deref());
            let stopped = trouble.and_then(|t| crate::ide::play::line_of(t).filter(|(file, _)| *file == path).map(|(file, line)| (file, line, t)));
            buffer.code.stopped = stopped.as_ref().map(|(_, line, _)| *line);
            if let Some(message) = &self.message {
                error_row(ui, message, message);
                ui.add_space(8.0);
            }
            if put_back {
                match buffer.put_back() {
                    Ok(()) => self.message = None,
                    Err(e) => self.message = Some(format!("{e:#}")),
                }
            }
        }
        if put_back {
            self.rescan();
        }
        if want_find {
            self.open_find(ui.ctx(), false);
        }
        self.find_row(ui, id);
        let (mut used_rows, mut says) = (Vec::new(), Vec::new());
        if let Some(name) = &sound_name {
            self.refresh_used(name);
            used_rows = self.used.as_ref().map(|u| u.rows.clone()).unwrap_or_default();
            says = self.rules_say(name);
        }
        let (mut rects, mut go_used) = (Vec::new(), None);
        let mut scroll = egui::ScrollArea::both().id_salt(&path).auto_shrink([false, false]).content_margin(egui::Margin { right: 12, bottom: 14, ..egui::Margin::ZERO });
        if let Some(offset) = self.restore.take() {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let out = scroll.show(ui, |ui| {
            match self.shown.as_mut().expect("loaded above") {
            Shown::Code(buffer) => {
                if buffer.code.ui(ui, id) {
                    buffer.edited(std::time::Instant::now());
                }
                self.find.had_keys |= ui.memory(|m| m.has_focus(id));
                if let Some(range) = egui::TextEdit::load_state(ui.ctx(), id).and_then(|s| s.cursor.char_range()) {
                    self.memory.cursor.insert(path.clone(), range.primary.index.0);
                }
            }
            Shown::Text(text) => {
                let mut text = text.as_str();
                ui.add(egui::TextEdit::multiline(&mut text).code_editor().frame(egui::Frame::NONE).desired_width(f32::INFINITY));
            }
            Shown::Image(texture) => picture(ui, texture),
            Shown::Note(note) => {
                if !note.is_empty() {
                    ui.label(RichText::new(note.as_str()).color(QUIET));
                }
            }
            }
            if let Some(name) = &sound_name {
                go_used = used_ui(ui, name, &says, &used_rows, &mut rects);
            }
        });
        self.memory.scroll.insert(path, out.state.clamped_offset().y);
        #[cfg(test)]
        {
            self.used_rows = rects;
        }
        if let Some((file, line)) = go_used {
            self.hide_game();
            self.help = false;
            self.open(&file);
            self.goto = Some((line, true));
        }
    }

    /// Whether the text shown is saved: "SAVED" when the file holds what is
    /// shown, "NOT SAVED YET" between a change and the autosave (None: no
    /// text to change).
    pub(super) fn saved_word(&self) -> Option<&'static str> {
        match &self.shown {
            Some(Shown::Code(b)) if b.gone => Some("FILE GONE"),
            Some(Shown::Code(b)) => Some(if b.unsaved() { "NOT SAVED YET" } else { "SAVED" }),
            _ => None,
        }
    }
}

/// The word saying whether the text shown is saved, small and quiet.
/// Whether FIND beside it was pressed (the find row over the text).
pub(super) fn saved_label(ui: &mut egui::Ui, saved: Option<&str>) -> bool {
    let Some(word) = saved else { return false };
    // (Right to left: FIND shows right of the word, before RENAME.)
    let find = ui.add(egui::Button::new(RichText::new("FIND").small())).on_hover_text("Find and replace in this text · Ctrl+F, Ctrl+H").clicked();
    ui.label(RichText::new(word).small().color(QUIET)).on_hover_text("The text is saved by itself a second after the last change; Ctrl+S saves it at once.");
    find
}

/// The row over an open file: its path and a line of what it is (with its
/// own hover words, or the whole line when it is cut) take the room left of
/// the buttons, cut with "…" where there is none; the buttons keep theirs,
/// right to left from the edge, so nothing ever runs into them.
pub(super) fn header_row(ui: &mut egui::Ui, path: &str, facts: Option<(&str, &str)>, buttons: impl FnOnce(&mut egui::Ui)) {
    egui::Sides::new().spacing(14.0).shrink_left().truncate().show(
        ui,
        |ui| {
            ui.label(RichText::new(path).color(TEXT)).on_hover_text(path);
            if let Some((line, tip)) = facts {
                let line = line.to_uppercase();
                let hover = if tip.is_empty() { line.clone() } else { format!("{line}\n{tip}") };
                ui.label(RichText::new(line).color(QUIET)).on_hover_text(hover);
            }
        },
        |ui| {
            ui.add_space(16.0);
            buttons(ui);
        },
    );
}

/// A picture, its pixels as whole squares, as large as fit.
pub(super) fn picture(ui: &mut egui::Ui, texture: &egui::TextureHandle) {
    let size = texture.size_vec2();
    let room = ui.available_size();
    let scale = (room.x / size.x).min(room.y / size.y).floor().max(1.0);
    ui.image((texture.id(), size * scale));
}

/// A line of what a file an editor opens is (`level`: the game's level of
/// blocks).
/// Whether a file of the game is a model or a block: Voxcade's `.vvx`, or a
/// MagicaVoxel `.vox` (a world, a map's prefab, or a model not brought in
/// yet), shown as a picture.
pub fn is_model_file(path: &str) -> bool {
    path.ends_with(".vvx") || path.ends_with(".vox")
}

/// A line of what a level is: its size, its blocks and things (a level
/// of pieces: its things, places, areas).
pub(in crate::ide) fn level_line(l: &crate::grid::GridLevel) -> String {
    let plural = |n: usize, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    if !l.has_grid() {
        return format!("{} × {} × {} · {} · {} · {}", l.size.x, l.size.y, l.size.z, plural(l.things.len(), "thing"), plural(l.places.len(), "place"), plural(l.areas.len(), "area"));
    }
    format!("{} × {} × {} cells of {} · {} · {}", l.size.x, l.size.y, l.size.z, l.n, plural(l.blocks.len(), "block"), plural(l.things.len(), "thing"))
}

pub(super) fn facts(file: &Path, path: &str, level: bool) -> String {
    if level {
        return match crate::grid::GridLevel::read(file) {
            Ok(l) => level_line(&l),
            Err(e) => format!("{e:#}"),
        };
    }
    if !path.ends_with(".vvx") {
        return "MagicaVoxel file".into();
    }
    match crate::vox::load_model(file) {
        Ok(m) => model_line(&m, path),
        Err(e) => format!("{e:#}"),
    }
}

/// A line of what a world or a map's prefab is: how big, its colours and
/// markers, and that it is shown only.
pub(super) fn scene_facts(file: &Path, path: &str) -> String {
    match crate::scene::load(file) {
        Ok(s) => {
            let what = if path.starts_with("prefabs/") { "Prefab" } else { "World" };
            let markers = match s.markers.len() {
                0 => String::new(),
                n => format!(" · {n} marker{} ({})", if n == 1 { "" } else { "s" }, s.markers.join(", ")),
            };
            format!("{what} · {} × {} × {} · {} colors{markers}", s.size.x, s.size.y, s.size.z, s.colours.len())
        }
        Err(e) => format!("{e:#}"),
    }
}

/// A line of what a model (`path` in the game folder) is: a block, a
/// figure of parts or frames, and how big.
pub(in crate::ide) fn model_line(m: &crate::vox::VoxModel, path: &str) -> String {
    let plural = |n: usize, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    let s = m.size;
    match () {
        _ if path.starts_with("blocks/") => format!("Block · {} × {} × {}", s.x, s.y, s.z),
        _ if !m.parts.is_empty() => format!("Figure · {} × {} × {} · {}", s.x, s.y, s.z, plural(m.parts.len(), "part")),
        _ => format!("Model · {} × {} × {} · {}", s.x, s.y, s.z, plural(m.frames.len(), "frame")),
    }
}

/// A line of what the song in `file` is: its title, how long until it
/// starts over, its channels (or why it cannot be read).
pub(super) fn song_facts(file: &Path) -> String {
    let song = std::fs::read_to_string(file).map_err(anyhow::Error::from).and_then(|text| crate::audio::music::parse(&text));
    match song {
        Ok(song) => crate::ide::code::song_line(&song),
        Err(e) => format!("The song cannot be read: {e:#}"),
    }
}

/// The file at `file` (`path` in the game folder) as the main area shows
/// it.
pub(super) fn load(file: &Path, path: &str, ctx: &egui::Context) -> Shown {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    match ext {
        "png" => match crate::map::read_png_rgba(file) {
            Ok((w, h, pixels)) => {
                let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], pixels.as_flattened());
                Shown::Image(ctx.load_texture(path, image, egui::TextureOptions::NEAREST))
            }
            Err(e) => Shown::Note(format!("{e:#}")),
        },
        // What a model is stands above it (`facts`), and what a sound is.
        "vox" | "vvx" => Shown::Note(String::new()),
        _ => match std::fs::read(file) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Shown::Text(text),
                Err(_) => Shown::Note("Not text, and no picture of it yet.".into()),
            },
            Err(e) => Shown::Note(format!("{}: {e}", file.display())),
        },
    }
}

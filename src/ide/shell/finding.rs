//! Finding: the find row over the open text (Ctrl+F, Ctrl+H, F3) and the
//! search across the game (Ctrl+Shift+F, SEARCH), a line it found opened.

use super::*;

impl Shell {
    /// The keys of finding: Ctrl+F and Ctrl+H the find row over the open
    /// text, Ctrl+Shift+F the search across the game, F3 and Shift+F3 the
    /// next match and the one before, Escape the row or the search put
    /// away while it (or the text under the row) had the keys. Never while
    /// the game or the console has the keys.
    pub(super) fn find_keys(&mut self, ui: &mut egui::Ui) {
        if self.stage.as_ref().is_some_and(|s| s.shown) || ui.memory(|m| m.focused()) == Some(egui::Id::unique("terminal")) {
            return;
        }
        let ctx = ui.ctx().clone();
        let pressed = |modifiers, key| ui.input_mut(|i| i.consume_key(modifiers, key));
        let text = self.text_open();
        // (The more particular first: Ctrl+F is also in Ctrl+Shift+F.)
        if pressed(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::F) {
            let selected = self.selection(&ctx).0;
            self.search.open(selected.as_deref());
        } else if pressed(egui::Modifiers::COMMAND, egui::Key::F) && text {
            self.open_find(&ctx, false);
        } else if pressed(egui::Modifiers::COMMAND, egui::Key::H) && text {
            self.open_find(&ctx, true);
        }
        if self.find.shown && text {
            if pressed(egui::Modifiers::SHIFT, egui::Key::F3) {
                self.find.step(false);
            } else if pressed(egui::Modifiers::NONE, egui::Key::F3) {
                self.find.step(true);
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            let offering = matches!(&self.shown, Some(Shown::Code(b)) if b.code.offering());
            if self.search.shown && self.search.had_keys {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                self.search.shown = false;
                self.opening.focus_code = text;
            } else if self.find.shown && self.find.had_keys && !offering && text {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                self.find.close();
                self.opening.focus_code = true;
            }
        }
    }

    /// Whether the main area shows a text to change (not HELP, the game,
    /// a built-in sound or a picture from outside).
    pub(super) fn text_open(&self) -> bool {
        matches!(self.shown, Some(Shown::Code(_))) && !self.help && self.outside.is_none() && self.sound.builtin.is_none() && !self.stage.as_ref().is_some_and(|s| s.shown)
    }

    /// What the open text has selected, and where the selection (or the
    /// cursor) begins, as a byte of the text.
    pub(super) fn selection(&self, ctx: &egui::Context) -> (Option<String>, usize) {
        let (Some((_, id)), Some(Shown::Code(buffer))) = (self.code_id(), &self.shown) else { return (None, 0) };
        let Some(range) = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()) else { return (None, 0) };
        let [from, to] = [range.primary.index.0.min(range.secondary.index.0), range.primary.index.0.max(range.secondary.index.0)];
        let text = &buffer.code.text;
        let byte = |c: usize| text.char_indices().nth(c).map_or(text.len(), |(i, _)| i);
        let (from, to) = (byte(from), byte(to));
        ((to > from).then(|| text[from..to].to_string()), from)
    }

    /// The find row shown with the keys (`replacing`: its replace field
    /// too), the selection its word.
    pub fn open_find(&mut self, ctx: &egui::Context, replacing: bool) {
        let (selected, anchor) = self.selection(ctx);
        self.find.open(replacing, selected.as_deref(), anchor);
    }

    /// The find row over the text open (its text edit `id`), what it asks
    /// done: the next match or the one before, REPLACE and REPLACE ALL
    /// (each one step of the text's undo), CLOSE; then the matches marked
    /// in the text, the current one selected and in view when asked.
    pub(super) fn find_row(&mut self, ui: &mut egui::Ui, id: egui::Id) {
        let Some(Shown::Code(buffer)) = &mut self.shown else { return };
        if !self.find.shown {
            buffer.code.marking = None;
            return;
        }
        let ctx = ui.ctx().clone();
        self.find.update(&buffer.code.text);
        let act = self.find.row(ui);
        ui.add_space(8.0);
        // A replacement: the text before it a step of the undo, the text
        // after it (with its cursor, below) another.
        let mut changed = false;
        let mut change = |buffer: &mut Buffer, text: String| {
            buffer.code.mark(&ctx, id);
            buffer.code.text = text;
            buffer.code.check();
            changed = true;
        };
        match act {
            Some(crate::ide::find::Act::Next) => self.find.step(true),
            Some(crate::ide::find::Act::Previous) => self.find.step(false),
            Some(crate::ide::find::Act::Replace) => {
                if let Some(text) = self.find.replace(&buffer.code.text) {
                    change(buffer, text);
                }
            }
            Some(crate::ide::find::Act::ReplaceAll) => {
                if let Some((text, _)) = self.find.replace_all(&buffer.code.text) {
                    change(buffer, text);
                }
            }
            Some(crate::ide::find::Act::Close) => {
                self.find.close();
                buffer.code.marking = None;
                ui.memory_mut(|m| m.request_focus(id));
                return;
            }
            None => {}
        }
        self.find.update(&buffer.code.text);
        buffer.code.marking = (!self.find.word.is_empty()).then(|| (self.find.word.clone(), self.find.match_case, Some(self.find.current)));
        if std::mem::take(&mut self.find.reveal)
            && let Some(range) = self.find.current_range()
        {
            // The cursor after it, so the keys in the text go on from there
            // (selected while the text has the keys: egui keeps no
            // selection in a text without them).
            let text = &buffer.code.text;
            let (from, to) = (text[..range.start].chars().count(), text[..range.end].chars().count());
            let (from, to) = (egui::text::CCursor::new(from), egui::text::CCursor::new(to));
            let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            let keys = ctx.memory(|m| m.has_focus(id));
            state.cursor.set_char_range(Some(if keys { egui::text::CCursorRange::two(from, to) } else { egui::text::CCursorRange::one(to) }));
            state.store(&ctx, id);
            buffer.code.reveal = Some(range);
        }
        if changed {
            buffer.code.mark(&ctx, id);
            buffer.edited(std::time::Instant::now());
        }
    }

    /// The search across the game shown for `word`, its answer waited for
    /// and, if asked, its `hit`-th line (from 1) opened as a click does (a
    /// shot's `--search`, `--hit`).
    pub fn show_search(&mut self, word: &str, ctx: &egui::Context, hit: Option<usize>) {
        self.search.open(Some(word));
        self.search.wait(&self.dir, ctx);
        if let Some(found) = hit.and_then(|n| self.search.hits.get(n.saturating_sub(1))).cloned() {
            self.go_to_hit(&found.file, found.line);
        }
    }

    /// The find row shown over the open text for `word` (a shot's
    /// `--find`), with its replace field holding `with` (`--replace`).
    pub fn show_find(&mut self, word: &str, with: Option<&str>) {
        self.find.open(with.is_some(), Some(word), 0);
        self.find.with = with.unwrap_or("").to_string();
    }

    /// A line the project search found: its file open at it, the keys in
    /// the text, the word found there current in the find row.
    pub(super) fn go_to_hit(&mut self, file: &str, line: usize) {
        self.hide_game();
        self.help = false;
        self.open(file);
        // A model's or a level's line shows in its text.
        if (file.ends_with(".vvx") || self.is_level(file)) && !self.pictures.model_text {
            self.flush();
            self.model_as_text();
        }
        self.opening.goto = Some((line, true));
        self.find.word = self.search.word.clone();
        self.find.match_case = self.search.match_case;
        self.find.shown = true;
        self.find.line = Some(line);
    }
}

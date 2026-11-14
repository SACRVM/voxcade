//! The text editor in the main area: the file as text to change, the line
//! numbers beside it. Lua is coloured by the lexer's pieces
//! (`lua::lex::spans`, the same that make the replays' fingerprint); a
//! sound file (TOML) and a song (MML) get comments, strings and numbers
//! coloured. After every change the text is read without running it, by
//! the reader of its sort (Lua, `audio::sound::read`, `audio::music::parse`):
//! a line it cannot read is marked and named. After `world:`, `thing.`, `input.` or `out:` and after
//! `function` the API's names offer themselves (from its register,
//! `lua::api`), with what they take and do; the mouse over one shows that
//! too.

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Color32, FontId, Key, Modifiers, RichText, TextStyle};

use crate::audio::music;
use crate::audio::sound::{self, SoundDef};
use crate::lua::api::{self, Item, What};
use crate::lua::lex::{self, Kind};

/// The colours of a script's pieces: few and calm.
const NAME: Color32 = Color32::from_rgb(226, 218, 198);
const KEYWORD: Color32 = Color32::from_rgb(136, 178, 232);
const STRING: Color32 = Color32::from_rgb(152, 187, 108);
const NUMBER: Color32 = Color32::from_rgb(214, 150, 96);
const COMMENT: Color32 = Color32::from_rgb(132, 126, 118);
const SYMBOL: Color32 = Color32::from_rgb(178, 170, 156);
const LINE_NUMBER: Color32 = Color32::from_rgb(78, 74, 86);
const ERROR: Color32 = Color32::from_rgb(214, 104, 92);
const ERROR_LINE: Color32 = Color32::from_rgb(58, 30, 34);
const CHOSEN: Color32 = Color32::from_rgb(52, 48, 64);

/// The names offered where the cursor is: what is typed of the name (from
/// `from`, chars), and the register's items that begin so.
struct Offer {
    from: usize,
    items: Vec<&'static Item>,
    chosen: usize,
}

/// What a text is, by its file's ending: how it is coloured and read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Syntax {
    Lua,
    /// Any other text (a level's lines).
    Plain,
    /// A sound file, `sounds/NAME.toml`.
    Sound,
    /// A song, `.mml`.
    Song,
    /// A model, `.vvx` (TOML, as a sound).
    Model,
    /// A level (`levels/NAME.toml`, "voxcade level 1") or a map
    /// (`map.toml`, "voxcade map 1"), told by its format line: TOML, as
    /// a sound, read by the level's or the map's reader (a level's line of
    /// what it is in `model`, as a model's).
    Level,
    Map,
}

/// A text being edited.
pub struct Code {
    /// The text, and which state of it this is (`revision`): what is made
    /// from it keeps the number, not a copy.
    text: String,
    revision: u64,
    syntax: Syntax,
    /// The file's name, as its reader's messages name it.
    file: String,
    /// The line its reader cannot read (0: no line to name), and why.
    pub error: Option<(usize, String)>,
    /// The sound the text says, when it is a sound that reads.
    pub sound: Option<SoundDef>,
    /// A line of what the song is, when it is a song that reads.
    pub song: Option<String>,
    /// A line of what the model is, when it is a model that reads.
    pub model: Option<String>,
    /// The line the game played stopped at (a script's error while it
    /// ran), marked as the one Lua cannot read.
    pub stopped: Option<usize>,
    /// The revision of the text the error was looked for in.
    checked: Option<u64>,
    offer: Option<Offer>,
    /// Where the cursor stood when the offer was put away (Escape): not
    /// again there.
    declined: Option<usize>,
    /// The find row's word marked in the text (`find::mark`): the word,
    /// whether case counts, the current match's place among them.
    pub marking: Option<(String, bool, Option<usize>)>,
    /// A match (bytes of the text) to scroll into view, once.
    pub reveal: Option<std::ops::Range<usize>>,
    reader: mlua::Lua,
    /// The text laid out last and the line numbers (`Laid`): laid out
    /// anew only when what they show changed, not every frame.
    laid: Laid,
}

/// What `Code::ui` laid out last: the text as coloured and marked, and the
/// line numbers, each with what it was made from.
#[derive(Default)]
struct Laid {
    text: Option<(TextKey, std::sync::Arc<egui::Galley>)>,
    numbers: Option<(NumbersKey, std::sync::Arc<egui::Galley>)>,
}

/// What the text's layout is made from besides the text (which the
/// layout holds itself): the find row's word, the brackets marked, the
/// font and the screen's scale.
#[derive(Clone, PartialEq)]
struct TextKey {
    marking: Option<(String, bool, Option<usize>)>,
    brackets: Option<[usize; 2]>,
    font: FontId,
    scale: f32,
}

/// What the line numbers' layout is made from: how many lines, the ones
/// marked, the font and the scale.
#[derive(Clone, PartialEq)]
struct NumbersKey {
    lines: usize,
    marked: Vec<usize>,
    font: FontId,
    scale: f32,
}

/// A revision no text had before (`Code::revision`).
fn fresh_revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// How far a line goes in for each level (spaces), as the templates write.
pub const INDENT: &str = "  ";

/// The bracket at the cursor marked with its partner, behind them.
const BRACKET: Color32 = Color32::from_rgb(70, 66, 84);

impl Code {
    /// The text of `file` (its path in the game folder), Lua if it is.
    pub fn new(file: &str, text: String) -> Self {
        let syntax = match file.rsplit_once('.').map_or("", |(_, e)| e) {
            "lua" => Syntax::Lua,
            "toml" => match text.lines().find(|l| l.trim_start().starts_with("format")) {
                Some(l) if l.contains(crate::grid::FORMAT) => Syntax::Level,
                Some(l) if l.contains(crate::map::FORMAT) => Syntax::Map,
                _ => Syntax::Sound,
            },
            "mml" => Syntax::Song,
            "vvx" => Syntax::Model,
            _ => Syntax::Plain,
        };
        let mut code = Self { text, revision: fresh_revision(), syntax, file: file.to_string(), error: None, sound: None, song: None, model: None, stopped: None, checked: None, offer: None, declined: None, marking: None, reveal: None, reader: mlua::Lua::new(), laid: Laid::default() };
        code.check();
        code
    }

    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Which state of the text this is: a number no other text, nor this
    /// one before a change, ever had.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The text changed to `text` from outside the editor (the file
    /// changed on disk, the sound maker, a replacement).
    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.changed();
    }

    /// The text changed: a new revision.
    fn changed(&mut self) {
        self.revision = fresh_revision();
    }

    /// Looks for a line its reader cannot read (Lua, or the sandbox
    /// refuses), if the text changed since.
    pub fn check(&mut self) {
        if self.syntax == Syntax::Plain || self.checked == Some(self.revision) {
            return;
        }
        match self.syntax {
            Syntax::Lua => self.error = check(&self.reader, &self.file, &self.text),
            Syntax::Sound => match sound::read(&self.file, &self.text) {
                Ok(def) => (self.error, self.sound) = (None, Some(def)),
                Err(e) => (self.error, self.sound) = (Some(sound_trouble(&self.file, &format!("{e:#}"))), None),
            },
            Syntax::Model => match crate::vvx::read(&self.text) {
                Ok(m) => (self.error, self.model) = (None, Some(super::shell::model_line(&m, &self.file))),
                Err(e) => (self.error, self.model) = (Some(song_trouble(&format!("{e:#}"))), None),
            },
            Syntax::Level => match crate::grid::GridLevel::from_text(&self.text) {
                Ok(l) => (self.error, self.model) = (None, Some(super::shell::level_line(&l))),
                Err(e) => (self.error, self.model) = (Some(song_trouble(&format!("{e:#}"))), None),
            },
            Syntax::Map => self.error = crate::map::MapData::read(&self.text).err().map(|e| song_trouble(&format!("{e:#}"))),
            _ => match music::parse(&self.text) {
                Ok(song) => (self.error, self.song) = (None, Some(song_line(&song))),
                Err(e) => (self.error, self.song) = (Some(song_trouble(&format!("{e:#}"))), None),
            },
        }
        self.checked = Some(self.revision);
    }

    /// The editor in `ui` (its widget's `id`); whether the text changed.
    /// Enter keeps the line's indent (one more after a line that opens a
    /// block: `then`, `do`, `else`, `repeat`, `function …)`, `{`), Tab and
    /// Shift+Tab indent the lines chosen by two spaces or take two away,
    /// each one step of the text's undo; the bracket at the cursor is
    /// marked with its partner; the line numbers stay where they are when
    /// a long line is scrolled sideways.
    pub fn ui(&mut self, ui: &mut egui::Ui, id: egui::Id) -> bool {
        let font = TextStyle::Monospace.resolve(ui.style());
        let focused = ui.memory(|m| m.has_focus(id));
        let mut changed = false;

        // The offer's keys, before the text takes them.
        if focused && self.offer.is_some() {
            let pressed = |ui: &mut egui::Ui, key| ui.input_mut(|i| i.consume_key(Modifiers::NONE, key));
            let offer = self.offer.as_mut().expect("checked above");
            let count = offer.items.len();
            if pressed(ui, Key::ArrowDown) {
                offer.chosen = (offer.chosen + 1) % count;
            }
            if pressed(ui, Key::ArrowUp) {
                offer.chosen = (offer.chosen + count - 1) % count;
            }
            if pressed(ui, Key::Escape) {
                self.declined = cursor(ui.ctx(), id);
                self.offer = None;
            } else if pressed(ui, Key::Enter) || pressed(ui, Key::Tab) {
                let before = self.text.len();
                self.accept(ui.ctx(), id);
                changed |= self.text.len() != before;
            }
        }
        // Enter, Tab and Shift+Tab: the editor's own, before the text's.
        if focused && self.offer.is_none() {
            let key = ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::Enter) {
                    Some(Typed::Enter)
                } else if i.consume_key(Modifiers::SHIFT, Key::Tab) {
                    Some(Typed::Outdent)
                } else if i.consume_key(Modifiers::NONE, Key::Tab) {
                    Some(Typed::Indent)
                } else {
                    None
                }
            });
            if let Some(key) = key {
                changed |= self.typed(ui.ctx(), id, key);
            }
        }

        let syntax = self.syntax;
        let marking = self.marking.clone();
        // The bracket at the cursor and its partner (bytes), marked.
        let at = egui::TextEdit::load_state(ui.ctx(), id).and_then(|s| s.cursor.char_range()).filter(|r| r.is_empty() && focused).map(|r| r.primary.index.0);
        let brackets = at.and_then(|c| partner(&self.text, byte(&self.text, c), syntax));
        let scale = ui.ctx().pixels_per_point();
        let mut kept = std::mem::take(&mut self.laid.text);
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
            let key = TextKey { marking: marking.clone(), brackets: brackets.filter(|pair| pair.iter().all(|&i| text.as_str().as_bytes().get(i).is_some_and(|c| b"(){}[]".contains(c)))), font: font.clone(), scale };
            if let Some((k, galley)) = &kept
                && galley.job.text == text.as_str()
                && *k == key
            {
                return galley.clone();
            }
            let mut job = highlight(text.as_str(), &font, syntax);
            // (Found anew in the text laid out, which typing may have changed.)
            if let Some((word, case, current)) = &marking {
                super::find::mark(&mut job, text.as_str(), word, *case, *current);
            }
            if let Some(pair) = key.brackets {
                behind(&mut job, &pair, BRACKET);
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            kept = Some((key, galley.clone()));
            galley
        };
        let lines = self.text.split('\n').count();
        let digits = lines.to_string().len().max(3);
        let marks: Vec<usize> = self.error.iter().map(|(l, _)| *l).chain(self.stopped).filter(|&l| l >= 1 && l <= lines).collect();
        // The line numbers, the one Lua cannot read (or the game stopped
        // at) in red; laid out anew only when they change.
        let numbers_key = NumbersKey { lines, marked: marks.clone(), font: font.clone(), scale };
        let numbers = match self.laid.numbers.take() {
            Some((k, g)) if k == numbers_key => g,
            _ => {
                let mut job = LayoutJob::default();
                for n in 1..=lines {
                    let colour = if marks.contains(&n) { ERROR } else { LINE_NUMBER };
                    job.append(&format!("{n:>digits$}\n"), 0.0, TextFormat { font_id: font.clone(), color: colour, ..Default::default() });
                }
                ui.fonts_mut(|f| f.layout_job(job))
            }
        };
        // (A row as the numbers' own layout has it: the font's row height
        // alone drifts from the text's after a hundred lines.)
        let row = numbers.size().y / (lines + 1) as f32;
        let gutter = numbers.size().x + 14.0;
        let top_left = ui.cursor().min;
        // Its line marked behind the text.
        for &line in &marks {
            let top = top_left.y + (line - 1) as f32 * row;
            let rect = egui::Rect::from_min_size(egui::pos2(top_left.x + gutter - 4.0, top), egui::vec2(ui.available_width() - gutter + 4.0, row));
            ui.painter().rect_filled(rect, 2.0, ERROR_LINE);
        }
        let mut out = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(gutter);
            out = Some(
                egui::TextEdit::multiline(&mut self.text)
                    .id(id)
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .desired_width(f32::INFINITY)
                    .lock_focus(true)
                    .layouter(&mut layouter)
                    .show(ui),
            );
        });
        let out = out.expect("shown above");
        self.laid.text = kept;
        // The numbers where the view begins, over what scrolled under them
        // sideways: they stay in place.
        let seen = ui.clip_rect();
        let x = top_left.x.max(seen.left());
        let column = egui::Rect::from_min_max(egui::pos2(x - 1.0, seen.top()), egui::pos2(x + gutter - 6.0, seen.bottom()));
        ui.painter().rect_filled(column, 0.0, crate::gui::GROUND);
        ui.painter().galley(egui::pos2(x, top_left.y), numbers.clone(), LINE_NUMBER);
        self.laid.numbers = Some((numbers_key, numbers));
        if out.response.changed() {
            self.changed();
            changed = true;
        }
        self.check();

        // The find row's current match in view.
        if let Some(range) = self.reveal.take().filter(|r| r.end <= self.text.len() && self.text.is_char_boundary(r.start) && self.text.is_char_boundary(r.end)) {
            let (from, to) = (self.text[..range.start].chars().count(), self.text[..range.end].chars().count());
            let at = |c| out.galley.pos_from_cursor(CCursor::new(c)).translate(out.galley_pos.to_vec2());
            let found = at(from).union(at(to)).expand(24.0);
            // Down to its line in the middle; sideways only if it is out of
            // sight (the line numbers stay in view).
            let seen = ui.clip_rect();
            let across = if seen.x_range().contains(found.left()) && seen.x_range().contains(found.right()) { seen.x_range() } else { found.x_range() };
            ui.scroll_to_rect(egui::Rect::from_x_y_ranges(across, found.y_range()), Some(egui::Align::Center));
        }

        // What to offer where the cursor is now.
        let at = out.cursor_range.map(|r| r.primary.index.0);
        if !focused || at != self.declined {
            self.declined = None;
        }
        let offer = at.filter(|_| focused && self.declined.is_none() && self.syntax == Syntax::Lua).and_then(|at| offer(&self.text, at));
        self.offer = match (offer, self.offer.take()) {
            // The same names: the one chosen stays chosen.
            (Some(new), Some(old)) if new.from == old.from && new.items.len() == old.items.len() => Some(Offer { chosen: old.chosen, ..new }),
            (new, _) => new,
        };
        if let (Some(offer), Some(at)) = (&self.offer, at) {
            let place = out.galley.pos_from_cursor(CCursor::new(at)).translate(out.galley_pos.to_vec2());
            show_offer(ui, offer, place.left_bottom(), &font);
        }

        // The mouse over a name of the API: what it is.
        if let Some(pointer) = out.response.hover_pos().filter(|_| self.syntax == Syntax::Lua) {
            let at = out.galley.cursor_from_pos(pointer - out.galley_pos).index.0;
            if let Some(item) = item_at(&self.text, at) {
                out.response.response.clone().on_hover_ui_at_pointer(|ui| help(ui, item, &font));
            }
        }
        changed
    }

    /// A step for the text's undo (the text edit `id`'s own, which Ctrl+Z
    /// goes back over): the text as it stands now. A change made from
    /// outside the text (the sound maker) is marked before and after, so
    /// it is one step back, as typing is.
    pub fn mark(&self, ctx: &egui::Context, id: egui::Id) {
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let at = here(&state, &self.text);
        let mut undoer = state.undoer();
        undoer.add_undo(&(at, self.text.clone()));
        state.set_undoer(undoer);
        state.cursor.set_char_range(Some(at));
        state.store(ctx, id);
    }

    /// Whether the API's names are offered at the cursor (Escape is theirs).
    pub fn offering(&self) -> bool {
        self.offer.is_some()
    }

    /// Whether there is a step to go back to.
    pub fn can_go_back(&self, ctx: &egui::Context, id: egui::Id) -> bool {
        egui::TextEdit::load_state(ctx, id).is_some_and(|s| s.undoer().has_undo(&(here(&s, &self.text), self.text.clone())))
    }

    /// Goes back a step, as Ctrl+Z in the text does; whether there was one.
    pub fn go_back(&mut self, ctx: &egui::Context, id: egui::Id) -> bool {
        let Some(mut state) = egui::TextEdit::load_state(ctx, id) else { return false };
        let mut undoer = state.undoer();
        let Some((at, text)) = undoer.undo(&(here(&state, &self.text), self.text.clone())).cloned() else { return false };
        state.set_undoer(undoer);
        state.cursor.set_char_range(Some(at));
        state.store(ctx, id);
        self.set_text(text);
        self.check();
        true
    }

    /// Enter, Tab or Shift+Tab done to the text (the text edit `id`'s
    /// cursor and selection), as one step of its undo; whether the text
    /// changed.
    fn typed(&mut self, ctx: &egui::Context, id: egui::Id, key: Typed) -> bool {
        let Some(state) = egui::TextEdit::load_state(ctx, id) else { return false };
        let range = here(&state, &self.text);
        let (from, to) = {
            let (a, b) = (range.primary.index.0, range.secondary.index.0);
            (a.min(b), a.max(b))
        };
        let (text, cursor) = match key {
            Typed::Enter => enter(&self.text, from, to, self.syntax),
            Typed::Indent | Typed::Outdent => indent(&self.text, from, to, key == Typed::Indent),
        };
        if text == self.text {
            return false;
        }
        self.mark(ctx, id);
        self.set_text(text);
        if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
            state.cursor.set_char_range(Some(cursor));
            state.store(ctx, id);
        }
        self.mark(ctx, id);
        true
    }

    /// Takes the name chosen: what is typed of it becomes it (a method's
    /// or function's with its opening bracket, a callback's with its
    /// parameters), the cursor after it.
    fn accept(&mut self, ctx: &egui::Context, id: egui::Id) {
        let Some(offer) = self.offer.take() else { return };
        let Some(at) = cursor(ctx, id) else { return };
        let item = offer.items[offer.chosen];
        let insert = match &item.what {
            What::Field { .. } => item.name.to_string(),
            What::Method { .. } | What::Function { .. } => format!("{}(", item.name),
            What::Callback { params, .. } => format!("{}({})", item.name, params.iter().map(|(n, _)| n.trim_end_matches('?')).collect::<Vec<_>>().join(", ")),
        };
        let (from, to) = (byte(&self.text, offer.from), byte(&self.text, at));
        self.text.replace_range(from..to, &insert);
        self.changed();
        let end = offer.from + insert.chars().count();
        if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
            state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(end))));
            state.store(ctx, id);
        }
    }
}

/// A key the editor takes before the text does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Typed {
    Enter,
    Indent,
    Outdent,
}

/// The chars `from`..`to` of `text` (the selection) replaced by a new
/// line indented as the line it is on, one level more after a line that
/// opens a block (Lua: `then`, `do`, `else`, `repeat`, a `function`'s
/// head, an open `{` or `(`; in any text an open bracket); the text and
/// the cursor after it.
fn enter(text: &str, from: usize, to: usize, syntax: Syntax) -> (String, CCursorRange) {
    let (a, b) = (byte(text, from), byte(text, to));
    let start = text[..a].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[start..a];
    let mut indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    if opens(line, syntax) {
        indent.push_str(INDENT);
    }
    let insert = format!("\n{indent}");
    let mut out = String::with_capacity(text.len() + insert.len());
    out.push_str(&text[..a]);
    out.push_str(&insert);
    out.push_str(&text[b..]);
    let at = from + insert.chars().count();
    (out, CCursorRange::one(CCursor::new(at)))
}

/// Whether a line (up to the cursor) opens a block, so the next goes one
/// level further in.
fn opens(line: &str, syntax: Syntax) -> bool {
    let code = match syntax {
        // Without its comment.
        Syntax::Lua => lex::spans(line).into_iter().rfind(|s| s.kind != Kind::Comment).map_or("", |s| &line[..s.range.end]),
        _ => line,
    };
    let code = code.trim_end();
    if code.ends_with(['{', '(', '[']) {
        return true;
    }
    if syntax != Syntax::Lua {
        return false;
    }
    let last = code.rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).next().unwrap_or("");
    if matches!(last, "then" | "do" | "else" | "repeat") {
        return true;
    }
    // `function name(…)` or `= function(…)` whose head ends the line.
    code.ends_with(')') && code.split_whitespace().any(|w| w == "function" || w.starts_with("function(")) && !code.split_whitespace().any(|w| w == "end")
}

/// The lines the chars `from`..`to` touch indented by `INDENT` (`more`)
/// or with up to that much indent taken away; the text and the selection
/// after it, the same text chosen.
fn indent(text: &str, from: usize, to: usize, more: bool) -> (String, CCursorRange) {
    let (a, b) = (byte(text, from), byte(text, to));
    let first = text[..a].rfind('\n').map_or(0, |i| i + 1);
    // A selection ending at a line's start leaves that line be.
    let end = if b > a && text[..b].ends_with('\n') { b - 1 } else { b };
    let last_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
    let mut out = String::with_capacity(text.len() + 64);
    out.push_str(&text[..first]);
    let (mut moved_from, mut moved_to) = (0i64, 0i64);
    let mut at = first;
    for (k, line) in text[first..last_end].split('\n').enumerate() {
        if k > 0 {
            out.push('\n');
        }
        let shift = if more {
            out.push_str(INDENT);
            out.push_str(line);
            INDENT.len() as i64
        } else {
            let gone = line.chars().take(INDENT.len()).take_while(|c| *c == ' ').count();
            let gone = if gone == 0 && line.starts_with('\t') { 1 } else { gone };
            out.push_str(&line[gone..]);
            -(gone as i64)
        };
        // How the cursor's ends move: by the lines before them and their
        // own (never past their line's start).
        let line_start = at;
        if line_start <= a {
            moved_from += if more { shift } else { shift.max(-((a - line_start) as i64)) };
        }
        if line_start <= b {
            moved_to += if more { shift } else { shift.max(-((b - line_start) as i64)) };
        }
        at += line.len() + 1;
    }
    out.push_str(&text[last_end..]);
    let chars = |byte_at: i64| out[..(byte_at.max(0) as usize).min(out.len())].chars().count();
    let (na, nb) = (chars(a as i64 + moved_from), chars(b as i64 + moved_to));
    let range = if from == to { CCursorRange::one(CCursor::new(na)) } else { CCursorRange::two(CCursor::new(na), CCursor::new(nb)) };
    (out, range)
}

/// The bracket at byte `at` of `text` (the one after the cursor, else the
/// one before it) and its partner, bytes; in Lua only the brackets of the
/// code, not those in strings and comments.
fn partner(text: &str, at: usize, syntax: Syntax) -> Option<[usize; 2]> {
    // The brackets that count, in order (bytes, the sign).
    let signs: Vec<(usize, u8)> = match syntax {
        Syntax::Lua => lex::spans(text).into_iter().filter(|s| s.kind == Kind::Symbol).flat_map(|s| s.range.clone().filter(|&i| b"(){}[]".contains(&text.as_bytes()[i])).collect::<Vec<_>>()).map(|i| (i, text.as_bytes()[i])).collect(),
        _ => text.bytes().enumerate().filter(|(_, c)| b"(){}[]".contains(c)).collect(),
    };
    let k = signs.iter().position(|&(i, _)| i == at).or_else(|| signs.iter().position(|&(i, _)| at > 0 && i == at - 1))?;
    let (open, close) = match signs[k].1 {
        b'(' | b')' => (b'(', b')'),
        b'{' | b'}' => (b'{', b'}'),
        _ => (b'[', b']'),
    };
    let mut depth = 0i32;
    if signs[k].1 == open {
        for &(i, c) in &signs[k..] {
            depth += (c == open) as i32 - (c == close) as i32;
            if depth == 0 {
                return Some([signs[k].0, i]);
            }
        }
    } else {
        for &(i, c) in signs[..=k].iter().rev() {
            depth += (c == close) as i32 - (c == open) as i32;
            if depth == 0 {
                return Some([i, signs[k].0]);
            }
        }
    }
    None
}

/// The chars at bytes `at` of a laid-out text given a `ground` behind.
fn behind(job: &mut LayoutJob, at: &[usize], ground: Color32) {
    let mut sections = Vec::with_capacity(job.sections.len() + 4);
    for section in job.sections.drain(..) {
        let (start, end) = (section.byte_range.start.0, section.byte_range.end.0);
        let mut cuts: Vec<usize> = at.iter().flat_map(|&i| [i, i + 1]).filter(|&c| c > start && c < end).collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut from = start;
        for to in cuts.into_iter().chain(std::iter::once(end)) {
            let mut format = section.format.clone();
            if at.contains(&from) {
                format.background = ground;
            }
            sections.push(egui::text::LayoutSection { leading_space: if from == start { section.leading_space } else { 0.0 }, byte_range: egui::text::ByteIndex(from)..egui::text::ByteIndex(to), format });
            from = to;
        }
    }
    job.sections = sections;
}

/// Where the cursor of the text edit `id` stands (chars).
fn cursor(ctx: &egui::Context, id: egui::Id) -> Option<usize> {
    egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0)
}

/// The cursor of a text edit's `state` inside `text`, as the text edit
/// itself feeds its undo (at the start if it has none).
fn here(state: &egui::text_edit::TextEditState, text: &str) -> CCursorRange {
    let count = text.chars().count();
    let mut range = state.cursor.char_range().unwrap_or(CCursorRange::one(CCursor::new(0)));
    for end in [&mut range.primary, &mut range.secondary] {
        if end.index.0 > count {
            *end = CCursor::new(count);
        }
    }
    range
}

/// The byte where char `at` of `text` begins.
fn byte(text: &str, at: usize) -> usize {
    text.char_indices().nth(at).map_or(text.len(), |(i, _)| i)
}

/// The text coloured by its pieces (Lua's, a sound's or a song's, or plain).
pub(super) fn highlight(text: &str, font: &FontId, syntax: Syntax) -> LayoutJob {
    let mut job = LayoutJob::default();
    let format = |color| TextFormat { font_id: font.clone(), color, ..Default::default() };
    match syntax {
        Syntax::Lua => {}
        Syntax::Plain => {
            job.append(text, 0.0, format(NAME));
            return job;
        }
        Syntax::Sound | Syntax::Song | Syntax::Model | Syntax::Level | Syntax::Map => {
            for (piece, colour) in plain_pieces(text, syntax) {
                job.append(piece, 0.0, format(colour));
            }
            return job;
        }
    }
    let mut done = 0;
    for span in lex::spans(text) {
        if span.range.start > done {
            job.append(&text[done..span.range.start], 0.0, format(NAME));
        }
        let colour = match span.kind {
            Kind::Comment => COMMENT,
            Kind::String => STRING,
            Kind::Number => NUMBER,
            Kind::Keyword => KEYWORD,
            Kind::Name => NAME,
            Kind::Symbol => SYMBOL,
        };
        job.append(&text[span.range.clone()], 0.0, format(colour));
        done = span.range.end;
    }
    job.append(&text[done..], 0.0, format(NAME));
    job
}

/// A sound's or a song's text in coloured pieces: comments (`#` in a
/// sound, `;` in a song), strings, numbers and, in a song, the words that
/// begin a line (`#tempo`, `@lead`, `$riff`).
fn plain_pieces(text: &str, syntax: Syntax) -> Vec<(&str, Color32)> {
    let comment = if syntax == Syntax::Song { ';' } else { '#' };
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        let mut at = 0;
        let mut push = |to: usize, colour: Color32, at: &mut usize| {
            if to > *at {
                out.push((&line[*at..to], colour));
                *at = to;
            }
        };
        // A song's directive, instrument or macro: its first word.
        if syntax == Syntax::Song && line.starts_with(['#', '@', '$']) {
            let end = line.find(char::is_whitespace).unwrap_or(line.len());
            push(end, KEYWORD, &mut at);
        }
        for (i, c) in line.char_indices() {
            if i < at {
                continue;
            }
            if c == comment {
                push(i, NAME, &mut at);
                push(line.len(), COMMENT, &mut at);
                break;
            }
            if c == '"' && syntax != Syntax::Song {
                push(i, NAME, &mut at);
                let end = line[i + 1..].find('"').map_or(line.len(), |e| i + 2 + e);
                push(end, STRING, &mut at);
            } else if c.is_ascii_digit() && !line[..i].ends_with(|p: char| p.is_ascii_alphanumeric() || p == '_') {
                // A number: digits and a decimal point (a minus belongs to it).
                push(if line[..i].ends_with('-') { i - 1 } else { i }, NAME, &mut at);
                let end = line[i..].find(|n: char| !(n.is_ascii_digit() || n == '.')).map_or(line.len(), |e| i + e);
                push(end, NUMBER, &mut at);
            }
        }
        push(line.len(), NAME, &mut at);
    }
    out
}

/// A sound's mistake as `sound::read` words it ("sounds/coin.toml line 3:
/// why"): the line, and the why without the file's name.
fn sound_trouble(file: &str, message: &str) -> (usize, String) {
    let rest = message.strip_prefix(file).unwrap_or(message);
    if let Some(after) = rest.strip_prefix(" line ")
        && let Some((n, why)) = after.split_once(": ")
        && let Ok(line) = n.parse::<usize>()
    {
        return (line, why.trim().to_string());
    }
    (0, rest.trim_start_matches(": ").trim().to_string())
}

/// A song's mistake as `music::parse` words it: "line 3: why" for
/// directives, instruments and macros, "channel A (from line 5), near
/// ...: why" inside a channel (its first line).
fn song_trouble(message: &str) -> (usize, String) {
    if let Some(after) = message.strip_prefix("line ")
        && let Some((n, why)) = after.split_once(": ")
        && let Ok(line) = n.parse::<usize>()
    {
        return (line, why.trim().to_string());
    }
    let line = message.split_once("(from line ").and_then(|(_, r)| r.split_once(')')).and_then(|(n, _)| n.parse().ok()).unwrap_or(0);
    (line, message.to_string())
}

/// A line of what a song is: its title, how long until it starts over,
/// its channels.
pub(super) fn song_line(song: &music::Song) -> String {
    let seconds = (song.length / crate::audio::SAMPLE_RATE as u64) as u32;
    let title = if song.title.is_empty() { String::new() } else { format!("{} · ", song.title) };
    let ending = if song.looping { "loops" } else { "once" };
    format!("{title}{}:{:02} · {} channels · {ending}", seconds / 60, seconds % 60, song.names.len())
}

/// The first line of `text` (the file `file`) Lua cannot read or the
/// sandbox refuses (`^`), and why.
fn check(reader: &mlua::Lua, file: &str, text: &str) -> Option<(usize, String)> {
    let message = match lex::tokens(file, text) {
        Err(e) => e.to_string(),
        Ok(_) => match reader.load(text).set_name(format!("@{file}")).into_function() {
            Ok(_) => return None,
            Err(mlua::Error::SyntaxError { message, .. }) => message,
            Err(e) => e.to_string(),
        },
    };
    // "main.lua:12: 'end' expected near <eof>"
    let rest = message.strip_prefix(&format!("{file}:"))?;
    let (line, why) = rest.split_once(':')?;
    Some((line.trim().parse().ok()?, why.trim().to_string()))
}

/// What to offer with the cursor at char `at` of `text`: after `level:`
/// the level's methods, after `level.` its fields (the same for the other
/// objects, `api::OWNERS`), after `function` at a line's start the
/// handlers; only those that begin with what is typed, and nothing once a
/// name is whole.
fn offer(text: &str, at: usize) -> Option<Offer> {
    let head: String = text.chars().take(at).collect();
    let line = head.rsplit('\n').next().unwrap_or("");
    let typed: String = line.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect::<Vec<_>>().into_iter().rev().collect();
    let before = &line[..line.len() - typed.len()];
    let fits = |item: &&Item| item.name.starts_with(typed.as_str()) && item.name != typed;
    let items: Vec<&'static Item> = if before.trim_start() == "function " || before.trim_start().ends_with(" function ") && before.trim_start().starts_with("function") {
        api::API.iter().filter(|i| matches!(i.what, What::Callback { .. })).filter(fits).collect()
    } else {
        let (owner, sep) = api::OWNERS.iter().find_map(|o| [':', '.'].iter().find(|&&s| before.ends_with(&format!("{o}{s}")) && !word_before(before, o.len() + 1)).map(|&s| (*o, s)))?;
        api::items_of(owner).filter(|i| matches!((&i.what, sep), (What::Method { .. }, ':') | (What::Field { .. }, '.'))).filter(fits).collect()
    };
    (!items.is_empty()).then(|| Offer { from: at - typed.chars().count(), items, chosen: 0 })
}

/// Whether the text before the last `n` bytes of `before` ends in a name's
/// letter (so `myworld:` is not `world:`).
fn word_before(before: &str, n: usize) -> bool {
    before[..before.len() - n].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The API's item whose name is at char `at` of `text` (`level:spawn`,
/// `thing.state`, a handler after `function`, `read`).
fn item_at(text: &str, at: usize) -> Option<&'static Item> {
    let chars: Vec<char> = text.chars().collect();
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if !chars.get(at).is_some_and(|&c| is_name(c)) {
        return None;
    }
    let (mut from, mut to) = (at, at);
    while from > 0 && is_name(chars[from - 1]) {
        from -= 1;
    }
    while to < chars.len() && is_name(chars[to]) {
        to += 1;
    }
    let name: String = chars[from..to].iter().collect();
    let before: String = chars[..from].iter().collect();
    let owner = api::OWNERS.iter().find(|o| (before.ends_with(&format!("{o}:")) || before.ends_with(&format!("{o}."))) && !word_before(&before, o.len() + 1));
    match owner {
        Some(o) => api::items_of(o).find(|i| i.name == name),
        None => api::API.iter().find(|i| i.of.is_empty() && i.name == name),
    }
}

/// The names offered, under the cursor: each with what it takes, the
/// chosen one's help below them.
fn show_offer(ui: &egui::Ui, offer: &Offer, at: egui::Pos2, font: &FontId) {
    egui::Area::new(egui::Id::unique("offer")).order(egui::Order::Foreground).fixed_pos(at + egui::vec2(0.0, 4.0)).show(ui.ctx(), |ui| {
        egui::Frame::new().fill(Color32::from_rgb(31, 29, 38)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(58, 54, 68))).corner_radius(4).inner_margin(8).show(ui, |ui| {
            ui.set_max_width(560.0);
            for (i, item) in offer.items.iter().enumerate().take(12) {
                let text = RichText::new(api::signature(item)).font(font.clone()).color(if i == offer.chosen { NAME } else { SYMBOL });
                egui::Frame::new().fill(if i == offer.chosen { CHOSEN } else { Color32::TRANSPARENT }).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                    ui.label(text);
                });
            }
            ui.add_space(4.0);
            ui.label(RichText::new(offer.items[offer.chosen].doc).small().color(COMMENT));
        });
    });
}

/// What an item of the API is: its whole name and what it takes, and what
/// it does.
fn help(ui: &mut egui::Ui, item: &Item, font: &FontId) {
    ui.set_max_width(520.0);
    ui.label(RichText::new(api::signature(item)).font(font.clone()).color(NAME));
    ui.label(RichText::new(item.doc).small().color(COMMENT));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<&'static str> {
        offer(text, text.chars().count()).map(|o| o.items.iter().map(|i| i.name).collect()).unwrap_or_default()
    }

    /// After `level:` the level's methods are offered, only those that
    /// begin as typed; after `world.` its fields; after `level.camera:`
    /// the camera's; after `player` a thing's and its own; after `function`
    /// the handlers; nothing after another
    /// name, nothing once a name is whole.
    #[test]
    fn completion_offers_the_api_after_world_colon() {
        let methods = names("local t = level:");
        assert!(methods.contains(&"spawn") && methods.contains(&"things") && !methods.contains(&"size"), "{methods:?}");
        assert_eq!(names("level:sp"), ["spawn"]);
        assert!(names("world.").contains(&"tick") && !names("world.").contains(&"win"));
        assert_eq!(names("level.camera:f"), ["follow"]);
        assert!(names("level.").contains(&"camera"));
        assert_eq!(names("player.st"), ["state"]);
        assert!(names("player:").contains(&"walk") && names("player.").contains(&"x"), "the player is a thing");
        assert!(item_at("player.state", 8).is_some_and(|i| i.of == "player"), "its own state");
        assert!(names("function ").contains(&"on_tick") && names("function on_d").contains(&"on_draw"));
        assert!(names("mylevel:").is_empty(), "another name");
        assert!(names("level:spawn").is_empty(), "whole");
        assert_eq!(item_at("x = level:things('gem')", 12).map(|i| i.name), Some("things"));
        assert!(item_at("x = things", 6).is_none(), "not the level's without level:");
    }

    /// A line Lua cannot read is named, and the sandbox's `^` too.
    #[test]
    fn a_line_lua_cannot_read_is_marked() {
        let reader = mlua::Lua::new();
        assert_eq!(check(&reader, "main.lua", "function on_tick()\n  local x = 1\n"), Some((3, "'end' expected (to close 'function' at line 1) near <eof>".into())));
        assert_eq!(check(&reader, "main.lua", "local a = 1\nlocal b = a ^ 3\n").map(|(l, _)| l), Some(2));
        assert_eq!(check(&reader, "main.lua", "function on_tick() end"), None);
    }

    /// A mistake in a sound or a song is named by its line, without the
    /// file's name; one the reader gives no line to is named without one.
    #[test]
    fn a_sound_or_song_mistake_names_its_line() {
        let mut code = Code::new("sounds/coin.toml", "wave = \"square\"\nfreq = 880\nhold = fast\n".into());
        assert!(code.sound.is_none() && matches!(&code.error, Some((3, _))), "{:?}", code.error);
        code.set_text("wave = \"square\"\nfrq = 880\n".into());
        code.check();
        assert!(code.error.as_ref().is_some_and(|(l, w)| *l == 2 && w.starts_with("frq: no such field") && !w.contains("coin.toml")), "{:?}", code.error);
        code.set_text("# a coin\nwave = \"sine\"\n".into());
        code.check();
        assert!(code.error.is_none() && code.sound.is_some());
        let mut song = Code::new("music/t.mml", "#tempo fast\nA: c d e\n".into());
        assert!(matches!(&song.error, Some((1, w)) if w.contains("tempo") || !w.is_empty()), "{:?}", song.error);
        song.set_text("#tempo 100\n\nA: c d e\nB: c d zz\n".into());
        song.check();
        assert!(matches!(&song.error, Some((4, _))), "inside a channel: its first line: {:?}", song.error);
        song.set_text("#title t\nA: c d e\n".into());
        song.check();
        assert!(song.error.is_none() && song.song.as_deref().is_some_and(|l| l.starts_with("t · ")));
    }

    /// Comments, strings and numbers of a sound and a song are coloured,
    /// the pieces together are the text.
    #[test]
    fn sound_and_song_pieces_keep_the_text() {
        let sound = "# a coin\nwave = \"sine\" # here\nfreq = 880.5\nsweep = -12\n";
        let pieces = plain_pieces(sound, Syntax::Sound);
        assert_eq!(pieces.iter().map(|(p, _)| *p).collect::<String>(), sound);
        assert!(pieces.contains(&("# a coin\n", COMMENT)) && pieces.contains(&("\"sine\"", STRING)) && pieces.contains(&("880.5", NUMBER)) && pieces.contains(&("-12", NUMBER)));
        let song = "#tempo 110 ; fast\nA: c4 e8 ; tune\n";
        let pieces = plain_pieces(song, Syntax::Song);
        assert_eq!(pieces.iter().map(|(p, _)| *p).collect::<String>(), song);
        assert!(pieces.contains(&("#tempo", KEYWORD)) && pieces.contains(&("; tune\n", COMMENT)));
    }

    /// Enter keeps the line's indent, one level more after a line that
    /// opens a block (not after one that closes it on the same line, nor
    /// for a keyword in a comment); the selection goes.
    #[test]
    fn enter_keeps_the_indent_and_goes_in_after_a_block_opens() {
        let next = |text: &str| {
            let at = text.chars().count();
            let (out, cursor) = enter(text, at, at, Syntax::Lua);
            assert_eq!(cursor.primary.index.0, out.chars().count());
            out[text.len() + 1..].to_string()
        };
        assert_eq!(next("  local x = 1"), "  ");
        assert_eq!(next("function on_tick()"), "  ");
        assert_eq!(next("  if p.hp < 1 then"), "    ");
        assert_eq!(next("    for _, p in ipairs(level:players()) do"), "      ");
        assert_eq!(next("  else"), "    ");
        assert_eq!(next("repeat"), "  ");
        assert_eq!(next("local t = {"), "  ");
        assert_eq!(next("local f = function(a, b)"), "  ");
        assert_eq!(next("  if a then b() end"), "  ", "closed on its line");
        assert_eq!(next("  x = 1 -- and then"), "  ", "a word in a comment");
        assert_eq!(next("  print(\"then\")"), "  ");
        // A selection is replaced.
        let (out, _) = enter("ab  cd", 2, 4, Syntax::Lua);
        assert_eq!(out, "ab\ncd");
        // A sound's text: brackets only.
        let (out, _) = enter("  a = [", 7, 7, Syntax::Sound);
        assert_eq!(out, "  a = [\n    ");
    }

    /// Tab indents every line the selection touches by two spaces, Shift
    /// and Tab take up to two away (a line with less loses what it has),
    /// and the same text stays chosen; without a selection the cursor's
    /// line.
    #[test]
    fn tab_and_shift_tab_indent_the_lines_chosen() {
        let text = "a\n  b\n c\nd";
        // From "a" to "c": three lines.
        let (out, sel) = indent(text, 0, 7, true);
        assert_eq!(out, "  a\n    b\n   c\nd");
        assert_eq!((sel.secondary.index.0, sel.primary.index.0), (2, 13));
        let (back, _) = indent(&out, 0, 13, false);
        assert_eq!(back, text);
        // Less than two: what it has.
        let (out, cursor) = indent(text, 7, 7, false);
        assert_eq!(out, "a\n  b\nc\nd");
        assert_eq!(cursor.primary.index.0, 6);
        // A selection ending at a line's start leaves that line.
        let (out, _) = indent("a\nb\nc", 0, 4, true);
        assert_eq!(out, "  a\n  b\nc");
    }

    /// The bracket after the cursor (else before it) is marked with its
    /// partner, over nested ones; in Lua not one in a string or comment.
    #[test]
    fn the_bracket_at_the_cursor_finds_its_partner() {
        let text = "f(a, g(b), \"(\") -- )";
        assert_eq!(partner(text, 1, Syntax::Lua), Some([1, 14]));
        assert_eq!(partner(text, 15, Syntax::Lua), Some([1, 14]), "the one left of the cursor");
        assert_eq!(partner(text, 6, Syntax::Lua), Some([6, 8]));
        assert_eq!(partner(text, 3, Syntax::Lua), None);
        assert_eq!(partner("(a + (b)", 0, Syntax::Lua), None, "no partner");
        assert_eq!(partner("[a]", 0, Syntax::Sound), Some([0, 2]));
    }

    /// One frame of `code`'s editor in a scroll area scrolled `across`
    /// points sideways, focused: the shapes it painted.
    fn frame(ctx: &egui::Context, code: &mut Code, across: f32, events: Vec<egui::Event>) -> Vec<egui::epaint::ClippedShape> {
        let id = egui::Id::unique("code test");
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), events, ..Default::default() };
        ctx.memory_mut(|m| m.request_focus(id));
        ctx.run_ui(input, |ui| {
            egui::ScrollArea::both().horizontal_scroll_offset(across).show(ui, |ui| {
                code.ui(ui, id);
            });
        })
        .shapes
    }

    /// The left of the galley whose text begins with `first`, painted.
    fn left_of(shapes: &[egui::epaint::ClippedShape], first: &str) -> Option<f32> {
        shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text.starts_with(first) => Some(t.pos.x),
            _ => None,
        })
    }

    /// Scrolled sideways along a long line, the line numbers stay where
    /// they were; the text moves.
    #[test]
    fn the_line_numbers_stay_when_scrolled_sideways() {
        let ctx = egui::Context::default();
        let mut code = Code::new("main.lua", format!("local long = \"{}\"\nlocal b = 2\n", "x".repeat(400)));
        let still = frame(&ctx, &mut code, 0.0, Vec::new());
        let moved = frame(&ctx, &mut code, 300.0, Vec::new());
        let (a, b) = (left_of(&still, "  1").unwrap(), left_of(&moved, "  1").unwrap());
        assert!((a - b).abs() < 0.5, "numbers at {a}, then {b}");
        let (ta, tb) = (left_of(&still, "local").unwrap(), left_of(&moved, "local").unwrap());
        assert!(ta - tb > 250.0, "the text moved: {ta} to {tb}");
    }

    /// A frame with nothing changed lays nothing out anew: the text's and
    /// the numbers' layouts are the ones kept.
    #[test]
    fn a_still_frame_lays_out_nothing_anew() {
        let ctx = egui::Context::default();
        let mut code = Code::new("main.lua", "function on_tick()\n  local x = 1\nend\n".repeat(50));
        frame(&ctx, &mut code, 0.0, Vec::new());
        let (text, numbers) = (code.laid.text.as_ref().unwrap().1.clone(), code.laid.numbers.as_ref().unwrap().1.clone());
        frame(&ctx, &mut code, 0.0, Vec::new());
        assert!(std::sync::Arc::ptr_eq(&text, &code.laid.text.as_ref().unwrap().1) && std::sync::Arc::ptr_eq(&numbers, &code.laid.numbers.as_ref().unwrap().1));
    }

    /// Enter after `then` in the editor itself: the new line indented, one
    /// step of the text's undo takes it back whole.
    #[test]
    fn enter_in_the_editor_is_one_step_back() {
        let ctx = egui::Context::default();
        let id = egui::Id::unique("code test");
        let text = "function on_tick()\n  if x then";
        let mut code = Code::new("main.lua", text.to_string());
        frame(&ctx, &mut code, 0.0, Vec::new());
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(text.chars().count()))));
        state.store(&ctx, id);
        let enter = egui::Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        frame(&ctx, &mut code, 0.0, vec![enter]);
        assert_eq!(code.text, format!("{text}\n    "));
        assert!(code.go_back(&ctx, id));
        assert_eq!(code.text, text, "one step back");
    }

    /// How long a frame of a 3,000-line file takes, nothing changing (the
    /// text laid out once, then kept): printed; `cargo test --release
    /// code_frame_time -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn code_frame_time() {
        let ctx = egui::Context::default();
        let line = "  local thing = level:spawn(\"gem\", { x = 10, y = 20 }) -- a gem\n";
        let mut code = Code::new("main.lua", line.repeat(3000));
        for _ in 0..3 {
            frame(&ctx, &mut code, 0.0, Vec::new());
        }
        let t = std::time::Instant::now();
        for _ in 0..30 {
            frame(&ctx, &mut code, 0.0, Vec::new());
        }
        println!("a frame of 3,000 lines: {:.2} ms", t.elapsed().as_secs_f64() * 1000.0 / 30.0);
    }
}

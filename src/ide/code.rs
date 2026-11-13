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
}

/// A text being edited.
pub struct Code {
    pub text: String,
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
    /// The text the error was looked for in.
    checked: Option<String>,
    offer: Option<Offer>,
    /// Where the cursor stood when the offer was put away (Escape): not
    /// again there.
    declined: Option<usize>,
    reader: mlua::Lua,
}

impl Code {
    /// The text of `file` (its path in the game folder), Lua if it is.
    pub fn new(file: &str, text: String) -> Self {
        let syntax = match file.rsplit_once('.').map_or("", |(_, e)| e) {
            "lua" => Syntax::Lua,
            "toml" => Syntax::Sound,
            "mml" => Syntax::Song,
            "vvx" => Syntax::Model,
            _ => Syntax::Plain,
        };
        let mut code = Self { text, syntax, file: file.to_string(), error: None, sound: None, song: None, model: None, stopped: None, checked: None, offer: None, declined: None, reader: mlua::Lua::new() };
        code.check();
        code
    }

    /// Looks for a line its reader cannot read (Lua, or the sandbox
    /// refuses), if the text changed since.
    pub fn check(&mut self) {
        if self.syntax == Syntax::Plain || self.checked.as_deref() == Some(self.text.as_str()) {
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
            _ => match music::parse(&self.text) {
                Ok(song) => (self.error, self.song) = (None, Some(song_line(&song))),
                Err(e) => (self.error, self.song) = (Some(song_trouble(&format!("{e:#}"))), None),
            },
        }
        self.checked = Some(self.text.clone());
    }

    /// The editor in `ui` (its widget's `id`); whether the text changed.
    pub fn ui(&mut self, ui: &mut egui::Ui, id: egui::Id) -> bool {
        let font = TextStyle::Monospace.resolve(ui.style());
        let focused = ui.memory(|m| m.has_focus(id));
        let before = self.text.clone();

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
                self.accept(ui.ctx(), id);
            }
        }

        let syntax = self.syntax;
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| ui.fonts_mut(|f| f.layout_job(highlight(text.as_str(), &font, syntax)));
        let lines = self.text.split('\n').count();
        let digits = lines.to_string().len().max(3);
        let marked = |n: usize| self.error.as_ref().is_some_and(|(l, _)| *l == n) || self.stopped == Some(n);
        let marks: Vec<usize> = self.error.iter().map(|(l, _)| *l).chain(self.stopped).filter(|&l| l >= 1 && l <= lines).collect();
        let mut out = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            // The line numbers, the one Lua cannot read (or the game
            // stopped at) in red.
            let mut numbers = LayoutJob::default();
            for n in 1..=lines {
                let colour = if marked(n) { ERROR } else { LINE_NUMBER };
                numbers.append(&format!("{n:>digits$}\n"), 0.0, TextFormat { font_id: font.clone(), color: colour, ..Default::default() });
            }
            // (A row as the numbers' own layout has it: the font's row height
            // alone drifts from the text's after a hundred lines.)
            let row = ui.label(numbers).rect.height() / (lines + 1) as f32;
            // Its line marked behind the text.
            for &line in &marks {
                let top = ui.cursor().min.y + (line - 1) as f32 * row;
                let rect = egui::Rect::from_min_size(egui::pos2(ui.cursor().min.x - 4.0, top), egui::vec2(ui.available_width() + 4.0, row));
                ui.painter().rect_filled(rect, 2.0, ERROR_LINE);
            }
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
        let changed = self.text != before;
        self.check();

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
        self.text = text;
        self.check();
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
        let end = offer.from + insert.chars().count();
        if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
            state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(end))));
            state.store(ctx, id);
        }
    }
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
        Syntax::Sound | Syntax::Song | Syntax::Model => {
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
        code.text = "wave = \"square\"\nfrq = 880\n".into();
        code.check();
        assert!(code.error.as_ref().is_some_and(|(l, w)| *l == 2 && w.starts_with("frq: no such field") && !w.contains("coin.toml")), "{:?}", code.error);
        code.text = "# a coin\nwave = \"sine\"\n".into();
        code.check();
        assert!(code.error.is_none() && code.sound.is_some());
        let mut song = Code::new("music/t.mml", "#tempo fast\nA: c d e\n".into());
        assert!(matches!(&song.error, Some((1, w)) if w.contains("tempo") || !w.is_empty()), "{:?}", song.error);
        song.text = "#tempo 100\n\nA: c d e\nB: c d zz\n".into();
        song.check();
        assert!(matches!(&song.error, Some((4, _))), "inside a channel: its first line: {:?}", song.error);
        song.text = "#title t\nA: c d e\n".into();
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
}

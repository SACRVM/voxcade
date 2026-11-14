//! Finding words. In the open text (Ctrl+F): the row above it, the word,
//! how many there are and which is current, PREVIOUS and NEXT, and with
//! Ctrl+H (or the row's arrow) REPLACE and REPLACE ALL; every match marked
//! in the text (`Code::marking`). Across the game (Ctrl+Shift+F, SEARCH in
//! the bar): every line of the game's text files that holds the word, in
//! the list's place on the left, the files as the list has them
//! (`Files::names`: the game's `.gitignore`, no `shots/`, `builds/` or
//! hidden files), searched on a thread of its own.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use egui::text::{ByteIndex, LayoutJob, TextFormat};
use egui::{Color32, RichText, TextStyle};

use crate::gui::{ACCENT, INSET, QUIET, ROW_HOVERED, TEXT};

/// Every match's ground in the text, and the current one's.
pub const MATCH: Color32 = Color32::from_rgb(84, 70, 40);
pub const CURRENT: Color32 = Color32::from_rgb(214, 172, 74);
/// The current match's letters, dark on its light ground.
const ON_CURRENT: Color32 = Color32::from_rgb(24, 23, 30);

/// How many matches a text marks at most (a digit sought in a big model:
/// the count is whole, the marks stop).
pub const MARKS: usize = 10_000;

/// Where `word` stands in `text`: byte ranges in order, none overlapping;
/// letters of either case alike unless `case`.
pub fn matches(text: &str, word: &str, case: bool) -> Vec<Range<usize>> {
    if word.is_empty() {
        return Vec::new();
    }
    if case {
        return text.match_indices(word).map(|(i, w)| i..i + w.len()).collect();
    }
    let mut out = Vec::new();
    if word.is_ascii() {
        // Bytes alike but for case: an ASCII word only meets ASCII bytes,
        // so every match begins and ends between characters.
        let (bytes, w) = (text.as_bytes(), word.as_bytes());
        let mut i = 0;
        while i + w.len() <= bytes.len() {
            if bytes[i..i + w.len()].eq_ignore_ascii_case(w) {
                out.push(i..i + w.len());
                i += w.len();
            } else {
                i += 1;
            }
        }
        return out;
    }
    let w: Vec<char> = word.chars().flat_map(char::to_lowercase).collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (mut k, mut j) = (0, i);
        'walk: while k < w.len() && j < chars.len() {
            for l in chars[j].1.to_lowercase() {
                if k < w.len() && w[k] == l {
                    k += 1;
                } else {
                    k = usize::MAX;
                    break 'walk;
                }
            }
            j += 1;
        }
        if k == w.len() {
            out.push(chars[i].0..chars.get(j).map_or(text.len(), |c| c.0));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// What the find row asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Next,
    Previous,
    Replace,
    ReplaceAll,
    Close,
}

/// Which of the row's fields has the keys.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Word,
    With,
}

/// The find row over the open text.
#[derive(Default)]
pub struct Find {
    pub shown: bool,
    pub word: String,
    pub with: String,
    /// The replace field and its buttons shown (Ctrl+H, the row's arrow).
    pub replacing: bool,
    pub match_case: bool,
    /// The current match, by its place among them.
    pub current: usize,
    /// Where the current one is looked for from when the word changes (a
    /// byte of the text): the first match there or after it.
    pub anchor: usize,
    /// A line to look from instead (a project search's hit), once the text
    /// is there.
    pub line: Option<usize>,
    /// The current match to be shown (scrolled to, selected) on the next
    /// frame.
    pub reveal: bool,
    /// A field of the row takes the keys on the next frame.
    focus: Option<Field>,
    /// The row or the text had the keys last frame (so Escape closes it).
    pub had_keys: bool,
    /// The matches as last counted, and of what (the text's revision,
    /// the word, the case).
    counted: Option<(u64, String, bool)>,
    ranges: Vec<Range<usize>>,
}

impl Find {
    /// Shows the row (Ctrl+F; `replacing`: Ctrl+H) with the keys in it:
    /// the word the text has selected (one line of it) to look for, the
    /// current match the first from `anchor` on.
    pub fn open(&mut self, replacing: bool, selected: Option<&str>, anchor: usize) {
        self.shown = true;
        self.replacing = replacing;
        if let Some(word) = selected.filter(|w| !w.is_empty() && !w.contains('\n')) {
            self.word = word.to_string();
        }
        self.anchor = anchor;
        self.line = None;
        self.counted = None;
        self.focus = Some(if replacing && !self.word.is_empty() { Field::With } else { Field::Word });
    }

    /// The row put away: the text has the keys again.
    pub fn close(&mut self) {
        self.shown = false;
        self.focus = None;
    }

    /// Counts the matches in `text` (its `revision`, `Code::revision`)
    /// anew if it, the word or the case changed; a new word (or case, or
    /// line to look from) makes the first match from the anchor current,
    /// and shown.
    pub fn update(&mut self, text: &str, revision: u64) {
        if let Some(line) = self.line.take() {
            self.anchor = text.split_inclusive('\n').take(line.saturating_sub(1)).map(str::len).sum();
            self.counted = None;
        }
        let fresh = match &self.counted {
            Some((r, w, c)) if *w == self.word && *c == self.match_case => {
                if *r == revision {
                    return;
                }
                false
            }
            _ => true,
        };
        self.ranges = matches(text, &self.word, self.match_case);
        self.counted = Some((revision, self.word.clone(), self.match_case));
        if fresh {
            self.current = self.ranges.iter().position(|r| r.start >= self.anchor).unwrap_or(0);
            self.reveal = !self.ranges.is_empty();
        } else {
            self.current = self.current.min(self.ranges.len().saturating_sub(1));
        }
    }

    /// The current match, if there is one.
    pub fn current_range(&self) -> Option<Range<usize>> {
        self.ranges.get(self.current).cloned()
    }

    /// The next match (`forward`) or the one before, round at the ends.
    pub fn step(&mut self, forward: bool) {
        let n = self.ranges.len();
        if n == 0 {
            return;
        }
        self.current = if forward { (self.current + 1) % n } else { (self.current + n - 1) % n };
        self.anchor = self.ranges[self.current].start;
        self.reveal = true;
    }

    /// `text` with the current match replaced (REPLACE), the next one
    /// after it current; None without a match.
    pub fn replace(&mut self, text: &str) -> Option<String> {
        let range = self.current_range()?;
        let mut out = text.to_string();
        out.replace_range(range.clone(), &self.with);
        // Looked for after what was put in (which may hold the word again).
        self.anchor = range.start + self.with.len();
        self.counted = None;
        Some(out)
    }

    /// `text` with every match replaced (REPLACE ALL), and how many; None
    /// without a match.
    pub fn replace_all(&mut self, text: &str) -> Option<(String, usize)> {
        let ranges = matches(text, &self.word, self.match_case);
        if ranges.is_empty() {
            return None;
        }
        let mut out = String::with_capacity(text.len());
        let mut done = 0;
        for r in &ranges {
            out.push_str(&text[done..r.start]);
            out.push_str(&self.with);
            done = r.end;
        }
        out.push_str(&text[done..]);
        self.counted = None;
        Some((out, ranges.len()))
    }

    /// The count as the row shows it: "3 OF 12", NO MATCHES, nothing
    /// without a word.
    pub fn count(&self) -> String {
        match self.ranges.len() {
            _ if self.word.is_empty() => String::new(),
            0 => "NO MATCHES".into(),
            n => format!("{} OF {n}", self.current + 1),
        }
    }

    /// The row in `ui` (above the text); what it asks for. Enter in the
    /// word goes on to the next match (Shift: the one before), Enter in the
    /// replacement replaces.
    pub fn row(&mut self, ui: &mut egui::Ui) -> Option<Act> {
        let mut act = None;
        let small = TextStyle::Small.resolve(ui.style());
        let label_width = ui.painter().layout_no_wrap("REPLACE WITH".into(), small.clone(), QUIET).size().x;
        // A field's height, so that the labels stand on its middle.
        let height = ui.text_style_height(&TextStyle::Monospace) + 2.0 * FIELD_MARGIN.y;
        let label = |ui: &mut egui::Ui, word: &str| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(label_width, height), egui::Sense::hover());
            ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, word, small.clone(), QUIET);
        };
        let field_width = 300.0_f32.min(ui.available_width() / 3.0);
        let mut keys = false;
        ui.horizontal(|ui| {
            // The arrow that folds the replace row out, as a heading's does.
            let (rect, fold) = ui.allocate_exact_size(egui::vec2(18.0, height), egui::Sense::click());
            let fold = fold.on_hover_text("REPLACE · Ctrl+H");
            if fold.hovered() {
                ui.painter().rect_filled(rect, 3.0, ROW_HOVERED);
            }
            crate::gui::arrow(ui, rect.center(), if self.replacing { 1.0 } else { 0.0 });
            if fold.clicked() {
                self.replacing = !self.replacing;
                self.focus = Some(if self.replacing { Field::With } else { Field::Word });
            }
            label(ui, "FIND");
            let word = ui.add(field(ui, &mut self.word, word_id(), field_width));
            keys |= word.has_focus();
            if self.focus == Some(Field::Word) {
                select_all(ui.ctx(), word_id(), &self.word);
                word.request_focus();
            }
            if word.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let back = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } if modifiers.shift)));
                act = Some(if back { Act::Previous } else { Act::Next });
                word.request_focus();
                keys = true;
            }
            if ui.add(egui::Button::new(RichText::new("MATCH CASE").small()).selected(self.match_case)).on_hover_text("Upper and lower case count as different letters.").clicked() {
                self.match_case = !self.match_case;
            }
            ui.add_space(8.0);
            ui.label(RichText::new(self.count()).color(QUIET));
            ui.add_space(8.0);
            let some = !self.ranges.is_empty();
            if ui.add_enabled(some, egui::Button::new("PREVIOUS")).on_hover_text("Shift+Enter · Shift+F3").clicked() {
                act = Some(Act::Previous);
            }
            if ui.add_enabled(some, egui::Button::new("NEXT")).on_hover_text("Enter · F3").clicked() {
                act = Some(Act::Next);
            }
            if ui.small_button("CLOSE").on_hover_text("Escape").clicked() {
                act = Some(Act::Close);
            }
        });
        if self.replacing {
            ui.horizontal(|ui| {
                ui.add_space(18.0 + ui.spacing().item_spacing.x);
                label(ui, "REPLACE WITH");
                let with = ui.add(field(ui, &mut self.with, with_id(), field_width));
                keys |= with.has_focus();
                if self.focus == Some(Field::With) {
                    select_all(ui.ctx(), with_id(), &self.with);
                    with.request_focus();
                }
                if with.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    act = Some(Act::Replace);
                    with.request_focus();
                    keys = true;
                }
                let some = !self.ranges.is_empty();
                if ui.add_enabled(some, egui::Button::new("REPLACE")).on_hover_text("The current match, then on to the next · Enter").clicked() {
                    act = Some(Act::Replace);
                }
                if ui.add_enabled(some, egui::Button::new("REPLACE ALL")).on_hover_text("Every match, one step for UNDO").clicked() {
                    act = Some(Act::ReplaceAll);
                }
            });
        }
        self.focus = None;
        self.had_keys = keys;
        act
    }
}

/// Room round a field's text.
const FIELD_MARGIN: egui::Vec2 = egui::vec2(6.0, 4.0);

/// A field to type a word in: framed, so it shows on the main area's
/// ground too.
fn field<'a>(ui: &egui::Ui, text: &'a mut String, id: egui::Id, width: f32) -> egui::TextEdit<'a> {
    // (Its edge lighter while it has the keys.)
    let edge = if ui.memory(|m| m.has_focus(id)) { QUIET } else { FIELD_EDGE };
    let frame = egui::Frame::new().fill(crate::gui::GROUND).stroke(egui::Stroke::new(1.0, edge)).corner_radius(3).inner_margin(egui::Margin::symmetric(FIELD_MARGIN.x as i8, FIELD_MARGIN.y as i8));
    egui::TextEdit::singleline(text).id(id).font(TextStyle::Monospace).frame(frame).desired_width(width)
}

/// A field's edge.
const FIELD_EDGE: Color32 = Color32::from_rgb(68, 64, 80);

/// The find row's fields.
pub fn word_id() -> egui::Id {
    egui::Id::unique("find word")
}

fn with_id() -> egui::Id {
    egui::Id::unique("find with")
}

/// The whole of a field selected, to be typed over.
fn select_all(ctx: &egui::Context, id: egui::Id, text: &str) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()))));
    state.store(ctx, id);
}

/// `job` (a text laid out for its colours) with the matches of `word` in
/// `text` on a ground of their own, the `current` one (its place among
/// them) brighter.
pub fn mark(job: &mut LayoutJob, text: &str, word: &str, case: bool, current: Option<usize>) {
    let ranges = matches(text, word, case);
    if ranges.is_empty() {
        return;
    }
    let ranges = &ranges[..ranges.len().min(MARKS)];
    let mut sections = Vec::with_capacity(job.sections.len() + 2 * ranges.len());
    let mut m = 0;
    for section in job.sections.drain(..) {
        let mut at = section.byte_range.start.0;
        let end = section.byte_range.end.0;
        let mut leading = section.leading_space;
        while at < end {
            // The matches that end before here are done with.
            while m < ranges.len() && ranges[m].end <= at {
                m += 1;
            }
            let (to, ground) = match ranges.get(m) {
                Some(r) if r.start <= at => (r.end.min(end), Some(if current == Some(m) { CURRENT } else { MATCH })),
                Some(r) => (r.start.min(end), None),
                None => (end, None),
            };
            let mut format = section.format.clone();
            if let Some(ground) = ground {
                format.background = ground;
                format.color = if ground == CURRENT { ON_CURRENT } else { TEXT };
            }
            sections.push(egui::text::LayoutSection { leading_space: leading, byte_range: ByteIndex(at)..ByteIndex(to), format });
            leading = 0.0;
            at = to;
        }
    }
    job.sections = sections;
}

/// One line of the game's text files that holds the word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub file: String,
    pub line: usize,
    /// The line as written, trimmed at its ends.
    pub text: String,
}

/// How many lines a search gives at most, and how big a file it reads.
pub const MOST: usize = 1000;
const BIGGEST: u64 = 4 << 20;

/// Every line of the game's texts in `dir` that holds `word`, file by
/// file in the order of their paths; and whether there were more than
/// `MOST`. The texts are those the list shows (`Files::names`) and the main
/// area opens as text, where the word can be marked: the logic, the
/// scenarios, the sounds, the songs, the models (`.vvx`, as TEXT), the
/// levels (as TEXT) and the maps; not a world or a prefab.
pub fn search(dir: &Path, word: &str, case: bool) -> (Vec<Hit>, bool) {
    let mut out = Vec::new();
    if word.is_empty() {
        return (out, false);
    }
    let names = super::files::Files::names(dir);
    let files = super::files::Files::from_names(dir, names.clone());
    let text = |f: &String| files.logic.contains(f) || files.scenarios.contains(f) || super::sounds::name_of(f).is_some() || f.ends_with(".mml") || (f.ends_with(".vvx") && files.sort_of(f) != Some("Levels")) || files.level_text(f);
    for file in names.iter().filter(|f| text(f)) {
        let path = dir.join(file);
        if !std::fs::metadata(&path).is_ok_and(|m| m.len() <= BIGGEST) {
            continue;
        }
        // Text only: a picture or anything else is no UTF-8, or holds a nul.
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if text.contains('\0') {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            if matches(line, word, case).is_empty() {
                continue;
            }
            if out.len() == MOST {
                return (out, true);
            }
            out.push(Hit { file: file.to_string(), line: i + 1, text: line.trim().to_string() });
        }
    }
    (out, false)
}

/// The project search, in the list's place while shown.
#[derive(Default)]
pub struct Search {
    pub shown: bool,
    pub word: String,
    pub match_case: bool,
    pub hits: Vec<Hit>,
    /// More than `MOST` lines hold it.
    pub more: bool,
    /// What the hits are of (word, case), and the search at work.
    searched: Option<(String, bool)>,
    working: Option<mpsc::Receiver<(Vec<Hit>, bool)>>,
    /// The field takes the keys on the next frame.
    focus: bool,
    /// The field had the keys last frame (so Escape closes it).
    pub had_keys: bool,
    /// The hit chosen last.
    chosen: Option<(String, usize)>,
}

impl Search {
    /// Shown with the keys in its field (Ctrl+Shift+F, SEARCH), searched
    /// afresh (the files may have changed).
    pub fn open(&mut self, selected: Option<&str>) {
        self.shown = true;
        self.focus = true;
        if let Some(word) = selected.filter(|w| !w.is_empty() && !w.contains('\n')) {
            self.word = word.to_string();
        }
        self.searched = None;
    }

    /// Searched again if the word or the case changed; the answer taken
    /// once the thread has it.
    pub fn poll(&mut self, dir: &Path, ctx: &egui::Context) {
        let wanted = (self.word.clone(), self.match_case);
        if self.searched.as_ref() != Some(&wanted) {
            self.searched = Some(wanted.clone());
            let (send, receive) = mpsc::channel();
            let dir: PathBuf = dir.to_path_buf();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                // (Gone if a newer search took its place: nobody listens.)
                let _ = send.send(search(&dir, &wanted.0, wanted.1));
                ctx.request_repaint();
            });
            self.working = Some(receive);
        }
        if let Some(rx) = &self.working {
            match rx.try_recv() {
                Ok((hits, more)) => {
                    (self.hits, self.more) = (hits, more);
                    self.working = None;
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
                Err(mpsc::TryRecvError::Disconnected) => self.working = None,
            }
        }
    }

    /// Whether a search is at work.
    pub fn busy(&self) -> bool {
        self.working.is_some()
    }

    /// Waits for the search at work (a test's, a shot's).
    pub fn wait(&mut self, dir: &Path, ctx: &egui::Context) {
        self.poll(dir, ctx);
        if let Some(rx) = self.working.take()
            && let Ok((hits, more)) = rx.recv()
        {
            (self.hits, self.more) = (hits, more);
        }
    }

    /// The search in `ui` (the list's place): its field, MATCH CASE, how
    /// many, and the lines under their files. The hit clicked (file, line);
    /// whether CLOSE was pressed.
    pub fn ui(&mut self, ui: &mut egui::Ui, dir: &Path) -> (Option<(String, usize)>, bool) {
        let (mut go, mut close) = (None, false);
        ui.horizontal(|ui| {
            ui.label(RichText::new("SEARCH").small().color(QUIET));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                close = ui.small_button("CLOSE").on_hover_text("Back to the files · Escape").clicked();
            });
        });
        ui.add_space(4.0);
        let field = ui.add(field(ui, &mut self.word, search_id(), ui.available_width() - 6.0 - 2.0 * FIELD_MARGIN.x));
        if std::mem::take(&mut self.focus) {
            select_all(ui.ctx(), search_id(), &self.word);
            field.request_focus();
        }
        self.had_keys = field.has_focus();
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            // Enter searches again (the files may have changed).
            self.searched = None;
            field.request_focus();
            self.had_keys = true;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("MATCH CASE").small()).selected(self.match_case)).on_hover_text("Upper and lower case count as different letters.").clicked() {
                self.match_case = !self.match_case;
            }
            let files = {
                let mut names: Vec<&str> = self.hits.iter().map(|h| h.file.as_str()).collect();
                names.dedup();
                names.len()
            };
            let said = match () {
                _ if self.word.is_empty() => String::new(),
                _ if self.busy() && self.hits.is_empty() => "…".into(),
                _ if self.hits.is_empty() => "NO MATCHES".into(),
                _ => format!("{}{} IN {files} FILE{}", self.hits.len(), if self.more { "+" } else { "" }, if files == 1 { "" } else { "S" }),
            };
            ui.add(egui::Label::new(RichText::new(said).small().color(QUIET)).truncate());
        });
        self.poll(dir, ui.ctx());
        ui.add_space(8.0);
        let (word, case) = (self.word.clone(), self.match_case);
        egui::ScrollArea::vertical().id_salt("search hits").auto_shrink([false, false]).content_margin(egui::Margin { right: 6, bottom: 8, ..egui::Margin::ZERO }).show(ui, |ui| {
            let mut file = None;
            for hit in &self.hits {
                if file != Some(hit.file.as_str()) {
                    if file.is_some() {
                        ui.add_space(6.0);
                    }
                    file = Some(hit.file.as_str());
                    ui.horizontal(|ui| {
                        ui.add_space(INSET);
                        ui.add(egui::Label::new(RichText::new(&hit.file).small().color(ACCENT)).truncate());
                    });
                }
                let on = self.chosen.as_ref().is_some_and(|(f, l)| *f == hit.file && *l == hit.line);
                let row = hit_row(ui, hit, &word, case, on).on_hover_text(format!("{}:{}  {}", hit.file, hit.line, hit.text));
                if row.clicked() {
                    go = Some((hit.file.clone(), hit.line));
                }
            }
        });
        if let Some(hit) = &go {
            self.chosen = Some(hit.clone());
        }
        (go, close)
    }
}

/// The project search's field.
pub fn search_id() -> egui::Id {
    egui::Id::unique("search word")
}

/// The part of `line` that `room` letters show with its first match of
/// `word` in it: the line's start if the match ends within it, else from a
/// little before the match, an ellipsis for what is left out at either end.
pub fn around_match(line: &str, word: &str, case: bool, room: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    let room = room.max(8);
    if chars.len() <= room {
        return line.to_string();
    }
    let at = matches(line, word, case).first().map(|r| (line[..r.start].chars().count(), line[r.clone()].chars().count()));
    let start = match at {
        Some((from, len)) if from + len > room - 1 => from.saturating_sub(6).min(chars.len().saturating_sub(room - 1)),
        _ => 0,
    };
    let mut out = String::new();
    let mut left = room;
    if start > 0 {
        out.push('…');
        left -= 1;
    }
    let end = (start + left).min(chars.len());
    let cut = end < chars.len();
    let end = if cut { end - 1 } else { end };
    out.extend(&chars[start..end]);
    if cut {
        out.push('…');
    }
    out
}

/// A hit as a row of the list: its line's number, quiet, then its text,
/// the word on the matches' ground; the whole row clicked.
fn hit_row(ui: &mut egui::Ui, hit: &Hit, word: &str, case: bool, on: bool) -> egui::Response {
    let font = TextStyle::Monospace.resolve(ui.style());
    let number = format!("{:>4}  ", hit.line);
    let mut job = LayoutJob::default();
    job.append(&number, 0.0, TextFormat { font_id: font.clone(), color: QUIET, ..Default::default() });
    // As much of the line as the row has room for, the first match in it.
    let letter = ui.painter().layout_no_wrap("M".into(), font.clone(), TEXT).size().x.max(1.0);
    let room = ((ui.available_width() / letter) as usize).saturating_sub(number.chars().count());
    let shown = around_match(&hit.text, word, case, room);
    job.append(&shown, 0.0, TextFormat { font_id: font.clone(), color: TEXT, ..Default::default() });
    let whole = format!("{number}{shown}");
    mark(&mut job, &whole, word, case, None);
    // (The number's own digits are no match.)
    let skip = number.len();
    for section in &mut job.sections {
        if section.byte_range.end.0 <= skip {
            section.format.background = Color32::TRANSPARENT;
        }
    }
    let galley = ui.painter().layout_job(job);
    let (id, rect) = ui.allocate_space(egui::vec2(ui.available_width(), galley.size().y + 8.0));
    let band = rect.expand2(egui::vec2(0.0, ui.spacing().item_spacing.y / 2.0));
    let response = ui.interact(band, id, egui::Sense::click());
    if on || response.hovered() {
        ui.painter().rect_filled(band, 3.0, if on { crate::gui::ROW_OPEN } else { ROW_HOVERED });
    }
    let painter = ui.painter().with_clip_rect(band.intersect(ui.clip_rect()));
    painter.galley(egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, TEXT);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long line in the list shows its match, not only its start.
    #[test]
    fn a_hit_shows_its_match_in_a_narrow_row() {
        let line = "-- the score and the time: every gem taken counts";
        let shown = around_match(line, "gem", false, 20);
        assert!(shown.contains("gem") && shown.starts_with('…') && shown.chars().count() <= 20, "{shown}");
        assert_eq!(around_match("c.things.gem = { big }", "gem", false, 20), "c.things.gem = { bi…");
        assert_eq!(around_match("short gem", "gem", false, 20), "short gem");
        // The match at the very end: the row ends with it.
        assert!(around_match("a long line that ends with the word gem", "GEM", false, 12).ends_with("gem"));
    }

    /// Matches are counted with the case ignored by default, exactly with
    /// MATCH CASE; NEXT goes round at the end and PREVIOUS at the start.
    #[test]
    fn finding_counts_steps_round_and_minds_the_case() {
        let text = "Gem gem GEM\nno g e m here\ngems ÄPFEL äpfel";
        assert_eq!(matches(text, "gem", false), [0..3, 4..7, 8..11, 26..29]);
        assert_eq!(matches(text, "gem", true), [4..7, 26..29]);
        assert_eq!(matches(text, "äpfel", false).len(), 2, "beyond ASCII too");
        assert_eq!(matches(text, "äpfel", true).len(), 1);
        assert!(matches(text, "", false).is_empty());
        assert_eq!(matches("aaaa", "aa", false), [0..2, 2..4], "never overlapping");
        let mut find = Find { word: "gem".into(), ..Find::default() };
        find.open(false, None, 5);
        find.update(text, 1);
        assert_eq!(find.count(), "3 OF 4", "the first from where the cursor stood");
        find.step(true);
        assert_eq!(find.count(), "4 OF 4");
        find.step(true);
        assert_eq!(find.count(), "1 OF 4", "round at the end");
        find.step(false);
        assert_eq!(find.count(), "4 OF 4", "and back round at the start");
        find.match_case = true;
        find.update(text, 1);
        assert_eq!(find.count(), "2 OF 2", "the first from the one current before");
        find.word = "zebra".into();
        find.update(text, 1);
        assert_eq!(find.count(), "NO MATCHES");
        find.word.clear();
        find.update(text, 1);
        assert_eq!(find.count(), "");
    }

    /// REPLACE replaces the current match and goes on to the one after
    /// what it put in; REPLACE ALL every one.
    #[test]
    fn replacing_one_and_all() {
        let mut find = Find { word: "a".into(), with: "aa".into(), ..Find::default() };
        find.open(true, None, 0);
        let text = "a b a";
        find.update(text, 1);
        let text = find.replace(text).unwrap();
        assert_eq!(text, "aa b a");
        find.update(&text, 2);
        assert_eq!(find.current_range(), Some(5..6), "on after what was put in, not on it again");
        let mut all = Find { word: "A".into(), with: "x".into(), ..Find::default() };
        assert_eq!(all.replace_all("a b A"), Some(("x b x".to_string(), 2)));
        all.match_case = true;
        assert_eq!(all.replace_all("a b A"), Some(("a b x".to_string(), 1)));
        all.word = "q".into();
        assert_eq!(all.replace_all("a b A"), None);
    }

    /// A line to look from (a project search's hit) makes the match on it
    /// current.
    #[test]
    fn a_line_to_look_from_makes_its_match_current() {
        let mut find = Find { word: "x".into(), ..Find::default() };
        find.line = Some(3);
        find.update("x\nx\nyx\nx\n", 1);
        assert_eq!(find.current_range(), Some(5..6));
        assert_eq!(find.count(), "3 OF 4");
    }

    /// The marks split the text's coloured pieces and keep its colours,
    /// the current match brighter.
    #[test]
    fn the_matches_are_marked_in_the_text() {
        let font = egui::FontId::monospace(16.0);
        let text = "local gem = gem";
        let mut job = super::super::code::highlight(text, &font, super::super::code::Syntax::Lua);
        let before: String = job.sections.iter().map(|s| &text[s.byte_range.start.0..s.byte_range.end.0]).collect();
        mark(&mut job, text, "GEM", false, Some(1));
        let after: String = job.sections.iter().map(|s| &text[s.byte_range.start.0..s.byte_range.end.0]).collect();
        assert_eq!(before, after, "the text whole");
        let grounds: Vec<(&str, Color32)> = job.sections.iter().filter(|s| s.format.background != Color32::TRANSPARENT).map(|s| (&text[s.byte_range.start.0..s.byte_range.end.0], s.format.background)).collect();
        assert_eq!(grounds, [("gem", MATCH), ("gem", CURRENT)]);
    }

    /// The project search finds the lines of the files the list shows:
    /// nothing in what the `.gitignore` ignores, in `shots/`, `builds/`,
    /// hidden files or pictures.
    #[test]
    fn the_project_search_keeps_to_the_listed_files() {
        let dir = crate::scratch::Scratch::new("find-search");
        for (path, text) in [
            ("main.lua", "local gem = 1\nfunction on_tick()\n  -- a Gem\nend\n"),
            ("conf.lua", "function conf(c)\n  c.things.gem = {}\nend\n"),
            ("sounds/gem.toml", "wave = \"sine\" # gem\n"),
            ("music/gem.mml", "; gem\n"),
            ("gem.vvx", "# gem\n"),
            ("levels/01.toml", "format = \"voxcade level 1\"\nkind = \"gem\"\n"),
            ("map.toml", "format = \"voxcade map 1\"\n# gem\n"),
            ("notes.md", "gem\n"),
            ("notes/gem.txt", "gem\n"),
            ("shots/gem.lua", "gem\n"),
            ("builds/gem.lua", "gem\n"),
            (".hidden.lua", "gem\n"),
            ("scratch.lua", "gem\n"),
            (".gitignore", "notes/\nscratch.lua\n"),
        ] {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        std::fs::write(dir.join("cover.png"), [0x89, b'P', b'N', b'G', 0, 0xff, b'g', b'e', b'm']).unwrap();
        let (hits, more) = search(&dir, "gem", false);
        let places: Vec<(&str, usize)> = hits.iter().map(|h| (h.file.as_str(), h.line)).collect();
        assert_eq!(places, [("conf.lua", 2), ("gem.vvx", 1), ("levels/01.toml", 2), ("main.lua", 1), ("main.lua", 3), ("map.toml", 2), ("music/gem.mml", 1), ("sounds/gem.toml", 1)], "the levels and the map too, no other file, nothing ignored or hidden");
        assert!(!more);
        assert_eq!(hits[4].text, "-- a Gem", "the line trimmed");
        assert_eq!(search(&dir, "Gem", true).0.len(), 1, "MATCH CASE");
        assert!(search(&dir, "", false).0.is_empty());
    }
}

//! RENAME and DELETE for the game's files, from the list's right click and
//! from the buttons over the open file. A name is typed in place in the
//! list as the plus's is (`new_file`'s checks: no folders, no empty name,
//! no name taken); the places in the game's Lua that name the file
//! (`refs`: a sound where a sound is taken, a module by `require`, a model
//! or a song by its path), and in its levels and maps (a piece's, a
//! scatter's or a prefab's model, a map's ground pictures), are listed with
//! a tick each, the certain ones
//! ticked, the maybes apart and not, and the ticked ones changed too. DELETE asks nothing: the
//! file goes to the system's trash and an UNDO shows for a few seconds
//! (the bytes are kept in memory for it).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::RichText;

use super::{Kind, Shell};
use crate::gui::{ACCENT, PANEL, QUIET, TEXT};
use crate::ide::refs::{self, Ref};
pub use crate::ide::refs::Sort;

/// How long UNDO shows after a delete.
pub const UNDO_FOR: Duration = Duration::from_secs(20);

/// The biggest file whose bytes are kept in memory for UNDO.
pub const MAX_UNDO: u64 = 8_000_000;

/// What the two ways in ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Rename,
    Delete,
}

/// Why a file of the list cannot be renamed or deleted (None: it can).
pub fn why_not(path: &str, group: &str) -> Option<&'static str> {
    if matches!(path, "main.lua" | "conf.lua") {
        return Some("The game needs main.lua and conf.lua: they keep their names and stay.");
    }
    // A map's prefab is named by its file in the map (found and changed
    // as the Lua's places are); levels and blocks by their names.
    let prefab = path.starts_with("prefabs/") && path.ends_with(".vvx");
    if group == "Blocks" || (group == "Levels" && !prefab) {
        return Some("Not yet: the level files link to levels and blocks by their names, so these stay as they are for now.");
    }
    None
}

/// A path as its folder (with the `/`), its name without the ending, and
/// the ending.
pub fn split_name(path: &str) -> (&str, &str, &str) {
    let (folder, name) = match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path),
    };
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (folder, stem, ext)
}

/// The path a name typed for `path` stands for (its folder and its ending
/// kept, not typed): the plus's rules for the sorts the plus makes (lower
/// case, spaces `_`, letters, digits, `_` and `-`), the same but for the
/// case and a `.` inside for images and the rest. A plain sentence says
/// why not. The name `path` already has comes back as it is.
pub fn renamed(dir: &Path, path: &str, typed: &str, sort: Sort) -> Result<String, String> {
    let (folder, _, ext) = split_name(path);
    let name = typed.trim().replace(' ', "_");
    let name = if sort == Sort::Other { name } else { name.to_lowercase() };
    let dotted = if ext.is_empty() { String::new() } else { format!(".{ext}") };
    // The ending typed too is not asked twice.
    let name = match name.len().checked_sub(dotted.len()).filter(|&c| !dotted.is_empty() && name.is_char_boundary(c) && name[c..].eq_ignore_ascii_case(&dotted)) {
        Some(cut) => name[..cut].to_string(),
        None => name,
    };
    if name.is_empty() {
        return Err("Type a name first.".into());
    }
    if name.contains(['/', '\\']) {
        return Err("A name cannot hold a folder: no / in it.".into());
    }
    if sort == Sort::Other {
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) || name.starts_with('.') {
            return Err("A name takes letters, digits, _, - and . only (not at its start).".into());
        }
    } else if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err("A name takes letters, digits, _ and - only.".into());
    }
    let new = format!("{folder}{name}{dotted}");
    if new == path || new.eq_ignore_ascii_case(path) {
        return Ok(new);
    }
    let taken = dir.join(&new).exists();
    if taken { Err(format!("{new} is already there.")) } else { Ok(new) }
}

/// The places of `rows` (the certain ones) in words of what the game still
/// does with a file just deleted: "main.lua still plays it in 2 lines".
pub fn still_used(rows: &[Ref], sort: Sort) -> Option<String> {
    let rows: Vec<&Ref> = rows.iter().filter(|r| r.sure).collect();
    if rows.is_empty() {
        return None;
    }
    let mut files: Vec<&str> = Vec::new();
    for r in &rows {
        if !files.contains(&r.file.as_str()) {
            files.push(&r.file);
        }
    }
    let (who, many) = match files.as_slice() {
        [one] => (one.to_string(), false),
        [a, b] => (format!("{a} and {b}"), true),
        more => (format!("{} files", more.len()), true),
    };
    let does = match (sort, many) {
        (Sort::Sound, false) => "still plays it",
        (Sort::Sound, true) => "still play it",
        (Sort::Lua, false) => "still requires it",
        (Sort::Lua, true) => "still require it",
        (_, false) => "still names it",
        (_, true) => "still name it",
    };
    Some(format!("{who} {does} in {} line{}", rows.len(), if rows.len() == 1 { "" } else { "s" }))
}

/// Where a deleted file went.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    /// The system's trash.
    Trash,
    /// A folder of the IDE's own (the trash would not take it).
    Folder(PathBuf),
}

/// The file to the system's trash. (Not in the tests: they would fill the
/// user's trash, so there the fallback folder takes everything.)
fn to_trash(file: &Path) -> Result<(), String> {
    #[cfg(not(test))]
    {
        trash::delete(file).map_err(|e| e.to_string())
    }
    #[cfg(test)]
    {
        let _ = file;
        Err("the tests keep out of the system's trash".into())
    }
}

/// The file `rel` of the game folder `dir` put away: into the system's
/// trash, else moved to `fallback` (its path kept, a number added if one
/// is there). Where it went, and why the trash would not take it if it
/// would not.
pub fn discard(dir: &Path, rel: &str, fallback: &Path) -> Result<(Place, Option<String>), String> {
    let file = dir.join(rel);
    let why = match to_trash(&file) {
        Ok(()) => return Ok((Place::Trash, None)),
        Err(why) => why,
    };
    let mut to = fallback.join(rel);
    let mut n = 1;
    while to.exists() {
        n += 1;
        to = fallback.join(format!("{rel}.{n}"));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    move_file(&file, &to).map_err(|e| format!("{rel}: {e}"))?;
    Ok((Place::Folder(to), Some(why)))
}

/// A file moved, across disks too.
pub fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// A name being typed for a file of the list (RENAME).
pub struct Renaming {
    pub path: String,
    pub text: String,
    /// Why the last Enter renamed nothing.
    pub problem: Option<String>,
    /// Its field is to take the keys, once.
    pub fresh: bool,
}

/// The question of a rename that other places of the game name: each
/// place with its tick (the certain ones ticked at first, the maybes not).
pub struct Question {
    pub from: String,
    pub to: String,
    pub rows: Vec<Ref>,
    pub ticked: Vec<bool>,
    /// RENAME is to take the keys, once.
    pub fresh: bool,
}

impl Question {
    /// The places ticked.
    pub fn chosen(&self) -> Vec<&Ref> {
        self.rows.iter().zip(&self.ticked).filter(|(_, t)| **t).map(|(r, _)| r).collect()
    }
}

/// The answers to a `Question`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Rename the file and change the places ticked.
    Change,
    OnlyTheFile,
    Cancel,
}

/// A file just deleted, what shows of it for a few seconds and what UNDO
/// needs.
pub struct Gone {
    pub path: String,
    /// The line in the bar: DELETED name, and what still uses it.
    pub said: String,
    pub bytes: Option<Vec<u8>>,
    pub place: Place,
    pub was_open: bool,
    pub scroll: Option<f32>,
    pub cursor: Option<usize>,
    pub until: Instant,
}

impl Gone {
    /// Whether UNDO can bring it back (its bytes are kept, or it lies in
    /// our folder).
    pub fn can_undo(&self) -> bool {
        self.bytes.is_some() || matches!(self.place, Place::Folder(_))
    }
}

/// RENAME and DELETE, small buttons, as the open file's row has them:
/// what was asked. `why` is the reason they are off, if they are.
pub fn buttons(ui: &mut egui::Ui, why: Option<&str>) -> Option<Act> {
    let mut asked = None;
    let mut one = |ui: &mut egui::Ui, word: &str, act: Act, tip: &str| {
        let button = egui::Button::new(RichText::new(word).small());
        let response = ui.add_enabled(why.is_none(), button).on_hover_text(tip).on_disabled_hover_text(why.unwrap_or(""));
        if response.clicked() {
            asked = Some(act);
        }
    };
    // (Right to left: the first added is the last shown.)
    one(ui, "DELETE", Act::Delete, "Moves the file to the trash. UNDO shows for a few seconds, and the trash keeps it after that.");
    one(ui, "RENAME", Act::Rename, "Gives the file another name, typed in the list (Enter takes it, Escape leaves it).");
    asked
}

impl Shell {
    /// The files of the game that may name another file: its Lua (logic
    /// and scenarios), its levels and maps (`refs` reads each by its
    /// format line).
    fn naming(&self) -> Vec<String> {
        let lua = self.files.logic.iter().chain(&self.files.scenarios).filter(|f| f.ends_with(".lua"));
        let data = self.files.assets.iter().filter(|g| g.name != "Sounds").flat_map(|g| &g.files).filter(|f| f.ends_with(".toml"));
        lua.chain(data).cloned().collect()
    }

    /// What `act` asks of `path` (from the menu or the buttons).
    pub(super) fn manage(&mut self, act: Act, path: &str) {
        match act {
            Act::Rename => self.begin_rename(path),
            Act::Delete => self.delete(path),
        }
    }

    /// RENAME: the file's row in the list turns into the field its name is
    /// typed in; the text being changed is saved first.
    pub fn begin_rename(&mut self, path: &str) {
        let Some(group) = self.files.sort_of(path) else { return };
        if let Some(why) = why_not(path, group) {
            return self.tell(why.into());
        }
        self.flush();
        self.managing.naming = None;
        self.message = None;
        self.managing.renaming = Some(Renaming { path: path.to_string(), text: split_name(path).1.to_string(), problem: None, fresh: true });
    }

    /// The name `typed` for the file `from`: it is renamed, or the question
    /// about the lines that name it is asked first. A plain sentence says why
    /// not.
    pub fn rename(&mut self, from: &str, typed: &str) -> Result<(), String> {
        let group = self.files.sort_of(from).ok_or_else(|| format!("{from} is not a file of the game."))?;
        if let Some(why) = why_not(from, group) {
            return Err(why.into());
        }
        let sort = Sort::of_file(group, from);
        let to = renamed(&self.dir, from, typed, sort)?;
        if to == from {
            return Ok(());
        }
        self.flush();
        let rows = refs::find(&self.dir, &self.naming(), from, &to, sort);
        if rows.is_empty() {
            self.do_rename(from, &to, &[]);
        } else {
            let ticked = rows.iter().map(|r| r.sure).collect();
            self.managing.asking = Some(Question { from: from.to_string(), to, rows, ticked, fresh: true });
        }
        Ok(())
    }

    /// The answer to the question: the file renamed (and the lines changed),
    /// or nothing.
    pub fn answer(&mut self, answer: Answer) {
        let Some(q) = self.managing.asking.take() else { return };
        match answer {
            Answer::Change => self.do_rename(&q.from, &q.to, &q.chosen()),
            Answer::OnlyTheFile => self.do_rename(&q.from, &q.to, &[]),
            Answer::Cancel => {}
        }
    }

    /// The file `from` renamed `to` (checked), the places `chosen` changed
    /// in the game's Lua. What the IDE remembers of the file goes with it:
    /// the open file stays open, scroll and cursor kept, no journal left
    /// under the old name.
    fn do_rename(&mut self, from: &str, to: &str, chosen: &[&Ref]) {
        self.flush();
        if self.sound.listening.as_deref() == Some(from) {
            self.hush();
        }
        if let Err(e) = std::fs::rename(self.dir.join(from), self.dir.join(to)) {
            return self.tell(format!("{from}: {e}"));
        }
        self.rescan();
        // The journal of a text not saved when the IDE last stopped.
        super::super::buffer::rename_journal(&self.dir, from, to, &self.journals());
        if let Some(v) = self.memory.scroll.remove(from) {
            self.memory.scroll.insert(to.to_string(), v);
        }
        if let Some(v) = self.memory.cursor.remove(from) {
            self.memory.cursor.insert(to.to_string(), v);
        }
        for path in [&mut self.memory.open, &mut self.opening.previous] {
            if path.as_deref() == Some(from) {
                *path = Some(to.to_string());
            }
        }
        let mut changed = Vec::new();
        if !chosen.is_empty() {
            match refs::rewrite(&self.dir, chosen) {
                Ok(files) => changed = files,
                Err(e) => self.tell(format!("{e:#}")),
            }
        }
        // What is open is read again from the disk (the text open on a file
        // just changed would otherwise save its old text over the new).
        if self.memory.open.as_deref().is_some_and(|open| open == to || changed.iter().any(|c| c == open)) {
            self.unload();
            self.opening.restore = self.memory.open.as_ref().and_then(|p| self.memory.scroll.get(p).copied());
        }
        self.sound.used = None;
        let name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
        let more = if changed.is_empty() { String::new() } else { format!(" · {} changed too", if changed.len() == 1 { changed[0].clone() } else { format!("{} files", changed.len()) }) };
        self.note(format!("RENAMED {} TO {}{more}", name(from), name(to)));
        let _ = self.save();
    }

    /// Where the journals of the texts being changed are kept.
    pub(super) fn journals(&self) -> PathBuf {
        self.journals.clone()
    }

    /// DELETE: the file goes to the system's trash (else into the IDE's own
    /// folder, said in OUTPUT), nothing asked; UNDO shows for a few seconds.
    /// A file the game still names is said so, and deleted all the same.
    pub fn delete(&mut self, path: &str) {
        let Some(group) = self.files.sort_of(path) else { return };
        if let Some(why) = why_not(path, group) {
            return self.tell(why.into());
        }
        let sort = Sort::of_file(group, path);
        self.flush();
        // A text of the user's not in the file (kept while it changed
        // outside too) would go with it, unasked: the question first.
        if super::super::buffer::holds_unsaved(&self.dir, path, &self.journals()) {
            return self.tell(format!("{path} changed outside while your text was not saved yet: open it and choose KEEP MINE or TAKE THE FILE'S before deleting it."));
        }
        if self.sound.listening.as_deref() == Some(path) {
            self.hush();
        }
        let file = self.dir.join(path);
        let size = std::fs::metadata(&file).map_or(0, |m| m.len());
        let bytes = if size <= MAX_UNDO { std::fs::read(&file).ok() } else { None };
        let still = still_used(&refs::find(&self.dir, &self.naming(), path, path, sort), sort);
        let (place, why) = match discard(&self.dir, path, &self.deleted_dir) {
            Ok(went) => went,
            Err(e) => return self.tell(e),
        };
        let name = path.rsplit('/').next().unwrap_or(path);
        if let (Place::Folder(to), Some(why)) = (&place, &why) {
            self.report(Kind::Note, format!("The system's trash would not take {path} ({why}); it is in {} instead.", to.display()), None);
        }
        let _ = std::fs::remove_file(self.journals().join(path.replace(['/', '\\'], "%")));
        let was_open = self.memory.open.as_deref() == Some(path);
        let (scroll, cursor) = (self.memory.scroll.remove(path), self.memory.cursor.remove(path));
        let mut said = format!("DELETED {name}");
        if let Some(still) = still {
            said.push_str(&format!(" · {still}"));
        }
        let gone = Gone { path: path.to_string(), said, bytes, place, was_open, scroll, cursor, until: Instant::now() + UNDO_FOR };
        if !gone.can_undo() {
            self.managing.gone = Some(Gone { said: format!("{} · it is in the trash", gone.said), ..gone });
        } else {
            self.managing.gone = Some(gone);
        }
        let said = self.managing.gone.as_ref().map(|g| g.said.clone()).unwrap_or_default();
        self.report(Kind::Note, said, None);
        self.bar.notice = None;
        self.rescan();
        self.sound.used = None;
        if self.opening.previous.as_deref() == Some(path) {
            self.opening.previous = None;
        }
        if was_open {
            // The file open before it, or nothing.
            self.memory.open = self.opening.previous.take().filter(|p| self.files.has(p));
            self.unload();
            self.opening.restore = self.memory.open.as_ref().map(|p| self.memory.scroll.get(p).copied().unwrap_or(0.0));
        }
        let _ = self.save();
    }

    /// UNDO: the file back exactly (its bytes, kept in memory, or moved
    /// back from our folder), and open again if it was open.
    pub fn undo(&mut self) {
        let Some(g) = self.managing.gone.take() else { return };
        let target = self.dir.join(&g.path);
        if target.exists() {
            return self.tell(format!("{} is there again, so UNDO leaves it as it is.", g.path));
        }
        let back = match (&g.bytes, &g.place) {
            (Some(bytes), _) => target.parent().map_or(Ok(()), std::fs::create_dir_all).map_err(anyhow::Error::from).and_then(|()| crate::blocks::save_at_once(&target, bytes)),
            (None, Place::Folder(from)) => target.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| move_file(from, &target)).map_err(anyhow::Error::from),
            _ => Err(anyhow::anyhow!("it is in the trash")),
        };
        if let Err(e) = back {
            return self.tell(format!("{}: {e:#}", g.path));
        }
        // The copy we kept is no longer wanted (the system's trash keeps its own).
        if let (Some(_), Place::Folder(copy)) = (&g.bytes, &g.place) {
            let _ = std::fs::remove_file(copy);
        }
        self.rescan();
        self.sound.used = None;
        if let Some(v) = g.scroll {
            self.memory.scroll.insert(g.path.clone(), v);
        }
        if let Some(v) = g.cursor {
            self.memory.cursor.insert(g.path.clone(), v);
        }
        if g.was_open {
            self.open(&g.path);
            self.back_to_file();
        }
        let name = g.path.rsplit('/').next().unwrap_or(&g.path).to_string();
        self.note(format!("PUT {name} BACK"));
    }

    /// The question over the window: the places of the game that name the
    /// file being renamed, each with its tick, and what to do.
    pub(super) fn question_ui(&mut self, ui: &mut egui::Ui) {
        let Some(q) = &mut self.managing.asking else { return };
        let mut answer = None;
        let fresh = std::mem::take(&mut q.fresh);
        let name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
        let title = format!("RENAME {} TO {}", name(&q.from), name(&q.to));
        let modal = egui::Modal::new(egui::Id::unique("rename asks")).frame(egui::Frame::popup(ui.style()).fill(PANEL).inner_margin(egui::Margin::same(20))).show(ui.ctx(), |ui| {
            ui.set_width(720.0f32.min(ui.ctx().content_rect().width() - 80.0));
            ui.label(RichText::new(title).color(ACCENT));
            ui.add_space(10.0);
            egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.5).auto_shrink([false, true]).show(ui, |ui| {
                let maybes = q.rows.iter().position(|r| !r.sure);
                for (i, (row, tick)) in q.rows.iter().zip(q.ticked.iter_mut()).enumerate() {
                    if Some(i) == maybes {
                        ui.add_space(8.0);
                        ui.label(RichText::new("MAYBE").small().color(QUIET)).on_hover_text("The same name in quotes where the game's code may mean something else (its own function, a variable): tick the ones that mean this file.");
                    }
                    let shown: String = row.text.chars().take(90).collect();
                    let text = RichText::new(format!("{}:{}  {shown}", row.file, row.line)).monospace().color(if *tick { TEXT } else { QUIET });
                    ui.checkbox(tick, text);
                }
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let change = ui.button("RENAME").on_hover_text("Renames the file and changes the places ticked to its new name.");
                if fresh {
                    change.request_focus();
                }
                if change.clicked() {
                    answer = Some(Answer::Change);
                }
                if ui.button("ONLY THE FILE").on_hover_text("Renames the file and leaves every place as it is (the game will not find it by its old name there).").clicked() {
                    answer = Some(Answer::OnlyTheFile);
                }
                if ui.button("CANCEL").on_hover_text("Leaves the file as it is.").clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Cancel);
        }
        if let Some(a) = answer {
            self.answer(a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> crate::scratch::Scratch {
        let dir = crate::scratch::Scratch::new(&format!("manage-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        dir
    }

    /// A name typed keeps the folder and the ending, follows the plus's
    /// rules for the sorts it makes and says plainly what is wrong.
    #[test]
    fn a_name_typed_for_a_rename_is_checked() {
        let dir = folder("names");
        std::fs::write(dir.join("sounds/bell.toml"), "").unwrap();
        std::fs::write(dir.join("sounds/coin.toml"), "").unwrap();
        std::fs::write(dir.join("Back.png"), "").unwrap();
        assert_eq!(renamed(&dir, "sounds/coin.toml", "Coin Two", Sort::Sound), Ok("sounds/coin_two.toml".into()));
        assert_eq!(renamed(&dir, "sounds/coin.toml", "pick.toml", Sort::Sound), Ok("sounds/pick.toml".into()));
        assert_eq!(renamed(&dir, "sounds/coin.toml", "coin", Sort::Sound), Ok("sounds/coin.toml".into()), "the same name: nothing to do");
        assert_eq!(renamed(&dir, "sounds/coin.toml", "bell", Sort::Sound), Err("sounds/bell.toml is already there.".into()));
        assert_eq!(renamed(&dir, "sounds/coin.toml", "  ", Sort::Sound), Err("Type a name first.".into()));
        assert_eq!(renamed(&dir, "sounds/coin.toml", "a/b", Sort::Sound), Err("A name cannot hold a folder: no / in it.".into()));
        assert_eq!(renamed(&dir, "sounds/coin.toml", "a.b", Sort::Sound), Err("A name takes letters, digits, _ and - only.".into()));
        // An image keeps its capitals and may have a dot inside.
        assert_eq!(renamed(&dir, "Back.png", "Sky.big", Sort::Other), Ok("Sky.big.png".into()));
        assert!(renamed(&dir, "Back.png", ".hidden", Sort::Other).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a deleted file's certain places say, in words.
    #[test]
    fn what_still_names_a_deleted_file_is_said() {
        let dir = folder("refs");
        std::fs::write(dir.join("main.lua"), "level:sound(\"coin\", p)\nfx.fly(\"coin\")\n").unwrap();
        std::fs::write(dir.join("conf.lua"), "c.a = { sound = 'coin' }\n").unwrap();
        let scripts = ["conf.lua".to_string(), "main.lua".to_string()];
        let rows = refs::find(&dir, &scripts, "sounds/coin.toml", "sounds/coin.toml", Sort::Sound);
        assert_eq!(rows.len(), 3, "two certain, one maybe");
        assert_eq!(still_used(&rows, Sort::Sound).as_deref(), Some("conf.lua and main.lua still play it in 2 lines"));
        assert_eq!(still_used(&[], Sort::Sound), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file put away goes to the fallback folder (the trash is not
    /// touched under test), a second of the same name beside the first.
    #[test]
    fn a_file_put_away_keeps_its_path_in_the_fallback_folder() {
        let dir = folder("away");
        let fallback = folder("away-kept");
        std::fs::write(dir.join("sounds/coin.toml"), "one").unwrap();
        let (place, why) = discard(&dir, "sounds/coin.toml", &fallback).unwrap();
        assert_eq!(place, Place::Folder(fallback.join("sounds/coin.toml")));
        assert!(why.is_some() && !dir.join("sounds/coin.toml").exists());
        std::fs::write(dir.join("sounds/coin.toml"), "two").unwrap();
        let (second, _) = discard(&dir, "sounds/coin.toml", &fallback).unwrap();
        assert_eq!(second, Place::Folder(fallback.join("sounds/coin.toml.2")));
        assert_eq!(std::fs::read_to_string(fallback.join("sounds/coin.toml")).unwrap(), "one");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&fallback);
    }
}

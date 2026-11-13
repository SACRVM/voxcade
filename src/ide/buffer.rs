//! The open file's text as the IDE keeps it while it is changed. It is
//! saved by itself, a second after the last change (at once, through
//! `save_at_once`: a game watching the folder never reads half a file),
//! and kept meanwhile in a journal in the config folder, so a crash loses
//! nothing: opened again, the journal's text comes back. The file on disk
//! is looked at now and then, and right before every save: changed from
//! outside (the agent in the console), it comes in by itself when the text
//! has no changes of its own, else the IDE asks whose to keep.
//!
//! A person's text is never lost nor written over a newer file unasked:
//! while the question waits nothing is saved and the journal stays, so a
//! text let go of then (another file chosen, OPEN, the IDE closed) is
//! asked about again when it opens. The journal names the file's text it
//! was changed from (`JOURNAL`, a BLAKE3): its text comes back by itself
//! only onto that same text; onto a file changed since, whose to keep is
//! asked. It names the file's whole path too, and is never offered on
//! another (two games of one name once shared their journals, and KEEP
//! MINE would have written one's text over the other's); a journal of an
//! older IDE, which names no file, is never offered.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use super::code::Code;

/// How long after the last change the file is saved.
pub const SAVE_AFTER: Duration = Duration::from_secs(1);

/// A journal's first line: its format, then `base` and the BLAKE3 of the
/// file's text the journal's text was changed from, then `file` and the
/// file's whole path (to the line's end). The text follows on the next
/// line.
const JOURNAL: &str = "voxcade journal 2 base ";
const FILE: &str = " file ";

/// The journal of `text`, changed from the file's text of the hash `base`,
/// for the file at `file` (its whole path).
fn journal(base: &str, file: &Path, text: &str) -> String {
    format!("{JOURNAL}{base}{FILE}{}\n{text}", file.to_string_lossy())
}

/// A journal's base when the text it was changed from is not known (it
/// matches no file).
const UNKNOWN: &str = "unknown";

/// A journal read, if it is one of this IDE's for the file at `file` (its
/// whole path): the hash of the text it was changed from, and its text.
fn read_journal(written: &str, file: &Path) -> Option<(String, String)> {
    let (head, text) = written.strip_prefix(JOURNAL)?.split_once('\n')?;
    let (base, named) = head.split_once(FILE)?;
    (named == file.to_string_lossy()).then(|| (base.to_string(), text.to_string()))
}

/// The file `path` of the game folder `dir` by its whole path, as a
/// journal names it.
fn whole(dir: &Path, path: &str) -> PathBuf {
    let file = dir.join(path);
    file.canonicalize().or_else(|_| std::path::absolute(&file)).unwrap_or(file)
}

/// Where the journal of the file `path` lies in `journals`.
fn journal_path(journals: &Path, path: &str) -> PathBuf {
    journals.join(path.replace(['/', '\\'], "%"))
}

/// Whether the journal in `journals` of the file `path` of the game folder
/// `dir` holds a text the file does not (one not saved yet, or kept while
/// its file changed outside too).
pub fn holds_unsaved(dir: &Path, path: &str, journals: &Path) -> bool {
    let Ok(written) = std::fs::read_to_string(journal_path(journals, path)) else { return false };
    let Some((_, text)) = read_journal(&written, &whole(dir, path)) else { return false };
    std::fs::read_to_string(dir.join(path)).ok() != Some(text)
}

/// Leaves `text` in the journal of the file `path` of the game folder
/// `dir`, changed from a text not known (`ide --shot --journal`): opened,
/// whose to keep is asked.
pub fn leave(dir: &Path, path: &str, journals: &Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(journals).with_context(|| format!("making {}", journals.display()))?;
    crate::blocks::save_at_once(&journal_path(journals, path), journal(UNKNOWN, &whole(dir, path), text).as_bytes())
}

/// The journal of the file `from`, renamed `to` in the game folder `dir`
/// (the file already moved), goes with it, naming the new path; one that
/// was not the file's stays where it is.
pub fn rename_journal(dir: &Path, from: &str, to: &str, journals: &Path) {
    let old = journal_path(journals, from);
    let Ok(written) = std::fs::read_to_string(&old) else { return };
    let mut was = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    was.push(from);
    if let Some((base, text)) = read_journal(&written, &was)
        && crate::blocks::save_at_once(&journal_path(journals, to), journal(&base, &whole(dir, to), &text).as_bytes()).is_ok()
    {
        let _ = std::fs::remove_file(&old);
    }
}

fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

pub struct Buffer {
    file: PathBuf,
    /// The file's whole path, as its journal names it.
    whole: PathBuf,
    journal: PathBuf,
    pub code: Code,
    /// What the file on disk holds, as last read or written.
    on_disk: String,
    /// The file's time then.
    stamp: Option<SystemTime>,
    /// When the text changed with no save since.
    changed: Option<Instant>,
    /// The file's text changed from outside while this one had changes of
    /// its own: which to keep is asked.
    pub theirs: Option<String>,
    /// While that is asked: the hash of the file's text this one was
    /// changed from (`UNKNOWN` if not known), which its journal keeps
    /// saying, so that it is asked again after a crash too.
    asked_from: String,
    /// The text came back from the journal (a crash before it was saved).
    pub restored: bool,
    /// The file is gone from the folder (the agent, or anything outside the
    /// IDE, removed or renamed it): the text is kept here and nothing is
    /// written, so the file does not come back by itself; `put_back` saves
    /// it again.
    pub gone: bool,
}

impl Buffer {
    /// The file `path` of the game folder `dir`, its journal in `journals`;
    /// a journal's text that is not the file's comes back: saved by itself
    /// if the file is still the text it was changed from, else asked about
    /// (`theirs` the file's text), nothing saved until answered.
    pub fn open(dir: &Path, path: &str, journals: &Path) -> Result<Self> {
        let file = dir.join(path);
        let on_disk = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        let whole = whole(dir, path);
        let journal = journal_path(journals, path);
        let kept = std::fs::read_to_string(&journal).ok().and_then(|written| read_journal(&written, &whole)).filter(|(_, text)| *text != on_disk);
        let restored = kept.is_some();
        let (text, theirs, asked_from) = match kept {
            Some((base, text)) => {
                let same = base == hash(&on_disk);
                (text, (!same).then(|| on_disk.clone()), base)
            }
            None => (on_disk.clone(), None, UNKNOWN.into()),
        };
        let stamp = modified(&file);
        let changed = restored.then(Instant::now);
        Ok(Self { file, whole, journal, code: Code::new(path, text), on_disk, stamp, changed, theirs, asked_from, restored, gone: false })
    }

    /// Whether the text has changes not yet saved.
    pub fn unsaved(&self) -> bool {
        self.code.text != self.on_disk
    }

    /// The text changed: into the journal at once, into the file a second
    /// after the last change.
    pub fn edited(&mut self, now: Instant) {
        self.changed = Some(now);
        self.write_journal();
    }

    /// The text into the journal, with the file's text it was changed from
    /// (while a question waits, the one before the change outside).
    fn write_journal(&self) {
        if let Some(folder) = self.journal.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let base = if self.theirs.is_some() { self.asked_from.clone() } else { hash(&self.on_disk) };
        let _ = crate::blocks::save_at_once(&self.journal, journal(&base, &self.whole, &self.code.text).as_bytes());
    }

    /// The file changed outside to `text` while this one has changes of its
    /// own: whose to keep is asked.
    fn ask(&mut self, text: String) {
        if self.theirs.is_none() {
            self.asked_from = hash(&self.on_disk);
        }
        self.theirs = Some(text);
    }

    /// Saves when it is due (not while a change from outside waits for an
    /// answer); whether it did.
    pub fn tick(&mut self, now: Instant) -> Result<bool> {
        match self.changed {
            Some(at) if self.theirs.is_none() && !self.gone && now.duration_since(at) >= SAVE_AFTER => self.save().map(|()| true),
            _ => Ok(false),
        }
    }

    /// How long until it saves (None: nothing to save).
    pub fn due(&self, now: Instant) -> Option<Duration> {
        self.changed.filter(|_| self.theirs.is_none() && !self.gone).map(|at| SAVE_AFTER.saturating_sub(now.duration_since(at)))
    }

    /// Saves the text now if it has changes (before Play, on leaving it).
    /// The disk is looked at first: a file changed there since the last
    /// look is not written over but asked about. While that question waits
    /// (`theirs`), or the file is gone, nothing is written and the text
    /// stays, in the journal too, to be asked about when it opens again.
    pub fn save(&mut self) -> Result<()> {
        self.look_closely();
        if self.gone || self.theirs.is_some() {
            return Ok(());
        }
        self.changed = None;
        if self.unsaved() {
            crate::blocks::save_at_once(&self.file, self.code.text.as_bytes())?;
            self.on_disk = self.code.text.clone();
            self.stamp = modified(&self.file);
        }
        let _ = std::fs::remove_file(&self.journal);
        self.restored = false;
        Ok(())
    }

    /// SAVE AGAIN: writes the text of a file that is gone back into the
    /// folder, where it was.
    pub fn put_back(&mut self) -> Result<()> {
        if let Some(folder) = self.file.parent() {
            std::fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
        }
        crate::blocks::save_at_once(&self.file, self.code.text.as_bytes())?;
        self.on_disk = self.code.text.clone();
        self.stamp = modified(&self.file);
        self.gone = false;
        self.theirs = None;
        self.changed = None;
        let _ = std::fs::remove_file(&self.journal);
        self.restored = false;
        Ok(())
    }

    /// Looks whether the file changed on disk: it comes in when the text
    /// has no changes of its own, else it waits in `theirs`. Whether there
    /// is something new to show.
    pub fn look(&mut self) -> bool {
        let stamp = modified(&self.file);
        // Gone from the folder, or back (made again by the same name).
        let gone = stamp.is_none() && !self.file.exists();
        let flipped = gone != self.gone;
        self.gone = gone;
        if stamp == self.stamp {
            return flipped;
        }
        self.stamp = stamp;
        let Ok(text) = std::fs::read_to_string(&self.file) else { return flipped };
        if text == self.on_disk || self.theirs.as_ref() == Some(&text) {
            return flipped;
        }
        if text == self.code.text {
            // Written outside just as it is here: nothing to ask.
            self.on_disk = text;
            self.theirs = None;
        } else if self.unsaved() {
            self.ask(text);
        } else {
            self.code.text = text.clone();
            self.on_disk = text;
        }
        true
    }

    /// `look`, and the file's text compared too (a change within the same
    /// tick of the file's clock shows no new time).
    fn look_closely(&mut self) {
        self.look();
        if self.gone || self.theirs.is_some() {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&self.file) else { return };
        if text == self.on_disk {
            return;
        }
        self.stamp = modified(&self.file);
        if text == self.code.text {
            self.on_disk = text;
        } else if self.unsaved() {
            self.ask(text);
        } else {
            self.code.text = text.clone();
            self.on_disk = text;
        }
    }

    /// Keeps this text over the one changed from outside: saved over it
    /// (its journal now changed from that one, so a crash before the save
    /// brings it back without asking again).
    pub fn keep_mine(&mut self) {
        if let Some(theirs) = self.theirs.take() {
            self.on_disk = theirs;
            self.changed = Some(Instant::now());
            self.write_journal();
        }
    }

    /// Takes the text changed from outside, letting this one's changes go.
    pub fn take_theirs(&mut self) {
        if let Some(theirs) = self.theirs.take() {
            self.code.text = theirs.clone();
            self.on_disk = theirs;
            self.changed = None;
            self.restored = false;
            let _ = std::fs::remove_file(&self.journal);
        }
    }
}

fn modified(file: &Path) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> (PathBuf, PathBuf, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("buffer-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::write(dir.join("game/main.lua"), "function on_tick() end\n").unwrap();
        (dir.join("game"), dir.join("journal"), dir)
    }

    /// A change not yet saved is in the journal; the IDE gone before it
    /// saved, the file opened again has it back, and saves it.
    #[test]
    fn a_buffer_survives_a_crash_in_the_journal() {
        let (game, journals, _dir) = folder("crash");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        b.code.text = "function on_tick() player.state.score = 1 end\n".into();
        b.edited(Instant::now());
        drop(b);
        assert_eq!(std::fs::read_to_string(game.join("main.lua")).unwrap(), "function on_tick() end\n", "not saved yet");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert!(b.restored && b.unsaved());
        assert_eq!(b.code.text, "function on_tick() player.state.score = 1 end\n");
        assert!(b.tick(Instant::now() + SAVE_AFTER).unwrap(), "saved when due");
        assert_eq!(std::fs::read_to_string(game.join("main.lua")).unwrap(), b.code.text);
        assert!(!Buffer::open(&game, "main.lua", &journals).unwrap().restored, "the journal gone once saved");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    /// The file changed from outside comes in by itself while the text has
    /// no changes of its own; with changes, it waits for an answer, and
    /// nothing is saved over it meanwhile.
    #[test]
    fn an_outside_change_reloads_a_clean_buffer() {
        let (game, journals, _dir) = folder("outside");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        let write = |text: &str| {
            // Its time set ahead: written twice in a moment, a file may
            // keep its time.
            std::thread::sleep(Duration::from_millis(20));
            std::fs::write(game.join("main.lua"), text).unwrap();
            let t = std::fs::File::options().write(true).open(game.join("main.lua")).unwrap();
            t.set_modified(SystemTime::now() + Duration::from_secs(1)).unwrap();
        };
        write("-- theirs\n");
        assert!(b.look());
        assert_eq!((b.code.text.as_str(), b.theirs.is_none()), ("-- theirs\n", true), "taken in");
        b.code.text = "-- mine\n".into();
        b.edited(Instant::now());
        write("-- theirs again\n");
        assert!(b.look());
        assert_eq!(b.theirs.as_deref(), Some("-- theirs again\n"), "asked");
        assert!(!b.tick(Instant::now() + SAVE_AFTER).unwrap(), "nothing saved over theirs");
        b.keep_mine();
        assert!(b.tick(Instant::now() + SAVE_AFTER).unwrap());
        assert_eq!(std::fs::read_to_string(game.join("main.lua")).unwrap(), "-- mine\n");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    /// `text` written into `file` from outside, its time set ahead (written
    /// twice in a moment, a file may keep its time).
    fn write_outside(file: &Path, text: &str) {
        std::fs::write(file, text).unwrap();
        let t = std::fs::File::options().write(true).open(file).unwrap();
        t.set_modified(SystemTime::now() + Duration::from_secs(2)).unwrap();
    }

    fn read(file: &Path) -> String {
        std::fs::read_to_string(file).unwrap()
    }

    /// A text with changes of its own while the file changed outside too
    /// is let go of before the question is answered (another file chosen,
    /// OPEN, the IDE closed: all save it first): nothing is written, the
    /// journal keeps it, and opened again both are there and the question
    /// is asked again. Either answer then holds.
    #[test]
    fn a_text_let_go_of_while_asked_is_asked_again() {
        let (game, journals, _dir) = folder("let-go");
        let file = game.join("main.lua");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        b.code.text = "-- mine\n".into();
        b.edited(Instant::now());
        write_outside(&file, "-- theirs\n");
        assert!(b.look() && b.theirs.is_some(), "asked");
        // Typed on while asked.
        b.code.text = "-- mine\n-- more\n".into();
        b.edited(Instant::now());
        b.save().unwrap();
        drop(b);
        assert_eq!(read(&file), "-- theirs\n", "nothing written over theirs");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert_eq!((b.code.text.as_str(), b.theirs.as_deref()), ("-- mine\n-- more\n", Some("-- theirs\n")), "both kept, asked again");
        assert!(!b.tick(Instant::now() + SAVE_AFTER * 2).unwrap() && b.due(Instant::now()).is_none(), "nothing saved while asked");
        // Typed on, let go of again: still both.
        b.code.text = "-- mine\n".into();
        b.edited(Instant::now());
        b.save().unwrap();
        drop(b);
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert_eq!((b.code.text.as_str(), b.theirs.as_deref()), ("-- mine\n", Some("-- theirs\n")));
        b.take_theirs();
        assert!(!b.restored && !b.unsaved());
        drop(b);
        let b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert!(b.theirs.is_none() && !b.restored && b.code.text == "-- theirs\n", "taken: the journal gone");
        drop(b);
        // KEEP MINE: saved over theirs.
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        b.code.text = "-- mine again\n".into();
        b.edited(Instant::now());
        write_outside(&file, "-- theirs again\n");
        b.save().unwrap();
        drop(b);
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert_eq!(b.theirs.as_deref(), Some("-- theirs again\n"));
        b.keep_mine();
        assert!(b.tick(Instant::now() + SAVE_AFTER).unwrap());
        assert_eq!(read(&file), "-- mine again\n");
        drop(b);
        assert!(!Buffer::open(&game, "main.lua", &journals).unwrap().restored, "no journal once saved");
    }

    /// After a crash the journal's text comes back by itself only onto the
    /// file's text it was changed from; the file changed since (the agent,
    /// git, another editor), whose to keep is asked and nothing is written
    /// over it. A journal from an older IDE, which does not say what it was
    /// changed from, is asked about too.
    #[test]
    fn a_journal_onto_a_file_changed_since_asks() {
        let (game, journals, _dir) = folder("crash-then-outside");
        let file = game.join("main.lua");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        b.code.text = "-- mine\n".into();
        b.edited(Instant::now());
        drop(b);
        write_outside(&file, "-- changed since\n");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert!(b.restored);
        assert_eq!((b.code.text.as_str(), b.theirs.as_deref()), ("-- mine\n", Some("-- changed since\n")), "asked");
        assert!(!b.tick(Instant::now() + SAVE_AFTER * 2).unwrap());
        assert_eq!(read(&file), "-- changed since\n", "not written over");
        // Typed on while asked, and another crash: still asked.
        b.code.text = "-- mine, more\n".into();
        b.edited(Instant::now());
        drop(b);
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert_eq!((b.code.text.as_str(), b.theirs.as_deref()), ("-- mine, more\n", Some("-- changed since\n")), "asked again");
        // KEEP MINE, and a crash before it is saved: back without asking.
        b.keep_mine();
        drop(b);
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert!(b.restored && b.theirs.is_none() && b.code.text == "-- mine, more\n");
        assert!(b.tick(Instant::now() + SAVE_AFTER).unwrap());
        assert_eq!(read(&file), "-- mine, more\n");
        drop(b);
        // A journal left unasked (`ide --shot --journal`): asked about.
        leave(&game, "main.lua", &journals, "-- left\n").unwrap();
        let b = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert_eq!((b.code.text.as_str(), b.theirs.as_deref()), ("-- left\n", Some("-- mine, more\n")));
    }

    /// A journal names its file's whole path and is offered on that file
    /// alone: two games of one folder name that shared their journals (an
    /// older IDE's, by the name) never get each other's text, and KEEP
    /// MINE can never write one's over the other's. An older IDE's
    /// journal names no file: never offered. RENAME takes the journal
    /// along, naming the new path.
    #[test]
    fn a_journal_is_never_offered_on_another_path() {
        let (game, journals, dir) = folder("two-paths");
        let other = dir.join("elsewhere").join("game");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("main.lua"), "function on_tick() end\n").unwrap();
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        b.code.text = "-- this game's\n".into();
        b.edited(Instant::now());
        drop(b);
        assert!(read(&journals.join("main.lua")).starts_with(&format!("voxcade journal 2 base {} file {}\n", hash("function on_tick() end\n"), game.canonicalize().unwrap().join("main.lua").display())));
        // The other folder's main.lua, its journals in the same place.
        let theirs = Buffer::open(&other, "main.lua", &journals).unwrap();
        assert!(!theirs.restored && theirs.theirs.is_none() && theirs.code.text == "function on_tick() end\n", "another path's journal offered");
        assert!(!holds_unsaved(&other, "main.lua", &journals) && holds_unsaved(&game, "main.lua", &journals));
        drop(theirs);
        let mine = Buffer::open(&game, "main.lua", &journals).unwrap();
        assert!(mine.restored && mine.code.text == "-- this game's\n", "its own path's journal");
        drop(mine);
        // Renamed: the journal goes along and names the new path.
        std::fs::rename(game.join("main.lua"), game.join("start.lua")).unwrap();
        rename_journal(&game, "main.lua", "start.lua", &journals);
        assert!(!journals.join("main.lua").exists());
        let moved = Buffer::open(&game, "start.lua", &journals).unwrap();
        assert!(moved.restored && moved.code.text == "-- this game's\n" && moved.theirs.is_none());
        drop(moved);
        // Older IDEs' journals: by the name only, never offered.
        for old in ["-- from an older IDE\n".to_string(), format!("voxcade journal 1 base {}\n-- version 1\n", hash("function on_tick() end\n"))] {
            std::fs::write(journals.join("main.lua"), &old).unwrap();
            let b = Buffer::open(&other, "main.lua", &journals).unwrap();
            assert!(!b.restored && b.theirs.is_none() && b.code.text == "function on_tick() end\n", "{old}");
            assert!(!holds_unsaved(&other, "main.lua", &journals));
        }
    }

    /// A save looks at the disk first: a change there that the last look
    /// did not see yet is asked about, not written over.
    #[test]
    fn a_save_does_not_write_over_a_change_not_seen_yet() {
        let (game, journals, _dir) = folder("unseen");
        let file = game.join("main.lua");
        let mut b = Buffer::open(&game, "main.lua", &journals).unwrap();
        b.code.text = "-- mine\n".into();
        b.edited(Instant::now());
        // Written with the time it had: only the text tells.
        let stamp = modified(&file).unwrap();
        std::fs::write(&file, "-- theirs\n").unwrap();
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(stamp).unwrap();
        b.tick(Instant::now() + SAVE_AFTER).unwrap();
        assert_eq!(read(&file), "-- theirs\n");
        assert_eq!(b.theirs.as_deref(), Some("-- theirs\n"), "asked");
    }
}

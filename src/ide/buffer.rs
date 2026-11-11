//! The open file's text as the IDE keeps it while it is changed. It is
//! saved by itself, a second after the last change (at once, through
//! `save_at_once`: a game watching the folder never reads half a file),
//! and kept meanwhile in a journal in the config folder, so a crash loses
//! nothing: opened again, the journal's text comes back. The file on disk
//! is looked at now and then: changed from outside (the agent in the
//! console), it comes in by itself when the text has no changes of its
//! own, else the IDE asks whose to keep.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use super::code::Code;

/// How long after the last change the file is saved.
pub const SAVE_AFTER: Duration = Duration::from_secs(1);

pub struct Buffer {
    file: PathBuf,
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
    /// a journal's text that is not the file's comes back.
    pub fn open(dir: &Path, path: &str, journals: &Path) -> Result<Self> {
        let file = dir.join(path);
        let on_disk = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        let journal = journals.join(path.replace(['/', '\\'], "%"));
        let kept = std::fs::read_to_string(&journal).ok().filter(|j| *j != on_disk);
        let restored = kept.is_some();
        let text = kept.unwrap_or_else(|| on_disk.clone());
        let stamp = modified(&file);
        let changed = restored.then(Instant::now);
        Ok(Self { file, journal, code: Code::new(path, text), on_disk, stamp, changed, theirs: None, restored, gone: false })
    }

    /// Whether the text has changes not yet saved.
    pub fn unsaved(&self) -> bool {
        self.code.text != self.on_disk
    }

    /// The text changed: into the journal at once, into the file a second
    /// after the last change.
    pub fn edited(&mut self, now: Instant) {
        self.changed = Some(now);
        if let Some(folder) = self.journal.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = crate::blocks::save_at_once(&self.journal, self.code.text.as_bytes());
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
    pub fn save(&mut self) -> Result<()> {
        // (A file that is gone is not written: its text stays in the journal.)
        if self.gone {
            return Ok(());
        }
        self.changed = None;
        if self.unsaved() && self.theirs.is_none() {
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
        if self.unsaved() {
            self.theirs = Some(text);
        } else {
            self.code.text = text.clone();
            self.on_disk = text;
        }
        true
    }

    /// Keeps this text over the one changed from outside: saved over it.
    pub fn keep_mine(&mut self) {
        if let Some(theirs) = self.theirs.take() {
            self.on_disk = theirs;
            self.changed = Some(Instant::now());
        }
    }

    /// Takes the text changed from outside, letting this one's changes go.
    pub fn take_theirs(&mut self) {
        if let Some(theirs) = self.theirs.take() {
            self.code.text = theirs.clone();
            self.on_disk = theirs;
            self.changed = None;
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

    fn folder(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("voxcade-buffer-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::write(dir.join("game/main.lua"), "function on_tick() end\n").unwrap();
        (dir.join("game"), dir.join("journal"))
    }

    /// A change not yet saved is in the journal; the IDE gone before it
    /// saved, the file opened again has it back, and saves it.
    #[test]
    fn a_buffer_survives_a_crash_in_the_journal() {
        let (game, journals) = folder("crash");
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
        let (game, journals) = folder("outside");
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
}

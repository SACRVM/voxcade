//! Polls the game folder and its subfolders for changed files. Polling
//! instead of OS file events: works everywhere, needs no inotify watches,
//! and a file only counts as changed once it has stopped changing (editors
//! write in steps).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

const INTERVAL: Duration = Duration::from_millis(250);
/// How many folders down from the game folder files are watched.
const DEPTH: u32 = 4;

pub struct FolderWatch {
    dir: PathBuf,
    known: HashMap<PathBuf, SystemTime>,
    /// Files seen with a new timestamp, waiting for one quiet poll.
    settling: HashMap<PathBuf, SystemTime>,
    last_poll: Instant,
}

impl FolderWatch {
    pub fn new(dir: &Path) -> Self {
        let mut watch = Self {
            dir: dir.to_path_buf(),
            known: HashMap::new(),
            settling: HashMap::new(),
            last_poll: Instant::now(),
        };
        watch.known = watch.scan();
        watch
    }

    fn scan(&self) -> HashMap<PathBuf, SystemTime> {
        let mut found = HashMap::new();
        scan_dir(&self.dir, DEPTH, &mut found);
        found
    }

    /// Files that changed and have been stable since the previous poll,
    /// and files gone since (a sound taken away: the reload sees it is
    /// no more).
    pub fn poll(&mut self) -> Vec<PathBuf> {
        if self.last_poll.elapsed() < INTERVAL {
            return Vec::new();
        }
        self.last_poll = Instant::now();
        let now = self.scan();
        let mut gone: Vec<PathBuf> = self.known.keys().filter(|p| !now.contains_key(*p)).cloned().collect();
        gone.sort();
        for path in &gone {
            self.known.remove(path);
            self.settling.remove(path);
        }
        let mut changed = gone;
        for (path, modified) in now {
            if self.known.get(&path) == Some(&modified) {
                self.settling.remove(&path);
                continue;
            }
            if self.settling.get(&path) == Some(&modified) {
                self.settling.remove(&path);
                self.known.insert(path.clone(), modified);
                changed.push(path);
            } else {
                self.settling.insert(path, modified);
            }
        }
        changed
    }
}

fn scan_dir(dir: &Path, depth: u32, found: &mut HashMap<PathBuf, SystemTime>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if meta.is_dir() && depth > 0 && !hidden {
            scan_dir(&entry.path(), depth - 1, found);
        } else if meta.is_file()
            && let Ok(modified) = meta.modified()
        {
            found.insert(entry.path(), modified);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file new, changed (once it stopped changing) or gone is told.
    #[test]
    fn a_file_gone_is_told_too() {
        let dir = std::env::temp_dir().join(format!("voxcade-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        std::fs::write(dir.join("sounds/zap.toml"), "freq = 600\n").unwrap();
        let mut watch = FolderWatch::new(&dir);
        let poll = |watch: &mut FolderWatch| {
            watch.last_poll = Instant::now() - INTERVAL;
            watch.poll()
        };
        assert!(poll(&mut watch).is_empty());
        std::fs::remove_file(dir.join("sounds/zap.toml")).unwrap();
        assert_eq!(poll(&mut watch), [dir.join("sounds/zap.toml")]);
        assert!(poll(&mut watch).is_empty(), "told once");
        std::fs::write(dir.join("sounds/zap.toml"), "freq = 300\n").unwrap();
        assert!(poll(&mut watch).is_empty(), "settling");
        assert_eq!(poll(&mut watch), [dir.join("sounds/zap.toml")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

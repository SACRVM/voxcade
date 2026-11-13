//! Save games' slots (`docs/game-model.md`, "Save games"): in the player's
//! config folder (`saves/<game>/N.toml`, never in the game folder, like the
//! settings and the high scores), or kept for one run only (`shot`, `check`,
//! `inspect`, a replay played back), so tools never write the player's
//! slots. A save is the game's own text (`Game::save`); the engine reads
//! three lines of it: the level, the play time in ticks, and when it was
//! made (for the menu only: a game never sees the clock).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// What a slot shows: the level, the play time, when (seconds since 1970;
/// 0 where nobody knows).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SlotInfo {
    pub level: String,
    pub ticks: u64,
    #[serde(default)]
    pub when: u64,
}

/// A game's slots.
#[derive(Clone, Debug, Default)]
pub struct Slots {
    /// Where they lie; None: this run's only.
    dir: Option<PathBuf>,
    count: u32,
    texts: BTreeMap<u32, Arc<str>>,
    /// A replay's loads: what its game read, load after load, whatever the
    /// slots hold (`replaying`).
    replayed: Vec<Arc<str>>,
    /// The slots whose files lie on disk but do not read (`unreadable`).
    broken: Vec<u32>,
}

/// Where a game's saves lie: `voxcade/saves/<game>/` beside its settings.
pub fn dir_for(game_dir: &Path) -> PathBuf {
    crate::settings::config_dir().join("saves").join(crate::settings::game_name(game_dir))
}

impl Slots {
    /// `count` slots for this run only, empty.
    pub fn for_run(count: u32) -> Self {
        Self { count, ..Self::default() }
    }

    /// Slots for this run only, holding these texts (a replay's).
    pub fn holding(texts: BTreeMap<u32, String>) -> Self {
        Self { texts: texts.into_iter().map(|(n, t)| (n, Arc::from(t))).collect(), ..Self::default() }
    }

    /// With a replay's loads, read in turn.
    pub fn replaying(mut self, loads: &[String]) -> Self {
        self.replayed = loads.iter().map(|t| Arc::from(t.as_str())).collect();
        self
    }

    /// A replay's loads (`replaying`).
    pub fn replayed(&self) -> &[Arc<str>] {
        &self.replayed
    }

    /// The player's slots of the game in `game_dir`, as they lie on disk;
    /// one that does not read is empty (`unreadable` lists them).
    pub fn on_disk(game_dir: &Path) -> Self {
        Self::on_disk_in(dir_for(game_dir))
    }

    /// The slots lying in `dir`.
    fn on_disk_in(dir: PathBuf) -> Self {
        let mut texts = BTreeMap::new();
        let mut broken = Vec::new();
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Some(slot) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u32>().ok()) else { continue };
            if path.extension().is_none_or(|e| e != "toml") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) if info_of(&text).is_some() => {
                    texts.insert(slot, Arc::from(text));
                }
                _ => {
                    eprintln!("saves: {} is no save", path.display());
                    broken.push(slot);
                }
            }
        }
        broken.sort_unstable();
        Self { dir: Some(dir), texts, broken, ..Self::default() }
    }

    /// The slots (of the game's, 1 to `count`) whose files lie on disk but
    /// do not read: shown as empty, and the window says so.
    pub fn unreadable(&self) -> Vec<u32> {
        self.broken.iter().copied().filter(|n| (1..=self.count).contains(n)).collect()
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    /// The game's slots are 1 to `count` (`c.saves.slots`); what lies
    /// beyond is not shown.
    pub fn set_count(&mut self, count: u32) {
        self.count = count;
    }

    /// What lies in each of the game's slots, from 1.
    pub fn listing(&self) -> Vec<Option<SlotInfo>> {
        (1..=self.count).map(|slot| self.info(slot)).collect()
    }

    /// Whether they lie on disk (the window's), not for one run only.
    pub fn on_disk_now(&self) -> bool {
        self.dir.is_some()
    }

    /// Each filled slot's first lines (level, play time, when): what a
    /// replay keeps of the slots as its run began.
    pub fn headers(&self) -> BTreeMap<u32, String> {
        self.texts.keys().filter_map(|&slot| Some((slot, toml::to_string(&self.info(slot)?).ok()?))).collect()
    }

    /// The slot holding the newest save (the title's CONTINUE), if any.
    pub fn newest(&self) -> Option<u32> {
        (1..=self.count).filter_map(|slot| Some((self.info(slot)?.when, slot))).max().map(|(_, slot)| slot)
    }

    /// What lies in `slot`, if anything.
    pub fn info(&self, slot: u32) -> Option<SlotInfo> {
        self.texts.get(&slot).and_then(|t| info_of(t))
    }

    /// The save in `slot`.
    pub fn read(&self, slot: u32) -> Option<&str> {
        self.texts.get(&slot).map(|t| &**t)
    }

    /// The game's slots' saves, by their number (the game reads its loads
    /// there).
    pub fn texts(&self) -> BTreeMap<u32, Arc<str>> {
        self.texts.iter().filter(|(n, _)| (1..=self.count).contains(*n)).map(|(n, t)| (*n, t.clone())).collect()
    }

    /// Puts `text` (a game's save) into `slot`, stamped with `when`; on
    /// disk too where the slots lie there.
    pub fn write(&mut self, slot: u32, text: &str, when: u64) -> Result<()> {
        anyhow::ensure!((1..=self.count).contains(&slot), "save slot {slot}: the game has slots 1 to {}", self.count);
        let text = format!("when = {when}\n{text}");
        if let Some(dir) = &self.dir {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            let path = dir.join(format!("{slot}.toml"));
            // Written beside and moved over: a crash leaves the old save.
            let fresh = dir.join(format!(".{slot}.toml"));
            std::fs::write(&fresh, &text).with_context(|| format!("writing {}", fresh.display()))?;
            std::fs::rename(&fresh, &path).with_context(|| format!("writing {}", path.display()))?;
        }
        self.texts.insert(slot, Arc::from(text));
        Ok(())
    }
}

/// The lines of a save the engine reads.
fn info_of(text: &str) -> Option<SlotInfo> {
    toml::from_str(text).ok()
}

/// Seconds since 1970, for a save's `when` (the menu's; never the game's).
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slot holds the game's text with when it was made; this run's
    /// slots stay off the disk; a slot outside the game's is a mistake.
    #[test]
    fn slots_hold_saves() {
        let mut slots = Slots::for_run(2);
        assert_eq!(slots.info(1), None);
        slots.write(1, "level = \"hall\"\nticks = 600\n[states]\nworld = '{}'\n", 1_761_000_000).unwrap();
        assert_eq!(slots.info(1), Some(SlotInfo { level: "hall".into(), ticks: 600, when: 1_761_000_000 }));
        assert!(slots.read(1).unwrap().contains("[states]"));
        assert!(slots.write(3, "", 0).unwrap_err().to_string().contains("slots 1 to 2"));
        assert_eq!(slots.newest(), Some(1));
        slots.write(2, "level = \"tower\"\nticks = 60\n[states]\nworld = '{}'\n", 1_761_000_100).unwrap();
        assert_eq!(slots.newest(), Some(2), "the newest save, whatever its slot");
        assert_eq!(Slots::for_run(2).newest(), None);
    }

    /// A save file that does not read is an empty slot, but never a silent
    /// one: the slots name it (the window says so on its screen).
    #[test]
    fn an_unreadable_save_is_named() {
        let dir = crate::scratch::Scratch::new("saves");
        std::fs::write(dir.join("1.toml"), "level = \"hall\"\nticks = 6\n").unwrap();
        std::fs::write(dir.join("2.toml"), "this is { not a save").unwrap();
        std::fs::write(dir.join("9.toml"), "nor this").unwrap();
        let mut slots = Slots::on_disk_in(dir.to_path_buf());
        slots.set_count(3);
        assert_eq!(slots.info(1).map(|i| i.ticks), Some(6));
        assert_eq!(slots.info(2), None);
        assert_eq!(slots.unreadable(), [2], "slot 9 is beyond the game's slots");
    }

    /// A save cut short (no play time), empty or not text at all is no
    /// save either, named as one; files that are no slot's are not looked at.
    #[test]
    fn a_truncated_save_is_named_too() {
        let dir = crate::scratch::Scratch::new("saves-cut");
        std::fs::write(dir.join("1.toml"), "when = 5\nlevel = \"hall\"\n").unwrap();
        std::fs::write(dir.join("2.toml"), "").unwrap();
        std::fs::write(dir.join("3.toml"), [0xff, 0xfe, 0x00, 0x41]).unwrap();
        std::fs::write(dir.join("notes.toml"), "level = \"x\"\nticks = 1\n").unwrap();
        std::fs::write(dir.join("4.txt"), "level = \"x\"\nticks = 1\n").unwrap();
        let mut slots = Slots::on_disk_in(dir.to_path_buf());
        slots.set_count(4);
        assert_eq!(slots.unreadable(), [1, 2, 3]);
        assert_eq!(slots.listing(), vec![None; 4]);
        assert_eq!(slots.newest(), None);
    }

    /// Each game keeps its saves in a folder of its own name: a save of
    /// another game is never among its slots.
    #[test]
    fn a_save_of_another_game_is_not_among_its_slots() {
        let dir = crate::scratch::Scratch::new("saves-games");
        let (a, b) = (dir.join("farm"), dir.join("castle"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        assert_ne!(dir_for(&a), dir_for(&b));
        assert_eq!(dir_for(&a).file_name(), Some(std::ffi::OsStr::new("farm")));
        let saves = dir.join("saves");
        let mut farm = Slots::on_disk_in(saves.join("farm"));
        farm.set_count(2);
        farm.write(1, "level = \"meadow\"\nticks = 60\n", 7).unwrap();
        let mut castle = Slots::on_disk_in(saves.join("castle"));
        castle.set_count(2);
        assert_eq!(castle.listing(), vec![None, None], "the farm's save is no castle's");
        assert_eq!(Slots::on_disk_in(saves.join("farm")).info(1).map(|i| i.level), Some("meadow".to_string()));
    }

    /// A save that cannot be written (its folder is a file) is a mistake
    /// naming the place, and the slot keeps what it held.
    #[test]
    fn a_save_that_cannot_be_written_keeps_the_slot() {
        let dir = crate::scratch::Scratch::new("saves-blocked");
        let blocked = dir.join("game");
        std::fs::write(&blocked, "a file where the folder should be").unwrap();
        let mut slots = Slots::on_disk_in(blocked.clone());
        slots.set_count(1);
        let e = format!("{:#}", slots.write(1, "level = \"hall\"\nticks = 6\n", 1).unwrap_err());
        assert!(e.contains(&blocked.display().to_string()), "{e}");
        assert_eq!(slots.info(1), None, "nothing kept that is not on disk");
    }
}

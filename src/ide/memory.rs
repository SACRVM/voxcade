//! What the IDE remembers of a game between two sessions: what was open,
//! where each file was scrolled to and where its cursor stood. It lives
//! in the user's config folder (`ide/<game>.toml`), never in the game
//! folder, like the settings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    /// The file shown in the main area (its path in the game folder).
    pub open: Option<String>,
    /// How far each file was scrolled down, in points.
    pub scroll: BTreeMap<String, f32>,
    /// Where the cursor stood in each text, in characters from its start.
    pub cursor: BTreeMap<String, usize>,
    /// The console folded away to its heading. A game opened for the first
    /// time (no memory of it yet) has it folded: the IDE starts it so
    /// (`Shell::new`); what the user does with it is kept from then on.
    pub console_folded: bool,
    /// How high the user dragged the console, in points (None: as the IDE
    /// first sets it).
    pub console_height: Option<f32>,
}

impl Memory {
    /// Where the IDE keeps what it remembers of the game in `game_dir`.
    pub fn path_for(game_dir: &Path) -> PathBuf {
        crate::settings::config_dir().join("ide").join(format!("{}.toml", crate::settings::game_name(game_dir)))
    }

    /// What was remembered at `path`; nothing if there is nothing there
    /// or it cannot be read.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    /// Writes it to `path` at once (a whole file or none).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::blocks::save_at_once(path, toml::to_string(self)?.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What was open, how far it was scrolled and where the cursor stood
    /// come back as they were.
    #[test]
    fn the_ide_remembers_what_was_open() {
        let scratch = crate::scratch::Scratch::new("ide-memory");
        let path = scratch.join("game.toml");
        assert_eq!(Memory::load(&path), Memory::default(), "nothing yet");
        let mut memory = Memory { open: Some("main.lua".into()), ..Memory::default() };
        memory.scroll.insert("main.lua".into(), 120.0);
        memory.cursor.insert("main.lua".into(), 42);
        memory.save(&path).unwrap();
        assert_eq!(Memory::load(&path), memory);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

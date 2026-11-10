//! The IDE's settings (the gear at the top right), the same for every
//! game: the console shown or not at all, the agent it starts and how
//! (`claude.rs`), where NEW puts games; and the games opened last (the
//! menu on the game's name, the start page). Kept in the config folder
//! (`ide/settings/ide.toml`, a folder of their own: a game's memory beside
//! them is named for the game, `ide/<game>.toml`), never in a game folder;
//! more will come.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::claude::Claude;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// The console below the main area (off: none at all, no shell
    /// started; for those who need no agent).
    pub console: bool,
    /// The agent the console starts.
    pub agent: Agent,
    pub claude: Claude,
    /// Where NEW puts games (None: `Voxcade` at home).
    pub projects: Option<PathBuf>,
    /// The games opened, the last first.
    pub recent: Vec<Recent>,
    /// Whether the launcher looks for a new version when it opens (it
    /// asks our site for a small file; nothing is fetched unasked).
    pub updates: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { console: true, agent: Agent::Claude, claude: Claude::default(), projects: None, recent: Vec::new(), updates: true }
    }
}

/// A game opened: its folder and its title then.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub dir: PathBuf,
    pub title: String,
}

/// How many games the list of those opened last keeps.
const RECENT: usize = 10;

/// An agent working in the console (Claude Code, for now).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    Claude,
}

impl Agent {
    pub const ALL: [Agent; 1] = [Agent::Claude];

    /// Its name in the settings.
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "CLAUDE CODE",
        }
    }

    /// Its name in a sentence.
    pub fn called(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
        }
    }

    /// The console's button that starts it.
    pub fn button(self) -> &'static str {
        match self {
            Agent::Claude => "CLAUDE",
        }
    }
}

impl Prefs {
    /// Where the IDE keeps them.
    pub fn path() -> PathBuf {
        crate::settings::config_dir().join("ide").join("settings").join("ide.toml")
    }

    /// The settings at `path`: the defaults when there are none yet (with
    /// Claude's choices from before they were here, `ide/claude.toml`); an
    /// error when the file is there but cannot be read (it is left as it
    /// is, not written over).
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("{} (left as it is)", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let before = path.parent().and_then(Path::parent).map(|ide| ide.join("claude.toml"));
                let claude = before.and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| toml::from_str(&t).ok()).unwrap_or_default();
                Ok(Self { claude, ..Self::default() })
            }
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::blocks::save_at_once(path, toml::to_string(self)?.as_bytes())
    }

    /// Where NEW puts games.
    pub fn projects_dir(&self) -> PathBuf {
        self.projects.clone().unwrap_or_else(|| {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map_or_else(|| PathBuf::from("."), PathBuf::from);
            home.join("Voxcade")
        })
    }

    /// Notes the game in `dir` called `title` as opened now: first in the
    /// list, once.
    pub fn opened(&mut self, dir: &Path, title: &str) {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        self.recent.retain(|r| r.dir != dir);
        self.recent.insert(0, Recent { dir, title: title.to_string() });
        self.recent.truncate(RECENT);
    }

    /// The games opened last that are still there, but the one in
    /// `except`.
    pub fn recent_games(&self, except: Option<&Path>) -> Vec<Recent> {
        let except = except.and_then(|d| d.canonicalize().ok());
        self.recent.iter().filter(|r| Some(&r.dir) != except.as_ref() && r.dir.join("conf.lua").is_file()).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_come_back() {
        let path = std::env::temp_dir().join(format!("voxcade-prefs-{}", std::process::id())).join("settings").join("ide.toml");
        assert_eq!(Prefs::load(&path).unwrap(), Prefs::default());
        let mut prefs = Prefs { console: false, ..Prefs::default() };
        prefs.claude.with_ide = true;
        prefs.claude.sessions.insert("voxkeep".into(), "x".into());
        prefs.opened(Path::new("games/voxkeep"), "Voxkeep");
        prefs.save(&path).unwrap();
        assert_eq!(Prefs::load(&path).unwrap(), prefs);
        // A file that cannot be read is an error, not the defaults (which
        // would be written over it).
        std::fs::write(&path, "console = [").unwrap();
        assert!(Prefs::load(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    /// Claude's choices from before the settings were here come with them.
    #[test]
    fn claudes_choices_from_before_come_along() {
        let ide = std::env::temp_dir().join(format!("voxcade-prefs-before-{}", std::process::id()));
        std::fs::create_dir_all(&ide).unwrap();
        std::fs::write(ide.join("claude.toml"), "with_ide = true\nresume = true\nconnect = true\n[sessions]\nvoxkeep = \"81fd2a14\"\n").unwrap();
        let prefs = Prefs::load(&ide.join("settings").join("ide.toml")).unwrap();
        assert!(prefs.claude.with_ide && prefs.console);
        assert_eq!(prefs.claude.sessions.get("voxkeep").map(String::as_str), Some("81fd2a14"));
        let _ = std::fs::remove_dir_all(&ide);
    }

    /// The games opened last: the newest first, each once, those gone left
    /// out, and the one open.
    #[test]
    fn the_games_opened_last_are_listed() {
        let mut prefs = Prefs::default();
        prefs.opened(Path::new("games/voxkeep"), "Voxkeep");
        prefs.opened(Path::new("games/sokovox"), "Sokovox");
        prefs.opened(Path::new("games/voxkeep"), "Voxkeep");
        prefs.opened(Path::new("games/gone"), "Gone");
        let titles = |list: Vec<Recent>| list.into_iter().map(|r| r.title).collect::<Vec<_>>();
        assert_eq!(titles(prefs.recent_games(None)), ["Voxkeep", "Sokovox"]);
        assert_eq!(titles(prefs.recent_games(Some(Path::new("games/voxkeep")))), ["Sokovox"]);
        assert_eq!(prefs.recent.len(), 3, "the one gone stays in the list");
    }
}

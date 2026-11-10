//! How Claude Code starts in the IDE's console (in the IDE's settings,
//! `prefs.rs`): with the IDE or on CLAUDE, going on with the game's last
//! conversation or a new one, connected to the IDE through `voxcade mcp`
//! or not; and each game's conversation by its session id (Claude Code
//! keeps the conversation itself, under `~/.claude/projects/`). It is told
//! its part (`brief`): the game in its folder, Voxcade the program it
//! uses; a game inside the engine's own source does not see the engine's
//! notes for its developers, nor change the engine.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Claude {
    /// Claude starts in the console when the IDE opens.
    pub with_ide: bool,
    /// Claude goes on with the game's last conversation (else a new one
    /// each time).
    pub resume: bool,
    /// Claude sees the IDE: its tools through `voxcade mcp`, named on the
    /// command line (no file in the game folder, no question to trust it).
    pub connect: bool,
    /// Each game's conversation, by the game folder's path (two games may
    /// have folders of the same name).
    pub sessions: BTreeMap<String, String>,
    /// Claude Code's own folder (`claude_dir`; a test's own).
    #[serde(skip)]
    root: Option<PathBuf>,
}

impl Default for Claude {
    fn default() -> Self {
        Self { with_ide: false, resume: true, connect: true, sessions: BTreeMap::new(), root: claude_dir() }
    }
}

impl Claude {
    /// The conversation Claude goes on with for the game in `dir`: the one
    /// remembered, else the newest Claude Code kept in the game folder
    /// (begun there by hand before), if any.
    pub fn session(&self, dir: &Path) -> Option<String> {
        self.sessions.get(&key(dir)).filter(|id| self.kept(id)).cloned().or_else(|| self.newest(dir))
    }

    /// What starts Claude for the game in `dir` called `title`, typed into
    /// the console's shell: going on with its conversation (when asked and
    /// there is one) or a new one (`fresh`: always), remembered; the IDE's
    /// tools from `mcp`, a file beside the choices (`connect`).
    pub fn command(&mut self, dir: &Path, title: &str, fresh: bool, mcp: Option<&Path>, briefing: &Briefing) -> String {
        let game = key(dir);
        let mut words = vec!["claude".to_string()];
        match self.session(dir).filter(|_| self.resume && !fresh) {
            Some(id) => {
                self.sessions.insert(game, id.clone());
                words.extend(["--resume".into(), id]);
            }
            None => {
                // A conversation never begun (Claude started, nothing said)
                // keeps its id.
                let id = match self.sessions.get(&game) {
                    Some(id) if !fresh && !self.kept(id) => id.clone(),
                    _ => session_id(),
                };
                self.sessions.insert(game, id.clone());
                // (The title as plain words: no shell reads more into it.)
                let name: String = title.chars().filter(|c| c.is_alphanumeric() || " -_.,!".contains(*c)).collect();
                words.extend(["--session-id".into(), id, "--name".into(), quote(name.trim())]);
            }
        }
        if let Some(file) = &briefing.prompt {
            words.extend(["--append-system-prompt-file".into(), quote(&file.to_string_lossy())]);
        }
        if let Some(file) = &briefing.settings {
            words.extend(["--settings".into(), quote(&file.to_string_lossy())]);
        }
        if let Some(file) = mcp.filter(|_| self.connect) {
            words.extend(["--mcp-config".into(), quote(&file.to_string_lossy())]);
        }
        words.join(" ")
    }

    /// The newest conversation Claude Code kept for the folder `dir` (its
    /// projects' folders are named for the path, every sign but letters and
    /// digits a dash).
    fn newest(&self, dir: &Path) -> Option<String> {
        let root = self.root.as_ref()?;
        let path = dir.canonicalize().ok()?;
        let name: String = path.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        let files = std::fs::read_dir(root.join("projects").join(name)).ok()?.flatten();
        let conversations = files.filter(|f| f.path().extension().is_some_and(|e| e == "jsonl")).filter_map(|f| Some((f.metadata().ok()?.modified().ok()?, f.path())));
        let (_, newest) = conversations.max()?;
        Some(newest.file_stem()?.to_string_lossy().into_owned()).filter(|id| self.kept(id))
    }

    /// Whether Claude Code kept the conversation `id` (its file is in one
    /// of its projects' folders).
    fn kept(&self, id: &str) -> bool {
        // An id as Claude Code gives them (typed into the shell as it is).
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return false;
        }
        let Some(root) = &self.root else { return false };
        let file = format!("{id}.jsonl");
        std::fs::read_dir(root.join("projects")).into_iter().flatten().flatten().any(|project| project.path().join(&file).is_file())
    }
}

/// What Claude is told as it starts (`brief`): its part, and for a game
/// inside the engine's source the settings that keep the engine's own
/// notes and skills from it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Briefing {
    /// `--append-system-prompt-file`.
    pub prompt: Option<PathBuf>,
    /// `--settings`.
    pub settings: Option<PathBuf>,
}

/// The skills of the engine's source that are for developing the engine
/// (its API, checking its changes), not for making a game.
const ENGINE_SKILLS: [&str; 2] = ["voxcade-api", "voxcade-verify"];

/// Writes into `files` what Claude is told for the game in `dir` called
/// `title`: its part; where the game lies inside the engine's own source,
/// also the settings that leave out the CLAUDE.md files above the game
/// folder up to the source's top (the engine developers' notes), hide its
/// engine skills and give the game a memory of its own (Claude Code's
/// would be the engine's, kept for the repository).
pub fn brief(files: &Path, dir: &Path, title: &str) -> Result<Briefing> {
    brief_with(files, dir, title, claude_dir().as_deref())
}

/// `brief`, with Claude Code's own folder `claude` (a test's).
fn brief_with(files: &Path, dir: &Path, title: &str, claude: Option<&Path>) -> Result<Briefing> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let name: String = dir.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    std::fs::create_dir_all(files)?;
    let exe = crate::cli::program().map_or_else(|_| "voxcade".to_string(), |p| p.display().to_string());
    let source = engine_source(&dir);
    let mut prompt = format!(
        "# The game in this folder

You help the person make one game with Voxcade: {title}, the game in {dir}. This folder is the game's project, and its files are yours to change when the person asks: conf.lua (its settings), main.lua (its rules), its levels, models, sounds, music and replays.

Voxcade is the engine: the program {exe}, which the game uses and does not change. It plays a game the same way every time (replays rely on that), so whatever changes while playing lives in the game's states.
- Its Lua API: `voxcade api` lists every name with what it does (`voxcade api NAME` one of them); the voxcade MCP server's api tool gives the same.
- To see the game: the MCP tools shot (a picture after some ticks of input), run and things (how it stands, as data); check, or `voxcade check .`, plays the game's replays and says whether each still runs as recorded. A change meant to alter the game records them anew with `voxcade check . --update`: ask the person first, their own recorded runs then play differently.
- To try what happens late in the game without playing there: scenarios (scenarios/NAME.lua: a table with about, setup = function() ... end, and where the run begins, level and place) set the game up in code; the scenarios tool lists them, shot, run and things begin from one (scenario), and run lines of Lua first (do, as the console does). Every game keeps scenarios for its late situations: when you build something that only shows late (an event, a level far in, a merchant, a full inventory), write a scenario for it, and a test replay from it (`voxcade shot . --scenario NAME --input ... --record replays/tests/NAME.toml`).
- The IDE the person works in: the MCP tools view_state, open, game_frame, errors, play and stop show and steer it; eval runs a line of Lua in the game being played there (the console), and the person sees it in OUTPUT.

When the game needs something the engine does not have, say so plainly, and how the game could do without it for now; do not reach around the engine.
",
        dir = dir.display()
    );
    let mut settings = None;
    if let Some(root) = &source {
        prompt += &format!(
            "
This game folder lies inside the engine's own source code ({}), where the engine is developed in a session of its own. From here that source is not yours: change only files in this game folder, never the engine's (its Rust source, its ready-made rules, other games, templates, tools, docs), do not build or test the engine, and commit nothing outside this folder. If the engine should change, say what and why: the person takes it to the engine's session.
",
            root.display()
        );
        let mut excludes = Vec::new();
        for above in dir.ancestors().skip(1).take_while(|a| a.starts_with(root)) {
            for file in ["CLAUDE.md", "CLAUDE.local.md", ".claude/CLAUDE.md"] {
                excludes.push(above.join(file).to_string_lossy().into_owned());
            }
            excludes.push(above.join(".claude/rules/**").to_string_lossy().into_owned());
        }
        let deny: Vec<String> = ENGINE_SKILLS.iter().map(|s| format!("Skill({s})")).collect();
        let hidden: serde_json::Map<String, serde_json::Value> = ENGINE_SKILLS.iter().map(|s| (s.to_string(), "off".into())).collect();
        let mut json = serde_json::json!({ "claudeMdExcludes": excludes, "skillOverrides": hidden, "permissions": { "deny": deny } });
        if let Some(claude) = claude {
            json["autoMemoryDirectory"] = claude.join("projects").join(&name).join("memory").to_string_lossy().into_owned().into();
        }
        let file = files.join(format!("{name}.json"));
        crate::blocks::save_at_once(&file, (serde_json::to_string_pretty(&json)? + "\n").as_bytes())?;
        settings = Some(file);
    }
    let file = files.join(format!("{name}.md"));
    crate::blocks::save_at_once(&file, prompt.as_bytes())?;
    Ok(Briefing { prompt: Some(file), settings })
}

/// The top of the engine's own source when the game folder `dir` lies
/// inside it (a folder above it whose Cargo.toml is Voxcade's).
fn engine_source(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().skip(1).find(|a| {
        let Ok(text) = std::fs::read_to_string(a.join("Cargo.toml")) else { return false };
        toml::from_str::<toml::Table>(&text).ok().and_then(|t| t.get("package")?.get("name")?.as_str().map(|n| n == "voxcade")).unwrap_or(false)
    }).map(Path::to_path_buf)
}

/// A game's place among the conversations: its folder's whole path.
fn key(dir: &Path) -> String {
    dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()).to_string_lossy().into_owned()
}

/// Writes the IDE's MCP server (this program's `voxcade mcp`) for Claude's
/// `--mcp-config` to `file`.
pub fn write_mcp(file: &Path) -> Result<()> {
    let exe = crate::cli::program()?;
    let config = serde_json::json!({ "mcpServers": { "voxcade": { "type": "stdio", "command": exe, "args": ["mcp"] } } });
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::blocks::save_at_once(file, (serde_json::to_string_pretty(&config)? + "\n").as_bytes())
}

/// A new session id: a random UUID (version 4), as Claude Code takes it.
fn session_id() -> String {
    let mut b = super::bridge::random();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// Claude Code's own folder: `CLAUDE_CONFIG_DIR`, else `.claude` at home.
fn claude_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| PathBuf::from(h).join(".claude"))
}

/// `word` as one word to the console's shell.
fn quote(word: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", word.replace('"', ""))
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game's first CLAUDE begins a conversation with an id the IDE
    /// keeps; not yet kept by Claude Code, the same id begins it again; a
    /// fresh one has a new id; the IDE's tools come from the file named.
    #[test]
    fn claude_starts_the_games_conversation() {
        let mut claude = Claude { root: None, ..Claude::default() };
        let dir = Path::new("games/voxkeep");
        let first = claude.command(dir, "Voxkeep", false, Some(Path::new("/tmp/mcp.json")), &Briefing::default());
        let id = claude.sessions[&key(dir)].clone();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4", "a version 4 UUID");
        assert!(first.starts_with(&format!("claude --session-id {id} --name 'Voxkeep'")), "{first}");
        assert!(first.ends_with(" --mcp-config '/tmp/mcp.json'"), "{first}");
        assert!(claude.command(dir, "Voxkeep", false, None, &Briefing::default()).contains(&id), "never begun: the same id");
        let fresh = claude.command(dir, "Voxkeep", true, None, &Briefing::default());
        assert!(!fresh.contains(&id) && fresh.contains(&claude.sessions[&key(dir)]), "a new one: {fresh}");
        assert!(claude.command(Path::new("games/pulsar"), "Pulsar's \\ $HOME", true, None, &Briefing::default()).contains("--name 'Pulsars  HOME'"), "plain words");
        claude.connect = false;
        assert!(!claude.command(dir, "Voxkeep", false, Some(Path::new("/tmp/mcp.json")), &Briefing::default()).contains("--mcp-config"));
    }

    /// Claude is told its part: the game in its folder. Inside the
    /// engine's own source it also leaves out the engine developers'
    /// CLAUDE.md files above the game and their engine skills; elsewhere
    /// nothing is left out.
    #[test]
    fn claude_is_told_its_part() {
        let tmp = std::env::temp_dir().join(format!("voxcade-brief-{}", std::process::id()));
        let engine = tmp.join("engine");
        let game = engine.join("games/keep");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(engine.join("Cargo.toml"), "[package]\nname = \"voxcade\"\n").unwrap();
        let alone = tmp.join("mine/keep");
        std::fs::create_dir_all(&alone).unwrap();
        let files = tmp.join("agents");

        let inside = brief_with(&files, &game, "Keep", Some(Path::new("/home/x/.claude"))).unwrap();
        let prompt = std::fs::read_to_string(inside.prompt.as_ref().unwrap()).unwrap();
        assert!(prompt.contains("Keep, the game in") && prompt.contains("inside the engine's own source"), "{prompt}");
        let settings: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(inside.settings.as_ref().unwrap()).unwrap()).unwrap();
        let excludes = settings["claudeMdExcludes"].as_array().unwrap();
        let root = engine.canonicalize().unwrap();
        for file in [root.join("CLAUDE.md"), root.join("games/CLAUDE.md")] {
            assert!(excludes.iter().any(|e| e == file.to_str().unwrap()), "{file:?} left out");
        }
        assert!(!excludes.iter().any(|e| e.as_str().unwrap().starts_with(game.canonicalize().unwrap().to_str().unwrap())), "the game's own CLAUDE.md stays");
        assert_eq!(settings["permissions"]["deny"], serde_json::json!(["Skill(voxcade-api)", "Skill(voxcade-verify)"]));
        assert_eq!(settings["skillOverrides"], serde_json::json!({ "voxcade-api": "off", "voxcade-verify": "off" }));
        let memory = settings["autoMemoryDirectory"].as_str().unwrap();
        assert!(memory.starts_with("/home/x/.claude/projects/") && memory.ends_with("-engine-games-keep/memory"), "the game's own memory: {memory}");
        let mut claude = Claude { root: None, ..Claude::default() };
        let command = claude.command(&game, "Keep", true, None, &inside);
        assert!(command.contains(" --append-system-prompt-file '") && command.contains(" --settings '"), "{command}");

        let outside = brief_with(&files, &alone, "Keep", Some(Path::new("/home/x/.claude"))).unwrap();
        assert_eq!(outside.settings, None, "nothing left out");
        assert!(!std::fs::read_to_string(outside.prompt.unwrap()).unwrap().contains("engine's own source"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A conversation Claude Code kept is gone on with; without one
    /// remembered, the newest in the game folder (begun by hand).
    #[test]
    fn a_kept_conversation_is_gone_on_with() {
        let root = std::env::temp_dir().join(format!("voxcade-claude-{}", std::process::id()));
        let mut claude = Claude { root: Some(root.clone()), ..Claude::default() };
        let dir = Path::new("games/voxkeep");
        let _ = claude.command(dir, "Voxkeep", false, None, &Briefing::default());
        let id = claude.sessions[&key(dir)].clone();
        let project = root.join("projects").join("-somewhere-voxkeep");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(format!("{id}.jsonl")), "{}\n").unwrap();
        assert_eq!(claude.command(dir, "Voxkeep", false, None, &Briefing::default()), format!("claude --resume {id}"));
        claude.resume = false;
        assert!(claude.command(dir, "Voxkeep", false, None, &Briefing::default()).contains("--session-id"), "a new one each time");
        // Begun by hand in the game folder: found by its path.
        let mut fresh = Claude { root: Some(root.clone()), ..Claude::default() };
        let path = dir.canonicalize().unwrap().to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>();
        std::fs::create_dir_all(root.join("projects").join(&path)).unwrap();
        std::fs::write(root.join("projects").join(&path).join("0be1-a2.jsonl"), "{}\n").unwrap();
        assert_eq!(fresh.command(dir, "Voxkeep", false, None, &Briefing::default()), "claude --resume 0be1-a2");
        assert_eq!(fresh.sessions[&key(dir)], "0be1-a2", "remembered from now on");
        // Not an id as Claude Code gives them: not typed into the shell.
        fresh.sessions.clear();
        std::fs::write(root.join("projects").join(&path).join("x; rm.jsonl"), "{}\n").unwrap();
        assert!(fresh.command(dir, "Voxkeep", false, None, &Briefing::default()).contains("--session-id"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

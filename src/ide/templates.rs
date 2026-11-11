//! NEW: a game begun from a template, in the projects folder (the
//! settings'). The templates lie in the repository's `templates/` and are
//! built into the program, so a build makes new games anywhere: an empty
//! one (no kit, the script makes the rules), a jump & run (the platformer kit, Voxkeep's room) and a
//! shooter (the shooter kit, a yard built in code). Their title is "My
//! Game", which becomes the new game's name.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::prefs::Agent;

/// The game's own notes for an agent, `{title}` and `{template}` to fill.
const NOTES: &str = include_str!("game_claude.md");

/// A game to begin from.
pub struct Template {
    /// Its name in the IDE, and a line of what it is.
    pub name: &'static str,
    pub about: &'static str,
    files: &'static [(&'static str, &'static [u8])],
}

macro_rules! file {
    ($template:literal, $file:literal) => {
        ($file, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/", $template, "/", $file)) as &[u8])
    };
}

pub const TEMPLATES: [Template; 3] = [
    Template {
        name: "EMPTY",
        about: "A floor and a figure the arrows walk; every rule is yours to write.",
        files: &[
            file!("empty", "conf.lua"),
            file!("empty", "hero.vox"),
            file!("empty", "hero2.vox"),
            file!("empty", "main.lua"),
            file!("empty", "scenarios/far_corner.lua"),
        ],
    },
    Template {
        name: "JUMP & RUN",
        about: "A castle room of voxel blocks, a hero who runs, jumps and climbs, gems and a door.",
        files: &[
            file!("platformer", "bat.vox"),
            file!("platformer", "blocks/ladder.vox"),
            file!("platformer", "blocks/plank.vox"),
            file!("platformer", "blocks/spikes.vox"),
            file!("platformer", "blocks/stone.vox"),
            file!("platformer", "blocks/wall.vox"),
            file!("platformer", "conf.lua"),
            file!("platformer", "door.vox"),
            file!("platformer", "gem.vox"),
            file!("platformer", "hero.vox"),
            file!("platformer", "hero2.vox"),
            file!("platformer", "level.json"),
            file!("platformer", "main.lua"),
            file!("platformer", "music/under_the_dunes.mml"),
            file!("platformer", "scenarios/door_open.lua"),
            file!("platformer", "slime.vox"),
            file!("platformer", "torch.vox"),
        ],
    },
    Template {
        name: "SHOOTER",
        about: "A yard to run and fire through, walls to hide behind, riflemen up ahead.",
        files: &[
            file!("shooter", "conf.lua"),
            file!("shooter", "enemy.vox"),
            file!("shooter", "main.lua"),
            file!("shooter", "music/hot_pursuit.mml"),
            file!("shooter", "player.vox"),
            file!("shooter", "player2.vox"),
            file!("shooter", "scenarios/last_life.lua"),
        ],
    },
];

/// The folder a game called `name` gets in `projects`: its name in small
/// letters, every sign but letters and digits a dash.
pub fn folder_for(projects: &Path, name: &str) -> PathBuf {
    let mut folder: String = name.trim().chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    while folder.contains("--") {
        folder = folder.replace("--", "-");
    }
    projects.join(folder.trim_matches('-'))
}

/// What came of making the game a git repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Git {
    /// `git` is not there, or would not start a repository.
    Without,
    /// A repository, but the first commit did not happen (no identity set).
    Started,
    /// A repository with the first commit.
    Committed,
}

impl Git {
    /// One plain sentence of it, for where the IDE says such things (NEW
    /// has no place for a remark that is no error yet).
    #[allow(dead_code)]
    pub fn said(self) -> &'static str {
        match self {
            Git::Without => "git is not installed, so the game is no git repository.",
            Git::Started => "The game is a git repository, but its first commit did not happen (git needs your name and email: git config --global user.name, user.email).",
            Git::Committed => "The game is a git repository, with its first commit made.",
        }
    }
}

/// A game begun: its folder and what became of its repository.
#[derive(Debug)]
pub struct Begun {
    pub dir: PathBuf,
    #[allow(dead_code)]
    pub git: Git,
}

/// Makes the folder `dir` a git repository: `.gitignore` for what BUILD and
/// the pictures write, `git init`, everything added, a first commit saying
/// `message` under the user's own identity (`identity`: environment
/// variables to give the git programs besides, for a test). Never fails:
/// what did not work is said in what it returns.
pub fn begin_repository(dir: &Path, message: &str, identity: &[(&str, &str)]) -> Git {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .envs(identity.iter().copied())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if !git(&["init", "-q"]) {
        return Git::Without;
    }
    // Written even if the rest fails: the folder is the user's to commit.
    let _ = std::fs::write(dir.join(".gitignore"), "builds/\nshots/\n");
    if git(&["add", "-A"]) && git(&["commit", "-q", "-m", message]) { Git::Committed } else { Git::Started }
}

/// Begins the game called `name` from `template` in a new folder in
/// `projects`, with the agent's notes for the game (`notes`: its file's
/// name comes from the agent) and as a git repository; the folder and what
/// git did.
pub fn create(template: &Template, projects: &Path, name: &str, notes: Option<Agent>) -> Result<Begun> {
    let name = name.trim();
    if name.is_empty() {
        bail!("the game needs a name");
    }
    let dir = folder_for(projects, name);
    if dir == projects {
        bail!("a name with letters or digits, please");
    }
    if dir.exists() {
        bail!("{} is there already", dir.display());
    }
    for (file, bytes) in template.files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().expect("in the folder")).with_context(|| format!("making {}", dir.display()))?;
        // The title the template has, the game's name in its place.
        let bytes = if *file == "conf.lua" {
            let text = String::from_utf8_lossy(bytes).replace("\"My Game\"", &format!("{name:?}")).replace("\"MY GAME\"", &format!("{:?}", name.to_uppercase()));
            text.into_bytes()
        } else {
            bytes.to_vec()
        };
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    }
    if let Some(agent) = notes {
        let text = NOTES.replace("{title}", name).replace("{template}", &template.name.to_lowercase());
        let path = dir.join(agent.notes_file());
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    let git = begin_repository(&dir, &format!("A new game from the {} template", template.name.to_lowercase()), &[]);
    Ok(Begun { dir, git })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every template begins a game that loads under its name and plays
    /// without trouble; a name taken, or none, is refused.
    #[test]
    fn every_template_begins_a_game_that_plays() {
        let projects = std::env::temp_dir().join(format!("voxcade-templates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&projects);
        for (i, template) in TEMPLATES.iter().enumerate() {
            let name = format!("Test Game {i}");
            let dir = create(template, &projects, &name, Some(Agent::Claude)).unwrap().dir;
            assert_eq!(dir, projects.join(format!("test-game-{i}")));
            let mut game = crate::session::Session::<crate::Voxcade>::load(&dir).unwrap();
            assert_eq!(game.cfg.title, name, "{}", template.name);
            for _ in 0..120 {
                game.step(&crate::input::Input { right: true, ..Default::default() });
            }
            assert!(game.trouble().is_none(), "{}: {:?}", template.name, game.trouble());
            assert!(create(template, &projects, &name, None).is_err(), "taken");
        }
        assert!(create(&TEMPLATES[0], &projects, "  ", None).is_err(), "no name");
        let _ = std::fs::remove_dir_all(&projects);
    }

    /// The tick in NEW puts the game's notes for the agent in, named for the
    /// agent, with the game's name and the template's in them; without it
    /// there is no such file. The notes are no part of the game: the
    /// fingerprint stays and a change to them reloads nothing.
    #[test]
    fn the_notes_come_with_the_tick_and_are_no_part_of_the_game() {
        let projects = std::env::temp_dir().join(format!("voxcade-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&projects);
        let with = create(&TEMPLATES[1], &projects, "Lava Run", Some(Agent::Claude)).unwrap().dir;
        let without = create(&TEMPLATES[1], &projects, "Dry Run", None).unwrap().dir;
        let notes = std::fs::read_to_string(with.join("CLAUDE.md")).unwrap();
        assert!(notes.starts_with("# Lava Run\n") && notes.contains("the jump & run template"), "{notes}");
        assert!(!notes.contains("{title}") && !notes.contains("{template}"));
        assert!(!without.join("CLAUDE.md").exists());
        // Both are the same game but for the title: the notes change nothing.
        let mut a = crate::session::Session::<crate::Voxcade>::load(&with).unwrap();
        let mut b = crate::session::Session::<crate::Voxcade>::load(&without).unwrap();
        assert_eq!(a.fingerprint().unwrap(), b.fingerprint().unwrap());
        let before = a.fingerprint().unwrap();
        std::fs::write(with.join("CLAUDE.md"), "# changed\n").unwrap();
        assert!(matches!(a.reload(&with.join("CLAUDE.md")).unwrap(), crate::session::Reloaded::Nothing));
        assert_eq!(crate::session::Session::<crate::Voxcade>::load(&with).unwrap().fingerprint().unwrap(), before);
        for _ in 0..30 {
            b.step(&crate::input::Input::default());
        }
        assert!(b.trouble().is_none());
        let _ = std::fs::remove_dir_all(&projects);
    }

    /// A new game is a git repository when git is there: one commit with
    /// the notes in it (given an identity), `.gitignore` written; and
    /// whatever git does, making the game does not fail.
    #[test]
    fn a_new_game_is_a_git_repository() {
        let git = |dir: &Path, args: &[&str]| std::process::Command::new("git").arg("-C").arg(dir).args(args).output();
        let projects = std::env::temp_dir().join(format!("voxcade-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&projects);
        let begun = create(&TEMPLATES[0], &projects, "Git Game", Some(Agent::Claude)).unwrap();
        if git(&begun.dir, &["--version"]).is_err() {
            assert_eq!(begun.git, Git::Without);
            return;
        }
        assert_ne!(begun.git, Git::Without);
        assert_eq!(std::fs::read_to_string(begun.dir.join(".gitignore")).unwrap(), "builds/\nshots/\n");
        // With an identity of the test's own the commit is sure to be made.
        let who = [("GIT_AUTHOR_NAME", "T"), ("GIT_AUTHOR_EMAIL", "t@example.com"), ("GIT_COMMITTER_NAME", "T"), ("GIT_COMMITTER_EMAIL", "t@example.com")];
        let dir = projects.join("again");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.lua"), "").unwrap();
        assert_eq!(begin_repository(&dir, "A new game from the test template", &who), Git::Committed);
        let log = String::from_utf8(git(&dir, &["log", "--format=%s"]).unwrap().stdout).unwrap();
        assert_eq!(log.trim(), "A new game from the test template");
        let files = String::from_utf8(git(&dir, &["ls-files"]).unwrap().stdout).unwrap();
        assert_eq!(files.lines().collect::<Vec<_>>(), [".gitignore", "main.lua"]);
        let _ = std::fs::remove_dir_all(&projects);
    }

    /// The templates built in are the ones in the repository, every file.
    #[test]
    fn the_templates_built_in_are_the_repositorys() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        for (folder, template) in ["empty", "platformer", "shooter"].iter().zip(&TEMPLATES) {
            let mut on_disk = Vec::new();
            fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
                for entry in std::fs::read_dir(dir).unwrap().flatten() {
                    let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
                    if entry.path().is_dir() {
                        walk(&entry.path(), &format!("{name}/"), out);
                    } else {
                        out.push(name);
                    }
                }
            }
            walk(&root.join(folder), "", &mut on_disk);
            on_disk.sort();
            let built: Vec<String> = template.files.iter().map(|(f, _)| f.to_string()).collect();
            assert_eq!(built, on_disk, "templates/{folder}");
        }
    }
}

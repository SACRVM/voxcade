//! NEW: a game begun from a template, in the projects folder (the
//! settings'). The templates lie in the repository's `templates/` and are
//! built into the program, so a build makes new games anywhere: an empty
//! one (no kit, the script makes the rules), a jump & run (the platformer kit, Voxkeep's room) and a
//! shooter (the shooter kit, a yard built in code). Their title is "My
//! Game", which becomes the new game's name.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

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

/// Begins the game called `name` from `template` in a new folder in
/// `projects`; the folder.
pub fn create(template: &Template, projects: &Path, name: &str) -> Result<PathBuf> {
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
    Ok(dir)
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
            let dir = create(template, &projects, &name).unwrap();
            assert_eq!(dir, projects.join(format!("test-game-{i}")));
            let mut game = crate::session::Session::<crate::Voxcade>::load(&dir).unwrap();
            assert_eq!(game.cfg.title, name, "{}", template.name);
            for _ in 0..120 {
                game.step(&crate::input::Input { right: true, ..Default::default() });
            }
            assert!(game.trouble().is_none(), "{}: {:?}", template.name, game.trouble());
            assert!(create(template, &projects, &name).is_err(), "taken");
        }
        assert!(create(&TEMPLATES[0], &projects, "  ").is_err(), "no name");
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

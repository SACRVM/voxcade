use super::*;

#[test]
fn versions_compare_by_their_numbers() {
    assert!(older("0.5.0", "0.6.0") && older("0.9.0", "0.10.0") && !older("0.6.0", "0.6.0") && !older("1.0.0", "0.9.9"));
    assert_eq!(version_in("  c.voxcade = \"0.6.0\"   -- for"), Some("0.6.0".into()));
    assert_eq!(version_in("c.voxcade='0.5.1'"), Some("0.5.1".into()));
    assert_eq!(version_in("-- c.voxcade = \"0.6.0\""), None);
}

/// The line goes first into conf(c) when there is none, and changes
/// only its version when there is.
#[test]
fn the_version_line_is_written_into_conf() {
    let text = "-- a game\nfunction conf(c)\n    c.title = \"X\"\nend\n";
    let new = with_version(text, "0.6.0").unwrap();
    assert_eq!(new.lines().nth(2).unwrap().split("--").next().unwrap().trim_end(), "    c.voxcade = \"0.6.0\"");
    assert!(new.ends_with("end\n"));
    let again = with_version(&new, "0.7.1").unwrap();
    assert_eq!(again.replace("0.7.1", "0.6.0"), new);
    assert_eq!(with_version("x = 1\n", "0.6.0"), None);
}

/// A game without the line was made before; one stating a newer
/// version does not open either; the same version opens.
#[test]
fn a_game_opens_only_for_its_version() {
    let dir = crate::scratch::Scratch::new("upgrade-open");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.title = \"X\"\nend\n").unwrap();
    // Without the line and nothing any step would change: it opens,
    // the line missing said plainly; `upgrade` writes only the line.
    opens(&dir).unwrap();
    assert!(matches!(standing(&dir).unwrap(), Standing::Current));
    assert!(unstated(&dir).is_some_and(|l| l.contains("c.voxcade = \"") && l.contains("voxcade upgrade")), "{:?}", unstated(&dir));
    let said = upgrade(&dir, true).unwrap();
    assert!(said.iter().any(|l| l.contains("would say c.voxcade")), "{said:?}");
    // Without the line and something to change: made for an older one,
    // the first thing to change named.
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/boom.wav"), b"RIFF").unwrap();
    let e = opens(&dir).unwrap_err().to_string();
    assert!(e.contains("made for an older Voxcade (conf.lua states none") && e.contains("sounds/boom.wav") && e.contains("voxcade upgrade"), "{e}");
    std::fs::remove_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.5.0\"\nend\n").unwrap();
    assert!(opens(&dir).unwrap_err().to_string().contains("made for Voxcade 0.5.0; this is"));
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.voxcade = \"99.0.0\"\nend\n").unwrap();
    assert!(opens(&dir).unwrap_err().to_string().contains("made with Voxcade 99.0.0"));
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.voxcade = \"{}\"\nend\n", engine())).unwrap();
    opens(&dir).unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

/// Every game and template here is for this Voxcade.
#[test]
fn the_games_here_open() {
    for folder in ["games", "templates", "examples"] {
        for game in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(folder)).unwrap().flatten() {
            opens(&game.path()).unwrap();
        }
    }
}

/// An older game's models become text, its Lua names them, the line
/// is written; a dry run changes nothing; its world becomes text too.
#[test]
fn an_older_game_is_brought_up_to_date() {
    let dir = crate::scratch::Scratch::new("upgrade-run");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("blocks")).unwrap();
    crate::vox::write_figure(&dir.join("robot.vox"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO), ("arm", glam::IVec3::new(1, 2, 5), glam::IVec3::new(6, 1, 3))]);
    crate::vox::write_figure(&dir.join("world.vox"), &[("ground", glam::IVec3::new(8, 8, 2), glam::IVec3::ZERO), ("rock", glam::IVec3::new(2, 2, 2), glam::IVec3::new(0, 0, 2))]);
    std::fs::copy(dir.join("robot.vox"), dir.join("blocks/stone.vox")).unwrap();
    let conf = "function conf(c)\n  c.things.robot = { model = \"robot.vox\" }\n  c.world.file = \"world.vox\"\nend\n";
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    std::fs::write(dir.join("main.lua"), "function on_draw(frame)\n  frame:model('robot' .. \".vox\", player)\nend\n").unwrap();
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/scream.wav"), b"RIFF").unwrap();
    let Standing::Older { made: None, steps } = standing(&dir).unwrap() else { panic!("not older") };
    assert_eq!(steps.len(), STEPS.len());
    let sounds = steps.iter().find(|s| s.name == "no recorded sounds").unwrap();
    assert!(sounds.would.iter().any(|l| l.starts_with("sounds/scream.wav is a recording") && l.contains("sounds/scream.toml")), "{:?}", sounds.would);
    let dry = upgrade(&dir, true).unwrap().join("\n");
    assert!(dry.contains("robot.vox becomes robot.vvx (a figure of 2 parts)") && dry.contains("world.vox is a world or a map's prefab") && dry.contains("world.vox becomes world.vvx (a world, 8 by 8 by 4 voxels") && dry.contains("would say c.voxcade"), "{dry}");
    assert!(dir.join("robot.vox").is_file() && std::fs::read_to_string(dir.join("conf.lua")).unwrap() == conf, "a dry run changes nothing");
    let done = upgrade(&dir, false).unwrap().join("\n");
    assert!(done.contains("robot.vvx") && done.contains("conf.lua:2 now"), "{done}");
    assert!(!dir.join("robot.vox").exists() && dir.join("robot.vvx").is_file() && dir.join("blocks/stone.vvx").is_file() && !dir.join("world.vox").exists() && dir.join("world.vvx").is_file());
    assert!(done.contains("sounds/scream.wav is a recording") && dir.join("sounds/scream.wav").is_file(), "the recording is named, not deleted");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    assert!(conf.contains("model = \"robot.vvx\"") && conf.contains("c.world.file = \"world.vvx\"") && conf.contains(&format!("c.voxcade = \"{}\"", engine())), "{conf}");
    assert!(std::fs::read_to_string(dir.join("main.lua")).unwrap().contains("'robot' .. \".vvx\""), "a model's name made in code");
    opens(&dir).unwrap();
    assert!(upgrade(&dir, false).unwrap().join("\n").contains("nothing to do"));
    std::fs::remove_dir_all(&dir).ok();
}

/// A game with every word Voxcade said before it took other tools'
/// (`turn`, `turns`, `behaviour`) in its levels, its map, `conf.lua` and
/// its Lua.
fn game_of_old_words(dir: &Path, version_line: &str) {
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    std::fs::create_dir_all(dir.join("blocks")).unwrap();
    std::fs::create_dir_all(dir.join("ground")).unwrap();
    let conf = format!(
        "-- A kind's behaviour moves it.\nfunction conf(c)\n{version_line}  c.kit = \"platformer\"\n  c.camera.turns = \"free\"\n  c.things.gem = {{ behaviour = \"pickup\", model = \"gem.vvx\", turns = \"free\" }}\n  c.things.bat = {{ model = \"gem.vvx\" }}\n  c.things.bat.behaviour = \"flyer\"\n  c.things.bat.frames = {{ turn = {{ 0, 0 }} }}\n  c.title_screen.props = {{\n    {{ model = \"gem.vvx\", at = {{ 10, 10 }}, turn = 1 }},\n    {{ model = \"gem.vvx\", at = {{ 20, 10 }} }},\n  }}\n  c.world.blocks = {{ file = \"levels/01.toml\" }}\nend\n"
    );
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let main = "local SPIN = { turn = 3 }   -- the game's own\n\nfunction on_draw(frame)\n  local a, turn = 1, 2\n  frame:model(\"gem.vvx\", player, 0, { turn = 45, pitch = 10 }, { head = { turn = 30 } })\nend\n";
    std::fs::write(dir.join("main.lua"), main).unwrap();
    std::fs::write(dir.join("gem.vvx"), "format = \"voxcade voxel 1\"\nsize = [1, 1, 1]\n\n[palette]\na = \"#ff0000\"\n\n[[frame]]\nlayers = \"\"\"\na\n\"\"\"\n").unwrap();
    let mut lv = crate::grid::GridLevel::new(2, glam::IVec3::new(3, 1, 2));
    lv.blocks.push(crate::grid::BlockEntry { name: "rock".into(), kind: crate::grid::Kind::Solid });
    lv.cells[1] = crate::grid::Cell { block: 1, turn: crate::blocks::Turn::MIRROR };
    let mut chair = crate::grid::Thing::new("gem", [1, 1, 2]);
    chair.facing = Some(crate::grid::Facing::Turn(30.0));
    lv.things.push(chair);
    // Written as the engine wrote it before.
    let text = crate::grid::text::write(&lv, &crate::grid::Legend::default())
        .replace(crate::grid::LEGEND_HEAD, words::OLD_LEGEND_HEAD)
        .replace("orientation = ", "turn = ")
        .replace("rotation = 30", "turn = 30    # askew");
    assert!(text.contains("turn = 30    # askew") && text.contains(", turn = "), "{text}");
    std::fs::write(dir.join("levels/01.toml"), text).unwrap();
    crate::headless::write_png(&dir.join("ground/height.png"), 16, 16, &[3, 3, 3, 255].repeat(256)).unwrap();
    let mut post = crate::scene::Scene::new(glam::IVec3::new(3, 2, 1));
    post.colours = vec![[200, 10, 10], [10, 200, 10]];
    post.cells = vec![1, 1, 2, 0, 0, 1];
    std::fs::write(dir.join("post.vvx"), post.write()).unwrap();
    let mut map = format!("format = \"{}\"\nsize = [16, 16, 8]\n\n[ground]\nheight = \"ground/height.png\"\n\n[[ground.materials]]\nsurface = [\"#00ff00\"]\n\n[prefabs]\npost = \"post.vvx\"\n\n{}\n", crate::map::FORMAT, words::OLD_PUT_HEAD);
    for (q, at) in [(0, [3, 3]), (1, [8, 3]), (2, [3, 8]), (3, [8, 8]), (5, [12, 12])] {
        map += &format!("[[put]]\nprefab = \"post\"\nat = [{}, {}]\n{}\n", at[0], at[1], if q == 0 { String::new() } else { format!("turn = {q}\n") });
    }
    std::fs::write(dir.join("map.toml"), map).unwrap();
}

/// What an old map's prefab put on the ground: each voxel turned by
/// `turn` quarter turns anticlockwise about its pivot, as the engine did
/// before (`map::turn`, written out again here).
fn old_post_cells(at: [i32; 2], quarters: i32) -> Vec<glam::IVec3> {
    let pivot = glam::IVec3::new(1, 1, 0);
    let cells = [(0, 0), (1, 0), (2, 0), (2, 1)].map(|(x, y)| glam::IVec3::new(x, y, 0) - pivot);
    cells
        .iter()
        .map(|&l| {
            let t = match quarters.rem_euclid(4) {
                0 => l,
                1 => glam::IVec3::new(-l.y, l.x, l.z),
                2 => glam::IVec3::new(-l.x, -l.y, l.z),
                _ => glam::IVec3::new(l.y, -l.x, l.z),
            };
            glam::IVec3::new(at[0], at[1], 3) + t
        })
        .collect()
}

/// An older game's words are Voxcade's of now: a level's thing turn its
/// rotation, the legend's turn an orientation, a map's quarter turns
/// degrees clockwise building the same world, conf.lua's behaviour,
/// turns and a title prop's turn, frame:model's turn in the call;
/// comments kept; a dry run changes nothing; a second run nothing more.
/// A game made before 0.6.0 and one stating 0.6.0 alike.
#[test]
fn the_old_words_become_other_tools_words() {
    for (name, line) in [("words-0.5", "  -- made before 0.6.0\n"), ("words-0.6", "  c.voxcade = \"0.6.0\"\n")] {
        let dir = crate::scratch::Scratch::new(name);
        game_of_old_words(&dir, line);
        let Standing::Older { steps, .. } = standing(&dir).unwrap() else { panic!("{name}: not older") };
        let words = steps.iter().find(|s| s.name == "rotation and behavior").expect("the step");
        let would = words.would.join("\n");
        for says in [
            "levels/01.toml: the legend's turn becomes orientation (1 sign)",
            "levels/01.toml: a thing's turn becomes rotation, the same degrees (1 thing)",
            "map.toml: a [[put]]'s turn (quarter turns anticlockwise) becomes rotation in degrees clockwise: turn = 1 is rotation = 270, 2 is 180, 3 is 90 (4 puts)",
            "conf.lua:1 becomes: -- A kind's behavior moves it.",
            "conf.lua:5 becomes: c.camera.rotates = \"free\"",
            "conf.lua:6 becomes: c.things.gem = { behavior = \"pickup\", model = \"gem.vvx\", rotates = \"free\" }",
            "conf.lua:8 becomes: c.things.bat.behavior = \"flyer\"",
            "conf.lua:11 becomes: { model = \"gem.vvx\", at = { 10, 10 }, rotation = 270 },",
            "main.lua:5 becomes: frame:model(\"gem.vvx\", player, 0, { rotation = 45, pitch = 10 }, { head = { rotation = 30 } })",
            "main.lua:1 is left as it is: if this table goes to frame:model",
        ] {
            assert!(would.contains(says), "{name}: {says}\n{would}");
        }
        assert!(!would.contains("conf.lua:9") && !would.contains("main.lua:4"), "a frame set called turn and a local are the game's: {would}");
        let before: Vec<_> = ["conf.lua", "main.lua", "map.toml", "levels/01.toml"].iter().map(|f| std::fs::read_to_string(dir.join(f)).unwrap()).collect();
        let dry = upgrade(&dir, true).unwrap().join("\n");
        assert!(dry.contains("rotation and behavior (Voxcade 0.6.0):") && dry.contains("conf.lua:5 becomes"), "{dry}");
        let after: Vec<_> = ["conf.lua", "main.lua", "map.toml", "levels/01.toml"].iter().map(|f| std::fs::read_to_string(dir.join(f)).unwrap()).collect();
        assert_eq!(before, after, "a dry run changes nothing");

        let done = upgrade(&dir, false).unwrap().join("\n");
        assert!(done.contains("conf.lua:5 now: c.camera.rotates = \"free\"") && done.contains("levels/01.toml: the legend's turn became orientation"), "{done}");
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        assert!(conf.contains("-- A kind's behavior moves it.") && conf.contains("c.things.bat.frames = { turn = { 0, 0 } }"), "{conf}");
        let level = std::fs::read_to_string(dir.join("levels/01.toml")).unwrap();
        assert!(level.contains(crate::grid::LEGEND_HEAD) && level.contains("rotation = 30    # askew") && level.contains(", orientation = ") && !level.contains("turn"), "{level}");
        let map = std::fs::read_to_string(dir.join("map.toml")).unwrap();
        assert!(map.contains(crate::map::PUT_HEAD) && map.contains("rotation = 270\n") && map.contains("rotation = 180\n") && map.contains("rotation = 90\n") && !map.contains("turn ="), "{map}");
        // The map builds the world the old one did, voxel for voxel.
        let (world, _, _) = crate::map::load_map(&dir.join("map.toml"), &[]).unwrap();
        for (q, at) in [(0, [3, 3]), (1, [8, 3]), (2, [3, 8]), (3, [8, 8]), (5, [12, 12])] {
            let cells = old_post_cells(at, q);
            assert!(cells.iter().all(|&c| world.get(c) != 0), "turn = {q}: the post where it stood");
            assert_eq!(world.palette[world.get(cells[2]) as usize], [10, 200, 10, 255], "turn = {q}: its green end where it was");
        }
        let mut cfg = crate::config::GameConfig::default();
        crate::lua::conf::run(&crate::files::Files::folder(&dir), &mut cfg).unwrap();
        assert_eq!((cfg.camera.rotates, cfg.title_screen.props[0].rotation), (crate::config::Rotates::Free, 270));
        assert_eq!(cfg.things["gem"]["behavior"].as_str(), Some("pickup"));
        assert!(crate::grid::GridLevel::read(&dir.join("levels/01.toml")).unwrap().things[0].facing == Some(crate::grid::Facing::Turn(30.0)));
        let again = upgrade(&dir, false).unwrap().join("\n");
        assert!(again.contains("nothing to do"), "{name}: {again}");
        opens(&dir).unwrap();
    }
    // What the step cannot place is said with its line, and left.
    let (edits, left) = words::conf_edits("function conf(c)\n  c.title_screen.props[2].turn = 2\n  local kind = { turns = \"free\" }\nend\n");
    assert!(edits.is_empty() && left.len() == 2, "{left:?}");
}

/// Every file under `dir` (but hidden ones) with its bytes, by its path
/// from `dir`.
fn contents(dir: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, from: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.file_name().unwrap().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                walk(&p, from, out);
            } else {
                out.insert(p.strip_prefix(from).unwrap().to_path_buf(), std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// A game outside git has its files kept, as they were, before the
/// first step writes (the steps change files in place: without git a
/// step failing halfway left no way back); a dry run and a game with
/// nothing to do keep nothing; a step that fails says where they are.
#[test]
fn an_upgrade_outside_git_keeps_the_files_first() {
    let dir = crate::scratch::Scratch::new("upgrade-keep");
    let keep = crate::scratch::Scratch::new("upgrade-keep-to");
    game_of_old_words(&dir, "");
    std::fs::create_dir_all(dir.join("shots")).unwrap();
    std::fs::write(dir.join("shots/a.png"), b"not the game's").unwrap();
    let before = contents(&dir);
    upgrade_keeping(&dir, true, &keep).unwrap();
    assert_eq!(std::fs::read_dir(&keep).unwrap().count(), 0, "a dry run keeps nothing");
    let done = upgrade_keeping(&dir, false, &keep).unwrap().join("\n");
    let kept = keep.join(format!("{}-for-an-older-voxcade", crate::settings::game_name(&dir)));
    assert!(done.contains(&format!("the game's files as they were are kept in {}", kept.display())), "{done}");
    let mut game = before.clone();
    game.retain(|p, _| !p.starts_with("shots"));
    assert_eq!(contents(&kept), game, "every file of the game as it was, not shots/");
    assert_ne!(contents(&dir), before, "the game itself was upgraded");
    assert!(upgrade_keeping(&dir, false, &keep).unwrap().join("\n").contains("nothing to do"));
    assert_eq!(std::fs::read_dir(&keep).unwrap().count(), 1, "nothing to do, nothing kept");

    // A step that cannot write says it stopped and where the files are;
    // a second keeping takes a name of its own.
    let stuck = crate::scratch::Scratch::new("upgrade-stuck");
    game_of_old_words(&stuck, "");
    // (levels/ read-only: the step writes a level beside it and renames.)
    let levels = stuck.join("levels");
    let was = std::fs::metadata(&levels).unwrap().permissions();
    let mut ro = was.clone();
    ro.set_readonly(true);
    std::fs::set_permissions(&levels, ro).unwrap();
    if std::fs::write(levels.join("probe"), "x").is_ok() {
        std::fs::set_permissions(&levels, was).unwrap();
        eprintln!("SKIPPED the failing half: a read-only folder is written here (root?)");
        return;
    }
    let failed = upgrade_keeping(&stuck, false, &keep);
    std::fs::set_permissions(&levels, was).unwrap();
    let e = format!("{:#}", failed.unwrap_err());
    let kept = keep.join(format!("{}-for-an-older-voxcade", crate::settings::game_name(&stuck)));
    assert!(e.contains("stopped halfway") && e.contains(&format!("the game's files as they were are in {}", kept.display())), "{e}");
    assert_eq!(std::fs::read(kept.join("conf.lua")).unwrap(), std::fs::read(stuck.join("conf.lua")).unwrap());
}

/// A game in a git repository with everything committed keeps nothing:
/// git has it all.
#[test]
fn an_upgrade_in_git_keeps_nothing() {
    let dir = crate::scratch::Scratch::new("upgrade-git");
    let keep = crate::scratch::Scratch::new("upgrade-git-to");
    game_of_old_words(&dir, "");
    let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(dir.path()).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output();
    if git(&["init", "-q"]).is_err() {
        eprintln!("SKIPPED an_upgrade_in_git_keeps_nothing: no git here");
        return;
    }
    git(&["add", "-A"]).unwrap();
    assert!(git(&["commit", "-q", "-m", "the game"]).unwrap().status.success());
    assert!(committed(&dir));
    let done = upgrade_keeping(&dir, false, &keep).unwrap().join("\n");
    assert!(!done.contains("kept in") && std::fs::read_dir(&keep).unwrap().count() == 0, "{done}");
    assert!(!committed(&dir), "the upgrade changed what git holds");
}

/// A game in a folder the repository ignores (a copy under `target/`)
/// is not held by git: git's status says nothing of it, yet the upgrade
/// keeps its files first.
#[test]
fn a_game_git_ignores_is_kept_before_the_upgrade() {
    let repo = crate::scratch::Scratch::new("upgrade-ignored");
    let keep = crate::scratch::Scratch::new("upgrade-ignored-to");
    let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(repo.path()).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output();
    if git(&["init", "-q"]).is_err() {
        eprintln!("SKIPPED a_game_git_ignores_is_kept_before_the_upgrade: no git here");
        return;
    }
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    git(&["add", "-A"]).unwrap();
    assert!(git(&["commit", "-q", "-m", "ignore"]).unwrap().status.success());
    let dir = repo.join("target/game");
    std::fs::create_dir_all(&dir).unwrap();
    game_of_old_words(&dir, "");
    assert!(!committed(&dir), "git holds nothing of an ignored folder");
    let done = upgrade_keeping(&dir, false, &keep).unwrap().join("\n");
    assert!(done.contains("kept in") && std::fs::read_dir(&keep).unwrap().count() == 1, "{done}");
}

/// A one-model `.vox`: voxels as x, y, z, colour index; colours for
/// indices 1, 2, ...
fn vox_bytes(size: [u32; 3], voxels: &[[u8; 4]], colours: &[[u8; 3]]) -> Vec<u8> {
    fn chunk(tag: &[u8], content: &[u8], children: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend((content.len() as u32).to_le_bytes());
        out.extend((children.len() as u32).to_le_bytes());
        out.extend(content);
        out.extend(children);
        out
    }
    let size: Vec<u8> = size.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut xyzi = (voxels.len() as u32).to_le_bytes().to_vec();
    xyzi.extend(voxels.iter().flatten());
    let mut rgba = vec![0u8; 1024];
    for (i, c) in colours.iter().enumerate() {
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
    }
    let children = [chunk(b"SIZE", &size, &[]), chunk(b"XYZI", &xyzi, &[]), chunk(b"RGBA", &rgba, &[])].concat();
    [b"VOX ".to_vec(), 150u32.to_le_bytes().to_vec(), chunk(b"MAIN", &[], &children)].concat()
}

/// An older game's map and its prefabs become text: the prefab's
/// colours in the order the map took them (a later colour first used
/// before an earlier one), its markers named in it, `c.markers` out of
/// `conf.lua`; the world built from the text is the one the old files
/// made. A level of blocks as JSON becomes text and reads the same.
#[test]
fn levels_maps_and_worlds_become_text() {
    use glam::IVec3;
    let dir = crate::scratch::Scratch::new("upgrade-maps");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ground")).unwrap();
    crate::headless::write_png(&dir.join("ground/height.png"), 8, 8, &[3, 3, 3, 255].repeat(64)).unwrap();
    // A post along x of colours 5 then 2 (5 first used), two markers of
    // one kind the reading order would swap, one of another.
    let post = vox_bytes([3, 1, 3], &[[0, 0, 0, 5], [1, 0, 0, 2], [2, 0, 0, 2], [2, 0, 2, 240], [0, 0, 1, 240], [1, 0, 1, 241]], &[[1, 1, 1], [9, 9, 9], [0, 0, 0], [0, 0, 0], [200, 10, 10]]);
    std::fs::write(dir.join("post.vox"), post).unwrap();
    std::fs::write(dir.join("map.json"), r##"{ "size": [8, 8, 16], "ground": { "height": "ground/height.png", "materials": [{ "surface": ["#00ff00"] }] }, "prefabs": { "post": "post.vox" }, "place": [{ "prefab": "post", "at": [2, 1], "turn": 1 }, { "marker": "goal", "at": [6, 6] }] }"##).unwrap();
    let conf = "function conf(c)\n  c.voxcade = \"0.6.0\"\n  c.world.file = \"map.json\"\n  c.things.goal = {}\n  c.things.rifleman = {}\n  c.things.guard = {}\n\n  -- palette slots that place things\n  c.markers.rifleman = 240\n  c.markers.guard = 241\n\n  c.player.spawn = { 1, 1 }\nend\n";
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    std::fs::write(dir.join("main.lua"), "function on_world_start() end\n").unwrap();
    let Standing::Older { made: Some(_), steps } = standing(&dir).unwrap() else { panic!("not older") };
    assert_eq!(steps.iter().map(|s| s.name).collect::<Vec<_>>(), ["maps as text", "worlds and prefabs as text"]);
    let e = opens(&dir).unwrap_err().to_string();
    assert!(e.contains("Voxcade 0.6.0 reads this no more (maps as text, worlds and prefabs as text): ") && !e.contains("earlier"), "{e}");
    let done = upgrade(&dir, false).unwrap().join("\n");
    assert!(done.contains("map.json became map.toml") && done.contains("post.vox became post.vvx") && done.contains("conf.lua:9 taken out: c.markers.rifleman = 240"), "{done}");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    assert!(conf.contains("c.world.file = \"map.toml\"") && !conf.contains("markers") && !conf.contains("\n\n\n"), "{conf}");
    let text = std::fs::read_to_string(dir.join("post.vvx")).unwrap();
    assert!(text.contains("a = \"#c80a0a\"\nb = \"#090909\"\nc = { marker = \"rifleman\" }\nd = { marker = \"rifleman\" }\ne = { marker = \"guard\" }"), "{text}");
    assert!(std::fs::read_to_string(dir.join("map.toml")).unwrap().contains("post = \"post.vvx\""));
    opens(&dir).unwrap();
    let mut cfg = crate::config::GameConfig::default();
    crate::lua::conf::run(&crate::files::Files::folder(&dir), &mut cfg).unwrap();
    let levels = crate::levels::load::<crate::Voxcade>(&crate::files::Files::folder(&dir), &cfg).unwrap();
    let level = &levels.list[0];
    let names = crate::sim::things::names(&cfg);
    let kinds: Vec<(&str, IVec3)> = level.markers.iter().map(|m| (names[m.kind], m.cell)).collect();
    // Turned a quarter about its pivot (1, 0, 0) standing on (2, 1, 3):
    // x runs along y.
    assert_eq!(kinds, [("rifleman", IVec3::new(2, 2, 5)), ("rifleman", IVec3::new(2, 0, 4)), ("guard", IVec3::new(2, 1, 4)), ("goal", IVec3::new(6, 6, 3))]);
    let w = &level.world;
    assert_eq!((w.palette[w.get(IVec3::new(2, 0, 3)) as usize], w.get(IVec3::new(2, 0, 3))), ([200, 10, 10, 255], 2), "the map's colors: the ground's first, then the post's in the order of first use");
    assert_eq!(w.palette[w.get(IVec3::new(2, 2, 3)) as usize], [9, 9, 9, 255]);

    // A level of blocks.
    let blocks = dir.join("blocks_game");
    std::fs::create_dir_all(blocks.join("levels")).unwrap();
    let mut lv = crate::grid::GridLevel::new(2, IVec3::new(3, 1, 2));
    lv.blocks.push(crate::grid::BlockEntry { name: "rock".into(), kind: crate::grid::Kind::Solid });
    lv.cells[1] = crate::grid::Cell { block: 1, turn: crate::blocks::Turn::MIRROR };
    lv.things.push(crate::grid::Thing::new("gem", [1, 1, 2]));
    let json = format!("{{ \"block\": 2, \"size\": [3, 1, 2], \"blocks\": [{{ \"name\": \"rock\", \"kind\": \"solid\" }}], \"cells\": [[1, 0, 0], [1, 1, {}], [4, 0, 0]], \"things\": [{{ \"kind\": \"gem\", \"at\": [1, 1, 2] }}] }}", crate::blocks::Turn::MIRROR.number());
    std::fs::write(blocks.join("levels/01_cave.json"), json).unwrap();
    std::fs::write(blocks.join("conf.lua"), "function conf(c)\n  c.voxcade = \"0.6.0\"\n  c.world.blocks = { file = \"levels/01_cave.json\" }\nend\n").unwrap();
    let done = upgrade(&blocks, false).unwrap().join("\n");
    assert!(done.contains("levels/01_cave.json became levels/01_cave.toml") && done.contains("conf.lua:3 now"), "{done}");
    assert_eq!(crate::grid::GridLevel::read(&blocks.join("levels/01_cave.toml")).unwrap(), lv);
    assert!(!blocks.join("levels/01_cave.json").exists());
    opens(&blocks).unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

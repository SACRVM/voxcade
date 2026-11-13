//! The upgrade's steps, each on a small game of the old formats in a
//! scratch folder: a dry run changes no byte and names the files the run
//! then changes, the run names the same files, every file it changed is
//! named, and afterwards the game opens and a second run has nothing to
//! do. (The steps' own tests in `upgrade.rs` check what each one writes.)

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::*;

/// Every file under `dir` with its bytes, by its path from `dir`.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, from: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, from, out);
            } else {
                out.insert(p.strip_prefix(from).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// The files of `among` a text names (by their path from the game).
fn named(text: &str, among: &BTreeSet<String>) -> BTreeSet<String> {
    among.iter().filter(|f| text.contains(f.as_str())).cloned().collect()
}

/// `game` (files by path) written into a fresh scratch folder.
fn game(name: &str, made: &dyn Fn(&Path)) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(name);
    made(&dir);
    dir
}

fn put(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The small old games, each for one step (`c.voxcade` older than every
/// step: all are due, the others find nothing).
fn old_games() -> Vec<(&'static str, crate::scratch::Scratch)> {
    let conf = |rest: &str| format!("function conf(c)\n  c.voxcade = \"0.5.0\"\n{rest}end\n");
    vec![
        (
            "rotation and behavior",
            game("step-words", &|d| {
                put(d, "conf.lua", &conf("  -- a kind's behaviour moves it\n  c.camera.turns = \"free\"\n  c.things.bat = { behaviour = \"flyer\", model = \"bat.vvx\" }\n"));
                put(d, "main.lua", "function on_draw(frame)\n  frame:model(\"bat.vvx\", player, 0, { turn = 45 })\nend\n");
            }),
        ),
        (
            "models as text",
            game("step-models", &|d| {
                put(d, "conf.lua", &conf("  c.things.robot = { model = \"robot.vox\" }\n"));
                put(d, "main.lua", "function on_draw(frame)\n  frame:model(\"robot.vox\", player)\nend\n");
                crate::vox::write_figure(&d.join("robot.vox"), &[("body", glam::IVec3::new(4, 4, 8), glam::IVec3::ZERO), ("arm", glam::IVec3::new(1, 2, 5), glam::IVec3::new(6, 1, 3))]);
            }),
        ),
        (
            "no recorded sounds",
            game("step-sounds", &|d| {
                put(d, "conf.lua", &conf(""));
                put(d, "sounds/boom.wav", "RIFF");
                put(d, "sounds/zap.toml", "# zap\nfile = \"zap.ogg\"\n");
            }),
        ),
        (
            "levels as text",
            game("step-levels", &|d| {
                put(d, "conf.lua", &conf("  c.world.blocks = { file = \"levels/01_cave.json\" }\n"));
                let json = format!("{{ \"block\": 2, \"size\": [3, 1, 2], \"blocks\": [{{ \"name\": \"rock\", \"kind\": \"solid\" }}], \"cells\": [[1, 0, 0], [1, 1, {}], [4, 0, 0]], \"things\": [{{ \"kind\": \"gem\", \"at\": [1, 1, 2] }}] }}", crate::blocks::Turn::MIRROR.number());
                put(d, "levels/01_cave.json", &json);
            }),
        ),
        (
            "maps as text",
            game("step-maps", &|d| {
                put(d, "conf.lua", &conf("  c.world.file = \"map.json\"\n  c.things.goal = {}\n"));
                std::fs::create_dir_all(d.join("ground")).unwrap();
                crate::headless::write_png(&d.join("ground/height.png"), 8, 8, &[3, 3, 3, 255].repeat(64)).unwrap();
                crate::vox::write_figure(&d.join("post.vox"), &[("post", glam::IVec3::new(3, 1, 3), glam::IVec3::ZERO)]);
                put(d, "map.json", r##"{ "size": [8, 8, 16], "ground": { "height": "ground/height.png", "materials": [{ "surface": ["#00ff00"] }] }, "prefabs": { "post": "post.vox" }, "place": [{ "prefab": "post", "at": [2, 1], "turn": 1 }, { "marker": "goal", "at": [6, 6] }] }"##);
            }),
        ),
        (
            "worlds and prefabs as text",
            game("step-worlds", &|d| {
                put(d, "conf.lua", &conf("  c.world.file = \"world.vox\"\n"));
                crate::vox::write_figure(&d.join("world.vox"), &[("ground", glam::IVec3::new(8, 8, 2), glam::IVec3::ZERO), ("rock", glam::IVec3::new(2, 2, 2), glam::IVec3::new(0, 0, 2))]);
            }),
        ),
    ]
}

/// Every step has a game here, and each game has its step due with
/// something to do.
#[test]
fn every_step_has_an_old_game_here() {
    let games = old_games();
    for step in STEPS {
        assert!(games.iter().any(|(name, _)| *name == step.name), "the step '{}' has no game in upgrade_tests.rs", step.name);
    }
    for (name, dir) in &games {
        let Standing::Older { steps, .. } = standing(dir).unwrap() else { panic!("{name}: not older") };
        let plan = steps.iter().find(|s| s.name == *name).unwrap_or_else(|| panic!("{name}: not due"));
        assert!(!plan.would.is_empty(), "{name}: the step finds nothing to do in its game");
    }
}

/// For each step's game: the dry run changes nothing and names every file
/// the run then changes; the run names the same files; the game opens
/// after it, and a second run has nothing to do.
#[test]
fn each_step_does_what_its_dry_run_says() {
    for (name, dir) in old_games() {
        let before = files(&dir);
        let dry = upgrade(&dir, true).unwrap().join("\n");
        assert_eq!(files(&dir), before, "{name}: a dry run changes nothing");
        assert!(dry.contains(&format!("{name} (Voxcade ")), "{name}: the dry run names the step:\n{dry}");
        let done = upgrade(&dir, false).unwrap().join("\n");
        let after = files(&dir);
        let all: BTreeSet<String> = before.keys().chain(after.keys()).cloned().collect();
        let changed: BTreeSet<String> = all.iter().filter(|f| before.get(*f) != after.get(*f)).cloned().collect();
        assert!(!changed.is_empty(), "{name}: the run changed nothing:\n{done}");
        // A file made anew (robot.vox becomes robot.vvx) is named as the
        // one it was made from with its new ending.
        let said = |text: &str| -> BTreeSet<String> {
            let mut out = named(text, &all);
            for f in &all {
                let stem = Path::new(f).with_extension("").to_string_lossy().into_owned();
                if !out.contains(f) && ["vvx", "toml"].iter().any(|e| text.contains(&format!("{stem}.{e}"))) && after.contains_key(f) && !before.contains_key(f) {
                    out.insert(f.clone());
                }
            }
            out
        };
        let (would, did) = (said(&dry), said(&done));
        let unsaid: Vec<&String> = changed.iter().filter(|f| !would.contains(*f)).collect();
        assert!(unsaid.is_empty(), "{name}: the dry run did not name {unsaid:?}:\n{dry}");
        let unsaid: Vec<&String> = changed.iter().filter(|f| !did.contains(*f)).collect();
        assert!(unsaid.is_empty(), "{name}: the run did not name {unsaid:?}:\n{done}");
        assert_eq!(would, did, "{name}: the dry run and the run name the same files\n-- dry:\n{dry}\n-- run:\n{done}");
        opens(&dir).unwrap_or_else(|e| panic!("{name}: {e:#}\n{done}"));
        let again = upgrade(&dir, false).unwrap().join("\n");
        assert!(again.contains("nothing to do"), "{name}: {again}");
        assert_eq!(files(&dir), after, "{name}: a second run changes nothing");
    }
}

/// The step that only names recordings deletes and changes none of them:
/// its dry run and its run say the same lines.
#[test]
fn recordings_are_named_never_touched() {
    let games = old_games();
    let (_, dir) = games.iter().find(|(n, _)| *n == "no recorded sounds").unwrap();
    let step = STEPS.iter().find(|s| s.name == "no recorded sounds").unwrap();
    let before = files(dir);
    let planned = (step.plan)(dir).unwrap();
    let applied = (step.apply)(dir).unwrap();
    assert_eq!(planned, applied);
    assert_eq!(files(dir), before);
    assert!(planned.iter().any(|l| l.starts_with("sounds/boom.wav is a recording")) && planned.iter().any(|l| l.starts_with("sounds/zap.toml:2 names a recording")), "{planned:?}");
}

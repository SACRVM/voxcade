//! The upgrade's steps, each on a small game of the old formats in a
//! scratch folder: a dry run changes no byte and names the files the run
//! then changes, the run names the same files, every file it changed is
//! named, and afterwards the game opens and a second run has nothing to
//! do. (The steps' own tests in `upgrade/tests.rs` check what each one writes.)

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
            "no list of models",
            game("step-listed", &|d| {
                put(d, "conf.lua", &conf("  -- drawn by main.lua\n  c.models = { \"ship.vvx\",\n    \"ship2.vvx\" }\n\n  c.things.ship = {}\n"));
                put(d, "main.lua", "function on_draw(frame)\n  frame:model(\"ship.vvx\", player)\nend\n");
            }),
        ),
        (
            "lamps stopped by walls",
            game("step-lamps", &|d| {
                put(d, "conf.lua", &conf(""));
                put(d, "main.lua", "function on_draw(frame)\n  frame:light(player, 40, { 255, 200, 120 })\nend\n");
            }),
        ),
        (
            "colors from 0 to 255",
            game("step-colors", &|d| {
                put(d, "conf.lua", &conf("  c.light.sky = { 0.06, 0.07, 0.1 }\n  c.effects = { haze_color = { 0.86, 0.78, 0.62 } }\n  c.transition.color = \"000000\"\n"));
                put(d, "levels/hall.toml", "format = \"voxcade level 1\"\nsize = [32, 32, 16]\n\n[light]\nsky = [0.125, 0.1875, 0.29]\nsun_color = \"#ffd0a0\"\n");
            }),
        ),
        (
            "names of 0.7",
            game("step-names", &|d| {
                put(d, "conf.lua", &conf("  c.things.hut = { behavior = \"spawner\", spawns = \"rifleman\", range = 240 }\n  c.title_screen.drip = 1\n"));
                put(d, "main.lua", "function on_level_start(how)\n  level.state.left = level.away\nend\n");
            }),
        ),
        (
            "music from the code",
            game("step-music", &|d| {
                put(d, "conf.lua", &conf("  c.audio.music = { \"music/kit.mml\", \"music/storm.mml\", \"music/calm.mml\" }\n  c.title_screen.level = \"title\"\n  c.title_screen.music = \"music/calm.mml\"\n  c.title_screen.music_at = 3\n"));
                put(d, "music/kit.mml", "#tempo 120\n@k sine\n");
                put(d, "music/storm.mml", "#tempo 120\n@a triangle\nA: c1 c1 c1 c1\n");
                put(d, "music/calm.mml", "#tempo 90\n@a triangle\nA: c1 c1 c1 c1\n");
                put(d, "main.lua", "function on_tick()\n  player:walk(input.dir, 40)\nend\n");
            }),
        ),
        (
            "kinds facing their way",
            game("step-facing", &|d| {
                put(d, "conf.lua", &conf("  c.things.door = { model = \"door.vvx\",\n    facing = { 1, 0 } }\n  c.things.gate = { like = \"door\" }\n  c.things.post = {}\n"));
                put(d, "main.lua", "function on_level_start(how)\n  level:spawn(\"door\", 4, 4)\nend\n");
            }),
        ),
        (
            "the title's song named",
            game("step-title-song", &|d| {
                put(d, "conf.lua", &conf("  c.audio.music = { \"music/kit.mml\",\n    \"music/storm.mml\" }\n  c.audio.music_volume = 0.4\n"));
                put(d, "music/kit.mml", "#tempo 120\n@k sine\n");
                put(d, "music/storm.mml", "#tempo 120\n@a triangle\nA: c1 c1 c1 c1\n");
                put(d, "main.lua", "function on_level_start(how)\n  music:play(\"storm\")\nend\n");
            }),
        ),
        (
            "see-through colors on the HUD",
            game("step-see-through", &|d| {
                put(d, "conf.lua", &conf(""));
                put(d, "main.lua", SEE_THROUGH);
            }),
        ),
        (
            "key and button names checked",
            game("step-bindings", &|d| {
                put(d, "conf.lua", &conf(BINDINGS));
                put(d, "main.lua", "function on_world_start() end\n");
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
        assert!(games.iter().any(|(name, _)| *name == step.name), "the step '{}' has no game in upgrade/step_tests.rs", step.name);
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

/// A game already stating 0.6.0 whose `conf.lua` still lists models (the
/// step came while 0.6.0 was being made): it does not open, says why, and
/// the upgrade takes the lines out, the rest of the file as it was (the
/// version line saying this Voxcade's; a later step with nothing to do).
#[test]
fn a_game_of_this_version_listing_models_is_brought_over() {
    let dir = game("listed-now", &|d| {
        put(d, "conf.lua", "function conf(c)\n  c.voxcade = \"0.6.0\"\n  c.things.ship = {}\n  c.models = { \"ship.vvx\" }\nend\n");
        put(d, "main.lua", "function on_world_start() end\n");
    });
    let Standing::Older { made: Some(_), steps } = standing(&dir).unwrap() else { panic!("not older") };
    assert_eq!(steps.iter().map(|s| s.name).collect::<Vec<_>>(), ["no list of models", "lamps stopped by walls", "colors from 0 to 255", "names of 0.7", "music from the code", "kinds facing their way", "the title's song named", "see-through colors on the HUD", "key and button names checked"]);
    assert!(steps[1].would.is_empty(), "no lamps to speak of");
    let e = opens(&dir).unwrap_err().to_string();
    assert!(e.contains("made for Voxcade 0.6.0"), "{e}");
    let dry = upgrade(&dir, true).unwrap().join("\n");
    assert!(dry.contains("conf.lua:4 taken out: c.models = { \"ship.vvx\" }"), "{dry}");
    upgrade(&dir, false).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("conf.lua")).unwrap(), format!("function conf(c)\n  c.voxcade = \"{}\"\n  c.things.ship = {{}}\nend\n", engine()));
    opens(&dir).unwrap();
    assert!(upgrade(&dir, false).unwrap().join("\n").contains("nothing to do"));
}

/// "music from the code" on games of each shape: without an
/// `on_level_start` and asking no song, the call is added (on a title level
/// of the game's own its song, `c.title_screen.music`'s lines taken out),
/// the game played as before; with an `on_level_start`, a `music:play()`
/// without a song, or a song asked already beside `c.title_screen.music`
/// on its own title, it says the line to add and changes nothing.
#[test]
fn music_from_the_code_adds_the_call_where_it_safely_can() {
    let songs = |d: &Path, rest: &str| {
        put(d, "conf.lua", &format!("function conf(c)\n  c.voxcade = \"0.6.0\"\n  c.audio.music = {{ \"music/storm.mml\", \"music/calm.mml\" }}\n{rest}end\n"));
        put(d, "music/storm.mml", "#tempo 120\n@a triangle\nA: c1 c1 c1 c1\n");
        put(d, "music/calm.mml", "#tempo 90\n@a triangle\nA: c1 c1 c1 c1\n");
    };
    // Added: the first song, and the call says so.
    let plain = game("step-music-plain", &|d| {
        songs(d, "");
        put(d, "main.lua", "function on_tick()\nend\n");
    });
    upgrade(&plain, false).unwrap();
    let main = std::fs::read_to_string(plain.join("main.lua")).unwrap();
    assert!(main.ends_with("function on_tick()\nend\n\n-- Voxcade 0.7: music comes from the game's code alone (voxcade upgrade added this).\nfunction on_level_start(how)\n  music:play(\"storm\")\nend\n"), "{main}");
    // A title of its own: its song on the title level, the first elsewhere,
    // the setting gone.
    let titled = game("step-music-title", &|d| {
        songs(d, "  c.title_screen.level = \"title\"\n  c.title_screen.music = \"calm\"\n");
        put(d, "main.lua", "");
    });
    upgrade(&titled, false).unwrap();
    let main = std::fs::read_to_string(titled.join("main.lua")).unwrap();
    assert!(main.contains("  if level.name == \"title\" then music:play(\"calm\") else music:play(\"storm\") end\n"), "{main}");
    assert!(!std::fs::read_to_string(titled.join("conf.lua")).unwrap().contains("title_screen.music"));
    // Its own handler: words only.
    let own = game("step-music-own", &|d| {
        songs(d, "");
        put(d, "main.lua", "-- the level\nfunction on_level_start(how)\n  level.state.n = 0\nend\n");
    });
    let before = files(&own);
    let said = upgrade(&own, false).unwrap().join("\n");
    assert!(said.contains("main.lua:2: music comes from the game's code alone now: add music:play(\"storm\") to on_level_start"), "{said}");
    assert_eq!(files(&own).get("main.lua"), before.get("main.lua"), "nothing changed in it");
    // music:play() without a song, and a title setting beside a song asked.
    let bare = game("step-music-bare", &|d| {
        songs(d, "  c.title_screen.level = \"title\"\n  c.title_screen.music = \"calm\"\n");
        put(d, "main.lua", "function on_level_start(how)\n  music:play(\"calm\")\nend\nfunction on_tick()\n  if world.tick == 9 then music:play() end\nend\n");
    });
    let before = files(&bare);
    let said = upgrade(&bare, false).unwrap().join("\n");
    assert!(said.contains("main.lua:5: music:play() without a song gave the music back to the player's song, which is no more: name the song, music:play(\"storm\")"), "{said}");
    assert!(said.contains("conf.lua:5: c.title_screen.music is the engine's title's: play calm from the title level title's code"), "{said}");
    assert_eq!((files(&bare).get("main.lua"), files(&bare).get("conf.lua").is_some_and(|c| String::from_utf8_lossy(c).contains("title_screen.music"))), (before.get("main.lua"), true));
    // Songs asked only later (a boss's): silent until then, said.
    let later = game("step-music-later", &|d| {
        songs(d, "");
        put(d, "main.lua", "function on_tick()\n  if world.tick == 600 then music:play(\"calm\") end\nend\n");
    });
    let said = upgrade(&later, false).unwrap().join("\n");
    assert!(said.contains("main.lua: on_level_start: music comes from the game's code alone now, and the game asks its songs only later"), "{said}");
    opens(&later).unwrap();
}

/// "the title's song named" on games of each shape: on the engine's title
/// with songs and no `c.title_screen.music`, the first song a player heard
/// (not a file of instruments) is written after `c.audio.music`, and the
/// title then has that song; a title song named, a title level of the
/// game's own or no song at all: nothing; `c.audio` set whole, the songs
/// set inside a block or `c.title_screen` set whole after them: the line
/// said, nothing changed.
#[test]
fn the_titles_song_is_named_where_the_title_played_the_first() {
    let conf = |rest: &str| format!("function conf(c)\n  c.voxcade = \"0.6.0\"\n{rest}end\n");
    let songs = |d: &Path, conf: &str| {
        put(d, "conf.lua", conf);
        put(d, "main.lua", "function on_level_start(how)\n  music:play(\"calm\")\nend\n");
        put(d, "music/kit.mml", "#tempo 120\n@k sine\n");
        put(d, "music/storm.mml", "#tempo 120\n@a triangle\nA: c1 c1 c1 c1\n");
        put(d, "music/calm.mml", "#tempo 90\n@a triangle\nA: c1 c1 c1 c1\n");
    };
    let step = STEPS.iter().find(|s| s.name == "the title's song named").unwrap();
    let plain = game("step-title-plain", &|d| songs(d, &conf("  c.audio.music = { \"music/kit.mml\", \"music/storm.mml\", \"music/calm.mml\" }\n  c.audio.music_volume = 0.4\n")));
    let dry = (step.plan)(&plain).unwrap();
    assert_eq!(dry, ["conf.lua:4: the engine's title is silent now unless c.title_screen.music names its song; adds c.title_screen.music = \"music/storm.mml\" after c.audio.music, so the title plays the song it played before"]);
    upgrade(&plain, false).unwrap();
    let text = std::fs::read_to_string(plain.join("conf.lua")).unwrap();
    assert!(text.contains("  c.audio.music = { \"music/kit.mml\", \"music/storm.mml\", \"music/calm.mml\" }\n  c.title_screen.music = \"music/storm.mml\"   -- the engine's title's song (unset, the title is silent)\n  c.audio.music_volume = 0.4\n"), "{text}");
    let mut cfg = crate::config::GameConfig::default();
    crate::lua::conf::run(&crate::files::Files::folder(&plain), &mut cfg).unwrap();
    assert_eq!(cfg.title_screen.music.as_deref(), Some("music/storm.mml"), "the title plays storm as before");
    assert!((step.plan)(&plain).unwrap().is_empty(), "nothing more to do");
    // Untouched: a title song named, a title of the game's own, no songs.
    for (name, rest) in [
        ("step-title-named", "  c.audio.music = { \"music/storm.mml\", \"music/calm.mml\" }\n  c.title_screen.music = \"calm\"\n"),
        ("step-title-own", "  c.audio.music = { \"music/storm.mml\" }\n  c.title_screen.level = \"title\"\n"),
        ("step-title-none", "  c.audio.music = { \"music/kit.mml\" }\n"),
    ] {
        let dir = game(name, &|d| songs(d, &conf(rest)));
        assert!((step.plan)(&dir).unwrap().is_empty(), "{name}");
    }
    // Where the line cannot safely go: said, nothing changed.
    for (name, rest) in [
        ("step-title-whole", "  c.audio = { music = { \"music/storm.mml\" }, music_volume = 0.4 }\n"),
        ("step-title-block", "  if true then\n    c.audio.music = { \"music/storm.mml\" }\n  end\n"),
        ("step-title-after", "  c.audio.music = { \"music/storm.mml\" }\n  c.title_screen = { logo = { \"GAME\" } }\n"),
    ] {
        let dir = game(name, &|d| songs(d, &conf(rest)));
        let before = std::fs::read(dir.join("conf.lua")).unwrap();
        let said = (step.apply)(&dir).unwrap();
        assert_eq!(said, ["conf.lua: the engine's title is silent now unless c.title_screen.music names its song: add c.title_screen.music = \"music/storm.mml\" to conf(c), after c.audio.music, for the song it played before (nothing is changed here)"], "{name}");
        assert_eq!(std::fs::read(dir.join("conf.lua")).unwrap(), before, "{name}");
    }
}

/// A script drawing the HUD with colors of every kind: the first four
/// literals are see-through now, the last two solid.
const SEE_THROUGH: &str = "function on_draw(frame)
  frame:box(1, 1, 2, 2, { 10, 20, 30, 40 })
  frame:text(\"HI\", 1, 1, \"#10203040\")
  frame:bar(1, 1, 8, 2, 0.5, { 200, 0, 0 }, { 0, 0, 0, 90, })
  frame:drop_in(\"GO\", 4, 4, 10, { 255, 255, 255, 128 }, 6)
  frame:box(1, 1, 2, 2, { 10, 20, 30 })
  frame:box(1, 1, 2, 2, { 10, 20, 30, 255 })
  frame:text(\"#\", 1, 1, \"#102030ff\")
  -- frame:box(1, 1, 2, 2, { 1, 2, 3, 4 })
  local s = \"frame:box(1, 1, 2, 2, { 1, 2, 3, 4 })\"
  frame:box(2, 2, 2, 2, shade)
end
";

/// The step names the literal colors with a fourth value below 255 by file
/// and line (a table, a "#rrggbbaa", a bar's back, an icon's letter), not
/// solid ones, comments or strings, says once that colors in variables are
/// not looked at, and changes nothing; a game without such a color has
/// nothing to change.
#[test]
fn see_through_colors_are_named_never_changed() {
    let games = old_games();
    let (_, dir) = games.iter().find(|(n, _)| *n == "see-through colors on the HUD").unwrap();
    put(dir, "conf.lua", "function conf(c)\n  c.voxcade = \"0.5.0\"\n  c.icons.heart = { rows = { \"a.\" }, colors = { a = { 200, 0, 0, 100 }, [\"b\"] = { 1, 2, 3 } } }\nend\n");
    let step = STEPS.iter().find(|s| s.name == "see-through colors on the HUD").unwrap();
    let before = files(dir);
    let planned = (step.plan)(dir).unwrap();
    assert_eq!(planned, (step.apply)(dir).unwrap());
    assert_eq!(files(dir), before);
    let lines: Vec<&str> = planned.iter().map(String::as_str).collect();
    let at = |prefix: &str| lines.iter().any(|l| l.starts_with(prefix) && l.contains("see-through now, was solid") && l.contains("drop its fourth value"));
    assert!(at("main.lua:2: frame:box: { 10, 20, 30, 40 }") && at("main.lua:3: frame:text: \"#10203040\"") && at("main.lua:4: frame:bar: { 0, 0, 0, 90, }") && at("main.lua:5: frame:drop_in: { 255, 255, 255, 128 }"), "{planned:#?}");
    assert!(at("conf.lua:3: c.icons.heart.colors a = { 200, 0, 0, 100 }"), "{planned:#?}");
    assert_eq!(lines.iter().filter(|l| l.contains("see-through now, was solid")).count(), 5, "no solid color, comment or string named: {planned:#?}");
    assert_eq!(lines.iter().filter(|l| l.contains("not looked at")).count(), 1, "{planned:#?}");
    // Nothing to say without a literal one.
    put(dir, "main.lua", "function on_draw(frame)\n  frame:box(1, 1, 2, 2, { 10, 20, 30 })\n  frame:box(1, 1, 2, 2, shade)\nend\n");
    put(dir, "conf.lua", "function conf(c)\n  c.voxcade = \"0.5.0\"\nend\n");
    assert!((step.plan)(dir).unwrap().is_empty());
}

/// Bindings of every kind that did nothing in 0.6: a pad's letters, a
/// typo of a button and of a key, an action the game lacks, a key that
/// does not walk its direction; the rest work (up stating what walks it).
const BINDINGS: &str = "  c.actions = { \"act\", \"back\", \"info\" }
  c.keys = { act = { \"Space\" }, back = { \"Retrun\", \"X\" } }
  c.keys.ghost = { \"G\" }
  c.pad = {
    act = { \"a\" },
    back = { \"east\" },
    info = { \"rightshoulde\" },
  }
  c.keys.up = { \"W\", \"Up\" }
  c.keys.down = { \"S\", \"I\" }
";

/// The step names each binding that did nothing by its line, the Xbox
/// pad's letter with the place it means and a typo with the name meant,
/// says once what to do, and changes nothing; a game whose bindings all
/// work has nothing to change.
#[test]
fn dead_bindings_are_named_never_changed() {
    let games = old_games();
    let (_, dir) = games.iter().find(|(n, _)| *n == "key and button names checked").unwrap();
    let step = STEPS.iter().find(|s| s.name == "key and button names checked").unwrap();
    let before = files(dir);
    let planned = (step.plan)(dir).unwrap();
    assert_eq!(planned, (step.apply)(dir).unwrap());
    assert_eq!(files(dir), before);
    let at = |prefix: &str, words: &str| planned.iter().any(|l| l.starts_with(prefix) && l.contains(words));
    assert!(at("conf.lua:4: c.keys.back: \"Retrun\"", "(did you mean \"Return\"?)"), "{planned:#?}");
    assert!(at("conf.lua:5: c.keys.ghost:", "the game has no action ghost"), "{planned:#?}");
    assert!(at("conf.lua:7: c.pad.act: \"a\"", "the Xbox pad's A is \"south\""), "{planned:#?}");
    assert!(at("conf.lua:9: c.pad.info: \"rightshoulde\"", "(did you mean \"rightshoulder\"?)"), "{planned:#?}");
    assert!(at("conf.lua:12: c.keys.down: \"I\"", "does not walk down"), "{planned:#?}");
    assert_eq!(planned.len(), 6, "the five, and what to do once: {planned:#?}");
    assert!(planned[5].contains("nothing here is changed"), "{planned:#?}");
    put(dir, "conf.lua", "function conf(c)\n  c.voxcade = \"0.5.0\"\n  c.actions = { \"act\" }\n  c.keys.act = { \"Space\" }\n  c.pad.act = { \"south\", \"righttrigger\" }\nend\n");
    assert!((step.plan)(dir).unwrap().is_empty());
}

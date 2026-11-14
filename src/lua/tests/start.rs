//! `c.start`, the level a run begins in.

use super::*;

/// Levels `a` and `b` (the gate in `b`), `c.start` as given (none: unset).
fn folder_starting(name: &str, start: Option<&str>) -> crate::scratch::Scratch {
    let main = "function on_level_start(how) world.state.how = how end\n";
    let dir = levels_folder(name, main, &[("a", &[]), ("b", &[("gate", [8, 2, 4])])]);
    if let Some(start) = start {
        let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
        std::fs::write(dir.join("conf.lua"), conf.replace("\nend", &format!("\n  c.start = {start}\nend"))).unwrap();
    }
    dir
}

fn where_it_is(g: &mut Session<LuaGame>) -> (String, String) {
    let seen = g.game.inspect();
    (seen["at"]["name"].as_str().unwrap().to_string(), seen["state"]["how"].as_str().unwrap().to_string())
}

/// A run begins in the level `c.start` names, told "start"; unset, in the
/// first; a restart of the run (the game loaded again) begins there again;
/// `world.levels` keeps its order.
#[test]
fn a_run_begins_in_the_level_c_start_names() {
    let dir = folder_starting("named", Some("\"b\""));
    let mut g = game_in(&dir);
    assert_eq!(where_it_is(&mut g), ("b".into(), "start".into()));
    assert_eq!(g.game.inspect()["at"]["levels"], serde_json::json!(["a", "b"]));
    steps(&mut g, 3);
    assert_eq!(where_it_is(&mut g).0, "b");
    let mut again = game_in(&dir);
    assert_eq!(where_it_is(&mut again), ("b".into(), "start".into()), "F5 begins in it again");
    let plain = folder_starting("unset", None);
    assert_eq!(where_it_is(&mut game_in(&plain)), ("a".into(), "start".into()));
}

/// A name that is no level, a game without levels in `levels/` and a
/// non-string are mistakes naming `c.start`.
#[test]
fn a_wrong_c_start_is_a_mistake_naming_it() {
    let e = start_error(&folder_starting("wrong", Some("\"bb\"")));
    assert!(e.contains("c.start = \"bb\"") && e.contains("did you mean b?"), "{e}");
    let e = start_error(&folder_starting("far", Some("\"hut_outside\"")));
    assert!(e.contains("c.start = \"hut_outside\"") && e.contains("levels: a, b"), "{e}");
    let dir = folder("none", "");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("\nend", "\n  c.start = \"a\"\nend")).unwrap();
    let e = start_error(&dir);
    assert!(e.contains("c.start = \"a\"") && e.contains("no levels in levels/"), "{e}");
    let e = start_error(&folder_starting("number", Some("3")));
    assert!(e.contains("start"), "{e}");
}

/// What says where a run begins wins: a scenario's level, the run begun
/// there. A conf.lua read anew waits for a restart for it, and a wrong
/// name there is a mistake.
#[test]
fn a_scenario_wins_and_a_change_waits_for_f5() {
    let dir = folder_starting("wins", Some("\"b\""));
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/early.lua"), "return { about = \"In a\", level = \"a\" }\n").unwrap();
    let mut g = game_in(&dir);
    g.start_scenario("early").unwrap();
    assert_eq!(where_it_is(&mut g), ("a".into(), "start".into()));
    let mut g = game_in(&dir);
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("c.start = \"b\"", "c.start = \"a\"")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    assert_eq!(g.cfg.start.as_deref(), Some("b"), "kept until a restart");
    std::fs::write(dir.join("conf.lua"), conf.replace("c.start = \"b\"", "c.start = \"z\"")).unwrap();
    let e = g.reload(&dir.join("conf.lua")).map(|_| ()).unwrap_err();
    assert!(format!("{e:#}").contains("c.start = \"z\""), "{e:#}");
}

/// A scenario's level is where the run begins: the same run as one whose
/// `c.start` names it, the random numbers drawn alike, the start level
/// never entered (what its `on_level_start` did nowhere, no
/// `on_level_end`), the scenario's level visited once.
#[test]
fn a_scenario_begins_the_run_in_its_level_as_c_start_would() {
    let main = "function on_world_start() world.state.seed = math.random(1000) end\n\
        function on_level_start(how)\n  if level.name == \"a\" then world.state.in_a = math.random(1000) end\n  world.state.how = how\n  world.state.starts = (world.state.starts or 0) + 1\nend\n\
        function on_level_end(why) world.state.ended = why end\n\
        function on_tick() world.state.roll = math.random(1000) end\n";
    let scenario = levels_folder("scenario-b", main, &[("a", &[]), ("b", &[("gate", [8, 2, 4])])]);
    std::fs::create_dir_all(scenario.join("scenarios")).unwrap();
    std::fs::write(scenario.join("scenarios/in_b.lua"), "return { level = \"b\" }\n").unwrap();
    let started = levels_folder("start-b", main, &[("a", &[]), ("b", &[("gate", [8, 2, 4])])]);
    let conf = std::fs::read_to_string(started.join("conf.lua")).unwrap();
    std::fs::write(started.join("conf.lua"), conf.replace("\nend", "\n  c.start = \"b\"\nend")).unwrap();
    let mut s = game_in(&scenario);
    s.start_scenario("in_b").unwrap();
    let mut c = game_in(&started);
    let seen = s.game.inspect();
    assert_eq!((&seen["at"]["name"], &seen["at"]["visits"], &seen["state"]["starts"], &seen["state"]["how"]), (&"b".into(), &1.into(), &1.into(), &"start".into()), "{seen}");
    assert!(seen["state"]["in_a"].is_null() && seen["state"]["ended"].is_null(), "level a never entered: {seen}");
    assert_eq!(seen, c.game.inspect());
    for tick in 0..30 {
        assert_eq!((s.game.state_hash(), s.start_mark()), (c.game.state_hash(), c.start_mark()), "tick {tick}");
        s.step(&Input::default());
        c.step(&Input::default());
    }
    // A scenario naming the start level enters it once, as every run.
    std::fs::write(scenario.join("scenarios/in_a.lua"), "return { level = \"a\" }\n").unwrap();
    let mut s = game_in(&scenario);
    s.start_scenario("in_a").unwrap();
    let seen = s.game.inspect();
    assert_eq!((&seen["at"]["name"], &seen["at"]["visits"], &seen["state"]["starts"]), (&"a".into(), &1.into(), &1.into()), "{seen}");
    assert_eq!(seen, game_in(&scenario).game.inspect());
}

/// A run recorded from `c.start` plays back the same; the setting is part
/// of the game's data, and a game without it adds nothing to the
/// fingerprint.
#[test]
fn a_run_from_c_start_replays_and_unset_changes_no_fingerprint() {
    use crate::replay::{Recorder, check};
    let dir = folder_starting("replay", Some("\"b\""));
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..60 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/b.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "same");
    let set = g.fingerprint().unwrap();
    let kept = folder_starting("print", None);
    let plain = game_in(&kept);
    assert_ne!(plain.fingerprint().unwrap(), set, "starting elsewhere is another run");
    let text = toml::to_string(&toml::Value::try_from(&plain.cfg).unwrap()).unwrap();
    assert!(!text.contains("start ="), "unset is not written: {text}");
}

//! Scenarios and the console.

use super::*;

/// What a script prints is kept for the IDE (OUTPUT) until asked for.
#[test]
fn what_a_script_prints_is_kept_for_the_ide() {
    let mut g = game("printed", "function on_tick()\n  if world.tick == 3 then print(\"hello\", 42) end\nend\n");
    steps(&mut g, 5);
    assert_eq!(g.printed(), ["hello\t42"]);
    assert!(g.printed().is_empty(), "once");
}

/// A scenario (`scenarios/NAME.lua`) begins a run late: after the start,
/// in its level at its place, its `setup` setting the states; a console
/// line runs between two ticks and answers. A run begun so, with lines run
/// in the middle of it, is recorded and plays back the same; the
/// scenario's file counts for the replay's data.
#[test]
fn a_scenario_and_console_lines_are_part_of_the_run() {
    use crate::replay::{Recorder, Replay, check};
    let main = "function on_world_start() world.state.day = 1 end\nfunction on_level_start(how) level.state.how = how end\nfunction on_tick() world.state.day = world.state.day + 1 end\n";
    let dir = levels_folder("scenario", main, &[("a", &[]), ("b", &[("gate", [8, 2, 4])])]);
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    let late = "return {\n  about = \"Day 30 in b\",\n  level = \"b\", place = \"gate\",\n  setup = function()\n    world.state.day = 30\n    level.state.set = true\n  end,\n}\n";
    std::fs::write(dir.join("scenarios/late.lua"), late).unwrap();
    let mut g = game_in(&dir);
    assert_eq!(g.scenarios().unwrap(), vec![("late".to_string(), "Day 30 in b".to_string())]);
    g.start_scenario("late").unwrap();
    let seen = g.game.inspect();
    assert_eq!((&seen["at"]["name"], &seen["level"]["how"], &seen["level"]["set"]), (&"b".into(), &"go_to".into(), &true.into()), "{seen}");
    assert_eq!(state(&mut g, "day"), format!("{:?}", crate::lua::Value::Int(30)));
    let mut recorder = Recorder::default();
    for n in 0..120 {
        if n == 30 {
            assert_eq!(g.eval("world.state.day").unwrap(), "60", "an expression answers");
            assert_eq!(g.eval("world.state.day = 1000").unwrap(), "", "a statement says nothing");
        }
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(state(&mut g, "day"), format!("{:?}", crate::lua::Value::Int(1090)));
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    let replay = recorder.replay(&g).unwrap();
    assert_eq!((replay.scenario.as_deref(), replay.evals.len(), replay.evals[1].at), (Some("late"), 2, 30));
    replay.save(&dir.join("replays/late.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "it plays back the same");
    let back = Replay::load(&dir.join("replays/late.toml")).unwrap();
    assert_eq!(back, replay, "written and read back as it was");
    // The scenario changed: the replay was recorded on other data.
    std::fs::write(dir.join("scenarios/late.lua"), late.replace("= 30", "= 31")).unwrap();
    assert!(!check::<LuaGame>(&dir, false).unwrap(), "the scenario's file counts");
}

/// Every game of the engine's and every template keeps scenarios, its
/// late situations set up in code (docs/game-model.md, "Scenarios and the
/// console"): without them a game can hardly be tried. Each says what it is
/// about and begins a run that plays and draws on. The Bench is a load
/// test, no game.
#[test]
fn every_game_and_template_has_scenarios() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut folders: Vec<PathBuf> = ["games", "templates"]
        .iter()
        .flat_map(|parent| std::fs::read_dir(root.join(parent)).unwrap().flatten().map(|e| e.path()))
        .filter(|dir| dir.join("conf.lua").is_file() && !dir.ends_with("games/bench"))
        .collect();
    folders.sort();
    assert!(folders.len() >= 9, "{folders:?}");
    for dir in folders {
        let scenarios = Session::<LuaGame>::load(&dir).unwrap().scenarios().unwrap();
        assert!(!scenarios.is_empty(), "{} has no scenarios (scenarios/NAME.lua)", dir.display());
        for (name, about) in scenarios {
            assert!(!about.is_empty() && !about.starts_with("(a mistake"), "{}: {name}: {about}", dir.display());
            let mut g = Session::<LuaGame>::load(&dir).unwrap();
            g.start_scenario(&name).unwrap_or_else(|e| panic!("{}: {name}: {e:#}", dir.display()));
            for _ in 0..60 {
                g.step(&Input::default());
                g.look();
            }
            assert_eq!(halted(&g), "", "{}: {name}", dir.display());
        }
    }
}

/// A run the console or a scenario helped is told as such (its high score
/// goes in as GOD); a line that went wrong was run all the same.
#[test]
fn a_run_helped_by_the_console_or_a_scenario_is_told() {
    let dir = folder("helped", "function on_tick() end\n");
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/late.lua"), "return { about = \"late\", setup = function() end }\n").unwrap();
    let mut fair = game_in(&dir);
    steps(&mut fair, 10);
    assert!(!fair.helped(), "played as it is");
    let mut begun = game_in(&dir);
    begun.start_scenario("late").unwrap();
    assert!(begun.helped(), "begun from a scenario");
    let mut lined = game_in(&dir);
    steps(&mut lined, 5);
    lined.eval("1 + 1").unwrap();
    assert!(lined.helped(), "a console line");
    let mut wrong = game_in(&dir);
    assert!(wrong.eval("nothing.here()").is_err());
    assert!(wrong.helped(), "a line that went wrong");
}

/// What goes wrong with a scenario or a console line says where: a
/// scenario the game lacks names the ones it has, a scenario's table is
/// checked, a mistake in its setup halts the game with its line. A
/// mistake in a console line is answered and the game goes on; one that
/// leaves a state holding what is no plain data stops it, as at a tick's
/// end.
#[test]
fn scenarios_and_console_lines_say_what_went_wrong() {
    let dir = folder("scenario-mistakes", "function on_tick() end\n");
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    for (name, src) in [
        ("plain", "return 3\n"),
        ("odd", "return { setup = function() end, start = 1 }\n"),
        ("bare", "return { about = \"no setup\" }\n"),
        ("broken", "return {\n  setup = function()\n    world.state.x = nothing.here\n  end,\n}\n"),
    ] {
        std::fs::write(dir.join(format!("scenarios/{name}.lua")), src).unwrap();
    }
    for (name, says) in [
        ("gone", "no scenario 'gone' (the game's: bare, broken, odd, plain)"),
        ("plain", "scenarios/plain.lua: a scenario gives back a table"),
        ("odd", "scenarios/odd.lua: no field called 'start' (about, level, place, setup)"),
        ("bare", "scenarios/bare.lua: a scenario says where the run begins (level = \"name\", place = \"name\") or sets it up"),
        ("broken", "scenarios/broken.lua:3:"),
    ] {
        let mut g = game_in(&dir);
        let e = format!("{:#}", g.start_scenario(name).unwrap_err());
        assert!(e.contains(says), "{name}: {e}");
    }
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    assert!(g.start_scenario("bare").unwrap_err().to_string().contains("before its first tick"), "a scenario begins a run");
    let e = g.eval("nothing.here = 1").unwrap_err().to_string();
    assert!(e.contains("console:1:") && halted(&g).is_empty(), "answered, the game goes on: {e}");
    assert_eq!(g.eval("print('hi') return 1, 'two'").unwrap(), "hi\n1\t\"two\"", "what it printed, then its values");
    assert!(g.eval("world.state.f = function() end").is_err());
    assert!(halted(&g).contains("world.state"), "a state that is no plain data stops it: {}", halted(&g));
}

/// A scenario that only names where the run begins (a level, a place
/// there) needs no setup; one that says nothing at all is a mistake.
#[test]
fn a_scenario_may_only_name_a_level() {
    let dir = levels_folder("scenario-level", "function on_level_start(how) world.state.how = how end\n", &[("a", &[]), ("b", &[("door", [6, 2, 4])])]);
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/in_b.lua"), "return { about = \"the second room\", level = \"b\" }\n").unwrap();
    std::fs::write(dir.join("scenarios/at_door.lua"), "return { level = \"b\", place = \"door\" }\n").unwrap();
    let mut g = game_in(&dir);
    g.start_scenario("in_b").unwrap();
    assert_eq!((said(&mut g, "level.name"), halted(&g)), ("\"b\"".to_string(), String::new()));
    let mut g = game_in(&dir);
    g.start_scenario("at_door").unwrap();
    assert_eq!(said(&mut g, "level.arrived.name"), "\"door\"");
    steps(&mut g, 5);
    assert_eq!(halted(&g), "");
}

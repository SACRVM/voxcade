//! A game's own title (`c.title_screen.level`): a level played before a
//! run that counts for nothing, `world:new_game()`, `world:load` there,
//! `world:open` and `world:quit`.

use super::*;
use crate::game::Request;

/// What the title's game does: its states, each tick what
/// `world.state.ask` says (a console line sets it).
const MAIN: &str = "function on_world_start() world.state.seed = math.random(1000) end\n\
    function on_level_start(how) world.state.how = how end\n\
    function on_tick()\n  world.state.ticks = (world.state.ticks or 0) + 1\n  local ask = world.state.ask\n  \
    if ask == \"new\" then world:new_game() elseif ask == \"load\" then world:load(1) end\nend\n";

/// Levels `title` (the game's own title, first by name), `zone_1` and
/// `zone_2`, two save slots, `main.lua` as given.
pub(crate) fn title_folder(name: &str, main: &str) -> crate::scratch::Scratch {
    let dir = levels_folder(name, main, &[("title", &[]), ("zone_1", &[]), ("zone_2", &[])]);
    with_conf(&dir, "  c.title_screen.level = \"title\"\n  c.saves = { slots = 2 }\n");
    dir
}

/// `lines` added to the folder's `conf.lua`.
fn with_conf(dir: &Path, lines: &str) {
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("\nend", &format!("\n{lines}end"))).unwrap();
}

/// Where the game is and how it came there.
fn at(g: &mut Session<LuaGame>) -> (String, String) {
    let seen = g.game.inspect();
    (seen["at"]["name"].as_str().unwrap_or("").to_string(), seen["state"]["how"].as_str().unwrap_or("").to_string())
}

/// A game loaded begins its runs in the first level but the title; begun
/// for the window it plays its title (`on_world_start`, then
/// `on_level_start("start")` there). `world:new_game()` there begins a
/// run at the tick's end, the very run a fresh load begins: tick 0, the
/// states fresh, nothing of the title kept, and that step no tick of it.
#[test]
fn the_title_begins_a_fresh_run_in_the_start_level() {
    let dir = title_folder("fresh", MAIN);
    let mut g = game_in(&dir);
    assert_eq!(at(&mut g), ("zone_1".into(), "start".into()), "a run begins after the title");
    assert!(!g.on_title());
    g.begin_title().unwrap();
    assert!(g.on_title());
    assert_eq!(at(&mut g), ("title".into(), "start".into()));
    assert_eq!(said(&mut g, "level.name"), "\"title\"");
    assert_eq!(said(&mut g, "#world.levels"), "3", "world.levels lists it");
    steps(&mut g, 20);
    assert_eq!(state(&mut g, "ticks"), format!("{:?}", crate::lua::Value::Int(20)));
    said(&mut g, "world.state.ask = \"new\"");
    g.step(&Input::default());
    assert!(std::mem::take(&mut g.begun) && !g.ticked && !g.on_title(), "a run began, the step was the title's");
    let fresh = game_in(&dir);
    assert_eq!(at(&mut g), ("zone_1".into(), "start".into()));
    assert_eq!(g.game.inspect(), fresh.game.inspect(), "the run a fresh load begins");
    assert_eq!((g.game.sim().tick, g.game.state_hash(), g.start_mark()), (0, fresh.game.state_hash(), fresh.start_mark()));
    assert!(g.evals.is_empty() && !g.helped(), "the title's console lines are no part of the run");
    steps(&mut g, 5);
    assert_eq!(state(&mut g, "ticks"), format!("{:?}", crate::lua::Value::Int(5)));
}

/// On the title it is no run: saving, going to a level, winning and losing
/// are mistakes saying to begin one; in a run `world:new_game()` is, and
/// going to the title. `level.next` passes the title over.
#[test]
fn the_title_is_no_run() {
    let dir = title_folder("no-run", MAIN);
    let mut g = game_in(&dir);
    g.begin_title().unwrap();
    for (line, says) in [
        ("world:save(1)", "world:save: no saving on the title"),
        ("world:go_to(\"zone_1\")", "world:go_to: the title (c.title_screen.level) is no run: world:new_game() begins one"),
        ("world:win()", "world:win: the title"),
        ("world:lose()", "world:lose: the title"),
    ] {
        let e = said(&mut g, line);
        assert!(e.contains(says), "{line}: {e}");
    }
    assert_eq!(halted(&g), "", "a console line's mistake stops nothing");
    let mut g = game_in(&dir);
    let e = said(&mut g, "world:new_game()");
    assert!(e.contains("world:new_game: from the game's own title only"), "{e}");
    let e = said(&mut g, "world:go_to(\"title\")");
    assert!(e.contains("'title' is the game's title"), "{e}");
    assert_eq!(said(&mut g, "level.next"), "\"zone_2\"");
    // A title between two levels: the first goes on to the one after it.
    let dir = levels_folder("between", MAIN, &[("a", &[]), ("b", &[]), ("c", &[])]);
    with_conf(&dir, "  c.title_screen.level = \"b\"\n");
    let mut g = game_in(&dir);
    assert_eq!((at(&mut g).0, said(&mut g, "level.next")), ("a".into(), "\"c\"".into()));
}

/// `world:load(slot)` on the title begins the run from the save, as
/// CONTINUE does: the same game as one loaded afresh and gone on from it.
#[test]
fn a_load_on_the_title_goes_on_from_the_save() {
    let dir = title_folder("load", MAIN);
    let mut g = game_in(&dir);
    said(&mut g, "world.state.kept = 7");
    steps(&mut g, 3);
    g.save_to(1).unwrap();
    let text = g.slots.read(1).unwrap().to_string();
    let slots = || crate::saves::Slots::holding([(1, text.clone())].into());
    let mut g = Session::<LuaGame>::load_with(&dir, slots()).unwrap();
    g.begin_title().unwrap();
    assert_eq!(said(&mut g, "world.saves[1].level"), "\"zone_1\"", "world.saves reads there");
    said(&mut g, "world.state.ask = \"load\"");
    g.step(&Input::default());
    assert!(std::mem::take(&mut g.begun) && !g.on_title());
    assert_eq!(state(&mut g, "kept"), format!("{:?}", crate::lua::Value::Int(7)));
    assert_eq!(g.started_from.as_deref(), Some(text.as_str()), "the run's replay begins from the save");
    let mut continued = Session::<LuaGame>::load_with(&dir, slots()).unwrap();
    continued.start_from(&text).unwrap();
    assert_eq!(g.game.inspect(), continued.game.inspect());
    assert_eq!(g.game.state_hash(), continued.game.state_hash());
}

/// `world:open` and `world:quit` are asked of the machine: the game's
/// state goes as without them, and each is told once, in its tick. A page
/// the engine has not is a mistake naming those it has.
#[test]
fn open_and_quit_are_the_machines_not_the_games() {
    let asks = "function on_tick()\n  world.state.n = (world.state.n or 0) + 1\n  if world.tick == 3 then world:open(\"settings\") world:open(\"settings\") world:quit() end\nend\n";
    let plain = "function on_tick()\n  world.state.n = (world.state.n or 0) + 1\nend\n";
    let (dir, quiet) = (title_folder("asks", asks), title_folder("plain", plain));
    for title in [true, false] {
        let (mut g, mut q) = (game_in(&dir), game_in(&quiet));
        if title {
            g.begin_title().unwrap();
            q.begin_title().unwrap();
        }
        let mut told = Vec::new();
        for tick in 0..8 {
            g.step(&Input::default());
            q.step(&Input::default());
            assert_eq!(g.game.state_hash(), q.game.state_hash(), "tick {tick}");
            told.push(g.requests());
        }
        told.retain(|t| !t.is_empty());
        assert_eq!(told, vec![vec![Request::Open("settings"), Request::Quit]], "once, in its tick");
    }
    let mut g = game_in(&dir);
    let e = said(&mut g, "world:open(\"map\")");
    assert!(e.contains("world:open: no page called 'map' (the engine's: settings)"), "{e}");
}

/// `world:to_title()` in a run is asked of the machine as `world:quit()`
/// is: the game's state goes as without it, told once in its tick, in a
/// game with its own title and one without. On the title itself it is a
/// mistake, as the run's asks are there.
#[test]
fn to_title_is_the_machines_and_no_ask_on_the_title() {
    let asks = "function on_tick()\n  world.state.n = (world.state.n or 0) + 1\n  if world.tick == 3 then world:to_title() world:to_title() end\nend\n";
    let plain = "function on_tick()\n  world.state.n = (world.state.n or 0) + 1\nend\n";
    let (titled, quiet) = (title_folder("to-title", asks), title_folder("to-title-plain", plain));
    let untitled = folder("to-title-none", asks);
    for (n, dir) in [&titled, &untitled].into_iter().enumerate() {
        let (mut g, mut q) = (game_in(dir), game_in(&quiet));
        let mut told = Vec::new();
        for tick in 0..8 {
            g.step(&Input::default());
            q.step(&Input::default());
            if n == 0 {
                assert_eq!(g.game.state_hash(), q.game.state_hash(), "tick {tick}");
            }
            told.push(g.requests());
        }
        told.retain(|t| !t.is_empty());
        assert_eq!(told, vec![vec![Request::ToTitle]], "once, in its tick");
    }
    let mut g = game_in(&titled);
    g.begin_title().unwrap();
    let e = said(&mut g, "world:to_title()");
    assert!(e.contains("world:to_title: the title (c.title_screen.level) is no run"), "{e}");
    assert!(g.requests().is_empty());
}

/// `c.title_screen.level` is checked as the game loads and as `conf.lua`
/// is read anew: a name that is no level (the near one said), the level a
/// run begins in, the only level.
#[test]
fn a_wrong_title_level_is_a_mistake_naming_it() {
    let dir = levels_folder("titel", MAIN, &[("title", &[]), ("zone_1", &[])]);
    with_conf(&dir, "  c.title_screen.level = \"titel\"\n");
    let e = start_error(&dir);
    assert!(e.contains("c.title_screen.level = \"titel\"") && e.contains("did you mean title?"), "{e}");
    let dir = levels_folder("same", MAIN, &[("title", &[]), ("zone_1", &[])]);
    with_conf(&dir, "  c.title_screen.level = \"title\"\n  c.start = \"title\"\n");
    let e = start_error(&dir);
    assert!(e.contains("c.title_screen.level = \"title\" is c.start too"), "{e}");
    let dir = levels_folder("alone", MAIN, &[("title", &[])]);
    with_conf(&dir, "  c.title_screen.level = \"title\"\n");
    let e = start_error(&dir);
    assert!(e.contains("a run begins in another level"), "{e}");
    let dir = title_folder("anew", MAIN);
    let mut g = game_in(&dir);
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("level = \"title\"", "level = \"zone_9\"")).unwrap();
    let e = format!("{:#}", g.reload(&dir.join("conf.lua")).unwrap_err());
    assert!(e.contains("c.title_screen.level = \"zone_9\""), "{e}");
}

/// A game without the setting is as before: unset or set to nil, the same
/// fingerprint, nothing written for it; set, the runs begin elsewhere.
#[test]
fn a_game_without_its_own_title_is_unchanged() {
    let main = "function on_level_start(how) world.state.how = how end\n";
    let unset = levels_folder("unset", main, &[("a", &[]), ("b", &[])]);
    let nil = levels_folder("nil", main, &[("a", &[]), ("b", &[])]);
    with_conf(&nil, "  c.title_screen.level = nil\n");
    let set = levels_folder("set", main, &[("a", &[]), ("b", &[])]);
    with_conf(&set, "  c.title_screen.level = \"a\"\n");
    let (mut unset, nil, mut set) = (game_in(&unset), game_in(&nil), game_in(&set));
    assert_eq!(unset.fingerprint().unwrap(), nil.fingerprint().unwrap());
    assert_ne!(unset.fingerprint().unwrap(), set.fingerprint().unwrap());
    assert_eq!((at(&mut unset).0, at(&mut set).0), ("a".into(), "b".into()));
    assert!(unset.begin_title().is_err(), "no title of its own");
    let text = toml::to_string(&toml::Value::try_from(&nil.cfg).unwrap()).unwrap();
    assert!(!text.contains("level ="), "unset is not written: {text}");
}

/// Nothing of the title is recorded or kept: the run its title began is,
/// from its first tick, and plays back as recorded.
#[test]
fn nothing_of_the_title_is_recorded() {
    let dir = title_folder("recorded", MAIN);
    let mut g = game_in(&dir);
    g.begin_title().unwrap();
    let mut runs = crate::replay::Runs::new(&dir);
    runs.start(&g);
    for n in 0..30 {
        if n == 20 {
            said(&mut g, "world.state.ask = \"new\"");
        }
        g.step(&Input::default());
        if std::mem::take(&mut g.begun) {
            runs.start(&g);
        }
        runs.step(&g);
        if g.on_title() {
            assert!(format!("{:#}", runs.keep(&g).unwrap_err()).contains("the title"), "F9 keeps nothing there");
        }
    }
    let kept = runs.keep(&g).unwrap();
    let replay = crate::replay::Replay::load(&kept).unwrap();
    assert_eq!(replay.ticks, 9, "the run's ticks alone");
    assert!(crate::replay::check::<LuaGame>(&dir, false).unwrap(), "same");
}

/// A scenario on the title begins a title run (the maker tries it), and its
/// new game is a plain run: no scenario, not helped.
#[test]
fn a_scenario_may_begin_on_the_title() {
    let dir = title_folder("scenario", MAIN);
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/menu.lua"), "return { level = \"title\" }\n").unwrap();
    let mut g = game_in(&dir);
    g.start_scenario("menu").unwrap();
    assert!(g.on_title());
    said(&mut g, "world.state.ask = \"new\"");
    g.step(&Input::default());
    assert!(!g.on_title() && g.scenario.is_none() && !g.helped());
    assert_eq!(at(&mut g), ("zone_1".into(), "start".into()));
}

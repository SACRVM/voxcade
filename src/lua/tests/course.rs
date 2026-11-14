//! The run's course: levels left and entered, asks, checkpoints, fades,
//! the run's end.

use super::*;

/// The levels are the files in `levels/` in the order of their names;
/// `world:go_to` and `world:restart` change at the tick's end, through
/// `on_level_end(why)` and `on_level_start(how)`, `level.state` fresh each
/// time and `world.state` kept; a place sets where the level is entered;
/// `world:win()` ends the run through `on_level_end` and `on_world_end`.
#[test]
fn a_run_goes_from_level_to_level() {
    let main = r#"
function on_world_start() world.state.log = "" end

function on_level_start(how)
  world.state.log = world.state.log .. level.number .. level.name .. ":" .. how .. ";"
  level.state.fresh = (level.state.fresh or 0) + 1
  world.state.fresh = level.state.fresh
  world.state.visits = level.visits
  world.state.places = level.places.east and "east" or "-"
end

function on_level_end(why) world.state.log = world.state.log .. "end:" .. why .. ";" end
function on_world_end(result) world.state.result = result end

function on_tick()
  if world.tick == 2 or world.tick == 4 then world:go_to(level.next) world.state.asked = world.leaving end
  if world.tick == 6 then world:restart() end
  if world.tick == 8 then world:go_to(1, "east") end
  if world.tick == 10 then world:win() end
end
"#;
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
    let dir = levels_folder("run", main, &[("level10", &[]), ("level2", &[]), ("level1", east)]);
    let mut g = game_in(&dir);
    steps(&mut g, 8);
    assert_eq!(halted(&g), "");
    let cam = g.game.sim().cam_target;
    assert_eq!((cam.x, cam.y), (12.5 + 16.0 * 0.0 + 0.0, 2.5), "entered at the place");
    steps(&mut g, 2);
    assert_eq!(state(&mut g, "log"), format!("{:?}", crate::lua::Value::Str(b"1level1:start;end:go_to;2level2:go_to;end:go_to;3level10:go_to;end:restart;3level10:restart;end:go_to;1level1:go_to;end:won;".to_vec())));
    assert_eq!(state(&mut g, "result"), format!("{:?}", crate::lua::Value::Str(b"won".to_vec())));
    assert_eq!(state(&mut g, "fresh"), format!("{:?}", crate::lua::Value::Int(1)), "level.state is fresh on every entry");
    assert_eq!(state(&mut g, "visits"), format!("{:?}", crate::lua::Value::Int(2)));
    assert_eq!(state(&mut g, "asked"), format!("{:?}", crate::lua::Value::Str(b"go_to".to_vec())));
    assert_eq!(g.game.outcome(), Some(crate::game::Outcome::Won));
}

/// One ask counts in a tick: going on outranks starting over, among equals
/// the first; the ones that did not count are noted with their line.
#[test]
fn one_ask_counts_and_going_on_wins() {
    let main = "function on_tick()\n  if world.tick == 2 then\n    world:restart()\n    world:go_to(level.next)\n    world:lose()\n    world:go_to(level.next)\n  end\n  world.state.at = level.name\nend\n";
    let dir = levels_folder("asks", main, &[("a", &[]), ("b", &[])]);
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(state(&mut g, "at"), format!("{:?}", crate::lua::Value::Str(b"b".to_vec())));
    let notes = g.printed();
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[0].starts_with("main.lua:4: world:go_to takes the place of the world:restart"), "{notes:?}");
    assert!(notes[1].starts_with("main.lua:5: world:lose does not count"), "{notes:?}");
}

/// An ask in `on_level_start` counts at once (no tick between); one that
/// never stops is a mistake; asks where there is no level, or where it is
/// left already, and names that do not exist, are mistakes with their
/// line.
#[test]
fn asks_have_their_places_and_their_names() {
    let start = "function on_level_start(how)\n  world.state.log = (world.state.log or '') .. level.name\n  if level.name == 'a' then world:go_to('b') end\nend\n";
    let mut g = game_in(&levels_folder("at-once", start, &[("a", &[]), ("b", &[])]));
    assert_eq!(state(&mut g, "log"), format!("{:?}", crate::lua::Value::Str(b"ab".to_vec())), "before the first tick");
    let looping = "function on_level_start(how) world:restart() end\n";
    assert!(start_error(&levels_folder("looping", looping, &[("a", &[])])).contains("levels keep being left"));
    for (name, main, says) in [
        ("in-world-start", "function on_world_start()\n  world:go_to(1)\nend\n", "world:go_to: there is no level to leave yet"),
        ("in-level-end", "function on_tick() world:go_to(2) end\nfunction on_level_end(why)\n  world:restart()\nend\n", "world:restart: the level is being left already"),
        ("no-level", "function on_tick() world:go_to('nowhere') end\n", "world:go_to: no level called 'nowhere' (levels: a, b)"),
        ("no-place", "function on_tick() world:go_to('b', 'west') end\n", "no place called 'west' in level 'b' (its places: start, east)"),
        ("after-last", "function on_tick() if level.name == 'b' then world:go_to(level.next) else world:go_to(2) end end\n", "world:go_to: which level?"),
    ] {
        let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
        let mut g = game_in(&levels_folder(name, main, &[("a", &[]), ("b", east)]));
        steps(&mut g, 3);
        let e = if halted(&g).is_empty() { start_error(&levels_folder(name, main, &[("a", &[]), ("b", east)])) } else { halted(&g) };
        assert!(e.contains(says), "{name}: {e}");
    }
}

/// `level.kept` lasts the run, a level each; a checkpoint is where a
/// restart goes, forgotten when another level is entered; a game kept at a
/// tick and brought back plays on the same across level changes.
#[test]
fn kept_checkpoints_and_snapshots_cross_levels() {
    let main = r#"
function on_level_start(how)
  level.kept.entered = (level.kept.entered or 0) + 1
  world.state["kept_" .. level.name] = level.kept.entered
  world.state.checkpoint = player.state.checkpoint or "-"
end

function on_tick()
  local t = world.tick
  if t == 2 then player.state.checkpoint = "east" world:restart() end
  if t == 4 then world:go_to("b") end
  if t == 6 then world:go_to("a") end
  if t == 8 then world:restart() end
end
"#;
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
    let mut g = game_in(&levels_folder("kept", main, &[("a", east), ("b", &[])]));
    steps(&mut g, 3);
    assert_eq!(state(&mut g, "checkpoint"), format!("{:?}", crate::lua::Value::Str(b"east".to_vec())));
    assert_eq!(g.game.sim().cam_target.x, 12.5, "the restart went to the checkpoint");
    let kept_at = g.game.state_hash();
    let snapshot = g.snapshot().unwrap();
    let mut first = Vec::new();
    for _ in 0..8 {
        steps(&mut g, 1);
        first.push(g.game.state_hash());
    }
    assert_eq!(state(&mut g, "kept_a"), format!("{:?}", crate::lua::Value::Int(4)));
    assert_eq!(state(&mut g, "checkpoint"), format!("{:?}", crate::lua::Value::Str(b"-".to_vec())), "forgotten on the way to b");
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), kept_at);
    for (n, hash) in first.iter().enumerate() {
        steps(&mut g, 1);
        assert_eq!(g.game.state_hash(), *hash, "tick {} after the snapshot played differently", n + 1);
    }
}

/// Where level changes fade, the fade's frames are no ticks: the game
/// holds still, the change comes between out and in, and a run recorded so
/// plays the same where changes cut (`check`).
#[test]
fn a_fade_is_no_tick() {
    use crate::replay::{Recorder, check};
    let main = "function on_tick()\n  if world.tick == 2 then world:go_to(2) end\n  world.state.at = level.name\nend\n";
    let dir = levels_folder("fade", main, &[("a", &[]), ("b", &[])]);
    let mut g = game_in(&dir);
    g.fades = true;
    let mut recorder = Recorder::default();
    let mut fading = 0;
    for _ in 0..90 {
        g.step(&Input::default());
        fading += !g.ticked as u32;
        recorder.record(&g);
    }
    assert!((34..=40).contains(&fading), "{fading} frames of fade (0.3 s out, 0.3 s in)");
    assert_eq!(g.game.sim().tick, 90 - fading as u64, "no ticks while fading");
    assert_eq!(state(&mut g, "at"), format!("{:?}", crate::lua::Value::Str(b"b".to_vec())));
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "the run plays the same without the fade");
}

/// The ready-made rules keep their run from level to level (the score, the
/// lives); the player stands where the next level starts.
#[test]
fn the_ready_made_rules_keep_their_run_across_levels() {
    let main = "function on_tick()\n  if world.tick == 2 then player.state.score = 50 end\n  if world.tick == 3 then world:go_to('b') end\n  world.state.at = level.name\nend\n";
    let dir = kit_folder("kit-levels", "", main);
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    for name in ["a", "b"] {
        std::fs::copy(dir.join("world.vvx"), dir.join("levels").join(format!("{name}.vvx"))).unwrap();
    }
    let mut g = game_in(&dir);
    let lives = g.game.inspect()["player"]["state"]["lives"].clone();
    let start = g.game.focus().map(|b| b.pos).unwrap();
    steps(&mut g, 2);
    g.game.place_player(start.x + 20.0, start.y + 20.0);
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "at"), format!("{:?}", crate::lua::Value::Str(b"b".to_vec())));
    assert_eq!(g.game.score(), 50, "the score went along");
    assert_eq!(g.game.inspect()["player"]["state"]["lives"], lives, "the lives went along");
    let at = g.game.focus().map(|b| b.pos).unwrap();
    assert!((at.x - start.x).abs() < 1.0 && (at.y - start.y).abs() < 1.0, "the player at the level's start, not where it stood: {at} vs {start}");
}

/// A level file saved while playing enters the level afresh (`"reload"`);
/// QUIT TO TITLE tells the script (`"quit"`).
#[test]
fn a_reload_and_a_quit_are_told() {
    let main = "function on_level_start(how) world.state.log = (world.state.log or '') .. 'start:' .. how .. ';' end\nfunction on_level_end(why) world.state.log = world.state.log .. 'end:' .. why .. ';' end\nfunction on_world_end(result) world.state.log = world.state.log .. 'world:' .. result end\n";
    let dir = levels_folder("reload-quit", main, &[("a", &[])]);
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let level = crate::grid::GridLevel::read(&dir.join("levels/a.toml")).unwrap();
    level.save(&dir.join("levels/a.toml")).unwrap();
    assert!(matches!(g.reload(&dir.join("levels/a.toml")).unwrap(), crate::session::Reloaded::World));
    g.game.quit();
    assert_eq!(state(&mut g, "log"), format!("{:?}", crate::lua::Value::Str(b"start:start;end:reload;start:reload;end:quit;world:quit".to_vec())));
}

/// After a level change every chunk the level before showed is drawn anew;
/// a level's start is found on its own ground, and `level.arrived` says
/// where the player came in.
#[test]
fn a_level_change_shows_the_next_level_where_it_starts() {
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind};
    let main = "function on_level_start(how)\n  world.state['start_' .. level.name] = level.places.start.z\n  world.state.arrived = level.arrived.name .. ' ' .. level.arrived.x\nend\nfunction on_tick() if world.tick == 2 then world:go_to('b', 'east') end end\n";
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 8])];
    let dir = levels_folder("next-start", main, &[("a", &[]), ("b", east)]);
    let mut taller = GridLevel::new(4, glam::IVec3::new(4, 1, 2));
    taller.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    taller.cells.fill(Cell { block: 1, turn: Default::default() });
    taller.places = vec![crate::grid::Place { name: "east".into(), at: [12, 2, 8], facing: None, named: false, line: 0 }];
    taller.save(&dir.join("levels/b.toml")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 1);
    let _ = g.world_mut().take_dirty();
    steps(&mut g, 2);
    assert!(!g.world_mut().take_dirty().is_empty(), "the chunks drawn anew");
    let (a, b) = (state(&mut g, "start_a"), state(&mut g, "start_b"));
    assert_ne!(a, b, "each level's start on its own ground: {a} {b}");
    assert_eq!(state(&mut g, "arrived"), format!("{:?}", crate::lua::Value::Str(b"east 12.5".to_vec())));
}

/// The shooter's player stands on a world the script builds after a
/// restart too, not in it.
#[test]
fn a_restart_stands_the_player_on_what_was_built() {
    let main = "function on_level_start(how)\n  level:fill(0, 0, 0, 511, 511, 3, \"#c2a878\")\nend\nfunction on_tick() if world.tick == 2 then world:restart() end end\n";
    let shooter = kit_folder("built-restart", "", main);
    let conf = std::fs::read_to_string(shooter.join("conf.lua")).unwrap().replace("c.world.file = \"world.vvx\"", "c.world.size = { 512, 512, 48 }");
    std::fs::write(shooter.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&shooter);
    steps(&mut g, 10);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.focus().map(|b| b.pos.z), Some(4.0), "on the sand after the restart");
}

/// A level without a place called start starts where its ready-made rules
/// stand the player: a restart puts the platformer's hero back on its
/// marker, facing as its rules turn it (along the level, not at the
/// camera), and `level.places.start` is there.
#[test]
fn a_restart_without_a_start_place_goes_back_to_the_marker() {
    let main = "function on_level_start(how) world.state.start = level.places.start.x end\nfunction on_tick() if world.tick == 30 then world:restart() end end\n";
    let dir = platformer_levels("marker-restart", main, "", &[]);
    let mut g = game_in(&dir);
    let start = g.game.focus().map(|b| b.pos).unwrap();
    for _ in 0..20 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert!(g.game.focus().unwrap().pos.x > start.x + 2.0, "it ran");
    steps(&mut g, 15);
    assert_eq!(halted(&g), "");
    let back = g.game.focus().unwrap();
    assert_eq!((back.pos.x, back.pos.y), (start.x, start.y), "back on its marker");
    assert_eq!(back.look.y, 0, "facing along the level: {}", back.look);
    assert_eq!(state(&mut g, "start"), format!("{:?}", crate::lua::Value::Float(3.5f64.to_bits())));
}

/// A door the code answers ends the run: at the tick's end, through the
/// fade, the script told once; the outcome shows when it was told.
#[test]
fn a_win_goes_through_the_fade() {
    let main = "function on_tick() if world.tick == 5 then world:win() end end\nfunction on_level_end(why) world.state.log = (world.state.log or '') .. 'end:' .. why .. ';' end\nfunction on_world_end(result) world.state.log = world.state.log .. 'world:' .. result end\n";
    let dir = platformer_levels("rules-end", main, "", &[("door", [3, 2, 4])]);
    let mut g = game_in(&dir);
    g.fades = true;
    let mut ticks = 0;
    while !g.fading() && ticks < 120 {
        steps(&mut g, 1);
        ticks += 1;
    }
    assert!(g.fading(), "the fade out of the win");
    assert_eq!(g.game.outcome(), None, "not over before the script is told");
    while g.fading() {
        steps(&mut g, 1);
    }
    assert_eq!(g.game.outcome(), Some(crate::game::Outcome::Won));
    assert_eq!(state(&mut g, "log"), format!("{:?}", crate::lua::Value::Str(b"end:won;world:won".to_vec())));
}

/// A mistake in `on_level_end` stops the game before anything changed:
/// the ask waits, and once the file is mended the level is left.
#[test]
fn an_ask_waits_for_a_mended_level_end() {
    let main = "function on_tick() if world.tick == 2 then world:go_to('b') end end\nfunction on_level_end(why) error('broken') end\n";
    let dir = levels_folder("mended-end", main, &[("a", &[]), ("b", &[])]);
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert!(halted(&g).contains("broken"), "{}", halted(&g));
    std::fs::write(dir.join("main.lua"), "function on_level_end(why) end\nfunction on_level_start(how) world.state.at = level.name .. ':' .. how end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "at"), format!("{:?}", crate::lua::Value::Str(b"b:go_to".to_vec())));
}

/// The run's end is in the snapshot: a replay whose win falls in the
/// check's guard (a second played twice) runs the same; asks after the end
/// count no more (a standing `world:win()` ends the level once).
#[test]
fn the_end_is_in_the_snapshot_and_asks_after_it_count_no_more() {
    use crate::replay::{Recorder, check};
    let main = "function on_tick()\n  if world.tick >= 330 then world:win() end\nend\nfunction on_level_end(why) world.state.ends = (world.state.ends or 0) + 1 end\n";
    let dir = levels_folder("end-snapshot", main, &[("a", &[])]);
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..400 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(state(&mut g, "ends"), format!("{:?}", crate::lua::Value::Int(1)), "once, though asked every tick");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap());
}

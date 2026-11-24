//! Save games: slots, loads, versions, saves in replays.

use super::*;

/// A game of two levels with a player's figure and two save slots.
fn saves_folder(name: &str, main: &str) -> crate::scratch::Scratch {
    let dir = levels_folder(name, main, &[("a", &[]), ("b", &[])]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.player.kind = \"hero\"\n  c.things.hero = {}\n  c.things.box = {}\n  c.saves = { slots = 2 }\nend\n").unwrap();
    dir
}

const SAVE_AND_LOAD: &str = r#"
function on_world_start() world.state.coins = 0 end

function on_level_start(how)
  world.state.log = (world.state.log or "") .. level.name .. ":" .. how .. ";"
  if how ~= "load" then level.state.seen = 0 end
end

function on_tick()
  player:walk({ x = 1, y = 0 }, 20)
  level.state.seen = level.state.seen + 1
  world.state.coins = world.state.coins + 1
  if world.tick == 10 then world:save(1) end
  if world.tick == 20 then world:go_to("b") end
  if world.tick == 30 and not world.state.loaded then
    world.state.loaded = true
    world.state.slot = world.saves[1].level .. world.saves[1].seconds
    world:load(1)
  end
end
"#;

/// `world:save` keeps the run at the tick's end and `world:load` brings it
/// back like a level change: the states as saved, the level it was saved
/// in with its `level.state`, the player where it stood, the time.
#[test]
fn a_save_brings_the_run_back_as_it_was() {
    let mut g = game_in(&saves_folder("save-load", SAVE_AND_LOAD));
    steps(&mut g, 10);
    let saved_at = g.game.inspect()["player"]["x"].clone();
    steps(&mut g, 20);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["at"]["name"], "a", "back in the level it was saved in");
    assert_eq!(state(&mut g, "coins"), format!("{:?}", crate::lua::Value::Int(10)), "world.state as saved");
    assert_eq!(state(&mut g, "log"), format!("{:?}", crate::lua::Value::Str(b"a:start;a:load;".to_vec())), "world.state as saved, then the load told");
    assert_eq!(g.game.sim().tick, 10, "the time as saved");
    assert_eq!(g.game.inspect()["player"]["x"], saved_at, "the player where it stood");
    assert_eq!(g.game.inspect()["level"]["seen"], 10, "level.state as saved");
    assert!(!g.slots.on_disk_now(), "a game without a window keeps its saves for the run only");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "coins"), format!("{:?}", crate::lua::Value::Int(11)), "and it goes on from there");
}

/// Slots are checked where they are named: one the game lacks, an empty
/// one to load, a game without slots.
#[test]
fn save_slots_are_checked_where_they_are_named() {
    for (name, line, says) in [
        ("no-slot", "world:save(3)", "world:save: no slot 3: the game's are 1 to 2 (c.saves.slots)"),
        ("empty-slot", "world:load(2)", "world:load: slot 2 is empty (world.saves[2] tells before)"),
    ] {
        let dir = saves_folder(&format!("slots-{name}"), &format!("function on_tick() {line} end\n"));
        let mut g = game_in(&dir);
        steps(&mut g, 1);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
    let mut g = game("no-slots", "function on_tick() world:save(1) end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("the game has no save slots"), "{}", halted(&g));
}

/// A run that saves and loads plays back the same from its replay, and so
/// does one that began from a save (LOAD in the pause menu); neither
/// reads or writes the player's slots.
#[test]
fn replays_carry_their_saves() {
    use crate::replay::{Recorder, check};
    let dir = saves_folder("replay-saves", SAVE_AND_LOAD);
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..400 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    let replay = recorder.replay(&g).unwrap();
    // The loaded run reaches tick 30 again: it loads every 20 ticks.
    assert_eq!(replay.loads.len(), 19, "every save it loaded");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/saves.toml")).unwrap();
    let text = g.slots.read(1).unwrap().to_string();

    let mut from = Session::<LuaGame>::load_with(&dir, crate::saves::Slots::holding([(1, text.clone())].into())).unwrap();
    from.start_from(&text).unwrap();
    assert_eq!(from.game.sim().tick, 10);
    let mut recorder = Recorder::default();
    for _ in 0..400 {
        from.step(&Input::default());
        recorder.record(&from);
    }
    let replay = recorder.replay(&from).unwrap();
    assert!(replay.from.is_some(), "it began from the save");
    replay.save(&dir.join("replays/from.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "both play the same");
}

/// A save of an older version of the game is handed to `on_save_loaded`
/// before its level is entered; one from the shooter keeps the level's
/// changed voxels and the ready-made rules' run.
#[test]
fn a_save_outlives_a_new_version_and_keeps_what_changed() {
    let main = "function on_tick()\n  if world.tick == 2 then\n    level:set_voxel(40, 40, 0, 0)\n    player.state.score = 120\n    world:save(1)\n  end\nend\nfunction on_save_loaded(version) world.state.from = version end\nfunction on_level_start(how) world.state.hole = level:voxel(40, 40, 0) end\n";
    let dir = kit_folder("save-version", "  c.saves = { slots = 1 }", main);
    let mut g = game_in(&dir);
    let before = g.game.sim().world.get(glam::IVec3::new(40, 40, 0));
    assert_ne!(before, 0, "the ground there to begin with");
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    let text = g.slots.read(1).unwrap().to_string();
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap().replace("c.saves = { slots = 1 }", "c.saves = { slots = 1, version = 2 }");
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    g.start_from(&text).unwrap();
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "from"), format!("{:?}", crate::lua::Value::Int(1)), "told the save's version");
    // A save of the same version is told too.
    std::fs::write(dir.join("conf.lua"), std::fs::read_to_string(dir.join("conf.lua")).unwrap().replace("version = 2", "version = 1")).unwrap();
    let mut same = game_in(&dir);
    same.start_from(&text).unwrap();
    assert_eq!(state(&mut same, "from"), format!("{:?}", crate::lua::Value::Int(1)), "every load is told");
    assert_eq!(state(&mut g, "hole"), format!("{:?}", crate::lua::Value::Int(0)), "the hole is back");
    assert_eq!(g.game.score(), 120, "the score kept");
}

/// What the review of stage 5 found: a load asked in `on_level_start`
/// works (the first one too: going on from a save as the game starts); a
/// save in the first `on_level_start` is in the replay's slots, so `check`
/// plays it the same; a load brings the random numbers back as they were.
#[test]
fn loads_and_saves_work_from_on_level_start_too() {
    use crate::replay::{Recorder, check};
    let main = r#"
function on_level_start(how)
  world.state.how = (world.state.how or "") .. how .. ";"
  if how == "start" then world:save(1) end
  if how == "go_to" and world.saves[1] then world:load(1) end
end

function on_tick()
  world.state.saw = world.saves[1] and world.saves[1].level or "none"
  world.state.roll = math.random(1000000)
  if world.tick == 8 then world:go_to("b") end
end
"#;
    let dir = saves_folder("load-at-start", main);
    let mut g = game_in(&dir);
    for _ in 0..7 {
        steps(&mut g, 1);
    }
    let roll = state(&mut g, "roll");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["at"]["name"], "a", "on_level_start of b loaded the save");
    assert_eq!(state(&mut g, "how"), format!("{:?}", crate::lua::Value::Str(b"start;load;".to_vec())));
    assert_eq!(g.game.sim().tick, 0, "the time of the save: before the first tick");

    // The same random numbers again from the save on.
    for _ in 0..7 {
        steps(&mut g, 1);
    }
    assert_eq!(state(&mut g, "roll"), roll, "math.random goes on as it did");

    // The autosave is in the replay's slots as the run began.
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..300 {
        g.step(&Input::default());
        recorder.record(&g);
    }
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/autosave.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap());

    // A game that starts with a save in slot 1 goes on from it at once.
    let text = g.slots.read(1).unwrap().to_string();
    std::fs::write(dir.join("main.lua"), "function on_level_start(how)\n  world.state.how = (world.state.how or '') .. how .. ';'\n  if how == 'start' and world.saves[1] then world:load(1) end\nend\n").unwrap();
    let g = Session::<LuaGame>::load_with(&dir, crate::saves::Slots::holding([(1, text)].into())).unwrap();
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["state"]["how"], "start;load;", "the save's world.state, then the load told");
}

/// BunnyFarm saved with a gap in a pen's fence: the save brings the
/// level's voxels back with the gap already open, and the game, breaking
/// the gap in anew as the farm is entered, still knows what the gap took,
/// so mending puts the fence back whole (it knew nothing, and the hole
/// stayed for good once mended).
#[test]
fn bunnyfarms_fence_saved_with_a_gap_still_mends_whole() {
    use crate::replay::{Replay, Script, parse_script};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/bunnyfarm");
    // An evening with a gap in the chicken pen's fence, the burrow beside.
    let replay = Replay::load(&dir.join("replays/tests/wood_mends_the_fence.toml")).unwrap();
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    g.start_from(replay.from.as_deref().unwrap()).unwrap();
    let taken = g.game.inspect()["level"]["fences"]["chicken"]["taken"].clone();
    assert!(taken.as_array().is_some_and(|t| t.len() > 20), "the gap broken in from the level's file: {taken}");
    // To bed: the farm is saved as the bunny sleeps.
    let mut script = Script::new(parse_script("none:3;act:1;none:5;down:1;none:3;act:1;none:5", g.actions()).unwrap());
    for _ in 0..19 {
        g.step(&script.next_input());
    }
    assert_eq!(halted(&g), "");
    let text = g.slots.read(1).expect("saved as it slept").to_string();
    let mut loaded = Session::<LuaGame>::load(&dir).unwrap();
    loaded.start_from(&text).unwrap();
    assert_eq!(halted(&loaded), "");
    assert_eq!(loaded.game.inspect()["level"]["fences"]["chicken"]["taken"], taken, "what the gap took, known after loading");
}

/// A script that does what `world.state.cmd` says, once: `save1`, `save2`,
/// `label1`, `del1`, `del2` (slots 1 and 2).
const DOES: &str = r#"
function on_tick()
  local c = world.state.cmd
  world.state.cmd = nil
  if c == "save1" then world:save(1) elseif c == "save2" then world:save(2)
  elseif c == "label1" then world:save(1, { label = "Chapter 3 – Harbor" })
  elseif c == "del1" then world:delete_save(1) elseif c == "del2" then world:delete_save(2) end
end
"#;

/// Does `cmd` in the next tick.
fn does(g: &mut Session<LuaGame>, cmd: &str) {
    said(g, &format!("world.state.cmd = \"{cmd}\""));
    steps(g, 1);
}

/// A label is kept with the save: read back as `world.saves[n].label`
/// (nil without one), by a session reading the slots afresh too, through
/// a load, and a save from before labels still reads and loads.
#[test]
fn a_label_is_kept_with_the_save() {
    let dir = saves_folder("save-label", DOES);
    let mut g = game_in(&dir);
    does(&mut g, "save2");
    assert_eq!(said(&mut g, "world.saves[2].label"), "nil", "none given, none shown");
    does(&mut g, "label1");
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "world.saves[1].label"), "\"Chapter 3 – Harbor\"");
    assert_eq!(g.slots.info(1).and_then(|i| i.label).as_deref(), Some("Chapter 3 – Harbor"), "the engine's pages read it");
    let (text, plain) = (g.slots.read(1).unwrap().to_string(), g.slots.read(2).unwrap().to_string());
    assert!(text.contains("label = "), "kept in the save file");
    assert!(!plain.contains("label"), "a save without one writes none");
    let slots = |one: &str| crate::saves::Slots::holding([(1, one.to_string())].into());
    let mut fresh = Session::<LuaGame>::load_with(&dir, slots(&text)).unwrap();
    assert_eq!(said(&mut fresh, "world.saves[1].label"), "\"Chapter 3 – Harbor\"", "a fresh session reads the slots");
    fresh.start_from(&text).unwrap();
    assert_eq!(halted(&fresh), "");
    assert_eq!(said(&mut fresh, "world.saves[1].label"), "\"Chapter 3 – Harbor\"", "and goes on from it");
    // An older save has no such line.
    let old: String = text.lines().filter(|l| !l.starts_with("label")).collect::<Vec<_>>().join("\n") + "\n";
    let mut older = Session::<LuaGame>::load_with(&dir, slots(&old)).unwrap();
    assert_eq!(said(&mut older, "world.saves[1].label"), "nil");
    assert_eq!(said(&mut older, "world.saves[1].level"), "\"a\"");
    older.start_from(&old).unwrap();
    assert_eq!(halted(&older), "");
}

/// A label is one line of 80 characters at most, a string; the options take
/// no other key.
#[test]
fn a_label_that_is_not_one_short_line_is_a_mistake() {
    let long = "x".repeat(81);
    for (name, options, says) in [
        ("long", format!("{{ label = \"{long}\" }}"), "world:save: label is one line of 80 characters at most, this is 81"),
        ("number", "{ label = 5 }".to_string(), "world:save: label is a string, not 5"),
        ("lines", "{ label = \"a\\nb\" }".to_string(), "world:save: label is one line"),
        ("key", "{ name = \"x\" }".to_string(), "world:save: no 'name' (it takes label)"),
        ("text", "\"x\"".to_string(), "world:save: the options are a table"),
    ] {
        let dir = saves_folder(&format!("label-{name}"), &format!("function on_tick() world:save(1, {options}) end\n"));
        let mut g = game_in(&dir);
        steps(&mut g, 1);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
    let ok = "é".repeat(80);
    let dir = saves_folder("label-80", &format!("function on_tick() world:save(1, {{ label = \"{ok}\" }}) end\n"));
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    assert_eq!(halted(&g), "", "80 characters, however many bytes");
}

/// `world:delete_save` empties the slot at the tick's end: the script still
/// sees it in that tick and not in the next; an empty slot or one out of
/// range is a mistake; a save and a delete of a tick leave the last ask.
#[test]
fn a_save_is_deleted_at_the_ticks_end() {
    let main = "function on_tick()\n  local seen = world.saves[1] and 'x' or '-'\n  if world.tick == 2 then world:save(1) end\n  \
        if world.tick == 5 then world:delete_save(1); seen = seen .. (world.saves[1] and 'x' or '-') .. '!' end\n  \
        world.state.log = (world.state.log or '') .. seen\nend\n";
    let mut g = game_in(&saves_folder("delete-timing", main));
    steps(&mut g, 8);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "world.state.log"), "\"--xxxx!---\"", "still there in the tick that deleted it, gone in the next");
    assert!(g.slots.info(1).is_none(), "the slot is empty");
    for (name, line, says) in [
        ("empty", "world:delete_save(2)", "world:delete_save: slot 2 is empty (world.saves[2] tells before)"),
        ("range", "world:delete_save(3)", "world:delete_save: no slot 3: the game's are 1 to 2 (c.saves.slots)"),
    ] {
        let mut g = game_in(&saves_folder(&format!("delete-{name}"), &format!("function on_tick() {line} end\n")));
        steps(&mut g, 1);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
    let mut g = game("delete-no-slots", "function on_tick() world:delete_save(1) end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("the game has no save slots"), "{}", halted(&g));
    // Save, then delete in one tick: the last ask counts.
    let mut g = game_in(&saves_folder("delete-last", "function on_tick()\n  if world.tick == 1 then world:save(1) end\n  if world.tick == 3 then world:save(1); world:delete_save(1) end\nend\n"));
    steps(&mut g, 2);
    assert!(g.slots.info(1).is_some());
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert!(g.slots.info(1).is_none());
}

/// `world.saves.newest` is the slot saved last, nil while all are empty,
/// and moves after a delete; `pairs`, `ipairs` and `#` go over the numbered
/// slots as ever.
#[test]
fn newest_is_the_slot_saved_last() {
    let mut g = game_in(&saves_folder("newest", DOES));
    assert_eq!(said(&mut g, "world.saves.newest"), "nil");
    does(&mut g, "save2");
    assert_eq!(said(&mut g, "world.saves.newest"), "2");
    does(&mut g, "save1");
    assert_eq!(said(&mut g, "world.saves.newest"), "1", "the last made, not the highest");
    does(&mut g, "save2");
    assert_eq!(said(&mut g, "world.saves.newest"), "2");
    let count = "(function() local c, k = 0, 0 for _ in pairs(world.saves) do c = c + 1 end for _ in ipairs(world.saves) do k = k + 1 end return c .. ':' .. k .. ':' .. #world.saves end)()";
    assert_eq!(said(&mut g, count), "\"2:2:2\"", "newest is no slot");
    does(&mut g, "del2");
    assert_eq!(said(&mut g, "world.saves.newest"), "1", "it moves to the next");
    does(&mut g, "del1");
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "world.saves.newest"), "nil");
    assert_eq!(said(&mut g, count), "\"0:0:0\"");
    // A save from the pause menu is the newest, whatever the script saved.
    let mut g = game_in(&saves_folder("newest-menu", DOES));
    g.set_clock(100);
    does(&mut g, "save1");
    g.set_clock(200);
    g.save_to(2).unwrap();
    assert_eq!(said(&mut g, "world.saves.newest"), "2");
    does(&mut g, "save1");
    assert_eq!(said(&mut g, "world.saves.newest"), "1");
}

/// A run that saves with a label, deletes and reads `newest` plays back
/// the same from its replay, whatever the clock said.
#[test]
fn replays_carry_labels_deletes_and_newest() {
    use crate::replay::{Recorder, check};
    let main = r#"
function on_tick()
  if world.tick == 4 then world:save(1, { label = "one" }) end
  if world.tick == 6 then world:save(2) end
  if world.tick == 8 then world:delete_save(2) end
  if world.tick == 10 then world:save(2, { label = "two" }) end
  if world.tick == 12 then world:save(1) end
  world.state.seen = tostring(world.saves.newest) .. tostring(world.saves[1] and world.saves[1].label) .. tostring(world.saves[2] and world.saves[2].label)
end
"#;
    let dir = saves_folder("replay-labels", main);
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for i in 0..40 {
        g.set_clock(1_000 - i);
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "world.state.seen"), "\"1niltwo\"");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/labels.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "plays the same");
}

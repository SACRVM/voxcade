//! Levels that remember (`lua::memory`): a location's levels as left while
//! the player stays among them, their one `level.state`, `level.ticks_away`,
//! `persistent` levels and kinds, restarts, saves, scenarios, snapshots.

use super::*;

/// A room's name, its location and whether it is persistent.
type Room<'a> = (&'a str, Option<&'a str>, bool);

/// A game of rooms of blocks (a floor of rock 16 voxels long), each with
/// a crate at x 4, a key at x 8 and a chest (a persistent kind) at x 12;
/// the player's figure a thing of `hero`; `conf` lines more.
fn rooms(name: &str, main: &str, conf: &str, rooms: &[Room]) -> crate::scratch::Scratch {
    let names: Vec<LevelPlaces> = rooms.iter().map(|(n, ..)| (*n, &[][..])).collect();
    let dir = levels_folder(&format!("remember-{name}"), main, &names);
    std::fs::write(
        dir.join("conf.lua"),
        format!("function conf(c)\n  c.world.blocks = {{ file = \"level.toml\" }}\n  c.player.spawn = {{ 2, 2 }}\n  c.player.kind = \"hero\"\n  c.things.hero = {{}}\n  c.things.crate = {{}}\n  c.things.key = {{}}\n  c.things.chest = {{ persistent = true }}\n  c.saves = {{ slots = 2 }}\n{conf}end\n"),
    )
    .unwrap();
    for (room, location, persistent) in rooms {
        let path = dir.join("levels").join(format!("{room}.toml"));
        let mut level = crate::grid::GridLevel::read(&path).unwrap();
        (level.location, level.persistent) = (location.map(str::to_string), *persistent);
        for (kind, x) in [("crate", 4), ("key", 8), ("chest", 12)] {
            level.things.push(crate::grid::Thing::new(kind, [x, 2, 4]));
        }
        level.save(&path).unwrap();
    }
    dir
}

/// The station: its forecourt and its hall, and a street of no location.
const STATION: &[Room] = &[("forecourt", Some("station"), false), ("hall", Some("station"), false), ("street", None, false)];

/// What `on_level_start` logs: how, then the room as it is entered.
const SEEN: &str = r#"
local function seen()
  local crate, key, chest = level:thing("crate_1"), level:thing("key_1"), level:thing("chest_1")
  return level.name .. ":" .. (level.location or "-") .. ":away" .. level.ticks_away .. ":rooms" .. level.state.rooms
    .. ":crate" .. (crate and math.floor(crate.x) or "-") .. ":" .. (key and "key" or "nokey") .. ":" .. (chest.state.open and "open" or "shut")
end

function on_level_start(how)
  level.state.rooms = (level.state.rooms or 0) + 1
  world.state.log = (world.state.log or "") .. how .. ">" .. seen() .. ";"
end
"#;

fn log(g: &mut Session<LuaGame>) -> String {
    g.game.inspect()["state"]["log"].as_str().unwrap_or_default().replace(';', ";\n")
}

/// While the player stays among a location's levels each is as it was
/// left (a crate moved, a key taken, a chest opened) and they share one
/// `level.state`; `level.ticks_away` counts the ticks it was left; leaving the
/// location forgets it but for what is persistent (the chest's kind).
#[test]
fn a_locations_levels_are_as_left_while_the_player_stays_among_them() {
    let main = format!(
        "{SEEN}
function on_tick()
  local t = world.tick
  if t == 2 then
    level:thing('crate_1'):place(10, 2, 4)
    level:thing('key_1'):remove()
    level:thing('chest_1').state.open = true
    world:go_to('hall')
  end
  if t == 4 then world:go_to('forecourt') end
  if t == 6 then world:go_to('street') end
  if t == 8 then world:go_to('forecourt') end
end
"
    );
    let mut g = game_in(&rooms("location", &main, "", STATION));
    steps(&mut g, 5);
    assert_eq!(halted(&g), "");
    assert!(g.game.inspect()["records"]["hall"]["things"]["crate_1"]["at"].is_array(), "the hall kept as left: {}", g.game.inspect()["records"]);
    steps(&mut g, 5);
    assert_eq!(halted(&g), "");
    assert_eq!(
        log(&mut g),
        "start>forecourt:station:away0:rooms1:crate4:key:shut;
go_to>hall:station:away0:rooms2:crate4:key:shut;
go_to>forecourt:station:away2:rooms3:crate10:nokey:open;
go_to>street:-:away0:rooms1:crate4:key:shut;
go_to>forecourt:station:away2:rooms1:crate4:key:open;
"
    );
    // Only the chests (a persistent kind) outlived the station's visit.
    let records = g.game.inspect()["records"].clone();
    assert_eq!(records["hall"]["things"].as_object().unwrap().keys().collect::<Vec<_>>(), ["chest_1"], "{records}");
}

/// `world:restart()` gives the level as it was entered this visit: its
/// things as they were then and the location's `level.state` as it came
/// in; `{ fresh = true }` from its file. `world:go_to` the same level of a
/// location keeps it as left.
#[test]
fn a_restart_gives_the_level_as_it_was_entered() {
    let main = format!(
        "{SEEN}
function on_tick()
  local t = world.tick
  if t == 2 then level:thing('crate_1'):place(10, 2, 4) world:go_to('hall') end
  if t == 4 then level:thing('crate_1'):place(14, 2, 4) level.state.rooms = 50 world:go_to('forecourt') end
  if t == 6 then level:thing('crate_1'):place(6, 2, 4) world:restart() end
  if t == 8 then world:restart({{ fresh = true }}) end
  if t == 10 then level:thing('key_1'):remove() world:go_to('forecourt') end
end
"
    );
    let mut g = game_in(&rooms("restart", &main, "", STATION));
    steps(&mut g, 11);
    assert_eq!(halted(&g), "");
    assert_eq!(
        log(&mut g),
        "start>forecourt:station:away0:rooms1:crate4:key:shut;
go_to>hall:station:away0:rooms2:crate4:key:shut;
go_to>forecourt:station:away2:rooms51:crate10:key:shut;
restart>forecourt:station:away4:rooms51:crate10:key:shut;
restart>forecourt:station:away0:rooms51:crate4:key:shut;
go_to>forecourt:station:away0:rooms52:crate4:nokey:shut;
"
    );
}

/// A save made inside a location keeps the level played as it stands, the
/// others as they were left and its `level.state` as entered: loaded, the
/// run is as it was; a save of an older Voxcade has none of it and loads
/// as before (every level as its file has it).
#[test]
fn a_save_inside_a_location_keeps_its_levels() {
    let main = format!(
        "{SEEN}
function on_tick()
  local t = world.tick
  if t == 2 then level:thing('crate_1'):place(10, 2, 4) world:go_to('hall') end
  if t == 4 then level:thing('key_1'):remove() level.state.mark = 'x' world:save(1) end
  if t == 5 then world:go_to('forecourt') end
  if t == 6 then level:thing('crate_1'):place(14, 2, 4) end
  if t == 7 and not world.state.loaded then world:load(1) end
end
function on_save_loaded() world.state.loaded = true end
"
    );
    let dir = rooms("save", &main, "", STATION);
    let mut g = game_in(&dir);
    steps(&mut g, 7);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["at"]["name"], "hall");
    assert_eq!(g.game.inspect()["level"]["mark"], "x", "level.state as saved");
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    let log = log(&mut g);
    assert!(log.ends_with("load>hall:station:away0:rooms3:crate4:nokey:shut;\ngo_to>forecourt:station:away3:rooms4:crate10:key:shut;\n"), "{log}");

    // A save without what the levels kept (an older Voxcade's).
    let text = g.slots.read(1).unwrap().to_string();
    let mut old = crate::lua::save::SaveData::read(&text).unwrap();
    assert!(!old.here.is_empty() && old.records.contains_key("forecourt"), "{text}");
    (old.here, old.entered, old.records) = (String::new(), String::new(), Default::default());
    let old = old.write().unwrap();
    let mut g = game_in(&dir);
    g.start_from(&old).unwrap();
    assert_eq!(halted(&g), "");
    assert!(g.game.inspect()["records"].is_null());
    let log = self::log(&mut g);
    assert!(log.ends_with("load>hall:station:away0:rooms3:crate4:key:shut;\n"), "{log}");
}

/// A persistent level keeps everything it set down and its voxels for the
/// whole run, outside a location too; its `level.state` stays one visit.
#[test]
fn a_persistent_level_stays_as_left_for_the_run() {
    let main = format!(
        "{SEEN}
function on_tick()
  local t = world.tick
  if t == 2 then world:go_to('street') end
  if t == 4 then level:thing('key_1'):remove() level:set_voxel(1, 1, 1, nil) world:go_to('forecourt') end
  if t == 6 then world:go_to('hall') end
  if t == 8 then world:go_to('street') end
  if t == 9 then world.state.voxel = level:voxel(1, 1, 1) end
end
"
    );
    let rooms_of = &[("forecourt", Some("station"), false), ("hall", None, false), ("street", None, true)];
    let mut g = game_in(&rooms("persistent", &main, "", rooms_of));
    steps(&mut g, 10);
    assert_eq!(halted(&g), "");
    let log = log(&mut g);
    assert!(log.ends_with("go_to>street:-:away4:rooms1:crate4:nokey:shut;\n"), "{log}");
    assert_eq!(g.game.inspect()["state"]["voxel"], 0, "its voxels as left");
}

/// The records are part of the game's state: a game kept at a tick and
/// brought back plays on the same across changes among a location's
/// levels; a game whose levels say nothing of remembering keeps none, so
/// its hash is as before.
#[test]
fn records_come_back_with_a_snapshot_and_a_game_without_them_keeps_none() {
    let main = format!(
        "{SEEN}
function on_tick()
  local t = world.tick
  if t % 3 == 0 then level:thing('crate_1'):place(4 + t % 8, 2, 4) end
  if t % 4 == 0 then world:go_to(level.name == 'forecourt' and 'hall' or 'forecourt') end
end
"
    );
    let mut g = game_in(&rooms("rollback", &main, "", STATION));
    steps(&mut g, 5);
    let kept_at = g.game.state_hash();
    let snapshot = g.snapshot().unwrap();
    let mut first = Vec::new();
    for _ in 0..20 {
        steps(&mut g, 1);
        first.push(g.game.state_hash());
    }
    assert!(!g.game.stage.records.is_empty());
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), kept_at);
    for (n, hash) in first.iter().enumerate() {
        steps(&mut g, 1);
        assert_eq!(g.game.state_hash(), *hash, "tick {} after the snapshot played differently", n + 1);
    }
    // The same rooms saying nothing of remembering, the chest's kind too.
    let plain = &[("forecourt", None, false), ("hall", None, false), ("street", None, false)];
    let mut g = game_in(&rooms("rollback-plain", &main, "  c.things.chest = {}\n", plain));
    steps(&mut g, 25);
    assert_eq!(halted(&g), "");
    assert!(g.game.stage.records.is_empty() && g.game.stage.entered.is_none() && g.game.stage.away == 0, "nothing enters the hash");
}

/// A scenario sets how other levels were left (`as_left`): laid over each
/// as it is entered; its mistakes name the level or the thing.
#[test]
fn a_scenario_sets_how_other_levels_were_left() {
    let main = format!("{SEEN}\nfunction on_tick() if world.tick == 2 then world:go_to('hall') end end\n");
    let dir = rooms("scenario", &main, "", STATION);
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    std::fs::write(dir.join("scenarios/late.lua"), "return {\n  level = 'forecourt',\n  as_left = { hall = { key_1 = { removed = true }, crate_1 = { at = { 12, 2, 4 } }, chest_1 = { state = { open = true } } } },\n}\n").unwrap();
    let mut g = game_in(&dir);
    g.start_scenario("late").unwrap();
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    let log = log(&mut g);
    assert!(log.ends_with("go_to>hall:station:away2:rooms2:crate12:nokey:open;\n"), "{log}");
    for (name, as_left, says) in [
        ("no_level", "{ hal = {} }", "as_left.hal: no such level (did you mean hall?)"),
        ("no_thing", "{ hall = { crate_7 = { removed = true } } }", "as_left.hall.crate_7: level hall sets down no thing called that"),
        ("own_level", "{ forecourt = {} }", "the run begins in forecourt"),
        ("no_word", "{ hall = { crate_1 = { gone = true } } }", "as_left.hall.crate_1: no gone"),
    ] {
        std::fs::write(dir.join(format!("scenarios/{name}.lua")), format!("return {{ level = 'forecourt', as_left = {as_left} }}\n")).unwrap();
        let mut g = game_in(&dir);
        let e = format!("{:#}", g.start_scenario(name).unwrap_err());
        assert!(e.contains(says), "{name}: {e}");
    }
}

/// A level file changed under a record: a thing new in the file is as the
/// file has it, a name the file no longer has is dropped with a note, a
/// thing taken out stays out by its name.
#[test]
fn a_record_over_a_changed_level_file() {
    let main = format!(
        "{SEEN}
function on_tick()
  local t = world.tick
  if t == 2 then world:go_to('hall') end
  if t == 4 then level:thing('crate_1'):remove() world:go_to('forecourt') end
  if t == 6 then world:save(1) end
  if world.state.loaded and world.tick == 8 then
    world.state.crate2 = level:thing('crate_2') and 'there' or 'gone'
  end
  if world.state.loaded and world.tick == 7 then world:go_to('hall') end
end
function on_save_loaded() world.state.loaded = true end
"
    );
    let dir = rooms("changed", &main, "", STATION);
    let mut g = game_in(&dir);
    steps(&mut g, 7);
    assert_eq!(halted(&g), "");
    let text = g.slots.read(1).unwrap().to_string();
    // The hall's key taken out of its file, a second crate put in.
    let path = dir.join("levels/hall.toml");
    let mut hall = crate::grid::GridLevel::read(&path).unwrap();
    hall.things.retain(|t| t.kind != "key");
    hall.things.push(crate::grid::Thing::new("crate", [6, 2, 4]));
    hall.save(&path).unwrap();
    let mut g = game_in(&dir);
    g.start_from(&text).unwrap();
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["state"]["crate2"], "there", "a thing new in the file is there");
    let log = log(&mut g);
    assert!(log.ends_with("go_to>hall:station:away3:rooms5:crate-:nokey:shut;\n"), "the crate stays out: {log}");
    let notes = g.printed();
    assert!(notes.iter().any(|n| n.contains("level hall kept a thing called 'key_1' that its file no longer sets down: dropped")), "{notes:?}");
}

/// What a level or a kind says of remembering is checked as the game
/// loads, naming the file and line or the kind; a level's unknown key is
/// still refused.
#[test]
fn remembering_words_are_checked_as_the_game_loads() {
    let dir = rooms("words", "", "", &[("forecourt", Some("station"), false)]);
    let path = dir.join("levels/forecourt.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("location = \"station\""), "{text}");
    for (from, to, says) in [
        ("location = \"station\"", "location = 3", "line 5: location is a name, in quotes"),
        ("location = \"station\"", "location = \"station\"\npersistent = \"yes\"", "line 6: persistent must be true or false"),
        ("location = \"station\"", "locatoin = \"station\"", "locatoin"),
    ] {
        std::fs::write(&path, text.replace(from, to)).unwrap();
        let e = start_error(&dir);
        assert!(e.contains("levels/forecourt.toml") && e.contains(says), "{to}: {e}");
    }
    std::fs::write(&path, &text).unwrap();
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    for (from, to, says) in [
        ("c.things.chest = { persistent = true }", "c.things.chest = { persistent = 1 }", "c.things.chest.persistent: true or false"),
        ("c.things.chest = { persistent = true }", "c.things.chest = { persistant = true }", "did you mean `persistent`?"),
    ] {
        std::fs::write(dir.join("conf.lua"), conf.replace(from, to)).unwrap();
        let e = start_error(&dir);
        assert!(e.contains(says), "{to}: {e}");
    }
    // The hero of the ready-made rules becomes the player: no name to keep.
    let dir = platformer_levels("remember-hero", "", "  c.things.hero.persistent = true\n", &[]);
    assert!(start_error(&dir).contains("c.things.hero.persistent: the first hero a level sets down becomes the player"), "{}", start_error(&dir));
    // The game's own title belongs to no location.
    let dir = rooms("title", "", "  c.title_screen = { level = \"forecourt\" }\n", &[("forecourt", Some("station"), false), ("hall", Some("station"), false)]);
    assert!(start_error(&dir).contains("this level is the game's own title"), "{}", start_error(&dir));
}

/// The level editor's text keeps the two words: written, read back, a
/// change rewriting only their lines.
#[test]
fn a_levels_remembering_words_are_written_and_rewritten_in_place() {
    let dir = rooms("text", "", "", &[("forecourt", None, false)]);
    let path = dir.join("levels/forecourt.toml");
    let src = std::fs::read_to_string(&path).unwrap();
    let was = crate::grid::GridLevel::from_text(&src).unwrap();
    let mut now = was.clone();
    (now.location, now.persistent) = (Some("station".into()), true);
    let (_, legend) = crate::grid::text::read(&src).unwrap();
    let out = crate::grid::text::rewrite(&src, &was, &now, &legend);
    assert_eq!(crate::grid::GridLevel::from_text(&out).unwrap(), now);
    let added: Vec<&str> = out.lines().filter(|l| !src.lines().any(|s| s == *l)).collect();
    assert_eq!(added, ["location = \"station\"", "persistent = true"], "{out}");
    let back = crate::grid::text::rewrite(&out, &now, &was, &legend);
    assert_eq!(back, src, "taken out again, the file as it was");
}

/// A jump & run's level of a location: a walker is where it was left, a
/// pickup taken stays taken while the player stays in the location.
#[test]
fn a_platformer_level_of_a_location_is_as_left() {
    let main = r#"
function on_level_start(how)
  local w, g = level:thing("walker_1"), level:thing("gem_1")
  world.state.log = (world.state.log or "") .. how .. ":" .. level.name .. ":" .. (w and math.floor(w.x * 10) or "-") .. ":" .. (g and "gem" or "nogem") .. ";"
end
function on_tick()
  if world.tick == 30 then
    world.state.walker = math.floor(level:thing("walker_1").x * 10)
    level:thing("gem_1"):remove()
    world:go_to("b")
  end
  if world.tick == 32 then world:go_to("a") end
end
"#;
    let conf = "  c.things.walker = { behavior = \"walker\", height = 6 }\n  c.things.gem = { behavior = \"pickup\", height = 4 }\n";
    let dir = platformer_levels("remember-kit", main, conf, &[("walker", [8, 2, 4]), ("gem", [12, 2, 4])]);
    let a = std::fs::read_to_string(dir.join("levels/a.toml")).unwrap();
    for name in ["a", "b"] {
        let mut level = crate::grid::GridLevel::from_text(&a).unwrap();
        level.location = Some("castle".into());
        level.save(&dir.join(format!("levels/{name}.toml"))).unwrap();
    }
    let mut g = game_in(&dir);
    steps(&mut g, 33);
    assert_eq!(halted(&g), "");
    let walker = g.game.inspect()["state"]["walker"].as_i64().unwrap();
    let log = g.game.inspect()["state"]["log"].as_str().unwrap().to_string();
    assert!(log.ends_with(&format!("go_to:a:{walker}:nogem;")), "{log}");
    assert!(!log.starts_with(&format!("start:a:{walker}:")), "the walker walked: {log}");
}

//! A thing's own props: set on a level's `[[thing]]` or given by
//! `level:spawn`, checked by its kind alike, read in `thing.props`.

use super::*;

/// The door's kind declares three props; the level's door sets two.
const DOOR: &str = "  c.things.door.props = { to = \"a\", at = \"start\", locked = false }\n";

/// The events game's level with its door given `props` (a TOML inline
/// table), the rest as it was.
fn door_props(dir: &Path, props: &str) {
    let path = dir.join("levels/a.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let text = text.replace("kind = \"door\"\nat = [12, 2, 4]\n", &format!("kind = \"door\"\nat = [12, 2, 4]\nprops = {props}\n"));
    assert!(text.contains("props ="), "the door's lines as the writer writes them");
    std::fs::write(path, text).unwrap();
}

const READS: &str = r#"
function on_level_start(how)
  world.state.how = how
  if how == "start" then level:spawn("door", 6, 2, { name = "back", props = { to = "b" } }) end
end

function on_tick()
  if world.tick == 3 then world:restart() end
end
"#;

/// A level's thing has its props in `thing.props` over its kind's, as a
/// spawned one has: through a snapshot restored, a restart (its file's
/// again; the spawned one gone with its own), a level file changed while
/// playing; and the fingerprint moves with a prop.
#[test]
fn a_levels_thing_has_its_props_over_its_kinds() {
    let dir = events_folder("props-read", READS, DOOR);
    door_props(&dir, "{ to = \"crypt\", locked = true }");
    let mut g = game_in(&dir);
    let props = "local d = level:thing('door_1').props return d.to, d.at, d.locked";
    assert_eq!(said(&mut g, props), "\"crypt\"\t\"start\"\ttrue", "its own over its kind's");
    assert_eq!(said(&mut g, "local b = level:thing('back').props return b.to, b.at"), "\"b\"\t\"start\"");
    steps(&mut g, 1);
    let snapshot = g.snapshot().unwrap();
    steps(&mut g, 4);
    assert_eq!(said(&mut g, "return world.state.how, level:thing('back')"), "\"restart\"\tnil");
    assert_eq!(said(&mut g, props), "\"crypt\"\t\"start\"\ttrue", "the file's again after a restart");
    g.restore(&snapshot);
    assert_eq!(said(&mut g, "return level:thing('back').props.to"), "\"b\"", "restored");
    assert_eq!(said(&mut g, props), "\"crypt\"\t\"start\"\ttrue");
    assert_eq!(halted(&g), "");
    // The file changed while playing: the level entered anew with it.
    let before = g.fingerprint().unwrap();
    let path = dir.join("levels/a.toml");
    std::fs::write(&path, std::fs::read_to_string(&path).unwrap().replace("to = \"crypt\"", "to = \"tower\"")).unwrap();
    g.reload(&path).unwrap();
    assert_eq!(said(&mut g, props), "\"tower\"\t\"start\"\ttrue");
    assert_ne!(g.fingerprint().unwrap(), before, "a prop is part of the game's data");
    // And taken out: the kind's.
    std::fs::write(&path, std::fs::read_to_string(&path).unwrap().replace("props = { to = \"tower\", locked = true }\n", "")).unwrap();
    g.reload(&path).unwrap();
    assert_eq!(said(&mut g, props), "\"a\"\t\"start\"\tfalse");
}

/// A level's props are checked as the game loads, as every link by name
/// is: a key the kind does not declare, a value of another type, a number
/// past the limit, each a mistake naming the file, the line and the key;
/// spawn's the same words. `props = {}` is none. `conf.lua` changed while
/// playing so that a level's prop is no longer declared is refused, the
/// old config kept.
#[test]
fn a_levels_props_are_checked_as_the_game_loads() {
    let main = "function on_level_start(how) end\n";
    for (props, says) in [
        ("{ too = \"crypt\" }", "line 20: props.too: a door has no such prop (c.things.door.props: at, locked, to): did you mean `to`?"),
        ("{ locked = \"yes\" }", "line 20: props.locked is a boolean by its kind (c.things.door.props.locked = false), not a string"),
        ("{ at = 7 }", "line 20: props.at is a string by its kind (c.things.door.props.at = \"start\"), not a number"),
        ("{ to = 2000000 }", "line 20: props.to = 2000000: past the limit, 1000000 at most either way"),
    ] {
        let dir = events_folder("props-wrong", main, DOOR);
        door_props(&dir, props);
        let e = start_error(&dir);
        assert!(e.contains("levels/a.toml") && e.contains(says), "{props}: {e}");
    }
    let dir = events_folder("props-empty", main, DOOR);
    door_props(&dir, "{}");
    assert_eq!(start_error(&dir), "", "props = {{}} is none");
    // Spawn's in the same words.
    let main = "function on_level_start(how) level:spawn(\"door\", 6, 2, { props = { too = 1 } }) end\n";
    assert!(start_error(&events_folder("props-spawned", main, DOOR)).contains("level:spawn: props.too: a door has no such prop (c.things.door.props: at, locked, to): did you mean `to`?"));
    // conf.lua saved without the prop the level sets.
    let dir = events_folder("props-conf", "function on_level_start(how) end\n", DOOR);
    door_props(&dir, "{ locked = true }");
    let mut g = game_in(&dir);
    let conf = dir.join("conf.lua");
    std::fs::write(&conf, std::fs::read_to_string(&conf).unwrap().replace(", locked = false", "")).unwrap();
    let e = format!("{:#}", g.reload(&conf).unwrap_err());
    assert!(e.contains("level a: line 20: props.locked: a door has no such prop (c.things.door.props: at, to)"), "{e}");
    assert_eq!(said(&mut g, "return level:thing('door_1').props.locked"), "true", "the old config kept");
}

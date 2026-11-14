//! Props that name something (`c.things.KIND.names`): a level, a place, a
//! thing, a list of things, checked as the game loads with every level
//! read, as a level file or `conf.lua` is read anew, and at `level:spawn`.

use super::*;

/// A door leads to a level and a place there; a lever works things and
/// opens one; boxes to name, one named `lid`, the other `box_2`.
const KINDS: &str = "  c.things.door = { props = { to = \"\", at = \"\", locked = false }, names = { to = \"level\", at = \"place\" } }
  c.things.lever = { props = { works = {}, opens = \"\" }, names = { works = \"things\", opens = \"thing\" } }
  c.things.box = {}
";

/// Two levels, `a` with a door, a lever and two boxes, `crypt` with
/// the place `stairs`; `conf.lua` with `kinds` and `main`.
fn names_folder(name: &str, main: &str, kinds: &str) -> crate::scratch::Scratch {
    let dir = levels_folder(name, main, &[("a", &[]), ("crypt", &[("stairs", [1, 0, 0])])]);
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.blocks = {{ file = \"level.toml\" }}\n  c.player.spawn = {{ 2, 2 }}\n{kinds}end\n")).unwrap();
    let path = dir.join("levels/a.toml");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    level.things.push(crate::grid::Thing::new("door", [12, 2, 4]));
    level.things.push(crate::grid::Thing::new("lever", [6, 2, 4]));
    level.things.push(crate::grid::Thing { name: Some("lid".into()), ..crate::grid::Thing::new("box", [8, 2, 4]) });
    level.things.push(crate::grid::Thing::new("box", [10, 2, 4]));
    level.save(&path).unwrap();
    dir
}

/// Gives the thing of `kind` in level `a` these props (a TOML inline
/// table); the line they are on.
fn give(dir: &Path, kind: &str, props: &str) -> usize {
    let path = dir.join("levels/a.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let head = format!("kind = \"{kind}\"\n");
    let at = text.find(&head).expect("the thing") + head.len();
    let end = at + text[at..].find('\n').unwrap() + 1;
    let text = format!("{}props = {props}\n{}", &text[..end], &text[end..]);
    std::fs::write(&path, &text).unwrap();
    text[..end].lines().count() + 1
}

const MAIN: &str = "function on_level_start(how) end\n";

/// Each word's name is checked as the game loads: the file, the line, the
/// key, what it is to be and the nearest name there is. A place lies in
/// the level the door's `to` names, else in its own; `start` is every
/// level's. Things are named as the game names them (`box_2`). A kind's
/// own value is checked for the things that leave it.
#[test]
fn names_in_props_are_checked_as_the_game_loads() {
    for (kind, props, says) in [
        ("door", "{ to = \"cript\" }", "props.to: no level called cript (did you mean crypt?)"),
        ("door", "{ to = \"crypt\", at = \"stair\" }", "props.at: no place called stair in level crypt (did you mean stairs?)"),
        ("door", "{ at = \"cellar\" }", "props.at: no place called cellar in level a (its places: start)"),
        ("lever", "{ opens = \"lod\" }", "props.opens: no thing called lod in level a (did you mean lid?)"),
        ("lever", "{ works = [\"lid\", \"box_3\"] }", "props.works: no thing called box_3 in level a (did you mean box_2?)"),
        ("lever", "{ works = [\"lid\", 3] }", "props.works: a list of things' names, in quotes, not 3"),
    ] {
        let dir = names_folder("names-wrong", MAIN, KINDS);
        let line = give(&dir, kind, props);
        let e = start_error(&dir);
        assert!(e.contains(&format!("levels/a.toml: line {line}: {says}")), "{props}: {e}");
    }
    for (kind, props) in [("door", "{ to = \"crypt\", at = \"stairs\" }"), ("door", "{ at = \"start\" }"), ("door", "{ to = \"a\" }"), ("lever", "{ works = [\"lid\", \"box_2\", \"door_1\"], opens = \"lid\" }")] {
        let dir = names_folder("names-right", MAIN, KINDS);
        give(&dir, kind, props);
        assert_eq!(start_error(&dir), "", "{props}");
    }
    // The kind's value, for a door that leaves it.
    let dir = names_folder("names-kinds", MAIN, &KINDS.replace("to = \"\"", "to = \"nowhere\""));
    let e = start_error(&dir);
    assert!(e.contains("levels/a.toml: the door: props.to: no level called nowhere (levels: a, crypt) (its kind's, c.things.door.props.to)"), "{e}");
}

/// `names` is refused in `conf.lua` for a prop the kind does not declare,
/// a word not among the four, a default not of the word's type, and a
/// place beside two levels; `like` carries it as it carries `props`.
#[test]
fn a_kinds_names_are_checked_in_conf_lua() {
    for (kinds, says) in [
        ("  c.things.door = { props = { to = \"\" }, names = { too = \"level\" } }\n", "c.things.door.names.too: a door has no such prop (c.things.door.props: to): did you mean `to`?"),
        ("  c.things.door = { props = { to = \"\" }, names = { to = \"levle\" } }\n", "c.things.door.names.to: \"level\", \"place\", \"thing\" or \"things\", not \"levle\": did you mean \"level\"?"),
        ("  c.things.door = { props = { to = 3 }, names = { to = \"level\" } }\n", "c.things.door.names.to = \"level\": its default is a name in quotes (\"\" for none), not c.things.door.props.to = 3"),
        ("  c.things.door = { props = { to = \"\" }, names = { to = \"things\" } }\n", "c.things.door.names.to = \"things\": its default is a list of names ({}), not c.things.door.props.to = \"\""),
        (
            "  c.things.door = { props = { from = \"\", to = \"\", at = \"\" }, names = { from = \"level\", to = \"level\", at = \"place\" } }\n",
            "c.things.door.names.at = \"place\": a place lies in the level its thing names, and a door names two (from and to): which one?",
        ),
    ] {
        let dir = names_folder("names-conf", MAIN, &format!("{kinds}  c.things.lever = {{}}\n  c.things.box = {{}}\n"));
        let e = start_error(&dir);
        assert!(e.contains(says), "{kinds}: {e}");
    }
    // A gate like the door names a level too.
    let dir = names_folder("names-like", MAIN, &format!("{KINDS}  c.things.gate = {{ like = \"door\" }}\n"));
    let path = dir.join("levels/a.toml");
    std::fs::write(&path, std::fs::read_to_string(&path).unwrap().replace("kind = \"door\"", "kind = \"gate\"")).unwrap();
    give(&dir, "gate", "{ to = \"tower\" }");
    assert!(start_error(&dir).contains("props.to: no level called tower"), "{}", start_error(&dir));
}

/// `level:spawn`'s props are checked as a level's file's, against the
/// level being played and the things in it now (one spawned before too).
#[test]
fn spawns_names_are_checked_as_a_files() {
    let main = "function on_level_start(how) level:spawn(\"lever\", 6, 2, { props = { works = { \"nobody\" } } }) end\n";
    let e = start_error(&names_folder("names-spawn", main, KINDS));
    assert!(e.contains("level:spawn: props.works: no thing called nobody in level a (its named things: "), "{e}");
    let main = "function on_level_start(how)\n  level:spawn(\"box\", 4, 2, { name = \"crate\" })\n  level:spawn(\"door\", 5, 2, { props = { to = \"crypt\", at = \"stairs\" } })\n  level:spawn(\"lever\", 6, 2, { props = { works = { \"crate\", \"lid\" } } })\nend\n";
    assert_eq!(start_error(&names_folder("names-spawn-right", main, KINDS)), "");
    let main = "function on_level_start(how) level:spawn(\"door\", 5, 2, { props = { at = \"stairs\" } }) end\n";
    assert!(start_error(&names_folder("names-spawn-place", main, KINDS)).contains("level:spawn: props.at: no place called stairs in level a"));
}

/// A level file saved while playing with a name that is not there is
/// refused as a wrong type is, the level kept as it was; so is `conf.lua`
/// saved so that a level's name no longer holds.
#[test]
fn a_reload_that_breaks_a_name_is_refused() {
    let dir = names_folder("names-reload", MAIN, KINDS);
    give(&dir, "door", "{ to = \"crypt\", at = \"stairs\" }");
    let mut g = game_in(&dir);
    let path = dir.join("levels/a.toml");
    std::fs::write(&path, std::fs::read_to_string(&path).unwrap().replace("at = \"stairs\"", "at = \"cellar\"")).unwrap();
    let e = format!("{:#}", g.reload(&path).unwrap_err());
    assert!(e.contains("props.at: no place called cellar in level crypt"), "{e}");
    assert_eq!(said(&mut g, "return level:thing('door_1').props.at"), "\"stairs\"", "the level as it was");
    // conf.lua: the door's at names things now, and the place is none.
    std::fs::write(&path, std::fs::read_to_string(&path).unwrap().replace("at = \"cellar\"", "at = \"stairs\"")).unwrap();
    g.reload(&path).unwrap();
    let conf = dir.join("conf.lua");
    std::fs::write(&conf, std::fs::read_to_string(&conf).unwrap().replace("at = \"place\"", "at = \"thing\"")).unwrap();
    let e = format!("{:#}", g.reload(&conf).unwrap_err());
    assert!(e.contains("props.at: no thing called stairs in level a"), "{e}");
}

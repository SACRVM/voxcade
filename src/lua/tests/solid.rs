//! Things that stop bodies: a kind's `solid = true`, its model's cells as
//! a level's solid piece's (`world::Solids`), where it stands.

use super::*;

/// A post 3 by 3 by 8.
const POST: &str = "format = \"voxcade voxel 1\"\nsize = [3, 3, 8]\n\n[palette]\na = { color = \"#806040\" }\n\n[[frame]]\nlayers = \"\"\"\naaa *3\n*8\n\"\"\"\n";

/// A room of 64 by 48 with a post at 32, 20 and the start south of it.
const YARD: &str = "format = \"voxcade level 1\"\nsize = [64, 48, 40]\n\n[[thing]]\nkind = \"post\"\nat = [32, 20, 0]\n\n[[place]]\nname = \"start\"\nat = [32, 10, 0]\n";

/// The game of `level` (a level of pieces) with a solid post's kind and
/// this `main.lua`, the player walked up by the keys.
fn yard(name: &str, level: &str, conf: &str) -> crate::scratch::Scratch {
    let dir = pieces_folder(name, &format!("  c.things.post = {{ model = \"post.vvx\", fixed = true, solid = true }}\n  c.things.crate = {{}}\n{conf}"), level, "function on_tick() player:walk(input.dir, 40) end\n");
    std::fs::write(dir.join("post.vvx"), POST).unwrap();
    dir
}

fn walk_up(g: &mut Session<LuaGame>, ticks: usize) {
    for _ in 0..ticks {
        g.step(&Input { up: true, ..Input::default() });
    }
}

/// A body walked by `thing:walk` stops at a solid thing as at a solid
/// piece; the thing placed elsewhere frees its cells and takes the new
/// ones (at the tick's end), removed it frees them, and the body goes on.
/// Its cells are no voxels of the world: `level:voxel` says 0 there.
#[test]
fn a_solid_thing_stops_bodies_where_it_stands() {
    let dir = yard("solid", YARD, "");
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "level:solid(32, 20, 3), level:solid(31, 19, 7), level:solid(32, 20, 8), level:voxel(32, 20, 3)"), "true\ttrue\tfalse\t0");
    walk_up(&mut g, 60);
    let y: f32 = said(&mut g, "player.y").parse().unwrap();
    assert!((14.0..19.0).contains(&y), "stopped south of the post at y {y}");
    // Placed elsewhere: the old cells free, the new ones solid.
    said(&mut g, "level:thing('post_1'):place(10, 30, 0)");
    assert_eq!(said(&mut g, "level:solid(32, 20, 3), level:solid(10, 30, 3)"), "false\ttrue", "moved at the tick's end");
    let kept = g.snapshot().unwrap();
    walk_up(&mut g, 30);
    let y: f32 = said(&mut g, "player.y").parse().unwrap();
    assert!(y > 22.0, "walked on where the post stood: y {y}");
    g.restore(&kept);
    assert_eq!(said(&mut g, "level:solid(32, 20, 3), level:solid(10, 30, 3)"), "false\ttrue", "a snapshot keeps the moved cells");
    said(&mut g, "level:thing('post_1'):remove()");
    assert_eq!(said(&mut g, "level:solid(10, 30, 3)"), "false", "removed: free");
    g.restore(&kept);
    assert_eq!(said(&mut g, "level:solid(10, 30, 3)"), "true", "and back with the snapshot");
}

/// A solid thing set onto a body (placed, or spawned) pushes it out
/// sideways to the nearest free spot at its height, never onto its head;
/// the state hash sees where the solid things stand.
#[test]
fn a_solid_thing_set_onto_a_body_pushes_it_out_sideways() {
    let mut g = game_in(&yard("solid-push", YARD, ""));
    steps(&mut g, 10);
    let at = |g: &mut Session<LuaGame>| -> Vec<f32> { said(g, "player.x, player.y, player.z").split('\t').map(|v| v.parse().unwrap()).collect() };
    let before = at(&mut g);
    let hash = g.game.state_hash();
    said(&mut g, "level:thing('post_1'):place(player.x, player.y, player.z)");
    assert_ne!(g.game.state_hash(), hash);
    let now = at(&mut g);
    assert_eq!(said(&mut g, "level:solid(math.floor(player.x), math.floor(player.y), math.floor(player.z) + 1)"), "false", "out of it: {before:?} to {now:?}");
    assert_eq!(now[2], before[2], "at its height, not on the post's head");
    assert!((now[0] - before[0]).abs() + (now[1] - before[1]).abs() <= 6.0, "beside it: {before:?} to {now:?}");
    // A spawned one too.
    said(&mut g, "level:spawn('post', player.x, player.y)");
    assert_eq!(said(&mut g, "level:solid(math.floor(player.x), math.floor(player.y), math.floor(player.z) + 1)"), "false");
}

/// Only a thing that stands stops bodies: `solid` on a kind that is not
/// `fixed`, has a behavior, has no model, or is no boolean, is refused at
/// load naming the key; a place or a body's thing the file sets inside a
/// solid thing is a mistake naming its line.
#[test]
fn a_solid_kind_stands_and_nothing_stands_inside_it() {
    for (conf, said) in [
        ("  c.things.wall = { model = \"post.vvx\", solid = true }\n", "c.things.wall.solid: only a thing that stands stops bodies: fixed = true with it"),
        ("  c.things.wall = { fixed = true, solid = true }\n", "c.things.wall.solid: the cells its model is drawn in stop bodies: give it a model"),
        ("  c.things.wall = { model = \"post.vvx\", fixed = true, solid = \"yes\" }\n", "c.things.wall.solid: true or false"),
        ("  c.things.wall = { model = \"post.vvx\", fixed = true, sloid = true }\n", "did you mean `solid`?"),
    ] {
        let e = start_error(&yard("solid-kind", YARD, conf));
        assert!(e.contains(said), "{conf}: {e}");
    }
    let dir = platformer_levels("solid-walker", "", "  c.things.slime = { behavior = \"walker\", model = \"hero.vvx\", solid = true }\n", &[]);
    let e = start_error(&dir);
    assert!(e.contains("c.things.slime.solid: only a thing that stands stops bodies, and a kind with a behavior moves by its ready-made rules"), "{e}");
    let dir = platformer_levels("solid-jump", "", "  c.things.post = { model = \"post.vvx\", fixed = true, solid = true }\n", &[("post", [10, 2, 4])]);
    std::fs::write(dir.join("post.vvx"), POST).unwrap();
    let mut g = game_in(&dir);
    assert_eq!((halted(&g), said(&mut g, "level:solid(10, 2, 5)")), (String::new(), "true".to_string()), "the jump & run's rules take it");

    let inside = YARD.replace("at = [32, 10, 0]", "at = [33, 21, 0]");
    let e = start_error(&yard("solid-place", &inside, ""));
    assert!(e.contains("levels/room.toml") && e.contains("line 9: the place 'start' lies inside the solid thing 'post' (line 5) (c.things.post.solid)"), "{e}");
    let crate_in = format!("{YARD}\n[[thing]]\nkind = \"crate\"\nat = [31, 19, 2]\n");
    let e = start_error(&yard("solid-crate", &crate_in, ""));
    assert!(e.contains("line 13: the crate at [31, 19, 2] stands inside the solid thing 'post' (line 5)"), "{e}");
    // Beside it, or a fixed kind inside it (a sign on it), loads.
    let beside = format!("{YARD}\n[[thing]]\nkind = \"crate\"\nat = [34, 19, 0]\n");
    assert_eq!(start_error(&yard("solid-beside", &beside, "")), "");
}

/// Rounds stop at a solid thing as at a solid piece: the shooter's rounds
/// at a rifleman behind a post never reach him; without the post they do.
#[test]
fn rounds_stop_at_a_solid_thing() {
    let hits = |post: bool| {
        let main = format!(
            "function on_level_start(how)\n  level:spawn('rifleman', player.x, player.y - 30)\n  {}\nend\nfunction on_hit(thing, by, damage) world.state.hits = (world.state.hits or 0) + damage end\n",
            if post { "level:spawn('post', player.x, player.y - 14)" } else { "" }
        );
        let conf = "  c.things.rifleman = { behavior = \"soldier\", weapon = \"enemy_rifle\" }\n  c.things.post = { model = \"player.vvx\", fixed = true, solid = true }";
        let mut g = game_in(&kit_folder(&format!("solid-rounds-{post}"), conf, &main));
        let fire = g.actions().iter().position(|a| *a == "fire").unwrap();
        for _ in 0..90 {
            g.step(&Input { actions: 1 << fire, ..Input::default() });
        }
        assert_eq!(halted(&g), "");
        state(&mut g, "hits")
    };
    assert!(hits(false).contains("Int"), "without the post the rounds hit: {}", hits(false));
    assert_eq!(hits(true), "", "the post stops them");
}

/// A solid thing in a level of a location (`location = "station"`) is
/// where the level's record puts it on return: a post moved in the yard,
/// the player gone to the hall and back, blocks at its new place and not
/// its old (its cells laid after the record, `settle_solids`); removed,
/// nothing blocks. A persistent kind the same after leaving the location.
#[test]
fn a_solid_thing_a_level_remembers_blocks_where_it_was_left() {
    let back = |how: &str, conf: &str, away: &str| {
        let level = YARD.replace("size = [64, 48, 40]\n", "size = [64, 48, 40]\nlocation = \"station\"\n").replace("kind = \"post\"", "kind = \"post\"\nname = \"post\"");
        let main = format!("function on_tick()\n  if world.tick == 2 then {how} world:go_to('{away}') end\n  if world.tick == 4 then world:go_to('room') end\nend\n");
        let dir = yard(&format!("solid-kept-{away}-{}", how.len()), &level, conf);
        std::fs::write(dir.join("main.lua"), main).unwrap();
        let hall = "format = \"voxcade level 1\"\nsize = [32, 32, 20]\nlocation = \"station\"\n\n[[place]]\nname = \"start\"\nat = [16, 16, 0]\n";
        std::fs::write(dir.join("levels/hall.toml"), hall).unwrap();
        std::fs::write(dir.join("levels/street.toml"), hall.replace("location = \"station\"\n", "")).unwrap();
        std::fs::write(dir.join("conf.lua"), std::fs::read_to_string(dir.join("conf.lua")).unwrap().replace("end\n", "  c.start = \"room\"\nend\n")).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 6);
        assert_eq!(halted(&g), "");
        assert_eq!(said(&mut g, "level.name"), "\"room\"");
        said(&mut g, "level:solid(32, 20, 3), level:solid(10, 30, 3)")
    };
    assert_eq!(back("level:thing('post'):place(10, 30, 0)", "", "hall"), "false\ttrue", "moved: blocks at its new place only");
    assert_eq!(back("level:thing('post'):remove()", "", "hall"), "false\tfalse", "removed: nothing blocks");
    assert_eq!(back("level:thing('post'):place(10, 30, 0)", "", "street"), "true\tfalse", "the location left: as its file has it");
    let persistent = "  c.things.post = { model = \"post.vvx\", fixed = true, solid = true, persistent = true }\n";
    assert_eq!(back("level:thing('post'):place(10, 30, 0)", persistent, "street"), "false\ttrue", "a persistent kind as left");
}

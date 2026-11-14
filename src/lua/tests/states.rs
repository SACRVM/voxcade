//! The states and the things in them: `player.state`, `thing.state`,
//! spawning, names, kinds' effects, inventories.

use super::*;

/// A world made in code starts empty and is built with colours; saving
/// `conf.lua` keeps what the script built, and a new size waits for F5.
#[test]
fn a_world_made_in_code_is_built_with_colours() {
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 31, 31, 0, "#406080")
  level:fill(2, 2, 1, 4, 4, 3, "#ff0000")
  level:fill(3, 3, 3, 3, 3, 3, nil)
  level:set_voxel(10, 10, 1, "#406080")
  world.state.floor = level:voxel(5, 5, 0)
  world.state.same = level:voxel(10, 10, 1)
  world.state.red = level:voxel(2, 2, 1)
  world.state.hole = level:voxel(3, 3, 3)
end
"##;
    let dir = code_world("built", main, &[]);
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    let w = &g.game.sim().world;
    assert_eq!(w.size, glam::IVec3::new(32, 32, 8));
    let (floor, red) = (w.get(glam::IVec3::new(5, 5, 0)), w.get(glam::IVec3::new(2, 2, 1)));
    assert_eq!(w.palette[floor as usize], [0x40, 0x60, 0x80, 255]);
    assert_eq!(w.palette[red as usize], [255, 0, 0, 255]);
    assert_eq!(state(&mut g, "same"), state(&mut g, "floor"), "one color, one palette entry");
    assert_eq!(state(&mut g, "hole"), format!("{:?}", crate::lua::Value::Int(0)));
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("{ 32, 32, 8 }", "{ 64, 64, 8 }").replace("\nend", "\n  c.camera.pitch = 60\nend")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    assert_eq!(g.cfg.camera.pitch, 60.0);
    assert_eq!(g.game.sim().world.size.x, 32, "a new size waits for a restart");
    assert_ne!(g.game.sim().world.get(glam::IVec3::new(5, 5, 0)), 0, "saving conf.lua kept the world the script built");
    let dir = code_world("bad-colour", "function on_level_start(how) level:set_voxel(1, 1, 1, \"red\") end\n", &[]);
    assert!(start_error(&dir).contains("#rrggbb"));
}

/// `place` puts one of the script's things exactly where it is told,
/// standing still; `level.camera:look_at` rests the camera over a point
/// until `follow` follows a thing again.
#[test]
fn place_and_look_at() {
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 31, 31, 0, "#406080")
  level.state.box = level:spawn("box", 8.5, 8.5)
  level.camera:look_at(20, 24)
end

function on_tick()
  local box = level:thing(level.state.box)
  if world.tick < 20 then box:walk({ x = 1, y = 0 }, 30) end
  if world.tick == 20 then box:place(12.5, 9.5) end
  if world.tick == 40 then level.camera:follow(box) end
end
"##;
    let mut g = game_in(&code_world("place", main, &[]));
    steps(&mut g, 21);
    assert_eq!(halted(&g), "");
    let body = g.game.focus().map(|b| b.pos);
    assert_eq!(body, None, "look_at follows nothing");
    let things = g.game.inspect();
    let x = things["things"][0]["x"].as_f64().unwrap();
    assert_eq!(x, 12.5, "placed exactly");
    let cam = g.game.sim().cam_target;
    assert_eq!((cam.x, cam.y), (20.0, 24.0), "the camera rests");
    steps(&mut g, 30);
    assert!(g.game.focus().is_some(), "focus follows again");
}

/// Sokovox reads all 155 of Skinner's Microban levels and builds each one
/// (N goes on to the next).
#[test]
fn sokovox_builds_every_level() {
    use crate::replay::{Script, parse_script};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/sokovox");
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let mut script = Script::new(parse_script(&["next:1;none:1"; 154].join(";"), g.actions()).unwrap());
    for _ in 0..154 * 2 {
        g.step(&script.next_input());
        assert_eq!(halted(&g), "");
    }
    assert_eq!(state(&mut g, "puzzle"), format!("{:?}", crate::lua::Value::Int(155)));
}

/// `player.state` holds the score (the engine's: the title and the high
/// scores see it) beside the game's own keys; `pairs` walks both in order,
/// snapshots keep the game's, and the state hash takes it in only once a
/// game keeps some.
#[test]
fn player_state_holds_the_score_and_the_games_own() {
    let main = r#"
function on_tick()
  player.state.score = player.state.score + 5
  if world.tick == 2 then player.state.coins = 3 end
  local keys = ""
  for k, v in pairs(player.state) do keys = keys .. k .. "=" .. tostring(v) .. " " end
  world.state.keys = keys
end
"#;
    let mut g = game("player-state", main);
    let plain = game("player-none", "function on_tick() end\n").game.state_hash();
    assert_eq!(g.game.state_hash(), plain, "no player state yet: the hash as before");
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.score(), 15);
    assert_eq!(state(&mut g, "keys"), format!("{:?}", crate::lua::Value::Str(b"coins=3 score=15 ".to_vec())));
    let inspected = g.game.inspect();
    assert_eq!((inspected["player"]["state"]["score"].as_i64(), inspected["player"]["state"]["coins"].as_i64()), (Some(15), Some(3)));
    let snapshot = g.snapshot().unwrap();
    let kept = g.game.state_hash();
    steps(&mut g, 2);
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), kept, "the coins and the score came back");
    let e = game("player-score-text", "function on_tick() player.state.score = 'many' end\n");
    let mut e = e;
    steps(&mut e, 1);
    assert!(halted(&e).contains("player.state.score: a whole number, not string"), "{}", halted(&e));
}

/// `thing.state` holds the thing's hits (`hp`, the engine's, where its
/// kind declares them) beside the game's own keys, goes with the thing,
/// and `inspect` shows both.
#[test]
fn thing_state_holds_hp_and_the_games_own() {
    let main = r#"
function on_level_start(how)
  level.state.gem = level:spawn("gem", 20, 20)
  level.state.gem.state.shine = 2
  level.state.gem.state.hp = 4
  level.state.miner = level:spawn("miner", 30, 30)
end

function on_tick()
  local gem = level.state.gem
  if gem and world.tick == 2 then
    gem.state.hp = gem.state.hp - 1
    world.state.hp = gem.state.hp
  end
  if gem and world.tick == 3 then
    gem:remove()
    level.state.gem = nil
  end
end
"#;
    let mut g = game("thing-state", main);
    let things = g.game.inspect();
    let gem = &things["things"][0]["state"];
    assert_eq!((gem["hp"].as_i64(), gem["shine"].as_i64()), (Some(4), Some(2)));
    assert!(things["things"][1]["state"].get("hp").is_none(), "a miner's kind declares no hit points");
    let gem_id = things["things"][0]["id"].as_u64().unwrap() as u32;
    let gem_hp = g.game.stage.host.get(crate::sim::things::Id::from_raw(gem_id)).map(|t| t.hp);
    assert_eq!(gem_hp, Some(4), "the engine's hit points, not a key of the game's");
    steps(&mut g, 2);
    assert_eq!(state(&mut g, "hp"), format!("{:?}", crate::lua::Value::Int(3)));
    steps(&mut g, 1);
    assert_eq!(halted(&g), "");
    assert!(g.game.capture().unwrap().data.is_empty(), "its state went with it");
}

/// `level` is not there in `on_world_start` (the run begins before its
/// first level); `world.state` is not there at the top of `main.lua`
/// (nothing would keep it).
#[test]
fn the_level_comes_after_the_worlds_start() {
    let e = start_error(&folder("level-at-start", "function on_world_start()\n  level:fill(0, 0, 0, 1, 1, 0, '#ffffff')\nend\n"));
    assert!(e.contains("main.lua:2") && e.contains("level:fill: there is no level yet in on_world_start; build it in on_level_start"), "{e}");
    let e = start_error(&folder("state-at-top", "world.state.counter = 100\n"));
    assert!(e.contains("there is no game here yet"), "{e}");
    let mut order = game("order", "function on_world_start() world.state.order = 'world' end\nfunction on_level_start(how) world.state.order = world.state.order .. ' level ' .. how end\n");
    assert_eq!(state(&mut order, "order"), format!("{:?}", crate::lua::Value::Str(b"world level start".to_vec())));
}

/// Things live in their level: `world.state` and `player.state`, which
/// outlive it, refuse them; a view of another state is no plain data
/// either. Each is named where it sits, at the tick it went in.
#[test]
fn long_lived_states_hold_no_things_and_no_views() {
    let e = start_error(&folder("thing-in-world", "function on_level_start(how)\n  world.state.hero = level:spawn('miner', 10, 10)\nend\n"));
    assert!(e.contains("world.state.hero is a thing"), "{e}");
    let mut g = game("thing-in-player", "function on_level_start(how) level.state.m = level:spawn('miner', 10, 10) end\nfunction on_tick() if world.tick == 3 then player.state.pet = level.state.m end end\n");
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    steps(&mut g, 1);
    assert!(halted(&g).contains("player.state.pet is a thing"), "{}", halted(&g));
    let mut g = game("view-in-state", "function on_tick() world.state.p = player.state end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("world.state.p is a view or another state"), "{}", halted(&g));
}

/// One handle per thing and one view per state: the same thing is the
/// same value wherever it comes from, and `t.state == t.state`.
#[test]
fn a_thing_is_one_value_everywhere() {
    let main = r#"
function on_level_start(how)
  level.state.m = level:spawn("miner", 10, 10)
end

function on_tick()
  local m = level.state.m
  local found = level:things("miner")[1]
  world.state.same = rawequal(m, found) and rawequal(m, level:thing(m.id))
  world.state.views = rawequal(m.state, m.state) and rawequal(player.state, player.state)
end
"#;
    let mut g = game("one-handle", main);
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "same"), format!("{:?}", crate::lua::Value::Bool(true)));
    assert_eq!(state(&mut g, "views"), format!("{:?}", crate::lua::Value::Bool(true)));
}

/// `thing:remove()` takes the thing away at the tick's end: it can still be
/// read until then, a second remove says it was going already, and it is
/// out of the level's lists and counts at once.
#[test]
fn removal_waits_for_the_ticks_end() {
    let main = r#"
function on_level_start(how)
  level.state.m = level:spawn("miner", 10, 10)
end

function on_tick()
  local m = level.state.m
  if world.tick == 2 then
    world.state.first = m:remove()
    world.state.again = m:remove()
    world.state.x = m.x
    world.state.counted = level:count("miner")
  elseif world.tick == 3 then
    world.state.after = level:count("miner")
    level.state.m = nil
  end
end
"#;
    let mut g = game("remove-later", main);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "first"), format!("{:?}", crate::lua::Value::Bool(true)));
    assert_eq!(state(&mut g, "again"), format!("{:?}", crate::lua::Value::Bool(false)));
    assert_eq!(state(&mut g, "counted"), format!("{:?}", crate::lua::Value::Int(0)), "out of the counts at once");
    assert_eq!(state(&mut g, "after"), format!("{:?}", crate::lua::Value::Int(0)));
}

/// `level:spawn` takes its options in a table: the height, the way it
/// faces, its first state (copied); a bare number is the old form, named.
#[test]
fn spawn_takes_a_table_of_options() {
    let main = r#"
function on_level_start(how)
  local first = { mood = "calm" }
  level.state.m = level:spawn("miner", 10, 10, { z = 5, facing = { x = 1, y = 0 }, state = first })
  first.mood = "changed"
end

function on_tick()
  local m = level.state.m
  world.state.mood = m.state.mood
  world.state.facing = m.facing.x
end
"#;
    let mut g = game("spawn-options", main);
    let z = g.game.inspect()["things"][0]["z"].as_f64().unwrap();
    assert_eq!(z, 5.0);
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "mood"), format!("{:?}", crate::lua::Value::Str(b"calm".to_vec())), "the first state is a copy");
    assert_eq!(state(&mut g, "facing"), format!("{:?}", crate::lua::Value::Int(1)));
    let e = start_error(&folder("spawn-z", "function on_level_start(how) level:spawn('miner', 10, 10, 5) end\n"));
    assert!(e.contains("the fourth is a table of options"), "{e}");
}

/// The player is a thing that belongs to no level: a game's
/// `c.player.kind` is its figure, standing where each level starts and
/// walked like any thing the script moves, the same `player` from level to
/// level with its state; the level's things do not list it, it has no id,
/// and a snapshot keeps it.
#[test]
fn the_player_is_a_thing_from_level_to_level() {
    let main = r#"
function on_world_start() player.state.coins = 3 end

function on_level_start(how)
  world.state.seen = (world.state.seen or "") .. level.name .. ":" .. player.kind .. ":" .. tostring(player.id) .. ":" .. level:count() .. ":" .. level:count("hero") .. ":" .. player.x .. ";"
  world.state.coins = player.state.coins
end

function on_tick()
  player:walk({ x = 1, y = 0 }, 30)
  if level.name == "a" then
    world.state.same = world.players[1] == player and tostring(player) == "player" and level:thing(level:things()[1].id) ~= player
  end
  if world.tick == 20 then world.state.walked = player.x end
  if world.tick == 21 then world:go_to("b", "east") end
end
"#;
    let east: &[(&str, [i32; 3])] = &[("east", [12, 2, 4])];
    let dir = levels_folder("player-thing", main, &[("a", &[]), ("b", east)]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.player.kind = \"hero\"\n  c.things.hero = {}\n  c.things.box = {}\nend\n").unwrap();
    let path = dir.join("levels/a.toml");
    let mut level = crate::grid::GridLevel::read(&path).unwrap();
    level.things.push(crate::grid::Thing::new("box", [10, 2, 4]));
    level.save(&path).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 10);
    let snapshot = g.snapshot().unwrap();
    let mut first = Vec::new();
    for _ in 0..20 {
        steps(&mut g, 1);
        first.push(g.game.state_hash());
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "seen"), format!("{:?}", crate::lua::Value::Str(b"a:hero:nil:1:0:2.0;b:hero:nil:0:0:12.5;".to_vec())));
    assert_eq!(state(&mut g, "same"), format!("{:?}", crate::lua::Value::Bool(true)));
    assert_eq!(state(&mut g, "coins"), format!("{:?}", crate::lua::Value::Int(3)), "its state went along");
    assert!(!state(&mut g, "walked").contains("2.0"), "it walked: {}", state(&mut g, "walked"));
    let x = g.game.focus().unwrap().pos.x;
    assert!(x > 12.5 && x < 16.0, "the camera follows it, from b's east on: {x}");
    g.restore(&snapshot);
    for (n, hash) in first.iter().enumerate() {
        steps(&mut g, 1);
        assert_eq!(g.game.state_hash(), *hash, "tick {} after the restore", n + 11);
    }
}

/// The player cannot be removed; without a figure it stands nowhere and
/// says so; with ready-made rules it is their player, moved by them.
#[test]
fn the_player_is_the_rules_figure_or_none() {
    let mut g = game("remove-player", "function on_tick() player:remove() end\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("player:remove: the player belongs to no level"), "{}", halted(&g));
    let mut g = game("no-figure", "function on_tick()\n  world.state.score = player.state.score\n  world.state.x = player.x\nend\n");
    steps(&mut g, 1);
    assert!(halted(&g).contains("the player has no figure in this game (c.player.kind names the kind of thing it is)"), "{}", halted(&g));

    let main = "function on_tick()\n  world.state.x = player.x\n  world.state.kind = player.kind\n  if world.tick == 3 then player:walk({ x = 1, y = 0 }, 10) end\nend\n";
    let dir = platformer_levels("platformer-player", main, "", &[]);
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    let x = g.game.focus().unwrap().pos.x as f64;
    assert_eq!(state(&mut g, "x"), format!("{:?}", crate::lua::Value::Float(x.to_bits())), "the hero");
    assert_eq!(state(&mut g, "kind"), format!("{:?}", crate::lua::Value::Str(b"hero".to_vec())));
    steps(&mut g, 1);
    assert!(halted(&g).contains("thing:walk: a hero moves by its ready-made rules"), "{}", halted(&g));

    let mut g = game_in(&kit_folder("shooter-player", "", "function on_tick()\n  world.state.x = player.x\n  world.state.kind = tostring(player.kind)\n  if world.tick == 3 then player:walk({ x = 1, y = 0 }, 10) end\nend\n"));
    steps(&mut g, 2);
    let x = g.game.focus().unwrap().pos.x as f64;
    assert_eq!(state(&mut g, "x"), format!("{:?}", crate::lua::Value::Float(x.to_bits())), "the shooter's own");
    assert_eq!(state(&mut g, "kind"), format!("{:?}", crate::lua::Value::Str(b"nil".to_vec())));
    steps(&mut g, 1);
    assert!(halted(&g).contains("thing:walk: the player moves by its ready-made rules"), "{}", halted(&g));
}

/// A pickup the code takes on touch counts into `player.state.inventory`
/// (a full stack takes no more); `world.items` lists the items in their
/// order.
#[test]
fn pickups_count_into_the_inventory() {
    let main = r#"
function on_touch(a, b)
  if a == player and b.kind == "key" then
    player:take(b.props.item)
    player.state.score = player.state.score + b.props.worth
    b:remove()
  end
end
function on_tick()
  world.state.keys = player.state.inventory.key
  world.state.first = world.items[1].name .. world.items[2].stack
end
"#;
    let conf = "  c.items.key = { stack = 2, order = 2 }\n  c.items.amulet = { order = 1 }\n  c.things.key = { behavior = \"pickup\", props = { item = \"key\", worth = 50 } }\n";
    let dir = platformer_levels("inventory", main, conf, &[("key", [6, 2, 4]), ("key", [8, 2, 4]), ("key", [10, 2, 4])]);
    let mut g = game_in(&dir);
    for _ in 0..40 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "keys"), format!("{:?}", crate::lua::Value::Int(2)), "two of three: its stack");
    assert_eq!(state(&mut g, "first"), format!("{:?}", crate::lua::Value::Str(b"amulet2".to_vec())));
    assert_eq!(g.game.score(), 150, "all three taken");
}

/// An inventory holds items of `c.items` by name, from 0 to their stack,
/// the player's and a thing's alike; a mistake stops the game at the end of
/// the tick, naming it.
#[test]
fn inventories_hold_items_by_name_up_to_their_stack() {
    for (name, line, says) in [
        ("unknown", "player.state.inventory.kye = 1", "player.state.inventory.kye: no item called 'kye' (c.items: key)"),
        ("too-many", "player.state.inventory.key = 3", "player.state.inventory.key = 3: from 0 to 2 (c.items.key.stack)"),
        ("half", "player.state.inventory.key = 0.5", "player.state.inventory.key is 0.5: a count is a whole number"),
        ("not-a-table", "player.state.inventory = 1", "player.state.inventory is an integer: an inventory is a table"),
        ("chest", "level.state.chest.state.inventory = { gold = 1 }", "state.inventory.gold: no item called 'gold' (c.items: key)"),
    ] {
        let main = format!("function on_level_start(how) level.state.chest = level:spawn('chest', 8, 8) end\nfunction on_tick() if world.tick == 2 then {line} end end\n");
        let dir = folder(&format!("inventory-{name}"), &main);
        let conf = CONF.replace("  c.things.gem = { hp = 2 }\n", "  c.things.gem = { hp = 2 }\n  c.items.key = { stack = 2 }\n  c.things.chest = {}\n");
        std::fs::write(dir.join("conf.lua"), conf).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 1);
        assert_eq!(halted(&g), "", "{name}: an empty inventory to begin with");
        steps(&mut g, 1);
        assert!(halted(&g).contains(says) && halted(&g).contains("(at the end of tick 2)"), "{name}: {}", halted(&g));
    }
}

/// What names the player's kind or an item is checked when the game loads.
#[test]
fn the_players_kind_and_the_items_are_checked_at_load() {
    let dir = platformer_levels("bad-item", "", "  c.items.key = {}\n  c.things.key = { behavior = \"pickup\", item = \"kye\" }\n", &[]);
    assert!(start_error(&dir).contains("c.things.key.item is no more: what a pickup gives is your code's"), "{}", start_error(&dir));
    let dir = platformer_levels("kit-player-kind", "", "  c.player.kind = \"hero\"\n", &[]);
    assert!(start_error(&dir).contains("c.player.kind = \"hero\": the ready-made rules (c.kit) bring their own player"), "{}", start_error(&dir));
    for (name, line, says) in [
        ("unknown-kind", "  c.player.kind = \"hro\"\n", "c.player.kind = \"hro\": no kind of thing called that (c.things: gem, miner)"),
        ("no-stack", "  c.items.key = { stack = 0 }\n", "c.items.key.stack = 0: from 1 to 1000000"),
    ] {
        let dir = folder(name, "");
        std::fs::write(dir.join("conf.lua"), CONF.replace("  c.things.miner = {}\n", &format!("  c.things.miner = {{}}\n{line}"))).unwrap();
        assert!(start_error(&dir).contains(says), "{name}: {}", start_error(&dir));
    }
}

/// What the review of stage 4 found: a level file saved while playing
/// keeps the player where its figure stands (not where the camera looks);
/// a `conf.lua` that takes the player's kind away is refused, the old one
/// kept; an item put back in `conf.lua` mends the inventory it stopped.
#[test]
fn the_player_outlasts_reloads_and_config_changes() {
    let main = "function on_level_start(how)\n  level.state.box = level:spawn('box', 12, 2)\n  level.camera:follow(level.state.box)\nend\nfunction on_tick() player:walk({ x = 1, y = 0 }, 20) end\n";
    let dir = levels_folder("player-reload", main, &[("a", &[])]);
    let conf = "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\n  c.player.kind = \"hero\"\n  c.things.hero = {}\n  c.things.box = {}\n  c.items.key = {}\nend\n";
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 10);
    let x = g.game.inspect()["player"]["x"].as_f64().unwrap();
    let level = crate::grid::GridLevel::read(&dir.join("levels/a.toml")).unwrap();
    level.save(&dir.join("levels/a.toml")).unwrap();
    g.reload(&dir.join("levels/a.toml")).unwrap();
    assert_eq!(g.game.inspect()["player"]["x"].as_f64(), Some(x), "where the player stood, not at the box");

    std::fs::write(dir.join("conf.lua"), conf.replace("  c.things.hero = {}\n", "")).unwrap();
    let e = g.reload(&dir.join("conf.lua")).err().map(|e| format!("{e:#}")).unwrap_or_default();
    assert!(e.contains("c.player.kind = \"hero\": no kind of thing called that"), "{e}");
    steps(&mut g, 1);
    assert_eq!(halted(&g), "", "the old config kept");

    std::fs::write(dir.join("main.lua"), "function on_tick() player.state.inventory.key = 1 end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("  c.items.key = {}\n", "")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    steps(&mut g, 1);
    assert!(halted(&g).contains("player.state.inventory.key: there are no items"), "{}", halted(&g));
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    assert_eq!(halted(&g), "", "the item back, the game goes on");
}

/// Things placed by the level file are named by their kind and number,
/// spawned ones as they were named; kinds have families and props; a
/// checkpoint may name a thing; hits name their weapon; a body may
/// bounce; falling out is told once.
#[test]
fn names_families_props_and_the_rest() {
    let main = r#"
function on_hit(thing, by, damage, weapon)
  world.state.weapon = weapon
end

function on_fall_out(thing)
  world.state.fell = (world.state.fell or 0) + 1
end

function on_level_start(how)
  world.state.how = how
  if how == "start" then
    level.state.b = level:spawn("box", 6, 2, { name = "crate", props = { worth = 9 } })
  end
end

function on_tick()
  local b = level:thing("crate")
  if world.tick == 1 then
    world.state.door = level:thing("door_1") ~= nil
    world.state.crates = level:count("crates")
    world.state.worth = b.props.worth
    world.state.kind_worth = level:spawn("box", 9, 2).props.worth
    world.state.family = b.families.crates
    world.state.block = level:block(player.x, player.y, player.z - 1)
    world.state.in_rock = player:in_block("solid")
    b:hurt(1, player, "sword")
  end
  if world.tick == 3 then player:bounce(10) end
  if world.tick == 4 then world.state.rising = player.z > 4 end
  if world.tick == 20 then player:place(2.5, 2.5, -60) end
  if world.tick == 30 then
    player.state.checkpoint = "door_1"
    world:restart()
  end
  if world.tick == 31 then world.state.at_door = math.floor(player.x) end
end
"#;
    let conf = "  c.things.box.family = \"crates\"\n  c.things.box.props = { worth = 1 }\n";
    let mut g = game_in(&events_folder("names", main, conf));
    steps(&mut g, 35);
    assert_eq!(halted(&g), "");
    let s = &g.game.inspect()["state"];
    assert_eq!((s["door"].as_bool(), s["crates"].as_i64(), s["worth"].as_i64(), s["kind_worth"].as_i64(), s["family"].as_bool()), (Some(true), Some(1), Some(9), Some(1), Some(true)));
    assert_eq!((s["block"].as_str(), s["in_rock"].as_bool()), (Some("solid"), Some(false)));
    assert_eq!(s["weapon"], "sword");
    assert_eq!(s["rising"], true, "bounced up");
    assert_eq!(s["fell"], 1, "fell out once");
    assert_eq!((s["how"].as_str(), s["at_door"].as_i64()), (Some("restart"), Some(12)), "back at the door the checkpoint names");

    // Props are the kind's, of their kind, and only to read.
    for (code, says) in [
        ("level:spawn(\"box\", 6, 2, { props = { wroth = 9 } })", "props.wroth: a box has no such prop (c.things.box.props: worth)"),
        ("level:spawn(\"box\", 6, 2, { props = { worth = \"lots\" } })", "props.worth is a number by its kind (c.things.box.props.worth = 1), not a string"),
        ("level:spawn(\"door\", 6, 2, { props = { worth = 1 } })", "a door has no such prop (c.things.door.props declares none)"),
        ("level:things(\"box\")[1].props.worth = 2", "thing.props.worth: only to read; what changes while playing goes in thing.state"),
        ("player.families.crates = true", "thing.families.crates: only to read"),
    ] {
        let main = format!("function on_level_start(how) level:spawn(\"box\", 6, 2) end\nfunction on_tick() {code} end\n");
        let mut g = game_in(&events_folder("props-checked", &main, conf));
        steps(&mut g, 2);
        assert!(halted(&g).contains(says), "{code}: {}", halted(&g));
    }
}

/// A kind's effect and sound show as one of its things goes: the
/// script's own kinds too, their hit points set to 0.
#[test]
fn a_kinds_effect_and_sound_show_as_it_goes() {
    let main = r#"
function on_level_start(how)
  level.state.b = level:spawn("box", 6, 2)
end

function on_tick()
  if world.tick == 2 then level.state.b.state.hp = 0 end
end
"#;
    let conf = "  c.things.box.effect = \"poof.vvx\"\n  c.things.box.sound = \"poof\"\n";
    let dir = events_folder("goes", main, conf);
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/poof.toml"), "wave = \"noise\"\nfreq = 700\ndecay = 0.1\n").unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena/player.vvx"), dir.join("poof.vvx")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.stage.effects.iter().map(|e| e.model.as_str()).collect::<Vec<_>>(), ["poof.vvx"], "its effect shows");
    assert!(g.game.stage.host.sim().sounds.iter().any(|s| s.name == "poof"), "its sound plays");
}

/// A kind whose effect is "burst" bursts into its own voxels as one of
/// its things goes: the simulation's pieces, each of its model's colours
/// (no blood without gore), no model played; and they are gone after a
/// few seconds.
#[test]
fn a_kind_that_bursts_goes_in_its_voxels() {
    let main = r#"
function on_level_start(how)
  level.state.b = level:spawn("box", 6, 2)
end

function on_tick()
  if world.tick == 2 then level.state.b.state.hp = 0 end
end
"#;
    let conf = "  c.things.box.model = \"box.vvx\"\n  c.things.box.effect = \"burst\"\n";
    let dir = events_folder("bursts", main, conf);
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/arena/player.vvx");
    std::fs::copy(&model, dir.join("box.vvx")).unwrap();
    let mut g = game_in(&dir);
    let pieces = |g: &Session<LuaGame>| g.game.stage.host.sim().particles.clone();
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert!(g.game.stage.effects.is_empty(), "no model played");
    let shape = crate::vox::load_model(&model).unwrap();
    let colours: std::collections::BTreeSet<[u8; 4]> = shape.frames[0].iter().map(|(_, c)| *c).collect();
    let burst = pieces(&g);
    assert_eq!(burst.len(), shape.frames[0].len(), "a piece for each of its voxels");
    assert!(burst.iter().all(|p| colours.contains(&p.color)), "only its own colors");
    steps(&mut g, 4 * 60);
    assert!(pieces(&g).is_empty(), "the pieces are gone after a few seconds");
}

/// A kind with `fixed = true` has no gravity and does not land: it stays
/// where it is spawned or placed. One without falls to the ground.
#[test]
fn a_fixed_thing_stays_where_it_is_put() {
    let main = r##"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, "#808080")
  level.state.sign = level:spawn("sign", 20, 20, { z = 10 })
  level.state.rock = level:spawn("rock", 30, 30, { z = 10 })
end

function on_tick()
  world.state.sign = math.floor(level.state.sign.z)
  world.state.rock = math.floor(level.state.rock.z)
  if world.tick == 5 then level.state.sign:place(40, 40, 7) end
end
"##;
    let dir = folder("fixed", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.things.sign = { fixed = true }\n  c.things.rock = {}\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 4);
    assert_eq!(state(&mut g, "sign"), "Int(10)", "the sign hangs in the air");
    assert_ne!(state(&mut g, "rock"), "Int(10)", "the rock falls");
    steps(&mut g, 40);
    assert_eq!(state(&mut g, "sign"), "Int(7)", "placed, it stays there");
    assert_eq!(state(&mut g, "rock"), "Int(1)", "on the floor");
    // With a behavior it is the ready-made rules' to move: a mistake.
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"shooter\"\n  c.things.sign = { behavior = \"soldier\", fixed = true }\nend\n").unwrap();
    let err = start_error(&dir);
    assert!(err.contains("c.things.sign.fixed"), "{err}");
}

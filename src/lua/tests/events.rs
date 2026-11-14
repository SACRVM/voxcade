//! Events told to the script: touches, deaths, hits, falls, interact,
//! holds.

use super::*;

/// Touches are told once when they begin and once when they end, the
/// player first; what touches where the level begins, or where
/// `thing:place` puts a thing, touches without `on_touch`.
#[test]
fn touches_begin_and_end_once() {
    let main = r#"
function on_level_start(how)
  -- a box on the player's spot: touching from the start, told nothing
  level:spawn("box", player.x, player.y)
end

function on_touch(a, b)
  world.state.log = (world.state.log or "") .. "touch " .. tostring(a) .. " " .. b.kind .. " " .. world.tick .. ";"
end

function on_touch_end(a, b)
  world.state.log = (world.state.log or "") .. "leave " .. tostring(a) .. " " .. b.kind .. " " .. world.tick .. ";"
end

function on_tick()
  if world.tick <= 40 then player:walk({ x = 1, y = 0 }, 30) end
  if world.tick > 60 and world.tick <= 100 then player:walk({ x = -1, y = 0 }, 30) end
  -- put back onto the door: no touch told
  if world.tick == 120 then player:place(12.5, 2.5) end
end
"#;
    let mut g = game_in(&events_folder("touches", main, ""));
    steps(&mut g, 130);
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    assert!(log.starts_with("leave player box 9;"), "the box from the start is left, never touched: {log}");
    assert_eq!((log.matches("touch player door").count(), log.matches("leave player door").count()), (1, 1), "{log}");
    assert!(!log.contains("door 120") && !log.contains("door 121"), "placed onto the door: no touch told: {log}");
}

/// A thing's death is told once when its hit points run out, by whoever
/// (here the script); it is no longer counted or listed; hit points back,
/// it lives again and may die again.
#[test]
fn deaths_are_told_once_and_things_live_again() {
    let main = r#"
function on_level_start(how) level.state.box = level:spawn("box", 8, 2) end

function on_death(thing, by)
  world.state.deaths = (world.state.deaths or 0) + 1
  world.state.kind = thing.kind
end

function on_tick()
  local box = level.state.box
  if world.tick == 2 then box.state.hp = 0 end
  if world.tick == 4 then world.state.counted = level:count("box") end
  if world.tick == 5 then box.state.hp = 2 end
  if world.tick == 7 then world.state.back = level:count("box") end
  if world.tick == 8 then box.state.hp = -1 end
end
"#;
    let mut g = game_in(&events_folder("deaths", main, ""));
    steps(&mut g, 12);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "deaths"), format!("{:?}", crate::lua::Value::Int(2)), "once each time");
    assert_eq!(state(&mut g, "counted"), format!("{:?}", crate::lua::Value::Int(0)), "the dead are not counted");
    assert_eq!(state(&mut g, "back"), format!("{:?}", crate::lua::Value::Int(1)), "alive again");
}

/// The ready-made rules' events reach the script: the shooter's rounds hit
/// and kill a soldier (`on_hit`, `on_death` naming its kind though it is
/// gone); an item taken by the code (`on_pickup`, counted in first).
#[test]
fn the_kits_events_reach_the_script() {
    let main = r#"
function on_level_start(how) level:spawn("rifleman", player.x, player.y - 24) end
function on_hit(thing, by, damage) world.state.hits = (world.state.hits or 0) + damage; world.state.by = tostring(by) end
function on_death(thing, by) world.state.dead = thing.kind .. " " .. math.floor(thing.y) end
"#;
    let mut g = game_in(&kit_folder("kit-events", "  c.things.rifleman = { behavior = \"soldier\", weapon = \"enemy_rifle\" }", main));
    let fire = g.actions().iter().position(|a| *a == "fire").unwrap();
    for _ in 0..240 {
        g.step(&Input { actions: 1 << fire, ..Input::default() });
    }
    assert_eq!(halted(&g), "");
    assert!(state(&mut g, "hits").contains("Int"), "hit: {}", state(&mut g, "hits"));
    assert_eq!(state(&mut g, "by"), format!("{:?}", crate::lua::Value::Str(b"player".to_vec())));
    assert!(state(&mut g, "dead").contains("114, 105, 102, 108, 101, 109, 97, 110"), "the rifleman's death: {}", state(&mut g, "dead"));

    let main = "function on_touch(a, b) if b.kind == 'key' then player:take('key') b:remove() end end\nfunction on_pickup(thing, item) world.state.got = tostring(thing) .. ' ' .. item .. ' ' .. player.state.inventory[item] end\n";
    let conf = "  c.items.key = { stack = 3 }\n  c.things.key = { behavior = \"pickup\" }\n";
    let mut g = game_in(&platformer_levels("kit-pickup-event", main, conf, &[("key", [6, 2, 4])]));
    for _ in 0..20 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert_eq!(state(&mut g, "got"), format!("{:?}", crate::lua::Value::Str(b"player key 1".to_vec())));
}

/// A held level (`level.state.held`) stops the rules, physics and touches;
/// on_tick and the time go on. The action called interact acts on what
/// the player touches (`on_interact`), and that press is used up.
#[test]
fn holds_and_interactions() {
    let main = r#"
function on_tick()
  level.state.held = world.tick >= 10 and world.tick < 30
  player:walk({ x = 1, y = 0 }, 30)
  if world.tick == 10 then world.state.before = player.x end
  if world.tick == 29 then world.state.held_x = player.x end
  world.state.pressed = input:pressed("interact") or world.state.pressed
end

function on_interact(thing, by)
  world.state.used = thing.kind .. " " .. tostring(by)
end
"#;
    let dir = events_folder("holds", main, "  c.actions = { \"interact\" }\n  c.keys = { interact = { \"E\" } }\n");
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "before"), state(&mut g, "held_x"), "nothing moved while held");
    assert_eq!(g.game.sim().tick, 30, "the time went on");
    // On to the door, then press.
    steps(&mut g, 60);
    g.step(&Input { actions: 1, ..Input::default() });
    assert_eq!(state(&mut g, "used"), format!("{:?}", crate::lua::Value::Str(b"door player".to_vec())));
    assert_eq!(state(&mut g, "pressed"), "", "the press went to on_interact");
}

/// The player's invulnerability with ready-made rules: ticks, or true
/// until set back.
#[test]
fn the_player_can_be_kept_from_hits() {
    let main = "function on_tick()\n  if world.tick == 1 then player.state.invulnerable = true end\n  if world.tick == 2 then world.state.a = player.state.invulnerable; player.state.invulnerable = 30 end\n  if world.tick == 3 then world.state.b = player.state.invulnerable end\nend\n";
    let mut g = game_in(&kit_folder("invulnerable", "", main));
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "a"), format!("{:?}", crate::lua::Value::Bool(true)));
    assert_eq!(state(&mut g, "b"), format!("{:?}", crate::lua::Value::Int(29)), "counted down a tick");
}

/// A door does nothing by itself; one the player stands on from the start
/// is not touched (told nothing); run into, it is.
#[test]
fn a_door_does_nothing_by_itself() {
    let main = "function on_touch(a, b) if b.kind == 'door' then world.state.at = (world.state.at or '') .. b.name .. ';' end end\n";
    let dir = platformer_levels("door-touch", main, "", &[("door", [3, 2, 4]), ("door", [10, 2, 4])]);
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(g.game.outcome(), None, "the door does not win");
    assert_eq!(state(&mut g, "at"), "", "touching from the start: told nothing");
    for _ in 0..40 {
        g.step(&Input { right: true, ..Input::default() });
    }
    assert_eq!((g.game.outcome(), state(&mut g, "at")), (None, format!("{:?}", crate::lua::Value::Str(b"door_2;".to_vec()))));
}

/// thing:hurt lowers the script's kinds' hit points and tells on_hit in the
/// next tick, then on_death; the ready-made rules take it as their own hits
/// (a soldier falls). thing:take counts an item in, up to its stack, and
/// tells on_pickup.
#[test]
fn the_script_hurts_and_gives_as_the_rules_do() {
    let main = r#"
function on_level_start(how) level.state.box = level:spawn("box", 8, 2) end
function on_hit(thing, by, damage) world.state.log = (world.state.log or "") .. "hit " .. thing.kind .. " " .. tostring(by) .. " " .. damage .. " " .. world.tick .. ";" end
function on_death(thing, by) world.state.log = (world.state.log or "") .. "death " .. thing.kind .. " " .. world.tick .. ";" end
function on_pickup(thing, item) world.state.log = (world.state.log or "") .. "pickup " .. tostring(thing) .. " " .. item .. " " .. world.tick .. ";" end
function on_tick()
  if world.tick == 2 then level.state.box:hurt(2, player) end
  if world.tick == 4 then world.state.took = player:take("key", 5) end
end
"#;
    let mut g = game_in(&events_folder("hurt-take", main, "  c.items.key = { stack = 3 }\n"));
    steps(&mut g, 6);
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    assert_eq!(log, "hit box player 2 3;death box 3;pickup player key 5;", "{log}");
    assert_eq!(state(&mut g, "took"), format!("{:?}", crate::lua::Value::Int(3)), "up to its stack");
    assert_eq!(g.game.inspect()["player"]["state"]["inventory"]["key"], 3);

    let main = r#"
function on_level_start(how) level.state.man = level:spawn("rifleman", player.x + 40, player.y + 40) end
function on_death(thing, by) world.state.fell = thing.kind .. " " .. tostring(by) end
function on_tick() if world.tick == 2 then level.state.man:hurt(5, player) end end
"#;
    let mut g = game_in(&kit_folder("kit-hurt", "  c.things.rifleman = { behavior = \"soldier\", weapon = \"enemy_rifle\" }", main));
    // His fall holds the game still a few ticks (hit stop) before it is told.
    steps(&mut g, 20);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "fell"), format!("{:?}", crate::lua::Value::Str(b"rifleman player".to_vec())));
}

/// How a touch began (`on_touch`'s `where`): walked into, the side;
/// come down onto, however fast, the top; something come down onto the
/// player, its bottom. `thing:touching` says what touches every tick.
#[test]
fn a_touch_says_where_it_began() {
    let main = r#"
function on_level_start(how)
  level.state.a = level:spawn("box", 8, 2, { name = "a" })
end

function on_touch(a, b, where)
  if a == player then
    world.state.log = (world.state.log or "") .. tostring(b.name) .. " " .. where .. ";"
  end
end

function on_tick()
  if world.tick <= 30 then player:walk({ x = 1, y = 0 }, 30) end
  if world.tick <= 30 then world.state.touching = math.max(world.state.touching or 0, #player:touching("box")) end
  if world.tick == 40 then
    level:spawn("box", 13, 2, { name = "low" })
    player:place(13.5, 2.5, 60)
  end
  if world.tick == 90 then
    player:place(4.5, 2.5)
    level:spawn("box", 4.5, 2.5, { z = 20, name = "high" })
  end
end
"#;
    let mut g = game_in(&events_folder("where", main, ""));
    steps(&mut g, 140);
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    assert!(log.starts_with("a side;"), "walked into: {log}");
    assert!(log.contains("low top;"), "come down onto it from high up, fast: {log}");
    assert!(log.contains("high bottom;"), "it came down onto the player: {log}");
    assert_eq!(g.game.inspect()["state"]["touching"], 1, "touching while it lasts");
}

/// Falling out of the level is told once, also while the game's code lets
/// the jump & run's hero lie there (the kit sees it out every tick).
#[test]
fn a_fall_is_told_once_while_the_code_waits() {
    use crate::grid::{BlockEntry, GridLevel, Kind};
    let main = "function on_fall_out(t) world.state.fell = (world.state.fell or 0) + 1 end\n";
    let dir = levels_folder("fall-once", main, &[("a", &[])]);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3, lives = 3, taken = 0 }\n  c.world.blocks = { file = \"level.toml\", below = 16 }\n  c.things.hero = { behavior = \"hero\", height = 6 }\nend\n").unwrap();
    // Nothing to stand on.
    let mut level = GridLevel::new(4, glam::IVec3::new(4, 1, 2));
    level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    level.things.push(crate::grid::Thing::new("hero", [6, 2, 4]));
    level.save(&dir.join("levels/a.toml")).unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 240);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["state"]["fell"], 1, "told once");
}

/// A touch between two things that both have a reach counts within the
/// larger one: each is touched within its own (a small ship's hit zone
/// beside a big power-up).
#[test]
fn each_is_touched_within_its_own_reach() {
    let main = r#"
function on_level_start()
  level.state.power = level:spawn("power", 30, 20)
end

function on_tick()
  player:walk({ x = 1, y = 0 }, 20)
end

function on_touch(a, b)
  if b.kind == "power" and not world.state.at then world.state.at = math.floor(30 - a.x) end
end
"#;
    let dir = folder("reach", main);
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.kind = \"ship\"\n  c.player.spawn = { 6, 20 }\n  c.things.ship = { reach = 3 }\n  c.things.power = { reach = 9 }\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 120);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "at"), "Int(8)", "touched as it came within the power-up's reach");
}

//! Several players: seats taken and left, the kits' figures for each,
//! teams, the hash in parts.

use super::*;

/// A second player takes a seat while the game runs: told first among the
/// events, its state a copy of `c.state.player`, its figure in at the
/// tick's end beside the first player, walked by its own keys; it leaves,
/// and comes back with its state.
#[test]
fn a_second_player_takes_a_seat_and_leaves() {
    let (mut g, _dir) = two_players("seats", TWO_PLAYERS);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    assert_eq!(said(&mut g, "return #world.players, world.players[1] == player, world.players[2]"), "2\ttrue\tfalse");
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    g.step(&Input::default());
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return world.state.told"), "\"join 2 new;\"");
    assert_eq!(said(&mut g, "return world.players[2].seat, world.players[2].state.lives, world.players[2].kind, #level:players()"), "2\t3\t\"hero_blue\"\t2");
    assert_eq!(said(&mut g, "return world.players[2].x == player.x"), "true", "beside the first player");
    let right = Input { right: true, ..Input::default() };
    g.set_seat(2, Some(right));
    steps(&mut g, 30);
    assert_eq!(said(&mut g, "return world.players[2].x > player.x + 10, player.x"), "true\t32.0", "seat 1 held nothing and stood");
    let fire = Input { actions: 2, ..Input::default() };
    g.set_seat(2, Some(fire));
    g.step(&Input::default());
    assert_eq!(said(&mut g, "return world.players[2].state.score, player.state.score"), "10\t0");
    g.set_seat(2, None);
    g.step(&Input::default());
    assert_eq!(said(&mut g, "return world.state.told, world.players[2], #level:players()"), "\"join 2 new;leave 2;\"\tfalse\t1");
    g.set_seat(2, Some(Input::default()));
    g.step(&Input::default());
    assert_eq!(said(&mut g, "return world.state.told, world.players[2].state.score"), "\"join 2 new;leave 2;join 2 back;\"\t10");
}

/// A run of two is written down seat by seat (`away` while nobody sat
/// there): played back it comes out the same, going back again and again
/// too, and its seats are in its file.
#[test]
fn a_run_of_two_replays_the_same() {
    use crate::replay::{Recorder, check, check_going_back};
    let (mut g, _dir) = two_players("seats-replay", TWO_PLAYERS);
    let dir = g.dir.clone();
    let mut recorder = Recorder::default();
    let right = Input { right: true, ..Input::default() };
    let fire = Input { actions: 2, ..Input::default() };
    for tick in 0..300 {
        let seat = match tick {
            0..40 => None,
            40..120 => Some(right),
            120..130 => Some(fire),
            130..200 => None,
            _ => Some(Input { left: true, ..Input::default() }),
        };
        g.set_seat(2, seat);
        let input = if tick % 50 < 20 { right } else { Input::default() };
        g.step(&input);
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    let replay = recorder.replay(&g).unwrap();
    assert_eq!(replay.seats.len(), 1);
    assert!(replay.seats[0].input.starts_with("away:40;right:80;fire:10;away:70;left:100"), "{}", replay.seats[0].input);
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/two.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "played back the same");
    assert!(check_going_back::<LuaGame>(&dir, 8).unwrap(), "going back the same");
}

/// Two players are held within `c.camera.hold` of each other, the later
/// seat giving way; moving together nobody is stopped; the camera looks at
/// the middle of them, each in whole voxels against it.
#[test]
fn the_hold_keeps_two_players_together() {
    let (mut g, _dir) = two_players("hold", TWO_PLAYERS);
    let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.players = 2", "c.players = 2\n  c.camera.hold = { 20 }");
    std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
    g = game_in(&g.dir.clone());
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    let right = Input { right: true, ..Input::default() };
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    g.set_seat(2, Some(right));
    steps(&mut g, 150);
    assert_eq!(said(&mut g, "return math.floor(world.players[2].x - player.x + 0.5)"), "20", "held at the hold");
    // Seat 2 back left, seat 1 after it: neither is stopped.
    g.set_seat(2, Some(Input { left: true, ..Input::default() }));
    let x = said(&mut g, "return world.players[2].x").parse::<f32>().unwrap();
    for _ in 0..30 {
        g.step(&Input { left: true, ..Input::default() });
    }
    let now = said(&mut g, "return world.players[2].x").parse::<f32>().unwrap();
    assert!(x - now > 10.0, "moving together, it went on: {x} to {now}");
    // The camera between them, whole voxels from the first.
    let target = g.view_target(1.0);
    let (a, b) = (said(&mut g, "return player.x").parse::<f32>().unwrap(), said(&mut g, "return world.players[2].x").parse::<f32>().unwrap());
    assert!((target.x - (a + b) * 0.5).abs() <= 1.0, "the middle: {} between {a} and {b}", target.x);
    assert_eq!(target.x.fract(), 0.0);
}

/// Each player acts on the thing it touches (`on_interact`, `by` the one
/// who pressed), the press used up for its seat; `frame.players` lists
/// the seats taken at this machine.
#[test]
fn each_player_interacts_and_the_frame_knows_them() {
    let main = r#"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, 1)
  level.state.sign = level:spawn("miner", 40, 32)
end
function on_interact(thing, by)
  world.state.read = (world.state.read or "") .. by.seat .. ";"
end
function on_tick()
  for _, p in ipairs(level:players()) do
    if p.input:pressed("interact") then world.state.unused = (world.state.unused or 0) + 1 end
  end
end
function on_draw(frame)
  frame:text(#frame.players .. " at this screen", 1, 1)
end
"#;
    let (mut g, _dir) = two_players("seats-interact", main);
    let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.actions = { \"jump\", \"fire\" }", "c.actions = { \"jump\", \"interact\" }\n  c.things.miner = {}");
    std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
    g = game_in(&g.dir.clone());
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    said(&mut g, "world.players[2]:place(40, 32)");
    steps(&mut g, 2);
    let press = Input { actions: 2, ..Input::default() };
    g.set_seat(2, Some(press));
    g.step(&Input::default());
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return world.state.read, world.state.unused"), "\"2;\"\tnil", "seat 2 read the sign, the press used up");
    g.look();
    assert_eq!(first_text(&g), "2 at this screen");
}

/// A save keeps every seat: a player's state, its score and where it
/// stood, and players named in states as their seats; loaded with the
/// second seat away, its state waits and comes back with whoever takes it.
#[test]
fn a_save_keeps_every_seat() {
    let (mut g, _dir) = two_players("seats-save", TWO_PLAYERS);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    said(&mut g, "local p = world.players[2]; p.state.score = 70; p.state.lives = 1; p:place(20, 40); world.state.best = p");
    steps(&mut g, 2);
    let text = g.game.save().unwrap();
    assert!(text.contains("world.players[2]"), "{text}");
    let (mut from, _dir) = two_players("seats-save-into", TWO_PLAYERS);
    from.start_from(&text).unwrap();
    assert_eq!(halted(&from), "");
    assert_eq!(said(&mut from, "return world.players[2], world.state.best.seat, world.state.best.state.score"), "false\t2\t70", "away, its state kept");
    from.set_seat(2, Some(Input::default()));
    from.step(&Input::default());
    assert_eq!(said(&mut from, "return world.state.told, world.players[2].state.lives, world.state.best == world.players[2]"), "\"join 2 new;join 2 back;\"\t1\ttrue", "the told from the save, then back");
}

/// A script's own thing hurt to death by `thing:hurt` names who did it in
/// `on_death` (a player of any seat), and with what; hit points set to 0
/// by hand name nobody.
#[test]
fn on_death_names_who_hurt_it() {
    let main = r#"
function on_level_start()
  level.state.a = level:spawn("gem", 10, 10)
  level.state.b = level:spawn("gem", 20, 20)
end
function on_death(thing, by, weapon)
  world.state.said = (world.state.said or "") .. (by and ("seat " .. tostring(by.seat)) or "nobody") .. " " .. tostring(weapon) .. ";"
end
function on_tick()
  if world.tick == 2 then level.state.a:hurt(2, player, "laser") end
  if world.tick == 4 then level.state.b.state.hp = 0 end
end
"#;
    let mut g = game("death-by", main);
    steps(&mut g, 6);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.eval("return world.state.said").unwrap(), "\"seat 1 laser;nobody nil;\"");
}

/// A copy of one of the templates in a temporary folder.
fn template_copy(template: &str, name: &str) -> crate::scratch::Scratch {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(entry.file_name()));
            } else {
                std::fs::copy(&path, to.join(entry.file_name())).unwrap();
            }
        }
    }
    let dir = crate::scratch::Scratch::new(&format!("lua-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("templates").join(template), &dir);
    dir
}

/// The jump & run's ready-made rules with a hero for a second seat: it
/// comes in beside the first, runs and jumps by its own keys, takes a hit
/// on its own hearts (thrown back, safe a while), and goes as it leaves;
/// seat 1's hero plays as ever.
#[test]
fn the_jump_and_run_has_a_hero_for_each_seat() {
    let dir = template_copy("platformer", "two-heroes");
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 30);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return world.players[2].kind, world.players[2].state.hp, world.players[2].x == player.x"), "\"hero2\"\t3\ttrue", "c.player.kind's for seat 2");
    let x = said(&mut g, "return world.players[2].x").parse::<f32>().unwrap();
    g.set_seat(2, Some(Input { right: true, ..Input::default() }));
    steps(&mut g, 30);
    let now = said(&mut g, "return world.players[2].x, player.x").split('\t').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
    assert!(now[0] > x + 10.0, "it ran right by its own keys: {x} to {}", now[0]);
    assert_eq!(now[1], x, "seat 1's hero stood still");
    let jump = Input { actions: 1, ..Input::default() };
    g.set_seat(2, Some(jump));
    steps(&mut g, 10);
    assert_eq!(said(&mut g, "return world.players[2].on_ground"), "false", "it jumped");
    g.set_seat(2, Some(Input::default()));
    said(&mut g, "world.players[2]:hurt(1)");
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2].state.hp, world.players[2].state.invulnerable ~= nil, player.state.hp"), "2\ttrue\t3");
    g.set_seat(2, None);
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2], #level:players(), level:count()"), format!("false\t1\t{}", said(&mut g, "return level:count()")));
    assert_eq!(halted(&g), "");
}

/// The shooter plays several (`docs/game-model.md`, "Several players"):
/// each seat's player a figure of `c.player.kind`'s with the run's start of
/// its own (hit points, gun, rounds), moved and throwing by its own keys
/// (its grenade its own), hit on its own; it leaves and its figure goes.
#[test]
fn the_shooter_has_a_soldier_for_each_seat() {
    let dir = template_copy("shooter", "two-soldiers");
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 10);
    assert_eq!(halted(&g), "");
    assert_eq!(said(&mut g, "return player.kind, player.state.hp, player.state.weapon, player.state.inventory.grenade"), "\"hero\"\t5\t\"rifle\"\t5");
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
    let second = "return world.players[2].kind, world.players[2].state.hp, world.players[2].state.weapon, world.players[2].state.inventory.grenade";
    assert_eq!(said(&mut g, second), "\"hero2\"\t5\t\"rifle\"\t5", "the run's start, its own");
    let x = said(&mut g, "return player.x").parse::<f32>().unwrap();
    g.set_seat(2, Some(Input { right: true, ..Input::default() }));
    steps(&mut g, 20);
    let now = said(&mut g, "return world.players[2].x, player.x").split('\t').map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
    assert!(now[0] > x + 10.0, "it ran right by its own keys: {x} to {}", now[0]);
    assert_eq!(now[1], x, "seat 1 stood still");
    let grenade = g.actions().iter().position(|a| *a == "grenade").unwrap();
    g.set_seat(2, Some(Input { actions: 1 << grenade, ..Input::default() }));
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2].state.inventory.grenade, player.state.inventory.grenade"), "4\t5", "its own grenade thrown");
    said(&mut g, "world.players[2]:hurt(1)");
    steps(&mut g, 1);
    assert_eq!(said(&mut g, "return world.players[2].state.hp, world.players[2].state.invulnerable ~= nil, player.state.hp"), "4\ttrue\t5");
    said(&mut g, "world.players[2].state.weapon = 'knife'");
    assert_eq!(said(&mut g, "return world.players[2].state.weapon, player.state.weapon"), "\"knife\"\t\"rifle\"");
    g.set_seat(2, None);
    steps(&mut g, 2);
    assert_eq!(said(&mut g, "return world.players[2], #level:players()"), "false\t1");
    assert_eq!(halted(&g), "");
}

/// The shooter's riflemen go for the nearest player in the fight, and a
/// soldier falling to a player's rounds is that player's kill (the
/// template gives it the points).
#[test]
fn the_shooters_enemies_go_for_the_nearest_player() {
    let dir = template_copy("shooter", "two-soldiers-fight");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap().replacen("c.camera.hold = { 220, 100 }", "c.camera.hold = { 400, 400 }", 1);
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    // Seat 2 up among the riflemen, seat 1 out of their sight.
    said(&mut g, "world.players[2]:place(128, 190)");
    steps(&mut g, 240);
    assert_eq!(halted(&g), "");
    let hp = said(&mut g, "return world.players[2].state.hp, player.state.hp").split('\t').map(|v| v.parse::<i32>().unwrap()).collect::<Vec<_>>();
    assert!(hp[0] < 5, "the nearest player was shot at: {hp:?}");
    assert_eq!(hp[1], 5, "the far one not");
    let fire = g.actions().iter().position(|a| *a == "fire").unwrap();
    for _ in 0..400 {
        g.set_seat(2, Some(Input { up: true, actions: 1 << fire, ..Input::default() }));
        g.step(&Input::default());
    }
    assert_eq!(halted(&g), "");
    let scores = said(&mut g, "return world.players[2] and world.players[2].state.score, player.state.score");
    assert!(scores.ends_with("\t0") && !scores.starts_with("0") && !scores.starts_with("false"), "seat 2's kills its own: {scores}");
}

/// Teams in the shooter (`p.state.team`): players of one team spare each
/// other (the start: co-op), of two they hurt each other, and the hit is
/// the shooter's (`by`); the riflemen's rounds hurt both as before.
#[test]
fn the_shooters_players_of_two_teams_hurt_each_other() {
    let dir = template_copy("shooter", "two-teams");
    let main = std::fs::read_to_string(dir.join("main.lua")).unwrap();
    let main = format!("{main}\nfunction on_hit(thing, by)\n  if thing.seat == 2 and by and by.seat then level.state.hit_by = by.seat end\nend\n");
    std::fs::write(dir.join("main.lua"), main).unwrap();
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 2);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    let fire = g.actions().iter().position(|a| *a == "fire").unwrap();
    // Seat 2 a little ahead of seat 1, who faces it and fires.
    let duel = |g: &mut Session<LuaGame>| {
        said(g, "player:place(128, 40); world.players[2]:place(128, 80)");
        g.step(&Input { up: true, ..Input::default() });
        for _ in 0..90 {
            g.step(&Input { actions: 1 << fire, ..Input::default() });
        }
        assert_eq!(halted(g), "");
        said(g, "return world.players[2].state.hp, level.state.hit_by")
    };
    assert_eq!(duel(&mut g), "5\tnil", "one team: spared");
    said(&mut g, "player.state.team = 'red'; world.players[2].state.team = 'blue'");
    assert_eq!(said(&mut g, "return player.state.team, world.players[2].state.team"), "\"red\"\t\"blue\"");
    let after = duel(&mut g);
    let hp = after.split('\t').next().unwrap().parse::<i32>().unwrap();
    assert!(hp < 5, "two teams: hurt ({after})");
    assert!(after.ends_with("\t1"), "seat 1's hit: {after}");
}

/// A shooter run of two plays back the same, and going back every 8
/// ticks as rollback would.
#[test]
fn a_shooter_run_of_two_replays_the_same() {
    use crate::replay::{Recorder, check, check_going_back};
    let dir = template_copy("shooter", "two-soldiers-replay");
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    let fire = 1 << g.actions().iter().position(|a| *a == "fire").unwrap();
    let grenade = 1 << g.actions().iter().position(|a| *a == "grenade").unwrap();
    for tick in 0..420 {
        let seat = match tick {
            0..30 => None,
            30..200 => Some(Input { up: true, right: tick % 60 < 20, actions: if tick % 9 == 0 { fire } else { 0 }, ..Input::default() }),
            200..205 => Some(Input { actions: grenade, ..Input::default() }),
            _ => Some(Input { up: tick % 40 < 25, left: tick % 80 < 10, actions: fire, ..Input::default() }),
        };
        g.set_seat(2, seat);
        let input = Input { up: tick % 50 < 35, actions: if tick % 13 == 0 { fire } else { 0 }, ..Input::default() };
        g.step(&input);
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "");
    let replay = recorder.replay(&g).unwrap();
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    replay.save(&dir.join("replays/two.toml")).unwrap();
    assert!(check::<LuaGame>(&dir, false).unwrap(), "played back the same");
    assert!(check_going_back::<LuaGame>(&dir, 8).unwrap(), "going back the same");
}

/// The jump & run's heroes are held together as a game's own figures are
/// (`c.camera.hold`), and the camera looks at their middle.
#[test]
fn the_jump_and_runs_heroes_are_held_together() {
    let dir = template_copy("platformer", "two-heroes-held");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap().replacen("c.camera.hold = { 230 }", "c.camera.hold = { 30 }", 1);
    std::fs::write(dir.join("conf.lua"), conf).unwrap();
    let mut g = game_in(&dir);
    let said = |g: &mut Session<LuaGame>, code: &str| g.game.eval(code).unwrap();
    steps(&mut g, 30);
    g.set_seat(2, Some(Input::default()));
    steps(&mut g, 2);
    g.set_seat(2, Some(Input { right: true, ..Input::default() }));
    steps(&mut g, 60);
    assert_eq!(halted(&g), "");
    let apart = said(&mut g, "return world.players[2].x - player.x").parse::<f32>().unwrap();
    assert!((apart - 30.0).abs() < 0.01, "held at the hold: {apart}");
    let (a, b) = (said(&mut g, "return player.x").parse::<f32>().unwrap(), said(&mut g, "return world.players[2].x").parse::<f32>().unwrap());
    let target = g.view_target(1.0);
    assert!((target.x - (a + b) * 0.5).abs() <= 1.0, "the middle: {} between {a} and {b}", target.x);
}

/// What a machine has of the game (`game::Local`): `frame.players` its own
/// seats, a key in braces and `controls` in `on_draw` the first of them's,
/// `world.language` there its own; the simulation goes by seat 1 as
/// before, the game is the same (its hash), and the high score lines are
/// its players'.
#[test]
fn each_machine_draws_its_own_players() {
    let main = r#"
function on_level_start()
  level:fill(0, 0, 0, 63, 63, 0, 1)
end
function on_tick()
  world.state.lang = world.language
end
function on_draw(frame)
  local first = frame.players[1]
  frame:text(#frame.players .. " " .. (first and first.seat or 0) .. " {fire} " .. controls:key("fire") .. " " .. world.language, 1, 1)
end
"#;
    let (mut g, _dir) = two_players("local", main);
    let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.players = 2", "c.players = 2\n  c.languages = { \"en\", \"de\" }");
    std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
    g = game_in(&g.dir.clone());
    let keys = |fire: &str, language: &str| {
        let mut c = crate::controls::Controls::nobody(g.actions());
        c.keys.insert("fire".into(), fire.into());
        c.language = Some(language.into());
        c
    };
    let (one, two) = (keys("X", "en"), keys("K", "de"));
    g.set_controls(one);
    g.set_seat(2, Some(Input::default()));
    g.set_seat_controls(2, two);
    steps(&mut g, 2);
    g.game.eval("player.state.score = 30; world.players[2].state.score = 70").unwrap();
    g.look();
    assert_eq!(first_text(&g), "2 1 X X en", "at one machine every seat, seat 1's keys");
    assert_eq!(g.local_scores(), [(1, 30), (2, 70)]);
    let hash = g.game.state_hash();
    g.set_local(crate::game::Local { seats: Some(vec![2]), language: Some("de".into()) });
    g.look();
    assert_eq!(first_text(&g), "1 2 K K de", "the machine of seat 2");
    assert_eq!(g.local_scores(), [(2, 70)]);
    assert_eq!(g.game.state_hash(), hash, "only the picture");
    steps(&mut g, 1);
    assert_eq!(g.game.eval("return world.state.lang").unwrap(), "\"en\"", "the simulation speaks seat 1's");
    g.set_local(crate::game::Local { seats: Some(Vec::new()), language: None });
    g.look();
    assert_eq!(first_text(&g), "0 0 X X en", "a machine that only watches: seat 1's keys");
}

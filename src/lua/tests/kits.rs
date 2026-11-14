//! The ready-made rules (`c.kit`) under the script: their things, events
//! and figures answering the code.

use super::*;

/// With `c.kit = "shooter"` the kit's things run by themselves; the script
/// counts them, sets their hits, keeps the score, and moves its own kinds
/// (those without a behavior) after its `on_tick`.
#[test]
fn a_lua_game_plays_with_the_shooter_kit() {
    let conf = "  c.things.crate = { model = \"player.vvx\", hp = 3 }\n  c.things.medkit = { behavior = \"pickup\", reach = 5 }";
    let main = r#"
function on_level_start(how)
  level.state.crate = level:spawn("crate", 230, 150)
  level.state.medkit = level:spawn("medkit", 300, 150)
  level.state.x = level.state.crate.x
end

function on_tick()
  level.state.crate:walk({ x = 1, y = 0 }, 30)
  world.state.medkits = level:count("medkit")
  level.state.medkit.state.hp = 7
  level.state.crate.state.hp = level.state.crate.state.hp
  player.state.score = world.tick * 10
end
"#;
    let mut g = game_in(&kit_folder("plays", conf, main));
    steps(&mut g, 60);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "medkits"), format!("{:?}", crate::lua::Value::Int(1)));
    let Some(crate::lua::Value::Table(s)) = g.game.capture().unwrap().level else { panic!("no level state") };
    let x = |key: &str| s.iter().find(|(k, _)| *k == crate::lua::value::Key::Str(key.as_bytes().to_vec())).map(|(_, v)| v.clone());
    let Some(crate::lua::Value::Thing(crate_id)) = x("crate") else { panic!("no crate") };
    let Some(crate::lua::Value::Float(start)) = x("x") else { panic!("no start") };
    let crate_now = g.game.stage.host.get(crate::sim::things::Id::from_raw(crate_id)).unwrap();
    assert!(crate_now.body.pos.x > f64::from_bits(start) as f32 + 10.0, "the crate did not walk: {}", crate_now.body.pos.x);
    assert_eq!(crate_now.hp, 3);
    assert_eq!(g.game.score(), 600, "player.state.score is the kit's score");
    let medkit = g.game.stage.host.ids().into_iter().find_map(|id| g.game.stage.host.get(id).filter(|t| g.game.stage.kinds[t.kind].name == "medkit").map(|t| t.hp));
    assert_eq!(medkit, Some(7), "thing.state.hp is the thing's hits");
}

/// The shooter's things too: a soldier switched off stands where physics
/// leaves him, `thing:launch` moves him and the player; the player's own
/// rules stay on (no thing of a kind yet).
#[test]
fn the_shooters_things_switch_off_and_launch() {
    let main = r#"
function on_level_start(how)
  level.state.man = level:spawn("rifleman", player.x, player.y + 40)
end

function on_tick()
  local man = level.state.man
  if world.tick == 1 then
    man:enable("soldier", false)
  elseif world.tick == 10 then
    world.state.x, world.state.y = man.x, man.y
  elseif world.tick == 60 then
    world.state.stood = man.x == world.state.x and man.y == world.state.y
    man:launch({ x = 1, y = 0 }, 80)
    player:launch({ x = -1, y = 0 }, 80)
    world.state.px = player.x
  elseif world.tick == 64 then
    world.state.moved = man.x > world.state.x and player.x < world.state.px
  end
end
"#;
    let mut g = game_in(&kit_folder("off", "  c.things.rifleman = { behavior = \"soldier\", model = \"player.vvx\", weapon = \"enemy_rifle\" }", main));
    steps(&mut g, 65);
    assert_eq!(halted(&g), "");
    let yes = format!("{:?}", crate::lua::Value::Bool(true));
    for key in ["stood", "moved"] {
        assert_eq!(state(&mut g, key), yes, "{key}");
    }
    let mut g = game_in(&kit_folder("off-player", "", "function on_tick() player:enable('soldier', false) end\n"));
    steps(&mut g, 1);
    assert!(halted(&g).contains("move by their keys"), "{}", halted(&g));
}

/// The kits are building blocks for many games, never one game's rules:
/// outside their tests and their drawing they name no game's weapon or
/// action, keep no hit points, lives, grenades, grace or down time as a
/// number of their own, and turn nothing north or south by themselves. A
/// line that must is marked `// why:` (on it or the line above), saying why.
#[test]
fn the_kits_keep_no_game_in_them() {
    const NAMES: [&str; 10] = ["rifle", "machine_gun", "rockets", "grenade", "knife", "enemy_rifle", "enemy_grenade", "door_gun", "rocket_pods", "fire"];
    const COUNTS: [&str; 6] = ["hp", "max_hp", "lives", "grenades", "grace", "down"];
    const WAYS: [&str; 4] = ["Vec3::Y", "Vec3::NEG_Y", "IVec2::Y", "IVec2::NEG_Y"];
    // A number other than 0 given to one of the counts: `hp: 5`, `lives = 3`.
    let counted = |code: &str| {
        COUNTS.iter().any(|w| {
            code.match_indices(w).any(|(at, _)| {
                let before = code[..at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                let rest = code[at + w.len()..].trim_start();
                let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=').filter(|r| !r.starts_with('=')));
                before && rest.is_some_and(|r| r.trim_start().starts_with(|c: char| c.is_ascii_digit() && c != '0'))
            })
        }) || (code.contains("const ") && ["HP", "LIVES", "GRENADE", "GRACE", "DOWN"].iter().any(|w| code.split("const ").nth(1).is_some_and(|c| c.split(':').next().unwrap_or("").contains(w))))
    };
    let mut found = Vec::new();
    for dir in ["src/shooter", "src/platformer"] {
        for path in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(dir)).unwrap().map(|e| e.unwrap().path()) {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if ["draw.rs", "hud.rs", "tests.rs", "pins.rs"].contains(&name.as_str()) || path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("mod tests") || (n > 0 && lines[n - 1].trim() == "#[cfg(test)]" && line.contains("mod ")) {
                    break;
                }
                let why = line.contains("// why:") || (n > 0 && lines[n - 1].trim_start().starts_with("// why:"));
                let code = line.split("//").next().unwrap_or("");
                // The code without what its strings say (hints may name old keys).
                let bare: String = code.split('"').step_by(2).collect::<Vec<_>>().join("\"\"");
                let named = NAMES.iter().find(|w| code.contains(&format!("\"{w}\"")));
                let way = WAYS.iter().find(|w| bare.match_indices(*w).any(|(at, _)| !bare[at + w.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')));
                let hit = named.map(|w| format!("names {w}")).or(way.map(|w| format!("turns by itself, {w}"))).or(counted(&bare).then(|| "keeps a count".to_string()));
                if let (Some(what), false) = (hit, why) {
                    found.push(format!("{dir}/{name}:{}: {what}: {}", n + 1, line.trim()));
                }
            }
        }
    }
    assert!(found.is_empty(), "a game's rule in a kit (the game's conf.lua or main.lua, or // why: and the reason):\n{}", found.join("\n"));
}

/// A hostage that runs to a place (`flee = "exit"`) needs a level that
/// has one so called.
#[test]
fn a_hostage_runs_to_a_place_some_level_has() {
    let conf = "  c.things.hostage = { behavior = \"captive\", model = \"player.vvx\", flee = \"exit\" }";
    let e = Session::<LuaGame>::load(&kit_folder("flee", conf, "")).map(|_| ()).unwrap_err();
    assert!(format!("{e:#}").contains("c.things.hostage.flee = \"exit\": no level has a place called exit"), "{e:#}");
}

/// A hostage is freed by the game's code: touched within its reach,
/// `thing.state.freed = true` bursts its cage and it runs away from the
/// player (its kind's `flee`, "away" unless it says otherwise).
#[test]
fn the_code_frees_a_hostage() {
    let conf = "  c.things.hostage = { behavior = \"captive\", model = \"player.vvx\", reach = 8 }";
    let main = r#"
function on_level_start(how)
  level.state.h = level:spawn("hostage", player.x, player.y + 5)
end

function on_tick()
  local h = level.state.h
  if world.tick == 30 then
    world.state.touched = #player:touching("hostage")
    world.state.caged = not h.state.freed
    h.state.freed = true
    world.state.y = h.y
  end
  world.state.freed = h.state.freed
  world.state.ran = world.state.y and h.y - world.state.y > 5
end
"#;
    let mut g = game_in(&kit_folder("hostage", conf, main));
    steps(&mut g, 60);
    assert_eq!(halted(&g), "");
    let yes = format!("{:?}", crate::lua::Value::Bool(true));
    for key in ["caged", "freed", "ran"] {
        assert_eq!(state(&mut g, key), yes, "{key}");
    }
    assert_eq!(state(&mut g, "touched"), format!("{:?}", crate::lua::Value::Int(1)));
    assert_eq!(g.game.stage.host.player_number("score"), Some(0), "freeing is worth what the code adds");
}

/// The shooter fights in a level built in code too, without markers (the
/// template's): its riflemen's rounds hurt the player.
#[test]
fn the_shooter_fights_without_markers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/shooter");
    let mut g = game_in(&dir);
    for n in 0..1200 {
        g.step(&Input { up: n < 400, ..Input::default() });
    }
    assert_eq!(halted(&g), "");
    // Down and back up again, a life less (the template's code).
    let lives = g.game.inspect()["player"]["state"]["lives"].as_i64().unwrap();
    assert!(lives < 3, "the player was not hurt: lives {lives}");
}

/// With ready-made rules the game's code steers the camera too (a thing
/// followed, a point looked at, the player again) and turns the player.
#[test]
fn the_kits_camera_and_player_answer_the_code() {
    let main = r#"
function on_level_start(how)
  level.state.box = level:spawn("crate", 230, 150)
end

function on_tick()
  if world.tick == 2 then level.camera:look_at(40, 50) end
  if world.tick == 4 then level.camera:follow(level.state.box) end
  if world.tick == 6 then
    level.camera:follow(player)
    player:face({ x = 1, y = 0 })
    world.state.facing = player.facing.x
  end
end
"#;
    let conf = "  c.things.crate = { model = \"player.vvx\" }";
    let mut g = game_in(&kit_folder("camera", conf, main));
    steps(&mut g, 3);
    assert_eq!(halted(&g), "");
    assert!(g.game.focus().is_none(), "looking at a point follows nothing");
    steps(&mut g, 2);
    let crate_at = g.game.focus().map(|b| b.pos.x);
    assert!(crate_at.is_some_and(|x| (x - 230.0).abs() < 1.0), "following the crate: {crate_at:?}");
    steps(&mut g, 2);
    assert_eq!(g.game.inspect()["state"]["facing"], 1, "the player turned by the code");
    assert!(g.game.focus().is_some_and(|b| (b.pos.x - 230.0).abs() > 1.0), "the player again");
}

/// The game's code attacks too (`thing:attack`): a thing of its own fires
/// at the player, the player knifes a man in front of him; a weapon the
/// game has not is a mistake with the names it has.
#[test]
fn the_code_attacks() {
    let conf = "  c.things.turret = { model = \"player.vvx\" }\n  c.things.man = { behavior = \"soldier\", model = \"player.vvx\", weapon = \"enemy_rifle\", hp = 3, sight = 0, hearing = 0 }";
    let main = r#"
function on_level_start(how)
  level.state.turret = level:spawn("turret", player.x, player.y + 40)
  level.state.man = level:spawn("man", player.x, player.y + 4)
end

function on_tick()
  local turret = level.state.turret
  turret:face({ x = 0, y = -1 })
  if world.tick % 30 == 1 then turret:attack("enemy_rifle", player) end
  player:face({ x = 0, y = 1 })
  if world.tick == 2 then player:attack("knife", level.state.man) end
  world.state.men = level:count("man")
  world.state.hp = player.state.hp
end
"#;
    let mut g = game_in(&kit_folder("attacks", conf, main));
    steps(&mut g, 200);
    assert_eq!(halted(&g), "");
    assert_eq!(g.game.inspect()["state"]["men"], 0, "the knife took the man in front");
    assert!(g.game.inspect()["state"]["hp"].as_i64().unwrap() < 5, "the turret's rounds hurt the player");
    let mut bad = game_in(&kit_folder("attacks-bad", conf, "function on_tick() player:attack(\"bow\") end\n"));
    steps(&mut bad, 2);
    assert!(halted(&bad).contains("thing:attack: no weapon called 'bow'"), "{}", halted(&bad));
}

/// The kit moves its own things: the script may not.
#[test]
fn the_kits_things_move_by_its_rules() {
    let conf = "  c.things.medkit = { behavior = \"pickup\" }";
    let main = "function on_tick() level:things(\"medkit\")[1]:walk({ x = 1, y = 0 }, 30) end\nfunction on_level_start(how) level:spawn(\"medkit\", 300, 150) end\n";
    let mut g = game_in(&kit_folder("kit-moves", conf, main));
    steps(&mut g, 1);
    assert!(halted(&g).contains("thing:walk: a medkit moves by its ready-made rules"), "{}", halted(&g));
}

/// A kit the program does not have is named; without a kit, a kind with a
/// behavior is a mistake.
#[test]
fn an_unknown_kit_and_a_behavior_without_one_are_named() {
    let dir = kit_folder("no-kit", "  c.kit = \"racer\"", "");
    let e = start_error(&dir);
    assert!(e.contains("c.kit = \"racer\": there is no such kit (this program's: \"shooter\", \"platformer\")"), "{e}");
    let dir = folder("behavior", "function on_world_start() end\n");
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("c.things.gem = { hp = 2 }", "c.things.gem = { behavior = \"soldier\" }")).unwrap();
    let e = start_error(&dir);
    assert!(e.contains("c.things.gem.behavior = \"soldier\": no behaviors without a kit"), "{e}");
}

/// A world the script builds (`c.world.size`) with a kit: the shooter's
/// player stands on what `on_level_start` built; the platformer's hero, spawned by
/// the script, stays the one it holds.
#[test]
fn a_kit_stands_on_a_world_the_script_built() {
    let shooter = kit_folder("built", "", "function on_level_start(how)\n  level:fill(0, 0, 0, 511, 511, 3, \"#c2a878\")\nend\n");
    let conf = std::fs::read_to_string(shooter.join("conf.lua")).unwrap().replace("c.world.file = \"world.vvx\"", "c.world.size = { 512, 512, 48 }");
    std::fs::write(shooter.join("conf.lua"), conf).unwrap();
    let g = game_in(&shooter);
    assert_eq!(g.game.focus().map(|b| b.pos.z), Some(4.0), "on the sand, not in it");

    let dir = crate::scratch::Scratch::new("lua-built-platformer");
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3, lives = 3, taken = 0 }\n  c.world.size = { 96, 16, 48 }\n  c.things.hero = { behavior = \"hero\", height = 14 }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_level_start(how)\n  level:fill(0, 0, 0, 95, 15, 3, \"#776655\")\n  level.state.hero = level:spawn(\"hero\", 40, 8)\nend\nfunction on_tick()\n  world.state.alive = level:thing(level.state.hero) ~= nil\nend\n").unwrap();
    let mut g = game_in(&dir);
    steps(&mut g, 30);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "alive"), format!("{:?}", crate::lua::Value::Bool(true)), "the hero the script holds");
    assert_eq!(g.game.focus().map(|b| b.pos.z), Some(4.0), "on the floor it built");
}

/// A behavior switched off (`thing:enable`) leaves the thing to physics
/// and the script: the hero no longer follows the keys and the script
/// walks it up to a wall, which `thing.wall` finds; `thing:launch` throws
/// it back; switched on, it follows the keys again; a walker off stands.
#[test]
fn the_code_switches_rules_off_and_launches() {
    let dir = crate::scratch::Scratch::new("lua-enable");
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3 }\n  c.world.size = { 128, 16, 64 }\n  c.things.hero = { behavior = \"hero\", height = 14 }\n  c.things.slug = { behavior = \"walker\", height = 6 }\nend\n").unwrap();
    let main = r##"
function on_level_start(how)
  level:fill(0, 0, 0, 127, 15, 3, "#776655")
  level:fill(100, 0, 4, 127, 15, 60, "#776655")
  level.state.slug = level:spawn("slug", 20, 8)
  level:spawn("hero", 60, 8)
end

function on_tick()
  local slug = level.state.slug
  if world.tick == 5 then
    player:enable("hero", false)
    slug:enable("walker", false)
  elseif world.tick == 15 then
    world.state.x, world.state.slug_x = player.x, slug.x
  elseif world.tick == 30 then
    world.state.kept_still = player.x == world.state.x
    world.state.slug_still = slug.x == world.state.slug_x
  elseif world.tick > 30 and world.tick < 90 then
    player:walk({ x = 1, y = 0 }, 60)
  elseif world.tick == 90 then
    world.state.wall = player.wall and player.wall.x
    world.state.no_wall = slug.wall == nil
    player:launch({ x = -1, y = 0 }, 100, 12)
    world.state.from = player.x
  elseif world.tick == 96 then
    world.state.thrown = player.x < world.state.from and not player.on_ground
  elseif world.tick == 150 then
    player:enable("hero", true)
    world.state.x = player.x
  elseif world.tick == 180 then
    world.state.ran = player.x > world.state.x
  end
end
"##;
    std::fs::write(dir.join("main.lua"), main).unwrap();
    let mut g = game_in(&dir);
    let right = Input { right: true, ..Input::default() };
    for _ in 0..180 {
        g.step(&right);
    }
    assert_eq!(halted(&g), "");
    let yes = format!("{:?}", crate::lua::Value::Bool(true));
    for key in ["kept_still", "slug_still", "no_wall", "thrown", "ran"] {
        assert_eq!(state(&mut g, key), yes, "{key}");
    }
    assert_eq!(state(&mut g, "wall"), format!("{:?}", crate::lua::Value::Int(1)));

    // Mistakes name what is wrong.
    for (line, says) in [
        ("level.state.slug:enable('hero', false)", "a slug's behavior is \"walker\", not \"hero\""),
        ("level:spawn('slug', 30, 8):launch({ x = 1, y = 0 }, 10)", ""),
        ("player.state.pose = 'dash'", "a hero has no frames called 'dash'"),
    ] {
        std::fs::write(dir.join("main.lua"), main.replace("  local slug = level.state.slug\n", &format!("  local slug = level.state.slug\n  if world.tick == 1 then {line} end\n"))).unwrap();
        let mut g = game_in(&dir);
        steps(&mut g, 2);
        assert!(if says.is_empty() { halted(&g).is_empty() } else { halted(&g).contains(says) }, "{line}: {}", halted(&g));
    }
}

/// The jump & run's rules as the platformer template writes them, played:
/// a gem taken counts, the checkpoint is noted, a fall into the pit costs
/// a life and brings the hero back at the checkpoint with every heart,
/// and with the last life the run is lost.
#[test]
fn the_jump_and_run_rules_as_code() {
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind};
    let template = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/platformer/main.lua")).unwrap();
    let rules = &template[..template.find("-- Every torch").expect("the template's torches")];
    let main = format!("{rules}\nlocal start, fell, died = on_tick, on_fall_out, on_death\nfunction on_fall_out(t) world.state.why = (world.state.why or '') .. 'fell ' .. tostring(t) .. ' ' .. world.tick .. ' ' .. math.floor(t.z) .. ';' fell(t) end\nfunction on_death(t, by) world.state.why = (world.state.why or '') .. 'died ' .. tostring(t) .. ' ' .. world.tick .. ';' died(t, by) end\nfunction on_tick()\n  start()\n  world.state.log = (world.state.log or '') .. (player.state.checkpoint or '-') .. ',' .. player.state.lives .. ',' .. player.state.taken .. ';'\nend\n");
    let dir = levels_folder("jump-and-run-rules", &main, &[("a", &[])]);
    std::fs::write(
        dir.join("conf.lua"),
        "function conf(c)\n  c.kit = \"platformer\"\n  c.state.player = { hp = 3, lives = 3, taken = 0 }\n  c.world.blocks = { file = \"level.toml\", below = 16 }\n  c.physics.gravity = 400\n  c.things.hero = { behavior = \"hero\", height = 6, speed = 60 }\n  c.things.gem = { behavior = \"pickup\", height = 4, family = \"pickups\", props = { worth = 100 }, sound = \"pickup\" }\n  c.things.checkpoint = { behavior = \"prop\", stands = true, height = 8 }\n  c.things.door = { behavior = \"prop\", stands = true, height = 8 }\nend\n",
    )
    .unwrap();
    // A floor of 12 cells, a pit two cells wide in it.
    let mut level = GridLevel::new(4, glam::IVec3::new(12, 1, 2));
    level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    for x in (0..12).filter(|x| !(7..9).contains(x)) {
        let i = level.index(glam::IVec3::new(x, 0, 0)).unwrap();
        level.cells[i] = Cell { block: 1, turn: Default::default() };
    }
    for (kind, x) in [("hero", 2), ("gem", 10), ("checkpoint", 18)] {
        level.things.push(crate::grid::Thing::new(kind, [x, 2, 4]));
    }
    level.save(&dir.join("levels/a.toml")).unwrap();
    // Solid rock blocks.
    let mut rock = crate::blocks::Block::empty(4);
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..4 {
                rock.set(glam::IVec3::new(x, y, z), Some([90, 80, 70])).unwrap();
            }
        }
    }
    rock.save(&crate::blocks::path(&dir, "rock")).unwrap();
    let mut g = game_in(&dir);
    for _ in 0..1200 {
        g.step(&Input { right: true, ..Input::default() });
        if g.game.outcome().is_some() {
            break;
        }
        // (The fade of the end does not run here: the run ends at once.)
    }
    assert_eq!(halted(&g), "");
    let log = g.game.inspect()["state"]["log"].as_str().unwrap_or_default().to_string();
    let why = g.game.inspect()["state"]["why"].as_str().unwrap_or_default().to_string();
    assert!(log.contains("-,3,1;"), "the gem taken: {log} {why}");
    assert!(log.contains("checkpoint_1,3,1;") && log.contains("checkpoint_1,2,1;") && log.contains("checkpoint_1,1,1;"), "lives lost in the pit: {log}");
    assert_eq!(g.game.outcome(), Some(crate::game::Outcome::Lost), "the last life ends the run");
    assert_eq!(g.game.score(), 100);
}

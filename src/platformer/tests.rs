//! The platformer's rules, each tried in a small world built in code.

use glam::IVec3;

use super::*;
use crate::sim::tests::flat_world;

/// Sunny Land's numbers (the kit's), gravity as its demo had it.
fn config(things: &[(&str, &str)]) -> GameConfig {
    let mut cfg = GameConfig { kit: Some("platformer".into()), ..GameConfig::default() };
    cfg.physics.gravity = 500.0;
    cfg.physics.max_fall = 300.0;
    cfg.physics.body_width = 12.0;
    cfg.state.player = toml::from_str("hp = 3").unwrap();
    let mut all = vec![("fox", "behavior = \"hero\"\nheight = 20")];
    all.extend_from_slice(things);
    for (name, toml) in all {
        cfg.things.insert(name.into(), toml::from_str(toml).unwrap());
    }
    cfg
}

/// A game in `world` with things of these kinds at these cells (the
/// platform's model 32 across, 8 deep, 4 high).
fn game(world: World, cfg: &GameConfig, things: &[(&str, IVec3)]) -> Platformer {
    let rules = Rules::from_game(cfg, |file| (file == "platform.vox").then_some(IVec3::new(32, 8, 4))).unwrap();
    let markers: Vec<Marker> = things.iter().map(|&(kind, cell)| Marker { kind: rules.find(kind).unwrap(), cell }).collect();
    Platformer::new(world, cfg, rules, BTreeMap::new(), &markers)
}

fn hero(g: &Platformer) -> &Body {
    &g.things.get(g.hero.unwrap()).unwrap().body
}

fn keys(right: bool, left: bool, jump: bool) -> Input {
    let mut input = Input { right, left, ..Input::default() };
    input.hold(JUMP, jump);
    input
}

const NONE: Input = Input { left: false, right: false, up: false, down: false, actions: 0 };

/// The highest the hero's feet get within `ticks`, holding jump for the
/// first `held` of them.
fn peak(g: &mut Platformer, held: usize, ticks: usize) -> f32 {
    let start = hero(g).pos.z;
    let mut top = start;
    for i in 0..ticks {
        g.run(&keys(false, false, i < held));
        top = top.max(hero(g).pos.z);
    }
    top - start
}

/// Held, a jump goes as high as the kind says; let go early, a third of
/// the way up, it is cut short.
#[test]
fn a_held_jump_goes_its_height_a_short_one_less() {
    let cfg = config(&[]);
    let world = flat_world(IVec3::new(64, 16, 64));
    let mut g = game(world.clone(), &cfg, &[("fox", IVec3::new(32, 8, 1))]);
    g.run(&NONE);
    let full = peak(&mut g, 60, 60);
    assert!((26.0..=30.0).contains(&full), "a held jump rose {full}, the kind says 29");
    let mut g = game(world, &cfg, &[("fox", IVec3::new(32, 8, 1))]);
    g.run(&NONE);
    let short = peak(&mut g, 3, 60);
    assert!(short < full * 0.6, "let go early it rose {short} of {full}");
}

/// A plateau 20 high on the left of a floor; the hero standing near its
/// edge.
fn plateau() -> World {
    let mut world = flat_world(IVec3::new(96, 16, 64));
    for z in 1..20 {
        for y in 0..16 {
            for x in 0..40 {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    world
}

/// The tick (from running right) the hero leaves the plateau's edge,
/// pressing jump `after` ticks later: whether it then rises.
fn jump_after_leaving(after: u32) -> bool {
    let cfg = config(&[]);
    let mut g = game(plateau(), &cfg, &[("fox", IVec3::new(30, 8, 20))]);
    g.run(&NONE);
    assert!(hero(&g).on_ground);
    let mut off = None;
    for tick in 0..120u32 {
        let press = off.is_some_and(|t| tick == t + after);
        g.run(&keys(true, false, press));
        if off.is_none() && !hero(&g).on_ground {
            off = Some(tick);
        }
        if press {
            g.run(&keys(true, false, true));
            return hero(&g).vel.z > 0.0;
        }
    }
    panic!("the hero never left the plateau");
}

/// A jump pressed a moment after running off a ledge still goes (coyote
/// time); later it does not.
#[test]
fn a_jump_just_off_a_ledge_still_goes() {
    let coyote = kinds::HeroRules::default().coyote;
    assert!(jump_after_leaving(1), "one tick off the ledge");
    assert!(jump_after_leaving(coyote - 1), "within the coyote time");
    assert!(!jump_after_leaving(coyote + 4), "well after it");
}

/// Falling onto the floor with jump pressed shortly before landing jumps
/// on landing (the jump buffer); pressed long before, it does not.
#[test]
fn a_jump_pressed_early_waits_for_the_ground() {
    let cfg = config(&[]);
    let world = flat_world(IVec3::new(64, 16, 128));
    let drop = |press_before: u64| -> bool {
        let mut g = game(world.clone(), &cfg, &[("fox", IVec3::new(32, 8, 1))]);
        let id = g.hero.unwrap();
        g.things.get_mut(id).unwrap().body.pos.z = 60.0;
        // A dry run: the tick it lands.
        let mut dry = g.clone();
        let mut land = 0;
        for tick in 1..200u64 {
            dry.run(&NONE);
            if hero(&dry).on_ground {
                land = tick;
                break;
            }
        }
        let mut rose = false;
        for tick in 1..land + 10 {
            g.run(&keys(false, false, tick + press_before >= land));
            rose |= tick > land && hero(&g).vel.z > 0.0;
        }
        rose
    };
    let buffer = kinds::HeroRules::default().buffer as u64;
    assert!(drop(2), "pressed two ticks before landing");
    assert!(drop(buffer), "pressed within the buffer");
    assert!(!drop(buffer + 8), "pressed (and held) long before");
}

/// Hurt (`thing:hurt`), the hero loses a heart and is thrown back away
/// from what hurt it, safe for a moment (a second hurt then counts for
/// nothing); with the last heart it lies still and its death is told. The
/// kit judges no touch: an enemy run into, or landed on, does nothing by
/// itself (the game's code decides, `on_touch`).
#[test]
fn a_hurt_costs_a_heart_the_last_lays_the_hero_down() {
    use crate::lua::host::Host;
    let cfg = config(&[("opossum", "behavior = \"walker\"\nheight = 10\nspeed = 0")]);
    let world = flat_world(IVec3::new(96, 16, 64));
    let mut g = game(world, &cfg, &[("fox", IVec3::new(20, 8, 1)), ("opossum", IVec3::new(48, 8, 1))]);
    let opossum = g.things.iter().find(|t| Some(t.id) != g.hero).unwrap().id;
    for _ in 0..90 {
        g.run(&keys(true, false, false));
    }
    assert_eq!((g.status.hp, g.things.len()), (3, 2), "run into and past it, nothing happens by itself");
    g.sim.events.clear();
    Host::hurt(&mut g, Who::Player, 1, Some(Who::Thing(opossum)), None).unwrap();
    assert_eq!(g.status.hp, 2, "a heart lost");
    let away = (hero(&g).pos.x - g.things.get(opossum).unwrap().body.pos.x).signum();
    assert_eq!(hero(&g).vel.x.signum(), away, "thrown back, away from it");
    Host::hurt(&mut g, Who::Player, 1, None, None).unwrap();
    assert_eq!(g.status.hp, 2, "safe for a moment");
    for (n, _) in std::iter::repeat_n((), 2).enumerate() {
        Host::set_player_number(&mut g, "invulnerable", 0);
        Host::hurt(&mut g, Who::Player, 1, None, None).unwrap();
        assert_eq!(g.status.hp, 1 - n as u32);
    }
    assert!(g.sim.events.iter().any(|e| matches!(e, Event::Death { who: Who::Player, .. })), "its death told");
    let at = hero(&g).pos;
    for _ in 0..30 {
        g.run(&keys(true, false, true));
    }
    assert_eq!(hero(&g).pos.x, at.x, "lying still, whatever is pressed");
    assert_eq!(g.status.outcome, None, "the kit ends no run");
}

/// A platform carries who stands on it, step by step, in whole voxels.
#[test]
fn a_platform_carries_its_rider() {
    let cfg = config(&[("ferry", "behavior = \"platform\"\nmodel = \"platform.vox\"\ntravel = [40, 0]\nspeed = 30\npause = 0")]);
    let mut world = flat_world(IVec3::new(128, 16, 64));
    // No floor under the platform's way: the hero stands on it or falls.
    for y in 0..16 {
        for x in 0..128 {
            world.set_raw(IVec3::new(x, y, 0), 0);
        }
    }
    let mut g = game(world, &cfg, &[("ferry", IVec3::new(40, 8, 20)), ("fox", IVec3::new(40, 8, 40))]);
    let id = g.hero.unwrap();
    g.things.get_mut(id).unwrap().body.pos = Vec3::new(40.5, 8.5, 26.0);
    for _ in 0..10 {
        g.run(&NONE);
    }
    assert!(hero(&g).on_ground, "standing on the platform");
    let start = hero(&g).pos;
    for _ in 0..60 {
        g.run(&NONE);
    }
    let moved = hero(&g).pos - start;
    assert!(hero(&g).on_ground && (moved.x - 30.0).abs() <= 1.0 && moved.z.abs() < 0.01, "carried {moved}");
    assert_eq!(hero(&g).pos.x.fract(), 0.5, "on whole voxels, as the platform");
}

/// With an air jump the hero jumps again in the air, from where it is,
/// once until it lands: highest when it comes at the top of the first;
/// without one a second press does nothing.
#[test]
fn a_double_jump_goes_again_from_the_air() {
    let top = |air_jumps: u32, second_at: usize| {
        let cfg = config(&[("jumper", &format!("behavior = \"hero\"\nheight = 14\nwidth = 6\njump = 22\nair_jumps = {air_jumps}\nair_jump = 12"))]);
        let mut g = game(flat_world(IVec3::new(64, 16, 128)), &cfg, &[("jumper", IVec3::new(30, 8, 1))]);
        let start = hero(&g).pos.z;
        let mut top = start;
        for i in 0..120 {
            // Held for the first jump, let go, pressed again (and a third time).
            let held = i < 17 || (second_at..second_at + 20).contains(&i) || (second_at + 25..second_at + 40).contains(&i);
            g.run(&keys(false, false, held));
            top = top.max(hero(&g).pos.z);
        }
        top - start
    };
    let single = top(0, 20);
    assert!((single - 22.0).abs() < 2.0, "one jump: {single}");
    let at_the_top = top(1, 20);
    assert!(at_the_top > single + 9.0, "again at its top: {at_the_top}");
    let late = top(1, 30);
    assert!(late < at_the_top - 3.0, "late, from lower down: {late} against {at_the_top}");
}

/// A platform coming down onto the hero waits above it (it moves on once
/// the hero is gone); one that crushes hurts it and goes on.
#[test]
fn a_platform_coming_down_waits_or_crushes() {
    let lift = |crushes: bool| config(&[("lift", &format!("behavior = \"platform\"\nmodel = \"platform.vox\"\ntravel = [0, -40]\nspeed = 40\npause = 0\ncrushes = {crushes}"))]);
    for crushes in [false, true] {
        let mut g = game(flat_world(IVec3::new(128, 16, 64)), &lift(crushes), &[("lift", IVec3::new(40, 8, 45)), ("fox", IVec3::new(40, 8, 1))]);
        let lift_z = |g: &Platformer| g.things.iter().find(|t| matches!(t.brain, Brain::Platform(_))).unwrap().body.pos.z;
        for _ in 0..90 {
            g.run(&NONE);
        }
        let head = hero(&g).pos.z + hero(&g).height;
        if crushes {
            assert!(g.status.hp < 3, "crushed: a heart less");
        } else {
            assert_eq!(g.status.hp, 3);
            assert!(lift_z(&g) >= head - 0.01, "waiting above the hero: lift at {}, head at {head}", lift_z(&g));
            // The hero goes; the lift comes down.
            let mut lowest = f32::MAX;
            for _ in 0..90 {
                g.run(&keys(true, false, false));
                lowest = lowest.min(lift_z(&g));
            }
            assert!(lowest < head - 10.0, "on its way once the hero is gone: down to {lowest}");
        }
    }
}

/// A flyer that keeps the way its kind says faces it as soon as the
/// settings change while the game runs, not only once placed anew.
#[test]
fn a_reload_turns_a_bat_to_its_kinds_way() {
    let bat = |facing: &str| config(&[("bat", &format!("behavior = \"flyer\"\nface_hero = false{facing}"))]);
    let mut g = game(flat_world(IVec3::new(64, 16, 64)), &bat(""), &[("bat", IVec3::new(10, 8, 20)), ("fox", IVec3::new(40, 8, 1))]);
    let look = |g: &Platformer| g.things.iter().find(|t| matches!(t.brain, Brain::Flyer { .. })).unwrap().body.look;
    assert_eq!(look(&g), glam::IVec2::X, "towards the middle, as placed");
    g.apply_config(&bat("\nfacing = [0, -1]")).unwrap();
    assert_eq!(look(&g), glam::IVec2::NEG_Y, "to the front at once");
}

/// Climbing a ladder up through a hole one cell wide, a hero that starts
/// off its middle moves to it and comes through, rather than catching its
/// head on the hole's edge.
#[test]
fn a_climber_moves_to_the_ladders_middle() {
    let cfg = config(&[("climber", "behavior = \"hero\"\nheight = 14\nwidth = 6\nclimb = 40\ngrab = 6\npull = 24")]);
    // A ceiling 30 to 40 high with a hole at x 20..30, a ladder up through it.
    let mut world = flat_world(IVec3::new(64, 16, 80));
    for z in 30..40 {
        for y in 0..16 {
            for x in (0..64).filter(|x| !(20..30).contains(x)) {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    let mut cells = crate::grid::CellKinds::new(IVec3::ZERO, IVec3::new(10, 16, 10), IVec3::new(7, 1, 8));
    for z in 0..4 {
        cells.set(IVec3::new(2, 0, z), crate::grid::LADDER);
    }
    world.cells = Some(std::sync::Arc::new(cells));
    let mut g = game(world, &cfg, &[("climber", IVec3::new(21, 8, 1))]);
    let up = Input { up: true, ..Input::default() };
    for _ in 0..60 {
        g.run(&up);
    }
    assert!((hero(&g).pos.x - 25.0).abs() < 0.01 && hero(&g).pos.z > 20.0, "in the ladder's middle, up past the edge: {}", hero(&g).pos);
    // At the top, right steps off onto the floor above.
    for _ in 0..60 {
        g.run(&Input { up: true, right: true, ..Input::default() });
    }
    assert!(hero(&g).pos.z == 40.0 && hero(&g).pos.x > 30.0 && hero(&g).on_ground, "on the floor above: {}", hero(&g).pos);
}

/// Without hands for ledges, a hero climbs a ladder up through a hole to
/// its top and stands there, as on a plank; down takes it down again.
#[test]
fn a_ladders_top_holds_from_above() {
    let cfg = config(&[("climber", "behavior = \"hero\"\nheight = 14\nwidth = 6\nclimb = 40")]);
    let mut world = flat_world(IVec3::new(64, 16, 80));
    for z in 30..40 {
        for y in 0..16 {
            for x in (0..64).filter(|x| !(20..30).contains(x)) {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    let mut cells = crate::grid::CellKinds::new(IVec3::ZERO, IVec3::new(10, 16, 10), IVec3::new(7, 1, 8));
    for z in 0..4 {
        cells.set(IVec3::new(2, 0, z), crate::grid::LADDER);
    }
    world.cells = Some(std::sync::Arc::new(cells));
    let mut g = game(world, &cfg, &[("climber", IVec3::new(25, 8, 1))]);
    let up = Input { up: true, ..Input::default() };
    for _ in 0..90 {
        g.run(&up);
    }
    assert!(hero(&g).pos.z == 40.0 && hero(&g).on_ground, "standing on the ladder's top: {}", hero(&g).pos);
    for _ in 0..30 {
        g.run(&NONE);
    }
    assert!(hero(&g).pos.z == 40.0, "still there without a key: {}", hero(&g).pos);
    for _ in 0..20 {
        g.run(&Input { down: true, ..Input::default() });
    }
    assert!(hero(&g).pos.z < 35.0, "down the ladder again: {}", hero(&g).pos);
}

/// Told a goal (`thing.state.goal`), a platform goes to that end of its
/// travel and stays; told none, it goes to and fro again.
#[test]
fn a_platform_told_a_goal_goes_there_and_stays() {
    use crate::lua::host::{Host, Kept};
    let cfg = config(&[("slab", "behavior = \"platform\"\nmodel = \"platform.vox\"\ntravel = [30, 0]\nspeed = 60\npause = 0")]);
    let mut g = game(flat_world(IVec3::new(128, 16, 64)), &cfg, &[("slab", IVec3::new(40, 8, 20)), ("fox", IVec3::new(100, 8, 1))]);
    let slab = g.things.iter().find(|t| matches!(t.brain, Brain::Platform(_))).unwrap().id;
    let x = |g: &Platformer| g.things.get(slab).unwrap().body.pos.x;
    let home = x(&g);
    g.set_thing_value(slab, "goal", Kept::Text("start".into())).unwrap();
    for _ in 0..90 {
        g.run(&NONE);
    }
    assert_eq!(x(&g), home, "held at its start");
    g.set_thing_value(slab, "goal", Kept::Text("end".into())).unwrap();
    for _ in 0..90 {
        g.run(&NONE);
    }
    assert_eq!((x(&g), g.thing_value(slab, "goal")), (home + 30.0, Some(Kept::Text("end".into()))), "out at its end, staying");
    g.set_thing_value(slab, "goal", Kept::Bool(false)).unwrap();
    g.run(&NONE);
    g.run(&NONE);
    assert!(x(&g) < home + 30.0, "to and fro again: back first");
    assert!(g.set_thing_value(slab, "goal", Kept::Text("middle".into())).unwrap_err().contains("\"start\" or \"end\""));
}

/// The hero walking into a crate pushes it (slower than it runs); a wall
/// stops the crate and the hero with it; over an edge the crate falls,
/// and the hero stands on it as on the floor; a walker turns at it.
#[test]
fn a_crate_is_pushed_falls_and_is_stood_on() {
    let cfg = config(&[("box", "behavior = \"crate\"\nwidth = 10\nheight = 10\npush = 30"), ("snail", "behavior = \"walker\"\nheight = 8\nspeed = 20")]);
    // A floor 20 high with a pit 12 wide and 10 deep at x 60..72.
    let mut world = flat_world(IVec3::new(128, 16, 64));
    for y in 0..16 {
        for x in 0..128 {
            let top = if (60..72).contains(&x) { 10 } else { 20 };
            for z in 1..top {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    let mut g = game(world, &cfg, &[("fox", IVec3::new(20, 8, 20)), ("box", IVec3::new(40, 8, 20))]);
    let crate_id = g.things.iter().find(|t| matches!(t.brain, Brain::Crate)).unwrap().id;
    let at = |g: &Platformer| g.things.get(crate_id).unwrap().body.pos;
    let start = at(&g);
    for _ in 0..40 {
        g.run(&keys(true, false, false));
    }
    assert!(at(&g).x > start.x + 5.0, "pushed along: {} from {}", at(&g).x, start.x);
    assert!(hero(&g).vel.x <= 30.0 + 1e-3, "pushing, the hero goes the crate's speed");
    assert!(hero(&g).pos.x < at(&g).x, "the hero stays behind it");
    for _ in 0..240 {
        g.run(&keys(true, false, false));
    }
    assert!((at(&g).z - 10.0).abs() < 0.01, "fallen into the pit: {}", at(&g));
    // The hero walks over it, the pit filled.
    for _ in 0..240 {
        g.run(&keys(true, false, false));
    }
    assert!(hero(&g).pos.x > 75.0 && hero(&g).pos.z > 19.5, "over the crate in the pit: {}", hero(&g).pos);
    // A crate against the wall stops the hero.
    let mut g = game(flat_world(IVec3::new(128, 16, 64)), &cfg, &[("fox", IVec3::new(20, 8, 1)), ("box", IVec3::new(40, 8, 1))]);
    for z in 1..40 {
        for y in 0..16 {
            for x in 50..54 {
                g.sim.world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    for _ in 0..240 {
        g.run(&keys(true, false, false));
    }
    let c = g.things.get(g.things.iter().find(|t| matches!(t.brain, Brain::Crate)).unwrap().id).unwrap().body.pos;
    assert!((c.x - 45.0).abs() < 0.6 && hero(&g).pos.x < c.x - 10.0, "the wall stops crate and hero: crate {c}, hero {}", hero(&g).pos);
    // A walker turns at a crate.
    let mut g = game(flat_world(IVec3::new(128, 16, 64)), &cfg, &[("snail", IVec3::new(70, 8, 1)), ("box", IVec3::new(40, 8, 1)), ("fox", IVec3::new(120, 8, 1))]);
    let snail = g.things.iter().find(|t| matches!(t.brain, Brain::Walker { .. })).unwrap().id;
    let mut nearest = f32::MAX;
    for _ in 0..600 {
        g.run(&NONE);
        nearest = nearest.min(g.things.get(snail).unwrap().body.pos.x);
    }
    assert!(nearest > 45.0 + 5.0 - 1.0 && nearest < 55.0, "it turned at the crate: nearest {nearest}");
}

/// What falls out of the level is told (`Event::FellOut`), the hero too;
/// the kit takes no life and touches nothing, it keeps the hero falling.
#[test]
fn falling_out_is_told_nothing_more() {
    let cfg = config(&[("gem", "behavior = \"pickup\"\nheight = 6")]);
    // Ground 20 high over open space, as a level with room below it; a pit
    // from x 70 on down to the world's floor.
    let mut world = World::new(IVec3::new(200, 16, 64), flat_world(IVec3::ONE).palette);
    for y in 0..16 {
        for x in 0..70 {
            world.set_raw(IVec3::new(x, y, 19), 1);
        }
    }
    world.stage = Some((IVec3::new(0, 0, 19), IVec3::new(200, 16, 64)));
    let mut g = game(world, &cfg, &[("fox", IVec3::new(10, 8, 20)), ("gem", IVec3::new(30, 8, 20))]);
    let mut told = 0;
    for _ in 0..300 {
        g.run(&keys(true, false, false));
        told += g.sim.events.iter().filter(|e| matches!(e, Event::FellOut { who: Who::Player })).count();
    }
    assert!(told > 0, "the fall told");
    assert_eq!((g.status.score, g.things.len()), (0, 2), "nothing scored, the gem untouched");
}

/// A game kept at a tick and brought back plays on exactly as the first
/// time.
#[test]
fn a_restored_platformer_game_plays_on_the_same() {
    let cfg = config(&[("frog", "behavior = \"hopper\"\nheight = 10"), ("eagle", "behavior = \"flyer\"\nheight = 10")]);
    let mut g = game(plateau(), &cfg, &[("fox", IVec3::new(10, 8, 20)), ("frog", IVec3::new(60, 8, 1)), ("eagle", IVec3::new(80, 8, 30))]);
    let input = |t: usize| keys(t % 90 < 50, t % 90 >= 70, t % 40 < 12);
    for t in 0..100 {
        g.run(&input(t));
    }
    let kept = g.snapshot().unwrap();
    let first: Vec<u64> = (100..300).map(|t| {
        g.run(&input(t));
        g.state_hash()
    }).collect();
    g.restore(&kept);
    let again: Vec<u64> = (100..300).map(|t| {
        g.run(&input(t));
        g.state_hash()
    }).collect();
    assert_eq!(first, again);
}

/// A flyer turns to the hero as it passes beneath; one told not to keeps
/// looking the way it was placed.
#[test]
fn a_flyer_faces_the_hero_unless_told_not_to() {
    let cfg = config(&[("eagle", "behavior = \"flyer\"\nheight = 10\nrange = 2"), ("bat", "behavior = \"flyer\"\nheight = 10\nrange = 2\nface_hero = false")]);
    let mut g = game(flat_world(IVec3::new(128, 16, 64)), &cfg, &[("fox", IVec3::new(10, 8, 1)), ("eagle", IVec3::new(40, 8, 30)), ("bat", IVec3::new(70, 8, 30))]);
    let look = |g: &Platformer, kind: &str| g.things.iter().find(|t| g.rules.kinds[t.kind].name == kind).unwrap().body.look;
    g.run(&NONE);
    let placed = look(&g, "bat");
    assert_eq!(look(&g, "eagle"), IVec2::NEG_X, "the hero still to the left");
    for _ in 0..60 {
        g.run(&keys(true, false, false));
    }
    assert!(hero(&g).pos.x > 80.0, "the hero ran past both");
    assert_eq!((look(&g, "eagle"), look(&g, "bat")), (IVec2::X, placed), "the hero gone by to the right");
}

/// New settings keep the things by their kind's name, whatever the order
/// of the kinds; a kind gone takes its things with it.
#[test]
fn a_reload_keeps_things_by_their_kind() {
    let cfg = config(&[("gem", "behavior = \"pickup\"\nheight = 6"), ("frog", "behavior = \"hopper\"\nheight = 10")]);
    let mut g = game(flat_world(IVec3::new(96, 16, 32)), &cfg, &[("fox", IVec3::new(10, 8, 1)), ("gem", IVec3::new(30, 8, 1)), ("frog", IVec3::new(60, 8, 1))]);
    let mut cfg = config(&[("frog", "behavior = \"hopper\"\nheight = 10\nevery = 30")]);
    cfg.things.insert("aaa".into(), toml::from_str("behavior = \"prop\"").unwrap());
    g.apply_config(&cfg).unwrap();
    let kinds: Vec<&str> = g.things.iter().map(|t| g.rules.kinds[t.kind].name.as_str()).collect();
    assert_eq!(kinds, ["fox", "frog"]);
    assert!(g.hero.is_some());
}

/// A prop that `stands` (a door, a checkpoint, spikes) placed in the air
/// stands on the floor below; one that does not (a torch on a wall) and
/// a pickup stay where they were put.
#[test]
fn what_stands_is_put_on_the_ground() {
    let cfg = config(&[
        ("gem", "behavior = \"pickup\"\nheight = 6"),
        ("door", "behavior = \"prop\"\nstands = true\nheight = 20"),
        ("torch", "behavior = \"prop\"\nheight = 6"),
    ]);
    let things = [("fox", IVec3::new(10, 8, 1)), ("gem", IVec3::new(30, 8, 20)), ("door", IVec3::new(70, 8, 20)), ("torch", IVec3::new(90, 8, 20))];
    let g = game(flat_world(IVec3::new(128, 16, 64)), &cfg, &things);
    let z: Vec<f32> = g.things.iter().map(|t| t.body.pos.z).collect();
    assert_eq!(z, [1.0, 20.0, 1.0, 20.0]);
}


/// Ticks from take-off to the top and from the top back to the floor of a
/// jump held all the way, for a hero of these numbers.
fn jump_times(numbers: &str) -> (u32, u32) {
    let cfg = config(&[("fox", numbers)]);
    let mut g = game(flat_world(IVec3::new(64, 16, 128)), &cfg, &[("fox", IVec3::new(32, 8, 1))]);
    g.run(&NONE);
    let (mut up, mut down, mut rising) = (0, 0, true);
    for i in 0..400 {
        g.run(&keys(false, false, true));
        let b = hero(&g);
        if i > 0 && b.on_ground {
            break;
        }
        if rising && b.vel.z <= 0.0 {
            rising = false;
        }
        if rising { up += 1 } else { down += 1 }
    }
    (up, down)
}

/// Heavier on the way down: the fall back takes less time than the rise;
/// as heavy, they take about the same.
#[test]
fn falling_is_quicker_than_rising() {
    let (up, down) = jump_times("behavior = \"hero\"\nheight = 20\nhang = 1.0");
    assert!((down as f32) < up as f32 * 0.9, "up {up}, down {down}");
    let (up, down) = jump_times("behavior = \"hero\"\nheight = 20\nhang = 1.0\nfall_gravity = 1.0");
    assert!(up.abs_diff(down) <= 2, "up {up}, down {down}");
}

/// With jump held the top of the jump is lighter: the hero stays up longer.
#[test]
fn holding_jump_hangs_at_the_top() {
    let plain = jump_times("behavior = \"hero\"\nheight = 20\nhang = 1.0\nfall_gravity = 1.0");
    let hung = jump_times("behavior = \"hero\"\nheight = 20\nhang = 0.5\nfall_gravity = 1.0");
    assert!(hung.0 + hung.1 >= plain.0 + plain.1 + 3, "{hung:?} against {plain:?}");
}

/// A head clipping a ceiling's edge by a voxel or two slips past it; one
/// under the ceiling squarely, or with `corner` 0, bumps.
#[test]
fn a_head_clipping_a_ceiling_edge_slips_past() {
    let mut world = flat_world(IVec3::new(96, 16, 64));
    for z in 24..26 {
        for y in 0..16 {
            for x in 0..40 {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    let peak = |corner: u32, x: i32| {
        let cfg = config(&[("fox", &*format!("behavior = \"hero\"\nheight = 20\ncorner = {corner}"))]);
        let mut g = game(world.clone(), &cfg, &[("fox", IVec3::new(x, 8, 1))]);
        g.run(&NONE);
        let mut top: f32 = 0.0;
        for _ in 0..60 {
            g.run(&keys(false, false, true));
            top = top.max(hero(&g).pos.z);
        }
        top
    };
    // The body reaches 6 voxels each way: at x 44 it clips the ceiling by 1.5.
    assert!(peak(4, 44) > 20.0, "slips past: {}", peak(4, 44));
    assert!(peak(0, 44) < 6.0, "bumps without corner: {}", peak(0, 44));
    assert!(peak(4, 30) < 6.0, "under it squarely: {}", peak(4, 30));
}

/// The view leads where the hero runs and swings round when it turns, a
/// little each tick.
#[test]
fn the_view_leads_where_the_hero_runs() {
    let cfg = config(&[]);
    let mut g = game(flat_world(IVec3::new(400, 16, 32)), &cfg, &[("fox", IVec3::new(200, 8, 1))]);
    for _ in 0..90 {
        g.run(&keys(true, false, false));
    }
    assert_eq!(g.sim.lead.x, 40.0);
    let mut last = g.sim.lead.x;
    for _ in 0..90 {
        g.run(&keys(false, true, false));
        assert!((g.sim.lead.x - last).abs() <= 1.5 + 1e-4, "it eases round");
        last = g.sim.lead.x;
    }
    assert_eq!(g.sim.lead.x, -40.0);
}

/// A wall `high` voxels tall from x 50 on, the floor before it: a ledge
/// too high to jump onto (the fox's feet get to 29 above the floor).
fn ledge(high: i32) -> World {
    let mut world = flat_world(IVec3::new(96, 16, 64));
    for z in 1..=high {
        for y in 0..16 {
            for x in 50..96 {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    world
}

/// Runs right and jumps against the wall, then (`up`) presses up: where
/// the hero ends and what it showed on the way.
fn at_the_wall(high: i32, grab: f32, up: bool) -> (Vec3, Vec<&'static str>) {
    let cfg = config(&[("fox", &*format!("behavior = \"hero\"\nheight = 20\ngrab = {grab}\npull = 20"))]);
    let mut g = game(ledge(high), &cfg, &[("fox", IVec3::new(30, 8, 1))]);
    let mut shown = Vec::new();
    for i in 0..150 {
        let input = match i {
            0..40 => keys(true, false, (20..40).contains(&i)),
            _ => Input { up: up && i >= 90, ..NONE },
        };
        g.run(&input);
        let Brain::Hero(h) = &g.things.get(g.hero.unwrap()).unwrap().brain else { unreachable!() };
        if shown.last() != Some(&h.anim.name()) {
            shown.push(h.anim.name());
        }
    }
    (hero(&g).pos, shown)
}

/// An edge above the head is caught: the hero hangs from it until up
/// pulls it onto the ledge; one it nearly made it pulls itself onto at
/// once; without `grab` it catches none.
#[test]
fn a_ledge_too_high_is_caught_and_climbed() {
    // The fox's head gets to 49: a top at 53 it hangs from, one at 37 it
    // nearly made.
    let (end, shown) = at_the_wall(52, 8.0, true);
    assert_eq!(end.z, 53.0, "on top of the wall: {end:?} after {shown:?}");
    assert!(end.x > 50.0 && shown.contains(&"hang") && shown.contains(&"pull"), "{end:?} {shown:?}");
    let (end, shown) = at_the_wall(52, 8.0, false);
    assert!(end.z == 53.0 - 22.0 && shown.last() == Some(&"hang"), "without up it keeps hanging, its head 2 under the top: {end:?} {shown:?}");
    let (end, shown) = at_the_wall(36, 8.0, false);
    assert_eq!(end.z, 37.0, "an edge below the head: up at once: {end:?} {shown:?}");
    assert!(!shown.contains(&"hang") && shown.contains(&"pull"), "{shown:?}");
    let (end, shown) = at_the_wall(36, 0.0, true);
    assert_eq!(end.z, 1.0, "without grab it stays below: {end:?} {shown:?}");
}

/// Hanging, down lets go: the hero drops to the floor.
#[test]
fn down_lets_go_of_the_edge() {
    let cfg = config(&[("fox", "behavior = \"hero\"\nheight = 20\ngrab = 8")]);
    let mut g = game(ledge(52), &cfg, &[("fox", IVec3::new(30, 8, 1))]);
    for i in 0..60 {
        g.run(&keys(true, false, (20..40).contains(&i)));
    }
    assert!(matches!(&g.things.get(g.hero.unwrap()).unwrap().brain, Brain::Hero(h) if h.edge.is_some()), "hangs");
    for _ in 0..60 {
        g.run(&Input { down: true, ..NONE });
    }
    assert!(hero(&g).on_ground && hero(&g).pos.z == 1.0, "back on the floor: {:?}", hero(&g).pos);
}

/// A kind's frames are named for what its behavior plays: a prop's
/// `idle` goes round (Voxkeep's torches burn), and it shows the pose the
/// game sets (`thing.state.pose`); any figure shows a pose the game sets
/// over its rules' own animation, from when it was set, until set to nil;
/// every animation of the hero has its name.
#[test]
fn frames_are_named_for_what_a_behavior_plays() {
    let cfg = config(&[("torch", "behavior = \"prop\"\nframes = { idle = [0, 1] }\nfps = 6")]);
    let world = flat_world(IVec3::new(64, 16, 32));
    let mut g = game(world, &cfg, &[("torch", IVec3::new(32, 8, 1))]);
    let torch = g.things.iter().next().unwrap().id;
    let mut shown = std::collections::BTreeSet::new();
    for _ in 0..30 {
        g.run(&NONE);
        shown.insert(g.frame(g.things.get(torch).unwrap()));
    }
    assert_eq!(shown.into_iter().collect::<Vec<_>>(), [0, 1], "the torch burns");
    let door = config(&[("door", "behavior = \"prop\"\nframes = { shut = [0, 0], open = [1, 1] }")]);
    let mut g = game(flat_world(IVec3::new(64, 16, 32)), &door, &[("door", IVec3::new(32, 8, 1))]);
    let id = g.things.iter().next().unwrap().id;
    assert_eq!(g.frame(g.things.get(id).unwrap()), 0, "shut at first");
    crate::lua::host::Host::set_thing_value(&mut g, id, "pose", crate::lua::host::Kept::Text("open".into())).unwrap();
    assert_eq!(g.frame(g.things.get(id).unwrap()), 1, "open once the game says so");
    assert!(crate::lua::host::Host::set_thing_value(&mut g, id, "pose", crate::lua::host::Kept::Text("ajar".into())).unwrap_err().contains("no frames called 'ajar'"));
    let posed = config(&[("slime", "behavior = \"walker\"\nframes = { walk = [0, 1], burn = [2, 3] }\nfps = 60")]);
    let mut g = game(flat_world(IVec3::new(64, 16, 32)), &posed, &[("slime", IVec3::new(32, 8, 1))]);
    let slime = g.things.iter().find(|t| matches!(t.brain, Brain::Walker { .. })).unwrap().id;
    let shown = |g: &Platformer| g.frame(g.things.get(slime).unwrap());
    g.run(&NONE);
    assert!(shown(&g) <= 1, "walking");
    crate::lua::host::Host::set_thing_value(&mut g, slime, "pose", crate::lua::host::Kept::Text("burn".into())).unwrap();
    assert_eq!(shown(&g), 2, "the pose from its first frame");
    g.run(&NONE);
    assert_eq!(shown(&g), 3);
    crate::lua::host::Host::set_thing_value(&mut g, slime, "pose", crate::lua::host::Kept::Text("burn".into())).unwrap();
    assert_eq!(shown(&g), 3, "set again it plays on");
    assert_eq!(crate::lua::host::Host::thing_value(&g, slime, "pose"), Some(crate::lua::host::Kept::Text("burn".into())));
    crate::lua::host::Host::set_thing_value(&mut g, slime, "pose", crate::lua::host::Kept::Bool(false)).unwrap();
    assert!(shown(&g) <= 1, "nil: walking again");
    assert!(crate::lua::host::Host::set_thing_value(&mut g, slime, "pose", crate::lua::host::Kept::Text("melt".into())).unwrap_err().contains("no frames called 'melt'"));
    // (A new animation of the hero fails to compile here until named.)
    let name = |a: Anim| match a {
        Anim::Idle | Anim::Run | Anim::Jump | Anim::Fall | Anim::Crouch | Anim::Hurt | Anim::Climb | Anim::Hang | Anim::Pull => a.name(),
    };
    for a in [Anim::Idle, Anim::Run, Anim::Jump, Anim::Fall, Anim::Crouch, Anim::Hurt, Anim::Climb, Anim::Hang, Anim::Pull] {
        assert!(HERO_PLAYS.contains(&name(a)), "{}", a.name());
    }
}

/// Each set of a kind's frames may go at its own speed (a slow breath
/// beside a quick run) and play once, its last frame held (a jump, a pull):
/// `{ first = 0, last = 1, fps = 2, loop = false }`; `{ 0, 1 }` goes round
/// at the kind's `fps`. A prop's pose plays from when the game set it. A
/// kind's effect may say its speed too.
#[test]
fn each_set_of_frames_has_its_own_speed() {
    let cfg = config(&[("torch", "behavior = \"prop\"\nfps = 60\nframes = { idle = { first = 0, last = 1, fps = 2 }, quick = [2, 3], flare = { first = 4, last = 6, fps = 60, loop = false } }")]);
    let mut g = game(flat_world(IVec3::new(64, 16, 32)), &cfg, &[("torch", IVec3::new(32, 8, 1))]);
    let torch = g.things.iter().next().unwrap().id;
    let shown = |g: &Platformer| g.frame(g.things.get(torch).unwrap());
    let mut seen = Vec::new();
    for _ in 0..60 {
        seen.push(shown(&g));
        g.run(&NONE);
    }
    assert!(seen[..29].iter().all(|&f| f == 0) && seen[31..59].iter().all(|&f| f == 1), "two frames a second: {seen:?}");
    crate::lua::host::Host::set_thing_value(&mut g, torch, "pose", crate::lua::host::Kept::Text("quick".into())).unwrap();
    let mut seen = Vec::new();
    for _ in 0..4 {
        g.run(&NONE);
        seen.push(shown(&g));
    }
    assert_eq!(seen, [3, 2, 3, 2], "the kind's speed, round and round");
    crate::lua::host::Host::set_thing_value(&mut g, torch, "pose", crate::lua::host::Kept::Text("flare".into())).unwrap();
    let mut seen = Vec::new();
    for _ in 0..6 {
        g.run(&NONE);
        seen.push(shown(&g));
    }
    assert_eq!(seen, [5, 6, 6, 6, 6, 6], "once, the last frame held");
    for (frames, says) in [
        ("{ idle = [0] }", "frames.idle: the first and the last frame"),
        ("{ idle = { first = 0 } }", "frames.idle (first, last, fps, stride, loop)"),
        ("{ idle = { first = 0, last = 1, speed = 2 } }", "frames.idle (first, last, fps, stride, loop)"),
    ] {
        let cfg = config(&[("torch", &format!("behavior = \"prop\"\nframes = {frames}"))]);
        let err = format!("{:#}", Rules::from_game(&cfg, |_| None).unwrap_err());
        assert!(err.contains(says), "{frames}: {err}");
    }
    let def = |effect: &str| {
        let cfg = config(&[("slime", &format!("behavior = \"walker\"\neffect = {effect}"))]);
        crate::sim::things::defs(&cfg, &super::BEHAVIORS).map(|d| d.into_iter().find(|d| d.name == "slime").unwrap().effect.unwrap())
    };
    assert_eq!(def("\"poof.vox\"").unwrap().fps, crate::sim::things::EFFECT_FPS);
    assert_eq!(def("{ model = \"poof.vox\", fps = 20 }").unwrap(), crate::sim::things::Effect { model: "poof.vox".into(), fps: 20.0 });
    assert!(format!("{:#}", def("{ model = \"poof.vox\", speed = 20 }").unwrap_err()).contains("c.things.slime.effect: a model,"));
    assert!(def("\"burst\"").unwrap().bursts(), "it bursts into its voxels");
    assert!(!def("\"poof.vox\"").unwrap().bursts());
}

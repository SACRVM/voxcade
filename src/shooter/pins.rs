//! Pins: runs of the shooter along paths More Blood's replays do not take
//! (the kit's own numbers with a gunship firing nose-on, aim assist off
//! with one side-on, two rounds on one man in a tick, two pickups at once),
//! with their state every second as it was before step 3 (things and
//! parts). Rebuilding the shooter must not change a single number here; a
//! change meant to alter the game says so and pins them anew.

use glam::{IVec3, Vec3};

use super::*;
use crate::config::GameConfig;
use crate::replay::parse_script;
use crate::sim::tests::flat_world;
use crate::world::World;

/// A flat field 160 wide and 400 long, a stone wall across its left half
/// to hide behind and blast through, and a stone tower to bring down.
fn field() -> World {
    let mut world = flat_world(IVec3::new(160, 400, 48));
    world.palette[2] = [120, 118, 112, 255];
    let mut fill = |lo: IVec3, hi: IVec3| {
        for z in lo.z..hi.z {
            for y in lo.y..hi.y {
                for x in lo.x..hi.x {
                    world.set_raw(IVec3::new(x, y, z), 2);
                }
            }
        }
    };
    fill(IVec3::new(30, 150, 1), IVec3::new(76, 154, 12));
    fill(IVec3::new(110, 180, 1), IVec3::new(113, 183, 20));
    fill(IVec3::new(106, 176, 20), IVec3::new(117, 187, 21));
    world
}

/// Every kind of thing More Blood places, spread over the field.
fn everything() -> Vec<Marker> {
    let cfg = tests::kit();
    let at = |kind: &str, x: i32, y: i32, z: i32| tests::marker(&cfg, kind, IVec3::new(x, y, z));
    vec![
        at("machine_gun", 80, 40, 1),
        at("grenades", 84, 44, 1),
        at("hostage", 92, 70, 1),
        at("health", 78, 90, 1),
        at("checkpoint", 80, 100, 1),
        at("rifleman", 70, 120, 1),
        at("rifleman", 96, 130, 1),
        at("grenadier", 60, 170, 1),
        at("guard", 111, 181, 21),
        at("hut", 100, 220, 1),
        at("rockets", 80, 240, 1),
        at("hostage", 50, 260, 1),
        at("gunship", 80, 320, 1),
        at("goal", 80, 390, 1),
    ]
}

/// Plays `script` (shot's format) from the start with the game's rules
/// after every tick (`Classic`), the state every second.
fn run(sim: Shooter, script: &str) -> Vec<u64> {
    let cfg = tests::kit();
    run_with(sim, script, Classic::new(&cfg))
}

/// The same with these rules of the game's.
fn run_with(mut sim: Shooter, script: &str, mut rules: Classic) -> Vec<u64> {
    rules.respawn = sim.player.body.pos;
    let mut hashes = Vec::new();
    let mut tick = 0;
    for (input, ticks) in parse_script(script, &ACTIONS).unwrap() {
        for _ in 0..ticks {
            sim.run(&input);
            rules.after(&mut sim);
            tick += 1;
            if tick % 60 == 0 {
                hashes.push(kit_hash(&sim));
            }
        }
    }
    hashes
}

/// The kit's state as one number, without the things a Lua game's script
/// moves: the checkpoints and the goal, kinds without a behaviour since
/// stage 7, stand still here (nothing moves them without a script) and
/// were no things of the hash before.
fn kit_hash(sim: &Shooter) -> u64 {
    let mut kit = sim.clone();
    kit.things.retain(|t| !matches!(t.brain, Brain::Script(_)));
    kit.state_hash()
}

/// The rules the kit kept before the game's code decided (stage 7): what
/// More Blood's `main.lua` decides now, in Rust and in the kit's old order,
/// so the pins hold the kit's motion and fighting as they were: points
/// for a kill (more for the knife) and for the gunship, the pickups (by
/// the hips within reach, the later first), checkpoints past their line,
/// coming back after going down, the last life, and the helipad once no
/// gunship is left.
struct Classic {
    props: Vec<toml::Table>,
    respawn: Vec3,
    down_at: Option<u64>,
    /// The last tick played (a hit stop plays none: nothing to decide).
    tick: u64,
    /// The lives left (the game's since stage D: More Blood starts with 3).
    lives: i32,
}

/// More Blood's full hit points.
const MAX_HP: i32 = 5;

impl Classic {
    fn new(cfg: &GameConfig) -> Self {
        let props = crate::sim::things::defs(cfg, &BEHAVIOURS).unwrap().into_iter().map(|d| d.props).collect();
        Self { props, respawn: Vec3::ZERO, down_at: None, tick: 0, lives: 3 }
    }

    fn number(&self, kind: usize, key: &str) -> i64 {
        self.props[kind].get(key).and_then(toml::Value::as_integer).unwrap_or(0)
    }

    fn after(&mut self, sim: &mut Shooter) {
        if sim.sim.tick == self.tick {
            return;
        }
        self.tick = sim.sim.tick;
        let events = std::mem::take(&mut sim.sim.events);
        for e in &events {
            match e {
                crate::sim::Event::Death { who: crate::sim::Who::Player, .. } => self.down_at = Some(sim.sim.tick),
                crate::sim::Event::Death { who: crate::sim::Who::Thing(_), kind, weapon, .. } => match sim.rules.kinds[*kind].rules {
                    KindRules::Soldier(_) => {
                        let key = if weapon.as_deref() == Some("knife") { "knife_worth" } else { "worth" };
                        sim.status.score += self.number(*kind, key) as u32;
                    }
                    KindRules::Gunship(_) => sim.status.score += self.number(*kind, "worth") as u32,
                    _ => {}
                },
                _ => {}
            }
        }
        sim.sim.events = events;
        // A gun whose rounds ran out: the rifle in hand again (More Blood's
        // main.lua since stage D).
        if let Some(w) = sim.status.weapon
            && sim.rules.weapon_names[w] != "rifle"
            && sim.status.stock.get(&sim.rules.weapon_names[w]).is_none_or(|&n| n == 0)
        {
            sim.status.weapon = sim.rules.weapon_names.iter().position(|n| n == "rifle");
        }
        if let Some(at) = self.down_at {
            if sim.sim.tick - at >= 149 {
                self.down_at = None;
                self.lives -= 1;
                if self.lives <= 0 {
                    sim.status.phase = Phase::GameOver;
                    let pos = sim.player.body.pos;
                    sim.sound(Sfx::GameOver, pos, 1.0);
                } else {
                    sim.stand_afresh(self.respawn);
                    sim.player.body.face(Vec3::Y);
                    sim.player.invulnerable = 150;
                    sim.status.hp = MAX_HP;
                    sim.back_to_life();
                    sim.clear_rounds(Some("enemies"));
                }
            }
            return;
        }
        if sim.status.phase != Phase::Playing {
            return;
        }
        let (pos, height) = (sim.player.body.pos, sim.player.body.height);
        let rules = Arc::clone(&sim.rules);
        let caged = sim.things.ids(|t| matches!(&t.brain, Brain::Captive(h) if !h.freed) && (t.body.pos - pos).truncate().length() < rules.captive(t.kind).reach);
        for id in caged {
            let kind = sim.things.get(id).map(|t| t.kind).unwrap();
            sim.free_captive(id);
            sim.status.score += self.number(kind, "worth") as u32;
        }
        let taken = sim.things.ids(|t| matches!(t.brain, Brain::Pickup) && (t.body.pos + 0.5).distance(pos + Vec3::Z * height * 0.2) < rules.pickup(t.kind).reach);
        for id in taken.into_iter().rev() {
            let Some(item) = sim.things.remove(id) else { continue };
            sim.sound(Sfx::Pickup, pos, 1.0);
            let p = &self.props[item.kind];
            let s = &mut sim.status;
            // As More Blood's main.lua takes them: rounds and grenades into
            // the inventory, no more than an item's stack.
            let take = |s: &mut super::Status, item: &str, n: u32| {
                let stack = sim.sim.cfg.items.get(item).map_or(u32::MAX, |i| i.stack);
                let have = s.stock.entry(item.to_string()).or_default();
                *have = (*have + n).min(stack).max(*have);
            };
            if let Some(gun) = p.get("weapon").and_then(toml::Value::as_str) {
                s.weapon = sim.rules.weapon_names.iter().position(|w| w == gun);
                take(s, gun, self.number(item.kind, "ammo") as u32);
            }
            let grenades = self.number(item.kind, "grenades") as u32;
            if grenades > 0 {
                take(s, "grenade", grenades);
            }
            let heal = self.number(item.kind, "heal") as i32;
            if heal != 0 {
                s.hp = (s.hp + heal).min(MAX_HP);
            }
            s.score += self.number(item.kind, "worth") as u32;
        }
        let checkpoint = rules.find("checkpoint");
        let before = self.respawn;
        for t in sim.things.iter().filter(|t| Some(t.kind) == checkpoint) {
            let c = Vec3::new(t.body.pos.x, t.body.pos.y, t.body.pos.z);
            if pos.y >= c.y && c.y > self.respawn.y {
                self.respawn = c;
            }
        }
        if self.respawn != before {
            sim.sound(Sfx::Checkpoint, pos, 1.0);
        }
        gunships_guard(sim, &self.props);
    }
}

const FIGHT: &str = "up:50;up+fire:60;fire:30;right:12;up:40;up+fire:60;grenade:1;none:40;left:10;up+jump:20;up+fire:90;\
    grenade:1;none:30;strafe+left+fire:40;up+fire:120;right:20;up:60;up+fire:200;grenade:1;none:20;grenade:1;\
    up+fire:200;left+fire:60;right+fire:120;up+fire:240;down+fire:120;up:180;up+right:60;up:120;\
    up+fire:400;left+fire:80;right+fire:80;up+fire:400;strafe+right+fire:60;up+fire:300;up:240;up+left:60;up:300";

/// The state every second of `the_kit_as_it_comes` and of
/// `without_aim_assist_and_a_gunship_side_on`, from the shooter before step 3.
const KIT: [u64; 65] = [
    0xcbfc59325a631418, 0x10e9904bc9365b98, 0xe4a0031d10e67fef, 0xbfe16d0503d21b13,
    0x553b1a61ed4b4579, 0xb4b2e8a7415524f2, 0xa575e75a84d58439, 0xf8c9f3c8436af733,
    0xe0680e7c2820041b, 0xa3bd92a867d91577, 0xc1f22e6f533e659a, 0xe53cfb41f9d6d9a1,
    0x1d9f3a7ec182436c, 0x59620e50da8cfb66, 0xeea8bf45ce60303b, 0xe96b25231cd399a2,
    0xed52d227028c94d9, 0x95c03a4f0277b8ed, 0x0dea8959d080a3be, 0x5efbd6a1063e881d,
    0xe1dbd1fbe41c38d4, 0xcf22daba896d16ee, 0xa20a45dc0c5c202a, 0x789e7441695ea76f,
    0xaf153b881c738b51, 0x0346563cb5571e27, 0x2dec467fee41e30c, 0x0a995537c9ce7236,
    0x3cd34353558212fe, 0x9490701455ea1ffa, 0x20ddc551e3b25d33, 0xa90b1aea4675c26d,
    0xf75c86853d34d232, 0x431ab19941ce18d8, 0x05c6912209e78791, 0x226adc2d4a0aaf88,
    0xd0e1d60212dd58fe, 0x57e514d56f8fd3be, 0xff566285ae979f34, 0x05912de807514f1b,
    0xb336f64643c72821, 0x438ebe082dc9f134, 0x229c8c2f380ae892, 0xa65191479b3ad09d,
    0x7bea365a48a277cc, 0xdba28962b469070a, 0xc100546ea22bbd92, 0x1a30c94bd3b997fb,
    0xb24e7d9870c06dcf, 0x84e3271247495867, 0x64ceef9fec73feb7, 0x967f26d9b7ac7892,
    0xe14ee01b451a9b5d, 0x4f2d87ac3cf695de, 0x652ce4404fdf488b, 0x127beb3020eeac5c,
    0x92e4a53780b47904, 0x56554222c763cdb9, 0xcb11ac20ec4c9065, 0x2b20dce1562fab45,
    0x4b347b674cae7f2e, 0xf2974d32f2695370, 0x3fc6a2cf173b4915, 0xd6ff5125ba067eb7,
    0xffd81973f6a19f2d,
];

const SIDE_ON: [u64; 65] = [
    0x2481318295aefd62, 0x2bf534fef82f0fb4, 0x1d286967de8e0cd1, 0x0b6b604d64bd4c52,
    0x7862a5fbf08ea9c5, 0x3a70c27229054368, 0x3a540237e8b1010f, 0xeac3675d8c95d865,
    0xf9a118e9bf28f577, 0x5cdb4b52c4d5d46f, 0x1b259f7df670cc5d, 0xf6b28ad3fcb822d3,
    0x5223bab1ec04559e, 0x3123af4a5dd30e4f, 0x9ba3fcb750816e1a, 0x38e9f69cc3e97122,
    0xebe84fc64edcf066, 0x0bc7f0cb908decbc, 0x5acbb3fec7c62380, 0xb07c7574f3ca9368,
    0x19692b983e3f78f3, 0x2ce7eec302158bda, 0xea005d2ed6c669ae, 0x8c5aac25128a0ffb,
    0xe7e7f16ec2289f91, 0xdc4f1157dbcc17ca, 0x137f4b7094093593, 0x3952d6d4daa37b45,
    0x1fc115121b05601c, 0x3bd7c26152e5c447, 0x0bbde168f934588a, 0x5d50413d06f91746,
    0x0a861673c8480ed8, 0x0b1ca63136037884, 0x9d078632aee6e469, 0xd025b523a467b802,
    0x7d6e52ff85e42290, 0x6d7808da7ef3b2c8, 0xef0e9b396258190a, 0xf78d021f1c384f5c,
    0x91c7ff77a613f6ef, 0xf3412b9e0c1ad158, 0x0f2a24165f3a76cb, 0x94f66aef421b9f8f,
    0x39d9eb769839f080, 0xfe03604abebc8fd3, 0xa5c53076cb59032e, 0x0dc6aa1930e2dfa5,
    0x6fa2b70b450bb9fc, 0xbf05b574d39d1463, 0x973a1ff99a89a272, 0xccd3dccef48945f1,
    0x9a4d9cabe72cd4eb, 0x73b567b544f6febc, 0x14bd37531b5e9ccc, 0x049559fe3a7fbee9,
    0x6aa364116f11f815, 0x14f1e10b0c241dd6, 0xde5db5497e8a0428, 0x641c8bb537926459,
    0xd7964beadc22a541, 0xb557290c679fca84, 0xb1d8c11b163a94e5, 0x232b395739557af4,
    0x2ac99442aaf7d8c6,
];

/// The shooter on the field with everything; the game's rules decide
/// (`Classic`).
fn shooter(change: impl FnOnce(&mut GameConfig)) -> Shooter {
    let mut cfg = tests::kit();
    cfg.player.spawn = [80.0, 16.0];
    // More Blood's ways, which the kit's huts and hostages had before
    // stage D: a hut sends only while north of the player, a hostage
    // freed runs south.
    tests::set(&mut cfg, "hut", "ahead", vec![0, 1]);
    tests::set(&mut cfg, "hostage", "flee", vec![0, -1]);
    change(&mut cfg);
    Shooter::new(field(), &cfg, tests::shapes(), &everything())
}

/// The rule the pins were made with (More Blood's before the events, which
/// its `main.lua` now keeps with `on_death` and `on_touch`): in the goal's
/// reach the run is won once no gunship is left, after the tick's rules.
fn gunships_guard(sim: &mut Shooter, props: &[toml::Table]) {
    if sim.status.phase != Phase::Playing || sim.gunships().next().is_some() {
        return;
    }
    let pos = sim.player.body.pos;
    let Some(kind) = sim.rules.find("goal") else { return };
    let goal = sim.things.iter().rev().find(|t| t.kind == kind).map(|t| t.body.pos);
    if let Some(at) = goal
        && (at - pos).truncate().length() < sim.rules.kinds[kind].reach.unwrap_or(0.0)
    {
        sim.status.phase = Phase::Won;
        sim.sound(Sfx::Win, pos, 1.0);
        sim.status.score += props[kind].get("worth").and_then(toml::Value::as_integer).unwrap_or(0) as u32;
    }
}

/// The kit's own numbers: the gunship faces the player and fires its chin
/// gun from alternating sides; the player is hurt, goes down twice, frees a
/// hostage, brings the gunship down and wins.
#[test]
fn the_kit_as_it_comes() {
    assert_eq!(run(shooter(|_| {}), FIGHT), KIT);
}

/// Aim assist off, everyone keeps to the eight ways; the gunship turns its
/// door to the player. The player cannot be hurt, so the fight goes on.
#[test]
fn without_aim_assist_and_a_gunship_side_on() {
    let mut sim = shooter(|c| {
        tests::set_weapons(c, "aim_assist", false);
        tests::set(c, "gunship", "side_on", true);
    });
    sim.player.invulnerable = u32::MAX / 2;
    assert_eq!(run(sim, FIGHT), SIDE_ON);
}

/// Two rounds strike the same man in one tick: the first kills him, and the
/// second lands on the man who moved up into his place in the list, however
/// far away he stands (how the shooter has always counted hits).
#[test]
fn two_rounds_on_one_man_hit_the_man_behind_him() {
    let mut cfg = tests::kit();
    cfg.player.spawn = [80.0, 16.0];
    tests::set(&mut cfg, "rifleman", "hp", 1);
    tests::set(&mut cfg, "guard", "hp", 3);
    let men = [tests::marker(&cfg, "rifleman", IVec3::new(80, 200, 1)), tests::marker(&cfg, "guard", IVec3::new(40, 300, 1))];
    let mut sim = Shooter::new(flat_world(IVec3::new(160, 400, 24)), &cfg, tests::shapes(), &men);
    let man = sim.enemy(0).0.body.centre();
    for side in [-0.5, 0.5] {
        let from = man + Vec3::new(side, -2.0, 0.0);
        sim.bullets.push(Bullet { pos: from, prev: from, origin: from, vel: Vec3::Y * 60.0, life: 1.0, damage: 1, blast: 0.0, rocket: false, hostile: false, weapon: 0, team: Some(super::PLAYER_TEAM), from: None });
    }
    sim.step(&Controls::default());
    Classic::new(&cfg).after(&mut sim);
    assert_eq!(sim.enemies().count(), 1, "the man in front should fall");
    assert_eq!(sim.enemy(0).0.hp, 2, "the second round should land on the man behind");
    assert_eq!(sim.state_hash(), 0x7f09abdbff18f459);
}

/// Two pickups on one spot are taken in the same tick, the later one first.
#[test]
fn two_pickups_in_one_tick() {
    let mut cfg = tests::kit();
    cfg.player.spawn = [80.0, 16.0];
    let things = [
        tests::marker(&cfg, "rockets", IVec3::new(80, 22, 1)),
        tests::marker(&cfg, "machine_gun", IVec3::new(80, 22, 1)),
        tests::marker(&cfg, "health", IVec3::new(80, 23, 1)),
    ];
    let sim = Shooter::new(flat_world(IVec3::new(160, 400, 24)), &cfg, tests::shapes(), &things);
    assert_eq!(run(sim, "up:30;none:30"), [0xc9c49890574cdac4]);
}

/// The pins' runs as text, a line a second (`PIN_TRACE=dir cargo test
/// --release pin_traces -- --ignored`): what the kit's state is made of,
/// to show that a change which only moves where a number is kept plays the
/// same before its pins are taken anew.
#[test]
#[ignore]
fn pin_traces() {
    let out = std::path::PathBuf::from(std::env::var("PIN_TRACE").expect("PIN_TRACE"));
    std::fs::create_dir_all(&out).unwrap();
    let side_on = || {
        let mut sim = shooter(|c| {
            tests::set_weapons(c, "aim_assist", false);
            tests::set(c, "gunship", "side_on", true);
        });
        sim.player.invulnerable = u32::MAX / 2;
        sim
    };
    for (name, mut sim) in [("kit", shooter(|_| {})), ("side_on", side_on())] {
        let cfg = tests::kit();
        let mut rules = Classic::new(&cfg);
        rules.respawn = sim.player.body.pos;
        let mut lines = Vec::new();
        let mut tick = 0;
        for (input, ticks) in parse_script(FIGHT, &ACTIONS).unwrap() {
            for _ in 0..ticks {
                sim.run(&input);
                rules.after(&mut sim);
                tick += 1;
                if tick % 60 == 0 {
                    lines.push(pin_line(&sim));
                }
            }
        }
        std::fs::write(out.join(format!("{name}.trace")), lines.join("\n") + "\n").unwrap();
    }
}

/// The gun in hand as the pins' runs wrote it before stage D (the player's
/// arsenal, More Blood's: rifle, machine gun, rockets).
fn slot(sim: &Shooter) -> usize {
    let held = sim.status.weapon.map(|w| sim.rules.weapon_names[w].as_str());
    ["rifle", "machine_gun", "rockets"].iter().position(|g| Some(*g) == held).unwrap_or(0)
}

/// One line of the kit's state for `pin_traces`.
fn pin_line(sim: &Shooter) -> String {
    let s = &sim.status;
    let p = &sim.player.body;
    let mut line = format!(
        "tick {} rng {} changes {} | player {:?} {:?} inv {} | {:?} hp {} score {} weapon {} ammo {} grenades {} | bullets {} grenades {}",
        sim.sim.tick,
        sim.sim.rng.state(),
        sim.sim.world.changes(),
        p.pos,
        p.vel,
        sim.player.invulnerable,
        s.phase,
        s.hp,
        s.score,
        slot(sim),
        if slot(sim) == 0 { 0 } else { s.stock.get(&sim.rules.weapon_names[s.weapon.unwrap()]).copied().unwrap_or(0) },
        s.stock.get("grenade").copied().unwrap_or(0),
        sim.bullets.len(),
        sim.grenades.len()
    );
    for t in sim.things.iter().filter(|t| !matches!(t.brain, Brain::Script(_))) {
        line += &format!(" | {} {:?} hp {}", t.kind, t.body.pos, t.hp);
    }
    line
}

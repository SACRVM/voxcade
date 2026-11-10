//! Network games of real sessions on the simulated network: every guest's
//! recording equals the host's byte for byte, and the host's plays back as
//! any replay.

use std::path::{Path, PathBuf};

use super::play::{Hosting, Joined, Joining, Locals};
use super::*;
use crate::game::Game as _;
use crate::hash::mix;
use crate::replay::{Replay, Script, Verdict, parse_script, parse_seat_script, play_through};

type Lua = crate::Voxcade;

/// A guest on the network: from when it joins, its player's keys.
struct Visitor {
    join: Option<Joining<Lua, usize>>,
    /// Its own copy of the game, if not the host's folder.
    dir: Option<PathBuf>,
    game: Option<crate::session::Session<Lua>>,
    locals: Locals,
    keys: Vec<crate::input::Input>,
    frames: usize,
    next_frame: u64,
    /// Bundles taken but waiting for a fade, as a window keeps them.
    held: std::collections::VecDeque<Bundle>,
}

struct Game {
    net: SimNet,
    now: u64,
    dir: PathBuf,
    game: crate::session::Session<Lua>,
    host: Hosting<Lua, usize>,
    keys: Script,
    owed: u64,
    last: Option<u64>,
    next_frame: u64,
    /// The host stops building ticks (as when it pauses).
    held: bool,
    guests: Vec<Visitor>,
    /// Where games ran apart, as the host found it.
    aparts: Vec<(Machine, Apart)>,
    reports: PathBuf,
}

impl Game {
    fn new(dir: &Path, seed: u64, link: Link, host_keys: &str) -> Self {
        let mut session = crate::session::Session::<Lua>::load(dir).unwrap();
        session.set_clock(1_761_000_000);
        let keys = Script::new(parse_script(host_keys, session.actions()).unwrap());
        let mut host = Hosting::new(&session, mix(seed) | 1, "host").unwrap();
        let reports = std::env::temp_dir().join(format!("voxcade-net-reports-{seed}-{}", std::process::id()));
        host.reports_to = Some(reports.clone());
        Self { net: SimNet::new(seed, link), now: 0, dir: dir.to_path_buf(), game: session, host, keys, owed: 0, last: None, next_frame: 0, held: false, guests: Vec::new(), aparts: Vec::new(), reports }
    }

    /// A guest joining after `join_at` µs, its player holding `keys` (a
    /// seat's script) frame by frame.
    fn guest(&mut self, join_at: u64, keys: &str) {
        let actions = self.game.actions().to_vec();
        let keys = parse_seat_script(keys, &actions).unwrap().into_iter().flat_map(|(i, n)| std::iter::repeat_n(i.unwrap_or_default(), n as usize)).collect();
        self.guests.push(Visitor { join: None, dir: None, game: None, locals: Locals::new(1, false), keys, frames: 0, next_frame: join_at + 7_000 * self.guests.len() as u64, held: Default::default() });
    }

    fn to(&mut self, until: u64) {
        while self.now < until {
            self.now += 500;
            self.net.advance(self.now);
            for (from, to, bytes) in self.net.deliver() {
                if to == 0 {
                    self.host.host.receive(self.now, from, &bytes);
                } else if let Some(j) = &mut self.guests[to - 1].join {
                    j.guest.receive(self.now, from, &bytes);
                }
            }
            let now = self.now;
            if now >= self.next_frame {
                self.next_frame += 16_667;
                for notice in self.host.notices(&self.game).unwrap() {
                    if let Notice::Apart { index, apart } = notice {
                        self.aparts.push((index, apart));
                    }
                }
                let dt = self.last.map_or(0, |t| now - t);
                self.last = Some(now);
                self.game.set_clock(1_761_000_000 + now / 1_000_000);
                if !self.held {
                    self.host.read(1, self.keys.next_input());
                    self.owed += dt;
                    let mut ticks = 0;
                    while self.owed >= TICK_US && ticks < MAX_CATCH_UP {
                        self.host.tick(&mut self.game).unwrap();
                        self.owed -= TICK_US;
                        ticks += 1;
                    }
                }
                for (to, bytes) in self.host.host.outgoing(now) {
                    self.net.send(0, to, bytes);
                }
            }
            for n in 0..self.guests.len() {
                let v = &mut self.guests[n];
                if now < v.next_frame {
                    continue;
                }
                v.next_frame += 16_667;
                let dir = v.dir.clone().unwrap_or_else(|| self.dir.clone());
                let reports = self.reports.clone();
                let join = v.join.get_or_insert_with(|| {
                    let mut j = Joining::new(&dir, Guest::new(0, [n as u8 + 1; 16], mix(n as u64), "guest", "test", now));
                    j.reports_to = Some(reports);
                    j
                });
                if v.game.is_none() && !matches!(join.state(), Joined::Gone(_)) {
                    match join.catch_up(std::time::Duration::from_millis(50)) {
                        Ok(game) => {
                            // A guest fades as its host does (a window).
                            v.game = game.map(|mut g| {
                                g.fades = self.game.fades;
                                g
                            });
                        }
                        Err(e) => {
                            join.end(&format!("{e:#}"));
                        }
                    }
                }
                if let Some(game) = &mut v.game {
                    v.locals.settle(join, |_| true);
                    if let Some(seat) = v.locals.seat(1) {
                        join.read(seat, v.keys.get(v.frames).copied().unwrap_or_default());
                        v.frames += 1;
                    }
                    if !matches!(join.state(), Joined::Playing) {
                        continue;
                    }
                    // A level change's fade frame by frame, the bundles
                    // waiting for it.
                    if game.fading() {
                        join.fade(game);
                        continue;
                    }
                    v.held.extend(join.ticks(now, usize::MAX));
                    while !game.fading()
                        && let Some(b) = v.held.pop_front()
                    {
                        join.play(game, &b).unwrap();
                    }
                }
                for (to, bytes) in join.guest.outgoing(now) {
                    self.net.send(n + 1, to, bytes);
                }
            }
        }
    }

    /// The host holds still for a second, so every guest has every tick;
    /// then each guest's recording must be the host's.
    fn agree(&mut self) -> Replay {
        assert!(self.aparts.is_empty(), "no false alarm: {:?}", self.aparts);
        self.held = true;
        let until = self.now + 1_000_000;
        self.to(until);
        let host = self.host.recording(&self.game).unwrap();
        let text = toml::to_string(&host).unwrap();
        for (n, v) in self.guests.iter().enumerate() {
            let join = v.join.as_ref().expect("joined");
            assert_eq!(join.state(), Joined::Playing, "guest {}", n + 1);
            let game = v.game.as_ref().unwrap();
            assert_eq!(game.game.state_hash(), self.game.game.state_hash(), "guest {}: the same state", n + 1);
            assert!(toml::to_string(&join.recording(game).unwrap()).unwrap() == text, "guest {}: its recording is the host's", n + 1);
        }
        host
    }
}

fn pulsar() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("games/pulsar")
}

/// Pulsar's two ships on two machines (the host's keys and the guest's
/// from `tests/two_ships`): the guest plays the same game, its recording
/// is the host's byte for byte, and the host's plays back as recorded.
#[test]
fn pulsar_plays_on_two_machines() {
    let replay = Replay::load(&pulsar().join("replays/tests/two_ships.toml")).unwrap();
    let mut game = Game::new(&pulsar(), 1, Link::lan(), &replay.input);
    game.guest(0, &replay.seats[0].input.replacen("away:200;", "", 1));
    game.to(25_000_000);
    let recorded = game.agree();
    assert!(recorded.ticks >= 1450 && !recorded.seats.is_empty(), "{} ticks, seats {:?}", recorded.ticks, recorded.seats.len());
    assert_eq!(game.guests[0].locals.seat(1), Some(2), "the guest sits on seat 2");
    let path = std::env::temp_dir().join(format!("voxcade-net-pulsar-{}.toml", std::process::id()));
    recorded.save(&path).unwrap();
    assert_eq!(play_through::<Lua>(&pulsar(), &Replay::load(&path).unwrap()).unwrap(), Verdict::Same, "the host's recording plays back");
}

/// A guest coming eight seconds late on a bad network plays the run so
/// far through, then on with the others; its recording begins at the
/// run's start, as the host's does. Pulsar's two seats taken, it watches.
#[test]
fn a_guest_coming_late_plays_the_run_through() {
    let replay = Replay::load(&pulsar().join("replays/tests/two_ships.toml")).unwrap();
    let mut game = Game::new(&pulsar(), 2, Link::bad(), &replay.input);
    game.guest(0, &replay.seats[0].input.replacen("away:200;", "", 1));
    game.guest(8_000_000, "left:100;right+fire:100;none:600");
    game.to(16_000_000);
    let recorded = game.agree();
    assert_eq!(recorded.seats.iter().map(|s| s.seat).collect::<Vec<_>>(), [2]);
    let late = &game.guests[1];
    assert!(late.locals.seat(1).is_none(), "no seat free: it watches");
    assert!(late.game.as_ref().unwrap().game.sim().tick > 900, "and plays on");
}

/// A game for two that saves.
fn saving_game() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxcade-net-saves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\" }\n  c.players = 2\n  c.saves = { slots = 2 }\n  c.things.hero = {}\n  c.player.kind = \"hero\"\nend\n").unwrap();
    std::fs::write(
        dir.join("main.lua"),
        "function on_world_start()\n  world.state.n = 0\nend\nfunction on_level_start()\n  level:fill(0, 0, 0, 63, 63, 0, 1)\nend\nfunction on_tick()\n  for _, p in ipairs(level:players()) do\n    p:walk(p.input.dir, 20)\n    if p.input:pressed(\"jump\") then world.state.n = world.state.n + p.seat end\n  end\nend\n",
    )
    .unwrap();
    dir
}

/// The host's SAVE travels with its time; F5 starts the run anew for
/// everyone; LOAD sends the save and starts from it for everyone at one
/// tick, the guests keeping the host's slots.
#[test]
fn f5_and_load_start_anew_for_everyone() {
    let dir = saving_game();
    let mut game = Game::new(&dir, 3, Link::lan(), "right+jump:30;left:30;jump:5;none:100;up+jump:200;down:2000");
    game.guest(0, "jump:3;right:200;jump:4;left:300;none:2000");
    game.to(3_000_000);
    game.game.save_to(1).unwrap();
    game.game.eval("world.state.n = world.state.n + 100").unwrap();
    game.to(5_000_000);
    game.agree();
    game.held = false;
    game.host.restart(&game.game).unwrap();
    game.to(7_000_000);
    let restarted = game.agree();
    assert!(restarted.ticks < 200, "a new run: {} ticks", restarted.ticks);
    game.held = false;
    game.host.load(&game.game, 1).unwrap();
    game.to(10_000_000);
    let loaded = game.agree();
    assert!(loaded.from.as_deref().is_some_and(|t| t.starts_with("when = 176100000")), "from the save, with the host's time: {:?}", loaded.from.as_deref().map(|t| &t[..30]));
    let guest = game.guests[0].game.as_ref().unwrap();
    assert_eq!(guest.slots.read(1), game.game.slots.read(1), "the host's slots");
}

/// A state changed on a guest alone (as a game that is not deterministic
/// would) is found within a few checks, named by its part, the guest told
/// (it leaves: joining again would run apart again), and both write a
/// report: the recording and the state hash by parts.
#[test]
fn a_state_run_apart_is_found_and_named() {
    let replay = Replay::load(&pulsar().join("replays/tests/two_ships.toml")).unwrap();
    let mut game = Game::new(&pulsar(), 4, Link::lan(), &replay.input);
    game.guest(0, &replay.seats[0].input.replacen("away:200;", "", 1));
    game.to(5_000_000);
    assert!(game.aparts.is_empty());
    let guest = game.guests[0].game.as_mut().unwrap();
    let tick = guest.game.sim().tick;
    guest.game.eval("world.state.only_here = true").unwrap();
    game.to(6_500_000);
    let (index, apart) = game.aparts.first().cloned().expect("found");
    assert_eq!((index, apart.part.as_str(), apart.inputs), (1, "world.state", false));
    assert!(apart.tick >= tick && apart.tick <= tick + 10 + 6, "within a check and the delay: at {} after {tick}", apart.tick);
    let join = game.guests[0].join.as_ref().unwrap();
    assert_eq!(join.state(), Joined::Apart(apart.clone()), "the guest is told");
    let at = join.report(game.guests[0].game.as_ref().unwrap(), &apart).unwrap().expect("written");
    for file in ["recording.toml", "sections.txt"] {
        assert!(at.join(file).is_file(), "{file}");
    }
    let host_report = game.host.reports.first().expect("the host's").join("sections.txt");
    let text = std::fs::read_to_string(host_report).unwrap();
    assert!(text.contains("world.state") && text.contains("the game"), "{text}");
    game.aparts.clear();
}

/// A guest that played something else than the host sent (its network
/// broke a bundle) differs in what was played: the network's fault, which
/// joining again mends.
#[test]
fn what_was_played_differing_is_the_networks() {
    let replay = Replay::load(&pulsar().join("replays/tests/two_ships.toml")).unwrap();
    let mut game = Game::new(&pulsar(), 5, Link::lan(), &replay.input);
    game.guest(0, &replay.seats[0].input.replacen("away:200;", "", 1));
    game.to(4_000_000);
    game.guests[0].join.as_mut().unwrap().break_next = true;
    game.to(5_000_000);
    let (_, apart) = game.aparts.first().cloned().expect("found");
    assert!(apart.inputs, "{apart:?}");
    game.aparts.clear();
}

/// A guest whose copy of the game differs from the host's is told which
/// file before it plays.
#[test]
fn a_guest_whose_files_differ_is_told_which() {
    let copy = std::env::temp_dir().join(format!("voxcade-net-copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    copy_dir(&pulsar(), &copy);
    let main = std::fs::read_to_string(copy.join("main.lua")).unwrap();
    std::fs::write(copy.join("main.lua"), main + "\nlocal ANOTHER = 1\n").unwrap();
    let mut game = Game::new(&pulsar(), 6, Link::lan(), "fire:600");
    game.guest(0, "fire:600");
    game.guests[0].dir = Some(copy.clone());
    game.to(3_000_000);
    let state = game.guests[0].join.as_ref().unwrap().state();
    assert!(matches!(&state, Joined::Gone(why) if why.contains("main.lua differs")), "{state:?}");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

/// A game for two that starts its level anew at ticks 300 and 600, where
/// a check of the recording falls, with a fade.
fn restarting_game() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxcade-net-fades-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.size = { 64, 64, 16 }\n  c.player.spawn = { 32, 32 }\n  c.actions = { \"jump\" }\n  c.players = 2\n  c.things.hero = {}\n  c.player.kind = \"hero\"\n  c.transition.fade = 0.4\nend\n").unwrap();
    std::fs::write(
        dir.join("main.lua"),
        "function on_world_start()\n  world.state.ends = 0\nend\nfunction on_level_start()\n  level:fill(0, 0, 0, 63, 63, 0, 1)\nend\nfunction on_level_end()\n  world.state.ends = world.state.ends + 1\nend\nfunction on_tick()\n  for _, p in ipairs(level:players()) do\n    p:walk(p.input.dir, 20)\n  end\n  if world.tick == 300 or world.tick == 600 then\n    world:restart()\n  end\nend\n",
    )
    .unwrap();
    dir
}

/// A level change fades on every machine, the bundles waiting for it; a
/// check of the recording falling on the change is taken once it is done
/// (as a replay, which cuts, sees it), so every recording is the host's
/// and a guest coming after the changes plays the run so far through.
#[test]
fn level_changes_fade_on_every_machine_alike() {
    let dir = restarting_game();
    let mut game = Game::new(&dir, 7, Link::lan(), "right:60;left:60;up:60;down:600");
    game.game.fades = true;
    game.guest(0, "left:90;right:90;up+jump:30;down:900");
    game.guest(12_000_000, "right:300");
    game.to(14_000_000);
    let recorded = game.agree();
    assert!(recorded.ticks > 700, "{} ticks", recorded.ticks);
    assert_eq!(game.game.game.eval("return world.state.ends").unwrap(), "2");
}

//! Runs that play the same: reloads, the fingerprint, snapshots, replays
//! and `check`, the game's own files read as it starts.

use super::*;

/// Saving `main.lua` takes the new code in and keeps `world.state`; a
/// mended error lets the game go on where it stopped.
#[test]
fn a_reload_keeps_world_state_and_a_fix_resumes() {
    let dir = folder("reload", "function on_world_start() world.state.n = 0 end\nfunction on_tick() world.state.n = world.state.n + 1 end\n");
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    steps(&mut g, 3);
    std::fs::write(dir.join("main.lua"), "function on_tick() world.state.n = world.state.n + 10 end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "n"), format!("{:?}", crate::lua::Value::Int(13)));
    std::fs::write(dir.join("main.lua"), "function on_tick() error('broken') end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 2);
    assert!(halted(&g).contains("broken"));
    std::fs::write(dir.join("main.lua"), "function on_tick() world.state.n = world.state.n + 1 end\n").unwrap();
    g.reload(&dir.join("main.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "n"), format!("{:?}", crate::lua::Value::Int(14)));
}

/// A setting that only takes effect at the start waits for a restart; the
/// others take effect at once.
#[test]
fn restart_only_settings_wait_for_f5() {
    let dir = folder("restart", "function on_world_start() end\n");
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("{ 32, 32 }", "{ 20, 20 }").replace("c.actions", "c.camera.pitch = 50\n  c.actions")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    assert_eq!(g.cfg.player.spawn, [32.0, 32.0], "the start moved without a restart");
    assert_eq!(g.cfg.camera.pitch, 50.0);
}

/// A kind's props changed in conf.lua show at once on its things; a
/// misspelt sound there is a mistake on a reload as at the start.
#[test]
fn a_reload_takes_the_kinds_props_and_checks_their_sounds() {
    let main = "function on_level_start(how) level:spawn(\"box\", 6, 2) end\nfunction on_tick() world.state.worth = level:things(\"box\")[1].props.worth end\n";
    let dir = events_folder("reload-props", main, "  c.things.box.props = { worth = 1 }\n");
    let mut g = game_in(&dir);
    steps(&mut g, 2);
    assert_eq!(g.game.inspect()["state"]["worth"], 1);
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("worth = 1", "worth = 5")).unwrap();
    g.reload(&dir.join("conf.lua")).unwrap();
    steps(&mut g, 1);
    assert_eq!(g.game.inspect()["state"]["worth"], 5, "the kind's props anew");
    std::fs::write(dir.join("conf.lua"), conf.replace("worth = 1 }", "worth = 1 }\n  c.things.box.sound = \"poof\"")).unwrap();
    let e = g.reload(&dir.join("conf.lua")).map(|_| ()).unwrap_err();
    assert!(format!("{e:#}").contains("c.things.box.sound = \"poof\": no sound called that"), "{e:#}");
}

/// Every setting that waits for a restart is kept until then: a config
/// that differs in all of them, kept back, differs in none.
#[test]
fn what_waits_for_a_restart_is_kept_until_then() {
    use crate::config::{BlocksConfig, GameConfig};
    use crate::controls::Bindings;
    let old = GameConfig::default();
    let mut new = GameConfig::default();
    new.kit = Some("shooter".into());
    new.actions = vec!["jump".into()];
    new.keys = Bindings::new(&[("jump", &["Space"])]);
    new.pad = Bindings::new(&[("jump", &["south"])]);
    new.world.file = "other.vvx".into();
    new.world.size = Some([8, 8, 8]);
    new.world.blocks = Some(BlocksConfig::default());
    new.markers = toml::from_str("rifleman = 240").unwrap();
    new.player.spawn = [1.0, 2.0];
    assert_eq!(old.restart_only(&new).len(), 9, "{:?}", old.restart_only(&new));
    old.keep_for_restart(&mut new);
    assert!(old.restart_only(&new).is_empty(), "{:?}", old.restart_only(&new));
}

/// A level of blocks is built anew when one of its blocks is saved, a new
/// one too, and its fingerprint follows; the hidden file a block is saved
/// through and other files in the folder do not count.
#[test]
fn saving_a_block_rebuilds_the_level() {
    use crate::blocks::{self, Block};
    use crate::grid::{BlockEntry, Cell, GridLevel, Kind};
    use crate::session::Reloaded;
    use glam::IVec3;
    let dir = crate::scratch::Scratch::new("lua-blocks");
    std::fs::write(dir.join("conf.lua"), "function conf(c)\n  c.world.blocks = { file = \"level.toml\" }\n  c.player.spawn = { 2, 2 }\nend\n").unwrap();
    std::fs::write(dir.join("main.lua"), "function on_world_start() end\n").unwrap();
    let mut level = GridLevel::new(4, IVec3::new(2, 1, 1));
    level.blocks.push(BlockEntry { name: "rock".into(), kind: Kind::Solid });
    level.cells[0] = Cell { block: 1, turn: Default::default() };
    level.save(&dir.join("level.toml")).unwrap();
    let rock = |c: [u8; 3]| {
        let mut b = Block::empty(4);
        b.set(IVec3::ZERO, Some(c)).unwrap();
        b
    };
    rock([1, 2, 3]).save(&blocks::path(&dir, "rock")).unwrap();
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let before = g.fingerprint().unwrap();
    rock([9, 9, 9]).save(&blocks::path(&dir, "rock")).unwrap();
    assert!(matches!(g.reload(&blocks::path(&dir, "rock")).unwrap(), Reloaded::World));
    let world = &g.game.sim().world;
    assert_eq!(world.palette[world.get(IVec3::ZERO) as usize], [9, 9, 9, 255]);
    assert_ne!(g.fingerprint().unwrap(), before);
    rock([5, 5, 5]).save(&blocks::path(&dir, "new")).unwrap();
    assert!(matches!(g.reload(&blocks::path(&dir, "new")).unwrap(), Reloaded::World), "a new block");
    std::fs::write(dir.join("blocks/notes.txt"), "notes").unwrap();
    assert!(matches!(g.reload(&dir.join("blocks/notes.txt")).unwrap(), Reloaded::Nothing));
    assert!(matches!(g.reload(&dir.join("blocks/.rock.vvx.tmp")).unwrap(), Reloaded::Nothing));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Comments, blank lines and the look in conf.lua leave the replays'
/// fingerprint; a changed number in main.lua does not.
#[test]
fn comments_and_the_look_leave_the_fingerprint() {
    let main = "function on_world_start()\n  world.state.n = 1\nend\n";
    let base = game("print-a", main).fingerprint().unwrap();
    assert_eq!(game("print-b", &format!("-- a game\n\n{}", main.replace("  ", "\t"))).fingerprint().unwrap(), base);
    let dir = folder("print-c", main);
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap();
    std::fs::write(dir.join("conf.lua"), conf.replace("c.actions", "c.camera.view_width = 300\n  c.light.ambient = 0.7\n  c.actions")).unwrap();
    assert_eq!(Session::<LuaGame>::load(&dir).unwrap().fingerprint().unwrap(), base);
    assert_ne!(game("print-d", &main.replace("= 1", "= 2")).fingerprint().unwrap(), base);
}

/// A Lua game kept at a tick and brought back plays on exactly as the
/// first time, its script's state with it.
#[test]
fn a_restored_lua_game_plays_on_the_same() {
    use crate::replay::{Replay, Script, parse_script};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/sokovox");
    let replay = Replay::load(&dir.join("replays/tests/solve.toml")).unwrap();
    let mut g = Session::<LuaGame>::load(&dir).unwrap();
    let mut script = Script::new(parse_script(&replay.input, g.actions()).unwrap());
    let inputs: Vec<_> = (0..600).map(|_| script.next_input()).collect();
    for input in &inputs[..300] {
        g.step(input);
    }
    let kept_at = g.game.state_hash();
    let snapshot = g.snapshot().unwrap();
    let mut first = Vec::new();
    for input in &inputs[300..] {
        g.step(input);
        first.push(g.game.state_hash());
    }
    g.restore(&snapshot);
    assert_eq!(g.game.state_hash(), kept_at);
    for (n, input) in inputs[300..].iter().enumerate() {
        g.step(input);
        assert_eq!(g.game.state_hash(), first[n], "tick {} played out differently", 300 + n + 1);
    }
}

/// A local of `main.lua` that `on_tick` changes is state no snapshot keeps:
/// `voxcade check` plays a second twice from a snapshot, sees the game run
/// differently and names the local.
#[test]
fn check_names_state_kept_outside_world_state() {
    use crate::replay::{Recorder, check};
    let clean = "function on_tick() world.state.n = (world.state.n or 0) + 1 end\n";
    let hidden = "local count = 0\nfunction on_tick()\n  count = count + 1\n  if count % 45 == 0 then world.state.n = (world.state.n or 0) + 1 end\nend\n";
    for (name, main, found) in [("guard-clean", clean, None), ("guard-hidden", hidden, Some("'count' (main.lua) changed"))] {
        let dir = folder(name, main);
        let mut g = game_in(&dir);
        let mut recorder = Recorder::default();
        for _ in 0..600 {
            let input = Input::default();
            g.step(&input);
            recorder.record(&g);
        }
        std::fs::create_dir_all(dir.join("replays")).unwrap();
        recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
        match (check::<LuaGame>(&dir, false), found) {
            (Ok(same), None) => assert!(same, "the clean game ran apart"),
            (Err(e), Some(found)) => assert!(e.to_string().contains(found), "{e}"),
            (r, _) => panic!("{name}: {:?}", r.map_err(|e| e.to_string())),
        }
    }
}

/// `check --rollback K` goes back K ticks again and again and plays on, as
/// rollback over a network would: a clean game comes out as recorded, one
/// that keeps state outside its states is found at the tick it went
/// apart.
#[test]
fn going_back_and_playing_on_comes_out_the_same() {
    use crate::replay::{Recorder, check_going_back};
    let clean = "function on_tick() world.state.n = (world.state.n or 0) + 1 end\n";
    let hidden = "local count = 0\nfunction on_tick()\n  count = count + 1\n  if count % 5 == 0 then world.state.n = (world.state.n or 0) + 1 end\nend\n";
    for (name, main, found) in [("back-clean", clean, None), ("back-hidden", hidden, Some("came out differently"))] {
        let dir = folder(name, main);
        let mut g = game_in(&dir);
        let mut recorder = Recorder::default();
        for _ in 0..240 {
            let input = Input::default();
            g.step(&input);
            recorder.record(&g);
        }
        std::fs::create_dir_all(dir.join("replays")).unwrap();
        recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
        match (check_going_back::<LuaGame>(&dir, 8), found) {
            (Ok(same), None) => assert!(same, "the clean game ran apart"),
            (Err(e), Some(found)) => assert!(e.to_string().contains(found), "{e}"),
            (r, _) => panic!("{name}: {:?}", r.map_err(|e| e.to_string())),
        }
    }
}

/// `check` draws the game after every tick, as a window does: a mistake
/// in `on_draw` that shows in a single picture (BunnyFarm's dog's board
/// stopped the game only in the window) stops the replay there, and so
/// does recording it anew.
#[test]
fn check_draws_every_tick() {
    use crate::replay::{Recorder, check};
    let main = "function on_tick() world.state.open = world.tick == 30 end\nfunction on_draw(frame)\n  if world.state.open then math.random(6) end\nend\n";
    let dir = folder("check-draws", main);
    let mut g = game_in(&dir);
    let mut recorder = Recorder::default();
    for _ in 0..120 {
        let input = Input::default();
        g.step(&input);
        recorder.record(&g);
    }
    assert_eq!(halted(&g), "", "played without a picture, nothing stopped it");
    std::fs::create_dir_all(dir.join("replays")).unwrap();
    recorder.replay(&g).unwrap().save(&dir.join("replays/run.toml")).unwrap();
    for update in [false, true] {
        let e = check::<LuaGame>(&dir, update).map(|_| ()).unwrap_err().to_string();
        assert!(e.contains("stopped at tick") && e.contains("math.random"), "update {update}: {e}");
    }
}

/// `world:read` gives a file of the game folder while `main.lua` starts,
/// never later and never outside the folder; what it read counts for
/// replays, and saving it runs `main.lua` anew. The old `read` says where
/// it went.
#[test]
fn read_reads_the_game_folder_while_starting() {
    let main = "TEXT = world:read(\"levels.txt\")\nfunction on_world_start() world.state.t = TEXT .. world:read(\"levels.txt\") end\nfunction on_tick() world.state.now = TEXT if world.tick == 3 then world:read(\"levels.txt\") end end\n";
    let dir = code_world("read", main, &[("levels.txt", "ab")]);
    let mut g = game_in(&dir);
    assert_eq!(state(&mut g, "t"), format!("{:?}", crate::lua::Value::Str(b"abab".to_vec())));
    let before = g.fingerprint().unwrap();
    std::fs::write(dir.join("levels.txt"), "cd").unwrap();
    assert!(matches!(g.reload(&dir.join("levels.txt")).unwrap(), crate::session::Reloaded::Code));
    assert_ne!(g.fingerprint().unwrap(), before, "the file read counts for replays");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "now"), format!("{:?}", crate::lua::Value::Str(b"cd".to_vec())), "saving it ran main.lua anew");
    steps(&mut g, 3);
    assert!(halted(&g).contains("only while main.lua starts"), "{}", halted(&g));
    for bad in ["../conf.lua", "/etc/passwd", ""] {
        let dir = code_world("read-bad", &format!("X = world:read(\"{bad}\")\n"), &[]);
        let e = start_error(&dir);
        assert!(e.contains("a file in the game folder"), "{bad}: {e}");
    }
    let dir = code_world("read-old", "X = read(\"levels.txt\")\n", &[("levels.txt", "ab")]);
    assert!(start_error(&dir).contains("read(name) is world:read(name) now"), "{}", start_error(&dir));
}

/// `require` runs a Lua file of the game folder once while the game starts
/// (dots between folders) and gives back what it returns, the same every
/// time; the files share `main.lua`'s globals, count for replays as
/// tokens (a comment changes nothing) and saving one runs the code anew.
/// Later, a loop, a missing file, a bad name, `main` and `conf` are
/// refused; a mistake in one names its file and line.
#[test]
fn a_game_runs_its_own_lua_files() {
    let main = "local util = require(\"util\")\nlocal de = require(\"lang.de\")\nSAME = require(\"util\") == util\nfunction on_world_start() world.state.t = util.twice(21) .. de.hello .. tostring(SAME) .. tostring(HELPER) end\nfunction on_tick() world.state.now = util.twice(1) if world.tick == 3 then require(\"util\") end end\n";
    let util = "HELPER = true\nlocal M = {}\nfunction M.twice(n) return n * 2 end\nreturn M\n";
    let dir = code_world("require", main, &[("util.lua", util)]);
    std::fs::create_dir_all(dir.join("lang")).unwrap();
    std::fs::write(dir.join("lang/de.lua"), "return { hello = \"hallo\" }\n").unwrap();
    let mut g = game_in(&dir);
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "t"), format!("{:?}", crate::lua::Value::Str(b"42hallotruetrue".to_vec())));
    let before = g.fingerprint().unwrap();
    std::fs::write(dir.join("util.lua"), format!("-- a comment\n{util}")).unwrap();
    assert!(matches!(g.reload(&dir.join("util.lua")).unwrap(), crate::session::Reloaded::Code), "saving a required file runs the code anew");
    assert_eq!(g.fingerprint().unwrap(), before, "a comment changes nothing for replays");
    std::fs::write(dir.join("util.lua"), util.replace("n * 2", "n * 3")).unwrap();
    assert!(matches!(g.reload(&dir.join("util.lua")).unwrap(), crate::session::Reloaded::Code));
    assert_ne!(g.fingerprint().unwrap(), before, "a required file is the game's code");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "now"), "Int(3)", "the new code runs");
    assert_eq!(state(&mut g, "t"), format!("{:?}", crate::lua::Value::Str(b"42hallotruetrue".to_vec())), "the states stay");
    steps(&mut g, 3);
    assert!(halted(&g).contains("require(\"util\"): only while the game starts"), "{}", halted(&g));
    let refused = |name: &str, main: &str, files: &[(&str, &str)], want: &str| {
        let e = start_error(&code_world(name, main, files));
        assert!(e.contains(want), "{name}: {e}");
    };
    refused("require-loop", "require(\"a\")\n", &[("a.lua", "require(\"b\")\n"), ("b.lua", "require(\"a\")\n")], "require(\"a\"): a loop: a → b → a");
    refused("require-missing", "require(\"nope\")\n", &[], "require(\"nope\"): no nope.lua in the game folder");
    for bad in ["../conf", "a/b", "", "a..b"] {
        refused("require-bad", &format!("require(\"{bad}\")\n"), &[], "a Lua file of the game folder by its name");
    }
    refused("require-dot-lua", "require(\"util.lua\")\n", &[], "the file's name without .lua: require(\"util\")");
    refused("require-main", "require(\"main\")\n", &[], "main.lua is the engine's to run");
    refused("require-conf", "require(\"conf\")\n", &[], "conf.lua is the engine's to run");
    refused("require-error", "require(\"boom\")\n", &[("boom.lua", "local x = 1\nerror(\"bang\")\n")], "boom.lua:2: bang");
    refused("require-power", "require(\"pow\")\n", &[("pow.lua", "return 2 ^ 3\n")], "pow.lua");
}

/// A mistake in `on_world_start` or the first `on_level_start` stops the
/// game rather than its loading; the mended file starts it afresh from its
/// files, not from the half-made start. A state mistake names its tick.
#[test]
fn a_mistake_at_the_start_stops_and_the_mend_starts_afresh() {
    let dir = folder("start-mend", "function on_world_start() world.state.starts = 1 end\nfunction on_level_start(how)\n  local x = nil\n  return x.y\nend\n");
    let mut g = game_in(&dir);
    assert!(halted(&g).contains("main.lua:4:"), "{}", halted(&g));
    assert!(g.game.start_failed());
    std::fs::write(dir.join("main.lua"), "function on_world_start() world.state.starts = (world.state.starts or 0) + 1 end\nfunction on_level_start(how) world.state.level = how end\n").unwrap();
    assert!(matches!(g.reload(&dir.join("main.lua")).unwrap(), crate::session::Reloaded::World));
    assert_eq!(halted(&g), "");
    assert!(!g.game.start_failed());
    assert_eq!(state(&mut g, "starts"), format!("{:?}", crate::lua::Value::Int(1)), "afresh, not on top of the stopped start");
    assert_eq!(state(&mut g, "level"), format!("{:?}", crate::lua::Value::Str(b"start".to_vec())));
    let mut g = game("state-tick", "function on_tick() if world.tick == 4 then world.state.f = print end end\n");
    steps(&mut g, 4);
    assert!(halted(&g).contains("world.state.f is a function") && halted(&g).contains("(at the end of tick 4)"), "{}", halted(&g));
}

/// The state hash in parts (`docs/multiplayer.md`, "Desyncs"): each part
/// the hash after it, the last of the game's the state hash itself, then
/// the ticks played; a difference in one state names that part, and what
/// was played stays the same, while a different tick shows in `inputs`.
#[test]
fn the_hash_in_parts_names_where_two_games_differ() {
    let main = "function on_level_start()\n  level:fill(0, 0, 0, 63, 63, 0, 1)\n  level.state.n = 0\nend\nfunction on_tick()\n  world.state.t = world.tick\nend\n";
    let (mut a, mut b) = (game("parts-a", main), game("parts-b", main));
    steps(&mut a, 30);
    steps(&mut b, 30);
    let parts = a.sections();
    let names: Vec<&str> = parts.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["things", "world.state", "player.state", "level.state", "levels", "kept", "thing.state", "seats", "music", "inputs"]);
    assert_eq!(parts[parts.len() - 2].1, a.game.state_hash(), "the last of the game's parts is its hash");
    assert_eq!(a.sections(), b.sections());
    b.game.eval("level.state.n = 1").unwrap();
    let first_apart = |a: &Session<LuaGame>, b: &Session<LuaGame>| a.sections().into_iter().zip(b.sections()).find(|(x, y)| x.1 != y.1).map(|(x, _)| x.0);
    assert_eq!(first_apart(&a, &b), Some("level.state"));
    assert_eq!(a.sections().last(), b.sections().last(), "both played the same");
    let (mut c, mut d) = (game("parts-c", main), game("parts-d", main));
    c.step(&Input::default());
    d.step(&Input { up: true, ..Input::default() });
    assert_ne!(c.sections().last(), d.sections().last(), "another key held shows in what was played");
}

/// Every tick as it was played (`Session::played`), fed again whole as the
/// network will feed it (`Session::play`), plays the same run: the same
/// states after every tick, the same recording byte for byte, a save
/// stamped with the time the tick brought, not the machine's.
#[test]
fn ticks_played_again_whole_give_the_same_run() {
    use crate::replay::Recorder;
    let saving = |name: &str| {
        let (g, dir) = two_players(name, TWO_PLAYERS);
        let conf = std::fs::read_to_string(g.dir.join("conf.lua")).unwrap().replace("c.players = 2", "c.players = 2\n  c.saves = { slots = 2 }");
        std::fs::write(g.dir.join("conf.lua"), conf).unwrap();
        (game_in(&g.dir), dir)
    };
    let (mut a, _a_dir) = saving("feed-a");
    let (mut ticks, mut hashes, mut recorder) = (Vec::new(), Vec::new(), Recorder::default());
    a.set_clock(1_761_000_000);
    for n in 0..240u64 {
        if (20..200).contains(&n) {
            a.set_seat(2, Some(Input { right: n % 40 < 20, actions: if n % 7 == 0 { 2 } else { 0 }, ..Input::default() }));
        }
        if n == 200 {
            a.set_seat(2, None);
        }
        if n == 100 {
            a.eval("world.state.helped = true").unwrap();
        }
        if n == 150 {
            a.set_clock(1_761_000_123);
            a.save_to(1).unwrap();
            a.eval("world.state.saved = world.tick").unwrap();
        }
        a.step(&Input { left: n % 50 < 10, actions: if n % 11 == 0 { 2 } else { 0 }, ..Input::default() });
        ticks.push(a.played().clone());
        hashes.push(a.game.state_hash());
        recorder.record(&a);
    }
    assert_eq!(halted(&a), "");
    let (mut b, _b_dir) = saving("feed-b");
    b.set_clock(5);
    let mut again = Recorder::default();
    for (n, tick) in ticks.iter().enumerate() {
        b.play(tick);
        assert_eq!(b.played(), tick, "tick {n}");
        assert_eq!(b.game.state_hash(), hashes[n], "tick {n}");
        again.record(&b);
    }
    assert_eq!(b.sections(), a.sections());
    let text = |r: &Recorder, g: &Session<LuaGame>| toml::to_string(&r.replay(g).unwrap()).unwrap();
    assert_eq!(text(&again, &b), text(&recorder, &a));
    assert!(a.slots.read(1).unwrap().starts_with("when = 1761000123"), "{:?}", a.slots.read(1));
    assert_eq!(b.slots.read(1), a.slots.read(1), "stamped with the time the tick brought");
}

/// Whether a game goes with its music or moves its sun is told by the
/// names in its code: a word in a comment or a string does not count.
#[test]
fn music_and_daylight_in_a_comment_or_a_string_do_not_count() {
    let main = "-- music:play and on_beat, frame:daylight\nlocal WORDS = \"music daylight on_beat\"\nfunction on_tick() world.state.w = WORDS end\n";
    let g = game_in(&code_world("words-only", main, &[]));
    assert!(!g.game.uses_music() && !g.game.moves_sun());
    let g = game_in(&code_world("names-music", "function on_beat(b) end\n", &[]));
    assert!(g.game.uses_music() && !g.game.moves_sun());
}

//! The keys: `controls`, `input`, the actions and what replays carry of
//! them.

use super::*;

/// `controls` is what the player plays with, there everywhere: the device
/// used last and how it labels an action's key (the keyboard by its
/// layout, the pad by its letters), in a text as the action in braces too
/// (its sign where there is one, `{action:name}` its name; a sign's own
/// character in a script's text is left out); nobody plays until a window
/// or a replay says who does. An action the game lacks is named, and so
/// is a brace left open or a word after the colon but name.
#[test]
fn controls_name_the_keys_in_hand() {
    use crate::controls::{Bindings, Device, PadStyle};
    let main = "function on_tick() world.state.seen = controls.device or 'nobody' end\n\
                function on_draw(frame) frame:text((controls.device or 'nobody') .. ':' .. controls:key('fire') .. ':{pause:name}:{pause}:{{\u{E000}', 0, 0) end\n";
    let mut g = game("keys-in-hand", main);
    let shown = |g: &Session<LuaGame>| {
        g.compose(0.0, 1.6, Scene::Play, None);
        first_text(g)
    };
    assert_eq!(shown(&g), "nobody::::{");
    let sign = |device, label| crate::ui::signs::key_sign(device, label).unwrap();
    let mut settings = crate::settings::Settings::default();
    settings.keys = Bindings::new(&[("fire", &["Z"]), ("pause", &["Escape"])]);
    settings.pad = Bindings::new(&[("fire", &["east"]), ("pause", &["start"])]);
    settings.key_names.insert("Z".into(), "Y".into());
    g.set_controls(g.player_controls(Device::Keyboard, PadStyle::Xbox, &settings));
    assert_eq!(shown(&g), format!("keyboard:Y:ESCAPE:{}:{{", sign(Device::Keyboard, "ESCAPE")), "a German keyboard's Y where the binding says Z");
    steps(&mut g, 1);
    assert_eq!(state(&mut g, "seen"), format!("{:?}", crate::lua::Value::Str(b"keyboard".to_vec())), "on_tick sees it too");
    g.set_controls(g.player_controls(Device::Gamepad, PadStyle::PlayStation, &settings));
    assert_eq!(shown(&g), format!("gamepad:CIRCLE:OPTIONS:{}:{{", sign(Device::Gamepad, "OPTIONS")));
    for (name, line, says) in [
        ("keys-unknown", "frame:text(controls:key('jumpp'), 0, 0)", "controls:key: no action 'jumpp'"),
        ("keys-unknown-text", "frame:text('PRESS {jumpp}', 0, 0)", "frame:text: no action 'jumpp'"),
        ("keys-open-brace", "frame:text('PRESS {fire', 0, 0)", "frame:text: a '{' without its '}'"),
        ("keys-other-word", "frame:text('PRESS {fire:sign}', 0, 0)", "frame:text: {fire:sign}: after an action only :name"),
    ] {
        let g = game(name, &format!("function on_draw(frame) {line} end\n"));
        g.compose(0.0, 1.6, Scene::Play, None);
        assert!(halted(&g).contains(says), "{name}: {}", halted(&g));
    }
}

/// A replay carries what the player played with, from the step it
/// changed on: a game that goes by `controls` plays back the same (the
/// guard's second played twice included), and would not without them.
#[test]
fn a_replay_carries_the_controls() {
    use crate::controls::{Device, PadStyle};
    use crate::replay::{Recorder, Replay, Verdict, play_through};
    let main = "function on_world_start() world.state.pad = 0 end\n\
                function on_tick() if controls.device == 'gamepad' then world.state.pad = world.state.pad + 1 end end\n";
    let dir = folder("controls-replay", main);
    let mut g = game_in(&dir);
    let mut settings = crate::settings::Settings::default();
    settings.adopt(g.actions(), &g.default_keys(), &g.default_pad(), g.has_pointer());
    let mut recorder = Recorder::default();
    for tick in 0..360 {
        let device = if (120..320).contains(&tick) { Device::Gamepad } else { Device::Keyboard };
        g.set_controls(g.player_controls(device, PadStyle::Xbox, &settings));
        g.step(&Input::default());
        recorder.record(&g);
    }
    assert_eq!(state(&mut g, "pad"), format!("{:?}", crate::lua::Value::Int(200)));
    let replay = recorder.replay(&g).unwrap();
    assert_eq!(replay.controls.iter().map(|c| c.at).collect::<Vec<_>>(), [0, 120, 320]);
    let path = dir.join("controls.toml");
    replay.save(&path).unwrap();
    let back = Replay::load(&path).unwrap();
    assert_eq!(back, replay, "written and read back");
    assert_eq!(play_through::<LuaGame>(&dir, &back).unwrap(), Verdict::Same);
    let without = Replay { controls: Vec::new(), ..back };
    assert!(matches!(play_through::<LuaGame>(&dir, &without).unwrap(), Verdict::RunsApart(_)), "nobody played the pad");
}

/// `input:repeated` is true as a key goes down and again while it is held
/// (after 20 ticks, every 6); `frame:menu` and `frame:grid` are read and
/// checked (rows as texts or tables, keys in braces filled, the chosen row
/// from 1) and drawn, a long list scrolled; a mistake names what is wanted.
#[test]
fn a_game_draws_menus_and_repeats_keys() {
    let main = r##"
function on_world_start() world.state.down = 0 world.state.act = 0 end
function on_tick()
  if input:repeated("down") then world.state.down = world.state.down + 1 end
  if input:repeated("fire") then world.state.act = world.state.act + 1 end
end
function on_draw(frame)
  local rows = {}
  for i = 1, 10 do rows[i] = { text = "row " .. i, value = i * 3, dim = i == 2 } end
  rows[1] = "Plain"
  frame:menu{ title = "Market", tabs = { "Buy", "Sell" }, tab = 2, rows = rows, at = 7, shown = 4, hint = "{fire} buys" }
  frame:grid{ cells = { { count = 3 }, false, { dim = true } }, at = 2, columns = 2 }
end
"##;
    let mut g = game("menus", main);
    let down = Input { down: true, ..Input::default() };
    for _ in 0..40 {
        g.step(&down);
    }
    let mut fire = Input::default();
    fire.hold(1, true);
    g.step(&fire);
    assert_eq!((state(&mut g, "down"), state(&mut g, "act")), ("Int(5)".into(), "Int(1)".into()), "at 1, 21, 27, 33 and 39 ticks held; a press");
    g.compose(0.0, 16.0 / 9.0, crate::session::Scene::Play, None);
    assert_eq!(halted(&g), "");
    let overlay = g.game.drawn.borrow();
    let menu = overlay.iter().find_map(|c| match c { crate::lua::world::Cmd::Menu(m) => Some(m.clone()), _ => None }).expect("a menu");
    assert_eq!((menu.rows.len(), menu.at, menu.tab, menu.shown), (10, Some(6), 1, 4));
    assert_eq!((menu.rows[0].text.as_str(), menu.rows[1].dim, menu.rows[2].value.as_deref()), ("Plain", true, Some("9")));
    assert!(menu.hint.as_deref().is_some_and(|h| h.ends_with(" buys") && !h.contains('{')), "{:?}", menu.hint);
    let grid = overlay.iter().find_map(|c| match c { crate::lua::world::Cmd::Grid(g) => Some(g.clone()), _ => None }).expect("a grid");
    assert_eq!((grid.cells.len(), grid.at, grid.cells[0].count, grid.cells[1].icon.is_none(), grid.cells[2].dim), (3, Some(1), Some(3), true, true));
    drop(overlay);
    let ui = crate::config::UiConfig::default();
    assert!(g.game.hud(&ui, 0.0, 16.0 / 9.0, None).0.len() > 500, "drawn");
    for (bad, want) in [
        ("frame:menu{ rows = { \"a\" }, at = 2 }", "frame:menu: at = 2, but there are 1 (from 1)"),
        ("frame:menu{ rows = { \"a\" }, colour = 1 }", "frame:menu: no 'colour'"),
        ("frame:menu{ rows = { \"a\", 7 } }", "rows[2] is a text or"),
        ("frame:grid{ cells = { 1 } }", "cells[1] is { icon =, model =, count =, dim = }"),
    ] {
        let g = game("menus-bad", &format!("function on_draw(frame) {bad} end\n"));
        g.compose(0.0, 16.0 / 9.0, crate::session::Scene::Play, None);
        assert!(halted(&g).contains(want), "{bad}: {}", halted(&g));
    }
}

/// The directions are pressed and released like actions: one step a press.
#[test]
fn directions_are_pressed_like_actions() {
    let main = "function on_tick()\n  if input:pressed('right') then world.state.steps = (world.state.steps or 0) + 1 end\n  if input:released('right') then world.state.ups = (world.state.ups or 0) + 1 end\n  world.state.held = input:held('right')\nend\n";
    let mut g = game("directions", main);
    let right = Input { right: true, ..Input::default() };
    for input in [right.clone(), right.clone(), right.clone(), Input::default(), right, Input::default()] {
        g.step(&input);
    }
    assert_eq!(halted(&g), "");
    assert_eq!(state(&mut g, "steps"), format!("{:?}", crate::lua::Value::Int(2)), "two presses, however long held");
    assert_eq!(state(&mut g, "ups"), format!("{:?}", crate::lua::Value::Int(2)));
}

/// A replay recorded before an action came carries no key for it: played
/// back, the game's texts show it unbound (as where nobody plays) and the
/// game draws on (Voxkeep's "{restart} STARTS THE ROOM AGAIN" stopped it
/// on an older replay, in every picture drawn of it).
#[test]
fn an_older_replay_shows_an_action_it_lacks_unbound() {
    use crate::replay::{Playback, Replay, replay_session};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/voxkeep");
    let replay = Replay::load(&dir.join("replays/tests/climb_to_the_door.toml")).unwrap();
    assert!(replay.controls.iter().all(|c| !c.controls.keys.contains_key("restart")), "the replay is from before the restart key");
    let mut g = replay_session::<LuaGame>(&dir, &replay).unwrap();
    let mut p = Playback::new(&replay, &mut g).unwrap();
    for _ in 0..60 {
        let input = p.next_input(&mut g);
        g.step(&input);
        let _ = g.compose(0.0, 16.0 / 9.0, Scene::Play, None);
    }
    assert_eq!(halted(&g), "");
}

/// Every key a game or a template binds to start with is one SDL knows by
/// that name (Pulsar's "LShift" was none: Shift never bombed, and only the
/// window's start said so), or one of the mouse's (`input::MOUSE_KEYS`, a
/// game with a pointer: Drums').
#[test]
fn every_games_keys_are_keys() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    for base in ["games", "templates"] {
        for entry in std::fs::read_dir(root.join(base)).unwrap().flatten() {
            let dir = entry.path();
            if !dir.join("conf.lua").is_file() {
                continue;
            }
            let cfg = crate::session::config::<LuaGame>(&dir).unwrap();
            for name in LuaGame::default_keys(&cfg).names() {
                assert!(sdl3::keyboard::Scancode::from_name(name).is_some() || crate::input::MOUSE_KEYS.contains(&name.to_lowercase().as_str()), "{}: no key named '{name}' (SDL's names: \"Left Shift\", \"Space\", \"Z\")", dir.display());
                seen += 1;
            }
        }
    }
    assert!(seen > 20, "{seen} keys looked at");
}

/// A game folder whose `conf.lua` binds `bindings` over two actions.
fn binding(name: &str, bindings: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("lua-{name}"));
    std::fs::write(dir.join("conf.lua"), format!("function conf(c)\n  c.world.size = {{ 64, 64, 16 }}\n  c.player.spawn = {{ 32, 32 }}\n  c.actions = {{ \"act\", \"prev\" }}\n{bindings}\nend\n")).unwrap();
    std::fs::write(dir.join("main.lua"), "").unwrap();
    dir
}

/// A pad's button the engine does not know is a mistake as the game
/// loads, naming the binding: the Xbox pad's letters with the place each
/// means (buttons go by where they lie), a slip with the name meant, and
/// the names there are, `controls::pad_names`, which `pad_held` reads.
#[test]
fn a_button_the_engine_does_not_know_is_a_mistake() {
    let e = start_error(&binding("pad-a", "  c.pad = { act = { \"a\" }, prev = { \"east\" } }"));
    assert!(e.starts_with("conf.lua: c.pad.act: \"a\" is no button's name: a pad's buttons go by where they lie"), "{e}");
    assert!(e.contains("so the Xbox pad's A is \"south\"") && e.contains("a Nintendo pad has its letters the other way round"), "{e}");
    let names: Vec<&str> = crate::controls::pad_names().collect();
    assert!(e.ends_with(&format!("the names: {}", names.join(", "))), "{e}");
    for (letter, at) in [("b", "east"), ("x", "west"), ("y", "north"), ("A", "south")] {
        let e = start_error(&binding("pad-letter", &format!("  c.pad.act = {{ \"{letter}\" }}")));
        assert!(e.contains(&format!("so the Xbox pad's {} is \"{at}\"", letter.to_uppercase())), "{e}");
    }
    let e = start_error(&binding("pad-slip", "  c.pad.prev = { \"south\", \"leftshoulde\" }"));
    assert!(e.starts_with("conf.lua: c.pad.prev: \"leftshoulde\" is no button's name (did you mean \"leftshoulder\"?)"), "{e}");
    // Every name there is loads, the triggers too.
    let all = names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
    assert_eq!(start_error(&binding("pad-all", &format!("  c.pad.act = {{ {all} }}"))), "");
    assert_eq!(names.len(), 16);
}

/// `voxcade api c.pad` says every name a pad's button has, in the order
/// a mistake lists them (`controls::pad_names`), and `c.keys` the mouse's.
#[test]
fn the_api_says_the_names_of_the_buttons_and_the_mouse() {
    let keys = crate::lua::api::conf_keys("GameConfig");
    let doc = |key: &str| keys.iter().find(|k| k.key == key).map(|k| k.doc.clone()).unwrap();
    let names: Vec<&str> = crate::controls::pad_names().collect();
    assert!(doc("pad").contains(&format!("The names: {}.", names.join(", "))), "{}", doc("pad"));
    assert!(doc("keys").contains(&crate::input::MOUSE_KEYS.join(", ")), "{}", doc("keys"));
}

/// A key the engine does not know is a mistake as the game loads, naming
/// the binding and the key meant (SDL's name, any case); the mouse's keys
/// with a pointer only.
#[test]
fn a_key_the_engine_does_not_know_is_a_mistake() {
    let e = start_error(&binding("key-slip", "  c.keys = { act = { \"Space\", \"Retrun\" } }"));
    assert!(e.starts_with("conf.lua: c.keys.act: \"Retrun\" is no key's name (did you mean \"Return\"?): keys go by their names on a US keyboard"), "{e}");
    assert!(!e.contains("mouse_left"), "no pointer, no mouse: {e}");
    let e = start_error(&binding("key-escape", "  c.keys.prev = { \"escpe\" }"));
    assert!(e.contains("\"escpe\" is no key's name (did you mean \"Escape\"?)"), "{e}");
    let e = start_error(&binding("key-mouse", "  c.pointer = \"own\"\n  c.keys.act = { \"mouse_lft\" }"));
    assert!(e.contains("(did you mean \"mouse_left\"?)") && e.contains("or the mouse's: mouse_left, mouse_right"), "{e}");
    assert_eq!(start_error(&binding("key-fine", "  c.pointer = \"own\"\n  c.keys = { act = { \"left shift\", \"Return\", \"mouse_left\" }, prev = { \"Z\" } }")), "");
}

/// A binding of an action the game lacks (`c.actions`, a kit's, `pause`)
/// did nothing: it is a mistake as the game loads, naming the action meant.
#[test]
fn an_action_the_game_lacks_is_a_mistake() {
    let e = start_error(&binding("action-slip", "  c.keys.prv = { \"Q\" }"));
    assert_eq!(e, "conf.lua: c.keys.prv: the game has no action prv (did you mean prev?); its actions: act, prev, pause (c.actions names its own)");
    let e = start_error(&binding("action-pad", "  c.pad.jump = { \"south\" }"));
    assert!(e.starts_with("conf.lua: c.pad.jump: the game has no action jump"), "{e}");
    assert_eq!(start_error(&binding("action-pause", "  c.keys.pause = { \"Q\" }\n  c.pad.pause = { \"back\" }")), "");
}

/// A direction in `c.keys` or `c.pad` stating the keys and buttons that
/// walk it anyway (its arrow and WASD letter, its d-pad button) loads and
/// says nothing; another key or button there would do nothing, a mistake
/// saying which walk, that conf.lua cannot give the directions others and
/// what to do instead.
#[test]
fn a_direction_may_state_what_walks_it_and_nothing_else() {
    let stated = "  c.keys = { up = { \"W\", \"Up\" }, down = { \"S\", \"Down\" }, left = { \"A\", \"Left\" }, right = { \"D\", \"Right\" }, act = { \"Space\" } }\n  c.pad = { up = { \"dpup\" }, down = { \"dpdown\" }, left = { \"dpleft\" }, right = { \"dpright\" } }";
    assert_eq!(start_error(&binding("way-stated", stated)), "");
    let e = start_error(&binding("way-other", "  c.keys.up = { \"W\", \"I\" }"));
    assert_eq!(e, "conf.lua: c.keys.up: \"I\" does not walk up: on the keyboard \"Up\" and \"W\" walk up, and a game's conf.lua cannot give the four directions other keys or buttons (stating those is fine); to have \"I\" do something, bind it to an action of your own (c.actions). In a game of several a player sharing the keyboard may change the keys of their half's directions (CONTROLS, KEYS)");
    let e = start_error(&binding("way-crossed", "  c.keys.down = { \"Up\" }"));
    assert!(e.starts_with("conf.lua: c.keys.down: \"Up\" does not walk down: on the keyboard \"Down\" and \"S\" walk down"), "{e}");
    let e = start_error(&binding("way-pad", "  c.pad.left = { \"dpright\" }"));
    assert!(e.starts_with("conf.lua: c.pad.left: \"dpright\" does not walk left: on a pad the d-pad (\"dpleft\") and a stick walk left") && !e.contains("CONTROLS"), "{e}");
    let e = start_error(&binding("way-slip", "  c.pad.up = { \"dpupp\" }"));
    assert!(e.contains("\"dpupp\" is no button's name (did you mean \"dpup\"?)"), "{e}");
}

/// A player's settings with names no longer there (an action the game
/// dropped, a key and a button no names) start the game as before: only
/// the game's own `conf.lua` is checked, and the stale names give way
/// quietly to the game's own (`Settings::adopt`).
#[test]
fn a_stale_name_in_the_settings_starts_the_game() {
    let dir = binding("stale-settings", "  c.keys.act = { \"Space\" }\n  c.pad.act = { \"south\" }");
    let file = dir.join("settings.toml");
    std::fs::write(&file, "[keys]\nact = [\"Retrun\"]\nghost = [\"G\"]\n\n[pad]\nact = [\"a\"]\nghost = [\"south\"]\n").unwrap();
    let (mut settings, trouble) = crate::settings::Settings::load_for(&file, false);
    assert_eq!(trouble, None);
    let mut g = game_in(&dir);
    settings.adopt(g.actions(), &g.default_keys(), &g.default_pad(), g.has_pointer());
    assert_eq!((settings.keys.get("act"), settings.pad.get("act")), (&["Space".to_string()][..], &["south".to_string()][..]), "the game's own");
    assert!(settings.keys.get("ghost").is_empty() && settings.pad.get("ghost").is_empty());
    steps(&mut g, 2);
    assert_eq!(halted(&g), "");
}
